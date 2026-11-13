//! Detaching into OS windows: the three fixed panels and each content window
//! can live in a dedicated viewport (`show_viewport_deferred`).
//! The closure of this callback is `'static` and `Send + Sync`: it cannot
//! capture `&mut AppState`, hence the local views (`DetachedTabViewModel`,
//! `ProjectManagerViewModel`) and the requests they send back to the root.
//!
//! Moved from `app.rs` (#101), then split by phase (#124): publishing
//! the snapshot, painting and collecting events, window commands.
//! The callback's payload stays owned and `'static`. **The detached graph is no
//! longer here** (#475, L1): its window locks the application and serves itself
//! (`detached_graph.rs`). What remains in this file, the workbook (until L2) and
//! the three panels (until L3), still works on a copy and queues its business
//! actions for the root. That stays true when such a window covers the main one
//! (#457): on native Wayland the covered root gets no frame callback, so the
//! vendored winit reports it occluded after 100 ms and eframe runs its `logic` and
//! `ui` unpainted about every 100 ms (`vendor/winit/STOKESFORGE.md`, "Frame
//! starvation"). The viewport's request wakes the root, the root applies it,
//! and the end of that same pass publishes the result back to the viewports
//! ([`StokesForgeApp::republish_detached_tabs`]), so one action waits for one
//! throttle, not two.
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
//! applies them at once, the others queue them for the root); Ctrl+Q
//! closes nothing there (`shortcuts::refuse_quit_in_detached`). No other global
//! key is read in a detached window.
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
//! [`StokesForgeApp::show_detached_project_manager`]): `egui::Context::
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
//! a plausible candidate for this symptom, **not proof**. Detail of the
//! choice, of the acknowledged uncertainty and of the synchronization scheme
//! (`ProjectManagerViewModel`, `DetachedProjectManagerRequests`) in the
//! top doc of [`StokesForgeApp::show_detached_project_manager`].
//!
//! # Generic detaching of content tabs
//!
//! [`StokesForgeApp::detach_tab`]/[`StokesForgeApp::reattach_tab`]/
//! [`StokesForgeApp::show_detached_tabs`] generalize the mechanism above to
//! [`Tab::Book`] and [`Tab::Graph`] only ([`Tab::is_detachable`], #473: a
//! note and a matrix stay in the main window) — a
//! "Detach into a window" button in the header of each floating
//! window (see § Confined floating windows), symmetric with
//! "Reattach to the main window" on the detached window side. Several
//! tabs can be detached at the same time:
//! [`StokesForgeApp::detached_tabs`] is a table per [`Tab`] (each variant
//! already carries a `NodeId`, so a `ViewportId::from_hash_of(tab)` is
//! unique by construction — no collision possible between two workbooks
//! detached simultaneously, for example).
//!
//! B8 (`retours-2026-09-23-bugs.md`): as for a shrunken floating
//! window (B4), a `Ui` whose children exceed its
//! `max_rect` still paints them. [`StokesForgeApp::show_detached_tabs`] thus puts
//! the content of the two kinds in a single bounded, clipped child `Ui`,
//! right before the `match` that routes to each of them.
//!
//! # Why no `ViewModel` per content kind as for the PM
//!
//! The PM has only one form of content (a read-only tree); the
//! two window kinds here are frankly different (virtualized editable
//! grid, interactive graph with shape cache). Rather than two disjoint `ViewModel`/`Requests`
//! to make coexist in [`StokesForgeApp::detached_tabs`],
//! [`DetachedTabViewModel`] is a single structure carrying one field per
//! content kind (`view_state` for a workbook,
//! `graph_view` for a graph) — the fields
//! unused for the kind of `Tab` it serves simply stay at their
//! default value. Fewer types, a single `Arc<Mutex<_>>` per detached
//! tab to carry in the closure, at the price of somewhat
//! wide typing; the head comment of each field says whom it belongs to.
//!
//! # The workbook shares its rendering between the two modes
//!
//! [`tab_contents`](crate::app::contents::tab_contents) (the function already shared by the floating windows and
//! the maximize area) takes `&mut AppState` — yet a detached tab cannot
//! capture `&mut AppState` in the `'static`, `Send + Sync`
//! closure required by `show_viewport_deferred`, exactly the
//! problem already solved for the Project Manager (see above): putting
//! the whole `AppState` behind a lock would move the problem to
//! solve, for one tab as for the other.
//!
//! The workaround chosen: [`DetachedTabViewModel::snapshot`], a clone of
//! [`stokesforge_session::Snapshot`] (cheap — two `Arc`s, see its doc), is
//! resynchronized from `self.state` on every frame, **before** the call to
//! `show_viewport_deferred` — same scheme as `ProjectManagerViewModel::tree`.
//! A publication whose epoch, business revision or geometries revision
//! changes wakes the target viewport (#104): publishing the snapshot
//! alone does not trigger the deferred callback. At rest, no periodic wake-up.
//! A mutation applied after painting also schedules a new
//! root frame to publish this state before the readers redraw.
//! Every pure read (cell text, visible columns, sheets of a
//! workbook, text of a note) goes through the free functions of
//! `stokesforge_app_state::state` factored over a `&Tree` for the occasion
//! (`cell_text_in`, `visible_columns_in`, `row_count_in`, `worksheets_of_in`,
//! `note_text_in`, `node_title_in`) — the exact body of the corresponding
//! `AppState` method, not a rewrite: a floating window and
//! a detached window thus literally display the same rule, never
//! two versions to keep in agreement.
//!
//! The detached workbook owns a local [`AppState`] view built by
//! [`AppState::fork_for_detached_view`]. It keeps selection, clipboard
//! and input buffers, but reads the same session snapshot as the root.
//! Thus [`book_contents`] and [`grid_contents`](crate::app::grid::grid_contents) are called identically
//! in both modes: tabs, headers, widths and gestures can no
//! longer drift apart in two implementations. Mutating actions rejoin
//! the main loop after [`AppState::adopt_detached_book_view`]; they
//! thus keep the single undoable-command path.
//!
//! [`Tab::Graph`] does not take this detour any more (#475, L1): the window of a
//! detached graph locks the whole application and paints
//! [`graph_view_contents`](super::graph_window::graph_view_contents) — the
//! function of the floating window — from the shared state
//! (`detached_graph.rs`); its [`GraphView`](stokesforge_render_egui::GraphView)
//! migrates to [`StokesForgeApp::detached_graphs`] for the duration of the
//! detaching ([`StokesForgeApp::detach_tab`]/[`StokesForgeApp::reattach_tab`]).
//!
//! # OM and console (#100)
//!
//! The detached panels have their own read and their requests, like
//! the PM, but do not become content tabs. The OM also publishes
//! the active graph and its selection; the console publishes input/output and the
//! state of the job. The Python engine stays at the root. Input requests
//! are drained before publication so as not to lose the last keystrokes.
//! All use `detached_panel_contents`: home-made frame without native
//! decoration, Reattach only, OS close converted into a reattach.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use eframe::egui;
use stokesforge_core::NodeId;
use stokesforge_session::{Snapshot, WindowGeometry};

use stokesforge_app_state::panel_layout::{DetachAt, PanelKind};
use stokesforge_app_state::state::{Action, AppState, ProjectTreeNode};
use stokesforge_app_state::tr;

use super::detached_graph::DetachedGraph;
use super::file_drop::{begin_file_drop, finish_file_drop};
use super::object_manager_ui::{object_manager_contents, ObjectManagerRequest};
use super::project_manager::{
    project_manager_contents_from_tree, PmEntry, PmInteraction, PROJECT_MANAGER_WIDTH,
};
use super::script_console::{script_model_contents, ScriptConsoleModel};
use super::tab::{tab_node, Tab};
use super::tab::{tab_title, tab_title_from_tree};
use super::window_chrome::{
    detached_resize_handles, paint_active_window_outline, panel_header,
    show_window_contents_bounded, window_header, HeaderPlacement, PanelPlacement,
    WindowHeaderActions, MIN_CONTENT_WINDOW_SIZE, WINDOW_CONTENT_MARGIN, WINDOW_HEADER_HEIGHT,
};
use super::window_placement::floating_window_display_size;
use super::window_stack::toggle_collapse_to;
use super::workbook::book_contents;
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

