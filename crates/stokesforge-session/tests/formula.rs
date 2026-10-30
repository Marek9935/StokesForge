//! Integration tests of the recalculation of calculated columns (v1), through
//! the public API of `stokesforge-session` only — see the top-level docs of
//! `stokesforge-session::recalc::recalc_formulas` for the exact scope of the
//! trigger that these tests check from the outside.

use stokesforge_commands::{ColumnRef, Command, HistoryLabel};
use stokesforge_core::{
    Cell, Column, ColumnMeta, ColumnType, MixedCell, NodeId, Project, ValueBlock,
};
use stokesforge_session::{Session, SessionHandle, Snapshot};

/// `Book1 → Sheet1` with `A [1,2,3]`, `B [10,20,30]` and an empty numeric
/// column `C` — something to put a formula on in each test. Returns
/// `(project, a, b, c)`.
fn fixture() -> (Project, NodeId, NodeId, NodeId) {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    project
        .add_workbook(root, Some("Book1"))
        .expect("workbook created");
    let sheet = project
        .resolve_path("[Book1]Sheet1")
        .expect("sheet resolved");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("column A");
    let b = project
        .add_column(
            sheet,
            None,
            Some("B"),
            Column::from_f64(&[10.0, 20.0, 30.0]),
        )
        .expect("column B");
    let c = project
        .add_column(sheet, None, Some("C"), Column::new(ColumnType::Numeric))
        .expect("column C");
    (project, a, b, c)
}

fn read_floats(snapshot: &Snapshot, column: NodeId) -> Vec<f64> {
    let node = snapshot.tree.node(column).expect("column present");
    let column = node.as_column().expect("the node is a column");
    (0..column.len())
        .map(|row| match column.get(row) {
            Cell::Float(value) => value,
            Cell::Text(_) => unreachable!("fixture columns are always numeric"),
        })
        .collect()
}

/// The command that sets `formula` (entered in **letters**): letters to
/// identifiers by `Tree::formula_from_letters`, like the interface (#321).
fn set_formula(handle: &SessionHandle, column: NodeId, formula: &str) -> Command {
    let formula = handle
        .snapshot()
        .expect("snapshot")
        .tree
        .formula_from_letters(column, formula)
        .expect("formula entered");
    Command::SetColumnMeta {
        column,
        meta: ColumnMeta {
            formula: Some(formula),
            ..Default::default()
        },
    }
}

fn set_one(column: NodeId, start: usize, value: f64) -> Command {
    Command::SetCells {
        column,
        start,
        values: ValueBlock::Float(vec![value]),
        expected_version: None,
    }
}

/// Setting a formula (`SetColumnMeta`) recalculates the column immediately —
/// C = A+B (position letters: A is the first column of the sheet, B the
/// second), without any command writing the values itself.
#[test]
fn setting_a_formula_recalculates_immediately() {
    let (project, a, b, c) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    handle
        .execute(set_formula(&handle, c, "A+B"))
        .expect("set the formula");
    let snapshot = handle.snapshot().expect("snapshot");
    assert_eq!(read_floats(&snapshot, c), vec![11.0, 22.0, 33.0]);
    // The source columns did not move.
    assert_eq!(read_floats(&snapshot, a), vec![1.0, 2.0, 3.0]);
    assert_eq!(read_floats(&snapshot, b), vec![10.0, 20.0, 30.0]);
}

