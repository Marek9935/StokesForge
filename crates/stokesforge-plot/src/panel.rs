//! A panel: a plot area, its four axes, its legend, its series,
//! its free texts.

use stokesforge_id::NodeId;

use crate::annotation::AnnotationSpec;
use crate::axis::{AxisSide, AxisSpec};
use crate::legend::LegendSpec;
use crate::series::{SeriesAxis, SeriesSpec};
use crate::text_anchor::TextAnchor;

/// Layer overlaid on another (#167), in the manner of Origin's "New Layer (Axes)":
/// it shares the grid cell **and the plot area** of the layer
/// it covers (linked dimension), on a transparent background.
///
/// Its `bottom` axis is its X axis, its `left` axis its Y axis. `x_top` puts it
/// at the top (the layer's **own** scale); otherwise the axis is not displayed and
/// the X scale is that of the covered layer (linked scale). Same rule for
/// `y_right` and the Y axis, on the right. A layer with no displayed axis (`false`,
/// `false`) therefore fully follows the covered layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OverlaySpec {
    /// The X axis of the layer is displayed at the top, with its own scale.
    pub x_top: bool,
    /// The Y axis of the layer is displayed on the right, with its own scale.
    pub y_right: bool,
    /// Number of the role layer (#168): 2 for the X2/Y2 columns, 3 for
    /// X3/Y3, 4 for X4/Y4; 0 for a layer no role feeds
    /// ("row" series, layer with no axis). This number, not the
    /// position of the layer, says where a column with role Xn/Yn goes.
    pub layer: u8,
}

impl OverlaySpec {
    /// Overlaid layer with both axes displayed, each at its own
    /// scale: the second layer of the X2/Y2 roles (#167).
    pub const SECOND_AXES: Self = Self::role_layer(Self::FIRST_ROLE_LAYER);

    /// Number of the first role layer (X2/Y2) and of the last (X4/Y4, #168).
    pub const FIRST_ROLE_LAYER: u8 = 2;
    /// Last role layer number.
    pub const LAST_ROLE_LAYER: u8 = 4;

    /// Overlaid layer of the Xn/Yn roles: both axes displayed, each at its
    /// own scale (#168). `layer` goes from [`Self::FIRST_ROLE_LAYER`] to
    /// [`Self::LAST_ROLE_LAYER`].
    pub const fn role_layer(layer: u8) -> Self {
        Self {
            x_top: true,
            y_right: true,
            layer,
        }
    }

    /// Overlaid layer with only the X axis displayed, at the top: that of the
    /// "row" series (#166). Its Y axis follows that of the covered layer, so it
    /// stays on the left (`y_right: false`), on purpose.
    pub const TOP_X: Self = Self::new(true, false);

    /// Overlaid layer showing (`true`) or not its X axis at the top and its
    /// Y axis on the right.
    pub const fn new(x_top: bool, y_right: bool) -> Self {
        Self {
            x_top,
            y_right,
            layer: 0,
        }
    }
}

/// A slot of the series list of a panel (#197), as
/// [`PanelSpec::slots`] projects it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeriesSlot {
    /// A series outside any group: its rank in [`PanelSpec::series`].
    Single(usize),
    /// A group and the ranks of all its members, in order.
    Group {
        /// Number of the group in the panel.
        id: u32,
        /// Ranks of the member series.
        members: Vec<usize>,
    },
}

impl SeriesSlot {
    /// True if the series of rank `index` is in this slot.
    pub fn contains(&self, index: usize) -> bool {
        match self {
            Self::Single(i) => *i == index,
            Self::Group { members, .. } => members.contains(&index),
        }
    }
}

/// A panel and its four axes.
///
/// `row`/`column` place the panel in the grid of the [`crate::PlotSpec`] —
/// this is the description of multi-panel layouts, borrowing nothing from the layout
/// system of a given engine. They are `usize`: a negative position
/// is not clamped to zero, it cannot be written.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelSpec {
    /// Title of the panel.
    pub title: String,
    /// Visibility of the title, independent of its content (#85).
    pub title_visible: bool,
    /// Position of the panel title if it was moved (issue #53).
    pub title_anchor: Option<TextAnchor>,
    /// Font and size of the panel title (issue #78).
    pub title_font: crate::TextFont,
    /// Color of the panel title (issue #90); empty = automatic, like
    /// [`crate::PlotSpec::title_color`].
    pub title_color: String,
    /// Plotted series, in stacking order.
    pub series: Vec<SeriesSpec>,
    /// Bottom axis.
    pub bottom: AxisSpec,
    /// Left axis.
    pub left: AxisSpec,
    /// Right axis (secondary).
    pub right: AxisSpec,
    /// Top axis, hidden by default.
    pub top: AxisSpec,
    /// Legend.
    pub legend: LegendSpec,
    /// Layer overlaid on another one of the same cell (#167); `None` for an
    /// ordinary layer. An overlaid layer covers the last ordinary layer
    /// that precedes it in the same cell.
    pub overlay: Option<OverlaySpec>,
    /// Row of the panel in the grid.
    pub row: usize,
    /// Column of the panel in the grid.
    pub column: usize,
    /// Free texts placed on the panel, anchored to the data (issue #35),
    /// in stacking order.
    pub annotations: Vec<AnnotationSpec>,
    /// Graphic objects (rectangle, ellipse, line, arrow — issue #76),
    /// anchored to the data, from bottom to top. Plotted below the free
    /// texts.
    pub shapes: Vec<crate::ShapeSpec>,
    /// Names given to the series groups (#197), by group number. A
    /// group with no entry has its automatic name ("Group n"). Only
    /// the entries of existing groups count: [`PanelSpec::group_label`]
    /// ignores the others and [`PanelSpec::next_group_id`] does not reuse their
    /// number, so a name left behind a vanished group never passes
    /// to a new group.
    pub group_names: std::collections::BTreeMap<u32, String>,
    /// Stops of the color gradient applied to each group (#213), by group
    /// number. A group with no entry received no color gradient: its members
    /// keep their colors. A series that joins a group with a
    /// color gradient gets its color ([`PanelSpec::set_series_group`]).
    pub group_gradients: std::collections::BTreeMap<u32, Vec<String>>,
}

