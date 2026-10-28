//! Smoothing of a curve (#324): adjacent average, Savitzky-Golay, rolling
//! median.
//!
//! Smoothing reads the neighbours **by rank** after sorting by X, not by distance:
//! a window of `n` points is `n` consecutive rows of the sorted curve. The
//! window is given in points (`Points`) or in X units (`XUnits`); in X units,
//! it is converted to points by the **median** X step (zero steps,
//! repeated X, do not count), so that both forms share a single
//! calculation. In both cases it is **odd**: a centerd smoothing has as many
//! neighbours on each side, so an even window is refused (`Points`) or
//! raised to the next odd number (`XUnits`, coming from a rounding).
//!
//! At the ends, the curve is extended by **reflection** (the neighbour beyond the
//! first point is the second one, etc.): the output has as many points as
//! the input, without the bias of a shrinking window. A window wider than the
//! curve is brought down to the largest odd number that fits (at least 1): smoothing a
//! short curve gives a result, not an error.
//!
//! Savitzky-Golay assumes a **uniform** step (the coefficients are those of a
//! polynomial fit on equidistant abscissae): a step that varies by
//! more than 1 % is reported (`step_uniform` false), the calculation is done anyway.

use crate::error::AnalysisError;
use crate::linalg::{solve, Matrix};
use crate::series::Series;

/// The smoothing method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmoothMethod {
    /// Mean of the points of the window.
    AdjacentAverage,
    /// Polynomial of order `order` fitted on the window, evaluated at its center.
    SavitzkyGolay { order: usize },
    /// Median of the points of the window (robust to isolated peaks).
    Median,
}

/// The window width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SmoothWindow {
    /// Number of points, odd.
    Points(usize),
    /// Width in X units, converted to points by the median step.
    XUnits(f64),
}

/// The settings of a smoothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothSpec {
    pub method: SmoothMethod,
    pub window: SmoothWindow,
}

/// Cap on the window, in points: a window read from a file must not allocate
/// anything absurd (`validate`, like `MAX_DEGREE`).
pub const MAX_WINDOW: usize = 1001;
/// Minimum and maximum order of a Savitzky-Golay.
pub const MIN_ORDER: usize = 2;
pub const MAX_ORDER: usize = 5;
/// Minimum window of a smoothing, in points.
pub const MIN_WINDOW: usize = 3;

impl SmoothSpec {
    /// Refuses settings above the cap before any calculation.
    pub fn validate(&self) -> Result<(), AnalysisError> {
        let bad = |name: &str| {
            Err(AnalysisError::InvalidParameter {
                name: name.to_string(),
            })
        };
        match self.window {
            SmoothWindow::Points(n) if !(MIN_WINDOW..=MAX_WINDOW).contains(&n) || n % 2 == 0 => {
                return bad("window");
            }
            SmoothWindow::XUnits(w) if !(w.is_finite() && w > 0.0) => return bad("window"),
            _ => {}
        }
        if let SmoothMethod::SavitzkyGolay { order } = self.method {
            if !(MIN_ORDER..=MAX_ORDER).contains(&order) {
                return bad("order");
            }
            if let SmoothWindow::Points(n) = self.window {
                if order >= n {
                    return bad("order");
                }
            }
        }
        Ok(())
    }
}

/// Result of a smoothing, row by row of the input series.
#[derive(Debug, Clone, PartialEq)]
pub struct Smoothed {
    pub values: Vec<f64>,
    /// The window actually used, in points (odd).
    pub window: usize,
    /// The X steps are equal to within 1 %; otherwise Savitzky-Golay is only
    /// approximate (the caller reports it for that method only).
    pub step_uniform: bool,
}

/// Smooths `data` (finite points, any order).
pub fn smooth(data: &Series, spec: &SmoothSpec) -> Result<Smoothed, AnalysisError> {
    spec.validate()?;
    let n = data.len();
    if n == 0 {
        return Err(AnalysisError::NotEnoughPoints {
            found: 0,
            needed: 1,
        });
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| data.x[a].total_cmp(&data.x[b]));
    let x: Vec<f64> = order.iter().map(|&i| data.x[i]).collect();
    let y: Vec<f64> = order.iter().map(|&i| data.y[i]).collect();

    let requested = match spec.window {
        SmoothWindow::Points(p) => p,
        SmoothWindow::XUnits(w) => {
            let step = median_step(&x).ok_or(AnalysisError::InvalidParameter {
                name: "window".to_string(),
            })?;
            let points = (w / step).round().clamp(1.0, MAX_WINDOW as f64) as usize;
            (points | 1).max(MIN_WINDOW)
        }
    };
    // A window wider than the curve: the largest odd number that fits.
    let window = requested.min(if n % 2 == 1 { n } else { n - 1 }).max(1);
    let half = window / 2;

    let smoothed_sorted = match spec.method {
        SmoothMethod::AdjacentAverage => convolve(&y, half, &vec![1.0 / window as f64; window]),
        SmoothMethod::SavitzkyGolay { order } => {
            // A polynomial of order ≥ window passes through all the points: the
            // window is raised to `order + 1` (odd) when the curve allows it.
            let kernel = savitzky_golay_kernel(half, order.min(window - 1))?;
            convolve(&y, half, &kernel)
        }
        SmoothMethod::Median => (0..n).map(|i| window_median(&y, i, half)).collect(),
    };
    let mut values = vec![f64::NAN; n];
    for (rank, &row) in order.iter().enumerate() {
        values[row] = smoothed_sorted[rank];
    }
    Ok(Smoothed {
        values,
        window,
        step_uniform: uniform_step(&x),
    })
}

