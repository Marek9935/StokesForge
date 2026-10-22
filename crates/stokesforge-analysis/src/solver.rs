//! Moteur de moindres carrés non linéaires (Levenberg-Marquardt) à **cases de
//! paramètres** (« slots »), commun à l'ajustement, à l'ajustement global, à la
//! décomposition en pics et à la reconvolution (#188, #189, #190).
//!
//! Un problème est une liste de **termes** (une courbe mesurée et une fonction
//! qui la modélise) qui lisent un même vecteur de cases. Une case partagée par
//! plusieurs termes, c'est un paramètre global (M13 b) ; une case lue par deux
//! pics, c'est une largeur liée (#189). Le moteur ne connaît ni modèle ni pic.
//!
//! Dérivées par différences centrées (pas relatif au paramètre, **raffiné** tant
//! que les dérivées à `h` et `h/4` divergent au-delà de l'arrondi, #294) : les
//! modèles libres n'ont pas de dérivée analytique, et un seul chemin de calcul
//! vaut mieux que deux (modèle intégré, modèle libre). Les bornes sont tenues
//! par projection du pas.
//!
//! X décalé (#282, #293, #294) : trois garde-fous tiennent un X de grande
//! valeur absolue. Le pas de Levenberg-Marquardt passe par les moindres carrés
//! augmentés ; un pas minuscule ne conclut à la convergence qu'après un pas
//! presque non amorti (l'amortissement de Marquardt bride la direction faible
//! des colonnes quasi colinéaires) ; une colonne n'est indéterminée que sous
//! `DETERMINED_MIN_SHARE` de sa norme.

use crate::error::AnalysisError;
use crate::linalg::{invert, least_squares, Matrix};

/// Une case de paramètre.
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

/// Fonction qui remplit un tampon avec le modèle évalué aux X d'un terme.
pub type TermEval<'a> = Box<dyn Fn(&[f64], &mut [f64]) + 'a>;

/// Une courbe mesurée et le modèle qui la reproduit.
pub struct Term<'a> {
    pub x: &'a [f64],
    pub y: &'a [f64],
    /// Écarts-types des Y (pondération `1/σ`). `None` : poids égaux.
    pub sigma: Option<&'a [f64]>,
    /// Remplit `out` (longueur de `x`) avec le modèle évalué aux X, d'après les
    /// valeurs de toutes les cases.
    pub eval: TermEval<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    pub max_iterations: usize,
    /// Mise à l'échelle des erreurs par le χ² réduit (comme `absolute_sigma =
    /// False` de scipy). Sans pondération, c'est toujours le cas.
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

