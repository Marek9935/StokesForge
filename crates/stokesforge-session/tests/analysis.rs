//! Recalculation of analysis calculations (#181), seen from the session's
//! public API: an automatic calculation follows its inputs, a manual
//! calculation marks itself stale without moving, a failure keeps the last
//! values, an input lost then found again restarts from zero.
//!
//! The contract "targeted recalculation gives exactly the full recalculation"
//! is held by the differential battery `stokesforge-robustesse` (`b22`); these
//! cases name each behavior.

use stokesforge_analysis::fit::FitSpec;
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::Operation;
use stokesforge_commands::Command;
use stokesforge_core::{
    compute, CalculationDraft, Column, ColumnMeta, NodeId, OutputDraft, Project, SeriesRef,
    Staleness, ValueBlock,
};
use stokesforge_session::{
    DiscardedCalculation, FormulaRecalc, LoadReport, Session, SessionError, SessionHandle, Snapshot,
};

struct Setup {
    sheet: NodeId,
    x: NodeId,
    y: NodeId,
}

fn start(mode: FormulaRecalc) -> (Session, SessionHandle, Setup) {
    let mut project = Project::new("Demo").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    let x = project
        .add_column(
            sheet,
            None,
            Some("X"),
            Column::from_f64(&[1.0, 2.0, 3.0, 4.0]),
        )
        .unwrap();
    let y = project
        .add_column(
            sheet,
            None,
            Some("Y"),
            Column::from_f64(&[10.0, 20.0, 30.0, 40.0]),
        )
        .unwrap();
    let session = Session::spawn_with_recalc(project, None, mode).unwrap();
    let handle = session.handle();
    (session, handle, Setup { sheet, x, y })
}

fn fit_operation() -> Operation {
    Operation::Fit {
        spec: FitSpec {
            model: Model::Linear,
            params: vec![],
        },
        range: None,
        residuals: false,
    }
}

/// Adds a linear fit of (X, Y), automatic or not.
fn add_fit(handle: &SessionHandle, m: &Setup, auto: bool) -> NodeId {
    add_fit_on(handle, m.x, m.y, auto)
}

/// Adds a linear fit of (`x`, `y`), automatic or not.
fn add_fit_on(handle: &SessionHandle, x: NodeId, y: NodeId, auto: bool) -> NodeId {
    let snapshot = handle.snapshot().unwrap();
    let series = vec![SeriesRef {
        x: Some(x),
        y,
        sigma: None,
    }];
    let op = fit_operation();
    let computed = compute(&snapshot.tree, &op, &series, None).unwrap();
    let outputs = computed
        .columns
        .iter()
        .map(|c| OutputDraft {
            series: c.series,
            key: c.key.clone(),
            index: c.index,
            meta: ColumnMeta::default(),
            data: c.data.clone(),
        })
        .collect();
    handle
        .execute(Command::AddCalculation {
            draft: CalculationDraft {
                operation: op,
                series,
                aux: None,
                outputs,
                results: computed.results,
                keep_results: true,
                auto_recalc: auto,
                low_r2_dialog: true,
            },
        })
        .unwrap();
    *handle
        .snapshot()
        .unwrap()
        .tree
        .calculations()
        .keys()
        .next()
        .unwrap()
}

fn enter(column: NodeId, values: &[f64]) -> Command {
    Command::SetCells {
        column,
        start: 0,
        values: ValueBlock::Float(values.to_vec()),
        expected_version: None,
    }
}

fn slope(snapshot: &Snapshot, id: NodeId) -> f64 {
    snapshot.tree.calculation(id).unwrap().results[0]
        .values
        .iter()
        .find(|v| v.key == "param.slope")
        .unwrap()
        .value
}

fn fit_column(snapshot: &Snapshot, id: NodeId) -> Vec<f64> {
    let calc = snapshot.tree.calculation(id).unwrap();
    let out = calc.outputs.iter().find(|o| o.key == "fit").unwrap();
    snapshot
        .tree
        .node(out.column.unwrap())
        .unwrap()
        .as_column()
        .unwrap()
        .numeric_view()
        .unwrap()
        .to_vec()
}

fn modes() -> [FormulaRecalc; 2] {
    [FormulaRecalc::Targeted, FormulaRecalc::Full]
}

