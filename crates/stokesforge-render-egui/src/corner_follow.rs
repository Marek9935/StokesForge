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

/// Absolute travel of the pointer, in points, after which the probe decides
/// what kind of device moves it.
pub(crate) const PROBE_TRAVEL: f32 = 8.0;

/// Raw motion over absolute travel that a relative device gives while the
/// mouse is not locked yet: the two measure the same hand movement, so the
/// ratio is about 1. Outside this range the "motion" is not a movement of the
/// device: winit hands absolute coordinates over as raw motion on a Windows
/// absolute device (a virtual machine's tablet pointer, 0..65535) and under
/// Xwayland (window coordinates), a ratio in the hundreds.
pub(crate) const PLAUSIBLE_RATIO: (f32, f32) = (0.2, 5.0);

/// A frame whose raw motion is more than this many times the probe's average
/// per frame is not a hand movement but corrupt input (an absolute value
/// slipping in after the probe was fooled). It is rejected, not capped: the
/// rest of the drag falls back to the absolute pointer, so the page never jumps.
pub(crate) const OUTLIER_FACTOR: f32 = 50.0;

/// Time constant, in seconds, of the glide of the page to a new snapped size.
pub(crate) const GLIDE_TIME_CONSTANT: f32 = 0.06;

/// What moves the pointer of a page-corner drag, found out by a probe at the
/// start of the drag (#512).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PointerSource {
    /// Not decided yet: the absolute pointer drives, and both the absolute
    /// travel and the raw motion are measured. No mouse lock yet.
    Probing {
        absolute: f32,
        motion: f32,
        frames: u32,
    },
    /// A relative device: the mouse is locked and only raw motion counts.
    /// `bound` is the largest raw motion one frame may carry.
    Relative { bound: f32 },
    /// An absolute device (or corrupt raw motion): no lock, no hidden cursor,
    /// the absolute pointer drives for the whole drag. The corner stops when
    /// the real cursor reaches the edge of the screen.
    Absolute,
}

impl PointerSource {
    pub(crate) fn probing() -> Self {
        Self::Probing {
            absolute: 0.0,
            motion: 0.0,
            frames: 0,
        }
    }

    /// Whether the mouse is locked for this drag.
    pub(crate) fn locks(&self) -> bool {
        matches!(self, Self::Relative { .. })
    }
}

