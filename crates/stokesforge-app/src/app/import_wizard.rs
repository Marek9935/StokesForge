//! The import window (`Import…`, File menu): **one screen that creates**.
//! The file tabs, what was understood, the left panel (the format sentence,
//! the points to check, what was found) and the file cut into a grid you mark
//! directly. This module only paints [`AppState`] and turns events into calls
//! of it; every decision (what is a doubt, what a role does to the recipe,
//! which template fits) lives in `stokesforge-app-state`.
//! A single entry point for `app.rs`: [`import_window`], a `modal_dialog_with`
//! (never a raw `egui::Window`).

use eframe::egui;

mod grid;
mod templates;

use stokesforge_app_state::errors;
use stokesforge_app_state::state::{Action, AppState, GridCmd, LineRoleCmd};
use stokesforge_app_state::tr;
use stokesforge_import::sniff::{AutoWhat, SniffRefusal};
use stokesforge_import::{PropField, PropTarget};

use super::dialogs::{
    modal_dialog_with, DialogFooter, DialogNature, DialogOutcome, InteractiveSize,
};

/// Colour of a role (its button, its text in the grid and in the role popup).
fn role_color(role: LineRoleCmd, dark: bool) -> Option<egui::Color32> {
    use crate::theme::{import_color, ImportColor};
    Some(match role {
        LineRoleCmd::DataStart => crate::theme::accent(),
        LineRoleCmd::LongName => import_color(dark, ImportColor::LongName),
        LineRoleCmd::Units => import_color(dark, ImportColor::Units),
        LineRoleCmd::Comments => import_color(dark, ImportColor::Comments),
        LineRoleCmd::XPrime => import_color(dark, ImportColor::XPrime),
        LineRoleCmd::Names => import_color(dark, ImportColor::ColumnNames),
        LineRoleCmd::DataEnd => import_color(dark, ImportColor::Footer),
        LineRoleCmd::Clear => return None,
    })
}


/// Label of the role of a column in the result table.
fn column_role_label(role: stokesforge_import::ColumnRoleChoice) -> String {
    use stokesforge_import::ColumnRoleChoice;
    match role {
        ColumnRoleChoice::Auto => tr!("common-auto"),
        ColumnRoleChoice::X => "X".to_string(),
        ColumnRoleChoice::Y => "Y".to_string(),
        ColumnRoleChoice::XError => tr!("column-role-x-error"),
        ColumnRoleChoice::YError => tr!("column-role-y-error"),
        ColumnRoleChoice::Ignore => tr!("import-option-none"),
    }
}

/// Two options of the same variant, whatever they carry: choosing one again
/// keeps the text or number already typed.
fn same_variant<T>(a: &T, b: &T) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// Label of a forced encoding.
fn encoding_label(encoding: stokesforge_import::Encoding) -> String {
    use stokesforge_import::Encoding;
    // Encoding names are technical names: only "Auto" is translated.
    match encoding {
        Encoding::Auto => tr!("common-auto"),
        Encoding::Utf8 => "UTF-8".to_string(),
        Encoding::Windows1252 => "Windows-1252".to_string(),
        Encoding::Latin1 => "ISO 8859-1".to_string(),
        Encoding::Utf16Le => "UTF-16 LE".to_string(),
        Encoding::Utf16Be => "UTF-16 BE".to_string(),
    }
}

