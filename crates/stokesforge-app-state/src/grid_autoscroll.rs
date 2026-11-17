//! Speed of the grid's scroll while a selection is dragged past its edge
//! (#535), as in a spreadsheet. Pure arithmetic only, no window and no model.
//!
//! The **push** of an axis is the distance of the pointer past the edge of the
//! viewport, in points, signed: positive past the far edge (right, bottom),
//! negative past the near one (left, top), 0 inside ([`push_past`]). The speed
//! grows with the push ([`scroll_speed`]) and is capped; back inside, it is 0.
//! The arithmetic lives in `stokesforge_render::edge_push`; this module keeps
//! the grid's constants.

pub use stokesforge_render::edge_push::push_past;
use stokesforge_render::edge_push::EdgePush;

/// Scroll speed per point of push, in points per second per point (1/s).
/// Tuned at the trial on the user's screen.
pub const SPEED_PER_PUSH: f32 = 10.0;

/// The fastest scroll, in points per second.
pub const MAX_SPEED: f32 = 6000.0;

/// A frame longer than this many seconds scrolls as a frame of this length: a
/// stall must not throw the grid across the sheet.
pub const MAX_FRAME_TIME: f32 = 0.1;

const LAW: EdgePush = EdgePush {
    speed_per_push: SPEED_PER_PUSH,
    max_speed: MAX_SPEED,
    max_frame_time: MAX_FRAME_TIME,
};

/// Scroll speed, in points per second, signed as the push. Not finite: 0.
pub fn scroll_speed(push: f32) -> f32 {
    LAW.speed(push)
}

/// How far the grid scrolls this frame, in points, signed as the push: the
/// speed times the frame time (at most [`MAX_FRAME_TIME`]). Not finite: 0.
pub fn scroll_step(push: f32, dt: f32) -> f32 {
    LAW.step(push, dt)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wrapper applies the grid's constants.
    #[test]
    fn the_grid_law_uses_the_grid_constants() {
        assert_eq!(scroll_speed(1.0), SPEED_PER_PUSH);
        assert_eq!(scroll_speed(1.0e6), MAX_SPEED);
        assert_eq!(scroll_step(1.0e6, 5.0), MAX_SPEED * MAX_FRAME_TIME);
    }
}
