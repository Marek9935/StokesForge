//! Les gestes d'analyse de `AppState` (#181), pilotés sans fenêtre : ouvrir la
//! fenêtre sur la sélection, valider (une entrée d'historique), modifier,
//! recalculer, détacher, supprimer avec confirmation.

use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::OperationKind;
use stokesforge_app_state::analysis::{Edge, HandleSlot, Preset, RangeSet};
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::state::{Action, AppState, CalculationDelete};
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
    let state = AppState::new_with_preferences(
        session.handle(),
        dir.join("p.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
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
    assert!(s.state.status().unwrap().is_error());
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
    assert!(!s.state.status().unwrap().is_error());
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
    // M7 (#228) : l'erreur va dans le pied de la fenêtre, jamais dans la barre
    // d'état.
    assert!(s.state.dialog_error().is_some());
    assert!(!s.state.status().is_some_and(|status| status.is_error()));
    assert!(s.state.tree().calculations().is_empty());
    s.state.dispatch(Action::CloseAnalysis);
    assert!(s.state.analysis_form().is_none());
    assert!(
        s.state.dialog_error().is_none(),
        "l'erreur ne survit pas à sa fenêtre"
    );
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
fn the_low_r2_dialog_setting_goes_from_the_form_to_the_calculation_and_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    open(&mut s, Preset::Integrate);
    assert!(
        s.state.analysis_form().unwrap().low_r2_dialog,
        "on by default"
    );
    s.state.dispatch(Action::ApplyAnalysis);
    let id = only_calculation(&s);
    assert!(s.state.tree().calculation(id).unwrap().low_r2_dialog);

    s.state.dispatch(Action::EditCalculation(id));
    assert!(s.state.analysis_form().unwrap().low_r2_dialog);
    s.state.analysis_form_mut().unwrap().low_r2_dialog = false;
    s.state.dispatch(Action::ApplyAnalysis);
    assert!(!s.state.tree().calculation(id).unwrap().low_r2_dialog);

    s.state.dispatch(Action::EditCalculation(id));
    assert!(
        !s.state.analysis_form().unwrap().low_r2_dialog,
        "the form reads the setting back from the calculation"
    );
    s.state.dispatch(Action::Undo);
    assert!(s.state.tree().calculation(id).unwrap().low_r2_dialog);
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
    assert!(!s.state.status().unwrap().is_error());
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
        !s.state.status().unwrap().is_error(),
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
        let preferences =
            stokesforge_app_state::preferences::Preferences::load_from(Some(file.clone()));
        assert_eq!(preferences.analysis_keep_results(), keep_expected);
        s.state = AppState::new_with_preferences(
            s._session.handle(),
            dir.path().join("p.stf").display().to_string(),
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

#[test]
fn analyse_depuis_graphe_garde_x_sigma_styles_calque_groupe_et_annulation() {
    use stokesforge_plot::{LineStyle, PanelSpec, PlotSpec, SeriesSpec};
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    // Le X de la série est volontairement différent du X apparié de B.
    s.state.dispatch(Action::AddColumn { sheet: s.sheet });
    let explicit_x = *s.state.tree().children(s.sheet).unwrap().last().unwrap();
    // Une abscisse implicite aurait aussi le mauvais résultat : les valeurs
    // choisies exercent réellement la lecture de cette autre colonne.
    s._session
        .handle()
        .execute(stokesforge_commands::Command::SetCells {
            column: explicit_x,
            start: 0,
            expected_version: None,
            values: stokesforge_core::ValueBlock::Float(
                (0..11).map(|v| 10.0 + f64::from(v)).collect(),
            ),
        })
        .unwrap();
    s.state.refresh();
    let mut curve = SeriesSpec::new_xy(explicit_x, s.y);
    curve.y_error = Some(s.x);
    curve.group = Some(42);
    curve.line.style = LineStyle::Dash;
    curve.line.width = 3.0;
    let mut panel = PanelSpec::default();
    panel.series.push(curve.clone());
    panel.bottom.auto_range = false;
    panel.bottom.range_min = Some(12.0);
    panel.bottom.range_max = Some(18.0);
    let mut spec = PlotSpec::default().with_panels(vec![PanelSpec::default(), panel]);
    // Le graphe d'essai se crée par la commande habituelle.
    s._session
        .handle()
        .execute(stokesforge_commands::Command::AddGraph {
            parent: s.state.tree().root(),
            name: None,
            spec: spec.clone(),
        })
        .unwrap();
    s.state.refresh();
    let graph = s.state.graphs()[0].0;
    s.state.dispatch(Action::OpenGraphAnalysis {
        preset: Preset::BaselineLine,
        graph,
        series: vec![stokesforge_plot::SeriesRef::new(1, 0)],
        spec: Box::new(spec.clone()),
    });
    let form = s.state.analysis_form().unwrap();
    assert_eq!(
        form.series[0],
        stokesforge_core::SeriesRef {
            x: Some(explicit_x),
            y: s.y,
            sigma: Some(s.x)
        }
    );
    assert!(form.add_to_graph);
    assert_eq!(form.graph, Some(graph));
    assert_eq!(form.graph_source.as_ref().unwrap().0, spec);
    assert_eq!(
        stokesforge_app_state::analysis::series_points(s.state.tree(), &form.series[0]).0[0],
        10.0
    );
    let form = s.state.analysis_form_mut().unwrap();
    form.setup.baseline.points = vec![(10.0, 1.0), (20.0, 21.0)];
    s.state.dispatch(Action::ApplyAnalysis);
    assert!(
        !s.state.status().unwrap().is_error(),
        "{:?}",
        s.state.status()
    );
    spec = match s.state.tree().node(graph).unwrap().kind() {
        stokesforge_core::NodeKind::Graph { spec, .. } => spec.clone(),
        _ => unreachable!(),
    };
    assert!(spec.panels[0].series.is_empty());
    assert!(spec.panels[1].series.len() > 1);
    for result in &spec.panels[1].series[1..] {
        assert_eq!(result.x, Some(explicit_x));
        assert_eq!(result.group, Some(42));
        assert_eq!(result.line, curve.line);
    }
    s.state.dispatch(Action::Undo);
    assert!(s.state.tree().calculations().is_empty());
    assert_eq!(plotted_columns(&s), vec![s.y]);
}

#[test]
fn remplacement_conserve_la_colonne_et_les_traces_successives() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let before = s.state.tree().clone();
    for constant in [1., 2.] {
        open(&mut s, Preset::BaselineLine);
        let form = s.state.analysis_form_mut().unwrap();
        form.setup.baseline.points = vec![(0., constant), (10., constant)];
        form.replace_original = true;
        s.state.dispatch(Action::ApplyAnalysis);
        assert!(s.state.analysis_form().is_none());
        assert_eq!(columns(&s), 2);
    }
    assert!(s.state.tree().calculations().is_empty());
    assert_eq!(s.state.tree().correction_traces().len(), 2);
    let values = s
        .state
        .tree()
        .get(s.y)
        .unwrap()
        .as_column()
        .unwrap()
        .numeric_view()
        .unwrap();
    assert_eq!(values[0], -2.);
    s.state.dispatch(Action::Undo);
    assert_eq!(s.state.tree().correction_traces().len(), 1);
    s.state.dispatch(Action::Undo);
    assert_eq!(s.state.tree(), &before);
    s.state.dispatch(Action::Redo);
    assert_eq!(s.state.tree().correction_traces().len(), 1);
}

#[test]
fn remplacement_partage_attend_confirmation_et_annuler_garde_le_formulaire() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    for _ in 0..2 {
        s.state.dispatch(Action::PlotColumns {
            sheet: s.sheet,
            columns: vec![s.x, s.y],
            plot_type: stokesforge_plot::PlotType::Line,
        });
    }
    open(&mut s, Preset::BaselineLine);
    let form = s.state.analysis_form_mut().unwrap();
    form.setup.baseline.points = vec![(0., 1.), (10., 1.)];
    form.replace_original = true;
    let before = s.state.tree().clone();
    s.state.dispatch(Action::ApplyAnalysis);
    assert!(s.state.pending_analysis_replacement());
    assert_eq!(s.state.tree(), &before);
    s.state.dispatch(Action::ConfirmAnalysisReplacement(false));
    assert!(!s.state.pending_analysis_replacement());
    assert!(s.state.analysis_form().is_some());
    assert_eq!(s.state.tree(), &before);
    s.state.dispatch(Action::ApplyAnalysis);
    s.state.dispatch(Action::ConfirmAnalysisReplacement(true));
    assert!(s.state.analysis_form().is_none());
    assert_eq!(s.state.tree().correction_traces().len(), 1);
    assert_eq!(columns(&s), 2);
}

// ── Analyses récentes (#249) ────────────────────────────────────────────

/// Un état dont les préférences vivent dans un fichier temporaire : jamais la
/// configuration réelle de la machine.
fn with_preferences(s: &mut Setup, dir: &std::path::Path) -> std::path::PathBuf {
    let file = dir.join("preferences.json");
    let preferences =
        stokesforge_app_state::preferences::Preferences::load_from(Some(file.clone()));
    s.state = AppState::new_with_preferences(
        s._session.handle(),
        dir.join("p.stf").display().to_string(),
        preferences,
    )
    .unwrap();
    file
}

fn target(s: &Setup) -> stokesforge_app_state::analysis::RecentTarget {
    stokesforge_app_state::analysis::RecentTarget::Columns {
        sheet: s.sheet,
        columns: vec![s.x, s.y],
    }
}

fn launch_integrate(s: &mut Setup, cumulative: bool) {
    open(s, Preset::Integrate);
    s.state
        .analysis_form_mut()
        .unwrap()
        .setup
        .integrate
        .cumulative = cumulative;
    s.state.dispatch(Action::ApplyAnalysis);
}

#[test]
fn les_analyses_lancees_sont_retenues_sans_doublon_et_survivent_a_la_session() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let file = with_preferences(&mut s, dir.path());
    assert!(s.state.analysis_recent().is_empty());

    launch_integrate(&mut s, false);
    launch_integrate(&mut s, true);
    open(&mut s, Preset::Fit(Model::Polynomial { degree: 3 }));
    s.state.dispatch(Action::ApplyAnalysis);
    assert_eq!(s.state.analysis_recent().len(), 3);
    assert_eq!(
        s.state.analysis_recent()[0].label(),
        "Polynomial Fit (degree 3)"
    );

    // La même analyse n'occupe qu'une place, remontée en tête.
    launch_integrate(&mut s, false);
    assert_eq!(s.state.analysis_recent().len(), 3);
    assert_eq!(s.state.analysis_recent()[0].label(), "Integrate");

    // Une quatrième fait partir la plus ancienne.
    open(&mut s, Preset::Fit(Model::Gaussian));
    s.state.dispatch(Action::ApplyAnalysis);
    assert_eq!(s.state.analysis_recent().len(), 3);

    // Un calcul qui échoue ou qu'on annule ne compte pas.
    let before = s.state.analysis_recent().to_vec();
    open(&mut s, Preset::Correct);
    s.state.dispatch(Action::ApplyAnalysis);
    s.state.dispatch(Action::CloseAnalysis);
    assert_eq!(s.state.analysis_recent(), before.as_slice());

    // Les entrées vivent dans les préférences, pas dans le projet.
    let preferences = stokesforge_app_state::preferences::Preferences::load_from(Some(file));
    assert_eq!(preferences.analysis_recent(), before.as_slice());
}

