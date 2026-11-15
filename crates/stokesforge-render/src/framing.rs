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

/// Which pair of margins an axis takes: its own direction on the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AxisRole {
    /// The bottom (and top) axis: `left` and `right` margins.
    Horizontal,
    /// The left and right axes: `bottom` and `top` margins.
    Vertical,
}

impl AutoMargins {
    /// Margins of an axis, in percent: `(low, high)` of the **data**, once
    /// the direction is known. `Bottom` and `Top` (`Left` and `Right`) are
    /// edges of the frame, so on an inverted axis the first edge is the high
    /// bound. Negative or non-finite values count as 0.
    fn along(self, role: AxisRole, inverted: bool) -> (f64, f64) {
        let (first, second) = match role {
            AxisRole::Horizontal => (self.left, self.right),
            AxisRole::Vertical => (self.bottom, self.top),
        };
        let clean = |percent: f64| {
            if percent.is_finite() {
                percent.max(0.0)
            } else {
                0.0
            }
        };
        let (first, second) = (clean(first), clean(second));
        if inverted {
            (second, first)
        } else {
            (first, second)
        }
    }
}

/// The automatic framing of an axis, in the transformed space and in
/// increasing direction: the one rule of the graph (`pipeline::padded`) and
/// of the analysis preview (`interact::fit_range`), #269.
///
/// `data` is the extent of the data (`None`: `(0, 1)`). `baseline` is the
/// transformed zero of a bar series: it is part of the extent, and a bound
/// that lands on it takes **no** margin (the bars sit on the frame). A
/// logarithmic axis snaps to whole decades and ignores the margins; a
/// constant extent widens by 5 % of its value (1 at zero) on each side;
/// a margin that would overflow leaves the bound where it is.
pub fn auto_frame(
    scale: stokesforge_plot::AxisScale,
    role: AxisRole,
    margins: AutoMargins,
    inverted: bool,
    data: Option<(f64, f64)>,
    baseline: Option<f64>,
) -> (f64, f64) {
    let Some((mut min, mut max)) = data else {
        return (0.0, 1.0);
    };
    if let Some(baseline) = baseline {
        min = min.min(baseline);
        max = max.max(baseline);
    }
    if matches!(scale, stokesforge_plot::AxisScale::Log) {
        return crate::scale::snap_to_decades((min, max));
    }
    if min == max {
        let pad = if min == 0.0 { 1.0 } else { min.abs() * 0.05 };
        return (min - pad, max + pad);
    }
    let (low, high) = margins.along(role, inverted);
    let low = if baseline == Some(min) { 0.0 } else { low };
    let high = if baseline == Some(max) { 0.0 } else { high };
    let span = max - min;
    if !span.is_finite() {
        return (min, max);
    }
    let framed = (min - span * low / 100.0, max + span * high / 100.0);
    if framed.0.is_finite() && framed.1.is_finite() {
        framed
    } else {
        (min, max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_plot::AxisScale::{Linear, Log};

    fn frame(role: AxisRole, inverted: bool, data: (f64, f64)) -> (f64, f64) {
        auto_frame(
            Linear,
            role,
            AutoMargins::default(),
            inverted,
            Some(data),
            None,
        )
    }

    fn close(actual: (f64, f64), expected: (f64, f64)) {
        assert!(
            (actual.0 - expected.0).abs() < 1e-9 && (actual.1 - expected.1).abs() < 1e-9,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn the_horizontal_axis_runs_edge_to_edge_by_default() {
        close(frame(AxisRole::Horizontal, false, (0.0, 2.0)), (0.0, 2.0));
    }

    #[test]
    fn a_vertical_axis_leaves_one_percent_below_and_five_above() {
        close(frame(AxisRole::Vertical, false, (10.0, 30.0)), (9.8, 31.0));
    }

    #[test]
    fn bottom_is_the_bottom_of_the_frame_on_an_inverted_axis() {
        close(frame(AxisRole::Vertical, true, (10.0, 30.0)), (9.0, 30.2));
    }

    #[test]
    fn a_bar_baseline_takes_no_margin_and_a_log_axis_ignores_them() {
        let bars = auto_frame(
            Linear,
            AxisRole::Vertical,
            AutoMargins::default(),
            false,
            Some((3.0, 7.0)),
            Some(0.0),
        );
        close(bars, (0.0, 7.0 + 7.0 * 0.05));
        let log = auto_frame(
            Log,
            AxisRole::Vertical,
            AutoMargins::default(),
            false,
            Some((0.0, 3.0)),
            None,
        );
        assert_eq!(log, crate::scale::snap_to_decades((0.0, 3.0)));
    }

    #[test]
    fn extreme_data_never_gives_a_non_finite_bound() {
        let (low, high) = frame(AxisRole::Vertical, false, (-f64::MAX, f64::MAX));
        assert!(low.is_finite() && high.is_finite());
    }
}
