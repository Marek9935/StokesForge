//! Projection en lecture du graphe actif pour Object Manager (#81–85).
//!
//! Le panneau ne possède ni contenus ni sélection : le spec vient de
//! l'instantané, la sélection de GraphView. Les parents commandent tous leurs
//! contenus en une écriture du spec ; les axes et la disposition restent en
//! place. Le groupe ne commande que les séries. Un parent mixte se révèle au
//! clic. Les adresses restent locales au spec : un déplacement suit le contenu,
//! une suppression efface la sélection plutôt que de viser son voisin.
//!
//! Échelles (#111) : sous chaque calque, un groupe « Axes » réunit une ligne
//! par axe — les quatre, masqués par défaut compris. Il est caché par défaut et
//! listé par sa propre bascule « Show scales » (même motif que les textes,
//! `object_manager_show_scales`). La case d'un axe commande `AxisSpec::visible`
//! (ligne, graduations et libellés d'un coup) ; celle du groupe
//! ([`VisibilityTarget::Axes`]) commande ses quatre axes et se montre mixte
//! tant que Right et Top restent masqués. Les cases du graphique, du calque et
//! du groupe de tracés **ne commandent pas** les axes : le parent serait
//! indéterminé dès l'ouverture, et « tout afficher » ferait apparaître deux
//! axes que l'utilisateur n'a pas demandés.
//!
//! Vue par défaut (#102) : calques, groupes de tracés et séries, plus les
//! objets rares (formes, textes libres). Les textes automatiques — titre du
//! graphique et des calques, titres d'axes, légende — ne sont listés que si la
//! bascule « Show graph texts » est active (préférence d'interface, hors `.stf`) ;
//! le filtre vit dans [`object_tree_with_selection`]. Les cases du graphique et
//! du calque commandent toujours le contenu complet, textes cachés compris
//! ([`VisibilityTarget::contents`]).

use stokesforge_core::{series_label, NodeId, Tree};
use stokesforge_plot::{
    AnnotationRef, AxisSide, PlotSpec, SeriesRef, SeriesSpec, ShapeKind, ShapeRef, TextTarget,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VisibilityTarget {
    Graph,
    Panel(usize),
    Group(usize),
    Series(SeriesRef),
    Text(TextTarget),
    Annotation(AnnotationRef),
    Shape(ShapeRef),
    /// Une échelle (ligne d'axe, graduations et libellés) : `AxisSpec::visible`.
    Axis(AxisRef),
    /// Le groupe des quatre axes d'un calque : il ne commande que ces axes.
    Axes(usize),
}

/// Un axe d'un calque, adressé par la position du calque dans la grille.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AxisRef {
    pub row: usize,
    pub column: usize,
    pub side: AxisSide,
}

impl AxisRef {
    fn get(self, spec: &PlotSpec) -> Option<&stokesforge_plot::AxisSpec> {
        spec.panels
            .iter()
            .find(|p| (p.row, p.column) == (self.row, self.column))
            .map(|p| p.axis(self.side))
    }
    fn get_mut(self, spec: &mut PlotSpec) -> Option<&mut stokesforge_plot::AxisSpec> {
        spec.panels
            .iter_mut()
            .find(|p| (p.row, p.column) == (self.row, self.column))
            .map(|p| p.axis_mut(self.side))
    }
}

/// Les quatre axes, dans l'ordre d'affichage de l'arbre.
pub const AXIS_SIDES: [AxisSide; 4] = [
    AxisSide::Bottom,
    AxisSide::Left,
    AxisSide::Right,
    AxisSide::Top,
];
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Visibility {
    pub checked: bool,
    pub mixed: bool,
    pub enabled: bool,
}

