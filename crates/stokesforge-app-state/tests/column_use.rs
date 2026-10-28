//! "Column used by a calculation" (#306): the window that opens after columns
//! read by a formula or an analysis calculation are deleted (`Close` does
//! nothing, `Undo` undoes the gesture; moving a column, for its part, opens
//! nothing: `column_move.rs`, #319), and the question on Ctrl+Shift+C on
//! formula columns (values or formulas).

use stokesforge_analysis::model::Model;
use stokesforge_app_state::analysis::Preset;
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::state::{Action, AppState, FullColumnsCopy, HeaderField};
use stokesforge_core::{Column, ColumnMeta, ColumnRole, NodeId, Project};
use stokesforge_session::Session;

struct Setup {
    _session: Session,
    state: AppState,
    sheet: NodeId,
    cols: Vec<NodeId>,
}

/// A sheet with plain `A` (X), `B` (Y), `C`, `D`, over 5 rows.
fn setup(dir: &std::path::Path) -> Setup {
    i18n::set_language(Language::English);
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let values: Vec<f64> = (0..5).map(f64::from).collect();
    let cols: Vec<NodeId> = ["A", "B", "C", "D"]
        .iter()
        .map(|name| {
            project
                .add_column(sheet, None, Some(name), Column::from_f64(&values))
                .unwrap()
        })
        .collect();
    project
        .set_column_meta(
            cols[0],
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
        cols,
    }
}

fn set_formula(s: &mut Setup, column: NodeId, text: &str) {
    s.state.begin_header_edit(column, HeaderField::Formula);
    s.state.header_editing_mut().unwrap().buffer = text.to_string();
    s.state.commit_header_edit();
}

fn delete(s: &mut Setup, columns: &[NodeId]) {
    s.state.select_column(s.sheet, columns[0]);
    for &c in &columns[1..] {
        s.state.toggle_column_selection(s.sheet, c);
    }
    s.state
        .dispatch(Action::DeleteSelectedCells { sheet: s.sheet });
}

fn column_count(s: &Setup) -> usize {
    s.state.tree().children(s.sheet).unwrap().len()
}

#[test]
fn deleting_a_column_read_by_a_formula_opens_the_list_then_close_does_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    set_formula(&mut s, c[2], "A*2");
    delete(&mut s, &[c[0]]);
    assert_eq!(
        s.state.column_use_lines(),
        Some(vec!["C (formula)".to_string()])
    );
    s.state.dispatch(Action::CloseColumnUse);
    assert_eq!(s.state.column_use_lines(), None);
    assert_eq!(column_count(&s), 3, "Close does not give the column back");
}

/// #321: a formula keeps its columns by identifier, so a sliding letter
/// changes nothing about what it reads: deleting A does not warn
/// `C = B*2` (which now displays as `A*2`, B having become the first
/// column). The reader of a deleted column is still listed.
#[test]
fn sliding_letters_do_not_count_only_the_read_column_does() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    set_formula(&mut s, c[2], "B*2");
    delete(&mut s, &[c[0]]);
    assert_eq!(
        s.state.column_use_lines(),
        None,
        "C does not read A: B changes letter, not column"
    );
    assert_eq!(
        s.state.tree().formula_letters(c[2]).as_deref(),
        Some("A*2"),
        "the displayed letter follows the column"
    );

    // Witness: deleting B, which C reads, opens the list.
    delete(&mut s, &[c[1]]);
    assert_eq!(
        s.state.column_use_lines(),
        Some(vec!["C (formula)".to_string()])
    );
}

#[test]
fn a_column_without_a_reader_opens_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    delete(&mut s, &[c[3]]);
    assert_eq!(s.state.column_use_lines(), None);
    assert_eq!(column_count(&s), 3);
}

