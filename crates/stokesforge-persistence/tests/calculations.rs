//! The analysis calculations (#181) go through the `.stf` file.

use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::ops::{Operation, Warning};
use stokesforge_core::{
    CalculationDraft, Column, ColumnMeta, OutputDraft, Project, ResultValue, SeriesRef,
    SeriesResult, Staleness,
};
use stokesforge_persistence::{load, save};

fn project_with_calculation() -> Project {
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
    project
        .add_calculation(CalculationDraft {
            operation: Operation::Correct {
                op: CorrectOp::Subtract,
            },
            series: vec![SeriesRef {
                x: Some(x),
                y,
                sigma: None,
            }],
            aux: None,
            outputs: vec![OutputDraft {
                series: 0,
                key: "corrected".into(),
                index: None,
                meta: ColumnMeta::default(),
                data: vec![9.0, 18.0, f64::NAN],
            }],
            results: vec![SeriesResult {
                values: vec![
                    ResultValue {
                        key: "count.outside".into(),
                        index: None,
                        value: 1.0,
                        error: None,
                    },
                    ResultValue {
                        key: "param.a".into(),
                        index: Some(2),
                        value: f64::NAN,
                        error: Some(0.5),
                    },
                ],
                skipped: 3,
                warnings: vec![
                    Warning::NegativeClipped(4),
                    Warning::IrfStepMismatch,
                    Warning::Undetermined(2),
                    Warning::NotConverged,
                    Warning::SignalStepNotUniform,
                    Warning::Undetermined(17),
                    Warning::IrfTruncated,
                    Warning::BridgedReferenceGap(3),
                    Warning::DivisionByZero(2),
                ],
            }],
            keep_results: true,
            auto_recalc: false,
            low_r2_dialog: false,
        })
        .unwrap();
    project
}

#[test]
fn a_calculation_survives_a_round_trip() {
    let project = project_with_calculation();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    save(&project, &path).unwrap();
    let back = load(&path).unwrap();
    assert_eq!(back.tree().calculations().len(), 1);
    assert_eq!(back.tree(), project.tree());
}

#[test]
fn the_weak_r2_dialog_setting_of_a_calculation_survives_the_file() {
    // The fixture turns it off (#288): the default `true` must not turn it back on.
    let project = project_with_calculation();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("r2.stf");
    save(&project, &path).unwrap();
    let back = load(&path).unwrap();
    let calc = back.tree().calculations().values().next().unwrap();
    assert!(!calc.low_r2_dialog);
}

#[test]
fn manual_staleness_and_the_lost_input_are_kept_the_failure_becomes_manual() {
    for (stale, expected) in [
        (Staleness::Manual, Staleness::Manual),
        (Staleness::InputMissing, Staleness::InputMissing),
        (
            Staleness::Failed(stokesforge_analysis::AnalysisError::EmptyRange),
            Staleness::Manual,
        ),
    ] {
        let mut project = project_with_calculation();
        let id = *project.tree().calculations().keys().next().unwrap();
        project.set_calculation_stale(id, Some(stale)).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.stf");
        save(&project, &path).unwrap();
        let back = load(&path).unwrap();
        assert_eq!(back.tree().calculation(id).unwrap().stale, Some(expected));
    }
}

#[test]
fn identifiers_continue_after_a_reload() {
    let project = project_with_calculation();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    save(&project, &path).unwrap();
    let mut back = load(&path).unwrap();
    let id = *back.tree().calculations().keys().next().unwrap();
    let book = back.tree().children(back.root()).unwrap()[0];
    let fresh = back.add_workbook(back.root(), Some("Other")).unwrap();
    assert_ne!(fresh, id);
    assert_ne!(fresh, book);
}

#[test]
fn the_warning_setting_of_a_graph_survives_a_round_trip() {
    for wanted in [true, false] {
        let mut project = project_with_calculation();
        let spec = stokesforge_plot::PlotSpec {
            stale_warning: wanted,
            ..Default::default()
        };
        let graph = project.add_graph(project.root(), None, spec).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.stf");
        save(&project, &path).unwrap();
        let back = load(&path).unwrap();
        let stokesforge_core::NodeKind::Graph { spec, .. } =
            back.tree().node(graph).unwrap().kind()
        else {
            panic!("a graph expected");
        };
        assert_eq!(spec.stale_warning, wanted);
    }
}

