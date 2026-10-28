//! Hover help for a field (#197): a small greyed-out icon placed **at the far
//! right** of the field it explains, which shows its reminder on hover.
//!
//! It replaces the collapsible "Formula syntax" memo that opened under the
//! fields: it was not clear which field it referred to, and it took up height
//! in windows that are already dense. The help is a shared building block —
//! a field that needs a reminder calls [`hover_help`] with the content of the
//! bubble, instead of copying the icon, tint and hover.
//!
//! A hover bubble (`Response::on_hover_ui`), not a popup window: the
//! properties window is a modal dialog where egui keeps only one popup window
//! at a time (#9), whereas a tooltip lives in its own layer.

use eframe::egui;

use crate::icons::{icon_image, Icon, IconRole};
use crate::theme;

/// Places the help icon at the end of the current row; `contents` paints the
/// bubble shown on hover. Returns the icon's response.
pub(crate) fn hover_help(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    ui.add(
        icon_image(Icon::Help, IconRole::Neutral)
            .tint(theme::help_icon_tint())
            .sense(egui::Sense::hover()),
    )
    .on_hover_ui(contents)
}

/// A field's reminder in one sentence (#225, S-M1): the "?" icon whose bubble
/// is only `text`.
pub(crate) fn help_text(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    let text = text.into();
    hover_help(ui, |ui| {
        ui.label(text);
    })
}

/// Paints a field (`add`) and places the "?" icon of its reminder right after,
/// on the same row (#225, S-M2: never a bare bubble on a field). Returns what
/// `add` returns, so that `.changed()` stays readable at the call site.
pub(crate) fn help_beside<R>(
    ui: &mut egui::Ui,
    text: impl Into<String>,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.horizontal(|ui| {
        let out = add(ui);
        help_text(ui, text);
        out
    })
    .inner
}
