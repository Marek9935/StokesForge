//! Graph window: the figure ([`graph_view_contents`], a single piece of code
//! for the floating window and the detached window), the drop zone for
//! columns ([`graph_drop_target`]), the context menu of the figure, the
//! signals of the view ([`annotation_interactions`]: in-place editors,
//! drag, Delete), the order and properties of graphic objects, and the
//! export request ([`request_graph_export`]). The in-place editors themselves
//! live in `graph_text_edit.rs`.
//!
//! Moved out of `app.rs` as is (#121).

use std::collections::HashMap;

use eframe::egui;
use stokesforge_core::compose::CellSide;
use stokesforge_core::{NodeId, Tree};
use stokesforge_render_egui::{
    AxisSide, FigureLayout, GraphView, PlotComponent, TextTarget, DEFAULT_FIGURE_SIZE,
};
use stokesforge_session::Snapshot;

use crate::color_picker::{color_field, ColorUsage, PickerEnv};
use crate::number_field::number_field;
use stokesforge_app_state::cell_sides;
use stokesforge_app_state::object_manager::{AxisRef, VisibilityTarget};
use stokesforge_app_state::quantities;
use stokesforge_app_state::state::{
    drop_payload_action, drop_rows_action, export_file_name, Action, AppState, ColumnDragPayload,
    PlotAxis, PlotSettingsTab, PlotTarget, RowDragPayload,
};
use stokesforge_app_state::tr;

use super::analysis_handles::calculation_handles;
use super::dialogs::{modal_dialog, DialogOutcome, InteractiveSize};
use super::graph_text_edit::{
    add_annotation_at, annotation_editor_id, annotation_editor_ui, open_annotation_editor,
    open_group_name_editor_in_graph, open_legend_entry_editor, open_series_name_editor,
    open_text_editor, text_editor_id, text_editor_ui, AnnotationEditor, ContextText, TextEditor,
};
use super::grid::keyboard_focus_outside_grid;
use super::menu_sections::MenuSections;
use super::object_manager_ui::{
    apply_object_request, group_shortcuts, object_node_menu, select_manager_target,
    series_shortcuts, ObjectManagerRequest,
};
use super::shortcuts::SHORTCUT_EXPORT_GRAPH;
use crate::color_picker::{saved_colors_id, SavedColorRequest};

/// Height of the only control kept under a figure: the return to the
/// initial framing. Reserving its height keeps the figure's rectangle stable
/// from one frame to the next — otherwise the shape cache would be invalidated
/// at the slightest hover.
const GRAPH_CONTROLS_HEIGHT: f32 = 28.0;

/// Wraps [`graph_contents`] in a drop zone for dragging a
/// selection of workbook columns — the counterpart, on the target side, of the
/// grab zone placed by [`grid_contents`](crate::app::grid::grid_contents) at the edge of the grid. Called by
/// [`tab_contents`](crate::app::contents::tab_contents), hence identically for a tab in a floating window
/// and for a detached tab ([`StokesForgeApp::show_detached_tabs`](crate::app::StokesForgeApp::show_detached_tabs)): it is the
/// only
/// entry point that both use, see the head doc of the latter — no
/// separate drop code to maintain for the detached case.
///
/// A hover with an active [`ColumnDragPayload`] records this window as the
/// drop target (`file_drop::set_drop_target`): the window paints its own
/// border in `theme::drop_target_stroke`, the same as for an external file
/// (#386), nothing inside the content. A release over it pushes
/// [`stokesforge_app_state::state::drop_payload_action`] into `actions`, converted into
/// [`Action::AddSeriesToGraph`]. Releasing anywhere other than over a graph
/// pushes nothing: `dnd_release_payload` answers only if the pointer is
/// inside this rectangle at the moment of release.
pub(super) fn graph_drop_target(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    graph_views: &mut HashMap<NodeId, GraphView>,
    graph_id: NodeId,
) {
    let tab_rect = ui.max_rect();
    // A `Response` covering the whole tab, even with `Sense::hover()`,
    // takes part in egui's target selection and wins the primary drag
    // over the `GraphView`. That was the difference with the detached window,
    // which calls `graph_view_contents` directly, and the cause of the
    // inoperative pan in the main window. The drop target does not need
    // to be a widget: reading the pointer and egui's payload is enough.
    let pointer_in_tab = ui.rect_contains_pointer(tab_rect);
    let hovering = pointer_in_tab
        && (egui::DragAndDrop::has_payload_of_type::<ColumnDragPayload>(ui.ctx())
            || egui::DragAndDrop::has_payload_of_type::<RowDragPayload>(ui.ctx()));
    // Columns dropped on the right or bottom edge of the figure make a new
    // cell beside the one nearest the pointer (#357).
    let figure = graph_views
        .get(&graph_id)
        .and_then(GraphView::display_rect)
        .unwrap_or(tab_rect);
    let pointer = ui.input(|input| input.pointer.latest_pos());
    let columns_over =
        pointer_in_tab && egui::DragAndDrop::has_payload_of_type::<ColumnDragPayload>(ui.ctx());
    let edge = pointer
        .filter(|_| columns_over)
        .and_then(|pos| edge_drop_side(pos, figure, EDGE_BAND_WIDTH));
    if pointer_in_tab && ui.input(|input| input.pointer.any_released()) {
        // Targeted layer: the active layer of the graph (#161).
        // `has_payload_of_type` before `take_payload`: the latter empties the
        // payload even when its type does not match (rows used to get lost).
        if egui::DragAndDrop::has_payload_of_type::<ColumnDragPayload>(ui.ctx()) {
            if let Some(payload) = egui::DragAndDrop::take_payload::<ColumnDragPayload>(ui.ctx()) {
                let beside = edge.zip(pointer).and_then(|(side, pos)| {
                    let cell = graph_views.get(&graph_id)?.nearest_cell(pos)?;
                    Some((side, cell))
                });
                if let Some((side, panel)) = beside {
                    actions.push(Action::AddCell {
                        graph: graph_id,
                        panel,
                        side,
                        columns: payload.columns.clone(),
                    });
                } else {
                    let panel = published_active_panel(ui.ctx(), graph_id);
                    actions.push(drop_payload_action(&payload, graph_id, panel));
                }
            }
        } else if let Some(payload) = egui::DragAndDrop::take_payload::<RowDragPayload>(ui.ctx()) {
            // Rows (#166): the state decides the axis, and asks the question
            // if the graph already has an X.
            let panel = published_active_panel(ui.ctx(), graph_id);
            actions.push(drop_rows_action(&payload, graph_id, panel));
        }
    }
    graph_contents(ui, state, actions, graph_views, graph_id);
    if hovering {
        super::file_drop::set_drop_target(ui.ctx(), ui.layer_id());
    }
    if columns_over {
        paint_edge_bands(ui, figure, edge);
    }
}

/// Width, in points, of the right and bottom bands of a figure where dropped
/// columns create a new cell (#357).
const EDGE_BAND_WIDTH: f32 = 36.0;

/// Which edge band of `figure` the pointer is in (#357): the right band gives
/// [`CellSide::Right`], the bottom band [`CellSide::Below`], and the
/// bottom-right corner, in both, gives `Right`. `None` elsewhere. Pure, so it is
/// tested without a window.
fn edge_drop_side(pointer: egui::Pos2, figure: egui::Rect, band: f32) -> Option<CellSide> {
    if !figure.contains(pointer) {
        return None;
    }
    if pointer.x >= figure.max.x - band {
        Some(CellSide::Right)
    } else if pointer.y >= figure.max.y - band {
        Some(CellSide::Below)
    } else {
        None
    }
}

/// The two bands, outlined while columns are dragged over the graph, the one
/// under the pointer filled (theme strokes, nothing in the exported figure).
fn paint_edge_bands(ui: &egui::Ui, figure: egui::Rect, active: Option<CellSide>) {
    let right = egui::Rect::from_min_max(
        egui::pos2(figure.max.x - EDGE_BAND_WIDTH, figure.min.y),
        figure.max,
    );
    let below = egui::Rect::from_min_max(
        egui::pos2(figure.min.x, figure.max.y - EDGE_BAND_WIDTH),
        egui::pos2(figure.max.x - EDGE_BAND_WIDTH, figure.max.y),
    );
    let painter = ui.painter();
    for (band, side) in [(right, CellSide::Right), (below, CellSide::Below)] {
        let fill = if active == Some(side) {
            crate::theme::drop_edge_band_fill()
        } else {
            egui::Color32::TRANSPARENT
        };
        painter.rect(
            band,
            0.0,
            fill,
            crate::theme::drop_edge_band_stroke(),
            egui::StrokeKind::Inside,
        );
    }
}

/// Opens the properties dialog of a graphic object: the Object Manager
/// (double-click on its row) and the figure (double-click on the object) go
/// through here (#113).
pub(super) fn open_shape_properties(
    ctx: &egui::Context,
    view: &mut GraphView,
    spec: &stokesforge_plot::PlotSpec,
    target: stokesforge_plot::ShapeRef,
) {
    view.set_tool(stokesforge_render_egui::GraphTool::Pointer);
    view.select_shape(Some(target));
    if let Some(draft) = target.get(spec).cloned() {
        ctx.data_mut(|data| {
            data.insert_temp(
                view.id().with("shape_properties"),
                ShapeProperties {
                    target,
                    draft,
                    macro_open: false,
                },
            )
        });
    }
}

#[derive(Clone)]
struct ShapeProperties {
    target: stokesforge_plot::ShapeRef,
    draft: stokesforge_plot::ShapeSpec,
    /// The history macro is open (first setting applied).
    macro_open: bool,
}

/// Live settings (S-F1): each change of the draft is applied right
/// away inside a history macro; OK keeps it (a single entry),
/// Cancel aborts it and the object returns to its opening state.
fn shape_properties_ui(
    ctx: &egui::Context,
    view: &GraphView,
    graph: NodeId,
    actions: &mut Vec<Action>,
) {
    let id = view.id().with("shape_properties");
    let Some(mut edit) = ctx.data(|data| data.get_temp::<ShapeProperties>(id)) else {
        return;
    };
    let (project, user): (stokesforge_plot::SavedColors, stokesforge_plot::SavedColors) = ctx
        .data(|data| data.get_temp(saved_colors_id()))
        .unwrap_or_default();
    let mut env = PickerEnv {
        project,
        user,
        requests: Vec::new(),
    };
    let before = edit.draft.clone();
    let outcome = modal_dialog(
        ctx,
        id.with("dialog"),
        &tr!("object-properties-title"),
        InteractiveSize::Default,
        |ui| {
            for (coordinate, value) in [
                ("X1", &mut edit.draft.x1),
                ("Y1", &mut edit.draft.y1),
                ("X2", &mut edit.draft.x2),
                ("Y2", &mut edit.draft.y2),
            ] {
                ui.horizontal(|ui| {
                    ui.label(tr!("object-coordinate", coordinate = coordinate));
                    number_field(
                        ui,
                        ("shape_coordinate", coordinate),
                        value,
                        &quantities::FREE,
                    );
                });
            }
            color_field(
                ui,
                &tr!("format-object-line-color"),
                &mut edit.draft.line_color,
                ColorUsage::Curves,
                Some(&tr!("format-object-line-color", "hint")),
                &mut env,
            );
            if edit.draft.kind.is_closed() {
                color_field(
                    ui,
                    &tr!("format-object-fill-color"),
                    &mut edit.draft.fill_color,
                    ColorUsage::Background,
                    Some(&tr!("format-object-fill-color", "hint")),
                    &mut env,
                );
            }
            ui.horizontal(|ui| {
                ui.label(tr!("format-object-line-width"));
                number_field(
                    ui,
                    "shape_line_width",
                    &mut edit.draft.line_width,
                    &quantities::SHAPE_LINE_WIDTH,
                );
            });
        },
    );
    actions.extend(env.requests.into_iter().map(SavedColorRequest::into_action));
    let changed = edit.draft != before;
    if changed {
        if !edit.macro_open {
            actions.push(Action::BeginShapeEdit);
            edit.macro_open = true;
        }
        actions.push(Action::SetShape {
            graph,
            target: edit.target,
            shape: Box::new(edit.draft.clone()),
        });
    }
    match outcome {
        DialogOutcome::Accept => {
            if edit.macro_open {
                actions.push(Action::EndShapeEdit { keep: true });
            }
            ctx.data_mut(|data| data.remove::<ShapeProperties>(id));
        }
        DialogOutcome::Cancel => {
            if edit.macro_open {
                actions.push(Action::EndShapeEdit { keep: false });
            }
            ctx.data_mut(|data| data.remove::<ShapeProperties>(id));
        }
        DialogOutcome::Open => {
            ctx.data_mut(|data| data.insert_temp(id, edit));
        }
    }
}

