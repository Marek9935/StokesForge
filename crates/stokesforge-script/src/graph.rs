//! Tracé natif depuis pyst : conversion explicite des options, puis commandes.
//!
//! Aucun moteur ici : le script produit le même PlotSpec que l'interface.
//! Les noms de série vides restent automatiques : le rendu partagé les lit
//! comme du texte littéral, sans interpréter le markup d'une colonne. Les
//! titres et labels explicitement fournis, eux, sont du markup Typst.
//! Sans option `title`, un nouveau graphe reste sans titre (#103) ; ajouter
//! une série à un graphe existant conserve son titre.
//! Le premier panneau est étendu sans réécrire les autres ni ses réglages.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use stokesforge_commands::Command;
use stokesforge_core::{NodeId, NodeKind};
use stokesforge_plot::{
    AxisScale, AxisSpec, BinCount, LineStyle, PlotSpec, PlotType, SeriesAxis, SeriesSpec,
    SymbolShape,
};
use stokesforge_session::Snapshot;

use crate::bridge::{self, HostError};
use crate::engine::{self, ScriptContext};
use crate::surface::{to_py_error, Handle, InvalidValueError};

type Target = (NodeId, u64);

/// Données Rust seulement : elles traversent Python::detach, jamais un objet Python.
struct Request {
    references: Vec<Target>,
    graph: Option<Target>,
    parent: Option<Target>,
    name: Option<String>,
    series: SeriesSpec,
    color: Option<String>,
    title: Option<String>,
    x_title: Option<String>,
    y_title: Option<String>,
    x_scale: Option<AxisScale>,
    y_scale: Option<AxisScale>,
    x_range: Option<(f64, f64)>,
    y_range: Option<(f64, f64)>,
}

fn invalid(message: impl Into<String>) -> PyErr {
    InvalidValueError::new_err(message.into())
}

/// Crée un graphe natif et rend sa poignée ; graph= ajoute au premier panneau.
/// y est une colonne ; x=None utilise l'indice de ligne. Options : plot_type
/// (line, scatter, line_scatter, bar, histogram, error_bar), color (#RRGGBB ou
/// #RRGGBBAA), line_style (none, solid, dash, dot, dash_dot), line_width,
/// symbol (none, circle, square, triangle, diamond, plus, cross), symbol_size,
/// label, title, x_title, y_title, x_scale/y_scale (linear, log), x_range/y_range
/// (min, max), axis (left, right), x_error/y_error (poignées), histogram_bins.
/// Titres et label explicites : markup Typst. Label absent : nom automatique
/// de la colonne, affiché littéralement. Une exécution = un groupe annulable.
#[pyfunction]
#[pyo3(signature = (y, x=None, *, graph=None, name=None, parent=None, **options))]
pub(crate) fn plot(
    py: Python<'_>,
    y: &Handle,
    x: Option<&Handle>,
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
    let mut request = Request {
        references: vec![target(y)],
        graph: graph.map(target),
        parent: parent.map(target),
        name,
        series: SeriesSpec::new(y.id),
        color: None,
        title: None,
        x_title: None,
        y_title: None,
        x_scale: None,
        y_scale: None,
        x_range: None,
        y_range: None,
    };
    if let Some(x) = x {
        request.series.x = Some(x.id);
        request.references.push(target(x));
    }
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
                    let color = string()?;
                    if stokesforge_plot::parse_hex(&color).is_none() { return Err(invalid("color must be a hexadecimal RGB or RGBA color")); }
                    request.color = Some(color);
                }
                "line_style" => request.series.line.style = match string()?.as_str() {
                    "none" => LineStyle::None, "solid" => LineStyle::Solid,
                    "dash" => LineStyle::Dash, "dot" => LineStyle::Dot, "dash_dot" => LineStyle::DashDot,
                    _ => return Err(invalid("unknown line_style: use none, solid, dash, dot or dash_dot")),
                },
                "line_width" => request.series.line.width = positive()?,
                "symbol_size" => request.series.symbol.size = positive()?,
                "symbol" => request.series.symbol.shape = match string()?.as_str() {
                    "none" => SymbolShape::None, "circle" => SymbolShape::Circle,
                    "square" => SymbolShape::Square, "triangle" => SymbolShape::Triangle,
                    "diamond" => SymbolShape::Diamond, "plus" => SymbolShape::Plus, "cross" => SymbolShape::Cross,
                    _ => return Err(invalid("unknown symbol: use none, circle, square, triangle, diamond, plus or cross")),
                },
                "label" => request.series.name = string()?,
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
                "x_range" | "y_range" => {
                    let (min, max) = value.extract::<(f64, f64)>().map_err(|_| invalid(format!("{key} must be a (min, max) pair")))?;
                    if !min.is_finite() || !max.is_finite() || min >= max { return Err(invalid(format!("{key} must contain finite increasing bounds"))); }
                    if key == "x_range" { request.x_range = Some((min, max)); } else { request.y_range = Some((min, max)); }
                }
                "axis" => request.series.axis = match string()?.as_str() {
                    "left" => SeriesAxis::Left, "right" => SeriesAxis::Right,
                    _ => return Err(invalid("unknown series axis: use left or right")),
                },
                "x_error" | "y_error" => {
                    let handle = value.extract::<PyRef<'_, Handle>>().map_err(|_| invalid(format!("{key} must be a column handle")))?;
                    request.references.push(target(&handle));
                    if key == "x_error" { request.series.x_error = Some(handle.id); } else { request.series.y_error = Some(handle.id); }
                }
                "histogram_bins" => {
                    let count = value.extract::<u32>().map_err(|_| invalid("histogram_bins must be a positive integer"))?;
                    request.series.histogram_bins = BinCount::new(count).ok_or_else(|| invalid(format!("histogram_bins must be between 1 and {}", stokesforge_plot::MAX_HISTOGRAM_BINS)))?;
                }
                _ => return Err(invalid(format!("unknown plot option: {key}"))),
            }
        }
    }
    let (id, epoch) = py
        .detach(|| plot_native(&context, request))
        .map_err(to_py_error)?;
    Ok(Handle { id, epoch })
}

fn update_axis(
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
        && axis.range_min.is_some_and(|min| min <= 0.0)
    {
        return Err(HostError::InvalidValue(
            "logarithmic axis bounds must be positive".into(),
        ));
    }
    Ok(())
}

/// Construit depuis un même instantané, avant ET après la réservation exclusive.
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
    let mut series = request.series.clone();
    let color = request
        .color
        .clone()
        .unwrap_or_else(|| stokesforge_plot::palette_color(panel.series.len()).into());
    series.line.color.clone_from(&color);
    series.symbol.color = color;
    update_axis(
        &mut panel.bottom,
        &request.x_title,
        request.x_scale,
        request.x_range,
    )?;
    let y_axis = panel.axis_mut(series.axis.side());
    update_axis(y_axis, &request.y_title, request.y_scale, request.y_range)?;
    if series.axis == SeriesAxis::Right {
        y_axis.visible = true;
    }
    panel.series.push(series);
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