impl PanelSpec {
    /// Default panel placed at `(row, column)`.
    pub fn at(row: usize, column: usize) -> Self {
        Self {
            row,
            column,
            ..Self::default()
        }
    }

    /// A layer overlaid (#167) on the layer `base`: same cell, no title or
    /// legend, only the axes that `overlay` displays are visible, with no
    /// grid.
    pub fn overlay_on(base: &PanelSpec, overlay: OverlaySpec) -> Self {
        let axis = |visible| AxisSpec {
            visible,
            grid: false,
            ..AxisSpec::default()
        };
        let legend = LegendSpec {
            visible: false,
            ..LegendSpec::default()
        };
        Self {
            title_visible: false,
            bottom: axis(overlay.x_top),
            left: axis(overlay.y_right),
            right: AxisSpec::hidden(),
            legend,
            overlay: Some(overlay),
            ..Self::at(base.row, base.column)
        }
    }

    /// The axis of a given side.
    pub fn axis(&self, side: AxisSide) -> &AxisSpec {
        match side {
            AxisSide::Bottom => &self.bottom,
            AxisSide::Left => &self.left,
            AxisSide::Right => &self.right,
            AxisSide::Top => &self.top,
        }
    }

    /// The axis of a given side, mutable.
    pub fn axis_mut(&mut self, side: AxisSide) -> &mut AxisSpec {
        match side {
            AxisSide::Bottom => &mut self.bottom,
            AxisSide::Left => &mut self.left,
            AxisSide::Right => &mut self.right,
            AxisSide::Top => &mut self.top,
        }
    }

    /// True if at least one series is attached to the right axis — this is what
    /// decides to display it.
    pub fn uses_right_axis(&self) -> bool {
        self.series.iter().any(|s| s.axis == SeriesAxis::Right)
    }

    /// The top axis is a mirror of the bottom one (#176).
    pub fn mirrors_top(&self) -> bool {
        self.overlay.is_none() && self.top.mirror
    }

    /// The right axis is a mirror of the left one (#176): only
    /// when no series is attached to it, otherwise it is a real axis.
    pub fn mirrors_right(&self) -> bool {
        self.overlay.is_none() && self.right.mirror && !self.uses_right_axis()
    }

    /// The frame is closed (#344): the top is a mirror, and so is the right
    /// as long as no series is attached to it (otherwise the right is a real
    /// axis and only the top counts). False on an overlaid layer.
    pub fn frame_closed(&self) -> bool {
        self.overlay.is_none() && self.top.mirror && (self.uses_right_axis() || self.right.mirror)
    }

    /// Closes or reopens the frame (#344), in one gesture. Closing sets the top
    /// (and the right, if no series is attached to it) as a mirror without
    /// numbers, and **copies once** the tick marks of the bottom (top) and left
    /// (right): presence, direction and minor ticks; they are then set separately. Reopening
    /// turns these mirrors off and gives their numbers back. No effect on an
    /// overlaid layer (which has its own axes, #167).
    pub fn set_frame_closed(&mut self, closed: bool) {
        if self.overlay.is_some() {
            return;
        }
        let with_right = !self.uses_right_axis();
        let (bottom, left) = (self.bottom.clone(), self.left.clone());
        let set = |mirror: &mut crate::AxisSpec, source: &crate::AxisSpec| {
            mirror.mirror = closed;
            mirror.tick_labels = !closed;
            if closed {
                mirror.ticks = source.ticks;
                mirror.tick_direction = source.tick_direction;
                mirror.minor_ticks = source.minor_ticks;
                mirror.minor_tick_count = source.minor_tick_count;
            }
        };
        set(&mut self.top, &bottom);
        if with_right {
            set(&mut self.right, &left);
        }
    }

    /// Columns referenced by the series of the panel, without duplicates, in
    /// order of first appearance.
    pub fn referenced_columns(&self) -> Vec<NodeId> {
        let mut seen: Vec<NodeId> = Vec::new();
        for series in &self.series {
            for column in series.referenced_columns() {
                if !seen.contains(&column) {
                    seen.push(column);
                }
            }
        }
        seen
    }

    /// The first group number (#177) that no series of the panel carries
    /// and that no group name reserves (#197).
    pub fn next_group_id(&self) -> u32 {
        self.series
            .iter()
            .filter_map(|s| s.group)
            .chain(self.group_names.keys().copied())
            .max()
            .map_or(1, |max| max.saturating_add(1))
    }

    /// Adds series **together** (#177): two or more form a
    /// new group; a single series has none. The group numbers the
    /// series already carried are discarded.
    pub fn push_series_batch(&mut self, series: Vec<SeriesSpec>) {
        let group = (series.len() > 1).then(|| self.next_group_id());
        self.series.extend(series.into_iter().map(|mut s| {
            s.group = group;
            s
        }));
    }

    /// Ranks, in [`PanelSpec::series`], of the series of the group `group`.
    pub fn group_members(&self, group: u32) -> Vec<usize> {
        self.series
            .iter()
            .enumerate()
            .filter_map(|(index, s)| (s.group == Some(group)).then_some(index))
            .collect()
    }

    /// The group numbers present, in order of first appearance.
    pub fn group_ids(&self) -> Vec<u32> {
        let mut ids = Vec::new();
        for group in self.series.iter().filter_map(|s| s.group) {
            if !ids.contains(&group) {
                ids.push(group);
            }
        }
        ids
    }

