//! How a page-corner drag follows the pointer (#512).
//!
//! The opposite corner of the page (the anchor) never moves on the screen. The
//! system cursor is never held: it is free, as in the grid (#535). Inside the
//! drawing area the corner is under the pointer, outward and inward. While the
//! pointer is past the edge of the area on the corner's outward side of an axis,
//! that axis grows every frame by a speed set by the distance past the edge
//! ([`PAGE_PUSH`], in page points per second, so the same push grows the same
//! length at any zoom), the hand still or not, and the axes not pushed keep their
//! length; the display zooms out (one uniform scale, never back in) so the page,
//! drawn from its fixed anchor, stays in the area. As soon as no axis is pushed
//! any more, every axis is set from the pointer at the scale now shown: the
//! corner jumps under the pointer. Why: the pointer cannot go past the screen,
//! and a held or warped cursor is not portable (a lock on Wayland and macOS only,
//! a warp on Windows that the trial rejected), so the distance past the edge
//! replaces the travel the screen cannot give.
//!
//! Pure functions only: [`follow_step`] turns the pointer and the frame time
//! into a page length and a display scale. The gesture code in `layout_mode.rs`
//! feeds them and hands the lengths to the geometry (`page_corner_slack`,
//! `moved_page_corner`).

use egui::Vec2;
use stokesforge_render::edge_push::{push_past, EdgePush};

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

/// One axis of the drag: the frozen parameters and the page length so far.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AxisFollow {
    /// Screen coordinate of the opposite page corner (the anchor): it never
    /// moves during the drag.
    pub anchor: f32,
    /// `1.0` when the dragged corner is on the far side of the anchor (right,
    /// bottom), `-1.0` on the near side.
    pub sign: f32,
    /// Figure points between the dragged page corner and the edge of the
    /// figure on its side: the title band above a top corner, else 0.
    pub lead: f32,
    /// Figure points past the page corner that the snap may add (half a grid
    /// step): the zoom-out keeps room for them, so the snapped page never
    /// leaves the area while the scale only follows the unsnapped length.
    pub reserve: f32,
    /// The page length now, in points (before the snap of the geometry).
    pub length: f32,
    /// The shortest and the longest page length the geometry accepts.
    pub min: f32,
    pub max: f32,
    /// The screen range the figure must stay in: the drawing area.
    pub bounds: (f32, f32),
}

impl AxisFollow {
    /// Screen position of the dragged page corner at `scale`.
    pub(crate) fn corner(&self, scale: f32) -> f32 {
        self.anchor + self.sign * self.length * scale
    }

    /// The scale at which a page of `length` has its outer edge on the edge of
    /// the drawing area, if the edge is on the corner's side of the anchor.
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

/// The push-to-speed law of a page corner held past the edge of the drawing
/// area (#512): the signed distance of the corner past the edge, in screen
/// points, sets the growth speed in **page** points per second, so the same
/// push grows the page by the same length whatever the zoom. Starts from the
/// grid's scroll (#535: 10 per second per point of push) and is tuned at the
/// trial.
pub(crate) const PAGE_PUSH: EdgePush = EdgePush {
    speed_per_push: 10.0,
    max_speed: 3000.0,
    max_frame_time: 0.1,
};

/// What one frame of the drag gave.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FollowStep {
    /// The new page lengths, in points.
    pub lengths: [f32; 2],
    /// The new display scale (never above the old one).
    pub scale: f32,
    /// The axes the pointer pushes past the edge of the area, on the corner's
    /// outward side (they grow at speed).
    pub pushed: [bool; 2],
}

