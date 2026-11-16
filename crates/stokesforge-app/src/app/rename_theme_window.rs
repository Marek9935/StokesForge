//! The **Rename Theme** window (#542): the new name of a user or project theme.
//!
//! Type: **creation on a draft** (`DialogNature::Draft`, Small), twin of Save
//! Theme As: `[Cancel] [Rename]`, the button carries its verb and is greyed
//! with its reason while the name is empty. A refused name (taken, library
//! read-only) is said in the footer and the window stays open. The state is
//! `AppState::rename_theme_window`.

use eframe::egui;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

use super::dialogs::{
    modal_dialog_with, DialogFooter, DialogNature, DialogOutcome, InteractiveSize, SizeClass,
};
use super::form::form;
use crate::theme;

/// Paints the window while it is open.
pub(super) fn rename_theme_window(
    ctx: &egui::Context,
    state: &mut AppState,
    actions: &mut Vec<Action>,
) {
    let Some(opened) = state.rename_theme_window().cloned() else {
        return;
    };
    let mut window = opened.clone();
    let usable = !window.name.trim().is_empty();
    let verb = tr!("theme-rename-verb");
    let hint = tr!("theme-rename-verb", "hint");
    let reason = tr!("theme-save-name-needed");
    let outcome = modal_dialog_with(
        ctx,
        egui::Id::new("stokesforge_rename_theme"),
        &tr!("theme-rename-title"),
        DialogNature::Draft(InteractiveSize::Class(SizeClass::Small)),
        DialogFooter::gated(&verb, &hint, usable, &reason),
        |_| {},
        |ui| {
            form(ui, "rename_theme", |form| {
                form.row(tr!("theme-save-name"), |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut window.name)
                            .hint_text(tr!("theme-save-name-hint"))
                            .desired_width(theme::FieldWidth::Medium.points(ui)),
                    );
                });
            });
        },
    );
    if window != opened {
        if let Some(stored) = state.rename_theme_window_mut() {
            *stored = window.clone();
        }
    }
    match outcome {
        DialogOutcome::Open => {}
        DialogOutcome::Accept => actions.push(Action::RenameTheme {
            theme: window.theme.clone(),
            name: window.name.clone(),
        }),
        DialogOutcome::Cancel => actions.push(Action::CloseRenameTheme),
    }
}
