//! The detached panels (#475, step 2, L3): the Project Manager, the Object
//! Manager and the script console in their own OS windows, serving themselves
//! like the detached graph and workbook (`detached_graph.rs`, `detached_book.rs`).
//!
//! The closure of the viewport locks the whole application
//! ([`lock_app_from_viewport`]), paints the panel with **the same function as
//! when it is docked** ([`StokesForgeApp::project_manager_body`],
//! [`StokesForgeApp::object_manager_body`], [`script_contents`]) from the shared
//! state, and applies the actions of its gestures at once, in the same pass
//! ([`StokesForgeApp::apply_pass_actions`]). No copy of the tree, the selection
//! or the console, and no queue to the root: a selection made in the Project
//! Manager window is the application's selection when the pass ends, and the
//! Object Manager window reads the selection of the graph in front, whichever
//! window made it.
//!
//! What a panel window keeps for itself is [`PanelWindow`]: the flag of
//! Reattach, whether the window registers itself (native Wayland), and its
//! **sender identity** (#21): put into the state for the length of the paint, so
//! that the entry a press in this window ends is committed as this window, and
//! the actor refuses it while the main window holds a macro open. A press in a
//! panel ends the entry held anywhere else (one entry in the application,
//! [`StokesForgeApp::commit_entry_held_elsewhere`]). The renaming of a node in
//! the Project Manager is an egui editor of that viewport that ends in an action
//! (`RenameObjects`), as when docked.
//!
//! **Reattach** (the header button or the OS close, which becomes a Reattach)
//! follows the graph (L4): on native Wayland the window carries it out in its
//! own pass and does not register itself again; elsewhere it raises the flag and
//! the root carries it out ([`StokesForgeApp::drain_detached_panel_flags`]). The
//! sizes and positions remembered between two launches (#148) are reported by the
//! pass ([`LiveWindow::report`](super::panel_memory::LiveWindow::report)) and
//! used as before. A panel that View hides while detached has no window: the
//! root stops publishing it and wakes it once if it registered itself.

use std::collections::{BTreeSet, HashSet};

use eframe::egui;
use stokesforge_app_state::object_manager::VisibilityTarget;
use stokesforge_app_state::panel_layout::PanelKind;
use stokesforge_app_state::preferences::ObjectManagerSettings;
use stokesforge_app_state::state::{active_graph, Action};
use stokesforge_app_state::tr;
use stokesforge_core::NodeId;
use stokesforge_session::SessionHandle;

use super::detached_book::{PassOf, WindowIdentity};
use super::file_drop::{begin_file_drop, finish_file_drop};
use super::object_manager_ui::selected_manager_target;
use super::panel_memory::panel_index;
use super::project_manager::{PmEntry, PROJECT_MANAGER_WIDTH};
use super::script_console::script_contents;
use super::server::lock_app_from_viewport;
use super::shortcuts::SHORTCUT_RUN_SCRIPT;
use super::tab::{active_tab_kind, Tab};
use super::window_detach::{detached_panel_contents, panel_viewport, publish_viewports};
use super::StokesForgeApp;

/// What a detached panel window keeps for itself.
#[derive(Default)]
pub(super) struct PanelWindow {
    /// The sender identity of this window (#21): in the state while the window
    /// paints, here the rest of the time. Made at the first pass.
    pub(super) identity: Option<SessionHandle>,
    /// "Reattach" (or the OS close) was actuated. Native Wayland: the window
    /// moves itself in the same pass; elsewhere the root drains the flag.
    pub(super) reattach: bool,
    /// Native Wayland: the window has had its first pass and registers itself
    /// from now on (it is its own parent); the root no longer publishes it.
    pub(super) self_owned: bool,
}

/// What the panels show besides the snapshot of the model: the root and the
/// windows compare it before and after a pass to wake the detached panels.
#[derive(Clone, PartialEq)]
pub(super) struct PanelsView {
    selection: HashSet<PmEntry>,
    open: HashSet<Tab>,
    active: Option<Tab>,
    cut: HashSet<NodeId>,
    manual: BTreeSet<NodeId>,
    target: Option<VisibilityTarget>,
    settings: ObjectManagerSettings,
}

/// The viewport of the detached window of a panel.
pub(super) fn panel_viewport_id(kind: PanelKind) -> egui::ViewportId {
    match kind {
        PanelKind::Project => super::project_manager::viewport_id(),
        PanelKind::Object => super::window_detach::object_manager_viewport(),
        PanelKind::Script => super::window_detach::script_viewport(),
    }
}

impl StokesForgeApp {
    /// Is the panel detached into its own OS window?
    pub(super) fn panel_detached(&self, kind: PanelKind) -> bool {
        match kind {
            PanelKind::Project => self.pm_detached,
            PanelKind::Object => self.om_detached,
            PanelKind::Script => self.script_detached,
        }
    }

