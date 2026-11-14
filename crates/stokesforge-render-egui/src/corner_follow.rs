//! How a page-corner drag follows the pointer (#512).
//!
//! The corner of the page stays under the cursor for the whole drag, outward
//! and inward, and the opposite corner does not move on the screen. When the
//! corner reaches the edge of the drawing area the cursor stays on it, pushing
//! further still grows the page, and the display zooms out (one uniform scale,
//! never zooming back in during the drag) so the whole page stays visible.
//! Coming back inward, the page shrinks at once and the corner follows the
//! cursor: the page length is clamped, so there is no dead zone.
//!
//! Pure functions only: [`pointer_step`] picks the movement of the pointer,
//! [`follow_step`] turns it into a page length and a display scale. The gesture
//! code in `layout_mode.rs` feeds them and hands the lengths to the geometry
//! (`page_corner_slack`, `moved_page_corner`).

use egui::Vec2;

/// Screen points of cursor movement per unit of raw mouse motion. 1 unit = 1
/// point for now; tuned at the trial on the user's screen.
pub(crate) const EDGE_MOTION_GAIN: f32 = 1.0;

/// The pointer movement of one frame of a page-corner drag.
///
/// Once any raw motion (`Event::MouseMoved`) arrived in this drag, only the
/// motion counts: under a cursor lock the absolute pointer is frozen, and on
/// Windows raw and absolute moves can come in different frames, so adding both
/// would count a move twice. Before the first motion event (an environment
/// without raw motion, a bench that only sends `PointerMoved`) the absolute
/// movement is used. Non-finite values count as no movement (invariant 10).
pub(crate) fn pointer_step(raw_seen: &mut bool, motion: Option<Vec2>, absolute: Vec2) -> Vec2 {
    if motion.is_some() {
        *raw_seen = true;
    }
    let source = if *raw_seen {
        motion.unwrap_or(Vec2::ZERO) * EDGE_MOTION_GAIN
    } else {
        absolute
    };
    finite(source)
}

fn finite(v: Vec2) -> Vec2 {
    Vec2::new(
        if v.x.is_finite() { v.x } else { 0.0 },
        if v.y.is_finite() { v.y } else { 0.0 },
    )
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
        let extent = length + self.lead;
        (span > 0.0 && extent > 0.0).then(|| span / extent)
    }
}

