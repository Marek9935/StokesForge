//! Acceptance criteria of the command bus, exercised through the public API
//! only: we never inspect the inside of `History`, only what a
//! caller can observe (the project tree and the accessors of the stack).

use std::sync::Arc;

use stokesforge_commands::{
    ColumnRef, Command, CommandError, History, HistoryLabel, MacroKind, DEFAULT_BYTE_LIMIT,
    DEFAULT_LIMIT,
};
use stokesforge_core::{
    Cell, Column, ColumnMeta, ColumnRole, ColumnType, CoreError, MixedCell, Node, NodeId, NodeKind,
    Project, Tree, ValueBlock, WindowMeta,
};
use stokesforge_plot::{ColorUsage, PlotSpec, SavedColors};

// -- fixture --------------------------------------------------------------

/// Project "Demo": `Book1` with `Sheet1` (columns `A [1,2,3]`, `B [10,20]`,
/// text `T ["a", None, "c"]`) and `Sheet2` (column `X [0.5, 1.5]`).
///
/// The setup goes through the `Project` primitives and not through commands:
/// this is the only place where the golden rule ("no mutation outside a
/// command") tolerates an exception, and it is the one it provides for —
/// tests.
struct Fixture {
    project: Project,
    book: NodeId,
    sheet1: NodeId,
    sheet2: NodeId,
    a: NodeId,
    b: NodeId,
    t: NodeId,
    x: NodeId,
}

fn fixture() -> Fixture {
    let mut project = Project::new("Demo").expect("project Demo");
    let root = project.root();
    let book = project
        .add_workbook(root, Some("Book1"))
        .expect("workbook Book1");
    let sheet1 = project.resolve_path("[Book1]Sheet1").expect("sheet Sheet1");
    let a = project
        .add_column(sheet1, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("column A");
    let b = project
        .add_column(sheet1, None, Some("B"), Column::from_f64(&[10.0, 20.0]))
        .expect("column B");
    let t = project
        .add_column(
            sheet1,
            None,
            Some("T"),
            Column::from_text(&[Some("a".to_string()), None, Some("c".to_string())]),
        )
        .expect("column T");
    let sheet2 = project
        .add_worksheet(book, Some("Sheet2"))
        .expect("sheet Sheet2");
    let x = project
        .add_column(sheet2, None, Some("X"), Column::from_f64(&[0.5, 1.5]))
        .expect("column X");
    Fixture {
        project,
        book,
        sheet1,
        sheet2,
        a,
        b,
        t,
        x,
    }
}

// -- helper reads ---------------------------------------------------------

fn snapshot(project: &Project) -> Arc<Tree> {
    Arc::clone(project.tree())
}

fn floats(project: &Project, column: NodeId) -> Vec<f64> {
    project
        .tree()
        .node(column)
        .and_then(Node::as_column)
        .and_then(|c| c.float_values().cloned())
        .map(|values| values.as_slice().to_vec())
        .expect("numeric column")
}

fn texts(project: &Project, column: NodeId) -> Vec<Option<String>> {
    let tree = project.tree();
    let target = tree
        .node(column)
        .and_then(Node::as_column)
        .expect("existing column");
    (0..target.len())
        .map(|row| match target.get(row) {
            Cell::Text(text) => text,
            Cell::Float(_) => panic!("numeric column, text expected"),
        })
        .collect()
}

fn column_len(project: &Project, column: NodeId) -> usize {
    project
        .tree()
        .node(column)
        .and_then(Node::as_column)
        .map(Column::len)
        .expect("existing column")
}

fn column_type(project: &Project, column: NodeId) -> ColumnType {
    project
        .tree()
        .node(column)
        .and_then(Node::as_column)
        .map(Column::column_type)
        .expect("existing column")
}

fn names(project: &Project, parent: NodeId) -> Vec<String> {
    project
        .tree()
        .children(parent)
        .expect("existing parent")
        .iter()
        .filter_map(|id| project.tree().node(*id))
        .map(|node| node.name().to_string())
        .collect()
}

fn set_cells(column: NodeId, start: usize, values: &[f64]) -> Command {
    Command::SetCells {
        column,
        start,
        values: ValueBlock::Float(values.to_vec()),
        expected_version: None,
    }
}

// -- 1. bus: elementary round trip ----------------------------------------

#[test]
fn execute_undo_redo_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    assert_eq!(history.limit(), DEFAULT_LIMIT);
    assert!(history.is_empty());

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[0.0]))
        .expect("input accepted");

    assert_eq!(history.len(), 1);
    assert!(history.can_undo());
    assert!(!history.can_redo());
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::SetCells {
            column: Some(ColumnRef(0))
        })
    );
    assert_eq!(floats(&f.project, f.a), vec![0.0, 2.0, 3.0]);

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(floats(&f.project, f.a), vec![1.0, 2.0, 3.0]);
    assert!(history.can_redo());
    assert_eq!(
        history.redo_label(),
        Some(&HistoryLabel::SetCells {
            column: Some(ColumnRef(0))
        })
    );
    assert_eq!(history.undo_label(), None);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(floats(&f.project, f.a), vec![0.0, 2.0, 3.0]);

    // Stack emptied then queried: nothing to undo is not an error.
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(history.undo(&mut f.project), Ok(false));
}

// -- 2. apply before pushing ----------------------------------------------

#[test]
fn a_failed_command_changes_neither_tree_nor_stack() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    // Name already taken by column B of the same sheet.
    let err = history
        .execute(
            &mut f.project,
            Command::Rename {
                id: f.a,
                name: "B".to_string(),
            },
        )
        .expect_err("rename refused");
    assert!(matches!(
        err,
        CommandError::Core(CoreError::DuplicateName(_))
    ));
    assert_eq!(history.len(), 0);
    assert_eq!(**f.project.tree(), *before);

    let err = history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: Some(99),
                name: None,
                column: Column::from_f64(&[1.0]),
            },
        )
        .expect_err("index refused");
    assert!(matches!(
        err,
        CommandError::Core(CoreError::IndexOutOfBounds(_))
    ));
    assert_eq!(history.len(), 0);
    assert_eq!(**f.project.tree(), *before);

    let unknown = NodeId::from_raw(9_999);
    let err = history
        .execute(&mut f.project, set_cells(unknown, 0, &[1.0]))
        .expect_err("unknown node refused");
    assert_eq!(err, CommandError::Core(CoreError::UnknownNode(unknown)));
    assert_eq!(history.len(), 0);
    assert!(!history.can_undo());
    assert_eq!(**f.project.tree(), *before);
}

// -- 3. a new command cuts the redo branch --------------------------------

#[test]
fn a_new_command_drops_the_redo_stack() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[7.0]))
        .expect("input");
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(history.redo_len(), 1);

    history
        .execute(&mut f.project, set_cells(f.b, 0, &[8.0]))
        .expect("input");
    assert_eq!(history.redo_len(), 0);
    assert!(!history.can_redo());
}

// -- 4. clear -------------------------------------------------------------

#[test]
fn clear_empties_both_stacks_and_is_refused_in_a_macro() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[7.0]))
        .expect("input");
    history
        .execute(&mut f.project, set_cells(f.a, 1, &[8.0]))
        .expect("input");
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(history.len(), 1);
    assert_eq!(history.redo_len(), 1);

    history.clear().expect("purge accepted");
    assert_eq!(history.len(), 0);
    assert_eq!(history.redo_len(), 0);

    history.begin_macro(MacroKind::Import).unwrap();
    assert_eq!(history.clear(), Err(CommandError::MacroOpen("clear")));
    history
        .abort_macro(&mut f.project)
        .expect("empty macro abandoned");
}

