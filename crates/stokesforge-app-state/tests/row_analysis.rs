//! Analyzing rows (#253): the linked transposed sheet is created with the
//! calculation in a single undoable group, Cancel leaves nothing of it, a column
//! without x' is refused by naming it, and updating the sheet asks for
//! confirmation before overwriting edited values.

use stokesforge_analysis::model::Model;
use stokesforge_app_state::analysis::Preset;
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::state::{Action, AlertKind, AppState};
use stokesforge_core::{Column, ColumnMeta, NodeId, Project, TRANSPOSE_MARK};
use stokesforge_plot::{PanelSpec, PlotSpec, SeriesRef, SeriesSpec};
use stokesforge_session::Session;

struct Setup {
    _language: i18n::LanguageGuard,
    session: Session,
    state: AppState,
    sheet: NodeId,
    cols: Vec<NodeId>,
}

/// Six columns of four rows, x' = 0, 10, 20..., row `r` being worth
/// `(r + 1) * (column + 1)`: each row is a line of x'.
fn setup(dir: &std::path::Path) -> Setup {
    setup_with(dir, true)
}

/// [`setup`] with no x' at all (#493).
fn setup_bare(dir: &std::path::Path) -> Setup {
    setup_with(dir, false)
}

fn setup_with(dir: &std::path::Path, with_x_prime: bool) -> Setup {
    let language = i18n::test_language(Language::English);
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let mut cols = Vec::new();
    for c in 0..6u32 {
        let data: Vec<f64> = (0..4u32).map(|r| f64::from((r + 1) * (c + 1))).collect();
        let mut column = Column::from_f64(&data);
        column.set_meta(ColumnMeta {
            x_prime: with_x_prime.then(|| f64::from(c) * 10.0),
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
        _language: language,
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
    let sheet = transposed(&s).expect("the transposed sheet exists");
    assert_eq!(name(&s, sheet), format!("Sheet1{TRANSPOSE_MARK}"));
    let kids = s.state.tree().children(sheet).unwrap().to_vec();
    let form = s.state.analysis_form().expect("the window is open");
    assert_eq!(form.series.len(), 1);
    assert_eq!(form.series[0].x, Some(kids[0]));
    assert_eq!(form.series[0].y, kids[1]);
    assert_eq!(name(&s, kids[1]), "Row 2");
    assert!(form.graph.is_none(), "no result plot on a graph");
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
        "the calculation reads the column of the transposed sheet"
    );

    s.state.dispatch(Action::Undo);
    assert!(
        transposed(&s).is_none(),
        "sheet and link undone as one block"
    );
    assert!(s.state.tree().calculations().is_empty());
    assert!(!s.state.can_undo(), "a single group");
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
    assert!(!s.state.can_undo(), "no history entry");
    // An ordinary write then files itself alone, outside any group left open.
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
            name: Some("Time".into()),
            column: Column::from_f64(&[1.0]),
        })
        .unwrap();
    s.state.refresh();
    open_rows(&mut s, &[0]);
    assert!(s.state.analysis_form().is_none());
    let status = s.state.status().unwrap();
    assert!(status.is_error());
    assert!(status.text.contains("Column G (Time)"), "{}", status.text);
    assert!(transposed(&s).is_none());
    s.state.dispatch(Action::Undo);
    assert_eq!(
        s.state.tree().children(s.sheet).unwrap().len(),
        6,
        "the only undone step is the column addition, the refused group left nothing behind"
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
    assert!(after > before, "one more column");
    s.state.dispatch(Action::CloseAnalysis);
    assert_eq!(
        s.state.tree().children(sheet).unwrap().len(),
        before,
        "Cancel removes the added column, not the sheet already analyzed"
    );
    open_rows(&mut s, &[0]);
    let row_columns = s.state.tree().children(sheet).unwrap().len();
    assert_eq!(row_columns, before, "row 1 reuses its column");
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
    let sheet = transposed(&s).expect("the graph row goes through the transposed sheet");
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
    // These cases pin the snapshot banner: the box is unchecked (#463).
    s.state
        .dispatch(Action::SetTransposeAuto { sheet, auto: false });
    assert!(!s.state.transpose_info(sheet).unwrap().stale);

    // Modified source: stale, updated without a question.
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

    // Edited value in the linked column: confirmation first.
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
        "Cancel does nothing"
    );
    s.state.dispatch(Action::RecalculateTranspose { sheet });
    s.state.dispatch(Action::ConfirmTransposeRelink(true));
    let info = s.state.transpose_info(sheet).unwrap();
    assert!(!info.modified && !info.stale);
    s.state.dispatch(Action::Undo);
    assert!(s.state.transpose_info(sheet).unwrap().modified, "undoable");
}

