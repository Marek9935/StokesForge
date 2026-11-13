//! The detached workbook window (#475, step 2, L2): a workbook in its own OS
//! window that serves itself, like the detached graph (`detached_graph.rs`).
//!
//! The closure of the viewport locks the whole application
//! ([`lock_app_from_viewport`]), paints the workbook from the **shared**
//! [`AppState`](stokesforge_app_state::state::AppState) — the function of the
//! floating window, [`book_contents`] — and applies the actions of its own
//! gestures at once, in the same pass ([`StokesForgeApp::apply_pass_actions`]).
//! No fork of the state, no queue and no root pass stand between a click and its
//! effect. The selection, the clipboard, the entry in progress and the
//! renaming of a sheet are the application's: **one cell entry in the whole
//! application**. Typing in a workbook and clicking in another one (docked or
//! detached) commits the first entry, exactly as between two workbooks of the
//! main window, in one history entry.
//!
//! What the window keeps for itself is a view state, [`DetachedBook`]: the
//! collapse, the focus, the size to restore, the flags of Reattach and Close,
//! and its **sender identity**.
//!
//! **The sender identity (#21).** A macro of the main window (the properties of
//! a graph) is opened with the main window's sender; the actor refuses the writes
//! of any other sender until it ends, so that undoing the macro takes away
//! nothing that was written beside it. The gestures of the grid write straight
//! into the state while the window paints (a cell entry closed by a click
//! elsewhere, a sheet tab): with one shared state they would write with the
//! main window's identity and be absorbed. Each detached workbook therefore owns
//! an identity ([`DetachedBook::identity`], made at the detaching) which the
//! pass puts into the state for the duration of the **paint only**
//! ([`WindowIdentity`], a guard that gives the main identity back even on a
//! panic or an early return). The actions the window returns are dispatched
//! afterwards with the main identity, as every action of the application: they
//! pass the gate of the open dialog first ([`StokesForgeApp::apply_pass_actions`]),
//! which stays the first barrier; the identity is the second. The graph window
//! writes only through its actions, so it needs no identity of its own.
//!
//! **Reattach and Close** follow the graph (L4): on native Wayland the window
//! registers itself again at the end of each pass and, to leave, does not; under
//! X11 and on Windows the root does it from the flags the window raised.

use eframe::egui;
use stokesforge_app_state::panel_layout::PanelKind;
use stokesforge_app_state::state::Action;
use stokesforge_app_state::tr;
use stokesforge_core::NodeId;
use stokesforge_session::SessionHandle;

use super::file_drop::{begin_file_drop, finish_file_drop};
use super::panel_memory::panel_index;
use super::server::lock_app_from_viewport;
use super::tab::{tab_title, tab_title_from_tree, Tab};
use super::window_chrome::{
    detached_resize_handles, paint_active_window_outline, show_window_contents_bounded,
    window_header, HeaderPlacement, WindowHeaderActions, MIN_CONTENT_WINDOW_SIZE,
    WINDOW_CONTENT_MARGIN, WINDOW_HEADER_HEIGHT,
};
use super::window_placement::floating_window_display_size;
use super::workbook::book_contents;
use super::StokesForgeApp;

/// The size a detached workbook opens with.
pub(super) const BOOK_WINDOW_SIZE: [f32; 2] = [420.0, 360.0];

/// The view state of a workbook detached into its own OS window.
pub(super) struct DetachedBook {
    /// The sender identity of this window (#21): in the state while the window
    /// paints, here the rest of the time. `None` only while it is in the state.
    pub(super) identity: Option<SessionHandle>,
    /// Collapse of the content, specific to this window: initialised from
    /// `collapsed_tabs` at the detaching and poured back on Reattach.
    pub(super) collapsed: bool,
    /// The first callback of a deferred viewport can arrive before the surface
    /// has its final size: one more frame, without sustaining a render loop.
    pub(super) needs_initial_repaint: bool,
    /// Last known OS size **outside collapse** (B9): the target of the resize
    /// sent on expanding.
    pub(super) expanded_size: Option<egui::Vec2>,
    /// The OS focus, as this window's own pass saw it.
    pub(super) focused: bool,
    /// "Reattach to the main window" was actuated (see the graph's flag).
    pub(super) reattach: bool,
    /// The cross, or Close, was actuated.
    pub(super) close: bool,
    /// Native Wayland: the window registers itself from now on.
    pub(super) self_owned: bool,
}

