//! Series groups from pyst (#196): reading, creating, changing.
//!
//! No rule here: everything goes through the `PanelSpec` gestures that the
//! interface also uses (`group_series`, `set_series_group`, `rename_group`,
//! `dissolve_group`, `remove_group`, `apply_group_gradient`), and the write is
//! a `SetPlotSpec` in the script's undoable group. The targeted layer is the
//! ordinary layer of the cell `panel=(row, column)`, like `add_shape`.
//! Group numbers are those of the spec: stable as long as the group exists,
//! never reused while a name reserves them.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use stokesforge_core::NodeId;
use stokesforge_plot::{PanelSpec, GRADIENT_PRESETS};
use stokesforge_session::Snapshot;

use crate::bridge::{self, HostError};
use crate::engine::{self, ScriptContext};
use crate::graph::{parse_line_style, parse_symbol};
use crate::surface::{to_py_error, Handle};

type Panel = (usize, usize);

/// Changes the graph's `panel` and returns what `edit` computes; a
/// gesture that does not change the spec writes nothing. `edit` runs on the
/// snapshot read before the write group's reservation, then again on the one
/// it freezes if it just opened the macro (#67).
fn edit_panel<T>(
    context: &ScriptContext,
    graph: NodeId,
    epoch: u64,
    panel: Panel,
    edit: impl Fn(&mut PanelSpec, &Snapshot) -> Result<T, HostError>,
) -> Result<T, HostError> {
    let prepare = |snapshot: &Snapshot| {
        let before = bridge::graph_spec(snapshot, graph, epoch)?;
        let mut spec = before.clone();
        let layer = bridge::base_layer_at(&spec, panel.0, panel.1).ok_or_else(|| {
            HostError::InvalidValue(format!(
                "the graph has no panel at row {}, column {}",
                panel.0, panel.1
            ))
        })?;
        let panel = spec
            .panels
            .get_mut(layer)
            .ok_or_else(|| HostError::InvalidValue("the graph has no such panel".to_owned()))?;
        let value = edit(panel, snapshot)?;
        // A gesture with no effect writes nothing: no empty history entry,
        // no macro opened for nothing.
        let changed = spec != before;
        Ok::<_, HostError>((spec, value, changed))
    };
    let session = context.session();
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    let (mut spec, mut value, mut changed) = prepare(&snapshot)?;
    if !changed {
        return Ok(value);
    }
    if context.begin_write()? {
        let snapshot = session.snapshot().map_err(HostError::from_session)?;
        (spec, value, changed) = prepare(&snapshot)?;
    }
    if changed {
        bridge::set_graph_spec(session, graph, spec)?;
    }
    Ok(value)
}

fn unknown_group(group: u32) -> HostError {
    HostError::InvalidValue(format!("the panel has no group {group}"))
}

/// The panel's series, in drawing order, each with its group.
#[pyfunction]
#[pyo3(signature = (graph, *, panel=(0, 0)))]
pub(crate) fn series(py: Python<'_>, graph: &Handle, panel: Panel) -> PyResult<Py<PyList>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (graph.id, graph.epoch);
    let rows = py
        .detach(|| -> Result<_, HostError> {
            let snapshot = context
                .session()
                .snapshot()
                .map_err(HostError::from_session)?;
            let spec = bridge::graph_spec(&snapshot, id, epoch)?;
            let layer = bridge::base_layer_at(&spec, panel.0, panel.1).ok_or_else(|| {
                HostError::InvalidValue(format!(
                    "the graph has no panel at row {}, column {}",
                    panel.0, panel.1
                ))
            })?;
            Ok(spec.panels[layer]
                .series
                .iter()
                .map(|s| {
                    (
                        s.name.clone(),
                        s.group,
                        s.line.color.clone(),
                        s.visible,
                        s.y,
                    )
                })
                .collect::<Vec<_>>())
        })
        .map_err(to_py_error)?;
    let list = PyList::empty(py);
    for (rank, (name, group, color, visible, y)) in rows.into_iter().enumerate() {
        let row = PyDict::new(py);
        row.set_item("index", rank)?;
        row.set_item("name", name)?;
        row.set_item("group", group)?;
        row.set_item("color", color)?;
        row.set_item("visible", visible)?;
        row.set_item("y", Py::new(py, Handle { id: y, epoch })?)?;
        list.append(row)?;
    }
    Ok(list.unbind())
}

