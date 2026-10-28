//! Non-linear least-squares engine (Levenberg-Marquardt) with **parameter
//! slots**, shared by the fit, the global fit, the
//! peak decomposition and the reconvolution (#188, #189, #190).
//!
//! A problem is a list of **terms** (a measured curve and a function
//! that models it) that read the same vector of slots. A slot shared by
//! several terms is a global parameter (M13 b); a slot read by two
//! peaks is a linked width (#189). The engine knows neither model nor peak.
//!
//! Derivatives by central differences (step relative to the parameter, **refined** as long
//! as the derivatives at `h` and `h/4` diverge beyond rounding, #294): free
//! models have no analytic derivative, and a single calculation path
//! is better than two (built-in model, free model). Bounds are enforced
//! by projecting the step.
//!
//! Shifted X (#282, #293, #294): three guards hold an X of large
//! absolute value. The Levenberg-Marquardt step goes through augmented
//! least squares; a tiny step concludes convergence only after an
//! almost undamped step (Marquardt damping restrains the weak direction
//! of nearly collinear columns); a column is undetermined only below
//! `DETERMINED_MIN_SHARE` of its norm.

use crate::error::AnalysisError;
use crate::linalg::{invert, least_squares, Matrix};

/// A parameter slot.
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub name: String,
    pub initial: f64,
    pub fixed: bool,
    pub min: f64,
    pub max: f64,
}

impl Slot {
    pub fn new(name: impl Into<String>, initial: f64) -> Self {
        Self {
            name: name.into(),
            initial,
            fixed: false,
            min: f64::NEG_INFINITY,
            max: f64::INFINITY,
        }
    }
}

/// Function that fills a buffer with the model evaluated at the X of a term.
pub type TermEval<'a> = Box<dyn Fn(&[f64], &mut [f64]) + 'a>;

/// A measured curve and the model that reproduces it.
pub struct Term<'a> {
    pub x: &'a [f64],
    pub y: &'a [f64],
    /// Standard deviations of Y (weighting `1/σ`). `None`: equal weights.
    pub sigma: Option<&'a [f64]>,
    /// Fills `out` (length of `x`) with the model evaluated at the X, from the
    /// values of all the slots.
    pub eval: TermEval<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    pub max_iterations: usize,
    /// Scaling of the errors by the reduced χ² (like `absolute_sigma =
    /// False` of scipy). Without weighting, this is always the case.
    pub scale_errors: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_iterations: 400,
            scale_errors: true,
        }
    }
}