/// Qualité de l'ajustement d'un terme.
#[derive(Debug, Clone, PartialEq)]
pub struct TermStats {
    pub points: usize,
    /// Somme des carrés des résidus, non pondérée.
    pub ss_res: f64,
    /// `1 − SSres/SStot` (non pondéré) ; `NaN` si Y est constant.
    pub r_squared: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Solution {
    pub values: Vec<f64>,
    /// Incertitude-type de chaque case ; `None` pour une case fixée ou si la
    /// covariance n'est pas calculable.
    pub errors: Vec<Option<f64>>,
    /// Case libre que les données ne déterminent pas (colonne du jacobien nulle
    /// ou combinaison des précédentes, #203) : son incertitude est `None`.
    pub undetermined: Vec<bool>,
    /// Somme pondérée des carrés des résidus.
    pub chi2: f64,
    pub dof: usize,
    pub reduced_chi2: f64,
    pub iterations: usize,
    pub terms: Vec<TermStats>,
    /// Le solveur s'est arrêté sur un **plateau**, pas sur un minimum (#281) :
    /// des cases libres sans effet sur le modèle (gradient nul) alors que le
    /// résidu reste grand, ex. un pic parti hors des données. La solution est
    /// rendue (jamais d'erreur dure : elle ferait perdre le résultat) mais ne
    /// doit pas être lue comme un ajustement convergé.
    pub plateau: bool,
    /// Le solveur n'a pu accepter **aucun** pas alors que la pente restait
    /// au-dessus de `STALL_COSINE` (C1, #313) : le point de départ est rendu
    /// tel quel, avec son R², sans être un minimum. Rendu et signalé comme le
    /// plateau, jamais en erreur dure ni en silence.
    pub stalled: bool,
}

/// Seuil relâché de la pente (cosinus résidu/colonne) sous lequel un arrêt
/// sans pas accepté reste un minimum atteint : plus lâche que le `1e-9` de
/// l'arrêt par gradient, car les derniers pas d'un vrai minimum butent sur
/// l'arrondi avant d'atteindre ce seuil.
const STALL_COSINE: f64 = 1e-6;

/// Part de l'énergie des Y (pondérée comme le résidu) sous laquelle le χ² est
/// du bruit d'arrondi : un ajustement exact (données sans bruit) n'a plus de
/// pente à lire, son cosinus vaut ~0,05 sans rien vouloir dire.
const EXACT_FIT_FRACTION: f64 = 1e-20;

/// Part de l'énergie des Y sous laquelle le χ² ne peut plus baisser que par
/// l'arrondi : un résidu de l'ordre de l'epsilon de `f64` (~1e-16 de Y, soit
/// ~1e-32 en carré) sur des données sans bruit. Au-delà de ce plancher
/// (1e-28, quelques epsilons) chaque pas « améliore » le χ² de quelques ulp sans
/// que les critères relatifs (`drop`, `moved`) ne se déclenchent, et la
/// convergence dépendait du dernier ulp de `exp`/`ln` de la plateforme (#332).
/// Le solveur est alors arrivé : ce n'est pas un plateau, le résidu est nul.
const ROUNDING_FLOOR_FRACTION: f64 = 1e-28;

/// λ du pas de vérification (#293) : l'amortissement de Marquardt suit la
/// diagonale de `JᵀJ`, qui vaut 1e15 et plus pour un X décalé de 1e7 ; sous ce
/// plancher il bride encore la direction faible, donc on le coupe presque.
const UNDAMPED_LAMBDA: f64 = 1e-30;

/// Part de la variance de Y (`SStot`) que le résidu (`SSres`) doit dépasser,
/// sur une courbe au moins, pour qu'un arrêt avec des cases sans effet soit un
/// plateau et non un modèle surparamétré qui s'ajuste bien (R² < 0,5). Seuil
/// conservateur : un minimum local de données bruitées reste loin de lui.
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

/// Ajuste les cases libres de `slots` aux `terms`.
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

