//! Colour tokens, sizes and the egui style for the whole GUI.
//!
//! No colour literals may appear anywhere else in `src/gui/`.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Vec2,
};

// Colours (spec 6.1).
pub const BG: Color32 = Color32::from_rgb(0x0F, 0x11, 0x13);
pub const PANEL: Color32 = Color32::from_rgb(0x16, 0x19, 0x1C);
pub const SURFACE: Color32 = Color32::from_rgb(0x1E, 0x22, 0x26);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x26, 0x2B, 0x30);
pub const BORDER: Color32 = Color32::from_rgb(0x2E, 0x34, 0x3A);
/// Slider rails, unchecked switch tracks and widget outlines: clearly lighter
/// than `SURFACE` so controls are visible on dark surfaces.
pub const RAIL: Color32 = Color32::from_rgb(0x3F, 0x47, 0x4F);
pub const RAIL_HOVER: Color32 = Color32::from_rgb(0x55, 0x5E, 0x68);
pub const OUTLINE: Color32 = Color32::from_rgb(0x5A, 0x63, 0x6C);
pub const TEXT: Color32 = Color32::from_rgb(0xE6, 0xE8, 0xEA);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8B, 0x93, 0x9B);
pub const ACCENT: Color32 = Color32::from_rgb(0xA3, 0xE6, 0x35);
pub const ACCENT_TEXT: Color32 = Color32::from_rgb(0x0F, 0x11, 0x13);
pub const WARN: Color32 = Color32::from_rgb(0xF5, 0xB8, 0x3D);
pub const BAD: Color32 = Color32::from_rgb(0xF0, 0x52, 0x4F);
pub const INFO: Color32 = Color32::from_rgb(0x4B, 0xA3, 0xF5);
pub const WHEEL_RIM: Color32 = Color32::from_rgb(0x38, 0x3F, 0x46);
/// Thin darker line on the outer edge of the rim and lighter line on the inner
/// edge, so the rim reads as a ring with a little depth.
pub const WHEEL_RIM_DARK: Color32 = Color32::from_rgb(0x1B, 0x1F, 0x23);
pub const WHEEL_RIM_LIGHT: Color32 = Color32::from_rgb(0x5C, 0x66, 0x70);
/// Hub plate and spokes: lighter than `BG`, darker than the rim.
pub const WHEEL_HUB: Color32 = Color32::from_rgb(0x22, 0x27, 0x2D);
pub const WHEEL_HUB_EDGE: Color32 = Color32::from_rgb(0x55, 0x5E, 0x68);
/// Idle buttons, paddles and unlit shift LEDs: visible on the hub, but quiet.
pub const WHEEL_BUTTON: Color32 = Color32::from_rgb(0x34, 0x3B, 0x43);
/// Idle button labels.
pub const WHEEL_LABEL: Color32 = Color32::from_rgb(0xB0, 0xB8, 0xC0);

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

    // Also the slider's trailing fill.
    v.selection.bg_fill = ACCENT.gamma_multiply(0.6);
    v.slider_trailing_fill = true;
    v.selection.stroke = Stroke::new(1.0, ACCENT);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.weak_bg_fill = PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);

    // `bg_fill` is the slider rail and the checkbox box; `weak_bg_fill` the
    // button face.
    w.inactive.bg_fill = RAIL;
    w.inactive.weak_bg_fill = SURFACE;
    w.inactive.bg_stroke = Stroke::new(1.0, OUTLINE);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);

    w.hovered.bg_fill = RAIL_HOVER;
    w.hovered.weak_bg_fill = SURFACE_HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0, OUTLINE);
    w.hovered.fg_stroke = Stroke::new(1.5, TEXT);

    // egui takes `.strong()` text from `active.fg_stroke`, so it must be
    // light. Pressed widgets are therefore neutral with an accent outline;
    // accent-filled buttons set their own dark text explicitly.
    w.active.bg_fill = ACCENT;
    w.active.weak_bg_fill = RAIL;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.5, TEXT);

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