    /// Does View show the panel (docked or detached)?
    fn panel_shown(&self, kind: PanelKind) -> bool {
        match kind {
            PanelKind::Project => self.pm_open,
            PanelKind::Object => self.state.object_manager_open(),
            PanelKind::Script => self.script_open,
        }
    }

    /// Does the panel have an OS window right now?
    pub(super) fn panel_has_window(&self, kind: PanelKind) -> bool {
        self.panel_detached(kind) && self.panel_shown(kind)
    }

    /// Switches a panel between its docked and detached presentation. Docking a
    /// panel whose window registered itself needs one pass of that window to
    /// leave egui: it is woken once ([`Self::wake_released_windows`]).
    pub(super) fn set_panel_detached(&mut self, kind: PanelKind, detached: bool) {
        match kind {
            PanelKind::Project => self.pm_detached = detached,
            PanelKind::Object => self.om_detached = detached,
            PanelKind::Script => self.script_detached = detached,
        }
        if !detached {
            let window = std::mem::take(&mut self.panel_windows[panel_index(kind)]);
            if window.self_owned {
                self.released_windows.push(panel_viewport_id(kind));
            }
        }
    }

    /// Switches to detached mode: the next render opens the Project Manager in
    /// its own OS window instead of the fixed panel
    /// ([`StokesForgeApp::project_manager_panel`]). No effect if it already is.
    pub(super) fn detach_project_manager(&mut self) {
        self.pm_detached = true;
    }

    /// Switches out of detached mode: the fixed panel reappears at the next
    /// render, always on the left since it is fixed in position.
    pub(super) fn reattach_project_manager(&mut self) {
        self.set_panel_detached(PanelKind::Project, false);
    }

    /// Detaches the console. The Python job never migrates: it stays in the state
    /// the window paints from.
    pub(super) fn detach_script_console(&mut self) {
        self.script_detached = true;
    }

    /// What the panels read besides the snapshot; `None` when no panel has a
    /// window (nothing to wake, nothing to compute).
    pub(super) fn panels_view(&self) -> Option<PanelsView> {
        if !PanelKind::ALL
            .into_iter()
            .any(|kind| self.panel_has_window(kind))
        {
            return None;
        }
        let active = self.current_content_tab();
        let target = active_graph(active_tab_kind(active))
            .and_then(|graph| self.read_graph_view(graph, selected_manager_target))
            .flatten();
        Some(PanelsView {
            selection: self.pm_selection.clone(),
            open: self.open_tabs(),
            active,
            cut: self.state.cut_objects().into_iter().collect(),
            manual: self.state.tree().manual_order_folders().clone(),
            target,
            settings: self.state.object_manager_settings(),
        })
    }

    /// Queues the registration of the window of a panel (`publish_viewport`, the
    /// one function that registers a viewport). Called by the root until the
    /// window's first pass, then by the window itself on native Wayland.
    fn publish_panel_window(&mut self, ctx: &egui::Context, kind: PanelKind) {
        let (title, size, min) = match kind {
            PanelKind::Project => (
                tr!("window-title-project-manager"),
                [PROJECT_MANAGER_WIDTH + 40.0, 480.0],
                [220.0, 140.0],
            ),
            PanelKind::Object => (tr!("action-object-manager"), [330.0, 480.0], [220.0, 140.0]),
            PanelKind::Script => (tr!("script-title"), [700.0, 440.0], [300.0, 220.0]),
        };
        let builder = panel_viewport(title, size, min, self.detach_init(kind));
        let shared = self.shared_self.clone();
        Self::publish_viewport(
            &mut self.pending_viewports,
            ctx,
            panel_viewport_id(kind),
            builder,
            false,
            move |ui, _class| {
                // The root released its guard before this window was published.
                let Some(shared) = shared.upgrade() else {
                    return;
                };
                let pending = {
                    let mut app = lock_app_from_viewport(&shared);
                    app.detached_panel_pass(ui, kind);
                    app.take_pending_viewports()
                };
                // The window registers itself with the lock released.
                publish_viewports(ui.ctx(), pending);
            },
        );
    }

    /// Publishes the window of each detached panel that is not yet its own
    /// parent (queued for after the lock is released). A hidden panel has no
    /// window: one that registered itself is woken once so that its pass ends it.
    pub(super) fn show_detached_panels(&mut self, _ctx: &egui::Context) {
        for kind in PanelKind::ALL {
            if !self.panel_detached(kind) {
                continue;
            }
            let i = panel_index(kind);
            if !self.panel_shown(kind) {
                if std::mem::take(&mut self.panel_windows[i].self_owned) {
                    self.released_windows.push(panel_viewport_id(kind));
                }
                continue;
            }
            if self.panel_windows[i].self_owned {
                continue;
            }
            self.publish_panel_window(_ctx, kind);
        }
    }