/// Editing a source column (`SetCells`, an ordinary data command, normally
/// undoable) triggers a silent recalculation of the formula column that
/// depends on it — without creating its own history entry: a single `undo` is
/// enough to go back on *both* the edited cell *and* the recalculated value.
#[test]
fn editing_a_source_column_recalculates_the_formula_column() {
    let (project, a, _b, c) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    handle
        .execute(set_formula(&handle, c, "A+B"))
        .expect("set the formula");
    let undo_label_after_formula = handle.snapshot().expect("snapshot").history.undo_label;

    handle.execute(set_one(a, 0, 100.0)).expect("write into A");
    let snapshot = handle.snapshot().expect("snapshot");
    assert_eq!(read_floats(&snapshot, a)[0], 100.0);
    assert_eq!(read_floats(&snapshot, c)[0], 110.0);
    assert_eq!(
        snapshot.history.undo_label,
        Some(HistoryLabel::SetCells {
            column: Some(ColumnRef(0))
        }),
        "the recalculation does not push its own history entry"
    );

    handle.undo().expect("undo the input into A");
    let snapshot = handle.snapshot().expect("snapshot");
    assert_eq!(read_floats(&snapshot, a)[0], 1.0);
    assert_eq!(
        read_floats(&snapshot, c)[0],
        11.0,
        "the formula column must reflect the restored value of A, not stay on 110.0"
    );
    assert_eq!(
        snapshot.history.undo_label, undo_label_after_formula,
        "a single undo was enough: the formula itself is still the next entry"
    );
}

/// A dependency cycle between two formula columns (`C = D+1`, `D = C+1`) never
/// loops indefinitely: both columns receive an entirely missing block
/// (`NumericStatus::Missing` everywhere) rather than a value that would make
/// no sense to choose anyway (invariant 10). Each references a column, so
/// retypes to `Mixed` (`desired_formula_column_type`) and shows `"---"` rather
/// than an empty cell — a cycle is a result not calculated like any other.
#[test]
fn a_dependency_cycle_degrades_to_nan_without_hanging() {
    let (project, _a, _b, c) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    let sheet = handle
        .snapshot()
        .expect("snapshot")
        .tree
        .node(c)
        .expect("column C")
        .parent()
        .expect("C has a parent sheet");
    let d = handle
        .execute(Command::AddColumn {
            sheet,
            index: None,
            name: Some("D".to_string()),
            column: Column::new(ColumnType::Numeric),
        })
        .map(|_| {
            handle
                .snapshot()
                .expect("snapshot")
                .tree
                .node(sheet)
                .expect("sheet")
                .children()
                .last()
                .copied()
                .expect("D was just added")
        })
        .expect("column D added");

    handle
        .execute(set_formula(&handle, c, "D+1"))
        .expect("set C = D+1");
    handle
        .execute(set_formula(&handle, d, "C+1"))
        .expect("set D = C+1, which closes the cycle");

    let snapshot = handle.snapshot().expect("snapshot");
    for &column in &[c, d] {
        let node = snapshot.tree.node(column).expect("column");
        let col = node.as_column().expect("the node is a column");
        assert_eq!(
            col.column_type(),
            ColumnType::Mixed,
            "each references the other, so retypes"
        );
        for row in 0..col.len() {
            assert_eq!(col.cell(row), MixedCell::Text("---".to_string()));
        }
    }
}

/// A loop that goes through functions is a cycle like any other (#401): the
/// columns a function reads are dependencies (invariant 11).
#[test]
fn a_cycle_through_functions_degrades_without_hanging() {
    let (project, _a, b, c) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    handle
        .execute(set_formula(&handle, b, "sqrt(C)"))
        .expect("set B = sqrt(C)");
    handle
        .execute(set_formula(&handle, c, "abs(B)"))
        .expect("set C = abs(B), which closes the cycle");

    let snapshot = handle.snapshot().expect("snapshot");
    for &column in &[b, c] {
        let node = snapshot.tree.node(column).expect("column");
        let col = node.as_column().expect("the node is a column");
        for row in 0..col.len() {
            assert_eq!(col.cell(row), MixedCell::Text("---".to_string()));
        }
    }
}