#[test]
fn the_decimal_separator_of_a_graph_survives_a_round_trip_and_defaults_to_point() {
    for wanted in [true, false] {
        let mut project = project_with_calculation();
        let spec = stokesforge_plot::PlotSpec {
            decimal_comma: wanted,
            ..Default::default()
        };
        let graph = project.add_graph(project.root(), None, spec).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.stf");
        save(&project, &path).unwrap();
        let back = load(&path).unwrap();
        let stokesforge_core::NodeKind::Graph { spec, .. } =
            back.tree().node(graph).unwrap().kind()
        else {
            panic!("a graph expected");
        };
        assert_eq!(spec.decimal_comma, wanted);
    }
    assert!(!stokesforge_plot::PlotSpec::default().decimal_comma);
}

#[test]
fn the_two_constant_baselines_go_through_the_stf_file() {
    use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
    for method in [
        BaselineMethod::Constant { point: (2.5, 7.0) },
        BaselineMethod::ConstantZone { zone: (1.0, 3.0) },
    ] {
        let mut project = project_with_calculation();
        let old = project
            .tree()
            .calculations()
            .values()
            .next()
            .unwrap()
            .clone();
        let operation = Operation::Baseline {
            method,
            op: BaselineOp::Subtract,
            keep_baseline: false,
        };
        project
            .add_calculation(CalculationDraft {
                operation: operation.clone(),
                series: old.series.clone(),
                aux: None,
                outputs: Vec::new(),
                results: Vec::new(),
                keep_results: false,
                auto_recalc: true,
                low_r2_dialog: true,
            })
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("constant.stf");
        save(&project, &path).unwrap();
        let back = load(&path).unwrap();
        assert!(back
            .tree()
            .calculations()
            .values()
            .any(|c| c.operation == operation));
        assert_eq!(back.tree(), project.tree());
    }
}

#[test]
fn final_corrections_survive_the_file_without_a_calculation_link() {
    let mut project = project_with_calculation();
    let (id, calc) = project
        .tree()
        .calculations()
        .iter()
        .next()
        .map(|(id, c)| (*id, c.clone()))
        .unwrap();
    let output = calc.outputs[0].column.unwrap();
    project.remove_calculation(id, true).unwrap();
    let y = calc.series[0].y;
    for correction in [1., 2.] {
        let versions = calc
            .series
            .iter()
            .flat_map(|s| [Some(s.y), s.x].into_iter().flatten())
            .map(|id| {
                (
                    id,
                    project
                        .tree()
                        .get(id)
                        .unwrap()
                        .as_column()
                        .unwrap()
                        .version(),
                )
            })
            .collect::<Vec<_>>();
        project
            .replace_analysis_data(
                CalculationDraft {
                    operation: calc.operation.clone(),
                    series: calc.series.clone(),
                    aux: None,
                    outputs: vec![OutputDraft {
                        series: 0,
                        key: "corrected".into(),
                        index: None,
                        meta: ColumnMeta::default(),
                        data: vec![correction; 3],
                    }],
                    results: calc.results.clone(),
                    keep_results: false,
                    auto_recalc: false,
                    low_r2_dialog: true,
                },
                &versions,
            )
            .unwrap();
    }
    assert!(project.tree().node(output).is_none());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("corrections.stf");
    save(&project, &path).unwrap();
    let back = load(&path).unwrap();
    assert_eq!(back.tree(), project.tree());
    assert_eq!(back.tree().correction_traces().len(), 2);
    // #308: the two corrections of Y share the same raw data, before
    // as well as after the file.
    let copies: Vec<_> = back
        .tree()
        .correction_traces()
        .values()
        .map(|t| t.originals[0].1)
        .collect();
    assert_eq!(copies[0], copies[1]);
    assert!(back.tree().calculations().is_empty());
    assert!(back.tree().calculation_of_column(y).is_none());
    let mut back = back;
    let next = back.add_note(back.root(), None, "next").unwrap();
    assert!(back
        .tree()
        .correction_traces()
        .keys()
        .all(|id| id.raw() < next.raw()));
}

