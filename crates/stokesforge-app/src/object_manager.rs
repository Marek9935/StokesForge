//! Projection en lecture du graphe actif pour Object Manager (#81–85).
//!
//! Le panneau ne possède ni contenus ni sélection : le spec vient de
//! l'instantané, la sélection de GraphView. Les parents commandent tous leurs
//! contenus en une écriture du spec ; les axes et la disposition restent en
//! place. Le groupe ne commande que les séries. Un parent mixte se révèle au
//! clic. Les adresses restent locales au spec : un déplacement suit le contenu,
//! une suppression efface la sélection plutôt que de viser son voisin.
//!
//! Axes (#111, #147, #171, #176) : sous chaque calque, un groupe « Axes »
//! réunit une ligne par axe tracé. Il est caché par défaut et listé par sa
//! propre bascule « Show Axes » (même motif que les textes,
//! `object_manager_show_scales`). La case d'un axe commande `AxisSpec::visible`
//! (ligne, graduations et libellés d'un coup) ; pour le haut et la droite d'un
//! calque ordinaire, ce sont des **axes miroirs** (#176) et la case commande
//! `AxisSpec::mirror` — grisée quand un calque superposé occupe ce côté. La
//! case du groupe ([`VisibilityTarget::Axes`]) commande ses axes. Les cases du
//! graphique et du calque ne comptent les axes que **listés** (bascule « Show
//! Axes », [`Shown::scales`]) ; la case du groupe de tracés n'a que ses séries.
//!
//! Vue par défaut (#102) : calques, groupes de tracés et séries, plus les
//! objets rares (formes, textes libres). Les textes automatiques — titre du
//! graphique et des calques, titres d'axes, légende — ne sont listés que si la
//! bascule « Show graph texts » est active (préférence d'interface, hors `.stf`) ;
//! le filtre vit dans [`object_tree_with_selection`]. Les cases du graphique et
//! du calque commandent toujours le contenu complet, textes cachés compris
//! ([`VisibilityTarget::contents`]) ; l'état d'une case parente et son
//! basculement ne comptent que les enfants **affichés** dans l'arbre (#173,
//! [`VisibilityTarget::contents_shown`]).

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

/// Un axe d'un calque, adressé par l'indice du calque dans `PlotSpec::panels`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AxisRef {
    pub panel: usize,
    pub side: AxisSide,
}

impl AxisRef {
    fn get(self, spec: &PlotSpec) -> Option<&stokesforge_plot::AxisSpec> {
        spec.panels.get(self.panel).map(|p| p.axis(self.side))
    }
    /// L'axe est un miroir (#176) : le haut d'un calque ordinaire, ou sa droite
    /// quand aucune série n'y est rattachée. Sa case commande alors
    /// `AxisSpec::mirror`, et non `visible`.
    fn is_mirror(self, spec: &PlotSpec) -> bool {
        spec.panels.get(self.panel).is_some_and(|panel| {
            panel.overlay.is_none()
                && match self.side {
                    AxisSide::Top => true,
                    AxisSide::Right => !panel.uses_right_axis(),
                    _ => false,
                }
        })
    }
    /// Un miroir que la figure ignore : un calque superposé de la même case a
    /// déjà son propre axe de ce côté (#176). Sa case est grisée.
    fn blocked(self, spec: &PlotSpec) -> bool {
        let Some(panel) = spec.panels.get(self.panel) else {
            return false;
        };
        self.is_mirror(spec)
            && spec.panels.iter().any(|other| {
                (other.row, other.column) == (panel.row, panel.column)
                    && other.overlay.is_some_and(|overlay| match self.side {
                        AxisSide::Top => overlay.x_top && other.bottom.visible,
                        _ => overlay.y_right && other.left.visible,
                    })
            })
    }
    /// La valeur de la case : le miroir ou la visibilité de l'axe.
    fn shown(self, spec: &PlotSpec) -> Option<bool> {
        let axis = self.get(spec)?;
        Some(if self.is_mirror(spec) {
            axis.mirror
        } else {
            axis.visible
        })
    }
    fn set_shown(self, spec: &mut PlotSpec, shown: bool) {
        let mirror = self.is_mirror(spec);
        if let Some(axis) = self.get_mut(spec) {
            if mirror {
                axis.mirror = shown;
            } else {
                axis.visible = shown;
            }
        }
    }
    fn get_mut(self, spec: &mut PlotSpec) -> Option<&mut stokesforge_plot::AxisSpec> {
        spec.panels
            .get_mut(self.panel)
            .map(|p| p.axis_mut(self.side))
    }
}

/// Les quatre axes d'un calque ordinaire, dans l'ordre d'affichage de l'arbre.
pub const AXIS_SIDES: [AxisSide; 4] = [
    AxisSide::Bottom,
    AxisSide::Left,
    AxisSide::Right,
    AxisSide::Top,
];

