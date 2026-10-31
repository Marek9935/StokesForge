//! Moving columns (#319, `Move ▸` submenu): a single Ctrl+Z, no "Column used"
//! window (a formula follows its column, #321), the selection stays on the
//! moved columns. The ordering logic is tested separately, without state
//! (`column_move::tests`).

use stokesforge_app_state::column_move::ColumnMove;
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::state::{Action, AppState, HeaderField};
use stokesforge_core::{Column, ColumnMeta, ColumnRole, NodeId, Project};
use stokesforge_session::Session;

struct Setup {
    _language: i18n::LanguageGuard,
    _session: Session,
    state: AppState,
    sheet: NodeId,
    cols: Vec<NodeId>,
}

/// A sheet `A`, `B`, `C`, `D`, `E` of distinct values over 3 rows.
fn setup(dir: &std::path::Path) -> Setup {
    let language = i18n::test_language(Language::English);
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let cols: Vec<NodeId> = ["A", "B", "C", "D", "E"]
        .iter()
        .enumerate()
        .map(|(rank, name)| {
            let base = (rank as f64 + 1.0) * 10.0;
            let id = project
                .add_column(
                    sheet,
                    None,
                    Some(name),
                    Column::from_f64(&[base, base + 1.0, base + 2.0]),
                )
                .unwrap();
            project
                .set_column_meta(
                    id,
                    ColumnMeta {
                        role: ColumnRole::Y,
                        ..ColumnMeta::default()
                    },
                )
                .unwrap();
            id
        })
        .collect();
    let session = Session::spawn(project, None).unwrap();
    let state = AppState::new_with_preferences(
        session.handle(),
        dir.join("p.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    Setup {
        _language: language,
        _session: session,
        state,
        sheet,
        cols,
    }
}

fn order(s: &Setup) -> Vec<NodeId> {
    s.state
        .visible_columns(s.sheet)
        .into_iter()
        .map(|(id, _, _)| id)
        .collect()
}

fn select(s: &mut Setup, columns: &[NodeId]) {
    s.state.select_column(s.sheet, columns[0]);
    for &column in &columns[1..] {
        s.state.toggle_column_selection(s.sheet, column);
    }
}

fn move_selection(s: &mut Setup, columns: &[NodeId], to: ColumnMove) {
    select(s, columns);
    s.state.dispatch(Action::MoveColumns {
        sheet: s.sheet,
        columns: columns.to_vec(),
        to,
    });
}

fn first_cell(s: &Setup, column: NodeId) -> String {
    s.state.cell_text(column, 0)
}

/// `A [B] C [D] E`, Left: `B A D C E`, in **a single** Undo that gives back the
/// original order, label "Move Columns". Witness: without a macro, two Undos.
#[test]
fn two_moved_columns_make_a_single_history_entry() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    move_selection(&mut s, &[c[1], c[3]], ColumnMove::Left);
    assert_eq!(order(&s), vec![c[1], c[0], c[3], c[2], c[4]]);
    let label = s.state.undo_label().expect("an entry to undo");
    assert!(label.contains("Move Columns"), "{label}");

    s.state.dispatch(Action::Undo);
    assert_eq!(order(&s), c, "a single Undo gives back the original order");
    // Nothing more to undo: the macro was the only entry.
    assert!(!s.state.can_undo());
}

/// A single column: a named `Move` entry.
#[test]
fn a_single_column_is_a_named_move_entry() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    move_selection(&mut s, &[c[1]], ColumnMove::Right);
    assert_eq!(order(&s), vec![c[0], c[2], c[1], c[3], c[4]]);
    let label = s.state.undo_label().unwrap();
    assert!(label.contains("Move “B”"), "{label}");
    s.state.dispatch(Action::Undo);
    assert_eq!(order(&s), c);
}

/// A gesture that changes nothing writes nothing (the menu entry is disabled).
#[test]
fn a_gesture_with_no_effect_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    let revision = s.state.revision();
    move_selection(&mut s, &[c[0], c[2]], ColumnMove::Left);
    move_selection(&mut s, &[c[3], c[4]], ColumnMove::Last);
    assert_eq!(order(&s), c);
    assert_eq!(s.state.revision(), revision);
    assert!(!s.state.can_undo());
}

