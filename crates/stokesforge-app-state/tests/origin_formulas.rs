//! Origin column formulas in the chooser import (#556): a formula that can be
//! translated and gives Origin's values is set live; a formula that gives other
//! values is asked about in the window `Formulas with Different Values`; every
//! other formula column keeps Origin's values and a line in its Comments.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use stokesforge_app_state::history_text::history_text;
use stokesforge_app_state::state::{Action, AppState, OriginWhere};
use stokesforge_app_state::tr;
use stokesforge_commands::{HistoryLabel, MacroKind};
use stokesforge_core::{ColumnFormula, FormulaPart, NodeId, Project, Tree};
use stokesforge_import::origin::synthetic::{self, Cell, Column};
use stokesforge_import::origin::OriginDesignation::{X, Y};
use stokesforge_import::FilesLayout;
use stokesforge_session::Session;

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Redirects the config folder of the import recipes for the test.
struct Isolation {
    _lock: std::sync::MutexGuard<'static, ()>,
    _dir: tempfile::TempDir,
    previous: [(&'static str, Option<std::ffi::OsString>); 2],
}

impl Drop for Isolation {
    fn drop(&mut self) {
        for (name, value) in &self.previous {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

fn isolate() -> Isolation {
    let lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let names = [
        stokesforge_import::config_dir::CONFIG_DIR_VARIABLE,
        "XDG_CONFIG_HOME",
    ];
    let previous = names.map(|name| (name, std::env::var_os(name)));
    for name in names {
        std::env::set_var(name, dir.path());
    }
    Isolation {
        _lock: lock,
        _dir: dir,
        previous,
    }
}

fn start(dir: &Path) -> (Session, AppState) {
    let session = Session::spawn(Project::new("P").unwrap(), None).unwrap();
    let state = AppState::new_with_preferences(
        session.handle(),
        dir.join("p.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    (session, state)
}

fn numbers(
    short: &str,
    designation: stokesforge_import::origin::OriginDesignation,
    v: &[f64],
) -> Column {
    Column::numbers(short, designation, v)
}

fn with_formula(mut column: Column, text: &str) -> Column {
    column.formula = Some(text.to_string());
    column
}

fn sheet(name: &str, columns: Vec<Column>) -> synthetic::Sheet {
    synthetic::Sheet {
        name: name.into(),
        columns,
    }
}

fn book(short: &str, long: &str, sheets: Vec<synthetic::Sheet>) -> synthetic::Book {
    synthetic::Book {
        short_name: short.into(),
        long_name: long.into(),
        sheets,
        horiba_xml: None,
    }
}

/// Opens the chooser on one Origin file with every column of every sheet
/// ticked.
fn open_all(state: &mut AppState, dir: &Path, books: Vec<synthetic::Book>) -> PathBuf {
    let path = dir.join("f.opj");
    let bytes = synthetic::write_project(&synthetic::Project {
        books,
        ..synthetic::Project::default()
    });
    std::fs::write(&path, bytes).unwrap();
    state.dispatch(Action::DropFiles {
        paths: vec![path.clone()],
        target_book: None,
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while state.import_progress().is_some() {
        state.poll_import();
        assert!(std::time::Instant::now() < deadline, "import too long");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    state.dispatch(Action::CheckAllOriginSheets);
    path
}

fn book_named(tree: &Tree, name: &str) -> NodeId {
    let root = tree.root();
    *tree
        .node(root)
        .unwrap()
        .children()
        .iter()
        .find(|&&id| tree.node(id).is_some_and(|n| n.name() == name))
        .unwrap_or_else(|| panic!("workbook {name}"))
}

/// The columns of the first sheet of `book`.
fn columns_of(state: &AppState, book: &str, sheet: usize) -> Vec<NodeId> {
    let tree = state.tree();
    let book = book_named(tree, book);
    let sheet = tree.node(book).unwrap().children()[sheet];
    tree.node(sheet).unwrap().children().to_vec()
}

fn bits(state: &AppState, column: NodeId) -> Vec<u64> {
    let node = state.tree().node(column).unwrap().as_column().unwrap();
    (0..node.len())
        .map(|row| node.values().number_at(row).to_bits())
        .collect()
}

fn formula_of(state: &AppState, column: NodeId) -> Option<ColumnFormula> {
    state.column_meta(column).formula
}

/// `Data`: X, Y (10 20 30) and a formula column `col(Y)*2` of the given values.
fn doubled(values: &[f64]) -> Vec<synthetic::Book> {
    vec![book(
        "A",
        "Run",
        vec![sheet(
            "Data",
            vec![
                numbers("X", X, &[1.0, 2.0, 3.0]),
                numbers("Y", Y, &[10.0, 20.0, 30.0]),
                with_formula(numbers("C", Y, values), "col(Y)*2"),
            ],
        )],
    )]
}

#[test]
fn a_translated_formula_is_set_and_one_undo_removes_the_book() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let before = state.tree().len();
    open_all(&mut state, dir.path(), doubled(&[20.0, 40.0, 60.0]));
    state.dispatch(Action::ImportOrigin);
    assert!(!state.origin_formulas_open(), "equal values: no question");
    let columns = columns_of(&state, "Run", 0);
    let formula = formula_of(&state, columns[2]).expect("the formula is set");
    assert!(formula
        .parts()
        .iter()
        .any(|part| matches!(part, FormulaPart::Column(id) if *id == columns[1])));
    assert_eq!(
        bits(&state, columns[2]),
        [20.0f64, 40.0, 60.0].map(f64::to_bits)
    );
    assert!(
        !state
            .column_meta(columns[2])
            .comment
            .contains("Origin formula"),
        "a formula that is set leaves no note"
    );
    // One entry: a single undo removes the book, the formula with it.
    assert_eq!(
        state.undo_label(),
        Some(tr!(
            "history-undo-entry",
            label = history_text(&HistoryLabel::Macro(MacroKind::Import))
        ))
    );
    state.dispatch(Action::Undo);
    assert_eq!(state.tree().len(), before, "one undo removes everything");
    state.dispatch(Action::Redo);
    let columns = columns_of(&state, "Run", 0);
    assert!(formula_of(&state, columns[2]).is_some(), "redo restores");
    assert_eq!(
        bits(&state, columns[2]),
        [20.0f64, 40.0, 60.0].map(f64::to_bits)
    );
}

#[test]
fn nan_and_trailing_empty_cells_are_equal() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let books = vec![book(
        "A",
        "Run",
        vec![sheet(
            "Data",
            vec![
                numbers("Y", Y, &[10.0, f64::NAN, 30.0, f64::NAN]),
                with_formula(
                    numbers("C", Y, &[20.0, f64::NAN, 60.0, f64::NAN, f64::NAN]),
                    "col(Y)*2",
                ),
            ],
        )],
    )];
    open_all(&mut state, dir.path(), books);
    state.dispatch(Action::ImportOrigin);
    assert!(
        !state.origin_formulas_open(),
        "NaN = NaN, trailing empties ignored"
    );
    let columns = columns_of(&state, "Run", 0);
    assert!(formula_of(&state, columns[1]).is_some());
}

#[test]
fn a_stale_formula_asks_and_keeps_origins_bits_unless_ticked() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let before = state.tree().len();
    open_all(&mut state, dir.path(), doubled(&[1.5, 1.5, 1.5]));
    state.dispatch(Action::ImportOrigin);
    assert!(state.origin_formulas_open());
    assert_eq!(state.tree().len(), before, "nothing is created yet");
    assert!(!state.origin_import_open(), "one modal at a time");
    let rows = state.origin_formulas_rows();
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].ticked, "all unticked");
    assert!(rows[0].label.contains("Run") && rows[0].label.contains("col(Y)*2"));
    assert!(rows[0].example.contains("1.5") && rows[0].example.contains("20"));

    // Cancel: nothing created, the chooser is back.
    state.dispatch(Action::CancelOriginFormulas);
    assert!(!state.origin_formulas_open());
    assert!(state.origin_import_open(), "the chooser stays open");
    assert_eq!(state.tree().len(), before);
    assert!(!state.can_undo(), "history unchanged");

    // Import with the box unticked: Origin's bits, no formula, a note.
    state.dispatch(Action::ImportOrigin);
    state.dispatch(Action::ImportOriginFormulas);
    let columns = columns_of(&state, "Run", 0);
    assert!(formula_of(&state, columns[2]).is_none());
    assert_eq!(bits(&state, columns[2]), [1.5f64; 3].map(f64::to_bits));
    assert_eq!(
        state.column_meta(columns[2]).comment,
        "Origin formula: col(Y)*2"
    );
}

#[test]
fn a_ticked_stale_formula_is_set_and_reopening_keeps_the_answer() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    open_all(&mut state, dir.path(), doubled(&[1.5, 1.5, 1.5]));
    state.dispatch(Action::ImportOrigin);
    state.dispatch(Action::SetOriginFormulaTick {
        index: 0,
        ticked: true,
    });
    state.dispatch(Action::ImportOriginFormulas);
    let columns = columns_of(&state, "Run", 0);
    assert!(formula_of(&state, columns[2]).is_some());
    assert_eq!(
        bits(&state, columns[2]),
        [20.0f64, 40.0, 60.0].map(f64::to_bits),
        "the formula's values replace Origin's"
    );
    let status = state.status().expect("a status line").text.clone();
    assert!(
        status.contains("1 formula set, 0 kept as values"),
        "{status}"
    );
    // Reopen Last Import: the window keeps the tick.
    state.dispatch(Action::ReopenLastImport);
    state.dispatch(Action::ImportOrigin);
    assert!(state.origin_formulas_open());
    assert!(state.origin_formulas_rows()[0].ticked);
}

#[test]
fn an_untranslatable_formula_keeps_its_values_and_the_text_in_the_comments() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let mut commented = with_formula(numbers("C", Y, &[10.0, 30.0, 60.0]), "sum(Y)");
    commented.comment = "cumulative".into();
    let books = vec![book(
        "A",
        "Run",
        vec![sheet(
            "Data",
            vec![numbers("Y", Y, &[10.0, 20.0, 30.0]), commented],
        )],
    )];
    open_all(&mut state, dir.path(), books);
    state.dispatch(Action::ImportOrigin);
    assert!(!state.origin_formulas_open(), "not a question");
    let columns = columns_of(&state, "Run", 0);
    assert!(formula_of(&state, columns[1]).is_none());
    assert_eq!(
        bits(&state, columns[1]),
        [10.0f64, 30.0, 60.0].map(f64::to_bits)
    );
    assert_eq!(
        state.column_meta(columns[1]).comment,
        "cumulative; Origin formula: sum(Y)"
    );
}

#[test]
fn a_formula_reading_a_sheet_created_later_resolves() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let books = vec![book(
        "A",
        "Run",
        vec![
            sheet(
                "First",
                vec![
                    numbers("Y", Y, &[1.0, 2.0, 3.0]),
                    with_formula(numbers("C", Y, &[30.0, 60.0, 90.0]), "Later!B*3"),
                ],
            ),
            sheet(
                "Later",
                vec![
                    numbers("A", X, &[0.0, 0.0, 0.0]),
                    numbers("B", Y, &[10.0, 20.0, 30.0]),
                ],
            ),
        ],
    )];
    open_all(&mut state, dir.path(), books);
    state.dispatch(Action::ImportOrigin);
    assert!(!state.origin_formulas_open());
    let columns = columns_of(&state, "Run", 0);
    let formula = formula_of(&state, columns[1]).expect("set");
    assert_eq!(formula.remote_refs().len(), 1, "read in another sheet");
    assert_eq!(
        bits(&state, columns[1]),
        [30.0f64, 60.0, 90.0].map(f64::to_bits)
    );
}

#[test]
fn a_renamed_sheet_still_resolves_by_its_origin_name() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    // `a/b` is refused by the model: the sheet keeps an automatic name.
    let books = vec![book(
        "A",
        "Run",
        vec![
            sheet(
                "First",
                vec![
                    numbers("Y", Y, &[1.0, 2.0]),
                    with_formula(numbers("C", Y, &[11.0, 21.0]), "'a/b'!B+1"),
                ],
            ),
            sheet("a/b", vec![numbers("B", Y, &[10.0, 20.0])]),
        ],
    )];
    open_all(&mut state, dir.path(), books);
    state.dispatch(Action::ImportOrigin);
    let columns = columns_of(&state, "Run", 0);
    assert!(formula_of(&state, columns[1]).is_some(), "resolved by name");
    assert_eq!(bits(&state, columns[1]), [11.0f64, 21.0].map(f64::to_bits));
}

/// Two sheets X Y, the second one's `C` reading its own X.
fn two_sheets() -> Vec<synthetic::Book> {
    vec![book(
        "A",
        "Run",
        vec![
            sheet(
                "S1",
                vec![numbers("X", X, &[1.0, 2.0]), numbers("Y", Y, &[5.0, 6.0])],
            ),
            sheet(
                "S2",
                vec![
                    numbers("X", X, &[1.0, 2.0]),
                    with_formula(numbers("C", Y, &[10.0, 40.0]), "col(X)*col(X)*10"),
                ],
            ),
        ],
    )]
}

#[test]
fn a_shared_x_formula_reads_the_kept_x_and_a_side_by_side_one_its_own() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    // Shared X: the second X is dropped, the formula reads the first one.
    let (_session, mut state) = start(dir.path());
    open_all(&mut state, dir.path(), two_sheets());
    state.dispatch(Action::SetOriginWhere(OriginWhere::NewWorkbook));
    state.dispatch(Action::SetOriginLayout(FilesLayout::SharedX));
    state.dispatch(Action::ImportOrigin);
    let keys = state.take_origin_created_columns();
    assert!(
        !keys.contains_key(&(0, 0, 1, 0)),
        "the second X was dropped"
    );
    let kept_x = keys[&(0, 0, 0, 0)].expect("the first X");
    let c = keys[&(0, 0, 1, 1)].expect("C");
    let formula = formula_of(&state, c).expect("formula set");
    let reads: Vec<NodeId> = formula
        .parts()
        .iter()
        .filter_map(|part| match part {
            FormulaPart::Column(id) => Some(*id),
            FormulaPart::Text(_) => None,
        })
        .collect();
    assert_eq!(reads, [kept_x, kept_x]);
    assert!(formula.remote_refs().is_empty(), "local");