/// #320: an analysis inserts its output columns **in the middle** of the sheet,
/// which made the position letters slide — a formula read another column
/// without warning. Since #321 a formula remembers the column itself.
///
/// Sheet `A [1,2,3,4]`, `B [10,20,30,41]`, `C [100,200,300,400]`, empty `D`;
/// `D = C*1`, then a linear fit of (A, B) in automatic recalculation: the
/// output is inserted right after B (the sheet becomes A, B, output, C, D).
/// `D` still equals `C`, and displays `D*1` (C is now the fourth column).
/// Mutant: resolution by position (D would read the fitted curve).
#[test]
fn an_analysis_that_inserts_its_outputs_does_not_change_what_a_formula_reads() {
    for mode in modes() {
        let mut project = Project::new("Demo").unwrap();
        let root = project.root();
        project.add_workbook(root, Some("Book1")).unwrap();
        let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
        let mut column = |name: &str, values: &[f64]| {
            project
                .add_column(sheet, None, Some(name), Column::from_f64(values))
                .unwrap()
        };
        let a = column("A", &[1.0, 2.0, 3.0, 4.0]);
        let b = column("B", &[10.0, 20.0, 30.0, 41.0]);
        let c = column("C", &[100.0, 200.0, 300.0, 400.0]);
        let d = project
            .add_column(
                sheet,
                None,
                Some("D"),
                Column::new(stokesforge_core::ColumnType::Numeric),
            )
            .unwrap();
        let session = Session::spawn_with_recalc(project, None, mode).unwrap();
        let handle = session.handle();
        let read_d = |s: &Snapshot| -> Vec<f64> {
            let column = s.tree.node(d).unwrap().as_column().unwrap();
            (0..column.len())
                .map(|row| match column.get(row) {
                    stokesforge_core::Cell::Float(v) => v,
                    other => panic!("unexpected cell: {other:?}"),
                })
                .collect()
        };

        let formula = handle
            .snapshot()
            .unwrap()
            .tree
            .formula_from_letters(d, "C*1")
            .unwrap();
        handle
            .execute(Command::SetColumnMeta {
                column: d,
                meta: ColumnMeta {
                    formula: Some(formula),
                    ..Default::default()
                },
            })
            .unwrap();
        assert_eq!(
            read_d(&handle.snapshot().unwrap()),
            [100.0, 200.0, 300.0, 400.0]
        );

        add_fit(&handle, &Setup { sheet, x: a, y: b }, true);
        let s = handle.snapshot().unwrap();
        let columns = s.tree.children(sheet).unwrap().to_vec();
        assert_eq!(columns.len(), 5, "the fit output is inserted");
        assert_eq!(
            (columns[0], columns[1], columns[3], columns[4]),
            (a, b, c, d),
            "the output is inserted in the middle, right after B: C and D slide ({mode:?})"
        );
        assert_eq!(
            read_d(&s),
            [100.0, 200.0, 300.0, 400.0],
            "D still reads C ({mode:?})"
        );
        assert_eq!(
            s.tree.formula_letters(d).as_deref(),
            Some("D*1"),
            "C is now the fourth column: the displayed letter follows ({mode:?})"
        );
    }
}

#[test]
fn an_automatic_calculation_follows_its_inputs() {
    for mode in modes() {
        let (_session, handle, m) = start(mode);
        let id = add_fit(&handle, &m, true);
        assert!((slope(&handle.snapshot().unwrap(), id) - 10.0).abs() < 1e-9);

        handle.execute(enter(m.y, &[2.0, 4.0, 6.0, 8.0])).unwrap();
        let after = handle.snapshot().unwrap();
        assert!((slope(&after, id) - 2.0).abs() < 1e-9, "{mode:?}");
        assert!((fit_column(&after, id)[3] - 8.0).abs() < 1e-9);
        assert_eq!(after.tree.calculation(id).unwrap().stale, None);

        // Undoing the input brings the calculation back with it.
        handle.undo().unwrap();
        let undone = handle.snapshot().unwrap();
        assert!((slope(&undone, id) - 10.0).abs() < 1e-9, "{mode:?}");
    }
}

#[test]
fn recalculation_does_not_enter_the_history() {
    let (_session, handle, m) = start(FormulaRecalc::Targeted);
    let id = add_fit(&handle, &m, true);
    handle.execute(enter(m.y, &[2.0, 4.0, 6.0, 8.0])).unwrap();
    handle.execute(enter(m.y, &[3.0, 6.0, 9.0, 12.0])).unwrap();
    // A single "undo" undoes the last input — not the recalculation that
    // followed it, which has no entry of its own.
    handle.undo().unwrap();
    assert!((slope(&handle.snapshot().unwrap(), id) - 2.0).abs() < 1e-9);
}

