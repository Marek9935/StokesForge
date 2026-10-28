//! The pure decisions of drag and drop for series, groups and free objects
//! (Object Manager, series window).
//!
//! Same gesture as the list of copy formats (`app/reorder_list.rs`): the row
//! grabbed by its handle **changes place immediately** when the pointer hovers
//! another, with no guide line; on release, the list is already in its new
//! order. This module only says *where* the dragged element goes according to
//! the hovered row — the interface passes it the element (`DragItem`) and the
//! hover (`DropOver`), it returns the new state.
//!
//! Rules, all in [`step`]:
//! - **a series over a series**: in the same container (the group, or the
//!   lone series), it takes the place of the hovered row; coming from another
//!   container, it is inserted before the row if the pointer is in its upper
//!   half, after otherwise. Hovering a member of a group thus makes the series
//!   **enter** it; hovering a lone series makes it lone (**exit**);
//! - **a series over a group header**: upper half, it is placed alone just
//!   before the group; lower half, it becomes its first member;
//! - **a series over the end zone**: alone, at the very bottom of the list
//!   (the only possible exit from a group that ends the list);
//! - **a group** (grabbed by its header): it takes the place of the hovered
//!   slot, as a block;
//! - **a free object** (text, shape): it takes the place of the hovered
//!   object **of its category**; a text never goes among the shapes.
//!
//! A move between layers does not exist: it would change the axes the data is
//! attached to. Axes are not reordered.

use stokesforge_plot::{PanelSpec, PlotSpec, SeriesGroupRef, SeriesRef, SeriesSlot};

use crate::object_manager::VisibilityTarget;

/// What the handle has grabbed. `origin` is the address at the start of the
/// gesture (that of the real spec), `current` that of the element in the
/// preview: the list reorders during the drag, so the element changes rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragItem {
    Series {
        panel: usize,
        origin: usize,
        current: usize,
    },
    Group {
        panel: usize,
        group: u32,
    },
    Text {
        panel: usize,
        origin: usize,
        current: usize,
    },
    Shape {
        panel: usize,
        origin: usize,
        current: usize,
    },
}

impl DragItem {
    /// The layer the element does not leave.
    pub fn panel(self) -> usize {
        match self {
            Self::Series { panel, .. }
            | Self::Group { panel, .. }
            | Self::Text { panel, .. }
            | Self::Shape { panel, .. } => panel,
        }
    }

    /// The grabbed element, as the preview shows it: the target to keep
    /// selected during the gesture.
    pub fn target(self) -> VisibilityTarget {
        use stokesforge_plot::{AnnotationRef, ShapeRef};
        match self {
            Self::Series { panel, current, .. } => {
                VisibilityTarget::Series(SeriesRef::new(panel, current))
            }
            Self::Group { panel, group } => {
                VisibilityTarget::SeriesGroup(SeriesGroupRef::new(panel, group))
            }
            Self::Text { panel, current, .. } => VisibilityTarget::Annotation(AnnotationRef {
                panel,
                index: current,
            }),
            Self::Shape { panel, current, .. } => VisibilityTarget::Shape(ShapeRef {
                panel,
                index: current,
            }),
        }
    }
}

/// What the pointer hovers during the drag. `after`: the pointer is in the
/// lower half of the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropOver {
    Series {
        panel: usize,
        index: usize,
        after: bool,
    },
    Header {
        panel: usize,
        group: u32,
        after: bool,
    },
    /// The empty zone under the layer's last series.
    End {
        panel: usize,
    },
    Text {
        panel: usize,
        index: usize,
    },
    Shape {
        panel: usize,
        index: usize,
    },
}

impl DropOver {
    fn panel(self) -> usize {
        match self {
            Self::Series { panel, .. }
            | Self::Header { panel, .. }
            | Self::End { panel }
            | Self::Text { panel, .. }
            | Self::Shape { panel, .. } => panel,
        }
    }
}

/// The rank, among `slots`, of the slot that contains series `index`.
fn slot_rank(slots: &[SeriesSlot], index: usize) -> Option<usize> {
    slots.iter().position(|slot| slot.contains(index))
}

