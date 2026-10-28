//! Fit of a model to one or several curves (#188), with fixed
//! and bounded parameters, weighting by a Y standard deviation, and **global fit**
//! (parameters shared between curves, M13 b).

use crate::error::AnalysisError;
use crate::linalg::least_squares;
use crate::model::{CompiledModel, Model};
use crate::series::Series;
use crate::solver::{solve_problem, Options, Slot, Term};

/// Setting of a model parameter, in the order of [`CompiledModel::params`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParamSetting {
    /// `None`: automatic initial value.
    pub initial: Option<f64>,
    pub fixed: bool,
    /// `None`: default bound of the model.
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// Global fit: a single value for all the curves.
    pub shared: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FitSpec {
    pub model: Model,
    /// Per-parameter settings; parameters beyond the list have the
    /// default settings.
    pub params: Vec<ParamSetting>,
}

impl FitSpec {
    /// Fills the starting values with the automatic estimate of the model
    /// ([`CompiledModel::guess`]) on the points of `series` whose X falls in
    /// `range` (bounds included, in either order; `None`: the whole
    /// series, except non-finite X). The per-parameter settings are brought to
    /// the length of the model; a **fixed** parameter keeps its value. No
    /// effect if the model does not compile.
    pub fn auto_initial(&mut self, series: &Series, range: Option<(f64, f64)>) {
        let Ok(compiled) = self.model.compile() else {
            return;
        };
        let (lo, hi) = range.unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
        let guess = compiled.guess(&series.restricted(lo, hi));
        self.params
            .resize(compiled.param_count(), ParamSetting::default());
        for (setting, value) in self.params.iter_mut().zip(guess) {
            if !setting.fixed {
                setting.initial = Some(value);
            }
        }
    }
}

