//! The workbook: [`book_contents`], the content shared by a floating **or**
//! detached [`Tab::Book`] (a single implementation: it depends only on
//! `(ui, state, actions, workbook)`, never on an `app` field), its sheet tabs
//! ([`sheet_tabs`]) and the Workbook menu ([`workbook_menu`]). The grid itself
//! lives in `grid.rs`.
//!
//! Moved out of `app.rs` as is (#120). The tests, at the end of the file, cover
//! the layout (grid, tabs, detached bars, clipping) and the workbook menus; those
//! of the keyboard and of the header are in `grid.rs`.
//!
//! [`Tab::Book`]: super::tab::Tab::Book

use eframe::egui;
use stokesforge_core::NodeId;

use crate::theme;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

use super::empty_state::empty_state;
use super::file_drop::claim_file_drop;
use super::graph_targets::{send_rows_to_graph_menu, send_to_graph_menu};
use super::grid::{
    columns_menu_contents, forget_gutter_width, grid_contents, plot_menu, plot_rows_menu,
    reset_sheet_widths_entry, show_grid_horizontal_bar, show_grid_vertical_bar,
    show_horizontally_scrollable_grid, toggle_header_row_menu,
};
use super::menu_sections::MenuSections;
use super::results_pane;
use super::shortcuts::SHORTCUT_DUPLICATE_SHEET;
use super::tab_strip::{
    tab_strip_with, TabEvent, TabOps, TabStripOutput, TabStripSide, TAB_STRIP_HEIGHT,
};

/// "Workbook" menu: the gestures on the displayed sheet, mirror of the tabs'
/// context menu and of the Ctrl+D shortcut (F2 edits the cells, never the name
/// of the sheets, #130/#143). The top menu **wraps** the same contents as the
/// right-clicks (#152): Sheet ▸, Columns ▸ and Plot ▸ name the target that the
/// right-click, for its part, already designates.
pub(super) fn workbook_menu(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &AppState,
    book: NodeId,
    sheet: NodeId,
    actions: &mut Vec<Action>,
) {
    let columns = state
        .selection(sheet)
        .map(|selection| selection.columns(state.tree(), sheet))
        .unwrap_or_default();
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        ui.menu_button(tr!("menu-sheet"), |ui| {
            sheet_menu(ui, ctx, state, book, sheet, actions);
        });
        ui.menu_button(tr!("menu-columns"), |ui| {
            columns_menu_contents(ui, state, actions, sheet, &columns);
        });
        // Whole rows on an x' sheet are plotted as rows (#166), not as the columns
        // they cross.
        if let Some(payload) = state.row_drag_payload(sheet) {
            plot_rows_menu(ui, actions, &payload);
            send_rows_to_graph_menu(ui, state.tree(), actions, &payload);
        } else {
            plot_menu(ui, actions, sheet, &columns);
            send_to_graph_menu(ui, state.tree(), actions, sheet, &columns);
        }
    });
    sections.section(ui, true, |ui| {
        toggle_header_row_menu(ui, book);
    });
}

/// Contents of the Sheet menu: Add Worksheet ─ Rename · Duplicate ─ Delete, in
/// the order of the tabs' right-click.
fn sheet_menu(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &AppState,
    book: NodeId,
    sheet: NodeId,
    actions: &mut Vec<Action>,
) {
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        if ui.button(tr!("workbook-worksheet-add")).clicked() {
            actions.push(Action::AddWorksheet { book });
            ui.close();
        }
    });
    sections.section(ui, true, |ui| {
        if ui
            .add(egui::Button::new(tr!("action-rename-sheet")))
            .on_hover_text(tr!("action-rename-sheet", "hint"))
            .clicked()
        {
            actions.push(Action::BeginRenameSheet(sheet));
            ui.close();
        }
        if ui
            .add(
                egui::Button::new(tr!("action-duplicate-sheet"))
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_DUPLICATE_SHEET)),
            )
            .on_hover_text(tr!("action-duplicate-sheet", "hint"))
            .clicked()
        {
            actions.push(Action::DuplicateSheet(sheet));
            ui.close();
        }
        reset_sheet_widths_entry(ui, state, actions, sheet);
    });
    sections.section(ui, true, |ui| {
        let can_delete = state.can_delete_sheet(sheet);
        let delete = ui
            .add_enabled(can_delete, egui::Button::new(tr!("action-delete-sheet")))
            .on_hover_text(tr!("action-delete-sheet", "hint"))
            .on_disabled_hover_text(tr!("action-delete-sheet", "disabled"));
        if delete.clicked() {
            actions.push(Action::DeleteSheet(sheet));
            ui.close();
        }
    });
}

