//! Correction by a reference column from another sheet (#186): the
//! reference (spectral response of a detector, blank) is brought onto the data
//! X grid by linear interpolation, then combined.
//!
//! Outside the reference domain, the corrected cell is **empty** (`NaN`) and
//! counted (M11 A): a measurement we do not have is not extended.
//!
//! An empty or non-finite cell **in the middle** of the reference (X known, Y not
//! finite) is bridged by interpolation between its neighbours, and the data
//! points that fall in this hole are counted (`bridged`, C3 #313). A reference
//! row whose X is missing has no position: it is not a hole.

use crate::error::AnalysisError;
use crate::series::{interpolate, Series};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrectOp {
    Divide,
    Multiply,
    Subtract,
}

/// Result of the correction, point by point of the input series.
#[derive(Debug, Clone, PartialEq)]
pub struct Corrected {
    pub values: Vec<f64>,
    /// The reference brought onto the data X values (`NaN` outside the domain).
    pub reference_on_grid: Vec<f64>,
    /// Points outside the reference domain.
    pub out_of_reference: usize,
    /// Data points interpolated across a hole in the reference (C3).
    pub bridged: usize,
    /// Divisions by an exactly zero reference, returned as `NaN` (C5). An
    /// overflow with a non-zero divisor is not counted.
    pub division_by_zero: usize,
}

/// Corrects `data` by `reference`. A division by zero gives `NaN` and is
/// counted; so is the domain. `holes`: the X values of the reference rows
/// whose Y is not finite (X finite), to count the bridged points.
pub fn correct(
    data: &Series,
    reference: &Series,
    holes: &[f64],
    op: CorrectOp,
) -> Result<Corrected, AnalysisError> {
    if reference.is_empty() {
        return Err(AnalysisError::EmptyReference);
    }
    let r = reference.sorted();
    let mut values = Vec::with_capacity(data.len());
    let mut on_grid = Vec::with_capacity(data.len());
    let mut out = 0;
    let mut bridged = 0;
    let mut by_zero = 0;
    for (&x, &y) in data.x.iter().zip(&data.y) {
        match interpolate(&r.x, &r.y, x) {
            Some(c) => {
                if bridges_hole(&r.x, holes, x) {
                    bridged += 1;
                }
                if op == CorrectOp::Divide && c == 0.0 && y.is_finite() {
                    by_zero += 1;
                }
                let v = match op {
                    CorrectOp::Divide => y / c,
                    CorrectOp::Multiply => y * c,
                    CorrectOp::Subtract => y - c,
                };
                values.push(if v.is_finite() { v } else { f64::NAN });
                on_grid.push(c);
            }
            None => {
                out += 1;
                values.push(f64::NAN);
                on_grid.push(f64::NAN);
            }
        }
    }
    Ok(Corrected {
        values,
        reference_on_grid: on_grid,
        out_of_reference: out,
        bridged,
        division_by_zero: by_zero,
    })
}

