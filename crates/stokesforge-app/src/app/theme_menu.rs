//! The graph-theme entries of the menus (#541): Copy Theme, Paste Theme,
//! Apply Theme… and, in the menu bar only, Save Theme As….
//!
//! One paint for the right-click of a graph, the right-click of graphs in the
//! Project Manager and the Graph menu (`docs/menus.md`): the entries sit in
//! **one section**. An entry that cannot act now is **greyed with its reason**
//! (never absent): the rules are `stokesforge_app_state::theme_window::theme_menu`.
//!
//! The menus of a graph are painted away from the state (the figure's
//! right-click knows its view and its tree only), so the root publishes the one
//! fact they need, « is something copied? », once per frame
//! ([`publish`]), the way the active layer is published.

use eframe::egui;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::theme_window::theme_menu;
use stokesforge_app_state::tr;
use stokesforge_core::NodeId;

fn clipboard_id() -> egui::Id {
    egui::Id::new("stokesforge_theme_clipboard_present")
}

/// Publishes whether Copy Theme holds a theme, for the menus painted away from
/// the state.
pub(super) fn publish(ctx: &egui::Context, state: &AppState) {
    let present = state.theme_clipboard().is_some();
    ctx.data_mut(|data| data.insert_temp(clipboard_id(), present));
}

/// The entries for `graphs` (the graphs the gesture acts on): Copy Theme, Paste
/// Theme, Apply Theme…, and Save Theme As… when `bar` (the Graph menu).
pub(super) fn theme_menu_entries(
    ui: &mut egui::Ui,
    graphs: &[NodeId],
    bar: bool,
    actions: &mut Vec<Action>,
) {
    let clipboard = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(clipboard_id()))
        .unwrap_or(false);
    let menu = theme_menu(clipboard, graphs.len());
    // The reason comes from the state (`theme_menu`): shown only while greyed.
    let entry = |ui: &mut egui::Ui, label: String, hint: String, grey: &Option<String>| {
        let clicked = ui
            .add_enabled(grey.is_none(), egui::Button::new(label))
            .on_hover_text(hint)
            .on_disabled_hover_text(grey.clone().unwrap_or_default())
            .clicked();
        if clicked {
            ui.close();
        }
        clicked
    };
    if entry(
        ui,
        tr!("action-copy-theme"),
        tr!("action-copy-theme", "hint"),
        &menu.copy,
    ) {
        if let Some(graph) = graphs.first() {
            actions.push(Action::CopyTheme { graph: *graph });
        }
    }
    if entry(
        ui,
        tr!("action-paste-theme"),
        tr!("action-paste-theme", "hint"),
        &menu.paste,
    ) {
        actions.push(Action::PasteTheme {
            graphs: graphs.to_vec(),
        });
    }
    if entry(
        ui,
        tr!("action-apply-theme"),
        tr!("action-apply-theme", "hint"),
        &menu.apply,
    ) {
        actions.push(Action::OpenApplyTheme {
            graphs: graphs.to_vec(),
        });
    }
    if bar
        && entry(
            ui,
            tr!("action-save-theme-as"),
            tr!("action-save-theme-as", "hint"),
            &menu.save,
        )
    {
        if let Some(graph) = graphs.first() {
            actions.push(Action::OpenSaveTheme { graph: *graph });
        }
    }
}
