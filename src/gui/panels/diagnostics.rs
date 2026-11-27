use eframe::egui;

use crate::gui::{theme, MozaMapperApp};
use crate::mapping;

pub fn show(app: &mut MozaMapperApp, ui: &mut egui::Ui) {
    let s = app.state.current_state.lock().unwrap().clone();

    // Buttons, 1-based, matching Joystick Gremlin numbering.
    ui.label("Buttons (1-based, Joystick Gremlin numbering)");
    let pressed = mapping::pressed_moza_buttons(&s);
    if pressed.is_empty() {
        ui.colored_label(theme::TEXT_MUTED, "No buttons pressed");
    } else {
        ui.colored_label(theme::ACCENT, format!("Pressed: {:?}", pressed));
    }

    ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
    for row in 0..4 {
        ui.horizontal(|ui| {
            for col in 0..10 {
                let n = row * 10 + col + 1; // 1-based button number
                if n > 38 {
                    continue;
                }
                let pressed = s.buttons.get(n - 1).copied().unwrap_or(false);
                let (fill, text) = if pressed {
                    (theme::ACCENT, theme::ACCENT_TEXT)
                } else {
                    (theme::BG, theme::TEXT)
                };
                ui.add(
                    egui::Button::new(egui::RichText::new(n.to_string()).size(11.0).color(text))
                        .fill(fill)
                        .min_size(egui::vec2(26.0, 24.0)),
                );
            }
        });
    }
}
