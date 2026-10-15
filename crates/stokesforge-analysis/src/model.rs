//! Les modèles d'ajustement : formes, paramètres, valeurs initiales
//! automatiques.
//!
//! Chaque modèle liste ses paramètres **par nom stable** (`tau`, `xc`, …) :
//! ces noms sont les clés des résultats, que l'interface met en libellés
//! traduits. Un modèle libre (`Custom`) tire les siens de l'expression.

use crate::error::AnalysisError;
use crate::expr::Expr;
use crate::linalg::{least_squares, Matrix};
use crate::ops::MAX_DEGREE;
use crate::series::Series;

/// Un modèle de courbe `y = f(x; p)`.
#[derive(Debug, Clone, PartialEq)]
pub enum Model {
    /// `a + b·x` — paramètres `intercept`, `slope`.
    Linear,
    /// `Σ cₖ·xᵏ`, `k = 0..=degree` — paramètres `c0`, `c1`, …
    Polynomial { degree: u32 },
    /// `A·exp(−x/τ)` — `A`, `tau`.
    Exponential,
    /// `y0 + A·exp(−x/τ)` — `y0`, `A`, `tau`.
    ExponentialOffset,
    /// `y0 + A₁·exp(−x/τ₁) + A₂·exp(−x/τ₂)` — `y0`, `A1`, `tau1`, `A2`, `tau2`.
    Exponential2,
    /// `a + b·ln(x)` — `a`, `b`.
    Logarithm,
    /// `a·x^b` — `a`, `b`.
    Power,
    /// `y0 + A·exp(−4 ln2·(x−xc)²/w²)` — `y0`, `A`, `xc`, `fwhm`.
    Gaussian,
    /// `y0 + A / (1 + 4(x−xc)²/w²)` — `y0`, `A`, `xc`, `fwhm`.
    Lorentzian,
    /// Expression saisie, en `x` et paramètres libres (voir [`crate::expr`]).
    Custom { formula: String },
}

/// Un paramètre d'un modèle : son nom et ses bornes par défaut.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamDef {
    pub name: String,
    pub min: f64,
    pub max: f64,
}

impl ParamDef {
    fn free(name: &str) -> Self {
        Self {
            name: name.to_string(),
            min: f64::NEG_INFINITY,
            max: f64::INFINITY,
        }
    }

    fn positive(name: &str) -> Self {
        Self {
            name: name.to_string(),
            min: 0.0,
            max: f64::INFINITY,
        }
    }
}

/// Un modèle prêt à être évalué : l'expression d'un modèle libre est compilée
/// une fois.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledModel {
    model: Model,
    expr: Option<Expr>,
    params: Vec<ParamDef>,
}

impl Model {
    /// Compile le modèle. Un modèle libre peut échouer (syntaxe), un polynôme
    /// au-delà de [`MAX_DEGREE`] aussi : `degree + 1` paramètres nommés seraient
    /// fabriqués avant que `Operation::validate` ne puisse le refuser (un degré
    /// lu tel quel des préférences ou d'un fichier).
    pub fn compile(&self) -> Result<CompiledModel, AnalysisError> {
        let (expr, params) = match self {
            Model::Linear => (
                None,
                vec![ParamDef::free("intercept"), ParamDef::free("slope")],
            ),
            Model::Polynomial { degree } if *degree > MAX_DEGREE => {
                return Err(AnalysisError::InvalidParameter {
                    name: "degree".into(),
                });
            }
            Model::Polynomial { degree } => (
                None,
                (0..=*degree)
                    .map(|k| ParamDef::free(&format!("c{k}")))
                    .collect(),
            ),
            Model::Exponential => (None, vec![ParamDef::free("A"), ParamDef::positive("tau")]),
            Model::ExponentialOffset => (
                None,
                vec![
                    ParamDef::free("y0"),
                    ParamDef::free("A"),
                    ParamDef::positive("tau"),
                ],
            ),
            Model::Exponential2 => (
                None,
                vec![
                    ParamDef::free("y0"),
                    ParamDef::free("A1"),
                    ParamDef::positive("tau1"),
                    ParamDef::free("A2"),
                    ParamDef::positive("tau2"),
                ],
            ),
            Model::Logarithm => (None, vec![ParamDef::free("a"), ParamDef::free("b")]),
            Model::Power => (None, vec![ParamDef::free("a"), ParamDef::free("b")]),
            Model::Gaussian | Model::Lorentzian => (
                None,
                vec![
                    ParamDef::free("y0"),
                    ParamDef::free("A"),
                    ParamDef::free("xc"),
                    ParamDef::positive("fwhm"),
                ],
            ),
            Model::Custom { formula } => {
                let e = Expr::parse(formula).map_err(AnalysisError::BadExpression)?;
                let params = e.params().iter().map(|n| ParamDef::free(n)).collect();
                (Some(e), params)
            }
        };
        Ok(CompiledModel {
            model: self.clone(),
            expr,
            params,
        })
    }

