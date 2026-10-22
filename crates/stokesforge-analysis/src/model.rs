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

/// Rapport |moyenne X| / étendue de X au-delà duquel un polynôme contraint
/// (Levenberg-Marquardt, non centré) est signalé (#296). Mesuré : à 10 d'étendue,
/// un décalage de 1e2 reste juste, 1e3 et plus se dégrade.
pub const X_SHIFT_WARN_RATIO: f64 = 100.0;

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
            Model::Exponential => p[0] * libm::exp(-x / p[1]),
            Model::ExponentialOffset => p[0] + p[1] * libm::exp(-x / p[2]),
            Model::Exponential2 => p[0] + p[1] * libm::exp(-x / p[2]) + p[3] * libm::exp(-x / p[4]),
            Model::Logarithm => p[0] + p[1] * libm::log(x),
            Model::Power => p[0] * libm::pow(x, p[1]),
            Model::Gaussian => {
                let u = (x - p[2]) / p[3];
                p[0] + p[1] * libm::exp(-4.0 * std::f64::consts::LN_2 * u * u)
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
                    a.set(i, 1, libm::log(xi));
                }
                _ => return None,
            }
        }
        Some(a)
    }

    /// Décalage de l'axe X pour un ajustement linéaire (#283) : la moyenne des
    /// X pour une droite ou un polynôme (une base de puissances de `x - c` reste
    /// conditionnée quand X est loin de 0), 0 pour les autres modèles.
    pub fn x_shift(&self, x: &[f64]) -> f64 {
        if !matches!(self.model, Model::Linear | Model::Polynomial { .. }) {
            return 0.0;
        }
        let finite: Vec<f64> = x.iter().copied().filter(|v| v.is_finite()).collect();
        if finite.is_empty() {
            return 0.0;
        }
        let m = finite.iter().sum::<f64>() / finite.len() as f64;
        if m.is_finite() {
            m
        } else {
            0.0
        }
    }

    /// Le X est-il si loin de 0, au regard de son étendue, qu'une base de
    /// puissances non centrée perd sa précision (#296) ? Vrai seulement pour
    /// une droite ou un polynôme (les autres modèles ne sont pas centrés) dont
    /// la moyenne des X dépasse [`X_SHIFT_WARN_RATIO`] fois l'étendue.
    pub fn x_shift_is_large(&self, x: &[f64]) -> bool {
        let shift = self.x_shift(x);
        if shift == 0.0 {
            return false;
        }
        let finite = x.iter().copied().filter(|v| v.is_finite());
        let (lo, hi) = finite.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(v), hi.max(v))
        });
        let span = hi - lo;
        span > 0.0 && shift.abs() > X_SHIFT_WARN_RATIO * span
    }

    /// Matrice `T` telle que `coefficients dans l'axe d'origine = T · coefficients
    /// dans l'axe décalé de s` (`x' = x - s`) : développement binomial de
    /// `Σ cⱼ (x - s)ʲ`. `None` si le modèle n'est pas un polynôme ou si `s = 0`.
    pub fn unshift_matrix(&self, s: f64) -> Option<Matrix> {
        if s == 0.0 || !matches!(self.model, Model::Linear | Model::Polynomial { .. }) {
            return None;
        }
        let k = self.params.len();
        let mut t = Matrix::zeros(k, k);
        for j in 0..k {
            // C(j, i) (-s)^(j-i), construit de proche en proche.
            let mut binom = 1.0;
            for i in (0..=j).rev() {
                t.set(i, j, binom * (-s).powi((j - i) as i32));
                binom = binom * i as f64 / (j - i + 1) as f64;
            }
        }
        Some(t)
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
                let shift = self.x_shift(x);
                let xs: Vec<f64> = x.iter().map(|v| v - shift).collect();
                if let Some(a) = self.design_matrix(&xs) {
                    if x.iter().all(|v| self.model != Model::Logarithm || *v > 0.0) {
                        if let Some(ls) = least_squares(&a, y) {
                            return match self.unshift_matrix(shift) {
                                Some(t) => t.mul_vec(&ls.coefficients),
                                None => ls.coefficients,
                            };
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
                        lx.push(libm::log(xv));
                        ly.push(libm::log(yv));
                    }
                }
                if lx.len() >= 2 {
                    let mut a = Matrix::zeros(lx.len(), 2);
                    for (i, v) in lx.iter().enumerate() {
                        a.set(i, 0, 1.0);
                        a.set(i, 1, *v);
                    }
                    if let Some(ls) = least_squares(&a, &ly) {
                        return vec![libm::exp(ls.coefficients[0]), ls.coefficients[1]];
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
            ly.push(libm::log(sign * d));
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
            let amp = sign * libm::exp(ls.coefficients[0]);
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
        let s = series(|x| 500.0 * libm::exp(-x / 40.0) + 30.0, 0.0, 200.0, 120);
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
        let s = series(|x| 3.0 + 5.0 * libm::exp(-x / 2.0), 0.0, 10.0, 200);
        let m = Model::ExponentialOffset.compile().unwrap();
        let g = m.guess(&s);
        // Un point de départ à un facteur 2 près suffit à Levenberg-Marquardt.
        assert!(g[2] > 1.0 && g[2] < 4.0, "tau initial {}", g[2]);
        assert!(g[1] > 2.5 && g[1] < 10.0, "A initial {}", g[1]);
    }

    #[test]
    fn la_valeur_initiale_d_un_pic_negatif_est_negative() {
        let s = series(
            |x| 10.0 - 4.0 * libm::exp(-(x - 5.0).powi(2) / 2.0),
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
        let s = series(|x| 2.0 * libm::pow(x, 1.5), 1.0, 10.0, 50);
        let g = Model::Power.compile().unwrap().guess(&s);
        assert!((g[0] - 2.0).abs() < 1e-9 && (g[1] - 1.5).abs() < 1e-9);
    }

    // ---- #283 : décalage de X et matrice de ré-expression des coefficients ----

    fn poly(degree: u32) -> CompiledModel {
        Model::Polynomial { degree }.compile().unwrap()
    }

    /// `Σ cⱼ (x - s)ʲ` développé par multiplications successives (indépendant
    /// de `unshift_matrix`) : coefficients en puissances de `x`.
    fn developpe(c: &[f64], s: f64) -> Vec<f64> {
        let mut out = vec![0.0; c.len()];
        // pow = (x - s)ʲ, coefficients en x.
        let mut pow = vec![1.0];
        for cj in c {
            for (i, p) in pow.iter().enumerate() {
                out[i] += cj * p;
            }
            let mut next = vec![0.0; pow.len() + 1];
            for (i, p) in pow.iter().enumerate() {
                next[i + 1] += p;
                next[i] -= s * p;
            }
            pow = next;
        }
        out
    }

    #[test]
    fn x_shift_est_la_moyenne_pour_la_droite_et_le_polynome() {
        let x = [1.0, 2.0, 6.0];
        assert_eq!(Model::Linear.compile().unwrap().x_shift(&x), 3.0);
        for d in [0, 1, 2, 5] {
            assert_eq!(poly(d).x_shift(&x), 3.0, "degré {d}");
        }
        // La moyenne, ni le minimum ni le milieu de plage (3,5).
        let x = [10.0, 10.0, 10.0, 13.0];
        assert_eq!(poly(2).x_shift(&x), 10.75);
    }

    #[test]
    fn x_shift_est_nul_pour_les_autres_modeles() {
        let x = [1e5, 1e5 + 1.0, 1e5 + 2.0];
        for m in [
            Model::Logarithm,
            Model::Power,
            Model::Exponential,
            Model::ExponentialOffset,
            Model::Exponential2,
            Model::Gaussian,
            Model::Lorentzian,
            Model::Custom {
                formula: "a*x+b".into(),
            },
        ] {
            assert_eq!(m.compile().unwrap().x_shift(&x), 0.0, "{m:?}");
        }
    }

    #[test]
    fn x_shift_ignore_les_x_non_finis_et_se_replie_sur_zero() {
        let p = poly(2);
        assert_eq!(p.x_shift(&[2.0, f64::NAN, 4.0, f64::INFINITY]), 3.0);
        assert_eq!(p.x_shift(&[]), 0.0);
        assert_eq!(p.x_shift(&[f64::NAN, f64::NEG_INFINITY]), 0.0);
        // Somme qui déborde : moyenne non finie, repli sur 0 (jamais inf).
        assert_eq!(p.x_shift(&[f64::MAX, f64::MAX]), 0.0);
    }

    #[test]
    fn unshift_matrix_absente_sans_decalage_ou_hors_polynome() {
        assert!(poly(3).unshift_matrix(0.0).is_none());
        assert!(poly(3).unshift_matrix(-0.0).is_none());
        assert!(Model::Linear
            .compile()
            .unwrap()
            .unshift_matrix(0.0)
            .is_none());
        for m in [Model::Logarithm, Model::Gaussian, Model::Exponential] {
            assert!(m.compile().unwrap().unshift_matrix(2.0).is_none(), "{m:?}");
        }
    }

    #[test]
    fn unshift_matrix_degre_0_est_l_identite_1x1() {
        let t = poly(0).unshift_matrix(7.5).unwrap();
        assert_eq!((t.rows(), t.cols()), (1, 1));
        assert_eq!(t.get(0, 0), 1.0);
    }

    #[test]
    fn unshift_matrix_droite_et_degre_1_coincident() {
        // y = c0 + c1 (x - s) = (c0 - s c1) + c1 x.
        let want = [[1.0, -3.0], [0.0, 1.0]];
        for m in [Model::Linear, Model::Polynomial { degree: 1 }] {
            let t = m.compile().unwrap().unshift_matrix(3.0).unwrap();
            assert_eq!((t.rows(), t.cols()), (2, 2));
            for (i, row) in want.iter().enumerate() {
                for (j, w) in row.iter().enumerate() {
                    assert_eq!(t.get(i, j), *w, "{m:?} ({i},{j})");
                }
            }
        }
    }

    #[test]
    fn unshift_matrix_degre_2_developpe_le_carre() {
        // (x-2)² = x² - 4x + 4 : colonne 2 = [4, -4, 1]. Colonne 1 : x-2.
        let t = poly(2).unshift_matrix(2.0).unwrap();
        let want = [[1.0, -2.0, 4.0], [0.0, 1.0, -4.0], [0.0, 0.0, 1.0]];
        for (i, row) in want.iter().enumerate() {
            for (j, w) in row.iter().enumerate() {
                assert_eq!(t.get(i, j), *w, "({i},{j})");
            }
        }
    }

    #[test]
    fn unshift_matrix_degre_3_developpe_le_cube() {
        // (x-3)³ = x³ - 9x² + 27x - 27.
        let t = poly(3).unshift_matrix(3.0).unwrap();
        let col3 = [-27.0, 27.0, -9.0, 1.0];
        for (i, w) in col3.iter().enumerate() {
            assert_eq!(t.get(i, 3), *w, "ligne {i}");
        }
        // (x-3)² = x² - 6x + 9.
        let col2 = [9.0, -6.0, 1.0, 0.0];
        for (i, w) in col2.iter().enumerate() {
            assert_eq!(t.get(i, 2), *w, "ligne {i}");
        }
    }

    #[test]
    fn unshift_matrix_degre_5_developpe_la_puissance_5() {
        // (x-1)⁵ : coefficients binomiaux alternés.
        let t = poly(5).unshift_matrix(1.0).unwrap();
        let col5 = [-1.0, 5.0, -10.0, 10.0, -5.0, 1.0];
        for (i, w) in col5.iter().enumerate() {
            assert_eq!(t.get(i, 5), *w, "ligne {i}");
        }
        // (x-2)⁵ : 5·(-2)⁴ = 80 devant x, -32 en constante.
        let t = poly(5).unshift_matrix(2.0).unwrap();
        let col5 = [-32.0, 80.0, -80.0, 40.0, -10.0, 1.0];
        for (i, w) in col5.iter().enumerate() {
            assert_eq!(t.get(i, 5), *w, "ligne {i}");
        }
    }

    #[test]
    fn unshift_matrix_est_triangulaire_superieure_a_diagonale_unite() {
        for d in 0..=6u32 {
            let t = poly(d).unshift_matrix(-1.5).unwrap();
            let k = (d + 1) as usize;
            assert_eq!((t.rows(), t.cols()), (k, k));
            for i in 0..k {
                assert_eq!(t.get(i, i), 1.0, "degré {d} diagonale {i}");
                for j in 0..i {
                    assert_eq!(t.get(i, j), 0.0, "degré {d} ({i},{j})");
                }
            }
        }
    }

    #[test]
    fn unshift_matrix_applique_a_un_polynome_connu() {
        // 1 + 2(x-2) + 3(x-2)² = 3x² - 10x + 9.
        let t = poly(2).unshift_matrix(2.0).unwrap();
        assert_eq!(t.mul_vec(&[1.0, 2.0, 3.0]), vec![9.0, -10.0, 3.0]);
        // Contre un développement indépendant, pour tous les degrés.
        for d in 0..=5u32 {
            let c: Vec<f64> = (0..=d).map(|j| 1.0 + 0.5 * f64::from(j)).collect();
            for s in [2.0, -3.0, 0.25] {
                let t = poly(d).unshift_matrix(s).unwrap();
                let got = t.mul_vec(&c);
                let want = developpe(&c, s);
                for (g, w) in got.iter().zip(&want) {
                    assert!((g - w).abs() <= 1e-12 * (1.0 + w.abs()), "d={d} s={s}");
                }
            }
        }
    }

    #[test]
    fn unshift_matrix_conserve_la_valeur_du_polynome() {
        // eval(T·c, x) == eval(c, x - s) : la matrice change d'axe, pas de courbe.
        for d in 0..=5u32 {
            let m = poly(d);
            let c: Vec<f64> = (0..=d).map(|j| libm::cos(f64::from(j) * 1.3)).collect();
            let s = 2.5;
            let a = m.unshift_matrix(s).unwrap().mul_vec(&c);
            for x in [-3.0, 0.0, 1.0, 4.5, 10.0] {
                let (g, w) = (m.eval(&a, x), m.eval(&c, x - s));
                assert!((g - w).abs() <= 1e-9 * (1.0 + w.abs()), "d={d} x={x}");
            }
        }
    }

    #[test]
    fn unshift_matrix_aller_retour_avec_le_decalage_oppose() {
        // Décaler de s puis de -s ramène les coefficients de départ.
        let m = poly(4);
        let c = [1.0, -2.0, 0.5, 3.0, -1.0];
        let a = m.unshift_matrix(1.75).unwrap().mul_vec(&c);
        let back = m.unshift_matrix(-1.75).unwrap().mul_vec(&a);
        for (b, w) in back.iter().zip(c) {
            assert!((b - w).abs() < 1e-12, "{b} vs {w}");
        }
    }

    #[test]
    fn la_covariance_de_la_droite_se_propage_par_t_cov_tt() {
        // Droite centrée : colonnes orthogonales, cov = diag(1/n, 1/sxx). Après
        // le décalage s : var(a0) = 1/n + s²/sxx, cov(a0,a1) = -s/sxx,
        // var(a1) = 1/sxx (propagation à la main de T·cov·Tᵀ).
        let (n, sxx, s) = (10.0_f64, 82.5_f64, 4.0_f64);
        let mut cov = Matrix::zeros(2, 2);
        cov.set(0, 0, 1.0 / n);
        cov.set(1, 1, 1.0 / sxx);
        let t = Model::Linear.compile().unwrap().unshift_matrix(s).unwrap();
        let p = t.mul(&cov).mul(&t.transpose());
        assert!((p.get(0, 0) - (1.0 / n + s * s / sxx)).abs() < 1e-15);
        assert!((p.get(0, 1) + s / sxx).abs() < 1e-15);
        assert!((p.get(1, 0) + s / sxx).abs() < 1e-15);
        assert!((p.get(1, 1) - 1.0 / sxx).abs() < 1e-15);
    }
}