    // Énergie des Y, pondérée comme le résidu : l'échelle des deux seuils
    // d'ajustement exact (`ROUNDING_FLOOR_FRACTION`, `EXACT_FIT_FRACTION`).
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
    // Un pas minuscule n'est une preuve de convergence que s'il n'est pas
    // minuscule à cause de l'amortissement (#293) : on le vérifie par un pas
    // presque non amorti avant de conclure.
    let mut undamped_tried = false;
    let mut jac = Matrix::zeros(m, free.len());
    while iterations < opts.max_iterations {
        iterations += 1;
        jacobian(terms, &values, slots, &free, &mut jac, &mut buf);
        let a = jac.gram();
        let g = jac.transpose_mul(&r);
        // Un non-fini dans le gradient ou le Gram échapperait au pli `max`
        // ci-dessous (qui ignore NaN) : le solveur conclurait « convergé »
        // sur un jacobien NaN (C6, #313). À tester avant le pli.
        if g.iter().any(|v| !v.is_finite()) || !(0..free.len()).all(|k| a.get(k, k).is_finite()) {
            return Err(AnalysisError::ModelNotEvaluable);
        }
        // Critère de gradient **indépendant de l'échelle** des données (le
        // `gtol` de MINPACK) : cosinus de l'angle entre le résidu et chaque
        // colonne du jacobien. Un seuil absolu arrêtait l'ajustement dès le
        // premier pas quand les Y valent 1e-12 (ampères, sections efficaces).
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
            // Pas de Levenberg-Marquardt par moindres carrés **augmentés**
            // (QR de [J; √(λD)] contre [−r; 0]) et non par les équations
            // normales : JᵀJ élève le conditionnement au carré, et deux
            // colonnes quasi colinéaires (X décalé de 1e6, #282) y perdaient
            // toute précision. Marquardt : l'amortissement suit la diagonale
            // de chaque paramètre, sans plancher absolu (les colonnes du
            // jacobien peuvent différer de vingt ordres de grandeur) ; une
            // colonne nulle prend 1.
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
                // Plancher très bas : à 1e-12, l'amortissement bridait le fond
                // d'une vallée étroite (colonnes quasi colinéaires, #282).
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
            // Plus aucun pas ne fait baisser le χ² : minimum local atteint si la
            // pente est (presque) nulle ; sinon le solveur est bloqué (C1) et la
            // solution est rendue marquée, jamais présentée comme convergée.
            converged = true;
            // Aucun pas n'ayant été accepté, `a`, `g` et `residual_norm` sont
            // ceux de ce point. Un paramètre collé à sa borne ne relève pas d'ici :
            // son pas projeté ne bouge plus, `chi2_trial <= chi2` l'accepte et
            // l'arrêt `moved < 1e-11` le déclare convergé, sans avertissement.
            // Un résidu au niveau de l'arrondi (ajustement exact) n'a pas de
            // pente lisible : son cosinus n'est que du bruit, jamais un blocage.
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

    // Covariance au point trouvé.
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
    // Covariance des seules cases déterminées : une case sans effet ne rend
    // pas muettes les incertitudes des autres.
    let kept: Vec<usize> = (0..free.len()).filter(|&k| determined[k]).collect();
    let mut sub = Matrix::zeros(m, kept.len());
    for row in 0..m {
        for (c, &k) in kept.iter().enumerate() {
            sub.set(row, c, jac.get(row, k));
        }
    }
    // À 0 degré de liberté (ajustement exact), la mise à l'échelle par le χ²
    // réduit n'a pas de base : pas d'incertitude, comme le chemin linéaire.
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

    // Statistiques par terme.
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
    // Plateau (#281) : une case libre sans effet (colonne du jacobien nulle)
    // ET un résidu qui reste grand. Des cases sans effet sur un bon ajustement
    // restent un simple `undetermined` ; un résidu grand sans case sans effet
    // est un vrai minimum local, pas un plateau.
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

/// Part de la norme d'une colonne du jacobien qu'elle doit avoir de neuf (hors
/// des colonnes précédentes) pour être déterminée. Le bruit d'une différence
/// centrée au pas relatif de 1e-6 est de l'ordre de `ε / 1e-6 ≈ 2e-10` : un
/// plancher à 1e-8 garde 50 fois de marge sur une vraie dépendance, et laisse
/// passer une droite sur un X décalé de 1e6 (part neuve ≈ 4,5e-7, #293), que
/// l'ancien millionième déclarait indéterminée à tort.
const DETERMINED_MIN_SHARE: f64 = 1e-8;

/// Pour chaque colonne du jacobien, `true` si les données la déterminent : sa
/// part non portée par les colonnes précédentes (Gram-Schmidt modifié) dépasse
/// `DETERMINED_MIN_SHARE` de sa norme. Une colonne nulle (paramètre sans effet)
/// ou combinaison des précédentes (X tous égaux pour une droite) est écartée.
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

/// `J[i][k] = ∂résidu_i/∂case_libre_k`, en différences centrées.
///
/// Le pas relatif à la valeur du paramètre (1e-6) est faux pour un paramètre
/// de position à grande valeur : 1 pour un centre de pic à 1e6, plus que la
/// largeur du pic, donc une dérivée fausse rendue comme convergée (#294). Le
/// pas est donc **raffiné** : tant que la dérivée à `h` et celle à `h/4`
/// divergent, on garde le pas plus fin. Un modèle lisse à l'échelle du pas
/// garde son pas d'origine, à l'identique.
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
        // Pas relatif à l'échelle des paramètres ; le repli absolu ne sert que si
        // tous sont nuls (jamais un plancher qui écraserait des valeurs en 1e-12).
        let mut h = 1e-6
            * values[i]
                .abs()
                .max(1e-2 * pmax)
                .max(if pmax > 0.0 { 0.0 } else { 1e-9 });
        let mut coarse = column(terms, &mut work, slots, i, h, buf, &mut plus, &mut minus);
        for _ in 0..MAX_STEP_REFINEMENTS {
            // Affiner un pas ne rend pas une dérivée non finie fine.
            if !h.is_finite() || coarse.values.iter().any(|v| !v.is_finite()) {
                break;
            }
            let fine_h = h / STEP_REFINEMENT_RATIO;
            let fine = column(
                terms, &mut work, slots, i, fine_h, buf, &mut plus, &mut minus,
            );
            // Un écart que l'arrondi explique n'est pas un désaccord : sans
            // cela, un départ à zéro (pas de 1e-15) descendrait dans le bruit.
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

/// Nombre maximal de divisions du pas de différence (`4^8` au plus).
const MAX_STEP_REFINEMENTS: usize = 8;
/// Rapport entre deux pas successifs de la comparaison des dérivées.
const STEP_REFINEMENT_RATIO: f64 = 4.0;
/// Écart relatif (norme de la colonne) toléré entre les dérivées à `h` et `h/4`.
const STEP_AGREEMENT: f64 = 1e-3;

/// Une colonne du jacobien et l'incertitude d'arrondi de chacune de ses lignes.
struct Column {
    values: Vec<f64>,
    noise: Vec<f64>,
}

/// Colonne du jacobien de la case `i`, en différences centrées de pas `h`.
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
    // Le pas ne sort pas des bornes : à une borne, différence décentrée.
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
        // Une dérivée non finie reste telle quelle (C6, #313) : la mettre à 0
        // faisait passer un modèle non évaluable pour une case sans effet.
        column.values.push(d);
        // Un pas qui ne déplace plus la valeur n'apprend rien : bruit infini.
        column.noise.push(if den > 0.0 {
            ROUNDOFF_FACTOR * f64::EPSILON * p.abs().max(mn.abs()) / den
        } else {
            f64::INFINITY
        });
    }
    column
}

/// Marge sur l'erreur d'arrondi d'une différence de deux résidus.
const ROUNDOFF_FACTOR: f64 = 8.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn xs(n: usize, a: f64, b: f64) -> Vec<f64> {
        (0..n)
            .map(|i| a + (b - a) * i as f64 / (n - 1) as f64)
            .collect()
    }