#[test]
fn a_manual_calculation_goes_stale_without_moving_then_reruns() {
    for mode in modes() {
        let (_session, handle, m) = start(mode);
        let id = add_fit(&handle, &m, false);
        let before = handle.snapshot().unwrap();
        assert_eq!(before.tree.calculation(id).unwrap().stale, None);

        handle.execute(enter(m.y, &[2.0, 4.0, 6.0, 8.0])).unwrap();
        let stale_calc = handle.snapshot().unwrap();
        assert_eq!(
            stale_calc.tree.calculation(id).unwrap().stale,
            Some(Staleness::Manual),
            "{mode:?}"
        );
        assert!((slope(&stale_calc, id) - 10.0).abs() < 1e-9, "values kept");
        assert_eq!(fit_column(&stale_calc, id), fit_column(&before, id));

        handle.recalculate(&[id]).unwrap();
        let fresh = handle.snapshot().unwrap();
        assert_eq!(fresh.tree.calculation(id).unwrap().stale, None);
        assert!((slope(&fresh, id) - 2.0).abs() < 1e-9);
    }
}

#[test]
fn recalculating_without_a_list_reruns_the_stale_calculations() {
    let (_session, handle, m) = start(FormulaRecalc::Targeted);
    let id = add_fit(&handle, &m, false);
    handle.execute(enter(m.y, &[2.0, 4.0, 6.0, 8.0])).unwrap();
    handle.recalculate(&[]).unwrap();
    let snapshot = handle.snapshot().unwrap();
    assert_eq!(snapshot.tree.calculation(id).unwrap().stale, None);
    assert!((slope(&snapshot, id) - 2.0).abs() < 1e-9);
}

#[test]
fn changing_the_definition_of_a_manual_calculation_does_not_make_it_stale() {
    let (_session, handle, m) = start(FormulaRecalc::Targeted);
    let id = add_fit(&handle, &m, false);
    // Undoing then redoing the addition redefines the calculation: no input
    // moved, so it must not declare itself stale.
    handle.undo().unwrap();
    handle.redo().unwrap();
    let snapshot = handle.snapshot().unwrap();
    let id = *snapshot.tree.calculations().keys().next().unwrap_or(&id);
    assert_eq!(snapshot.tree.calculation(id).unwrap().stale, None);
}

#[test]
fn a_failure_keeps_the_last_valid_values_then_recovers() {
    for mode in modes() {
        let (_session, handle, m) = start(mode);
        let id = add_fit(&handle, &m, true);
        let before = handle.snapshot().unwrap();

        // No usable point left: the fit cannot be done.
        handle
            .execute(enter(m.y, &[f64::NAN, f64::NAN, f64::NAN, f64::NAN]))
            .unwrap();
        let failure = handle.snapshot().unwrap();
        assert!(
            matches!(
                failure.tree.calculation(id).unwrap().stale,
                Some(Staleness::Failed(_))
            ),
            "{mode:?}"
        );
        assert_eq!(fit_column(&failure, id), fit_column(&before, id));
        assert!((slope(&failure, id) - 10.0).abs() < 1e-9);

        // Valid data comes back: the calculation recovers.
        handle.execute(enter(m.y, &[2.0, 4.0, 6.0, 8.0])).unwrap();
        let resumed = handle.snapshot().unwrap();
        assert_eq!(resumed.tree.calculation(id).unwrap().stale, None);
        assert!((slope(&resumed, id) - 2.0).abs() < 1e-9);
    }
}

#[test]
fn a_deleted_input_makes_the_calculation_stale_and_undo_restores_it() {
    for mode in modes() {
        let (_session, handle, m) = start(mode);
        let id = add_fit(&handle, &m, true);
        handle
            .execute(Command::DeleteColumn { column: m.x })
            .unwrap();
        let lost = handle.snapshot().unwrap();
        assert_eq!(
            lost.tree.calculation(id).unwrap().stale,
            Some(Staleness::InputMissing),
            "{mode:?}"
        );
        assert!((slope(&lost, id) - 10.0).abs() < 1e-9, "values kept");

        handle.undo().unwrap();
        let found = handle.snapshot().unwrap();
        assert_eq!(found.tree.calculation(id).unwrap().stale, None, "{mode:?}");
    }
}