#[test]
fn modifier_un_calcul_ne_change_pas_les_analyses_recentes() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    with_preferences(&mut s, dir.path());
    launch_integrate(&mut s, true);
    let before = s.state.analysis_recent().to_vec();
    let id = only_calculation(&s);
    s.state.dispatch(Action::EditCalculation(id));
    s.state
        .analysis_form_mut()
        .unwrap()
        .setup
        .integrate
        .normalize = true;
    s.state.dispatch(Action::ApplyAnalysis);
    assert_eq!(s.state.analysis_recent(), before.as_slice());
}

#[test]
fn un_clic_rouvre_la_fenetre_avec_les_reglages_sans_les_plages() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    with_preferences(&mut s, dir.path());
    open(&mut s, Preset::Integrate);
    let form = s.state.analysis_form_mut().unwrap();
    form.setup.integrate.cumulative = true;
    form.setup.integrate.ranges = vec![(2.0, 4.0)];
    s.state.dispatch(Action::ApplyAnalysis);

    let calculations = s.state.tree().calculations().len();
    let target = target(&s);
    s.state.dispatch(Action::OpenRecentAnalysis {
        index: 0,
        run: false,
        target,
    });
    let form = s.state.analysis_form().expect("la fenêtre s'ouvre");
    assert_eq!(form.kind, OperationKind::Integrate);
    assert!(form.setup.integrate.cumulative, "le réglage est repris");
    assert!(form.setup.integrate.ranges.is_empty(), "pas la plage");
    assert_eq!(form.series.len(), 1, "la sélection courante");
    assert_eq!(
        s.state.tree().calculations().len(),
        calculations,
        "rien n'est lancé"
    );
}