    // Side by side: every X is kept, the formula reads its own.
    let (_session, mut state) = start(dir.path());
    open_all(&mut state, dir.path(), two_sheets());
    state.dispatch(Action::SetOriginWhere(OriginWhere::NewWorkbook));
    state.dispatch(Action::SetOriginLayout(FilesLayout::SideBySide));
    state.dispatch(Action::ImportOrigin);
    let keys = state.take_origin_created_columns();
    let own_x = keys[&(0, 0, 1, 0)].expect("the second X is kept");
    let c = keys[&(0, 0, 1, 1)].expect("C");
    let formula = formula_of(&state, c).expect("formula set");
    assert!(formula
        .parts()
        .iter()
        .any(|part| matches!(part, FormulaPart::Column(id) if *id == own_x)));
    assert!(formula.remote_refs().is_empty());
}

#[test]
fn an_unchecked_input_gives_an_orange_line_and_values_only() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    open_all(&mut state, dir.path(), doubled(&[20.0, 40.0, 60.0]));
    // Uncheck Y (column 1): C reads it.
    state.dispatch(Action::ToggleOriginColumn {
        file: 0,
        book: 0,
        sheet: 0,
        column: 1,
    });
    let files = state.origin_import_files();
    let columns = &files[0].books[0].sheets[0].column_list;
    assert!(columns[2].reads_unticked == ["Y"], "{:?}", columns[2]);
    assert!(columns[1].reads_unticked.is_empty());
    // Never ticked for the user.
    assert!(!columns[1].ticked);
    state.dispatch(Action::ImportOrigin);
    assert!(!state.origin_formulas_open());
    let columns = columns_of(&state, "Run", 0);
    assert_eq!(columns.len(), 2);
    assert!(formula_of(&state, columns[1]).is_none());
    assert_eq!(
        bits(&state, columns[1]),
        [20.0f64, 40.0, 60.0].map(f64::to_bits)
    );
    assert_eq!(
        state.column_meta(columns[1]).comment,
        "Origin formula: col(Y)*2"
    );
}

