//! MOZA-specific PID quirks.

/// Infinite-duration sentinel. MOZA bases reject a duration of `0x0000`
/// (interpreted as "no effect"), so zero durations must be sent as `0xFFFF`.
pub const QUIRK_INFINITE_DURATION: u16 = 0xFFFF;

/// Invert the direction bytes of a conditional-effect report (spring/damper/
/// friction/inertia), where MOZA firmware applies direction with the
/// opposite sign (the `HID_PIDFF_QUIRK_FIX_CONDITIONAL_DIRECTION` quirk
/// from hid-universal-pidff).
///
/// No-op when `is_conditional` is false or the report is too short.
pub fn fix_conditional_direction(report: &mut [u8], is_conditional: bool) {
    if !is_conditional || report.len() < 5 {
        return;
    }
    let dir = u16::from_le_bytes([report[3], report[4]]);
    let inverted = dir.wrapping_neg();
    report[3..5].copy_from_slice(&inverted.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::pid::{set_effect_report, EffectSpec, PidParams, ET_CONSTANT};

    #[test]
    fn inverts_direction_when_conditional() {
        let mut r = vec![0x03u8, 1, 0x01, 0x01, 0x00, 0, 0];
        fix_conditional_direction(&mut r, true);
        assert_eq!(u16::from_le_bytes([r[3], r[4]]), 0xFFFF); // -1
    }

    #[test]
    fn no_op_when_not_conditional() {
        let mut r = vec![0x03u8, 1, 0x01, 0x34, 0x12, 0, 0];
        let before = r.clone();
        fix_conditional_direction(&mut r, false);
        assert_eq!(r, before);
    }

    #[test]
    fn no_op_when_too_short() {
        let mut r = vec![0x03u8, 1, 2];
        fix_conditional_direction(&mut r, true);
        assert_eq!(r, vec![0x03u8, 1, 2]);
    }

    #[test]
    fn zero_duration_stored_as_infinite() {
        let p = PidParams::default();
        let e = EffectSpec { effect_type: ET_CONSTANT, duration: 0, ..EffectSpec::constant() };
        let r = set_effect_report(&p, 1, &e);
        assert_eq!(u16::from_le_bytes([r[3], r[4]]), QUIRK_INFINITE_DURATION);
    }
}
