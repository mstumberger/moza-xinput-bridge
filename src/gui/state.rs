use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::hid::state::DeviceState;

/// Shared state between the bridge thread and the GUI.
#[derive(Clone)]
pub struct AppState {
    pub device_connected: Arc<Mutex<bool>>,
    pub current_state: Arc<Mutex<DeviceState>>,
    pub fps: Arc<Mutex<f64>>,
    pub report_count: Arc<Mutex<u64>>,
    pub ffb_active: Arc<AtomicBool>,
    /// Last error to surface in the GUI (never swallowed silently).
    pub last_error: Arc<Mutex<Option<String>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            device_connected: Arc::new(Mutex::new(false)),
            current_state: Arc::new(Mutex::new(DeviceState::default())),
            fps: Arc::new(Mutex::new(0.0)),
            report_count: Arc::new(Mutex::new(0)),
            ffb_active: Arc::new(AtomicBool::new(false)),
            last_error: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn set_error(&self, msg: impl Into<String>) {
        let msg = msg.into();
        log::error!("{}", msg);
        *self.last_error.lock().unwrap() = Some(msg);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
