use eframe::egui;

use crate::gui::{theme, MozaMapperApp};

pub fn show(app: &mut MozaMapperApp, ui: &mut egui::Ui) {
    // RPM shift lights — serial LED-bar channel (decoded from USBPcap).
    ui.group(|ui| {
        ui.label("RPM shift lights");
        if let Some(err) = app.lights_error.clone() {
            ui.horizontal(|ui| {
                ui.colored_label(theme::BAD, format!("Lights: {}", err));
                if ui.small_button("×").clicked() {
                    app.lights_error = None;
                }
            });
        }
        let mut enabled = app.config.rpm_lights.enabled;
        if ui.checkbox(&mut enabled, "Enable (opens the wheelbase serial port)").changed() {
            app.config.rpm_lights.enabled = enabled;
            if enabled {
                app.start_lights();
            } else {
                app.stop_lights();
            }
            if let Err(e) = app.config.save() {
                app.config_error = Some(e);
            }
        }
        if app.lights.is_some() {
            let mut cfg = app.config.rpm_lights.clone();
            let mut save = false;
            ui.horizontal(|ui| {
                if ui
                    .add(egui::DragValue::new(&mut cfg.max_rpm).range(500..=20_000).suffix(" rpm"))
                    .changed()
                {
                    app.manual_rpm = app.manual_rpm.min(cfg.max_rpm);
                    save = true;
                }
                ui.label("Max RPM");
            });
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Slider::new(&mut cfg.num_leds, 1..=16).text("LEDs"))
                    .changed()
                {
                    save = true;
                }
                if ui
                    .add(egui::Slider::new(&mut cfg.start_pct, 0..=99).text("First light %"))
                    .changed()
                {
                    cfg.end_pct = cfg.end_pct.max(cfg.start_pct + 1);
                    save = true;
                }
                if ui
                    .add(egui::Slider::new(&mut cfg.end_pct, 1..=100).text("Full bar %"))
                    .changed()
                {
                    cfg.start_pct = cfg.start_pct.min(cfg.end_pct.saturating_sub(1));
                    save = true;
                }
            });
            let mut lights_on = app.lights_handle.is_some();
            ui.horizontal(|ui| {
                let mut udp = cfg.udp_enabled;
                if ui.checkbox(&mut udp, "UDP telemetry").changed() {
                    cfg.udp_enabled = udp;
                    save = true;
                }
                let mut port = cfg.udp_port as i32;
                if ui
                    .add(egui::DragValue::new(&mut port).range(1024..=65535))
                    .changed()
                {
                    cfg.udp_port = port.clamp(1024, 65535) as u16;
                    save = true;
                }
                ui.weak("port (restart lights to apply)");
                let _ = &mut lights_on;
            });
            let mut rumble = cfg.rumble_enabled;
            if ui
                .checkbox(&mut rumble, "Forward controller rumble to lights")
                .changed()
            {
                cfg.rumble_enabled = rumble;
                save = true;
            }
            if save {
                if let Some(h) = &app.lights_handle {
                    h.set_config(cfg.clone());
                }
                app.config.rpm_lights = cfg;
                if let Err(e) = app.config.save() {
                    app.config_error = Some(e);
                }
            }
            // Manual RPM value — drives the bar directly.
            let mut rpm = app.manual_rpm;
            if ui
                .add(
                    egui::Slider::new(&mut rpm, 0..=app.config.rpm_lights.max_rpm)
                        .text("Manual RPM"),
                )
                .changed()
            {
                app.manual_rpm = rpm;
                if let Some(h) = &app.lights_handle {
                    h.send_rpm(rpm);
                }
            }
            let sweeping = app.sweep_thread.as_ref().is_some_and(|t| !t.is_finished());
            if sweeping {
                if ui.button("Stop test sweep").clicked() {
                    app.stop_sweep();
                }
            } else if ui.button("Test sweep").clicked() {
                app.start_sweep();
            }
        }
    });
}
