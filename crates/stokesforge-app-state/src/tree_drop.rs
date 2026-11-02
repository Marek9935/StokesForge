//! The pure decisions of drag and drop in the Project Manager tree (#425).
//!
//! Three small questions, answered without a window:
//!
//! - [`drop_target`]: given the displayed tree, the dragged rows, the hovered
//!   row and where the pointer sits in it (0 = top edge, 1 = bottom edge),
//!   where would a release put the rows? A folder row: top quarter before it,
//!   middle inside it, bottom quarter after it (before its first child when it
//!   is open with children). A window row: upper half before, lower half after.
//!   Hovering a dragged row or something inside it gives no target.
//! - [`hover_opens_folder`]: a closed folder hovered for a second opens
//!   (half a second does not: it would open while crossing the tree).
//! - [`final_order`] and [`plan_moves`]: the order a folder ends with after an
//!   insertion, and the fewest `Move`s that turn the model order into it.
//!
//! Every threshold sits in a named constant, easy to change.

use stokesforge_core::NodeId;

use crate::state::ProjectTreeNode;

/// Share of a folder row, from the top (and from the bottom), where a release
/// inserts before (after) the folder instead of dropping inside it.
pub const FOLDER_EDGE_FRACTION: f32 = 0.25;

/// Time a dragged row must hover a closed folder before it opens, in seconds.
pub const OPEN_AFTER_SECS: f64 = 1.0;

/// Where, inside the folder `parent`, the dragged rows go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovePlace {
    /// At the end of the folder; its order mode is left as it is.
    Inside,
    /// Before the child `anchor`; the folder becomes manual.
    Before(NodeId),
    /// After the last child; the folder becomes manual.
    AtEnd,
}

/// A drop destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropTarget {
    /// Folder that receives the rows.
    pub parent: NodeId,
    /// Place in that folder.
    pub place: MovePlace,
}

impl DropTarget {
    /// The folder to highlight when the drop lands inside it (not an insertion line).
    pub fn highlighted_folder(&self) -> Option<NodeId> {
        (self.place == MovePlace::Inside).then_some(self.parent)
    }
}

/// True once a dragged row has hovered a closed folder long enough.
pub fn hover_opens_folder(hovered_secs: f64) -> bool {
    hovered_secs >= OPEN_AFTER_SECS
}

/// The drop target below the last row: inside the root.
pub fn drop_on_empty_area(root: NodeId) -> DropTarget {
    DropTarget {
        parent: root,
        place: MovePlace::Inside,
    }
}

/// Where the pointer sits, `fraction` from the top of the hovered row, giving
/// the target, or `None` when hovering a dragged row or its subtree. `tree` is
/// the displayed tree (the children of `root`), `is_open` tells whether a folder
/// is expanded in the panel.
pub fn drop_target(
    tree: &[ProjectTreeNode],
    root: NodeId,
    dragged: &[NodeId],
    hovered: NodeId,
    fraction: f32,
    is_open: &dyn Fn(NodeId) -> bool,
) -> Option<DropTarget> {
    let (siblings, parent, index, inside_dragged) = locate(tree, root, dragged, hovered, false)?;
    if inside_dragged {
        return None;
    }
    let node = siblings.get(index)?;
    let before = |anchor: usize, siblings: &[ProjectTreeNode], parent: NodeId| {
        Some(skip_dragged(siblings, anchor, dragged, parent))
    };
    match node {
        ProjectTreeNode::Window { .. } => {
            if fraction < 0.5 {
                before(index, siblings, parent)
            } else {
                before(index + 1, siblings, parent)
            }
        }
        ProjectTreeNode::Folder { id, children, .. } => {
            if fraction < FOLDER_EDGE_FRACTION {
                before(index, siblings, parent)
            } else if fraction <= 1.0 - FOLDER_EDGE_FRACTION {
                Some(DropTarget {
                    parent: *id,
                    place: MovePlace::Inside,
                })
            } else if is_open(*id) && !children.is_empty() {
                before(0, children, *id)
            } else {
                before(index + 1, siblings, parent)
            }
        }
    }
}

/// Insertion before the sibling at `anchor` (or at the end), moving on past
/// dragged siblings: they leave the list, they cannot be an anchor.
fn skip_dragged(
    siblings: &[ProjectTreeNode],
    anchor: usize,
    dragged: &[NodeId],
    parent: NodeId,
) -> DropTarget {
    let next = siblings
        .iter()
        .skip(anchor)
        .map(ProjectTreeNode::id)
        .find(|id| !dragged.contains(id));
    DropTarget {
        parent,
        place: next.map_or(MovePlace::AtEnd, MovePlace::Before),
    }
}

/// Finds `hovered`: its siblings, its parent, its index, and whether it is a
/// dragged row or sits under one.
fn locate<'a>(
    nodes: &'a [ProjectTreeNode],
    parent: NodeId,
    dragged: &[NodeId],
    hovered: NodeId,
    under_dragged: bool,
) -> Option<(&'a [ProjectTreeNode], NodeId, usize, bool)> {
    for (index, node) in nodes.iter().enumerate() {
        let id = node.id();
        let dragged_here = under_dragged || dragged.contains(&id);
        if id == hovered {
            return Some((nodes, parent, index, dragged_here));
        }
        if let ProjectTreeNode::Folder { children, .. } = node {
            if let Some(found) = locate(children, id, dragged, hovered, dragged_here) {
                return Some(found);
            }
        }
    }
    None
}

