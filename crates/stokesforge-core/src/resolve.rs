//! Resolution of a [`PlotSpec`] against the current tree: turns
//! references (`NodeId`) into data ready to plot.
//!
//! Port of `legacy-python/src/stokesforge/plot/resolve.py`. This is the only module
//! of the crate that knows `stokesforge-plot` — exactly as `resolve.py` was
//! the only module of `stokesforge.plot` that knew `stokesforge.core` on the Python side. The
//! boundary is the same, just inverted: there the neutral layer imported
//! the model to resolve; here the model imports the neutral
//! layer to carry the spec (`NodeKind::Graph`), and resolution remains
//! the place that brings them together.
//!
//! # Broken references: a first-class state, not an exception
//!
//! Quote from the top-level documentation of `resolve.py`, carried over as is
//! (it is directly decision §11.1 of `architecture-choisie.md`):
//!
//! > The stance of Python is to make it a **first-class state**
//! > rather than an exception: `ResolvedSeries` always exists, with
//! > `missing` non-empty and `ok` false.
//!
//! Nothing crashes; the series is not removed from the spec; it stays visible,
//! marked as a broken reference ([`ResolvedSeries::broken`]). Renaming or
//! moving the referenced column breaks nothing (the reference is a
//! `NodeId`); deleting it produces this detectable state, never a silently
//! empty series.
//!
//! # Zero copy
//!
//! The columns of the model are already `Arc<Vec<f64>>`
//! ([`Column::float_values`]). Resolution clones the `Arc` (a reference
//! counter increment), never the buffer: [`ResolvedValues`] is a view
//! on the same memory as the column, possibly truncated to the common prefix
//! of a series whose columns have different lengths.
//!
//! **One exception, and only one**: a mixed-type column
//! ([`stokesforge_core::ColumnType::Mixed`](crate::ColumnType::Mixed)) has no
//! contiguous `f64` buffer — its cells are a `Vec<MixedCell>` — and there is
//! therefore nothing to share. [`Column::numeric_view`] materializes one, where
//! text and empty cells are `NaN`, which the renderer already discards:
//! a mixed column plots exactly its numeric cells. The cost is
//! `O(n)` per resolution and **per mixed column referenced by a series**,
//! never per column of the project, and it touches no homogeneous column.
//!
//! # What is not ported, and why
//!
//! `prepare()` and `ComputeCancelled` of `resolve.py` are **specific to
//! Python's async**: `prepare()` wraps `resolve_spec` in a progress
//! report and a cooperative cancellation designed for a background-thread
//! computation (`stokesforge.ui.plot_worker`, `QThread`). This mechanism does not exist
//! on the Rust side — automatic recalculation is neither built nor planned
//! (`architecture-choisie.md` §11.4) — and porting it would invent a
//! cancellation infrastructure that nothing depends on yet. [`resolve_spec`]
//! covers the part that remains: resolution itself, which never walks
//! the data (cost independent of the number of points, as in
//! Python). The histogram precomputation that `prepare()` interleaved is
//! available separately ([`histogram`]): a future renderer calls it
//! itself on the `Histogram` series it meets.

use std::num::NonZeroU32;
use std::sync::Arc;

use stokesforge_plot::{PanelSpec, PlotSpec, SeriesSpec};

use crate::column::Column;
use crate::id::NodeId;
use crate::node::Node;
use crate::tree::Tree;

/// Role of a series reference that could not be resolved.
///
/// Replaces the strings `"x"`/`"y"`/`"xErr"`/`"yErr"` of `resolve.py` with a
/// closed set — same reasoning as everywhere else in the Rust port
/// (`stokesforge-plot`): a named enumeration rather than a vocabulary of
/// strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MissingRole {
    /// X column.
    X,
    /// Y column.
    Y,
    /// X error bar column.
    XError,
    /// Y error bar column.
    YError,
}

/// Zero-copy view, possibly truncated, on the resolved `f64` values
/// of a column.
///
/// Carries the same `Arc` as the original [`Column`]: cloning it only increments
/// a reference counter, never the bytes. The length carried here
/// may be smaller than that of the buffer — it is the common prefix of a
/// series whose columns do not have the same length (columns of free
/// length, a decision of the core), instead of a refusal to plot.
#[derive(Debug, Clone)]
pub struct ResolvedValues {
    data: Arc<Vec<f64>>,
    len: usize,
}

impl ResolvedValues {
    fn new(data: Arc<Vec<f64>>, len: usize) -> Self {
        debug_assert!(
            len <= data.len(),
            "a resolved view cannot exceed its buffer"
        );
        Self { data, len }
    }

    /// The values, up to the possible truncation.
    pub fn as_slice(&self) -> &[f64] {
        &self.data[..self.len]
    }

    /// Number of visible values (after possible truncation).
    pub fn len(&self) -> usize {
        self.len
    }

