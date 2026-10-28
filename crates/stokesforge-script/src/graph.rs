//! Native plotting from pyst: explicit option conversion, then commands.
//!
//! No engine here: the script produces the same PlotSpec as the interface.
//! Empty series names stay automatic: the shared rendering reads them as
//! literal text, without interpreting a column's markup. Titles and labels
//! explicitly given, on the other hand, are Typst markup.
//! Without a `title` option, a new graph stays untitled (#103); adding a
//! series to an existing graph keeps its title.
//! The first panel is extended without rewriting the others or its settings.
//! Several `y` columns at once (#196) form a group, like the series added
//! together from the interface (`push_series_batch`).

use pyo3::prelude::*;
use pyo3::types::PyDict;
use stokesforge_commands::Command;
use stokesforge_core::{compose, NodeId, NodeKind};
use stokesforge_plot::{
    AxisScale, AxisSpec, BinCount, LineStyle, PlotSpec, PlotType, SeriesAxis, SeriesSpec,
    SymbolShape,
};
use stokesforge_session::Snapshot;

use crate::bridge::{self, HostError};
use crate::engine::{self, ScriptContext};
use crate::surface::{to_py_error, Handle, InvalidValueError};

type Target = (NodeId, u64);

/// Rust data only: it crosses Python::detach, never a Python object.
/// The `color=` option: a string (`scalar`, valid for all the series of the
/// batch) or a list with one color per series.
struct ColorOption {
    list: Vec<String>,
    scalar: bool,
}

struct Request {
    references: Vec<Target>,
    /// One series per entry: y column, own (or shared) x column and name.
    members: Vec<Member>,
    group_name: Option<String>,
    graph: Option<Target>,
    parent: Option<Target>,
    name: Option<String>,
    series: SeriesSpec,
    /// One color per series (list) or one for all (string), #207.
    color: Option<ColorOption>,
    title: Option<String>,
    x_title: Option<String>,
    y_title: Option<String>,
    x_scale: Option<AxisScale>,
    y_scale: Option<AxisScale>,
    x_bounds: (Option<f64>, Option<f64>),
    y_bounds: (Option<f64>, Option<f64>),
}

/// The columns of one series of the batch; empty `label` = automatic name.
struct Member {
    y: NodeId,
    x: Option<NodeId>,
    label: String,
}

pub(crate) fn invalid(message: impl Into<String>) -> PyErr {
    InvalidValueError::new_err(message.into())
}

/// The options `plot` accepts (`**options`), in one place: the error message
/// for an unknown option and the surface test (`tests/frozen_api.rs`, #331)
/// start from this list. An option added to `plot`'s `match` goes in here
/// too, and in "Frozen API V1" (`docs/python-et-script.md`).
pub(crate) const PLOT_OPTIONS: &[&str] = &[
    "plot_type",
    "color",
    "line_style",
    "line_width",
    "symbol",
    "symbol_size",
    "label",
    "group_name",
    "title",
    "x_title",
    "y_title",
    "x_scale",
    "y_scale",
    "x_from",
    "x_to",
    "y_from",
    "y_to",
    "axis",
    "x_error",
    "y_error",
    "histogram_bins",
];

