//! The operations of the Analysis menu, end to end: raw columns as
//! input, computed columns and values as output.
//!
//! This is the **only** entry point that the application, the `pyst` scripts and
//! recalculation use: they supply the columns read from the model and
//! write back what comes out. Outputs are **aligned on the rows of the
//! input** (`NaN` where a row is dropped or out of range), so that a
//! computed column has the length of its source.
//!
//! Output keys are stable and sentence-free (see [`Value::key`] and
//! [`OutColumn::key`]): the application turns them into translated labels.

use crate::baseline::{self, Baseline, BaselineMethod, BaselineOp};
use crate::correct::{self, CorrectOp};
use crate::deconv::{self, DirectMethod, ReconvSpec};
use crate::error::AnalysisError;
use crate::expr::ExprError;
use crate::fit::{self, FitInput, FitSpec, ParamSetting};
use crate::integrate::{self, IntegrateSpec};
use crate::model::Model;
use crate::peaks::{self, PeakSeed, PeaksSpec};
use crate::series::{clean, interpolate, Series};
use crate::smooth::{self, SmoothMethod, SmoothSpec};

/// The columns of a curve to process, row by row. `NaN`: empty cell,
/// text or invalid.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SeriesInput {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    /// Standard deviation of Y (column with the error role), for the weighted fit.
    pub sigma: Option<Vec<f64>>,
}

/// An auxiliary (X, Y) column: the reference of a correction, the IRF of a
/// deconvolution.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AuxInput {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
}

/// Automatic detection of the starting peaks (M14 a C).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoDetect {
    /// Minimum prominence, as a fraction of the Y range (0 to 1).
    pub min_prominence: f64,
    pub max_peaks: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operation {
    Baseline {
        method: BaselineMethod,
        op: BaselineOp,
        /// Also write the baseline into a column.
        keep_baseline: bool,
    },
    /// Reference: auxiliary column.
    Correct {
        op: CorrectOp,
    },
    Integrate {
        spec: IntegrateSpec,
    },
    Fit {
        spec: FitSpec,
        range: Option<(f64, f64)>,
        residuals: bool,
    },
    /// Without `seeds` in `spec`, `auto` detects the peaks on the curve.
    Peaks {
        spec: PeaksSpec,
        auto: Option<AutoDetect>,
        range: Option<(f64, f64)>,
        residuals: bool,
    },
    /// IRF: auxiliary column.
    Reconvolution {
        spec: ReconvSpec,
        residuals: bool,
    },
    /// IRF: auxiliary column.
    DirectDeconvolution {
        method: DirectMethod,
        shift: f64,
    },
    /// Smoothing of a curve (#324).
    Smooth {
        spec: SmoothSpec,
    },
}

/// The family of an operation, without its settings: the history label, the
/// glossary key and the default "automatic recalculation" setting
/// depend on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationKind {
    Baseline,
    Correct,
    Integrate,
    Fit,
    Peaks,
    Reconvolution,
    DirectDeconvolution,
    Smooth,
}

impl OperationKind {
    pub const ALL: [OperationKind; 8] = [
        OperationKind::Baseline,
        OperationKind::Correct,
        OperationKind::Integrate,
        OperationKind::Fit,
        OperationKind::Peaks,
        OperationKind::Reconvolution,
        OperationKind::DirectDeconvolution,
        OperationKind::Smooth,
    ];

    /// Stable key (that of `Tree` and of the scripts).
    pub fn key(self) -> &'static str {
        match self {
            OperationKind::Baseline => "baseline",
            OperationKind::Correct => "correct",
            OperationKind::Integrate => "integrate",
            OperationKind::Fit => "fit",
            OperationKind::Peaks => "peaks",
            OperationKind::Reconvolution => "reconvolution",
            OperationKind::DirectDeconvolution => "direct_deconvolution",
            OperationKind::Smooth => "smooth",
        }
    }

    pub fn from_key(key: &str) -> Option<OperationKind> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }
}

/// Beyond this number of series, a batch is no longer recalculated automatically
/// by default (M9 b C).
pub const HEAVY_BATCH: usize = 20;