/// The pointer movement of one frame of a page-corner drag, and the update of
/// what is known about the device. Non-finite values count as no movement
/// (invariant 10).
///
/// While probing the absolute movement drives (the pointer is free, so it is
/// the true one) and, once it travelled [`PROBE_TRAVEL`], the raw motion
/// measured over the same frames decides: plausible ([`PLAUSIBLE_RATIO`]) means
/// a relative device. Under a lock the absolute pointer is frozen, so only the
/// motion counts; a frame beyond the bound ([`OUTLIER_FACTOR`]) ends the lock.
pub(crate) fn pointer_step(
    source: &mut PointerSource,
    motion: Option<Vec2>,
    absolute: Vec2,
) -> Vec2 {
    let motion = motion.map(finite);
    match *source {
        PointerSource::Probing {
            absolute: travelled,
            motion: raw,
            frames,
        } => {
            let step = finite(absolute);
            let travelled = travelled + step.length();
            let raw = raw + motion.map_or(0.0, Vec2::length);
            let frames = frames + 1;
            *source = if travelled < PROBE_TRAVEL {
                PointerSource::Probing {
                    absolute: travelled,
                    motion: raw,
                    frames,
                }
            } else {
                let ratio = raw / travelled;
                if (PLAUSIBLE_RATIO.0..=PLAUSIBLE_RATIO.1).contains(&ratio) {
                    PointerSource::Relative {
                        bound: OUTLIER_FACTOR * raw / frames as f32,
                    }
                } else {
                    PointerSource::Absolute
                }
            };
            step
        }
        PointerSource::Relative { bound } => {
            let motion = motion.unwrap_or(Vec2::ZERO);
            if motion.length() > bound {
                *source = PointerSource::Absolute;
                finite(absolute)
            } else {
                motion * EDGE_MOTION_GAIN
            }
        }
        PointerSource::Absolute => finite(absolute),
    }
}

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
        let lead = axis.lead + axis.reserve;
        let outer = axis.corner(scale) + axis.sign * lead * scale;
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
        let length = (extent - lead).clamp(axis.min.min(axis.max), axis.max);
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
            reserve: 0.0,
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

    /// Drives the probe with `frames` of `(motion, absolute)` and returns the
    /// source and the steps.
    fn run(frames: &[(Option<Vec2>, Vec2)]) -> (PointerSource, Vec<Vec2>) {
        let mut source = PointerSource::probing();
        let steps = frames
            .iter()
            .map(|(motion, absolute)| pointer_step(&mut source, *motion, *absolute))
            .collect();
        (source, steps)
    }

    /// A relative device: motion matches the absolute travel, the probe locks
    /// and the motion drives from then on. Witness: a probe that never decides
    /// `Relative` leaves the absolute (frozen under the lock) pointer driving,
    /// the last step is zero instead of (30, 0).
    #[test]
    fn a_relative_device_locks_and_motion_drives_after_the_probe() {
        let both = (Some(Vec2::new(5.0, 0.0)), Vec2::new(5.0, 0.0));
        let (source, steps) = run(&[both, both]);
        assert!(source.locks(), "{source:?}");
        assert_eq!(
            steps,
            vec![Vec2::new(5.0, 0.0); 2],
            "the probe follows the pointer"
        );
        let mut source = source;
        let step = pointer_step(&mut source, Some(Vec2::new(30.0, 0.0)), Vec2::ZERO);
        assert_eq!(step, Vec2::new(30.0, 0.0));
        assert_eq!(pointer_step(&mut source, None, Vec2::ZERO), Vec2::ZERO);
        assert!(source.locks());
    }

    /// Xwayland-like: the raw motion is the absolute window coordinates, the
    /// same every frame. No lock, the absolute movement drives for the whole
    /// drag. Witness: no probe (motion used at once) gives (778, 514).
    #[test]
    fn absolute_coordinates_as_motion_decide_an_absolute_device() {
        let wrong = Some(Vec2::new(778.0, 514.0));
        let (mut source, steps) =
            run(&[(wrong, Vec2::new(0.0, 5.0)), (wrong, Vec2::new(0.0, 5.0))]);
        assert_eq!(source, PointerSource::Absolute);
        assert_eq!(steps, vec![Vec2::new(0.0, 5.0); 2]);
        let step = pointer_step(&mut source, wrong, Vec2::new(0.0, 7.0));
        assert_eq!(step, Vec2::new(0.0, 7.0));
        assert!(!source.locks());
    }

    /// A device that sends no raw motion at all is absolute too, and a drag
    /// shorter than the probe stays undecided (no lock).
    #[test]
    fn no_motion_is_absolute_and_a_short_drag_stays_undecided() {
        let (source, _) = run(&[(None, Vec2::new(0.0, 10.0))]);
        assert_eq!(source, PointerSource::Absolute);
        let (source, _) = run(&[(Some(Vec2::new(0.0, 3.0)), Vec2::new(0.0, 3.0))]);
        assert!(matches!(source, PointerSource::Probing { .. }));
        assert!(!source.locks());
    }

    /// After the lock, one frame far beyond the probe's average is corrupt: it
    /// moves nothing, and the rest of the drag is absolute. Witness: bound
    /// removed gives (778, 514) as the step.
    #[test]
    fn one_outlier_frame_after_the_lock_falls_back_without_a_jump() {
        let both = (Some(Vec2::new(4.0, 0.0)), Vec2::new(4.0, 0.0));
        let (mut source, _) = run(&[both, both, both]);
        assert!(source.locks());
        let step = pointer_step(&mut source, Some(Vec2::new(778.0, 514.0)), Vec2::ZERO);
        assert_eq!(step, Vec2::ZERO);
        assert_eq!(source, PointerSource::Absolute);
        let step = pointer_step(&mut source, Some(Vec2::new(5.0, 0.0)), Vec2::new(0.0, 2.0));
        assert_eq!(step, Vec2::new(0.0, 2.0));
    }

    /// Non-finite values count as no movement, whatever the source.
    #[test]
    fn non_finite_pointer_values_move_nothing() {
        let mut source = PointerSource::probing();
        let nan = Vec2::new(f32::NAN, 3.0);
        assert_eq!(
            pointer_step(&mut source, Some(nan), nan),
            Vec2::new(0.0, 3.0)
        );
        let mut locked = PointerSource::Relative { bound: 100.0 };
        assert_eq!(
            pointer_step(&mut locked, Some(nan), Vec2::ZERO),
            Vec2::new(0.0, 3.0)
        );
    }

    /// The glide is an exponential ease, the same whatever the frame rate, and
    /// lands exactly on the target. Witness: a fixed share per frame instead of
    /// one from `dt` makes the 4 x 15 ms run differ from the 1 x 60 ms one.
    #[test]
    fn the_glide_is_frame_rate_independent_and_converges() {
        let once = glide_step(0.0, 100.0, 0.060);
        let mut split = 0.0;
        for _ in 0..4 {
            split = glide_step(split, 100.0, 0.015);
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
        .corner(scale);
        assert!((corner - (500.0 - 25.0 * scale)).abs() < 1e-3, "{corner}");
    }
}
