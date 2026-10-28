//! Moving columns within their sheet (#319): the pure logic of the `Move ▸`
//! submenu — which order each gesture aims at, and which sequence of
//! `Command::Move` leads there. No interface state, no command executed here:
//! `AppState` calls it, the menu queries it to grey out its entries.
//!
//! # Rules (decisions of 2026-10-21)
//!
//! - **Same sheet only.** An order is that of the visible columns of a
//!   sheet.
//! - **Left / Right**: each selected column advances one place, and the gap
//!   between non-contiguous columns stays (`A [B] C [D] E` → `B A D C E`,
//!   not a tightened block `B D A C E`).
//! - **First / Last**: the selected columns gather at the start or at the
//!   end, in their order.
//! - **All or nothing**: if the gesture would change nothing — or if just one
//!   of the selected columns cannot advance (`[A] C` or `[A] [C]`, Left) —
//!   there is no target order. The menu then greys out the entry, with its
//!   reason.
//!
//! # From the target order to commands
//!
//! `Command::Move { id, new_parent, index }` counts the index **after
//! removal** of the node (`Tree::move_node`). An `Unknown` child can live
//! under a sheet (invariant 10) without being a visible column: the order is
//! computed on the visible columns, then the index is translated into the
//! **full** list of children ([`plan`]). Only the selected columns move, one
//! after another starting from the edge they head to: the order is that of an
//! advance without jumping over (ascending for Left and First, descending for
//! Right and Last), so that each column finds its final place without the
//! following ones undoing it.

use stokesforge_core::NodeId;

/// Where the selection goes: the four entries of the `Move ▸` submenu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnMove {
    /// To the start of the sheet, in the selection's order.
    First,
    /// One place to the left, each column.
    Left,
    /// One place to the right, each column.
    Right,
    /// To the end of the sheet, in the selection's order.
    Last,
}

/// One step: `Command::Move { id, new_parent: <the sheet>, index }`, the index
/// counting **after removal** of `id` in the full list of children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveStep {
    /// Column to move.
    pub id: NodeId,
    /// Destination position, after removal.
    pub index: usize,
}

/// The order aimed at by `direction` for the selection `selected` over the
/// visible columns `order`, or `None` if the gesture would change nothing
/// (greyed out). Identifiers of `selected` absent from `order` are ignored;
/// with no selected column in `order`, `None`.
pub fn target_order(
    order: &[NodeId],
    selected: &[NodeId],
    direction: ColumnMove,
) -> Option<Vec<NodeId>> {
    let is_selected = |id: &NodeId| selected.contains(id);
    if !order.iter().any(is_selected) {
        return None;
    }
    let target = match direction {
        ColumnMove::First | ColumnMove::Last => {
            let (chosen, others): (Vec<NodeId>, Vec<NodeId>) =
                order.iter().copied().partition(is_selected);
            if direction == ColumnMove::First {
                [chosen, others].concat()
            } else {
                [others, chosen].concat()
            }
        }
        ColumnMove::Left => {
            if order.first().is_some_and(is_selected) {
                return None;
            }
            let mut target = order.to_vec();
            for position in 1..target.len() {
                if is_selected(&target[position]) {
                    target.swap(position - 1, position);
                }
            }
            target
        }
        ColumnMove::Right => {
            if order.last().is_some_and(is_selected) {
                return None;
            }
            let mut target = order.to_vec();
            for position in (0..target.len().saturating_sub(1)).rev() {
                if is_selected(&target[position]) {
                    target.swap(position, position + 1);
                }
            }
            target
        }
    };
    (target != order).then_some(target)
}