impl Operation {
    pub fn kind(&self) -> OperationKind {
        match self {
            Operation::Baseline { .. } => OperationKind::Baseline,
            Operation::Correct { .. } => OperationKind::Correct,
            Operation::Integrate { .. } => OperationKind::Integrate,
            Operation::Fit { .. } => OperationKind::Fit,
            Operation::Peaks { .. } => OperationKind::Peaks,
            Operation::Reconvolution { .. } => OperationKind::Reconvolution,
            Operation::DirectDeconvolution { .. } => OperationKind::DirectDeconvolution,
            Operation::Smooth { .. } => OperationKind::Smooth,
        }
    }

    /// Initial setting of "recalculate automatically": checked, except for
    /// reconvolution, direct deconvolution and batches of more than
    /// [`HEAVY_BATCH`] series (M9 b C).
    pub fn auto_recalc_by_default(&self, series_count: usize) -> bool {
        !matches!(
            self.kind(),
            OperationKind::Reconvolution | OperationKind::DirectDeconvolution
        ) && series_count <= HEAVY_BATCH
    }

    /// This operation reads an auxiliary column.
    pub fn needs_aux(&self) -> bool {
        matches!(
            self,
            Operation::Correct { .. }
                | Operation::Reconvolution { .. }
                | Operation::DirectDeconvolution { .. }
        )
    }
}

/// A computed column, of the length of the input.
#[derive(Debug, Clone, PartialEq)]
pub struct OutColumn {
    /// Stable key: `corrected`, `baseline`, `fit`, `residuals`, `cumulative`,
    /// `normalized`, `peak` (+ `index`), `peaks_sum`, `background`,
    /// `deconvolved`.
    pub key: &'static str,
    /// Rank of the peak for `peak` (from 1).
    pub index: Option<usize>,
    pub data: Vec<f64>,
}

/// A computed value, with its optional uncertainty.
#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    /// Stable dotted key: `param.<name>`, `stat.<name>`, `area`, `area.total`,
    /// `peak.<name>`, `background.<name>`, `baseline.<name>`, `count.<name>`.
    pub key: String,
    /// Rank (range, peak), from 1.
    pub index: Option<usize>,
    pub value: f64,
    pub error: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// Step of the non-constant signal (> 1 %).
    SignalStepNotUniform,
    /// IRF step different from that of the signal (> 1 %).
    IrfStepMismatch,
    /// Negative points brought to 0 (Richardson-Lucy).
    NegativeClipped(usize),
    /// Free parameters that the data do not determine: their uncertainty
    /// is omitted (#203).
    Undetermined(usize),
    /// The solver stopped on a plateau (#281) or without being able to accept
    /// a step while the slope was still large (C1, #313), not on a
    /// minimum: the result is returned but is not a converged fit.
    NotConverged,
    /// The IRF is cut above 1 % of its peak at the chosen shift: the missing
    /// tail is 0 and the kernel is renormalized (C2, #313).
    IrfTruncated,
    /// Data points interpolated across a hole (empty or non-finite
    /// cell) of the reference column (C3, #313).
    BridgedReferenceGap(usize),
    /// Divisions by zero, returned empty (`NaN`) (C5, #313).
    DivisionByZero(usize),
    /// Constrained polynomial fitted on an X very far from 0, without centering:
    /// the precision may have degraded (A2, #296).
    ShiftedX,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Outcome {
    pub columns: Vec<OutColumn>,
    pub values: Vec<Value>,
    /// Rows dropped: X or Y non-finite.
    pub skipped: usize,
    pub warnings: Vec<Warning>,
}

impl Outcome {
    fn value(
        &mut self,
        key: impl Into<String>,
        index: Option<usize>,
        value: f64,
        error: Option<f64>,
    ) {
        self.values.push(Value {
            key: key.into(),
            index,
            value,
            error,
        });
    }

    fn column(&mut self, key: &'static str, index: Option<usize>, data: Vec<f64>) {
        self.columns.push(OutColumn { key, index, data });
    }

    fn warn_undetermined(&mut self, count: usize) {
        if count > 0 {
            self.warnings.push(Warning::Undetermined(count));
        }
    }

    fn warn_shifted_x(&mut self, shifted: bool) {
        if shifted {
            self.warnings.push(Warning::ShiftedX);
        }
    }

