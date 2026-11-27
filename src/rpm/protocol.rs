//! MOZA wheelbase serial (CDC bulk OUT) frame protocol, decoded from a
//! USBPcap capture of Pit House driving the shift lights.
//!
//! Frame layout on the CDC interface's bulk OUT endpoint:
//!
//! ```text
//! 7E <len:u8> <cmd:u16 LE> <data[len]> <cksum:u8>
//! ```
//!
//! where `cksum = (sum of all preceding bytes + 0x0D) & 0xFF`.
//!
//! The shift-light frame is command `0x1841` with a fixed `FD DE 00 00`
//! prefix followed by a big-endian u16 LED bitmask (bit i = light i,
//! values up to `0x01FF` observed = 9-10 lights). Pit House streams it at
//! ~100 Hz; we send on change plus a periodic keepalive.

/// LED bitmask frame (cmd `0x1841`): `mask` bit i lights LED i. The mask
/// travels big-endian (capture: mask 0x0007 -> `00 07`, 0x01FF -> `01 ff`).
pub fn led_mask_frame(mask: u16) -> Vec<u8> {
    frame(
        0x1841,
        &[0xFD, 0xDE, 0x00, 0x00, (mask >> 8) as u8, mask as u8],
    )
}

/// Build one protocol frame for `cmd` (LE16) and `data`.
pub fn frame(cmd: u16, data: &[u8]) -> Vec<u8> {
    debug_assert!(data.len() <= 255, "data longer than the 1-byte length field");
    let mut out = Vec::with_capacity(4 + data.len() + 1);
    out.push(0x7E);
    out.push(data.len() as u8);
    out.push(cmd as u8);
    out.push((cmd >> 8) as u8);
    out.extend_from_slice(data);
    let cksum = out.iter().fold(0x0Du32, |acc, &b| acc.wrapping_add(b as u32)) as u8;
    out.push(cksum);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn led_mask_frame_matches_capture() {
        // Exact bytes from the USBPcap capture of Pit House:
        //   mask 0x0000 -> ... 00 00 c5
        //   mask 0x0007 -> ... 00 07 cc
        //   mask 0x00ff -> ... 00 ff c4
        //   mask 0x01ff -> ... 01 ff c5
        assert_eq!(
            led_mask_frame(0x0000),
            vec![0x7E, 0x06, 0x41, 0x18, 0xFD, 0xDE, 0x00, 0x00, 0x00, 0x00, 0xC5]
        );
        assert_eq!(
            led_mask_frame(0x0007),
            vec![0x7E, 0x06, 0x41, 0x18, 0xFD, 0xDE, 0x00, 0x00, 0x00, 0x07, 0xCC]
        );
        assert_eq!(
            led_mask_frame(0x00FF),
            vec![0x7E, 0x06, 0x41, 0x18, 0xFD, 0xDE, 0x00, 0x00, 0x00, 0xFF, 0xC4]
        );
        assert_eq!(
            led_mask_frame(0x01FF),
            vec![0x7E, 0x06, 0x41, 0x18, 0xFD, 0xDE, 0x00, 0x00, 0x01, 0xFF, 0xC5]
        );
    }

    #[test]
    fn checksum_formula_matches_other_captured_frames() {
        // Heartbeat-style frames of other commands from the same capture,
        // verifying the +0x0D additive checksum across frame types.
        assert_eq!(
            frame(0x132D, &[0xF5, 0x31, 0x00, 0x00, 0x00, 0x00]),
            vec![0x7E, 0x06, 0x2D, 0x13, 0xF5, 0x31, 0x00, 0x00, 0x00, 0x00, 0xF7]
        );
        assert_eq!(
            frame(0x1925, &[0x02, 0x00, 0x00]),
            vec![0x7E, 0x03, 0x25, 0x19, 0x02, 0x00, 0x00, 0xCE]
        );
    }

    #[test]
    fn empty_data_frame() {
        // Degenerate but legal: len 0.
        let f = frame(0x1234, &[]);
        assert_eq!(f, vec![0x7E, 0x00, 0x34, 0x12, 0x0D + 0x7E + 0x00 + 0x34 + 0x12]);
    }
}