impl DetachedBook {
    pub(super) fn new(identity: SessionHandle, collapsed: bool) -> Self {
        Self {
            identity: Some(identity),
            collapsed,
            needs_initial_repaint: true,
            expanded_size: None,
            focused: false,
            reattach: false,
            close: false,
            self_owned: false,
        }
    }
}

/// The viewport of the detached window of `book`.
pub(super) fn book_viewport(book: NodeId) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(Tab::Book(book))
}

/// The sender identity of a detached window, put into the state for the length
/// of a paint (#21). Dereferences to the application; gives the previous
/// identity back when it is dropped, whatever way the paint ended. Used by the
/// detached workbook (whose grid writes while it paints) and by the detached
/// graph (for the entry a click in its window closes).
pub(super) struct WindowIdentity<'a> {
    app: &'a mut StokesForgeApp,
    key: WindowKey,
    /// The identity the state had; `None` when the window had none to put in.
    previous: Option<SessionHandle>,
}

impl<'a> WindowIdentity<'a> {
    pub(super) fn enter(app: &'a mut StokesForgeApp, key: impl Into<WindowKey>) -> Self {
        let key = key.into();
        let own = app.window_identity(key).and_then(Option::take);
        let previous = own.map(|own| app.state.replace_session_identity(own));
        Self { app, key, previous }
    }
}

/// A detached window that owns a sender identity: a content tab or a panel.
#[derive(Clone, Copy)]
pub(super) enum WindowKey {
    Tab(Tab),
    Panel(PanelKind),
}

impl From<Tab> for WindowKey {
    fn from(tab: Tab) -> Self {
        Self::Tab(tab)
    }
}

impl From<PanelKind> for WindowKey {
    fn from(kind: PanelKind) -> Self {
        Self::Panel(kind)
    }
}

impl std::ops::Deref for WindowIdentity<'_> {
    type Target = StokesForgeApp;
    fn deref(&self) -> &StokesForgeApp {
        self.app
    }
}

impl std::ops::DerefMut for WindowIdentity<'_> {
    fn deref_mut(&mut self) -> &mut StokesForgeApp {
        self.app
    }
}

impl Drop for WindowIdentity<'_> {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            let own = self.app.state.replace_session_identity(previous);
            // A window the pass removed (Reattach, Close) has no place to keep it.
            if let Some(slot) = self.app.window_identity(self.key) {
                *slot = Some(own);
            }
        }
    }
}

/// Who holds the entry in progress.
#[derive(Clone, Copy)]
enum Holder {
    /// A docked workbook or a note (the main window's grid).
    Docked,
    /// The detached workbook.
    Book(NodeId),
}

/// Which pass ends the entry held elsewhere.
#[derive(Clone, Copy)]
pub(super) enum PassOf {
    Root,
    Book(NodeId),
    Graph,
    /// A detached panel (Project Manager, Object Manager, console).
    Panel,
}

impl StokesForgeApp {
    /// The identity slot of the detached window `key`, if there is one.
    pub(super) fn window_identity(&mut self, key: WindowKey) -> Option<&mut Option<SessionHandle>> {
        match key {
            WindowKey::Panel(kind) => Some(&mut self.panel_windows[panel_index(kind)].identity),
            WindowKey::Tab(Tab::Book(book)) => self
                .detached_books
                .get_mut(&book)
                .map(|window| &mut window.identity),
            WindowKey::Tab(Tab::Graph(graph)) => self
                .detached_graphs
                .get_mut(&graph)
                .map(|window| &mut window.identity),
            WindowKey::Tab(Tab::Matrix(_) | Tab::Note(_)) => None,
        }
    }

