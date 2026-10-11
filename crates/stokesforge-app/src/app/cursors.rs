//! Curseurs de la souris (#231, X2) : le seul endroit qui nomme un
//! `egui::CursorIcon`. Trois sens, partout les mêmes :
//!
//! - **glisser** : main ouverte au survol d'une prise ([`grab`]), fermée
//!   pendant le geste ([`grabbing`]) ;
//! - **redimensionner** : la flèche du bord ou du coin (`RESIZE_*`) ;
//! - **cliquer** : le doigt sur un élément cliquable qui n'est pas un bouton
//!   — onglet, ligne de liste, zone de la barre d'état ([`clickable`]). Un
//!   bouton garde la flèche.
//!
//! Garde `aucun_curseur_hors_du_module_des_curseurs` (`tests/gardes.rs`).

use eframe::egui;

/// Main ouverte : la prise se saisit.
pub(super) fn grab(ctx: &egui::Context) {
    ctx.set_cursor_icon(egui::CursorIcon::Grab);
}

/// Main fermée : le glisser est en cours.
pub(super) fn grabbing(ctx: &egui::Context) {
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
}

/// Une prise : main ouverte au survol, fermée pendant le glisser.
pub(super) fn grab_handle(response: egui::Response) -> egui::Response {
    if response.dragged() {
        grabbing(&response.ctx);
    } else if response.hovered() {
        grab(&response.ctx);
    }
    response
}

/// Le doigt sur un élément cliquable qui n'est pas un bouton.
pub(super) fn clickable(response: egui::Response) -> egui::Response {
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(super) const RESIZE_NORTH: egui::CursorIcon = egui::CursorIcon::ResizeNorth;
pub(super) const RESIZE_SOUTH: egui::CursorIcon = egui::CursorIcon::ResizeSouth;
pub(super) const RESIZE_WEST: egui::CursorIcon = egui::CursorIcon::ResizeWest;
pub(super) const RESIZE_EAST: egui::CursorIcon = egui::CursorIcon::ResizeEast;
pub(super) const RESIZE_NW_SE: egui::CursorIcon = egui::CursorIcon::ResizeNwSe;
pub(super) const RESIZE_NE_SW: egui::CursorIcon = egui::CursorIcon::ResizeNeSw;
