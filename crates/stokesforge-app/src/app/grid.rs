//! The grid of a worksheet: virtualized table ([`grid_contents`]), stacked
//! header (letter, long name, unit, comment, formula, metadata), row-number
//! gutter, cells, edit menu shared by all its zones ([`grid_edit_menu`]),
//! keyboard focus (Tab, Shift+Tab and the arrows belong to the grid, #38:
//! [`GRID_KEYS`]), detached scroll bars and the grip that drags columns to a
//! graph, by the edge of a block of selected columns ([`column_grip_bands`],
//! #160).
//!
//! Moved out of `app.rs` as is (#120). The workbook around it (sheet tabs,
//! Workbook menu) lives in `workbook.rs`. The other modules read only
//! [`keyboard_focus_outside_grid`] from here (`app.rs` for the global shortcuts,
//! `dialogs.rs`, `graph_window.rs`) and the visibility of the "Metadata" row
//! (`workbook.rs`); [`role_label`] also serves the detached workbook test
//! (`window_detach.rs`).
//!
//! The tests are at the end of the file: they paint the workbook without a
//! window, inject keyboard and mouse, and read what is painted (texts,
//! positions) or the pushed actions, never the source text.

use eframe::egui;
use egui_extras::{Column as TableColumn, TableBuilder};
use stokesforge_core::{ColumnRole, ColumnType, NodeId};
use stokesforge_plot::PlotType;

use crate::icons;
use stokesforge_app_state::column_move::{target_order, ColumnMove};
use stokesforge_app_state::selection::{GridCursor, Selection};
use stokesforge_app_state::state::{
    Action, AppState, ColumnDragPayload, GridMenu, GridZone, HeaderField, MoveDirection,
    RowDragPayload,
};
use stokesforge_app_state::tr;

use super::empty_state::empty_state;
use super::graph_targets::{send_rows_to_graph_menu, send_to_graph_menu};
use super::inplace_edit;
use super::menu_sections::{menu_choice, MenuSections};
use super::shortcuts::{
    shortcut_text_backspace, shortcut_text_delete, SHORTCUT_COPY, SHORTCUT_COPY_FULL_COLUMNS,
    SHORTCUT_CUT, SHORTCUT_EDIT_CELL, SHORTCUT_PASTE, SHORTCUT_PASTE_ABSOLUTE, SHORTCUT_SELECT_ALL,
};

/// Height of a grid row, in points.
const ROW_HEIGHT: f32 = 20.0;
/// Default width of a data column in the docked grid. 54px (rather than the
/// historical 110px): explicit request, to fit more columns on screen — the user
/// can still resize with the mouse (`.resizable(true)`).
pub(super) const DEFAULT_COLUMN_WIDTH: f32 = 54.0;
/// Minimum width of a data column after a manual resize. Lower than the old
/// floor (56px) so as not to prevent a return to [`DEFAULT_COLUMN_WIDTH`] once it
/// has been widened then narrowed.
const MIN_COLUMN_WIDTH: f32 = 28.0;
/// Default width of the row-number gutter in the docked grid — widened (the old
/// value, 56px, already cut the "Long Name"/"Comments" header labels) and
/// resizable like the data columns rather than fixed (`TableColumn::exact`).
const ROW_NUMBER_COLUMN_WIDTH: f32 = 90.0;
/// Minimum width of the row-number gutter after a manual resize — enough to
/// keep `#` and a few digits readable.
const MIN_ROW_NUMBER_COLUMN_WIDTH: f32 = 32.0;
/// Height of a secondary row of the stacked header (letter, unit, comment,
/// formula).
const HEADER_SUBROW_HEIGHT: f32 = 18.0;
/// Height of the long-name row, slightly taller — it is the column's main name.
const HEADER_LONGNAME_HEIGHT: f32 = 20.0;
/// Total height of the stacked header of the docked grid with all its rows
/// (letter, x', long name, unit, comment, formula, metadata).
const HEADER_HEIGHT_STACKED: f32 = HEADER_LONGNAME_HEIGHT + 6.0 * HEADER_SUBROW_HEIGHT;

/// The optional rows of a workbook's header: "Metadata" and x'.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HeaderRows {
    pub(super) metadata: bool,
    pub(super) x_prime: bool,
}

impl HeaderRows {
    /// Without a stored preference: Metadata shown, x' hidden.
    const DEFAULT: Self = Self {
        metadata: true,
        x_prime: false,
    };
}

/// Height of the stacked header according to the displayed rows.
fn stacked_header_height(rows: HeaderRows) -> f32 {
    let hidden = usize::from(!rows.metadata) + usize::from(!rows.x_prime);
    HEADER_HEIGHT_STACKED - hidden as f32 * HEADER_SUBROW_HEIGHT
}

fn header_rows_default_id() -> egui::Id {
    egui::Id::new("stokesforge_header_rows_default")
}

/// Publishes the setting for **new** workbooks (preferences), every frame: the
/// only channel shared by the viewports, which share the `Context`.
pub(super) fn publish_header_row_defaults(ctx: &egui::Context, metadata: bool, x_prime: bool) {
    ctx.data_mut(|data| {
        data.insert_temp(header_rows_default_id(), HeaderRows { metadata, x_prime });
    });
}

/// The header rows of a workbook. A view setting **specific to the workbook**,
/// kept in egui's memory (one `Context` for all viewports: floating and detached
/// workbooks follow it together), never in the model or the history. As long as
/// the user has not touched it, it is the preferences setting at the moment they
/// look.
pub(super) fn header_rows(ctx: &egui::Context, book: Option<NodeId>) -> HeaderRows {
    ctx.data(|data| {
        book.and_then(|book| data.get_temp::<HeaderRows>(header_rows_id(book)))
            .or_else(|| data.get_temp::<HeaderRows>(header_rows_default_id()))
    })
    .unwrap_or(HeaderRows::DEFAULT)
}

fn header_rows_id(book: NodeId) -> egui::Id {
    egui::Id::new(("stokesforge_header_rows", book.raw()))
}

pub(super) fn set_header_rows(ctx: &egui::Context, book: NodeId, rows: HeaderRows) {
    ctx.data_mut(|data| data.insert_temp(header_rows_id(book), rows));
}

/// The workbook of a worksheet.
pub(super) fn book_of(tree: &stokesforge_core::Tree, sheet: NodeId) -> Option<NodeId> {
    tree.node(sheet).and_then(|node| node.parent())
}

/// Toggle of a row for the workbook `book` alone.
pub(super) fn toggle_header_row_menu(ui: &mut egui::Ui, book: NodeId) {
    let ctx = ui.ctx().clone();
    let mut rows = header_rows(&ctx, Some(book));
    let before = rows;
    ui.checkbox(&mut rows.metadata, tr!("menu-window-show-metadata-row"))
        .on_hover_text(tr!("menu-window-show-metadata-row", "hint"));
    ui.checkbox(&mut rows.x_prime, tr!("menu-workbook-show-x-prime-row"))
        .on_hover_text(tr!("menu-workbook-show-x-prime-row", "hint"));
    if rows != before {
        set_header_rows(&ctx, book, rows);
    }
}

/// Half-width, in points, of the grip band for dragging columns: it extends on
/// both sides of the vertical edge of a block of selected columns (#160).
const COLUMN_GRIP_HALF_WIDTH: f32 = 3.0;

/// Horizontal extent of a grid column: `(column, left, right)`.
type ColumnExtent = (NodeId, f32, f32);

/// Vertical extent of a **painted** grid row: `(row, top, bottom)`. The table
/// paints only the visible rows (virtualization).
type RowExtent = (usize, f32, f32);

/// Half-height, in points, of the grip band for dragging rows (#166).
const ROW_GRIP_HALF_HEIGHT: f32 = 3.0;

/// Grip bands for dragging rows to a graph (#166): one above and one below each
/// **block** of consecutive selected rows, on its outer edge, from `left` to
/// `right` — the row-number gutter, which selects on click, is excluded by the
/// caller. Blocks are read from `selected` (ascending), not from what is
/// painted: an edge exists only if its row is visible AND is really the edge of
/// the block, never because the block continues off screen. The bands stay
/// within `top..bottom` (the table body, without the header).
fn row_grip_bands(
    extents: &[RowExtent],
    selected: &[usize],
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
) -> Vec<egui::Rect> {
    let mut bands = Vec::new();
    let mut index = 0;
    while index < selected.len() {
        let first = selected[index];
        while index + 1 < selected.len() && selected[index + 1] == selected[index] + 1 {
            index += 1;
        }
        let last = selected[index];
        index += 1;
        let edges = [
            extents.iter().find(|e| e.0 == first).map(|e| e.1),
            extents.iter().find(|e| e.0 == last).map(|e| e.2),
        ];
        for y in edges.into_iter().flatten() {
            let band = egui::Rect::from_min_max(
                egui::pos2(left, (y - ROW_GRIP_HALF_HEIGHT).max(top)),
                egui::pos2(right, (y + ROW_GRIP_HALF_HEIGHT).min(bottom)),
            );
            if band.height() > 0.0 {
                bands.push(band);
            }
        }
    }
    bands
}

/// Grip bands for dragging columns to a graph (#160): one on each side of each
/// **block** of consecutive selected columns, on its outer edge. They cover the
/// height `top..bottom` — the letter row, where column edges serve for
/// resizing, is excluded by the caller. Pure function: testable without a
/// window, like the rest of this module's decisions.
fn column_grip_bands(
    extents: &[ColumnExtent],
    selected: &[NodeId],
    top: f32,
    bottom: f32,
) -> Vec<egui::Rect> {
    let mut bands = Vec::new();
    let mut index = 0;
    while index < extents.len() {
        if !selected.contains(&extents[index].0) {
            index += 1;
            continue;
        }
        let first = index;
        while index + 1 < extents.len() && selected.contains(&extents[index + 1].0) {
            index += 1;
        }
        for x in [extents[first].1, extents[index].2] {
            bands.push(egui::Rect::from_min_max(
                egui::pos2(x - COLUMN_GRIP_HALF_WIDTH, top),
                egui::pos2(x + COLUMN_GRIP_HALF_WIDTH, bottom),
            ));
        }
        index += 1;
    }
    bands
}

/// Horizontal scroll area of the grid. Its own bar is hidden: the visible bar is
/// moved **under the sheet tabs** ([`show_grid_horizontal_bar`]), as in a
/// spreadsheet. Scrolling (wheel with Shift, drag) stays active; an offset
/// requested by the moved bar in the previous frame (`pending_offset`) is
/// applied here.
pub(super) fn show_horizontally_scrollable_grid<R>(
    ui: &mut egui::Ui,
    pending_offset: Option<f32>,
    add_grid: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    let mut area = egui::ScrollArea::horizontal()
        .auto_shrink([false, true])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    if let Some(offset) = pending_offset {
        area = area.horizontal_scroll_offset(offset);
    }
    area.show(ui, add_grid)
}

/// Horizontal scroll bar of the grid, moved into the strip reserved **under the
/// sheet tabs** (`strip`). Same principle as [`show_grid_vertical_bar`]: an
/// empty horizontal `ScrollArea`, as wide as the grid's content, that mirrors
/// its offset; a drag on this bar is put on hold (`pending_id`) and applied to
/// the grid in the next frame ([`show_horizontally_scrollable_grid`]). The strip
/// has the grid's width: no scaling.
pub(super) fn show_grid_horizontal_bar<R>(
    ui: &mut egui::Ui,
    pending_id: egui::Id,
    strip: egui::Rect,
    grid: &egui::scroll_area::ScrollAreaOutput<R>,
) {
    if !strip.is_positive() {
        return;
    }
    let grid_offset = grid.state.offset.x;
    let mut bar_ui = ui.new_child(egui::UiBuilder::new().max_rect(strip));
    let bar = egui::ScrollArea::horizontal()
        .id_salt(pending_id)
        .auto_shrink([false, false])
        .horizontal_scroll_offset(grid_offset)
        .show(&mut bar_ui, |ui| {
            ui.allocate_exact_size(egui::vec2(grid.content_size.x, 0.0), egui::Sense::hover());
        });
    let dragged_offset = bar.state.offset.x;
    if (dragged_offset - grid_offset).abs() > 0.5 {
        ui.memory_mut(|memory| memory.data.insert_temp(pending_id, dragged_offset));
        ui.ctx().request_repaint();
    }
}

/// Scale ratio between the grid's vertical bar and the table body it drives:
/// the bar covers the whole window height (`bar_height`), while the body
/// scrolls only its own (`body_height`, under the frozen header). The bar's
/// content and offset are the table's multiplied by this ratio: the handle thus
/// keeps the same proportion (visible / total) and the same relative position as
/// if it covered only the body. `1.0` when the body has no height.
fn grid_vertical_bar_scale(bar_height: f32, body_height: f32) -> f32 {
    if body_height > 0.0 {
        bar_height / body_height
    } else {
        1.0
    }
}

/// Vertical scroll bar of the grid (B10), stuck to the right edge of the window
/// rather than to the right edge of the table, and over the whole window height
/// (`strip`, header and sheet tabs included).
///
/// The table (`TableBuilder`) scrolls vertically itself — that is what freezes
/// its header — but, drawn in the horizontal scroll area, its own bar would
/// follow the width of its columns. Its bar is therefore hidden, and this one
/// mirrors it: an empty vertical `ScrollArea`, placed in the reserved strip,
/// whose content height is the table body's scaled to the bar
/// ([`grid_vertical_bar_scale`]). The offset follows the table (wheel, drag in
/// the table); conversely, a drag on this bar is put on hold in memory
/// (`pending_id`), scaled back to the table, and applied to it in the next frame
/// (`grid_contents`, `pending_vertical_offset`).
pub(super) fn show_grid_vertical_bar(
    ui: &mut egui::Ui,
    pending_id: egui::Id,
    strip: egui::Rect,
    table: &egui::scroll_area::ScrollAreaOutput<()>,
) {
    if !strip.is_positive() {
        return;
    }
    let scale = grid_vertical_bar_scale(strip.height(), table.inner_rect.height());
    let bar_offset = table.state.offset.y * scale;
    let mut bar_ui = ui.new_child(egui::UiBuilder::new().max_rect(strip));
    // RAW-SCROLL(#347): mirror bar, its height is computed by grid_vertical_bar_scale
    let bar = egui::ScrollArea::vertical()
        .id_salt(pending_id)
        .auto_shrink([false, false])
        .vertical_scroll_offset(bar_offset)
        .show(&mut bar_ui, |ui| {
            ui.allocate_exact_size(
                egui::vec2(0.0, table.content_size.y * scale),
                egui::Sense::hover(),
            );
        });
    let dragged_offset = bar.state.offset.y;
    if (dragged_offset - bar_offset).abs() > 0.5 {
        let table_offset = dragged_offset / scale;
        ui.memory_mut(|memory| memory.data.insert_temp(pending_id, table_offset));
        ui.ctx().request_repaint();
    }
}

/// An icon followed by a name (a folder or window name, chosen by the user:
/// never translated, R15).
fn icon_label(icon: &str, name: &str) -> String {
    format!("{icon} {name}")
}

/// Name of a column type, as offered by the menu of the letter row.
fn type_label(column_type: ColumnType) -> String {
    match column_type {
        ColumnType::Numeric => tr!("column-type-numeric"),
        ColumnType::DateTime => tr!("column-type-datetime"),
        ColumnType::Mixed => tr!("column-type-mixed"),
    }
}

/// Name of the role, shown in parentheses after the column letter to recall its
/// plot role and in the "Role" submenu ([`stokesforge_core::ColumnRole`]) —
/// `A(x)`, `B(y)`… For now this role is read by nothing but persistence
/// (`stokesforge-persistence`): no series resolution uses it yet (see
/// `stokesforge_core::resolve`, which determines X/Y otherwise) — showing it
/// and making it editable here therefore has, for now, only informative value,
/// ahead of its wiring to rendering.
pub(super) fn role_label(role: ColumnRole) -> String {
    match role {
        ColumnRole::X => tr!("workbook-role-x"),
        ColumnRole::Y => tr!("workbook-role-y"),
        ColumnRole::X2 => tr!("workbook-role-x2"),
        ColumnRole::Y2 => tr!("workbook-role-y2"),
        ColumnRole::X3 => tr!("workbook-role-x3"),
        ColumnRole::Y3 => tr!("workbook-role-y3"),
        ColumnRole::X4 => tr!("workbook-role-x4"),
        ColumnRole::Y4 => tr!("workbook-role-y4"),
        ColumnRole::Z => tr!("workbook-role-z"),
        ColumnRole::XError => tr!("column-role-x-error"),
        ColumnRole::YError => tr!("column-role-y-error"),
        ColumnRole::Label => tr!("workbook-role-label"),
        ColumnRole::None => tr!("workbook-role-none"),
    }
}

/// Spreadsheet-style column letter, in base 26: `0 → "A"`, `25 → "Z"`,
/// `26 → "AA"`, `701 → "ZZ"`… — the order in the worksheet, not a name stored in
/// the model.
///
/// Simple alias of [`stokesforge_core::spreadsheet_name`], which already held
/// this algorithm (it names columns created without an explicit name). This
/// module had its own copy, so the letter shown in the header and the one used
/// as the default name of a new column came from two distinct implementations —
/// and, since history labels also designate a column by its letter
/// (`stokesforge_commands::Command::label`), a third would have been needed.
fn column_letter(index: usize) -> String {
    stokesforge_core::spreadsheet_name(index)
}

/// True if `column` is the last visible column of `sheet` — a condition shared
/// by the two places where Tab must add a column rather than return to the start
/// of the next row (see `Action::AddColumnAndSelect`): the global keyboard
/// shortcut of `grid_contents` and the edit validation of `cell_widget`.
fn is_last_visible_column(state: &AppState, sheet: NodeId, column: NodeId) -> bool {
    state
        .visible_columns(sheet)
        .last()
        .is_some_and(|(last, ..)| *last == column)
}

/// Contents of the **system** clipboard, read on demand — for "Paste" taken from
/// a context menu, which has no `egui::Event::Paste` to rely on.
///
/// Ctrl+V works without this because `egui-winit` reads the clipboard itself and
/// pushes the text into the event; egui exposes no **read** API (only
/// `Context::copy_text`, for writing). Hence this direct read through `arboard`
/// — the library `egui-winit` already uses internally, so already in the
/// dependency tree.
///
/// A new handle on each call, rather than one kept alive: it is the **write**
/// that requires, under X11, keeping the process that owns the clipboard alive,
/// never the read, which is a single round trip. And a menu opens too rarely for
/// this cost to matter — which is also why "Paste" is never grayed out: knowing
/// whether there is something to paste would require this read on *every frame*
/// the menu is open. We try, and say so if it yields nothing.
///
/// `None` if the clipboard is inaccessible (no display server, portal
/// refused…) or if it holds no text (an image, for example).
fn system_clipboard_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// Edit block shared by all the grid's context menus: the keyboard shortcuts of
/// [`grid_contents`], contextualized to what the current selection really
/// touches (see [`AppState::grid_menu`] and [`GridMenu`]).
///
/// The same block, in the same order, on the three zones — data cell, header
/// field, letter row: only the labels change, and "Delete and shift" disappears
/// in the header, where Delete has nothing to bring up. The letter row's menu
/// then adds what is its own (column type and role, see
/// [`column_header_widget`]).
///
/// Opens nothing if [`AppState::grid_menu`] returns `None` (no valid selection
/// left): the caller must then refrain from placing a menu, rather than placing
/// an empty one.
fn grid_edit_menu(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    menu: &GridMenu,
) {
    let mut sections = MenuSections::new();
    grid_clipboard_section(ui, &mut sections, state, actions, sheet, menu);
    grid_erase_section(ui, &mut sections, state, actions, sheet, menu);
}

/// Grid clipboard, **a single group** (#152): Cut · Copy · Copy with Header ·
/// Paste · Paste in Place. No Undo/Redo on right-click: Edit and the keyboard
/// are enough.
fn grid_clipboard_section(
    ui: &mut egui::Ui,
    sections: &mut MenuSections,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    menu: &GridMenu,
) {
    let ctx = ui.ctx().clone();
    sections.section(ui, true, |ui| {
        if ui
            .add_enabled(
                menu.copy_enabled,
                egui::Button::new(tr!("action-cut"))
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_CUT)),
            )
            .on_disabled_hover_text(tr!("action-copy", "disabled"))
            .clicked()
        {
            // Cut = copy then clear, exactly like the keyboard block: `copy_selection`
            // sets the internal clipboard *and* returns the text to push to the system
            // clipboard.
            if let Some(text) = state.copy_selection(sheet) {
                ctx.copy_text(text);
            }
            state.forget_delete_repeat(sheet);
            actions.push(Action::ClearSelectedCells { sheet });
            ui.close();
        }
        if ui
            .add_enabled(
                menu.copy_enabled,
                egui::Button::new(tr!("action-copy"))
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_COPY)),
            )
            .on_disabled_hover_text(tr!("action-copy", "disabled"))
            .clicked()
        {
            if let Some(text) = state.copy_selection(sheet) {
                ctx.copy_text(text);
            }
            ui.close();
        }
        if ui
            .add(
                egui::Button::new(tr!("action-copy-with-header"))
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_COPY_FULL_COLUMNS)),
            )
            .on_hover_text(tr!("action-copy-with-header", "hint"))
            .clicked()
        {
            if let Some(text) = state.copy_full_columns(sheet) {
                ctx.copy_text(text);
            }
            ui.close();
        }
        if ui
            .add_enabled(
                menu.paste_enabled,
                egui::Button::new(tr!("action-paste"))
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_PASTE)),
            )
            .on_disabled_hover_text(tr!("action-paste", "disabled"))
            .clicked()
        {
            state.forget_delete_repeat(sheet);
            match system_clipboard_text() {
                Some(text) => actions.push(Action::Paste { sheet, text }),
                None => state.warn(tr!("status-clipboard-unreadable")),
            }
            ui.close();
        }
        if ui
            .add_enabled(
                menu.paste_enabled && menu.paste_absolute_enabled,
                egui::Button::new(tr!("action-paste-in-place"))
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_PASTE_ABSOLUTE)),
            )
            .on_hover_text(tr!("action-paste-in-place", "hint"))
            .on_disabled_hover_text(if menu.paste_enabled {
                tr!("action-paste-in-place", "disabled")
            } else {
                tr!("action-paste", "disabled")
            })
            .clicked()
        {
            state.forget_delete_repeat(sheet);
            actions.push(Action::PasteAbsolute { sheet });
            ui.close();
        }
    });
}

/// Clear and delete: Remove Formula, Keep Data (if a formula is selected, #236)
/// · Clear · Delete (Shift Up).
fn grid_erase_section(
    ui: &mut egui::Ui,
    sections: &mut MenuSections,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    menu: &GridMenu,
) {
    // A range of whole columns has nothing to "bring up": Delete removes the
    // columns themselves (`AppState::delete_selected_cells`).
    let columns = menu.zone == GridZone::Columns;
    sections.section(ui, true, |ui| {
        if menu.offers_keep_formula_data
            && ui
                .add(egui::Button::new(tr!("action-remove-formula-keep-data")))
                .on_hover_text(tr!("action-remove-formula-keep-data", "hint"))
                .clicked()
        {
            state.forget_delete_repeat(sheet);
            actions.push(Action::RemoveFormulaKeepData { sheet });
            ui.close();
        }
        if ui
            .add(egui::Button::new(tr!("action-clear")).shortcut_text(shortcut_text_backspace()))
            .on_hover_text(tr!("action-clear", "hint"))
            .clicked()
        {
            state.forget_delete_repeat(sheet);
            actions.push(Action::ClearSelectedCells { sheet });
            ui.close();
        }
        if menu.offers_delete_shift
            && ui
                .add(
                    egui::Button::new(if columns {
                        tr!("action-delete-columns")
                    } else {
                        tr!("action-delete-shift-up")
                    })
                    .shortcut_text(shortcut_text_delete()),
                )
                .on_hover_text(if columns {
                    tr!("action-delete-columns", "hint")
                } else {
                    tr!("action-delete-shift-up", "hint")
                })
                .clicked()
        {
            // Go through the action directly, without `consume_delete_key`: a context menu
            // is an explicit, designated gesture, whereas the double press on Delete exists
            // only to protect against a reflex keystroke. The running counter is forgotten
            // all the same, so that a press on Delete right after does not start from the
            // wrong time.
            state.forget_delete_repeat(sheet);
            actions.push(Action::DeleteSelectedCells { sheet });
            ui.close();
        }
    });
}

