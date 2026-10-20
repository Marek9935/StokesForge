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
use crate::expr::ExprError;
use crate::fit::{self, FitInput, FitSpec, ParamSetting};
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
    /// Paramètres libres que les données ne déterminent pas : leur incertitude
    /// est omise (#203).
    Undetermined(usize),
    /// Le solveur s'est arrêté sur un plateau (#281) ou sans pouvoir accepter
    /// de pas alors que la pente restait grande (C1, #313), non sur un
    /// minimum : le résultat est rendu mais n'est pas un ajustement convergé.
    NotConverged,
    /// L'IRF est coupée au-dessus de 1 % de son pic au décalage retenu : la
    /// queue manquante vaut 0 et le noyau est renormalisé (C2, #313).
    IrfTruncated,
    /// Points de données interpolés à travers un trou (cellule vide ou non
    /// finie) de la colonne de référence (C3, #313).
    BridgedReferenceGap(usize),
    /// Divisions par zéro, rendues vides (`NaN`) (C5, #313).
    DivisionByZero(usize),
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

    fn warn_undetermined(&mut self, count: usize) {
        if count > 0 {
            self.warnings.push(Warning::Undetermined(count));
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
        let s = input.sigma.as_ref().and_then(|s| s.get(i).copied());
        // Une ligne pondérée dont σ est vide, nul ou négatif est écartée et
        // comptée, comme une ligne dont X ou Y manque (#205).
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

/// X des lignes de la référence dont Y n'est pas fini alors que X l'est : les
/// trous que `correct` peut enjamber. Une ligne sans X n'a pas de position.
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

/// Plafonds des réglages, communs à l'interface (la table des grandeurs de
/// `stokesforge-app` les lit), à `pyst` et au fichier : un degré de 10⁹
/// créerait un milliard de paramètres, des itérations sans borne bloqueraient
/// l'acteur de session au recalcul. `Model::compile` applique aussi
/// [`MAX_DEGREE`] : un degré lu tel quel d'un fichier ne doit rien allouer
/// avant d'être refusé.
pub const MAX_DEGREE: u32 = 9;
pub const MAX_ITERATIONS: usize = 5000;
/// Nombre maximal de pics cherchés par la détection automatique.
pub const MAX_PEAKS: usize = 100;
/// Nombre maximal de composantes d'une reconvolution (le minimum est 1).
pub const MAX_COMPONENTS: usize = 4;

/// Ce qui manque à une opération pour être lancée, **sans consulter les
/// données** : une donnée, jamais une phrase (l'interface la met en phrase par
/// un `match` exhaustif). Ce que seule la lecture du projet révèle — pas de
/// série, colonne de référence absente, colonne disparue — n'est pas ici.
///
/// Un point ou une zone « pas encore posé » se dit par un `NaN` dans
/// l'opération, comme partout dans le crate (`NaN` : vide) : le brouillon d'un
/// formulaire se décrit alors en [`Operation`] même incomplet.
#[derive(Debug, Clone, PartialEq)]
pub enum Readiness {
    /// Rien ne manque.
    Ready,
    /// Pas assez de points posés pour la ligne de base.
    NeedsPoints { needed: usize, have: usize },
    /// Une zone de fond au moins.
    NeedsZone,
    /// L'expression du modèle libre est illisible.
    BadExpression(ExprError),
    /// Le modèle n'a aucun paramètre.
    NoParameter,
    /// Un réglage est refusé par le calcul (par exemple un degré hors plafond).
    Invalid(AnalysisError),
}

impl Operation {
    /// Ce qui empêche de lancer l'opération, sans lire de donnée (voir
    /// [`Readiness`]). Ne refait pas [`Operation::validate`] : celui-ci juge les
    /// plafonds, celui-là ce qui reste à poser.
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
                    // Une ancre non posée (non finie) ne compte pas.
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
            // Une plage non posée (NaN) n'est pas « toute la courbe » : `run`
            // la refuse (#276), elle reste donc à poser.
            Operation::Integrate { spec } if !spec.ranges.iter().all(|r| is_posed(*r)) => {
                Readiness::NeedsZone
            }
            Operation::Correct { .. }
            | Operation::Integrate { .. }
            | Operation::Peaks { .. }
            | Operation::Reconvolution { .. }
            | Operation::DirectDeconvolution { .. } => Readiness::Ready,
        }
    }

    /// Retire de l'opération tout ce qui désigne des **données** : plages
    /// d'intégration, d'ajustement et de pics, positions de départ des pics,
    /// réglages par paramètre d'un ajustement ou d'une reconvolution (valeurs
    /// initiales, bornes, paramètres fixés **et partagés**), fenêtre d'une
    /// reconvolution. Il reste la méthode et ses réglages (modèle, degré,
    /// fenêtre de recentrage des pics, cases de sortie) et la définition d'une
    /// ligne de base. La longueur de `params` est gardée.
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
            | Operation::DirectDeconvolution { .. } => {}
        }
    }

    /// Refuse les réglages hors plafond avant tout calcul.
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
            } if !(1..=MAX_DEGREE).contains(degree) => bad("degree"),
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
            // Une graine posée n'a que des valeurs finies (centre toujours ;
            // hauteur et largeur quand elles sont données). La largeur
            // imposée est de plus strictement positive (#285) ; la hauteur,
            // de signe libre, est jugée par l'ajustement.
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
            _ => Ok(()),
        }
    }
}

/// Le modèle n'a aucun paramètre libre : aucun, ou tous fixés (le solveur
/// refuse de toute façon avec la même erreur). Un modèle qui ne se compile pas
/// n'est pas jugé ici : sa propre erreur sort à l'ajustement.
fn no_free_parameter(spec: &FitSpec) -> bool {
    spec.model.compile().is_ok_and(|compiled| {
        (0..compiled.param_count()).all(|j| spec.params.get(j).is_some_and(|p| p.fixed))
    })
}

/// Un point ou une zone posé n'a que des coordonnées finies.
fn is_posed(pair: (f64, f64)) -> bool {
    pair.0.is_finite() && pair.1.is_finite()
}

/// Exécute `op` sur chaque entrée. Une erreur sur une entrée annule le lot
/// (l'appelant décide de relancer les autres). Un ajustement global traite
/// toutes les entrées ensemble.
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
                o.warn_undetermined(r.undetermined);
                o.warn_plateau(r.plateau);
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
                // Étendre, pas écraser : `Undetermined` et `NotConverged` sont déjà posés
                // plus haut ; `deconv_warnings` ne produit que les trois autres (#292).
                o.warnings.extend(deconv_warnings(r.warnings));
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
    // Division par une ligne de base exactement nulle : comptée (C5, #313).
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
        o.warn_undetermined(fs.undetermined);
        o.warn_plateau(fs.plateau);
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
