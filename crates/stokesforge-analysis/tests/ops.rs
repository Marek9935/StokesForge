//! Contract of `ops::run`: outputs aligned on the input rows, dropped rows
//! counted, a batch processed series by series, global fit.

use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp, Interpolation};
use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::deconv::{DirectMethod, ReconvSpec};
use stokesforge_analysis::fit::{FitSpec, ParamSetting};
use stokesforge_analysis::integrate::{IntegrateSpec, Under};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{
    run, AutoDetect, AuxInput, Operation, OutColumn, Outcome, SeriesInput, Value, Warning,
};
use stokesforge_analysis::peaks::{PeakBackground, PeakShape, PeaksSpec};
use stokesforge_analysis::AnalysisError;

fn col<'a>(o: &'a Outcome, key: &str) -> &'a OutColumn {
    o.columns.iter().find(|c| c.key == key).unwrap_or_else(|| {
        panic!(
            "column '{key}' missing: {:?}",
            o.columns.iter().map(|c| c.key).collect::<Vec<_>>()
        )
    })
}

fn val<'a>(o: &'a Outcome, key: &str) -> &'a Value {
    o.values
        .iter()
        .find(|v| v.key == key)
        .unwrap_or_else(|| panic!("value '{key}' missing"))
}

fn line(n: usize, f: impl Fn(f64) -> f64) -> SeriesInput {
    let x: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let y = x.iter().map(|&v| f(v)).collect();
    SeriesInput { x, y, sigma: None }
}

#[test]
fn the_baseline_keeps_the_source_length_and_counts_the_dropped_rows() {
    let mut inp = line(11, |x| 2.0 + 0.5 * x);
    inp.y[3] = f64::NAN; // empty cell
    inp.x[7] = f64::NAN; // missing X
    let op = Operation::Baseline {
        method: BaselineMethod::Line {
            p1: (0.0, 2.0),
            p2: (10.0, 7.0),
        },
        op: BaselineOp::Subtract,
        keep_baseline: true,
    };
    let out = run(&op, &[inp], None).unwrap();
    let o = &out[0];
    assert_eq!(o.skipped, 2);
    let corrected = &col(o, "corrected").data;
    let base = &col(o, "baseline").data;
    assert_eq!(corrected.len(), 11);
    assert!(corrected[3].is_nan() && corrected[7].is_nan());
    for i in [0usize, 1, 2, 4, 5, 6, 8, 9, 10] {
        assert!(corrected[i].abs() < 1e-12, "row {i}: {}", corrected[i]);
    }
    // The baseline reads even where Y is empty, as long as X is known.
    assert!((base[3] - 3.5).abs() < 1e-12);
    assert!(base[7].is_nan());
    assert!((val(o, "baseline.slope").value - 0.5).abs() < 1e-12);
    // Without `keep_baseline`, no baseline column.
    let op2 = Operation::Baseline {
        method: BaselineMethod::Line {
            p1: (0.0, 2.0),
            p2: (10.0, 7.0),
        },
        op: BaselineOp::Divide,
        keep_baseline: false,
    };
    let out2 = run(&op2, &[line(5, |x| 2.0 + 0.5 * x)], None).unwrap();
    assert!(out2[0].columns.iter().all(|c| c.key != "baseline"));
    assert!(col(&out2[0], "corrected")
        .data
        .iter()
        .all(|v| (v - 1.0).abs() < 1e-12));
}

#[test]
fn baseline_by_spline_anchors() {
    let inp = line(21, |x| 0.01 * x * x);
    let op = Operation::Baseline {
        method: BaselineMethod::Anchors {
            points: (0..=4)
                .map(|i| (5.0 * i as f64, 0.01 * (5.0 * i as f64).powi(2)))
                .collect(),
            interpolation: Interpolation::Spline,
        },
        op: BaselineOp::Subtract,
        keep_baseline: false,
    };
    let o = &run(&op, &[inp], None).unwrap()[0];
    let max = col(o, "corrected")
        .data
        .iter()
        .fold(0.0_f64, |a, b| a.max(b.abs()));
    assert!(max < 0.05, "max deviation {max}");
}