/// Creates a native graph and returns its handle; graph= adds to the first panel.
/// y is a column, or a list of at least two columns that form a series group
/// (#196); without x, each y takes the nearest X-role column to its left in
/// its sheet (X2 for a Y2-role y), like the window, and the row index if there
/// is none (#257); x can be a shared column or a list of the same length as y.
/// With several series, label is a list (one name per series) and group_name
/// names the group; x_error/y_error serve a single series only. Options:
/// plot_type (line, scatter, line_scatter, bar, histogram, error_bar), color
/// (#RRGGBB or #RRGGBBAA; a list gives one color per series, a string applies
/// to all), line_style (none, solid, dash, dot, dash_dot), line_width,
/// symbol (none, circle, square, triangle, diamond, plus, cross), symbol_size,
/// label, title, x_title, y_title, x_scale/y_scale (linear, log), x_from/x_to and
/// y_from/y_to (both together; from > to draws the axis reversed), axis (left,
/// right), x_error/y_error (handles), histogram_bins.
/// Explicit titles and label: Typst markup. No label: the column's automatic
/// name, shown literally. One run = one undoable group.
#[pyfunction]
#[pyo3(signature = (y, x=None, *, graph=None, name=None, parent=None, **options))]
pub(crate) fn plot(
    py: Python<'_>,
    y: &Bound<'_, PyAny>,
    x: Option<&Bound<'_, PyAny>>,
    graph: Option<&Handle>,
    name: Option<String>,
    parent: Option<&Handle>,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<Handle> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    if graph.is_some() && (name.is_some() || parent.is_some()) {
        return Err(invalid("name and parent apply only when creating a graph"));
    }
    let target = |h: &Handle| (h.id, h.epoch);
    let ys = column_handles(y, "y")?;
    let xs = match x {
        None => vec![None; ys.len()],
        Some(x) => {
            let xs = column_handles(x, "x")?;
            match xs.len() {
                1 => vec![Some(xs[0]); ys.len()],
                n if n == ys.len() => xs.into_iter().map(Some).collect(),
                _ => return Err(invalid("x must be one column or one column per y")),
            }
        }
    };
    let mut references: Vec<Target> = ys.clone();
    references.extend(xs.iter().flatten().copied());
    let mut labels: Option<Vec<String>> = None;
    let mut request = Request {
        references,
        members: ys
            .iter()
            .zip(&xs)
            .map(|(y, x)| Member {
                y: y.0,
                x: x.map(|x| x.0),
                label: String::new(),
            })
            .collect(),
        group_name: None,
        graph: graph.map(target),
        parent: parent.map(target),
        name,
        series: SeriesSpec::new(ys[0].0),
        color: None,
        title: None,
        x_title: None,
        y_title: None,
        x_scale: None,
        y_scale: None,
        x_bounds: (None, None),
        y_bounds: (None, None),
    };
    if let Some(options) = options {
        for (key, value) in options.iter() {
            let key: String = key.extract()?;
            let string = || {
                value
                    .extract::<String>()
                    .map_err(|_| invalid(format!("{key} must be a string")))
            };
            let positive = || {
                let n = value
                    .extract::<f64>()
                    .map_err(|_| invalid(format!("{key} must be a number")))?;
                if !n.is_finite() || n <= 0.0 {
                    return Err(invalid(format!("{key} must be finite and positive")));
                }
                Ok(n)
            };
            match key.as_str() {
                "plot_type" => request.series.plot_type = match string()?.as_str() {
                    "line" => PlotType::Line, "scatter" => PlotType::Scatter,
                    "line_scatter" => PlotType::LineScatter, "bar" => PlotType::Bar,
                    "histogram" => PlotType::Histogram, "error_bar" => PlotType::ErrorBar,
                    _ => return Err(invalid("unknown plot_type: use line, scatter, line_scatter, bar, histogram or error_bar")),
                },
                "color" => {
                    let (list, scalar) = match value.extract::<Vec<String>>() {
                        Ok(list) => (list, false),
                        Err(_) => (vec![string()?], true),
                    };
                    if list.iter().any(|c| stokesforge_plot::parse_hex(c).is_none()) { return Err(invalid("color must be a hexadecimal RGB or RGBA color, or a list of them")); }
                    request.color = Some(ColorOption { list, scalar });
                }
                "line_style" => request.series.line.style = parse_line_style(&string()?)?,
                "line_width" => request.series.line.width = positive()?,
                "symbol_size" => request.series.symbol.size = positive()?,
                "symbol" => request.series.symbol.shape = parse_symbol(&string()?)?,
                "label" => labels = Some(match value.extract::<Vec<String>>() {
                    Ok(list) => list,
                    Err(_) => vec![string()?],
                }),
                "group_name" => request.group_name = Some(string()?),
                "title" => request.title = Some(string()?),
                "x_title" => request.x_title = Some(string()?),
                "y_title" => request.y_title = Some(string()?),
                "x_scale" | "y_scale" => {
                    let scale = match string()?.as_str() {
                        "linear" => AxisScale::Linear, "log" => AxisScale::Log,
                        _ => return Err(invalid("unknown axis scale: use linear or log")),
                    };
                    if key == "x_scale" { request.x_scale = Some(scale); } else { request.y_scale = Some(scale); }
                }
                "x_from" | "x_to" | "y_from" | "y_to" => {
                    let bound = value.extract::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| invalid(format!("{key} must be a finite number")))?;
                    let bounds = if key.starts_with('x') { &mut request.x_bounds } else { &mut request.y_bounds };
                    if key.ends_with("from") { bounds.0 = Some(bound); } else { bounds.1 = Some(bound); }
                }
                "axis" => request.series.axis = match string()?.as_str() {
                    "left" => SeriesAxis::Left, "right" => SeriesAxis::Right,
                    _ => return Err(invalid("unknown series axis: use left or right")),
                },
                "x_error" | "y_error" => {
                    let handle = value.extract::<PyRef<'_, Handle>>().map_err(|_| invalid(format!("{key} must be a column handle")))?;
                    if request.members.len() > 1 {
                        return Err(invalid(format!("{key} applies to a single series")));
                    }
                    request.references.push(target(&handle));
                    if key == "x_error" { request.series.x_error = Some(handle.id); } else { request.series.y_error = Some(handle.id); }
                }
                "histogram_bins" => {
                    let count = value.extract::<u32>().map_err(|_| invalid("histogram_bins must be a positive integer"))?;
                    request.series.histogram_bins = BinCount::new(count).ok_or_else(|| invalid(format!("histogram_bins must be between 1 and {}", stokesforge_plot::MAX_HISTOGRAM_BINS)))?;
                }
                _ => {
                    return Err(invalid(format!(
                        "unknown plot option: {key} (accepted: {})",
                        PLOT_OPTIONS.join(", ")
                    )))
                }
            }
        }
    }
    if let Some(labels) = labels {
        if labels.len() != request.members.len() {
            return Err(invalid(
                "label must be one string, or a list with one name per series",
            ));
        }
        for (member, label) in request.members.iter_mut().zip(labels) {
            member.label = label;
        }
    }
    if let Some(color) = &request.color {
        if !color.scalar && color.list.len() != request.members.len() {
            return Err(invalid(
                "color must be one color, or a list with one color per series",
            ));
        }
    }
    if request.group_name.is_some() && request.members.len() < 2 {
        return Err(invalid("group_name needs several series"));
    }
    let (id, epoch) = py
        .detach(|| plot_native(&context, request))
        .map_err(to_py_error)?;
    Ok(Handle { id, epoch })
}

