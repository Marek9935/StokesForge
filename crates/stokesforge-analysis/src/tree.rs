//! **Tree** representation of an [`Operation`]: the form in which
//! the trace of a calculation crosses the model and the `.stf` file.
//!
//! Why a generic tree rather than transport types or `serde`:
//! this crate stays dependency-free (so without `serde`), and persistence, which
//! already has `serde_json`, converts a [`Tree`] to JSON in about thirty
//! lines. A new operation (or a new setting) therefore does **not** change
//! the file schema: an absent **optional** field (booleans, lists,
//! ranges) takes its default value on read, an unknown field is
//! ignored; an **essential** field (`op`, `under`, `shift_initial`,
//! `center`…) is mandatory and its absence drops the calculation (invariant 10:
//! a data defect degrades, it does not crash).

use crate::baseline::{BaselineMethod, BaselineOp, Interpolation};
use crate::correct::CorrectOp;
use crate::deconv::{DirectMethod, ReconvSpec};
use crate::error::AnalysisError;
use crate::fit::{FitSpec, ParamSetting};
use crate::integrate::{IntegrateSpec, Under};
use crate::model::Model;
use crate::ops::{AutoDetect, Operation};
use crate::peaks::{PeakBackground, PeakSeed, PeakShape, PeaksSpec};
use crate::smooth::{SmoothMethod, SmoothSpec, SmoothWindow};

#[derive(Debug, Clone, PartialEq)]
pub enum Tree {
    Null,
    Bool(bool),
    Num(f64),
    Text(String),
    List(Vec<Tree>),
    Map(Vec<(String, Tree)>),
}

// ── Writing ────────────────────────────────────────────────────────────

#[derive(Default)]
struct W(Vec<(String, Tree)>);

impl W {
    fn kind(kind: &str) -> W {
        let mut w = W::default();
        w.0.push(("kind".into(), Tree::Text(kind.into())));
        w
    }
    fn put(mut self, k: &str, t: Tree) -> W {
        self.0.push((k.into(), t));
        self
    }
    fn num(self, k: &str, v: f64) -> W {
        self.put(k, num(v))
    }
    fn opt(self, k: &str, v: Option<f64>) -> W {
        self.put(k, v.map_or(Tree::Null, num))
    }
    fn text(self, k: &str, v: &str) -> W {
        self.put(k, Tree::Text(v.into()))
    }
    fn bool(self, k: &str, v: bool) -> W {
        self.put(k, Tree::Bool(v))
    }
    fn done(self) -> Tree {
        Tree::Map(self.0)
    }
}

/// A non-finite number cannot be represented in JSON: it becomes "absent".
fn num(v: f64) -> Tree {
    if v.is_finite() {
        Tree::Num(v)
    } else {
        Tree::Null
    }
}

fn pair(a: f64, b: f64) -> Tree {
    Tree::List(vec![num(a), num(b)])
}

fn range_tree(r: Option<(f64, f64)>) -> Tree {
    r.map_or(Tree::Null, |(a, b)| pair(a, b))
}

fn model_tree(m: &Model) -> Tree {
    match m {
        Model::Linear => W::kind("linear").done(),
        Model::Polynomial { degree } => W::kind("polynomial").num("degree", *degree as f64).done(),
        Model::Exponential => W::kind("exponential").done(),
        Model::ExponentialOffset => W::kind("exponential_offset").done(),
        Model::Exponential2 => W::kind("exponential2").done(),
        Model::Logarithm => W::kind("logarithm").done(),
        Model::Power => W::kind("power").done(),
        Model::Gaussian => W::kind("gaussian").done(),
        Model::Lorentzian => W::kind("lorentzian").done(),
        Model::Custom { formula } => W::kind("custom").text("formula", formula).done(),
    }
}

fn settings_tree(p: &[ParamSetting]) -> Tree {
    Tree::List(
        p.iter()
            .map(|s| {
                W::default()
                    .opt("initial", s.initial)
                    .bool("fixed", s.fixed)
                    .opt("min", s.min)
                    .opt("max", s.max)
                    .bool("shared", s.shared)
                    .done()
            })
            .collect(),
    )
}

