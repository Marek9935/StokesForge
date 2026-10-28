//! #282: the Levenberg-Marquardt step by augmented least squares (QR) and the
//! λ floor at 1e-20 hold up on an offset X (nearly collinear Jacobian
//! columns). References: the `Linear` model (direct least squares) and the
//! same fit in a centerd frame.

use stokesforge_analysis::fit::{fit, FitInput, FitReport, FitSpec, ParamSetting};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{run, Operation, SeriesInput};
use stokesforge_analysis::series::Series;
use stokesforge_analysis::AnalysisError;

fn t_grid() -> Vec<f64> {
    (0..60).map(|i| f64::from(i) / 59.0).collect()
}

/// Deterministic noise of amplitude `amp`.
fn noise(i: usize, amp: f64) -> f64 {
    amp * libm::sin((i as f64) * 1.7)
}

fn free_line() -> Model {
    Model::Custom {
        formula: "a + b*x".into(),
    }
}

fn line(off: f64, amp: f64) -> FitInput {
    let x: Vec<f64> = t_grid().iter().map(|t| off + t).collect();
    let y: Vec<f64> = t_grid()
        .iter()
        .enumerate()
        .map(|(i, t)| 2.0 + 3.0 * t + noise(i, amp))
        .collect();
    FitInput {
        series: Series::new(x, y),
        sigma: None,
    }
}

fn spec(model: Model, params: Vec<ParamSetting>) -> FitSpec {
    FitSpec { model, params }
}

fn fit_with(inputs: &[FitInput], model: Model) -> FitReport {
    fit(inputs, &spec(model, vec![])).unwrap()
}

fn close(a: f64, b: f64, rel: f64, abs: f64) -> bool {
    (a - b).abs() <= abs + rel * a.abs().max(b.abs())
}

const HELD_OFFSETS: [f64; 3] = [1e3, 1e5, 1e6];

#[test]
fn the_issue_case_gives_the_linear_model_values() {
    let input = [line(1e6, 0.0)];
    let free = fit_with(&input, free_line());
    let lin = fit_with(&input, Model::Linear);
    let (l, d) = (&free.series[0], &lin.series[0]);
    // Free: (a, b); linear: (intercept, slope).
    assert!(close(l.values[1], d.values[1], 0.0, 1e-8), "{l:?} {d:?}");
    assert!(close(l.values[0], d.values[0], 1e-9, 0.0), "{l:?} {d:?}");
    assert!(close(l.values[1], 3.0, 0.0, 1e-8));
    assert!(!l.plateau);
}

#[test]
fn the_issue_case_gives_the_linear_model_uncertainties() {
    // Noisy, so that the uncertainties mean something.
    let input = [line(1e6, 0.01)];
    let free = fit_with(&input, free_line());
    let lin = fit_with(&input, Model::Linear);
    let (l, d) = (&free.series[0], &lin.series[0]);
    assert_eq!(l.undetermined, 0, "{l:?}");
    for j in 0..2 {
        let (el, ed) = (l.errors[j].unwrap(), d.errors[j].unwrap());
        assert!(close(el, ed, 1e-2, 0.0), "param {j}: {el} vs {ed}");
    }
}

#[test]
fn the_held_offsets_stay_consistent_with_the_linear_model() {
    for off in HELD_OFFSETS {
        for amp in [0.0, 0.01] {
            let input = [line(off, amp)];
            let free = fit_with(&input, free_line());
            let lin = fit_with(&input, Model::Linear);
            let (l, d) = (&free.series[0], &lin.series[0]);
            let ctx = format!("X = {off:e}, noise {amp}: {l:?} vs {d:?}");
            assert!(!l.plateau, "{ctx}");
            assert!(close(l.values[1], d.values[1], 0.0, 1e-6), "{ctx}");
            assert!(close(l.values[0], d.values[0], 1e-6, 0.0), "{ctx}");
            // χ²: noisy, equal to within 1e-6; noiseless, both ~0.
            assert!(close(l.chi2, d.chi2, 1e-6, 1e-15 * off * off), "{ctx}");
            assert_eq!(l.points, d.points, "{ctx}");
        }
    }
}