/// #439: a user's write draws a fresh column version even when it changes
/// nothing, so a command writing the value a source cell already holds (a paste,
/// a script; typing it in the cell sends none) makes the link
/// out of date, exactly like another number (what the banner reads).
#[test]
fn writing_the_same_value_in_the_source_marks_the_sheet_out_of_date() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    open_rows(&mut s, &[1]);
    s.state.dispatch(Action::ApplyAnalysis);
    let sheet = transposed(&s).unwrap();
    // These cases pin the snapshot banner: the box is unchecked (#463).
    s.state
        .dispatch(Action::SetTransposeAuto { sheet, auto: false });
    assert!(!s.state.transpose_info(sheet).unwrap().stale);

    // Row 2 of the first column already holds 2.0.
    s.session
        .handle()
        .execute(stokesforge_commands::Command::SetCells {
            column: s.cols[0],
            start: 1,
            values: stokesforge_core::ValueBlock::Float(vec![2.0]),
            expected_version: None,
        })
        .unwrap();
    s.state.refresh();
    let info = s.state.transpose_info(sheet).unwrap();
    assert!(info.stale && !info.modified, "wrote 2 on 2: out of date");
}

/// #463, T7: a followed sheet whose source has a refused formula reading it
/// cannot catch up, and says which column is in cause (it used to read a plain
/// "Out of date" with the box checked). Witness: the reason left out of
/// `transpose_info`.
#[test]
fn a_followed_sheet_blocked_by_a_source_formula_names_the_column() {
    use stokesforge_app_state::state::HeaderField;
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    open_rows(&mut s, &[1]);
    s.state.dispatch(Action::ApplyAnalysis);
    let sheet = transposed(&s).unwrap();
    assert!(s.state.transpose_info(sheet).unwrap().auto, "box checked");
    let name = s.state.tree().node(sheet).unwrap().name().to_string();
    s.state.begin_header_edit(s.cols[2], HeaderField::Formula);
    s.state.header_editing_mut().unwrap().buffer = format!("max('{name}'!B)");
    s.state.commit_header_edit();
    s.state.refresh();
    let info = s.state.transpose_info(sheet).unwrap();
    assert!(info.auto && info.stale && !info.modified, "{info:?}");
    assert_eq!(
        info.stuck.as_deref(),
        Some(
            format!("Column C of Sheet1 calculates from {name}. Remove that formula to follow")
                .as_str()
        )
    );
}

// -- #484: a checked sheet with a reader refuses Recalculate, Relink, Analyze Rows

/// A followed sheet of row 2 whose source column C keeps a refused formula
/// reading it: the sheet is out of date and stuck. Returns the sheet.
fn followed_with_a_reader(s: &mut Setup) -> NodeId {
    use stokesforge_app_state::state::HeaderField;
    open_rows(s, &[1]);
    s.state.dispatch(Action::ApplyAnalysis);
    let sheet = transposed(s).unwrap();
    let sheet_name = name(s, sheet);
    s.state.begin_header_edit(s.cols[2], HeaderField::Formula);
    s.state.header_editing_mut().unwrap().buffer = format!("max('{sheet_name}'!B)");
    s.state.commit_header_edit();
    s.state.refresh();
    sheet
}

fn type_in_the_sheet(s: &mut Setup, sheet: NodeId) {
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
}

/// Witness: the old catalog sentence (it said "to check Recalculate
/// automatically"), or the refusal removed from `recalculate_transpose_inner`.
#[test]
fn recalculate_of_a_followed_sheet_with_a_reader_says_why_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let sheet = followed_with_a_reader(&mut s);
    let info = s.state.transpose_info(sheet).unwrap();
    assert!(info.stale && !info.modified);
    let stuck = info.stuck.clone().expect("a reason");
    let undo_before = s.state.can_undo();
    let tree_before = s.state.tree().clone();
    s.state.dispatch(Action::RecalculateTranspose { sheet });
    let status = s.state.status().expect("a message");
    assert_eq!(
        status.level,
        stokesforge_app_state::state::StatusLevel::Error
    );
    assert_eq!(status.text, stuck);
    assert!(status.text.ends_with("Remove that formula to follow"));
    let after = s.state.transpose_info(sheet).unwrap();
    assert!(after.stale);
    assert_eq!(after.stuck, Some(stuck));
    assert_eq!(s.state.can_undo(), undo_before);
    assert_eq!(s.state.tree().as_ref(), tree_before.as_ref());
}