    /// The name given to the group `group` (#197), if it exists and is not empty.
    /// `None`: the group has its automatic name, or does not exist.
    pub fn group_label(&self, group: u32) -> Option<&str> {
        self.group_names
            .get(&group)
            .map(String::as_str)
            .filter(|name| !name.is_empty() && self.series.iter().any(|s| s.group == Some(group)))
    }

    /// Names the group `group`; an empty name restores the automatic name. False if
    /// the group does not exist or if nothing changes.
    pub fn rename_group(&mut self, group: u32, name: &str) -> bool {
        if !self.series.iter().any(|s| s.group == Some(group)) {
            return false;
        }
        let name = name.trim();
        let before = self.group_names.get(&group).map(String::as_str);
        if before.unwrap_or("") == name {
            return false;
        }
        if name.is_empty() {
            self.group_names.remove(&group);
        } else {
            self.group_names.insert(group, name.to_string());
        }
        true
    }

    /// The slots of the series list (#197), in order: a single
    /// series, or a group with **all** its members. A group takes the
    /// place of its first member — this is the only projection that the Object
    /// Manager and the series window share.
    pub fn slots(&self) -> Vec<SeriesSlot> {
        let mut slots: Vec<SeriesSlot> = Vec::new();
        for (index, series) in self.series.iter().enumerate() {
            let Some(id) = series.group else {
                slots.push(SeriesSlot::Single(index));
                continue;
            };
            let existing = slots.iter_mut().find_map(|slot| match slot {
                SeriesSlot::Group { id: other, members } if *other == id => Some(members),
                _ => None,
            });
            match existing {
                Some(members) => members.push(index),
                None => slots.push(SeriesSlot::Group {
                    id,
                    members: vec![index],
                }),
            }
        }
        slots
    }

    /// Sorts the series in the order of the slots `slots`, then forgets the
    /// names of vanished groups. Returns the final rank of the series `index`
    /// (rank before the call), which a move changes.
    fn apply_slots(&mut self, slots: &[SeriesSlot], index: usize) -> usize {
        let order = self.reorder_by_slots(slots);
        order.iter().position(|i| *i == index).unwrap_or(index)
    }

    /// The work of [`PanelSpec::apply_slots`], which returns the order obtained:
    /// `order[new rank] = old rank`.
    fn reorder_by_slots(&mut self, slots: &[SeriesSlot]) -> Vec<usize> {
        let order: Vec<usize> = slots
            .iter()
            .flat_map(|slot| match slot {
                SeriesSlot::Single(i) => vec![*i],
                SeriesSlot::Group { members, .. } => members.clone(),
            })
            .collect();
        debug_assert_eq!(order.len(), self.series.len());
        let mut taken: Vec<Option<SeriesSpec>> = std::mem::take(&mut self.series)
            .into_iter()
            .map(Some)
            .collect();
        self.series = order.iter().filter_map(|i| taken[*i].take()).collect();
        self.group_names
            .retain(|id, _| self.series.iter().any(|s| s.group == Some(*id)));
        self.group_gradients
            .retain(|id, _| self.series.iter().any(|s| s.group == Some(*id)));
        order
    }

    /// Puts the series `index` in the group `group`, **after** its
    /// members (a group is always contiguous, #197); `None` takes it out of its
    /// group, right behind it. A number no one carries creates the
    /// group. Returns the final rank of the series, `None` if nothing changes.
    pub fn set_series_group(&mut self, index: usize, group: Option<u32>) -> Option<usize> {
        if index >= self.series.len() || self.series[index].group == group {
            return None;
        }
        let mut slots = self.slots();
        let rank = slots.iter().position(|slot| slot.contains(index))?;
        // Where the lone series would land: in place of a slot it
        // empties, otherwise right behind the group it leaves.
        let emptied = match &mut slots[rank] {
            SeriesSlot::Single(_) => true,
            SeriesSlot::Group { members, .. } => {
                members.retain(|m| *m != index);
                members.is_empty()
            }
        };
        let here = if emptied {
            slots.remove(rank);
            rank
        } else {
            rank + 1
        };
        let joined = group.and_then(|id| {
            slots.iter_mut().find_map(|slot| match slot {
                SeriesSlot::Group { id: other, members } if *other == id => Some(members),
                _ => None,
            })
        });
        match (group, joined) {
            (Some(_), Some(members)) => members.push(index),
            (Some(id), None) => slots.insert(
                here,
                SeriesSlot::Group {
                    id,
                    members: vec![index],
                },
            ),
            (None, _) => slots.insert(here, SeriesSlot::Single(index)),
        }
        self.series[index].group = group;
        let rank = self.apply_slots(&slots, index);
        // Dynamic (#213): a series that joins a group with a color gradient
        // gets its color, and the color gradient is spread again.
        if let Some(stops) = group.and_then(|id| self.group_gradients.get(&id).cloned()) {
            if let Some(id) = group {
                self.apply_group_gradient(id, &stops);
            }
        }
        Some(rank)
    }

    /// Where the series `index` is: its group and its rank in that
    /// container — among the slots of the panel if it is alone, among
    /// the members of its group otherwise. This is the pair that
    /// [`PanelSpec::place_series`] returns to find it again.
    pub fn series_position(&self, index: usize) -> Option<(Option<u32>, usize)> {
        let group = self.series.get(index)?.group;
        let slots = self.slots();
        match group {
            None => slots
                .iter()
                .position(|slot| matches!(slot, SeriesSlot::Single(i) if *i == index))
                .map(|rank| (None, rank)),
            Some(id) => slots.iter().find_map(|slot| match slot {
                SeriesSlot::Group { id: other, members } if *other == id => members
                    .iter()
                    .position(|m| *m == index)
                    .map(|rank| (Some(id), rank)),
                _ => None,
            }),
        }
    }