#[test]
fn the_uncertainties_at_moderate_offsets_follow_the_linear_model() {
    // 1e3 and 1e5: the two columns are still judged independent.
    for off in [1e3, 1e5] {
        let input = [line(off, 0.01)];
        let free = fit_with(&input, free_line());
        let lin = fit_with(&input, Model::Linear);
        let (l, d) = (&free.series[0], &lin.series[0]);
        assert_eq!(l.undetermined, 0, "X = {off:e}: {l:?}");
        for j in 0..2 {
            let (el, ed) = (l.errors[j].unwrap(), d.errors[j].unwrap());
            assert!(
                close(el, ed, 1e-3, 0.0),
                "X = {off:e}, param {j}: {el} / {ed}"
            );
        }
    }
}

#[test]
fn beyond_that_the_result_is_finite_and_never_silently_wrong() {
    // Current behavior (#293): no panic, no NaN, and either the χ² of the
    // linear model, or a warning saying the result is not reliable. Never a
    // wrong result without saying anything.
    for off in [1e7, 1e8, 1e10] {
        let x: Vec<f64> = t_grid().iter().map(|t| off + t).collect();
        let y: Vec<f64> = t_grid()
            .iter()
            .enumerate()
            .map(|(i, t)| 2.0 + 3.0 * t + noise(i, 0.01))
            .collect();
        let op = Operation::Fit {
            spec: spec(free_line(), vec![]),
            range: None,
            residuals: false,
        };
        let res = run(&op, &[SeriesInput { x, y, sigma: None }], None);
        let o = match res {
            Ok(mut v) => v.remove(0),
            Err(e) => {
                assert!(
                    matches!(e, AnalysisError::NotConverged { .. }),
                    "X = {off:e}: {e:?}"
                );
                continue;
            }
        };
        for v in &o.values {
            assert!(v.value.is_finite(), "X = {off:e}: {v:?}");
            assert!(v.error.is_none_or(f64::is_finite), "X = {off:e}: {v:?}");
        }
        let chi2 = |o: &stokesforge_analysis::ops::Outcome| {
            let dof = o.values.iter().find(|v| v.key == "stat.dof").unwrap().value;
            let red = o
                .values
                .iter()
                .find(|v| v.key == "stat.reduced_chi2")
                .unwrap()
                .value;
            dof * red
        };
        let lin = fit_with(&[line(off, 0.01)], Model::Linear);
        let right = close(chi2(&o), lin.series[0].chi2, 1e-3, 0.0);
        let warned = !o.warnings.is_empty();
        assert!(right || warned, "X = {off:e}: wrong and silent: {o:?}");
    }
}

#[test]
fn beyond_that_the_result_is_the_linear_model_one() {
    for off in [1e7, 1e8, 1e10] {
        let input = [line(off, 0.01)];
        let l = &fit_with(&input, free_line()).series[0].clone();
        let d = &fit_with(&input, Model::Linear).series[0].clone();
        assert!(
            close(l.values[1], d.values[1], 0.0, 1e-3),
            "X = {off:e}: {l:?}"
        );
        assert!(close(l.chi2, d.chi2, 1e-3, 0.0), "X = {off:e}: {l:?}");
    }
}

fn offset_gaussian(off: f64) -> Vec<f64> {
    // y0 = 1, A = 5, xc = off + 0.5, fwhm = 0.3, noiseless.
    let x: Vec<f64> = (0..120).map(|i| off + f64::from(i) / 119.0).collect();
    let y: Vec<f64> = x
        .iter()
        .map(|x| {
            let u = x - (off + 0.5);
            1.0 + 5.0 * libm::exp(-4.0 * libm::log(2f64) * u * u / 0.09)
        })
        .collect();
    let r = fit_with(
        &[FitInput {
            series: Series::new(x, y),
            sigma: None,
        }],
        Model::Gaussian,
    );
    assert!(!r.series[0].plateau);
    r.series[0].values.clone()
}

fn check_gaussian(off: f64) {
    let v = offset_gaussian(off);
    let ctx = format!("X = {off:e}: {v:?}");
    assert!(close(v[0], 1.0, 0.0, 1e-6), "{ctx}");
    assert!(close(v[1], 5.0, 0.0, 1e-6), "{ctx}");
    assert!(close(v[2], off + 0.5, 1e-12, 0.0), "{ctx}");
    assert!(close(v[3], 0.3, 0.0, 1e-6), "{ctx}");
}

#[test]
fn a_gaussian_on_an_offset_x_recovers_its_parameters() {
    for off in [0.0, 1e3, 1e5, 3e5] {
        check_gaussian(off);
    }
}

#[test]
fn a_gaussian_offset_by_one_million_recovers_its_parameters() {
    check_gaussian(1e6);
}

