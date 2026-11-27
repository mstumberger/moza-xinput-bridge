use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::MozaMapperApp;

/// Manual FFB test: a thread streams the torque command at ~200 Hz while
/// the GUI holds the target value; a reader thread tracks the steering axis
/// so movement can be measured objectively.
pub(crate) struct FfbTest {
    pub(crate) run: Arc<AtomicBool>,
    pub(crate) torque_cmd: Arc<Mutex<i16>>,
    pub(crate) thread: Option<std::thread::JoinHandle<()>>,
    pub strength_pct: f32,
    /// Latest raw steering value (u16), u32::MAX until the first report.
    pub(crate) steer: Arc<AtomicU32>,
    /// Auto pulse test: request flag, strength (f32 bits), result text.
    pub(crate) pulse_req: Arc<AtomicBool>,
    pub(crate) pulse_strength: Arc<AtomicU32>,
    pub(crate) pulse_result: Arc<Mutex<String>>,
}

/// Run torque `t` for `dur` while sampling steering; returns the signed net
/// movement and the peak deviation (raw counts).
fn ffb_pulse(
    channel: &crate::ffb::vendor::TorqueChannel,
    steer: &AtomicU32,
    t: i16,
    dur: Duration,
) -> (i32, u32) {
    let start_raw = steer.load(Ordering::Relaxed) as i32;
    let mut peak = 0u32;
    let start = Instant::now();
    while start.elapsed() < dur {
        if let Err(e) = channel.set_torque(t) {
            log::warn!("ffb pulse write failed: {}", e);
            break;
        }
        let cur = steer.load(Ordering::Relaxed) as i32;
        peak = peak.max((cur - start_raw).unsigned_abs());
        thread::sleep(Duration::from_millis(5));
    }
    let _ = channel.set_torque(0);
    let net = steer.load(Ordering::Relaxed) as i32 - start_raw;
    (net, peak)
}

impl MozaMapperApp {
    pub(crate) fn start_ffb_test(&mut self) {
        if self.ffb_test.is_some() {
            return;
        }
        let run = Arc::new(AtomicBool::new(true));
        let torque_cmd = Arc::new(Mutex::new(0i16));
        let run_thread = Arc::clone(&run);
        let cmd_thread = Arc::clone(&torque_cmd);
        let steer = Arc::new(AtomicU32::new(u32::MAX));
        let pulse_req = Arc::new(AtomicBool::new(false));
        let pulse_strength = Arc::new(AtomicU32::new(30f32.to_bits()));
        let pulse_result = Arc::new(Mutex::new(String::new()));
        let steer_t = Arc::clone(&steer);
        let req_t = Arc::clone(&pulse_req);
        let str_t = Arc::clone(&pulse_strength);
        let res_t = Arc::clone(&pulse_result);
        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let handle = thread::spawn(move || {
            let api = match hidapi::HidApi::new() {
                Ok(a) => a,
                Err(e) => {
                    let _ = tx.send(Err(format!("hidapi init failed: {}", e)));
                    return;
                }
            };
            let path = api.device_list().find_map(|i| {
                if i.vendor_id() == crate::hid::MOZA_VID
                    && crate::hid::MOZA_PIDS.contains(&i.product_id())
                {
                    Some(i.path().to_owned())
                } else {
                    None
                }
            });
            let path = match path {
                Some(p) => p,
                None => {
                    let _ = tx.send(Err("MOZA wheel not found".into()));
                    return;
                }
            };
            log::info!("ffb test: opening torque channel");
            let channel = match crate::ffb::vendor::TorqueChannel::open(&path) {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Err(e));
                    return;
                }
            };
            // Steering reader for objective movement measurement.
            let api_r = hidapi::HidApi::new();
            if let Ok(api_r) = api_r {
                if let Ok((dev, _)) = crate::hid::device::open_moza_with_layout(&api_r) {
                    let steer_r = Arc::clone(&steer_t);
                    let run_r = Arc::clone(&run_thread);
                    thread::spawn(move || {
                        let mut buf = [0u8; 64];
                        while run_r.load(Ordering::Relaxed) {
                            if let Ok(n) = dev.read_timeout(&mut buf, 20) {
                                if n >= 3 {
                                    let raw = u16::from_le_bytes([buf[1], buf[2]]);
                                    steer_r.store(raw as u32, Ordering::Relaxed);
                                }
                            }
                        }
                    });
                } else {
                    log::warn!("ffb test: steering reader unavailable");
                }
            }
            let _ = tx.send(Ok(()));
            while run_thread.load(Ordering::Relaxed) {
                if req_t.swap(false, Ordering::Relaxed) {
                    if steer_t.load(Ordering::Relaxed) == u32::MAX {
                        *res_t.lock().unwrap() = "no steering data, cannot measure".into();
                    } else {
                        let pct = f32::from_bits(str_t.load(Ordering::Relaxed));
                        let mag = (pct / 100.0 * 32767.0) as i16;
                        *res_t.lock().unwrap() = "running: LEFT pulse...".into();
                        let (l_net, l_peak) =
                            ffb_pulse(&channel, &steer_t, mag, Duration::from_millis(500));
                        thread::sleep(Duration::from_millis(800));
                        *res_t.lock().unwrap() = "running: RIGHT pulse...".into();
                        let (r_net, r_peak) =
                            ffb_pulse(&channel, &steer_t, -mag, Duration::from_millis(500));
                        let verdict = if l_peak < 300 && r_peak < 300 {
                            "NO FORCE: wheel did not move"
                        } else if l_net < 0 && r_net > 0 {
                            "OK: force applied, left and right correct"
                        } else if l_net > 0 && r_net < 0 {
                            "REVERSED: left pushed right and right pushed left"
                        } else {
                            "MOVED, but directions unclear (hands on wheel?)"
                        };
                        *res_t.lock().unwrap() = format!(
                            "{verdict}\nleft pulse: net {l_net:+} counts (peak {l_peak})\nright pulse: net {r_net:+} counts (peak {r_peak})\n(full lock-to-lock = 65535 counts)"
                        );
                        log::info!("ffb auto pulse: L net {l_net} peak {l_peak}, R net {r_net} peak {r_peak}");
                    }
                    continue;
                }
                let t = *cmd_thread.lock().unwrap();
                if let Err(e) = channel.set_torque(t) {
                    log::warn!("ffb test torque write failed: {}", e);
                }
                // 5 ms while applying force, 50 ms while idle
                let delay = if t != 0 { 5 } else { 50 };
                thread::sleep(Duration::from_millis(delay));
            }
            channel.stop();
        });
        match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(Ok(())) => {
                self.ffb_test = Some(FfbTest {
                    run,
                    torque_cmd,
                    thread: Some(handle),
                    strength_pct: 30.0,
                    steer,
                    pulse_req,
                    pulse_strength,
                    pulse_result,
                });
            }
            Ok(Err(e)) => {
                run.store(false, Ordering::Relaxed);
                self.ffb_test_error = Some(e);
            }
            Err(_) => {
                run.store(false, Ordering::Relaxed);
                self.ffb_test_error = Some("FFB test init timed out".into());
            }
        }
    }

    pub(crate) fn stop_ffb_test(&mut self) {
        if let Some(mut t) = self.ffb_test.take() {
            *t.torque_cmd.lock().unwrap() = 0;
            t.run.store(false, Ordering::Relaxed);
            if let Some(h) = t.thread.take() {
                let _ = h.join();
            }
        }
    }
}
