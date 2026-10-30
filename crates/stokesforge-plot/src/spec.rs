//! The complete description of a graph, and the addressing of its series.

use std::ops::Deref;

use stokesforge_id::NodeId;

use crate::panel::PanelSpec;
use crate::series::SeriesSpec;
use crate::text_anchor::TextAnchor;
use crate::text_font::TextFont;

/// The panels of a graph: **at least one, always**.
///
/// This is the only invariant of the crate that the type system does not give
/// for free, so it is held by construction rather than checked: the
/// `Vec` is private, and no public operation can empty it. A graph
/// without a panel could not be rendered, and the Python version caught it at
/// each construction (`tuple(panels) or (PanelSpec(),)`) — here there is
/// nothing to catch because there is no state to catch.
#[derive(Debug, Clone, PartialEq)]
pub struct Panels(Vec<PanelSpec>);

impl Panels {
    /// A single panel.
    pub fn one(panel: PanelSpec) -> Self {
        Self(vec![panel])
    }

    /// The given panels, in order. **An empty sequence gives a default
    /// panel** rather than an error: this is the behavior of the Python
    /// version, and the only one that makes sense — refusing would force every caller to
    /// handle a case that has no interesting repair.
    pub fn new(panels: impl IntoIterator<Item = PanelSpec>) -> Self {
        let panels: Vec<PanelSpec> = panels.into_iter().collect();
        if panels.is_empty() {
            Self::default()
        } else {
            Self(panels)
        }
    }

    /// Adds a panel at the end.
    pub fn push(&mut self, panel: PanelSpec) {
        self.0.push(panel);
    }

    /// Removes the panels whose index is in `indices` (unknown or repeated
    /// indices are ignored). Refused, and nothing changed, if it would remove
    /// them all: the list stays non-empty. Returns whether it removed.
    pub fn remove_indices(&mut self, indices: &[usize]) -> bool {
        let keep = |index: usize| !indices.contains(&index);
        if !(0..self.0.len()).any(keep) || !(0..self.0.len()).any(|i| !keep(i)) {
            return false;
        }
        let mut index = 0;
        self.0.retain(|_| {
            let kept = keep(index);
            index += 1;
            kept
        });
        true
    }

    /// Mutable access to the panel `index`.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut PanelSpec> {
        self.0.get_mut(index)
    }

    /// The panels, mutable one by one (their number does not change: the
    /// list stays non-empty).
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, PanelSpec> {
        self.0.iter_mut()
    }

    /// The panels, read-only.
    pub fn as_slice(&self) -> &[PanelSpec] {
        &self.0
    }
}

impl Default for Panels {
    fn default() -> Self {
        Self(vec![PanelSpec::default()])
    }
}

impl Deref for Panels {
    type Target = [PanelSpec];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<PanelSpec> for Panels {
    fn from(panel: PanelSpec) -> Self {
        Self::one(panel)
    }
}

impl From<Vec<PanelSpec>> for Panels {
    fn from(panels: Vec<PanelSpec>) -> Self {
        Self::new(panels)
    }
}

/// Address of a series in a [`PlotSpec`]: the panel, then the rank in that
/// panel.
///
/// This replaces the opaque identifier that the Python version attached
/// to each series. An address is only valid for the `PlotSpec` value
/// it came from: the spec being immutable and rebuilt at each
/// change, there is no address to keep alive across an edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SeriesRef {
    /// Index of the panel in [`PlotSpec::panels`].
    pub panel: usize,
    /// Rank of the series in that panel.
    pub index: usize,
}

impl SeriesRef {
    /// Address of the series `index` of the panel `panel`.
    pub const fn new(panel: usize, index: usize) -> Self {
        Self { panel, index }
    }
}

/// Address of a series group (#197): the panel, then the group number
/// in that panel. Like [`SeriesRef`], it is only valid for the [`PlotSpec`]
/// value it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SeriesGroupRef {
    /// Index of the panel in [`PlotSpec::panels`].
    pub panel: usize,
    /// Number of the group in that panel.
    pub group: u32,
}

impl SeriesGroupRef {
    /// Address of the group `group` of the panel `panel`.
    pub const fn new(panel: usize, group: u32) -> Self {
        Self { panel, group }
    }
}

impl std::fmt::Display for SeriesRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "series {} of panel {}", self.index, self.panel)
    }
}

/// No color: white with zero opacity. The default background of a graph.
pub const TRANSPARENT: &str = "#ffffff00";

