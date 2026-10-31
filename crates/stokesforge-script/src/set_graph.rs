//! `pyst.set_graph`: set an existing graph (#331, batch 2, S4).
//!
//! The keys are those of the Graph Properties window, named like the `plot`
//! options when they exist there too (`title`, `x_title`, `x_scale`,
//! `x_from`/`x_to`...). No new rule: the [`PlotSpec`] is edited then written
//! through `SetPlotSpec`, hence **a single history entry** whatever the number
//! of keys, in the script's undoable group. All keys are read and validated
//! before the first write: an unknown key or a refused value leaves nothing
//! behind. A call that changes nothing writes nothing.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use stokesforge_core::compose::{self, FigureOffset, FrameAxis, FreeField};
use stokesforge_core::NodeId;
use stokesforge_plot::{
    AxisScale, AxisSpec, LayerPlacement, LegendPosition, LengthUnit, PlotSpec, SeriesAxis,
    MAX_FIGURE_EXTENT, MAX_FRAME_SIZE, MIN_FRAME_SIZE,
};
use stokesforge_render_svg::SizeAxis;
use stokesforge_session::Snapshot;

use crate::bridge::{self, HostError};
use crate::engine::{self, ScriptContext};
use crate::graph::{axis_range, invalid, update_axis};
use crate::graph_size::parse_unit;
use crate::surface::{to_py_error, Handle};

/// The keys `set_graph` accepts, in one place: option parsing, the
/// documentation and the surface test all start from this list.
pub(crate) const SET_GRAPH_OPTIONS: &[&str] = &[
    // The graph.
    "title",
    "title_visible",
    "title_color",
    "background",
    "stale_warning",
    "decimal_comma",
    // The targeted panel.
    "panel_title",
    // The X axis (bottom) and the Y axis (left or right depending on `axis`).
    "x_title",
    "y_title",
    "x_unit",
    "y_unit",
    "x_scale",
    "y_scale",
    "x_from",
    "x_to",
    "y_from",
    "y_to",
    "x_auto_range",
    "y_auto_range",
    "x_inverted",
    "y_inverted",
    "x_grid",
    "y_grid",
    "x_visible",
    "y_visible",
    // The targeted panel's legend.
    "legend_visible",
    "legend_position",
    // The size (#399): plot area of the panel's column / row, Left / Top of the
    // first plot area (None = Auto), Figure size = Fixed. In `unit`.
    "plot_width",
    "plot_height",
    "plot_left",
    "plot_top",
    "fixed_size",
    // The total size of the image (#407), in `unit`: the frames follow.
    "figure_width",
    "figure_height",
    // Free layer placement (#411): the placement, and the position of the
    // layer of `panel` in page coordinates (Free only).
    "layer_placement",
    "layer_left",
    "layer_top",
];

/// A length in `unit` at most as precise as this once in points: the
/// comparison against the bounds tolerates the rounding of the conversion.
const LENGTH_TOLERANCE: f64 = 1e-9;

/// `value` rounded up to two decimals (the least a user can type and be
/// accepted at the bottom of a range).
fn ceil_2(value: f64) -> f64 {
    (value * 100.0 - 1e-6).ceil() / 100.0
}

/// `value` rounded down to two decimals.
fn floor_2(value: f64) -> f64 {
    (value * 100.0 + 1e-6).floor() / 100.0
}