    fn warn_plateau(&mut self, plateau: bool) {
        if plateau {
            self.warnings.push(Warning::NotConverged);
        }
    }

    fn stats(&mut self, points: usize, r2: f64, reduced_chi2: f64, dof: usize) {
        self.value("stat.points", None, points as f64, None);
        self.value("stat.r_squared", None, r2, None);
        self.value("stat.reduced_chi2", None, reduced_chi2, None);
        self.value("stat.dof", None, dof as f64, None);
    }
}

/// An aligned input: the clean series and the rank of each point in the
/// original rows.
struct Aligned {
    series: Series,
    rows: Vec<usize>,
    /// Number of rows of the input.
    len: usize,
    skipped: usize,
    /// X of each row (`NaN` if absent), to evaluate a curve at any X.
    x_all: Vec<f64>,
    sigma: Option<Vec<f64>>,
}

fn align(input: &SeriesInput) -> Aligned {
    let len = input.y.len();
    let mut x_all = input.x.clone();
    x_all.resize(len, f64::NAN);
    let (mut sx, mut sy, mut rows, mut sig) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut skipped = 0;
    for i in 0..len {
        let s = input.sigma.as_ref().and_then(|s| s.get(i).copied());
        // A weighted row whose σ is empty, zero or negative is dropped and
        // counted, like a row whose X or Y is missing (#205).
        let sigma_ok = input.sigma.is_none() || s.is_some_and(|v| v.is_finite() && v > 0.0);
        let ok = x_all[i].is_finite() && input.y[i].is_finite() && sigma_ok;
        if ok {
            sx.push(x_all[i]);
            sy.push(input.y[i]);
            rows.push(i);
            sig.push(s.unwrap_or(f64::NAN));
        } else {
            skipped += 1;
        }
    }
    let sigma = input.sigma.as_ref().map(|_| sig);
    Aligned {
        series: Series::new(sx, sy),
        rows,
        len,
        skipped,
        x_all,
        sigma,
    }
}

impl Aligned {
    /// Spreads a value per clean point over all the rows.
    fn expand(&self, per_point: &[f64]) -> Vec<f64> {
        let mut out = vec![f64::NAN; self.len];
        for (k, &row) in self.rows.iter().enumerate() {
            out[row] = per_point[k];
        }
        out
    }

    /// Evaluates `f` on each row whose X is finite.
    fn eval_all(&self, f: impl Fn(f64) -> f64) -> Vec<f64> {
        self.x_all
            .iter()
            .map(|&x| if x.is_finite() { f(x) } else { f64::NAN })
            .collect()
    }

    /// Evaluates `f` on each row whose X is finite and within `range`.
    fn eval_in(&self, range: Option<(f64, f64)>, f: impl Fn(f64) -> f64) -> Vec<f64> {
        let (lo, hi) = range.map_or((f64::NEG_INFINITY, f64::INFINITY), |(a, b)| {
            (a.min(b), a.max(b))
        });
        self.x_all
            .iter()
            .map(|&x| {
                if x.is_finite() && x >= lo && x <= hi {
                    f(x)
                } else {
                    f64::NAN
                }
            })
            .collect()
    }

    /// The `y` of the rows (original value or `NaN`).
    fn y_rows(&self, input: &SeriesInput) -> Vec<f64> {
        (0..self.len)
            .map(|i| {
                if self.x_all[i].is_finite() {
                    input.y[i]
                } else {
                    f64::NAN
                }
            })
            .collect()
    }
}

fn aux_series(aux: Option<&AuxInput>) -> Result<Series, AnalysisError> {
    let aux = aux.ok_or(AnalysisError::EmptyReference)?;
    let c = clean(&aux.x, &aux.y);
    if c.series.is_empty() {
        return Err(AnalysisError::EmptyReference);
    }
    Ok(c.series)
}

/// X of the reference rows whose Y is not finite while X is: the
/// holes that `correct` can bridge. A row without X has no position.
fn aux_holes(aux: Option<&AuxInput>) -> Vec<f64> {
    aux.map_or_else(Vec::new, |a| {
        a.x.iter()
            .zip(&a.y)
            .filter(|(x, y)| x.is_finite() && !y.is_finite())
            .map(|(x, _)| *x)
            .collect()
    })
}

