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
            panel.apply_group_gradient(group, &stops);
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
