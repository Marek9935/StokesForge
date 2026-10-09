//! Les opérations du menu Analysis, de bout en bout : colonnes brutes en
//! entrée, colonnes calculées et valeurs en sortie.
//!
//! C'est la **seule** porte d'entrée dont l'application, les scripts `pyst` et
//! le recalcul se servent : ils fournissent les colonnes lues dans le modèle et
//! écrivent ce qui revient. Les sorties sont **alignées sur les lignes de
//! l'entrée** (`NaN` où une ligne est écartée ou hors plage), pour qu'une
//! colonne calculée ait la longueur de sa source.
//!
//! Les clés de sortie sont stables et sans phrase (voir [`Value::key`] et
//! [`OutColumn::key`]) : l'application les met en libellés traduits.

use crate::baseline::{self, Baseline, BaselineMethod, BaselineOp};
use crate::correct::{self, CorrectOp};
use crate::deconv::{self, DirectMethod, ReconvSpec};
use crate::error::AnalysisError;
use crate::fit::{self, FitInput, FitSpec};
use crate::integrate::{self, IntegrateSpec};
use crate::model::Model;
use crate::peaks::{self, PeakSeed, PeaksSpec};
use crate::series::{clean, interpolate, Series};

/// Les colonnes d'une courbe à traiter, ligne à ligne. `NaN` : cellule vide,
/// texte ou invalide.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SeriesInput {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    /// Écart-type de Y (colonne de rôle erreur), pour l'ajustement pondéré.
    pub sigma: Option<Vec<f64>>,
}

/// Une colonne annexe (X, Y) : la référence d'une correction, l'IRF d'une
/// déconvolution.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AuxInput {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
}

/// Détection automatique des pics de départ (M14 a C).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoDetect {
    /// Proéminence minimale, en fraction de l'étendue de Y (0 à 1).
    pub min_prominence: f64,
    pub max_peaks: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operation {
    Baseline {
        method: BaselineMethod,
        op: BaselineOp,
        /// Écrire aussi la ligne de base dans une colonne.
        keep_baseline: bool,
    },
    /// Référence : colonne annexe.
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
    /// Sans `seeds` dans `spec`, `auto` détecte les pics sur la courbe.
    Peaks {
        spec: PeaksSpec,
        auto: Option<AutoDetect>,
        range: Option<(f64, f64)>,
        residuals: bool,
    },
    /// IRF : colonne annexe.
    Reconvolution {
        spec: ReconvSpec,
        residuals: bool,
    },
    /// IRF : colonne annexe.
    DirectDeconvolution {
        method: DirectMethod,
        shift: f64,
    },
}

/// La famille d'une opération, sans ses réglages : le libellé d'historique, la
/// clé du glossaire et le réglage « recalcul automatique » par défaut en
/// dépendent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationKind {
    Baseline,
    Correct,
    Integrate,
    Fit,
    Peaks,
    Reconvolution,
    DirectDeconvolution,
}

impl OperationKind {
    pub const ALL: [OperationKind; 7] = [
        OperationKind::Baseline,
        OperationKind::Correct,
        OperationKind::Integrate,
        OperationKind::Fit,
        OperationKind::Peaks,
        OperationKind::Reconvolution,
        OperationKind::DirectDeconvolution,
    ];

    /// Clé stable (celle de `Tree` et des scripts).
    pub fn key(self) -> &'static str {
        match self {
            OperationKind::Baseline => "baseline",
            OperationKind::Correct => "correct",
            OperationKind::Integrate => "integrate",
            OperationKind::Fit => "fit",
            OperationKind::Peaks => "peaks",
            OperationKind::Reconvolution => "reconvolution",
            OperationKind::DirectDeconvolution => "direct_deconvolution",
        }
    }

    pub fn from_key(key: &str) -> Option<OperationKind> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }
}

/// Au-delà de ce nombre de séries, un lot n'est plus recalculé automatiquement
/// par défaut (M9 b C).
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
        }
    }

    /// Réglage initial de « recalculer automatiquement » : coché, sauf pour la
    /// reconvolution, la déconvolution directe et les lots de plus de
    /// [`HEAVY_BATCH`] séries (M9 b C).
    pub fn auto_recalc_by_default(&self, series_count: usize) -> bool {
        !matches!(
            self.kind(),
            OperationKind::Reconvolution | OperationKind::DirectDeconvolution
        ) && series_count <= HEAVY_BATCH
    }

    /// Cette opération lit une colonne annexe.
    pub fn needs_aux(&self) -> bool {
        matches!(
            self,
            Operation::Correct { .. }
                | Operation::Reconvolution { .. }
                | Operation::DirectDeconvolution { .. }
        )
    }
}

/// Une colonne calculée, de la longueur de l'entrée.
#[derive(Debug, Clone, PartialEq)]
pub struct OutColumn {
    /// Clé stable : `corrected`, `baseline`, `fit`, `residuals`, `cumulative`,
    /// `normalized`, `peak` (+ `index`), `peaks_sum`, `background`,
    /// `deconvolved`.
    pub key: &'static str,
    /// Rang du pic pour `peak` (à partir de 1).
    pub index: Option<usize>,
    pub data: Vec<f64>,
}

