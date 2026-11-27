//! Input report decoding into [`DeviceState`].

use crate::hid::descriptor::ReportLayout;

/// Fully decoded state of one input report.
#[derive(Clone, Debug)]
pub struct DeviceState {
    /// Steering, normalized to -1.0 .. 1.0 (0 = center).
    pub steer: f64,
    /// Pedals, unsigned, normalized to 0.0 .. 1.0 via calibration.
    pub throttle: f64,
    pub brake: f64,
    pub clutch: f64,
    /// Buttons in 1-based logical order: button N lives at index N-1.
    pub buttons: [bool; 64],
    /// Timestamp of the report in ms (set by the caller / reader loop).
    pub timestamp_ms: u64,
}

impl Default for DeviceState {
    fn default() -> Self {
        DeviceState {
            steer: 0.0,
            throttle: 0.0,
            brake: 0.0,
            clutch: 0.0,
            buttons: [false; 64],
            timestamp_ms: 0,
        }
    }
}

/// Per-pedal (min, max) raw calibration values.
pub use crate::config::Calibration;

const STEER_DEADZONE: f64 = 0.005;

/// Extract `bits` bits starting at bit `bit` (LSB-first within bytes) from
/// `data`. Returns `None` when any needed byte is missing (short report).
fn extract_bits(data: &[u8], bit: usize, bits: usize) -> Option<u64> {
    if bits == 0 || bits > 64 {
        return None;
    }
    let mut value: u64 = 0;
    for i in 0..bits {
        let idx = bit + i;
        let byte = idx / 8;
        if byte >= data.len() {
            return None;
        }
        if (data[byte] >> (idx % 8)) & 1 == 1 {
            value |= 1u64 << i;
        }
    }
    Some(value)
}

/// Normalize a raw pedal value into 0..1.
///
/// Verified against live hardware (2026-08-25 pedal captures): MOZA R3 pedal
/// axes idle at 0x8000; the raw value then travels AWAY from idle in a fixed
/// per-pedal direction — throttle DOWNWARD through the 0x0000 wrap (0x00E8 →
/// 0xFF44 → … observed mid-press), clutch UPWARD. The transform is the
/// wraparound-continuous travel in the pedal's direction, monotonic through
/// any wrap. (Earlier abs-deviation decode fell back from 100% to ~83% after
/// the throttle wrap — the "pedals jump around past 50%" bug — and read
/// upward pedals backwards: instant 100% then falling.)
fn pedal_value(raw: u64, cal: (u16, u16), direction: crate::config::PedalDirection) -> f64 {
    let raw = raw as u16 as u32;
    let pos = match direction {
        crate::config::PedalDirection::Down => 0x8000u32.wrapping_sub(raw) & 0xFFFF,
        crate::config::PedalDirection::Up => raw.wrapping_sub(0x8000) & 0xFFFF,
    };
    let (min, max) = (cal.0 as f64, cal.1 as f64);
    if max <= min {
        return 0.0;
    }
    ((pos as f64 - min) / (max - min)).clamp(0.0, 1.0)
}