    /// Le modèle est linéaire en ses paramètres : moindres carrés exacts.
    pub fn is_linear_in_params(&self) -> bool {
        matches!(
            self,
            Model::Linear | Model::Polynomial { .. } | Model::Logarithm
        )
    }
}

impl CompiledModel {
    pub fn model(&self) -> &Model {
        &self.model
    }

    pub fn params(&self) -> &[ParamDef] {
        &self.params
    }

    pub fn param_count(&self) -> usize {
        self.params.len()
    }

    /// `f(x; p)`. Un domaine invalide donne `NaN`.
    pub fn eval(&self, p: &[f64], x: f64) -> f64 {
        match &self.model {
            Model::Linear => p[0] + p[1] * x,
            Model::Polynomial { .. } => p.iter().rev().fold(0.0, |acc, c| acc * x + c),
            Model::Exponential => p[0] * (-x / p[1]).exp(),
            Model::ExponentialOffset => p[0] + p[1] * (-x / p[2]).exp(),
            Model::Exponential2 => p[0] + p[1] * (-x / p[2]).exp() + p[3] * (-x / p[4]).exp(),
            Model::Logarithm => p[0] + p[1] * x.ln(),
            Model::Power => p[0] * x.powf(p[1]),
            Model::Gaussian => {
                let u = (x - p[2]) / p[3];
                p[0] + p[1] * (-4.0 * std::f64::consts::LN_2 * u * u).exp()
            }
            Model::Lorentzian => {
                let u = (x - p[2]) / p[3];
                p[0] + p[1] / (1.0 + 4.0 * u * u)
            }
            Model::Custom { .. } => self.expr.as_ref().map_or(f64::NAN, |e| e.eval(x, p)),
        }
    }

    /// Colonnes de la matrice de conception d'un modèle linéaire en ses
    /// paramètres (`None` sinon) : `A[i][k] = ∂f/∂pₖ (xᵢ)`.
    pub fn design_matrix(&self, x: &[f64]) -> Option<Matrix> {
        let k = self.params.len();
        let mut a = Matrix::zeros(x.len(), k);
        for (i, &xi) in x.iter().enumerate() {
            match &self.model {
                Model::Linear => {
                    a.set(i, 0, 1.0);
                    a.set(i, 1, xi);
                }
                Model::Polynomial { .. } => {
                    let mut v = 1.0;
                    for j in 0..k {
                        a.set(i, j, v);
                        v *= xi;
                    }
                }
                Model::Logarithm => {
                    a.set(i, 0, 1.0);
                    a.set(i, 1, xi.ln());
                }
                _ => return None,
            }
        }
        Some(a)
    }