#[test]
fn ctrl_clic_lance_sur_la_selection_courante_et_se_defait_en_un_pas() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    with_preferences(&mut s, dir.path());
    launch_integrate(&mut s, true);
    assert_eq!(s.state.tree().calculations().len(), 1);
    let target = target(&s);
    s.state.dispatch(Action::OpenRecentAnalysis {
        index: 0,
        run: true,
        target,
    });
    assert!(s.state.analysis_form().is_none(), "lancée tout de suite");
    assert_eq!(s.state.tree().calculations().len(), 2);
    s.state.dispatch(Action::Undo);
    assert_eq!(s.state.tree().calculations().len(), 1);
}

#[test]
fn ctrl_clic_ouvre_la_fenetre_quand_il_manque_une_colonne_de_reference() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    with_preferences(&mut s, dir.path());
    open(&mut s, Preset::Correct);
    // La référence n'est pas un réglage retenu : on la pose pour lancer.
    s.state.analysis_form_mut().unwrap().aux = Some(stokesforge_core::AuxRef { x: None, y: s.x });
    s.state.dispatch(Action::ApplyAnalysis);
    assert!(s.state.analysis_form().is_none());
    let target = target(&s);
    let calculations = s.state.tree().calculations().len();
    s.state.dispatch(Action::OpenRecentAnalysis {
        index: 0,
        run: true,
        target,
    });
    let form = s
        .state
        .analysis_form()
        .expect("la fenêtre attend sa colonne");
    assert!(form.aux.is_none());
    assert_eq!(s.state.tree().calculations().len(), calculations);
}

