//! Debug preview (`MOZA_GUI_PREVIEW=1`): a synthetic, time-driven device state
//! so the running view can be checked without hardware. Pure functions of
//! elapsed seconds; nothing here opens a device or touches the bridge.

use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::{Arc, Mutex};

use crate::gui::ffb_test::FfbTest;
use crate::hid::state::DeviceState;

/// Preview only: an inert `FfbTest` (no thread, no device) so the OPEN state of
/// the FFB test section can be laid out without hardware. Nothing ever reads
/// its `torque_cmd` or `pulse_req`.
pub fn dummy_ffb_test() -> FfbTest {
    FfbTest {
        run: Arc::new(AtomicBool::new(false)),
        torque_cmd: Arc::new(Mutex::new(0)),
        thread: None,
        strength_pct: 30.0,
        steer: Arc::new(AtomicU32::new(u32::MAX)),
        pulse_req: Arc::new(AtomicBool::new(false)),
        pulse_strength: Arc::new(AtomicU32::new(30f32.to_bits())),
        pulse_result: Arc::new(Mutex::new(
            "OK: force works in both directions
left pulse: net +4210 counts (peak 4800)
right pulse: net -4105 counts (peak 4650)
(full lock-to-lock = 65535 counts)"
                .into(),
        )),
    }
}

/// Raw steering value (0..=65535, centre 32768) matching `state_at(t).steer`.
pub fn raw_steer_at(t: f64) -> u32 {
    (32768.0 + state_at(t).steer * 32768.0).clamp(0.0, 65535.0) as u32
}

/// Whether the environment asks for the preview. Read once at startup.
pub fn enabled() -> bool {
    std::env::var("MOZA_GUI_PREVIEW").as_deref() == Ok("1")
}

/// Wheel buttons pressed in turn, about one second each.
pub const PRESS_SEQUENCE: [usize; 5] = [1, 6, 13, 17, 36];

/// Synthetic device state at `t` seconds.
pub fn state_at(t: f64) -> DeviceState {
    let tau = std::f64::consts::TAU;
    let mut s = DeviceState {
        steer: 0.25 * (tau * t / 6.0).sin(),
        throttle: 0.5 + 0.5 * (t * 0.9).sin(),
        brake: 0.5 + 0.5 * (t * 0.6 + 2.0).sin(),
        clutch: 0.5 + 0.5 * (t * 0.4 + 4.0).sin(),
        ..DeviceState::default()
    };
    let n = PRESS_SEQUENCE[(t.max(0.0) as usize) % PRESS_SEQUENCE.len()];
    s.buttons[n - 1] = true;
    s
}

/// Synthetic LED mask (10 LEDs) that fills up over time, then restarts.
pub fn mask_at(t: f64) -> u16 {
    let lit = ((t.max(0.0) * 3.0) as u32 % 13).min(10);
    ((1u32 << lit) - 1) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_cycle_one_at_a_time() {
        for (i, want) in PRESS_SEQUENCE.iter().enumerate() {
            let s = state_at(i as f64 + 0.5);
            assert_eq!(s.buttons.iter().filter(|b| **b).count(), 1);
            assert!(s.buttons[want - 1]);
        }
    }

    #[test]
    fn values_stay_in_range_and_mask_fills() {
        for k in 0..600 {
            let s = state_at(k as f64 * 0.1);
            assert!(s.steer.abs() <= 0.25 + 1e-9);
            for p in [s.throttle, s.brake, s.clutch] {
                assert!((0.0..=1.0).contains(&p));
            }
            assert!(mask_at(k as f64 * 0.1) <= 0x3FF);
        }
        assert_eq!(mask_at(0.0), 0);
        assert_eq!(mask_at(3.6), 0x3FF);
    }
}