/// Witness: the pre-check removed from `recalculate_transpose` (the question
/// would be asked, then refused).
#[test]
fn relink_with_a_reader_is_refused_before_the_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let sheet = followed_with_a_reader(&mut s);
    type_in_the_sheet(&mut s, sheet);
    assert!(s.state.transpose_info(sheet).unwrap().modified);
    s.state.dispatch(Action::RecalculateTranspose { sheet });
    assert_eq!(s.state.pending_transpose_relink(), None);
    let status = s.state.status().expect("a message");
    assert_eq!(
        status.level,
        stokesforge_app_state::state::StatusLevel::Error
    );
    assert!(status.text.ends_with("Remove that formula to follow"));
    assert!(s.state.transpose_info(sheet).unwrap().modified, "kept");
}

/// Witness: the call removed from `transpose_rows_inner` (a form would open).
#[test]
fn analyzing_a_row_of_a_stale_modified_sheet_with_a_reader_opens_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let sheet = followed_with_a_reader(&mut s);
    type_in_the_sheet(&mut s, sheet);
    let undo_before = s.state.can_undo();
    open_rows(&mut s, &[2]);
    assert!(s.state.analysis_form().is_none(), "no form");
    let status = s.state.status().expect("the reason");
    assert_eq!(
        status.level,
        stokesforge_app_state::state::StatusLevel::Error
    );
    assert!(status.text.ends_with("Remove that formula to follow"));
    assert_eq!(s.state.can_undo(), undo_before);
    // No group left open: the next write files itself alone and Undo takes it.
    s.state.dispatch(Action::AddColumn { sheet: s.sheet });
    s.state.dispatch(Action::Undo);
    assert_eq!(s.state.can_undo(), undo_before);
}

#[test]
fn analyzing_a_row_of_an_unchecked_sheet_with_a_reader_opens_the_form() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let sheet = followed_with_a_reader(&mut s);
    s.state
        .dispatch(Action::SetTransposeAuto { sheet, auto: false });
    open_rows(&mut s, &[2]);
    assert!(s.state.analysis_form().is_some(), "form opens");
    s.state.dispatch(Action::CloseAnalysis);
}

// No x' on any Y column (#493): the rows are analyzed with the column numbers.

/// Removes the x' of every column of the sheet.
fn strip_x_prime(s: &mut Setup) {
    for column in s.cols.clone() {
        s.session
            .handle()
            .execute(stokesforge_commands::Command::SetColumnMeta {
                column,
                meta: ColumnMeta::default(),
            })
            .unwrap();
    }
    s.state.refresh();
}

fn x_values(s: &Setup, x: NodeId) -> Vec<f64> {
    s.state
        .tree()
        .node(x)
        .unwrap()
        .as_column()
        .unwrap()
        .numeric_view()
        .unwrap()
        .to_vec()
}

#[test]
fn whole_rows_without_x_prime_are_analyzed_with_the_column_numbers_and_warn() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup_bare(dir.path());
    s.state.select_row(s.sheet, 1);
    let target = s.state.analysis_rows_target(s.sheet).expect("rows target");
    assert_eq!(target.rows, vec![1]);
    assert!(
        s.state.row_drag_payload(s.sheet).is_none(),
        "plot unchanged"
    );
    open_rows(&mut s, &[1]);
    let form = s.state.analysis_form().expect("the window is open");
    let x = form.series[0].x.expect("x'");
    assert_eq!(x_values(&s, x), vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    let alert = s.state.alert().expect("the warning");
    assert_eq!(alert.kind, AlertKind::NoXPrime);
    assert_eq!(
        alert.message,
        "Sheet1 has no x' values. The analysis uses the column numbers 1, 2, 3… as x'."
    );
    assert!(s.state.row_analysis_cancellable());
    assert!(
        !s.state.status().is_some_and(|status| status.is_error()),
        "a warning is not a failure"
    );
    s.state.dispatch(Action::DismissAlert);
    s.state.dispatch(Action::ApplyAnalysis);
    assert_eq!(s.state.tree().calculations().len(), 1);
    s.state.dispatch(Action::Undo);
    assert!(transposed(&s).is_none());
    assert!(
        !s.state.can_undo(),
        "one 'Analyze Rows' step besides the setup"
    );
}