/// What the detached OS window of the Project Manager needs to paint —
/// strictly that, and nothing more. See the top doc of
/// [`StokesForgeApp::show_detached_project_manager`] for why this detour
/// rather than putting the whole `AppState` behind a lock.
#[derive(Debug, Default, Clone)]
pub(super) struct ProjectManagerViewModel {
    /// Last published snapshot; a changed publication wakes the viewport.
    pub(super) snapshot_stamp: Option<(u64, u64, u64)>,
    /// Copy of [`AppState::project_tree`], resynchronized on every frame
    /// by [`StokesForgeApp::show_detached_project_manager`] before the call to
    /// `show_viewport_deferred` — `AppState` stays the only source of
    /// truth, this is only a snapshot for the deferred callback.
    pub(super) tree: Vec<ProjectTreeNode>,
    pub(super) root: Option<NodeId>,
    /// Copy of [`StokesForgeApp::pm_selection`], same resynchronization as
    /// `tree` — needed so that the detached Project Manager window
    /// also shows the current selection and the closed windows dimmed
    /// up to date (§6).
    pub(super) selected: HashSet<PmEntry>,
    /// Copy of the open tabs ([`StokesForgeApp::open_tabs`]), same
    /// resynchronization: the other rows are dimmed.
    pub(super) open: HashSet<Tab>,
    /// Copy of the foreground tab, same resynchronization.
    pub(super) active_tab: Option<Tab>,
    /// Copy of the nodes of the last Cut ([`AppState::cut_objects`]), same
    /// resynchronization: their rows are dimmed (#423).
    pub(super) cut: HashSet<NodeId>,
    /// Copy of the folders in manual order (#425), same resynchronization.
    pub(super) manual: std::collections::BTreeSet<NodeId>,
}

/// What the detached OS window of the Project Manager writes, so that the
/// main loop (the only one to carry `&mut AppState` and `&mut
/// StokesForgeApp::floating_tabs`) applies them after rendering — same scheme as
/// the `open_requests`/
/// `actions` local to [`StokesForgeApp::ui`](eframe::App::ui), just set by a deferred callback
/// (potentially called again on a frame where the parent window does not paint)
/// rather than collected live during painting.
#[derive(Debug, Default)]
pub(super) struct DetachedProjectManagerRequests {
    /// File drops, handled by the same session as the main window.
    pub(super) actions: Vec<Action>,
    /// Tabs requested by a click in the tree since the last
    /// drain; [`StokesForgeApp::ui`](eframe::App::ui) transfers them to its own
    /// `open_requests` then empties them.
    pub(super) open: Vec<Tab>,
    /// True if the "Reattach" button or the closing of the OS window was
    /// actuated since the last drain.
    pub(super) reattach: bool,
    /// Selection/hiding as they were at the end of the last frame
    /// painted by the detached window — `None` as long as no frame has
    /// run since the last drain. Replaced as a whole rather
    /// than as a delta, like `DetachedTabRequests::geometry` (§6).
    pub(super) selection: Option<HashSet<PmEntry>>,
    /// Tabs that "Close" asked to close since the last drain.
    pub(super) close: Vec<Tab>,
}

/// The detached OM receives the projection of the active graph, even with no event
/// in its viewport. The gestures stay tied to the graph that was painted.
#[derive(Clone)]
pub(super) struct ObjectManagerModel {
    pub(super) stamp: (u64, u64, u64),
    pub(super) tree: Arc<stokesforge_core::Tree>,
    pub(super) graph: Option<NodeId>,
    pub(super) selection: Option<stokesforge_app_state::object_manager::VisibilityTarget>,
    pub(super) model: Option<stokesforge_app_state::object_manager::ObjectTree>,
    pub(super) spec: Option<stokesforge_plot::PlotSpec>,
    pub(super) settings: stokesforge_app_state::preferences::ObjectManagerSettings,
}

#[derive(Default)]
struct ObjectManagerRequests {
    gestures: Vec<(NodeId, ObjectManagerRequest)>,
    actions: Vec<Action>,
    reattach: bool,
}

pub(super) struct DetachedObjectManager {
    model: Arc<Mutex<ObjectManagerModel>>,
    requests: Arc<Mutex<ObjectManagerRequests>>,
}

#[cfg(test)]
impl DetachedObjectManager {
    /// The actions the window queued and the root has not drained (tests).
    pub(super) fn queued_actions(&self) -> Vec<Action> {
        self.requests.lock().unwrap().actions.clone()
    }
}

#[derive(Default)]
struct ScriptConsoleRequests {
    source: Option<String>,
    actions: Vec<Action>,
    reattach: bool,
}

pub(super) struct DetachedScriptConsole {
    model: Arc<Mutex<ScriptConsoleModel>>,
    requests: Arc<Mutex<ScriptConsoleRequests>>,
}

#[cfg(test)]
impl DetachedScriptConsole {
    /// The actions the window queued and the root has not drained (tests).
    pub(super) fn queued_actions(&self) -> Vec<Action> {
        self.requests.lock().unwrap().actions.clone()
    }
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

/// A content tab detached into a separate OS window — see the top doc
/// of the module, § generic detaching. Same pattern as the Project Manager
/// (`pm_view_model`/`pm_requests`), repeated once per tab rather than once
/// for the whole app: [`StokesForgeApp::detached_tabs`] carries one per
/// entry.
pub(super) struct DetachedTab {
    /// What the detached window paints, resynchronized from `self.state`
    /// (and `self.graph_views` for a graph) on every frame by
    /// [`StokesForgeApp::show_detached_tabs`].
    pub(super) view_model: Arc<Mutex<DetachedTabViewModel>>,
    /// What the detached window writes, drained every frame by
    /// [`StokesForgeApp::ui`](eframe::App::ui).
    pub(super) requests: Arc<Mutex<DetachedTabRequests>>,
}

/// What a detached window needs to paint. A single structure for
/// the two detachable kinds rather than two disjoint `ViewModel`s —
/// see the top doc of the module for the trade-off. Each field, except
/// `snapshot`, makes sense only for one particular kind of [`Tab`]; the
/// comment says which.
pub(super) struct DetachedTabViewModel {
    /// Snapshot of the model, resynchronized on every frame — see the top doc of the
    /// module, § "the model read is factored out". `None`
    /// only before the very first synchronization (never observable
    /// from outside [`StokesForgeApp::detach_tab`], which sets it at once).
    pub(super) snapshot: Option<Snapshot>,
    /// `Tab::Book`: [`AppState`] view local to the window, which
    /// renders the content shared with the floating window (same
    /// `book_contents`) while keeping its selection and its
    /// input in progress.
    pub(super) view_state: Option<AppState>,
    /// `Tab::Graph`/`Tab::Book`: collapse of the content, specific to this detached
    /// window — initialized from [`StokesForgeApp::collapsed_tabs`] by
    /// `detach_tab`, toggled directly here (no detour through
    /// `DetachedTabRequests`, this collapse concerns only this window), and
    /// poured back into `collapsed_tabs` by `reattach_tab`.
    pub(super) collapsed: bool,
    /// The first callback of a deferred viewport can arrive before
    /// Wayland/WGPU has stabilized its size. One extra frame
    /// then gives the figure its real rectangle, without sustaining a render
    /// loop when the window stays inactive.
    pub(super) needs_initial_repaint: bool,
    /// Last known OS size of the window **outside collapse** (B9,
    /// `retours-2026-09-23-bugs.md`): updated on every
    /// frame as long as `collapsed` is false, frozen during a collapse. Serves
    /// both as the target for the `ViewportCommand::InnerSize` sent on
    /// expanding and as the size to persist in `window_geometries` during a
    /// collapse — same role as `current_size`/`window_sizes` on the floating
    /// window side (B3), but carried by the model rather than by an app table
    /// since a detached window has no equivalent. `None`
    /// only before the first frame where the viewport reports a
    /// size (or if the window was detached already collapsed and never
    /// expanded since).
    pub(super) expanded_size: Option<egui::Vec2>,
    pub(super) focused: bool,
}

/// What a detached window writes, so that [`StokesForgeApp::ui`](eframe::App::ui) (the only one to
/// carry `&mut AppState`) applies them after rendering — same role as
/// [`DetachedProjectManagerRequests`].
#[derive(Debug, Default)]
pub(super) struct DetachedTabRequests {
    /// Actions produced by this window since the last drain
    /// (cell/note validation, grid structure…) — the same undoable-command
    /// path as a floating window, see the top doc of the
    /// module.
    pub(super) actions: Vec<Action>,
    /// True if "Reattach to the main window" was actuated since
    /// the last drain.
    pub(super) reattach: bool,
    /// The system's cross closes the detached view without reattaching it.
    pub(super) close: bool,
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

