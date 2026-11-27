//! HID PID force feedback controller for the MOZA R3. Maintains one
//! persistent Constant Force effect (block 1, exactly as a real game creates
//! it through the MOZA driver; see `vendor.rs` for the captured sequence)
//! whose magnitude is updated from ViGEm X360 motor values.

pub mod pid;
pub mod quirks;
#[cfg(windows)]
pub mod transport;
#[cfg(windows)]
pub mod vendor;

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pid::{
    block_free_report, device_control_report, device_gain_report, effect_operation_report,
    magnitude_from_motors, set_constant_report, set_effect_report, DeviceControl, EffectOp,
    EffectSpec, PidParams,
};

/// FFB is disabled after this many consecutive write errors.
const MAX_CONSECUTIVE_ERRORS: usize = 3;
/// Effect block index of our single persistent effect (the game's block).
const EFFECT_INDEX: u8 = 1;

pub type WriteFn = Arc<dyn Fn(&[u8]) -> Result<usize, String> + Send + Sync>;

fn f32_to_bits(f: f32) -> u32 {
    f.to_bits()
}

fn bits_to_f32(b: u32) -> f32 {
    f32::from_bits(b)
}

/// Pad a PID report (which already starts with its report ID) to the
/// device's output report length — Windows HID requires the exact length.
fn frame(report: &[u8], output_report_len: usize) -> Vec<u8> {
    let mut buf = Vec::with_capacity(output_report_len.max(report.len()));
    buf.extend_from_slice(report);
    buf.resize(output_report_len.max(report.len()), 0x00);
    buf
}

pub struct FfbController {
    writer: WriteFn,
    params: PidParams,
    gain: AtomicU32, // f32 bit pattern
    enabled: AtomicBool,
    consecutive_errors: AtomicUsize,
    output_report_len: usize,
}

impl FfbController {
    /// Reset, enable actuators and set full device gain: `0C 08`, `0C 01`,
    /// `0D FF`, the game's opening sequence.
    pub fn new(writer: WriteFn, gain: f32, output_report_len: usize) -> Result<Self, String> {
        let c = Self {
            writer,
            params: PidParams::default(),
            gain: AtomicU32::new(f32_to_bits(gain)),
            enabled: AtomicBool::new(false),
            consecutive_errors: AtomicUsize::new(0),
            output_report_len,
        };
        c.write(&device_control_report(&c.params, DeviceControl::Reset))
            .map_err(|e| format!("FFB reset failed: {e}"))?;
        c.write(&device_control_report(&c.params, DeviceControl::EnableActuators))
            .map_err(|e| format!("FFB enable actuators failed: {e}"))?;
        c.write(&device_gain_report(&c.params, 0xFF))
            .map_err(|e| format!("FFB device gain failed: {e}"))?;
        c.enabled.store(true, Ordering::SeqCst);
        log::info!("FFB initialized: reset + actuators enabled + gain 100%");
        Ok(c)
    }

    /// Create and start the persistent constant-force effect at magnitude 0.
    pub fn start_effect(&self) {
        let steps = [
            set_constant_report(&self.params, EFFECT_INDEX, 0),
            set_effect_report(&self.params, EFFECT_INDEX, &EffectSpec::constant()),
            effect_operation_report(&self.params, EFFECT_INDEX, EffectOp::Start),
        ];
        for r in steps {
            if let Err(e) = self.write(&r) {
                log::error!("FFB effect create/start failed: {e}");
                return;
            }
        }
        log::debug!("FFB constant effect started (block {EFFECT_INDEX})");
    }