/// One frame. `wanted` is where the pointer wants the outer edge of the figure
/// on the screen (the pointer minus the grab offset of the press), `dt` the
/// frame time in seconds.
///
/// Per axis, the push is how far `wanted` is past the edge of the area on the
/// corner's outward side. While at least one axis is pushed, each pushed axis
/// is at least as long as puts its corner at the edge, and grows from there by
/// [`PAGE_PUSH`] at the speed its push sets (the hand may be still),
/// and the axes not pushed keep their length. When no axis is pushed, every
/// axis is set from `wanted` at the scale now shown: the corner jumps under
/// the pointer. A push on the anchor side grows nothing: the length is clamped
/// to what the geometry accepts (the graphs). The scale is then the smallest of
/// the old one and the one that puts the page, from its fixed anchor, inside the
/// area: the display zooms out, never back in. A `wanted` or a bound that is not
/// finite pushes nothing and moves nothing (invariant 10).
pub(crate) fn follow_step(axes: [AxisFollow; 2], scale: f32, wanted: Vec2, dt: f32) -> FollowStep {
    let mut step = FollowStep {
        lengths: [axes[0].length, axes[1].length],
        scale,
        pushed: [false; 2],
    };
    if !(scale.is_finite() && scale > 0.0 && wanted.x.is_finite() && wanted.y.is_finite()) {
        return step;
    }
    let wanted = [wanted.x, wanted.y];
    let mut push = [0.0_f32; 2];
    for (i, axis) in axes.iter().enumerate() {
        let (low, high) = axis.bounds;
        if low.is_finite() && high.is_finite() {
            push[i] = (axis.sign * push_past(wanted[i], low, high)).max(0.0);
        }
        step.pushed[i] = push[i] > 0.0;
    }
    let pushing = step.pushed[0] || step.pushed[1];
    for (i, axis) in axes.iter().enumerate() {
        let lead = axis.lead + axis.reserve;
        let range = axis.min.min(axis.max)..=axis.max;
        let length = if !pushing {
            (axis.sign * (wanted[i] - axis.anchor) / scale - lead)
                .clamp(*range.start(), *range.end())
        } else if step.pushed[i] {
            // The corner first reaches the edge (a pointer that went out in one
            // frame is not waited for), then the page grows from there.
            let edge = if axis.sign > 0.0 {
                axis.bounds.1
            } else {
                axis.bounds.0
            };
            let at_edge = axis.sign * (edge - axis.anchor) / scale - lead;
            (axis.length.max(at_edge) + PAGE_PUSH.step(push[i], dt)).min(axis.max)
        } else {
            axis.length
        };
        step.lengths[i] = length;
        if let Some(fit) = (AxisFollow { length, ..*axis }).scale_to_fit(length) {
            // A hair of tolerance: a page already inside never creeps.
            if fit < step.scale * (1.0 - 1.0e-5) {
                step.scale = fit;
            }
        }
    }
    step
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
        }
    }

    /// One frame with the pointer wanting the outer edge at `(x, y)` and a frame
    /// time of `dt`.
    fn step(x: AxisFollow, y: AxisFollow, scale: f32, at: (f32, f32), dt: f32) -> FollowStep {
        follow_step([x, y], scale, Vec2::new(at.0, at.1), dt)
    }

    /// Inside the area the corner is under the pointer at the scale now shown.
    /// Witness: the scale frozen at the press when the scale has changed gives
    /// 250 instead of 300 (here 100 + 200 / 0.5 = 500 at x = 200).
    #[test]
    fn inside_the_area_the_corner_is_under_the_pointer_at_the_scale_shown() {
        let moved = step(right(), down(), 0.5, (200.0, 150.0), 0.016);
        assert_eq!(moved.scale, 0.5);
        assert_eq!(moved.lengths[0], 200.0, "(200 - 100) / 0.5");
        assert_eq!(moved.lengths[1], 100.0, "(150 - 100) / 0.5");
        assert_eq!(moved.pushed, [false, false]);
    }

    /// Past the edge the length grows by the speed times the frame time, the
    /// other axis keeps its length, and the scale puts the page at the edge.
    /// Witness: no zoom-out leaves the scale at 1.0.
    #[test]
    fn past_the_edge_the_page_grows_at_speed_and_the_display_zooms_out() {
        let x = right();
        let y = down();
        // 30 points past the edge at 500: 300 page points per second. The
        // corner (at 300) first goes to the edge (a length of 400).
        let moved = step(x, y, 1.0, (530.0, 20.0), 0.1);
        assert_eq!(moved.pushed, [true, false]);
        assert!((moved.lengths[0] - 430.0).abs() < 1e-3, "{:?}", moved);
        assert_eq!(moved.lengths[1], 100.0, "the other axis keeps its length");
        assert!((moved.scale - 400.0 / 430.0).abs() < 1e-5, "{moved:?}");
        let corner_x = AxisFollow {
            length: moved.lengths[0],
            ..x
        }
        .corner(moved.scale);
        assert!(corner_x <= 500.0 + 1e-3, "{corner_x}");
    }

    /// The speed is the same in page points whatever the scale, and grows with
    /// the push. Witness: a speed in screen points (divided by the scale).
    #[test]
    fn the_speed_is_in_page_points_whatever_the_zoom() {
        let grown = |scale: f32, push: f32| {
            // Bounds so that a page of 200 points at `scale` is inside.
            let x = AxisFollow {
                bounds: (0.0, 100.0 + 200.0 * scale),
                ..right()
            };
            let at = 100.0 + 200.0 * scale + push;
            step(x, down(), scale, (at, 0.0), 0.05).lengths[0] - 200.0
        };
        assert!((grown(1.0, 20.0) - grown(0.25, 20.0)).abs() < 1e-3);
        assert!(grown(1.0, 40.0) > grown(1.0, 20.0) * 1.9);
        assert!((grown(1.0, 20.0) - 10.0).abs() < 1e-3, "20 * 10 * 0.05");
    }

    /// A stall counts as one frame of the longest allowed time. Witness: no dt
    /// clamp (the 5 s stall grows by 2000 points).
    #[test]
    fn a_stalled_frame_grows_by_the_longest_frame_time_only() {
        let moved = step(right(), down(), 1.0, (540.0, 0.0), 5.0);
        assert!(
            (moved.lengths[0] - 400.0 - 40.0 * 10.0 * 0.1).abs() < 1e-3,
            "{:?}",
            moved
        );
    }

    /// Pushed on the anchor side, nothing grows, the length is clamped to the
    /// minimum. Witness: an unsigned push (the left side grows the page).
    #[test]
    fn a_push_on_the_anchor_side_grows_nothing() {
        let to_the_left = AxisFollow {
            bounds: (150.0, 500.0),
            ..right()
        };
        let moved = step(to_the_left, down(), 1.0, (100.0, 0.0), 0.1);
        assert_eq!(moved.pushed, [false, false]);
        assert_eq!(moved.lengths[0], 50.0, "clamped to the minimum");
    }

    /// A diagonal push grows both axes, each at its own speed.
    #[test]
    fn a_diagonal_push_grows_both_axes() {
        let moved = step(right(), down(), 1.0, (540.0, 820.0), 0.1);
        assert_eq!(moved.pushed, [true, true]);
        assert!((moved.lengths[0] - 440.0).abs() < 1e-3, "{moved:?}");
        assert!((moved.lengths[1] - 720.0).abs() < 1e-3, "{moved:?}");
    }

    /// With the pointer back inside, every axis is set from the pointer at the
    /// scale now shown, the axis that was not pushed too.
    /// Witness: the length kept for the axis that was not pushed.
    #[test]
    fn back_inside_every_axis_is_set_from_the_pointer() {
        let grown = AxisFollow {
            length: 650.0,
            ..right()
        };
        let moved = step(grown, down(), 0.5, (300.0, 250.0), 0.016);
        assert_eq!(moved.pushed, [false, false]);
        assert_eq!(moved.lengths[0], 400.0, "(300 - 100) / 0.5");
        assert_eq!(moved.lengths[1], 300.0, "(250 - 100) / 0.5");
        assert_eq!(moved.scale, 0.5, "the display does not zoom back in");
    }

    /// Below the graphs' minimum the length stops, and coming back outward
    /// grows at once. Witness: no clamp.
    #[test]
    fn the_inward_stop_clamps_the_length_too() {
        let moved = step(right(), down(), 1.0, (-300.0, 0.0), 0.016);
        assert_eq!(moved.lengths[0], 50.0);
        let stopped = AxisFollow {
            length: moved.lengths[0],
            ..right()
        };
        let again = step(stopped, down(), 1.0, (160.0, 0.0), 0.016);
        assert_eq!(again.lengths[0], 60.0);
    }

    /// The page never passes the longest length the geometry accepts.
    #[test]
    fn the_growth_stops_at_the_longest_page() {
        let near_max = AxisFollow {
            length: 19999.0,
            ..right()
        };
        let moved = step(near_max, down(), 1.0, (900.0, 0.0), 0.1);
        assert_eq!(moved.lengths[0], 20000.0);
    }

    /// A pointer or a frame time that is not finite changes nothing
    /// (invariant 10).
    #[test]
    fn non_finite_input_changes_nothing() {
        let moved = step(right(), down(), 1.0, (f32::NAN, f32::INFINITY), 0.1);
        assert_eq!(moved.lengths, [200.0, 100.0]);
        assert_eq!(moved.scale, 1.0);
        assert_eq!(moved.pushed, [false, false]);
        let no_time = step(right(), down(), 1.0, (540.0, 0.0), f32::NAN);
        assert_eq!(no_time.lengths[0], 400.0, "at the edge, no growth");
    }

    /// A top corner (near side, title band above it): the band is part of the
    /// figure that must stay in the area, the page corner sits below its edge.
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
        };
        // 20 past the top edge at 100: 200 per second, 0.1 s.
        let moved = step(right(), up, 1.0, (0.0, 80.0), 0.1);
        assert_eq!(moved.pushed, [false, true]);
        assert!((moved.lengths[1] - 290.0).abs() < 1e-3, "{moved:?}");
        let corner = AxisFollow {
            length: moved.lengths[1],
            ..up
        }
        .corner(moved.scale);
        let outer = corner - up.lead * moved.scale;
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
    /// counts it. Witness: reserve ignored gives scale 400 / (length) with the
    /// corner at the edge.
    #[test]
    fn the_reserve_is_kept_inside_the_area_when_zooming_out() {
        let x = AxisFollow {
            reserve: 25.0,
            ..right()
        };
        let moved = step(x, down(), 1.0, (550.0, 0.0), 0.1);
        assert!(
            (moved.scale - 400.0 / (moved.lengths[0] + 25.0)).abs() < 1e-5,
            "{moved:?}"
        );
        let corner = AxisFollow {
            length: moved.lengths[0],
            ..x
        }
        .corner(moved.scale);
        assert!(
            (corner - (500.0 - 25.0 * moved.scale)).abs() < 1e-3,
            "{corner}"
        );
    }
}
