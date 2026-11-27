//! Interactive button-mapping wizard (port of the old `button_mapper.rs`,
//! with its bugs fixed).
//!
//! Fixes over the legacy wizard:
//! - No inverted-baseline hack: a button held during startup can no longer
//!   permanently invert its detection.
//! - `read_button_state` returns `Option`: when no HID report arrives within
//!   the timeout, the previously-pressed set is left untouched, so a wheel
//!   that stops sending reports cannot cause spurious re-detection of
//!   held buttons.
//! - Console I/O is kept OUT of this module: callers supply callbacks, so
//!   the wizard can run from the GUI.
//!
//! All MOZA button numbers are 1-based.

use crate::config::{MappingConfig, XBOX_ORDER};
use crate::hid::descriptor::ReportLayout;
use crate::hid::state::decode;
use std::time::{Duration, Instant};

/// How long to wait for a single button press per prompt before skipping.
const PRESS_TIMEOUT: Duration = Duration::from_secs(30);
/// Per-read HID timeout.
const READ_TIMEOUT_MS: i32 = 50;

/// Pure helper: 1-based numbers of buttons newly pressed in `cur` relative
/// to `prev`.
fn diff_new_presses(prev: &[bool; 64], cur: &[bool; 64]) -> Vec<u8> {
    (0..64)
        .filter(|&i| cur[i] && !prev[i])
        .map(|i| (i + 1) as u8)
        .collect()
}

pub struct Wizard {
    dev: hidapi::HidDevice,
    layout: ReportLayout,
    previously_pressed: [bool; 64],
    /// Checked between prompts; when false the wizard stops early and
    /// returns what it has learned so far (partial results are kept).
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Wizard {
    pub fn new(
        dev: hidapi::HidDevice,
        layout: ReportLayout,
        cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Wizard {
            dev,
            layout,
            previously_pressed: [false; 64],
            cancel,
        }
    }

    /// Read one report and return its decoded button state, or `None` when
    /// no report arrived within the timeout. `previously_pressed` is only
    /// updated when a report actually arrived.
    fn read_button_state(&mut self, timeout_ms: i32) -> Option<[bool; 64]> {
        let mut buf = [0u8; 128];
        match self.dev.read_timeout(&mut buf, timeout_ms) {
            Ok(len) if len > 0 => {
                let state = decode(&buf[..len], &self.layout, &Default::default());
                Some(state.buttons)
            }
            _ => None,
        }
    }

    /// Wait up to `timeout` for a new button press; returns its 1-based
    /// number, or `None` on timeout or cancellation.
    pub fn wait_for_press(&mut self, timeout: Duration) -> Option<u8> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return None;
            }
            if let Some(cur) = self.read_button_state(READ_TIMEOUT_MS) {
                let new = diff_new_presses(&self.previously_pressed, &cur);
                self.previously_pressed = cur;
                if let Some(first) = new.first() {
                    return Some(*first);
                }
            }
        }
        None
    }

    /// Wait until no buttons are pressed (so the just-learned press does not
    /// immediately leak into the next prompt). Times out after `timeout`.
    fn wait_for_release(&mut self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            if let Some(cur) = self.read_button_state(READ_TIMEOUT_MS) {
                self.previously_pressed = cur;
                if !cur.iter().any(|&p| p) {
                    return;
                }
            }
        }
    }

    /// Walk the wizard through every xbox button in [`XBOX_ORDER`] order.
    ///
    /// `on_prompt(name)` fires before waiting for that button's press;
    /// `on_result(name, Some(n))` after a successful learn, and
    /// `on_result(name, None)` when the entry was skipped (timeout/cancel)
    /// — in that case the entry keeps its previously configured value.
    ///
    /// Starts from `base` (the caller's current mapping) and RETURNS the
    /// new mapping — the caller owns persisting it. The wizard never
    /// touches app.json itself: it must not race the GUI's config saves.
    pub fn learn_all(
        &mut self,
        base: MappingConfig,
        mut on_prompt: impl FnMut(&str),
        mut on_result: impl FnMut(&str, Option<u8>),
    ) -> MappingConfig {
        let mut mapping = base;
        // Prime the baseline: a button already held when the wizard starts
        // must not register as a "new press" for the first prompt.
        self.wait_for_release(Duration::from_secs(2));
        for name in XBOX_ORDER {
            if self.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return mapping;
            }
            on_prompt(name);
            match self.wait_for_press(PRESS_TIMEOUT) {
                Some(n) => {
                    if let Some(entry) = mapping.buttons.iter_mut().find(|b| b.xbox == name) {
                        entry.moza_button = n;
                    } else {
                        mapping.buttons.push(crate::config::MappedButton {
                            xbox: name.to_string(),
                            moza_button: n,
                        });
                    }
                    on_result(name, Some(n));
                    self.wait_for_release(PRESS_TIMEOUT);
                }
                None => on_result(name, None),
            }
        }
        mapping
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_detects_only_new_presses() {
        let prev = [false; 64];
        let mut cur = [false; 64];
        cur[0] = true; // button 1
        cur[4] = true; // button 5
        assert_eq!(diff_new_presses(&prev, &cur), vec![1, 5]);
    }

    #[test]
    fn diff_ignores_still_held_buttons() {
        // The held-button re-detect regression: a button already in prev must
        // not reappear even if the wheel keeps reporting it.
        let mut prev = [false; 64];
        prev[6] = true; // button 7 held since before
        let mut cur = [false; 64];
        cur[6] = true; // still held
        assert!(diff_new_presses(&prev, &cur).is_empty());
    }

    #[test]
    fn diff_detects_new_press_alongside_held() {
        let mut prev = [false; 64];
        prev[6] = true;
        let mut cur = [false; 64];
        cur[6] = true;
        cur[63] = true; // button 64 newly pressed
        assert_eq!(diff_new_presses(&prev, &cur), vec![64]);
    }

    #[test]
    fn diff_reports_released_then_repressed() {
        // A genuine release + re-press IS a new press (prev updated only on
        // real reports).
        let mut released = [false; 64];
        released[2] = false; // button 3 released; prev updated to this
        let mut cur = [false; 64];
        cur[2] = true; // pressed again -> new press
        assert_eq!(diff_new_presses(&released, &cur), vec![3]);
    }
}