/// The median step of the sorted X, without zero steps; `None` with no positive step.
fn median_step(x: &[f64]) -> Option<f64> {
    let mut steps: Vec<f64> = x
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|d| *d > 0.0 && d.is_finite())
        .collect();
    if steps.is_empty() {
        return None;
    }
    steps.sort_by(f64::total_cmp);
    Some(steps[steps.len() / 2])
}

/// Are the steps of the sorted X equal to within 1 % of the median step?
fn uniform_step(x: &[f64]) -> bool {
    match median_step(x) {
        None => true,
        Some(step) => x
            .windows(2)
            .all(|w| ((w[1] - w[0]) - step).abs() <= 0.01 * step),
    }
}

/// The index `i` brought back into `0..n` by reflection without repeating the edge.
fn reflect(i: isize, n: usize) -> usize {
    if n == 1 {
        return 0;
    }
    let period = 2 * (n as isize - 1);
    let m = i.rem_euclid(period);
    if m < n as isize {
        m as usize
    } else {
        (period - m) as usize
    }
}

/// Convolution by `kernel` (`2·half + 1` coefficients, from the oldest to the
/// newest), reflected edges.
fn convolve(y: &[f64], half: usize, kernel: &[f64]) -> Vec<f64> {
    let n = y.len();
    (0..n)
        .map(|i| {
            kernel
                .iter()
                .enumerate()
                .map(|(k, c)| c * y[reflect(i as isize + k as isize - half as isize, n)])
                .sum()
        })
        .collect()
}

fn window_median(y: &[f64], i: usize, half: usize) -> f64 {
    let n = y.len();
    let mut w: Vec<f64> = (0..=2 * half)
        .map(|k| y[reflect(i as isize + k as isize - half as isize, n)])
        .collect();
    w.sort_by(f64::total_cmp);
    w[half]
}