/// Stacked header of a column: letter (click → selects the whole column), then
/// long name, unit, comment and formula — the four fields of
/// [`stokesforge_core::ColumnMeta`], until now stored without ever being shown
/// in the UI. Each toggles display/edit on double-click, like [`cell_widget`].
///
/// The letter row has a distinct background
/// ([`crate::theme::header_letter_row_fill`]) — it alone selects the column on
/// click; the four field rows that follow show only a grayed label repeated per
/// column (their field name lives once in the row-number gutter, see
/// [`row_number_header_widget`]): they show only the field **value**, empty if
/// the field is.
fn column_header_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    column: NodeId,
    column_index: usize,
) {
    ui.vertical(|ui| {
        // Without this, egui's default spacing between children of a `vertical`
        // (`Spacing::item_spacing.y`) leaves a thin white line between each stacked
        // row — the background color then does not really fill the cell, it floats
        // in it.
        ui.spacing_mut().item_spacing.y = 0.0;
        let is_column_selected =
            column_letter_widget(ui, state, actions, sheet, column, column_index);
        let rows = header_rows(ui.ctx(), book_of(state.tree(), sheet));
        if rows.x_prime {
            header_field_widget(
                ui,
                state,
                actions,
                sheet,
                column,
                HeaderField::XPrime,
                HEADER_SUBROW_HEIGHT,
                is_column_selected,
            );
        }
        header_field_widget(
            ui,
            state,
            actions,
            sheet,
            column,
            HeaderField::LongName,
            HEADER_LONGNAME_HEIGHT,
            is_column_selected,
        );
        header_field_widget(
            ui,
            state,
            actions,
            sheet,
            column,
            HeaderField::Unit,
            HEADER_SUBROW_HEIGHT,
            is_column_selected,
        );
        header_field_widget(
            ui,
            state,
            actions,
            sheet,
            column,
            HeaderField::Comment,
            HEADER_SUBROW_HEIGHT,
            is_column_selected,
        );
        header_field_widget(
            ui,
            state,
            actions,
            sheet,
            column,
            HeaderField::Formula,
            HEADER_SUBROW_HEIGHT,
            is_column_selected,
        );
        if rows.metadata {
            metadata_cell_widget(ui, state, actions, column);
        }
    });
}

/// Letter row: column selection, then menu on the selection.
fn column_letter_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    column: NodeId,
    column_index: usize,
) -> bool {
    let letter = column_letter(column_index);
    let is_column_selected = state.is_column_selected(sheet, column);
    let touched = state.column_touched_by_selection(sheet, column);
    let role = state.column_meta(column).role;
    ui.scope(|ui| {
        ui.set_min_height(HEADER_SUBROW_HEIGHT);
        ui.set_max_height(HEADER_SUBROW_HEIGHT);
        let rect = ui.available_rect_before_wrap();
        ui.painter()
            .rect_filled(rect, 0.0, gutter_fill(is_column_selected, touched));
        header_row_separator(ui, rect);
        // The whole row width selects the column — not only the letter —, dragging or
        // Shift-click extends to several columns (same logic as the multi-selection of
        // data cells, see `cell_widget`), on the primary click as on the secondary
        // click (`response.secondary_clicked()` also covers
        // `Sense::click_and_drag()`): a right-click must be able to select *and* open
        // the context menu in a single gesture, without requiring the column to be
        // already selected beforehand.
        let id = ui
            .id()
            .with(("stokesforge_column_letter_row", column.raw()));
        let response = ui.interact(rect, id, egui::Sense::click_and_drag());
        let letter_text = format!("{letter}({})", role_label(role));
        // Column of an out-of-date calculation (#181, M9 a): tinted letter, the reason
        // in the tooltip.
        let stale_text = state.column_stale_text(column);
        let letter_color = if stale_text.is_some() {
            crate::theme::stale_color(ui.visuals().dark_mode)
        } else {
            crate::theme::gutter_text_color()
        };
        ui.put(
            rect,
            egui::Label::new(egui::RichText::new(letter_text).color(letter_color))
                .selectable(false),
        );
        let corrected = state
            .tree()
            .correction_traces()
            .values()
            .any(|trace| trace.series.iter().any(|source| source.y == column));
        // A single marker in the margin: the out-of-date calculation warning keeps
        // priority, the trace of the corrections stays in the bubble.
        if corrected {
            if stale_text.is_none() && rect.width() >= 3.0 * crate::icons::ICON_SIZE {
                let size = egui::Vec2::splat(crate::icons::ICON_SIZE);
                let at = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + 2.0, rect.center().y - size.y / 2.0),
                    size,
                );
                crate::icons::icon_image(
                    crate::icons::Icon::ModifiedData,
                    crate::icons::IconRole::Neutral,
                )
                .paint_at(ui, at);
            }
            response.clone().on_hover_text(tr!("grid-column-modified"));
        }
        if let Some(text) = stale_text {
            // The warning icon to the left of the letter (#200), if the column is wide
            // enough not to bite into it.
            if rect.width() >= 3.0 * crate::icons::ICON_SIZE {
                let size = egui::Vec2::splat(crate::icons::ICON_SIZE);
                let at = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + 2.0, rect.center().y - size.y / 2.0),
                    size,
                );
                crate::icons::icon_image(
                    crate::icons::Icon::Stale,
                    crate::icons::IconRole::Neutral,
                )
                .paint_at(ui, at);
            }
            response.clone().on_hover_text(text);
        }
        let modifiers = ui.input(|i| i.modifiers);
        let press_here = press_started_in(ui, rect);
        if response.clicked() || response.drag_started() {
            // Ctrl+click toggles the column, Shift+click takes the anchor → column range,
            // Ctrl+Shift+click adds it to the selection (#159).
            match (modifiers.command, modifiers.shift) {
                (true, true) => state.extend_column_selection(sheet, column),
                (true, false) => state.toggle_column_selection(sheet, column),
                (false, true) => state.extend_column_selection_replacing(sheet, column),
                (false, false) => state.select_column(sheet, column),
            }
        } else if response.secondary_clicked() && !is_column_selected {
            // `&& !is_column_selected`: a right-click *inside* the selection (even a
            // non-contiguous one) leaves it intact and the menu acts on the whole
            // selection; outside the selection, it replaces it with the targeted column
            // (#152, #159).
            state.select_column(sheet, column);
        } else if response.contains_pointer()
            && !press_here
            && !grid_drag_in_progress(ui.ctx())
            && ui.input(|i| i.pointer.primary_down())
            && state.selection(sheet).is_some()
        {
            // Drag started from another letter: extend on hover. A press started on this
            // letter does not belong here — otherwise a Ctrl+click, which toggles on
            // release, would already have added the anchor → column range during the press.
            state.extend_column_selection(sheet, column);
        }
        // Columns targeted by the context menu: the whole selected range if `column`
        // is part of it (menu opened on an already placed multiple selection), else
        // `column` alone — a right-click outside the selected range has already reduced
        // it to itself above.
        let target_columns: Vec<NodeId> = state
            .selection(sheet)
            .map(|selection| selection.columns(state.tree(), sheet))
            .filter(|columns| columns.contains(&column))
            .unwrap_or_else(|| vec![column]);
        response.context_menu(|ui| {
            column_properties_menu(ui, state, actions, sheet, &target_columns);
        });
    });
    is_column_selected
}

/// Menu of the letter row (#152): plot and add, clipboard, column type and role,
/// then clear — flat, without a "Columns ▸" wrapper. Lazy computation of
/// [`AppState::grid_menu`], for the same reason as in `cell_widget`.
fn column_properties_menu(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    target_columns: &[NodeId],
) {
    let menu = state.grid_menu(sheet);
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        plot_menu(ui, actions, sheet, target_columns);
        send_to_graph_menu(ui, state.tree(), actions, sheet, target_columns);
        add_column_entry(ui, sheet, actions);
        super::analysis_menu::analysis_entry(
            ui,
            state,
            actions,
            super::analysis_menu::AnalysisTarget::Columns {
                sheet,
                columns: target_columns,
            },
        );
    });
    if let Some(menu) = &menu {
        grid_clipboard_section(ui, &mut sections, state, actions, sheet, menu);
    }
    sections.section(ui, true, |ui| {
        column_type_role_menus(ui, state, actions, target_columns);
    });
    super::analysis_menu::calculation_section(ui, &mut sections, state, actions, target_columns);
    if let Some(menu) = &menu {
        grid_erase_section(ui, &mut sections, state, actions, sheet, menu);
    }
}

/// Menu of a selection of **whole rows** of an x' sheet (#166): Plot and Send to
/// plot the rows (one series per row, against the x'), then clipboard and clear
/// as for columns.
fn row_properties_menu(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    payload: &RowDragPayload,
) {
    let sheet = payload.sheet;
    let menu = state.grid_menu(sheet);
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        plot_rows_menu(ui, actions, payload);
        send_rows_to_graph_menu(ui, state.tree(), actions, payload);
        let rows: Vec<(NodeId, usize)> = payload.rows.iter().map(|row| (sheet, *row)).collect();
        super::analysis_menu::analysis_entry(
            ui,
            state,
            actions,
            super::analysis_menu::AnalysisTarget::Rows { rows: &rows },
        );
    });
    if let Some(menu) = &menu {
        grid_clipboard_section(ui, &mut sections, state, actions, sheet, menu);
        grid_erase_section(ui, &mut sections, state, actions, sheet, menu);
    }
}

/// "Plot" of a selection of rows: the same plot types as [`plot_menu`], pushing
/// [`Action::PlotRows`].
pub(super) fn plot_rows_menu(
    ui: &mut egui::Ui,
    actions: &mut Vec<Action>,
    payload: &RowDragPayload,
) {
    ui.menu_button(tr!("workbook-plot-menu"), |ui| {
        for (label, plot_type) in [
            (tr!("plot-type-line"), PlotType::Line),
            (tr!("plot-type-scatter"), PlotType::Scatter),
            (tr!("plot-type-line-symbol"), PlotType::LineScatter),
            (tr!("plot-type-bar"), PlotType::Bar),
        ] {
            if ui.button(label).clicked() {
                actions.push(Action::PlotRows {
                    sheet: payload.sheet,
                    rows: payload.rows.clone(),
                    plot_type,
                });
                ui.close();
            }
        }
    });
}

/// The right-click on a grid box (cell, row number, header field, corner). On a
/// selection of **whole columns or rows**, it is the menu of the column letters
/// — Plot, Send to, clipboard, type and role, clear — applied to the columns of
/// the selection; on a range of cells or fields, clipboard and clear only.
fn selection_context_menu(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
) {
    let whole = state
        .selection(sheet)
        .filter(|selection| selection.is_columns() || selection.is_full_rows())
        .map(|selection| selection.columns(state.tree(), sheet));
    // Whole rows on an x' sheet (#166): plot the **rows**, not all the columns
    // they cross.
    if let Some(payload) = state.row_drag_payload(sheet) {
        row_properties_menu(ui, state, actions, &payload);
        return;
    }
    match whole {
        Some(columns) if !columns.is_empty() => {
            column_properties_menu(ui, state, actions, sheet, &columns);
        }
        _ => {
            if let Some(menu) = state.grid_menu(sheet) {
                grid_edit_menu(ui, state, actions, sheet, &menu);
            }
        }
    }
}

/// Add available even without a selection, shared by the workbook and the empty
/// part of the grid.
fn add_column_entry(ui: &mut egui::Ui, sheet: NodeId, actions: &mut Vec<Action>) {
    if ui.button(tr!("workbook-column-add")).clicked() {
        actions.push(Action::AddColumn { sheet });
        ui.close();
    }
}

/// `Move ▸` submenu (#319): four entries that move the targeted columns within
/// their worksheet (never elsewhere). Painted by `column_type_role_menus`, so a
/// single implementation for the right-click on letters and the Columns menu of
/// the bar. An entry is **grayed out**, reason in a tooltip, when the gesture
/// would change nothing — all or nothing: a single column that cannot move
/// grays out Left or Right for all (`column_move::target_order`, the same
/// decision as the one that executes).
fn move_columns_menu(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    target_columns: &[NodeId],
) {
    let sheet = target_columns
        .first()
        .and_then(|column| state.tree().node(*column)?.parent());
    let order: Vec<NodeId> = sheet
        .map(|sheet| {
            state
                .visible_columns(sheet)
                .into_iter()
                .map(|(id, _, _)| id)
                .collect()
        })
        .unwrap_or_default();
    ui.menu_button(tr!("column-move"), |ui| {
        let entries = [
            (
                ColumnMove::First,
                tr!("action-move-columns-first"),
                tr!("action-move-columns-first", "hint"),
                tr!("action-move-columns-first", "disabled"),
            ),
            (
                ColumnMove::Left,
                tr!("action-move-columns-left"),
                tr!("action-move-columns-left", "hint"),
                tr!("action-move-columns-left", "disabled"),
            ),
            (
                ColumnMove::Right,
                tr!("action-move-columns-right"),
                tr!("action-move-columns-right", "hint"),
                tr!("action-move-columns-right", "disabled"),
            ),
            (
                ColumnMove::Last,
                tr!("action-move-columns-last"),
                tr!("action-move-columns-last", "hint"),
                tr!("action-move-columns-last", "disabled"),
            ),
        ];
        for (to, label, hint, disabled) in entries {
            let available = target_order(&order, target_columns, to).is_some();
            let clicked = ui
                .add_enabled(available, egui::Button::new(label))
                .on_hover_text(hint)
                .on_disabled_hover_text(disabled)
                .clicked();
            if let (true, Some(sheet)) = (clicked, sheet) {
                actions.push(Action::MoveColumns {
                    sheet,
                    columns: target_columns.to_vec(),
                    to,
                });
                ui.close();
            }
        }
    });
}

/// Type and role of the targeted columns: same target and same actions in the
/// Workbook menu and on right-click. A heterogeneous selection ticks no choice.
pub(super) fn column_type_role_menus(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    target_columns: &[NodeId],
) {
    let columns: Vec<_> = target_columns
        .iter()
        .filter_map(|id| state.tree().node(*id)?.as_column())
        .collect();
    let first = columns.first();
    let column_type = first
        .map(|column| column.column_type())
        .filter(|kind| columns.iter().all(|column| column.column_type() == *kind));
    let role = first
        .map(|column| column.meta().role)
        .filter(|role| columns.iter().all(|column| column.meta().role == *role));
    ui.add_enabled_ui(!target_columns.is_empty(), |ui| {
        move_columns_menu(ui, state, actions, target_columns);
        // Manual choice of the column type — destructive conversion accepted
        // (`Action::SetColumnType`/`Command::
        // SetColumnType`, `stokesforge-core::Column::set_column_type`):
        // non-numeric text cleared when switching to `Numeric`, as for any
        // direct entry in a numeric column.
        ui.menu_button(tr!("column-type"), |ui| {
            for candidate in [ColumnType::Numeric, ColumnType::Mixed] {
                if menu_choice(ui, column_type == Some(candidate), type_label(candidate)).clicked()
                {
                    if Some(candidate) != column_type {
                        actions.push(Action::SetColumnsType {
                            columns: target_columns.to_vec(),
                            column_type: candidate,
                        });
                    }
                    ui.close();
                }
            }
        });
        // The role describes the columns, and stays shared with the Workbook menu
        // like the type.
        ui.menu_button(tr!("column-role"), |ui| {
            for candidate in ColumnRole::ALL {
                if menu_choice(ui, role == Some(candidate), role_label(candidate)).clicked() {
                    if Some(candidate) != role {
                        actions.push(Action::SetColumnsRole {
                            columns: target_columns.to_vec(),
                            role: candidate,
                        });
                    }
                    ui.close();
                }
            }
        });
        // View setting (#330): not undoable, no confirmation, like setting a width.
        // Grayed out when no targeted column has a stored width — the selection is
        // handled as a whole.
        let resettable = !state.columns_with_width(target_columns).is_empty();
        if ui
            .add_enabled(
                resettable,
                egui::Button::new(tr!("action-reset-column-widths")),
            )
            .on_hover_text(tr!("action-reset-column-widths", "hint"))
            .on_disabled_hover_text(tr!("action-reset-column-widths", "disabled"))
            .clicked()
        {
            actions.push(Action::ResetColumnWidths {
                columns: target_columns.to_vec(),
            });
            ui.close();
        }
    });
}

/// "Reset Column Widths" entry of a sheet (#330): Workbook ▸ Sheet and the tab's
/// right-click (on the tab side, the tab strip has no state: see
/// `TabEvent::ResetColumnWidths`). All the columns of the sheet; the row-number
/// gutter also returns to its default width, without being recorded.
pub(super) fn reset_sheet_widths_entry(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
) {
    let columns = state.sheet_column_ids(sheet);
    let resettable = !state.columns_with_width(&columns).is_empty();
    if ui
        .add_enabled(
            resettable,
            egui::Button::new(tr!("action-reset-sheet-column-widths")),
        )
        .on_hover_text(tr!("action-reset-sheet-column-widths", "hint"))
        .on_disabled_hover_text(tr!("action-reset-sheet-column-widths", "disabled"))
        .clicked()
    {
        forget_gutter_width(ui.ctx(), sheet);
        actions.push(Action::ResetColumnWidths { columns });
        ui.close();
    }
}

/// Create a graph from the targeted columns — work package "plot from a
/// workbook". The X/Y pairing rule (each Y takes the nearest X column to its
/// left) lives in `AppState::plot_columns`/`series_from_columns`; this menu only
/// chooses the plot type and pushes the action on `target_columns`. First level
/// of the right-click on letters as well as of the Workbook menu (#152).
pub(super) fn plot_menu(
    ui: &mut egui::Ui,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    target_columns: &[NodeId],
) {
    ui.add_enabled_ui(!target_columns.is_empty(), |ui| {
        ui.menu_button(tr!("workbook-plot-menu"), |ui| {
            for (label, plot_type) in [
                (tr!("plot-type-line"), PlotType::Line),
                (tr!("plot-type-scatter"), PlotType::Scatter),
                (tr!("plot-type-line-symbol"), PlotType::LineScatter),
                (tr!("plot-type-bar"), PlotType::Bar),
            ] {
                if ui.button(label).clicked() {
                    actions.push(Action::PlotColumns {
                        sheet,
                        columns: target_columns.to_vec(),
                        plot_type,
                    });
                    ui.close();
                }
            }
        });
    });
}

/// Columns menu of the Workbook bar: Add Column, then Type and Role.
pub(super) fn columns_menu_contents(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    target_columns: &[NodeId],
) {
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| add_column_entry(ui, sheet, actions));
    sections.section(ui, true, |ui| {
        column_type_role_menus(ui, state, actions, target_columns);
    });
    sections.section(ui, true, |ui| {
        if ui
            .add_enabled(
                state.any_formula(target_columns),
                egui::Button::new(tr!("action-remove-formula-keep-data")),
            )
            .on_hover_text(tr!("action-remove-formula-keep-data", "hint"))
            .on_disabled_hover_text(tr!("action-remove-formula-keep-data", "disabled"))
            .clicked()
        {
            actions.push(Action::RemoveColumnsFormulaKeepData {
                columns: target_columns.to_vec(),
            });
            ui.close();
        }
    });
}

/// "Metadata" box of the header: the summary of the metadata of the original
/// file ("23 keys"), read-only. A click opens the window that lists them
/// ([`column_metadata_window`](crate::app::plot_settings::column_metadata_window)).
/// Outside field selection: there is nothing to copy, clear or edit.
fn metadata_cell_widget(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    column: NodeId,
) {
    ui.scope(|ui| {
        ui.set_min_height(HEADER_SUBROW_HEIGHT);
        ui.set_max_height(HEADER_SUBROW_HEIGHT);
        let rect = ui.available_rect_before_wrap();
        header_row_separator(ui, rect);
        let summary = state.file_metadata_summary(column);
        if summary.is_empty() {
            return;
        }
        let response = ui.interact(
            rect,
            ui.id().with(("stokesforge_metadata_cell", column.raw())),
            egui::Sense::click(),
        );
        ui.put(
            rect,
            egui::Label::new(
                egui::RichText::new(icon_label("📋", &summary))
                    .color(crate::theme::gutter_text_color()),
            )
            .selectable(false)
            .truncate(),
        );
        if response.clicked() {
            actions.push(Action::ViewColumnMetadata { column });
        }
        response.on_hover_text(tr!("workbook-metadata-summary", "hint"));
    });
}

/// Thin separator line under a row of the stacked header (letter, or one of the
/// four fields): without it, the four field rows had no fill of their own and
/// blended visually into one another (unlike the data rows, striped by
/// `TableBuilder::striped`).
fn header_row_separator(ui: &egui::Ui, rect: egui::Rect) {
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
}

/// Background of a gutter box of the grid, in three levels — see
/// [`crate::theme::header_letter_row_fill`]/[`crate::theme::
/// header_letter_row_touched_fill`]/[`crate::theme::
/// header_letter_row_selected_fill`]: rest, concerned by the current selection
/// without being so directly, or selected itself (the strongest level wins).
/// Shared by [`column_header_widget`], [`row_number_
/// widget`], [`header_row_label_widget`] and the "#" of [`row_number_header_
/// widget`].
fn gutter_fill(selected: bool, touched: bool) -> egui::Color32 {
    if selected {
        crate::theme::header_letter_row_selected_fill()
    } else if touched {
        crate::theme::header_letter_row_touched_fill()
    } else {
        crate::theme::header_letter_row_fill()
    }
}

