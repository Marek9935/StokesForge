//! `pyst.analysis` (#181): the computations of the Analysis menu, from a script.
//!
//! Each function describes **one operation** of `stokesforge-analysis` on a
//! series (a Y column, its optional X) and records it as a project
//! calculation: the same output columns, the same trace, the same automatic
//! recalculation as from the analysis window. The write is a
//! `Command::AddCalculation` of the script's write group: a single Ctrl+Z
//! (or `pyst.undo()`) undoes the calculation with its columns.
//!
//! # What is returned
//!
//! A dictionary: `columns` (output key -> handle of the written column;
//! `"peak.2"` for the second peak), `values` (`"area.total"`, `"param.tau"`,
//! `"peak.height.1"`...), `errors` (standard error of the values that have
//! one) and `warnings` (English sentences). The keys are those of the
//! computation crate.
//!
//! # Batches (#201)
//!
//! `y` accepts a handle **or a list** of handles: the batch is a single
//! multi-series calculation, as in the analysis window, undone by a single
//! Ctrl+Z. `x` and `sigma` apply to all the series (one handle) or one per
//! series (a list of the same length). A batch's result is
//! `{"series": [dict, ...]}`, one dictionary per series in the order of `y`;
//! `fit(shared=[names])` makes it a global fit.
//!
//! # Names of the written columns
//!
//! Long name, unit and comment are those of the analysis window, through the
//! single rule of `stokesforge-core` (`OutputName`, #256): "Intensity
//! (B-corr)", "Baseline of B"... The unit is the source's, the comment is
//! the family title. Only difference: the text is always in English (the
//! interface language does not go through scripts). The name is frozen at
//! creation; recalculation rewrites only the data.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyModule};
use pyo3::wrap_pyfunction;
use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp, Interpolation};
use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::deconv::{DirectMethod, ReconvSpec};
use stokesforge_analysis::fit::{FitSpec, ParamSetting};
use stokesforge_analysis::integrate::{IntegrateSpec, Under};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{AutoDetect, Operation, Readiness, Warning};
use stokesforge_analysis::peaks::{PeakBackground, PeakSeed, PeakShape, PeaksSpec};
use stokesforge_analysis::smooth::{SmoothMethod, SmoothSpec, SmoothWindow};
use stokesforge_commands::Command;
use stokesforge_core::{
    compute, family_english, output_unit, wants_output, AuxRef, CalculationDraft, ColumnMeta,
    Computed, NodeId, OutputDraft, OutputName, PeakOutputs, SeriesRef, Tree,
};

use crate::bridge::{checked_node, HostError};
use crate::engine;
use crate::surface::{to_py_error, Handle};

fn invalid(message: impl Into<String>) -> PyErr {
    to_py_error(HostError::InvalidValue(message.into()))
}

fn choice<T: Copy>(name: &str, value: &str, options: &[(&str, T)]) -> PyResult<T> {
    options
        .iter()
        .find(|(key, _)| *key == value)
        .map(|(_, v)| *v)
        .ok_or_else(|| {
            let known: Vec<String> = options.iter().map(|(k, _)| format!("\"{k}\"")).collect();
            invalid(format!(
                "unknown {name} \"{value}\": use {}",
                known.join(", ")
            ))
        })
}

fn finite_range(name: &str, range: Option<(f64, f64)>) -> PyResult<Option<(f64, f64)>> {
    match range {
        Some((a, b)) if a.is_finite() && b.is_finite() => Ok(Some((a.min(b), a.max(b)))),
        Some(_) => Err(invalid(format!("{name} must hold finite numbers"))),
        None => Ok(None),
    }
}

/// The batch to record. The name, unit and comment of each column are the
/// core rule (`OutputName`, `output_unit`, `family_english`): the same as
/// those of the analysis window, in English; `wants_output` chooses the
/// columns written.
#[allow(clippy::too_many_arguments)]
fn draft(
    tree: &Tree,
    series: &[SeriesRef],
    op: Operation,
    aux: Option<AuxRef>,
    computed: Computed,
    keep_results: bool,
    auto_recalc: bool,
    peaks: PeakOutputs,
) -> CalculationDraft {
    let kind = op.kind();
    let outputs = computed
        .columns
        .iter()
        .filter(|c| wants_output(&op, &c.key, peaks))
        .map(|c| {
            let source = &series[c.series];
            OutputDraft {
                series: c.series,
                key: c.key.clone(),
                index: c.index,
                meta: ColumnMeta {
                    long_name: OutputName::of(tree, kind, source, &c.key, c.index).english(),
                    unit: output_unit(tree, source, &c.key),
                    comment: family_english(kind).to_string(),
                    ..ColumnMeta::default()
                },
                data: c.data.clone(),
            }
        })
        .collect();
    CalculationDraft {
        operation: op,
        series: series.to_vec(),
        aux,
        outputs,
        results: if keep_results {
            computed.results
        } else {
            Vec::new()
        },
        keep_results,
        auto_recalc,
        low_r2_dialog: true,
    }
}

fn host<T>(result: Result<T, impl std::fmt::Display>) -> PyResult<T> {
    result.map_err(|e| invalid(e.to_string()))
}

