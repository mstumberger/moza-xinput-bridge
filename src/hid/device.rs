//! MOZA device discovery and opening.

use crate::hid::descriptor::{AxisLayout, ButtonRange, ReportLayout};
use crate::hid::{MOZA_PIDS, MOZA_VID};

/// Open the MOZA wheel's joystick HID interface.
///
/// Enumerates all HID devices matching the MOZA VID/PID pair, logs every
/// candidate, and returns the first one that opens successfully. hidapi on
/// Windows does not expose the raw report descriptor through `DeviceInfo`,
/// so the practical layout is the known-good [`fallback_layout`] (see
/// `open_moza_with_layout`).
pub fn open_moza(api: &hidapi::HidApi) -> Result<hidapi::HidDevice, String> {
    open_moza_with_layout(api).map(|(device, _)| device)
}

/// Open the MOZA wheel and derive the [`ReportLayout`] used to decode its
/// input reports.
///
/// Descriptor-first when obtainable; otherwise falls back to the fixed
/// working layout reverse-engineered from the MOZA R3 (steering bytes 1-2
/// u16 LE, pedals at bytes 5-6 / 11-12 / 15-16, button bitmask from byte 17
/// bit 4 onward — the first four bits of byte 17 are status bits, not
/// buttons).
pub fn open_moza_with_layout(
    api: &hidapi::HidApi,
) -> Result<(hidapi::HidDevice, ReportLayout), String> {
    let mut last_err = "no MOZA device found".to_string();
    for info in api.device_list() {
        if info.vendor_id() != MOZA_VID || !MOZA_PIDS.contains(&info.product_id()) {
            continue;
        }
        log::info!(
            "MOZA candidate: vid=0x{:04X} pid=0x{:04X} path={:?} usage_page=0x{:04X} usage=0x{:04X} interface={}",
            info.vendor_id(),
            info.product_id(),
            info.path(),
            info.usage_page(),
            info.usage(),
            info.interface_number(),
        );
        match api.open_path(info.path()) {
            Ok(dev) => {
                // Validate the device responds by draining one report; a
                // 200 ms timeout with zero bytes is not an error (idle).
                let mut buf = [0u8; 64];
                match dev.read_timeout(&mut buf, 200) {
                    Ok(n) => log::info!("MOZA device opened, initial report {} bytes", n),
                    Err(e) => log::warn!("MOZA device opened but initial read failed: {}", e),
                }
                log::info!("using fallback report layout for MOZA input decoding");
                return Ok((dev, fallback_layout()));
            }
            Err(e) => {
                last_err = format!("failed to open {:?}: {}", info.path(), e);
                log::warn!("{}", last_err);
            }
        }
    }
    Err(last_err)
}

/// Fixed report layout replicating the known-working offsets from the
/// original MOZA R3 parsing (see `src/domain_minimal.rs` before deletion and
/// the Moza2Joystick reference):
///
/// - steering: bytes 1-2, u16 LE, logical 0..65535 (center 0x8000),
/// - throttle: bytes 5-6, brake: bytes 11-12, clutch: bytes 15-16 (u16 LE),
/// - buttons: bitmask starting at bit 4 of byte 17 (bits 0-3 of byte 17 are
///   status bits, bit 3 is always-on and was the "flickering button 1").
pub fn fallback_layout() -> ReportLayout {
    ReportLayout {
        report_id: None,
        steer: Some(AxisLayout {
            bit_offset: 8, // byte 1
            bits: 16,
            logical_min: 0,
            logical_max: 65535,
        }),
        throttle: Some(AxisLayout {
            bit_offset: 40, // byte 5
            bits: 16,
            logical_min: 0,
            logical_max: 65535,
        }),
        brake: Some(AxisLayout {
            bit_offset: 88, // byte 11
            bits: 16,
            logical_min: 0,
            logical_max: 65535,
        }),
        clutch: Some(AxisLayout {
            bit_offset: 120, // byte 15
            bits: 16,
            logical_min: 0,
            logical_max: 65535,
        }),
        buttons: vec![ButtonRange {
            start_bit: 17 * 8 + 4, // skip status bits of byte 17
            count: 38,
        }],
        total_input_bits: 32 * 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_layout_shape() {
        let lay = fallback_layout();
        assert_eq!(lay.report_id, None);
        assert_eq!(lay.steer.unwrap().bit_offset, 8);
        assert_eq!(lay.throttle.unwrap().bit_offset, 40);
        assert_eq!(lay.brake.unwrap().bit_offset, 88);
        assert_eq!(lay.clutch.unwrap().bit_offset, 120);
        assert_eq!(lay.buttons.len(), 1);
        assert_eq!(lay.buttons[0].start_bit, 140);
        assert_eq!(lay.buttons[0].count, 38);
    }
}
