//! Minimal HID report-descriptor parser.
//!
//! Walks short items (0/1/2/4-byte payloads, plus long-item skipping),
//! maintains the global-item stack (Usage Page, Logical Min/Max, Report
//! Size/Count/ID) and local usages, and produces a [`ReportLayout`] for the
//! input report.

/// Layout of a single analog axis field inside the input report.
///
/// `bit_offset` is relative to the start of the report *data*, i.e. after the
/// report-id byte when the device uses report ids.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AxisLayout {
    pub bit_offset: usize,
    pub bits: usize,
    pub logical_min: i32,
    pub logical_max: i32,
}

/// A contiguous run of button bits (Usage Page 0x09) in the input report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ButtonRange {
    pub start_bit: usize,
    pub count: usize,
}

/// Parsed layout of the device's input report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportLayout {
    pub report_id: Option<u8>,
    pub steer: Option<AxisLayout>,
    pub throttle: Option<AxisLayout>,
    pub brake: Option<AxisLayout>,
    pub clutch: Option<AxisLayout>,
    pub buttons: Vec<ButtonRange>,
    /// Total size of the input report data in bits (excluding report id).
    pub total_input_bits: usize,
}

// Generic Desktop usages of interest.
const GD_X: u32 = 0x30;
const GD_Y: u32 = 0x31;
const GD_Z: u32 = 0x32;
const GD_RZ: u32 = 0x35;
// Vendor-defined usages MOZA uses for pedals (per Moza2Joystick reference).
const VENDOR_THROTTLE: u32 = 0x24;
const VENDOR_BRAKE: u32 = 0x25;
const VENDOR_CLUTCH: u32 = 0x26;

const PAGE_GENERIC_DESKTOP: u16 = 0x01;
const PAGE_BUTTON: u16 = 0x09;
/// Sanity bound for any report field offset (HID reports max out at 64 KB;
/// offsets beyond this mean a malformed descriptor).
const MAX_REPORT_BITS: usize = 64 * 1024 * 8;

#[derive(Clone, Debug)]
struct Globals {
    usage_page: u16,
    logical_min: i32,
    logical_max: i32,
    report_size: usize,
    report_count: usize,
    report_id: Option<u8>,
}

impl Default for Globals {
    fn default() -> Self {
        Globals {
            usage_page: 0,
            logical_min: 0,
            logical_max: 0,
            report_size: 0,
            report_count: 0,
            report_id: None,
        }
    }
}

#[derive(Debug, Clone)]
struct AxisCandidate {
    usage_page: u16,
    usage: u32,
    layout: AxisLayout,
}

