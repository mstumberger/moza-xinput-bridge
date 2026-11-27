//! Shared widgets (spec 6.3). Icons are drawn with the painter.

use eframe::egui::{self, Color32, CornerRadius, Painter, Pos2, Sense, Shape, Stroke, StrokeKind, Vec2};

use crate::gui::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Good,
    Neutral,
    #[allow(dead_code)]
    Warn,
    Bad,
}

impl Level {
    /// Foreground colour for icons and text of this level.
    pub fn color(self) -> Color32 {
        match self {
            Level::Good => theme::ACCENT,
            Level::Neutral => theme::TEXT_MUTED,
            Level::Warn => theme::WARN,
            Level::Bad => theme::BAD,
        }
    }
}

// ---------------------------------------------------------------- icons

#[allow(dead_code)]
pub fn icon_dot(painter: &Painter, center: Pos2, radius: f32, color: Color32) {
    painter.circle_filled(center, radius, color);
}

#[allow(dead_code)]
pub fn icon_check(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let h = size / 2.0;
    let pts = vec![
        center + Vec2::new(-h * 0.8, 0.0),
        center + Vec2::new(-h * 0.2, h * 0.6),
        center + Vec2::new(h * 0.8, -h * 0.6),
    ];
    painter.add(Shape::line(pts, Stroke::new(2.0, color)));
}

#[allow(dead_code)]
pub fn icon_cross(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let h = size * 0.35;
    let stroke = Stroke::new(2.0, color);
    painter.line_segment([center + Vec2::new(-h, -h), center + Vec2::new(h, h)], stroke);
    painter.line_segment([center + Vec2::new(-h, h), center + Vec2::new(h, -h)], stroke);
}

#[allow(dead_code)]
pub fn icon_warning(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let h = size / 2.0;
    let pts = vec![
        center + Vec2::new(0.0, -h * 0.9),
        center + Vec2::new(h, h * 0.8),
        center + Vec2::new(-h, h * 0.8),
    ];
    painter.add(Shape::closed_line(pts, Stroke::new(1.5, color)));
    painter.line_segment(
        [center + Vec2::new(0.0, -h * 0.3), center + Vec2::new(0.0, h * 0.2)],
        Stroke::new(1.5, color),
    );
    painter.circle_filled(center + Vec2::new(0.0, h * 0.5), 1.0, color);
}

pub fn icon_chevron_right(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let h = size * 0.3;
    let pts = vec![
        center + Vec2::new(-h * 0.5, -h),
        center + Vec2::new(h * 0.5, 0.0),
        center + Vec2::new(-h * 0.5, h),
    ];
    painter.add(Shape::line(pts, Stroke::new(1.5, color)));
}

pub fn icon_chevron_down(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let h = size * 0.3;
    let pts = vec![
        center + Vec2::new(-h, -h * 0.5),
        center + Vec2::new(0.0, h * 0.5),
        center + Vec2::new(h, -h * 0.5),
    ];
    painter.add(Shape::line(pts, Stroke::new(1.5, color)));
}

/// The icon that goes with a level: check, dot, warning triangle, cross.
#[allow(dead_code)]
pub fn icon_for_level(painter: &Painter, level: Level, center: Pos2, size: f32) {
    let c = level.color();
    match level {
        Level::Good => icon_check(painter, center, size, c),
        Level::Neutral => icon_dot(painter, center, size * 0.2, c),
        Level::Warn => icon_warning(painter, center, size, c),
        Level::Bad => icon_cross(painter, center, size, c),
    }
}

// -------------------------------------------------------------- widgets

/// Small filled dot plus `label: text`, on a surface-coloured pill.
pub fn status_pill(ui: &mut egui::Ui, label: &str, text: &str, level: Level) -> egui::Response {
    let color = level.color();
    let font = egui::FontId::proportional(theme::SIZE_PILL);
    let galley = ui
        .painter()
        .layout_no_wrap(format!("{label}: {text}"), font, color);
    let pad_x = 10.0;
    let dot_r = 4.0;
    let gap = 6.0;
    let size = Vec2::new(
        pad_x * 2.0 + dot_r * 2.0 + gap + galley.size().x,
        galley.size().y + 8.0,
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let radius = CornerRadius::same(theme::RADIUS_PILL);
        p.rect_filled(rect, radius, theme::SURFACE);
        p.rect_stroke(rect, radius, Stroke::new(1.0, theme::BORDER), StrokeKind::Inside);
        p.circle_filled(Pos2::new(rect.left() + pad_x + dot_r, rect.center().y), dot_r, color);
        let text_pos = Pos2::new(
            rect.left() + pad_x + dot_r * 2.0 + gap,
            rect.center().y - galley.size().y / 2.0,
        );
        p.galley(text_pos, galley, color);
    }
    response
}