/// Decode one raw input report into a [`DeviceState`] using the layout
/// obtained from the device's HID report descriptor.
///
/// Tolerates short reports: any axis or button bit whose bytes are missing
/// is simply left at its default value; this function never panics.
pub fn decode(report: &[u8], layout: &ReportLayout, cal: &Calibration) -> DeviceState {
    let mut state = DeviceState::default();
    // Bit offsets in the layout are relative to the report data, i.e. after
    // the report-id byte when one is present.
    let base = if layout.report_id.is_some() { 8 } else { 0 };

    if let Some(ax) = &layout.steer {
        if let Some(raw) = extract_bits(report, base + ax.bit_offset, ax.bits) {
            let (lo, hi) = (ax.logical_min as f64, ax.logical_max as f64);
            if hi > lo {
                let mut v = (2.0 * (raw as f64 - lo) / (hi - lo) - 1.0).clamp(-1.0, 1.0);
                // 0.5% deadzone with rescale so the full range is preserved.
                if v.abs() < STEER_DEADZONE {
                    v = 0.0;
                } else {
                    let sign = v.signum();
                    v = sign * (v.abs() - STEER_DEADZONE) / (1.0 - STEER_DEADZONE);
                }
                state.steer = v;
            }
        }
    }

    for (slot, ax, cal_range, direction) in [
        (&mut state.throttle, &layout.throttle, cal.throttle, cal.throttle_dir),
        (&mut state.brake, &layout.brake, cal.brake, cal.brake_dir),
        (&mut state.clutch, &layout.clutch, cal.clutch, cal.clutch_dir),
    ] {
        if let Some(ax) = ax {
            if let Some(raw) = extract_bits(report, base + ax.bit_offset, ax.bits) {
                *slot = pedal_value(raw, cal_range, direction);
            }
        }
    }

    let mut logical_index = 0usize;
    'outer: for range in &layout.buttons {
        for i in 0..range.count {
            logical_index += 1;
            if logical_index > 64 {
                break 'outer;
            }
            if extract_bits(report, base + range.start_bit + i, 1) == Some(1) {
                state.buttons[logical_index - 1] = true;
            }
        }
    }

    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hid::descriptor::parse;

    /// 10-byte report: X/Y/Z/Rz u16 LE (logical 0..65535) + 13 button bits
    /// + 3 padding bits. Descriptors and reports share this shape.
    fn layout() -> ReportLayout {
        let descriptor: Vec<u8> = {
            let mut d = Vec::new();
            let mut item = |t: u8, data: &[u8]| {
                d.push(t);
                d.extend_from_slice(data);
            };
            item(0x05, &[0x01]); // Usage Page (Generic Desktop)
            item(0x09, &[0x04]); // Usage (Joystick)
            item(0xA1, &[0x01]); // Collection (Application)
            for usage in [0x30u8, 0x31, 0x32, 0x35] {
                item(0x09, &[usage]);
                item(0x15, &[0x00]);
                item(0x26, &[0xFF, 0xFF]); // 0..65535
                item(0x75, &[0x10]);
                item(0x95, &[0x01]);
                item(0x81, &[0x02]);
            }
            item(0x05, &[0x09]); // Usage Page (Button)
            item(0x19, &[0x01]);
            item(0x29, &[0x0D]);
            item(0x15, &[0x00]);
            item(0x25, &[0x01]);
            item(0x75, &[0x01]);
            item(0x95, &[0x0D]);
            item(0x81, &[0x02]);
            item(0x95, &[0x03]);
            item(0x81, &[0x03]); // padding
            item(0xC0, &[]);
            d
        };
        parse(&descriptor).expect("descriptor parses")
    }

    fn report(steer: u16, throttle: u16, brake: u16, clutch: u16) -> Vec<u8> {
        let mut r = vec![0u8; 10];
        r[0..2].copy_from_slice(&steer.to_le_bytes());
        r[2..4].copy_from_slice(&throttle.to_le_bytes());
        r[4..6].copy_from_slice(&brake.to_le_bytes());
        r[6..8].copy_from_slice(&clutch.to_le_bytes());
        r
    }

    #[test]
    fn steer_center_min_max() {
        let (lay, cal) = (layout(), Calibration::default());
        let s = decode(&report(0x8000, 0, 0, 0), &lay, &cal);
        assert!(s.steer.abs() < 1e-9, "center -> 0, got {}", s.steer);
        let s = decode(&report(0x0000, 0, 0, 0), &lay, &cal);
        assert!((s.steer + 1.0).abs() < 1e-9, "min -> -1, got {}", s.steer);
        let s = decode(&report(0xFFFF, 0, 0, 0), &lay, &cal);
        assert!((s.steer - 1.0).abs() < 1e-3, "max -> +1, got {}", s.steer);
    }

    #[test]
    fn steer_deadzone_scales() {
        let (lay, cal) = (layout(), Calibration::default());
        // Just outside the 0.5% deadzone: small but nonzero.
        let s = decode(&report(0x8000 + 200, 0, 0, 0), &lay, &cal);
        assert!(s.steer > 0.0 && s.steer < 0.02);
        // Inside the deadzone: exactly zero.
        let s = decode(&report(0x8000 + 100, 0, 0, 0), &lay, &cal);
        assert_eq!(s.steer, 0.0);
    }

    #[test]
    fn gas_brake_travel_up_monotonic() {
        // Live-verified 2026-08-26: all R3 pedals travel UP from the 0x8000
        // idle (clutch confirmed working with Up; gas/brake showed the
        // downward-transform inversion). Decoded value must rise
        // monotonically, including through the upward 0xFFFF -> 0x0000 wrap.
        let (lay, cal) = (layout(), Calibration::default());
        let s = decode(&report(0x8000, 0x8000, 0, 0), &lay, &cal);
        assert_eq!(s.throttle, 0.0);
        let seq = [0x8000u16, 0x9000, 0xC000, 0xE000, 0xFFFF, 0x0FFF, 0x3FFF, 0x7FFF];
        let mut last = -1.0f64;
        for raw in seq {
            let s = decode(&report(0x8000, raw, 0, 0), &lay, &cal);
            assert!(
                s.throttle > last,
                "raw {raw:#06X} must decode higher (got {} after {})",
                s.throttle, last
            );
            last = s.throttle;
        }
        assert!(last > 0.999, "got {last}");
    }

    #[test]
    fn clutch_travels_upward() {
        let (lay, cal) = (layout(), Calibration::default());
        // Clutch (report() writes axes Y/Z/Rz — clutch is Rz, 4th axis).
        // Pressing clutch moves raw UP from 0x8000; decoded value must rise
        // monotonically (regression for the "jumps to 100 then falls" bug).
        let seq = [0x8000u16, 0x9000, 0xA000, 0xC000, 0xE000, 0xFFFF];
        let mut last = -1.0f64;
        for raw in seq {
            let s = decode(&report(0x8000, 0x8000, 0, raw), &lay, &cal);
            assert!(
                s.clutch > last,
                "clutch raw {raw:#06X} must decode higher (got {} after {})",
                s.clutch,
                last
            );
            last = s.clutch;
        }
    }

    #[test]
    fn pedal_calibration_narrows_and_clamps() {
        let lay = layout();
        let cal = Calibration { throttle: (10000, 20000), ..Default::default() };
        // Position 15000 (between 10000 and 20000) -> ~0.5 (Up direction).
        let raw = 0x8000u16.wrapping_add(15000);
        let s = decode(&report(0x8000, raw, 0, 0), &lay, &cal);
        assert!((s.throttle - 0.5).abs() < 0.01, "got {}", s.throttle);
        // Beyond the calibrated max must clamp to 1.0, never wrap.
        let raw = 0x8000u16.wrapping_add(30000);
        let s = decode(&report(0x8000, raw, 0, 0), &lay, &cal);
        assert_eq!(s.throttle, 1.0);
        // Below min clamps to 0.0.
        let s = decode(&report(0x8000, 0x8000, 0, 0), &lay, &cal);
        assert_eq!(s.throttle, 0.0);
    }

    #[test]
    fn button_bit_independent_of_steering_bytes() {
        let (lay, cal) = (layout(), Calibration::default());
        // Steering at extreme value AND button 7 pressed (bit 6 of byte 8).
        let mut r = report(0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF);
        r[8] |= 1 << 6;
        let s = decode(&r, &lay, &cal);
        assert_eq!(s.steer, 1.0);
        for (i, &pressed) in s.buttons.iter().enumerate() {
            let expected = i == 6; // button 7 -> index 6 (1-based order)
            assert_eq!(pressed, expected, "button {} wrong", i + 1);
        }
    }

    #[test]
    fn short_report_defaults_no_panic() {
        let (lay, cal) = (layout(), Calibration::default());
        let s = decode(&[0x00, 0x80], &lay, &cal);
        assert_eq!(s.steer, 0.0);
        assert_eq!(s.throttle, 0.0);
        assert_eq!(s.brake, 0.0);
        assert_eq!(s.clutch, 0.0);
        assert!(s.buttons.iter().all(|&b| !b));
        // Totally empty report is fine too.
        let _ = decode(&[], &lay, &cal);
    }

    #[test]
    fn report_id_byte_is_skipped() {
        let mut d: Vec<u8> = Vec::new();
        let mut item = |t: u8, data: &[u8]| {
            d.push(t);
            d.extend_from_slice(data);
        };
        item(0x05, &[0x01]);
        item(0x09, &[0x30]); // X only
        item(0x15, &[0x00]);
        item(0x26, &[0xFF, 0xFF]);
        item(0x75, &[0x10]);
        item(0x95, &[0x01]);
        item(0x85, &[0x05]); // Report ID 5
        item(0x81, &[0x02]);
        let lay = parse(&d).unwrap();
        assert_eq!(lay.report_id, Some(5));
        let mut r = vec![0u8; 3];
        r[0] = 5; // report id byte
        r[1] = 0x00;
        r[2] = 0x80; // steer raw 0x8000 -> center
        let s = decode(&r, &lay, &Calibration::default());
        assert_eq!(s.steer, 0.0);
    }

    #[test]
    fn fallback_layout_decodes_legacy_offsets() {
        use crate::hid::device::fallback_layout;
        let lay = fallback_layout();
        let cal = Calibration::default();
        // Center steering: bytes 1-2 = 0x00 0x80.
        let mut r = vec![0u8; 33];
        r[1] = 0x00;
        r[2] = 0x80;
        let s = decode(&r, &lay, &cal);
        assert!(s.steer.abs() < 1e-9);
        // Throttle (Up direction): full press = raw just above center on
        // the wrap's far side (0x7FFF -> position 0xFFFE ≈ 100%).
        r[5] = 0xFF;
        r[6] = 0x7F;
        let s = decode(&r, &lay, &cal);
        assert!(s.throttle > 0.999, "got {}", s.throttle);
        // Button 1 sits at bit 4 of byte 17.
        r[17] = 1 << 4;
        let s = decode(&r, &lay, &cal);
        assert!(s.buttons[0], "button 1 (index 0) should be set");
        assert!(s.buttons[1..].iter().all(|&b| !b));
    }
}