#[test]
fn undo_gives_back_the_column_and_the_formula_in_one_step_and_several_columns_make_one_list() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    set_formula(&mut s, c[3], "A+B");
    let before = s.state.column_meta(c[3]);
    delete(&mut s, &[c[0], c[1]]);
    assert_eq!(
        s.state.column_use_lines(),
        Some(vec!["D (formula)".to_string()]),
        "a single list for the whole gesture"
    );
    assert_eq!(column_count(&s), 2);
    s.state.dispatch(Action::UndoColumnUse);
    assert_eq!(s.state.column_use_lines(), None);
    assert_eq!(column_count(&s), 4, "a single Undo gives back both columns");
    assert_eq!(s.state.column_meta(c[3]), before);
}

#[test]
fn an_analysis_calculation_is_listed_by_its_family_and_its_first_output() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    s.state.dispatch(Action::OpenAnalysis {
        preset: Preset::Fit(Model::Linear),
        sheet: s.sheet,
        columns: vec![c[0], c[1]],
    });
    s.state.dispatch(Action::ApplyAnalysis);
    assert_eq!(s.state.tree().calculations().len(), 1);
    let first_output = s
        .state
        .tree()
        .calculations()
        .values()
        .next()
        .unwrap()
        .outputs
        .iter()
        .find_map(|o| o.column)
        .expect("an output column");

    // Delete Y (B): the calculation reads B, so it is listed.
    delete(&mut s, &[c[1]]);
    let lines = s.state.column_use_lines().expect("window open");
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with("Fit → [Book]Sheet1!"), "{lines:?}");
    s.state.dispatch(Action::UndoColumnUse);

    // Witness: deleting its output column opens nothing (it disappears with it).
    delete(&mut s, &[first_output]);
    assert_eq!(s.state.column_use_lines(), None);
}

#[test]
fn deleting_a_workbook_takes_its_calculations_with_it_without_signaling_anything() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    s.state.dispatch(Action::OpenAnalysis {
        preset: Preset::Fit(Model::Linear),
        sheet: s.sheet,
        columns: vec![c[0], c[1]],
    });
    s.state.dispatch(Action::ApplyAnalysis);
    // The outputs live in the same workbook: it takes the calculation with
    // it, nothing to signal.
    let book = s.state.tree().parent(s.sheet).unwrap().unwrap();
    s.state.dispatch(Action::DeleteObjects(vec![book]));
    assert!(s.state.tree().node(book).is_none(), "workbook deleted");
    assert_eq!(s.state.column_use_lines(), None);
}

#[test]
fn ctrl_shift_c_on_a_formula_column_asks_the_question() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    set_formula(&mut s, c[2], "A*2");

    s.state.select_column(s.sheet, c[2]);
    assert_eq!(s.state.copy_full_columns(s.sheet), None);
    assert_eq!(s.state.pending_full_copy(), Some(s.sheet));
    s.state
        .dispatch(Action::ConfirmFullColumnsCopy(FullColumnsCopy::Values));
    let values = s.state.take_object_clipboard_text().expect("text copied");
    assert!(!values.contains("A*2"), "the Formula row stays empty");
    assert_eq!(s.state.pending_full_copy(), None);

    s.state.select_column(s.sheet, c[2]);
    s.state.copy_full_columns(s.sheet);
    s.state
        .dispatch(Action::ConfirmFullColumnsCopy(FullColumnsCopy::Formulas));
    let formulas = s.state.take_object_clipboard_text().expect("text copied");
    assert!(formulas.contains("A*2"));

    s.state.select_column(s.sheet, c[2]);
    s.state.copy_full_columns(s.sheet);
    s.state
        .dispatch(Action::ConfirmFullColumnsCopy(FullColumnsCopy::Cancel));
    assert_eq!(s.state.take_object_clipboard_text(), None);
}

#[test]
fn ctrl_shift_c_without_a_formula_copies_directly() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    s.state.select_column(s.sheet, c[1]);
    assert!(s.state.copy_full_columns(s.sheet).is_some());
    assert_eq!(s.state.pending_full_copy(), None);
}
