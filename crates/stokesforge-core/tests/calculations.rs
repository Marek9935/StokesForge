//! The analysis calculations of the model (#181): creation of columns at the right
//! place, in-place update, removal, writes outside history, tree equality
//! despite `NaN`s.

use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::ops::Operation;
use stokesforge_core::{
    CalculationDraft, Column, ColumnMeta, CoreError, NodeId, OutputDraft, Project, SeriesRef,
    SeriesResult, Staleness,
};

struct Fixture {
    project: Project,
    sheet: NodeId,
    x: NodeId,
    y1: NodeId,
    y2: NodeId,
}

fn fixture() -> Fixture {
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let x = project
        .add_column(sheet, None, Some("X"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    let y1 = project
        .add_column(
            sheet,
            None,
            Some("Y1"),
            Column::from_f64(&[10.0, 20.0, 30.0]),
        )
        .unwrap();
    let y2 = project
        .add_column(sheet, None, Some("Y2"), Column::from_f64(&[5.0, 6.0, 7.0]))
        .unwrap();
    Fixture {
        project,
        sheet,
        x,
        y1,
        y2,
    }
}

fn names(f: &Fixture) -> Vec<String> {
    f.project
        .tree()
        .children(f.sheet)
        .unwrap()
        .iter()
        .map(|c| f.project.tree().get(*c).unwrap().name().to_string())
        .collect()
}

fn out(series: usize, key: &str, data: Vec<f64>, long: &str) -> OutputDraft {
    OutputDraft {
        series,
        key: key.to_string(),
        index: None,
        meta: ColumnMeta {
            long_name: long.to_string(),
            ..Default::default()
        },
        data,
    }
}

fn draft(f: &Fixture, outputs: Vec<OutputDraft>) -> CalculationDraft {
    CalculationDraft {
        operation: Operation::Correct {
            op: CorrectOp::Divide,
        },
        series: vec![
            SeriesRef {
                x: Some(f.x),
                y: f.y1,
                sigma: None,
            },
            SeriesRef {
                x: Some(f.x),
                y: f.y2,
                sigma: None,
            },
        ],
        aux: None,
        outputs,
        results: vec![SeriesResult::default(), SeriesResult::default()],
        keep_results: true,
        auto_recalc: true,
        low_r2_dialog: true,
    }
}

#[test]
fn each_output_column_follows_its_own_y_column() {
    let mut f = fixture();
    let d = draft(
        &f,
        vec![
            out(0, "corrected", vec![1.0, 2.0, 3.0], "Y1 (corr.)"),
            out(0, "fit", vec![4.0, 5.0, 6.0], "Y1 (fit)"),
            out(1, "corrected", vec![7.0, 8.0, 9.0], "Y2 (corr.)"),
        ],
    );
    let id = f.project.add_calculation(d).unwrap();
    // X, Y1, [2 outputs of Y1], Y2, [1 output of Y2]; free automatic names.
    assert_eq!(f.project.tree().children(f.sheet).unwrap().len(), 6);
    let kids = f.project.tree().children(f.sheet).unwrap().to_vec();
    assert_eq!(kids[0], f.x);
    assert_eq!(kids[1], f.y1);
    assert_eq!(kids[4], f.y2);
    let calc = f.project.tree().calculation(id).unwrap().clone();
    assert_eq!(calc.outputs[0].column, Some(kids[2]));
    assert_eq!(calc.outputs[1].column, Some(kids[3]));
    assert_eq!(calc.outputs[2].column, Some(kids[5]));
    // The trace is read from the column.
    assert_eq!(
        f.project.tree().calculation_of_column(kids[3]),
        Some((id, 1))
    );
    assert_eq!(f.project.tree().calculation_of_column(f.y1), None);
    let col = f.project.tree().get(kids[2]).unwrap().as_column().unwrap();
    assert_eq!(col.meta().long_name, "Y1 (corr.)");
    assert_eq!(col.numeric_view().unwrap().as_slice(), &[1.0, 2.0, 3.0]);
    assert_eq!(names(&f).len(), 6);
}

#[test]
fn a_failure_leaves_the_tree_intact() {
    let mut f = fixture();
    let before = f.project.tree().clone();
    // Unknown series.
    let mut d = draft(&f, vec![out(0, "corrected", vec![1.0], "a")]);
    d.series[0].y = NodeId::from_raw(99_999);
    assert!(f.project.add_calculation(d).is_err());
    // Output that targets a missing series.
    let d = draft(&f, vec![out(5, "corrected", vec![1.0], "a")]);
    assert!(matches!(
        f.project.add_calculation(d),
        Err(CoreError::Structure(_))
    ));
    // No series.
    let mut d = draft(&f, vec![]);
    d.series.clear();
    d.results.clear();
    assert!(f.project.add_calculation(d).is_err());
    assert_eq!(f.project.tree(), &before);
}

/// A non-finite operation is not recorded (#278): it would be
/// written then lost when the `.stf` is reread.
#[test]
fn a_non_finite_operation_is_refused_on_add_and_on_update() {
    use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
    let nan_baseline = || Operation::Baseline {
        method: BaselineMethod::Constant {
            point: (f64::NAN, 1.0),
        },
        op: BaselineOp::Subtract,
        keep_baseline: false,
    };
    let mut f = fixture();
    let before = f.project.tree().clone();
    let mut d = draft(&f, vec![out(0, "corrected", vec![1.0], "a")]);
    d.operation = nan_baseline();
    assert!(matches!(
        f.project.add_calculation(d),
        Err(CoreError::Structure(_))
    ));
    assert_eq!(f.project.tree(), &before);

    let id = f
        .project
        .add_calculation(draft(&f, vec![out(0, "corrected", vec![1.0], "a")]))
        .unwrap();
    let before = f.project.tree().clone();
    let mut d = draft(&f, vec![out(0, "corrected", vec![1.0], "a")]);
    d.operation = nan_baseline();
    assert!(matches!(
        f.project.update_calculation(id, d),
        Err(CoreError::Structure(_))
    ));
    assert_eq!(f.project.tree(), &before);
}

/// A non-finite operation per family and per field: `(name expected in
/// the error, operation)`.
fn non_finite_operations() -> Vec<(&'static str, Operation)> {
    use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
    use stokesforge_analysis::deconv::{DirectMethod, ReconvSpec};
    use stokesforge_analysis::fit::FitSpec;
    use stokesforge_analysis::integrate::{IntegrateSpec, Under};
    use stokesforge_analysis::model::Model;
    use stokesforge_analysis::ops::AutoDetect;
    use stokesforge_analysis::peaks::{PeakBackground, PeakSeed, PeakShape, PeaksSpec};
    let base = |method| Operation::Baseline {
        method,
        op: BaselineOp::Subtract,
        keep_baseline: false,
    };
    let peaks = |seeds: Vec<PeakSeed>, range, prominence| Operation::Peaks {
        spec: PeaksSpec {
            shape: PeakShape::Gaussian,
            background: PeakBackground::Constant,
            seeds,
            link_widths: false,
            center_window: None,
        },
        auto: Some(AutoDetect {
            min_prominence: prominence,
            max_peaks: 3,
        }),
        range,
        residuals: false,
    };
    let nan = f64::NAN;
    let inf = f64::INFINITY;
    vec![
        (
            "point",
            base(BaselineMethod::Constant { point: (nan, 1.0) }),
        ),
        (
            "zone",
            base(BaselineMethod::ConstantZone { zone: (1.0, inf) }),
        ),
        (
            "points",
            base(BaselineMethod::Line {
                p1: (0.0, 1.0),
                p2: (nan, 2.0),
            }),
        ),
        // Unplaced anchors are ignored, not refused (#298).
        (
            "zones",
            base(BaselineMethod::Zones {
                zones: vec![(0.0, 1.0), (nan, 3.0)],
                degree: 1,
            }),
        ),
        (
            "range",
            Operation::Fit {
                spec: FitSpec {
                    model: Model::Linear,
                    params: Vec::new(),
                },
                range: Some((0.0, nan)),
                residuals: false,
            },
        ),
        (
            "ranges",
            Operation::Integrate {
                spec: IntegrateSpec {
                    ranges: vec![(nan, 1.0)],
                    under: Under::Zero,
                    cumulative: false,
                    normalize: false,
                },
            },
        ),
        (
            "seeds",
            peaks(
                vec![PeakSeed {
                    center: nan,
                    height: None,
                    fwhm: None,
                }],
                None,
                0.1,
            ),
        ),
        ("range", peaks(Vec::new(), Some((inf, 1.0)), 0.1)),
        ("min_prominence", peaks(Vec::new(), None, nan)),
        (
            "window",
            Operation::Reconvolution {
                spec: ReconvSpec {
                    components: 1,
                    params: Vec::new(),
                    shift_initial: 0.0,
                    fit_shift: false,
                    window: Some((nan, 1.0)),
                },
                residuals: false,
            },
        ),
        (
            "shift",
            Operation::DirectDeconvolution {
                method: DirectMethod::RichardsonLucy { iterations: 5 },
                shift: nan,
            },
        ),
    ]
}

/// Each family and each non-finite field (#278): `add_calculation`,
/// `update_calculation` and `replace_analysis_data` refuses with a
/// `Structure` that names the field, without touching the tree or consuming
/// an identifier.
#[test]
fn every_non_finite_operation_is_refused_without_touching_the_tree() {
    let mut f = fixture();
    let id = f
        .project
        .add_calculation(draft(&f, vec![out(0, "corrected", vec![1.0], "a")]))
        .unwrap();
    let versions: Vec<(NodeId, u64)> = [f.x, f.y1, f.y2]
        .into_iter()
        .map(|c| {
            let v = f
                .project
                .tree()
                .get(c)
                .unwrap()
                .as_column()
                .unwrap()
                .version();
            (c, v)
        })
        .collect();
    let refuses = |e: CoreError, name: &str| match e {
        CoreError::Structure(message) => {
            assert!(message.contains(&format!("'{name}'")), "{message}")
        }
        other => panic!("expected Structure, got {other:?}"),
    };
    for (name, operation) in non_finite_operations() {
        let before = f.project.tree().clone();
        let next = f.project.next_id_hint();
        let mut d = draft(&f, vec![out(0, "corrected", vec![1.0], "a")]);
        d.operation = operation.clone();
        refuses(f.project.add_calculation(d.clone()).unwrap_err(), name);
        refuses(
            f.project.update_calculation(id, d.clone()).unwrap_err(),
            name,
        );
        // Putting the data back in place validates the operation first: same
        // refusal, before any version or input check.
        refuses(
            f.project.replace_analysis_data(d, &versions).unwrap_err(),
            name,
        );
        assert_eq!(f.project.tree(), &before, "{name}");
        assert_eq!(f.project.next_id_hint(), next, "{name}: no id consumed");
    }
}

/// The finiteness check does not reject the ordinary: a finite operation
/// is added, updated, and the extreme finite value passes.
#[test]
fn a_finite_operation_is_added_and_updated() {
    use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
    let mut f = fixture();
    let constant = |v: f64| Operation::Baseline {
        method: BaselineMethod::Constant { point: (v, -v) },
        op: BaselineOp::Subtract,
        keep_baseline: false,
    };
    let mut d = draft(&f, vec![out(0, "corrected", vec![1.0], "a")]);
    d.operation = constant(f64::MAX);
    let id = f.project.add_calculation(d.clone()).unwrap();
    d.operation = constant(0.5);
    f.project.update_calculation(id, d).unwrap();
    assert_eq!(
        f.project.tree().calculation(id).unwrap().operation,
        constant(0.5)
    );
}

#[test]
fn calculation_ids_are_never_reused() {
    let mut f = fixture();
    let a = f.project.add_calculation(draft(&f, vec![])).unwrap();
    f.project.remove_calculation(a, false).unwrap();
    let b = f.project.add_calculation(draft(&f, vec![])).unwrap();
    assert_ne!(a, b);
    assert!(b.raw() > a.raw());
    // A reloaded project does not reassign a calculation's identifier either.
    let tree = f.project.tree().clone();
    let mut reloaded = Project::from_tree(tree, 0);
    let c = reloaded.add_calculation(draft(&f, vec![])).unwrap();
    assert!(c.raw() > b.raw(), "{c:?} must exceed {b:?}");
}

#[test]
fn an_update_rewrites_in_place_creates_and_deletes() {
    let mut f = fixture();
    let id = f
        .project
        .add_calculation(draft(
            &f,
            vec![
                out(0, "corrected", vec![1.0, 2.0, 3.0], "c"),
                out(0, "fit", vec![4.0, 5.0, 6.0], "f"),
            ],
        ))
        .unwrap();
    let kids = f.project.tree().children(f.sheet).unwrap().to_vec();
    let (corrected, fit) = (kids[2], kids[3]);
    // "fit" disappears, "residuals" appears, "corrected" is rewritten.
    f.project
        .update_calculation(
            id,
            draft(
                &f,
                vec![
                    out(0, "corrected", vec![9.0, 9.0, 9.0], "c2"),
                    out(0, "residuals", vec![0.5, 0.5, 0.5], "r"),
                ],
            ),
        )
        .unwrap();
    assert!(
        f.project.tree().node(fit).is_none(),
        "column removed with its output"
    );
    let calc = f.project.tree().calculation(id).unwrap().clone();
    assert_eq!(
        calc.outputs[0].column,
        Some(corrected),
        "rewritten in place, same column"
    );
    let col = f
        .project
        .tree()
        .get(corrected)
        .unwrap()
        .as_column()
        .unwrap();
    assert_eq!(col.numeric_view().unwrap().as_slice(), &[9.0, 9.0, 9.0]);
    assert_eq!(col.meta().long_name, "c2");
    let res = calc.outputs[1].column.unwrap();
    assert_eq!(f.project.tree().calculation_of_column(res), Some((id, 1)));
    assert_eq!(f.project.tree().children(f.sheet).unwrap().len(), 5);
}

#[test]
fn removing_a_calculation_keeps_or_deletes_its_columns() {
    let mut f = fixture();
    let id = f
        .project
        .add_calculation(draft(
            &f,
            vec![out(0, "corrected", vec![1.0, 2.0, 3.0], "c")],
        ))
        .unwrap();
    let produced = f.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    f.project.remove_calculation(id, false).unwrap();
    assert!(f.project.tree().calculation(id).is_none());
    assert!(
        f.project.tree().node(produced).is_some(),
        "detached: ordinary column"
    );
    assert_eq!(f.project.tree().calculation_of_column(produced), None);

    let id = f
        .project
        .add_calculation(draft(
            &f,
            vec![out(0, "corrected", vec![1.0, 2.0, 3.0], "c")],
        ))
        .unwrap();
    let produced = f.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    f.project.remove_calculation(id, true).unwrap();
    assert!(f.project.tree().node(produced).is_none());
    assert!(f.project.remove_calculation(id, true).is_err());
}

#[test]
fn a_refresh_rewrites_only_what_changes() {
    let mut f = fixture();
    let id = f
        .project
        .add_calculation(draft(
            &f,
            vec![out(0, "corrected", vec![1.0, f64::NAN, 3.0], "c")],
        ))
        .unwrap();
    let produced = f.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    let v0 = f
        .project
        .tree()
        .get(produced)
        .unwrap()
        .as_column()
        .unwrap()
        .version();
    // Same values (NaN included): no write, hence no cascade.
    f.project
        .refresh_calculation(id, &[Some(vec![1.0, f64::NAN, 3.0])], vec![])
        .unwrap();
    assert_eq!(
        f.project
            .tree()
            .get(produced)
            .unwrap()
            .as_column()
            .unwrap()
            .version(),
        v0
    );
    // Different values: written, fresh version.
    f.project
        .refresh_calculation(id, &[Some(vec![1.0, 2.0, 3.0, 4.0])], vec![])
        .unwrap();
    let col = f.project.tree().get(produced).unwrap().as_column().unwrap();
    assert!(col.version() > v0);
    assert_eq!(col.numeric_view().unwrap().len(), 4);
    // `None` leaves the column.
    f.project.refresh_calculation(id, &[None], vec![]).unwrap();
    assert_eq!(
        f.project
            .tree()
            .get(produced)
            .unwrap()
            .as_column()
            .unwrap()
            .numeric_view()
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn the_stale_state_is_set_and_cleared() {
    let mut f = fixture();
    let id = f.project.add_calculation(draft(&f, vec![])).unwrap();
    f.project
        .set_calculation_stale(id, Some(Staleness::Manual))
        .unwrap();
    assert_eq!(
        f.project.tree().calculation(id).unwrap().stale,
        Some(Staleness::Manual)
    );
    f.project.refresh_calculation(id, &[], vec![]).unwrap();
    assert_eq!(f.project.tree().calculation(id).unwrap().stale, None);
    assert!(f
        .project
        .set_calculation_stale(NodeId::from_raw(424_242), None)
        .is_err());
}

#[test]
fn nan_results_do_not_break_tree_equality() {
    let mut f = fixture();
    let mut d = draft(&f, vec![]);
    d.results = vec![
        SeriesResult {
            values: vec![stokesforge_core::ResultValue {
                key: "stat.r_squared".into(),
                index: None,
                value: f64::NAN,
                error: None,
            }],
            ..Default::default()
        },
        SeriesResult::default(),
    ];
    let id = f.project.add_calculation(d).unwrap();
    let a = f.project.tree().clone();
    let calc = f.project.tree().calculation(id).unwrap().as_ref().clone();
    f.project.restore_calculation(id, Some(calc));
    assert_eq!(&a, f.project.tree(), "a NaN equals a NaN in a result");
}

#[test]
fn without_keeping_results_nothing_is_stored() {
    let mut f = fixture();
    let mut d = draft(&f, vec![out(0, "corrected", vec![1.0, 2.0, 3.0], "c")]);
    d.keep_results = false;
    d.results = vec![
        SeriesResult {
            skipped: 3,
            ..Default::default()
        },
        SeriesResult::default(),
    ];
    let id = f.project.add_calculation(d).unwrap();
    let calc = f.project.tree().calculation(id).unwrap();
    assert!(calc.results.is_empty() && !calc.keep_results);
    f.project
        .refresh_calculation(
            id,
            &[None],
            vec![SeriesResult {
                skipped: 9,
                ..Default::default()
            }],
        )
        .unwrap();
    assert!(f.project.tree().calculation(id).unwrap().results.is_empty());
}

#[test]
fn deleting_the_inputs_removes_the_orphan_calculation_but_keeps_a_partial_one() {
    let mut f = fixture();
    let id = f
        .project
        .add_calculation(draft(
            &f,
            vec![out(0, "corrected", vec![1.0, 2.0, 3.0], "c")],
        ))
        .unwrap();
    // Only one of the two series loses its Y: the calculation stays.
    f.project.remove(f.y2).unwrap();
    assert!(f.project.tree().calculation(id).is_some());
    // The second one too: nothing left to compute.
    f.project.remove(f.y1).unwrap();
    assert!(f.project.tree().calculation(id).is_none());
}

#[test]
fn restoring_the_tree_from_before_a_deletion_brings_the_calculation_back() {
    let mut f = fixture();
    let id = f
        .project
        .add_calculation(draft(
            &f,
            vec![out(0, "corrected", vec![1.0, 2.0, 3.0], "c")],
        ))
        .unwrap();
    let before = f.project.tree().clone();
    f.project.remove(f.y2).unwrap();
    f.project.remove(f.y1).unwrap();
    assert!(f.project.tree().calculation(id).is_none());
    f.project.replace_tree(before.clone());
    assert_eq!(f.project.tree(), &before);
    assert!(f.project.tree().calculation(id).is_some());
}

// ── compute: the model read shared by creation and recalculation ──

use stokesforge_core::{compute, AuxRef, ComputeError};

#[test]
fn compute_reads_the_columns_and_aligns_the_outputs_on_the_rows() {
    let f = fixture();
    // Reference: constant Y = 2 on the same X → Divide gives y / 2.
    let mut project = f.project;
    let rx = project
        .add_column(
            f.sheet,
            None,
            Some("RX"),
            Column::from_f64(&[1.0, 2.0, 3.0]),
        )
        .unwrap();
    let ry = project
        .add_column(
            f.sheet,
            None,
            Some("RY"),
            Column::from_f64(&[2.0, 2.0, 2.0]),
        )
        .unwrap();
    let computed = compute(
        project.tree(),
        &Operation::Correct {
            op: CorrectOp::Divide,
        },
        &[SeriesRef {
            x: Some(f.x),
            y: f.y1,
            sigma: None,
        }],
        Some(&AuxRef { x: Some(rx), y: ry }),
    )
    .unwrap();
    assert_eq!(computed.columns.len(), 1);
    assert_eq!(computed.columns[0].key, "corrected");
    assert_eq!(computed.columns[0].data, vec![5.0, 10.0, 15.0]);
    assert_eq!(computed.results.len(), 1);
}

#[test]
fn compute_names_the_vanished_input_column() {
    let mut f = fixture();
    f.project.remove(f.y1).unwrap();
    let err = compute(
        f.project.tree(),
        &Operation::Correct {
            op: CorrectOp::Divide,
        },
        &[SeriesRef {
            x: Some(f.x),
            y: f.y1,
            sigma: None,
        }],
        None,
    )
    .unwrap_err();
    assert_eq!(err, ComputeError::MissingInput(f.y1));
}

#[test]
fn refresh_says_whether_it_wrote_something() {
    let mut f = fixture();
    let id = f
        .project
        .add_calculation(draft(
            &f,
            vec![out(0, "corrected", vec![1.0, 2.0, 3.0], "C")],
        ))
        .unwrap();
    let same = f
        .project
        .refresh_calculation(
            id,
            &[Some(vec![1.0, 2.0, 3.0])],
            vec![SeriesResult::default(), SeriesResult::default()],
        )
        .unwrap();
    assert!(!same, "same data, nothing written");
    let diff = f
        .project
        .refresh_calculation(
            id,
            &[Some(vec![1.0, 2.0, 4.0])],
            vec![SeriesResult::default(), SeriesResult::default()],
        )
        .unwrap();
    assert!(diff);
}

/// A graph that plots the output column of a calculation and the source column.
fn graph_of_both(f: &mut Fixture, stale_warning: bool) -> (NodeId, NodeId) {
    let id = f
        .project
        .add_calculation(draft(
            f,
            vec![out(0, "corrected", vec![1.0, 2.0, 3.0], "Y1 (corr.)")],
        ))
        .unwrap();
    let produced = f.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    let mut spec = stokesforge_plot::PlotSpec {
        stale_warning,
        ..Default::default()
    };
    let panel = spec.panels.get_mut(0).unwrap();
    panel
        .series
        .push(stokesforge_plot::SeriesSpec::new(produced));
    panel.series.push(stokesforge_plot::SeriesSpec::new(f.y2));
    f.project.add_graph(f.project.root(), None, spec).unwrap();
    (id, produced)
}

fn flags(f: &Fixture) -> Vec<bool> {
    let graph = f.project.tree().children(f.project.root()).unwrap()[1];
    let stokesforge_core::NodeKind::Graph { spec, .. } =
        f.project.tree().node(graph).unwrap().kind()
    else {
        panic!("a graph was expected");
    };
    stokesforge_core::resolve_spec(f.project.tree(), spec).panels[0]
        .series
        .iter()
        .map(|s| s.stale)
        .collect()
}

#[test]
fn a_series_plotted_from_a_stale_calculation_is_flagged_and_the_graph_can_opt_out() {
    let mut f = fixture();
    let (id, _) = graph_of_both(&mut f, true);
    assert_eq!(flags(&f), [false, false], "up-to-date calculation: nothing");
    f.project
        .set_calculation_stale(id, Some(Staleness::Manual))
        .unwrap();
    assert_eq!(flags(&f), [true, false], "only the output is flagged");
    f.project.refresh_calculation(id, &[None], vec![]).unwrap();
    assert_eq!(flags(&f), [false, false], "recalculated: no more flag");

    // The graph that disabled the warning no longer flags anything.
    let mut g = fixture();
    let (id, _) = graph_of_both(&mut g, false);
    g.project
        .set_calculation_stale(id, Some(Staleness::Manual))
        .unwrap();
    assert_eq!(flags(&g), [false, false]);
}

// ── Columns of different lengths (C4, #313) ─────────────────────────

/// A workbook with X, Y of given lengths, and a constant reference = 2 over
/// a wide domain: `Correct`/`Divide` gives y / 2.
fn unequal(x: &[f64], y: &[f64]) -> (Project, NodeId, SeriesRef, AuxRef) {
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let mut add = |name: &str, v: &[f64]| {
        project
            .add_column(sheet, None, Some(name), Column::from_f64(v))
            .unwrap()
    };
    let cx = add("X", x);
    let cy = add("Y", y);
    let rx = add("RX", &[-100.0, 100.0]);
    let ry = add("RY", &[2.0, 2.0]);
    (
        project,
        cy,
        SeriesRef {
            x: Some(cx),
            y: cy,
            sigma: None,
        },
        AuxRef { x: Some(rx), y: ry },
    )
}

fn corrects(project: &Project, series: SeriesRef, aux: &AuxRef) -> Vec<f64> {
    let computed = compute(
        project.tree(),
        &Operation::Correct {
            op: CorrectOp::Divide,
        },
        &[series],
        Some(aux),
    )
    .unwrap();
    computed.columns[0].data.clone()
}

#[test]
fn the_output_has_the_length_of_x_when_x_is_given() {
    // X shorter than Y: the rows of Y beyond X have no abscissa.
    let (p, _, s, aux) = unequal(&[1.0, 2.0, 3.0], &[2.0, 4.0, 6.0, 8.0, 10.0]);
    assert_eq!(corrects(&p, s, &aux), vec![1.0, 2.0, 3.0]);
    // X longer than Y: the missing rows of Y are empty (NaN).
    let (p, _, s, aux) = unequal(&[1.0, 2.0, 3.0, 4.0, 5.0], &[2.0, 4.0, 6.0]);
    let data = corrects(&p, s, &aux);
    assert_eq!(data.len(), 5);
    assert_eq!(&data[..3], &[1.0, 2.0, 3.0]);
    assert!(data[3].is_nan() && data[4].is_nan());
}

#[test]
fn without_x_the_output_has_the_length_of_y() {
    let (p, _, mut s, aux) = unequal(&[1.0, 2.0], &[2.0, 4.0, 6.0, 8.0]);
    s.x = None;
    assert_eq!(corrects(&p, s, &aux), vec![1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn replacement_with_y_longer_than_x_keeps_the_cells_beyond_x() {
    let (mut p, y, s, aux) = unequal(&[1.0, 2.0, 3.0], &[2.0, 4.0, 6.0, 8.0, 10.0]);
    let data = corrects(&p, s.clone(), &aux);
    let versions: Vec<(NodeId, u64)> = [s.x.unwrap(), y, aux.y, aux.x.unwrap()]
        .into_iter()
        .map(|c| (c, p.tree().get(c).unwrap().as_column().unwrap().version()))
        .collect();
    let mut draft = CalculationDraft {
        operation: Operation::Correct {
            op: CorrectOp::Divide,
        },
        series: vec![s],
        aux: Some(aux),
        outputs: vec![out(0, "corrected", data, "c")],
        results: vec![SeriesResult::default()],
        keep_results: false,
        auto_recalc: false,
        low_r2_dialog: false,
    };
    // Without completion, the core's length guard refuses the short output.
    assert!(matches!(
        p.replace_analysis_data(draft.clone(), &versions),
        Err(CoreError::Structure(_))
    ));
    draft.complete_for_replacement(p.tree());
    assert_eq!(draft.outputs[0].data.len(), 5);
    p.replace_analysis_data(draft, &versions).unwrap();
    let col = p.tree().get(y).unwrap().as_column().unwrap();
    let got: Vec<f64> = (0..5).map(|r| col.values().number_at(r)).collect();
    // The first three rows are corrected, the other two keep the original.
    assert_eq!(got, vec![1.0, 2.0, 3.0, 8.0, 10.0]);
}

/// #324: a smoothing replaces the Ys with its `smoothed` output (like a
/// correction with `corrected`); a `corrected` output does not suit it.
#[test]
fn a_smoothing_replaces_the_ys_with_its_smoothed_output() {
    use stokesforge_analysis::smooth::{SmoothMethod, SmoothSpec, SmoothWindow};
    let mut f = fixture();
    let versions: Vec<(NodeId, u64)> = [f.x, f.y1]
        .into_iter()
        .map(|c| {
            (
                c,
                f.project
                    .tree()
                    .get(c)
                    .unwrap()
                    .as_column()
                    .unwrap()
                    .version(),
            )
        })
        .collect();
    let smooth = |key: &str| CalculationDraft {
        operation: Operation::Smooth {
            spec: SmoothSpec {
                method: SmoothMethod::AdjacentAverage,
                window: SmoothWindow::Points(3),
            },
        },
        series: vec![SeriesRef {
            x: Some(f.x),
            y: f.y1,
            sigma: None,
        }],
        aux: None,
        outputs: vec![out(0, key, vec![12.0, 20.0, 28.0], "Y1 (smoothed)")],
        results: vec![SeriesResult::default()],
        keep_results: false,
        auto_recalc: false,
        low_r2_dialog: false,
    };
    assert!(matches!(
        f.project
            .replace_analysis_data(smooth("corrected"), &versions),
        Err(CoreError::Structure(_))
    ));
    f.project
        .replace_analysis_data(smooth("smoothed"), &versions)
        .unwrap();
    let column = f.project.tree().get(f.y1).unwrap().as_column().unwrap();
    let got: Vec<f64> = (0..3).map(|r| column.values().number_at(r)).collect();
    assert_eq!(got, vec![12.0, 20.0, 28.0]);
}
