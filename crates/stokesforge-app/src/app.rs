//! The `eframe` glue: the **orchestration** of a frame. [`StokesForgeApp`]
//! carries the presentation state (floating and detached windows, graph
//! views, Project Manager selection) around the [`AppState`], the sole
//! holder of the logic: everything that decides is in
//! [`stokesforge_app_state::state`], which is tested without a window. This
//! module keeps only what a move would break (#115, #122); each surface
//! lives in its module of `src/app/`, with its own top doc.
//!
//! # What stays here
//!
//! [`StokesForgeApp::ui`](eframe::App::ui), and with it:
//!
//! - **the order of the panels**, which makes the layout: with `egui::Panel`,
//!   the first panel placed is the outermost, each following one slots into
//!   what the previous ones leave. The order is: menu bar (`Panel::top`) →
//!   toolbars → status bar (`Panel::bottom`) → script console
//!   (`Panel::bottom`) → Project Manager (`Panel::left`, unless detached) →
//!   Object Manager (`Panel::right`, or floating) → `CentralPanel` last. The
//!   console, placed before the Project Manager, runs under it across the
//!   whole width; the Project Manager, placed before the `CentralPanel`, is
//!   never covered by the floating windows nor covers them. This order also
//!   gives `viewport_rect` its extent: what the fixed panels leave — the
//!   *visible* portion of the workspace, to be told apart from its fixed
//!   logical size (`WORKSPACE_SIZE`). Held by
//!   `fixed_panels_are_placed_in_the_order_of_ui`;
//! - **the disjoint borrows**: a `&mut self` method called in a closure that
//!   already borrows fields no longer compiles, hence the fields passed one by
//!   one (`show_workspace`);
//! - **the deferred application**: openings, detachments, closings and
//!   actions collected during painting are applied only once the frame is
//!   painted (mutating `floating_tabs` in the middle of the traversal would
//!   invalidate it), in the dispatch loop and its application-bound cases
//!   (Save without a path, New Project with entries in progress, opening of
//!   the plotted graph or the imported workbook);
//! - **the blocking confirmations** (new project, sheet deletion): they empty
//!   the frame's actions and cut the shortcuts, in a single place;
//! - **the global shortcuts** ([`StokesForgeApp::collect_shortcuts`]),
//!   consumed at the context level, before the widgets;
//! - the **draining of the detached windows** (Project Manager and content
//!   tabs), and the scheduled root frame when a command applied after
//!   painting changes the snapshot; the publication itself, which wakes the
//!   reader viewports, lives in `window_detach.rs` (#104).
//!
//! The `tests` module at the end of the file tests only this orchestration;
//! each surface has its tests in its module (tests follow their code, #123).
//! Since #124, `ui()` makes this order explicit and delegates the recovery,
//! dialog and deferred application phases to private methods. These
//! extractions add neither a sub-Ui nor an egui id.
//!
//! # Where the rest lives
//!
//! The complete module → role table is in
//! `docs/application/shell-and-code.md`, § Code split.
//!
//! - menus and status bar: `menus.rs` ([`StokesForgeApp::menu_bar`],
//!   [`StokesForgeApp::status_bar`]); format bars and contextual bar:
//!   `format.rs`; the bars themselves: `crate::toolbar`;
//! - fixed panels, with their content: `project_manager.rs`
//!   (`project_manager_panel`, the tree `project_manager_contents_from_tree`),
//!   `script_console.rs` (`script_panel`), `object_manager_ui.rs`
//!   (`object_manager_panel`, `run_object_manager_request`);
//! - content windows: `tab.rs` (the four kinds, [`Tab::Book`], [`Tab::Graph`],
//!   [`Tab::Matrix`], [`Tab::Note`]), `contents.rs` (`tab_contents`, the note,
//!   and `matrix_contents`, documented placeholder), `workbook.rs`, `grid.rs`,
//!   `graph_window.rs`, `graph_text_edit.rs`, `file_drop.rs`; placement,
//!   stack, maximization and frame: `window_placement.rs`, `window_stack.rs`,
//!   `window_chrome.rs`; detaching into OS windows: `window_detach.rs`;
//! - dialogs: `dialogs.rs` (common building block), `import_wizard.rs`,
//!   `plot_settings.rs`; declared shortcuts: `shortcuts.rs`.
//!
//! [`StokesForgeApp::open_tab`] (`window_stack.rs`) is the only place that
//! decides "open" vs "focus" a tab, whatever its kind.
//!
//! # What Qt gave for free and has to be written here
//!
//! A `QAction` linked to a `QUndoStack` carried at once the keyboard shortcut,
//! the menu entry, its graying and its label ("Undo *Rename*"). Here the four
//! are separate: the shortcut is declared (`shortcuts.rs`) and consumed by
//! hand ([`StokesForgeApp::collect_shortcuts`]), the entry and its graying
//! (`add_enabled`) are in `menus.rs`, the label comes from
//! [`AppState::undo_label`], and nothing guarantees their consistency except
//! proofreading — and the tests of `menus.rs`, which read the graying of each
//! top-level menu.
//!
//! # Fixed panels (#100)
//!
//! PM stays on the left, OM on the right and the console at the bottom.
//! Collapsing keeps a visible handle; hiding is commanded from View. The
//! dimensions of the expanded panel are kept by distinct strip ids. Each
//! panel can detach into an OS window with Reattach only; no side switch nor
//! internal floating. `panel_header` and the reads and requests of
//! `window_detach.rs` ensure the agreement between the two modes.
//!
//! # Native file picker
//!
//! The "File" menu opens the native `rfd::FileDialog` for "Open…" and
//! "Save As…" (modal, like any desktop file picker — blocking the rendering
//! while the user chooses a file is the expected behavior, not to be confused
//! with the blocking that `RunScript` avoids, which has no reason to last).
//! Ctrl+O follows "Open…" exactly, Ctrl+Shift+S "Save As…", while Ctrl+S
//! keeps the quick gesture of saving to the already known path.
//!
//! # Accepted limits of the bench
//!
//! - **Shortcuts are consumed at the context level**, hence *before* the
//!   widgets. Measurable consequence: Ctrl+Z while typing in a cell undoes the
//!   last **model command**, not the last keystroke in the field — the
//!   internal undo of egui's `TextEdit`s is masked. This is the expected
//!   behavior of a spreadsheet, and it is also the limit: the bench cannot
//!   tell "undo my entry in progress" from "undo the last command".
//! - **Only one cell is editable at a time** (all grids of all open
//!   workbooks together). The grid is virtualized
//!   (`TableBuilder::body().rows()`), so there is no question of keeping a
//!   buffer per cell: a sheet can have a million rows.
//! - **No persistence of the layout between two launches** —
//!   `StokesForgeApp::new` rebuilds it at every startup.
//!
//! # The script console does not freeze the window
//!
//! `ScriptEngine::run` **blocks** until the script ends; calling it from this
//! render loop would freeze the whole window, "Stop" button included — the
//! most visible defect this console could have. The remedy is **not** here: it
//! lives in [`stokesforge_app_state::state::AppState::dispatch`], which
//! handles `RunScript` by launching the work on a detached thread and
//! returning at once (see the top doc of `stokesforge_app_state::state`). This
//! module thus has only two things to do, every frame: call
//! [`AppState::poll_script`] to collect a ready result, and request a new
//! rendering ([`egui::Context::request_repaint`]) as long as a script runs —
//! without which, egui redrawing only on an input event, the end of the
//! script would appear only at the next mouse movement.

mod about_window;
mod analysis_dialog;
mod analysis_forms;
mod analysis_handles;
mod analysis_menu;
mod analysis_preview;
mod busy;
mod column_formula_help;
mod column_use;
mod confirmations;
mod contents;
mod cursors;
mod curve_settings;
mod dialogs;
mod empty_state;
mod figure_copy;
mod file_drop;
mod form;
mod format;
mod graph_targets;
mod graph_text_edit;
mod graph_window;
mod grid;
pub(crate) mod hover_help;
mod import_wizard;
mod inplace_edit;
mod keyboard_shortcuts_window;
mod list_row;
#[cfg(test)]
mod manual_pages;
#[cfg(test)]
mod manual_screenshots;
mod menu_sections;
mod menus;
mod object_dnd;
mod object_manager_ui;
mod panel_memory;
#[cfg(test)]
mod panel_tests;
mod plot_settings;
#[cfg(test)]
mod preferences_proto;
mod preferences_window;
mod project_manager;
mod recovery;
mod reorder_list;
mod results_pane;
mod script_console;
pub(crate) mod shortcuts;
mod stale_export;
mod status_history;
mod tab;
pub(crate) mod tab_strip;
#[cfg(test)]
mod test_support;
mod transpose_ui;
mod window_chrome;
mod window_detach;
mod window_placement;
mod window_stack;
mod workbook;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use about_window::about_window;
use analysis_dialog::{analysis_replace_window, analysis_window, calculation_delete_window};
use column_use::{column_use_window, full_copy_window};
use file_drop::{
    begin_file_drop, finish_file_drop, open_project_in_new_window, remember_recent_imports,
};
use graph_text_edit::{
    annotation_editor_id, pending_format_id, pending_zone_font_id, text_editor_id,
    AnnotationEditor, TextEditor,
};
use graph_window::request_graph_export;
use grid::keyboard_focus_outside_grid;
use import_wizard::import_window;
use keyboard_shortcuts_window::keyboard_shortcuts_window;
use object_manager_ui::{select_manager_target, selected_manager_target};
use plot_settings::{column_metadata_window, plot_settings_window};
use preferences_window::preferences_window;
use recovery::recovery_window;
use shortcuts::{
    collect_undo_redo_shortcuts, SHORTCUT_DUPLICATE_SHEET, SHORTCUT_EXPORT_GRAPH, SHORTCUT_IMPORT,
    SHORTCUT_IMPORT_QWERTY, SHORTCUT_IMPORT_TEXT, SHORTCUT_NEW_PROJECT, SHORTCUT_OPEN,
    SHORTCUT_REDO_Y, SHORTCUT_RUN_SCRIPT, SHORTCUT_SAVE, SHORTCUT_SAVE_AS, SHORTCUT_UNDO,
};
use stale_export::stale_export_window;
use status_history::{low_r2_list_window, status_history_window};
use stokesforge_app_state::file_picker::{FilePicker, PickRequest};
use tab::{active_tab_kind, tab_for_window, tab_node, Tab};
use transpose_ui::transpose_relink_window;
use window_detach::{
    snapshot_stamp, DetachedProjectManagerRequests, DetachedTab, ProjectManagerViewModel,
};
use window_placement::geometry_rect;
use window_stack::window_nodes;

use eframe::egui;
use stokesforge_core::{Node, NodeId};
use stokesforge_render_egui::GraphView;
use stokesforge_session::WindowGeometry;

use crate::color_picker::publish_saved_colors;
use crate::toolbar::{BarEnv, ToolbarLayout, ToolbarState};
use stokesforge_app_state::errors;
use stokesforge_app_state::i18n;
use stokesforge_app_state::preferences::Preferences;
use stokesforge_app_state::state::{
    active_graph, Action, AppState, ImportDestination, UnsavedChoice,
};
use stokesforge_app_state::tr;

/// Environment variable of the auto-close mechanism (see
/// [`StokesForgeApp::new`]).
pub const AUTOQUIT_ENV: &str = "STOKESFORGE_APP_AUTOQUIT_MS";
/// Environment variable of the capture mechanism (see
/// [`StokesForgeApp::screenshot`]).
pub const SCREENSHOT_ENV: &str = "STOKESFORGE_APP_SCREENSHOT";
/// Frames let through before requesting a capture.
///
/// egui resolves the size of its panels over two or three frames: capturing
/// the first would show a transient layout — and, for a graph tab, a cache
/// counter that has not yet had the chance to serve an unchanged frame. We
/// look at the steady state, not the startup.
const SCREENSHOT_SETTLE_FRAMES: u32 = 8;

