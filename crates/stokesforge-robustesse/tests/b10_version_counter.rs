//! Battery 10 — the contract of a column's version counter.
//!
//! `Column::version` is the only mechanism in the foundation that answers "has
//! this column changed since I last read it?" without rereading the values.
//! Its documentation sets the meaning of the tolerated approximation: "a false
//! *changed* costs a useless recalculation, a false *unchanged* would show a
//! wrong result. Only the first error is tolerable."
//!
//! This battery tests exactly that sentence, at the level of the command bus
//! rather than at the level of the column (the unit tests of `column.rs`
//! already cover the column alone, and they pass).

use stokesforge_commands::{Command, History};
use stokesforge_core::{Column, ColumnMeta, ColumnType, MixedCell, NodeId, Project, ValueBlock};

/// `[Book1]Sheet1` with `A` = [1, 2, 3] and `B` = [10].
fn project_with_two_columns() -> (Project, NodeId, NodeId, NodeId) {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("worksheet");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("column A");
    let b = project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[10.0]))
        .expect("column B");
    (project, sheet, a, b)
}

fn version(project: &Project, column: NodeId) -> u64 {
    project
        .tree()
        .node(column)
        .and_then(stokesforge_core::Node::as_column)
        .expect("column")
        .version()
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

fn write(history: &mut History, project: &mut Project, column: NodeId, value: f64) {
    history
        .execute(
            project,
            Command::SetCells {
                column,
                start: 0,
                values: ValueBlock::Float(vec![value]),
                expected_version: None,
            },
        )
        .expect("write accepted");
}

// -- guards ---------------------------------------------------------------

#[test]
fn b10_the_counter_advances_on_every_write() {
    let (mut project, _sheet, a, _b) = project_with_two_columns();
    let mut history = History::with_limit(0);
    let start = version(&project, a);
    write(&mut history, &mut project, a, 9.0);
    let after = version(&project, a);
    assert!(
        after > start,
        "the counter must advance: {start} then {after}"
    );
}

#[test]
fn b10_an_untouched_column_keeps_its_counter() {
    let (mut project, _sheet, a, b) = project_with_two_columns();
    let mut history = History::with_limit(0);
    let before = version(&project, b);
    write(&mut history, &mut project, a, 9.0);
    assert_eq!(
        version(&project, b),
        before,
        "writing to A must not move B's counter"
    );
}

#[test]
fn b10_metadata_is_not_data() {
    let (mut project, _sheet, a, _b) = project_with_two_columns();
    let before = version(&project, a);
    project
        .set_column_meta(
            a,
            ColumnMeta {
                unit: "ms".to_string(),
                ..ColumnMeta::default()
            },
        )
        .expect("metadata accepted");
    assert_eq!(version(&project, a), before, "a unit is not data");
}

#[test]
fn b10_a_rejected_command_increments_nothing() {
    let (mut project, _sheet, a, _b) = project_with_two_columns();
    let mut history = History::with_limit(0);
    let before = version(&project, a);
    // Mixed block on a numeric column: `Err(TypeMismatch)`.
    let rejection = history.execute(
        &mut project,
        Command::SetCells {
            column: a,
            start: 0,
            values: ValueBlock::Mixed(vec![MixedCell::Text("x".to_string())]),
            expected_version: None,
        },
    );
    assert!(rejection.is_err(), "the command must be rejected");
    assert_eq!(version(&project, a), before);
}

#[test]
fn b10_the_counter_survives_a_type_change() {
    let (mut project, _sheet, a, _b) = project_with_two_columns();
    let mut history = History::with_limit(0);
    let before = version(&project, a);
    history
        .execute(
            &mut project,
            Command::SetColumnType {
                column: a,
                column_type: ColumnType::Mixed,
            },
        )
        .expect("conversion accepted");
    assert!(version(&project, a) > before, "a conversion is a write");
}

#[test]
fn b10_a_tolerant_entry_that_promotes_a_column_is_a_write() {
    // `Command::SetCellsPromoting` on a numeric column that receives a text:
    // the column is promoted to `Mixed` (path 3, see `command.rs`) *and* the
    // cell is written in the same history entry. Both effects must advance the
    // counter, like any other write.
    let (mut project, _sheet, a, _b) = project_with_two_columns();
    let mut history = History::with_limit(0);
    let before = version(&project, a);
    history
        .execute(
            &mut project,
            Command::SetCellsPromoting {
                column: a,
                start: 0,
                values: ValueBlock::Mixed(vec![MixedCell::Text("n/a".to_string())]),
                expected_version: None,
            },
        )
        .expect("tolerant entry accepted");
    let after_promotion = version(&project, a);
    assert!(
        after_promotion > before,
        "promoting a column to Mixed and writing to it is a write: {before} then \
         {after_promotion}"
    );

    // A second tolerant entry, on a column that is already mixed (path 1/2,
    // no promotion): it must also advance the counter, and never go back to a
    // version already seen.
    history
        .execute(
            &mut project,
            Command::SetCellsPromoting {
                column: a,
                start: 1,
                values: ValueBlock::Mixed(vec![MixedCell::Text("hs".to_string())]),
                expected_version: None,
            },
        )
        .expect("second tolerant entry accepted");
    let after_second = version(&project, a);
    assert!(
        after_second > after_promotion,
        "a tolerant entry on an already mixed column must also advance the counter: \
         {after_promotion} then {after_second}"
    );
}

// -- reproductions --------------------------------------------------------

#[test]
fn b10_the_version_counter_never_goes_back() {
    let (mut project, _sheet, a, _b) = project_with_two_columns();
    let mut history = History::with_limit(0);

    write(&mut history, &mut project, a, 9.0);
    // A STRUCTURE command: it does not touch the data, but its history entry
    // keeps the whole tree, columns included, so counters included.
    history
        .execute(
            &mut project,
            Command::Rename {
                id: a,
                name: "A2".to_string(),
            },
        )
        .expect("rename accepted");
    write(&mut history, &mut project, a, 5.0);

    let top = version(&project, a);
    history.undo(&mut project).expect("data undo");
    let after_data = version(&project, a);
    history.undo(&mut project).expect("structure undo");
    let after_structure = version(&project, a);

    println!(
        "top = {top}, after undoing the data = {after_data}, \
         after undoing the structure = {after_structure}"
    );
    assert!(
        after_structure >= after_data,
        "the counter went from {after_data} to {after_structure}: it went back, \
         while its documentation makes it the monotonic witness of a modification"
    );
}

#[test]
fn b10_two_contents_never_share_a_version_number() {
    let (mut project, _sheet, a, _b) = project_with_two_columns();
    let mut history = History::with_limit(0);

    write(&mut history, &mut project, a, 9.0);
    history
        .execute(
            &mut project,
            Command::Rename {
                id: a,
                name: "A2".to_string(),
            },
        )
        .expect("rename accepted");

    // What a reader that keeps (version, content) would see — this is
    // literally the use described by the documentation of `Column::version`.
    write(&mut history, &mut project, a, 5.0);
    let kept_version = version(&project, a);
    let kept_content = values(&project, a);

    history.undo(&mut project).expect("data undo");
    history.undo(&mut project).expect("structure undo");

    // A fresh write (the redo is lost: `execute` empties the redo stack), with
    // ANOTHER value.
    write(&mut history, &mut project, a, 7.0);
    let current_version = version(&project, a);
    let current_content = values(&project, a);

    println!("kept    : version {kept_version} → {kept_content:?}");
    println!("current : version {current_version} → {current_content:?}");
    assert!(
        current_version != kept_version || current_content == kept_content,
        "version {kept_version} now designates {current_content:?} while a reader \
         kept it for {kept_content:?}: it will think it is up to date and show wrong \
         values — the only false negative that the documentation calls intolerable"
    );
}