/// The panel's groups, in drawing order: `id`, `name` (`None` =
/// automatic name) and `members` (series ranks).
#[pyfunction]
#[pyo3(signature = (graph, *, panel=(0, 0)))]
pub(crate) fn series_groups(py: Python<'_>, graph: &Handle, panel: Panel) -> PyResult<Py<PyList>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (graph.id, graph.epoch);
    let rows = py
        .detach(|| -> Result<_, HostError> {
            let snapshot = context
                .session()
                .snapshot()
                .map_err(HostError::from_session)?;
            let spec = bridge::graph_spec(&snapshot, id, epoch)?;
            let layer = bridge::base_layer_at(&spec, panel.0, panel.1).ok_or_else(|| {
                HostError::InvalidValue(format!(
                    "the graph has no panel at row {}, column {}",
                    panel.0, panel.1
                ))
            })?;
            let panel = &spec.panels[layer];
            Ok(panel
                .group_ids()
                .into_iter()
                .map(|group| {
                    (
                        group,
                        panel.group_label(group).map(str::to_owned),
                        panel.group_members(group),
                    )
                })
                .collect::<Vec<_>>())
        })
        .map_err(to_py_error)?;
    let list = PyList::empty(py);
    for (group, name, members) in rows {
        let row = PyDict::new(py);
        row.set_item("id", group)?;
        row.set_item("name", name)?;
        row.set_item("members", members)?;
        list.append(row)?;
    }
    Ok(list.unbind())
}

/// Gathers the series `indices` (ranks from `pyst.series`) into a new group,
/// in place of the first one, and returns its number. `name` names it.
#[pyfunction]
#[pyo3(signature = (graph, indices, *, name=None, panel=(0, 0)))]
pub(crate) fn group_series(
    py: Python<'_>,
    graph: &Handle,
    indices: Vec<usize>,
    name: Option<String>,
    panel: Panel,
) -> PyResult<u32> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (graph.id, graph.epoch);
    py.detach(|| {
        edit_panel(&context, id, epoch, panel, |panel, _| {
            let group = panel.group_series(&indices).ok_or_else(|| {
                HostError::InvalidValue(
                    "indices must be distinct ranks of series of the panel".to_owned(),
                )
            })?;
            if let Some(name) = &name {
                panel.rename_group(group, name);
            }
            Ok(group)
        })
    })
    .map_err(to_py_error)
}

/// Puts series `index` into group `group` (behind its members), or takes it
/// out of its group with `None`. Returns its new rank.
#[pyfunction]
#[pyo3(signature = (graph, index, group, *, panel=(0, 0)))]
pub(crate) fn set_series_group(
    py: Python<'_>,
    graph: &Handle,
    index: usize,
    group: Option<u32>,
    panel: Panel,
) -> PyResult<usize> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (graph.id, graph.epoch);
    py.detach(|| {
        edit_panel(&context, id, epoch, panel, |panel, _| {
            let Some(series) = panel.series.get(index) else {
                return Err(HostError::InvalidValue(format!("no series {index}")));
            };
            if let Some(group) = group {
                if !panel.group_ids().contains(&group) {
                    return Err(unknown_group(group));
                }
            }
            if series.group == group {
                return Ok(index);
            }
            panel
                .set_series_group(index, group)
                .ok_or_else(|| HostError::InvalidValue(format!("series {index} cannot move")))
        })
    })
    .map_err(to_py_error)
}

/// Names the group; `""` makes the name automatic.
#[pyfunction]
#[pyo3(signature = (graph, group, name, *, panel=(0, 0)))]
pub(crate) fn rename_group(
    py: Python<'_>,
    graph: &Handle,
    group: u32,
    name: String,
    panel: Panel,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (graph.id, graph.epoch);
    py.detach(|| {
        edit_panel(&context, id, epoch, panel, |panel, _| {
            if !panel.group_ids().contains(&group) {
                return Err(unknown_group(group));
            }
            panel.rename_group(group, &name);
            Ok(())
        })
    })
    .map_err(to_py_error)
}

/// Dissolves the group: the series stay, with their style.
#[pyfunction]
#[pyo3(signature = (graph, group, *, panel=(0, 0)))]
pub(crate) fn ungroup(py: Python<'_>, graph: &Handle, group: u32, panel: Panel) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (graph.id, graph.epoch);
    py.detach(|| {
        edit_panel(&context, id, epoch, panel, |panel, _| {
            panel
                .dissolve_group(group)
                .then_some(())
                .ok_or_else(|| unknown_group(group))
        })
    })
    .map_err(to_py_error)
}

