//! LTTB decimation — task R5.5.
//!
//! # What the R0 0.3 measurement says, and what it does not
//!
//! The decision threshold was fixed in advance (8 ms per tessellation
//! frame), it is crossed (22.1 ms), so LTTB enters R5: we conclude according
//! to the threshold rather than renegotiate it afterwards. **But the same
//! measurement shows that LTTB does not fix the measured case**, and it is
//! written here rather than diluted in a summary:
//!
//! | series | points/series | total | tessellation | what LTTB removes |
//! |---|---|---|---|---|
//! | 50 | 6,000 | 300 k | 18.3 ms | ~73 % of the points (≈ 6.5 ms) |
//! | 500 | 600 | 300 k | 22.7 ms | **nothing** |
//!
//! 600 points spread over 1,600 px are already undersampled: there is
//! nothing to decimate. The document's load case — 500 spectra of 600
//! points — gets its load from the **number of series**, not their length,
//! and the lever that covers it is *feathering* (78 % of the tessellation
//! cost, `stokesforge_render_egui::feathering`), not this one.
//!
//! This module therefore serves **the other case**, the acquisition one: a
//! few very long series. There it is the right tool, and the only one that
//! asks nothing of the rest of the architecture.
//!
//! # Why LTTB and not "one point in N"
//!
//! *Largest Triangle Three Buckets* picks, in each bucket, the point that
//! forms the largest triangle with the point kept on the left and the mean of
//! the bucket on the right. Concretely, it **keeps the peaks**: an artifact
//! of a single measurement — exactly what a spectrum is for seeing —
//! survives the decimation, where a fixed step erases it every other time.
//!
//! Two properties held, and tested:
//!
//! - **no invented value**: every output is an input point, never a mean. A
//!   crosshair that designates a point designates a real measurement (what
//!   phase R7 will need);
//! - **the endpoints are kept**, so the axis framing does not move because
//!   of the decimation.
//!
//! # Where it applies, and where it does not
//!
//! In the pipeline, just before drawing, because the threshold depends on the
//! **width in pixels** of the plot area — information that does not exist
//! before Plotters has distributed the margins. It applies to the series that
//! draw a line and nothing else: not to bars (whose number is already that of
//! the bins), not to series with error bars (decimating would dissociate the
//! whiskers from their points).

/// Number of points below which decimating makes no sense.
///
/// The algorithm itself needs at least three points (two endpoints and one
/// bucket); below that it returns the input as is.
pub const MIN_POINTS: usize = 3;

/// How many points per pixel of width we accept to keep.
///
/// Two: an anti-aliased plot still distinguishes a back-and-forth within a
/// single pixel, and going down to one would lose the visual thickness of a
/// noisy signal. Beyond two, we pay for vertices the screen cannot show.
pub const POINTS_PER_PIXEL: usize = 2;

/// The decimation threshold for a plot area `width` pixels wide.
pub fn threshold_for_width(width: u32) -> usize {
    (width as usize)
        .saturating_mul(POINTS_PER_PIXEL)
        .max(MIN_POINTS)
}

