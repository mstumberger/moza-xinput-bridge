//! Debug preview (`MOZA_GUI_PREVIEW=1`): a synthetic, time-driven device state
//! so the running view can be checked without hardware. Pure functions of
//! elapsed seconds; nothing here opens a device or touches the bridge.

use crate::hid::state::DeviceState;

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
