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

/// Fabrique la ligne de base de `method`. `series` ne sert qu'à la méthode par
/// zones (les points de fond).
pub fn build(method: &BaselineMethod, series: &Series) -> Result<Baseline, AnalysisError> {
    match method {
        BaselineMethod::Constant { point } if !point.0.is_finite() || !point.1.is_finite() => {
            Err(AnalysisError::InvalidParameter {
                name: "point".into(),
            })
        }
        BaselineMethod::Constant { point } => Ok(Baseline {
            shape: Shape::Poly {
                coeffs: vec![point.1],
                center: 0.0,
                scale: 1.0,
            },
            points_used: 1,
        }),
        BaselineMethod::ConstantZone { zone } if !zone.0.is_finite() || !zone.1.is_finite() => {
            Err(AnalysisError::InvalidParameter {
                name: "zone".into(),
            })
        }
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
            let center = (lo + hi) / 2.0;
            let scale = ((hi - lo) / 2.0).max(f64::MIN_POSITIVE);
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
            if points.len() < 2 {
                return Err(AnalysisError::NotEnoughAnchors {
                    found: points.len(),
                    needed: 2,
                });
            }
            let mut pts = points.clone();
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
            .map(|&v| 2.0 + 0.05 * v + 10.0 * (-(v - 50.0).powi(2) / 20.0).exp())
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