/// Removes the group **and its series**.
#[pyfunction]
#[pyo3(signature = (graph, group, *, panel=(0, 0)))]
pub(crate) fn remove_group(
    py: Python<'_>,
    graph: &Handle,
    group: u32,
    panel: Panel,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (graph.id, graph.epoch);
    py.detach(|| {
        edit_panel(&context, id, epoch, panel, |panel, _| {
            panel
                .remove_group(group)
                .then_some(())
                .ok_or_else(|| unknown_group(group))
        })
    })
    .map_err(to_py_error)
}

/// Common style: each given option is set on all the members.
#[pyfunction]
#[pyo3(signature = (graph, group, *, panel=(0, 0), line_style=None, line_width=None, symbol=None, symbol_size=None))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn set_group_style(
    py: Python<'_>,
    graph: &Handle,
    group: u32,
    panel: Panel,
    line_style: Option<&str>,
    line_width: Option<f64>,
    symbol: Option<&str>,
    symbol_size: Option<f64>,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let line_style = line_style.map(parse_line_style).transpose()?;
    let symbol = symbol.map(parse_symbol).transpose()?;
    for (name, value) in [("line_width", line_width), ("symbol_size", symbol_size)] {
        if value.is_some_and(|n| !n.is_finite() || n <= 0.0) {
            return Err(crate::graph::invalid(format!(
                "{name} must be finite and positive"
            )));
        }
    }
    let (id, epoch) = (graph.id, graph.epoch);
    py.detach(|| {
        edit_panel(&context, id, epoch, panel, |panel, _| {
            let members = panel.group_members(group);
            if members.is_empty() {
                return Err(unknown_group(group));
            }
            for index in members {
                let series = &mut panel.series[index];
                if let Some(style) = line_style {
                    series.line.style = style;
                }
                if let Some(width) = line_width {
                    series.line.width = width;
                }
                if let Some(shape) = symbol {
                    series.symbol.shape = shape;
                }
                if let Some(size) = symbol_size {
                    series.symbol.size = size;
                }
            }
            Ok(())
        })
    })
    .map_err(to_py_error)
}

/// Spreads a color gradient over the group's members. `gradient`: the id of
/// a built-in gradient (`"blue-red"`...), the name of a gradient saved in the
/// project, or a list of `"#RRGGBB"` stops.
#[pyfunction]
#[pyo3(signature = (graph, group, gradient, *, panel=(0, 0)))]
pub(crate) fn apply_gradient(
    py: Python<'_>,
    graph: &Handle,
    group: u32,
    gradient: &Bound<'_, PyAny>,
    panel: Panel,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let requested = match gradient.extract::<String>() {
        Ok(name) => Gradient::Named(name),
        Err(_) => Gradient::Stops(gradient.extract::<Vec<String>>().map_err(|_| {
            crate::graph::invalid("gradient must be a name or a list of \"#RRGGBB\" colors")
        })?),
    };
    let (id, epoch) = (graph.id, graph.epoch);
    py.detach(|| {
        edit_panel(&context, id, epoch, panel, |panel, snapshot| {
            if !panel.group_ids().contains(&group) {
                return Err(unknown_group(group));
            }
            let stops = requested.stops(snapshot)?;
            // Retained too (#499): the color bar reads the group's gradient.
            panel.set_group_gradient(group, &stops);
            Ok(())
        })
    })
    .map_err(to_py_error)
}

/// A choice of a keyed enum (`ColorBarScale`, ...) from the key a script wrote;
/// a key outside `choices` is refused with the list of the accepted ones.
fn bar_choice<T: Clone + PartialEq>(
    what: &str,
    key: &str,
    choices: &[T],
    from_key: impl Fn(&str) -> T,
    key_of: impl Fn(&T) -> String,
) -> Result<T, HostError> {
    let value = from_key(key);
    if choices.contains(&value) {
        return Ok(value);
    }
    let accepted: Vec<String> = choices.iter().map(key_of).collect();
    Err(HostError::InvalidValue(format!(
        "unknown {what} \"{key}\": use {}",
        accepted.join(", ")
    )))
}

/// The `row_column` option of `color_bar`: not given keeps the column, `None`
/// clears it, a column sets it.
pub(crate) enum RowColumn {
    Keep,
    Clear,
    Set(NodeId),
}

