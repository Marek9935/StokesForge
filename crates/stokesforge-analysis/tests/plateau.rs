//! #281: `Warning::NotConverged` on the paths that go through the solver
//! (fit, global fit, peaks, reconvolution), its absence on a sound fit,
//! linear or noisy, and on direct deconvolution.

use stokesforge_analysis::deconv::{DirectMethod, ReconvSpec};
use stokesforge_analysis::fit::{FitSpec, ParamSetting};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{run, AuxInput, Operation, Outcome, SeriesInput, Warning};
use stokesforge_analysis::peaks::{PeakBackground, PeakSeed, PeakShape, PeaksSpec};

fn gaussian(x: f64, xc: f64, a: f64, fwhm: f64) -> f64 {
    let z = (x - xc) / (fwhm / 2.354_820_045);
    a * libm::exp(-0.5 * z * z)
}

/// 200 points on [0, 20], one peak xc = 8, A = 5, fwhm = 3 (the example of #281).
fn peak_signal(noise: f64) -> SeriesInput {
    let x: Vec<f64> = (0..200).map(|i| 20.0 * f64::from(i) / 199.0).collect();
    let y = x
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            gaussian(v, 8.0, 5.0, 3.0)
                + noise * (((i as f64 * 0.618_033_988_7).fract() - 0.5) * 2.0)
        })
        .collect();
    SeriesInput { x, y, sigma: None }
}

fn run_one(op: &Operation, input: SeriesInput, aux: Option<&AuxInput>) -> Outcome {
    run(op, &[input], aux).unwrap().remove(0)
}

fn fit(model: Model, initial: &[(usize, f64)], n: usize) -> Operation {
    let mut params = vec![ParamSetting::default(); n];
    for &(j, v) in initial {
        params[j].initial = Some(v);
    }
    Operation::Fit {
        spec: FitSpec { model, params },
        range: None,
        residuals: false,
    }
}

/// Gaussian (y0, A, xc, fwhm) started at the given `xc` and `fwhm`.
fn gaussian_fit(xc: f64, fwhm: f64) -> Operation {
    fit(Model::Gaussian, &[(2, xc), (3, fwhm)], 4)
}

fn peaks(center: f64) -> Operation {
    Operation::Peaks {
        spec: PeaksSpec {
            shape: PeakShape::Gaussian,
            background: PeakBackground::Constant,
            seeds: vec![PeakSeed {
                center,
                height: Some(1.0),
                fwhm: Some(1.0),
            }],
            link_widths: false,
            center_window: None,
        },
        auto: None,
        range: None,
        residuals: false,
    }
}

const ORTHO: [f64; 8] = [1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0];

/// `y = x + k·ORTHO` on X = 0..7: an affine fit leaves an SSres/SStot share
/// equal to `fraction` (SSres = 8k², SStot = 42 + 8k²).
fn affine_residue(fraction: f64) -> SeriesInput {
    let k = (42.0 * fraction / (8.0 * (1.0 - fraction))).sqrt();
    let x: Vec<f64> = (0..8).map(f64::from).collect();
    let y = x.iter().zip(ORTHO).map(|(x, r)| x + k * r).collect();
    SeriesInput { x, y, sigma: None }
}

/// `a + b·x + 0·c`: "c" is a free slot with no effect.
fn affine_with_dead_slot(shared: bool) -> Operation {
    let params = vec![
        ParamSetting {
            shared,
            ..ParamSetting::default()
        };
        3
    ];
    Operation::Fit {
        spec: FitSpec {
            model: Model::Custom {
                formula: "a + b*x + 0*c".into(),
            },
            params,
        },
        range: None,
        residuals: false,
    }
}

#[test]
fn the_example_of_issue_281_warns_not_converged_after_the_indetermination() {
    let o = run_one(&gaussian_fit(1000.0, 1.0), peak_signal(0.0), None);
    assert_eq!(
        o.warnings,
        vec![Warning::Undetermined(3), Warning::NotConverged]
    );
    let r2 = o.values.iter().find(|v| v.key == "stat.r_squared").unwrap();
    assert!(r2.value < 0.1, "{}", r2.value);
}