/// The sequence of moves that leads `children` (all the sheet's children,
/// visible columns and others) to the order `direction` for `selected` over
/// the visible columns `order`. `None` when [`target_order`] has nothing to do.
/// The steps are to be executed in order, each on the state left by the
/// previous one; a column already in place has no step.
pub fn plan(
    children: &[NodeId],
    order: &[NodeId],
    selected: &[NodeId],
    direction: ColumnMove,
) -> Option<Vec<MoveStep>> {
    let target = target_order(order, selected, direction)?;
    let mut moving: Vec<NodeId> = target
        .iter()
        .copied()
        .filter(|id| selected.contains(id))
        .collect();
    if matches!(direction, ColumnMove::Right | ColumnMove::Last) {
        moving.reverse();
    }
    let mut current = children.to_vec();
    let mut steps = Vec::new();
    for id in moving {
        let wanted = target.iter().position(|other| *other == id)?;
        let before = current.clone();
        current.retain(|child| *child != id);
        let index = full_index(&current, &target, wanted);
        current.insert(index, id);
        if current != before {
            steps.push(MoveStep { id, index });
        }
    }
    Some(steps)
}

/// Translates "position `wanted` among the visible columns" into a position in
/// the full list `current` (without the moving column): just before the
/// visible column that occupies `wanted`, or right after the last visible
/// column when `wanted` is the end.
fn full_index(current: &[NodeId], target: &[NodeId], wanted: usize) -> usize {
    let visible: Vec<usize> = current
        .iter()
        .enumerate()
        .filter(|(_, child)| target.contains(child))
        .map(|(position, _)| position)
        .collect();
    match visible.get(wanted) {
        Some(position) => *position,
        None => visible.last().map_or(current.len(), |last| last + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(names: &str) -> Vec<NodeId> {
        names
            .chars()
            .map(|c| NodeId::from_raw(u64::from(c as u32 - 'A' as u32) + 1))
            .collect()
    }

    fn names(ids: &[NodeId]) -> String {
        ids.iter()
            .map(|id| char::from_u32(id.raw() as u32 - 1 + 'A' as u32).unwrap())
            .collect()
    }

    fn target(order: &str, selected: &str, direction: ColumnMove) -> Option<String> {
        target_order(&ids(order), &ids(selected), direction).map(|t| names(&t))
    }

    /// `A [B] C [D] E`, Left: each column steps back one place, the gap
    /// stays. Witness: tightening the block would give `B D A C E`.
    #[test]
    fn left_keeps_the_gap_between_non_contiguous_columns() {
        assert_eq!(target("ABCDE", "BD", ColumnMove::Left).unwrap(), "BADCE");
        assert_eq!(target("ABCDE", "BD", ColumnMove::Right).unwrap(), "ACBED");
    }

    #[test]
    fn first_and_last_gather_the_selection_in_its_order() {
        assert_eq!(target("ABCDE", "BD", ColumnMove::First).unwrap(), "BDACE");
        assert_eq!(target("ABCDE", "BD", ColumnMove::Last).unwrap(), "ACEBD");
        // The order is the sheet's, not the one in which the selection is given.
        assert_eq!(target("ABCDE", "DB", ColumnMove::First).unwrap(), "BDACE");
    }

    /// All or nothing: a column that cannot advance blocks the gesture.
    /// Witness: moving only C.
    #[test]
    fn a_column_that_cannot_move_greys_the_whole_gesture() {
        assert_eq!(target("ABC", "AC", ColumnMove::Left), None);
        assert_eq!(target("AC", "A", ColumnMove::Left), None);
        assert_eq!(target("ABC", "AB", ColumnMove::Left), None);
        assert_eq!(target("ABC", "BC", ColumnMove::Right), None);
        assert_eq!(target("ABC", "A", ColumnMove::Right).unwrap(), "BAC");
    }

    #[test]
    fn a_gesture_that_changes_nothing_has_no_target() {
        assert_eq!(target("ABCDE", "AB", ColumnMove::First), None);
        assert_eq!(target("ABCDE", "DE", ColumnMove::Last), None);
        assert_eq!(target("ABCDE", "ABCDE", ColumnMove::First), None);
        assert_eq!(target("ABCDE", "", ColumnMove::Left), None);
        // A column from another sheet is not in the order: ignored.
        assert_eq!(target("ABC", "Z", ColumnMove::First), None);
    }

    /// Replays a sequence of steps on a list of nodes, counting the index
    /// after removal like `Tree::move_node`.
    fn replay(children: &[NodeId], steps: &[MoveStep]) -> Vec<NodeId> {
        let mut list = children.to_vec();
        for step in steps {
            list.retain(|id| *id != step.id);
            assert!(step.index <= list.len(), "index out of bounds");
            list.insert(step.index, step.id);
        }
        list
    }

    /// The steps lead exactly to the target order, for every selection and
    /// the four directions, on a sheet of six columns. Witness: counting the
    /// index before removal (out of bounds or wrong place).
    #[test]
    fn the_steps_reach_the_target_order_for_every_selection() {
        let order = ids("ABCDEF");
        for mask in 1u32..(1 << 6) {
            let selected: Vec<NodeId> = (0..6)
                .filter(|bit| mask & (1 << bit) != 0)
                .map(|bit| order[bit])
                .collect();
            for direction in [
                ColumnMove::First,
                ColumnMove::Left,
                ColumnMove::Right,
                ColumnMove::Last,
            ] {
                let wanted = target_order(&order, &selected, direction);
                let steps = plan(&order, &order, &selected, direction);
                assert_eq!(wanted.is_some(), steps.is_some());
                if let (Some(wanted), Some(steps)) = (wanted, steps) {
                    assert_eq!(
                        replay(&order, &steps),
                        wanted,
                        "{mask:b} {direction:?} : {steps:?}"
                    );
                    // Only the selected columns move.
                    assert!(steps.iter().all(|step| selected.contains(&step.id)));
                    assert!(!steps.is_empty());
                }
            }
        }
    }

    /// An `Unknown` child under the sheet is not a visible column: it keeps
    /// its relative place, and the index is counted in the full list.
    #[test]
    fn an_unknown_child_keeps_its_place_and_indexes_are_full_list_indexes() {
        // Children: A, ?, B, C, ?; visible columns: A B C.
        let unknown_1 = NodeId::from_raw(100);
        let unknown_2 = NodeId::from_raw(101);
        let [a, b, c] = [ids("A")[0], ids("B")[0], ids("C")[0]];
        let children = vec![a, unknown_1, b, c, unknown_2];
        let order = vec![a, b, c];
        for (selected, direction) in [
            (vec![b], ColumnMove::Left),
            (vec![b], ColumnMove::First),
            (vec![b, c], ColumnMove::First),
            (vec![a], ColumnMove::Right),
            (vec![a], ColumnMove::Last),
            (vec![a, b], ColumnMove::Last),
        ] {
            let wanted = target_order(&order, &selected, direction).unwrap();
            let steps = plan(&children, &order, &selected, direction).unwrap();
            let after = replay(&children, &steps);
            let visible: Vec<NodeId> = after
                .iter()
                .copied()
                .filter(|id| order.contains(id))
                .collect();
            assert_eq!(visible, wanted, "{selected:?} {direction:?}");
            let unknowns: Vec<NodeId> = after
                .iter()
                .copied()
                .filter(|id| !order.contains(id))
                .collect();
            assert_eq!(unknowns, vec![unknown_1, unknown_2]);
            assert_eq!(after.len(), children.len());
        }
    }

    /// A single column moving: a single step, on that column.
    #[test]
    fn one_selected_column_makes_one_step() {
        let order = ids("ABC");
        let steps = plan(&order, &order, &ids("B"), ColumnMove::Right).unwrap();
        assert_eq!(
            steps,
            vec![MoveStep {
                id: ids("B")[0],
                index: 2
            }]
        );
        let steps = plan(&order, &order, &ids("C"), ColumnMove::First).unwrap();
        assert_eq!(
            steps,
            vec![MoveStep {
                id: ids("C")[0],
                index: 0
            }]
        );
    }
}