/// Setting a formula on a column that already held values (user data, not yet
/// a calculated column) overwrites these values by the immediate recalculation
/// (`setting_a_formula_recalculates_immediately` above already checks this) —
/// but a single `undo` must then restore **both** the formula (removed)
/// **and** the original values, not only the formula: that was the bug
/// (`DataDelta::ColumnMeta::values_before`, `stokesforge-commands`).
#[test]
fn undoing_a_formula_set_on_a_column_with_prior_values_restores_them() {
    let (project, _a, _b, c) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    // C already holds user data before any formula.
    handle
        .execute(Command::SetCells {
            column: c,
            start: 0,
            values: ValueBlock::Float(vec![7.0, 8.0, 9.0]),
            expected_version: None,
        })
        .expect("initial values of C");
    let undo_label_before_formula = handle.snapshot().expect("snapshot").history.undo_label;

    handle
        .execute(set_formula(&handle, c, "A+B"))
        .expect("set C = A+B");
    let snapshot = handle.snapshot().expect("snapshot");
    assert_eq!(
        read_floats(&snapshot, c),
        vec![11.0, 22.0, 33.0],
        "recalculated immediately, overwriting the entered values"
    );

    handle.undo().expect("undo the setting of the formula");
    let snapshot = handle.snapshot().expect("snapshot");
    let meta = snapshot
        .tree
        .node(c)
        .expect("column C")
        .as_column()
        .expect("the node is a column")
        .meta();
    assert_eq!(meta.formula, None, "the formula must be removed");
    assert_eq!(
        read_floats(&snapshot, c),
        vec![7.0, 8.0, 9.0],
        "the values from before the formula must be restored, not stay at 11/22/33"
    );
    assert_eq!(
        snapshot.history.undo_label, undo_label_before_formula,
        "a single undo was enough"
    );
}

/// A `load` systematically recalculates the formula columns rather than
/// trusting the values read from disk (see the docs of
/// `schema::KindRecord::Column::formula`): a deliberately wrong value in the
/// file does not survive loading.
#[test]
fn load_recalculates_rather_than_trusting_the_saved_values() {
    let (mut project, a, b, c) = fixture();
    let formula = project
        .tree()
        .formula_from_letters(c, "A+B")
        .expect("formula entered");
    project
        .set_column_meta(
            c,
            ColumnMeta {
                formula: Some(formula),
                ..Default::default()
            },
        )
        .expect("formula set outside the session");
    // Deliberately wrong values, never recalculated by this direct path (the
    // actor is not involved here).
    project
        .set_values(c, 0, ValueBlock::Float(vec![-1.0, -1.0, -1.0]))
        .expect("starting values, wrong on purpose");

    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("formula.stf");
    stokesforge_persistence::save(&project, &path).expect("save");

    let empty = Project::new("Empty").expect("empty project");
    let session = Session::spawn(empty, None).expect("actor thread started");
    let handle = session.handle();
    handle.load(&path).expect("load succeeded");

    let snapshot = handle.snapshot().expect("snapshot");
    assert_eq!(read_floats(&snapshot, a), vec![1.0, 2.0, 3.0]);
    assert_eq!(read_floats(&snapshot, b), vec![10.0, 20.0, 30.0]);
    assert_eq!(
        read_floats(&snapshot, c),
        vec![11.0, 22.0, 33.0],
        "recalculated on load, not the -1.0 read from the file"
    );
}