    /// The window that holds the entry in progress, if there is one: the
    /// detached workbook the cell or header belongs to, else the main window.
    fn entry_holder(&self) -> Option<Holder> {
        let state = &self.state;
        let column = state
            .editing()
            .map(|edit| edit.column)
            .or_else(|| state.header_editing().map(|edit| edit.column));
        let Some(column) = column else {
            return state.note_editing().map(|_| Holder::Docked);
        };
        let tree = state.tree();
        let book = tree
            .parent(column)
            .ok()
            .flatten()
            .and_then(|sheet| tree.parent(sheet).ok().flatten())
            .filter(|book| self.detached_books.contains_key(book));
        Some(book.map_or(Holder::Docked, Holder::Book))
    }

    /// A workbook window that closes takes its entry with it: committed like a
    /// click elsewhere would (under the window's identity, refused while a dialog
    /// is open in the main window), so that no entry is left on a sheet nobody
    /// shows.
    pub(super) fn commit_entry_of_closing_book(&mut self, book: NodeId) {
        if matches!(self.entry_holder(), Some(Holder::Book(held)) if held == book)
            && self.state.open_dialog().is_none()
        {
            WindowIdentity::enter(self, Tab::Book(book))
                .state
                .commit_open_entries();
        }
    }

    /// One cell entry in the whole application (#475, L2): a press in a window
    /// that does not hold the entry ends it, exactly as a click in another
    /// workbook ends it in the main window. The entry is the state's own, so
    /// the window that is clicked commits it, before it paints. While a dialog
    /// is open in the main window the commit is refused like any other
    /// (the entry stays, the band says why). The caller holds the identity of
    /// its window ([`WindowIdentity`]) when it has one.
    pub(super) fn commit_entry_held_elsewhere(&mut self, ctx: &egui::Context, pass: PassOf) {
        if !ctx.input(|input| input.pointer.any_pressed()) {
            return;
        }
        let Some(holder) = self.entry_holder() else {
            return;
        };
        let ends_it = match (pass, holder) {
            // The root commits an entry of a detached workbook; its own docked
            // entries are the docked grid's.
            (PassOf::Root, Holder::Book(_)) => true,
            (PassOf::Root, Holder::Docked) => false,
            (PassOf::Book(this), Holder::Book(held)) => held != this,
            (PassOf::Book(_), Holder::Docked) => true,
            (PassOf::Graph | PassOf::Panel, _) => true,
        };
        if !ends_it {
            return;
        }
        // A dialog open in the main window refuses it: in a window by the usual
        // refusal (the band says why), in the root, which owns the dialog, by
        // doing nothing.
        let refused = match pass {
            PassOf::Root => self.state.open_dialog().is_some(),
            PassOf::Book(_) | PassOf::Graph | PassOf::Panel => {
                super::window_band::refuse_entry(ctx)
            }
        };
        if refused {
            return;
        }
        self.state.commit_open_entries();
    }
}

/// Frame state read before the clicks, then actions of the painted header.
struct Frame {
    was_collapsed: bool,
    is_active: bool,
    header_actions: WindowHeaderActions,
    inner_margin: egui::Margin,
}

impl StokesForgeApp {
    /// The viewport of the detached window of `book`: no native decoration, the
    /// home-made bar is all the user has to move, resize and close it. Neither
    /// position nor size remembered (issue #16).
    fn book_window_builder(&self, book: NodeId) -> egui::ViewportBuilder {
        let title = tr!(
            "window-title-detached",
            name = tab_title(&self.state, Tab::Book(book))
        );
        crate::app_logo::viewport()
            .with_title(title)
            .with_decorations(false)
            .with_inner_size(BOOK_WINDOW_SIZE)
            // The B9 collapse must be able to reduce the height below the usual
            // minimum. Below this width, the sheet tabs scroll instead of
            // shrinking.
            .with_min_inner_size([MIN_CONTENT_WINDOW_SIZE.x, WINDOW_HEADER_HEIGHT])
    }

    /// Queues the registration of the window of `book` (`publish_viewport`, the
    /// one function that registers a viewport). Called by the root until the
    /// window's first pass, then by the window itself on native Wayland.
    fn publish_book_window(&mut self, ctx: &egui::Context, book: NodeId) {
        let viewport = self.book_window_builder(book);
        let shared = self.shared_self.clone();
        Self::publish_viewport(
            &mut self.pending_viewports,
            ctx,
            book_viewport(book),
            viewport,
            false,
            move |ui, _class| {
                // The root released its guard before this window was published.
                let Some(shared) = shared.upgrade() else {
                    return;
                };
                let pending = {
                    let mut app = lock_app_from_viewport(&shared);
                    app.detached_book_pass(ui, book);
                    app.take_pending_viewports()
                };
                // The window registers itself with the lock released.
                super::window_detach::publish_viewports(ui.ctx(), pending);
            },
        );
    }

