//! The analysis calculation commands (#181): one history entry per
//! gesture, undoable and redoable, refusal to write into a produced column.

use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::ops::Operation;
use stokesforge_commands::{Command, History};
use stokesforge_core::{
    AuxRef, CalculationDraft, Column, ColumnMeta, CoreError, NodeId, OutputDraft, Project,
    SeriesRef, SeriesResult, ValueBlock,
};

struct World {
    project: Project,
    history: History,
    sheet: NodeId,
    x: NodeId,
    y: NodeId,
}

fn world() -> World {
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let x = project
        .add_column(sheet, None, Some("X"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    let y = project
        .add_column(
            sheet,
            None,
            Some("Y"),
            Column::from_f64(&[10.0, 20.0, 30.0]),
        )
        .unwrap();
    World {
        project,
        history: History::new(),
        sheet,
        x,
        y,
    }
}

fn draft(w: &World, data: Vec<f64>) -> CalculationDraft {
    CalculationDraft {
        operation: Operation::Correct {
            op: CorrectOp::Divide,
        },
        series: vec![SeriesRef {
            x: Some(w.x),
            y: w.y,
            sigma: None,
        }],
        aux: None,
        outputs: vec![OutputDraft {
            series: 0,
            key: "corrected".into(),
            index: None,
            meta: ColumnMeta::default(),
            data,
        }],
        results: vec![SeriesResult::default()],
        keep_results: true,
        auto_recalc: true,
        low_r2_dialog: true,
    }
}

fn add(w: &mut World, data: Vec<f64>) {
    let d = draft(w, data);
    w.history
        .execute(&mut w.project, Command::AddCalculation { draft: d })
        .unwrap();
}

fn calc_id(w: &World) -> NodeId {
    *w.project.tree().calculations().keys().next().unwrap()
}

#[test]
fn adding_undoing_and_redoing_is_a_single_entry() {
    let mut w = world();
    let before = w.project.tree().clone();
    add(&mut w, vec![1.0, 2.0, 3.0]);
    assert_eq!(w.history.len(), 1, "column and record: one entry");
    let after = w.project.tree().clone();
    assert_eq!(after.calculations().len(), 1);
    assert_eq!(w.project.tree().children(w.sheet).unwrap().len(), 3);

    w.history.undo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &before);
    assert!(w.project.tree().calculations().is_empty());

    w.history.redo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &after);
}

#[test]
fn the_update_is_undone_by_a_single_undo() {
    let mut w = world();
    add(&mut w, vec![1.0, 2.0, 3.0]);
    let id = calc_id(&w);
    let created = w.project.tree().clone();
    let update = draft(&w, vec![7.0, 8.0, 9.0]);
    w.history
        .execute(
            &mut w.project,
            Command::UpdateCalculation { id, draft: update },
        )
        .unwrap();
    let out = w.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    let values = |w: &World| {
        w.project
            .tree()
            .get(out)
            .unwrap()
            .as_column()
            .unwrap()
            .numeric_view()
            .unwrap()
            .to_vec()
    };
    assert_eq!(values(&w), vec![7.0, 8.0, 9.0]);
    w.history.undo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &created);
    assert_eq!(values(&w), vec![1.0, 2.0, 3.0]);
}

#[test]
fn the_low_r2_dialog_setting_can_be_changed_and_undone() {
    let mut w = world();
    add(&mut w, vec![1.0, 2.0, 3.0]);
    let id = calc_id(&w);
    assert!(w.project.tree().calculation(id).unwrap().low_r2_dialog);
    let created = w.project.tree().clone();
    let mut update = draft(&w, vec![1.0, 2.0, 3.0]);
    update.low_r2_dialog = false;
    w.history
        .execute(
            &mut w.project,
            Command::UpdateCalculation { id, draft: update },
        )
        .unwrap();
    let edited = w.project.tree().clone();
    assert!(!edited.calculation(id).unwrap().low_r2_dialog);
    w.history.undo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &created);
    assert!(w.project.tree().calculation(id).unwrap().low_r2_dialog);
    w.history.redo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &edited);
}