/// A formula column that references a mixed column retypes automatically to
/// `Mixed` (calculation failure becomes possible), and a row that could not be
/// calculated shows `"---"` (`MixedCell::Text`) rather than an empty cell —
/// whether a real failure (non-numeric text in a referenced column) or a
/// simply missing operand: both are a result not calculated, not a cell that
/// has not been filled yet. Takes the task's example: A = [1,2,3,4,""]
/// (numeric, the 5th cell is out of length → empty), B = [1,1,"oops",1,1]
/// (mixed), C = A+B.
#[test]
fn mixed_reference_retypes_the_formula_column_and_shows_dashes_on_failure() {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    project
        .add_workbook(root, Some("Book1"))
        .expect("workbook created");
    let sheet = project
        .resolve_path("[Book1]Sheet1")
        .expect("sheet resolved");
    let _a = project
        .add_column(
            sheet,
            None,
            Some("A"),
            Column::from_f64(&[1.0, 2.0, 3.0, 4.0]),
        )
        .expect("column A");
    let _b = project
        .add_column(
            sheet,
            None,
            Some("B"),
            Column::from_mixed(&[
                MixedCell::Number(1.0),
                MixedCell::Number(1.0),
                MixedCell::Text("oops".to_string()),
                MixedCell::Number(1.0),
                MixedCell::Number(1.0),
            ]),
        )
        .expect("column B");
    // C is created numeric at the start: setting the formula must retype it.
    let c = project
        .add_column(sheet, None, Some("C"), Column::new(ColumnType::Numeric))
        .expect("column C");

    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();
    handle
        .execute(set_formula(&handle, c, "A+B"))
        .expect("set C = A+B");

    let snapshot = handle.snapshot().expect("snapshot");
    let node = snapshot.tree.node(c).expect("column C");
    let column = node.as_column().expect("the node is a column");
    assert_eq!(
        column.column_type(),
        ColumnType::Mixed,
        "B is mixed: C must retype automatically to Mixed"
    );

    // Row 0: 1+1 = 2 (Ok).
    assert_eq!(column.cell(0), MixedCell::Number(2.0));
    // Row 1: 2+1 = 3 (Ok).
    assert_eq!(column.cell(1), MixedCell::Number(3.0));
    // Row 2: 3+"oops" fails (non-numeric text): "---", not an empty
    // cell.
    assert_eq!(column.cell(2), MixedCell::Text("---".to_string()));
    // Row 3: 4+1 = 5 (Ok).
    assert_eq!(column.cell(3), MixedCell::Number(5.0));
    // Row 4: A out of bounds (length 4) so missing, B = 1: "---" too —
    // Missing no longer gets a mute cell, exactly like Failed (see the doc
    // comment of the test).
    assert_eq!(column.cell(4), MixedCell::Text("---".to_string()));
}

/// A formula column referencing `Numeric`/`DateTime` columns — where failure
/// (non-numeric text) is therefore impossible — still retypes to `Mixed` as
/// soon as it references at least one column: any row of any referenced
/// column can be empty, and only a `Mixed` column can carry the `"---"` that
/// this shows (`write_formula_column`). The values present stay ordinary
/// `MixedCell::Number`s, readable as `Cell::Float` by any caller — the retype
/// changes nothing for a fully calculated result.
#[test]
fn formula_referencing_only_numeric_columns_still_becomes_mixed() {
    let (project, _a, _b, c) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    handle
        .execute(set_formula(&handle, c, "A+B"))
        .expect("set C = A+B");

    let snapshot = handle.snapshot().expect("snapshot");
    let node = snapshot.tree.node(c).expect("column C");
    let column = node.as_column().expect("the node is a column");
    assert_eq!(
        column.column_type(),
        ColumnType::Mixed,
        "a reference to A/B is enough, even if both are Numeric"
    );
    assert_eq!(column.cell(0), MixedCell::Number(11.0));
    assert_eq!(column.cell(1), MixedCell::Number(22.0));
    assert_eq!(column.cell(2), MixedCell::Number(33.0));
}

/// A formula without any column reference (a pure constant) can never be
/// missing nor fail: it stays `Numeric`, no needless retype to `Mixed`.
#[test]
fn a_formula_with_no_column_reference_stays_numeric() {
    let (project, _a, _b, c) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    handle
        .execute(set_formula(&handle, c, "42"))
        .expect("set C = 42, a pure constant");

    let snapshot = handle.snapshot().expect("snapshot");
    let node = snapshot.tree.node(c).expect("column C");
    let column = node.as_column().expect("the node is a column");
    assert_eq!(column.column_type(), ColumnType::Numeric);
    assert_eq!(read_floats(&snapshot, c), vec![42.0, 42.0, 42.0]);
}

