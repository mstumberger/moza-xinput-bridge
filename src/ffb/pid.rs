//! Pure HID PID (Physical Interface Device) report building for the MOZA R3.
//! No I/O here, only encoding.
//!
//! Every value below was verified against a USBPcap capture of a real game
//! driving FFB through the MOZA driver (2026-11-27,
//! `captures/ffb_20261127_183235_hub2.pcap`) and against the HID report
//! descriptor injected into that capture:
//!
//! - `0C` Device Control is a **bitfield** (descriptor: Output Data,Var):
//!   0x01 enable actuators, 0x02 disable, 0x04 stop all, 0x08 reset,
//!   0x10 pause, 0x20 continue. The game opens with `0C 08`, `0C 01`, `0D FF`.
//! - Effect Type is a 1-based array index: 1 = Constant Force ... 9 = Damper.
//!   The game plays its force on block 1 (type 1) via `05 01 <i16>` and a
//!   damper on block 2 (type 9).
//! - Effect Operation: 1 = start, 2 = start solo, 3 = stop.
//! - Device Gain is a single byte.

/// Effect type values: 1-based indices into the descriptor's Effect Type
/// array (NOT the HID usage IDs 0x26..).
pub const ET_CONSTANT: u8 = 1;
pub const ET_RAMP: u8 = 2;
pub const ET_SQUARE: u8 = 3;
pub const ET_SINE: u8 = 4;
pub const ET_TRIANGLE: u8 = 5;
pub const ET_SAWTOOTH_UP: u8 = 6;
pub const ET_SAWTOOTH_DOWN: u8 = 7;
pub const ET_SPRING: u8 = 8;
pub const ET_DAMPER: u8 = 9;
pub const ET_INERTIA: u8 = 10;
pub const ET_FRICTION: u8 = 11;

/// Device Control bits.
const DC_ENABLE_ACTUATORS: u8 = 0x01;
const DC_DISABLE_ACTUATORS: u8 = 0x02;
const DC_STOP_ALL_EFFECTS: u8 = 0x04;
const DC_RESET: u8 = 0x08;

/// Effect Operation values; the game starts with loop count 1, stops with 0.
const OP_START: u8 = 0x01;
const OP_STOP: u8 = 0x03;

/// Direction the game sends for its constant force: 9000 = 90.00 degrees
/// (polar "east", the wheel's X axis).
pub const DIRECTION_X_AXIS: u16 = 9000;

/// PID output report IDs of the MOZA R3 (from its report descriptor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PidParams {
    pub set_effect_id: u8,
    pub set_envelope_id: u8,
    pub set_condition_id: u8,
    pub set_periodic_id: u8,
    pub set_constant_id: u8,
    pub set_ramp_id: u8,
    pub effect_op_id: u8,
    pub block_free_id: u8,
    pub device_control_id: u8,
    pub gain_id: u8,
}

