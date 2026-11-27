//! Environment probe (spec 8.3): ViGEm driver, wheel on USB, Pit House.
//!
//! Runs on a background thread only while the bridge is stopped. It never
//! opens the wheel and never creates a virtual controller.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::gui::widgets::Level;

const CHECK_INTERVAL: Duration = Duration::from_secs(2);
const SLICE: Duration = Duration::from_millis(50);
const PIT_HOUSE_EXE: &str = "MOZA Pit House.exe";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProbeState {
    /// `None` = not checked yet.
    pub vigem: Option<bool>,
    pub wheel: bool,
    pub pit_house: bool,
}

/// Owner of the probe thread and its shared results.
pub struct Probe {
    state: Arc<Mutex<ProbeState>>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Probe {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(ProbeState::default())),
            stop: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.handle.as_ref().is_some_and(|h| !h.is_finished())
    }

    /// Start the thread (no-op when already running). Results are reset so
    /// nothing stale from an earlier run is shown.
    pub fn start(&mut self) {
        if self.is_running() {
            return;
        }
        // Reap a thread that ended by itself.
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        *self.state.lock().unwrap() = ProbeState::default();
        self.stop = Arc::new(AtomicBool::new(false));
        let state = Arc::clone(&self.state);
        let stop = Arc::clone(&self.stop);
        self.handle = Some(thread::spawn(move || run(state, stop)));
    }

    /// Ask the thread to stop and wait for it (returns within ~250 ms).
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }

    pub fn snapshot(&self) -> ProbeState {
        *self.state.lock().unwrap()
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(state: Arc<Mutex<ProbeState>>, stop: Arc<AtomicBool>) {
    log::info!("environment probe started");
    while !stop.load(Ordering::Relaxed) {
        let next = ProbeState {
            vigem: Some(check_vigem()),
            wheel: check_wheel(),
            pit_house: check_pit_house(),
        };
        if stop.load(Ordering::Relaxed) {
            break;
        }
        *state.lock().unwrap() = next;
        sleep_unless_stopped(&stop, CHECK_INTERVAL);
    }
    log::info!("environment probe stopped");
}

fn sleep_unless_stopped(stop: &AtomicBool, total: Duration) {
    let end = Instant::now() + total;
    while !stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        if now >= end {
            break;
        }
        thread::sleep(SLICE.min(end - now));
    }
}

// ---------------------------------------------------------------- system calls

/// The bus driver is usable when a client can connect. No target is created
/// and the client is dropped immediately.
fn check_vigem() -> bool {
    vigem_rust::Client::connect().is_ok()
}

/// Enumeration only; the device is never opened.
fn check_wheel() -> bool {
    match hidapi::HidApi::new() {
        Ok(api) => api
            .device_list()
            .any(|i| is_moza(i.vendor_id(), i.product_id())),
        Err(_) => false,
    }
}

fn check_pit_house() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    // SAFETY: plain Win32 snapshot walk; the entry is zeroed with dwSize set
    // as the API requires, and the handle is closed before returning.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = false;
        let mut ok = Process32FirstW(snap, &mut entry) != 0;
        while ok {
            if exe_matches(&entry.szExeFile, PIT_HOUSE_EXE) {
                found = true;
                break;
            }
            ok = Process32NextW(snap, &mut entry) != 0;
        }
        CloseHandle(snap);
        found
    }
}

// ----------------------------------------------------------------- pure logic

fn is_moza(vid: u16, pid: u16) -> bool {
    vid == crate::hid::MOZA_VID && crate::hid::MOZA_PIDS.contains(&pid)
}

/// Compare a NUL-terminated UTF-16 exe name with `want`, ignoring case.
fn exe_matches(wide: &[u16], want: &str) -> bool {
    let len = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..len]).eq_ignore_ascii_case(want)
}

pub struct ChecklistRow {
    pub level: Level,
    pub title: &'static str,
    pub detail: &'static str,
    pub action: Option<&'static str>,
}

pub struct Checklist {
    pub title: &'static str,
    pub rows: [ChecklistRow; 3],
    /// False while a required item (driver, wheel) is known to be bad.
    pub start_allowed: bool,
}

fn row(
    level: Level,
    title: &'static str,
    detail: &'static str,
    action: Option<&'static str>,
) -> ChecklistRow {
    ChecklistRow { level, title, detail, action }
}

pub fn checklist(p: ProbeState) -> Checklist {
    const D: &str = "ViGEm Bus Driver";
    let vigem = match p.vigem {
        None => row(Level::Neutral, D, "Checking...", None),
        Some(true) => row(Level::Good, D, "Installed", None),
        Some(false) => row(
            Level::Bad,
            D,
            "Not installed. The virtual Xbox controller needs it.",
            Some("Get the driver"),
        ),
    };
    let wheel = if p.wheel {
        row(Level::Good, "MOZA wheel", "Connected over USB", None)
    } else {
        row(Level::Bad, "MOZA wheel", "Not found. Plug in the wheelbase and switch it on.", None)
    };
    let pit = if p.pit_house {
        row(Level::Warn, "MOZA Pit House", "Running. Pedals may jitter and shift lights cannot connect while it is open.", None)
    } else {
        row(Level::Good, "MOZA Pit House", "Not running", None)
    };
    let bad = vigem.level == Level::Bad || wheel.level == Level::Bad;
    Checklist {
        title: if bad { "Almost there" } else { "Ready to drive?" },
        rows: [vigem, wheel, pit],
        start_allowed: !bad,
    }
}