/// A number with at most two decimals, no trailing zero: `1.27`, `70.56`, `36`.
fn short_number(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// What is asked to change on an axis; `None` = leave it alone.
#[derive(Default)]
struct AxisChange {
    title: Option<String>,
    unit: Option<String>,
    scale: Option<AxisScale>,
    bounds: (Option<f64>, Option<f64>),
    auto_range: Option<bool>,
    inverted: Option<bool>,
    grid: Option<bool>,
    visible: Option<bool>,
}

impl AxisChange {
    fn apply(&self, name: &str, axis: &mut AxisSpec) -> Result<(), HostError> {
        let range = axis_range(name, self.bounds)?;
        if range.is_some() && self.auto_range == Some(true) {
            return Err(HostError::InvalidValue(format!(
                "{name}_auto_range=True contradicts {name}_from and {name}_to"
            )));
        }
        if let Some(auto) = self.auto_range {
            axis.auto_range = auto;
        }
        if let Some(unit) = &self.unit {
            axis.unit.clone_from(unit);
        }
        if let Some(inverted) = self.inverted {
            axis.inverted = inverted;
        }
        if let Some(grid) = self.grid {
            axis.grid = grid;
        }
        if let Some(visible) = self.visible {
            axis.visible = visible;
        }
        // Title, scale and bounds: the same gesture as `plot`, with its refusal
        // of a non-positive bound on a logarithmic scale.
        update_axis(axis, &self.title, self.scale, range)
    }
}

/// The requested changes: Rust data only, they cross `Python::detach`.
#[derive(Default)]
struct Changes {
    title: Option<String>,
    title_visible: Option<bool>,
    title_color: Option<String>,
    background: Option<String>,
    stale_warning: Option<bool>,
    decimal_comma: Option<bool>,
    panel_title: Option<String>,
    x: AxisChange,
    y: AxisChange,
    legend_visible: Option<bool>,
    legend_position: Option<LegendPosition>,
    /// Sizes in points (already converted and checked). Left / Top: the inner
    /// `None` is Auto.
    plot_width: Option<f64>,
    plot_height: Option<f64>,
    plot_left: Option<Option<f64>>,
    plot_top: Option<Option<f64>>,
    fixed_size: Option<bool>,
    /// The total size of the image in points (#407); checked against the
    /// range the graph allows when applied.
    figure_width: Option<f64>,
    figure_height: Option<f64>,
    /// The unit the call was given in, for the messages.
    unit: LengthUnit,
    /// `true` = free placement, `false` = regular grid (#411).
    free_placement: Option<bool>,
    /// Page coordinates of the layer of `panel` in free placement, in points.
    layer_left: Option<f64>,
    layer_top: Option<f64>,
}

impl Changes {
    /// True if a key needs the plot areas to be stored: a graph from an old
    /// file is given the ones it is drawn with first.
    fn sets_size(&self) -> bool {
        self.plot_width.is_some()
            || self.plot_height.is_some()
            || self.plot_left.is_some()
            || self.plot_top.is_some()
            || self.fixed_size.is_some()
            || self.figure_width.is_some()
            || self.figure_height.is_some()
    }

    /// The placement the graph has once the call is applied.
    fn target_free(&self, before: &PlotSpec) -> bool {
        self.free_placement
            .unwrap_or(before.layer_placement == LayerPlacement::Free)
    }

    fn parse(options: &Bound<'_, PyDict>, unit: LengthUnit) -> PyResult<Self> {
        let mut changes = Self {
            unit,
            ..Self::default()
        };
        for (key, value) in options.iter() {
            let key: String = key.extract()?;
            let string = || {
                value
                    .extract::<String>()
                    .map_err(|_| invalid(format!("{key} must be a string")))
            };
            let boolean = || {
                value
                    .extract::<bool>()
                    .map_err(|_| invalid(format!("{key} must be True or False")))
            };
            // Empty = automatic, otherwise `#RRGGBB` or `#RRGGBBAA`.
            let color = || {
                let text = string()?;
                if !text.is_empty() && stokesforge_plot::parse_hex(&text).is_none() {
                    return Err(invalid(format!(
                        "{key} must be a hexadecimal RGB or RGBA color, or an empty string"
                    )));
                }
                Ok(text)
            };
            let scale = || match string()?.as_str() {
                "linear" => Ok(AxisScale::Linear),
                "log" => Ok(AxisScale::Log),
                _ => Err(invalid("unknown axis scale: use linear or log")),
            };
            let bound = || {
                value
                    .extract::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| invalid(format!("{key} must be a finite number")))
            };
            // A length in `unit`, refused (never brought back) outside
            // `[low, high]` points; the message gives the range in `unit`.
            let length = |low: f64, high: f64| {
                let number = value
                    .extract::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| invalid(format!("{key} must be a finite number")))?;
                let points = unit.to_points(number);
                if points < low - LENGTH_TOLERANCE || points > high + LENGTH_TOLERANCE {
                    return Err(invalid(format!(
                        "{key} must be between {} and {} {}",
                        short_number(unit.from_points(low)),
                        short_number(unit.from_points(high)),
                        unit.symbol()
                    )));
                }
                Ok(points.clamp(low, high))
            };
            // Left / Top: a length from the image edge, or `None` = Auto.
            let offset = || {
                if value.is_none() {
                    Ok(None)
                } else {
                    length(0.0, MAX_FRAME_SIZE).map(Some)
                }
            };
            // A page coordinate: may be negative, refused beyond the bound
            // the stored position is brought into.
            let position = || length(-MAX_FIGURE_EXTENT / 2.0, MAX_FIGURE_EXTENT / 2.0);
            match key.as_str() {
                "layer_placement" => {
                    changes.free_placement = Some(match string()?.as_str() {
                        "grid" => false,
                        "free" => true,
                        _ => return Err(invalid("unknown layer placement: use grid or free")),
                    })
                }
                "layer_left" => changes.layer_left = Some(position()?),
                "layer_top" => changes.layer_top = Some(position()?),
                "plot_width" => changes.plot_width = Some(length(MIN_FRAME_SIZE, MAX_FRAME_SIZE)?),
                "plot_height" => {
                    changes.plot_height = Some(length(MIN_FRAME_SIZE, MAX_FRAME_SIZE)?)
                }
                "figure_width" => changes.figure_width = Some(unit.to_points(bound()?)),
                "figure_height" => changes.figure_height = Some(unit.to_points(bound()?)),
                "plot_left" => changes.plot_left = Some(offset()?),
                "plot_top" => changes.plot_top = Some(offset()?),
                "fixed_size" => changes.fixed_size = Some(boolean()?),
                "title" => changes.title = Some(string()?),
                "title_visible" => changes.title_visible = Some(boolean()?),
                "title_color" => changes.title_color = Some(color()?),
                "background" => changes.background = Some(color()?),
                "stale_warning" => changes.stale_warning = Some(boolean()?),
                "decimal_comma" => changes.decimal_comma = Some(boolean()?),
                "panel_title" => changes.panel_title = Some(string()?),
                "x_title" => changes.x.title = Some(string()?),
                "y_title" => changes.y.title = Some(string()?),
                "x_unit" => changes.x.unit = Some(string()?),
                "y_unit" => changes.y.unit = Some(string()?),
                "x_scale" => changes.x.scale = Some(scale()?),
                "y_scale" => changes.y.scale = Some(scale()?),
                "x_from" => changes.x.bounds.0 = Some(bound()?),
                "x_to" => changes.x.bounds.1 = Some(bound()?),
                "y_from" => changes.y.bounds.0 = Some(bound()?),
                "y_to" => changes.y.bounds.1 = Some(bound()?),
                "x_auto_range" => changes.x.auto_range = Some(boolean()?),
                "y_auto_range" => changes.y.auto_range = Some(boolean()?),
                "x_inverted" => changes.x.inverted = Some(boolean()?),
                "y_inverted" => changes.y.inverted = Some(boolean()?),
                "x_grid" => changes.x.grid = Some(boolean()?),
                "y_grid" => changes.y.grid = Some(boolean()?),
                "x_visible" => changes.x.visible = Some(boolean()?),
                "y_visible" => changes.y.visible = Some(boolean()?),
                "legend_visible" => changes.legend_visible = Some(boolean()?),
                "legend_position" => {
                    changes.legend_position = Some(match string()?.as_str() {
                        "top_left" => LegendPosition::TopLeft,
                        "top_right" => LegendPosition::TopRight,
                        "bottom_left" => LegendPosition::BottomLeft,
                        "bottom_right" => LegendPosition::BottomRight,
                        _ => {
                            return Err(invalid(
                                "unknown legend position: use top_left, top_right, bottom_left or bottom_right",
                            ))
                        }
                    })
                }
                _ => {
                    return Err(invalid(format!(
                        "unknown graph option: {key} (accepted: {})",
                        SET_GRAPH_OPTIONS.join(", ")
                    )))
                }
            }
        }
        if (changes.figure_width.is_some() || changes.figure_height.is_some())
            && (changes.plot_width.is_some() || changes.plot_height.is_some())
        {
            return Err(invalid(
                "figure_width and figure_height cannot be combined with plot_width or plot_height: give the size of the image or the size of the plot area",
            ));
        }
        Ok(changes)
    }

    /// Fits the total size of the image along `axis`, the frames following in
    /// proportion (#407). A value out of the range the graph allows is refused
    /// with that range, never brought back.
    fn fit_figure(
        &self,
        spec: &mut PlotSpec,
        key: &str,
        axis: SizeAxis,
        points: f64,
    ) -> Result<(), HostError> {
        let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
        let choice = stokesforge_render_svg::FontChoice::default();
        let fitted =
            stokesforge_render_svg::fit_figure_size(spec, &measurer, &choice, axis, points)
                .ok()
                .flatten()
                .ok_or_else(|| {
                    HostError::InvalidValue(format!(
                        "{key} cannot be set: the graph has no plot area to scale"
                    ))
                })?;
        let (low, high) = fitted.range;
        if points < low - LENGTH_TOLERANCE || points > high + LENGTH_TOLERANCE {
            return Err(HostError::InvalidValue(format!(
                "{key} must be between {} and {} {}",
                short_number(ceil_2(self.unit.from_points(low))),
                short_number(floor_2(self.unit.from_points(high))),
                self.unit.symbol()
            )));
        }
        fitted.apply_to(spec);
        Ok(())
    }

    /// Applies the changes to the spec; `layer` is the rank of the targeted
    /// panel, `cell` its `(row, column)`.
    fn apply(
        &self,
        spec: &mut PlotSpec,
        layer: usize,
        cell: (usize, usize),
        side: SeriesAxis,
    ) -> Result<(), HostError> {
        // The placement first (#411): the other keys act on the result. `layer`
        // stays the rank of the panel; its grid identity is read again.
        let free = self.target_free(spec);
        if free != (spec.layer_placement == LayerPlacement::Free) {
            let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
            let choice = stokesforge_render_svg::FontChoice::default();
            if !stokesforge_render_svg::set_layer_placement(spec, free, &measurer, &choice) {
                return Err(HostError::InvalidValue(
                    "the layer placement cannot be switched: the layers do not fit a grid"
                        .to_owned(),
                ));
            }
        }
        let cell = spec
            .panels
            .get(layer)
            .map_or(cell, |panel| (panel.row, panel.column));
        if free && (self.plot_left.is_some() || self.plot_top.is_some()) {
            return Err(HostError::InvalidValue(
                "plot_left and plot_top do not apply to free layer placement: use layer_left and layer_top"
                    .to_owned(),
            ));
        }
        if !free && (self.layer_left.is_some() || self.layer_top.is_some()) {
            return Err(HostError::InvalidValue(
                "layer_left and layer_top need free layer placement: the graph uses the regular grid"
                    .to_owned(),
            ));
        }
        if let Some(title) = &self.title {
            spec.title.clone_from(title);
        }
        if let Some(visible) = self.title_visible {
            spec.title_visible = visible;
        }
        if let Some(color) = &self.title_color {
            spec.title_color.clone_from(color);
        }
        if let Some(background) = &self.background {
            spec.background.clone_from(background);
        }
        if let Some(stale) = self.stale_warning {
            spec.stale_warning = stale;
        }
        if let Some(comma) = self.decimal_comma {
            spec.decimal_comma = comma;
        }
        let panel = spec
            .panels
            .get_mut(layer)
            .ok_or_else(|| HostError::InvalidValue("the graph has no such panel".to_owned()))?;
        if let Some(title) = &self.panel_title {
            panel.title.clone_from(title);
        }
        self.x.apply("x", &mut panel.bottom)?;
        self.y.apply("y", panel.axis_mut(side.side()))?;
        if let Some(visible) = self.legend_visible {
            panel.legend.visible = visible;
        }
        if let Some(position) = self.legend_position {
            panel.legend.position = position;
        }
        // The size, through the setters of the interface (#399). They only
        // refuse a graph that stores no plot area and could not be given any
        // (more cells than the layout stores): never a silent no-op.
        let stored = |done: bool| {
            if done {
                Ok(())
            } else {
                Err(HostError::InvalidValue(
                    "the plot areas of this graph cannot be set: it has too many cells".to_owned(),
                ))
            }
        };
        if free {
            for (field, value) in [
                (FreeField::Width, self.plot_width),
                (FreeField::Height, self.plot_height),
                (FreeField::Left, self.layer_left),
                (FreeField::Top, self.layer_top),
            ] {
                if let Some(value) = value {
                    stored(compose::set_free_frame(spec, layer, field, value))?;
                }
            }
        } else if let Some(width) = self.plot_width {
            stored(compose::set_frame_size(
                spec,
                FrameAxis::Column,
                cell.1,
                width,
            ))?;
        }
        if let (false, Some(height)) = (free, self.plot_height) {
            stored(compose::set_frame_size(
                spec,
                FrameAxis::Row,
                cell.0,
                height,
            ))?;
        }
        if let Some(left) = self.plot_left {
            stored(compose::set_figure_offset(spec, FigureOffset::Left, left))?;
        }
        if let Some(top) = self.plot_top {
            stored(compose::set_figure_offset(spec, FigureOffset::Top, top))?;
        }
        if let Some(fixed) = self.fixed_size {
            if free {
                // A free graph stores no grid frame to test: the flag alone.
                spec.fixed_figure = fixed;
            } else {
                stored(compose::set_fixed_figure(spec, fixed))?;
            }
        }
        // The total size last (#407): the bands it keeps depend on the title,
        // the axes, Fixed, Left / Top and the placement set above. Width
        // first: the height is measured at the new width.
        if let Some(width) = self.figure_width {
            self.fit_figure(spec, "figure_width", SizeAxis::Width, width)?;
        }
        if let Some(height) = self.figure_height {
            self.fit_figure(spec, "figure_height", SizeAxis::Height, height)?;
        }
        Ok(())
    }
}

