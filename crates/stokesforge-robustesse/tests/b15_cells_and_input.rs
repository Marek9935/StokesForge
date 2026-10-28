//! Battery 15 — the cell ↔ text translation, in both directions.
//!
//! This is the only boundary in the repository where a model value becomes a
//! string meant for a human, then becomes a value again. Three ways to get it
//! wrong, one per test of this battery:
//!
//! - **losing a distinction** that the model holds and that the format
//!   promises (present empty string vs absent value);
//! - **writing when nothing was typed** (passing through a cell is a command);
//! - **making unreadable** what the software exists to show (a physical
//!   measurement of the order of 10⁻¹⁹).
//!
//! `AppState` does not depend on egui (see `stokesforge-app-state/tests/shell.rs`):
//! everything below runs without a graphical server.

use stokesforge_app_state::state::AppState;
use stokesforge_core::{Cell, Column, ColumnType, ColumnValues, NodeId, Project};
use stokesforge_session::{Session, SessionHandle};

/// `[Book1]Sheet1` with a text column `T` = ["a", "", absent] and a numeric
/// column `N` holding three physicist's values.
fn test_bench() -> (Session, SessionHandle, NodeId, NodeId) {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("worksheet");
    let text = project
        .add_column(
            sheet,
            None,
            Some("T"),
            Column::from_text(&[Some("a".to_string()), Some(String::new()), None]),
        )
        .expect("text column");
    let numbers = project
        .add_column(
            sheet,
            None,
            Some("N"),
            // electron charge, Avogadro's number, an ordinary float
            Column::from_f64(&[1.602_176_634e-19, 6.022_140_76e23, 0.1 + 0.2]),
        )
        .expect("numeric column");
    let session = Session::spawn(project, None).expect("actor started");
    let handle = session.handle();
    (session, handle, text, numbers)
}

fn cell(state: &AppState, column: NodeId, row: usize) -> Cell {
    state
        .tree()
        .node(column)
        .and_then(stokesforge_core::Node::as_column)
        .expect("column")
        .get(row)
}

fn version(state: &AppState, column: NodeId) -> u64 {
    state
        .tree()
        .node(column)
        .and_then(stokesforge_core::Node::as_column)
        .expect("column")
        .version()
}

// -- guards ---------------------------------------------------------------

#[test]
fn b15_a_numeric_entry_makes_the_round_trip() {
    let (_session, handle, _text, numbers) = test_bench();
    let mut state = AppState::new(handle, "x.stf".to_string()).expect("state");
    for row in 0..3 {
        let displayed = state.cell_text(numbers, row);
        let expected = cell(&state, numbers, row);
        state.begin_edit(numbers, row);
        state.commit_edit();
        assert_eq!(
            cell(&state, numbers, row),
            expected,
            "row {row} (\"{displayed}\") must come back bit for bit"
        );
    }
}

#[test]
fn b15_a_cleared_numeric_cell_becomes_missing_not_zero() {
    let (_session, handle, _text, numbers) = test_bench();
    let mut state = AppState::new(handle, "x.stf".to_string()).expect("state");
    state.begin_edit(numbers, 0);
    if let Some(edit) = state.editing_mut() {
        edit.buffer.clear();
    }
    state.commit_edit();
    assert_eq!(cell(&state, numbers, 0), Cell::Float(f64::NAN));
}

/// A text that is not a number, typed into a numeric column, is no longer
/// lost: it widens the column to [`ColumnType::Mixed`] and becomes a text
/// cell, the neighbors keeping their numbers. **What must hold here is that no
/// data disappears** — neither the entry (which was rejected before mixed-type
/// columns), nor the values already present (which a conversion of the whole
/// column to text would have made untraceable).
#[test]
fn b15_a_text_that_is_not_a_number_widens_the_column_without_losing_anything() {
    let (_session, handle, _text, numbers) = test_bench();
    let mut state = AppState::new(handle, "x.stf".to_string()).expect("state");
    let neighbor = cell(&state, numbers, 1);
    state.begin_edit(numbers, 0);
    if let Some(edit) = state.editing_mut() {
        edit.buffer = "not a number".to_string();
    }
    state.commit_edit();

    assert!(
        state.status().is_none_or(|s| !s.is_error()),
        "this is no longer an error"
    );
    assert_eq!(
        state
            .tree()
            .node(numbers)
            .and_then(stokesforge_core::Node::as_column)
            .expect("column")
            .column_type(),
        ColumnType::Mixed
    );
    assert_eq!(state.cell_text(numbers, 0), "not a number");
    assert_eq!(
        cell(&state, numbers, 1),
        neighbor,
        "the neighboring row keeps its number, bit for bit"
    );

    // And the display round trip still holds on the numeric rows of a column
    // that has become mixed: retyping what is displayed writes nothing that
    // changes the value.
    let expected = cell(&state, numbers, 1);
    state.begin_edit(numbers, 1);
    state.commit_edit();
    assert_eq!(cell(&state, numbers, 1), expected);
}