/// An open graph — the **visible rendering** of phase R4 (task 4.6).
/// Content of a [`Tab::Graph`](crate::app::tab::Tab::Graph).
///
/// This is not a sketch of the final graph window: like this whole
/// bench, it is the proof that the end-to-end integration holds. What it
/// really shows, and no test shows:
///
/// - the figure itself, with its real fonts and its real HiDPI;
/// - the return to the initial framing, to abandon a zoom or a pan without
///   changing the data or the description of the graph.
fn graph_contents(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    graph_views: &mut HashMap<NodeId, GraphView>,
    graph_id: NodeId,
) {
    let view = graph_views.entry(graph_id).or_default();
    graph_view_contents(ui, view, state.snapshot(), graph_id, actions);
}

/// Body of [`graph_contents`], factored over a [`GraphView`] and a
/// [`Snapshot`] already in hand rather than over `&AppState`/the table
/// [`StokesForgeApp::graph_views`](crate::app::StokesForgeApp::graph_views): this is what lets
/// [`StokesForgeApp::show_detached_tabs`](crate::app::StokesForgeApp::show_detached_tabs) paint a detached `Tab::Graph` with
/// **exactly** this function — no degraded version, see the head doc of
/// `window_detach.rs`. `GraphView` does not have the global lock problem of
/// `AppState` that the cell/note input buffers have (its cache and its
/// navigation are already scoped per graph): no need for a separate
/// "detached" variant, unlike [`grid_contents`](crate::app::grid::grid_contents)/
/// [`note_contents`](crate::app::contents::note_contents).
pub(super) fn graph_view_contents(
    ui: &mut egui::Ui,
    view: &mut GraphView,
    snapshot: &Snapshot,
    graph_id: NodeId,
    actions: &mut Vec<Action>,
) {
    let available = ui.available_size_before_wrap();
    // The figure takes everything left after its controls. Reserving their
    // full height avoids the same auto-growing mechanism that has
    // already hit the workbook windows: an overrun would make
    // `rect` change every frame, hence cause a cache miss on hover.
    let (outer, _) = ui.allocate_exact_size(
        egui::vec2(available.x, (available.y - GRAPH_CONTROLS_HEIGHT).max(0.0)),
        egui::Sense::hover(),
    );
    // The displayed rectangle follows the available space (F11) with the ratio of
    // the figure (`GraphView::figure_layout`, #394: the sum of the cell sizes
    // plus the title band): "contain" scaling, bounded by the
    // limiting dimension. `paint_scaled` lays the figure out at its own size and
    // then enlarges or shrinks the shapes, text included: no
    // tick mark is recomputed for the window size (point 3 of
    // `retours-2026-09-24-bugs.md`). The child Ui bounds the
    // interactions without growing the parent window.
    publish_figure_area(ui.ctx(), graph_id, outer.size());
    let layout = view.figure_layout(ui.ctx(), snapshot, graph_id).ok();
    let (fig_w, fig_h) = layout.as_ref().map_or(DEFAULT_FIGURE_SIZE, |l| l.size);
    if let Some(layout) = layout {
        publish_figure_layout(ui.ctx(), graph_id, layout);
    }
    let scale = if fig_w > 0 && fig_h > 0 {
        (outer.width() / fig_w as f32)
            .min(outer.height() / fig_h as f32)
            .max(0.0)
    } else {
        0.0
    };
    let rect = egui::Rect::from_center_size(
        outer.center(),
        egui::vec2(fig_w as f32 * scale, fig_h as f32 * scale),
    );
    let mut figure_ui = ui.new_child(egui::UiBuilder::new().max_rect(outer));
    // The grab margin of the figure's edges reaches into the frame margin of
    // the window (#405): the clip rectangle must let it through.
    let grab = stokesforge_render_egui::FIGURE_GRAB_MARGIN;
    figure_ui.set_clip_rect(ui.clip_rect().expand(grab).intersect(outer.expand(grab)));
    // A transparent background ("None" color, or partial opacity) shows on
    // white: without it, the figure would take the color of the window,
    // which is not that of the export. This white is painted **only here**,
    // under the figure, never in `stokesforge-render`: the SVG/PNG/PDF export
    // of a graph without a background stays transparent (invariant 5, read
    // the other way; test `a_colorless_background_gives_a_transparent_png`).
    let background = stokesforge_core::compose::current_plot_spec(&snapshot.tree, graph_id)
        .map(|spec| spec.background)
        .unwrap_or_default();
    if let Some(backdrop) = display_backdrop(&background) {
        figure_ui.painter().rect_filled(rect, 0.0, backdrop);
    }
    view.set_figure_texts(&stokesforge_app_state::figure_texts::figure_texts());
    match view.paint_scaled(&figure_ui, rect, snapshot, graph_id) {
        Ok(_) => {
            publish_axis_bounds(ui.ctx(), view, graph_id);
            if let Some(spec) =
                stokesforge_core::compose::current_plot_spec(&snapshot.tree, graph_id)
            {
                layer_badges(&figure_ui, view, &spec, graph_id, outer);
            }
            graph_context_menu(ui, view, &snapshot.tree, graph_id, actions);
            annotation_interactions(ui, view, &snapshot.tree, graph_id, actions);
            if let Some(spec) =
                stokesforge_core::compose::current_plot_spec(&snapshot.tree, graph_id)
            {
                calculation_handles(ui, view, &snapshot.tree, graph_id, &spec, actions);
            }
            if !keyboard_focus_outside_grid(ui.ctx()) {
                if let Some(spec) =
                    stokesforge_core::compose::current_plot_spec(&snapshot.tree, graph_id)
                {
                    if let Some(group) = view.selected_group() {
                        // A selected group (#197): same gestures as with the
                        // Object Manager keyboard; renaming is done in place
                        // in the graph (#206), as the tree may be hidden
                        // or folded.
                        let mut requests = Vec::new();
                        group_shortcuts(ui.ctx(), graph_id, group, &spec, actions, &mut requests);
                        for request in requests {
                            if let ObjectManagerRequest::RenameGroup(group) = request {
                                open_group_name_editor_in_graph(ui.ctx(), view, &spec, group);
                            }
                        }
                        if ui.input_mut(|i| i.consume_shortcut(&super::shortcuts::KEY_DELETE)) {
                            actions.push(Action::RemovePlotObject {
                                graph: graph_id,
                                target: stokesforge_app_state::object_manager::VisibilityTarget::SeriesGroup(group),
                            });
                        }
                    } else if let Some(series) = view.selected_series() {
                        let mut requests = Vec::new();
                        series_shortcuts(ui.ctx(), graph_id, series, &spec, actions, &mut requests);
                        for request in requests {
                            if let ObjectManagerRequest::Rename(series) = request {
                                open_series_name_editor(
                                    ui.ctx(),
                                    view,
                                    &snapshot.tree,
                                    graph_id,
                                    series,
                                );
                            }
                        }
                        if ui.input_mut(|i| i.consume_shortcut(&super::shortcuts::KEY_DELETE)) {
                            actions.push(Action::RemovePlotObject {
                                graph: graph_id,
                                target:
                                    stokesforge_app_state::object_manager::VisibilityTarget::Series(
                                        series,
                                    ),
                            });
                        }
                    }
                }
            }
            shape_properties_ui(ui.ctx(), view, graph_id, actions);
            if ui.small_button(tr!("plot-view-reset-zoom")).clicked() {
                view.reset_view();
            }
        }
        Err(err) => {
            ui.colored_label(
                ui.visuals().error_fg_color,
                stokesforge_app_state::errors::render::error_message(&err),
            );
        }
    }
}

/// The bounds that the last draw kept for the axes of a layer, in
/// data values and in ascending order (#221). Computed by
/// `stokesforge-render` (`DrawnPanel`), never redone here.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct PanelBounds {
    /// Index of the layer in `PlotSpec::panels`.
    pub index: usize,
    pub bottom: Option<(f64, f64)>,
    pub left: Option<(f64, f64)>,
    pub right: Option<(f64, f64)>,
}

fn axis_bounds_id(graph: NodeId) -> egui::Id {
    egui::Id::new(("stokesforge_axis_bounds", graph))
}

fn ascending((a, b): (f64, f64)) -> (f64, f64) {
    (a.min(b), a.max(b))
}

/// Publishes the bounds of the last draw of `graph`: the properties window,
/// which does not have the [`GraphView`], shows them grayed out under "Auto range".
fn publish_axis_bounds(ctx: &egui::Context, view: &GraphView, graph: NodeId) {
    let Some(drawn) = view.last_drawn() else {
        return;
    };
    let bounds: Vec<PanelBounds> = drawn
        .panels
        .iter()
        .map(|panel| PanelBounds {
            index: panel.index,
            bottom: Some(ascending(panel.x_range)),
            left: Some(ascending(panel.y_range)),
            right: panel.y2_range.map(ascending),
        })
        .collect();
    ctx.data_mut(|data| data.insert_temp(axis_bounds_id(graph), bounds));
}

/// Publishes bounds by hand: the tests of the properties window, which
/// have no painted view, need them for the From / To fields under Auto.
#[cfg(test)]
pub(super) fn set_published_axis_bounds(
    ctx: &egui::Context,
    graph: NodeId,
    bounds: Vec<PanelBounds>,
) {
    ctx.data_mut(|data| data.insert_temp(axis_bounds_id(graph), bounds));
}

/// The bounds published for the layer `panel` of `graph`; `None` for lack of a
/// painted view.
pub(super) fn published_axis_bounds(
    ctx: &egui::Context,
    graph: NodeId,
    panel: usize,
) -> Option<PanelBounds> {
    ctx.data(|data| data.get_temp::<Vec<PanelBounds>>(axis_bounds_id(graph)))?
        .into_iter()
        .find(|bounds| bounds.index == panel)
}

/// Publishes the layout of the figure of `graph`, as its view last laid it out
/// (#394): the size in points, the title band and the cell breakpoints. The PNG
/// resolution labels of the menus and the size fields of Graph Properties read
/// it without a [`GraphView`], and the window that follows the figure compares
/// the size with the previous one.
fn publish_figure_layout(ctx: &egui::Context, graph: NodeId, layout: FigureLayout) {
    ctx.data_mut(|data| data.insert_temp(figure_layout_id(graph), layout));
}

/// Publishes the area the figure of `graph` is contained in (the window's
/// content, controls left out): the window that follows the figure measures
/// what its frame adds around it (#394).
fn publish_figure_area(ctx: &egui::Context, graph: NodeId, area: egui::Vec2) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(("stokesforge_figure_area", graph)), area));
}

/// The area `graph`'s figure was last contained in; `None` before a painted view.
pub(super) fn published_figure_area(ctx: &egui::Context, graph: NodeId) -> Option<egui::Vec2> {
    ctx.data(|data| data.get_temp(egui::Id::new(("stokesforge_figure_area", graph))))
}

/// The figure layout `graph` last published; `None` for lack of a painted view.
pub(super) fn published_figure_layout(ctx: &egui::Context, graph: NodeId) -> Option<FigureLayout> {
    ctx.data(|data| data.get_temp(figure_layout_id(graph)))
}

/// The figure size `graph` last published, in points.
pub(super) fn published_figure_size(ctx: &egui::Context, graph: NodeId) -> Option<(u32, u32)> {
    published_figure_layout(ctx, graph).map(|layout| layout.size)
}

fn figure_layout_id(graph: NodeId) -> egui::Id {
    egui::Id::new(("stokesforge_figure_layout", graph))
}

/// The active layer of `graph`, as its last painted view published it
/// (#161): readable from a workbook menu or a neighboring window, which
/// have no access to the [`GraphView`]. The first layer for lack of a painted view.
pub(super) fn published_active_panel(ctx: &egui::Context, graph: NodeId) -> usize {
    ctx.data(|data| data.get_temp(active_panel_id(graph)))
        .unwrap_or(0)
}

pub(super) fn active_panel_id(graph: NodeId) -> egui::Id {
    egui::Id::new(("stokesforge_active_panel", graph))
}

/// Layer marker (#161), Origin style: a small number per
/// layer at the top left of its own cell (#395; side by side for the overlays of
/// one cell, in the corner of the window for a layer not drawn), the active layer's highlighted;
/// clicking a number activates the layer. Interface marker painted over the
/// figure: never in the export nor in the figure copy.
fn layer_badges(
    ui: &egui::Ui,
    view: &mut GraphView,
    spec: &stokesforge_plot::PlotSpec,
    graph: NodeId,
    outer: egui::Rect,
) {
    let active = view.active_panel(spec);
    ui.ctx()
        .data_mut(|data| data.insert_temp(active_panel_id(graph), active));
    let side = 18.0;
    let gap = 3.0;
    // Badges already placed per anchor (the cell's outer corner, or the window
    // corner for a layer not drawn): overlays of one cell go side by side.
    let mut placed: Vec<(egui::Pos2, usize)> = Vec::new();
    for index in 0..spec.panels.len() {
        let anchor = view
            .cell_rect_screen(index)
            .map_or(outer.left_top(), |cell| cell.left_top());
        let slot = match placed.iter_mut().find(|(pos, _)| *pos == anchor) {
            Some((_, count)) => {
                *count += 1;
                *count - 1
            }
            None => {
                placed.push((anchor, 1));
                0
            }
        };
        let rect = egui::Rect::from_min_size(
            anchor + egui::vec2(4.0 + slot as f32 * (side + gap), 4.0),
            egui::vec2(side, side),
        );
        if !outer.contains_rect(rect) {
            continue;
        }
        let number = index + 1;
        let response = ui
            .interact(
                rect,
                view.id().with(("layer_badge", index)),
                egui::Sense::click(),
            )
            .on_hover_text(tr!("plot-layer-badge", "hint", n = number.to_string()));
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                ui.is_enabled(),
                index == active,
                tr!("plot-layer-badge-accessible", n = number.to_string()),
            )
        });
        let visuals = ui.visuals();
        let (fill, ink) = if index == active {
            (visuals.selection.bg_fill, visuals.selection.stroke.color)
        } else if response.hovered() {
            (
                visuals.widgets.hovered.bg_fill,
                visuals.widgets.hovered.fg_stroke.color,
            )
        } else {
            (
                visuals.widgets.inactive.bg_fill,
                visuals.widgets.inactive.fg_stroke.color,
            )
        };
        let painter = ui.painter();
        painter.rect_filled(rect, 3.0, fill);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            number.to_string(),
            egui::FontId::proportional(11.0),
            ink,
        );
        if response.clicked() {
            view.set_active_panel(index, spec);
            ui.ctx()
                .data_mut(|data| data.insert_temp(active_panel_id(graph), index));
        }
    }
}

