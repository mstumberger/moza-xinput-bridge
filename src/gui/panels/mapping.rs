use std::sync::atomic::Ordering;

use eframe::egui;

use crate::gui::{theme, MozaMapperApp};

pub fn show(app: &mut MozaMapperApp, ui: &mut egui::Ui) {
    // Mapping wizard. (A finished wizard's result is picked up every frame
    // in `MozaMapperApp::poll_wizard_result`, not here.)
    if app.wizard_running() {
        let prompt = app.wizard_prompt.lock().unwrap().clone();
        if !prompt.is_empty() {
            ui.colored_label(theme::WARN, &prompt);
        }
        let log = app.wizard_log.lock().unwrap();
        for line in log.iter().rev().take(5) {
            ui.monospace(line);
        }
        if ui.button("Cancel wizard (keep learned so far)").clicked() {
            if let Some(cancel) = &app.wizard_cancel {
                cancel.store(true, Ordering::Relaxed);
            }
        }
    } else if ui.button("Re-learn button mapping").clicked() {
        app.wizard_log.lock().unwrap().clear();
        app.start_wizard();
    }
}
