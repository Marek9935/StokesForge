//! How a drag of a handle that may grow the page follows the pointer (#512).
//!
//! The handle (the outer edge of a regular grid, or an internal line pulled
//! with Alt) stays under the cursor for the whole drag, outward and inward, and
//! the opposite page edge (the anchor) does not move on the screen. When the
//! handle reaches the edge of the drawing area, pushing further still grows the
//! page 1:1 with the outward movement of the pointer, and the display zooms out
//! (one uniform scale, never zooming back in during the drag) so the whole page
//! stays visible. While the pointer is outside the area, moving back shrinks
//! nothing: the handle waits at the edge, and growth resumes only past the
//! farthest point the pointer had reached. Once the pointer is back inside, the
//! handle follows it again at the current scale. A pointer held still grows
//! nothing: only movement counts, never time.
//!
//! Why this and nothing else: the only input is the **absolute** pointer
//! position, so the drag behaves the same on every OS and in a virtual machine
//! (a lock or a warp of the cursor is not portable, and a speed set by the push
//! was rejected: in a diagonal drag one axis races ahead of the other).
//!
//! Pure functions only: [`follow_step`] turns the movement of the pointer into
//! a handle distance and a display scale. The gesture code in `layout_mode.rs`
//! feeds them and hands the lengths to the geometry (`resized_edge`,
//! `resized_line_alone`).

use egui::Vec2;

/// Time constant, in seconds, of the glide of the page to a new snapped size.
pub(crate) const GLIDE_TIME_CONSTANT: f32 = 0.03;

/// One step of the glide of the page toward a snapped size: an exponential
/// ease of time constant [`GLIDE_TIME_CONSTANT`], whatever the frame rate. The
/// last half point is taken at once (page sizes are whole points). A frame time
/// that is not finite or not positive moves nothing.
pub(crate) fn glide_step(eased: f64, target: f64, dt: f32) -> f64 {
    if !(eased.is_finite() && target.is_finite()) {
        return target;
    }
    if !(dt.is_finite() && dt > 0.0) {
        return eased;
    }
    let share = 1.0 - (-f64::from(dt) / f64::from(GLIDE_TIME_CONSTANT)).exp();
    let next = eased + (target - eased) * share;
    if (target - next).abs() < 0.5 {
        target
    } else {
        next
    }
}

fn finite(v: Vec2) -> Vec2 {
    Vec2::new(
        if v.x.is_finite() { v.x } else { 0.0 },
        if v.y.is_finite() { v.y } else { 0.0 },
    )
}

/// One axis of the drag: the frozen parameters and the handle distance so far.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AxisFollow {
    /// Screen coordinate of the opposite page edge (the anchor): it never
    /// moves during the drag.
    pub anchor: f32,
    /// `1.0` when the dragged handle is on the far side of the anchor (right,
    /// bottom), `-1.0` on the near side.
    pub sign: f32,
    /// Figure points between the handle and the outer edge of the figure on
    /// its side, which must stay in the area: the title band above a top edge,
    /// else 0. The empty space of the grid is left out on purpose: it never
    /// grows during the drag.
    pub lead: f32,
    /// Figure points past the handle that the snap may add (half a grid
    /// step): the zoom-out keeps room for them, so the snapped page never
    /// leaves the area while the scale only follows the unsnapped length.
    pub reserve: f32,
    /// The distance from the anchor to the handle now, in points (before the
    /// snap of the geometry).
    pub length: f32,
    /// The shortest and the longest distance the geometry accepts.
    pub min: f32,
    pub max: f32,
    /// The screen range the figure must stay in: the drawing area.
    pub bounds: (f32, f32),
    /// Screen points by which the pointer is beyond the edge of the area in
    /// the handle's direction, `0` while it is inside (or while the handle is
    /// not pinned at the edge).
    pub over: f32,
    /// The farthest `over` reached since the pointer left the area: moving
    /// back toward it grows nothing.
    pub reach: f32,
}

impl AxisFollow {
    /// Screen position of the dragged handle at `scale`.
    pub(crate) fn position(&self, scale: f32) -> f32 {
        self.anchor + self.sign * self.length * scale
    }

    /// The scale at which a handle at `length` has the outer edge of the figure
    /// on the edge of the drawing area, if the edge is on the handle's side of
    /// the anchor.
    pub(crate) fn scale_to_fit(&self, length: f32) -> Option<f32> {
        let edge = if self.sign > 0.0 {
            self.bounds.1
        } else {
            self.bounds.0
        };
        let span = self.sign * (edge - self.anchor);
        let extent = length + self.lead + self.reserve;
        (span > 0.0 && extent > 0.0).then(|| span / extent)
    }
}

