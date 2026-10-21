//! `pyst.analysis` (#181) : les calculs du menu Analysis, depuis un script.
//!
//! Chaque fonction décrit **une opération** de `stokesforge-analysis` sur une
//! série (une colonne Y, son X facultatif) et l'enregistre comme un calcul du
//! projet — les mêmes colonnes de sortie, la même trace, le même recalcul
//! automatique que depuis la fenêtre d'analyse. L'écriture est une
//! `Command::AddCalculation` du groupe d'écritures du script : un seul Ctrl+Z
//! (ou `pyst.undo()`) annule le calcul avec ses colonnes.
//!
//! # Ce qui est rendu
//!
//! Un dictionnaire : `columns` (clé de sortie → poignée de la colonne écrite ;
//! `"peak.2"` pour le deuxième pic), `values` (`"area.total"`, `"param.tau"`,
//! `"peak.height.1"`…), `errors` (erreur type des valeurs qui en ont une) et
//! `warnings` (phrases anglaises). Les clés sont celles du crate de calcul.
//!
//! # Lots (#201)
//!
//! `y` accepte une poignée **ou une liste** de poignées : le lot est un seul
//! calcul multi-séries, comme dans la fenêtre d'analyse, annulé d'un seul
//! Ctrl+Z. `x` et `sigma` valent pour toutes les séries (une poignée) ou une
//! par série (une liste de même longueur). Le résultat d'un lot est
//! `{"series": [dict, …]}`, un dictionnaire par série dans l'ordre de `y` ;
//! `fit(shared=[noms])` en fait un ajustement global.
//!
//! # Noms des colonnes écrites
//!
//! Nom long, unité et commentaire sont ceux de la fenêtre d'analyse, par la
//! règle unique de `stokesforge-core` (`OutputName`, #256) : « Intensity
//! (B-corr) », « Baseline of B »… L'unité est celle de la source, le
//! commentaire le titre de la famille. Seule différence : le texte est toujours
//! en anglais (la langue de l'interface ne passe pas par les scripts). Le nom
//! est figé à la création ; le recalcul n'en réécrit que les données.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyModule};
use pyo3::wrap_pyfunction;
use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp, Interpolation};
use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::deconv::{DirectMethod, ReconvSpec};
use stokesforge_analysis::fit::{FitSpec, ParamSetting};
use stokesforge_analysis::integrate::{IntegrateSpec, Under};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{AutoDetect, Operation, Warning};
use stokesforge_analysis::peaks::{PeakBackground, PeakSeed, PeakShape, PeaksSpec};
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

/// Le lot à enregistrer. Le nom, l'unité et le commentaire de chaque colonne
/// sont la règle du cœur (`OutputName`, `output_unit`, `family_english`) : les
/// mêmes que ceux de la fenêtre d'analyse, en anglais ; `wants_output` choisit
/// les colonnes écrites.
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

/// Une poignée lue d'un argument, avant validation contre l'instantané.
type Raw = (NodeId, u64);

/// Les poignées d'entrée : une ou plusieurs séries (un **lot**, un seul calcul
/// multi-séries comme dans la fenêtre d'analyse, #201).
struct Inputs {
    /// `y` était une liste : le résultat est rendu série par série.
    batch: bool,
    y: Vec<Raw>,
    /// Un X par série (le même répété quand un seul est donné).
    x: Vec<Option<Raw>>,
    sigma: Vec<Option<Raw>>,
    aux: Option<(Raw, Option<Raw>)>,
}

fn raw(handle: &Handle) -> Raw {
    (handle.id, handle.epoch)
}

/// `Handle` ou liste de `Handle`.
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
    /// Rassemble les arguments `y`, `x`, `sigma` (chacun une poignée ou une
    /// liste) : une poignée seule pour `x` ou `sigma` vaut pour toutes les séries.
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

/// Le dictionnaire d'une série : `columns`, `values`, `errors`, `warnings`.
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