/// A correction by a frozen reference, replacing the data (#248): the
/// hidden workbook, the copy of the original and the reference survive the file,
/// and "Restore original data" works after a reload.
#[test]
fn frozen_copies_and_originals_survive_the_file() {
    use stokesforge_core::AuxRef;
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let mut add = |name: &str, values: &[f64]| {
        project
            .add_column(sheet, None, Some(name), Column::from_f64(values))
            .unwrap()
    };
    let x = add("X", &[1.0, 2.0, 3.0]);
    let y = add("Y", &[10.0, 20.0, 30.0]);
    let rx = add("RX", &[1.0, 2.0, 3.0]);
    let ry = add("RY", &[2.0, 2.0, 2.0]);
    let versions: Vec<_> = [x, y, rx, ry]
        .iter()
        .map(|id| {
            let version = project
                .tree()
                .get(*id)
                .unwrap()
                .as_column()
                .unwrap()
                .version();
            (*id, version)
        })
        .collect();
    project
        .replace_analysis_data(
            CalculationDraft {
                operation: Operation::Correct {
                    op: CorrectOp::Divide,
                },
                series: vec![SeriesRef {
                    x: Some(x),
                    y,
                    sigma: None,
                }],
                aux: Some(AuxRef { x: Some(rx), y: ry }),
                outputs: vec![OutputDraft {
                    series: 0,
                    key: "corrected".into(),
                    index: None,
                    meta: ColumnMeta::default(),
                    data: vec![5.0, 10.0, 15.0],
                }],
                results: vec![SeriesResult::default()],
                keep_results: false,
                auto_recalc: false,
                low_r2_dialog: true,
            },
            &versions,
        )
        .unwrap();
    assert!(project.analysis_store_sheet().is_some());

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("copies.stf");
    save(&project, &path).unwrap();
    let mut back = load(&path).unwrap();
    assert_eq!(back.tree(), project.tree());
    assert!(
        back.analysis_store_sheet().is_some(),
        "the hidden workbook comes back marked"
    );
    let trace = *back.tree().correction_traces().keys().next().unwrap();
    assert_eq!(back.tree().correction_traces()[&trace].originals.len(), 1);
    back.restore_original_data(trace).unwrap();
    let restored: Vec<f64> = back
        .tree()
        .get(y)
        .unwrap()
        .as_column()
        .unwrap()
        .numeric_view()
        .unwrap()
        .to_vec();
    assert_eq!(restored, vec![10.0, 20.0, 30.0]);
    assert!(back.analysis_store_sheet().is_none());
}

/// A calculation whose operation is unreadable is not lost in silence (#278):
/// the load report names it, and the rest of the project loads.
#[test]
fn an_unreadable_calculation_is_named_by_the_report_and_the_rest_loads() {
    let project = project_with_calculation();
    let id = project.tree().calculations().keys().next().unwrap().raw();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    save(&project, &path).unwrap();

    // Same length: the operation is damaged ("subtract" no longer exists) without
    // shifting a single byte of the file.
    let mut bytes = std::fs::read(&path).unwrap();
    let needle = b"subtract";
    let at = bytes
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("the operation is written in the file");
    bytes[at..at + needle.len()].copy_from_slice(b"subtraXX");
    std::fs::write(&path, &bytes).unwrap();

    let (loaded, _geometries, report) = stokesforge_persistence::load_with_report(&path).unwrap();
    assert!(loaded.tree().calculations().is_empty());
    assert_eq!(
        report.discarded_calculations,
        vec![stokesforge_persistence::DiscardedCalculation {
            id,
            kind: Some("correct".into())
        }]
    );
    // The rest of the project is there.
    assert_eq!(
        loaded.tree().root(),
        project.tree().root(),
        "the root is that of the file"
    );
    assert_eq!(loaded.tree().len(), project.tree().len());
    // A sound file reports nothing.
    save(&project, &path).unwrap();
    assert!(stokesforge_persistence::load_with_report(&path)
        .unwrap()
        .2
        .is_clean());
}

