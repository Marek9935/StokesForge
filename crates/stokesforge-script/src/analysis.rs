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
//! # Limites de cette première surface
//!
//! Une série par appel (pas de lot ni d'ajustement global) ; les noms longs
//! des colonnes sont en anglais (la langue de l'interface ne passe pas par les
//! scripts).

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
    compute, AuxRef, CalculationDraft, ColumnMeta, Computed, NodeId, OutputDraft, SeriesRef, Tree,
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

/// Le suffixe anglais du nom long d'une colonne produite.
fn suffix(key: &str, index: Option<usize>) -> String {
    match key {
        "corrected" => "corrected".to_string(),
        "baseline" => "baseline".to_string(),
        "cumulative" => "cumulative".to_string(),
        "normalized" => "normalized".to_string(),
        "fit" => "fit".to_string(),
        "residuals" => "resid.".to_string(),
        "peak" => format!("peak {}", index.unwrap_or(1)),
        "background" => "background".to_string(),
        "peaks_sum" => "sum of peaks".to_string(),
        "deconvolved" => "deconvolved".to_string(),
        other => other.to_string(),
    }
}

fn long_name(tree: &Tree, y: NodeId) -> String {
    let node = tree.node(y);
    let short = node.map(|n| n.name().to_string()).unwrap_or_default();
    node.and_then(|n| n.as_column())
        .map(|c| c.meta().long_name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or(short)
}

fn draft(
    tree: &Tree,
    series: &SeriesRef,
    op: Operation,
    aux: Option<AuxRef>,
    computed: Computed,
    keep_results: bool,
    auto_recalc: bool,
) -> CalculationDraft {
    let name = long_name(tree, series.y);
    let unit = tree
        .node(series.y)
        .and_then(|n| n.as_column())
        .map(|c| c.meta().unit.clone())
        .unwrap_or_default();
    let outputs = computed
        .columns
        .iter()
        .map(|c| OutputDraft {
            series: c.series,
            key: c.key.clone(),
            index: c.index,
            meta: ColumnMeta {
                long_name: format!("{name} ({})", suffix(&c.key, c.index)),
                unit: unit.clone(),
                ..ColumnMeta::default()
            },
            data: c.data.clone(),
        })
        .collect();
    CalculationDraft {
        operation: op,
        series: vec![series.clone()],
        aux,
        outputs,
        results: if keep_results {
            computed.results
        } else {
            Vec::new()
        },
        keep_results,
        auto_recalc,
    }
}

fn host<T>(result: Result<T, impl std::fmt::Display>) -> PyResult<T> {
    result.map_err(|e| invalid(e.to_string()))
}

/// Les poignées d'entrée, validées contre l'instantané.
struct Inputs<'a> {
    y: &'a Handle,
    x: Option<&'a Handle>,
    sigma: Option<&'a Handle>,
    aux: Option<(&'a Handle, Option<&'a Handle>)>,
}