/// A curve to fit: finite points, already restricted to the wanted range.
#[derive(Debug, Clone, PartialEq)]
pub struct FitInput {
    pub series: Series,
    /// Standard deviation of each Y (weighting `1/σ`), of the same length.
    pub sigma: Option<Vec<f64>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FittedSeries {
    pub values: Vec<f64>,
    /// `None`: fixed parameter, or uncertainty that cannot be computed.
    pub errors: Vec<Option<f64>>,
    pub points: usize,
    pub r_squared: f64,
    /// Weighted χ² of this curve.
    pub chi2: f64,
    /// Degrees of freedom of this curve alone (global fit: of the
    /// whole, shared by all the curves).
    pub dof: usize,
    /// Reduced χ² of this curve alone (global fit: of the whole).
    pub reduced_chi2: f64,
    /// Number of free parameters that the data do not determine (#203).
    pub undetermined: usize,
    /// Stop on a plateau (#281) or solver stalled without an accepted step (C1,
    /// #313): not to be read as a converged fit.
    pub plateau: bool,
    /// Polynomial fitted by Levenberg-Marquardt (constrained) on an X very
    /// far from 0, without the centering of the QR path: the precision may have
    /// degraded (#296, warn rather than center).
    pub shifted_x: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FitReport {
    pub param_names: Vec<String>,
    pub series: Vec<FittedSeries>,
    /// Degrees of freedom and reduced χ² of the fit (global: of the whole).
    pub dof: usize,
    pub reduced_chi2: f64,
    pub iterations: usize,
}

fn validate_sigma(input: &FitInput) -> Result<(), AnalysisError> {
    if let Some(s) = &input.sigma {
        if s.len() != input.series.len() || s.iter().any(|v| !(*v > 0.0) || !v.is_finite()) {
            return Err(AnalysisError::InvalidParameter {
                name: "sigma".into(),
            });
        }
    }
    Ok(())
}

fn setting(spec: &FitSpec, j: usize) -> ParamSetting {
    spec.params.get(j).cloned().unwrap_or_default()
}

/// Fits the model. Without a shared parameter, each curve is fitted
/// separately; as soon as a parameter is shared, all are fitted together.
pub fn fit(inputs: &[FitInput], spec: &FitSpec) -> Result<FitReport, AnalysisError> {
    let model = spec.model.compile()?;
    if inputs.is_empty() {
        return Err(AnalysisError::NotEnoughPoints {
            found: 0,
            needed: model.param_count(),
        });
    }
    for i in inputs {
        validate_sigma(i)?;
    }
    let k = model.param_count();
    let global = (0..k).any(|j| setting(spec, j).shared) && inputs.len() > 1;
    if global {
        return fit_together(inputs, spec, &model);
    }
    // Independent curves: aggregated report (dof and reduced χ² summed).
    let mut series = Vec::new();
    let (mut dof, mut chi2_sum, mut iterations) = (0usize, 0.0, 0usize);
    for input in inputs {
        let (one, d, it) = fit_one(input, spec, &model)?;
        dof += d;
        chi2_sum += one.chi2;
        iterations = iterations.max(it);
        series.push(one);
    }
    Ok(FitReport {
        param_names: model.params().iter().map(|p| p.name.clone()).collect(),
        series,
        dof,
        reduced_chi2: if dof > 0 {
            chi2_sum / dof as f64
        } else {
            f64::NAN
        },
        iterations,
    })
}

/// A single curve: exact least squares if the model is linear and
/// unconstrained, otherwise Levenberg-Marquardt.
fn fit_one(
    input: &FitInput,
    spec: &FitSpec,
    model: &CompiledModel,
) -> Result<(FittedSeries, usize, usize), AnalysisError> {
    let k = model.param_count();
    let constrained = (0..k).any(|j| {
        let s = setting(spec, j);
        s.fixed || s.min.is_some() || s.max.is_some() || s.initial.is_some()
    });
    if spec.model.is_linear_in_params() && !constrained {
        if let Some(r) = fit_linear(input, model) {
            return Ok((r.0, r.1, 1));
        }
    }
    let guess = model.guess(&input.series);
    let slots: Vec<Slot> = (0..k)
        .map(|j| {
            let s = setting(spec, j);
            let def = &model.params()[j];
            Slot {
                name: def.name.clone(),
                initial: s.initial.unwrap_or(guess[j]),
                fixed: s.fixed,
                min: s.min.unwrap_or(def.min),
                max: s.max.unwrap_or(def.max),
            }
        })
        .collect();
    let x = &input.series.x;
    let sol = {
        let term = Term {
            x,
            y: &input.series.y,
            sigma: input.sigma.as_deref(),
            eval: Box::new(|p, out| {
                for (o, &xi) in out.iter_mut().zip(x.iter()) {
                    *o = model.eval(p, xi);
                }
            }),
        };
        solve_problem(&slots, &[term], Options::default())?
    };
    Ok((
        FittedSeries {
            values: sol.values,
            errors: sol.errors,
            points: sol.terms[0].points,
            r_squared: sol.terms[0].r_squared,
            chi2: sol.chi2,
            dof: sol.dof,
            reduced_chi2: sol.reduced_chi2,
            undetermined: sol.undetermined.iter().filter(|u| **u).count(),
            plateau: sol.plateau || sol.stalled,
            shifted_x: model.x_shift_is_large(x),
        },
        sol.dof,
        sol.iterations,
    ))
}

/// Exact linear least squares (QR), weighted by `1/σ`.
fn fit_linear(input: &FitInput, model: &CompiledModel) -> Option<(FittedSeries, usize)> {
    let s = &input.series;
    let n = s.len();
    let k = model.param_count();
    if n < k || (matches!(model.model(), Model::Logarithm) && s.x.iter().any(|v| *v <= 0.0)) {
        return None;
    }
    // Polynomial: X centerd before the QR (#283), coefficients then brought back to
    // the original axis; the residual is computed in the centerd axis (without the catastrophic
    // cancellation of large powers of X).
    let shift = model.x_shift(&s.x);
    let xs: Vec<f64> = s.x.iter().map(|v| v - shift).collect();
    let mut a = model.design_matrix(&xs)?;
    let mut y = s.y.clone();
    if let Some(sig) = &input.sigma {
        for i in 0..n {
            let w = 1.0 / sig[i];
            for j in 0..k {
                a.set(i, j, a.get(i, j) * w);
            }
            y[i] *= w;
        }
    }
    let ls = least_squares(&a, &y)?;
    let fitted: Vec<f64> = xs
        .iter()
        .map(|&x| model.eval(&ls.coefficients, x))
        .collect();
    let chi2: f64 = (0..n)
        .map(|i| {
            let w = input.sigma.as_ref().map_or(1.0, |sg| 1.0 / sg[i]);
            ((s.y[i] - fitted[i]) * w).powi(2)
        })
        .sum();
    let dof = n - k;
    let scale = if dof > 0 { chi2 / dof as f64 } else { f64::NAN };
    let (values, cov) = match model.unshift_matrix(shift) {
        Some(t) => (
            t.mul_vec(&ls.coefficients),
            t.mul(&ls.unscaled_covariance).mul(&t.transpose()),
        ),
        None => (ls.coefficients, ls.unscaled_covariance),
    };
    let errors = (0..k)
        .map(|j| {
            let v = cov.get(j, j) * scale;
            (v.is_finite() && v >= 0.0).then(|| v.sqrt())
        })
        .collect();
    let mean = s.y.iter().sum::<f64>() / n as f64;
    let ss_tot: f64 = s.y.iter().map(|v| (v - mean).powi(2)).sum();
    let ss_res: f64 = (0..n).map(|i| (s.y[i] - fitted[i]).powi(2)).sum();
    Some((
        FittedSeries {
            values,
            errors,
            points: n,
            r_squared: if ss_tot > 0.0 {
                1.0 - ss_res / ss_tot
            } else {
                f64::NAN
            },
            chi2,
            dof,
            reduced_chi2: scale,
            undetermined: 0,
            plateau: false,
            shifted_x: false,
        },
        dof,
    ))
}

/// Global fit: one slot per shared parameter, one slot per curve for
/// the others.
fn fit_together(
    inputs: &[FitInput],
    spec: &FitSpec,
    model: &CompiledModel,
) -> Result<FitReport, AnalysisError> {
    let k = model.param_count();
    let guesses: Vec<Vec<f64>> = inputs.iter().map(|i| model.guess(&i.series)).collect();
    let mut slots: Vec<Slot> = Vec::new();
    // map[series][param] = slot index.
    let mut map = vec![vec![0usize; k]; inputs.len()];
    for j in 0..k {
        let s = setting(spec, j);
        let def = &model.params()[j];
        let mk = |initial: f64| Slot {
            name: def.name.clone(),
            initial: s.initial.unwrap_or(initial),
            fixed: s.fixed,
            min: s.min.unwrap_or(def.min),
            max: s.max.unwrap_or(def.max),
        };
        if s.shared {
            // Starting value: mean of the estimates of the curves.
            let mean = guesses.iter().map(|g| g[j]).sum::<f64>() / inputs.len() as f64;
            slots.push(mk(mean));
            for m in map.iter_mut() {
                m[j] = slots.len() - 1;
            }
        } else {
            for (i, m) in map.iter_mut().enumerate() {
                slots.push(mk(guesses[i][j]));
                m[j] = slots.len() - 1;
            }
        }
    }
    let terms: Vec<Term<'_>> = inputs
        .iter()
        .enumerate()
        .map(|(i, input)| {
            let idx = map[i].clone();
            let x = &input.series.x;
            Term {
                x,
                y: &input.series.y,
                sigma: input.sigma.as_deref(),
                eval: Box::new(move |vals, out| {
                    let p: Vec<f64> = idx.iter().map(|&s| vals[s]).collect();
                    for (o, &xi) in out.iter_mut().zip(x.iter()) {
                        *o = model.eval(&p, xi);
                    }
                }),
            }
        })
        .collect();
    let sol = solve_problem(&slots, &terms, Options::default())?;
    let series = inputs
        .iter()
        .enumerate()
        .map(|(i, _)| FittedSeries {
            values: map[i].iter().map(|&s| sol.values[s]).collect(),
            errors: map[i].iter().map(|&s| sol.errors[s]).collect(),
            points: sol.terms[i].points,
            r_squared: sol.terms[i].r_squared,
            dof: sol.dof,
            reduced_chi2: sol.reduced_chi2,
            undetermined: map[i].iter().filter(|&&s| sol.undetermined[s]).count(),
            plateau: sol.plateau || sol.stalled,
            shifted_x: false,
            chi2: {
                // χ² of the curve alone.
                let t = &terms[i];
                let mut f = vec![0.0; t.x.len()];
                (t.eval)(&sol.values, &mut f);
                (0..t.x.len())
                    .map(|r| ((t.y[r] - f[r]) * t.sigma.map_or(1.0, |s| 1.0 / s[r])).powi(2))
                    .sum()
            },
        })
        .collect();
    Ok(FitReport {
        param_names: model.params().iter().map(|p| p.name.clone()).collect(),
        series,
        dof: sol.dof,
        reduced_chi2: sol.reduced_chi2,
        iterations: sol.iterations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(n: usize, a: f64, b: f64) -> Vec<f64> {
        (0..n)
            .map(|i| a + (b - a) * i as f64 / (n - 1) as f64)
            .collect()
    }

    fn input(f: impl Fn(f64) -> f64, x: Vec<f64>) -> FitInput {
        let y = x.iter().map(|&v| f(v)).collect();
        FitInput {
            series: Series::new(x, y),
            sigma: None,
        }
    }

    fn spec(model: Model) -> FitSpec {
        FitSpec {
            model,
            params: vec![],
        }
    }

    #[test]
    fn too_deep_free_model_is_refused_at_the_fit() {
        // The parser cap reaches the fit, with the
        // error position, without walking the whole formula.
        let formula = format!("a*{}x{}", "(".repeat(100), ")".repeat(100));
        let e = fit(
            &[input(|v| v, vec![1.0, 2.0, 3.0])],
            &spec(Model::Custom { formula }),
        )
        .unwrap_err();
        assert_eq!(
            e,
            AnalysisError::BadExpression(crate::expr::ExprError {
                kind: crate::expr::ExprErrorKind::TooDeep,
                position: 102 // `*` costs 1: the 100th parenthesis already exceeds
            })
        );
    }

    #[test]
    fn free_model_log_is_base_10_end_to_end() {
        // y = 2 log10(x) + 1: with `log` (base 10), a = 2; with `ln`
        // (natural), a = 2 / ln 10; with `logx(x, 10)`, a = 2. If `log`
        // became natural again (#287), a would be 2 ln 10 ≈ 4.6.
        let x = grid(30, 0.5, 200.0);
        let truth = |v: f64| 2.0 * libm::log10(v) + 1.0;
        for (formula, a_expected) in [
            ("a*log(x)+b", 2.0),
            ("a*log10(x)+b", 2.0),
            ("a*logx(x, 10)+b", 2.0),
            ("a*ln(x)+b", 2.0 / libm::log(10f64)),
            ("a*log2(x)+b", 2.0 * libm::log(2f64) / libm::log(10f64)),
        ] {
            let r = fit(
                &[input(truth, x.clone())],
                &spec(Model::Custom {
                    formula: formula.into(),
                }),
            )
            .unwrap();
            assert_eq!(r.param_names, ["a", "b"], "{formula}");
            let v = &r.series[0].values;
            assert!((v[0] - a_expected).abs() < 1e-8, "{formula} : a = {}", v[0]);
            assert!((v[1] - 1.0).abs() < 1e-8, "{formula} : b = {}", v[1]);
            assert!((r.series[0].r_squared - 1.0).abs() < 1e-12, "{formula}");
        }
        // Free base: y = 2 ln(x) / ln(b) depends only on a and b together;
        // fixing the base at 10 recovers a.
        let r = fit(
            &[input(truth, x)],
            &spec(Model::Custom {
                formula: "a*logx(x, 10)+b".into(),
            }),
        )
        .unwrap();
        assert!((r.series[0].values[0] - 2.0).abs() < 1e-8);
    }

    #[test]
    fn exact_line_by_linear_least_squares() {
        let i = input(|x| 1.5 - 0.25 * x, grid(20, 0.0, 10.0));
        let r = fit(&[i], &spec(Model::Linear)).unwrap();
        let s = &r.series[0];
        assert!((s.values[0] - 1.5).abs() < 1e-12 && (s.values[1] + 0.25).abs() < 1e-12);
        assert!((s.r_squared - 1.0).abs() < 1e-12);
        assert_eq!(r.dof, 18);
        assert_eq!(r.iterations, 1, "exact path, no iteration");
    }

    #[test]
    fn zero_degrees_of_freedom_no_uncertainty_on_both_paths() {
        // Witness: 2 points, 2 free parameters. The non-linear path
        // gave uncertainties (not scaled) where the linear one
        // gives none.
        let x = vec![0.0, 1.0];
        let lin = fit(&[input(|v| 3.0 - 0.5 * v, x.clone())], &spec(Model::Linear)).unwrap();
        let exp = fit(
            &[input(|v| 3.0 * libm::exp(-v / 2.0), x.clone())],
            &spec(Model::Exponential),
        )
        .unwrap();
        for r in [&lin, &exp] {
            assert_eq!(r.dof, 0);
            assert!(r.series[0].errors.iter().all(Option::is_none));
            assert!(r.series[0].reduced_chi2.is_nan() && r.reduced_chi2.is_nan());
        }
        // A fixed parameter: 1 free for 2 points, dof = 1, uncertainty.
        let mut sp = spec(Model::Exponential);
        sp.params = vec![
            ParamSetting {
                fixed: true,
                initial: Some(3.0),
                ..Default::default()
            },
            ParamSetting::default(),
        ];
        let r = fit(&[input(|v| 3.0 * libm::exp(-v / 2.0), x)], &sp).unwrap();
        assert_eq!(r.dof, 1);
        assert!(r.series[0].errors[0].is_none() && r.series[0].errors[1].is_some());
    }

    #[test]
    fn polynomial_on_shifted_x_keeps_its_precision() {
        // #283: X = 1e5 + t, y = 1 + 2t - 3t². Before centering X before the
        // QR, the residual was ~1e-5; it must stay at the rounding level.
        let t = grid(21, -5.0, 5.0);
        let x: Vec<f64> = t.iter().map(|v| 1e5 + v).collect();
        let y: Vec<f64> = t.iter().map(|v| 1.0 + 2.0 * v - 3.0 * v * v).collect();
        let i = FitInput {
            series: Series::new(x, y),
            sigma: None,
        };
        let r = fit(&[i], &spec(Model::Polynomial { degree: 2 })).unwrap();
        let s = &r.series[0];
        assert!(s.chi2 < 1e-18, "chi2 = {:e}", s.chi2);
        assert!((s.r_squared - 1.0).abs() < 1e-14);
        // Coefficients in the original axis: y = 1 + 2(X-S) - 3(X-S)², S = 1e5.
        let sh = 1e5;
        let want = [1.0 - 2.0 * sh - 3.0 * sh * sh, 2.0 + 6.0 * sh, -3.0];
        for (g, w) in s.values.iter().zip(want) {
            assert!((g - w).abs() <= 1e-9 * w.abs(), "{g} vs {w}");
        }
    }

    #[test]
    fn shifted_line_same_coefficients_and_errors_as_the_centerd_line() {
        // Deterministic noisy data. Reference: X centerd at 0 (no
        // shift); shifted by S, the line is deduced by y = c0 + c1 (X-S):
        // a1 = c1 (same error), a0 = c0 - S c1, var(a0) = var(c0) + S² var(c1)
        // (orthogonal columns, t of zero mean).
        let t = grid(11, -5.0, 5.0);
        let noise = |i: usize| 0.3 * libm::sin((i as f64) * 1.7);
        let mk = |off: f64| {
            let x: Vec<f64> = t.iter().map(|v| off + v).collect();
            let y: Vec<f64> = t
                .iter()
                .enumerate()
                .map(|(i, v)| 2.0 + 0.5 * v + noise(i))
                .collect();
            FitInput {
                series: Series::new(x, y),
                sigma: None,
            }
        };
        let r0 = fit(&[mk(0.0)], &spec(Model::Linear)).unwrap();
        let sh = 1000.0;
        let r1 = fit(&[mk(sh)], &spec(Model::Linear)).unwrap();
        let (a, b) = (&r0.series[0], &r1.series[0]);
        let rel = |x: f64, y: f64| (x - y).abs() <= 1e-9 * (1.0 + y.abs());
        assert!(rel(b.values[1], a.values[1]));
        assert!(rel(b.values[0], a.values[0] - sh * a.values[1]));
        let (e0, e1) = (a.errors[0].unwrap(), a.errors[1].unwrap());
        assert!(rel(b.errors[1].unwrap(), e1));
        assert!(rel(
            b.errors[0].unwrap(),
            (e0 * e0 + sh * sh * e1 * e1).sqrt()
        ));
        assert!(rel(b.chi2, a.chi2));
    }

    fn fixed(v: f64) -> ParamSetting {
        ParamSetting {
            fixed: true,
            initial: Some(v),
            ..Default::default()
        }
    }

    /// Deterministic noise of amplitude ±1 (Weyl sequence).
    fn noise(i: usize) -> f64 {
        ((i as f64 * 0.618_033_988_749_895).fract() - 0.5) * 2.0
    }

    fn assert_uncertainties_present(r: &FitReport, free: &[usize]) {
        for (j, e) in r.series[0].errors.iter().enumerate() {
            if free.contains(&j) {
                let e = e.unwrap_or_else(|| panic!("uncertainty absent at {j}"));
                assert!(e.is_finite() && e > 0.0, "uncertainty {j} = {e}");
            } else {
                assert!(e.is_none(), "fixed parameter {j} with uncertainty");
            }
        }
        assert!(r.reduced_chi2.is_finite() && r.series[0].reduced_chi2.is_finite());
        assert!(r.series[0].chi2.is_finite());
    }

    #[test]
    fn linear_and_exponential_consistent_at_equal_n() {
        // n = 2 → dof 0 (nothing), n ≥ 3 → dof = n − 2 (uncertainties on both).
        for n in 2..=6usize {
            let x = grid(n, 0.0, 4.0);
            let mk = |f: &dyn Fn(f64) -> f64| {
                let y: Vec<f64> = x
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| f(v) + 1e-3 * noise(i + 1))
                    .collect();
                FitInput {
                    series: Series::new(x.clone(), y),
                    sigma: None,
                }
            };
            let lin = fit(&[mk(&|v| 3.0 - 0.5 * v)], &spec(Model::Linear)).unwrap();
            let exp = fit(
                &[mk(&|v| 3.0 * libm::exp(-v / 2.0))],
                &spec(Model::Exponential),
            )
            .unwrap();
            for r in [&lin, &exp] {
                assert_eq!(r.dof, n - 2, "n = {n}");
                assert_eq!(r.series[0].dof, n - 2);
                if n == 2 {
                    assert!(r.series[0].errors.iter().all(Option::is_none));
                    assert!(r.reduced_chi2.is_nan());
                    assert!(r.series[0].chi2.is_finite());
                } else {
                    assert_uncertainties_present(r, &[0, 1]);
                    let expected = r.series[0].chi2 / (n - 2) as f64;
                    assert!(
                        (r.reduced_chi2 - expected).abs() <= 1e-12 * expected.abs().max(1e-300)
                    );
                }
            }
        }
    }

    #[test]
    fn one_degree_of_freedom_gives_finite_uncertainties_for_several_models() {
        // Exponential: 3 points, 2 free.
        let x = vec![0.0, 1.0, 2.5];
        let y: Vec<f64> = x
            .iter()
            .enumerate()
            .map(|(i, &v): (usize, &f64)| 3.0 * libm::exp(-v / 2.0) + 1e-2 * noise(i + 1))
            .collect();
        let inp = FitInput {
            series: Series::new(x, y),
            sigma: None,
        };
        let r = fit(&[inp], &spec(Model::Exponential)).unwrap();
        assert_eq!(r.dof, 1);
        assert_uncertainties_present(&r, &[0, 1]);

        // Gaussian: 4 points, y0 fixed → 3 free.
        let x = vec![-1.0, 0.0, 0.7, 2.0];
        let y: Vec<f64> = x
            .iter()
            .enumerate()
            .map(|(i, v)| {
                2.0 * libm::exp(-4.0 * libm::log(2f64) * (v - 0.3) * (v - 0.3) / 1.5f64.powi(2))
                    + 1e-2 * noise(i + 1)
            })
            .collect();
        let mut sp = spec(Model::Gaussian);
        sp.params = vec![
            fixed(0.0),
            ParamSetting {
                initial: Some(2.0),
                ..Default::default()
            },
            ParamSetting {
                initial: Some(0.3),
                ..Default::default()
            },
            ParamSetting {
                initial: Some(1.5),
                ..Default::default()
            },
        ];
        let inp = FitInput {
            series: Series::new(x, y),
            sigma: None,
        };
        let r = fit(&[inp], &sp).unwrap();
        assert_eq!(r.dof, 1);
        assert_uncertainties_present(&r, &[1, 2, 3]);

        // Free expression: 3 points, 2 parameters.
        let x = vec![0.0, 1.0, 2.0];
        let y: Vec<f64> = x
            .iter()
            .enumerate()
            .map(|(i, &v): (usize, &f64)| 2.0 * libm::exp(-0.7 * v) + 1e-2 * noise(i + 1))
            .collect();
        let mut sp = spec(Model::Custom {
            formula: "a*exp(-b*x)".into(),
        });
        sp.params = vec![
            ParamSetting {
                initial: Some(1.5),
                ..Default::default()
            },
            ParamSetting {
                initial: Some(0.5),
                ..Default::default()
            },
        ];
        let inp = FitInput {
            series: Series::new(x, y),
            sigma: None,
        };
        let r = fit(&[inp], &sp).unwrap();
        assert_eq!(r.dof, 1);
        assert_uncertainties_present(&r, &[0, 1]);
    }

    #[test]
    fn zero_dof_with_a_fixed_parameter_and_a_custom_model() {
        // Gaussian, y0 fixed, 3 free for 3 points: dof 0.
        let x = vec![-1.0, 0.0, 1.5];
        let mut sp = spec(Model::Gaussian);
        sp.params = vec![
            fixed(0.0),
            ParamSetting {
                initial: Some(1.8),
                ..Default::default()
            },
            ParamSetting {
                initial: Some(0.2),
                ..Default::default()
            },
            ParamSetting {
                initial: Some(1.4),
                ..Default::default()
            },
        ];
        let r = fit(
            &[input(
                |v| 2.0 * libm::exp(-4.0 * libm::log(2f64) * (v - 0.3).powi(2) / 2.25),
                x,
            )],
            &sp,
        )
        .unwrap();
        assert_eq!(r.dof, 0);
        assert!(r.series[0].errors.iter().all(Option::is_none));
        assert!(r.reduced_chi2.is_nan() && r.series[0].reduced_chi2.is_nan());
        assert!(r.series[0].chi2.is_finite() && r.series[0].chi2 >= 0.0);

        // Custom, 2 points, 2 parameters.
        let mut sp = spec(Model::Custom {
            formula: "a*exp(-b*x)".into(),
        });
        sp.params = vec![
            ParamSetting {
                initial: Some(1.5),
                ..Default::default()
            },
            ParamSetting {
                initial: Some(0.5),
                ..Default::default()
            },
        ];
        let r = fit(&[input(|v| 2.0 * libm::exp(-0.7 * v), vec![0.0, 1.0])], &sp).unwrap();
        assert_eq!(r.dof, 0);
        assert!(r.series[0].errors.iter().all(Option::is_none));
        assert!(r.reduced_chi2.is_nan());
    }

    #[test]
    fn zero_dof_with_sigma_weighting_no_uncertainty() {
        let mut i = input(|v| 3.0 * libm::exp(-v / 2.0), vec![0.0, 1.0]);
        i.sigma = Some(vec![0.1, 0.2]);
        let r = fit(&[i], &spec(Model::Exponential)).unwrap();
        assert_eq!(r.dof, 0);
        assert!(r.series[0].errors.iter().all(Option::is_none));
    }

    #[test]
    fn negative_dof_refused_upstream_no_solver() {
        // n < k: NotEnoughPoints, on both paths, never an uncertainty.
        for m in [Model::Linear, Model::Exponential] {
            let e = fit(&[input(|v| 1.0 + v, vec![1.0])], &spec(m)).unwrap_err();
            assert_eq!(
                e,
                AnalysisError::NotEnoughPoints {
                    found: 1,
                    needed: 2
                }
            );
        }
        // A fixed parameter brings the need down to 1 free: 1 point is enough (dof 0).
        let mut sp = spec(Model::Exponential);
        sp.params = vec![
            fixed(3.0),
            ParamSetting {
                initial: Some(1.5),
                ..Default::default()
            },
        ];
        let r = fit(&[input(|v| 3.0 * libm::exp(-v / 2.0), vec![1.0])], &sp).unwrap();
        assert_eq!(r.dof, 0);
        assert!(r.series[0].errors.iter().all(Option::is_none));
        // Cubic on 3 points: 4 free > 3.
        let e = fit(
            &[input(|v| v, vec![0.0, 1.0, 2.0])],
            &spec(Model::Polynomial { degree: 3 }),
        )
        .unwrap_err();
        assert_eq!(
            e,
            AnalysisError::NotEnoughPoints {
                found: 3,
                needed: 4
            }
        );
    }

    #[test]
    fn global_fit_dof_on_the_total() {
        // A per curve, τ shared: 3 free. 2 curves × 3 points = 6 → dof 3.
        let x = vec![0.0, 1.0, 2.5];
        let mk = |a: f64, s: usize| {
            let y: Vec<f64> = x
                .iter()
                .enumerate()
                .map(|(i, &v): (usize, &f64)| a * libm::exp(-v / 3.0) + 1e-2 * noise(i + s))
                .collect();
            FitInput {
                series: Series::new(x.clone(), y),
                sigma: None,
            }
        };
        let mut sp = spec(Model::Exponential);
        sp.params = vec![
            ParamSetting::default(),
            ParamSetting {
                shared: true,
                ..Default::default()
            },
        ];
        let r = fit(&[mk(3.0, 1), mk(4.0, 5)], &sp).unwrap();
        assert_eq!(r.dof, 3);
        assert!(r.series.iter().all(|s| s.dof == 3));
        let chi2_total: f64 = r.series.iter().map(|s| s.chi2).sum();
        assert!((r.reduced_chi2 - chi2_total / 3.0).abs() <= 1e-9 * r.reduced_chi2);
        for s in &r.series {
            assert!(s.errors.iter().all(|e| e.is_some_and(f64::is_finite)));
        }

        // Line with shared slope: 2 intercepts + 1 slope = 3 free.
        // Curves of 2 and 1 points = 3 → dof 0, no uncertainty.
        let mut sp = spec(Model::Linear);
        sp.params = vec![
            ParamSetting::default(),
            ParamSetting {
                shared: true,
                ..Default::default()
            },
        ];
        let a = input(|v| 1.0 + 2.0 * v, vec![0.0, 1.0]);
        let b = input(|v| 5.0 + 2.0 * v, vec![2.0]);
        let r = fit(&[a.clone(), b.clone()], &sp).unwrap();
        assert_eq!(r.dof, 0);
        assert!(r.reduced_chi2.is_nan());
        for s in &r.series {
            assert_eq!(s.dof, 0);
            assert!(s.errors.iter().all(Option::is_none));
            assert!(s.chi2.is_finite());
        }
        // One more point (dof 1): uncertainties present.
        let b2 = input(|v| 5.0 + 2.0 * v, vec![2.0, 3.0]);
        let r = fit(&[a, b2], &sp).unwrap();
        assert_eq!(r.dof, 1);
        assert!(r
            .series
            .iter()
            .all(|s| s.errors.iter().all(Option::is_some)));
    }

    #[test]
    fn global_fit_fewer_points_than_free_refused() {
        // 3 free (2 A + τ), 2 curves of one point: 2 < 3.
        let mut sp = spec(Model::Exponential);
        sp.params = vec![
            ParamSetting::default(),
            ParamSetting {
                shared: true,
                ..Default::default()
            },
        ];
        let e = fit(
            &[
                input(|v| 3.0 * libm::exp(-v), vec![0.0]),
                input(|v| 4.0 * libm::exp(-v), vec![1.0]),
            ],
            &sp,
        )
        .unwrap_err();
        assert_eq!(
            e,
            AnalysisError::NotEnoughPoints {
                found: 2,
                needed: 3
            }
        );
    }

    #[test]
    fn degree_3_polynomial() {
        let i = input(
            |x| 1.0 - 2.0 * x + 0.5 * x * x + 0.1 * x * x * x,
            grid(40, -3.0, 3.0),
        );
        let r = fit(&[i], &spec(Model::Polynomial { degree: 3 })).unwrap();
        let want = [1.0, -2.0, 0.5, 0.1];
        for (a, b) in r.series[0].values.iter().zip(want) {
            assert!((a - b).abs() < 1e-9, "{a} vs {b}");
        }
    }

    #[test]
    fn logarithm_refuses_negative_x_then_fits_without_going_through_the_linear_path() {
        // x ≤ 0 present: the exact path is dropped, Levenberg-Marquardt
        // fails cleanly (non-finite model at the start).
        let i = input(|x| 2.0 + 3.0 * libm::log(x.abs()), grid(11, -5.0, 5.0));
        assert!(fit(&[i], &spec(Model::Logarithm)).is_err());
        let ok = input(|x| 2.0 + 3.0 * libm::log(x), grid(30, 0.5, 20.0));
        let r = fit(&[ok], &spec(Model::Logarithm)).unwrap();
        assert!((r.series[0].values[1] - 3.0).abs() < 1e-10);
    }

    #[test]
    fn noisy_exponential_in_the_uncertainties() {
        let x = grid(300, 0.0, 12.0);
        let noise = |i: usize| ((i as f64 * 0.618_033_988_749_895).fract() - 0.5) * 0.1;
        let y: Vec<f64> = x
            .iter()
            .enumerate()
            .map(|(i, v)| 1.0 + 4.0 * libm::exp(-v / 2.0) + noise(i))
            .collect();
        let i = FitInput {
            series: Series::new(x, y),
            sigma: None,
        };
        let r = fit(&[i], &spec(Model::ExponentialOffset)).unwrap();
        let s = &r.series[0];
        assert!((s.values[2] - 2.0).abs() < 0.1, "tau {}", s.values[2]);
        let e = s.errors[2].unwrap();
        // The gap to the true τ is of the order of the stated uncertainty.
        assert!(
            (s.values[2] - 2.0).abs() < 6.0 * e,
            "τ={} ±{e}",
            s.values[2]
        );
        assert!(s.r_squared > 0.99);
    }

    #[test]
    fn gaussian_with_a_fixed_parameter_and_bounds() {
        let x = grid(200, 0.0, 20.0);
        let i = input(
            |v| 1.0 + 5.0 * libm::exp(-4.0 * std::f64::consts::LN_2 * ((v - 8.0) / 3.0).powi(2)),
            x,
        );
        let mut sp = spec(Model::Gaussian);
        sp.params = vec![
            ParamSetting {
                fixed: true,
                initial: Some(1.0),
                ..Default::default()
            },
            ParamSetting::default(),
            ParamSetting {
                min: Some(5.0),
                max: Some(12.0),
                ..Default::default()
            },
            ParamSetting::default(),
        ];
        let r = fit(&[i], &sp).unwrap();
        let s = &r.series[0];
        assert_eq!(s.values[0], 1.0);
        assert!(s.errors[0].is_none());
        assert!((s.values[1] - 5.0).abs() < 1e-5 && (s.values[2] - 8.0).abs() < 1e-5);
        assert!((s.values[3] - 3.0).abs() < 1e-5);
    }

    #[test]
    fn free_model() {
        let x = grid(100, 0.0, 5.0);
        let i = input(|v| 2.0 * libm::sin(v * 1.3) + 0.5, x);
        let r = fit(
            &[i],
            &FitSpec {
                model: Model::Custom {
                    formula: "a*sin(w*x) + c".into(),
                },
                params: vec![
                    ParamSetting {
                        initial: Some(1.5),
                        ..Default::default()
                    },
                    ParamSetting {
                        initial: Some(1.2),
                        ..Default::default()
                    },
                    ParamSetting::default(),
                ],
            },
        )
        .unwrap();
        let v = &r.series[0].values;
        assert!(
            (v[0] - 2.0).abs() < 1e-6 && (v[1] - 1.3).abs() < 1e-6 && (v[2] - 0.5).abs() < 1e-6
        );
    }

    #[test]
    fn global_fit_shares_tau_between_three_curves() {
        let x = grid(120, 0.0, 10.0);
        let amplitudes = [2.0, 5.0, 9.0];
        let inputs: Vec<FitInput> = amplitudes
            .iter()
            .map(|a| input(|v| a * libm::exp(-v / 3.0), x.clone()))
            .collect();
        let mut sp = spec(Model::Exponential);
        sp.params = vec![
            ParamSetting::default(),
            ParamSetting {
                shared: true,
                ..Default::default()
            },
        ];
        let r = fit(&inputs, &sp).unwrap();
        for (s, a) in r.series.iter().zip(amplitudes) {
            assert!((s.values[0] - a).abs() < 1e-5, "A={}", s.values[0]);
            assert!((s.values[1] - 3.0).abs() < 1e-5);
        }
        // A single τ for the three curves.
        assert_eq!(r.series[0].values[1], r.series[2].values[1]);
        // 360 points, 4 free parameters (3 A + 1 τ).
        assert_eq!(r.dof, 356);
    }

    #[test]
    fn the_global_fit_holds_where_the_single_curves_diverge() {
        // Witness: two noisy curves whose true τ is identical; the global
        // fit has a smaller uncertainty on τ than each alone.
        let x = grid(60, 0.0, 6.0);
        let mk = |a: f64, phase: f64| {
            let y: Vec<f64> = x
                .iter()
                .enumerate()
                .map(|(i, v)| a * libm::exp(-v / 2.0) + 0.05 * ((i as f64 * phase).fract() - 0.5))
                .collect();
            FitInput {
                series: Series::new(x.clone(), y),
                sigma: None,
            }
        };
        let inputs = [mk(3.0, 0.618_033_988_7), mk(4.0, 0.414_213_562_3)];
        let alone = fit(&inputs, &spec(Model::Exponential)).unwrap();
        let mut sp = spec(Model::Exponential);
        sp.params = vec![
            ParamSetting::default(),
            ParamSetting {
                shared: true,
                ..Default::default()
            },
        ];
        let together = fit(&inputs, &sp).unwrap();
        let e_alone = alone.series[0].errors[1].unwrap();
        let e_together = together.series[0].errors[1].unwrap();
        assert!(e_together < e_alone, "{e_together} vs {e_alone}");
    }

    #[test]
    fn weighting_by_sigma() {
        let x = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let y = vec![0.0, 1.0, 2.0, 3.0, 30.0];
        let i = FitInput {
            series: Series::new(x, y),
            sigma: Some(vec![0.1, 0.1, 0.1, 0.1, 1e4]),
        };
        let r = fit(&[i], &spec(Model::Linear)).unwrap();
        assert!((r.series[0].values[1] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn invalid_sigma() {
        let i = FitInput {
            series: Series::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0]),
            sigma: Some(vec![1.0, 0.0, 1.0]),
        };
        assert!(matches!(
            fit(&[i], &spec(Model::Linear)),
            Err(AnalysisError::InvalidParameter { .. })
        ));
    }

    #[test]
    fn too_few_points() {
        let i = input(|x| x, vec![0.0, 1.0]);
        assert!(fit(&[i], &spec(Model::Polynomial { degree: 3 })).is_err());
    }

    // ---- #283: polynomial (QR) on shifted X ----

    /// Coefficients in powers of `x` of `Σ cⱼ (x - s)ʲ`, by successive multiplications:
    /// independent reference of `CompiledModel::unshift_matrix`.
    fn expand_shift(c: &[f64], s: f64) -> Vec<f64> {
        let mut out = vec![0.0; c.len()];
        let mut pow = vec![1.0];
        for cj in c {
            for (i, p) in pow.iter().enumerate() {
                out[i] += cj * p;
            }
            let mut next = vec![0.0; pow.len() + 1];
            for (i, p) in pow.iter().enumerate() {
                next[i + 1] += p;
                next[i] -= s * p;
            }
            pow = next;
        }
        out
    }

    /// Value of `Σ cⱼ tʲ` (Horner), in the centerd axis.
    fn horner(c: &[f64], t: f64) -> f64 {
        c.iter().rev().fold(0.0, |a, cj| a * t + cj)
    }

    /// `(AᵀA)⁻¹` of the basis `1, t, …, tᵈ`, by Gauss-Jordan (reference).
    fn raw_covariance(t: &[f64], d: usize) -> Vec<Vec<f64>> {
        let k = d + 1;
        let mut m = vec![vec![0.0; 2 * k]; k];
        for i in 0..k {
            for j in 0..k {
                m[i][j] = t.iter().map(|v| v.powi((i + j) as i32)).sum();
            }
            m[i][k + i] = 1.0;
        }
        for c in 0..k {
            let p = (c..k)
                .max_by(|&a, &b| m[a][c].abs().total_cmp(&m[b][c].abs()))
                .unwrap();
            m.swap(c, p);
            let piv = m[c][c];
            for v in m[c].iter_mut() {
                *v /= piv;
            }
            for r in 0..k {
                if r != c {
                    let f = m[r][c];
                    for j in 0..2 * k {
                        let u = m[c][j];
                        m[r][j] -= f * u;
                    }
                }
            }
        }
        m.into_iter().map(|r| r[k..].to_vec()).collect()
    }

    /// Variances of the coefficients in the original axis: diag(T·cov·Tᵀ), `T` being
    /// built column by column by `expand_shift`.
    fn original_variances(cov: &[Vec<f64>], s: f64) -> Vec<f64> {
        let k = cov.len();
        let cols: Vec<Vec<f64>> = (0..k)
            .map(|j| {
                let mut e = vec![0.0; k];
                e[j] = 1.0;
                expand_shift(&e, s)
            })
            .collect();
        (0..k)
            .map(|i| {
                let mut v = 0.0;
                for j in 0..k {
                    for l in 0..k {
                        v += cols[j][i] * cols[l][i] * cov[j][l];
                    }
                }
                v
            })
            .collect()
    }

    /// Series at X = off + t, where `t` is recomputed *exactly* from the rounded
    /// X (Sterbenz): the data are then an exact polynomial of X - off.
    fn shifted_series(off: f64, t: &[f64], f: impl Fn(usize, f64) -> f64) -> (FitInput, Vec<f64>) {
        let x: Vec<f64> = t.iter().map(|v| off + v).collect();
        let tt: Vec<f64> = x.iter().map(|v| v - off).collect();
        let y = tt.iter().enumerate().map(|(i, &v)| f(i, v)).collect();
        (
            FitInput {
                series: Series::new(x, y),
                sigma: None,
            },
            tt,
        )
    }

    const COEFFS: [f64; 6] = [1.0, -2.0, 0.5, 1.5, -0.7, 0.3];

    #[test]
    fn exact_polynomial_degrees_1_to_5_on_shifted_x() {
        // Max residual < 1e-10 (chi2 < 1e-20), coefficients re-expressed in
        // the original axis to 1e-8 relative, exact path (no LM iteration).
        let t = grid(21, -2.0, 2.0);
        for d in 1..=5usize {
            let c = &COEFFS[..=d];
            for off in [0.0, 1e2, 1e5, 1e6] {
                let (i, _) = shifted_series(off, &t, |_, v| horner(c, v));
                let r = fit(&[i], &spec(Model::Polynomial { degree: d as u32 })).unwrap();
                let s = &r.series[0];
                let ctx = format!("degree {d}, shift {off:e}");
                assert_eq!(r.iterations, 1, "{ctx}: QR path expected");
                assert_eq!(s.dof, 21 - d - 1, "{ctx}");
                assert!(s.chi2 < 1e-20, "{ctx} : chi2 = {:e}", s.chi2);
                assert!((s.r_squared - 1.0).abs() < 1e-13, "{ctx}");
                let want = expand_shift(c, off);
                for (j, (g, w)) in s.values.iter().zip(&want).enumerate() {
                    assert!(
                        (g - w).abs() <= 1e-8 * w.abs(),
                        "{ctx}: a{j} = {g:e} against {w:e}"
                    );
                }
                assert!(
                    s.errors.iter().all(|e| e.is_some_and(f64::is_finite)),
                    "{ctx} : {:?}",
                    s.errors
                );
            }
        }
    }

    #[test]
    fn noisy_polynomial_coefficients_and_errors_invariant_by_translation_of_x() {
        // Reference: same centerd set (X = t). Shifted by S, the polynomial is the
        // same in X - S: coefficients = expansion, errors = diag(T·cov·Tᵀ)
        // computed by hand (cov inverted by Gauss-Jordan on the basis of t).
        let t = grid(25, -2.0, 2.0);
        let noise = |i: usize| 0.05 * libm::sin((i as f64) * 1.7);
        for d in 1..=5usize {
            let c = &COEFFS[..=d];
            let model = Model::Polynomial { degree: d as u32 };
            let (i0, t0) = shifted_series(0.0, &t, |i, v| horner(c, v) + noise(i));
            let r0 = fit(&[i0], &spec(model.clone())).unwrap();
            let a = &r0.series[0];
            let cov = raw_covariance(&t0, d);
            for off in [1e2, 1e5, 1e6] {
                let (i1, _) = shifted_series(off, &t, |i, v| horner(c, v) + noise(i));
                let r1 = fit(&[i1], &spec(model.clone())).unwrap();
                let b = &r1.series[0];
                let ctx = format!("degree {d}, shift {off:e}");
                // Quality statistics unchanged.
                assert!((b.chi2 - a.chi2).abs() <= 1e-6 * a.chi2, "{ctx}");
                assert!((b.reduced_chi2 - a.reduced_chi2).abs() <= 1e-6 * a.reduced_chi2);
                assert!((b.r_squared - a.r_squared).abs() < 1e-9, "{ctx}");
                assert_eq!((b.dof, b.points), (a.dof, a.points), "{ctx}");
                // Coefficients = those of the centerd set, re-expressed.
                let want = expand_shift(&a.values, off);
                for (j, (g, w)) in b.values.iter().zip(&want).enumerate() {
                    assert!(
                        (g - w).abs() <= 1e-6 * w.abs(),
                        "{ctx}: a{j} = {g:e} against {w:e}"
                    );
                }
                // Errors = root of scale · diag(T·cov·Tᵀ).
                let var = original_variances(&cov, off);
                for (j, (e, v)) in b.errors.iter().zip(&var).enumerate() {
                    let w = (v * a.reduced_chi2).sqrt();
                    let g = e.unwrap();
                    assert!(
                        (g - w).abs() <= 1e-5 * w,
                        "{ctx}: error a{j} = {g:e} against {w:e}"
                    );
                }
                // The leading coefficient does not move with the axis: same error.
                let (ga, gb) = (a.errors[d].unwrap(), b.errors[d].unwrap());
                assert!((ga - gb).abs() <= 1e-6 * ga, "{ctx}: leading");
            }
        }
    }

    /// Linear regression by hand: (slope, intercept, errors
    /// [intercept, slope]) for weights `w` (1 without weighting).
    fn hand_line(x: &[f64], y: &[f64], w: &[f64]) -> ([f64; 2], [f64; 2], f64) {
        let sw: f64 = w.iter().sum();
        let xb = x.iter().zip(w).map(|(x, w)| x * w).sum::<f64>() / sw;
        let yb = y.iter().zip(w).map(|(y, w)| y * w).sum::<f64>() / sw;
        let sxx: f64 = x.iter().zip(w).map(|(x, w)| w * (x - xb).powi(2)).sum();
        let sxy: f64 = (0..x.len()).map(|i| w[i] * (x[i] - xb) * (y[i] - yb)).sum();
        let b = sxy / sxx;
        let a = yb - b * xb;
        let chi2: f64 = (0..x.len())
            .map(|i| w[i] * (y[i] - a - b * x[i]).powi(2))
            .sum();
        let scale = chi2 / (x.len() - 2) as f64;
        (
            [a, b],
            [
                (scale * (1.0 / sw + xb * xb / sxx)).sqrt(),
                (scale / sxx).sqrt(),
            ],
            chi2,
        )
    }

    #[test]
    fn noisy_line_values_and_errors_against_the_hand_regression() {
        // Standard deviation of the intercept = s·sqrt(1/n + x̄²/Sxx): the term
        // x̄² is precisely what the T·cov·Tᵀ propagation must restore.
        let t = grid(15, -3.0, 3.0);
        let noise = |i: usize| 0.2 * libm::cos((i as f64) * 2.3);
        for off in [0.0, 5.0, 1e3, 1e5, 1e6] {
            let (i, tt) = shifted_series(off, &t, |k, v| 3.0 - 0.4 * v + noise(k));
            let x = i.series.x.clone();
            let y: Vec<f64> = tt
                .iter()
                .enumerate()
                .map(|(k, v)| 3.0 - 0.4 * v + noise(k))
                .collect();
            let w = vec![1.0; x.len()];
            // The hand reference works in the centerd axis then re-shifts: we
            // give it t for precision and convert the intercept.
            let (c, _, chi2) = hand_line(&tt, &y, &w);
            let r = fit(&[i], &spec(Model::Linear)).unwrap();
            let s = &r.series[0];
            let ctx = format!("shift {off:e}");
            let (a0, a1) = (c[0] - off * c[1], c[1]);
            assert!((s.values[1] - a1).abs() <= 1e-9 * a1.abs(), "{ctx}");
            assert!(
                (s.values[0] - a0).abs() <= 1e-9 * a0.abs().max(1.0),
                "{ctx}"
            );
            assert!((s.chi2 - chi2).abs() <= 1e-9 * chi2, "{ctx}");
            // Errors in the original axis: x̄ = off + mean(t).
            let (_, e_o, _) = hand_line(&x, &y, &w);
            assert!(
                (s.errors[1].unwrap() - e_o[1]).abs() <= 1e-6 * e_o[1],
                "{ctx}"
            );
            assert!(
                (s.errors[0].unwrap() - e_o[0]).abs() <= 1e-6 * e_o[0],
                "{ctx}"
            );
        }
    }

    #[test]
    fn line_weighted_by_sigma_on_shifted_x() {
        let t = grid(12, -3.0, 3.0);
        let sig: Vec<f64> = (0..12).map(|i| 0.1 + 0.05 * i as f64).collect();
        let noise = |i: usize| 0.2 * libm::sin((i as f64) * 1.1);
        let w: Vec<f64> = sig.iter().map(|s| 1.0 / (s * s)).collect();
        for off in [0.0, 1e4, 1e6] {
            let (mut i, tt) = shifted_series(off, &t, |k, v| 1.0 + 0.5 * v + noise(k));
            i.sigma = Some(sig.clone());
            let y = i.series.y.clone();
            let (c, e_t, chi2) = hand_line(&tt, &y, &w);
            let r = fit(&[i.clone()], &spec(Model::Linear)).unwrap();
            let s = &r.series[0];
            let ctx = format!("shift {off:e}");
            assert!((s.values[1] - c[1]).abs() <= 1e-9 * c[1].abs(), "{ctx}");
            assert!((s.chi2 - chi2).abs() <= 1e-9 * chi2, "{ctx}");
            // Slope error independent of the axis; the intercept one follows
            // s·sqrt(1/Σw + x̄_w²/Sxx) with weighted x̄_w, in the original axis.
            assert!(
                (s.errors[1].unwrap() - e_t[1]).abs() <= 1e-6 * e_t[1],
                "{ctx}"
            );
            let x = &i.series.x;
            let (_, e_o, _) = hand_line(x, &y, &w);
            assert!(
                (s.errors[0].unwrap() - e_o[0]).abs() <= 1e-6 * e_o[0],
                "{ctx}"
            );
            // Doubling all the σ: same coefficients and same errors
            // (estimated residual variance), χ² divided by 4.
            let mut j = i.clone();
            j.sigma = Some(sig.iter().map(|v| 2.0 * v).collect());
            let r2 = fit(&[j], &spec(Model::Linear)).unwrap();
            let s2 = &r2.series[0];
            assert!((s2.chi2 - s.chi2 / 4.0).abs() <= 1e-9 * s.chi2, "{ctx}");
            assert!((s2.values[1] - s.values[1]).abs() <= 1e-9 * s.values[1].abs());
            assert!(
                (s2.errors[0].unwrap() - s.errors[0].unwrap()).abs() <= 1e-6 * s.errors[0].unwrap()
            );
        }
    }

    #[test]
    fn weighted_polynomial_on_shifted_x_same_coefficients_as_centerd() {
        let t = grid(14, -2.0, 2.0);
        let sig: Vec<f64> = (0..14).map(|i| 0.05 + 0.02 * i as f64).collect();
        let noise = |i: usize| 0.04 * libm::sin((i as f64) * 0.9);
        let c = &COEFFS[..=3];
        let m = Model::Polynomial { degree: 3 };
        let (mut i0, _) = shifted_series(0.0, &t, |k, v| horner(c, v) + noise(k));
        i0.sigma = Some(sig.clone());
        let a = fit(&[i0], &spec(m.clone())).unwrap();
        let (mut i1, _) = shifted_series(1e5, &t, |k, v| horner(c, v) + noise(k));
        i1.sigma = Some(sig);
        let b = fit(&[i1], &spec(m)).unwrap();
        let want = expand_shift(&a.series[0].values, 1e5);
        for (g, w) in b.series[0].values.iter().zip(&want) {
            assert!((g - w).abs() <= 1e-6 * w.abs(), "{g:e} against {w:e}");
        }
        assert!((b.series[0].chi2 - a.series[0].chi2).abs() <= 1e-6 * a.series[0].chi2);
    }

    #[test]
    fn independent_curves_with_different_x_each_have_their_own_shift() {
        // Without a shared parameter, each curve goes through fit_linear with the
        // mean of *its* X: the result is that of the isolated fit.
        let t = grid(15, -3.0, 3.0);
        let mk = |off: f64, c0: f64| {
            shifted_series(off, &t, |k, v| {
                c0 + 0.5 * v - 0.2 * v * v + 0.01 * libm::sin(k as f64)
            })
            .0
        };
        let (a, b) = (mk(0.0, 1.0), mk(1e5, 4.0));
        let m = Model::Polynomial { degree: 2 };
        let sa = fit(std::slice::from_ref(&a), &spec(m.clone())).unwrap();
        let sb = fit(std::slice::from_ref(&b), &spec(m.clone())).unwrap();
        let both = fit(&[a, b], &spec(m)).unwrap();
        assert_eq!(both.series[0], sa.series[0]);
        assert_eq!(both.series[1], sb.series[0]);
        assert_eq!(both.dof, sa.dof + sb.dof);
    }

    #[test]
    fn global_fit_of_lines_with_different_x_keeps_the_right_coefficients() {
        // The global goes through Levenberg-Marquardt, without centering: we pin
        // that shared slope and intercepts stay exact for a line.
        let t = grid(21, -5.0, 5.0);
        for off in [1e2, 1e5, 1e6] {
            let (a, _) = shifted_series(off, &t, |_, v| 1.0 + 2.0 * v);
            let (b, _) = shifted_series(2.0 * off, &t, |_, v| 5.0 + 2.0 * v);
            let mut sp = spec(Model::Linear);
            sp.params = vec![
                ParamSetting::default(),
                ParamSetting {
                    shared: true,
                    ..Default::default()
                },
            ];
            let r = fit(&[a, b], &sp).unwrap();
            let want = [(1.0 - 2.0 * off), (5.0 - 4.0 * off)];
            for (s, w) in r.series.iter().zip(want) {
                assert!((s.values[1] - 2.0).abs() < 1e-8, "slope {}", s.values[1]);
                assert!((s.values[0] - w).abs() <= 1e-8 * w.abs(), "{off:e}");
            }
            assert_eq!(r.dof, 42 - 3);
        }
    }

    #[test]
    fn constrained_polynomial_on_shifted_x_accurate_if_moderate_and_flagged_when_it_degrades() {
        // A constrained polynomial (fixed, initial value) goes through LM, without
        // centering: its precision degrades when X moves away from 0. Pinned:
        // (1) moderate shift, accurate result; (2) otherwise, a result whose
        // χ² exceeds 100 times that of the centerd set always carries a
        // warning (plateau or undetermined parameters), never silent.
        let t = grid(21, -5.0, 5.0);
        let noise = |i: usize| 0.01 * libm::sin((i as f64) * 1.7);
        for d in 2..=4usize {
            let c: Vec<f64> = (0..=d).map(|j| 1.0 / (1.0 + j as f64)).collect();
            let top = c[d];
            for fixed in [false, true] {
                let mut sp = spec(Model::Polynomial { degree: d as u32 });
                sp.params = vec![ParamSetting::default(); d];
                sp.params.push(ParamSetting {
                    fixed,
                    initial: Some(top),
                    ..Default::default()
                });
                let (i0, _) = shifted_series(0.0, &t, |k, v| horner(&c, v) + noise(k));
                let ref0 = fit(&[i0], &sp).unwrap().series.remove(0);
                for off in [1e2, 1e3, 1e4, 1e5, 1e6] {
                    let (i, _) = shifted_series(off, &t, |k, v| horner(&c, v) + noise(k));
                    let s = fit(&[i], &sp).unwrap().series.remove(0);
                    let ctx = format!("degree {d}, fixed {fixed}, shift {off:e}");
                    assert!(s.values.iter().all(|v| v.is_finite()), "{ctx}");
                    if s.chi2 > 100.0 * ref0.chi2 {
                        assert!(
                            s.plateau || s.undetermined > 0,
                            "{ctx}: chi2 = {:e}",
                            s.chi2
                        );
                    }
                    if off <= 1e2 && d <= 3 {
                        let want = expand_shift(&ref0.values, off);
                        for (g, w) in s.values.iter().zip(&want) {
                            assert!((g - w).abs() <= 1e-5 * w.abs(), "{ctx}: {g:e} / {w:e}");
                        }
                    }
                }
            }
        }
    }

    /// #296: a constrained polynomial (LM, uncenterd) on an X very far from 0
    /// is flagged; centerd, moderately shifted or unconstrained (centerd QR),
    /// it is not.
    #[test]
    fn constrained_polynomial_on_very_shifted_x_is_flagged() {
        let t = grid(21, -5.0, 5.0);
        let c = [1.0, 0.5, 0.25];
        let constrained = |fixed_top: bool| {
            let mut sp = spec(Model::Polynomial { degree: 2 });
            sp.params = vec![ParamSetting::default(); 2];
            sp.params.push(ParamSetting {
                fixed: fixed_top,
                initial: Some(c[2]),
                ..Default::default()
            });
            sp
        };
        let flagged = |off: f64, sp: &FitSpec| {
            let (i, _) = shifted_series(off, &t, |_, v| horner(&c, v));
            fit(&[i], sp).unwrap().series.remove(0).shifted_x
        };
        assert!(flagged(1e4, &constrained(true)), "constrained, X ≈ 1e4");
        assert!(flagged(1e4, &constrained(false)), "initial value only");
        assert!(!flagged(0.0, &constrained(true)), "witness: centerd X");
        assert!(!flagged(1e2, &constrained(true)), "witness: moderate shift");
        let mut free_spec = spec(Model::Polynomial { degree: 2 });
        free_spec.params = vec![ParamSetting::default(); 3];
        assert!(
            !flagged(1e4, &free_spec),
            "witness: unconstrained (centerd QR)"
        );
    }

    #[test]
    fn all_x_equal_never_a_silent_nan() {
        // Rank deficient: the QR refuses (column x - mean zero), the fit
        // falls back on Levenberg-Marquardt. Pinned: no error, finite
        // values, the line goes through the mean of Y, and the defect is flagged
        // (plateau, undetermined parameter), with no NaN in the uncertainties.
        for x0 in [0.0, 1e5] {
            let y = vec![0.0, 1.0, 2.0];
            let i = FitInput {
                series: Series::new(vec![x0; 3], y),
                sigma: None,
            };
            let r = fit(&[i], &spec(Model::Linear)).unwrap();
            let s = &r.series[0];
            assert!(s.values.iter().all(|v| v.is_finite()), "x0 = {x0}");
            assert!(s.plateau && s.undetermined >= 1, "x0 = {x0} : {s:?}");
            assert!(
                s.errors.iter().flatten().all(|e| e.is_finite()),
                "x0 = {x0} : {:?}",
                s.errors
            );
            let at = s.values[0] + s.values[1] * x0;
            assert!((at - 1.0).abs() < 1e-6, "fitted value at x0: {at}");
        }
        // Polynomial degree 2, 3 coinciding points: dof 0, no uncertainty.
        let i = FitInput {
            series: Series::new(vec![1e5; 3], vec![0.0, 1.0, 2.0]),
            sigma: None,
        };
        let r = fit(&[i], &spec(Model::Polynomial { degree: 2 })).unwrap();
        let s = &r.series[0];
        assert!(s.values.iter().all(|v| v.is_finite()));
        assert_eq!(r.dof, 0);
        assert!(s.errors.iter().all(Option::is_none));
        assert!(s.plateau && s.undetermined >= 1);
    }

    #[test]
    fn n_equal_degree_plus_one_on_shifted_x_interpolates_without_uncertainty() {
        // dof = 0: exact interpolation, no uncertainty, reduced χ² NaN.
        for off in [0.0, 1e5, 1e6] {
            let x = vec![off - 1.0, off, off + 2.0];
            let c = [2.0, -1.0, 0.5];
            let y: Vec<f64> = x.iter().map(|v| horner(&c, v - off)).collect();
            let i = FitInput {
                series: Series::new(x, y),
                sigma: None,
            };
            let r = fit(&[i], &spec(Model::Polynomial { degree: 2 })).unwrap();
            let s = &r.series[0];
            let want = expand_shift(&c, off);
            for (g, w) in s.values.iter().zip(&want) {
                assert!((g - w).abs() <= 1e-8 * w.abs(), "{off:e} : {g:e} / {w:e}");
            }
            assert_eq!((s.dof, r.dof, s.points), (0, 0, 3));
            assert!(s.errors.iter().all(Option::is_none), "{:?}", s.errors);
            assert!(s.reduced_chi2.is_nan() && r.reduced_chi2.is_nan());
            assert!(s.chi2 < 1e-18);
        }
        // Line through two points at X = 1e6.
        let i = FitInput {
            series: Series::new(vec![1e6, 1e6 + 1.0], vec![1.0, 3.0]),
            sigma: None,
        };
        let r = fit(&[i], &spec(Model::Linear)).unwrap();
        let s = &r.series[0];
        assert!((s.values[1] - 2.0).abs() < 1e-9);
        assert!((s.values[0] - (1.0 - 2.0e6)).abs() < 1e-3);
        assert!(s.errors.iter().all(Option::is_none) && s.dof == 0);
    }

    #[test]
    fn a_single_point_on_shifted_x_is_refused() {
        for (m, needed) in [(Model::Linear, 2), (Model::Polynomial { degree: 2 }, 3)] {
            let i = FitInput {
                series: Series::new(vec![1e5], vec![1.0]),
                sigma: None,
            };
            assert!(matches!(
                fit(&[i], &spec(m)),
                Err(AnalysisError::NotEnoughPoints { found: 1, needed: n }) if n == needed
            ));
        }
    }

    #[test]
    fn x_centerd_at_zero_reference_values_unchanged() {
        // Non-regression: symmetric X (mean exactly 0, no shift),
        // values computed outside the repository (least squares, numpy).
        let x = vec![-2.0, -1.0, 0.0, 1.0, 2.0];
        let y = vec![4.1, 1.9, 1.2, 1.8, 4.2];
        let mk = || FitInput {
            series: Series::new(x.clone(), y.clone()),
            sigma: None,
        };
        let close = |g: f64, w: f64| (g - w).abs() <= 1e-12 * (1.0 + w.abs());
        let r = fit(&[mk()], &spec(Model::Polynomial { degree: 2 })).unwrap();
        let s = &r.series[0];
        for (g, w) in s.values.iter().zip([1.14, 0.01, 0.75]) {
            assert!(close(*g, w), "{g} against {w}");
        }
        assert!(close(s.chi2, 0.016) && s.dof == 2);
        let scale = 0.016 / 2.0;
        let want = [
            (scale * 34.0 / 70.0_f64).sqrt(),
            (scale / 10.0_f64).sqrt(),
            (scale * 5.0 / 70.0_f64).sqrt(),
        ];
        for (e, w) in s.errors.iter().zip(want) {
            assert!(close(e.unwrap(), w), "{e:?} against {w}");
        }
        let r = fit(&[mk()], &spec(Model::Linear)).unwrap();
        let s = &r.series[0];
        assert!(close(s.values[0], 2.64) && close(s.values[1], 0.01));
        assert!(close(s.chi2, 7.891));
        let scale = 7.891 / 3.0;
        assert!(close(s.errors[0].unwrap(), (scale / 5.0_f64).sqrt()));
        assert!(close(s.errors[1].unwrap(), (scale / 10.0_f64).sqrt()));
    }
}