/// Noisy Gaussian (same noise at every offset), fitted on an offset X.
fn noisy_gaussian(off: f64) -> stokesforge_analysis::fit::FittedSeries {
    let x: Vec<f64> = (0..120).map(|i| off + f64::from(i) / 119.0).collect();
    let y: Vec<f64> = x
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let u = x - (off + 0.5);
            1.0 + 5.0 * libm::exp(-4.0 * libm::log(2f64) * u * u / 0.09) + noise(i, 0.02)
        })
        .collect();
    let input = FitInput {
        series: Series::new(x, y),
        sigma: None,
    };
    fit_with(&[input], Model::Gaussian).series.remove(0)
}

#[test]
fn a_noisy_gaussian_offset_by_one_million_has_the_centerd_frame_uncertainties() {
    // #294: the derivative in xc was wrong (step of 1 > width 0.3), so the
    // uncertainties were too (σ_xc = 22); they do not depend on the offset.
    let (ref0, far) = (noisy_gaussian(0.0), noisy_gaussian(1e6));
    assert!(!far.plateau && far.undetermined == 0, "{far:?}");
    for j in 0..4 {
        let (e0, e1) = (ref0.errors[j].unwrap(), far.errors[j].unwrap());
        assert!(close(e0, e1, 1e-2, 0.0), "param {j}: {e0} vs {e1}");
        let (v0, v1) = (ref0.values[j], far.values[j]);
        let expected = if j == 2 { v0 + 1e6 } else { v0 };
        assert!(
            close(v1, expected, 1e-6, 1e-6),
            "param {j}: {v1} / {expected}"
        );
    }
}

#[test]
fn an_exponential_on_an_offset_x_recovers_its_parameters() {
    // y = A·exp(−x/τ), x = off + t: the factor e^(−off/τ) is absorbed by A.
    for (off, tau) in [(100.0, 50.0), (1e3, 500.0)] {
        let x: Vec<f64> = (0..80).map(|i| off + 4.0 * f64::from(i) / 79.0).collect();
        let a = 7.0;
        let y: Vec<f64> = x.iter().map(|x| a * libm::exp(-x / tau)).collect();
        let r = fit_with(
            &[FitInput {
                series: Series::new(x, y),
                sigma: None,
            }],
            Model::Exponential,
        );
        let v = &r.series[0].values;
        let ctx = format!("X = {off}: {v:?}");
        assert!(close(v[1], tau, 1e-6, 0.0), "{ctx}");
        assert!(close(v[0], a, 1e-5, 0.0), "{ctx}");
    }
}

#[test]
fn a_fixed_parameter_stays_exact_and_the_other_follows_least_squares() {
    // b fixed at 2.9: a = mean(y − 2.9·x), exact.
    let input = line(1e5, 0.01);
    let n = input.series.x.len() as f64;
    let expected = input
        .series
        .x
        .iter()
        .zip(&input.series.y)
        .map(|(x, y)| y - 2.9 * x)
        .sum::<f64>()
        / n;
    let params = vec![
        ParamSetting::default(),
        ParamSetting {
            initial: Some(2.9),
            fixed: true,
            ..Default::default()
        },
    ];
    let r = fit(&[input], &spec(free_line(), params)).unwrap();
    let s = &r.series[0];
    assert_eq!(s.values[1], 2.9);
    assert!(s.errors[1].is_none());
    assert!(
        close(s.values[0], expected, 1e-9, 0.0),
        "{s:?} vs {expected}"
    );
    assert!(s.errors[0].is_some());
}

#[test]
fn an_active_bound_holds_on_an_offset_x() {
    // True slope 3, bounded at 2.5: the solver stops on the bound, and the
    // intercept is the least-squares one at slope 2.5.
    let input = line(1e5, 0.0);
    let expected = input
        .series
        .x
        .iter()
        .zip(&input.series.y)
        .map(|(x, y)| y - 2.5 * x)
        .sum::<f64>()
        / input.series.x.len() as f64;
    let params = vec![
        ParamSetting::default(),
        ParamSetting {
            initial: Some(2.0),
            max: Some(2.5),
            ..Default::default()
        },
    ];
    let r = fit(&[input], &spec(free_line(), params)).unwrap();
    let s = &r.series[0];
    assert!(s.values[1] <= 2.5, "{s:?}");
    assert!(close(s.values[1], 2.5, 0.0, 1e-9), "{s:?}");
    assert!(
        close(s.values[0], expected, 1e-6, 0.0),
        "{s:?} vs {expected}"
    );
}

