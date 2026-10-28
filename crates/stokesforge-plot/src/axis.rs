//! The four axes of a panel: scale, limits, grid.

use crate::text_anchor::TextAnchor;

/// Scale of an axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AxisScale {
    /// Linear scale (default).
    #[default]
    Linear,
    /// Logarithmic scale.
    Log,
}

/// Side of the panel where an axis lives. All four exist; a series, though,
/// attaches to only two of them — see [`crate::SeriesAxis`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AxisSide {
    /// Abscissa, at the bottom.
    Bottom,
    /// Main ordinate, on the left.
    Left,
    /// Secondary ordinate, on the right.
    Right,
    /// Secondary abscissa, at the top.
    Top,
}

/// Direction of the tick marks of an axis (#344): outside the plot area (default, like
/// Plotters), inward, or on both sides of the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TickDirection {
    /// Toward the outside of the frame (default).
    #[default]
    Out,
    /// Toward the inside of the plot area.
    In,
    /// On both sides of the line.
    Both,
}

/// An axis: title, unit, scale, limits.
///
/// [`AxisSpec::auto_range`] takes precedence over `range_min`/`range_max`: the bounds
/// stay remembered when switching back to automatic, so that a round trip
/// through the properties panel does not lose them. This is why the
/// bounds are not merged into a single `Option<(f64, f64)>` — the
/// representation would be tighter but would lose what the user had
/// entered.
#[derive(Debug, Clone, PartialEq)]
pub struct AxisSpec {
    /// Title of the axis, without the unit.
    pub label: String,
    /// Visibility of the title, independent of its content (#85).
    pub title_visible: bool,
    /// Unit shown in parentheses after the title.
    pub unit: String,
    /// Scale.
    pub scale: AxisScale,
    /// Automatic framing on the data.
    pub auto_range: bool,
    /// Remembered lower bound.
    pub range_min: Option<f64>,
    /// Remembered upper bound.
    pub range_max: Option<f64>,
    /// Grid on this axis.
    pub grid: bool,
    /// Axis is displayed.
    pub visible: bool,
    /// Inverted axis (decreasing values).
    pub inverted: bool,
    /// Color of the axis title (`#RRGGBB`). Empty = automatic, see
    /// [`crate::PlotSpec::title_color`].
    pub title_color: String,
    /// Position of the axis title if it was moved (issue #53).
    pub title_anchor: Option<TextAnchor>,
    /// Font and size of the axis title (issue #78).
    pub title_font: crate::TextFont,
    /// Color of the scale — axis line, ticks and their labels, and
    /// the grid, dimmed (`#RRGGBB`). Empty = automatic.
    pub tick_color: String,
    /// Mirror axis (#176): on an ordinary layer, the top or right axis
    /// takes the scale, bounds and ticks of the bottom or left one,
    /// without a title. No effect on the other axes, nor on the right
    /// axis when a series is attached to it (it is then a real axis).
    pub mirror: bool,
    /// Tick labels are displayed; false = the line and the tick marks
    /// only (#176).
    pub tick_labels: bool,
    /// Tick marks are drawn; false = the line and the numbers
    /// only, the spacing of the numbers is kept (#344).
    pub ticks: bool,
    /// Direction of the tick marks (#344).
    pub tick_direction: TickDirection,
    /// Minor ticks drawn between the major ticks (#344),
    /// with or without the major tick marks. True for a new axis; an old
    /// file, which had none, reads back false.
    pub minor_ticks: bool,
    /// Minor ticks between two major ticks (#344); `None` =
    /// automatic (4 or 5 subdivisions depending on the step). No effect on a
    /// logarithmic scale, where they are the multiples 2 to 9.
    pub minor_tick_count: Option<u8>,
    /// One number out of `label_every` is written, the first always (#344); 1 =
    /// all. 0 reads as 1.
    pub label_every: u8,
    /// Length of the major tick marks in figure pixels (#344, section
    /// « Avancé »); `None` = automatic. The raw value is kept as
    /// is: rendering clamps it.
    pub tick_length: Option<f64>,
    /// Length of the minor tick marks in figure pixels; `None` = half of the
    /// effective major tick mark.
    pub minor_tick_length: Option<f64>,
    /// Thickness of the tick marks in points; `None` = automatic (1).
    pub tick_width: Option<f64>,
    /// Color of the tick marks (`#RRGGBB`), distinct from [`AxisSpec::tick_color`]
    /// (line and numbers). Empty = the color of the scale.
    pub tick_mark_color: String,
}

impl AxisSpec {
    /// Hidden axis — the default value of the top axis of a panel.
    pub fn hidden() -> Self {
        Self {
            visible: false,
            ..Self::default()
        }
    }

