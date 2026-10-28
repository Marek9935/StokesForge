//! #203: a fit that the data do not determine is reported, not returned as a
//! success; #205: a row with empty or zero σ is dropped and counted.

use stokesforge_analysis::fit::FitSpec;
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{run, Operation, Outcome, SeriesInput, Warning};
use stokesforge_analysis::AnalysisError;

fn fit_op(model: Model) -> Operation {
    Operation::Fit {
        spec: FitSpec {
            model,
            params: vec![],
        },
        range: None,
        residuals: false,
    }
}

fn error_of(o: &Outcome, key: &str) -> Option<f64> {
    o.values.iter().find(|v| v.key == key).unwrap().error
}

fn run_one(op: &Operation, input: SeriesInput) -> Result<Outcome, AnalysisError> {
    run(op, &[input], None).map(|mut v| v.remove(0))
}

#[test]
fn a_model_with_an_effectless_parameter_warns_without_silencing_the_others() {
    // y = a + b·x + 0·c: "c" has no influence on the model.
    let x: Vec<f64> = (0..40).map(f64::from).collect();
    let y: Vec<f64> = x
        .iter()
        .map(|v| 1.0 + 0.5 * v + 0.01 * libm::sin(v * 7.0))
        .collect();
    let op = fit_op(Model::Custom {
        formula: "a + b*x + 0*c".into(),
    });
    let o = run_one(&op, SeriesInput { x, y, sigma: None }).unwrap();
    assert!(
        o.warnings.contains(&Warning::Undetermined(1)),
        "{:?}",
        o.warnings
    );
    // Well fitted: an effectless slot is not a plateau (#281).
    assert!(!o.warnings.contains(&Warning::NotConverged));
    assert!(error_of(&o, "param.c").is_none());
    assert!(error_of(&o, "param.a").is_some() && error_of(&o, "param.b").is_some());
}

#[test]
fn all_equal_x_make_the_slope_undetermined() {
    let x = vec![2.0; 10];
    let y: Vec<f64> = (0..10).map(|i| 3.0 + 0.01 * f64::from(i % 3)).collect();
    let o = run_one(&fit_op(Model::Linear), SeriesInput { x, y, sigma: None }).unwrap();
    assert!(
        o.warnings.contains(&Warning::Undetermined(1)),
        "{:?}",
        o.warnings
    );
}

#[test]
fn no_determined_parameter_is_refused() {
    let x: Vec<f64> = (0..10).map(f64::from).collect();
    let y = vec![1.0; 10];
    let op = fit_op(Model::Custom {
        formula: "0*a + 0*b".into(),
    });
    assert_eq!(
        run_one(&op, SeriesInput { x, y, sigma: None }).unwrap_err(),
        AnalysisError::SingularSystem
    );
}

#[test]
fn a_well_posed_fit_does_not_warn() {
    let x: Vec<f64> = (0..30).map(f64::from).collect();
    let y: Vec<f64> = x
        .iter()
        .map(|v| 2.0 + 0.3 * v + 0.01 * libm::sin(v * 3.0))
        .collect();
    let o = run_one(&fit_op(Model::Linear), SeriesInput { x, y, sigma: None }).unwrap();
    assert!(o.warnings.is_empty());
}

#[test]
fn a_row_with_empty_or_zero_sigma_is_dropped_and_counted() {
    let x: Vec<f64> = (0..20).map(f64::from).collect();
    let y: Vec<f64> = x.iter().map(|v| 1.0 + 2.0 * v).collect();
    let mut sigma = vec![0.5; 20];
    sigma[3] = f64::NAN;
    sigma[7] = 0.0;
    let o = run_one(
        &fit_op(Model::Linear),
        SeriesInput {
            x,
            y,
            sigma: Some(sigma),
        },
    )
    .unwrap();
    assert_eq!(o.skipped, 2);
    let points = o
        .values
        .iter()
        .find(|v| v.key == "stat.points")
        .unwrap()
        .value;
    assert_eq!(points, 18.0);
    let slope = o
        .values
        .iter()
        .find(|v| v.key == "param.slope")
        .unwrap()
        .value;
    assert!((slope - 2.0).abs() < 1e-9);
}

/// #281: a start outside the data does not "converge" silently.
fn gaussian_input(noise: f64) -> SeriesInput {
    let x: Vec<f64> = (0..200).map(|i| 20.0 * f64::from(i) / 199.0).collect();
    let y = x
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let z = (v - 8.0) / (3.0 / 2.354_820_045);
            5.0 * libm::exp(-0.5 * z * z)
                + noise * (((i as f64 * 0.618_033_988_7).fract() - 0.5) * 2.0)
        })
        .collect();
    SeriesInput { x, y, sigma: None }
}

fn gaussian_fit(initial: &[(usize, f64)]) -> Operation {
    use stokesforge_analysis::fit::ParamSetting;
    let mut params = vec![ParamSetting::default(); 4];
    for &(j, v) in initial {
        params[j].initial = Some(v);
    }
    Operation::Fit {
        spec: FitSpec {
            model: Model::Gaussian,
            params,
        },
        range: None,
        residuals: false,
    }
}

#[test]
fn a_start_outside_the_data_warns_not_converged() {
    // xc = 1000, fwhm = 1: the peak is outside the range, R² ≈ 0.
    let op = gaussian_fit(&[(2, 1000.0), (3, 1.0)]);
    let o = run_one(&op, gaussian_input(0.0)).unwrap();
    assert!(
        o.warnings.contains(&Warning::NotConverged),
        "{:?}",
        o.warnings
    );
}

#[test]
fn a_sound_or_noisy_fit_does_not_warn() {
    for noise in [0.0, 0.3] {
        let o = run_one(&gaussian_fit(&[]), gaussian_input(noise)).unwrap();
        assert!(o.warnings.is_empty(), "noise {noise}: {:?}", o.warnings);
    }
}

#[test]
fn a_free_model_converges_on_an_offset_x() {
    // X = 1e6 + t: the "1" column and the "x" column of the Jacobian are
    // nearly collinear (cosine 1 − 1e-13); the linear model (direct least
    // squares) succeeds, the free model must succeed too.
    let t: Vec<f64> = (0..60).map(|i| f64::from(i) / 59.0).collect();
    let x: Vec<f64> = t.iter().map(|v| 1e6 + v).collect();
    let y: Vec<f64> = t.iter().map(|v| 2.0 + 3.0 * v).collect();
    let op = fit_op(Model::Custom {
        formula: "a + b*x".into(),
    });
    let o = run_one(&op, SeriesInput { x, y, sigma: None }).unwrap();
    let b = o.values.iter().find(|v| v.key == "param.b").unwrap().value;
    assert!((b - 3.0).abs() < 1e-3, "b = {b}");
    assert!(!o.warnings.contains(&Warning::NotConverged));
}
