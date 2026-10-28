//! Battery 12 — commands that succeed without doing what they say.
//!
//! Two opposite patterns, tested here at the level of the command bus:
//!
//! - a command that is **accepted but changes nothing** and still takes an
//!   undo step — the user sees a success, the history an entry, and the model
//!   nothing at all;
//! - a command that is **accepted but changes more than it writes** — a block
//!   of zero values that lengthens the column anyway.
//!
//! Neither is a crash; both lie to the user.

use stokesforge_commands::{Command, History};
use stokesforge_core::{Column, NodeId, Project, ValueBlock};

/// Empty `[Book1]Sheet1`, ready to receive columns.
fn empty_sheet() -> (Project, NodeId) {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("worksheet");
    (project, sheet)
}

fn length(project: &Project, column: NodeId) -> usize {
    project
        .tree()
        .node(column)
        .and_then(stokesforge_core::Node::as_column)
        .map_or(0, Column::len)
}

fn values(project: &Project, column: NodeId) -> Vec<f64> {
    project
        .tree()
        .node(column)
        .and_then(stokesforge_core::Node::as_column)
        .and_then(Column::float_values)
        .expect("numeric column")
        .as_slice()
        .to_vec()
}

// -- guards ---------------------------------------------------------------

#[test]
fn b12_insert_rows_lengthens_the_columns_that_own_the_row() {
    let (mut project, sheet) = empty_sheet();
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("column A");
    let mut history = History::with_limit(0);
    history
        .execute(
            &mut project,
            Command::InsertRows {
                sheet,
                at: 1,
                count: 2,
            },
        )
        .expect("insertion accepted");
    assert_eq!(length(&project, a), 5);
    assert!(values(&project, a)[1].is_nan());
    assert!(history.undo(&mut project).expect("undo"));
    assert_eq!(values(&project, a), vec![1.0, 2.0, 3.0]);
}

#[test]
fn b12_delete_rows_stays_undoable_on_columns_of_different_lengths() {
    let (mut project, sheet) = empty_sheet();
    let a = project
        .add_column(
            sheet,
            None,
            Some("A"),
            Column::from_f64(&[1.0, 2.0, 3.0, 4.0, 5.0]),
        )
        .expect("column A");
    let b = project
        .add_column(
            sheet,
            None,
            Some("B"),
            Column::from_f64(&[10.0, 20.0, 30.0]),
        )
        .expect("column B");
    let mut history = History::with_limit(0);
    // `count` overruns the end of B: the deletion is clamped, without error.
    history
        .execute(
            &mut project,
            Command::DeleteRows {
                sheet,
                at: 2,
                count: 10,
            },
        )
        .expect("deletion accepted");
    assert_eq!(length(&project, a), 2);
    assert_eq!(length(&project, b), 2);
    assert!(history.undo(&mut project).expect("undo"));
    assert_eq!(values(&project, a), vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    assert_eq!(values(&project, b), vec![10.0, 20.0, 30.0]);
}

#[test]
fn b12_setcells_does_write_well_past_the_end() {
    let (mut project, sheet) = empty_sheet();
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0]))
        .expect("column A");
    let mut history = History::with_limit(0);
    history
        .execute(
            &mut project,
            Command::SetCells {
                column: a,
                start: 3,
                values: ValueBlock::Float(vec![4.0]),
                expected_version: None,
            },
        )
        .expect("write accepted");
    assert_eq!(length(&project, a), 4);
    assert_eq!(values(&project, a)[3], 4.0);
    assert!(history.undo(&mut project).expect("undo"));
    assert_eq!(values(&project, a), vec![1.0]);
}

// -- reproductions --------------------------------------------------------

#[test]
fn b12_a_command_with_no_effect_does_not_take_an_undo_step() {
    // B5 (`guide-de-correction-robustesse.md` §F5) has two halves. This one
    // covers the empty target: a worksheet without columns, where
    // `InsertRows` cannot, by construction, touch anything. The former
    // scenario here — a worksheet with a single new, hence empty, column at
    // `at: 0` — actually tested the OTHER half of B5 (`row_targets` refused
    // `at == len`): with that half fixed (see `Project::insert_rows`) and
    // `stokesforge-app/src/app.rs` (the grid has since moved to
    // `src/app/grid.rs`) no longer hard-coding `at: 0`, this
    // command now has a real effect — a row is added — and DESERVES its
    // history entry; it is therefore no longer the right scenario for this
    // test.
    let (mut project, sheet) = empty_sheet();
    let mut history = History::with_limit(0);

    let result = history.execute(
        &mut project,
        Command::InsertRows {
            sheet,
            at: 0,
            count: 1,
        },
    );
    println!("InsertRows(at=0, count=1) on a worksheet without columns → {result:?}");
    println!(
        "worksheet rows = {}, history entries = {} ({:?})",
        project.tree().row_count(sheet).unwrap_or(0),
        history.len(),
        history.undo_label()
    );
    assert!(
        result.is_err() || history.is_empty(),
        "the command succeeded, changed nothing (no column to touch), and still \
         pushed \"{:?}\": the user reads a success, the Undo menu offers an action \
         that will undo nothing, and a 100-step history loses a real one",
        history.undo_label()
    );
}

#[test]
fn b12_a_row_can_be_added_at_the_end_of_a_worksheet() {
    let (mut project, sheet) = empty_sheet();
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .expect("column A");
    let mut history = History::with_limit(0);
    let rows = project.tree().row_count(sheet).unwrap_or(0);

    let result = history.execute(
        &mut project,
        Command::InsertRows {
            sheet,
            at: rows,
            count: 1,
        },
    );
    println!("InsertRows(at={rows}, count=1) → {result:?}");
    println!("length of A after = {}", length(&project, a));
    assert_eq!(
        project.tree().row_count(sheet).unwrap_or(0),
        rows + 1,
        "inserting at position `len` is the way to add a row at the end of a \
         worksheet (this is what `Column::insert_rows` accepts, `at ∈ 0..=len`); at \
         worksheet level the `at < len` filter of `row_targets` silently refuses it, \
         and no other command does"
    );
}

#[test]
fn b12_setcells_of_an_empty_block_changes_nothing() {
    let (mut project, sheet) = empty_sheet();
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("column A");
    let mut history = History::with_limit(0);
    let before = values(&project, a);

    history
        .execute(
            &mut project,
            Command::SetCells {
                column: a,
                start: 10,
                values: ValueBlock::Float(Vec::new()),
                expected_version: None,
            },
        )
        .expect("write accepted");

    let after = values(&project, a);
    println!("before: {before:?}");
    println!("after SetCells(start = 10, 0 values): {after:?}");
    assert_eq!(
        after, before,
        "a command that writes no value must not change the data; yet \
         `write_block` does `resize_with(start, missing)` before discovering \
         that there is nothing to write"
    );
}