#[test]
fn a_formula_feeds_a_calculation_and_a_calculation_feeds_a_formula() {
    for mode in modes() {
        let (_session, handle, m) = start(mode);
        let id = add_fit(&handle, &m, true);
        // A formula column reads the "fit" output of the calculation (same sheet).
        let output = {
            let snapshot = handle.snapshot().unwrap();
            let calc = snapshot.tree.calculation(id).unwrap();
            calc.outputs
                .iter()
                .find(|o| o.key == "fit")
                .unwrap()
                .column
                .unwrap()
        };
        let snapshot = handle.snapshot().unwrap();
        let columns = snapshot.tree.children(m.sheet).unwrap().to_vec();
        let letter = columns.iter().position(|&c| c == output).unwrap();
        let name = |i: usize| ((b'A' + i as u8) as char).to_string();
        handle
            .execute(Command::AddColumn {
                sheet: m.sheet,
                index: None,
                name: Some("Double".into()),
                column: Column::new(stokesforge_core::ColumnType::Numeric),
            })
            .unwrap();
        let double = *handle
            .snapshot()
            .unwrap()
            .tree
            .children(m.sheet)
            .unwrap()
            .last()
            .unwrap();
        let formula_of_double = handle
            .snapshot()
            .unwrap()
            .tree
            .formula_from_letters(double, &format!("{}*2", name(letter)))
            .unwrap();
        handle
            .execute(Command::SetColumnMeta {
                column: double,
                meta: ColumnMeta {
                    formula: Some(formula_of_double),
                    ..Default::default()
                },
            })
            .unwrap();
        let before = handle.snapshot().unwrap();
        let read_cells = |s: &Snapshot| {
            s.tree
                .node(double)
                .unwrap()
                .as_column()
                .unwrap()
                .numeric_view()
                .map(|v| v.to_vec())
        };
        assert!(
            (read_cells(&before).unwrap()[3] - 80.0).abs() < 1e-9,
            "{mode:?}"
        );

        // Changing the calculation's source goes through calculation then formula.
        handle.execute(enter(m.y, &[2.0, 4.0, 6.0, 8.0])).unwrap();
        let after = handle.snapshot().unwrap();
        assert!(
            (read_cells(&after).unwrap()[3] - 16.0).abs() < 1e-9,
            "{mode:?}"
        );
    }
}

/// Replaces, in the file, the first occurrence of the bytes of `before` with
/// those of `after`: a modification "outside StokesForge" of a column's values
/// (a `Float` buffer is written as little-endian `f64`, without envelope).
fn alter_file(path: &std::path::Path, before: &[f64], after: &[f64]) {
    let bytes = |v: &[f64]| -> Vec<u8> { v.iter().flat_map(|x| x.to_le_bytes()).collect() };
    let (before, after) = (bytes(before), bytes(after));
    assert_eq!(before.len(), after.len());
    let mut content = std::fs::read(path).unwrap();
    let position = content
        .windows(before.len())
        .position(|window| window == before.as_slice())
        .expect("the values to alter are in the file");
    content[position..position + before.len()].copy_from_slice(&after);
    std::fs::write(path, content).unwrap();
}

/// A1 (#316): on open, no analysis calculation is rerun, automatic or manual.
/// The file's values are kept; the fingerprint of the inputs' content, written
/// at save, says whether they changed since.
#[test]
fn on_load_no_calculation_is_rerun_and_the_fingerprint_says_whether_it_is_stale() {
    for auto in [true, false] {
        for mode in modes() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("p.stf");

            // Intact file: nothing stale (witness: comparison always false).
            let (_session, handle, m) = start(mode);
            let id = add_fit(&handle, &m, auto);
            handle.save(&path).unwrap();
            handle.load(&path).unwrap();
            let intact = handle.snapshot().unwrap();
            assert_eq!(
                intact.tree.calculation(id).unwrap().stale,
                None,
                "auto={auto} {mode:?}"
            );
            assert!((slope(&intact, id) - 10.0).abs() < 1e-9);

            // Input modified outside StokesForge: stale, file's values kept,
            // nothing was rerun (witness: remove the `trust_loaded` flag;
            // comparison always true).
            alter_file(&path, &[10.0, 20.0, 30.0, 40.0], &[2.0, 4.0, 6.0, 8.0]);
            handle.load(&path).unwrap();
            let altered = handle.snapshot().unwrap();
            assert_eq!(
                altered.tree.calculation(id).unwrap().stale,
                Some(Staleness::Manual),
                "auto={auto} {mode:?}"
            );
            assert!(
                (slope(&altered, id) - 10.0).abs() < 1e-9,
                "file values kept, auto={auto} {mode:?}"
            );

            // Recalculation on demand: the values become right.
            handle.recalculate(&[id]).unwrap();
            let after = handle.snapshot().unwrap();
            assert_eq!(after.tree.calculation(id).unwrap().stale, None);
            assert!(
                (slope(&after, id) - 2.0).abs() < 1e-9,
                "auto={auto} {mode:?}"
            );
        }
    }
}