    /// Publishes the viewport of each detached workbook that is not yet its own
    /// parent (queued for after the lock is released).
    pub(super) fn show_detached_books(&mut self, ctx: &egui::Context) {
        let ids: Vec<NodeId> = self.detached_books.keys().copied().collect();
        for book in ids {
            if self.detached_books[&book].self_owned {
                continue;
            }
            self.publish_book_window(ctx, book);
        }
    }

    /// Takes the detached window of `book` out of the list. A window that
    /// registered itself needs one pass of its own to leave egui: it is woken
    /// once, by [`Self::wake_released_windows`].
    pub(super) fn release_book_window(&mut self, book: NodeId) -> Option<DetachedBook> {
        let window = self.detached_books.remove(&book)?;
        if window.self_owned {
            self.released_windows.push(book_viewport(book));
        }
        Some(window)
    }

    /// The flags a detached workbook window raised since the last root pass:
    /// Reattach and Close go through the root where the window cannot close
    /// itself.
    pub(super) fn drain_detached_book_flags(&mut self, ctx: &egui::Context) {
        let mut to_reattach = Vec::new();
        let mut to_close = Vec::new();
        for (book, window) in &mut self.detached_books {
            if std::mem::take(&mut window.reattach) {
                to_reattach.push(*book);
            }
            if std::mem::take(&mut window.close) {
                to_close.push(*book);
            }
        }
        for book in to_reattach {
            super::window_band::clear(ctx, book_viewport(book));
            self.reattach_tab(Tab::Book(book));
        }
        for book in to_close {
            super::window_band::clear(ctx, book_viewport(book));
            self.close_detached_tab(Tab::Book(book));
        }
    }

    /// One pass of the detached window of `book`, under the application lock:
    /// keys, paint (with the window's identity), then its own actions applied at
    /// once.
    fn detached_book_pass(&mut self, ui: &mut egui::Ui, book: NodeId) {
        let ctx = ui.ctx().clone();
        let viewport = ctx.viewport_id();
        // The dialog is the application's: seen at once, not at the next root
        // pass.
        super::window_band::publish_dialog(&ctx, self.state.open_dialog());
        super::window_band::stop_keys(&ctx);
        super::shortcuts::refuse_quit_in_detached(&ctx);
        begin_file_drop(&ctx);
        if self.detached_books.contains_key(&book) {
            let mut prelude = Vec::new();
            self.pass_prelude(&ctx, &mut prelude);
            let mut own = Vec::new();
            let recorded = self.state.recorded_count();
            let woke = WindowIdentity::enter(self, Tab::Book(book))
                .paint_detached_book(ui, book, &mut own);
            // What the grid said while it wrote (a refused entry) is told here.
            self.tell_window(&ctx, viewport, recorded);
            self.answer_refusals(&ctx);
            self.apply_pass_actions(&ctx, viewport, prelude, own, woke);
            self.keep_or_close_own_book_window(&ctx, viewport, book);
        }
        super::page_layout::note_field_focus(&ctx);
        super::window_band::paint(&ctx);
    }

    /// Native Wayland (L4): the end of the window's pass. Reattach or Close
    /// raised in this pass is carried out here, at once; otherwise the window
    /// registers itself again. Elsewhere the root does both.
    fn keep_or_close_own_book_window(
        &mut self,
        ctx: &egui::Context,
        viewport: egui::ViewportId,
        book: NodeId,
    ) {
        if !self.windows_close_themselves {
            return;
        }
        let Some(window) = self.detached_books.get_mut(&book) else {
            return;
        };
        let reattach = std::mem::take(&mut window.reattach);
        let close = std::mem::take(&mut window.close);
        if reattach || close {
            // This pass is the window's last: nobody wakes it for its removal.
            window.self_owned = false;
            super::window_band::clear(ctx, viewport);
            let tab = Tab::Book(book);
            if reattach {
                self.reattach_tab(tab);
            } else {
                self.close_detached_tab(tab);
            }
            return;
        }
        window.self_owned = true;
        self.publish_book_window(ctx, book);
    }