/// An editable row of the stacked header, for a single field of
/// [`stokesforge_core::ColumnMeta`] — the same selection/edit scheme as
/// [`cell_widget`] for a data cell: a single click selects (identical
/// highlight), double-click edits; driven by [`AppState::
/// header_editing`]/[`AppState::select_header_field`] rather than by
/// [`AppState::editing`]/[`AppState::select_cell`].
///
/// Shows only the field **value**, empty if the field is — no placeholder
/// repeated per column: the field label ("Long Name", "Units", "Comments",
/// "F(x)=") lives once in the row-number gutter, see
/// [`row_number_header_widget`].
#[allow(clippy::too_many_arguments)]
fn header_field_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    column: NodeId,
    field: HeaderField,
    height: f32,
    column_selected: bool,
) {
    // `actions` is not used to validate the entry — that goes through
    // `AppState::commit_header_edit`, called directly (transient mutation, like
    // `commit_edit`) — but it is used by the context menu below, which pushes
    // actions as everywhere else.
    ui.scope(|ui| {
        ui.set_min_height(height);
        ui.set_max_height(height);
        let rect = ui.available_rect_before_wrap();
        header_row_separator(ui, rect);
        let editing_here = state
            .header_editing()
            .is_some_and(|edit| edit.column == column && edit.field == field);
        if editing_here {
            // The refusal of the Formula row (unknown letter, column that reads itself,
            // modified `#REF`) is decided in `app-state`: the grid paints it and keeps
            // the field open (#321).
            let refusal = state.header_edit_refusal();
            let help = field == HeaderField::Formula && state.show_column_formula_help();
            let Some(edit) = state.header_editing_mut() else {
                return;
            };
            let opening = if edit.typed {
                inplace_edit::Opening::KeepCursor
            } else {
                inplace_edit::Opening::SelectAll
            };
            // The shared in-place edit contract (#224); it consumes Enter, Tab and Escape,
            // which the shortcut block further down must not see again (#38).
            // The "?" of the F(x)= cell (#404, S-T6): only while it is edited, at the
            // right of the field, which gives up the width of the icon so that a
            // narrow column does not clip it. The icon senses only the hover, so it
            // never takes the focus whose loss ends the edit.
            let id = ui.id().with("header_edit");
            let mut paint = |ui: &mut egui::Ui, width: f32| {
                inplace_edit::show(
                    ui,
                    id,
                    egui::TextEdit::singleline(&mut edit.buffer)
                        .frame(egui::Frame::NONE)
                        .desired_width(width),
                    opening,
                    inplace_edit::FocusLoss::Ends,
                    refusal.as_deref(),
                )
            };
            let shown = if help {
                ui.horizontal(|ui| {
                    let width =
                        ui.available_width() - super::column_formula_help::reserved_width(ui);
                    let shown = paint(ui, width.max(0.0));
                    super::column_formula_help::icon(ui, actions);
                    shown
                })
                .inner
            } else {
                paint(ui, f32::INFINITY)
            };
            match shown.end {
                None => {}
                Some(inplace_edit::EditEnd::Cancel) => state.cancel_header_edit(),
                Some(end) => {
                    let refused = state.header_edit_refusal().is_some();
                    // Enter or Tab on a refused text: the field stays open (red frame and
                    // bubble). A click elsewhere closes without writing anything.
                    let write = match inplace_edit::resolve(end, false, refused) {
                        inplace_edit::Resolution::Stay => return,
                        inplace_edit::Resolution::Close { write, .. } => write,
                    };
                    if !write {
                        state.cancel_header_edit();
                        return;
                    }
                    let (tab, shift) = match end {
                        inplace_edit::EditEnd::Tab { shift } => (true, shift),
                        _ => (false, false),
                    };
                    state.commit_header_edit();
                    if tab && !shift && is_last_visible_column(state, sheet, column) {
                        // Tab validates on any header field of the last column (#52): same gesture as
                        // Tab on the last column of a data row — adds a column rather than entering
                        // the data (see `Action::AddColumnAndSelectHeaderField`).
                        actions.push(Action::AddColumnAndSelectHeaderField { sheet, field });
                    } else if tab {
                        state.move_selection_tab(sheet, shift);
                    }
                }
            }
            return;
        }
        let text = header_field_text(state, column, field);
        let id = ui.id().with((column.raw(), field as u8));
        let response = ui.interact(rect, id, egui::Sense::click_and_drag());
        // Highlighted if this exact field is selected, or if the whole column is
        // (selection made from the letter row, see `column_header_widget`): the
        // selection of a column must show on its 5 header rows, not only on the letter
        // row.
        let selected = column_selected || state.is_header_field_selected(sheet, column, field);
        if selected {
            ui.painter().rect_filled(
                rect,
                0.0,
                ui.visuals().selection.bg_fill.linear_multiply(0.35),
            );
        }
        ui.put(
            rect,
            egui::Label::new(egui::RichText::new(text).weak())
                .selectable(false)
                .truncate(),
        );
        // A formula that reads a deleted column shows `#REF`: red frame and bubble on
        // its box (#321). The sentence comes from `app-state`.
        if field == HeaderField::Formula {
            if let Some(message) = state.formula_issue(column) {
                crate::field_state::show_issue(
                    ui,
                    &response,
                    &crate::field_state::FieldIssue::refused(message),
                );
            }
        }
        if response.double_clicked() {
            state.begin_header_edit(column, field);
            return;
        }
        // Same mouse multi-selection logic as for data cells (click/drag/Shift-click)
        // — see the detailed comment of `cell_widget`.
        let modifiers = ui.input(|i| i.modifiers);
        let (command, shift) = (modifiers.command, modifiers.shift);
        let dragging = response.contains_pointer()
            && !press_started_in(ui, rect)
            && !grid_drag_in_progress(ui.ctx())
            && ui.input(|i| i.pointer.primary_down())
            && state.selection(sheet).is_some();
        if response.drag_started() && command {
            state.add_header_field_selection(sheet, column, field);
        } else if response.clicked() && command && !shift {
            state.toggle_header_field_selection(sheet, column, field);
        } else if response.drag_started() || (response.clicked() && !shift) {
            state.select_header_field(sheet, column, field);
        } else if (response.clicked() && shift) || dragging {
            state.extend_header_field_selection(sheet, column, field);
        } else if response.secondary_clicked() && !selected {
            // Same rule as the letter row: a right-click *inside* the selection opens the
            // menu on it without touching it; a right-click outside first reduces it to
            // the targeted box, so that the menu acts on what was just designated and not
            // on a selection forgotten elsewhere.
            state.select_header_field(sheet, column, field);
        }
        // Lazy computation, for the same reason as in `cell_widget`.
        response.context_menu(|ui| selection_context_menu(ui, state, actions, sheet));
    });
}

/// Row-number gutter, stacked-header side: the "#" on the letter row — a click
/// selects the whole table (see [`AppState::select_all`]), the same gesture as a
/// click on a column letter or a row number but for the whole — then, aligned
/// with each of the four field rows of [`column_header_widget`], the **label**
/// of the field concerned ("Long Name", "Units", "Comments", "F(x)=", taken
/// from the old Qt UI), once rather than repeated grayed out in each column.
/// The per-column cells now show only the value (see [`header_field_widget`]).
///
/// Consistent from top to bottom with the rest of the gutter: each row has the
/// same background as the "#" ([`gutter_fill`]) — the four label rows used to
/// stay transparent, only the "#" above breaking the column background. Each
/// label now selects its field for **all** the visible columns of the worksheet
/// (drag/Shift-click extends to several fields), symmetric to
/// [`row_number_widget`] on the data side — see [`header_row_label_widget`].
/// Long labels stay on one line (#125): egui truncates them and shows the full
/// text on hover, without overflowing into a neighboring row.
fn row_number_header_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.scope(|ui| {
            ui.set_min_height(HEADER_SUBROW_HEIGHT);
            ui.set_max_height(HEADER_SUBROW_HEIGHT);
            let selected = state.is_all_selected(sheet);
            let rect = ui.available_rect_before_wrap();
            ui.painter()
                .rect_filled(rect, 0.0, gutter_fill(selected, false));
            header_row_separator(ui, rect);
            let id = ui.id().with("stokesforge_select_all");
            let response = super::cursors::clickable(
                ui.interact(rect, id, egui::Sense::click())
                    .on_hover_text(tr!("action-select-all")),
            );
            ui.put(
                rect,
                egui::Label::new(egui::RichText::new("#").color(crate::theme::gutter_text_color()))
                    .selectable(false),
            );
            if response.clicked() && selected {
                // A second click deselects (#159).
                state.deselect_all(sheet);
            } else if response.clicked() || (response.secondary_clicked() && !selected) {
                // `&& !selected`: a right-click when everything is already selected opens the
                // menu without touching it — same rule as in the other gutter boxes (see
                // `column_header_widget`).
                state.select_all(sheet);
            }
            response.context_menu(|ui| {
                let mut sections = MenuSections::new();
                sections.section(ui, true, |ui| {
                    selection_context_menu(ui, state, actions, sheet);
                });
                if let Some(book) = book_of(state.tree(), sheet) {
                    sections.section(ui, true, |ui| toggle_header_row_menu(ui, book));
                }
            });
        });
        let book = book_of(state.tree(), sheet);
        let rows = header_rows(ui.ctx(), book);
        if rows.x_prime {
            header_row_label_widget(
                ui,
                state,
                actions,
                sheet,
                HeaderField::XPrime,
                HEADER_SUBROW_HEIGHT,
                &tr!("column-x-prime"),
            );
        }
        header_row_label_widget(
            ui,
            state,
            actions,
            sheet,
            HeaderField::LongName,
            HEADER_LONGNAME_HEIGHT,
            &tr!("column-long-name"),
        );
        header_row_label_widget(
            ui,
            state,
            actions,
            sheet,
            HeaderField::Unit,
            HEADER_SUBROW_HEIGHT,
            &tr!("column-units"),
        );
        header_row_label_widget(
            ui,
            state,
            actions,
            sheet,
            HeaderField::Comment,
            HEADER_SUBROW_HEIGHT,
            &tr!("column-comments"),
        );
        header_row_label_widget(
            ui,
            state,
            actions,
            sheet,
            HeaderField::Formula,
            HEADER_SUBROW_HEIGHT,
            &tr!("workbook-header-formula"),
        );
        if rows.metadata {
            ui.scope(|ui| {
                ui.set_min_height(HEADER_SUBROW_HEIGHT);
                ui.set_max_height(HEADER_SUBROW_HEIGHT);
                let rect = ui.available_rect_before_wrap();
                ui.painter()
                    .rect_filled(rect, 0.0, gutter_fill(false, false));
                header_row_separator(ui, rect);
                let response = ui.put(
                    rect,
                    egui::Label::new(
                        egui::RichText::new(tr!("common-metadata"))
                            .color(crate::theme::gutter_text_color()),
                    )
                    .selectable(false)
                    .truncate()
                    .sense(egui::Sense::click()),
                );
                // Mirror of the Window menu: nothing is reachable by right-click alone.
                response.context_menu(|ui| {
                    if ui.button(tr!("workbook-metadata-hide")).clicked() {
                        if let Some(book) = book {
                            set_header_rows(
                                ui.ctx(),
                                book,
                                HeaderRows {
                                    metadata: false,
                                    ..rows
                                },
                            );
                        }
                        ui.close();
                    }
                });
            });
        }
    });
}

/// A row of the gutter, header side (see [`row_number_header_widget`]): the
/// label of a field, at the exact height of the row of
/// [`column_header_widget`] it runs along — and, since whole-field selection
/// exists, a real selection gesture, exact mirror of [`row_number_widget`] for
/// the data rows and of [`column_header_widget`] for the columns: the whole
/// width of the label selects the field for **all** the visible columns of the
/// worksheet (drag/Shift-click extends to several fields), the right-click opens
/// the same edit context menu ([`grid_edit_menu`]).
fn header_row_label_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    field: HeaderField,
    height: f32,
    label: &str,
) {
    ui.scope(|ui| {
        ui.set_min_height(height);
        ui.set_max_height(height);
        let selected = state.is_header_row_selected(sheet, field);
        let rect = ui.available_rect_before_wrap();
        ui.painter()
            .rect_filled(rect, 0.0, gutter_fill(selected, false));
        header_row_separator(ui, rect);
        let id = ui.id().with(("stokesforge_header_row_label", field as u8));
        let response = ui.interact(rect, id, egui::Sense::click_and_drag());
        ui.put(
            rect,
            egui::Label::new(egui::RichText::new(label).color(crate::theme::gutter_text_color()))
                .selectable(false)
                .truncate(),
        );
        let shift = ui.input(|i| i.modifiers.shift);
        if response.drag_started() || (response.clicked() && !shift) {
            state.select_header_row(sheet, field);
        } else if response.clicked() && shift {
            state.extend_header_row_selection(sheet, field);
        } else if response.contains_pointer()
            && ui.input(|i| i.pointer.primary_down())
            && state.selection(sheet).is_some()
        {
            // Same gesture as for data cells and the letter row — extend during the drag,
            // not only on the starting click (see the detailed comment of `cell_widget`).
            state.extend_header_row_selection(sheet, field);
        } else if response.secondary_clicked() && !selected {
            // Right-click outside the selection: reduce it to this field before opening
            // the menu — same rule as in the other three zones.
            state.select_header_row(sheet, field);
        }
        // `grid_menu` **inside** the closure, never before — same reason as in
        // `cell_widget`/`row_number_widget`: computed outside, it would be recomputed
        // on every frame for each of the 4 rows.
        response.context_menu(|ui| selection_context_menu(ui, state, actions, sheet));
    });
}

/// Current value of a header field, as displayed outside editing.
fn header_field_text(state: &AppState, column: NodeId, field: HeaderField) -> String {
    state.header_field_display_text(column, field)
}

/// Keys that the grid claims from egui as long as it has keyboard focus (#38):
/// Tab and the arrows. egui handles them at the **start** of the frame
/// (`Focus::begin_pass`) to move its own focus from widget to widget, unless the
/// focused widget claims them through this filter — reading them afterwards
/// with `key_pressed`, as the grid used to, removes nothing: the selection
/// advanced (or not), and the focus left anyway for the rest of the interface.
/// Escape stays with egui: it gives the focus back.
const GRID_KEYS: egui::EventFilter = egui::EventFilter {
    tab: true,
    horizontal_arrows: true,
    vertical_arrows: true,
    escape: false,
};

/// egui memory key of the last focus anchor that a grid claimed
/// ([`claim_grid_focus`]) — a way to recognize, from the rest of the
/// application, that keyboard focus is on the grid and not on a text field.
fn grid_focus_owner_key() -> egui::Id {
    egui::Id::new("stokesforge_grid_focus_owner")
}

/// Identifier of the grid's focus anchor widget for `sheet` — one per worksheet
/// and per `Ui`, so that the floating workbook and its detached copy do not
/// fight over the same identifier.
fn grid_focus_id(ui: &egui::Ui, sheet: NodeId) -> egui::Id {
    ui.id().with(("grid_focus", sheet.raw()))
}

/// The grid takes keyboard focus: its selection now receives Tab and the arrows
/// (see [`GRID_KEYS`]).
fn claim_grid_focus(ctx: &egui::Context, anchor: egui::Id) {
    ctx.memory_mut(|memory| memory.request_focus(anchor));
    ctx.data_mut(|data| data.insert_temp(grid_focus_owner_key(), anchor));
}

/// True if a widget **other than a grid** has keyboard focus — typically a text
/// field, which then keeps Enter, Delete, Tab… Replaces the
/// `focused().is_some()` test wherever the grid can legitimately hold the focus
/// (#38): without it, a focused grid would make its neighbors pass for fields
/// being typed in.
pub(super) fn keyboard_focus_outside_grid(ctx: &egui::Context) -> bool {
    let Some(focused) = ctx.memory(|memory| memory.focused()) else {
        return false;
    };
    let owner: Option<egui::Id> = ctx.data(|data| data.get_temp(grid_focus_owner_key()));
    owner != Some(focused)
}

/// The grid, in **virtualized** rows: only the visible rows are built, without
/// which a worksheet of a million rows would block every frame. Scoped to
/// `sheet` — see [`book_contents`](crate::app::workbook::book_contents), its
/// only caller.
///
/// Stacked header (letter A/B/C, long name, unit, comment, formula — see
/// [`column_header_widget`]), mouse selection of cells/columns (see
/// [`cell_widget`]), and Ctrl+C/X/V/Delete/Backspace shortcuts scoped to this
/// grid: they are consumed only if no cell and no header field is being edited,
/// **nor the import window open**, so as never to steal the native copy/paste of
/// a `TextEdit` being typed in — the import window is a floating modal dialog
/// painted over this grid, which is always present underneath as long as it is
/// open (see the guard further down, in the function body).
pub(super) fn grid_contents(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    pending_vertical_offset: Option<f32>,
) -> Option<egui::scroll_area::ScrollAreaOutput<()>> {
    let columns = state.visible_columns(sheet);
    let row_count = state.row_count(sheet);
    if columns.is_empty() {
        let empty = ui.scope(|ui| {
            ui.set_min_height(ui.available_height());
            empty_state(ui, tr!("workbook-grid-no-columns"));
        });
        empty
            .response
            .context_menu(|ui| add_column_entry(ui, sheet, actions));
        return None;
    }
    let (grid_rect, focus_anchor) = prepare_grid(ui, sheet);
    let was_editing = state.editing().is_some() || state.header_editing().is_some();
    let mut extents: Vec<ColumnExtent> = Vec::new();
    let mut row_extents: Vec<RowExtent> = Vec::new();
    let header_rows = header_rows(ui.ctx(), book_of(state.tree(), sheet));
    let header_height = stacked_header_height(header_rows);
    // #325: the width of a column is a fact of the **column** (`NodeId`, in the
    // actor and the `.stf`), not of its position. egui_extras keeps its own by
    // position and discards them as soon as the number of columns changes
    // (`TableState::load`): so every frame we give it back the width of each
    // column, and we make it forget its state when the ordered list of columns is
    // no longer the previous frame's (add, delete, undo, move, another worksheet,
    // another project). Never a variable `id_salt`: the identifier of the vertical
    // scroll area derives from it, and the scrolling would be lost.
    let column_ids: Vec<NodeId> = columns.iter().map(|(id, _, _)| *id).collect();
    // #330: the stored widths of the visible columns enter the key, otherwise
    // putting a column back to the default (its key leaves the table) would change
    // nothing on screen — `TableColumn::initial` is read only after a `reset`.
    // Their value, not just their state: a width set by hand redoes the layout with
    // the value sent.
    let widths: Vec<Option<u32>> = column_ids
        .iter()
        .map(|id| state.column_width(*id).map(f32::to_bits))
        .collect();
    let layout_key = (state.snapshot().epoch, column_ids.clone(), widths);
    let layout_memory = ui.id().with("stokesforge_grid_layout");
    // #337: the first frame of an `egui::Window` is a measuring pass
    // (`Ui::is_sizing_pass`) during which egui_extras shrinks each column to the
    // width its content used (50 instead of 54), then keeps that value. The table
    // therefore forgets its state on the frame that follows a measuring pass, and
    // `TableColumn::initial` gives back the stored widths.
    let sizing_memory = ui.id().with("stokesforge_grid_sizing_pass");
    let after_sizing_pass = ui.data(|data| data.get_temp::<bool>(sizing_memory)) == Some(true);
    ui.data_mut(|data| data.insert_temp(sizing_memory, ui.is_sizing_pass()));
    let layout_changed = after_sizing_pass
        || ui.data(|data| data.get_temp::<(u64, Vec<NodeId>, Vec<Option<u32>>)>(layout_memory))
            != Some(layout_key.clone());
    if layout_changed {
        ui.data_mut(|data| data.insert_temp(layout_memory, layout_key));
    }
    // `TableBuilder::reset` puts **all** the columns back to their initial width,
    // gutter included: the gutter gives back the one it had in the previous frame
    // (temporary memory, per worksheet), otherwise every drag of a column would
    // bring it back to 90.
    let gutter_width = ui
        .ctx()
        .data(|data| data.get_temp::<f32>(gutter_width_id(sheet)))
        .unwrap_or(ROW_NUMBER_COLUMN_WIDTH);
    let mut table = TableBuilder::new(ui)
        .id_salt(GRID_TABLE_SALT)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(TableColumn::initial(gutter_width).at_least(MIN_ROW_NUMBER_COLUMN_WIDTH));
    for id in &column_ids {
        let width = state.column_width(*id).unwrap_or(DEFAULT_COLUMN_WIDTH);
        table = table.column(TableColumn::initial(width).at_least(MIN_COLUMN_WIDTH));
    }
    if layout_changed {
        table.reset();
    }
    let mut table = table
        .min_scrolled_height(0.0)
        // The table takes the space left by the tab panel, even when its worksheet
        // holds few rows: the grid thus keeps its height and its vertical scrolling.
        .auto_shrink([false, false])
        // Its vertical bar is not drawn here: in the horizontal scroll area, it would
        // follow the right edge of the table and leave the view on a wide grid (B10).
        // Scrolling (wheel, drag) stays active; the visible bar is carried by
        // [`show_grid_vertical_bar`], stuck to the right edge of the window.
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    if let Some(offset) = pending_vertical_offset {
        table = table.vertical_scroll_offset(offset);
    }
    let table_output = table
        .header(header_height, |header| {
            grid_table_header(header, state, actions, sheet, &columns, &mut extents);
        })
        .body(|body| {
            grid_table_body(
                body,
                state,
                actions,
                sheet,
                &columns,
                row_count,
                &mut row_extents,
            );
        });

    // A width is pushed only at the **end of a resize done by hand**: pushing the
    // rendered width on every frame would mark the project modified on every
    // opening (#325).
    for (column, width) in finished_column_resizes(ui, &column_ids, &extents) {
        actions.push(Action::SetColumnWidth { column, width });
    }

    let last_column_right = extents
        .last()
        .map_or(grid_rect.left(), |&(_, _, right)| right);
    grid_blank_column_target(ui, actions, sheet, grid_rect, last_column_right);
    grid_column_drag_source(ui, state, sheet, grid_rect, &extents);
    grid_row_drag_source(
        ui,
        state,
        sheet,
        (grid_rect.right(), grid_rect.top() + header_height),
        grid_rect.bottom(),
        &extents,
        &row_extents,
    );
    grid_keyboard(
        ui,
        state,
        actions,
        sheet,
        grid_rect,
        focus_anchor,
        was_editing,
    );
    Some(table_output)
}

/// Where the grid keeps, in egui's temporary memory, the rendered width of the
/// row-number gutter of `sheet` (#330).
fn gutter_width_id(sheet: NodeId) -> egui::Id {
    egui::Id::new(("stokesforge_gutter_width", sheet.raw()))
}

/// Forgets the gutter width of `sheet`: the next layout brings it back to its
/// default width (reset of the whole worksheet, #330).
pub(super) fn forget_gutter_width(ctx: &egui::Context, sheet: NodeId) {
    ctx.data_mut(|data| data.remove::<f32>(gutter_width_id(sheet)));
}

/// Salt of the grid table: fixed, so without effect on the identifier of its
/// vertical scroll area. `finished_column_resizes` rebuilds from it the
/// identifiers of the resize handles.
const GRID_TABLE_SALT: &str = "stokesforge_grid_table";

/// The columns whose manual resize has just ended (button released this frame),
/// with their rendered width: `(column, width)`. The handle of data column `k`
/// is the one of rank `k + 1` of the table (the row-number gutter has rank 0);
/// its identifier is the one egui_extras gives it (`table.rs`,
/// `state_id.with("resize_column")`).
fn finished_column_resizes(
    ui: &egui::Ui,
    column_ids: &[NodeId],
    extents: &[ColumnExtent],
) -> Vec<(NodeId, f32)> {
    let state_id = ui.id().with(egui::IdSalt::new(GRID_TABLE_SALT));
    column_ids
        .iter()
        .enumerate()
        .filter_map(|(index, id)| {
            let handle = state_id.with("resize_column").with(index + 1);
            let finished = ui
                .ctx()
                .read_response(handle)
                .is_some_and(|response| response.drag_stopped());
            let (_, left, right) = extents.iter().find(|(extent_id, _, _)| extent_id == id)?;
            finished.then_some((*id, right - left))
        })
        .collect()
}

/// Prepares the spacings and the focusable anchor, without creating a new Ui.
fn prepare_grid(ui: &mut egui::Ui, sheet: NodeId) -> (egui::Rect, egui::Id) {
    // Without this, egui's default vertical spacing between the table rows
    // (`Spacing::item_spacing.y`, which `TableBuilder` uses internally to separate
    // the lines) leaves a thin white line between each — the gutter's background
    // color then does not really fill the cell, it floats in it (row numbers in
    // particular, noticeable to the eye because painted as a solid fill rather than
    // plain text). `item_spacing.x` stays intact: it carries the spacing between
    // columns, on which the resize handles depend.
    ui.spacing_mut().item_spacing.y = 0.0;
    // Captured *before* the table, whose `.auto_shrink([false, false])` occupies
    // exactly all this available space: serves as the reference for the column
    // drag grip area further down ([`column_grip_bands`]).
    let grid_rect = ui.available_rect_before_wrap();
    // Focus anchor of the grid's keyboard focus (#38): a focusable widget
    // **only while it has the focus** (otherwise a Tab arriving while nothing
    // has focus would make the focus enter it by itself). It carries the event filter
    // [`GRID_KEYS`] — see the doc of this constant. The grid claims it through
    // [`claim_grid_focus`] after a press in its rectangle or the end of an edit
    // (further down); egui removes it by itself on a press elsewhere or if the grid
    // stops being painted. The filter is taken into account only in the frame that
    // follows the focus grab (`set_focus_lock_filter` requires
    // `had_focus_last_frame`): a Tab in the very frame of the click is not a
    // realistic keystroke.
    let focus_anchor = grid_focus_id(ui, sheet);
    let anchor_focused = ui.memory(|memory| memory.has_focus(focus_anchor));
    ui.interact(
        grid_rect,
        focus_anchor,
        if anchor_focused {
            egui::Sense::focusable_noninteractive()
        } else {
            egui::Sense::hover()
        },
    );
    if anchor_focused {
        ui.memory_mut(|memory| memory.set_focus_lock_filter(focus_anchor, GRID_KEYS));
    }
    (grid_rect, focus_anchor)
}

/// The header lays out the columns and records the bound of the free area.
fn grid_table_header(
    mut header: egui_extras::TableRow<'_, '_>,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    columns: &[(NodeId, String, ColumnType)],
    extents: &mut Vec<ColumnExtent>,
) {
    header.col(|ui| {
        let width = ui.max_rect().width();
        // Not during a measuring pass (#337): its width is not the gutter's.
        if width > 0.0 && !ui.is_sizing_pass() {
            ui.ctx()
                .data_mut(|data| data.insert_temp(gutter_width_id(sheet), width));
        }
        row_number_header_widget(ui, state, actions, sheet);
    });
    for (index, (id, _, _)) in columns.iter().enumerate() {
        header.col(|ui| {
            let rect = ui.max_rect();
            extents.push((*id, rect.left(), rect.right()));
            column_header_widget(ui, state, actions, sheet, *id, index);
        });
    }
}

