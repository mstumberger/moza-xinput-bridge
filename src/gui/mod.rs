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

        // Per-frame logic that must not depend on which section is open.
        self.poll_wizard_result();

        panels::top_bar::show(self, ctx);
        self.error_banner(ctx);
        self.right_column(ctx);

        let central = egui::Frame::new()
            .fill(theme::BG)
            .inner_margin(egui::Margin::same(theme::COLUMN_PADDING as i8));
        egui::CentralPanel::default().frame(central).show(ctx, |ui| {
            panels::left_live::show(self, ui);
        });
    }
}

impl MozaMapperApp {
    /// Red banner under the top bar for config and bridge errors (spec 4.2).
    fn error_banner(&mut self, ctx: &egui::Context) {
        let last_error = self.state.last_error.lock().unwrap().clone();
        if self.config_error.is_none() && last_error.is_none() {
            return;
        }
        let frame = egui::Frame::new()
            .fill(theme::BAD.gamma_multiply(0.15))
            .stroke(egui::Stroke::new(1.0, theme::BAD.gamma_multiply(0.6)))
            .inner_margin(egui::Margin::symmetric(theme::COLUMN_PADDING as i8, 8));
        egui::TopBottomPanel::top("error_banner")
            .frame(frame)
            .show(ctx, |ui| {
                if let Some(err) = &self.config_error {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("Config error: {}", err))
                                .color(theme::BAD),
                        )
                        .wrap(),
                    );
                }
                if let Some(err) = last_error {
                    ui.horizontal_top(|ui| {
                        if ui.small_button("\u{00d7}").clicked() {
                            *self.state.last_error.lock().unwrap() = None;
                        }
                        ui.add(
                            egui::Label::new(egui::RichText::new(err).color(theme::BAD)).wrap(),
                        );
                    });
                }
            });
    }

    /// Fixed-width right column with the accordion (spec 4, 5).
    fn right_column(&mut self, ctx: &egui::Context) {
        let ffb_status = if self.config.ffb_enabled { "On" } else { "Off" };
        let test_status = if self.ffb_test.is_some() { "Open" } else { "" };
        let lights_status = if self.config.rpm_lights.enabled { "On" } else { "Off" };

        let mut open = self.config.ui.open_section.clone();
        let mut changed = false;
        let frame = egui::Frame::new()
            .fill(theme::PANEL)
            .stroke(egui::Stroke::new(1.0, theme::BORDER))
            .inner_margin(egui::Margin::same(8));
        egui::SidePanel::right("sections")
            .exact_width(theme::RIGHT_COLUMN_WIDTH)
            .resizable(false)
            .frame(frame)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        changed |= widgets::section(ui, "ffb", "Force feedback", ffb_status, &mut open, |ui| {
                            panels::ffb::show(self, ui)
                        });
                        changed |= widgets::section(ui, "ffb_test", "FFB test", test_status, &mut open, |ui| {
                            panels::ffb_test::show(self, ui, ctx)
                        });
                        changed |= widgets::section(ui, "lights", "Shift lights", lights_status, &mut open, |ui| {
                            panels::lights::show(self, ui)
                        });
                        changed |= widgets::section(ui, "mapping", "Button mapping", "", &mut open, |ui| {
                            panels::mapping::show(self, ui)
                        });
                        changed |= widgets::section(ui, "diagnostics", "Diagnostics", "", &mut open, |ui| {
                            panels::diagnostics::show(self, ui)
                        });
                    });
            });

        // Safety: the push buttons only run while their body is drawn. If the
        // section is collapsed, make sure no held command is left behind.
        if open != "ffb_test" {
            if let Some(test) = &self.ffb_test {
                *test.torque_cmd.lock().unwrap() = 0;
            }
        }

        if changed {
            self.config.ui.open_section = open;
            if let Err(e) = self.config.save() {
                self.config_error = Some(e);
            }
        }
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
