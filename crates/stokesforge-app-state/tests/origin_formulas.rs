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
    let path = open_unticked(
        state,
        dir,
        synthetic::Project {
            books,
            ..synthetic::Project::default()
        },
    );
    state.dispatch(Action::CheckAllOriginSheets);
    path
}

/// Opens the chooser on one Origin file with nothing ticked.
fn open_unticked(state: &mut AppState, dir: &Path, project: synthetic::Project) -> PathBuf {
    let path = dir.join("f.opj");
    let bytes = synthetic::write_project(&project);
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

#[test]
fn the_orange_line_of_the_chooser_counts_the_formulas_that_read_an_unchecked_column() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let mut columns = vec![
        numbers("X", X, &[1.0, 2.0, 3.0]),
        numbers("Y", Y, &[10.0, 20.0, 30.0]),
    ];
    for name in ["C", "D", "E", "F", "G"] {
        columns.push(with_formula(
            numbers(name, Y, &[20.0, 40.0, 60.0]),
            "col(Y)*2",
        ));
    }
    open_all(
        &mut state,
        dir.path(),
        vec![book("A", "Run", vec![sheet("Data", columns)])],
    );
    assert_eq!(state.origin_import_reads_warning(), None, "all checked");
    let toggle_y = || Action::ToggleOriginColumn {
        file: 0,
        book: 0,
        sheet: 0,
        column: 1,
    };
    state.dispatch(toggle_y());
    let line = state.origin_import_reads_warning().expect("a warning");
    assert_eq!(
        line,
        tr!(
            "origin-import-reads-warning",
            count = 5_i64,
            names = tr!(
                "origin-import-reads-names-more",
                names = "C, D, E",
                more = 2_i64
            )
        )
    );
    // A formula column that is not checked itself is not counted.
    state.dispatch(Action::ToggleOriginColumn {
        file: 0,
        book: 0,
        sheet: 0,
        column: 2,
    });
    let line = state.origin_import_reads_warning().expect("still");
    assert!(line.contains("D, E, F"), "{line}");
    state.dispatch(toggle_y());
    assert_eq!(state.origin_import_reads_warning(), None, "input checked");
}

// ---- Include the columns formulas read (#556, #557) -----------------------

/// `Data`: X (col 0), B (1), C = `col(B)*2` (2), D = `col(C)+1` (3), all the
/// values consistent so that the formulas are set live.
fn chain() -> synthetic::Project {
    synthetic::Project {
        books: vec![book(
            "A",
            "Run",
            vec![
                sheet(
                    "Data",
                    vec![
                        numbers("X", X, &[1.0, 2.0, 3.0]),
                        numbers("B", Y, &[10.0, 20.0, 30.0]),
                        with_formula(numbers("C", Y, &[20.0, 40.0, 60.0]), "col(B)*2"),
                        with_formula(numbers("D", Y, &[21.0, 41.0, 61.0]), "col(C)+1"),
                    ],
                ),
                sheet(
                    "Raw",
                    vec![
                        numbers("A", X, &[1.0, 2.0, 3.0]),
                        numbers("B", Y, &[5.0, 6.0, 7.0]),
                        numbers("E", Y, &[0.0, 0.0, 0.0]),
                    ],
                ),
            ],
        )],
        ..synthetic::Project::default()
    }
}

fn tick(state: &mut AppState, sheet: usize, column: usize) {
    state.dispatch(Action::ToggleOriginColumn {
        file: 0,
        book: 0,
        sheet,
        column,
    });
}

/// The ticks of the Data and Raw sheets, as the tree shows them.
fn shown(state: &AppState) -> Vec<Vec<bool>> {
    state.origin_import_files()[0].books[0]
        .sheets
        .iter()
        .map(|sheet| sheet.column_list.iter().map(|c| c.ticked).collect())
        .collect()
}

fn on(state: &mut AppState) {
    state.dispatch(Action::SetOriginIncludeReads(true));
}

fn columns_named(state: &AppState) -> Vec<String> {
    let tree = state.tree();
    columns_of(state, "Run", 0)
        .iter()
        .map(|&id| tree.node(id).unwrap().name().to_string())
        .collect()
}

#[test]
fn the_box_brings_the_column_a_ticked_formula_reads_and_the_formula_is_set() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    open_unticked(&mut state, dir.path(), chain());
    assert!(!state.origin_import_include_reads(), "off at opening");
    tick(&mut state, 0, 2);
    // Off: C alone, an orange line, and C keeps Origin's values.
    assert!(state.origin_import_reads_warning().is_some());
    on(&mut state);
    assert!(state.origin_import_include_reads());
    assert_eq!(shown(&state)[0], [false, true, true, false]);
    assert_eq!(state.origin_import_reads_warning(), None);
    state.dispatch(Action::ImportOrigin);
    let columns = columns_of(&state, "Run", 0);
    assert_eq!(columns.len(), 2, "B and C");
    assert!(formula_of(&state, columns[1]).is_some(), "C is live");
    assert_eq!(
        bits(&state, columns[0]),
        [10.0f64, 20.0, 30.0].map(f64::to_bits)
    );
}

