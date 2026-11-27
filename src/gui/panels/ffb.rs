use eframe::egui;

use crate::gui::MozaMapperApp;

pub fn show(app: &mut MozaMapperApp, ui: &mut egui::Ui) {
    // FFB settings — both controls autosave on change; a separate
    // Save button was inconsistent (checkbox saved, slider didn't).
    ui.group(|ui| {
        ui.label("Force feedback");
        ui.horizontal(|ui| {
            let mut enabled = app.config.ffb_enabled;
            if ui
                .checkbox(&mut enabled, "Enable FFB (applies on next Start)")
                .changed()
            {
                app.config.ffb_enabled = enabled;
                if let Err(e) = app.config.save() {
                    app.config_error = Some(e);
                }
            }
            let slider = egui::Slider::new(&mut app.config.ffb_gain, 0.1..=2.0)
                .text("Gain (applies on next Start)");
            if ui.add(slider).changed() {
                if let Err(e) = app.config.save() {
                    app.config_error = Some(e);
                }
            }
        });
    });
}
