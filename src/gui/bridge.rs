use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use crate::config::AppConfig;
use crate::hid::state::{decode, DeviceState};
use crate::mapping;
use crate::output::X360Out;

use super::{AppState, MozaMapperApp};

impl MozaMapperApp {
    pub(crate) fn start_bridge(&mut self) {
        if self.running || self.wizard_running() {
            // The wizard reads the same HID interface; concurrent reads are
            // split between the two handles and both learn/bridge garbage.
            return;
        }
        // The probe must be gone before the bridge touches the wheel and ViGEm.
        self.probe.stop();
        let state = self.state.clone();
        let config = self.config.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let lights = self.lights_handle.clone();

        let handle = thread::spawn(move || run_bridge(state, config, stop_thread, lights));
        self.bridge_handle = Some(handle);
        self.stop_flag = Some(stop);
        self.running = true;
        // A fresh start gets a clean slate; stale errors must not mask new
        // ones (e.g. the known FFB-unavailable banner).
        *self.state.last_error.lock().unwrap() = None;
    }

    pub(crate) fn stop_bridge(&mut self) {
        if let Some(stop) = self.stop_flag.take() {
            stop.store(true, Ordering::Relaxed);
        }
        if let Some(handle) = self.bridge_handle.take() {
            let _ = handle.join();
        }
        self.running = false;
    }
}

/// Bridge thread: HID read → decode → map → ViGEm X360 update.
fn run_bridge(state: AppState, config: AppConfig, stop: Arc<AtomicBool>, lights: Option<Arc<crate::rpm::LightsHandle>>) {
    let api = match hidapi::HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            state.set_error(format!("hidapi init failed: {}", e));
            return;
        }
    };

    let (device, layout) = match crate::hid::device::open_moza_with_layout(&api) {
        Ok(v) => v,
        Err(e) => {
            state.set_error(format!(
                "MOZA wheel not found (close MOZA Pit House, then press Start again): {}",
                e
            ));
            return;
        }
    };
    log::info!("MOZA wheel connected");
    *state.device_connected.lock().unwrap() = true;

    // Force feedback uses a dedicated Win32 handle (SET_REPORT transport).
    let mut ffb = None;
    if config.ffb_enabled {
        // Re-enumerate to get the device path for the transport.
        match hidapi::HidApi::new() {
            Ok(api2) => {
                let path = api2.device_list().find_map(|i| {
                    if i.vendor_id() == crate::hid::MOZA_VID
                        && crate::hid::MOZA_PIDS.contains(&i.product_id())
                    {
                        Some(i.path().to_owned())
                    } else {
                        None
                    }
                });
                match path {
                    Some(path) => match crate::ffb::FfbController::from_device_path(
                        &path,
                        config.ffb_gain,
                    ) {
                        Ok(c) => {
                            c.start_effect();
                            ffb = Some(Arc::new(c));
                        }
                        Err(e) => state.set_error(format!(
                            "Force feedback unavailable (wheel input continues): {}",
                            e
                        )),
                    },
                    None => log::warn!("FFB: MOZA device not found on re-enumeration"),
                }
            }
            Err(e) => state.set_error(format!("hidapi re-init for FFB failed: {}", e)),
        }
    }
    if let Some(c) = &ffb {
        state.ffb_active.store(c.enabled(), Ordering::Relaxed);
    }

    let mut controller = match X360Out::create(ffb, lights) {
        Ok(c) => c,
        Err(e) => {
            state.set_error(format!(
                "Virtual controller failed (is the ViGEm Bus Driver installed?): {}",
                e
            ));
            *state.device_connected.lock().unwrap() = false;
            return;
        }
    };
    log::info!("virtual X360 controller ready");

    let cal = config.calibration;
    let mapping_cfg = config.mapping;
    let mut buffer = vec![0u8; 64];
    let mut frame_count = 0u64;
    let mut fps_time = Instant::now();
    // On Windows, Ok(0) is only the timeout; Err means real failure — an
    // unplugged wheel errors on EVERY read, which without this counter would
    // spin forever while the UI still says "Connected".
    const MAX_CONSECUTIVE_READ_ERRORS: u32 = 50;
    let mut read_errors: u32 = 0;

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        match device.read_timeout(&mut buffer, 10) {
            Ok(0) => {}
            Err(e) => {
                read_errors += 1;
                if read_errors >= MAX_CONSECUTIVE_READ_ERRORS {
                    state.set_error(format!(
                        "MOZA wheel lost (read error: {}). Press Start to reconnect.",
                        e
                    ));
                    break;
                }
            }
            Ok(len) => {
                read_errors = 0;
                let dev_state = decode(&buffer[..len], &layout, &cal);
                let buttons = mapping::apply(&dev_state, &mapping_cfg);

                if let Err(e) = controller.update(&dev_state, buttons) {
                    log::warn!("controller update error: {}", e);
                }

                *state.current_state.lock().unwrap() = dev_state;
                *state.report_count.lock().unwrap() += 1;

                frame_count += 1;
                let now = Instant::now();
                if now.duration_since(fps_time).as_secs() >= 1 {
                    let elapsed = now.duration_since(fps_time).as_secs_f64();
                    *state.fps.lock().unwrap() = frame_count as f64 / elapsed;
                    frame_count = 0;
                    fps_time = now;
                }
            }
        }
    }

    // Release the virtual controller cleanly: send a neutral (centered, no
    // buttons, no triggers) report so the game does not keep the last latched
    // steering/trigger values.
    let _ = controller.update(&DeviceState::default(), 0);
    *state.device_connected.lock().unwrap() = false;
    state.ffb_active.store(false, Ordering::Relaxed);
    log::info!("bridge stopped");
}