// -- reproductions --------------------------------------------------------

#[test]
fn b15_an_empty_string_survives_passing_through_its_cell() {
    let (_session, handle, text, _numbers) = test_bench();
    let mut state = AppState::new(handle, "x.stf".to_string()).expect("state");

    assert_eq!(cell(&state, text, 1), Cell::Text(Some(String::new())));
    assert_eq!(cell(&state, text, 2), Cell::Text(None));
    // A9 (option 2, already fixed): the present empty string and the absent
    // value now display differently — see
    // `b15_a9_option2_the_grid_tells_absent_from_empty` for the reproduction
    // dedicated to that half of the anomaly. This test only covers the other
    // half (option 1): the survival of `Some("")` when passing through a cell.
    assert_eq!(state.cell_text(text, 1), "");
    assert_ne!(state.cell_text(text, 2), "");

    // The user clicks in cell 1 and clicks elsewhere — or moves on with the
    // keyboard: `cell_widget` commits on **any** loss of focus except Esc.
    state.begin_edit(text, 1);
    state.commit_edit();

    println!("after passing through: {:?}", cell(&state, text, 1));
    assert_eq!(
        cell(&state, text, 1),
        Cell::Text(Some(String::new())),
        "the present empty string became an absent value when passing through its \
         cell: the model tells them apart (`column.rs`), the format has promised it \
         since version 1 (criterion 3 of `stokesforge-persistence`) — fixed in \
         `commit_cell` (A9 option 1): comparing the committed text with the text \
         already displayed before dispatching avoids writing anything when nothing \
         was typed"
    );
}

/// A9 (option 2) — the other half of the anomaly: even without passing
/// through, nothing on screen told a present empty string from an absent value
/// (`cell_text` returned `""` for both). Fixed in `state.rs::cell_text`: an
/// absent value now displays as a non-breaking space
/// ([`stokesforge_app_state::state::MISSING_TEXT_DISPLAY`]) rather than as an
/// empty string.
#[test]
fn b15_a9_option2_the_grid_tells_absent_from_empty() {
    use stokesforge_app_state::state::MISSING_TEXT_DISPLAY;

    let (_session, handle, text, _numbers) = test_bench();
    let state = AppState::new(handle, "x.stf".to_string()).expect("state");

    assert_eq!(cell(&state, text, 1), Cell::Text(Some(String::new())));
    assert_eq!(cell(&state, text, 2), Cell::Text(None));

    let present_empty = state.cell_text(text, 1);
    let absent = state.cell_text(text, 2);
    assert_eq!(
        present_empty, "",
        "a present empty string stays displayed as an empty string"
    );
    assert_eq!(
        absent, MISSING_TEXT_DISPLAY,
        "an absent value displays according to the convention chosen by `cell_text`"
    );
    assert_ne!(
        present_empty, absent,
        "\"{present_empty:?}\" (present) and \"{absent:?}\" (absent) must display \
         differently, otherwise nothing tells them apart on screen"
    );
}

#[test]
fn b15_passing_through_a_cell_without_typing_writes_nothing() {
    let (_session, handle, _text, numbers) = test_bench();
    let mut state = AppState::new(handle, "x.stf".to_string()).expect("state");
    let revision = state.snapshot().revision;
    let counter = version(&state, numbers);

    // Three tabs through a column, without a keystroke.
    for row in 0..3 {
        state.begin_edit(numbers, row);
        state.commit_edit();
    }

    println!(
        "revision {revision} → {} ; column version {counter} → {} ; undo = {:?}",
        state.snapshot().revision,
        version(&state, numbers),
        state.undo_label()
    );
    assert_eq!(
        (state.snapshot().revision, version(&state, numbers)),
        (revision, counter),
        "walking along a row with the keyboard produced three `SetCells` commands, \
         three undo steps \"Edit Cells in Column N\" and three increments of the \
         counter that tells readers the column has changed — while not one value \
         moved"
    );
}

