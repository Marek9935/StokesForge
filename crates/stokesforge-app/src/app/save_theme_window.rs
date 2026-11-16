//! The **Save Theme As** window (#541): the name of the theme and where it goes,
//! Mine (the user's library on this machine, the default) or Project (the open
//! project, undoable).
//!
//! Type: **creation on a draft** (`DialogNature::Draft`, Small): `[Cancel]
//! [Save]`, the button carries its verb and is greyed with its reason while the
//! name is empty. A refused name (already used, library full) is said in the
//! footer and the window stays open. The state is `AppState::save_theme_window`.

use eframe::egui;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::theme_window::ThemeTarget;
use stokesforge_app_state::tr;

use super::dialogs::{
    modal_dialog_with, DialogFooter, DialogNature, DialogOutcome, InteractiveSize, SizeClass,
};
use super::form::form;
use super::hover_help::help_text;
use super::state_toggle::state_toggle;
use crate::theme;

const WINDOW_ID: &str = "stokesforge_save_theme";

/// Paints the window while it is open.
pub(super) fn save_theme_window(
    ctx: &egui::Context,
    state: &mut AppState,
    actions: &mut Vec<Action>,
) {
    let Some(opened) = state.save_theme_window().cloned() else {
        return;
    };
    let read_only = state.user_themes_read_only().is_some();
    let mut window = opened.clone();
    if read_only {
        // Mine cannot be written this session: the project is the only target.
        window.target = ThemeTarget::Project;
    }
    let usable = !window.name.trim().is_empty();
    let verb = tr!("theme-save-verb");
    let hint = tr!("theme-save-verb", "hint");
    let reason = tr!("theme-save-name-needed");
    let outcome = modal_dialog_with(
        ctx,
        egui::Id::new(WINDOW_ID),
        &tr!("theme-save-title"),
        DialogNature::Draft(InteractiveSize::Class(SizeClass::Small)),
        DialogFooter::gated(&verb, &hint, usable, &reason),
        |_| {},
        |ui| {
            form(ui, "save_theme", |form| {
                form.row(tr!("theme-save-name"), |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut window.name)
                            .hint_text(tr!("theme-save-name-hint"))
                            .desired_width(theme::FieldWidth::Medium.points(ui)),
                    );
                });
                form.row(tr!("theme-save-where"), |ui| {
                    let labels = [tr!("theme-save-mine"), tr!("theme-save-project")];
                    let selected = usize::from(window.target == ThemeTarget::Project);
                    let disabled = read_only.then(|| tr!("theme-save-mine-read-only"));
                    if state_toggle(
                        ui,
                        [&labels[0], &labels[1]],
                        selected,
                        &tr!("theme-save-where-hint"),
                        disabled.as_deref(),
                    ) {
                        window.target = if selected == 0 {
                            ThemeTarget::Project
                        } else {
                            ThemeTarget::Mine
                        };
                    }
                    help_text(ui, tr!("theme-save-where-hint"));
                });
            });
        },
    );
    if window != opened {
        if let Some(stored) = state.save_theme_window_mut() {
            *stored = window.clone();
        }
    }
    match outcome {
        DialogOutcome::Open => {}
        DialogOutcome::Accept => actions.push(Action::SaveThemeAs {
            graph: window.graph,
            name: window.name.clone(),
            target: window.target,
        }),
        DialogOutcome::Cancel => actions.push(Action::CloseSaveTheme),
    }
}