#[test]
fn a_global_fit_with_a_shared_slope_on_an_offset_x() {
    let off = 1e5;
    let mk = |a: f64| {
        let x: Vec<f64> = t_grid().iter().map(|t| off + t).collect();
        let y: Vec<f64> = x.iter().map(|x| a + 3.0 * (x - off)).collect();
        FitInput {
            series: Series::new(x, y),
            sigma: None,
        }
    };
    // a + b·x with x = off + t: y = a_k + 3t = (a_k − 3·off) + 3x.
    let params = vec![
        ParamSetting::default(),
        ParamSetting {
            shared: true,
            ..Default::default()
        },
    ];
    let r = fit(&[mk(2.0), mk(-1.0)], &spec(free_line(), params)).unwrap();
    assert_eq!(r.series.len(), 2);
    for (s, a) in r.series.iter().zip([2.0, -1.0]) {
        assert!(close(s.values[1], 3.0, 0.0, 1e-6), "{s:?}");
        assert!(close(s.values[0], a - 3.0 * off, 1e-8, 0.0), "{s:?}");
        assert!(!s.plateau);
    }
    // Shared slope: the same exact value for both curves.
    assert_eq!(r.series[0].values[1], r.series[1].values[1]);
}

#[test]
fn the_centerd_frame_result_does_not_depend_on_the_offset() {
    // "a + b·(x − off)" on X = off + t is "a + b·t": same parameters and
    // same χ² as on X = t, within tolerance, whatever off is.
    let ref_ = fit_with(&[line(0.0, 0.01)], free_line());
    let r0 = &ref_.series[0];
    for off in [1e3, 1e5, 1e6, 1e8] {
        let model = Model::Custom {
            formula: format!("a + b*(x - {off:e})"),
        };
        let r = fit_with(&[line(off, 0.01)], model);
        let s = &r.series[0];
        let ctx = format!("X = {off:e}: {s:?} vs {r0:?}");
        assert!(close(s.values[0], r0.values[0], 1e-6, 0.0), "{ctx}");
        assert!(close(s.values[1], r0.values[1], 1e-6, 0.0), "{ctx}");
        assert!(close(s.chi2, r0.chi2, 1e-6, 0.0), "{ctx}");
        for j in 0..2 {
            assert!(
                close(s.errors[j].unwrap(), r0.errors[j].unwrap(), 1e-4, 0.0),
                "{ctx}"
            );
        }
    }
}

#[test]
fn an_ordinary_fit_without_offset_does_not_change() {
    // Non-regression: ordinary X, free model = linear model to within 1e-9.
    let input = [line(0.0, 0.05)];
    let l = fit_with(&input, free_line());
    let d = fit_with(&input, Model::Linear);
    for j in 0..2 {
        assert!(close(
            l.series[0].values[j],
            d.series[0].values[j],
            1e-9,
            1e-9
        ));
        assert!(close(
            l.series[0].errors[j].unwrap(),
            d.series[0].errors[j].unwrap(),
            1e-6,
            0.0
        ));
    }
}

fn parabola(off: f64) -> FitInput {
    let x: Vec<f64> = t_grid().iter().map(|t| off + t).collect();
    let y: Vec<f64> = t_grid()
        .iter()
        .enumerate()
        .map(|(i, t)| 2.0 + 3.0 * t - 1.5 * t * t + noise(i, 0.01))
        .collect();
    FitInput {
        series: Series::new(x, y),
        sigma: None,
    }
}

#[test]
fn a_free_degree_two_polynomial_reaches_the_chi2_of_the_polynomial_model() {
    // Three nearly collinear columns: the normal equations (squared
    // conditioning) stop at a χ² 300 times too large at X = 1e4; the QR does
    // not. (The uncertainties of this case belong to #293.)
    for off in [1e3, 1e4] {
        let input = [parabola(off)];
        let free = fit_with(
            &input,
            Model::Custom {
                formula: "a + b*x + c*x^2".into(),
            },
        );
        let poly = fit_with(&input, Model::Polynomial { degree: 2 });
        let (l, p) = (&free.series[0], &poly.series[0]);
        assert!(!l.plateau, "X = {off:e}: {l:?}");
        assert!(
            close(l.chi2, p.chi2, 1e-4, 0.0),
            "X = {off:e}: {l:?} vs {p:?}"
        );
    }
}