#[test]
fn a_sound_or_noisy_fit_does_not_warn() {
    for noise in [0.0, 0.3] {
        let o = run_one(&gaussian_fit(7.0, 2.0), peak_signal(noise), None);
        assert_eq!(o.warnings, vec![], "noise {noise}");
        // Automatic start too.
        let o = run_one(&fit(Model::Gaussian, &[], 4), peak_signal(noise), None);
        assert_eq!(o.warnings, vec![], "auto, noise {noise}");
    }
}

#[test]
fn a_linear_fit_never_reports_a_plateau_even_at_zero_r2() {
    // Almost no affine signal: R² ≈ 0, but the linear path is exact.
    let input = affine_residue(0.99);
    let o = run_one(&fit(Model::Linear, &[], 2), input, None);
    let r2 = o.values.iter().find(|v| v.key == "stat.r_squared").unwrap();
    assert!(r2.value < 0.05, "{}", r2.value);
    assert_eq!(o.warnings, vec![]);
}

#[test]
fn the_plateau_threshold_shows_end_to_end_at_one_half() {
    let below = run_one(&affine_with_dead_slot(false), affine_residue(0.49), None);
    assert_eq!(below.warnings, vec![Warning::Undetermined(1)]);
    let above = run_one(&affine_with_dead_slot(false), affine_residue(0.51), None);
    assert_eq!(
        above.warnings,
        vec![Warning::Undetermined(1), Warning::NotConverged]
    );
}

#[test]
fn an_overparameterized_model_that_fits_well_is_undetermined_without_not_converged() {
    let o = run_one(&affine_with_dead_slot(false), affine_residue(0.01), None);
    assert_eq!(o.warnings, vec![Warning::Undetermined(1)]);
}

#[test]
fn independent_curves_each_warn_on_their_own() {
    let batch = [affine_residue(0.01), affine_residue(0.9)];
    let out = run(&affine_with_dead_slot(false), &batch, None).unwrap();
    assert_eq!(out[0].warnings, vec![Warning::Undetermined(1)]);
    assert_eq!(
        out[1].warnings,
        vec![Warning::Undetermined(1), Warning::NotConverged]
    );
}

#[test]
fn a_global_fit_with_only_one_curve_on_a_plateau_warns() {
    for batch in [
        [affine_residue(0.01), affine_residue(0.9)],
        [affine_residue(0.9), affine_residue(0.01)],
    ] {
        let out = run(&affine_with_dead_slot(true), &batch, None).unwrap();
        for (k, o) in out.iter().enumerate() {
            assert!(o.warnings.contains(&Warning::NotConverged), "curve {k}");
        }
    }
    let batch = [affine_residue(0.01), affine_residue(0.02)];
    let out = run(&affine_with_dead_slot(true), &batch, None).unwrap();
    for o in &out {
        assert!(!o.warnings.contains(&Warning::NotConverged));
    }
}

#[test]
fn the_peak_decomposition_warns_not_converged_outside_the_data_and_not_on_the_peak() {
    let o = run_one(&peaks(1000.0), peak_signal(0.0), None);
    assert_eq!(
        o.warnings,
        vec![Warning::Undetermined(3), Warning::NotConverged]
    );
    for noise in [0.0, 0.3] {
        let o = run_one(&peaks(8.0), peak_signal(noise), None);
        assert_eq!(o.warnings, vec![], "noise {noise}");
    }
}

fn signal_and_response() -> (AuxInput, SeriesInput) {
    let step = 0.05;
    let n = 400;
    let x: Vec<f64> = (0..n).map(|i| f64::from(i) * step).collect();
    let irf_y: Vec<f64> = x.iter().map(|t| gaussian(*t, 1.0, 1.0, 0.2)).collect();
    let aux = AuxInput {
        x: x.clone(),
        y: irf_y.clone(),
    };
    let sum: f64 = irf_y.iter().sum();
    let h: Vec<f64> = irf_y.iter().map(|v| v / sum).collect();
    let u: Vec<f64> = x.iter().map(|t| 50.0 * libm::exp(-t / 1.5)).collect();
    let y = (0..x.len())
        .map(|k| (0..=k).map(|j| h[j] * u[k - j]).sum::<f64>())
        .collect();
    (aux, SeriesInput { x, y, sigma: None })
}

