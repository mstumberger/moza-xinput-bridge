use std::sync::atomic::Ordering;

use eframe::egui;

use crate::gui::widgets::{status_pill, Level};
use crate::gui::{theme, MozaMapperApp};

pub fn show(app: &mut MozaMapperApp, ctx: &egui::Context) {
    let frame = egui::Frame::new()
        .fill(theme::PANEL)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .inner_margin(egui::Margin::symmetric(theme::COLUMN_PADDING as i8, 0));
    egui::TopBottomPanel::top("header")
        .exact_height(theme::TOP_BAR_HEIGHT)
        .frame(frame)
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("MOZA XInput Bridge")
                        .size(theme::SIZE_HEADING)
                        .color(theme::TEXT),
                );
                ui.add_space(8.0);

                let connected = *app.state.device_connected.lock().unwrap();
                let (wheel_text, wheel_level) = if connected {
                    ("Connected", Level::Good)
                } else {
                    ("Not found", Level::Neutral)
                };
                status_pill(ui, "Wheel", wheel_text, wheel_level);
                // Task 4 wires this to the environment probe.
                status_pill(ui, "Driver", "Checking", Level::Neutral);

                let ffb_active = app.state.ffb_active.load(Ordering::Relaxed);
                let (ffb_text, ffb_level) = if !app.config.ffb_enabled {
                    ("Off", Level::Neutral)
                } else if !app.running {
                    ("Standby", Level::Neutral)
                } else if ffb_active {
                    ("Active", Level::Good)
                } else {
                    ("Failed", Level::Bad)
                };
                status_pill(ui, "FFB", ffb_text, ffb_level);

                let (lights_text, lights_level) = if app.lights_error.is_some() {
                    ("Error", Level::Bad)
                } else if app.lights.is_some() {
                    ("On", Level::Good)
                } else {
                    ("Off", Level::Neutral)
                };
                status_pill(ui, "Lights", lights_text, lights_level);

                if app.running {
                    let fps = *app.state.fps.lock().unwrap() as u32;
                    ui.label(
                        egui::RichText::new(format!("{} Hz", fps))
                            .size(theme::SIZE_SMALL)
                            .color(theme::TEXT_MUTED),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    start_stop_button(app, ui);
                });
            });
        });
}

fn start_stop_button(app: &mut MozaMapperApp, ui: &mut egui::Ui) {
    // Disabled while the wizard runs: it owns the HID device.
    let blocked = !app.running && app.wizard_running();
    let (label, fill, text_color) = if app.running {
        ("Stop", theme::BAD, theme::TEXT)
    } else if blocked {
        ("Start", theme::SURFACE, theme::TEXT_MUTED)
    } else {
        ("Start", theme::ACCENT, theme::ACCENT_TEXT)
    };
    let btn = egui::Button::new(
        egui::RichText::new(label)
            .size(theme::SIZE_SECTION_TITLE)
            .strong()
            .color(text_color),
    )
    .fill(fill)
    .min_size(egui::vec2(110.0, 36.0));
    let response = ui
        .add_enabled(!blocked, btn)
        .on_disabled_hover_text("Finish or cancel the mapping wizard first");
    if response.clicked() {
        if app.running {
            app.stop_bridge();
        } else {
            app.start_bridge();
        }
    }
}
