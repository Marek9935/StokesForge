//! Analyser des lignes (#253) : la feuille transposée liée se crée avec le
//! calcul en un seul groupe annulable, Cancel n'en laisse rien, une colonne
//! sans x' est refusée en la nommant, et la remise à jour de la feuille
//! demande confirmation avant d'écraser des valeurs éditées.

use stokesforge_analysis::model::Model;
use stokesforge_app_state::analysis::Preset;
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_core::{Column, ColumnMeta, NodeId, Project, TRANSPOSE_MARK};
use stokesforge_plot::{PanelSpec, PlotSpec, SeriesRef, SeriesSpec};
use stokesforge_session::Session;

struct Setup {
    session: Session,
    state: AppState,
    sheet: NodeId,
    cols: Vec<NodeId>,
}

/// Six colonnes de quatre lignes, x' = 0, 10, 20…, la ligne `r` valant
/// `(r + 1) * (colonne + 1)` : chaque ligne est une droite de x'.
fn setup(dir: &std::path::Path) -> Setup {
    i18n::set_language(Language::English);
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let mut cols = Vec::new();
    for c in 0..6u32 {
        let data: Vec<f64> = (0..4u32).map(|r| f64::from((r + 1) * (c + 1))).collect();
        let mut column = Column::from_f64(&data);
        column.set_meta(ColumnMeta {
            x_prime: Some(f64::from(c) * 10.0),
            ..ColumnMeta::default()
        });
        cols.push(
            project
                .add_column(sheet, None, Some(&format!("S{c}")), column)
                .unwrap(),
        );
    }
    let session = Session::spawn(project, None).unwrap();
    let state = AppState::new_with_preferences(
        session.handle(),
        dir.join("p.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    Setup {
        session,
        state,
        sheet,
        cols,
    }
}

fn open_rows(s: &mut Setup, rows: &[usize]) {
    s.state.dispatch(Action::OpenRowAnalysis {
        preset: Preset::Fit(Model::Linear),
        rows: rows.iter().map(|r| (s.sheet, *r)).collect(),
    });
}

fn transposed(s: &Setup) -> Option<NodeId> {
    s.state
        .tree()
        .transposed_from(s.sheet)
        .map(|(sheet, _)| sheet)
}

fn name(s: &Setup, id: NodeId) -> String {
    s.state.tree().node(id).unwrap().name().to_string()
}

#[test]
fn opening_creates_the_linked_sheet_and_analyses_its_row_column() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    open_rows(&mut s, &[1]);
    let sheet = transposed(&s).expect("la feuille transposée existe");
    assert_eq!(name(&s, sheet), format!("Sheet1{TRANSPOSE_MARK}"));
    let kids = s.state.tree().children(sheet).unwrap().to_vec();
    let form = s.state.analysis_form().expect("la fenêtre est ouverte");
    assert_eq!(form.series.len(), 1);
    assert_eq!(form.series[0].x, Some(kids[0]));
    assert_eq!(form.series[0].y, kids[1]);
    assert_eq!(name(&s, kids[1]), "Row 2");
    assert!(
        form.graph.is_none(),
        "aucun tracé de résultat sur un graphe"
    );
}

#[test]
fn apply_closes_one_group_and_one_undo_removes_sheet_and_calculation() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    open_rows(&mut s, &[1]);
    s.state.dispatch(Action::ApplyAnalysis);
    assert!(s.state.analysis_form().is_none());
    assert!(
        !s.state.status().unwrap().is_error(),
        "{:?}",
        s.state.status()
    );
    let sheet = transposed(&s).unwrap();
    assert_eq!(s.state.tree().calculations().len(), 1);
    let calc = s
        .state
        .tree()
        .calculations()
        .values()
        .next()
        .unwrap()
        .clone();
    assert!(
        calc.series
            .iter()
            .all(|series| s.state.tree().parent(series.y).unwrap() == Some(sheet)),
        "le calcul lit la colonne de la transposée"
    );

    s.state.dispatch(Action::Undo);
    assert!(
        transposed(&s).is_none(),
        "feuille et lien défaits d'un bloc"
    );
    assert!(s.state.tree().calculations().is_empty());
    assert!(!s.state.can_undo(), "un seul groupe");
    s.state.dispatch(Action::Redo);
    assert!(transposed(&s).is_some());
    assert_eq!(s.state.tree().calculations().len(), 1);
}

#[test]
fn cancel_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    open_rows(&mut s, &[0, 2]);
    assert!(transposed(&s).is_some());
    s.state.dispatch(Action::CloseAnalysis);
    assert!(transposed(&s).is_none());
    assert!(s.state.tree().transposes().is_empty());
    assert!(!s.state.can_undo(), "aucune entrée d'historique");
    // Une écriture ordinaire se range ensuite seule, hors de tout groupe resté ouvert.
    s.state.dispatch(Action::AddColumn { sheet: s.sheet });
    assert!(s.state.can_undo());
    s.state.dispatch(Action::Undo);
    assert!(!s.state.can_undo());
}

