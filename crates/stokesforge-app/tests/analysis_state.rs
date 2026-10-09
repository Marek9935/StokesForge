//! Les gestes d'analyse de `AppState` (#181), pilotés sans fenêtre : ouvrir la
//! fenêtre sur la sélection, valider (une entrée d'historique), modifier,
//! recalculer, détacher, supprimer avec confirmation.

use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::OperationKind;
use stokesforge_app::analysis::{Edge, HandleSlot, Preset, RangeSet};
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

#[test]
fn dragging_a_handle_is_one_undoable_step_that_changes_the_data() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    open(&mut s, Preset::Integrate);
    {
        let form = s.state.analysis_form_mut().unwrap();
        form.add_zone(2.0, 4.0);
        form.setup.integrate.cumulative = true;
    }
    s.state.dispatch(Action::ApplyAnalysis);
    let id = only_calculation(&s);
    let range = |s: &Setup| match &s.state.tree().calculation(id).unwrap().operation {
        stokesforge_analysis::ops::Operation::Integrate { spec } => spec.ranges.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(range(&s), vec![(2.0, 4.0)]);

    s.state.dispatch(Action::MoveCalculationHandle {
        calculation: id,
        slot: HandleSlot::Edge(RangeSet::Integrate, 0, Edge::Hi),
        x: 6.0,
        y: 0.0,
    });
    assert!(!s.state.status().unwrap().is_error);
    assert_eq!(range(&s), vec![(2.0, 6.0)]);

    // Un seul pas : annuler ramène l'ancienne plage.
    s.state.dispatch(Action::Undo);
    assert_eq!(range(&s), vec![(2.0, 4.0)]);

    // Une poignée qui n'a pas bougé n'écrit rien dans l'historique.
    s.state.dispatch(Action::MoveCalculationHandle {
        calculation: id,
        slot: HandleSlot::Edge(RangeSet::Integrate, 0, Edge::Hi),
        x: 4.0,
        y: 0.0,
    });
    s.state.dispatch(Action::Redo);
    assert_eq!(range(&s), vec![(2.0, 6.0)], "le Redo est resté disponible");
}
