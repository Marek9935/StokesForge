//! Battery 5: nested macros, abort, and the redo stack.
//! This is the part of the history that the repository's `undo_random.rs`
//! does not draw.

use std::sync::Arc;
use stokesforge_commands::{Command, History, MacroKind};
use stokesforge_core::{Column, ColumnType, MixedCell, NodeId, Project, Tree, ValueBlock};

fn sample_project() -> (Project, NodeId, Vec<NodeId>) {
    let mut p = Project::new("M").unwrap();
    let root = p.root();
    p.add_workbook(root, Some("Book1")).unwrap();
    let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
    let mut cols = Vec::new();
    for (i, name) in ["A", "B", "C"].iter().enumerate() {
        cols.push(
            p.add_column(
                sheet,
                None,
                Some(name),
                Column::from_f64(&[i as f64, i as f64 + 1.0, i as f64 + 2.0]),
            )
            .unwrap(),
        );
    }
    (p, sheet, cols)
}

fn same_tree(a: &Arc<Tree>, b: &Arc<Tree>) -> bool {
    a.as_ref() == b.as_ref()
}

#[test]
fn b5_aborted_macro_leaves_no_trace() {
    let (mut p, sheet, cols) = sample_project();
    let mut h = History::new();
    // Some prior history, and one undone step to populate the redo.
    h.execute(
        &mut p,
        Command::SetCells {
            column: cols[0],
            start: 0,
            values: ValueBlock::Float(vec![9.0]),
            expected_version: None,
        },
    )
    .unwrap();
    h.execute(
        &mut p,
        Command::Rename {
            id: cols[1],
            name: "BB".into(),
        },
    )
    .unwrap();
    h.undo(&mut p).unwrap();
    let before = Arc::clone(p.tree());
    let (undo_before, redo_before) = (h.len(), h.redo_len());
    assert_eq!(redo_before, 1, "setup: one step must be redoable");

    h.begin_macro(MacroKind::Import).unwrap();
    h.execute(
        &mut p,
        Command::SetCells {
            column: cols[0],
            start: 0,
            values: ValueBlock::Float(vec![1.0, 2.0]),
            expected_version: None,
        },
    )
    .unwrap();
    h.execute(
        &mut p,
        Command::AddColumn {
            sheet,
            index: None,
            name: Some("Z".into()),
            column: Column::new(ColumnType::Numeric),
        },
    )
    .unwrap();
    h.execute(&mut p, Command::DeleteColumn { column: cols[2] })
        .unwrap();
    h.abort_macro(&mut p).unwrap();

    assert!(
        same_tree(&before, p.tree()),
        "the tree differs after the abort"
    );
    assert_eq!(h.len(), undo_before, "the undo stack changed");
    assert_eq!(h.redo_len(), redo_before, "the redo stack was not restored");
    // And the saved redo must still work.
    assert!(h.redo(&mut p).unwrap(), "the redoable step is lost");
}

#[test]
fn b5_nested_aborted_macro_does_not_touch_the_parent() {
    let (mut p, sheet, cols) = sample_project();
    let mut h = History::new();
    h.begin_macro(MacroKind::Import).unwrap();
    h.execute(
        &mut p,
        Command::Rename {
            id: cols[0],
            name: "AA".into(),
        },
    )
    .unwrap();
    let after_outer = Arc::clone(p.tree());

    h.begin_macro(MacroKind::Import).unwrap();
    h.execute(
        &mut p,
        Command::AddColumn {
            sheet,
            index: None,
            name: Some("Y".into()),
            column: Column::new(ColumnType::Numeric),
        },
    )
    .unwrap();
    h.execute(
        &mut p,
        Command::SetCells {
            column: cols[1],
            start: 0,
            values: ValueBlock::Float(vec![5.0]),
            expected_version: None,
        },
    )
    .unwrap();
    h.abort_macro(&mut p).unwrap();

    assert!(
        same_tree(&after_outer, p.tree()),
        "aborting the inner macro undid (or left) something other than its own steps"
    );
    assert_eq!(h.macro_depth(), 1, "the outer macro must stay open");
    h.end_macro(&mut p).unwrap();
    assert_eq!(h.len(), 1, "the outer macro must make one entry");
    assert!(h.undo(&mut p).unwrap());
    assert_eq!(
        p.tree().node(cols[0]).unwrap().name(),
        "A",
        "undoing the outer macro did not give the column its name back"
    );
}