#[test]
fn a_batch_is_processed_series_by_series() {
    let a = line(10, |x| 3.0 * x);
    let b = line(10, |x| 4.0 - x);
    let op = Operation::Fit {
        spec: FitSpec {
            model: Model::Linear,
            params: vec![],
        },
        range: None,
        residuals: true,
    };
    let out = run(&op, &[a, b], None).unwrap();
    assert_eq!(out.len(), 2);
    assert!((val(&out[0], "param.slope").value - 3.0).abs() < 1e-10);
    assert!((val(&out[1], "param.slope").value + 1.0).abs() < 1e-10);
    assert!((val(&out[1], "param.intercept").value - 4.0).abs() < 1e-10);
    assert!(col(&out[0], "residuals")
        .data
        .iter()
        .all(|r| r.abs() < 1e-9));
}

#[test]
fn the_fit_range_limits_the_fitted_curve() {
    let inp = line(21, |x| 2.0 * x);
    let op = Operation::Fit {
        spec: FitSpec {
            model: Model::Linear,
            params: vec![],
        },
        range: Some((5.0, 10.0)),
        residuals: false,
    };
    let o = &run(&op, &[inp], None).unwrap()[0];
    let fit = &col(o, "fit").data;
    assert!(fit[4].is_nan() && fit[11].is_nan());
    assert!((fit[5] - 10.0).abs() < 1e-9 && (fit[10] - 20.0).abs() < 1e-9);
    assert_eq!(val(o, "stat.points").value, 6.0);
}

#[test]
fn an_empty_range_is_an_error() {
    let op = Operation::Fit {
        spec: FitSpec {
            model: Model::Linear,
            params: vec![],
        },
        range: Some((100.0, 200.0)),
        residuals: false,
    };
    assert_eq!(
        run(&op, &[line(10, |x| x)], None).unwrap_err(),
        AnalysisError::EmptyRange
    );
}

#[test]
fn the_global_fit_shares_a_parameter_between_the_series() {
    let mk = |a: f64| {
        let x: Vec<f64> = (0..80).map(|i| i as f64 * 0.1).collect();
        let y = x.iter().map(|v| a * libm::exp(-v / 2.0)).collect();
        SeriesInput { x, y, sigma: None }
    };
    let op = Operation::Fit {
        spec: FitSpec {
            model: Model::Exponential,
            params: vec![
                ParamSetting::default(),
                ParamSetting {
                    shared: true,
                    ..Default::default()
                },
            ],
        },
        range: None,
        residuals: false,
    };
    let out = run(&op, &[mk(1.0), mk(3.0)], None).unwrap();
    assert_eq!(
        val(&out[0], "param.tau").value,
        val(&out[1], "param.tau").value
    );
    assert!((val(&out[1], "param.A").value - 3.0).abs() < 1e-5);
}

#[test]
fn the_correction_requires_a_reference_and_counts_the_out_of_domain_points() {
    let inp = line(6, |_| 10.0);
    let op = Operation::Correct {
        op: CorrectOp::Divide,
    };
    assert_eq!(
        run(&op, std::slice::from_ref(&inp), None).unwrap_err(),
        AnalysisError::EmptyReference
    );
    let aux = AuxInput {
        x: vec![1.0, 4.0],
        y: vec![2.0, 2.0],
    };
    let o = &run(&op, &[inp], Some(&aux)).unwrap()[0];
    let c = &col(o, "corrected").data;
    assert!(c[0].is_nan() && c[5].is_nan());
    assert_eq!(c[2], 5.0);
    assert_eq!(val(o, "count.out_of_reference").value, 2.0);
}

#[test]
fn the_integral_gives_areas_total_and_columns() {
    let inp = line(11, |_| 2.0);
    let op = Operation::Integrate {
        spec: IntegrateSpec {
            ranges: vec![(0.0, 4.0), (6.0, 10.0)],
            under: Under::Zero,
            cumulative: true,
            normalize: true,
        },
    };
    let o = &run(&op, &[inp], None).unwrap()[0];
    assert_eq!(val(o, "area").value, 8.0);
    assert_eq!(o.values.iter().filter(|v| v.key == "area").count(), 2);
    assert_eq!(val(o, "area.total").value, 16.0);
    assert_eq!(col(o, "cumulative").data[10], 20.0);
    assert!((col(o, "normalized").data[0] - 0.125).abs() < 1e-12);
}