impl Operation {
    /// The operation as a tree; [`Operation::from_tree`] is its exact
    /// inverse (up to non-finite numbers, written "absent").
    pub fn to_tree(&self) -> Tree {
        match self {
            Operation::Baseline {
                method,
                op,
                keep_baseline,
            } => {
                let m = match method {
                    BaselineMethod::Constant { point } => W::kind("constant")
                        .put("point", pair(point.0, point.1))
                        .done(),
                    BaselineMethod::ConstantZone { zone } => W::kind("constant-zone")
                        .put("zone", pair(zone.0, zone.1))
                        .done(),
                    BaselineMethod::Line { p1, p2 } => W::kind("line")
                        .put("p1", pair(p1.0, p1.1))
                        .put("p2", pair(p2.0, p2.1))
                        .done(),
                    BaselineMethod::Zones { zones, degree } => W::kind("zones")
                        .put(
                            "zones",
                            Tree::List(zones.iter().map(|z| pair(z.0, z.1)).collect()),
                        )
                        .num("degree", *degree as f64)
                        .done(),
                    BaselineMethod::Anchors {
                        points,
                        interpolation,
                    } => W::kind("anchors")
                        .put(
                            "points",
                            Tree::List(points.iter().map(|p| pair(p.0, p.1)).collect()),
                        )
                        .text(
                            "interpolation",
                            match interpolation {
                                Interpolation::Segments => "segments",
                                Interpolation::Spline => "spline",
                            },
                        )
                        .done(),
                };
                W::kind("baseline")
                    .put("method", m)
                    .text(
                        "op",
                        match op {
                            BaselineOp::Subtract => "subtract",
                            BaselineOp::Divide => "divide",
                        },
                    )
                    .bool("keep_baseline", *keep_baseline)
                    .done()
            }
            Operation::Correct { op } => W::kind("correct")
                .text(
                    "op",
                    match op {
                        CorrectOp::Divide => "divide",
                        CorrectOp::Multiply => "multiply",
                        CorrectOp::Subtract => "subtract",
                    },
                )
                .done(),
            Operation::Integrate { spec } => W::kind("integrate")
                .put(
                    "ranges",
                    Tree::List(spec.ranges.iter().map(|r| pair(r.0, r.1)).collect()),
                )
                .text(
                    "under",
                    match spec.under {
                        Under::Zero => "zero",
                        Under::Line => "line",
                    },
                )
                .bool("cumulative", spec.cumulative)
                .bool("normalize", spec.normalize)
                .done(),
            Operation::Fit {
                spec,
                range,
                residuals,
            } => W::kind("fit")
                .put("model", model_tree(&spec.model))
                .put("params", settings_tree(&spec.params))
                .put("range", range_tree(*range))
                .bool("residuals", *residuals)
                .done(),
            Operation::Peaks {
                spec,
                auto,
                range,
                residuals,
            } => W::kind("peaks")
                .text(
                    "shape",
                    match spec.shape {
                        PeakShape::Gaussian => "gaussian",
                        PeakShape::Lorentzian => "lorentzian",
                        PeakShape::Voigt => "voigt",
                    },
                )
                .text(
                    "background",
                    match spec.background {
                        PeakBackground::None => "none",
                        PeakBackground::Constant => "constant",
                        PeakBackground::Linear => "linear",
                    },
                )
                .put(
                    "seeds",
                    Tree::List(
                        spec.seeds
                            .iter()
                            .map(|s| {
                                W::default()
                                    .num("center", s.center)
                                    .opt("height", s.height)
                                    .opt("fwhm", s.fwhm)
                                    .done()
                            })
                            .collect(),
                    ),
                )
                .bool("link_widths", spec.link_widths)
                .opt("center_window", spec.center_window)
                .put(
                    "auto",
                    auto.map_or(Tree::Null, |a| {
                        W::default()
                            .num("min_prominence", a.min_prominence)
                            .num("max_peaks", a.max_peaks as f64)
                            .done()
                    }),
                )
                .put("range", range_tree(*range))
                .bool("residuals", *residuals)
                .done(),
            Operation::Reconvolution { spec, residuals } => W::kind("reconvolution")
                .num("components", spec.components as f64)
                .put("params", settings_tree(&spec.params))
                .num("shift_initial", spec.shift_initial)
                .bool("fit_shift", spec.fit_shift)
                .put("window", range_tree(spec.window))
                .bool("residuals", *residuals)
                .done(),
            Operation::DirectDeconvolution { method, shift } => {
                let m = match method {
                    DirectMethod::RichardsonLucy { iterations } => W::kind("richardson_lucy")
                        .num("iterations", *iterations as f64)
                        .done(),
                    DirectMethod::Wiener { noise } => W::kind("wiener").opt("noise", *noise).done(),
                };
                W::kind("direct_deconvolution")
                    .put("method", m)
                    .num("shift", *shift)
                    .done()
            }
            Operation::Smooth { spec } => {
                let (unit, width) = match spec.window {
                    SmoothWindow::Points(n) => ("points", n as f64),
                    SmoothWindow::XUnits(w) => ("x", w),
                };
                let w = W::kind("smooth")
                    .text("window_unit", unit)
                    .num("window", width);
                match spec.method {
                    SmoothMethod::AdjacentAverage => w.text("method", "adjacent_average"),
                    SmoothMethod::SavitzkyGolay { order } => w
                        .text("method", "savitzky_golay")
                        .num("order", order as f64),
                    SmoothMethod::Median => w.text("method", "median"),
                }
                .done()
            }
        }
    }
}