    /// Places the series `index` in the container `group` at the rank `position` of that
    /// container, **counted once the series is removed** (drag and drop):
    /// `None` makes it alone, at the rank `position` among the slots;
    /// `Some(id)` puts it in that group, at the rank `position` of its members (a
    /// number no one carries creates the group). A rank that is too large goes to the
    /// end. A group the series empties disappears with its name.
    ///
    /// Returns the order obtained (`order[new rank] = old rank`), `None` without
    /// changing anything if the series is already there or does not exist.
    pub fn place_series(
        &mut self,
        index: usize,
        group: Option<u32>,
        position: usize,
    ) -> Option<Vec<usize>> {
        if index >= self.series.len() {
            return None;
        }
        let before = self.slots();
        let mut slots = before.clone();
        let rank = slots.iter().position(|slot| slot.contains(index))?;
        let emptied = match &mut slots[rank] {
            SeriesSlot::Single(_) => true,
            SeriesSlot::Group { members, .. } => {
                members.retain(|m| *m != index);
                members.is_empty()
            }
        };
        if emptied {
            slots.remove(rank);
        }
        let joined = group.and_then(|id| {
            slots.iter_mut().find_map(|slot| match slot {
                SeriesSlot::Group { id: other, members } if *other == id => Some(members),
                _ => None,
            })
        });
        match (group, joined) {
            (Some(_), Some(members)) => members.insert(position.min(members.len()), index),
            (Some(id), None) => slots.insert(
                position.min(slots.len()),
                SeriesSlot::Group {
                    id,
                    members: vec![index],
                },
            ),
            (None, _) => slots.insert(position.min(slots.len()), SeriesSlot::Single(index)),
        }
        if slots == before && self.series[index].group == group {
            return None;
        }
        self.series[index].group = group;
        let order = self.reorder_by_slots(&slots);
        // Like `set_series_group`: a series that joins a group with a
        // color gradient gets its color.
        if let Some(id) = group {
            if let Some(stops) = self.group_gradients.get(&id).cloned() {
                self.apply_group_gradient(id, &stops);
            }
        }
        Some(order)
    }

    /// Places the group `group` at the rank `position` among the slots, counted
    /// once the group is removed (drag and drop). Returns the order obtained
    /// (`order[new rank] = old rank`), `None` if the group does not exist
    /// or does not move.
    pub fn place_group(&mut self, group: u32, position: usize) -> Option<Vec<usize>> {
        let before = self.slots();
        let mut slots = before.clone();
        let rank = slots
            .iter()
            .position(|slot| matches!(slot, SeriesSlot::Group { id, .. } if *id == group))?;
        let slot = slots.remove(rank);
        slots.insert(position.min(slots.len()), slot);
        if slots == before {
            return None;
        }
        Some(self.reorder_by_slots(&slots))
    }

    /// Gathers the series `indices` (distinct ranks, in the wanted order)
    /// into a new group, which takes the place of the first one met in
    /// the list (#196). A group they come from and empty
    /// disappears with its name. Returns the group number; `None`, without changing
    /// anything, if `indices` is empty, outside the panel or repeats a rank.
    pub fn group_series(&mut self, indices: &[usize]) -> Option<u32> {
        let mut seen = indices.to_vec();
        seen.sort_unstable();
        seen.dedup();
        if indices.is_empty() || seen.len() != indices.len() || seen.last()? >= &self.series.len() {
            return None;
        }
        let id = self.next_group_id();
        let mut slots = Vec::new();
        let mut placed = false;
        for slot in self.slots() {
            let touched = match &slot {
                SeriesSlot::Single(i) => indices.contains(i),
                SeriesSlot::Group { members, .. } => members.iter().any(|m| indices.contains(m)),
            };
            match slot {
                SeriesSlot::Single(i) if !touched => slots.push(SeriesSlot::Single(i)),
                SeriesSlot::Single(_) => {}
                SeriesSlot::Group { id: other, members } => {
                    let kept: Vec<usize> = members
                        .into_iter()
                        .filter(|m| !indices.contains(m))
                        .collect();
                    if !kept.is_empty() {
                        slots.push(SeriesSlot::Group {
                            id: other,
                            members: kept,
                        });
                    }
                }
            }
            if touched && !placed {
                placed = true;
                slots.push(SeriesSlot::Group {
                    id,
                    members: indices.to_vec(),
                });
            }
        }
        for &index in indices {
            self.series[index].group = Some(id);
        }
        self.apply_slots(&slots, 0);
        Some(id)
    }

    /// Takes the series `index` out of its group (#197).
    pub fn leave_group(&mut self, index: usize) -> Option<usize> {
        self.series.get(index)?.group?;
        self.set_series_group(index, None)
    }

    /// Makes the series `index` the first member of a new group.
    pub fn start_group(&mut self, index: usize) -> Option<u32> {
        let alone = self
            .series
            .get(index)?
            .group
            .is_some_and(|group| self.group_members(group).len() == 1);
        if alone {
            return None;
        }
        let id = self.next_group_id();
        self.set_series_group(index, Some(id))?;
        Some(id)
    }

    /// Dissolves the group `group` (#197): its series become single, in the
    /// same place, each keeping its style. False if the group does not exist.
    pub fn dissolve_group(&mut self, group: u32) -> bool {
        let mut changed = false;
        for series in &mut self.series {
            if series.group == Some(group) {
                series.group = None;
                changed = true;
            }
        }
        self.group_names.remove(&group);
        self.group_gradients.remove(&group);
        changed
    }

    /// Removes the series of rank `index` (#257), without touching the others or
    /// their groups. False, without changing anything, if the rank does not exist.
    pub fn remove_series(&mut self, index: usize) -> bool {
        if index >= self.series.len() {
            return false;
        }
        self.series.remove(index);
        true
    }

    /// Deletes the group `group` and its series (#197). False if it does not exist.
    pub fn remove_group(&mut self, group: u32) -> bool {
        let before = self.series.len();
        self.series.retain(|s| s.group != Some(group));
        self.group_names.remove(&group);
        self.group_gradients.remove(&group);
        self.series.len() != before
    }