/// Default size of a StokesForge figure, in points — `(width, height)`.
///
/// The meaning of a file that records no cell size (`column_widths` and
/// `row_heights` empty): 480 × 360 pt shared equally between the cells, title
/// band included. **Fixed for good** (invariant 12): changing it would change
/// every old file. A new graph does not rely on it: it is created with
/// explicit sizes ([`PlotSpec::new_graph`]).
pub const DEFAULT_FIGURE_SIZE: (u32, u32) = (480, 360);

/// Dimensions below which a cell is no longer a plot area, in points —
/// `(width, height)`.
///
/// A panel smaller than this has room for neither its ticks nor its frame;
/// the renderer counts it as omitted rather than drawing a mess. It is also the
/// lower bound of a stored cell size ([`PlotSpec::cell_widths`]), so that a cell
/// set by the user is never omitted. Data, hence here (invariant 3): the
/// renderer and the interface read this single copy.
pub const MIN_CELL: (u32, u32) = (48, 36);

/// Upper bound of a stored cell size, in points: a PNG export enlarges the
/// figure up to four times, and a hostile file must not ask for the moon.
pub const MAX_CELL_SIZE: f64 = 2000.0;

/// Upper bound of the sum of the sizes of one axis of the grid (a figure's
/// width, or its height without the title band), in points: a grid of 256
/// cells of 2000 pt would otherwise ask for a figure of half a million points.
/// A sum beyond it is scaled down, proportionally.
pub const MAX_FIGURE_EXTENT: f64 = 20_000.0;

/// The complete description of a graph: the only object a renderer will read.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotSpec {
    /// Title of the graph.
    pub title: String,
    /// Visibility of the title, independent of its content (#85).
    pub title_visible: bool,
    /// Panels, at least one.
    pub panels: Panels,
    /// Background of the graph (`#RRGGBB` or `#RRGGBBAA`). By default **no
    /// color** ([`TRANSPARENT`]): the exported figure has no background and
    /// fits as is into another presentation. Empty = white background
    /// (what files written before this choice carry).
    pub background: String,
    /// Color of the graph title (`#RRGGBB`). Empty = automatic: the ink
    /// that contrasts with the background (dark on a light background, white on a dark
    /// one), like all the text and axes left automatic.
    pub title_color: String,
    /// Position of the graph title if it was moved (issue #53), in the
    /// data frame of the first panel; `None` = its default place.
    pub title_anchor: Option<TextAnchor>,
    /// Font and size of the graph title (issue #78).
    pub title_font: TextFont,
    /// Flag the series with a column coming from an out-of-date calculation: greyed
    /// line and "!" triangle (#198). Setting of the graph, kept with it; the
    /// confirmation on export offers a copy without the warning.
    pub stale_warning: bool,
    /// Decimal separator of the ticks: `false` = point, `true` = comma
    /// (#223, S-N4 → S-R9). Setting **of the graph**, independent of the
    /// interface language; applies to the screen and to exports.
    pub decimal_comma: bool,
    /// Width of each grid column, in points (#394). Entry `i` is column `i`.
    /// **Empty = legacy**: the figure is [`DEFAULT_FIGURE_SIZE`] wide, shared
    /// equally. Read through [`PlotSpec::cell_widths`], which sanitizes.
    pub column_widths: Vec<f64>,
    /// Height of each grid row, in points (#394), the title band **not**
    /// included: it is added on top. Empty = legacy: the figure is
    /// [`DEFAULT_FIGURE_SIZE`] high, title band included. Read through
    /// [`PlotSpec::cell_heights`].
    pub row_heights: Vec<f64>,
}

/// A stored size made usable: a missing, non-finite or non-positive entry
/// takes `fallback`, then every entry is brought into `[min, MAX_CELL_SIZE]`,
/// and a sum beyond [`MAX_FIGURE_EXTENT`] is scaled down.
fn sanitized_sizes(stored: &[f64], count: usize, fallback: f64, min: f64) -> Vec<f64> {
    let mut sizes: Vec<f64> = (0..count)
        .map(|i| {
            let value = stored
                .get(i)
                .copied()
                .filter(|v| v.is_finite() && *v > 0.0)
                .unwrap_or(fallback);
            value.clamp(min, MAX_CELL_SIZE)
        })
        .collect();
    let sum: f64 = sizes.iter().sum();
    if sum > MAX_FIGURE_EXTENT {
        let factor = MAX_FIGURE_EXTENT / sum;
        sizes.iter_mut().for_each(|size| *size *= factor);
    }
    sizes
}