// -- 5. SetCells ----------------------------------------------------------

#[test]
fn set_cells_beyond_the_end_restores_the_previous_length() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(&mut f.project, set_cells(f.a, 5, &[42.0]))
        .expect("input beyond the length");
    let after = snapshot(&f.project);
    assert_eq!(column_len(&f.project, f.a), 6);
    assert!(floats(&f.project, f.a)[3].is_nan());

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(column_len(&f.project, f.a), 3);
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn set_cells_on_a_mixed_column() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::SetCells {
                column: f.t,
                start: 1,
                values: ValueBlock::Mixed(vec![MixedCell::Text("b".to_string())]),
                expected_version: None,
            },
        )
        .expect("text input");
    let after = snapshot(&f.project);
    assert_eq!(
        texts(&f.project, f.t),
        vec![
            Some("a".to_string()),
            Some("b".to_string()),
            Some("c".to_string())
        ]
    );

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn set_cells_with_the_wrong_block_variant_is_refused() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    let err = history
        .execute(
            &mut f.project,
            Command::SetCells {
                column: f.a,
                start: 0,
                values: ValueBlock::Mixed(vec![MixedCell::Text("x".to_string())]),
                expected_version: None,
            },
        )
        .expect_err("mixed block on a numeric column");
    assert!(matches!(
        err,
        CommandError::Core(CoreError::TypeMismatch(_))
    ));
    assert!(history.is_empty());
    assert_eq!(**f.project.tree(), *before);
}

// -- 6. AddColumn ---------------------------------------------------------

#[test]
fn add_column_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: None,
                column: Column::from_f64(&[9.0]),
            },
        )
        .expect("column addition");
    let after = snapshot(&f.project);
    // D1 (`guide-de-correction-robustesse.md` §F7) — the automatic name starts
    // from the NUMBER of columns already present (three: A, B, T), not from the
    // first free name spreadsheet-style: rebuilding the list of taken names to
    // find the smallest gap ("C" would stay free here) is precisely the
    // O(n³) cost that D1 removes. The first candidate tried is therefore
    // "D".
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T", "D"]);
    assert_eq!(history.undo_label(), Some(&HistoryLabel::AddColumn));

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn add_column_with_an_explicit_name_and_position() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: Some(0),
                name: Some("D".to_string()),
                column: Column::from_f64(&[4.0]),
            },
        )
        .expect("addition of column D");
    let after = snapshot(&f.project);
    assert_eq!(names(&f.project, f.sheet1), vec!["D", "A", "B", "T"]);

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

// -- 7. DeleteColumn ------------------------------------------------------

#[test]
fn delete_column_round_trip_restores_the_same_node_id() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(&mut f.project, Command::DeleteColumn { column: f.a })
        .expect("deletion of A");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::DeleteColumn {
            column: Some(ColumnRef(0))
        })
    );
    assert!(!f.project.tree().contains(f.a));

    assert_eq!(history.undo(&mut f.project), Ok(true));
    // The earlier tree kept the deleted node: it comes back under its
    // original identifier, with its values.
    assert!(f.project.tree().contains(f.a));
    assert_eq!(floats(&f.project, f.a), vec![1.0, 2.0, 3.0]);
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert!(!f.project.tree().contains(f.a));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn delete_column_refuses_a_node_that_is_not_a_column() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    let err = history
        .execute(&mut f.project, Command::DeleteColumn { column: f.sheet1 })
        .expect_err("a sheet is not a column");
    match err {
        CommandError::Core(CoreError::Structure(message)) => {
            assert!(
                message.contains("is not a column"),
                "unexpected message: {message}"
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert!(history.is_empty());
    assert_eq!(**f.project.tree(), *before);
}

// -- 8. Rename ------------------------------------------------------------

#[test]
fn rename_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::Rename {
                id: f.a,
                name: "Z".to_string(),
            },
        )
        .expect("rename");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::Rename {
            from: Some("A".to_string()),
            to: "Z".to_string()
        })
    );
    assert_eq!(names(&f.project, f.sheet1), vec!["Z", "B", "T"]);

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn renaming_to_the_current_name_is_not_an_entry() {
    // B5, the "Rename" case (`guide-de-correction-robustesse.md` §F5, §7.3 —
    // recorded without identifier or reproduction until now). "Rename M to
    // M" already succeeded and already wrote nothing in the tree, but
    // still pushed an entry "Rename M to M": an undo slot
    // that undoes nothing. `entry_has_no_effect` compares
    // the tree before to the one after for any structural entry, so
    // it does not need to know `Rename` by its name.
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::Rename {
                id: f.a,
                name: "A".to_string(),
            },
        )
        .expect("rename to the current name");
    assert!(history.is_empty());
    assert_eq!(history.undo_label(), None);
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
    assert_eq!(**f.project.tree(), *before);
}

#[test]
fn execute_recorded_says_whether_an_entry_was_stored() {
    // On Stop the actor undoes only what the command has just stored: the
    // boolean comes from `execute` itself, not from a difference of `len()`.
    let mut f = fixture();
    let mut history = History::new();
    let rename = |id, name: &str| Command::Rename {
        id,
        name: name.to_string(),
    };

    // No effect ("Rename A to A"): accepted, no entry.
    assert!(!history
        .execute_recorded(&mut f.project, rename(f.a, "A"))
        .expect("no effect"));
    assert!(history.is_empty());

    // With an effect: one entry stored.
    assert!(history
        .execute_recorded(&mut f.project, rename(f.a, "A2"))
        .expect("with an effect"));
    assert_eq!(history.len(), 1);

    // In an open macro, the entry is stored in the macro: true, without
    // `len()` necessarily moving.
    history.begin_macro(MacroKind::Import).unwrap();
    assert!(history
        .execute_recorded(&mut f.project, rename(f.a, "A3"))
        .expect("in the macro"));
    history.end_macro(&mut f.project).unwrap();
}

#[test]
fn execute_recorded_is_true_even_when_the_limit_evicts_an_older_entry() {
    // Trap: with a limit of 1, storing an entry evicts another and
    // `len()` does not change. Deducing the boolean from `len()` would say "nothing stored".
    let mut f = fixture();
    let mut history = History::with_limit(1);
    for name in ["A2", "A3"] {
        assert!(history
            .execute_recorded(
                &mut f.project,
                Command::Rename {
                    id: f.a,
                    name: name.to_string(),
                },
            )
            .expect("rename"));
        assert_eq!(history.len(), 1);
    }
}

#[test]
fn renaming_the_root_is_accepted() {
    let mut f = fixture();
    let mut history = History::new();
    let root = f.project.root();

    history
        .execute(
            &mut f.project,
            Command::Rename {
                id: root,
                name: "P".to_string(),
            },
        )
        .expect("rename of the root");
    assert_eq!(f.project.name(), "P");
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(f.project.name(), "Demo");
}

// -- 9. Move --------------------------------------------------------------

