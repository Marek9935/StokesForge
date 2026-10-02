//! Projection en lecture du graphe actif pour Object Manager (#81–83).
//!
//! Le panneau ne possède ni séries ni sélection : le spec vient de l'instantané,
//! la sélection de GraphView. Les cases des parents commandent leurs contenus
//! en une seule écriture SetPlotSpec ; les axes et la disposition restent en
//! place. Un parent partiellement visible est indéterminé, cliquer le révèle.
//! Textes et formes portent déjà leur visibilité ; leur arbre viendra avec #85.

use stokesforge_core::{series_label, NodeId, Tree};
use stokesforge_plot::{AnnotationRef, PlotSpec, SeriesRef, SeriesSpec, ShapeRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibilityTarget {
    Graph,
    Panel(usize),
    Group(usize),
    Series(SeriesRef),
    Annotation(AnnotationRef),
    Shape(ShapeRef),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Visibility {
    pub checked: bool,
    pub mixed: bool,
    pub enabled: bool,
}

impl VisibilityTarget {
    fn values(self, spec: &PlotSpec) -> Vec<bool> {
        match self {
            Self::Graph => spec
                .panels
                .iter()
                .flat_map(|p| {
                    p.series
                        .iter()
                        .map(|s| s.visible)
                        .chain(p.annotations.iter().map(|a| a.visible))
                        .chain(p.shapes.iter().map(|s| s.visible))
                })
                .collect(),
            Self::Panel(index) => spec
                .panels
                .get(index)
                .map(|p| {
                    p.series
                        .iter()
                        .map(|s| s.visible)
                        .chain(p.annotations.iter().map(|a| a.visible))
                        .chain(p.shapes.iter().map(|s| s.visible))
                        .collect()
                })
                .unwrap_or_default(),
            Self::Group(index) => spec
                .panels
                .get(index)
                .map(|p| p.series.iter().map(|s| s.visible).collect())
                .unwrap_or_default(),
            Self::Series(target) => spec
                .series_at(target)
                .map(|s| vec![s.visible])
                .unwrap_or_default(),
            Self::Annotation(target) => target
                .get(spec)
                .map(|a| vec![a.visible])
                .unwrap_or_default(),
            Self::Shape(target) => target
                .get(spec)
                .map(|s| vec![s.visible])
                .unwrap_or_default(),
        }
    }

    pub fn visibility(self, spec: &PlotSpec) -> Visibility {
        let values = self.values(spec);
        Visibility {
            checked: !values.is_empty() && values.iter().all(|v| *v),
            mixed: values.iter().any(|v| *v) && values.iter().any(|v| !v),
            enabled: !values.is_empty(),
        }
    }

    /// Cible périmée ou valeur identique : aucune entrée d'historique.
    pub fn set(self, spec: &mut PlotSpec, visible: bool) -> bool {
        let changed = self.values(spec).iter().any(|value| *value != visible);
        if !changed {
            return false;
        }
        match self {
            Self::Graph | Self::Panel(_) | Self::Group(_) => {
                for (index, panel) in spec.panels.iter_mut().enumerate() {
                    if matches!(self, Self::Panel(i) | Self::Group(i) if i != index) {
                        continue;
                    }
                    for series in &mut panel.series {
                        series.visible = visible;
                    }
                    if !matches!(self, Self::Group(_)) {
                        for text in &mut panel.annotations {
                            text.visible = visible;
                        }
                        for shape in &mut panel.shapes {
                            shape.visible = visible;
                        }
                    }
                }
            }
            Self::Series(target) => {
                if let Some(series) = spec.series_at_mut(target) {
                    series.visible = visible;
                }
            }
            Self::Annotation(target) => {
                if let Some(mut text) = target.get(spec).cloned() {
                    text.visible = visible;
                    target.set(spec, text);
                }
            }
            Self::Shape(target) => {
                if let Some(mut shape) = target.get(spec).cloned() {
                    shape.visible = visible;
                    target.set(spec, shape);
                }
            }
        }
        true
    }
}

#[derive(Debug, Clone)]
pub struct SeriesRow {
    pub target: SeriesRef,
    pub label: String,
    pub spec: SeriesSpec,
    pub selected: bool,
}
#[derive(Debug, Clone)]
pub struct LayerRow {
    pub index: usize,
    pub series: Vec<SeriesRow>,
}
#[derive(Debug, Clone)]
pub struct ObjectTree {
    pub graph: NodeId,
    pub title: String,
    pub layers: Vec<LayerRow>,
}

pub fn object_tree(tree: &Tree, graph: NodeId, selected: Option<SeriesRef>) -> Option<ObjectTree> {
    let node = tree.node(graph)?;
    let spec = crate::state::current_plot_spec(tree, graph)?;
    Some(ObjectTree {
        graph,
        title: node.name().to_string(),
        layers: spec
            .panels
            .iter()
            .enumerate()
            .map(|(index, panel)| LayerRow {
                index,
                series: panel
                    .series
                    .iter()
                    .enumerate()
                    .map(|(rank, spec)| {
                        let target = SeriesRef::new(index, rank);
                        SeriesRow {
                            target,
                            label: series_label(spec, tree),
                            spec: spec.clone(),
                            selected: selected == Some(target),
                        }
                    })
                    .collect(),
            })
            .collect(),
    })
}