/// Strip of a workbook's sheet tabs: [`tab_strip`](crate::app::tab_strip::tab_strip)
/// with, in addition, "Add Worksheet" on right-click.
fn sheet_tabs(
    ui: &mut egui::Ui,
    sheets: &[(NodeId, String)],
    selected: NodeId,
    on_select: impl FnMut(NodeId),
    on_add: impl FnMut(),
    ops: Option<&TabOps<NodeId>>,
    on_event: impl FnMut(TabEvent<NodeId>),
) -> TabStripOutput {
    let add_label = tr!("workbook-worksheet-add");
    tab_strip_with(
        ui,
        TabStripSide::Bottom,
        sheets,
        selected,
        on_select,
        Some(&add_label),
        on_add,
        ops,
        on_event,
    )
}

/// Shared content of a floating or detached [`Tab::Book`](crate::app::tab::Tab::Book): the grid
/// ([`grid_contents`]) scoped to the sheet chosen *in this workbook*, and the
/// sub-navigation between sheets — compact tabs at the **bottom** of the grid
/// (deliberate choice, not a preexisting convention of the crate: no other tab
/// of this kind existed to imitate). The "Insert/Delete 1 row" buttons are
/// gone: adding a row is now done by pressing Enter on the last cell of a column
/// ([`Action::CommitCellAndAdvance`]), deletion by the Delete key on the
/// selection ([`Action::DeleteSelectedCells`]).
pub(super) fn book_contents(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    book: NodeId,
) {
    claim_file_drop(ui, book);
    let sheets = state.worksheets_of(book);
    let selected = state.selected_sheet_in(book).unwrap_or(book);
    // The root viewport already consumes this gesture on the active workbook.
    // In a separate OS window, the same content receives its own keyboard.
    if ui.ctx().viewport_id() != egui::ViewportId::ROOT
        && !ui.ctx().text_edit_focused()
        && state.renaming_sheet().is_none()
        && ui.input_mut(|input| input.consume_shortcut(&SHORTCUT_DUPLICATE_SHEET))
    {
        actions.push(Action::DuplicateSheet(selected));
    }
    let mut chosen_sheet = None;
    let mut add_sheet = false;
    // The tab strip is cut from the workbook's rectangle **before** building the
    // grid: reserving a height in `allocate_ui` was not enough, its `ScrollArea`
    // could then make the consumed size grow and push the tabs out of the frame. No
    // `egui::Panel` for this split: it replaces its parent's clipping with its own
    // instead of combining it, and drew the tabs under the view when the window was
    // cut there (`show_floating_window`). Two bounded sub-`Ui`s, each clipped to
    // the intersection of its rectangle and the parent clip.
    let ctx = ui.ctx().clone();
    let area = ui.available_rect_before_wrap();
    // The right edge of the window is reserved, over its whole height, for the
    // vertical scroll bar (B10): the horizontal scroll area and the sheet tabs
    // carry only the rest, and the bar stays stuck to the window edge whatever the
    // width of the columns. The bottom is reserved for the grid's horizontal bar,
    // **under** the sheet tabs, as in a spreadsheet. From top to bottom: grid,
    // tabs, horizontal bar.
    let bar_width = theme::scroll_bar_allocated_width();
    let (main_area, vertical_bar_strip) =
        area.split_left_right_at_x(area.right() - bar_width.min(area.width()));
    // #51: the horizontal bar's strip is reserved only if the grid overflows in
    // width — the state comes from the previous frame (the strip's height does not
    // change the content's width: no oscillation).
    let horizontal_bar_present_id = ui.id().with("grid_horizontal_bar_present");
    let horizontal_bar_present = ui
        .memory(|memory| memory.data.get_temp::<bool>(horizontal_bar_present_id))
        .unwrap_or(false);
    let horizontal_bar_height = if horizontal_bar_present {
        bar_width
    } else {
        0.0
    };
    let (upper_area, horizontal_bar_strip) = main_area
        .split_top_bottom_at_y(main_area.bottom() - horizontal_bar_height.min(main_area.height()));
    let tabs_height = TAB_STRIP_HEIGHT.min(upper_area.height());
    // The Results pane (#181) sits between the grid and the tabs; with no kept
    // result its height is zero and the layout does not change.
    let results_height = results_pane::pane_height(ui, state, book)
        .min((upper_area.height() - tabs_height).max(0.0) / 2.0);
    let (grid_with_banner, lower_area) =
        upper_area.split_top_bottom_at_y(upper_area.bottom() - tabs_height - results_height);
    // The banner of a linked transposed sheet (#253) takes the top of the grid.
    let banner_height = if state.transpose_info(selected).is_some() {
        super::transpose_ui::BANNER_HEIGHT.min(grid_with_banner.height())
    } else {
        0.0
    };
    let (banner_rect, grid_rect) =
        grid_with_banner.split_top_bottom_at_y(grid_with_banner.top() + banner_height);
    let (results_rect, tabs_rect) =
        lower_area.split_top_bottom_at_y(lower_area.top() + results_height);
    // With the bar, the tabs go down by half the strip: the space under them is
    // halved and the same space appears above (#51). Without a bar, no reserve and
    // no shift.
    let tabs_rect = tabs_rect.translate(egui::vec2(0.0, horizontal_bar_height / 2.0));
    let mut bounded = |rect: egui::Rect| {
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(ui.clip_rect().intersect(rect));
        child
    };
    let mut tabs_ui = bounded(tabs_rect);
    let mut grid_ui = bounded(grid_rect);
    if banner_height > 0.0 {
        let mut banner_ui = bounded(banner_rect);
        super::transpose_ui::transpose_banner(&mut banner_ui, state, selected, actions);
    }
    if results_height > 0.0 {
        let mut results_ui = bounded(results_rect);
        results_pane::results_pane(&mut results_ui, state, actions, book, selected);
    }
    let sheet_ops = TabOps {
        renaming: state.renaming_sheet(),
        can_delete: sheets.len() > 1,
        resettable: sheets
            .iter()
            .map(|(id, _)| *id)
            .filter(|id| {
                !state
                    .columns_with_width(&state.sheet_column_ids(*id))
                    .is_empty()
            })
            .collect(),
    };
    sheet_tabs(
        &mut tabs_ui,
        &sheets,
        selected,
        |id| chosen_sheet = Some(id),
        || add_sheet = true,
        Some(&sheet_ops),
        |event| {
            actions.push(match event {
                TabEvent::BeginRename(sheet) => Action::BeginRenameSheet(sheet),
                TabEvent::CommitRename(sheet, name) => Action::CommitRenameSheet { sheet, name },
                TabEvent::CancelRename => Action::CancelRenameSheet,
                TabEvent::Duplicate(sheet) => Action::DuplicateSheet(sheet),
                TabEvent::ResetColumnWidths(sheet) => {
                    forget_gutter_width(&ctx, sheet);
                    Action::ResetColumnWidths {
                        columns: state.sheet_column_ids(sheet),
                    }
                }
                TabEvent::Delete(sheet) => Action::DeleteSheet(sheet),
            })
        },
    );
    if sheets.is_empty() {
        empty_state(&mut grid_ui, tr!("workbook-no-worksheets"));
    } else {
        let vertical_bar_id = grid_ui.id().with("grid_vertical_bar");
        let horizontal_bar_id = grid_ui.id().with("grid_horizontal_bar");
        let pending_offset: Option<f32> =
            grid_ui.memory_mut(|memory| memory.data.remove_temp(vertical_bar_id));
        let pending_horizontal_offset: Option<f32> =
            grid_ui.memory_mut(|memory| memory.data.remove_temp(horizontal_bar_id));
        let scroll =
            show_horizontally_scrollable_grid(&mut grid_ui, pending_horizontal_offset, |ui| {
                grid_contents(ui, state, actions, selected, pending_offset)
            });
        let overflows = scroll.content_size.x > scroll.inner_rect.width() + 0.5;
        if overflows != horizontal_bar_present {
            grid_ui.memory_mut(|memory| {
                memory
                    .data
                    .insert_temp(horizontal_bar_present_id, overflows)
            });
            grid_ui.ctx().request_repaint();
        }
        let mut horizontal_bar_ui = bounded(horizontal_bar_strip);
        show_grid_horizontal_bar(
            &mut horizontal_bar_ui,
            horizontal_bar_id,
            horizontal_bar_strip,
            &scroll,
        );
        if let Some(table) = scroll.inner {
            let mut bar_ui = bounded(vertical_bar_strip);
            show_grid_vertical_bar(&mut bar_ui, vertical_bar_id, vertical_bar_strip, &table);
        }
    }
    ui.advance_cursor_after_rect(area);
    if let Some(id) = chosen_sheet {
        state.dispatch(Action::SelectSheet(id));
    }
    if add_sheet {
        actions.push(Action::AddWorksheet { book });
    }
}