/// The `eframe` application: the bench's state, the layout in confined
/// floating windows, and the strict minimum for rendering.
pub struct StokesForgeApp {
    /// The state, sole holder of the logic.
    state: AppState,
    /// Native file picker in progress, on a separate thread (#192).
    file_picker: FilePicker,
    /// Opens a folder in the system's file manager (Preferences › Folders);
    /// replaced in the tests, which must not start a real program.
    folder_opener: fn(&std::path::Path) -> std::io::Result<()>,
    /// Last title sent to the window (S-R15): `Title` is resent only on change.
    window_title: String,
    /// The toolbars: layout, visibility and drag in progress
    /// (issue #60).
    toolbars: ToolbarState,
    om_detached: bool,
    om_collapsed: bool,
    om_view: Option<window_detach::DetachedObjectManager>,
    /// Visibility and collapse are view states, outside history and the .stf format.
    pm_open: bool,
    pm_collapsed: bool,
    script_open: bool,
    script_detached: bool,
    script_view: Option<window_detach::DetachedScriptConsole>,
    active_detached: Option<Tab>,
    /// The content tabs currently open as floating windows, in the order in
    /// which they were last focused — the last element is the most recently
    /// focused (see the top doc of `window_stack.rs`, § Confined floating
    /// windows).
    /// Absent while maximized or detached into a separate OS window
    /// (same invariant as `detached_tabs` for this second case).
    floating_tabs: Vec<Tab>,
    /// Cascade rank assigned to each currently open floating window, at its
    /// creation — used to compute its default position
    /// (`StokesForgeApp::ui`); it no longer moves afterwards, including if the
    /// user moves the window (`egui` then remembers its position itself, via
    /// its `Id`). An entry disappears from this table at the same time as from
    /// `floating_tabs`.
    cascade_rank: HashMap<Tab, u32>,
    /// Next cascade rank to hand out — incremented at each new floating window
    /// created (not at a simple refocus), reset to zero every
    /// [`CASCADE_WRAP`](crate::app::window_placement::CASCADE_WRAP) so as not to drift out of the work area.
    next_cascade: u32,
    /// Tab to bring to the foreground at the next rendering — set by
    /// [`StokesForgeApp::open_tab`] when the reopened tab was already an
    /// existing floating window (see the top doc of `window_stack.rs`).
    focus_next: Option<Tab>,
    /// A graph view (and its shape cache, phase R4) per **open** graph — that
    /// is, one that has had a tab at least once, the entry survives the
    /// closing of the tab. One cache per view is the rule
    /// (`stokesforge_render_egui::GraphView`); with several graphs displayable
    /// at once, a single view is no longer enough.
    graph_views: HashMap<NodeId, GraphView>,
    /// Tabs (`Graph` or `Book`, B5 — `retours-2026-09-23-bugs.md`)
    /// whose content alone is collapsed. This presentation state stays local
    /// to the application: it modifies neither the node, nor its saved
    /// geometry, nor the other windows.
    collapsed_tabs: HashSet<Tab>,
    /// Currently maximized tab, if there is one — see the top doc of
    /// `window_stack.rs`, § Maximizing a tab. Can no longer be the Project
    /// Manager since this task: it is no longer a tab, and a fixed panel
    /// always fully visible has nothing to gain from being "maximized".
    maximized_tab: Option<Tab>,
    /// True when the Project Manager is detached into a separate OS window (see
    /// the top doc of `window_detach.rs`, § Detaching the Project Manager). In this state,
    /// [`StokesForgeApp::project_manager_panel`] is simply not painted.
    pm_detached: bool,
    /// True when the script console is collapsed to its title bar alone
    /// (issue #79). Collapsed at the first launch, then remembered with the
    /// other panels (#148, `panel_memory.rs`); launching a script expands it,
    /// so that the output is seen.
    script_collapsed: bool,
    /// Memory of the layout of the three panels between two launches (#148):
    /// live geometry of the detached windows, where they open, what was last
    /// handed to the preferences. The collapsed and detached flags above stay
    /// the single source; `panel_memory.rs` reads them and sends
    /// `Action::RememberPanelLayout` on every change.
    panel_memory: panel_memory::PanelMemory,
    /// What the detached OS window of the Project Manager paints, resynchronized
    /// from `self.state` at every frame by
    /// [`StokesForgeApp::show_detached_project_manager`] — see its top doc.
    /// `Arc<Mutex<_>>` only because the `show_viewport_deferred` callback
    /// requires it (`Fn` + `Send + Sync + 'static`, no capture of `&mut
    /// self`); it is **not** `AppState` itself behind a lock,
    /// only this strict subset.
    pm_view_model: Arc<Mutex<ProjectManagerViewModel>>,
    /// What the detached OS window of the Project Manager writes (open
    /// clicks, reattach request), drained every frame by
    /// [`StokesForgeApp::ui`](eframe::App::ui) — same role as `open_requests`/`actions` for the
    /// rest of the bench, just set by a deferred callback rather than collected
    /// directly.
    pm_requests: Arc<Mutex<DetachedProjectManagerRequests>>,
    /// Content tabs (`Book`/`Graph`/`Matrix`/`Note`) currently
    /// detached into a separate OS window — see the top doc of
    /// `window_detach.rs`, § Generic detaching of content tabs. One entry per detached tab; absent from
    /// `floating_tabs` while it is (same invariant as `pm_detached`
    /// for the Project Manager, generalized to several simultaneous tabs).
    detached_tabs: HashMap<Tab, DetachedTab>,
    /// Position of each floating window, in logical coordinates of the
    /// workspace, moved by the house title bar and bounded by
    /// `clamp_to_workspace` — sole source of truth of the position.
    window_positions: HashMap<Tab, egui::Pos2>,
    /// Logical size of each floating window, modified by the house resize
    /// handles ([`floating_resize_handles`](crate::app::window_chrome::floating_resize_handles),
    /// 4 sides + 4 corners) — sole source of truth of the size, as
    /// `window_positions` is of the position.
    window_sizes: HashMap<Tab, egui::Vec2>,
    /// The figure each open graph window last painted (size and display
    /// scale), from which the window follows the figure when its size changes
    /// (#394). Window geometry: stays in the shell (invariant 2). Cleared with
    /// the windows when a project is opened, so that opening resizes nothing.
    figure_follow: HashMap<NodeId, window_placement::FigureFollow>,
    /// Current scroll of the workspace (MDI area, fixed size
    /// [`WORKSPACE_SIZE`](crate::app::window_placement::WORKSPACE_SIZE)) — see its doc. `window_positions`/`window_sizes`
    /// above, and the geometry persisted in the project, are expressed
    /// in the logical coordinates of this space, not in screen pixels;
    /// [`StokesForgeApp::ui`](eframe::App::ui) applies this offset at painting. Transient, like the scroll position of an
    /// `egui::ScrollArea`: never persisted.
    workspace_scroll: egui::Vec2,
    /// Last geometry (with its state) sent to the actor, so as not to send it
    /// a request at every frame while a window stays
    /// still.
    saved_window_geometries: HashMap<NodeId, WindowGeometry>,
    /// Last known **floating** rectangle of each window, in
    /// workspace coordinates. It survives closing,
    /// maximizing and detaching (issue #16) and takes precedence over the snapshot,
    /// which `AppState::set_window_geometry` does not refresh: without it, a
    /// reopened window would take the geometry of the project load again.
    floating_rects: HashMap<NodeId, egui::Rect>,
    /// Current selection in the Project Manager tree (§6) — Ctrl/Cmd-click
    /// toggles membership; consumed by "Show"/"Hide".
    /// Independent of `selection.rs`, specific to the data grid.
    pm_selection: HashSet<project_manager::PmEntry>,
    /// Last active content observed to follow the simple selection (#146).
    pm_last_active_tab: Option<Tab>,
    /// File where to drop a capture of the window, if one is requested
    /// (verification mechanism, see [`StokesForgeApp::screenshot`]).
    screenshot: Option<std::path::PathBuf>,
    /// Frames remaining to let through before requesting the capture.
    screenshot_delay: u32,
    /// Auto-close delay, if requested (verification mechanism).
    autoquit: Option<Duration>,
    /// Instant of the first rendering, from which the auto-close delay is measured.
    first_frame: Option<Instant>,
    /// The window asked to close (cross, Alt+F4, Quit): to be posted to
    /// the state like `Action::Quit` (#229).
    close_requested: bool,
    /// The state released the closing: the next request is no longer held back.
    quit_confirmed: bool,
}

impl StokesForgeApp {
    /// Builds the application around a state.
    ///
    /// # Initial layout
    ///
    /// Neither the Project Manager nor the script console enter into this
    /// plan: they are fixed panels painted separately (see
    /// [`StokesForgeApp::project_manager_panel`] and [`StokesForgeApp::script_panel`]),
    /// not floating windows. If the project contains at least one
    /// workbook, its tab opens as a floating window, followed by the tab of
    /// the first graph if there is one — showing something without anyone
    /// having to click, like the former default behavior. Without any
    /// workbook or graph, no floating window opens (only the fixed
    /// panels and the menu bar are visible).
    ///
    /// # Auto-close
    ///
    /// If the environment variable `STOKESFORGE_APP_AUTOQUIT_MS` contains an
    /// integer, the window closes by itself after this delay in
    /// milliseconds, measured from the first rendering. **It is a verification
    /// mechanism, not a feature**: it makes it possible to prove from the
    /// command line that the window opens and really paints — which no
    /// `cargo` test can do — without a human having to click. An unreadable
    /// value is silently ignored: the bench must not refuse
    /// to start because of verification tooling.
    pub fn new(state: AppState) -> Self {
        let autoquit = std::env::var(AUTOQUIT_ENV)
            .ok()
            .and_then(|raw| raw.trim().parse::<u64>().ok())
            .map(Duration::from_millis);

        let saved_window_geometries: HashMap<NodeId, WindowGeometry> =
            (*state.snapshot().window_geometries).clone();
        let floating_rects: HashMap<NodeId, egui::Rect> = saved_window_geometries
            .iter()
            .map(|(node, geometry)| (*node, geometry_rect(geometry)))
            .collect();
        let mut floating_tabs = Vec::new();
        let mut cascade_rank = HashMap::new();
        let mut next_cascade = 0u32;
        let mut collapsed_tabs = HashSet::new();
        let mut maximized_tab = None;
        let mut open_initial = |tab: Tab| {
            floating_tabs.push(tab);
            cascade_rank.insert(tab, next_cascade);
            next_cascade = next_cascade.wrapping_add(1);
        };
        if saved_window_geometries
            .values()
            .any(|geometry| geometry.view.is_some())
        {
            // Project saved with the state of the windows (issue #16): it is
            // restored as is, a detached window coming back floating.
            let mut restored: Vec<(u32, Tab, stokesforge_session::WindowView)> = Vec::new();
            for (node, kind) in window_nodes(&state.project_tree()) {
                let Some(view) = saved_window_geometries.get(&node).and_then(|g| g.view) else {
                    continue;
                };
                if view.mode != stokesforge_session::WindowMode::Closed {
                    restored.push((view.stack, tab_for_window(node, kind), view));
                }
            }
            restored.sort_by_key(|(stack, tab, _)| (*stack, tab_node(*tab).raw()));
            for (_, tab, view) in restored {
                open_initial(tab);
                if view.collapsed {
                    collapsed_tabs.insert(tab);
                }
                if view.mode == stokesforge_session::WindowMode::Maximized {
                    maximized_tab = Some(tab);
                }
            }
        } else {
            if let Some((first_book, _)) = state.books().into_iter().next() {
                open_initial(Tab::Book(first_book));
            }
            if let Some((first_graph, _)) = state.graphs().into_iter().next() {
                open_initial(Tab::Graph(first_graph));
            }
        }
        // Without a workbook or graph to show, no floating window
        // opens — neither the script console nor the Project Manager has its
        // place here, they are fixed panels.

        let window_title = state.window_title();
        Self {
            state,
            file_picker: FilePicker::default(),
            folder_opener: open_folder_in_file_manager,
            window_title,
            toolbars: ToolbarState::default(),
            om_detached: false,
            om_collapsed: false,
            om_view: None,
            pm_open: true,
            pm_collapsed: false,
            script_open: true,
            script_detached: false,
            script_view: None,
            active_detached: None,
            floating_tabs,
            cascade_rank,
            next_cascade,
            focus_next: None,
            graph_views: HashMap::new(),
            collapsed_tabs,
            maximized_tab,
            pm_detached: false,
            script_collapsed: true,
            panel_memory: panel_memory::PanelMemory::default(),
            pm_view_model: Arc::new(Mutex::new(ProjectManagerViewModel::default())),
            pm_requests: Arc::new(Mutex::new(DetachedProjectManagerRequests::default())),
            detached_tabs: HashMap::new(),
            window_positions: HashMap::new(),
            window_sizes: HashMap::new(),
            figure_follow: HashMap::new(),
            workspace_scroll: egui::Vec2::ZERO,
            saved_window_geometries,
            floating_rects,
            pm_selection: HashSet::new(),
            pm_last_active_tab: None,
            autoquit,
            first_frame: None,
            close_requested: false,
            quit_confirmed: false,
            screenshot: std::env::var_os(SCREENSHOT_ENV).map(std::path::PathBuf::from),
            screenshot_delay: SCREENSHOT_SETTLE_FRAMES,
        }
    }

    /// Opens as windows the workbooks added at the root since it
    /// had `before` children (import into new workbooks).
    fn open_new_workbooks_after(&mut self, before: usize) {
        let root = self.state.tree().root();
        let new_books: Vec<NodeId> = self
            .state
            .tree()
            .node(root)
            .map(|node| node.children().iter().skip(before).copied().collect())
            .unwrap_or_default();
        for book in new_books {
            if self.state.tree().node(book).is_some_and(|node| {
                matches!(
                    node.kind(),
                    stokesforge_core::node::NodeKind::Workbook { .. }
                )
            }) {
                self.open_tab(Tab::Book(book));
            }
        }
    }

    /// Opens the import wizard on the active workbook (maximized, or in the
    /// foreground, or the only detached workbook) — File menu and Ctrl+".
    fn open_import_dialog(&mut self) {
        let active_book = self.active_book();
        self.state.open_import(active_book);
    }

