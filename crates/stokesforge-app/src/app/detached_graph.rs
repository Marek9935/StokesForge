//! The detached graph window (#475, step 2, L1): a graph in its own OS window
//! that serves itself.
//!
//! The closure of the viewport locks the whole application through
//! [`lock_app_from_viewport`] (the one blocking door, after the root released
//! its guard), paints the graph from the shared state, and applies the actions
//! of its own gestures at once, in the same pass (`dispatch_actions`). No copy
//! of the model, no queue and no root pass stand between a click and its
//! effect: a root covered by this window (#457) does not delay it.
//!
//! What the window keeps for itself is a view state, [`DetachedGraph`]: the
//! [`GraphView`] (shape cache, zoom/pan), the collapse, the focus, the size to
//! restore. It lives in [`StokesForgeApp::detached_graphs`] and is reached under
//! the lock like everything else.
//!
//! **What stays with the root.** Reattach and Close only set a flag
//! ([`DetachedGraph::reattach`], [`DetachedGraph::close`]) and wake the root,
//! which moves the window ([`StokesForgeApp::drain_detached_graph_flags`], as
//! before; the window closing itself is L4). The dialogs are painted by the
//! root (#472): this window shows the band that says where the dialog is. Not
//! run in this pass, because they target the current viewport or belong to the
//! root alone: `sync_window_title_and_status`, `poll_background_jobs`,
//! `refresh_monitors`, the occlusion opt-in, the dialogs, the stack of the
//! floating windows.
//!
//! **Current-viewport traps.** In a child pass `request_repaint`,
//! `send_viewport_cmd` and `ctx.input` target THIS window: right for what the
//! window does to itself (collapse, maximise, drag, resize), wrong for waking
//! the others, which goes through [`wake`](super::server::wake) with the id of
//! each window.
//!
//! The Page Layout Mode of a detached graph stays as it was: the OS window
//! maximises and restores itself ([`super::layout_window::follow_mode_maximize`]).

use std::collections::HashMap;

use eframe::egui;
use stokesforge_app_state::state::Action;
use stokesforge_app_state::tr;
use stokesforge_core::NodeId;
use stokesforge_render_egui::GraphView;
use stokesforge_session::Snapshot;

use super::figure_copy::request_figure_copy;
use super::file_drop::{begin_file_drop, finish_file_drop};
use super::graph_window::graph_view_contents;
use super::object_manager_ui::selected_manager_target;
use super::server::{lock_app_from_viewport, wake};
use super::tab::{tab_title, tab_title_from_tree, Tab};
use super::window_chrome::{
    detached_resize_handles, paint_active_window_outline, show_window_contents_bounded,
    window_header, HeaderPlacement, WindowHeaderActions, MIN_CONTENT_WINDOW_SIZE,
    WINDOW_CONTENT_MARGIN, WINDOW_HEADER_HEIGHT,
};
use super::window_placement::{
    floating_window_display_size, viewport_room, FigureFollow, GRAPH_WINDOW_SIZE,
};
use super::StokesForgeApp;

/// The view state of a graph detached into its own OS window.
pub(super) struct DetachedGraph {
    /// The view (shape cache, zoom/pan), moved out of
    /// [`StokesForgeApp::graph_views`] for the duration of the detaching: it
    /// goes back there, the same one, on Reattach or Close.
    pub(super) view: GraphView,
    /// Collapse of the content, specific to this window: initialised from
    /// `collapsed_tabs` at the detaching and poured back on Reattach.
    pub(super) collapsed: bool,
    /// The first callback of a deferred viewport can arrive before Wayland/WGPU
    /// has stabilised its size: one more frame gives the figure its real
    /// rectangle, without sustaining a render loop.
    pub(super) needs_initial_repaint: bool,
    /// Last known OS size **outside collapse** (B9): the target of the resize
    /// sent on expanding.
    pub(super) expanded_size: Option<egui::Vec2>,
    /// The OS focus, as this window's own pass saw it.
    pub(super) focused: bool,
    /// The figure last painted, from which the OS window follows its size
    /// (#394).
    pub(super) figure_follow: HashMap<NodeId, FigureFollow>,
    /// The maximise the Page Layout Mode forced on the OS window.
    pub(super) mode_maximize: super::layout_window::ModeMaximize,
    /// "Reattach to the main window" was actuated; the root moves the window.
    pub(super) reattach: bool,
    /// The cross, or Close, was actuated; the root closes the window.
    pub(super) close: bool,
}

