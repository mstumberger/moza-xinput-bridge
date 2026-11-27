//! Temporary left column: the old live steering and pedal bars.
//! Replaced by the wheel view in a later task.

use eframe::egui;

use crate::gui::{theme, MozaMapperApp};

pub fn show(app: &mut MozaMapperApp, ui: &mut egui::Ui) {
    let s = app.state.current_state.lock().unwrap().clone();

    ui.horizontal(|ui| {
        ui.label(format!("Steering: {:+.3}", s.steer));
        let normalized = ((s.steer.clamp(-1.0, 1.0) + 1.0) / 2.0) as f32;
        ui.add(egui::ProgressBar::new(normalized).desired_width(240.0));
    });

    // Pedals are 0..1 — one convention everywhere.
    for (name, val, color) in [
        ("Throttle", s.throttle, theme::ACCENT),
        ("Brake", s.brake, theme::BAD),
        ("Clutch", s.clutch, theme::INFO),
    ] {
        ui.horizontal(|ui| {
            ui.label(format!("{}: {:.2}", name, val));
            ui.add(
                egui::ProgressBar::new(val.clamp(0.0, 1.0) as f32)
                    .desired_width(240.0)
                    .fill(color),
            );
        });
    }
}