    /// True if no value is visible.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// A resolved series and its arrays, or the description of what is missing.
///
/// Borrows `spec` from the caller rather than cloning it: `SeriesSpec` does not
/// carry a large buffer (it is identities and style), but nothing
/// forces duplicating even the light part when a borrow is enough —
/// unlike `resolve.py`, where the Python dataclass is in any case
/// an object shared by reference.
#[derive(Debug)]
pub struct ResolvedSeries<'s> {
    /// The original series.
    pub spec: &'s SeriesSpec,
    /// Displayed name: the spec's if set, otherwise that of the Y
    /// column, otherwise [`SeriesLabel::Missing`]. Data, not a sentence: the
    /// interface phrases the placeholders.
    pub label: SeriesLabel,
    /// X values. `None` if the series has none (histogram) or if the
    /// reference is broken.
    pub x: Option<ResolvedValues>,
    /// Y values. `None` only if the reference is broken.
    pub y: Option<ResolvedValues>,
    /// X error bars, if requested and resolved.
    pub x_error: Option<ResolvedValues>,
    /// Y error bars, if requested and resolved.
    pub y_error: Option<ResolvedValues>,
    /// Roles whose referenced column cannot be found. Non-empty ⇔ the
    /// series is a broken reference ([`ResolvedSeries::broken`]).
    pub missing: Vec<MissingRole>,
    /// True if the resolved columns did not all have the same length:
    /// the plot then covers their common prefix instead of being refused.
    pub truncated: bool,
    /// True if a plotted column is written by an out-of-date analysis calculation
    /// (#198). The rendering knows no more: it greys the series out and places its
    /// marker; where the staleness comes from remains the model's business.
    pub stale: bool,
}

impl<'s> ResolvedSeries<'s> {
    /// True if the series is plottable as it is.
    pub fn ok(&self) -> bool {
        self.missing.is_empty() && self.y.as_ref().is_some_and(|y| !y.is_empty())
    }

    /// True if at least one referenced column cannot be found.
    pub fn broken(&self) -> bool {
        !self.missing.is_empty()
    }

    /// Number of points of the series (0 if the reference is broken).
    pub fn point_count(&self) -> usize {
        self.y.as_ref().map_or(0, ResolvedValues::len)
    }
}

/// A resolved panel: the original panel, and its resolved series in
/// stacking order.
#[derive(Debug)]
pub struct ResolvedPanel<'s> {
    /// The original panel.
    pub spec: &'s PanelSpec,
    /// Its series, resolved, in the same order.
    pub series: Vec<ResolvedSeries<'s>>,
}

/// A resolved graph: all its panels, resolved.
#[derive(Debug)]
pub struct ResolvedPlot<'s> {
    /// The original spec.
    pub spec: &'s PlotSpec,
    /// Its panels, resolved, in the same order.
    pub panels: Vec<ResolvedPanel<'s>>,
}

impl<'s> ResolvedPlot<'s> {
    /// The series whose reference is broken, all panels together.
    pub fn broken_series(&self) -> impl Iterator<Item = &ResolvedSeries<'s>> {
        self.panels
            .iter()
            .flat_map(|panel| panel.series.iter())
            .filter(|series| series.broken())
    }

    /// Total number of plotted points, all panels and series together.
    pub fn point_count(&self) -> usize {
        self.panels
            .iter()
            .flat_map(|panel| panel.series.iter())
            .map(ResolvedSeries::point_count)
            .sum()
    }
}

/// Column registered under `id`, or `None` if the reference is broken.
///
/// `None` covers two cases deliberately indistinct for the caller: node
/// absent from the current tree (deleted, awaiting an `undo`), or node
/// present but not a column. In both there is nothing to
/// plot — port of `resolve_column` (`resolve.py` line 68), which treats these
/// two cases the same way on the Python side (`project.registry.get` returning either
/// nothing, or an object of another type).
pub fn resolve_column(tree: &Tree, id: NodeId) -> Option<&Column> {
    tree.node(id).and_then(Node::as_column)
}

/// Displayed name of a series, as data. Core makes no sentence: the
/// placeholders are phrased by the interface (Fluent keys in
/// `stokesforge-app-state`, English default in the renderer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeriesLabel {
    /// A name: the spec's, the long name of the Y column, or its node name.
    Text(String),
    /// An unnamed "row" series (#166): the 0-based row (the interface shows
    /// `row + 1`, as the grid does).
    Row(usize),
    /// An unnamed series whose Y column no longer exists.
    Missing,
}

impl SeriesLabel {
    /// The name if there is one (`Text`), otherwise `None`.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }
}

/// Displayed name of a series: `SeriesSpec::name` if set, otherwise the
/// long name of the Y column, otherwise its node name, otherwise
/// [`SeriesLabel::Missing`] if the column no longer exists.
///
/// **Single source**: the graph legend (via [`resolve_series`]) and the
/// series list of the properties window (`stokesforge-app`) both call it,
/// so that a rename or a rule change reaches
/// both (issue #25). It lives here, in `stokesforge-core`, because it is
/// the place that knows how to read a column; the interface need not rewrite it.
pub fn series_label(series: &SeriesSpec, tree: &Tree) -> SeriesLabel {
    if !series.name.is_empty() {
        return SeriesLabel::Text(series.name.clone());
    }
    if let Some(row) = series.row {
        // "Row" series (#166): the row is its name, as in the grid.
        return SeriesLabel::Row(row);
    }
    let Some(y_node) = tree.node(series.y).filter(|node| node.kind().is_column()) else {
        return SeriesLabel::Missing;
    };
    let long_name = y_node
        .as_column()
        .map(|column| column.meta().long_name.as_str())
        .unwrap_or_default();
    SeriesLabel::Text(if long_name.is_empty() {
        y_node.name().to_string()
    } else {
        long_name.to_string()
    })
}