/// A handle read from an argument, before validation against the snapshot.
type Raw = (NodeId, u64);

/// The input handles: one or several series (a **batch**, a single
/// multi-series calculation as in the analysis window, #201).
struct Inputs {
    /// `y` was a list: the result is returned series by series.
    batch: bool,
    y: Vec<Raw>,
    /// One X per series (the same one repeated when only one is given).
    x: Vec<Option<Raw>>,
    sigma: Vec<Option<Raw>>,
    aux: Option<(Raw, Option<Raw>)>,
}

fn raw(handle: &Handle) -> Raw {
    (handle.id, handle.epoch)
}

/// `Handle` or list of `Handle`.
fn handles(name: &str, object: &Bound<'_, PyAny>) -> PyResult<(Vec<Raw>, bool)> {
    if let Ok(one) = object.cast::<Handle>() {
        return Ok((vec![raw(one.get())], false));
    }
    let many: Vec<Bound<'_, PyAny>> = object
        .extract()
        .map_err(|_| invalid(format!("{name} must be a handle or a list of handles")))?;
    if many.is_empty() {
        return Err(invalid(format!("{name} is an empty list")));
    }
    let list = many
        .iter()
        .map(|item| {
            item.cast::<Handle>()
                .map(|h| raw(h.get()))
                .map_err(|_| invalid(format!("{name} must hold only handles")))
        })
        .collect::<PyResult<Vec<_>>>()?;
    Ok((list, true))
}

impl Inputs {
    /// Gathers the `y`, `x`, `sigma` arguments (each a handle or a list): a
    /// single handle for `x` or `sigma` applies to all the series.
    fn new(
        y: &Bound<'_, PyAny>,
        x: Option<&Bound<'_, PyAny>>,
        sigma: Option<&Bound<'_, PyAny>>,
        aux: Option<(&Handle, Option<&Handle>)>,
    ) -> PyResult<Self> {
        let (y, batch) = handles("y", y)?;
        let spread = |name: &str, arg: Option<&Bound<'_, PyAny>>| -> PyResult<Vec<Option<Raw>>> {
            let Some(arg) = arg else {
                return Ok(vec![None; y.len()]);
            };
            let (list, many) = handles(name, arg)?;
            match (many, list.len()) {
                (false, _) => Ok(vec![Some(list[0]); y.len()]),
                (true, n) if n == y.len() => Ok(list.into_iter().map(Some).collect()),
                (true, n) => Err(invalid(format!(
                    "{name} has {n} handles but y has {}: give one handle or one per series",
                    y.len()
                ))),
            }
        };
        let x = spread("x", x)?;
        let sigma = spread("sigma", sigma)?;
        Ok(Self {
            batch,
            y,
            x,
            sigma,
            aux: aux.map(|(r, rx)| (raw(r), rx.map(raw))),
        })
    }

    fn all(&self) -> impl Iterator<Item = Raw> + '_ {
        self.y
            .iter()
            .copied()
            .chain(self.x.iter().flatten().copied())
            .chain(self.sigma.iter().flatten().copied())
            .chain(
                self.aux
                    .iter()
                    .flat_map(|(y, x)| std::iter::once(*y).chain(*x)),
            )
    }
}

/// A series' dictionary: `columns`, `values`, `errors`, `warnings`.
fn series_dict<'py>(
    py: Python<'py>,
    calc: &stokesforge_core::Calculation,
    series: usize,
    result: &stokesforge_core::SeriesResult,
    epoch: u64,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    let columns = PyDict::new(py);
    for out in calc.outputs.iter().filter(|o| o.series == series) {
        if let Some(id) = out.column {
            let key = match out.index {
                Some(i) if out.key == "peak" => format!("peak.{i}"),
                _ => out.key.clone(),
            };
            columns.set_item(key, Py::new(py, Handle { id, epoch })?)?;
        }
    }
    let numbers = PyDict::new(py);
    let errors = PyDict::new(py);
    for v in &result.values {
        let key = match v.index {
            Some(i) => format!("{}.{i}", v.key),
            None => v.key.clone(),
        };
        numbers.set_item(&key, v.value)?;
        if let Some(e) = v.error {
            errors.set_item(&key, e)?;
        }
    }
    let warnings = PyList::empty(py);
    for w in &result.warnings {
        warnings.append(match w {
            Warning::SignalStepNotUniform => "the X step is not constant".to_string(),
            Warning::IrfStepMismatch => "the response has a different X step".to_string(),
            Warning::NegativeClipped(n) => format!("{n} negative points were set to zero"),
            Warning::NotConverged => {
                "the fit did not converge: the model does not reach the data".to_string()
            }
            Warning::Undetermined(n) => {
                format!("{n} parameter(s) are not determined by the data (no uncertainty)")
            }
            Warning::IrfTruncated => {
                "the response is cut off above 1% of its peak (missing tail taken as zero)"
                    .to_string()
            }
            Warning::BridgedReferenceGap(n) => {
                format!("{n} point(s) were interpolated across an empty cell of the reference")
            }
            Warning::DivisionByZero(n) => format!("{n} division(s) by zero were left empty"),
            Warning::ShiftedX => {
                "the polynomial was fitted with constraints on X values far from 0: precision may be low (center X, e.g. subtract its mean)"
                    .to_string()
            }
        })?;
    }
    dict.set_item("columns", columns)?;
    dict.set_item("values", numbers)?;
    dict.set_item("errors", errors)?;
    dict.set_item("warnings", warnings)?;
    Ok(dict)
}