    /// The Reattach a panel window raised since the last root pass, where the
    /// window cannot close itself (X11, Windows).
    pub(super) fn drain_detached_panel_flags(&mut self, ctx: &egui::Context) {
        for kind in PanelKind::ALL {
            if std::mem::take(&mut self.panel_windows[panel_index(kind)].reattach) {
                super::window_band::clear(ctx, panel_viewport_id(kind));
                self.set_panel_detached(kind, false);
                ctx.request_repaint();
            }
        }
    }

    /// One pass of the detached window of a panel, under the application lock:
    /// keys, paint with the docked function, then its own actions applied at once.
    pub(super) fn detached_panel_pass(&mut self, ui: &mut egui::Ui, kind: PanelKind) {
        let ctx = ui.ctx().clone();
        let viewport = ctx.viewport_id();
        // The dialog is the application's: seen at once, not at the next root
        // pass.
        super::window_band::publish_dialog(&ctx, self.state.open_dialog());
        super::window_band::stop_keys(&ctx);
        super::shortcuts::refuse_quit_in_detached(&ctx);
        if kind == PanelKind::Project {
            begin_file_drop(&ctx);
        }
        if self.panel_has_window(kind) {
            let mut prelude = Vec::new();
            self.pass_prelude(&ctx, &mut prelude);
            self.panel_windows[panel_index(kind)]
                .identity
                .get_or_insert_with(|| self.state.new_session_identity());
            // One entry in the application: a press here ends the entry held by
            // a workbook, before this window paints. Under the window's own
            // identity (#21).
            WindowIdentity::enter(self, kind).commit_entry_held_elsewhere(&ctx, PassOf::Panel);
            let mut own = Vec::new();
            let recorded = self.state.recorded_count();
            let woke = WindowIdentity::enter(self, kind).paint_detached_panel(ui, kind, &mut own);
            // What the panel said while it wrote (a refused entry) is told here.
            self.tell_window(&ctx, viewport, recorded);
            self.answer_refusals(&ctx);
            self.apply_pass_actions(&ctx, viewport, prelude, own, woke);
            self.keep_or_close_own_panel(&ctx, viewport, kind);
        }
        super::page_layout::note_field_focus(&ctx);
        super::window_band::paint(&ctx);
    }

    /// Paints the panel with the function of the docked one and collects the
    /// actions of its gestures into `own`. Returns whether the root and the other
    /// windows must be woken for something that is not an action (the selection
    /// moved, a window opened or closed, Reattach was raised).
    fn paint_detached_panel(
        &mut self,
        ui: &mut egui::Ui,
        kind: PanelKind,
        own: &mut Vec<Action>,
    ) -> bool {
        let ctx = ui.ctx().clone();
        let viewport = ctx.viewport_id();
        self.live_window(kind).report(&ctx);
        let before = self.panels_view();
        // Read before anything is painted (#459): a field never sees these keys.
        // Queued after the window's own actions, below.
        let mut keys = super::shortcuts::read_detached_history_and_save(&ctx);
        if kind == PanelKind::Script
            && ctx.input_mut(|input| input.consume_shortcut(&SHORTCUT_RUN_SCRIPT))
        {
            own.push(Action::RunScript);
        }
        let (mut opened, mut closed) = (Vec::new(), Vec::new());
        let title = match kind {
            PanelKind::Project => tr!("project-manager-title"),
            PanelKind::Object => tr!("action-object-manager"),
            PanelKind::Script => tr!("script-title"),
        };
        let reattach = detached_panel_contents(ui, &title, |ui| match kind {
            PanelKind::Project => self.project_manager_body(ui, &mut opened, &mut closed, own),
            PanelKind::Object => {
                let active = self.current_content_tab();
                self.object_manager_body(ui, &ctx, active, own, Some(viewport));
            }
            PanelKind::Script => {
                if !self.state.script_available() {
                    ui.weak(tr!("script-unavailable"));
                }
                script_contents(ui, &ctx, &mut self.state, own);
            }
        });
        own.append(&mut keys);
        if kind == PanelKind::Project {
            if let Some(action) = finish_file_drop(&ctx) {
                own.push(action);
            }
        }
        let windows_moved = !opened.is_empty() || !closed.is_empty();
        // Windows opened or closed by a click in the tree: the same application
        // of the requests as the root's, after the painting.
        self.apply_window_requests(&ctx, opened, Vec::new(), closed);
        if reattach {
            self.panel_windows[panel_index(kind)].reattach = true;
        }
        let after = self.panels_view();
        // The others are woken by this pass, if it moved something; the root
        // will not wake the panels again for what this pass did.
        let moved = before != after;
        self.panels_seen = after;
        self.panel_windows[panel_index(kind)].reattach || windows_moved || moved
    }

