//! Read-only projection of the active graph for Object Manager (#81–85).
//!
//! The panel owns neither contents nor selection: the spec comes from the
//! snapshot, the selection from GraphView. Parents control all their contents
//! in a single write of the spec; the axes and the layout stay in place. The
//! group controls only the series. A mixed parent reveals itself on click.
//! Addresses stay local to the spec: a move follows the content, a deletion
//! clears the selection rather than aiming at its neighbor.
//!
//! Axes (#111, #147, #171, #176): under each layer, an "Axes" group gathers
//! one row per plotted axis. It is hidden by default and listed by its own
//! "Show Axes" toggle (same pattern as the texts, `object_manager_show_scales`).
//! An axis's checkbox controls `AxisSpec::visible` (line, ticks and labels at
//! once); for the top and the right of an ordinary layer, these are **mirror
//! axes** (#176) and the checkbox controls `AxisSpec::mirror` — disabled when
//! an overlaid layer occupies that side. The group's checkbox
//! ([`VisibilityTarget::Axes`]) controls its axes. The graph's and the layer's
//! checkboxes count axes only when **listed** ("Show Axes" toggle,
//! [`Shown::scales`]); the plot group's checkbox has only its series.
//!
//! Default view (#102, #214): layers, plot groups and series, plus the free
//! objects (shapes, free texts). Linked texts — graph and layer titles, axis
//! titles, legend — are never listed: they are edited by double-click or in
//! the properties. The state of a parent checkbox and its toggling count only
//! the children **shown** in the tree (#173,
//! [`VisibilityTarget::contents_shown`]).

use crate::cell_sides::{displayed_place, side_use};
use stokesforge_core::{NodeId, Tree};
use stokesforge_plot::{
    AnnotationRef, AxisSide, PanelSpec, PlotSpec, SeriesGroupRef, SeriesRef, SeriesSlot,
    SeriesSpec, ShapeKind, ShapeRef, TextTarget,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VisibilityTarget {
    Graph,
    Panel(usize),
    Group(usize),
    /// A series group (#197): a parent like the others, whose checkbox
    /// controls only its members.
    SeriesGroup(SeriesGroupRef),
    Series(SeriesRef),
    Text(TextTarget),
    Annotation(AnnotationRef),
    Shape(ShapeRef),
    /// A scale (axis line, ticks and labels): `AxisSpec::visible`.
    Axis(AxisRef),
    /// The group of a layer's four axes: it controls only those axes.
    Axes(usize),
}

/// An axis of a layer, addressed by the layer's index in `PlotSpec::panels`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AxisRef {
    pub panel: usize,
    pub side: AxisSide,
}

impl AxisRef {
    fn get(self, spec: &PlotSpec) -> Option<&stokesforge_plot::AxisSpec> {
        spec.panels.get(self.panel).map(|p| p.axis(self.side))
    }
    /// The axis is a mirror (#176): the top of an ordinary layer, or its right
    /// when no series is attached to it. Its checkbox then controls
    /// `AxisSpec::mirror`, not `visible`.
    fn is_mirror(self, spec: &PlotSpec) -> bool {
        side_use(spec, self.panel, self.side).mirror
    }
    /// A mirror that the figure ignores: an overlaid layer of the same cell
    /// already has its own axis on that side (#176). Its checkbox is disabled.
    /// The question "who holds this side of the cell" is answered by
    /// [`crate::cell_sides`], the same as for the tabs of Graph Properties.
    fn blocked(self, spec: &PlotSpec) -> bool {
        side_use(spec, self.panel, self.side).held_by.is_some()
    }
    /// The checkbox value: the mirror or the axis's visibility.
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

/// The four axes of an ordinary layer, in the tree's display order.
pub const AXIS_SIDES: [AxisSide; 4] = [
    AxisSide::Bottom,
    AxisSide::Left,
    AxisSide::Right,
    AxisSide::Top,
];

/// The axes that a layer really displays, hence those the tree lists (#171).
/// An axis that the figure does not draw has no row: its checkbox would have
/// no effect.
///
/// An overlaid layer stores its X axis in `bottom` and its Y axis in
/// `left`, but displays them at the top and on the right, and only if it has
/// its own scale (`x_top` / `y_right`): its other rows would be dead.
pub fn axis_sides(panel: &stokesforge_plot::PanelSpec) -> Vec<AxisSide> {
    match panel.overlay {
        Some(overlay) => [
            (overlay.x_top, AxisSide::Bottom),
            (overlay.y_right, AxisSide::Left),
        ]
        .into_iter()
        .filter_map(|(shown, side)| shown.then_some(side))
        .collect(),
        // Top and right: a mirror axis of the bottom / left one (#176), or,
        // on the right, the real axis of the series attached to it.
        None => AXIS_SIDES.to_vec(),
    }
}
/// What the tree displays, hence what the parent checkboxes count (#173):
/// the "Axes" group is listed by its toggle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Shown {
    pub scales: bool,
    pub objects: bool,
}