    /// Moves the series `index` up (`up`) or down by one step (#197). A series
    /// of a group stays in its group; a single series jumps over
    /// a whole group. Returns its final rank, `None` at the limit.
    pub fn move_series(&mut self, index: usize, up: bool) -> Option<usize> {
        let mut slots = self.slots();
        let rank = slots.iter().position(|slot| slot.contains(index))?;
        match &mut slots[rank] {
            SeriesSlot::Group { members, .. } => {
                let at = members.iter().position(|m| *m == index)?;
                let other = if up { at.checked_sub(1)? } else { at + 1 };
                if other >= members.len() {
                    return None;
                }
                members.swap(at, other);
            }
            SeriesSlot::Single(_) => {
                let next = if up { rank.checked_sub(1)? } else { rank + 1 };
                if next >= slots.len() {
                    return None;
                }
                slots.swap(rank, next);
            }
        }
        Some(self.apply_slots(&slots, index))
    }

    /// Spreads the color gradient with stops `stops` over the series of the group `group`
    /// (#178), in panel order: the line and the fill of the
    /// symbols take the color. False, without changing anything, if the group
    /// does not exist or if no stop is readable.
    pub fn apply_group_gradient<S: AsRef<str>>(&mut self, group: u32, stops: &[S]) -> bool {
        let members = self.group_members(group);
        let colors = crate::gradient_colors(stops, members.len());
        let mut changed = false;
        for (&member, color) in members.iter().zip(colors) {
            let series = &mut self.series[member];
            changed |= series.line.color != color || series.symbol.color != color;
            series.line.color.clone_from(&color);
            series.symbol.color = color;
        }
        changed
    }

    /// Retains `stops` as the color gradient of the group `group` and applies it to its
    /// members (#213). False, without changing anything, if the group does not exist or
    /// if no stop is readable; otherwise true when the retained color gradient or
    /// a color changed.
    pub fn set_group_gradient<S: AsRef<str>>(&mut self, group: u32, stops: &[S]) -> bool {
        if self.group_members(group).is_empty() || crate::gradient_colors(stops, 1).is_empty() {
            return false;
        }
        let stops: Vec<String> = stops.iter().map(|s| s.as_ref().to_string()).collect();
        let recolored = self.apply_group_gradient(group, &stops);
        let retained = self.group_gradients.insert(group, stops.clone());
        recolored || retained.as_ref() != Some(&stops)
    }

    /// The color gradient retained for the group `group` (#213), if it received one.
    pub fn group_gradient(&self, group: u32) -> Option<&[String]> {
        self.group_gradients
            .get(&group)
            .filter(|_| self.series.iter().any(|s| s.group == Some(group)))
            .map(Vec::as_slice)
    }

    /// True if [`PanelSpec::move_series`] would move the series `index`.
    pub fn can_move_series(&self, index: usize, up: bool) -> bool {
        self.clone().move_series(index, up).is_some()
    }

    /// True if [`PanelSpec::move_group`] would move the group `group`.
    pub fn can_move_group(&self, group: u32, up: bool) -> bool {
        self.clone().move_group(group, up)
    }

    /// Moves the group `group` up (`up`) or down by one slot (#197).
    /// False at the limit or if the group does not exist.
    pub fn move_group(&mut self, group: u32, up: bool) -> bool {
        let mut slots = self.slots();
        let Some(rank) = slots
            .iter()
            .position(|slot| matches!(slot, SeriesSlot::Group { id, .. } if *id == group))
        else {
            return false;
        };
        let next = if up {
            rank.checked_sub(1)
        } else {
            Some(rank + 1)
        };
        let Some(next) = next.filter(|n| *n < slots.len()) else {
            return false;
        };
        slots.swap(rank, next);
        self.apply_slots(&slots, 0);
        true
    }

    /// True if a series of the panel references this column.
    pub fn references(&self, column: NodeId) -> bool {
        self.series.iter().any(|s| s.references(column))
    }
}