/// The body keeps the row virtualization in the same place in the table.
fn grid_table_body(
    body: egui_extras::TableBody<'_>,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    columns: &[(NodeId, String, ColumnType)],
    row_count: usize,
    row_extents: &mut Vec<RowExtent>,
) {
    body.rows(ROW_HEIGHT, row_count, |mut row| {
        let index = row.index();
        row.col(|ui| {
            let rect = ui.max_rect();
            row_extents.push((index, rect.top(), rect.bottom()));
            row_number_widget(ui, state, actions, sheet, index);
        });
        for (id, _, _) in columns {
            row.col(|ui| {
                cell_widget(ui, state, actions, sheet, *id, index);
            });
        }
    });
}

fn grid_blank_column_target(
    ui: &mut egui::Ui,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    grid_rect: egui::Rect,
    last_column_right: f32,
) {
    // The free portion after the last column is a target distinct from the cells
    // and their menus. The bound comes from the header actually laid out
    // (including resized widths), not from the default widths.
    if last_column_right < grid_rect.right() {
        let rect = egui::Rect::from_min_max(
            egui::pos2(last_column_right, grid_rect.top()),
            grid_rect.right_bottom(),
        );
        let response = ui.interact(
            rect,
            ui.id().with(("add_column_blank", sheet)),
            egui::Sense::click(),
        );
        response.context_menu(|ui| add_column_entry(ui, sheet, actions));
    }
}

/// Drag a selection of **whole columns** to a graph (#160), by grabbing the
/// outer edge of a block of selected columns, like Excel's selection: "hand"
/// cursor on hover, "closed hand" during the drag. The bands
/// ([`column_grip_bands`]) are placed **after** the table, so above the cells
/// and the resize handles: they exist only on the edges of the selected
/// columns, outside the letter row, so that the selection-extending drag of the
/// cells and the resizing keep all the rest of the surface.
///
/// The payload ([`AppState::column_drag_payload`]) carries all the selected
/// columns, in the worksheet's order. Escape cancels the drag; a marker (number
/// of columns) follows the pointer.
fn grid_column_drag_source(
    ui: &mut egui::Ui,
    state: &AppState,
    sheet: NodeId,
    grid_rect: egui::Rect,
    extents: &[ColumnExtent],
) {
    let ctx = ui.ctx().clone();
    let dragging = egui::DragAndDrop::payload::<ColumnDragPayload>(&ctx)
        .filter(|payload| payload.sheet == sheet);
    if let Some(payload) = &dragging {
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            egui::DragAndDrop::clear_payload(&ctx);
        } else {
            super::cursors::grabbing(&ctx);
            column_drag_marker(&ctx, sheet, payload.columns.len());
        }
        return;
    }
    let Some(payload) = state.column_drag_payload(sheet) else {
        return;
    };
    // Under the letter row: its column edges resize.
    let top = grid_rect.top() + HEADER_SUBROW_HEIGHT;
    for (index, band) in column_grip_bands(extents, &payload.columns, top, grid_rect.bottom())
        .into_iter()
        .enumerate()
    {
        let response = ui.interact(
            band,
            ui.id().with(("column_grip", sheet, index)),
            egui::Sense::drag(),
        );
        if response.hovered() {
            super::cursors::grab(&ctx);
        }
        if response.drag_started() {
            response.dnd_set_drag_payload(payload.clone());
        }
    }
}

/// Drag a selection of **whole rows** to a graph (#166), by the top or bottom
/// edge of a block of selected rows — same cursors, same marker and same Escape
/// as for columns ([`grid_column_drag_source`]). Offered only when the worksheet
/// has an x' row (see [`AppState::row_drag_payload`]): without x', a row has no
/// abscissa. `body_top`/`right` bound the table (under the header); the
/// row-number gutter, to the left of the first column, stays free for selecting.
fn grid_row_drag_source(
    ui: &mut egui::Ui,
    state: &AppState,
    sheet: NodeId,
    (right, body_top): (f32, f32),
    bottom: f32,
    columns: &[ColumnExtent],
    rows: &[RowExtent],
) {
    let ctx = ui.ctx().clone();
    let dragging =
        egui::DragAndDrop::payload::<RowDragPayload>(&ctx).filter(|payload| payload.sheet == sheet);
    if let Some(payload) = &dragging {
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            egui::DragAndDrop::clear_payload(&ctx);
        } else {
            super::cursors::grabbing(&ctx);
            row_drag_marker(&ctx, sheet, payload.rows.len());
        }
        return;
    }
    let Some(payload) = state.row_drag_payload(sheet) else {
        return;
    };
    let Some(left) = columns.first().map(|column| column.1) else {
        return;
    };
    for (index, band) in row_grip_bands(rows, &payload.rows, left, right, body_top, bottom)
        .into_iter()
        .enumerate()
    {
        let response = ui.interact(
            band,
            ui.id().with(("row_grip", sheet, index)),
            egui::Sense::drag(),
        );
        if response.hovered() {
            super::cursors::grab(&ctx);
        }
        if response.drag_started() {
            response.dnd_set_drag_payload(payload.clone());
        }
    }
}

/// Marker that follows the pointer during the row drag.
fn row_drag_marker(ctx: &egui::Context, sheet: NodeId, count: usize) {
    let Some(pointer) = ctx.pointer_latest_pos() else {
        return;
    };
    egui::Area::new(egui::Id::new(("row_drag_marker", sheet)))
        .order(egui::Order::Tooltip)
        .fixed_pos(pointer + egui::vec2(14.0, 14.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(icons::icon_image(
                        icons::Icon::PlotLine,
                        icons::IconRole::Neutral,
                    ));
                    ui.label(tr!("grid-drag-rows", count = count));
                });
            });
        });
}

/// Marker that follows the pointer during the column drag: an icon from the
/// icon component and the number of columns carried.
fn column_drag_marker(ctx: &egui::Context, sheet: NodeId, count: usize) {
    let Some(pointer) = ctx.pointer_latest_pos() else {
        return;
    };
    egui::Area::new(egui::Id::new(("column_drag_marker", sheet)))
        .order(egui::Order::Tooltip)
        .fixed_pos(pointer + egui::vec2(14.0, 14.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(icons::icon_image(
                        icons::Icon::PlotLine,
                        icons::IconRole::Neutral,
                    ));
                    ui.label(tr!("grid-drag-columns", count = count));
                });
            });
        });
}

/// After painting and the drop: focus recovery, then typing, navigation and editing.
fn grid_keyboard(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    grid_rect: egui::Rect,
    focus_anchor: egui::Id,
    was_editing: bool,
) {
    // Taking keyboard focus (#38), **before** the shortcut block, so that the key
    // following a click is already the grid's. Two cases: a press in the grid's
    // rectangle (click, drag, context menu), unless it opens or continues an
    // edit, whose text field claims the focus itself; and the end of an edit that
    // gave the focus back to nobody (Enter, Tab — see [`cell_widget`] —, Escape):
    // without it, the next Tab would go off into the rest of the interface. A focus
    // held by another widget (click in another field) is never taken back.
    let editing_now = state.editing().is_some() || state.header_editing().is_some();
    // Press **or** release: egui gives the anchor's focus back on click
    // (`SurrenderFocusOn::Clicks`, its default setting), so on release, because
    // the anchor is not under the cursor; the press alone is not enough for a
    // simple click.
    let pressed_in_grid = ui
        .input(|input| input.pointer.any_pressed() || input.pointer.any_released())
        && ui.rect_contains_pointer(grid_rect);
    let edit_just_ended =
        was_editing && !editing_now && ui.ctx().memory(|memory| memory.focused()).is_none();
    if !editing_now && (pressed_in_grid || edit_just_ended) {
        claim_grid_focus(ui.ctx(), focus_anchor);
    }
    let focus_free = ui
        .ctx()
        .memory(|memory| memory.focused())
        .is_none_or(|focused| focused == focus_anchor);

    // Keyboard shortcuts of the grid — see the head doc of this function for the
    // edit guard.
    //
    // `state.import_state().is_none()` is the third condition of this guard, added
    // with the redesign of the import window: `consume_key` below acts on the
    // `InputState` of the whole `Context`, not on that of a `Ui` — so not only on
    // this grid. As long as the import window (a floating `egui::Window`, painted
    // *after* the floating content windows — see `StokesForgeApp::ui`) is open,
    // this grid keeps being painted underneath on every frame; without this
    // condition, Backspace/Delete typed in a text field of the import window were
    // removed from here before that field could even see them: no more character
    // deletion possible without going through a selection (which replaces the
    // selected text through the insertion of `Event::Text`, never consumed here) —
    // that was the known bug of the import module's text field, fixed here rather
    // than in the window itself because it was born here.
    //
    // #17: same theft, this time in the graph properties dialog — the per-window
    // list of conditions could only miss the next one. `focus_free` (no widget has
    // keyboard focus, or it is the anchor of **this** grid — see `GRID_KEYS`, #38;
    // `egui_wants_keyboard_input` did not tell it from a text field) covers this
    // dialog, the import and any future dialog at once; `import_state` stays for
    // the case of an open import with no focused field.
    //
    // #406: same theft for Esc, typing, arrows and Ctrl+A under **any** modal
    // window. egui drops the focus on Esc at the start of the pass, so
    // `focus_free` is always true on that frame and the grid, painted before the
    // dialogs, took the Esc that closes them. `dialogs::modal_shown` (a modal
    // painted this frame or the previous one) closes this hole for all of them;
    // the one-frame lag after a close is intended.
    if state.editing().is_none()
        && state.header_editing().is_none()
        && state.import_state().is_none()
        && focus_free
        && !super::dialogs::modal_shown(ui.ctx())
    {
        // #143: F2 belongs only to the focused grid. The other shortcuts keep their
        // fallback without focus, but F2 must stay available for the managers. Reuse
        // the double-click editor.
        if ui.ctx().memory(|memory| memory.focused()) == Some(focus_anchor)
            && ui.input_mut(|input| input.consume_shortcut(&SHORTCUT_EDIT_CELL))
        {
            let target = state.selection(sheet).and_then(|selection| {
                let rank = selection.full_rank_range()?.0;
                let column = *selection.columns(state.tree(), sheet).first()?;
                Some((column, rank))
            });
            if let Some((column, rank)) = target {
                if rank < 0 {
                    state.begin_header_edit(column, HeaderField::from_rank(rank));
                } else {
                    state.begin_edit(column, rank as usize);
                }
            }
            return;
        }
        // Ctrl+A: same effect as the corner box (#159). Escape lifts the selection,
        // like a second click on the corner box.
        if ui.input_mut(|input| input.consume_shortcut(&SHORTCUT_SELECT_ALL)) {
            actions.push(Action::SelectAll { sheet });
        } else if ui.input_mut(|input| input.consume_shortcut(&super::shortcuts::KEY_ESCAPE)) {
            state.deselect_all(sheet);
        }
        start_grid_typing(ui, state, sheet);
        navigate_grid_selection(ui, state, actions, sheet);
        grid_clipboard_shortcuts(ui, state, actions, sheet);
    }
}

fn start_grid_typing(ui: &egui::Ui, state: &mut AppState, sheet: NodeId) {
    // Typing on a selected cell or header field (single click, not yet
    // being edited) starts the edit by replacing all its content with what is
    // typed — standard spreadsheet behavior, the same for both. Triggers only for
    // a selection reduced to a single cell ([`Selection::single_cell`]) on the
    // data side: typing on a range must never overwrite several cells at once — a
    // header field, for its part, is never a range. `egui::Event::Text` carries
    // only printable text — never a control or navigation key, which appears only
    // in `events` as an `Event::Key` — so no extra filtering to do here.
    let typed: String = ui.input(|input| {
        input
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect()
    });
    if !typed.is_empty() {
        if let Some((column, row)) = state.selection(sheet).and_then(Selection::single_cell) {
            state.begin_edit(column, row);
            if let Some(edit) = state.editing_mut() {
                edit.buffer = typed;
                edit.typed = true;
            }
        } else if let Some((column, field)) = state.selected_header_field(sheet) {
            state.begin_header_edit(column, field);
            if let Some(edit) = state.header_editing_mut() {
                edit.buffer = typed;
                edit.typed = true;
            }
        }
    }
}

fn navigate_grid_selection(
    ui: &egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
) {
    // Keyboard move of an already selected cell, with no edit in progress (during
    // editing, these same keys are handled by `cell_widget`, which validates *then*
    // moves: see its doc). Arrow keys + Shift to extend the rectangle; Tab/
    // Shift+Tab with wrap at the edges; Enter/Shift+Enter as non-editing
    // equivalents of Down/Up. No effect if the current selection is not a
    // rectangle of cells (whole column or header field) — see `Selection::cursor`.
    let shift = ui.input(|i| i.modifiers.shift);
    let (up, down, left, right, tab, enter) = ui.input(|input| {
        (
            input.key_pressed(egui::Key::ArrowUp),
            input.key_pressed(egui::Key::ArrowDown),
            input.key_pressed(egui::Key::ArrowLeft),
            input.key_pressed(egui::Key::ArrowRight),
            input.key_pressed(egui::Key::Tab),
            input.key_pressed(egui::Key::Enter),
        )
    });
    if up {
        state.move_selection(sheet, MoveDirection::Up, shift);
    }
    if down {
        state.move_selection(sheet, MoveDirection::Down, shift);
    }
    if left {
        state.move_selection(sheet, MoveDirection::Left, shift);
    }
    if right {
        state.move_selection(sheet, MoveDirection::Right, shift);
    }
    if tab && !shift {
        // Tab on the last column of a data row, or on any header field of the last
        // column: same gesture — adds a column rather than returning to the start of
        // the next row or entering the data, then immediately selects the box that
        // will correspond to it (see `Action::AddColumnAndSelect`/
        // `Action::AddColumnAndSelectHeaderField`, which can place it only once the
        // column is really created, unlike a row). Applied only to Formula before #52
        // (Tab used to go column by column first for the same field); decision lifted
        // by the user.
        let cursor = state.selection(sheet).and_then(Selection::grid_cursor);
        let needs_new_column = match cursor {
            Some(GridCursor::Data(column, _) | GridCursor::Header(column, _)) => {
                is_last_visible_column(state, sheet, column)
            }
            None => false,
        };
        if needs_new_column {
            match cursor {
                Some(GridCursor::Data(_, row)) => {
                    actions.push(Action::AddColumnAndSelect { sheet, row });
                }
                Some(GridCursor::Header(_, field)) => {
                    actions.push(Action::AddColumnAndSelectHeaderField { sheet, field });
                }
                None => {}
            }
        } else {
            state.move_selection_tab(sheet, false);
        }
    } else if tab {
        state.move_selection_tab(sheet, true);
    }
    if enter && shift {
        state.move_selection(sheet, MoveDirection::Up, false);
    } else if enter {
        // Enter on the last row of the worksheet, cell already empty and never edited
        // in this keystroke: `move_selection` alone would do nothing (`Down` is bounded
        // to `row_count - 1`, see its doc). Same gesture as `CommitCellAndAdvance` on a
        // cell that was just filled — adds the row, then immediately selects the cell
        // that will correspond to it (it will be created before the next render, see
        // `cell_widget`).
        let data_cursor = state
            .selection(sheet)
            .and_then(Selection::grid_cursor)
            .and_then(|cursor| match cursor {
                GridCursor::Data(column, row) => Some((column, row)),
                GridCursor::Header(..) => None,
            });
        let at_last_row = data_cursor.is_some_and(|(_, row)| row + 1 >= state.row_count(sheet));
        if at_last_row {
            if let Some((column, row)) = data_cursor {
                actions.push(Action::InsertRows {
                    sheet,
                    at: row + 1,
                    count: 1,
                });
                state.select_cell(sheet, column, row + 1);
            }
        } else {
            state.move_selection(sheet, MoveDirection::Down, false);
        }
    }
}

fn grid_clipboard_shortcuts(
    ui: &egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
) {
    // Ctrl+C/X/V **never** appear as an ordinary `egui::Event::Key` (so never via
    // `consume_key`/`consume_shortcut`): `egui-winit` intercepts them itself when
    // translating the window events and pushes instead an `Event::Copy`/
    // `Event::Cut`/`Event::Paste(String)` (the latter already carries the content
    // of the system clipboard, read by `egui-winit` through `arboard` internally —
    // no need to reread it ourselves). This also holds for Ctrl+Shift+C/Ctrl+Shift+V:
    // `egui-winit::is_copy_command`/`is_paste_command` test only
    // `modifiers.command`, never `shift` (see `egui-winit/src/lib.rs`), so Shift
    // held or not produces the **same** `Event::Copy`/`Event::Paste` — never an
    // `Event::Key` to capture with a separate shortcut (an earlier attempt wrongly
    // assumed so: `consume_shortcut` on a Ctrl+Shift+C/V shortcut never matched).
    // The two are therefore told apart by reading `input.modifiers.shift` at the
    // time of the event — always up to date at that point, `ModifiersChanged`
    // coming before the keystroke itself. Backspace/Delete, for their part, have
    // no modifier and stay ordinary `Event::Key`s, captured via `consume_key` as
    // before. Delete is the only one of the two that does not dispatch a fixed
    // action: `AppState::consume_delete_key` decides whether it is a clear or a
    // deletion depending on whether it is the second consecutive press on the same
    // selection (see its doc).
    let (mut copy, mut cut, mut paste_text, mut copy_full_columns, mut paste_absolute) =
        (false, false, None, false, false);
    ui.input(|input| {
        let shift = input.modifiers.shift;
        for event in &input.events {
            match event {
                egui::Event::Copy if shift => copy_full_columns = true,
                egui::Event::Copy => copy = true,
                egui::Event::Cut => cut = true,
                egui::Event::Paste(_) if shift => paste_absolute = true,
                egui::Event::Paste(text) => paste_text = Some(text.clone()),
                _ => {}
            }
        }
    });
    let (delete, backspace) = ui.input_mut(|input| {
        (
            input.consume_shortcut(&super::shortcuts::KEY_DELETE),
            input.consume_shortcut(&super::shortcuts::KEY_BACKSPACE),
        )
    });
    // The whole behavior matrix (a single copied box becomes plain text, a
    // rectangle becomes relative, Ctrl+Shift+C promotes the touched columns in
    // full, a paste shifts onto the anchor or replays its original ranks…) lives
    // in `AppState` — see its head doc, copy/paste section. This layer only
    // translates keyboard/mouse into calls, the same way whatever the kind of
    // current selection: no separate branch any more for a selected header field,
    // `AppState::copy_selection`/`AppState::paste`/`Action::ClearSelectedCells`/
    // `Action::DeleteSelectedCells` all already tell header and data apart by
    // themselves.
    if copy_full_columns {
        if let Some(text) = state.copy_full_columns(sheet) {
            ui.ctx().copy_text(text);
        }
    } else if copy || cut {
        match state.copy_selection(sheet) {
            Some(text) => ui.ctx().copy_text(text),
            // Ambiguous copy (#159): Cut clears nothing, it copied nothing.
            None => cut = false,
        }
    }
    if cut || backspace {
        // Another grid mutation slips in between two presses on Delete: the second
        // must start again at the first step (clear), not delete by surprise — see
        // the doc of `AppState::consume_delete_key`.
        state.forget_delete_repeat(sheet);
        actions.push(Action::ClearSelectedCells { sheet });
    }
    if delete {
        if let Some(action) = state.consume_delete_key(sheet) {
            actions.push(action);
        }
    }
    if paste_absolute {
        state.forget_delete_repeat(sheet);
        actions.push(Action::PasteAbsolute { sheet });
    } else if let Some(text) = paste_text {
        state.forget_delete_repeat(sheet);
        actions.push(Action::Paste { sheet, text });
    }
}

/// Box of the row-number gutter, in the table **body** — exact mirror of the
/// column letter row ([`column_header_widget`]) but for rows: the whole width
/// of the box selects **the whole row** (drag/Shift-click extends to several
/// rows, same multi-selection logic as [`cell_widget`]/
/// [`column_header_widget`]) and opens the same edit context menu
/// ([`grid_edit_menu`]).
///
/// Background in three levels like the letter row ([`gutter_fill`]) — rest,
/// touched by a data selection without being selected itself
/// ([`AppState::row_touched_by_selection`]), or directly selected — so that the
/// gutter is recognized as a control area at first glance, and the row of a
/// selected cell is noticed even without having clicked the gutter itself.
/// Consistent with the choice already made for `column_header_widget` and for
/// the "#" of [`row_number_header_widget`] just above.
fn row_number_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    row: usize,
) {
    let selected = state.is_row_selected(sheet, row);
    let touched = state.row_touched_by_selection(sheet, row);
    let rect = ui.available_rect_before_wrap();
    ui.painter()
        .rect_filled(rect, 0.0, gutter_fill(selected, touched));
    let id = ui.id().with(("stokesforge_row_number", row));
    let response = super::cursors::clickable(ui.interact(rect, id, egui::Sense::click_and_drag()));
    ui.put(
        rect,
        egui::Label::new(
            egui::RichText::new(format!("{}", row + 1)).color(crate::theme::gutter_text_color()),
        )
        .selectable(false),
    );
    let modifiers = ui.input(|i| i.modifiers);
    if response.clicked() || response.drag_started() {
        // Ctrl+click toggles the row, Shift+click takes the anchor → row range,
        // Ctrl+Shift+click adds it to the selection (#159) — like the letters.
        match (modifiers.command, modifiers.shift) {
            (true, true) => state.extend_row_selection(sheet, row),
            (true, false) => state.toggle_row_selection(sheet, row),
            (false, true) => state.extend_row_selection_replacing(sheet, row),
            (false, false) => state.select_row(sheet, row),
        }
    } else if response.contains_pointer()
        && !press_started_in(ui, rect)
        && !grid_drag_in_progress(ui.ctx())
        && ui.input(|i| i.pointer.primary_down())
        && state.selection(sheet).is_some()
    {
        // Same gesture as for data cells and the letter row — extend during the drag,
        // not only on the starting click (see the detailed comment of `cell_widget`).
        state.extend_row_selection(sheet, row);
    } else if response.secondary_clicked() && !selected {
        // Right-click outside the selection: reduce it to this row before opening the
        // menu — same rule as for a cell or a header field (see
        // `cell_widget`/`header_field_widget`).
        state.select_row(sheet, row);
    }
    // `grid_menu` **inside** the closure, never before — same reason as in
    // `cell_widget`: computed outside, it would be recomputed on every frame for
    // each visible row.
    response.context_menu(|ui| selection_context_menu(ui, state, actions, sheet));
}

/// Paints a cell and collects the actions it produces.
///
/// Only one cell at a time carries a real buffer (the one of
/// [`AppState::editing`]); all the others are painted as a `Label` highlighted
/// if selected, never grabbing keyboard focus — which keeps the grid's memory
/// cost independent of the number of rows, and leaves the single click to
/// selection rather than to editing.
///
/// - Single click (or drag starting here): selects this cell, anchor of a
///   future rectangle.
/// - Shift-click, or hover while a drag is in progress elsewhere in the grid:
///   extends the selection to here.
/// - Double-click: opens the edit.
///
/// The content is sent to the model only on **loss of focus** of the edit —
/// which the Enter key also causes — never on each keystroke: otherwise the
/// history would count one entry per character. Losing focus through Enter
/// pushes [`Action::CommitCellAndAdvance`] rather than [`Action::
/// CommitCell`]: it is the one that adds a row when we were on the last one of
/// the column.
fn cell_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    column: NodeId,
    row: usize,
) {
    // The identifier is anchored on (column, row) and not on the position on
    // screen: with virtualized rows, scrolling would otherwise move the focus from
    // one cell to another.
    ui.push_id((column.raw(), row), |ui| {
        let editing_here = state
            .editing()
            .is_some_and(|edit| edit.column == column && edit.row == row);

        if editing_here {
            edit_cell_widget(ui, state, actions, sheet, column, row);
            return;
        }
        display_cell_widget(ui, state, actions, sheet, column, row);
    });
}