    /// Called from the ViGEm X360 motor callback with large/small magnitudes.
    pub fn on_motors(&self, large: u8, small: u8) {
        if !self.enabled() {
            return;
        }
        log::debug!("ffb motors: large={large} small={small}");
        let gain = bits_to_f32(self.gain.load(Ordering::Relaxed));
        let mag = magnitude_from_motors(large, small, gain);
        // The game refreshes Set Effect after every magnitude update.
        let result = self
            .write(&set_constant_report(&self.params, EFFECT_INDEX, mag))
            .and_then(|_| {
                self.write(&set_effect_report(&self.params, EFFECT_INDEX, &EffectSpec::constant()))
            });
        if let Err(e) = result {
            let n = self.consecutive_errors.fetch_add(1, Ordering::SeqCst) + 1;
            log::error!("FFB write failed ({n}/{MAX_CONSECUTIVE_ERRORS}): {e}");
            if n >= MAX_CONSECUTIVE_ERRORS {
                self.enabled.store(false, Ordering::SeqCst);
                log::error!("FFB disabled after {n} consecutive write errors");
            }
            return;
        }
        self.consecutive_errors.store(0, Ordering::SeqCst);
    }

    /// Zero the force, stop the effect and free its block (the game's exit
    /// sequence; actuators stay enabled, as the game leaves them).
    pub fn stop(&self) {
        let _ = self.write(&set_constant_report(&self.params, EFFECT_INDEX, 0));
        let _ = self.write(&effect_operation_report(&self.params, EFFECT_INDEX, EffectOp::Stop));
        let _ = self.write(&block_free_report(&self.params, EFFECT_INDEX));
        self.enabled.store(false, Ordering::SeqCst);
        log::info!("FFB stopped, effect freed");
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    pub fn set_gain(&self, g: f32) {
        self.gain.store(f32_to_bits(g), Ordering::Relaxed);
    }

    fn write(&self, report: &[u8]) -> Result<usize, String> {
        let buf = frame(report, self.output_report_len);
        (self.writer)(&buf)
    }

    /// Preferred constructor on Windows: opens the device directly via the
    /// Win32 API (`transport::WinHid`: interrupt-OUT writes padded to the
    /// exact `OutputReportByteLength`, SET_REPORT fallback).
    #[cfg(windows)]
    pub fn from_device_path(
        path: &std::ffi::CStr,
        gain: f32,
    ) -> Result<Self, String> {
        let win = transport::WinHid::open(path)?;
        if win.output_report_len == 0 {
            return Err("device declares no output reports".into());
        }
        let len = win.output_report_len;
        let win = Arc::new(Mutex::new(win));
        let writer: WriteFn = Arc::new(move |report: &[u8]| {
            let win = win.lock().map_err(|_| "hid handle lock poisoned".to_string())?;
            win.set_output_report(report)
                .map(|_| report.len())
                .map_err(|e| e)
        });
        Self::new(writer, gain, len)
    }

    #[cfg(windows)]
    pub fn from_hid_device(
        dev: hidapi::HidDevice,
        gain: f32,
        output_report_len: usize,
    ) -> Result<Self, String> {
        let dev = Mutex::new(dev);
        let writer: WriteFn = Arc::new(move |report: &[u8]| {
            let dev = dev.lock().map_err(|_| "hid device lock poisoned".to_string())?;
            dev.write(report)
                .map_err(|e| format!("hid write error: {e}"))
        });
        Self::new(writer, gain, output_report_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn ok_writer() -> (WriteFn, Arc<Mutex<Vec<Vec<u8>>>>) {
        let log: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
        let l = log.clone();
        (
            Arc::new(move |r: &[u8]| {
                l.lock().unwrap().push(r.to_vec());
                Ok(r.len())
            }),
            log,
        )
    }

    fn fail_writer() -> WriteFn {
        Arc::new(|_: &[u8]| Err("hid write failed".to_string()))
    }

    #[test]
    fn constructor_sends_reset_enable_gain_padded() {
        let (w, log) = ok_writer();
        let _c = FfbController::new(w, 1.0, 16).unwrap();
        let log = log.lock().unwrap();
        assert_eq!(log.len(), 3);
        for r in log.iter() {
            assert_eq!(r.len(), 16, "padded to output_report_len");
        }
        // Game capture: 0C 08 (reset), 0C 01 (enable), 0D FF (gain)
        assert_eq!(&log[0][..2], &[0x0C, 0x08]);
        assert_eq!(&log[1][..2], &[0x0C, 0x01]);
        assert_eq!(&log[2][..2], &[0x0D, 0xFF]);
    }

    #[test]
    fn constructor_fails_on_write_error() {
        assert!(FfbController::new(fail_writer(), 1.0, 16).is_err());
    }

    #[test]
    fn start_effect_matches_game_sequence() {
        let (w, log) = ok_writer();
        let c = FfbController::new(w, 1.0, 16).unwrap();
        c.start_effect();
        let log = log.lock().unwrap();
        assert_eq!(log.len(), 6); // 3 init + constant, set effect, start
        assert_eq!(&log[3][..4], &[0x05, EFFECT_INDEX, 0x00, 0x00]);
        assert_eq!(&log[4][..3], &[0x01, EFFECT_INDEX, pid::ET_CONSTANT]);
        assert_eq!([log[4][3], log[4][4]], [0xFF, 0xFF], "infinite duration");
        assert_eq!(&log[5][..4], &[0x0A, EFFECT_INDEX, 0x01, 0x01]);
    }

    /// Writer that succeeds for the first `ok_for` calls (constructor uses
    /// three), then fails forever.
    fn late_fail_writer(ok_for: usize) -> WriteFn {
        let n = Arc::new(AtomicUsize::new(0));
        Arc::new(move |_: &[u8]| {
            if n.fetch_add(1, Ordering::SeqCst) < ok_for {
                Ok(8)
            } else {
                Err("hid write failed".to_string())
            }
        })
    }

    #[test]
    fn failing_writer_disables_after_exactly_3_on_motors() {
        let c = FfbController::new(late_fail_writer(3), 1.0, 16).unwrap();
        assert!(c.enabled());
        c.on_motors(255, 0);
        assert!(c.enabled(), "1 error: still enabled");
        c.on_motors(255, 0);
        assert!(c.enabled(), "2 errors: still enabled");
        c.on_motors(255, 0);
        assert!(!c.enabled(), "3 errors: disabled");
    }

    #[test]
    fn error_counter_resets_on_success() {
        let n = Arc::new(AtomicUsize::new(0));
        let s = n.clone();
        // writes 0-2: init; each on_motors = 2 writes. Fail #5 and #9.
        let w: WriteFn = Arc::new(move |_: &[u8]| {
            let v = s.fetch_add(1, Ordering::SeqCst);
            if v == 5 || v == 9 { Err("fail".into()) } else { Ok(8) }
        });
        let c = FfbController::new(w, 1.0, 16).unwrap();
        c.on_motors(255, 0); // writes 3,4 ok
        c.on_motors(255, 0); // write 5 fails -> err 1
        c.on_motors(255, 0); // writes 6,7 ok -> reset
        c.on_motors(255, 0); // write 8 ok, 9 fails -> err 1
        c.on_motors(255, 0); // ok
        assert!(c.enabled(), "counter was reset by interleaved success");
    }

    #[test]
    fn on_motors_disabled_is_noop() {
        let (w, log) = ok_writer();
        let c = FfbController::new(w, 1.0, 16).unwrap();
        c.stop();
        assert!(!c.enabled());
        let n = log.lock().unwrap().len();
        c.on_motors(255, 0);
        assert_eq!(log.lock().unwrap().len(), n, "no write while disabled");
    }

    #[test]
    fn on_motors_magnitude_applied() {
        let (w, log) = ok_writer();
        let c = FfbController::new(w, 1.0, 16).unwrap();
        c.on_motors(255, 255);
        let log = log.lock().unwrap();
        let set_const = &log[log.len() - 2];
        // Set Constant: [0x05, block, mag_lo, mag_hi]; 32767 -> FF 7F
        assert_eq!(set_const[0], 0x05);
        assert_eq!([set_const[1], set_const[2], set_const[3]], [EFFECT_INDEX, 0xFF, 0x7F]);
        assert_eq!(log.last().unwrap()[0], 0x01, "set effect refresh follows");
    }
}
