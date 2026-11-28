//! Left column while the bridge is stopped: checklist and big Start (spec 4.3).

use eframe::egui;

use crate::gui::probe::checklist;
use crate::gui::widgets::checklist_row;
use crate::gui::{theme, MozaMapperApp};

const DRIVER_URL: &str = "https://github.com/nefarius/ViGEmBus/releases";
const MAX_WIDTH: f32 = 460.0;

pub fn show(app: &mut MozaMapperApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let list = checklist(app.probe.snapshot());
    let wizard = app.wizard_running();
    let mut start = false;

    ui.vertical_centered(|ui| {
        ui.add_space(((ui.available_height() - 400.0) / 3.0).max(8.0));
        let width = ui.available_width().min(MAX_WIDTH);
        ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
            ui.set_width(width);
            ui.label(
                egui::RichText::new(list.title)
                    .size(theme::SIZE_HEADING + 6.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.add_space(8.0);
            // Rows are laid out left-to-right inside, so reset the alignment.
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                for row in &list.rows {
                    if checklist_row(ui, row.level, row.title, row.detail, row.action) {
                        ctx.open_url(egui::OpenUrl::new_tab(DRIVER_URL));
                    }
                }
            });
            ui.add_space(16.0);

            let enabled = list.start_allowed && !wizard;
            let btn = egui::Button::new(
                egui::RichText::new("Start")
                    .size(theme::SIZE_HEADING)
                    .strong()
                    .color(if enabled { theme::ACCENT_TEXT } else { theme::TEXT_MUTED }),
            )
            .fill(if enabled { theme::ACCENT } else { theme::SURFACE })
            .min_size(egui::vec2(220.0, 52.0));
            let response = ui
                .add_enabled(enabled, btn)
                .on_disabled_hover_text(if wizard {
                    "Finish or cancel the mapping wizard first"
                } else {
                    "Fix the items above first"
                });
            if response.clicked() {
                start = true;
            }
            if !list.start_allowed {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Fix the items above first")
                        .size(theme::SIZE_SMALL)
                        .color(theme::TEXT_MUTED),
                );
            }
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new("Starts a virtual Xbox 360 controller driven by your wheel.")
                    .size(theme::SIZE_SMALL)
                    .color(theme::TEXT_MUTED),
            );
        });
    });

    if start {
        app.start_bridge();
    }
}