/// Enregistre l'opération et rend le dictionnaire de résultats : à plat pour
/// une série, `{"series": [dict, …]}` pour un lot.
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
            // Les calculs d'avant : le nôtre est celui qui s'y ajoute, même si un
            // autre client en crée un au même moment.
            let mut known: std::collections::BTreeSet<NodeId> =
                snapshot.tree.calculations().keys().copied().collect();
            if context.begin_write()? {
                // Le groupe vient de s'ouvrir : relire pour valider l'époque.
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
                            key: "corrected".into(),
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

/// Ligne de base : `method` = `"anchors"` (avec `points=[(x, y), …]`),
/// `"constant"` (un point ou une zone), `"line"` (deux `points`) ou
/// `"zones"` (avec `zones=[(de, à), …]` et
/// `degree`). `op` : `"subtract"` ou `"divide"`.
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
    let points = points.unwrap_or_default();
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

/// Correction par une colonne de référence : `op` = `"divide"`, `"multiply"`
/// ou `"subtract"`. La référence est interpolée sur les X de la série ; hors
/// d'elle, les cellules restent vides.
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

/// Intégrale : `ranges=[(de, à), …]` (vide : toute la courbe), `under` =
/// `"zero"` ou `"line"`.
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
    run(
        py,
        Operation::Integrate { spec },
        Inputs::new(y, x, None, None)?,
        keep_results,
        auto_recalc,
        false,
        PeakOutputs::default(),
    )
}

/// Ajustement : `model` = `"linear"`, `"polynomial"` (avec `degree`),
/// `"exponential"` (`A·exp(−x/τ)`), `"exponential_offset"` (avec `y0`),
/// `"exponential2"` (deux composantes), `"logarithm"`, `"power"`, `"gaussian"`, `"lorentzian"` ou
/// une formule (`model="a*exp(-x/b)+c"`). `initial`, `bounds` (nom →
/// `(min, max)`) et `fixed` (noms) règlent les paramètres ; `sigma` pondère.
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
    let compiled = host(model.compile())?;
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
        range: finite_range("fit_range", fit_range)?,
        residuals,
    };
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

/// Décomposition en pics : `centers=[…]` place les pics, sinon ils sont
/// détectés (`prominence` en fraction de l'étendue, `max_peaks`).
/// `center_window` borne chaque centre à ± cette distance de sa position de
/// départ. `peak_columns` écrit une colonne par pic (`peak.N`) et le fond
/// (`background`), `sum_column` la somme des pics (`peaks_sum`) : les deux cases
/// de la fenêtre, cochées par défaut.
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

/// Reconvolution : `components` exponentielles (1 à 4) convoluées avec la
/// réponse instrumentale `response`.
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

/// Déconvolution directe : `method` = `"richardson-lucy"` (`iterations`) ou
/// `"wiener"` (`noise`, estimé si absent).
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

/// Documentation du module `pyst.analysis`.
const MODULE_DOC: &str = "\
Calculs d'analyse de StokesForge : un appel, un calcul du projet. y peut être une poignée ou une liste
(lot : un seul calcul multi-séries, résultat {'series': [...]}), x et sigma une poignée ou une liste.

    pyst.analysis.baseline(y, x=None, *, method='anchors', points=None, zones=None, degree=1,
                           interpolation='segments', op='subtract', keep_baseline=False)
    pyst.analysis.correct(y, reference, x=None, reference_x=None, *, op='divide')
    pyst.analysis.integrate(y, x=None, *, ranges=None, under='zero', cumulative=False, normalize=False)
    pyst.analysis.fit(y, x=None, *, model='linear', degree=3, initial=None, bounds=None, fixed=None,
                      shared=None, fit_range=None, residuals=False, sigma=None)
    pyst.analysis.peaks(y, x=None, *, shape='gaussian', background='constant', centers=None,
                        link_widths=False, center_window=None, prominence=0.05, max_peaks=6,
                        fit_range=None, residuals=False, peak_columns=True, sum_column=True)
    pyst.analysis.reconvolve(y, response, x=None, response_x=None, *, components=1, shift=0.0, ...)
    pyst.analysis.deconvolve(y, response, x=None, response_x=None, *, method='richardson-lucy', ...)

Chaque fonction accepte aussi keep_results=False et auto_recalc=True (False pour
reconvolve et deconvolve). Elle rend un dict : columns (clé de sortie -> Handle),
values, errors et warnings.
";

/// Construit `pyst.analysis` et l'enregistre dans `sys.modules`.
pub(crate) fn install(py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let module = PyModule::new(py, "pyst.analysis")?;
    module.add("__doc__", MODULE_DOC)?;
    module.add_function(wrap_pyfunction!(baseline, &module)?)?;
    module.add_function(wrap_pyfunction!(correct, &module)?)?;
    module.add_function(wrap_pyfunction!(integrate, &module)?)?;
    module.add_function(wrap_pyfunction!(fit, &module)?)?;
    module.add_function(wrap_pyfunction!(peaks, &module)?)?;
    module.add_function(wrap_pyfunction!(reconvolve, &module)?)?;
    module.add_function(wrap_pyfunction!(deconvolve, &module)?)?;
    parent.add("analysis", &module)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pyst.analysis", &module)?;
    Ok(())
}