#[test]
fn move_round_trip_restores_the_original_index() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::Move {
                id: f.a,
                new_parent: f.sheet2,
                index: 0,
            },
        )
        .expect("move of A");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::Move {
            name: Some("A".to_string())
        })
    );
    assert_eq!(names(&f.project, f.sheet2), vec!["A", "X"]);
    assert_eq!(names(&f.project, f.sheet1), vec!["B", "T"]);

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn a_circular_move_is_refused() {
    let mut f = fixture();
    let root = f.project.root();
    // Local setup: two nested folders, to be able to try
    // moving an ancestor into its own descendant.
    let outer = f.project.add_folder(root, "Outer").expect("outer folder");
    let inner = f.project.add_folder(outer, "Inner").expect("inner folder");
    let mut history = History::new();
    let before = snapshot(&f.project);

    let err = history
        .execute(
            &mut f.project,
            Command::Move {
                id: outer,
                new_parent: inner,
                index: 0,
            },
        )
        .expect_err("circular move refused");
    assert!(matches!(err, CommandError::Core(CoreError::Structure(_))));
    assert!(history.is_empty());
    assert_eq!(**f.project.tree(), *before);
}

// -- 10. SetColumnType ----------------------------------------------------

#[test]
fn set_column_type_mixed_to_numeric_round_trip() {
    // Former `set_column_type_text_to_numeric_round_trip`: `f.t` is
    // built by `Column::from_text`, which now directly produces
    // a `Mixed` column (there is no longer a homogeneous `Text` type) — same
    // coverage, on `Mixed → Numeric`.
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::SetColumnType {
                column: f.t,
                column_type: ColumnType::Numeric,
            },
        )
        .expect("conversion of T");
    let after = snapshot(&f.project);
    // "column C", not "T": the label designates the column by its
    // **position** in the sheet, as the grid shows it, never by its
    // node name. This case is precisely the one that motivates
    // `column_ref` — the node is called `T` but it is the third column,
    // so "C" on screen, and "T" appears nowhere for the user.
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::SetColumnType {
            column: Some(ColumnRef(2))
        })
    );
    let converted = floats(&f.project, f.t);
    assert_eq!(converted.len(), 3);
    assert!(
        converted.iter().all(|value| value.is_nan()),
        "'a', nothing and 'c' do not parse: {converted:?}"
    );

    // The conversion is destructive: only the retained copy makes it possible
    // to get back the original text **exactly**.
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(column_type(&f.project, f.t), ColumnType::Mixed);
    assert_eq!(
        texts(&f.project, f.t),
        vec![Some("a".to_string()), None, Some("c".to_string())]
    );
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(column_type(&f.project, f.t), ColumnType::Numeric);
    assert_eq!(**f.project.tree(), *after);
}

// `set_column_type_numeric_to_text_round_trip` was removed: it targeted
// `ColumnType::Text`, which no longer exists. `Numeric → Mixed` is not its
// equivalent — the conversion is lossless (`MixedCell::Number`, not a
// rewording as text) — so the invariant it checked (the number
// is written in its display form) has no grip any more; there is nothing to
// retarget.

// -- 11. InsertRows -------------------------------------------------------

#[test]
fn insert_rows_touches_only_the_columns_that_have_that_row() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::InsertRows {
                sheet: f.sheet1,
                at: 2,
                count: 2,
            },
        )
        .expect("row insertion");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::InsertRows { count: 2 })
    );
    assert_eq!(column_len(&f.project, f.a), 5);
    // B5 (`guide-de-correction-robustesse.md` §F5) — B has 2 rows:
    // `at == len` (2 == 2) is now a target, not only `at <
    // len`: it is the only way to add a row at the end of a column, the
    // range that `Column::insert_rows` already accepts (`at ∈ 0..=len`).
    assert_eq!(column_len(&f.project, f.b), 4);
    assert_eq!(column_len(&f.project, f.t), 5);
    let values = floats(&f.project, f.a);
    assert!(values[2].is_nan() && values[3].is_nan());
    assert_eq!(values[4], 3.0);

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn an_insert_that_changes_nothing_is_not_an_entry() {
    // B5 (`guide-de-correction-robustesse.md` §F5) — an accepted command
    // that changes nothing takes no undo slot: the former
    // version of this test asserted it for the opposite reason ("the position
    // of the history cursor would depend on the state of the data"), but
    // that is precisely what misled the user — a success with no
    // effect, and an "Undo" that undoes nothing. `History::execute`
    // now recognizes this case (`entry_has_no_effect`) and pushes nothing.
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    // No column has a row 10: the command is valid and changes
    // nothing.
    history
        .execute(
            &mut f.project,
            Command::InsertRows {
                sheet: f.sheet1,
                at: 10,
                count: 1,
            },
        )
        .expect("insertion with no effect");
    assert!(history.is_empty());
    assert_eq!(history.undo_label(), None);
    assert_eq!(**f.project.tree(), *before);

    // Nothing to undo: `undo` returns `Ok(false)`, not a regression.
    assert_eq!(history.undo(&mut f.project), Ok(false));
    assert_eq!(**f.project.tree(), *before);
}

// -- 12. DeleteRows -------------------------------------------------------

#[test]
fn delete_rows_round_trip_even_when_a_column_is_emptied_to_its_end() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::DeleteRows {
                sheet: f.sheet1,
                at: 1,
                count: 5,
            },
        )
        .expect("row deletion");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::DeleteRows { count: 5 })
    );
    assert_eq!(floats(&f.project, f.a), vec![1.0]);
    assert_eq!(floats(&f.project, f.b), vec![10.0]);
    assert_eq!(texts(&f.project, f.t), vec![Some("a".to_string())]);

    // Afterwards, no column has a row 1 any more: `insert_rows` at
    // sheet level would not reach them. Undo must therefore work
    // column by column, with the list retained at the time of the deletion.
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

// -- 12b. DeleteCells -------------------------------------------------------

#[test]
fn delete_cells_removes_independently_per_column_ignoring_duplicates() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    // `f.a` twice: ignored (a single deletion per column). `f.b`
    // (length 2) can lose only a single row from `at: 1`,
    // even if `count: 5` asks for more — each column stops at
    // its own end, independently of the others. `f.t` is not listed:
    // intact.
    history
        .execute(
            &mut f.project,
            Command::DeleteCells {
                columns: vec![f.a, f.b, f.a],
                at: 1,
                count: 5,
            },
        )
        .expect("cell deletion");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::DeleteCells { count: 5 })
    );
    assert_eq!(floats(&f.project, f.a), vec![1.0]);
    assert_eq!(floats(&f.project, f.b), vec![10.0]);
    assert_eq!(
        texts(&f.project, f.t),
        vec![Some("a".to_string()), None, Some("c".to_string())]
    );

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn delete_cells_beyond_every_targeted_column_is_not_an_entry() {
    // Same policy as `an_insert_that_changes_nothing_is_not_an_entry`
    // (B5): `at` beyond the end of all the targeted columns changes
    // nothing, so takes no undo slot.
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::DeleteCells {
                columns: vec![f.a, f.b],
                at: 10,
                count: 1,
            },
        )
        .expect("deletion with no effect");
    assert!(history.is_empty());
    assert_eq!(history.undo_label(), None);
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.undo(&mut f.project), Ok(false));
    assert_eq!(**f.project.tree(), *before);
}

// -- 13-14. macros --------------------------------------------------------