/// #276: a non-finite range (not placed yet) is refused, instead of
/// integrating the whole curve (`min/max` ignore NaN); `readiness` says it
/// must be placed.
#[test]
fn a_non_finite_integral_range_is_refused() {
    use stokesforge_analysis::integrate::integrate;
    use stokesforge_analysis::ops::Readiness;
    use stokesforge_analysis::series::Series;
    for range in [(f64::NAN, f64::NAN), (f64::NAN, 5.0), (5.0, f64::INFINITY)] {
        let spec = IntegrateSpec {
            ranges: vec![(1.0, 2.0), range],
            under: Under::Zero,
            cumulative: false,
            normalize: false,
        };
        let op = Operation::Integrate { spec: spec.clone() };
        let invalid = |r: Result<_, AnalysisError>| matches!(r, Err(AnalysisError::InvalidParameter { ref name }) if name == "ranges");
        assert!(invalid(run(&op, &[line(11, |_| 2.0)], None).map(|_| ())));
        assert!(invalid(op.validate()));
        let x: Vec<f64> = (0..11).map(f64::from).collect();
        assert!(invalid(
            integrate(&Series::new(x.clone(), x), &spec).map(|_| ())
        ));
        assert_eq!(op.readiness(), Readiness::NeedsZone);
    }
}

/// #276: each path (`validate`, `readiness`, `run`) sees the non-finite
/// bounds on its own, without the help of the others; placed ranges and the
/// empty list stay accepted.
#[test]
fn validate_and_readiness_of_the_integral_stand_independently() {
    use stokesforge_analysis::ops::Readiness;
    let (nan, inf) = (f64::NAN, f64::INFINITY);
    let op = |ranges: Vec<(f64, f64)>| Operation::Integrate {
        spec: IntegrateSpec {
            ranges,
            under: Under::Zero,
            cumulative: false,
            normalize: false,
        },
    };
    for r in [
        (nan, nan),
        (nan, 5.0),
        (5.0, nan),
        (inf, 5.0),
        (-inf, 5.0),
        (2.0, inf),
        (2.0, -inf),
    ] {
        let o = op(vec![r]);
        assert_eq!(
            o.validate(),
            Err(AnalysisError::InvalidParameter {
                name: "ranges".into()
            }),
            "{r:?}"
        );
        assert_eq!(o.readiness(), Readiness::NeedsZone, "{r:?}");
        // Bad range in second position: same verdict.
        let o = op(vec![(1.0, 2.0), r]);
        assert!(o.validate().is_err(), "{r:?}");
        assert_eq!(o.readiness(), Readiness::NeedsZone, "{r:?}");
    }
    for ok in [vec![], vec![(1.0, 2.0)], vec![(8.0, 2.0), (0.0, 1.0)]] {
        let o = op(ok.clone());
        assert_eq!(o.validate(), Ok(()), "{ok:?}");
        assert_eq!(o.readiness(), Readiness::Ready, "{ok:?}");
    }
    // Empty list = the whole curve, y = x on [0, 10]: 50.
    let out = run(&op(vec![]), &[line(11, |x| x)], None).unwrap();
    assert_eq!(val(&out[0], "area.total").value, 50.0);
}

#[test]
fn automatically_detected_peaks_give_one_column_per_component() {
    let x: Vec<f64> = (0..400).map(|i| i as f64 * 0.1).collect();
    let g = |v: f64, h: f64, c: f64, w: f64| {
        h * libm::exp(-4.0 * std::f64::consts::LN_2 * ((v - c) / w).powi(2))
    };
    let y = x
        .iter()
        .map(|&v| 1.0 + g(v, 10.0, 12.0, 3.0) + g(v, 6.0, 28.0, 4.0))
        .collect();
    let inp = SeriesInput { x, y, sigma: None };
    let op = Operation::Peaks {
        spec: PeaksSpec {
            shape: PeakShape::Gaussian,
            background: PeakBackground::Constant,
            seeds: vec![],
            link_widths: false,
            center_window: None,
        },
        auto: Some(AutoDetect {
            min_prominence: 0.1,
            max_peaks: 5,
        }),
        range: None,
        residuals: true,
    };
    let o = &run(&op, &[inp], None).unwrap()[0];
    let peaks: Vec<&OutColumn> = o.columns.iter().filter(|c| c.key == "peak").collect();
    assert_eq!(peaks.len(), 2);
    assert_eq!(peaks[0].index, Some(1));
    assert_eq!(val(o, "peak.height").index, Some(1));
    assert!((val(o, "background.y0").value - 1.0).abs() < 1e-4);
    // The sum (background included) matches the curve: residuals ≈ 0.
    assert!(col(o, "residuals").data.iter().all(|r| r.abs() < 1e-4));
    assert!(col(o, "background")
        .data
        .iter()
        .all(|b| (b - 1.0).abs() < 1e-4));
}