/// Records the operation and returns the results dictionary: flat for a
/// single series, `{"series": [dict, ...]}` for a batch.
fn run(
    py: Python<'_>,
    op: Operation,
    inputs: Inputs,
    keep_results: bool,
    auto_recalc: bool,
    replace_original: bool,
    peaks: PeakOutputs,
) -> PyResult<Py<PyDict>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let batch = inputs.batch;
    let outcome = py
        .detach(|| -> Result<_, HostError> {
            let session = context.session();
            let prepare = |snapshot: &stokesforge_session::Snapshot| {
                for (id, epoch) in inputs.all() {
                    checked_node(snapshot, id, epoch)?;
                }
                let series: Vec<SeriesRef> = (0..inputs.y.len())
                    .map(|i| SeriesRef {
                        x: inputs.x[i].map(|h| h.0),
                        y: inputs.y[i].0,
                        sigma: inputs.sigma[i].map(|h| h.0),
                    })
                    .collect();
                let aux = inputs.aux.map(|(y, x)| AuxRef {
                    x: x.map(|h| h.0),
                    y: y.0,
                });
                let computed = compute(&snapshot.tree, &op, &series, aux.as_ref())
                    .map_err(|e| HostError::InvalidValue(e.to_string()))?;
                Ok::<_, HostError>((series, aux, computed))
            };
            let snapshot = session.snapshot().map_err(HostError::from_session)?;
            let (mut series, mut aux, mut computed) = prepare(&snapshot)?;
            let mut tree = snapshot.tree.clone();
            // The earlier calculations: ours is the one added to them, even if
            // another client creates one at the same moment.
            let mut known: std::collections::BTreeSet<NodeId> =
                snapshot.tree.calculations().keys().copied().collect();
            if context.begin_write()? {
                // The group has just opened: read again to validate the epoch.
                let snapshot = session.snapshot().map_err(HostError::from_session)?;
                (series, aux, computed) = prepare(&snapshot)?;
                tree = snapshot.tree.clone();
                known = snapshot.tree.calculations().keys().copied().collect();
            }
            let values = computed.results.clone();
            let mut draft = draft(
                &tree,
                &series,
                op.clone(),
                aux,
                computed,
                keep_results || replace_original,
                auto_recalc,
                peaks,
            );
            if replace_original {
                draft.complete_for_replacement(&tree);
                let mut ids: Vec<NodeId> = series
                    .iter()
                    .flat_map(|s| [Some(s.y), s.x, s.sigma].into_iter().flatten())
                    .collect();
                if let Some(aux) = &draft.aux {
                    ids.push(aux.y);
                    ids.extend(aux.x);
                }
                ids.sort();
                ids.dedup();
                let expected_versions = ids
                    .iter()
                    .filter_map(|id| {
                        tree.node(*id)
                            .and_then(|n| n.as_column())
                            .map(|c| (*id, c.version()))
                    })
                    .collect();
                session
                    .execute(Command::ReplaceAnalysisData {
                        draft,
                        expected_versions,
                    })
                    .map_err(HostError::from_session)?;
                let after = session.snapshot().map_err(HostError::from_session)?;
                let calc = stokesforge_core::Calculation {
                    operation: op.clone(),
                    series: series.clone(),
                    aux: inputs.aux.map(|(y, x)| AuxRef {
                        y: y.0,
                        x: x.map(|h| h.0),
                    }),
                    outputs: series
                        .iter()
                        .enumerate()
                        .map(|(rank, s)| stokesforge_core::OutputRef {
                            series: rank,
                            key: if matches!(op, Operation::Smooth { .. }) {
                                "smoothed"
                            } else {
                                "corrected"
                            }
                            .into(),
                            index: None,
                            column: Some(s.y),
                        })
                        .collect(),
                    results: values.clone(),
                    keep_results: true,
                    auto_recalc: false,
                    low_r2_dialog: true,
                    stale: None,
                };
                return Ok((std::sync::Arc::new(calc), values, after.epoch));
            }
            session
                .execute(Command::AddCalculation { draft })
                .map_err(HostError::from_session)?;
            let after = session.snapshot().map_err(HostError::from_session)?;
            let calc = after
                .tree
                .calculations()
                .iter()
                .rfind(|(id, _)| !known.contains(id))
                .map(|(_, c)| c.clone())
                .ok_or_else(|| {
                    HostError::InvalidValue("the calculation was not recorded".into())
                })?;
            Ok((calc, values, after.epoch))
        })
        .map_err(to_py_error)?;
    let (calc, values, epoch) = outcome;
    if !batch {
        let one = values.first().cloned().unwrap_or_default();
        return Ok(series_dict(py, &calc, 0, &one, epoch)?.unbind());
    }
    let list = PyList::empty(py);
    for (i, result) in values.iter().enumerate() {
        list.append(series_dict(py, &calc, i, result, epoch)?)?;
    }
    let result = PyDict::new(py);
    result.set_item("series", list)?;
    Ok(result.unbind())
}