impl DetachedGraph {
    pub(super) fn new(view: GraphView, collapsed: bool) -> Self {
        Self {
            view,
            collapsed,
            needs_initial_repaint: true,
            expanded_size: None,
            focused: false,
            figure_follow: HashMap::new(),
            mode_maximize: None,
            reattach: false,
            close: false,
        }
    }

    /// The Page Layout Mode was toggled from outside the window (a menu of the
    /// root): the OS window has to maximise or restore itself at its next pass.
    pub(super) fn mode_differs(&self) -> bool {
        self.view.layout_mode() != self.mode_maximize.is_some()
    }
}

/// The viewport of the detached window of `graph`.
pub(super) fn graph_viewport(graph: NodeId) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(Tab::Graph(graph))
}

/// Frame state read before the clicks, then actions of the painted header.
struct Frame {
    was_collapsed: bool,
    is_active: bool,
    header_actions: WindowHeaderActions,
    inner_margin: egui::Margin,
}

impl StokesForgeApp {
    /// Publishes the viewport of each detached graph (queued for after the lock
    /// is released) and wakes the ones whose OS window must follow the mode.
    pub(super) fn show_detached_graphs(&mut self, ctx: &egui::Context) {
        let ids: Vec<NodeId> = self.detached_graphs.keys().copied().collect();
        for graph in ids {
            let Some(window) = self.detached_graphs.get(&graph) else {
                continue;
            };
            let changed = window.mode_differs();
            let title = tr!(
                "window-title-detached",
                name = tab_title(&self.state, Tab::Graph(graph))
            );
            // Starts at a size that contains the reference figure and its
            // return to the initial framing.
            let size = [GRAPH_WINDOW_SIZE.x, GRAPH_WINDOW_SIZE.y];
            // Without native decoration: the home-made bar (`window_header`,
            // placement `Detached`) is all the user has to move, resize and close
            // this window. Neither position nor size remembered (issue #16).
            let viewport = crate::app_logo::viewport()
                .with_title(title)
                .with_decorations(false)
                .with_inner_size(size)
                // The B9 collapse must be able to reduce the height below the
                // usual minimum.
                .with_min_inner_size([MIN_CONTENT_WINDOW_SIZE.x, WINDOW_HEADER_HEIGHT]);
            let shared = self.shared_self.clone();
            Self::publish_viewport(
                &mut self.pending_viewports,
                ctx,
                graph_viewport(graph),
                viewport,
                changed,
                move |ui, _class| {
                    // The root released its guard before this window was
                    // published, so it can take it.
                    let Some(shared) = shared.upgrade() else {
                        return;
                    };
                    let mut app = lock_app_from_viewport(&shared);
                    app.detached_graph_pass(ui, graph, size);
                },
            );
        }
    }

    /// The flags a detached graph window raised since the last root pass:
    /// Reattach and Close go through the root (L4 moves them to the window).
    pub(super) fn drain_detached_graph_flags(&mut self, ctx: &egui::Context) {
        let mut to_reattach = Vec::new();
        let mut to_close = Vec::new();
        for (graph, window) in &mut self.detached_graphs {
            if std::mem::take(&mut window.reattach) {
                to_reattach.push(*graph);
            }
            if std::mem::take(&mut window.close) {
                to_close.push(*graph);
            }
        }
        for graph in to_reattach {
            super::window_band::clear(ctx, graph_viewport(graph));
            self.reattach_tab(Tab::Graph(graph));
        }
        for graph in to_close {
            super::window_band::clear(ctx, graph_viewport(graph));
            self.close_detached_tab(Tab::Graph(graph));
        }
    }