/// Quality of the fit of a term.
#[derive(Debug, Clone, PartialEq)]
pub struct TermStats {
    pub points: usize,
    /// Sum of the squares of the residuals, unweighted.
    pub ss_res: f64,
    /// `1 − SSres/SStot` (unweighted); `NaN` if Y is constant.
    pub r_squared: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Solution {
    pub values: Vec<f64>,
    /// Standard uncertainty of each slot; `None` for a fixed slot or if the
    /// covariance cannot be computed.
    pub errors: Vec<Option<f64>>,
    /// Free slot that the data do not determine (zero Jacobian column
    /// or combination of the previous ones, #203): its uncertainty is `None`.
    pub undetermined: Vec<bool>,
    /// Weighted sum of the squares of the residuals.
    pub chi2: f64,
    pub dof: usize,
    pub reduced_chi2: f64,
    pub iterations: usize,
    pub terms: Vec<TermStats>,
    /// The solver stopped on a **plateau**, not on a minimum (#281):
    /// free slots with no effect on the model (zero gradient) while the
    /// residual stays large, e.g. a peak that started outside the data. The solution is
    /// returned (never a hard error: it would lose the result) but must
    /// not be read as a converged fit.
    pub plateau: bool,
    /// The solver could accept **no** step while the slope stayed
    /// above `STALL_COSINE` (C1, #313): the starting point is returned
    /// as is, with its R², without being a minimum. Returned and reported like the
    /// plateau, never as a hard error nor silently.
    pub stalled: bool,
}

/// Relaxed slope threshold (residual/column cosine) below which a stop
/// without an accepted step remains a reached minimum: looser than the `1e-9` of
/// the gradient stop, because the last steps of a true minimum hit
/// rounding before reaching this threshold.
const STALL_COSINE: f64 = 1e-6;

/// Share of the energy of Y (weighted like the residual) below which χ² is
/// rounding noise: an exact fit (noise-free data) has no slope left
/// to read, its cosine is ~0.05 without meaning anything.
const EXACT_FIT_FRACTION: f64 = 1e-20;

/// Share of the energy of Y below which χ² can only decrease by
/// rounding: a residual of the order of the `f64` epsilon (~1e-16 of Y, i.e.
/// ~1e-32 squared) on noise-free data. Beyond this floor
/// (1e-28, a few epsilons) each step "improves" χ² by a few ulp without
/// the relative criteria (`drop`, `moved`) firing, and
/// convergence depended on the last ulp of the platform's `exp`/`ln` (#332).
/// The solver has then arrived: this is not a plateau, the residual is zero.
const ROUNDING_FLOOR_FRACTION: f64 = 1e-28;

/// λ of the verification step (#293): Marquardt damping follows the
/// diagonal of `JᵀJ`, which is 1e15 and more for an X shifted by 1e7; below this
/// floor it still restrains the weak direction, so it is almost removed.
const UNDAMPED_LAMBDA: f64 = 1e-30;

/// Share of the variance of Y (`SStot`) that the residual (`SSres`) must exceed,
/// on at least one curve, for a stop with no-effect slots to be a
/// plateau and not an over-parameterized model that fits well (R² < 0.5). Conservative
/// threshold: a local minimum of noisy data stays far from it.
const PLATEAU_RESIDUAL_FRACTION: f64 = 0.5;

fn residuals(terms: &[Term<'_>], values: &[f64], buf: &mut Vec<f64>, out: &mut Vec<f64>) {
    out.clear();
    for t in terms {
        buf.clear();
        buf.resize(t.x.len(), 0.0);
        (t.eval)(values, buf);
        for (i, f) in buf.iter().enumerate() {
            let w = t.sigma.map_or(1.0, |s| 1.0 / s[i]);
            out.push((t.y[i] - f) * w);
        }
    }
}

fn sum_sq(r: &[f64]) -> f64 {
    r.iter().map(|v| v * v).sum()
}

fn clamp_slot(v: f64, s: &Slot) -> f64 {
    v.max(s.min).min(s.max)
}

/// Fits the free slots of `slots` to the `terms`.
pub fn solve_problem(
    slots: &[Slot],
    terms: &[Term<'_>],
    opts: Options,
) -> Result<Solution, AnalysisError> {
    for s in slots {
        if s.min > s.max {
            return Err(AnalysisError::EmptyBounds {
                name: s.name.clone(),
                min: s.min,
                max: s.max,
            });
        }
    }
    let free: Vec<usize> = (0..slots.len()).filter(|&i| !slots[i].fixed).collect();
    if free.is_empty() {
        return Err(AnalysisError::NoFreeParameter);
    }
    let m: usize = terms.iter().map(|t| t.x.len()).sum();
    if m < free.len() {
        return Err(AnalysisError::NotEnoughPoints {
            found: m,
            needed: free.len(),
        });
    }
    let mut values: Vec<f64> = slots.iter().map(|s| clamp_slot(s.initial, s)).collect();
    let (mut buf, mut r) = (Vec::new(), Vec::new());
    residuals(terms, &values, &mut buf, &mut r);
    let mut chi2 = sum_sq(&r);
    if !chi2.is_finite() {
        return Err(AnalysisError::NonFiniteModel);
    }

    // Energy of Y, weighted like the residual: the scale of the two exact-fit
    // thresholds (`ROUNDING_FLOOR_FRACTION`, `EXACT_FIT_FRACTION`).
    let energy: f64 = terms
        .iter()
        .map(|t| {
            t.y.iter()
                .enumerate()
                .map(|(i, y)| {
                    let w = t.sigma.map_or(1.0, |s| 1.0 / s[i]);
                    (y * w) * (y * w)
                })
                .sum::<f64>()
        })
        .sum();

    let mut lambda = 1e-3;
    let mut iterations = 0;
    let mut converged = false;
    let mut stalled = false;
    // A tiny step is proof of convergence only if it is not tiny
    // because of the damping (#293): we check it with an almost
    // undamped step before concluding.
    let mut undamped_tried = false;
    let mut jac = Matrix::zeros(m, free.len());
    while iterations < opts.max_iterations {
        iterations += 1;
        jacobian(terms, &values, slots, &free, &mut jac, &mut buf);
        let a = jac.gram();
        let g = jac.transpose_mul(&r);
        // A non-finite value in the gradient or the Gram matrix would escape the `max` fold
        // below (which ignores NaN): the solver would conclude "converged"
        // on a NaN Jacobian (C6, #313). Test before the fold.
        if g.iter().any(|v| !v.is_finite()) || !(0..free.len()).all(|k| a.get(k, k).is_finite()) {
            return Err(AnalysisError::ModelNotEvaluable);
        }
        // Gradient criterion **independent of the scale** of the data (the
        // `gtol` of MINPACK): cosine of the angle between the residual and each
        // Jacobian column. An absolute threshold stopped the fit at the
        // first step when Y is 1e-12 (amperes, cross sections).
        let residual_norm = chi2.sqrt();
        let cosine = (0..free.len()).fold(0.0_f64, |acc, k| {
            let norm = a.get(k, k).sqrt();
            if norm > 0.0 && residual_norm > 0.0 {
                acc.max(g[k].abs() / (norm * residual_norm))
            } else {
                acc
            }
        });
        if residual_norm == 0.0 || cosine < 1e-9 || chi2 <= ROUNDING_FLOOR_FRACTION * energy {
            converged = true;
            break;
        }

        let mut improved = false;
        for _ in 0..40 {
            // Levenberg-Marquardt step by **augmented** least squares
            // (QR of [J; √(λD)] against [−r; 0]) and not by the normal
            // equations: JᵀJ squares the condition number, and two
            // nearly collinear columns (X shifted by 1e6, #282) lost all
            // precision there. Marquardt: the damping follows the diagonal
            // of each parameter, with no absolute floor (the Jacobian
            // columns can differ by twenty orders of magnitude); a zero
            // column takes 1.
            let n = free.len();
            let mut aug = Matrix::zeros(m + n, n);
            let mut rhs = vec![0.0; m + n];
            for row in 0..m {
                for k in 0..n {
                    aug.set(row, k, jac.get(row, k));
                }
                rhs[row] = -r[row];
            }
            for i in 0..n {
                let d = if a.get(i, i) > 0.0 { a.get(i, i) } else { 1.0 };
                aug.set(m + i, i, (lambda * d).sqrt());
            }
            let Some(step) = least_squares(&aug, &rhs).map(|ls| ls.coefficients) else {
                lambda *= 10.0;
                continue;
            };
            let mut trial = values.clone();
            let mut moved = 0.0_f64;
            for (k, &i) in free.iter().enumerate() {
                trial[i] = clamp_slot(values[i] + step[k], &slots[i]);
                moved = moved.max((trial[i] - values[i]).abs() / (values[i].abs() + 1e-12));
            }
            let mut rt = Vec::new();
            residuals(terms, &trial, &mut buf, &mut rt);
            let chi2_trial = sum_sq(&rt);
            if chi2_trial.is_finite() && chi2_trial <= chi2 {
                let drop = chi2 - chi2_trial;
                values = trial;
                r = rt;
                chi2 = chi2_trial;
                // Very low floor: at 1e-12, the damping restrained the bottom
                // of a narrow valley (nearly collinear columns, #282).
                lambda = (lambda / 3.0).max(1e-20);
                improved = true;
                if drop <= 1e-14 * chi2.max(1e-300) || moved < 1e-11 {
                    if undamped_tried {
                        converged = true;
                    } else {
                        undamped_tried = true;
                        lambda = UNDAMPED_LAMBDA;
                    }
                } else {
                    undamped_tried = false;
                }
                break;
            }
            lambda *= 5.0;
            if lambda > 1e14 {
                break;
            }
        }
        if !improved {
            // No step lowers χ² any more: local minimum reached if the
            // slope is (almost) zero; otherwise the solver is stalled (C1) and the
            // solution is returned flagged, never presented as converged.
            converged = true;
            // As no step was accepted, `a`, `g` and `residual_norm` are
            // those of this point. A parameter stuck on its bound does not belong here:
            // its projected step no longer moves, `chi2_trial <= chi2` accepts it
            // and the `moved < 1e-11` stop declares it converged, without a warning.
            // A residual at the rounding level (exact fit) has no readable
            // slope: its cosine is only noise, never a stall.
            let exact = chi2 <= EXACT_FIT_FRACTION * energy;
            stalled = !exact
                && (0..free.len()).any(|k| {
                    let norm = a.get(k, k).sqrt();
                    norm > 0.0
                        && residual_norm > 0.0
                        && g[k].abs() / (norm * residual_norm) >= STALL_COSINE
                });
            break;
        }
        if converged {
            break;
        }
    }
    if !converged {
        return Err(AnalysisError::NotConverged { iterations });
    }

    // Covariance at the point found.
    jacobian(terms, &values, slots, &free, &mut jac, &mut buf);
    let dof = m.saturating_sub(free.len());
    let reduced_chi2 = if dof > 0 { chi2 / dof as f64 } else { f64::NAN };
    let mut errors = vec![None; slots.len()];
    let mut undetermined = vec![false; slots.len()];
    let determined = determined_columns(&jac);
    if !determined.iter().any(|d| *d) {
        return Err(AnalysisError::SingularSystem);
    }
    for (k, &i) in free.iter().enumerate() {
        undetermined[i] = !determined[k];
    }
    // Covariance of the determined slots only: a no-effect slot does not make
    // the uncertainties of the others mute.
    let kept: Vec<usize> = (0..free.len()).filter(|&k| determined[k]).collect();
    let mut sub = Matrix::zeros(m, kept.len());
    for row in 0..m {
        for (c, &k) in kept.iter().enumerate() {
            sub.set(row, c, jac.get(row, k));
        }
    }
    // At 0 degrees of freedom (exact fit), scaling by the reduced
    // χ² has no basis: no uncertainty, like the linear path.
    let no_basis = opts.scale_errors && dof == 0;
    if let Some(cov) = invert(&sub.gram()).filter(|_| !no_basis) {
        let scale = if opts.scale_errors && dof > 0 {
            reduced_chi2
        } else {
            1.0
        };
        for (c, &k) in kept.iter().enumerate() {
            let var = cov.get(c, c) * scale;
            if var.is_finite() && var >= 0.0 {
                errors[free[k]] = Some(var.sqrt());
            }
        }
    }

    // Statistics per term.
    let mut stats = Vec::with_capacity(terms.len());
    for t in terms {
        buf.clear();
        buf.resize(t.x.len(), 0.0);
        (t.eval)(&values, &mut buf);
        let n = t.y.len();
        let ss_res: f64 = t.y.iter().zip(&buf).map(|(y, f)| (y - f) * (y - f)).sum();
        let mean = t.y.iter().sum::<f64>() / n.max(1) as f64;
        let ss_tot: f64 = t.y.iter().map(|y| (y - mean) * (y - mean)).sum();
        stats.push(TermStats {
            points: n,
            ss_res,
            r_squared: if ss_tot > 0.0 {
                1.0 - ss_res / ss_tot
            } else {
                f64::NAN
            },
        });
    }
    // Plateau (#281): a free slot with no effect (zero Jacobian column)
    // AND a residual that stays large. No-effect slots on a good fit
    // remain a simple `undetermined`; a large residual with no no-effect slot
    // is a true local minimum, not a plateau.
    let plateau = undetermined.iter().any(|u| *u)
        && stats.iter().zip(terms).any(|(st, t)| {
            let mean = t.y.iter().sum::<f64>() / t.y.len().max(1) as f64;
            let ss_tot: f64 = t.y.iter().map(|y| (y - mean) * (y - mean)).sum();
            ss_tot > 0.0 && st.ss_res / ss_tot > PLATEAU_RESIDUAL_FRACTION
        });
    Ok(Solution {
        values,
        errors,
        undetermined,
        chi2,
        dof,
        reduced_chi2,
        iterations,
        terms: stats,
        plateau,
        stalled,
    })
}

/// Share of the norm of a Jacobian column that it must have anew (outside
/// the previous columns) to be determined. The noise of a central difference
/// at relative step 1e-6 is of the order of `ε / 1e-6 ≈ 2e-10`: a
/// floor at 1e-8 keeps a 50-fold margin over a true dependency, and lets
/// through a line on an X shifted by 1e6 (new share ≈ 4.5e-7, #293), which
/// the former millionth wrongly declared undetermined.
const DETERMINED_MIN_SHARE: f64 = 1e-8;

/// For each Jacobian column, `true` if the data determine it: its
/// share not carried by the previous columns (modified Gram-Schmidt) exceeds
/// `DETERMINED_MIN_SHARE` of its norm. A zero column (parameter with no effect)
/// or combination of the previous ones (all X equal for a line) is dropped.
fn determined_columns(jac: &Matrix) -> Vec<bool> {
    let (m, n) = (jac.rows(), jac.cols());
    let mut basis: Vec<Vec<f64>> = Vec::new();
    let mut out = vec![false; n];
    for k in 0..n {
        let mut col: Vec<f64> = (0..m).map(|r| jac.get(r, k)).collect();
        let norm = col.iter().map(|v| v * v).sum::<f64>().sqrt();
        if !(norm.is_finite() && norm > 0.0) {
            continue;
        }
        for b in &basis {
            let dot: f64 = col.iter().zip(b).map(|(a, c)| a * c).sum();
            for (a, c) in col.iter_mut().zip(b) {
                *a -= dot * c;
            }
        }
        let rest = col.iter().map(|v| v * v).sum::<f64>().sqrt();
        if rest > DETERMINED_MIN_SHARE * norm {
            for v in &mut col {
                *v /= rest;
            }
            basis.push(col);
            out[k] = true;
        }
    }
    out
}

/// `J[i][k] = ∂residual_i/∂free_slot_k`, by central differences.
///
/// The step relative to the parameter value (1e-6) is wrong for a position
/// parameter of large value: 1 for a peak center at 1e6, more than the
/// peak width, hence a wrong derivative returned as converged (#294). The
/// step is therefore **refined**: as long as the derivative at `h` and the one at `h/4`
/// diverge, the finer step is kept. A model that is smooth at the scale of the step
/// keeps its original step, identically.
fn jacobian(
    terms: &[Term<'_>],
    values: &[f64],
    slots: &[Slot],
    free: &[usize],
    jac: &mut Matrix,
    buf: &mut Vec<f64>,
) {
    let pmax = free.iter().fold(0.0_f64, |a, &i| a.max(values[i].abs()));
    let (mut plus, mut minus) = (Vec::new(), Vec::new());
    let mut work = values.to_vec();
    for (k, &i) in free.iter().enumerate() {
        // Step relative to the scale of the parameters; the absolute fallback is only used if
        // all are zero (never a floor that would crush values at 1e-12).
        let mut h = 1e-6
            * values[i]
                .abs()
                .max(1e-2 * pmax)
                .max(if pmax > 0.0 { 0.0 } else { 1e-9 });
        let mut coarse = column(terms, &mut work, slots, i, h, buf, &mut plus, &mut minus);
        for _ in 0..MAX_STEP_REFINEMENTS {
            // Refining a step does not make a non-finite derivative fine.
            if !h.is_finite() || coarse.values.iter().any(|v| !v.is_finite()) {
                break;
            }
            let fine_h = h / STEP_REFINEMENT_RATIO;
            let fine = column(
                terms, &mut work, slots, i, fine_h, buf, &mut plus, &mut minus,
            );
            // A difference explained by rounding is not a disagreement: without
            // this, a start at zero (step of 1e-15) would go down into the noise.
            let gap = (coarse.values.iter().zip(&coarse.noise))
                .zip(fine.values.iter().zip(&fine.noise))
                .map(|((c, nc), (f, nf))| ((c - f).abs() - nc - nf).max(0.0).powi(2))
                .sum::<f64>();
            let scale = (coarse.values.iter().chain(&fine.values))
                .map(|v| v * v)
                .sum::<f64>();
            if gap <= STEP_AGREEMENT * STEP_AGREEMENT * scale {
                break;
            }
            (h, coarse) = (fine_h, fine);
        }
        for (row, d) in coarse.values.iter().enumerate() {
            jac.set(row, k, *d);
        }
    }
}

/// Maximum number of divisions of the difference step (`4^8` at most).
const MAX_STEP_REFINEMENTS: usize = 8;
/// Ratio between two successive steps of the derivative comparison.
const STEP_REFINEMENT_RATIO: f64 = 4.0;
/// Relative gap (norm of the column) tolerated between the derivatives at `h` and `h/4`.
const STEP_AGREEMENT: f64 = 1e-3;

/// A Jacobian column and the rounding uncertainty of each of its rows.
struct Column {
    values: Vec<f64>,
    noise: Vec<f64>,
}

/// Jacobian column of slot `i`, by central differences of step `h`.
#[allow(clippy::too_many_arguments)]
fn column(
    terms: &[Term<'_>],
    work: &mut [f64],
    slots: &[Slot],
    i: usize,
    h: f64,
    buf: &mut Vec<f64>,
    plus: &mut Vec<f64>,
    minus: &mut Vec<f64>,
) -> Column {
    let value = work[i];
    // The step does not leave the bounds: at a bound, off-center difference.
    let hi = (value + h).min(slots[i].max);
    let lo = (value - h).max(slots[i].min);
    work[i] = hi;
    residuals(terms, work, buf, plus);
    work[i] = lo;
    residuals(terms, work, buf, minus);
    work[i] = value;
    let den = hi - lo;
    let mut column = Column {
        values: Vec::with_capacity(plus.len()),
        noise: Vec::with_capacity(plus.len()),
    };
    for (p, mn) in plus.iter().zip(minus.iter()) {
        let d = if den > 0.0 { (p - mn) / den } else { 0.0 };
        // A non-finite derivative stays as is (C6, #313): setting it to 0
        // made a non-evaluable model pass for a no-effect slot.
        column.values.push(d);
        // A step that no longer moves the value learns nothing: infinite noise.
        column.noise.push(if den > 0.0 {
            ROUNDOFF_FACTOR * f64::EPSILON * p.abs().max(mn.abs()) / den
        } else {
            f64::INFINITY
        });
    }
    column
}

/// Margin on the rounding error of a difference of two residuals.
const ROUNDOFF_FACTOR: f64 = 8.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn xs(n: usize, a: f64, b: f64) -> Vec<f64> {
        (0..n)
            .map(|i| a + (b - a) * i as f64 / (n - 1) as f64)
            .collect()
    }

    /// Deterministic noise (Weyl sequence) of amplitude ±1.
    fn noise(i: usize) -> f64 {
        ((i as f64 * 0.618_033_988_749_895).fract() - 0.5) * 2.0
    }

    #[test]
    fn recovers_an_exponential_with_offset() {
        let x = xs(200, 0.0, 10.0);
        let y: Vec<f64> = x.iter().map(|v| 1.0 + 4.0 * libm::exp(-v / 2.5)).collect();
        let term = Term {
            x: &x,
            y: &y,
            sigma: None,
            eval: Box::new(|p, out| {
                for (o, v) in out.iter_mut().zip(&x) {
                    *o = p[0] + p[1] * libm::exp(-v / p[2]);
                }
            }),
        };
        let slots = vec![
            Slot::new("y0", 0.0),
            Slot::new("A", 1.0),
            Slot::new("tau", 1.0),
        ];
        let sol = solve_problem(&slots, &[term], Options::default()).unwrap();
        assert!((sol.values[0] - 1.0).abs() < 1e-6, "{:?}", sol.values);
        assert!((sol.values[1] - 4.0).abs() < 1e-6);
        assert!((sol.values[2] - 2.5).abs() < 1e-6);
        assert!(sol.terms[0].r_squared > 1.0 - 1e-12);
    }

    /// Gaussian + offset: the start of issue #281.
    fn gaussian_problem<'a>(x: &'a [f64], y: &'a [f64]) -> Term<'a> {
        Term {
            x,
            y,
            sigma: None,
            eval: Box::new(move |p, out| {
                for (o, v) in out.iter_mut().zip(x) {
                    let z = (v - p[1]) / (p[3] / 2.354_820_045);
                    *o = p[0] + p[2] * libm::exp(-0.5 * z * z);
                }
            }),
        }
    }

    fn gaussian_data(noise_amp: f64) -> (Vec<f64>, Vec<f64>) {
        let x = xs(200, 0.0, 20.0);
        let y = x
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let z = (v - 8.0) / (3.0 / 2.354_820_045);
                5.0 * libm::exp(-0.5 * z * z) + noise_amp * noise(i)
            })
            .collect();
        (x, y)
    }

    fn gaussian_slots(xc: f64, fwhm: f64) -> Vec<Slot> {
        vec![
            Slot::new("y0", 0.0),
            Slot::new("xc", xc),
            Slot::new("A", 1.0),
            Slot::new("fwhm", fwhm),
        ]
    }

    #[test]
    fn a_start_outside_the_data_stops_on_a_plateau() {
        // #281: xc = 1000, far from the peak -> zero Jacobian except y0.
        let (x, y) = gaussian_data(0.0);
        let sol = solve_problem(
            &gaussian_slots(1000.0, 1.0),
            &[gaussian_problem(&x, &y)],
            Options::default(),
        )
        .unwrap();
        assert!(sol.terms[0].r_squared < 0.1, "{}", sol.terms[0].r_squared);
        assert!(sol.plateau);
    }

    #[test]
    fn a_sound_or_noisy_fit_is_not_a_plateau() {
        for amp in [0.0, 0.3] {
            let (x, y) = gaussian_data(amp);
            let sol = solve_problem(
                &gaussian_slots(7.0, 2.0),
                &[gaussian_problem(&x, &y)],
                Options::default(),
            )
            .unwrap();
            assert!(sol.terms[0].r_squared > 0.9 && !sol.plateau, "noise {amp}");
        }
    }

    /// Eight points X = 0..7 and a vector orthogonal to `1` and to `x`: an
    /// affine fit leaves exactly `k · ORTHO` of residual.
    const ORTHO: [f64; 8] = [1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0];
    /// `Σ (x − x̄)²` for X = 0..7.
    const SS_X: f64 = 42.0;

    /// The data `y = x + k·ORTHO` whose SSres/SStot is `fraction` after an
    /// affine fit: SSres = 8k², SStot = 42 + 8k².
    fn affine_with_residual(fraction: f64) -> (Vec<f64>, Vec<f64>) {
        let k = (SS_X * fraction / (8.0 * (1.0 - fraction))).sqrt();
        let x: Vec<f64> = (0..8).map(f64::from).collect();
        let y = x.iter().zip(ORTHO).map(|(x, r)| x + k * r).collect();
        (x, y)
    }

    /// `p0 + p1·x`, plus (if `dead`) a free slot `p2` with no effect at all.
    fn affine_term<'a>(x: &'a [f64], y: &'a [f64]) -> Term<'a> {
        Term {
            x,
            y,
            sigma: None,
            eval: Box::new(move |p, out| {
                for (o, v) in out.iter_mut().zip(x) {
                    *o = p[0] + p[1] * v + 0.0 * p[2];
                }
            }),
        }
    }

    fn affine_slots(dead: bool) -> Vec<Slot> {
        let mut slots = vec![Slot::new("a", 0.0), Slot::new("b", 1.0)];
        let mut c = Slot::new("c", 1.0);
        c.fixed = !dead;
        slots.push(c);
        slots
    }

    fn solve_affine(fraction: f64, dead: bool) -> Solution {
        let (x, y) = affine_with_residual(fraction);
        #[allow(clippy::let_and_return)] // `x` and `y` must outlive the call.
        let sol = solve_problem(
            &affine_slots(dead),
            &[affine_term(&x, &y)],
            Options::default(),
        )
        .unwrap();
        sol
    }

    #[test]
    fn the_constructed_data_set_gives_the_wanted_residual() {
        for f in [0.2, 0.49, 0.51, 0.9] {
            let sol = solve_affine(f, true);
            let (x, y) = affine_with_residual(f);
            let mean = y.iter().sum::<f64>() / 8.0;
            let ss_tot: f64 = y.iter().map(|v| (v - mean) * (v - mean)).sum();
            assert!((sol.terms[0].ss_res / ss_tot - f).abs() < 1e-9, "{f}");
            assert_eq!(x.len(), 8);
        }
    }

    #[test]
    fn the_plateau_threshold_on_the_residual_share_is_one_half() {
        // No-effect slot in both cases: only the residual share changes.
        let below = solve_affine(0.49, true);
        let above = solve_affine(0.51, true);
        assert_eq!(below.undetermined, vec![false, false, true]);
        assert_eq!(above.undetermined, vec![false, false, true]);
        assert!(!below.plateau, "0.49: below the threshold");
        assert!(above.plateau, "0.51: above the threshold");
        const { assert!(PLATEAU_RESIDUAL_FRACTION > 0.49 && PLATEAU_RESIDUAL_FRACTION < 0.51) };
    }

    #[test]
    fn a_large_residual_without_a_no_effect_slot_is_a_minimum_not_a_plateau() {
        for f in [0.51, 0.9, 0.99] {
            let sol = solve_affine(f, false);
            assert!(sol.undetermined.iter().all(|u| !u), "{f}");
            assert!(!sol.plateau, "{f}");
        }
    }

    #[test]
    fn an_over_parameterized_model_that_fits_well_is_undetermined_without_a_plateau() {
        for f in [0.0, 1e-6, 0.1] {
            let sol = solve_affine(f, true);
            assert_eq!(sol.undetermined, vec![false, false, true], "{f}");
            assert!(!sol.plateau, "{f}");
        }
    }

    #[test]
    fn a_constant_curve_is_never_a_plateau() {
        // SStot = 0: the residual share is meaningless (SSres/0 = +inf if the
        // model cannot reach it: `a` fixed at 0, y = 3).
        let x: Vec<f64> = (0..8).map(f64::from).collect();
        let y = vec![3.0; 8];
        let mut slots = affine_slots(true);
        slots[0].fixed = true;
        slots[0].initial = 0.0;
        let sol = solve_problem(&slots, &[affine_term(&x, &y)], Options::default()).unwrap();
        assert!(sol.terms[0].ss_res > 1.0, "{}", sol.terms[0].ss_res);
        assert!(sol.undetermined[2]);
        assert!(!sol.plateau);
    }

    #[test]
    fn a_global_fit_is_on_a_plateau_if_a_single_curve_is() {
        let (xa, ya) = affine_with_residual(0.01);
        let (xb, yb) = affine_with_residual(0.9);
        let slots = affine_slots(true);
        for order in [[0, 1], [1, 0]] {
            let curves = [(&xa, &ya), (&xb, &yb)];
            let terms: Vec<Term> = order
                .iter()
                .map(|&i| affine_term(curves[i].0, curves[i].1))
                .collect();
            let sol = solve_problem(&slots, &terms, Options::default()).unwrap();
            assert!(sol.plateau, "order {order:?}");
        }
        // Two well-fitted curves: no plateau.
        let sol = solve_problem(
            &slots,
            &[affine_term(&xa, &ya), affine_term(&xa, &ya)],
            Options::default(),
        )
        .unwrap();
        assert!(!sol.plateau);
    }

    #[test]
    fn the_example_of_issue_281_is_a_plateau_and_keeps_its_values() {
        let (x, y) = gaussian_data(0.0);
        let sol = solve_problem(
            &gaussian_slots(1000.0, 1.0),
            &[gaussian_problem(&x, &y)],
            Options::default(),
        )
        .unwrap();
        assert!(sol.plateau);
        // The solution is returned (no hard error): xc and fwhm did not move.
        assert!((sol.values[1] - 1000.0).abs() < 1e-6, "{:?}", sol.values);
        assert!((sol.values[3] - 1.0).abs() < 1e-6, "{:?}", sol.values);
        assert_eq!(sol.undetermined, vec![false, true, true, true]);
    }

    #[test]
    fn the_uncertainties_follow_the_noise() {
        let x = xs(400, 0.0, 10.0);
        let y: Vec<f64> = x
            .iter()
            .enumerate()
            .map(|(i, v)| 2.0 + 3.0 * v + 0.2 * noise(i))
            .collect();
        let term = Term {
            x: &x,
            y: &y,
            sigma: None,
            eval: Box::new(|p, out| {
                for (o, v) in out.iter_mut().zip(&x) {
                    *o = p[0] + p[1] * v;
                }
            }),
        };
        let slots = vec![Slot::new("a", 0.0), Slot::new("b", 0.0)];
        let sol = solve_problem(&slots, &[term], Options::default()).unwrap();
        assert!((sol.values[1] - 3.0).abs() < 0.01);
        let e = sol.errors[1].unwrap();
        // σ_slope ≈ σ_noise / (√n · σ_x) = 0.2·0.577/(20·2.89) ≈ 0.002.
        assert!(e > 0.0005 && e < 0.01, "error on the slope {e}");
        assert!(sol.reduced_chi2 > 0.0 && sol.dof == 398);
    }

    #[test]
    fn a_fixed_slot_does_not_move_and_has_no_error() {
        let x = xs(50, 0.0, 5.0);
        let y: Vec<f64> = x.iter().map(|v| 2.0 * v + 1.0).collect();
        let term = Term {
            x: &x,
            y: &y,
            sigma: None,
            eval: Box::new(|p, out| {
                for (o, v) in out.iter_mut().zip(&x) {
                    *o = p[0] * v + p[1];
                }
            }),
        };
        let mut b = Slot::new("b", 1.0);
        b.fixed = true;
        let sol = solve_problem(&[Slot::new("a", 0.0), b], &[term], Options::default()).unwrap();
        assert!((sol.values[0] - 2.0).abs() < 1e-8);
        assert_eq!(sol.values[1], 1.0);
        assert!(sol.errors[1].is_none() && sol.errors[0].is_some());
    }

    #[test]
    fn the_bounds_are_respected() {
        let x = xs(50, 0.0, 5.0);
        let y: Vec<f64> = x.iter().map(|v| 5.0 * v).collect();
        let term = Term {
            x: &x,
            y: &y,
            sigma: None,
            eval: Box::new(|p, out| {
                for (o, v) in out.iter_mut().zip(&x) {
                    *o = p[0] * v;
                }
            }),
        };
        let mut a = Slot::new("a", 1.0);
        a.max = 3.0;
        let sol = solve_problem(&[a], &[term], Options::default()).unwrap();
        assert!((sol.values[0] - 3.0).abs() < 1e-9);
    }

    /// Line `a + b·x` on `x`, two free parameters.
    fn line_on<'a>(x: &'a [f64], y: &'a [f64]) -> Term<'a> {
        Term {
            x,
            y,
            sigma: None,
            eval: Box::new(move |p, out| {
                for (o, v) in out.iter_mut().zip(x) {
                    *o = p[0] + p[1] * v;
                }
            }),
        }
    }

    /// Line `y = 2 + 3t` on `x = off + t` (60 points, `t ∈ [0, 1]`), start
    /// `(a, b) = (-off, 1)`: the shifted X makes the columns nearly collinear.
    fn shifted_line(off: f64) -> Solution {
        let t: Vec<f64> = (0..60).map(|i| f64::from(i) / 59.0).collect();
        let x: Vec<f64> = t.iter().map(|t| off + t).collect();
        let y: Vec<f64> = t
            .iter()
            .enumerate()
            .map(|(i, t)| 2.0 + 3.0 * t + 0.01 * libm::sin(i as f64 * 1.7))
            .collect();
        let slots = vec![Slot::new("a", -off), Slot::new("b", 1.0)];
        let sol = solve_problem(&slots, &[line_on(&x, &y)], Options::default());
        sol.unwrap()
    }

    #[test]
    fn a_tiny_damped_step_is_not_a_convergence() {
        // #293: Marquardt damping follows `JᵀJ` (≈ off²) and restrains the
        // weak direction; the step became tiny, taken for
        // convergence, while χ² was 7000 times the minimum.
        for off in [1e7, 1e8, 1e9] {
            let sol = shifted_line(off);
            let (a, b) = (sol.values[0], sol.values[1]);
            assert!((b - 3.0).abs() < 1e-2, "off = {off:e} : {:?}", sol.values);
            assert!(sol.chi2 < 1e-2, "off = {off:e} : χ² = {:e}", sol.chi2);
            assert!((a + b * off - 2.0).abs() < 1e-2, "off = {off:e} : a = {a}");
        }
    }

    #[test]
    fn zero_dof_with_scaling_no_uncertainty() {
        let (x, y) = (vec![0.0, 1.0], vec![1.0, 3.0]);
        let slots = vec![Slot::new("a", 0.0), Slot::new("b", 0.0)];
        let sol = solve_problem(&slots, &[line_on(&x, &y)], Options::default()).unwrap();
        assert_eq!(sol.dof, 0);
        assert!(sol.errors.iter().all(Option::is_none));
        assert!(sol.reduced_chi2.is_nan());
        assert!(sol.chi2.is_finite() && sol.chi2 < 1e-12);
    }

    #[test]
    fn zero_dof_without_scaling_keeps_the_uncertainties() {
        // Documented choice of #280: `scale_errors == false` (absolute σ) does not
        // need the reduced χ²; the raw covariance stays valid at 0 dof.
        // For x = {0, 1}, σ = 1: var(a) = 1, var(b) = 2.
        let (x, y) = (vec![0.0, 1.0], vec![1.0, 3.0]);
        let slots = vec![Slot::new("a", 0.0), Slot::new("b", 0.0)];
        let opts = Options {
            scale_errors: false,
            ..Options::default()
        };
        let sol = solve_problem(&slots, &[line_on(&x, &y)], opts).unwrap();
        assert_eq!(sol.dof, 0);
        assert!(sol.reduced_chi2.is_nan());
        let (ea, eb) = (sol.errors[0].unwrap(), sol.errors[1].unwrap());
        assert!((ea - 1.0).abs() < 1e-5, "{ea}");
        assert!((eb - 2f64.sqrt()).abs() < 1e-5, "{eb}");
    }

    #[test]
    fn one_dof_with_scaling_scaled_uncertainties() {
        // 3 points, 2 free: dof 1, errors = raw covariance × reduced χ².
        let (x, y) = (vec![0.0, 1.0, 2.0], vec![1.0, 3.5, 4.0]);
        let slots = vec![Slot::new("a", 0.0), Slot::new("b", 0.0)];
        let scaled = solve_problem(&slots, &[line_on(&x, &y)], Options::default()).unwrap();
        let opts = Options {
            scale_errors: false,
            ..Options::default()
        };
        let raw = solve_problem(&slots, &[line_on(&x, &y)], opts).unwrap();
        assert_eq!(scaled.dof, 1);
        assert!(scaled.reduced_chi2.is_finite() && scaled.reduced_chi2 > 0.0);
        for k in 0..2 {
            let (m, b) = (scaled.errors[k].unwrap(), raw.errors[k].unwrap());
            let expected = b * scaled.reduced_chi2.sqrt();
            assert!((m - expected).abs() < 1e-6 * expected, "{m} vs {expected}");
        }
    }

    #[test]
    fn fewer_points_than_free_slots_refused_before_the_solver() {
        // m < free: never a negative dof, clean refusal with both counts.
        let (x, y) = (vec![0.0, 1.0], vec![1.0, 3.0]);
        let slots = vec![
            Slot::new("a", 0.0),
            Slot::new("b", 0.0),
            Slot::new("c", 0.0),
        ];
        let e = solve_problem(&slots, &[line_on(&x, &y)], Options::default()).unwrap_err();
        assert_eq!(
            e,
            AnalysisError::NotEnoughPoints {
                found: 2,
                needed: 3
            }
        );
        // A fixed slot brings the need down to 2: accepted, dof 0.
        let mut fixed = Slot::new("c", 0.0);
        fixed.fixed = true;
        let slots = vec![Slot::new("a", 0.0), Slot::new("b", 0.0), fixed];
        let sol = solve_problem(&slots, &[line_on(&x, &y)], Options::default()).unwrap();
        assert_eq!(sol.dof, 0);
    }

    #[test]
    fn two_terms_share_a_global_slot() {
        // y1 = k·x, y2 = k·x + 10: k shared, own offset for the 2nd curve.
        let x = xs(30, 0.0, 3.0);
        let y1: Vec<f64> = x.iter().map(|v| 4.0 * v).collect();
        let y2: Vec<f64> = x.iter().map(|v| 4.0 * v + 10.0).collect();
        let t1 = Term {
            x: &x,
            y: &y1,
            sigma: None,
            eval: Box::new(|p, out| {
                for (o, v) in out.iter_mut().zip(&x) {
                    *o = p[0] * v;
                }
            }),
        };
        let t2 = Term {
            x: &x,
            y: &y2,
            sigma: None,
            eval: Box::new(|p, out| {
                for (o, v) in out.iter_mut().zip(&x) {
                    *o = p[0] * v + p[1];
                }
            }),
        };
        let slots = vec![Slot::new("k", 1.0), Slot::new("offset", 0.0)];
        let sol = solve_problem(&slots, &[t1, t2], Options::default()).unwrap();
        assert!((sol.values[0] - 4.0).abs() < 1e-7 && (sol.values[1] - 10.0).abs() < 1e-6);
        assert_eq!(sol.terms.len(), 2);
    }

    #[test]
    fn weighting_gives_more_weight_to_precise_points() {
        // Two outliers with large σ do not pull the line.
        let x = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let y = vec![0.0, 1.0, 2.0, 3.0, 40.0, 5.0];
        let sigma = vec![0.1, 0.1, 0.1, 0.1, 1000.0, 0.1];
        let term = Term {
            x: &x,
            y: &y,
            sigma: Some(&sigma),
            eval: Box::new(|p, out| {
                for (o, v) in out.iter_mut().zip(&x) {
                    *o = p[0] + p[1] * v;
                }
            }),
        };
        let slots = vec![Slot::new("a", 0.0), Slot::new("b", 0.0)];
        let sol = solve_problem(&slots, &[term], Options::default()).unwrap();
        assert!((sol.values[1] - 1.0).abs() < 1e-3, "{:?}", sol.values);
    }

    #[test]
    fn problem_errors() {
        let x = vec![0.0, 1.0];
        let y = vec![0.0, 1.0];
        let mk = || Term {
            x: &x,
            y: &y,
            sigma: None,
            eval: Box::new(|p: &[f64], out: &mut [f64]| {
                for o in out.iter_mut() {
                    *o = p[0];
                }
            }),
        };
        let mut fixed = Slot::new("a", 0.0);
        fixed.fixed = true;
        assert_eq!(
            solve_problem(&[fixed], &[mk()], Options::default()).unwrap_err(),
            AnalysisError::NoFreeParameter
        );
        let mut bad = Slot::new("a", 0.0);
        bad.min = 2.0;
        bad.max = 1.0;
        assert!(matches!(
            solve_problem(&[bad], &[mk()], Options::default()),
            Err(AnalysisError::EmptyBounds { .. })
        ));
        let slots = vec![
            Slot::new("a", 0.0),
            Slot::new("b", 0.0),
            Slot::new("c", 0.0),
        ];
        assert!(matches!(
            solve_problem(&slots, &[mk()], Options::default()),
            Err(AnalysisError::NotEnoughPoints { .. })
        ));
        let nan = Term {
            x: &x,
            y: &y,
            sigma: None,
            eval: Box::new(|_: &[f64], out: &mut [f64]| {
                for o in out.iter_mut() {
                    *o = f64::NAN;
                }
            }),
        };
        assert_eq!(
            solve_problem(&[Slot::new("a", 0.0)], &[nan], Options::default()).unwrap_err(),
            AnalysisError::NonFiniteModel
        );
    }

    /// Term with a custom `eval`: constant `y`, model `f` of one slot.
    fn one_slot<'a>(x: &'a [f64], y: &'a [f64], f: impl Fn(f64) -> f64 + 'a) -> Term<'a> {
        Term {
            x,
            y,
            sigma: None,
            eval: Box::new(move |p, out| {
                for o in out.iter_mut() {
                    *o = f(p[0]);
                }
            }),
        }
    }