    /// Paints the window and collects the actions of its gestures into `own`.
    /// Returns whether the root and the other windows must be woken for
    /// something that is not an action (the selection moved, the focus changed,
    /// a flag was raised, the grid wrote).
    fn paint_detached_book(
        &mut self,
        ui: &mut egui::Ui,
        book: NodeId,
        own: &mut Vec<Action>,
    ) -> bool {
        // One entry in the application: a press here ends the entry held by
        // another window, before this one paints.
        self.commit_entry_held_elsewhere(ui.ctx(), PassOf::Book(book));
        let snapshot = self.state.snapshot().clone();
        let before = self.state_stamp(book);
        let Self {
            state,
            detached_books,
            ..
        } = self;
        let Some(window) = detached_books.get_mut(&book) else {
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
        // Read before anything is painted (#459): a cell or a field never sees
        // these keys. Queued after the window's own actions, below.
        let mut keys = super::shortcuts::read_detached_history_and_save(ui.ctx());
        let (header_actions, frame) = paint_frame(ui, state, &snapshot, book, own, was_collapsed);
        // Undo, Redo and Save come after the gestures of this frame: Ctrl+Z
        // pressed in the frame a drag is released undoes that drag (#398).
        own.append(&mut keys);
        if let Some(mut action) = finish_file_drop(ui.ctx()) {
            // Same destination as a floating workbook, even if its grid is
            // collapsed and did not claim the drop.
            if let Action::DropFiles { target_book, .. } = &mut action {
                *target_book = Some(book);
            }
            own.push(action);
        }
        apply_window_actions(
            ui,
            window,
            Frame {
                was_collapsed,
                is_active,
                header_actions,
                inner_margin: frame.inner_margin,
            },
        );
        if std::mem::take(&mut window.needs_initial_repaint) {
            // `show_viewport_deferred` creates the window after the root frame.
            // Under Wayland the first callback may see a surface without a final
            // size; redrawing once rebuilds the grid with its real rectangle.
            ui.ctx().request_repaint();
        }
        if ui.ctx().input(|input| input.viewport().close_requested()) {
            window.close = true;
        }
        let raised = window.reattach || window.close;
        wake_others || raised || self.state_stamp(book) != before
    }

    /// What the other windows read of this workbook: the model revision and the
    /// selection of its sheets (the Object Manager and the root follow it).
    fn state_stamp(
        &self,
        book: NodeId,
    ) -> (
        u64,
        Vec<Option<stokesforge_app_state::selection::Selection>>,
    ) {
        let selections = self
            .state
            .worksheets_of(book)
            .into_iter()
            .map(|(sheet, _)| self.state.selection(sheet).cloned())
            .collect();
        (self.state.snapshot().revision, selections)
    }
}

/// The frame of the window: header and content. Returns the header actions and
/// the frame.
fn paint_frame(
    ui: &mut egui::Ui,
    state: &mut stokesforge_app_state::state::AppState,
    snapshot: &stokesforge_session::Snapshot,
    book: NodeId,
    own: &mut Vec<Action>,
    was_collapsed: bool,
) -> (WindowHeaderActions, egui::Frame) {
    let tab = Tab::Book(book);
    let id = egui::Id::new(tab);
    let mut header_actions = WindowHeaderActions::default();
    // Default stroke (inactive) only: the active outline is painted separately,
    // after `.show()`, on the rectangle of the real OS viewport (B7).
    let frame = egui::Frame::central_panel(ui.style())
        .inner_margin(WINDOW_CONTENT_MARGIN)
        .stroke(ui.style().visuals.window_stroke());
    egui::CentralPanel::default().frame(frame).show(ui, |ui| {
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
        // The content may overflow, but it must modify neither the Frame nor
        // the viewport's clip rectangle.
        show_window_contents_bounded(ui, |ui| book_contents(ui, state, own, book));
    });
    (header_actions, frame)
}

/// The window commands and the outline: collapse, resize handles, maximise,
/// drag, Reattach, Close. Everything addressed to the OS window targets this
/// pass's own viewport.
fn apply_window_actions(ui: &mut egui::Ui, window: &mut DetachedBook, frame: Frame) {
    let Frame {
        was_collapsed,
        is_active,
        header_actions,
        inner_margin,
    } = frame;
    // Collapse state that applies from this frame.
    let collapsed_now = was_collapsed ^ header_actions.toggle_collapse;
    // The drop target's border replaces the active outline (#386).
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
            .unwrap_or_else(|| egui::vec2(BOOK_WINDOW_SIZE[0], BOOK_WINDOW_SIZE[1]));
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
        window.reattach = true;
    }
    if header_actions.close {
        window.close = true;
    }
}

