//! Direct torque channel for the MOZA R3, speaking the exact PID sequence a
//! real game sends through the MOZA driver. Decoded 2026-11-27 from a
//! plug-in -> Pit House -> in-game USBPcap capture
//! (`captures/ffb_20261127_183235_hub2.pcap`, markers in the same folder):
//!
//! - init: `0C 08` (reset), `0C 01` (enable actuators), `0D FF` (gain 100%)
//! - create: Set Constant `05 01 0000`, Set Effect block 1 type 1 (constant,
//!   infinite, direction 90.00 deg), Effect Operation `0A 01 01 01` (start)
//! - stream: `05 01 <i16 LE>`, followed by the same Set Effect refresh the
//!   game sends with every update
//! - stop: `05 01 0000`, `0A 01 03 00`, `0B 01` (block free)
//!
//! Earlier attempts failed because `0C 04` was taken for "enable" (it is
//! Stop All Effects), the effect was created as type 09 (Damper) and the
//! constant-force magnitude was written to that damper block.
//!
//! Sign (verified on hardware): **positive torque turns the wheel left**,
//! i.e. drives the steering raw value down; negative turns it right.

use std::sync::Arc;

use super::pid::{
    block_free_report, device_control_report, device_gain_report, effect_operation_report,
    set_constant_report, set_effect_report, DeviceControl, EffectOp, EffectSpec, PidParams,
};
use super::transport::WinHid;

/// Effect block the game uses for its constant force.
pub const CONSTANT_BLOCK: u8 = 1;

/// The init + create + start sequence, as raw reports (pure; unit-tested).
pub fn open_sequence(p: &PidParams) -> Vec<Vec<u8>> {
    vec![
        device_control_report(p, DeviceControl::Reset),
        device_control_report(p, DeviceControl::EnableActuators),
        device_gain_report(p, 0xFF),
        set_constant_report(p, CONSTANT_BLOCK, 0),
        set_effect_report(p, CONSTANT_BLOCK, &EffectSpec::constant()),
        effect_operation_report(p, CONSTANT_BLOCK, EffectOp::Start),
    ]
}

/// One torque update, as raw reports (pure; unit-tested).
pub fn torque_sequence(p: &PidParams, torque: i16) -> Vec<Vec<u8>> {
    vec![
        set_constant_report(p, CONSTANT_BLOCK, torque),
        set_effect_report(p, CONSTANT_BLOCK, &EffectSpec::constant()),
    ]
}

/// Release sequence, as raw reports (pure; unit-tested).
pub fn stop_sequence(p: &PidParams) -> Vec<Vec<u8>> {
    vec![
        set_constant_report(p, CONSTANT_BLOCK, 0),
        effect_operation_report(p, CONSTANT_BLOCK, EffectOp::Stop),
        block_free_report(p, CONSTANT_BLOCK),
    ]
}

pub struct TorqueChannel {
    win: Arc<WinHid>,
    params: PidParams,
}

impl TorqueChannel {
    /// Open the wheelbase, reset/enable it and start the constant-force
    /// effect at zero torque.
    pub fn open(path: &std::ffi::CStr) -> Result<Self, String> {
        let win = WinHid::open(path)?;
        if win.output_report_len == 0 {
            return Err("device declares no output reports".into());
        }
        let ch = Self { win: Arc::new(win), params: PidParams::default() };
        for r in open_sequence(&ch.params) {
            ch.win.set_output_report(&r)?;
        }
        log::info!("FFB torque channel open (game protocol, block {CONSTANT_BLOCK})");
        Ok(ch)
    }

    /// Send one torque command (signed, full scale +-32767). Keep sending
    /// while force should hold; send 0 to release.
    pub fn set_torque(&self, torque: i16) -> Result<(), String> {
        for r in torque_sequence(&self.params, torque) {
            self.win.set_output_report(&r)?;
        }
        Ok(())
    }

    /// Stop the effect and free its block.
    pub fn stop(&self) {
        for r in stop_sequence(&self.params) {
            let _ = self.win.set_output_report(&r);
        }
    }
}

impl Drop for TorqueChannel {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Byte-for-byte the game's sequence at t=382.985 s in the capture.
    #[test]
    fn open_sequence_matches_game_capture() {
        let want: Vec<Vec<u8>> = [
            "0c08",
            "0c01",
            "0dff",
            "05010000",
            "010101ffff000000000000ffff042823000000000000",
            "0a010101",
        ]
        .iter()
        .map(|s| hex(s))
        .collect();
        assert_eq!(open_sequence(&PidParams::default()), want);
    }

    #[test]
    fn torque_and_stop_match_game_capture() {
        let p = PidParams::default();
        assert_eq!(
            torque_sequence(&p, -707),
            vec![hex("05013dfd"), hex("010101ffff000000000000ffff042823000000000000")]
        );
        assert_eq!(stop_sequence(&p), vec![hex("05010000"), hex("0a010300"), hex("0b01")]);
    }
}
