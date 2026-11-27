//! Small layout helpers shared by the section bodies (spec 5).

use eframe::egui;

use crate::gui::theme;

/// Width reserved for the label column of a labelled control row.
const LABEL_W: f32 = 100.0;
/// Room kept for the slider's value box.
const VALUE_W: f32 = 64.0;

/// A row with a left label and a slider that fills the rest of the width.
/// `add` receives the row's `Ui` and returns the slider's response.
pub fn slider_row(
    ui: &mut egui::Ui,
    label: &str,
    add: impl FnOnce(&mut egui::Ui) -> egui::Response,
) -> egui::Response {
    ui.horizontal(|ui| {
        label_cell(ui, label);
        ui.spacing_mut().slider_width = (ui.available_width() - VALUE_W).max(60.0);
        add(ui)
    })
    .inner
}

/// A row with a left label and a compact control (drag value, ...).
pub fn value_row(
    ui: &mut egui::Ui,
    label: &str,
    add: impl FnOnce(&mut egui::Ui) -> egui::Response,
) -> egui::Response {
    ui.horizontal(|ui| {
        label_cell(ui, label);
        add(ui)
    })
    .inner
}

fn label_cell(ui: &mut egui::Ui, label: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(LABEL_W, theme::MIN_INTERACT_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(LABEL_W);
            ui.label(label);
        },
    );
}

/// Sub-heading inside a section body.
pub fn subheading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.separator();
    ui.label(
        egui::RichText::new(text)
            .strong()
            .size(theme::SIZE_BODY)
            .color(theme::TEXT),
    );
}

/// Dismissible bad-coloured line (`prefix: message` with an x button).
pub fn error_line(ui: &mut egui::Ui, prefix: &str, error: &mut Option<String>) {
    let Some(err) = error.clone() else { return };
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(theme::BAD, format!("{prefix}: {err}"));
        if ui.small_button("\u{00d7}").clicked() {
            *error = None;
        }
    });
}
