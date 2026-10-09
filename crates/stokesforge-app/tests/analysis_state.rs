//! Les gestes d'analyse de `AppState` (#181), pilotés sans fenêtre : ouvrir la
//! fenêtre sur la sélection, valider (une entrée d'historique), modifier,
//! recalculer, détacher, supprimer avec confirmation.

use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::OperationKind;
use stokesforge_app::analysis::Preset;
use stokesforge_app::i18n::{self, Language};
use stokesforge_app::state::{Action, AppState, CalculationDelete};
use stokesforge_core::{Column, ColumnMeta, ColumnRole, NodeId, Project};
use stokesforge_session::Session;

struct Setup {
    _session: Session,
    state: AppState,
    sheet: NodeId,
    x: NodeId,
    y: NodeId,
}

/// Une feuille `A` (X), `B` (Y) : `y = 2x + 1` sur 11 points.
fn setup(dir: &std::path::Path) -> Setup {
    i18n::set_language(Language::English);
    let mut project = Project::new("P").unwrap();
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
    let state = AppState::new(
        session.handle(),
        dir.join("p.stokesforge").display().to_string(),
    )
    .unwrap();
    Setup {
        _session: session,
        state,
        sheet,
        x,
        y,
    }
}

fn open(s: &mut Setup, preset: Preset) {
    s.state.dispatch(Action::OpenAnalysis {
        preset,
        sheet: s.sheet,
        columns: vec![s.x, s.y],
    });
}

fn columns(s: &Setup) -> usize {
    s.state.tree().children(s.sheet).unwrap().len()
}

fn only_calculation(s: &Setup) -> NodeId {
    *s.state.tree().calculations().keys().next().unwrap()
}

#[test]
fn opening_without_a_y_column_says_why_and_opens_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    s.state.dispatch(Action::OpenAnalysis {
        preset: Preset::Integrate,
        sheet: s.sheet,
        columns: vec![s.x],
    });
    assert!(s.state.analysis_form().is_none());
    assert!(s.state.status().unwrap().is_error);
}

#[test]
fn opening_pairs_each_y_with_its_x_and_validating_is_one_undoable_step() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    open(&mut s, Preset::Fit(Model::Linear));
    let form = s.state.analysis_form().unwrap();
    assert_eq!(form.series.len(), 1);
    assert_eq!(form.series[0].x, Some(s.x));
    assert_eq!(form.series[0].y, s.y);

    s.state.analysis_form_mut().unwrap().setup.fit.residuals = true;
    s.state.dispatch(Action::ApplyAnalysis);
    assert!(s.state.analysis_form().is_none(), "the window closes");
    assert!(!s.state.status().unwrap().is_error);
    assert_eq!(s.state.tree().calculations().len(), 1);
    // x, y, résidus, ajustement.
    assert_eq!(columns(&s), 4);

    s.state.dispatch(Action::Undo);
    assert!(s.state.tree().calculations().is_empty());
    assert_eq!(columns(&s), 2);
    s.state.dispatch(Action::Redo);
    assert_eq!(columns(&s), 4);
}

#[test]
fn a_failing_calculation_keeps_the_window_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    // Une correction sans colonne de référence ne se valide pas.
    open(&mut s, Preset::Correct);
    s.state.dispatch(Action::ApplyAnalysis);
    assert!(s.state.analysis_form().is_some());
    assert!(s.state.status().unwrap().is_error);
    assert!(s.state.tree().calculations().is_empty());
    s.state.dispatch(Action::CloseAnalysis);
    assert!(s.state.analysis_form().is_none());
}

#[test]
fn editing_a_calculation_updates_it_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    open(&mut s, Preset::Integrate);
    s.state
        .analysis_form_mut()
        .unwrap()
        .setup
        .integrate
        .cumulative = true;
    s.state.dispatch(Action::ApplyAnalysis);
    let id = only_calculation(&s);
    let before = columns(&s);

    s.state.dispatch(Action::EditCalculation(id));
    let form = s.state.analysis_form().unwrap();
    assert_eq!(form.editing, Some(id));
    assert_eq!(form.kind, OperationKind::Integrate);
    s.state
        .analysis_form_mut()
        .unwrap()
        .setup
        .integrate
        .normalize = true;
    s.state.dispatch(Action::ApplyAnalysis);
    assert_eq!(s.state.tree().calculations().len(), 1);
    assert_eq!(columns(&s), before + 1, "the normalized column is added");
}

#[test]
fn detaching_keeps_the_columns_and_deleting_can_remove_them() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    for _ in 0..2 {
        open(&mut s, Preset::Integrate);
        s.state
            .analysis_form_mut()
            .unwrap()
            .setup
            .integrate
            .cumulative = true;
        s.state.dispatch(Action::ApplyAnalysis);
    }
    let ids: Vec<NodeId> = s.state.tree().calculations().keys().copied().collect();
    assert_eq!(ids.len(), 2);
    let total = columns(&s);

    s.state.dispatch(Action::DetachCalculation(ids[0]));
    assert_eq!(s.state.tree().calculations().len(), 1);
    assert_eq!(columns(&s), total, "detached columns become ordinary");

    s.state.dispatch(Action::RequestDeleteCalculation(ids[1]));
    assert_eq!(s.state.pending_calculation_delete(), Some(ids[1]));
    s.state
        .dispatch(Action::ConfirmDeleteCalculation(CalculationDelete::Cancel));
    assert_eq!(s.state.pending_calculation_delete(), None);
    assert_eq!(s.state.tree().calculations().len(), 1);

    s.state.dispatch(Action::RequestDeleteCalculation(ids[1]));
    s.state.dispatch(Action::ConfirmDeleteCalculation(
        CalculationDelete::WithColumns,
    ));
    assert!(s.state.tree().calculations().is_empty());
    assert_eq!(columns(&s), total - 1);
}

