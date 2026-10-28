//! Mouse cursors (#231, S-X2): the only place that names an
//! `egui::CursorIcon`. Three meanings, the same everywhere:
//!
//! - **drag**: open hand on hover over a grip ([`grab`]), closed hand
//!   during the gesture ([`grabbing`]);
//! - **resize**: the arrow of the edge or corner (`RESIZE_*`);
//! - **click**: the finger on a clickable element that is not a button
//!   — tab, list row, status bar area ([`clickable`]). A button keeps the
//!   arrow.
//!
//! Guard `no_cursor_outside_the_cursors_module` (`tests/guards.rs`).

use eframe::egui;

/// Open hand: the grip can be grabbed.
pub(super) fn grab(ctx: &egui::Context) {
    ctx.set_cursor_icon(egui::CursorIcon::Grab);
}

/// Closed hand: the drag is in progress.
pub(super) fn grabbing(ctx: &egui::Context) {
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
}

/// A grip: open hand on hover, closed during the drag.
pub(super) fn grab_handle(response: egui::Response) -> egui::Response {
    if response.dragged() {
        grabbing(&response.ctx);
    } else if response.hovered() {
        grab(&response.ctx);
    }
    response
}

/// The finger on a clickable element that is not a button.
pub(super) fn clickable(response: egui::Response) -> egui::Response {
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(super) const RESIZE_NORTH: egui::CursorIcon = egui::CursorIcon::ResizeNorth;
pub(super) const RESIZE_SOUTH: egui::CursorIcon = egui::CursorIcon::ResizeSouth;
pub(super) const RESIZE_WEST: egui::CursorIcon = egui::CursorIcon::ResizeWest;
pub(super) const RESIZE_EAST: egui::CursorIcon = egui::CursorIcon::ResizeEast;
pub(super) const RESIZE_NW_SE: egui::CursorIcon = egui::CursorIcon::ResizeNwSe;
pub(super) const RESIZE_NE_SW: egui::CursorIcon = egui::CursorIcon::ResizeNeSw;