/// The Savitzky-Golay coefficients (smoothing, derivative 0) of a window of
/// `2·half + 1` points and a polynomial of order `order`: the first row of
/// `(AᵀA)⁻¹Aᵀ` with `A[j][k] = j^k`, j from `-half` to `half`. The abscissae are
/// scaled down by `half` to keep the system well conditioned.
fn savitzky_golay_kernel(half: usize, order: usize) -> Result<Vec<f64>, AnalysisError> {
    let size = 2 * half + 1;
    if half == 0 {
        return Ok(vec![1.0]);
    }
    let cols = order + 1;
    let mut a = Matrix::zeros(size, cols);
    for r in 0..size {
        let t = (r as f64 - half as f64) / half as f64;
        let mut p = 1.0;
        for c in 0..cols {
            a.set(r, c, p);
            p *= t;
        }
    }
    // c = e₀ᵀ (AᵀA)⁻¹ Aᵀ: we solve (AᵀA) u = e₀, then c = A u.
    let gram = a.gram();
    let mut e0 = vec![0.0; cols];
    e0[0] = 1.0;
    let u = solve(&gram, &e0).ok_or(AnalysisError::SingularSystem)?;
    Ok(a.mul_vec(&u))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(n: usize) -> Series {
        let x: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let y = x.iter().map(|x| 2.0 * x + 1.0).collect();
        Series::new(x, y)
    }

    fn spec(method: SmoothMethod, points: usize) -> SmoothSpec {
        SmoothSpec {
            method,
            window: SmoothWindow::Points(points),
        }
    }

    #[test]
    fn reflection_does_not_repeat_the_edge() {
        let indices: Vec<usize> = (-3..8).map(|i| reflect(i, 5)).collect();
        assert_eq!(indices, [3, 2, 1, 0, 1, 2, 3, 4, 3, 2, 1]);
        assert_eq!(reflect(-4, 1), 0);
    }

    #[test]
    fn adjacent_average_keeps_a_line_inside_and_adjusts_the_edges() {
        let s = smooth(&line(9), &spec(SmoothMethod::AdjacentAverage, 3)).unwrap();
        // In the middle, the mean of three aligned points is the central point.
        assert!((s.values[4] - 9.0).abs() < 1e-12);
        // At the edge, reflection replaces the missing neighbour (point 1): (3+1+3)/3.
        assert!((s.values[0] - 7.0 / 3.0).abs() < 1e-12, "{:?}", s.values);
        assert_eq!(s.window, 3);
    }

    /// Witness: order-2 Savitzky-Golay returns a degree-2 polynomial
    /// **exactly** (away from the edges), where the average distorts it.
    #[test]
    fn savitzky_golay_keeps_a_parabola_that_the_average_distorts() {
        let x: Vec<f64> = (0..21).map(f64::from).collect();
        let y: Vec<f64> = x.iter().map(|x| 0.5 * x * x - 3.0 * x + 2.0).collect();
        let data = Series::new(x, y.clone());
        let sg = smooth(&data, &spec(SmoothMethod::SavitzkyGolay { order: 2 }, 7)).unwrap();
        for i in 3..18 {
            assert!((sg.values[i] - y[i]).abs() < 1e-9, "i={i}");
        }
        let avg = smooth(&data, &spec(SmoothMethod::AdjacentAverage, 7)).unwrap();
        assert!((avg.values[10] - y[10]).abs() > 1.0, "the average distorts");
    }

    #[test]
    fn the_median_drops_an_isolated_peak() {
        let mut data = line(11);
        data.y[5] = 1000.0;
        let s = smooth(&data, &spec(SmoothMethod::Median, 3)).unwrap();
        assert!((s.values[5] - 13.0).abs() < 1e-12, "{}", s.values[5]);
    }

    #[test]
    fn the_output_follows_the_input_rows_even_unsorted() {
        let data = Series::new(vec![3.0, 1.0, 2.0, 0.0], vec![30.0, 10.0, 20.0, 0.0]);
        let s = smooth(&data, &spec(SmoothMethod::Median, 3)).unwrap();
        // Sorted: 0,10,20,30 -> reflected medians 10,10,20,20; put back on their row.
        assert_eq!(s.values, [20.0, 10.0, 20.0, 10.0]);
    }

    #[test]
    fn a_window_in_x_units_is_converted_by_the_median_step() {
        let data = line(30);
        let in_x = SmoothSpec {
            method: SmoothMethod::AdjacentAverage,
            window: SmoothWindow::XUnits(4.0),
        };
        // 4 units at step 1: 4 points, raised to 5 (odd).
        assert_eq!(smooth(&data, &in_x).unwrap().window, 5);
        let five = smooth(&data, &spec(SmoothMethod::AdjacentAverage, 5)).unwrap();
        assert_eq!(smooth(&data, &in_x).unwrap().values, five.values);
    }

    #[test]
    fn a_window_that_is_too_wide_is_brought_down_to_the_curve() {
        let s = smooth(&line(6), &spec(SmoothMethod::AdjacentAverage, 101)).unwrap();
        assert_eq!(s.window, 5);
        assert_eq!(s.values.len(), 6);
        let one = smooth(
            &Series::new(vec![0.0], vec![4.0]),
            &spec(SmoothMethod::Median, 5),
        )
        .unwrap();
        assert_eq!(one.values, [4.0]);
    }

    #[test]
    fn settings_above_the_cap_are_refused() {
        for bad in [
            spec(SmoothMethod::AdjacentAverage, 4),
            spec(SmoothMethod::AdjacentAverage, 1),
            spec(SmoothMethod::AdjacentAverage, MAX_WINDOW + 2),
            spec(SmoothMethod::SavitzkyGolay { order: 6 }, 9),
            spec(SmoothMethod::SavitzkyGolay { order: 1 }, 9),
            spec(SmoothMethod::SavitzkyGolay { order: 5 }, 5),
            SmoothSpec {
                method: SmoothMethod::Median,
                window: SmoothWindow::XUnits(f64::NAN),
            },
            SmoothSpec {
                method: SmoothMethod::Median,
                window: SmoothWindow::XUnits(-1.0),
            },
        ] {
            assert!(
                matches!(bad.validate(), Err(AnalysisError::InvalidParameter { .. })),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_non_uniform_step_is_reported() {
        let data = Series::new(vec![0.0, 1.0, 2.0, 4.0, 8.0, 9.0, 10.0], vec![0.0; 7]);
        let s = smooth(&data, &spec(SmoothMethod::SavitzkyGolay { order: 2 }, 5)).unwrap();
        assert!(!s.step_uniform);
        assert!(
            smooth(&line(9), &spec(SmoothMethod::Median, 3))
                .unwrap()
                .step_uniform
        );
    }
}