#[test]
fn peaks_without_a_seed_or_detection_are_refused() {
    let op = Operation::Peaks {
        spec: PeaksSpec {
            shape: PeakShape::Gaussian,
            background: PeakBackground::None,
            seeds: vec![],
            link_widths: false,
            center_window: None,
        },
        auto: None,
        range: None,
        residuals: false,
    };
    assert!(matches!(
        run(&op, &[line(50, |x| x)], None),
        Err(AnalysisError::InvalidParameter { .. })
    ));
}

#[test]
fn reconvolution_and_direct_deconvolution_end_to_end() {
    let step = 0.05;
    let n = 400;
    let x: Vec<f64> = (0..n).map(|i| i as f64 * step).collect();
    let irf_y: Vec<f64> = x
        .iter()
        .map(|t| libm::exp(-(t - 1.0_f64).powi(2) / (2.0 * 0.1 * 0.1)))
        .collect();
    let aux = AuxInput {
        x: x.clone(),
        y: irf_y.clone(),
    };
    let sum: f64 = irf_y.iter().sum();
    let h: Vec<f64> = irf_y.iter().map(|v| v / sum).collect();
    // Signal: direct convolution of h with 50·exp(−t/1.5).
    let u: Vec<f64> = x.iter().map(|t| 50.0 * libm::exp(-t / 1.5)).collect();
    let y: Vec<f64> = (0..n)
        .map(|k| (0..=k).map(|j| h[j] * u[k - j]).sum::<f64>())
        .collect();
    let mut inp = SeriesInput {
        x: x.clone(),
        y,
        sigma: None,
    };
    inp.y[10] = f64::NAN; // dropped row: the output stays aligned
    let op = Operation::Reconvolution {
        spec: ReconvSpec {
            components: 1,
            params: vec![],
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        },
        residuals: true,
    };
    let o = &run(&op, &[inp.clone()], Some(&aux)).unwrap()[0];
    assert_eq!(o.skipped, 1);
    assert!((val(o, "param.tau1").value - 1.5).abs() < 1e-3);
    assert!(col(o, "fit").data[10].is_finite() && col(o, "residuals").data[10].is_nan());
    assert_eq!(col(o, "fit").data.len(), n);

    let dir = Operation::DirectDeconvolution {
        method: DirectMethod::RichardsonLucy { iterations: 50 },
        shift: 0.0,
    };
    let mut clean_inp = inp;
    clean_inp.y[10] = 0.5 * (clean_inp.y[9] + clean_inp.y[11]);
    let o = &run(&dir, &[clean_inp], Some(&aux)).unwrap()[0];
    assert_eq!(col(o, "deconvolved").data.len(), n);
    assert!(col(o, "deconvolved").data.iter().all(|v| v.is_finite()));
    // Without an IRF, a dedicated error.
    assert_eq!(
        run(&dir, &[line(50, |x| x + 1.0)], None).unwrap_err(),
        AnalysisError::EmptyResponse
    );
}

#[test]
fn no_input() {
    let op = Operation::Integrate {
        spec: IntegrateSpec {
            ranges: vec![],
            under: Under::Zero,
            cumulative: false,
            normalize: false,
        },
    };
    assert!(run(&op, &[], None).is_err());
    assert!(!op.needs_aux());
}

#[test]
fn families_and_automatic_recalculation_by_default() {
    use stokesforge_analysis::ops::OperationKind;
    for k in OperationKind::ALL {
        assert_eq!(OperationKind::from_key(k.key()), Some(k));
    }
    assert_eq!(OperationKind::from_key("unknown_key"), None);
    let recon = Operation::Reconvolution {
        spec: ReconvSpec {
            components: 1,
            params: vec![],
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        },
        residuals: false,
    };
    let lin = Operation::Fit {
        spec: FitSpec {
            model: Model::Linear,
            params: vec![],
        },
        range: None,
        residuals: false,
    };
    assert!(!recon.auto_recalc_by_default(1));
    assert!(lin.auto_recalc_by_default(20));
    assert!(
        !lin.auto_recalc_by_default(21),
        "a batch of more than 20 series is heavy"
    );
    assert_eq!(lin.kind(), OperationKind::Fit);
}

