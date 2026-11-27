use std::process::Command;

use eframe::egui;

use crate::gui::widgets::{self, controls};
use crate::gui::{preview, theme, MozaMapperApp};
use crate::mapping;

const PROJECT_URL: &str = "https://github.com/mstumberger/moza-xinput-bridge";

/// Text for the line above the raw grid.
fn pressed_line(pressed: &[u8]) -> String {
    if pressed.is_empty() {
        "No buttons pressed".to_owned()
    } else {
        let list: Vec<String> = pressed.iter().map(u8::to_string).collect();
        format!("Pressed: {}", list.join(", "))
    }
}

/// Round a steering range to the 10 degree step and clamp to 180..=2520.
fn snap_range(deg: u16) -> u16 {
    (((deg as u32 + 5) / 10 * 10) as u16).clamp(180, 2520)
}

pub fn show(app: &mut MozaMapperApp, ui: &mut egui::Ui) {
    let s = if app.preview {
        preview::state_at(app.preview_start.elapsed().as_secs_f64())
    } else {
        app.state.current_state.lock().unwrap().clone()
    };

    // Buttons, 1-based, matching Joystick Gremlin numbering.
    let pressed = mapping::pressed_moza_buttons(&s);
    let color = if pressed.is_empty() { theme::TEXT_MUTED } else { theme::ACCENT };
    ui.colored_label(color, pressed_line(&pressed));
    raw_grid(ui, &s.buttons);

    ui.add_space(4.0);
    let mut deg = app.config.ui.steering_range_deg;
    let changed = controls::value_row(ui, "Steering range", |ui| {
        ui.add(egui::DragValue::new(&mut deg).range(180..=2520).speed(10.0).suffix(" \u{00b0}"))
    })
    .changed();
    if changed {
        app.config.ui.steering_range_deg = snap_range(deg);
        if let Err(e) = app.config.save() {
            app.config_error = Some(e);
        }
    }
    widgets::note(ui, "Only affects how far the drawn wheel turns.");

    ui.add_space(4.0);
    let fps = *app.state.fps.lock().unwrap();
    let total = *app.state.report_count.lock().unwrap();
    ui.label(format!("Report rate: {fps:.0} Hz"));
    ui.label(format!("Total reports: {total}"));

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button("Open log file").clicked() {
            let path = std::env::temp_dir().join("moza_mapper.log");
            open_path(app, path.as_os_str());
        }
        if ui.button("Open config folder").clicked() {
            let dir = crate::config::config_dir();
            open_path(app, dir.as_os_str());
        }
    });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        widgets::note(ui, &format!("Version {}", env!("CARGO_PKG_VERSION")));
        ui.hyperlink_to("Project page", PROJECT_URL);
    });
}

/// Buttons 1 to 38, ten per row, 26x24 cells, accent when pressed.
fn raw_grid(ui: &mut egui::Ui, buttons: &[bool]) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
        for row in 0..4 {
            ui.horizontal(|ui| {
                for col in 0..10 {
                    let n = row * 10 + col + 1; // 1-based button number
                    if n > 38 {
                        continue;
                    }
                    let pressed = buttons.get(n - 1).copied().unwrap_or(false);
                    let (fill, text) = if pressed {
                        (theme::ACCENT, theme::ACCENT_TEXT)
                    } else {
                        (theme::BG, theme::TEXT)
                    };
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(26.0, 24.0), egui::Sense::hover());
                    let radius = egui::CornerRadius::same(4);
                    let p = ui.painter();
                    p.rect_filled(rect, radius, fill);
                    p.rect_stroke(
                        rect,
                        radius,
                        egui::Stroke::new(1.0, if pressed { theme::ACCENT } else { theme::BORDER }),
                        egui::StrokeKind::Inside,
                    );
                    p.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        n.to_string(),
                        egui::FontId::proportional(11.0),
                        text,
                    );
                }
            });
        }
    });
}

/// Open a file or folder with Explorer (default handler); failures surface in
/// the error banner.
fn open_path(app: &MozaMapperApp, path: &std::ffi::OsStr) {
    if let Err(e) = Command::new("explorer.exe").arg(path).spawn() {
        app.state.set_error(format!("Could not open {}: {e}", path.to_string_lossy()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressed_line_text() {
        assert_eq!(pressed_line(&[]), "No buttons pressed");
        assert_eq!(pressed_line(&[3, 17]), "Pressed: 3, 17");
    }

    #[test]
    fn range_snaps_and_clamps() {
        assert_eq!(snap_range(904), 900);
        assert_eq!(snap_range(905), 910);
        assert_eq!(snap_range(100), 180);
        assert_eq!(snap_range(9000), 2520);
    }
}