// -- column functions (#402) ---------------------------------------------------

/// A sheet `A` (role X), `B`, an empty `C` for the formula under test, then
/// `D` and `E` (empty, to hold bounds). Returns `[a, b, c, d, e]`.
fn spectrum(x: &[f64], y: &[f64]) -> (Project, [NodeId; 5]) {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    project
        .add_workbook(root, Some("Book1"))
        .expect("workbook created");
    let sheet = project
        .resolve_path("[Book1]Sheet1")
        .expect("sheet resolved");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(x))
        .expect("column A");
    project
        .set_column_meta(
            a,
            ColumnMeta {
                role: stokesforge_core::ColumnRole::X,
                ..Default::default()
            },
        )
        .expect("A is X");
    let b = project
        .add_column(sheet, None, Some("B"), Column::from_f64(y))
        .expect("column B");
    let c = project
        .add_column(sheet, None, Some("C"), Column::new(ColumnType::Numeric))
        .expect("column C");
    let nan = vec![f64::NAN; x.len()];
    let d = project
        .add_column(sheet, None, Some("D"), Column::from_f64(&nan))
        .expect("column D");
    let e = project
        .add_column(sheet, None, Some("E"), Column::from_f64(&nan))
        .expect("column E");
    (project, [a, b, c, d, e])
}

fn cell_numbers(snapshot: &Snapshot, column: NodeId) -> Vec<Option<f64>> {
    let column = snapshot
        .tree
        .node(column)
        .and_then(|node| node.as_column())
        .expect("column present");
    (0..column.len())
        .map(|row| match column.get(row) {
            Cell::Float(value) if !value.is_nan() => Some(value),
            _ => None,
        })
        .collect()
}

fn is_dashes(snapshot: &Snapshot, column: NodeId) -> bool {
    let column = snapshot
        .tree
        .node(column)
        .and_then(|node| node.as_column())
        .expect("column present");
    !column.is_empty()
        && (0..column.len()).all(|row| matches!(column.get(row), Cell::Text(Some(t)) if t == "---"))
}

/// Normalizing a spectrum at 340: `B / at(B, 340)` is 1 on the 340 row.
#[test]
fn normalizing_at_a_wavelength_gives_one_there() {
    let (project, [_a, _b, c, _d, _e]) = spectrum(&[339.0, 340.0, 341.0], &[1.0, 2.0, 4.0]);
    let session = Session::spawn(project, None).expect("actor started");
    let handle = session.handle();
    handle
        .execute(set_formula(&handle, c, "B / at(B, 340)"))
        .expect("formula");
    let snapshot = handle.snapshot().expect("snapshot");
    assert_eq!(
        cell_numbers(&snapshot, c),
        vec![Some(0.5), Some(1.0), Some(2.0)]
    );
}

/// `integral(B)` is the Integration tool's area on the same series, duplicate
/// X included (X 0, 1, 1, 3 gives 5; merging the duplicates would give 4.5).
#[test]
fn integral_equals_the_integration_tool() {
    use stokesforge_analysis::integrate::{IntegrateSpec, Under};
    use stokesforge_analysis::ops::Operation;
    use stokesforge_core::{compute, SeriesRef};

    let (project, [a, b, c, _d, _e]) = spectrum(&[0.0, 1.0, 1.0, 3.0], &[0.0, 2.0, 4.0, 0.0]);
    let session = Session::spawn(project, None).expect("actor started");
    let handle = session.handle();
    handle
        .execute(set_formula(&handle, c, "integral(B)"))
        .expect("formula");
    let snapshot = handle.snapshot().expect("snapshot");
    let operation = Operation::Integrate {
        spec: IntegrateSpec {
            ranges: vec![],
            under: Under::Zero,
            cumulative: false,
            normalize: false,
        },
    };
    let series = [SeriesRef {
        x: Some(a),
        y: b,
        sigma: None,
    }];
    let computed = compute(&snapshot.tree, &operation, &series, None).expect("tool");
    let tool = computed.results[0]
        .values
        .iter()
        .find(|v| v.key == "area.total")
        .expect("total")
        .value;
    assert_eq!(tool, 5.0);
    assert_eq!(cell_numbers(&snapshot, c), vec![Some(tool); 4]);
}