/// Decimates `points` to at most `threshold` points, by *Largest Triangle
/// Three Buckets*.
///
/// Returns the input as is — without allocating other than the output `Vec`
/// — when it is already short enough, or when the requested threshold is too
/// small for the algorithm to make sense.
pub fn lttb(points: &[(f64, f64)], threshold: usize) -> Vec<(f64, f64)> {
    if threshold < MIN_POINTS || points.len() <= threshold {
        return points.to_vec();
    }

    let n = points.len();
    let last = n - 1;
    // Width of a bucket, in number of points. Fractional on purpose:
    // rounding here would make the buckets drift and lose the last points
    // in the last one.
    let bucket = (n - 2) as f64 / (threshold - 2) as f64;

    let mut out = Vec::with_capacity(threshold);
    out.push(points[0]);

    // Point kept on the left: the vertex common to all the triangles of the
    // current bucket.
    let mut anchor = 0usize;

    for i in 0..threshold - 2 {
        // Mean of the **next** bucket, which serves as the third vertex.
        let next_start = (((i + 1) as f64 * bucket).floor() as usize + 1).min(last);
        let next_end = (((i + 2) as f64 * bucket).floor() as usize + 1).min(n);
        let (mean_x, mean_y) = mean(&points[next_start..next_end.max(next_start)])
            .unwrap_or((points[last].0, points[last].1));

        // Current bucket, in which we choose.
        let start = ((i as f64 * bucket).floor() as usize + 1).min(last);
        let end = (((i + 1) as f64 * bucket).floor() as usize + 1).min(last);

        let (ax, ay) = points[anchor];
        let mut best = start;
        let mut best_area = f64::NEG_INFINITY;
        for (offset, &(x, y)) in points[start..end.max(start)].iter().enumerate() {
            // Twice the area of the triangle (anchor, candidate, mean): the
            // factor 1/2 is common to all candidates, hence useless.
            let area = ((ax - mean_x) * (y - ay) - (ax - x) * (mean_y - ay)).abs();
            if area > best_area {
                best_area = area;
                best = start + offset;
            }
        }
        out.push(points[best]);
        anchor = best;
    }

    out.push(points[last]);
    out
}

/// Mean of a bucket, or `None` if it is empty.
fn mean(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    if points.is_empty() {
        return None;
    }
    let (mut sx, mut sy) = (0.0, 0.0);
    for (x, y) in points {
        sx += x;
        sy += y;
    }
    let n = points.len() as f64;
    Some((sx / n, sy / n))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(n: usize) -> Vec<(f64, f64)> {
        (0..n).map(|i| (i as f64, (i as f64).sin())).collect()
    }

    #[test]
    fn an_already_short_series_comes_out_intact() {
        let points = ramp(10);
        assert_eq!(lttb(&points, 10), points);
        assert_eq!(lttb(&points, 100), points);
    }

    #[test]
    fn an_absurd_threshold_does_not_decimate() {
        let points = ramp(1000);
        assert_eq!(lttb(&points, 0).len(), 1000);
        assert_eq!(lttb(&points, 2).len(), 1000);
    }

    #[test]
    fn the_number_of_points_is_exactly_the_threshold() {
        for threshold in [3usize, 17, 100, 999] {
            assert_eq!(
                lttb(&ramp(10_000), threshold).len(),
                threshold,
                "threshold {threshold}"
            );
        }
    }

    #[test]
    fn the_endpoints_are_kept() {
        let points = ramp(5000);
        let decimated = lttb(&points, 50);
        assert_eq!(decimated.first().copied(), points.first().copied());
        assert_eq!(decimated.last().copied(), points.last().copied());
    }

    #[test]
    fn no_value_is_invented() {
        // The property that distinguishes LTTB from averaging: every
        // returned point is a measured point. R7 will depend on it for point
        // picking.
        let points = ramp(2000);
        let decimated = lttb(&points, 61);
        for point in &decimated {
            assert!(points.contains(point), "{point:?} is not an input point");
        }
    }

    #[test]
    fn the_abscissa_stays_increasing() {
        let points = ramp(3000);
        let decimated = lttb(&points, 120);
        assert!(
            decimated.windows(2).all(|p| p[0].0 < p[1].0),
            "LTTB must keep the traversal order"
        );
    }

    #[test]
    fn an_isolated_peak_survives_decimation() {
        // The reason for LTTB compared with "one point in N": an artifact of
        // a single measurement — what a spectrum is for seeing — must not
        // disappear.
        let mut points: Vec<(f64, f64)> = (0..1000).map(|i| (i as f64, 0.0)).collect();
        points[437].1 = 42.0;
        let decimated = lttb(&points, 50);
        assert!(
            decimated.contains(&(437.0, 42.0)),
            "the peak was erased by the decimation"
        );
    }

    #[test]
    fn the_threshold_follows_the_width() {
        assert_eq!(threshold_for_width(800), 1600);
        // A degenerate plot area must not give a threshold of zero, which
        // would fall back on "no decimation" by accident.
        assert_eq!(threshold_for_width(0), MIN_POINTS);
    }
}