fn deconv_warnings(w: deconv::DeconvWarnings) -> Vec<Warning> {
    let mut out = Vec::new();
    if w.non_uniform_signal {
        out.push(Warning::SignalStepNotUniform);
    }
    if w.irf_step_mismatch {
        out.push(Warning::IrfStepMismatch);
    }
    if w.irf_truncated {
        out.push(Warning::IrfTruncated);
    }
    if w.negative_clipped > 0 {
        out.push(Warning::NegativeClipped(w.negative_clipped));
    }
    out
}

fn restrict(series: &Series, range: Option<(f64, f64)>) -> Result<Series, AnalysisError> {
    match range {
        None => Ok(series.clone()),
        Some((a, b)) => {
            let r = series.restricted(a, b);
            if r.is_empty() {
                Err(AnalysisError::EmptyRange)
            } else {
                Ok(r)
            }
        }
    }
}

/// Caps on the settings, shared by the interface (the quantities table of
/// `stokesforge-app` reads them), by `pyst` and by the file: a degree of 10⁹
/// would create a billion parameters, unbounded iterations would block
/// the session actor on recalculation. `Model::compile` also applies
/// [`MAX_DEGREE`]: a degree read as is from a file must not allocate anything
/// before being refused.
pub const MAX_DEGREE: u32 = 9;
pub const MAX_ITERATIONS: usize = 5000;
/// Maximum number of peaks sought by automatic detection.
pub const MAX_PEAKS: usize = 100;
/// Maximum number of components of a reconvolution (the minimum is 1).
pub const MAX_COMPONENTS: usize = 4;

/// What an operation lacks to be launched, **without consulting the
/// data**: a piece of data, never a sentence (the interface turns it into a sentence
/// by an exhaustive `match`). What only reading the project reveals — no
/// series, reference column missing, column gone — is not here.
///
/// A point or a zone "not yet placed" is expressed by a `NaN` in the
/// operation, as everywhere in the crate (`NaN`: empty): the draft of a
/// form is then described as an [`Operation`] even if incomplete.
#[derive(Debug, Clone, PartialEq)]
pub enum Readiness {
    /// Nothing is missing.
    Ready,
    /// Not enough points placed for the baseline.
    NeedsPoints { needed: usize, have: usize },
    /// At least one background zone.
    NeedsZone,
    /// The free model expression is unreadable.
    BadExpression(ExprError),
    /// The model has no parameter.
    NoParameter,
    /// A setting is refused by the calculation (for example a degree above the cap).
    Invalid(AnalysisError),
}

impl Operation {
    /// What prevents launching the operation, without reading any data (see
    /// [`Readiness`]). Does not redo [`Operation::validate`]: that one judges the
    /// caps, this one what remains to be placed.
    pub fn readiness(&self) -> Readiness {
        match self {
            Operation::Baseline { method, .. } => match method {
                BaselineMethod::Constant { point } if !is_posed(*point) => {
                    Readiness::NeedsPoints { needed: 1, have: 0 }
                }
                BaselineMethod::ConstantZone { zone } if !is_posed(*zone) => Readiness::NeedsZone,
                BaselineMethod::Line { p1, p2 } => {
                    let have = [p1, p2].into_iter().filter(|p| is_posed(**p)).count();
                    if have < 2 {
                        Readiness::NeedsPoints { needed: 2, have }
                    } else {
                        Readiness::Ready
                    }
                }
                BaselineMethod::Zones { zones, .. }
                    if zones.is_empty() || !zones.iter().all(|z| is_posed(*z)) =>
                {
                    Readiness::NeedsZone
                }
                BaselineMethod::Anchors { points, .. } => {
                    // An unplaced (non-finite) anchor does not count.
                    let have = points.iter().filter(|p| is_posed(**p)).count();
                    if have < 2 {
                        Readiness::NeedsPoints { needed: 2, have }
                    } else {
                        Readiness::Ready
                    }
                }
                _ => Readiness::Ready,
            },
            Operation::Fit { spec, .. } => match spec.model.compile() {
                Err(AnalysisError::BadExpression(error)) => Readiness::BadExpression(error),
                Err(other) => Readiness::Invalid(other),
                Ok(compiled) if compiled.param_count() == 0 => Readiness::NoParameter,
                Ok(_) => Readiness::Ready,
            },
            // An unplaced range (NaN) is not "the whole curve": `run`
            // refuses it (#276), so it remains to be placed.
            Operation::Integrate { spec } if !spec.ranges.iter().all(|r| is_posed(*r)) => {
                Readiness::NeedsZone
            }
            Operation::Correct { .. }
            | Operation::Integrate { .. }
            | Operation::Peaks { .. }
            | Operation::Reconvolution { .. }
            | Operation::DirectDeconvolution { .. }
            | Operation::Smooth { .. } => Readiness::Ready,
        }
    }