#[test]
fn constant_and_zone_mean_correct_without_slope_and_keep_the_rows() {
    let input = SeriesInput {
        x: vec![0.0, 1.0, 2.0, 3.0, f64::NAN],
        y: vec![9.0, 2.0, f64::NAN, 6.0, 99.0],
        sigma: None,
    };
    for method in [
        BaselineMethod::Constant { point: (-5.0, 4.0) },
        BaselineMethod::ConstantZone { zone: (1.0, 3.0) },
    ] {
        let op = Operation::Baseline {
            method,
            op: BaselineOp::Subtract,
            keep_baseline: true,
        };
        let out = run(&op, std::slice::from_ref(&input), None).unwrap();
        let o = &out[0];
        assert_eq!(o.skipped, 2);
        let result = &col(o, "corrected").data;
        assert_eq!(result.len(), 5);
        assert_eq!(&result[..2], &[5.0, -2.0]);
        assert!(result[2].is_nan() && result[4].is_nan());
        assert_eq!(result[3], 2.0);
        assert_eq!(val(o, "baseline.slope").value, 0.0);
        assert_eq!(val(o, "baseline.intercept").value, 4.0);
    }
    for value in [2.0, 0.0] {
        let op = Operation::Baseline {
            method: BaselineMethod::Constant {
                point: (0.0, value),
            },
            op: BaselineOp::Divide,
            keep_baseline: false,
        };
        let out = run(&op, std::slice::from_ref(&input), None).unwrap();
        if value == 0.0 {
            assert!(col(&out[0], "corrected").data.iter().all(|v| v.is_nan()));
        } else {
            assert_eq!(col(&out[0], "corrected").data[0], 4.5);
        }
    }
}

#[test]
fn deconvolution_of_a_non_uniform_signal_raises_the_warning_and_covers_the_last_x() {
    let aux = AuxInput {
        x: (0..40).map(|i| i as f64 * 0.1).collect(),
        y: (0..40)
            .map(|i| libm::exp(-(i as f64 * 0.1 - 1.0).powi(2) / 0.18))
            .collect(),
    };
    let dir = Operation::DirectDeconvolution {
        method: DirectMethod::Wiener { noise: Some(0.01) },
        shift: 0.0,
    };
    let irregular = SeriesInput {
        x: vec![0.0, 1.0, 2.0, 3.0, 4.4],
        y: vec![1.0, 2.0, 3.0, 2.0, 1.0],
        sigma: None,
    };
    let o = &run(&dir, &[irregular], Some(&aux)).unwrap()[0];
    assert!(o.warnings.contains(&Warning::SignalStepNotUniform));
    let d = &col(o, "deconvolved").data;
    assert_eq!(d.len(), 5);
    assert!(d.iter().all(|v| v.is_finite()), "{d:?}");
    let regular = SeriesInput {
        x: vec![0.0, 1.0, 2.0, 3.0, 4.0],
        y: vec![1.0, 2.0, 3.0, 2.0, 1.0],
        sigma: None,
    };
    let o = &run(&dir, &[regular], Some(&aux)).unwrap()[0];
    assert!(!o.warnings.contains(&Warning::SignalStepNotUniform));
}

// ── C3, C5 (#313): reference gaps, divisions by zero ───────────

fn correction(op: CorrectOp) -> Operation {
    Operation::Correct { op }
}

fn reference(x: &[f64], y: &[f64]) -> AuxInput {
    AuxInput {
        x: x.to_vec(),
        y: y.to_vec(),
    }
}

fn data(x: &[f64]) -> SeriesInput {
    SeriesInput {
        x: x.to_vec(),
        y: vec![1.0; x.len()],
        sigma: None,
    }
}

/// The gap of an empty cell in the middle of the reference is bridged by
/// interpolation: the three points that fall inside are counted.
#[test]
fn a_gap_in_the_middle_of_the_reference_is_bridged_and_reported() {
    let aux = reference(&[1.0, 2.0, 3.0, 4.0], &[1.0, f64::NAN, 3.0, 4.0]);
    let out = run(
        &correction(CorrectOp::Multiply),
        &[data(&[1.5, 2.0, 2.5, 3.5])],
        Some(&aux),
    )
    .unwrap();
    let o = &out[0];
    assert_eq!(o.warnings, vec![Warning::BridgedReferenceGap(3)]);
    // Interpolation between (1, 1) and (3, 3): y = x.
    assert_eq!(col(o, "corrected").data, vec![1.5, 2.0, 2.5, 3.5]);
}

