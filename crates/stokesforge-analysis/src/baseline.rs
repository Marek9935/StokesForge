//! Lignes de base (#185, #211) : constante par un point ou une zone, droite
//! par deux points, polynôme ajusté sur des
//! zones de fond, ou courbe par points d'ancrage (segments ou spline).
//!
//! Ce module ne fabrique que la **fonction** ligne de base ; soustraire ou
//! diviser est [`apply`]. Les points d'ancrage posés « avec aimant et moyenne
//! locale » (M4 b) sont calculés par l'interface avec
//! [`crate::series::local_mean`] ; ici un ancrage est un point `(x, y)`.

use crate::error::AnalysisError;
use crate::linalg::{least_squares, Matrix};
use crate::series::Series;

/// Interpolation entre points d'ancrage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Segments,
    Spline,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BaselineMethod {
    /// Fond plat fixé par un point libre ou aimanté.
    Constant { point: (f64, f64) },
    /// Fond plat : moyenne des Y dans une seule plage inclusive de X.
    ConstantZone { zone: (f64, f64) },
    /// Droite passant par deux points.
    Line { p1: (f64, f64), p2: (f64, f64) },
    /// Polynôme de degré `degree` ajusté par moindres carrés sur les points de
    /// la série dont X tombe dans une des `zones` ; `degree = 1` est la droite.
    Zones { zones: Vec<(f64, f64)>, degree: u32 },
    /// Courbe passant par les points d'ancrage.
    Anchors {
        points: Vec<(f64, f64)>,
        interpolation: Interpolation,
    },
}

/// Ce qu'on fait de la ligne de base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineOp {
    Subtract,
    Divide,
}

/// Une ligne de base évaluable en tout X.
#[derive(Debug, Clone, PartialEq)]
pub struct Baseline {
    shape: Shape,
    /// Nombre de points qui ont servi à la déterminer (zones), sinon le nombre
    /// d'ancrages.
    pub points_used: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum Shape {
    /// Polynôme en `(x − center)/scale`, coefficients par puissances croissantes.
    Poly {
        coeffs: Vec<f64>,
        center: f64,
        scale: f64,
    },
    /// Segments : points triés par X.
    Segments(Vec<(f64, f64)>),
    /// Spline cubique naturelle : points triés, dérivées secondes.
    Spline {
        points: Vec<(f64, f64)>,
        second: Vec<f64>,
    },
}

impl Baseline {
    /// Valeur de la ligne de base en `x`.
    pub fn eval(&self, x: f64) -> f64 {
        match &self.shape {
            Shape::Poly {
                coeffs,
                center,
                scale,
            } => {
                let t = (x - center) / scale;
                coeffs.iter().rev().fold(0.0, |acc, c| acc * t + c)
            }
            Shape::Segments(pts) => eval_segments(pts, x),
            Shape::Spline { points, second } => eval_spline(points, second, x),
        }
    }