#[test]
fn detaching_keeps_the_columns_and_deleting_removes_them() {
    let mut w = world();
    add(&mut w, vec![1.0, 2.0, 3.0]);
    let id = calc_id(&w);
    let out = w.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    let with_calc = w.project.tree().clone();

    w.history
        .execute(
            &mut w.project,
            Command::RemoveCalculation {
                id,
                with_columns: false,
            },
        )
        .unwrap();
    assert!(w.project.tree().calculations().is_empty());
    assert!(w.project.tree().node(out).is_some());
    w.history.undo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &with_calc);

    w.history
        .execute(
            &mut w.project,
            Command::RemoveCalculation {
                id,
                with_columns: true,
            },
        )
        .unwrap();
    assert!(w.project.tree().node(out).is_none());
    w.history.undo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &with_calc);
}

#[test]
fn a_produced_column_refuses_input_until_detached() {
    let mut w = world();
    add(&mut w, vec![1.0, 2.0, 3.0]);
    let id = calc_id(&w);
    let out = w.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    let write = |w: &mut World| {
        w.history.execute(
            &mut w.project,
            Command::SetCells {
                column: out,
                start: 0,
                values: ValueBlock::Float(vec![99.0]),
                expected_version: None,
            },
        )
    };
    let err = write(&mut w).unwrap_err();
    assert!(
        matches!(
            err,
            stokesforge_commands::CommandError::Core(CoreError::CalculatedColumnReadOnly { column, calculation })
                if column == out && calculation == id
        ),
        "{err:?}"
    );
    // The source column stays editable (witness: the refusal is not general).
    w.history
        .execute(
            &mut w.project,
            Command::SetCells {
                column: w.y,
                start: 0,
                values: ValueBlock::Float(vec![5.0]),
                expected_version: None,
            },
        )
        .unwrap();
    // Detached, the column is ordinary.
    w.history
        .execute(
            &mut w.project,
            Command::RemoveCalculation {
                id,
                with_columns: false,
            },
        )
        .unwrap();
    write(&mut w).unwrap();
}

#[test]
fn a_failure_leaves_neither_entry_nor_change() {
    let mut w = world();
    let before = w.project.tree().clone();
    let mut bad = draft(&w, vec![1.0]);
    bad.series[0].y = NodeId::from_raw(404_040);
    assert!(w
        .history
        .execute(&mut w.project, Command::AddCalculation { draft: bad })
        .is_err());
    assert_eq!(w.history.len(), 0);
    assert_eq!(w.project.tree(), &before);
    assert!(w
        .history
        .execute(
            &mut w.project,
            Command::RemoveCalculation {
                id: NodeId::from_raw(404_040),
                with_columns: true
            }
        )
        .is_err());
    assert_eq!(w.history.len(), 0);
}

/// Any formula: only its presence counts for these tests.
fn constant_formula() -> stokesforge_core::ColumnFormula {
    stokesforge_core::ColumnFormula::from_parts(vec![stokesforge_core::FormulaPart::Text(
        "2".to_string(),
    )])
}