/// NaN at the end of the reference: the domain ends before, nothing is
/// bridged. A reference row without X has no position: silence.
#[test]
fn a_gap_at_the_end_or_without_a_position_is_not_a_bridge() {
    let tail = reference(&[1.0, 2.0, 3.0, 4.0], &[1.0, 2.0, 3.0, f64::NAN]);
    let o = &run(
        &correction(CorrectOp::Multiply),
        &[data(&[1.5, 2.5, 3.5])],
        Some(&tail),
    )
    .unwrap()[0];
    assert_eq!(o.warnings, vec![]);
    let without_x = reference(&[1.0, f64::NAN, 3.0], &[1.0, 2.0, 3.0]);
    let o = &run(
        &correction(CorrectOp::Multiply),
        &[data(&[1.5, 2.5])],
        Some(&without_x),
    )
    .unwrap()[0];
    assert_eq!(o.warnings, vec![]);
}

#[test]
fn a_zero_reference_at_a_point_gives_nan_and_a_division_by_zero() {
    let aux = reference(&[1.0, 2.0, 3.0], &[1.0, 0.0, 1.0]);
    let o = &run(
        &correction(CorrectOp::Divide),
        &[data(&[1.0, 2.0, 3.0])],
        Some(&aux),
    )
    .unwrap()[0];
    let c = &col(o, "corrected").data;
    assert!(c[1].is_nan() && c[0] == 1.0 && c[2] == 1.0);
    assert_eq!(o.warnings, vec![Warning::DivisionByZero(1)]);
    // Multiplying by zero is not a division.
    let o = &run(
        &correction(CorrectOp::Multiply),
        &[data(&[1.0, 2.0, 3.0])],
        Some(&aux),
    )
    .unwrap()[0];
    assert_eq!(o.warnings, vec![]);
}

/// A baseline that passes through zero: the division is counted, the
/// subtraction is not.
#[test]
fn a_baseline_passing_through_zero_counts_the_division() {
    let method = BaselineMethod::Line {
        p1: (0.0, 0.0),
        p2: (10.0, 10.0),
    };
    let divide = Operation::Baseline {
        method: method.clone(),
        op: BaselineOp::Divide,
        keep_baseline: false,
    };
    let o = &run(&divide, &[line(11, |x| x + 1.0)], None).unwrap()[0];
    assert!(col(o, "corrected").data[0].is_nan());
    assert_eq!(o.warnings, vec![Warning::DivisionByZero(1)]);
    let subtract = Operation::Baseline {
        method,
        op: BaselineOp::Subtract,
        keep_baseline: false,
    };
    let o = &run(&subtract, &[line(11, |x| x + 1.0)], None).unwrap()[0];
    assert_eq!(o.warnings, vec![]);
}

/// #324: smoothing keeps the source length (dropped rows empty and counted),
/// writes `smoothed` and the window used; a non-uniform step warns only for
/// Savitzky-Golay.
#[test]
fn smoothing_aligns_its_output_on_the_rows_and_warns_of_an_irregular_step() {
    use stokesforge_analysis::smooth::{SmoothMethod, SmoothSpec, SmoothWindow};
    let smooth = |method| Operation::Smooth {
        spec: SmoothSpec {
            method,
            window: SmoothWindow::Points(5),
        },
    };
    let mut inp = line(12, |x| x * x);
    inp.y[3] = f64::NAN;
    let o = &run(&smooth(SmoothMethod::AdjacentAverage), &[inp.clone()], None).unwrap()[0];
    assert_eq!(o.skipped, 1);
    let output = &col(o, "smoothed").data;
    assert_eq!(output.len(), 12);
    assert!(output[3].is_nan(), "the dropped row stays empty");
    assert!(output[5].is_finite());
    assert_eq!(val(o, "count.window").value, 5.0);
    assert!(o.warnings.is_empty());

    inp.x[6] = 6.4; // irregular step
    let sg = &run(
        &smooth(SmoothMethod::SavitzkyGolay { order: 2 }),
        &[inp.clone()],
        None,
    )
    .unwrap()[0];
    assert!(sg.warnings.contains(&Warning::SignalStepNotUniform));
    let med = &run(&smooth(SmoothMethod::Median), &[inp], None).unwrap()[0];
    assert!(
        med.warnings.is_empty(),
        "the median does not need a regular step"
    );

    let out_of_range = Operation::Smooth {
        spec: SmoothSpec {
            method: SmoothMethod::AdjacentAverage,
            window: SmoothWindow::Points(4),
        },
    };
    assert!(matches!(
        run(&out_of_range, &[line(8, |x| x)], None),
        Err(AnalysisError::InvalidParameter { .. })
    ));
}