/// Enregistre l'opération et rend le dictionnaire de résultats.
fn run(
    py: Python<'_>,
    op: Operation,
    inputs: Inputs<'_>,
    keep_results: bool,
    auto_recalc: bool,
) -> PyResult<Py<PyDict>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let ids = (
        (inputs.y.id, inputs.y.epoch),
        inputs.x.map(|h| (h.id, h.epoch)),
        inputs.sigma.map(|h| (h.id, h.epoch)),
        inputs
            .aux
            .map(|(y, x)| ((y.id, y.epoch), x.map(|h| (h.id, h.epoch)))),
    );
    let outcome = py
        .detach(|| -> Result<_, HostError> {
            let session = context.session();
            let prepare = |snapshot: &stokesforge_session::Snapshot| {
                for (id, epoch) in [Some(ids.0), ids.1, ids.2, ids.3.map(|a| a.0)]
                    .into_iter()
                    .flatten()
                    .chain(ids.3.and_then(|a| a.1))
                {
                    checked_node(snapshot, id, epoch)?;
                }
                let series = SeriesRef {
                    x: ids.1.map(|h| h.0),
                    y: ids.0 .0,
                    sigma: ids.2.map(|h| h.0),
                };
                let aux = ids.3.map(|(y, x)| AuxRef {
                    x: x.map(|h| h.0),
                    y: y.0,
                });
                let computed = compute(
                    &snapshot.tree,
                    &op,
                    std::slice::from_ref(&series),
                    aux.as_ref(),
                )
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
            let values = computed.results.first().cloned().unwrap_or_default();
            let draft = draft(
                &tree,
                &series,
                op.clone(),
                aux,
                computed,
                keep_results,
                auto_recalc,
            );
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
    let result = PyDict::new(py);
    let columns = PyDict::new(py);
    for out in &calc.outputs {
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
    for v in &values.values {
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
    for w in &values.warnings {
        warnings.append(match w {
            Warning::SignalStepNotUniform => "the X step is not constant".to_string(),
            Warning::IrfStepMismatch => "the response has a different X step".to_string(),
            Warning::NegativeClipped(n) => format!("{n} negative points were set to zero"),
            Warning::Undetermined(n) => {
                format!("{n} parameter(s) are not determined by the data (no uncertainty)")
            }
        })?;
    }
    result.set_item("columns", columns)?;
    result.set_item("values", numbers)?;
    result.set_item("errors", errors)?;
    result.set_item("warnings", warnings)?;
    Ok(result.unbind())
}

/// Ligne de base : `method` = `"anchors"` (avec `points=[(x, y), …]`),
/// `"line"` (deux `points`) ou `"zones"` (avec `zones=[(de, à), …]` et
/// `degree`). `op` : `"subtract"` ou `"divide"`.
#[pyfunction]
#[pyo3(signature = (y, x=None, *, method="anchors", points=None, zones=None, degree=1, interpolation="segments", op="subtract", keep_baseline=false, keep_results=false, auto_recalc=true))]
#[allow(clippy::too_many_arguments)]
fn baseline(
    py: Python<'_>,
    y: &Handle,
    x: Option<&Handle>,
    method: &str,
    points: Option<Vec<(f64, f64)>>,
    zones: Option<Vec<(f64, f64)>>,
    degree: u32,
    interpolation: &str,
    op: &str,
    keep_baseline: bool,
    keep_results: bool,
    auto_recalc: bool,
) -> PyResult<Py<PyDict>> {
    let points = points.unwrap_or_default();
    let method = match method {
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
                "unknown method \"{other}\": use \"anchors\", \"line\" or \"zones\""
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
        Inputs {
            y,
            x,
            sigma: None,
            aux: None,
        },
        keep_results,
        auto_recalc,
    )
}

/// Correction par une colonne de référence : `op` = `"divide"`, `"multiply"`
/// ou `"subtract"`. La référence est interpolée sur les X de la série ; hors
/// d'elle, les cellules restent vides.
#[pyfunction]
#[pyo3(signature = (y, reference, x=None, reference_x=None, *, op="divide", keep_results=false, auto_recalc=true))]
#[allow(clippy::too_many_arguments)]
fn correct(
    py: Python<'_>,
    y: &Handle,
    reference: &Handle,
    x: Option<&Handle>,
    reference_x: Option<&Handle>,
    op: &str,
    keep_results: bool,
    auto_recalc: bool,
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
        Inputs {
            y,
            x,
            sigma: None,
            aux: Some((reference, reference_x)),
        },
        keep_results,
        auto_recalc,
    )
}

/// Intégrale : `ranges=[(de, à), …]` (vide : toute la courbe), `under` =
/// `"zero"` ou `"line"`.
#[pyfunction]
#[pyo3(signature = (y, x=None, *, ranges=None, under="zero", cumulative=false, normalize=false, keep_results=false, auto_recalc=true))]
#[allow(clippy::too_many_arguments)]
fn integrate(
    py: Python<'_>,
    y: &Handle,
    x: Option<&Handle>,
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
        Inputs {
            y,
            x,
            sigma: None,
            aux: None,
        },
        keep_results,
        auto_recalc,
    )
}

/// Ajustement : `model` = `"linear"`, `"polynomial"` (avec `degree`),
/// `"exponential"` (`A·exp(−x/τ)`), `"exponential_offset"` (avec `y0`),
/// `"exponential2"` (deux composantes), `"logarithm"`, `"power"`, `"gaussian"`, `"lorentzian"` ou
/// une formule (`model="a*exp(-x/b)+c"`). `initial`, `bounds` (nom →
/// `(min, max)`) et `fixed` (noms) règlent les paramètres ; `sigma` pondère.
#[pyfunction]
#[pyo3(signature = (y, x=None, *, model="linear", degree=3, initial=None, bounds=None, fixed=None, fit_range=None, residuals=false, sigma=None, keep_results=false, auto_recalc=true))]
#[allow(clippy::too_many_arguments)]
fn fit(
    py: Python<'_>,
    y: &Handle,
    x: Option<&Handle>,
    model: &str,
    degree: u32,
    initial: Option<std::collections::HashMap<String, f64>>,
    bounds: Option<std::collections::HashMap<String, (f64, f64)>>,
    fixed: Option<Vec<String>>,
    fit_range: Option<(f64, f64)>,
    residuals: bool,
    sigma: Option<&Handle>,
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
    let (initial, bounds, fixed) = (
        initial.unwrap_or_default(),
        bounds.unwrap_or_default(),
        fixed.unwrap_or_default(),
    );
    for name in initial.keys().chain(bounds.keys()).chain(fixed.iter()) {
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
            shared: false,
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
        Inputs {
            y,
            x,
            sigma,
            aux: None,
        },
        keep_results,
        auto_recalc,
    )
}

/// Décomposition en pics : `centers=[…]` place les pics, sinon ils sont
/// détectés (`prominence` en fraction de l'étendue, `max_peaks`).
/// `center_window` borne chaque centre à ± cette distance de sa position de
/// départ.
#[pyfunction]
#[pyo3(signature = (y, x=None, *, shape="gaussian", background="constant", centers=None, link_widths=false, center_window=None, prominence=0.05, max_peaks=6, fit_range=None, residuals=false, keep_results=false, auto_recalc=true))]
#[allow(clippy::too_many_arguments)]
fn peaks(
    py: Python<'_>,
    y: &Handle,
    x: Option<&Handle>,
    shape: &str,
    background: &str,
    centers: Option<Vec<f64>>,
    link_widths: bool,
    center_window: Option<f64>,
    prominence: f64,
    max_peaks: usize,
    fit_range: Option<(f64, f64)>,
    residuals: bool,
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
        Inputs {
            y,
            x,
            sigma: None,
            aux: None,
        },
        keep_results,
        auto_recalc,
    )
}

/// Reconvolution : `components` exponentielles (1 à 4) convoluées avec la
/// réponse instrumentale `response`.
#[pyfunction]
#[pyo3(signature = (y, response, x=None, response_x=None, *, components=1, shift=0.0, fit_shift=true, window=None, residuals=false, keep_results=false, auto_recalc=false))]
#[allow(clippy::too_many_arguments)]
fn reconvolve(
    py: Python<'_>,
    y: &Handle,
    response: &Handle,
    x: Option<&Handle>,
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
        Inputs {
            y,
            x,
            sigma: None,
            aux: Some((response, response_x)),
        },
        keep_results,
        auto_recalc,
    )
}