    /// One pass of the detached window of `graph`, under the application lock:
    /// keys, paint, then its own actions applied at once.
    fn detached_graph_pass(&mut self, ui: &mut egui::Ui, graph: NodeId, size: [f32; 2]) {
        let ctx = ui.ctx().clone();
        let viewport = ctx.viewport_id();
        super::window_band::stop_keys(&ctx);
        super::shortcuts::refuse_quit_in_detached(&ctx);
        begin_file_drop(&ctx);
        if self.detached_graphs.contains_key(&graph) {
            let mut prelude = Vec::new();
            // What every pass may need before it acts (the file picker): a Save
            // As started here is answered here.
            self.pass_prelude(&ctx, &mut prelude);
            let mut own = Vec::new();
            let woke = self.paint_detached_graph(ui, graph, size, &mut own);
            self.apply_pass_actions(&ctx, viewport, prelude, own, woke);
        }
        super::page_layout::note_field_focus(&ctx);
        super::window_band::paint(&ctx);
    }

    /// Applies at once what a pass of the window produced: `prelude` (an
    /// answer of the file picker) and `own` (the gestures of the window, keys
    /// included), in the order of the root: the dialog gate and the origin of
    /// what the window says are the root's own function (#471), then the
    /// dispatch, the band of an opened dialog and the offer of a copied figure.
    /// Then a write, a selection moved or a flag raised (`woke`) wakes the root
    /// and the other open windows, never this one: it is painting.
    pub(super) fn apply_pass_actions(
        &mut self,
        ctx: &egui::Context,
        viewport: egui::ViewportId,
        mut prelude: Vec<Action>,
        own: Vec<Action>,
        woke: bool,
    ) {
        if self.state.blocking_dialog() {
            // As in the root: only the dialog can act.
            prelude.clear();
            self.action_origins.clear();
        }
        let wrote = !own.is_empty() || !prelude.is_empty();
        self.accept_detached_actions(ctx, viewport, own, &mut prelude);
        self.dispatch_actions(ctx, prelude);
        self.announce_opened_dialog(ctx);
        self.offer_figure();
        wake(ctx, woke || wrote, &self.open_viewports(), Some(viewport));
    }

    /// Paints the window and collects the actions of its gestures into `own`.
    /// Returns whether the root must be woken for something that is not an action.
    fn paint_detached_graph(
        &mut self,
        ui: &mut egui::Ui,
        graph: NodeId,
        size: [f32; 2],
        own: &mut Vec<Action>,
    ) -> bool {
        let snapshot = self.state.snapshot().clone();
        let Some(window) = self.detached_graphs.get_mut(&graph) else {
            return false;
        };
        let was_collapsed = window.collapsed;
        // B9: the "logical" OS size (outside collapse) is remembered by
        // ourselves; read before any handling of this frame's click.
        if !was_collapsed {
            if let Some(rect) = ui.ctx().input(|input| input.viewport().outer_rect) {
                window.expanded_size = Some(rect.size());
            }
        }
        // Highlight: a detached window is a separate OS viewport, so "active"
        // is the OS focus of this very viewport.
        let is_active = ui.input(|input| input.focused);
        let mut wake_others = false;
        if window.focused != is_active {
            window.focused = is_active;
            wake_others = true;
        }
        // Read before anything is painted (#459): a cell, a field or the Page
        // Layout Mode never sees these keys. Queued after the window's own
        // actions, below.
        let mut keys = super::shortcuts::read_detached_history_and_save(ui.ctx());
        // Page Layout Mode (#426): the keys come first, before anything is
        // painted, and the OS window follows the mode.
        super::page_layout::handle_keys(ui.ctx(), &mut window.view, graph, own);
        let in_mode = window.view.layout_mode();
        super::layout_window::follow_mode_maximize(ui.ctx(), &mut window.mode_maximize, in_mode);
        let (header_actions, frame, selection_moved) =
            paint_frame(ui, window, &snapshot, graph, own, was_collapsed);
        wake_others |= selection_moved;
        // Undo, Redo and Save, read before the painting (#459), come after the
        // gestures of this frame: Ctrl+Z pressed in the frame a drag is released
        // undoes that drag (#398).
        own.append(&mut keys);
        if let Some(action) = finish_file_drop(ui.ctx()) {
            own.push(action);
        }
        apply_window_actions(
            ui,
            window,
            graph,
            size,
            own,
            Frame {
                was_collapsed,
                is_active,
                header_actions,
                inner_margin: frame.inner_margin,
            },
        );
        if std::mem::take(&mut window.needs_initial_repaint) {
            // `show_viewport_deferred` creates the window after the root frame.
            // Under Wayland, the first callback may see a surface still without
            // a final size; redrawing once rebuilds the figure with the
            // rectangle configured by WGPU. This pass's own viewport.
            ui.ctx().request_repaint();
        }
        if ui.ctx().input(|input| input.viewport().close_requested()) {
            window.close = true;
        }
        wake_others || window.reattach || window.close
    }
}

