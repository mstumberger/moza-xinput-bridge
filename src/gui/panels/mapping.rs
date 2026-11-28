use std::sync::atomic::Ordering;

use eframe::egui;

use crate::config::XBOX_ORDER;
use crate::gui::{theme, MozaMapperApp};

/// Display name for a canonical `XBOX_ORDER` name.
pub fn display_name(xbox: &str) -> &str {
    match xbox {
        "DPAD_UP" => "D-pad up",
        "DPAD_RIGHT" => "D-pad right",
        "DPAD_DOWN" => "D-pad down",
        "DPAD_LEFT" => "D-pad left",
        other => other,
    }
}

pub fn show(app: &mut MozaMapperApp, ui: &mut egui::Ui) {
    // Mapping wizard. (A finished wizard's result is picked up every frame
    // in `MozaMapperApp::poll_wizard_result`, not here.)
    if app.wizard_running() {
        let prompt = app.wizard_prompt.lock().unwrap().clone();
        if !prompt.is_empty() {
            ui.add(egui::Label::new(egui::RichText::new(&prompt).color(theme::ACCENT)).wrap());
        }
        {
            let log = app.wizard_log.lock().unwrap();
            for line in log.iter().rev().take(5) {
                ui.monospace(line);
            }
        }
        if ui.button("Cancel (keep what was learned)").clicked() {
            if let Some(cancel) = &app.wizard_cancel {
                cancel.store(true, Ordering::Relaxed);
            }
        }
        return;
    }

    egui::Grid::new("mapping_table")
        .num_columns(4)
        .spacing(egui::vec2(10.0, 4.0))
        .min_col_width(40.0)
        .show(ui, |ui| {
            for (i, name) in XBOX_ORDER.iter().enumerate() {
                ui.label(display_name(name));
                let n = app.config.mapping.lookup(name);
                if n == 0 {
                    ui.label(egui::RichText::new("not set").color(theme::TEXT_MUTED));
                } else {
                    ui.label(egui::RichText::new(n.to_string()).color(theme::ACCENT));
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    ui.add_space(4.0);
    let relearn = ui
        .add_enabled(!app.running, egui::Button::new("Re-learn mapping"))
        .on_disabled_hover_text("Stop the bridge first");
    if relearn.clicked() {
        app.wizard_log.lock().unwrap().clear();
        app.start_wizard();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names() {
        assert_eq!(display_name("DPAD_UP"), "D-pad up");
        assert_eq!(display_name("DPAD_RIGHT"), "D-pad right");
        assert_eq!(display_name("DPAD_DOWN"), "D-pad down");
        assert_eq!(display_name("DPAD_LEFT"), "D-pad left");
        for n in ["A", "B", "X", "Y", "LB", "RB", "Guide", "Back", "Start", "LS", "RS"] {
            assert_eq!(display_name(n), n);
        }
        // Every canonical name has a display name without underscores.
        assert!(XBOX_ORDER.iter().all(|n| !display_name(n).contains('_')));
    }
}
