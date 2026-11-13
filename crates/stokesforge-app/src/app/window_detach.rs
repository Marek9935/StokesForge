//! Detaching into OS windows: the three fixed panels and each content window
//! can live in a dedicated viewport (`show_viewport_deferred`).
//! The closure of this callback is `'static` and `Send + Sync`.
//!
//! Moved from `app.rs` (#101), then split by phase (#124). **The detached graph
//! and the detached workbook are no longer here** (#475, L1 and L2): their
//! windows lock the whole application and serve themselves
//! (`detached_graph.rs`, `detached_book.rs`): they paint the shared state with
//! the very function of the floating window, apply the actions of their own
//! gestures in the same pass, and the workbook's sender identity (#21) is
//! swapped into the state for its paint. What remains in this file, the three
//! panels (until L3), still works on a copy and queues its business actions for
//! the root. That stays true when such a window covers the main one (#457): on
//! native Wayland the covered root gets no frame callback, so the vendored winit
//! reports it occluded after 100 ms and eframe runs its `logic` and `ui`
//! unpainted about every 100 ms (`vendor/winit/STOKESFORGE.md`, "Frame
//! starvation"). The viewport's request wakes the root and the root applies it.
//!
//! Moving and resizing a detached window is handed to the OS through
//! [`super::os_gesture`], the only path for `StartDrag` / `BeginResize`: the
//! compositor keeps the button release, so the helper plays it for egui
//! (#457). Without it every second grab of a title or an edge did nothing.
//!
//! The dialogs of the application are always painted by the root, whatever
//! window opened them (#472): a detached window only shows a band that says
//! where the dialog is (`app.rs`, `announce_opened_dialog`).
//!
//! The graph, the workbook and the three panels read Ctrl+Z, Ctrl+Y and Ctrl+S
//! at the top of their frame, before anything is painted, and put the actions
//! after their own (#459, `shortcuts::read_detached_history_and_save`; the graph
//! and the workbook apply them at once, the others queue them for the root);
//! Ctrl+Q closes nothing there (`shortcuts::refuse_quit_in_detached`). No other
//! global key is read in a detached window.
//!
//! # Taken from the top doc of `app.rs`
//!
//! # Detaching the Project Manager
//!
//! The `egui_dock` floating (`Surface::Window`) that a tab could get
//! by being dragged out of the dock no longer exists for the Project Manager,
//! since it is no longer a tab — only this remains:
//!
//! **A real separate OS window** ([`StokesForgeApp::pm_detached`],
//! `detached_panel.rs`): `egui::Context::
//! show_viewport_deferred` opens a real native viewport, movable to
//! another screen. When this mode is active, [`StokesForgeApp::project_manager_panel`]
//! is simply not painted that frame; closing it (button
//! of the OS window, or "Reattach") sets `pm_detached` back to `false`, which
//! makes the panel reappear at the next render — always in the same
//! place, since it is fixed on the left. Can be triggered from its header
//! or the "Window" menu. Under Wayland, the compositor may ignore a window
//! position
//! requested by the application — the same caveat already documented in
//! `docs/application/theme-i18n-system.md` for `rfd`; this task explicitly asks for
//! no position (it lets the compositor place the window), so it does not
//! suffer from it directly. Since #148 the size and position of the three
//! panels' windows are remembered (`panel_memory.rs`): the position is kept only
//! where winit can read it, and the viewport builders take the saved values
//! only when the window opens (`panel_viewport`).
//!
//! **`show_viewport_deferred`, not `_immediate`**: observed
//! interactively (Fedora/GNOME/Wayland, Mutter), the window detached through
//! `show_viewport_immediate` stayed draggable but visually confined
//! to the parent window's rectangle, even when reduced — not just
//! the lack of absolute positioning above, something else kept the
//! window from leaving the frame. `show_viewport_immediate` itself documents
//! "pause the current viewport and render the child viewport in
//! its own window" (coupled rendering of the two windows), and `egui`
//! explicitly recommends `show_viewport_deferred` for a real
//! independent window, repainted independently of the parent window —
//! a plausible candidate for this symptom, **not proof**. A detached window
//! serves itself under the application lock (see § The workbook and the graph
//! share their rendering and their state, and `detached_panel.rs`).
//!
//! # Generic detaching of content tabs
//!
//! [`StokesForgeApp::detach_tab`]/[`StokesForgeApp::reattach_tab`] move a
//! [`Tab::Book`] or a [`Tab::Graph`] only ([`Tab::is_detachable`], #473: a note
//! and a matrix stay in the main window) between the floating windows and an OS
//! window — a "Detach into a window" button in the header of each floating
//! window (see § Confined floating windows), symmetric with "Reattach to the
//! main window" on the detached window side. Several tabs can be detached at the
//! same time: [`StokesForgeApp::detached_books`] and
//! [`StokesForgeApp::detached_graphs`] hold one view state per tab (a
//! `ViewportId::from_hash_of(tab)` is unique by construction).
//!
//! B8 (`retours-2026-09-23-bugs.md`): as for a shrunken floating window (B4), a
//! `Ui` whose children exceed its `max_rect` still paints them. Both windows put
//! their content in a single bounded, clipped child `Ui`
//! ([`show_window_contents_bounded`]).
//!
//! # The workbook and the graph share their rendering and their state
//!
//! [`tab_contents`](crate::app::contents::tab_contents) takes `&mut AppState`.
//! A detached window gets it by locking the application from its own closure
//! ([`lock_app_from_viewport`](super::server::lock_app_from_viewport)); there is
//! no copy of the state any more (the `fork_for_detached_view` of #21 and #106
//! is gone, and with it `sync_detached_*`, `adopt_detached_*` and the queue back
//! to the root). The selection, the clipboard, the entry in progress and the
//! renaming of a sheet are the application's: one cell entry in the whole
//! application, ended by a press in another window
//! ([`StokesForgeApp::commit_entry_held_elsewhere`]). The graph's
//! [`GraphView`](stokesforge_render_egui::GraphView) migrates to
//! [`StokesForgeApp::detached_graphs`] for the duration of the detaching.
//!
//! # The three panels (#100, #475)
//!
//! The detached Project Manager, Object Manager and console do not become content
//! tabs, and they serve themselves like the graph and the workbook
//! (`detached_panel.rs`): the closure locks the application, paints with the
//! function of the docked panel and applies its actions at once; no copy, no
//! queue. They all use `detached_panel_contents`: home-made frame without native
//! decoration, Reattach only, OS close converted into a reattach.

use eframe::egui;
use stokesforge_core::NodeId;
use stokesforge_session::{Snapshot, WindowGeometry};

use stokesforge_app_state::panel_layout::DetachAt;

use super::detached_book::DetachedBook;
use super::detached_graph::DetachedGraph;
use super::tab::{tab_node, Tab};
use super::window_chrome::{
    detached_resize_handles, paint_active_window_outline, panel_header,
    show_window_contents_bounded, PanelPlacement, WINDOW_CONTENT_MARGIN,
};
use super::window_stack::toggle_collapse_to;
use super::StokesForgeApp;

/// The two counters are not enough after a project replacement:
/// the epoch also belongs to the identity of the published snapshot.
pub(super) fn snapshot_stamp(snapshot: &Snapshot) -> (u64, u64, u64) {
    (
        snapshot.epoch,
        snapshot.revision,
        snapshot.geometry_revision,
    )
}

/// The detached panels have the home-made frame and a single window action.
/// A close requested by the compositor becomes a reattach.
pub(super) fn detached_panel_contents(
    ui: &mut egui::Ui,
    title: &str,
    contents: impl FnOnce(&mut egui::Ui),
) -> bool {
    let mut reattach = false;
    egui::CentralPanel::default()
        .frame(
            egui::Frame::central_panel(ui.style())
                .inner_margin(WINDOW_CONTENT_MARGIN)
                .stroke(ui.visuals().window_stroke()),
        )
        .show(ui, |ui| {
            let header = panel_header(ui, title, PanelPlacement::Detached, false);
            reattach = header.detach;
            if header.title_drag_started {
                super::os_gesture::start_drag(ui.ctx());
            }
            show_window_contents_bounded(ui, contents);
        });
    if ui.ctx().input(|input| input.focused) {
        paint_active_window_outline(
            &ui.ctx().layer_painter(egui::LayerId::background()),
            ui.ctx().input(|input| input.content_rect()),
            egui::CornerRadius::ZERO,
            ui.style(),
        );
    }
    detached_resize_handles(ui);
    if ui.ctx().input(|input| input.viewport().close_requested()) {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::CancelClose);
        reattach = true;
    }
    reattach
}

/// The builder of the OS window of a detached panel: undecorated, `default_size`
/// unless the saved layout says where it opens (#148). The builder is the same on
/// every frame of the detached session: egui sends a size or position command
/// when the builder changes, which would fight the user's drag.
pub(super) fn panel_viewport(
    title: String,
    default_size: [f32; 2],
    min_size: [f32; 2],
    init: Option<DetachAt>,
) -> egui::ViewportBuilder {
    let builder = crate::app_logo::viewport()
        .with_title(title)
        .with_decorations(false)
        .with_inner_size(init.map_or(default_size, |at| at.size))
        .with_min_inner_size(min_size);
    match init.and_then(|at| at.position) {
        Some(position) => builder.with_position(position),
        None => builder,
    }
}

/// The viewport of the detached Object Manager.
pub(super) fn object_manager_viewport() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("stokesforge_object_manager_window")
}

/// The viewport of the detached script console.
pub(super) fn script_viewport() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("stokesforge_script_window")
}