/// One-exponential reconvolution (bkg, A1, tau1, shift) where the amplitude
/// A1 is pinned to 0: the model no longer does anything with `tau1`.
fn reconvolution_zero_amplitude() -> Operation {
    let mut params = vec![ParamSetting::default(); 4];
    params[1] = ParamSetting {
        initial: Some(0.0),
        min: Some(0.0),
        max: Some(0.0),
        ..ParamSetting::default()
    };
    Operation::Reconvolution {
        spec: ReconvSpec {
            components: 1,
            params,
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        },
        residuals: false,
    }
}

#[test]
fn a_sound_reconvolution_does_not_warn() {
    let (aux, sig) = signal_and_response();
    let op = Operation::Reconvolution {
        spec: ReconvSpec {
            components: 1,
            params: vec![],
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        },
        residuals: false,
    };
    let o = run_one(&op, sig, Some(&aux));
    assert_eq!(o.warnings, vec![]);
}

#[test]
fn the_reconvolution_exposes_the_solver_plateau() {
    // Computation layer: the flag is set (direct path).
    use stokesforge_analysis::deconv::reconvolve;
    use stokesforge_analysis::series::Series;
    let (aux, sig) = signal_and_response();
    let Operation::Reconvolution { spec, .. } = reconvolution_zero_amplitude() else {
        unreachable!()
    };
    let r = reconvolve(
        &Series::new(sig.x.clone(), sig.y.clone()),
        &Series::new(aux.x.clone(), aux.y.clone()),
        &spec,
    )
    .unwrap();
    assert_eq!(r.undetermined, 2);
    assert!(r.plateau);
}

#[test]
fn a_reconvolution_on_a_plateau_warns_not_converged_through_ops_run() {
    let (aux, sig) = signal_and_response();
    let o = run_one(&reconvolution_zero_amplitude(), sig, Some(&aux));
    assert_eq!(
        o.warnings,
        vec![Warning::Undetermined(2), Warning::NotConverged]
    );
}

/// Two-exponential reconvolution (bkg, A1, tau1, A2, tau2, shift) where only
/// the amplitude A2 is pinned to 0: `tau2` is undetermined, but the signal
/// (a single exponential) is still fitted very well.
fn reconvolution_second_zero_amplitude() -> Operation {
    let mut params = vec![ParamSetting::default(); 6];
    params[3] = ParamSetting {
        initial: Some(0.0),
        min: Some(0.0),
        max: Some(0.0),
        ..ParamSetting::default()
    };
    Operation::Reconvolution {
        spec: ReconvSpec {
            components: 2,
            params,
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        },
        residuals: false,
    }
}

#[test]
fn the_reconvolution_warns_undetermined_alone_through_ops_run() {
    let (aux, sig) = signal_and_response();
    let o = run_one(&reconvolution_second_zero_amplitude(), sig, Some(&aux));
    assert_eq!(o.warnings, vec![Warning::Undetermined(2)]);
}

#[test]
fn the_reconvolution_keeps_the_order_and_has_no_duplicate() {
    // IRF step different from the signal's: `IrfStepMismatch` (deconvolution)
    // is added after `Undetermined` and `NotConverged`, without duplicating them.
    let (mut aux, sig) = signal_and_response();
    aux.x.iter_mut().for_each(|x| *x *= 1.5);
    let o = run_one(&reconvolution_zero_amplitude(), sig, Some(&aux));
    assert_eq!(
        o.warnings,
        vec![
            Warning::Undetermined(2),
            Warning::NotConverged,
            Warning::IrfStepMismatch
        ]
    );
}