/// Input field: validation consumes Tab/Enter before the grid keyboard.
fn edit_cell_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    column: NodeId,
    row: usize,
) {
    let Some(edit) = state.editing_mut() else {
        return;
    };
    let opening = if edit.typed {
        inplace_edit::Opening::KeepCursor
    } else {
        inplace_edit::Opening::SelectAll
    };
    // The shared in-place edit contract (#224): focus and selection on opening,
    // Enter, Tab and Escape **consumed** here (before the shortcut block of
    // `grid_contents`, which would reread them and move the selection a second
    // time: Enter on row 2 landed on row 4). Escape closes without writing
    // anything; any other end validates, and Enter/Shift+Enter/Tab/Shift+Tab also
    // move the selection, as in any spreadsheet.
    let shown = inplace_edit::show(
        ui,
        ui.id().with("cell_edit"),
        egui::TextEdit::singleline(&mut edit.buffer)
            .frame(egui::Frame::NONE)
            .desired_width(f32::INFINITY),
        opening,
        inplace_edit::FocusLoss::Ends,
        None,
    );
    let Some(end) = shown.end else {
        return;
    };
    if end == inplace_edit::EditEnd::Cancel {
        state.cancel_edit();
        return;
    }
    let (enter, tab, shift) = match end {
        inplace_edit::EditEnd::Enter { shift } => (true, false, shift),
        inplace_edit::EditEnd::Tab { shift } => (false, true, shift),
        _ => (false, false, false),
    };
    let pending = state
        .editing()
        .filter(|edit| edit.column == column && edit.row == row)
        .map(|edit| edit.buffer.clone());
    if let Some(text) = pending {
        state.cancel_edit();
        if enter && !shift {
            // `CommitCellAndAdvance` already adds a row when `row` was the last of the
            // column; selecting `row + 1` without waiting for that command (applied after
            // this frame's render) to have created the row is safe: either it already
            // existed, or it is about to.
            actions.push(Action::CommitCellAndAdvance {
                column,
                row,
                text,
                sheet,
            });
            state.select_cell(sheet, column, row + 1);
        } else {
            actions.push(Action::CommitCell { column, row, text });
            if enter {
                // Shift+Enter: goes up one row, never adds.
                state.move_selection(sheet, MoveDirection::Up, false);
            } else if tab && !shift && is_last_visible_column(state, sheet, column) {
                // Same gesture as in the shortcut block outside editing: Tab validating on the
                // last column adds a column rather than returning to the start of the next row
                // (see `Action::AddColumnAndSelect`).
                actions.push(Action::AddColumnAndSelect { sheet, row });
            } else if tab {
                state.move_selection_tab(sheet, shift);
            }
        }
    }
}

/// Resting cell: display, selection and lazy context menu.
fn display_cell_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    column: NodeId,
    row: usize,
) {
    let text = state.cell_display(column, row);
    let stale = state.column_staleness(column).is_some();
    let selected = state.is_selected(sheet, column, row);
    let rect = ui.available_rect_before_wrap();
    let response = ui.interact(rect, ui.id().with("cell"), egui::Sense::click_and_drag());
    if selected {
        ui.painter().rect_filled(
            rect,
            0.0,
            ui.visuals().selection.bg_fill.linear_multiply(0.35),
        );
    }
    // `.truncate()` rather than the default line wrapping of a `Label`: a value
    // too wide for its column is cut on the right (with a slight fade), never sent
    // onto a second line that would break the uniform height of the table rows.
    //
    // Spreadsheet-style alignment, in the **body** only (never the gutter nor the
    // header, which carry no column value): numbers on the right, text on the left
    // — `AppState::cell_is_numeric` decides per cell, not per column, so that a
    // `Mixed` column aligns each row according to its own value.
    //
    // `Label::halign` alone is not enough here: `ui.put` places the widget in a
    // `Layout::centered_and_justified`, which justifies the text (stretches it to
    // fill the width) before `halign` has a say — the result stayed visually
    // centered. A real directional `Layout` (`right_to_left`/`left_to_right`), like
    // the one already placed on the whole table (`cell_layout` of the
    // `TableBuilder`), really aligns the text against the wanted edge.
    let layout = if state.cell_is_numeric(column, row) {
        egui::Layout::right_to_left(egui::Align::Center)
    } else {
        egui::Layout::left_to_right(egui::Align::Center)
    };
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(4.0, 0.0)))
            .layout(layout),
        |ui| {
            // A column that is not up to date is entirely grayed out (#181, M9 a).
            let text = if stale {
                egui::RichText::new(text).color(ui.visuals().weak_text_color())
            } else {
                egui::RichText::new(text)
            };
            ui.add(egui::Label::new(text).selectable(false).truncate());
        },
    );

    if response.double_clicked() {
        state.begin_edit(column, row);
        return;
    }
    let modifiers = ui.input(|i| i.modifiers);
    let (command, shift) = (modifiers.command, modifiers.shift);
    if response.drag_started() && command {
        // Ctrl+drag: a new rectangle, the previous ones stay (#159).
        state.add_cell_selection(sheet, column, row);
    } else if response.clicked() && command && !shift {
        state.toggle_cell_selection(sheet, column, row);
    } else if response.drag_started() || (response.clicked() && !shift) {
        state.select_cell(sheet, column, row);
    } else if response.clicked() && shift {
        state.extend_selection(sheet, column, row);
    } else if response.contains_pointer()
        && !press_started_in(ui, rect)
        && !grid_drag_in_progress(ui.ctx())
        && ui.input(|i| i.pointer.primary_down())
        && state.selection(sheet).is_some()
    {
        // `response.dragged()` triggers only on the widget that *started* the drag —
        // never on cells merely hovered afterwards —, hence this manual test rather
        // than `dragged()` to extend the selection while dragging.
        //
        // `response.hovered()` is not suitable either: as long as a drag is in
        // progress *elsewhere* (on the starting cell), egui considers only that
        // original widget "hovered" — no other cell becomes `hovered()` again during
        // the whole drag, even under the cursor (`egui::
        // Context::interact`, doc of `InteractionSnapshot::hovered`:
        // "While dragging or clicking something, nothing else is hovered").
        // `contains_pointer()` stays true regardless of what is being dragged
        // elsewhere — that is the test needed here.
        state.extend_selection(sheet, column, row);
    } else if response.secondary_clicked() && !selected {
        // Right-click outside the selection: reduce it to this cell before opening
        // the menu — same rule as in the header and on the letter row (see
        // `header_field_widget`).
        state.select_cell(sheet, column, row);
    }
    // `grid_menu` **inside** the closure, never before: it runs only while the
    // menu is open. Computed outside, it would be recomputed on every frame for
    // *every visible cell* — and it walks the tree (`Selection::columns`), which
    // would make it a cost quadratic in the number of columns on a grid that only
    // displays.
    response.context_menu(|ui| selection_context_menu(ui, state, actions, sheet));
}

/// Is a drag of columns or rows to a graph in progress? As long as it lasts,
/// hovering other boxes no longer extends the selection: otherwise it would be
/// replaced "on the fly" by a cell selection (#160, #166).
fn grid_drag_in_progress(ctx: &egui::Context) -> bool {
    egui::DragAndDrop::has_payload_of_type::<ColumnDragPayload>(ctx)
        || egui::DragAndDrop::has_payload_of_type::<RowDragPayload>(ctx)
}

/// True if the current press started in `rect`. The selection extension on
/// hover concerns only the boxes where the drag did **not** start: on the
/// origin box, a Ctrl+click (which toggles on release) would otherwise already
/// have extended the selection during the press (#159).
fn press_started_in(ui: &egui::Ui, rect: egui::Rect) -> bool {
    ui.input(|i| i.pointer.press_origin())
        .is_some_and(|origin| rect.contains(origin))
}

#[cfg(test)]
mod tests {
    //! Grid tests, without a window: a bare `egui::Context`, injected events, and
    //! reading what is **painted** (texts, positions) or the pushed actions —
    //! never the source text.
    //!
    //! The bench paints the whole workbook ([`book_contents`]) rather than the grid
    //! alone: it is the content shared by the floating workbook and the detached
    //! workbook, hence the path the user takes in both windows.
    use super::super::test_support::frame_of;

    use super::super::workbook::book_contents;
    use super::*;
    use crate::theme;
    use stokesforge_core::{Column, Project};
    use stokesforge_session::Session;

    /// The grip bands border the **block** of selected columns — one on each side
    /// —, and only it: a block that is not contiguous has two pairs, the
    /// unselected columns none.
    #[test]
    fn grip_bands_border_each_block_of_selected_columns() {
        let ids: Vec<NodeId> = (1..=5).map(NodeId::from_raw).collect();
        let extents: Vec<ColumnExtent> = ids
            .iter()
            .enumerate()
            .map(|(i, &id)| (id, 100.0 * i as f32, 100.0 * (i + 1) as f32))
            .collect();
        let half = COLUMN_GRIP_HALF_WIDTH;

        let bands = column_grip_bands(&extents, &[ids[1], ids[2]], 20.0, 200.0);
        assert_eq!(bands.len(), 2, "one block: its two edges");
        assert_eq!(
            (bands[0].left(), bands[0].right()),
            (100.0 - half, 100.0 + half)
        );
        assert_eq!(
            (bands[1].left(), bands[1].right()),
            (300.0 - half, 300.0 + half)
        );
        assert_eq!((bands[0].top(), bands[0].bottom()), (20.0, 200.0));

        let bands = column_grip_bands(&extents, &[ids[0], ids[3], ids[4]], 20.0, 200.0);
        assert_eq!(bands.len(), 4, "two non-contiguous blocks: two pairs");
        assert_eq!(bands[2].center().x, 300.0);
        assert_eq!(bands[3].center().x, 500.0);

        assert!(column_grip_bands(&extents, &[], 20.0, 200.0).is_empty());
    }

    /// #166: the row grip bands border each **block** of selected rows — above the
    /// first, below the last —, within the bounds of the table body; an edge exists
    /// only if its row is painted AND it really borders the block.
    #[test]
    fn grip_bands_border_each_block_of_selected_rows() {
        let extents: Vec<RowExtent> = (3..8)
            .map(|row| (row, 20.0 * row as f32, 20.0 * (row + 1) as f32))
            .collect();
        let half = ROW_GRIP_HALF_HEIGHT;

        let bands = row_grip_bands(&extents, &[4, 5], 50.0, 400.0, 0.0, 1000.0);
        assert_eq!(bands.len(), 2, "one block: its two edges");
        assert_eq!(
            (bands[0].top(), bands[0].bottom()),
            (80.0 - half, 80.0 + half)
        );
        assert_eq!(
            (bands[1].top(), bands[1].bottom()),
            (120.0 - half, 120.0 + half)
        );
        assert_eq!((bands[0].left(), bands[0].right()), (50.0, 400.0));

        // Two non-contiguous blocks: two pairs.
        assert_eq!(
            row_grip_bands(&extents, &[3, 5, 6], 50.0, 400.0, 0.0, 1000.0).len(),
            4
        );
        // The block continues off screen (rows 1..=4, row 1 is not painted): no
        // invented top edge, only the real bottom edge.
        let bands = row_grip_bands(&extents, &[1, 2, 3, 4], 50.0, 400.0, 0.0, 1000.0);
        assert_eq!(bands.len(), 1);
        assert_eq!(bands[0].center().y, 100.0);
        // A band does not overflow onto the header (`top`) nor under the table.
        let bands = row_grip_bands(&extents, &[3], 50.0, 400.0, 61.0, 1000.0);
        assert_eq!(bands[0].top(), 61.0, "top edge clipped under the header");
        assert!(row_grip_bands(&extents, &[], 50.0, 400.0, 0.0, 1000.0).is_empty());
    }

    /// A neighboring button ("before", the first focusable widget of the frame, so
    /// the one egui would pick on a Tab) then a workbook of three columns of four
    /// rows, values 101..104, 201..204, 301..304.
    struct Bench {
        _session: Session,
        /// The preferences file of the bench: no test writes the user's.
        _preferences_dir: tempfile::TempDir,
        context: egui::Context,
        state: AppState,
        actions: Vec<Action>,
        book: NodeId,
        sheet: NodeId,
        columns: Vec<NodeId>,
        neighbor_focused: bool,
        last_frame: Option<egui::FullOutput>,
        /// How many `Action::SetColumnWidth` the grid emitted, all frames together
        /// (#325).
        widths_sent: usize,
        /// All the actions the grid pushed, all frames together (`actions` keeps only
        /// the last).
        seen: Vec<Action>,
        /// Size of the simulated window (700 x 420 by default); changing it enlarges
        /// the workbook in the next frame.
        size: egui::Vec2,
        /// A modal window painted after the workbook, in the same frame, as in
        /// `StokesForgeApp::ui` (#406).
        modal: Option<TestModal>,
        /// How many frames the modal answered Cancel / Close.
        cancels: usize,
    }

    /// The natures of modal window the bench can paint under the workbook.
    #[derive(Clone, Copy, Debug)]
    enum TestModal {
        /// The real Formula Functions window (a read-only modal).
        FormulaFunctions,
        Settings,
        Draft,
        Wizard,
        LiveSettings,
        ReadOnly,
        Confirm,
    }

    impl TestModal {
        /// Paints the window; true when it answered Cancel (or Close).
        fn paint(self, ctx: &egui::Context, state: &AppState, actions: &mut Vec<Action>) -> bool {
            use super::super::dialogs::{
                modal_confirm, modal_dialog, modal_dialog_with, modal_live_settings,
                modal_read_only, modal_wizard, ConfirmOutcome, DialogFooter, DialogNature,
                DialogOutcome, InteractiveSize, PlainSize,
            };
            let id = egui::Id::new("grid_test_modal");
            let mut text = String::from("abc");
            let field = |ui: &mut egui::Ui| {
                ui.text_edit_singleline(&mut text);
            };
            let footer = DialogFooter::always("Go", "Go hint");
            let size = InteractiveSize::Free(egui::vec2(300.0, 200.0));
            let outcome = match self {
                Self::FormulaFunctions => {
                    super::super::formula_functions_window::formula_functions_window(
                        ctx, state, actions,
                    );
                    return false;
                }
                Self::Settings => modal_dialog(ctx, id, "Modal", size, field),
                Self::Draft => modal_dialog_with(
                    ctx,
                    id,
                    "Modal",
                    DialogNature::Draft(size),
                    footer,
                    |_| {},
                    field,
                ),
                Self::Wizard => modal_wizard(ctx, id, "Modal", size, footer, |_| {}, field),
                Self::LiveSettings => modal_live_settings(ctx, id, "Modal", size, field),
                Self::ReadOnly => modal_read_only(
                    ctx,
                    id,
                    "Modal",
                    PlainSize::Content(egui::vec2(300.0, 200.0)),
                    |ui| {
                        ui.label("content");
                    },
                ),
                Self::Confirm => {
                    let outcome = modal_confirm(
                        ctx,
                        id,
                        "Modal",
                        PlainSize::Content(egui::vec2(300.0, 200.0)),
                        "Delete",
                        "Delete hint",
                        |ui| {
                            ui.label("content");
                        },
                    );
                    return outcome == ConfirmOutcome::Cancel;
                }
            };
            outcome == DialogOutcome::Cancel
        }
    }

    impl Bench {
        fn new() -> Self {
            Self::with_values(4, |c, r| f64::from(c * 100 + r))
        }

        /// Like `new`, with `rows` rows where the value of row `r` (1..=rows) of
        /// column `c` (1..=3) is given by `value`: wide numbers make the measured
        /// content of a cell grow beyond the default width.
        fn with_values(rows: u32, value: impl Fn(u32, u32) -> f64) -> Self {
            let mut project = Project::new("Keyboard").unwrap();
            let root = project.root();
            let book = project.add_empty_workbook(root, None).unwrap();
            let sheet = project.add_worksheet(book, None).unwrap();
            let columns = (1..=3)
                .map(|c| {
                    let values: Vec<f64> = (1..=rows).map(|r| value(c, r)).collect();
                    project
                        .add_column(sheet, None, None, Column::from_f64(&values))
                        .unwrap()
                })
                .collect();
            let session = Session::spawn(project, None).unwrap();
            let preferences_dir = tempfile::tempdir().unwrap();
            let preferences = stokesforge_app_state::preferences::Preferences::load_from(Some(
                preferences_dir.path().join("preferences.json"),
            ));
            let state = AppState::new_with_preferences(
                session.handle(),
                "test.stf".to_string(),
                preferences,
            )
            .unwrap();
            let context = egui::Context::default();
            context.set_fonts(stokesforge_render_egui::font_definitions());
            theme::install(&context);
            // All the header rows: the layout under test is the full one.
            set_header_rows(
                &context,
                book,
                HeaderRows {
                    metadata: true,
                    x_prime: true,
                },
            );
            let mut bench = Bench {
                _session: session,
                _preferences_dir: preferences_dir,
                context,
                state,
                actions: Vec::new(),
                book,
                sheet,
                columns,
                neighbor_focused: false,
                last_frame: None,
                widths_sent: 0,
                seen: Vec::new(),
                size: egui::vec2(700.0, 420.0),
                modal: None,
                cancels: 0,
            };
            for _ in 0..3 {
                bench.image(Vec::new());
            }
            bench
        }

        fn image(&mut self, events: Vec<egui::Event>) {
            self.actions.clear();
            let (state, actions, book) = (&mut self.state, &mut self.actions, self.book);
            let modal = self.modal;
            let mut neighbor = false;
            let mut cancelled = false;
            let output = frame_of(&self.context, self.size, events, |ui| {
                neighbor = ui.button("before").has_focus();
                book_contents(ui, state, actions, book);
                if let Some(modal) = modal {
                    cancelled |= modal.paint(ui.ctx(), state, actions);
                }
            });
            self.cancels += usize::from(cancelled);
            self.neighbor_focused = neighbor;
            self.last_frame = Some(output);
            self.seen.extend(self.actions.iter().cloned());
            self.widths_sent += self
                .actions
                .iter()
                .filter(|action| matches!(action, Action::SetColumnWidth { .. }))
                .count();
        }

        fn press_key(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
            // Explicit `ModifiersChanged`: an `Event::Key` does not update
            // `InputState::modifiers`, which the grid reads for Shift.
            let press_key = |pressed| egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            };
            self.image(vec![
                egui::Event::ModifiersChanged(modifiers),
                press_key(true),
            ]);
            self.image(vec![
                press_key(false),
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            ]);
            // One more frame, without events: the focus given back to another widget by
            // `end_pass` would show here.
            self.image(Vec::new());
        }

        fn tab(&mut self) {
            self.press_key(egui::Key::Tab, egui::Modifiers::NONE);
        }

        fn shift_tab(&mut self) {
            self.press_key(egui::Key::Tab, egui::Modifiers::SHIFT);
        }

        /// Center of the painted text exactly equal to `text` in the last frame, if
        /// there is one.
        fn center_of(&self, text: &str) -> Option<egui::Pos2> {
            self.last_frame
                .as_ref()
                .unwrap()
                .shapes
                .iter()
                .find_map(|painted| match &painted.shape {
                    egui::Shape::Text(shape) if shape.galley.job.text == text => {
                        Some(shape.pos + shape.galley.rect.center().to_vec2())
                    }
                    _ => None,
                })
        }

        /// All the texts painted in the last frame.
        fn texts(&self) -> Vec<String> {
            self.last_frame
                .as_ref()
                .unwrap()
                .shapes
                .iter()
                .filter_map(|painted| match &painted.shape {
                    egui::Shape::Text(shape) => Some(shape.galley.job.text.clone()),
                    _ => None,
                })
                .collect()
        }

        /// Click on the painted text `text`, as with the mouse, with button
        /// `button`.
        fn click_button(&mut self, text: &str, button: egui::PointerButton) {
            let position = self
                .center_of(text)
                .unwrap_or_else(|| panic!("no painted text carries {text:?}"));
            self.image(vec![egui::Event::PointerMoved(position)]);
            for pressed in [true, false] {
                self.image(vec![egui::Event::PointerButton {
                    pos: position,
                    button,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
            // Let the fall-back frames pass (focus, release).
            self.image(Vec::new());
        }

        /// Left click on the painted text `text`, `modifiers` held down (explicit
        /// `ModifiersChanged`: the grid reads `InputState::modifiers`).
        fn click_with(&mut self, text: &str, modifiers: egui::Modifiers) {
            let position = self
                .center_of(text)
                .unwrap_or_else(|| panic!("no painted text carries {text:?}"));
            self.image(vec![
                egui::Event::PointerMoved(position),
                egui::Event::ModifiersChanged(modifiers),
            ]);
            for pressed in [true, false] {
                self.image(vec![egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
                }]);
            }
            self.image(vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)]);
            self.image(Vec::new());
        }

        /// Left click on the painted cell carrying `text`.
        fn click_on(&mut self, text: &str) {
            self.click_button(text, egui::PointerButton::Primary);
        }

        fn cursor(&self) -> Option<GridCursor> {
            self.state
                .selection(self.sheet)
                .and_then(Selection::grid_cursor)
        }
    }

    // --- Keyboard focus (#38) ---------------------------------------------
    //
    // Tab and the arrows belong to the grid. egui handles them at the start of
    // the frame (`Focus::begin_pass`) to move **its** focus, unless the focused
    // widget claims them through an `EventFilter` ([`GRID_KEYS`]): without a
    // filter, Tab made the focus leave the table for the rest of the interface
    // instead of advancing the selection from cell to cell.