#[test]
fn a_macro_is_a_single_entry() {
    let mut f = fixture();
    let mut history = History::new();

    history.begin_macro(MacroKind::Import).unwrap();
    assert!(history.in_macro());
    assert_eq!(history.macro_depth(), 1);
    for name in ["D", "E"] {
        history
            .execute(
                &mut f.project,
                Command::AddColumn {
                    sheet: f.sheet1,
                    index: None,
                    name: Some(name.to_string()),
                    column: Column::from_f64(&[0.0]),
                },
            )
            .expect("addition in the macro");
    }
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[42.0]))
        .expect("input in the macro");
    assert_eq!(
        history.len(),
        0,
        "nothing is pushed while the macro is open"
    );

    history.end_macro(&mut f.project).expect("macro closed");
    assert!(!history.in_macro());
    assert_eq!(history.len(), 1);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::Macro(MacroKind::Import))
    );

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
    assert_eq!(floats(&f.project, f.a)[0], 1.0);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T", "D", "E"]);
    assert_eq!(floats(&f.project, f.a)[0], 42.0);
}

#[test]
fn nested_macros_collapse_into_one_entry() {
    let mut f = fixture();
    let mut history = History::new();

    history.begin_macro(MacroKind::GraphProperties).unwrap();
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("D".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("addition of D");
    history.begin_macro(MacroKind::Import).unwrap();
    assert_eq!(history.macro_depth(), 2);
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("E".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("addition of E");
    history.end_macro(&mut f.project).expect("inner macro");
    assert_eq!(history.len(), 0);
    history.end_macro(&mut f.project).expect("outer macro");

    assert_eq!(history.len(), 1);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::Macro(MacroKind::GraphProperties))
    );
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
}

// -- 15. mixed macros: the test that makes the hybrid hold ----------------

#[test]
fn a_macro_mixing_structure_then_data_undoes_in_reverse_order() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(
            &mut f.project,
            Command::Rename {
                id: f.a,
                name: "Z".to_string(),
            },
        )
        .expect("rename");
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[7.0]))
        .expect("input");
    history.end_macro(&mut f.project).expect("macro closed");

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["Z", "B", "T"]);
    assert_eq!(floats(&f.project, f.a)[0], 7.0);
}

#[test]
fn a_macro_mixing_data_then_structure_undoes_in_reverse_order() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[7.0]))
        .expect("input");
    history
        .execute(
            &mut f.project,
            Command::Rename {
                id: f.a,
                name: "Z".to_string(),
            },
        )
        .expect("rename");
    history.end_macro(&mut f.project).expect("macro closed");

    // Why the order matters: the structural entry of the rename keeps a
    // snapshot of the tree taken **after** the input, so a tree where A is
    // already [7,2,3]. Restoring it first (reverse order) brings back this
    // intermediate state, and the data delta, replayed afterwards, puts [1,2,3] back.
    // The other way round — delta first, snapshot afterwards — the snapshot
    // would overwrite the restoration of the data and A would stay at [7,2,3]:
    // this is exactly the flaw that strict reverse order closes.
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(floats(&f.project, f.a), vec![1.0, 2.0, 3.0]);
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["Z", "B", "T"]);
    assert_eq!(floats(&f.project, f.a)[0], 7.0);
}

#[test]
fn a_macro_deleting_then_recreating_a_homonym_restores_the_original_node() {
    let mut f = fixture();
    let mut history = History::new();

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(&mut f.project, Command::DeleteColumn { column: f.a })
        .expect("deletion of A");
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: Some(0),
                name: Some("A".to_string()),
                column: Column::from_f64(&[100.0]),
            },
        )
        .expect("new homonymous column");
    history.end_macro(&mut f.project).expect("macro closed");

    let new_a = f
        .project
        .tree()
        .child_by_name(f.sheet1, "A")
        .expect("valid sheet")
        .expect("column A present");
    assert_ne!(new_a, f.a, "the new homonym is another node");

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert!(f.project.tree().contains(f.a), "the old node A came back");
    assert!(
        !f.project.tree().contains(new_a),
        "the new homonym disappeared"
    );
    assert_eq!(floats(&f.project, f.a), vec![1.0, 2.0, 3.0]);
}

// -- 16. abort_macro ------------------------------------------------------

#[test]
fn abort_macro_undoes_everything_it_applied() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("Z".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("addition outside the macro");

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("D".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("addition in the macro");
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[5.0]))
        .expect("input in the macro");
    history
        .abort_macro(&mut f.project)
        .expect("macro abandoned");

    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T", "Z"]);
    assert_eq!(floats(&f.project, f.a)[0], 1.0);
    assert_eq!(history.len(), 1);
    assert!(!history.can_redo());
    assert!(!history.in_macro());
}

#[test]
fn aborting_a_nested_macro_leaves_the_parent_alone() {
    let mut f = fixture();
    let mut history = History::new();

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("D".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("addition of D");
    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("E".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("addition of E");
    history
        .abort_macro(&mut f.project)
        .expect("inner macro abandoned");

    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T", "D"]);
    assert!(history.in_macro());
    assert_eq!(history.macro_depth(), 1);

    history.end_macro(&mut f.project).expect("parent macro");
    assert_eq!(history.len(), 1);
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
}

// -- 17. refusals tied to macros -----------------------------------------

#[test]
fn macro_api_refusals() {
    let mut f = fixture();
    let mut history = History::new();

    assert_eq!(
        history.end_macro(&mut f.project),
        Err(CommandError::NotInMacro)
    );
    assert_eq!(
        history.abort_macro(&mut f.project),
        Err(CommandError::NotInMacro)
    );

    history.begin_macro(MacroKind::Import).unwrap();
    assert_eq!(
        history.undo(&mut f.project),
        Err(CommandError::MacroOpen("undo"))
    );
    assert_eq!(
        history.redo(&mut f.project),
        Err(CommandError::MacroOpen("redo"))
    );
    assert_eq!(
        history.set_limit(10),
        Err(CommandError::MacroOpen("set_limit"))
    );
    history
        .end_macro(&mut f.project)
        .expect("empty macro closed");
}

#[test]
fn an_empty_macro_leaves_no_entry() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[1.5]))
        .expect("input");
    let entries = history.len();

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .end_macro(&mut f.project)
        .expect("empty macro closed");
    assert_eq!(history.len(), entries);
}

// -- 18. failure inside a macro ------------------------------------------

#[test]
fn a_failing_command_inside_a_macro_leaves_nothing_behind() {
    let mut f = fixture();
    let mut history = History::new();

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("D".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("addition of D");
    let d = f
        .project
        .tree()
        .child_by_name(f.sheet1, "D")
        .expect("valid sheet")
        .expect("column D");

    let err = history
        .execute(
            &mut f.project,
            Command::Rename {
                id: d,
                name: "A".to_string(),
            },
        )
        .expect_err("name already taken");
    assert!(matches!(
        err,
        CommandError::Core(CoreError::DuplicateName(_))
    ));

    // The macro stays open, with the only successful addition.
    assert!(history.in_macro());
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T", "D"]);

    history.end_macro(&mut f.project).expect("macro closed");
    assert_eq!(history.len(), 1);
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
}

// -- 19-20. limit and release --------------------------------------------

#[test]
fn the_oldest_entries_are_evicted_silently() {
    let mut f = fixture();
    let mut history = History::with_limit(3);

    for step in 1..=5 {
        history
            .execute(
                &mut f.project,
                set_cells(f.a, 0, &[100.0 + f64::from(step)]),
            )
            .expect("input");
    }
    assert_eq!(history.len(), 3);

    for _ in 0..3 {
        assert_eq!(history.undo(&mut f.project), Ok(true));
    }
    assert_eq!(history.undo(&mut f.project), Ok(false));
    // The first two commands are no longer undoable: we stop at
    // the state they left.
    assert_eq!(floats(&f.project, f.a)[0], 102.0);
}

#[test]
fn set_limit_requires_an_empty_stack() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[1.0]))
        .expect("input");
    assert_eq!(history.set_limit(5), Err(CommandError::StackNotEmpty));

    assert_eq!(history.undo(&mut f.project), Ok(true));
    // The redo stack counts too.
    assert_eq!(history.set_limit(5), Err(CommandError::StackNotEmpty));

    history.clear().expect("purge");
    history.set_limit(5).expect("limit accepted");
    assert_eq!(history.limit(), 5);
}

