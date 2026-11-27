//! Virtual Xbox 360 output via ViGEm, with clamped report building and
//! force-feedback notification wiring.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::Result;
use vigem_rust::target::Xbox360;
use vigem_rust::{Client, TargetHandle, X360Button, X360Report};

use crate::hid::state::DeviceState;

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

/// Build a clamped X360 report from device state and a pre-computed button
/// bitmask (X360 bit layout).
///
/// All axes are clamped before conversion — out-of-range garbage from the
/// input path must never wrap (the pedal 50%-wrap regression fix).
pub fn build_report(state: &DeviceState, buttons: u16) -> X360Report {
    X360Report {
        buttons: X360Button::from_bits_retain(buttons),
        // Brake -> left trigger, throttle -> right trigger, clamped 0..255.
        left_trigger: (clamp01(state.brake) * 255.0).round() as u8,
        right_trigger: (clamp01(state.throttle) * 255.0).round() as u8,
        // Steering -> left thumbstick X, clamped -1..1 before scaling.
        thumb_lx: (state.steer.clamp(-1.0, 1.0) * 32767.0) as i16,
        thumb_ly: 0,
        thumb_rx: 0,
        thumb_ry: 0,
    }
}

/// A persistent virtual Xbox 360 controller.
pub struct X360Out {
    // Keep the client alive: dropping it would unplug the target.
    _client: Client,
    target: TargetHandle<Xbox360>,
    ffb: Option<Arc<crate::ffb::FfbController>>,
    ffb_thread: Option<JoinHandle<()>>,
    ffb_run: Arc<AtomicBool>,
}

impl X360Out {
    pub fn create(ffb: Option<Arc<crate::ffb::FfbController>>) -> Result<Self> {
        let client = Client::connect()?;

        // Create and plug in the virtual controller ONCE; the handle is
        // persistent and reused for every update. Re-creating targets per
        // frame caused enumeration lag and dropped inputs.
        let target = client.new_x360_target().plugin()?;

        // Right after plugin() Windows is still enumerating the device and
        // update() calls silently fail, so settle before the first report.
        //
        // We deliberately do NOT use wait_for_ready(): it parks a bus thread
        // in an uncancellable overlapped IOCTL and drops the receiver that
        // could wake it (no CancelIoEx anywhere in vigem-rust), leaking one
        // thread + bus handle per Start/Stop cycle. A bounded sleep is
        // cruder but leak-free.
        std::thread::sleep(Duration::from_millis(300));

        let mut out = Self {
            _client: client,
            target,
            ffb: None,
            ffb_thread: None,
            ffb_run: Arc::new(AtomicBool::new(false)),
        };

        if let Some(ffb) = ffb {
            // register_notification spawns the bus-side notification thread;
            // we drain its receiver on our own thread so FFB never blocks
            // the input path.
            let rx = out.target.register_notification()?;
            out.ffb_run.store(true, Ordering::SeqCst);
            let run = out.ffb_run.clone();
            let thread_ffb = Arc::clone(&ffb);
            let ffb_thread = std::thread::Builder::new()
                .name("ffb-notify".into())
                .spawn(move || {
                    while run.load(Ordering::SeqCst) {
                        match rx.recv_timeout(Duration::from_millis(100)) {
                            Ok(Ok(n)) => thread_ffb.on_motors(n.large_motor, n.small_motor),
                            Ok(Err(e)) => {
                                log::warn!("ViGEm notification error: {e}");
                            }
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    }
                })?;
            out.ffb = Some(ffb);
            out.ffb_thread = Some(ffb_thread);
        }

        Ok(out)
    }

    /// Send one report to the persistent target (low latency: one update
    /// per frame, no target churn).
    pub fn update(&mut self, state: &DeviceState, buttons: u16) -> Result<()> {
        let report = build_report(state, buttons);
        self.target.update(&report)?;
        Ok(())
    }
}

impl Drop for X360Out {
    fn drop(&mut self) {
        // Stop the FFB drain thread first so no on_motors calls race the stop.
        self.ffb_run.store(false, Ordering::SeqCst);
        if let Some(t) = self.ffb_thread.take() {
            let _ = t.join();
        }
        if let Some(ffb) = &self.ffb {
            if ffb.enabled() {
                ffb.stop();
            }
        }
        // The target is unplugged automatically when the TargetHandle (and
        // its internal Arc) drops; vigem-rust's TargetHandleInner::Drop
        // calls bus.unplug. No explicit unplug needed.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(steer: f64, throttle: f64, brake: f64) -> DeviceState {
        DeviceState { steer, throttle, brake, ..Default::default() }
    }

    #[test]
    fn pedal_full_is_255() {
        let r = build_report(&state(0.0, 1.0, 1.0), 0);
        assert_eq!(r.right_trigger, 255, "throttle 1.0 -> rt 255");
        assert_eq!(r.left_trigger, 255, "brake 1.0 -> lt 255");
    }

    #[test]
    fn pedal_above_range_clamps_not_wraps() {
        let r = build_report(&state(0.0, 1.2, 2.5), 0);
        assert_eq!(r.right_trigger, 255, "throttle 1.2 clamps to 255");
        assert_eq!(r.left_trigger, 255, "brake 2.5 clamps to 255");
    }

    #[test]
    fn pedal_negative_clamps_to_zero() {
        let r = build_report(&state(0.0, -0.5, -3.0), 0);
        assert_eq!(r.right_trigger, 0, "throttle -0.5 clamps to 0");
        assert_eq!(r.left_trigger, 0, "brake -3.0 clamps to 0");
    }

    #[test]
    fn pedal_midpoint() {
        let r = build_report(&state(0.0, 0.5, 0.25), 0);
        assert_eq!(r.right_trigger, 128);
        assert_eq!(r.left_trigger, 64);
    }

    #[test]
    fn steer_full_range() {
        let r = build_report(&state(-1.0, 0.0, 0.0), 0);
        assert_eq!(r.thumb_lx, -32767);
        let r = build_report(&state(1.0, 0.0, 0.0), 0);
        assert_eq!(r.thumb_lx, 32767);
        let r = build_report(&state(0.0, 0.0, 0.0), 0);
        assert_eq!(r.thumb_lx, 0);
    }

    #[test]
    fn steer_out_of_range_clamps() {
        let r = build_report(&state(1.5, 0.0, 0.0), 0);
        assert_eq!(r.thumb_lx, 32767, "steer 1.5 clamps to +32767");
        let r = build_report(&state(-2.0, 0.0, 0.0), 0);
        assert_eq!(r.thumb_lx, -32767, "steer -2.0 clamps to -32767");
    }

    #[test]
    fn buttons_mask_passed_through() {
        let r = build_report(&state(0.0, 0.0, 0.0), X360Button::A.bits() | X360Button::START.bits());
        assert!(r.buttons.contains(X360Button::A));
        assert!(r.buttons.contains(X360Button::START));
        assert!(!r.buttons.contains(X360Button::B));
    }

    #[test]
    fn other_axes_zero() {
        let r = build_report(&state(0.3, 0.1, 0.1), 0);
        assert_eq!((r.thumb_ly, r.thumb_rx, r.thumb_ry), (0, 0, 0));
    }
}
