//! Speed of the grid's scroll while a selection is dragged past its edge
//! (#535), as in a spreadsheet. Pure arithmetic only, no window and no model.
//!
//! The **push** of an axis is the distance of the pointer past the edge of the
//! viewport, in points, signed: positive past the far edge (right, bottom),
//! negative past the near one (left, top), 0 inside ([`push_past`]). The speed
//! grows with the push ([`scroll_speed`]) and is capped; back inside, it is 0.

/// Scroll speed per point of push, in points per second per point (1/s).
/// Tuned at the trial on the user's screen.
pub const SPEED_PER_PUSH: f32 = 10.0;

/// The fastest scroll, in points per second.
pub const MAX_SPEED: f32 = 6000.0;

/// A frame longer than this many seconds scrolls as a frame of this length: a
/// stall must not throw the grid across the sheet.
pub const MAX_FRAME_TIME: f32 = 0.1;

fn finite(value: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

/// Signed distance of `pos` past the range `min..=max`:
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
        }
    }
}