    pub(super) fn drain_object_manager(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let Some(view) = &self.om_view else {
            return;
        };
        let (incoming, gestures, reattach) = {
            let mut requests = view.requests.lock().unwrap_or_else(|err| err.into_inner());
            (
                std::mem::take(&mut requests.actions),
                std::mem::take(&mut requests.gestures),
                std::mem::take(&mut requests.reattach),
            )
        };
        let viewport = object_manager_viewport();
        // While a dialog is open in the main window only a selection goes
        // through: it writes nothing. The rest would open an editor, a window
        // or an analysis, or write (#471).
        let dialog = self.state.open_dialog();
        let mut refused = None;
        let start = actions.len();
        if !incoming.is_empty() {
            match dialog {
                Some(open) => refused = Some(open),
                None => actions.extend(incoming),
            }
        }
        for (graph, request) in gestures {
            match dialog {
                Some(open) if !matches!(request, ObjectManagerRequest::Select(_)) => {
                    refused = Some(open);
                }
                _ => self.run_object_manager_request(ctx, graph, request, actions),
            }
        }
        if actions.len() > start {
            self.action_origins.push((start..actions.len(), viewport));
        }
        if let Some(open) = refused {
            self.refuse_in_window(ctx, viewport, open);
        }
        if reattach {
            self.om_detached = false;
            // The detached read must no longer hold an old whole tree.
            self.om_view = None;
        }
    }

    pub(super) fn show_detached_object_manager(
        &mut self,
        ctx: &egui::Context,
        next: ObjectManagerModel,
    ) {
        let view = self.om_view.get_or_insert_with(|| DetachedObjectManager {
            model: Arc::new(Mutex::new(next.clone())),
            requests: Arc::default(),
        });
        let changed = {
            let mut model = view.model.lock().unwrap_or_else(|err| err.into_inner());
            let changed = model.stamp != next.stamp
                || model.graph != next.graph
                || model.selection != next.selection
                || model.settings != next.settings;
            *model = next;
            changed
        };
        let model = Arc::clone(&view.model);
        let requests = Arc::clone(&view.requests);
        let root = ctx.clone();
        let id = object_manager_viewport();
        let live = self.live_window(PanelKind::Object);
        let builder = panel_viewport(
            tr!("action-object-manager"),
            [330.0, 480.0],
            [220.0, 140.0],
            self.detach_init(PanelKind::Object),
        );
        Self::publish_viewport(
            &mut self.pending_viewports,
            ctx,
            id,
            builder,
            changed,
            move |ui, _class| {
                super::window_band::stop_keys(ui.ctx());
                super::shortcuts::refuse_quit_in_detached(ui.ctx());
                // Read before painting (#459), queued after the window's own actions.
                let mut keys = super::shortcuts::read_detached_history_and_save(ui.ctx());
                live.report(ui.ctx());
                let model = model.lock().unwrap_or_else(|err| err.into_inner()).clone();
                let mut actions = Vec::new();
                let mut gestures = Vec::new();
                let reattach = detached_panel_contents(ui, &tr!("action-object-manager"), |ui| {
                    gestures = object_manager_contents(
                        ui,
                        &model.tree,
                        model.model.as_ref(),
                        model.spec.as_ref(),
                        model.settings,
                        &mut actions,
                    );
                });
                actions.append(&mut keys);
                if reattach || !actions.is_empty() || !gestures.is_empty() {
                    let mut requests = requests.lock().unwrap_or_else(|err| err.into_inner());
                    requests.reattach |= reattach;
                    requests.actions.append(&mut actions);
                    if let Some(graph) = model.graph {
                        requests
                            .gestures
                            .extend(gestures.into_iter().map(|gesture| (graph, gesture)));
                    }
                    root.request_repaint_of(egui::ViewportId::ROOT);
                }
                super::page_layout::note_field_focus(ui.ctx());
                super::window_band::paint(ui.ctx());
            },
        );
    }

    pub(super) fn detach_script_console(&mut self) {
        self.script_detached = true;
        // The Python job never migrates; this reader owns only the input and the output.
        self.script_view = Some(DetachedScriptConsole {
            model: Arc::new(Mutex::new(ScriptConsoleModel::from_state(&self.state))),
            requests: Arc::default(),
        });
    }

    pub(super) fn drain_script_console(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let Some(view) = &self.script_view else {
            return;
        };
        let (source, reattach, incoming) = {
            let mut requests = view.requests.lock().unwrap_or_else(|err| err.into_inner());
            (
                requests.source.take(),
                std::mem::take(&mut requests.reattach),
                std::mem::take(&mut requests.actions),
            )
        };
        self.accept_detached_actions(ctx, script_viewport(), incoming, actions);
        if let Some(source) = source {
            self.state.set_script_source(source);
        }
        if reattach {
            self.script_detached = false;
            self.script_view = None;
            ctx.request_repaint();
        }
    }

    pub(super) fn show_detached_script_console(&mut self, ctx: &egui::Context) {
        let Some(view) = &self.script_view else {
            return;
        };
        let next = ScriptConsoleModel::from_state(&self.state);
        let changed = {
            let mut model = view.model.lock().unwrap_or_else(|err| err.into_inner());
            let changed = *model != next;
            // An input that arrived after the drain must not be overwritten
            // by a publication of the old buffer in this same frame.
            let requests = view.requests.lock().unwrap_or_else(|err| err.into_inner());
            if requests.source.is_none() {
                model.source = next.source;
            }
            model.output = next.output;
            model.running = next.running;
            model.available = next.available;
            changed
        };
        let model = Arc::clone(&view.model);
        let requests = Arc::clone(&view.requests);
        let root = ctx.clone();
        let id = script_viewport();
        let live = self.live_window(PanelKind::Script);
        let builder = panel_viewport(
            tr!("script-title"),
            [700.0, 440.0],
            [300.0, 220.0],
            self.detach_init(PanelKind::Script),
        );
        Self::publish_viewport(
            &mut self.pending_viewports,
            ctx,
            id,
            builder,
            changed,
            move |ui, _class| {
                super::window_band::stop_keys(ui.ctx());
                super::shortcuts::refuse_quit_in_detached(ui.ctx());
                // Read before painting (#459), queued after the window's own actions.
                let mut keys = super::shortcuts::read_detached_history_and_save(ui.ctx());
                live.report(ui.ctx());
                let mut model = model.lock().unwrap_or_else(|err| err.into_inner());
                let source = model.source.clone();
                let mut actions = Vec::new();
                let ctx = ui.ctx().clone();
                if ctx.input_mut(|input| {
                    input.consume_shortcut(&super::shortcuts::SHORTCUT_RUN_SCRIPT)
                }) {
                    actions.push(Action::RunScript);
                }
                let reattach = detached_panel_contents(ui, &tr!("script-title"), |ui| {
                    if !model.available {
                        ui.weak(tr!("script-unavailable"));
                    }
                    script_model_contents(ui, &ctx, &mut model, &mut actions);
                });
                actions.append(&mut keys);
                if source != model.source || reattach || !actions.is_empty() {
                    let mut requests = requests.lock().unwrap_or_else(|err| err.into_inner());
                    if source != model.source {
                        requests.source = Some(model.source.clone());
                    }
                    requests.reattach |= reattach;
                    requests.actions.append(&mut actions);
                    root.request_repaint_of(egui::ViewportId::ROOT);
                }
                super::page_layout::note_field_focus(ui.ctx());
                super::window_band::paint(ui.ctx());
            },
        );
    }