/// The rank of slot `rank` once series `dragged` is removed: its slot
/// disappears if it was alone in it, and shifts what follows.
fn rank_without(slots: &[SeriesSlot], dragged: usize, rank: usize) -> usize {
    let vanishes = slots.iter().position(|slot| match slot {
        SeriesSlot::Single(i) => *i == dragged,
        SeriesSlot::Group { members, .. } => members.as_slice() == [dragged],
    });
    match vanishes {
        Some(gone) if gone < rank => rank - 1,
        _ => rank,
    }
}

/// Where series `dragged` goes when the pointer hovers `over`: the container
/// (`None`: alone) and the rank in that container, counted with the series
/// removed.
fn series_destination(
    panel: &PanelSpec,
    dragged: usize,
    over: DropOver,
) -> Option<(Option<u32>, usize)> {
    let own = panel.series.get(dragged)?.group;
    let slots = panel.slots();
    match over {
        DropOver::Series { index, after, .. } => {
            if index == dragged {
                return None;
            }
            match panel.series.get(index)?.group {
                None => {
                    let rank = slot_rank(&slots, index)?;
                    Some(if own.is_none() {
                        (None, rank)
                    } else {
                        (
                            None,
                            rank_without(&slots, dragged, rank) + usize::from(after),
                        )
                    })
                }
                Some(group) => {
                    let place = panel
                        .group_members(group)
                        .iter()
                        .position(|member| *member == index)?;
                    Some(if own == Some(group) {
                        (Some(group), place)
                    } else {
                        (Some(group), place + usize::from(after))
                    })
                }
            }
        }
        DropOver::Header { group, after, .. } => {
            let rank = slots
                .iter()
                .position(|slot| matches!(slot, SeriesSlot::Group { id, .. } if *id == group))?;
            if !after {
                return Some((None, rank_without(&slots, dragged, rank)));
            }
            (own != Some(group)).then_some((Some(group), 0))
        }
        DropOver::End { .. } => Some((None, usize::MAX)),
        DropOver::Text { .. } | DropOver::Shape { .. } => None,
    }
}

/// Where group `group` goes when the pointer hovers `over`: the rank among
/// the slots, with the group removed.
fn group_destination(panel: &PanelSpec, group: u32, over: DropOver) -> Option<usize> {
    let slots = panel.slots();
    let own = slots
        .iter()
        .position(|slot| matches!(slot, SeriesSlot::Group { id, .. } if *id == group))?;
    let rank = match over {
        DropOver::Series { index, .. } => slot_rank(&slots, index)?,
        DropOver::Header { group: other, .. } => slots
            .iter()
            .position(|slot| matches!(slot, SeriesSlot::Group { id, .. } if *id == other))?,
        DropOver::End { .. } => return Some(usize::MAX),
        DropOver::Text { .. } | DropOver::Shape { .. } => return None,
    };
    (rank != own).then_some(rank)
}

/// Moves element `from` to rank `to` of a list of free objects.
fn shift<T>(list: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from >= list.len() || to >= list.len() || from == to {
        return false;
    }
    let item = list.remove(from);
    list.insert(to, item);
    true
}

/// Applies to `spec` what the hover `over` asks of the dragged element
/// `item`. Returns the element at its new place, `None` if nothing changes
/// (other layer, wrong category, element already there).
pub fn step(spec: &mut PlotSpec, item: DragItem, over: DropOver) -> Option<DragItem> {
    if item.panel() != over.panel() {
        return None;
    }
    let panel = spec.panels.get_mut(item.panel())?;
    step_panel(panel, item, over).map(|(item, _)| item)
}