    /// C1 (#313): a corner with no descent. `f = 1 + |d| + 0.5·d` (d = a − 3) has a
    /// kinked minimum: the centerd slope is 0.5, no step lowers
    /// χ². The start is returned, flagged `stalled`, never silently "converged".
    #[test]
    fn a_solver_with_no_accepted_step_and_a_steep_slope_is_flagged_stalled() {
        let x = xs(5, 0.0, 4.0);
        let y = vec![0.0; 5];
        let term = one_slot(&x, &y, |a| {
            let d = a - 3.0;
            1.0 + d.abs() + 0.5 * d
        });
        let sol = solve_problem(&[Slot::new("a", 3.0)], &[term], Options::default()).unwrap();
        assert!(sol.stalled, "{sol:?}");
        assert_eq!(sol.values[0], 3.0);
    }

    /// A true smooth minimum (zero slope) does not flip; neither does a parameter stuck
    /// on its bound.
    #[test]
    fn a_smooth_minimum_or_a_reached_bound_are_not_stalled() {
        let x = xs(5, 0.0, 4.0);
        let y = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let smooth_term = one_slot(&x, &y, |a| a);
        let sol =
            solve_problem(&[Slot::new("a", 5.0)], &[smooth_term], Options::default()).unwrap();
        assert!(
            !sol.stalled && (sol.values[0] - 3.0).abs() < 1e-4,
            "{sol:?}"
        );

        // f = 1 + a, a ≥ 0: the minimum under constraint is the bound.
        let mut slot = Slot::new("a", 2.0);
        slot.min = 0.0;
        let zero = vec![0.0; 5];
        let bound = one_slot(&x, &zero, |a| 1.0 + a);
        let sol = solve_problem(&[slot], &[bound], Options::default()).unwrap();
        assert!(!sol.stalled && sol.values[0] == 0.0, "{sol:?}");
    }

    /// C6 (#313): a NaN Jacobian escaped the `max` fold (which ignores NaN) and
    /// the solver concluded "converged". `sqrt(a)` at a = 0: the centerd
    /// difference reads `sqrt(−h)`.
    #[test]
    fn a_non_finite_slope_is_a_non_evaluable_model_error() {
        let x = xs(5, 0.0, 4.0);
        let y = vec![1.0; 5];
        let term = one_slot(&x, &y, f64::sqrt);
        assert_eq!(
            solve_problem(&[Slot::new("a", 0.0)], &[term], Options::default()).unwrap_err(),
            AnalysisError::ModelNotEvaluable
        );
    }
}
