//! Pure mapping application: MOZA button state -> X360 button bitmask.
//!
//! All MOZA button numbers are 1-based everywhere (index `0` in a config
//! entry means unmapped). This single path replaces the divergent 0-based
//! legacy code that caused the long-standing off-by-one mapping bug.

pub mod wizard;

use crate::config::{MappedButton, MappingConfig};
use crate::hid::state::DeviceState;
use vigem_rust::X360Button;

/// Map a canonical XBOX_ORDER name to its ViGEm X360Button bit.
///
/// ViGEm variant names (vigem-rust 0.1.1) differ from our canonical names:
/// LB/RB are `LEFT_SHOULDER`/`RIGHT_SHOULDER`, LS/RS are the thumbstick
/// clicks `LEFT_THUMB`/`RIGHT_THUMB`.
pub fn xbox_button_by_name(name: &str) -> Option<X360Button> {
    Some(match name {
        "A" => X360Button::A,
        "B" => X360Button::B,
        "X" => X360Button::X,
        "Y" => X360Button::Y,
        "DPAD_UP" => X360Button::DPAD_UP,
        "DPAD_RIGHT" => X360Button::DPAD_RIGHT,
        "DPAD_DOWN" => X360Button::DPAD_DOWN,
        "DPAD_LEFT" => X360Button::DPAD_LEFT,
        "LB" => X360Button::LEFT_SHOULDER,
        "RB" => X360Button::RIGHT_SHOULDER,
        "Guide" => X360Button::GUIDE,
        "Back" => X360Button::BACK,
        "Start" => X360Button::START,
        "LS" => X360Button::LEFT_THUMB,
        "RS" => X360Button::RIGHT_THUMB,
        _ => return None,
    })
}

/// Apply a mapping config to a decoded device state, producing the X360
/// button bitmask (ViGEm bit values ORed together).
///
/// For each mapped button with `moza_button != 0`, the xbox bit is set when
/// `state.buttons[moza_button - 1]` is pressed — 1-based, no baseline hack,
/// no inversion.
pub fn apply(state: &DeviceState, cfg: &MappingConfig) -> u16 {
    let mut bits = X360Button::empty();
    for MappedButton { xbox, moza_button } in &cfg.buttons {
        if *moza_button == 0 || *moza_button > 64 {
            continue; // unmapped or out of range: never sets a bit
        }
        if state.buttons[*moza_button as usize - 1] {
            if let Some(btn) = xbox_button_by_name(xbox) {
                bits |= btn;
            } else {
                log::warn!("mapping entry has unknown xbox name {:?}", xbox);
            }
        }
    }
    bits.bits()
}

