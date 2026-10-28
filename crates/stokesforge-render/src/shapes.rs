//! Drawing a panel's graphic objects — rectangle, ellipse, line, arrow
//! (issue #76) — and picking them under the pointer.
//!
//! Like all rendering, this is **here**, generic over the backend, and in
//! **pixels**: the object's two points are projected by the pipeline (with no
//! framing membership test: a rectangle that overflows the zoom is still
//! drawn, clipped), then this module builds the geometry and clips it
//! against the plot rectangle ([`crate::clip`]).
//!
//! - the **stroke** goes through [`clip_polyline`]: the outline of a rectangle
//!   with one side outside the frame is not flattened onto the edge;
//! - the **fill** is the polygon clipped by Sutherland-Hodgman
//!   ([`clip_polygon`]) — a closed ellipse outline, polygonal with
//!   [`ellipse_segments`] sides, is convex, which is enough for this algorithm;
//! - the **head** of an arrow is a solid triangle, omitted when the tip is
//!   outside the frame.

use plotters::coord::Shift;
use plotters::element::Polygon;
use plotters::prelude::*;
use plotters_backend::{BackendCoord, DrawingBackend};
use stokesforge_plot::{ShapeKind, ShapeSpec};

use crate::clip::{clip_polyline_f64, clip_segment, PixelBounds};
use crate::color::{parse_hex, rgba_or};
use crate::paint::{opaque, rgba, stroke_width};
use crate::subpixel;

/// Bounds of the number of sides of the polygon that approximates an ellipse.
const ELLIPSE_SEGMENTS_RANGE: (usize, usize) = (24, 2048);
/// Maximum sagitta (gap between the chord and the arc), in pixels, of the
/// ellipse polygon. The backend rounds coordinates to whole pixels, so we aim
/// well below half a pixel.
const ELLIPSE_SAGITTA: f64 = 0.1;

/// Sides of the polygon that approximates an ellipse of maximum semi-axis
/// `radius` pixels: enough that the chord never strays from the arc by more
/// than [`ELLIPSE_SAGITTA`] (issue #91: 72 fixed sides were visible on a large
/// circle). A regular inscribed polygon stays convex, whatever n.
pub fn ellipse_segments(radius: f64) -> usize {
    let (min, max) = ELLIPSE_SEGMENTS_RANGE;
    if !radius.is_finite() || radius <= ELLIPSE_SAGITTA {
        return min;
    }
    // sagitta = r (1 - cos(π/n))  ⇒  n = π / acos(1 - s/r)
    let n = std::f64::consts::PI / (1.0 - ELLIPSE_SAGITTA / radius).acos();
    (n.ceil() as usize).clamp(min, max)
}

/// Picking tolerance around the stroke, in pixels.
const HIT_TOLERANCE: f64 = 4.0;

/// A graphic object as it was drawn — enough to pick it under the pointer
/// ([`crate::hit`]) and give it its handles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawnShape {
    /// Rank in `PanelSpec::shapes`.
    pub index: usize,
    /// Its kind.
    pub kind: ShapeKind,
    /// First point, in backend pixels (unclipped).
    pub p1: (f64, f64),
    /// Second point, in backend pixels (unclipped).
    pub p2: (f64, f64),
    /// True if the interior is filled (hence pickable).
    pub filled: bool,
    /// Stroke width, in pixels.
    pub width: f64,
}

impl DrawnShape {
    /// True if pixel `(x, y)` picks the object: on its stroke, or, if it is
    /// filled, inside it.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let tolerance = HIT_TOLERANCE + self.width / 2.0;
        match self.kind {
            ShapeKind::Line | ShapeKind::Arrow => {
                distance_to_segment((x, y), self.p1, self.p2) <= tolerance
            }
            ShapeKind::Rectangle => {
                let (left, right) = ordered(self.p1.0, self.p2.0);
                let (top, bottom) = ordered(self.p1.1, self.p2.1);
                let outer = x >= left - tolerance
                    && x <= right + tolerance
                    && y >= top - tolerance
                    && y <= bottom + tolerance;
                if self.filled {
                    return outer;
                }
                let inner = x > left + tolerance
                    && x < right - tolerance
                    && y > top + tolerance
                    && y < bottom - tolerance;
                outer && !inner
            }
            ShapeKind::Ellipse => {
                let (left, right) = ordered(self.p1.0, self.p2.0);
                let (top, bottom) = ordered(self.p1.1, self.p2.1);
                let (a, b) = (
                    ((right - left) / 2.0).max(1.0),
                    ((bottom - top) / 2.0).max(1.0),
                );
                let (dx, dy) = (x - (left + right) / 2.0, y - (top + bottom) / 2.0);
                let radial = ((dx / a).powi(2) + (dy / b).powi(2)).sqrt();
                let slack = tolerance / a.min(b);
                if self.filled {
                    radial <= 1.0 + slack
                } else {
                    (radial - 1.0).abs() <= slack
                }
            }
        }
    }
}