#[test]
fn a_formula_cannot_be_set_on_a_produced_column() {
    let mut w = world();
    add(&mut w, vec![1.0, 2.0, 3.0]);
    let id = calc_id(&w);
    let out = w.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    let before = w.project.tree().clone();
    let err = w
        .history
        .execute(
            &mut w.project,
            Command::SetColumnMeta {
                column: out,
                meta: ColumnMeta {
                    formula: Some(constant_formula()),
                    ..Default::default()
                },
            },
        )
        .unwrap_err();
    assert!(matches!(
        err,
        stokesforge_commands::CommandError::Core(CoreError::CalculatedColumnReadOnly { .. })
    ));
    assert_eq!(w.project.tree(), &before);
    // Another metadata setting (long name, unit) stays allowed.
    w.history
        .execute(
            &mut w.project,
            Command::SetColumnMeta {
                column: out,
                meta: ColumnMeta {
                    long_name: "Corrected".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
}

/// The type and the deletion of cells change the shape of a produced
/// column: the recalculation would undo them, or fail without saying anything.
#[test]
fn type_change_and_cell_deletion_on_an_output_are_refused() {
    let mut w = world();
    add(&mut w, vec![1.0, 2.0, 3.0]);
    let id = calc_id(&w);
    let out = w.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    let before = w.project.tree().clone();
    for command in [
        Command::SetColumnType {
            column: out,
            column_type: stokesforge_core::ColumnType::Mixed,
        },
        Command::DeleteCells {
            columns: vec![w.x, out],
            at: 0,
            count: 1,
        },
    ] {
        let err = w.history.execute(&mut w.project, command).unwrap_err();
        assert!(
            matches!(
                err,
                stokesforge_commands::CommandError::Core(
                    CoreError::CalculatedColumnReadOnly { .. }
                )
            ),
            "{err:?}"
        );
        assert_eq!(w.project.tree(), &before, "no partial state");
    }
}

/// #212: the values change in the same column, with no live calculation.
#[test]
fn replacing_keeps_a_trace_and_is_undone_in_a_single_step() {
    let mut w = world();
    let before = w.project.tree().clone();
    let d = draft(&w, vec![1., 2., 3.]);
    let versions = [w.x, w.y]
        .map(|id| {
            (
                id,
                w.project
                    .tree()
                    .get(id)
                    .unwrap()
                    .as_column()
                    .unwrap()
                    .version(),
            )
        })
        .to_vec();
    w.history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: versions,
            },
        )
        .unwrap();
    assert_eq!(w.project.tree().children(w.sheet).unwrap().len(), 2);
    assert!(w.project.tree().calculation_of_column(w.y).is_none());
    assert_eq!(w.project.tree().correction_traces().len(), 1);
    let after = w.project.tree().clone();
    assert_ne!(
        before.get(w.y).unwrap().as_column().unwrap().version(),
        after.get(w.y).unwrap().as_column().unwrap().version()
    );
    w.history.undo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &before);
    w.history.redo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &after);
    w.history
        .execute(
            &mut w.project,
            Command::SetCells {
                column: w.y,
                start: 0,
                values: ValueBlock::Float(vec![99.]),
                expected_version: None,
            },
        )
        .unwrap();
    assert_eq!(
        w.project.tree().correction_traces().len(),
        1,
        "typing keeps the trace"
    );
}

#[test]
fn replacement_refuses_length_and_versions_without_partial_write() {
    let mut w = world();
    let before = w.project.tree().clone();
    let versions = [w.x, w.y]
        .map(|id| {
            (
                id,
                w.project
                    .tree()
                    .get(id)
                    .unwrap()
                    .as_column()
                    .unwrap()
                    .version(),
            )
        })
        .to_vec();
    let d = draft(&w, vec![1.]);
    assert!(w
        .history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: versions.clone()
            }
        )
        .is_err());
    assert_eq!(w.project.tree(), &before);
    assert_eq!(w.history.len(), 0);
    let d = draft(&w, vec![1., 2., 3.]);
    let mut bad = versions;
    bad[0].1 += 1;
    assert!(w
        .history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: bad
            }
        )
        .is_err());
    assert_eq!(w.project.tree(), &before);
    assert_eq!(w.history.len(), 0);
}

#[test]
fn replacement_preserves_unusable_rows_and_result_nan() {
    let mut w = world();
    w.project
        .set_values(w.x, 1, ValueBlock::Float(vec![f64::NAN]))
        .unwrap();
    let d = draft(&w, vec![1., f64::NAN, f64::NAN, 777.]);
    let versions = [w.x, w.y]
        .map(|id| {
            (
                id,
                w.project
                    .tree()
                    .get(id)
                    .unwrap()
                    .as_column()
                    .unwrap()
                    .version(),
            )
        })
        .to_vec();
    w.history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: versions,
            },
        )
        .unwrap();
    let values = w
        .project
        .tree()
        .get(w.y)
        .unwrap()
        .as_column()
        .unwrap()
        .numeric_view()
        .unwrap();
    assert_eq!(values.len(), 3);
    assert_eq!(values[0], 1.);
    assert_eq!(values[1], 20.);
    assert!(values[2].is_nan());
}

