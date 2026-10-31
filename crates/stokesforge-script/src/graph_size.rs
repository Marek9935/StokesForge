//! `pyst.graph_size`: reads the size of a graph (#399).
//!
//! One snapshot, the layout the screen and the exports read
//! (`stokesforge_render_svg::figure_layout`, same measurer and font choice as
//! `bridge::fix_frames`, invariant 4). It never writes and never opens the
//! script's undoable group: a legacy graph (nothing stored) is read as it is
//! drawn and stays legacy. All the lengths are in the `unit` asked for
//! (centimetres by default, never the Preferences unit); the stored sizes are
//! points. The total image size (`width`, `height`) is read-only.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use stokesforge_plot::{LayerPlacement, LengthUnit};

use crate::bridge::{self, HostError};
use crate::engine;
use crate::graph::invalid;
use crate::surface::{to_py_error, Handle};

/// The `unit=` keyword of `graph_size` and `set_graph`.
pub(crate) fn parse_unit(unit: &str) -> PyResult<LengthUnit> {
    LengthUnit::from_symbol(unit).ok_or_else(|| invalid("unknown unit: use cm, mm, pt or in"))
}

/// What the layout says, in points; Rust data only (it crosses
/// `Python::detach`).
struct Sizes {
    size: (f64, f64),
    widths: Vec<f64>,
    heights: Vec<f64>,
    left: Option<f64>,
    top: Option<f64>,
    fixed: bool,
    /// Free placement (#411): the stored frame of each cell, in stacking order.
    layers: Option<Vec<Layer>>,
}

/// One cell of a free graph: its `(row, column)` and stored frame, in points.
struct Layer {
    panel: (usize, usize),
    frame: [f64; 4],
}

/// Reads the size of a graph: its total image size, the plot areas of its grid
/// columns and rows, the Left / Top of the first plot area and whether the
/// figure size is Fixed.
///
/// Returns a `dict` in `unit` (`'cm'` by default, `'mm'`, `'pt'` or `'in'`):
/// `width` and `height` (the image, read-only: it follows the plot areas and
/// the axes, titles and legend around them), `plot_widths` (one per grid
/// column) and `plot_heights` (one per grid row), `plot_left` and `plot_top`
/// (`None` = Auto, otherwise the distance from the image edge, raised to the
/// smallest value the axes allow) and `fixed_size` (`bool`). A graph from an
/// old file, which stores no plot area, reports the ones it is drawn with and
/// stays as it is. `layer_placement` is `'grid'` or `'free'` (#411). A graph
/// with free layer placement (#398) reports its image size, empty
/// `plot_widths` and `plot_heights`, `None` for `plot_left` and `plot_top`, and
/// `layers` (`[]` on the grid): one `dict` per cell in stacking order, `panel` (`(row, column)`),
/// `left`, `top` (stable page coordinates as stored, may be negative), `width`
/// and `height`.
///
/// Raises `InvalidValueError` for an unknown unit, `BrokenReferenceError` for
/// a stale handle.
#[pyfunction]
#[pyo3(signature = (graph, *, unit="cm"))]
pub(crate) fn graph_size<'py>(
    py: Python<'py>,
    graph: &Handle,
    unit: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let unit = parse_unit(unit)?;
    let (id, epoch) = (graph.id, graph.epoch);
    let sizes = py
        .detach(|| -> Result<Sizes, HostError> {
            let snapshot = context
                .session()
                .snapshot()
                .map_err(HostError::from_session)?;
            let spec = bridge::graph_spec(&snapshot, id, epoch)?;
            let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
            let choice = stokesforge_render_svg::FontChoice::default();
            let layout = stokesforge_render_svg::figure_layout(&spec, &measurer, &choice)
                .map_err(|error| HostError::InvalidValue(error.to_string()))?;
            // A free graph (#398) has no grid column or row, and the figure's
            // Left / Top are not used: only the image size is reported.
            let free = spec.layer_placement == LayerPlacement::Free;
            Ok(Sizes {
                size: (f64::from(layout.size.0), f64::from(layout.size.1)),
                widths: layout.frame_widths.clone(),
                heights: layout.frame_heights.clone(),
                left: spec
                    .figure_left
                    .filter(|_| !free)
                    .map(|left| left.max(f64::from(layout.min_left))),
                top: spec
                    .figure_top
                    .filter(|_| !free)
                    .map(|top| top.max(f64::from(layout.min_top))),
                fixed: spec.fixed_figure,
                layers: free.then(|| {
                    let bases = spec.cell_bases();
                    spec.panels
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| bases.get(*index) == Some(index))
                        .map(|(_, panel)| {
                            let frame = panel.sanitized_free_frame();
                            Layer {
                                panel: (panel.row, panel.column),
                                frame: [frame.left, frame.top, frame.width, frame.height],
                            }
                        })
                        .collect()
                }),
            })
        })
        .map_err(to_py_error)?;
    let convert = |points: f64| unit.from_points(points);
    let all = |points: &[f64]| points.iter().copied().map(convert).collect::<Vec<_>>();
    let result = PyDict::new(py);
    result.set_item("width", convert(sizes.size.0))?;
    result.set_item("height", convert(sizes.size.1))?;
    result.set_item("plot_widths", all(&sizes.widths))?;
    result.set_item("plot_heights", all(&sizes.heights))?;
    result.set_item("plot_left", sizes.left.map(convert))?;
    result.set_item("plot_top", sizes.top.map(convert))?;
    result.set_item("fixed_size", sizes.fixed)?;
    result.set_item(
        "layer_placement",
        if sizes.layers.is_some() {
            "free"
        } else {
            "grid"
        },
    )?;
    {
        // Always present, empty on the regular grid.
        let list = PyList::empty(py);
        for layer in sizes.layers.iter().flatten() {
            let item = PyDict::new(py);
            item.set_item("panel", layer.panel)?;
            for (name, points) in ["left", "top", "width", "height"]
                .into_iter()
                .zip(layer.frame)
            {
                item.set_item(name, convert(points))?;
            }
            list.append(item)?;
        }
        result.set_item("layers", list)?;
    }
    Ok(result)
}