#[test]
fn without_the_box_the_formula_keeps_the_values() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    open_unticked(&mut state, dir.path(), chain());
    tick(&mut state, 0, 2);
    state.dispatch(Action::ImportOrigin);
    let columns = columns_of(&state, "Run", 0);
    assert_eq!(columns.len(), 1);
    assert!(formula_of(&state, columns[0]).is_none());
}

#[test]
fn the_columns_are_read_transitively_and_across_sheets() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let mut project = chain();
    // E reads Raw's own B and Data!C by name: D -> C -> B, E -> Data!D.
    project.books[0].sheets[1].columns[2] =
        with_formula(numbers("E", Y, &[22.0, 42.0, 62.0]), "Data!D+1");
    open_unticked(&mut state, dir.path(), project);
    tick(&mut state, 1, 2);
    on(&mut state);
    assert_eq!(shown(&state)[0], [false, true, true, true], "B, C, D");
    assert_eq!(shown(&state)[1], [false, false, true], "E itself");
    let view = &state.origin_import_files()[0].books[0].sheets[0].column_list;
    assert_eq!(view[3].read_by.as_deref(), Some("E"));
    assert_eq!(view[2].read_by.as_deref(), Some("D"));
    assert_eq!(view[1].read_by.as_deref(), Some("C"));
    assert_eq!(view[0].read_by, None, "X is not read");
    state.dispatch(Action::ImportOrigin);
    assert!(!state.origin_formulas_open());
    let columns = columns_of(&state, "Run", 0);
    assert_eq!(columns.len(), 3);
    assert!(formula_of(&state, columns[2]).is_some());
}

#[test]
fn the_x_column_an_at_form_reads_comes_too() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let mut project = chain();
    // table(X, B, 2): reads Raw's A (the X) and B.
    project.books[0].sheets[0].columns[3] =
        with_formula(numbers("D", Y, &[6.0, 6.0, 6.0]), "table(Raw!A, Raw!B, 2)");
    open_unticked(&mut state, dir.path(), project);
    tick(&mut state, 0, 3);
    on(&mut state);
    assert_eq!(shown(&state)[1], [true, true, false]);
}

#[test]
fn the_box_off_returns_to_exactly_the_previous_ticks() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    open_unticked(&mut state, dir.path(), chain());
    tick(&mut state, 0, 3);
    let before = shown(&state);
    on(&mut state);
    assert_eq!(shown(&state)[0], [false, true, true, true]);
    state.dispatch(Action::SetOriginIncludeReads(false));
    assert_eq!(shown(&state), before, "what only the overlay added is gone");
    // A column ticked by hand stays when the box goes off.
    tick(&mut state, 0, 1);
    on(&mut state);
    state.dispatch(Action::SetOriginIncludeReads(false));
    assert_eq!(shown(&state)[0], [false, true, false, true]);
}

#[test]
fn a_click_on_a_column_the_overlay_ticked_changes_nothing_and_the_box_stays() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    open_unticked(&mut state, dir.path(), chain());
    tick(&mut state, 0, 2);
    on(&mut state);
    tick(&mut state, 0, 1);
    assert_eq!(shown(&state)[0], [false, true, true, false]);
    assert!(state.origin_import_include_reads());
    // A column ticked by hand is not locked; unticking it while a ticked
    // formula reads it leaves it ticked by the overlay, and locked.
    state.dispatch(Action::SetOriginIncludeReads(false));
    tick(&mut state, 0, 1);
    on(&mut state);
    let view = &state.origin_import_files()[0].books[0].sheets[0].column_list;
    assert_eq!(view[1].read_by, None, "ticked by hand, not locked");
    tick(&mut state, 0, 1);
    let view = &state.origin_import_files()[0].books[0].sheets[0].column_list;
    assert!(view[1].ticked, "still read by C");
    assert_eq!(view[1].read_by.as_deref(), Some("C"), "locked now");
}

#[test]
fn a_sheet_whose_columns_are_all_read_is_locked() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let mut project = chain();
    project.books[0].sheets[1].columns[2] = with_formula(numbers("E", Y, &[0.0; 3]), "Raw!B");
    project.books[0].sheets[0].columns[3] =
        with_formula(numbers("D", Y, &[0.0; 3]), "Raw!A+Raw!B+Raw!E");
    open_unticked(&mut state, dir.path(), project);
    tick(&mut state, 0, 3);
    on(&mut state);
    let sheet = &state.origin_import_files()[0].books[0].sheets[1];
    assert!(sheet.locked, "A, B and E are read");
    state.dispatch(Action::ToggleOriginSheet {
        file: 0,
        book: 0,
        sheet: 1,
    });
    assert_eq!(shown(&state)[1], [true, true, true], "no change, greyed");
}

