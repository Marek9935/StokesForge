//! La fenêtre de récupération (#317), ouverte au démarrage quand des copies de
//! secours de processus disparus sont plus récentes que leurs `.stf`.
//!
//! Lecture seule (`[Close]`, S-F2) dont chaque ligne porte ses boutons :
//! `Open` charge la copie dans la fenêtre courante (derrière « Enregistrer ? »
//! s'il y a du travail) et la liste reste ouverte pour les autres, `Ignore`
//! laisse la copie sur disque (reproposée au démarrage suivant), `Delete`
//! demande confirmation (`confirmations.rs`, la suppression ne s'annule pas).
//! `Close` ignore ce qui reste. La liste vient de `AppState::recovery_rows` ;
//! la fenêtre ne décide rien. Jumelle de référence : `column_use.rs`.

use eframe::egui;

use super::dialogs::{modal_read_only, DialogOutcome, PlainSize};
use stokesforge_app_state::state::{Action, AppState, RecoveryRow};
use stokesforge_app_state::tr;

/// Peint la fenêtre tant qu'il reste des copies à proposer, et pousse la
/// réponse.
pub(super) fn recovery_window(ctx: &egui::Context, state: &AppState, actions: &mut Vec<Action>) {
    let Some(rows) = state.recovery_rows() else {
        return;
    };
    let mut requested = Vec::new();
    let outcome = modal_read_only(
        ctx,
        egui::Id::new("stokesforge_recovery"),
        &tr!("recovery-title"),
        PlainSize::Content(egui::vec2(560.0, 230.0)),
        |ui| {
            ui.label(tr!("recovery-message"));
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    for row in &rows {
                        row_buttons(ui, row, &mut requested);
                    }
                });
        },
    );
    actions.extend(requested);
    if outcome != DialogOutcome::Open {
        actions.push(Action::CloseRecovery);
    }
}

/// Une copie : son nom et sa date, puis `Open · Ignore · Delete` à droite.
fn row_buttons(ui: &mut egui::Ui, row: &RecoveryRow, requested: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        ui.label(&row.text);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Posés de droite à gauche : à l'écran, Open · Ignore · Delete.
            if ui
                .button(tr!("recovery-delete"))
                .on_hover_text(tr!("recovery-delete", "hint"))
                .clicked()
            {
                requested.push(Action::RecoveryDelete(row.path.clone()));
            }
            if ui
                .button(tr!("recovery-ignore"))
                .on_hover_text(tr!("recovery-ignore", "hint"))
                .clicked()
            {
                requested.push(Action::RecoveryIgnore(row.path.clone()));
            }
            if ui
                .button(tr!("recovery-open"))
                .on_hover_text(tr!("recovery-open", "hint"))
                .clicked()
            {
                requested.push(Action::RecoveryOpen(row.path.clone()));
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    struct Banc {
        _session: stokesforge_session::Session,
        _dir: tempfile::TempDir,
        state: AppState,
    }

    /// Deux copies de processus disparus (une avec chemin d'origine, une sans),
    /// listées au démarrage.
    fn banc() -> egui_kittest::Harness<'static, Banc> {
        let dir = tempfile::tempdir().unwrap();
        let session = stokesforge_session::Session::spawn(
            stokesforge_core::Project::new("Secours").unwrap(),
            None,
        )
        .unwrap();
        let mut state = AppState::new_with_preferences(
            session.handle(),
            String::new(),
            stokesforge_app_state::preferences::Preferences::load_from(None),
        )
        .unwrap();
        let snapshot = session.handle().snapshot().unwrap();
        let recovery = dir.path().join("recovery");
        stokesforge_app_state::recovery::write_copy(&recovery, "/x/mesures.stf", &snapshot)
            .unwrap();
        stokesforge_app_state::recovery::write_copy(&recovery, "", &snapshot).unwrap();
        state.set_recovery_dir(Some(recovery));
        state.scan_recovery();
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(800.0, 600.0))
            .build_ui_state(
                |ui, banc: &mut Banc| {
                    let mut actions = Vec::new();
                    recovery_window(ui.ctx(), &banc.state, &mut actions);
                    for action in actions {
                        banc.state.dispatch(action);
                    }
                },
                Banc {
                    _session: session,
                    _dir: dir,
                    state,
                },
            );
        harness
            .ctx
            .set_fonts(stokesforge_render_egui::font_definitions());
        crate::theme::install(&harness.ctx);
        harness.run();
        harness
    }

    fn fichiers(harness: &egui_kittest::Harness<'static, Banc>) -> usize {
        let dir = harness.state()._dir.path().join("recovery");
        std::fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "stf"))
            .count()
    }

    /// La fenêtre peint son titre, ses deux lignes (nom ou Untitled, date) et
    /// leurs trois boutons.
    #[test]
    fn la_fenetre_peint_ses_lignes_et_ses_boutons() {
        let harness = banc();
        harness.get_by_label(&tr!("recovery-message"));
        let rows = harness.state().state.recovery_rows().expect("deux copies");
        assert_eq!(rows.len(), 2);
        for row in &rows {
            harness.get_by_label(&row.text);
        }
        assert!(
            rows.iter().any(|r| r.text.contains("mesures.stf")),
            "{rows:?}"
        );
        assert!(
            rows.iter()
                .any(|r| r.text.contains(&tr!("recovery-untitled"))),
            "{rows:?}"
        );
        for bouton in [
            tr!("recovery-open"),
            tr!("recovery-ignore"),
            tr!("recovery-delete"),
        ] {
            assert_eq!(
                harness.get_all_by_label(&bouton).count(),
                2,
                "un {bouton} par ligne"
            );
        }
    }

    /// Open charge la copie et la liste reste ouverte pour l'autre.
    #[test]
    fn open_charge_la_copie_et_garde_la_liste_ouverte() {
        let mut harness = banc();
        harness
            .get_all_by_label(&tr!("recovery-open"))
            .next()
            .unwrap()
            .click();
        harness.run();
        assert_eq!(
            harness.state().state.recovery_rows().map(|r| r.len()),
            Some(1),
            "l'autre copie reste proposée"
        );
        assert!(harness.state().state.alert().is_none());
    }

    /// Ignore retire la ligne sans toucher le fichier ; Delete passe par la
    /// confirmation (rien n'est effacé avant elle).
    #[test]
    fn ignore_garde_le_fichier_et_delete_attend_sa_confirmation() {
        let mut harness = banc();
        assert_eq!(fichiers(&harness), 2);
        harness
            .get_all_by_label(&tr!("recovery-ignore"))
            .next()
            .unwrap()
            .click();
        harness.run();
        assert_eq!(harness.state().state.recovery_rows().unwrap().len(), 1);
        assert_eq!(fichiers(&harness), 2, "Ignore garde le fichier");
        harness
            .get_all_by_label(&tr!("recovery-delete"))
            .next()
            .unwrap()
            .click();
        harness.run();
        assert!(harness.state().state.pending_recovery_delete().is_some());
        assert_eq!(
            fichiers(&harness),
            2,
            "rien n'est effacé avant la confirmation"
        );
    }
}