    /// Displayed title: `label (unit)`, without empty parentheses.
    pub fn display_label(&self) -> String {
        match (self.label.is_empty(), self.unit.is_empty()) {
            (false, false) => format!("{} ({})", self.label, self.unit),
            (false, true) => self.label.clone(),
            (true, false) => format!("({})", self.unit),
            (true, true) => String::new(),
        }
    }

    /// The axis in its canonical form: the direction is read from the order of the bounds
    /// (« De » > « À » = inverted axis), the field [`AxisSpec::inverted`] is only its
    /// trace in the file.
    ///
    /// `inverted` becomes true when the bounds are **used** and
    /// `range_min > range_max`; an inverted axis puts its remembered bounds
    /// back in decreasing order, whether `auto_range` is true or not (in
    /// automatic mode, the remembered bounds are inert, but they take back
    /// the direction of the axis). Decreasing remembered bounds under
    /// `auto_range` therefore invert nothing: the original axis did not read them.
    /// A missing or non-finite bound does not move. **The rendering is
    /// identical** to that of the original axis (`axis_range` reads "one or
    /// the other").
    #[must_use]
    pub fn orient(&self) -> Self {
        let mut axis = self.clone();
        if let (Some(min), Some(max)) = (axis.range_min, axis.range_max) {
            if !(min.is_finite() && max.is_finite()) {
                return axis;
            }
            if min > max {
                axis.inverted |= !axis.auto_range;
            } else if axis.inverted && min < max {
                axis.range_min = Some(max);
                axis.range_max = Some(min);
            }
        }
        axis
    }

    /// True when manual framing is both requested and complete: both
    /// bounds are there and automatic is off.
    pub fn has_explicit_range(&self) -> bool {
        !self.auto_range && self.range_min.is_some() && self.range_max.is_some()
    }
}

impl Default for AxisSpec {
    fn default() -> Self {
        Self {
            label: String::new(),
            unit: String::new(),
            scale: AxisScale::Linear,
            auto_range: true,
            range_min: None,
            range_max: None,
            grid: false,
            visible: true,
            inverted: false,
            title_color: String::new(),
            title_visible: true,
            title_anchor: None,
            title_font: crate::TextFont::default(),
            tick_color: String::new(),
            mirror: false,
            tick_labels: true,
            ticks: true,
            tick_direction: TickDirection::Out,
            minor_ticks: true,
            minor_tick_count: None,
            label_every: 1,
            tick_length: None,
            minor_tick_length: None,
            tick_width: None,
            tick_mark_color: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis(auto: bool, inverted: bool, bounds: (Option<f64>, Option<f64>)) -> AxisSpec {
        AxisSpec {
            auto_range: auto,
            inverted,
            range_min: bounds.0,
            range_max: bounds.1,
            ..AxisSpec::default()
        }
    }

    /// Table: (auto, inverted, bounds) -> (inverted, bounds) after `orient`.
    #[test]
    fn orient_reads_the_direction_from_the_order_of_the_bounds() {
        let b = |a, z| (Some(a), Some(z));
        let table = [
            // explicit or automatic: same rules on the remembered bounds.
            (false, false, b(1.0, 5.0), false, b(1.0, 5.0)),
            (false, false, b(5.0, 1.0), true, b(5.0, 1.0)),
            (false, true, b(1.0, 5.0), true, b(5.0, 1.0)),
            (false, true, b(5.0, 1.0), true, b(5.0, 1.0)),
            (true, false, b(1.0, 5.0), false, b(1.0, 5.0)),
            (true, false, b(5.0, 1.0), false, b(5.0, 1.0)),
            (true, true, b(1.0, 5.0), true, b(5.0, 1.0)),
            (true, true, b(5.0, 1.0), true, b(5.0, 1.0)),
            // equal, absent or partial bounds: the direction does not change.
            (false, true, b(3.0, 3.0), true, b(3.0, 3.0)),
            (true, true, (None, None), true, (None, None)),
            (true, false, (None, None), false, (None, None)),
            (false, true, (Some(1.0), None), true, (Some(1.0), None)),
            (
                false,
                false,
                (Some(f64::INFINITY), Some(1.0)),
                false,
                (Some(f64::INFINITY), Some(1.0)),
            ),
        ];
        for (auto, inverted, bounds, want_inverted, want_bounds) in table {
            let got = axis(auto, inverted, bounds).orient();
            assert_eq!(
                (got.inverted, (got.range_min, got.range_max)),
                (want_inverted, want_bounds),
                "auto={auto} inverted={inverted} bounds={bounds:?}"
            );
            assert_eq!(got.auto_range, auto, "orient does not touch auto_range");
            assert_eq!(got.orient(), got, "idempotent");
        }
    }

    #[test]
    fn hidden_only_flips_visibility() {
        let hidden = AxisSpec::hidden();
        assert!(!hidden.visible);
        assert_eq!(
            AxisSpec {
                visible: true,
                ..hidden
            },
            AxisSpec::default()
        );
    }
}