/// The "baseline" operation described by the arguments of `baseline`
/// (shared with `readiness`).
fn baseline_operation(
    method: &str,
    points: Vec<(f64, f64)>,
    zones: Option<Vec<(f64, f64)>>,
    degree: u32,
    interpolation: &str,
    op: &str,
    keep_baseline: bool,
) -> PyResult<Operation> {
    let method = match method {
        "constant" => {
            let zones = zones.unwrap_or_default();
            match (points.as_slice(), zones.as_slice()) {
                ([point], []) => BaselineMethod::Constant { point: *point },
                ([], [zone]) => BaselineMethod::ConstantZone { zone: *zone },
                _ => {
                    return Err(invalid(
                        "method \"constant\" needs exactly one point or one zone",
                    ))
                }
            }
        }
        "line" => {
            let [p1, p2] = points[..] else {
                return Err(invalid("method \"line\" needs exactly two points"));
            };
            BaselineMethod::Line { p1, p2 }
        }
        "zones" => BaselineMethod::Zones {
            zones: zones.ok_or_else(|| invalid("method \"zones\" needs zones=[(from, to), …]"))?,
            degree,
        },
        "anchors" => BaselineMethod::Anchors {
            points,
            interpolation: choice(
                "interpolation",
                interpolation,
                &[
                    ("segments", Interpolation::Segments),
                    ("spline", Interpolation::Spline),
                ],
            )?,
        },
        other => {
            return Err(invalid(format!(
                "unknown method \"{other}\": use \"constant\", \"anchors\", \"line\" or \"zones\""
            )))
        }
    };
    let op = choice(
        "op",
        op,
        &[
            ("subtract", BaselineOp::Subtract),
            ("divide", BaselineOp::Divide),
        ],
    )?;
    let operation = Operation::Baseline {
        method,
        op,
        keep_baseline,
    };
    Ok(operation)
}

/// The "integral" operation described by the arguments of `integrate`
/// (shared with `readiness`).
fn integrate_operation(
    ranges: Option<Vec<(f64, f64)>>,
    under: &str,
    cumulative: bool,
    normalize: bool,
) -> PyResult<Operation> {
    let under = choice(
        "under",
        under,
        &[("zero", Under::Zero), ("line", Under::Line)],
    )?;
    let spec = IntegrateSpec {
        ranges: ranges.unwrap_or_default(),
        under,
        cumulative,
        normalize,
    };
    Ok(Operation::Integrate { spec })
}

/// The "fit" operation described by the arguments of `fit` (shared
/// with `readiness`).
#[allow(clippy::too_many_arguments)]
fn fit_operation(
    model: &str,
    degree: u32,
    initial: Option<std::collections::HashMap<String, f64>>,
    bounds: Option<std::collections::HashMap<String, (f64, f64)>>,
    fixed: Option<Vec<String>>,
    shared: Option<Vec<String>>,
    fit_range: Option<(f64, f64)>,
    residuals: bool,
    lenient: bool,
) -> PyResult<Operation> {
    let model = match model {
        "linear" => Model::Linear,
        "polynomial" => Model::Polynomial { degree },
        "exponential" => Model::Exponential,
        "exponential_offset" => Model::ExponentialOffset,
        "exponential2" => Model::Exponential2,
        "logarithm" => Model::Logarithm,
        "power" => Model::Power,
        "gaussian" => Model::Gaussian,
        "lorentzian" => Model::Lorentzian,
        formula => Model::Custom {
            formula: formula.to_string(),
        },
    };
    let range = finite_range("fit_range", fit_range)?;
    let compiled = match model.compile() {
        Ok(compiled) => compiled,
        // `readiness` must be able to *say* that an expression is unreadable:
        // the operation is then returned without parameters, and it is the
        // one that carries the refusal (`Operation::readiness`).
        Err(_) if lenient => {
            return Ok(Operation::Fit {
                spec: FitSpec {
                    model,
                    params: Vec::new(),
                },
                range,
                residuals,
            })
        }
        Err(error) => return host(Err(error)),
    };
    let names: Vec<String> = compiled.params().iter().map(|p| p.name.clone()).collect();
    let (initial, bounds, fixed, shared) = (
        initial.unwrap_or_default(),
        bounds.unwrap_or_default(),
        fixed.unwrap_or_default(),
        shared.unwrap_or_default(),
    );
    for name in initial
        .keys()
        .chain(bounds.keys())
        .chain(fixed.iter())
        .chain(shared.iter())
    {
        if !names.contains(name) {
            return Err(invalid(format!(
                "the model has no parameter \"{name}\": its parameters are {}",
                names.join(", ")
            )));
        }
    }
    let params = names
        .iter()
        .map(|name| ParamSetting {
            initial: initial.get(name).copied(),
            fixed: fixed.contains(name),
            min: bounds.get(name).map(|b| b.0),
            max: bounds.get(name).map(|b| b.1),
            shared: shared.contains(name),
        })
        .collect();
    let operation = Operation::Fit {
        spec: FitSpec { model, params },
        range,
        residuals,
    };
    Ok(operation)
}

