//! MOZA CDC serial port discovery and writing.
//!
//! The shift-light frames travel over the wheelbase's CDC-Data interface
//! (bulk OUT endpoint 0x02 in the USBPcap capture), which Windows exposes
//! as a COM port — not through HID, hence the separate transport from the
//! FFB module.

use std::time::Duration;

use anyhow::{anyhow, Result};

use crate::hid::{MOZA_PIDS, MOZA_VID};

/// Find and open the MOZA wheelbase's CDC serial port.
pub fn open_moza_serial() -> Result<Box<dyn serialport::SerialPort>> {
    let name = serialport::available_ports()?
        .into_iter()
        .find_map(|p| match &p.port_type {
            serialport::SerialPortType::UsbPort(info)
                if info.vid == MOZA_VID && MOZA_PIDS.contains(&info.pid) =>
            {
                Some(p.port_name.clone())
            }
            _ => None,
        })
        .ok_or_else(|| {
            anyhow!("no MOZA serial port found (is the wheelbase connected?)")
        })?;
    log::info!("rpm lights: opening serial port {}", name);
    // Baud is meaningless for CDC-ACM but the API requires one.
    serialport::new(&name, 115_200)
        .timeout(Duration::from_millis(50))
        .open()
        .map_err(|e| anyhow!("failed to open {}: {}", name, e))
}

/// Type alias used by the writer thread in `mod.rs`.
pub type SerialWriter = Box<dyn serialport::SerialPort>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_device_is_an_error_not_a_panic() {
        // On a machine without the wheelbase attached this must return Err,
        // never panic. (On one with it attached, it opens — also fine.)
        let _ = open_moza_serial();
    }
}
