use std::sync::atomic::Ordering;

use eframe::egui;

use crate::gui::MozaMapperApp;

pub fn show(app: &mut MozaMapperApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("header").show(ctx, |ui| {
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Moza Mapper")
                    .size(22.0)
                    .color(egui::Color32::LIGHT_BLUE),
            );
            ui.separator();

            let connected = *app.state.device_connected.lock().unwrap();
            let (text, color) = if connected {
                ("Connected", egui::Color32::GREEN)
            } else {
                ("Disconnected", egui::Color32::GRAY)
            };
            ui.label(egui::RichText::new(text).color(color).size(15.0));
            ui.separator();

            let fps = *app.state.fps.lock().unwrap() as u32;
            let count = *app.state.report_count.lock().unwrap();
            ui.label(
                egui::RichText::new(format!("{} Hz | {} reports", fps, count))
                    .color(egui::Color32::YELLOW)
                    .size(13.0),
            );

            let ffb_on = app.state.ffb_active.load(Ordering::Relaxed);
            let (ffb_text, ffb_color) = if ffb_on {
                ("FFB on", egui::Color32::GREEN)
            } else {
                ("FFB off", egui::Color32::GRAY)
            };
            ui.separator();
            ui.label(egui::RichText::new(ffb_text).color(ffb_color).size(13.0));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let wizard_active = app.wizard_running();
                let (button_text, color) = if app.running {
                    ("Stop", egui::Color32::RED)
                } else {
                    ("Start", egui::Color32::GREEN)
                };
                let mut btn = egui::Button::new(egui::RichText::new(button_text).size(15.0));
                if app.running {
                    btn = btn.fill(color);
                } else if wizard_active {
                    // Greyed out: the wizard owns the HID device.
                    btn = btn.fill(egui::Color32::DARK_GRAY);
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