#[cfg(test)]
mod tests {
    //! The detached workbook serves itself (#475, L2): its actions apply in its
    //! own pass, the selection is the application's, Reattach and Close follow
    //! the graph's pattern, and the sender identity keeps a write out of a macro
    //! of the main window (#21).
    use std::time::{Duration, Instant};

    use super::super::test_support::bench::{delay, Bench, Pass};
    use super::super::test_support::texts;
    use super::super::window_band;
    use super::*;

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// The first column and first sheet of the first workbook.
    fn first_column(bench: &Bench) -> (NodeId, NodeId) {
        let sheet = bench.app.state.selected_sheet_in(bench.book()).unwrap();
        (sheet, bench.app.state.tree().children(sheet).unwrap()[0])
    }

    /// Clicks the painted text `label` of the workbook window (hover, press,
    /// release): child passes only.
    fn click_text(bench: &mut Bench, label: &str) {
        let id = bench.book_id();
        let output = bench.child_in(id, Pass::child());
        let target = texts(&output.shapes)
            .into_iter()
            .filter(|text| text.text == label)
            .max_by(|a, b| a.center.y.total_cmp(&b.center.y))
            .unwrap_or_else(|| panic!("{label:?} is painted"))
            .center;
        bench.child_in(
            id,
            Pass::child().events(vec![egui::Event::PointerMoved(target)]),
        );
        for pressed in [true, false] {
            bench.child_in(
                id,
                Pass::child().events(vec![egui::Event::PointerButton {
                    pos: target,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }]),
            );
        }
    }

