//! A list of rows reordered by dragging (#148, figure copy formats).
//!
//! The gesture: the handle at the start of a row is grabbed; the whole row
//! is dragged (its content follows the pointer from the grabbed spot, in a
//! floating layer) and its original place stays reserved, empty. The other
//! rows move aside **at once** when the pointer hovers one: the list is
//! already in its new order on release, with no marker line and no drop to
//! handle.
//!
//! Two pitfalls handled here. egui centers its dragged row on the pointer
//! (`dnd_drag_source`): the row would jump from the handle to its middle; so
//! we keep the grabbed point, relative to the row, and reposition the layer
//! on every frame. And since the list reorders during the drag, the payload
//! is the row's identifier, never its index.
//!
//! The module does not touch the data: [`reorder_row`] paints a row and says
//! whether a dragged row hovers it; [`reorder_move`] turns that into a move;
//! the caller applies it with [`move_before`].

use eframe::egui;

use super::cursors::grab_handle;
use crate::icons::{icon_image, Icon, IconRole};

/// The payload of a drag: the grabbed row.
struct RowDrag {
    id: egui::Id,
}

/// Where we keep the grabbed point of the dragged row, relative to the row:
/// only one drag at a time, so a single key — a row's identifier can change
/// during the gesture (Object Manager: it follows the rank).
const GRAB: &str = "reorder_row_grab";
/// The floating layer of the dragged row.
const FLOATING: &str = "reorder_row_floating";

/// What one frame of a row saw.
pub(super) struct RowOutcome {
    /// The row that a dragged row hovers: its frame.
    pub hover: Option<egui::Rect>,
    /// The handle was just grabbed.
    pub grabbed: bool,
}

/// A row drag is in progress (or was just released, on the frame of the
/// release).
pub(super) fn dragging(ctx: &egui::Context) -> bool {
    egui::DragAndDrop::has_payload_of_type::<RowDrag>(ctx)
}

/// Paints a row: the handle (`grip_alt` for accessibility, `grip_tip` as a
/// tooltip) then `body`. Returns `true` if another, dragged row hovers this
/// one: the caller remembers its index for [`reorder_move`].
pub(super) fn reorder_row(
    ui: &mut egui::Ui,
    id: egui::Id,
    grip_alt: String,
    grip_tip: &str,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    let dragged = egui::DragAndDrop::payload::<RowDrag>(ui.ctx()).is_some_and(|drag| drag.id == id);
    reorder_row_with(ui, id, dragged, |ui| {
        let grip = ui.add(
            icon_image(Icon::Grip, IconRole::Neutral)
                .alt_text(grip_alt)
                .sense(egui::Sense::drag()),
        );
        let grip = grab_handle(grip).on_hover_text(grip_tip);
        body(ui);
        Some(grip)
    })
    .hover
    .is_some()
}

/// [`reorder_row`] without an imposed handle: `body` paints the row and returns
/// the response that **grabs** the row (a handle, or the whole label area:
/// it must carry `Sense::drag`), `None` for a row that cannot be dragged.
/// The caller says whether the row is the one being dragged: when the list
/// reorders during the gesture and a row's identifier follows its rank,
/// the payload no longer recognizes it.
pub(super) fn reorder_row_with(
    ui: &mut egui::Ui,
    id: egui::Id,
    dragged: bool,
    body: impl FnOnce(&mut egui::Ui) -> Option<egui::Response>,
) -> RowOutcome {
    let started = std::cell::Cell::new(false);
    let content = |ui: &mut egui::Ui| {
        if let Some(handle) = body(ui) {
            handle.dnd_set_drag_payload(RowDrag { id });
            started.set(handle.drag_started());
        }
    };
    if dragged {
        let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new(FLOATING));
        let slot = ui
            .scope_builder(egui::UiBuilder::new().layer_id(layer), content)
            .response
            .rect;
        ui.allocate_space(slot.size());
        let grab: egui::Vec2 = ui
            .data(|data| data.get_temp(egui::Id::new(GRAB)))
            .unwrap_or_default();
        if let Some(now) = ui.input(|input| input.pointer.interact_pos()) {
            ui.ctx().transform_layer_shapes(
                layer,
                egui::emath::TSTransform::from_translation(now - grab - slot.min),
            );
        }
        return RowOutcome {
            hover: None,
            grabbed: false,
        };
    }
    let row = ui.scope(content).response;
    if started.get() {
        if let Some(start) = ui.input(|input| input.pointer.press_origin()) {
            ui.data_mut(|data| data.insert_temp(egui::Id::new(GRAB), start - row.rect.min));
        }
    }
    RowOutcome {
        hover: row
            .dnd_hover_payload::<RowDrag>()
            .is_some()
            .then_some(row.rect),
        grabbed: started.get(),
    }
}

/// The move to make this frame: `ids` are the identifiers of the rows in
/// displayed order, `over` the index of the row hovered by the dragged row.
/// Returns `(from, to)` for [`move_before`], or `None`.
pub(super) fn reorder_move(
    ctx: &egui::Context,
    ids: &[egui::Id],
    over: Option<usize>,
) -> Option<(usize, usize)> {
    let over = over?;
    let drag = egui::DragAndDrop::payload::<RowDrag>(ctx)?;
    let from = ids.iter().position(|id| *id == drag.id)?;
    Some((from, if over > from { over + 1 } else { over }))
}

/// Moves the item `from` so that it ends up before the item `to` of the
/// original list (`to == len`: at the very end). Pure, to be testable.
pub(super) fn move_before<T>(list: &mut Vec<T>, from: usize, to: usize) {
    if from >= list.len() || to > list.len() || to == from || to == from + 1 {
        return;
    }
    let item = list.remove(from);
    list.insert(if to > from { to - 1 } else { to }, item);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_before_places_before_the_target() {
        let mut list = vec!['a', 'b', 'c'];
        move_before(&mut list, 2, 0);
        assert_eq!(list, ['c', 'a', 'b']);
        move_before(&mut list, 0, 3);
        assert_eq!(list, ['a', 'b', 'c']);
        move_before(&mut list, 1, 1);
        move_before(&mut list, 1, 2);
        move_before(&mut list, 5, 0);
        assert_eq!(list, ['a', 'b', 'c']);
    }

    #[test]
    fn without_a_drag_nothing_moves() {
        let ctx = egui::Context::default();
        let ids = [egui::Id::new(1), egui::Id::new(2)];
        assert_eq!(reorder_move(&ctx, &ids, Some(0)), None);
        assert_eq!(reorder_move(&ctx, &ids, None), None);
    }
}