#[test]
fn a_zero_limit_means_unlimited() {
    let mut f = fixture();
    let mut history = History::with_limit(0);

    for step in 0..200 {
        history
            .execute(&mut f.project, set_cells(f.a, 0, &[f64::from(step)]))
            .expect("input");
    }
    assert_eq!(history.len(), 200);
}

#[test]
fn eviction_releases_the_nodes_only_the_history_retained() {
    let mut f = fixture();
    let mut history = History::with_limit(1);

    // Weak reference taken without keeping any test `Arc<Tree>`:
    // only the history will keep the old node after the deletion.
    let weak = Arc::downgrade(
        f.project
            .tree()
            .node_arc(f.a)
            .expect("node A present before deletion"),
    );

    history
        .execute(&mut f.project, Command::DeleteColumn { column: f.a })
        .expect("deletion of A");
    assert!(
        weak.upgrade().is_some(),
        "as long as the entry is in the stack, the old tree keeps A"
    );

    history
        .execute(&mut f.project, set_cells(f.b, 0, &[0.0]))
        .expect("input");
    assert_eq!(history.len(), 1, "the oldest entry was evicted");
    assert!(
        weak.upgrade().is_none(),
        "eviction releases the tree before, hence the deleted node"
    );
}

// -- 21. partial redo ----------------------------------------------------

#[test]
fn partial_redo_then_a_new_command_cuts_the_branch() {
    let mut f = fixture();
    let mut history = History::with_limit(0);

    for step in 1..=5 {
        history
            .execute(
                &mut f.project,
                set_cells(f.a, 0, &[100.0 + f64::from(step)]),
            )
            .expect("input");
    }
    for _ in 0..3 {
        assert_eq!(history.undo(&mut f.project), Ok(true));
    }
    assert_eq!(floats(&f.project, f.a)[0], 102.0);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(floats(&f.project, f.a)[0], 103.0);
    assert_eq!(history.len(), 3);
    assert_eq!(history.redo_len(), 2);

    history
        .execute(&mut f.project, set_cells(f.x, 0, &[9.0]))
        .expect("input on X");
    assert_eq!(history.redo_len(), 0);
    assert!(!history.can_redo());
}

// -- 22. abort_macro restores the redo from before the macro ---------------
//
// Decision: an aborted macro leaves no trace, including on the
// redo. On opening the outermost macro, the current redo
// stack is set aside; it comes back as it was if
// this macro is aborted (or closed empty), and is dropped only if it
// succeeds with at least one entry — as for any command.

#[test]
fn abort_macro_restores_the_redo_stack_that_predates_it() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[1.0]))
        .expect("first input");
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[2.0]))
        .expect("second input");
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert!(history.can_redo());
    let before_macro = snapshot(&f.project);

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[9.0]))
        .expect("input in the macro");
    history
        .abort_macro(&mut f.project)
        .expect("macro abandoned");

    // The macro made something redoable along the way
    // (each `execute` empties the redo, even in a macro): as soon as it is aborted, it is
    // the redo from before the macro that must have come back, not an empty redo.
    assert!(history.can_redo());
    assert_eq!(history.redo_len(), 1);
    assert_eq!(**f.project.tree(), *before_macro);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(floats(&f.project, f.a)[0], 2.0);
}

#[test]
fn a_successful_macro_drops_the_redo_stack_like_any_command() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[1.0]))
        .expect("first input");
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[2.0]))
        .expect("second input");
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert!(history.can_redo());

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[9.0]))
        .expect("input in the macro");
    history.end_macro(&mut f.project).expect("macro closed");

    // The macro succeeded with an entry: the redo is lost, as for
    // any command that succeeds.
    assert!(!history.can_redo());
}

#[test]
fn an_empty_macro_restores_the_redo_stack_on_close() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[1.0]))
        .expect("first input");
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[2.0]))
        .expect("second input");
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert!(history.can_redo());
    let len_before = history.len();

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .end_macro(&mut f.project)
        .expect("empty macro closed");

    // An empty macro changed nothing: the redo from before comes back.
    assert!(history.can_redo());
    assert_eq!(history.len(), len_before);
}

#[test]
fn aborting_the_outer_macro_of_a_nested_pair_restores_redo() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[1.0]))
        .expect("first input");
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[2.0]))
        .expect("second input");
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert!(history.can_redo());
    let before_macro = snapshot(&f.project);

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("D".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("addition of D");

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("E".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("addition of E");
    history
        .abort_macro(&mut f.project)
        .expect("inner macro abandoned");

    // The aborted inner macro does not touch what the outer one has already
    // applied, nor the redo backup (which belongs to the outer one).
    let current_names = names(&f.project, f.sheet1);
    assert!(current_names.contains(&"D".to_string()));
    assert!(!current_names.contains(&"E".to_string()));
    assert!(history.in_macro());

    history
        .abort_macro(&mut f.project)
        .expect("outer macro abandoned");

    let current_names = names(&f.project, f.sheet1);
    assert!(!current_names.contains(&"D".to_string()));
    assert!(!history.in_macro());
    assert!(history.can_redo());
    assert_eq!(**f.project.tree(), *before_macro);
}

// -- 23-29. second limit: bytes of retained payload -----------------------
//
// `History` now bounds the history by two limits, the stricter of the
// two winning: the number of entries (already covered
// above) and the payload bytes that the history alone
// keeps alive. These tests exercise only the public API, on a
// light project (fixture): the proof at scale (a million floats)
// lives in `tests/memory_pinning.rs`.

#[test]
fn new_history_has_both_default_limits_and_starts_at_zero_retained_bytes() {
    let history = History::new();
    assert_eq!(history.limit(), DEFAULT_LIMIT);
    assert_eq!(history.byte_limit(), DEFAULT_BYTE_LIMIT);
    assert_eq!(history.retained_bytes(), 0);
}

#[test]
fn with_limit_keeps_the_default_byte_limit() {
    let history = History::with_limit(3);
    assert_eq!(history.limit(), 3);
    assert_eq!(history.byte_limit(), DEFAULT_BYTE_LIMIT);
}

#[test]
fn with_limits_zero_zero_is_fully_unbounded() {
    let mut f = fixture();
    let mut history = History::with_limits(0, 0);
    assert_eq!(history.limit(), 0);
    assert_eq!(history.byte_limit(), 0);

    for step in 0..200 {
        history
            .execute(&mut f.project, set_cells(f.a, 0, &[f64::from(step)]))
            .expect("input");
    }
    assert_eq!(history.len(), 200);
}

#[test]
fn retained_bytes_grows_with_structure_then_write_and_falls_on_eviction() {
    let mut f = fixture();
    // Entry limit at 2: the third command evicts the first (the
    // rename), which must make the retained bytes fall.
    let mut history = History::with_limit(2);
    assert_eq!(history.retained_bytes(), 0);

    // Renaming alone changes nothing: the `before` tree and the `after` tree
    // still reference exactly the buffers of the current tree.
    history
        .execute(
            &mut f.project,
            Command::Rename {
                id: f.a,
                name: "A2".to_string(),
            },
        )
        .expect("rename");
    let after_rename = history.retained_bytes();

    // The write that follows forces a copy-on-write of the buffer of A: the
    // version that the rename had captured in its `after` tree
    // (shared with the `before` tree, since nothing had changed yet)
    // is no longer that of the current tree. It becomes a pure cost of the
    // history.
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[42.0]))
        .expect("input");
    let after_write = history.retained_bytes();
    assert!(
        after_write > after_rename,
        "the write that follows the rename must make the retained bytes grow: \
         {after_rename} -> {after_write}"
    );

    // A third command evicts the rename (limit = 2): the buffer
    // that it pinned is no longer referenced by any entry of the stack.
    history
        .execute(&mut f.project, set_cells(f.b, 0, &[0.0]))
        .expect("input on B");
    let after_eviction = history.retained_bytes();
    assert!(
        after_eviction < after_write,
        "evicting the structural entry that pinned the buffer must make the \
         retained bytes fall: {after_write} -> {after_eviction}"
    );
}

