//! RPM shift lights: drive the wheelbase's LED bar over its CDC serial
//! interface.
//!
//! The wheelbase (VID 0x346E) exposes a CDC serial port; Pit House streams
//! LED-mask frames (`0x1841`) over its bulk OUT endpoint — the same channel
//! hidapi cannot use, which is why this module talks to the COM port via
//! the `serialport` crate instead of the HID transport.
//!
//! Sources feeding the lights (all optional, last writer wins):
//! - GUI manual RPM slider / test sweep
//! - UDP telemetry listener (`telemetry::spawn_udp_listener`)
//! - Rumble forwarding from the virtual X360 controller: XInput has no RPM
//!   field, but games do send motor (rumble) commands; when enabled the
//!   large-motor value is mapped onto the light bar as an approximation.

pub mod protocol;
pub mod serial;
pub mod telemetry;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::config::RpmLightsConfig;

/// Commands sent to the writer thread.
enum Cmd {
    /// Target RPM; the thread converts it to an LED mask using the live config.
    Rpm(u16),
    /// Direct LED mask (used by the test sweep).
    Mask(u16),
}

/// Shared handle: safe to clone into the GUI, the bridge thread and the
/// ViGEm notification thread.
pub struct LightsHandle {
    tx: Mutex<Sender<Cmd>>,
    run: Arc<AtomicBool>,
    cfg: Arc<RwLock<RpmLightsConfig>>,
}

impl LightsHandle {
    /// Update the live config (thresholds, max RPM, LED count) without a
    /// restart.
    pub fn set_config(&self, cfg: RpmLightsConfig) {
        *self.cfg.write().unwrap() = cfg;
    }

    /// Feed a target RPM; converted to an LED mask by the writer thread.
    pub fn send_rpm(&self, rpm: u16) {
        let _ = self.tx.lock().unwrap().send(Cmd::Rpm(rpm));
    }

    /// Drive the bar directly with a raw LED bitmask.
    pub fn send_mask(&self, mask: u16) {
        let _ = self.tx.lock().unwrap().send(Cmd::Mask(mask));
    }

    /// Whether UDP telemetry is currently accepted (live config).
    pub fn udp_enabled(&self) -> bool {
        self.cfg.read().unwrap().udp_enabled
    }

    /// Forward a rumble (large motor, 0..255) update from the virtual
    /// controller. No-op unless rumble forwarding is enabled.
    pub fn on_rumble(&self, large_motor: u8) {
        let cfg = self.cfg.read().unwrap().clone();
        if cfg.rumble_enabled {
            let rpm = (large_motor as u32 * cfg.max_rpm as u32 / 255) as u16;
            self.send_rpm(rpm);
        }
    }
}

/// Owns the writer thread. Dropping stops it (and clears the lights).
pub struct LightsController {
    handle: Arc<LightsHandle>,
    thread: Option<JoinHandle<()>>,
}

impl LightsController {
    /// Open the wheelbase's serial port and start streaming. Fails if no
    /// MOZA CDC port is present (e.g. Pit House has it exclusively? it is
    /// a separate interface, so usually both can coexist).
    pub fn start(cfg: RpmLightsConfig) -> Result<Self, String> {
        let mut port = serial::open_moza_serial().map_err(|e| e.to_string())?;
        let (tx, rx) = std::sync::mpsc::channel::<Cmd>();
        let run = Arc::new(AtomicBool::new(true));
        let shared_cfg = Arc::new(RwLock::new(cfg));
        let run_thread = Arc::clone(&run);

        let handle = Arc::new(LightsHandle {
            tx: Mutex::new(tx),
            run: Arc::clone(&run),
            cfg: Arc::clone(&shared_cfg),
        });

        let thread_cfg = Arc::clone(&shared_cfg);
        let thread = std::thread::Builder::new()
            .name("rpm-lights".into())
            .spawn(move || {
                let mut last_mask = 0u16;
                loop {
                    if !run_thread.load(Ordering::Relaxed) {
                        break;
                    }
                    // Drain commands for up to 100 ms, then keepalive.
                    match rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(Cmd::Rpm(rpm)) => {
                            let cfg = thread_cfg.read().unwrap().clone();
                            last_mask = mask_for_rpm(rpm, &cfg);
                        }
                        Ok(Cmd::Mask(mask)) => last_mask = mask,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                    // Resend the current mask every loop (~10 Hz) so the bar
                    // stays driven even without new telemetry.
                    if let Err(e) = port.write_all(&protocol::led_mask_frame(last_mask)) {
                        log::warn!("rpm lights write failed: {}", e);
                    }
                }
                let _ = port.write_all(&protocol::led_mask_frame(0));
            })
            .map_err(|e| format!("failed to spawn rpm-lights thread: {}", e))?;

        Ok(Self { handle, thread: Some(thread) })
    }

