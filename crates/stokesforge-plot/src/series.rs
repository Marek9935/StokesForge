//! A series: what is plotted, how it is plotted, and on which axis.
//!
//! This is where the crate touches the model, and in one way only: through
//! [`NodeId`]s. A series carries **no data** and **no column
//! name** — only the identity of the columns it plots.

use std::num::NonZeroU32;

use stokesforge_id::NodeId;

use crate::axis::AxisSide;
use crate::raw::RawVariant;
use crate::style::{LineSpec, SymbolSpec};

/// Plot type of a series. Closed set — except for [`PlotType::Unknown`],
/// which carries the plot type of a newer version of StokesForge that this
/// reader does not know (C5, level 2,
/// `guide-de-correction-robustesse.md` §F3): a series marked this way
/// draws nothing and blocks nothing, but its original content is kept
/// for a faithful rewrite.
///
/// This variant removes `Copy`/`Eq`/`Hash` from the set of derives: a
/// `RawVariant` carries a string and bytes, neither of them `Copy`,
/// and equality/hashing of unknown content has no use here.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PlotType {
    /// Scatter of points.
    Scatter,
    /// Line (default).
    #[default]
    Line,
    /// Line and markers.
    LineScatter,
    /// Bars.
    Bar,
    /// Histogram: a single variable, split into bins.
    Histogram,
    /// Line and error bars.
    ErrorBar,
    /// Plot type of a newer version of StokesForge, unknown to this
    /// reader.
    Unknown(RawVariant),
}

impl PlotType {
    /// True if this plot type joins the points with a line. Always
    /// false for [`PlotType::Unknown`]: without knowing what it is, the safest
    /// is to draw nothing.
    pub fn draws_line(&self) -> bool {
        matches!(
            self,
            PlotType::Line | PlotType::LineScatter | PlotType::ErrorBar
        )
    }

    /// True if this plot type puts a marker on each point. Always
    /// false for [`PlotType::Unknown`], same reason as `draws_line`.
    pub fn draws_symbols(&self) -> bool {
        matches!(
            self,
            PlotType::Scatter | PlotType::LineScatter | PlotType::ErrorBar
        )
    }

    /// `false` for a histogram: it only consumes one variable. True
    /// for [`PlotType::Unknown`]: not knowing, better to keep
    /// the abscissa than lose it.
    pub fn needs_x(&self) -> bool {
        !matches!(self, PlotType::Histogram)
    }
}

/// Y axis a series is attached to.
///
/// Two variants and not four, where the Python version reused its
/// `AxisSide` and refused `BOTTOM`/`TOP` at run time: here the refusal no longer
/// has a reason, the caller has no value to refuse at hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SeriesAxis {
    /// Main ordinate, on the left (default).
    #[default]
    Left,
    /// Secondary ordinate, on the right.
    Right,
}

impl SeriesAxis {
    /// The matching side of the panel, to look up the [`crate::AxisSpec`].
    pub fn side(self) -> AxisSide {
        match self {
            SeriesAxis::Left => AxisSide::Left,
            SeriesAxis::Right => AxisSide::Right,
        }
    }
}

impl From<SeriesAxis> for AxisSide {
    fn from(value: SeriesAxis) -> Self {
        value.side()
    }
}

/// Cap of [`BinCount`] (B4, `guide-de-correction-robustesse.md` §F4):
/// beyond it, a histogram no longer describes anything a screen can show
/// (a few thousand pixels wide at most, ten times less than this
/// cap), and computing it would allocate tens of gigabytes for
/// bins that no rendering will ever show.
pub const MAX_HISTOGRAM_BINS: u32 = 1 << 16;

/// Number of bins of a histogram: never zero (decision R2 — "illegal
/// states cannot be written"), never huge either (B4).
/// `NonZeroU32` already made zero inexpressible; this type tightens the other
/// end of the same interval, at the same level — in the type, not in
/// a validation that a caller could forget to call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BinCount(NonZeroU32);

impl BinCount {
    /// `None` if `value` is 0 or exceeds [`MAX_HISTOGRAM_BINS`].
    pub fn new(value: u32) -> Option<Self> {
        if value == 0 || value > MAX_HISTOGRAM_BINS {
            None
        } else {
            NonZeroU32::new(value).map(Self)
        }
    }