#[test]
fn b15_a_scientific_measurement_fits_in_a_cell() {
    let (_session, handle, _text, numbers) = test_bench();
    let state = AppState::new(handle, "x.stf".to_string()).expect("state");

    // A grid column is 110 px wide: about fifteen characters. 24 is already
    // wide.
    const READABLE: usize = 24;
    let mut offenders = Vec::new();
    for (row, value) in [1.602_176_634e-19, 6.022_140_76e23, 0.1 + 0.2]
        .into_iter()
        .enumerate()
    {
        let displayed = state.cell_text(numbers, row);
        let scientific = format!("{value:e}");
        println!(
            "{:>22} → {:>3} characters; in scientific notation: {scientific} ({} characters)",
            scientific,
            displayed.len(),
            scientific.len()
        );
        if displayed.len() > READABLE {
            offenders.push((displayed.len(), displayed));
        }
    }
    assert!(
        offenders.is_empty(),
        "{offenders:?}: `cell_text` formats with the `Display` of `f64`, which never \
         emits an exponent — `f64::MIN_POSITIVE` would be written on 326 characters. \
         This is the kind of value this software exists for"
    );
}

/// C4 — no longer a reproduction: a **documented architecture choice**, not a
/// bug. There is no longer a homogeneous `Text` type to convert a numeric
/// column to (such a conversion would have reformatted every value as a
/// string, via `format_f64_for_display` — a cost that varies with the
/// content). `Numeric → Mixed` is the conversion that takes its place today,
/// and it is **lossless**: every `f64` becomes a `MixedCell::Number`, never a
/// string. `size_of::<MixedCell>()` is 24 bytes on a 64-bit machine (same
/// footprint as an `Option<String>`, see the top-level doc of
/// `stokesforge-core::column`), against 8 for the replaced `f64`: the factor is
/// therefore **exactly** 3.0, a fixed number independent of the content or of
/// the size of the column — unlike the old `Numeric → Text`, there is no longer
/// any text content whose size could make this factor drift.
#[test]
fn b15_converting_to_mixed_multiplies_memory_by_a_fixed_factor() {
    use stokesforge_commands::{Command, History};

    fn factor(rows: usize) -> f64 {
        let mut project = Project::new("Demo").expect("valid project");
        let root = project.root();
        project.add_workbook(root, Some("Book1")).expect("workbook");
        let sheet = project.resolve_path("[Book1]Sheet1").expect("worksheet");
        // A measured current, in amperes: the order of magnitude of the domain.
        let measurements: Vec<f64> = (0..rows)
            .map(|i| 1.602_176_634e-19 * (1.0 + i as f64 * 1e-9))
            .collect();
        let column = project
            .add_column(sheet, None, Some("I"), Column::from_f64(&measurements))
            .expect("column");

        let before = rows * std::mem::size_of::<f64>();
        let mut history = History::new();
        history
            .execute(
                &mut project,
                Command::SetColumnType {
                    column,
                    column_type: ColumnType::Mixed,
                },
            )
            .expect("conversion accepted");

        let after: usize = match project
            .tree()
            .node(column)
            .and_then(stokesforge_core::Node::as_column)
            .expect("column")
            .values()
        {
            ColumnValues::Mixed(values) => {
                values.len() * std::mem::size_of::<stokesforge_core::MixedCell>()
            }
            // This test measures the `Numeric -> Mixed` conversion only.
            ColumnValues::Float(_) => 0,
        };
        after as f64 / before as f64
    }

    let small = factor(5_000);
    let large = factor(50_000);
    println!("5,000 rows: {small:.2}× ; 50,000 rows: {large:.2}×");
    for (rows, f) in [(5_000, small), (50_000, large)] {
        assert!(
            (f - 3.0).abs() < 1e-9,
            "{rows} rows: factor {f:.2}×, expected exactly 3.0 — `Numeric → \
             Mixed` reformats nothing, every cell stays an `f64` carried by a \
             `MixedCell::Number`"
        );
    }
}
