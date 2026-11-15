//! Speed of the grid's scroll while a selection is dragged past its edge
//! (#535). Pure arithmetic only, no window and no model: the grid keeps the
//! push between frames and applies the scroll.
//!
//! The **push** of an axis is how far the hand has gone past the edge, in
//! points, signed: positive past the far edge (right, bottom), negative past
//! the near one (left, top). With the mouse locked at the edge it is the raw
//! motion accumulated since the lock ([`push_under_lock`]); with an absolute
//! pointer it is the distance of the cursor past the edge ([`push_past`]). The
//! speed grows with the push ([`scroll_speed`]) and is capped; pushing back
//! reduces it, then stops it.

/// Scroll speed per point of push, in points per second per point (1/s).
/// Tuned at the trial on the user's screen.
pub const SPEED_PER_PUSH: f32 = 10.0;

/// The fastest scroll, in points per second.
pub const MAX_SPEED: f32 = 6000.0;

/// A frame longer than this many seconds scrolls as a frame of this length: a
/// stall must not throw the grid across the sheet.
pub const MAX_FRAME_TIME: f32 = 0.1;

/// How close to the edge, in points, the pointer counts as having reached it:
/// the lock is taken there, with at least this push.
pub const REACH_MARGIN: f32 = 1.0;

fn finite(value: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

/// Signed distance of `pos` past the range `min..=max` (an absolute pointer):
/// 0 inside, positive beyond `max`, negative before `min`. Not finite: 0.
pub fn push_past(pos: f32, min: f32, max: f32) -> f32 {
    if !pos.is_finite() {
        return 0.0;
    }
    let (min, max) = (finite(min), finite(max));
    if pos > max {
        pos - max
    } else if pos < min {
        pos - min
    } else {
        0.0
    }
}

/// The push at which a locked mouse starts: 0 while `pos` is more than
/// [`REACH_MARGIN`] inside the range, else the distance past the edge, at
/// least [`REACH_MARGIN`] (so that a pointer exactly on the edge holds). Signed
/// as [`push_past`]. Not finite: 0.
pub fn push_reaching(pos: f32, min: f32, max: f32) -> f32 {
    if !pos.is_finite() {
        return 0.0;
    }
    let (min, max) = (finite(min), finite(max));
    if pos >= max - REACH_MARGIN {
        (pos - max).max(0.0) + REACH_MARGIN
    } else if pos <= min + REACH_MARGIN {
        -((min - pos).max(0.0) + REACH_MARGIN)
    } else {
        0.0
    }
}

/// The push after `motion` (points of raw mouse motion on this axis) while the
/// mouse is locked at the edge. Pushing back to the edge ends it at exactly 0
/// (the hold is released there: what is left of the motion is not kept). Not
/// finite motion counts as none.
pub fn push_under_lock(push: f32, motion: f32) -> f32 {
    let push = finite(push);
    let next = push + finite(motion);
    if (push > 0.0 && next <= 0.0) || (push < 0.0 && next >= 0.0) {
        0.0
    } else {
        next
    }
}

/// Scroll speed, in points per second, signed as the push. Not finite: 0.
pub fn scroll_speed(push: f32) -> f32 {
    (finite(push) * SPEED_PER_PUSH).clamp(-MAX_SPEED, MAX_SPEED)
}

/// How far the grid scrolls this frame, in points, signed as the push: the
/// speed times the frame time (at most [`MAX_FRAME_TIME`]). Not finite: 0.
pub fn scroll_step(push: f32, dt: f32) -> f32 {
    let dt = finite(dt).clamp(0.0, MAX_FRAME_TIME);
    scroll_speed(push) * dt
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inside the edges nothing moves. Witness: a speed with an offset
    /// (`push + 1`) is not 0 here.
    #[test]
    fn inside_the_edges_the_speed_is_zero() {
        assert_eq!(push_past(50.0, 0.0, 100.0), 0.0);
        assert_eq!(scroll_speed(0.0), 0.0);
        assert_eq!(scroll_step(0.0, 0.05), 0.0);
    }

    /// Past an edge the push is signed by the side. Witness: an unsigned
    /// distance gives +20 for the left side.
    #[test]
    fn the_push_is_signed_by_the_edge_it_is_past() {
        assert_eq!(push_past(130.0, 0.0, 100.0), 30.0);
        assert_eq!(push_past(-20.0, 0.0, 100.0), -20.0);
        assert!(scroll_step(-20.0, 0.05) < 0.0);
    }

    /// A bigger push is faster, up to the cap. Witness: a constant speed fails
    /// the first assertion, no cap the last.
    #[test]
    fn more_push_is_faster_and_the_speed_is_capped() {
        assert!(scroll_speed(20.0) > scroll_speed(10.0));
        assert!(scroll_speed(10.0) > 0.0);
        assert_eq!(scroll_speed(1.0e6), MAX_SPEED);
        assert_eq!(scroll_speed(-1.0e6), -MAX_SPEED);
    }

    /// Twice the frame time scrolls twice as far, under the cap and under the
    /// frame-time bound. Witness: a step that ignores `dt` fails.
    #[test]
    fn the_step_is_proportional_to_the_frame_time() {
        let one = scroll_step(30.0, 0.02);
        assert!(one > 0.0);
        assert!((scroll_step(30.0, 0.04) - 2.0 * one).abs() < 1e-3);
        // A stall counts as MAX_FRAME_TIME.
        assert_eq!(scroll_step(30.0, 5.0), scroll_step(30.0, MAX_FRAME_TIME));
        assert_eq!(scroll_step(30.0, -1.0), 0.0);
    }

    /// Not finite values move nothing (invariant 10). Witness: no guard gives
    /// NaN.
    #[test]
    fn non_finite_values_move_nothing() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(scroll_speed(bad), 0.0);
            assert_eq!(scroll_step(bad, 0.05), 0.0);
            assert_eq!(scroll_step(30.0, bad), 0.0);
            assert_eq!(push_past(bad, 10.0, 100.0), 0.0);
            assert_eq!(push_reaching(bad, 10.0, 100.0), 0.0);
            assert_eq!(push_under_lock(bad, 5.0), 5.0);
            assert_eq!(push_under_lock(5.0, bad), 5.0);
        }
    }

    /// Pushing back reduces the speed, then stops it and ends the hold at 0.
    /// Witness: no clamp at zero leaves a negative push (the grid would scroll
    /// the other way).
    #[test]
    fn pushing_back_slows_then_stops_at_zero() {
        let mut push = push_under_lock(0.0, 0.0) + 40.0;
        let fast = scroll_speed(push);
        push = push_under_lock(push, -15.0);
        assert_eq!(push, 25.0);
        assert!(scroll_speed(push) < fast);
        push = push_under_lock(push, -100.0);
        assert_eq!(push, 0.0, "back past the edge: stopped, hold ended");
        let mut near = push_under_lock(-10.0, 30.0);
        assert_eq!(near, 0.0);
        near = push_under_lock(near, 4.0);
        assert_eq!(near, 4.0, "a push starts again only from a new hold");
    }

    /// The lock starts a point before the edge, with at least the margin, so a
    /// pointer exactly on the edge holds. Witness: no margin gives 0 on the
    /// edge.
    #[test]
    fn the_lock_starts_at_the_edge_with_a_minimum_push() {
        assert_eq!(push_reaching(50.0, 0.0, 100.0), 0.0);
        assert_eq!(push_reaching(100.0, 0.0, 100.0), REACH_MARGIN);
        assert_eq!(push_reaching(112.0, 0.0, 100.0), 12.0 + REACH_MARGIN);
        assert_eq!(push_reaching(0.0, 0.0, 100.0), -REACH_MARGIN);
        assert_eq!(push_reaching(-5.0, 0.0, 100.0), -5.0 - REACH_MARGIN);
    }
}