#[test]
fn rows_with_real_x_prime_raise_no_warning() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    open_rows(&mut s, &[1]);
    assert!(s.state.analysis_form().is_some());
    assert!(s.state.alert().is_none());
}

#[test]
fn cancel_analysis_from_the_warning_leaves_nothing_and_close_keeps_the_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup_bare(dir.path());
    open_rows(&mut s, &[0]);
    // Close: the warning goes, the analysis stays.
    s.state.dispatch(Action::DismissAlert);
    assert!(s.state.alert().is_none());
    assert!(s.state.analysis_form().is_some());
    assert!(transposed(&s).is_some());
    s.state.dispatch(Action::CloseAnalysis);
    // Cancel Analysis: both actions, as the shell pushes them.
    open_rows(&mut s, &[0]);
    assert!(s.state.alert().is_some());
    s.state.dispatch(Action::DismissAlert);
    s.state.dispatch(Action::CloseAnalysis);
    assert!(s.state.alert().is_none());
    assert!(s.state.analysis_form().is_none());
    assert!(transposed(&s).is_none());
    assert!(!s.state.can_undo(), "no history entry");
}

#[test]
fn partial_x_prime_is_still_refused_without_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    strip_x_prime(&mut s);
    s.session
        .handle()
        .execute(stokesforge_commands::Command::SetColumnMeta {
            column: s.cols[0],
            meta: ColumnMeta {
                x_prime: Some(1.0),
                ..ColumnMeta::default()
            },
        })
        .unwrap();
    s.state.refresh();
    open_rows(&mut s, &[0]);
    assert!(s.state.analysis_form().is_none());
    assert!(s.state.status().unwrap().is_error());
    assert!(s.state.alert().is_none());
}

#[test]
fn recalculating_after_the_x_prime_are_gone_switches_to_numbers_and_warns() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    open_rows(&mut s, &[1]);
    s.state.dispatch(Action::ApplyAnalysis);
    let sheet = transposed(&s).unwrap();
    let x = s.state.tree().children(sheet).unwrap()[0];
    strip_x_prime(&mut s);
    // Followed (the box is checked), yet it keeps its numbers and says why.
    let info = s.state.transpose_info(sheet).unwrap();
    assert!(info.auto && info.stale);
    assert_eq!(x_values(&s, x), vec![0.0, 10.0, 20.0, 30.0, 40.0, 50.0]);
    assert!(info.stuck.unwrap().contains("Recalculate"));
    assert!(s.state.alert().is_none());
    s.state.dispatch(Action::RecalculateTranspose { sheet });
    assert_eq!(x_values(&s, x), vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    let alert = s.state.alert().expect("the warning");
    assert_eq!(alert.kind, AlertKind::NoXPrime);
    assert!(!s.state.row_analysis_cancellable(), "Close only");
}

/// #493: a sheet with no Y column has no rows to analyze: the target stays
/// empty (the Analysis menu stays greyed), x' or not.
#[test]
fn a_sheet_without_a_y_column_has_no_rows_target() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup_bare(dir.path());
    s.state.select_row(s.sheet, 1);
    assert!(s.state.analysis_rows_target(s.sheet).is_some());
    for column in s.cols.clone() {
        s.session
            .handle()
            .execute(stokesforge_commands::Command::SetColumnMeta {
                column,
                meta: ColumnMeta {
                    role: stokesforge_core::ColumnRole::Label,
                    ..ColumnMeta::default()
                },
            })
            .unwrap();
    }
    s.state.refresh();
    s.state.select_row(s.sheet, 1);
    assert!(s.state.analysis_rows_target(s.sheet).is_none());
}

/// #493: the warning is not a failure of the gesture that raised it: the
/// analysis applied while it is still up is not seen as failed (witness:
/// `gesture_failed` counting the alert makes Apply refuse to go on).
#[test]
fn the_warning_does_not_make_the_next_gesture_look_failed() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup_bare(dir.path());
    open_rows(&mut s, &[1]);
    assert!(s.state.alert().is_some());
    s.state.dispatch(Action::ApplyAnalysis);
    assert_eq!(s.state.tree().calculations().len(), 1);
    assert!(s.state.analysis_form().is_none(), "the window closed");
}
