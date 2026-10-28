//! Integration by the trapezoid method (#187), in the **X units as they
//! are**: a wavelength curve is integrated in wavelength, an energy curve in
//! energy. No conversion (decision 2026-10-08).
//!
//! The area is that of the series **sorted by increasing X**: it is positive
//! for a positive Y whatever the recording direction of the spectrum.

use crate::error::AnalysisError;
use crate::series::{interpolate, Series};

/// What is subtracted under the curve of a range (M12 b).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Under {
    /// Area down to Y = 0.
    Zero,
    /// Area above the line joining the two ends of the range.
    Line,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IntegrateSpec {
    /// Ranges `(from, to)`. Empty: the whole series.
    pub ranges: Vec<(f64, f64)>,
    pub under: Under,
    pub cumulative: bool,
    pub normalize: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RangeArea {
    /// Effective bounds, clipped to the data domain.
    pub from: f64,
    pub to: f64,
    pub area: f64,
    /// Points of the series in the range.
    pub points: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Integrated {
    pub areas: Vec<RangeArea>,
    pub total: f64,
    /// Cumulative integral from the smallest X, one point per point of the
    /// input series (in its order).
    pub cumulative: Option<Vec<f64>>,
    /// `y / total`, one point per point of the input series.
    pub normalized: Option<Vec<f64>>,
}

/// Trapezoid area of the sorted series on `[lo, hi]` (interpolated bounds).
fn trapezoid(sorted: &Series, lo: f64, hi: f64, under: Under) -> Option<(f64, usize)> {
    let (x0, x1) = (*sorted.x.first()?, *sorted.x.last()?);
    let (lo, hi) = (lo.max(x0), hi.min(x1));
    if !(hi > lo) {
        return None;
    }
    let ylo = interpolate(&sorted.x, &sorted.y, lo)?;
    let yhi = interpolate(&sorted.x, &sorted.y, hi)?;
    let inside: Vec<usize> = (0..sorted.len())
        .filter(|&i| sorted.x[i] > lo && sorted.x[i] < hi)
        .collect();
    let mut xs = vec![lo];
    let mut ys = vec![ylo];
    for &i in &inside {
        xs.push(sorted.x[i]);
        ys.push(sorted.y[i]);
    }
    xs.push(hi);
    ys.push(yhi);
    let mut area = 0.0;
    for k in 1..xs.len() {
        area += 0.5 * (ys[k] + ys[k - 1]) * (xs[k] - xs[k - 1]);
    }
    if under == Under::Line {
        area -= 0.5 * (ylo + yhi) * (hi - lo);
    }
    Some((area, inside.len() + 2))
}

pub fn integrate(series: &Series, spec: &IntegrateSpec) -> Result<Integrated, AnalysisError> {
    if series.len() < 2 {
        return Err(AnalysisError::NotEnoughPoints {
            found: series.len(),
            needed: 2,
        });
    }
    // A non-finite bound (range not yet placed, NaN) would make `min/max`
    // return the other argument: (NaN, NaN) would integrate the whole curve (#276).
    if spec
        .ranges
        .iter()
        .any(|&(a, b)| !(a.is_finite() && b.is_finite()))
    {
        return Err(AnalysisError::InvalidParameter {
            name: "ranges".into(),
        });
    }
    let sorted = series.sorted();
    let (xmin, xmax) = (sorted.x[0], sorted.x[sorted.len() - 1]);
    let ranges: Vec<(f64, f64)> = if spec.ranges.is_empty() {
        vec![(xmin, xmax)]
    } else {
        spec.ranges
            .iter()
            .map(|&(a, b)| (a.min(b), a.max(b)))
            .collect()
    };
    let mut areas = Vec::new();
    for (lo, hi) in ranges {
        let (area, points) =
            trapezoid(&sorted, lo, hi, spec.under).ok_or(AnalysisError::EmptyRange)?;
        areas.push(RangeArea {
            from: lo.max(xmin),
            to: hi.min(xmax),
            area,
            points,
        });
    }
    // An overflow (infinite area) is not an area: NaN, like the other
    // non-finite values of the module (#275), so that normalization does not
    // give a misleading 0 (`y / inf`).
    let total: f64 = areas.iter().map(|a| a.area).sum();
    let total = if total.is_finite() { total } else { f64::NAN };

    let cumulative = spec.cumulative.then(|| {
        // Cumulative sum on the sorted series, then put back in input order.
        let mut order: Vec<usize> = (0..series.len()).collect();
        order.sort_by(|&a, &b| series.x[a].total_cmp(&series.x[b]));
        let mut out = vec![0.0; series.len()];
        let mut acc = 0.0;
        for k in 0..order.len() {
            if k > 0 {
                let (a, b) = (order[k - 1], order[k]);
                acc += 0.5 * (series.y[a] + series.y[b]) * (series.x[b] - series.x[a]);
            }
            out[order[k]] = if acc.is_finite() { acc } else { f64::NAN };
        }
        out
    });
    let normalized = spec.normalize.then(|| {
        series
            .y
            .iter()
            .map(|y| {
                let v = y / total;
                if v.is_finite() {
                    v
                } else {
                    f64::NAN
                }
            })
            .collect()
    });
    Ok(Integrated {
        areas,
        total,
        cumulative,
        normalized,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> IntegrateSpec {
        IntegrateSpec {
            ranges: vec![],
            under: Under::Zero,
            cumulative: false,
            normalize: false,
        }
    }

    fn parabola() -> Series {
        let x: Vec<f64> = (0..=1000).map(|i| i as f64 / 1000.0).collect();
        let y = x.iter().map(|v| v * v).collect();
        Series::new(x, y)
    }

    #[test]
    fn total_area_of_a_parabola() {
        let r = integrate(&parabola(), &spec()).unwrap();
        assert!((r.total - 1.0 / 3.0).abs() < 1e-6, "{}", r.total);
        assert_eq!(r.areas.len(), 1);
        assert_eq!(r.areas[0].points, 1001);
    }

    #[test]
    fn the_range_is_interpolated_at_the_bounds() {
        // Bounds between two samples: Y is piecewise affine, the area of
        // [0.25, 0.75] of y = x on a coarse grid is exact.
        let s = Series::new(vec![0.0, 0.5, 1.0], vec![0.0, 0.5, 1.0]);
        let mut sp = spec();
        sp.ranges = vec![(0.25, 0.75)];
        let r = integrate(&s, &sp).unwrap();
        assert!((r.total - 0.25).abs() < 1e-12, "{}", r.total);
    }

    #[test]
    fn several_ranges_give_one_area_each() {
        let mut sp = spec();
        sp.ranges = vec![(0.0, 0.5), (0.5, 1.0)];
        let r = integrate(&parabola(), &sp).unwrap();
        assert!((r.areas[0].area - 1.0 / 24.0).abs() < 1e-6);
        assert!((r.areas[1].area - 7.0 / 24.0).abs() < 1e-6);
        assert!((r.total - 1.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn below_the_line_between_the_bounds() {
        // Triangular peak on a flat background at 2: area above the line = 1.
        let s = Series::new(vec![0.0, 1.0, 2.0, 3.0, 4.0], vec![2.0, 2.0, 3.0, 2.0, 2.0]);
        let mut sp = spec();
        sp.under = Under::Line;
        let r = integrate(&s, &sp).unwrap();
        assert!((r.total - 1.0).abs() < 1e-12);
        sp.under = Under::Zero;
        assert!((integrate(&s, &sp).unwrap().total - 9.0).abs() < 1e-12);
    }

    #[test]
    fn a_spectrum_decreasing_in_x_gives_a_positive_area() {
        let s = Series::new(vec![4.0, 3.0, 2.0, 1.0], vec![1.0, 1.0, 1.0, 1.0]);
        assert!((integrate(&s, &spec()).unwrap().total - 3.0).abs() < 1e-12);
    }

    #[test]
    fn cumulative_and_normalization() {
        let s = Series::new(vec![0.0, 1.0, 2.0], vec![2.0, 2.0, 2.0]);
        let mut sp = spec();
        sp.cumulative = true;
        sp.normalize = true;
        let r = integrate(&s, &sp).unwrap();
        assert_eq!(r.cumulative.unwrap(), vec![0.0, 2.0, 4.0]);
        assert_eq!(r.normalized.unwrap(), vec![0.5, 0.5, 0.5]);
    }

    #[test]
    fn cumulative_in_input_order_for_a_decreasing_x() {
        let s = Series::new(vec![2.0, 1.0, 0.0], vec![2.0, 2.0, 2.0]);
        let mut sp = spec();
        sp.cumulative = true;
        let c = integrate(&s, &sp).unwrap().cumulative.unwrap();
        // From the smallest X: x=0 → 0, x=1 → 2, x=2 → 4.
        assert_eq!(c, vec![4.0, 2.0, 0.0]);
    }

    #[test]
    fn normalization_by_a_zero_area_gives_empty_cells() {
        let s = Series::new(vec![0.0, 1.0], vec![0.0, 0.0]);
        let mut sp = spec();
        sp.normalize = true;
        let r = integrate(&s, &sp).unwrap();
        assert!(r.normalized.unwrap().iter().all(|v| v.is_nan()));
    }

    #[test]
    fn area_overflow_gives_nan_not_zero() {
        let s = Series::new(vec![0.0, 10.0], vec![1e308, 1e308]);
        let mut sp = spec();
        sp.cumulative = true;
        sp.normalize = true;
        let r = integrate(&s, &sp).unwrap();
        assert!(r.total.is_nan());
        assert!(r.normalized.unwrap().iter().all(|v| v.is_nan()));
        let c = r.cumulative.unwrap();
        assert_eq!(c[0], 0.0);
        assert!(c[1].is_nan());
    }

    #[test]
    fn range_outside_the_data_and_series_too_short() {
        let mut sp = spec();
        sp.ranges = vec![(5.0, 6.0)];
        assert_eq!(
            integrate(&parabola(), &sp).unwrap_err(),
            AnalysisError::EmptyRange
        );
        assert!(matches!(
            integrate(&Series::new(vec![1.0], vec![1.0]), &spec()),
            Err(AnalysisError::NotEnoughPoints { .. })
        ));
    }

    #[test]
    fn range_partly_outside_the_data_is_clipped() {
        let mut sp = spec();
        sp.ranges = vec![(-3.0, 0.5)];
        let r = integrate(&parabola(), &sp).unwrap();
        assert_eq!(r.areas[0].from, 0.0);
        assert!((r.total - 1.0 / 24.0).abs() < 1e-6);
    }

    fn ramp() -> Series {
        let x: Vec<f64> = (0..=10).map(f64::from).collect();
        Series::new(x.clone(), x)
    }

    fn with_ranges(ranges: Vec<(f64, f64)>) -> IntegrateSpec {
        let mut sp = spec();
        sp.ranges = ranges;
        sp
    }

    fn invalid_ranges() -> Result<Integrated, AnalysisError> {
        Err(AnalysisError::InvalidParameter {
            name: "ranges".into(),
        })
    }

    #[test]
    fn any_non_finite_bound_is_refused_whatever_the_position() {
        let (nan, inf) = (f64::NAN, f64::INFINITY);
        let faulty = [
            (nan, nan),
            (nan, 5.0),
            (5.0, nan),
            (inf, 5.0),
            (-inf, 5.0),
            (2.0, inf),
            (2.0, -inf),
            (-inf, inf),
        ];
        for f in faulty {
            // Alone, or after / before a valid range: the order does not matter.
            for ranges in [vec![f], vec![(1.0, 2.0), f], vec![f, (1.0, 2.0)]] {
                assert_eq!(
                    integrate(&ramp(), &with_ranges(ranges.clone())),
                    invalid_ranges(),
                    "{ranges:?}"
                );
            }
        }
    }

    #[test]
    fn the_range_guard_runs_before_the_cumulative_and_normalization_calculation() {
        let mut sp = with_ranges(vec![(f64::NAN, f64::NAN)]);
        sp.cumulative = true;
        sp.normalize = true;
        assert_eq!(integrate(&ramp(), &sp), invalid_ranges());
    }

    #[test]
    fn a_series_too_short_is_reported_before_a_non_finite_range() {
        let s = Series::new(vec![1.0], vec![1.0]);
        assert_eq!(
            integrate(&s, &with_ranges(vec![(f64::NAN, f64::NAN)])),
            Err(AnalysisError::NotEnoughPoints {
                found: 1,
                needed: 2
            })
        );
    }

    #[test]
    fn reversed_range_accepted_and_normalized() {
        let r = integrate(&ramp(), &with_ranges(vec![(8.0, 2.0)])).unwrap();
        // Area of y = x on [2, 8]: (64 - 4) / 2 = 30.
        assert_eq!(r.areas[0].from, 2.0);
        assert_eq!(r.areas[0].to, 8.0);
        assert_eq!(r.areas[0].area, 30.0);
        assert_eq!(r.total, 30.0);
    }

    #[test]
    fn degenerate_range_is_an_empty_range() {
        assert_eq!(
            integrate(&ramp(), &with_ranges(vec![(5.0, 5.0)])),
            Err(AnalysisError::EmptyRange)
        );
        // An empty range after a valid one makes the whole fail.
        assert_eq!(
            integrate(&ramp(), &with_ranges(vec![(1.0, 2.0), (5.0, 5.0)])),
            Err(AnalysisError::EmptyRange)
        );
    }

    #[test]
    fn range_outside_the_data_on_the_other_side_is_empty() {
        assert_eq!(
            integrate(&ramp(), &with_ranges(vec![(-6.0, -5.0)])),
            Err(AnalysisError::EmptyRange)
        );
        // Range that touches the data at only one point: degenerate.
        assert_eq!(
            integrate(&ramp(), &with_ranges(vec![(10.0, 12.0)])),
            Err(AnalysisError::EmptyRange)
        );
    }

    #[test]
    fn empty_range_list_integrates_the_whole_curve() {
        let r = integrate(&ramp(), &spec()).unwrap();
        assert_eq!(r.total, 50.0);
        assert_eq!(r.areas.len(), 1);
        assert_eq!((r.areas[0].from, r.areas[0].to), (0.0, 10.0));
        assert_eq!(r.areas[0].points, 11);
    }

    #[test]
    fn overlapping_ranges_counted_each_and_summed() {
        // Current behavior: the overlap [4, 6] is counted twice.
        let r = integrate(&ramp(), &with_ranges(vec![(2.0, 6.0), (4.0, 8.0)])).unwrap();
        assert_eq!(r.areas[0].area, 16.0);
        assert_eq!(r.areas[1].area, 24.0);
        assert_eq!(r.total, 40.0);
    }

    #[test]
    fn ordinary_cumulative_and_normalization_on_a_ramp() {
        let mut sp = spec();
        sp.cumulative = true;
        sp.normalize = true;
        let s = Series::new(vec![0.0, 1.0, 2.0, 3.0], vec![0.0, 1.0, 2.0, 3.0]);
        let r = integrate(&s, &sp).unwrap();
        assert_eq!(r.total, 4.5);
        assert_eq!(r.cumulative.unwrap(), vec![0.0, 0.5, 2.0, 4.5]);
        assert_eq!(
            r.normalized.unwrap(),
            vec![0.0, 1.0 / 4.5, 2.0 / 4.5, 3.0 / 4.5]
        );
    }

    #[test]
    fn negative_infinite_total_overflow_also_gives_nan() {
        let s = Series::new(vec![0.0, 10.0], vec![-1e308, -1e308]);
        let mut sp = spec();
        sp.normalize = true;
        let r = integrate(&s, &sp).unwrap();
        assert!(r.total.is_nan());
        assert!(r.normalized.unwrap().iter().all(|v| v.is_nan()));
    }

    #[test]
    fn just_below_the_overflow_the_result_stays_finite_and_exact() {
        let s = Series::new(vec![0.0, 10.0], vec![1e307, 1e307]);
        let mut sp = spec();
        sp.cumulative = true;
        sp.normalize = true;
        let r = integrate(&s, &sp).unwrap();
        assert_eq!(r.total, 1e308);
        assert_eq!(r.areas[0].area, 1e308);
        assert_eq!(r.cumulative.unwrap(), vec![0.0, 1e308]);
        assert!(r
            .normalized
            .unwrap()
            .iter()
            .all(|v| (v - 0.1).abs() < 1e-15));
    }

    #[test]
    fn overflow_of_one_range_out_of_two_makes_the_total_nan_but_keeps_the_other_area() {
        let s = Series::new(vec![0.0, 1.0, 2.0, 12.0], vec![1.0, 1.0, 1e308, 1e308]);
        let r = integrate(&s, &with_ranges(vec![(0.0, 1.0), (2.0, 12.0)])).unwrap();
        assert_eq!(r.areas[0].area, 1.0);
        assert!(r.areas[1].area.is_infinite());
        assert!(r.total.is_nan());
    }

    #[test]
    fn normalization_by_a_zero_area_with_non_zero_y_gives_nan_not_infinity() {
        // Zero area (+1 then -1) but non-zero Y: `y / 0` = ±inf → NaN.
        let s = Series::new(vec![0.0, 1.0, 2.0], vec![1.0, 0.0, -1.0]);
        let mut sp = spec();
        sp.normalize = true;
        let r = integrate(&s, &sp).unwrap();
        assert_eq!(r.total, 0.0);
        assert!(r.normalized.unwrap().iter().all(|v| v.is_nan()));
    }
}
