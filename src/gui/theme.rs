//! Colour tokens, sizes and the egui style for the whole GUI.
//!
//! No colour literals may appear anywhere else in `src/gui/`.

#![allow(dead_code)]

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Vec2,
};

// Colours (spec 6.1).
pub const BG: Color32 = Color32::from_rgb(0x0F, 0x11, 0x13);
pub const PANEL: Color32 = Color32::from_rgb(0x16, 0x19, 0x1C);
pub const SURFACE: Color32 = Color32::from_rgb(0x1E, 0x22, 0x26);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x26, 0x2B, 0x30);
pub const BORDER: Color32 = Color32::from_rgb(0x2E, 0x34, 0x3A);
pub const TEXT: Color32 = Color32::from_rgb(0xE6, 0xE8, 0xEA);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8B, 0x93, 0x9B);
pub const ACCENT: Color32 = Color32::from_rgb(0xA3, 0xE6, 0x35);
pub const ACCENT_TEXT: Color32 = Color32::from_rgb(0x0F, 0x11, 0x13);
pub const WARN: Color32 = Color32::from_rgb(0xF5, 0xB8, 0x3D);
pub const BAD: Color32 = Color32::from_rgb(0xF0, 0x52, 0x4F);
pub const INFO: Color32 = Color32::from_rgb(0x4B, 0xA3, 0xF5);
pub const WHEEL_RIM: Color32 = Color32::from_rgb(0x2A, 0x2F, 0x34);
pub const WHEEL_HUB: Color32 = Color32::from_rgb(0x1A, 0x1D, 0x20);

// Sizes (spec 6.2).
pub const SIZE_HEADING: f32 = 20.0;
pub const SIZE_SECTION_TITLE: f32 = 15.0;
pub const SIZE_BODY: f32 = 14.0;
pub const SIZE_SMALL: f32 = 12.0;
pub const SIZE_PILL: f32 = 12.0;
pub const SIZE_MONO: f32 = 13.0;
pub const SPACING_UNIT: f32 = 8.0;
pub const SECTION_PADDING: f32 = 12.0;
pub const COLUMN_PADDING: f32 = 16.0;
pub const RADIUS: u8 = 6;
pub const RADIUS_PILL: u8 = 255;
pub const MIN_INTERACT_HEIGHT: f32 = 28.0;
pub const TOP_BAR_HEIGHT: f32 = 56.0;
pub const RIGHT_COLUMN_WIDTH: f32 = 340.0;

/// Install the dark theme on the context. Call once at startup.
pub fn apply(ctx: &egui::Context) {
    // Always dark, whatever the system theme is.
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut v = egui::Visuals::dark();
    let radius = CornerRadius::same(RADIUS);

    v.override_text_color = None;
    v.window_fill = BG;
    v.panel_fill = BG;
    v.extreme_bg_color = SURFACE;
    v.faint_bg_color = SURFACE;
    v.code_bg_color = SURFACE;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = radius;
    v.menu_corner_radius = radius;
    v.hyperlink_color = INFO;
    v.warn_fg_color = WARN;
    v.error_fg_color = BAD;

    v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, ACCENT);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.weak_bg_fill = PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);

    w.inactive.bg_fill = SURFACE;
    w.inactive.weak_bg_fill = SURFACE;
    w.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);

    w.hovered.bg_fill = SURFACE_HOVER;
    w.hovered.weak_bg_fill = SURFACE_HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0, BORDER);
    w.hovered.fg_stroke = Stroke::new(1.5, TEXT);

    w.active.bg_fill = ACCENT;
    w.active.weak_bg_fill = ACCENT;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(2.0, ACCENT_TEXT);

    w.open.bg_fill = SURFACE_HOVER;
    w.open.weak_bg_fill = SURFACE_HOVER;
    w.open.bg_stroke = Stroke::new(1.0, BORDER);
    w.open.fg_stroke = Stroke::new(1.0, TEXT);

    for wv in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        wv.corner_radius = radius;
    }

    ctx.set_visuals(v);

    ctx.style_mut(|style| {
        style.spacing.item_spacing = Vec2::splat(SPACING_UNIT);
        style.spacing.button_padding = Vec2::new(SPACING_UNIT, 4.0);
        style.spacing.interact_size.y = MIN_INTERACT_HEIGHT;
        style.spacing.window_margin = egui::Margin::same(SECTION_PADDING as i8);
        style.text_styles = [
            (TextStyle::Heading, FontId::new(SIZE_HEADING, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(SIZE_BODY, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(SIZE_BODY, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(SIZE_SMALL, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(SIZE_MONO, FontFamily::Monospace)),
        ]
        .into();
    });
}