/// An automatic calculation that the file made stale (`Manual`) waits for a
/// gesture: an unrelated command that follows does not recalculate it, in both
/// modes (the full mode recalculated it, moved to `Failed` or "up to date" and
/// diverged from the targeted one — found by `b22` while setting up #321). A
/// real modification of its inputs does recalculate it like any automatic one.
/// Mutant: remove the two `Manual` arms of `update_calculation` (the full mode
/// erases the marker).
#[test]
fn an_automatic_calculation_stale_on_load_waits_for_a_gesture_even_in_full_mode() {
    for mode in modes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.stf");
        let (_session, handle, m) = start(mode);
        let id = add_fit(&handle, &m, true);
        handle.save(&path).unwrap();
        alter_file(&path, &[10.0, 20.0, 30.0, 40.0], &[2.0, 4.0, 6.0, 8.0]);
        handle.load(&path).unwrap();
        assert_eq!(
            handle
                .snapshot()
                .unwrap()
                .tree
                .calculation(id)
                .unwrap()
                .stale,
            Some(Staleness::Manual),
            "{mode:?}"
        );

        // An unrelated column: a recalculation pass, no input touched.
        handle
            .execute(Command::AddColumn {
                sheet: m.sheet,
                index: None,
                name: Some("Other".into()),
                column: Column::from_f64(&[7.0, 8.0]),
            })
            .unwrap();
        let after = handle.snapshot().unwrap();
        assert_eq!(
            after.tree.calculation(id).unwrap().stale,
            Some(Staleness::Manual),
            "still stale, {mode:?}"
        );
        assert!(
            (slope(&after, id) - 10.0).abs() < 1e-9,
            "file values kept, {mode:?}"
        );

        // Witness: really changing an input recalculates it (rewriting it identically does not).
        handle.execute(enter(m.y, &[3.0, 6.0, 9.0, 12.0])).unwrap();
        let last = handle.snapshot().unwrap();
        assert_eq!(last.tree.calculation(id).unwrap().stale, None, "{mode:?}");
        assert!((slope(&last, id) - 3.0).abs() < 1e-9, "{mode:?}");
    }
}

/// Same case, when the input is a formula column: the full mode rewrites it on
/// every pass, so changes its **version** without changing its **content** —
/// the stale calculation stays stale (`Renumber`), in both modes. Mutant:
/// remove the content comparison (the `Renumber` arm).
#[test]
fn a_stale_automatic_calculation_reading_a_formula_stays_stale_in_full_mode() {
    for mode in modes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.stf");
        let (_session, handle, m) = start(mode);
        handle
            .execute(Command::AddColumn {
                sheet: m.sheet,
                index: None,
                name: Some("F".into()),
                column: Column::from_f64(&[0.0; 4]),
            })
            .unwrap();
        let f = *handle
            .snapshot()
            .unwrap()
            .tree
            .children(m.sheet)
            .unwrap()
            .last()
            .unwrap();
        let formula = handle
            .snapshot()
            .unwrap()
            .tree
            .formula_from_letters(f, "B*1")
            .unwrap();
        handle
            .execute(Command::SetColumnMeta {
                column: f,
                meta: ColumnMeta {
                    formula: Some(formula),
                    ..Default::default()
                },
            })
            .unwrap();
        let id = add_fit_on(&handle, m.x, f, true);
        handle.save(&path).unwrap();
        // The values of Y, then those of F (the same content, further in the
        // file): the formula recalculated on load finds the new ones, the
        // calculation did not see the inputs change.
        alter_file(&path, &[10.0, 20.0, 30.0, 40.0], &[2.0, 4.0, 6.0, 8.0]);
        alter_file(&path, &[10.0, 20.0, 30.0, 40.0], &[2.0, 4.0, 6.0, 8.0]);
        handle.load(&path).unwrap();
        assert_eq!(
            handle
                .snapshot()
                .unwrap()
                .tree
                .calculation(id)
                .unwrap()
                .stale,
            Some(Staleness::Manual),
            "{mode:?}"
        );

        handle
            .execute(Command::AddColumn {
                sheet: m.sheet,
                index: None,
                name: Some("Other".into()),
                column: Column::from_f64(&[7.0, 8.0]),
            })
            .unwrap();
        let after = handle.snapshot().unwrap();
        assert_eq!(
            after.tree.calculation(id).unwrap().stale,
            Some(Staleness::Manual),
            "still stale, {mode:?}"
        );
        assert!(
            (slope(&after, id) - 10.0).abs() < 1e-9,
            "file values kept, {mode:?}"
        );
    }
}

/// The trust flag is single-use: an automatic calculation without a memo after
/// the load (addition, undo) is recalculated as before.
#[test]
fn trust_on_load_applies_only_to_the_load() {
    for mode in modes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.stf");
        let (_session, handle, m) = start(mode);
        handle.save(&path).unwrap();
        handle.load(&path).unwrap();
        // After the load: an added automatic calculation follows its inputs.
        let id = add_fit(&handle, &m, true);
        handle.execute(enter(m.y, &[2.0, 4.0, 6.0, 8.0])).unwrap();
        let snapshot = handle.snapshot().unwrap();
        assert_eq!(snapshot.tree.calculation(id).unwrap().stale, None);
        assert!((slope(&snapshot, id) - 2.0).abs() < 1e-9, "{mode:?}");
    }
}