fn ordered(a: f64, b: f64) -> (f64, f64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn distance_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length_sq = dx * dx + dy * dy;
    let t = if length_sq == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / length_sq).clamp(0.0, 1.0)
    };
    ((p.0 - (a.0 + t * dx)).hypot(p.1 - (a.1 + t * dy))).abs()
}

/// The vertices, in pixels, of the closed outline of a rectangle or an
/// ellipse between `p1` and `p2` (the last vertex is not repeated). Empty for
/// a line or an arrow.
pub fn outline(kind: ShapeKind, p1: (f64, f64), p2: (f64, f64)) -> Vec<(f64, f64)> {
    let (left, right) = ordered(p1.0, p2.0);
    let (top, bottom) = ordered(p1.1, p2.1);
    match kind {
        ShapeKind::Rectangle => vec![(left, top), (right, top), (right, bottom), (left, bottom)],
        ShapeKind::Ellipse => {
            let (cx, cy) = ((left + right) / 2.0, (top + bottom) / 2.0);
            let (a, b) = ((right - left) / 2.0, (bottom - top) / 2.0);
            let segments = ellipse_segments(a.max(b));
            (0..segments)
                .map(|i| {
                    let angle = std::f64::consts::TAU * i as f64 / segments as f64;
                    (cx + a * angle.cos(), cy + b * angle.sin())
                })
                .collect()
        }
        ShapeKind::Line | ShapeKind::Arrow => Vec::new(),
    }
}

/// The head of an arrow: a triangle whose apex is `tip`, open toward
/// `tail`. Length and width grow with the stroke width. `None` if the arrow
/// is reduced to a point.
pub fn arrow_head(tail: (f64, f64), tip: (f64, f64), width: f64) -> Option<[(f64, f64); 3]> {
    let (dx, dy) = (tip.0 - tail.0, tip.1 - tail.1);
    let length = dx.hypot(dy);
    if !length.is_finite() || length < 1.0 {
        return None;
    }
    let (ux, uy) = (dx / length, dy / length);
    // Never longer than half the arrow: a small arrow stays an
    // arrow.
    let head = (8.0 + 3.0 * width).min(length / 2.0);
    let half = head * 0.4;
    let base = (tip.0 - ux * head, tip.1 - uy * head);
    Some([
        tip,
        (base.0 - uy * half, base.1 + ux * half),
        (base.0 + uy * half, base.1 - ux * half),
    ])
}

/// Clips a polygon against the rectangle `bounds` (Sutherland-Hodgman).
pub fn clip_polygon(bounds: PixelBounds, polygon: &[(f64, f64)]) -> Vec<(f64, f64)> {
    type Edge = (
        fn((f64, f64), PixelBounds) -> bool,
        fn((f64, f64), (f64, f64), PixelBounds) -> (f64, f64),
    );
    fn at_x(a: (f64, f64), b: (f64, f64), x: f64) -> (f64, f64) {
        let t = (x - a.0) / (b.0 - a.0);
        (x, a.1 + t * (b.1 - a.1))
    }
    fn at_y(a: (f64, f64), b: (f64, f64), y: f64) -> (f64, f64) {
        let t = (y - a.1) / (b.1 - a.1);
        (a.0 + t * (b.0 - a.0), y)
    }
    let edges: [Edge; 4] = [
        (|p, bd| p.0 >= bd.x_min, |a, b, bd| at_x(a, b, bd.x_min)),
        (|p, bd| p.0 <= bd.x_max, |a, b, bd| at_x(a, b, bd.x_max)),
        (|p, bd| p.1 >= bd.y_min, |a, b, bd| at_y(a, b, bd.y_min)),
        (|p, bd| p.1 <= bd.y_max, |a, b, bd| at_y(a, b, bd.y_max)),
    ];
    let mut current: Vec<(f64, f64)> = polygon
        .iter()
        .copied()
        .filter(|p| p.0.is_finite() && p.1.is_finite())
        .collect();
    for (inside, cut) in edges {
        if current.is_empty() {
            break;
        }
        let mut next = Vec::with_capacity(current.len() + 2);
        for (i, &b) in current.iter().enumerate() {
            let a = current[(i + current.len() - 1) % current.len()];
            match (inside(a, bounds), inside(b, bounds)) {
                (true, true) => next.push(b),
                (true, false) => next.push(cut(a, b, bounds)),
                (false, true) => {
                    next.push(cut(a, b, bounds));
                    next.push(b);
                }
                (false, false) => {}
            }
        }
        current = next;
    }
    current
}