impl Default for PanelSpec {
    fn default() -> Self {
        Self {
            title: String::new(),
            title_visible: true,
            title_anchor: None,
            title_font: crate::TextFont::default(),
            title_color: String::new(),
            series: Vec::new(),
            bottom: AxisSpec::default(),
            left: AxisSpec::default(),
            right: AxisSpec::default(),
            top: AxisSpec::hidden(),
            legend: LegendSpec::default(),
            overlay: None,
            row: 0,
            column: 0,
            annotations: Vec::new(),
            shapes: Vec::new(),
            group_names: std::collections::BTreeMap::new(),
            group_gradients: std::collections::BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod group_tests {
    use super::*;

    fn make_series(n: u64) -> SeriesSpec {
        SeriesSpec::new(NodeId::from_raw(n))
    }

    /// #257: `remove_series` removes exactly the series of rank `index`, without
    /// touching the others or their groups; an out-of-range rank changes nothing.
    #[test]
    fn remove_series_removes_the_designated_series_and_no_other() {
        let mut panel = PanelSpec::default();
        panel.push_series_batch(vec![make_series(1)]);
        panel.push_series_batch(vec![make_series(2), make_series(3)]);
        let before = panel.clone();

        assert!(!panel.remove_series(3), "out of range: nothing changes");
        assert_eq!(panel, before);

        assert!(panel.remove_series(1));
        let referenced: Vec<NodeId> = panel.series.iter().map(|s| s.y).collect();
        assert_eq!(referenced, [NodeId::from_raw(1), NodeId::from_raw(3)]);
        assert_eq!(panel.series[1].group, Some(1), "the group keeps its member");

        assert!(panel.remove_series(0));
        assert_eq!(panel.series.len(), 1);
        assert_eq!(panel.series[0].y, NodeId::from_raw(3));
    }

    /// #257: the named layers — the second X2/Y2 layer has both axes, the
    /// row layer only X at the top (its Y follows the covered layer).
    #[test]
    fn named_layers_say_which_axes_they_display() {
        assert_eq!(
            (
                OverlaySpec::SECOND_AXES.x_top,
                OverlaySpec::SECOND_AXES.y_right
            ),
            (true, true)
        );
        assert_eq!(
            (OverlaySpec::TOP_X.x_top, OverlaySpec::TOP_X.y_right),
            (true, false)
        );
        assert_eq!(
            OverlaySpec::new(false, true),
            OverlaySpec {
                x_top: false,
                y_right: true,
                layer: 0
            }
        );
        assert_ne!(OverlaySpec::SECOND_AXES, OverlaySpec::TOP_X);
        // The panel constructor derives the displayed axes from it.
        let base = PanelSpec::default();
        let layer = PanelSpec::overlay_on(&base, OverlaySpec::TOP_X);
        assert!(layer.bottom.visible && !layer.left.visible);
    }

    #[test]
    fn series_added_together_form_a_group_a_single_one_does_not() {
        let mut panel = PanelSpec::default();
        panel.push_series_batch(vec![make_series(1)]);
        assert_eq!(panel.series[0].group, None);
        panel.push_series_batch(vec![make_series(2), make_series(3)]);
        assert_eq!(panel.group_members(1), [1, 2]);
        panel.push_series_batch(vec![make_series(4), make_series(5)]);
        assert_eq!(panel.group_ids(), [1, 2]);
        assert_eq!(panel.next_group_id(), 3);
    }

    #[test]
    fn the_gradient_colors_the_series_of_the_group_in_order_and_only_them() {
        let mut panel = PanelSpec::default();
        panel.push_series_batch(vec![make_series(1)]);
        panel.push_series_batch((2..=4).map(make_series).collect());
        let alone = panel.series[0].clone();
        let stops = ["#000000", "#ff0000"];
        assert!(panel.apply_group_gradient(1, &stops));
        assert_eq!(
            panel.series[0], alone,
            "the series outside the group is intact"
        );
        let colors: Vec<&str> = panel.series[1..]
            .iter()
            .map(|s| s.line.color.as_str())
            .collect();
        assert_eq!(colors, ["#000000", "#800000", "#ff0000"]);
        assert!(panel.series[1..]
            .iter()
            .all(|s| s.symbol.color == s.line.color));
        assert!(!panel.apply_group_gradient(1, &stops), "already applied");
    }

    #[test]
    fn an_unreadable_gradient_changes_nothing() {
        let mut panel = PanelSpec::default();
        panel.push_series_batch((1..=3).map(make_series).collect());
        let before = panel.clone();
        assert!(!panel.apply_group_gradient(1, &["not a color"]));
        assert_eq!(panel, before);
        assert!(!panel.apply_group_gradient(9, &["#000000", "#ffffff"]));
    }

    /// #213: the group retains its color gradient; a new group has none and
    /// its colors do not move.
    #[test]
    fn the_group_retains_its_gradient() {
        let mut panel = PanelSpec::default();
        panel.push_series_batch((1..=3).map(make_series).collect());
        let before = panel.clone();
        assert_eq!(
            panel.group_gradient(1),
            None,
            "new group: no color gradient"
        );
        assert!(!panel.set_group_gradient(1, &["not a color"]));
        assert!(!panel.set_group_gradient(9, &["#000000", "#ffffff"]));
        assert_eq!(panel, before, "a refused color gradient retains nothing");

        assert!(panel.set_group_gradient(1, &["#000000", "#ff0000"]));
        assert_eq!(
            panel.group_gradient(1),
            Some(&["#000000".to_string(), "#ff0000".to_string()][..])
        );
        assert!(
            !panel.set_group_gradient(1, &["#000000", "#ff0000"]),
            "already there"
        );
        assert!(panel.dissolve_group(1));
        assert_eq!(
            panel.group_gradient(1),
            None,
            "dissolving forgets the color gradient"
        );
    }

    /// #213: a series that joins the group is colored by its color gradient,
    /// which is spread again; without a color gradient, it keeps its color.
    #[test]
    fn a_series_added_to_the_group_receives_the_gradient() {
        let mut panel = PanelSpec::default();
        panel.push_series_batch((1..=2).map(make_series).collect());
        panel.push_series_batch(vec![make_series(3)]);
        let color = panel.series[2].line.color.clone();
        panel.push_series_batch((4..=5).map(make_series).collect());
        // Group 2 without a color gradient: the series that joined it keeps its color.
        assert!(panel.set_series_group(2, Some(2)).is_some());
        assert_eq!(panel.series[4].line.color, color);

        assert!(panel.set_group_gradient(1, &["#000000", "#ffffff"]));
        assert!(panel.set_series_group(4, Some(1)).is_some());
        let colors: Vec<&str> = panel
            .group_members(1)
            .iter()
            .map(|&m| panel.series[m].line.color.as_str())
            .collect();
        assert_eq!(colors, ["#000000", "#808080", "#ffffff"]);
    }

    /// A panel "1, [2 3], 4, [5 6]", the columns being worth their rank.
    fn sample_panel() -> PanelSpec {
        let mut panel = PanelSpec::default();
        panel.push_series_batch(vec![make_series(1)]);
        panel.push_series_batch(vec![make_series(2), make_series(3)]);
        panel.push_series_batch(vec![make_series(4)]);
        panel.push_series_batch(vec![make_series(5), make_series(6)]);
        panel
    }

    /// The Y columns in panel order.
    fn columns_of(panel: &PanelSpec) -> Vec<u64> {
        panel.series.iter().map(|s| s.y.raw()).collect()
    }

    #[test]
    fn a_group_projects_into_one_slot_with_all_its_members() {
        let slots = sample_panel().slots();
        assert_eq!(
            slots,
            [
                SeriesSlot::Single(0),
                SeriesSlot::Group {
                    id: 1,
                    members: vec![1, 2]
                },
                SeriesSlot::Single(3),
                SeriesSlot::Group {
                    id: 2,
                    members: vec![4, 5]
                },
            ]
        );
    }

    #[test]
    fn a_group_with_scattered_members_gathers_at_the_place_of_the_first() {
        let mut panel = sample_panel();
        panel.series.swap(2, 3); // 1, 2, 4, 3, …
        assert_eq!(
            panel.slots()[1],
            SeriesSlot::Group {
                id: 1,
                members: vec![1, 3]
            }
        );
    }

    #[test]
    fn a_group_name_is_set_read_and_cleared() {
        let mut panel = sample_panel();
        assert_eq!(panel.group_label(1), None);
        assert!(panel.rename_group(1, "  Whites  "));
        assert_eq!(panel.group_label(1), Some("Whites"));
        assert!(
            !panel.rename_group(1, "Whites"),
            "same name: nothing changes"
        );
        assert!(panel.rename_group(1, ""));
        assert_eq!(panel.group_label(1), None);
        assert!(!panel.rename_group(9, "Ghost"), "nonexistent group");
        assert!(panel.group_names.is_empty());
    }

    #[test]
    fn the_name_of_a_vanished_group_does_not_return_to_a_new_group() {
        let mut panel = sample_panel();
        panel.rename_group(2, "Reds");
        assert!(panel.dissolve_group(2));
        assert!(panel.group_names.is_empty(), "dissolving forgets the name");
        // A name left alone (edited file) reserves its number without naming.
        panel.group_names.insert(7, "Rest".into());
        assert_eq!(panel.next_group_id(), 8);
        assert_eq!(panel.group_label(7), None);
    }

    #[test]
    fn entering_a_group_places_the_series_after_its_members() {
        let mut panel = sample_panel();
        // The single series "4" (rank 3) joins group 1: 1, [2 3 4], [5 6].
        assert_eq!(panel.set_series_group(3, Some(1)), Some(3));
        assert_eq!(columns_of(&panel), [1, 2, 3, 4, 5, 6]);
        // The first single series joins group 2: [2 3 4], [5 6 1].
        assert_eq!(panel.set_series_group(0, Some(2)), Some(5));
        assert_eq!(columns_of(&panel), [2, 3, 4, 5, 6, 1]);
        assert_eq!(panel.group_members(2), [3, 4, 5]);
    }

    #[test]
    fn leaving_a_group_places_the_series_right_behind_it() {
        let mut panel = sample_panel();
        // "2" leaves group 1: 1, [3], 2, 4, [5 6].
        assert_eq!(panel.leave_group(1), Some(2));
        assert_eq!(columns_of(&panel), [1, 3, 2, 4, 5, 6]);
        assert_eq!(panel.series[2].group, None);
        // "3" is the last: group 1 disappears, the series stays in place.
        assert_eq!(panel.leave_group(1), Some(1));
        assert_eq!(panel.group_ids(), [2]);
        assert_eq!(panel.leave_group(0), None, "already alone");
    }

    #[test]
    fn placing_a_single_series_between_two_slots() {
        let mut panel = sample_panel();
        // "4" (rank 3) moves to the head of the slots: 4, 1, [2 3], [5 6].
        let order = panel.place_series(3, None, 0).unwrap();
        assert_eq!(columns_of(&panel), [4, 1, 2, 3, 5, 6]);
        assert_eq!(order, [3, 0, 1, 2, 4, 5]);
        // Already there: nothing changes.
        assert!(panel.place_series(0, None, 0).is_none());
        // Beyond the end: all the way to the end.
        panel.place_series(0, None, 99).unwrap();
        assert_eq!(columns_of(&panel), [1, 2, 3, 5, 6, 4]);
        assert_eq!(panel.series_position(5), Some((None, 3)));
    }

    #[test]
    fn placing_a_series_in_a_group_chooses_its_rank() {
        let mut panel = sample_panel();
        // "4" enters group 1 between its two members: 1, [2 4 3], [5 6].
        panel.place_series(3, Some(1), 1).unwrap();
        assert_eq!(columns_of(&panel), [1, 2, 4, 3, 5, 6]);
        assert_eq!(panel.series[2].group, Some(1));
        assert_eq!(panel.series_position(2), Some((Some(1), 1)));
        // Reorder within the group: [4 2 3].
        panel.place_series(2, Some(1), 0).unwrap();
        assert_eq!(columns_of(&panel), [1, 4, 2, 3, 5, 6]);
        assert!(panel.place_series(1, Some(1), 0).is_none());
    }

    #[test]
    fn placing_a_series_out_of_its_group_makes_it_single_at_the_wanted_rank() {
        let mut panel = sample_panel();
        // "3" leaves group 1 for the head: 3, 1, [2], 4, [5 6].
        panel.place_series(2, None, 0).unwrap();
        assert_eq!(columns_of(&panel), [3, 1, 2, 4, 5, 6]);
        assert_eq!(panel.series[0].group, None);
        assert_eq!(panel.group_members(1), [2]);
        // The last member leaves: the group and its name disappear.
        panel.rename_group(1, "Batch");
        panel.place_series(2, None, 4).unwrap();
        assert_eq!(panel.group_ids(), [2]);
        assert!(panel.group_names.is_empty());
    }

    #[test]
    fn placing_a_series_from_a_group_into_another_group() {
        let mut panel = sample_panel();
        // "2" moves from group 1 to group 2, at the head: 1, [3], 4, [2 5 6].
        panel.place_series(1, Some(2), 0).unwrap();
        assert_eq!(columns_of(&panel), [1, 3, 4, 2, 5, 6]);
        assert_eq!(panel.group_members(2), [3, 4, 5]);
        assert_eq!(panel.group_members(1), [1]);
    }

    #[test]
    fn placing_a_series_in_an_unknown_group_creates_it_at_that_rank() {
        let mut panel = sample_panel();
        panel.place_series(0, Some(9), 1).unwrap();
        assert_eq!(columns_of(&panel), [2, 3, 1, 4, 5, 6]);
        assert_eq!(panel.group_members(9), [2]);
    }

    #[test]
    fn placing_a_group_moves_it_as_a_block() {
        let mut panel = sample_panel();
        // Group 2 moves to the head: [5 6], 1, [2 3], 4.
        let order = panel.place_group(2, 0).unwrap();
        assert_eq!(columns_of(&panel), [5, 6, 1, 2, 3, 4]);
        assert_eq!(order, [4, 5, 0, 1, 2, 3]);
        assert!(panel.place_group(2, 0).is_none(), "already at the head");
        assert!(panel.place_group(7, 0).is_none(), "unknown group");
        panel.place_group(2, 99).unwrap();
        assert_eq!(columns_of(&panel), [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn a_group_emptied_by_departure_loses_its_name() {
        let mut panel = PanelSpec::default();
        panel.push_series_batch(vec![make_series(1), make_series(2)]);
        panel.rename_group(1, "Ephemeral");
        panel.leave_group(0);
        panel.leave_group(0);
        assert!(panel.group_names.is_empty());
    }

    #[test]
    fn starting_a_group_gives_a_new_number() {
        let mut panel = sample_panel();
        assert_eq!(panel.start_group(0), Some(3));
        assert_eq!(panel.group_members(3), [0]);
        assert_eq!(panel.start_group(0), None, "already alone in its group");
    }

    #[test]
    fn dissolving_keeps_the_series_and_their_style() {
        let mut panel = sample_panel();
        panel.series[1].line.width = 3.5;
        let before = columns_of(&panel);
        assert!(panel.dissolve_group(1));
        assert_eq!(columns_of(&panel), before);
        assert_eq!(panel.series[1].group, None);
        assert_eq!(panel.series[1].line.width, 3.5);
        assert!(!panel.dissolve_group(1));
    }

    #[test]
    fn deleting_the_group_removes_its_series_and_its_name() {
        let mut panel = sample_panel();
        panel.rename_group(1, "To discard");
        assert!(panel.remove_group(1));
        assert_eq!(columns_of(&panel), [1, 4, 5, 6]);
        assert!(panel.group_names.is_empty());
        assert!(!panel.remove_group(1));
    }

    #[test]
    fn moving_a_series_up_and_down_stays_within_its_group() {
        let mut panel = sample_panel();
        assert_eq!(panel.move_series(1, true), None, "first of the group");
        assert_eq!(panel.move_series(1, false), Some(2));
        assert_eq!(columns_of(&panel), [1, 3, 2, 4, 5, 6]);
        assert_eq!(panel.move_series(2, false), None, "last of the group");
    }

    #[test]
    fn a_single_series_jumps_over_a_whole_group() {
        let mut panel = sample_panel();
        assert_eq!(panel.move_series(3, true), Some(1));
        assert_eq!(columns_of(&panel), [1, 4, 2, 3, 5, 6]);
        assert_eq!(panel.move_series(1, true), Some(0));
        assert_eq!(panel.move_series(0, true), None);
        assert_eq!(panel.move_series(0, false), Some(1));
    }

    #[test]
    fn a_group_moves_as_a_block() {
        let mut panel = sample_panel();
        assert!(panel.move_group(2, true));
        assert_eq!(columns_of(&panel), [1, 2, 3, 5, 6, 4]);
        assert!(panel.move_group(1, false));
        assert_eq!(columns_of(&panel), [1, 5, 6, 2, 3, 4]);
        assert!(panel.move_group(2, true));
        assert_eq!(columns_of(&panel), [5, 6, 1, 2, 3, 4]);
        assert!(!panel.move_group(2, true), "limit");
        assert!(!panel.move_group(9, true), "nonexistent");
    }

    #[test]
    fn gathering_series_creates_a_group_at_the_place_of_the_first() {
        let mut panel = sample_panel(); // 1, [2 3], 4, [5 6]
                                        // "4" (rank 3) and "1" (rank 0), in that order: the group takes the
                                        // place of "1", the first met in the list.
        let id = panel.group_series(&[3, 0]).expect("group");
        assert_eq!(id, 3);
        assert_eq!(columns_of(&panel), [4, 1, 2, 3, 5, 6]);
        assert_eq!(panel.group_members(id), [0, 1]);
        assert_eq!(
            panel.group_members(1),
            [2, 3],
            "the other groups are intact"
        );
    }

    #[test]
    fn gathering_empties_an_original_group_and_forgets_its_name() {
        let mut panel = sample_panel();
        panel.rename_group(1, "Old");
        let id = panel.group_series(&[1, 2, 3]).expect("group");
        assert_eq!(panel.group_ids(), [id, 2], "group 1 has disappeared");
        assert!(panel.group_names.is_empty());
        assert_eq!(columns_of(&panel), [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn gathering_part_of_a_group_keeps_the_rest_and_its_name() {
        let mut panel = sample_panel();
        panel.rename_group(1, "Rest");
        let id = panel.group_series(&[1, 3]).expect("group");
        assert_eq!(panel.group_label(1), Some("Rest"));
        assert_eq!(panel.group_members(1).len(), 1);
        assert_eq!(panel.group_members(id).len(), 2);
    }

    #[test]
    fn gathering_rejects_an_invalid_request_without_changing_anything() {
        let mut panel = sample_panel();
        let before = panel.clone();
        assert_eq!(panel.group_series(&[]), None);
        assert_eq!(panel.group_series(&[1, 1]), None);
        assert_eq!(panel.group_series(&[0, 9]), None);
        assert_eq!(panel, before);
    }
}