// ── Reading ────────────────────────────────────────────────────────────

fn bad(name: &str) -> AnalysisError {
    AnalysisError::InvalidParameter { name: name.into() }
}

struct R<'a>(&'a [(String, Tree)]);

impl<'a> R<'a> {
    fn of(t: &'a Tree, name: &str) -> Result<R<'a>, AnalysisError> {
        match t {
            Tree::Map(m) => Ok(R(m)),
            _ => Err(bad(name)),
        }
    }
    fn get(&self, k: &str) -> Option<&'a Tree> {
        self.0.iter().find(|(n, _)| n == k).map(|(_, t)| t)
    }
    fn req(&self, k: &str) -> Result<&'a Tree, AnalysisError> {
        match self.get(k) {
            None | Some(Tree::Null) => Err(bad(k)),
            Some(t) => Ok(t),
        }
    }
    fn num(&self, k: &str) -> Result<f64, AnalysisError> {
        match self.req(k)? {
            Tree::Num(v) => Ok(*v),
            _ => Err(bad(k)),
        }
    }
    fn opt_num(&self, k: &str) -> Result<Option<f64>, AnalysisError> {
        match self.get(k) {
            None | Some(Tree::Null) => Ok(None),
            Some(Tree::Num(v)) => Ok(Some(*v)),
            _ => Err(bad(k)),
        }
    }
    fn usize(&self, k: &str) -> Result<usize, AnalysisError> {
        let v = self.num(k)?;
        if v >= 0.0 && v.fract() == 0.0 && v < 1e9 {
            Ok(v as usize)
        } else {
            Err(bad(k))
        }
    }
    /// An absent boolean is `default`.
    fn bool(&self, k: &str, default: bool) -> Result<bool, AnalysisError> {
        match self.get(k) {
            None | Some(Tree::Null) => Ok(default),
            Some(Tree::Bool(b)) => Ok(*b),
            _ => Err(bad(k)),
        }
    }
    fn text(&self, k: &str) -> Result<&'a str, AnalysisError> {
        match self.req(k)? {
            Tree::Text(s) => Ok(s),
            _ => Err(bad(k)),
        }
    }
    fn kind(&self) -> Result<&'a str, AnalysisError> {
        self.text("kind")
    }
    fn list(&self, k: &str) -> Result<&'a [Tree], AnalysisError> {
        match self.get(k) {
            None | Some(Tree::Null) => Ok(&[]),
            Some(Tree::List(l)) => Ok(l),
            _ => Err(bad(k)),
        }
    }
    fn pair(&self, k: &str) -> Result<(f64, f64), AnalysisError> {
        as_pair(self.req(k)?, k)
    }
    fn opt_pair(&self, k: &str) -> Result<Option<(f64, f64)>, AnalysisError> {
        match self.get(k) {
            None | Some(Tree::Null) => Ok(None),
            Some(t) => as_pair(t, k).map(Some),
        }
    }
}