    /// Native Wayland (L4): the end of the window's pass. A Reattach raised in
    /// this pass is carried out here, at once; the window then does not register
    /// itself and egui removes it at the end of this very pass. Otherwise the
    /// window registers itself again (it is its own parent from its first pass
    /// on). Elsewhere the root does both.
    fn keep_or_close_own_panel(
        &mut self,
        ctx: &egui::Context,
        viewport: egui::ViewportId,
        kind: PanelKind,
    ) {
        if !self.windows_close_themselves {
            return;
        }
        let window = &mut self.panel_windows[panel_index(kind)];
        if std::mem::take(&mut window.reattach) {
            // This pass is the window's last: nobody wakes it for its removal.
            window.self_owned = false;
            super::window_band::clear(ctx, viewport);
            self.set_panel_detached(kind, false);
            return;
        }
        window.self_owned = true;
        self.publish_panel_window(ctx, kind);
    }
}

#[cfg(test)]
mod tests {
    //! The three detached panels serve themselves (#475, L3): every test drives a
    //! root and the windows the way eframe does (`Bench::with_every_window`).
    use std::time::{Duration, Instant};

    use super::super::project_manager;
    use super::super::tab::Tab;
    use super::super::test_support::bench::{delay, Bench, Pass, Window};
    use super::super::test_support::{key, text_exact, texts};
    use super::super::window_band;
    use super::*;
    use eframe::egui;
    use stokesforge_app_state::panel_layout::PanelKind;
    use stokesforge_app_state::state::Action;

    const PANELS: [(PanelKind, Window); 3] = [
        (PanelKind::Project, Window::ProjectManager),
        (PanelKind::Object, Window::ObjectManager),
        (PanelKind::Script, Window::Console),
    ];

    fn started(wayland: bool) -> Bench {
        let mut bench = Bench::with_every_window();
        bench.app.windows_close_themselves = wayland;
        bench.start_every_window();
        bench
    }

    fn os_close() -> egui::ViewportInfo {
        egui::ViewportInfo {
            events: vec![egui::ViewportEvent::Close],
            ..Default::default()
        }
    }

    fn click(bench: &mut Bench, window: Window, pos: egui::Pos2) {
        let id = bench.id_of(window);
        for events in [
            vec![egui::Event::PointerMoved(pos)],
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ] {
            bench.child_in(id, Pass::child().events(events));
        }
        bench.child_in(id, Pass::child());
    }

    /// The painted row of the node `name` in the Project Manager window.
    fn row_of(bench: &mut Bench, name: &str) -> egui::Pos2 {
        let id = bench.id_of(Window::ProjectManager);
        let output = bench.child_in(id, Pass::child());
        text_exact(&texts(&output.shapes), name).center
    }

    /// A folder made in the detached Project Manager, with its name field closed.
    fn folder_in_panel(bench: &mut Bench) -> NodeId {
        let root = bench.app.state.tree().root();
        let pm = bench.id_of(Window::ProjectManager);
        bench.act_in(pm, Action::AddFolder { parent: root });
        let folder = *bench
            .app
            .state
            .tree()
            .node(root)
            .unwrap()
            .children()
            .last()
            .unwrap();
        for _ in 0..3 {
            bench.child_in(pm, Pass::child());
        }
        // Esc abandons the name field the new folder opened.
        bench.child_in(pm, Pass::child().events(vec![key(egui::Key::Escape)]));
        bench.child_in(pm, Pass::child());
        assert!(project_manager::renaming_in(&bench.ctx, pm).is_empty());
        folder
    }

    /// T2 for the panels, native Wayland: Reattach raised in the window's own
    /// pass alone docks the panel again, with no root pass; the window leaves egui
    /// at the end of that pass and the root is woken. Witness: flags drained at
    /// the root only (`keep_or_close_own_panel` returns early).
    #[test]
    fn reattach_in_the_panel_pass_alone_docks_it_on_wayland() {
        for (kind, window) in PANELS {
            let mut bench = started(true);
            let id = bench.id_of(window);
            let i = panel_index(kind);
            assert!(
                bench.app.panel_windows[i].self_owned,
                "{kind:?}: after its passes the window registers itself"
            );
            bench.app.panel_windows[i].reattach = true;
            let output = bench.child_in(id, Pass::child());
            assert!(!bench.app.panel_detached(kind), "{kind:?}: docked at once");
            assert!(!bench.app.open_viewports().contains(&id));
            assert!(
                !output.viewport_output.contains_key(&id),
                "{kind:?}: egui dropped the window at the end of its own pass"
            );
            assert_eq!(delay(&output, egui::ViewportId::ROOT), Duration::ZERO);
            assert!(bench.app.released_windows.is_empty(), "not woken twice");
            bench.root(Pass::root());
            assert!(!bench.app.panel_detached(kind));
            bench.finish();
        }
    }