impl VisibilityTarget {
    /// Feuilles concernées ; un titre vide ne rend pas un parent indéterminé.
    pub fn contents(self, spec: &PlotSpec) -> Vec<Self> {
        if let Self::Axes(index) = self {
            let Some(panel) = spec.panels.get(index) else {
                return Vec::new();
            };
            return AXIS_SIDES
                .into_iter()
                .map(|side| {
                    Self::Axis(AxisRef {
                        row: panel.row,
                        column: panel.column,
                        side,
                    })
                })
                .collect();
        }
        if !matches!(self, Self::Graph | Self::Panel(_) | Self::Group(_)) {
            return vec![self];
        }
        let mut leaves = Vec::new();
        if self == Self::Graph && !spec.title.is_empty() {
            leaves.push(Self::Text(TextTarget::Title));
        }
        for (index, panel) in spec.panels.iter().enumerate() {
            if matches!(self, Self::Panel(i) | Self::Group(i) if i != index) {
                continue;
            }
            leaves.extend(
                panel
                    .series
                    .iter()
                    .enumerate()
                    .map(|(rank, _)| Self::Series(SeriesRef::new(index, rank))),
            );
            if matches!(self, Self::Group(_)) {
                continue;
            }
            leaves.extend(panel_texts(spec, index).into_iter().map(Self::Text));
            leaves.extend(panel.annotations.iter().enumerate().map(|(rank, _)| {
                Self::Annotation(AnnotationRef {
                    row: panel.row,
                    column: panel.column,
                    index: rank,
                })
            }));
            leaves.extend(panel.shapes.iter().enumerate().map(|(rank, _)| {
                Self::Shape(ShapeRef {
                    row: panel.row,
                    column: panel.column,
                    index: rank,
                })
            }));
        }
        leaves
    }
    fn value(self, spec: &PlotSpec) -> Option<bool> {
        match self {
            Self::Series(t) => spec.series_at(t).map(|s| s.visible),
            Self::Text(t) => t.visible(spec),
            Self::Annotation(t) => t.get(spec).map(|a| a.visible),
            Self::Shape(t) => t.get(spec).map(|s| s.visible),
            Self::Axis(t) => t.get(spec).map(|a| a.visible),
            Self::Graph | Self::Panel(_) | Self::Group(_) | Self::Axes(_) => None,
        }
    }
    pub fn visibility(self, spec: &PlotSpec) -> Visibility {
        let values: Vec<bool> = self
            .contents(spec)
            .into_iter()
            .filter_map(|t| t.value(spec))
            .collect();
        Visibility {
            checked: !values.is_empty() && values.iter().all(|v| *v),
            mixed: values.iter().any(|v| *v) && values.iter().any(|v| !v),
            enabled: !values.is_empty(),
        }
    }
    /// Cible périmée ou valeur identique : aucune entrée d'historique.
    pub fn set(self, spec: &mut PlotSpec, visible: bool) -> bool {
        let mut changed = false;
        for leaf in self.contents(spec) {
            if leaf.value(spec).is_none_or(|value| value == visible) {
                continue;
            }
            changed = true;
            match leaf {
                Self::Series(t) => {
                    if let Some(s) = spec.series_at_mut(t) {
                        s.visible = visible;
                    }
                }
                Self::Text(t) => {
                    t.set_visible(spec, visible);
                }
                Self::Annotation(t) => {
                    if let Some(mut a) = t.get(spec).cloned() {
                        a.visible = visible;
                        t.set(spec, a);
                    }
                }
                Self::Shape(t) => {
                    if let Some(mut s) = t.get(spec).cloned() {
                        s.visible = visible;
                        t.set(spec, s);
                    }
                }
                Self::Axis(t) => {
                    if let Some(axis) = t.get_mut(spec) {
                        axis.visible = visible;
                    }
                }
                Self::Graph | Self::Panel(_) | Self::Group(_) | Self::Axes(_) => {}
            }
        }
        changed
    }
    pub fn exists(self, spec: &PlotSpec) -> bool {
        match self {
            Self::Graph => true,
            Self::Panel(i) | Self::Group(i) | Self::Axes(i) => spec.panels.get(i).is_some(),
            _ => self.value(spec).is_some(),
        }
    }
    pub fn removable(self, spec: &PlotSpec) -> bool {
        self.exists(spec)
            && matches!(
                self,
                Self::Series(_)
                    | Self::Annotation(_)
                    | Self::Shape(_)
                    | Self::Text(
                        TextTarget::Title
                            | TextTarget::PanelTitle { .. }
                            | TextTarget::AxisTitle { .. }
                    )
            )
    }
}

/// Les adresses de rang suivent le contenu après suppression d'un voisin.
pub fn selection_after_remove(
    selected: Option<VisibilityTarget>,
    removed: VisibilityTarget,
) -> Option<VisibilityTarget> {
    use VisibilityTarget as T;
    let selected = selected?;
    if selected == removed {
        return None;
    }
    Some(match (selected, removed) {
        (T::Series(mut s), T::Series(r)) if s.panel == r.panel && s.index > r.index => {
            s.index -= 1;
            T::Series(s)
        }
        (T::Annotation(mut s), T::Annotation(r))
            if (s.row, s.column) == (r.row, r.column) && s.index > r.index =>
        {
            s.index -= 1;
            T::Annotation(s)
        }
        (T::Shape(mut s), T::Shape(r))
            if (s.row, s.column) == (r.row, r.column) && s.index > r.index =>
        {
            s.index -= 1;
            T::Shape(s)
        }
        _ => selected,
    })
}