impl<'py> FromPyObject<'_, 'py> for RowColumn {
    type Error = PyErr;

    fn extract(object: pyo3::Borrowed<'_, 'py, PyAny>) -> PyResult<Self> {
        if object.is_none() {
            Ok(Self::Clear)
        } else {
            Ok(Self::Set(object.extract::<PyRef<'_, Handle>>()?.id))
        }
    }
}

/// Sets the color bar of the panel (#499): the groups on it, in bar order,
/// and how it reads and draws them. Creates the bar (visible) when the layer
/// has none; an option left out keeps its value. A group put on the bar
/// without a retained gradient gets the first built-in one, so its curves have
/// colors to take. One undo step.
///
/// `groups`: group numbers in bar order (a group already on the bar keeps its
/// range and break label); `labels`: Typst break labels, one per group
/// (`''` for none); `ranges`: `(from, to)` per group, each end a number or
/// `None` for automatic. `value_from`: `'x_prime'`, `'comments'`,
/// `'long_name'`, `'short_name'`, `'units'`; `row_column`: the column whose
/// cell gives the value of a row series; `scale`: `'linear'` or `'log'`;
/// `orientation`: `'horizontal'` or `'vertical'` (forced by `'below'` and
/// `'right'`); `position`: `'top_left'`, `'top_right'`, `'bottom_left'`,
/// `'bottom_right'`, `'below'`, `'right'`; `ticks`: `'breaks'` or `'auto'`;
/// `title`: Typst text; `visible`: show or hide the bar (hidden, the curves
/// get their own colors and legend entries back).
#[pyfunction]
#[pyo3(signature = (graph, groups=None, *, value_from=None, row_column=RowColumn::Keep, chain=None, scale=None, orientation=None, position=None, ticks=None, title=None, labels=None, ranges=None, visible=None, panel=(0, 0)))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn color_bar(
    py: Python<'_>,
    graph: &Handle,
    groups: Option<Vec<u32>>,
    value_from: Option<String>,
    row_column: RowColumn,
    chain: Option<bool>,
    scale: Option<String>,
    orientation: Option<String>,
    position: Option<String>,
    ticks: Option<String>,
    title: Option<String>,
    labels: Option<Vec<String>>,
    ranges: Option<Vec<(Option<f64>, Option<f64>)>>,
    visible: Option<bool>,
    panel: Panel,
) -> PyResult<()> {
    use stokesforge_plot::{
        ColorBarOrientation, ColorBarPosition, ColorBarScale, ColorBarSegment, ColorBarTicks,
        ColorBarValue,
    };
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let row_column: Option<Option<NodeId>> = match row_column {
        RowColumn::Keep => None,
        RowColumn::Clear => Some(None),
        RowColumn::Set(id) => Some(Some(id)),
    };
    // Every option is read before anything is written: a refused one changes
    // nothing.
    let parsed = (|| -> Result<_, HostError> {
        Ok((
            value_from
                .as_deref()
                .map(|key| {
                    bar_choice(
                        "value source",
                        key,
                        ColorBarValue::CHOICES,
                        ColorBarValue::from_key,
                        |v| v.key().to_owned(),
                    )
                })
                .transpose()?,
            scale
                .as_deref()
                .map(|key| {
                    bar_choice(
                        "scale",
                        key,
                        ColorBarScale::CHOICES,
                        ColorBarScale::from_key,
                        |v| v.key().to_owned(),
                    )
                })
                .transpose()?,
            orientation
                .as_deref()
                .map(|key| {
                    bar_choice(
                        "orientation",
                        key,
                        ColorBarOrientation::CHOICES,
                        ColorBarOrientation::from_key,
                        |v| v.key().to_owned(),
                    )
                })
                .transpose()?,
            position
                .as_deref()
                .map(|key| {
                    bar_choice(
                        "position",
                        key,
                        ColorBarPosition::CHOICES,
                        ColorBarPosition::from_key,
                        |v| v.key().to_owned(),
                    )
                })
                .transpose()?,
            ticks
                .as_deref()
                .map(|key| {
                    bar_choice(
                        "ticks",
                        key,
                        ColorBarTicks::CHOICES,
                        ColorBarTicks::from_key,
                        |v| v.key().to_owned(),
                    )
                })
                .transpose()?,
        ))
    })()
    .map_err(to_py_error)?;
    let (value_from, scale, orientation, position, ticks) = parsed;
    if let Some(groups) = &groups {
        let mut seen = Vec::new();
        for group in groups {
            if seen.contains(group) {
                return Err(crate::graph::invalid(format!(
                    "group {group} is listed twice"
                )));
            }
            seen.push(*group);
        }
        for (name, len) in [
            ("labels", labels.as_ref().map(Vec::len)),
            ("ranges", ranges.as_ref().map(Vec::len)),
        ] {
            if len.is_some_and(|len| len != groups.len()) {
                return Err(crate::graph::invalid(format!(
                    "{name} needs one entry per group ({} groups)",
                    groups.len()
                )));
            }
        }
    }
    let (id, epoch) = (graph.id, graph.epoch);
    py.detach(|| {
        edit_panel(&context, id, epoch, panel, |panel, _| {
            let mut bar = panel.color_bar.clone().unwrap_or_default();
            if let Some(groups) = &groups {
                let mut segments = Vec::with_capacity(groups.len());
                for (rank, group) in groups.iter().enumerate() {
                    if !panel.group_ids().contains(group) {
                        return Err(unknown_group(*group));
                    }
                    let mut segment = bar
                        .segment_of(*group)
                        .cloned()
                        .unwrap_or_else(|| ColorBarSegment::new(*group));
                    if let Some(labels) = &labels {
                        segment.label.clone_from(&labels[rank]);
                    }
                    if let Some(ranges) = &ranges {
                        (segment.from, segment.to) = ranges[rank];
                    }
                    segments.push(segment);
                }
                bar.segments = segments;
            } else if labels.is_some() || ranges.is_some() {
                let count = bar.segments.len();
                for (name, len) in [
                    ("labels", labels.as_ref().map(Vec::len)),
                    ("ranges", ranges.as_ref().map(Vec::len)),
                ] {
                    if len.is_some_and(|len| len != count) {
                        return Err(HostError::InvalidValue(format!(
                            "{name} needs one entry per group on the bar ({count})"
                        )));
                    }
                }
                for (rank, segment) in bar.segments.iter_mut().enumerate() {
                    if let Some(labels) = &labels {
                        segment.label.clone_from(&labels[rank]);
                    }
                    if let Some(ranges) = &ranges {
                        (segment.from, segment.to) = ranges[rank];
                    }
                }
            }
            if let Some(value) = &value_from {
                bar.value = value.clone();
            }
            if let Some(column) = row_column {
                bar.row_column = column;
            }
            if let Some(chain) = chain {
                bar.chain = chain;
            }
            if let Some(scale) = &scale {
                bar.scale = scale.clone();
            }
            if let Some(orientation) = &orientation {
                bar.orientation = orientation.clone();
            }
            if let Some(position) = &position {
                bar.position = position.clone();
                // A place named in a script wins over a dragged position.
                bar.anchor = None;
            }
            if let Some(ticks) = &ticks {
                bar.ticks = ticks.clone();
            }
            if let Some(title) = &title {
                bar.title.clone_from(title);
            }
            // A new bar is shown; an existing one keeps its visibility.
            if let Some(visible) = visible {
                bar.visible = visible;
            }
            let on_bar: Vec<u32> = bar.segments.iter().map(|segment| segment.group).collect();
            panel.color_bar = Some(bar);
            for group in on_bar {
                if panel.group_gradient(group).is_none() {
                    let first: Vec<&str> = GRADIENT_PRESETS[0].stops.to_vec();
                    panel.set_group_gradient(group, &first);
                }
            }
            Ok(())
        })
    })
    .map_err(to_py_error)
}