/// Parse an HID report descriptor into an input-report [`ReportLayout`].
///
/// Axis labeling heuristic: the first Generic Desktop X usage becomes
/// `steer`; Y/Z/Rz map to throttle/brake/clutch. If the pedals are reported
/// as vendor-defined usages 0x24/0x25/0x26 (MOZA's convention), those fill
/// throttle/brake/clutch instead — but only for slots not already taken by
/// the Generic Desktop usages.
pub fn parse(descriptor: &[u8]) -> Result<ReportLayout, String> {
    let mut globals = Globals::default();
    let mut global_stack: Vec<Globals> = Vec::new();
    let mut local_usages: Vec<u32> = Vec::new();
    let mut usage_min: Option<u32> = None;
    let mut usage_max: Option<u32> = None;

    let mut bit_offset = 0usize;
    let mut report_id: Option<u8> = None;
    let mut buttons: Vec<ButtonRange> = Vec::new();
    let mut axes: Vec<AxisCandidate> = Vec::new();

    let mut i = 0usize;
    while i < descriptor.len() {
        let item = descriptor[i];
        if item == 0xFE {
            // Long item: two header bytes + data length byte.
            if i + 2 >= descriptor.len() {
                return Err("truncated long item in descriptor".to_string());
            }
            let data_len = descriptor[i + 1] as usize;
            i += 3 + data_len;
            continue;
        }
        let size = match item & 0x03 {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 4,
            _ => unreachable!(),
        };
        let tag = item >> 4;
        let typ = (item >> 2) & 0x03;
        if i + 1 + size > descriptor.len() {
            return Err("truncated short item in descriptor".to_string());
        }
        let mut data: u64 = 0;
        for (n, b) in descriptor[i + 1..i + 1 + size].iter().enumerate() {
            data |= (*b as u64) << (8 * n);
        }
        let signed = match size {
            1 => data as u8 as i8 as i32,
            2 => data as u16 as i16 as i32,
            4 => data as u32 as i32,
            _ => 0,
        };
        i += 1 + size;

        match typ {
            0x01 => {
                // Global item.
                match tag {
                    0x0 => globals.usage_page = data as u16,
                    0x1 => globals.logical_min = signed,
                    0x2 => {
                        // HID convention: if the signed interpretation of
                        // Logical Max lands below Logical Min, the field was
                        // meant unsigned (e.g. 0xFFFF -> 65535, not -1).
                        globals.logical_max = if signed < globals.logical_min {
                            data as u32 as i32
                        } else {
                            signed
                        };
                    }
                    0x7 => globals.report_size = (data as usize).min(1024),
                    0x8 => {
                        globals.report_id = Some(data as u8);
                        if report_id.is_none() {
                            report_id = Some(data as u8);
                        }
                        // Offsets restart per report id; we track only the
                        // first input report.
                        bit_offset = 0;
                    }
                    0x9 => globals.report_count = (data as usize).min(64 * 1024),
                    0xA => global_stack.push(globals.clone()),
                    0xB => {
                        if let Some(g) = global_stack.pop() {
                            globals = g;
                        }
                    }
                    _ => {}
                }
            }
            0x02 => {
                // Local item.
                match tag {
                    0x0 => local_usages.push(data as u32),
                    0x1 => usage_min = Some(data as u32),
                    0x2 => usage_max = Some(data as u32),
                    _ => {}
                }
            }
            0x00 if tag == 0x8 => {
                // Input main item. Bit 0 clear = data (not constant);
                // constant items are padding and still occupy report space.
                let count = globals.report_count;
                let size_bits = globals.report_size;
                if data & 0x01 == 0 {
                    if globals.usage_page == PAGE_BUTTON {
                        buttons.push(ButtonRange {
                            start_bit: bit_offset,
                            count,
                        });
                    } else {
                        for n in 0..count {
                            let usage = if local_usages.len() == 1 {
                                local_usages[0]
                            } else if local_usages.len() > n {
                                local_usages[n]
                            } else if let (Some(min), Some(max)) = (usage_min, usage_max) {
                                min.saturating_add(n as u32).min(max)
                            } else if let Some(&last) = local_usages.last() {
                                last
                            } else {
                                continue;
                            };
                            axes.push(AxisCandidate {
                                usage_page: globals.usage_page,
                                usage,
                                layout: AxisLayout {
                                    bit_offset: bit_offset + n * size_bits,
                                    bits: size_bits,
                                    logical_min: globals.logical_min,
                                    logical_max: globals.logical_max,
                                },
                            });
                        }
                    }
                }
                // Malformed descriptors can carry huge count/size values;
                // cap the running offset instead of overflowing usize.
                bit_offset = (bit_offset + count * size_bits).min(MAX_REPORT_BITS);
                clear_locals(&mut local_usages, &mut usage_min, &mut usage_max);
            }
            0x00 if (0x9..=0xC).contains(&tag) => {
                // Other main items (Output / Feature / Collection / End
                // Collection) terminate the current local-item set.
                clear_locals(&mut local_usages, &mut usage_min, &mut usage_max);
            }
            _ => {}
        }
    }

    let mut layout = ReportLayout {
        report_id,
        total_input_bits: bit_offset,
        buttons,
        ..Default::default()
    };
    // First pass: Generic Desktop axes.
    for cand in &axes {
        if cand.usage_page != PAGE_GENERIC_DESKTOP {
            continue;
        }
        let slot = match cand.usage {
            GD_X => &mut layout.steer,
            GD_Y => &mut layout.throttle,
            GD_Z => &mut layout.brake,
            GD_RZ => &mut layout.clutch,
            _ => continue,
        };
        if slot.is_none() {
            *slot = Some(cand.layout.clone());
        }
    }
    // Second pass: vendor-defined pedal usages fill remaining slots.
    for cand in &axes {
        if cand.usage_page < 0xFF00 {
            continue;
        }
        let slot = match cand.usage {
            VENDOR_THROTTLE => &mut layout.throttle,
            VENDOR_BRAKE => &mut layout.brake,
            VENDOR_CLUTCH => &mut layout.clutch,
            _ => continue,
        };
        if slot.is_none() {
            *slot = Some(cand.layout.clone());
        }
    }

    if layout.steer.is_none()
        && layout.throttle.is_none()
        && layout.brake.is_none()
        && layout.clutch.is_none()
        && layout.buttons.is_empty()
    {
        return Err("descriptor contains no usable input items".to_string());
    }
    Ok(layout)
}