/// The frame of the window: header and content. Returns the header actions,
/// the frame, and whether the selection of the view moved (the Object Manager
/// follows it).
fn paint_frame(
    ui: &mut egui::Ui,
    window: &mut DetachedGraph,
    snapshot: &Snapshot,
    graph: NodeId,
    own: &mut Vec<Action>,
    was_collapsed: bool,
) -> (WindowHeaderActions, egui::Frame, bool) {
    let id = egui::Id::new(Tab::Graph(graph));
    let mut header_actions = WindowHeaderActions::default();
    let mut selection_moved = false;
    // Default stroke (inactive) only: the active outline is painted separately,
    // after `.show()`, on the rectangle of the real OS viewport (B7).
    let frame = egui::Frame::central_panel(ui.style())
        .inner_margin(WINDOW_CONTENT_MARGIN)
        .stroke(ui.style().visuals.window_stroke());
    egui::CentralPanel::default().frame(frame).show(ui, |ui| {
        header_actions = window_header(
            ui,
            id,
            &tab_title_from_tree(&snapshot.tree, Tab::Graph(graph)),
            Some(Tab::Graph(graph)),
            was_collapsed,
            HeaderPlacement::Detached,
        );
        if was_collapsed ^ header_actions.toggle_collapse {
            return;
        }
        // The content may overflow, but it must modify neither the Frame nor
        // the viewport's clip rectangle.
        show_window_contents_bounded(ui, |ui| {
            let before = selected_manager_target(&window.view);
            // Ctrl+C in this viewport: the figure of **this** graph (#149).
            request_figure_copy(ui.ctx(), &window.view, &snapshot.tree, graph, own);
            graph_view_contents(ui, &mut window.view, snapshot, graph, own);
            follow_figure_in_viewport(
                ui.ctx(),
                &mut window.figure_follow,
                graph,
                window.view.display_scale(),
                window.mode_maximize.is_some(),
            );
            selection_moved = before != selected_manager_target(&window.view);
        });
    });
    (header_actions, frame, selection_moved)
}

/// The window commands and the outline: collapse, resize handles, maximise,
/// drag, Reattach, Close. Everything addressed to the OS window targets this
/// pass's own viewport.
fn apply_window_actions(
    ui: &mut egui::Ui,
    window: &mut DetachedGraph,
    graph: NodeId,
    size: [f32; 2],
    own: &mut Vec<Action>,
    frame: Frame,
) {
    let Frame {
        was_collapsed,
        is_active,
        header_actions,
        inner_margin,
    } = frame;
    // Collapse state that applies from this frame.
    let collapsed_now = was_collapsed ^ header_actions.toggle_collapse;
    // The drop target's border replaces the active outline (#386): the marker
    // is the layer of the content (`CentralPanel`), read after it.
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
        window.collapsed = collapsed_now;
    }
    // B9: these drive the real OS viewport through `BeginResize`; leaving them
    // active during a collapse would let the user enlarge the collapsed window.
    if !collapsed_now {
        detached_resize_handles(ui);
    }
    // B9: on collapse, the remembered width and the header height; on expand,
    // the remembered size as is. Sent only on the frame of the toggle.
    if header_actions.toggle_collapse {
        let expanded_size = window
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
        // Restore while in the Page Layout Mode leaves it, as the Restore of a
        // floating window does, through the same path as Esc. A restore done
        // from outside the application (an OS shortcut) cannot be told from a
        // maximise the compositor has not applied yet, so it leaves the mode
        // alone.
        if is_maximized && window.view.layout_mode() {
            super::page_layout::leave_mode(&mut window.view, graph, own);
        }
    }
    if header_actions.detach {
        window.reattach = true;
    }
    if header_actions.close {
        window.close = true;
    }
}