enum Gradient {
    Named(String),
    Stops(Vec<String>),
}

impl Gradient {
    fn stops(&self, snapshot: &Snapshot) -> Result<Vec<String>, HostError> {
        let stops = match self {
            Self::Named(name) => {
                let wanted = name.trim().to_lowercase();
                if let Some(preset) = GRADIENT_PRESETS.iter().find(|p| p.id == wanted) {
                    preset.stops.iter().map(|s| (*s).to_owned()).collect()
                } else if let Some(saved) = snapshot
                    .tree
                    .saved_colors()
                    .gradients
                    .iter()
                    .find(|g| g.name.to_lowercase() == wanted)
                {
                    saved.stops.clone()
                } else {
                    let ids: Vec<&str> = GRADIENT_PRESETS.iter().map(|p| p.id).collect();
                    return Err(HostError::InvalidValue(format!(
                        "unknown gradient \"{name}\": use a list of colors, a gradient saved in the project, or one of {}",
                        ids.join(", ")
                    )));
                }
            }
            Self::Stops(stops) => stops.clone(),
        };
        if stops.is_empty()
            || stops
                .iter()
                .any(|s| stokesforge_plot::parse_hex(s).is_none())
        {
            return Err(HostError::InvalidValue(
                "a gradient needs at least one color, each \"#RRGGBB\"".to_owned(),
            ));
        }
        Ok(stops)
    }
}