/// The order of a folder after an insertion: `displayed` (the order shown, hidden
/// children included) without the dragged rows, which are inserted at `place`
/// in the order given.
pub fn final_order(displayed: &[NodeId], dragged: &[NodeId], place: MovePlace) -> Vec<NodeId> {
    let mut rest: Vec<NodeId> = displayed
        .iter()
        .copied()
        .filter(|id| !dragged.contains(id))
        .collect();
    let at = match place {
        MovePlace::Before(anchor) => rest
            .iter()
            .position(|id| *id == anchor)
            .unwrap_or(rest.len()),
        MovePlace::Inside | MovePlace::AtEnd => rest.len(),
    };
    rest.splice(at..at, dragged.iter().copied());
    rest
}

/// The fewest moves `(id, index)` that turn the model order `current` into
/// `wanted`. Each index is where the row goes **after it was taken out** of its
/// place, the way `Command::Move` counts. `wanted` holds every child of the
/// folder, plus the rows coming from another folder; a row that comes from
/// elsewhere is always moved. The rows of the longest run that is already in the
/// wanted order stay, the others move: sliding one row across 500 takes one
/// move, not 499.
pub fn plan_moves(current: &[NodeId], wanted: &[NodeId]) -> Vec<(NodeId, usize)> {
    // Wanted rank of each current child that is wanted; the longest increasing
    // run of those ranks is what stays in place.
    let rank = |id: &NodeId| wanted.iter().position(|w| w == id);
    let ranks: Vec<(NodeId, usize)> = current
        .iter()
        .filter_map(|id| rank(id).map(|r| (*id, r)))
        .collect();
    let keep = longest_increasing(&ranks.iter().map(|(_, r)| *r).collect::<Vec<_>>());
    let stays: Vec<NodeId> = keep.iter().map(|k| ranks[*k].0).collect();

    let mut now: Vec<NodeId> = current.to_vec();
    let mut moves = Vec::new();
    for (i, id) in wanted.iter().enumerate() {
        if stays.contains(id) {
            continue;
        }
        if let Some(pos) = now.iter().position(|x| x == id) {
            now.remove(pos);
        }
        let index = if i == 0 {
            0
        } else {
            now.iter()
                .position(|x| *x == wanted[i - 1])
                .map_or(now.len(), |p| p + 1)
        };
        now.insert(index, *id);
        moves.push((*id, index));
    }
    moves
}