/// The detached window follows the figure (#394), like a floating one: when the
/// figure's logical size changed since the last frame, the inner size of the OS
/// window grows or shrinks by the difference at the display scale, clamped to
/// the monitor. A maximized window only records the figure.
fn follow_figure_in_viewport(
    ctx: &egui::Context,
    follows: &mut HashMap<NodeId, FigureFollow>,
    graph: NodeId,
    scale: Option<f32>,
    in_layout_mode: bool,
) {
    let Some(size) = super::graph_window::published_figure_size(ctx, graph) else {
        return;
    };
    let origin = super::graph_window::published_figure_origin(ctx, graph);
    let Some(step) = FigureFollow::observe(follows, graph, size, origin, scale) else {
        return;
    };
    // In the Page Layout Mode the window is maximised (or about to be): the
    // figure is recorded, the window keeps its size.
    if in_layout_mode {
        return;
    }
    let (inner, maximized, monitor, outer) = ctx.input(|input| {
        let viewport = input.viewport();
        (
            viewport.inner_rect.map(|rect| rect.size()),
            viewport.maximized.unwrap_or(false),
            viewport.monitor_size,
            viewport.outer_rect,
        )
    });
    let Some(inner) = inner.filter(|_| !maximized) else {
        return;
    };
    // Free placement (#398): the image's origin moved, and the window moves by
    // that at the kept scale so that page coordinates stay still on screen,
    // as a floating window does. Without the outer rectangle (a native Wayland
    // client cannot know nor set where its window is) nothing is sent.
    let free_shift = FigureFollow::origin_follow(step);
    if let (Some(outer), true) = (outer, free_shift != egui::Vec2::ZERO) {
        let corner = FigureFollow::shifted_corner(follows, graph, outer.min, free_shift);
        // Kept on the monitor when the window was on it.
        let corner = match monitor {
            Some(monitor) => {
                let limit = (monitor - outer.size()).max(egui::Vec2::ZERO);
                egui::pos2(
                    if outer.min.x <= limit.x {
                        corner.x.min(limit.x)
                    } else {
                        corner.x
                    },
                    if outer.min.y <= limit.y {
                        corner.y.min(limit.y)
                    } else {
                        corner.y
                    },
                )
            }
            None => corner,
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(corner));
    }
    let Some(area) = super::graph_window::published_figure_area(ctx, graph) else {
        return;
    };
    let chrome = (inner - area).max(egui::Vec2::ZERO);
    let target = FigureFollow::resize(follows, graph, step, inner, chrome, viewport_room(monitor));
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(target));
}

#[cfg(test)]
mod tests {
    //! The detached graph serves itself (#475, L1): its actions apply in its own
    //! pass, Reattach gives its view back, Ctrl+C offers the figure from its pass,
    //! and a write wakes the root and the other open windows.
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::super::test_support::bench::{delay, Bench, Pass, Window};
    use super::*;
    use stokesforge_clipboard::{ClipboardError, Representation};

