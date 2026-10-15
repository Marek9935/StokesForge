//! La confirmation d'un export (ou d'une copie) de graphe qui montre des séries
//! pas à jour (#198, maquette M9 a).
//!
//! `AppState::export_unless_stale` met l'action en attente quand la figure
//! contient une série dont une colonne vient d'un calcul pas à jour ; cette
//! fenêtre propose d'exporter quand même (la figure garde ses séries grisées et
//! leur marqueur), sans l'avertissement, ou après avoir recalculé. Elle ne
//! s'ouvre jamais pour un graphe qui a désactivé l'avertissement
//! (`PlotSpec::stale_warning`, propriétés du graphe).

use eframe::egui;

use super::dialogs::{modal_dialog_with, DialogFooter, DialogOutcome};
use crate::state::{Action, AppState, StaleExport};
use crate::tr;

/// Peint la confirmation tant qu'un export attend, et pousse la réponse.
pub(super) fn stale_export_window(
    ctx: &egui::Context,
    state: &AppState,
    actions: &mut Vec<Action>,
) {
    let Some(count) = state.pending_stale_export() else {
        return;
    };
    let accept = tr!("stale-export-anyway");
    let accept_hint = tr!("stale-export-anyway", "hint");
    let mut chosen = None;
    let outcome = modal_dialog_with(
        ctx,
        egui::Id::new("stokesforge_stale_export"),
        &tr!("stale-export-title"),
        egui::vec2(560.0, 170.0),
        DialogFooter {
            accept_label: &accept,
            accept_hint: &accept_hint,
            accept_enabled: true,
            accept_disabled: None,
        },
        |ui| {
            if ui
                .button(tr!("stale-export-without-warning"))
                .on_hover_text(tr!("stale-export-without-warning", "hint"))
                .clicked()
            {
                chosen = Some(StaleExport::WithoutWarning);
            }
            if ui
                .button(tr!("stale-export-recalculate"))
                .on_hover_text(tr!("stale-export-recalculate", "hint"))
                .clicked()
            {
                chosen = Some(StaleExport::Recalculate);
            }
        },
        |ui| {
            ui.label(tr!(
                "stale-export-message",
                count = i64::try_from(count).unwrap_or(i64::MAX)
            ));
        },
    );
    let choice = chosen.or(match outcome {
        DialogOutcome::Accept => Some(StaleExport::Anyway),
        DialogOutcome::Cancel => Some(StaleExport::Cancel),
        DialogOutcome::Open => None,
    });
    if let Some(choice) = choice {
        actions.push(Action::ConfirmStaleExport(choice));
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{cliquer, contexte, image_de, painted, texte_exact, textes};
    use super::*;
    use crate::analysis::Preset;
    use stokesforge_analysis::model::Model;
    use stokesforge_core::{Column, ColumnMeta, ColumnRole, NodeId, Project};
    use stokesforge_plot::PlotType;
    use stokesforge_session::Session;

    /// Un graphe qui trace B et un ajustement manuel de B, rendu pas à jour par
    /// la saisie d'une cellule de B.
    fn banc(dir: &std::path::Path) -> (Session, AppState, NodeId) {
        let mut project = Project::new("Export").unwrap();
        let book = project.add_workbook(project.root(), Some("Book")).unwrap();
        let sheet = project.tree().children(book).unwrap()[0];
        let xs: Vec<f64> = (0..11).map(f64::from).collect();
        let ys: Vec<f64> = xs.iter().map(|x| 2.0 * x + 1.0).collect();
        let x = project
            .add_column(sheet, None, Some("A"), Column::from_f64(&xs))
            .unwrap();
        let y = project
            .add_column(sheet, None, Some("B"), Column::from_f64(&ys))
            .unwrap();
        project
            .set_column_meta(
                x,
                ColumnMeta {
                    role: ColumnRole::X,
                    ..ColumnMeta::default()
                },
            )
            .unwrap();
        let session = Session::spawn(project, None).unwrap();
        let mut state =
            AppState::new(session.handle(), dir.join("p.stf").display().to_string()).unwrap();
        state.dispatch(Action::PlotColumns {
            sheet,
            columns: vec![x, y],
            plot_type: PlotType::Line,
        });
        let graph = state.graphs()[0].0;
        state.dispatch(Action::OpenAnalysis {
            preset: Preset::Fit(Model::Linear),
            sheet,
            columns: vec![x, y],
        });
        {
            let form = state.analysis_form_mut().unwrap();
            form.auto_recalc = false;
            form.auto_recalc_chosen = true;
            form.add_to_graph = true;
            form.graph = Some(graph);
        }
        state.dispatch(Action::ApplyAnalysis);
        state.dispatch(Action::CommitCell {
            column: y,
            row: 0,
            text: "50".into(),
        });
        assert_eq!(state.stale_calculations().len(), 1);
        (session, state, graph)
    }

    fn spec_of(state: &AppState, graph: NodeId) -> Box<stokesforge_plot::PlotSpec> {
        match state.tree().node(graph).unwrap().kind() {
            stokesforge_core::NodeKind::Graph { spec, .. } => Box::new(spec.clone()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn exporter_un_graphe_pas_a_jour_demande_d_abord_puis_exporte_selon_la_reponse() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, graph) = banc(dir.path());
        let spec = spec_of(&state, graph);
        let svg = dir.path().join("a.svg");
        state.dispatch(Action::ExportGraph {
            path: svg.clone(),
            spec: spec.clone(),
        });
        assert!(!svg.exists(), "rien n'est écrit avant la réponse");
        assert_eq!(state.pending_stale_export(), Some(1));

        // Annuler : rien n'est écrit, plus rien n'attend.
        state.dispatch(Action::ConfirmStaleExport(StaleExport::Cancel));
        assert!(!svg.exists() && state.pending_stale_export().is_none());

        // Exporter quand même : la série pas à jour est grisée (#9a9a9a).
        state.dispatch(Action::ExportGraph {
            path: svg.clone(),
            spec: spec.clone(),
        });
        state.dispatch(Action::ConfirmStaleExport(StaleExport::Anyway));
        let with_warning = std::fs::read_to_string(&svg).unwrap();
        assert!(
            with_warning.contains("#9A9A9A")
                || with_warning.contains("#9a9a9a")
                || with_warning.to_lowercase().contains("9a9a9a"),
            "série grisée attendue"
        );

        // Sans avertissement : aucune série grisée.
        let plain = dir.path().join("b.svg");
        state.dispatch(Action::ExportGraph {
            path: plain.clone(),
            spec: spec.clone(),
        });
        state.dispatch(Action::ConfirmStaleExport(StaleExport::WithoutWarning));
        let without = std::fs::read_to_string(&plain).unwrap();
        assert!(
            !without.to_lowercase().contains("9a9a9a"),
            "témoin : pas de gris"
        );

        // Recalculer puis exporter : le calcul est à jour, la figure aussi.
        let fresh = dir.path().join("c.svg");
        state.dispatch(Action::ExportGraph {
            path: fresh.clone(),
            spec,
        });
        state.dispatch(Action::ConfirmStaleExport(StaleExport::Recalculate));
        assert!(state.stale_calculations().is_empty());
        assert!(!std::fs::read_to_string(&fresh)
            .unwrap()
            .to_lowercase()
            .contains("9a9a9a"));
    }

    #[test]
    fn un_graphe_qui_a_desactive_l_avertissement_s_exporte_sans_demander() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, graph) = banc(dir.path());
        let mut spec = spec_of(&state, graph);
        spec.stale_warning = false;
        let svg = dir.path().join("a.svg");
        state.dispatch(Action::ExportGraph {
            path: svg.clone(),
            spec,
        });
        assert!(svg.exists());
        assert!(state.pending_stale_export().is_none());
        assert!(!std::fs::read_to_string(&svg)
            .unwrap()
            .to_lowercase()
            .contains("9a9a9a"));
    }

    #[test]
    fn la_fenetre_propose_trois_issues_et_annuler() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, graph) = banc(dir.path());
        state.dispatch(Action::CopyGraphFigure {
            spec: spec_of(&state, graph),
        });
        let ctx = contexte();
        let mut actions = Vec::new();
        let mut output = egui::FullOutput::default();
        for _ in 0..2 {
            output = image_de(&ctx, egui::vec2(1200.0, 800.0), Vec::new(), |ui| {
                stale_export_window(ui.ctx(), &state, &mut actions);
            });
        }
        for label in [
            tr!("stale-export-anyway"),
            tr!("stale-export-without-warning"),
            tr!("stale-export-recalculate"),
            tr!("dialog-cancel"),
        ] {
            assert!(painted(&output, &label), "bouton {label:?}");
        }
        let pos = texte_exact(&textes(&output.shapes), &tr!("stale-export-recalculate")).centre;
        cliquer(&ctx, pos, egui::PointerButton::Primary, |ui| {
            stale_export_window(ui.ctx(), &state, &mut actions);
        });
        assert_eq!(
            actions,
            [Action::ConfirmStaleExport(StaleExport::Recalculate)]
        );
    }

    #[test]
    fn l_object_manager_marque_la_serie_pas_a_jour_sauf_si_le_graphe_y_renonce() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, state, graph) = banc(dir.path());
        let marques = |state: &AppState| -> Vec<bool> {
            crate::object_manager::object_tree(state.tree(), graph, None)
                .unwrap()
                .layers[0]
                .series
                .iter()
                .map(|s| s.stale)
                .collect()
        };
        // Les deux premières séries sont les données, la dernière est l'ajustement.
        let avec = marques(&state);
        assert_eq!(avec.iter().filter(|s| **s).count(), 1);
        assert!(*avec.last().unwrap());
        // Témoin : sans avertissement, plus aucune marque.
        let mut spec = spec_of(&state, graph);
        spec.stale_warning = false;
        let mut state = state;
        state.dispatch(Action::SetPlotSpec { graph, spec: *spec });
        assert!(marques(&state).iter().all(|s| !s));
    }
}