    /// X11 and Windows: the window raises the flag and stays until the root
    /// drains it. The OS cross is converted into the same Reattach. Witness:
    /// self-close on that path.
    #[test]
    fn without_wayland_the_root_docks_the_panel() {
        for (kind, window) in PANELS {
            for os_cross in [false, true] {
                let mut bench = started(false);
                let id = bench.id_of(window);
                assert!(!bench.app.panel_windows[panel_index(kind)].self_owned);
                let output = if os_cross {
                    bench.child_in(id, Pass::child().info(os_close()))
                } else {
                    bench.app.panel_windows[panel_index(kind)].reattach = true;
                    bench.child_in(id, Pass::child())
                };
                if os_cross {
                    assert!(
                        output.viewport_output[&id]
                            .commands
                            .contains(&egui::ViewportCommand::CancelClose),
                        "{kind:?}: the OS close is cancelled and becomes a Reattach"
                    );
                }
                assert!(
                    bench.app.panel_detached(kind),
                    "{kind:?}: still detached until the root"
                );
                bench.root(Pass::root());
                assert!(!bench.app.panel_detached(kind), "{kind:?}: docked");
                assert!(!bench.app.panel_windows[panel_index(kind)].reattach);
                bench.finish();
            }
        }
    }

    /// Wayland: the OS cross in the window's own pass docks the panel at once.
    #[test]
    fn the_os_cross_of_a_panel_window_docks_it_on_wayland() {
        for (kind, window) in PANELS {
            let mut bench = started(true);
            let id = bench.id_of(window);
            let output = bench.child_in(id, Pass::child().info(os_close()));
            assert!(!bench.app.panel_detached(kind), "{kind:?}");
            assert!(!output.viewport_output.contains_key(&id));
            bench.finish();
        }
    }

    /// A panel that View hides while detached has no window: the root wakes the
    /// self-registered window once, which ends it; showing the panel again opens
    /// it again. Witness: keep publishing a hidden panel.
    #[test]
    fn a_hidden_detached_panel_has_no_window() {
        let mut bench = started(true);
        let id = bench.id_of(Window::ProjectManager);
        assert!(bench.has_window(id));
        bench.app.pm_open = false;
        bench.root(Pass::root());
        assert!(!bench.app.panel_windows[0].self_owned);
        assert!(!bench.app.open_viewports().contains(&id));
        let output = bench.child_in(id, Pass::child());
        assert!(
            !output.viewport_output.contains_key(&id),
            "the window leaves egui"
        );
        assert!(!bench.has_window(id));
        bench.app.pm_open = true;
        bench.root(Pass::root());
        assert!(bench.has_window(id), "published again");
        bench.child_in(id, Pass::child());
        assert!(bench.app.panel_windows[0].self_owned);
        bench.finish();
    }

    /// #148: a panel window reports its size and place from its own pass.
    /// Witness: drop the `report` call.
    #[test]
    fn a_panel_window_reports_its_geometry_from_its_pass() {
        for (kind, window) in PANELS {
            let mut bench = started(true);
            let rect = egui::Rect::from_min_size(egui::pos2(30.0, 40.0), egui::vec2(410.0, 520.0));
            let info = egui::ViewportInfo {
                inner_rect: Some(rect),
                outer_rect: Some(rect),
                ..Default::default()
            };
            bench.child_in(bench.id_of(window), Pass::child().info(info));
            let live = bench.app.live_window(kind).get().expect("reported");
            assert_eq!(live.size, [410.0, 520.0], "{kind:?}");
            assert_eq!(live.position, Some([30.0, 40.0]));
            bench.finish();
        }
    }