/// Switch-style toggle with a label; accent track when on.
#[allow(dead_code)]
pub fn toggle(ui: &mut egui::Ui, on: &mut bool, label: &str) -> egui::Response {
    let track = Vec2::new(36.0, 20.0);
    let inner = ui.horizontal(|ui| {
        let (rect, mut response) = ui.allocate_exact_size(track, Sense::click());
        if response.clicked() {
            *on = !*on;
            response.mark_changed();
        }
        let t = ui.ctx().animate_bool_responsive(response.id, *on);
        if ui.is_rect_visible(rect) {
            let p = ui.painter();
            let radius = CornerRadius::same((track.y / 2.0) as u8);
            let fill = if *on {
                theme::ACCENT
            } else if response.hovered() {
                theme::SURFACE_HOVER
            } else {
                theme::SURFACE
            };
            p.rect_filled(rect, radius, fill);
            p.rect_stroke(rect, radius, Stroke::new(1.0, theme::BORDER), StrokeKind::Inside);
            let knob_r = track.y / 2.0 - 3.0;
            let x = egui::lerp(
                (rect.left() + knob_r + 3.0)..=(rect.right() - knob_r - 3.0),
                t,
            );
            let knob = if *on { theme::ACCENT_TEXT } else { theme::TEXT_MUTED };
            p.circle_filled(Pos2::new(x, rect.center().y), knob_r, knob);
        }
        if ui.add(egui::Label::new(label).sense(Sense::click())).clicked() {
            *on = !*on;
            response.mark_changed();
        }
        response
    });
    inner.inner
}

/// Muted, small, wrapped text.
#[allow(dead_code)]
pub fn note(ui: &mut egui::Ui, text: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .size(theme::SIZE_SMALL)
                .color(theme::TEXT_MUTED),
        )
        .wrap(),
    );
}

/// Card tinted by level: bold first line, muted lines below.
#[allow(dead_code)]
pub fn result_card(ui: &mut egui::Ui, level: Level, title: &str, lines: &[String]) {
    let color = level.color();
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.6)))
        .corner_radius(CornerRadius::same(theme::RADIUS))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(title).strong().color(color));
            for line in lines {
                ui.label(
                    egui::RichText::new(line.as_str())
                        .size(theme::SIZE_SMALL)
                        .color(theme::TEXT_MUTED),
                );
            }
        });
}

/// Checklist row: level icon, title, one-line detail, optional action button.
/// Returns true when the action button was clicked.
#[allow(dead_code)]
pub fn checklist_row(
    ui: &mut egui::Ui,
    level: Level,
    title: &str,
    detail: &str,
    action: Option<&str>,
) -> bool {
    let mut clicked = false;
    egui::Frame::new()
        .fill(theme::SURFACE)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(CornerRadius::same(theme::RADIUS))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                icon_for_level(ui.painter(), level, rect.center(), 16.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(title).strong());
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(detail)
                                .size(theme::SIZE_SMALL)
                                .color(theme::TEXT_MUTED),
                        )
                        .wrap(),
                    );
                });
                if let Some(label) = action {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        clicked = ui.button(label).clicked();
                    });
                }
            });
        });
    clicked
}

/// Accordion item. `open_id` holds the id of the open section ("" = none).
/// Clicking a closed header sets it to `id`; clicking the open header sets it
/// to "". The body is drawn only while open. Returns true when `open_id` was
/// changed by this call.
pub fn section(
    ui: &mut egui::Ui,
    id: &str,
    title: &str,
    status: &str,
    open_id: &mut String,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    let is_open = open_id == id;
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 34.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let fill = if response.hovered() {
            theme::SURFACE_HOVER
        } else if is_open {
            theme::SURFACE
        } else {
            theme::PANEL
        };
        let radius = CornerRadius::same(theme::RADIUS);
        p.rect_filled(rect, radius, fill);
        p.rect_stroke(rect, radius, Stroke::new(1.0, theme::BORDER), StrokeKind::Inside);
        let chev = Pos2::new(rect.left() + 16.0, rect.center().y);
        if is_open {
            icon_chevron_down(p, chev, 16.0, theme::TEXT);
        } else {
            icon_chevron_right(p, chev, 16.0, theme::TEXT_MUTED);
        }
        p.text(
            Pos2::new(rect.left() + 32.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            title,
            egui::FontId::proportional(theme::SIZE_SECTION_TITLE),
            theme::TEXT,
        );
        if !status.is_empty() {
            p.text(
                Pos2::new(rect.right() - 12.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                status,
                egui::FontId::proportional(theme::SIZE_SMALL),
                theme::TEXT_MUTED,
            );
        }
    }
    let mut changed = false;
    if response.clicked() {
        *open_id = if is_open { String::new() } else { id.to_string() };
        changed = true;
    }
    if open_id == id {
        egui::Frame::new()
            .fill(theme::SURFACE)
            .corner_radius(CornerRadius::same(theme::RADIUS))
            .inner_margin(egui::Margin::same(theme::SECTION_PADDING as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                body(ui);
            });
    }
    changed
}