/// Indices of a longest strictly increasing subsequence of `values`.
fn longest_increasing(values: &[usize]) -> Vec<usize> {
    let mut tails: Vec<usize> = Vec::new(); // index in `values` ending a run of each length
    let mut prev: Vec<Option<usize>> = vec![None; values.len()];
    for (i, v) in values.iter().enumerate() {
        let at = tails.partition_point(|t| values[*t] < *v);
        prev[i] = at.checked_sub(1).map(|p| tails[p]);
        if at == tails.len() {
            tails.push(i);
        } else {
            tails[at] = i;
        }
    }
    let mut out = Vec::new();
    let mut cursor = tails.last().copied();
    while let Some(i) = cursor {
        out.push(i);
        cursor = prev[i];
    }
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::WindowKind;

    fn n(raw: u64) -> NodeId {
        NodeId::from_raw(raw)
    }

    fn win(raw: u64) -> ProjectTreeNode {
        ProjectTreeNode::Window {
            id: n(raw),
            name: format!("W{raw}"),
            kind: WindowKind::Book,
        }
    }

    fn folder(raw: u64, children: Vec<ProjectTreeNode>) -> ProjectTreeNode {
        ProjectTreeNode::Folder {
            id: n(raw),
            name: format!("F{raw}"),
            children,
        }
    }

    /// root: F10 [W11, W12], W20, F30 [], W40
    fn tree() -> Vec<ProjectTreeNode> {
        vec![
            folder(10, vec![win(11), win(12)]),
            win(20),
            folder(30, vec![]),
            win(40),
        ]
    }

    fn target(hovered: u64, fraction: f32, dragged: &[u64], open: &[u64]) -> Option<DropTarget> {
        let dragged: Vec<NodeId> = dragged.iter().map(|r| n(*r)).collect();
        let open: Vec<NodeId> = open.iter().map(|r| n(*r)).collect();
        drop_target(&tree(), n(1), &dragged, n(hovered), fraction, &|id| {
            open.contains(&id)
        })
    }

    fn at(parent: u64, place: MovePlace) -> Option<DropTarget> {
        Some(DropTarget {
            parent: n(parent),
            place,
        })
    }

    #[test]
    fn a_folder_row_has_three_zones() {
        // Closed folder: before, inside, after (before the next sibling).
        assert_eq!(target(30, 0.1, &[40], &[]), at(1, MovePlace::Before(n(30))));
        assert_eq!(target(30, 0.5, &[40], &[]), at(30, MovePlace::Inside));
        assert_eq!(target(30, 0.9, &[20], &[]), at(1, MovePlace::Before(n(40))));
        // Last row of its list: after = at the end.
        assert_eq!(target(40, 0.9, &[20], &[]), at(1, MovePlace::AtEnd));
        // Open folder with children: the bottom quarter is before its first child.
        assert_eq!(
            target(10, 0.9, &[20], &[10]),
            at(10, MovePlace::Before(n(11)))
        );
        // Open but empty folder: after it.
        assert_eq!(
            target(30, 0.9, &[20], &[30]),
            at(1, MovePlace::Before(n(40)))
        );
        // A closed folder with children: after it.
        assert_eq!(target(10, 0.9, &[40], &[]), at(1, MovePlace::Before(n(20))));
    }

    #[test]
    fn a_window_row_has_two_zones() {
        assert_eq!(target(20, 0.4, &[40], &[]), at(1, MovePlace::Before(n(20))));
        assert_eq!(target(20, 0.6, &[40], &[]), at(1, MovePlace::Before(n(30))));
        assert_eq!(
            target(11, 0.6, &[40], &[10]),
            at(10, MovePlace::Before(n(12)))
        );
        assert_eq!(target(12, 0.6, &[40], &[10]), at(10, MovePlace::AtEnd));
    }

    #[test]
    fn a_dragged_row_is_never_an_anchor() {
        // Dragging W20 and hovering the top of F30: before F30; dragging F30
        // itself over the bottom of W20 would anchor on F30, so it moves on.
        assert_eq!(target(20, 0.6, &[30], &[]), at(1, MovePlace::Before(n(40))));
        assert_eq!(target(20, 0.6, &[30, 40], &[]), at(1, MovePlace::AtEnd));
    }

    #[test]
    fn hovering_a_dragged_row_or_its_subtree_gives_no_target() {
        assert_eq!(target(20, 0.5, &[20], &[]), None);
        assert_eq!(target(10, 0.5, &[10], &[10]), None);
        assert_eq!(target(11, 0.5, &[10], &[10]), None);
        assert_eq!(target(12, 0.1, &[10, 20], &[10]), None);
        assert_eq!(target(99, 0.5, &[20], &[]), None);
    }

    #[test]
    fn the_empty_area_drops_inside_the_root() {
        assert_eq!(drop_on_empty_area(n(1)).place, MovePlace::Inside);
        assert_eq!(drop_on_empty_area(n(1)).parent, n(1));
    }

    #[test]
    fn a_closed_folder_opens_after_one_second_not_half_a_second() {
        assert!(!hover_opens_folder(0.5));
        assert!(!hover_opens_folder(0.99));
        assert!(hover_opens_folder(1.0));
    }

    #[test]
    fn final_order_inserts_the_dragged_rows_at_the_anchor() {
        let shown = [n(1), n(2), n(3), n(4)];
        assert_eq!(
            final_order(&shown, &[n(4)], MovePlace::Before(n(2))),
            vec![n(1), n(4), n(2), n(3)]
        );
        assert_eq!(
            final_order(&shown, &[n(1), n(2)], MovePlace::AtEnd),
            vec![n(3), n(4), n(1), n(2)]
        );
        // From another folder: the row joins.
        assert_eq!(
            final_order(&shown, &[n(9)], MovePlace::Before(n(3))),
            vec![n(1), n(2), n(9), n(3), n(4)]
        );
    }

    /// Plays the moves the way `Command::Move` does.
    fn play(current: &[NodeId], moves: &[(NodeId, usize)]) -> Vec<NodeId> {
        let mut now = current.to_vec();
        for (id, index) in moves {
            if let Some(p) = now.iter().position(|x| x == id) {
                now.remove(p);
            }
            now.insert(*index, *id);
        }
        now
    }

    #[test]
    fn the_planned_moves_reach_the_wanted_order_forward_and_backward() {
        let current: Vec<NodeId> = (1..=6).map(n).collect();
        for wanted in [
            vec![n(2), n(3), n(4), n(1), n(5), n(6)],
            vec![n(1), n(2), n(6), n(3), n(4), n(5)],
            vec![n(6), n(5), n(4), n(3), n(2), n(1)],
            vec![n(1), n(9), n(2), n(3), n(4), n(5), n(6)],
            current.clone(),
        ] {
            let moves = plan_moves(&current, &wanted);
            assert_eq!(play(&current, &moves), wanted, "{moves:?}");
        }
        assert!(plan_moves(&current, &current).is_empty());
    }

    #[test]
    fn sliding_one_row_across_many_takes_one_move() {
        let current: Vec<NodeId> = (1..=500).map(n).collect();
        let mut wanted = current.clone();
        let first = wanted.remove(0);
        wanted.push(first);
        assert_eq!(plan_moves(&current, &wanted).len(), 1);
        let mut wanted = current.clone();
        let last = wanted.pop().unwrap();
        wanted.insert(0, last);
        assert_eq!(plan_moves(&current, &wanted).len(), 1);
    }
}
