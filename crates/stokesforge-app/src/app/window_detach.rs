//! Detaching into OS windows: the three fixed panels and each content window
//! can live in a dedicated viewport (`show_viewport_deferred`).
//! The closure of this callback is `'static` and `Send + Sync`: it cannot
//! capture `&mut AppState`, hence the local views (`DetachedTabViewModel`,
//! `ProjectManagerViewModel`) and the requests they send back to the root.
//!
//! Moved from `app.rs` (#101), then split by phase (#124): publishing
//! the snapshot, painting and collecting events, window commands.
//! The callback's payload stays owned and `'static`; its two locks
//! keep the same duration, and business actions still wait for the root.
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
//! suffer from it directly, but the limit is worth knowing before one day
//! adding a remembered position.
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
//! [`Tab::Book`], [`Tab::Graph`], [`Tab::Matrix`] and [`Tab::Note`] — a
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
//! the content of the four kinds in a single bounded, clipped child `Ui`,
//! right before the `match` that routes to each of them.
//!
//! # Why no `ViewModel` per content kind as for the PM
//!
//! The PM has only one form of content (a read-only tree); the
//! four window kinds here are frankly different (virtualized editable
//! grid, interactive graph with shape cache, editable text,
//! placeholder). Rather than four disjoint `ViewModel`/`Requests`
//! to make coexist in [`StokesForgeApp::detached_tabs`],
//! [`DetachedTabViewModel`] is a single structure carrying one field per
//! content kind (`view_state` for a workbook or a note,
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
//! thus keep the single undoable-command path. The note follows the same
//! pattern: [`note_contents`] on the local view, whose input buffer
//! ([`AppState::note_editing`]) stays specific to the window.
//!
//! [`Tab::Graph`] escapes this limit: [`stokesforge_render_egui::GraphView`]
//! has no global lock on `AppState` to respect (its shape cache and
//! its zoom/pan navigation are already scoped per graph, in
//! [`StokesForgeApp::graph_views`]) — [`graph_view_contents`] is thus
//! **literally** the same function for a floating window and for a
//! detached window, the view itself simply migrating out of `graph_views` for the
//! duration of the detaching ([`StokesForgeApp::detach_tab`]/[`StokesForgeApp::
//! reattach_tab`]) to stay an `Arc<Mutex<_>>` the closure can capture
//! without duplicating the cache.
//!
//! [`Tab::Matrix`] has no state to carry (placeholder):
//! [`matrix_contents`] reads only the tree and is used as is by both
//! windows.
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
use stokesforge_render_egui::GraphView;
use stokesforge_session::{Snapshot, WindowGeometry};

use stokesforge_app_state::state::{Action, AppState, ProjectTreeNode};
use stokesforge_app_state::tr;

use super::contents::{matrix_contents, note_contents};
use super::figure_copy::request_figure_copy;
use super::file_drop::{begin_file_drop, finish_file_drop};
use super::graph_window::graph_view_contents;
use super::object_manager_ui::{
    object_manager_contents, selected_manager_target, ObjectManagerRequest,
};
use super::project_manager::{
    project_manager_contents_from_tree, PmEntry, PmInteraction, PROJECT_MANAGER_WIDTH,
};
use super::script_console::{script_model_contents, ScriptConsoleModel};
use super::shortcuts::collect_undo_redo_shortcuts;
use super::tab::{tab_node, Tab};
use super::tab::{tab_title, tab_title_from_tree};
use super::window_chrome::{
    detached_resize_handles, paint_active_window_outline, panel_header,
    show_window_contents_bounded, window_header, HeaderPlacement, PanelPlacement,
    WindowHeaderActions, MIN_CONTENT_WINDOW_SIZE, WINDOW_CONTENT_MARGIN, WINDOW_HEADER_HEIGHT,
};
use super::window_placement::{floating_window_display_size, GRAPH_WINDOW_SIZE};
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

