//! Stack of content windows (designated by the [`Tab`] of `tab.rs`):
//! the stack order `floating_tabs` (the last one is the active tab), its
//! alignment with egui's layers, hiding, collapsing, maximizing
//! and the drawing of the workspace (`show_workspace`).
//!
//! A click received by a window explicitly resumes the Project
//! Manager follow (#154), even if that window was already active. Passive
//! following keeps multi-selections for the panel's grouped actions; only
//! the gesture on the content replaces them, after checking the touched layer.
//!
//! Moved as is from `app.rs` (#101), then split up in place (#124):
//! maximizing, scrolling and painting of a window share only
//! the frame's disjoint borrows, without adding sub-Uis.
//!
//! # Taken from the top doc of `app.rs`
//!
//! # Confined floating windows
//!
//! The four window kinds above open by default as floating windows
//! inside the [`egui::CentralPanel`], rather than docked as full-frame
//! tiles — one per [`StokesForgeApp::floating_tabs`], never
//! duplicated (same invariant as the old dock: [`StokesForgeApp::open_tab`]
//! decides "open" vs "focus"). Neither the Project Manager nor the script
//! console takes part in it, see the two previous sections: they are
//! fixed panels painted *before* the `CentralPanel`.
//!
//! **One `egui::Area` per window, not an `egui::Window`**: see the doc of
//! [`show_floating_window`]. `egui::Window` is a dialog box; its
//! functions (title bar, drag, resize, clamping) had
//! all been cut and redone here, and each frame had to reconcile the
//! requested rectangle with the one egui readjusted — the common source of
//! the outline and overflow bugs of the 2026-09-23 and 2026-09-24
//! reports. Now this module alone owns the geometry: frame, outline,
//! clipping, handles and persisted geometry come from the same rectangle.
//!
//! **Scrollable workspace**: the area that carries the floating
//! windows has a **fixed** logical size (`WORKSPACE_SIZE`), independent
//! of the `eframe` window — before this task, that area was the `ui.max_rect()`
//! of the `CentralPanel`, so it shrank with the main window and
//! crushed the floating windows against its edges. `window_positions`/
//! `window_sizes` (below) and the geometry persisted in the project are
//! expressed in these logical coordinates; [`StokesForgeApp::ui`](eframe::App::ui) converts them
//! to screen coordinates just before painting (`workspace_to_screen`), from
//! `viewport_rect` (what the fixed panels and the scroll bars leave) and from
//! [`StokesForgeApp::workspace_scroll`] (0,0 at startup). Below
//! `WORKSPACE_SIZE`, `viewport_rect` thus shows only a portion of the
//! workspace — an invisible `egui::ScrollArea` (nothing painted in its
//! content, only reserved to `WORKSPACE_SIZE`) gives scroll handles/wheel
//! for free and feeds `workspace_scroll`; the floating windows each paint
//! on their own layer and cannot live inside that scroll area, hence the
//! manual conversion.
//!
//! **Confinement**: each window's logical position is clamped to the
//! workspace by [`clamp_to_workspace`], and rewritten as is —
//! that is the only clamp. The rendering, in turn, is clipped to `viewport_rect` by
//! [`show_floating_window`], for the drawing **and** for the clicks: a
//! window can thus neither paint over the Project Manager, the console
//! or the scroll bars, nor capture a click there, even straddling the
//! edge of the view. Entirely outside the visible portion (the user
//! scrolled elsewhere), it is not painted at all that frame; scrolling back
//! makes it reappear as is, at the same logical position.
//!
//! **Cascade**: a newly opened window (not a mere refocus) gets a default
//! position offset from the previous one
//! ([`StokesForgeApp::next_cascade`], reset to zero every eight
//! tabs so as not to drift out of `WORKSPACE_SIZE`) — the user
//! immediately sees that a new window appeared rather than finding it
//! exactly stacked on the previous one.
//!
//! **Refocusing a window scrolled out of view**: reopening from the Project
//! Manager an entry that is already open (§ z-order, just below) also adjusts
//! `workspace_scroll` if needed (`workspace_scroll_to_reveal`), so that the
//! targeted window becomes visible again rather than just rising in a
//! layer one cannot see.
//!
//! **Z-order**: a window's `egui::Area` already rises to the front on its
//! own on click (`Sense::CLICK` by default) — but in its own layer order,
//! independent of [`StokesForgeApp::floating_tabs`], which stays the only
//! source of `active_tab` (thus of the highlight outline). Without
//! synchronization, a direct click in a window brought it to the
//! front without ever highlighting it (B2,
//! `retours-2026-09-23-bugs.md`). [`StokesForgeApp::ui`](eframe::App::ui) thus rereads
//! `Context::top_layer_id` right after painting the floating windows
//! and moves the tab found to the end of `floating_tabs` through
//! `sync_floating_tabs_order_to_top_layer` — one frame behind at most,
//! `egui` itself asking for an immediate repaint on click. The only case
//! this module still has to handle *before* `egui` has painted anything
//! is "reopening from the Project Manager an entry that is already open must
//! bring it to the front without it being clicked":
//! [`StokesForgeApp::open_tab`] then sets [`StokesForgeApp::focus_next`], consumed at the
//! next render by a call to `Context::move_to_top` on the layer
//! ([`FLOATING_WINDOW_ORDER`]) of the window concerned. Until the switch to
//! `egui::Area`, that call targeted `Order::Foreground` while the
//! `egui::Window`s lived on `Order::Middle`: it raised the outline's
//! layer, not the window itself.
//!
//! # Maximizing a tab
//!
//! [`StokesForgeApp::maximized_tab`] is a state **beside**
//! [`StokesForgeApp::floating_tabs`], which it never touches: when it is
//! `Some`, [`StokesForgeApp::ui`](eframe::App::ui) paints no floating window that
//! frame — instead, a separate area shows only the maximized tab, filling
//! the `CentralPanel`. Restoring (`maximized_tab =
//! None`) goes back to painting the floating windows normally: their
//! positions/sizes (kept by `egui` through their `Id`, not by this module)
//! never moved during the maximization, so "restoring" is
//! exactly "as before", not a recomputation.
//!
//! The Project Manager never has a value in `maximized_tab`: it is
//! not a floating window, and a fixed panel that is always entirely
//! visible has nothing to gain from being "maximized" anyway. It stays
//! visible with nothing special while *another* tab is maximized,
//! since [`StokesForgeApp::project_manager_panel`] is painted before the
//! maximization area just as before the `CentralPanel` — a separate fixed
//! panel, never duplicated, needs no special case here.
//!

use std::collections::{HashMap, HashSet};

use super::project_manager::PmEntry;

use eframe::egui;
use stokesforge_core::NodeId;
use stokesforge_render_egui::GraphView;

use stokesforge_app_state::state::{Action, AppState, WindowKind};
use stokesforge_app_state::tr;