    fn ctrl(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }
    }

    /// T3: Reattach gives back the very view that was detached, with what it
    /// holds (its identity, its selection), and detaching again moves that same
    /// view to the window. Witness: build a new `GraphView` on reattach.
    #[test]
    fn a_reattached_graph_keeps_its_view() {
        let mut bench = Bench::new();
        bench.start();
        let graph = bench.graph;
        let tab = Tab::Graph(graph);
        let spec = bench.app.state.plot_spec(graph).unwrap();
        let target = stokesforge_plot::SeriesRef::new(0, 0);
        bench
            .app
            .with_graph_view(graph, |view| view.select_series(Some(target), &spec));
        let identity = bench.app.detached_graphs[&graph].view.id();
        // The "↩" button raises the flag; the root moves the window.
        bench.app.detached_graphs.get_mut(&graph).unwrap().reattach = true;
        bench.root(Pass::root());
        assert!(!bench.app.is_tab_detached(tab));
        assert!(
            bench.app.floating_tabs.contains(&tab),
            "the tab floats again"
        );
        let view = &bench.app.graph_views[&graph];
        assert_eq!(view.id(), identity, "the same view came back");
        assert_eq!(view.selected_series(), Some(target));
        bench.app.detach_tab(tab);
        assert_eq!(bench.app.detached_graphs[&graph].view.id(), identity);
        bench.finish();
    }

    /// A fake offer to the system clipboard.
    struct Fake(Arc<Mutex<Vec<Vec<&'static str>>>>);

    impl super::super::figure_copy::FigureOffer for Fake {
        fn offer(&self, representations: &[Representation]) -> Result<(), ClipboardError> {
            self.0
                .lock()
                .unwrap()
                .push(representations.iter().map(|rep| rep.mime).collect());
            Ok(())
        }
    }

    /// T8: Ctrl+C in the detached graph's pass alone prepares the figure, and
    /// that same pass offers it to the clipboard (the display was captured by
    /// the root, a window's pass has no `Frame`). Witness: offer at the root
    /// only (`offer_figure` out of the window's pass).
    #[test]
    fn ctrl_c_in_the_detached_graph_alone_offers_the_figure() {
        let mut bench = Bench::new();
        bench.start();
        let offered = Arc::new(Mutex::new(Vec::new()));
        bench
            .app
            .set_figure_offer(Box::new(Fake(Arc::clone(&offered))));
        // Every pass so far was before the gesture; no root pass follows.
        bench.child(Pass::child().events(vec![egui::Event::Copy]));
        let offers = offered.lock().unwrap().clone();
        assert_eq!(
            offers.len(),
            1,
            "one offer, from the window's pass: {offers:?}"
        );
        assert!(offers[0].contains(&"image/svg+xml") && offers[0].contains(&"image/png"));
        assert!(
            bench.app.state.take_figure_clipboard().is_none(),
            "the clipboard was emptied by the offer"
        );
        bench.finish();
    }

    /// T12: a write made in the graph's pass wakes the root and every OTHER open
    /// window and never a closed one (that the painting window is left out is
    /// the unit test of `wake` in `server.rs`). Witness: wake the root only; or
    /// wake the ids of the windows that ever were open.
    #[test]
    fn a_write_in_the_graph_wakes_the_root_and_the_other_open_windows() {
        let mut bench = Bench::with_every_window();
        bench.start_every_window();
        let settle = |bench: &mut Bench| {
            for _ in 0..8 {
                bench.root(Pass::root());
                for window in Window::ALL {
                    bench.child_in(bench.id_of(window), Pass::child().focused(false));
                }
            }
        };
        settle(&mut bench);
        // The workbook window is closed: it must not be woken any more.
        let book = Tab::Book(bench.book());
        let closed = bench.id_of(Window::Book);
        bench.app.close_detached_tab(book);
        settle_without(&mut bench, closed);
        bench.app.state.dispatch(Action::AddWorkbook);
        settle_without(&mut bench, closed);
        let output = bench.child_in(
            bench.id_of(Window::Graph),
            Pass::child().events(vec![ctrl(egui::Key::Z)]),
        );
        for id in [
            egui::ViewportId::ROOT,
            bench.id_of(Window::ProjectManager),
            bench.id_of(Window::ObjectManager),
            bench.id_of(Window::Console),
        ] {
            assert_eq!(delay(&output, id), Duration::ZERO, "{id:?} must be woken");
        }
        assert!(
            output
                .viewport_output
                .get(&closed)
                .is_none_or(|viewport| viewport.repaint_delay != Duration::ZERO),
            "a closed window is never woken"
        );
        bench.finish();

        fn settle_without(bench: &mut Bench, closed: egui::ViewportId) {
            for _ in 0..8 {
                bench.root(Pass::root());
                for window in Window::ALL {
                    let id = bench.id_of(window);
                    if id != closed {
                        bench.child_in(id, Pass::child().focused(false));
                    }
                }
            }
        }
    }
}