/// Les axes qu'un calque affiche vraiment, donc ceux que l'arbre liste (#171).
/// Un axe que la figure ne trace pas n'a pas de ligne : sa case serait sans effet.
///
/// Un calque superposé range son axe X dans `bottom` et son axe Y dans
/// `left`, mais les affiche en haut et à droite, et seulement s'il a une
/// échelle propre (`x_top` / `y_right`) : ses autres lignes seraient mortes.
pub fn axis_sides(panel: &stokesforge_plot::PanelSpec) -> Vec<AxisSide> {
    match panel.overlay {
        Some(overlay) => [
            (overlay.x_top, AxisSide::Bottom),
            (overlay.y_right, AxisSide::Left),
        ]
        .into_iter()
        .filter_map(|(shown, side)| shown.then_some(side))
        .collect(),
        // Haut et droite : un axe miroir de celui du bas / de gauche (#176), ou,
        // à droite, le vrai axe des séries qui y sont rattachées.
        None => AXIS_SIDES.to_vec(),
    }
}
/// Ce que l'arbre affiche, donc ce que comptent les cases parentes (#173) :
/// les textes automatiques et le groupe « Axes » sont listés par leurs bascules.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Shown {
    pub texts: bool,
    pub scales: bool,
}

impl Shown {
    /// Les textes seuls (feuilles : la valeur n'importe pas).
    pub const TEXTS: Self = Self {
        texts: true,
        scales: false,
    };
    /// Ni textes ni axes.
    pub const NONE: Self = Self {
        texts: false,
        scales: false,
    };
}