// ── Calculations discarded at load (#278) ──────────────────────────────────

/// A project with three correction calculations on three distinct Y columns:
/// `(project, [(identifier of the calculation, operation word written in the file)])`
/// in increasing order of identifiers.
fn project_with_three_calculations() -> (Project, Vec<(u64, &'static str)>) {
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let mut made = Vec::new();
    for (name, op, word) in [
        ("Y1", CorrectOp::Subtract, "subtract"),
        ("Y2", CorrectOp::Divide, "divide"),
        ("Y3", CorrectOp::Multiply, "multiply"),
    ] {
        let y = project
            .add_column(sheet, None, Some(name), Column::from_f64(&[1.0, 2.0]))
            .unwrap();
        let id = project
            .add_calculation(CalculationDraft {
                operation: Operation::Correct { op },
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
                    data: vec![3.0, 4.0],
                }],
                results: vec![SeriesResult::default()],
                keep_results: true,
                auto_recalc: false,
                low_r2_dialog: false,
            })
            .unwrap();
        made.push((id.raw(), word));
    }
    (project, made)
}

/// Replaces, in the bytes of the file, `from` by `to` (same length).
fn patch(path: &std::path::Path, from: &[u8], to: &[u8]) {
    assert_eq!(from.len(), to.len());
    let mut bytes = std::fs::read(path).unwrap();
    let at = bytes
        .windows(from.len())
        .position(|w| w == from)
        .expect("the pattern is in the file");
    bytes[at..at + to.len()].copy_from_slice(to);
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn several_discarded_calculations_are_all_listed_in_order_and_the_sound_ones_load() {
    let (project, made) = project_with_three_calculations();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    save(&project, &path).unwrap();
    patch(&path, b"subtract", b"subtraXX");
    patch(&path, b"multiply", b"multipXX");

    let expected = vec![
        stokesforge_persistence::DiscardedCalculation {
            id: made[0].0,
            kind: Some("correct".into()),
        },
        stokesforge_persistence::DiscardedCalculation {
            id: made[2].0,
            kind: Some("correct".into()),
        },
    ];
    // Same report at each read: the order depends on nothing random.
    for _ in 0..3 {
        let (loaded, _, report) = stokesforge_persistence::load_with_report(&path).unwrap();
        assert_eq!(report.discarded_calculations, expected);
        assert!(!report.is_clean());
        let kept: Vec<u64> = loaded
            .tree()
            .calculations()
            .keys()
            .map(|k| k.raw())
            .collect();
        assert_eq!(kept, vec![made[1].0], "only the sound one is kept");
    }
    // `load` and `load_with_geometries` keep their behavior: they
    // load the same project, with no report and no failure.
    let by_load = load(&path).unwrap();
    let (by_geometries, _) = stokesforge_persistence::load_with_geometries(&path).unwrap();
    let (by_report, _, _) = stokesforge_persistence::load_with_report(&path).unwrap();
    assert_eq!(by_load.tree(), by_report.tree());
    assert_eq!(by_geometries.tree(), by_report.tree());
    // The output columns of a discarded calculation stay ordinary columns.
    assert_eq!(by_report.tree().len(), project.tree().len());
}

#[test]
fn a_file_without_a_discarded_calculation_gives_an_empty_report() {
    let (project, made) = project_with_three_calculations();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    save(&project, &path).unwrap();
    let (loaded, _, report) = stokesforge_persistence::load_with_report(&path).unwrap();
    assert!(report.is_clean());
    assert_eq!(report, stokesforge_persistence::LoadReport::default());
    assert_eq!(loaded.tree().calculations().len(), made.len());
    // A project without any calculation too.
    let empty = Project::new("Empty").unwrap();
    save(&empty, &path).unwrap();
    assert!(stokesforge_persistence::load_with_report(&path)
        .unwrap()
        .2
        .is_clean());
}

/// A NaN written by hand in a baseline point must not enter
/// the model (it would be lost on re-save): the calculation is discarded
/// **and** reported, like any unreadable record.
#[test]
fn a_nan_edited_by_hand_in_the_file_discards_and_reports_the_calculation() {
    use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let y = project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&[1.0, 2.0]))
        .unwrap();
    let id = project
        .add_calculation(CalculationDraft {
            operation: Operation::Baseline {
                method: BaselineMethod::Constant {
                    point: (1234.5, 6.25),
                },
                op: BaselineOp::Subtract,
                keep_baseline: false,
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
                data: vec![0.0, 1.0],
            }],
            results: vec![SeriesResult::default()],
            keep_results: true,
            auto_recalc: false,
            low_r2_dialog: false,
        })
        .unwrap()
        .raw();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    save(&project, &path).unwrap();
    let mut from = vec![0xcb];
    from.extend_from_slice(&1234.5_f64.to_be_bytes());
    let mut to = vec![0xcb];
    to.extend_from_slice(&f64::NAN.to_be_bytes());
    patch(&path, &from, &to);

    let (loaded, _, report) = stokesforge_persistence::load_with_report(&path).unwrap();
    assert!(
        loaded.tree().calculations().is_empty(),
        "a NaN must not enter the model"
    );
    assert_eq!(
        report.discarded_calculations,
        vec![stokesforge_persistence::DiscardedCalculation {
            id,
            kind: Some("baseline".into())
        }]
    );
}