impl Default for PidParams {
    fn default() -> Self {
        Self {
            set_effect_id: 0x01,
            set_envelope_id: 0x02,
            set_condition_id: 0x03,
            set_periodic_id: 0x04,
            set_constant_id: 0x05,
            set_ramp_id: 0x06,
            effect_op_id: 0x0A,
            block_free_id: 0x0B,
            device_control_id: 0x0C,
            gain_id: 0x0D,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceControl {
    Reset,
    EnableActuators,
    DisableActuators,
    StopAllEffects,
}

impl DeviceControl {
    pub fn control_byte(self) -> u8 {
        match self {
            DeviceControl::Reset => DC_RESET,
            DeviceControl::StopAllEffects => DC_STOP_ALL_EFFECTS,
            DeviceControl::EnableActuators => DC_ENABLE_ACTUATORS,
            DeviceControl::DisableActuators => DC_DISABLE_ACTUATORS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectOp {
    Start,
    Stop,
}

/// Description of a PID effect for the Set Effect report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectSpec {
    /// Effect type index, e.g. [`ET_CONSTANT`].
    pub effect_type: u8,
    /// Duration in ms; `0xFFFF` = infinite (MOZA quirk: never `0x0000`).
    pub duration: u16,
    /// Effect gain, 0..=0xFF.
    pub gain: u8,
    /// Polar direction in hundredths of a degree.
    pub direction: u16,
}

impl EffectSpec {
    /// The constant-force effect exactly as the game creates it.
    pub const fn constant() -> Self {
        Self {
            effect_type: ET_CONSTANT,
            duration: crate::ffb::quirks::QUIRK_INFINITE_DURATION,
            gain: 0xFF,
            direction: DIRECTION_X_AXIS,
        }
    }
}

pub fn device_control_report(p: &PidParams, control: DeviceControl) -> Vec<u8> {
    vec![p.device_control_id, control.control_byte()]
}

/// Device Gain report (single byte, 0xFF = 100%).
pub fn device_gain_report(p: &PidParams, gain: u8) -> Vec<u8> {
    vec![p.gain_id, gain]
}

/// Zero durations are stored as `0xFFFF` (MOZA quirk: never `0x0000`).
fn encode_duration(duration: u16) -> u16 {
    if duration == 0 {
        crate::ffb::quirks::QUIRK_INFINITE_DURATION
    } else {
        duration
    }
}

/// Set Effect report (22 bytes), layout as captured from the game:
/// `01 <block> <type> <duration u16> <trigger repeat u16> <sample period u16>
/// <start delay u16> <gain u8> <trigger button u8 = FF none> <axes/direction
/// enable = 04> <direction u16> 00*6`.
pub fn set_effect_report(p: &PidParams, index: u8, e: &EffectSpec) -> Vec<u8> {
    let dur = encode_duration(e.duration).to_le_bytes();
    let dir = e.direction.to_le_bytes();
    let mut r = vec![
        p.set_effect_id,
        index,
        e.effect_type,
        dur[0],
        dur[1],
        0x00,
        0x00, // trigger repeat interval
        0x00,
        0x00, // sample period: default
        0x00,
        0x00, // start delay
        e.gain,
        0xFF, // trigger button: none
        0x04, // axes/direction enable flags (as sent by the game)
        dir[0],
        dir[1],
    ];
    r.resize(22, 0x00);
    r
}

/// Set Constant Force report: effect block index + signed magnitude (LE).
pub fn set_constant_report(p: &PidParams, index: u8, magnitude: i16) -> Vec<u8> {
    let m = magnitude.to_le_bytes();
    vec![p.set_constant_id, index, m[0], m[1]]
}

/// Effect Operation report: `[report_id, block_index, op, loop_count]`.
pub fn effect_operation_report(p: &PidParams, index: u8, op: EffectOp) -> Vec<u8> {
    let (op_byte, loop_count) = match op {
        EffectOp::Start => (OP_START, 1),
        EffectOp::Stop => (OP_STOP, 0),
    };
    vec![p.effect_op_id, index, op_byte, loop_count]
}

/// PID Block Free report.
pub fn block_free_report(p: &PidParams, index: u8) -> Vec<u8> {
    vec![p.block_free_id, index]
}

/// Magnitude from ViGEm X360 motors driven by the large motor only; the
/// small motor is logged at debug level by the controller (no overlay).
pub fn magnitude_from_motors(large: u8, small: u8, gain: f32) -> i16 {
    let _ = small;
    let m = (large as f32 / 255.0 * gain).clamp(-1.0, 1.0) * 32767.0;
    m as i16
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

    #[test]
    fn device_control_bits_match_capture() {
        let p = PidParams::default();
        assert_eq!(device_control_report(&p, DeviceControl::Reset), hex("0c08"));
        assert_eq!(device_control_report(&p, DeviceControl::EnableActuators), hex("0c01"));
        assert_eq!(device_control_report(&p, DeviceControl::StopAllEffects), hex("0c04"));
        assert_eq!(device_control_report(&p, DeviceControl::DisableActuators), hex("0c02"));
        assert_eq!(device_gain_report(&p, 0xFF), hex("0dff"));
    }

    #[test]
    fn set_effect_matches_captured_game_bytes() {
        let p = PidParams::default();
        assert_eq!(
            set_effect_report(&p, 1, &EffectSpec::constant()),
            hex("010101ffff000000000000ffff042823000000000000")
        );
        let damper = EffectSpec { effect_type: ET_DAMPER, ..EffectSpec::constant() };
        assert_eq!(
            set_effect_report(&p, 2, &damper),
            hex("010209ffff000000000000ffff042823000000000000")
        );
    }

    #[test]
    fn zero_duration_is_never_zero_quirk() {
        let p = PidParams::default();
        let e = EffectSpec { duration: 0, ..EffectSpec::constant() };
        assert_eq!(&set_effect_report(&p, 1, &e)[3..5], &[0xFF, 0xFF]);
    }

    #[test]
    fn set_constant_little_endian() {
        let p = PidParams::default();
        assert_eq!(set_constant_report(&p, 1, 0x7FFF), vec![0x05, 1, 0xFF, 0x7F]);
        assert_eq!(set_constant_report(&p, 1, -707), hex("05013dfd"));
    }

    #[test]
    fn effect_operation_matches_capture() {
        let p = PidParams::default();
        assert_eq!(effect_operation_report(&p, 1, EffectOp::Start), hex("0a010101"));
        assert_eq!(effect_operation_report(&p, 1, EffectOp::Stop), hex("0a010300"));
        assert_eq!(block_free_report(&p, 1), hex("0b01"));
    }

    #[test]
    fn magnitude_clamps_to_32767() {
        assert_eq!(magnitude_from_motors(255, 0, 2.0), 32767);
        assert_eq!(magnitude_from_motors(255, 255, 1.0), 32767);
    }

    #[test]
    fn magnitude_scales_linearly() {
        assert_eq!(magnitude_from_motors(0, 0, 1.0), 0);
        let half = magnitude_from_motors(128, 0, 1.0);
        assert!((half as i32 - 16448).abs() < 100, "got {half}");
    }
}
