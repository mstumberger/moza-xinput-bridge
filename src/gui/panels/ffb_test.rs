use std::sync::atomic::Ordering;
use std::time::Duration;

use eframe::egui;

use crate::gui::{theme, MozaMapperApp};

pub fn show(app: &mut MozaMapperApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    // FFB manual test — decoded vendor torque protocol.
    let mut close_ffb_test = false;
    if let Some(err) = app.ffb_test_error.clone() {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(theme::BAD, format!("FFB test: {}", err));
            if ui.small_button("×").clicked() {
                app.ffb_test_error = None;
            }
        });
    }
    if let Some(test) = &mut app.ffb_test {
        ui.add(
            egui::Slider::new(&mut test.strength_pct, 5.0..=100.0)
                .text("Strength %"),
        );
        let strength = test.strength_pct;
        test.pulse_strength.store(strength.to_bits(), Ordering::Relaxed);
        let cmd = test.torque_cmd.clone();
        let magnitude = (strength / 100.0 * 32767.0) as i16;

        // Live steering readout.
        let raw = test.steer.load(Ordering::Relaxed);
        if raw == u32::MAX {
            ui.label("Steering: no data");
        } else {
            let centered = raw as i32 - 32768;
            ui.label(format!("Steering raw: {raw}  (center offset {centered:+})"));
            ui.add(
                egui::ProgressBar::new(raw as f32 / 65535.0)
                    .text(format!("{:+.1}%", centered as f32 / 327.68)),
            );
        }

        ui.label("Hold a button to push (force only while held):");
        let (left, right) = ui
            .horizontal(|ui| {
                let l = ui
                    .add(
                        egui::Button::new("◀ Push left")
                            .min_size(egui::vec2(100.0, 32.0))
                            .sense(egui::Sense::click_and_drag()),
                    )
                    .is_pointer_button_down_on();
                let r = ui
                    .add(
                        egui::Button::new("Push right ▶")
                            .min_size(egui::vec2(100.0, 32.0))
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

        ui.separator();
        if ui
            .button("Auto pulse test (hands off: left 0.5 s, then right 0.5 s)")
            .clicked()
        {
            test.pulse_req.store(true, Ordering::Relaxed);
            *test.pulse_result.lock().unwrap() = "starting...".into();
        }
        let res = test.pulse_result.lock().unwrap().clone();
        if !res.is_empty() {
            let color = if res.starts_with("OK") {
                theme::ACCENT
            } else if res.starts_with("NO FORCE") || res.starts_with("REVERSED") {
                theme::BAD
            } else {
                theme::WARN
            };
            ui.colored_label(color, res);
        }
        // Keep the live readout moving.
        ctx.request_repaint_after(Duration::from_millis(30));

        if ui.button("Close FFB test").clicked() {
            close_ffb_test = true;
        }
    } else {
        ui.label("Opens the wheel for direct torque: Push left/right buttons, live steering and an auto pulse test appear here.");
        if ui.button("Open FFB test").clicked() {
            app.start_ffb_test();
        }
    }
    if close_ffb_test {
        app.stop_ffb_test();
    }
}
