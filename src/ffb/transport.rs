//! Windows HID transport that hidapi does not expose.
//!
//! hidapi's `write()` uses the interrupt-OUT pipe (`WriteFile`), which fails
//! with ERROR_INVALID_PARAMETER when a device declares no OUT endpoint. Many
//! HID PID force-feedback devices (the MOZA R3 included, as verified on live
//! hardware) receive output reports via SET_REPORT **control transfers**
//! (`HidD_SetOutputReport`) instead. This module opens the device directly
//! with the Win32 API and provides:
//!
//! - `set_output_report` — interrupt-OUT write (as the MOZA driver does),
//!   with SET_REPORT(output) control transfer as fallback,
//! - `report_descriptor` — the raw HID report descriptor via
//!   IOCTL_HID_GET_REPORT_DESCRIPTOR (hidapi cannot read it on Windows),
//! - `caps` — `HIDP_CAPS` report lengths.

#![cfg(windows)]

use std::ffi::CStr;

use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HidD_FreePreparsedData, HidD_GetPreparsedData, HidD_SetOutputReport, HidP_GetCaps,
    HIDP_CAPS,
};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, WriteFile, OPEN_EXISTING};
use windows_sys::Win32::System::IO::DeviceIoControl;

/// CTL_CODE(FILE_DEVICE_KEYBOARD (0x0B), 0x66, METHOD_OUT_DIRECT (2),
/// FILE_ANY_ACCESS)
const IOCTL_HID_GET_REPORT_DESCRIPTOR: u32 = 0x000B_019A;

pub struct WinHid {
    handle: HANDLE,
    pub output_report_len: usize,
    pub feature_report_len: usize,
    /// Set once WriteFile (interrupt OUT) has failed; use SET_REPORT after.
    use_control: std::sync::atomic::AtomicBool,
}

// The handle is used from immutable `&self` methods only, serialized by the
// caller (FfbController's WriteFn is invoked from one drain thread).
unsafe impl Send for WinHid {}
unsafe impl Sync for WinHid {}

impl WinHid {
    /// Open the HID device at `path` (a hidapi device path — a `CStr` on
    /// Windows, e.g. from `DeviceInfo::path()`).
    pub fn open(path: &CStr) -> Result<Self, String> {
        let wide: Vec<u16> = path
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_WRITE
                    | windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_READ,
                // share everything so hidapi's read handle keeps working.
                3, // FILE_SHARE_READ | FILE_SHARE_WRITE
                std::ptr::null(),
                OPEN_EXISTING,
                0, // synchronous handle: HidD_* APIs are synchronous and
                // undefined on overlapped handles (they blocked forever)
                0,
            )
        };
        if handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return Err(format!(
                "CreateFileW failed for {:?}: {}",
                path,
                std::io::Error::last_os_error()
            ));
        }

        let mut preparsed: windows_sys::Win32::Devices::HumanInterfaceDevice::PHIDP_PREPARSED_DATA = 0;
        if unsafe { HidD_GetPreparsedData(handle, &mut preparsed) } == 0 || preparsed == 0 {
            let e = std::io::Error::last_os_error();
            unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
            return Err(format!("HidD_GetPreparsedData failed: {}", e));
        }
        let mut caps = unsafe { std::mem::zeroed::<HIDP_CAPS>() };
        const HIDP_STATUS_SUCCESS: i32 = 0x0011_0000;
        let status = unsafe { HidP_GetCaps(preparsed, &mut caps) };
        unsafe { HidD_FreePreparsedData(preparsed) };
        if status != HIDP_STATUS_SUCCESS {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
            return Err(format!("HidP_GetCaps failed: {}", std::io::Error::last_os_error()));
        }

        Ok(Self {
            handle,
            output_report_len: caps.OutputReportByteLength as usize,
            feature_report_len: caps.FeatureReportByteLength as usize,
            use_control: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Send one output report the way the MOZA driver does on the wire: an
    /// interrupt-OUT transfer (`WriteFile`), verified in the 2026-11-27 game
    /// capture (every FFB report went host -> EP 0x03 interrupt, zero
    /// control transfers). Falls back to a SET_REPORT control transfer
    /// permanently if `WriteFile` is rejected.
    pub fn set_output_report(&self, report: &[u8]) -> Result<(), String> {
        use std::sync::atomic::Ordering;
        if self.output_report_len == 0 {
            return Err("device declares no output reports".into());
        }
        let mut buf = report.to_vec();
        if buf.is_empty() {
            buf.push(0);
        }
        buf.resize(self.output_report_len, 0);
        if !self.use_control.load(Ordering::Relaxed) {
            let mut written = 0u32;
            let ok = unsafe {
                WriteFile(
                    self.handle,
                    buf.as_ptr(),
                    buf.len() as u32,
                    &mut written,
                    std::ptr::null_mut(),
                )
            };
            if ok != 0 {
                return Ok(());
            }
            log::warn!(
                "HID interrupt-OUT write rejected ({}); falling back to SET_REPORT",
                std::io::Error::last_os_error()
            );
            self.use_control.store(true, Ordering::Relaxed);
        }
        if unsafe { HidD_SetOutputReport(self.handle, buf.as_ptr() as *const core::ffi::c_void, buf.len() as u32) }
            == 0
        {
            return Err(format!(
                "HidD_SetOutputReport failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    /// Read the raw HID report descriptor via IOCTL.
    pub fn report_descriptor(&self) -> Result<Vec<u8>, String> {
        let mut buf = vec![0u8; 1024];
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                self.handle,
                IOCTL_HID_GET_REPORT_DESCRIPTOR,
                std::ptr::null(),
                0,
                buf.as_mut_ptr() as *mut _,
                buf.len() as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(format!(
                "IOCTL_HID_GET_REPORT_DESCRIPTOR failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        buf.truncate(returned as usize);
        Ok(buf)
    }
}

impl Drop for WinHid {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.handle) };
    }
}