fn set_graph_native(
    context: &ScriptContext,
    graph: NodeId,
    epoch: u64,
    panel: (usize, usize),
    side: SeriesAxis,
    changes: &Changes,
) -> Result<(), HostError> {
    let prepare = |snapshot: &Snapshot| {
        let before = bridge::graph_spec(snapshot, graph, epoch)?;
        let layer = bridge::base_layer_at(&before, panel.0, panel.1).ok_or_else(|| {
            HostError::InvalidValue(format!(
                "the graph has no panel at row {}, column {}",
                panel.0, panel.1
            ))
        })?;
        let mut spec = before.clone();
        let switching =
            changes.target_free(&before) != (before.layer_placement == LayerPlacement::Free);
        // A size key on a graph from an old file: it is given the plot areas
        // it is drawn with first (G7), and a call that then changes nothing
        // writes nothing, the graph staying as it was. A switch to free fixes
        // them itself; a free graph is never legacy.
        if changes.sets_size() && !switching && before.layer_placement == LayerPlacement::Grid {
            bridge::fix_frames(Some(&before), &mut spec);
        }
        let reference = if switching {
            before.clone()
        } else {
            spec.clone()
        };
        changes.apply(&mut spec, layer, panel, side)?;
        let changed = spec != reference;
        Ok::<_, HostError>((spec, changed))
    };
    let session = context.session();
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    let (mut spec, mut changed) = prepare(&snapshot)?;
    if !changed {
        return Ok(());
    }
    if context.begin_write()? {
        // Same validation under the script group's exclusive reservation.
        let snapshot = session.snapshot().map_err(HostError::from_session)?;
        (spec, changed) = prepare(&snapshot)?;
    }
    if changed {
        bridge::set_graph_spec(session, graph, spec)?;
    }
    Ok(())
}

