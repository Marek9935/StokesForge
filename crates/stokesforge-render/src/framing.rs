//! The margins of the automatic framing (#529).
//!
//! An axis whose range is computed from the data leaves an empty band on each
//! side of the plot area, expressed in percent of the data extent. The values
//! are a user preference (`stokesforge-app-state`), never part of the graph
//! description: a new graph writes the range they give once, and does not
//! follow them afterwards. `Bottom` and `Top` name the edges of the frame, so
//! on an inverted axis `bottom` applies to the high bound.

/// Empty space left by the automatic framing, in percent of the data extent,
/// per edge of the plot area (`1.0` = 1 %).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutoMargins {
    /// Left edge of the frame (horizontal axis).
    pub left: f64,
    /// Right edge of the frame (horizontal axis).
    pub right: f64,
    /// Bottom edge of the frame (vertical axes).
    pub bottom: f64,
    /// Top edge of the frame (vertical axes).
    pub top: f64,
}

impl Default for AutoMargins {
    /// Data edge to edge horizontally, 1 % below and 5 % above (#529).
    fn default() -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            bottom: 1.0,
            top: 5.0,
        }
    }
}
