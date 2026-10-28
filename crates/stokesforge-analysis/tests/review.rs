//! Guards from the review of work package #181: empty inputs without panic,
//! fit independent of the Y scale, per-curve statistics of a batch, setting
//! caps.

use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp, Interpolation};
use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::deconv::{DirectMethod, ReconvSpec};
use stokesforge_analysis::fit::{FitSpec, ParamSetting};
use stokesforge_analysis::integrate::{IntegrateSpec, Under};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{run, AuxInput, Operation, Outcome, SeriesInput};
use stokesforge_analysis::peaks::{PeakBackground, PeakSeed, PeakShape, PeaksSpec};
use stokesforge_analysis::AnalysisError;

fn value(o: &Outcome, key: &str) -> f64 {
    o.values
        .iter()
        .find(|v| v.key == key)
        .unwrap_or_else(|| panic!("value '{key}' missing"))
        .value
}

fn input(x: Vec<f64>, y: Vec<f64>) -> SeriesInput {
    SeriesInput { x, y, sigma: None }
}

fn every_operation() -> Vec<Operation> {
    vec![
        Operation::Baseline {
            method: BaselineMethod::Anchors {
                points: vec![(0.0, 0.0), (1.0, 1.0)],
                interpolation: Interpolation::Segments,
            },
            op: BaselineOp::Subtract,
            keep_baseline: true,
        },
        Operation::Baseline {
            method: BaselineMethod::Zones {
                zones: vec![(0.0, 1.0)],
                degree: 1,
            },
            op: BaselineOp::Subtract,
            keep_baseline: false,
        },
        Operation::Correct {
            op: CorrectOp::Divide,
        },
        Operation::Integrate {
            spec: IntegrateSpec {
                ranges: vec![(0.0, 1.0)],
                under: Under::Zero,
                cumulative: true,
                normalize: true,
            },
        },
        Operation::Fit {
            spec: FitSpec {
                model: Model::Linear,
                params: vec![],
            },
            range: None,
            residuals: true,
        },
        Operation::Peaks {
            spec: PeaksSpec {
                shape: PeakShape::Gaussian,
                background: PeakBackground::Constant,
                seeds: vec![PeakSeed {
                    center: 1.0,
                    height: Some(1.0),
                    fwhm: None,
                }],
                link_widths: false,
                center_window: None,
            },
            auto: None,
            range: None,
            residuals: true,
        },
        Operation::DirectDeconvolution {
            method: DirectMethod::RichardsonLucy { iterations: 5 },
            shift: 0.0,
        },
        Operation::DirectDeconvolution {
            method: DirectMethod::Wiener { noise: None },
            shift: 0.0,
        },
    ]
}

#[test]
fn an_empty_or_tiny_series_never_panics() {
    let aux = AuxInput {
        x: vec![0.0, 1.0, 2.0],
        y: vec![1.0, 2.0, 1.0],
    };
    for n in 0..4 {
        let series = input((0..n).map(f64::from).collect(), vec![1.0; n as usize]);
        for op in every_operation() {
            // Ok or Err, no matter: the session actor has no `catch_unwind`,
            // a panic there is fatal.
            let _ = run(&op, std::slice::from_ref(&series), Some(&aux));
        }
        let _ = run(
            &Operation::Reconvolution {
                spec: ReconvSpec {
                    components: 1,
                    params: vec![],
                    shift_initial: 0.0,
                    fit_shift: true,
                    window: None,
                },
                residuals: false,
            },
            std::slice::from_ref(&series),
            Some(&aux),
        );
    }
}

#[test]
fn hand_placed_peaks_on_an_emptied_column_are_an_error() {
    let op = every_operation().remove(5);
    assert!(matches!(
        run(&op, &[input(vec![], vec![])], None),
        Err(AnalysisError::NotEnoughPoints { .. })
    ));
}

#[test]
fn the_fit_does_not_depend_on_the_y_scale() {
    // y0 + A·exp(−x/τ), τ = 2, fitted with the automatic initial values.
    for scale in [1e-12, 1e-9, 1e-3, 1.0, 1e6] {
        let x: Vec<f64> = (0..60).map(|i| f64::from(i) * 0.25).collect();
        let y: Vec<f64> = x
            .iter()
            .map(|&v| scale * (0.3 + 2.0 * libm::exp(-v / 2.0)))
            .collect();
        let op = Operation::Fit {
            spec: FitSpec {
                model: Model::ExponentialOffset,
                params: vec![],
            },
            range: None,
            residuals: false,
        };
        let out = run(&op, &[input(x, y)], None).unwrap_or_else(|e| panic!("scale {scale}: {e}"));
        let tau = value(&out[0], "param.tau");
        assert!((tau - 2.0).abs() < 1e-4, "scale {scale}: τ = {tau}");
    }
}

#[test]
fn an_independent_batch_carries_the_degrees_of_freedom_of_each_curve() {
    let curves: Vec<SeriesInput> = (0..3)
        .map(|k| {
            let x: Vec<f64> = (0..20).map(f64::from).collect();
            let y = x
                .iter()
                .enumerate()
                .map(|(i, v)| f64::from(k) + 0.5 * v + if i % 2 == 0 { 0.01 } else { -0.01 })
                .collect();
            input(x, y)
        })
        .collect();
    let op = Operation::Fit {
        spec: FitSpec {
            model: Model::Linear,
            params: vec![ParamSetting::default()],
        },
        range: None,
        residuals: false,
    };
    let out = run(&op, &curves, None).unwrap();
    for o in &out {
        assert_eq!(value(o, "stat.dof"), 18.0, "20 points − 2 parameters");
    }
}

#[test]
fn settings_beyond_the_cap_are_refused_by_run() {
    let series = input((0..30).map(f64::from).collect(), vec![1.0; 30]);
    let poly = |degree| Operation::Fit {
        spec: FitSpec {
            model: Model::Polynomial { degree },
            params: vec![],
        },
        range: None,
        residuals: false,
    };
    assert!(matches!(
        run(&poly(1_000_000_000), std::slice::from_ref(&series), None),
        Err(AnalysisError::InvalidParameter { .. })
    ));
    let aux = AuxInput {
        x: vec![0.0, 1.0, 2.0],
        y: vec![0.0, 1.0, 0.0],
    };
    for method in [
        DirectMethod::RichardsonLucy { iterations: 10_000 },
        DirectMethod::Wiener { noise: Some(0.0) },
        DirectMethod::Wiener { noise: Some(-1.0) },
    ] {
        let op = Operation::DirectDeconvolution { method, shift: 0.0 };
        assert!(matches!(
            run(&op, std::slice::from_ref(&series), Some(&aux)),
            Err(AnalysisError::InvalidParameter { .. })
        ));
    }
}