/// Display white under a figure whose background is (semi-)transparent —
/// `None` under an opaque background, which already covers everything. The
/// transparent background is thus seen composed over white, as the automatic ink
/// judges it (`stokesforge_render::color::over_white`).
fn display_backdrop(background: &str) -> Option<egui::Color32> {
    crate::color_picker::parse_hex(background)
        .is_some_and(|[_, _, _, alpha]| alpha < 255)
        .then_some(egui::Color32::WHITE)
}

/// What the right-click targeted in the figure, read **at the time of the click** and
/// remembered: the menu stays open for several frames, during which the
/// pointer moves.
#[derive(Debug, Clone, Copy)]
struct ContextHit {
    /// The component, for the grayed header and the properties page of the background.
    target: PlotTarget,
    /// The object that has its own menu (curve, free text, object, title,
    /// legend, axis) — the same as in the Object Manager.
    object: Option<VisibilityTarget>,
    /// A legend entry: no Object Manager node, its own menu.
    legend_entry: Option<ContextText>,
    /// The egui pixel of the click: insertion point of a text or a shape.
    pos: Option<egui::Pos2>,
    /// The data point under the click, for "Add Text".
    point: Option<(usize, f64, f64)>,
}

/// Context menu of the figure (#152): a right-click (without dragging — the
/// secondary drag remains the rubber-band zoom of `stokesforge-render-egui`)
/// opens **the menu of the targeted object**, that of the Object Manager
/// ([`object_node_menu`]), or, on the background and the plot area, Add Text,
/// Insert Object, Export and Properties…. A right-click on a curve
/// selects it. The grayed header names the target: the figure is the only place
/// where it is not obvious.
fn graph_context_menu(
    ui: &egui::Ui,
    view: &mut GraphView,
    tree: &Tree,
    graph_id: NodeId,
    actions: &mut Vec<Action>,
) {
    let Some(response) = view.response().cloned() else {
        return;
    };
    let hit_id = response.id.with("context_hit");
    if response.secondary_clicked() {
        let spec = stokesforge_core::compose::current_plot_spec(tree, graph_id);
        let pos = response.interact_pointer_pos();
        let component = pos.and_then(|pos| view.component_at(pos));
        let annotation = pos.and_then(|pos| view.annotation_at(pos));
        let shape = pos.and_then(|pos| view.shape_at(pos));
        let series = pos
            .filter(|_| matches!(component, None | Some(PlotComponent::PlotArea { .. })))
            .zip(spec.as_ref())
            .and_then(|(pos, spec)| view.series_at(pos, spec));
        let target = match (annotation, shape) {
            (Some(annotation), _) => PlotTarget::Annotation(annotation),
            (None, Some(shape)) => PlotTarget::Object(shape),
            (None, None) => plot_target(
                component,
                &|panel, side| match spec.as_ref() {
                    Some(spec) => PlotAxis::from_side(cell_sides::place_of(spec, panel, side)),
                    None => PlotAxis::from_side(side),
                },
                &|rank| {
                    view.last_drawn()
                        .and_then(|last| last.panels.get(rank))
                        .map_or(rank, |drawn| drawn.index)
                },
            ),
        };
        let object = if let Some(annotation) = annotation {
            Some(VisibilityTarget::Annotation(annotation))
        } else if let Some(shape) = shape {
            Some(VisibilityTarget::Shape(shape))
        } else if let Some(series) = series {
            Some(VisibilityTarget::Series(series))
        } else {
            match component {
                Some(PlotComponent::Text(text)) => Some(VisibilityTarget::Text(text)),
                Some(
                    PlotComponent::Axis { panel, side } | PlotComponent::AxisTitle { panel, side },
                ) => axis_target(view, panel, side),
                _ => None,
            }
        };
        // Title, axis title, legend or legend entry targeted (issue #53).
        let legend_entry = match component {
            Some(PlotComponent::LegendEntry { panel, series }) => view
                .last_drawn()
                .and_then(|last| last.panels.get(panel))
                .map(|drawn| ContextText::LegendEntry {
                    panel: drawn.index,
                    series,
                }),
            _ => None,
        };
        // A right-click on an object selects it, like a simple click.
        if let (Some(spec), Some(object)) = (spec.as_ref(), object) {
            select_manager_target(view, Some(object), spec);
        }
        let hit = ContextHit {
            target,
            object,
            legend_entry,
            pos,
            point: pos.and_then(|pos| view.data_at(pos)),
        };
        ui.memory_mut(|memory| memory.data.insert_temp(hit_id, hit));
    }
    let mut requests = Vec::new();
    let mut insert = None;
    response.context_menu(|ui| {
        let hit: ContextHit =
            ui.memory(|memory| memory.data.get_temp(hit_id))
                .unwrap_or(ContextHit {
                    target: PlotTarget::Background,
                    object: None,
                    legend_entry: None,
                    pos: None,
                    point: None,
                });
        ui.weak(hit.target.label());
        let spec = stokesforge_core::compose::current_plot_spec(tree, graph_id);
        match (hit.object, hit.legend_entry, spec.as_ref()) {
            (Some(object), _, Some(spec)) => object_node_menu(
                ui,
                graph_id,
                object,
                spec,
                stokesforge_app_state::object_manager::Shown::BASE,
                actions,
                &mut requests,
            ),
            (None, Some(entry), _) => {
                legend_entry_menu(ui, graph_id, entry, hit.target, actions, &mut requests)
            }
            _ => background_menu(ui, view, tree, graph_id, &hit, actions, &mut insert),
        }
    });
    let ctx = ui.ctx().clone();
    for request in requests {
        apply_object_request(&ctx, view, tree, graph_id, request, actions);
    }
    if let Some((pos, kind)) = insert {
        if let Some(spec) = stokesforge_core::compose::current_plot_spec(tree, graph_id) {
            view.insert_shape_at(pos, kind, &spec);
        }
    }
}

/// Legend entry: edit its text, then `Properties…` (Legend page).
fn legend_entry_menu(
    ui: &mut egui::Ui,
    graph_id: NodeId,
    entry: ContextText,
    target: PlotTarget,
    actions: &mut Vec<Action>,
    requests: &mut Vec<ObjectManagerRequest>,
) {
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        if ui.button(tr!("action-edit-text")).clicked() {
            requests.push(ObjectManagerRequest::EditText(entry));
            ui.close();
        }
    });
    sections.section(ui, true, |ui| {
        if ui
            .button(tr!("action-properties"))
            .on_hover_text(tr!("action-properties", "hint"))
            .clicked()
        {
            actions.push(target.open_settings(graph_id));
            ui.close();
        }
    });
}

/// Figure background and plot area: Add Text, Insert Object ▸, Export ▸,
/// then `Properties…` on the component's page.
fn background_menu(
    ui: &mut egui::Ui,
    view: &mut GraphView,
    tree: &Tree,
    graph_id: NodeId,
    hit: &ContextHit,
    actions: &mut Vec<Action>,
    insert: &mut Option<(egui::Pos2, stokesforge_render_egui::ShapeKind)>,
) {
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        if ui
            .add_enabled(
                hit.point.is_some(),
                egui::Button::new(tr!("action-add-text")),
            )
            .on_hover_text(tr!("action-add-text", "hint"))
            .on_disabled_hover_text(tr!("action-add-text", "disabled"))
            .clicked()
        {
            if let Some(point) = hit.point {
                add_annotation_at(ui.ctx(), view, tree, graph_id, point);
            }
            ui.close();
        }
        ui.menu_button(tr!("menu-insert-object"), |ui| {
            for (label, kind) in [
                (
                    tr!("tool-rectangle"),
                    stokesforge_render_egui::ShapeKind::Rectangle,
                ),
                (
                    tr!("tool-ellipse"),
                    stokesforge_render_egui::ShapeKind::Ellipse,
                ),
                (tr!("tool-line"), stokesforge_render_egui::ShapeKind::Line),
                (tr!("tool-arrow"), stokesforge_render_egui::ShapeKind::Arrow),
            ] {
                if ui
                    .add_enabled(hit.point.is_some(), egui::Button::new(label))
                    .on_disabled_hover_text(tr!("action-add-text", "disabled"))
                    .clicked()
                {
                    *insert = hit.pos.map(|pos| (pos, kind));
                    ui.close();
                }
            }
        });
    });
    sections.section(ui, true, |ui| {
        ui.menu_button(tr!("menu-export"), |ui| {
            if ui
                .add(
                    egui::Button::new(tr!("action-export-graph"))
                        .shortcut_text(ui.ctx().format_shortcut(&SHORTCUT_EXPORT_GRAPH)),
                )
                .on_hover_text(tr!("action-export-graph", "hint"))
                .clicked()
            {
                request_graph_export(Some(view), tree, graph_id, actions);
                ui.close();
            }
            if ui
                .button(tr!("action-copy-graph"))
                .on_hover_text(tr!("action-copy-graph", "hint"))
                .clicked()
            {
                if let Some(spec) = stokesforge_core::compose::current_plot_spec(tree, graph_id) {
                    actions.push(Action::CopyGraphFigure {
                        spec: Box::new(view.framed_spec(&spec)),
                    });
                }
                ui.close();
            }
        });
    });
    sections.section(ui, true, |ui| {
        if ui
            .button(tr!("action-properties"))
            .on_hover_text(tr!("action-properties", "hint"))
            .clicked()
        {
            actions.push(hit.target.open_settings(graph_id));
            ui.close();
        }
    });
}

/// Properties pages: a single set for Graph and its right-click.
pub(super) fn graph_properties_menu(ui: &mut egui::Ui, graph: NodeId, actions: &mut Vec<Action>) {
    for (label, hint, tab) in [
        (
            tr!("menu-graph-properties"),
            tr!("menu-graph-properties", "hint"),
            PlotSettingsTab::Graph,
        ),
        (
            tr!("menu-graph-plot-area"),
            tr!("menu-graph-plot-area", "hint"),
            PlotSettingsTab::PlotArea,
        ),
        (
            tr!("menu-graph-axes"),
            tr!("menu-graph-axes", "hint"),
            PlotSettingsTab::Axes,
        ),
        (
            tr!("menu-graph-series"),
            tr!("menu-graph-series", "hint"),
            PlotSettingsTab::Series,
        ),
        (
            tr!("menu-graph-legend"),
            tr!("menu-graph-legend", "hint"),
            PlotSettingsTab::Legend,
        ),
        (
            tr!("menu-graph-theme"),
            tr!("menu-graph-theme", "hint"),
            PlotSettingsTab::Theme,
        ),
    ] {
        if ui.button(label).on_hover_text(hint).clicked() {
            actions.push(Action::OpenPlotSettings {
                graph,
                tab,
                axis: None,
            });
            ui.close();
        }
    }
}