    /// Switches to detached mode: the next render opens the Project Manager
    /// in its own OS window ([`StokesForgeApp::show_detached_project_manager`])
    /// instead of the fixed panel ([`StokesForgeApp::project_manager_panel`]). No
    /// effect if it already is.
    pub(super) fn detach_project_manager(&mut self) {
        self.pm_detached = true;
    }

    /// Switches out of detached mode: the fixed panel reappears at the next
    /// render, always on the left since it is fixed in position. No effect
    /// if it is not detached.
    pub(super) fn reattach_project_manager(&mut self) {
        self.pm_detached = false;
    }

    /// Paints the Project Manager in its own native OS window, through a
    /// **deferred** `eframe`/`egui` viewport — see the top doc of the module,
    /// § detaching.
    ///
    /// # Why `show_viewport_deferred`, not `_immediate`
    ///
    /// This module used `show_viewport_immediate` until now — the simplest
    /// choice, judged sufficient as long as this panel animates nothing
    /// continuously. The user's interactive finding (Fedora/GNOME/Wayland,
    /// Mutter compositor): the detached window was indeed draggable, but
    /// stayed **visually confined to the parent window's rectangle**,
    /// even with the latter reduced (not full screen) — not just the lack of
    /// absolute positioning specific to Wayland (already documented above
    /// for `rfd`), something else kept the window from leaving the frame.
    /// `egui::Context::show_viewport_immediate` itself documents: "This
    /// call will pause the current viewport and render the child viewport
    /// in its own window" — this synchronized rendering coupling between the two
    /// windows is a plausible candidate for interacting badly with Wayland's frame
    /// model. `egui::Context::show_viewport_deferred` is the
    /// form that the `egui` doc explicitly recommends for a real
    /// independent window ("To avoid this, use show_viewport_deferred
    /// instead"), repainted independently of the parent window.
    ///
    /// **It is a test, not a guarantee.** Nothing proves that the symptom
    /// observed on the Mutter side comes from this rendering coupling rather than from something
    /// else (a compositor behavior toward subwindows in
    /// general, for example). If the user finds afterwards that the
    /// detached window is still confined, that will have to be documented as a
    /// known limit — next to the Wayland caveat already noted for the
    /// position — rather than trying another switch without a new
    /// lead.
    ///
    /// # `Fn` callback, no `&mut self`
    ///
    /// `show_viewport_deferred` requires `impl Fn(&mut Ui, ViewportClass) +
    /// Send + Sync + 'static`: no capture of `&mut self`, not even a classic
    /// borrow of `AppState` — the callback can outlive this
    /// frame and be called again alone, on a later frame of the detached
    /// window while the parent window sleeps. Putting the whole `AppState`
    /// behind a lock to give it what it needs
    /// would go far beyond the problem to solve (dispatch, main
    /// render loop); instead, only what is strictly necessary to the panel
    /// travels:
    ///
    /// - [`ProjectManagerViewModel`] (the project tree) is resynchronized
    ///   here from `self.state` — the real source of truth — on every
    ///   frame, **before** the call to `show_viewport_deferred`;
    /// - [`DetachedProjectManagerRequests`] collects in the other direction
    ///   what the callback produces (open clicks, requested reattach);
    ///   [`StokesForgeApp::ui`](eframe::App::ui) drains it before publication to produce the
    ///   same `open_requests`/`Action` as before, without duplicating the logic
    ///   of [`StokesForgeApp::open_tab`] nor calling
    ///   [`StokesForgeApp::reattach_project_manager`] from inside a
    ///   callback that has no access to `&mut self`.
    pub(super) fn show_detached_project_manager(
        &mut self,
        ctx: &egui::Context,
        active_tab: Option<Tab>,
    ) {
        let open = self.open_tabs();
        let cut: HashSet<NodeId> = self.state.cut_objects().into_iter().collect();
        let changed = {
            let mut model = self
                .pm_view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            let stamp = snapshot_stamp(self.state.snapshot());
            let changed = model.snapshot_stamp != Some(stamp)
                || model.selected != self.pm_selection
                || model.open != open
                || model.active_tab != active_tab
                || model.cut != cut
                || &model.manual != self.state.tree().manual_order_folders();
            model.snapshot_stamp = Some(stamp);
            model.tree = self.state.project_tree();
            model.root = Some(self.state.tree().root());
            model.selected = self.pm_selection.clone();
            model.open = open;
            model.active_tab = active_tab;
            model.cut = cut;
            model.manual = self.state.tree().manual_order_folders().clone();
            changed
        };
        let view_model = Arc::clone(&self.pm_view_model);
        let requests = Arc::clone(&self.pm_requests);
        let root_context = ctx.clone();
        let live = self.live_window(PanelKind::Project);
        let builder = panel_viewport(
            tr!("window-title-project-manager"),
            [PROJECT_MANAGER_WIDTH + 40.0, 480.0],
            [220.0, 140.0],
            self.detach_init(PanelKind::Project),
        );
        Self::publish_viewport(
            &mut self.pending_viewports,
            ctx,
            egui::ViewportId::from_hash_of("stokesforge_project_manager_window"),
            builder,
            changed,
            move |ui, _class| {
                super::window_band::stop_keys(ui.ctx());
                super::shortcuts::refuse_quit_in_detached(ui.ctx());
                // Read before painting (#459), queued after the window's own actions.
                let mut keys = super::shortcuts::read_detached_history_and_save(ui.ctx());
                live.report(ui.ctx());
                begin_file_drop(ui.ctx());
                let mut opened = Vec::new();
                let mut closed = Vec::new();
                let mut actions = Vec::new();
                // Read of a copy rather than of the lock held during
                // the whole painting: `project_manager_contents_from_tree`
                // must not keep `view_model` locked for the duration
                // of a whole `ScrollArea`. The selection/hiding are
                // mutated on these copies then sent back as a whole afterwards
                // (`selection`/`close` on `DetachedProjectManagerRequests`).
                let mut model = view_model
                    .lock()
                    .unwrap_or_else(|err| err.into_inner())
                    .clone();
                let selected_before = model.selected.clone();
                let reattach = detached_panel_contents(ui, &tr!("project-manager-title"), |ui| {
                    let mut interaction = PmInteraction {
                        actions: &mut actions,
                        root: model.root.unwrap_or(NodeId::from_raw(1)),
                        open_requests: &mut opened,
                        selection: &mut model.selected,
                        open: &model.open,
                        close_requests: &mut closed,
                        cut: &model.cut,
                        manual: &model.manual,
                    };
                    project_manager_contents_from_tree(ui, &model.tree, &mut interaction);
                });
                actions.append(&mut keys);
                {
                    let mut requests = requests.lock().unwrap_or_else(|err| err.into_inner());
                    if reattach
                        || !opened.is_empty()
                        || model.selected != selected_before
                        || !closed.is_empty()
                    {
                        root_context.request_repaint_of(egui::ViewportId::ROOT);
                    }
                    requests.reattach |= reattach;
                    if !actions.is_empty() {
                        root_context.request_repaint_of(egui::ViewportId::ROOT);
                    }
                    requests.actions.append(&mut actions);
                    requests.open.append(&mut opened);
                    if model.selected != selected_before {
                        requests.selection = Some(model.selected);
                    }
                    requests.close.append(&mut closed);
                    if let Some(action) = finish_file_drop(ui.ctx()) {
                        requests.actions.push(action);
                        root_context.request_repaint_of(egui::ViewportId::ROOT);
                    }
                }
                super::page_layout::note_field_focus(ui.ctx());
                super::window_band::paint(ui.ctx());
            },
        );
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
                DetachedGraph::new(view, self.collapsed_tabs.contains(&tab)),
            );
            return;
        }
        let view_state = match tab {
            Tab::Book(_) => self.state.fork_for_detached_view().ok(),
            _ => None,
        };
        let view_model = DetachedTabViewModel {
            snapshot: Some(self.state.snapshot().clone()),
            view_state,
            needs_initial_repaint: true,
            collapsed: self.collapsed_tabs.contains(&tab),
            focused: false,
            expanded_size: None,
        };
        self.detached_tabs.insert(
            tab,
            DetachedTab {
                view_model: Arc::new(Mutex::new(view_model)),
                requests: Arc::new(Mutex::new(DetachedTabRequests::default())),
            },
        );
    }

    /// Removes `tab` from the detached windows and pushes it back as a
    /// floating window (new cascade rank). No effect if it is not
    /// detached. The [`GraphView`] of a graph returns to
    /// [`StokesForgeApp::graph_views`] — the graph's shape cache survives
    /// detaching/reattaching, as it already survives the closing of a floating
    /// window.
    pub(super) fn reattach_tab(&mut self, tab: Tab) {
        if let Tab::Graph(graph) = tab {
            let Some(window) = self.detached_graphs.remove(&graph) else {
                return;
            };
            let mut view = window.view;
            view.invalidate_surface();
            self.graph_views.insert(graph, view);
            toggle_collapse_to(&mut self.collapsed_tabs, tab, window.collapsed);
            self.figure_follow.remove(&graph);
            self.leave_layout_mode(graph);
        } else {
            let Some(detached) = self.detached_tabs.remove(&tab) else {
                return;
            };
            let model = detached
                .view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            toggle_collapse_to(&mut self.collapsed_tabs, tab, model.collapsed);
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
            let Some(window) = self.detached_graphs.remove(&graph) else {
                return;
            };
            let mut view = window.view;
            view.invalidate_surface();
            self.graph_views.insert(graph, view);
            // Closing the window ends the Page Layout Mode.
            self.leave_layout_mode(graph);
        } else {
            self.detached_tabs.remove(&tab);
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

    /// Paints each tab of [`StokesForgeApp::detached_tabs`] in its own
    /// OS window, through `show_viewport_deferred` — see the top doc of the
    /// module, § generic detaching, and [`StokesForgeApp::
    /// show_detached_project_manager`] for the justification of the
    /// `_deferred`/`Fn` choice already made for the Project Manager (identical here).
    ///
    /// Does **not** drain the requests produced: [`StokesForgeApp::ui`](eframe::App::ui) does it
    /// right after, once this function has returned, exactly as for
    /// `pm_requests`.
    /// Publishes the snapshot to the detached workbooks, outside
    /// `show_detached_tabs` (#457). Called at the end of the root pass, once the
    /// actions of the frame are applied; the wake of the windows is
    /// [`server::wake`](super::server::wake)'s.
    pub(super) fn republish_detached_tabs(&self) {
        for detached in self.detached_tabs.values() {
            detached.publish_snapshot(&self.state);
        }
    }

    /// After the actions of the pass: a selection they moved reaches the local
    /// view of each detached workbook (Enter in a cell selects the row below).
    pub(super) fn return_selection_to_detached_books(&self, ctx: &egui::Context) {
        for (tab, detached) in &self.detached_tabs {
            let Tab::Book(book) = tab else { continue };
            let mut model = detached
                .view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            if let Some(view) = model.view_state.as_mut() {
                if self.state.return_selection_to_detached_view(view, *book) {
                    ctx.request_repaint_of(egui::ViewportId::from_hash_of(tab));
                }
            }
        }
    }

    pub(super) fn show_detached_tabs(&mut self, ctx: &egui::Context) {
        let Self {
            state,
            detached_tabs,
            pending_viewports,
            ..
        } = self;
        for (tab, detached) in detached_tabs.iter() {
            let changed = detached.publish_snapshot(state);
            let view_model = Arc::clone(&detached.view_model);
            let requests = Arc::clone(&detached.requests);
            let root_context = ctx.clone();
            let tab = *tab;
            let title = tr!("window-title-detached", name = tab_title(state, tab));
            // The workbook uses a uniform size; its grid uses the same
            // widths and headers as in the floating window. (The graph has
            // its own window, `detached_graph.rs`.)
            let size = [420.0, 360.0];
            // Without native decoration: the home-made bar (`window_header`,
            // placement `Detached`) is all the user has to
            // move/resize/close this window — see the top doc of the
            // module, § generic detaching, and
            // [`detached_resize_handles`] for the resizing.
            // Neither position nor size remembered: the detached OS window has
            // no persisted geometry (issue #16), only the floating
            // geometry is.
            let viewport = crate::app_logo::viewport()
                .with_title(title)
                .with_decorations(false)
                .with_inner_size(size)
                // The B9 collapse must be able to reduce the height below the usual
                // minimum. Below this width, the sheet tabs (natural
                // size) scroll instead of shrinking — see `sheet_tabs`.
                .with_min_inner_size([MIN_CONTENT_WINDOW_SIZE.x, WINDOW_HEADER_HEIGHT]);
            let id = egui::Id::new(tab);
            let payload = DetachedTabViewport {
                tab,
                id,
                size,
                view_model,
                requests,
                root_context,
            };
            Self::publish_viewport(
                pending_viewports,
                ctx,
                egui::ViewportId::from_hash_of(tab),
                viewport,
                changed,
                move |ui, _class| payload.show(ui),
            );
        }
    }
}

/// Owned data of the deferred callback: no borrow from the application
/// survives the publication of the viewport. The phases create no sub-Ui.
struct DetachedTabViewport {
    tab: Tab,
    id: egui::Id,
    size: [f32; 2],
    view_model: Arc<Mutex<DetachedTabViewModel>>,
    requests: Arc<Mutex<DetachedTabRequests>>,
    root_context: egui::Context,
}

/// Frame state read before the clicks, then actions of the painted header.
struct DetachedTabFrame {
    was_collapsed: bool,
    is_active: bool,
    header_actions: WindowHeaderActions,
    inner_margin: egui::Margin,
}

impl DetachedTab {
    /// Publishes first the snapshot and the local view; the wake-up stays after
    /// the registration of the callback, in `show_detached_tabs`.
    fn publish_snapshot(&self, state: &AppState) -> bool {
        let snapshot = state.snapshot();
        let mut model = self
            .view_model
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let changed = model.snapshot.as_ref().map(snapshot_stamp) != Some(snapshot_stamp(snapshot))
            || model
                .view_state
                .as_ref()
                .is_some_and(|view| view.renaming_sheet() != state.renaming_sheet());
        model.snapshot = Some(snapshot.clone());
        if let Some(view_state) = model.view_state.as_mut() {
            view_state.sync_detached_snapshot(snapshot.clone());
            view_state.sync_detached_sheet_editor(state);
        }
        changed
    }
}

impl DetachedTabViewport {
    /// The window's frame, then the band.
    fn show(&self, ui: &mut egui::Ui) {
        super::window_band::stop_keys(ui.ctx());
        super::shortcuts::refuse_quit_in_detached(ui.ctx());
        self.show_locked(ui);
        super::page_layout::note_field_focus(ui.ctx());
        super::window_band::paint(ui.ctx());
    }

    /// The two locks stay held during the whole frame, as before the
    /// split: no root drain comes in between its phases.
    fn show_locked(&self, ui: &mut egui::Ui) {
        let root_context = &self.root_context;
        begin_file_drop(ui.ctx());
        let mut model = self
            .view_model
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let mut requests = self.requests.lock().unwrap_or_else(|err| err.into_inner());
        let was_collapsed = model.collapsed;
        // B9 (`retours-2026-09-23-bugs.md`): the
        // "logical" OS size (outside collapse) is remembered by
        // ourselves rather than deduced from the rectangle reported
        // this frame — during a collapse that rectangle is
        // deliberately reduced (see below), and freezing it here
        // is what allows both to recover the exact
        // size from before the collapse on expanding and not to persist the
        // collapsed size (same trap as
        // `floating_window_geometry_to_persist`, B3). Read before
        // any handling of this frame's click: `was_collapsed`
        // reflects the incoming state, not a possible toggle of
        // this very frame.
        if !was_collapsed {
            if let Some(rect) = ui.ctx().input(|input| input.viewport().outer_rect) {
                model.expanded_size = Some(rect.size());
            }
        }
        // Highlight (B1/B3 continued): a detached window is
        // a separate OS viewport, so "active" only makes sense
        // in the sense of the OS focus — `active_tab`/`floating_tabs`
        // do not cover it. `RawInput::focused` is already
        // reported by the backend per viewport.
        let is_active = ui.input(|input| input.focused);
        if model.focused != is_active {
            model.focused = is_active;
            root_context.request_repaint_of(egui::ViewportId::ROOT);
        }
        // Read before anything is painted (#459): a cell or a field never sees
        // these keys. Queued after the window's own actions, below. Workbook
        // only here; the graph reads them in its own pass.
        let mut keys = if matches!(self.tab, Tab::Book(_)) {
            super::shortcuts::read_detached_history_and_save(ui.ctx())
        } else {
            Vec::new()
        };
        let (header_actions, frame) =
            self.paint_contents(ui, &mut model, &mut requests, was_collapsed);
        self.collect_actions(ui, &mut requests, &mut keys);
        // A message the local view recorded (an unreadable clipboard...) has no
        // action behind it: only the root can bring it to the band (#471).
        if model
            .view_state
            .as_ref()
            .is_some_and(AppState::has_local_messages)
        {
            root_context.request_repaint_of(egui::ViewportId::ROOT);
        }
        self.apply_window_actions(
            ui,
            &mut model,
            &mut requests,
            DetachedTabFrame {
                was_collapsed,
                is_active,
                header_actions,
                inner_margin: frame.inner_margin,
            },
        );
        if std::mem::take(&mut model.needs_initial_repaint) {
            // `show_viewport_deferred` creates the window after
            // the root frame. Under Wayland, the first callback
            // may thus see a surface still without a final
            // size; redrawing once rebuilds the
            // figure with the rectangle configured by WGPU.
            ui.ctx().request_repaint();
        }
        if ui.ctx().input(|input| input.viewport().close_requested()) {
            requests.close = true;
            root_context.request_repaint_of(egui::ViewportId::ROOT);
        }
    }

    fn paint_contents(
        &self,
        ui: &mut egui::Ui,
        model: &mut DetachedTabViewModel,
        requests: &mut DetachedTabRequests,
        was_collapsed: bool,
    ) -> (WindowHeaderActions, egui::Frame) {
        let tab = self.tab;
        let id = self.id;
        let mut header_actions = WindowHeaderActions::default();
        // Default stroke (inactive) only — the active outline
        // is no longer recolored here (B7,
        // `retours-2026-09-23-bugs.md`): the
        // geometry of this `Frame` follows `content_ui.min_rect()`
        // (`containers/frame.rs`), and `CentralPanel::show_inside_dyn`
        // clips its display to `outer_rect` *before* painting it
        // — the logical overflow of content (tool band that
        // does not wrap) thus made the outline disappear
        // on the overflowing edges, instead of showing it.
        // The active state is painted separately, after `.show()`, on
        // `input.content_rect()` — the rectangle of the real OS
        // viewport, not the one egui recomputes.
        let frame = egui::Frame::central_panel(ui.style())
            .inner_margin(WINDOW_CONTENT_MARGIN)
            .stroke(ui.style().visuals.window_stroke());
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            let Some(snapshot) = model.snapshot.clone() else {
                // Should never happen: `detach_tab` sets the
                // first snapshot before any detached frame.
                return;
            };
            header_actions = window_header(
                ui,
                id,
                &tab_title_from_tree(&snapshot.tree, tab),
                Some(tab),
                was_collapsed,
                HeaderPlacement::Detached,
            );
            if was_collapsed ^ header_actions.toggle_collapse {
                return;
            }
            // Same logical bound for both kinds: their
            // content may overflow, but it must modify neither
            // the Frame nor the viewport's clip rectangle.
            show_window_contents_bounded(ui, |ui| {
                match tab {
                    Tab::Book(id) => {
                        if let Some(view_state) = model.view_state.as_mut() {
                            book_contents(ui, view_state, &mut requests.actions, id);
                        } else {
                            ui.label(tr!("detached-workbook-unavailable"));
                        }
                    }
                    // Never here: the graph has its own window
                    // (`detached_graph.rs`), the note and the matrix never
                    // detach (`Tab::is_detachable`, #473): nothing to paint,
                    // and no panic (invariant 10).
                    Tab::Graph(_) | Tab::Matrix(_) | Tab::Note(_) => {}
                }
            });
        });
        (header_actions, frame)
    }

    fn collect_actions(
        &self,
        ui: &mut egui::Ui,
        requests: &mut DetachedTabRequests,
        keys: &mut Vec<Action>,
    ) {
        let tab = self.tab;
        let root_context = &self.root_context;
        // Undo, Redo and Save, read before the painting (#459), come after
        // the gestures of this frame: Ctrl+Z pressed in the frame a drag is
        // released undoes that drag (#398).
        requests.actions.append(keys);
        if let Some(mut action) = finish_file_drop(ui.ctx()) {
            // Same destination as a floating workbook, even
            // if its grid is collapsed and did not claim the drop.
            if let (Tab::Book(book), Action::DropFiles { target_book, .. }) = (tab, &mut action) {
                *target_book = Some(book);
            }
            requests.actions.push(action);
        }
        if !requests.actions.is_empty() {
            root_context.request_repaint_of(egui::ViewportId::ROOT);
        }
    }

    fn apply_window_actions(
        &self,
        ui: &mut egui::Ui,
        model: &mut DetachedTabViewModel,
        requests: &mut DetachedTabRequests,
        frame: DetachedTabFrame,
    ) {
        let DetachedTabFrame {
            was_collapsed,
            is_active,
            header_actions,
            inner_margin,
        } = frame;
        let size = self.size;
        let root_context = &self.root_context;
        // Collapse state that applies from this frame
        // (even before the assignment to `model.collapsed`
        // below) — computed once, serving at once
        // the target of the `ViewportCommand::InnerSize`, the guard
        // of the resize handles and the geometry
        // persisted at the end of closing.
        let collapsed_now = was_collapsed ^ header_actions.toggle_collapse;
        // Active outline (B7): painted on the rectangle of the
        // real OS viewport (`input.content_rect()`, already used
        // by `detached_resize_handles` just below), not
        // on the one egui recomputes from the content — see the
        // doc of the `Frame` above. `LayerId::background()`:
        // same layer as `CentralPanel`, painted after it thus
        // on top; a single detached viewport here, no other
        // window whose order would be contested.
        // The drop target's border replaces the active outline (#386): the
        // marker is the layer of the content (`CentralPanel`), read after it.
        if super::file_drop::is_drop_target(ui.ctx(), egui::LayerId::background()) {
            let content_rect = ui.ctx().input(|input| input.content_rect());
            ui.ctx()
                .layer_painter(egui::LayerId::background())
                .rect_stroke(
                    content_rect,
                    egui::CornerRadius::ZERO,
                    crate::theme::drop_target_stroke(),
                    egui::StrokeKind::Inside,
                );
        } else if is_active {
            let content_rect = ui.ctx().input(|input| input.content_rect());
            paint_active_window_outline(
                &ui.ctx().layer_painter(egui::LayerId::background()),
                content_rect,
                egui::CornerRadius::ZERO,
                ui.style(),
            );
        }
        if header_actions.toggle_collapse {
            model.collapsed = collapsed_now;
        }
        // B9: unlike the home-made handles of the internal floating
        // windows (harmless during a collapse,
        // since they mutate only a purely logical size),
        // these drive the real OS viewport through
        // `ViewportCommand::BeginResize` — leaving them active
        // during a collapse would let the user
        // enlarge the collapsed window live, a size
        // that would be neither displayed (the content stays skipped) nor
        // remembered (`expanded_size` is frozen during a collapse)
        // and thus lost at the first expand.
        if !collapsed_now {
            detached_resize_handles(ui);
        }
        // B9: a detached window is a real OS viewport — unlike
        // an internal floating window, where
        // `.fixed_size(display_size)` (B3) is enough to reduce what
        // is requested from egui, nothing here reduces the viewport's
        // size as long as the OS is not explicitly asked
        // to. Reuses the same computation as
        // `floating_window_display_size` (same height of
        // header band): on collapse, the remembered width and
        // the header height; on expand, the remembered
        // size as is. Sent only on the frame of the
        // toggle (not on every frame: a repeated `InnerSize`
        // while a user resizes the window by
        // hand would make it jump back).
        if header_actions.toggle_collapse {
            let expanded_size = model
                .expanded_size
                .unwrap_or_else(|| egui::vec2(size[0], size[1]));
            let target = floating_window_display_size(
                expanded_size,
                collapsed_now,
                WINDOW_HEADER_HEIGHT,
                inner_margin.sum(),
            );
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::InnerSize(target));
        }
        if header_actions.title_drag_started {
            super::os_gesture::start_drag(ui.ctx());
        }
        if header_actions.maximize {
            let is_maximized = ui
                .ctx()
                .input(|input| input.viewport().maximized)
                .unwrap_or(false);
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
        }
        if header_actions.detach {
            requests.reattach = true;
            root_context.request_repaint_of(egui::ViewportId::ROOT);
        }
        if header_actions.close {
            requests.close = true;
            root_context.request_repaint_of(egui::ViewportId::ROOT);
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
    use std::time::Duration;

    use super::super::graph_window::tests::plotted_graph;
    use super::super::grid::role_label;
    use super::super::test_support::bench::delay;
    use super::super::test_support::bench::{Bench, Pass, Window};
    use super::super::window_chrome::tests::find_icon_centers;
    use super::super::window_placement::GRAPH_WINDOW_SIZE;
    use super::*;
    use crate::theme;
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

    /// #124: the OS close of a detached workbook is a request for the root,
    /// not a reattach; the window stays until the root drains it. (Its note
    /// twin went with the detached note, #473.)
    #[test]
    fn a_detached_workbook_defers_its_closing() {
        let (session, _graph) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book = app.state.books()[0].0;
        let tab = Tab::Book(book);
        app.open_tab(tab);
        app.detach_tab(tab);
        assert!(app.detached_tabs.contains_key(&tab));
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        context.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&context);
        egui_extras::install_image_loaders(&context);
        let root_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let viewport_id = egui::ViewportId::from_hash_of(tab);
        let mut output = context.run_ui(root_input.clone(), |ui| app.show_detached_tabs(ui.ctx()));
        let callback = output.viewport_output[&viewport_id]
            .viewport_ui_cb
            .clone()
            .unwrap();
        output.textures_delta.clear();
        let paint = |close: bool| {
            let mut input = root_input.clone();
            input.viewport_id = viewport_id;
            input.viewports.insert(
                viewport_id,
                egui::ViewportInfo {
                    parent: Some(egui::ViewportId::ROOT),
                    events: if close {
                        vec![egui::ViewportEvent::Close]
                    } else {
                        Vec::new()
                    },
                    ..Default::default()
                },
            );
            input.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(420.0, 360.0),
            ));
            let mut output = context.run_ui(input, |ui| callback(ui));
            output.textures_delta.clear();
        };
        paint(false);
        paint(false);
        assert!(!app.detached_tabs[&tab].requests.lock().unwrap().close);
        paint(true);
        let requests = app.detached_tabs[&tab].requests.lock().unwrap();
        assert!(requests.close, "the OS cross must go back to the root");
        assert!(!requests.reattach, "the cross closes the detached view");
        assert!(
            app.detached_tabs.contains_key(&tab),
            "the closing also waits for its drain"
        );
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
            assert!(!app.detached_tabs.contains_key(&tab), "{tab:?}");
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

    /// The detached callbacks emit the same action and do not consume
    /// the pending drop in the root viewport (shared egui memory).
    #[test]
    fn detached_drops_keep_their_destination_and_are_isolated_from_the_root() {
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
        for kind in ["workbook", "explorer"] {
            let (session, graph) = plotted_graph();
            let state = AppState::new(session.handle(), "test.stf".into()).unwrap();
            let mut app = StokesForgeApp::new(state);
            let book = app.state.books()[0].0;
            let tab = if kind == "workbook" {
                Tab::Book(book)
            } else {
                Tab::Graph(graph)
            };
            let viewport = if kind == "explorer" {
                app.detach_project_manager();
                egui::ViewportId::from_hash_of("stokesforge_project_manager_window")
            } else {
                app.detach_tab(tab);
                egui::ViewportId::from_hash_of(tab)
            };
            let context = egui::Context::default();
            context.set_embed_viewports(false);
            crate::theme::install(&context);
            let mut root_input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 600.),
                )),
                ..Default::default()
            };
            root_input.dropped_files = vec![Arc::new(FakeFile("root.csv".into()))];
            let mut output = context.run_ui(root_input.clone(), |ui| {
                begin_file_drop(ui.ctx());
                if kind == "explorer" {
                    app.show_detached_project_manager(ui.ctx(), None);
                } else {
                    app.show_detached_tabs(ui.ctx());
                }
            });
            let callback = output.viewport_output[&viewport]
                .viewport_ui_cb
                .clone()
                .unwrap();
            output.textures_delta.clear();
            let mut detached_input = root_input.clone();
            detached_input.viewport_id = viewport;
            detached_input.viewports.insert(
                viewport,
                egui::ViewportInfo {
                    parent: Some(egui::ViewportId::ROOT),
                    ..Default::default()
                },
            );
            detached_input.dropped_files = vec![Arc::new(FakeFile("detached.csv".into()))];
            if kind == "workbook" {
                // #124: within a single frame, the shortcut joins the queue
                // before the drop. The order is observable at the root drain.
                detached_input.events.push(egui::Event::Key {
                    key: egui::Key::Z,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND.plus(egui::Modifiers::CTRL),
                });
            }
            context
                .run_ui(detached_input, |ui| callback(ui))
                .textures_delta
                .clear();
            let actions = if kind == "explorer" {
                std::mem::take(&mut app.pm_requests.lock().unwrap().actions)
            } else {
                std::mem::take(&mut app.detached_tabs[&tab].requests.lock().unwrap().actions)
            };
            let target = (kind == "workbook").then_some(book);
            let mut expected = Vec::new();
            if kind == "workbook" {
                expected.push(Action::Undo);
            }
            expected.push(Action::DropFiles {
                paths: vec!["detached.csv".into()],
                target_book: target,
            });
            assert_eq!(actions, expected);
            let mut root_action = None;
            root_input.dropped_files.clear();
            context
                .run_ui(root_input, |ui| {
                    root_action = finish_file_drop(ui.ctx());
                })
                .textures_delta
                .clear();
            assert_eq!(
                root_action,
                Some(Action::DropFiles {
                    paths: vec!["root.csv".into()],
                    target_book: None
                })
            );
        }
    }

    /// A file dropped on a detached graph is applied in the window's own pass
    /// (the import starts there, with no destination workbook) and does not
    /// consume the pending drop of the root viewport (shared egui memory).
    #[test]
    fn a_file_dropped_on_a_detached_graph_leaves_the_pending_drop_of_the_root() {
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
        let (session, _graph) = plotted_graph();
        let state = AppState::new(session.handle(), "test.stf".into()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book = app.state.books()[0].0;
        let tab = Tab::Book(book);
        app.detach_tab(tab);
        let viewport = egui::ViewportId::from_hash_of(tab);
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        crate::theme::install(&context);
        let root_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800., 600.),
            )),
            ..Default::default()
        };
        let mut output = context.run_ui(root_input.clone(), |ui| {
            app.show_detached_tabs(ui.ctx());
        });
        let callback = output.viewport_output[&viewport]
            .viewport_ui_cb
            .clone()
            .unwrap();
        output.textures_delta.clear();
        let size = egui::vec2(700., 500.);
        let content_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let selection = egui::Style::default().visuals.selection.stroke.color;
        let drop_color = crate::theme::drop_target_stroke().color;
        let mut strokes = Vec::new();
        for _ in 0..3 {
            let mut input = root_input.clone();
            input.viewport_id = viewport;
            input.focused = true;
            input.viewports.insert(
                viewport,
                egui::ViewportInfo {
                    parent: Some(egui::ViewportId::ROOT),
                    ..Default::default()
                },
            );
            input.screen_rect = Some(content_rect);
            input.hovered_files = vec![egui::HoveredFile::default()];
            input
                .events
                .push(egui::Event::PointerMoved(egui::pos2(350., 300.)));
            let mut output = context.run_ui(input, |ui| callback(ui));
            output.textures_delta.clear();
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
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        context.set_fonts(stokesforge_render_egui::font_definitions());

        let book_tab = Tab::Book(book);
        assert!(
            app.floating_tabs.contains(&book_tab),
            "StokesForgeApp::new must open the first workbook in a floating window"
        );
        app.detach_tab(book_tab);

        let viewport_id = egui::ViewportId::from_hash_of(book_tab);
        let root_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 700.0),
            )),
            ..Default::default()
        };
        let mut output = context.run_ui(root_input.clone(), |ui| {
            app.show_detached_tabs(ui.ctx());
        });
        let callback = output
            .viewport_output
            .get(&viewport_id)
            .and_then(|viewport| viewport.viewport_ui_cb.clone())
            .expect("the detaching must publish a viewport callback");
        output.textures_delta.clear();

        // OS window deliberately narrower than the ten named
        // columns and their headers — like the native shrinking in the
        // reproduction of the bug report.
        let window_size = egui::vec2(200.0, 150.0);
        let window_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, window_size);
        let mut viewport_input = root_input;
        viewport_input.viewport_id = viewport_id;
        viewport_input.viewports.insert(
            viewport_id,
            egui::ViewportInfo {
                parent: Some(egui::ViewportId::ROOT),
                ..Default::default()
            },
        );
        viewport_input.screen_rect = Some(window_rect);
        let mut output = context.run_ui(viewport_input, |ui| callback(ui));
        output.textures_delta.clear();

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

    fn send(app: &StokesForgeApp, actions: Vec<Action>) {
        let (_, detached) = app.detached_tabs.iter().next().unwrap();
        detached.requests.lock().unwrap().actions.extend(actions);
    }

    fn drain(app: &mut StokesForgeApp, ctx: &egui::Context) -> Vec<Action> {
        let mut actions = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.drain_detached_tabs(ui.ctx(), &mut actions);
        });
        output.textures_delta.clear();
        actions
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
        send(&app, vec![Action::RecalculateTranspose { sheet }]);
        let actions = drain(&mut app, &ctx);
        assert_eq!(actions.len(), 1);
        app.dispatch_actions(&ctx, actions);
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

    /// #471: the detached console refuses a run while a dialog is open, with
    /// the band in its own window. Witness: restoring the old drain.
    #[test]
    fn the_detached_console_refuses_while_a_dialog_is_open() {
        let (session, mut app, _) = detached_graph_app();
        app.detach_script_console();
        let ctx = egui::Context::default();
        app.state.dispatch(Action::OpenAbout);
        app.script_view
            .as_ref()
            .unwrap()
            .requests
            .lock()
            .unwrap()
            .actions
            .push(Action::RunScript);
        let mut actions = Vec::new();
        app.drain_script_console(&ctx, &mut actions);
        assert!(actions.is_empty());
        let now = std::time::Instant::now();
        assert!(
            super::super::window_band::text_at(&ctx, script_viewport(), now).is_some(),
            "band in the console"
        );
        app.state.dispatch(Action::CloseAbout);
        app.script_view
            .as_ref()
            .unwrap()
            .requests
            .lock()
            .unwrap()
            .actions
            .push(Action::RunScript);
        app.drain_script_console(&ctx, &mut actions);
        assert_eq!(actions.len(), 1);
        session.shutdown().unwrap();
    }

    /// #471: the detached Object Manager lets a selection through and refuses
    /// the rest. Witness: restoring the old drain, or refusing `Select` too.
    #[test]
    fn the_detached_object_manager_refuses_all_but_a_selection() {
        let (session, mut app, _) = detached_graph_app();
        let ctx = egui::Context::default();
        let tree = app.state.tree().clone();
        app.om_view = Some(DetachedObjectManager {
            model: Arc::new(Mutex::new(ObjectManagerModel {
                stamp: (0, 0, 0),
                tree,
                graph: None,
                selection: None,
                model: None,
                spec: None,
                settings: Default::default(),
            })),
            requests: Arc::default(),
        });
        app.state.dispatch(Action::OpenAbout);
        app.om_view
            .as_ref()
            .unwrap()
            .requests
            .lock()
            .unwrap()
            .actions
            .push(Action::Undo);
        let mut actions = Vec::new();
        app.drain_object_manager(&ctx, &mut actions);
        assert!(actions.is_empty());
        let now = std::time::Instant::now();
        assert!(super::super::window_band::text_at(&ctx, object_manager_viewport(), now).is_some());
        session.shutdown().unwrap();
    }

    /// #471: the detached Project Manager refuses writes while a dialog is open
    /// (the Delete that Cancel used to bring back). Witness: restoring the old
    /// drain.
    #[test]
    fn the_detached_project_manager_refuses_while_a_dialog_is_open() {
        let (session, mut app, _) = detached_graph_app();
        app.pm_detached = true;
        app.pm_open = false;
        let ctx = egui::Context::default();
        app.state.dispatch(Action::OpenAbout);
        app.pm_requests.lock().unwrap().actions.push(Action::Undo);
        let (mut open, mut close, mut actions) = (Vec::new(), Vec::new(), Vec::new());
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_project_manager(
                ui,
                &ui.ctx().clone(),
                None,
                &mut open,
                &mut close,
                &mut actions,
            );
        });
        output.textures_delta.clear();
        assert!(actions.is_empty());
        let now = std::time::Instant::now();
        assert!(super::super::window_band::text_at(
            &ctx,
            super::super::project_manager::viewport_id(),
            now
        )
        .is_some());
        session.shutdown().unwrap();
    }
}