#[test]
fn a_column_without_x_prime_is_refused_by_its_letter_and_nothing_is_created() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    s.session
        .handle()
        .execute(stokesforge_commands::Command::AddColumn {
            sheet: s.sheet,
            index: None,
            name: Some("Temps".into()),
            column: Column::from_f64(&[1.0]),
        })
        .unwrap();
    s.state.refresh();
    open_rows(&mut s, &[0]);
    assert!(s.state.analysis_form().is_none());
    let status = s.state.status().unwrap();
    assert!(status.is_error());
    assert!(status.text.contains("Column G (Temps)"), "{}", status.text);
    assert!(transposed(&s).is_none());
    s.state.dispatch(Action::Undo);
    assert_eq!(
        s.state.tree().children(s.sheet).unwrap().len(),
        6,
        "le seul pas annulé est l'ajout de la colonne, le groupe refusé n'a rien laissé"
    );
}

#[test]
fn a_second_row_completes_the_same_sheet_and_the_same_row_reuses_its_column() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    open_rows(&mut s, &[0]);
    s.state.dispatch(Action::ApplyAnalysis);
    let sheet = transposed(&s).unwrap();
    let before = s.state.tree().children(sheet).unwrap().len();
    open_rows(&mut s, &[2]);
    assert_eq!(transposed(&s), Some(sheet));
    let after = s.state.tree().children(sheet).unwrap().len();
    assert!(after > before, "une colonne de plus");
    s.state.dispatch(Action::CloseAnalysis);
    assert_eq!(
        s.state.tree().children(sheet).unwrap().len(),
        before,
        "Cancel retire la colonne ajoutée, pas la feuille déjà analysée"
    );
    open_rows(&mut s, &[0]);
    let row_columns = s.state.tree().children(sheet).unwrap().len();
    assert_eq!(row_columns, before, "la ligne 1 réutilise sa colonne");
}

#[test]
fn a_row_curve_of_a_graph_is_analysed_through_the_transposed_sheet() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let mut curve = SeriesSpec::new(s.cols[0]);
    curve.row = Some(1);
    let mut panel = PanelSpec::default();
    panel.series.push(curve);
    let spec = PlotSpec::default().with_panels(vec![panel]);
    s.session
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
        preset: Preset::Fit(Model::Linear),
        graph,
        series: vec![SeriesRef::new(0, 0)],
        spec: Box::new(spec),
    });
    let sheet = transposed(&s).expect("la ligne du graphe passe par la transposée");
    let form = s.state.analysis_form().unwrap();
    assert_eq!(
        s.state.tree().parent(form.series[0].y).unwrap(),
        Some(sheet)
    );
    assert!(form.graph.is_none());
}

#[test]
fn recalculating_asks_before_erasing_edited_values() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    open_rows(&mut s, &[1]);
    s.state.dispatch(Action::ApplyAnalysis);
    let sheet = transposed(&s).unwrap();
    assert!(!s.state.transpose_info(sheet).unwrap().stale);

    // Source modifiée : périmée, remise à jour sans question.
    s.session
        .handle()
        .execute(stokesforge_commands::Command::SetCells {
            column: s.cols[0],
            start: 1,
            values: stokesforge_core::ValueBlock::Float(vec![500.0]),
            expected_version: None,
        })
        .unwrap();
    s.state.refresh();
    let info = s.state.transpose_info(sheet).unwrap();
    assert!(info.stale && !info.modified);
    s.state.dispatch(Action::RecalculateTranspose { sheet });
    assert!(s.state.pending_transpose_relink().is_none());
    let info = s.state.transpose_info(sheet).unwrap();
    assert!(!info.stale);

    // Valeur éditée dans la colonne liée : confirmation d'abord.
    let row = s.state.tree().children(sheet).unwrap()[1];
    s.session
        .handle()
        .execute(stokesforge_commands::Command::SetCells {
            column: row,
            start: 0,
            values: stokesforge_core::ValueBlock::Float(vec![-7.0]),
            expected_version: None,
        })
        .unwrap();
    s.state.refresh();
    assert!(s.state.transpose_info(sheet).unwrap().modified);
    s.state.dispatch(Action::RecalculateTranspose { sheet });
    assert_eq!(s.state.pending_transpose_relink(), Some(sheet));
    s.state.dispatch(Action::ConfirmTransposeRelink(false));
    assert!(
        s.state.transpose_info(sheet).unwrap().modified,
        "Cancel ne fait rien"
    );
    s.state.dispatch(Action::RecalculateTranspose { sheet });
    s.state.dispatch(Action::ConfirmTransposeRelink(true));
    let info = s.state.transpose_info(sheet).unwrap();
    assert!(!info.modified && !info.stale);
    s.state.dispatch(Action::Undo);
    assert!(s.state.transpose_info(sheet).unwrap().modified, "annulable");
}
