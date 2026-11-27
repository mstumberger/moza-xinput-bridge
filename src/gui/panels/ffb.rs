use eframe::egui;

use crate::gui::widgets::{self, controls};
use crate::gui::MozaMapperApp;

/// Gain slider text: the stored f32 (1.0) shown as a percentage (`100 %`).
fn gain_slider(gain: &mut f32) -> egui::Slider<'_> {
    egui::Slider::new(gain, 0.1..=2.0)
        .custom_formatter(|v, _| format!("{:.0} %", v * 100.0))
        .custom_parser(|s| {
            s.trim()
                .trim_end_matches('%')
                .trim()
                .parse::<f64>()
                .ok()
                .map(|p| p / 100.0)
        })
}

pub fn show(app: &mut MozaMapperApp, ui: &mut egui::Ui) {
    // Both controls autosave on change.
    let mut enabled = app.config.ffb_enabled;
    if widgets::toggle(ui, &mut enabled, "Enable force feedback").changed() {
        app.config.ffb_enabled = enabled;
        save(app);
    }
    let changed = controls::slider_row(ui, "Gain", |ui| ui.add(gain_slider(&mut app.config.ffb_gain)))
        .changed();
    if changed {
        save(app);
    }
    widgets::note(ui, "Changes apply the next time you press Start.");
    widgets::note(
        ui,
        "Games send only rumble to an Xbox controller, so in-game force through the bridge is approximate.",
    );
}

fn save(app: &mut MozaMapperApp) {
    if let Err(e) = app.config.save() {
        app.config_error = Some(e);
    }
}