#[test]
fn replacement_deduplicates_sources_and_refuses_contradictory_corrections() {
    let mut w = world();
    let before = w.project.tree().clone();
    let mut d = draft(&w, vec![1., 2., 3.]);
    d.series.push(d.series[0].clone());
    d.results.push(SeriesResult::default());
    let mut output = d.outputs[0].clone();
    output.series = 1;
    output.data[0] = 4.;
    d.outputs.push(output);
    let versions = [w.x, w.y]
        .map(|id| {
            (
                id,
                w.project
                    .tree()
                    .get(id)
                    .unwrap()
                    .as_column()
                    .unwrap()
                    .version(),
            )
        })
        .to_vec();
    assert!(w
        .history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: versions
            }
        )
        .is_err());
    assert_eq!(w.project.tree(), &before);
    assert_eq!(w.history.len(), 0);
}

#[test]
fn replacement_refuses_formula_and_calculated_output_then_removes_orphan_trace() {
    let mut w = world();
    let d = draft(&w, vec![1., 2., 3.]);
    let mut versions = [w.x, w.y]
        .map(|id| {
            (
                id,
                w.project
                    .tree()
                    .get(id)
                    .unwrap()
                    .as_column()
                    .unwrap()
                    .version(),
            )
        })
        .to_vec();
    let mut meta = w
        .project
        .tree()
        .get(w.y)
        .unwrap()
        .as_column()
        .unwrap()
        .meta()
        .clone();
    meta.formula = Some(constant_formula());
    w.project.set_column_meta(w.y, meta).unwrap();
    versions.iter_mut().for_each(|(id, version)| {
        *version = w
            .project
            .tree()
            .get(*id)
            .unwrap()
            .as_column()
            .unwrap()
            .version()
    });
    let before = w.project.tree().clone();
    let error = w
        .history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: versions,
            },
        )
        .unwrap_err();
    assert!(matches!(
        error,
        stokesforge_commands::CommandError::Core(CoreError::FormulaColumnReadOnly { .. })
    ));
    assert_eq!(w.project.tree(), &before);
    let mut meta = w
        .project
        .tree()
        .get(w.y)
        .unwrap()
        .as_column()
        .unwrap()
        .meta()
        .clone();
    meta.formula = None;
    w.project.set_column_meta(w.y, meta).unwrap();
    let d = draft(&w, vec![1., 2., 3.]);
    let versions = [w.x, w.y]
        .map(|id| {
            (
                id,
                w.project
                    .tree()
                    .get(id)
                    .unwrap()
                    .as_column()
                    .unwrap()
                    .version(),
            )
        })
        .to_vec();
    w.history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: versions,
            },
        )
        .unwrap();
    w.history
        .execute(&mut w.project, Command::RemoveNode { id: w.y })
        .unwrap();
    assert!(w.project.tree().correction_traces().is_empty());
    w.history.undo(&mut w.project).unwrap();
    assert_eq!(w.project.tree().correction_traces().len(), 1);
    add(&mut w, vec![1., 2., 3.]);
    let out = w.project.tree().calculation(calc_id(&w)).unwrap().outputs[0]
        .column
        .unwrap();
    let mut d = draft(&w, vec![4., 5., 6.]);
    d.series[0].y = out;
    let versions = [w.x, out]
        .map(|id| {
            (
                id,
                w.project
                    .tree()
                    .get(id)
                    .unwrap()
                    .as_column()
                    .unwrap()
                    .version(),
            )
        })
        .to_vec();
    let before = w.project.tree().clone();
    assert!(w
        .history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: versions
            }
        )
        .is_err());
    assert_eq!(w.project.tree(), &before);
}

// ── Frozen reference and kept originals (#248) ──────────────────────────

/// A world with a reference column (RX, RY) in addition to the series (X, Y).
fn reference_world() -> (World, NodeId, NodeId) {
    let mut w = world();
    let rx = w
        .project
        .add_column(
            w.sheet,
            None,
            Some("RX"),
            Column::from_f64(&[1.0, 2.0, 3.0]),
        )
        .unwrap();
    let ry = w
        .project
        .add_column(
            w.sheet,
            None,
            Some("RY"),
            Column::from_f64(&[2.0, 2.0, 2.0]),
        )
        .unwrap();
    (w, rx, ry)
}

fn corrected_by(w: &World, rx: NodeId, ry: NodeId) -> CalculationDraft {
    CalculationDraft {
        aux: Some(AuxRef { x: Some(rx), y: ry }),
        ..draft(w, vec![5.0, 10.0, 15.0])
    }
}