#[test]
fn a_graph_plotting_a_formula_column_brings_its_inputs_and_unticking_removes_them() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let mut project = chain();
    // Datasets in file order: X 1, B 2, C 3, D 4, A 5, B 6, E 7. G plots C vs X.
    project.graph_windows = vec![synthetic::GraphWindow {
        short_name: "G1".into(),
        long_name: "G1".into(),
        layers: vec![synthetic::GraphLayer {
            name: "Layer1".into(),
            curves: vec![synthetic::GraphCurve::new(3, 1)],
        }],
    }];
    open_unticked(&mut state, dir.path(), project);
    on(&mut state);
    state.dispatch(Action::ToggleOriginGraph { file: 0, graph: 0 });
    assert_eq!(shown(&state)[0], [true, true, true, false], "X, C and B");
    state.dispatch(Action::ToggleOriginGraph { file: 0, graph: 0 });
    assert_eq!(shown(&state)[0], [false; 4], "B goes with C");
}

#[test]
fn ticks_made_after_turning_the_box_on_are_covered() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    open_unticked(&mut state, dir.path(), chain());
    on(&mut state);
    assert_eq!(shown(&state)[0], [false; 4]);
    assert!(state.origin_import_summary().is_none());
    tick(&mut state, 0, 3);
    assert_eq!(shown(&state)[0], [false, true, true, true]);
    assert!(state.origin_import_summary().is_some());
    tick(&mut state, 0, 3);
    assert_eq!(shown(&state)[0], [false; 4]);
    assert!(state.origin_import_blocker().is_some(), "nothing to import");
}

#[test]
fn a_loop_between_two_formula_columns_ends() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let project = synthetic::Project {
        books: vec![book(
            "A",
            "Run",
            vec![sheet(
                "Data",
                vec![
                    with_formula(numbers("A", Y, &[1.0, 2.0]), "col(B)+1"),
                    with_formula(numbers("B", Y, &[0.0, 1.0]), "col(A)-1"),
                    with_formula(numbers("C", Y, &[0.0, 1.0]), "col(C)"),
                ],
            )],
        )],
        ..synthetic::Project::default()
    };
    open_unticked(&mut state, dir.path(), project);
    on(&mut state);
    tick(&mut state, 0, 0);
    assert_eq!(shown(&state)[0], [true, true, false]);
    tick(&mut state, 0, 2);
    assert_eq!(
        shown(&state)[0],
        [true, true, true],
        "a self read is no loop"
    );
}

#[test]
fn the_overlay_feeds_the_plan_with_shared_x_and_the_summary() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    open_unticked(&mut state, dir.path(), chain());
    tick(&mut state, 0, 3);
    state.dispatch(Action::SetOriginLayout(
        stokesforge_import::FilesLayout::SideBySide,
    ));
    on(&mut state);
    state.dispatch(Action::ImportOrigin);
    assert_eq!(columns_named(&state).len(), 3, "B, C, D side by side");
}

#[test]
fn check_none_clears_the_ticks_and_leaves_the_box_and_reopen_restores_it() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    open_unticked(&mut state, dir.path(), chain());
    on(&mut state);
    tick(&mut state, 0, 2);
    state.dispatch(Action::CheckNoOriginSheets);
    assert_eq!(shown(&state)[0], [false; 4]);
    assert!(state.origin_import_include_reads(), "the box stays");
    tick(&mut state, 0, 2);
    state.dispatch(Action::ImportOrigin);
    assert_eq!(columns_of(&state, "Run", 0).len(), 2);
    state.dispatch(Action::ReopenLastImport);
    assert!(state.origin_import_include_reads(), "restored");
    assert_eq!(shown(&state)[0], [false, true, true, false]);
}

#[test]
fn the_box_is_greyed_out_when_no_formula_reads_a_column() {
    let _isolation = isolate();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let project = synthetic::Project {
        books: vec![book(
            "A",
            "Run",
            vec![sheet("Data", vec![numbers("X", X, &[1.0])])],
        )],
        ..synthetic::Project::default()
    };
    open_unticked(&mut state, dir.path(), project);
    assert_eq!(
        state.origin_import_include_reads_blocker(),
        Some(tr!("disabled-origin-include-reads-no-formula"))
    );
    let other = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(other.path());
    open_unticked(&mut state, other.path(), chain());
    assert_eq!(state.origin_import_include_reads_blocker(), None);
}