/// The detached panels have the home-made frame and a single window action.
/// A close requested by the compositor becomes a reattach.
fn detached_panel_contents(
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
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
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
/// the four detachable kinds rather than four disjoint `ViewModel`s —
/// see the top doc of the module for the trade-off. Each field, except
/// `snapshot`, makes sense only for one particular kind of [`Tab`]; the
/// comment says which.
pub(super) struct DetachedTabViewModel {
    /// Snapshot of the model, resynchronized on every frame — see the top doc of the
    /// module, § "the model read is factored out". `None`
    /// only before the very first synchronization (never observable
    /// from outside [`StokesForgeApp::detach_tab`], which sets it at once).
    pub(super) snapshot: Option<Snapshot>,
    /// `Tab::Book`/`Tab::Note`: [`AppState`] view local to the window, which
    /// renders the content shared with the floating window (same
    /// `book_contents`/`note_contents`) while keeping its selection and its
    /// input in progress.
    pub(super) view_state: Option<AppState>,
    /// `Tab::Graph`: the view (shape cache, zoom/pan), moved out of
    /// [`StokesForgeApp::graph_views`] for the duration of the detaching — see
    /// [`StokesForgeApp::detach_tab`]/[`StokesForgeApp::reattach_tab`].
    pub(super) graph_view: Option<GraphView>,
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

impl StokesForgeApp {
    pub(super) fn drain_object_manager(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let Some(view) = &self.om_view else {
            return;
        };
        let (gestures, reattach) = {
            let mut requests = view.requests.lock().unwrap_or_else(|err| err.into_inner());
            actions.append(&mut requests.actions);
            (
                std::mem::take(&mut requests.gestures),
                std::mem::take(&mut requests.reattach),
            )
        };
        for (graph, request) in gestures {
            self.run_object_manager_request(ctx, graph, request, actions);
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
        let id = egui::ViewportId::from_hash_of("stokesforge_object_manager_window");
        ctx.show_viewport_deferred(
            id,
            egui::ViewportBuilder::default()
                .with_title(tr!("action-object-manager"))
                .with_decorations(false)
                .with_inner_size([330.0, 480.0])
                .with_min_inner_size([220.0, 140.0]),
            move |ui, _class| {
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
            },
        );
        if changed {
            ctx.request_repaint_of(id);
        }
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
        let (source, reattach) = {
            let mut requests = view.requests.lock().unwrap_or_else(|err| err.into_inner());
            actions.append(&mut requests.actions);
            (
                requests.source.take(),
                std::mem::take(&mut requests.reattach),
            )
        };
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
        let id = egui::ViewportId::from_hash_of("stokesforge_script_window");
        ctx.show_viewport_deferred(
            id,
            egui::ViewportBuilder::default()
                .with_title(tr!("script-title"))
                .with_decorations(false)
                .with_inner_size([700.0, 440.0])
                .with_min_inner_size([300.0, 220.0]),
            move |ui, _class| {
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
                if source != model.source || reattach || !actions.is_empty() {
                    let mut requests = requests.lock().unwrap_or_else(|err| err.into_inner());
                    if source != model.source {
                        requests.source = Some(model.source.clone());
                    }
                    requests.reattach |= reattach;
                    requests.actions.append(&mut actions);
                    root.request_repaint_of(egui::ViewportId::ROOT);
                }
            },
        );
        if changed {
            ctx.request_repaint_of(id);
        }
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
        let changed = {
            let mut model = self
                .pm_view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            let stamp = snapshot_stamp(self.state.snapshot());
            let changed = model.snapshot_stamp != Some(stamp)
                || model.selected != self.pm_selection
                || model.open != open
                || model.active_tab != active_tab;
            model.snapshot_stamp = Some(stamp);
            model.tree = self.state.project_tree();
            model.root = Some(self.state.tree().root());
            model.selected = self.pm_selection.clone();
            model.open = open;
            model.active_tab = active_tab;
            changed
        };
        let view_model = Arc::clone(&self.pm_view_model);
        let requests = Arc::clone(&self.pm_requests);
        let root_context = ctx.clone();
        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("stokesforge_project_manager_window"),
            egui::ViewportBuilder::default()
                .with_title(tr!("window-title-project-manager"))
                .with_inner_size([PROJECT_MANAGER_WIDTH + 40.0, 480.0])
                .with_decorations(false)
                .with_min_inner_size([220.0, 140.0]),
            move |ui, _class| {
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
                    };
                    project_manager_contents_from_tree(ui, &model.tree, &mut interaction);
                });
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
            },
        );
        if changed {
            ctx.request_repaint_of(egui::ViewportId::from_hash_of(
                "stokesforge_project_manager_window",
            ));
        }
    }

    /// Removes `tab` from the floating windows and detaches it into a separate
    /// OS window — see the top doc of the module, § generic detaching. No
    /// effect if `tab` is already detached or is not a currently
    /// open floating window. Neither the Project Manager nor the script console
    /// goes through this path since they became fixed panels — see
    /// [`StokesForgeApp::detach_project_manager`] and
    /// [`StokesForgeApp::detach_script_console`] for their own paths.
    pub(super) fn detach_tab(&mut self, tab: Tab) {
        if self.detached_tabs.contains_key(&tab) {
            return;
        }
        if let Some(pos) = self.floating_tabs.iter().position(|t| *t == tab) {
            self.floating_tabs.remove(pos);
            self.cascade_rank.remove(&tab);
        } else {
            return;
        }
        if self.maximized_tab == Some(tab) {
            // A maximized tab that was no longer a floating window
            // would be an inconsistent state: step down cleanly before
            // detaching it.
            self.maximized_tab = None;
        }
        let graph_view = match tab {
            Tab::Graph(id) => {
                let mut view = self.graph_views.remove(&id).unwrap_or_default();
                view.invalidate_surface();
                Some(view)
            }
            _ => None,
        };
        self.window_positions.remove(&tab);
        self.window_sizes.remove(&tab);
        let view_state = match tab {
            Tab::Book(_) | Tab::Note(_) => self.state.fork_for_detached_view().ok(),
            _ => None,
        };
        let view_model = DetachedTabViewModel {
            snapshot: Some(self.state.snapshot().clone()),
            view_state,
            graph_view,
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

    /// Removes `tab` from [`StokesForgeApp::detached_tabs`] and pushes it back as a
    /// floating window (new cascade rank). No effect if it is not
    /// detached. Any [`GraphView`] moved by
    /// [`StokesForgeApp::detach_tab`] returns to [`StokesForgeApp::graph_views`] — the
    /// graph's shape cache survives detaching/reattaching, as
    /// it already survives the closing of a floating window.
    pub(super) fn reattach_tab(&mut self, tab: Tab) {
        let Some(detached) = self.detached_tabs.remove(&tab) else {
            return;
        };
        {
            let mut model = detached
                .view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            if let (Tab::Graph(id), Some(view)) = (tab, model.graph_view.take()) {
                let mut view = view;
                view.invalidate_surface();
                self.graph_views.insert(id, view);
            }
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
        let Some(detached) = self.detached_tabs.remove(&tab) else {
            return;
        };
        let mut model = detached
            .view_model
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        if let (Tab::Graph(id), Some(view)) = (tab, model.graph_view.take()) {
            let mut view = view;
            view.invalidate_surface();
            self.graph_views.insert(id, view);
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
        let mut detached: Vec<NodeId> = self.detached_tabs.keys().map(|t| tab_node(*t)).collect();
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
            let mode = if self.maximized_tab.map(tab_node) == Some(node) {
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
    pub(super) fn show_detached_tabs(&mut self, ctx: &egui::Context) {
        let Self {
            state,
            detached_tabs,
            ..
        } = self;
        for (tab, detached) in detached_tabs.iter() {
            let changed = detached.publish_snapshot(state);
            let view_model = Arc::clone(&detached.view_model);
            let requests = Arc::clone(&detached.requests);
            let root_context = ctx.clone();
            let tab = *tab;
            let title = tr!("window-title-detached", name = tab_title(state, tab));
            // `Tab::Graph` starts at a size that contains its reference
            // figure and its return to the initial framing.
            // The rest (`Book`/`Matrix`/`Note`) shares a uniform size.
            // The workbook's grid now uses the same widths
            // and headers as in the floating window.
            // `Tab::Note` has no grid strictly speaking but gets
            // the same size as "the others" to stay consistent with
            // them — this is explicitly intended, not an oversight.
            let size = match tab {
                Tab::Graph(_) => [GRAPH_WINDOW_SIZE.x, GRAPH_WINDOW_SIZE.y],
                _ => [420.0, 360.0],
            };
            // Without native decoration: the home-made bar (`window_header`,
            // placement `Detached`) is all the user has to
            // move/resize/close this window — see the top doc of the
            // module, § generic detaching, and
            // [`detached_resize_handles`] for the resizing.
            // Neither position nor size remembered: the detached OS window has
            // no persisted geometry (issue #16), only the floating
            // geometry is.
            let viewport = egui::ViewportBuilder::default()
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
            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of(tab),
                viewport,
                move |ui, _class| payload.show(ui),
            );
            if changed {
                ctx.request_repaint_of(egui::ViewportId::from_hash_of(tab));
            }
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
    /// The two locks stay held during the whole frame, as before the
    /// split: no root drain comes in between its phases.
    fn show(&self, ui: &mut egui::Ui) {
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
        let (header_actions, frame) =
            self.paint_contents(ui, &mut model, &mut requests, was_collapsed);
        self.collect_actions(ui, &mut requests);
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
        let root_context = &self.root_context;
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
            // Same logical bound for the four kinds: their
            // content may overflow, but it must modify neither
            // the Frame nor the viewport's clip rectangle.
            show_window_contents_bounded(ui, |ui| {
                let tree = &snapshot.tree;
                match tab {
                    Tab::Book(id) => {
                        if let Some(view_state) = model.view_state.as_mut() {
                            book_contents(ui, view_state, &mut requests.actions, id);
                        } else {
                            ui.label(tr!("detached-workbook-unavailable"));
                        }
                    }
                    Tab::Graph(id) => {
                        let view = model.graph_view.get_or_insert_with(GraphView::default);
                        let before = selected_manager_target(view);
                        // Ctrl+C in this viewport: the figure of **this** graph (#149).
                        request_figure_copy(ui.ctx(), view, tree, id, &mut requests.actions);
                        graph_view_contents(ui, view, &snapshot, id, &mut requests.actions);
                        if before != selected_manager_target(view) {
                            root_context.request_repaint_of(egui::ViewportId::ROOT);
                        }
                    }
                    Tab::Matrix(id) => {
                        matrix_contents(ui, tree, id);
                    }
                    Tab::Note(id) => {
                        if let Some(view_state) = model.view_state.as_mut() {
                            note_contents(ui, view_state, &mut requests.actions, id);
                        } else {
                            ui.label(tr!("detached-note-unavailable"));
                        }
                    }
                }
            });
        });
        (header_actions, frame)
    }

    fn collect_actions(&self, ui: &mut egui::Ui, requests: &mut DetachedTabRequests) {
        let tab = self.tab;
        let root_context = &self.root_context;
        // The edit shortcuts are read in the context
        // of the viewport that has the focus, as for the root.
        if matches!(tab, Tab::Book(_)) {
            collect_undo_redo_shortcuts(ui.ctx(), &mut requests.actions);
        }
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
        if is_active {
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
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
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

    use super::super::graph_window::graph_drop_target;
    use super::super::graph_window::tests::plotted_graph;
    use super::super::grid::role_label;
    use super::super::project_manager::tests::find_text_center;
    use super::super::window_chrome::tests::find_icon_centers;
    use super::*;
    use crate::theme;
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

    /// #124: the owned payload of a note keeps its local input; losing
    /// focus produces a request, then only the root applies it.
    /// The OS close is also a request, without reattaching.
    #[test]
    fn a_detached_note_defers_its_editing_and_its_closing() {
        let mut project = Project::new("Detached note").unwrap();
        let note = project
            .add_note(project.root(), Some("Journal"), "Start")
            .unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let tab = Tab::Note(note);
        app.open_tab(tab);
        app.detach_tab(tab);
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
        let paint = |events: Vec<egui::Event>, close: bool| {
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
            input.events = events;
            let mut output = context.run_ui(input, |ui| callback(ui));
            output.textures_delta.clear();
            output
        };
        paint(Vec::new(), false);
        let first = paint(Vec::new(), false);
        let inside = find_text_center(&first.shapes, "Start") + egui::vec2(80.0, 0.0);
        let pointer = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        paint(
            vec![egui::Event::PointerMoved(inside), pointer(inside, true)],
            false,
        );
        paint(vec![pointer(inside, false)], false);
        paint(vec![egui::Event::Text(" more".into())], false);
        {
            let model = app.detached_tabs[&tab].view_model.lock().unwrap();
            assert_eq!(
                model
                    .view_state
                    .as_ref()
                    .unwrap()
                    .note_editing()
                    .unwrap()
                    .buffer,
                "Start more"
            );
        }
        assert!(app.detached_tabs[&tab]
            .requests
            .lock()
            .unwrap()
            .actions
            .is_empty());
        assert_eq!(app.state.note_text(note).as_deref(), Some("Start"));

        let outside = egui::pos2(200.0, 15.0);
        paint(
            vec![egui::Event::PointerMoved(outside), pointer(outside, true)],
            false,
        );
        paint(vec![pointer(outside, false)], false);
        let actions = std::mem::take(&mut app.detached_tabs[&tab].requests.lock().unwrap().actions);
        assert_eq!(
            actions,
            vec![Action::SetNoteText {
                note,
                text: "Start more".into()
            }]
        );
        assert_eq!(
            app.state.note_text(note).as_deref(),
            Some("Start"),
            "the request waits for the root"
        );
        for action in actions {
            app.state.dispatch(action);
        }
        assert_eq!(app.state.note_text(note).as_deref(), Some("Start more"));
        app.state.dispatch(Action::Undo);
        assert_eq!(app.state.note_text(note).as_deref(), Some("Start"));

        assert!(!app.detached_tabs[&tab].requests.lock().unwrap().close);
        paint(Vec::new(), true);
        let requests = app.detached_tabs[&tab].requests.lock().unwrap();
        assert!(requests.close, "the OS cross must go back to the root");
        assert!(!requests.reattach, "the cross closes the detached view");
        assert!(
            app.detached_tabs.contains_key(&tab),
            "the closing also waits for its drain"
        );
    }

    /// #104: no mouse input in the reader viewports; the
    /// publication of a new snapshot is enough to ask for their redraw.
    #[test]
    fn a_mutation_wakes_the_detached_windows_at_rest() {
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book = app.state.books()[0].0;
        let sheet = app.state.selected_sheet_in(book).unwrap();
        let column = app.state.visible_columns(sheet)[0].0;
        app.detach_tab(Tab::Book(book));
        app.detach_tab(Tab::Graph(graph));
        app.detach_project_manager();
        let ids = [
            egui::ViewportId::from_hash_of(Tab::Book(book)),
            egui::ViewportId::from_hash_of(Tab::Graph(graph)),
            egui::ViewportId::from_hash_of("stokesforge_project_manager_window"),
        ];
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let paint = |app: &mut StokesForgeApp| {
            let mut output = ctx.run_ui(input.clone(), |ui| {
                app.show_detached_tabs(ui.ctx());
                app.show_detached_project_manager(ui.ctx(), None);
            });
            output.textures_delta.clear();
            output
        };
        let settle = |app: &mut StokesForgeApp| {
            for _ in 0..8 {
                let output = paint(app);
                for id in ids {
                    let callback = output.viewport_output[&id].viewport_ui_cb.as_ref().unwrap();
                    let mut child = input.clone();
                    child.viewport_id = id;
                    child.viewports.insert(
                        id,
                        egui::ViewportInfo {
                            parent: Some(egui::ViewportId::ROOT),
                            ..Default::default()
                        },
                    );
                    ctx.run_ui(child, |ui| callback(ui)).textures_delta.clear();
                }
            }
            let output = paint(app);
            for id in ids {
                assert_eq!(
                    output.viewport_output[&id].repaint_delay,
                    Duration::MAX,
                    "the viewport {id:?} must sleep at rest"
                );
            }
        };
        settle(&mut app);
        app.state.dispatch(Action::CommitCell {
            column,
            row: 0,
            text: "42".into(),
        });
        let output = paint(&mut app);
        for id in ids {
            assert_eq!(
                output.viewport_output[&id].repaint_delay,
                Duration::ZERO,
                "the mutation must wake {id:?}"
            );
        }
        settle(&mut app);
        app.state.set_window_geometry(
            graph,
            WindowGeometry {
                x: 10.0,
                y: 20.0,
                width: 500.0,
                height: 400.0,
                view: None,
            },
        );
        app.state.refresh();
        let output = paint(&mut app);
        for id in ids {
            assert_eq!(
                output.viewport_output[&id].repaint_delay,
                Duration::ZERO,
                "the geometry must wake {id:?}"
            );
        }
        settle(&mut app);
        app.pm_selection
            .insert(super::super::project_manager::PmEntry::Window(Tab::Book(
                book,
            )));
        let output = paint(&mut app);
        assert_eq!(
            output.viewport_output[&ids[2]].repaint_delay,
            Duration::ZERO,
            "a navigation selection wakes the Project Manager"
        );
        for id in &ids[..2] {
            assert_eq!(output.viewport_output[id].repaint_delay, Duration::MAX);
        }
        settle(&mut app);
        session.shutdown().unwrap();
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
        app.detached_tabs.clear();
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
        for kind in ["workbook", "graph", "explorer"] {
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

    #[test]
    fn a_detached_graph_rebuilds_its_shapes_in_its_viewport() {
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        context.set_fonts(stokesforge_render_egui::font_definitions());
        assert!(app.floating_tabs.contains(&Tab::Graph(graph)));

        // Paint the floating view first: the detaching must transfer
        // an already filled `GraphView`, then rebuild it in the
        // coordinates of the distinct OS viewport.
        let mut root_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 700.0),
            )),
            ..Default::default()
        };
        let mut output = context.run_ui(root_input.clone(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                graph_drop_target(ui, &app.state, &mut Vec::new(), &mut app.graph_views, graph);
            });
        });
        output.textures_delta.clear();
        assert!(
            !app.graph_views[&graph].cache().shapes().is_empty(),
            "the starting floating view must have built its figure"
        );

        app.detach_tab(Tab::Graph(graph));
        let viewport_id = egui::ViewportId::from_hash_of(Tab::Graph(graph));
        output = context.run_ui(root_input.clone(), |ui| {
            app.show_detached_tabs(ui.ctx());
        });
        let callback = output
            .viewport_output
            .get(&viewport_id)
            .and_then(|viewport| viewport.viewport_ui_cb.clone());
        output.textures_delta.clear();
        let callback = callback.expect("the detaching must publish a viewport callback");

        root_input.viewport_id = viewport_id;
        root_input.viewports.insert(
            viewport_id,
            egui::ViewportInfo {
                parent: Some(egui::ViewportId::ROOT),
                ..Default::default()
            },
        );
        root_input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            GRAPH_WINDOW_SIZE,
        ));
        output = context.run_ui(root_input, |ui| callback(ui));
        output.textures_delta.clear();
        assert_eq!(
            output.viewport_output[&viewport_id].repaint_delay,
            Duration::ZERO,
            "the first callback must schedule the viewport's stabilization frame"
        );

        let model = app.detached_tabs[&Tab::Graph(graph)]
            .view_model
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        assert!(
            !model.needs_initial_repaint,
            "the first callback must ask for a single stabilization frame"
        );
        let view = model.graph_view.as_ref().unwrap();
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

    // B9 (`retours-2026-09-23-bugs.md`): the B3 fix
    // reduces the size requested from egui for an internal floating window
    // during a collapse, but a detached window is a real OS viewport —
    // nothing sent a `ViewportCommand::InnerSize` when its collapse
    // chevron toggled, so the OS kept the big expanded size. This test
    // drives the detached viewport's callback (same scheme as
    // `a_detached_graph_rebuilds_its_shapes_in_its_viewport`), clicks
    // the collapse chevron, and checks the geometry/state — not
    // the appearance: the rendering tests do not cover this mechanism (see
    // the report, § "In your final report").
    #[test]
    fn a_collapsed_detached_window_shrinks_then_restores_its_os_viewport() {
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        context.set_fonts(stokesforge_render_egui::font_definitions());
        egui_extras::install_image_loaders(&context);

        let root_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 700.0),
            )),
            ..Default::default()
        };
        context
            .run_ui(root_input.clone(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    graph_drop_target(ui, &app.state, &mut Vec::new(), &mut app.graph_views, graph);
                });
            })
            .textures_delta
            .clear();

        app.detach_tab(Tab::Graph(graph));
        let tab = Tab::Graph(graph);
        let viewport_id = egui::ViewportId::from_hash_of(tab);
        let mut output = context.run_ui(root_input.clone(), |ui| {
            app.show_detached_tabs(ui.ctx());
        });
        let callback = output.viewport_output[&viewport_id]
            .viewport_ui_cb
            .clone()
            .expect("the detaching must publish a viewport callback");
        output.textures_delta.clear();

        // Simulated "OS" rectangle: the test controls it, as the real
        // backend would by reporting, frame after frame, the size
        // actually applied by the native window.
        let outer_rect = std::cell::Cell::new(egui::Rect::from_min_size(
            egui::pos2(100.0, 80.0),
            GRAPH_WINDOW_SIZE,
        ));

        let paint = |events: Vec<egui::Event>| -> egui::FullOutput {
            let mut input = root_input.clone();
            input.viewport_id = viewport_id;
            input.viewports.insert(
                viewport_id,
                egui::ViewportInfo {
                    parent: Some(egui::ViewportId::ROOT),
                    outer_rect: Some(outer_rect.get()),
                    ..Default::default()
                },
            );
            input.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                outer_rect.get().size(),
            ));
            input.events = events;
            let mut output = context.run_ui(input, |ui| callback(ui));
            output.textures_delta.clear();
            output
        };

        // Stabilization frame (Wayland, see `needs_initial_repaint`):
        // consumes the initial repaint and primes `expanded_size` with the
        // starting size.
        paint(Vec::new());
        // Second frame: the icons' SVG textures are ready.
        let first = paint(Vec::new());
        let icon_pos = *find_icon_centers(&first.shapes)
            .first()
            .expect("the collapse arrow must be painted, at the top left");

        // A click (null drag, press then release) on the collapse
        // chevron, spread over two frames as egui's click detection
        // requires — see `click_at`.
        paint(vec![egui::Event::PointerMoved(icon_pos)]);
        paint(vec![egui::Event::PointerButton {
            pos: icon_pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        let collapsed_output = paint(vec![egui::Event::PointerButton {
            pos: icon_pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);

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
        {
            let model = app.detached_tabs[&tab]
                .view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            assert!(model.collapsed, "the collapse must be applied to the model");
        }

        // The OS applies the command: the next frame reports the small
        // rectangle. Nothing of it is persisted since issue #16: the
        // detached OS window has no remembered geometry (only the
        // floating geometry is, see `sync_window_views`).
        outer_rect.set(egui::Rect::from_min_size(
            outer_rect.get().min,
            egui::vec2(GRAPH_WINDOW_SIZE.x, expected_collapsed_height),
        ));
        paint(Vec::new());

        // Expand: same chevron (different glyph, same rectangle), same
        // click scheme — must recover exactly the size from before
        // the collapse, not a default size.
        paint(vec![egui::Event::PointerMoved(icon_pos)]);
        paint(vec![egui::Event::PointerButton {
            pos: icon_pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        let expanded_output = paint(vec![egui::Event::PointerButton {
            pos: icon_pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
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
        {
            let model = app.detached_tabs[&tab]
                .view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            assert!(!model.collapsed, "the expand must be applied to the model");
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
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        context.set_fonts(stokesforge_render_egui::font_definitions());

        let root_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 700.0),
            )),
            ..Default::default()
        };
        context
            .run_ui(root_input.clone(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    graph_drop_target(ui, &app.state, &mut Vec::new(), &mut app.graph_views, graph);
                });
            })
            .textures_delta
            .clear();

        app.detach_tab(Tab::Graph(graph));
        let viewport_id = egui::ViewportId::from_hash_of(Tab::Graph(graph));
        let mut output = context.run_ui(root_input.clone(), |ui| {
            app.show_detached_tabs(ui.ctx());
        });
        let callback = output.viewport_output[&viewport_id]
            .viewport_ui_cb
            .clone()
            .expect("the detaching must publish a viewport callback");
        output.textures_delta.clear();

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

        let paint_focused = |focused: bool| {
            let mut input = root_input.clone();
            input.viewport_id = viewport_id;
            input.focused = focused;
            input.viewports.insert(
                viewport_id,
                egui::ViewportInfo {
                    parent: Some(egui::ViewportId::ROOT),
                    ..Default::default()
                },
            );
            input.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                GRAPH_WINDOW_SIZE,
            ));
            let mut output = context.run_ui(input, |ui| callback(ui));
            output.textures_delta.clear();
            stroke_shapes(&output.shapes)
        };

        let style = egui::Style::default();
        let content_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, GRAPH_WINDOW_SIZE);

        // Other shapes of the window (border of the plotted graph, etc.) have
        // their own stroke, unrelated to activity — so we look for
        // the presence of the right outline rather than the exclusivity of its color.
        let inactive_strokes = paint_focused(false);
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

        let active_strokes = paint_focused(true);
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
    }
}