#[cfg(test)]
mod tests {
    //! Tests of the workbook, without a window: layout (grid, sheet tabs, detached
    //! scroll bars), clipping, menus. They read what is painted (rectangles,
    //! texts, clips) or the pushed actions, never the source text.

    use super::super::window_chrome::show_floating_window;
    use super::*;
    use stokesforge_core::{Column, Project};
    use stokesforge_session::Session;

    /// A workbook of one sheet, `columns` columns of `rows` rows.
    fn workbook(columns: usize, rows: usize) -> (Session, NodeId) {
        let mut project = Project::new("Workbook").unwrap();
        let root = project.root();
        let book = project.add_empty_workbook(root, None).unwrap();
        let sheet = project.add_worksheet(book, None).unwrap();
        let values: Vec<f64> = (0..rows).map(|row| row as f64).collect();
        for _ in 0..columns {
            project
                .add_column(sheet, None, None, Column::from_f64(&values))
                .unwrap();
        }
        (Session::spawn(project, None).unwrap(), book)
    }

    /// Center of the first painted text equal to `needle`.
    fn text_center(shapes: &[egui::epaint::ClippedShape], needle: &str) -> egui::Pos2 {
        shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) if shape.galley.job.text == needle => {
                    Some(shape.pos + shape.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no painted text carries {needle:?}"))
    }

    /// Left click at `pos`, spread over three frames (hover, press,
    /// release).
    fn click(paint: &mut impl FnMut(Vec<egui::Event>) -> egui::FullOutput, pos: egui::Pos2) {
        paint(vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            paint(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
    }

    // A workbook cut by the bottom of the view: its sheet tab strip, placed at the
    // bottom of the window, must not be drawn under the view (on the scroll bar or
    // the console). `egui::Panel` replaces its parent's clipping instead of
    // combining it: it is what made the tabs overflow.
    #[test]
    fn a_workbook_cut_by_the_bottom_of_the_view_draws_nothing_below() {
        let (session, book) = workbook(2, 2);
        let mut state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let id = egui::Id::new("workbook_cut");
        let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1400.0, 400.0));
        let window_rect =
            egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(500.0, 400.0));
        let mut actions = Vec::new();
        let mut output = None;
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1000.0),
                )),
                ..Default::default()
            };
            let mut frame = context.run_ui(input, |ui| {
                show_floating_window(
                    ui.ctx(),
                    id,
                    window_rect,
                    viewport,
                    false,
                    |ui| book_contents(ui, &mut state, &mut actions, book),
                    |_, _| {},
                );
            });
            frame.textures_delta.clear();
            output = Some(frame);
        }
        let output = output.unwrap();
        assert!(!output.shapes.is_empty());
        // A shape whose clip is empty (tabs entirely under the view) draws nothing:
        // only a non-empty cut counts.
        for painted in output
            .shapes
            .iter()
            .filter(|painted| painted.clip_rect.is_positive())
        {
            assert!(
                viewport.contains_rect(painted.clip_rect),
                "a workbook shape is clipped outside the view: {:?}",
                painted.clip_rect,
            );
        }
    }

    /// Solid rectangles painted by `shapes`, flattened, with their clip.
    fn rectangles_and_clips(
        shapes: &[egui::epaint::ClippedShape],
    ) -> Vec<(egui::Rect, egui::Rect)> {
        fn visit(shape: &egui::Shape, clip: egui::Rect, out: &mut Vec<(egui::Rect, egui::Rect)>) {
            match shape {
                egui::Shape::Rect(rect) => out.push((rect.rect, clip)),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        visit(shape, clip, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for painted in shapes {
            visit(&painted.shape, painted.clip_rect, &mut out);
        }
        out
    }

    /// Screen of 300 x 320: `book_contents` of a workbook of 12 columns (much wider
    /// than the screen) and 200 rows, painted six frames, the fifth (index 4)
    /// receiving `events`. Returns the screen and the rectangles (with clip) of the
    /// last frame.
    fn paint_wide_workbook(
        events: Vec<egui::Event>,
    ) -> (egui::Rect, Vec<(egui::Rect, egui::Rect)>) {
        paint_workbook(12, events)
    }

    /// [`paint_wide_workbook`] for a workbook of `columns` columns.
    fn paint_workbook(
        columns: usize,
        events: Vec<egui::Event>,
    ) -> (egui::Rect, Vec<(egui::Rect, egui::Rect)>) {
        let (session, book) = workbook(columns, 200);
        let mut state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&context);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 320.0));
        let mut actions = Vec::new();
        let mut last_frame = None;
        for frame_index in 0..6 {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events: if frame_index == 4 {
                    events.clone()
                } else {
                    Vec::new()
                },
                ..Default::default()
            };
            let mut frame = context.run_ui(input, |ui| {
                book_contents(ui, &mut state, &mut actions, book);
            });
            frame.textures_delta.clear();
            last_frame = Some(frame);
        }
        (screen, rectangles_and_clips(&last_frame.unwrap().shapes))
    }

    /// The handle of the vertical bar (not its track): thin, short, in the right
    /// strip of the screen.
    fn bar_handles(
        screen: egui::Rect,
        rectangles: &[(egui::Rect, egui::Rect)],
    ) -> Vec<(egui::Rect, egui::Rect)> {
        let bar_width = theme::scroll_bar_allocated_width();
        rectangles
            .iter()
            .copied()
            .filter(|(rect, _)| {
                rect.width() <= bar_width
                    && rect.height() >= 11.9
                    && rect.height() < 100.0
                    && rect.left() >= screen.right() - bar_width - 0.5
            })
            .collect()
    }

    /// B10: the grid's vertical scroll bar is stuck to the right edge of the window,
    /// even when the columns overflow (it used to follow the width of the columns
    /// and left the view).
    #[test]
    fn the_grid_vertical_bar_stays_at_the_right_edge_of_the_window() {
        let (screen, rectangles) = paint_wide_workbook(Vec::new());
        let handles = bar_handles(screen, &rectangles);
        assert!(
            !handles.is_empty(),
            "the vertical bar handle must be in the right strip of the window \
             ({screen:?}); painted rectangles: {rectangles:?}",
        );
        for (rect, clip) in handles {
            assert!(
                screen.contains_rect(rect) && clip.is_positive(),
                "the handle {rect:?} leaves the window {screen:?} (clip {clip:?})"
            );
        }
    }

    /// The bar moved to the window edge follows the table's scrolling (wheel on the
    /// table): its handle goes down.
    #[test]
    fn the_grid_vertical_bar_follows_the_wheel_on_the_table() {
        let (screen, at_rest) = paint_wide_workbook(Vec::new());
        let before = bar_handles(screen, &at_rest);
        let (_, scrolled) = paint_wide_workbook(vec![
            egui::Event::PointerMoved(egui::pos2(100.0, 150.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -120.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        let after = bar_handles(screen, &scrolled);
        assert_eq!(before.len(), 1, "one handle at rest: {before:?}");
        assert_eq!(after.len(), 1, "one handle after scrolling: {after:?}");
        assert!(
            after[0].0.top() > before[0].0.top() + 1.0,
            "the handle must go down: {:?} -> {:?}",
            before[0].0,
            after[0].0
        );
    }

    /// The bar covers the whole height of the screen, frozen header and sheet tabs
    /// included.
    #[test]
    fn the_grid_vertical_bar_covers_the_whole_window_height() {
        let (screen, rectangles) = paint_wide_workbook(Vec::new());
        let bar_width = theme::scroll_bar_allocated_width();
        let tracks: Vec<_> = rectangles
            .iter()
            .filter(|(rect, _)| {
                rect.width() <= bar_width
                    && rect.left() >= screen.right() - bar_width - 0.5
                    && rect.height() >= screen.height() - bar_width
            })
            .collect();
        assert!(
            !tracks.is_empty(),
            "a vertical bar track over the whole height of {screen:?}; \
             painted rectangles: {rectangles:?}"
        );
    }

    /// The grid's horizontal bar is moved **under** the sheet tabs: its track
    /// occupies the bottom of the window, over almost its whole width (minus the
    /// vertical bar's strip).
    #[test]
    fn the_grid_horizontal_bar_is_under_the_tabs() {
        let (screen, rectangles) = paint_wide_workbook(Vec::new());
        let bar_height = theme::scroll_bar_allocated_width();
        let tracks: Vec<_> = rectangles
            .iter()
            .filter(|(rect, _)| {
                rect.height() <= bar_height
                    && rect.top() >= screen.bottom() - bar_height - 0.5
                    && rect.width() >= screen.width() - 2.0 * bar_height
            })
            .collect();
        assert!(
            !tracks.is_empty(),
            "a horizontal bar track at the bottom of {screen:?}, under the tabs; \
             painted rectangles: {rectangles:?}"
        );
    }

    #[test]
    fn right_click_in_the_empty_grid_and_on_the_tabs_offers_the_additions() {
        let mut project = Project::new("Menus").unwrap();
        let book = project.add_workbook(project.root(), None).unwrap();
        let sheet = project.tree().node(book).unwrap().children()[0];
        project
            .add_column(sheet, None, None, Column::from_f64(&[1.0]))
            .unwrap();
        let session = Session::spawn(project, None).unwrap();

        for (on_tabs, label) in [
            (false, tr!("workbook-column-add")),
            (true, tr!("workbook-worksheet-add")),
        ] {
            let context = egui::Context::default();
            let mut state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
            let mut actions = Vec::new();
            {
                let mut paint = |events: Vec<egui::Event>| {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600.0, 500.0),
                        )),
                        events,
                        ..Default::default()
                    };
                    let mut output = context.run_ui(input, |ui| {
                        book_contents(ui, &mut state, &mut actions, book)
                    });
                    output.textures_delta.clear();
                    output
                };
                let first = paint(Vec::new());
                let tab_is_visible = first.shapes.iter().any(|painted| match &painted.shape {
                    egui::Shape::Text(shape) if shape.galley.job.text == "Sheet1" => painted
                        .clip_rect
                        .contains(shape.pos + shape.galley.rect.center().to_vec2()),
                    _ => false,
                });
                assert!(tab_is_visible, "the tab strip must stay in the window");
                let pos = if on_tabs {
                    text_center(&first.shapes, "Sheet1")
                } else {
                    egui::pos2(450.0, 180.0)
                };
                paint(vec![egui::Event::PointerMoved(pos)]);
                paint(vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }]);
                paint(vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
                // The right-click already designates the target: the add is direct, with no
                // "Sheet ▸" or "Columns ▸" submenu (#152).
                let output = paint(Vec::new());
                assert!(
                    output.shapes.iter().any(|painted| match &painted.shape {
                        egui::Shape::Text(shape) => shape.galley.job.text == *label,
                        _ => false,
                    }),
                    "the context menu must offer {label}: {:?}",
                    output
                        .shapes
                        .iter()
                        .filter_map(|painted| match &painted.shape {
                            egui::Shape::Text(shape) => Some(shape.galley.job.text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                );
                let menu_pos = text_center(&output.shapes, &label);
                paint(vec![egui::Event::PointerMoved(menu_pos)]);
                paint(vec![egui::Event::PointerButton {
                    pos: menu_pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }]);
                paint(vec![egui::Event::PointerButton {
                    pos: menu_pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
            if on_tabs {
                assert!(
                    matches!(actions.as_slice(), [Action::AddWorksheet { book: id }] if *id == book)
                );
            } else {
                assert!(
                    matches!(actions.as_slice(), [Action::AddColumn { sheet: id }] if *id == sheet)
                );
            }
        }
    }

    /// #51: the horizontal bar's strip is reserved only if the grid overflows in
    /// width. A single narrow column: no track at the bottom of the window (witness
    /// of `the_grid_horizontal_bar_is_under_the_tabs`, whose rectangle filter could
    /// otherwise catch anything).
    #[test]
    fn without_overflow_no_horizontal_bar_is_reserved() {
        let (screen, rectangles) = paint_workbook(1, Vec::new());
        let bar_height = theme::scroll_bar_allocated_width();
        let tracks: Vec<_> = rectangles
            .iter()
            .filter(|(rect, _)| {
                rect.height() <= bar_height
                    && rect.top() >= screen.bottom() - bar_height - 0.5
                    && rect.width() >= screen.width() - 2.0 * bar_height
            })
            .collect();
        assert!(
            tracks.is_empty(),
            "one column fits in {screen:?}: no horizontal track; {tracks:?}"
        );
    }

    /// Two sheets: their tabs are painted under the grid (under the first data
    /// row), and a click on the second shows its sheet.
    #[test]
    fn the_sheet_tabs_are_under_the_grid_and_choose_the_sheet() {
        let mut project = Project::new("Sheets").unwrap();
        let root = project.root();
        let book = project.add_empty_workbook(root, None).unwrap();
        let first = project.add_worksheet(book, None).unwrap();
        let second = project.add_worksheet(book, None).unwrap();
        for sheet in [first, second] {
            project
                .add_column(sheet, None, None, Column::from_f64(&[42.0]))
                .unwrap();
        }
        let session = Session::spawn(project, None).unwrap();
        let mut state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let names: Vec<String> = state
            .worksheets_of(book)
            .into_iter()
            .map(|(_, name)| name)
            .collect();
        assert_eq!(names.len(), 2);
        assert_eq!(state.selected_sheet_in(book), Some(first));
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&context);
        let mut actions = Vec::new();
        let mut paint = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 400.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                book_contents(ui, &mut state, &mut actions, book);
            });
            output.textures_delta.clear();
            output
        };
        paint(Vec::new());
        let output = paint(Vec::new());
        let data_pos = text_center(&output.shapes, "42");
        for name in &names {
            let tab_pos = text_center(&output.shapes, name);
            assert!(
                tab_pos.y > data_pos.y,
                "the tab {name:?} ({tab_pos:?}) must be under the grid ({data_pos:?})"
            );
        }
        let second_pos = text_center(&output.shapes, &names[1]);
        click(&mut paint, second_pos);
        assert_eq!(
            state.selected_sheet_in(book),
            Some(second),
            "the click on the tab shows its sheet"
        );
    }

    /// Workbook menu: "Add Worksheet" pushes its action; "Delete Sheet" is grayed
    /// out when the workbook has only one sheet (a click pushes nothing there),
    /// active otherwise.
    #[test]
    fn the_workbook_menu_grays_out_deleting_the_last_sheet() {
        for sheet_count in [1, 2] {
            let mut project = Project::new("Menu").unwrap();
            let root = project.root();
            let book = project.add_empty_workbook(root, None).unwrap();
            let mut sheets = Vec::new();
            for _ in 0..sheet_count {
                sheets.push(project.add_worksheet(book, None).unwrap());
            }
            let sheet = sheets[0];
            let session = Session::spawn(project, None).unwrap();
            let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
            let context = egui::Context::default();
            context.set_fonts(stokesforge_render_egui::font_definitions());
            let mut actions = Vec::new();
            let mut paint = |events: Vec<egui::Event>| {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 300.0),
                    )),
                    events,
                    ..Default::default()
                };
                let mut output = context.run_ui(input, |ui| {
                    let ctx = ui.ctx().clone();
                    sheet_menu(ui, &ctx, &state, book, sheet, &mut actions);
                });
                output.textures_delta.clear();
                output
            };
            let output = paint(Vec::new());
            let delete = text_center(&output.shapes, &tr!("action-delete-sheet"));
            let add = text_center(&output.shapes, &tr!("workbook-worksheet-add"));
            click(&mut paint, delete);
            click(&mut paint, add);
            let deletion = actions
                .iter()
                .any(|action| matches!(action, Action::DeleteSheet(s) if *s == sheet));
            assert_eq!(
                deletion,
                sheet_count > 1,
                "{sheet_count} sheet(s): deletion requested = {deletion} ({actions:?})"
            );
            assert!(
                actions
                    .iter()
                    .any(|action| matches!(action, Action::AddWorksheet { book: b } if *b == book)),
                "{actions:?}"
            );
        }
    }
}