    /// Removes from the operation everything that designates **data**: ranges
    /// of integration, fit and peaks, starting positions of the peaks,
    /// per-parameter settings of a fit or reconvolution (initial
    /// values, bounds, fixed **and shared** parameters), window of a
    /// reconvolution. What remains is the method and its settings (model, degree,
    /// peak recentering window, output checkboxes) and the definition of a
    /// baseline. The length of `params` is kept.
    pub fn forget_positions(&mut self) {
        match self {
            Operation::Integrate { spec } => spec.ranges.clear(),
            Operation::Fit { spec, range, .. } => {
                spec.params = vec![ParamSetting::default(); spec.params.len()];
                *range = None;
            }
            Operation::Peaks { spec, range, .. } => {
                spec.seeds.clear();
                *range = None;
            }
            Operation::Reconvolution { spec, .. } => {
                spec.params = vec![ParamSetting::default(); spec.params.len()];
                spec.window = None;
            }
            Operation::Baseline { .. }
            | Operation::Correct { .. }
            | Operation::DirectDeconvolution { .. }
            | Operation::Smooth { .. } => {}
        }
    }

    /// Refuses settings above the cap before any calculation.
    pub fn validate(&self) -> Result<(), AnalysisError> {
        let bad = |name: &str| {
            Err(AnalysisError::InvalidParameter {
                name: name.to_string(),
            })
        };
        match self {
            Operation::Baseline { method, .. } if method.check_finite().is_err() => {
                method.check_finite()
            }
            Operation::Baseline {
                method: BaselineMethod::Zones { degree, .. },
                ..
            } if *degree > MAX_DEGREE => bad("degree"),
            Operation::Fit { range, .. } if range.is_some_and(|r| !is_posed(r)) => bad("range"),
            Operation::Fit { spec, .. } => match spec.model {
                Model::Polynomial { degree } if !(1..=MAX_DEGREE).contains(&degree) => {
                    bad("degree")
                }
                _ if no_free_parameter(spec) => Err(AnalysisError::NoFreeParameter),
                _ => Ok(()),
            },
            Operation::Integrate { spec } if !spec.ranges.iter().all(|r| is_posed(*r)) => {
                bad("ranges")
            }
            Operation::Peaks { auto, .. } if auto.is_some_and(|a| a.max_peaks > MAX_PEAKS) => {
                bad("max_peaks")
            }
            Operation::Peaks { spec, .. } if spec.center_window.is_some_and(|w| !w.is_finite()) => {
                bad("center_window")
            }
            Operation::Peaks { auto, .. }
                if auto.is_some_and(|a| !a.min_prominence.is_finite()) =>
            {
                bad("min_prominence")
            }
            // A placed seed has only finite values (center always;
            // height and width when given). The imposed width
            // is moreover strictly positive (#285); the height,
            // of free sign, is judged by the fit.
            Operation::Peaks { spec, .. }
                if !spec.seeds.iter().all(|s| {
                    s.center.is_finite()
                        && s.height.is_none_or(f64::is_finite)
                        && s.fwhm.is_none_or(|f| f.is_finite() && f > 0.0)
                }) =>
            {
                bad("seeds")
            }
            Operation::Peaks { range, .. } if range.is_some_and(|r| !is_posed(r)) => bad("range"),
            Operation::DirectDeconvolution { shift, .. } if !shift.is_finite() => bad("shift"),
            Operation::DirectDeconvolution { method, .. } => match method {
                DirectMethod::RichardsonLucy { iterations }
                    if !(1..=MAX_ITERATIONS).contains(iterations) =>
                {
                    bad("iterations")
                }
                DirectMethod::Wiener { noise: Some(k) } if !(k.is_finite() && *k > 0.0) => {
                    bad("noise")
                }
                _ => Ok(()),
            },
            Operation::Reconvolution { spec, .. }
                if !(1..=MAX_COMPONENTS).contains(&spec.components) =>
            {
                bad("components")
            }
            Operation::Reconvolution { spec, .. } if !spec.shift_initial.is_finite() => {
                bad("shift_initial")
            }
            Operation::Reconvolution { spec, .. } if spec.window.is_some_and(|w| !is_posed(w)) => {
                bad("window")
            }
            Operation::Smooth { spec } => spec.validate(),
            _ => Ok(()),
        }
    }
}