fn add_corrected(w: &mut World, rx: NodeId, ry: NodeId) {
    let d = corrected_by(w, rx, ry);
    w.history
        .execute(&mut w.project, Command::AddCalculation { draft: d })
        .unwrap();
}

fn values_of(project: &Project, column: NodeId) -> Vec<f64> {
    project
        .tree()
        .get(column)
        .unwrap()
        .as_column()
        .unwrap()
        .numeric_view()
        .unwrap()
        .to_vec()
}

fn store_columns(project: &Project) -> Vec<NodeId> {
    project
        .analysis_store_sheet()
        .map(|sheet| project.tree().children(sheet).unwrap().to_vec())
        .unwrap_or_default()
}

#[test]
fn a_correction_reference_is_frozen_in_a_hidden_workbook() {
    let (mut w, rx, ry) = reference_world();
    add_corrected(&mut w, rx, ry);
    let aux = w
        .project
        .tree()
        .calculation(calc_id(&w))
        .unwrap()
        .aux
        .clone()
        .unwrap();
    assert_ne!(
        aux.y, ry,
        "the calculation reads a copy, not the original column"
    );
    assert!(w.project.is_analysis_copy(aux.y));
    assert!(w.project.is_analysis_copy(aux.x.unwrap()));
    assert_eq!(values_of(&w.project, aux.y), vec![2.0, 2.0, 2.0]);
    assert_eq!(
        store_columns(&w.project).len(),
        2,
        "the Y and its X, nothing else"
    );
    // The workbook is hidden and marked: books() and the Project Manager keep quiet about it.
    let sheet = w.project.analysis_store_sheet().unwrap();
    let book = w.project.tree().parent(sheet).unwrap().unwrap();
    assert!(matches!(
        w.project.tree().get(book).unwrap().kind(),
        stokesforge_core::NodeKind::Workbook { meta, .. } if meta.hidden
    ));
    // The user's sheet gained only the produced column.
    assert_eq!(w.project.tree().children(w.sheet).unwrap().len(), 5);
}

#[test]
fn deleting_or_changing_the_original_reference_breaks_nothing() {
    let (mut w, rx, ry) = reference_world();
    add_corrected(&mut w, rx, ry);
    let id = calc_id(&w);
    let aux = w
        .project
        .tree()
        .calculation(id)
        .unwrap()
        .aux
        .clone()
        .unwrap();
    w.history
        .execute(
            &mut w.project,
            Command::SetCells {
                column: ry,
                start: 0,
                values: ValueBlock::Float(vec![99.0]),
                expected_version: None,
            },
        )
        .unwrap();
    assert_eq!(
        values_of(&w.project, aux.y),
        vec![2.0, 2.0, 2.0],
        "witness: copy intact"
    );
    w.history
        .execute(&mut w.project, Command::RemoveNode { id: ry })
        .unwrap();
    w.history
        .execute(&mut w.project, Command::RemoveNode { id: rx })
        .unwrap();
    let calc = w
        .project
        .tree()
        .calculation(id)
        .expect("the calculation stays");
    assert_eq!(calc.aux.as_ref(), Some(&aux));
    assert_eq!(values_of(&w.project, aux.y), vec![2.0, 2.0, 2.0]);
}

#[test]
fn two_calculations_share_the_copy_until_the_last_one() {
    let (mut w, rx, ry) = reference_world();
    add_corrected(&mut w, rx, ry);
    add_corrected(&mut w, rx, ry);
    assert_eq!(store_columns(&w.project).len(), 2, "one copy for both");
    let ids: Vec<NodeId> = w.project.tree().calculations().keys().copied().collect();
    let before = w.project.tree().clone();
    w.history
        .execute(
            &mut w.project,
            Command::RemoveCalculation {
                id: ids[0],
                with_columns: false,
            },
        )
        .unwrap();
    assert_eq!(
        store_columns(&w.project).len(),
        2,
        "the second still reads it"
    );
    w.history
        .execute(
            &mut w.project,
            Command::RemoveCalculation {
                id: ids[1],
                with_columns: true,
            },
        )
        .unwrap();
    assert!(
        w.project.analysis_store_sheet().is_none(),
        "the last analysis takes the copies and their workbook with it"
    );
    assert!(w.project.tree().windows().iter().all(|id| {
        !matches!(
            w.project.tree().get(*id).unwrap().kind(),
            stokesforge_core::NodeKind::Workbook { meta, .. } if meta.hidden
        )
    }));
    // Undo brings everything back, copies included.
    w.history.undo(&mut w.project).unwrap();
    w.history.undo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &before);
}