fn annotation_interactions(
    ui: &egui::Ui,
    view: &mut GraphView,
    tree: &Tree,
    graph: NodeId,
    actions: &mut Vec<Action>,
) {
    for event in view.take_events() {
        match event {
            stokesforge_render_egui::GraphEvent::PlaceText { panel, x, y } => {
                add_annotation_at(ui.ctx(), view, tree, graph, (panel, x, y));
                view.set_tool(stokesforge_render_egui::GraphTool::Pointer);
            }
            stokesforge_render_egui::GraphEvent::EditAnnotation(target) => {
                open_annotation_editor(ui.ctx(), view, tree, graph, target);
            }
            stokesforge_render_egui::GraphEvent::AnnotationMoved { target, annotation } => {
                actions.push(Action::SetAnnotation {
                    graph,
                    target,
                    annotation: Box::new(annotation),
                });
            }
            stokesforge_render_egui::GraphEvent::ShapeChanged { target, shape } => {
                actions.push(Action::SetShape {
                    graph,
                    target,
                    shape: Box::new(shape),
                });
            }
            stokesforge_render_egui::GraphEvent::EditText(target) => {
                open_text_editor(ui.ctx(), view, tree, graph, target);
            }
            stokesforge_render_egui::GraphEvent::EditLegendEntry { panel, series } => {
                open_legend_entry_editor(ui.ctx(), view, tree, graph, panel, series)
            }
            stokesforge_render_egui::GraphEvent::OpenAxisProperties { panel, side } => {
                if let Some(target) = axis_target(view, panel, side) {
                    actions.push(Action::OpenObjectProperties { graph, target });
                }
            }
            stokesforge_render_egui::GraphEvent::OpenSeriesProperties(series) => {
                actions.push(Action::OpenObjectProperties {
                    graph,
                    target: stokesforge_app_state::object_manager::VisibilityTarget::Series(series),
                });
            }
            stokesforge_render_egui::GraphEvent::OpenShapeProperties(shape) => {
                if let Some(spec) = stokesforge_core::compose::current_plot_spec(tree, graph) {
                    open_shape_properties(ui.ctx(), view, &spec, shape);
                }
            }
            stokesforge_render_egui::GraphEvent::TextResized { target, width } => {
                if let Some(mut font) = stokesforge_core::compose::current_plot_spec(tree, graph)
                    .and_then(|spec| target.font(&spec))
                {
                    font.width = Some(width);
                    actions.push(Action::SetTextFont {
                        graph,
                        target,
                        font,
                    });
                }
            }
            stokesforge_render_egui::GraphEvent::FrameResized { column, row } => {
                actions.push(Action::SetFrameSizes { graph, column, row });
            }
            stokesforge_render_egui::GraphEvent::FramePlaced { left, top } => {
                actions.push(Action::SetFigureOffsets { graph, left, top });
            }
            stokesforge_render_egui::GraphEvent::TextMoved { target, anchor } => {
                actions.push(Action::SetTextAnchor {
                    graph,
                    target,
                    anchor: Some(anchor),
                });
            }
        }
    }
    // The editor of a title or a legend name, if one is open.
    let text_editor: Option<TextEditor> = ui.ctx().data(|data| data.get_temp(text_editor_id(view)));
    if let Some(editor) = text_editor {
        text_editor_ui(ui.ctx(), view, graph, editor, actions);
        return;
    }
    let id = annotation_editor_id(view);
    let editor: Option<AnnotationEditor> = ui.ctx().data(|data| data.get_temp(id));
    match editor {
        Some(editor) => annotation_editor_ui(ui.ctx(), view, graph, editor, actions),
        None => {
            // Delete removes the selected text, when no field has
            // focus (otherwise the key belongs to the input).
            let typing = keyboard_focus_outside_grid(ui.ctx());
            if let Some(target) = view.selected_annotation() {
                if !typing
                    && ui
                        .ctx()
                        .input_mut(|input| input.consume_shortcut(&super::shortcuts::KEY_DELETE))
                {
                    actions.push(Action::RemoveAnnotation { graph, target });
                    view.clear_annotation_selection();
                }
            } else if let Some(target) = view.selected_shape() {
                // Graphic objects (issue #76): same gesture.
                if !typing
                    && ui
                        .ctx()
                        .input_mut(|input| input.consume_shortcut(&super::shortcuts::KEY_DELETE))
                {
                    actions.push(Action::RemoveShape { graph, target });
                    view.select_shape(None);
                }
            }
        }
    }
}

/// Asks where to export the graph, then hands the export to
/// [`AppState::dispatch`] (issue #34). The exported spec is the one the view
/// shows, zoom included ([`GraphView::framed_spec`]); without a view (graph not
/// yet painted), the saved framing. The format is read from the chosen
/// extension.
pub(super) fn request_graph_export(
    view: Option<&GraphView>,
    tree: &Tree,
    graph: NodeId,
    actions: &mut Vec<Action>,
) {
    let Some(spec) = stokesforge_core::compose::current_plot_spec(tree, graph) else {
        return;
    };
    let spec = match view {
        Some(view) => view.framed_spec(&spec),
        None => spec,
    };
    actions.push(Action::PickFile(
        stokesforge_app_state::file_picker::PickRequest::ExportGraph {
            spec: Box::new(spec),
            file_name: export_file_name(tree, graph),
        },
    ));
}

/// The rendered rank may differ from the rank in the spec (omitted panels):
/// address the axis by the layer's position, like the Object Manager (#113).
fn axis_target(view: &GraphView, panel: usize, side: AxisSide) -> Option<VisibilityTarget> {
    let drawn = view.last_drawn()?.panels.get(panel)?;
    Some(VisibilityTarget::Axis(AxisRef {
        panel: drawn.index,
        side: field_of(side),
    }))
}

/// The field of the layer that a hit-test side designates (#396): on an
/// overlaid layer the rendering reports the field it draws (its X axis as
/// `Bottom` at the top, its Y axis as `Left` on the right).
fn field_of(side: AxisSide) -> stokesforge_plot::AxisSide {
    match side {
        AxisSide::Bottom => stokesforge_plot::AxisSide::Bottom,
        AxisSide::Left => stokesforge_plot::AxisSide::Left,
        AxisSide::Right => stokesforge_plot::AxisSide::Right,
        AxisSide::Top => stokesforge_plot::AxisSide::Top,
    }
}