    #[test]
    fn f2_edits_the_cell_like_a_double_click() {
        let mut entries = Vec::new();
        for keyboard in [false, true] {
            let mut bench = Bench::new();
            bench.click_on("101");
            if keyboard {
                bench.press_key(egui::Key::F2, egui::Modifiers::NONE);
            } else {
                bench.click_on("101");
            }
            let edit = bench
                .state
                .editing()
                .unwrap_or_else(|| panic!("the gesture opens the editor (keyboard: {keyboard})"));
            assert_eq!(
                (edit.column, edit.row, edit.buffer.as_str()),
                (bench.columns[0], 0, "101")
            );
            assert!(
                keyboard_focus_outside_grid(&bench.context),
                "the field has the focus"
            );
            bench.image(vec![egui::Event::Text("7".into())]);
            let entry = bench.state.editing().unwrap().buffer.clone();
            assert_ne!(entry, "101");
            entries.push(entry.clone());
            bench.image(vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            assert!(bench.state.editing().is_none());
            assert!(bench.actions.iter().any(|action| matches!(action,
                Action::CommitCellAndAdvance { column, row: 0, text, .. }
                if *column == bench.columns[0] && text == &entry)));
        }
        assert_eq!(entries[0], entries[1], "same focus and same entry cursor");
    }

    #[test]
    fn f2_edits_each_header_field_like_a_double_click() {
        for field in [
            HeaderField::LongName,
            HeaderField::Unit,
            HeaderField::Comment,
            HeaderField::Formula,
        ] {
            let mut entries = Vec::new();
            for keyboard in [false, true] {
                let mut bench = Bench::new();
                let column = bench.columns[0];
                bench.state.begin_header_edit(column, field);
                bench.state.header_editing_mut().unwrap().buffer = "9".into();
                bench.state.commit_header_edit();
                bench.image(Vec::new());
                bench.click_on("9");
                if keyboard {
                    bench.press_key(egui::Key::F2, egui::Modifiers::NONE);
                } else {
                    bench.click_on("9");
                }
                let edit = bench
                    .state
                    .header_editing()
                    .expect("the gesture opens the field");
                assert_eq!(
                    (edit.column, edit.field, edit.buffer.as_str()),
                    (column, field, "9")
                );
                assert!(bench.context.text_edit_focused());
                bench.image(vec![egui::Event::Text("7".into())]);
                let entry = bench.state.header_editing().unwrap().buffer.clone();
                assert_ne!(entry, "9");
                bench.press_key(egui::Key::Enter, egui::Modifiers::NONE);
                assert!(bench.state.header_editing().is_none());
                assert_eq!(header_field_text(&bench.state, column, field), entry);
                entries.push(entry.clone());
                // Enter may move the selection: go back to the validated field to then test
                // its cancellation, as with a double-click.
                bench.state.select_header_field(bench.sheet, column, field);
                bench.press_key(egui::Key::F2, egui::Modifiers::NONE);
                bench.image(vec![egui::Event::Text("8".into())]);
                assert_ne!(bench.state.header_editing().unwrap().buffer, entry);
                bench.press_key(egui::Key::Escape, egui::Modifiers::NONE);
                assert!(bench.state.header_editing().is_none());
                assert_eq!(header_field_text(&bench.state, column, field), entry);
            }
            assert_eq!(entries[0], entries[1], "same entry for {field:?}");
        }
    }

    #[test]
    fn f2_edits_the_top_left_corner_of_a_reversed_header_range() {
        let mut bench = Bench::new();
        bench.click_on("303");
        bench
            .state
            .select_header_field(bench.sheet, bench.columns[2], HeaderField::Comment);
        bench.state.extend_header_field_selection(
            bench.sheet,
            bench.columns[0],
            HeaderField::LongName,
        );
        bench.press_key(egui::Key::F2, egui::Modifiers::NONE);
        let edit = bench.state.header_editing().unwrap();
        assert_eq!(
            (edit.column, edit.field),
            (bench.columns[0], HeaderField::LongName)
        );
        assert!(bench.state.editing().is_none());
    }

    #[test]
    fn f2_edits_the_top_left_corner_of_a_reversed_range() {
        let mut bench = Bench::new();
        bench.click_on("303");
        bench
            .state
            .extend_selection(bench.sheet, bench.columns[0], 0);
        bench.press_key(egui::Key::F2, egui::Modifiers::NONE);
        let edit = bench.state.editing().unwrap();
        assert_eq!(
            (edit.column, edit.row, edit.buffer.as_str()),
            (bench.columns[0], 0, "101")
        );
    }

    #[test]
    fn f2_cancels_without_writing_and_does_not_steal_the_neighbor_focus() {
        let mut bench = Bench::new();
        bench.click_on("101");
        bench.press_key(egui::Key::F2, egui::Modifiers::NONE);
        assert!(bench.state.editing().is_some());
        bench.image(vec![egui::Event::Text("7".into())]);
        bench.image(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(bench.actions.is_empty(), "Escape submits no command");
        assert!(bench.state.editing().is_none());
        assert_eq!(bench.state.cell_text(bench.columns[0], 0), "101");
        bench.click_on("before");
        bench.press_key(egui::Key::F2, egui::Modifiers::NONE);
        assert!(bench.state.editing().is_none());
    }

    #[test]
    fn tab_outside_editing_advances_the_selection_without_leaving_the_grid() {
        let mut bench = Bench::new();
        bench.click_on("101");
        assert_eq!(
            bench.cursor(),
            Some(GridCursor::Data(bench.columns[0], 0)),
            "the click selects the cell"
        );
        bench.tab();
        assert_eq!(
            bench.cursor(),
            Some(GridCursor::Data(bench.columns[1], 0)),
            "Tab advances by one column"
        );
        assert!(
            !bench.neighbor_focused,
            "Tab made the focus leave the grid for the neighboring widget"
        );
        bench.tab();
        assert_eq!(bench.cursor(), Some(GridCursor::Data(bench.columns[2], 0)));
        bench.shift_tab();
        assert_eq!(
            bench.cursor(),
            Some(GridCursor::Data(bench.columns[1], 0)),
            "Shift+Tab goes back"
        );
        assert!(!bench.neighbor_focused);
    }

    #[test]
    fn the_arrows_move_the_selection_and_keep_the_focus_in_the_grid() {
        let mut bench = Bench::new();
        bench.click_on("102");
        for (key, expected) in [
            (egui::Key::ArrowRight, GridCursor::Data(bench.columns[1], 1)),
            (egui::Key::ArrowDown, GridCursor::Data(bench.columns[1], 2)),
            (egui::Key::ArrowLeft, GridCursor::Data(bench.columns[0], 2)),
            (egui::Key::ArrowUp, GridCursor::Data(bench.columns[0], 1)),
        ] {
            bench.press_key(key, egui::Modifiers::NONE);
            assert_eq!(bench.cursor(), Some(expected), "{key:?}");
            assert!(!bench.neighbor_focused, "{key:?} moved the egui focus");
        }
    }

    #[test]
    fn tab_during_editing_validates_and_keeps_the_focus_in_the_grid() {
        let mut bench = Bench::new();
        bench.click_on("101");
        // A keystroke on a selected cell opens the edit.
        bench.image(vec![egui::Event::Text("7".to_string())]);
        bench.image(Vec::new());
        bench.image(Vec::new());
        assert!(
            bench.state.editing().is_some(),
            "the keystroke opens the edit"
        );
        bench.tab();
        assert!(bench.state.editing().is_none(), "Tab validates the entry");
        assert_eq!(
            bench.cursor(),
            Some(GridCursor::Data(bench.columns[1], 0)),
            "Tab validates *and* advances"
        );
        // And the grid keeps control for the following Tabs, outside editing.
        bench.tab();
        assert_eq!(
            bench.cursor(),
            Some(GridCursor::Data(bench.columns[2], 0)),
            "after validation, Tab keeps advancing in the grid"
        );
        assert!(!bench.neighbor_focused);
    }

    /// Tab validated in a cell pushes the entry to the model
    /// (`Action::CommitCell`) in the very frame of the key: the typed value is not
    /// lost along the way.
    #[test]
    fn tab_during_editing_pushes_the_entry_in_the_frame_of_the_key() {
        let mut bench = Bench::new();
        bench.click_on("101");
        bench.image(vec![egui::Event::Text("7".to_string())]);
        bench.image(Vec::new());
        bench.image(vec![
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        let first_column = bench.columns[0];
        assert!(
            bench.actions.iter().any(|action| matches!(
                action,
                Action::CommitCell { column, row: 0, text } if *column == first_column && text == "7"
            )),
            "the entry must go to the model: {:?}",
            bench.actions
        );
    }

    /// #52: Tab on the last column adds a column whatever the header field,
    /// outside editing as in editing — no longer only Formula.
    #[test]
    fn tab_on_the_last_column_adds_a_column_for_each_header_field() {
        for edit in [false, true] {
            for field in [
                HeaderField::LongName,
                HeaderField::Unit,
                HeaderField::Comment,
                HeaderField::Formula,
            ] {
                let mut bench = Bench::new();
                let (sheet, last) = (bench.sheet, bench.columns[2]);
                // The click gives the focus to the grid (otherwise Tab is not its own).
                bench.click_on("301");
                bench.state.select_header_field(sheet, last, field);
                if edit {
                    bench.state.begin_header_edit(last, field);
                }
                for _ in 0..3 {
                    bench.image(Vec::new());
                }
                bench.image(vec![
                    egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                    egui::Event::Key {
                        key: egui::Key::Tab,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
                assert!(
                    bench.actions.iter().any(|action| matches!(
                        action,
                        Action::AddColumnAndSelectHeaderField { field: f, .. } if *f == field
                    )),
                    "Tab on {field:?} (edit: {edit}) must add a column"
                );
            }
        }
    }

    #[test]
    fn tab_during_a_header_field_edit_validates_and_stays_in_the_grid() {
        let mut bench = Bench::new();
        let (sheet, first_column) = (bench.sheet, bench.columns[0]);
        bench
            .state
            .select_header_field(sheet, first_column, HeaderField::LongName);
        bench
            .state
            .begin_header_edit(first_column, HeaderField::LongName);
        for _ in 0..3 {
            bench.image(Vec::new());
        }
        assert!(bench.state.header_editing().is_some());
        bench.tab();
        assert!(
            bench.state.header_editing().is_none(),
            "Tab validates the field"
        );
        assert_eq!(
            bench.cursor(),
            Some(GridCursor::Header(bench.columns[1], HeaderField::LongName)),
            "Tab validates *and* moves to the next column"
        );
        bench.tab();
        assert_eq!(
            bench.cursor(),
            Some(GridCursor::Header(bench.columns[2], HeaderField::LongName)),
            "then continues outside editing"
        );
        assert!(!bench.neighbor_focused);
    }

    /// The grid's focus is not retained: a click elsewhere gives it back, and Tab
    /// then takes back its egui role (move the focus), without touching the grid's
    /// selection.
    #[test]
    fn a_click_outside_the_grid_takes_tab_away_from_it() {
        let mut bench = Bench::new();
        bench.click_on("101");
        assert!(!keyboard_focus_outside_grid(&bench.context));
        assert!(
            bench.context.memory(|memory| memory.focused()).is_some(),
            "a click in the grid gives it keyboard focus"
        );
        bench.click_on("before");
        let before = bench.cursor();
        bench.tab();
        assert_eq!(
            bench.cursor(),
            before,
            "Tab, outside the grid, does not move its selection"
        );
    }

    /// The grid's focus is not a text field for the rest of the application (Enter
    /// of a dialog, Delete of an annotation): only another focused widget is.
    #[test]
    fn the_grid_focus_is_not_a_field_being_typed_in() {
        let mut bench = Bench::new();
        assert!(!keyboard_focus_outside_grid(&bench.context), "no focus");
        bench.click_on("101");
        assert!(!keyboard_focus_outside_grid(&bench.context), "grid focus");
        bench
            .context
            .memory_mut(|memory| memory.request_focus(egui::Id::new("foreign_field")));
        assert!(keyboard_focus_outside_grid(&bench.context), "foreign focus");
    }

    /// The keyboard focus of a field foreign to the grid (here a text field of a
    /// dialog) stays its own: neither Tab nor arrow moves the selection. The
    /// witness is `the_arrows_move_the_selection_…` and `tab_outside_editing_…`:
    /// same keys, focused grid, selection moved.
    #[test]
    fn a_focused_foreign_field_keeps_tab_and_the_arrows() {
        let mut bench = Bench::new();
        bench.click_on("101");
        let field_id = egui::Id::new("foreign_field");
        let before = bench.cursor();
        let mut text = String::new();
        for key in [egui::Key::ArrowRight, egui::Key::Tab] {
            // Three frames so that the field is placed and focused, then the key.
            for frame in 0..4 {
                let events = if frame == 3 {
                    vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }]
                } else {
                    Vec::new()
                };
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(700.0, 420.0),
                    )),
                    events,
                    ..Default::default()
                };
                let (state, actions, book) = (&mut bench.state, &mut bench.actions, bench.book);
                let mut output = bench.context.run_ui(input, |ui| {
                    egui::Area::new(egui::Id::new("dialog")).show(ui.ctx(), |ui| {
                        ui.add(egui::TextEdit::singleline(&mut text).id(field_id));
                    });
                    if frame < 3 {
                        ui.ctx().memory_mut(|memory| memory.request_focus(field_id));
                    }
                    book_contents(ui, state, actions, book);
                });
                output.textures_delta.clear();
            }
            assert_eq!(
                bench.cursor(),
                before,
                "{key:?}: the grid stole the key from the focused text field"
            );
        }
    }

    /// #17: the grid stays painted under a dialog (Graph properties, import…); as
    /// long as a text field there has the focus, Backspace and Delete belong to
    /// it — the grid must neither consume them nor clear the selected cell.
    /// Witness: the same bench without focus on the field does clear the cell
    /// (without it, a grid deaf to Backspace would pass the test).
    #[test]
    fn a_focused_text_field_keeps_backspace_and_delete() {
        for field_focused in [true, false] {
            let mut project = Project::new("Keyboard").unwrap();
            let root = project.root();
            let book = project.add_empty_workbook(root, None).unwrap();
            let sheet = project.add_worksheet(book, None).unwrap();
            let column = project
                .add_column(sheet, None, None, Column::from_f64(&[1.0, 2.0]))
                .unwrap();
            let session = Session::spawn(project, None).unwrap();
            let mut state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
            state.select_cell(sheet, column, 0);
            let context = egui::Context::default();
            context.set_fonts(stokesforge_render_egui::font_definitions());
            let mut title = String::from("Title");
            let mut actions = Vec::new();
            for frame in 0..3 {
                let mut input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    ..Default::default()
                };
                if frame == 2 {
                    for key in [egui::Key::Backspace, egui::Key::Delete] {
                        input.events.push(egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
                let mut output = context.run_ui(input, |ui| {
                    let field = egui::Id::new("dialog_field");
                    egui::Area::new(egui::Id::new("dialog")).show(ui.ctx(), |ui| {
                        ui.add(egui::TextEdit::singleline(&mut title).id(field));
                    });
                    if field_focused {
                        ui.ctx().memory_mut(|memory| memory.request_focus(field));
                    }
                    actions.clear();
                    grid_contents(ui, &mut state, &mut actions, sheet, None);
                });
                output.textures_delta.clear();
            }
            let cleared = actions.iter().any(|action| {
                matches!(
                    action,
                    Action::ClearSelectedCells { .. } | Action::DeleteSelectedCells { .. }
                )
            });
            if field_focused {
                assert!(
                    !cleared,
                    "the grid stole Backspace/Delete from the text field: {actions:?}"
                );
            } else {
                assert!(
                    cleared,
                    "witness: without a focused field, Backspace clears the cell: {actions:?}"
                );
            }
        }
    }

    // --- Stacked header and gutter --------------------------------------

    /// By default Metadata is shown and x' hidden; the setting is specific to each
    /// workbook and the preference applies only to those that have not touched it.
    /// The stacked header loses a sub-row per hidden row.
    #[test]
    fn header_rows_are_set_per_workbook() {
        let context = egui::Context::default();
        let (a, b) = (NodeId::from_raw(1), NodeId::from_raw(2));
        assert_eq!(
            header_rows(&context, Some(a)),
            HeaderRows {
                metadata: true,
                x_prime: false
            }
        );
        // The preference applies to workbooks with no setting…
        publish_header_row_defaults(&context, false, true);
        assert_eq!(
            header_rows(&context, Some(b)),
            HeaderRows {
                metadata: false,
                x_prime: true
            }
        );
        // …but not to the one that has its own setting.
        let own = HeaderRows {
            metadata: true,
            x_prime: false,
        };
        set_header_rows(&context, a, own);
        publish_header_row_defaults(&context, true, true);
        assert_eq!(header_rows(&context, Some(a)), own);
        assert!(header_rows(&context, Some(b)).x_prime);
        let all = HeaderRows {
            metadata: true,
            x_prime: true,
        };
        assert_eq!(
            stacked_header_height(all) - stacked_header_height(own),
            HEADER_SUBROW_HEIGHT
        );
        assert_eq!(
            stacked_header_height(all)
                - stacked_header_height(HeaderRows {
                    metadata: false,
                    x_prime: false
                }),
            2.0 * HEADER_SUBROW_HEIGHT
        );
    }

    /// The stacked header, as painted: the gutter carries "#" on the letter row,
    /// then each field label at the height of its row (long name a bit taller than
    /// the others), the column letter and its role on the same row as "#", the
    /// first data row below. Hiding the "Metadata" row removes its label and moves
    /// the data up by exactly one sub-row — the witness that the position read
    /// follows the header height.
    #[test]
    fn the_stacked_header_aligns_its_labels_on_its_rows() {
        let mut bench = Bench::new();
        let letter = format!(
            "A({})",
            role_label(bench.state.column_meta(bench.columns[0]).role)
        );
        let labels = [
            "#".to_string(),
            tr!("column-x-prime"),
            tr!("column-long-name"),
            tr!("column-units"),
            tr!("column-comments"),
            tr!("workbook-header-formula"),
            tr!("common-metadata"),
            "1".to_string(),
        ];
        let y: Vec<f32> = labels
            .iter()
            .map(|label| {
                bench
                    .center_of(label)
                    .unwrap_or_else(|| panic!("{label:?} missing: {:?}", bench.texts()))
                    .y
            })
            .collect();
        // Gap between the centers of two neighboring rows: half-height of one plus
        // half-height of the other.
        let gaps = [
            HEADER_SUBROW_HEIGHT,
            (HEADER_SUBROW_HEIGHT + HEADER_LONGNAME_HEIGHT) / 2.0,
            (HEADER_LONGNAME_HEIGHT + HEADER_SUBROW_HEIGHT) / 2.0,
            HEADER_SUBROW_HEIGHT,
            HEADER_SUBROW_HEIGHT,
            HEADER_SUBROW_HEIGHT,
        ];
        for (index, gap) in gaps.iter().enumerate() {
            let measured = y[index + 1] - y[index];
            assert!(
                (measured - gap).abs() <= 0.5,
                "{:?} → {:?}: gap {measured}, expected {gap}",
                labels[index],
                labels[index + 1]
            );
        }
        assert!(y[7] > y[6], "the first data row is under the header");
        let y_letter = bench
            .center_of(&letter)
            .unwrap_or_else(|| panic!("{letter:?} missing: {:?}", bench.texts()))
            .y;
        assert!(
            (y_letter - y[0]).abs() <= 0.5,
            "the letter {letter:?} and '#' share the letter row"
        );

        set_header_rows(
            &bench.context,
            bench.book,
            HeaderRows {
                metadata: false,
                x_prime: true,
            },
        );
        bench.image(Vec::new());
        bench.image(Vec::new());
        assert!(
            bench.center_of(&tr!("common-metadata")).is_none(),
            "the hidden 'Metadata' row no longer paints its label"
        );
        let y_data = bench.center_of("1").expect("first row").y;
        assert!(
            (y[7] - y_data - HEADER_SUBROW_HEIGHT).abs() <= 0.5,
            "the data moves up by one sub-row: {} → {y_data}",
            y[7]
        );
    }

    /// #125: long translations stay in their row, even when the gutter is
    /// narrowed to its minimum width.
    #[test]
    fn gutter_labels_stay_on_a_single_line() {
        use stokesforge_app_state::i18n::{build_loader, Language};
        let mut bench = Bench::new();
        for language in [Language::English, Language::French, Language::PseudoEnglish] {
            let loader = build_loader(language);
            let labels = [
                tr!(in loader; "column-x-prime"),
                tr!(in loader; "column-long-name"),
                tr!(in loader; "column-units"),
                tr!(in loader; "column-comments"),
                tr!(in loader; "workbook-header-formula"),
                tr!(in loader; "common-metadata"),
            ];
            for width in [ROW_NUMBER_COLUMN_WIDTH, MIN_ROW_NUMBER_COLUMN_WIDTH] {
                let mut output = bench.context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 500.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        ui.set_width(width);
                        row_number_header_widget(
                            ui,
                            &mut bench.state,
                            &mut Vec::new(),
                            bench.sheet,
                        );
                        // The field labels go through exactly the same widget.
                        for label in &labels {
                            header_row_label_widget(
                                ui,
                                &mut bench.state,
                                &mut Vec::new(),
                                bench.sheet,
                                HeaderField::LongName,
                                HEADER_LONGNAME_HEIGHT,
                                label,
                            );
                        }
                    },
                );
                output.textures_delta.clear();
                let mut found = 0;
                for painted in &output.shapes {
                    if let egui::Shape::Text(text) = &painted.shape {
                        if labels.contains(&text.galley.text().to_string())
                            || text.galley.text() == tr!("common-metadata")
                        {
                            found += 1;
                            assert_eq!(
                                text.galley.rows.len(),
                                1,
                                "{language:?}, {width} px: {:?}",
                                text.galley.text()
                            );
                        }
                    }
                }
                assert!(
                    found >= 6,
                    "the five translations and the Metadata row are really painted"
                );
            }
        }
    }

    /// The gutter background has three distinct levels, and "selected" wins over
    /// "concerned by the selection".
    #[test]
    fn the_gutter_background_has_three_levels() {
        let at_rest = gutter_fill(false, false);
        let touched = gutter_fill(false, true);
        let chosen = gutter_fill(true, false);
        assert_ne!(at_rest, touched);
        assert_ne!(at_rest, chosen);
        assert_ne!(touched, chosen);
        assert_eq!(gutter_fill(true, true), chosen);
    }

    /// Every type and every role offered by the letter row's menu has a non-empty
    /// and distinct label: two entries with the same name would make the choice
    /// ambiguous.
    #[test]
    fn type_and_role_labels_are_distinct() {
        let types: Vec<String> = [ColumnType::Numeric, ColumnType::DateTime, ColumnType::Mixed]
            .into_iter()
            .map(type_label)
            .collect();
        let roles: Vec<String> = ColumnRole::ALL.into_iter().map(role_label).collect();
        for labels in [&types, &roles] {
            let distinct: std::collections::HashSet<&String> = labels.iter().collect();
            assert_eq!(distinct.len(), labels.len(), "{labels:?}");
            assert!(labels.iter().all(|label| !label.is_empty()));
        }
        assert_eq!(column_letter(0), "A");
        assert_eq!(column_letter(26), "AA");
    }

    // --- Edit menu ---------------------------------------------------

    /// The same edit block on right-click on a cell and on a field label of the
    /// gutter, with one difference: "Delete (Shift Up)" does not exist in the
    /// header, where Delete has nothing to bring up. A right-click outside the
    /// selection first reduces it to the targeted box.
    #[test]
    fn the_edit_menu_is_the_same_on_cell_and_header_except_delete_and_shift() {
        let common = [
            tr!("action-cut"),
            tr!("action-copy"),
            tr!("action-copy-with-header"),
            tr!("action-paste"),
            tr!("action-paste-in-place"),
            tr!("action-clear"),
        ];
        let shift_up = tr!("action-delete-shift-up");

        let mut cell_bench = Bench::new();
        cell_bench.click_button("102", egui::PointerButton::Secondary);
        let texts = cell_bench.texts();
        for entry in common.iter().chain([&shift_up]) {
            assert!(
                texts.contains(entry),
                "cell: {entry:?} missing from {texts:?}"
            );
        }
        for missing_entry in [tr!("history-undo"), tr!("history-redo")] {
            assert!(
                !texts.contains(&missing_entry),
                "Undo and Redo stay with Edit and the keyboard: {texts:?}"
            );
        }
        assert_eq!(
            cell_bench.cursor(),
            Some(GridCursor::Data(cell_bench.columns[0], 1)),
            "the right-click selects the targeted cell"
        );

        let mut header_bench = Bench::new();
        header_bench.click_button(&tr!("column-units"), egui::PointerButton::Secondary);
        let texts = header_bench.texts();
        for entry in &common {
            assert!(
                texts.contains(entry),
                "header: {entry:?} missing from {texts:?}"
            );
        }
        assert!(
            !texts.contains(&shift_up),
            "'Delete (Shift Up)' does not belong in the header: {texts:?}"
        );
    }

    /// On a selection of whole columns (or rows), the right-click of a cell is the
    /// letters' one: Plot, type and role included; on a lone cell, it stays the
    /// edit one (witness).
    #[test]
    fn the_right_click_of_a_column_or_row_selection_is_the_letters_one() {
        let letter_specific = [tr!("workbook-plot-menu"), tr!("column-type")];

        let mut columns_bench = Bench::new();
        columns_bench
            .state
            .select_column(columns_bench.sheet, columns_bench.columns[0]);
        columns_bench.image(Vec::new());
        columns_bench.click_button("102", egui::PointerButton::Secondary);
        let texts = columns_bench.texts();
        for entry in letter_specific.iter().chain([
            &tr!("action-copy"),
            &tr!("action-clear"),
            &tr!("menu-analysis"),
        ]) {
            assert!(
                texts.contains(entry),
                "selected column: {entry:?} missing from {texts:?}"
            );
        }
        assert!(
            columns_bench
                .state
                .is_column_selected(columns_bench.sheet, columns_bench.columns[0]),
            "a right-click inside the selection leaves it intact"
        );

        let mut rows_bench = Bench::new();
        rows_bench.state.select_row(rows_bench.sheet, 1);
        rows_bench.image(Vec::new());
        rows_bench.click_button("102", egui::PointerButton::Secondary);
        let texts = rows_bench.texts();
        for entry in &letter_specific {
            assert!(
                texts.contains(entry),
                "selected row: {entry:?} missing from {texts:?}"
            );
        }

        let mut cell_bench = Bench::new();
        cell_bench.click_button("102", egui::PointerButton::Secondary);
        let texts = cell_bench.texts();
        for entry in &letter_specific {
            assert!(
                !texts.contains(entry),
                "witness: a lone cell does not have {entry:?}: {texts:?}"
            );
        }
    }

    /// The header of a column produced by an out-of-date calculation carries a
    /// warning icon (#200); up to date, it carries none (witness).
    #[test]
    fn the_header_of_a_stale_column_carries_the_warning_icon() {
        use stokesforge_app_state::analysis::Preset;
        let mut bench = Bench::new();
        egui_extras::install_image_loaders(&bench.context);
        let icons = |bench: &mut Bench| {
            bench.image(Vec::new());
            bench.image(Vec::new());
            crate::app::window_chrome::tests::find_icon_centers(
                &bench.last_frame.as_ref().unwrap().shapes,
            )
            .len()
        };
        bench.state.dispatch(Action::OpenAnalysis {
            preset: Preset::Integrate,
            sheet: bench.sheet,
            columns: vec![bench.columns[0]],
        });
        {
            let form = bench.state.analysis_form_mut().unwrap();
            form.setup.integrate.cumulative = true;
            form.auto_recalc = false;
            form.auto_recalc_chosen = true;
        }
        bench.state.dispatch(Action::ApplyAnalysis);
        let up_to_date = icons(&mut bench);
        bench.state.dispatch(Action::CommitCell {
            column: bench.columns[0],
            row: 0,
            text: "999".to_string(),
        });
        assert_eq!(bench.state.stale_calculations().len(), 1);
        let stale = icons(&mut bench);
        assert_eq!(stale, up_to_date + 1, "one more warning icon");
    }

    /// Choosing "Clear" in a cell's menu pushes the clearing of the selection,
    /// like Backspace.
    #[test]
    fn clear_in_a_cell_menu_clears_the_selection() {
        let mut bench = Bench::new();
        bench.click_button("102", egui::PointerButton::Secondary);
        let position = bench.center_of(&tr!("action-clear")).expect("menu open");
        let sheet = bench.sheet;
        bench.image(vec![egui::Event::PointerMoved(position)]);
        bench.image(vec![egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        bench.image(vec![egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(
            bench.actions.iter().any(
                |action| matches!(action, Action::ClearSelectedCells { sheet: s } if *s == sheet)
            ),
            "{:?}",
            bench.actions
        );
    }

    // --- Scroll bars ---------------------------------------------------

    #[test]
    fn the_grid_vertical_bar_is_scaled_to_the_table_body() {
        assert_eq!(grid_vertical_bar_scale(320.0, 160.0), 2.0);
        assert_eq!(grid_vertical_bar_scale(320.0, 0.0), 1.0);
    }

    /// Content width, visible width and horizontal offset of the scroll area
    /// around a table of `columns` columns of 110 points, on a screen of 220,
    /// `offset` being the one the detached bar would have requested in the
    /// previous frame.
    fn scrollable_area(columns: usize, offset: Option<f32>) -> (f32, f32, f32) {
        let context = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(220.0, 180.0),
            )),
            ..Default::default()
        };
        let mut scroll = None;
        let mut output = context.run_ui(input, |ui| {
            scroll = Some(show_horizontally_scrollable_grid(ui, offset, |ui| {
                TableBuilder::new(ui)
                    .columns(TableColumn::initial(110.0).at_least(56.0), columns)
                    .auto_shrink([false, false])
                    .header(20.0, |mut header| {
                        for _ in 0..columns {
                            header.col(|ui| {
                                ui.label("Column");
                            });
                        }
                    })
                    .body(|_| {});
            }));
        });
        output.textures_delta.clear();
        let scroll = scroll.expect("the grid must be drawn");
        (
            scroll.content_size.x,
            scroll.inner_rect.width(),
            scroll.state.offset.x,
        )
    }

    /// Columns wider than the screen overflow in the scroll area; a single narrow
    /// column does not (witness: the measure does tell the two cases apart).
    #[test]
    fn the_grid_overflows_horizontally_in_its_scrollable_area() {
        let (content, visible, _) = scrollable_area(5, None);
        assert!(
            content > visible,
            "the columns must be able to scroll in the visible rectangle: {content:?} > {visible:?}",
        );
        let (content, visible, _) = scrollable_area(1, None);
        assert!(
            content <= visible + 0.5,
            "witness: one column fits on the screen: {content:?} <= {visible:?}",
        );
    }

    /// The offset requested by the detached horizontal bar (under the sheet tabs)
    /// is applied to the grid; without overflow, there is nothing to offset
    /// (witness: the offset read is not a mere echo of the request).
    #[test]
    fn the_offset_of_the_moved_bar_applies_to_the_grid() {
        let (_, _, offset) = scrollable_area(5, Some(60.0));
        assert!(
            (offset - 60.0).abs() < 0.5,
            "the grid must scroll by 60 points: {offset}"
        );
        let (_, _, offset) = scrollable_area(1, Some(60.0));
        assert!(
            offset.abs() < 0.5,
            "witness: nothing to scroll, offset {offset}"
        );
    }
    /// A right-click inside a column range keeps it. A type change targets each of
    /// these columns once, without touching the neighbor.
    #[test]
    fn the_type_in_the_letters_menu_targets_the_whole_selection() {
        let mut bench = Bench::new();
        bench.state.select_column(bench.sheet, bench.columns[0]);
        bench
            .state
            .extend_column_selection(bench.sheet, bench.columns[1]);
        bench.state.dispatch(Action::SetColumnType {
            column: bench.columns[0],
            column_type: ColumnType::Mixed,
        });
        bench.image(Vec::new());
        let letter = format!("B({})", role_label(ColumnRole::Y));
        bench.click_button(&letter, egui::PointerButton::Secondary);
        assert!(bench
            .state
            .is_column_selected(bench.sheet, bench.columns[0]));
        assert!(bench
            .state
            .is_column_selected(bench.sheet, bench.columns[1]));
        assert!(
            !bench
                .state
                .is_column_selected(bench.sheet, bench.columns[2]),
            "witness: the neighbor is not selected"
        );
        assert!(bench.center_of(&type_label(ColumnType::Numeric)).is_none());
        bench.click_on(&tr!("column-type"));
        let position = bench
            .center_of(&type_label(ColumnType::Numeric))
            .expect("type menu open");
        bench.image(vec![egui::Event::PointerMoved(position)]);
        for pressed in [true, false] {
            bench.image(vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
        assert_eq!(
            bench.actions,
            vec![Action::SetColumnsType {
                columns: bench.columns[..2].to_vec(),
                column_type: ColumnType::Numeric,
            }],
            "a single action, on the two selected columns, not the neighbor"
        );
    }

    // --- Multiple selection (#159) ------------------------------------------

    fn letter(bench: &Bench, index: usize) -> String {
        let role = bench.state.column_meta(bench.columns[index]).role;
        format!("{}({})", column_letter(index), role_label(role))
    }

    #[test]
    fn ctrl_click_on_the_letters_places_a_non_contiguous_selection() {
        let mut bench = Bench::new();
        let (a, c) = (letter(&bench, 0), letter(&bench, 2));
        bench.click_on(&a);
        bench.click_with(&c, egui::Modifiers::COMMAND);
        let sheet = bench.sheet;
        assert!(bench.state.is_column_selected(sheet, bench.columns[0]));
        assert!(!bench.state.is_column_selected(sheet, bench.columns[1]));
        assert!(bench.state.is_column_selected(sheet, bench.columns[2]));
        // A second Ctrl+click removes the column.
        bench.click_with(&c, egui::Modifiers::COMMAND);
        assert!(!bench.state.is_column_selected(sheet, bench.columns[2]));
        assert!(bench.state.is_column_selected(sheet, bench.columns[0]));
        // A simple click replaces everything.
        bench.click_with(&c, egui::Modifiers::COMMAND);
        bench.click_on(&letter(&bench, 1));
        assert!(!bench.state.is_column_selected(sheet, bench.columns[0]));
        assert!(bench.state.is_column_selected(sheet, bench.columns[1]));
    }

    #[test]
    fn shift_click_replaces_and_ctrl_shift_click_adds_a_range() {
        let mut bench = Bench::new();
        let sheet = bench.sheet;
        let (a, b, c) = (letter(&bench, 0), letter(&bench, 1), letter(&bench, 2));
        bench.click_on(&a);
        bench.click_with(&b, egui::Modifiers::COMMAND);
        // Shift+click: anchor (B) → C range, A disappears.
        bench.click_with(&c, egui::Modifiers::SHIFT);
        assert!(!bench.state.is_column_selected(sheet, bench.columns[0]));
        assert!(bench.state.is_column_selected(sheet, bench.columns[1]));
        assert!(bench.state.is_column_selected(sheet, bench.columns[2]));
        // Ctrl+Shift+click: adds it to the columns already placed.
        bench.click_on(&a);
        bench.click_with(&c, egui::Modifiers::COMMAND);
        bench.click_with(&b, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT);
        for column in &bench.columns {
            assert!(bench.state.is_column_selected(sheet, *column));
        }
    }

    #[test]
    fn the_corner_box_selects_all_then_deselects() {
        let mut bench = Bench::new();
        let sheet = bench.sheet;
        bench.click_on("#");
        assert!(bench.state.is_all_selected(sheet));
        bench.click_on("#");
        assert!(bench.state.selection(sheet).is_none());
    }

    #[test]
    fn ctrl_a_in_the_grid_pushes_select_all_and_escape_deselects() {
        let mut bench = Bench::new();
        bench.click_on("101");
        bench.image(vec![
            egui::Event::ModifiersChanged(egui::Modifiers::COMMAND),
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            },
        ]);
        assert!(bench
            .actions
            .contains(&Action::SelectAll { sheet: bench.sheet }));
        bench.image(vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)]);
        bench.press_key(egui::Key::Escape, egui::Modifiers::NONE);
        assert!(bench.state.selection(bench.sheet).is_none());
    }

    /// #406: a modal window painted after the workbook gets the Esc, the typing
    /// and the other keys: the grid behind stands aside. Here the real Formula
    /// Functions window. Witness: remove `!modal_shown` from the grid guard.
    #[test]
    fn escape_closes_the_formula_functions_window_over_a_workbook() {
        let mut bench = Bench::new();
        bench.click_on("101");
        bench.state.dispatch(Action::OpenFormulaFunctions);
        bench.modal = Some(TestModal::FormulaFunctions);
        bench.image(Vec::new());
        bench.image(Vec::new());
        bench.seen.clear();
        bench.press_key(egui::Key::Escape, egui::Modifiers::NONE);
        assert!(bench.seen.contains(&Action::CloseFormulaFunctions));
        assert!(
            bench.state.selection(bench.sheet).is_some(),
            "the grid behind keeps its selection"
        );
    }

    /// #406: typing under a modal does not start a cell edit behind it.
    #[test]
    fn typing_under_a_modal_does_not_edit_the_cell_behind() {
        let mut bench = Bench::new();
        bench.click_on("101");
        bench.state.dispatch(Action::OpenFormulaFunctions);
        bench.modal = Some(TestModal::FormulaFunctions);
        bench.image(Vec::new());
        bench.image(Vec::new());
        bench.image(vec![egui::Event::Text("7".to_string())]);
        bench.image(Vec::new());
        assert!(bench.state.editing().is_none());
    }

    /// #406, S-F7: Esc closes every nature of modal window over a workbook the
    /// grid had focused: twice when the window put the focus in a field (the
    /// first Esc leaves the field), once otherwise.
    #[test]
    fn escape_cancels_every_modal_nature_over_a_focused_workbook() {
        for (modal, escapes) in [
            (TestModal::Settings, 2),
            (TestModal::Draft, 2),
            (TestModal::Wizard, 2),
            (TestModal::LiveSettings, 2),
            (TestModal::ReadOnly, 1),
            (TestModal::Confirm, 1),
        ] {
            let mut bench = Bench::new();
            bench.click_on("101");
            bench.modal = Some(modal);
            for _ in 0..4 {
                bench.image(Vec::new());
            }
            for pressed in 1..=escapes {
                assert_eq!(
                    bench.cancels, 0,
                    "{modal:?}: not cancelled before Esc {pressed}"
                );
                bench.press_key(egui::Key::Escape, egui::Modifiers::NONE);
            }
            assert!(bench.cancels >= 1, "{modal:?}: Esc x{escapes} cancels");
            assert!(
                bench.state.selection(bench.sheet).is_some(),
                "{modal:?}: the grid keeps its selection"
            );
        }
    }

    /// #406: the grid answers again once the modal has been gone for two
    /// frames (the one-frame lag after a close is intended, no more).
    /// Witness: make `modal_shown` ignore the frame number.
    #[test]
    fn escape_deselects_again_two_frames_after_the_modal_closed() {
        let mut bench = Bench::new();
        bench.click_on("101");
        bench.modal = Some(TestModal::ReadOnly);
        bench.image(Vec::new());
        bench.image(Vec::new());
        bench.modal = None;
        bench.image(Vec::new());
        bench.image(Vec::new());
        bench.press_key(egui::Key::Escape, egui::Modifiers::NONE);
        assert!(bench.state.selection(bench.sheet).is_none());
    }

    #[test]
    fn ctrl_click_on_cells_and_rows_composes_a_multiple_selection() {
        let mut bench = Bench::new();
        let sheet = bench.sheet;
        bench.click_on("101");
        bench.click_with("303", egui::Modifiers::COMMAND);
        assert!(bench.state.is_selected(sheet, bench.columns[0], 0));
        assert!(bench.state.is_selected(sheet, bench.columns[2], 2));
        assert!(!bench.state.is_selected(sheet, bench.columns[1], 1));
        // Ctrl+click on an already selected cell removes it.
        bench.click_with("101", egui::Modifiers::COMMAND);
        assert!(!bench.state.is_selected(sheet, bench.columns[0], 0));
        assert!(bench.state.is_selected(sheet, bench.columns[2], 2));
        // The row numbers: a single kind at a time.
        bench.click_on("1");
        bench.click_with("3", egui::Modifiers::COMMAND);
        assert!(bench.state.is_row_selected(sheet, 0));
        assert!(!bench.state.is_row_selected(sheet, 1));
        assert!(bench.state.is_row_selected(sheet, 2));
        assert!(!bench.state.is_selected(sheet, bench.columns[2], 1));
    }

    // --- Column drag to a graph (#160) ---------------------------------------

    /// Right edge of the block: a point 1 pt inside the last column (the grip
    /// bands extend 3 pt on each side).
    fn right_edge(bench: &Bench, last: usize, y: f32) -> egui::Pos2 {
        let letter = letter(bench, last);
        let center = bench.center_of(&letter).expect("letter painted");
        egui::pos2(center.x + DEFAULT_COLUMN_WIDTH / 2.0 - 1.0, y)
    }

    fn payload_set(bench: &Bench) -> bool {
        egui::DragAndDrop::has_payload_of_type::<ColumnDragPayload>(&bench.context)
    }

    /// Presses at `start` then drags 30 pt to the right.
    fn drag_from(bench: &mut Bench, start: egui::Pos2) {
        bench.image(vec![egui::Event::PointerMoved(start)]);
        bench.image(vec![egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        for step in 1..=3 {
            bench.image(vec![egui::Event::PointerMoved(
                start + egui::vec2(10.0 * step as f32, 0.0),
            )]);
        }
    }

    #[test]
    fn grabbing_the_edge_of_a_column_block_carries_the_whole_selection() {
        let mut bench = Bench::new();
        bench.state.select_column(bench.sheet, bench.columns[0]);
        bench
            .state
            .extend_column_selection(bench.sheet, bench.columns[1]);
        bench.image(Vec::new());
        let y = bench.center_of("101").expect("cell painted").y;
        let start = right_edge(&bench, 1, y);

        drag_from(&mut bench, start);

        assert!(payload_set(&bench), "the block edge starts the drag");
        let payload =
            egui::DragAndDrop::payload::<ColumnDragPayload>(&bench.context).expect("payload set");
        assert_eq!(payload.sheet, bench.sheet);
        assert_eq!(payload.columns, bench.columns[..2]);
    }

    #[test]
    fn escape_cancels_the_column_drag() {
        let mut bench = Bench::new();
        bench.state.select_column(bench.sheet, bench.columns[0]);
        bench.image(Vec::new());
        let y = bench.center_of("101").unwrap().y;
        let start = right_edge(&bench, 0, y);
        drag_from(&mut bench, start);
        assert!(payload_set(&bench));

        bench.image(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);

        assert!(!payload_set(&bench));
    }

    #[test]
    fn cells_or_the_middle_of_a_column_do_not_start_the_drag() {
        // A selection of cells does not offer the gesture, even at the edge.
        let mut bench = Bench::new();
        bench.click_on("101");
        let y = bench.center_of("101").unwrap().y;
        let start = right_edge(&bench, 0, y);
        drag_from(&mut bench, start);
        assert!(!payload_set(&bench), "selected cells");

        // A whole column, but far from its edge: the cell drag stays the
        // selection-extending one.
        let mut bench = Bench::new();
        bench.state.select_column(bench.sheet, bench.columns[0]);
        bench.image(Vec::new());
        let middle = bench.center_of("201").expect("cell painted");
        drag_from(&mut bench, middle);
        assert!(!payload_set(&bench), "column middle");
        assert!(
            bench.state.is_selected(bench.sheet, bench.columns[1], 0)
                || bench.state.selection(bench.sheet).is_some(),
            "the drag extended or placed a cell selection"
        );
    }

    #[test]
    fn the_letter_row_keeps_resizing() {
        let mut bench = Bench::new();
        bench.state.select_column(bench.sheet, bench.columns[0]);
        bench.image(Vec::new());
        let letter = bench.center_of(&letter(&bench, 0)).expect("letter painted");
        let start = right_edge(&bench, 0, letter.y);
        drag_from(&mut bench, start);
        assert!(
            !payload_set(&bench),
            "the edge in the letter row resizes, it does not grab"
        );
    }

    // --- Column width by identifier (#325) ------------------------

    /// The letter of the visible column of rank `index` (the list changes with
    /// additions and deletions, `bench.columns` does not).
    fn visible_letter(bench: &Bench, index: usize) -> String {
        let (id, _, _) = bench.state.visible_columns(bench.sheet)[index].clone();
        format!(
            "{}({})",
            column_letter(index),
            role_label(bench.state.column_meta(id).role)
        )
    }

    /// Painted width of the visible column of rank `index`: the fill of its
    /// letter, which fills the whole cell.
    fn painted_width(bench: &Bench, index: usize) -> f32 {
        let center = bench
            .center_of(&visible_letter(bench, index))
            .expect("letter painted");
        bench
            .last_frame
            .as_ref()
            .unwrap()
            .shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Rect(rect)
                    if (rect.rect.height() - HEADER_SUBROW_HEIGHT).abs() < 0.5
                        && rect.rect.contains(center) =>
                {
                    Some(rect.rect.width())
                }
                _ => None,
            })
            .expect("fill of the letter")
    }

    fn painted_widths(bench: &Bench, count: usize) -> Vec<f32> {
        (0..count).map(|i| painted_width(bench, i)).collect()
    }

    fn let_frames_pass(bench: &mut Bench) {
        for _ in 0..3 {
            bench.image(Vec::new());
        }
    }

    /// Applies the actions of the last frame to the state, like the application.
    fn apply(bench: &mut Bench) {
        for action in std::mem::take(&mut bench.actions) {
            bench.state.dispatch(action);
        }
    }

    fn seen_widths(bench: &Bench) -> Vec<Action> {
        bench
            .actions
            .iter()
            .filter(|action| matches!(action, Action::SetColumnWidth { .. }))
            .cloned()
            .collect()
    }

    /// Resizes the visible column `index` by `delta` points by dragging its right
    /// edge, as with the mouse; applies the actions of each frame. Returns the
    /// `SetColumnWidth`s seen **during** the drag, then those of the release frame.
    fn resize(bench: &mut Bench, index: usize, delta: f32) -> (Vec<Action>, Vec<Action>) {
        let center = bench
            .center_of(&visible_letter(bench, index))
            .expect("letter painted");
        let edge = center.x
            + painted_width(bench, index) / 2.0
            + bench.context.global_style().spacing.item_spacing.x / 2.0;
        let start = egui::pos2(edge, center.y);
        let mut during = Vec::new();
        bench.image(vec![egui::Event::PointerMoved(start)]);
        bench.image(vec![egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        during.extend(seen_widths(bench));
        for step in 1..=3 {
            let x = start.x + delta * step as f32 / 3.0;
            bench.image(vec![egui::Event::PointerMoved(egui::pos2(x, start.y))]);
            during.extend(seen_widths(bench));
            apply(bench);
        }
        bench.image(vec![egui::Event::PointerButton {
            pos: egui::pos2(start.x + delta, start.y),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        let end = seen_widths(bench);
        apply(bench);
        let_frames_pass(bench);
        (during, end)
    }

    fn nearly_equal(a: f32, b: f32) -> bool {
        (a - b).abs() <= 0.6
    }

    /// The dragged edge of a column pushes its width only on release, and it is the
    /// painted width.
    #[test]
    fn a_manual_resize_sends_the_width_at_the_end_only() {
        let mut bench = Bench::new();
        let before = painted_width(&bench, 1);
        let (during, end) = resize(&mut bench, 1, 30.0);
        assert!(during.is_empty(), "nothing during the drag: {during:?}");
        let [Action::SetColumnWidth { column, width }] = end.as_slice() else {
            panic!("one width on release, saw {end:?}");
        };
        assert_eq!(*column, bench.columns[1]);
        assert!(
            nearly_equal(*width, before + 30.0),
            "{width} for {}",
            before + 30.0
        );
        assert_eq!(bench.state.column_width(bench.columns[1]), Some(*width));
    }

    /// Open the worksheet and paint it: no width sent, the project is not marked
    /// modified. Witness: pushing the rendered width on every frame.
    #[test]
    fn opening_the_worksheet_sends_no_width() {
        let mut bench = Bench::new();
        let_frames_pass(&mut bench);
        assert_eq!(bench.widths_sent, 0);
        let actor = bench._session.handle().snapshot().unwrap();
        assert!(actor.column_widths.is_empty());
        assert_eq!(actor.geometry_revision, 0, "the project is not modified");
    }

    /// A width set on B, then Add Column: A, B and C keep theirs, the new one takes
    /// the default width. Witness: storage by position.
    #[test]
    fn each_column_width_survives_adding_a_column() {
        let mut bench = Bench::new();
        resize(&mut bench, 1, 30.0);
        let before = painted_widths(&bench, 3);
        assert!(before[1] > before[0] + 20.0, "B is widened: {before:?}");

        bench
            .state
            .dispatch(Action::AddColumn { sheet: bench.sheet });
        let_frames_pass(&mut bench);

        let after = painted_widths(&bench, 4);
        for i in 0..3 {
            assert!(
                nearly_equal(after[i], before[i]),
                "column {i}: {after:?} / {before:?}"
            );
        }
        assert!(
            nearly_equal(after[3], DEFAULT_COLUMN_WIDTH),
            "the new column takes the default width: {after:?}"
        );
    }

    /// Delete B then Ctrl+Z: B and its neighbors get their width back; in between,
    /// A and C keep theirs, even when a new column brings back the same number of
    /// columns (the list, for its part, has changed).
    #[test]
    fn delete_then_undo_gives_the_column_and_its_neighbors_their_width_back() {
        let mut bench = Bench::new();
        resize(&mut bench, 0, 12.0);
        resize(&mut bench, 1, 30.0);
        let before = painted_widths(&bench, 3);

        bench.state.select_column(bench.sheet, bench.columns[1]);
        bench
            .state
            .dispatch(Action::DeleteSelectedCells { sheet: bench.sheet });
        let_frames_pass(&mut bench);
        let after = painted_widths(&bench, 2);
        assert!(
            nearly_equal(after[0], before[0]),
            "A: {after:?} / {before:?}"
        );
        assert!(
            nearly_equal(after[1], before[2]),
            "C: {after:?} / {before:?}"
        );

        // A new column: two columns become three, the same quantity as before the
        // deletion, but not the same list.
        bench
            .state
            .dispatch(Action::AddColumn { sheet: bench.sheet });
        let_frames_pass(&mut bench);
        let with_new_column = painted_widths(&bench, 3);
        assert!(
            nearly_equal(with_new_column[0], before[0])
                && nearly_equal(with_new_column[1], before[2]),
            "{with_new_column:?}"
        );
        assert!(
            nearly_equal(with_new_column[2], DEFAULT_COLUMN_WIDTH),
            "{with_new_column:?}"
        );

        bench.state.dispatch(Action::Undo);
        bench.state.dispatch(Action::Undo);
        let_frames_pass(&mut bench);
        let restored = painted_widths(&bench, 3);
        for i in 0..3 {
            assert!(
                nearly_equal(restored[i], before[i]),
                "column {i}: {restored:?} / {before:?}"
            );
        }
    }

    // --- Reset to the default width (#330) ------------------------------

    /// Painted width of the gutter: the fill of the "#".
    fn painted_gutter_width(bench: &Bench) -> f32 {
        let center = bench.center_of("#").expect("'#' painted");
        bench
            .last_frame
            .as_ref()
            .unwrap()
            .shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Rect(rect)
                    if (rect.rect.height() - HEADER_SUBROW_HEIGHT).abs() < 0.5
                        && rect.rect.contains(center) =>
                {
                    Some(rect.rect.width())
                }
                _ => None,
            })
            .expect("fill of '#'")
    }

    /// Drags the right edge of the gutter by `delta` points, as with the mouse.
    fn resize_gutter(bench: &mut Bench, delta: f32) {
        let center = bench.center_of("#").expect("'#' painted");
        let edge = center.x
            + painted_gutter_width(bench) / 2.0
            + bench.context.global_style().spacing.item_spacing.x / 2.0;
        let start = egui::pos2(edge, center.y);
        bench.image(vec![egui::Event::PointerMoved(start)]);
        bench.image(vec![egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        for step in 1..=3 {
            let x = start.x + delta * step as f32 / 3.0;
            bench.image(vec![egui::Event::PointerMoved(egui::pos2(x, start.y))]);
            apply(bench);
        }
        bench.image(vec![egui::Event::PointerButton {
            pos: egui::pos2(start.x + delta, start.y),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        apply(bench);
        let_frames_pass(bench);
    }

    /// B widened by 40 then reset: it returns to 54, A and C keep theirs. Witness:
    /// leaving the stored widths out of the layout key (`TableColumn::initial` is
    /// then ignored, B stays wide).
    #[test]
    fn resetting_a_column_to_default_does_not_touch_the_others() {
        let mut bench = Bench::new();
        resize(&mut bench, 0, 12.0);
        resize(&mut bench, 1, 40.0);
        resize(&mut bench, 2, 20.0);
        let before = painted_widths(&bench, 3);
        assert!(before[1] > 90.0, "B is widened: {before:?}");

        bench.state.dispatch(Action::ResetColumnWidths {
            columns: vec![bench.columns[1]],
        });
        let_frames_pass(&mut bench);

        let after = painted_widths(&bench, 3);
        assert!(nearly_equal(after[1], DEFAULT_COLUMN_WIDTH), "B: {after:?}");
        assert!(
            nearly_equal(after[0], before[0]),
            "A: {after:?} / {before:?}"
        );
        assert!(
            nearly_equal(after[2], before[2]),
            "C: {after:?} / {before:?}"
        );
    }

    /// Widening a column by hand does not bring the gutter back to 90: it keeps
    /// the width it had been given. Witness: not remembering the rendered width of
    /// the gutter.
    #[test]
    fn a_column_drag_keeps_the_gutter_width() {
        let mut bench = Bench::new();
        resize_gutter(&mut bench, 30.0);
        let wide = painted_gutter_width(&bench);
        assert!(
            nearly_equal(wide, ROW_NUMBER_COLUMN_WIDTH + 30.0),
            "dragged gutter: {wide}"
        );
        resize(&mut bench, 1, 40.0);
        assert!(
            nearly_equal(painted_gutter_width(&bench), wide),
            "gutter after the drag: {}",
            painted_gutter_width(&bench)
        );
        // Resetting a column does not touch it either.
        bench.state.dispatch(Action::ResetColumnWidths {
            columns: vec![bench.columns[1]],
        });
        let_frames_pass(&mut bench);
        assert!(nearly_equal(painted_gutter_width(&bench), wide));
        // The whole worksheet brings it back to 90, without recording it.
        resize(&mut bench, 1, 40.0);
        forget_gutter_width(&bench.context, bench.sheet);
        bench.state.dispatch(Action::ResetColumnWidths {
            columns: bench.columns.clone(),
        });
        let_frames_pass(&mut bench);
        assert!(nearly_equal(
            painted_gutter_width(&bench),
            ROW_NUMBER_COLUMN_WIDTH
        ));
        assert!(bench.state.snapshot().column_widths.is_empty());
    }

    /// Opens the right-click of the letter `index` and returns the painted menu.
    fn open_letter_menu(bench: &mut Bench, index: usize) -> Vec<MenuEntry> {
        bench.context.enable_accesskit();
        bench.image(Vec::new());
        bench.click_button(
            &visible_letter(bench, index),
            egui::PointerButton::Secondary,
        );
        read_menu(bench)
    }

    /// The entry follows Role ▸; grayed out as long as no targeted column has a
    /// stored width, active afterwards, and pushes the action on the target.
    /// Proven witness: `add_enabled(true, …)` (red).
    #[test]
    fn the_reset_entry_follows_role_and_is_grayed_out_without_a_width() {
        let mut bench = Bench::new();
        let menu = open_letter_menu(&mut bench, 1);
        let (_, active, _) = menu_entry(&menu, &tr!("action-reset-column-widths"));
        assert!(!active, "nothing to reset: grayed out");
        let y = |label: String| menu_entry(&menu, &label).2.y;
        assert!(
            y(tr!("column-role")) < y(tr!("action-reset-column-widths")),
            "{menu:#?}"
        );

        let mut bench = Bench::new();
        resize(&mut bench, 1, 30.0);
        let b = bench.columns[1];
        let menu = open_letter_menu(&mut bench, 1);
        assert!(menu_entry(&menu, &tr!("action-reset-column-widths")).1);
        bench.click_on(&tr!("action-reset-column-widths"));
        assert!(
            bench
                .seen
                .contains(&Action::ResetColumnWidths { columns: vec![b] }),
            "{:?}",
            bench.seen
        );
    }

    /// A selection of columns is handled as a whole: A and B, only one of which has
    /// a width, go together. Proven witness: targeting only the clicked column
    /// (red).
    #[test]
    fn the_reset_handles_the_whole_selection() {
        let mut bench = Bench::new();
        resize(&mut bench, 1, 30.0);
        let (sheet, a, b) = (bench.sheet, bench.columns[0], bench.columns[1]);
        bench.state.select_column(sheet, a);
        bench.state.toggle_column_selection(sheet, b);
        bench.image(Vec::new());
        open_letter_menu(&mut bench, 0);
        bench.click_on(&tr!("action-reset-column-widths"));
        assert!(
            bench.seen.contains(&Action::ResetColumnWidths {
                columns: vec![a, b]
            }),
            "{:?}",
            bench.seen
        );
    }

    /// Right-click on the worksheet's tab: the worksheet entry, grayed out without
    /// a width, active with one, pushes all the columns of the worksheet. Proven
    /// witnesses: an always active entry, or a single targeted column (red).
    #[test]
    fn the_tab_right_click_resets_the_whole_sheet() {
        let mut bench = Bench::new();
        bench.context.enable_accesskit();
        let name = bench.state.worksheets_of(bench.book)[0].1.clone();
        bench.image(Vec::new());
        bench.click_button(&name, egui::PointerButton::Secondary);
        let menu = read_menu(&bench);
        assert!(!menu_entry(&menu, &tr!("action-reset-sheet-column-widths")).1);

        let mut bench = Bench::new();
        resize(&mut bench, 1, 30.0);
        bench.context.enable_accesskit();
        bench.image(Vec::new());
        bench.click_button(&name, egui::PointerButton::Secondary);
        let menu = read_menu(&bench);
        assert!(menu_entry(&menu, &tr!("action-reset-sheet-column-widths")).1);
        bench.click_on(&tr!("action-reset-sheet-column-widths"));
        assert!(
            bench.seen.contains(&Action::ResetColumnWidths {
                columns: bench.columns.clone()
            }),
            "{:?}",
            bench.seen
        );
    }

    // --- Move ▸ submenu of the column letters (#319) -----------------------

    /// An entry of the painted menu, read through accesskit: `(label, active, center)`.
    type MenuEntry = (String, bool, egui::Pos2);

    fn read_menu(bench: &Bench) -> Vec<MenuEntry> {
        let update = bench
            .last_frame
            .as_ref()
            .unwrap()
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("accesskit enabled by `open_move`");
        update
            .nodes
            .iter()
            .filter_map(|(_, node)| {
                let label = node.label()?.to_string();
                let label = label.strip_suffix(" ⏵").unwrap_or(&label).to_string();
                let bounds = node.bounds()?;
                Some((
                    label,
                    !node.is_disabled(),
                    egui::pos2(
                        ((bounds.x0 + bounds.x1) / 2.0) as f32,
                        ((bounds.y0 + bounds.y1) / 2.0) as f32,
                    ),
                ))
            })
            .collect()
    }

    fn menu_entry<'a>(menu: &'a [MenuEntry], label: &str) -> &'a MenuEntry {
        // A shortcut is written after the label ("Copy Ctrl+C").
        menu.iter()
            .find(|(text, _, _)| {
                text == label
                    || text
                        .strip_prefix(label)
                        .is_some_and(|rest| rest.starts_with(" Ctrl+"))
            })
            .unwrap_or_else(|| panic!("entry {label:?} missing from {menu:#?}"))
    }

    /// Labels of the four entries of `Move ▸`, in the menu's order.
    fn move_entries() -> [String; 4] {
        [
            tr!("action-move-columns-first"),
            tr!("action-move-columns-left"),
            tr!("action-move-columns-right"),
            tr!("action-move-columns-last"),
        ]
    }

    /// Right-click on the letter of the visible column `index`, then opening of
    /// `Move ▸`: returns the painted menu (main and submenu).
    fn open_move(bench: &mut Bench, index: usize) -> Vec<MenuEntry> {
        bench.context.enable_accesskit();
        bench.image(Vec::new());
        bench.click_button(
            &visible_letter(bench, index),
            egui::PointerButton::Secondary,
        );
        bench.click_on(&tr!("column-move"));
        read_menu(bench)
    }

    /// `Move ▸` between the clipboard and `Type ▸`, with its four entries in the
    /// submenu's order.
    #[test]
    fn move_sits_between_the_clipboard_and_type_with_four_entries() {
        let mut bench = Bench::new();
        let menu = open_move(&mut bench, 1);
        let y = |label: String| menu_entry(&menu, &label).2.y;
        assert!(y(tr!("action-copy")) < y(tr!("column-move")), "{menu:#?}");
        assert!(y(tr!("action-paste")) < y(tr!("column-move")), "{menu:#?}");
        assert!(y(tr!("column-move")) < y(tr!("column-type")), "{menu:#?}");
        assert!(y(tr!("column-type")) < y(tr!("column-role")), "{menu:#?}");
        let ys: Vec<f32> = move_entries().into_iter().map(y).collect();
        assert!(ys.windows(2).all(|pair| pair[0] < pair[1]), "{ys:?}");
        assert_eq!(
            menu.iter()
                .filter(|(text, _, _)| text.starts_with("Move"))
                .count(),
            5,
            "Move and its four entries, nothing more: {menu:#?}"
        );
    }

    /// Graying out: all or nothing, by the same computation as the execution.
    #[test]
    fn move_entries_are_grayed_out_when_the_gesture_would_change_nothing() {
        let actives = |menu: &[MenuEntry]| -> Vec<bool> {
            move_entries()
                .iter()
                .map(|label| menu_entry(menu, label).1)
                .collect()
        };
        // A alone: already first, neither First nor Left.
        let mut bench = Bench::new();
        let menu = open_move(&mut bench, 0);
        assert_eq!(actives(&menu), [false, false, true, true], "A alone");
        // B alone: everything is possible.
        let mut bench = Bench::new();
        let menu = open_move(&mut bench, 1);
        assert_eq!(actives(&menu), [true, true, true, true], "B alone");
        // C alone: already last.
        let mut bench = Bench::new();
        let menu = open_move(&mut bench, 2);
        assert_eq!(actives(&menu), [true, true, false, false], "C alone");
        // A and C (non-contiguous): A blocks Left, C blocks Right, all or nothing.
        let mut bench = Bench::new();
        let (sheet, a, c) = (bench.sheet, bench.columns[0], bench.columns[2]);
        bench.state.select_column(sheet, a);
        bench.state.toggle_column_selection(sheet, c);
        bench.image(Vec::new());
        let menu = open_move(&mut bench, 0);
        assert_eq!(actives(&menu), [true, false, false, true], "A and C");
        // Whole rows: all the columns, nothing moves.
        let mut bench = Bench::new();
        bench.context.enable_accesskit();
        bench.state.select_row(bench.sheet, 1);
        bench.image(Vec::new());
        bench.click_button("102", egui::PointerButton::Secondary);
        bench.click_on(&tr!("column-move"));
        let menu = read_menu(&bench);
        assert_eq!(actives(&menu), [false; 4], "whole rows");
    }

    /// The active entry pushes `MoveColumns` on the selection, the grayed one does not.
    #[test]
    fn a_move_entry_pushes_the_action_on_the_selection() {
        let mut bench = Bench::new();
        let (sheet, b) = (bench.sheet, bench.columns[1]);
        open_move(&mut bench, 1);
        bench.click_on(&tr!("action-move-columns-left"));
        assert_eq!(
            bench
                .seen
                .iter()
                .filter(|action| matches!(action, Action::MoveColumns { .. }))
                .cloned()
                .collect::<Vec<_>>(),
            vec![Action::MoveColumns {
                sheet,
                columns: vec![b],
                to: ColumnMove::Left
            }]
        );
        // A grayed-out entry pushes nothing.
        let mut bench = Bench::new();
        open_move(&mut bench, 0);
        bench.click_on(&tr!("action-move-columns-left"));
        assert!(
            !bench
                .seen
                .iter()
                .any(|action| matches!(action, Action::MoveColumns { .. })),
            "{:?}",
            bench.seen
        );
    }

    /// The width follows its column: B (set) and A (set) swap places, each keeps
    /// its own. Witness: storage by position.
    #[test]
    fn the_width_follows_the_moved_column() {
        let mut bench = Bench::new();
        resize(&mut bench, 0, 12.0);
        resize(&mut bench, 1, 30.0);
        let before = painted_widths(&bench, 3);
        assert!(!nearly_equal(before[0], before[1]), "two distinct widths");

        open_move(&mut bench, 1);
        bench.click_on(&tr!("action-move-columns-left"));
        for action in std::mem::take(&mut bench.seen) {
            bench.state.dispatch(action);
        }
        let_frames_pass(&mut bench);

        let order: Vec<NodeId> = bench
            .state
            .visible_columns(bench.sheet)
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(
            order,
            vec![bench.columns[1], bench.columns[0], bench.columns[2]]
        );
        let after = painted_widths(&bench, 3);
        assert!(
            nearly_equal(after[0], before[1]),
            "B first: {after:?} / {before:?}"
        );
        assert!(
            nearly_equal(after[1], before[0]),
            "A second: {after:?} / {before:?}"
        );
        assert!(
            nearly_equal(after[2], before[2]),
            "C: {after:?} / {before:?}"
        );
    }

    // --- Row drag to a graph (#166) ------------------------------------

    fn row_payload_set(bench: &Bench) -> bool {
        egui::DragAndDrop::has_payload_of_type::<RowDragPayload>(&bench.context)
    }

    /// Gives an x' value to the first column of the bench.
    fn set_x_prime(bench: &mut Bench, text: &str) {
        bench
            .state
            .begin_header_edit(bench.columns[0], HeaderField::XPrime);
        if let Some(edit) = bench.state.header_editing_mut() {
            edit.buffer = text.to_string();
        }
        bench.state.commit_header_edit();
    }

    /// A point 1 pt above the bottom edge of row `row`, in the middle of column
    /// `column`.
    fn bottom_edge(bench: &Bench, cell: &str) -> egui::Pos2 {
        let center = bench.center_of(cell).expect("cell painted");
        egui::pos2(center.x, center.y + ROW_HEIGHT / 2.0 - 1.0)
    }

    #[test]
    fn grabbing_the_edge_of_a_row_block_carries_the_whole_selection() {
        let mut bench = Bench::new();
        set_x_prime(&mut bench, "5");
        bench.state.select_row(bench.sheet, 0);
        bench.state.extend_row_selection(bench.sheet, 1);
        bench.image(Vec::new());
        bench.image(Vec::new());
        let start = bottom_edge(&bench, "202");

        drag_from(&mut bench, start);

        assert!(row_payload_set(&bench), "the block edge starts the drag");
        assert!(!payload_set(&bench), "this is not a column drag");
        let payload =
            egui::DragAndDrop::payload::<RowDragPayload>(&bench.context).expect("payload set");
        assert_eq!(
            (payload.sheet, payload.rows.clone()),
            (bench.sheet, vec![0, 1])
        );
    }

    #[test]
    fn escape_cancels_the_row_drag() {
        let mut bench = Bench::new();
        set_x_prime(&mut bench, "5");
        bench.state.select_row(bench.sheet, 0);
        bench.image(Vec::new());
        bench.image(Vec::new());
        let start = bottom_edge(&bench, "201");
        drag_from(&mut bench, start);
        assert!(row_payload_set(&bench));

        bench.image(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);

        assert!(!row_payload_set(&bench));
    }

    #[test]
    fn without_an_x_prime_row_or_far_from_the_edge_the_row_drag_does_not_exist() {
        // Without an x' value: the gesture is not offered, even at the edge.
        let mut bench = Bench::new();
        bench.state.select_row(bench.sheet, 0);
        bench.image(Vec::new());
        bench.image(Vec::new());
        let start = bottom_edge(&bench, "201");
        drag_from(&mut bench, start);
        assert!(!row_payload_set(&bench), "without x'");

        // With x', but in the middle of the row: it is the selection extension.
        let mut bench = Bench::new();
        set_x_prime(&mut bench, "5");
        bench.state.select_row(bench.sheet, 0);
        bench.image(Vec::new());
        bench.image(Vec::new());
        let middle = bench.center_of("201").expect("cell painted");
        drag_from(&mut bench, middle);
        assert!(!row_payload_set(&bench), "row middle");
    }

    /// A fast drag: the pointer leaves the band and crosses other cells from the
    /// first movement frame. The selection extension on hover must not undo the row
    /// selection before the drag starts.
    #[test]
    fn a_fast_row_drag_keeps_the_selection_and_carries_the_rows() {
        let mut bench = Bench::new();
        set_x_prime(&mut bench, "5");
        bench.state.select_row(bench.sheet, 0);
        bench.image(Vec::new());
        bench.image(Vec::new());
        let start = bottom_edge(&bench, "201");
        bench.image(vec![egui::Event::PointerMoved(start)]);
        bench.image(vec![egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        for step in 1..=4 {
            bench.image(vec![egui::Event::PointerMoved(
                start + egui::vec2(60.0 * step as f32, 25.0 * step as f32),
            )]);
        }
        assert!(row_payload_set(&bench), "the fast drag started");
    }

    /// The user's complete gesture: click on the row number, then grab the bottom
    /// edge of the selected row.
    #[test]
    fn click_on_the_row_number_then_grab_the_edge() {
        let mut bench = Bench::new();
        set_x_prime(&mut bench, "5");
        bench.click_on("2");
        assert!(bench.state.is_row_selected(bench.sheet, 1));
        bench.image(Vec::new());
        let start = bottom_edge(&bench, "202");
        drag_from(&mut bench, start);
        assert!(row_payload_set(&bench));
    }

    /// During the row drag, hovering other cells does not replace the row
    /// selection by a cell selection.
    #[test]
    fn the_row_drag_keeps_the_row_selection() {
        let mut bench = Bench::new();
        set_x_prime(&mut bench, "5");
        bench.state.select_row(bench.sheet, 1);
        bench.image(Vec::new());
        bench.image(Vec::new());
        let start = bottom_edge(&bench, "202");
        bench.image(vec![egui::Event::PointerMoved(start)]);
        bench.image(vec![egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        let target = bench.center_of("303").expect("cell painted");
        for step in 1..=4 {
            let t = step as f32 / 4.0;
            bench.image(vec![egui::Event::PointerMoved(start.lerp(target, t))]);
            bench.image(Vec::new());
        }
        assert!(row_payload_set(&bench));
        assert!(
            bench.state.is_row_selected(bench.sheet, 1),
            "the row stays selected during the drag"
        );
    }

    /// Same guarantee for a column drag.
    #[test]
    fn the_column_drag_keeps_the_column_selection() {
        let mut bench = Bench::new();
        bench.state.select_column(bench.sheet, bench.columns[0]);
        bench.image(Vec::new());
        let y = bench.center_of("101").expect("cell painted").y;
        let start = right_edge(&bench, 0, y);
        bench.image(vec![egui::Event::PointerMoved(start)]);
        bench.image(vec![egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        let target = bench.center_of("303").expect("cell painted");
        for step in 1..=4 {
            let t = step as f32 / 4.0;
            bench.image(vec![egui::Event::PointerMoved(start.lerp(target, t))]);
            bench.image(Vec::new());
        }
        assert!(payload_set(&bench));
        assert!(bench
            .state
            .is_column_selected(bench.sheet, bench.columns[0]));
    }

    // --- "?" help of the F(x)= cell (#404) ---------------------------------

    /// The centers of the images (icons) published through accesskit in the
    /// last frame.
    fn image_centers(bench: &Bench) -> Vec<egui::Pos2> {
        let update = bench
            .last_frame
            .as_ref()
            .unwrap()
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("accesskit enabled");
        update
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == egui::accesskit::Role::Image)
            .filter_map(|(_, node)| {
                let bounds = node.bounds()?;
                Some(egui::pos2(
                    ((bounds.x0 + bounds.x1) / 2.0) as f32,
                    ((bounds.y0 + bounds.y1) / 2.0) as f32,
                ))
            })
            .collect()
    }

    /// The icons painted while `field` of the first column is edited, with the
    /// column-formula help preference at `help`.
    fn icons_while_editing(field: HeaderField, help: bool) -> (Bench, Vec<egui::Pos2>) {
        let mut bench = Bench::new();
        bench.context.enable_accesskit();
        bench
            .context
            .all_styles_mut(|style| style.interaction.tooltip_delay = 0.0);
        bench.state.dispatch(Action::SetShowColumnFormulaHelp(help));
        let column = bench.columns[0];
        bench.state.begin_header_edit(column, field);
        for _ in 0..4 {
            bench.image(Vec::new());
        }
        assert!(bench.state.header_editing().is_some());
        let icons = image_centers(&bench);
        (bench, icons)
    }

    /// The "?" icon of the F(x)= cell is the one image that appears with the
    /// preference on, only for the Formula field.
    fn help_icon(field: HeaderField, help: bool) -> (Bench, Option<egui::Pos2>) {
        let (bench, with) = icons_while_editing(field, help);
        let (_, without) = icons_while_editing(field, false);
        let icon = with.into_iter().find(|center| {
            !without
                .iter()
                .any(|other| (*other - *center).length() < 1.0)
        });
        (bench, icon)
    }

    #[test]
    fn the_formula_cell_shows_the_help_icon_only_while_edited_and_when_the_preference_is_on() {
        let (_, icon) = help_icon(HeaderField::Formula, true);
        assert!(icon.is_some(), "edited F(x)= cell: the icon");
        // Preference off: nothing more than without it.
        let (_, off) = icons_while_editing(HeaderField::Formula, false);
        let (_, on) = icons_while_editing(HeaderField::Formula, true);
        assert_eq!(on.len(), off.len() + 1);
        // Not edited: no icon either way.
        let mut bench = Bench::new();
        bench.context.enable_accesskit();
        for _ in 0..3 {
            bench.image(Vec::new());
        }
        let idle = image_centers(&bench).len();
        assert_eq!(idle, off.len(), "the edited cell without help adds nothing");
    }

    #[test]
    fn the_other_header_fields_have_no_formula_help_icon() {
        for field in [
            HeaderField::LongName,
            HeaderField::Unit,
            HeaderField::Comment,
        ] {
            let (_, icon) = help_icon(field, true);
            assert!(icon.is_none(), "{field:?}");
        }
    }

    /// A press on the icon asks for the Formula Functions window (#402); the
    /// pointer merely resting on it asks for nothing.
    #[test]
    fn pressing_the_help_icon_opens_the_formula_functions_window() {
        let (mut bench, icon) = help_icon(HeaderField::Formula, true);
        let icon = icon.expect("the icon");
        for _ in 0..3 {
            bench.image(vec![egui::Event::PointerMoved(icon)]);
        }
        assert!(
            !bench.seen.contains(&Action::OpenFormulaFunctions),
            "hovering opens nothing"
        );
        bench.image(vec![egui::Event::PointerButton {
            pos: icon,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(bench.seen.contains(&Action::OpenFormulaFunctions));
    }

    #[test]
    fn the_help_icon_takes_its_width_from_the_field_not_from_the_cell() {
        let (bench, icon) = help_icon(HeaderField::Formula, true);
        let icon = icon.expect("the icon");
        // The icon stays inside the first column: right of its text, left of
        // the second column's first cell.
        let second = bench.center_of("201").expect("cell of the second column");
        assert!(
            icon.x < second.x - 20.0,
            "icon {icon:?}, next cell {second:?}"
        );
    }
}