#[test]
fn changing_a_calculation_reference_freezes_the_new_one_and_releases_the_old_one() {
    let (mut w, rx, ry) = reference_world();
    add_corrected(&mut w, rx, ry);
    let id = calc_id(&w);
    let first = w
        .project
        .tree()
        .calculation(id)
        .unwrap()
        .aux
        .clone()
        .unwrap();
    // Another state of the reference: a new copy.
    w.history
        .execute(
            &mut w.project,
            Command::SetCells {
                column: ry,
                start: 0,
                values: ValueBlock::Float(vec![4.0, 4.0, 4.0]),
                expected_version: None,
            },
        )
        .unwrap();
    let update = corrected_by(&w, rx, ry);
    w.history
        .execute(
            &mut w.project,
            Command::UpdateCalculation { id, draft: update },
        )
        .unwrap();
    let second = w
        .project
        .tree()
        .calculation(id)
        .unwrap()
        .aux
        .clone()
        .unwrap();
    assert_ne!(first.y, second.y);
    assert_eq!(values_of(&w.project, second.y), vec![4.0, 4.0, 4.0]);
    assert!(
        w.project.tree().node(first.y).is_none(),
        "the old copy is no longer used"
    );
    assert_eq!(store_columns(&w.project).len(), 2);
    // Reopening the calculation without touching the reference keeps the copy:
    // the form passes the AuxRef of the calculation again, already a copy.
    let reopen = CalculationDraft {
        aux: Some(second.clone()),
        ..draft(&w, vec![5.0, 10.0, 15.0])
    };
    w.history
        .execute(
            &mut w.project,
            Command::UpdateCalculation { id, draft: reopen },
        )
        .unwrap();
    assert_eq!(w.project.tree().calculation(id).unwrap().aux, Some(second));
    assert_eq!(store_columns(&w.project).len(), 2);
}

fn replace_with_reference(w: &mut World, rx: NodeId, ry: NodeId) {
    let d = corrected_by(w, rx, ry);
    let versions = [w.x, w.y, rx, ry]
        .map(|id| {
            let version = w
                .project
                .tree()
                .get(id)
                .unwrap()
                .as_column()
                .unwrap()
                .version();
            (id, version)
        })
        .to_vec();
    w.history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: versions,
            },
        )
        .unwrap();
}

#[test]
fn restoring_gives_back_the_originals_overwrites_inputs_and_can_be_undone() {
    let (mut w, rx, ry) = reference_world();
    replace_with_reference(&mut w, rx, ry);
    assert_eq!(
        values_of(&w.project, w.y),
        vec![5.0, 10.0, 15.0],
        "replaced"
    );
    let trace = *w.project.tree().correction_traces().keys().next().unwrap();
    // An original (Y) and the frozen reference (Y, X) live in the hidden workbook.
    assert_eq!(store_columns(&w.project).len(), 3);
    let record = w.project.tree().correction_traces()[&trace].clone();
    assert_eq!(record.originals.len(), 1);
    assert_eq!(
        values_of(&w.project, record.originals[0].1),
        vec![10.0, 20.0, 30.0]
    );

    // An input made since, and a row added: "it restores everything".
    w.history
        .execute(
            &mut w.project,
            Command::SetCells {
                column: w.y,
                start: 0,
                values: ValueBlock::Float(vec![99.0, 98.0, 97.0, 96.0]),
                expected_version: None,
            },
        )
        .unwrap();
    let typed = w.project.tree().clone();
    w.history
        .execute(&mut w.project, Command::RestoreOriginalData { trace })
        .unwrap();
    assert_eq!(values_of(&w.project, w.y), vec![10.0, 20.0, 30.0]);
    assert!(w.project.tree().correction_traces().is_empty());
    assert!(
        w.project.analysis_store_sheet().is_none(),
        "no trace: no copy"
    );
    let restored = w.project.tree().clone();

    w.history.undo(&mut w.project).unwrap();
    assert_eq!(
        w.project.tree(),
        &typed,
        "Ctrl+Z gives back the correction and its copies"
    );
    assert_eq!(w.project.tree().correction_traces().len(), 1);
    w.history.redo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &restored);
}

