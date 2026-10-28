//! The "column used by a calculation" window (#306), opened **after** the
//! deletion of columns (or of a sheet, a workbook) that a formula column or an
//! analysis calculation reads.
//!
//! Read-only (`[Undo] [Close]`, S-F2): `Close` does nothing, `Undo` undoes the
//! gesture. The gesture is never blocked beforehand. The list comes from
//! `AppState::column_use_lines`, computed before the deletion.

use eframe::egui;

use super::dialogs::{
    modal_dialog_with, modal_read_only_with, DialogFooter, DialogNature, DialogOutcome, PlainSize,
};
use stokesforge_app_state::state::{Action, AppState, FullColumnsCopy};
use stokesforge_app_state::tr;

/// Paints the window while it is open, and pushes the answer.
pub(super) fn column_use_window(ctx: &egui::Context, state: &AppState, actions: &mut Vec<Action>) {
    let Some(lines) = state.column_use_lines() else {
        return;
    };
    let mut undo = false;
    let outcome = modal_read_only_with(
        ctx,
        egui::Id::new("stokesforge_column_use"),
        &tr!("column-use-title"),
        PlainSize::Content(egui::vec2(480.0, 170.0)),
        |ui| {
            if ui
                .button(tr!("dialog-undo"))
                .on_hover_text(tr!("dialog-undo", "hint"))
                .clicked()
            {
                undo = true;
            }
        },
        |ui| {
            ui.label(tr!(
                "column-use-message",
                count = i64::try_from(lines.len()).unwrap_or(i64::MAX)
            ));
            for line in &lines {
                ui.label(format!("• {line}"));
            }
        },
    );
    if undo {
        actions.push(Action::UndoColumnUse);
    } else if outcome != DialogOutcome::Open {
        actions.push(Action::CloseColumnUse);
    }
}

/// Paints the Ctrl+Shift+C question while it is pending (formula columns),
/// and pushes the answer: `[Values] [Cancel] [Formulas]`, Formulas by default
/// (the behavior before the question existed).
pub(super) fn full_copy_window(ctx: &egui::Context, state: &AppState, actions: &mut Vec<Action>) {
    if state.pending_full_copy().is_none() {
        return;
    }
    let formulas = tr!("full-copy-formulas");
    let formulas_hint = tr!("full-copy-formulas", "hint");
    let mut values = false;
    let outcome = modal_dialog_with(
        ctx,
        egui::Id::new("stokesforge_full_copy"),
        &tr!("full-copy-title"),
        DialogNature::Choice(PlainSize::Default),
        DialogFooter::always(&formulas, &formulas_hint),
        |ui| {
            if ui
                .button(tr!("full-copy-values"))
                .on_hover_text(tr!("full-copy-values", "hint"))
                .clicked()
            {
                values = true;
            }
        },
        |ui| {
            ui.label(tr!("full-copy-message"));
        },
    );
    let choice = if values {
        Some(FullColumnsCopy::Values)
    } else {
        match outcome {
            DialogOutcome::Accept => Some(FullColumnsCopy::Formulas),
            DialogOutcome::Cancel => Some(FullColumnsCopy::Cancel),
            DialogOutcome::Open => None,
        }
    };
    if let Some(choice) = choice {
        actions.push(Action::ConfirmFullColumnsCopy(choice));
    }
}