/// One frame: the corner is carried by `motion` (screen points) from where it
/// is drawn. Returns the new page lengths and the new display scale.
///
/// Per axis, the wanted position of the figure's outer edge is where it is
/// drawn plus the motion. Inside the drawing area the length follows it. Past
/// the edge, the edge stays where it is, the overflow becomes length (at the
/// old scale) and the scale that puts the grown page exactly in the area is
/// wanted. The result scale is the smallest of the old one and the wanted ones:
/// the display zooms out, never back in. The length is clamped to what the
/// geometry accepts, so a page held at its minimum (the graphs) or maximum does
/// not hide motion that the next frame would have to repay.
pub(crate) fn follow_step(axes: [AxisFollow; 2], scale: f32, motion: Vec2) -> ([f32; 2], f32) {
    let motion = finite(motion);
    let moved = [motion.x, motion.y];
    let mut lengths = [axes[0].length, axes[1].length];
    let mut next = scale;
    if !(scale.is_finite() && scale > 0.0) {
        return (lengths, scale);
    }
    for (i, axis) in axes.iter().enumerate() {
        let outer = axis.corner(scale) + axis.sign * axis.lead * scale;
        let wanted = outer + moved[i];
        let (low, high) = axis.bounds;
        let edge = if axis.sign > 0.0 { high } else { low };
        let beyond = axis.sign * (wanted - edge);
        let span = axis.sign * (edge - axis.anchor);
        let extent = if beyond > 0.0 && span > 0.0 {
            span / scale + beyond / scale
        } else {
            axis.sign * (wanted - axis.anchor) / scale
        };
        let length = (extent - axis.lead).clamp(axis.min.min(axis.max), axis.max);
        lengths[i] = length;
        if beyond > 0.0 {
            if let Some(fit) = (AxisFollow { length, ..*axis }).scale_to_fit(length) {
                next = next.min(fit);
            }
        }
    }
    (lengths, next)
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
            length: 100.0,
            min: 50.0,
            max: 20000.0,
            bounds: (0.0, 800.0),
        }
    }

    fn step(x: AxisFollow, y: AxisFollow, scale: f32, dx: f32, dy: f32) -> ([f32; 2], f32) {
        follow_step([x, y], scale, Vec2::new(dx, dy))
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
        .corner(scale);
        assert!((corner_x - 500.0).abs() < 1e-3, "the corner is at the edge");
        let corner_y = AxisFollow {
            length: lengths[1],
            ..y
        }
        .corner(scale);
        assert!(corner_y < y.corner(1.0), "y moved toward its anchor");
        assert!(corner_y > y.anchor);
    }

    /// Right after an overflow, going back inward shrinks the page at once.
    /// Witness: an accumulator that keeps the overflow (not clamped to the
    /// edge) needs the whole 250 of motion before the length moves.
    #[test]
    fn coming_back_inward_has_no_dead_zone() {
        let x = right();
        let (lengths, scale) = step(x, down(), 1.0, 450.0, 0.0);
        let at_edge = AxisFollow {
            length: lengths[0],
            ..x
        };
        let (back, scale_back) = follow_step([at_edge, down()], scale, Vec2::new(-10.0, 0.0));
        assert!(back[0] < lengths[0], "{} < {}", back[0], lengths[0]);
        assert!((back[0] - (lengths[0] - 10.0 / scale)).abs() < 1e-3);
        assert_eq!(scale_back, scale, "the display does not zoom back in");
        let corner = AxisFollow {
            length: back[0],
            ..x
        }
        .corner(scale_back);
        assert!(
            (corner - 490.0).abs() < 1e-3,
            "the corner follows: {corner}"
        );
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
        assert_eq!(again[0], 60.0);
    }

    /// Non-finite motion changes nothing (invariant 10).
    #[test]
    fn non_finite_motion_changes_nothing() {
        let (lengths, scale) = step(right(), down(), 1.0, f32::NAN, f32::INFINITY);
        assert_eq!(lengths, [200.0, 100.0]);
        assert_eq!(scale, 1.0);
    }

    /// A top corner (near side, title band above it): the band is part of the
    /// figure that must stay in the area, the page corner sits below its edge.
    #[test]
    fn a_near_corner_keeps_the_title_band_in_the_area() {
        let up = AxisFollow {
            anchor: 400.0,
            sign: -1.0,
            lead: 30.0,
            length: 200.0,
            min: 50.0,
            max: 20000.0,
            bounds: (100.0, 800.0),
        };
        // Outer edge at 400 - 230 = 170; 100 of motion up goes 30 past 100.
        let (lengths, scale) = follow_step([right(), up], 1.0, Vec2::new(0.0, -100.0));
        assert_eq!(lengths[1], 200.0 + 70.0 + 30.0);
        let corner = AxisFollow {
            length: lengths[1],
            ..up
        }
        .corner(scale);
        let outer = corner - up.lead * scale;
        assert!((outer - 100.0).abs() < 1e-3, "the band edge is at the area");
    }

    /// Raw motion alone once it was seen; the absolute movement before.
    /// Witness: summing both gives (12, 0) and (15, 0).
    #[test]
    fn the_pointer_step_takes_motion_only_after_the_first_motion_event() {
        let mut seen = false;
        let absolute = Vec2::new(10.0, 0.0);
        assert_eq!(pointer_step(&mut seen, None, absolute), absolute);
        assert!(!seen);
        let motion = Some(Vec2::new(2.0, 0.0));
        assert_eq!(
            pointer_step(&mut seen, motion, absolute),
            Vec2::new(2.0, 0.0)
        );
        assert!(seen);
        assert_eq!(
            pointer_step(&mut seen, None, absolute),
            Vec2::ZERO,
            "a frame without motion after the first one moves nothing"
        );
        assert_eq!(
            pointer_step(&mut seen, Some(Vec2::new(f32::NAN, 3.0)), absolute),
            Vec2::new(0.0, 3.0)
        );
    }
}
