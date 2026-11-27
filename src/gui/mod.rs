//! egui GUI for the MOZA R3 → ViGEm XInput bridge.

mod bridge;
mod ffb_test;
mod lights;
mod panels;
mod state;
mod theme;
mod widgets;
mod wizard;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use eframe::egui;

use crate::config::AppConfig;

use ffb_test::FfbTest;
pub use state::AppState;

pub struct MozaMapperApp {
    pub(crate) state: AppState,
    pub(crate) running: bool,
    pub(crate) config: AppConfig,
    pub(crate) config_error: Option<String>,
    pub(crate) bridge_handle: Option<thread::JoinHandle<()>>,
    pub(crate) stop_flag: Option<Arc<AtomicBool>>,
    // Mapping wizard (runs on its own thread, drives the GUI via shared state)
    pub(crate) wizard_prompt: Arc<Mutex<String>>,
    pub(crate) wizard_log: Arc<Mutex<Vec<String>>>,
    pub(crate) wizard_result: Arc<Mutex<Option<crate::config::MappingConfig>>>,
    pub(crate) wizard_thread: Option<thread::JoinHandle<()>>,
    pub(crate) wizard_cancel: Option<Arc<AtomicBool>>,
    // FFB manual test channel (vendor torque protocol, decoded from USBPcap)
    pub(crate) ffb_test: Option<FfbTest>,
    pub(crate) ffb_test_error: Option<String>,
    // RPM shift lights (serial LED-bar channel, decoded from USBPcap)
    pub(crate) lights: Option<crate::rpm::LightsController>,
    pub(crate) lights_handle: Option<Arc<crate::rpm::LightsHandle>>,
    pub(crate) lights_error: Option<String>,
    pub(crate) manual_rpm: u16,
    pub(crate) sweep_thread: Option<thread::JoinHandle<()>>,
    pub(crate) sweep_run: Option<Arc<AtomicBool>>,
    pub(crate) udp_thread: Option<thread::JoinHandle<()>>,
    pub(crate) udp_run: Option<Arc<AtomicBool>>,
}

impl MozaMapperApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
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

        panels::top_bar::show(self, ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            // Surface config errors — never swallow them.
            if let Some(err) = &self.config_error {
                ui.colored_label(theme::BAD, format!("Config error: {}", err));
                ui.separator();
            }
            if let Some(err) = self.state.last_error.lock().unwrap().clone() {
                ui.horizontal(|ui| {
                    ui.colored_label(theme::BAD, &err);
                    if ui.small_button("×").clicked() {
                        *self.state.last_error.lock().unwrap() = None;
                    }
                });
                ui.separator();
            }

            panels::ffb::show(self, ui);
            ui.add_space(10.0);

            panels::ffb_test::show(self, ui, ctx);
            ui.add_space(10.0);

            panels::lights::show(self, ui);
            ui.add_space(10.0);

            panels::mapping::show(self, ui);
            ui.add_space(10.0);

            panels::diagnostics::show(self, ui);
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