#[test]
fn direct_deconvolution_does_not_use_the_solver_and_does_not_warn_of_non_convergence() {
    let (aux, sig) = signal_and_response();
    let op = Operation::DirectDeconvolution {
        method: DirectMethod::RichardsonLucy { iterations: 20 },
        shift: 0.0,
    };
    let o = run_one(&op, sig, Some(&aux));
    assert!(!o.warnings.contains(&Warning::NotConverged));
    assert!(!o
        .warnings
        .iter()
        .any(|w| matches!(w, Warning::Undetermined(_))));
}

fn custom(formula: &str, initial: f64) -> Operation {
    Operation::Fit {
        spec: FitSpec {
            model: Model::Custom {
                formula: formula.into(),
            },
            params: vec![ParamSetting {
                initial: Some(initial),
                ..ParamSetting::default()
            }],
        },
        range: None,
        residuals: false,
    }
}

fn constant(y: f64) -> SeriesInput {
    SeriesInput {
        x: (0..8).map(f64::from).collect(),
        y: vec![y; 8],
        sigma: None,
    }
}

/// C1 (#313): a sharp minimum (`|a−3| + 0.5(a−3) + 1`, centerd slope 0.5)
/// where no step lowers the χ² is returned as "not converged", not presented
/// as a converged fit.
#[test]
fn a_solver_stuck_on_a_sharp_point_warns_not_converged() {
    let o = run_one(
        &custom("abs(a-3) + 0.5*(a-3) + 1", 3.0),
        constant(0.0),
        None,
    );
    assert_eq!(o.warnings, vec![Warning::NotConverged]);
}

/// C6 (#313): `sqrt(a)` at a = 0 has no slope (sqrt of a negative): a "model
/// not evaluable" error, not a "converged" nor a singular system.
#[test]
fn a_model_without_a_slope_at_the_start_is_a_model_not_evaluable_error() {
    let err = run(&custom("sqrt(a)", 0.0), &[constant(1.0)], None).unwrap_err();
    assert_eq!(
        err,
        stokesforge_analysis::error::AnalysisError::ModelNotEvaluable
    );
}

/// 21 points x = 0..20, Gaussian xc = 8, A = 5, fwhm = 3, **noiseless**: the
/// `pyst` bench of #281 (`stokesforge-script/tests/analysis.rs`, column G).
fn noiseless_peak_with_21_points() -> SeriesInput {
    let x: Vec<f64> = (0..21).map(f64::from).collect();
    let y = x.iter().map(|&v| gaussian(v, 8.0, 5.0, 3.0)).collect();
    SeriesInput { x, y, sigma: None }
}

/// #332: on exact data, the χ² falls to ~1e-31 (rounding) and then moved only
/// by a few ulp per step, without any relative criterion noticing: the
/// convergence depended on the last ulp of the platform's `exp` (MSVC
/// converged, `libm` did not, 400 iterations). A χ² at the rounding floor is a
/// zero residual: the solver has arrived, without a warning. The start
/// outside the data (xc = 1000) remains a plateau.
#[test]
fn exact_data_converge_at_the_rounding_floor_and_the_plateau_stays_a_plateau() {
    let plateau_run = run_one(
        &gaussian_fit(1000.0, 1.0),
        noiseless_peak_with_21_points(),
        None,
    );
    assert_eq!(
        plateau_run.warnings,
        vec![Warning::Undetermined(3), Warning::NotConverged]
    );

    let sound = run_one(
        &gaussian_fit(7.0, 2.0),
        noiseless_peak_with_21_points(),
        None,
    );
    assert_eq!(sound.warnings, vec![], "converges without a warning");
    let value = |key: &str| sound.values.iter().find(|v| v.key == key).unwrap().value;
    assert!((value("param.A") - 5.0).abs() < 1e-9);
    assert!((value("param.xc") - 8.0).abs() < 1e-9);
    assert!((value("param.fwhm") - 3.0).abs() < 1e-6);
    assert!(value("stat.r_squared") > 1.0 - 1e-12);
}