/// Undo/redo peut changer les rangs sans passer par le geste d'interface.
/// Retrouver le contenu exact permet de suivre une permutation ; une édition
/// de style ou de texte conserve l'adresse quand le nombre de contenus reste
/// identique. Sans correspondant après une suppression, on désélectionne.
pub fn selection_after_history(
    previous: &PlotSpec,
    next: &PlotSpec,
    selected: Option<VisibilityTarget>,
) -> Option<VisibilityTarget> {
    use VisibilityTarget as T;
    let selected = selected?;
    let remap = |old_index: usize,
                 old_len: usize,
                 same_at_index: bool,
                 matches: Vec<usize>,
                 new_len: usize| {
        if same_at_index {
            Some(old_index)
        } else if matches.len() == 1 {
            matches.first().copied()
        } else if old_len == new_len && old_index < new_len {
            Some(old_index)
        } else {
            None
        }
    };
    match selected {
        T::Series(t) => {
            let old = previous.panels.get(t.panel)?;
            let new = next.panels.get(t.panel)?;
            let value = old.series.get(t.index)?;
            remap(
                t.index,
                old.series.len(),
                new.series.get(t.index) == Some(value),
                new.series
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| (s == value).then_some(i))
                    .collect(),
                new.series.len(),
            )
            .map(|index| T::Series(SeriesRef { index, ..t }))
        }
        T::Annotation(t) => {
            let old = previous
                .panels
                .iter()
                .find(|p| (p.row, p.column) == (t.row, t.column))?;
            let new = next
                .panels
                .iter()
                .find(|p| (p.row, p.column) == (t.row, t.column))?;
            let value = t.get(previous)?;
            remap(
                t.index,
                old.annotations.len(),
                new.annotations.get(t.index) == Some(value),
                new.annotations
                    .iter()
                    .enumerate()
                    .filter_map(|(i, a)| (a == value).then_some(i))
                    .collect(),
                new.annotations.len(),
            )
            .map(|index| T::Annotation(AnnotationRef { index, ..t }))
        }
        T::Shape(t) => {
            let old = previous
                .panels
                .iter()
                .find(|p| (p.row, p.column) == (t.row, t.column))?;
            let new = next
                .panels
                .iter()
                .find(|p| (p.row, p.column) == (t.row, t.column))?;
            let value = t.get(previous)?;
            remap(
                t.index,
                old.shapes.len(),
                new.shapes.get(t.index) == Some(value),
                new.shapes
                    .iter()
                    .enumerate()
                    .filter_map(|(i, a)| (a == value).then_some(i))
                    .collect(),
                new.shapes.len(),
            )
            .map(|index| T::Shape(ShapeRef { index, ..t }))
        }
        _ => selected.exists(next).then_some(selected),
    }
}

fn panel_texts(spec: &PlotSpec, index: usize) -> Vec<TextTarget> {
    let Some(panel) = spec.panels.get(index) else {
        return Vec::new();
    };
    let (row, column) = (panel.row, panel.column);
    let mut texts = Vec::new();
    if !panel.title.is_empty() {
        texts.push(TextTarget::PanelTitle { row, column });
    }
    // Le modèle réserve aussi Top ; le pipeline ne trace encore que ces
    // trois axes. Ne pas proposer un titre sans composant à éditer.
    for side in [AxisSide::Bottom, AxisSide::Left, AxisSide::Right] {
        if !panel.axis(side).display_label().is_empty() {
            texts.push(TextTarget::AxisTitle { row, column, side });
        }
    }
    if !panel.series.is_empty() {
        texts.push(TextTarget::Legend { row, column });
    }
    texts
}

/// Rang final après un déplacement dans le groupe. Les dépôts entre calques
/// sont refusés : ils changeraient les axes de rattachement des données.
pub fn series_drop(
    spec: &PlotSpec,
    source: SeriesRef,
    onto: SeriesRef,
    after: bool,
) -> Option<usize> {
    spec.series_at(source)?;
    spec.series_at(onto)?;
    if source.panel != onto.panel || source == onto {
        return None;
    }
    let slot = onto.index + usize::from(after);
    let index = slot - usize::from(source.index < slot);
    (index != source.index).then_some(index)
}
pub fn reorder_series(spec: &mut PlotSpec, source: SeriesRef, index: usize) -> bool {
    let Some(panel) = spec.panels.get_mut(source.panel) else {
        return false;
    };
    if source.index >= panel.series.len() || index >= panel.series.len() || source.index == index {
        return false;
    }
    let series = panel.series.remove(source.index);
    panel.series.insert(index, series);
    true
}

