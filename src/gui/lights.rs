use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::MozaMapperApp;

impl MozaMapperApp {
    /// Start the shift-light controller + UDP listener from the current
    /// config. Threshold/max-rpm changes apply live via set_config; UDP
    /// port changes need an off/on cycle.
    pub(crate) fn start_lights(&mut self) {
        if self.lights.is_some() {
            return;
        }
        match crate::rpm::LightsController::start(self.config.rpm_lights.clone()) {
            Ok(c) => {
                let handle = c.handle();
                if self.config.rpm_lights.udp_enabled {
                    let run = Arc::new(AtomicBool::new(true));
                    match crate::rpm::telemetry::spawn_udp_listener(
                        Arc::clone(&handle),
                        self.config.rpm_lights.udp_port,
                        Arc::clone(&run),
                    ) {
                        Ok(t) => {
                            self.udp_thread = Some(t);
                            self.udp_run = Some(run);
                        }
                        Err(e) => {
                            self.lights_error = Some(format!(
                                "UDP listener (port {}) failed: {}",
                                self.config.rpm_lights.udp_port, e
                            ));
                        }
                    }
                }
                self.lights = Some(c);
                self.lights_handle = Some(handle);
                self.lights_error = None;
            }
            Err(e) => self.lights_error = Some(e),
        }
    }

    pub(crate) fn stop_lights(&mut self) {
        self.stop_sweep();
        if let Some(run) = self.udp_run.take() {
            run.store(false, Ordering::Relaxed);
        }
        if let Some(t) = self.udp_thread.take() {
            let _ = t.join();
        }
        // Dropping the controller stops its thread and clears the bar.
        self.lights = None;
        self.lights_handle = None;
    }

    pub(crate) fn stop_sweep(&mut self) {
        if let Some(run) = self.sweep_run.take() {
            run.store(false, Ordering::Relaxed);
        }
        if let Some(t) = self.sweep_thread.take() {
            let _ = t.join();
        }
    }

    /// Sweep the bar 0 -> full -> 0 over ~2.5 s so the user can see all
    /// lights and their order without telemetry.
    pub(crate) fn start_sweep(&mut self) {
        if let (Some(handle), None) = (&self.lights_handle, &self.sweep_thread) {
            let handle = Arc::clone(handle);
            let run = Arc::new(AtomicBool::new(true));
            self.sweep_run = Some(Arc::clone(&run));
            self.sweep_thread = Some(thread::spawn(move || {
                let n = 10u32; // full bar regardless of config; it is a test
                let step = Duration::from_millis(120);
                for i in 0..=n {
                    if !run.load(Ordering::Relaxed) {
                        break;
                    }
                    handle.send_mask(((1u32 << i) - 1) as u16);
                    thread::sleep(step);
                }
                for i in (0..n).rev() {
                    if !run.load(Ordering::Relaxed) {
                        break;
                    }
                    handle.send_mask(((1u32 << i) - 1) as u16);
                    thread::sleep(step);
                }
                handle.send_mask(0);
            }));
        }
    }
}