/// `at(B, x, D)` reads the X in `D`, whatever is paired with `B`.
#[test]
fn at_with_a_third_argument_reads_that_column() {
    let (project, [_a, _b, c, d, _e]) = spectrum(&[1.0, 2.0, 3.0], &[10.0, 20.0, 30.0]);
    let session = Session::spawn(project, None).expect("actor started");
    let handle = session.handle();
    // D = 0, 10, 20: at x = 15 B is between rows 1 and 2.
    handle
        .execute(Command::SetCells {
            column: d,
            start: 0,
            values: ValueBlock::Float(vec![0.0, 10.0, 20.0]),
            expected_version: None,
        })
        .expect("fill D");
    handle
        .execute(set_formula(&handle, c, "at(B, 15, D)"))
        .expect("formula");
    let snapshot = handle.snapshot().expect("snapshot");
    assert_eq!(cell_numbers(&snapshot, c), vec![Some(25.0); 3]);
}

/// A column without a paired X: the formula is kept, the cells show `---`, and
/// it computes as soon as an X appears (the role of A set to X).
#[test]
fn a_column_without_x_shows_dashes_until_one_appears() {
    let (project, _a, _b, c) = fixture();
    let session = Session::spawn(project, None).expect("actor started");
    let handle = session.handle();
    handle
        .execute(set_formula(&handle, c, "at(B, 2)"))
        .expect("formula entered");
    let snapshot = handle.snapshot().expect("snapshot");
    assert!(is_dashes(&snapshot, c), "no X to the left of B");
    assert!(snapshot.tree.formula_lacks_paired_x(c));
    assert_eq!(
        snapshot.tree.formula_letters(c).as_deref(),
        Some("at(B, 2)"),
        "the formula is kept"
    );
    // A becomes an X: B is paired with it.
    let a = snapshot
        .tree
        .children(snapshot.tree.parent(c).unwrap().unwrap())
        .unwrap()[0];
    handle
        .execute(Command::SetColumnMeta {
            column: a,
            meta: ColumnMeta {
                role: stokesforge_core::ColumnRole::X,
                ..Default::default()
            },
        })
        .expect("A is X");
    let snapshot = handle.snapshot().expect("snapshot");
    assert!(!snapshot.tree.formula_lacks_paired_x(c));
    assert_eq!(cell_numbers(&snapshot, c), vec![Some(20.0); 3]);
}

/// Bounds read from columns, row by row: `max(B, D, E)` is the max of B for X
/// between D and E of that row.
#[test]
fn bounds_in_columns_are_read_row_by_row() {
    let (project, [_a, _b, c, d, e]) = spectrum(&[1.0, 2.0, 3.0, 4.0], &[5.0, 9.0, 7.0, 6.0]);
    let session = Session::spawn(project, None).expect("actor started");
    let handle = session.handle();
    for (column, values) in [(d, vec![1.0, 3.0, 3.0, 8.0]), (e, vec![2.0, 4.0, 1.0, 9.0])] {
        handle
            .execute(Command::SetCells {
                column,
                start: 0,
                values: ValueBlock::Float(values),
                expected_version: None,
            })
            .expect("bounds");
    }
    handle
        .execute(set_formula(&handle, c, "max(B, D, E)"))
        .expect("formula");
    let snapshot = handle.snapshot().expect("snapshot");
    // [1,2] -> 9; [3,4] -> 7; [3,1] reversed -> 9; [8,9] no point -> empty.
    assert_eq!(
        cell_numbers(&snapshot, c),
        vec![Some(9.0), Some(7.0), Some(9.0), None]
    );
}