    /// Bruit déterministe (suite de Weyl) d'amplitude ±1.
    fn noise(i: usize) -> f64 {
        ((i as f64 * 0.618_033_988_749_895).fract() - 0.5) * 2.0
    }

    #[test]
    fn retrouve_une_exponentielle_avec_decalage() {
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

    /// Gaussienne + décalage : le départ de l'issue #281.
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
    fn un_depart_hors_des_donnees_s_arrete_sur_un_plateau() {
        // #281 : xc = 1000, loin du pic -> jacobien nul sauf y0.
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
    fn un_ajustement_sain_ou_bruite_n_est_pas_un_plateau() {
        for amp in [0.0, 0.3] {
            let (x, y) = gaussian_data(amp);
            let sol = solve_problem(
                &gaussian_slots(7.0, 2.0),
                &[gaussian_problem(&x, &y)],
                Options::default(),
            )
            .unwrap();
            assert!(sol.terms[0].r_squared > 0.9 && !sol.plateau, "bruit {amp}");
        }
    }

    /// Huit points X = 0..7 et un vecteur orthogonal à `1` et à `x` : un
    /// ajustement affine laisse exactement `k · ORTHO` de résidu.
    const ORTHO: [f64; 8] = [1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0];
    /// `Σ (x − x̄)²` pour X = 0..7.
    const SS_X: f64 = 42.0;

    /// Les données `y = x + k·ORTHO` dont SSres/SStot vaut `fraction` après un
    /// ajustement affine : SSres = 8k², SStot = 42 + 8k².
    fn affine_with_residual(fraction: f64) -> (Vec<f64>, Vec<f64>) {
        let k = (SS_X * fraction / (8.0 * (1.0 - fraction))).sqrt();
        let x: Vec<f64> = (0..8).map(f64::from).collect();
        let y = x.iter().zip(ORTHO).map(|(x, r)| x + k * r).collect();
        (x, y)
    }

    /// `p0 + p1·x`, plus (si `dead`) une case libre `p2` sans aucun effet.
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
        #[allow(clippy::let_and_return)] // `x` et `y` doivent survivre à l'appel.
        let sol = solve_problem(
            &affine_slots(dead),
            &[affine_term(&x, &y)],
            Options::default(),
        )
        .unwrap();
        sol
    }

    #[test]
    fn le_jeu_de_donnees_construit_donne_le_residu_voulu() {
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
    fn le_seuil_du_plateau_sur_la_part_de_residu_est_a_une_demie() {
        // Case sans effet dans les deux cas : seule la part de résidu change.
        let sous = solve_affine(0.49, true);
        let dessus = solve_affine(0.51, true);
        assert_eq!(sous.undetermined, vec![false, false, true]);
        assert_eq!(dessus.undetermined, vec![false, false, true]);
        assert!(!sous.plateau, "0,49 : sous le seuil");
        assert!(dessus.plateau, "0,51 : au-dessus du seuil");
        const { assert!(PLATEAU_RESIDUAL_FRACTION > 0.49 && PLATEAU_RESIDUAL_FRACTION < 0.51) };
    }

    #[test]
    fn un_gros_residu_sans_case_sans_effet_est_un_minimum_pas_un_plateau() {
        for f in [0.51, 0.9, 0.99] {
            let sol = solve_affine(f, false);
            assert!(sol.undetermined.iter().all(|u| !u), "{f}");
            assert!(!sol.plateau, "{f}");
        }
    }

    #[test]
    fn un_modele_surparametre_qui_s_ajuste_bien_est_indetermine_sans_plateau() {
        for f in [0.0, 1e-6, 0.1] {
            let sol = solve_affine(f, true);
            assert_eq!(sol.undetermined, vec![false, false, true], "{f}");
            assert!(!sol.plateau, "{f}");
        }
    }

    #[test]
    fn une_courbe_constante_n_est_jamais_un_plateau() {
        // SStot = 0 : la part de résidu n'a pas de sens (SSres/0 = +inf si le
        // modèle ne peut pas l'atteindre : `a` fixé à 0, y = 3).
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
    fn un_ajustement_global_est_sur_un_plateau_si_une_seule_courbe_l_est() {
        let (xa, ya) = affine_with_residual(0.01);
        let (xb, yb) = affine_with_residual(0.9);
        let slots = affine_slots(true);
        for ordre in [[0, 1], [1, 0]] {
            let courbes = [(&xa, &ya), (&xb, &yb)];
            let terms: Vec<Term> = ordre
                .iter()
                .map(|&i| affine_term(courbes[i].0, courbes[i].1))
                .collect();
            let sol = solve_problem(&slots, &terms, Options::default()).unwrap();
            assert!(sol.plateau, "ordre {ordre:?}");
        }
        // Deux courbes bien ajustées : pas de plateau.
        let sol = solve_problem(
            &slots,
            &[affine_term(&xa, &ya), affine_term(&xa, &ya)],
            Options::default(),
        )
        .unwrap();
        assert!(!sol.plateau);
    }

    #[test]
    fn l_exemple_de_l_issue_281_est_un_plateau_et_garde_ses_valeurs() {
        let (x, y) = gaussian_data(0.0);
        let sol = solve_problem(
            &gaussian_slots(1000.0, 1.0),
            &[gaussian_problem(&x, &y)],
            Options::default(),
        )
        .unwrap();
        assert!(sol.plateau);
        // La solution est rendue (pas d'erreur dure) : xc et fwhm n'ont pas bougé.
        assert!((sol.values[1] - 1000.0).abs() < 1e-6, "{:?}", sol.values);
        assert!((sol.values[3] - 1.0).abs() < 1e-6, "{:?}", sol.values);
        assert_eq!(sol.undetermined, vec![false, true, true, true]);
    }

    #[test]
    fn les_incertitudes_suivent_le_bruit() {
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
        // σ_pente ≈ σ_bruit / (√n · σ_x) = 0.2·0.577/(20·2.89) ≈ 0.002.
        assert!(e > 0.0005 && e < 0.01, "erreur sur la pente {e}");
        assert!(sol.reduced_chi2 > 0.0 && sol.dof == 398);
    }

    #[test]
    fn une_case_fixee_ne_bouge_pas_et_n_a_pas_d_erreur() {
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
    fn les_bornes_sont_respectees() {
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

    /// Droite `a + b·x` sur `x`, deux paramètres libres.
    fn droite_sur<'a>(x: &'a [f64], y: &'a [f64]) -> Term<'a> {
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

    /// Droite `y = 2 + 3t` sur `x = off + t` (60 points, `t ∈ [0, 1]`), départ
    /// `(a, b) = (-off, 1)` : le X décalé rend les colonnes quasi colinéaires.
    fn droite_decalee(off: f64) -> Solution {
        let t: Vec<f64> = (0..60).map(|i| f64::from(i) / 59.0).collect();
        let x: Vec<f64> = t.iter().map(|t| off + t).collect();
        let y: Vec<f64> = t
            .iter()
            .enumerate()
            .map(|(i, t)| 2.0 + 3.0 * t + 0.01 * libm::sin(i as f64 * 1.7))
            .collect();
        let slots = vec![Slot::new("a", -off), Slot::new("b", 1.0)];
        let sol = solve_problem(&slots, &[droite_sur(&x, &y)], Options::default());
        sol.unwrap()
    }

    #[test]
    fn un_pas_amorti_minuscule_n_est_pas_une_convergence() {
        // #293 : l'amortissement de Marquardt suit `JᵀJ` (≈ off²) et bride la
        // direction faible ; le pas devenait minuscule, pris pour la
        // convergence, alors que le χ² valait 7000 fois le minimum.
        for off in [1e7, 1e8, 1e9] {
            let sol = droite_decalee(off);
            let (a, b) = (sol.values[0], sol.values[1]);
            assert!((b - 3.0).abs() < 1e-2, "off = {off:e} : {:?}", sol.values);
            assert!(sol.chi2 < 1e-2, "off = {off:e} : χ² = {:e}", sol.chi2);
            assert!((a + b * off - 2.0).abs() < 1e-2, "off = {off:e} : a = {a}");
        }
    }

    #[test]
    fn zero_ddl_avec_echelle_pas_d_incertitude() {
        let (x, y) = (vec![0.0, 1.0], vec![1.0, 3.0]);
        let slots = vec![Slot::new("a", 0.0), Slot::new("b", 0.0)];
        let sol = solve_problem(&slots, &[droite_sur(&x, &y)], Options::default()).unwrap();
        assert_eq!(sol.dof, 0);
        assert!(sol.errors.iter().all(Option::is_none));
        assert!(sol.reduced_chi2.is_nan());
        assert!(sol.chi2.is_finite() && sol.chi2 < 1e-12);
    }

    #[test]
    fn zero_ddl_sans_echelle_garde_les_incertitudes() {
        // Choix documenté de #280 : `scale_errors == false` (σ absolus) n'a pas
        // besoin du χ² réduit ; la covariance brute reste valable à 0 ddl.
        // Pour x = {0, 1}, σ = 1 : var(a) = 1, var(b) = 2.
        let (x, y) = (vec![0.0, 1.0], vec![1.0, 3.0]);
        let slots = vec![Slot::new("a", 0.0), Slot::new("b", 0.0)];
        let opts = Options {
            scale_errors: false,
            ..Options::default()
        };
        let sol = solve_problem(&slots, &[droite_sur(&x, &y)], opts).unwrap();
        assert_eq!(sol.dof, 0);
        assert!(sol.reduced_chi2.is_nan());
        let (ea, eb) = (sol.errors[0].unwrap(), sol.errors[1].unwrap());
        assert!((ea - 1.0).abs() < 1e-5, "{ea}");
        assert!((eb - 2f64.sqrt()).abs() < 1e-5, "{eb}");
    }

    #[test]
    fn un_ddl_avec_echelle_incertitudes_mises_a_l_echelle() {
        // 3 points, 2 libres : dof 1, erreurs = covariance brute × χ² réduit.
        let (x, y) = (vec![0.0, 1.0, 2.0], vec![1.0, 3.5, 4.0]);
        let slots = vec![Slot::new("a", 0.0), Slot::new("b", 0.0)];
        let mis = solve_problem(&slots, &[droite_sur(&x, &y)], Options::default()).unwrap();
        let opts = Options {
            scale_errors: false,
            ..Options::default()
        };
        let brut = solve_problem(&slots, &[droite_sur(&x, &y)], opts).unwrap();
        assert_eq!(mis.dof, 1);
        assert!(mis.reduced_chi2.is_finite() && mis.reduced_chi2 > 0.0);
        for k in 0..2 {
            let (m, b) = (mis.errors[k].unwrap(), brut.errors[k].unwrap());
            let attendu = b * mis.reduced_chi2.sqrt();
            assert!((m - attendu).abs() < 1e-6 * attendu, "{m} vs {attendu}");
        }
    }

    #[test]
    fn moins_de_points_que_de_libres_refuse_avant_le_solveur() {
        // m < libres : jamais de dof négatif, refus net avec les deux effectifs.
        let (x, y) = (vec![0.0, 1.0], vec![1.0, 3.0]);
        let slots = vec![
            Slot::new("a", 0.0),
            Slot::new("b", 0.0),
            Slot::new("c", 0.0),
        ];
        let e = solve_problem(&slots, &[droite_sur(&x, &y)], Options::default()).unwrap_err();
        assert_eq!(
            e,
            AnalysisError::NotEnoughPoints {
                found: 2,
                needed: 3
            }
        );
        // Une case fixée ramène le besoin à 2 : accepté, dof 0.
        let mut fixed = Slot::new("c", 0.0);
        fixed.fixed = true;
        let slots = vec![Slot::new("a", 0.0), Slot::new("b", 0.0), fixed];
        let sol = solve_problem(&slots, &[droite_sur(&x, &y)], Options::default()).unwrap();
        assert_eq!(sol.dof, 0);
    }

    #[test]
    fn deux_termes_partagent_une_case_globale() {
        // y1 = k·x, y2 = k·x + 10 : k partagé, décalage propre à la 2e courbe.
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
    fn la_ponderation_donne_plus_de_poids_aux_points_precis() {
        // Deux points aberrants à grand σ n'attirent pas la droite.
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
    fn erreurs_de_probleme() {
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

    /// Terme à `eval` sur mesure : `y` constant, modèle `f` d'une case.
    fn one_case<'a>(x: &'a [f64], y: &'a [f64], f: impl Fn(f64) -> f64 + 'a) -> Term<'a> {
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

    /// C1 (#313) : un coin sans descente. `f = 1 + |d| + 0,5·d` (d = a − 3) a un
    /// minimum en pointe : la pente centrée vaut 0,5, aucun pas ne baisse le
    /// χ². Le départ est rendu, marqué `stalled`, jamais « convergé » en silence.
    #[test]
    fn un_solveur_sans_pas_accepte_et_a_pente_forte_est_marque_bloque() {
        let x = xs(5, 0.0, 4.0);
        let y = vec![0.0; 5];
        let term = one_case(&x, &y, |a| {
            let d = a - 3.0;
            1.0 + d.abs() + 0.5 * d
        });
        let sol = solve_problem(&[Slot::new("a", 3.0)], &[term], Options::default()).unwrap();
        assert!(sol.stalled, "{sol:?}");
        assert_eq!(sol.values[0], 3.0);
    }

    /// Un vrai minimum lisse (pente nulle) ne bascule pas ; un paramètre collé
    /// à sa borne non plus.
    #[test]
    fn un_minimum_lisse_ou_une_borne_atteinte_ne_sont_pas_bloques() {
        let x = xs(5, 0.0, 4.0);
        let y = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let lisse = one_case(&x, &y, |a| a);
        let sol = solve_problem(&[Slot::new("a", 5.0)], &[lisse], Options::default()).unwrap();
        assert!(
            !sol.stalled && (sol.values[0] - 3.0).abs() < 1e-4,
            "{sol:?}"
        );

        // f = 1 + a, a ≥ 0 : le minimum sous contrainte est la borne.
        let mut slot = Slot::new("a", 2.0);
        slot.min = 0.0;
        let zero = vec![0.0; 5];
        let borne = one_case(&x, &zero, |a| 1.0 + a);
        let sol = solve_problem(&[slot], &[borne], Options::default()).unwrap();
        assert!(!sol.stalled && sol.values[0] == 0.0, "{sol:?}");
    }

    /// C6 (#313) : un jacobien NaN échappait au pli `max` (qui ignore NaN) et
    /// le solveur concluait « convergé ». `sqrt(a)` en a = 0 : la différence
    /// centrée lit `sqrt(−h)`.
    #[test]
    fn une_pente_non_finie_est_une_erreur_modele_non_evaluable() {
        let x = xs(5, 0.0, 4.0);
        let y = vec![1.0; 5];
        let term = one_case(&x, &y, f64::sqrt);
        assert_eq!(
            solve_problem(&[Slot::new("a", 0.0)], &[term], Options::default()).unwrap_err(),
            AnalysisError::ModelNotEvaluable
        );
    }
}