    /// A key pressed in the window's own pass alone applies at once (no root
    /// pass, nothing queued) and wakes the root and nobody else is needed.
    /// Witness: queue the action for the root (the undo waits).
    #[test]
    fn a_key_in_the_window_acts_in_its_own_pass_and_wakes_the_root() {
        let mut bench = Bench::with_detached_book();
        bench.start_book();
        bench.app.state.dispatch(Action::AddWorkbook);
        let books = bench.app.state.books().len();
        let output = bench.child_in(
            bench.book_id(),
            Pass::child().events(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]),
        );
        assert_eq!(bench.app.state.books().len(), books - 1, "undone at once");
        assert_eq!(delay(&output, egui::ViewportId::ROOT), Duration::ZERO);
        bench.finish();
    }

    /// T13-like: a selection made by a click in the detached workbook is the
    /// application's own, seen by the root in the same pass, and wakes the root.
    /// Witness: keep the selection in a copy of the state (the fork), or wake
    /// nobody.
    #[test]
    fn a_selection_made_in_the_detached_workbook_is_seen_by_the_root_at_once() {
        let mut bench = Bench::with_detached_book();
        bench.start_book();
        let (sheet, column) = first_column(&bench);
        let second = bench.app.state.cell_display(column, 1);
        assert!(bench.app.state.selection(sheet).is_none());
        click_text(&mut bench, &second);
        assert!(
            bench.app.state.selection(sheet).is_some(),
            "the shared state holds the selection, no root pass ran"
        );
        let output = bench.child_in(
            bench.book_id(),
            Pass::child().events(vec![egui::Event::PointerMoved(egui::pos2(1.0, 1.0))]),
        );
        let _ = output;
        // The click that moved it woke the root.
        let output = click_again(&mut bench, &second);
        assert_eq!(delay(&output, egui::ViewportId::ROOT), Duration::ZERO);
        bench.finish();
    }

    /// A second click on the same text, returning the output of the release.
    fn click_again(bench: &mut Bench, label: &str) -> egui::FullOutput {
        let id = bench.book_id();
        let output = bench.child_in(id, Pass::child());
        let (_, column) = first_column(bench);
        // Another row: a selection that really moves.
        let other = bench.app.state.cell_display(column, 2);
        let target = texts(&output.shapes)
            .into_iter()
            .find(|text| text.text == other && text.text != label)
            .map_or_else(|| panic!("{other:?} is painted"), |text| text.center);
        bench.child_in(
            id,
            Pass::child().events(vec![egui::Event::PointerMoved(target)]),
        );
        let button = |pressed| {
            Pass::child().events(vec![egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }])
        };
        bench.child_in(id, button(true));
        bench.child_in(id, button(false))
    }

    /// T4 (b), #21: with a macro of the main window open and NO dialog (so the
    /// gate of the open dialog lets the gesture through), the commit of a typed
    /// cell closed by a click in the detached workbook is refused by the actor
    /// (the window writes under its own identity): the cell is unchanged, the
    /// entry stays, the band of the window says why, and rolling back the macro
    /// removes nothing of it. Witness: paint with the main identity
    /// (`WindowIdentity::enter` leaves the state alone): the write is absorbed,
    /// the cell changes inside the macro and the rollback then takes it away.
    #[test]
    fn a_cell_committed_in_a_detached_workbook_during_a_root_macro_is_refused() {
        let _language = stokesforge_app_state::i18n::test_language(
            stokesforge_app_state::i18n::Language::English,
        );
        let mut bench = Bench::with_detached_book();
        bench.start_book();
        let (sheet, column) = first_column(&bench);
        let before = bench.app.state.cell_text(column, 0);
        let other = bench.app.state.cell_display(column, 1);
        bench.app.state.select_cell(sheet, column, 0);
        let id = bench.book_id();
        bench.child_in(id, Pass::child());
        for digit in ["7", "7"] {
            bench.child_in(
                id,
                Pass::child().events(vec![
                    egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                    egui::Event::Text(digit.into()),
                ]),
            );
            for _ in 0..3 {
                bench.child_in(id, Pass::child());
            }
        }
        assert!(bench.app.state.editing().is_some(), "the entry is open");
        bench.app.state.open_root_macro_for_test();
        assert!(
            bench.app.state.open_dialog().is_none(),
            "no dialog, only the macro"
        );
        // A click elsewhere closes the entry: the grid commits it.
        click_text(&mut bench, &other);
        bench.child_in(id, Pass::child());
        assert_eq!(bench.app.state.cell_text(column, 0), before, "not written");
        let (band, _) = window_band::text_at(&bench.ctx, id, Instant::now())
            .expect("the band of the window says why");
        assert!(!band.is_empty());
        assert!(bench.app.state.status().is_some_and(|s| s.is_error()));
        // The macro is rolled back: nothing of the refused write was in it.
        bench.app.state.cancel_plot_settings();
        assert_eq!(bench.app.state.cell_text(column, 0), before);
        assert!(bench.app.state.undo_label().is_none(), "no entry was made");
        bench.finish();
    }

    /// The identity guard gives the main identity back whatever way the paint
    /// ends, a panic included, and the state writes with the window's identity
    /// only inside it. Witness: skip the restore in `Drop` (the write after the
    /// guard is refused too), or skip the swap in `enter` (the write inside is
    /// absorbed).
    #[test]
    fn the_window_identity_is_in_the_state_only_for_the_paint() {
        let mut bench = Bench::with_detached_book();
        bench.start_book();
        let book = bench.book();
        let (_, column) = first_column(&bench);
        let before = bench.app.state.cell_text(column, 0);
        bench.app.state.open_root_macro_for_test();
        let write = |app: &mut StokesForgeApp, text: &str| {
            app.state.dispatch(Action::CommitCell {
                column,
                row: 0,
                text: text.into(),
            });
        };
        {
            let mut guard = WindowIdentity::enter(&mut bench.app, Tab::Book(book));
            write(&mut guard, "11");
        }
        assert_eq!(
            bench.app.state.cell_text(column, 0),
            before,
            "inside the guard the macro's owner is another sender: refused"
        );
        // A panic during the paint restores it too.
        let app: &mut StokesForgeApp = &mut bench.app;
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = WindowIdentity::enter(app, Tab::Book(book));
            panic!("the paint failed");
        }));
        assert!(caught.is_err());
        write(&mut bench.app, "22");
        assert_eq!(
            bench.app.state.cell_text(column, 0),
            "22",
            "after the guard the main identity writes into its own macro"
        );
        assert!(
            bench.app.detached_books[&book].identity.is_some(),
            "the window got its identity back"
        );
        bench.app.state.cancel_plot_settings();
        bench.finish();
    }

    /// T10: a preference changed in the main window reaches the detached grid
    /// at once: the workbook reads the application's state, not a copy made when
    /// it was detached. The "?" help icon of the F(x)= cell is painted only with
    /// its preference on. Witness: a copy of the preferences kept by the window.
    #[test]
    fn a_preference_changed_in_the_main_window_reaches_the_detached_grid_at_once() {
        use stokesforge_app_state::state::HeaderField;
        let mut bench = Bench::with_detached_book();
        bench.start_book();
        let (_, column) = first_column(&bench);
        bench
            .app
            .state
            .dispatch(Action::SetShowColumnFormulaHelp(false));
        bench
            .app
            .state
            .begin_header_edit(column, HeaderField::Formula);
        let id = bench.book_id();
        bench.ctx.enable_accesskit();
        let images = |output: &egui::FullOutput| {
            output
                .platform_output
                .accesskit_update
                .as_ref()
                .map_or(0, |update| {
                    update
                        .nodes
                        .iter()
                        .filter(|(_, node)| node.role() == egui::accesskit::Role::Image)
                        .count()
                })
        };
        let mut off = 0;
        for _ in 0..4 {
            off = images(&bench.child_in(id, Pass::child()));
        }
        bench
            .app
            .state
            .dispatch(Action::SetShowColumnFormulaHelp(true));
        let mut on = 0;
        for _ in 0..4 {
            on = images(&bench.child_in(id, Pass::child()));
        }
        assert!(
            on > off,
            "the icon appears with the preference on ({off} -> {on})"
        );
        bench
            .app
            .state
            .dispatch(Action::SetShowColumnFormulaHelp(false));
        bench.finish();
    }

    /// L4 for the workbook, native Wayland: Reattach raised in the window's own
    /// pass alone floats the workbook again at the next root paint, and the
    /// window leaves egui at the end of its pass. Witness: flags drained at the
    /// root only.
    #[test]
    fn reattach_in_the_window_pass_alone_floats_the_workbook() {
        let mut bench = Bench::with_detached_book();
        bench.app.windows_close_themselves = true;
        bench.start_book();
        let (book, id) = (bench.book(), bench.book_id());
        let tab = Tab::Book(book);
        assert!(bench.app.detached_books[&book].self_owned);
        bench.app.detached_books.get_mut(&book).unwrap().reattach = true;
        let output = bench.child_in(id, Pass::child());
        assert!(!bench.app.is_tab_detached(tab), "absent from the list");
        assert!(!bench.app.open_viewports().contains(&id));
        assert!(!output.viewport_output.contains_key(&id), "egui dropped it");
        assert_eq!(delay(&output, egui::ViewportId::ROOT), Duration::ZERO);
        bench.root(Pass::root());
        assert!(bench.app.floating_tabs.contains(&tab));
        bench.finish();
    }

    /// X11 and Windows: without the Wayland switch the window raises the flag
    /// and stays until the root drains it. Witness: self-close on that path.
    #[test]
    fn without_wayland_the_root_reattaches_the_workbook() {
        let mut bench = Bench::with_detached_book();
        assert!(!bench.app.windows_close_themselves);
        bench.start_book();
        let (book, id) = (bench.book(), bench.book_id());
        let tab = Tab::Book(book);
        bench.app.detached_books.get_mut(&book).unwrap().reattach = true;
        bench.child_in(id, Pass::child());
        assert!(
            bench.app.is_tab_detached(tab),
            "still listed until the root"
        );
        bench.root(Pass::root());
        assert!(!bench.app.is_tab_detached(tab));
        assert!(bench.app.floating_tabs.contains(&tab));
        bench.finish();
    }
}