fn as_pair(t: &Tree, name: &str) -> Result<(f64, f64), AnalysisError> {
    match t {
        Tree::List(l) if l.len() == 2 => match (&l[0], &l[1]) {
            (Tree::Num(a), Tree::Num(b)) => Ok((*a, *b)),
            _ => Err(bad(name)),
        },
        _ => Err(bad(name)),
    }
}

fn pairs(r: &R<'_>, k: &str) -> Result<Vec<(f64, f64)>, AnalysisError> {
    r.list(k)?.iter().map(|t| as_pair(t, k)).collect()
}

fn model_of(t: &Tree) -> Result<Model, AnalysisError> {
    let r = R::of(t, "model")?;
    Ok(match r.kind()? {
        "linear" => Model::Linear,
        "polynomial" => Model::Polynomial {
            degree: r.usize("degree")? as u32,
        },
        "exponential" => Model::Exponential,
        "exponential_offset" => Model::ExponentialOffset,
        "exponential2" => Model::Exponential2,
        "logarithm" => Model::Logarithm,
        "power" => Model::Power,
        "gaussian" => Model::Gaussian,
        "lorentzian" => Model::Lorentzian,
        "custom" => Model::Custom {
            formula: r.text("formula")?.to_string(),
        },
        _ => return Err(bad("model")),
    })
}

fn settings_of(r: &R<'_>) -> Result<Vec<ParamSetting>, AnalysisError> {
    r.list("params")?
        .iter()
        .map(|t| {
            let s = R::of(t, "params")?;
            Ok(ParamSetting {
                initial: s.opt_num("initial")?,
                fixed: s.bool("fixed", false)?,
                min: s.opt_num("min")?,
                max: s.opt_num("max")?,
                shared: s.bool("shared", false)?,
            })
        })
        .collect()
}