/// A formula reads a column; moving it opens no "Column used" window, and the
/// formula keeps its values. Witness: call `open_column_use` in
/// `move_columns`.
#[test]
fn moving_a_column_read_by_a_formula_opens_no_window() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    // C = A*2
    s.state.begin_header_edit(c[2], HeaderField::Formula);
    s.state.header_editing_mut().unwrap().buffer = "A*2".to_string();
    s.state.commit_header_edit();
    let value = first_cell(&s, c[2]);
    assert_eq!(value, "20", "formula set");

    move_selection(&mut s, &[c[0]], ColumnMove::Right);

    assert_eq!(s.state.column_use_lines(), None, "no window");
    assert_eq!(first_cell(&s, c[2]), value, "the formula values are kept");
    assert_eq!(
        s.state.tree().formula_letters(c[2]).as_deref(),
        Some("B*2"),
        "the displayed letter follows the read column"
    );
    // Reading a column other than the one that moves: just as silent.
    move_selection(&mut s, &[c[1]], ColumnMove::First);
    assert_eq!(s.state.column_use_lines(), None);
    assert_eq!(first_cell(&s, c[2]), value);
}

/// After the move, the selection covers the same columns (by identifier,
/// including a range).
#[test]
fn the_selection_stays_on_the_moved_columns() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    for (columns, to) in [
        (vec![c[1], c[3]], ColumnMove::Left),
        (vec![c[1], c[3]], ColumnMove::First),
        (vec![c[1], c[3]], ColumnMove::Last),
        (vec![c[1], c[3]], ColumnMove::Right),
    ] {
        move_selection(&mut s, &columns, to);
        let selected = s
            .state
            .selection(s.sheet)
            .expect("the selection survives")
            .columns(s.state.tree(), s.sheet);
        let mut expected = columns.clone();
        expected.sort_by_key(|id| order(&s).iter().position(|o| o == id));
        assert_eq!(selected, expected, "{to:?}");
        s.state.dispatch(Action::Undo);
    }
    // A range (Shift-click) B..C moves as a block: still selected.
    s.state.select_column(s.sheet, c[1]);
    s.state.extend_column_selection(s.sheet, c[2]);
    s.state.dispatch(Action::MoveColumns {
        sheet: s.sheet,
        columns: vec![c[1], c[2]],
        to: ColumnMove::Last,
    });
    assert_eq!(order(&s), vec![c[0], c[3], c[4], c[1], c[2]]);
    let selected = s
        .state
        .selection(s.sheet)
        .unwrap()
        .columns(s.state.tree(), s.sheet);
    assert_eq!(selected, vec![c[1], c[2]]);
}

fn status_text(s: &Setup) -> String {
    s.state.status().expect("a status message").text.clone()
}

/// The status bar says the label of the **group** ("Move Columns"), like the
/// undo, and not that of the last step ("Move “B”") that `execute` had put
/// there. A single column keeps its named label. Witness: remove the
/// replacement when the group closes (`end_grouped_macro`).
#[test]
fn the_status_bar_says_the_group_label_after_a_multiple_move() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    move_selection(&mut s, &[c[1], c[3]], ColumnMove::Left);
    assert_eq!(status_text(&s), "Move Columns");
    s.state.dispatch(Action::Undo);
    move_selection(&mut s, &[c[1]], ColumnMove::Right);
    assert_eq!(status_text(&s), "Move “B”");
}

/// Same rule for any group closed by `end_grouped_macro`, not only
/// Move: here the type of several columns. Witness: the same.
#[test]
fn the_status_bar_says_the_group_label_for_another_group() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    s.state.dispatch(Action::SetColumnsType {
        columns: vec![c[0], c[1]],
        column_type: stokesforge_core::ColumnType::Mixed,
    });
    assert_eq!(status_text(&s), "Change Column Types");
}
