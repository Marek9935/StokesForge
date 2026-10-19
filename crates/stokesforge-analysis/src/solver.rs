//! Moteur de moindres carrés non linéaires (Levenberg-Marquardt) à **cases de
//! paramètres** (« slots »), commun à l'ajustement, à l'ajustement global, à la
//! décomposition en pics et à la reconvolution (#188, #189, #190).
//!
//! Un problème est une liste de **termes** (une courbe mesurée et une fonction
//! qui la modélise) qui lisent un même vecteur de cases. Une case partagée par
//! plusieurs termes, c'est un paramètre global (M13 b) ; une case lue par deux
//! pics, c'est une largeur liée (#189). Le moteur ne connaît ni modèle ni pic.
//!
//! Dérivées par différences centrées (pas relatif au paramètre) : les modèles
//! libres n'ont pas de dérivée analytique, et un seul chemin de calcul vaut
//! mieux que deux (modèle intégré, modèle libre). Les bornes sont tenues par
//! projection du pas.

use crate::error::AnalysisError;
use crate::linalg::{invert, solve, Matrix};

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
}

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

    let mut lambda = 1e-3;
    let mut iterations = 0;
    let mut converged = false;
    let mut jac = Matrix::zeros(m, free.len());
    while iterations < opts.max_iterations {
        iterations += 1;
        jacobian(terms, &values, slots, &free, &mut jac, &mut buf);
        let a = jac.gram();
        let g = jac.transpose_mul(&r);
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
        if residual_norm == 0.0 || cosine < 1e-9 {
            converged = true;
            break;
        }
        let mut improved = false;
        for _ in 0..40 {
            let mut damped = a.clone();
            for i in 0..free.len() {
                // Marquardt : l'amortissement suit la diagonale de chaque paramètre,
                // sans plancher absolu (les colonnes du jacobien peuvent différer de
                // vingt ordres de grandeur) ; une colonne nulle prend 1.
                let d = if a.get(i, i) > 0.0 { a.get(i, i) } else { 1.0 };
                damped.add(i, i, lambda * d);
            }
            let rhs: Vec<f64> = g.iter().map(|v| -v).collect();
            let Some(step) = solve(&damped, &rhs) else {
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
                lambda = (lambda / 3.0).max(1e-12);
                improved = true;
                if drop <= 1e-14 * chi2.max(1e-300) || moved < 1e-11 {
                    converged = true;
                }
                break;
            }
            lambda *= 5.0;
            if lambda > 1e14 {
                break;
            }
        }
        if !improved {
            // Plus aucun pas ne fait baisser le χ² : minimum local atteint.
            converged = true;
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
    Ok(Solution {
        values,
        errors,
        undetermined,
        chi2,
        dof,
        reduced_chi2,
        iterations,
        terms: stats,
    })
}

/// Pour chaque colonne du jacobien, `true` si les données la déterminent : sa
/// part non portée par les colonnes précédentes (Gram-Schmidt modifié) dépasse
/// un millionième de sa norme. Une colonne nulle (paramètre sans effet) ou
/// combinaison des précédentes (X tous égaux pour une droite) est écartée.
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
        if rest > 1e-6 * norm {
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
        let h = 1e-6
            * values[i]
                .abs()
                .max(1e-2 * pmax)
                .max(if pmax > 0.0 { 0.0 } else { 1e-9 });
        // Le pas ne sort pas des bornes : à une borne, différence décentrée.
        let hi = (values[i] + h).min(slots[i].max);
        let lo = (values[i] - h).max(slots[i].min);
        work[i] = hi;
        residuals(terms, &work, buf, &mut plus);
        work[i] = lo;
        residuals(terms, &work, buf, &mut minus);
        work[i] = values[i];
        let den = hi - lo;
        for (row, (p, mn)) in plus.iter().zip(&minus).enumerate() {
            let d = if den > 0.0 { (p - mn) / den } else { 0.0 };
            jac.set(row, k, if d.is_finite() { d } else { 0.0 });
        }
    }
}

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
        let y: Vec<f64> = x.iter().map(|v| 1.0 + 4.0 * (-v / 2.5).exp()).collect();
        let term = Term {
            x: &x,
            y: &y,
            sigma: None,
            eval: Box::new(|p, out| {
                for (o, v) in out.iter_mut().zip(&x) {
                    *o = p[0] + p[1] * (-v / p[2]).exp();
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
}