impl From<crate::preferences::ObjectManagerSettings> for Shown {
    fn from(settings: crate::preferences::ObjectManagerSettings) -> Self {
        Self {
            texts: settings.show_texts,
            scales: settings.show_scales,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Visibility {
    pub checked: bool,
    pub mixed: bool,
    pub enabled: bool,
}

impl VisibilityTarget {
    /// Feuilles concernées ; un titre vide ne rend pas un parent indéterminé.
    pub fn contents(self, spec: &PlotSpec) -> Vec<Self> {
        self.contents_shown(spec, Shown::TEXTS)
    }
    /// Feuilles que l'arbre **affiche** sous ce parent (#173) : sans les
    /// textes automatiques tant que `show_texts` est faux, un texte caché
    /// ne compte ni dans l'état de la case ni dans son basculement.
    pub fn contents_shown(self, spec: &PlotSpec, shown: Shown) -> Vec<Self> {
        if let Self::Axes(index) = self {
            let Some(panel) = spec.panels.get(index) else {
                return Vec::new();
            };
            return axis_sides(panel)
                .into_iter()
                .map(|side| Self::Axis(AxisRef { panel: index, side }))
                .collect();
        }
        if !matches!(self, Self::Graph | Self::Panel(_) | Self::Group(_)) {
            return vec![self];
        }
        let mut leaves = Vec::new();
        if self == Self::Graph && shown.texts && !spec.title.is_empty() {
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
            if shown.scales {
                leaves.extend(Self::Axes(index).contents_shown(spec, shown));
            }
            if shown.texts {
                leaves.extend(panel_texts(spec, index).into_iter().map(Self::Text));
            }
            leaves.extend(panel.annotations.iter().enumerate().map(|(rank, _)| {
                Self::Annotation(AnnotationRef {
                    panel: index,
                    index: rank,
                })
            }));
            leaves.extend(panel.shapes.iter().enumerate().map(|(rank, _)| {
                Self::Shape(ShapeRef {
                    panel: index,
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
            Self::Axis(t) => t.shown(spec),
            Self::Graph | Self::Panel(_) | Self::Group(_) | Self::Axes(_) => None,
        }
    }
    pub fn visibility(self, spec: &PlotSpec) -> Visibility {
        self.visibility_shown(spec, Shown::TEXTS)
    }
    /// État de la case, d'après les seuls enfants affichés (#173).
    pub fn visibility_shown(self, spec: &PlotSpec, shown: Shown) -> Visibility {
        let values: Vec<bool> = self
            .contents_shown(spec, shown)
            .into_iter()
            .filter_map(|t| t.value(spec))
            .collect();
        let blocked = matches!(self, Self::Axis(axis) if axis.blocked(spec));
        Visibility {
            checked: !values.is_empty() && values.iter().all(|v| *v),
            mixed: values.iter().any(|v| *v) && values.iter().any(|v| !v),
            enabled: !values.is_empty() && !blocked,
        }
    }
    /// Cible périmée ou valeur identique : aucune entrée d'historique.
    pub fn set(self, spec: &mut PlotSpec, visible: bool) -> bool {
        self.set_shown(spec, visible, Shown::TEXTS)
    }
    /// Comme [`Self::set`], sur les seuls enfants affichés (#173) : un texte
    /// caché garde son état.
    pub fn set_shown(self, spec: &mut PlotSpec, visible: bool, shown: Shown) -> bool {
        let mut changed = false;
        for leaf in self.contents_shown(spec, shown) {
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
                Self::Axis(t) => t.set_shown(spec, visible),
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
        (T::Annotation(mut s), T::Annotation(r)) if s.panel == r.panel && s.index > r.index => {
            s.index -= 1;
            T::Annotation(s)
        }
        (T::Shape(mut s), T::Shape(r)) if s.panel == r.panel && s.index > r.index => {
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
            let old = previous.panels.get(t.panel)?;
            let new = next.panels.get(t.panel)?;
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
            let old = previous.panels.get(t.panel)?;
            let new = next.panels.get(t.panel)?;
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
    let mut texts = Vec::new();
    if !panel.title.is_empty() {
        texts.push(TextTarget::PanelTitle { panel: index });
    }
    // Le modèle réserve aussi Top ; le pipeline ne trace encore que ces
    // trois axes. Ne pas proposer un titre sans composant à éditer.
    for side in [AxisSide::Bottom, AxisSide::Left, AxisSide::Right] {
        if !panel.axis(side).display_label().is_empty() {
            texts.push(TextTarget::AxisTitle { panel: index, side });
        }
    }
    if !panel.series.is_empty() {
        texts.push(TextTarget::Legend { panel: index });
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

/// Le nom d'un axe **à l'endroit où il est affiché** : l'axe X d'un calque
/// superposé est en haut, son axe Y à droite (#171).
fn axis_label(panel: &stokesforge_plot::PanelSpec, side: AxisSide) -> String {
    if panel.overlay.is_some() {
        let axis = match side {
            AxisSide::Left => crate::tr!("plot-axis-overlay-y"),
            _ => crate::tr!("plot-axis-overlay-x"),
        };
        return crate::tr!("plot-target-axis", axis = axis);
    }
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
                    axis_sides(panel)
                        .into_iter()
                        .map(|side| {
                            let target = VisibilityTarget::Axis(AxisRef { panel: index, side });
                            ObjectRow {
                                target,
                                label: axis_label(panel, side),
                                kind: ObjectKind::Axis,
                                selected: selected == Some(target),
                            }
                        })
                        .collect()
                };
                objects.extend(panel.annotations.iter().enumerate().map(|(rank, a)| {
                    let target = VisibilityTarget::Annotation(AnnotationRef {
                        panel: index,
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
                        panel: index,
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

    /// #171 : un calque superposé ne liste que les axes qu'il affiche, nommés
    /// là où ils sont affichés ; la case de son groupe « Axes » ne commande
    /// que ceux-là.
    #[test]
    fn un_calque_superpose_liste_ses_axes_la_ou_ils_sont_affiches() {
        use crate::object_manager::{axis_label, axis_sides, VisibilityTarget as T};
        use stokesforge_plot::{AxisSide, OverlaySpec, PanelSpec, Panels};
        let base = PanelSpec::at(0, 0);
        let both = PanelSpec::overlay_on(
            &base,
            OverlaySpec {
                x_top: true,
                y_right: true,
            },
        );
        let only_y = PanelSpec::overlay_on(
            &base,
            OverlaySpec {
                x_top: false,
                y_right: true,
            },
        );
        let linked = PanelSpec::overlay_on(&base, OverlaySpec::default());
        assert_eq!(axis_sides(&base), crate::object_manager::AXIS_SIDES);
        assert_eq!(axis_sides(&both), [AxisSide::Bottom, AxisSide::Left]);
        assert_eq!(axis_sides(&only_y), [AxisSide::Left]);
        assert!(axis_sides(&linked).is_empty());
        let (x, y) = (
            axis_label(&both, AxisSide::Bottom),
            axis_label(&both, AxisSide::Left),
        );
        assert_ne!(x, axis_label(&base, AxisSide::Bottom));
        assert_ne!(x, y);
        let spec = PlotSpec::empty("Test").with_panels(Panels::new(vec![base, both]));
        assert_eq!(T::Axes(0).contents(&spec).len(), 4);
        assert_eq!(T::Axes(1).contents(&spec).len(), 2);
    }
    /// #176 : les lignes haut et droite d'un calque ordinaire commandent le
    /// miroir ; un calque superposé qui occupe le côté grise la case.
    #[test]
    fn les_cases_haut_et_droite_commandent_les_miroirs() {
        use crate::object_manager::{AxisRef, VisibilityTarget as T};
        use stokesforge_plot::{AxisSide, OverlaySpec, PanelSpec, Panels};
        let base = PanelSpec::at(0, 0);
        let top = T::Axis(AxisRef {
            panel: 0,
            side: AxisSide::Top,
        });
        let right = T::Axis(AxisRef {
            panel: 0,
            side: AxisSide::Right,
        });
        let mut spec = PlotSpec::empty("Test").with_panels(Panels::new(vec![base.clone()]));
        assert!(!top.visibility(&spec).checked && top.visibility(&spec).enabled);
        assert!(top.set(&mut spec, true) && right.set(&mut spec, true));
        let panel = &spec.panels[0];
        assert!(panel.top.mirror && panel.right.mirror);
        assert!(
            !panel.top.visible,
            "la visibilité de l'axe n'est pas touchée"
        );
        assert!(panel.mirrors_top() && panel.mirrors_right());
        assert!(top.visibility(&spec).checked);
        // Un calque superposé aux axes propres prime : cases grisées.
        let haut = PanelSpec::overlay_on(
            &base,
            OverlaySpec {
                x_top: true,
                y_right: false,
            },
        );
        spec.panels = Panels::new(vec![spec.panels[0].clone(), haut]);
        assert!(!top.visibility(&spec).enabled);
        assert!(right.visibility(&spec).enabled);
    }

    /// Axes listés (« Show Axes ») : les cases du calque et du graphique les
    /// comptent et les commandent, comme tout enfant affiché ; le groupe de
    /// tracés, lui, n'a que ses séries.
    #[test]
    fn les_axes_listes_comptent_pour_le_calque_et_le_graphique() {
        use crate::object_manager::{Shown, VisibilityTarget as T};
        use stokesforge_plot::{PanelSpec, Panels};
        let mut base = PanelSpec::at(0, 0);
        base.series.push(SeriesSpec::new(NodeId::from_raw(900)));
        let mut spec = PlotSpec::empty("T").with_panels(Panels::new(vec![base]));
        let axes = Shown {
            texts: false,
            scales: true,
        };
        // Série décochée, axes cochés : le calque et le graphique sont mixtes.
        assert!(T::Group(0).set_shown(&mut spec, false, axes));
        assert!(T::Group(0).visibility_shown(&spec, axes).enabled);
        for parent in [T::Panel(0), T::Graph] {
            assert!(parent.visibility_shown(&spec, axes).mixed, "{parent:?}");
            assert!(!parent.visibility_shown(&spec, Shown::NONE).mixed);
        }
        // Un clic sur le calque ne laisse pas les axes seuls cochés.
        assert!(T::Panel(0).set_shown(&mut spec, false, axes));
        assert!(!spec.panels[0].bottom.visible && !spec.panels[0].left.visible);
        assert!(!T::Panel(0).visibility_shown(&spec, axes).mixed);
    }

    /// #173 : un texte automatique caché dans l'arbre ne rend pas son parent
    /// indéterminé et n'est pas basculé par un clic sur ce parent.
    #[test]
    fn un_parent_ne_tient_compte_que_des_enfants_affiches() {
        use crate::object_manager::{Shown, VisibilityTarget as T};
        use stokesforge_plot::{OverlaySpec, PanelSpec, Panels, TextTarget};
        let mut base = PanelSpec::at(0, 0);
        base.series.push(SeriesSpec::new(NodeId::from_raw(900)));
        let mut haut = PanelSpec::overlay_on(&base, OverlaySpec::default());
        haut.series.push(SeriesSpec::new(NodeId::from_raw(901)));
        let mut spec = PlotSpec::empty("Test").with_panels(Panels::new(vec![base, haut]));
        spec.title = String::new();
        // La légende du calque 2 est masquée à la création.
        let legende = T::Text(TextTarget::Legend { panel: 1 });
        assert_eq!(legende.value(&spec), Some(false));
        for parent in [T::Graph, T::Panel(1)] {
            let tout = parent.visibility_shown(&spec, Shown::TEXTS);
            assert!(tout.mixed, "avec les textes, la légende cachée compte");
            let affiche = parent.visibility_shown(&spec, Shown::NONE);
            assert!(affiche.checked && !affiche.mixed, "{parent:?}");
        }
        // Décocher le parent laisse le texte caché dans son état.
        assert!(T::Panel(1).set_shown(&mut spec, false, Shown::NONE));
        assert_eq!(legende.value(&spec), Some(false));
        assert!(T::Panel(1).set_shown(&mut spec, true, Shown::NONE));
        assert_eq!(legende.value(&spec), Some(false));
        assert!(T::Panel(1).set_shown(&mut spec, true, Shown::TEXTS));
        assert_eq!(legende.value(&spec), Some(true));
    }

    #[test]
    fn supprimer_un_voisin_ne_selectionne_pas_le_suivant() {
        use crate::object_manager::{selection_after_remove, VisibilityTarget as T};
        use stokesforge_plot::{AnnotationRef, ShapeRef};
        let a = AnnotationRef { panel: 1, index: 3 };
        let s = ShapeRef { panel: 1, index: 3 };
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