/// The staleness marker written in the file survives loading, and the file's
/// values are trusted as they are.
#[test]
fn a_manual_calculation_stale_before_saving_keeps_its_marker() {
    let dir = tempfile::tempdir().unwrap();
    let (_session2, handle2, m2) = start(FormulaRecalc::Targeted);
    let manual = add_fit(&handle2, &m2, false);
    handle2.execute(enter(m2.y, &[2.0, 4.0, 6.0, 8.0])).unwrap();
    let path2 = dir.path().join("q.stf");
    handle2.save(&path2).unwrap();
    handle2.load(&path2).unwrap();
    let reloaded = handle2.snapshot().unwrap();
    let calc = reloaded.tree.calculation(manual).unwrap();
    assert_eq!(calc.stale, Some(Staleness::Manual));
    assert!(
        (slope(&reloaded, manual) - 10.0).abs() < 1e-9,
        "values trusted"
    );
}

#[test]
fn editing_a_stale_manual_calculation_updates_it_without_making_it_stale_again() {
    let (_session, handle, m) = start(FormulaRecalc::Targeted);
    let id = add_fit(&handle, &m, false);
    handle.execute(enter(m.y, &[2.0, 4.0, 6.0, 8.0])).unwrap();
    assert_eq!(
        handle
            .snapshot()
            .unwrap()
            .tree
            .calculation(id)
            .unwrap()
            .stale,
        Some(Staleness::Manual)
    );
    // "Edit Calculation…": the application recalculates the data then sends
    // the command — the calculation is up to date by construction.
    let snapshot = handle.snapshot().unwrap();
    let calc = snapshot.tree.calculation(id).unwrap();
    let computed = compute(&snapshot.tree, &calc.operation, &calc.series, None).unwrap();
    let outputs = computed
        .columns
        .iter()
        .map(|c| OutputDraft {
            series: c.series,
            key: c.key.clone(),
            index: c.index,
            meta: ColumnMeta::default(),
            data: c.data.clone(),
        })
        .collect();
    handle
        .execute(Command::UpdateCalculation {
            id,
            draft: CalculationDraft {
                operation: calc.operation.clone(),
                series: calc.series.clone(),
                aux: None,
                outputs,
                results: computed.results,
                keep_results: true,
                auto_recalc: false,
                low_r2_dialog: true,
            },
        })
        .unwrap();
    let after = handle.snapshot().unwrap();
    assert_eq!(after.tree.calculation(id).unwrap().stale, None);
    assert!((slope(&after, id) - 2.0).abs() < 1e-9);
}

/// An output that no longer accepts the result (a damaged file, a type forced
/// outside a command) marks the calculation as failed instead of staying frozen
/// with no sign.
#[test]
fn an_output_that_refuses_the_result_marks_the_calculation_as_failed() {
    for mode in modes() {
        let mut project = Project::new("Demo").unwrap();
        let root = project.root();
        project.add_workbook(root, Some("Book1")).unwrap();
        let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
        let x = project
            .add_column(sheet, None, Some("X"), Column::from_f64(&[1.0, 2.0, 3.0]))
            .unwrap();
        let y = project
            .add_column(sheet, None, Some("Y"), Column::from_f64(&[2.0, 4.0, 6.0]))
            .unwrap();
        let series = vec![SeriesRef {
            x: Some(x),
            y,
            sigma: None,
        }];
        let op = fit_operation();
        let computed = compute(project.tree(), &op, &series, None).unwrap();
        let outputs = computed
            .columns
            .iter()
            .map(|c| OutputDraft {
                series: c.series,
                key: c.key.clone(),
                index: c.index,
                meta: ColumnMeta::default(),
                data: c.data.clone(),
            })
            .collect();
        let id = project
            .add_calculation(CalculationDraft {
                operation: op,
                series,
                aux: None,
                outputs,
                results: computed.results,
                keep_results: true,
                auto_recalc: true,
                low_r2_dialog: true,
            })
            .unwrap();
        let out = project.tree().calculation(id).unwrap().outputs[0]
            .column
            .unwrap();
        project
            .set_column_type(out, stokesforge_core::ColumnType::Mixed)
            .unwrap();

        let session = Session::spawn_with_recalc(project, None, mode).unwrap();
        let handle = session.handle();
        handle.execute(enter(y, &[1.0, 2.0, 3.0])).unwrap();
        let snapshot = handle.snapshot().unwrap();
        assert!(
            matches!(
                snapshot.tree.calculation(id).unwrap().stale,
                Some(Staleness::Failed(
                    stokesforge_analysis::AnalysisError::OutputUnwritable
                ))
            ),
            "{mode:?} : {:?}",
            snapshot.tree.calculation(id).unwrap().stale
        );
    }
}