    /// The active workbook: maximized, otherwise in the foreground, otherwise the only
    /// detached workbook.
    fn active_book(&self) -> Option<NodeId> {
        let mut detached_books = self.detached_tabs.keys().filter_map(|tab| match tab {
            Tab::Book(book) => Some(*book),
            _ => None,
        });
        let only_detached_book = detached_books
            .next()
            .filter(|_| detached_books.next().is_none());
        self.maximized_tab
            .or_else(|| self.floating_tabs.last().copied())
            .and_then(|tab| match tab {
                Tab::Book(book) => Some(book),
                _ => None,
            })
            .or(only_detached_book)
    }

    /// Consumes the shortcuts at the context level and turns them into actions.
    fn collect_shortcuts(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if self.state.blocking_dialog() {
            return;
        }
        let mut save_as = false;
        let mut open = false;
        let mut import = false;
        let mut export = false;
        // The sheet gestures do not apply during an entry in a
        // field, nor during the editing of a tab name.
        let typing = keyboard_focus_outside_grid(ctx)
            || self.state.renaming_sheet().is_some()
            || project_manager::has_focus(ctx);
        let active_sheet = self.active_content_tab(ctx).and_then(|tab| match tab {
            Tab::Book(book) => self.state.selected_sheet_in(book),
            _ => None,
        });
        ctx.input_mut(|input| {
            if input.consume_shortcut(&SHORTCUT_NEW_PROJECT) && self.state.can_new_project() {
                actions.push(Action::NewProject {
                    pending_edits: false,
                });
            }
            // Ctrl+Shift+S must be tested before Ctrl+S: the extra
            // modifiers stay compatible with the latter.
            if input.consume_shortcut(&SHORTCUT_SAVE_AS) {
                save_as = true;
            } else if input.consume_shortcut(&SHORTCUT_SAVE) {
                actions.push(Action::Save);
            }
            if input.consume_shortcut(&SHORTCUT_OPEN) {
                open = true;
            }
            if input.consume_shortcut(&SHORTCUT_EXPORT_GRAPH) {
                export = true;
            }
            if input.consume_shortcut(&SHORTCUT_RUN_SCRIPT) {
                actions.push(Action::RunScript);
            }
            if !typing {
                if let Some(sheet) = active_sheet {
                    if input.consume_shortcut(&SHORTCUT_DUPLICATE_SHEET) {
                        actions.push(Action::DuplicateSheet(sheet));
                    }
                }
            }
            if input.consume_shortcut(&SHORTCUT_IMPORT)
                || input.consume_shortcut(&SHORTCUT_IMPORT_QWERTY)
            {
                import = true;
            }
        });
        if import && self.state.import_state().is_none() {
            self.open_import_dialog();
        }
        collect_undo_redo_shortcuts(ctx, actions);
        if save_as {
            self.choose_save_path(actions);
        }
        if open {
            self.choose_open_path(actions);
        }
        // The active graph, computed as for the menu bar: the
        // shortcut acts only where the "Graph" menu exists.
        let active_tab = self.active_content_tab(ctx);
        if let Some(graph) = active_graph(active_tab_kind(active_tab)).filter(|_| export) {
            self.request_graph_export(graph, actions);
        }
    }

    /// Asks for the file to open: the picker opens on a separate thread
    /// (#192), the answer comes back through [`StokesForgeApp::poll_file_picker`].
    fn choose_open_path(&mut self, actions: &mut Vec<Action>) {
        actions.push(Action::PickFile(PickRequest::OpenProject));
    }

    /// Takes up the toolbar layout from the user's preferences. Separate from
    /// [`StokesForgeApp::new`] so that the tests do not read the machine's
    /// file.
    pub fn load_toolbar_layout(&mut self, preferences: &Preferences) {
        let layout = preferences
            .toolbars()
            .map_or_else(ToolbarLayout::default, ToolbarLayout::from_json);
        self.toolbars = ToolbarState::new(layout);
    }

    /// Saves the bar layout into `preferences.json`; a failure
    /// is told in the status bar, the layout stays applied.
    fn save_toolbar_layout(&mut self) {
        let layout = self.toolbars.layout().to_json();
        if let Err(err) = Preferences::load().save_toolbars(layout) {
            self.state.notice(tr!(
                "status-toolbars-not-saved",
                detail = errors::io::error_message(&err)
            ));
        }
    }

    /// Applies the language right away (the texts are redrawn at the next
    /// frame) and saves it into `preferences.json`. A save
    /// that fails is told in the status bar; the language stays applied.
    fn choose_language(&mut self, language: i18n::Language) {
        i18n::set_language(language);
        if let Err(err) = Preferences::load().save_language(language.tag()) {
            self.state.notice(tr!(
                "status-language-not-saved",
                detail = errors::io::error_message(&err)
            ));
        }
    }

    /// Asks for the save destination; same circuit as for opening.
    fn choose_save_path(&mut self, actions: &mut Vec<Action>) {
        actions.push(Action::PickFile(PickRequest::SaveProjectAs {
            suggested: suggested_file_name(self.state.path()),
        }));
    }

    /// Starts the requested pickers, then applies the answer of the one that
    /// has just finished (#192). Never blocks.
    fn poll_file_picker(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if self.file_picker.is_open() {
            // Safety net: the thread wakes the interface, this covers a lost wake-up.
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        let Some((request, picked)) = self.file_picker.poll() else {
            return;
        };
        let Some(paths) = picked else {
            // Without a chosen file, a gesture that was waiting for the save
            // (Save answer, #229) is abandoned.
            if matches!(request, PickRequest::SaveProjectAs { .. }) {
                actions.push(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
            }
            return;
        };
        let first = paths.first().cloned();
        // Only file dialogs feed the "last folder" (not a drop, not a recent file).
        self.state.remember_picked_folder(&request, &paths);
        match request {
            PickRequest::OpenProject => {
                if let Some(path) = first {
                    actions.push(Action::OpenProject {
                        path: path.display().to_string(),
                        pending_edits: false,
                    });
                }
            }
            PickRequest::SaveProjectAs { .. } => {
                if let Some(path) = first {
                    actions.push(Action::SaveAs {
                        path: path.display().to_string(),
                    });
                }
            }
            PickRequest::ExportGraph { spec, .. } => {
                if let Some(path) = first {
                    actions.push(Action::ExportGraph { path, spec });
                }
            }
            PickRequest::ExportImportRecipe { .. } => {
                if let Some(path) = first {
                    self.state.export_import_recipe(&path);
                }
            }
            PickRequest::ImportImportRecipe => {
                if let Some(path) = first {
                    self.state.import_import_recipe(&path);
                }
            }
            PickRequest::AddImportFiles => self.state.add_import_paths(paths),
            PickRequest::ChooseDefaultFolder { kind } => {
                if let Some(path) = first {
                    self.state.apply_default_folder_pick(kind, &path);
                }
            }
        }
    }

    fn active_content_tab(&mut self, ctx: &egui::Context) -> Option<Tab> {
        if !ctx.input(|input| input.focused) {
            for (&tab, detached) in &self.detached_tabs {
                if detached.view_model.lock().is_ok_and(|model| model.focused) {
                    self.active_detached = Some(tab);
                }
            }
        }
        self.active_detached
            .filter(|tab| self.detached_tabs.contains_key(tab))
            .or(self.maximized_tab)
            .or_else(|| self.floating_tabs.last().copied())
    }

    /// The bars and the export also read the detached view targeted by the panel.
    fn read_graph_view<R>(&self, graph: NodeId, read: impl FnOnce(&GraphView) -> R) -> Option<R> {
        if let Some(detached) = self.detached_tabs.get(&Tab::Graph(graph)) {
            let model = detached
                .view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            model.graph_view.as_ref().map(read)
        } else {
            self.graph_views.get(&graph).map(read)
        }
    }

    fn request_graph_export(&mut self, graph: NodeId, actions: &mut Vec<Action>) {
        let tree = self.state.snapshot().tree.clone();
        self.with_graph_view(graph, |view| {
            request_graph_export(Some(view), &tree, graph, actions)
        });
    }

    /// Same selection view for floating and detached graphs.
    fn with_graph_view<R>(
        &mut self,
        graph: NodeId,
        run: impl FnOnce(&mut GraphView) -> R,
    ) -> Option<R> {
        if let Some(detached) = self.detached_tabs.get(&Tab::Graph(graph)) {
            let mut model = detached
                .view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            Some(run(model.graph_view.get_or_insert_with(GraphView::default)))
        } else {
            Some(run(self.graph_views.entry(graph).or_default()))
        }
    }

    fn selected_series(&self, graph: NodeId) -> Option<stokesforge_plot::SeriesRef> {
        self.read_graph_view(graph, GraphView::selected_series)
            .flatten()
    }

    /// Drops a capture of the window into the file named by
    /// `STOKESFORGE_APP_SCREENSHOT`, then asks for no more.
    ///
    /// **Verification mechanism, not a feature** — exactly like the
    /// auto-close, and for a neighboring need that appeared with
    /// phase R4: on a rendering work package, "the window opens and does not
    /// crash" is no longer enough, one must be able to *look* at what it
    /// paints. The tests cover the geometry (`stokesforge-render-egui`), not
    /// the appearance; and a capture taken by the window itself is the
    /// only way to get at it under Wayland, where no external tool can
    /// photograph a given window.
    ///
    /// The format is **binary PPM** (`P6`), seven lines of code and no new
    /// dependency. Converting it to PNG is a matter of
    /// `magick capture.ppm capture.png`; bringing a PNG encoder into
    /// the bench for a verification tool would be paying dearly for a comfort.
    ///
    /// A write error is reported on the error output and nothing
    /// more: the bench must not stop because verification tooling
    /// failed.
    fn screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else {
            return;
        };
        if self.screenshot_delay > 0 {
            self.screenshot_delay -= 1;
            ctx.request_repaint();
            return;
        }
        // A capture arrives at the frame *following* the request: as long as
        // nothing has been received, we ask again.
        let image = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let Some(image) = image else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            ctx.request_repaint();
            return;
        };
        self.screenshot = None;

        let [width, height] = image.size;
        let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
        for pixel in &image.pixels {
            let [r, g, b, _] = pixel.to_srgba_unmultiplied();
            ppm.extend_from_slice(&[r, g, b]);
        }
        match std::fs::write(&path, ppm) {
            Ok(()) => eprintln!("[stokesforge-app] capture written: {}", path.display()),
            Err(err) => eprintln!(
                "[stokesforge-app] capture impossible in {}: {err}",
                path.display()
            ),
        }
    }

