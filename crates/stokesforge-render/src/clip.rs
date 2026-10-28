//! Geometric clipping of a polyline against the plot rectangle, in
//! **pixels** — fix for the defect documented in [`crate::paint`]
//! (section "Overflow was mishandled by Plotters") and postponed twice by `roadmap-rendu.md` (R5, then
//! R7).
//!
//! # Why in pixels, and not in data
//!
//! A logarithmic axis is linear only in the transformed space
//! ([`crate::scale::forward`]); a straight line in data coordinates is
//! therefore not necessarily a straight line in pixels, whereas it
//! **becomes** a straight line again once projected into pixels (the
//! pixel ↔ transformed projection is always affine — see
//! [`crate::interact`]). A segment/rectangle intersection is an operation on
//! straight lines: it must be done where the segment is actually straight,
//! that is after the projection, never before.
//!
//! # Why here, and not in a backend
//!
//! Same reason as for [`crate::paint`] and [`crate::pipeline`]: a single
//! implementation, called before the point reaches
//! `plotters::element::PathElement`, rather than a discipline to keep in
//! each of the two `DrawingBackend`s.
//!
//! # The algorithm
//!
//! Liang-Barsky, on an axis-aligned rectangle — simpler to get right than
//! Cohen-Sutherland for this case, and enough: the plot rectangle is never
//! tilted. [`clip_segment`] clips one segment; [`clip_polyline`] applies it
//! to each pair of consecutive vertices of a curve and joins the runs that
//! stay adjacent into a single sub-polyline, exactly like
//! [`crate::paint::dashes`] joins runs of dashes — a segment entirely
//! outside the rectangle is omitted, not brought back onto the edge.

use plotters_backend::BackendCoord;

/// The plot rectangle of a panel, in backend pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelBounds {
    pub x_min: f64,
    pub y_min: f64,
    pub x_max: f64,
    pub y_max: f64,
}

impl PixelBounds {
    /// The rectangle of a drawn panel, from its origin and its size in
    /// pixels ([`crate::pipeline::DrawnPanel::pixel_origin`] /
    /// `pixel_size`). `None` if the area is degenerate — same guard as
    /// [`crate::interact::PanelTransform::of`].
    pub fn of_panel(origin: (i32, i32), size: (u32, u32)) -> Option<Self> {
        if size.0 == 0 || size.1 == 0 {
            return None;
        }
        Some(Self {
            x_min: origin.0 as f64,
            y_min: origin.1 as f64,
            x_max: origin.0 as f64 + size.0 as f64,
            y_max: origin.1 as f64 + size.1 as f64,
        })
    }
}