/// [`step`] on a single layer (the series window's draft), without a layer
/// check. Also returns the resulting series order (`order[new rank] = old
/// rank`, empty for a free object), which makes it possible to follow a
/// selected series.
pub fn step_panel(
    panel: &mut PanelSpec,
    item: DragItem,
    over: DropOver,
) -> Option<(DragItem, Vec<usize>)> {
    match item {
        DragItem::Series {
            panel: p,
            origin,
            current,
        } => {
            let (group, position) = series_destination(panel, current, over)?;
            let order = panel.place_series(current, group, position)?;
            let current = order.iter().position(|old| *old == current)?;
            Some((
                DragItem::Series {
                    panel: p,
                    origin,
                    current,
                },
                order,
            ))
        }
        DragItem::Group { panel: p, group } => {
            let position = group_destination(panel, group, over)?;
            let order = panel.place_group(group, position)?;
            Some((DragItem::Group { panel: p, group }, order))
        }
        DragItem::Text {
            panel: p,
            origin,
            current,
        } => {
            let DropOver::Text { index, .. } = over else {
                return None;
            };
            shift(&mut panel.annotations, current, index).then_some((
                DragItem::Text {
                    panel: p,
                    origin,
                    current: index,
                },
                Vec::new(),
            ))
        }
        DragItem::Shape {
            panel: p,
            origin,
            current,
        } => {
            let DropOver::Shape { index, .. } = over else {
                return None;
            };
            shift(&mut panel.shapes, current, index).then_some((
                DragItem::Shape {
                    panel: p,
                    origin,
                    current: index,
                },
                Vec::new(),
            ))
        }
    }
}

/// Places the free object `target` (text or shape) at rank `index` of its
/// category, in its layer. False if the target is not a free object, does not
/// exist or does not move.
pub fn reorder_object(spec: &mut PlotSpec, target: VisibilityTarget, index: usize) -> bool {
    match target {
        VisibilityTarget::Annotation(t) => spec
            .panels
            .get_mut(t.panel)
            .is_some_and(|panel| shift(&mut panel.annotations, t.index, index)),
        VisibilityTarget::Shape(t) => spec
            .panels
            .get_mut(t.panel)
            .is_some_and(|panel| shift(&mut panel.shapes, t.index, index)),
        _ => false,
    }
}

/// What the release asks of the model, deduced from the preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drop {
    Series {
        source: SeriesRef,
        group: Option<u32>,
        position: usize,
    },
    Group {
        group: SeriesGroupRef,
        position: usize,
    },
    Object {
        target: VisibilityTarget,
        index: usize,
    },
}

