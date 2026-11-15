//! The cursor lock of a page-corner drag (#512).
//!
//! Pulling a page corner outward can go past the screen edge, so for the length
//! of the gesture the mouse is locked: `CursorGrab::Locked`, the system cursor
//! hidden, a cursor drawn by the application on the displayed corner (the
//! motion comes from raw relative events, see `corner_follow`). A lock left on
//! would freeze the user's mouse, so it is a **lease**: every pass of a viewport
//! where a page-corner drag is alive and the pointer button still held calls
//! [`renew`]; an [`egui::Plugin`] checks at the end of every pass of that
//! viewport and, if the lease was not renewed in it, unlocks. That covers every
//! way the drag can vanish with no release event: Escape or leaving the mode,
//! the graph closed or detached, a modal opening, the window losing focus.
//!
//! At the release, [`release`] sends `CursorPosition` (the displayed corner)
//! THEN `CursorGrab(None)` in the same pass: on Wayland the position is only a
//! hint honoured at the unlock, so it must be sent while still locked.
//!
//! This is the only module that sends these two commands (guard
//! `cursor_grab_commands_stay_in_corner_lock`). The plugin registers itself on
//! the first lock, as `os_gesture` does in the application. Commands go to the
//! viewport of the pass (`send_viewport_cmd`): a detached graph window locks and
//! unlocks its own window.

use egui::{Context, CursorIcon, Pos2, ViewportCommand, ViewportId};
use std::collections::HashSet;

/// Viewports with a lock on, and those whose lease was renewed in the pass now
/// running.
#[derive(Default)]
struct CornerLockPlugin {
    locked: HashSet<ViewportId>,
    renewed: HashSet<ViewportId>,
}

impl egui::Plugin for CornerLockPlugin {
    fn debug_name(&self) -> &'static str {
        "corner_lock"
    }

    fn on_end_pass(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let viewport = ctx.viewport_id();
        if self.renewed.remove(&viewport) || !self.locked.remove(&viewport) {
            return;
        }
        // The drag was dropped without a release: give the mouse back.
        ctx.send_viewport_cmd(ViewportCommand::CursorGrab(egui::CursorGrab::None));
        ctx.set_cursor_icon(CursorIcon::Default);
        ctx.request_repaint();
    }
}

/// Keeps the lock alive for this pass of the viewport, taking it on the first
/// call, and hides the system cursor. Only while the window has the focus and
/// the primary button is held: else nothing is renewed and the plugin lets go.
pub(crate) fn renew(ctx: &Context) {
    let held = ctx.input(|input| input.focused && input.pointer.primary_down());
    if !held {
        return;
    }
    ctx.add_plugin(CornerLockPlugin::default());
    let viewport = ctx.viewport_id();
    let first = ctx
        .with_plugin(|plugin: &mut CornerLockPlugin| {
            plugin.renewed.insert(viewport);
            plugin.locked.insert(viewport)
        })
        .unwrap_or(false);
    if first {
        ctx.send_viewport_cmd(ViewportCommand::CursorGrab(egui::CursorGrab::Locked));
    }
    ctx.set_cursor_icon(CursorIcon::None);
    ctx.request_repaint();
}

/// The release of the gesture: the cursor goes back to `corner` (screen
/// points), then the mouse is unlocked, in the same pass.
pub(crate) fn release(ctx: &Context, corner: Option<Pos2>) {
    let viewport = ctx.viewport_id();
    let was_locked = ctx
        .with_plugin(|plugin: &mut CornerLockPlugin| {
            plugin.renewed.remove(&viewport);
            plugin.locked.remove(&viewport)
        })
        .unwrap_or(false);
    if !was_locked {
        return;
    }
    if let Some(corner) = corner {
        ctx.send_viewport_cmd(ViewportCommand::CursorPosition(corner));
    }
    ctx.send_viewport_cmd(ViewportCommand::CursorGrab(egui::CursorGrab::None));
    ctx.set_cursor_icon(CursorIcon::Default);
}
