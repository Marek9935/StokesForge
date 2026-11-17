//! Pure arithmetic of a drag pushed past the edge of an area (#535, #512): the
//! signed distance of the pointer past the edge (the **push**), the speed it
//! sets and the step of one frame. No window, no egui, no model. The grid's
//! scroll and the growth of a handle both use it; each keeps its own gain, cap
//! and frame-time bound in an [`EdgePush`].

/// Gain, cap and frame-time bound of a push-to-speed law.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgePush {
    /// Speed per unit of push, in units per second per unit (1/s).
    pub speed_per_push: f32,
    /// The fastest speed, in units per second.
    pub max_speed: f32,
    /// A frame longer than this many seconds counts as a frame of this length.
    pub max_frame_time: f32,
}

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

impl EdgePush {
    /// Speed, in units per second, signed as the push. Not finite: 0.
    pub fn speed(&self, push: f32) -> f32 {
        (finite(push) * self.speed_per_push).clamp(-self.max_speed, self.max_speed)
    }

    /// How far to move this frame, signed as the push: the speed times the
    /// frame time (at most `max_frame_time`). Not finite: 0.
    pub fn step(&self, push: f32, dt: f32) -> f32 {
        let dt = finite(dt).clamp(0.0, self.max_frame_time);
        self.speed(push) * dt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAW: EdgePush = EdgePush {
        speed_per_push: 10.0,
        max_speed: 6000.0,
        max_frame_time: 0.1,
    };

    fn scroll_speed(push: f32) -> f32 {
        LAW.speed(push)
    }
    fn scroll_step(push: f32, dt: f32) -> f32 {
        LAW.step(push, dt)
    }
    const MAX_SPEED: f32 = 6000.0;
    const MAX_FRAME_TIME: f32 = 0.1;

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