impl Operation {
    pub fn from_tree(t: &Tree) -> Result<Operation, AnalysisError> {
        let r = R::of(t, "operation")?;
        Ok(match r.kind()? {
            "baseline" => {
                let m = R::of(r.req("method")?, "method")?;
                let method = match m.kind()? {
                    "constant" => BaselineMethod::Constant {
                        point: m.pair("point")?,
                    },
                    "constant-zone" => BaselineMethod::ConstantZone {
                        zone: m.pair("zone")?,
                    },
                    "line" => BaselineMethod::Line {
                        p1: m.pair("p1")?,
                        p2: m.pair("p2")?,
                    },
                    "zones" => BaselineMethod::Zones {
                        zones: pairs(&m, "zones")?,
                        degree: m.usize("degree")? as u32,
                    },
                    "anchors" => BaselineMethod::Anchors {
                        points: pairs(&m, "points")?,
                        interpolation: match m.text("interpolation")? {
                            "segments" => Interpolation::Segments,
                            "spline" => Interpolation::Spline,
                            _ => return Err(bad("interpolation")),
                        },
                    },
                    _ => return Err(bad("method")),
                };
                Operation::Baseline {
                    method,
                    op: match r.text("op")? {
                        "subtract" => BaselineOp::Subtract,
                        "divide" => BaselineOp::Divide,
                        _ => return Err(bad("op")),
                    },
                    keep_baseline: r.bool("keep_baseline", false)?,
                }
            }
            "correct" => Operation::Correct {
                op: match r.text("op")? {
                    "divide" => CorrectOp::Divide,
                    "multiply" => CorrectOp::Multiply,
                    "subtract" => CorrectOp::Subtract,
                    _ => return Err(bad("op")),
                },
            },
            "integrate" => Operation::Integrate {
                spec: IntegrateSpec {
                    ranges: pairs(&r, "ranges")?,
                    under: match r.text("under")? {
                        "zero" => Under::Zero,
                        "line" => Under::Line,
                        _ => return Err(bad("under")),
                    },
                    cumulative: r.bool("cumulative", false)?,
                    normalize: r.bool("normalize", false)?,
                },
            },
            "fit" => Operation::Fit {
                spec: FitSpec {
                    model: model_of(r.req("model")?)?,
                    params: settings_of(&r)?,
                },
                range: r.opt_pair("range")?,
                residuals: r.bool("residuals", false)?,
            },
            "peaks" => {
                let seeds = r
                    .list("seeds")?
                    .iter()
                    .map(|t| {
                        let s = R::of(t, "seeds")?;
                        Ok(PeakSeed {
                            center: s.num("center")?,
                            height: s.opt_num("height")?,
                            fwhm: s.opt_num("fwhm")?,
                        })
                    })
                    .collect::<Result<Vec<_>, AnalysisError>>()?;
                let auto = match r.get("auto") {
                    None | Some(Tree::Null) => None,
                    Some(t) => {
                        let a = R::of(t, "auto")?;
                        Some(AutoDetect {
                            min_prominence: a.num("min_prominence")?,
                            max_peaks: a.usize("max_peaks")?,
                        })
                    }
                };
                Operation::Peaks {
                    spec: PeaksSpec {
                        shape: match r.text("shape")? {
                            "gaussian" => PeakShape::Gaussian,
                            "lorentzian" => PeakShape::Lorentzian,
                            "voigt" => PeakShape::Voigt,
                            _ => return Err(bad("shape")),
                        },
                        background: match r.text("background")? {
                            "none" => PeakBackground::None,
                            "constant" => PeakBackground::Constant,
                            "linear" => PeakBackground::Linear,
                            _ => return Err(bad("background")),
                        },
                        seeds,
                        link_widths: r.bool("link_widths", false)?,
                        center_window: r.opt_num("center_window")?,
                    },
                    auto,
                    range: r.opt_pair("range")?,
                    residuals: r.bool("residuals", false)?,
                }
            }
            "reconvolution" => Operation::Reconvolution {
                spec: ReconvSpec {
                    components: r.usize("components")?,
                    params: settings_of(&r)?,
                    shift_initial: r.num("shift_initial")?,
                    fit_shift: r.bool("fit_shift", false)?,
                    window: r.opt_pair("window")?,
                },
                residuals: r.bool("residuals", false)?,
            },
            "direct_deconvolution" => {
                let m = R::of(r.req("method")?, "method")?;
                Operation::DirectDeconvolution {
                    method: match m.kind()? {
                        "richardson_lucy" => DirectMethod::RichardsonLucy {
                            iterations: m.usize("iterations")?,
                        },
                        "wiener" => DirectMethod::Wiener {
                            noise: m.opt_num("noise")?,
                        },
                        _ => return Err(bad("method")),
                    },
                    shift: r.num("shift")?,
                }
            }
            "smooth" => {
                let width = r.num("window")?;
                let window = match r.text("window_unit")? {
                    "points" => SmoothWindow::Points(r.usize("window")?),
                    "x" => SmoothWindow::XUnits(width),
                    _ => return Err(bad("window_unit")),
                };
                let method = match r.text("method")? {
                    "adjacent_average" => SmoothMethod::AdjacentAverage,
                    "savitzky_golay" => SmoothMethod::SavitzkyGolay {
                        order: r.usize("order")?,
                    },
                    "median" => SmoothMethod::Median,
                    _ => return Err(bad("method")),
                };
                Operation::Smooth {
                    spec: SmoothSpec { method, window },
                }
            }
            _ => return Err(bad("kind")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_operations() -> Vec<Operation> {
        let settings = vec![
            ParamSetting {
                initial: Some(1.5),
                fixed: true,
                min: Some(0.0),
                max: None,
                shared: false,
            },
            ParamSetting {
                shared: true,
                ..Default::default()
            },
        ];
        let mut v = vec![
            Operation::Baseline {
                method: BaselineMethod::Constant { point: (5.0, 2.5) },
                op: BaselineOp::Subtract,
                keep_baseline: true,
            },
            Operation::Baseline {
                method: BaselineMethod::ConstantZone { zone: (3.0, 7.0) },
                op: BaselineOp::Divide,
                keep_baseline: false,
            },
            Operation::Baseline {
                method: BaselineMethod::Line {
                    p1: (1.0, 2.0),
                    p2: (3.0, 4.5),
                },
                op: BaselineOp::Subtract,
                keep_baseline: true,
            },
            Operation::Baseline {
                method: BaselineMethod::Zones {
                    zones: vec![(0.0, 1.0), (5.0, 6.0)],
                    degree: 2,
                },
                op: BaselineOp::Divide,
                keep_baseline: false,
            },
            Operation::Baseline {
                method: BaselineMethod::Anchors {
                    points: vec![(0.0, 1.0), (2.0, 3.0)],
                    interpolation: Interpolation::Spline,
                },
                op: BaselineOp::Subtract,
                keep_baseline: false,
            },
            Operation::Baseline {
                method: BaselineMethod::Anchors {
                    points: vec![(0.0, 1.0), (2.0, 3.0)],
                    interpolation: Interpolation::Segments,
                },
                op: BaselineOp::Subtract,
                keep_baseline: false,
            },
            Operation::Integrate {
                spec: IntegrateSpec {
                    ranges: vec![(1.0, 2.0)],
                    under: Under::Line,
                    cumulative: true,
                    normalize: false,
                },
            },
            Operation::Integrate {
                spec: IntegrateSpec {
                    ranges: vec![],
                    under: Under::Zero,
                    cumulative: false,
                    normalize: true,
                },
            },
            Operation::Peaks {
                spec: PeaksSpec {
                    shape: PeakShape::Voigt,
                    background: PeakBackground::Linear,
                    seeds: vec![
                        PeakSeed {
                            center: 1.0,
                            height: Some(2.0),
                            fwhm: None,
                        },
                        PeakSeed {
                            center: 4.0,
                            height: None,
                            fwhm: Some(0.5),
                        },
                    ],
                    link_widths: true,
                    center_window: Some(0.25),
                },
                auto: Some(AutoDetect {
                    min_prominence: 0.1,
                    max_peaks: 4,
                }),
                range: Some((0.0, 10.0)),
                residuals: true,
            },
            Operation::Reconvolution {
                spec: ReconvSpec {
                    components: 2,
                    params: settings.clone(),
                    shift_initial: -0.3,
                    fit_shift: true,
                    window: Some((1.0, 9.0)),
                },
                residuals: true,
            },
            Operation::DirectDeconvolution {
                method: DirectMethod::RichardsonLucy { iterations: 30 },
                shift: 0.1,
            },
            Operation::DirectDeconvolution {
                method: DirectMethod::Wiener { noise: Some(1e-3) },
                shift: 0.0,
            },
            Operation::DirectDeconvolution {
                method: DirectMethod::Wiener { noise: None },
                shift: 0.0,
            },
        ];
        for c in [CorrectOp::Divide, CorrectOp::Multiply, CorrectOp::Subtract] {
            v.push(Operation::Correct { op: c });
        }
        for (method, window) in [
            (SmoothMethod::AdjacentAverage, SmoothWindow::Points(5)),
            (
                SmoothMethod::SavitzkyGolay { order: 3 },
                SmoothWindow::Points(11),
            ),
            (SmoothMethod::Median, SmoothWindow::XUnits(2.5)),
        ] {
            v.push(Operation::Smooth {
                spec: SmoothSpec { method, window },
            });
        }
        for m in [
            Model::Linear,
            Model::Polynomial { degree: 4 },
            Model::Exponential,
            Model::ExponentialOffset,
            Model::Exponential2,
            Model::Logarithm,
            Model::Power,
            Model::Gaussian,
            Model::Lorentzian,
            Model::Custom {
                formula: "a*exp(-x/b)".into(),
            },
        ] {
            v.push(Operation::Fit {
                spec: FitSpec {
                    model: m,
                    params: settings.clone(),
                },
                range: Some((1.0, 2.0)),
                residuals: true,
            });
        }
        v.push(Operation::Fit {
            spec: FitSpec {
                model: Model::Linear,
                params: vec![],
            },
            range: None,
            residuals: false,
        });
        v
    }

    #[test]
    fn the_text_of_a_free_model_with_logarithms_is_kept_as_is() {
        for formula in [
            "a*log(x)+b",
            "a * ln( x ) + b",
            "logx(x, 3) - log10(x)/log2(x)",
            "LOG(x)",
        ] {
            let op = Operation::Fit {
                spec: FitSpec {
                    model: Model::Custom {
                        formula: formula.into(),
                    },
                    params: vec![],
                },
                range: Some((1.0, 50.0)),
                residuals: true,
            };
            let tree = op.to_tree();
            let reread = Operation::from_tree(&tree).unwrap();
            assert_eq!(reread, op, "{formula}");
            let Operation::Fit { spec, .. } = reread else {
                panic!("expected Fit");
            };
            assert_eq!(
                spec.model,
                Model::Custom {
                    formula: formula.into()
                }
            );
        }
    }

    #[test]
    fn every_operation_round_trips() {
        for op in all_operations() {
            let tree = op.to_tree();
            assert_eq!(Operation::from_tree(&tree).unwrap(), op, "{tree:?}");
        }
    }

    #[test]
    fn absent_fields_take_their_default_and_unknown_ones_are_ignored() {
        let tree = Tree::Map(vec![
            ("kind".into(), Tree::Text("integrate".into())),
            ("under".into(), Tree::Text("zero".into())),
            ("future_field".into(), Tree::Num(1.0)),
        ]);
        let op = Operation::from_tree(&tree).unwrap();
        assert_eq!(
            op,
            Operation::Integrate {
                spec: IntegrateSpec {
                    ranges: vec![],
                    under: Under::Zero,
                    cumulative: false,
                    normalize: false
                }
            }
        );
    }

    #[test]
    fn an_invalid_tree_is_a_named_error_not_a_panic() {
        let cases = [
            Tree::Null,
            Tree::Map(vec![]),
            Tree::Map(vec![("kind".into(), Tree::Text("unknown".into()))]),
            Tree::Map(vec![
                ("kind".into(), Tree::Text("correct".into())),
                ("op".into(), Tree::Text("modulo".into())),
            ]),
            Tree::Map(vec![
                ("kind".into(), Tree::Text("reconvolution".into())),
                ("components".into(), Tree::Num(-1.0)),
            ]),
        ];
        for c in cases {
            assert!(
                matches!(
                    Operation::from_tree(&c),
                    Err(AnalysisError::InvalidParameter { .. })
                ),
                "{c:?}"
            );
        }
    }

    #[test]
    fn a_non_finite_number_is_written_absent() {
        let op = Operation::Fit {
            spec: FitSpec {
                model: Model::Linear,
                params: vec![ParamSetting {
                    min: Some(f64::NEG_INFINITY),
                    ..Default::default()
                }],
            },
            range: None,
            residuals: false,
        };
        let back = Operation::from_tree(&op.to_tree()).unwrap();
        match back {
            Operation::Fit { spec, .. } => assert_eq!(spec.params[0].min, None),
            _ => unreachable!(),
        }
    }
}
