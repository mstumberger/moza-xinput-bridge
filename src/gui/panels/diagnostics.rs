use eframe::egui;

use crate::gui::{theme, MozaMapperApp};
use crate::mapping;

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

    ui.add_space(10.0);
    ui.separator();

    // Buttons, 1-based, matching Joystick Gremlin numbering.
    ui.heading("Buttons (1-based, Joystick Gremlin numbering)");
    let pressed = mapping::pressed_moza_buttons(&s);
    if pressed.is_empty() {
        ui.colored_label(theme::TEXT_MUTED, "No buttons pressed");
    } else {
        ui.colored_label(
            theme::ACCENT,
            format!("Pressed: {:?}", pressed),
        );
    }
    ui.add_space(6.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        for row in 0..4 {
            ui.horizontal(|ui| {
                for col in 0..10 {
                    let n = row * 10 + col + 1; // 1-based button number
                    if n > 38 {
                        continue;
                    }
                    let pressed = s.buttons.get(n - 1).copied().unwrap_or(false);
                    let color = if pressed {
                        theme::ACCENT
                    } else {
                        theme::SURFACE
                    };
                    ui.add(
                        egui::Button::new(egui::RichText::new(n.to_string()).size(11.0))
                            .fill(color)
                            .min_size(egui::vec2(34.0, 30.0)),
                    );
                }
            });
        }
    });
}