/// Baseline: `method` = `"anchors"` (with `points=[(x, y), ...]`),
/// `"constant"` (a point or a zone), `"line"` (two `points`) or
/// `"zones"` (with `zones=[(from, to), ...]` and
/// `degree`). `op`: `"subtract"` or `"divide"`.
#[pyfunction]
#[pyo3(signature = (y, x=None, *, method="anchors", points=None, zones=None, degree=1, interpolation="segments", op="subtract", keep_baseline=false, keep_results=false, auto_recalc=true, replace_original=false))]
#[allow(clippy::too_many_arguments)]
fn baseline(
    py: Python<'_>,
    y: &Bound<'_, PyAny>,
    x: Option<&Bound<'_, PyAny>>,
    method: &str,
    points: Option<Vec<(f64, f64)>>,
    zones: Option<Vec<(f64, f64)>>,
    degree: u32,
    interpolation: &str,
    op: &str,
    keep_baseline: bool,
    keep_results: bool,
    auto_recalc: bool,
    replace_original: bool,
) -> PyResult<Py<PyDict>> {
    let operation = baseline_operation(
        method,
        points.unwrap_or_default(),
        zones,
        degree,
        interpolation,
        op,
        keep_baseline,
    )?;
    run(
        py,
        operation,
        Inputs::new(y, x, None, None)?,
        keep_results,
        auto_recalc,
        replace_original,
        PeakOutputs::default(),
    )
}

/// Correction by a reference column: `op` = `"divide"`, `"multiply"`
/// or `"subtract"`. The reference is interpolated on the series' X values;
/// outside it, the cells stay empty.
#[pyfunction]
#[pyo3(signature = (y, reference, x=None, reference_x=None, *, op="divide", keep_results=false, auto_recalc=true, replace_original=false))]
#[allow(clippy::too_many_arguments)]
fn correct(
    py: Python<'_>,
    y: &Bound<'_, PyAny>,
    reference: &Handle,
    x: Option<&Bound<'_, PyAny>>,
    reference_x: Option<&Handle>,
    op: &str,
    keep_results: bool,
    auto_recalc: bool,
    replace_original: bool,
) -> PyResult<Py<PyDict>> {
    let op = choice(
        "op",
        op,
        &[
            ("divide", CorrectOp::Divide),
            ("multiply", CorrectOp::Multiply),
            ("subtract", CorrectOp::Subtract),
        ],
    )?;
    run(
        py,
        Operation::Correct { op },
        Inputs::new(y, x, None, Some((reference, reference_x)))?,
        keep_results,
        auto_recalc,
        replace_original,
        PeakOutputs::default(),
    )
}

/// Smoothing: `method` = `"adjacent"` (adjacent average), `"savitzky-golay"`
/// (`order`, 2 to 5) or `"median"` (moving median). `window` is an odd number
/// of points (`window_unit="points"`, default) or a width in X units
/// (`window_unit="x"`, converted through the median X step).
#[pyfunction]
#[pyo3(signature = (y, x=None, *, method="adjacent", window=5.0, window_unit="points", order=2, keep_results=false, auto_recalc=true, replace_original=false))]
#[allow(clippy::too_many_arguments)]
fn smooth(
    py: Python<'_>,
    y: &Bound<'_, PyAny>,
    x: Option<&Bound<'_, PyAny>>,
    method: &str,
    window: f64,
    window_unit: &str,
    order: usize,
    keep_results: bool,
    auto_recalc: bool,
    replace_original: bool,
) -> PyResult<Py<PyDict>> {
    let method = match choice(
        "method",
        method,
        &[("adjacent", 0), ("savitzky-golay", 1), ("median", 2)],
    )? {
        0 => SmoothMethod::AdjacentAverage,
        1 => SmoothMethod::SavitzkyGolay { order },
        _ => SmoothMethod::Median,
    };
    let window = match choice(
        "window_unit",
        window_unit,
        &[("points", true), ("x", false)],
    )? {
        true if window.is_finite() && window.fract() == 0.0 && window >= 0.0 => {
            SmoothWindow::Points(window as usize)
        }
        true => return Err(invalid("window must be a whole number of points")),
        false => SmoothWindow::XUnits(window),
    };
    run(
        py,
        Operation::Smooth {
            spec: SmoothSpec { method, window },
        },
        Inputs::new(y, x, None, None)?,
        keep_results,
        auto_recalc,
        replace_original,
        PeakOutputs::default(),
    )
}

/// Integral: `ranges=[(from, to), ...]` (empty: the whole curve), `under` =
/// `"zero"` or `"line"`.
#[pyfunction]
#[pyo3(signature = (y, x=None, *, ranges=None, under="zero", cumulative=false, normalize=false, keep_results=false, auto_recalc=true))]
#[allow(clippy::too_many_arguments)]
fn integrate(
    py: Python<'_>,
    y: &Bound<'_, PyAny>,
    x: Option<&Bound<'_, PyAny>>,
    ranges: Option<Vec<(f64, f64)>>,
    under: &str,
    cumulative: bool,
    normalize: bool,
    keep_results: bool,
    auto_recalc: bool,
) -> PyResult<Py<PyDict>> {
    let operation = integrate_operation(ranges, under, cumulative, normalize)?;
    run(
        py,
        operation,
        Inputs::new(y, x, None, None)?,
        keep_results,
        auto_recalc,
        false,
        PeakOutputs::default(),
    )
}