impl Shown {
    /// Free objects listed, axes not (leaves: the value does not matter).
    pub const BASE: Self = Self {
        scales: false,
        objects: true,
    };
}

impl From<crate::preferences::ObjectManagerSettings> for Shown {
    fn from(settings: crate::preferences::ObjectManagerSettings) -> Self {
        Self {
            scales: settings.show_scales,
            objects: settings.show_objects,
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
    /// Leaves concerned; an empty title does not make a parent indeterminate.
    pub fn contents(self, spec: &PlotSpec) -> Vec<Self> {
        self.contents_shown(spec, Shown::BASE)
    }
    /// Leaves that the tree **displays** under this parent (#173): without the
    /// linked texts (#214) or the unlisted axes, which count neither in the
    /// checkbox's state nor in its toggling.
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
        if let Self::SeriesGroup(group) = self {
            return spec
                .panels
                .get(group.panel)
                .map(|panel| panel.group_members(group.group))
                .unwrap_or_default()
                .into_iter()
                .map(|rank| Self::Series(SeriesRef::new(group.panel, rank)))
                .collect();
        }
        if !matches!(self, Self::Graph | Self::Panel(_) | Self::Group(_)) {
            return vec![self];
        }
        let mut leaves = Vec::new();
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
            if !shown.objects {
                continue;
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
            Self::Graph
            | Self::Panel(_)
            | Self::Group(_)
            | Self::SeriesGroup(_)
            | Self::Axes(_) => None,
        }
    }
    /// True for the mirror axis that a layer above already draws (#176): the
    /// reason for its disabled checkbox (#226).
    pub fn is_blocked_mirror(self, spec: &PlotSpec) -> bool {
        matches!(self, Self::Axis(axis) if axis.blocked(spec))
    }
    pub fn visibility(self, spec: &PlotSpec) -> Visibility {
        self.visibility_shown(spec, Shown::BASE)
    }
    /// State of the checkbox, from the displayed children only (#173).
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
    /// Stale target or identical value: no history entry.
    pub fn set(self, spec: &mut PlotSpec, visible: bool) -> bool {
        self.set_shown(spec, visible, Shown::BASE)
    }
    /// Like [`Self::set`], on the displayed children only (#173): a hidden
    /// text keeps its state.
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
                Self::Graph
                | Self::Panel(_)
                | Self::Group(_)
                | Self::SeriesGroup(_)
                | Self::Axes(_) => {}
            }
        }
        changed
    }
    pub fn exists(self, spec: &PlotSpec) -> bool {
        match self {
            Self::Graph => true,
            Self::Panel(i) | Self::Group(i) | Self::Axes(i) => spec.panels.get(i).is_some(),
            Self::SeriesGroup(group) => spec
                .panels
                .get(group.panel)
                .is_some_and(|panel| !panel.group_members(group.group).is_empty()),
            _ => self.value(spec).is_some(),
        }
    }
    pub fn removable(self, spec: &PlotSpec) -> bool {
        if let Self::Panel(panel) = self {
            // A cell with its overlays, or an overlay alone (#357); never the last cell.
            return stokesforge_core::compose::can_remove_layer(spec, panel);
        }
        self.exists(spec)
            && matches!(
                self,
                Self::Series(_)
                    | Self::SeriesGroup(_)
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

impl VisibilityTarget {
    /// The same target after layers were deleted or came back (#357):
    /// `map[old]` is the new index of layer `old`. `None` if the target sat on
    /// a layer that is gone. The graph itself has no layer: unchanged.
    pub fn remap_panel(self, map: &[Option<usize>]) -> Option<Self> {
        use VisibilityTarget as T;
        let to = |panel: usize| map.get(panel).copied().flatten();
        Some(match self {
            T::Graph => T::Graph,
            T::Panel(p) => T::Panel(to(p)?),
            T::Group(p) => T::Group(to(p)?),
            T::Axes(p) => T::Axes(to(p)?),
            T::SeriesGroup(g) => T::SeriesGroup(SeriesGroupRef {
                panel: to(g.panel)?,
                ..g
            }),
            T::Series(s) => T::Series(SeriesRef {
                panel: to(s.panel)?,
                ..s
            }),
            T::Annotation(a) => T::Annotation(AnnotationRef {
                panel: to(a.panel)?,
                ..a
            }),
            T::Shape(s) => T::Shape(ShapeRef {
                panel: to(s.panel)?,
                ..s
            }),
            T::Axis(a) => T::Axis(AxisRef {
                panel: to(a.panel)?,
                ..a
            }),
            T::Text(t) => T::Text(match t {
                TextTarget::Title => TextTarget::Title,
                TextTarget::PanelTitle { panel } => TextTarget::PanelTitle { panel: to(panel)? },
                TextTarget::AxisTitle { panel, side } => TextTarget::AxisTitle {
                    panel: to(panel)?,
                    side,
                },
                TextTarget::Legend { panel } => TextTarget::Legend { panel: to(panel)? },
            }),
        })
    }
}

/// Rank addresses follow the content after a neighbor is deleted.
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

/// Undo/redo can change ranks without going through the interface gesture.
/// Finding the exact content makes it possible to follow a permutation; a
/// style or text edit keeps the address when the number of contents stays the
/// same. With no match after a deletion, the selection is cleared.
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

#[derive(Debug, Clone)]
pub struct SeriesRow {
    pub target: SeriesRef,
    pub label: String,
    pub spec: SeriesSpec,
    pub selected: bool,
    /// A column of the series comes from an out-of-date calculation and the
    /// graph wants the warning (#198).
    pub stale: bool,
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
/// A series group listed under a layer (#197).
#[derive(Debug, Clone)]
pub struct GroupRow {
    pub target: SeriesGroupRef,
    /// The displayed name: the one given, otherwise "Group n".
    pub name: String,
    /// Ranks of the members in [`LayerRow::series`].
    pub members: Vec<usize>,
    pub selected: bool,
}
/// A slot of a layer's series list: the projection shared by the Object
/// Manager and the series window (#197).
#[derive(Debug, Clone)]
pub enum LayerSlot {
    /// Rank of a series outside any group in [`LayerRow::series`].
    Single(usize),
    Group(GroupRow),
}
#[derive(Debug, Clone)]
pub struct LayerRow {
    pub index: usize,
    pub series: Vec<SeriesRow>,
    /// The series arranged in slots: a lone series or a group and all its
    /// members (`PanelSpec::slots`).
    pub slots: Vec<LayerSlot>,
    /// The rows of the "Axes" group (#111); empty without the toggle.
    pub axes: Vec<ObjectRow>,
    pub objects: Vec<ObjectRow>,
}
#[derive(Debug, Clone)]
pub struct ObjectTree {
    pub graph: NodeId,
    pub title: String,
    pub layers: Vec<LayerRow>,
}

/// The name of an axis **where it is displayed**: the X axis of an overlaid
/// layer is at the top, its Y axis on the right (#171).
fn axis_label(panel: &stokesforge_plot::PanelSpec, side: AxisSide) -> String {
    let place = if panel.overlay.is_some() {
        displayed_place(side)
    } else {
        side
    };
    let place = match place {
        AxisSide::Bottom => "bottom",
        AxisSide::Left => "left",
        AxisSide::Right => "right",
        AxisSide::Top => "top",
    };
    crate::tr!("plot-target-axis", side = place)
}

/// The displayed name of a series group (#197): the one given to it, otherwise
/// "Group n". Everywhere a group carries a name (series list, Object Manager,
/// membership choice), it comes from here.
pub fn group_title(panel: &PanelSpec, group: u32) -> String {
    panel.group_label(group).map_or_else(
        || crate::tr!("curve-group-name", number = group),
        str::to_string,
    )
}

pub fn object_tree(tree: &Tree, graph: NodeId, selected: Option<SeriesRef>) -> Option<ObjectTree> {
    object_tree_with_selection(
        tree,
        graph,
        selected.map(VisibilityTarget::Series),
        false,
        true,
    )
}
/// Linked texts (graph and layer titles, axis titles, legend) are never listed
/// (#214): they are edited by double-click on the figure or in the properties
/// window. The "Objects" group receives only the free objects (free texts,
/// shapes), and only with `show_objects`.
///
/// `show_scales`: list the "Axes" group under each layer, one row per axis
/// (all four, including those hidden by default) (#111). False by default,
/// same pattern.
pub fn object_tree_with_selection(
    tree: &Tree,
    graph: NodeId,
    selected: Option<VisibilityTarget>,
    show_scales: bool,
    show_objects: bool,
) -> Option<ObjectTree> {
    let node = tree.node(graph)?;
    let spec = stokesforge_core::compose::current_plot_spec(tree, graph)?;
    Some(object_tree_of_spec(
        tree,
        graph,
        node.name(),
        &spec,
        selected,
        show_scales,
        show_objects,
    ))
}

/// [`object_tree_with_selection`] for a given spec: the preview of a drag and
/// drop in progress is painted without the model having changed.
pub fn object_tree_of_spec(
    tree: &Tree,
    graph: NodeId,
    title: &str,
    spec: &PlotSpec,
    selected: Option<VisibilityTarget>,
    show_scales: bool,
    show_objects: bool,
) -> ObjectTree {
    ObjectTree {
        graph,
        title: title.to_string(),
        layers: spec
            .panels
            .iter()
            .enumerate()
            .map(|(index, panel)| {
                let mut objects: Vec<ObjectRow> = Vec::new();
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
                let free = if show_objects {
                    panel.annotations.len()
                } else {
                    0
                };
                objects.extend(
                    panel
                        .annotations
                        .iter()
                        .take(free)
                        .enumerate()
                        .map(|(rank, a)| {
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
                        }),
                );
                let free = if show_objects { panel.shapes.len() } else { 0 };
                objects.extend(panel.shapes.iter().take(free).enumerate().map(|(rank, s)| {
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
                    slots: panel
                        .slots()
                        .into_iter()
                        .map(|slot| match slot {
                            SeriesSlot::Single(rank) => LayerSlot::Single(rank),
                            SeriesSlot::Group { id, members } => {
                                let target = SeriesGroupRef::new(index, id);
                                LayerSlot::Group(GroupRow {
                                    target,
                                    name: group_title(panel, id),
                                    members,
                                    selected: selected
                                        == Some(VisibilityTarget::SeriesGroup(target)),
                                })
                            }
                        })
                        .collect(),
                    series: panel
                        .series
                        .iter()
                        .enumerate()
                        .map(|(rank, s)| {
                            let target = SeriesRef::new(index, rank);
                            SeriesRow {
                                target,
                                label: crate::state::series_list_label(tree, s),
                                spec: s.clone(),
                                selected: selected == Some(VisibilityTarget::Series(target)),
                                stale: spec.stale_warning
                                    && stokesforge_core::resolve_series(tree, s).stale,
                            }
                        })
                        .collect(),
                }
            })
            .collect(),
    }
}

/// The pure decisions of the projection, without a window (came from `app.rs`
/// with #121; the end-to-end contract, undoable and persistent, is in
/// `tests/shell.rs`).
#[cfg(test)]
mod tests {
    use stokesforge_core::NodeId;
    use stokesforge_plot::{PlotSpec, SeriesRef, SeriesSpec};

    /// #357: a layer node is removable when another cell remains, or when it is
    /// an overlay; the last cell is not. Targets follow the layers that move.
    #[test]
    fn a_layer_is_removable_unless_it_is_the_last_cell_and_targets_follow_a_removal() {
        use crate::object_manager::VisibilityTarget as T;
        use stokesforge_plot::{OverlaySpec, PanelSpec, Panels};
        let one = PlotSpec::default();
        assert!(!T::Panel(0).removable(&one), "the last cell");
        assert!(!T::Panel(5).removable(&one), "no such layer");

        let base = PanelSpec::at(0, 0);
        let with_overlay = PlotSpec {
            panels: Panels::new(vec![
                base.clone(),
                PanelSpec::overlay_on(&base, OverlaySpec::TOP_X),
            ]),
            ..PlotSpec::default()
        };
        assert!(!T::Panel(0).removable(&with_overlay), "still one cell");
        assert!(T::Panel(1).removable(&with_overlay), "an overlay");

        let two = PlotSpec {
            panels: Panels::new(vec![PanelSpec::at(0, 0), PanelSpec::at(0, 1)]),
            ..PlotSpec::default()
        };
        assert!(T::Panel(0).removable(&two));
        assert!(T::Panel(1).removable(&two));

        // Layer 0 and its overlay (1) are removed from [0, 1, 2]: layer 2 becomes 0.
        let map = [None, None, Some(0)];
        let series = SeriesRef::new(2, 3);
        assert_eq!(
            T::Series(series).remap_panel(&map),
            Some(T::Series(SeriesRef::new(0, 3)))
        );
        assert_eq!(T::Series(SeriesRef::new(1, 0)).remap_panel(&map), None);
        assert_eq!(T::Graph.remap_panel(&map), Some(T::Graph));
    }

    /// #171: an overlaid layer lists only the axes it displays, named where
    /// they are displayed; the checkbox of its "Axes" group controls only
    /// those.
    #[test]
    fn an_overlaid_layer_lists_its_axes_where_they_are_displayed() {
        use crate::object_manager::{axis_label, axis_sides, VisibilityTarget as T};
        use stokesforge_plot::{AxisSide, OverlaySpec, PanelSpec, Panels};
        let base = PanelSpec::at(0, 0);
        let both = PanelSpec::overlay_on(&base, OverlaySpec::new(true, true));
        let only_y = PanelSpec::overlay_on(&base, OverlaySpec::new(false, true));
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
    /// #176: the top and right rows of an ordinary layer control the mirror;
    /// an overlaid layer that occupies the side disables the checkbox.
    #[test]
    fn the_top_and_right_checkboxes_control_the_mirrors() {
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
        assert!(!panel.top.visible, "the axis's visibility is not touched");
        assert!(panel.mirrors_top() && panel.mirrors_right());
        assert!(top.visibility(&spec).checked);
        // An overlaid layer with its own axes takes precedence: checkboxes disabled.
        let upper = PanelSpec::overlay_on(&base, OverlaySpec::new(true, false));
        spec.panels = Panels::new(vec![spec.panels[0].clone(), upper]);
        assert!(!top.visibility(&spec).enabled);
        assert!(right.visibility(&spec).enabled);
    }

    /// Listed axes ("Show Axes"): the layer's and the graph's checkboxes count
    /// and control them, like any displayed child; the plot group, for its
    /// part, has only its series.
    #[test]
    fn listed_axes_count_for_the_layer_and_the_graph() {
        use crate::object_manager::{Shown, VisibilityTarget as T};
        use stokesforge_plot::{PanelSpec, Panels};
        let mut base = PanelSpec::at(0, 0);
        base.series.push(SeriesSpec::new(NodeId::from_raw(900)));
        let mut spec = PlotSpec::empty("T").with_panels(Panels::new(vec![base]));
        let axes = Shown {
            scales: true,
            ..Shown::BASE
        };
        // Series unchecked, axes checked: the layer and the graph are mixed.
        assert!(T::Group(0).set_shown(&mut spec, false, axes));
        assert!(T::Group(0).visibility_shown(&spec, axes).enabled);
        for parent in [T::Panel(0), T::Graph] {
            assert!(parent.visibility_shown(&spec, axes).mixed, "{parent:?}");
            assert!(!parent.visibility_shown(&spec, Shown::BASE).mixed);
        }
        // A click on the layer does not leave the axes alone checked.
        assert!(T::Panel(0).set_shown(&mut spec, false, axes));
        assert!(!spec.panels[0].bottom.visible && !spec.panels[0].left.visible);
        assert!(!T::Panel(0).visibility_shown(&spec, axes).mixed);
    }

    /// #173, #214: a linked text is never listed; it does not make its parent
    /// indeterminate and a click on that parent does not toggle it.
    #[test]
    fn a_parent_only_accounts_for_displayed_children() {
        use crate::object_manager::{Shown, VisibilityTarget as T};
        use stokesforge_plot::{OverlaySpec, PanelSpec, Panels, TextTarget};
        let mut base = PanelSpec::at(0, 0);
        base.series.push(SeriesSpec::new(NodeId::from_raw(900)));
        let mut upper = PanelSpec::overlay_on(&base, OverlaySpec::default());
        upper.series.push(SeriesSpec::new(NodeId::from_raw(901)));
        let mut spec = PlotSpec::empty("Test").with_panels(Panels::new(vec![base, upper]));
        spec.title = String::new();
        // The legend of layer 2 is hidden at creation.
        let legend = T::Text(TextTarget::Legend { panel: 1 });
        assert_eq!(legend.value(&spec), Some(false));
        for parent in [T::Graph, T::Panel(1)] {
            let shown = parent.visibility_shown(&spec, Shown::BASE);
            assert!(shown.checked && !shown.mixed, "{parent:?}");
        }
        // Unchecking then rechecking the parent leaves the text in its state.
        assert!(T::Panel(1).set_shown(&mut spec, false, Shown::BASE));
        assert_eq!(legend.value(&spec), Some(false));
        assert!(T::Panel(1).set_shown(&mut spec, true, Shown::BASE));
        assert_eq!(legend.value(&spec), Some(false));
    }

    #[test]
    fn deleting_a_neighbor_does_not_select_the_next_one() {
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
    fn undo_redo_follows_the_reordered_series_and_keeps_a_style_edit() {
        use crate::object_manager::{selection_after_history, VisibilityTarget as T};
        let mut before = PlotSpec::default();
        for name in ["A", "B", "C"] {
            let mut series = SeriesSpec::new(NodeId::from_raw(900));
            series.name = name.into();
            before.panels.get_mut(0).unwrap().series.push(series);
        }
        let mut after = before.clone();
        assert!(after
            .panels
            .get_mut(0)
            .unwrap()
            .place_series(0, None, 2)
            .is_some());
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