#[test]
fn a_lone_data_command_retains_only_its_delta() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[42.0]))
        .expect("input");
    let retained = history.retained_bytes();

    // No structure operation around: no whole buffer pinned,
    // only the delta (a few `f64`), a few dozen bytes at
    // most — very far from the size of a full column buffer.
    assert!(
        retained < 1024,
        "a lone data command should retain only a tiny delta, \
         not {retained} bytes"
    );
}

#[test]
fn set_byte_limit_requires_an_empty_stack_and_no_open_macro() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[1.0]))
        .expect("input");
    assert_eq!(
        history.set_byte_limit(1024),
        Err(CommandError::StackNotEmpty)
    );

    assert_eq!(history.undo(&mut f.project), Ok(true));
    // The redo stack counts too.
    assert_eq!(
        history.set_byte_limit(1024),
        Err(CommandError::StackNotEmpty)
    );

    history.clear().expect("purge");

    history.begin_macro(MacroKind::Import).unwrap();
    assert_eq!(
        history.set_byte_limit(1024),
        Err(CommandError::MacroOpen("set_byte_limit"))
    );
    history
        .end_macro(&mut f.project)
        .expect("empty macro closed");

    history.set_byte_limit(2048).expect("limit accepted");
    assert_eq!(history.byte_limit(), 2048);
}

#[test]
fn the_last_entry_is_never_evicted_even_over_the_byte_limit() {
    let mut f = fixture();
    // `0`: unlimited in number of entries. `1` byte: any
    // entry exceeds it by itself.
    let mut history = History::with_limits(0, 1);

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[42.0]))
        .expect("input");

    assert_eq!(history.len(), 1);
    assert!(
        history.retained_bytes() > history.byte_limit(),
        "the scenario assumes that the single entry alone exceeds the limit"
    );
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(floats(&f.project, f.a), vec![1.0, 2.0, 3.0]);
}

#[test]
fn an_absurdly_low_byte_limit_still_settles_on_exactly_one_entry() {
    let mut f = fixture();
    let mut history = History::with_limits(0, 1);

    for step in 0..5 {
        history
            .execute(&mut f.project, set_cells(f.a, 0, &[f64::from(step)]))
            .expect("input");
        assert!(!history.is_empty(), "the last entry must never be evicted");
    }
    assert_eq!(
        history.len(),
        1,
        "the byte limit, absurd here, brings the stack down to a single entry"
    );
}

// -- node creation/deletion ------------------------------------------------

#[test]
fn add_folder_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let root = f.project.root();

    history
        .execute(
            &mut f.project,
            Command::AddFolder {
                parent: root,
                name: "Experiments".to_string(),
            },
        )
        .expect("folder addition");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::AddFolder {
            name: "Experiments".to_string()
        })
    );
    assert!(names(&f.project, root).contains(&"Experiments".to_string()));

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn add_workbook_round_trip_creates_a_default_sheet() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let root = f.project.root();

    history
        .execute(
            &mut f.project,
            Command::AddWorkbook {
                parent: root,
                name: Some("Book2".to_string()),
            },
        )
        .expect("workbook addition");
    let after = snapshot(&f.project);
    let sheet = f
        .project
        .resolve_path("[Book2]Sheet1")
        .expect("default sheet created with the workbook");
    assert!(f.project.tree().contains(sheet));

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn add_empty_workbook_round_trip_has_no_sheet() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let root = f.project.root();

    history
        .execute(
            &mut f.project,
            Command::AddEmptyWorkbook {
                parent: root,
                name: Some("Empty".to_string()),
            },
        )
        .expect("empty workbook addition");
    let after = snapshot(&f.project);
    let book = f.project.resolve_path("[Empty]").expect("empty workbook");
    assert!(f
        .project
        .tree()
        .children(book)
        .expect("workbook")
        .is_empty());

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn add_worksheet_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::AddWorksheet {
                book: f.book,
                name: None,
            },
        )
        .expect("sheet addition");
    let after = snapshot(&f.project);
    // D1 — two sheets already present (Sheet1, Sheet2): the first
    // candidate tried is "Sheet3".
    let sheet3 = f
        .project
        .resolve_path("[Book1]Sheet3")
        .expect("automatic name Sheet3");
    assert!(f.project.tree().contains(sheet3));

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn add_graph_round_trip_carries_its_spec() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let root = f.project.root();
    let spec = PlotSpec::empty("Curve");

    history
        .execute(
            &mut f.project,
            Command::AddGraph {
                parent: root,
                name: Some("Fig1".to_string()),
                spec: spec.clone(),
            },
        )
        .expect("graph addition");
    let after = snapshot(&f.project);
    let graph = f.project.resolve_path("[Fig1]").expect("graph");
    match f.project.tree().get(graph).expect("node").kind() {
        NodeKind::Graph { spec: found, .. } => assert_eq!(found, &spec),
        other => panic!("expected a graph, found {other:?}"),
    }

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn add_matrix_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let root = f.project.root();

    history
        .execute(
            &mut f.project,
            Command::AddMatrix {
                parent: root,
                name: Some("Mat1".to_string()),
            },
        )
        .expect("matrix addition");
    let after = snapshot(&f.project);
    assert!(f.project.resolve_path("[Mat1]").is_ok());

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn add_note_round_trip_carries_its_text() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let root = f.project.root();

    history
        .execute(
            &mut f.project,
            Command::AddNote {
                parent: root,
                name: Some("N1".to_string()),
                text: "Hello".to_string(),
            },
        )
        .expect("note addition");
    let after = snapshot(&f.project);
    let note = f.project.resolve_path("[N1]").expect("note");
    match f.project.tree().get(note).expect("node").kind() {
        NodeKind::Note { text, .. } => assert_eq!(text, "Hello"),
        other => panic!("expected a note, found {other:?}"),
    }

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn remove_node_round_trip_restores_the_same_node_id_and_its_descendants() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(&mut f.project, Command::RemoveNode { id: f.sheet2 })
        .expect("deletion of Sheet2");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::RemoveNode {
            name: Some("Sheet2".to_string())
        })
    );
    assert!(!f.project.tree().contains(f.sheet2));
    assert!(!f.project.tree().contains(f.x));

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert!(f.project.tree().contains(f.sheet2));
    assert!(f.project.tree().contains(f.x));
    assert_eq!(floats(&f.project, f.x), vec![0.5, 1.5]);
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert!(!f.project.tree().contains(f.sheet2));
    assert_eq!(**f.project.tree(), *after);
}