    /// Closes the window once the state has accepted Quit (#229).
    fn close_when_ready(&mut self, ctx: &egui::Context) {
        if self.state.take_quit_ready() {
            self.quit_confirmed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Closes the window if the auto-close delay has elapsed.
    fn autoquit(&mut self, ctx: &egui::Context) {
        let Some(delay) = self.autoquit else {
            return;
        };
        let start = *self.first_frame.get_or_insert_with(Instant::now);
        let elapsed = start.elapsed();
        if elapsed >= delay {
            // Verification tooling: asks for nothing (#229).
            self.quit_confirmed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            // Without an input event, egui does not redraw: it must be
            // asked to wake up, otherwise the deadline never arrives.
            ctx.request_repaint_after(delay - elapsed);
        }
    }
}

/// Opens `folder` in the system's file manager, creating it first if it does not
/// exist yet (the configuration folder is only created by the first write). The
/// program is started and left alone; a thread waits for it so that it does not
/// stay a zombie.
fn open_folder_in_file_manager(folder: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let program = "xdg-open";
    let mut child = std::process::Command::new(program)
        .arg(folder)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

impl eframe::App for StokesForgeApp {
    /// A window close (cross, Alt+F4, File › Quit) goes through the
    /// question "Discard Changes · Cancel · Save" (S-F3, #229): held back here
    /// (also with the window minimized, where only `logic` runs), it becomes
    /// `Action::Quit`, and the window closes when the state releases it.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.quit_confirmed && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_requested = true;
        }
        // Autosave (#317): here and not in `ui()`, which does not run
        // with the window minimized; the wake-up is scheduled for the next
        // deadline, no periodic redraw at rest.
        if let Some(delay) = self.state.poll_autosave(Instant::now()) {
            ctx.request_repaint_after(delay);
        }
    }

    /// The sizes and positions the detached panels reported since the last
    /// change of their state (#148).
    fn on_exit(&mut self) {
        self.flush_panel_layout();
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // `Context` is an `Arc`: the clone avoids the read borrow of the painted Ui.
        let ctx = ui.ctx().clone();
        self.sync_window_title_and_status(&ctx);
        self.prepare_panel_windows();
        let published_stamp = snapshot_stamp(self.state.snapshot());
        let mut actions = Vec::new();
        // These queues are applied only after the floating stack is painted.
        let mut open_requests = Vec::new();
        let mut detach_requests = Vec::new();
        let mut close_requests = Vec::new();
        let mut geometry_updates = Vec::new();
        let active_tab = self.active_content_tab(&ctx);
        self.follow_project_manager_active_tab(&ctx, active_tab);

        begin_file_drop(&ctx);
        self.poll_background_jobs(&ctx);
        self.poll_file_picker(&ctx, &mut actions);
        if std::mem::take(&mut self.close_requested) {
            actions.push(Action::Quit {
                pending_edits: false,
            });
        }
        publish_saved_colors(&ctx, &self.state);
        let (metadata, x_prime) = self.state.workbook_header_defaults();
        grid::publish_header_row_defaults(&ctx, metadata, x_prime);
        analysis_menu::publish_last_presets(&ctx, &self.state);
        graph_targets::publish(&ctx, &self.floating_tabs, self.detached_tabs.keys());
        self.collect_shortcuts(&ctx, &mut actions);
        // Ctrl+C on the active graph, before panels and contents read
        // the `Event::Copy` (#149).
        self.request_active_figure_copy(&ctx, active_tab, &mut actions);

        // The order of these calls determines the extent of the egui panels (#115).
        self.menu_bar(ui, &ctx, &mut actions, active_tab);
        self.show_toolbars(ui, &ctx, active_tab, &mut actions);
        self.status_bar(ui, &mut actions);
        self.script_panel(ui, &ctx, &mut actions);
        self.show_project_manager(
            ui,
            &ctx,
            active_tab,
            &mut open_requests,
            &mut close_requests,
            &mut actions,
        );
        self.object_manager_panel(ui, active_tab, &mut actions);
        self.drain_detached_tabs(&ctx, &mut actions);
        self.show_workspace(
            ui,
            active_tab,
            &mut actions,
            &mut detach_requests,
            &mut close_requests,
            &mut geometry_updates,
        );

        for (tab, rect) in geometry_updates {
            self.remember_window_geometry(tab, rect);
        }
        self.show_edit_dialogs(&ctx, &mut actions);
        if let Some(action) = finish_file_drop(&ctx) {
            actions.push(action);
        }
        for project in self.state.take_projects_to_open() {
            open_project_in_new_window(&project);
        }
        self.apply_window_requests(&ctx, open_requests, detach_requests, close_requests);
        self.blocking_confirmations(&ctx, &mut actions);
        self.refresh_monitors(|| frame.winit_window().map(|w| w.available_monitors().count()));
        self.sync_panel_layout(&mut actions);
        self.dispatch_actions(&ctx, actions);
        self.close_when_ready(&ctx);
        self.offer_figure(frame);

        self.sync_window_views();
        if snapshot_stamp(self.state.snapshot()) != published_stamp {
            // A command applied after publication wakes the viewports.
            ctx.request_repaint();
        }
        self.screenshot(&ctx);
        self.autoquit(&ctx);
    }
}

impl StokesForgeApp {
    /// Collects the finished jobs before the panels read the snapshot.
    fn poll_background_jobs(&mut self, ctx: &egui::Context) {
        // Probes a script finished on its detached thread **before** painting:
        // without a push notification from the actor, this is the only place
        // that refreshes the snapshot after a script (see the top doc of
        // `stokesforge_app_state::state`). Never blocks, even without a running script.
        self.state.poll_script();
        // The work also continues when its console is hidden or collapsed.
        if self.state.script_running() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        // Formula recalculation that the command did not wait for (#239): we
        // take over again at its end; meanwhile, the next frame comes.
        self.state.poll_recalc();
        if self.state.recalc_work().is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        // Background import: progress, and creation of the sheets at the end. The
        // created workbooks open as after an immediate import.
        if self.state.import_progress().is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
            let root = self.state.tree().root();
            let before = self
                .state
                .tree()
                .node(root)
                .map_or(0, |node| node.children().len());
            if self.state.poll_import() {
                self.open_new_workbooks_after(before);
            }
        }
        let imported = self.state.take_imported_paths();
        if !imported.is_empty() {
            remember_recent_imports(&imported);
        }
    }

    /// Paints the bars at the same rank between menus and status, without creating a sub-Ui.
    fn show_toolbars(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
    ) {
        let bar_env = BarEnv {
            root: Some(self.state.tree().root()),
            undo_label: self.state.undo_label(),
            redo_label: self.state.redo_label(),
            new_project_enabled: self.state.can_new_project(),
            save_blocked: self.state.script_running(),
            export_enabled: active_graph(active_tab_kind(active_tab)).is_some(),
            tools: self.tools_context(active_tab),
            text_format: self.format_target(ctx, active_tab),
            text_style: self.text_format_state(ctx, active_tab),
            object_format: self.object_format_state(active_tab),
            shortcuts: crate::toolbar::ShortcutTexts {
                new_project: ctx.format_shortcut(&SHORTCUT_NEW_PROJECT),
                open: ctx.format_shortcut(&SHORTCUT_OPEN),
                save: ctx.format_shortcut(&SHORTCUT_SAVE),
                save_as: ctx.format_shortcut(&SHORTCUT_SAVE_AS),
                import: SHORTCUT_IMPORT_TEXT.to_string(),
                export: ctx.format_shortcut(&SHORTCUT_EXPORT_GRAPH),
                undo: ctx.format_shortcut(&SHORTCUT_UNDO),
                redo: ctx.format_shortcut(&SHORTCUT_REDO_Y),
            },
        };
        let mut bar_requests = Vec::new();
        self.toolbars.show(ui, &bar_env, &mut bar_requests);
        for request in bar_requests {
            self.run_toolbar_request(ctx, request, active_tab, actions);
        }
        if self.toolbars.take_changed() {
            self.save_toolbar_layout();
        }
    }

    /// The panel or its detached viewport occupies a single step of the frame.
    fn show_project_manager(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        active_tab: Option<Tab>,
        open_requests: &mut Vec<Tab>,
        close_requests: &mut Vec<Tab>,
        actions: &mut Vec<Action>,
    ) {
        if self.pm_detached {
            // Draining before publication avoids publishing a stale selection.
            // Empties what the detached OS window wrote since the
            // last frame (see the top doc of
            // `show_detached_project_manager`) — same moment as the rest of
            // `open_requests`, before painting the fixed panel (which will not
            // be painted this frame anyway) and the `CentralPanel`.
            let (should_reattach, new_selection, mut new_close) = {
                let mut requests = self
                    .pm_requests
                    .lock()
                    .unwrap_or_else(|err| err.into_inner());
                open_requests.append(&mut requests.open);
                actions.append(&mut requests.actions);
                (
                    std::mem::take(&mut requests.reattach),
                    requests.selection.take(),
                    std::mem::take(&mut requests.close),
                )
            };
            if let Some(selection) = new_selection {
                self.pm_selection = selection;
            }
            close_requests.append(&mut new_close);
            if should_reattach {
                self.reattach_project_manager();
                ctx.request_repaint();
            }
        }
        if !self.pm_open {
            return;
        }
        if self.pm_detached {
            self.show_detached_project_manager(ctx, active_tab);
        } else {
            // Fixed panel, painted *before* the `CentralPanel` — see the top
            // doc of the module, § What stays here: this is what guarantees by
            // construction that it is never covered by it nor covers it,
            // like the menu bar (`egui::Panel::top`) painted
            // higher up.
            self.project_manager_panel(ui, open_requests, close_requests, actions);
        }
    }

    /// The detached requests stay drained before the workspace.
    fn drain_detached_tabs(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if !self.detached_tabs.is_empty() {
            self.show_detached_tabs(ctx);
            // Same draining as for the Project Manager, generalized to
            // several tabs: each may have accumulated actions and/or
            // asked to be reattached since the last frame.
            let mut to_reattach: Vec<Tab> = Vec::new();
            let mut to_close: Vec<Tab> = Vec::new();
            for (tab, detached) in &self.detached_tabs {
                if let Tab::Book(book) = tab {
                    let mut model = detached
                        .view_model
                        .lock()
                        .unwrap_or_else(|err| err.into_inner());
                    if let Some(view_state) = model.view_state.as_mut() {
                        self.state.adopt_detached_book_view(view_state, *book);
                    }
                }
                let mut requests = detached
                    .requests
                    .lock()
                    .unwrap_or_else(|err| err.into_inner());
                // During the modal graph properties window, a history
                // macro is open by the root: letting the actions of a detached
                // window in would have them absorbed, then undone with the graph
                // (issue #21). They are refused, with a message.
                if self.state.plot_settings().is_some() {
                    if !requests.actions.is_empty() {
                        requests.actions.clear();
                        self.state.reject_detached_actions();
                    }
                } else {
                    actions.append(&mut requests.actions);
                }
                if std::mem::take(&mut requests.reattach) {
                    to_reattach.push(*tab);
                }
                if std::mem::take(&mut requests.close) {
                    to_close.push(*tab);
                }
            }
            for tab in to_reattach {
                self.reattach_tab(tab);
            }
            for tab in to_close {
                self.close_detached_tab(tab);
            }
        }
    }

    /// Up-to-date window title (S-R15) and clearing of expired
    /// information (S-M5): a single redraw is scheduled, at the deadline, never a
    /// periodic wake-up at rest.
    fn sync_window_title_and_status(&mut self, ctx: &egui::Context) {
        let title = self.state.window_title();
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
        if let Some(remaining) = self.state.expire_status(std::time::Instant::now()) {
            ctx.request_repaint_after(remaining);
        }
    }

    /// Paints the dialogs after the CentralPanel, before the deferred application.
    fn show_edit_dialogs(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        // Import window (`Action::ImportFiles`, see the top doc of
        // [`import_window`]): a transient dialog, painted as long as
        // `self.state.import_state()` is `Some`, rather than one more floating
        // window — after the `CentralPanel` but before the draining of the
        // requests below, so that the `Action::ImportFiles` it
        // possibly pushes is dispatched this frame like any
        // other action.
        import_window(ctx, &mut self.state, actions);
        // Graph properties window: same family of transient
        // dialog as the import window, painted as long as
        // `self.state.plot_settings()` is `Some`.
        plot_settings_window(ctx, &mut self.state);
        column_metadata_window(ctx, &self.state, actions);
        status_history_window(ctx, &self.state, actions);
        low_r2_list_window(ctx, &self.state, actions);
        keyboard_shortcuts_window(ctx, &self.state, actions);
        about_window(ctx, &self.state, actions);
        if let Some(language) = preferences_window(ctx, &self.state, actions) {
            self.choose_language(language);
        }
        analysis_window(ctx, &mut self.state, actions);
        calculation_delete_window(ctx, &self.state, actions);
        analysis_replace_window(ctx, &self.state, actions);
        transpose_relink_window(ctx, &self.state, actions);
        stale_export_window(ctx, &self.state, actions);
        column_use_window(ctx, &self.state, actions);
        full_copy_window(ctx, &self.state, actions);
        recovery_window(ctx, &self.state, actions);
        dialogs::sync_dialogs(ctx, &self.state, actions);
    }

    /// Applies the stack changes only after the windows are painted.
    fn apply_window_requests(
        &mut self,
        ctx: &egui::Context,
        open_requests: Vec<Tab>,
        detach_requests: Vec<Tab>,
        close_requests: Vec<Tab>,
    ) {
        // The requested tabs (Project Manager, fixed or detached panel),
        // the detachments and closings requested by the header of a floating
        // window, and the actions collected by all the panels and
        // tabs are applied only once the frame is painted: mutating
        // `floating_tabs` in the middle of the traversal of the floating windows
        // would invalidate it.
        for tab in open_requests {
            self.open_tab(tab);
        }
        let detached_this_frame = !detach_requests.is_empty();
        for tab in detach_requests {
            self.detach_tab(tab);
        }
        if detached_this_frame {
            // `show_detached_tabs` already published the viewports of this
            // frame before the header asked for the detachment. Without a
            // second root frame, eframe may thus stay idle
            // without ever creating the deferred viewport and calling its callback.
            ctx.request_repaint();
        }
        // Same path as the cross: the floating or detached window closes.
        for tab in &close_requests {
            self.close_detached_tab(*tab);
        }
        self.close_floating_tabs(close_requests);
    }

    /// The open tabs, floating or detached (#327).
    fn open_tabs(&self) -> HashSet<Tab> {
        self.floating_tabs
            .iter()
            .chain(self.detached_tabs.keys())
            .copied()
            .collect()
    }

    /// A confirmation replaces all the actions already collected in this frame.
    fn blocking_confirmations(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if self.state.blocking_dialog() {
            // Only the dialog can act on this frame: keyboard, drops and
            // requests of the detached viewports are also blocked.
            actions.clear();
        }
        confirmations::show(ctx, &self.state, actions);
    }

    /// The entries of the external views count in the question "Discard Changes ·
    /// Cancel · Save" of New Project, Open… and Quit (#229).
    fn prepare_new_project_action(&self, ctx: &egui::Context, action: Action) -> Action {
        if let Action::NewProject { pending_edits }
        | Action::OpenProject { pending_edits, .. }
        | Action::Quit { pending_edits } = action
        {
            // An entry in a detached viewport, or a graph text,
            // is not yet in the AppState snapshot. Signal it
            // to ask for confirmation even if the model is saved.
            let graph_editing = |view: &GraphView| {
                ctx.data(|data| {
                    data.get_temp::<TextEditor>(text_editor_id(view)).is_some()
                        || data
                            .get_temp::<AnnotationEditor>(annotation_editor_id(view))
                            .is_some()
                })
            };
            let pending_edits = pending_edits
                || self.graph_views.values().any(&graph_editing)
                || self.detached_tabs.values().any(|detached| {
                    detached.view_model.lock().map_or(true, |model| {
                        model.view_state.as_ref().is_some_and(|state| {
                            state.editing().is_some()
                                || state.header_editing().is_some()
                                || state.note_editing().is_some()
                        }) || model.graph_view.as_ref().is_some_and(graph_editing)
                    })
                });
            match action {
                Action::OpenProject { path, .. } => Action::OpenProject {
                    path,
                    pending_edits,
                },
                Action::Quit { .. } => Action::Quit { pending_edits },
                _ => Action::NewProject { pending_edits },
            }
        } else {
            action
        }
    }

