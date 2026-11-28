//! Pedal bars (spec 4.4): vertical next to the wheel, horizontal when narrow.

use eframe::egui::{self, Align2, FontId, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::gui::theme;

pub const BAR_WIDTH: f32 = 28.0;
pub const BAR_GAP: f32 = 14.0;
/// Total width of the vertical pedal group.
pub const GROUP_WIDTH: f32 = 3.0 * BAR_WIDTH + 2.0 * BAR_GAP;
const TEXT_H: f32 = 16.0;
/// Height of the horizontal block (3 rows of 18 plus 2 gaps of 6).
pub const HORIZONTAL_HEIGHT: f32 = 66.0;

/// (label, colour) per pedal, in THR / BRK / CLU order.
fn pedals() -> [(&'static str, egui::Color32); 3] {
    [("THR", theme::ACCENT), ("BRK", theme::BAD), ("CLU", theme::INFO)]
}

/// Whole-percent text for a 0..1 value.
pub fn percent(v: f64) -> String {
    format!("{}%", (v.clamp(0.0, 1.0) * 100.0).round() as u32)
}

/// Height of the vertical group for a given bar height.
pub fn vertical_height(bar_h: f32) -> f32 {
    bar_h + 2.0 * TEXT_H
}

/// Three vertical bars: value above, label below. `values` = THR, BRK, CLU.
pub fn vertical(ui: &mut egui::Ui, values: [f64; 3], bar_h: f32) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(GROUP_WIDTH, vertical_height(bar_h)), Sense::hover());
    let p = ui.painter_at(rect);
    let small = FontId::proportional(theme::SIZE_SMALL);
    let radius = theme::RADIUS as f32 / 2.0;
    for (i, ((label, color), value)) in pedals().into_iter().zip(values).enumerate() {
        let x = rect.left() + i as f32 * (BAR_WIDTH + BAR_GAP);
        let track =
            Rect::from_min_size(egui::pos2(x, rect.top() + TEXT_H), Vec2::new(BAR_WIDTH, bar_h));
        p.rect_filled(track, radius, theme::SURFACE);
        let fill_h = bar_h * value.clamp(0.0, 1.0) as f32;
        let fill = Rect::from_min_max(egui::pos2(track.left(), track.bottom() - fill_h), track.max);
        p.rect_filled(fill, radius, color);
        p.rect_stroke(track, radius, Stroke::new(1.0_f32, theme::BORDER), StrokeKind::Inside);
        let cx = track.center().x;
        p.text(
            egui::pos2(cx, rect.top() + TEXT_H / 2.0),
            Align2::CENTER_CENTER,
            percent(value),
            small.clone(),
            theme::TEXT,
        );
        p.text(
            egui::pos2(cx, track.bottom() + TEXT_H / 2.0 + 2.0),
            Align2::CENTER_CENTER,
            label,
            small.clone(),
            theme::TEXT_MUTED,
        );
    }
}

/// Three horizontal bars stacked: label, bar, value.
pub fn horizontal(ui: &mut egui::Ui, values: [f64; 3], width: f32) {
    const ROW_H: f32 = 18.0;
    const ROW_GAP: f32 = 6.0;
    const LABEL_W: f32 = 36.0;
    const VALUE_W: f32 = 44.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, HORIZONTAL_HEIGHT), Sense::hover());
    let p = ui.painter_at(rect);
    let small = FontId::proportional(theme::SIZE_SMALL);
    for (i, ((label, color), value)) in pedals().into_iter().zip(values).enumerate() {
        let y = rect.top() + i as f32 * (ROW_H + ROW_GAP);
        let row = Rect::from_min_size(egui::pos2(rect.left(), y), Vec2::new(width, ROW_H));
        let track = Rect::from_min_max(
            egui::pos2(row.left() + LABEL_W, row.top() + 3.0),
            egui::pos2(row.right() - VALUE_W, row.bottom() - 3.0),
        );
        p.text(
            egui::pos2(row.left(), row.center().y),
            Align2::LEFT_CENTER,
            label,
            small.clone(),
            theme::TEXT_MUTED,
        );
        p.rect_filled(track, 3.0, theme::SURFACE);
        let fill_w = track.width() * value.clamp(0.0, 1.0) as f32;
        p.rect_filled(
            Rect::from_min_size(track.min, Vec2::new(fill_w, track.height())),
            3.0,
            color,
        );
        p.rect_stroke(track, 3.0, Stroke::new(1.0_f32, theme::BORDER), StrokeKind::Inside);
        p.text(
            egui::pos2(row.right(), row.center().y),
            Align2::RIGHT_CENTER,
            percent(value),
            small.clone(),
            theme::TEXT,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_rounds_and_clamps() {
        assert_eq!(percent(0.0), "0%");
        assert_eq!(percent(0.456), "46%");
        assert_eq!(percent(1.7), "100%");
        assert_eq!(percent(-0.2), "0%");
    }
}