    /// Pente et ordonnée à l'origine, si la ligne de base est une droite.
    pub fn as_line(&self) -> Option<(f64, f64)> {
        match &self.shape {
            Shape::Poly {
                coeffs,
                center,
                scale,
            } if coeffs.len() <= 2 => {
                let c1 = coeffs.get(1).copied().unwrap_or(0.0) / scale;
                let intercept = coeffs[0] - c1 * center;
                Some((c1, intercept))
            }
            _ => None,
        }
    }
}

fn eval_segments(pts: &[(f64, f64)], x: f64) -> f64 {
    let n = pts.len();
    // Prolongement linéaire des segments d'extrémité.
    let i = pts.partition_point(|p| p.0 < x).clamp(1, n - 1);
    let (x0, y0) = pts[i - 1];
    let (x1, y1) = pts[i];
    y0 + (y1 - y0) * (x - x0) / (x1 - x0)
}

fn spline_second_derivatives(pts: &[(f64, f64)]) -> Vec<f64> {
    let n = pts.len();
    let mut second = vec![0.0; n];
    if n < 3 {
        return second;
    }
    // Système tridiagonal de la spline naturelle (algorithme de Thomas).
    let mut c = vec![0.0; n];
    let mut d = vec![0.0; n];
    for i in 1..n - 1 {
        let h0 = pts[i].0 - pts[i - 1].0;
        let h1 = pts[i + 1].0 - pts[i].0;
        let sig = h0 / (h0 + h1);
        let p = sig * c[i - 1] + 2.0;
        c[i] = (sig - 1.0) / p;
        let dd = (pts[i + 1].1 - pts[i].1) / h1 - (pts[i].1 - pts[i - 1].1) / h0;
        d[i] = (6.0 * dd / (h0 + h1) - sig * d[i - 1]) / p;
    }
    for i in (1..n - 1).rev() {
        second[i] = c[i] * second[i + 1] + d[i];
    }
    second
}

fn eval_spline(pts: &[(f64, f64)], second: &[f64], x: f64) -> f64 {
    let n = pts.len();
    if n == 2 {
        return eval_segments(pts, x);
    }
    if x < pts[0].0 {
        // Prolongement par la tangente de l'extrémité (dérivée seconde nulle).
        let h = pts[1].0 - pts[0].0;
        let slope = (pts[1].1 - pts[0].1) / h - h * (2.0 * second[0] + second[1]) / 6.0;
        return pts[0].1 + slope * (x - pts[0].0);
    }
    if x > pts[n - 1].0 {
        let h = pts[n - 1].0 - pts[n - 2].0;
        let slope =
            (pts[n - 1].1 - pts[n - 2].1) / h + h * (second[n - 2] + 2.0 * second[n - 1]) / 6.0;
        return pts[n - 1].1 + slope * (x - pts[n - 1].0);
    }
    let i = pts.partition_point(|p| p.0 < x).clamp(1, n - 1);
    let (x0, y0) = pts[i - 1];
    let (x1, y1) = pts[i];
    let h = x1 - x0;
    let a = (x1 - x) / h;
    let b = (x - x0) / h;
    a * y0 + b * y1 + ((a * a * a - a) * second[i - 1] + (b * b * b - b) * second[i]) * h * h / 6.0
}

impl BaselineMethod {
    /// Refuse une coordonnée non finie (NaN, ±inf) posée par l'utilisateur :
    /// `f64::min/max` et les comparaisons l'ignoreraient en silence (une borne
    /// de zone NaN deviendrait la zone `[b, b]`). Source unique de la règle,
    /// appelée par [`build`] et par `Operation::validate` ; une méthode qui
    /// porte des coordonnées s'ajoute ici.
    pub(crate) fn check_finite(&self) -> Result<(), AnalysisError> {
        let (name, finite) = match self {
            BaselineMethod::Constant { point } => ("point", pair_finite(*point)),
            BaselineMethod::Line { p1, p2 } => ("points", pair_finite(*p1) && pair_finite(*p2)),
            BaselineMethod::ConstantZone { zone } => ("zone", pair_finite(*zone)),
            BaselineMethod::Zones { zones, .. } => ("zones", zones.iter().all(|z| pair_finite(*z))),
            // Une ancre non posée (non finie) est ignorée par `build`, comme par
            // `readiness` (#298) : jamais une erreur.
            BaselineMethod::Anchors { .. } => ("points", true),
        };
        if finite {
            Ok(())
        } else {
            Err(AnalysisError::InvalidParameter { name: name.into() })
        }
    }
}

fn pair_finite(pair: (f64, f64)) -> bool {
    pair.0.is_finite() && pair.1.is_finite()
}

/// Fabrique la ligne de base de `method`. `series` ne sert qu'à la méthode par
/// zones (les points de fond).
pub fn build(method: &BaselineMethod, series: &Series) -> Result<Baseline, AnalysisError> {
    method.check_finite()?;
    match method {
        BaselineMethod::Constant { point } => Ok(Baseline {
            shape: Shape::Poly {
                coeffs: vec![point.1],
                center: 0.0,
                scale: 1.0,
            },
            points_used: 1,
        }),
        BaselineMethod::ConstantZone { zone } => {
            let values: Vec<f64> = series
                .x
                .iter()
                .zip(&series.y)
                .filter(|(x, y)| {
                    x.is_finite()
                        && y.is_finite()
                        && **x >= zone.0.min(zone.1)
                        && **x <= zone.0.max(zone.1)
                })
                .map(|(_, y)| *y)
                .collect();
            if values.is_empty() {
                return Err(AnalysisError::ZonesTooSmall {
                    found: 0,
                    needed: 1,
                    degree: 0,
                });
            }
            // Moyenne en ligne : évite la somme intermédiaire qui déborde.
            let mean = values.iter().enumerate().fold(0.0_f64, |mean, (i, y)| {
                let n = (i + 1) as f64;
                if mean.signum() == y.signum() {
                    mean + (y - mean) / n
                } else {
                    // La différence entre deux signes opposés peut déborder.
                    mean * (i as f64 / n) + y / n
                }
            });
            Ok(Baseline {
                shape: Shape::Poly {
                    coeffs: vec![mean],
                    center: 0.0,
                    scale: 1.0,
                },
                points_used: values.len(),
            })
        }
        BaselineMethod::Line { p1, p2 } => {
            if p1.0 == p2.0 {
                return Err(AnalysisError::DuplicateAnchor);
            }
            let slope = (p2.1 - p1.1) / (p2.0 - p1.0);
            Ok(Baseline {
                shape: Shape::Poly {
                    coeffs: vec![p1.1, slope],
                    center: p1.0,
                    scale: 1.0,
                },
                points_used: 2,
            })
        }
        BaselineMethod::Zones { zones, degree } => {
            let mut xs = Vec::new();
            let mut ys = Vec::new();
            for (&x, &y) in series.x.iter().zip(&series.y) {
                if zones.iter().any(|&(a, b)| x >= a.min(b) && x <= a.max(b)) {
                    xs.push(x);
                    ys.push(y);
                }
            }
            let needed = *degree as usize + 1;
            if xs.len() < needed {
                return Err(AnalysisError::ZonesTooSmall {
                    found: xs.len(),
                    needed,
                    degree: *degree,
                });
            }
            let lo = xs.iter().cloned().fold(f64::INFINITY, f64::min);
            let hi = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            // Conditionnement : t = (x − centre)/demi-étendue ∈ [-1, 1].
            // Moitiés calculées avant la somme/différence : `lo + hi` et `hi − lo`
            // débordent pour des X extrêmes. Étendue nulle (X tous égaux) : échelle 1,
            // t = 0 partout, sans quoi `t` deviendrait énorme et `0·inf` donnerait NaN.
            let center = lo / 2.0 + hi / 2.0;
            let half = hi / 2.0 - lo / 2.0;
            let scale = if half > 0.0 { half } else { 1.0 };
            let mut a = Matrix::zeros(xs.len(), needed);
            for (i, &x) in xs.iter().enumerate() {
                let t = (x - center) / scale;
                let mut v = 1.0;
                for j in 0..needed {
                    a.set(i, j, v);
                    v *= t;
                }
            }
            // Assez de points mais de rang insuffisant (X tous égaux) : ce n'est
            // pas un manque de points, le dire autrement.
            let ls = least_squares(&a, &ys).ok_or(AnalysisError::DuplicateX)?;
            Ok(Baseline {
                shape: Shape::Poly {
                    coeffs: ls.coefficients,
                    center,
                    scale,
                },
                points_used: xs.len(),
            })
        }
        BaselineMethod::Anchors {
            points,
            interpolation,
        } => {
            let mut pts: Vec<(f64, f64)> =
                points.iter().copied().filter(|p| pair_finite(*p)).collect();
            if pts.len() < 2 {
                return Err(AnalysisError::NotEnoughAnchors {
                    found: pts.len(),
                    needed: 2,
                });
            }
            pts.sort_by(|a, b| a.0.total_cmp(&b.0));
            if pts.windows(2).any(|w| w[0].0 == w[1].0) {
                return Err(AnalysisError::DuplicateAnchor);
            }
            let used = pts.len();
            let shape = match interpolation {
                Interpolation::Segments => Shape::Segments(pts),
                Interpolation::Spline => {
                    let second = spline_second_derivatives(&pts);
                    Shape::Spline {
                        points: pts,
                        second,
                    }
                }
            };
            Ok(Baseline {
                shape,
                points_used: used,
            })
        }
    }
}

/// Applique la ligne de base à une valeur Y. Une division par zéro (ou un
/// résultat non fini) donne `NaN`.
pub fn apply(op: BaselineOp, y: f64, baseline: f64) -> f64 {
    let v = match op {
        BaselineOp::Subtract => y - baseline,
        BaselineOp::Divide => y / baseline,
    };
    if v.is_finite() {
        v
    } else {
        f64::NAN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_series() -> Series {
        let x: Vec<f64> = (0..101).map(|i| i as f64).collect();
        // Fond : 2 + 0.05 x ; un pic large au milieu.
        let y = x
            .iter()
            .map(|&v| 2.0 + 0.05 * v + 10.0 * libm::exp(-(v - 50.0).powi(2) / 20.0))
            .collect();
        Series::new(x, y)
    }

    #[test]
    fn la_constante_n_a_aucune_pente_et_la_zone_prend_la_moyenne() {
        let s = Series::new(
            vec![0.0, 1.0, 2.0, 3.0, 4.0],
            vec![100.0, 2.0, f64::NAN, 6.0, 100.0],
        );
        let point = build(&BaselineMethod::Constant { point: (2.0, 7.0) }, &s).unwrap();
        assert_eq!(point.as_line(), Some((0.0, 7.0)));
        for x in [-100.0, 0.0, 200.0] {
            assert_eq!(point.eval(x), 7.0);
        }
        let zone = build(&BaselineMethod::ConstantZone { zone: (3.0, 1.0) }, &s).unwrap();
        assert_eq!(zone.as_line(), Some((0.0, 4.0)));
        assert_eq!(zone.points_used, 2);
        assert_ne!(point.eval(2.0), zone.eval(2.0));
        assert!(build(&BaselineMethod::ConstantZone { zone: (10.0, 11.0) }, &s).is_err());
        assert!(build(
            &BaselineMethod::Constant {
                point: (0.0, f64::NAN)
            },
            &s
        )
        .is_err());
        assert!(build(
            &BaselineMethod::ConstantZone {
                zone: (0.0, f64::INFINITY)
            },
            &s
        )
        .is_err());
    }

    #[test]
    fn moyenne_de_zone_ne_deborde_pas_pour_un_fond_fini_tres_grand() {
        let s = Series::new(vec![0.0, 1.0, 2.0], vec![f64::MAX; 3]);
        let b = build(&BaselineMethod::ConstantZone { zone: (0.0, 2.0) }, &s).unwrap();
        assert!(b.eval(1.0).is_finite());
        assert!((b.eval(1.0) / f64::MAX - 1.0).abs() < 1e-15);
    }

    #[test]
    fn la_droite_par_deux_points_passe_par_eux() {
        let b = build(
            &BaselineMethod::Line {
                p1: (0.0, 2.0),
                p2: (100.0, 7.0),
            },
            &Series::default(),
        )
        .unwrap();
        assert!((b.eval(0.0) - 2.0).abs() < 1e-12);
        assert!((b.eval(100.0) - 7.0).abs() < 1e-12);
        assert!((b.eval(40.0) - 4.0).abs() < 1e-12);
        let (slope, intercept) = b.as_line().unwrap();
        assert!((slope - 0.05).abs() < 1e-12 && (intercept - 2.0).abs() < 1e-12);
    }

    #[test]
    fn deux_points_de_meme_x_sont_refuses() {
        assert_eq!(
            build(
                &BaselineMethod::Line {
                    p1: (1.0, 2.0),
                    p2: (1.0, 5.0)
                },
                &Series::default()
            )
            .unwrap_err(),
            AnalysisError::DuplicateAnchor
        );
    }

    #[test]
    fn les_zones_retrouvent_le_fond_malgre_le_pic() {
        let s = line_series();
        let b = build(
            &BaselineMethod::Zones {
                zones: vec![(0.0, 20.0), (80.0, 100.0)],
                degree: 1,
            },
            &s,
        )
        .unwrap();
        let (slope, intercept) = b.as_line().unwrap();
        assert!((slope - 0.05).abs() < 1e-6, "pente {slope}");
        assert!((intercept - 2.0).abs() < 1e-4, "ordonnée {intercept}");
        assert_eq!(b.points_used, 42);
        // Corrigée au sommet du pic : ≈ 10.
        let corrected = apply(BaselineOp::Subtract, s.y[50], b.eval(50.0));
        assert!((corrected - 10.0).abs() < 1e-3, "{corrected}");
    }

    #[test]
    fn un_polynome_de_degre_2_suit_un_fond_courbe() {
        let x: Vec<f64> = (0..50).map(|i| i as f64).collect();
        let y: Vec<f64> = x.iter().map(|v| 1.0 + 0.1 * v + 0.01 * v * v).collect();
        let s = Series::new(x, y);
        let b = build(
            &BaselineMethod::Zones {
                zones: vec![(0.0, 49.0)],
                degree: 2,
            },
            &s,
        )
        .unwrap();
        for v in [0.0, 13.0, 49.0, 60.0] {
            let want = 1.0 + 0.1 * v + 0.01 * v * v;
            assert!((b.eval(v) - want).abs() < 1e-8, "x={v}");
        }
        assert!(b.as_line().is_none());
    }

    #[test]
    fn une_borne_de_zone_non_finie_est_refusee() {
        let x: Vec<f64> = (0..5).map(f64::from).collect();
        let s = Series::new(x.clone(), x.iter().map(|v| v + 1.0).collect());
        for bad in [f64::NAN, f64::INFINITY] {
            for zones in [vec![(bad, 2.0)], vec![(0.0, 1.0), (3.0, bad)]] {
                assert_eq!(
                    build(&BaselineMethod::Zones { zones, degree: 0 }, &s).unwrap_err(),
                    AnalysisError::InvalidParameter {
                        name: "zones".into()
                    }
                );
            }
        }
    }

    #[test]
    fn un_point_de_droite_non_fini_est_refuse() {
        let s = line_series();
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for (p1, p2) in [
                ((bad, 1.0), (4.0, 3.0)),
                ((0.0, bad), (4.0, 3.0)),
                ((0.0, 1.0), (bad, 3.0)),
                ((0.0, 1.0), (4.0, bad)),
            ] {
                assert_eq!(
                    build(&BaselineMethod::Line { p1, p2 }, &s).unwrap_err(),
                    AnalysisError::InvalidParameter {
                        name: "points".into()
                    }
                );
            }
        }
    }

    #[test]
    fn zones_x_tous_egaux_degre_0_reste_fini_loin_du_centre() {
        // #274 : l'échelle plancher MIN_POSITIVE rendait t énorme, `0·inf` = NaN.
        let s = Series::new(vec![0.0, 0.0], vec![1.0, 3.0]);
        let b = build(
            &BaselineMethod::Zones {
                zones: vec![(0.0, 0.0)],
                degree: 0,
            },
            &s,
        )
        .unwrap();
        assert!((b.eval(0.0) - 2.0).abs() < 1e-12);
        assert!((b.eval(5.0) - 2.0).abs() < 1e-12);
        assert!((b.eval(1e10) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn zones_x_extremes_donnent_une_droite_et_non_duplicate_x() {
        // #274 : `hi − lo` débordait en infini, d'où un DuplicateX trompeur.
        let s = Series::new(vec![-1e308, 0.0, 1e308], vec![-1.0, 0.0, 1.0]);
        let b = build(
            &BaselineMethod::Zones {
                zones: vec![(-1e308, 1e308)],
                degree: 1,
            },
            &s,
        )
        .unwrap();
        assert!(b.eval(0.0).abs() < 1e-9);
        assert!((b.eval(1e308) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn toute_borne_non_finie_est_refusee_sur_lo_comme_sur_hi() {
        let x: Vec<f64> = (0..10).map(f64::from).collect();
        let s = Series::new(x.clone(), x.iter().map(|v| 2.0 * v + 1.0).collect());
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for zone in [(bad, 2.0), (0.0, bad), (bad, bad)] {
                // Seule, première ou seconde zone : l'ordre ne change rien.
                for zones in [vec![zone], vec![zone, (6.0, 9.0)], vec![(0.0, 3.0), zone]] {
                    for degree in [0, 1, 9] {
                        let method = BaselineMethod::Zones {
                            zones: zones.clone(),
                            degree,
                        };
                        assert_eq!(
                            build(&method, &s).unwrap_err(),
                            AnalysisError::InvalidParameter {
                                name: "zones".into()
                            },
                            "{method:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn zones_finies_valeurs_de_reference_inchangees() {
        // y = 2x + 1 sur 0..=9 ; deux zones : la droite est retrouvée.
        let x: Vec<f64> = (0..10).map(f64::from).collect();
        let s = Series::new(x.clone(), x.iter().map(|v| 2.0 * v + 1.0).collect());
        let b = build(
            &BaselineMethod::Zones {
                zones: vec![(0.0, 2.0), (7.0, 9.0)],
                degree: 1,
            },
            &s,
        )
        .unwrap();
        assert_eq!(b.points_used, 6);
        for v in [0.0, 3.5, 5.0, 9.0] {
            assert!((b.eval(v) - (2.0 * v + 1.0)).abs() < 1e-9, "x = {v}");
        }
        // Degré 0 : la moyenne de y sur [1, 3] = (3 + 5 + 7) / 3 = 5.
        let s = Series::new(
            (0..6).map(f64::from).collect(),
            vec![1.0, 3.0, 5.0, 7.0, 9.0, 11.0],
        );
        let mean = |zone| {
            build(
                &BaselineMethod::Zones {
                    zones: vec![zone],
                    degree: 0,
                },
                &s,
            )
            .unwrap()
        };
        let b = mean((1.0, 3.0));
        assert_eq!(b.points_used, 3);
        assert!((b.eval(0.0) - 5.0).abs() < 1e-12);
        // Zone inversée : normalisée, même ligne de base.
        let inv = mean((3.0, 1.0));
        assert_eq!(inv.points_used, 3);
        assert!((inv.eval(4.0) - 5.0).abs() < 1e-12);
        // Zone dégénérée lo == hi : un point, accepté (degré 0).
        let d = mean((2.0, 2.0));
        assert_eq!(d.points_used, 1);
        assert!((d.eval(0.0) - 5.0).abs() < 1e-12);
    }

    fn anchors(points: Vec<(f64, f64)>, interpolation: Interpolation) -> BaselineMethod {
        BaselineMethod::Anchors {
            points,
            interpolation,
        }
    }

    /// #298 : une ancre non posée (non finie) est ignorée, comme par
    /// `readiness` ; seules les ancres posées comptent pour le minimum de 2.
    #[test]
    fn une_ancre_non_finie_est_ignoree_en_toute_position_et_coordonnee() {
        let s = line_series();
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for p in [(bad, 2.0), (2.0, bad), (bad, bad)] {
                for interpolation in [Interpolation::Segments, Interpolation::Spline] {
                    for points in [
                        vec![p, (1.0, 1.0), (3.0, 2.0)],
                        vec![(1.0, 1.0), p, (3.0, 2.0)],
                        vec![(1.0, 1.0), (3.0, 2.0), p],
                    ] {
                        let b = build(&anchors(points.clone(), interpolation), &s).unwrap();
                        assert_eq!(b.points_used, 2, "{points:?}");
                    }
                    // Une seule ancre posée sur deux : trop peu.
                    assert_eq!(
                        build(&anchors(vec![(1.0, 1.0), p], interpolation), &s).unwrap_err(),
                        AnalysisError::NotEnoughAnchors {
                            found: 1,
                            needed: 2
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn les_ancres_valides_donnent_les_valeurs_de_reference() {
        let s = Series::default();
        // Segments : deux pentes, ancres traversées exactement.
        let seg = build(
            &anchors(
                vec![(0.0, 0.0), (2.0, 4.0), (4.0, 0.0)],
                Interpolation::Segments,
            ),
            &s,
        )
        .unwrap();
        assert_eq!(seg.points_used, 3);
        for (x, y) in [(0.0, 0.0), (1.0, 2.0), (2.0, 4.0), (3.0, 2.0), (4.0, 0.0)] {
            assert!((seg.eval(x) - y).abs() < 1e-12, "x={x}");
        }
        // Spline naturelle sur (0,0) (1,1) (2,0) : dérivée seconde −3 au milieu,
        // donc 0,5 + 3·0,375/6 = 0,6875 à mi-intervalle.
        let sp = build(
            &anchors(
                vec![(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)],
                Interpolation::Spline,
            ),
            &s,
        )
        .unwrap();
        assert_eq!(sp.points_used, 3);
        for (x, y) in [
            (0.0, 0.0),
            (1.0, 1.0),
            (2.0, 0.0),
            (0.5, 0.6875),
            (1.5, 0.6875),
        ] {
            assert!((sp.eval(x) - y).abs() < 1e-12, "x={x}");
        }
    }

    #[test]
    fn les_ancres_non_triees_donnent_la_meme_ligne_que_triees() {
        let s = Series::default();
        for interpolation in [Interpolation::Segments, Interpolation::Spline] {
            let sorted = build(
                &anchors(
                    vec![(0.0, 1.0), (2.0, 5.0), (3.0, 2.0), (6.0, 4.0)],
                    interpolation,
                ),
                &s,
            )
            .unwrap();
            let shuffled = build(
                &anchors(
                    vec![(3.0, 2.0), (0.0, 1.0), (6.0, 4.0), (2.0, 5.0)],
                    interpolation,
                ),
                &s,
            )
            .unwrap();
            assert_eq!(sorted.points_used, 4);
            assert_eq!(shuffled.points_used, 4);
            for x in [-1.0, 0.0, 1.0, 2.5, 3.0, 4.5, 6.0, 7.0] {
                assert_eq!(sorted.eval(x), shuffled.eval(x), "x={x}");
            }
        }
    }

    #[test]
    fn trop_peu_d_ancres_ou_x_egaux_sont_refuses_sans_confusion() {
        let s = Series::default();
        for interpolation in [Interpolation::Segments, Interpolation::Spline] {
            for points in [vec![], vec![(1.0, 1.0)]] {
                let found = points.len();
                assert_eq!(
                    build(&anchors(points, interpolation), &s).unwrap_err(),
                    AnalysisError::NotEnoughAnchors { found, needed: 2 }
                );
            }
            // X égaux (y compris non adjacents dans la liste) : DuplicateAnchor.
            for points in [
                vec![(1.0, 1.0), (1.0, 2.0)],
                vec![(1.0, 1.0), (3.0, 2.0), (1.0, 5.0)],
            ] {
                assert_eq!(
                    build(&anchors(points, interpolation), &s).unwrap_err(),
                    AnalysisError::DuplicateAnchor
                );
            }
        }
    }

    #[test]
    fn zones_trop_petites() {
        let s = line_series();
        let err = build(
            &BaselineMethod::Zones {
                zones: vec![(0.0, 1.0)],
                degree: 3,
            },
            &s,
        )
        .unwrap_err();
        assert_eq!(
            err,
            AnalysisError::ZonesTooSmall {
                found: 2,
                needed: 4,
                degree: 3
            }
        );
    }

    #[test]
    fn segments_passent_par_les_ancrages_et_se_prolongent() {
        let b = build(
            &BaselineMethod::Anchors {
                // Désordonnés exprès : le tri est interne.
                points: vec![(10.0, 4.0), (0.0, 0.0), (5.0, 1.0)],
                interpolation: Interpolation::Segments,
            },
            &Series::default(),
        )
        .unwrap();
        assert_eq!(b.eval(0.0), 0.0);
        assert_eq!(b.eval(5.0), 1.0);
        assert_eq!(b.eval(10.0), 4.0);
        assert!((b.eval(7.5) - 2.5).abs() < 1e-12);
        // Prolongement du dernier segment (pente 0.6).
        assert!((b.eval(20.0) - 10.0).abs() < 1e-12);
        assert!((b.eval(-5.0) + 1.0).abs() < 1e-12);
    }

    #[test]
    fn la_spline_passe_par_les_ancrages_et_reproduit_une_droite() {
        let pts = vec![(0.0, 1.0), (2.0, 5.0), (4.0, 9.0), (6.0, 13.0)];
        let b = build(
            &BaselineMethod::Anchors {
                points: pts.clone(),
                interpolation: Interpolation::Spline,
            },
            &Series::default(),
        )
        .unwrap();
        for (x, y) in &pts {
            assert!((b.eval(*x) - y).abs() < 1e-12);
        }
        // Points alignés : la spline naturelle est la droite, extrapolation incluse.
        assert!((b.eval(3.0) - 7.0).abs() < 1e-12);
        assert!((b.eval(8.0) - 17.0).abs() < 1e-9);
        assert!((b.eval(-1.0) + 1.0).abs() < 1e-9);
    }

    #[test]
    fn la_spline_est_lisse_la_ou_les_segments_cassent() {
        // Ancrages d'une parabole : la spline est plus proche que les segments.
        let pts: Vec<(f64, f64)> = (0..=6).map(|i| (i as f64, (i as f64).powi(2))).collect();
        let spline = build(
            &BaselineMethod::Anchors {
                points: pts.clone(),
                interpolation: Interpolation::Spline,
            },
            &Series::default(),
        )
        .unwrap();
        let seg = build(
            &BaselineMethod::Anchors {
                points: pts,
                interpolation: Interpolation::Segments,
            },
            &Series::default(),
        )
        .unwrap();
        let (es, eg) = (
            (spline.eval(3.5) - 12.25).abs(),
            (seg.eval(3.5) - 12.25).abs(),
        );
        assert!(es < eg, "spline {es} vs segments {eg}");
        assert!(eg > 0.2, "témoin : les segments s'écartent de la parabole");
    }

    #[test]
    fn ancrages_insuffisants_ou_dupliques() {
        let one = BaselineMethod::Anchors {
            points: vec![(0.0, 0.0)],
            interpolation: Interpolation::Segments,
        };
        assert_eq!(
            build(&one, &Series::default()).unwrap_err(),
            AnalysisError::NotEnoughAnchors {
                found: 1,
                needed: 2
            }
        );
        let dup = BaselineMethod::Anchors {
            points: vec![(1.0, 0.0), (1.0, 3.0)],
            interpolation: Interpolation::Spline,
        };
        assert_eq!(
            build(&dup, &Series::default()).unwrap_err(),
            AnalysisError::DuplicateAnchor
        );
    }

    #[test]
    fn apply_soustrait_divise_et_degrade_en_nan() {
        assert_eq!(apply(BaselineOp::Subtract, 5.0, 2.0), 3.0);
        assert_eq!(apply(BaselineOp::Divide, 6.0, 2.0), 3.0);
        assert!(apply(BaselineOp::Divide, 6.0, 0.0).is_nan());
    }
}
