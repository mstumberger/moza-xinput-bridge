//! Left column while the bridge runs: wheel, pedal bars and a status line
//! (spec 4.4). With `MOZA_GUI_PREVIEW=1` it is fed by `gui::preview` instead.

use eframe::egui;

use crate::gui::widgets::pedals;
use crate::gui::widgets::wheel::{WheelView, CHIP_ROW_HEIGHT};
use crate::gui::widgets::wheel_layout::build_labels;
use crate::gui::{preview, theme, MozaMapperApp};

/// Window width below which the pedals move under the wheel.
const NARROW_WINDOW: f32 = 900.0;
const INFO_LINE_HEIGHT: f32 = 20.0;
const PEDAL_GAP: f32 = 24.0;

/// Wheel side length for the available space (spec 4.4). The chip row and the
/// info line are reserved below the wheel on top of the spec's 24 px margin.
pub fn wheel_side(avail_w: f32, avail_h: f32, narrow: bool) -> f32 {
    let below = CHIP_ROW_HEIGHT + INFO_LINE_HEIGHT;
    let side = if narrow {
        avail_w.min(avail_h - 24.0 - below - pedals::HORIZONTAL_HEIGHT - 8.0)
    } else {
        (avail_w - 150.0).min(avail_h - 24.0 - below)
    };
    side.clamp(240.0, 640.0)
}

/// `Steering +12.4 %` plus `Buttons 3, 17` when any are pressed.
pub fn info_line(steer: f64, buttons: &[bool; 64]) -> String {
    let mut line = format!("Steering {:+.1} %", steer * 100.0);
    let pressed: Vec<String> = (0..64)
        .filter(|&i| buttons[i])
        .map(|i| (i + 1).to_string())
        .collect();
    if !pressed.is_empty() {
        line.push_str(&format!("    Buttons {}", pressed.join(", ")));
    }
    line
}

pub fn show(app: &mut MozaMapperApp, ui: &mut egui::Ui) {
    let (state, led_mask) = if app.preview {
        let t = app.preview_start.elapsed().as_secs_f64();
        (preview::state_at(t), Some(preview::mask_at(t)))
    } else {
        let mask = app.lights_handle.as_ref().map(|h| h.current_mask());
        (app.state.current_state.lock().unwrap().clone(), mask)
    };
    let labels = build_labels(&app.config.mapping);
    let view = WheelView {
        steer: state.steer,
        steering_range_deg: app.config.ui.steering_range_deg as f32,
        buttons: &state.buttons,
        labels: &labels,
        led_mask,
        led_count: app.config.rpm_lights.num_leds,
    };
    let values = [state.throttle, state.brake, state.clutch];

    let avail = ui.available_size();
    let narrow = ui.ctx().screen_rect().width() < NARROW_WINDOW;
    let side = wheel_side(avail.x, avail.y, narrow);
    let info_text = egui::RichText::new(info_line(state.steer, &state.buttons))
        .size(theme::SIZE_SMALL)
        .color(theme::TEXT_MUTED);

    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
    let wheel_block_h = side + CHIP_ROW_HEIGHT + INFO_LINE_HEIGHT;
    if narrow {
        let content_h = wheel_block_h + 8.0 + pedals::HORIZONTAL_HEIGHT;
        ui.add_space(((avail.y - content_h) / 2.0).max(0.0));
        ui.vertical_centered(|ui| {
            view.show(ui, side);
            ui.label(info_text);
            ui.add_space(8.0);
            pedals::horizontal(ui, values, side.min(360.0));
        });
    } else {
        let bar_h = side * 0.6;
        let content_w = side + PEDAL_GAP + pedals::GROUP_WIDTH;
        ui.add_space(((avail.y - wheel_block_h) / 2.0).max(0.0));
        ui.horizontal_top(|ui| {
            ui.add_space(((avail.x - content_w) / 2.0).max(0.0));
            ui.vertical(|ui| {
                view.show(ui, side);
                let layout = egui::Layout::top_down(egui::Align::Center);
                ui.allocate_ui_with_layout(egui::vec2(side, INFO_LINE_HEIGHT), layout, |ui| {
                    ui.label(info_text);
                });
            });
            ui.add_space(PEDAL_GAP);
            ui.vertical(|ui| {
                ui.add_space((side - pedals::vertical_height(bar_h)) / 2.0);
                pedals::vertical(ui, values, bar_h);
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_is_clamped() {
        assert_eq!(wheel_side(2000.0, 2000.0, false), 640.0);
        assert_eq!(wheel_side(300.0, 300.0, false), 240.0);
        assert_eq!(wheel_side(700.0, 2000.0, false), 550.0);
    }

    #[test]
    fn info_line_lists_pressed_buttons() {
        let mut b = [false; 64];
        assert_eq!(info_line(0.124, &b), "Steering +12.4 %");
        b[2] = true;
        b[16] = true;
        assert_eq!(info_line(-0.05, &b), "Steering -5.0 %    Buttons 3, 17");
    }
}