/// The body of a detached window: what `show_viewport_deferred` runs for each
/// of its passes.
pub(super) type ViewportBody = Box<dyn Fn(&mut egui::Ui, egui::ViewportClass) + Send + Sync>;

/// A window `ui` wants published this pass (#475, step 2). Published by
/// [`publish_viewports`] once the application lock is released, so that the
/// body, which may run inside the pass (embedded viewports), can lock it.
pub(super) struct PendingViewport {
    pub(super) id: egui::ViewportId,
    pub(super) builder: egui::ViewportBuilder,
    pub(super) body: ViewportBody,
    /// Wake the window after the publication (its content changed).
    pub(super) repaint: bool,
}

/// THE place that registers a deferred viewport (`tests/guards.rs`:
/// `show_viewport_deferred_lives_only_in_the_publish_function`).
pub(super) fn publish_viewports(ctx: &egui::Context, pending: Vec<PendingViewport>) {
    for PendingViewport {
        id,
        builder,
        body,
        repaint,
    } in pending
    {
        ctx.show_viewport_deferred(id, builder, body);
        if repaint {
            ctx.request_repaint_of(id);
        }
    }
}

impl StokesForgeApp {
    /// Asks for the publication of a detached window: queued when a
    /// [`ServedApp`](super::ServedApp) publishes after releasing the lock,
    /// at once otherwise (a test that drives `ui` on a plain app).
    pub(super) fn publish_viewport(
        queue: &mut Option<Vec<PendingViewport>>,
        ctx: &egui::Context,
        id: egui::ViewportId,
        builder: egui::ViewportBuilder,
        repaint: bool,
        body: impl Fn(&mut egui::Ui, egui::ViewportClass) + Send + Sync + 'static,
    ) {
        let pending = PendingViewport {
            id,
            builder,
            body: Box::new(body),
            repaint,
        };
        match queue {
            Some(queue) => queue.push(pending),
            None => publish_viewports(ctx, vec![pending]),
        }
    }

    /// From now on `ui` queues the windows instead of publishing them.
    pub(super) fn defer_viewport_publication(&mut self) {
        self.pending_viewports.get_or_insert_with(Vec::new);
    }