#[test]
fn ctrl_clic_remplace_directement_les_donnees_et_le_dit() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    with_preferences(&mut s, dir.path());
    open(&mut s, Preset::BaselineLine);
    let form = s.state.analysis_form_mut().unwrap();
    form.setup.baseline.points = vec![(0., 1.), (10., 1.)];
    form.replace_original = true;
    s.state.dispatch(Action::ApplyAnalysis);
    assert!(
        s.state.analysis_recent()[0].replace_original,
        "mémorisé avec les réglages"
    );
    assert_eq!(s.state.tree().correction_traces().len(), 1);

    let target = target(&s);
    s.state.dispatch(Action::OpenRecentAnalysis {
        index: 0,
        run: true,
        target,
    });
    assert!(s.state.analysis_form().is_none());
    assert_eq!(
        s.state.tree().correction_traces().len(),
        2,
        "remplacé une seconde fois"
    );
    let status = s.state.status().unwrap();
    assert!(!status.is_error());
    assert!(status.text.contains("Ctrl+Z"), "{}", status.text);
    s.state.dispatch(Action::Undo);
    assert_eq!(
        s.state.tree().correction_traces().len(),
        1,
        "Ctrl+Z le rattrape"
    );

    // Témoin : un clic simple n'applique rien.
    let target = self::target(&s);
    s.state.dispatch(Action::OpenRecentAnalysis {
        index: 0,
        run: false,
        target,
    });
    assert!(s.state.analysis_form().unwrap().replace_original);
    assert_eq!(s.state.tree().correction_traces().len(), 1);
}

// ── Catégories, référence figée, restauration (#248) ────────────────────