    /// Valeurs initiales automatiques d'après les données (jamais `NaN`).
    pub fn guess(&self, s: &Series) -> Vec<f64> {
        let n = s.len();
        if n == 0 {
            return vec![1.0; self.params.len()];
        }
        let (x, y) = (&s.x[..], &s.y[..]);
        let span = (x.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            - x.iter().cloned().fold(f64::INFINITY, f64::min))
        .abs()
        .max(f64::MIN_POSITIVE);
        let first = s.sorted();
        let y_last = first.y[n - 1];
        match &self.model {
            Model::Linear | Model::Polynomial { .. } | Model::Logarithm => {
                // Linéaire en les paramètres : le point de départ importe peu.
                if let Some(a) = self.design_matrix(x) {
                    if x.iter().all(|v| self.model != Model::Logarithm || *v > 0.0) {
                        if let Some(ls) = least_squares(&a, y) {
                            return ls.coefficients;
                        }
                    }
                }
                vec![0.0; self.params.len()]
            }
            Model::Exponential => {
                let (a, tau) = log_linear_decay(&first.x, &first.y, 0.0, span);
                vec![a, tau]
            }
            Model::ExponentialOffset => {
                let y0 = y_last;
                let (a, tau) = log_linear_decay(&first.x, &first.y, y0, span);
                vec![y0, a, tau]
            }
            Model::Exponential2 => {
                let y0 = y_last;
                let (a, tau) = log_linear_decay(&first.x, &first.y, y0, span);
                vec![y0, a / 2.0, tau / 3.0, a / 2.0, tau * 3.0]
            }
            Model::Power => {
                // ln y = ln a + b ln x, sur les points strictement positifs.
                let (mut lx, mut ly) = (Vec::new(), Vec::new());
                for (&xv, &yv) in x.iter().zip(y) {
                    if xv > 0.0 && yv > 0.0 {
                        lx.push(xv.ln());
                        ly.push(yv.ln());
                    }
                }
                if lx.len() >= 2 {
                    let mut a = Matrix::zeros(lx.len(), 2);
                    for (i, v) in lx.iter().enumerate() {
                        a.set(i, 0, 1.0);
                        a.set(i, 1, *v);
                    }
                    if let Some(ls) = least_squares(&a, &ly) {
                        return vec![ls.coefficients[0].exp(), ls.coefficients[1]];
                    }
                }
                vec![1.0, 1.0]
            }
            Model::Gaussian | Model::Lorentzian => {
                let g = peak_guess(x, y);
                vec![g.y0, g.height, g.center, g.fwhm]
            }
            Model::Custom { .. } => self.guess_free(x, y),
        }
    }

    /// Départ d'un modèle libre (#204, bouton « Auto ») : tous les paramètres à 1,
    /// puis deux passes qui testent, un paramètre à la fois, une grille de
    /// valeurs (0 et ±10ᵏ × l'échelle de 1, de Y, de X, de 1/X) et gardent
    /// celle qui fait baisser la somme des carrés. Jamais pire que « tout à 1 » ;
    /// sur au plus 400 points régulièrement espacés pour rester instantané.
    fn guess_free(&self, x: &[f64], y: &[f64]) -> Vec<f64> {
        let k = self.params.len();
        let mut p = vec![1.0; k];
        let stride = x.len().div_ceil(400).max(1);
        let pts: Vec<(f64, f64)> = x
            .iter()
            .zip(y)
            .step_by(stride)
            .map(|(&a, &b)| (a, b))
            .collect();
        let sse = |p: &[f64]| -> f64 {
            let v: f64 = pts
                .iter()
                .map(|&(xi, yi)| (yi - self.eval(p, xi)).powi(2))
                .sum();
            if v.is_finite() {
                v
            } else {
                f64::INFINITY
            }
        };
        let magnitude = |it: &mut dyn Iterator<Item = f64>| {
            let m = it.fold(0.0_f64, |a, v| a.max(v.abs()));
            if m.is_finite() && m > 0.0 {
                m
            } else {
                1.0
            }
        };
        let (ys, xs) = (
            magnitude(&mut y.iter().copied()),
            magnitude(&mut x.iter().copied()),
        );
        let mut candidates = vec![0.0];
        for scale in [1.0, ys, xs, 1.0 / xs] {
            for e in -2..=2 {
                let v = scale * 10f64.powi(e);
                candidates.push(v);
                candidates.push(-v);
            }
        }
        let mut best = sse(&p);
        for _ in 0..2 {
            for j in 0..k {
                for &c in &candidates {
                    let old = p[j];
                    p[j] = c;
                    let s = sse(&p);
                    if s < best {
                        best = s;
                    } else {
                        p[j] = old;
                    }
                }
            }
        }
        p
    }
}

/// Estimation `(A, τ)` de `A·exp(−x/τ)` sur `y − y0` par régression de `ln|y−y0|`
/// sur les points du signe dominant. Repli : `A = y₀ − y0`, `τ = étendue/3`.
fn log_linear_decay(x: &[f64], y: &[f64], y0: f64, span: f64) -> (f64, f64) {
    let shifted: Vec<f64> = y.iter().map(|v| v - y0).collect();
    let pos: f64 = shifted.iter().filter(|v| **v > 0.0).map(|v| v.abs()).sum();
    let neg: f64 = shifted.iter().filter(|v| **v < 0.0).map(|v| v.abs()).sum();
    let sign = if pos >= neg { 1.0 } else { -1.0 };
    let (mut lx, mut ly) = (Vec::new(), Vec::new());
    for (&xv, &d) in x.iter().zip(&shifted) {
        if sign * d > 0.0 {
            lx.push(xv);
            ly.push((sign * d).ln());
        }
    }
    let fallback = (shifted.first().copied().unwrap_or(1.0), span / 3.0);
    if lx.len() < 2 {
        return fallback;
    }
    let mut a = Matrix::zeros(lx.len(), 2);
    for (i, v) in lx.iter().enumerate() {
        a.set(i, 0, 1.0);
        a.set(i, 1, *v);
    }
    match least_squares(&a, &ly) {
        Some(ls) if ls.coefficients[1].abs() > 1e-300 => {
            let tau = -1.0 / ls.coefficients[1];
            let amp = sign * ls.coefficients[0].exp();
            if amp.is_finite() && tau.is_finite() && tau > 0.0 {
                (amp, tau)
            } else {
                fallback
            }
        }
        _ => fallback,
    }
}