/// The single action of the release: `preview` is the spec as the gesture left
/// it, `item` the element at its final place. `None` if the preview changed
/// nothing compared with `base`.
pub fn finish(base: &PlotSpec, preview: &PlotSpec, item: DragItem) -> Option<Drop> {
    if preview == base {
        return None;
    }
    match item {
        DragItem::Series {
            panel,
            origin,
            current,
        } => {
            let (group, position) = preview.panels.get(panel)?.series_position(current)?;
            Some(Drop::Series {
                source: SeriesRef::new(panel, origin),
                group,
                position,
            })
        }
        DragItem::Group { panel, group } => {
            let slots = preview.panels.get(panel)?.slots();
            let position = slots
                .iter()
                .position(|slot| matches!(slot, SeriesSlot::Group { id, .. } if *id == group))?;
            Some(Drop::Group {
                group: SeriesGroupRef::new(panel, group),
                position,
            })
        }
        DragItem::Text {
            panel,
            origin,
            current,
        } => Some(Drop::Object {
            target: VisibilityTarget::Annotation(stokesforge_plot::AnnotationRef {
                panel,
                index: origin,
            }),
            index: current,
        }),
        DragItem::Shape {
            panel,
            origin,
            current,
        } => Some(Drop::Object {
            target: VisibilityTarget::Shape(stokesforge_plot::ShapeRef {
                panel,
                index: origin,
            }),
            index: current,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_core::NodeId;
    use stokesforge_plot::{Panels, SeriesSpec};

    /// A, [B C], D, [E F] — columns 1 to 6.
    fn spec() -> PlotSpec {
        let mut panel = PanelSpec::at(0, 0);
        panel.push_series_batch(vec![SeriesSpec::new(NodeId::from_raw(1))]);
        panel.push_series_batch(vec![
            SeriesSpec::new(NodeId::from_raw(2)),
            SeriesSpec::new(NodeId::from_raw(3)),
        ]);
        panel.push_series_batch(vec![SeriesSpec::new(NodeId::from_raw(4))]);
        panel.push_series_batch(vec![
            SeriesSpec::new(NodeId::from_raw(5)),
            SeriesSpec::new(NodeId::from_raw(6)),
        ]);
        PlotSpec::empty("Test").with_panels(Panels::new(vec![panel]))
    }

    fn columns(spec: &PlotSpec) -> Vec<u64> {
        spec.panels[0].series.iter().map(|s| s.y.raw()).collect()
    }

    fn series_item(index: usize) -> DragItem {
        DragItem::Series {
            panel: 0,
            origin: index,
            current: index,
        }
    }

    fn over_series(index: usize, after: bool) -> DropOver {
        DropOver::Series {
            panel: 0,
            index,
            after,
        }
    }

    #[test]
    fn two_lone_series_swap_when_they_touch() {
        let mut spec = spec();
        // A hovers D: A takes D's rank among the slots.
        let item = step(&mut spec, series_item(0), over_series(3, false)).unwrap();
        assert_eq!(columns(&spec), [2, 3, 4, 1, 5, 6]);
        assert_eq!(
            item,
            DragItem::Series {
                panel: 0,
                origin: 0,
                current: 3
            }
        );
        // Then it moves back up by hovering D (now above).
        let item = step(&mut spec, item, over_series(2, false)).unwrap();
        assert_eq!(columns(&spec), [2, 3, 1, 4, 5, 6]);
        assert_eq!(
            item.target(),
            VisibilityTarget::Series(SeriesRef::new(0, 2))
        );
    }

    #[test]
    fn hovering_a_member_enters_the_group_depending_on_the_half() {
        // D over B, upper half: D enters before B.
        let mut upper = spec();
        step(&mut upper, series_item(3), over_series(1, false)).unwrap();
        assert_eq!(columns(&upper), [1, 4, 2, 3, 5, 6]);
        assert_eq!(upper.panels[0].series[1].group, Some(1));
        // D over C, lower half: D enters after C.
        let mut lower = spec();
        step(&mut lower, series_item(3), over_series(2, true)).unwrap();
        assert_eq!(columns(&lower), [1, 2, 3, 4, 5, 6]);
        assert_eq!(lower.panels[0].series[3].group, Some(1));
    }

    #[test]
    fn hovering_a_lone_series_leaves_the_group() {
        let mut spec = spec();
        // B (group 1) over A, upper half: B moves to the head alone.
        step(&mut spec, series_item(1), over_series(0, false)).unwrap();
        assert_eq!(columns(&spec), [2, 1, 3, 4, 5, 6]);
        assert_eq!(spec.panels[0].series[0].group, None);
        assert_eq!(spec.panels[0].group_members(1), [2]);
    }

    #[test]
    fn a_group_header_splits_into_two_halves() {
        let header_over = |after| DropOver::Header {
            panel: 0,
            group: 2,
            after,
        };
        // Upper half: D is placed alone just before group 2 (it is already there).
        assert!(step(&mut spec(), series_item(3), header_over(false)).is_none());
        // A, upper half: A moves just before group 2.
        let mut spec = spec();
        step(&mut spec, series_item(0), header_over(false)).unwrap();
        assert_eq!(columns(&spec), [2, 3, 4, 1, 5, 6]);
        assert_eq!(spec.panels[0].series[3].group, None);
        // Lower half: A becomes the first member.
        let mut spec = self::spec();
        step(&mut spec, series_item(0), header_over(true)).unwrap();
        assert_eq!(columns(&spec), [2, 3, 4, 1, 5, 6]);
        assert_eq!(spec.panels[0].group_members(2), [3, 4, 5]);
    }

    #[test]
    fn the_end_zone_is_the_exit_of_a_group_that_ends_the_list() {
        let mut spec = spec();
        // F leaves group 2, which ends the list.
        let item = step(&mut spec, series_item(5), DropOver::End { panel: 0 }).unwrap();
        assert_eq!(columns(&spec), [1, 2, 3, 4, 5, 6]);
        assert_eq!(spec.panels[0].series[5].group, None);
        assert_eq!(
            item,
            DragItem::Series {
                panel: 0,
                origin: 5,
                current: 5
            }
        );
        // Already alone at the end of the list: nothing moves.
        assert!(step(&mut spec, item, DropOver::End { panel: 0 }).is_none());
    }

    #[test]
    fn a_group_moves_as_a_block_onto_the_hovered_slot() {
        let mut spec = spec();
        let group = DragItem::Group { panel: 0, group: 2 };
        step(&mut spec, group, over_series(0, false)).unwrap();
        assert_eq!(columns(&spec), [5, 6, 1, 2, 3, 4]);
        // Hovering one of its own members does nothing.
        assert!(step(&mut spec, group, over_series(0, true)).is_none());
        // The end zone sends it back to the bottom.
        step(&mut spec, group, DropOver::End { panel: 0 }).unwrap();
        assert_eq!(columns(&spec), [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn never_from_one_layer_to_another_nor_from_one_category_to_another() {
        let mut spec = spec();
        let elsewhere = DropOver::Series {
            panel: 1,
            index: 0,
            after: false,
        };
        assert!(step(&mut spec, series_item(0), elsewhere).is_none());
        assert!(step(
            &mut spec,
            series_item(0),
            DropOver::Text { panel: 0, index: 0 }
        )
        .is_none());
        let text = DragItem::Text {
            panel: 0,
            origin: 0,
            current: 0,
        };
        assert!(step(&mut spec, text, DropOver::Shape { panel: 0, index: 0 }).is_none());
        assert!(step(&mut spec, text, over_series(1, false)).is_none());
    }

    #[test]
    fn free_objects_keep_their_category() {
        use stokesforge_plot::{AnnotationSpec, ShapeKind, ShapeSpec};
        let mut spec = spec();
        let panel = spec.panels.get_mut(0).unwrap();
        for label in ["a", "b", "c"] {
            panel.annotations.push(AnnotationSpec::new(label, 0.0, 0.0));
        }
        panel
            .shapes
            .push(ShapeSpec::new(ShapeKind::Line, 0.0, 0.0, 1.0, 1.0));
        panel
            .shapes
            .push(ShapeSpec::new(ShapeKind::Arrow, 0.0, 0.0, 1.0, 1.0));
        let text = DragItem::Text {
            panel: 0,
            origin: 0,
            current: 0,
        };
        let item = step(&mut spec, text, DropOver::Text { panel: 0, index: 2 }).unwrap();
        let order: Vec<_> = spec.panels[0]
            .annotations
            .iter()
            .map(|a| a.text.as_str())
            .collect();
        assert_eq!(order, ["b", "c", "a"]);
        assert_eq!(spec.panels[0].shapes.len(), 2, "the shapes did not move");
        let drop = finish(&self::spec_with(&spec), &spec, item);
        assert!(
            matches!(drop, Some(Drop::Object { index: 2, .. })),
            "{drop:?}"
        );
    }

    /// The spec from before the gesture: the free objects of `spec` in their
    /// original order (a, b, c).
    fn spec_with(after: &PlotSpec) -> PlotSpec {
        let mut before = after.clone();
        before
            .panels
            .get_mut(0)
            .unwrap()
            .annotations
            .rotate_right(1);
        before
    }

    #[test]
    fn the_release_boils_down_to_one_destination() {
        let base = spec();
        let mut preview = base.clone();
        let item = step(&mut preview, series_item(3), over_series(1, false)).unwrap();
        let drop = finish(&base, &preview, item).unwrap();
        assert_eq!(
            drop,
            Drop::Series {
                source: SeriesRef::new(0, 3),
                group: Some(1),
                position: 0
            }
        );
        // Replayed on the real spec, the destination gives the preview back.
        let mut replayed = base.clone();
        let (group, position) = match drop {
            Drop::Series {
                group, position, ..
            } => (group, position),
            _ => unreachable!(),
        };
        replayed
            .panels
            .get_mut(0)
            .unwrap()
            .place_series(3, group, position)
            .unwrap();
        assert_eq!(replayed, preview);
        // With no change, nothing to ask.
        assert!(finish(&base, &base, item).is_none());
    }

    #[test]
    fn reorder_object_rejects_what_is_not_a_free_object() {
        let mut spec = spec();
        assert!(!reorder_object(
            &mut spec,
            VisibilityTarget::Series(SeriesRef::new(0, 0)),
            1
        ));
        assert!(!reorder_object(&mut spec, VisibilityTarget::Panel(0), 0));
    }
}
