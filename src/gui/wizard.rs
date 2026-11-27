use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread;

use super::MozaMapperApp;

impl MozaMapperApp {
    pub(crate) fn wizard_running(&self) -> bool {
        self.wizard_thread.as_ref().is_some_and(|t| !t.is_finished())
    }

    pub(crate) fn start_wizard(&mut self) {
        if self.wizard_running() || self.running {
            return;
        }
        let prompt = Arc::clone(&self.wizard_prompt);
        let log = Arc::clone(&self.wizard_log);
        let result = Arc::clone(&self.wizard_result);
        // Base mapping comes from the GUI's live config (NOT a fresh disk
        // read) and the wizard writes nothing to disk itself — the GUI merges
        // and saves once. Two writers to app.json caused lost settings.
        let base_mapping = self.config.mapping.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.wizard_cancel = Some(Arc::clone(&cancel));

        let handle = thread::spawn(move || {
            let api = match hidapi::HidApi::new() {
                Ok(a) => a,
                Err(e) => {
                    log.lock().unwrap().push(format!("hidapi init failed: {}", e));
                    return;
                }
            };
            let (dev, layout) = match crate::hid::device::open_moza_with_layout(&api) {
                Ok(v) => v,
                Err(e) => {
                    log.lock().unwrap().push(format!("MOZA wheel not found: {}", e));
                    return;
                }
            };
            let mut wizard = crate::mapping::wizard::Wizard::new(dev, layout, cancel);
            let p = Arc::clone(&prompt);
            let l = Arc::clone(&log);
            let mapping = wizard.learn_all(
                base_mapping,
                move |name| *p.lock().unwrap() = format!("Press the wheel button for: {}", name),
                move |name, learned| match learned {
                    Some(n) => l.lock().unwrap().push(format!("{} -> button {}", name, n)),
                    None => l.lock().unwrap().push(format!("{}: skipped, kept previous", name)),
                },
            );
            *result.lock().unwrap() = Some(mapping);
            *prompt.lock().unwrap() = String::new();
        });
        self.wizard_thread = Some(handle);
    }
}
