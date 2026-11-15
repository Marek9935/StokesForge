//! The probe that tells a relative pointer from an absolute one (#512, #535).
//!
//! A gesture that holds the pointer at an edge (the page corner of Page Layout
//! Mode, a selection drag in the grid) needs the movement of the hand, not the
//! position of the cursor, once the mouse is locked. Raw motion is only
//! trustworthy on a relative device: winit hands absolute coordinates over as
//! raw motion on a Windows absolute device (a virtual machine's tablet) and
//! under Xwayland. [`PointerSource`] and [`pointer_step`] find out, at the
//! start of a drag, which kind moves the pointer, and reject corrupt input.
//! The lock itself lives in `pointer_lock`. Nothing here knows what the
//! gesture does with the movement.

use egui::Vec2;

/// Screen points of cursor movement per unit of raw mouse motion. 1 unit = 1
/// point for now; tuned at the trial on the user's screen.
pub const EDGE_MOTION_GAIN: f32 = 1.0;

/// Absolute travel of the pointer, in points, after which the probe decides
/// what kind of device moves it.
pub const PROBE_TRAVEL: f32 = 8.0;

/// Raw motion over absolute travel that a relative device gives while the
/// mouse is not locked yet: the two measure the same hand movement, so the
/// ratio is about 1. Outside this range the "motion" is not a movement of the
/// device: winit hands absolute coordinates over as raw motion on a Windows
/// absolute device (a virtual machine's tablet pointer, 0..65535) and under
/// Xwayland (window coordinates), a ratio in the hundreds.
pub const PLAUSIBLE_RATIO: (f32, f32) = (0.2, 5.0);

/// A frame whose raw motion is more than this many times the probe's average
/// per frame is not a hand movement but corrupt input (an absolute value
/// slipping in after the probe was fooled). It is rejected, not capped: the
/// rest of the drag falls back to the absolute pointer, so the page never jumps.
pub const OUTLIER_FACTOR: f32 = 50.0;

/// The bound never falls below this raw motion in one frame (points): a slow,
/// careful grab followed by a fast flick is a hand movement, not corrupt input.
/// The corrupt values seen are absolute coordinates, hundreds to tens of
/// thousands of units in every frame.
pub const OUTLIER_FLOOR: f32 = 400.0;

/// What moves the pointer of a page-corner drag, found out by a probe at the
/// start of the drag (#512).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerSource {
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
    pub fn probing() -> Self {
        Self::Probing {
            absolute: 0.0,
            motion: 0.0,
            frames: 0,
        }
    }

    /// Whether the mouse is locked for this drag.
    pub fn locks(&self) -> bool {
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
pub fn pointer_step(source: &mut PointerSource, motion: Option<Vec2>, absolute: Vec2) -> Vec2 {
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
                        bound: (OUTLIER_FACTOR * raw / frames as f32).max(OUTLIER_FLOOR),
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

pub(crate) fn finite(v: Vec2) -> Vec2 {
    Vec2::new(
        if v.x.is_finite() { v.x } else { 0.0 },
        if v.y.is_finite() { v.y } else { 0.0 },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// A slow probe then a fast flick is a hand movement: the floor keeps it.
    /// Witness: the bound without its floor (50 x 1) rejects the flick.
    #[test]
    fn a_fast_flick_after_a_slow_probe_is_kept() {
        let slow = (Some(Vec2::new(1.0, 0.0)), Vec2::new(1.0, 0.0));
        let (mut source, _) = run(&[slow; 8]);
        assert!(source.locks());
        let flick = Vec2::new(120.0, 0.0);
        assert_eq!(pointer_step(&mut source, Some(flick), Vec2::ZERO), flick);
        assert!(source.locks());
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
}