use super::contents::tab_contents;
use super::tab::tab_title;
use super::tab::{tab_node, Tab};
use super::window_chrome::{
    floating_resize_handles, floating_window_frame, show_floating_window,
    show_window_contents_bounded, window_header, HeaderPlacement, WindowHeaderActions,
    FLOATING_WINDOW_ORDER, MIN_CONTENT_WINDOW_SIZE, WINDOW_HEADER_HEIGHT,
};
use super::window_placement::{
    clamp_to_workspace, floating_window_display_size, floating_window_geometry_to_persist,
    followed_floating_size, geometry_rect, workspace_scroll_to_reveal, workspace_to_screen,
    FigureFollow, BOOK_WINDOW_WIDTH, CASCADE_STEP, CASCADE_WRAP, FLOATING_WINDOW_SIZE,
    GRAPH_WINDOW_SIZE, WORKSPACE_SIZE,
};
use super::StokesForgeApp;

/// A click reactivates the content only on its layer: a floating Object
/// Manager can cover it while keeping another graph as the target.
pub(super) fn window_pointer_pressed(
    ctx: &egui::Context,
    layer: egui::LayerId,
    rect: egui::Rect,
) -> bool {
    ctx.input(|input| input.pointer.any_pressed()) && ctx.rect_contains_pointer(layer, rect)
}

/// The windows of the Project Manager tree, flattened.
pub(super) fn window_nodes(
    tree: &[stokesforge_app_state::state::ProjectTreeNode],
) -> Vec<(NodeId, WindowKind)> {
    let mut out = Vec::new();
    for node in tree {
        match node {
            stokesforge_app_state::state::ProjectTreeNode::Folder { children, .. } => {
                out.extend(window_nodes(children));
            }
            stokesforge_app_state::state::ProjectTreeNode::Window { id, kind, .. } => {
                out.push((*id, *kind))
            }
        }
    }
    out
}

/// Toggles the collapse of a tab's content. Reserved for `Tab::Graph` and
/// `Tab::Book` (B5, `retours-2026-09-23-bugs.md`) — `Matrix`
/// and `Note` do not have this control and so always stay shown; nothing
/// in the painting of the collapsed content (see the loop over floating
/// and detached windows in `StokesForgeApp::ui`) depends on the type of `tab`,
/// so extending to these two kinds is only a matter of a visible icon,
/// not of hidden behavior to recheck.
pub(super) fn toggle_collapse(collapsed_tabs: &mut HashSet<Tab>, tab: Tab) {
    if !matches!(tab, Tab::Graph(_) | Tab::Book(_)) {
        return;
    }
    if !collapsed_tabs.insert(tab) {
        collapsed_tabs.remove(&tab);
    }
}

/// Sets the collapse to a precise value rather than toggling it — used
/// by [`StokesForgeApp::reattach_tab`] to pour back into
/// `collapsed_tabs` the state proper to the detached window
/// (`DetachedTabViewModel::collapsed`) rather than guess it.
pub(super) fn toggle_collapse_to(collapsed_tabs: &mut HashSet<Tab>, tab: Tab, collapsed: bool) {
    if !matches!(tab, Tab::Graph(_) | Tab::Book(_)) {
        return;
    }
    if collapsed {
        collapsed_tabs.insert(tab);
    } else {
        collapsed_tabs.remove(&tab);
    }
}

/// Brings `top_layer_tab` to the end of `floating_tabs` (§ stack order,
/// `active_tab = floating_tabs.last()`), doing nothing if it is already there,
/// if it does not exist, or if no floating window is in the foreground
/// (B2, `retours-2026-09-23-bugs.md`). `egui` manages its own
/// layer order alone (a direct click in a window already brings it to the
/// foreground visually); this function only realigns
/// `floating_tabs`, the only source of `active_tab`, with it.
pub(super) fn sync_floating_tabs_order_to_top_layer(
    floating_tabs: &mut Vec<Tab>,
    top_layer_tab: Option<Tab>,
) {
    let Some(top_layer_tab) = top_layer_tab else {
        return;
    };
    if let Some(pos) = floating_tabs.iter().position(|&t| t == top_layer_tab) {
        if pos + 1 != floating_tabs.len() {
            let tab = floating_tabs.remove(pos);
            floating_tabs.push(tab);
        }
    }
}

/// Disjoint borrows limited to one paint: neither new state nor new Ui.
/// Extracting each window cannot take `&mut StokesForgeApp`
/// while the contents already borrow its AppState and its GraphView.
struct Workspace<'a> {
    state: &'a mut AppState,
    floating_tabs: &'a mut Vec<Tab>,
    cascade_rank: &'a mut HashMap<Tab, u32>,
    next_cascade: &'a mut u32,
    focus_next: &'a mut Option<Tab>,
    active_detached: &'a mut Option<Tab>,
    pm_selection: &'a mut HashSet<PmEntry>,
    graph_views: &'a mut HashMap<NodeId, GraphView>,
    collapsed_tabs: &'a mut HashSet<Tab>,
    maximized_tab: &'a mut Option<Tab>,
    window_positions: &'a mut HashMap<Tab, egui::Pos2>,
    window_sizes: &'a mut HashMap<Tab, egui::Vec2>,
    figure_follow: &'a mut HashMap<NodeId, FigureFollow>,
    workspace_scroll: &'a mut egui::Vec2,
    floating_rects: &'a mut HashMap<NodeId, egui::Rect>,
    actions: &'a mut Vec<Action>,
    detach_requests: &'a mut Vec<Tab>,
    close_requests: &'a mut Vec<Tab>,
    geometry_updates: &'a mut Vec<(Tab, egui::Rect)>,
}

