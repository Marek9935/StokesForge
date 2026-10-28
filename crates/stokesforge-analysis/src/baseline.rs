//! Baselines (#185, #211): constant from a point or a zone, line
//! from two points, polynomial fitted on
//! background zones, or curve through anchor points (segments or spline).
//!
//! This module only builds the baseline **function**; subtracting or
//! dividing is [`apply`]. Anchor points placed "with magnet and local
//! mean" (M4 b) are computed by the interface with
//! [`crate::series::local_mean`]; here an anchor is a point `(x, y)`.

use crate::error::AnalysisError;
use crate::linalg::{least_squares, Matrix};
use crate::series::Series;

/// Interpolation between anchor points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Segments,
    Spline,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BaselineMethod {
    /// Flat background set by a free or magnetized point.
    Constant { point: (f64, f64) },
    /// Flat background: mean of Y in a single inclusive X range.
    ConstantZone { zone: (f64, f64) },
    /// Line through two points.
    Line { p1: (f64, f64), p2: (f64, f64) },
    /// Polynomial of degree `degree` fitted by least squares on the points of
    /// the series whose X falls in one of the `zones`; `degree = 1` is the line.
    Zones { zones: Vec<(f64, f64)>, degree: u32 },
    /// Curve through the anchor points.
    Anchors {
        points: Vec<(f64, f64)>,
        interpolation: Interpolation,
    },
}

/// What is done with the baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineOp {
    Subtract,
    Divide,
}

/// A baseline that can be evaluated at any X.
#[derive(Debug, Clone, PartialEq)]
pub struct Baseline {
    shape: Shape,
    /// Number of points used to determine it (zones), otherwise the number
    /// of anchors.
    pub points_used: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum Shape {
    /// Polynomial in `(x − center)/scale`, coefficients by increasing powers.
    Poly {
        coeffs: Vec<f64>,
        center: f64,
        scale: f64,
    },
    /// Segments: points sorted by X.
    Segments(Vec<(f64, f64)>),
    /// Natural cubic spline: sorted points, second derivatives.
    Spline {
        points: Vec<(f64, f64)>,
        second: Vec<f64>,
    },
}

impl Baseline {
    /// Value of the baseline at `x`.
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