#[derive(Debug, Clone)]
pub struct SeriesRow {
    pub target: SeriesRef,
    pub label: String,
    pub spec: SeriesSpec,
    pub selected: bool,
}
#[derive(Debug, Clone, Copy)]
pub enum ObjectKind {
    Text,
    Shape(ShapeKind),
    Axis,
}
#[derive(Debug, Clone)]
pub struct ObjectRow {
    pub target: VisibilityTarget,
    pub label: String,
    pub kind: ObjectKind,
    pub selected: bool,
}
#[derive(Debug, Clone)]
pub struct LayerRow {
    pub index: usize,
    pub series: Vec<SeriesRow>,
    /// Les lignes du groupe « Axes » (#111) ; vide sans la bascule.
    pub axes: Vec<ObjectRow>,
    pub objects: Vec<ObjectRow>,
}
#[derive(Debug, Clone)]
pub struct ObjectTree {
    pub graph: NodeId,
    pub title: String,
    pub layers: Vec<LayerRow>,
    pub objects: Vec<ObjectRow>,
}

fn axis_label(side: AxisSide) -> String {
    let axis = match side {
        AxisSide::Bottom => crate::tr!("plot-axis-bottom"),
        AxisSide::Left => crate::tr!("plot-axis-left"),
        AxisSide::Right => crate::tr!("plot-axis-right"),
        AxisSide::Top => crate::tr!("plot-axis-top"),
    };
    crate::tr!("plot-target-axis", axis = axis)
}

pub fn object_tree(tree: &Tree, graph: NodeId, selected: Option<SeriesRef>) -> Option<ObjectTree> {
    object_tree_with_selection(
        tree,
        graph,
        selected.map(VisibilityTarget::Series),
        false,
        false,
    )
}
/// `show_texts` : lister les textes automatiques (titre du graphique et des
/// calques, titres d'axes, légende). Faux par défaut (#102) ; les textes libres
/// et les formes sont toujours listés. Un texte caché n'apparaît pas même
/// sélectionné : la sélection ne le révèle pas.
///
/// `show_scales` : lister sous chaque calque le groupe « Axes », une ligne par
/// axe (les quatre, y compris ceux masqués par défaut) (#111). Faux par
/// défaut, même motif.
pub fn object_tree_with_selection(
    tree: &Tree,
    graph: NodeId,
    selected: Option<VisibilityTarget>,
    show_texts: bool,
    show_scales: bool,
) -> Option<ObjectTree> {
    let node = tree.node(graph)?;
    let spec = crate::state::current_plot_spec(tree, graph)?;
    let text_row = |target: TextTarget| {
        let target = VisibilityTarget::Text(target);
        ObjectRow {
            target,
            label: match target {
                VisibilityTarget::Text(t) => t
                    .text(&spec)
                    .unwrap_or_else(|| crate::tr!("plot-tab-legend")),
                _ => String::new(),
            },
            kind: ObjectKind::Text,
            selected: selected == Some(target),
        }
    };
    Some(ObjectTree {
        graph,
        title: node.name().to_string(),
        objects: if !show_texts || spec.title.is_empty() {
            Vec::new()
        } else {
            vec![text_row(TextTarget::Title)]
        },
        layers: spec
            .panels
            .iter()
            .enumerate()
            .map(|(index, panel)| {
                let mut objects: Vec<ObjectRow> = if show_texts {
                    panel_texts(&spec, index)
                        .into_iter()
                        .map(&text_row)
                        .collect()
                } else {
                    Vec::new()
                };
                let axes: Vec<ObjectRow> = if !show_scales {
                    Vec::new()
                } else {
                    AXIS_SIDES
                        .into_iter()
                        .map(|side| {
                            let target = VisibilityTarget::Axis(AxisRef {
                                row: panel.row,
                                column: panel.column,
                                side,
                            });
                            ObjectRow {
                                target,
                                label: axis_label(side),
                                kind: ObjectKind::Axis,
                                selected: selected == Some(target),
                            }
                        })
                        .collect()
                };
                objects.extend(panel.annotations.iter().enumerate().map(|(rank, a)| {
                    let target = VisibilityTarget::Annotation(AnnotationRef {
                        row: panel.row,
                        column: panel.column,
                        index: rank,
                    });
                    ObjectRow {
                        target,
                        label: a.text.clone(),
                        kind: ObjectKind::Text,
                        selected: selected == Some(target),
                    }
                }));
                objects.extend(panel.shapes.iter().enumerate().map(|(rank, s)| {
                    let target = VisibilityTarget::Shape(ShapeRef {
                        row: panel.row,
                        column: panel.column,
                        index: rank,
                    });
                    let label = match s.kind {
                        ShapeKind::Rectangle => crate::tr!("tool-rectangle"),
                        ShapeKind::Ellipse => crate::tr!("tool-ellipse"),
                        ShapeKind::Line => crate::tr!("tool-line"),
                        ShapeKind::Arrow => crate::tr!("tool-arrow"),
                    };
                    ObjectRow {
                        target,
                        label,
                        kind: ObjectKind::Shape(s.kind),
                        selected: selected == Some(target),
                    }
                }));
                LayerRow {
                    index,
                    axes,
                    objects,
                    series: panel
                        .series
                        .iter()
                        .enumerate()
                        .map(|(rank, s)| {
                            let target = SeriesRef::new(index, rank);
                            SeriesRow {
                                target,
                                label: series_label(s, tree),
                                spec: s.clone(),
                                selected: selected == Some(VisibilityTarget::Series(target)),
                            }
                        })
                        .collect(),
                }
            })
            .collect(),
    })
}