// -- content editing: window, note, column, graph ---------------------------

#[test]
fn set_window_meta_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let meta = WindowMeta {
        label: "Experiment 3".to_string(),
        comment: "trial".to_string(),
        ..Default::default()
    };

    history
        .execute(
            &mut f.project,
            Command::SetWindowMeta {
                id: f.book,
                meta: meta.clone(),
            },
        )
        .expect("window metadata");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::SetWindowMeta {
            name: Some("Book1".to_string())
        })
    );
    match f.project.tree().get(f.book).expect("node").kind() {
        NodeKind::Workbook { meta: found, .. } => assert_eq!(found, &meta),
        other => panic!("expected a workbook, found {other:?}"),
    }

    assert_eq!(history.undo(&mut f.project), Ok(true));
    match f.project.tree().get(f.book).expect("node").kind() {
        NodeKind::Workbook { meta: found, .. } => assert_eq!(found, &WindowMeta::default()),
        other => panic!("expected a workbook, found {other:?}"),
    }
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn set_window_meta_refuses_a_node_that_is_not_a_window() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    let err = history
        .execute(
            &mut f.project,
            Command::SetWindowMeta {
                id: f.sheet1,
                meta: WindowMeta::default(),
            },
        )
        .expect_err("a sheet is not a window");
    assert!(matches!(err, CommandError::Core(CoreError::Structure(_))));
    assert!(history.is_empty());
    assert_eq!(**f.project.tree(), *before);
}

#[test]
fn set_note_text_round_trip() {
    let mut f = fixture();
    let root = f.project.root();
    let note = f
        .project
        .add_note(root, Some("N1"), "before")
        .expect("note");
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::SetNoteText {
                id: note,
                text: "after".to_string(),
            },
        )
        .expect("note text");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::SetNoteText {
            name: Some("N1".to_string())
        })
    );
    match f.project.tree().get(note).expect("node").kind() {
        NodeKind::Note { text, .. } => assert_eq!(text, "after"),
        other => panic!("expected a note, found {other:?}"),
    }

    assert_eq!(history.undo(&mut f.project), Ok(true));
    match f.project.tree().get(note).expect("node").kind() {
        NodeKind::Note { text, .. } => assert_eq!(text, "before"),
        other => panic!("expected a note, found {other:?}"),
    }
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn set_column_meta_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let meta = ColumnMeta {
        role: ColumnRole::X,
        unit: "s".to_string(),
        ..Default::default()
    };

    history
        .execute(
            &mut f.project,
            Command::SetColumnMeta {
                column: f.a,
                meta: meta.clone(),
            },
        )
        .expect("column metadata");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::SetColumnMeta {
            column: Some(ColumnRef(0))
        })
    );
    assert_eq!(
        f.project
            .tree()
            .get(f.a)
            .expect("column")
            .as_column()
            .expect("column")
            .meta(),
        &meta
    );

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(
        f.project
            .tree()
            .get(f.a)
            .expect("column")
            .as_column()
            .expect("column")
            .meta(),
        &ColumnMeta::default()
    );
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

/// Calculated columns (v1) — setting a formula on a column that already
/// carried values, then undoing, must restore **those values**, not
/// only remove the formula. `stokesforge-commands` never recalculates
/// by itself (that is `stokesforge-session::recalc::recalc_formulas`, outside the
/// history — see `crates/stokesforge-session/tests/formula.rs` for the
/// end-to-end test through the real actor): this test simulates here, by a
/// direct `project.set_values` after the execution of `SetColumnMeta`, the effect
/// of this recalculation, to check that `DataDelta::ColumnMeta::values_before`
/// (captured by `History::execute` before this recalculation took place) is
/// indeed what lets `undo` restore the original values — that was
/// the bug: without this capture, `undo` only removed the formula and
/// left the recalculated values in place.
#[test]
fn undoing_a_formula_set_on_a_column_with_prior_values_restores_them() {
    let mut f = fixture();
    let mut history = History::new();
    let formule = f
        .project
        .tree()
        .formula_from_letters(f.a, "B*2")
        .expect("formula entered");

    history
        .execute(
            &mut f.project,
            Command::SetColumnMeta {
                column: f.a,
                meta: ColumnMeta {
                    formula: Some(formule),
                    ..Default::default()
                },
            },
        )
        .expect("set a formula on A, which already carried [1,2,3]");
    // Simulates the immediate recalculation that `stokesforge-session::actor` would do here,
    // outside the history: overwrites the values of A with values that have
    // nothing to do with [1,2,3].
    f.project
        .set_values(f.a, 0, ValueBlock::Float(vec![20.0, 40.0]))
        .expect("simulates the recalculation (outside the history, as the actor would)");

    assert_eq!(history.undo(&mut f.project), Ok(true));

    let column = f
        .project
        .tree()
        .get(f.a)
        .expect("column A")
        .as_column()
        .expect("the node is a column");
    assert_eq!(column.meta().formula, None, "the formula must be removed");
    assert_eq!(
        (0..column.len())
            .map(|row| match column.get(row) {
                Cell::Float(value) => value,
                Cell::Text(_) => unreachable!("A is numeric"),
            })
            .collect::<Vec<_>>(),
        vec![1.0, 2.0, 3.0],
        "the values from before the formula must be restored"
    );
}

/// Calculated columns (v1) — a direct `SetCells` on a column whose
/// `ColumnMeta.formula` is set is refused before any mutation, and
/// pushes nothing on the history (neither a new entry to undo, nor an
/// observable change of the tree).
#[test]
fn set_cells_on_a_formula_column_is_refused() {
    let mut f = fixture();
    let mut history = History::new();
    let formule = f
        .project
        .tree()
        .formula_from_letters(f.a, "B*2")
        .expect("formula entered");
    history
        .execute(
            &mut f.project,
            Command::SetColumnMeta {
                column: f.a,
                meta: ColumnMeta {
                    formula: Some(formule),
                    ..Default::default()
                },
            },
        )
        .expect("set a formula on A");
    let before = snapshot(&f.project);
    let undo_label_before = history.undo_label().cloned();

    let err = history
        .execute(
            &mut f.project,
            Command::SetCells {
                column: f.a,
                start: 0,
                values: ValueBlock::Float(vec![99.0]),
                expected_version: None,
            },
        )
        .expect_err("a formula column refuses a direct cell write");
    assert_eq!(
        err,
        CommandError::Core(CoreError::FormulaColumnReadOnly { column: f.a })
    );

    // Neither the model nor the history moved.
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.undo_label(), undo_label_before.as_ref());
}