/// Fit: `model` = `"linear"`, `"polynomial"` (with `degree`),
/// `"exponential"` (`A·exp(−x/τ)`), `"exponential_offset"` (with `y0`),
/// `"exponential2"` (two components), `"logarithm"`, `"power"`, `"gaussian"`,
/// `"lorentzian"` or a formula (`model="a*exp(-x/b)+c"`). `initial`, `bounds`
/// (name → `(min, max)`) and `fixed` (names) set the parameters; `sigma`
/// weights.
#[pyfunction]
#[pyo3(signature = (y, x=None, *, model="linear", degree=3, initial=None, bounds=None, fixed=None, shared=None, fit_range=None, residuals=false, sigma=None, keep_results=false, auto_recalc=true))]
#[allow(clippy::too_many_arguments)]
fn fit(
    py: Python<'_>,
    y: &Bound<'_, PyAny>,
    x: Option<&Bound<'_, PyAny>>,
    model: &str,
    degree: u32,
    initial: Option<std::collections::HashMap<String, f64>>,
    bounds: Option<std::collections::HashMap<String, (f64, f64)>>,
    fixed: Option<Vec<String>>,
    shared: Option<Vec<String>>,
    fit_range: Option<(f64, f64)>,
    residuals: bool,
    sigma: Option<&Bound<'_, PyAny>>,
    keep_results: bool,
    auto_recalc: bool,
) -> PyResult<Py<PyDict>> {
    let operation = fit_operation(
        model, degree, initial, bounds, fixed, shared, fit_range, residuals, false,
    )?;
    run(
        py,
        operation,
        Inputs::new(y, x, sigma, None)?,
        keep_results,
        auto_recalc,
        false,
        PeakOutputs::default(),
    )
}

/// Peak decomposition: `centers=[...]` places the peaks, otherwise they are
/// detected (`prominence` as a fraction of the range, `max_peaks`).
/// `center_window` bounds each center to ± this distance from its starting
/// position. `peak_columns` writes one column per peak (`peak.N`) and the
/// background (`background`), `sum_column` the sum of the peaks (`peaks_sum`):
/// the two checkboxes of the window, checked by default.
#[pyfunction]
#[pyo3(signature = (y, x=None, *, shape="gaussian", background="constant", centers=None, link_widths=false, center_window=None, prominence=0.05, max_peaks=6, fit_range=None, residuals=false, peak_columns=true, sum_column=true, keep_results=false, auto_recalc=true))]
#[allow(clippy::too_many_arguments)]
fn peaks(
    py: Python<'_>,
    y: &Bound<'_, PyAny>,
    x: Option<&Bound<'_, PyAny>>,
    shape: &str,
    background: &str,
    centers: Option<Vec<f64>>,
    link_widths: bool,
    center_window: Option<f64>,
    prominence: f64,
    max_peaks: usize,
    fit_range: Option<(f64, f64)>,
    residuals: bool,
    peak_columns: bool,
    sum_column: bool,
    keep_results: bool,
    auto_recalc: bool,
) -> PyResult<Py<PyDict>> {
    let shape = choice(
        "shape",
        shape,
        &[
            ("gaussian", PeakShape::Gaussian),
            ("lorentzian", PeakShape::Lorentzian),
            ("voigt", PeakShape::Voigt),
        ],
    )?;
    let background = choice(
        "background",
        background,
        &[
            ("none", PeakBackground::None),
            ("constant", PeakBackground::Constant),
            ("linear", PeakBackground::Linear),
        ],
    )?;
    if !(prominence > 0.0 && prominence <= 1.0) {
        return Err(invalid("prominence must be in ]0, 1]"));
    }
    let seeds = centers
        .unwrap_or_default()
        .into_iter()
        .map(|center| PeakSeed {
            center,
            height: None,
            fwhm: None,
        })
        .collect();
    let operation = Operation::Peaks {
        spec: PeaksSpec {
            shape,
            background,
            seeds,
            link_widths,
            center_window,
        },
        auto: Some(AutoDetect {
            min_prominence: prominence,
            max_peaks,
        }),
        range: finite_range("fit_range", fit_range)?,
        residuals,
    };
    run(
        py,
        operation,
        Inputs::new(y, x, None, None)?,
        keep_results,
        auto_recalc,
        false,
        PeakOutputs {
            peak_columns,
            sum_column,
        },
    )
}