/// The model has no free parameter: none, or all fixed (the solver
/// refuses anyway with the same error). A model that does not compile
/// is not judged here: its own error comes out at the fit.
fn no_free_parameter(spec: &FitSpec) -> bool {
    spec.model.compile().is_ok_and(|compiled| {
        (0..compiled.param_count()).all(|j| spec.params.get(j).is_some_and(|p| p.fixed))
    })
}

/// A placed point or zone has only finite coordinates.
fn is_posed(pair: (f64, f64)) -> bool {
    pair.0.is_finite() && pair.1.is_finite()
}

/// Runs `op` on each input. An error on one input cancels the batch
/// (the caller decides whether to rerun the others). A global fit processes
/// all the inputs together.
pub fn run(
    op: &Operation,
    inputs: &[SeriesInput],
    aux: Option<&AuxInput>,
) -> Result<Vec<Outcome>, AnalysisError> {
    op.validate()?;
    if inputs.is_empty() {
        return Err(AnalysisError::NotEnoughPoints {
            found: 0,
            needed: 1,
        });
    }
    let aligned: Vec<Aligned> = inputs.iter().map(align).collect();
    if let Operation::Fit {
        spec,
        range,
        residuals,
    } = op
    {
        return run_fit(spec, *range, *residuals, inputs, &aligned);
    }
    let mut out = Vec::new();
    for (input, a) in inputs.iter().zip(&aligned) {
        let mut o = Outcome {
            skipped: a.skipped,
            ..Default::default()
        };
        match op {
            Operation::Baseline {
                method,
                op,
                keep_baseline,
            } => run_baseline(&mut o, a, input, method, *op, *keep_baseline)?,
            Operation::Correct { op } => {
                let reference = aux_series(aux)?;
                let holes = aux_holes(aux);
                let c = correct::correct(&a.series, &reference, &holes, *op)?;
                o.column("corrected", None, a.expand(&c.values));
                if c.bridged > 0 {
                    o.warnings.push(Warning::BridgedReferenceGap(c.bridged));
                }
                if c.division_by_zero > 0 {
                    o.warnings.push(Warning::DivisionByZero(c.division_by_zero));
                }
                o.value(
                    "count.out_of_reference",
                    None,
                    c.out_of_reference as f64,
                    None,
                );
            }
            Operation::Integrate { spec } => {
                let r = integrate::integrate(&a.series, spec)?;
                for (i, ra) in r.areas.iter().enumerate() {
                    o.value("area", Some(i + 1), ra.area, None);
                    o.value("area.from", Some(i + 1), ra.from, None);
                    o.value("area.to", Some(i + 1), ra.to, None);
                }
                o.value("area.total", None, r.total, None);
                if let Some(c) = &r.cumulative {
                    o.column("cumulative", None, a.expand(c));
                }
                if let Some(n) = &r.normalized {
                    o.column("normalized", None, a.expand(n));
                }
            }
            Operation::Fit { .. } => unreachable!("handled above"),
            Operation::Peaks {
                spec,
                auto,
                range,
                residuals,
            } => run_peaks(&mut o, a, spec, *auto, *range, *residuals)?,
            Operation::Reconvolution { spec, residuals } => {
                let irf = aux_series(aux).map_err(|_| AnalysisError::EmptyResponse)?;
                let r = deconv::reconvolve(&a.series, &irf, spec)?;
                for (i, name) in r.names.iter().enumerate() {
                    o.value(format!("param.{name}"), None, r.values[i], r.errors[i]);
                }
                o.value("stat.tau_amplitude", None, r.tau_amplitude_weighted, None);
                o.value("stat.tau_intensity", None, r.tau_intensity_weighted, None);
                o.stats(r.points, r.r_squared, r.reduced_chi2, r.dof);
                o.warn_undetermined(r.undetermined);
                o.warn_plateau(r.plateau);
                // `fitted` is aligned on the sorted signal: it is read again at each X of
                // the input (linear interpolation, exact at the nodes), including
                // at the rows whose Y is empty.
                let by_x = |x: f64| interpolate(&r.x, &r.fitted, x).unwrap_or(f64::NAN);
                let fitted = a.eval_all(by_x);
                if *residuals {
                    let y = a.y_rows(input);
                    o.column(
                        "residuals",
                        None,
                        y.iter().zip(&fitted).map(|(y, f)| y - f).collect(),
                    );
                }
                o.column("fit", None, fitted);
                // Extend, do not overwrite: `Undetermined` and `NotConverged` are already set
                // above; `deconv_warnings` only produces the other three (#292).
                o.warnings.extend(deconv_warnings(r.warnings));
            }
            Operation::Smooth { spec } => {
                let r = smooth::smooth(&a.series, spec)?;
                o.column("smoothed", None, a.expand(&r.values));
                o.value("count.window", None, r.window as f64, None);
                if !r.step_uniform && matches!(spec.method, SmoothMethod::SavitzkyGolay { .. }) {
                    o.warnings.push(Warning::SignalStepNotUniform);
                }
            }
            Operation::DirectDeconvolution { method, shift } => {
                let irf = aux_series(aux).map_err(|_| AnalysisError::EmptyResponse)?;
                let r = deconv::deconvolve(&a.series, &irf, *shift, *method)?;
                let sorted = a.series.sorted();
                let by_x = |x: f64| interpolate(&sorted.x, &r.values, x).unwrap_or(f64::NAN);
                o.column("deconvolved", None, a.eval_all(by_x));
                if let Some(k) = r.noise_ratio {
                    o.value("stat.noise_ratio", None, k, None);
                }
                o.warnings = deconv_warnings(r.warnings);
            }
        }
        out.push(o);
    }
    Ok(out)
}

