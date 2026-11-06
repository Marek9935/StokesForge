//! The commands of the linked transposed sheet (#253): one history entry
//! per gesture, undoable and redoable, and the link travels with the tree
//! (the "Analyser des lignes" group undoes sheet and calculation as one block).

use stokesforge_commands::{Command, History, MacroKind};
use stokesforge_core::{Column, ColumnMeta, CoreError, NodeId, Project, ValueBlock};

struct World {
    project: Project,
    history: History,
    sheet: NodeId,
    cols: Vec<NodeId>,
}

fn world() -> World {
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let mut cols = Vec::new();
    for i in 0..3 {
        let mut column = Column::from_f64(&[1.0 + i as f64, 10.0 + i as f64]);
        column.set_meta(ColumnMeta {
            x_prime: Some(i as f64),
            ..ColumnMeta::default()
        });
        cols.push(
            project
                .add_column(sheet, None, Some(&format!("C{i}")), column)
                .unwrap(),
        );
    }
    World {
        project,
        history: History::new(),
        sheet,
        cols,
    }
}

fn run(w: &mut World, command: Command) {
    w.history.execute(&mut w.project, command).unwrap();
}

#[test]
fn transposing_is_one_undoable_entry_and_the_link_returns_with_redo() {
    let mut w = world();
    let before = w.project.tree().clone();
    let source = w.sheet;
    run(
        &mut w,
        Command::TransposeRows {
            source,
            rows: vec![1],
        },
    );
    let after = w.project.tree().clone();
    assert_eq!(after.transposes().len(), 1);
    assert!(w.history.undo(&mut w.project).unwrap());
    assert_eq!(w.project.tree().as_ref(), before.as_ref());
    assert!(w.project.tree().transposes().is_empty());
    assert!(w.history.redo(&mut w.project).unwrap());
    assert_eq!(w.project.tree().as_ref(), after.as_ref());
}

#[test]
fn a_refused_transposition_changes_nothing_and_leaves_no_entry() {
    let mut w = world();
    w.project
        .add_column(w.sheet, None, Some("Without"), Column::from_f64(&[1.0]))
        .unwrap();
    let before = w.project.tree().clone();
    let source = w.sheet;
    let error = w
        .history
        .execute(
            &mut w.project,
            Command::TransposeRows {
                source,
                rows: vec![0],
            },
        )
        .unwrap_err();
    assert!(matches!(
        error,
        stokesforge_commands::CommandError::Core(CoreError::TransposeNoXPrime { .. })
    ));
    assert_eq!(w.project.tree().as_ref(), before.as_ref());
    assert!(!w.history.undo(&mut w.project).unwrap());
}

#[test]
fn recalculating_is_one_entry_that_restores_edited_values_on_undo() {
    let mut w = world();
    let source = w.sheet;
    run(
        &mut w,
        Command::TransposeRows {
            source,
            rows: vec![0],
        },
    );
    let sheet = *w.project.tree().transposes().keys().next().unwrap();
    let row = w.project.tree().children(sheet).unwrap()[1];
    run(
        &mut w,
        Command::SetCells {
            column: row,
            start: 0,
            values: ValueBlock::Float(vec![-1.0]),
            expected_version: None,
        },
    );
    let edited = w.project.tree().clone();
    run(&mut w, Command::RecalculateTranspose { sheet });
    let values = |p: &Project| {
        p.tree()
            .node(row)
            .unwrap()
            .as_column()
            .unwrap()
            .numeric_view()
            .unwrap()
            .to_vec()
    };
    assert_eq!(values(&w.project), vec![1.0, 2.0, 3.0]);
    assert!(w.history.undo(&mut w.project).unwrap());
    assert_eq!(w.project.tree().as_ref(), edited.as_ref());
    assert_eq!(values(&w.project), vec![-1.0, 2.0, 3.0]);
}

