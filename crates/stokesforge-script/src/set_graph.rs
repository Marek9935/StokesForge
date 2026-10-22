//! `pyst.set_graph` : régler un graphe existant (#331, lot 2, S4).
//!
//! Les clés sont celles de la fenêtre Graph Properties, nommées comme les
//! options de `plot` quand elles existent là aussi (`title`, `x_title`,
//! `x_scale`, `x_from`/`x_to`…). Aucune règle nouvelle : on édite le
//! [`PlotSpec`] puis on l'écrit par `SetPlotSpec`, donc **une seule entrée
//! d'historique** quel que soit le nombre de clés, dans le groupe annulable du
//! script. Toutes les clés sont lues et validées avant la moindre écriture : une
//! clé inconnue ou une valeur refusée ne laisse rien derrière elle. Un appel qui
//! ne change rien n'écrit rien.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use stokesforge_core::NodeId;
use stokesforge_plot::{AxisScale, AxisSpec, LegendPosition, PlotSpec, SeriesAxis};
use stokesforge_session::Snapshot;

use crate::bridge::{self, HostError};
use crate::engine::{self, ScriptContext};
use crate::graph::{axis_range, invalid, update_axis};
use crate::surface::{to_py_error, Handle};

/// Les clés que `set_graph` accepte, en un seul endroit : l'analyse des options,
/// la documentation et le test de surface partent de cette liste.
pub(crate) const SET_GRAPH_OPTIONS: &[&str] = &[
    // Le graphe.
    "title",
    "title_visible",
    "title_color",
    "background",
    "stale_warning",
    "decimal_comma",
    // Le panneau visé.
    "panel_title",
    // L'axe X (bas) et l'axe Y (gauche ou droite selon `axis`).
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
    // La légende du panneau visé.
    "legend_visible",
    "legend_position",
];

/// Ce qu'on demande de changer sur un axe ; `None` = ne pas y toucher.
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
        // Titre, échelle et bornes : le même geste que `plot`, avec son refus
        // d'une borne non positive sur une échelle logarithmique.
        update_axis(axis, &self.title, self.scale, range)
    }
}

/// Les changements demandés : données Rust seulement, elles traversent
/// `Python::detach`.
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
}

impl Changes {
    fn parse(options: &Bound<'_, PyDict>) -> PyResult<Self> {
        let mut changes = Self::default();
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
            // Vide = automatique, sinon `#RRGGBB` ou `#RRGGBBAA`.
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
            match key.as_str() {
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
        Ok(changes)
    }

    /// Applique les changements au spec ; `layer` est le rang du panneau visé.
    fn apply(&self, spec: &mut PlotSpec, layer: usize, side: SeriesAxis) -> Result<(), HostError> {
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
        changes.apply(&mut spec, layer, side)?;
        let changed = spec != before;
        Ok::<_, HostError>((spec, changed))
    };
    let session = context.session();
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    let (mut spec, mut changed) = prepare(&snapshot)?;
    if !changed {
        return Ok(());
    }
    if context.begin_write()? {
        // Même validation sous la réservation exclusive du groupe du script.
        let snapshot = session.snapshot().map_err(HostError::from_session)?;
        (spec, changed) = prepare(&snapshot)?;
    }
    if changed {
        bridge::set_graph_spec(session, graph, spec)?;
    }
    Ok(())
}

/// Règle un graphe existant par les clés de la fenêtre Graph Properties ; une
/// seule entrée d'historique (`pyst.undo()` la défait). Les clés non données ne
/// changent rien.
///
/// Le graphe : `title`, `title_visible`, `title_color` (`'#RRGGBB'`, `''` =
/// automatique), `background` (`'#RRGGBB[AA]'`, `''` = blanc), `stale_warning`,
/// `decimal_comma` (booléens). Le panneau `panel=(ligne, colonne)` : `panel_title`,
/// `legend_visible`, `legend_position` (`'top_left'`, `'top_right'`,
/// `'bottom_left'`, `'bottom_right'`). L'axe X (en bas) et l'axe Y (`axis='left'`
/// ou `'right'`) : `x_title`/`y_title`, `x_unit`/`y_unit`, `x_scale`/`y_scale`
/// (`'linear'`, `'log'`), `x_from` et `x_to` (ensemble et différents ; `from > to`
/// trace l'axe à l'envers) et `y_from`/`y_to`, `x_auto_range`/`y_auto_range`
/// (rend le cadrage automatique), `x_inverted`/`y_inverted`, `x_grid`/`y_grid`,
/// `x_visible`/`y_visible`. Titres : markup Typst, comme dans `plot`.
///
/// Une clé inconnue, une valeur du mauvais type ou des bornes refusées lèvent
/// `InvalidValueError` sans rien écrire ; `BrokenReferenceError` pour une
/// poignée périmée.
#[pyfunction]
#[pyo3(signature = (graph, *, panel=(0, 0), axis="left", **options))]
pub(crate) fn set_graph(
    py: Python<'_>,
    graph: &Handle,
    panel: (usize, usize),
    axis: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let side = match axis {
        "left" => SeriesAxis::Left,
        "right" => SeriesAxis::Right,
        _ => return Err(invalid("unknown axis: use left or right")),
    };
    let changes = match options {
        Some(options) => Changes::parse(options)?,
        None => Changes::default(),
    };
    let (id, epoch) = (graph.id, graph.epoch);
    py.detach(|| set_graph_native(&context, id, epoch, panel, side, &changes))
        .map_err(to_py_error)
}