impl PlotSpec {
    /// A graph as the interface creates it: one empty panel, with **explicit**
    /// cell sizes so that the figure is [`DEFAULT_FIGURE_SIZE`] (no title yet;
    /// a title added later makes the figure taller, #394).
    pub fn new_graph() -> Self {
        Self {
            column_widths: vec![f64::from(DEFAULT_FIGURE_SIZE.0)],
            row_heights: vec![f64::from(DEFAULT_FIGURE_SIZE.1)],
            ..Self::default()
        }
    }

    /// The width of each grid column in points, sanitized, or `None` if the
    /// graph is legacy (no stored width). Always [`PlotSpec::columns`]
    /// entries: missing, non-finite or non-positive entries read 480,
    /// every entry is clamped to `[MIN_CELL.0, MAX_CELL_SIZE]`; entries beyond
    /// the grid are ignored (but kept in the field).
    pub fn cell_widths(&self) -> Option<Vec<f64>> {
        (!self.column_widths.is_empty()).then(|| {
            sanitized_sizes(
                &self.column_widths,
                self.columns(),
                f64::from(DEFAULT_FIGURE_SIZE.0),
                f64::from(MIN_CELL.0),
            )
        })
    }

    /// The height of each grid row in points, sanitized (see
    /// [`PlotSpec::cell_widths`]; the fallback is 360 and the lower bound
    /// `MIN_CELL.1`), or `None` if the graph is legacy.
    pub fn cell_heights(&self) -> Option<Vec<f64>> {
        (!self.row_heights.is_empty()).then(|| {
            sanitized_sizes(
                &self.row_heights,
                self.rows(),
                f64::from(DEFAULT_FIGURE_SIZE.1),
                f64::from(MIN_CELL.1),
            )
        })
    }