/// Letters of a column, A, B… Z, AA…
fn column_letters(mut index: usize) -> String {
    let mut letters = Vec::new();
    loop {
        letters.push((b'A' + (index % 26) as u8) as char);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    letters.iter().rev().collect()
}

/// "Result" tab: one row per imported column, from the real
/// parse of the file shown.
fn import_result_table(ui: &mut egui::Ui, state: &AppState) {
    let table = match state.import_preview() {
        None => {
            ui.weak(tr!("import-preview-empty"));
            return;
        }
        Some(Err(message)) => {
            ui.colored_label(
                ui.visuals().error_fg_color,
                errors::import::read_message(&message),
            );
            return;
        }
        Some(Ok(table)) => table,
    };
    let Some(import) = state.import_state() else {
        return;
    };
    ui.weak(tr!("import-result-count", count = table.columns.len()));
    let dash = "–";
    let opt = |text: &Option<String>| text.clone().unwrap_or_else(|| dash.to_string());
    // RAW-SCROLL(#347): table preview, rows are the content: no end margin
    egui::ScrollArea::both()
        .id_salt("stokesforge_import_result_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("stokesforge_import_result_grid")
                .striped(true)
                .spacing([14.0, 3.0])
                .show(ui, |ui| {
                    for title in [
                        "#".to_string(),
                        tr!("column-long-name"),
                        tr!("column-units"),
                        tr!("column-comments"),
                        tr!("column-x-prime"),
                        tr!("column-role"),
                        tr!("import-columns-values"),
                    ] {
                        ui.label(crate::theme::table_header(title));
                    }
                    ui.end_row();
                    for (index, column) in table.columns.iter().enumerate() {
                        ui.label(column_letters(index));
                        ui.label(opt(&column.long_name));
                        ui.label(opt(&column.unit));
                        ui.label(opt(&column.comment));
                        ui.label(
                            column
                                .x_prime
                                .map_or(dash.to_string(), |value| format!("{value}")),
                        );
                        ui.label(column_role_label(import.recipe.column_role(index)));
                        ui.label(column.values.len().to_string());
                        ui.end_row();
                    }
                });
        });
}

// -- Automatic choices ---------------------------------------------------

const PILL_WIDTH: f32 = 34.0;

fn prop_field_label(field: PropField) -> String {
    match field {
        PropField::LongName => tr!("column-long-name"),
        PropField::Units => tr!("column-units"),
        PropField::Comment => tr!("column-comments"),
    }
}

fn prop_target_label(target: PropTarget) -> String {
    match target {
        PropTarget::X => "X".to_string(),
        PropTarget::Y => "Y".to_string(),
        PropTarget::All => tr!("import-auto-target-all"),
    }
}

fn prop_color(dark: bool, field: PropField) -> egui::Color32 {
    use crate::theme::{import_color, ImportColor};
    import_color(
        dark,
        match field {
            PropField::LongName => ImportColor::LongName,
            PropField::Units => ImportColor::Units,
            PropField::Comment => ImportColor::Comments,
        },
    )
}

/// The preview of a file that is not readable as a table: a message card.
fn refused_card(ui: &mut egui::Ui, refusal: SniffRefusal) {
    let (title, text) = match refusal {
        SniffRefusal::Binary => (
            tr!("import-mark-refused-binary"),
            tr!("import-refused-binary"),
        ),
        SniffRefusal::Empty => (
            tr!("import-mark-refused-empty"),
            tr!("import-refused-empty"),
        ),
        SniffRefusal::NoNumbers => (
            tr!("import-mark-refused-numbers"),
            tr!("import-refused-numbers"),
        ),
        SniffRefusal::Nmr => (tr!("import-mark-refused-nmr"), tr!("import-refused-nmr")),
    };
    let red = ui.visuals().error_fg_color;
    ui.vertical_centered(|ui| {
        ui.add_space(crate::theme::SPACE_SECTION * 12.0);
        egui::Frame::new()
            .fill(red.gamma_multiply(0.08))
            .stroke(egui::Stroke::new(1.0, red))
            .corner_radius(6.0)
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                ui.set_max_width(420.0);
                ui.colored_label(red, crate::theme::bold(title));
                ui.label(text);
            });
    });
}

/// What the window asks of the application after a frame, once the borrow of
/// the state is released.
#[derive(Default)]
struct ImportWizardRequests {
    cancel: bool,
    import: bool,
    /// File pickers requested (#192), pushed as actions.
    picks: Vec<Action>,
    /// Automatic choices taken back by their pill in the grid .
    auto: Vec<AutoWhat>,
    /// Commands of the cut sheet grid .
    grid: Vec<GridCmd>,
    /// Clicks of the left panel (review, templates).
    checks: Vec<grid::CheckCmd>,
}

