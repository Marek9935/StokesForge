//! The confirmation of a graph export (or copy) that shows out-of-date series
//! (#198, mockup M9 a).
//!
//! `AppState::export_unless_stale` holds the action back when the figure
//! contains a series with a column that comes from an out-of-date
//! computation; this window offers to export anyway (the figure keeps its
//! series greyed out and their marker), to export without the warning, or to
//! export after recalculating. It never opens for a graph that turned the
//! warning off (`PlotSpec::stale_warning`, graph properties).

use eframe::egui;

use super::dialogs::{modal_dialog_with, DialogFooter, DialogNature, DialogOutcome, PlainSize};
use stokesforge_app_state::state::{Action, AppState, StaleExport};
use stokesforge_app_state::tr;

/// Paints the confirmation while an export is waiting, and pushes the answer.
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
        // Four buttons with long labels: wider than the Small class.
        DialogNature::Choice(PlainSize::Content(egui::vec2(560.0, 170.0))),
        DialogFooter::always(&accept, &accept_hint),
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
    use super::super::test_support::{click, context, frame_of, painted, text_exact, texts};
    use super::*;
    use stokesforge_analysis::model::Model;
    use stokesforge_app_state::analysis::Preset;
    use stokesforge_core::{Column, ColumnMeta, ColumnRole, NodeId, Project};
    use stokesforge_plot::PlotType;
    use stokesforge_session::Session;

    /// A graph that plots B and a manual fit of B, made out of date by typing
    /// into a cell of B.
    fn bench(dir: &std::path::Path) -> (Session, AppState, NodeId) {
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
    fn exporting_an_out_of_date_graph_asks_first_then_exports_according_to_the_answer() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, graph) = bench(dir.path());
        let spec = spec_of(&state, graph);
        let svg = dir.path().join("a.svg");
        state.dispatch(Action::ExportGraph {
            path: svg.clone(),
            spec: spec.clone(),
        });
        assert!(!svg.exists(), "nothing is written before the answer");
        assert_eq!(state.pending_stale_export(), Some(1));

        // Cancel: nothing is written, nothing is waiting any more.
        state.dispatch(Action::ConfirmStaleExport(StaleExport::Cancel));
        assert!(!svg.exists() && state.pending_stale_export().is_none());

        // Export anyway: the out-of-date series is greyed out (#9a9a9a).
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
            "greyed-out series expected"
        );

        // Without warning: no greyed-out series.
        let plain = dir.path().join("b.svg");
        state.dispatch(Action::ExportGraph {
            path: plain.clone(),
            spec: spec.clone(),
        });
        state.dispatch(Action::ConfirmStaleExport(StaleExport::WithoutWarning));
        let without = std::fs::read_to_string(&plain).unwrap();
        assert!(
            !without.to_lowercase().contains("9a9a9a"),
            "witness: no grey"
        );

        // Recalculate then export: the computation is up to date, and so is the figure.
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
    fn a_graph_that_turned_the_warning_off_exports_without_asking() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, graph) = bench(dir.path());
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
    fn the_window_offers_three_ways_out_and_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, graph) = bench(dir.path());
        state.dispatch(Action::CopyGraphFigure {
            spec: spec_of(&state, graph),
        });
        let ctx = context();
        let mut actions = Vec::new();
        let mut output = egui::FullOutput::default();
        for _ in 0..2 {
            output = frame_of(&ctx, egui::vec2(1200.0, 800.0), Vec::new(), |ui| {
                stale_export_window(ui.ctx(), &state, &mut actions);
            });
        }
        for label in [
            tr!("stale-export-anyway"),
            tr!("stale-export-without-warning"),
            tr!("stale-export-recalculate"),
            tr!("dialog-cancel"),
        ] {
            assert!(painted(&output, &label), "button {label:?}");
        }
        let pos = text_exact(&texts(&output.shapes), &tr!("stale-export-recalculate")).center;
        click(&ctx, pos, egui::PointerButton::Primary, |ui| {
            stale_export_window(ui.ctx(), &state, &mut actions);
        });
        assert_eq!(
            actions,
            [Action::ConfirmStaleExport(StaleExport::Recalculate)]
        );
    }

    #[test]
    fn the_object_manager_marks_the_out_of_date_series_unless_the_graph_opts_out() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, state, graph) = bench(dir.path());
        let marks = |state: &AppState| -> Vec<bool> {
            stokesforge_app_state::object_manager::object_tree(state.tree(), graph, None)
                .unwrap()
                .layers[0]
                .series
                .iter()
                .map(|s| s.stale)
                .collect()
        };
        // The first two series are the data, the last one is the fit.
        let with = marks(&state);
        assert_eq!(with.iter().filter(|s| **s).count(), 1);
        assert!(*with.last().unwrap());
        // Witness: without warning, no mark any more.
        let mut spec = spec_of(&state, graph);
        spec.stale_warning = false;
        let mut state = state;
        state.dispatch(Action::SetPlotSpec { graph, spec: *spec });
        assert!(marks(&state).iter().all(|s| !s));
    }
}