    /// The windows queued by the pass that just ended, to publish after the lock
    /// is released.
    pub(super) fn take_pending_viewports(&mut self) -> Vec<PendingViewport> {
        self.pending_viewports
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    /// Removes `tab` from the floating windows and detaches it into a separate
    /// OS window — see the top doc of the module, § generic detaching. No
    /// effect if `tab` is already detached or is not a currently
    /// open floating window. Neither the Project Manager nor the script console
    /// goes through this path since they became fixed panels — see
    /// [`StokesForgeApp::detach_project_manager`] and
    /// [`StokesForgeApp::detach_script_console`] for their own paths.
    pub(super) fn detach_tab(&mut self, tab: Tab) {
        if self.is_tab_detached(tab) {
            return;
        }
        if !self.floating_tabs.contains(&tab) {
            return;
        }
        // A note and a matrix stay in the main window (#473).
        if !tab.is_detachable() {
            return;
        }
        // The Page Layout Mode ends before the view moves to its own window.
        if let Tab::Graph(graph) = tab {
            self.leave_layout_mode(graph);
        }
        if let Some(pos) = self.floating_tabs.iter().position(|t| *t == tab) {
            self.floating_tabs.remove(pos);
            self.cascade_rank.remove(&tab);
        }
        if self.maximized_tab == Some(tab) {
            // A maximized tab that was no longer a floating window
            // would be an inconsistent state: step down cleanly before
            // detaching it.
            self.maximized_tab = None;
        }
        self.window_positions.remove(&tab);
        self.window_sizes.remove(&tab);
        if let Tab::Graph(graph) = tab {
            self.figure_follow.remove(&graph);
            // The view moves with its cache to the window's state; it comes
            // back, the same one, on Reattach or Close.
            let mut view = self.graph_views.remove(&graph).unwrap_or_default();
            view.invalidate_surface();
            self.detached_graphs.insert(
                graph,
                DetachedGraph::new(
                    view,
                    self.collapsed_tabs.contains(&tab),
                    self.state.new_session_identity(),
                ),
            );
            return;
        }
        if let Tab::Book(book) = tab {
            // The window owns a sender identity of its own (#21).
            let identity = self.state.new_session_identity();
            self.detached_books.insert(
                book,
                DetachedBook::new(identity, self.collapsed_tabs.contains(&tab)),
            );
        }
    }

    /// Removes `tab` from the detached windows and pushes it back as a
    /// floating window (new cascade rank). No effect if it is not
    /// detached. The [`GraphView`] of a graph returns to
    /// [`StokesForgeApp::graph_views`] — the graph's shape cache survives
    /// detaching/reattaching, as it already survives the closing of a floating
    /// window.
    pub(super) fn reattach_tab(&mut self, tab: Tab) {
        if let Tab::Graph(graph) = tab {
            let Some(window) = self.release_graph_window(graph) else {
                return;
            };
            let mut view = window.view;
            view.invalidate_surface();
            self.graph_views.insert(graph, view);
            toggle_collapse_to(&mut self.collapsed_tabs, tab, window.collapsed);
            self.figure_follow.remove(&graph);
            self.leave_layout_mode(graph);
        } else if let Tab::Book(book) = tab {
            let Some(window) = self.release_book_window(book) else {
                return;
            };
            toggle_collapse_to(&mut self.collapsed_tabs, tab, window.collapsed);
        } else {
            return;
        }
        if !self.floating_tabs.contains(&tab) {
            self.push_floating_tab(tab);
        }
    }

    /// Closes only the detached view. The data stays in the project
    /// and the graph's cache returns to the application, as after the
    /// closing of a floating window.
    pub(super) fn close_detached_tab(&mut self, tab: Tab) {
        if let Tab::Graph(graph) = tab {
            let Some(window) = self.release_graph_window(graph) else {
                return;
            };
            let mut view = window.view;
            view.invalidate_surface();
            self.graph_views.insert(graph, view);
            // Closing the window ends the Page Layout Mode.
            self.leave_layout_mode(graph);
        } else if let Tab::Book(book) = tab {
            self.commit_entry_of_closing_book(book);
            self.release_book_window(book);
        }
    }

    /// Sends the geometry actually kept by egui after a move
    /// or resize. This view information is persistent but
    /// stays outside the business history.
    pub(super) fn remember_window_geometry(&mut self, tab: Tab, rect: egui::Rect) {
        self.floating_rects.insert(tab_node(tab), rect);
    }

    /// Sends the session the floating geometry of each known window
    /// with its current state (open/maximized/closed, collapsed, stacking
    /// rank) — only what changed since the last send.
    /// A detached window counts as open: the detaching is not
    /// remembered, and the position of its OS window is never written.
    pub(super) fn sync_window_views(&mut self) {
        let open_order: Vec<NodeId> = self
            .floating_tabs
            .iter()
            .map(|tab| tab_node(*tab))
            .collect();
        let mut detached: Vec<NodeId> = self
            .detached_tab_list()
            .iter()
            .map(|t| tab_node(*t))
            .collect();
        detached.sort_by_key(|node| node.raw());
        let nodes: Vec<NodeId> = self.floating_rects.keys().copied().collect();
        for node in nodes {
            let rect = self.floating_rects[&node];
            let stack = open_order
                .iter()
                .position(|n| *n == node)
                .or_else(|| {
                    detached
                        .iter()
                        .position(|n| *n == node)
                        .map(|i| open_order.len() + i)
                })
                .unwrap_or(0);
            let is_open = open_order.contains(&node) || detached.contains(&node);
            let mode = if self.reported_maximized_tab().map(tab_node) == Some(node) {
                stokesforge_session::WindowMode::Maximized
            } else if is_open {
                stokesforge_session::WindowMode::Open
            } else {
                stokesforge_session::WindowMode::Closed
            };
            let collapsed = self.collapsed_tabs.iter().any(|tab| tab_node(*tab) == node);
            let geometry = WindowGeometry {
                x: f64::from(rect.min.x),
                y: f64::from(rect.min.y),
                width: f64::from(rect.width()),
                height: f64::from(rect.height()),
                view: Some(stokesforge_session::WindowView {
                    mode,
                    collapsed,
                    stack: u32::try_from(stack).unwrap_or(u32::MAX),
                }),
            };
            if self.saved_window_geometries.get(&node) != Some(&geometry) {
                self.state.set_window_geometry(node, geometry);
                self.saved_window_geometries.insert(node, geometry);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Detaching into OS windows, driven by the viewport callbacks
    //! that egui publishes: the publication of the snapshot that wakes the
    //! readers (#104), the window state sent to the session (#16), the
    //! file drops isolated from the root, the figure rebuilt in the
    //! viewport, the clipping (B8), the OS viewport collapse (B9) and the focus
    //! outline (B7). Moved from `app.rs` with their code (#123).
    use std::sync::Arc;
    use std::time::Duration;

    use super::super::file_drop::{begin_file_drop, finish_file_drop};
    use super::super::graph_window::tests::plotted_graph;
    use super::super::grid::role_label;
    use super::super::test_support::bench::delay;
    use super::super::test_support::bench::{Bench, Pass, Window};
    use super::super::window_chrome::tests::find_icon_centers;
    use super::super::window_chrome::{WINDOW_CONTENT_MARGIN, WINDOW_HEADER_HEIGHT};
    use super::super::window_placement::GRAPH_WINDOW_SIZE;
    use super::*;
    use stokesforge_app_state::state::{Action, AppState};
    use stokesforge_app_state::tr;
    use stokesforge_render_egui::GraphView;

    /// A detached panel carries the application id: egui copies the icon to a
    /// child window, not the id (#388).
    #[test]
    fn a_detached_panel_has_the_application_id() {
        let builder = panel_viewport("Panel".to_string(), [300.0, 200.0], [100.0, 100.0], None);
        assert_eq!(builder.app_id.as_deref(), Some("stokesforge"));
    }
    use stokesforge_core::{Column, ColumnRole, Project};
    use stokesforge_render_egui::DEFAULT_FIGURE_SIZE;
    use stokesforge_session::Session;

    /// The stamp of a snapshot published to the detached windows: each of
    /// its three counters changes it **on its own**. Revision and geometries
    /// revision are not enough: the epoch also belongs to the identity
    /// of the snapshot (a project replacement), and a stamp that
    /// forgot it would leave a detached reader on the old project if the
    /// two other counters coincided.
    #[test]
    fn each_of_the_three_counters_changes_the_stamp_of_a_snapshot() {
        let session = Session::spawn(Project::new("Stamp").unwrap(), None).unwrap();
        let base = session.handle().snapshot().unwrap();
        let stamp = snapshot_stamp(&base);
        assert_eq!(
            stamp,
            (base.epoch, base.revision, base.geometry_revision),
            "the stamp reads the snapshot's counters"
        );
        // The witness: the same snapshot, recloned, keeps its stamp.
        assert_eq!(snapshot_stamp(&base.clone()), stamp);

        let mut epoch = base.clone();
        epoch.epoch += 1;
        let mut revision = base.clone();
        revision.revision += 1;
        let mut geometries = base.clone();
        geometries.geometry_revision += 1;
        let changed = [
            ("epoch", snapshot_stamp(&epoch)),
            ("revision", snapshot_stamp(&revision)),
            ("geometries revision", snapshot_stamp(&geometries)),
        ];
        for (counter, other) in changed {
            assert_ne!(other, stamp, "{counter} alone must change the stamp");
        }
        // Each changes a different component: two counters mixed up
        // would give two equal stamps.
        let distinct: std::collections::HashSet<(u64, u64, u64)> =
            changed.iter().map(|(_, other)| *other).collect();
        assert_eq!(distinct.len(), changed.len(), "{changed:?}");
    }

    fn persisted_view(session: &Session, node: NodeId) -> WindowGeometry {
        *session
            .handle()
            .snapshot()
            .unwrap()
            .window_geometries
            .get(&node)
            .expect("geometry sent to the session")
    }

    /// #124, L2 of #475: the OS close of a detached workbook closes the view
    /// only (it does not reattach). Where the window cannot close itself (X11,
    /// Windows) it raises a flag and stays until the root drains it; on native
    /// Wayland the window leaves in its own pass. Witness: reattach instead of
    /// closing (the tab floats); or let the Wayland window wait for the root.
    #[test]
    fn a_detached_workbook_closes_only_the_view_on_the_os_cross() {
        let close = || {
            Pass::child().info(egui::ViewportInfo {
                events: vec![egui::ViewportEvent::Close],
                ..Default::default()
            })
        };
        for wayland in [false, true] {
            let mut bench = Bench::with_detached_book();
            bench.app.windows_close_themselves = wayland;
            bench.start_book();
            let book = bench.book();
            let id = bench.book_id();
            bench.child_in(id, close());
            if wayland {
                assert!(
                    !bench.app.detached_books.contains_key(&book),
                    "the window closed itself, no root pass"
                );
                assert!(!bench.has_window(id), "and stopped registering");
            } else {
                let window = &bench.app.detached_books[&book];
                assert!(window.close, "the OS cross goes back to the root");
                assert!(!window.reattach, "the cross closes the detached view");
                bench.root(Pass::root());
                assert!(!bench.app.detached_books.contains_key(&book));
            }
            assert!(
                !bench.app.floating_tabs.contains(&Tab::Book(book)),
                "closed, not reattached"
            );
            bench.finish();
        }
    }

    /// #473: only the workbook and the graph detach. A note and a matrix
    /// stay floating, whoever asks.
    #[test]
    fn a_note_and_a_matrix_do_not_detach() {
        let mut project = Project::new("Not detachable").unwrap();
        let note = project
            .add_note(project.root(), Some("Journal"), "Start")
            .unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let matrix_tab = Tab::Matrix(NodeId::from_raw(999));
        for tab in [Tab::Note(note), matrix_tab] {
            app.open_tab(tab);
            assert!(app.floating_tabs.contains(&tab), "{tab:?} must open");
            app.detach_tab(tab);
            assert!(
                app.floating_tabs.contains(&tab),
                "{tab:?} must stay floating"
            );
            assert!(!app.is_tab_detached(tab), "{tab:?}");
        }
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book = Tab::Book(app.state.books()[0].0);
        for tab in [book, Tab::Graph(graph)] {
            app.detach_tab(tab);
            assert!(app.is_tab_detached(tab), "{tab:?} must detach");
            assert!(!app.floating_tabs.contains(&tab), "{tab:?}");
        }
    }

    /// #104 (T7): at rest every viewport sleeps, the root and the five windows;
    /// a write made in the root's pass wakes them all; a navigation selection
    /// wakes the Project Manager only. No periodic
    /// wake-up. Witness: an unconditional wake at the end of the root pass.
    #[test]
    fn a_mutation_wakes_the_detached_windows_at_rest() {
        let mut bench = Bench::with_every_window();
        bench.start_every_window();
        let ids: Vec<_> = Window::ALL.iter().map(|w| bench.id_of(*w)).collect();
        let every: Vec<_> = std::iter::once(egui::ViewportId::ROOT)
            .chain(ids.iter().copied())
            .collect();
        let settle = |bench: &mut Bench| {
            for _ in 0..8 {
                bench.root(Pass::root());
                for id in &ids {
                    bench.child_in(*id, Pass::child().focused(false));
                }
            }
        };
        settle(&mut bench);
        let output = bench.root(Pass::root());
        for id in &every {
            assert_eq!(
                delay(&output, *id),
                Duration::MAX,
                "the viewport {id:?} must sleep at rest"
            );
        }
        // Something to undo, then Ctrl+Z in the root: a write of the root's pass.
        bench.app.state.dispatch(Action::AddWorkbook);
        settle(&mut bench);
        let ctrl_z = egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        };
        let output = bench.root(Pass::root().focused(true).events(vec![ctrl_z]));
        for id in &ids {
            assert_eq!(
                delay(&output, *id),
                Duration::ZERO,
                "the mutation must wake {id:?}"
            );
        }
        settle(&mut bench);
        let book = bench.book();
        bench
            .app
            .pm_selection
            .insert(super::super::project_manager::PmEntry::Window(Tab::Book(
                book,
            )));
        let output = bench.root(Pass::root());
        assert_eq!(
            delay(&output, bench.id_of(Window::ProjectManager)),
            Duration::ZERO,
            "a navigation selection wakes the Project Manager"
        );
        for window in [Window::Graph, Window::Book] {
            assert_eq!(delay(&output, bench.id_of(window)), Duration::MAX);
        }
        settle(&mut bench);
        bench.finish();
    }

    /// #426: a Page Layout Mode asked from inside a detached window (its
    /// right-click Layout menu) starts at once. The request is run by the root,
    /// so it must wake the root, not only the detached viewport; the root then
    /// wakes the viewport, which maximises its OS window. Witness: the request
    /// waking only the viewport it was made in.
    #[test]
    fn a_page_layout_mode_asked_in_a_detached_window_starts_without_another_click() {
        let mut bench = Bench::new();
        bench.start();
        for _ in 0..6 {
            bench.root(Pass::root());
            bench.child(Pass::child());
        }
        let id = bench.child_id();
        let graph = bench.graph;
        let output = bench.root(Pass::root());
        assert_eq!(delay(&output, id), Duration::MAX);
        assert_eq!(
            delay(&output, egui::ViewportId::ROOT),
            Duration::MAX,
            "the bench must be at rest"
        );
        // The menu entry, clicked in the detached window.
        let child_output = bench.child_with(id, Pass::child(), |ctx| {
            super::super::page_layout::request_layout_mode_toggle(ctx, graph);
        });
        assert_eq!(
            delay(&child_output, egui::ViewportId::ROOT),
            Duration::ZERO,
            "the root must run the request"
        );
        // The root frame that follows enters the mode and wakes the window.
        let output = bench.root(Pass::root());
        assert_eq!(
            bench.app.read_graph_view(graph, GraphView::layout_mode),
            Some(true)
        );
        assert_eq!(delay(&output, id), Duration::ZERO);
        let child_output = bench.child(Pass::child());
        let commands = &child_output.viewport_output[&id].commands;
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Maximized(true))),
            "{commands:?}"
        );
        bench.finish();
    }