#[test]
fn b5_empty_macro_leaves_nothing_and_keeps_the_redo() {
    let (mut p, _sheet, cols) = sample_project();
    let mut h = History::new();
    h.execute(
        &mut p,
        Command::Rename {
            id: cols[0],
            name: "AA".into(),
        },
    )
    .unwrap();
    h.undo(&mut p).unwrap();
    assert_eq!(h.redo_len(), 1);

    h.begin_macro(MacroKind::Import).unwrap();
    h.end_macro(&mut p).unwrap();
    assert_eq!(h.len(), 0);
    assert_eq!(h.redo_len(), 1, "an empty macro ate the redo");
}

#[test]
fn b5_macro_with_a_refused_command_stays_consistent() {
    let (mut p, sheet, cols) = sample_project();
    let mut h = History::new();
    let before = Arc::clone(p.tree());
    h.begin_macro(MacroKind::Import).unwrap();
    h.execute(
        &mut p,
        Command::Rename {
            id: cols[0],
            name: "AA".into(),
        },
    )
    .unwrap();
    // Refused: "B" is already taken in the sheet.
    let refusal = h.execute(
        &mut p,
        Command::Rename {
            id: cols[0],
            name: "B".into(),
        },
    );
    assert!(refusal.is_err(), "a rename to a taken name must be refused");
    h.execute(
        &mut p,
        Command::SetCells {
            column: cols[2],
            start: 1,
            values: ValueBlock::Float(vec![8.0, 9.0, 10.0]),
            expected_version: None,
        },
    )
    .unwrap();
    // And a command on a sheet that does not exist.
    let refusal2 = h.execute(
        &mut p,
        Command::InsertRows {
            sheet: NodeId::from_raw(4242),
            at: 0,
            count: 1,
        },
    );
    assert!(refusal2.is_err());
    h.end_macro(&mut p).unwrap();

    assert_eq!(h.len(), 1);
    assert!(h.undo(&mut p).unwrap());
    assert!(
        same_tree(&before, p.tree()),
        "undoing the macro did not bring back the initial tree despite refused commands inside it"
    );
    let _ = sheet;
}

#[test]
fn b5_deeply_nested_macro() {
    let (mut p, _sheet, cols) = sample_project();
    let mut h = History::new();
    let before = Arc::clone(p.tree());
    const N: usize = 64;
    for i in 0..N {
        h.begin_macro(MacroKind::Import).unwrap();
        h.execute(
            &mut p,
            Command::SetCells {
                column: cols[i % 3],
                start: 0,
                values: ValueBlock::Float(vec![i as f64]),
                expected_version: None,
            },
        )
        .unwrap();
    }
    for _ in 0..N {
        h.end_macro(&mut p).unwrap();
    }
    assert_eq!(h.macro_depth(), 0);
    assert_eq!(h.len(), 1, "the {N} nested macros must make a single entry");
    assert!(h.undo(&mut p).unwrap());
    assert!(
        same_tree(&before, p.tree()),
        "the deep nesting is not fully undone"
    );
    assert!(h.redo(&mut p).unwrap(), "and is not redone");
}

