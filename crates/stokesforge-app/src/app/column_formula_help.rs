//! The "?" icon of the `F(x)=` cell of the workbook grid (#404, S-T6).
//!
//! While the cell is edited, a "?" icon is painted just right of it, over the
//! neighbouring column, so the text field keeps the whole width of the cell
//! (#410); a press on it opens
//! the **Formula Functions** window ([`super::formula_functions_window`], #402),
//! which lists the callable functions one family at a time. The list comes from
//! `stokesforge_app_state::formula_help`, itself built from the callees of the
//! formula parser: nothing is copied here.
//!
//! Not [`super::plot_settings`]'s `formula_help`, which is the Typst syntax
//! reminder of the title fields: another content, another preference
//! ([`AppState::show_column_formula_help`]).
//!
//! The icon never takes the keyboard focus: the cell closes when it loses it
//! (`FocusLoss::Ends`), and a press on the icon is a press outside the field,
//! which validates the cell like any other, then opens the window.
//!
//! [`AppState::show_column_formula_help`]: stokesforge_app_state::state::AppState::show_column_formula_help

use eframe::egui;
use stokesforge_app_state::state::Action;
use stokesforge_app_state::tr;

use crate::icons::{icon_image, Icon, IconRole, ICON_SIZE};
use crate::theme;

/// Paints the icon in a layer above the grid, just right of `cell` and
/// centred on its height: it overlaps the neighbouring column (or the empty
/// area after the last one) and the text field keeps the whole cell. The layer
/// keeps the icon inside the viewport, so the last column does not hide it.
/// Call it only in the frames where the cell is edited.
pub(super) fn show_beside(
    ctx: &egui::Context,
    id: egui::Id,
    cell: egui::Rect,
    actions: &mut Vec<Action>,
) {
    let pos = egui::pos2(cell.right(), cell.center().y - ICON_SIZE / 2.0);
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .constrain(true)
        .show(ctx, |ui| {
            icon(ui, actions);
        });
}

/// Paints the "?" icon at the current position; a click asks to open the
/// window of the functions.
pub(super) fn icon(ui: &mut egui::Ui, actions: &mut Vec<Action>) -> egui::Response {
    let response = ui
        .add(
            icon_image(Icon::Help, IconRole::Neutral)
                .tint(theme::help_icon_tint())
                .sense(egui::Sense::click()),
        )
        .on_hover_text(tr!("formula-help-icon", "hint"));
    // Opens on the **press**: the press already takes the focus away from the
    // cell, which ends its edit and removes the icon before the button is
    // released, so a plain `clicked()` would never fire.
    if response.clicked() || (response.hovered() && ui.input(|i| i.pointer.primary_pressed())) {
        actions.push(Action::OpenFormulaFunctions);
    }
    response
}