#[test]
fn replacing_updates_the_dependents_and_undo_restores_their_values() {
    use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
    let (session, handle, m) = start(FormulaRecalc::Targeted);
    let calc = add_fit(&handle, &m, true);
    let before = handle.snapshot().unwrap();
    let initial = fit_column(&before, calc);
    let series = vec![SeriesRef {
        x: Some(m.x),
        y: m.y,
        sigma: None,
    }];
    let operation = Operation::Baseline {
        method: BaselineMethod::Line {
            p1: (0., 1.),
            p2: (100., 1.),
        },
        op: BaselineOp::Subtract,
        keep_baseline: false,
    };
    let computed = compute(&before.tree, &operation, &series, None).unwrap();
    let outputs = computed
        .columns
        .into_iter()
        .map(|c| OutputDraft {
            series: c.series,
            key: c.key,
            index: c.index,
            meta: ColumnMeta::default(),
            data: c.data,
        })
        .collect();
    let expected_versions = [m.x, m.y]
        .map(|id| {
            (
                id,
                before.tree.get(id).unwrap().as_column().unwrap().version(),
            )
        })
        .to_vec();
    handle
        .execute(Command::ReplaceAnalysisData {
            draft: CalculationDraft {
                operation,
                series,
                aux: None,
                outputs,
                results: computed.results,
                keep_results: true,
                auto_recalc: false,
                low_r2_dialog: true,
            },
            expected_versions,
        })
        .unwrap();
    let after = handle.snapshot().unwrap();
    let corrected = fit_column(&after, calc);
    assert_ne!(initial, corrected);
    for (a, b) in initial.iter().zip(corrected) {
        assert!((a - b - 1.).abs() < 1e-8);
    }
    assert_ne!(
        before.tree.get(m.y).unwrap().as_column().unwrap().version(),
        after.tree.get(m.y).unwrap().as_column().unwrap().version()
    );
    handle.undo().unwrap();
    let undone = handle.snapshot().unwrap();
    assert_eq!(undone.tree.correction_traces().len(), 0);
    for (a, b) in initial.iter().zip(fit_column(&undone, calc)) {
        assert!((a - b).abs() < 1e-8);
    }
    session.shutdown().unwrap();
}

#[test]
fn recalculation_keeps_the_low_r2_dialog_setting() {
    let (_session, handle, m) = start(FormulaRecalc::Targeted);
    let id = add_fit(&handle, &m, true);
    let before = handle.snapshot().unwrap();
    let calc = before.tree.calculation(id).unwrap();
    let series = calc.series.clone();
    let op = calc.operation.clone();
    let computed = compute(&before.tree, &op, &series, None).unwrap();
    let outputs = computed
        .columns
        .iter()
        .map(|c| OutputDraft {
            series: c.series,
            key: c.key.clone(),
            index: c.index,
            meta: ColumnMeta::default(),
            data: c.data.clone(),
        })
        .collect();
    handle
        .execute(Command::UpdateCalculation {
            id,
            draft: CalculationDraft {
                operation: op,
                series,
                aux: None,
                outputs,
                results: computed.results,
                keep_results: true,
                auto_recalc: true,
                low_r2_dialog: false,
            },
        })
        .unwrap();
    let slope_before = slope(&handle.snapshot().unwrap(), id);
    handle.execute(enter(m.y, &[3.0, 1.0, 7.0])).unwrap();
    let after = handle.snapshot().unwrap();
    assert_ne!(
        slope(&after, id),
        slope_before,
        "the calculation was indeed redone"
    );
    assert!(!after.tree.calculation(id).unwrap().low_r2_dialog);
}

/// A non-finite operation (#278) is refused by the session for the three
/// commands that write a calculation: model error, and neither the tree, nor
/// the revision, nor the history moves (a refusal is not an undoable gesture).
#[test]
fn the_session_refuses_a_non_finite_operation_without_changing_anything() {
    use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
    use stokesforge_commands::CommandError;
    use stokesforge_core::CoreError;
    let (_session, handle, m) = start(FormulaRecalc::Targeted);
    let id = add_fit(&handle, &m, false);
    let before = handle.snapshot().unwrap();
    let calc = before.tree.calculation(id).unwrap().clone();
    let nan = Operation::Baseline {
        method: BaselineMethod::Constant {
            point: (f64::NAN, 1.0),
        },
        op: BaselineOp::Subtract,
        keep_baseline: false,
    };
    let draft = CalculationDraft {
        operation: nan,
        series: calc.series.clone(),
        aux: None,
        outputs: vec![OutputDraft {
            series: 0,
            key: "corrected".into(),
            index: None,
            meta: ColumnMeta::default(),
            data: vec![0.0; 4],
        }],
        results: calc.results.clone(),
        keep_results: true,
        auto_recalc: false,
        low_r2_dialog: true,
    };
    let versions: Vec<(NodeId, u64)> = [m.x, m.y]
        .into_iter()
        .map(|c| {
            let v = before.tree.node(c).unwrap().as_column().unwrap().version();
            (c, v)
        })
        .collect();
    for command in [
        Command::AddCalculation {
            draft: draft.clone(),
        },
        Command::UpdateCalculation {
            id,
            draft: draft.clone(),
        },
        Command::ReplaceAnalysisData {
            draft,
            expected_versions: versions,
        },
    ] {
        let error = handle.execute(command).unwrap_err();
        assert!(
            matches!(
                error,
                SessionError::Command(CommandError::Core(CoreError::Structure(_)))
            ),
            "{error:?}"
        );
        let after = handle.snapshot().unwrap();
        assert_eq!(after.revision, before.revision, "revision unchanged");
        assert!(std::sync::Arc::ptr_eq(&after.tree, &before.tree));
        assert_eq!(after.history, before.history, "history unchanged");
    }
}