/// `f64` view of a column referenced by `id`, or `None` if the reference is
/// broken (port of `_numeric`, `resolve.py` line 77). There is no longer a
/// non-plottable column type: a `Mixed` column — including a
/// fully textual column — plots its numeric cells and is `NaN`
/// elsewhere (see [`Column::numeric_view`]).
///
/// Zero copy for a numeric or date-time column; one copy for a
/// **mixed** column, which has no contiguous `f64` buffer to share — see
/// [`Column::numeric_view`], which carries the detail and the cost of this sole
/// exception to the "zero copy" property of this module.
fn resolve_numeric(tree: &Tree, id: NodeId) -> Option<Arc<Vec<f64>>> {
    resolve_column(tree, id).and_then(Column::numeric_view)
}

/// Resolves a series. Never raises: a broken reference is a state
/// (port of `resolve_series`, `resolve.py` line 165).
pub fn resolve_series<'s>(tree: &Tree, series: &'s SeriesSpec) -> ResolvedSeries<'s> {
    let mut resolved = resolve_series_data(tree, series);
    resolved.stale = [Some(series.y), series.x, series.x_error, series.y_error]
        .into_iter()
        .flatten()
        .any(|column| column_is_stale(tree, column));
    resolved
}

/// True if `column` is written by an out-of-date analysis calculation.
fn column_is_stale(tree: &Tree, column: NodeId) -> bool {
    tree.calculation_of_column(column)
        .and_then(|(id, _)| tree.calculation(id))
        .is_some_and(|calc| calc.stale.is_some())
}

fn resolve_series_data<'s>(tree: &Tree, series: &'s SeriesSpec) -> ResolvedSeries<'s> {
    let label = series_label(series, tree);
    if let Some(row) = series.row {
        return resolve_row_series(tree, series, row, label);
    }
    let mut missing = Vec::new();

    let y = resolve_numeric(tree, series.y);
    if y.is_none() {
        missing.push(MissingRole::Y);
    }

    let mut x = None;
    if series.plot_type.needs_x() {
        if let Some(x_id) = series.x {
            x = resolve_numeric(tree, x_id);
            if x.is_none() {
                missing.push(MissingRole::X);
            }
        }
    }

    let mut x_error = None;
    if let Some(id) = series.x_error {
        x_error = resolve_numeric(tree, id);
        if x_error.is_none() {
            missing.push(MissingRole::XError);
        }
    }

    let mut y_error = None;
    if let Some(id) = series.y_error {
        y_error = resolve_numeric(tree, id);
        if y_error.is_none() {
            missing.push(MissingRole::YError);
        }
    }

    if !missing.is_empty() {
        return ResolvedSeries {
            spec: series,
            label,
            x: None,
            y: None,
            x_error: None,
            y_error: None,
            missing,
            truncated: false,
            stale: false,
        };
    }

    // Columns of free length (a decision of the core): the plot covers the
    // common prefix instead of being refused.
    //
    // A10 (`guide-de-correction-robustesse.md` §F1): `n` — the number of
    // points of the series — comes only from the **data** columns (`x`,
    // `y`), which go in pairs and are truncated together. The error
    // columns are **per-point annotations**: they do not take part in
    // this computation, otherwise a too-short (or empty) error column
    // would truncate the signal itself — which is exactly the anomaly.
    let lengths: Vec<usize> = [&x, &y]
        .into_iter()
        .flatten()
        .map(|values| values.len())
        .collect();
    let n = lengths.iter().copied().min().unwrap_or(0);
    let error_lengths: Vec<usize> = [&x_error, &y_error]
        .into_iter()
        .flatten()
        .map(|values| values.len())
        .collect();
    let truncated =
        lengths.iter().any(|&len| len != n) || error_lengths.iter().any(|&len| len != n);

    if x.is_none() && series.plot_type.needs_x() {
        // Implicit x: the row index, Origin style when no X
        // column is designated. Built on the fly, never stored.
        x = Some(Arc::new((0..n).map(|i| i as f64).collect()));
    }

    ResolvedSeries {
        spec: series,
        label,
        x: x.map(|values| ResolvedValues::new(values, n)),
        y: y.map(|values| ResolvedValues::new(values, n)),
        x_error: x_error.map(|values| conform_error_values(values, n)),
        y_error: y_error.map(|values| conform_error_values(values, n)),
        missing,
        truncated,
        stale: false,
    }
}

