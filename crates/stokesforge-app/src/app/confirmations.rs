//! The application's **blocking** dialogs (#229,
//! `docs/interface-standard.md`, § Confirmations): while one waits for its
//! answer, it is the only one acting in the frame (`StokesForgeApp::
//! blocking_confirmations` empties the other actions).
//!
//! Classification of the dialogs that ask something (S-F3):
//!
//! - **confirmations** — only what Ctrl+Z does not recover: unsaved project
//!   (New Project, Open…, Quit, window close:
//!   "Discard Changes · Cancel · Save"), export that would overwrite a file
//!   the native picker did not see (here), deletion of an import template
//!   (`import_wizard`). An undoable deletion (sheet, Project Manager objects)
//!   asks nothing;
//! - **choices** — a question where no answer is "yes": the X axis of a row
//!   drop (here), deletion of a computation with or without its columns
//!   (`analysis_dialog`), export of stale series (`stale_export`). They keep
//!   the shape of dialogs (`modal_dialog_with`);
//! - **serious failure** (S-M8) — failed save, unreadable project: message,
//!   Close and one possible action.

use eframe::egui;

use stokesforge_app_state::state::{
    Action, AlertKind, AppState, RowAxis, UnsavedChoice, UnsavedGesture,
};
use stokesforge_app_state::tr;

use super::dialogs::{
    modal_alert, modal_confirm, modal_dialog_with, modal_read_only, modal_save_changes,
    AlertOutcome, ConfirmOutcome, DialogFooter, DialogNature, DialogOutcome, PlainSize,
    SaveChangesOutcome, SizeClass,
};

// All blocking dialogs are of class Small (S-F11), the default size of their
// nature, except two read-only ones that ask for it.

/// Paints the pending blocking dialog, if there is one, and pushes its answer.
pub(super) fn show(ctx: &egui::Context, state: &AppState, actions: &mut Vec<Action>) {
    if let Some(gesture) = state.unsaved_pending() {
        unsaved_dialog(ctx, gesture, actions);
    }
    if let Some(alert) = state.alert().filter(|a| a.kind == AlertKind::LowRSquared) {
        // A warning, not a failure (#288): nothing to retry, Close alone
        // (Enter, Escape, cross) closes it.
        let outcome = modal_read_only(
            ctx,
            egui::Id::new("stokesforge_alert"),
            &tr!("alert-low-r2-title"),
            PlainSize::Class(SizeClass::Small),
            |ui| {
                ui.label(tr!("alert-low-r2"));
                // Normal text: the counters are the content of the box.
                ui.label(&alert.message);
            },
        );
        if outcome != DialogOutcome::Open {
            actions.push(Action::DismissAlert);
        }
    } else if let Some(alert) = state.alert().filter(|a| a.kind == AlertKind::SessionLost) {
        // The session is dead (#310): the model is lost, Close closes the
        // application (`Action::DismissAlert`). The message says where the
        // backup copy is.
        let outcome = modal_read_only(
            ctx,
            egui::Id::new("stokesforge_alert"),
            &tr!("alert-session-lost-title"),
            PlainSize::Class(SizeClass::Small),
            |ui| {
                ui.label(tr!("alert-session-lost"));
                ui.label(egui::RichText::new(&alert.message).weak());
            },
        );
        if outcome != DialogOutcome::Open {
            actions.push(Action::DismissAlert);
        }
    } else if let Some(alert) = state.alert() {
        let (title, intro, action, retry) = match alert.kind {
            AlertKind::SaveFailed => (
                tr!("alert-save-failed-title"),
                tr!("alert-save-failed"),
                (tr!("alert-save-as"), tr!("alert-save-as", "hint")),
                Action::PickFile(
                    stokesforge_app_state::file_picker::PickRequest::SaveProjectAs {
                        suggested: super::suggested_file_name(state.path()),
                    },
                ),
            ),
            // Handled above: neither the warning nor the lost session has an
            // action to retry.
            AlertKind::LowRSquared | AlertKind::SessionLost => return,
            AlertKind::OpenFailed => (
                tr!("alert-open-failed-title"),
                tr!("alert-open-failed"),
                (tr!("alert-open-another"), tr!("alert-open-another", "hint")),
                Action::PickFile(stokesforge_app_state::file_picker::PickRequest::OpenProject),
            ),
        };
        match modal_alert(
            ctx,
            egui::Id::new("stokesforge_alert"),
            &title,
            PlainSize::Default,
            (&action.0, &action.1),
            |ui| {
                ui.label(intro);
                ui.label(egui::RichText::new(&alert.message).weak());
            },
        ) {
            AlertOutcome::Open => {}
            AlertOutcome::Close => actions.push(Action::DismissAlert),
            AlertOutcome::Action => {
                actions.push(Action::DismissAlert);
                actions.push(retry);
            }
        }
    }
    if let Some(path) = state.pending_overwrite() {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        match modal_confirm(
            ctx,
            egui::Id::new("stokesforge_overwrite"),
            &tr!("overwrite-title"),
            PlainSize::Default,
            &tr!("overwrite-replace"),
            &tr!("overwrite-replace", "hint"),
            |ui| {
                ui.label(tr!("overwrite-confirm", name = name.as_str()));
            },
        ) {
            ConfirmOutcome::Open => {}
            ConfirmOutcome::Confirm => actions.push(Action::ConfirmOverwrite(true)),
            ConfirmOutcome::Cancel => actions.push(Action::ConfirmOverwrite(false)),
        }
    }
    if let Some((name, what)) = state.pending_discarded_save() {
        match modal_confirm(
            ctx,
            egui::Id::new("stokesforge_discarded_save"),
            &tr!("discarded-save-title"),
            PlainSize::Default,
            &tr!("discarded-save-replace"),
            &tr!("discarded-save-replace", "hint"),
            |ui| {
                ui.label(tr!(
                    "discarded-save-confirm",
                    name = name.as_str(),
                    what = what.as_str()
                ));
            },
        ) {
            ConfirmOutcome::Open => {}
            ConfirmOutcome::Confirm => actions.push(Action::ConfirmDiscardedSave(true)),
            ConfirmOutcome::Cancel => actions.push(Action::ConfirmDiscardedSave(false)),
        }
    }
    if let Some(name) = state.pending_recovery_delete() {
        match modal_confirm(
            ctx,
            egui::Id::new("stokesforge_recovery_delete"),
            &tr!("recovery-delete-title"),
            PlainSize::Default,
            &tr!("recovery-delete"),
            &tr!("recovery-delete", "hint"),
            |ui| {
                ui.label(tr!("recovery-delete-confirm", name = name.as_str()));
            },
        ) {
            ConfirmOutcome::Open => {}
            ConfirmOutcome::Confirm => actions.push(Action::ConfirmRecoveryDelete(true)),
            ConfirmOutcome::Cancel => actions.push(Action::ConfirmRecoveryDelete(false)),
        }
    }
    if state.pending_row_drop().is_some() {
        row_drop_dialog(ctx, actions);
    }
}