#[test]
fn b5_undo_refused_in_macro_breaks_nothing() {
    let (mut p, _sheet, cols) = sample_project();
    let mut h = History::new();
    h.execute(
        &mut p,
        Command::Rename {
            id: cols[0],
            name: "AA".into(),
        },
    )
    .unwrap();
    h.begin_macro(MacroKind::Import).unwrap();
    h.execute(
        &mut p,
        Command::Rename {
            id: cols[1],
            name: "BB".into(),
        },
    )
    .unwrap();
    let during = Arc::clone(p.tree());
    assert!(h.undo(&mut p).is_err(), "undo must be refused in a macro");
    assert!(h.redo(&mut p).is_err(), "redo must be refused in a macro");
    assert!(h.clear().is_err(), "clear must be refused in a macro");
    assert!(
        same_tree(&during, p.tree()),
        "an operation refused during a macro modified the tree"
    );
    h.end_macro(&mut p).unwrap();
}

#[test]
fn b5_macro_then_full_undo_restores_the_initial_tree() {
    // Long sequence mixing macros, structure and data, then a full undo.
    let (mut p, sheet, cols) = sample_project();
    let mut h = History::new();
    let initial = Arc::clone(p.tree());

    for round in 0..20u64 {
        if round % 3 == 0 {
            h.begin_macro(MacroKind::Import).unwrap();
        }
        // The block must follow the current type of the target column: a
        // command refused because of its type is a legitimate refusal by the
        // core, not what this test is looking for.
        let target = cols[(round % 3) as usize];
        let block = match p
            .tree()
            .node(target)
            .unwrap()
            .as_column()
            .unwrap()
            .column_type()
        {
            ColumnType::Mixed => {
                ValueBlock::Mixed(vec![MixedCell::Text(format!("t{round}")), MixedCell::Empty])
            }
            _ => ValueBlock::Float(vec![round as f64, f64::NAN]),
        };
        h.execute(
            &mut p,
            Command::SetCells {
                column: target,
                start: (round % 4) as usize,
                values: block,
                expected_version: None,
            },
        )
        .unwrap();
        if round % 5 == 0 {
            let name = format!("N{round}");
            h.execute(
                &mut p,
                Command::AddColumn {
                    sheet,
                    index: None,
                    name: Some(name),
                    column: Column::from_f64(&[round as f64]),
                },
            )
            .unwrap();
        }
        if round % 7 == 0 {
            h.execute(
                &mut p,
                Command::SetColumnType {
                    column: cols[1],
                    column_type: if round % 2 == 0 {
                        ColumnType::Mixed
                    } else {
                        ColumnType::Numeric
                    },
                },
            )
            .unwrap();
        }
        if round % 3 == 2 && h.in_macro() {
            h.end_macro(&mut p).unwrap();
        }
    }
    while h.in_macro() {
        h.end_macro(&mut p).unwrap();
    }
    let mut steps = 0;
    while h.undo(&mut p).unwrap() {
        steps += 1;
        assert!(steps < 1000, "endless undo loop");
    }
    assert!(
        same_tree(&initial, p.tree()),
        "after a full undo, the tree is not the initial tree"
    );
    while h.redo(&mut p).unwrap() {}
    let mut steps2 = 0;
    while h.undo(&mut p).unwrap() {
        steps2 += 1;
    }
    assert_eq!(
        steps, steps2,
        "the number of steps differs on the second round"
    );
    assert!(
        same_tree(&initial, p.tree()),
        "second round trip is unfaithful"
    );
}

#[test]
fn b5_byte_limit_does_not_drop_the_last_step() {
    let (mut p, _sheet, cols) = sample_project();
    // Tiny byte limit: every entry exceeds it.
    let mut h = History::with_limits(0, 1);
    for i in 0..10u64 {
        h.execute(
            &mut p,
            Command::SetCells {
                column: cols[0],
                start: 0,
                values: ValueBlock::Float(vec![i as f64; 100]),
                expected_version: None,
            },
        )
        .unwrap();
    }
    assert!(
        h.can_undo(),
        "the byte limit evicted everything: nothing left to undo"
    );
    assert_eq!(h.len(), 1, "exactly the last step must remain");
    assert!(h.undo(&mut p).unwrap());
}
