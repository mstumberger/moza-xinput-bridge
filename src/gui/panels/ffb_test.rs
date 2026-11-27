use std::sync::atomic::Ordering;

use eframe::egui;

use crate::gui::ffb_test::FfbTest;
use crate::gui::widgets::{self, controls, Level};
use crate::gui::{preview, theme, MozaMapperApp};

/// Colour level of an auto pulse verdict (the first word decides).
fn verdict_level(result: &str) -> Level {
    if result.starts_with("OK") {
        Level::Good
    } else if result.starts_with("NO FORCE") || result.starts_with("REVERSED") {
        Level::Bad
    } else {
        Level::Warn
    }
}

/// Steering offset from centre in percent of half travel, from the raw value.
fn offset_pct(raw: u32) -> f32 {
    (raw as i32 - 32768) as f32 / 327.68
}

pub fn show(app: &mut MozaMapperApp, ui: &mut egui::Ui) {
    // FFB manual test: decoded vendor torque protocol.
    controls::error_line(ui, "FFB test", &mut app.ffb_test_error);
    let mut close_ffb_test = false;
    if app.ffb_test.is_none() && app.preview {
        // Preview only: lay out the open state against an inert dummy.
        let t = app.preview_start.elapsed().as_secs_f64();
        let test = app.preview_ffb_test.get_or_insert_with(preview::dummy_ffb_test);
        test.steer.store(preview::raw_steer_at(t), Ordering::Relaxed);
        let mut ignored = false; // closing does nothing in the preview
        show_open(test, ui, &mut ignored);
    } else if let Some(test) = &mut app.ffb_test {
        show_open(test, ui, &mut close_ffb_test);
    } else {
        ui.add(
            egui::Label::new(
                "Push the wheel directly to check the motor. Stop the bridge first for a clean reading.",
            )
            .wrap(),
        );
        if ui.button("Open FFB test").clicked() {
            app.start_ffb_test();
        }
    }
    if close_ffb_test {
        app.stop_ffb_test();
    }
}

fn show_open(test: &mut FfbTest, ui: &mut egui::Ui, close: &mut bool) {
    controls::slider_row(ui, "Strength", |ui| {
        ui.add(egui::Slider::new(&mut test.strength_pct, 5.0..=100.0).suffix(" %"))
    });
    let strength = test.strength_pct;
    test.pulse_strength.store(strength.to_bits(), Ordering::Relaxed);
    let cmd = test.torque_cmd.clone();
    let magnitude = (strength / 100.0 * 32767.0) as i16;

    // Two hold buttons side by side: 150x40, or as wide as fits equally.
    let gap = ui.spacing().item_spacing.x;
    let w = ((ui.available_width() - gap) / 2.0).floor().min(150.0);
    let (left, right) = ui
        .horizontal(|ui| {
            let l = ui
                .add(
                    egui::Button::new("Push left")
                        .min_size(egui::vec2(w, 40.0))
                        .sense(egui::Sense::click_and_drag()),
                )
                .is_pointer_button_down_on();
            let r = ui
                .add(
                    egui::Button::new("Push right")
                        .min_size(egui::vec2(w, 40.0))
                        .sense(egui::Sense::click_and_drag()),
                )
                .is_pointer_button_down_on();
            (l, r)
        })
        .inner;
    // Positive torque turns the wheel LEFT (verified on hardware).
    *cmd.lock().unwrap() = if left {
        magnitude
    } else if right {
        -magnitude
    } else {
        0
    };

    steering_bar(ui, test.steer.load(Ordering::Relaxed));

    if ui.button("Auto pulse test").clicked() {
        test.pulse_req.store(true, Ordering::Relaxed);
        *test.pulse_result.lock().unwrap() = "starting...".into();
    }
    widgets::note(ui, "Hands off the wheel.");

    let res = test.pulse_result.lock().unwrap().clone();
    if !res.is_empty() {
        let mut lines = res.lines();
        let title = lines.next().unwrap_or("");
        let rest: Vec<String> = lines.map(str::to_owned).collect();
        widgets::result_card(ui, verdict_level(&res), title, &rest);
    }

    if ui.button("Close FFB test").clicked() {
        *close = true;
    }
}

/// Thin horizontal bar with a centre mark and the offset in percent.
fn steering_bar(ui: &mut egui::Ui, raw: u32) {
    if raw == u32::MAX {
        widgets::note(ui, "Steering: no data");
        return;
    }
    let pct = offset_pct(raw);
    ui.horizontal(|ui| {
        let text_w = 56.0;
        let width = (ui.available_width() - text_w - ui.spacing().item_spacing.x).max(40.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 10.0), egui::Sense::hover());
        let p = ui.painter();
        let radius = egui::CornerRadius::same(5);
        p.rect_filled(rect, radius, theme::BG);
        p.rect_stroke(rect, radius, egui::Stroke::new(1.0, theme::OUTLINE), egui::StrokeKind::Inside);
        let cx = rect.center().x;
        let x = cx + (pct / 100.0).clamp(-1.0, 1.0) * rect.width() / 2.0;
        let fill = egui::Rect::from_x_y_ranges(cx.min(x)..=cx.max(x), rect.y_range())
            .shrink2(egui::vec2(0.0, 2.0));
        p.rect_filled(fill, egui::CornerRadius::same(2), theme::ACCENT);
        p.line_segment(
            [egui::pos2(cx, rect.top() - 2.0), egui::pos2(cx, rect.bottom() + 2.0)],
            egui::Stroke::new(1.5, theme::TEXT),
        );
        ui.label(egui::RichText::new(format!("{pct:+.1} %")).monospace().color(theme::TEXT_MUTED));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_levels() {
        assert_eq!(verdict_level("OK: both ways"), Level::Good);
        assert_eq!(verdict_level("NO FORCE: nothing moved"), Level::Bad);
        assert_eq!(verdict_level("REVERSED: swapped"), Level::Bad);
        assert_eq!(verdict_level("MOVED, but directions unclear"), Level::Warn);
        assert_eq!(verdict_level("starting..."), Level::Warn);
    }

    #[test]
    fn offset_is_percent_of_half_travel() {
        assert_eq!(offset_pct(32768), 0.0);
        assert!((offset_pct(65535) - 100.0).abs() < 0.01);
        assert!((offset_pct(0) + 100.0).abs() < 0.01);
    }
}