/// Déconvolution directe : `method` = `"richardson-lucy"` (`iterations`) ou
/// `"wiener"` (`noise`, estimé si absent).
#[pyfunction]
#[pyo3(signature = (y, response, x=None, response_x=None, *, method="richardson-lucy", iterations=50, noise=None, shift=0.0, keep_results=false, auto_recalc=false))]
#[allow(clippy::too_many_arguments)]
fn deconvolve(
    py: Python<'_>,
    y: &Handle,
    response: &Handle,
    x: Option<&Handle>,
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
        Inputs {
            y,
            x,
            sigma: None,
            aux: Some((response, response_x)),
        },
        keep_results,
        auto_recalc,
    )
}

/// Documentation du module `pyst.analysis`.
const MODULE_DOC: &str = "\
Calculs d'analyse de StokesForge : une série par appel, enregistrée comme un calcul du projet.

    pyst.analysis.baseline(y, x=None, *, method='anchors', points=None, zones=None, degree=1,
                           interpolation='segments', op='subtract', keep_baseline=False)
    pyst.analysis.correct(y, reference, x=None, reference_x=None, *, op='divide')
    pyst.analysis.integrate(y, x=None, *, ranges=None, under='zero', cumulative=False, normalize=False)
    pyst.analysis.fit(y, x=None, *, model='linear', degree=3, initial=None, bounds=None, fixed=None,
                      fit_range=None, residuals=False, sigma=None)
    pyst.analysis.peaks(y, x=None, *, shape='gaussian', background='constant', centers=None,
                        link_widths=False, prominence=0.05, max_peaks=6, fit_range=None, residuals=False)
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