#[test]
fn a_text_cell_of_origin_counts_as_a_difference() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let mut c = with_formula(numbers("C", Y, &[20.0, 40.0, 60.0]), "col(Y)*2");
    c.cells[1] = Cell::Text("n/a".into());
    let books = vec![book(
        "A",
        "Run",
        vec![sheet("Data", vec![numbers("Y", Y, &[10.0, 20.0, 30.0]), c])],
    )];
    open_all(&mut state, dir.path(), books);
    state.dispatch(Action::ImportOrigin);
    assert!(state.origin_formulas_open());
    assert!(state.origin_formulas_rows()[0].example.contains("n/a"));
}

#[test]
fn a_formula_that_reads_itself_or_loops_is_not_set() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let books = vec![book(
        "A",
        "Run",
        vec![sheet(
            "Data",
            vec![
                with_formula(numbers("A", Y, &[1.0, 2.0]), "col(B)+1"),
                with_formula(numbers("B", Y, &[0.0, 1.0]), "col(A)-1"),
                with_formula(numbers("C", Y, &[1.0, 2.0]), "col(C)"),
            ],
        )],
    )];
    open_all(&mut state, dir.path(), books);
    state.dispatch(Action::ImportOrigin);
    assert!(!state.origin_formulas_open(), "refused, not asked");
    let columns = columns_of(&state, "Run", 0);
    for column in columns {
        assert!(formula_of(&state, column).is_none());
        assert!(state
            .column_meta(column)
            .comment
            .starts_with("Origin formula: "));
    }
}

/// Measurement only (#556): the import of a sheet of 570 formula columns, each
/// read by the recalculation after every command. Prints the durations.
#[test]
#[ignore = "measurement only"]
fn measure_the_import_of_570_formula_columns() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let rows: Vec<f64> = (0..200).map(f64::from).collect();
    let mut columns = vec![numbers("B", Y, &rows)];
    for k in 1..=570 {
        let values: Vec<f64> = rows.iter().map(|v| v * f64::from(k)).collect();
        columns.push(with_formula(
            numbers(&format!("F{k}"), Y, &values),
            &format!("col(B)*{k}"),
        ));
    }
    open_all(
        &mut state,
        dir.path(),
        vec![book("A", "Run", vec![sheet("Data", columns)])],
    );
    let started = std::time::Instant::now();
    state.dispatch(Action::ImportOrigin);
    let elapsed = started.elapsed();
    let created = columns_of(&state, "Run", 0);
    let set = created
        .iter()
        .filter(|&&id| formula_of(&state, id).is_some())
        .count();
    println!("570 formula columns, 200 rows: import {elapsed:.2?}, {set} formulas set");
    assert_eq!(set, 570);
}
