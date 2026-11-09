//! The import window.
//!
//! // #488 L5: provisional. The six-step wizard is gone; this window lists the
//! files and imports them, each with the recipe the state chose for it
//! (`AppState::import_finish`). L5 replaces it by the one-screen import
//! (sentences of what was understood, "To check", the file as a grid), which
//! only paints `stokesforge_app_state::state` and turns events into its calls.

use eframe::egui;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

use super::dialogs::{
    modal_dialog_with, DialogFooter, DialogNature, DialogOutcome, InteractiveSize,
};

/// The import window, painted from [`StokesForgeApp::ui`](eframe::App::ui) as
/// long as [`AppState::import_state`] is `Some`; the close cross and Escape
/// mean "Cancel", Enter "Import".
pub(super) fn import_window(_ctx: &egui::Context, state: &mut AppState, actions: &mut Vec<Action>) {
    // #488 L5: the one-screen import replaces this window.
    let ctx = _ctx;
    let Some(import) = state.import_state() else {
        return;
    };
    let names: Vec<String> = import
        .paths
        .iter()
        .map(|path| {
            path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            )
        })
        .collect();
    let has_files = !names.is_empty();
    let to_check = state.to_check_count();
    let finish_label = tr!("import-button-finish");
    let finish_hint = tr!("import-button-finish", "hint");
    let add_file_first = tr!("disabled-add-file-first");
    let outcome = modal_dialog_with(
        ctx,
        egui::Id::new("stokesforge_import_wizard"),
        &tr!("import-window-title"),
        DialogNature::Draft(InteractiveSize::Free(egui::vec2(560.0, 360.0))),
        DialogFooter::gated(&finish_label, &finish_hint, has_files, &add_file_first),
        |_footer| {},
        |ui| {
            for name in &names {
                ui.label(name);
            }
            if to_check > 0 {
                ui.label(to_check.to_string());
            }
        },
    );
    match outcome {
        DialogOutcome::Open => {}
        DialogOutcome::Accept => {
            if let Some(action) = state.import_finish() {
                actions.push(action);
            }
        }
        DialogOutcome::Cancel => state.cancel_import(),
    }
}