#[test]
fn a_manual_calculation_goes_out_of_date_and_recalculates_on_demand() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    open(&mut s, Preset::Integrate);
    {
        let form = s.state.analysis_form_mut().unwrap();
        form.setup.integrate.cumulative = true;
        form.auto_recalc = false;
        form.auto_recalc_chosen = true;
    }
    s.state.dispatch(Action::ApplyAnalysis);
    assert!(s.state.stale_calculations().is_empty());

    s.state.dispatch(Action::CommitCell {
        column: s.y,
        row: 0,
        text: "100".to_string(),
    });
    assert_eq!(s.state.stale_calculations().len(), 1);

    s.state
        .dispatch(Action::RecalculateCalculations(Vec::new()));
    assert!(s.state.stale_calculations().is_empty());
}

fn graphs_count(s: &Setup) -> usize {
    s.state.graphs().len()
}

fn plotted_columns(s: &Setup) -> Vec<NodeId> {
    let graph = s.state.graphs()[0].0;
    match s.state.tree().node(graph).unwrap().kind() {
        stokesforge_core::NodeKind::Graph { spec, .. } => spec
            .panels
            .iter()
            .flat_map(|p| p.series.iter().map(|series| series.y))
            .collect(),
        _ => unreachable!(),
    }
}

#[test]
fn add_to_graph_plots_the_new_columns_in_the_same_undo_step() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    // Sans graphique qui montre la courbe, aucun candidat.
    assert!(s.state.graphs_plotting(&[s.y]).is_empty());
    s.state.dispatch(Action::PlotColumns {
        sheet: s.sheet,
        columns: vec![s.x, s.y],
        plot_type: stokesforge_plot::PlotType::Line,
    });
    assert_eq!(graphs_count(&s), 1);
    let graph = s.state.graphs()[0].0;
    assert_eq!(s.state.graphs_plotting(&[s.y]).len(), 1);
    assert_eq!(plotted_columns(&s), vec![s.y]);

    open(&mut s, Preset::Fit(Model::Linear));
    {
        let form = s.state.analysis_form_mut().unwrap();
        form.setup.fit.residuals = true;
        form.add_to_graph = true;
        form.graph = Some(graph);
    }
    s.state.dispatch(Action::ApplyAnalysis);
    assert!(
        !s.state.status().unwrap().is_error,
        "{:?}",
        s.state.status()
    );
    let plotted = plotted_columns(&s);
    // La source et l'ajustement ; les résidus ne sont pas tracés.
    assert_eq!(plotted.len(), 2, "{plotted:?}");
    assert_eq!(plotted[0], s.y);
    assert_eq!(columns(&s), 4);

    // Un seul Ctrl+Z défait le calcul, ses colonnes et leur tracé.
    s.state.dispatch(Action::Undo);
    assert!(s.state.tree().calculations().is_empty());
    assert_eq!(columns(&s), 2);
    assert_eq!(plotted_columns(&s), vec![s.y]);
    s.state.dispatch(Action::Redo);
    assert_eq!(plotted_columns(&s).len(), 2);
}

#[test]
fn without_the_box_nothing_is_plotted() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    s.state.dispatch(Action::PlotColumns {
        sheet: s.sheet,
        columns: vec![s.x, s.y],
        plot_type: stokesforge_plot::PlotType::Line,
    });
    open(&mut s, Preset::Fit(Model::Linear));
    s.state.dispatch(Action::ApplyAnalysis);
    assert_eq!(plotted_columns(&s), vec![s.y], "témoin : case décochée");
}

#[test]
fn the_output_choice_is_remembered_across_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("preferences.json");
    let make = |keep_expected: bool| {
        let mut s = setup(dir.path());
        let preferences = stokesforge_app::preferences::Preferences::load_from(Some(file.clone()));
        assert_eq!(preferences.analysis_keep_results(), keep_expected);
        s.state = AppState::new_with_preferences(
            s._session.handle(),
            dir.path().join("p.stokesforge").display().to_string(),
            preferences,
        )
        .unwrap();
        s
    };
    let mut s = make(false);
    open(&mut s, Preset::Fit(Model::Linear));
    assert!(
        !s.state.analysis_form().unwrap().keep_results,
        "Show par défaut"
    );
    s.state.analysis_form_mut().unwrap().keep_results = true;
    s.state.dispatch(Action::ApplyAnalysis);
    // Une session neuve retrouve le choix.
    let mut s = make(true);
    open(&mut s, Preset::Fit(Model::Linear));
    assert!(s.state.analysis_form().unwrap().keep_results);
}