    /// Slope and intercept, if the baseline is a line.
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
    // Linear extension of the end segments.
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
    // Tridiagonal system of the natural spline (Thomas algorithm).
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
        // Extension by the tangent at the end (second derivative zero).
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
    /// Refuses a non-finite coordinate (NaN, ±inf) placed by the user:
    /// `f64::min/max` and comparisons would silently ignore it (a NaN zone
    /// bound would become the zone `[b, b]`). Single source of the rule,
    /// called by [`build`] and by `Operation::validate`; a method that
    /// carries coordinates is added here.
    pub(crate) fn check_finite(&self) -> Result<(), AnalysisError> {
        let (name, finite) = match self {
            BaselineMethod::Constant { point } => ("point", pair_finite(*point)),
            BaselineMethod::Line { p1, p2 } => ("points", pair_finite(*p1) && pair_finite(*p2)),
            BaselineMethod::ConstantZone { zone } => ("zone", pair_finite(*zone)),
            BaselineMethod::Zones { zones, .. } => ("zones", zones.iter().all(|z| pair_finite(*z))),
            // An unplaced (non-finite) anchor is ignored by `build`, like by
            // `readiness` (#298): never an error.
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

/// Builds the baseline of `method`. `series` is only used by the
/// zones method (the background points).
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
            // Running mean: avoids the intermediate sum that overflows.
            let mean = values.iter().enumerate().fold(0.0_f64, |mean, (i, y)| {
                let n = (i + 1) as f64;
                if mean.signum() == y.signum() {
                    mean + (y - mean) / n
                } else {
                    // The difference between two opposite signs can overflow.
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
            // Conditioning: t = (x − center)/half-extent ∈ [-1, 1].
            // Halves computed before the sum/difference: `lo + hi` and `hi − lo`
            // overflow for extreme X. Zero extent (all X equal): scale 1,
            // t = 0 everywhere, otherwise `t` would become huge and `0·inf` would give NaN.
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
            // Enough points but insufficient rank (all X equal): this is not
            // a lack of points, say it differently.
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

/// Applies the baseline to a Y value. A division by zero (or a
/// non-finite result) gives `NaN`.
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
        // Background: 2 + 0.05 x; a wide peak in the middle.
        let y = x
            .iter()
            .map(|&v| 2.0 + 0.05 * v + 10.0 * libm::exp(-(v - 50.0).powi(2) / 20.0))
            .collect();
        Series::new(x, y)
    }

    #[test]
    fn the_constant_has_no_slope_and_the_zone_takes_the_mean() {
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
    fn zone_mean_does_not_overflow_for_a_very_large_finite_background() {
        let s = Series::new(vec![0.0, 1.0, 2.0], vec![f64::MAX; 3]);
        let b = build(&BaselineMethod::ConstantZone { zone: (0.0, 2.0) }, &s).unwrap();
        assert!(b.eval(1.0).is_finite());
        assert!((b.eval(1.0) / f64::MAX - 1.0).abs() < 1e-15);
    }

    #[test]
    fn the_line_through_two_points_passes_through_them() {
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
    fn two_points_with_the_same_x_are_refused() {
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
    fn the_zones_recover_the_background_despite_the_peak() {
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
        assert!((slope - 0.05).abs() < 1e-6, "slope {slope}");
        assert!((intercept - 2.0).abs() < 1e-4, "intercept {intercept}");
        assert_eq!(b.points_used, 42);
        // Corrected at the top of the peak: ≈ 10.
        let corrected = apply(BaselineOp::Subtract, s.y[50], b.eval(50.0));
        assert!((corrected - 10.0).abs() < 1e-3, "{corrected}");
    }

    #[test]
    fn a_degree_2_polynomial_follows_a_curved_background() {
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
    fn a_non_finite_zone_bound_is_refused() {
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
    fn a_non_finite_line_point_is_refused() {
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
    fn zones_with_all_x_equal_degree_0_stays_finite_far_from_the_center() {
        // #274: the floor scale MIN_POSITIVE made t huge, `0·inf` = NaN.
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
    fn extreme_x_zones_give_a_line_and_not_duplicate_x() {
        // #274: `hi − lo` overflowed to infinity, hence a misleading DuplicateX.
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
    fn any_non_finite_bound_is_refused_on_lo_as_on_hi() {
        let x: Vec<f64> = (0..10).map(f64::from).collect();
        let s = Series::new(x.clone(), x.iter().map(|v| 2.0 * v + 1.0).collect());
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for zone in [(bad, 2.0), (0.0, bad), (bad, bad)] {
                // Alone, first or second zone: the order changes nothing.
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
    fn finite_zones_reference_values_unchanged() {
        // y = 2x + 1 on 0..=9; two zones: the line is recovered.
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
        // Degree 0: the mean of y on [1, 3] = (3 + 5 + 7) / 3 = 5.
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
        // Reversed zone: normalized, same baseline.
        let inv = mean((3.0, 1.0));
        assert_eq!(inv.points_used, 3);
        assert!((inv.eval(4.0) - 5.0).abs() < 1e-12);
        // Degenerate zone lo == hi: one point, accepted (degree 0).
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

    /// #298: an unplaced (non-finite) anchor is ignored, like by
    /// `readiness`; only placed anchors count towards the minimum of 2.
    #[test]
    fn a_non_finite_anchor_is_ignored_at_any_position_and_coordinate() {
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
                    // Only one anchor placed out of two: too few.
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
    fn valid_anchors_give_the_reference_values() {
        let s = Series::default();
        // Segments: two slopes, anchors crossed exactly.
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
        // Natural spline on (0,0) (1,1) (2,0): second derivative −3 in the middle,
        // hence 0.5 + 3·0.375/6 = 0.6875 at mid-interval.
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
    fn unsorted_anchors_give_the_same_line_as_sorted() {
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
    fn too_few_anchors_or_equal_x_are_refused_without_confusion() {
        let s = Series::default();
        for interpolation in [Interpolation::Segments, Interpolation::Spline] {
            for points in [vec![], vec![(1.0, 1.0)]] {
                let found = points.len();
                assert_eq!(
                    build(&anchors(points, interpolation), &s).unwrap_err(),
                    AnalysisError::NotEnoughAnchors { found, needed: 2 }
                );
            }
            // Equal X (including non-adjacent in the list): DuplicateAnchor.
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
    fn zones_too_small() {
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
    fn segments_pass_through_the_anchors_and_extend() {
        let b = build(
            &BaselineMethod::Anchors {
                // Deliberately unordered: the sort is internal.
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
        // Extension of the last segment (slope 0.6).
        assert!((b.eval(20.0) - 10.0).abs() < 1e-12);
        assert!((b.eval(-5.0) + 1.0).abs() < 1e-12);
    }

    #[test]
    fn the_spline_passes_through_the_anchors_and_reproduces_a_line() {
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
        // Aligned points: the natural spline is the line, extrapolation included.
        assert!((b.eval(3.0) - 7.0).abs() < 1e-12);
        assert!((b.eval(8.0) - 17.0).abs() < 1e-9);
        assert!((b.eval(-1.0) + 1.0).abs() < 1e-9);
    }

    #[test]
    fn the_spline_is_smooth_where_segments_break() {
        // Anchors of a parabola: the spline is closer than the segments.
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
        assert!(eg > 0.2, "witness: the segments deviate from the parabola");
    }

    #[test]
    fn insufficient_or_duplicated_anchors() {
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
    fn apply_subtracts_divides_and_degrades_to_nan() {
        assert_eq!(apply(BaselineOp::Subtract, 5.0, 2.0), 3.0);
        assert_eq!(apply(BaselineOp::Divide, 6.0, 2.0), 3.0);
        assert!(apply(BaselineOp::Divide, 6.0, 0.0).is_nan());
    }
}