/// Reconvolution: `components` exponentials (1 to 4) convolved with the
/// instrument response `response`.
#[pyfunction]
#[pyo3(signature = (y, response, x=None, response_x=None, *, components=1, shift=0.0, fit_shift=true, window=None, residuals=false, keep_results=false, auto_recalc=false))]
#[allow(clippy::too_many_arguments)]
fn reconvolve(
    py: Python<'_>,
    y: &Bound<'_, PyAny>,
    response: &Handle,
    x: Option<&Bound<'_, PyAny>>,
    response_x: Option<&Handle>,
    components: usize,
    shift: f64,
    fit_shift: bool,
    window: Option<(f64, f64)>,
    residuals: bool,
    keep_results: bool,
    auto_recalc: bool,
) -> PyResult<Py<PyDict>> {
    if !(1..=4).contains(&components) {
        return Err(invalid("components must be between 1 and 4"));
    }
    let spec = ReconvSpec {
        components,
        params: vec![ParamSetting::default(); 2 + 2 * components],
        shift_initial: shift,
        fit_shift,
        window: finite_range("window", window)?,
    };
    run(
        py,
        Operation::Reconvolution { spec, residuals },
        Inputs::new(y, x, None, Some((response, response_x)))?,
        keep_results,
        auto_recalc,
        false,
        PeakOutputs::default(),
    )
}

/// Direct deconvolution: `method` = `"richardson-lucy"` (`iterations`) or
/// `"wiener"` (`noise`, estimated if absent).
#[pyfunction]
#[pyo3(signature = (y, response, x=None, response_x=None, *, method="richardson-lucy", iterations=50, noise=None, shift=0.0, keep_results=false, auto_recalc=false))]
#[allow(clippy::too_many_arguments)]
fn deconvolve(
    py: Python<'_>,
    y: &Bound<'_, PyAny>,
    response: &Handle,
    x: Option<&Bound<'_, PyAny>>,
    response_x: Option<&Handle>,
    method: &str,
    iterations: usize,
    noise: Option<f64>,
    shift: f64,
    keep_results: bool,
    auto_recalc: bool,
) -> PyResult<Py<PyDict>> {
    let method = match method {
        "richardson-lucy" => DirectMethod::RichardsonLucy { iterations },
        "wiener" => DirectMethod::Wiener { noise },
        other => {
            return Err(invalid(format!(
                "unknown method \"{other}\": use \"richardson-lucy\" or \"wiener\""
            )))
        }
    };
    run(
        py,
        Operation::DirectDeconvolution { method, shift },
        Inputs::new(y, x, None, Some((response, response_x)))?,
        keep_results,
        auto_recalc,
        false,
        PeakOutputs::default(),
    )
}

/// What an analysis is missing before it can be started, **without reading any
/// data** and without writing anything: the same rule as the Apply button of
/// the analysis window (`Operation::readiness`).
///
/// `kind`: `"baseline"`, `"integrate"`, `"fit"` (with the same options as the
/// function of the same name: `method`, `points`, `zones`, `ranges`,
/// `model`..., a `nan` point or zone meaning "not set yet"), or `"correct"`,
/// `"smooth"`, `"peaks"`, `"reconvolve"`, `"deconvolve"` (no options, always
/// ready). Returns a dict `ready` (bool), `reason` (`None`, `"needs_points"`,
/// `"needs_zone"`, `"bad_expression"`, `"no_parameter"` or `"invalid"`) and
/// `message` (English, `None` if ready); `needs_points` adds `needed` and
/// `have`. An unknown or mistyped option raises `InvalidValueError`.
#[pyfunction]
#[pyo3(signature = (kind, **options))]
fn readiness(
    py: Python<'_>,
    kind: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<Py<PyDict>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let mut options = Options::new(options)?;
    let operation = match kind {
        "baseline" => baseline_operation(
            &options.take("method", "anchors".to_string())?,
            options.take("points", Vec::new())?,
            options.take_opt("zones")?,
            options.take("degree", 1)?,
            &options.take("interpolation", "segments".to_string())?,
            &options.take("op", "subtract".to_string())?,
            options.take("keep_baseline", false)?,
        )?,
        "integrate" => integrate_operation(
            options.take_opt("ranges")?,
            &options.take("under", "zero".to_string())?,
            options.take("cumulative", false)?,
            options.take("normalize", false)?,
        )?,
        "fit" => fit_operation(
            &options.take("model", "linear".to_string())?,
            options.take("degree", 3)?,
            options.take_opt("initial")?,
            options.take_opt("bounds")?,
            options.take_opt("fixed")?,
            options.take_opt("shared")?,
            options.take_opt("fit_range")?,
            options.take("residuals", false)?,
            true,
        )?,
        "correct" | "smooth" | "peaks" | "reconvolve" | "deconvolve" => {
            options.expect_empty()?;
            let result = PyDict::new(py);
            result.set_item("ready", true)?;
            result.set_item("reason", py.None())?;
            result.set_item("message", py.None())?;
            return Ok(result.unbind());
        }
        other => {
            return Err(invalid(format!(
                "unknown analysis \"{other}\": use \"baseline\", \"correct\", \"smooth\", \
                 \"integrate\", \"fit\", \"peaks\", \"reconvolve\" or \"deconvolve\""
            )))
        }
    };
    options.expect_empty()?;
    let result = PyDict::new(py);
    let (reason, message) = match operation.readiness() {
        Readiness::Ready => (None, None),
        Readiness::NeedsPoints { needed, have } => {
            result.set_item("needed", needed)?;
            result.set_item("have", have)?;
            (
                Some("needs_points"),
                Some(format!("{needed} point(s) are needed, {have} placed")),
            )
        }
        Readiness::NeedsZone => (
            Some("needs_zone"),
            Some("at least one zone must be placed".to_string()),
        ),
        Readiness::BadExpression(error) => (Some("bad_expression"), Some(error.to_string())),
        Readiness::NoParameter => (
            Some("no_parameter"),
            Some("the model has no parameter to fit".to_string()),
        ),
        Readiness::Invalid(error) => (Some("invalid"), Some(error.to_string())),
    };
    result.set_item("ready", reason.is_none())?;
    result.set_item("reason", reason)?;
    result.set_item("message", message)?;
    Ok(result.unbind())
}