impl Workspace<'_> {
    fn activate_clicked_tab(&mut self, ctx: &egui::Context, tab: Tab) {
        *self.active_detached = None;
        self.pm_selection.clear();
        self.pm_selection.insert(PmEntry::Window(tab));
        super::project_manager::follow_selection_anchor(ctx, Some(tab));
        // The panel and its detached viewport have already been published this frame.
        ctx.request_repaint();
    }

    fn show_maximized(&mut self, ui: &mut egui::Ui, tab: Tab) {
        // § maximize (top doc of the module): no floating
        // window is painted at all this frame. The Project
        // Manager needs no special handling here: its
        // fixed panel is painted above, before this `CentralPanel`,
        // whether another tab is maximized or not.
        ui.horizontal(|ui| {
            ui.heading(tab_title(self.state, tab));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(tr!(
                        "action-restore-window",
                        title = tab_title(self.state, tab)
                    ))
                    .on_hover_text(tr!("action-restore-window", "hint"))
                    .clicked()
                {
                    *self.maximized_tab = None;
                }
            });
        });
        ui.separator();
        if window_pointer_pressed(ui.ctx(), ui.layer_id(), ui.max_rect()) {
            self.activate_clicked_tab(ui.ctx(), tab);
        }
        tab_contents(ui, self.state, self.actions, self.graph_views, tab);
        // A maximized window does not resize (the figure is contained), but it
        // keeps the record up to date so that restoring it resizes nothing.
        self.follow_figure(ui.ctx(), tab, None);
        // The maximized window has no frame: the drop target (#386) lights up
        // the maximized area itself.
        if super::file_drop::is_drop_target(ui.ctx(), ui.layer_id()) {
            ui.painter().rect_stroke(
                ui.max_rect(),
                0.0,
                crate::theme::drop_target_stroke(),
                egui::StrokeKind::Inside,
            );
        }
    }

    /// The window follows the figure (#394): when the logical size of the figure
    /// a graph window painted changed since the last frame, a floating window
    /// grows or shrinks by the difference at its display scale, top-left fixed,
    /// right and down, clamped to the visible part of the workspace (`visible`, the
    /// far corner; `None`: a maximized window only records). The first frame of a graph, after opening or loading,
    /// resizes nothing.
    fn follow_figure(&mut self, ctx: &egui::Context, tab: Tab, visible: Option<egui::Vec2>) {
        let Tab::Graph(graph) = tab else {
            return;
        };
        let Some(size) = super::graph_window::published_figure_size(ctx, graph) else {
            return;
        };
        let scale = self
            .graph_views
            .get(&graph)
            .and_then(GraphView::display_scale);
        let Some(delta) = FigureFollow::observe(self.figure_follow, graph, size, scale) else {
            return;
        };
        let Some(visible) = visible else {
            return;
        };
        if let (Some(&position), Some(window)) = (
            self.window_positions.get(&tab),
            self.window_sizes.get_mut(&tab),
        ) {
            *window = followed_floating_size(position, *window, delta, visible);
            ctx.request_repaint();
        }
    }

    fn show_floating(&mut self, ui: &mut egui::Ui, active_tab: Option<Tab>) {
        // What the fixed panels painted above (menu, tools,
        // status, script, Project Manager) leave — see the top
        // doc of the module, § Confined floating windows. The workspace
        // has a fixed size (`WORKSPACE_SIZE`), independent
        // of `viewport_rect`: below this size, it scrolls
        // (`self.workspace_scroll`) rather than crushing the windows
        // it carries against the edges of the `CentralPanel`.
        // `self.window_positions`/`self.window_sizes`, and the geometry
        // persisted in the project, are thus in logical
        // coordinates of this space — clamped to it by
        // `clamp_to_workspace`, then converted to screen coordinates
        // just before painting (`workspace_to_screen`). A
        // window thus never leaves the workspace; it
        // may leave its visible portion, in which case it is
        // clipped at the edge of the view, or not painted at all if it
        // leaves it entirely (`show_floating_window`).
        let viewport_rect = ui.max_rect();

        // A window already open but currently scrolled out of view
        // becomes visible again on the click that refocuses it (Project
        // Manager, double-click…) — read before this frame's scrolling
        // so that it takes effect right now, not on the
        // next one.
        if let Some(tab) = *self.focus_next {
            if let (Some(&position), Some(&size)) =
                (self.window_positions.get(&tab), self.window_sizes.get(&tab))
            {
                let target = egui::Rect::from_min_size(position, size);
                *self.workspace_scroll = workspace_scroll_to_reveal(
                    *self.workspace_scroll,
                    viewport_rect.size(),
                    target,
                );
            }
        }

        // An invisible `egui::ScrollArea` (nothing is painted in its
        // content, just reserved to `WORKSPACE_SIZE`) rather than a
        // home-made scroll: it gives drag handles, wheel
        // and clamping for free, in the common style of the scroll bars
        // (`theme::scroll_style`, set globally). The floating windows are
        // **not** painted inside its closure: each one
        // paints on its own layer (`show_floating_window`),
        // independent of any ambient scroll area — only the
        // resulting offset is used, applied by hand below.
        // RAW-SCROLL(#347): workspace canvas, not a page: no end margin under the last window
        let workspace_scroll_output = egui::ScrollArea::both()
            .id_salt("stokesforge_workspace")
            .auto_shrink([false, false])
            .scroll_offset(*self.workspace_scroll)
            .show(ui, |ui| {
                ui.allocate_exact_size(WORKSPACE_SIZE, egui::Sense::hover());
            });
        *self.workspace_scroll = workspace_scroll_output.state.offset;
        // What stays visible once the scroll bars are
        // placed: the floating windows are clipped to this
        // rectangle, so as never to cover these bars.
        let viewport_rect = workspace_scroll_output.inner_rect;

        for index in 0..self.floating_tabs.len() {
            // The stack is reordered only after its traversal, as before #124.
            let tab = self.floating_tabs[index];
            self.show_floating_tab(ui, tab, active_tab, viewport_rect);
        }
        let refocused = self.focus_next.take().is_some();

        // B2 (`retours-2026-09-23-bugs.md`):
        // `egui::Area` already brings the clicked window to the front
        // of its own layer order as soon as it is clicked
        // (`Sense::CLICK`, independently of the Project Manager) — but
        // `self.floating_tabs`, the only source of `active_tab` (thus of
        // `is_active`/the highlight, see above), does not know
        // it: a direct click brought the window to the front
        // without ever highlighting it. We resynchronize here on what
        // egui has just established, once mutating
        // `self.floating_tabs` is safe again (the loop that iterates it is
        // closed).
        //
        // A reopening from the Project Manager has just requested
        // `move_to_top`: egui reflects it in `top_layer_id` only on the next
        // frame. Resynchronizing now would put the old window back at the
        // end of the stack (and the Project Manager selection with it); so we
        // wait for the next frame, requested here.
        if refocused {
            ui.ctx().request_repaint();
            return;
        }
        let top_layer_tab = ui.ctx().top_layer_id().and_then(|top| {
            self.floating_tabs
                .iter()
                .copied()
                .find(|&tab| egui::Id::new(tab) == top.id)
        });
        sync_floating_tabs_order_to_top_layer(self.floating_tabs, top_layer_tab);
    }

    fn show_floating_tab(
        &mut self,
        ui: &mut egui::Ui,
        tab: Tab,
        active_tab: Option<Tab>,
        viewport_rect: egui::Rect,
    ) {
        let id = egui::Id::new(tab);
        if *self.focus_next == Some(tab) {
            ui.ctx()
                .move_to_top(egui::LayerId::new(FLOATING_WINDOW_ORDER, id));
        }
        let rank = self.cascade_rank.entry(tab).or_insert_with(|| {
            let rank = *self.next_cascade;
            *self.next_cascade = self.next_cascade.wrapping_add(1);
            rank
        });
        let cascade = CASCADE_STEP * (*rank % CASCADE_WRAP) as f32;
        // Logical coordinates of the workspace, not screen
        // coordinates — see the doc just above.
        let default_pos = egui::pos2(cascade, cascade);
        let natural_size = match tab {
            Tab::Book(_) => egui::vec2(BOOK_WINDOW_WIDTH, FLOATING_WINDOW_SIZE.y),
            Tab::Graph(_) => GRAPH_WINDOW_SIZE,
            _ => FLOATING_WINDOW_SIZE,
        };
        let default_size = natural_size.min(WORKSPACE_SIZE);
        // The last floating rectangle known to the application
        // takes precedence over the snapshot (stale after a move).
        let saved_rect = self
            .floating_rects
            .get(&tab_node(tab))
            .copied()
            .or_else(|| {
                self.state
                    .snapshot()
                    .window_geometries
                    .get(&tab_node(tab))
                    .map(geometry_rect)
            });
        let saved_pos = saved_rect.map_or(default_pos, |rect| rect.min);
        let saved_size = saved_rect
            .map_or(default_size, |rect| rect.size())
            .max(MIN_CONTENT_WINDOW_SIZE)
            .min(WORKSPACE_SIZE);
        self.window_positions.entry(tab).or_insert(saved_pos);
        let current_size = *self.window_sizes.entry(tab).or_insert(saved_size);
        // Only source of truth for the position: clamped here, to
        // the workspace, and rewritten as is. egui no longer
        // readjusts anything on its side (no more `constrain_to` on
        // the workspace), so the painted rectangle, the
        // outline, the handles and the persisted geometry
        // all come from the same rectangle — it is the gap between
        // a requested rectangle and a rectangle readjusted by egui
        // that produced bug 1 of
        // `retours-2026-09-24-bugs.md`.
        let logical_rect = clamp_to_workspace(egui::Rect::from_min_size(
            self.window_positions[&tab],
            current_size,
        ));
        self.window_positions.insert(tab, logical_rect.min);
        let mut header_actions = WindowHeaderActions::default();
        let was_collapsed = self.collapsed_tabs.contains(&tab);
        let is_active = Some(tab) == active_tab;
        // Collapsed window (B3): `window_header` paints only its
        // header band, `tab_contents` is skipped — painting the
        // window at `current_size` would draw a frame (and an
        // outline) at the expanded size around an empty area.
        // `current_size` stays the logical size kept in
        // `self.window_sizes`/persisted; only the rectangle painted
        // this frame shrinks.
        let frame = floating_window_frame(ui.style());
        let display_size = floating_window_display_size(
            current_size,
            was_collapsed,
            WINDOW_HEADER_HEIGHT,
            frame.inner_margin.sum(),
        );
        let window_rect = egui::Rect::from_min_size(
            workspace_to_screen(logical_rect.min, viewport_rect.min, *self.workspace_scroll),
            display_size,
        );
        let title = tab_title(self.state, tab);
        // Outside the visible portion: `None`, neither painted nor
        // interactive — "hidden", not crushed. Partially
        // visible: clipped at the edge of `viewport_rect`, for the
        // drawing as for the clicks (bug 2 of the same report).
        let shown = show_floating_window(
            ui.ctx(),
            id,
            window_rect,
            viewport_rect,
            is_active,
            |ui| {
                header_actions = window_header(
                    ui,
                    id,
                    &title,
                    Some(tab),
                    was_collapsed,
                    HeaderPlacement::Floating,
                );
                if !(was_collapsed ^ header_actions.toggle_collapse) {
                    // The bounded sub-Ui keeps the content from
                    // overflowing the window. Only the grid
                    // gets horizontal scrolling (F8).
                    show_window_contents_bounded(ui, |ui| {
                        tab_contents(ui, self.state, self.actions, self.graph_views, tab);
                    });
                }
            },
            |ui, rect| {
                // Home-made handles (§3): mutate `self.window_positions`/
                // `self.window_sizes` directly; the next frame
                // clamps then paints them.
                if let (Some(position), Some(size)) = (
                    self.window_positions.get_mut(&tab),
                    self.window_sizes.get_mut(&tab),
                ) {
                    floating_resize_handles(ui, tab, rect, position, size);
                }
            },
        );
        if shown.is_none() {
            return;
        }
        if !was_collapsed {
            // Far corner of the visible part of the workspace, in its own
            // coordinates: the window never grows past it.
            let visible = *self.workspace_scroll + viewport_rect.size();
            self.follow_figure(ui.ctx(), tab, Some(visible));
        }
        if window_pointer_pressed(
            ui.ctx(),
            egui::LayerId::new(FLOATING_WINDOW_ORDER, id),
            window_rect.intersect(viewport_rect),
        ) {
            self.activate_clicked_tab(ui.ctx(), tab);
        }
        if header_actions.toggle_collapse {
            toggle_collapse(self.collapsed_tabs, tab);
        }
        if header_actions.maximize {
            *self.maximized_tab = Some(tab);
        }
        if header_actions.detach {
            self.detach_requests.push(tab);
        }
        if header_actions.drag_delta != egui::Vec2::ZERO {
            if let Some(position) = self.window_positions.get_mut(&tab) {
                *position += header_actions.drag_delta;
            }
        }
        // Persisted geometry: this frame's clamped logical
        // rectangle, logical size included (not the collapsed
        // size, see `floating_window_geometry_to_persist`).
        self.geometry_updates.push((
            tab,
            floating_window_geometry_to_persist(logical_rect, current_size),
        ));
        if header_actions.close {
            self.close_requests.push(tab);
        }
    }
}