    /// A confirmation request also stops the remaining actions of the frame.
    fn dispatch_actions(&mut self, ctx: &egui::Context, actions: Vec<Action>) {
        for action in actions {
            // Without a destination, "Save" first asks where to write; the
            // picker's answer then pushes `Action::Save` (#192). The
            // Save answer to "Discard Changes · Cancel · Save" (#229) does
            // the same, once the state has noted the gesture to continue.
            let action = if matches!(action, Action::ConfirmUnsaved(UnsavedChoice::Save))
                && self.state.path().is_empty()
            {
                self.state.dispatch(action);
                Action::Save
            } else {
                action
            };
            let action = if matches!(action, Action::Save) && self.state.path().is_empty() {
                Action::PickFile(PickRequest::SaveProjectAs {
                    suggested: suggested_file_name(self.state.path()),
                })
            } else {
                action
            };
            if let Action::PickFile(request) = action {
                let wake = ctx.clone();
                // Plain paths: the picker's thread looks them up on the disk.
                let candidates = self.state.folder_candidates(&request);
                self.file_picker
                    .start(request, candidates, move || wake.request_repaint());
                continue;
            }
            if let Action::OpenFolder(folder) = action {
                if let Err(error) = (self.folder_opener)(&folder) {
                    self.state.report_folder_open_error(&error);
                }
                continue;
            }
            let action = self.prepare_new_project_action(ctx, action);
            if self.dispatch_action(ctx, action) {
                break;
            }
        }
    }

    /// Captures the presentation effects before the command, then applies them to its result.
    fn dispatch_action(&mut self, ctx: &egui::Context, action: Action) -> bool {
        // `Action::PlotColumns` must open the graph it creates
        // (work package "plot from a workbook"). `dispatch` never
        // returns the created identifier — same constraint as the import
        // (`AppState::import_files`) — so the parent folder is
        // computed **before** sending the command, and the graph is
        // found afterwards as the last child of that folder:
        // `Project::add_graph` inserts it there in last position, and
        // `AppState::dispatch` already refreshed `self.state`
        // (`AppState::refresh`) on return from this call.
        let plotted_from = if matches!(action, Action::AddGraph) {
            Some(self.state.tree().root())
        } else if let Action::PlotColumns { sheet, .. } = &action {
            self.state
                .tree()
                .node(*sheet)
                .and_then(Node::parent)
                .and_then(|book| self.state.tree().node(book).and_then(Node::parent))
        } else {
            None
        };
        let creates_workbook = matches!(action, Action::AddWorkbook);
        // *Add Folder* (#326): same pattern as `plotted_from` — the new folder
        // is the last child of its parent, and its name is edited in place.
        let folder_parent = if let Action::AddFolder { parent } = &action {
            Some(*parent)
        } else {
            None
        };
        let pm_viewport = if self.pm_detached {
            project_manager::viewport_id()
        } else {
            egui::ViewportId::ROOT
        };
        let new_import_books_after = if matches!(
            action,
            Action::ImportFiles {
                destination: ImportDestination::NewBookPerFile | ImportDestination::NewBookForAll,
                ..
            }
        ) {
            let root = self.state.tree().root();
            self.state
                .tree()
                .node(root)
                .map(|node| node.children().len())
        } else {
            None
        };
        let revision_before = self.state.revision();
        let epoch_before = self.state.snapshot().epoch;
        // Save can continue a New Project or an Open… (#229).
        let replaces_project = matches!(
            action,
            Action::NewProject { .. }
                | Action::OpenProject { .. }
                | Action::Load
                | Action::Save
                | Action::SaveAs { .. }
                | Action::ConfirmUnsaved(_)
        );
        // The history and the gestures that change the rank of the series or the
        // composition of a group (#197, including its deletion: several
        // series leave at once) make the selection follow by content.
        let mut history_selections: Vec<_> = if matches!(
            action,
            Action::Undo
                | Action::Redo
                | Action::MoveSeries { .. }
                | Action::MoveSeriesGroup { .. }
                | Action::PlaceSeries { .. }
                | Action::PlaceSeriesGroup { .. }
                | Action::ReorderObject { .. }
                | Action::UngroupSeries { .. }
                | Action::RemovePlotObject {
                    target: stokesforge_app_state::object_manager::VisibilityTarget::SeriesGroup(_),
                    ..
                }
        ) {
            let graphs =
                self.graph_views
                    .keys()
                    .copied()
                    .chain(self.detached_tabs.keys().filter_map(|tab| match tab {
                        Tab::Graph(graph) => Some(*graph),
                        _ => None,
                    }));
            graphs
                .filter_map(|graph| {
                    let target = self
                        .read_graph_view(graph, selected_manager_target)
                        .flatten()?;
                    let previous =
                        stokesforge_core::compose::current_plot_spec(self.state.tree(), graph)?;
                    Some((graph, target, previous))
                })
                .collect()
        } else {
            Vec::new()
        };
        // Layers added or deleted (#357): the views address layers by index
        // (framing, active layer, selection), so they follow the move.
        let layer_changes: Vec<LayerChange> = self.layer_changes_before(&action);
        let added_cell = match &action {
            Action::AddCell { graph, .. } => Some(*graph),
            _ => None,
        };
        let selection_change = match &action {
            Action::RemovePlotObject {
                target: stokesforge_app_state::object_manager::VisibilityTarget::Panel(_),
                ..
            } => None,
            Action::RemovePlotObject { graph, target } => Some((*graph, *target)),
            Action::RemoveAnnotation { graph, target } => Some((
                *graph,
                stokesforge_app_state::object_manager::VisibilityTarget::Annotation(*target),
            )),
            Action::RemoveShape { graph, target } => Some((
                *graph,
                stokesforge_app_state::object_manager::VisibilityTarget::Shape(*target),
            )),
            _ => None,
        };
        self.state.dispatch(action);
        if let Some(text) = self.state.take_object_clipboard_text() {
            ctx.copy_text(text);
        }
        for (source, destination, duplicate) in self.state.take_copied_objects() {
            let source_open = self
                .floating_tabs
                .iter()
                .chain(self.detached_tabs.keys())
                .any(|tab| tab_node(*tab) == source);
            if !duplicate || source_open {
                if let Some((id, kind)) = window_nodes(&self.state.project_tree())
                    .into_iter()
                    .find(|(id, _)| *id == destination)
                {
                    self.open_tab(tab_for_window(id, kind));
                }
            }
        }
        let removed: Vec<_> = self
            .floating_tabs
            .iter()
            .chain(self.detached_tabs.keys())
            .copied()
            .filter(|tab| {
                self.state.tree().node(tab_node(*tab)).is_none_or(
                    |node| matches!(node.kind(), stokesforge_core::NodeKind::Workbook { meta, .. } if meta.hidden),
                )
            })
            .collect();
        for tab in &removed {
            if self.detached_tabs.remove(tab).is_some() {
                ctx.send_viewport_cmd_to(
                    egui::ViewportId::from_hash_of(tab),
                    egui::ViewportCommand::Close,
                );
            }
            self.pm_selection
                .remove(&project_manager::PmEntry::Window(*tab));
            self.collapsed_tabs.remove(tab);
            self.graph_views.remove(&tab_node(*tab));
            if self.active_detached == Some(*tab) {
                self.active_detached = None;
            }
        }
        self.close_floating_tabs(removed);

        if self.state.revision() != revision_before {
            if let Some((graph, removed)) = selection_change {
                self.update_selection_after_action(graph, removed);
            }
        }
        let mut moved_layers = Vec::new();
        if self.state.revision() != revision_before {
            moved_layers = self.follow_layer_changes(layer_changes);
            if let Some(graph) = added_cell {
                // The new cell is the last layer, and becomes the active one.
                if let Some(spec) =
                    stokesforge_core::compose::current_plot_spec(self.state.tree(), graph)
                {
                    let last = spec.panels.len().saturating_sub(1);
                    self.with_graph_view(graph, |view| view.set_active_panel(last, &spec));
                }
            }
        }
        history_selections.retain(|(graph, _, _)| !moved_layers.contains(graph));
        if self.state.revision() != revision_before {
            for (graph, target, previous) in history_selections {
                if let Some(next) =
                    stokesforge_core::compose::current_plot_spec(self.state.tree(), graph)
                {
                    let target = stokesforge_app_state::object_manager::selection_after_history(
                        &previous,
                        &next,
                        Some(target),
                    );
                    self.with_graph_view(graph, |view| select_manager_target(view, target, &next));
                }
            }
        }
        if replaces_project && self.state.snapshot().epoch != epoch_before {
            self.forget_project_views(ctx);
        }
        if self.state.blocking_dialog() {
            ctx.request_repaint();
            return true;
        }
        if creates_workbook && self.state.revision() != revision_before {
            let root = self.state.tree().root();
            if let Some(book) = self
                .state
                .tree()
                .node(root)
                .and_then(|node| node.children().last().copied())
                .filter(|id| self.state.worksheets_of(*id).len() == 1)
            {
                self.open_tab(Tab::Book(book));
            }
        }
        if let Some(parent) = folder_parent {
            if self.state.revision() != revision_before {
                if let Some(folder) = self
                    .state
                    .tree()
                    .node(parent)
                    .and_then(|node| node.children().last().copied())
                {
                    project_manager::begin_rename_of_new_folder(ctx, pm_viewport, folder, parent);
                }
            }
        }
        if let Some(before) = new_import_books_after {
            self.open_new_workbooks_after(before);
        }
        if let Some(parent) = plotted_from {
            // `self.state.revision()` moved only if `AddGraph` was
            // actually sent (see `AppState::plot_columns`):
            // without this guard, a selection without Y (no-op on the model side)
            // would wrongly reopen the last child already present in
            // `parent`, which has nothing to do with this click.
            if self.state.revision() != revision_before {
                if let Some(graph) = self
                    .state
                    .tree()
                    .node(parent)
                    .and_then(|node| node.children().last().copied())
                {
                    self.open_tab(Tab::Graph(graph));
                }
            }
        }
        false
    }

    /// What a layer-changing action may move, read before it runs (#357): the
    /// graph, its spec, the selection, and the exact move when the action knows
    /// it (a deletion); history leaves it to be found by comparing the specs.
    fn layer_changes_before(&self, action: &Action) -> Vec<LayerChange> {
        use stokesforge_app_state::object_manager::VisibilityTarget as T;
        let graphs: Vec<NodeId> = match action {
            Action::RemovePlotObject {
                graph,
                target: T::Panel(_),
            } => vec![*graph],
            Action::Undo | Action::Redo => self
                .graph_views
                .keys()
                .copied()
                .chain(self.detached_tabs.keys().filter_map(|tab| match tab {
                    Tab::Graph(graph) => Some(*graph),
                    _ => None,
                }))
                .collect(),
            _ => Vec::new(),
        };
        graphs
            .into_iter()
            .filter_map(|graph| {
                let spec = stokesforge_core::compose::current_plot_spec(self.state.tree(), graph)?;
                let exact = match action {
                    Action::RemovePlotObject {
                        target: T::Panel(panel),
                        ..
                    } => Some(stokesforge_core::compose::remap_after_removal(
                        spec.panels.len(),
                        &stokesforge_core::compose::layers_removed_with(&spec, *panel),
                    )),
                    _ => None,
                };
                let selected = self
                    .read_graph_view(graph, selected_manager_target)
                    .flatten();
                Some(LayerChange {
                    graph,
                    previous: spec,
                    exact,
                    selected,
                })
            })
            .collect()
    }

    /// Applies the move of the layers to the views of the graphs that changed
    /// (#357); returns those graphs. The selection is kept when its layer is.
    fn follow_layer_changes(&mut self, changes: Vec<LayerChange>) -> Vec<NodeId> {
        let mut moved = Vec::new();
        for change in changes {
            let Some(next) =
                stokesforge_core::compose::current_plot_spec(self.state.tree(), change.graph)
            else {
                continue;
            };
            let map = change.exact.or_else(|| {
                (change.previous.panels.len() != next.panels.len())
                    .then(|| stokesforge_core::compose::remap_between(&change.previous, &next))
            });
            let Some(map) = map else {
                continue;
            };
            let selected = change.selected.and_then(|target| target.remap_panel(&map));
            self.with_graph_view(change.graph, |view| {
                view.remap_layers(&map);
                select_manager_target(view, selected, &next);
            });
            moved.push(change.graph);
        }
        moved
    }

    /// The deletion makes the selection follow only if the command acted.
    fn update_selection_after_action(
        &mut self,
        graph: NodeId,
        removed: stokesforge_app_state::object_manager::VisibilityTarget,
    ) {
        if let Some(spec) = stokesforge_core::compose::current_plot_spec(self.state.tree(), graph) {
            self.with_graph_view(graph, |view| {
                let next = stokesforge_app_state::object_manager::selection_after_remove(
                    selected_manager_target(view),
                    removed,
                );
                select_manager_target(view, next, &spec);
            });
        }
    }

