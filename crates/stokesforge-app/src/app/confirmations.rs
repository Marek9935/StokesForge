//! Les dialogues **bloquants** de l'application (#229,
//! `docs/standard-interface.md`, § Confirmations) : tant que l'un attend sa
//! réponse, il est seul à agir dans l'image (`StokesForgeApp::
//! blocking_confirmations` vide les autres actions).
//!
//! Classement des dialogues qui demandent quelque chose (S-F3) :
//!
//! - **confirmations** — seulement ce que Ctrl+Z ne rattrape pas : projet non
//!   enregistré (New Project, Open…, Quit, fermeture de la fenêtre :
//!   « Discard Changes · Cancel · Save »), export qui écraserait un fichier
//!   que le sélecteur natif n'a pas vu (ici), suppression d'un modèle d'import
//!   (`import_wizard`). Une suppression annulable (feuille, objets du Project
//!   Manager) ne demande rien ;
//! - **choix** — une question dont aucune réponse n'est « oui » : l'axe X d'un
//!   dépôt de lignes (ici), la suppression d'un calcul avec ou sans ses
//!   colonnes (`analysis_dialog`), l'export de séries périmées
//!   (`stale_export`). Ils gardent la forme des dialogues
//!   (`modal_dialog_with`) ;
//! - **échec grave** (S-M8) — enregistrement raté, projet illisible : message,
//!   Close et une action possible.

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

// Tous les dialogues bloquants sont de classe Petite (S-F11), la taille par
// défaut de leur nature, sauf deux lectures seules qui la demandent.

/// Peint le dialogue bloquant en attente, s'il y en a un, et pousse sa réponse.
pub(super) fn show(ctx: &egui::Context, state: &AppState, actions: &mut Vec<Action>) {
    if let Some(gesture) = state.unsaved_pending() {
        unsaved_dialog(ctx, gesture, actions);
    }
    if let Some(alert) = state.alert().filter(|a| a.kind == AlertKind::LowRSquared) {
        // Un avertissement, pas un échec (#288) : rien à réessayer, Close seul
        // (Entrée, Échap, croix) le referme.
        let outcome = modal_read_only(
            ctx,
            egui::Id::new("stokesforge_alert"),
            &tr!("alert-low-r2-title"),
            PlainSize::Class(SizeClass::Small),
            |ui| {
                ui.label(tr!("alert-low-r2"));
                // Texte normal : les compteurs sont le contenu de la boîte.
                ui.label(&alert.message);
            },
        );
        if outcome != DialogOutcome::Open {
            actions.push(Action::DismissAlert);
        }
    } else if let Some(alert) = state.alert().filter(|a| a.kind == AlertKind::SessionLost) {
        // La session est morte (#310) : le modèle est perdu, Close ferme
        // l'application (`Action::DismissAlert`). Le message dit où est la copie
        // de secours.
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
            // Traités plus haut : ni l'avertissement ni la session perdue
            // n'ont d'action à relancer.
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

/// « Discard Changes · Cancel · Save » (S-F4) ; Entrée = Save.
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

/// Le choix d'axe d'un dépôt de lignes (#166) : un **choix**, pas une
/// confirmation — « Create Axis » par défaut, « Use Present Axis » en second
/// bouton, « Cancel » abandonne le dépôt.
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