    /// An empty but valid graph: one panel, no series.
    pub fn empty(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    /// Number of rows of the panel grid.
    ///
    /// B2 (`guide-de-correction-robustesse.md` §F3/F4/rendu) —
    /// `saturating_add(1)`: a panel position coming from a file can
    /// be `usize::MAX` (nothing bounds it on read, see C5/the
    /// reader), and `+ 1` panics in debug, overflows to 0 in release.
    pub fn rows(&self) -> usize {
        self.panels
            .iter()
            .map(|p| p.row)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }

    /// Number of columns of the panel grid. Not to be confused with
    /// [`PlotSpec::referenced_columns`]: here they are grid columns,
    /// there data columns. See [`PlotSpec::rows`] for the
    /// `saturating_add`.
    pub fn columns(&self) -> usize {
        self.panels
            .iter()
            .map(|p| p.column)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }

    /// Brings each axis of each panel to its canonical form
    /// ([`crate::AxisSpec::orient`]). On opening a file and after a script;
    /// the rendering of an oriented spec is that of the original spec.
    pub fn orient_axes(&mut self) {
        for panel in self.panels.iter_mut() {
            for axis in [
                &mut panel.bottom,
                &mut panel.left,
                &mut panel.right,
                &mut panel.top,
            ] {
                *axis = axis.orient();
            }
        }
    }

    /// All the series of the graph, with their address, panel by panel.
    pub fn iter_series(&self) -> impl Iterator<Item = (SeriesRef, &SeriesSpec)> {
        self.panels.iter().enumerate().flat_map(|(panel, spec)| {
            spec.series
                .iter()
                .enumerate()
                .map(move |(index, series)| (SeriesRef::new(panel, index), series))
        })
    }

    /// The series at this address, if it exists.
    pub fn series_at(&self, at: SeriesRef) -> Option<&SeriesSpec> {
        self.panels.get(at.panel)?.series.get(at.index)
    }

    /// The series at this address, mutable.
    pub fn series_at_mut(&mut self, at: SeriesRef) -> Option<&mut SeriesSpec> {
        self.panels.get_mut(at.panel)?.series.get_mut(at.index)
    }

    /// Total number of series, all panels together.
    pub fn series_count(&self) -> usize {
        self.panels.iter().map(|p| p.series.len()).sum()
    }

    /// Columns referenced by all the series, without duplicates, in
    /// order of first appearance.
    ///
    /// This is the list that resolution will look up in the tree, and the one
    /// that a column deletion will have to consult to know which graphs
    /// it breaks.
    pub fn referenced_columns(&self) -> Vec<NodeId> {
        let mut seen: Vec<NodeId> = Vec::new();
        for (_, series) in self.iter_series() {
            for column in series.referenced_columns() {
                if !seen.contains(&column) {
                    seen.push(column);
                }
            }
        }
        seen
    }

    /// True if a series of the graph references this column.
    pub fn references(&self, column: NodeId) -> bool {
        self.panels.iter().any(|p| p.references(column))
    }

    /// The same graph with other panels.
    pub fn with_panels(self, panels: impl Into<Panels>) -> Self {
        Self {
            panels: panels.into(),
            ..self
        }
    }
}

impl Default for PlotSpec {
    fn default() -> Self {
        Self {
            title: String::new(),
            panels: Panels::default(),
            background: TRANSPARENT.to_string(),
            title_color: String::new(),
            title_visible: true,
            title_anchor: None,
            title_font: TextFont::default(),
            stale_warning: true,
            decimal_comma: false,
            column_widths: Vec::new(),
            row_heights: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new graph has no background: "no color" is the default.
    #[test]
    fn the_default_background_has_no_color() {
        assert_eq!(PlotSpec::default().background, TRANSPARENT);
        assert_eq!(PlotSpec::empty("Title").background, TRANSPARENT);
        assert!(TRANSPARENT.ends_with("00"), "zero opacity");
    }

    #[test]
    fn panels_are_never_empty_even_when_asked_to_be() {
        assert_eq!(Panels::new(Vec::new()).len(), 1);
        assert_eq!(Panels::default().len(), 1);
    }

    #[test]
    fn panels_keep_the_order_they_were_given() {
        let panels = Panels::new([PanelSpec::at(0, 0), PanelSpec::at(1, 0)]);
        assert_eq!(panels.len(), 2);
        assert_eq!(panels[1].row, 1);
    }

    fn three_columns() -> PlotSpec {
        PlotSpec::default().with_panels(Panels::new([
            PanelSpec::at(0, 0),
            PanelSpec::at(0, 1),
            PanelSpec::at(0, 2),
        ]))
    }

    /// Hostile or damaged widths become usable ones, never a panic.
    #[test]
    fn stored_widths_are_sanitized() {
        let mut spec = three_columns();
        assert_eq!(spec.cell_widths(), None, "legacy: nothing stored");
        spec.column_widths = vec![200.0, f64::NAN, -5.0];
        assert_eq!(spec.cell_widths(), Some(vec![200.0, 480.0, 480.0]));
        spec.column_widths = vec![1.0, 1e9, f64::INFINITY];
        assert_eq!(
            spec.cell_widths(),
            Some(vec![f64::from(MIN_CELL.0), MAX_CELL_SIZE, 480.0])
        );
        spec.column_widths = vec![300.0];
        assert_eq!(spec.cell_widths(), Some(vec![300.0, 480.0, 480.0]));
        spec.column_widths = vec![300.0, 300.0, 300.0, 999.0, 999.0];
        assert_eq!(
            spec.cell_widths().unwrap().len(),
            3,
            "extra entries ignored"
        );
        assert_eq!(spec.column_widths.len(), 5, "but kept for the round trip");
    }

    #[test]
    fn a_figure_beyond_the_extent_cap_is_scaled_down_proportionally() {
        let mut spec = PlotSpec::default().with_panels(Panels::new(
            (0..20).map(|c| PanelSpec::at(0, c)).collect::<Vec<_>>(),
        ));
        spec.column_widths = vec![MAX_CELL_SIZE; 20];
        let widths = spec.cell_widths().unwrap();
        assert!((widths.iter().sum::<f64>() - MAX_FIGURE_EXTENT).abs() < 1e-6);
        assert!(widths.iter().all(|w| (w - 1000.0).abs() < 1e-9));
    }

    #[test]
    fn stored_heights_are_sanitized() {
        let mut spec = PlotSpec {
            row_heights: vec![0.0],
            ..PlotSpec::default()
        };
        assert_eq!(spec.cell_heights(), Some(vec![360.0]));
        spec.row_heights = vec![5.0];
        assert_eq!(spec.cell_heights(), Some(vec![f64::from(MIN_CELL.1)]));
    }

    #[test]
    fn a_new_graph_is_explicitly_480_by_360() {
        let spec = PlotSpec::new_graph();
        assert_eq!(spec.cell_widths(), Some(vec![480.0]));
        assert_eq!(spec.cell_heights(), Some(vec![360.0]));
    }

    #[test]
    fn a_series_ref_reads_as_english() {
        assert_eq!(SeriesRef::new(1, 0).to_string(), "series 0 of panel 1");
    }
}