    /// An identity replacement forgets the views that could target reused NodeIds.
    fn forget_project_views(&mut self, ctx: &egui::Context) {
        project_manager::forget_views(ctx);
        // NodeIds may be reused: forget all the
        // views and geometries of the old identity before painting.
        for tab in self.detached_tabs.keys() {
            ctx.send_viewport_cmd_to(
                egui::ViewportId::from_hash_of(tab),
                egui::ViewportCommand::Close,
            );
        }
        self.detached_tabs.clear();
        self.floating_tabs.clear();
        self.cascade_rank.clear();
        self.next_cascade = 0;
        self.focus_next = None;
        self.graph_views.clear();
        self.active_detached = None;
        self.collapsed_tabs.clear();
        self.maximized_tab = None;
        self.window_positions.clear();
        self.window_sizes.clear();
        self.figure_follow.clear();
        self.workspace_scroll = egui::Vec2::ZERO;
        self.saved_window_geometries.clear();
        self.floating_rects.clear();
        self.pm_selection.clear();
        self.pm_last_active_tab = None;
        self.pm_requests = Arc::new(Mutex::new(DetachedProjectManagerRequests::default()));
        ctx.memory_mut(|memory| {
            // In-place editors and deferred formats must not
            // outlive the window they targeted. Keep the
            // preferences and panel dimensions of the application.
            memory.data.remove_by_type::<TextEditor>();
            memory.data.remove_by_type::<AnnotationEditor>();
            memory
                .data
                .remove::<Option<stokesforge_app_state::text_format::TextFormat>>(
                    pending_format_id(),
                );
            memory.data.remove::<Option<String>>(pending_zone_font_id());
            memory.data.remove_by_type::<egui::scroll_area::State>();
            if let Some(focused) = memory.focused() {
                memory.surrender_focus(focused);
            }
        });
        ctx.request_repaint();
    }
}

/// The file name suggested by default to the "Save As…" picker:
/// the name of the current path if it has one, otherwise the name of a new project.
fn suggested_file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("{}.stf", stokesforge_session::NEW_PROJECT_NAME))
}

/// A graph whose layers an action may move (#357), see
/// [`StokesForgeApp::layer_changes_before`].
struct LayerChange {
    graph: NodeId,
    previous: stokesforge_plot::PlotSpec,
    /// New index of each previous layer, when the action knows it.
    exact: Option<Vec<Option<usize>>>,
    selected: Option<stokesforge_app_state::object_manager::VisibilityTarget>,
}

#[cfg(test)]
mod tests {
    //! The orchestration alone (#115, #123): the order of the panels, the
    //! publication scheduled after a detached command, the blocking
    //! confirmation of the new project, the global shortcuts, the restoration
    //! of the windows by [`StokesForgeApp::new`], and the measurement of a full
    //! frame. Detaching is tested in `window_detach.rs`, each
    //! surface in its module.
    use super::graph_window::tests::plotted_graph;
    use super::grid::{header_rows, set_header_rows, HeaderRows};
    use super::*;
    use crate::theme;
    use stokesforge_app_state::state::RowAxis;
    use stokesforge_core::{Column, Project};
    use stokesforge_plot::{PanelSpec, PlotSpec, SeriesSpec};
    use stokesforge_session::Session;