/// Saving a color in the project is an undoable data command:
/// Undo gives back the earlier list, Redo the later one.
#[test]
fn set_saved_colors_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let colors = SavedColors::default().with_added(ColorUsage::Curves, "#ff0000");

    history
        .execute(
            &mut f.project,
            Command::SetSavedColors {
                colors: colors.clone(),
            },
        )
        .expect("colors saved");
    assert_eq!(history.undo_label(), Some(&HistoryLabel::SetSavedColors));
    assert_eq!(f.project.tree().saved_colors(), &colors);
    let after = snapshot(&f.project);

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert!(f.project.tree().saved_colors().is_empty());
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn set_plot_spec_round_trip() {
    let mut f = fixture();
    let root = f.project.root();
    let graph = f
        .project
        .add_graph(root, Some("Fig1"), PlotSpec::default())
        .expect("graph");
    let mut history = History::new();
    let before = snapshot(&f.project);
    let spec = PlotSpec::empty("Curve");

    history
        .execute(
            &mut f.project,
            Command::SetPlotSpec {
                graph,
                spec: spec.clone(),
            },
        )
        .expect("graph spec");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::SetPlotSpec {
            name: Some("Fig1".to_string())
        })
    );
    match f.project.tree().get(graph).expect("node").kind() {
        NodeKind::Graph { spec: found, .. } => assert_eq!(found, &spec),
        other => panic!("expected a graph, found {other:?}"),
    }

    assert_eq!(history.undo(&mut f.project), Ok(true));
    match f.project.tree().get(graph).expect("node").kind() {
        NodeKind::Graph { spec: found, .. } => assert_eq!(found, &PlotSpec::default()),
        other => panic!("expected a graph, found {other:?}"),
    }
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn set_plot_spec_refuses_a_node_that_is_not_a_graph() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    let err = history
        .execute(
            &mut f.project,
            Command::SetPlotSpec {
                graph: f.book,
                spec: PlotSpec::default(),
            },
        )
        .expect_err("a workbook is not a graph");
    assert!(matches!(err, CommandError::Core(CoreError::Structure(_))));
    assert!(history.is_empty());
    assert_eq!(**f.project.tree(), *before);
}

// -- 12 bis. "row" series (#307) -------------------------------------------

/// Graph where each series plots the row `rows[i]` of the sheet of
/// `anchors[i]`. Returns `(graph, V)` where `V` is a column of 8 rows
/// (`0..8`) of `Sheet2`, carrying an `x'`.
fn row_graph(f: &mut Fixture, series: &[(Option<NodeId>, Option<usize>)]) -> (NodeId, NodeId) {
    let v = f
        .project
        .add_column(
            f.sheet2,
            None,
            Some("V"),
            Column::from_f64(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]),
        )
        .expect("column V");
    f.project
        .set_column_meta(
            v,
            ColumnMeta {
                x_prime: Some(1.0),
                ..ColumnMeta::default()
            },
        )
        .expect("x'");
    let mut spec = PlotSpec::empty("Rows");
    let panel = spec.panels.get_mut(0).expect("panel");
    for (anchor, row) in series {
        let mut s = stokesforge_plot::SeriesSpec::new(anchor.unwrap_or(v));
        s.row = *row;
        panel.series.push(s);
    }
    let root = f.project.root();
    let graph = f.project.add_graph(root, Some("Fig"), spec).expect("graph");
    (graph, v)
}

fn series_rows(project: &Project, graph: NodeId) -> Vec<Option<usize>> {
    match project.tree().get(graph).expect("node").kind() {
        NodeKind::Graph { spec, .. } => spec.panels[0].series.iter().map(|s| s.row).collect(),
        other => panic!("expected a graph, found {other:?}"),
    }
}

fn plot_spec_of(project: &Project, graph: NodeId) -> PlotSpec {
    match project.tree().get(graph).expect("node").kind() {
        NodeKind::Graph { spec, .. } => spec.clone(),
        other => panic!("expected a graph, found {other:?}"),
    }
}

#[test]
fn delete_rows_shifts_row_series_and_round_trips() {
    let mut f = fixture();
    let (graph, v) = row_graph(&mut f, &[(None, Some(5))]);
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::DeleteRows {
                sheet: f.sheet2,
                at: 1,
                count: 1,
            },
        )
        .expect("deletion");
    let after = snapshot(&f.project);
    assert_eq!(series_rows(&f.project, graph), vec![Some(4)]);
    // End to end: the series still plots the old row 5.
    let spec = plot_spec_of(&f.project, graph);
    let resolved = stokesforge_core::resolve_series(f.project.tree(), &spec.panels[0].series[0]);
    assert_eq!(resolved.y.expect("y").as_slice(), &[5.0]);
    assert_eq!(floats(&f.project, v)[4], 5.0);

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(series_rows(&f.project, graph), vec![Some(5)]);
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(series_rows(&f.project, graph), vec![Some(4)]);
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn delete_rows_leaves_row_series_before_or_inside_the_range() {
    let mut f = fixture();
    let (graph, _) = row_graph(&mut f, &[(None, Some(1)), (None, Some(3)), (None, Some(4))]);
    let mut history = History::new();
    // Range [3, 5): row 1 is before, 3 and 4 are in the range (open
    // case, left as they are).
    history
        .execute(
            &mut f.project,
            Command::DeleteRows {
                sheet: f.sheet2,
                at: 3,
                count: 2,
            },
        )
        .expect("deletion");
    assert_eq!(
        series_rows(&f.project, graph),
        vec![Some(1), Some(3), Some(4)]
    );
}

#[test]
fn delete_rows_shifts_a_series_just_after_the_range() {
    let mut f = fixture();
    let (graph, _) = row_graph(&mut f, &[(None, Some(5))]);
    let mut history = History::new();
    // Range [3, 5): row 5 is the first after the range.
    history
        .execute(
            &mut f.project,
            Command::DeleteRows {
                sheet: f.sheet2,
                at: 3,
                count: 2,
            },
        )
        .expect("deletion");
    assert_eq!(series_rows(&f.project, graph), vec![Some(3)]);
}

#[test]
fn insert_rows_shifts_row_series_from_the_insertion_row_on() {
    let mut f = fixture();
    let (graph, _) = row_graph(&mut f, &[(None, Some(2)), (None, Some(1))]);
    let mut history = History::new();
    let before = snapshot(&f.project);
    history
        .execute(
            &mut f.project,
            Command::InsertRows {
                sheet: f.sheet2,
                at: 2,
                count: 3,
            },
        )
        .expect("insertion");
    let after = snapshot(&f.project);
    assert_eq!(series_rows(&f.project, graph), vec![Some(5), Some(1)]);
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(series_rows(&f.project, graph), vec![Some(2), Some(1)]);
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn row_series_of_another_sheet_and_column_series_are_untouched() {
    let mut f = fixture();
    // A series anchored in Sheet1 (column A); a "column" series.
    let a = f.a;
    let (graph, _) = row_graph(&mut f, &[(Some(a), Some(5)), (None, None)]);
    let spec_before = plot_spec_of(&f.project, graph);
    let mut history = History::new();
    history
        .execute(
            &mut f.project,
            Command::DeleteRows {
                sheet: f.sheet2,
                at: 1,
                count: 1,
            },
        )
        .expect("deletion");
    assert_eq!(plot_spec_of(&f.project, graph), spec_before);
    history
        .execute(
            &mut f.project,
            Command::InsertRows {
                sheet: f.sheet2,
                at: 0,
                count: 1,
            },
        )
        .expect("insertion");
    assert_eq!(plot_spec_of(&f.project, graph), spec_before);
}

#[test]
fn row_series_beyond_the_sheet_resolve_to_nan_without_panicking() {
    let mut f = fixture();
    let (graph, v) = row_graph(&mut f, &[(None, Some(20))]);
    let mut history = History::new();
    history
        .execute(
            &mut f.project,
            Command::DeleteRows {
                sheet: f.sheet2,
                at: 0,
                count: 1,
            },
        )
        .expect("deletion");
    assert_eq!(series_rows(&f.project, graph), vec![Some(19)]);
    let spec = plot_spec_of(&f.project, graph);
    let resolved = stokesforge_core::resolve_series(f.project.tree(), &spec.panels[0].series[0]);
    assert!(resolved.y.expect("y").as_slice()[0].is_nan());
    let _ = v;
}