fn run_baseline(
    o: &mut Outcome,
    a: &Aligned,
    input: &SeriesInput,
    method: &BaselineMethod,
    op: BaselineOp,
    keep: bool,
) -> Result<(), AnalysisError> {
    let b: Baseline = baseline::build(method, &a.series)?;
    let base = a.eval_all(|x| b.eval(x));
    let corrected: Vec<f64> = (0..a.len)
        .map(|i| baseline::apply(op, input.y[i], base[i]))
        .collect();
    // Division by an exactly zero baseline: counted (C5, #313).
    let by_zero = if op == BaselineOp::Divide {
        (0..a.len)
            .filter(|&i| input.y[i].is_finite() && base[i] == 0.0)
            .count()
    } else {
        0
    };
    if by_zero > 0 {
        o.warnings.push(Warning::DivisionByZero(by_zero));
    }
    o.column("corrected", None, corrected);
    if keep {
        o.column("baseline", None, base);
    }
    o.value("baseline.points", None, b.points_used as f64, None);
    if let Some((slope, intercept)) = b.as_line() {
        o.value("baseline.slope", None, slope, None);
        o.value("baseline.intercept", None, intercept, None);
    }
    Ok(())
}

fn run_fit(
    spec: &FitSpec,
    range: Option<(f64, f64)>,
    residuals: bool,
    inputs: &[SeriesInput],
    aligned: &[Aligned],
) -> Result<Vec<Outcome>, AnalysisError> {
    let model = spec.model.compile()?;
    let mut fit_inputs = Vec::new();
    for a in aligned {
        let s = restrict(&a.series, range)?;
        // σ aligned on the points kept by the range.
        let sigma = a.sigma.as_ref().map(|all| {
            let (lo, hi) = range.map_or((f64::NEG_INFINITY, f64::INFINITY), |(p, q)| {
                (p.min(q), p.max(q))
            });
            a.series
                .x
                .iter()
                .zip(all)
                .filter(|(x, _)| **x >= lo && **x <= hi)
                .map(|(_, s)| *s)
                .collect::<Vec<f64>>()
        });
        fit_inputs.push(FitInput { series: s, sigma });
    }
    let report = fit::fit(&fit_inputs, spec)?;
    let mut out = Vec::new();
    for (k, fs) in report.series.iter().enumerate() {
        let a = &aligned[k];
        let mut o = Outcome {
            skipped: a.skipped,
            ..Default::default()
        };
        for (j, name) in report.param_names.iter().enumerate() {
            o.value(format!("param.{name}"), None, fs.values[j], fs.errors[j]);
        }
        o.stats(fs.points, fs.r_squared, fs.reduced_chi2, fs.dof);
        o.warn_undetermined(fs.undetermined);
        o.warn_plateau(fs.plateau);
        o.warn_shifted_x(fs.shifted_x);
        let fitted = a.eval_in(range, |x| model.eval(&fs.values, x));
        if residuals {
            let y = a.y_rows(&inputs[k]);
            o.column(
                "residuals",
                None,
                y.iter().zip(&fitted).map(|(y, f)| y - f).collect(),
            );
        }
        o.column("fit", None, fitted);
        out.push(o);
    }
    Ok(out)
}