/// One frame: the handle is carried by `motion` (screen points) from where it
/// is drawn. Returns the axes with their new distance and pointer state, and
/// the new display scale.
///
/// Per axis, inside the drawing area the distance follows the pointer at the
/// scale now shown. Past the edge, the handle stays at the edge, the overflow
/// becomes length (at the old scale) and the scale that puts the grown page
/// exactly in the area is wanted; moving back while the pointer is outside
/// grows nothing, and only a pointer past its farthest reach grows again. The
/// result scale is the smallest of the old one and the wanted ones: the display
/// zooms out, never back in. The length is clamped to what the geometry
/// accepts, so a handle held at its minimum or maximum does not hide motion
/// that the next frame would have to repay.
pub(crate) fn follow_step(
    axes: [AxisFollow; 2],
    scale: f32,
    motion: Vec2,
) -> ([AxisFollow; 2], f32) {
    let motion = finite(motion);
    let moved = [motion.x, motion.y];
    let mut out = axes;
    let mut next = scale;
    if !(scale.is_finite() && scale > 0.0) {
        return (out, scale);
    }
    for (i, axis) in axes.iter().enumerate() {
        let lead = axis.lead + axis.reserve;
        let (low, high) = axis.bounds;
        let edge = if axis.sign > 0.0 { high } else { low };
        let span = axis.sign * (edge - axis.anchor);
        let clamp = |extent: f32| (extent - lead).clamp(axis.min.min(axis.max), axis.max);
        let mut fit_to = None;
        if axis.over > 0.0 {
            // The pointer is outside: the handle waits at the edge.
            let over = axis.over + axis.sign * moved[i];
            if over <= 0.0 {
                // Back inside: the handle follows the pointer again.
                let outer = edge + axis.sign * over;
                out[i].length = clamp(axis.sign * (outer - axis.anchor) / scale);
                out[i].over = 0.0;
                out[i].reach = 0.0;
            } else {
                let grow = (over - axis.reach).max(0.0);
                let length = (axis.length + grow / scale).clamp(axis.min.min(axis.max), axis.max);
                out[i].length = length;
                out[i].over = over;
                out[i].reach = axis.reach.max(over);
                fit_to = Some(length);
            }
        } else {
            let outer = axis.position(scale) + axis.sign * lead * scale;
            let wanted = outer + moved[i];
            let beyond = axis.sign * (wanted - edge);
            let extent = if beyond > 0.0 && span > 0.0 {
                span / scale + beyond / scale
            } else {
                axis.sign * (wanted - axis.anchor) / scale
            };
            let length = clamp(extent);
            out[i].length = length;
            if beyond > 0.0 {
                out[i].over = beyond;
                out[i].reach = beyond;
                fit_to = Some(length);
            }
        }
        if let Some(length) = fit_to {
            if let Some(fit) = (AxisFollow { length, ..*axis }).scale_to_fit(length) {
                next = next.min(fit);
            }
        }
    }
    (out, next)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Axis growing to the right from an anchor at 100, length 200 pt at scale
    /// 1.0, drawing area up to 500.
    fn right() -> AxisFollow {
        AxisFollow {
            anchor: 100.0,
            sign: 1.0,
            lead: 0.0,
            reserve: 0.0,
            length: 200.0,
            min: 50.0,
            max: 20000.0,
            bounds: (0.0, 500.0),
            over: 0.0,
            reach: 0.0,
        }
    }

    /// A vertical axis (bottom corner) that stays far from its edge.
    fn down() -> AxisFollow {
        AxisFollow {
            anchor: 100.0,
            sign: 1.0,
            lead: 0.0,
            reserve: 0.0,
            length: 100.0,
            min: 50.0,
            max: 20000.0,
            bounds: (0.0, 800.0),
            over: 0.0,
            reach: 0.0,
        }
    }

    fn step(x: AxisFollow, y: AxisFollow, scale: f32, dx: f32, dy: f32) -> ([f32; 2], f32) {
        let (axes, scale) = follow_step([x, y], scale, Vec2::new(dx, dy));
        ([axes[0].length, axes[1].length], scale)
    }

    fn step_axes(axes: [AxisFollow; 2], scale: f32, dx: f32, dy: f32) -> ([AxisFollow; 2], f32) {
        follow_step(axes, scale, Vec2::new(dx, dy))
    }

    /// Inside the area the corner follows the pointer at the scale now shown.
    /// Witness: the scale frozen at the press when the scale has changed gives
    /// 250 instead of 300.
    #[test]
    fn inside_the_area_the_corner_follows_at_the_scale_shown() {
        let (lengths, scale) = step(right(), down(), 0.5, 50.0, 0.0);
        assert_eq!(scale, 0.5);
        assert_eq!(lengths[0], 300.0, "200 + 50 / 0.5");
        assert_eq!(lengths[1], 100.0);
    }

    /// Past the edge the corner sits at the edge, the overflow grows the page
    /// at the old scale, the scale shrinks, and the other axis' corner moves
    /// toward its anchor on the screen.
    /// Witness: no zoom-out leaves the scale at 1.0 and the length at 400.
    #[test]
    fn past_the_edge_the_page_grows_and_the_display_zooms_out() {
        let x = right();
        let y = down();
        // The corner is at 300, 200 from the edge at 500; 250 wanted beyond.
        let (lengths, scale) = step(x, y, 1.0, 450.0, 0.0);
        assert_eq!(lengths[0], 650.0, "to the edge (400) plus 250 of overflow");
        assert!((scale - 400.0 / 650.0).abs() < 1e-6);
        let corner_x = AxisFollow {
            length: lengths[0],
            ..x
        }
        .position(scale);
        assert!((corner_x - 500.0).abs() < 1e-3, "the corner is at the edge");
        let corner_y = AxisFollow {
            length: lengths[1],
            ..y
        }
        .position(scale);
        assert!(corner_y < y.position(1.0), "y moved toward its anchor");
        assert!(corner_y > y.anchor);
    }

    /// While the pointer is outside the area, moving back shrinks nothing;
    /// past the farthest point reached it grows again; back inside, the handle
    /// follows the pointer at the scale then shown. Witness: the old rule (the
    /// length follows the pointer at once) shrinks it at the first step back.
    #[test]
    fn outside_the_area_moving_back_shrinks_nothing_and_inside_it_follows_again() {
        let (axes, scale) = step_axes([right(), down()], 1.0, 450.0, 0.0);
        let grown = axes[0].length;
        assert_eq!(grown, 650.0);
        // 100 back, still 150 beyond the edge: nothing changes.
        let (back, scale_back) = step_axes(axes, scale, -100.0, 0.0);
        assert_eq!(back[0].length, grown);
        assert_eq!(scale_back, scale, "the display does not zoom back in");
        // Out again, but not past the farthest point: still nothing.
        let (again, scale_again) = step_axes(back, scale_back, 60.0, 0.0);
        assert_eq!(again[0].length, grown);
        assert_eq!(scale_again, scale);
        // Past it: grows by the excess only, at the scale shown.
        let (past, scale_past) = step_axes(again, scale_again, 90.0, 0.0);
        assert!((past[0].length - (grown + 50.0 / scale)).abs() < 1e-3);
        assert!(scale_past < scale_again);
        // Back inside: 300 beyond, then 350 back = 50 inside the edge.
        let (inside, scale_inside) = step_axes(past, scale_past, -350.0, 0.0);
        assert_eq!(inside[0].over, 0.0);
        let corner = inside[0].position(scale_inside);
        assert!(
            (corner - 450.0).abs() < 1e-2,
            "the handle follows: {corner}"
        );
        assert_eq!(scale_inside, scale_past);
    }

    /// Below the graphs' minimum the length stops, and coming back outward
    /// grows at once. Witness: no clamp (the length goes to 0 and further, the
    /// way back repays it).
    #[test]
    fn the_inward_stop_clamps_the_length_too() {
        let x = right();
        let (lengths, _) = step(x, down(), 1.0, -400.0, 0.0);
        assert_eq!(lengths[0], 50.0);
        let stopped = AxisFollow {
            length: lengths[0],
            ..x
        };
        let (again, _) = follow_step([stopped, down()], 1.0, Vec2::new(10.0, 0.0));
        assert_eq!(again[0].length, 60.0);
    }

    /// A pointer held still past the edge grows nothing: the step is the
    /// movement, never time. Witness: a growth added per call (not per motion)
    /// changes the length.
    #[test]
    fn a_still_pointer_grows_nothing() {
        let (axes, scale) = step_axes([right(), down()], 1.0, 450.0, 0.0);
        let (again, again_scale) = step_axes(axes, scale, 0.0, 0.0);
        assert!((again[0].length - axes[0].length).abs() < 1e-3);
        assert!((again[1].length - axes[1].length).abs() < 1e-3);
        assert!((again_scale - scale).abs() < 1e-6);
    }

    /// The page never passes the longest length the geometry accepts, and the
    /// display scale freezes there. Witness: a max of 20000 instead of 700.
    #[test]
    fn the_growth_stops_at_the_longest_page_and_the_display_freezes() {
        let x = AxisFollow {
            max: 700.0,
            ..right()
        };
        let (axes, scale) = step_axes([x, down()], 1.0, 900.0, 0.0);
        assert_eq!(axes[0].length, 700.0);
        let (again, again_scale) = step_axes(axes, scale, 50.0, 0.0);
        assert_eq!(again[0].length, 700.0);
        assert_eq!(again_scale, scale, "the display does not move any more");
    }

    /// A diagonal push grows both axes by their own movement.
    #[test]
    fn a_diagonal_push_grows_both_axes() {
        let y = AxisFollow {
            bounds: (0.0, 300.0),
            ..down()
        };
        let (lengths, _) = step(right(), y, 1.0, 450.0, 400.0);
        assert!(lengths[0] > 600.0 && lengths[1] > 400.0, "{lengths:?}");
    }

    /// Non-finite motion changes nothing (invariant 10).
    #[test]
    fn non_finite_motion_changes_nothing() {
        let (lengths, scale) = step(right(), down(), 1.0, f32::NAN, f32::INFINITY);
        assert_eq!(lengths, [200.0, 100.0]);
        assert_eq!(scale, 1.0);
    }

    /// A top corner (near side, title band above it): the band is part of the
    /// figure that must stay in the area, the handle sits below its edge.
    #[test]
    fn a_near_corner_keeps_the_title_band_in_the_area() {
        let up = AxisFollow {
            anchor: 400.0,
            sign: -1.0,
            lead: 30.0,
            reserve: 0.0,
            length: 200.0,
            min: 50.0,
            max: 20000.0,
            bounds: (100.0, 800.0),
            over: 0.0,
            reach: 0.0,
        };
        // Outer edge at 400 - 230 = 170; 100 of motion up goes 30 past 100.
        let (axes, scale) = step_axes([right(), up], 1.0, 0.0, -100.0);
        assert_eq!(axes[1].length, 200.0 + 70.0 + 30.0);
        let corner = axes[1].position(scale);
        let outer = corner - up.lead * scale;
        assert!((outer - 100.0).abs() < 1e-3, "the band edge is at the area");
    }

    /// The glide is an exponential ease, the same whatever the frame rate, and
    /// lands exactly on the target. Witness: a fixed share per frame instead of
    /// one from `dt` makes the 4 x 15 ms run differ from the 1 x 60 ms one.
    #[test]
    fn the_glide_is_frame_rate_independent_and_converges() {
        let once = glide_step(0.0, 100.0, GLIDE_TIME_CONSTANT);
        let mut split = 0.0;
        for _ in 0..4 {
            split = glide_step(split, 100.0, GLIDE_TIME_CONSTANT / 4.0);
        }
        assert!((once - split).abs() < 0.5, "{once} vs {split}");
        assert!((once - 100.0 * (1.0 - (-1.0_f64).exp())).abs() < 1e-6);
        let mut eased = 0.0;
        for _ in 0..60 {
            eased = glide_step(eased, 100.0, 1.0 / 60.0);
        }
        assert_eq!(eased, 100.0);
        assert_eq!(glide_step(10.0, 100.0, f32::NAN), 10.0);
        assert_eq!(glide_step(10.0, 100.0, 0.0), 10.0);
    }

    /// The reserve keeps room for the snap: the scale that fits the page
    /// counts it, and it starts zooming out before the corner is at the edge.
    /// Witness: reserve ignored gives scale 400/650 (the corner at the edge).
    #[test]
    fn the_reserve_is_kept_inside_the_area_when_zooming_out() {
        let x = AxisFollow {
            reserve: 25.0,
            ..right()
        };
        let (lengths, scale) = step(x, down(), 1.0, 450.0, 0.0);
        assert!(
            (scale - 400.0 / (lengths[0] + 25.0)).abs() < 1e-5,
            "{scale}"
        );
        let corner = AxisFollow {
            length: lengths[0],
            ..x
        }
        .position(scale);
        assert!((corner - (500.0 - 25.0 * scale)).abs() < 1e-3, "{corner}");
    }
}
