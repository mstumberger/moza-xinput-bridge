use eframe::egui;

use crate::gui::MozaMapperApp;

pub fn show(app: &mut MozaMapperApp, ui: &mut egui::Ui) {
    // FFB settings — both controls autosave on change; a separate
    // Save button was inconsistent (checkbox saved, slider didn't).
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
    // Label above the slider: slider + inline text does not fit 316 px.
    ui.label("Gain (applies on next Start)");
    let slider = egui::Slider::new(&mut app.config.ffb_gain, 0.1..=2.0);
    if ui.add(slider).changed() {
        if let Err(e) = app.config.save() {
            app.config_error = Some(e);
        }
    }
}