    /// The Project Manager follows the window that is pressed from the
    /// press on, mouse held, and highlights its row in the frame requested
    /// by the press, without waiting for a mouse movement.
    #[test]
    fn project_manager_selection_follows_a_press_on_a_window() {
        use eframe::App as _;
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let book = window_stack::window_nodes(&state.project_tree())
            .into_iter()
            .find(|(_, kind)| *kind == stokesforge_app_state::state::WindowKind::Book)
            .map(|(id, _)| Tab::Book(id))
            .unwrap();
        let tab = Tab::Graph(graph);
        let mut app = StokesForgeApp::new(state);
        app.floating_tabs = vec![book, tab];
        app.window_positions.insert(tab, egui::pos2(0.0, 0.0));
        app.window_positions.insert(book, egui::pos2(640.0, 300.0));
        let ctx = egui::Context::default();
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let mut frame = eframe::Frame::_new_kittest();
        let mut now = 0.0;
        let mut run = |app: &mut StokesForgeApp, events: Vec<egui::Event>| {
            now += 0.05;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    time: Some(now),
                    events,
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut frame),
            );
            output.textures_delta.clear();
            output
        };
        for _ in 0..5 {
            run(&mut app, Vec::new());
        }
        assert_eq!(app.pm_selection, project_manager::tests::windows([tab]));
        let output = run(&mut app, Vec::new());
        let title = |output: &egui::FullOutput, wanted: &str, left: bool| {
            output.shapes.iter().find_map(|sh| match &sh.shape {
                egui::Shape::Text(t)
                    if t.galley.job.text == wanted && (t.pos.x < 300.0) == left =>
                {
                    Some(t.pos)
                }
                _ => None,
            })
        };
        let on_book =
            title(&output, "Book1", false).expect("workbook title") + egui::vec2(10.0, 30.0);
        run(&mut app, vec![egui::Event::PointerMoved(on_book)]);
        let mut output = run(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: on_book,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        // Only the frames that the application requests take place, as under eframe.
        let mut extra = 0;
        while output.viewport_output[&egui::ViewportId::ROOT].repaint_delay
            == std::time::Duration::ZERO
            && extra < 10
        {
            output = run(&mut app, Vec::new());
            extra += 1;
        }
        assert_eq!(app.pm_selection, project_manager::tests::windows([book]));
        let row = title(&output, "Book1", true).expect("workbook row").y;
        let highlighted: Vec<f32> = output
            .shapes
            .iter()
            .filter_map(|sh| match &sh.shape {
                egui::Shape::Rect(r) if r.fill == theme::list_row_selected_fill() => {
                    Some(r.rect.center().y)
                }
                _ => None,
            })
            .collect();
        assert!(
            matches!(highlighted.as_slice(), [y] if (y - row).abs() < 15.0),
            "only the workbook row is highlighted: {highlighted:?} / {row}"
        );
        session.shutdown().unwrap();
    }

    /// A click on a Project Manager row updates the row highlight and
    /// the window outline in the frame requested by the click,
    /// without waiting for a mouse movement.
    #[test]
    fn a_click_in_the_project_manager_repaints_without_moving_the_mouse() {
        use eframe::App as _;
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let book = window_stack::window_nodes(&state.project_tree())
            .into_iter()
            .find(|(_, kind)| *kind == stokesforge_app_state::state::WindowKind::Book)
            .map(|(id, _)| Tab::Book(id))
            .unwrap();
        let tab = Tab::Graph(graph);
        let mut app = StokesForgeApp::new(state);
        app.floating_tabs = vec![book, tab];
        app.window_positions.insert(tab, egui::pos2(0.0, 0.0));
        app.window_positions.insert(book, egui::pos2(640.0, 300.0));
        let ctx = egui::Context::default();
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let mut frame = eframe::Frame::_new_kittest();
        let mut now = 0.0;
        let mut run = |app: &mut StokesForgeApp, events: Vec<egui::Event>| {
            now += 0.05;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    time: Some(now),
                    events,
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut frame),
            );
            output.textures_delta.clear();
            output
        };
        for _ in 0..5 {
            run(&mut app, Vec::new());
        }
        let output = run(&mut app, Vec::new());
        let row = project_manager::tests::find_text_center(&output.shapes, "Book1");
        let button = |pressed| egui::Event::PointerButton {
            pos: row,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run(&mut app, vec![egui::Event::PointerMoved(row)]);
        run(&mut app, vec![button(true)]);
        let mut output = run(&mut app, vec![button(false)]);
        // Only the frames that the application requests take place, as under eframe.
        let mut extra = 0;
        while output.viewport_output[&egui::ViewportId::ROOT].repaint_delay
            == std::time::Duration::ZERO
            && extra < 10
        {
            output = run(&mut app, Vec::new());
            extra += 1;
        }
        assert_eq!(app.floating_tabs.last(), Some(&book));
        assert_eq!(app.pm_selection, project_manager::tests::windows([book]));
        let highlighted: Vec<f32> = output
            .shapes
            .iter()
            .filter_map(|sh| match &sh.shape {
                egui::Shape::Rect(r) if r.fill == theme::list_row_selected_fill() => {
                    Some(r.rect.center().y)
                }
                _ => None,
            })
            .collect();
        assert!(
            matches!(highlighted.as_slice(), [y] if (y - row.y).abs() < 15.0),
            "the workbook row is highlighted without mouse movement: {highlighted:?} / {}",
            row.y
        );
        session.shutdown().unwrap();
    }

    /// Trap 1 of #115: the order of the panels in `ui()` makes the
    /// layout. Menu bar at the top and status bar at the bottom, across the
    /// whole width; the script console, placed before the Project Manager, runs
    /// under it across the whole width; Project Manager on the left and Object
    /// Manager on the right stop above it.
    #[test]
    fn fixed_panels_are_placed_in_the_order_of_ui() {
        use eframe::App as _;
        use egui::containers::panel::PanelState;
        let (session, _graph) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        app.script_collapsed = false;
        app.state.set_object_manager_open_for_test(true);
        let ctx = egui::Context::default();
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let mut frame = eframe::Frame::_new_kittest();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        for _ in 0..4 {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut frame),
            )
            .textures_delta
            .clear();
        }
        let rect = |id: &str| {
            PanelState::load(&ctx, egui::Id::new(id))
                .unwrap_or_else(|| panic!("panel {id} missing"))
                .outer_rect
        };
        let near = |a: f32, b: f32| (a - b).abs() <= 1.0;
        let menu = rect("stokesforge_menu_bar");
        let status = rect("stokesforge_status");
        let script = rect("stokesforge_script");
        let pm = rect("stokesforge_pm");
        let om = rect("stokesforge_object_manager");
        for (name, panel) in [("menu", menu), ("status", status), ("console", script)] {
            assert!(
                near(panel.left(), screen.left()) && near(panel.right(), screen.right()),
                "{name} across the whole width: {panel:?}"
            );
        }
        assert!(near(menu.top(), screen.top()));
        assert!(near(status.bottom(), screen.bottom()));
        assert!(
            script.bottom() <= status.top() + 1.0,
            "console on the status bar"
        );
        assert!(
            near(pm.left(), screen.left()),
            "Project Manager on the left: {pm:?}"
        );
        assert!(
            near(om.right(), screen.right()),
            "Object Manager on the right: {om:?}"
        );
        for (name, panel) in [("Project Manager", pm), ("Object Manager", om)] {
            assert!(
                panel.bottom() <= script.top() + 1.0,
                "{name} above the console: {panel:?} / {script:?}"
            );
            assert!(panel.top() >= menu.bottom() - 1.0, "{name} under the menu");
        }
        assert!(pm.right() <= om.left(), "{pm:?} then {om:?}");
        session.shutdown().unwrap();
    }

    #[test]
    fn a_detached_command_schedules_the_publication_of_its_result() {
        use eframe::App as _;
        let (session, _graph) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book = app.state.books()[0].0;
        let sheet = app.state.selected_sheet_in(book).unwrap();
        let column = app.state.visible_columns(sheet)[0].0;
        let tab = Tab::Book(book);
        app.detach_tab(tab);
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        egui_extras::install_image_loaders(&ctx);
        let mut frame = eframe::Frame::_new_kittest();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 700.0),
            )),
            ..Default::default()
        };
        for _ in 0..8 {
            ctx.run_ui(input.clone(), |ui| app.ui(ui, &mut frame))
                .textures_delta
                .clear();
        }
        let mut idle = ctx.run_ui(input.clone(), |ui| app.ui(ui, &mut frame));
        idle.textures_delta.clear();
        assert_eq!(
            idle.viewport_output[&egui::ViewportId::ROOT].repaint_delay,
            Duration::MAX
        );
        app.detached_tabs[&tab]
            .requests
            .lock()
            .unwrap()
            .actions
            .push(Action::CommitCell {
                column,
                row: 0,
                text: "42".into(),
            });
        let mut output = ctx.run_ui(input.clone(), |ui| app.ui(ui, &mut frame));
        output.textures_delta.clear();
        assert_eq!(
            output.viewport_output[&egui::ViewportId::ROOT].repaint_delay,
            Duration::ZERO,
            "the command changed the snapshot after painting"
        );
        assert_eq!(app.state.cell_text(column, 0), "42");
        let mut output = ctx.run_ui(input, |ui| app.ui(ui, &mut frame));
        output.textures_delta.clear();
        assert_eq!(
            output.viewport_output[&egui::ViewportId::from_hash_of(tab)].repaint_delay,
            Duration::ZERO
        );
        assert_eq!(
            app.detached_tabs[&tab]
                .view_model
                .lock()
                .unwrap()
                .snapshot
                .as_ref()
                .unwrap()
                .revision,
            app.state.revision(),
            "the scheduled frame publishes the result"
        );
        session.shutdown().unwrap();
    }

    /// Stopping the dispatch also protects the actions already collected after Ctrl+N.
    #[test]
    fn the_confirmation_stops_the_following_actions_of_the_frame() {
        let (session, _) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book = app.state.books()[0].0;
        let sheet = app.state.selected_sheet_in(book).unwrap();
        let column = app.state.visible_columns(sheet)[0].0;
        let context = egui::Context::default();
        let commit = || Action::CommitCell {
            column,
            row: 0,
            text: "42".into(),
        };
        let before = app.state.cell_text(column, 0);
        assert_ne!(before, "42", "witness: the entry does change the value");
        app.dispatch_actions(
            &context,
            vec![
                Action::NewProject {
                    pending_edits: true,
                },
                commit(),
            ],
        );
        assert!(app.state.unsaved_pending().is_some());
        assert_eq!(app.state.cell_text(column, 0), before);

        // Witness: once the confirmation is canceled, the same entry acts.
        app.dispatch_actions(
            &context,
            vec![Action::ConfirmUnsaved(UnsavedChoice::Cancel), commit()],
        );
        assert!(app.state.unsaved_pending().is_none());
        assert_eq!(app.state.cell_text(column, 0), "42");
        session.shutdown().unwrap();
    }

    /// #166: dropping rows on a graph that already has an X opens the blocking
    /// axis dialog — "Create Axis" is the default choice (Enter),
    /// "Use Present Axis" the second button — and suspends the other
    /// actions of the frame; without a question, nothing is painted.
    #[test]
    fn dropping_rows_on_a_graph_with_x_opens_the_axis_dialog() {
        use super::test_support::painted;
        use eframe::App as _;
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book = app.state.books()[0].0;
        let sheet = app.state.selected_sheet_in(book).unwrap();
        let columns = app.state.visible_columns(sheet);
        app.state.dispatch(Action::SetColumnXPrime {
            column: columns[0].0,
            text: "3".into(),
        });
        let ctx = egui::Context::default();
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let mut frame = eframe::Frame::_new_kittest();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        let mut paint = |app: &mut StokesForgeApp, events: Vec<egui::Event>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut frame),
            );
            output.textures_delta.clear();
            output
        };
        let before = app.state.plot_spec(graph).unwrap();
        paint(&mut app, Vec::new());
        assert!(!painted(
            &paint(&mut app, Vec::new()),
            &tr!("row-drop-title")
        ));

        app.dispatch_actions(
            &ctx,
            vec![Action::AddRowsToGraph {
                graph,
                panel: 0,
                sheet,
                rows: vec![0],
                axis: RowAxis::Ask,
            }],
        );
        assert_eq!(app.state.pending_row_drop(), Some((graph, 1)));
        assert_eq!(
            app.state.plot_spec(graph).unwrap(),
            before,
            "nothing before the answer"
        );
        paint(&mut app, Vec::new());
        let output = paint(&mut app, Vec::new());
        for (label, text) in [
            ("title", tr!("row-drop-title")),
            ("create axis", tr!("row-drop-new-axis")),
            ("present axis", tr!("row-drop-present-axis")),
        ] {
            assert!(painted(&output, &text), "{label} missing from the dialog");
        }

        // Enter = default button: "Create Axis".
        paint(&mut app, vec![super::test_support::key(egui::Key::Enter)]);
        assert_eq!(app.state.pending_row_drop(), None);
        let spec = app.state.plot_spec(graph).unwrap();
        assert_eq!(spec.panels.len(), 2, "new layer with its own X axis");
        assert_eq!(spec.panels[1].series[0].row, Some(0));
        session.shutdown().unwrap();
    }

    /// #326: Add Folder opens the name of the new folder for editing in the
    /// panel on screen (fixed or detached); a failure opens nothing.
    #[test]
    fn add_folder_opens_the_new_folder_name_for_editing() {
        let (session, _) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        let root = app.state.tree().root();
        let fixed = egui::ViewportId::ROOT;
        let detached = project_manager::viewport_id();

        app.dispatch_actions(&context, vec![Action::AddFolder { parent: root }]);
        let folder = *app
            .state
            .tree()
            .node(root)
            .unwrap()
            .children()
            .last()
            .unwrap();
        assert_eq!(app.state.tree().node(folder).unwrap().name(), "Folder1");
        assert_eq!(project_manager::renaming_in(&context, fixed), vec![folder]);
        assert!(project_manager::renaming_in(&context, detached).is_empty());

        // Detached panel: it is the one that receives the editing.
        app.pm_detached = true;
        app.dispatch_actions(&context, vec![Action::AddFolder { parent: folder }]);
        let inner = *app
            .state
            .tree()
            .node(folder)
            .unwrap()
            .children()
            .last()
            .unwrap();
        assert_eq!(
            project_manager::renaming_in(&context, detached),
            vec![inner]
        );
        assert_eq!(
            project_manager::renaming_in(&context, fixed),
            vec![folder],
            "the fixed panel keeps its previous editing"
        );

        // Witness: a vanished parent opens no editing.
        let ghost = egui::Context::default();
        app.dispatch_actions(&ghost, vec![Action::DeleteObjects(vec![folder])]);
        app.dispatch_actions(&ghost, vec![Action::AddFolder { parent: folder }]);
        assert!(project_manager::renaming_in(&ghost, detached).is_empty());
        session.shutdown().unwrap();
    }

    #[test]
    fn delete_closes_the_views_and_undo_does_not_reopen_them() {
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        let book = app.state.books()[0].0;
        app.open_tab(Tab::Graph(graph));
        app.detach_tab(Tab::Graph(graph));
        assert!(app.detached_tabs.contains_key(&Tab::Graph(graph)));
        app.dispatch_actions(&context, vec![Action::DeleteObjects(vec![graph])]);
        assert!(!app.detached_tabs.contains_key(&Tab::Graph(graph)));
        assert!(!app.state.tree().contains(graph));
        app.dispatch_actions(&context, vec![Action::Undo]);
        assert!(app.state.tree().contains(graph));
        assert!(!app.floating_tabs.contains(&Tab::Graph(graph)));
        assert!(!app.detached_tabs.contains_key(&Tab::Graph(graph)));
        app.open_tab(Tab::Book(book));
        assert!(app.floating_tabs.contains(&Tab::Book(book)), "witness");
        // An undoable deletion asks nothing (S-F3, #229).
        app.dispatch_actions(&context, vec![Action::DeleteObjects(vec![book])]);
        assert!(!app.state.tree().contains(book));
        assert!(!app.floating_tabs.contains(&Tab::Book(book)));
        app.dispatch_actions(&context, vec![Action::Undo]);
        assert!(app.state.tree().contains(book));
        assert!(!app.floating_tabs.contains(&Tab::Book(book)));
    }

    /// #197: A, [B C], D. Deleting or moving a group changes the rank of
    /// several series at once; the selection follows its content.
    fn graph_with_group() -> (Session, NodeId) {
        let (session, graph) = plotted_graph();
        let handle = session.handle();
        let snapshot = handle.snapshot().unwrap();
        let stokesforge_core::NodeKind::Graph { spec, .. } =
            snapshot.tree.node(graph).unwrap().kind()
        else {
            panic!("graph expected");
        };
        let mut spec = spec.clone();
        let panel = spec.panels.get_mut(0).unwrap();
        let model = panel.series[0].clone();
        panel.series = ["A", "B", "C", "D"]
            .iter()
            .enumerate()
            .map(|(rank, name)| {
                let mut s = model.clone();
                s.name = (*name).to_owned();
                s.group = (rank == 1 || rank == 2).then_some(1);
                s
            })
            .collect();
        handle
            .execute(stokesforge_commands::Command::SetPlotSpec { graph, spec })
            .unwrap();
        (session, graph)
    }

    fn selected_name(app: &StokesForgeApp, graph: NodeId) -> Option<String> {
        let rank = app.selected_series(graph)?;
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph)?;
        Some(spec.panels[rank.panel].series[rank.index].name.clone())
    }

    fn select(app: &mut StokesForgeApp, graph: NodeId, index: usize) {
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| {
            view.select_series(Some(stokesforge_plot::SeriesRef::new(0, index)), &spec);
        });
    }

    #[test]
    fn deleting_a_group_makes_the_selected_series_follow() {
        let context = egui::Context::default();
        let group = stokesforge_plot::SeriesGroupRef::new(0, 1);
        let remove = |graph| Action::RemovePlotObject {
            graph,
            target: stokesforge_app_state::object_manager::VisibilityTarget::SeriesGroup(group),
        };
        for (selected, expected) in [(3, Some("D")), (0, Some("A")), (1, None)] {
            let (session, graph) = graph_with_group();
            let state = AppState::new(session.handle(), String::new()).unwrap();
            let mut app = StokesForgeApp::new(state);
            select(&mut app, graph, selected);
            app.dispatch_actions(&context, vec![remove(graph)]);
            let spec =
                stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
            assert_eq!(spec.panels[0].series.len(), 2, "group and series deleted");
            assert_eq!(
                selected_name(&app, graph).as_deref(),
                expected,
                "series {selected} selected before the deletion"
            );
            session.shutdown().unwrap();
        }
    }

    #[test]
    fn moving_a_group_makes_the_selected_series_follow() {
        let context = egui::Context::default();
        let (session, graph) = graph_with_group();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        select(&mut app, graph, 2);
        app.dispatch_actions(
            &context,
            vec![Action::MoveSeriesGroup {
                graph,
                group: stokesforge_plot::SeriesGroupRef::new(0, 1),
                up: false,
            }],
        );
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        let names: Vec<&str> = spec.panels[0]
            .series
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(names, ["A", "D", "B", "C"], "witness: the group moved");
        assert_eq!(selected_name(&app, graph).as_deref(), Some("C"));
        session.shutdown().unwrap();
    }

    #[test]
    fn duplicating_a_workbook_keeps_its_open_or_closed_state() {
        let (session, _) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        let book = app.state.books()[0].0;
        app.close_floating_tabs(vec![Tab::Book(book)]);
        app.dispatch_actions(&context, vec![Action::DuplicateObjects(vec![book])]);
        let closed = app
            .state
            .books()
            .into_iter()
            .find(|(_, name)| name == "Book1 copy 1")
            .unwrap()
            .0;
        assert!(!app.floating_tabs.contains(&Tab::Book(closed)));
        app.open_tab(Tab::Book(book));
        app.dispatch_actions(&context, vec![Action::DuplicateObjects(vec![book])]);
        let opened = app
            .state
            .books()
            .into_iter()
            .find(|(_, name)| name == "Book1 copy 2")
            .unwrap()
            .0;
        assert!(app.floating_tabs.contains(&Tab::Book(opened)));
    }

    /// S-F3 (#229): closing the window (cross, Alt+F4, File › Quit) is held back
    /// and becomes `Action::Quit`; with unsaved work, the question
    /// "Discard Changes · Cancel · Save" opens and the window stays;
    /// Discard releases it, and the next request is no longer held back.
    #[test]
    fn closing_the_window_asks_before_losing_work() {
        use eframe::App as _;
        let (session, _) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        theme::install(&context);
        egui_extras::install_image_loaders(&context);
        let mut frame = eframe::Frame::_new_kittest();
        let mut image = |app: &mut StokesForgeApp, close: bool| {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 700.0),
                )),
                ..Default::default()
            };
            if close {
                input
                    .viewports
                    .entry(egui::ViewportId::ROOT)
                    .or_default()
                    .events
                    .push(egui::ViewportEvent::Close);
            }
            let mut output = context.run_ui(input, |ui| {
                app.logic(ui.ctx(), &mut frame);
                app.ui(ui, &mut frame);
            });
            output.textures_delta.clear();
            output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .map(|viewport| viewport.commands.clone())
                .unwrap_or_default()
        };
        let commands = image(&mut app, true);
        assert!(commands.contains(&egui::ViewportCommand::CancelClose));
        assert!(!commands.contains(&egui::ViewportCommand::Close));
        assert_eq!(
            app.state.unsaved_pending(),
            Some(&stokesforge_app_state::state::UnsavedGesture::Quit)
        );
        app.dispatch_actions(
            &context,
            vec![Action::ConfirmUnsaved(UnsavedChoice::Discard)],
        );
        let commands = image(&mut app, false);
        assert!(
            commands.contains(&egui::ViewportCommand::Close),
            "Discard: the window closes"
        );
        let commands = image(&mut app, true);
        assert!(
            !commands.contains(&egui::ViewportCommand::CancelClose),
            "the released closing is no longer held back"
        );
        session.shutdown().unwrap();
    }

    #[test]
    fn the_painted_confirmation_empties_the_actions_of_the_frame() {
        let (session, _) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        theme::install(&context);
        egui_extras::install_image_loaders(&context);
        let mut actions = vec![Action::AddGraph];
        context
            .run_ui(egui::RawInput::default(), |ui| {
                app.blocking_confirmations(ui.ctx(), &mut actions);
            })
            .textures_delta
            .clear();
        assert_eq!(
            actions.len(),
            1,
            "without a confirmation, the action stays collected"
        );
        app.state.dispatch(Action::NewProject {
            pending_edits: true,
        });
        context
            .run_ui(egui::RawInput::default(), |ui| {
                app.blocking_confirmations(ui.ctx(), &mut actions);
            })
            .textures_delta
            .clear();
        assert!(actions.is_empty(), "only the dialog can act in this frame");
        session.shutdown().unwrap();
    }

    #[test]
    fn dispatch_opens_a_graph_only_if_the_command_creates_one() {
        let (session, _) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book = app.state.books()[0].0;
        let sheet = app.state.selected_sheet_in(book).unwrap();
        app.floating_tabs.clear();
        let context = egui::Context::default();
        let before = app.state.revision();
        app.dispatch_actions(
            &context,
            vec![Action::PlotColumns {
                sheet,
                columns: Vec::new(),
                plot_type: stokesforge_plot::PlotType::Line,
            }],
        );
        assert_eq!(app.state.revision(), before);
        assert!(
            app.floating_tabs.is_empty(),
            "no reopening of the last child"
        );

        app.dispatch_actions(&context, vec![Action::AddGraph]);
        assert!(app.state.revision() > before, "witness: effective creation");
        let graph = *app
            .state
            .tree()
            .node(app.state.tree().root())
            .unwrap()
            .children()
            .last()
            .unwrap();
        assert_eq!(app.floating_tabs, vec![Tab::Graph(graph)]);
        session.shutdown().unwrap();
    }

    #[test]
    fn ctrl_n_confirms_and_forgets_the_views_of_the_previous_project() {
        use eframe::App as _;
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        // The detached workbook, the floating graph: both kinds of view
        // (OS window, root `GraphView`) exist before Ctrl+N.
        let book = app.state.books()[0].0;
        app.detach_tab(Tab::Book(book));
        let context = egui::Context::default();
        set_header_rows(
            &context,
            book,
            HeaderRows {
                metadata: false,
                x_prime: false,
            },
        );
        context.set_embed_viewports(false);
        theme::install(&context);
        egui_extras::install_image_loaders(&context);
        let mut frame = eframe::Frame::_new_kittest();
        let mut paint = |app: &mut StokesForgeApp, events| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 700.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| app.ui(ui, &mut frame));
            output.textures_delta.clear();
            output
        };
        paint(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::N,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
        );
        // The dialog is now painted (visible at the second frame): we
        // look in it for Discard Changes.
        paint(&mut app, Vec::new());
        let output = paint(&mut app, Vec::new());
        let discard = super::test_support::texts(&output.shapes)
            .into_iter()
            .find(|text| text.text == tr!("unsaved-discard"))
            .expect("Discard Changes painted")
            .center;
        assert_eq!(
            app.state.unsaved_pending(),
            Some(&stokesforge_app_state::state::UnsavedGesture::NewProject)
        );
        assert!(app.state.tree().node(graph).is_some());
        // Witnesses: without them, the emptiness assertions further down would
        // pass as well on tables that never held anything.
        assert!(
            app.graph_views.contains_key(&graph),
            "view of the floating graph"
        );
        assert!(app.floating_rects.contains_key(&graph));
        assert!(app.floating_tabs.contains(&Tab::Graph(graph)));
        assert!(app.detached_tabs.contains_key(&Tab::Book(book)));
        paint(&mut app, vec![egui::Event::PointerMoved(discard)]);
        let button = |pressed| egui::Event::PointerButton {
            pos: discard,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        paint(&mut app, vec![button(true)]);
        let output = paint(&mut app, vec![button(false)]);
        assert_eq!(app.state.tree().len(), 1);
        assert!(app.floating_tabs.is_empty());
        assert!(app.detached_tabs.is_empty());
        assert!(app.graph_views.is_empty());
        assert!(app.floating_rects.is_empty());
        assert!(!header_rows(&context, Some(book)).metadata);
        assert!(
            output.viewport_output[&egui::ViewportId::from_hash_of(Tab::Book(book))]
                .commands
                .contains(&egui::ViewportCommand::Close)
        );
        session.shutdown().unwrap();
    }

    /// Issue #16: after saving and reloading, the windows
    /// come back in their state — maximized, collapsed, closed, and in their
    /// stacking order.
    #[test]
    fn window_state_survives_saving_and_reloading() {
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book = app.state.books()[0].0;
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(400.0, 300.0));
        app.remember_window_geometry(Tab::Graph(graph), rect);
        app.remember_window_geometry(Tab::Book(book), rect);
        // The graph is under the workbook and collapsed; the workbook is maximized.
        app.floating_tabs = vec![Tab::Graph(graph), Tab::Book(book)];
        app.collapsed_tabs.insert(Tab::Graph(graph));
        app.maximized_tab = Some(Tab::Book(book));
        app.sync_window_views();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("windows.stf");
        session.handle().save(&path).unwrap();

        let reloaded = Session::spawn(Project::new("Empty").unwrap(), None).unwrap();
        reloaded.handle().load(&path).unwrap();
        let state = AppState::new(reloaded.handle(), "test.stf".to_string()).unwrap();
        let app = StokesForgeApp::new(state);
        assert_eq!(app.maximized_tab, Some(Tab::Book(book)));
        assert!(
            app.floating_tabs.contains(&Tab::Graph(graph)),
            "detached → floating"
        );
        assert_eq!(
            app.floating_tabs.last(),
            Some(&Tab::Book(book)),
            "the workbook, on top, stays on top"
        );
        assert!(app.collapsed_tabs.contains(&Tab::Graph(graph)));
        assert!(app.detached_tabs.is_empty(), "detaching is not remembered");
        assert_eq!(app.floating_rects[&graph], rect);
    }

    /// Ctrl+" opens the import wizard, on an AZERTY keyboard (the 3 key
    /// without Shift) as on QWERTY (Shift+apostrophe), and not while it is
    /// already open.
    #[test]
    fn ctrl_quote_opens_the_import_wizard() {
        let (session, _graph) = plotted_graph();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let press = |key: egui::Key, modifiers: egui::Modifiers| egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        };
        for (key, modifiers) in [
            (egui::Key::Num3, egui::Modifiers::COMMAND),
            (
                egui::Key::Quote,
                egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            ),
        ] {
            assert!(app.state.import_state().is_none());
            let mut actions = Vec::new();
            let mut output = context.run_ui(press(key, modifiers), |ui| {
                app.collect_shortcuts(ui.ctx(), &mut actions);
            });
            output.textures_delta.clear();
            assert!(app.state.import_state().is_some(), "{key:?}");
            // Witness: the already open wizard is not reopened afresh by
            // a second Ctrl+" — the file list in progress survives.
            let file = std::path::PathBuf::from("in-progress.csv");
            app.state
                .import_state_mut()
                .unwrap()
                .paths
                .push(file.clone());
            let mut output = context.run_ui(press(key, modifiers), |ui| {
                app.collect_shortcuts(ui.ctx(), &mut actions);
            });
            output.textures_delta.clear();
            assert_eq!(
                app.state.import_state().map(|import| import.paths.clone()),
                Some(vec![file]),
                "{key:?} must not reopen the wizard that is already open"
            );
            app.state.cancel_import();
        }
    }

    /// Measurement (not a check): time of a full frame — egui pass and
    /// tessellation — at rest then while moving a floating window by its
    /// title bar.
    /// `cargo test -p stokesforge-app --release --lib window_move -- --ignored --nocapture`
    #[test]
    #[ignore = "performance measurement, to be run by hand"]
    fn window_move_measurement() {
        use eframe::App as _;
        let mut project = Project::new("Measure").unwrap();
        let root = project.root();
        let book = project.add_workbook(root, Some("Book1")).unwrap();
        let sheet = project
            .tree()
            .child_by_name(book, "Sheet1")
            .unwrap()
            .unwrap();
        let xs: Vec<f64> = (0..=120).map(|i| 400.0 + i as f64 * 2.5).collect();
        let ys: Vec<f64> = xs
            .iter()
            .map(|x| 0.85 * (-((x - 520.0) / 38.0).powi(2)).exp() + 0.04)
            .collect();
        let x = project
            .add_column(sheet, None, Some("Lambda"), Column::from_f64(&xs))
            .unwrap();
        let y = project
            .add_column(sheet, None, Some("Absorbance"), Column::from_f64(&ys))
            .unwrap();
        let mut panel = PanelSpec::at(0, 0);
        panel.series.push(SeriesSpec::new_xy(x, y));
        let graph = project
            .add_graph(
                root,
                Some("Graph1"),
                PlotSpec::empty("Spectrum").with_panels(panel),
            )
            .unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "measure.stf".to_string()).unwrap();
        let mut app = StokesForgeApp::new(state);
        app.open_tab(Tab::Book(book));
        app.open_tab(Tab::Graph(graph));

        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        egui_extras::install_image_loaders(&context);
        crate::theme::install(&context);
        let mut frame = eframe::Frame::_new_kittest();
        let mut events: Vec<egui::Event> = Vec::new();
        let mut now = 0.0;
        let mut frame_ms = |events: Vec<egui::Event>| -> (f64, f64, egui::FullOutput) {
            now += 1.0 / 60.0;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                time: Some(now),
                events,
                ..Default::default()
            };
            let t0 = std::time::Instant::now();
            let mut output = context.run_ui(input, |ui| app.ui(ui, &mut frame));
            let t1 = std::time::Instant::now();
            let primitives = context.tessellate(output.shapes.clone(), output.pixels_per_point);
            let t2 = std::time::Instant::now();
            std::hint::black_box(&primitives);
            output.textures_delta.clear();
            (
                (t1 - t0).as_secs_f64() * 1e3,
                (t2 - t1).as_secs_f64() * 1e3,
                output,
            )
        };
        let report = |label: &str, mut samples: Vec<(f64, f64)>| {
            samples.sort_by(|a, b| (a.0 + a.1).total_cmp(&(b.0 + b.1)));
            let n = samples.len() as f64;
            let ui_avg = samples.iter().map(|s| s.0).sum::<f64>() / n;
            let tess_avg = samples.iter().map(|s| s.1).sum::<f64>() / n;
            let (median, worst) = (samples[samples.len() / 2], samples[samples.len() - 1]);
            eprintln!(
                "{label:<28} ui avg {ui_avg:6.2} ms | tessellation avg {tess_avg:6.2} ms | \
                 median {:6.2} | worst {:6.2}",
                median.0 + median.1,
                worst.0 + worst.1
            );
        };

        // Warm-up (loading of textures, render caches).
        let mut last = frame_ms(Vec::new()).2;
        for _ in 0..10 {
            last = frame_ms(Vec::new()).2;
        }
        let idle: Vec<(f64, f64)> = (0..60)
            .map(|_| {
                let (a, b, _) = frame_ms(Vec::new());
                (a, b)
            })
            .collect();
        report("at rest", idle);

        // Moving the graph window (in the foreground) by its title.
        // The title of the window, not the Project Manager entry (on the left).
        let grab = last
            .shapes
            .iter()
            .filter_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) if shape.galley.job.text == "Graph1" => {
                    Some(shape.pos + shape.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .find(|pos| pos.x > 300.0)
            .expect("window title painted");
        events.push(egui::Event::PointerMoved(grab));
        frame_ms(std::mem::take(&mut events));
        events.push(egui::Event::PointerButton {
            pos: grab,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        frame_ms(std::mem::take(&mut events));
        let mut pos = grab;
        let moving: Vec<(f64, f64)> = (0..60)
            .map(|i| {
                pos += egui::vec2(if i < 30 { 6.0 } else { -6.0 }, 2.0);
                let (a, b, _) = frame_ms(vec![egui::Event::PointerMoved(pos)]);
                (a, b)
            })
            .collect();
        report("graph window moving", moving);
        let after = frame_ms(Vec::new()).2;
        eprintln!(
            "shapes painted: {} ; title {grab:?} -> {:?}",
            after.shapes.len(),
            after
                .shapes
                .iter()
                .filter_map(|painted| match &painted.shape {
                    egui::Shape::Text(shape) if shape.galley.job.text == "Graph1" => {
                        Some(shape.pos + shape.galley.rect.center().to_vec2())
                    }
                    _ => None,
                })
                .find(|pos| pos.x > 300.0)
        );
        frame_ms(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
    }

    #[test]
    fn embedded_data_stays_closed_at_startup_and_after_undo() {
        let mut project = stokesforge_core::Project::new("Test").unwrap();
        let root = project.root();
        let book = project.add_workbook(root, None).unwrap();
        project
            .set_window_meta(
                book,
                stokesforge_core::WindowMeta {
                    hidden: true,
                    ..Default::default()
                },
            )
            .unwrap();
        let sheet = project.tree().children(book).unwrap()[0];
        let column = project
            .add_column(sheet, None, None, stokesforge_core::Column::from_f64(&[1.]))
            .unwrap();
        let mut panel = stokesforge_plot::PanelSpec::default();
        panel.series.push(stokesforge_plot::SeriesSpec::new(column));
        let graph = project
            .add_graph(
                root,
                None,
                stokesforge_plot::PlotSpec::default()
                    .with_panels(stokesforge_plot::Panels::one(panel)),
            )
            .unwrap();
        let session = stokesforge_session::Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        assert_eq!(app.floating_tabs, vec![Tab::Graph(graph)]);
        let ctx = egui::Context::default();
        app.dispatch_action(&ctx, Action::ShowGraphData { graph });
        assert!(app.floating_tabs.contains(&Tab::Book(book)));
        app.dispatch_action(&ctx, Action::Undo);
        assert!(!app.floating_tabs.contains(&Tab::Book(book)));
        session.shutdown().unwrap();
    }
}
