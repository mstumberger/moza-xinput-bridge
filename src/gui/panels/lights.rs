use eframe::egui;

use crate::gui::widgets::{self, controls};
use crate::gui::MozaMapperApp;

pub fn show(app: &mut MozaMapperApp, ui: &mut egui::Ui) {
    // RPM shift lights: serial LED-bar channel (decoded from USBPcap).
    controls::error_line(ui, "Lights", &mut app.lights_error);
    // Preview only: draw the enabled layout without opening the serial port.
    // The toggle then does nothing and the sweep button is inert.
    let preview = app.preview;
    let mut enabled = app.config.rpm_lights.enabled || preview;
    if widgets::toggle(ui, &mut enabled, "Enable shift lights").changed() && !preview {
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
    widgets::note(ui, "Uses the wheelbase serial port.");
    if app.lights.is_none() && !preview {
        return;
    }

    let mut cfg = app.config.rpm_lights.clone();
    let mut save = false;
    ui.add_space(4.0);
    if controls::value_row(ui, "Max RPM", |ui| {
        ui.add(egui::DragValue::new(&mut cfg.max_rpm).range(500..=20_000).suffix(" rpm"))
    })
    .changed()
    {
        app.manual_rpm = app.manual_rpm.min(cfg.max_rpm);
        save = true;
    }
    if controls::slider_row(ui, "LEDs", |ui| ui.add(egui::Slider::new(&mut cfg.num_leds, 1..=16)))
        .changed()
    {
        save = true;
    }
    if controls::slider_row(ui, "First light at", |ui| {
        ui.add(egui::Slider::new(&mut cfg.start_pct, 0..=99).suffix(" %"))
    })
    .changed()
    {
        cfg.end_pct = cfg.end_pct.max(cfg.start_pct + 1);
        save = true;
    }
    if controls::slider_row(ui, "All lit at", |ui| {
        ui.add(egui::Slider::new(&mut cfg.end_pct, 1..=100).suffix(" %"))
    })
    .changed()
    {
        cfg.start_pct = cfg.start_pct.min(cfg.end_pct.saturating_sub(1));
        save = true;
    }

    controls::subheading(ui, "Sources");
    let mut udp = cfg.udp_enabled;
    if widgets::toggle(ui, &mut udp, "UDP telemetry").changed() {
        cfg.udp_enabled = udp;
        save = true;
    }
    let mut port = cfg.udp_port as i32;
    if controls::value_row(ui, "Port", |ui| {
        ui.add(egui::DragValue::new(&mut port).range(1024..=65535))
    })
    .changed()
    {
        cfg.udp_port = port.clamp(1024, 65535) as u16;
        save = true;
    }
    widgets::note(ui, "Turn lights off and on to apply a new port.");
    let mut rumble = cfg.rumble_enabled;
    if widgets::toggle(ui, &mut rumble, "Controller rumble").changed() {
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

    controls::subheading(ui, "Test");
    // Manual RPM value: drives the bar directly.
    let mut rpm = app.manual_rpm;
    let max_rpm = app.config.rpm_lights.max_rpm;
    if controls::slider_row(ui, "Manual RPM", |ui| ui.add(egui::Slider::new(&mut rpm, 0..=max_rpm)))
        .changed()
    {
        app.manual_rpm = rpm;
        if let Some(h) = &app.lights_handle {
            h.send_rpm(rpm);
        }
    }
    let sweeping = app.sweep_thread.as_ref().is_some_and(|t| !t.is_finished());
    if sweeping {
        if ui.button("Stop sweep").clicked() {
            app.stop_sweep();
        }
    } else if ui.button("Test sweep").clicked() && !preview {
        app.start_sweep();
    }
}