/// The 1-based MOZA button numbers currently pressed, for GUI display.
pub fn pressed_moza_buttons(state: &DeviceState) -> Vec<u8> {
    state
        .buttons
        .iter()
        .enumerate()
        .filter(|(_, &pressed)| pressed)
        .map(|(i, _)| (i + 1) as u8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hid::state::DeviceState;
    use crate::config::MappingConfig;
    use std::collections::HashSet;

    fn state_with(buttons: &[usize]) -> DeviceState {
        let mut s = DeviceState::default();
        for &b in buttons {
            s.buttons[b - 1] = true;
        }
        s
    }

    #[test]
    fn default_r3_maps_button_1_to_a() {
        let cfg = MappingConfig::default_r3();
        let mask = apply(&state_with(&[1]), &cfg);
        assert_eq!(mask, X360Button::A.bits());
    }

    #[test]
    fn default_r3_maps_33_to_ls_34_to_rs() {
        let cfg = MappingConfig::default_r3();
        assert_eq!(apply(&state_with(&[33]), &cfg), X360Button::LEFT_THUMB.bits());
        assert_eq!(apply(&state_with(&[34]), &cfg), X360Button::RIGHT_THUMB.bits());
    }

    #[test]
    fn default_r3_maps_dpad_and_shoulders() {
        let cfg = MappingConfig::default_r3();
        assert_eq!(apply(&state_with(&[5]), &cfg), X360Button::DPAD_UP.bits());
        assert_eq!(apply(&state_with(&[6]), &cfg), X360Button::DPAD_RIGHT.bits());
        assert_eq!(apply(&state_with(&[7]), &cfg), X360Button::DPAD_DOWN.bits());
        assert_eq!(apply(&state_with(&[8]), &cfg), X360Button::DPAD_LEFT.bits());
        assert_eq!(apply(&state_with(&[17]), &cfg), X360Button::LEFT_SHOULDER.bits());
        assert_eq!(apply(&state_with(&[18]), &cfg), X360Button::RIGHT_SHOULDER.bits());
        assert_eq!(apply(&state_with(&[35]), &cfg), X360Button::BACK.bits());
        assert_eq!(apply(&state_with(&[36]), &cfg), X360Button::START.bits());
        assert_eq!(apply(&state_with(&[38]), &cfg), X360Button::GUIDE.bits());
    }

    #[test]
    fn default_r3_unmapped_buttons_set_no_bits() {
        let cfg = MappingConfig::default_r3();
        // 24 (S1) and 37 (S2) are not in the default mapping table.
        assert_eq!(apply(&state_with(&[24]), &cfg), 0);
        assert_eq!(apply(&state_with(&[37]), &cfg), 0);
        assert_eq!(apply(&state_with(&[9]), &cfg), 0);
    }

    #[test]
    fn explicit_zero_entry_never_sets_bits() {
        let mut cfg = MappingConfig::default_r3();
        cfg.buttons.iter_mut().for_each(|b| b.moza_button = 0);
        assert_eq!(apply(&state_with(&[1, 5, 33]), &cfg), 0);
    }

    #[test]
    fn button_64_boundary_and_out_of_range_are_safe() {
        let mut cfg = MappingConfig::default_r3();
        cfg.buttons[0].moza_button = 64;
        cfg.buttons[1].moza_button = 65; // out of range: ignored
        let mask = apply(&state_with(&[64]), &cfg);
        assert_eq!(mask, X360Button::A.bits(), "button 64 is valid");
        // 65 cannot exist in the state array; the out-of-range config entry
        // (65) is silently ignored by apply().
        let mask = apply(&state_with(&[2]), &cfg);
        assert_eq!(mask, 0, "out-of-range mapping entry ignored");
    }

    #[test]
    fn two_xbox_buttons_on_same_moza_button_or_together() {
        let cfg = MappingConfig {
            buttons: vec![
                crate::config::MappedButton { xbox: "A".into(), moza_button: 7 },
                crate::config::MappedButton { xbox: "B".into(), moza_button: 7 },
            ],
        };
        let mask = apply(&state_with(&[7]), &cfg);
        assert_eq!(mask, (X360Button::A | X360Button::B).bits());
    }

    #[test]
    fn all_buttons_pressed_all_bits() {
        let cfg = MappingConfig::default_r3();
        let all: Vec<usize> = (1..=64).collect();
        let mask = apply(&state_with(&all), &cfg);
        // All 15 mapped bits set (out-of-table buttons like 24 contribute 0).
        let expected = X360Button::all().bits();
        assert_eq!(mask, expected);
    }

    #[test]
    fn pressed_moza_buttons_is_one_based() {
        let s = state_with(&[1, 3, 64]);
        assert_eq!(pressed_moza_buttons(&s), vec![1, 3, 64]);
        assert_eq!(pressed_moza_buttons(&DeviceState::default()), Vec::<u8>::new());
    }

    #[test]
    fn every_xbox_order_name_resolves() {
        for name in crate::config::XBOX_ORDER {
            assert!(xbox_button_by_name(name).is_some(), "{} unresolved", name);
        }
        assert!(xbox_button_by_name("Nonsense").is_none());
    }

    #[test]
    fn distinct_buttons_have_distinct_bits() {
        let mut seen = HashSet::new();
        for name in crate::config::XBOX_ORDER {
            let b = xbox_button_by_name(name).unwrap();
            assert!(seen.insert(b.bits()), "{} collides with another button", name);
        }
    }
}