    /// T13: a selection made in the detached Project Manager is the
    /// application's when its pass ends, with no root pass and no queue; the
    /// root is woken to look. Witness: paint from a copy of the selection (it
    /// reaches the root at the next drain only).
    #[test]
    fn a_selection_in_the_detached_project_manager_is_seen_by_the_root_at_once() {
        let mut bench = started(true);
        let folder = folder_in_panel(&mut bench);
        let name = bench
            .app
            .state
            .tree()
            .node(folder)
            .unwrap()
            .name()
            .to_string();
        let before = bench.app.pm_selection.clone();
        let row = row_of(&mut bench, &name);
        let pm = bench.id_of(Window::ProjectManager);
        let mut woke_root = false;
        for events in [
            vec![egui::Event::PointerMoved(row)],
            vec![egui::Event::PointerButton {
                pos: row,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            vec![egui::Event::PointerButton {
                pos: row,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ] {
            let output = bench.child_in(pm, Pass::child().events(events));
            woke_root |= delay(&output, egui::ViewportId::ROOT) == Duration::ZERO;
        }
        assert_ne!(bench.app.pm_selection, before, "selected, no root pass ran");
        assert!(
            bench
                .app
                .pm_selection
                .iter()
                .any(|entry| entry.node() == folder),
            "the folder is the selection"
        );
        assert!(woke_root, "the root is woken to look at it");
        // The root sees the same selection and keeps it.
        bench.root(Pass::root());
        assert!(bench
            .app
            .pm_selection
            .iter()
            .any(|entry| entry.node() == folder));
        bench.finish();
    }

    /// T13: a rename typed in the detached Project Manager is applied by its own
    /// pass: the tree has the new name before any root pass, and the window
    /// paints it. Witness: queue the rename for the root.
    #[test]
    fn a_rename_in_the_detached_project_manager_is_applied_in_its_own_pass() {
        let mut bench = started(true);
        let folder = folder_in_panel(&mut bench);
        let pm = bench.id_of(Window::ProjectManager);
        let root = bench.app.state.tree().root();
        project_manager::begin_rename_of_new_folder(&bench.ctx, pm, folder, root);
        bench.child_in(pm, Pass::child());
        bench.child_in(
            pm,
            Pass::child().events(vec![egui::Event::Text("Renamed".into())]),
        );
        bench.child_in(pm, Pass::child());
        assert!(project_manager::rename_text_in(&bench.ctx, pm)
            .is_some_and(|text| text.contains("Renamed")));
        bench.child_in(pm, Pass::child().events(vec![key(egui::Key::Enter)]));
        let node_name = |bench: &Bench| {
            bench
                .app
                .state
                .tree()
                .node(folder)
                .unwrap()
                .name()
                .to_string()
        };
        assert!(
            node_name(&bench).contains("Renamed"),
            "renamed in the pass of the window, no root pass: {}",
            node_name(&bench)
        );
        let output = bench.child_in(pm, Pass::child());
        assert!(texts(&output.shapes)
            .iter()
            .any(|text| text.text == node_name(&bench)));
        bench.finish();
    }

    /// A name half typed in the detached Project Manager, then a click elsewhere
    /// in the window: the name is committed as when the panel is docked (a click
    /// elsewhere ends the field), in one history entry.
    #[test]
    fn a_half_typed_name_is_committed_by_a_click_elsewhere_in_the_panel() {
        let mut bench = started(true);
        let folder = folder_in_panel(&mut bench);
        let pm = bench.id_of(Window::ProjectManager);
        let root = bench.app.state.tree().root();
        project_manager::begin_rename_of_new_folder(&bench.ctx, pm, folder, root);
        bench.child_in(pm, Pass::child());
        bench.child_in(
            pm,
            Pass::child().events(vec![egui::Event::Text("Half".into())]),
        );
        bench.child_in(pm, Pass::child());
        let label = bench.app.state.undo_label();
        // A press elsewhere in the same window: low in the panel, on no row.
        click(&mut bench, Window::ProjectManager, egui::pos2(100.0, 560.0));
        let name = bench
            .app
            .state
            .tree()
            .node(folder)
            .unwrap()
            .name()
            .to_string();
        assert!(name.contains("Half"), "committed: {name}");
        assert!(project_manager::renaming_in(&bench.ctx, pm).is_empty());
        assert_ne!(bench.app.state.undo_label(), label, "one history entry");
        bench.finish();
    }

    /// The user's original fear (#475, form 475-3): the detached Object Manager
    /// follows a selection made in the main window at once, in its next pass,
    /// with no copy to refresh. Witness: paint from a copy of the selection taken
    /// at an earlier pass.
    #[test]
    fn the_detached_object_manager_follows_a_selection_made_in_the_main_window() {
        let mut bench = started(true);
        let graph = bench.graph;
        let om = bench.id_of(Window::ObjectManager);
        let selected = |bench: &mut Bench| {
            let output = bench.child_in(bench.id_of(Window::ObjectManager), Pass::child());
            output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape,
                        egui::Shape::Rect(rect) if rect.fill == crate::theme::list_row_selected_fill())
                })
                .count()
        };
        // The graph is in front: bring it to the main window's side by reading
        // the Object Manager's own active tab.
        let none = selected(&mut bench);
        let spec = bench.app.state.plot_spec(graph).unwrap();
        let tree = bench.app.state.tree().clone();
        let series = stokesforge_plot::SeriesRef::new(0, 0);
        let target = stokesforge_app_state::object_manager::VisibilityTarget::Series(series);
        // A gesture of the main window selects a series of the graph view.
        bench.app.with_graph_view(graph, |view| {
            super::super::object_manager_ui::apply_object_request(
                &bench.ctx,
                view,
                &tree,
                graph,
                super::super::object_manager_ui::ObjectManagerRequest::Select(target),
                &mut Vec::new(),
            )
        });
        let _ = (spec, om);
        assert_eq!(
            selected(&mut bench),
            none + 1,
            "the very next pass of the panel shows the selection of the main window"
        );
        bench.finish();
    }

    /// With a dialog open in the main window only a selection goes through the
    /// detached Object Manager; any other gesture is refused with the band in its
    /// window (#471). Witness: let the request through.
    #[test]
    fn the_detached_object_manager_refuses_all_but_a_selection_while_a_dialog_is_open() {
        use super::super::object_manager_ui::ObjectManagerRequest as Request;
        let mut bench = started(true);
        let graph = bench.graph;
        let om = bench.id_of(Window::ObjectManager);
        let series = stokesforge_plot::SeriesRef::new(0, 0);
        let target = stokesforge_app_state::object_manager::VisibilityTarget::Series(series);
        let ctx = bench.ctx.clone();
        bench.app.state.dispatch(Action::OpenAbout);
        let mut actions = Vec::new();
        bench.app.run_object_manager_requests(
            &ctx,
            graph,
            vec![Request::Select(target)],
            &mut actions,
            Some(om),
        );
        assert!(
            window_band::text_at(&ctx, om, Instant::now()).is_none(),
            "a selection goes through"
        );
        assert_eq!(
            bench
                .app
                .read_graph_view(
                    graph,
                    super::super::object_manager_ui::selected_manager_target
                )
                .flatten(),
            Some(target),
            "and selects"
        );
        bench.app.run_object_manager_requests(
            &ctx,
            graph,
            vec![Request::Properties(target)],
            &mut actions,
            Some(om),
        );
        assert!(actions.is_empty(), "the rest is refused");
        assert!(window_band::text_at(&ctx, om, Instant::now()).is_some());
        bench.finish();
    }

    /// A press in a panel window ends the cell entry held by a detached workbook,
    /// as a click in another workbook would, under the panel's own identity: with
    /// a macro of the main window open the commit is refused and the cell stays.
    /// Witness: paint and commit with the main identity (the write is absorbed in
    /// the macro).
    #[test]
    fn a_press_in_a_panel_ends_the_entry_of_a_detached_workbook_under_its_identity() {
        let _language = stokesforge_app_state::i18n::test_language(
            stokesforge_app_state::i18n::Language::English,
        );
        let mut bench = started(true);
        let book = bench.book();
        let sheet = bench.app.state.selected_sheet_in(book).unwrap();
        let column = bench.app.state.tree().children(sheet).unwrap()[0];
        let before = bench.app.state.cell_text(column, 0);
        bench.app.state.select_cell(sheet, column, 0);
        let id = bench.id_of(Window::Book);
        bench.child_in(id, Pass::child());
        for _ in 0..2 {
            bench.child_in(
                id,
                Pass::child().events(vec![
                    egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                    egui::Event::Text("7".into()),
                ]),
            );
            for _ in 0..3 {
                bench.child_in(id, Pass::child());
            }
        }
        assert!(bench.app.state.editing().is_some(), "the entry is open");
        bench.app.state.open_root_macro_for_test();
        assert!(bench.app.state.open_dialog().is_none(), "only the macro");
        click(&mut bench, Window::ProjectManager, egui::pos2(100.0, 560.0));
        assert_eq!(bench.app.state.cell_text(column, 0), before, "not written");
        assert!(bench.app.state.status().is_some_and(|s| s.is_error()));
        bench.app.state.cancel_plot_settings();
        assert_eq!(bench.app.state.cell_text(column, 0), before);
        assert!(bench.app.state.undo_label().is_none(), "no entry was made");
        // Without the macro the same press commits the entry (one entry).
        bench.finish();
    }

    /// Same press without a macro: the entry ends, in one history entry.
    /// Witness: drop `PassOf::Panel` (the entry stays open).
    #[test]
    fn a_press_in_a_panel_commits_the_entry_of_a_detached_workbook() {
        let mut bench = started(true);
        let book = bench.book();
        let sheet = bench.app.state.selected_sheet_in(book).unwrap();
        let column = bench.app.state.tree().children(sheet).unwrap()[0];
        bench.app.state.select_cell(sheet, column, 0);
        let id = bench.id_of(Window::Book);
        bench.child_in(id, Pass::child());
        for _ in 0..2 {
            bench.child_in(
                id,
                Pass::child().events(vec![
                    egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                    egui::Event::Text("7".into()),
                ]),
            );
            for _ in 0..3 {
                bench.child_in(id, Pass::child());
            }
        }
        assert!(bench.app.state.editing().is_some());
        click(&mut bench, Window::Console, egui::pos2(100.0, 30.0));
        assert!(bench.app.state.editing().is_none(), "the entry ended");
        assert!(bench.app.state.undo_label().is_some(), "one entry");
        bench.finish();
    }

    /// #471: the detached console refuses a run while a dialog is open, with the
    /// band in its own window, and runs it once the dialog is closed. Witness:
    /// apply the actions without the gate.
    #[test]
    fn the_detached_console_refuses_a_run_while_a_dialog_is_open() {
        let mut bench = started(true);
        let console = bench.id_of(Window::Console);
        let run = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        };
        bench.app.state.dispatch(Action::OpenAbout);
        bench.root(Pass::root());
        bench.child_in(console, Pass::child().events(vec![run.clone()]));
        assert!(
            window_band::text_at(&bench.ctx, console, Instant::now()).is_some(),
            "band in the console"
        );
        bench.app.state.dispatch(Action::CloseAbout);
        bench.root(Pass::root());
        window_band::clear(&bench.ctx, console);
        let recorded = bench.app.state.recorded_count();
        bench.child_in(console, Pass::child().events(vec![run]));
        assert!(
            window_band::text_at(&bench.ctx, console, Instant::now()).is_none()
                || bench.app.state.recorded_count() > recorded,
            "the run goes through the same path as a docked console"
        );
        bench.finish();
    }