    pub fn handle(&self) -> Arc<LightsHandle> {
        Arc::clone(&self.handle)
    }
}

impl Drop for LightsController {
    fn drop(&mut self) {
        self.handle.run.store(false, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// LED bitmask for a target RPM: lights i is on when rpm/max_rpm reaches
/// the i-th threshold. Thresholds are evenly spaced from `start_pct` to
/// `end_pct` of max RPM across `num_leds` lights.
pub fn mask_for_rpm(rpm: u16, cfg: &RpmLightsConfig) -> u16 {
    let max = cfg.max_rpm.max(1) as f32;
    let pct = (rpm as f32 / max * 100.0).clamp(0.0, 100.0);
    let n = cfg.num_leds.clamp(1, 16) as u32;
    if pct >= cfg.end_pct as f32 {
        return ((1u32 << n) - 1) as u16;
    }
    if pct < cfg.start_pct as f32 {
        return 0;
    }
    let span = (cfg.end_pct.saturating_sub(cfg.start_pct)).max(1) as f32;
    let lit = ((pct - cfg.start_pct as f32) / span * (n - 1) as f32).round() as u32 + 1;
    ((1u32 << lit.min(n)) - 1) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> RpmLightsConfig {
        RpmLightsConfig {
            enabled: true,
            max_rpm: 10_000,
            num_leds: 10,
            start_pct: 55,
            end_pct: 100,
            udp_enabled: false,
            udp_port: 26999,
            rumble_enabled: false,
        }
    }

    #[test]
    fn below_start_lights_nothing() {
        assert_eq!(mask_for_rpm(0, &cfg()), 0);
        assert_eq!(mask_for_rpm(5_499, &cfg()), 0);
    }

    #[test]
    fn at_redline_lights_everything() {
        let c = cfg();
        assert_eq!(mask_for_rpm(10_000, &c), 0x3FF);
        assert_eq!(mask_for_rpm(9_999, &c), 0x3FF, "100% band is the last light band");
    }

    #[test]
    fn thresholds_are_monotonic() {
        let c = cfg();
        let mut prev = 0u16;
        for rpm in (0..=10_000).step_by(100) {
            let m = mask_for_rpm(rpm, &c);
            assert!(m >= prev, "mask must not decrease with rpm ({} -> {})", prev, m);
            assert!(m.count_ones() <= c.num_leds as u32);
            prev = m;
        }
    }

    #[test]
    fn above_max_clamps() {
        let c = cfg();
        assert_eq!(mask_for_rpm(20_000, &c), 0x3FF);
    }

    #[test]
    fn degenerate_config_is_safe() {
        let c = RpmLightsConfig {
            num_leds: 0,
            start_pct: 90,
            end_pct: 90,
            ..cfg()
        };
        assert_eq!(mask_for_rpm(5_000, &c), 0);
        assert_eq!(mask_for_rpm(10_000, &c), 1); // num_leds clamps to 1
    }
}