fn clear_locals(usages: &mut Vec<u32>, umin: &mut Option<u32>, umax: &mut Option<u32>) {
    usages.clear();
    *umin = None;
    *umax = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic joystick descriptor: X/Y/Z/Rz axes (16-bit, logical
    /// 0..65535) followed by a 13-button bitmask and 3 padding bits.
    fn synthetic_descriptor() -> Vec<u8> {
        let mut d = Vec::new();
        let mut item = |tag_typ_size: u8, data: &[u8]| {
            d.push(tag_typ_size);
            d.extend_from_slice(data);
        };
        item(0x05, &[0x01]); // Usage Page (Generic Desktop)
        item(0x09, &[0x04]); // Usage (Joystick)
        item(0xA1, &[0x01]); // Collection (Application)
        item(0xA1, &[0x00]); //   Collection (Physical)
        for usage in [0x30u8, 0x31, 0x32, 0x35] {
            item(0x09, &[usage]); // Usage (X / Y / Z / Rz)
            item(0x15, &[0x00]); //   Logical Min 0
            item(0x26, &[0xFF, 0xFF]); // Logical Max 65535
            item(0x75, &[0x10]); //   Report Size 16
            item(0x95, &[0x01]); //   Report Count 1
            item(0x81, &[0x02]); //   Input (Data, Var, Abs)
        }
        item(0x05, &[0x09]); // Usage Page (Button)
        item(0x19, &[0x01]); //   Usage Min 1
        item(0x29, &[0x0D]); //   Usage Max 13
        item(0x15, &[0x00]); //   Logical Min 0
        item(0x25, &[0x01]); //   Logical Max 1
        item(0x75, &[0x01]); //   Report Size 1
        item(0x95, &[0x0D]); //   Report Count 13
        item(0x81, &[0x02]); //   Input (Data, Var, Abs)
        item(0x75, &[0x01]); //   Report Size 1
        item(0x95, &[0x03]); //   Report Count 3
        item(0x81, &[0x03]); //   Input (Constant) padding
        item(0xC0, &[]); //     End Collection
        item(0xC0, &[]); // End Collection
        d
    }

    #[test]
    fn parse_synthetic_layout_offsets() {
        let layout = parse(&synthetic_descriptor()).expect("parse should succeed");
        assert_eq!(layout.report_id, None);
        let steer = layout.steer.expect("steer");
        assert_eq!((steer.bit_offset, steer.bits, steer.logical_min, steer.logical_max),
                   (0, 16, 0, 65535));
        let throttle = layout.throttle.expect("throttle");
        assert_eq!((throttle.bit_offset, throttle.bits), (16, 16));
        let brake = layout.brake.expect("brake");
        assert_eq!((brake.bit_offset, brake.bits), (32, 16));
        let clutch = layout.clutch.expect("clutch");
        assert_eq!((clutch.bit_offset, clutch.bits), (48, 16));
        assert_eq!(layout.buttons.len(), 1);
        assert_eq!(layout.buttons[0], ButtonRange { start_bit: 64, count: 13 });
        assert_eq!(layout.total_input_bits, 80);
    }

    #[test]
    fn parse_report_id_descriptor() {
        let mut d = synthetic_descriptor();
        // Insert a Report ID (5) item after the application collection opens.
        d.splice(4..4, [0x85u8, 0x05]);
        let layout = parse(&d).expect("parse should succeed");
        assert_eq!(layout.report_id, Some(5));
        assert_eq!(layout.steer.unwrap().bit_offset, 0);
    }

    #[test]
    fn vendor_pedal_usages_fill_axes() {
        let mut d = Vec::new();
        let mut item = |t: u8, data: &[u8]| {
            d.push(t);
            d.extend_from_slice(data);
        };
        item(0x05, &[0x01]); // Usage Page (Generic Desktop)
        item(0x09, &[0x04]); // Usage (Joystick)
        item(0xA1, &[0x01]); // Collection (Application)
        item(0x05, &[0x01]); //   Usage Page (Generic Desktop)
        item(0x09, &[0x30]); //   Usage (X)
        item(0x15, &[0x00]);
        item(0x26, &[0xFF, 0x7F]); // Logical Max 32767
        item(0x75, &[0x0F]); //   Report Size 15
        item(0x95, &[0x01]);
        item(0x81, &[0x02]);
        item(0x06, &[0x00, 0xFF]); // Usage Page (Vendor 0xFF00)
        for usage in [0x24u8, 0x25, 0x26] {
            item(0x09, &[usage]);
            item(0x15, &[0x00]);
            item(0x26, &[0xFF, 0xFF]);
            item(0x75, &[0x11]); // Report Size 17 (unaligned on purpose)
            item(0x95, &[0x01]);
            item(0x81, &[0x02]);
        }
        item(0xC0, &[]);
        let layout = parse(&d).expect("parse should succeed");
        assert_eq!(layout.steer.unwrap().bits, 15);
        assert_eq!(layout.throttle.unwrap().bit_offset, 15);
        assert_eq!(layout.brake.unwrap().bit_offset, 32);
        assert_eq!(layout.clutch.unwrap().bit_offset, 49);
    }

    #[test]
    fn truncated_descriptor_is_error() {
        let d = synthetic_descriptor();
        let err = parse(&d[..d.len() - 3]).unwrap_err();
        assert!(err.contains("truncated"));
    }
}