/// Resolves a "row" series (#166): one value per column of the sheet of
/// `series.y` that carries a finite x' value, in sheet order —
/// y = the cell of row `row` (`NaN` if the column is shorter,
/// or if the cell is not a number), x = the x' of the
/// column. A sheet with no x' column gives an empty series, not a
/// broken reference; only the loss of the anchor column (`y`) is one.
fn resolve_row_series<'s>(
    tree: &Tree,
    series: &'s SeriesSpec,
    row: usize,
    label: SeriesLabel,
) -> ResolvedSeries<'s> {
    let broken = |label: SeriesLabel| ResolvedSeries {
        spec: series,
        label,
        x: None,
        y: None,
        x_error: None,
        y_error: None,
        missing: vec![MissingRole::Y],
        truncated: false,
        stale: false,
    };
    let Some(sheet) =
        resolve_column(tree, series.y).and_then(|_| tree.parent(series.y).ok().flatten())
    else {
        return broken(label);
    };
    let (mut x, mut y) = (Vec::new(), Vec::new());
    for &id in tree.node(sheet).map(Node::children).unwrap_or_default() {
        let Some(column) = resolve_column(tree, id) else {
            continue;
        };
        let Some(x_prime) = column.meta().x_prime.filter(|value| value.is_finite()) else {
            continue;
        };
        x.push(x_prime);
        y.push(
            column
                .numeric_view()
                .and_then(|values| values.get(row).copied())
                .unwrap_or(f64::NAN),
        );
    }
    let n = x.len();
    ResolvedSeries {
        spec: series,
        label,
        x: Some(ResolvedValues::new(Arc::new(x), n)),
        y: Some(ResolvedValues::new(Arc::new(y), n)),
        x_error: None,
        y_error: None,
        missing: Vec::new(),
        truncated: false,
        stale: false,
    }
}

/// Conforms an error column (error bar in X or in Y) to the length
/// `n` of the data it annotates (A10, `guide-de-correction-robustesse.md`
/// §F1): truncated if it is longer, **padded with `NaN`** — the model's
/// missing value — if it is shorter, never the reverse
/// (shortening `x`/`y` to align with it).
///
/// Zero copy in the usual case (enough values, or too many): it is only
/// a truncated view on the same `Arc`, like [`ResolvedValues::new`]
/// everywhere else. A too-short error column is the rare case; it
/// alone pays a copy (the original buffer, padded with `NaN`), rather than
/// making `x`/`y` pay for a truncation that would lose measurements.
fn conform_error_values(values: Arc<Vec<f64>>, n: usize) -> ResolvedValues {
    if values.len() >= n {
        return ResolvedValues::new(values, n);
    }
    let mut padded = (*values).clone();
    padded.resize(n, f64::NAN);
    ResolvedValues::new(Arc::new(padded), n)
}

/// Resolves all the series of a panel, in order.
pub fn resolve_panel<'s>(tree: &Tree, panel: &'s PanelSpec) -> ResolvedPanel<'s> {
    ResolvedPanel {
        spec: panel,
        series: panel
            .series
            .iter()
            .map(|series| resolve_series(tree, series))
            .collect(),
    }
}

/// Resolves all the series of a graph. Never raises.
///
/// Cost independent of the number of points: one `NodeId` resolution and one
/// `Arc` clone per referenced column, never a copy of the data
/// (port of `resolve_spec`, `resolve.py` line 227).
pub fn resolve_spec<'s>(tree: &Tree, spec: &'s PlotSpec) -> ResolvedPlot<'s> {
    let mut panels: Vec<ResolvedPanel<'s>> = spec
        .panels
        .iter()
        .map(|panel| resolve_panel(tree, panel))
        .collect();
    // The graph may waive the warning for out-of-date series (#198).
    if !spec.stale_warning {
        for series in panels.iter_mut().flat_map(|panel| panel.series.iter_mut()) {
            series.stale = false;
        }
    }
    ResolvedPlot { spec, panels }
}

/// Width of the bars: the requested one if positive, otherwise 80% of the
/// median step of the x values.
///
/// Here, in the core, and not in a renderer: the deduction must not
/// give two different widths depending on the engine that paints — this is the
/// same reason that gave birth to `PlotSpec` (port of `bar_width`,
/// `resolve.py` line 287).
///
/// `requested` is already an `Option<f64>` rather than the Python `float`
/// zero sentinel: this is the decision of R2 (`SeriesSpec::bar_width`,
/// `None` = deduced), so no `> 0` test to do here to tell it apart
/// from an explicit value of zero.
pub fn bar_width(x: Option<&[f64]>, requested: Option<f64>) -> f64 {
    // A6 — an absurd requested width (negative, zero, NaN or infinite)
    // is not a width: it is treated as absent and we fall back on
    // the deduction, rather than returning it as is to the renderer.
    if let Some(requested) = requested.filter(|w| w.is_finite() && *w > 0.0) {
        return requested;
    }
    let Some(x) = x else {
        return 1.0;
    };
    if x.len() < 2 {
        return 1.0;
    }
    let mut sorted = x.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mut steps: Vec<f64> = sorted
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .filter(|step| step.is_finite() && *step > 0.0)
        .collect();
    if steps.is_empty() {
        return 1.0;
    }
    steps.sort_by(f64::total_cmp);
    median_of_sorted(&steps) * 0.8
}