/// Estimation grossière d'un pic isolé.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeakGuess {
    pub y0: f64,
    pub height: f64,
    pub center: f64,
    pub fwhm: f64,
}

/// Pic dominant de `(x, y)` : fond (10e centile côté du pic), hauteur signée,
/// centre, largeur à mi-hauteur lue par interpolation.
pub fn peak_guess(x: &[f64], y: &[f64]) -> PeakGuess {
    let n = x.len().min(y.len());
    let span = if n > 1 {
        let (lo, hi) = x[..n]
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| {
                (l.min(*v), h.max(*v))
            });
        (hi - lo).abs()
    } else {
        1.0
    };
    if n == 0 {
        return PeakGuess {
            y0: 0.0,
            height: 1.0,
            center: 0.0,
            fwhm: 1.0,
        };
    }
    let mut sorted: Vec<f64> = y[..n].to_vec();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[n / 2];
    let (imax, ymax) =
        y[..n].iter().enumerate().fold(
            (0, f64::NEG_INFINITY),
            |b, (i, v)| if *v > b.1 { (i, *v) } else { b },
        );
    let (imin, ymin) =
        y[..n].iter().enumerate().fold(
            (0, f64::INFINITY),
            |b, (i, v)| if *v < b.1 { (i, *v) } else { b },
        );
    let positive = (ymax - median).abs() >= (ymin - median).abs();
    let (ipeak, ytop) = if positive { (imax, ymax) } else { (imin, ymin) };
    let y0 = if positive {
        sorted[n / 10]
    } else {
        sorted[n - 1 - n / 10]
    };
    let height = ytop - y0;
    let half = y0 + height / 2.0;
    // Traversées du niveau mi-hauteur de part et d'autre du sommet.
    let crossing = |dir: i64| -> Option<f64> {
        let mut i = ipeak as i64;
        while i + dir >= 0 && (i + dir) < n as i64 {
            let (a, b) = (i as usize, (i + dir) as usize);
            let above_a = (y[a] - half) * height.signum() >= 0.0;
            let above_b = (y[b] - half) * height.signum() >= 0.0;
            if above_a && !above_b {
                let t = (half - y[a]) / (y[b] - y[a]);
                return Some(x[a] + t * (x[b] - x[a]));
            }
            i += dir;
        }
        None
    };
    let fwhm = match (crossing(-1), crossing(1)) {
        (Some(l), Some(r)) => (r - l).abs(),
        (Some(l), None) => 2.0 * (x[ipeak] - l).abs(),
        (None, Some(r)) => 2.0 * (r - x[ipeak]).abs(),
        (None, None) => span / 4.0,
    };
    PeakGuess {
        y0,
        height,
        center: x[ipeak],
        fwhm: if fwhm > 0.0 { fwhm } else { span / 4.0 }.max(f64::MIN_POSITIVE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(f: impl Fn(f64) -> f64, from: f64, to: f64, n: usize) -> Series {
        let x: Vec<f64> = (0..n)
            .map(|i| from + (to - from) * i as f64 / (n - 1) as f64)
            .collect();
        let y = x.iter().map(|&v| f(v)).collect();
        Series::new(x, y)
    }

    /// Le départ d'un modèle libre est meilleur que « tout à 1 » quand les
    /// données sont loin de cette échelle, et reste fini (#204).
    #[test]
    fn le_depart_d_un_modele_libre_suit_l_echelle_des_donnees() {
        let model = Model::Custom {
            formula: "a*exp(-x/b) + c".into(),
        }
        .compile()
        .unwrap();
        let s = series(|x| 500.0 * (-x / 40.0).exp() + 30.0, 0.0, 200.0, 120);
        let sse = |p: &[f64]| -> f64 {
            s.x.iter()
                .zip(&s.y)
                .map(|(&x, &y)| (y - model.eval(p, x)).powi(2))
                .sum()
        };
        let guess = model.guess(&s);
        assert!(guess.iter().all(|v| v.is_finite()));
        assert!(
            sse(&guess) < 0.1 * sse(&[1.0, 1.0, 1.0]),
            "départ {guess:?} : {} contre {}",
            sse(&guess),
            sse(&[1.0, 1.0, 1.0])
        );
    }

    #[test]
    fn les_parametres_de_chaque_modele() {
        let names = |m: Model| -> Vec<String> {
            m.compile()
                .unwrap()
                .params()
                .iter()
                .map(|p| p.name.clone())
                .collect()
        };
        assert_eq!(names(Model::Linear), ["intercept", "slope"]);
        assert_eq!(
            names(Model::Polynomial { degree: 3 }),
            ["c0", "c1", "c2", "c3"]
        );
        assert_eq!(
            names(Model::Exponential2),
            ["y0", "A1", "tau1", "A2", "tau2"]
        );
        assert_eq!(names(Model::Gaussian), ["y0", "A", "xc", "fwhm"]);
        assert_eq!(
            names(Model::Custom {
                formula: "a*exp(-x/b)+c".into()
            }),
            ["a", "b", "c"]
        );
    }

    #[test]
    fn un_modele_libre_mal_forme_est_une_erreur() {
        assert!(matches!(
            Model::Custom {
                formula: "a +".into()
            }
            .compile(),
            Err(AnalysisError::BadExpression(_))
        ));
    }

    #[test]
    fn evaluation_des_modeles_connus() {
        let g = Model::Gaussian.compile().unwrap();
        // Au centre : y0 + A ; à ±fwhm/2 : y0 + A/2.
        assert!((g.eval(&[1.0, 4.0, 10.0, 2.0], 10.0) - 5.0).abs() < 1e-12);
        assert!((g.eval(&[1.0, 4.0, 10.0, 2.0], 11.0) - 3.0).abs() < 1e-12);
        let l = Model::Lorentzian.compile().unwrap();
        assert!((l.eval(&[0.0, 4.0, 0.0, 2.0], 1.0) - 2.0).abs() < 1e-12);
        let p = Model::Polynomial { degree: 2 }.compile().unwrap();
        assert!((p.eval(&[1.0, 2.0, 3.0], 2.0) - 17.0).abs() < 1e-12);
        let e = Model::ExponentialOffset.compile().unwrap();
        assert!((e.eval(&[1.0, 2.0, 5.0], 5.0) - (1.0 + 2.0 / std::f64::consts::E)).abs() < 1e-12);
    }

    #[test]
    fn la_valeur_initiale_exponentielle_est_proche() {
        let s = series(|x| 3.0 + 5.0 * (-x / 2.0).exp(), 0.0, 10.0, 200);
        let m = Model::ExponentialOffset.compile().unwrap();
        let g = m.guess(&s);
        // Un point de départ à un facteur 2 près suffit à Levenberg-Marquardt.
        assert!(g[2] > 1.0 && g[2] < 4.0, "tau initial {}", g[2]);
        assert!(g[1] > 2.5 && g[1] < 10.0, "A initial {}", g[1]);
    }

    #[test]
    fn la_valeur_initiale_d_un_pic_negatif_est_negative() {
        let s = series(
            |x| 10.0 - 4.0 * (-(x - 5.0).powi(2) / 2.0).exp(),
            0.0,
            10.0,
            201,
        );
        let g = peak_guess(&s.x, &s.y);
        assert!(g.height < -3.0, "hauteur {}", g.height);
        assert!((g.center - 5.0).abs() < 0.1);
        // fwhm = 2·sqrt(2 ln 2)·σ avec σ = 1.
        assert!((g.fwhm - 2.3548).abs() < 0.2, "fwhm {}", g.fwhm);
    }

    #[test]
    fn la_valeur_initiale_de_puissance_vient_du_log_log() {
        let s = series(|x| 2.0 * x.powf(1.5), 1.0, 10.0, 50);
        let g = Model::Power.compile().unwrap().guess(&s);
        assert!((g[0] - 2.0).abs() < 1e-9 && (g[1] - 1.5).abs() < 1e-9);
    }
}