/// Les décisions pures de la projection, sans fenêtre (venues d'`app.rs` avec
/// #121 ; le contrat bout en bout, annulable et persistant, est dans
/// `tests/shell.rs`).
#[cfg(test)]
mod tests {
    use stokesforge_core::NodeId;
    use stokesforge_plot::{PlotSpec, SeriesRef, SeriesSpec};
    #[test]
    fn supprimer_un_voisin_ne_selectionne_pas_le_suivant() {
        use crate::object_manager::{selection_after_remove, VisibilityTarget as T};
        use stokesforge_plot::{AnnotationRef, ShapeRef};
        let a = AnnotationRef {
            row: 1,
            column: 2,
            index: 3,
        };
        let s = ShapeRef {
            row: 1,
            column: 2,
            index: 3,
        };
        for (selected, removed, expected) in [
            (
                T::Series(SeriesRef::new(1, 3)),
                T::Series(SeriesRef::new(1, 0)),
                Some(T::Series(SeriesRef::new(1, 2))),
            ),
            (
                T::Annotation(a),
                T::Annotation(AnnotationRef { index: 0, ..a }),
                Some(T::Annotation(AnnotationRef { index: 2, ..a })),
            ),
            (
                T::Shape(s),
                T::Shape(ShapeRef { index: 0, ..s }),
                Some(T::Shape(ShapeRef { index: 2, ..s })),
            ),
        ] {
            assert_eq!(selection_after_remove(Some(selected), removed), expected);
            assert_eq!(selection_after_remove(Some(selected), selected), None);
        }
    }

    #[test]
    fn undo_redo_suit_la_serie_reordonnee_et_garde_une_edition_de_style() {
        use crate::object_manager::{
            reorder_series, selection_after_history, VisibilityTarget as T,
        };
        let mut before = PlotSpec::default();
        for name in ["A", "B", "C"] {
            let mut series = SeriesSpec::new(NodeId::from_raw(900));
            series.name = name.into();
            before.panels.get_mut(0).unwrap().series.push(series);
        }
        let mut after = before.clone();
        assert!(reorder_series(&mut after, SeriesRef::new(0, 0), 2));
        assert_eq!(
            selection_after_history(&after, &before, Some(T::Series(SeriesRef::new(0, 2)))),
            Some(T::Series(SeriesRef::new(0, 0)))
        );
        assert_eq!(
            selection_after_history(&before, &after, Some(T::Series(SeriesRef::new(0, 0)))),
            Some(T::Series(SeriesRef::new(0, 2)))
        );
        let mut formatted = after.clone();
        formatted.panels.get_mut(0).unwrap().series[2].line.width = 4.0;
        assert_eq!(
            selection_after_history(&formatted, &after, Some(T::Series(SeriesRef::new(0, 2)))),
            Some(T::Series(SeriesRef::new(0, 2)))
        );
    }
}