/// "Discard Changes · Cancel · Save" (S-F4); Enter = Save.
fn unsaved_dialog(ctx: &egui::Context, gesture: &UnsavedGesture, actions: &mut Vec<Action>) {
    let question = match gesture {
        UnsavedGesture::NewProject => tr!("unsaved-new"),
        UnsavedGesture::Open(_) | UnsavedGesture::OpenRecovery(_) => tr!("unsaved-open"),
        UnsavedGesture::Quit => tr!("unsaved-quit"),
    };
    let choice = match modal_save_changes(
        ctx,
        egui::Id::new("stokesforge_unsaved"),
        &tr!("unsaved-title"),
        PlainSize::Default,
        |ui| {
            ui.label(question);
        },
    ) {
        SaveChangesOutcome::Open => return,
        SaveChangesOutcome::Discard => UnsavedChoice::Discard,
        SaveChangesOutcome::Cancel => UnsavedChoice::Cancel,
        SaveChangesOutcome::Save => UnsavedChoice::Save,
    };
    actions.push(Action::ConfirmUnsaved(choice));
}

/// The axis choice of a row drop (#166): a **choice**, not a
/// confirmation — "Create Axis" by default, "Use Present Axis" as the second
/// button, "Cancel" abandons the drop.
fn row_drop_dialog(ctx: &egui::Context, actions: &mut Vec<Action>) {
    let label = tr!("row-drop-new-axis");
    let hint = tr!("row-drop-new-axis", "hint");
    let mut present = false;
    match modal_dialog_with(
        ctx,
        egui::Id::new("stokesforge_row_drop"),
        &tr!("row-drop-title"),
        DialogNature::Choice(PlainSize::Default),
        DialogFooter::always(&label, &hint),
        |ui| {
            if ui
                .button(tr!("row-drop-present-axis"))
                .on_hover_text(tr!("row-drop-present-axis", "hint"))
                .clicked()
            {
                present = true;
            }
        },
        |ui| {
            ui.label(tr!("row-drop-confirm"));
        },
    ) {
        _ if present => actions.push(Action::ConfirmRowDrop(Some(RowAxis::Present))),
        DialogOutcome::Accept => actions.push(Action::ConfirmRowDrop(Some(RowAxis::New))),
        DialogOutcome::Cancel => actions.push(Action::ConfirmRowDrop(None)),
        DialogOutcome::Open => {}
    }
}