/// `load_with_report` relays the file's report (#278); `load` loads the same
/// project without saying anything; a sound file returns an empty report.
#[test]
fn loading_relays_the_calculations_dropped_by_persistence() {
    let (_session, handle, m) = start(FormulaRecalc::Targeted);
    let id = add_fit(&handle, &m, false);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    handle.save(&path).unwrap();

    let (revision, report) = handle.load_with_report(&path).unwrap();
    assert_eq!(report, LoadReport::default(), "sound file: empty report");
    assert!(report.is_clean());
    assert_eq!(handle.snapshot().unwrap().revision, revision);

    // Same length: the "linear" model becomes unreadable.
    let mut bytes = std::fs::read(&path).unwrap();
    let at = bytes
        .windows(6)
        .position(|w| w == b"linear")
        .expect("the model is in the file");
    bytes[at..at + 6].copy_from_slice(b"linexx");
    std::fs::write(&path, &bytes).unwrap();

    let (revision2, report) = handle.load_with_report(&path).unwrap();
    assert!(revision2 > revision, "a load advances the revision");
    assert_eq!(
        report.discarded_calculations,
        vec![DiscardedCalculation {
            id: id.raw(),
            kind: Some("fit".into())
        }]
    );
    assert!(handle.snapshot().unwrap().tree.calculations().is_empty());
    // `load`: same loading, the revision advances, nothing breaks.
    let revision3 = handle.load(&path).unwrap();
    assert!(revision3 > revision2);
    assert!(handle.snapshot().unwrap().tree.calculations().is_empty());
    // A read failure reports nothing: nothing changed.
    let missing = dir.path().join("missing.stf");
    assert!(handle.load_with_report(&missing).is_err());
    assert_eq!(handle.snapshot().unwrap().revision, revision3);
}

/// #309: the report also relays the dropped correction traces.
#[test]
fn loading_relays_the_dropped_correction_traces() {
    use stokesforge_analysis::correct::CorrectOp;
    use stokesforge_core::{CalculationDraft, ColumnMeta, OutputDraft, SeriesRef, SeriesResult};
    let mut project = stokesforge_core::Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let y = project
        .add_column(
            sheet,
            None,
            Some("Y"),
            stokesforge_core::Column::from_f64(&[10.0, 20.0]),
        )
        .unwrap();
    let version = project
        .tree()
        .get(y)
        .unwrap()
        .as_column()
        .unwrap()
        .version();
    let trace = project
        .replace_analysis_data(
            CalculationDraft {
                operation: Operation::Correct {
                    op: CorrectOp::Subtract,
                },
                series: vec![SeriesRef {
                    x: None,
                    y,
                    sigma: None,
                }],
                aux: None,
                outputs: vec![OutputDraft {
                    series: 0,
                    key: "corrected".into(),
                    index: None,
                    meta: ColumnMeta::default(),
                    data: vec![0.0, 10.0],
                }],
                results: vec![SeriesResult::default()],
                keep_results: false,
                auto_recalc: false,
                low_r2_dialog: true,
            },
            &[(y, version)],
        )
        .unwrap();
    let session = Session::spawn(project, None).unwrap();
    let handle = session.handle();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    handle.save(&path).unwrap();
    assert!(handle.load_with_report(&path).unwrap().1.is_clean());
    let mut bytes = std::fs::read(&path).unwrap();
    let at = bytes
        .windows(8)
        .position(|w| w == b"subtract")
        .expect("the operation is in the file");
    bytes[at..at + 8].copy_from_slice(b"subtraXX");
    std::fs::write(&path, &bytes).unwrap();
    let (_, report) = handle.load_with_report(&path).unwrap();
    assert_eq!(
        report.discarded_corrections,
        vec![DiscardedCalculation {
            id: trace.raw(),
            kind: Some("correct".into())
        }]
    );
    assert!(handle
        .snapshot()
        .unwrap()
        .tree
        .correction_traces()
        .is_empty());
}