fn rounded(point: (f64, f64)) -> BackendCoord {
    (point.0.round() as i32, point.1.round() as i32)
}

/// The path that loops over `polygon`, in floating-point pixels.
///
/// A path whose last point repeats the first stays an **open** polyline for
/// the backends: at the start point, two butt-capped ends touch without a
/// join, hence a notch at the corner (#92). So the seam is put at the
/// **middle of the first side**, where the two ends are aligned and rejoin
/// without a gap.
fn closed_path(polygon: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let [first, second, ..] = polygon else {
        return polygon.to_vec();
    };
    let seam = ((first.0 + second.0) / 2.0, (first.1 + second.1) / 2.0);
    let mut path = vec![seam];
    path.extend_from_slice(&polygon[1..]);
    path.push(*first);
    path.push(seam);
    path
}

/// Draws a sub-pixel path ([`crate::subpixel`], #91).
fn draw_subpixel_path<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    points: &[(f64, f64)],
    stroke: ShapeStyle,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    area.draw(&subpixel::SubpixelPath::new(points, stroke))
}

/// Draws an object between pixels `p1` and `p2`, clipped to `bounds` if there
/// is one. `ink` is the color of an automatic stroke.
pub fn draw_shape<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    shape: &ShapeSpec,
    p1: (f64, f64),
    p2: (f64, f64),
    bounds: Option<PixelBounds>,
    ink: (u8, u8, u8),
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    let stroke = ShapeStyle {
        color: rgba(rgba_or(&shape.line_color, opaque(ink))),
        filled: false,
        stroke_width: stroke_width(shape.effective_width()),
    };
    if shape.kind.is_closed() {
        let polygon = outline(shape.kind, p1, p2);
        if let Some(fill) = parse_hex(&shape.fill_color) {
            let visible = match bounds {
                Some(bounds) => clip_polygon(bounds, &polygon),
                None => polygon.clone(),
            };
            if visible.len() >= 3 {
                let style = ShapeStyle {
                    color: rgba(fill),
                    filled: true,
                    stroke_width: 1,
                };
                area.draw(&Polygon::new(
                    visible.into_iter().map(rounded).collect::<Vec<_>>(),
                    style,
                ))?;
            }
        }
        let closed = closed_path(&polygon);
        match bounds {
            Some(bounds) => {
                for run in clip_polyline_f64(bounds, &closed) {
                    draw_subpixel_path(area, &run, stroke)?;
                }
            }
            None => draw_subpixel_path(area, &closed, stroke)?,
        }
        return Ok(());
    }
    // Line or arrow.
    let segment = match bounds {
        Some(bounds) => clip_segment(bounds, p1, p2),
        None => Some((p1, p2)),
    };
    if let Some((a, b)) = segment {
        if rounded(a) != rounded(b) {
            draw_subpixel_path(area, &[a, b], stroke)?;
        }
    }
    if shape.kind == ShapeKind::Arrow {
        let tip_visible = bounds.is_none_or(|bounds| {
            p2.0 >= bounds.x_min
                && p2.0 <= bounds.x_max
                && p2.1 >= bounds.y_min
                && p2.1 <= bounds.y_max
        });
        if let (true, Some(head)) = (tip_visible, arrow_head(p1, p2, shape.effective_width())) {
            let style = ShapeStyle {
                color: stroke.color,
                filled: true,
                stroke_width: 1,
            };
            area.draw(&Polygon::new(head.map(rounded).to_vec(), style))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> PixelBounds {
        PixelBounds {
            x_min: 0.0,
            y_min: 0.0,
            x_max: 100.0,
            y_max: 100.0,
        }
    }

    fn drawn(kind: ShapeKind, filled: bool) -> DrawnShape {
        DrawnShape {
            index: 0,
            kind,
            p1: (10.0, 10.0),
            p2: (50.0, 40.0),
            filled,
            width: 1.5,
        }
    }

    #[test]
    fn an_empty_rectangle_is_picked_on_its_stroke_not_inside() {
        let rect = drawn(ShapeKind::Rectangle, false);
        assert!(rect.contains(10.0, 25.0), "left side");
        assert!(rect.contains(30.0, 40.0), "bottom side");
        assert!(!rect.contains(30.0, 25.0), "empty interior");
        assert!(!rect.contains(80.0, 25.0), "outside");
        assert!(drawn(ShapeKind::Rectangle, true).contains(30.0, 25.0));
    }

    #[test]
    fn an_ellipse_is_picked_on_its_outline() {
        let ellipse = drawn(ShapeKind::Ellipse, false);
        assert!(ellipse.contains(50.0, 25.0), "right end");
        assert!(ellipse.contains(30.0, 10.0), "top");
        assert!(!ellipse.contains(30.0, 25.0), "center");
        assert!(!ellipse.contains(10.0, 10.0), "corner of the box");
        assert!(drawn(ShapeKind::Ellipse, true).contains(30.0, 25.0));
    }

    #[test]
    fn a_line_is_picked_within_a_few_pixels() {
        let line = drawn(ShapeKind::Line, false);
        assert!(line.contains(30.0, 25.0), "middle of the segment");
        assert!(line.contains(30.0, 28.0));
        assert!(!line.contains(30.0, 40.0));
        assert!(!line.contains(80.0, 55.0), "beyond the end");
    }

    #[test]
    fn the_ellipse_keeps_the_gap_to_the_arc_under_the_tolerance_whatever_the_radius() {
        for radius in [5.0, 40.0, 150.0, 600.0, 3000.0] {
            let polygon = outline(ShapeKind::Ellipse, (0.0, 0.0), (2.0 * radius, 2.0 * radius));
            assert!(polygon.len() >= ELLIPSE_SEGMENTS_RANGE.0, "{radius}");
            // The middle of each chord is the point farthest from the arc.
            let worst = polygon
                .iter()
                .zip(polygon.iter().cycle().skip(1))
                .map(|(a, b)| {
                    let mid = ((a.0 + b.0) / 2.0 - radius, (a.1 + b.1) / 2.0 - radius);
                    radius - mid.0.hypot(mid.1)
                })
                .fold(0.0_f64, f64::max);
            assert!(worst <= ELLIPSE_SAGITTA * 1.01, "radius {radius}: {worst}");
        }
        assert!(ellipse_segments(600.0) > ellipse_segments(40.0));
        assert_eq!(ellipse_segments(f64::NAN), ELLIPSE_SEGMENTS_RANGE.0);
    }

    #[test]
    fn the_outline_seam_is_in_the_middle_of_a_side_not_at_a_corner() {
        // #92: start and end at the same vertex = two butt-capped ends without
        // a join, hence a notch at the top-left corner.
        let rect = outline(ShapeKind::Rectangle, (10.0, 10.0), (50.0, 30.0));
        let path = closed_path(&rect);
        assert_eq!(path.first(), path.last(), "closed path");
        assert_eq!(path[0], (30.0, 10.0), "seam in the middle of the top side");
        for corner in &rect {
            assert!(
                path[1..path.len() - 1].contains(corner),
                "the four corners stay interior vertices (join)"
            );
        }
    }

    #[test]
    fn the_polygon_is_clipped_to_the_frame() {
        let rect = outline(ShapeKind::Rectangle, (-50.0, 20.0), (50.0, 200.0));
        let clipped = clip_polygon(square(), &rect);
        assert!(!clipped.is_empty());
        for (x, y) in &clipped {
            assert!(
                (0.0..=100.0).contains(x) && (0.0..=100.0).contains(y),
                "{x},{y}"
            );
        }
        let outside = outline(ShapeKind::Rectangle, (200.0, 200.0), (300.0, 300.0));
        assert!(clip_polygon(square(), &outside).is_empty());
    }

    #[test]
    fn the_head_is_at_the_arrow_tip_and_never_longer_than_half() {
        let head = arrow_head((0.0, 0.0), (100.0, 0.0), 1.5).unwrap();
        assert_eq!(head[0], (100.0, 0.0));
        assert!(head[1].0 < 100.0 && head[2].0 < 100.0);
        assert!((head[1].1 + head[2].1).abs() < 1e-9, "symmetric");
        let small = arrow_head((0.0, 0.0), (6.0, 0.0), 1.5).unwrap();
        assert!(small[1].0 >= 3.0 - 1e-9, "head capped at half");
        assert!(arrow_head((5.0, 5.0), (5.0, 5.0), 1.5).is_none());
    }
}