pub(crate) fn parse_line_style(name: &str) -> PyResult<LineStyle> {
    Ok(match name {
        "none" => LineStyle::None,
        "solid" => LineStyle::Solid,
        "dash" => LineStyle::Dash,
        "dot" => LineStyle::Dot,
        "dash_dot" => LineStyle::DashDot,
        _ => {
            return Err(invalid(
                "unknown line_style: use none, solid, dash, dot or dash_dot",
            ))
        }
    })
}

pub(crate) fn parse_symbol(name: &str) -> PyResult<SymbolShape> {
    Ok(match name {
        "none" => SymbolShape::None,
        "circle" => SymbolShape::Circle,
        "square" => SymbolShape::Square,
        "triangle" => SymbolShape::Triangle,
        "diamond" => SymbolShape::Diamond,
        "plus" => SymbolShape::Plus,
        "cross" => SymbolShape::Cross,
        _ => {
            return Err(invalid(
                "unknown symbol: use none, circle, square, triangle, diamond, plus or cross",
            ))
        }
    })
}

/// A column handle, or a list of handles.
fn column_handles(value: &Bound<'_, PyAny>, what: &str) -> PyResult<Vec<Target>> {
    if let Ok(handle) = value.extract::<PyRef<'_, Handle>>() {
        return Ok(vec![(handle.id, handle.epoch)]);
    }
    let list = value
        .extract::<Vec<PyRef<'_, Handle>>>()
        .map_err(|_| invalid(format!("{what} must be a column handle or a list of them")))?;
    if list.is_empty() {
        return Err(invalid(format!("{what} must not be an empty list")));
    }
    Ok(list.iter().map(|h| (h.id, h.epoch)).collect())
}

/// The `from ... to` bounds of an axis: both or neither, different; `from >
/// to` is allowed and draws the axis reversed.
pub(crate) fn axis_range(
    axis: &str,
    (from, to): (Option<f64>, Option<f64>),
) -> Result<Option<(f64, f64)>, HostError> {
    match (from, to) {
        (None, None) => Ok(None),
        (Some(from), Some(to)) if from != to => Ok(Some((from, to))),
        (Some(_), Some(_)) => Err(HostError::InvalidValue(format!(
            "{axis}_from and {axis}_to must differ"
        ))),
        _ => Err(HostError::InvalidValue(format!(
            "{axis}_from and {axis}_to must be given together"
        ))),
    }
}

pub(crate) fn update_axis(
    axis: &mut AxisSpec,
    title: &Option<String>,
    scale: Option<AxisScale>,
    range: Option<(f64, f64)>,
) -> Result<(), HostError> {
    if let Some(title) = title {
        axis.label.clone_from(title);
    }
    if let Some(scale) = scale {
        axis.scale = scale;
    }
    if let Some((min, max)) = range {
        axis.auto_range = false;
        axis.range_min = Some(min);
        axis.range_max = Some(max);
    }
    if axis.scale == AxisScale::Log
        && axis.has_explicit_range()
        && axis
            .range_min
            .zip(axis.range_max)
            .is_some_and(|(a, b)| a.min(b) <= 0.0)
    {
        return Err(HostError::InvalidValue(
            "logarithmic axis bounds must be positive".into(),
        ));
    }
    // The direction is read from the order of the bounds (#344): `x_inverted=True` on
    // (1, 5) gives (5, 1), `x_from=5, x_to=1` gives a reversed axis. The `pyst`
    // surface (`x_inverted`, `x_auto_range`) does not change.
    *axis = axis.orient();
    Ok(())
}