/// Label and tab of the component. The address of the axes is kept apart
/// by `axis_target`, to open the properties of the targeted layer.
/// `place` names an axis where it is drawn: it takes a layer of the spec (not
/// its rank among the drawn layers) and the field of that layer.
fn plot_target(
    component: Option<PlotComponent>,
    place: &dyn Fn(usize, stokesforge_plot::AxisSide) -> PlotAxis,
    drawn_index: &dyn Fn(usize) -> usize,
) -> PlotTarget {
    match component {
        None => PlotTarget::Background,
        Some(PlotComponent::Title) => PlotTarget::Title,
        Some(PlotComponent::PlotArea { .. }) => PlotTarget::PlotArea,
        Some(PlotComponent::Axis { panel, side }) => {
            PlotTarget::Axis(place(drawn_index(panel), field_of(side)))
        }
        Some(PlotComponent::AxisTitle { panel, side }) => {
            PlotTarget::AxisTitle(place(drawn_index(panel), field_of(side)))
        }
        // The context menu reads the free text through `GraphView::annotation_at`,
        // which gives its address in the spec; this fallback is not used.
        Some(PlotComponent::Annotation { .. }) => PlotTarget::PlotArea,
        // Same for a graphic object (`GraphView::shape_at`).
        Some(PlotComponent::Shape { .. }) => PlotTarget::PlotArea,
        // Titles and legend (issue #53): their properties are those of the
        // component, as before they could be designated in their own right.
        Some(PlotComponent::Text(target)) => match target {
            TextTarget::Title | TextTarget::PanelTitle { .. } => PlotTarget::Title,
            TextTarget::AxisTitle { panel, side } => PlotTarget::AxisTitle(place(panel, side)),
            TextTarget::Legend { .. } => PlotTarget::Legend,
        },
        Some(PlotComponent::LegendEntry { .. }) => PlotTarget::Legend,
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::test_support::xy_columns;
    use super::*;

    use stokesforge_app_state::state::ShapeOrder;
    use stokesforge_core::Project;
    use stokesforge_plot::{PanelSpec, PlotSpec, SeriesSpec, ShapeKind, ShapeRef, ShapeSpec};
    use stokesforge_session::Session;

    use super::super::project_manager::tests::{click_at, find_text_center};
    use super::super::tab::Tab;
    use super::super::window_chrome::{show_floating_window, window_header, HeaderPlacement};
    use super::super::window_placement::GRAPH_WINDOW_SIZE;

    /// A workbook of two columns and a graph titled "Figure" that plots
    /// them — the common project of the `app` tests that need a
    /// plotted graph (`format.rs`, `window_detach.rs`, `app.rs`).
    pub(in crate::app) fn plotted_graph() -> (Session, NodeId) {
        let mut project = Project::new("Graph").unwrap();
        let root = project.root();
        let (x, y) = xy_columns(&mut project);
        let mut panel = PanelSpec::at(0, 0);
        panel.series.push(SeriesSpec::new_xy(x, y));
        let graph = project
            .add_graph(root, None, PlotSpec::empty("Figure").with_panels(panel))
            .unwrap();
        (Session::spawn(project, None).unwrap(), graph)
    }
    /// #12: a background with no color or semi-transparent is displayed over
    /// white; an opaque background needs no underlay.
    #[test]
    fn the_display_backdrop_only_underlays_transparent_backgrounds() {
        assert_eq!(
            display_backdrop(stokesforge_plot::TRANSPARENT),
            Some(egui::Color32::WHITE)
        );
        assert_eq!(display_backdrop("#12345680"), Some(egui::Color32::WHITE));
        assert_eq!(display_backdrop("#123456"), None);
    }

    #[test]
    fn the_rendering_keeps_its_ratio_and_scales_into_the_available_area() {
        // F11: the graph keeps the ratio of `DEFAULT_FIGURE_SIZE` but is
        // scaled "contain" — maximized in the available area, taking
        // the limiting dimension (width or height) — rather than
        // painted at a fixed size and cropped. Point 3 of the report of 2026-09-24:
        // it is a **zoom** of the reference rendering, never a new layout
        // at the displayed size — the cached shapes must be
        // identical whatever the window size.
        let (session, graph) = plotted_graph();
        let snapshot = session.handle().snapshot().unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut view = GraphView::new();
        let (fig_w, fig_h) = DEFAULT_FIGURE_SIZE;
        let mut reference_shapes: Option<Vec<egui::Shape>> = None;

        for size in [
            GRAPH_WINDOW_SIZE,
            egui::vec2(320.0, 260.0),
            egui::vec2(200.0, 100.0),
        ] {
            let contents = egui::Rect::from_min_size(egui::pos2(300.0, 300.0), size);
            let visible = egui::Rect::from_min_max(
                contents.min,
                contents.max - egui::vec2(0.0, GRAPH_CONTROLS_HEIGHT),
            );
            let scale = (visible.width() / fig_w as f32).min(visible.height() / fig_h as f32);
            let expected_size = egui::vec2(fig_w as f32 * scale, fig_h as f32 * scale);
            for frame in 0..3 {
                // The wheel is below the visible area, outside the
                // figure and its controls: it must not zoom it.
                let pos = egui::pos2(visible.center().x, visible.bottom() + 10.0);
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 1200.0),
                    )),
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, 4.0),
                            modifiers: egui::Modifiers::NONE,
                            phase: egui::TouchPhase::Move,
                        },
                    ],
                    ..Default::default()
                };
                let misses = view.cache().misses();
                let mut output = context.run_ui(input, |ui| {
                    let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(contents));
                    content_ui.set_clip_rect(contents);
                    graph_view_contents(
                        &mut content_ui,
                        &mut view,
                        &snapshot,
                        graph,
                        &mut Vec::new(),
                    );
                });
                output.textures_delta.clear();
                let rect = view.cache().key().unwrap().rect;
                assert!(
                    (rect.size() - egui::vec2(fig_w as f32, fig_h as f32)).length() < 0.1,
                    "the layout must stay at the reference size, got {:?} for an \
                     area of {size:?}",
                    rect.size()
                );
                let display = view.display_rect().unwrap();
                assert!(
                    (display.size() - expected_size).length() < 0.1,
                    "expected displayed size for an area of {size:?}: {expected_size:?}, \
                     got {:?}",
                    display.size()
                );
                assert!(
                    visible.contains_rect(display),
                    "the scaled figure must fit in the visible area"
                );
                assert_eq!(
                    view.view().generation(),
                    0,
                    "interaction outside the figure and its controls"
                );
                if frame > 0 {
                    assert_eq!(view.cache().misses(), misses, "recomputed without change");
                }
                // Check the shapes actually sent to egui, not
                // only the rectangle computed before rendering.
                assert!(!view.cache().shapes().is_empty());
                if let Some(reference) = &reference_shapes {
                    assert_eq!(
                        view.cache().shapes(),
                        reference.as_slice(),
                        "the displayed size must not re-lay out the figure (area of {size:?})"
                    );
                } else {
                    reference_shapes = Some(view.cache().shapes().to_vec());
                }
                let transform = egui::emath::TSTransform::new(
                    display.min.to_vec2(),
                    display.width() / fig_w as f32,
                );
                for shape in view.cache().shapes() {
                    let mut displayed = shape.clone();
                    // The text is laid out again at the displayed size (#18):
                    // same transformation as `GraphView::paint_scaled`. A
                    // Typst text (titles, legend: #32) changes texture
                    // there, rasterized at the displayed size (#31): a
                    // mesh is thus compared on its geometry alone, to within a
                    // pixel: it is snapped to the grid of physical
                    // pixels on screen (#86).
                    stokesforge_render_egui::scaled::transform_shape(
                        &context,
                        &mut displayed,
                        transform,
                        &mut |_| None,
                    );
                    let same = |painted: &egui::Shape| match (painted, &displayed) {
                        (egui::Shape::Mesh(a), egui::Shape::Mesh(b)) => {
                            a.indices == b.indices
                                && a.vertices.len() == b.vertices.len()
                                && a.vertices.iter().zip(&b.vertices).all(|(p, q)| {
                                    (p.pos - q.pos).abs().max_elem() <= 1.0 && p.uv == q.uv
                                })
                        }
                        (a, b) => a == b,
                    };
                    assert!(
                        output.shapes.iter().any(|painted| same(&painted.shape)
                            && visible.contains_rect(painted.clip_rect)),
                        "graph shape outside the visible area"
                    );
                }
            }
        }
    }

    #[test]
    fn ctrl_pan_reaches_the_figure_under_the_drop_zone() {
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut graph_views = HashMap::new();
        let mut actions = Vec::new();
        let contents =
            egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(560.0, 540.0));

        let mut paint = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1200.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(contents));
                content_ui.set_clip_rect(contents);
                graph_drop_target(
                    &mut content_ui,
                    &state,
                    &mut actions,
                    &mut graph_views,
                    graph,
                );
            });
            output.textures_delta.clear();
        };

        let start = egui::pos2(380.0, 322.0);
        paint(vec![egui::Event::PointerMoved(start)]);
        let end = start + egui::vec2(30.0, -20.0);
        // Ctrl + drag: the graph pan (#318); a simple drag no longer
        // moves the view.
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: true,
            ..egui::Modifiers::NONE
        };
        paint(vec![
            egui::Event::ModifiersChanged(ctrl),
            egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: ctrl,
            },
        ]);
        paint(vec![egui::Event::PointerMoved(end)]);
        paint(vec![egui::Event::PointerMoved(
            end + egui::vec2(20.0, -10.0),
        )]);

        assert!(
            graph_views.get(&graph).unwrap().view().generation() > 0,
            "the drop target must not intercept the figure's pan"
        );
    }

    #[test]
    fn ctrl_pan_reaches_the_figure_in_the_floating_window() {
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut graph_views = HashMap::new();
        let mut actions = Vec::new();
        let tab = Tab::Graph(graph);
        let id = egui::Id::new(("content_window", tab));
        let panel_center = std::cell::Cell::new(None);

        let mut paint = |events: Vec<egui::Event>| {
            let is_initial_frame = events.is_empty();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1200.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                show_floating_window(
                    ui.ctx(),
                    id,
                    egui::Rect::from_min_size(egui::pos2(100.0, 100.0), GRAPH_WINDOW_SIZE),
                    ui.ctx().content_rect(),
                    true,
                    |ui| {
                        let _ = window_header(
                            ui,
                            id,
                            "Figure",
                            Some(tab),
                            false,
                            HeaderPlacement::Floating,
                        );
                        graph_drop_target(ui, &state, &mut actions, &mut graph_views, graph);
                    },
                    |_, _| {},
                );
            });
            output.textures_delta.clear();
            if is_initial_frame {
                let view = graph_views.get(&graph).unwrap();
                let panel = &view.last_drawn().unwrap().panels[0];
                let figure = view.display_rect().unwrap();
                let scale = figure.width() / DEFAULT_FIGURE_SIZE.0 as f32;
                panel_center.set(Some(egui::pos2(
                    figure.min.x
                        + (panel.pixel_origin.0 as f32 + panel.pixel_size.0 as f32 / 2.0) * scale,
                    figure.min.y
                        + (panel.pixel_origin.1 as f32 + panel.pixel_size.1 as f32 / 2.0) * scale,
                )));
            }
        };

        paint(Vec::new());
        let start = panel_center.get().unwrap();
        paint(vec![egui::Event::PointerMoved(start)]);
        let end = start + egui::vec2(30.0, -20.0);
        // Ctrl + drag: the graph pan (#318); a simple drag no longer
        // moves the view.
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: true,
            ..egui::Modifiers::NONE
        };
        paint(vec![
            egui::Event::ModifiersChanged(ctrl),
            egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: ctrl,
            },
        ]);
        paint(vec![egui::Event::PointerMoved(end)]);
        paint(vec![egui::Event::PointerMoved(
            end + egui::vec2(20.0, -10.0),
        )]);

        assert!(
            graph_views.get(&graph).unwrap().view().generation() > 0,
            "the floating window must not intercept the figure's pan"
        );
    }

    /// #113: both gestures must address the targeted layer and side.
    /// The row in the grid differs here from the layer index.
    #[test]
    fn axis_gestures_open_the_right_layer() {
        // `line`: the pointer is on the axis line, 2 points inside the edge
        // of the plot area (#385), instead of on its numbers.
        for (context_menu, line) in [(false, false), (true, false), (false, true), (true, true)] {
            let (session, graph) = plotted_graph();
            let mut state = AppState::new(session.handle(), "test.stf".into()).unwrap();
            let mut spec =
                stokesforge_core::compose::current_plot_spec(state.tree(), graph).unwrap();
            spec.panels.push(PanelSpec::at(0, 1));
            state.dispatch(Action::SetPlotSpec { graph, spec });
            let context = egui::Context::default();
            context.set_fonts(stokesforge_render_egui::font_definitions());
            let mut view = GraphView::new();
            let mut actions = Vec::new();
            {
                let mut paint = |events| {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900.0, 900.0),
                        )),
                        events,
                        ..Default::default()
                    };
                    let mut output = context.run_ui(input, |ui| {
                        graph_view_contents(ui, &mut view, state.snapshot(), graph, &mut actions);
                    });
                    output.textures_delta.clear();
                };
                paint(Vec::new());
            }
            let drawn = &view.last_drawn().unwrap().panels[1];
            let figure = view.response().unwrap().rect;
            let scale = figure.width() / DEFAULT_FIGURE_SIZE.0 as f32;
            let dx = if line { 2.0 } else { -5.0 * scale };
            let pos = figure.min
                + egui::vec2(
                    drawn.pixel_origin.0 as f32 * scale + dx,
                    (drawn.pixel_origin.1 as f32 + drawn.pixel_size.1 as f32 / 2.0) * scale,
                );
            assert_eq!(
                view.component_at(pos),
                Some(PlotComponent::Axis {
                    panel: 1,
                    side: AxisSide::Left
                })
            );
            let button = if context_menu {
                egui::PointerButton::Secondary
            } else {
                egui::PointerButton::Primary
            };
            let mut events = vec![egui::Event::PointerMoved(pos)];
            let presses: &[bool] = if context_menu {
                &[true, false]
            } else {
                &[true, false, true, false]
            };
            for &pressed in presses {
                events.push(egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let mut paint = |events| {
                let mut output = context.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        graph_view_contents(ui, &mut view, state.snapshot(), graph, &mut actions);
                    },
                );
                output.textures_delta.clear();
                output
            };
            paint(events);
            if context_menu {
                let output = paint(Vec::new());
                let properties = find_text_center(&output.shapes, &tr!("action-properties"));
                // The menu must keep the original layer when the pointer leaves it.
                click_at(&context, properties, egui::Modifiers::NONE, |_, events| {
                    paint(events)
                });
            }
            assert_eq!(actions.len(), 1);
            state.dispatch(actions.pop().unwrap());
            assert_eq!(state.plot_settings().unwrap().axis, PlotAxis::Left);
            assert_eq!(state.plot_edit().unwrap().selected_panel, 1);
        }
    }

    /// Each component designated by the rendering opens the properties tab that
    /// sets it; a click in the void opens the background.
    #[test]
    fn plot_target_maps_each_component_to_its_properties() {
        use stokesforge_render_egui::TextTarget as T;
        // An ordinary layer: a side is named by its own field.
        let named = |component| {
            plot_target(component, &|_, side| PlotAxis::from_side(side), &|rank| {
                rank
            })
        };
        assert_eq!(named(None), PlotTarget::Background);
        assert_eq!(named(Some(PlotComponent::Title)), PlotTarget::Title);
        assert_eq!(
            named(Some(PlotComponent::PlotArea { panel: 0 })),
            PlotTarget::PlotArea
        );
        for (side, axis) in [
            (AxisSide::Bottom, PlotAxis::Bottom),
            (AxisSide::Left, PlotAxis::Left),
            (AxisSide::Right, PlotAxis::Right),
            (AxisSide::Top, PlotAxis::Top),
        ] {
            assert_eq!(
                named(Some(PlotComponent::Axis { panel: 0, side })),
                PlotTarget::Axis(axis)
            );
            assert_eq!(
                named(Some(PlotComponent::AxisTitle { panel: 0, side })),
                PlotTarget::AxisTitle(axis)
            );
        }
        assert_eq!(
            named(Some(PlotComponent::Text(T::Title))),
            PlotTarget::Title
        );
        assert_eq!(
            named(Some(PlotComponent::Text(T::PanelTitle { panel: 0 }))),
            PlotTarget::Title
        );
        for (side, axis) in [
            (stokesforge_plot::AxisSide::Bottom, PlotAxis::Bottom),
            (stokesforge_plot::AxisSide::Left, PlotAxis::Left),
            (stokesforge_plot::AxisSide::Right, PlotAxis::Right),
        ] {
            assert_eq!(
                named(Some(PlotComponent::Text(T::AxisTitle { panel: 0, side }))),
                PlotTarget::AxisTitle(axis)
            );
        }
        assert_eq!(
            named(Some(PlotComponent::Text(T::Legend { panel: 0 }))),
            PlotTarget::Legend
        );
        assert_eq!(
            named(Some(PlotComponent::LegendEntry {
                panel: 0,
                series: 0
            })),
            PlotTarget::Legend
        );
    }

    /// #396: on an overlaid layer the rendering reports the field it draws; the
    /// menu header and the page that opens name the axis where it is drawn.
    #[test]
    fn an_overlaid_layer_axis_is_named_where_it_is_drawn() {
        use stokesforge_plot::{OverlaySpec, Panels};
        let base = PanelSpec::at(0, 0);
        let overlay = PanelSpec::overlay_on(&base, OverlaySpec::new(true, true));
        let spec = PlotSpec::empty("Figure").with_panels(Panels::new(vec![base, overlay]));
        let named = |component| {
            plot_target(
                component,
                &|panel, side| PlotAxis::from_side(cell_sides::place_of(&spec, panel, side)),
                &|rank| rank,
            )
        };
        let axis = |panel, side| PlotComponent::Axis { panel, side };
        assert_eq!(
            named(Some(axis(1, AxisSide::Bottom))),
            PlotTarget::Axis(PlotAxis::Top)
        );
        assert_eq!(
            named(Some(axis(1, AxisSide::Left))),
            PlotTarget::Axis(PlotAxis::Right)
        );
        assert_eq!(
            named(Some(axis(0, AxisSide::Bottom))),
            PlotTarget::Axis(PlotAxis::Bottom)
        );
        assert_eq!(
            PlotTarget::Axis(PlotAxis::Top).label(),
            tr!("plot-target-axis", side = "top")
        );
        assert_eq!(
            named(Some(PlotComponent::AxisTitle {
                panel: 1,
                side: AxisSide::Left
            })),
            PlotTarget::AxisTitle(PlotAxis::Right)
        );
    }

    /// #170: each badge is named like a layer and announces its
    /// selection; both states are exercised on each of the two layers.
    #[test]
    fn layer_badges_announce_their_number_and_selection() {
        for active in [0, 1] {
            let context = egui::Context::default();
            context.enable_accesskit();
            let mut spec = PlotSpec::empty("Figure");
            spec.panels.push(PanelSpec::at(0, 1));
            let mut view = GraphView::new();
            view.set_active_panel(active, &spec);
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(300.0, 200.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    layer_badges(
                        ui,
                        &mut view,
                        &spec,
                        NodeId::from_raw(2),
                        ui.available_rect_before_wrap(),
                    );
                },
            );
            output.textures_delta.clear();
            let mut badges: Vec<_> = output
                .platform_output
                .accesskit_update
                .expect("accessibility tree")
                .nodes
                .into_iter()
                .filter(|(_, node)| node.role() == egui::accesskit::Role::Button)
                .map(|(_, node)| {
                    assert!(!node.is_disabled());
                    let bounds = node.bounds().expect("badge rectangle");
                    assert!(bounds.x1 > bounds.x0 && bounds.y1 > bounds.y0);
                    (node.label().unwrap().to_owned(), node.toggled())
                })
                .collect();
            let mut expected: Vec<_> = (0..2)
                .map(|index| {
                    (
                        tr!("plot-layer-badge-accessible", n = (index + 1).to_string()),
                        Some(if index == active {
                            egui::accesskit::Toggled::True
                        } else {
                            egui::accesskit::Toggled::False
                        }),
                    )
                })
                .collect();
            // The publication order of AccessKit nodes is not that
            // of painting: compare the descriptions, without that order.
            badges.sort_by(|a, b| a.0.cmp(&b.0));
            expected.sort_by(|a, b| a.0.cmp(&b.0));
            assert_eq!(badges, expected);
        }
    }

    /// Paints the graph `spec` in a 900x700 window and returns the view and the
    /// AccessKit bounds of the layer badges by number (#395).
    fn painted_badges(
        spec: PlotSpec,
    ) -> (GraphView, std::collections::BTreeMap<usize, egui::Rect>) {
        let mut project = Project::new("Badges").unwrap();
        let root = project.root();
        let graph = project.add_graph(root, None, spec).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let snapshot = session.handle().snapshot().unwrap();
        let context = egui::Context::default();
        context.enable_accesskit();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut view = GraphView::new();
        let mut bounds = std::collections::BTreeMap::new();
        for _ in 0..2 {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    ..Default::default()
                },
                |ui| graph_view_contents(ui, &mut view, &snapshot, graph, &mut Vec::new()),
            );
            output.textures_delta.clear();
            bounds.clear();
            let nodes = output
                .platform_output
                .accesskit_update
                .expect("accessibility tree")
                .nodes;
            for (_, node) in nodes {
                if node.role() != egui::accesskit::Role::Button {
                    continue;
                }
                let label = node.label().unwrap_or_default().to_owned();
                for n in 1..=3 {
                    if label == tr!("plot-layer-badge-accessible", n = n.to_string()) {
                        let b = node.bounds().expect("badge rectangle");
                        bounds.insert(
                            n - 1,
                            egui::Rect::from_min_max(
                                egui::pos2(b.x0 as f32, b.y0 as f32),
                                egui::pos2(b.x1 as f32, b.y1 as f32),
                            ),
                        );
                    }
                }
            }
        }
        (view, bounds)
    }

    /// #395: with two cells side by side, each badge sits in its own cell, at
    /// its top left, not in the window corner.
    #[test]
    fn each_layer_badge_sits_at_the_top_left_of_its_cell() {
        use stokesforge_plot::Panels;
        let spec = PlotSpec::empty("Figure")
            .with_panels(Panels::new(vec![PanelSpec::at(0, 0), PanelSpec::at(0, 1)]));
        let (view, badges) = painted_badges(spec);
        for index in 0..2 {
            let cell = view.cell_rect_screen(index).expect("drawn");
            let badge = badges[&index];
            assert!(cell.contains_rect(badge), "badge {index} inside its cell");
            assert!(
                (badge.min - cell.left_top()).length() < 8.0,
                "badge {index} in the corner of its cell"
            );
        }
        let second = view.cell_rect_screen(1).unwrap();
        assert!(
            !second.contains(badges[&0].center()),
            "badge 1 not in cell 2"
        );
    }

    /// #395: overlays of one cell share its corner, side by side in layer order;
    /// a layer that is not drawn keeps the window corner.
    #[test]
    fn overlay_badges_sit_side_by_side_in_their_cell() {
        use stokesforge_plot::{OverlaySpec, Panels};
        let base = PanelSpec::at(0, 0);
        let overlay = PanelSpec::overlay_on(&base, OverlaySpec::new(true, true));
        let (view, badges) =
            painted_badges(PlotSpec::empty("Figure").with_panels(Panels::new(vec![base, overlay])));
        let cell = view.cell_rect_screen(0).unwrap();
        assert_eq!(view.cell_rect_screen(1), Some(cell), "same cell");
        assert_eq!(badges[&0].min.y, badges[&1].min.y);
        assert!(badges[&1].min.x > badges[&0].max.x - 0.5, "side by side");
        assert!(cell.contains_rect(badges[&0]) && cell.contains_rect(badges[&1]));
    }

    /// #395: a layer with no drawn cell (no figure painted yet) keeps its badge at
    /// the corner of the window, one slot per layer as before.
    #[test]
    fn an_undrawn_layer_badge_stays_at_the_window_corner() {
        use stokesforge_plot::Panels;
        let context = egui::Context::default();
        context.enable_accesskit();
        let spec = PlotSpec::empty("Figure")
            .with_panels(Panels::new(vec![PanelSpec::at(0, 0), PanelSpec::at(0, 1)]));
        let mut view = GraphView::new();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(300.0, 200.0),
                )),
                ..Default::default()
            },
            |ui| {
                layer_badges(
                    ui,
                    &mut view,
                    &spec,
                    NodeId::from_raw(2),
                    ui.available_rect_before_wrap(),
                );
            },
        );
        output.textures_delta.clear();
        let mut xs: Vec<(f64, f64)> = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .filter(|(_, n)| n.role() == egui::accesskit::Role::Button)
            .map(|(_, n)| {
                let b = n.bounds().unwrap();
                (b.x0, b.y0)
            })
            .collect();
        xs.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(xs.len(), 2);
        assert!(xs[0].0 < 10.0 && xs[0].1 < 10.0, "badge 1 at the corner");
        assert!(
            xs[1].0 > 20.0 && xs[1].0 < 35.0 && xs[1].1 < 10.0,
            "badge 2 beside it"
        );
    }

    /// #357: the right band gives a cell to the right, the bottom band a cell
    /// below, the bottom-right corner (in both) the right; the interior and
    /// the outside of the figure give nothing.
    #[test]
    fn the_edge_bands_of_the_figure_decide_where_the_new_cell_goes() {
        use stokesforge_core::compose::CellSide;
        let figure = egui::Rect::from_min_max(egui::pos2(100.0, 50.0), egui::pos2(500.0, 350.0));
        let band = 36.0;
        let side = |x: f32, y: f32| edge_drop_side(egui::pos2(x, y), figure, band);
        assert_eq!(side(300.0, 200.0), None, "interior");
        assert_eq!(side(480.0, 200.0), Some(CellSide::Right));
        assert_eq!(
            side(480.0, 60.0),
            Some(CellSide::Right),
            "top of the right band"
        );
        assert_eq!(side(300.0, 340.0), Some(CellSide::Below));
        assert_eq!(side(490.0, 340.0), Some(CellSide::Right), "corner: right");
        assert_eq!(side(300.0, 20.0), None, "outside");
        assert_eq!(side(520.0, 200.0), None, "outside");
        assert_eq!(
            edge_drop_side(egui::pos2(480.0, 200.0), figure, 0.0),
            None,
            "a band of width 0 catches nothing"
        );
    }

    /// #161: the numbered marker activates a layer, the column drop
    /// targets it, a layer that disappears falls back to the first.
    #[test]
    fn the_active_layer_follows_the_marker_and_targets_the_drop() {
        let (session, graph) = plotted_graph();
        let mut state = AppState::new(session.handle(), "test.stf".into()).unwrap();
        let mut spec = stokesforge_core::compose::current_plot_spec(state.tree(), graph).unwrap();
        spec.panels.push(PanelSpec::at(0, 1));
        state.dispatch(Action::SetPlotSpec { graph, spec });
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut view = GraphView::new();
        let paint = |view: &mut GraphView, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 900.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                graph_view_contents(ui, view, state.snapshot(), graph, &mut Vec::new());
            });
            output.textures_delta.clear();
        };
        let spec = stokesforge_core::compose::current_plot_spec(state.tree(), graph).unwrap();
        paint(&mut view, Vec::new());
        assert_eq!(view.active_panel(&spec), 0, "new graph: the first");

        let cell = view.cell_rect_screen(1).expect("layer 2 is drawn");
        let badge = cell.left_top() + egui::vec2(4.0 + 9.0, 4.0 + 9.0);
        let click = |pressed| egui::Event::PointerButton {
            pos: badge,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        paint(&mut view, vec![egui::Event::PointerMoved(badge)]);
        paint(&mut view, vec![click(true)]);
        paint(&mut view, vec![click(false)]);
        assert_eq!(view.active_panel(&spec), 1, "marker 2 activates layer 2");
        assert_eq!(published_active_panel(&context, graph), 1);

        let payload = ColumnDragPayload {
            sheet: NodeId::from_raw(7),
            columns: vec![NodeId::from_raw(8)],
        };
        assert_eq!(
            drop_payload_action(&payload, graph, published_active_panel(&context, graph)),
            drop_payload_action(&payload, graph, 1),
            "the drop targets the active layer"
        );

        let mut one = spec.clone();
        one.panels = stokesforge_plot::Panels::one(PanelSpec::at(0, 0));
        assert_eq!(view.active_panel(&one), 0, "layer gone: the first");
    }

    /// #161: an Object Manager layer node makes it active, without
    /// touching the selection; a nonexistent layer is ignored.
    #[test]
    fn an_object_manager_layer_node_activates_it() {
        let mut spec = stokesforge_plot::PlotSpec::empty("");
        spec.panels.push(PanelSpec::at(0, 1));
        let mut view = GraphView::new();
        select_manager_target(&mut view, Some(VisibilityTarget::Panel(1)), &spec);
        assert_eq!(view.active_panel(&spec), 1);
        select_manager_target(&mut view, Some(VisibilityTarget::Group(7)), &spec);
        assert_eq!(view.active_panel(&spec), 1, "nonexistent layer: unchanged");
        select_manager_target(&mut view, Some(VisibilityTarget::Axes(0)), &spec);
        assert_eq!(view.active_panel(&spec), 0);
    }

    /// Dragging columns from a workbook: released on the graph, it
    /// adds a series (one action); released elsewhere, nothing. Hovering with
    /// a payload outlines the graph, absent without a payload.
    #[test]
    fn dropping_columns_on_the_graph_adds_a_series_and_elsewhere_nothing() {
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let contents =
            egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(560.0, 540.0));
        let payload = ColumnDragPayload {
            sheet: NodeId::from_raw(7),
            columns: vec![NodeId::from_raw(8), NodeId::from_raw(9)],
        };
        let mut graph_views = HashMap::new();
        // One frame; `drag` sets the drag payload (the workbook does it
        // when the gesture starts, outside this bench).
        let mut paint = |events: Vec<egui::Event>, drag: bool| {
            let mut actions = Vec::new();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1200.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                if drag {
                    egui::DragAndDrop::set_payload(ui.ctx(), payload.clone());
                }
                // Through the real window frame: the drop border is the
                // window's, not a stroke inside the content (#386).
                show_floating_window(
                    ui.ctx(),
                    egui::Id::new("drop_bench_window"),
                    contents,
                    ui.ctx().content_rect(),
                    false,
                    |ui| {
                        graph_drop_target(ui, &state, &mut actions, &mut graph_views, graph);
                    },
                    |_, _| {},
                );
            });
            output.textures_delta.clear();
            (output, actions)
        };
        let outline = |output: &egui::FullOutput| {
            output.shapes.iter().any(|painted| {
                matches!(&painted.shape,
                    egui::Shape::Rect(rect) if rect.rect == contents && rect.stroke == crate::theme::drop_target_stroke())
            })
        };
        // No drop-coloured stroke anywhere else (nothing inside the content).
        let strokes_count = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .filter(|painted| {
                    matches!(&painted.shape,
                    egui::Shape::Rect(rect) if rect.stroke == crate::theme::drop_target_stroke())
                })
                .count()
        };
        let release = |pos| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        };

        let inside = egui::pos2(380.0, 322.0);
        // Warm-up: the window's layer answers the pointer from the second frame.
        paint(vec![egui::Event::PointerMoved(inside)], false);
        let (output, actions) = paint(vec![egui::Event::PointerMoved(inside)], false);
        assert!(!outline(&output), "no outline without a drag in progress");
        assert!(actions.is_empty());
        let (output, actions) = paint(vec![egui::Event::PointerMoved(inside)], true);
        assert!(
            outline(&output),
            "hovering with a payload outlines the graph window's border"
        );
        assert_eq!(strokes_count(&output), 1, "a single drop stroke");
        assert!(actions.is_empty(), "hovering alone adds nothing");
        let (_, actions) = paint(vec![release(inside)], false);
        assert_eq!(
            actions,
            vec![drop_payload_action(&payload, graph, 0)],
            "a release on the graph adds the series"
        );

        let outside = egui::pos2(900.0, 900.0);
        paint(vec![egui::Event::PointerMoved(outside)], true);
        let (_, actions) = paint(vec![release(outside)], false);
        assert!(actions.is_empty(), "released outside the graph, nothing");

        // #357: released on the right edge band, the columns make a cell
        // beside the one nearest the pointer; on the bottom band, below it.
        let right_edge = egui::pos2(contents.max.x - 8.0, 300.0);
        paint(vec![egui::Event::PointerMoved(right_edge)], true);
        let (_, actions) = paint(vec![release(right_edge)], false);
        assert_eq!(
            actions,
            vec![Action::AddCell {
                graph,
                panel: 0,
                side: stokesforge_core::compose::CellSide::Right,
                columns: payload.columns.clone(),
            }],
            "a release on the right band adds a cell to the right"
        );
        // The figure is centred in the window, its bottom is at about y = 560.
        let bottom_edge = egui::pos2(300.0, 550.0);
        paint(vec![egui::Event::PointerMoved(bottom_edge)], true);
        let (_, actions) = paint(vec![release(bottom_edge)], false);
        assert_eq!(
            actions,
            vec![Action::AddCell {
                graph,
                panel: 0,
                side: stokesforge_core::compose::CellSide::Below,
                columns: payload.columns.clone(),
            }],
            "a release on the bottom band adds a cell below"
        );

        // Rows (#166): the drop produces the rows action. Regression:
        // `take_payload::<ColumnDragPayload>` emptied the rows payload before
        // it was read, and nothing got plotted.
        let rows = RowDragPayload {
            sheet: NodeId::from_raw(7),
            rows: vec![0, 2],
        };
        egui::DragAndDrop::set_payload(&context, rows.clone());
        paint(vec![egui::Event::PointerMoved(inside)], false);
        let (_, actions) = paint(vec![release(inside)], false);
        assert_eq!(
            actions,
            vec![drop_rows_action(&rows, graph, 0)],
            "a release of rows on the graph plots the rows"
        );
    }

    #[test]
    fn right_click_on_a_shape_selects_before_the_submenu_and_keeps_its_target() {
        let mut project = Project::new("Menu target").unwrap();
        let mut spec = PlotSpec::empty("Shapes");
        for (a, b) in [(0.2, 0.4), (0.6, 0.8)] {
            spec.panels.get_mut(0).unwrap().shapes.push(ShapeSpec::new(
                ShapeKind::Rectangle,
                a,
                a,
                b,
                b,
            ));
        }
        let graph = project.add_graph(project.root(), None, spec).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stf".into()).unwrap();
        let object = ShapeRef { panel: 0, index: 0 };
        let other = ShapeRef { index: 1, ..object };
        for properties in [true, false] {
            let context = super::super::test_support::context();
            context.all_styles_mut(|style| style.animation_time = 0.0);
            let view = std::cell::RefCell::new(GraphView::new());
            let actions = std::cell::RefCell::new(Vec::new());
            let paint = |events| {
                super::super::test_support::frame_of(
                    &context,
                    egui::vec2(1200.0, 900.0),
                    events,
                    |ui| {
                        graph_view_contents(
                            ui,
                            &mut view.borrow_mut(),
                            state.snapshot(),
                            graph,
                            &mut actions.borrow_mut(),
                        );
                    },
                )
            };
            paint(Vec::new());
            paint(Vec::new());
            let pos = {
                let view = view.borrow();
                let shape = view.drawn_shape(object).unwrap();
                let display = view.display_rect().unwrap();
                let scale = display.width() / DEFAULT_FIGURE_SIZE.0 as f32;
                display.min
                    + egui::vec2(
                        shape.p1.0 as f32 * scale,
                        ((shape.p1.1 + shape.p2.1) / 2.0) as f32 * scale,
                    )
            };
            assert_eq!(
                view.borrow().shape_at(pos),
                Some(object),
                "witness: the click reaches the shape"
            );
            view.borrow_mut().select_shape(Some(other));
            paint(vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                paint(vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
            let output = paint(Vec::new());
            assert_eq!(
                view.borrow().selected_shape(),
                Some(object),
                "the selection precedes any submenu opening"
            );
            assert!(actions.borrow().is_empty());
            // The object's menu is flat: neither "Properties ▸" nor "Objects ▸".
            let label = if properties {
                tr!("action-properties")
            } else {
                tr!("action-bring-to-front")
            };
            let pos = find_text_center(&output.shapes, &label);
            click_at(&context, pos, egui::Modifiers::NONE, |_, events| {
                paint(events)
            });
            if properties {
                paint(Vec::new());
                assert!(actions.borrow().is_empty());
                let id = view.borrow().id().with("shape_properties");
                assert!(
                    context
                        .data(|data| data.get_temp::<ShapeProperties>(id))
                        .is_some(),
                    "Properties… opens the window of the targeted object"
                );
            } else {
                assert_eq!(
                    *actions.borrow(),
                    vec![Action::ReorderShape {
                        graph,
                        target: object,
                        order: ShapeOrder::ToFront,
                    }]
                );
            }
            assert!(!egui::Popup::is_any_open(&context));
        }
        session.shutdown().unwrap();
    }

    /// #152: the right-click opens the menu **of the targeted object**. On a curve,
    /// Show Data, the organizing gestures, Delete and Properties… — without
    /// Add Text, Export or Reset Zoom; on the background, Add Text, Insert Object ▸,
    /// Export ▸ and Properties… — without Show Data, Reset Zoom or "Properties ▸".
    #[test]
    fn right_click_on_the_figure_opens_the_curve_menu_or_the_background_menu() {
        use super::super::test_support::{context as new_context, frame_of};
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), "test.stf".into()).unwrap();
        let spec = stokesforge_core::compose::current_plot_spec(state.tree(), graph).unwrap();
        let series = stokesforge_plot::SeriesRef::new(0, 0);
        for on_curve in [true, false] {
            let context = new_context();
            context.all_styles_mut(|style| style.animation_time = 0.0);
            let view = std::cell::RefCell::new(GraphView::new());
            let actions = std::cell::RefCell::new(Vec::new());
            let paint = |events| {
                frame_of(&context, egui::vec2(1200.0, 900.0), events, |ui| {
                    graph_view_contents(
                        ui,
                        &mut view.borrow_mut(),
                        state.snapshot(),
                        graph,
                        &mut actions.borrow_mut(),
                    );
                })
            };
            paint(Vec::new());
            paint(Vec::new());
            // A pixel of the curve, or of the plot area outside any curve.
            let pos = {
                let view = view.borrow();
                let display = view.display_rect().unwrap();
                let mut found = None;
                'scan: for iy in 0..120 {
                    for ix in 0..160 {
                        let pos = display.min
                            + egui::vec2(
                                display.width() * ix as f32 / 160.0,
                                display.height() * iy as f32 / 120.0,
                            );
                        let hit = view.series_at(pos, &spec);
                        let area =
                            matches!(view.component_at(pos), Some(PlotComponent::PlotArea { .. }));
                        if area && (hit == Some(series)) == on_curve {
                            found = Some(pos);
                            break 'scan;
                        }
                    }
                }
                found.expect("a pixel of the targeted case")
            };
            paint(vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                paint(vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
            let output = paint(Vec::new());
            let painted = |label: &str| {
                output.shapes.iter().any(|painted| match &painted.shape {
                    egui::Shape::Text(shape) => shape.galley.job.text == label,
                    _ => false,
                })
            };
            let resets = output
                .shapes
                .iter()
                .filter(|painted| match &painted.shape {
                    egui::Shape::Text(shape) => shape.galley.job.text == tr!("action-reset-zoom"),
                    _ => false,
                })
                .count();
            assert_eq!(
                resets, 1,
                "Reset Zoom stays the graph's button, no longer a right-click entry"
            );
            assert!(!painted(&tr!("menu-properties")), "no \"Properties ▸\"");
            assert!(painted(&tr!("action-properties")), "Properties… direct");
            if on_curve {
                assert_eq!(view.borrow().selected_series(), Some(series));
                for present in [
                    tr!("menu-analysis"),
                    tr!("action-show-graph-data"),
                    tr!("action-rename-series"),
                    tr!("action-move-series-up"),
                    tr!("action-hide-content"),
                    tr!("action-delete-content"),
                ] {
                    assert!(painted(&present), "{present} on a curve");
                }
                for absent in [tr!("action-add-text"), tr!("menu-export")] {
                    assert!(!painted(&absent), "{absent} makes no sense on a curve");
                }
                let at = find_text_center(&output.shapes, &tr!("action-show-graph-data"));
                click_at(&context, at, egui::Modifiers::NONE, |_, events| {
                    paint(events)
                });
                assert_eq!(
                    *actions.borrow(),
                    vec![Action::ShowSeriesData { graph, series }]
                );
            } else {
                for present in [
                    tr!("action-add-text"),
                    tr!("menu-insert-object"),
                    tr!("menu-export"),
                ] {
                    assert!(painted(&present), "{present} on the background");
                }
                assert!(
                    !painted(&tr!("action-show-graph-data")),
                    "Show Data only on a curve"
                );
                let at = find_text_center(&output.shapes, &tr!("menu-insert-object"));
                click_at(&context, at, egui::Modifiers::NONE, |_, events| {
                    paint(events)
                });
                let output = paint(Vec::new());
                let at = find_text_center(&output.shapes, &tr!("tool-rectangle"));
                click_at(&context, at, egui::Modifiers::NONE, |_, events| {
                    paint(events)
                });
                paint(Vec::new());
                assert!(
                    actions
                        .borrow()
                        .iter()
                        .any(|action| matches!(action, Action::SetShape { .. })),
                    "the shape is placed at the clicked point: {:?}",
                    actions.borrow()
                );
            }
        }
        session.shutdown().unwrap();
    }

    /// Bring to front, send to back and deletion of a graphic object:
    /// one action each, and the selection follows the object to its new rank.
    #[test]
    fn object_order_and_deletion_go_through_its_single_menu() {
        let mut project = Project::new("Objects").unwrap();
        let root = project.root();
        let mut spec = PlotSpec::empty("Objects");
        for _ in 0..3 {
            spec.panels.get_mut(0).unwrap().shapes.push(ShapeSpec::new(
                ShapeKind::Rectangle,
                0.0,
                0.0,
                1.0,
                1.0,
            ));
        }
        let graph = project.add_graph(root, None, spec.clone()).unwrap();
        let _ = project.tree().clone();
        let object = ShapeRef { panel: 0, index: 1 };
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut actions = Vec::new();
        let mut requests = Vec::new();
        let mut paint = |context: &egui::Context, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 300.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                object_node_menu(
                    ui,
                    graph,
                    VisibilityTarget::Shape(object),
                    &spec,
                    stokesforge_app_state::object_manager::Shown::BASE,
                    &mut actions,
                    &mut requests,
                );
            });
            output.textures_delta.clear();
            output
        };
        let first = paint(&context, Vec::new());
        let front = find_text_center(&first.shapes, &tr!("action-bring-to-front"));
        let back = find_text_center(&first.shapes, &tr!("action-send-to-back"));
        let delete = find_text_center(&first.shapes, &tr!("action-delete-content"));

        click_at(&context, front, egui::Modifiers::NONE, &mut paint);
        click_at(&context, back, egui::Modifiers::NONE, &mut paint);
        click_at(&context, delete, egui::Modifiers::NONE, &mut paint);
        assert_eq!(
            actions,
            vec![
                Action::ReorderShape {
                    graph,
                    target: object,
                    order: ShapeOrder::ToFront,
                },
                Action::ReorderShape {
                    graph,
                    target: object,
                    order: ShapeOrder::ToBack,
                },
                Action::RemovePlotObject {
                    graph,
                    target: VisibilityTarget::Shape(object),
                },
            ]
        );
        let followed: Vec<_> = requests
            .iter()
            .filter_map(|request| match request {
                ObjectManagerRequest::Select(VisibilityTarget::Shape(shape)) => Some(shape.index),
                _ => None,
            })
            .collect();
        assert_eq!(
            followed,
            vec![2, 0],
            "the selection follows the moved object"
        );
    }

    /// The rank the selection takes after "bring to front" (the last) and
    /// "send to back" (the first), checked button by button.
    #[test]
    fn the_selection_takes_the_rank_of_front_or_back() {
        let mut project = Project::new("Objects").unwrap();
        let root = project.root();
        let mut spec = PlotSpec::empty("Objects");
        for _ in 0..3 {
            spec.panels.get_mut(0).unwrap().shapes.push(ShapeSpec::new(
                ShapeKind::Ellipse,
                0.0,
                0.0,
                1.0,
                1.0,
            ));
        }
        let graph = project.add_graph(root, None, spec.clone()).unwrap();
        let object = ShapeRef { panel: 0, index: 1 };
        for (label, expected) in [
            (tr!("action-bring-to-front"), 2),
            (tr!("action-send-to-back"), 0),
        ] {
            let context = egui::Context::default();
            context.set_fonts(stokesforge_render_egui::font_definitions());
            let mut requests = Vec::new();
            let mut paint = |context: &egui::Context, events: Vec<egui::Event>| {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 300.0),
                    )),
                    events,
                    ..Default::default()
                };
                let mut output = context.run_ui(input, |ui| {
                    object_node_menu(
                        ui,
                        graph,
                        VisibilityTarget::Shape(object),
                        &spec,
                        stokesforge_app_state::object_manager::Shown::BASE,
                        &mut Vec::new(),
                        &mut requests,
                    );
                });
                output.textures_delta.clear();
                output
            };
            let first = paint(&context, Vec::new());
            let at = find_text_center(&first.shapes, &label);
            click_at(&context, at, egui::Modifiers::NONE, &mut paint);
            assert!(
                requests.iter().any(|request| matches!(
                    request,
                    ObjectManagerRequest::Select(VisibilityTarget::Shape(shape))
                        if *shape == ShapeRef { index: expected, ..object }
                )),
                "{label}"
            );
        }
    }

    /// Delete removes the selected free text or object when no field
    /// has focus; during input, the key belongs to the field.
    #[test]
    fn delete_removes_the_selection_except_while_typing() {
        use stokesforge_render_egui::AnnotationRef;
        let mut project = Project::new("Objects").unwrap();
        let root = project.root();
        let mut spec = PlotSpec::empty("Objects");
        spec.panels.get_mut(0).unwrap().annotations.push(
            stokesforge_render_egui::AnnotationSpec::new("Peak", 0.0, 0.0),
        );
        spec.panels.get_mut(0).unwrap().shapes.push(ShapeSpec::new(
            ShapeKind::Line,
            0.0,
            0.0,
            1.0,
            1.0,
        ));
        let graph = project.add_graph(root, None, spec.clone()).unwrap();
        let tree = project.tree().clone();
        let text = AnnotationRef { panel: 0, index: 0 };
        let shape = ShapeRef { panel: 0, index: 0 };
        let delete = egui::Event::Key {
            key: egui::Key::Delete,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        // Two frames: the first sets (or not) the focus on a field, the
        // second sends Delete.
        let run = |view: &mut GraphView, typing: bool| {
            let context = egui::Context::default();
            let mut actions = Vec::new();
            let mut buffer = String::new();
            for events in [Vec::new(), vec![delete.clone()]] {
                let first = events.is_empty();
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 300.0),
                    )),
                    events,
                    ..Default::default()
                };
                let mut output = context.run_ui(input, |ui| {
                    let field = ui.text_edit_singleline(&mut buffer);
                    if typing && first {
                        field.request_focus();
                    }
                    annotation_interactions(ui, view, &tree, graph, &mut actions);
                });
                output.textures_delta.clear();
            }
            actions
        };

        let mut view = GraphView::new();
        view.select_annotation(Some(text), &spec);
        assert!(
            run(&mut view, true).is_empty(),
            "Delete belongs to the field"
        );
        assert_eq!(view.selected_annotation(), Some(text));
        assert_eq!(
            run(&mut view, false),
            vec![Action::RemoveAnnotation {
                graph,
                target: text
            }]
        );
        assert_eq!(view.selected_annotation(), None);

        let mut view = GraphView::new();
        view.select_shape(Some(shape));
        assert!(
            run(&mut view, true).is_empty(),
            "Delete belongs to the field"
        );
        assert_eq!(
            run(&mut view, false),
            vec![Action::RemoveShape {
                graph,
                target: shape
            }]
        );
        assert_eq!(view.selected_shape(), None);

        // Witness: without a selection, Delete removes nothing.
        assert!(run(&mut GraphView::new(), false).is_empty());
    }

    /// #209: the shortcuts of a **group** selected in the graph (#197)
    /// — Delete, F2, Ctrl+↑/↓ — target the group, not its first series;
    /// Ctrl+↑/↓ asks for nothing when the group is already at the edge, and an
    /// input in progress keeps its keys. Layer "A, [B C]".
    #[test]
    fn shortcuts_of_a_selected_group_target_the_group() {
        use super::super::object_manager_ui::group_rename_pending;
        use stokesforge_app_state::object_manager::VisibilityTarget;
        use stokesforge_plot::{SeriesGroupRef, SeriesRef};

        let mut project = Project::new("Groups").unwrap();
        let root = project.root();
        let (x, y) = xy_columns(&mut project);
        let mut panel = PanelSpec::at(0, 0);
        panel.series.push(SeriesSpec::new_xy(x, y));
        panel.push_series_batch(vec![SeriesSpec::new_xy(x, y), SeriesSpec::new_xy(x, y)]);
        let spec = PlotSpec::empty("Groups").with_panels(panel);
        let group = SeriesGroupRef::new(0, spec.panels[0].group_ids()[0]);
        let graph = project.add_graph(root, None, spec.clone()).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let snapshot = session.handle().snapshot().unwrap();

        let key = |key: egui::Key, modifiers: egui::Modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        // Two frames: the first paints the graph (and sets or not the focus
        // on a field), the second sends the key. Returns the actions and the
        // context, which keeps the name editor if any.
        let run = |view: &mut GraphView, event: egui::Event, typing: bool| {
            let context = egui::Context::default();
            context.set_fonts(stokesforge_render_egui::font_definitions());
            let mut actions = Vec::new();
            let mut buffer = String::new();
            for events in [Vec::new(), vec![event.clone()]] {
                let first = events.is_empty();
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    events,
                    ..Default::default()
                };
                let mut output = context.run_ui(input, |ui| {
                    let field = ui.text_edit_singleline(&mut buffer);
                    if typing && first {
                        field.request_focus();
                    }
                    graph_view_contents(ui, view, &snapshot, graph, &mut actions);
                });
                output.textures_delta.clear();
            }
            (actions, context)
        };
        let group_view = || {
            let mut view = GraphView::new();
            view.select_series_group(Some(group), &spec);
            assert_eq!(view.selected_group(), Some(group));
            view
        };

        let delete = key(egui::Key::Delete, egui::Modifiers::NONE);
        assert_eq!(
            run(&mut group_view(), delete.clone(), false).0,
            vec![Action::RemovePlotObject {
                graph,
                target: VisibilityTarget::SeriesGroup(group),
            }]
        );
        // Witness: the same key on the first series alone targets only it.
        let mut view = GraphView::new();
        view.select_series(Some(SeriesRef::new(0, 1)), &spec);
        assert_eq!(
            run(&mut view, delete.clone(), false).0,
            vec![Action::RemovePlotObject {
                graph,
                target: VisibilityTarget::Series(SeriesRef::new(0, 1)),
            }]
        );
        assert!(
            run(&mut group_view(), delete, true).0.is_empty(),
            "Delete belongs to the field"
        );

        assert_eq!(
            run(
                &mut group_view(),
                key(egui::Key::ArrowUp, egui::Modifiers::CTRL),
                false
            )
            .0,
            vec![Action::MoveSeriesGroup {
                graph,
                group,
                up: true
            }]
        );
        assert!(
            run(
                &mut group_view(),
                key(egui::Key::ArrowDown, egui::Modifiers::CTRL),
                false
            )
            .0
            .is_empty(),
            "the group is already at the bottom"
        );

        let mut f2_view = group_view();
        let (actions, context) = run(
            &mut f2_view,
            key(egui::Key::F2, egui::Modifiers::NONE),
            false,
        );
        assert!(actions.is_empty(), "F2 does not act on the model");
        // #206: the editor opens in the graph, not in the tree.
        assert!(!group_rename_pending(&context, graph, group));
        let editor: super::super::graph_text_edit::TextEditor = context
            .data(|data| data.get_temp(text_editor_id(&f2_view)))
            .expect("F2 opens the name editor in the graph");
        assert_eq!(
            editor.target,
            super::super::graph_text_edit::TextEditTarget::GroupName(group)
        );
        // Witness: F2 on a single series does not rename its group.
        let mut view = GraphView::new();
        view.select_series(Some(SeriesRef::new(0, 1)), &spec);
        let (_, context) = run(&mut view, key(egui::Key::F2, egui::Modifiers::NONE), false);
        assert!(!group_rename_pending(&context, graph, group));
        assert!(context
            .data(|data| data
                .get_temp::<super::super::graph_text_edit::TextEditor>(text_editor_id(&view)))
            .is_some_and(|editor| editor.target
                != super::super::graph_text_edit::TextEditTarget::GroupName(group)));
        session.shutdown().unwrap();
    }

    // -- Published bounds and grayed fields (#170, point 1; #221) -------------

    /// A graph with one curve (x on the abscissa, y = 1, 2, 3…) whose layer `adjust`
    /// tunes, painted for three frames; returns what is needed to read the published
    /// bounds and the geometry of the last figure.
    fn painted_graph(
        xs: &[f64],
        adjust: impl FnOnce(&mut PanelSpec),
    ) -> (Session, NodeId, egui::Context, GraphView) {
        let mut project = Project::new("Bounds").unwrap();
        let root = project.root();
        let book = project.add_empty_workbook(root, None).unwrap();
        let sheet = project.add_worksheet(book, None).unwrap();
        let ys: Vec<f64> = (1..=xs.len()).map(|n| n as f64).collect();
        let x = project
            .add_column(sheet, None, None, stokesforge_core::Column::from_f64(xs))
            .unwrap();
        let y = project
            .add_column(sheet, None, None, stokesforge_core::Column::from_f64(&ys))
            .unwrap();
        let mut panel = PanelSpec::at(0, 0);
        panel.series.push(SeriesSpec::new_xy(x, y));
        adjust(&mut panel);
        let graph = project
            .add_graph(root, None, PlotSpec::empty("Bounds").with_panels(panel))
            .unwrap();
        let session = Session::spawn(project, None).unwrap();
        let snapshot = session.handle().snapshot().unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut view = GraphView::new();
        let contents =
            egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(560.0, 540.0));
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1200.0),
                )),
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(contents));
                content_ui.set_clip_rect(contents);
                graph_view_contents(
                    &mut content_ui,
                    &mut view,
                    &snapshot,
                    graph,
                    &mut Vec::new(),
                );
            });
            output.textures_delta.clear();
        }
        (session, graph, context, view)
    }

    /// #170, point 1: the bounds that the properties window shows grayed out
    /// are those of the draw (`DrawnPanel`), published **in ascending order**
    /// even when the axis is inverted (the draw then returns them descending).
    #[test]
    fn published_bounds_are_those_of_the_draw_in_ascending_order() {
        let (session, graph, context, view) =
            painted_graph(&[0.0, 1.0, 2.0], |panel| panel.bottom.inverted = true);
        let drawn = &view.last_drawn().unwrap().panels[0];
        assert!(
            drawn.x_range.0 > drawn.x_range.1,
            "precondition: the inverted axis is drawn backwards, {:?}",
            drawn.x_range
        );
        let published = published_axis_bounds(&context, graph, 0).expect("published bounds");
        assert_eq!(
            published.bottom,
            Some((drawn.x_range.1, drawn.x_range.0)),
            "ascending despite the inversion"
        );
        assert_eq!(
            published.left,
            Some((
                drawn.y_range.0.min(drawn.y_range.1),
                drawn.y_range.0.max(drawn.y_range.1)
            ))
        );
        assert_eq!(published.right, None, "no series on the right");
        assert_eq!(published_axis_bounds(&context, graph, 7), None);
        session.shutdown().unwrap();
    }

    /// Paints the axes page (bottom axis) with the bounds published for
    /// `graph`, after moving the pointer to `pointer`.
    fn paint_axes(
        context: &egui::Context,
        graph: NodeId,
        panel: &mut PanelSpec,
        pointer: Option<egui::Pos2>,
    ) -> Vec<egui::epaint::ClippedShape> {
        let mut env = crate::color_picker::PickerEnv {
            project: Default::default(),
            user: Default::default(),
            requests: Vec::new(),
        };
        let mut axis = stokesforge_app_state::state::PlotAxis::Bottom;
        let mut spec = PlotSpec::empty("Figure")
            .with_panels(stokesforge_plot::Panels::new(vec![panel.clone()]));
        let events = pointer.map(egui::Event::PointerMoved).into_iter().collect();
        let shapes = super::super::test_support::image(context, events, |ui| {
            super::super::plot_settings::axes_settings_page(
                ui,
                &mut spec,
                0,
                &mut axis,
                published_axis_bounds(ui.ctx(), graph, 0),
                &mut env,
            );
        });
        if let Some(edited) = spec.panels.first() {
            *panel = edited.clone();
        }
        shapes
    }

    /// #170, point 1 (#344): under Auto, the bound fields show the
    /// computed bounds — for a logarithmic axis from 1 to 1000, the values and
    /// not their logarithms — and the Auto button, already active, is grayed out:
    /// hovering it says why.
    #[test]
    fn bound_fields_show_the_bounds_and_the_grayed_auto_button_says_why() {
        use super::super::test_support::texts;
        let (session, graph, drawn_context, _view) = painted_graph(&[1.0, 10.0, 1000.0], |panel| {
            panel.bottom.scale = stokesforge_plot::AxisScale::Log;
        });
        // The page context is the draw's: the bounds are published there.
        let context = drawn_context;
        crate::theme::install(&context);
        context.all_styles_mut(|style| style.interaction.tooltip_delay = 0.0);
        let mut panel = PanelSpec::at(0, 0);
        panel.bottom.scale = stokesforge_plot::AxisScale::Log;
        assert!(panel.bottom.auto_range, "precondition: automatic scale");
        // The form grid is measured on the first frame.
        paint_axes(&context, graph, &mut panel, None);
        let painted = texts(&paint_axes(&context, graph, &mut panel, None));
        assert!(
            painted.iter().any(|t| t.text == "1000"),
            "the computed upper bound is displayed: {:?}",
            painted.iter().map(|t| &t.text).collect::<Vec<_>>()
        );
        // The Auto button, already active: grayed out, and hovering says why.
        let reason = tr!("plot-axes-from-auto", "disabled");
        assert!(
            !painted.iter().any(|t| t.text == reason),
            "witness: without hovering, no explanation"
        );
        let button = painted
            .iter()
            .find(|t| t.text == tr!("plot-axes-from-auto"))
            .unwrap()
            .center;
        let mut seen = Vec::new();
        for _ in 0..4 {
            seen = texts(&paint_axes(&context, graph, &mut panel, Some(button)));
        }
        assert!(
            seen.iter().any(|t| t.text == reason),
            "hovering the grayed button says why: {:?}",
            seen.iter().map(|t| &t.text).collect::<Vec<_>>()
        );
        assert_eq!(
            panel.bottom.range_max, None,
            "nothing is written without a gesture"
        );
        session.shutdown().unwrap();
    }
}
