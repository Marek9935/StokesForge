//! Drag and drop in the Project Manager tree (#425), interface side: the
//! payload, the target under the pointer, and the feedback.
//!
//! The decisions — which target a pointer position means, when a hovered
//! closed folder opens — are pure and live in
//! `stokesforge_app_state::tree_drop`. Here: remember what is grabbed, find the
//! row under the pointer from the rectangles painted this frame, and paint the
//! folder highlight or the insertion line with the theme colours.
//!
//! The payload has a type of its own, [`PmDrag`], distinct from the Object
//! Manager's `RowDrag` (`reorder_list.rs`): the two gestures never answer to
//! each other. egui drops the payload on Escape and on release; a release
//! with no payload is not a drop, so Escape cancels without any code here.
//! The drag always moves (no Ctrl copy) and nothing leaves the panel: a release
//! outside the rows area asks for nothing.

use std::collections::HashMap;

use eframe::egui;
use stokesforge_app_state::state::{Action, ProjectTreeNode};
use stokesforge_app_state::tree_drop::{
    drop_on_empty_area, drop_target, hover_opens_folder, DropTarget, MovePlace,
};
use stokesforge_core::NodeId;

/// What the drag carries: the rows, in reading order.
pub(super) struct PmDrag {
    pub(super) ids: Vec<NodeId>,
}

/// The rows grabbed, as the payload of the drag in progress.
pub(super) fn begin(ctx: &egui::Context, ids: Vec<NodeId>) {
    egui::DragAndDrop::set_payload(ctx, PmDrag { ids });
}

/// The rows being dragged, if a Project Manager drag is in progress.
pub(super) fn dragged(ctx: &egui::Context) -> Option<std::sync::Arc<PmDrag>> {
    egui::DragAndDrop::payload::<PmDrag>(ctx)
}

/// Where the pointer is for the tree: the row it is over, and how far down it.
fn row_under(
    rects: &HashMap<NodeId, egui::Rect>,
    pos: egui::Pos2,
    slack: f32,
) -> Option<(NodeId, f32)> {
    rects
        .iter()
        .filter(|(_, rect)| pos.y >= rect.top() && pos.y <= rect.bottom() + slack)
        .min_by(|a, b| a.1.top().total_cmp(&b.1.top()))
        .map(|(id, rect)| {
            let fraction = ((pos.y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0);
            (*id, fraction)
        })
}

/// What a frame of drag decides.
pub(super) struct Frame {
    /// The target under the pointer, if the rows can go there.
    pub(super) target: Option<DropTarget>,
    /// The folder under the pointer, if it is closed: a candidate to open.
    pub(super) closed_folder: Option<NodeId>,
}

/// The target under the pointer. `area` is the visible rows area: outside it, no
/// target; inside it but below the last row, the root.
#[allow(clippy::too_many_arguments)]
pub(super) fn frame(
    tree: &[ProjectTreeNode],
    root: NodeId,
    dragged: &[NodeId],
    rects: &HashMap<NodeId, egui::Rect>,
    area: egui::Rect,
    pos: egui::Pos2,
    slack: f32,
    is_open: &dyn Fn(NodeId) -> bool,
) -> Frame {
    if !area.contains(pos) {
        return Frame {
            target: None,
            closed_folder: None,
        };
    }
    let Some((hovered, fraction)) = row_under(rects, pos, slack) else {
        return Frame {
            target: Some(drop_on_empty_area(root)),
            closed_folder: None,
        };
    };
    let target = drop_target(tree, root, dragged, hovered, fraction, is_open);
    let closed_folder = target
        .is_some()
        .then(|| is_folder(tree, hovered).then_some(hovered))
        .flatten()
        .filter(|id| !is_open(*id));
    Frame {
        target,
        closed_folder,
    }
}

fn is_folder(tree: &[ProjectTreeNode], id: NodeId) -> bool {
    tree.iter().any(|node| match node {
        ProjectTreeNode::Folder {
            id: folder,
            children,
            ..
        } => *folder == id || is_folder(children, id),
        ProjectTreeNode::Window { .. } => false,
    })
}

/// How long the pointer has been on the same closed folder; `since` is kept by
/// the caller between frames. Returns the folder to open when the delay is up.
pub(super) fn hover_open(
    since: &mut Option<(NodeId, f64)>,
    candidate: Option<NodeId>,
    now: f64,
) -> Option<NodeId> {
    match (candidate, *since) {
        (None, _) => {
            *since = None;
            None
        }
        (Some(id), Some((held, start))) if held == id => {
            hover_opens_folder(now - start).then(|| {
                *since = None;
                id
            })
        }
        (Some(id), _) => {
            *since = Some((id, now));
            None
        }
    }
}

/// The action a release asks for.
pub(super) fn drop_action(ids: &[NodeId], target: DropTarget) -> Action {
    Action::MoveObjects {
        ids: ids.to_vec(),
        parent: target.parent,
        place: target.place,
    }
}

/// Paints the feedback of `target`: the folder highlighted when the rows go
/// inside it, otherwise a line where they would be inserted.
pub(super) fn paint_feedback(
    painter: &egui::Painter,
    tree: &[ProjectTreeNode],
    root: NodeId,
    target: DropTarget,
    rects: &HashMap<NodeId, egui::Rect>,
    area: egui::Rect,
) {
    let painter = painter.with_clip_rect(area);
    if let Some(folder) = target.highlighted_folder() {
        if folder == root {
            // Below the last row: the whole area is the root.
            painter.rect_stroke(
                area.shrink(1.0),
                2.0,
                crate::theme::drop_edge_band_stroke(),
                egui::StrokeKind::Inside,
            );
            return;
        }
        if let Some(rect) = rects.get(&folder) {
            painter.rect_filled(*rect, 2.0, crate::theme::drop_edge_band_fill());
            painter.rect_stroke(
                *rect,
                2.0,
                crate::theme::drop_edge_band_stroke(),
                egui::StrokeKind::Inside,
            );
        }
        return;
    }
    let line = match target.place {
        MovePlace::Before(anchor) => rects.get(&anchor).map(|r| (r.left(), r.right(), r.top())),
        MovePlace::AtEnd => children_of(tree, root, target.parent)
            .and_then(|children| children.last())
            .and_then(|last| {
                let left = rects.get(&last.id())?.left();
                let bottom = subtree_ids(last)
                    .iter()
                    .filter_map(|id| rects.get(id))
                    .map(|r| r.bottom())
                    .fold(f32::MIN, f32::max);
                let right = rects.get(&last.id())?.right();
                Some((left, right, bottom))
            }),
        MovePlace::Inside => None,
    };
    if let Some((left, right, y)) = line {
        let stroke = egui::Stroke::new(2.0, crate::theme::drop_target_stroke().color);
        painter.hline(left..=right, y, stroke);
    }
}

fn children_of(
    tree: &[ProjectTreeNode],
    root: NodeId,
    parent: NodeId,
) -> Option<&[ProjectTreeNode]> {
    if parent == root {
        return Some(tree);
    }
    tree.iter().find_map(|node| match node {
        ProjectTreeNode::Folder { id, children, .. } => {
            if *id == parent {
                Some(children.as_slice())
            } else {
                children_of(children, root, parent)
            }
        }
        ProjectTreeNode::Window { .. } => None,
    })
}

fn subtree_ids(node: &ProjectTreeNode) -> Vec<NodeId> {
    let mut out = vec![node.id()];
    if let ProjectTreeNode::Folder { children, .. } = node {
        for child in children {
            out.extend(subtree_ids(child));
        }
    }
    out
}