/// Une valeur calculée, avec son incertitude éventuelle.
#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    /// Clé stable à points : `param.<nom>`, `stat.<nom>`, `area`, `area.total`,
    /// `peak.<nom>`, `background.<nom>`, `baseline.<nom>`, `count.<nom>`.
    pub key: String,
    /// Rang (plage, pic), à partir de 1.
    pub index: Option<usize>,
    pub value: f64,
    pub error: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// Pas du signal non constant (> 1 %).
    SignalStepNotUniform,
    /// Pas de l'IRF différent de celui du signal (> 1 %).
    IrfStepMismatch,
    /// Points négatifs ramenés à 0 (Richardson-Lucy).
    NegativeClipped(usize),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Outcome {
    pub columns: Vec<OutColumn>,
    pub values: Vec<Value>,
    /// Lignes écartées : X ou Y non fini.
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

    fn stats(&mut self, points: usize, r2: f64, reduced_chi2: f64, dof: usize) {
        self.value("stat.points", None, points as f64, None);
        self.value("stat.r_squared", None, r2, None);
        self.value("stat.reduced_chi2", None, reduced_chi2, None);
        self.value("stat.dof", None, dof as f64, None);
    }
}

/// Une entrée alignée : la série propre et le rang de chaque point dans les
/// lignes d'origine.
struct Aligned {
    series: Series,
    rows: Vec<usize>,
    /// Nombre de lignes de l'entrée.
    len: usize,
    skipped: usize,
    /// X de chaque ligne (`NaN` si absent), pour évaluer une courbe en tout X.
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
        let ok = x_all[i].is_finite() && input.y[i].is_finite();
        let s = input.sigma.as_ref().and_then(|s| s.get(i).copied());
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
    /// Étale une valeur par point propre sur toutes les lignes.
    fn expand(&self, per_point: &[f64]) -> Vec<f64> {
        let mut out = vec![f64::NAN; self.len];
        for (k, &row) in self.rows.iter().enumerate() {
            out[row] = per_point[k];
        }
        out
    }

    /// Évalue `f` sur chaque ligne dont X est fini.
    fn eval_all(&self, f: impl Fn(f64) -> f64) -> Vec<f64> {
        self.x_all
            .iter()
            .map(|&x| if x.is_finite() { f(x) } else { f64::NAN })
            .collect()
    }

    /// Évalue `f` sur chaque ligne dont X est fini et dans `range`.
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

    /// Les `y` des lignes (valeur d'origine ou `NaN`).
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

fn deconv_warnings(w: deconv::DeconvWarnings) -> Vec<Warning> {
    let mut out = Vec::new();
    if w.non_uniform_signal {
        out.push(Warning::SignalStepNotUniform);
    }
    if w.irf_step_mismatch {
        out.push(Warning::IrfStepMismatch);
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

/// Exécute `op` sur chaque entrée. Une erreur sur une entrée annule le lot
/// (l'appelant décide de relancer les autres). Un ajustement global traite
/// toutes les entrées ensemble.
/// Plafonds des réglages, communs à l'interface, à `pyst` et au fichier : un
/// degré de 10⁹ créerait un milliard de paramètres, des itérations sans borne
/// bloqueraient l'acteur de session au recalcul.
pub const MAX_DEGREE: u32 = 9;
pub const MAX_ITERATIONS: usize = 5000;

impl Operation {
    /// Refuse les réglages hors plafond avant tout calcul.
    pub fn validate(&self) -> Result<(), AnalysisError> {
        let bad = |name: &str| {
            Err(AnalysisError::InvalidParameter {
                name: name.to_string(),
            })
        };
        match self {
            Operation::Baseline {
                method: BaselineMethod::Zones { degree, .. },
                ..
            } if !(1..=MAX_DEGREE).contains(degree) => bad("degree"),
            Operation::Fit { spec, .. } => match spec.model {
                Model::Polynomial { degree } if !(1..=MAX_DEGREE).contains(&degree) => {
                    bad("degree")
                }
                _ => Ok(()),
            },
            Operation::Peaks { auto, .. } if auto.is_some_and(|a| a.max_peaks > 100) => {
                bad("max_peaks")
            }
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
            Operation::Reconvolution { spec, .. } if !(1..=4).contains(&spec.components) => {
                bad("components")
            }
            _ => Ok(()),
        }
    }
}

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
                let c = correct::correct(&a.series, &reference, *op)?;
                o.column("corrected", None, a.expand(&c.values));
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
            Operation::Fit { .. } => unreachable!("traité plus haut"),
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
                // `fitted` est aligné sur le signal trié : on le relit à chaque X de
                // l'entrée (interpolation linéaire, exacte aux nœuds), y compris
                // aux lignes dont Y est vide.
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
                o.warnings = deconv_warnings(r.warnings);
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
        // σ aligné sur les points retenus par la plage.
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

/// `y − modèle` sur les lignes propres où le modèle est défini.
fn fill_residuals(a: &Aligned, total: &[f64]) -> Vec<f64> {
    let mut out = vec![f64::NAN; a.len];
    for (k, &row) in a.rows.iter().enumerate() {
        if total[row].is_finite() {
            out[row] = a.series.y[k] - total[row];
        }
    }
    out
}
