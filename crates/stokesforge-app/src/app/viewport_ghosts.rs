//! No viewport without a closure leaves egui (#475, after the GNOME trial).
//!
//! A detached window of native Wayland closes itself: it stops registering
//! itself in its own pass and egui drops it from that pass's output, so
//! eframe destroys the native window. But egui 0.36.2 then recreates an empty
//! entry for the id (`Context::run_dyn`: `viewport_for(viewport_id)` after the
//! last pass of the viewport), whose parent is the viewport itself, so no
//! other pass ever prunes it (`end_pass`: a viewport is only pruned by its
//! parent's pass, or when its parent is gone). The entry has the default
//! builder and no closure; the next output lists it, and eframe creates a
//! window for every listed viewport (`initialized_all_windows`): an empty
//! "egui window" per reattached window, until the application quits. The
//! previous guess ("the empty entry creates no window") was wrong.
//!
//! The one door egui offers for it is [`egui::Plugin::output_hook`], run just
//! before the output reaches eframe: this plugin drops from it every viewport
//! that is not the root and has no closure. Every window of the application
//! is a deferred viewport, registered with its closure, so none of them is
//! lost; a window opened again registers itself and gets its entry back.

use eframe::egui;

/// Registers the plugin; once is enough (a second call is ignored).
pub(super) fn install(ctx: &egui::Context) {
    ctx.add_plugin(ViewportGhostFilter);
}

struct ViewportGhostFilter;

impl egui::Plugin for ViewportGhostFilter {
    fn debug_name(&self) -> &'static str {
        "viewport_ghost_filter"
    }

    fn output_hook(&mut self, _ctx: &egui::Context, output: &mut egui::FullOutput) {
        drop_ghosts(output);
    }
}

/// Removes from `output` the viewports eframe would give an empty window.
fn drop_ghosts(output: &mut egui::FullOutput) {
    output
        .viewport_output
        .retain(|id, viewport| *id == egui::ViewportId::ROOT || viewport.viewport_ui_cb.is_some());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sequence of a self-closing window on egui 0.36.2: the root registers
    /// the window, the window registers itself, then ends a pass without doing
    /// so. egui keeps an empty entry for it that every later output lists;
    /// with the plugin no output lists it (witness: remove `install`).
    #[test]
    fn a_window_that_closed_itself_is_never_listed_again() {
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        install(&ctx);
        let child = egui::ViewportId::from_hash_of("ghost-child");
        let register = |ui: &egui::Ui| {
            ui.ctx()
                .show_viewport_deferred(child, egui::ViewportBuilder::default(), |_, _| {});
        };
        let root = |register_it: bool| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                if register_it {
                    register(ui);
                }
            });
            output.textures_delta.clear();
            output
        };
        let child_pass = |again: bool| {
            let mut raw = egui::RawInput {
                viewport_id: child,
                ..Default::default()
            };
            raw.viewports.insert(
                child,
                egui::ViewportInfo {
                    parent: Some(egui::ViewportId::ROOT),
                    ..Default::default()
                },
            );
            let mut output = ctx.run_ui(raw, |ui| {
                if again {
                    register(ui);
                }
            });
            output.textures_delta.clear();
            output
        };
        assert!(root(true).viewport_output.contains_key(&child));
        assert!(child_pass(true).viewport_output.contains_key(&child));
        assert!(
            !child_pass(false).viewport_output.contains_key(&child),
            "the closing pass lists no window"
        );
        for _ in 0..3 {
            assert!(
                !root(false).viewport_output.contains_key(&child),
                "no later pass lists the closed window again"
            );
        }
        // Opened again: the window is back, with its closure.
        assert!(root(true).viewport_output.contains_key(&child));
    }
}