#[test]
fn restaurer_les_donnees_originales_ecrase_les_saisies_et_s_annule() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let original: Vec<f64> = s
        .state
        .tree()
        .get(s.y)
        .unwrap()
        .as_column()
        .unwrap()
        .numeric_view()
        .unwrap()
        .to_vec();
    open(&mut s, Preset::BaselineLine);
    let form = s.state.analysis_form_mut().unwrap();
    form.setup.baseline.points = vec![(0., 1.), (10., 1.)];
    form.replace_original = true;
    s.state.dispatch(Action::ApplyAnalysis);
    let trace = *s.state.tree().correction_traces().keys().next().unwrap();
    let values = |s: &Setup| {
        s.state
            .tree()
            .get(s.y)
            .unwrap()
            .as_column()
            .unwrap()
            .numeric_view()
            .unwrap()
            .to_vec()
    };
    assert_ne!(values(&s), original, "témoin : remplacé");
    s.state.dispatch(Action::CommitCell {
        column: s.y,
        row: 0,
        text: "999".to_string(),
    });

    s.state.dispatch(Action::RestoreOriginalData(trace));
    assert_eq!(values(&s), original, "la saisie est écrasée");
    assert!(s.state.tree().correction_traces().is_empty());
    assert!(!s.state.status().is_some_and(|m| m.is_error()));

    s.state.dispatch(Action::Undo);
    assert_eq!(
        values(&s)[0],
        999.0,
        "Ctrl+Z rend l'état d'avant la restauration"
    );
    assert_eq!(s.state.tree().correction_traces().len(), 1);
}

#[test]
fn une_correction_garde_sa_reference_meme_si_la_colonne_d_origine_disparait() {
    i18n::set_language(Language::English);
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let xs: Vec<f64> = (0..11).map(f64::from).collect();
    let ys: Vec<f64> = xs.iter().map(|x| 2.0 * x + 1.0).collect();
    let mut add = |name: &str, values: &[f64]| {
        project
            .add_column(sheet, None, Some(name), Column::from_f64(values))
            .unwrap()
    };
    let x = add("A", &xs);
    let y = add("B", &ys);
    let reference = add("C", &[2.0; 11]);
    let session = Session::spawn(project, None).unwrap();
    let mut state = AppState::new_with_preferences(
        session.handle(),
        dir.path().join("p.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(Some(
            dir.path().join("prefs.json"),
        )),
    )
    .unwrap();
    state.dispatch(Action::OpenAnalysis {
        preset: Preset::Correct,
        sheet,
        columns: vec![x, y],
    });
    state.analysis_form_mut().unwrap().aux = Some(stokesforge_core::AuxRef {
        x: Some(x),
        y: reference,
    });
    state.dispatch(Action::ApplyAnalysis);
    let id = *state.tree().calculations().keys().next().unwrap();
    let frozen = state.tree().calculation(id).unwrap().aux.clone().unwrap();
    assert_ne!(frozen.y, reference, "le calcul lit une copie figée");
    assert!(state.tree().is_analysis_copy(frozen.y));
    let corrected = |state: &AppState| {
        let out = state.tree().calculation(id).unwrap().outputs[0]
            .column
            .unwrap();
        state
            .tree()
            .get(out)
            .unwrap()
            .as_column()
            .unwrap()
            .numeric_view()
            .unwrap()
            .to_vec()
    };
    let expected = corrected(&state);

    // La colonne d'origine est supprimée, puis le calcul recalculé de force.
    session
        .handle()
        .execute(stokesforge_commands::Command::RemoveNode { id: reference })
        .unwrap();
    state.dispatch(Action::RecalculateCalculations(vec![id]));
    assert!(state.tree().node(reference).is_none());
    let calc = state.tree().calculation(id).unwrap();
    assert!(calc.stale.is_none(), "rien n'est cassé : {:?}", calc.stale);
    let bits = |values: Vec<f64>| values.into_iter().map(f64::to_bits).collect::<Vec<_>>();
    assert_eq!(bits(corrected(&state)), bits(expected));

    // Le classeur caché n'est ni un classeur de l'application ni une ligne du
    // Project Manager.
    assert_eq!(state.books().len(), 1);
    assert_eq!(state.project_tree().len(), 1);
    session.shutdown().unwrap();
}