impl StokesForgeApp {
    /// Opens the tab `tab`, or focuses it if it is already open.
    ///
    /// The only place that decides between the two, for every kind of
    /// window (workbook, graph, matrix, note): clicking the same Project
    /// Manager entry a second time must never duplicate its tab, only
    /// bring it to the front.
    pub(super) fn open_tab(&mut self, tab: Tab) {
        self.active_detached = None;
        if let Some(pos) = self.floating_tabs.iter().position(|t| *t == tab) {
            self.floating_tabs.remove(pos);
            self.floating_tabs.push(tab);
            self.focus_next = Some(tab);
        } else {
            self.push_floating_tab(tab);
        }
    }

    /// Adds `tab` to [`StokesForgeApp::floating_tabs`] with a new cascade
    /// rank — shared by [`StokesForgeApp::open_tab`] (first opening) and
    /// [`StokesForgeApp::reattach_tab`] (return of a separate OS window), the two
    /// only places where a floating window appears without already existing.
    pub(super) fn push_floating_tab(&mut self, tab: Tab) {
        self.floating_tabs.push(tab);
        self.cascade_rank.insert(tab, self.next_cascade);
        self.next_cascade = self.next_cascade.wrapping_add(1);
    }
}

impl StokesForgeApp {
    /// Paints the workspace (`CentralPanel`): the maximized window, or
    /// the floating windows in the order of `floating_tabs` with their
    /// cascade, their collapse, their hiding and their handles. The requests
    /// for detaching, closing and geometry are handed back to the caller, who
    /// applies them once the frame is painted (mutating `floating_tabs` in the
    /// middle of the traversal would invalidate it).
    pub(super) fn show_workspace(
        &mut self,
        ui: &mut egui::Ui,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
        detach_requests: &mut Vec<Tab>,
        close_requests: &mut Vec<Tab>,
        geometry_updates: &mut Vec<(Tab, egui::Rect)>,
    ) {
        // Disjoint borrows of the fields (edition 2021): the layout and
        // the contents it carries each need distinct access
        // to `self`, simultaneously while painting a frame.
        let Self {
            state,
            floating_tabs,
            cascade_rank,
            next_cascade,
            focus_next,
            active_detached,
            pm_selection,
            graph_views,
            collapsed_tabs,
            maximized_tab,
            window_positions,
            window_sizes,
            figure_follow,
            workspace_scroll,
            floating_rects,
            ..
        } = self;

        let mut workspace = Workspace {
            state,
            floating_tabs,
            cascade_rank,
            next_cascade,
            focus_next,
            active_detached,
            pm_selection,
            graph_views,
            collapsed_tabs,
            maximized_tab,
            window_positions,
            window_sizes,
            figure_follow,
            workspace_scroll,
            floating_rects,
            actions,
            detach_requests,
            close_requests,
            geometry_updates,
        };
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(tab) = *workspace.maximized_tab {
                workspace.show_maximized(ui, tab);
            } else {
                workspace.show_floating(ui, active_tab);
            }
        });
    }

    /// Applies the closings requested by the header of the floating windows.
    pub(super) fn close_floating_tabs(&mut self, close_requests: Vec<Tab>) {
        for tab in close_requests {
            if let Some(pos) = self.floating_tabs.iter().position(|t| *t == tab) {
                self.floating_tabs.remove(pos);
                self.cascade_rank.remove(&tab);
                self.window_positions.remove(&tab);
                self.window_sizes.remove(&tab);
                if let Tab::Graph(graph) = tab {
                    self.figure_follow.remove(&graph);
                }
            }
            if self.maximized_tab == Some(tab) {
                // Consistency guard, as in `detach_tab` — should not
                // happen: a maximized tab is not painted as a
                // floating window, so its native close button is not
                // reachable while it is.
                self.maximized_tab = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::project_manager::tests::windows;
    use super::super::window_chrome::show_window_in_layer;
    use super::*;
    use stokesforge_app_state::state::ProjectTreeNode;

    /// #154: the explicit click resumes following, even without a tab change.
    #[test]
    fn the_workspace_click_replaces_the_project_manager_multi_selection() {
        for (maximized, already_active, header) in [
            (false, false, false),
            (false, true, false),
            (true, false, false),
            (true, true, false),
            (false, false, true),
            (false, true, true),
            (true, false, true),
            (true, true, true),
        ] {
            let (session, graph) = crate::app::graph_window::tests::plotted_graph();
            let state = AppState::new(session.handle(), String::new()).unwrap();
            let book = window_nodes(&state.project_tree())
                .into_iter()
                .find(|(_, kind)| *kind == WindowKind::Book)
                .map(|(id, _)| Tab::Book(id))
                .unwrap();
            let tab = Tab::Graph(graph);
            let mut app = StokesForgeApp::new(state);
            app.floating_tabs = if already_active {
                vec![book, tab]
            } else {
                vec![tab, book]
            };
            app.window_positions.insert(tab, egui::pos2(30.0, 30.0));
            app.window_positions.insert(book, egui::pos2(750.0, 30.0));
            app.maximized_tab = maximized.then_some(tab);
            app.pm_selection = windows([tab, book]);
            let context = egui::Context::default();
            context.set_fonts(stokesforge_render_egui::font_definitions());
            crate::theme::install(&context);
            egui_extras::install_image_loaders(&context);
            let mut paint = |events| {
                context
                    .run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(1400.0, 900.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            let active = app
                                .maximized_tab
                                .or_else(|| app.floating_tabs.last().copied());
                            app.follow_project_manager_active_tab(ui.ctx(), active);
                            app.show_workspace(
                                ui,
                                active,
                                &mut Vec::new(),
                                &mut Vec::new(),
                                &mut Vec::new(),
                                &mut Vec::new(),
                            );
                        },
                    )
                    .textures_delta
                    .clear();
                app.pm_selection.clone()
            };
            for _ in 0..3 {
                assert_eq!(
                    paint(Vec::new()),
                    windows([tab, book]),
                    "rest keeps the grouped actions"
                );
            }
            if !maximized {
                let empty = egui::pos2(1350.0, 850.0);
                paint(vec![egui::Event::PointerMoved(empty)]);
                assert_eq!(
                    paint(vec![egui::Event::PointerButton {
                        pos: empty,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    }]),
                    windows([tab, book]),
                    "the empty part of the workspace keeps the selection"
                );
                paint(vec![egui::Event::PointerButton {
                    pos: empty,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
            let pos = egui::pos2(
                150.0,
                if header {
                    if maximized {
                        15.0
                    } else {
                        45.0
                    }
                } else {
                    150.0
                },
            );
            assert_eq!(
                paint(vec![egui::Event::PointerMoved(pos)]),
                windows([tab, book]),
                "hovering keeps the selection"
            );
            let selection = paint(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }]);
            assert_eq!(
                selection,
                windows([tab]),
                "maximized={maximized}, already active={already_active}, header={header}"
            );
            session.shutdown().unwrap();
        }
    }

    /// #386: a file hovering a maximized workbook lights up the area it fills
    /// (it has no frame), and the application's outline stays off.
    #[test]
    fn a_file_over_a_maximized_workbook_lights_up_its_area() {
        let mut project = stokesforge_core::Project::new("Maximized").unwrap();
        let root = project.root();
        let book = project.add_empty_workbook(root, None).unwrap();
        project.add_worksheet(book, None).unwrap();
        let session = stokesforge_session::Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let tab = Tab::Book(book);
        let mut app = StokesForgeApp::new(state);
        app.floating_tabs = vec![tab];
        app.maximized_tab = Some(tab);
        let context = super::super::test_support::context();
        egui_extras::install_image_loaders(&context);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        let mut drops = Vec::new();
        for _ in 0..4 {
            let mut input = egui::RawInput {
                screen_rect: Some(screen),
                events: vec![egui::Event::PointerMoved(egui::pos2(700.0, 450.0))],
                ..Default::default()
            };
            input.hovered_files = vec![egui::HoveredFile::default()];
            let mut output = context.run_ui(input, |ui| {
                super::super::file_drop::begin_file_drop(ui.ctx());
                app.show_workspace(
                    ui,
                    Some(tab),
                    &mut Vec::new(),
                    &mut Vec::new(),
                    &mut Vec::new(),
                    &mut Vec::new(),
                );
                super::super::file_drop::finish_file_drop(ui.ctx());
            });
            output.textures_delta.clear();
            drops = output
                .shapes
                .iter()
                .filter_map(|p| match &p.shape {
                    egui::Shape::Rect(r) if r.stroke == crate::theme::drop_target_stroke() => {
                        Some(r.rect)
                    }
                    _ => None,
                })
                .collect();
        }
        assert_eq!(drops.len(), 1, "one drop stroke: {drops:?}");
        assert_ne!(
            drops[0], screen,
            "the maximized area, not the application's outline"
        );
        assert!(screen.contains_rect(drops[0]));
    }

    /// #337: the width of a column that was never set is the default one,
    /// whether the workbook is floating or maximized. In a floating window, the
    /// first frame of an `egui::Window` is a measuring pass
    /// (`Ui::is_sizing_pass`) during which egui_extras shrinks each
    /// column to the width its content used, then keeps it.
    #[test]
    fn the_default_width_of_a_column_is_the_same_floating_and_maximized() {
        let mut project = stokesforge_core::Project::new("Width").unwrap();
        let root = project.root();
        let book = project.add_empty_workbook(root, None).unwrap();
        let sheet = project.add_worksheet(book, None).unwrap();
        for column in 1..=3_u32 {
            let values: Vec<f64> = (1..=20_u32).map(|r| f64::from(column * 100 + r)).collect();
            project
                .add_column(
                    sheet,
                    None,
                    None,
                    stokesforge_core::Column::from_f64(&values),
                )
                .unwrap();
        }
        let session = stokesforge_session::Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let tab = Tab::Book(book);
        let mut app = StokesForgeApp::new(state);
        app.floating_tabs = vec![tab];
        let context = super::super::test_support::context();
        egui_extras::install_image_loaders(&context);
        // Gap between the centers of the column letters "A(Y)", "B(Y)"…
        let step = |app: &mut StokesForgeApp| {
            let mut output = None;
            for _ in 0..6 {
                output = Some(super::super::test_support::frame_of(
                    &context,
                    egui::vec2(1400.0, 900.0),
                    Vec::new(),
                    |ui| {
                        app.show_workspace(
                            ui,
                            Some(tab),
                            &mut Vec::new(),
                            &mut Vec::new(),
                            &mut Vec::new(),
                            &mut Vec::new(),
                        );
                    },
                ));
            }
            let centers: Vec<f32> = output
                .unwrap()
                .shapes
                .iter()
                .filter_map(|painted| match &painted.shape {
                    egui::Shape::Text(shape) if shape.galley.job.text.ends_with("(Y)") => {
                        Some(shape.pos.x + shape.galley.rect.center().x)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(centers.len(), 3, "three column letters: {centers:?}");
            centers[1] - centers[0]
        };
        let expected = super::super::grid::DEFAULT_COLUMN_WIDTH
            + context.global_style().spacing.item_spacing.x;
        let floating = step(&mut app);
        app.maximized_tab = Some(tab);
        let maximized = step(&mut app);
        app.maximized_tab = None;
        let restored = step(&mut app);
        assert!(
            (floating - expected).abs() < 0.6,
            "floating: {floating}, expected {expected}"
        );
        assert!(
            (maximized - expected).abs() < 0.6,
            "maximized: {maximized}, expected {expected}"
        );
        assert!(
            (restored - expected).abs() < 0.6,
            "restored: {restored}, expected {expected}"
        );
        session.shutdown().unwrap();
    }

    #[test]
    fn collapsing_concerns_only_the_targeted_tab() {
        let node = NodeId::from_raw(1);
        let graph_tab = Tab::Graph(node);
        let mut collapsed = HashSet::new();

        toggle_collapse(&mut collapsed, graph_tab);
        assert_eq!(collapsed, HashSet::from([graph_tab]));

        // B5 (`retours-2026-09-23-bugs.md`): `Tab::Book` also has
        // this control since that fix, tracked independently of the
        // graph even when its node identifier coincides (display
        // test).
        let book_tab = Tab::Book(node);
        toggle_collapse(&mut collapsed, book_tab);
        assert_eq!(collapsed, HashSet::from([graph_tab, book_tab]));

        toggle_collapse(&mut collapsed, graph_tab);
        assert_eq!(collapsed, HashSet::from([book_tab]));

        toggle_collapse(&mut collapsed, book_tab);
        assert!(collapsed.is_empty());
    }

    #[test]
    fn collapsing_stays_refused_for_matrices_and_notes() {
        let node = NodeId::from_raw(1);
        let mut collapsed = HashSet::new();

        toggle_collapse(&mut collapsed, Tab::Matrix(node));
        assert!(
            collapsed.is_empty(),
            "a matrix does not have this control: nothing to collapse"
        );

        toggle_collapse(&mut collapsed, Tab::Note(node));
        assert!(
            collapsed.is_empty(),
            "a note does not have this control: nothing to collapse"
        );

        // The refusal does not touch the collapse of the other tabs.
        toggle_collapse(&mut collapsed, Tab::Graph(node));
        toggle_collapse(&mut collapsed, Tab::Matrix(node));
        assert_eq!(collapsed, HashSet::from([Tab::Graph(node)]));
    }

    /// [`toggle_collapse_to`] (reattaching a detached window) sets the
    /// requested value instead of toggling: setting it twice does not
    /// flip it. Same kinds as [`toggle_collapse`].
    #[test]
    fn collapsing_sets_a_value_without_toggling() {
        let node = NodeId::from_raw(1);
        let mut collapsed = HashSet::new();

        toggle_collapse_to(&mut collapsed, Tab::Book(node), true);
        toggle_collapse_to(&mut collapsed, Tab::Book(node), true);
        assert_eq!(collapsed, HashSet::from([Tab::Book(node)]));

        toggle_collapse_to(&mut collapsed, Tab::Book(node), false);
        toggle_collapse_to(&mut collapsed, Tab::Book(node), false);
        assert!(collapsed.is_empty());

        toggle_collapse_to(&mut collapsed, Tab::Matrix(node), true);
        toggle_collapse_to(&mut collapsed, Tab::Note(node), true);
        assert!(
            collapsed.is_empty(),
            "matrices and notes have no collapse, even when set"
        );
    }

    /// The tree's windows, flattened, in tree order (the content of a
    /// folder in its place); the folders themselves are not windows.
    #[test]
    fn the_tree_windows_are_flattened_in_order() {
        let window = |raw, kind| ProjectTreeNode::Window {
            id: NodeId::from_raw(raw),
            name: format!("{raw}"),
            kind,
        };
        let tree = vec![
            window(1, WindowKind::Book),
            ProjectTreeNode::Folder {
                id: NodeId::from_raw(2),
                name: "Folder".to_string(),
                children: vec![
                    window(3, WindowKind::Graph),
                    ProjectTreeNode::Folder {
                        id: NodeId::from_raw(4),
                        name: "Empty".to_string(),
                        children: Vec::new(),
                    },
                    window(5, WindowKind::Note),
                ],
            },
            window(6, WindowKind::Matrix),
        ];
        assert_eq!(
            window_nodes(&tree),
            vec![
                (NodeId::from_raw(1), WindowKind::Book),
                (NodeId::from_raw(3), WindowKind::Graph),
                (NodeId::from_raw(5), WindowKind::Note),
                (NodeId::from_raw(6), WindowKind::Matrix),
            ]
        );
    }

    // B2 (`retours-2026-09-23-bugs.md`): `sync_floating_tabs_order_to_top_layer`
    // tests the pure realignment logic.

    #[test]
    fn the_foreground_tab_is_moved_to_the_end_of_floating_tabs() {
        let tab_a = Tab::Graph(NodeId::from_raw(1));
        let tab_b = Tab::Graph(NodeId::from_raw(2));
        let tab_c = Tab::Book(NodeId::from_raw(3));
        let mut floating_tabs = vec![tab_a, tab_b, tab_c];

        // `tab_c` is already at the end of the list (thus already "active"): nothing to
        // do.
        sync_floating_tabs_order_to_top_layer(&mut floating_tabs, Some(tab_c));
        assert_eq!(floating_tabs, vec![tab_a, tab_b, tab_c]);

        // `tab_a` is in the foreground (direct click) without being at the end of
        // the list: it must go last, the others keeping their relative
        // order (the stack order of the windows underneath).
        sync_floating_tabs_order_to_top_layer(&mut floating_tabs, Some(tab_a));
        assert_eq!(floating_tabs, vec![tab_b, tab_c, tab_a]);
    }

    #[test]
    fn no_foreground_layer_or_an_unknown_tab_leaves_floating_tabs_intact() {
        let tab_a = Tab::Graph(NodeId::from_raw(1));
        let tab_b = Tab::Graph(NodeId::from_raw(2));
        let unknown_tab = Tab::Graph(NodeId::from_raw(3));
        let mut floating_tabs = vec![tab_a, tab_b];

        sync_floating_tabs_order_to_top_layer(&mut floating_tabs, None);
        assert_eq!(floating_tabs, vec![tab_a, tab_b]);

        sync_floating_tabs_order_to_top_layer(&mut floating_tabs, Some(unknown_tab));
        assert_eq!(
            floating_tabs,
            vec![tab_a, tab_b],
            "a tab that is no longer in floating_tabs (closed in the meantime) must disturb nothing"
        );
    }

    // B2, end to end: a direct click in a floating window that is not
    // already at the end of `floating_tabs` makes egui reorder its own
    // layer (`Area`, `Sense::CLICK`) *before* `floating_tabs` knows
    // it — that is exactly the gap fixed by
    // `sync_floating_tabs_order_to_top_layer`. This test paints two windows
    // with the same `egui::Id`s as production (`egui::Id::new(tab)`),
    // simulates a click in the one that is not at the end of the list, then checks
    // that the resolution used in production (`ctx.top_layer_id()` +
    // lookup of the matching tab) does put `tab_a` in the foreground —
    // and that without the resynchronization, `floating_tabs.last()`
    // (`active_tab`) would stay on `tab_b`, stale.
    #[test]
    fn a_direct_click_in_a_background_window_makes_it_active() {
        let tab_a = Tab::Graph(NodeId::from_raw(1));
        let tab_b = Tab::Graph(NodeId::from_raw(2));
        let mut floating_tabs = vec![tab_a, tab_b];
        let context = egui::Context::default();

        let rect_a = egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(100.0, 100.0));
        let rect_b = egui::Rect::from_min_size(egui::pos2(200.0, 20.0), egui::vec2(100.0, 100.0));

        let paint = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                let viewport = ui.ctx().content_rect();
                for (tab, rect) in [(tab_a, rect_a), (tab_b, rect_b)] {
                    show_floating_window(
                        ui.ctx(),
                        egui::Id::new(tab),
                        rect,
                        viewport,
                        false,
                        |_| {},
                        |_, _| {},
                    );
                }
            });
            output.textures_delta.clear();
        };
        let top_layer_tab = |floating_tabs: &[Tab]| {
            context.top_layer_id().and_then(|top| {
                floating_tabs
                    .iter()
                    .copied()
                    .find(|&tab| egui::Id::new(tab) == top.id)
            })
        };

        // Rest frame: `floating_tabs` puts `tab_b` last, as if
        // it had legitimately been opened (or clicked) last — and so does egui,
        // since it painted it last.
        paint(vec![]);
        assert_eq!(top_layer_tab(&floating_tabs), Some(tab_b));

        // Click in `tab_a`, which is not at the end of `floating_tabs` — over
        // two distinct frames, like the other interaction tests
        // (move the mouse, then press): `layer_id_at` must be able to
        // rely on an already known pointer position.
        let click_pos = rect_a.center();
        paint(vec![egui::Event::PointerMoved(click_pos)]);
        assert_eq!(
            top_layer_tab(&floating_tabs),
            Some(tab_b),
            "hovering a window does not bring it to the foreground"
        );
        paint(vec![egui::Event::PointerButton {
            pos: click_pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        }]);

        let top = top_layer_tab(&floating_tabs);
        assert_eq!(
            top,
            Some(tab_a),
            "egui must have brought the clicked window to the front of its own \
             layer order, independently of floating_tabs"
        );

        // Without the resynchronization (B2), `active_tab` would stay stale on
        // `tab_b` despite the click.
        assert_eq!(
            floating_tabs.last().copied(),
            Some(tab_b),
            "before resynchronization, floating_tabs (thus active_tab) still ignores the click"
        );

        sync_floating_tabs_order_to_top_layer(&mut floating_tabs, top);
        assert_eq!(
            floating_tabs.last().copied(),
            Some(tab_a),
            "after resynchronization, the clicked window must become active_tab \
             (thus highlight itself)"
        );
    }

    /// #85: a click in the floating Object Manager (layer above) does not
    /// reactivate the content window it covers; a click on the uncovered
    /// part of that same window does (witness).
    #[test]
    fn clicking_the_floating_manager_does_not_reactivate_the_content_behind() {
        let ctx = egui::Context::default();
        let content_id = egui::Id::new("object_manager_content_test");
        let manager_id = egui::Id::new("object_manager_float_test");
        let content = egui::Rect::from_min_size(egui::pos2(40.0, 40.0), egui::vec2(400.0, 300.0));
        let manager = egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(200.0, 200.0));
        let paint = |events: Vec<egui::Event>, expected: bool| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let viewport = ui.ctx().content_rect();
                    show_window_in_layer(
                        &ctx,
                        manager_id,
                        egui::Order::Foreground,
                        manager,
                        viewport,
                        false,
                        |_| {},
                        |_, _| {},
                    );
                    show_floating_window(
                        &ctx,
                        content_id,
                        content,
                        viewport,
                        false,
                        |_| {},
                        |_, _| {},
                    );
                    assert_eq!(
                        window_pointer_pressed(
                            &ctx,
                            egui::LayerId::new(FLOATING_WINDOW_ORDER, content_id),
                            content
                        ),
                        expected
                    );
                },
            );
            output.textures_delta.clear();
        };
        let press = |pos| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        };
        paint(Vec::new(), false);
        paint(vec![egui::Event::PointerMoved(manager.center())], false);
        paint(vec![press(manager.center())], false);
        paint(
            vec![egui::Event::PointerButton {
                pos: manager.center(),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            false,
        );
        let exposed = egui::pos2(70.0, 70.0);
        paint(vec![egui::Event::PointerMoved(exposed)], false);
        paint(vec![press(exposed)], true);
    }

    /// A workspace with one open graph window at `position`, `size`, painted a
    /// few frames so that the figure is recorded and the scale settled.
    struct FollowBench {
        app: StokesForgeApp,
        context: egui::Context,
        session: stokesforge_session::Session,
        tab: Tab,
        graph: NodeId,
        screen: egui::Vec2,
    }

    impl FollowBench {
        fn new(position: egui::Pos2, size: egui::Vec2) -> Self {
            let (session, graph) = crate::app::graph_window::tests::plotted_graph();
            let state = AppState::new(session.handle(), String::new()).unwrap();
            let tab = Tab::Graph(graph);
            let mut app = StokesForgeApp::new(state);
            app.floating_tabs = vec![tab];
            app.window_positions.insert(tab, position);
            app.window_sizes.insert(tab, size);
            let context = super::super::test_support::context();
            egui_extras::install_image_loaders(&context);
            let mut bench = Self {
                app,
                context,
                session,
                tab,
                graph,
                screen: egui::vec2(1920.0, 1080.0),
            };
            bench.settle();
            bench
        }

        fn frame(&mut self) {
            let Self {
                app,
                context,
                tab,
                screen,
                ..
            } = self;
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, *screen)),
                        ..Default::default()
                    },
                    |ui| {
                        let active = app.maximized_tab.or(Some(*tab));
                        app.show_workspace(
                            ui,
                            active,
                            &mut Vec::new(),
                            &mut Vec::new(),
                            &mut Vec::new(),
                            &mut Vec::new(),
                        );
                    },
                )
                .textures_delta
                .clear();
        }

        fn settle(&mut self) {
            for _ in 0..4 {
                self.frame();
            }
        }

        fn size(&self) -> egui::Vec2 {
            self.app.window_sizes[&self.tab]
        }

        fn figure(&self) -> (u32, u32) {
            crate::app::graph_window::published_figure_size(&self.context, self.graph)
                .expect("the figure is published")
        }

        fn scale(&self) -> f32 {
            self.app.graph_views[&self.graph]
                .display_scale()
                .expect("painted")
        }
    }

    /// #394: adding a cell beside grows the window to the right by the growth
    /// of the figure at the display scale, height and top-left unchanged; Undo
    /// brings both back.
    #[test]
    fn the_window_grows_with_the_figure_and_shrinks_back_on_undo() {
        let position = egui::pos2(40.0, 30.0);
        let mut bench = FollowBench::new(position, egui::vec2(700.0, 560.0));
        let (before, scale, figure) = (bench.size(), bench.scale(), bench.figure());
        bench.app.state.dispatch(Action::AddCell {
            graph: bench.graph,
            panel: 0,
            side: stokesforge_core::compose::CellSide::Right,
            columns: Vec::new(),
        });
        bench.settle();
        let grown = bench.figure().0 - figure.0;
        assert!(grown > 0, "the figure is wider");
        assert_eq!(bench.figure().1, figure.1, "same height");
        let after = bench.size();
        assert!(
            (after.x - before.x - grown as f32 * scale).abs() < 0.01,
            "{before:?} -> {after:?}, figure +{grown} at scale {scale}"
        );
        assert_eq!(after.y, before.y, "height unchanged");
        assert_eq!(
            bench.app.window_positions[&bench.tab], position,
            "top-left fixed"
        );

        bench.app.state.dispatch(Action::Undo);
        bench.settle();
        assert_eq!(bench.figure(), figure);
        assert!(
            (bench.size() - before).length() < 0.01,
            "back to {before:?}"
        );
        assert_eq!(bench.app.window_positions[&bench.tab], position);

        // Redo, then a handle commit (what a drag releases): the top-left stays
        // where it was, whatever the figure does.
        bench.app.state.dispatch(Action::Redo);
        bench.settle();
        assert_eq!(
            bench.app.window_positions[&bench.tab], position,
            "after Redo"
        );
        bench.app.state.dispatch(Action::SetFrameSizes {
            graph: bench.graph,
            column: Some((0, 380.0)),
            row: Some((0, 330.0)),
        });
        bench.settle();
        assert_eq!(
            bench.app.window_positions[&bench.tab], position,
            "after a drag"
        );
        bench.session.shutdown().unwrap();
    }

    /// #394: an axis added makes the image grow around the fixed plot area, and
    /// the window follows (the stored frame does not change).
    #[test]
    fn adding_a_right_axis_grows_the_window() {
        let mut bench = FollowBench::new(egui::pos2(40.0, 30.0), egui::vec2(700.0, 560.0));
        let (before, scale, figure) = (bench.size(), bench.scale(), bench.figure());
        let mut spec = bench.app.state.plot_spec(bench.graph).expect("spec");
        let mut panels: Vec<_> = spec.panels.iter().cloned().collect();
        panels[0].set_series_axis(0, stokesforge_plot::SeriesAxis::Right);
        spec.panels = stokesforge_plot::Panels::new(panels);
        bench.app.state.dispatch(Action::SetPlotSpec {
            graph: bench.graph,
            spec,
        });
        bench.settle();
        let grown = bench.figure().0 - figure.0;
        assert!(grown > 0, "the right axis widens the image");
        assert!(
            (bench.size().x - before.x - grown as f32 * scale).abs() < 0.01,
            "{before:?} -> {:?}",
            bench.size()
        );
        bench.session.shutdown().unwrap();
    }

    /// #394: the window never leaves the workspace to the right.
    #[test]
    fn the_window_stops_at_the_edge_of_the_workspace() {
        let position = egui::pos2(1500.0, 30.0);
        let mut bench = FollowBench::new(position, egui::vec2(380.0, 560.0));
        bench.app.state.dispatch(Action::AddCell {
            graph: bench.graph,
            panel: 0,
            side: stokesforge_core::compose::CellSide::Right,
            columns: Vec::new(),
        });
        bench.settle();
        assert!(position.x + bench.size().x <= WORKSPACE_SIZE.x);
        bench.session.shutdown().unwrap();
    }

    /// #394 (T2): in a workspace narrower than its canvas the window stops at the
    /// right edge of the VISIBLE part, title bar and buttons included, and the
    /// figure is contained in it; the top-left does not move.
    #[test]
    fn the_window_stops_at_the_visible_edge_of_a_narrow_workspace() {
        let position = egui::pos2(40.0, 30.0);
        let mut bench = FollowBench::new(position, egui::vec2(700.0, 400.0));
        bench.screen = egui::vec2(1000.0, 700.0);
        bench.settle();
        bench.app.state.dispatch(Action::AddCell {
            graph: bench.graph,
            panel: 0,
            side: stokesforge_core::compose::CellSide::Right,
            columns: Vec::new(),
        });
        bench.settle();
        let visible = bench.screen.x;
        assert!(
            position.x + bench.size().x <= visible,
            "window right edge {} past the visible edge {visible}",
            position.x + bench.size().x
        );
        assert!(bench.size().x > 700.0, "it did grow up to the edge");
        assert_eq!(bench.app.window_positions[&bench.tab], position);
        bench.session.shutdown().unwrap();
    }

    /// #394: a maximized window does not resize, and restoring it does not
    /// either (the record was kept up to date).
    #[test]
    fn a_maximized_window_does_not_resize() {
        let mut bench = FollowBench::new(egui::pos2(40.0, 30.0), egui::vec2(700.0, 560.0));
        let before = bench.size();
        bench.app.maximized_tab = Some(bench.tab);
        bench.settle();
        bench.app.state.dispatch(Action::AddCell {
            graph: bench.graph,
            panel: 0,
            side: stokesforge_core::compose::CellSide::Right,
            columns: Vec::new(),
        });
        bench.settle();
        assert_eq!(bench.size(), before, "maximized");
        bench.app.maximized_tab = None;
        bench.settle();
        assert_eq!(bench.size(), before, "restored");
        bench.session.shutdown().unwrap();
    }

    /// #394: a figure seen for the first time (opening or loading a project)
    /// resizes nothing, whatever its size.
    #[test]
    fn opening_a_graph_resizes_nothing() {
        let (session, graph) = crate::app::graph_window::tests::plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let tab = Tab::Graph(graph);
        let mut app = StokesForgeApp::new(state);
        app.state.dispatch(Action::AddCell {
            graph,
            panel: 0,
            side: stokesforge_core::compose::CellSide::Right,
            columns: Vec::new(),
        });
        app.floating_tabs = vec![tab];
        app.window_positions.insert(tab, egui::pos2(40.0, 30.0));
        app.window_sizes.insert(tab, egui::vec2(700.0, 560.0));
        let mut bench = FollowBench {
            app,
            context: {
                let context = super::super::test_support::context();
                egui_extras::install_image_loaders(&context);
                context
            },
            session,
            tab,
            graph,
            screen: egui::vec2(1920.0, 1080.0),
        };
        bench.settle();
        assert_eq!(bench.size(), egui::vec2(700.0, 560.0));
        bench.session.shutdown().unwrap();
    }
}