/// Clips the segment `a`→`b` (in pixels) to the rectangle `bounds` with
/// Liang-Barsky. Returns the two ends of the remaining run, or `None` if the
/// segment does not cross the rectangle.
///
/// A non-finite point in `a` or `b` (`NaN`, infinity) makes the segment be
/// rejected rather than produce a non-finite result in turn — consistent
/// with how [`crate::series::curve_geometry`] already treats unrepresentable
/// values upstream (they never enter the geometry).
pub fn clip_segment(
    bounds: PixelBounds,
    a: (f64, f64),
    b: (f64, f64),
) -> Option<((f64, f64), (f64, f64))> {
    if !a.0.is_finite() || !a.1.is_finite() || !b.0.is_finite() || !b.1.is_finite() {
        return None;
    }
    let (x0, y0) = a;
    let (x1, y1) = b;
    let dx = x1 - x0;
    let dy = y1 - y0;

    let mut t0 = 0.0f64;
    let mut t1 = 1.0f64;

    // The four half-planes of the rectangle: left, right, top, bottom.
    let edges = [
        (-dx, x0 - bounds.x_min),
        (dx, bounds.x_max - x0),
        (-dy, y0 - bounds.y_min),
        (dy, bounds.y_max - y0),
    ];
    for (p, q) in edges {
        if p == 0.0 {
            // The segment is parallel to this edge: rejected if on the
            // wrong side, unchanged otherwise.
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let r = q / p;
        if p < 0.0 {
            if r > t1 {
                return None;
            }
            if r > t0 {
                t0 = r;
            }
        } else {
            if r < t0 {
                return None;
            }
            if r < t1 {
                t1 = r;
            }
        }
    }
    if t0 > t1 {
        return None;
    }
    Some(((x0 + t0 * dx, y0 + t0 * dy), (x0 + t1 * dx, y0 + t1 * dy)))
}

/// Clips a whole polyline against `bounds`, in integer pixels.
///
/// Returns one or more sub-polylines — several when the curve leaves the
/// rectangle and re-enters it, none if it is entirely outside. Two
/// consecutive clipped runs are joined into a single sub-polyline when the
/// second starts exactly where the first ends (the common vertex stayed
/// inside the rectangle); otherwise each forms its own sub-polyline.
pub fn clip_polyline(bounds: PixelBounds, points: &[BackendCoord]) -> Vec<Vec<BackendCoord>> {
    let mut runs: Vec<Vec<BackendCoord>> = Vec::new();
    for pair in points.windows(2) {
        let a = (pair[0].0 as f64, pair[0].1 as f64);
        let b = (pair[1].0 as f64, pair[1].1 as f64);
        let Some((ca, cb)) = clip_segment(bounds, a, b) else {
            continue;
        };
        let pa = round(ca);
        let pb = round(cb);
        if pa == pb {
            // The rectangle was only grazed at one point (a corner, or a
            // segment tangent to an edge): nothing to draw.
            continue;
        }
        match runs.last_mut() {
            Some(run) if run.last() == Some(&pa) => run.push(pb),
            _ => runs.push(vec![pa, pb]),
        }
    }
    runs
}

/// Like [`clip_polyline`], on **floating** pixel vertices: nothing is
/// rounded, for sub-pixel paths ([`crate::subpixel`]).
pub fn clip_polyline_f64(bounds: PixelBounds, points: &[(f64, f64)]) -> Vec<Vec<(f64, f64)>> {
    let mut runs: Vec<Vec<(f64, f64)>> = Vec::new();
    for pair in points.windows(2) {
        let Some((ca, cb)) = clip_segment(bounds, pair[0], pair[1]) else {
            continue;
        };
        if ca == cb {
            continue;
        }
        match runs.last_mut() {
            Some(run) if run.last() == Some(&ca) => run.push(cb),
            _ => runs.push(vec![ca, cb]),
        }
    }
    runs
}

fn round((x, y): (f64, f64)) -> BackendCoord {
    (x.round() as i32, y.round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rectangle 0..100 × 0..100, in pixels — the usual test case.
    fn square() -> PixelBounds {
        PixelBounds {
            x_min: 0.0,
            y_min: 0.0,
            x_max: 100.0,
            y_max: 100.0,
        }
    }

    #[test]
    fn a_segment_entirely_inside_is_unchanged() {
        let bounds = square();
        let result = clip_segment(bounds, (10.0, 20.0), (80.0, 90.0)).unwrap();
        assert_eq!(result, ((10.0, 20.0), (80.0, 90.0)));
    }

    #[test]
    fn a_segment_entirely_outside_is_dropped() {
        let bounds = square();
        // The whole line x = 200 is to the right of the rectangle.
        assert!(clip_segment(bounds, (200.0, -50.0), (200.0, 150.0)).is_none());
        // A purely diagonal segment, entirely beyond the corner.
        assert!(clip_segment(bounds, (150.0, 150.0), (300.0, 300.0)).is_none());
    }

    #[test]
    fn a_segment_crossing_the_frame_is_cut_at_the_right_point() {
        let bounds = square();
        // From (-50, 50) to (150, 50): horizontal, cuts x=0 and x=100 at
        // y = 50 on both sides — computable by hand.
        let (start, end) = clip_segment(bounds, (-50.0, 50.0), (150.0, 50.0)).unwrap();
        assert_eq!(start, (0.0, 50.0));
        assert_eq!(end, (100.0, 50.0));
    }

    #[test]
    fn a_diagonal_segment_is_cut_at_the_geometric_intersection() {
        let bounds = square();
        // From (50, 50) [inside] to (200, 200) [outside, diagonally]: the
        // line y = x leaves the square at (100, 100), not at (100,
        // something truncated per component) nor at (something, 100).
        let (start, end) = clip_segment(bounds, (50.0, 50.0), (200.0, 200.0)).unwrap();
        assert_eq!(start, (50.0, 50.0));
        assert_eq!(end, (100.0, 100.0));
    }

    #[test]
    fn a_segment_that_just_touches_an_edge_does_not_crash() {
        let bounds = square();
        // Tangent to the top edge (y = 0) without ever entering the square.
        let result = clip_segment(bounds, (-10.0, 0.0), (110.0, 0.0));
        assert_eq!(result, Some(((0.0, 0.0), (100.0, 0.0))));

        // One point exactly on a corner, the other far outside: the
        // clipped run is a single point (t0 == t1) — not a crash, and
        // `clip_polyline` must then produce nothing for this segment.
        let bounds_line = clip_segment(bounds, (100.0, 100.0), (200.0, 200.0));
        assert_eq!(bounds_line, Some(((100.0, 100.0), (100.0, 100.0))));
    }

    #[test]
    fn a_polyline_outside_the_frame_disappears_without_a_stray_segment() {
        let bounds = square();
        let points = [(200, 200), (300, 300), (400, 100)];
        assert!(clip_polyline(bounds, &points).is_empty());
    }

    #[test]
    fn a_polyline_that_leaves_and_reenters_gives_several_runs() {
        let bounds = square();
        // Inside, leaves on the right, comes back inside from the bottom: two
        // disjoint sub-polylines, neither pressed against an edge.
        let points = [(50, 50), (200, 50), (50, 200), (50, 50)];
        let runs = clip_polyline(bounds, &points);
        assert_eq!(runs.len(), 2, "{runs:?}");
        for run in &runs {
            for &(x, y) in run {
                assert!((0..=100).contains(&x) && (0..=100).contains(&y), "{run:?}");
            }
        }
    }

    #[test]
    fn a_polyline_entirely_inside_joins_all_its_runs() {
        let bounds = square();
        let points = [(10, 10), (50, 60), (90, 20)];
        let runs = clip_polyline(bounds, &points);
        assert_eq!(runs, vec![vec![(10, 10), (50, 60), (90, 20)]]);
    }

    #[test]
    fn the_rectangle_of_a_degenerate_area_is_absent() {
        assert!(PixelBounds::of_panel((0, 0), (0, 10)).is_none());
        assert!(PixelBounds::of_panel((0, 0), (10, 0)).is_none());
        assert!(PixelBounds::of_panel((5, 5), (10, 10)).is_some());
    }
}