#[test]
fn the_analyze_rows_macro_undoes_sheet_and_link_in_one_step() {
    let mut w = world();
    let before = w.project.tree().clone();
    w.history.begin_macro(MacroKind::AnalyzeRows).unwrap();
    let source = w.sheet;
    run(
        &mut w,
        Command::TransposeRows {
            source,
            rows: vec![0, 1],
        },
    );
    let sheet = *w.project.tree().transposes().keys().next().unwrap();
    run(&mut w, Command::RecalculateTranspose { sheet });
    w.history.end_macro(&mut w.project).unwrap();
    assert!(w.history.undo(&mut w.project).unwrap());
    assert_eq!(w.project.tree().as_ref(), before.as_ref());
    assert!(!w.history.undo(&mut w.project).unwrap(), "a single group");
    let _ = &w.cols;
}

// -- #463: Recalculate automatically -------------------------------------------

fn transposed(w: &mut World) -> NodeId {
    let source = w.sheet;
    run(
        w,
        Command::TransposeRows {
            source,
            rows: vec![0],
        },
    );
    *w.project.tree().transposes().keys().next().unwrap()
}

#[test]
fn the_box_toggle_is_one_entry_that_undo_and_redo_restore() {
    let mut w = world();
    let sheet = transposed(&mut w);
    assert!(w.project.tree().transpose_link(sheet).unwrap().auto_recalc);
    let checked = w.project.tree().clone();
    run(&mut w, Command::SetTransposeAuto { sheet, auto: false });
    assert!(!w.project.tree().transpose_link(sheet).unwrap().auto_recalc);
    let unchecked = w.project.tree().clone();
    assert!(w.history.undo(&mut w.project).unwrap());
    assert_eq!(w.project.tree().as_ref(), checked.as_ref());
    assert!(w.history.redo(&mut w.project).unwrap());
    assert_eq!(w.project.tree().as_ref(), unchecked.as_ref());
}

#[test]
fn rechecking_is_refused_while_the_source_reads_the_sheet_and_leaves_no_entry() {
    let mut w = world();
    let sheet = transposed(&mut w);
    run(&mut w, Command::SetTransposeAuto { sheet, auto: false });
    // A column of the source reads the transposed sheet (allowed while unchecked).
    let reader = w
        .project
        .add_column(w.sheet, None, Some("R"), Column::from_f64(&[0.0, 0.0]))
        .unwrap();
    let name = w.project.tree().node(sheet).unwrap().name().to_string();
    let formula = w
        .project
        .tree()
        .formula_from_letters(reader, &format!("max('{name}'!B)"))
        .unwrap();
    w.project
        .set_column_meta(
            reader,
            ColumnMeta {
                formula: Some(formula),
                ..ColumnMeta::default()
            },
        )
        .unwrap();
    let before = w.project.tree().clone();
    let error = w
        .history
        .execute(
            &mut w.project,
            Command::SetTransposeAuto { sheet, auto: true },
        )
        .unwrap_err();
    assert!(matches!(
        error,
        stokesforge_commands::CommandError::Core(CoreError::TransposeSourceReadsIt { .. })
    ));
    assert_eq!(w.project.tree().as_ref(), before.as_ref());
    // The refused command left no entry: the unchecking is the last one.
    assert!(w.history.undo(&mut w.project).unwrap());
    assert!(w.project.tree().transpose_link(sheet).unwrap().auto_recalc);
}

/// The pass rewrites the linked columns of a followed sheet outside the
/// history, so a macro replayed over them meets a column that no longer fits
/// its delta: a followed link column is derived and is made to fit (#448); the
/// same column of an unchecked sheet holds the user's snapshot and stays exact.
/// Witness: `is_derived` unchanged.
#[test]
fn a_macro_over_a_followed_link_column_finishes_but_not_over_an_unchecked_one() {
    for followed in [true, false] {
        let mut w = world();
        let sheet = transposed(&mut w);
        if !followed {
            run(&mut w, Command::SetTransposeAuto { sheet, auto: false });
        }
        let row = w.project.tree().children(sheet).unwrap()[1];
        w.history.begin_macro(MacroKind::PasteCells).unwrap();
        run(
            &mut w,
            Command::DeleteCells {
                columns: vec![row],
                at: 2,
                count: 1,
            },
        );
        // The pass that follows the source cuts the column short.
        w.project.set_length(row, 1).unwrap();
        let outcome = w.history.abort_macro(&mut w.project);
        assert_eq!(outcome.is_ok(), followed, "followed: {followed}");
    }
}