    /// The Restore button of a detached graph's header, pressed while the window
    /// is maximised in the Page Layout Mode, leaves the mode and restores the
    /// OS window, as the Restore of a floating window does (#457). Pressed
    /// on a window that is not maximised it maximises and keeps the mode: a
    /// restore the application did not see (an OS shortcut) leaves the mode
    /// alone. Witness: drop the `leave_mode` call of the header's maximise.
    #[test]
    fn the_restore_button_of_a_detached_graph_leaves_the_page_layout_mode() {
        for (maximized, leaves) in [(true, true), (false, false)] {
            let mut bench = Bench::new();
            let graph = bench.graph;
            bench.app.toggle_layout_mode(graph, Some(true));
            bench.start();
            let id = bench.child_id();
            let info = egui::ViewportInfo {
                maximized: Some(maximized),
                ..Default::default()
            };
            let pass = |events: Vec<egui::Event>| {
                Pass::child()
                    .size(1400.0, 900.0)
                    .info(info.clone())
                    .events(events)
            };
            bench.child_in(id, pass(Vec::new()));
            let first = bench.child_in(id, pass(Vec::new()));
            // The three icons at the right end of the header band: maximise /
            // restore, reattach, close.
            let in_header: Vec<egui::Pos2> = find_icon_centers(&first.shapes)
                .into_iter()
                .filter(|pos| pos.y < WINDOW_HEADER_HEIGHT)
                .collect();
            let restore = in_header[in_header.len() - 3];
            bench.child_in(id, pass(vec![egui::Event::PointerMoved(restore)]));
            let button = |pressed| egui::Event::PointerButton {
                pos: restore,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            bench.child_in(id, pass(vec![button(true)]));
            let clicked = bench.child_in(id, pass(vec![button(false)]));
            let commands = &clicked.viewport_output[&id].commands;
            assert!(
                commands
                    .iter()
                    .any(|c| matches!(c, egui::ViewportCommand::Maximized(m) if *m == !maximized)),
                "maximized={maximized}: {commands:?}"
            );
            assert_eq!(
                bench.app.read_graph_view(graph, GraphView::layout_mode),
                Some(!leaves),
                "maximized={maximized}"
            );
            bench.finish();
        }
    }

    /// #426: the strip of a detached graph keeps answering: each click on its
    /// controls takes effect, not only the first. The clicks go through the
    /// detached viewport's own input, as in the real window, and the window
    /// applies them in its own pass; the root is painted only when the viewport
    /// asked for it.
    #[test]
    fn the_strip_of_a_detached_graph_answers_every_click() {
        let mut bench = Bench::new();
        let graph = bench.graph;
        bench.app.toggle_layout_mode(graph, Some(true));
        bench.start();
        let id = bench.child_id();
        let info = egui::ViewportInfo {
            maximized: Some(true),
            ..Default::default()
        };
        let paint_child = |bench: &mut Bench, events: Vec<egui::Event>| {
            let output = bench.child_in(
                id,
                Pass::child()
                    .size(1400.0, 900.0)
                    .dt(0.7)
                    .info(info.clone())
                    .events(events),
            );
            if delay(&output, egui::ViewportId::ROOT) == Duration::ZERO {
                bench.root(Pass::root().size(1400.0, 900.0).dt(0.7));
            }
            output
        };
        let mut last = paint_child(&mut bench, Vec::new());
        for _ in 0..6 {
            last = paint_child(&mut bench, Vec::new());
        }
        let find = |output: &egui::FullOutput, needle: &str| {
            super::super::test_support::texts(&output.shapes)
                .into_iter()
                .find(|t| t.text == needle)
                .map(|t| t.center)
                .unwrap_or_else(|| panic!("{needle:?} not painted"))
        };
        let fine =
            |bench: &Bench| super::super::page_layout::published_snap(&bench.ctx, graph).fine;
        let snap_on =
            |bench: &Bench| super::super::page_layout::published_snap(&bench.ctx, graph).on;
        let click = |bench: &mut Bench, last: &mut egui::FullOutput, label: String| {
            let pos = find(last, &label);
            for pressed in [None, Some(true), Some(false)] {
                let events = match pressed {
                    None => vec![egui::Event::PointerMoved(pos)],
                    Some(pressed) => vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                };
                *last = paint_child(bench, events);
            }
            for _ in 0..3 {
                *last = paint_child(bench, Vec::new());
            }
        };
        assert!(!fine(&bench));
        // The same spot, four times: the step toggles every time.
        for (round, want) in [true, false, true, false].into_iter().enumerate() {
            click(&mut bench, &mut last, tr!("layout-mode-step-fine"));
            assert_eq!(fine(&bench), want, "step click {round}");
        }
        // Snap: on, off, on.
        for (round, want) in [false, true, false].into_iter().enumerate() {
            click(&mut bench, &mut last, tr!("layout-mode-snap"));
            assert_eq!(snap_on(&bench), want, "snap click {round}");
        }
        // Grid mode and When axes are added reach the model in the window's own
        // pass, and keep answering after each round trip.
        let placement = |bench: &Bench| bench.app.state.plot_spec(graph).unwrap().layer_placement;
        let fixed = |bench: &Bench| bench.app.state.plot_spec(graph).unwrap().fixed_figure;
        for (round, free) in [true, false, true, false].into_iter().enumerate() {
            click(&mut bench, &mut last, tr!("layout-mode-placement-free"));
            assert_eq!(
                placement(&bench) == stokesforge_plot::LayerPlacement::Free,
                free,
                "grid mode click {round}"
            );
        }
        for (round, want) in [true, false, true, false].into_iter().enumerate() {
            click(&mut bench, &mut last, tr!("plot-figure-size-fixed"));
            assert_eq!(fixed(&bench), want, "axes click {round}");
        }
        bench.finish();
    }

    /// #426 (T1): Ctrl+Z, Ctrl+Y and Ctrl+Shift+Z pressed in a detached graph
    /// window itself (Page Layout Mode, focused) reach the model like the same
    /// keys in the main window, **in the window's own pass**: no root pass runs
    /// here. Witness: send the actions back to a root list, or read the
    /// shortcuts only for a workbook (#459 moved the read before the painting).
    #[test]
    fn undo_and_redo_work_from_a_detached_graph_window() {
        let mut bench = Bench::new();
        let graph = bench.graph;
        bench.app.toggle_layout_mode(graph, Some(true));
        bench.start();
        let id = bench.child_id();
        let info = egui::ViewportInfo {
            maximized: Some(true),
            ..Default::default()
        };
        let pass = |events: Vec<egui::Event>| {
            Pass::child()
                .size(1400.0, 900.0)
                .dt(0.7)
                .info(info.clone())
                .events(events)
        };
        for _ in 0..6 {
            bench.child_in(id, pass(Vec::new()));
        }
        let fixed = |bench: &Bench| bench.app.state.plot_spec(graph).unwrap().fixed_figure;
        let key = |key: egui::Key, shift: bool| {
            let mut modifiers = egui::Modifiers::COMMAND.plus(egui::Modifiers::CTRL);
            if shift {
                modifiers = modifiers.plus(egui::Modifiers::SHIFT);
            }
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }]
        };
        let before = fixed(&bench);
        // An edit made from the model, as the strip does.
        bench.app.state.dispatch(Action::SetFigureFixed {
            graph,
            fixed: !before,
        });
        assert_eq!(fixed(&bench), !before);
        for (events, want, what) in [
            (key(egui::Key::Z, false), before, "Ctrl+Z"),
            (key(egui::Key::Y, false), !before, "Ctrl+Y"),
            (key(egui::Key::Z, false), before, "Ctrl+Z again"),
            (key(egui::Key::Z, true), !before, "Ctrl+Shift+Z"),
        ] {
            // The key and nothing else: the history moves in that very pass.
            bench.child_in(id, pass(events));
            assert_eq!(fixed(&bench), want, "{what} in the detached graph window");
        }
        bench.finish();
    }

    /// Issue #16: the state of each window (open/maximized/closed, collapsed,
    /// stacking rank) goes to the session with its floating geometry, and
    /// the latter moves neither on maximizing, nor on closing, nor on
    /// detaching.
    #[test]
    fn the_window_state_is_sent_to_the_session_without_touching_the_floating_geometry() {
        use stokesforge_session::WindowMode;
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book = app.state.books()[0].0;
        let rect = egui::Rect::from_min_size(egui::pos2(40.0, 60.0), egui::vec2(500.0, 300.0));
        app.remember_window_geometry(Tab::Graph(graph), rect);
        app.remember_window_geometry(Tab::Book(book), rect.translate(egui::vec2(9.0, 9.0)));
        app.sync_window_views();

        let open = persisted_view(&session, graph);
        assert_eq!(
            (open.x, open.y, open.width, open.height),
            (40.0, 60.0, 500.0, 300.0)
        );
        let view = open.view.unwrap();
        assert_eq!(view.mode, WindowMode::Open);
        assert!(!view.collapsed);
        // The graph is opened after the workbook: it is on top.
        assert!(view.stack > persisted_view(&session, book).view.unwrap().stack);

        // Collapsed + maximized: state kept, rectangle unchanged.
        app.collapsed_tabs.insert(Tab::Graph(graph));
        app.maximized_tab = Some(Tab::Graph(graph));
        app.sync_window_views();
        let maximized = persisted_view(&session, graph);
        assert_eq!(maximized.view.unwrap().mode, WindowMode::Maximized);
        assert!(maximized.view.unwrap().collapsed);
        assert_eq!((maximized.x, maximized.width), (open.x, open.width));

        // Detached: still "open", floating rectangle intact.
        app.maximized_tab = None;
        app.detach_tab(Tab::Graph(graph));
        app.sync_window_views();
        let detached = persisted_view(&session, graph);
        assert_eq!(detached.view.unwrap().mode, WindowMode::Open);
        assert_eq!((detached.x, detached.y), (open.x, open.y));

        // Closed: the floating geometry stays the one from before.
        app.detached_graphs.clear();
        app.sync_window_views();
        let closed = persisted_view(&session, graph);
        assert_eq!(closed.view.unwrap().mode, WindowMode::Closed);
        assert_eq!(
            (closed.x, closed.y, closed.width, closed.height),
            (40.0, 60.0, 500.0, 300.0)
        );
        // Reopened: it is that rectangle that the paint loop rereads.
        assert_eq!(app.floating_rects[&graph], rect);
    }

    #[derive(Debug)]
    struct FakeFile(std::path::PathBuf);
    impl egui::DroppedFile for FakeFile {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
    }

    /// A file dropped on the detached Project Manager is applied in the window's
    /// own pass (the import starts there, with no destination workbook) and does
    /// not consume the pending drop of the root viewport (shared egui memory).
    /// Witness: take the drop in the root, or leave the window without its
    /// `finish_file_drop`.
    #[test]
    fn a_detached_explorer_drop_keeps_its_destination_and_is_isolated_from_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("detached.csv");
        std::fs::write(&csv, "1,2\n3,4\n").unwrap();
        let mut bench = Bench::with_every_window();
        bench.start_every_window();
        let books = bench.app.state.books().len();
        let root_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800., 600.),
            )),
            dropped_files: vec![Arc::new(FakeFile("root.csv".into()))],
            ..Default::default()
        };
        bench
            .ctx
            .run_ui(root_input.clone(), |ui| begin_file_drop(ui.ctx()))
            .textures_delta
            .clear();
        let pm = bench.id_of(Window::ProjectManager);
        bench.child_in(pm, Pass::child().dropped(vec![Arc::new(FakeFile(csv))]));
        assert!(
            bench.app.state.import_progress().is_some(),
            "the drop is applied by the window's own pass"
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while bench.app.state.import_progress().is_some() {
            bench.app.state.poll_import();
            assert!(std::time::Instant::now() < deadline, "import too long");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(bench.app.state.books().len(), books + 1, "a new workbook");
        let mut root_action = None;
        let mut again = root_input;
        again.dropped_files.clear();
        bench
            .ctx
            .run_ui(again, |ui| root_action = finish_file_drop(ui.ctx()))
            .textures_delta
            .clear();
        assert_eq!(
            root_action,
            Some(Action::DropFiles {
                paths: vec!["root.csv".into()],
                target_book: None
            })
        );
        bench.finish();
    }

    /// A file dropped on a detached workbook goes into THAT workbook (one more
    /// sheet), in the window's own pass, and leaves the pending drop of the
    /// root viewport alone (shared egui memory). Witness: leave `target_book`
    /// unset in the window (a new workbook appears); or take the drop in the
    /// root.
    #[test]
    fn a_file_dropped_on_a_detached_workbook_goes_into_it_and_leaves_the_roots_drop() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("detached.csv");
        std::fs::write(&csv, "1,2\n3,4\n").unwrap();
        let mut bench = Bench::with_detached_book();
        bench.start_book();
        let book = bench.book();
        let (sheets, books) = (
            bench.app.state.worksheets_of(book).len(),
            bench.app.state.books().len(),
        );
        let root_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800., 600.),
            )),
            dropped_files: vec![Arc::new(FakeFile("root.csv".into()))],
            ..Default::default()
        };
        bench
            .ctx
            .run_ui(root_input.clone(), |ui| begin_file_drop(ui.ctx()))
            .textures_delta
            .clear();
        let id = bench.book_id();
        bench.child_in(id, Pass::child().dropped(vec![Arc::new(FakeFile(csv))]));
        assert!(
            bench.app.state.import_progress().is_some(),
            "the drop is applied by the window's own pass"
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while bench.app.state.import_progress().is_some() {
            bench.app.state.poll_import();
            assert!(std::time::Instant::now() < deadline, "import too long");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(bench.app.state.worksheets_of(book).len(), sheets + 1);
        assert_eq!(bench.app.state.books().len(), books, "no new workbook");
        let mut root_action = None;
        let mut again = root_input;
        again.dropped_files.clear();
        bench
            .ctx
            .run_ui(again, |ui| root_action = finish_file_drop(ui.ctx()))
            .textures_delta
            .clear();
        assert_eq!(
            root_action,
            Some(Action::DropFiles {
                paths: vec!["root.csv".into()],
                target_book: None
            })
        );
        bench.finish();
    }

    /// A file dropped on a detached graph is applied in the window's own pass
    /// (the import starts there, with no destination workbook) and does not
    /// consume the pending drop of the root viewport (shared egui memory).
    #[test]
    fn a_file_dropped_on_a_detached_graph_leaves_the_pending_drop_of_the_root() {
        let mut bench = Bench::new();
        bench.start();
        let root_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800., 600.),
            )),
            dropped_files: vec![Arc::new(FakeFile("root.csv".into()))],
            ..Default::default()
        };
        // The root takes its drop at the start of its pass, alone.
        bench
            .ctx
            .run_ui(root_input.clone(), |ui| begin_file_drop(ui.ctx()))
            .textures_delta
            .clear();
        assert!(bench.app.state.import_progress().is_none());
        let id = bench.child_id();
        bench.child_in(
            id,
            Pass::child().dropped(vec![Arc::new(FakeFile("detached.csv".into()))]),
        );
        assert!(
            bench.app.state.import_progress().is_some(),
            "the drop is applied by the window's own pass"
        );
        let mut root_action = None;
        let mut again = root_input;
        again.dropped_files.clear();
        bench
            .ctx
            .run_ui(again, |ui| root_action = finish_file_drop(ui.ctx()))
            .textures_delta
            .clear();
        assert_eq!(
            root_action,
            Some(Action::DropFiles {
                paths: vec!["root.csv".into()],
                target_book: None
            })
        );
        bench.finish();
    }

    /// A file hovering a detached workbook: its viewport's border lights up in
    /// the drop colour, replacing the active outline, and nothing else is
    /// outlined (#386).
    #[test]
    fn a_file_over_a_detached_workbook_lights_up_the_viewport_border() {
        let mut bench = Bench::with_detached_book();
        bench.start_book();
        let id = bench.book_id();
        let size = egui::vec2(700., 500.);
        let content_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let selection = egui::Style::default().visuals.selection.stroke.color;
        let drop_color = crate::theme::drop_target_stroke().color;
        let mut strokes = Vec::new();
        for _ in 0..3 {
            let output = bench.child_in(
                id,
                Pass::child()
                    .size(size.x, size.y)
                    .hovering(1)
                    .events(vec![egui::Event::PointerMoved(egui::pos2(350., 300.))]),
            );
            strokes = output
                .shapes
                .iter()
                .filter_map(|p| match &p.shape {
                    egui::Shape::Rect(r) if r.stroke.width > 0.0 => Some((r.stroke.color, r.rect)),
                    _ => None,
                })
                .collect();
        }
        let drops: Vec<_> = strokes.iter().filter(|(c, _)| *c == drop_color).collect();
        assert_eq!(drops.len(), 1, "one drop stroke: {strokes:?}");
        assert_eq!(drops[0].1, content_rect);
        assert!(
            !strokes.iter().any(|(c, _)| *c == selection),
            "no active outline under it: {strokes:?}"
        );
        bench.finish();
    }

    #[test]
    fn a_detached_graph_rebuilds_its_shapes_in_its_viewport() {
        // The floating view is painted first: the detaching must transfer an
        // already filled `GraphView`, then rebuild it in the coordinates of the
        // distinct OS viewport.
        let mut bench = Bench::floating_graph();
        let graph = bench.graph;
        for _ in 0..4 {
            bench.root(Pass::root().size(1100.0, 700.0));
        }
        assert!(
            !bench.app.graph_views[&graph].cache().shapes().is_empty(),
            "the starting floating view must have built its figure"
        );
        bench.app.detach_tab(Tab::Graph(graph));
        bench.root(Pass::root().size(1100.0, 700.0));
        let viewport_id = bench.child_id();
        let output = bench.child(Pass::child().size(GRAPH_WINDOW_SIZE.x, GRAPH_WINDOW_SIZE.y));
        assert_eq!(
            delay(&output, viewport_id),
            Duration::ZERO,
            "the first callback must schedule the viewport's stabilization frame"
        );
        let window = &bench.app.detached_graphs[&graph];
        assert!(
            !window.needs_initial_repaint,
            "the first callback must ask for a single stabilization frame"
        );
        let view = &window.view;
        assert!(
            !view.cache().shapes().is_empty(),
            "the detached viewport must not serve the emptied cache of the root viewport"
        );
        let display = view.display_rect().unwrap();
        let transform = egui::emath::TSTransform::new(
            display.min.to_vec2(),
            display.width() / DEFAULT_FIGURE_SIZE.0 as f32,
        );
        assert!(
            output
                .shapes
                .iter()
                .any(|painted| view.cache().shapes().iter().any(|shape| {
                    let mut expected = shape.clone();
                    expected.transform(transform);
                    painted.shape == expected
                })),
            "the detached viewport must really receive the graph's shapes"
        );
        // Regression: the reattach "↩" button, unbounded in
        // height, could consume the whole detached window (see the
        // top doc of the line that allocates it) — the figure then ended up
        // outside the visible rectangle, invisible though its cache
        // was filled (which the two assertions above do not detect:
        // a `Shape` stays in the list even outside the clip).
        let figure_rect = display;
        assert!(
            figure_rect.max.y <= GRAPH_WINDOW_SIZE.y,
            "the figure must stay inside the detached window, not {figure_rect:?} for a window of {GRAPH_WINDOW_SIZE:?}"
        );
        bench.finish();
    }

    // B8 (`retours-2026-09-23-bugs.md`): same mechanism as
    // B4 (`le_contenu_qui_deborde_est_rogne_au_rectangle_de_la_fenetre`),
    // never carried over to `show_detached_tabs` — a detached workbook with several
    // columns overflows the OS viewport without controlled clipping, truncated to the pixel
    // by the OS window itself rather than by the application code.
    // Scaffolding modeled on `a_detached_graph_rebuilds_its_shapes_in_its_
    // viewport`: really detach, paint the viewport callback
    // published by `show_detached_tabs` in a `screen_rect` deliberately
    // smaller than the content, then check the `clip_rect` actually
    // sent to egui for each painted column header.
    #[test]
    fn the_detached_workbook_is_clipped_to_the_viewport_rectangle() {
        let mut project = Project::new("Overflow").unwrap();
        let root = project.root();
        let book = project.add_empty_workbook(root, None).unwrap();
        let sheet = project.add_worksheet(book, None).unwrap();
        for i in 0..10 {
            project
                .add_column(
                    sheet,
                    None,
                    Some(&format!("Column {i}")),
                    Column::from_f64(&[0.0, 1.0]),
                )
                .unwrap();
        }
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book_tab = Tab::Book(book);
        assert!(
            app.floating_tabs.contains(&book_tab),
            "StokesForgeApp::new must open the first workbook in a floating window"
        );
        app.detach_tab(book_tab);
        let mut bench = Bench::from_app(session, app, book);
        let viewport_id = egui::ViewportId::from_hash_of(book_tab);
        bench.root(Pass::root());

        // OS window deliberately narrower than the ten named
        // columns and their headers — like the native shrinking in the
        // reproduction of the bug report.
        let window_size = egui::vec2(200.0, 150.0);
        let window_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, window_size);
        let mut output = bench.child_in(viewport_id, Pass::child().size(200.0, 150.0));
        for _ in 0..2 {
            output = bench.child_in(viewport_id, Pass::child().size(200.0, 150.0));
        }

        // The part on which egui offers no guarantee of its own is
        // the *logical* clipping rectangle: the backend clamps the
        // buffer to the OS viewport anyway, so even without the fix, no shape
        // has a `clip_rect` that overflows `window_rect`. The regression that B8
        // fixes is subtler but just as real: without
        // `ui.set_clip_rect(...)` set on the `CentralPanel`'s `Ui`, all the
        // content (column headers, grid, tabs) inherits the panel's
        // *default* clipping — the full rectangle of the viewport, `Frame`
        // margin included — instead of the content rectangle really
        // available once that margin is removed. A header right at the
        // right edge then ends up clipped exactly at the viewport's edge rather than at the
        // panel's edge, which makes it encroach on the `Frame`'s border/margin —
        // the same symptom family as the bug report's screenshot
        // (truncation that ignores the content's geometry). This test
        // thus locks in: the logical clipping of any content must be
        // strictly smaller than `window_rect`, not equal to it.
        //
        // What it does not prove (reread by mutation, #123): the workbook
        // today sets its own clipping (the grid's scrollable area, the
        // tab strip), so that removing the one from
        // `show_window_contents_bounded` changes nothing here. The common bound
        // has its own witness, with no content that clips itself:
        // `the_bounded_content_is_clipped_under_the_header_and_does_not_push_the_frame`
        // (`window_chrome.rs`).
        let letter_label = format!("A({})", role_label(ColumnRole::Y));
        let content_labels = [
            "#".to_string(),
            tr!("column-long-name"),
            tr!("column-units"),
            letter_label.clone(),
            "Sheet1".to_string(),
        ];
        let mut checked = 0;
        for painted in &output.shapes {
            let egui::Shape::Text(shape) = &painted.shape else {
                continue;
            };
            if !content_labels.contains(&shape.galley.job.text) {
                continue;
            }
            checked += 1;
            assert!(
                window_rect.contains_rect(painted.clip_rect),
                "\"{}\" has a clip rectangle ({:?}) that overflows the viewport \
                 ({window_rect:?}) — B8 is not fixed",
                shape.galley.job.text,
                painted.clip_rect,
            );
            assert!(
                painted.clip_rect.width() < window_rect.width()
                    && painted.clip_rect.height() < window_rect.height(),
                "\"{}\" has a clip rectangle ({:?}) equal to the full viewport \
                 ({window_rect:?}) — nothing distinguishes this clipping from its absence: \
                 B8 is not fixed",
                shape.galley.job.text,
                painted.clip_rect,
            );
        }
        assert_eq!(
            checked,
            content_labels.len(),
            "the detached workbook must have painted all the expected labels \
             (stacked header and sheet tab): {:?}",
            output
                .shapes
                .iter()
                .filter_map(|painted| match &painted.shape {
                    egui::Shape::Text(shape) => Some(shape.galley.job.text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        );
        let position_of = |label: &str| {
            output
                .shapes
                .iter()
                .find_map(|painted| match &painted.shape {
                    egui::Shape::Text(shape) if shape.galley.job.text == label => {
                        let center = shape.pos + shape.galley.rect.center().to_vec2();
                        painted.clip_rect.contains(center).then_some(center)
                    }
                    _ => None,
                })
        };
        let tab_y = position_of("Sheet1")
            .expect("the detached tab must be visible")
            .y;
        let header_y = position_of(&letter_label)
            .expect("the detached header must be visible")
            .y;
        assert!(
            tab_y > header_y,
            "the tab must stay under the detached grid"
        );
        bench.finish();
    }

    #[test]
    fn a_collapsed_detached_window_shrinks_then_restores_its_os_viewport() {
        let mut bench = Bench::floating_graph();
        let graph = bench.graph;
        for _ in 0..4 {
            bench.root(Pass::root().size(1100.0, 700.0));
        }
        bench.app.detach_tab(Tab::Graph(graph));
        bench.root(Pass::root().size(1100.0, 700.0));
        let viewport_id = bench.child_id();

        // Simulated "OS" rectangle: the test controls it, as the real
        // backend would by reporting, frame after frame, the size
        // actually applied by the native window.
        let outer_rect = std::cell::Cell::new(egui::Rect::from_min_size(
            egui::pos2(100.0, 80.0),
            GRAPH_WINDOW_SIZE,
        ));

        let paint = |bench: &mut Bench, events: Vec<egui::Event>| -> egui::FullOutput {
            let info = egui::ViewportInfo {
                outer_rect: Some(outer_rect.get()),
                ..Default::default()
            };
            bench.child_in(
                viewport_id,
                Pass::child()
                    .size(outer_rect.get().width(), outer_rect.get().height())
                    .info(info)
                    .events(events),
            )
        };

        // Stabilization frame (Wayland, see `needs_initial_repaint`):
        // consumes the initial repaint and primes `expanded_size` with the
        // starting size.
        paint(&mut bench, Vec::new());
        // Second frame: the icons' SVG textures are ready.
        let first = paint(&mut bench, Vec::new());
        let icon_pos = *find_icon_centers(&first.shapes)
            .first()
            .expect("the collapse arrow must be painted, at the top left");

        // A click (null drag, press then release) on the collapse
        // chevron, spread over two frames as egui's click detection
        // requires — see `click_at`.
        paint(&mut bench, vec![egui::Event::PointerMoved(icon_pos)]);
        paint(
            &mut bench,
            vec![egui::Event::PointerButton {
                pos: icon_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let collapsed_output = paint(
            &mut bench,
            vec![egui::Event::PointerButton {
                pos: icon_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );

        let expected_collapsed_height = WINDOW_HEADER_HEIGHT + WINDOW_CONTENT_MARGIN.sum().y;
        let commands = &collapsed_output.viewport_output[&viewport_id].commands;
        assert!(
            commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::InnerSize(size)
                    if (size.x - GRAPH_WINDOW_SIZE.x).abs() < f32::EPSILON
                        && (size.y - expected_collapsed_height).abs() < f32::EPSILON
            )),
            "the collapse must reduce the OS viewport to the height of its header band, \
             commands={commands:?}"
        );
        assert!(
            bench.app.detached_graphs[&graph].collapsed,
            "the collapse must be applied to the window's state"
        );

        // The OS applies the command: the next frame reports the small
        // rectangle. Nothing of it is persisted since issue #16: the
        // detached OS window has no remembered geometry (only the
        // floating geometry is, see `sync_window_views`).
        outer_rect.set(egui::Rect::from_min_size(
            outer_rect.get().min,
            egui::vec2(GRAPH_WINDOW_SIZE.x, expected_collapsed_height),
        ));
        paint(&mut bench, Vec::new());

        // Expand: same chevron (different glyph, same rectangle), same
        // click scheme — must recover exactly the size from before
        // the collapse, not a default size.
        paint(&mut bench, vec![egui::Event::PointerMoved(icon_pos)]);
        paint(
            &mut bench,
            vec![egui::Event::PointerButton {
                pos: icon_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let expanded_output = paint(
            &mut bench,
            vec![egui::Event::PointerButton {
                pos: icon_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let commands = &expanded_output.viewport_output[&viewport_id].commands;
        assert!(
            commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::InnerSize(size)
                    if (size.x - GRAPH_WINDOW_SIZE.x).abs() < f32::EPSILON
                        && (size.y - GRAPH_WINDOW_SIZE.y).abs() < f32::EPSILON
            )),
            "the expand must restore exactly the size from before the collapse, commands={commands:?}"
        );
        assert!(
            !bench.app.detached_graphs[&graph].collapsed,
            "the expand must be applied to the window's state"
        );
        bench.finish();
    }

    /// #394: a detached graph window follows its figure through the viewport's
    /// inner size, at the display scale it had, clamped to the monitor; a
    /// maximized OS window is left alone.
    #[test]
    fn a_detached_graph_window_follows_its_figure() {
        let mut bench = Bench::new();
        let graph = bench.graph;
        bench.start();
        let viewport_id = bench.child_id();
        let inner = std::cell::Cell::new(egui::Rect::from_min_size(
            egui::pos2(100.0, 80.0),
            GRAPH_WINDOW_SIZE,
        ));
        let monitor = std::cell::Cell::new(Some(egui::vec2(3000.0, 2000.0)));
        let maximized = std::cell::Cell::new(false);
        let paint = |bench: &mut Bench| -> Option<egui::Vec2> {
            let info = egui::ViewportInfo {
                outer_rect: Some(inner.get()),
                inner_rect: Some(inner.get()),
                monitor_size: monitor.get(),
                maximized: Some(maximized.get()),
                ..Default::default()
            };
            let output = bench.child_in(
                viewport_id,
                Pass::child()
                    .size(inner.get().width(), inner.get().height())
                    .info(info),
            );
            output.viewport_output[&viewport_id]
                .commands
                .iter()
                .find_map(|command| match command {
                    egui::ViewportCommand::InnerSize(size) => Some(*size),
                    _ => None,
                })
        };
        let figure =
            |bench: &Bench| super::super::graph_window::published_figure_size(&bench.ctx, graph);
        let scale = |bench: &Bench| {
            bench.app.detached_graphs[&graph]
                .view
                .display_scale()
                .expect("painted")
        };
        for _ in 0..3 {
            assert_eq!(paint(&mut bench), None, "the first frames resize nothing");
        }
        let (before, scale_before) = (figure(&bench).expect("published"), scale(&bench));

        let add_cell = |bench: &mut Bench, side| {
            bench.app.state.dispatch(Action::AddCell {
                graph,
                panel: 0,
                side,
                columns: Vec::new(),
            })
        };
        // A cell on the left (#503) grows the figure like one on the right. The
        // window reads the shared state: no publication in between.
        add_cell(&mut bench, stokesforge_core::compose::CellSide::Left);
        let command = paint(&mut bench).expect("the viewport follows the figure");
        let grown = figure(&bench).expect("published").0 - before.0;
        assert!(grown > 0);
        assert!(
            (command.x - (inner.get().width() + grown as f32 * scale_before)).abs() < 0.01,
            "{command:?} for +{grown} at scale {scale_before}"
        );
        assert_eq!(command.y, inner.get().height(), "same height");
        assert_eq!(paint(&mut bench), None, "once");

        // The OS applied it; a new change is clamped to the monitor.
        inner.set(egui::Rect::from_min_size(inner.get().min, command));
        for _ in 0..2 {
            paint(&mut bench);
        }
        monitor.set(Some(command + egui::vec2(10.0, 0.0)));
        add_cell(&mut bench, stokesforge_core::compose::CellSide::Right);
        let clamped = paint(&mut bench).expect("clamped growth");
        assert_eq!(clamped.x, command.x + 10.0, "stops at the monitor");

        // Maximized by the OS: nothing is sent.
        inner.set(egui::Rect::from_min_size(inner.get().min, clamped));
        for _ in 0..2 {
            paint(&mut bench);
        }
        maximized.set(true);
        bench.app.state.dispatch(Action::Undo);
        assert_eq!(
            paint(&mut bench),
            None,
            "a maximized window does not resize"
        );
        bench.finish();
    }

    /// #398: a detached free graph follows the move of its image's origin: the
    /// OS window moves by the shift at the kept display scale; without the
    /// outer rectangle (native Wayland) nothing is sent. Witness: sending the
    /// shift unscaled, or sending a position when `outer_rect` is unknown.
    #[test]
    fn a_detached_free_graph_window_follows_the_origin_of_its_figure() {
        for known in [true, false] {
            let mut bench = Bench::new();
            let graph = bench.graph;
            bench
                .app
                .state
                .dispatch(Action::SetLayerPlacement { graph, free: true });
            bench.start();
            let viewport_id = bench.child_id();
            let outer = egui::Rect::from_min_size(egui::pos2(400.0, 300.0), GRAPH_WINDOW_SIZE);
            let paint = |bench: &mut Bench| -> Option<egui::Pos2> {
                let info = egui::ViewportInfo {
                    outer_rect: known.then_some(outer),
                    inner_rect: Some(outer),
                    monitor_size: Some(egui::vec2(3000.0, 2000.0)),
                    maximized: Some(false),
                    ..Default::default()
                };
                let output = bench.child_in(
                    viewport_id,
                    Pass::child().size(outer.width(), outer.height()).info(info),
                );
                output.viewport_output[&viewport_id]
                    .commands
                    .iter()
                    .find_map(|command| match command {
                        egui::ViewportCommand::OuterPosition(pos) => Some(*pos),
                        _ => None,
                    })
            };
            for _ in 0..3 {
                assert_eq!(paint(&mut bench), None, "the first frames move nothing");
            }
            let origin = |bench: &Bench| {
                super::super::graph_window::published_figure_origin(&bench.ctx, graph)
                    .expect("free origin")
            };
            let before = origin(&bench);
            let scale = bench.app.detached_graphs[&graph]
                .view
                .display_scale()
                .expect("painted");
            let snapshot = bench.app.state.tree().clone();
            let spec = stokesforge_core::compose::current_plot_spec(&snapshot, graph).unwrap();
            let mut frame = spec.panels[0].sanitized_free_frame();
            frame.left -= 30.0;
            bench.app.state.dispatch(Action::SetFreeLayout {
                graph,
                frames: vec![(0, frame)],
                page: None,
            });
            let command = paint(&mut bench);
            let shift = (origin(&bench).0 - before.0) as f32;
            assert!(shift < -1.0, "the origin moved left: {shift}");
            if known {
                let command = command.expect("the window follows the origin");
                assert!(
                    (command.x - (outer.min.x + shift * scale)).abs() < 0.01,
                    "{command:?} for {shift} at scale {scale}"
                );
                assert_eq!(command.y, outer.min.y, "no vertical move");
            } else {
                assert_eq!(command, None, "no outer rectangle: nothing to send");
            }
            bench.finish();
        }
    }

    // B1/B3 continued, then B7 (`retours-2026-09-23-bugs.md`):
    // the detached windows (separate OS viewports) had no highlight
    // outline — `active_tab`/`floating_tabs` do not cover them,
    // only the viewport's OS focus makes sense for them. Same logic as
    // the floating windows (`active_window_stroke`), wired to
    // `RawInput::focused` rather than to `active_tab`. B7 changed *where* this
    // active outline is painted: no longer on the `Frame`'s stroke (whose
    // geometry follows the content and got clipped by
    // `CentralPanel::show_inside_dyn` on the overflowing edges), but
    // separately, on `input.content_rect()` — this test now checks the
    // color *and* the geometry of this outline.
    #[test]
    fn the_detached_window_recolors_its_outline_according_to_the_os_focus() {
        let mut bench = Bench::floating_graph();
        let graph = bench.graph;
        for _ in 0..4 {
            bench.root(Pass::root().size(1100.0, 700.0));
        }
        bench.app.detach_tab(Tab::Graph(graph));
        bench.root(Pass::root().size(1100.0, 700.0));
        let viewport_id = bench.child_id();

        // All the rectangular shapes whose stroke is not of zero
        // width — since B7, there can be two: the default stroke of the
        // central panel's `Frame`, and the active outline painted
        // separately on top (`paint_active_window_outline`) when the window has
        // the OS focus.
        fn stroke_shapes(
            shapes: &[egui::epaint::ClippedShape],
        ) -> Vec<(egui::Color32, egui::Rect)> {
            fn find(shape: &egui::Shape, out: &mut Vec<(egui::Color32, egui::Rect)>) {
                match shape {
                    egui::Shape::Rect(rect) if rect.stroke.width > 0.0 => {
                        out.push((rect.stroke.color, rect.rect));
                    }
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| find(s, out)),
                    _ => {}
                }
            }
            let mut out = Vec::new();
            shapes
                .iter()
                .for_each(|painted| find(&painted.shape, &mut out));
            out
        }

        let paint_focused = |bench: &mut Bench, focused: bool| {
            let output = bench.child_in(
                viewport_id,
                Pass::child()
                    .focused(focused)
                    .size(GRAPH_WINDOW_SIZE.x, GRAPH_WINDOW_SIZE.y),
            );
            stroke_shapes(&output.shapes)
        };

        let style = (*bench.ctx.global_style()).clone();
        let content_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, GRAPH_WINDOW_SIZE);

        // Other shapes of the window (border of the plotted graph, etc.) have
        // their own stroke, unrelated to activity — so we look for
        // the presence of the right outline rather than the exclusivity of its color.
        let inactive_strokes = paint_focused(&mut bench, false);
        assert!(
            inactive_strokes.iter().any(|(color, rect)| *color
                == style.visuals.window_stroke().color
                && *rect == content_rect),
            "without OS focus, the central panel's Frame must paint egui's default stroke \
             on {content_rect:?}: {inactive_strokes:?}"
        );
        assert!(
            !inactive_strokes
                .iter()
                .any(|(color, _)| *color == style.visuals.selection.stroke.color),
            "without OS focus, no outline must take the theme's selection color: \
             {inactive_strokes:?}"
        );

        let active_strokes = paint_focused(&mut bench, true);
        let active_outline = active_strokes
            .iter()
            .find(|(color, _)| *color == style.visuals.selection.stroke.color)
            .expect(
                "with the OS focus, the detached window must paint an outline taking the \
                 theme's selection color",
            );
        assert_eq!(
            active_outline.1, content_rect,
            "the active outline must run exactly along the OS viewport's rectangle \
             (`input.content_rect()`, {content_rect:?}), not a rectangle recomputed by egui \
             from the content — B7 is not fixed (found {:?})",
            active_outline.1,
        );
        bench.finish();
    }

    /// A detached graph whose `requests.actions` hold `CommitCell`-like writes
    /// (an undo here: any action), drained by the root.
    fn detached_graph_app() -> (Session, StokesForgeApp, egui::ViewportId) {
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        app.detach_tab(Tab::Graph(graph));
        (
            session,
            app,
            egui::ViewportId::from_hash_of(Tab::Graph(graph)),
        )
    }

    /// The detached graph produces `actions` in its pass: applied at once.
    fn act(
        app: &mut StokesForgeApp,
        ctx: &egui::Context,
        viewport: egui::ViewportId,
        actions: Vec<Action>,
    ) {
        app.apply_pass_actions(ctx, viewport, Vec::new(), actions, false);
    }

    /// #471: with a dialog open in the main window, the actions of a detached
    /// window are thrown away, its band says which dialog, and the main status
    /// bar and history get one orange line. The same refusal again restarts the
    /// band and merges into that history line as a count (#453). Witness: posting to the root, or
    /// `warn` instead of `refuse_outside_main`.
    #[test]
    fn a_refused_detached_gesture_shows_the_band_in_its_own_window() {
        let _language = stokesforge_app_state::i18n::test_language(
            stokesforge_app_state::i18n::Language::English,
        );
        let (session, mut app, viewport) = detached_graph_app();
        let ctx = egui::Context::default();
        app.state.dispatch(Action::OpenAbout);
        let history = app.state.status_history().count();
        act(&mut app, &ctx, viewport, vec![Action::Undo]);
        let now = std::time::Instant::now();
        let (text, level) = super::super::window_band::text_at(&ctx, viewport, now)
            .expect("the band of the window of the gesture");
        assert!(text.contains("About StokesForge") && text.contains("Close it first"));
        assert_eq!(level, super::super::window_band::BandLevel::Warning);
        assert_eq!(
            super::super::window_band::text_at(&ctx, egui::ViewportId::ROOT, now),
            None
        );
        assert_eq!(app.state.status_history().count(), history + 1);
        // Refused again: the band shows again; the history merges the two into
        // one entry with a count (#453).
        let recorded = app.state.recorded_count();
        super::super::window_band::clear(&ctx, viewport);
        act(&mut app, &ctx, viewport, vec![Action::Undo]);
        assert_eq!(app.state.recorded_count(), recorded + 1);
        assert_eq!(app.state.status_history().count(), history + 1);
        assert_eq!(app.state.status_history().last().unwrap().count, 2);
        assert!(super::super::window_band::text_at(&ctx, viewport, now).is_some());
        // Dialog closed: the gesture goes through, no band is posted.
        app.state.dispatch(Action::CloseAbout);
        super::super::window_band::clear(&ctx, viewport);
        act(&mut app, &ctx, viewport, vec![Action::Undo]);
        assert_eq!(
            app.state.status().expect("the line").text,
            "Nothing to undo",
            "the gesture goes through"
        );
        assert_eq!(
            super::super::window_band::text_at(&ctx, viewport, now),
            None
        );
        session.shutdown().unwrap();
    }

    /// #484: Recalculate pressed in a detached workbook, on a followed sheet
    /// whose source keeps a formula reading it, is refused and says why in that
    /// window's band (red), not in the root's. Witness: dropping
    /// `action_origins` (the band would be empty).
    #[test]
    fn a_refused_recalculate_is_told_in_the_band_of_the_detached_workbook() {
        let _language = stokesforge_app_state::i18n::test_language(
            stokesforge_app_state::i18n::Language::English,
        );
        let mut project = Project::new("P").unwrap();
        let book = project.add_workbook(project.root(), Some("Book")).unwrap();
        let source = project.tree().children(book).unwrap()[0];
        let mut ids = Vec::new();
        for (name, x) in [("A", 0.0), ("B", 10.0), ("C", 20.0)] {
            let mut column = Column::from_f64(&[1.0, 2.0]);
            column.set_meta(stokesforge_core::ColumnMeta {
                x_prime: Some(x),
                ..Default::default()
            });
            ids.push(
                project
                    .add_column(source, None, Some(name), column)
                    .unwrap(),
            );
        }
        let sheet = project.transpose_rows(source, &[0]).unwrap();
        let sheet_name = project.tree().node(sheet).unwrap().name().to_string();
        let (kept, _) = project
            .tree()
            .formula_keeping(ids[2], &format!("max('{sheet_name}'!B)"));
        project
            .set_column_meta(
                ids[2],
                stokesforge_core::ColumnMeta {
                    x_prime: Some(20.0),
                    formula: Some(kept),
                    ..Default::default()
                },
            )
            .unwrap();
        // A column added after the link makes the sheet out of date.
        let mut late = Column::from_f64(&[5.0, 6.0]);
        late.set_meta(stokesforge_core::ColumnMeta {
            x_prime: Some(30.0),
            ..Default::default()
        });
        project.add_column(source, None, Some("D"), late).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        app.detach_tab(Tab::Book(book));
        let viewport = egui::ViewportId::from_hash_of(Tab::Book(book));
        let ctx = egui::Context::default();
        let expected = app
            .state
            .transpose_info(sheet)
            .and_then(|info| info.stuck)
            .expect("the sheet is stuck on the reader");
        act(
            &mut app,
            &ctx,
            viewport,
            vec![Action::RecalculateTranspose { sheet }],
        );
        let now = std::time::Instant::now();
        let (text, level) = super::super::window_band::text_at(&ctx, viewport, now)
            .expect("the band of the detached workbook");
        assert_eq!(text, expected);
        assert_eq!(level, super::super::window_band::BandLevel::Error);
        assert_eq!(
            super::super::window_band::text_at(&ctx, egui::ViewportId::ROOT, now),
            None
        );
        session.shutdown().unwrap();
    }

    /// #471: an action of a detached window that ends in a warning says it in
    /// that window (here a title that makes a Fixed figure grow past its
    /// reserve), and in no other. A message with no gesture behind it (a job)
    /// gives no band; information never does, and neither does "Nothing to
    /// undo", plain since #453 (S-R18). Witnesses: dropping the origin ranges,
    /// or the level filter; making "Nothing to undo" orange again.
    #[test]
    fn what_an_action_says_is_told_in_its_own_window_only() {
        let _language = stokesforge_app_state::i18n::test_language(
            stokesforge_app_state::i18n::Language::English,
        );
        let (session, mut app, viewport) = detached_graph_app();
        let ctx = egui::Context::default();
        let now = std::time::Instant::now();
        let band = |viewport| super::super::window_band::text_at(&ctx, viewport, now);
        // A message with no origin: a job, the autosave.
        app.dispatch_actions(&ctx, vec![Action::Undo]);
        assert_eq!(
            band(viewport),
            None,
            "no gesture behind it: main window only"
        );
        // Information from a gesture of the window: status bar only.
        let before = app.state.recorded_count();
        app.state.notice("fine".into());
        app.tell_window(&ctx, viewport, before);
        assert_eq!(band(viewport), None);
        // Ctrl+Z with nothing to undo, from the window: plain, no band (#453).
        assert!(app.state.undo_label().is_none());
        act(&mut app, &ctx, viewport, vec![Action::Undo]);
        assert_eq!(band(viewport), None, "Nothing to undo is plain");
        let status = app.state.status().expect("the line");
        assert_eq!(status.text, "Nothing to undo");
        assert_eq!(
            status.level,
            stokesforge_app_state::state::StatusLevel::Info
        );
        // A real warning of an action drained from the window: a title makes
        // a Fixed figure grow past its reserve.
        let graph = *app.detached_graphs.keys().next().expect("a detached graph");
        let mut spec = app.state.plot_spec(graph).expect("the spec");
        spec.title.clear();
        spec.fixed_figure = true;
        app.state.dispatch(Action::SetPlotSpec { graph, spec });
        let mut titled = app.state.plot_spec(graph).expect("the spec");
        titled.title = "A title".to_owned();
        act(
            &mut app,
            &ctx,
            viewport,
            vec![Action::SetPlotSpec {
                graph,
                spec: titled,
            }],
        );
        let (text, level) = band(viewport).expect("the band of the window");
        assert!(text.contains("Fixed"), "{text}");
        assert_eq!(level, super::super::window_band::BandLevel::Warning);
        assert_eq!(band(egui::ViewportId::ROOT), None);
        session.shutdown().unwrap();
    }

    /// #471: windows that do nothing get no band by surprise while a dialog is
    /// open. Witness: pushing an action that no gesture caused.
    #[test]
    fn idle_detached_windows_get_no_band_while_a_dialog_is_open() {
        let mut bench = Bench::new();
        bench.start();
        bench.app.state.dispatch(Action::OpenAbout);
        let recorded = bench.app.state.recorded_count();
        for _ in 0..10 {
            bench.child(Pass::child().focused(false));
            bench.root(Pass::root());
        }
        let now = std::time::Instant::now();
        assert_eq!(
            super::super::window_band::text_at(&bench.ctx, bench.child_id(), now),
            None
        );
        assert_eq!(bench.app.state.recorded_count(), recorded);
        bench.finish();
    }
}