    /// The console shows the output of a run started from its own window, and a
    /// run is refused while a macro of the main window is open, as for a script
    /// (the script writes under an identity of its own). Python feature.
    #[cfg(feature = "python")]
    #[test]
    fn a_command_typed_in_the_detached_console_runs_and_shows_its_output() {
        use stokesforge_python::interpreter::{self, EmbeddedPython};
        let mut bench = started(true);
        let home = std::env::var_os(interpreter::HOME_ENV)
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../vendor/python/extracted/python")
            });
        bench.app.state.set_python(EmbeddedPython::with_home(home));
        let console = bench.id_of(Window::Console);
        let run = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        };
        // The source typed in the window is the state's own.
        bench
            .app
            .state
            .set_script_source("print('console_out')".into());
        bench.child_in(console, Pass::child());
        bench.child_in(console, Pass::child().events(vec![run.clone()]));
        let deadline = Instant::now() + Duration::from_secs(20);
        while bench.app.state.script_running() {
            assert!(Instant::now() < deadline, "the script must finish");
            std::thread::sleep(Duration::from_millis(5));
            bench.root(Pass::root());
        }
        bench.root(Pass::root());
        let output = bench.child_in(console, Pass::child());
        assert!(
            super::super::test_support::painted(&output, "console_out"),
            "the output shows in the detached console"
        );
        // With a macro of the main window open, a script that writes is refused.
        bench.app.state.open_root_macro_for_test();
        let books = bench.app.state.books().len();
        bench
            .app
            .state
            .set_script_source("import pyst\npyst.create_workbook('FromConsole')".into());
        bench.child_in(console, Pass::child());
        bench.child_in(console, Pass::child().events(vec![run]));
        let deadline = Instant::now() + Duration::from_secs(20);
        while bench.app.state.script_running() {
            assert!(Instant::now() < deadline, "the script must finish");
            std::thread::sleep(Duration::from_millis(5));
            bench.root(Pass::root());
        }
        bench.root(Pass::root());
        bench.app.state.cancel_plot_settings();
        bench.app.state.refresh();
        assert_eq!(bench.app.state.books().len(), books, "refused");
        bench.finish();
    }

    /// Dragging a node from the tree or opening a window from the detached
    /// Project Manager opens it in the main window in the same pass of the
    /// panel: the tab is open for the root and the other windows. Witness: queue
    /// the open request for the root.
    #[test]
    fn opening_a_window_from_the_detached_project_manager_is_done_in_its_pass() {
        let mut bench = started(true);
        let tab = Tab::Book(bench.book());
        // Close the workbook's window first: it is detached; close it from the
        // panel with its close request.
        bench.app.close_detached_tab(tab);
        assert!(!bench.app.open_tabs().contains(&tab));
        let name = bench
            .app
            .state
            .tree()
            .node(bench.book())
            .unwrap()
            .name()
            .to_string();
        let row = row_of(&mut bench, &name);
        let pm = bench.id_of(Window::ProjectManager);
        let press = |pressed| {
            Pass::child().events(vec![egui::Event::PointerButton {
                pos: row,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }])
        };
        // Double click.
        for _ in 0..2 {
            bench.child_in(
                pm,
                Pass::child().events(vec![egui::Event::PointerMoved(row)]),
            );
            bench.child_in(pm, press(true));
            bench.child_in(pm, press(false));
        }
        assert!(
            bench.app.open_tabs().contains(&tab),
            "opened by the panel's pass alone"
        );
        bench.finish();
    }
}