/// Sets an existing graph through the keys of the Graph Properties window; a
/// single history entry (`pyst.undo()` undoes it). Keys not given change
/// nothing.
///
/// The graph: `title`, `title_visible`, `title_color` (`'#RRGGBB'`, `''` =
/// automatic), `background` (`'#RRGGBB[AA]'`, `''` = white), `stale_warning`,
/// `decimal_comma` (booleans). The panel `panel=(row, column)`: `panel_title`,
/// `legend_visible`, `legend_position` (`'top_left'`, `'top_right'`,
/// `'bottom_left'`, `'bottom_right'`). The X axis (bottom) and the Y axis
/// (`axis='left'` or `'right'`): `x_title`/`y_title`, `x_unit`/`y_unit`,
/// `x_scale`/`y_scale` (`'linear'`, `'log'`), `x_from` and `x_to` (given
/// together and different; `from > to` draws the axis reversed) and
/// `y_from`/`y_to`, `x_auto_range`/`y_auto_range` (turns view framing back to
/// automatic), `x_inverted`/`y_inverted`, `x_grid`/`y_grid`,
/// `x_visible`/`y_visible`. Titles: Typst markup, as in `plot`.
///
/// The size, in `unit` (`'cm'` by default, `'mm'`, `'pt'` or `'in'`; never the
/// unit of the Preferences): `plot_width` (plot area of the column of `panel`)
/// and `plot_height` (of its row), each between 1.27 cm and 70.56 cm (36 to
/// 2000 pt), `plot_left` and `plot_top` (distance from the image edge to the
/// first plot area, `None` = Auto, 0 to 70.56 cm; a value below what the axes
/// need is kept and drawn raised to it) and `fixed_size` (Figure size =
/// Fixed). The total size of the image is set with `figure_width` and
/// `figure_height` (#407): the frames follow in proportion, axes, margins and
/// titles keeping their size (in free placement, positions and frames scale
/// about the top-left layer). They are applied last, after everything else in
/// the call, and cannot be combined with `plot_width` / `plot_height`. The
/// range depends on the graph (smallest frame 1.27 cm, largest 70.56 cm, the
/// bands around them); a value out of it is refused with the range. The image
/// is whole points, so the size lands within half a point (read it with
/// `pyst.graph_size`). A value out of range is refused, never brought back. A
/// graph from an old file is given the plot areas it is drawn with in the same
/// history entry.
///
/// Layer placement (#411): `layer_placement='grid'` or `'free'` switches the
/// graph like Graph Properties does, first in the call (switching to free moves
/// nothing; back to the grid, layers are put in rows then columns and each
/// column and row takes its largest frame). `panel=(row, column)` is read in the
/// placement the graph has when the call starts. In free placement
/// `plot_width` and `plot_height` set the frame of `panel` alone, `layer_left`
/// and `layer_top` its position in stable page coordinates (in `unit`, may be
/// negative, at most 352.78 cm either side of the origin), and `plot_left` /
/// `plot_top` are refused; on the regular grid `layer_left` / `layer_top` are
/// refused.
///
/// An unknown key or unit, a value of the wrong type or refused bounds raise
/// `InvalidValueError` without writing anything; `BrokenReferenceError` for a
/// stale handle.
#[pyfunction]
#[pyo3(signature = (graph, *, panel=(0, 0), axis="left", unit="cm", **options))]
pub(crate) fn set_graph(
    py: Python<'_>,
    graph: &Handle,
    panel: (usize, usize),
    axis: &str,
    unit: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let side = match axis {
        "left" => SeriesAxis::Left,
        "right" => SeriesAxis::Right,
        _ => return Err(invalid("unknown axis: use left or right")),
    };
    let unit = parse_unit(unit)?;
    let changes = match options {
        Some(options) => Changes::parse(options, unit)?,
        None => Changes::default(),
    };
    let (id, epoch) = (graph.id, graph.epoch);
    py.detach(|| set_graph_native(&context, id, epoch, panel, side, &changes))
        .map_err(to_py_error)
}