fn run_peaks(
    o: &mut Outcome,
    a: &Aligned,
    spec: &PeaksSpec,
    auto: Option<AutoDetect>,
    range: Option<(f64, f64)>,
    residuals: bool,
) -> Result<(), AnalysisError> {
    let s = restrict(&a.series, range)?;
    let mut spec = spec.clone();
    if spec.seeds.is_empty() {
        let d = auto.ok_or(AnalysisError::InvalidParameter {
            name: "peaks".into(),
        })?;
        let found = peaks::detect(&s.sorted(), d.min_prominence, d.max_peaks);
        spec.seeds = found
            .iter()
            .map(|p| PeakSeed {
                center: p.x,
                height: None,
                fwhm: None,
            })
            .collect();
        if spec.seeds.is_empty() {
            return Err(AnalysisError::InvalidParameter {
                name: "peaks".into(),
            });
        }
    }
    let r = peaks::decompose(&s, &spec)?;
    for (i, p) in r.peaks.iter().enumerate() {
        let n = Some(i + 1);
        o.value("peak.height", n, p.height, p.height_error);
        o.value("peak.center", n, p.center, p.center_error);
        o.value("peak.fwhm", n, p.fwhm, p.fwhm_error);
        if let (Some(g), Some(l)) = (p.fwhm_gauss, p.fwhm_lorentz) {
            o.value("peak.fwhm_gauss", n, g, None);
            o.value("peak.fwhm_lorentz", n, l, None);
        }
        o.value("peak.area", n, p.area, None);
        o.value("peak.area_percent", n, p.area_percent, None);
    }
    for (i, (v, e)) in r.background.iter().enumerate() {
        let name = if i == 0 { "y0" } else { "slope" };
        o.value(format!("background.{name}"), None, *v, *e);
    }
    o.stats(r.points, r.r_squared, r.reduced_chi2, r.dof);
    o.warn_undetermined(r.undetermined);
    o.warn_plateau(r.plateau);
    for i in 0..r.model.peak_count() {
        o.column(
            "peak",
            Some(i + 1),
            a.eval_in(range, |x| r.model.peak(i, x)),
        );
    }
    if spec.background != peaks::PeakBackground::None {
        o.column(
            "background",
            None,
            a.eval_in(range, |x| r.model.background(x)),
        );
    }
    let total = a.eval_in(range, |x| r.model.total(x));
    if residuals {
        o.column("residuals", None, fill_residuals(a, &total));
    }
    o.column("peaks_sum", None, total);
    Ok(())
}

/// `y − model` on the clean rows where the model is defined.
fn fill_residuals(a: &Aligned, total: &[f64]) -> Vec<f64> {
    let mut out = vec![f64::NAN; a.len];
    for (k, &row) in a.rows.iter().enumerate() {
        if total[row].is_finite() {
            out[row] = a.series.y[k] - total[row];
        }
    }
    out
}