/// The options of `readiness`: taken one by one, then nothing must be left
/// (a typo raises instead of being ignored).
struct Options<'a, 'py> {
    dict: Option<&'a Bound<'py, PyDict>>,
    seen: Vec<String>,
}

impl<'a, 'py> Options<'a, 'py> {
    fn new(dict: Option<&'a Bound<'py, PyDict>>) -> PyResult<Self> {
        Ok(Self {
            dict,
            seen: Vec::new(),
        })
    }

    fn take_opt<T: for<'x> FromPyObject<'x, 'py, Error = PyErr>>(
        &mut self,
        key: &str,
    ) -> PyResult<Option<T>> {
        self.seen.push(key.to_string());
        let Some(value) = self.dict.map(|d| d.get_item(key)).transpose()?.flatten() else {
            return Ok(None);
        };
        if value.is_none() {
            return Ok(None);
        }
        value
            .extract::<T>()
            .map(Some)
            .map_err(|_| invalid(format!("option \"{key}\" has the wrong type")))
    }

    fn take<T: for<'x> FromPyObject<'x, 'py, Error = PyErr>>(
        &mut self,
        key: &str,
        default: T,
    ) -> PyResult<T> {
        Ok(self.take_opt(key)?.unwrap_or(default))
    }

    fn expect_empty(&self) -> PyResult<()> {
        if let Some(dict) = self.dict {
            for key in dict.keys() {
                let key = key.extract::<String>()?;
                if !self.seen.contains(&key) {
                    return Err(invalid(format!("unknown option \"{key}\"")));
                }
            }
        }
        Ok(())
    }
}

/// Documentation of the `pyst.analysis` module.
const MODULE_DOC: &str = "\
StokesForge analysis calculations: one call, one project calculation. y can be a handle or a list
(batch: a single multi-series calculation, result {'series': [...]}), x and sigma a handle or a list.

    pyst.analysis.baseline(y, x=None, *, method='anchors', points=None, zones=None, degree=1,
                           interpolation='segments', op='subtract', keep_baseline=False)
    pyst.analysis.correct(y, reference, x=None, reference_x=None, *, op='divide')
    pyst.analysis.smooth(y, x=None, *, method='adjacent', window=5, window_unit='points', order=2)
    pyst.analysis.integrate(y, x=None, *, ranges=None, under='zero', cumulative=False, normalize=False)
    pyst.analysis.fit(y, x=None, *, model='linear', degree=3, initial=None, bounds=None, fixed=None,
                      shared=None, fit_range=None, residuals=False, sigma=None)
    pyst.analysis.peaks(y, x=None, *, shape='gaussian', background='constant', centers=None,
                        link_widths=False, center_window=None, prominence=0.05, max_peaks=6,
                        fit_range=None, residuals=False, peak_columns=True, sum_column=True)
    pyst.analysis.reconvolve(y, response, x=None, response_x=None, *, components=1, shift=0.0, ...)
    pyst.analysis.deconvolve(y, response, x=None, response_x=None, *, method='richardson-lucy', ...)

    pyst.analysis.readiness(kind, **options)   # what is missing before a calculation, without writing anything

Each function also accepts keep_results=False and auto_recalc=True (False for
reconvolve and deconvolve). It returns a dict: columns (output key -> Handle),
values, errors and warnings.
";

/// Builds `pyst.analysis` and registers it in `sys.modules`.
pub(crate) fn install(py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let module = PyModule::new(py, "pyst.analysis")?;
    module.add("__doc__", MODULE_DOC)?;
    module.add_function(wrap_pyfunction!(baseline, &module)?)?;
    module.add_function(wrap_pyfunction!(correct, &module)?)?;
    module.add_function(wrap_pyfunction!(smooth, &module)?)?;
    module.add_function(wrap_pyfunction!(integrate, &module)?)?;
    module.add_function(wrap_pyfunction!(fit, &module)?)?;
    module.add_function(wrap_pyfunction!(peaks, &module)?)?;
    module.add_function(wrap_pyfunction!(reconvolve, &module)?)?;
    module.add_function(wrap_pyfunction!(deconvolve, &module)?)?;
    module.add_function(wrap_pyfunction!(readiness, &module)?)?;
    parent.add("analysis", &module)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pyst.analysis", &module)?;
    Ok(())
}