/// `(text, level)` for the Wheel and Driver pills (spec 4.1).
///
/// While the bridge runs the probe is stopped and its results are stale, so
/// they are ignored then.
pub fn pills(
    p: ProbeState,
    running: bool,
    bridge_has_wheel: bool,
) -> ((&'static str, Level), (&'static str, Level)) {
    let wheel = if bridge_has_wheel {
        ("Connected", Level::Good)
    } else if running {
        ("Connecting", Level::Neutral)
    } else if p.wheel {
        ("Detected", Level::Neutral)
    } else {
        ("Not found", Level::Bad)
    };
    let driver = if running {
        // A running bridge proves the driver works; until it has the wheel
        // open nothing is known.
        if bridge_has_wheel {
            ("Ready", Level::Good)
        } else {
            ("Checking", Level::Neutral)
        }
    } else {
        match p.vigem {
            None => ("Checking", Level::Neutral),
            Some(true) => ("Ready", Level::Good),
            Some(false) => ("Missing", Level::Bad),
        }
    };
    (wheel, driver)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(vigem: Option<bool>, wheel: bool, pit_house: bool) -> ProbeState {
        ProbeState { vigem, wheel, pit_house }
    }

    #[test]
    fn all_good() {
        let c = checklist(st(Some(true), true, false));
        assert_eq!(c.title, "Ready to drive?");
        assert!(c.start_allowed);
        assert_eq!(c.rows[0].level, Level::Good);
        assert_eq!(c.rows[0].detail, "Installed");
        assert_eq!(c.rows[1].detail, "Connected over USB");
        assert_eq!(c.rows[2].level, Level::Good);
        assert_eq!(c.rows[2].detail, "Not running");
    }

    #[test]
    fn driver_missing_offers_download() {
        let c = checklist(st(Some(false), true, false));
        assert_eq!(c.title, "Almost there");
        assert!(!c.start_allowed);
        assert_eq!(c.rows[0].level, Level::Bad);
        assert_eq!(c.rows[0].action, Some("Get the driver"));
        assert_eq!(
            c.rows[0].detail,
            "Not installed. The virtual Xbox controller needs it."
        );
    }

    #[test]
    fn wheel_missing_blocks_start() {
        let c = checklist(st(Some(true), false, false));
        assert_eq!(c.title, "Almost there");
        assert!(!c.start_allowed);
        assert_eq!(c.rows[1].level, Level::Bad);
        assert_eq!(c.rows[1].detail, "Not found. Plug in the wheelbase and switch it on.");
    }

    #[test]
    fn unchecked_driver_is_neutral_and_not_bad() {
        let c = checklist(st(None, true, false));
        assert_eq!(c.rows[0].level, Level::Neutral);
        assert_eq!(c.rows[0].detail, "Checking...");
        assert!(c.start_allowed);
        assert_eq!(c.title, "Ready to drive?");
    }

    #[test]
    fn pit_house_is_only_a_warning() {
        let c = checklist(st(Some(true), true, true));
        assert_eq!(c.rows[2].level, Level::Warn);
        assert!(c.start_allowed);
        assert_eq!(c.title, "Ready to drive?");
    }

    #[test]
    fn default_probe_state_is_unchecked_and_wheel_missing() {
        let c = checklist(ProbeState::default());
        assert!(!c.start_allowed);
        assert_eq!(c.rows[0].level, Level::Neutral);
    }

    #[test]
    fn pills_when_stopped() {
        let (w, d) = pills(st(Some(true), true, false), false, false);
        assert_eq!(w, ("Detected", Level::Neutral));
        assert_eq!(d, ("Ready", Level::Good));
        let (w, d) = pills(st(Some(false), false, false), false, false);
        assert_eq!(w, ("Not found", Level::Bad));
        assert_eq!(d, ("Missing", Level::Bad));
        let (_, d) = pills(st(None, false, false), false, false);
        assert_eq!(d, ("Checking", Level::Neutral));
    }

    #[test]
    fn pills_when_running_ignore_stale_probe() {
        // Stale probe says driver missing and no wheel; bridge proves otherwise.
        let (w, d) = pills(st(Some(false), false, false), true, true);
        assert_eq!(w, ("Connected", Level::Good));
        assert_eq!(d, ("Ready", Level::Good));
        // Started but wheel not open yet: nothing is claimed from stale data.
        let (w, d) = pills(st(Some(true), true, false), true, false);
        assert_eq!(w, ("Connecting", Level::Neutral));
        assert_eq!(d, ("Checking", Level::Neutral));
    }

    #[test]
    fn moza_matching() {
        assert!(is_moza(0x346E, 0x0005));
        assert!(is_moza(0x346E, 0x0015));
        assert!(!is_moza(0x346E, 0x0001));
        assert!(!is_moza(0x1234, 0x0005));
    }

    #[test]
    fn exe_name_match_is_case_insensitive_and_nul_terminated() {
        let mut buf = [0u16; 260];
        for (i, c) in "moza pit house.EXE".encode_utf16().enumerate() {
            buf[i] = c;
        }
        assert!(exe_matches(&buf, PIT_HOUSE_EXE));
        let mut other = [0u16; 260];
        for (i, c) in "notepad.exe".encode_utf16().enumerate() {
            other[i] = c;
        }
        assert!(!exe_matches(&other, PIT_HOUSE_EXE));
    }

    #[test]
    fn probe_thread_stops_quickly() {
        let mut p = Probe::new();
        p.start();
        assert!(p.is_running());
        std::thread::sleep(Duration::from_millis(100));
        let t = Instant::now();
        p.stop();
        assert!(t.elapsed() < Duration::from_millis(1500), "stop took {:?}", t.elapsed());
        assert!(!p.is_running());
    }

    #[test]
    fn sleep_slices_honour_stop_flag() {
        let stop = AtomicBool::new(true);
        let t = Instant::now();
        sleep_unless_stopped(&stop, Duration::from_secs(5));
        assert!(t.elapsed() < Duration::from_millis(100));
    }
}