/// Second correction of the same column Y, with its new results.
fn replace_again(w: &mut World, rx: NodeId, ry: NodeId, results: Vec<f64>) {
    let d = CalculationDraft {
        aux: Some(AuxRef { x: Some(rx), y: ry }),
        ..draft(w, results)
    };
    let versions = [w.x, w.y, rx, ry]
        .map(|id| {
            let version = w
                .project
                .tree()
                .get(id)
                .unwrap()
                .as_column()
                .unwrap()
                .version();
            (id, version)
        })
        .to_vec();
    w.history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: versions,
            },
        )
        .unwrap();
}

/// #308: two successive corrections of the same column keep **the same**
/// raw data. The copy of the second is not a snapshot of the state already
/// corrected by the first.
#[test]
fn two_corrections_of_the_same_column_share_the_raw_data() {
    let (mut w, rx, ry) = reference_world();
    replace_with_reference(&mut w, rx, ry);
    replace_again(&mut w, rx, ry, vec![1.0, 2.0, 3.0]);
    assert_eq!(values_of(&w.project, w.y), vec![1.0, 2.0, 3.0]);
    let traces = w.project.tree().correction_traces().clone();
    assert_eq!(traces.len(), 2);
    let copies: Vec<NodeId> = traces.values().map(|t| t.originals[0].1).collect();
    assert_eq!(copies[0], copies[1], "a single copy of the raw data");
    assert_eq!(values_of(&w.project, copies[0]), vec![10.0, 20.0, 30.0]);
}

/// #308: "Restore original data" always gives back the raw data, whichever
/// correction is restored, and removes all the corrections of the column.
#[test]
fn restoring_any_correction_gives_back_the_raw_data() {
    for restore_first in [true, false] {
        let (mut w, rx, ry) = reference_world();
        replace_with_reference(&mut w, rx, ry);
        replace_again(&mut w, rx, ry, vec![1.0, 2.0, 3.0]);
        let before_restore = w.project.tree().clone();
        let ids: Vec<NodeId> = w
            .project
            .tree()
            .correction_traces()
            .keys()
            .copied()
            .collect();
        let trace = if restore_first { ids[0] } else { ids[1] };
        w.history
            .execute(&mut w.project, Command::RestoreOriginalData { trace })
            .unwrap();
        assert_eq!(
            values_of(&w.project, w.y),
            vec![10.0, 20.0, 30.0],
            "raw data (first trace: {restore_first})"
        );
        assert!(
            w.project.tree().correction_traces().is_empty(),
            "no correction left to restore on this column"
        );
        assert!(w.project.analysis_store_sheet().is_none());
        w.history.undo(&mut w.project).unwrap();
        assert_eq!(
            w.project.tree(),
            &before_restore,
            "Ctrl+Z gives back everything"
        );
    }
}

#[test]
fn deleting_the_replaced_column_removes_the_trace_and_the_copies() {
    let (mut w, rx, ry) = reference_world();
    replace_with_reference(&mut w, rx, ry);
    let trace = *w.project.tree().correction_traces().keys().next().unwrap();
    let with_trace = w.project.tree().clone();
    w.history
        .execute(&mut w.project, Command::RemoveNode { id: w.y })
        .unwrap();
    // No more Results row, hence no more button: nothing to restore.
    assert!(w.project.tree().correction_traces().is_empty());
    assert!(w.project.analysis_store_sheet().is_none());
    let error = w
        .history
        .execute(&mut w.project, Command::RestoreOriginalData { trace })
        .unwrap_err();
    assert!(matches!(
        error,
        stokesforge_commands::CommandError::Core(CoreError::UnknownNode(_))
    ));
    // Undoing the deletion brings back the trace, the copies and the restoration.
    w.history.undo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &with_trace);
    w.history
        .execute(&mut w.project, Command::RestoreOriginalData { trace })
        .unwrap();
    assert_eq!(values_of(&w.project, w.y), vec![10.0, 20.0, 30.0]);
}