/// The import window (« Import… », File menu): **one screen that creates**.
/// The file tabs, what was understood, the left panel (the format sentence, the
/// points to check, what was found) and the file cut into a grid you mark
/// directly; « Import » is always there, the doubts are a warning in the footer,
/// not a gate. A draft dialog of the common building block ([`modal_dialog_with`]),
/// painted from [`StokesForgeApp::ui`](eframe::App::ui) as long as
/// [`AppState::import_state`] is `Some`; the close cross and Escape mean
/// "Cancel", Enter "Import". Footer (S-F6): Destination and Template on the
/// left, the message area, Cancel, Import. The windows of the templates are
/// painted **inside** the content (see [`templates::windows`]) to read Enter and
/// Escape before it.
pub(super) fn import_window(ctx: &egui::Context, state: &mut AppState, actions: &mut Vec<Action>) {
    if state.import_state().is_none() {
        return;
    }
    let has_files = state
        .import_state()
        .is_some_and(|import| !import.paths.is_empty());
    // The two closures of the building block (content, footer buttons) write
    // into the same requests, one after the other: `RefCell`.
    let requests = std::cell::RefCell::new(ImportWizardRequests::default());
    // Escape clears the selection of the grid first.
    grid::grid_escape(ctx);
    // Doubts left do not block the import: the footer says so.
    let doubts = state.import_check().map_or(0, |view| view.n_check);
    let reviewing = state.check_review_kind().is_some();
    let warning = if doubts > 0 && !reviewing {
        tr!("import-footer-unchecked", count = doubts)
    } else {
        String::new()
    };
    let finish_label = tr!("import-grid-import");
    let finish_hint = tr!("import-button-finish", "hint");
    let add_file_first = tr!("disabled-add-file-first");
    let state_cell = std::cell::RefCell::new(&mut *state);
    let outcome = modal_dialog_with(
        ctx,
        egui::Id::new("stokesforge_import_wizard"),
        &tr!("import-window-title"),
        // FREE-SIZE(#300): the one-screen import (tabs, left panel, grid, result line) needs the room; a smaller screen shrinks it
        DialogNature::Draft(InteractiveSize::Free(egui::vec2(1320.0, 840.0))),
        DialogFooter::gated(&finish_label, &finish_hint, has_files, &add_file_first)
            .with_warning(&warning),
        |footer| {
            let mut guard = state_cell.borrow_mut();
            let state: &mut AppState = &mut guard;
            let mut requests = requests.borrow_mut();
            grid::footer_buttons(footer, state, &mut requests);
        },
        |ui| {
            let mut requests = requests.borrow_mut();
            let mut guard = state_cell.borrow_mut();
            let state: &mut AppState = &mut guard;
            grid::import_screen(ui, state, &mut requests);
            templates::windows(ui.ctx(), state);
        },
    );
    // Previews are recomputed after a pause in typing (S-T1, #224).
    let now = ctx.input(|input| input.time);
    if let Some(wait) = state
        .import_state_mut()
        .and_then(|import| import.debounce_preview(ctx.text_edit_focused(), now))
    {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait + 0.01));
    }
    let mut requests = requests.into_inner();
    match outcome {
        DialogOutcome::Open => {}
        DialogOutcome::Accept => requests.import = true,
        DialogOutcome::Cancel => requests.cancel = true,
    }

    // Everything that needs the whole state is applied here, once the window
    // is painted.
    actions.append(&mut requests.picks);
    // The pills of the grid take an automatic choice back.
    for what in std::mem::take(&mut requests.auto) {
        state.import_auto_undo(&what);
    }
    for cmd in std::mem::take(&mut requests.grid) {
        state.import_grid_cmd(cmd);
    }
    for cmd in std::mem::take(&mut requests.checks) {
        grid::apply_check(state, cmd);
    }
    if requests.import {
        if let Some(action) = state.import_finish() {
            actions.push(action);
        }
    } else if requests.cancel {
        state.cancel_import();
    }
}