/// #309: an unreadable correction trace is not lost silently: the
/// report counts it (`discarded_corrections`), the rest loads.
#[test]
fn an_unreadable_correction_trace_is_counted_by_the_report() {
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let y = project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&[10.0, 20.0]))
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
        .unwrap()
        .raw();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    save(&project, &path).unwrap();
    let clean = stokesforge_persistence::load_with_report(&path).unwrap();
    assert!(clean.2.is_clean());
    assert_eq!(clean.0.tree().correction_traces().len(), 1);

    patch(&path, b"subtract", b"subtraXX");
    let (loaded, _, report) = stokesforge_persistence::load_with_report(&path).unwrap();
    assert!(loaded.tree().correction_traces().is_empty());
    assert!(report.discarded_calculations.is_empty());
    assert_eq!(
        report.discarded_corrections,
        vec![stokesforge_persistence::DiscardedCalculation {
            id: trace,
            kind: Some("correct".into())
        }]
    );
    assert!(!report.is_clean());
}

/// A1 (#316): the fingerprint of the content of the inputs, written at save,
/// is compared at read without recomputing anything.
#[test]
fn inputs_modified_in_the_file_make_the_calculation_stale_without_recalculating_it() {
    let (project, id) = {
        let project = project_with_calculation();
        let id = *project.tree().calculations().keys().next().unwrap();
        (project, id)
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    save(&project, &path).unwrap();

    // Intact: nothing stale.
    let loaded = load(&path).unwrap();
    assert_eq!(loaded.tree().calculation(id).unwrap().stale, None);

    // Y becomes [10, 20, 31] outside StokesForge (same length, no shift).
    let float_bytes = |v: &[f64]| -> Vec<u8> { v.iter().flat_map(|x| x.to_le_bytes()).collect() };
    let (before, after) = (
        float_bytes(&[10.0, 20.0, 30.0]),
        float_bytes(&[10.0, 20.0, 31.0]),
    );
    let mut bytes = std::fs::read(&path).unwrap();
    let at = bytes
        .windows(before.len())
        .position(|w| w == before.as_slice())
        .expect("Y is in the file");
    bytes[at..at + before.len()].copy_from_slice(&after);
    std::fs::write(&path, &bytes).unwrap();

    let loaded = load(&path).unwrap();
    let calc = loaded.tree().calculation(id).unwrap();
    assert_eq!(calc.stale, Some(Staleness::Manual));
    // The output values are those of the file: nothing was recomputed.
    let out = calc.outputs[0].column.unwrap();
    let values = loaded
        .tree()
        .node(out)
        .unwrap()
        .as_column()
        .unwrap()
        .numeric_view()
        .unwrap()
        .to_vec();
    assert_eq!(values[..2], [9.0, 18.0]);
}