    /// The carried value.
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl std::fmt::Display for BinCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

/// Default number of bins of a histogram.
///
/// The `match` form is the only way to write a literal [`NonZeroU32`] in
/// a constant context without going through an `unwrap`, which the crate forbids itself.
pub const DEFAULT_HISTOGRAM_BINS: BinCount = match NonZeroU32::new(40) {
    Some(bins) => BinCount(bins),
    None => BinCount(NonZeroU32::MIN),
};

/// A series: **column identities**, a plot type, a style.
///
/// [`SeriesSpec::y`] is required and so not optional; `x` set to
/// `None` means "implicit abscissa" (the row index), which is
/// Origin's behavior when no X column is designated.
///
/// The four references are [`NodeId`]s and never names: renaming or
/// moving a column leaves the graph intact, and deleting a column
/// produces a **detectable** broken reference rather than a silently
/// empty series. Detection belongs to resolution, which
/// is the only place to see a tree.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesSpec {
    /// Ordinate column. Required: a series without Y describes nothing.
    pub y: NodeId,
    /// Abscissa column. `None` = implicit abscissa (row index).
    pub x: Option<NodeId>,
    /// Column of the X error bars.
    pub x_error: Option<NodeId>,
    /// Column of the Y error bars.
    pub y_error: Option<NodeId>,
    /// Plot type.
    pub plot_type: PlotType,
    /// Name shown in the legend. Empty = derived from the column at display time, so
    /// automatically up to date after a rename.
    pub name: String,
    /// Line style.
    pub line: LineSpec,
    /// Marker style.
    pub symbol: SymbolSpec,
    /// Ordinate it is attached to (secondary axis).
    pub axis: SeriesAxis,
    /// Series is plotted or not.
    pub visible: bool,
    /// Series appears in the legend or not.
    pub show_in_legend: bool,
    /// Bar width. `None` = derived from the abscissa step.
    pub bar_width: Option<f64>,
    /// Number of bins of a histogram, never zero, never huge (B4).
    pub histogram_bins: BinCount,
    /// Row of the sheet of `y` plotted **as a series** (#166). When present, the
    /// series is no longer "a column" but "a row": its points are
    /// the values of that row (0 = the first) in the columns of the
    /// sheet that carry an x' value, placed at the x' abscissa of their
    /// column (kinetics: one column = one instant). `y` then only serves
    /// to designate the sheet (one of its columns) and `x` is ignored.
    pub row: Option<usize>,
    /// Curve group of the panel (#177): series that carry the same
    /// number are set together (common style, color gradient). `None`: single
    /// curve. The number only has meaning within its panel; see
    /// [`crate::PanelSpec::push_series_batch`].
    pub group: Option<u32>,
}

impl SeriesSpec {
    /// Series with the default style plotting the column `y`.
    ///
    /// There is no `Default` implementation: a series without a Y column
    /// does not exist, and the constructor says so.
    pub fn new(y: NodeId) -> Self {
        Self {
            y,
            x: None,
            x_error: None,
            y_error: None,
            plot_type: PlotType::Line,
            name: String::new(),
            line: LineSpec::default(),
            symbol: SymbolSpec::default(),
            axis: SeriesAxis::Left,
            visible: true,
            show_in_legend: true,
            bar_width: None,
            histogram_bins: DEFAULT_HISTOGRAM_BINS,
            row: None,
            group: None,
        }
    }

    /// Series plotting `y` against `x`, with the default style.
    pub fn new_xy(x: NodeId, y: NodeId) -> Self {
        Self {
            x: Some(x),
            ..Self::new(y)
        }
    }

    /// Series with the default style, colored with the `index` color of the
    /// palette — line and markers together, as the UI does when a curve
    /// is added to an existing graph.
    pub fn from_palette(y: NodeId, index: usize) -> Self {
        Self {
            line: LineSpec::from_palette(index),
            symbol: SymbolSpec::from_palette(index),
            ..Self::new(y)
        }
    }

    /// All referenced columns, in the order x, y, X error, Y error.
    pub fn referenced_columns(&self) -> Vec<NodeId> {
        [self.x, Some(self.y), self.x_error, self.y_error]
            .into_iter()
            .flatten()
            .collect()
    }

    /// True if the series references this column, in any role.
    /// This is the question that deleting a column will ask.
    pub fn references(&self, column: NodeId) -> bool {
        self.x == Some(column)
            || self.y == column
            || self.x_error == Some(column)
            || self.y_error == Some(column)
    }
}
