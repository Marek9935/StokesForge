//! Drag and drop of series, groups and free objects, on the interface side:
//! the state of the gesture in progress, shared by the Object Manager and the
//! series window (`curve_settings.rs`).
//!
//! The decision — where the item goes according to the hovered row — is pure
//! and lives in `stokesforge_app_state::object_drag`. Here: remember what is
//! grabbed, read where the pointer is, and, for the Object Manager, hold the
//! **preview**.
//!
//! The Object Manager reads the snapshot's spec, which only a command can
//! change; but the gesture reorders the list on every hovered row. It
//! therefore paints a preview spec (`Live::preview`), cloned from the real spec
//! at the first move, and asks for **one** action only on release (one history
//! entry per gesture), computed by `object_drag::finish`. The series window
//! works on a draft: it edits it directly.
//!
//! Only one drag at a time: a single key in egui's memory. The state is read
//! only while a drag payload exists — Escape or a release outside the window
//! abandon it without cleaning anything.

use std::cell::RefCell;

use eframe::egui;
use stokesforge_core::NodeId;
use stokesforge_plot::PlotSpec;

use super::reorder_list::dragging;
use stokesforge_app_state::object_drag::{self, DragItem, Drop, DropOver};
use stokesforge_app_state::state::Action;

const LIVE: &str = "object_dnd_live";

/// The drag in progress.
#[derive(Clone)]
pub(super) struct Live {
    /// The Object Manager's graph; `None` for the series window.
    pub graph: Option<NodeId>,
    /// The grabbed item, at its current place.
    pub item: DragItem,
    /// The spec as the gesture left it (Object Manager), absent until
    /// something has moved.
    pub preview: Option<PlotSpec>,
}

/// The handle has just been grabbed.
pub(super) fn begin(ctx: &egui::Context, graph: Option<NodeId>, item: DragItem) {
    store(
        ctx,
        Live {
            graph,
            item,
            preview: None,
        },
    );
}

fn store(ctx: &egui::Context, live: Live) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(LIVE), live));
}

/// The drag in progress, if any.
pub(super) fn live(ctx: &egui::Context) -> Option<Live> {
    if !dragging(ctx) {
        return None;
    }
    ctx.data(|data| data.get_temp(egui::Id::new(LIVE)))
}

/// The dragged item changes place (series window).
pub(super) fn set_item(ctx: &egui::Context, item: DragItem) {
    if let Some(live) = live(ctx) {
        store(ctx, Live { item, ..live });
    }
}

/// The release happens on this frame.
fn released(ctx: &egui::Context) -> bool {
    ctx.input(|input| input.pointer.any_released())
}

/// The pointer is in the lower half of `rect`.
pub(super) fn pointer_after(ui: &egui::Ui, rect: egui::Rect) -> bool {
    ui.input(|input| input.pointer.interact_pos())
        .is_some_and(|pos| pos.y > rect.center().y)
}

/// The first row hovered in the frame: the one that decides the move, as in
/// the copy formats list.
#[derive(Default)]
pub(super) struct Hover(RefCell<Option<DropOver>>);

impl Hover {
    pub fn offer(&self, over: DropOver) {
        let mut slot = self.0.borrow_mut();
        if slot.is_none() {
            *slot = Some(over);
        }
    }

    pub fn take(&self) -> Option<DropOver> {
        self.0.borrow_mut().take()
    }
}

/// End of the Object Manager's frame: applies the hover to the preview, and on
/// release returns the gesture's single action. `base` is the real spec.
pub(super) fn end_of_frame(
    ctx: &egui::Context,
    graph: NodeId,
    base: &PlotSpec,
    hover: Option<DropOver>,
    actions: &mut Vec<Action>,
) {
    let Some(mut live) = live(ctx).filter(|live| live.graph == Some(graph)) else {
        return;
    };
    if released(ctx) {
        let drop = live
            .preview
            .as_ref()
            .and_then(|preview| object_drag::finish(base, preview, live.item));
        if let Some(drop) = drop {
            actions.push(action_of(graph, drop));
        }
        ctx.data_mut(|data| data.remove::<Live>(egui::Id::new(LIVE)));
        return;
    }
    let Some(over) = hover else {
        return;
    };
    let mut working = live.preview.clone().unwrap_or_else(|| base.clone());
    if let Some(item) = object_drag::step(&mut working, live.item, over) {
        live.item = item;
        live.preview = Some(working);
        store(ctx, live);
        ctx.request_repaint();
    }
}

/// The model action for a destination.
fn action_of(graph: NodeId, drop: Drop) -> Action {
    match drop {
        Drop::Series {
            source,
            group,
            position,
        } => Action::PlaceSeries {
            graph,
            source,
            group,
            position,
        },
        Drop::Group { group, position } => Action::PlaceSeriesGroup {
            graph,
            group,
            position,
        },
        Drop::Object { target, index } => Action::ReorderObject {
            graph,
            target,
            index,
        },
    }
}