/// Does `x` fall between two kept reference points that surround a hole?
/// An `x` exactly on a kept point is exact, not bridged; outside the
/// domain, there is nothing to bridge.
fn bridges_hole(kept: &[f64], holes: &[f64], x: f64) -> bool {
    if holes.is_empty() {
        return false;
    }
    let hi = kept.partition_point(|&v| v < x);
    if hi == 0 || hi >= kept.len() || kept[hi] == x {
        return false;
    }
    let (lo, hi) = (kept[hi - 1], kept[hi]);
    holes.iter().any(|&h| h > lo && h < hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn division_by_a_reference_on_another_grid() {
        let data = Series::new(vec![1.0, 2.0, 3.0, 4.0], vec![10.0, 20.0, 30.0, 40.0]);
        // Linear reference 1 + 0.5·(x−1) sampled at the ends only.
        let reference = Series::new(vec![1.0, 5.0], vec![1.0, 3.0]);
        let c = correct(&data, &reference, &[], CorrectOp::Divide).unwrap();
        assert_eq!(c.out_of_reference, 0);
        let want = [10.0 / 1.0, 20.0 / 1.5, 30.0 / 2.0, 40.0 / 2.5];
        for (a, b) in c.values.iter().zip(want) {
            assert!((a - b).abs() < 1e-12, "{a} vs {b}");
        }
    }

    #[test]
    fn outside_the_domain_the_cell_is_empty_and_counted() {
        let data = Series::new(vec![0.0, 1.0, 2.0, 9.0], vec![1.0; 4]);
        let reference = Series::new(vec![1.0, 2.0], vec![2.0, 2.0]);
        let c = correct(&data, &reference, &[], CorrectOp::Multiply).unwrap();
        assert_eq!(c.out_of_reference, 2);
        assert!(c.values[0].is_nan() && c.values[3].is_nan());
        assert_eq!(c.values[1], 2.0);
        assert!(c.reference_on_grid[0].is_nan());
    }

    #[test]
    fn subtraction_and_unsorted_reference() {
        let data = Series::new(vec![1.0, 2.0], vec![5.0, 7.0]);
        let reference = Series::new(vec![2.0, 1.0], vec![3.0, 1.0]);
        let c = correct(&data, &reference, &[], CorrectOp::Subtract).unwrap();
        assert_eq!(c.values, vec![4.0, 4.0]);
    }

    #[test]
    fn division_by_zero_gives_an_empty_cell() {
        let data = Series::new(vec![1.0, 2.0], vec![5.0, 7.0]);
        let reference = Series::new(vec![1.0, 2.0], vec![0.0, 1.0]);
        let c = correct(&data, &reference, &[], CorrectOp::Divide).unwrap();
        assert!(c.values[0].is_nan());
        assert_eq!(c.values[1], 7.0);
        assert_eq!(c.out_of_reference, 0);
    }

    #[test]
    fn a_hole_in_the_reference_is_bridged_and_counted() {
        // Reference x=[1,2,3,4], y=[1,NaN,3,4]: the hole is at x = 2.
        let reference = Series::new(vec![1.0, 3.0, 4.0], vec![1.0, 3.0, 4.0]);
        let data = Series::new(vec![1.5, 2.0, 2.5, 3.5], vec![1.0; 4]);
        let c = correct(&data, &reference, &[2.0], CorrectOp::Multiply).unwrap();
        assert_eq!(c.bridged, 3);
        assert_eq!(c.values, vec![1.5, 2.0, 2.5, 3.5]);
        // Exactly on a kept point: not bridged.
        let on_kept = Series::new(vec![1.0, 3.0, 4.0], vec![1.0; 3]);
        let c = correct(&on_kept, &reference, &[2.0], CorrectOp::Multiply).unwrap();
        assert_eq!(c.bridged, 0);
    }

    #[test]
    fn a_hole_at_the_end_of_the_reference_bridges_nothing() {
        // y = [1, 2, 3, NaN]: the hole (x = 4) is outside the kept domain.
        let reference = Series::new(vec![1.0, 2.0, 3.0], vec![1.0, 2.0, 3.0]);
        let data = Series::new(vec![1.5, 2.5, 3.5], vec![1.0; 3]);
        let c = correct(&data, &reference, &[4.0], CorrectOp::Multiply).unwrap();
        assert_eq!((c.bridged, c.out_of_reference), (0, 1));
    }

    #[test]
    fn division_by_a_zero_reference_is_counted_not_the_overflow() {
        let data = Series::new(vec![1.0, 2.0, 3.0], vec![5.0, 1e300, 1.0]);
        let reference = Series::new(vec![1.0, 2.0, 3.0], vec![0.0, 1e-300, 1e-300]);
        let c = correct(&data, &reference, &[], CorrectOp::Divide).unwrap();
        assert_eq!(c.division_by_zero, 1);
        assert!(c.values[0].is_nan() && c.values[1].is_nan());
        assert!(c.values[2].is_finite() && c.values[2] > 1e299);
    }

    #[test]
    fn empty_reference() {
        let data = Series::new(vec![1.0], vec![1.0]);
        assert_eq!(
            correct(&data, &Series::default(), &[], CorrectOp::Divide).unwrap_err(),
            AnalysisError::EmptyReference
        );
    }
}
