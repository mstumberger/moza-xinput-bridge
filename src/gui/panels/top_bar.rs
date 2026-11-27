use std::sync::atomic::Ordering;

use eframe::egui;

use crate::gui::{theme, MozaMapperApp};

pub fn show(app: &mut MozaMapperApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("header").show(ctx, |ui| {
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("MOZA XInput Bridge")
                    .size(22.0)
                    .color(theme::TEXT),
            );
            ui.separator();

            let connected = *app.state.device_connected.lock().unwrap();
            let (text, color) = if connected {
                ("Connected", theme::ACCENT)
            } else {
                ("Disconnected", theme::TEXT_MUTED)
            };
            ui.label(egui::RichText::new(text).color(color).size(15.0));
            ui.separator();

            let fps = *app.state.fps.lock().unwrap() as u32;
            let count = *app.state.report_count.lock().unwrap();
            ui.label(
                egui::RichText::new(format!("{} Hz | {} reports", fps, count))
                    .color(theme::WARN)
                    .size(13.0),
            );

            let ffb_on = app.state.ffb_active.load(Ordering::Relaxed);
            let (ffb_text, ffb_color) = if ffb_on {
                ("FFB on", theme::ACCENT)
            } else {
                ("FFB off", theme::TEXT_MUTED)
            };
            ui.separator();
            ui.label(egui::RichText::new(ffb_text).color(ffb_color).size(13.0));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let wizard_active = app.wizard_running();
                let (button_text, color) = if app.running {
                    ("Stop", theme::BAD)
                } else {
                    ("Start", theme::ACCENT)
                };
                let text_color = if app.running {
                    theme::TEXT
                } else if wizard_active {
                    theme::TEXT_MUTED
                } else {
                    theme::ACCENT_TEXT
                };
                let mut btn = egui::Button::new(
                    egui::RichText::new(button_text).size(15.0).color(text_color),
                );
                if app.running {
                    btn = btn.fill(color);
                } else if wizard_active {
                    // Greyed out: the wizard owns the HID device.
                    btn = btn.fill(theme::SURFACE);
                } else {
                    btn = btn.fill(color);
                }
                let response = ui.add(btn);
                let clickable = !(!app.running && wizard_active);
                if response.clicked() && clickable {
                    if app.running {
                        app.stop_bridge();
                    } else {
                        app.start_bridge();
                    }
                }
            });
        });
        ui.add_space(6.0);
    });
}
