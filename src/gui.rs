//! egui GUI for the MOZA R3 → ViGEm XInput bridge.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::config::AppConfig;
use crate::hid::state::{decode, DeviceState};
use crate::mapping;
use crate::output::X360Out;

/// Shared state between the bridge thread and the GUI.
#[derive(Clone)]
pub struct AppState {
    pub device_connected: Arc<Mutex<bool>>,
    pub current_state: Arc<Mutex<DeviceState>>,
    pub fps: Arc<Mutex<f64>>,
    pub report_count: Arc<Mutex<u64>>,
    pub ffb_active: Arc<AtomicBool>,
    /// Last error to surface in the GUI (never swallowed silently).
    pub last_error: Arc<Mutex<Option<String>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            device_connected: Arc::new(Mutex::new(false)),
            current_state: Arc::new(Mutex::new(DeviceState::default())),
            fps: Arc::new(Mutex::new(0.0)),
            report_count: Arc::new(Mutex::new(0)),
            ffb_active: Arc::new(AtomicBool::new(false)),
            last_error: Arc::new(Mutex::new(None)),
        }
    }

    fn set_error(&self, msg: impl Into<String>) {
        let msg = msg.into();
        log::error!("{}", msg);
        *self.last_error.lock().unwrap() = Some(msg);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct MozaMapperApp {
    state: AppState,
    running: bool,
    config: AppConfig,
    config_error: Option<String>,
    bridge_handle: Option<thread::JoinHandle<()>>,
    stop_flag: Option<Arc<AtomicBool>>,
    // Mapping wizard (runs on its own thread, drives the GUI via shared state)
    wizard_prompt: Arc<Mutex<String>>,
    wizard_log: Arc<Mutex<Vec<String>>>,
    wizard_result: Arc<Mutex<Option<crate::config::MappingConfig>>>,
    wizard_thread: Option<thread::JoinHandle<()>>,
    wizard_cancel: Option<Arc<AtomicBool>>,
    // FFB manual test channel (vendor torque protocol, decoded from USBPcap)
    ffb_test: Option<FfbTest>,
    ffb_test_error: Option<String>,
    // RPM shift lights (serial LED-bar channel, decoded from USBPcap)
    lights: Option<crate::rpm::LightsController>,
    lights_handle: Option<Arc<crate::rpm::LightsHandle>>,
    lights_error: Option<String>,
    manual_rpm: u16,
    sweep_thread: Option<thread::JoinHandle<()>>,
    sweep_run: Option<Arc<AtomicBool>>,
    udp_thread: Option<thread::JoinHandle<()>>,
    udp_run: Option<Arc<AtomicBool>>,
}

/// Manual FFB test: a thread streams the torque command at ~200 Hz while
/// the GUI holds the target value; a reader thread tracks the steering axis
/// so movement can be measured objectively.
struct FfbTest {
    run: Arc<AtomicBool>,
    torque_cmd: Arc<Mutex<i16>>,
    thread: Option<std::thread::JoinHandle<()>>,
    pub strength_pct: f32,
    /// Latest raw steering value (u16), u32::MAX until the first report.
    steer: Arc<AtomicU32>,
    /// Auto pulse test: request flag, strength (f32 bits), result text.
    pulse_req: Arc<AtomicBool>,
    pulse_strength: Arc<AtomicU32>,
    pulse_result: Arc<Mutex<String>>,
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
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (config, config_error) = AppConfig::load_or_default();
        Self {
            state: AppState::new(),
            running: false,
            config,
            config_error,
            bridge_handle: None,
            stop_flag: None,
            wizard_prompt: Arc::new(Mutex::new(String::new())),
            wizard_log: Arc::new(Mutex::new(Vec::new())),
            wizard_result: Arc::new(Mutex::new(None)),
            wizard_thread: None,
            wizard_cancel: None,
            ffb_test: None,
            ffb_test_error: None,
            lights: None,
            lights_handle: None,
            lights_error: None,
            manual_rpm: 0,
            sweep_thread: None,
            sweep_run: None,
            udp_thread: None,
            udp_run: None,
        }
    }

    /// Start the shift-light controller + UDP listener from the current
    /// config. Threshold/max-rpm changes apply live via set_config; UDP
    /// port changes need an off/on cycle.
    fn start_lights(&mut self) {
        if self.lights.is_some() {
            return;
        }
        match crate::rpm::LightsController::start(self.config.rpm_lights.clone()) {
            Ok(c) => {
                let handle = c.handle();
                if self.config.rpm_lights.udp_enabled {
                    let run = Arc::new(AtomicBool::new(true));
                    match crate::rpm::telemetry::spawn_udp_listener(
                        Arc::clone(&handle),
                        self.config.rpm_lights.udp_port,
                        Arc::clone(&run),
                    ) {
                        Ok(t) => {
                            self.udp_thread = Some(t);
                            self.udp_run = Some(run);
                        }
                        Err(e) => {
                            self.lights_error = Some(format!(
                                "UDP listener (port {}) failed: {}",
                                self.config.rpm_lights.udp_port, e
                            ));
                        }
                    }
                }
                self.lights = Some(c);
                self.lights_handle = Some(handle);
                self.lights_error = None;
            }
            Err(e) => self.lights_error = Some(e),
        }
    }

    fn stop_lights(&mut self) {
        self.stop_sweep();
        if let Some(run) = self.udp_run.take() {
            run.store(false, Ordering::Relaxed);
        }
        if let Some(t) = self.udp_thread.take() {
            let _ = t.join();
        }
        // Dropping the controller stops its thread and clears the bar.
        self.lights = None;
        self.lights_handle = None;
    }

    fn stop_sweep(&mut self) {
        if let Some(run) = self.sweep_run.take() {
            run.store(false, Ordering::Relaxed);
        }
        if let Some(t) = self.sweep_thread.take() {
            let _ = t.join();
        }
    }

    /// Sweep the bar 0 -> full -> 0 over ~2.5 s so the user can see all
    /// lights and their order without telemetry.
    fn start_sweep(&mut self) {
        if let (Some(handle), None) = (&self.lights_handle, &self.sweep_thread) {
            let handle = Arc::clone(handle);
            let run = Arc::new(AtomicBool::new(true));
            self.sweep_run = Some(Arc::clone(&run));
            self.sweep_thread = Some(thread::spawn(move || {
                let n = 10u32; // full bar regardless of config; it is a test
                let step = Duration::from_millis(120);
                for i in 0..=n {
                    if !run.load(Ordering::Relaxed) {
                        break;
                    }
                    handle.send_mask(((1u32 << i) - 1) as u16);
                    thread::sleep(step);
                }
                for i in (0..n).rev() {
                    if !run.load(Ordering::Relaxed) {
                        break;
                    }
                    handle.send_mask(((1u32 << i) - 1) as u16);
                    thread::sleep(step);
                }
                handle.send_mask(0);
            }));
        }
    }

    fn start_ffb_test(&mut self) {
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

    fn stop_ffb_test(&mut self) {
        if let Some(mut t) = self.ffb_test.take() {
            *t.torque_cmd.lock().unwrap() = 0;
            t.run.store(false, Ordering::Relaxed);
            if let Some(h) = t.thread.take() {
                let _ = h.join();
            }
        }
    }

    fn wizard_running(&self) -> bool {
        self.wizard_thread.as_ref().is_some_and(|t| !t.is_finished())
    }

    fn start_wizard(&mut self) {
        if self.wizard_running() || self.running {
            return;
        }
        let prompt = Arc::clone(&self.wizard_prompt);
        let log = Arc::clone(&self.wizard_log);
        let result = Arc::clone(&self.wizard_result);
        // Base mapping comes from the GUI's live config (NOT a fresh disk
        // read) and the wizard writes nothing to disk itself — the GUI merges
        // and saves once. Two writers to app.json caused lost settings.
        let base_mapping = self.config.mapping.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.wizard_cancel = Some(Arc::clone(&cancel));

        let handle = thread::spawn(move || {
            let api = match hidapi::HidApi::new() {
                Ok(a) => a,
                Err(e) => {
                    log.lock().unwrap().push(format!("hidapi init failed: {}", e));
                    return;
                }
            };
            let (dev, layout) = match crate::hid::device::open_moza_with_layout(&api) {
                Ok(v) => v,
                Err(e) => {
                    log.lock().unwrap().push(format!("MOZA wheel not found: {}", e));
                    return;
                }
            };
            let mut wizard = crate::mapping::wizard::Wizard::new(dev, layout, cancel);
            let p = Arc::clone(&prompt);
            let l = Arc::clone(&log);
            let mapping = wizard.learn_all(
                base_mapping,
                move |name| *p.lock().unwrap() = format!("Press the wheel button for: {}", name),
                move |name, learned| match learned {
                    Some(n) => l.lock().unwrap().push(format!("{} -> button {}", name, n)),
                    None => l.lock().unwrap().push(format!("{}: skipped, kept previous", name)),
                },
            );
            *result.lock().unwrap() = Some(mapping);
            *prompt.lock().unwrap() = String::new();
        });
        self.wizard_thread = Some(handle);
    }

    fn start_bridge(&mut self) {
        if self.running || self.wizard_running() {
            // The wizard reads the same HID interface; concurrent reads are
            // split between the two handles and both learn/bridge garbage.
            return;
        }
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

    fn stop_bridge(&mut self) {
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

impl eframe::App for MozaMapperApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(16));

        // Reconcile the running flag with actual thread liveness: the bridge
        // thread exits by itself on errors (wheel lost, ViGEm missing), and
        // without this the Start/Stop button would show "Stop" forever.
        if self.running
            && self
                .bridge_handle
                .as_ref()
                .is_some_and(|t| t.is_finished())
        {
            if let Some(handle) = self.bridge_handle.take() {
                let _ = handle.join();
            }
            self.stop_flag = None;
            self.running = false;
        }

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Moza Mapper")
                        .size(22.0)
                        .color(egui::Color32::LIGHT_BLUE),
                );
                ui.separator();

                let connected = *self.state.device_connected.lock().unwrap();
                let (text, color) = if connected {
                    ("Connected", egui::Color32::GREEN)
                } else {
                    ("Disconnected", egui::Color32::GRAY)
                };
                ui.label(egui::RichText::new(text).color(color).size(15.0));
                ui.separator();

                let fps = *self.state.fps.lock().unwrap() as u32;
                let count = *self.state.report_count.lock().unwrap();
                ui.label(
                    egui::RichText::new(format!("{} Hz | {} reports", fps, count))
                        .color(egui::Color32::YELLOW)
                        .size(13.0),
                );

                let ffb_on = self.state.ffb_active.load(Ordering::Relaxed);
                let (ffb_text, ffb_color) = if ffb_on {
                    ("FFB on", egui::Color32::GREEN)
                } else {
                    ("FFB off", egui::Color32::GRAY)
                };
                ui.separator();
                ui.label(egui::RichText::new(ffb_text).color(ffb_color).size(13.0));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let wizard_active = self.wizard_running();
                    let (button_text, color) = if self.running {
                        ("Stop", egui::Color32::RED)
                    } else {
                        ("Start", egui::Color32::GREEN)
                    };
                    let mut btn = egui::Button::new(egui::RichText::new(button_text).size(15.0));
                    if self.running {
                        btn = btn.fill(color);
                    } else if wizard_active {
                        // Greyed out: the wizard owns the HID device.
                        btn = btn.fill(egui::Color32::DARK_GRAY);
                    } else {
                        btn = btn.fill(color);
                    }
                    let response = ui.add(btn);
                    let clickable = !(!self.running && wizard_active);
                    if response.clicked() && clickable {
                        if self.running {
                            self.stop_bridge();
                        } else {
                            self.start_bridge();
                        }
                    }
                });
            });
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // Surface config errors — never swallow them.
            if let Some(err) = &self.config_error {
                ui.colored_label(egui::Color32::RED, format!("Config error: {}", err));
                ui.separator();
            }
            if let Some(err) = self.state.last_error.lock().unwrap().clone() {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::RED, &err);
                    if ui.small_button("×").clicked() {
                        *self.state.last_error.lock().unwrap() = None;
                    }
                });
                ui.separator();
            }

            let s = self.state.current_state.lock().unwrap().clone();

            // FFB settings — both controls autosave on change; a separate
            // Save button was inconsistent (checkbox saved, slider didn't).
            ui.group(|ui| {
                ui.label("Force feedback");
                ui.horizontal(|ui| {
                    let mut enabled = self.config.ffb_enabled;
                    if ui
                        .checkbox(&mut enabled, "Enable FFB (applies on next Start)")
                        .changed()
                    {
                        self.config.ffb_enabled = enabled;
                        if let Err(e) = self.config.save() {
                            self.config_error = Some(e);
                        }
                    }
                    let slider = egui::Slider::new(&mut self.config.ffb_gain, 0.1..=2.0)
                        .text("Gain (applies on next Start)");
                    if ui.add(slider).changed() {
                        if let Err(e) = self.config.save() {
                            self.config_error = Some(e);
                        }
                    }
                });
            });
            ui.add_space(10.0);

            // FFB manual test — decoded vendor torque protocol.
            let mut close_ffb_test = false;
            ui.group(|ui| {
                ui.label("FFB test (direct torque)");
                if let Some(err) = self.ffb_test_error.clone() {
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::RED, format!("FFB test: {}", err));
                        if ui.small_button("×").clicked() {
                            self.ffb_test_error = None;
                        }
                    });
                }
                if let Some(test) = &mut self.ffb_test {
                    ui.add(
                        egui::Slider::new(&mut test.strength_pct, 5.0..=100.0)
                            .text("Strength %"),
                    );
                    let strength = test.strength_pct;
                    test.pulse_strength.store(strength.to_bits(), Ordering::Relaxed);
                    let cmd = test.torque_cmd.clone();
                    let magnitude = (strength / 100.0 * 32767.0) as i16;

                    // Live steering readout.
                    let raw = test.steer.load(Ordering::Relaxed);
                    if raw == u32::MAX {
                        ui.label("Steering: no data");
                    } else {
                        let centered = raw as i32 - 32768;
                        ui.label(format!("Steering raw: {raw}  (center offset {centered:+})"));
                        ui.add(
                            egui::ProgressBar::new(raw as f32 / 65535.0)
                                .text(format!("{:+.1}%", centered as f32 / 327.68)),
                        );
                    }

                    ui.label("Hold a button to push (force only while held):");
                    let (left, right) = ui
                        .horizontal(|ui| {
                            let l = ui
                                .add(
                                    egui::Button::new("◀ Push left")
                                        .min_size(egui::vec2(120.0, 32.0))
                                        .sense(egui::Sense::click_and_drag()),
                                )
                                .is_pointer_button_down_on();
                            let r = ui
                                .add(
                                    egui::Button::new("Push right ▶")
                                        .min_size(egui::vec2(120.0, 32.0))
                                        .sense(egui::Sense::click_and_drag()),
                                )
                                .is_pointer_button_down_on();
                            (l, r)
                        })
                        .inner;
                    // Positive torque turns the wheel LEFT (verified on hardware).
                    *cmd.lock().unwrap() = if left {
                        magnitude
                    } else if right {
                        -magnitude
                    } else {
                        0
                    };

                    ui.separator();
                    if ui
                        .button("Auto pulse test (hands off: left 0.5 s, then right 0.5 s)")
                        .clicked()
                    {
                        test.pulse_req.store(true, Ordering::Relaxed);
                        *test.pulse_result.lock().unwrap() = "starting...".into();
                    }
                    let res = test.pulse_result.lock().unwrap().clone();
                    if !res.is_empty() {
                        let color = if res.starts_with("OK") {
                            egui::Color32::GREEN
                        } else if res.starts_with("NO FORCE") || res.starts_with("REVERSED") {
                            egui::Color32::RED
                        } else {
                            egui::Color32::YELLOW
                        };
                        ui.colored_label(color, res);
                    }
                    // Keep the live readout moving.
                    ctx.request_repaint_after(Duration::from_millis(30));

                    if ui.button("Close FFB test").clicked() {
                        close_ffb_test = true;
                    }
                } else {
                    ui.label("Opens the wheel for direct torque: Push left/right buttons, live steering and an auto pulse test appear here.");
                    if ui.button("Open FFB test").clicked() {
                        self.start_ffb_test();
                    }
                }
            });
            if close_ffb_test {
                self.stop_ffb_test();
            }
            ui.add_space(10.0);

            // RPM shift lights — serial LED-bar channel (decoded from USBPcap).
            ui.group(|ui| {
                ui.label("RPM shift lights");
                if let Some(err) = self.lights_error.clone() {
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::RED, format!("Lights: {}", err));
                        if ui.small_button("×").clicked() {
                            self.lights_error = None;
                        }
                    });
                }
                let mut enabled = self.config.rpm_lights.enabled;
                if ui.checkbox(&mut enabled, "Enable (opens the wheelbase serial port)").changed() {
                    self.config.rpm_lights.enabled = enabled;
                    if enabled {
                        self.start_lights();
                    } else {
                        self.stop_lights();
                    }
                    if let Err(e) = self.config.save() {
                        self.config_error = Some(e);
                    }
                }
                if self.lights.is_some() {
                    let mut cfg = self.config.rpm_lights.clone();
                    let mut save = false;
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::DragValue::new(&mut cfg.max_rpm).range(500..=20_000).suffix(" rpm"))
                            .changed()
                        {
                            self.manual_rpm = self.manual_rpm.min(cfg.max_rpm);
                            save = true;
                        }
                        ui.label("Max RPM");
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Slider::new(&mut cfg.num_leds, 1..=16).text("LEDs"))
                            .changed()
                        {
                            save = true;
                        }
                        if ui
                            .add(egui::Slider::new(&mut cfg.start_pct, 0..=99).text("First light %"))
                            .changed()
                        {
                            cfg.end_pct = cfg.end_pct.max(cfg.start_pct + 1);
                            save = true;
                        }
                        if ui
                            .add(egui::Slider::new(&mut cfg.end_pct, 1..=100).text("Full bar %"))
                            .changed()
                        {
                            cfg.start_pct = cfg.start_pct.min(cfg.end_pct.saturating_sub(1));
                            save = true;
                        }
                    });
                    let mut lights_on = self.lights_handle.is_some();
                    ui.horizontal(|ui| {
                        let mut udp = cfg.udp_enabled;
                        if ui.checkbox(&mut udp, "UDP telemetry").changed() {
                            cfg.udp_enabled = udp;
                            save = true;
                        }
                        let mut port = cfg.udp_port as i32;
                        if ui
                            .add(egui::DragValue::new(&mut port).range(1024..=65535))
                            .changed()
                        {
                            cfg.udp_port = port.clamp(1024, 65535) as u16;
                            save = true;
                        }
                        ui.weak("port (restart lights to apply)");
                        let _ = &mut lights_on;
                    });
                    let mut rumble = cfg.rumble_enabled;
                    if ui
                        .checkbox(&mut rumble, "Forward controller rumble to lights")
                        .changed()
                    {
                        cfg.rumble_enabled = rumble;
                        save = true;
                    }
                    if save {
                        if let Some(h) = &self.lights_handle {
                            h.set_config(cfg.clone());
                        }
                        self.config.rpm_lights = cfg;
                        if let Err(e) = self.config.save() {
                            self.config_error = Some(e);
                        }
                    }
                    // Manual RPM value — drives the bar directly.
                    let mut rpm = self.manual_rpm;
                    if ui
                        .add(
                            egui::Slider::new(&mut rpm, 0..=self.config.rpm_lights.max_rpm)
                                .text("Manual RPM"),
                        )
                        .changed()
                    {
                        self.manual_rpm = rpm;
                        if let Some(h) = &self.lights_handle {
                            h.send_rpm(rpm);
                        }
                    }
                    let sweeping = self.sweep_thread.as_ref().is_some_and(|t| !t.is_finished());
                    if sweeping {
                        if ui.button("Stop test sweep").clicked() {
                            self.stop_sweep();
                        }
                    } else if ui.button("Test sweep").clicked() {
                        self.start_sweep();
                    }
                }
            });
            ui.add_space(10.0);

            // Mapping wizard
            ui.group(|ui| {
                ui.label("Mapping wizard");
                if self.wizard_running() {
                    let prompt = self.wizard_prompt.lock().unwrap().clone();
                    if !prompt.is_empty() {
                        ui.colored_label(egui::Color32::YELLOW, &prompt);
                    }
                    let log = self.wizard_log.lock().unwrap();
                    for line in log.iter().rev().take(5) {
                        ui.monospace(line);
                    }
                    if ui.button("Cancel wizard (keep learned so far)").clicked() {
                        if let Some(cancel) = &self.wizard_cancel {
                            cancel.store(true, Ordering::Relaxed);
                        }
                    }
                } else if ui.button("Re-learn button mapping").clicked() {
                    self.wizard_log.lock().unwrap().clear();
                    self.start_wizard();
                }
                // Pick up a finished wizard's result.
                if let Some(mapping) = self.wizard_result.lock().unwrap().take() {
                    self.config.mapping = mapping;
                    if let Err(e) = self.config.save() {
                        self.config_error = Some(e);
                    }
                }
            });
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label(format!("Steering: {:+.3}", s.steer));
                let normalized = ((s.steer.clamp(-1.0, 1.0) + 1.0) / 2.0) as f32;
                ui.add(egui::ProgressBar::new(normalized).desired_width(240.0));
            });

            // Pedals are 0..1 — one convention everywhere.
            for (name, val, color) in [
                ("Throttle", s.throttle, egui::Color32::GREEN),
                ("Brake", s.brake, egui::Color32::RED),
                ("Clutch", s.clutch, egui::Color32::BLUE),
            ] {
                ui.horizontal(|ui| {
                    ui.label(format!("{}: {:.2}", name, val));
                    ui.add(
                        egui::ProgressBar::new(val.clamp(0.0, 1.0) as f32)
                            .desired_width(240.0)
                            .fill(color),
                    );
                });
            }

            ui.add_space(10.0);
            ui.separator();

            // Buttons, 1-based, matching Joystick Gremlin numbering.
            ui.heading("Buttons (1-based, Joystick Gremlin numbering)");
            let pressed = mapping::pressed_moza_buttons(&s);
            if pressed.is_empty() {
                ui.colored_label(egui::Color32::GRAY, "No buttons pressed");
            } else {
                ui.colored_label(
                    egui::Color32::GREEN,
                    format!("Pressed: {:?}", pressed),
                );
            }
            ui.add_space(6.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                for row in 0..4 {
                    ui.horizontal(|ui| {
                        for col in 0..10 {
                            let n = row * 10 + col + 1; // 1-based button number
                            if n > 38 {
                                continue;
                            }
                            let pressed = s.buttons.get(n - 1).copied().unwrap_or(false);
                            let color = if pressed {
                                egui::Color32::GREEN
                            } else {
                                egui::Color32::DARK_GRAY
                            };
                            ui.add(
                                egui::Button::new(egui::RichText::new(n.to_string()).size(11.0))
                                    .fill(color)
                                    .min_size(egui::vec2(34.0, 30.0)),
                            );
                        }
                    });
                }
            });
        });
    }
}

impl Drop for MozaMapperApp {
    fn drop(&mut self) {
        // Cancel the wizard first so its (potentially 30 s) waits return
        // promptly, then join both threads before the process tears down —
        // an unjoined wizard could die mid config-write.
        if let Some(cancel) = &self.wizard_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Some(handle) = self.wizard_thread.take() {
            let _ = handle.join();
        }
        self.stop_ffb_test();
        self.stop_lights();
        self.stop_bridge();
    }
}