/// Builds from one snapshot, both BEFORE and AFTER the exclusive reservation.
fn build_spec(
    snapshot: &Snapshot,
    request: &Request,
    parent: Target,
) -> Result<PlotSpec, HostError> {
    for &(id, epoch) in &request.references {
        let node = bridge::checked_node(snapshot, id, epoch)?;
        if node.as_column().is_none() {
            return Err(HostError::InvalidValue(
                "plot expects column handles".into(),
            ));
        }
    }
    let mut spec = if let Some((id, epoch)) = request.graph {
        let node = bridge::checked_node(snapshot, id, epoch)?;
        match node.kind() {
            NodeKind::Graph { spec, .. } => spec.clone(),
            _ => {
                return Err(HostError::InvalidValue(
                    "graph must be a graph handle".into(),
                ))
            }
        }
    } else {
        let node = bridge::checked_node(snapshot, parent.0, parent.1)?;
        if !node.kind().is_folder() {
            return Err(HostError::InvalidValue(
                "parent must be a folder handle".into(),
            ));
        }
        PlotSpec::default()
    };
    if let Some(title) = &request.title {
        spec.title.clone_from(title);
    }
    let panel = spec
        .panels
        .get_mut(0)
        .ok_or_else(|| HostError::InvalidValue("graph has no panel".into()))?;
    let batch: Vec<SeriesSpec> = request
        .members
        .iter()
        .enumerate()
        .map(|(rank, member)| {
            let mut series = request.series.clone();
            series.y = member.y;
            // An explicit `x=` wins; otherwise the nearest X column to the
            // left by role, like the window, and the row number if the
            // sheet has none (`compose::x_for`, #257).
            series.x = member
                .x
                .or_else(|| compose::x_for(&snapshot.tree, member.y));
            series.name.clone_from(&member.label);
            // Without `color=`, the palette color of the series' rank in the
            // panel (`SeriesSpec::from_palette`, line and markers).
            let palette = SeriesSpec::from_palette(member.y, panel.series.len() + rank);
            let chosen =
                request
                    .color
                    .as_ref()
                    .map(|c| if c.scalar { &c.list[0] } else { &c.list[rank] });
            match chosen {
                Some(color) => {
                    series.line.color.clone_from(color);
                    series.symbol.color.clone_from(color);
                }
                None => {
                    series.line.color = palette.line.color;
                    series.symbol.color = palette.symbol.color;
                }
            }
            series
        })
        .collect();
    update_axis(
        &mut panel.bottom,
        &request.x_title,
        request.x_scale,
        axis_range("x", request.x_bounds)?,
    )?;
    let axis = request.series.axis;
    let y_axis = panel.axis_mut(axis.side());
    update_axis(
        y_axis,
        &request.y_title,
        request.y_scale,
        axis_range("y", request.y_bounds)?,
    )?;
    if axis == SeriesAxis::Right {
        y_axis.visible = true;
    }
    let first = panel.series.len();
    panel.push_series_batch(batch);
    if let Some(name) = &request.group_name {
        if let Some(group) = panel.series[first].group {
            panel.rename_group(group, name);
        }
    }
    Ok(spec)
}

fn plot_native(context: &ScriptContext, request: Request) -> Result<Target, HostError> {
    let session = context.session();
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    let parent = request
        .parent
        .unwrap_or((snapshot.tree.root(), snapshot.epoch));
    let mut spec = build_spec(&snapshot, &request, parent)?;
    if context.begin_write()? {
        let snapshot = session.snapshot().map_err(HostError::from_session)?;
        spec = build_spec(&snapshot, &request, parent)?;
    }
    if let Some((graph, epoch)) = request.graph {
        session
            .execute(Command::SetPlotSpec { graph, spec })
            .map_err(HostError::from_session)?;
        Ok((graph, epoch))
    } else {
        let id = bridge::add_created_node(
            session,
            parent.0,
            Command::AddGraph {
                parent: parent.0,
                name: request.name,
                spec,
            },
        )?;
        Ok((id, parent.1))
    }
}