/// Median of an already sorted slice (mean of the two central values if
/// the length is even, like `numpy.median`).
fn median_of_sorted(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// Bounds and counts of a histogram, `NaN` excluded ("finite" in the Python
/// code).
///
/// In the core, with no heavy computation dependency (no `numpy` — nor a
/// Rust equivalent like `ndarray`: `stokesforge-core` stays free of
/// computation dependencies, brief of phase R3), so that the computation is
/// testable alone and shared by any future renderer (port of `histogram`,
/// `resolve.py` line 304). `bins` is already a `NonZeroU32` on the
/// `SeriesSpec::histogram_bins` side — no `max(1, …)` to redo here, R2 has
/// already made it impossible to express zero classes.
pub fn histogram(values: &[f64], bins: NonZeroU32) -> (Vec<f64>, Vec<f64>) {
    let finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return (vec![0.0], Vec::new());
    }
    let bins = bins.get() as usize;

    let mut min = finite[0];
    let mut max = finite[0];
    for &v in &finite[1..] {
        min = min.min(v);
        max = max.max(v);
    }
    // `numpy.histogram` widens a degenerate interval by 0.5 on each
    // side rather than dividing by a zero width.
    let (first_edge, last_edge) = if min == max {
        (min - 0.5, max + 0.5)
    } else {
        (min, max)
    };

    // A5 (`guide-de-correction-robustesse.md`, "A deadline, not a
    // cause") — `last_edge - first_edge` can overflow `f64` (two values
    // close to `±f64::MAX`), which made `width = +inf`, then `NaN`
    // bounds (`inf * 0.0`) and a count that put everything in bin
    // 0. We avoid any non-finite intermediate:
    //
    // - the bounds are computed by convex combination
    //   `first*(1-t) + last*t`, which always stays within `[first, last]`
    //   (hence finite) whatever `first`/`last`, without ever going through
    //   a difference that could overflow;
    // - the position of a value in the histogram is computed via
    //   `half_width = last/2 - first/2` — half the width, but
    //   computed by dividing each bound by two first, never
    //   overflowing — then `t = (v/2 - first/2) / half_width`, which gives the
    //   same result as `(v - first) / width` without ever materializing a
    //   non-finite difference.
    let half_width = last_edge / 2.0 - first_edge / 2.0;

    let edges: Vec<f64> = (0..=bins)
        .map(|i| {
            let t = (i as f64) / (bins as f64);
            first_edge * (1.0 - t) + last_edge * t
        })
        .collect();

    let mut counts = vec![0.0f64; bins];
    for &v in &finite {
        let t = (v / 2.0 - first_edge / 2.0) / half_width;
        let position = t * bins as f64;
        let index = (position.floor() as isize).clamp(0, bins as isize - 1) as usize;
        counts[index] += 1.0;
    }

    (edges, counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::column::{Column, ColumnMeta, ColumnType, ValueBlock};
    use crate::node::WindowMeta;
    use crate::project::Project;
    use stokesforge_plot::{PanelSpec, PlotType, SeriesSpec};

    fn numeric_column(values: &[f64]) -> Column {
        Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(values.to_vec()),
            ColumnMeta::default(),
        )
        .expect("valid numeric column")
    }

    /// Fixture: a project with a sheet containing the columns `x`, `y`
    /// (two points) and `text` (an entirely textual mixed column, with no
    /// numeric content), plus a graph.
    struct Fixture {
        project: Project,
        x: NodeId,
        y: NodeId,
        text: NodeId,
        graph: NodeId,
    }

    impl Fixture {
        fn new() -> Self {
            let mut project = Project::new("Demo").expect("valid project");
            let root = project.root();
            let book = project.add_empty_workbook(root, None).expect("workbook");
            let sheet = project.add_worksheet(book, None).expect("sheet");
            let x = project
                .add_column(sheet, None, Some("X"), numeric_column(&[0.0, 1.0, 2.0]))
                .expect("column X");
            let y = project
                .add_column(sheet, None, Some("Y"), numeric_column(&[10.0, 20.0, 30.0]))
                .expect("column Y");
            let text = project
                .add_column(
                    sheet,
                    None,
                    Some("T"),
                    Column::from_text(&[Some("a".to_string()), Some("b".to_string())]),
                )
                .expect("text column");
            let graph = project
                .add_graph(root, None, PlotSpec::default())
                .expect("graph");
            Self {
                project,
                x,
                y,
                text,
                graph,
            }
        }
    }

    #[test]
    fn a_fully_resolved_series_carries_zero_copy_views() {
        let f = Fixture::new();
        let series = SeriesSpec::new_xy(f.x, f.y);
        let resolved = resolve_series(f.project.tree(), &series);

        assert!(resolved.ok());
        assert!(!resolved.broken());
        assert_eq!(resolved.missing, Vec::new());
        assert_eq!(resolved.point_count(), 3);
        assert_eq!(resolved.x.unwrap().as_slice(), &[0.0, 1.0, 2.0]);
        assert_eq!(resolved.y.unwrap().as_slice(), &[10.0, 20.0, 30.0]);
    }

    /// Task 3.6, first part: renaming a column referenced by a
    /// graph leaves the graph resolvable — the reference is a `NodeId`, the
    /// rename does not touch it (decision §11.1 of `architecture-choisie.md`).
    #[test]
    fn renaming_the_referenced_column_leaves_the_series_resolvable() {
        let f = Fixture::new();
        let mut project = f.project;
        let series = SeriesSpec::new_xy(f.x, f.y);

        let before = resolve_series(project.tree(), &series);
        assert!(before.ok());

        project.rename(f.y, "Voltage").expect("valid rename");

        let after = resolve_series(project.tree(), &series);
        assert!(after.ok(), "the rename must not break resolution");
        assert!(!after.broken());
        assert_eq!(after.y.unwrap().as_slice(), &[10.0, 20.0, 30.0]);
        // The displayed name follows the rename: the column is now called
        // "Voltage", and nothing in the spec freezes the old name "Y".
        assert_eq!(
            series_label(&series, project.tree()),
            SeriesLabel::Text("Voltage".to_string()),
            "the displayed name must follow the column rename"
        );
    }

    /// Task 3.6, second part: deleting the referenced column produces an
    /// **explicit** broken reference in the resolution result — never
    /// a silently empty series. It is `ResolvedSeries::ok() == false`
    /// with `missing` non-empty, not a panic nor a series removed from the spec.
    #[test]
    fn a_broken_reference_is_a_state_not_a_panic() {
        let f = Fixture::new();
        let mut project = f.project;
        project.remove(f.y).expect("deletion of column Y");
        let series = SeriesSpec::new_xy(f.x, f.y);

        let resolved = resolve_series(project.tree(), &series);

        assert!(!resolved.ok());
        assert!(resolved.broken());
        assert_eq!(resolved.missing, vec![MissingRole::Y]);
        assert!(resolved.y.is_none());
        assert!(
            resolved.x.is_none(),
            "the whole series is marked, not just Y"
        );
        assert_eq!(
            resolved.label,
            SeriesLabel::Missing,
            "an unnamed series with no Y column is data, not a sentence"
        );
    }

    /// There is no longer a non-plottable column type (`Text` disappeared in
    /// favor of `Mixed`): an entirely textual column resolves its
    /// series normally, simply without a single numeric point — this
    /// is not a broken reference.
    #[test]
    fn a_fully_textual_column_resolves_with_no_numeric_points() {
        let f = Fixture::new();
        let series = SeriesSpec::new(f.text);
        let resolved = resolve_series(f.project.tree(), &series);
        assert!(resolved.ok(), "a mixed column is never a broken reference");
        assert!(resolved.missing.is_empty());
        let slice = resolved.y.expect("resolved y values");
        assert!(slice.as_slice().iter().all(|value| value.is_nan()));
    }

    /// A **mixed** column is plottable, unlike a text column:
    /// the series resolves, and only the numeric cells carry a
    /// value — the others are `NaN`, which the renderer already discards.
    #[test]
    fn a_mixed_column_resolves_and_only_its_numbers_carry_a_value() {
        use crate::column::MixedCell;

        let mut project = Project::new("Demo").expect("valid project");
        let root = project.root();
        let book = project.add_empty_workbook(root, None).expect("workbook");
        let sheet = project.add_worksheet(book, None).expect("sheet");
        let y = project
            .add_column(
                sheet,
                None,
                Some("Y"),
                Column::from_mixed(&[
                    MixedCell::Number(1.2),
                    MixedCell::Text("n/a".to_string()),
                    MixedCell::Empty,
                    MixedCell::Number(1.5),
                ]),
            )
            .expect("mixed column");

        let series = SeriesSpec::new(y);
        let resolved = resolve_series(project.tree(), &series);

        assert!(resolved.ok(), "a mixed column is not a broken reference");
        assert!(!resolved.broken());
        assert_eq!(resolved.point_count(), 4);
        let values = resolved.y.expect("resolved y values");
        let slice = values.as_slice();
        assert_eq!(slice[0], 1.2);
        assert!(slice[1].is_nan(), "the text cell is not plotted");
        assert!(slice[2].is_nan(), "the empty cell is not plotted");
        assert_eq!(slice[3], 1.5);
    }

    /// A "kinetics" sheet: A, B, C have an x' (0, 10, 20), D does not.
    fn kinetics() -> (Project, NodeId, Vec<NodeId>) {
        let mut project = Project::new("Kinetics").expect("valid project");
        let root = project.root();
        let book = project.add_empty_workbook(root, None).expect("workbook");
        let sheet = project.add_worksheet(book, None).expect("sheet");
        let mut ids = Vec::new();
        for (name, values, x_prime) in [
            ("A", [1.0, 2.0, 3.0], Some(0.0)),
            ("B", [4.0, 5.0, 6.0], Some(10.0)),
            ("C", [7.0, 8.0, 9.0], Some(20.0)),
            ("D", [0.0, 0.0, 0.0], None),
        ] {
            let id = project
                .add_column(sheet, None, Some(name), Column::from_f64(&values))
                .expect("column");
            let mut meta = project
                .tree()
                .node(id)
                .unwrap()
                .as_column()
                .unwrap()
                .meta()
                .clone();
            meta.x_prime = x_prime;
            project.set_column_meta(id, meta).expect("x'");
            ids.push(id);
        }
        (project, sheet, ids)
    }

    /// #166: a "row" series plots, for each column that carries an x',
    /// the cell of its row at the abscissa x'; the column without an x' is not in it.
    #[test]
    fn a_row_series_plots_the_row_against_the_x_prime_of_each_column() {
        let (project, _, ids) = kinetics();
        let series = SeriesSpec {
            row: Some(1),
            ..SeriesSpec::new(ids[0])
        };
        let resolved = resolve_series(project.tree(), &series);
        assert!(resolved.ok());
        assert_eq!(resolved.label, SeriesLabel::Row(1));
        assert_eq!(resolved.point_count(), 3);
        assert_eq!(resolved.x.as_ref().unwrap().as_slice(), &[0.0, 10.0, 20.0]);
        assert_eq!(resolved.y.as_ref().unwrap().as_slice(), &[2.0, 5.0, 8.0]);
    }

    /// A `row` read from a file may be `usize::MAX`: the label must
    /// not overflow (#316).
    #[test]
    fn a_row_series_label_does_not_overflow_on_the_largest_row() {
        let (project, _, ids) = kinetics();
        let series = SeriesSpec {
            row: Some(usize::MAX),
            ..SeriesSpec::new(ids[0])
        };
        assert_eq!(
            series_label(&series, project.tree()),
            SeriesLabel::Row(usize::MAX)
        );
    }

    /// A row beyond a shorter column is `NaN` (the point is not
    /// plotted), and the abscissa of an explicit `x` is ignored.
    #[test]
    fn a_row_series_reads_nan_past_a_short_column_and_ignores_x() {
        let (project, _, ids) = kinetics();
        let series = SeriesSpec {
            row: Some(7),
            x: Some(ids[1]),
            ..SeriesSpec::new(ids[0])
        };
        let resolved = resolve_series(project.tree(), &series);
        assert!(resolved.ok());
        assert_eq!(resolved.x.as_ref().unwrap().as_slice(), &[0.0, 10.0, 20.0]);
        assert!(resolved
            .y
            .as_ref()
            .unwrap()
            .as_slice()
            .iter()
            .all(|v| v.is_nan()));
    }

    /// Without an x' column in the sheet, the series is empty but not broken;
    /// without its anchor column, it is.
    #[test]
    fn a_row_series_is_empty_without_x_prime_and_broken_without_its_anchor() {
        let (mut project, _, ids) = kinetics();
        for id in &ids[..3] {
            let mut meta = project
                .tree()
                .node(*id)
                .unwrap()
                .as_column()
                .unwrap()
                .meta()
                .clone();
            meta.x_prime = None;
            project.set_column_meta(*id, meta).expect("x'");
        }
        let series = SeriesSpec {
            row: Some(0),
            ..SeriesSpec::new(ids[0])
        };
        let resolved = resolve_series(project.tree(), &series);
        assert!(!resolved.broken(), "empty, not broken");
        assert_eq!(resolved.point_count(), 0);

        project.remove(ids[0]).expect("deletion");
        let resolved = resolve_series(project.tree(), &series);
        assert!(resolved.broken());
        assert_eq!(resolved.missing, vec![MissingRole::Y]);
    }

    #[test]
    fn no_x_column_means_the_implicit_row_index() {
        let f = Fixture::new();
        let series = SeriesSpec::new(f.y);
        let resolved = resolve_series(f.project.tree(), &series);
        assert!(resolved.ok());
        assert_eq!(resolved.x.unwrap().as_slice(), &[0.0, 1.0, 2.0]);
    }

    #[test]
    fn a_histogram_series_never_asks_for_x() {
        let f = Fixture::new();
        let series = SeriesSpec {
            plot_type: PlotType::Histogram,
            ..SeriesSpec::new(f.y)
        };
        let resolved = resolve_series(f.project.tree(), &series);
        assert!(resolved.ok());
        assert!(resolved.x.is_none());
    }

    #[test]
    fn mismatched_lengths_truncate_to_the_common_prefix() {
        let mut project = Project::new("Demo").expect("valid project");
        let root = project.root();
        let book = project.add_empty_workbook(root, None).expect("workbook");
        let sheet = project.add_worksheet(book, None).expect("sheet");
        let x = project
            .add_column(
                sheet,
                None,
                Some("X"),
                numeric_column(&[0.0, 1.0, 2.0, 3.0]),
            )
            .expect("column X");
        let y = project
            .add_column(sheet, None, Some("Y"), numeric_column(&[10.0, 20.0]))
            .expect("column Y, shorter");

        let series = SeriesSpec::new_xy(x, y);
        let resolved = resolve_series(project.tree(), &series);

        assert!(resolved.ok());
        assert!(resolved.truncated);
        assert_eq!(resolved.x.unwrap().as_slice(), &[0.0, 1.0]);
        assert_eq!(resolved.y.unwrap().as_slice(), &[10.0, 20.0]);
    }

    #[test]
    fn explicit_name_wins_over_the_column_name() {
        let f = Fixture::new();
        let series = SeriesSpec {
            name: "My series".to_string(),
            ..SeriesSpec::new(f.y)
        };
        assert_eq!(
            series_label(&series, f.project.tree()),
            SeriesLabel::Text("My series".to_string())
        );
    }

    #[test]
    fn empty_name_falls_back_to_the_long_name_then_the_column_name() {
        let f = Fixture::new();
        let series = SeriesSpec::new(f.y);
        assert_eq!(
            series_label(&series, f.project.tree()),
            SeriesLabel::Text("Y".to_string())
        );

        let mut project = f.project;
        project
            .set_column_meta(
                f.y,
                ColumnMeta {
                    long_name: "Voltage".to_string(),
                    ..ColumnMeta::default()
                },
            )
            .expect("valid metadata");
        assert_eq!(
            series_label(&series, project.tree()),
            SeriesLabel::Text("Voltage".to_string())
        );
    }

    #[test]
    fn resolve_spec_walks_every_panel_and_series() {
        let f = Fixture::new();
        let mut project = f.project;
        // A broken reference, for the second series: delete the column
        // it targeted rather than go through a text column, which now
        // always resolves (see `a_fully_textual_column_resolves_with_no_numeric_points`).
        project.remove(f.text).expect("deletion of the text column");
        let spec = PlotSpec {
            panels: vec![
                PanelSpec {
                    series: vec![SeriesSpec::new_xy(f.x, f.y)],
                    ..PanelSpec::at(0, 0)
                },
                PanelSpec {
                    series: vec![SeriesSpec::new(f.text)],
                    ..PanelSpec::at(1, 0)
                },
            ]
            .into(),
            ..PlotSpec::empty("Two panels")
        };

        let resolved = resolve_spec(project.tree(), &spec);

        assert_eq!(resolved.panels.len(), 2);
        assert_eq!(resolved.point_count(), 3);
        assert_eq!(resolved.broken_series().count(), 1);
    }

    #[test]
    fn bar_width_prefers_the_requested_value() {
        assert_eq!(bar_width(Some(&[0.0, 1.0, 2.0]), Some(0.3)), 0.3);
    }

    #[test]
    fn bar_width_without_x_or_a_single_point_defaults_to_one() {
        assert_eq!(bar_width(None, None), 1.0);
        assert_eq!(bar_width(Some(&[1.0]), None), 1.0);
    }

    #[test]
    fn bar_width_deduces_eighty_percent_of_the_median_step() {
        // Constant step of 2 → deduced width 1.6.
        assert_eq!(bar_width(Some(&[0.0, 2.0, 4.0, 6.0]), None), 1.6);
    }

    #[test]
    fn bar_width_ignores_non_finite_and_non_positive_steps() {
        // A duplicate produces a zero step, excluded; NaN too.
        assert_eq!(bar_width(Some(&[0.0, 0.0, 1.0, f64::NAN]), None), 0.8);
    }

    #[test]
    fn histogram_of_an_all_nan_slice_is_the_empty_case() {
        let (edges, counts) = histogram(&[f64::NAN, f64::NAN], NonZeroU32::new(10).unwrap());
        assert_eq!(edges, vec![0.0]);
        assert!(counts.is_empty());
    }

    #[test]
    fn histogram_excludes_non_finite_values() {
        let values = [0.0, 1.0, 2.0, 3.0, f64::NAN, f64::INFINITY];
        let (edges, counts) = histogram(&values, NonZeroU32::new(4).unwrap());
        assert_eq!(edges.len(), 5);
        assert_eq!(edges.first().copied(), Some(0.0));
        assert_eq!(edges.last().copied(), Some(3.0));
        let total: f64 = counts.iter().sum();
        assert_eq!(total, 4.0, "the two non-finite values are excluded");
    }

    #[test]
    fn histogram_of_a_single_repeated_value_widens_the_degenerate_range() {
        let (edges, counts) = histogram(&[5.0, 5.0, 5.0], NonZeroU32::new(2).unwrap());
        assert_eq!(edges, vec![4.5, 5.0, 5.5]);
        // All the values fall into the right bin with this
        // implementation (inclusive right edge): the total count is
        // preserved, which is the property that matters.
        let total: f64 = counts.iter().sum();
        assert_eq!(total, 3.0);
    }

    #[test]
    fn window_meta_default_is_used_for_fixture_windows() {
        // Sanity check of the fixture itself: the graph exists and its window
        // is the default.
        let f = Fixture::new();
        let node = f.project.tree().get(f.graph).expect("graph present");
        assert!(matches!(
            node.kind(),
            crate::node::NodeKind::Graph { meta, .. } if *meta == WindowMeta::default()
        ));
    }
}
