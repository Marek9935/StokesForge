//! The shell of the Page Layout Mode (#426): what surrounds the figure while
//! the mode is on. The mode itself (boxes, handles, drags, nudges) lives in
//! `stokesforge-render-egui` (`GraphView`); this module owns the rulers, the
//! settings strip, the keys, the snap state of a graph window (view state,
//! never saved) and the requests of the menu and of the toolbar.
//!
//! The mode is a **view**: entering or leaving it sends no action and adds no
//! undo step, whatever the placement of the graph. The "Grid mode" button of
//! the strip is the one thing in it that touches the model: it sends
//! [`Action::SetLayerPlacement`], one undo step of its own.

use std::collections::HashMap;

use eframe::egui;
use stokesforge_app_state::frame_sizes::{page_fit_available, SizeLock};
use stokesforge_app_state::quantities::Quantity;
use stokesforge_app_state::state::Action;
use stokesforge_app_state::tr;
use stokesforge_core::NodeId;
use stokesforge_plot::{FreePage, LayerPlacement, LengthUnit, PlotSpec, MIN_FRAME_SIZE};
use stokesforge_render::grid_gesture::splitter;
use stokesforge_render::pipeline::{FigureLayout, SizeAxis};
use stokesforge_render::{scaled_to_page, GraphBox, MIN_PAGE_EXTENT};
use stokesforge_render_egui::{GraphEvent, GraphView};

use super::grid::keyboard_focus_outside_grid;
use super::state_toggle::{paint_hover_outline, state_toggle, state_toggle_width};
use super::StokesForgeApp;
use crate::icons::{icon_button, Icon, IconRole};
use crate::number_field::number_field;

/// Width of the rulers, in screen points.
pub(super) const RULER_WIDTH: f32 = 18.0;

/// Least height reserved under the figure for the settings strip in the mode:
/// two rows of controls (more when they wrap, see [`controls_height`]).
pub(super) const LAYOUT_CONTROLS_HEIGHT: f32 = 62.0;

/// Pixels a labelled ruler tick keeps from the next one, at least.
const MIN_LABEL_GAP: f32 = 40.0;

// ---------------------------------------------------------------------------
// Published state and requests
// ---------------------------------------------------------------------------

fn layout_mode_id(graph: NodeId) -> egui::Id {
    egui::Id::new(("stokesforge_layout_mode", graph))
}

/// Whether the Page Layout Mode is on in `graph`, as its last painted view
/// published it: read by the Layout menu, which has no `GraphView`.
pub(super) fn published_layout_mode(ctx: &egui::Context, graph: NodeId) -> bool {
    ctx.data(|data| data.get_temp(layout_mode_id(graph)))
        .unwrap_or(false)
}

pub(super) fn publish_layout_mode(ctx: &egui::Context, graph: NodeId, on: bool) {
    ctx.data_mut(|data| data.insert_temp(layout_mode_id(graph), on));
}

fn toggle_requests_id() -> egui::Id {
    egui::Id::new("stokesforge_layout_mode_toggles")
}

/// Asks, from a menu, for the mode of `graph` to be toggled; the application
/// runs it at the start of the next frame ([`StokesForgeApp::run_layout_mode_requests`]).
pub(super) fn request_layout_mode_toggle(ctx: &egui::Context, graph: NodeId) {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Vec<NodeId>>(toggle_requests_id())
            .push(graph)
    });
    // The root runs the request. A detached window asks from its own
    // viewport, where a plain `request_repaint` would wake only that one and
    // leave the request waiting for the next click on the root.
    ctx.request_repaint_of(egui::ViewportId::ROOT);
    ctx.request_repaint();
}

fn foreground_id() -> egui::Id {
    egui::Id::new("stokesforge_layout_foreground_graph")
}

fn unit_id() -> egui::Id {
    egui::Id::new("stokesforge_layout_length_unit")
}

fn size_locks_id() -> egui::Id {
    egui::Id::new("stokesforge_layout_size_locks")
}

fn foreground_graph(ctx: &egui::Context) -> Option<NodeId> {
    ctx.data(|data| data.get_temp::<Option<NodeId>>(foreground_id()))
        .flatten()
}

pub(super) fn length_unit(ctx: &egui::Context) -> LengthUnit {
    ctx.data(|data| data.get_temp(unit_id()))
        .unwrap_or_default()
}

/// The ratio locks of the application, as the root published them this frame:
/// the strip of a detached graph window has no `AppState` of its own for them.
pub(super) fn published_size_lock(ctx: &egui::Context, graph: NodeId) -> Option<SizeLock> {
    ctx.data(|data| data.get_temp::<HashMap<NodeId, SizeLock>>(size_locks_id()))
        .and_then(|locks| locks.get(&graph).copied())
}

/// Snap and its step for a graph window in the mode: on by default, coarse.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SnapState {
    pub(super) on: bool,
    pub(super) fine: bool,
}

impl Default for SnapState {
    fn default() -> Self {
        Self {
            on: true,
            fine: false,
        }
    }
}

fn inspector_height_id(graph: NodeId) -> egui::Id {
    egui::Id::new(("stokesforge_layout_inspector_height", graph))
}

/// The height the strip used at the last frame, in whole points.
fn inspector_height(ctx: &egui::Context, graph: NodeId) -> f32 {
    ctx.data(|data| data.get_temp(inspector_height_id(graph)))
        .unwrap_or(0.0)
}

/// Stores the measured height; true if it changed.
fn set_inspector_height(ctx: &egui::Context, graph: NodeId, height: f32) -> bool {
    let changed = inspector_height(ctx, graph) != height;
    if changed {
        ctx.data_mut(|data| data.insert_temp(inspector_height_id(graph), height));
    }
    changed
}

/// The height reserved under the figure for the strip: at least
/// [`LAYOUT_CONTROLS_HEIGHT`], more when its rows wrap. Depends only on the
/// width of the strip, so the figure's rectangle is stable from frame to frame.
pub(super) fn controls_height(ctx: &egui::Context, graph: NodeId) -> f32 {
    LAYOUT_CONTROLS_HEIGHT.max(inspector_height(ctx, graph))
}

fn snap_id(graph: NodeId) -> egui::Id {
    egui::Id::new(("stokesforge_layout_snap", graph))
}

fn snap_state(ctx: &egui::Context, graph: NodeId) -> SnapState {
    ctx.data(|data| data.get_temp(snap_id(graph)))
        .unwrap_or_default()
}

/// The snap state as the strip left it (read by tests).
#[cfg(test)]
pub(super) fn published_snap(ctx: &egui::Context, graph: NodeId) -> SnapState {
    snap_state(ctx, graph)
}

fn set_snap_state(ctx: &egui::Context, graph: NodeId, state: SnapState) {
    ctx.data_mut(|data| data.insert_temp(snap_id(graph), state));
}

/// The grid step in points: coarse / fine = 5 mm / 1 mm (cm, mm), 10 pt /
/// 1 pt, 1/4 in / 1/16 in.
pub(super) fn grid_step_points(unit: LengthUnit, fine: bool) -> f64 {
    match (unit, fine) {
        (LengthUnit::Cm | LengthUnit::Mm, false) => LengthUnit::Mm.to_points(5.0),
        (LengthUnit::Cm | LengthUnit::Mm, true) => LengthUnit::Mm.to_points(1.0),
        (LengthUnit::Pt, false) => 10.0,
        (LengthUnit::Pt, true) => 1.0,
        (LengthUnit::In, false) => LengthUnit::In.to_points(0.25),
        (LengthUnit::In, true) => LengthUnit::In.to_points(1.0 / 16.0),
    }
}

/// Feeds the view of the frame: the unit of the sizes written on the boxes, the
/// snapping grid.
pub(super) fn feed_view(ctx: &egui::Context, view: &mut GraphView, graph: NodeId) {
    let unit = length_unit(ctx);
    let snap = snap_state(ctx, graph);
    view.set_length_unit(unit);
    view.set_layout_grid(snap.on.then(|| grid_step_points(unit, snap.fine)));
}

// ---------------------------------------------------------------------------
// The application side: toggle, publication
// ---------------------------------------------------------------------------

/// Publishes what the graph windows read without an `AppState` of their own.
fn publish_app_state(
    ctx: &egui::Context,
    foreground: Option<NodeId>,
    unit: LengthUnit,
    locks: HashMap<NodeId, SizeLock>,
) {
    ctx.data_mut(|data| {
        data.insert_temp(foreground_id(), foreground);
        data.insert_temp(unit_id(), unit);
        data.insert_temp(size_locks_id(), locks);
    });
}

impl StokesForgeApp {
    /// Publishes what the graph windows need to know about the application
    /// (the foreground graph, the length unit, the ratio locks) and runs the
    /// toggles the menus asked for.
    pub(super) fn run_layout_mode_requests(&mut self, ctx: &egui::Context, graph: Option<NodeId>) {
        publish_app_state(
            ctx,
            graph,
            self.state.length_unit(),
            self.state.size_locks().clone(),
        );
        let requests: Vec<NodeId> = ctx.data_mut(|data| {
            std::mem::take(data.get_temp_mut_or_default::<Vec<NodeId>>(toggle_requests_id()))
        });
        for graph in requests {
            self.toggle_layout_mode(graph, None);
        }
    }
}

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

fn field_focus_id(ctx: &egui::Context) -> egui::Id {
    egui::Id::new(("stokesforge_layout_field_focus", ctx.viewport_id()))
}

/// Records, at the **end** of a frame, whether a text field of this viewport
/// has the keyboard. egui drops the focus on Esc at the start of the pass, so
/// by the time the keys are read a field that was being typed in no longer
/// looks focused: this mark of the previous frame is what tells that the Esc is
/// the field's to give back (S-F7). Called by the root and by each detached
/// viewport once it has painted.
pub(super) fn note_field_focus(ctx: &egui::Context) {
    let focused = ctx.text_edit_focused();
    let id = field_focus_id(ctx);
    ctx.data_mut(|data| data.insert_temp(id, focused));
}

pub(super) fn field_had_focus(ctx: &egui::Context) -> bool {
    let id = field_focus_id(ctx);
    ctx.data(|data| data.get_temp(id)).unwrap_or(false)
}

/// Esc leaves the mode, `G` turns Snap on or off, the arrows nudge (Shift: ten
/// steps). Consumed for the foreground graph only, **before** any window is
/// painted (a workbook painted behind the graph would take the Esc first, #406),
/// and only when nothing else owns the keys: no modal window, no popup, no text
/// field with the keyboard (nor one that had it a frame ago: the Esc that
/// gives a field back is not the Esc that leaves). Called by the root for a
/// floating graph and at the top of the viewport of a detached one; the edit a
/// leaving commits is pushed to `actions`. Returns whether the mode was left.
/// A dialog open in the main window has already removed the keys of a detached
/// viewport (`window_band::stop_keys`, #460), so nothing is read here then.
pub(super) fn handle_keys(
    ctx: &egui::Context,
    view: &mut GraphView,
    graph: NodeId,
    actions: &mut Vec<Action>,
) -> bool {
    if !view.layout_mode()
        || foreground_graph(ctx) != Some(graph)
        || super::dialogs::modal_shown(ctx)
        || keyboard_focus_outside_grid(ctx)
        || field_had_focus(ctx)
        || super::project_manager::owns_keyboard(ctx)
        || egui::Popup::is_any_open(ctx)
    {
        return false;
    }
    use super::shortcuts::{
        KEY_ARROW_DOWN, KEY_ARROW_LEFT, KEY_ARROW_RIGHT, KEY_ARROW_UP, KEY_ESCAPE, KEY_LAYOUT_SNAP,
        KEY_SHIFT_ARROW_DOWN, KEY_SHIFT_ARROW_LEFT, KEY_SHIFT_ARROW_RIGHT, KEY_SHIFT_ARROW_UP,
    };
    let (escape, snap, steps, now) = ctx.input_mut(|input| {
        let escape = input.consume_shortcut(&KEY_ESCAPE);
        let snap = input.consume_shortcut(&KEY_LAYOUT_SNAP);
        let mut steps = (0i32, 0i32);
        // Shift first: a plain arrow shortcut would also match a shifted key.
        for (shortcut, dx, dy, amount) in [
            (&KEY_SHIFT_ARROW_LEFT, -1, 0, 10),
            (&KEY_SHIFT_ARROW_RIGHT, 1, 0, 10),
            (&KEY_SHIFT_ARROW_UP, 0, -1, 10),
            (&KEY_SHIFT_ARROW_DOWN, 0, 1, 10),
            (&KEY_ARROW_LEFT, -1, 0, 1),
            (&KEY_ARROW_RIGHT, 1, 0, 1),
            (&KEY_ARROW_UP, 0, -1, 1),
            (&KEY_ARROW_DOWN, 0, 1, 1),
        ] {
            if input.consume_shortcut(shortcut) {
                steps.0 += dx * amount;
                steps.1 += dy * amount;
            }
        }
        (escape, snap, steps, input.time)
    });
    if escape {
        leave_mode(view, graph, actions);
        return true;
    }
    if snap {
        let mut state = snap_state(ctx, graph);
        state.on = !state.on;
        set_snap_state(ctx, graph, state);
        feed_view(ctx, view, graph);
    }
    if steps != (0, 0) {
        view.nudge(steps, now);
    }
    false
}

/// Leaves the mode of `view`, committing a nudge in progress: the edit is
/// pushed to `actions` as the one `SetFreeLayout` it is.
pub(super) fn leave_mode(view: &mut GraphView, graph: NodeId, actions: &mut Vec<Action>) {
    view.set_layout_mode(false);
    for event in view.take_events() {
        if let GraphEvent::LayoutEdited { frames, page } = event {
            actions.push(Action::SetFreeLayout {
                graph,
                frames,
                page,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Rulers
// ---------------------------------------------------------------------------

/// The areas of a graph window in the mode: the two ruler strips and what is
/// left for the figure.
pub(super) struct Strips {
    pub top: egui::Rect,
    pub left: egui::Rect,
    pub corner: egui::Rect,
    pub figure: egui::Rect,
}

pub(super) fn split_strips(area: egui::Rect) -> Strips {
    let r = RULER_WIDTH;
    Strips {
        corner: egui::Rect::from_min_size(area.min, egui::vec2(r, r)),
        top: egui::Rect::from_min_max(
            egui::pos2(area.min.x + r, area.min.y),
            egui::pos2(area.max.x.max(area.min.x + r), area.min.y + r),
        ),
        left: egui::Rect::from_min_max(
            egui::pos2(area.min.x, area.min.y + r),
            egui::pos2(area.min.x + r, area.max.y.max(area.min.y + r)),
        ),
        figure: egui::Rect::from_min_max(
            area.min + egui::vec2(r, r),
            egui::pos2(
                area.max.x.max(area.min.x + r),
                area.max.y.max(area.min.y + r),
            ),
        ),
    }
}

/// A step in `unit`s of at least `minimum`, from 1, 2, 5 × 10^k, and the
/// number of minor intervals in it.
fn nice_step(minimum: f64) -> (f64, i64) {
    if !minimum.is_finite() || minimum <= 0.0 {
        return (1.0, 5);
    }
    let base = 10f64.powi(minimum.log10().floor() as i32);
    for mantissa in [1.0, 2.0, 5.0, 10.0] {
        if mantissa * base >= minimum * 0.999_999 {
            let subs = if mantissa == 2.0 { 4 } else { 5 };
            return (mantissa * base, subs);
        }
    }
    (10.0 * base, 5)
}

/// Paints the two rulers: unit of the Preferences, zero at the page top-left,
/// the scale of the figure on screen; the page extent light, outside darker,
/// the selected box marked by a band of the selection colour.
pub(super) fn paint_rulers(ui: &egui::Ui, strips: &Strips, view: &GraphView, unit: LengthUnit) {
    let painter = ui.painter();
    let visuals = ui.visuals();
    painter.rect_filled(
        strips.corner,
        0.0,
        crate::theme::darker(visuals.window_fill()),
    );
    let (Some(page), Some(scale)) = (view.layout_image(), view.screen_scale()) else {
        painter.rect_filled(strips.top, 0.0, crate::theme::darker(visuals.window_fill()));
        painter.rect_filled(
            strips.left,
            0.0,
            crate::theme::darker(visuals.window_fill()),
        );
        return;
    };
    let (Some(start), Some(end)) = (
        view.stored_to_screen(page.left, page.top),
        view.stored_to_screen(page.left + page.width, page.top + page.height),
    ) else {
        return;
    };
    let band = view.selected_box().and_then(|selected| {
        let o = selected.outer;
        let a = view.stored_to_screen(o.left, o.top)?;
        let b = view.stored_to_screen(o.left + o.width, o.top + o.height)?;
        Some((a, b))
    });
    paint_ruler(
        ui,
        strips.top,
        true,
        (start.x, end.x),
        scale,
        unit,
        band.map(|(a, b)| (a.x, b.x)),
    );
    paint_ruler(
        ui,
        strips.left,
        false,
        (start.y, end.y),
        scale,
        unit,
        band.map(|(a, b)| (a.y, b.y)),
    );
}

fn paint_ruler(
    ui: &egui::Ui,
    strip: egui::Rect,
    horizontal: bool,
    (zero, end): (f32, f32),
    scale: f32,
    unit: LengthUnit,
    band: Option<(f32, f32)>,
) {
    let painter = ui.painter().with_clip_rect(strip);
    let visuals = ui.visuals();
    let inside = visuals.window_fill();
    let along = |value: f32| {
        if horizontal {
            egui::pos2(value, strip.min.y)
        } else {
            egui::pos2(strip.min.x, value)
        }
    };
    let span = |from: f32, to: f32| {
        if horizontal {
            egui::Rect::from_min_max(
                egui::pos2(from, strip.min.y),
                egui::pos2(to.max(from), strip.max.y),
            )
        } else {
            egui::Rect::from_min_max(
                egui::pos2(strip.min.x, from),
                egui::pos2(strip.max.x, to.max(from)),
            )
        }
    };
    painter.rect_filled(strip, 0.0, crate::theme::darker(inside));
    painter.rect_filled(span(zero, end), 0.0, inside);
    if let Some((from, to)) = band {
        let (from, to) = (from.min(to), from.max(to));
        painter.rect_filled(
            span(from, to),
            0.0,
            visuals.selection.bg_fill.gamma_multiply(0.7),
        );
    }
    let (lo, hi) = if horizontal {
        (strip.min.x, strip.max.x)
    } else {
        (strip.min.y, strip.max.y)
    };
    let per_unit = unit.to_points(1.0) as f32 * scale;
    if !(per_unit.is_finite() && per_unit > 0.0) {
        return;
    }
    let (step, subs) = nice_step(f64::from(MIN_LABEL_GAP / per_unit));
    let minor = step / subs as f64;
    let first = (f64::from((lo - zero) / per_unit) / minor).floor() as i64;
    let last = (f64::from((hi - zero) / per_unit) / minor).ceil() as i64;
    if last - first > 4000 {
        return;
    }
    let decimals = if step < 1.0 {
        (-step.log10()).ceil().clamp(0.0, 4.0) as usize
    } else {
        0
    };
    let stroke = egui::Stroke::new(1.0, visuals.text_color().gamma_multiply(0.6));
    let text_color = visuals.weak_text_color();
    let font = egui::FontId::proportional(8.5);
    for k in first..=last {
        let value = k as f64 * minor;
        let position = zero + value as f32 * per_unit;
        if position < lo - 1.0 || position > hi + 1.0 {
            continue;
        }
        let major = k % subs == 0;
        let length = if major { 7.0 } else { 3.5 };
        let (from, to) = if horizontal {
            (
                egui::pos2(position, strip.max.y - length),
                egui::pos2(position, strip.max.y),
            )
        } else {
            (
                egui::pos2(strip.max.x - length, position),
                egui::pos2(strip.max.x, position),
            )
        };
        painter.line_segment([from, to], stroke);
        if major {
            let shown = if value.abs() < 1e-9 { 0.0 } else { value };
            let label = format!("{shown:.decimals$}");
            let anchor = along(position) + egui::vec2(2.0, 1.0);
            let anchor = if horizontal {
                anchor
            } else {
                egui::pos2(strip.min.x + 1.0, position + 1.0)
            };
            painter.text(
                anchor,
                egui::Align2::LEFT_TOP,
                label,
                font.clone(),
                text_color,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Settings strip
// ---------------------------------------------------------------------------

fn length_quantity(unit: LengthUnit, min_points: f64, max_points: f64) -> Quantity {
    Quantity::real(unit.step(), unit.step() * 10.0, unit.decimals())
        .between(unit.from_points(min_points), unit.from_points(max_points))
        .short()
}

/// A length as the strip shows it: rounded to the unit's decimals.
fn shown(value: f64, unit: LengthUnit) -> f64 {
    let factor = 10f64.powi(unit.decimals() as i32);
    (value * factor).round() / factor
}

fn within(value: f64, low: f64, high: f64) -> f64 {
    value.min(high.max(low)).max(low)
}

/// The box `selected` given the outer rectangle `(x, y, width, height)` read in
/// the fields (from the page top-left, in points), held inside `page`.
fn box_in_page(selected: &GraphBox, page: FreePage, fields: (f64, f64, f64, f64)) -> FreePage {
    let (l, t, r, b) = selected.gutters;
    let (min_w, min_h) = (l + r + MIN_FRAME_SIZE, t + b + MIN_FRAME_SIZE);
    let width = within(fields.2, min_w, page.width);
    let height = within(fields.3, min_h, page.height);
    FreePage {
        left: within(
            page.left + fields.0,
            page.left,
            page.left + page.width - width,
        ),
        top: within(
            page.top + fields.1,
            page.top,
            page.top + page.height - height,
        ),
        width,
        height,
    }
}

/// The cell of `selected` in a regular grid, and whether its width and its
/// height have a neighbour to trade with: `((column, row), (width, height))`.
/// `None` when the layout has no frame to edit (compact fallback).
fn regular_cell(
    layout: &FigureLayout,
    spec: &PlotSpec,
    selected: &GraphBox,
) -> Option<((usize, usize), (bool, bool))> {
    let columns = layout.column_breaks.len().checked_sub(1)?;
    let rows = layout.row_breaks.len().checked_sub(1)?;
    if layout.frame_widths.len() != columns || layout.frame_heights.len() != rows {
        return None;
    }
    let panel = spec.panels.get(selected.base)?;
    Some(((panel.column, panel.row), (columns >= 2, rows >= 2)))
}

/// One axis after the frame `index` grew by `delta` points at the expense of
/// its right neighbour (the last frame: of its left one): the figure keeps its
/// size. `None` without a neighbour.
fn traded(sizes: &[f64], index: usize, delta: f64) -> Option<Vec<f64>> {
    if sizes.len() < 2 || index >= sizes.len() {
        return None;
    }
    Some(if index + 1 < sizes.len() {
        splitter(sizes, index, delta)
    } else {
        splitter(sizes, index - 1, -delta)
    })
}

/// The frame widths and heights of a regular grid once the outer rectangle of
/// `selected` is typed to `wanted` (width, height): the difference is traded
/// with a neighbour, the figure keeps its size. `None` if nothing can be traded.
fn regular_resize(
    layout: &FigureLayout,
    spec: &PlotSpec,
    selected: &GraphBox,
    wanted: (f64, f64),
) -> Option<(Vec<f64>, Vec<f64>)> {
    let ((column, row), _) = regular_cell(layout, spec, selected)?;
    let widths = if (wanted.0 - selected.outer.width).abs() > 1e-9 {
        traded(
            &layout.frame_widths,
            column,
            wanted.0 - selected.outer.width,
        )?
    } else {
        layout.frame_widths.clone()
    };
    let heights = if (wanted.1 - selected.outer.height).abs() > 1e-9 {
        traded(&layout.frame_heights, row, wanted.1 - selected.outer.height)?
    } else {
        layout.frame_heights.clone()
    };
    Some((widths, heights))
}

/// Commits a pending arrow-key burst now and turns it into its action, so that
/// it reaches the model **before** whatever the strip sends next (a placement
/// change would otherwise drop it).
fn flush_layout_edit(view: &mut GraphView, graph: NodeId, actions: &mut Vec<Action>) {
    view.commit_layout_edit();
    for event in view.take_events() {
        // Committing a burst yields nothing else; the events of the frame were
        // taken before the strip was painted.
        if let GraphEvent::LayoutEdited { frames, page } = event {
            actions.push(Action::SetFreeLayout {
                graph,
                frames,
                page,
            });
        }
    }
}

/// The settings strip under the figure: the figure size and its lock, the
/// numbers of the selected graph, the arrangement menus, Snap, the grid mode
/// and what happens when axes are added. Every numeric field commits on Enter,
/// on leaving the field or at a step (never each frame). What touches the model
/// goes after a pending burst of arrow keys ([`flush_layout_edit`]).
///
/// `selected` is the box that was selected **before** the canvas was painted
/// this frame: a click on another box on the canvas takes effect on the next
/// frame, so a number typed for the old box is validated on it (#498).
pub(super) fn inspector(
    ui: &mut egui::Ui,
    view: &mut GraphView,
    spec: &PlotSpec,
    layout: Option<&FigureLayout>,
    graph: NodeId,
    selected: Option<GraphBox>,
    actions: &mut Vec<Action>,
) {
    let ctx = ui.ctx().clone();
    let active = view.active_panel(spec);
    let extra = view.extra_layers(spec);
    let mut model: Vec<Action> = Vec::new();
    // Both rows wrap in a narrow window; the strip height follows the content.
    let top = ui.cursor().top();
    ui.horizontal_wrapped(|ui| {
        measures_row(ui, view, spec, layout, graph, selected, &mut model, actions)
    });
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        arrange_row(ui, &ctx, spec, graph, active, &extra, &mut model);
    });
    // Whole points, so that float noise never moves the figure's rectangle; a
    // little room under the last row keeps it off the window's border.
    let used = (ui.min_rect().bottom() - top + ui.spacing().item_spacing.y).ceil();
    if set_inspector_height(&ctx, graph, used) {
        ctx.request_repaint();
    }
    if !model.is_empty() {
        flush_layout_edit(view, graph, actions);
        actions.append(&mut model);
    }
}

/// Starts a new line first if the `texts` and the `fields` number fields that
/// follow, plus `extra` points of other widgets (buttons, icons), would not fit
/// on the rest of this one, so that a wrapped row never splits a label from
/// what it belongs with (nested `horizontal` groups do not wrap in egui). Does
/// nothing at the start of a line.
fn keep_together(ui: &mut egui::Ui, texts: &[&str], fields: usize, extra: f32) {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let spacing = ui.spacing().item_spacing.x;
    let text_width: f32 = texts
        .iter()
        .map(|text| {
            ui.painter()
                .layout_no_wrap((*text).to_owned(), font.clone(), egui::Color32::WHITE)
                .size()
                .x
        })
        .sum();
    let needed = text_width
        + fields as f32 * crate::number_field::FIELD_WIDTH
        + extra
        + (texts.len() + fields) as f32 * spacing;
    let at_line_start = ui.cursor().left() <= ui.min_rect().left() + 1.0;
    // `available_width` of a wrapped layout is the whole row, not what is left.
    let left = ui.max_rect().right() - ui.cursor().left();
    if !at_line_start && left < needed {
        ui.end_row();
    }
}

/// The extra width of one text button beyond its text.
fn button_padding(ui: &egui::Ui) -> f32 {
    2.0 * ui.spacing().button_padding.x
}

/// First row of the strip: figure size, selected graph, plot area.
#[allow(clippy::too_many_arguments)]
fn measures_row(
    ui: &mut egui::Ui,
    view: &GraphView,
    spec: &PlotSpec,
    layout: Option<&FigureLayout>,
    graph: NodeId,
    selected: Option<GraphBox>,
    model: &mut Vec<Action>,
    session: &mut Vec<Action>,
) {
    let ctx = ui.ctx().clone();
    let unit = length_unit(&ctx);
    ui.spacing_mut().item_spacing.x = 4.0;
    let Some(image) = view.layout_image() else {
        return;
    };
    figure_size_controls(ui, view, spec, graph, image, unit, model, session);
    ui.separator();
    if let Some(selected) = selected {
        // The box is the target of the number fields (#498).
        crate::field_state::for_target(ui, (graph, selected.base), |ui| {
            box_controls(ui, spec, layout, graph, image, &selected, unit, model);
        });
    }
}

/// "Figure size" with its two fields and the ratio lock.
#[allow(clippy::too_many_arguments)]
fn figure_size_controls(
    ui: &mut egui::Ui,
    view: &GraphView,
    spec: &PlotSpec,
    graph: NodeId,
    image: FreePage,
    unit: LengthUnit,
    model: &mut Vec<Action>,
    session: &mut Vec<Action>,
) {
    let ctx = ui.ctx().clone();
    let size = (image.width, image.height);
    let published = published_size_lock(&ctx, graph);
    // The lock takes the ratio again from an image changed by anything else;
    // not while a corner is being pulled (the preview is not the image).
    let mut lock = published;
    if let Some(closed) = lock.as_mut().filter(|_| !view.layout_dragging()) {
        closed.follow(size);
    }
    if lock != published {
        session.push(Action::SetSizeLock { graph, lock });
    }
    let label = tr!("plot-size-total", unit = unit.symbol());
    let by = tr!("plot-size-total-by");
    keep_together(ui, &[&label, &by], 2, ui.spacing().interact_size.x);
    ui.label(label);
    let mut fields = (
        shown(unit.from_points(size.0), unit),
        shown(unit.from_points(size.1), unit),
    );
    let quantity = length_quantity(unit, MIN_PAGE_EXTENT, stokesforge_plot::MAX_FIGURE_EXTENT);
    let width_changed = number_field(ui, ("layout_figure", 0), &mut fields.0, &quantity).changed;
    ui.label(by);
    let height_changed = number_field(ui, ("layout_figure", 1), &mut fields.1, &quantity).changed;
    let (icon, role, tooltip) = if published.is_some() {
        (
            Icon::LockClosed,
            IconRole::Accent,
            tr!("plot-size-lock-closed"),
        )
    } else {
        (Icon::LockOpen, IconRole::Muted, tr!("plot-size-lock-open"))
    };
    if icon_button(ui, icon, role, tooltip).clicked() {
        session.push(Action::SetSizeLock {
            graph,
            lock: match published {
                Some(_) => None,
                None => SizeLock::close(size),
            },
        });
    }
    // #505: the page does not follow the layers by itself; this button does it.
    // K1: its inverse comes right after.
    let fit = tr!("action-layout-fit-page");
    keep_together(ui, &[&fit], 1, ui.spacing().interact_size.x);
    let available = page_fit_available(spec, &view.layout_boxes(), image);
    if ui
        .add_enabled(available, egui::Button::new(fit))
        .on_hover_text(tr!("action-layout-fit-page", "hint"))
        .on_disabled_hover_text(tr!("action-layout-fit-page", "disabled"))
        .clicked()
    {
        model.push(Action::FitPageToLayout { graph });
    }
    // K1: the inverse, the graphs scale to fill the page. A closed padlock
    // keeps their proportions.
    let fill = tr!("action-layout-fit-layout");
    keep_together(ui, &[&fill], 1, ui.spacing().interact_size.x);
    if ui
        .add_enabled(available, egui::Button::new(fill))
        .on_hover_text(tr!("action-layout-fit-layout", "hint"))
        .on_disabled_hover_text(tr!("action-layout-fit-layout", "disabled"))
        .clicked()
    {
        model.push(Action::FitLayoutToPage {
            graph,
            uniform: published.is_some(),
        });
    }
    if !(width_changed || height_changed) {
        return;
    }
    let ratio = lock.map(|lock| lock.ratio);
    if spec.layer_placement == LayerPlacement::Free {
        let mut wanted = image;
        if width_changed {
            wanted.width = unit.to_points(fields.0);
            if let Some(ratio) = ratio {
                wanted.height = wanted.width * ratio;
            }
        } else {
            wanted.height = unit.to_points(fields.1);
            if let Some(ratio) = ratio {
                wanted.width = wanted.height / ratio;
            }
        }
        let new_page = wanted.sanitized().unwrap_or(image);
        model.push(Action::SetFreeLayout {
            graph,
            frames: scaled_to_page(&view.layout_boxes(), image, new_page),
            page: Some(new_page),
        });
    } else {
        let (axis, points) = if width_changed {
            (SizeAxis::Width, unit.to_points(fields.0))
        } else {
            (SizeAxis::Height, unit.to_points(fields.1))
        };
        model.push(Action::SetFigureTotal {
            graph,
            axis,
            target: points,
            ratio,
        });
    }
}

/// The selected graph: X, Y, W, H of its outer rectangle and its plot area.
/// On a regular grid X and Y are greyed (the grid places the graphs) and W and
/// H trade with a neighbour.
#[allow(clippy::too_many_arguments)]
fn box_controls(
    ui: &mut egui::Ui,
    spec: &PlotSpec,
    layout: Option<&FigureLayout>,
    graph: NodeId,
    image: FreePage,
    selected: &GraphBox,
    unit: LengthUnit,
    model: &mut Vec<Action>,
) {
    let free = spec.layer_placement == LayerPlacement::Free;
    let trade = layout
        .and_then(|layout| regular_cell(layout, spec, selected))
        .map_or((false, false), |(_, trade)| trade);
    let title = tr!("layout-mode-layer", n = (selected.base + 1).to_string());
    let o = selected.outer;
    let exact = [o.left - image.left, o.top - image.top, o.width, o.height];
    let mut fields = exact.map(|points| shown(unit.from_points(points), unit));
    let mut edited = [false; 4];
    let extent = image.width.max(image.height);
    let position = length_quantity(unit, 0.0, extent);
    let size = length_quantity(unit, MIN_FRAME_SIZE, extent);
    let columns = [
        (
            tr!("layout-mode-left"),
            tr!("layout-mode-left", "disabled"),
            &position,
            free,
        ),
        (
            tr!("layout-mode-top"),
            tr!("layout-mode-top", "disabled"),
            &position,
            free,
        ),
        (
            tr!("layout-mode-width"),
            tr!("layout-mode-width", "disabled"),
            &size,
            free || trade.0,
        ),
        (
            tr!("layout-mode-height"),
            tr!("layout-mode-height", "disabled"),
            &size,
            free || trade.1,
        ),
    ];
    for (index, (label, reason, quantity, enabled)) in columns.into_iter().enumerate() {
        if index == 0 {
            keep_together(ui, &[&title, &label], 1, 0.0);
            ui.label(&title);
        } else {
            keep_together(ui, &[&label], 1, 0.0);
        }
        ui.label(label);
        if enabled {
            edited[index] =
                number_field(ui, ("layout_box", index), &mut fields[index], quantity).changed;
        } else {
            ui.add_enabled_ui(false, |ui| {
                number_field(ui, ("layout_box", index), &mut fields[index], quantity).response
            })
            .inner
            .on_disabled_hover_text(reason);
        }
    }
    let points = |index: usize| {
        if edited[index] {
            unit.to_points(fields[index])
        } else {
            exact[index]
        }
    };
    if edited.iter().any(|edited| *edited) {
        if free {
            let outer = box_in_page(
                selected,
                image,
                (points(0), points(1), points(2), points(3)),
            );
            model.push(Action::SetFreeLayout {
                graph,
                frames: vec![(selected.base, selected.frame_for(outer))],
                page: Some(image),
            });
        } else if let Some((widths, heights)) =
            layout.and_then(|layout| regular_resize(layout, spec, selected, (points(2), points(3))))
        {
            model.push(Action::SetGridFrames {
                graph,
                widths,
                heights,
                figure: false,
                slack: None,
            });
        }
    }
    ui.label(crate::theme::dimmed_text(tr!(
        "layout-mode-plot-area",
        width = format!(
            "{:.*}",
            unit.decimals(),
            unit.from_points(selected.frame.width)
        ),
        height = format!(
            "{:.*}",
            unit.decimals(),
            unit.from_points(selected.frame.height)
        ),
        unit = unit.symbol()
    )));
}

/// A menu button of the arrangement group, greyed with its reason: the grid
/// places the graphs on a regular grid, and the count is short in Free.
fn arrange_menu(
    ui: &mut egui::Ui,
    enabled: bool,
    label: String,
    reason: String,
    contents: impl FnOnce(&mut egui::Ui),
) {
    ui.add_enabled_ui(enabled, |ui| ui.menu_button(label, contents).response)
        .inner
        .on_disabled_hover_text(reason);
}

/// Second row of the strip: arrangement menus, Snap, the grid mode and what
/// happens when axes are added.
fn arrange_row(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    spec: &PlotSpec,
    graph: NodeId,
    active: usize,
    extra: &[usize],
    model: &mut Vec<Action>,
) {
    let free = spec.layer_placement == LayerPlacement::Free;
    let cells = stokesforge_core::compose::arrange_targets(spec, active, extra).len();
    let reason = |free_reason: String| {
        if free {
            free_reason
        } else {
            tr!("layout-mode-arrange-regular")
        }
    };
    arrange_menu(
        ui,
        free && cells >= 2,
        tr!("menu-layout-align"),
        reason(tr!("menu-layout-align", "disabled")),
        |ui| super::menus::align_items(ui, graph, active, extra, model),
    );
    arrange_menu(
        ui,
        free && cells >= 2,
        tr!("layout-mode-same-size"),
        reason(tr!("action-layout-same-width", "disabled")),
        |ui| super::menus::same_size_items(ui, graph, active, extra, cells, model),
    );
    arrange_menu(
        ui,
        free && cells >= 3,
        tr!("layout-mode-distribute"),
        reason(tr!("action-layout-distribute-horizontally", "disabled")),
        |ui| super::menus::distribute_items(ui, graph, active, extra, cells, model),
    );
    ui.separator();

    // Snap on or off, then the grid step as a two-state toggle. View state only.
    let mut snap = snap_state(ctx, graph);
    let before = snap;
    let (snap_label, coarse, fine) = (
        tr!("layout-mode-snap"),
        tr!("layout-mode-step-coarse"),
        tr!("layout-mode-step-fine"),
    );
    let steps_width = state_toggle_width(ui, [&coarse, &fine]);
    keep_together(ui, &[&snap_label], 0, button_padding(ui) + steps_width);
    let snap_button = ui
        .add(egui::Button::new(snap_label).selected(snap.on))
        .on_hover_text(tr!("layout-mode-snap", "hint"));
    if snap_button.hovered() {
        paint_hover_outline(ui, snap_button.rect);
    }
    if snap_button.clicked() {
        snap.on = !snap.on;
    }
    let step_reason = tr!("layout-mode-step", "disabled");
    if state_toggle(
        ui,
        [&coarse, &fine],
        usize::from(snap.fine),
        &tr!("layout-mode-step", "hint"),
        (!snap.on).then_some(step_reason.as_str()),
    ) {
        snap.fine = !snap.fine;
    }
    if snap != before {
        set_snap_state(ctx, graph, snap);
    }
    ui.separator();

    // The placement of the layers, saved and undoable: both states are shown.
    let (placement_label, regular, free_label) = (
        tr!("layout-mode-placement"),
        tr!("layout-mode-placement-regular"),
        tr!("layout-mode-placement-free"),
    );
    keep_together(
        ui,
        &[&placement_label],
        0,
        state_toggle_width(ui, [&regular, &free_label]),
    );
    ui.label(placement_label);
    if state_toggle(
        ui,
        [&regular, &free_label],
        usize::from(free),
        &tr!("layout-mode-placement", "hint"),
        None,
    ) {
        model.push(Action::SetLayerPlacement { graph, free: !free });
    }
    ui.separator();

    // What an added axis or title does to the image.
    let axes_label = tr!("plot-figure-size");
    let (grows, fixed) = (tr!("plot-figure-size-grows"), tr!("plot-figure-size-fixed"));
    keep_together(
        ui,
        &[&axes_label],
        0,
        state_toggle_width(ui, [&grows, &fixed]),
    );
    ui.label(axes_label);
    if state_toggle(
        ui,
        [&grows, &fixed],
        usize::from(spec.fixed_figure),
        &tr!("plot-figure-size", "hint"),
        None,
    ) {
        model.push(Action::SetFigureFixed {
            graph,
            fixed: !spec.fixed_figure,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::project_manager::tests::{click_at, find_text_center};
    use super::super::test_support::painted;
    use super::*;
    use stokesforge_core::compose::CellSide;
    use stokesforge_session::Session;

    #[test]
    fn grid_steps_follow_the_unit() {
        assert!(
            (grid_step_points(LengthUnit::Cm, false) - LengthUnit::Mm.to_points(5.0)).abs() < 1e-9
        );
        assert!(
            (grid_step_points(LengthUnit::Mm, true) - LengthUnit::Mm.to_points(1.0)).abs() < 1e-9
        );
        assert_eq!(grid_step_points(LengthUnit::Pt, false), 10.0);
        assert_eq!(grid_step_points(LengthUnit::Pt, true), 1.0);
        assert!((grid_step_points(LengthUnit::In, false) - 18.0).abs() < 1e-9);
        assert!((grid_step_points(LengthUnit::In, true) - 4.5).abs() < 1e-9);
    }

    #[test]
    fn ruler_steps_are_nice_and_far_enough() {
        for minimum in [0.03, 0.4, 1.0, 1.1, 2.0, 3.0, 7.0, 55.0] {
            let (step, subs) = nice_step(minimum);
            assert!(step >= minimum * 0.999, "{step} for {minimum}");
            assert!(subs == 4 || subs == 5);
            let mantissa = step / 10f64.powi(step.log10().floor() as i32);
            assert!(
                [1.0, 2.0, 5.0].iter().any(|m| (mantissa - m).abs() < 1e-6),
                "{step}"
            );
        }
    }

    #[test]
    fn a_box_typed_outside_the_page_is_held_inside() {
        let page = FreePage {
            left: 10.0,
            top: 20.0,
            width: 400.0,
            height: 300.0,
        };
        let selected = GraphBox {
            base: 0,
            outer: FreePage {
                left: 10.0,
                top: 20.0,
                width: 100.0,
                height: 100.0,
            },
            gutters: (10.0, 10.0, 5.0, 10.0),
            frame: stokesforge_plot::FreeFrame::FALLBACK,
        };
        let outer = box_in_page(&selected, page, (1000.0, -5.0, 150.0, 5000.0));
        assert_eq!(outer.width, 150.0);
        assert_eq!(outer.height, 300.0);
        assert_eq!(outer.left, 10.0 + 400.0 - 150.0);
        assert_eq!(outer.top, 20.0);
    }

    /// A graph window in the mode, painted by the real contents function, with
    /// its own `AppState` to apply the actions it sends.
    struct Bench {
        session: Session,
        state: stokesforge_app_state::state::AppState,
        view: GraphView,
        ctx: egui::Context,
        graph: NodeId,
        time: f64,
        width: f32,
    }

    impl Bench {
        /// A plotted graph with `columns` columns on a regular grid (`free`:
        /// then switched to Free), the mode on.
        fn new(columns: usize, free: bool) -> Self {
            let (session, graph) = super::super::graph_window::tests::plotted_graph();
            let mut state =
                stokesforge_app_state::state::AppState::new(session.handle(), "t.stf".into())
                    .unwrap();
            for _ in 1..columns {
                state.dispatch(Action::AddCell {
                    graph,
                    panel: 0,
                    side: CellSide::Right,
                    columns: Vec::new(),
                });
            }
            if free {
                state.dispatch(Action::SetLayerPlacement { graph, free: true });
            }
            let ctx = egui::Context::default();
            ctx.set_fonts(stokesforge_render_egui::font_definitions());
            let mut bench = Self {
                session,
                state,
                view: GraphView::new(),
                ctx,
                graph,
                time: 0.0,
                width: 1100.0,
            };
            bench.frame(Vec::new());
            bench.view.set_layout_mode(true);
            bench.frame(Vec::new());
            bench.frame(Vec::new());
            bench
        }

        fn spec(&self) -> PlotSpec {
            self.state.plot_spec(self.graph).expect("spec")
        }

        /// One painted frame; the actions it sent.
        fn frame(&mut self, events: Vec<egui::Event>) -> (egui::FullOutput, Vec<Action>) {
            publish_app_state(
                &self.ctx,
                Some(self.graph),
                self.state.length_unit(),
                self.state.size_locks().clone(),
            );
            self.time += 0.1;
            let snapshot = self.state.snapshot().clone();
            let mut actions = Vec::new();
            let input = egui::RawInput {
                time: Some(self.time),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(self.width, 700.0),
                )),
                events,
                ..Default::default()
            };
            let (view, graph) = (&mut self.view, self.graph);
            let mut output = self.ctx.run_ui(input, |ui| {
                // What the root or a detached viewport does around the contents.
                handle_keys(ui.ctx(), view, graph, &mut actions);
                super::super::graph_window::graph_view_contents(
                    ui,
                    view,
                    &snapshot,
                    graph,
                    &mut actions,
                );
                note_field_focus(ui.ctx());
            });
            output.textures_delta.clear();
            (output, actions)
        }

        /// Clicks the text `needle`; the actions sent over the three frames.
        fn click_text(&mut self, needle: &str) -> Vec<Action> {
            let (output, _) = self.frame(Vec::new());
            let pos = find_text_center(&output.shapes, needle);
            self.click_text_at(pos)
        }

        /// Clicks at `pos`; the actions sent over the frames.
        fn click_text_at(&mut self, pos: egui::Pos2) -> Vec<Action> {
            let mut sent = Vec::new();
            let ctx = self.ctx.clone();
            let bench = std::cell::RefCell::new(&mut *self);
            click_at(&ctx, pos, egui::Modifiers::NONE, |_, events| {
                let (_, actions) = bench.borrow_mut().frame(events);
                sent.extend(actions);
            });
            sent.extend(self.frame(Vec::new()).1);
            sent
        }

        fn apply(&mut self, actions: Vec<Action>) {
            for action in actions {
                self.state.dispatch(action);
            }
        }

        fn is_painted(&mut self, needle: &str) -> bool {
            painted(&self.frame(Vec::new()).0, needle)
        }
    }

    /// The Grid mode choice is the one thing of the strip that touches the
    /// model: choosing the other segment sends one `SetLayerPlacement`, one
    /// undo step; both segments stay painted, Undo returns. Witness: a segment
    /// that shows only the current state.
    #[test]
    fn the_grid_mode_choice_is_one_undo_step_and_both_options_stay_visible() {
        let mut bench = Bench::new(2, false);
        let regular = tr!("layout-mode-placement-regular");
        let free = tr!("layout-mode-placement-free");
        assert!(bench.is_painted(&regular) && bench.is_painted(&free));
        let revision = bench.state.revision();
        let actions = bench.click_text(&free);
        assert_eq!(
            actions,
            vec![Action::SetLayerPlacement {
                graph: bench.graph,
                free: true
            }]
        );
        bench.apply(actions);
        assert_ne!(bench.state.revision(), revision);
        assert_eq!(
            bench.spec().layer_placement,
            stokesforge_plot::LayerPlacement::Free
        );
        assert!(bench.is_painted(&free) && bench.is_painted(&regular));
        bench.state.dispatch(Action::Undo);
        assert_eq!(
            bench.spec().layer_placement,
            stokesforge_plot::LayerPlacement::Grid
        );
        assert!(bench.is_painted(&regular) && bench.is_painted(&free));
        // And back to Regular from Free with one more click.
        bench.state.dispatch(Action::Redo);
        let actions = bench.click_text(&regular);
        assert_eq!(
            actions,
            vec![Action::SetLayerPlacement {
                graph: bench.graph,
                free: false
            }]
        );
        bench.session.shutdown().unwrap();
    }

    /// An arrow-key burst still pending when the placement is toggled reaches
    /// the model first: `LayoutEdited` (as `SetFreeLayout`) before
    /// `SetLayerPlacement`, so the toggle does not drop it. Witness: the flush
    /// after the toggle's action.
    #[test]
    fn a_pending_nudge_reaches_the_model_before_the_placement_toggle() {
        let mut bench = Bench::new(2, true);
        let spec = bench.spec();
        bench.view.set_active_panel(0, &spec);
        bench.frame(Vec::new());
        let now = bench.time;
        bench.view.nudge((1, 0), now);
        let actions = bench.click_text(&tr!("layout-mode-placement-regular"));
        let kinds: Vec<&str> = actions
            .iter()
            .map(|action| match action {
                Action::SetFreeLayout { .. } => "layout",
                Action::SetLayerPlacement { .. } => "placement",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, ["layout", "placement"], "{actions:?}");
        bench.session.shutdown().unwrap();
    }

    /// Snap, its step and G are view state: no action, no model change; the
    /// step button shows the step it is on and toggles it. Witness: the state
    /// kept in the spec.
    #[test]
    fn snap_the_step_and_g_are_view_state_only() {
        let mut bench = Bench::new(2, false);
        let spec = bench.spec();
        let revision = bench.state.revision();
        assert!(snap_state(&bench.ctx, bench.graph).on);
        assert!(!snap_state(&bench.ctx, bench.graph).fine);
        let sent = bench.click_text(&tr!("layout-mode-step-fine"));
        assert!(sent.is_empty(), "{sent:?}");
        assert!(snap_state(&bench.ctx, bench.graph).fine);
        assert!(bench.is_painted(&tr!("layout-mode-step-coarse")));
        let sent = bench.click_text(&tr!("layout-mode-snap"));
        assert!(sent.is_empty(), "{sent:?}");
        assert!(!snap_state(&bench.ctx, bench.graph).on);
        assert_eq!(bench.view.layout_grid(), None, "no grid without Snap");
        let g = egui::Event::Key {
            key: egui::Key::G,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let (_, sent) = bench.frame(vec![g]);
        assert!(sent.is_empty());
        bench.frame(Vec::new());
        assert!(snap_state(&bench.ctx, bench.graph).on, "G toggled Snap");
        assert!(bench.view.layout_grid().is_some());
        assert_eq!(bench.spec(), spec);
        assert_eq!(bench.state.revision(), revision);
        bench.session.shutdown().unwrap();
    }

    /// A two-state toggle of the strip is ONE button: both labels stay painted
    /// and fixed, the current state alone is in the selection colour, a click
    /// anywhere on it, either part and the seam included, switches to the other
    /// state and sends exactly one action (none for the view-state step), so
    /// two clicks on the same spot come back. Hover outlines the whole button
    /// once, in the stroke of the theme. Witnesses: a click handled per part, an
    /// outline on the hovered part only and a hard-coded colour each fail.
    #[test]
    fn a_state_toggle_is_one_button_that_shows_both_states() {
        fn rects(output: &egui::FullOutput) -> Vec<egui::epaint::RectShape> {
            fn walk(shape: &egui::Shape, out: &mut Vec<egui::epaint::RectShape>) {
                match shape {
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
                    egui::Shape::Rect(rect) => out.push(rect.clone()),
                    _ => {}
                }
            }
            let mut out = Vec::new();
            output.shapes.iter().for_each(|c| walk(&c.shape, &mut out));
            out
        }
        fn selected_texts(output: &egui::FullOutput, fill: egui::Color32) -> Vec<String> {
            let filled: Vec<egui::Rect> = rects(output)
                .into_iter()
                .filter(|r| r.fill == fill)
                .map(|r| r.rect)
                .collect();
            super::super::test_support::texts(&output.shapes)
                .into_iter()
                .filter(|t| filled.iter().any(|r| r.contains(t.center)))
                .map(|t| t.text)
                .collect()
        }
        let mut bench = Bench::new(2, false);
        let visuals = bench.ctx.global_style().visuals.clone();
        let fill = visuals.selection.bg_fill;
        let outline = visuals.widgets.hovered.bg_stroke;
        let pairs = [
            (tr!("layout-mode-step-coarse"), tr!("layout-mode-step-fine")),
            (
                tr!("layout-mode-placement-regular"),
                tr!("layout-mode-placement-free"),
            ),
            (tr!("plot-figure-size-grows"), tr!("plot-figure-size-fixed")),
        ];
        // At rest: both labels, only the first selected, no outline.
        let (output, _) = bench.frame(Vec::new());
        let selected = selected_texts(&output, fill);
        for (first, second) in &pairs {
            assert!(painted(&output, first) && painted(&output, second));
            assert!(
                selected.contains(first),
                "{first:?} not selected: {selected:?}"
            );
            assert!(
                !selected.contains(second),
                "{second:?} selected: {selected:?}"
            );
        }
        assert!(
            !rects(&output).iter().any(|r| r.stroke == outline),
            "no outline at rest"
        );
        for (index, (first, second)) in pairs.iter().enumerate() {
            let at = |bench: &mut Bench, text: &str| {
                let (output, _) = bench.frame(Vec::new());
                find_text_center(&output.shapes, text)
            };
            let first_at = at(&mut bench, first);
            let second_at = at(&mut bench, second);
            // Hover on the first part: ONE outline, around both parts.
            let ctx = bench.ctx.clone();
            let (output, _) = bench.frame(vec![egui::Event::PointerMoved(first_at)]);
            let (output, _) = {
                drop(output);
                bench.frame(Vec::new())
            };
            let outlines: Vec<_> = rects(&output)
                .into_iter()
                .filter(|r| r.stroke == outline && r.rect.contains(first_at))
                .collect();
            assert_eq!(outlines.len(), 1, "{first:?}: {outlines:?}");
            let whole = outlines[0].rect;
            assert!(
                whole.contains(second_at) && whole.contains(first_at),
                "the outline surrounds both parts of {first:?} | {second:?}"
            );
            drop(ctx);
            // Hit area: the middle of each part and the seam (the edge of the
            // selected fill facing the other part), each one switches.
            let seam = rects(&output)
                .into_iter()
                .filter(|r| r.fill == fill && r.rect.contains(first_at))
                .map(|r| r.rect.right())
                .next()
                .expect("the selected part");
            let spots = [first_at, egui::pos2(seam, whole.center().y), second_at];
            let mut state_is_second = false;
            for spot in spots {
                let sent = bench.click_text_at(spot);
                assert_eq!(
                    sent.len(),
                    usize::from(index > 0),
                    "{first:?} at {spot:?}: {sent:?}"
                );
                if index == 0 {
                    state_is_second = !state_is_second;
                    assert_eq!(snap_state(&bench.ctx, bench.graph).fine, state_is_second);
                } else {
                    // The model is not applied here: the state stays put and
                    // each click asks for the same switch again.
                    assert!(matches!(
                        sent[0],
                        Action::SetLayerPlacement { free: true, .. }
                            | Action::SetFigureFixed { fixed: true, .. }
                    ));
                }
            }
            // The step toggle was clicked three times: the last state is Fine.
        }
        // Both labels are still painted and unchanged afterwards.
        let (output, _) = bench.frame(Vec::new());
        for (first, second) in &pairs {
            assert!(painted(&output, first) && painted(&output, second));
        }
        bench.session.shutdown().unwrap();
    }

    /// Tab reaches the step toggle, outlined like a hover, and Space
    /// switches it. Witness: a toggle that is not focusable never gets the
    /// outline.
    #[test]
    fn the_step_toggle_is_reached_by_tab_and_space_switches_it() {
        let mut bench = Bench::new(2, false);
        let outline = bench.ctx.global_style().visuals.widgets.hovered.bg_stroke;
        let key = |key: egui::Key| {
            [true, false]
                .into_iter()
                .map(|pressed| egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
                .collect::<Vec<_>>()
        };
        let mut stops = 0;
        for _ in 0..60 {
            bench.frame(key(egui::Key::Tab));
            let (output, _) = bench.frame(Vec::new());
            let at = find_text_center(&output.shapes, &tr!("layout-mode-step-fine"));
            let outlined = output.shapes.iter().any(|clipped| {
                matches!(&clipped.shape, egui::Shape::Rect(r)
                    if r.stroke == outline && r.rect.contains(at))
            });
            if outlined {
                stops += 1;
                if stops == 1 {
                    assert!(!snap_state(&bench.ctx, bench.graph).fine);
                    bench.frame(key(egui::Key::Space));
                    bench.frame(Vec::new());
                    assert!(snap_state(&bench.ctx, bench.graph).fine, "Space switches");
                }
            }
        }
        assert!(stops >= 1, "Tab never reached the toggle");
        bench.session.shutdown().unwrap();
    }

    /// Greyed (Snap off): the step toggle neither reacts nor outlines.
    #[test]
    fn the_step_toggle_is_inert_while_snap_is_off() {
        let mut bench = Bench::new(2, false);
        bench.click_text(&tr!("layout-mode-snap"));
        assert!(!snap_state(&bench.ctx, bench.graph).on);
        let outline = bench.ctx.global_style().visuals.widgets.hovered.bg_stroke;
        let (output, _) = bench.frame(Vec::new());
        let at = find_text_center(&output.shapes, &tr!("layout-mode-step-fine"));
        bench.frame(vec![egui::Event::PointerMoved(at)]);
        let (output, _) = bench.frame(Vec::new());
        let mut hovered_outline = false;
        for clipped in &output.shapes {
            if let egui::Shape::Rect(rect) = &clipped.shape {
                hovered_outline |= rect.stroke == outline && rect.rect.contains(at);
            }
        }
        assert!(!hovered_outline, "no outline while greyed");
        let before = snap_state(&bench.ctx, bench.graph);
        bench.click_text(&tr!("layout-mode-step-fine"));
        assert_eq!(snap_state(&bench.ctx, bench.graph), before, "no click");
        bench.session.shutdown().unwrap();
    }

    /// "Grows with the axes" / "Fixed" is the setter of Graph Properties: one
    /// step, the label follows. Witness: the label of the other state.
    #[test]
    fn the_axes_button_toggles_fixed_in_one_step() {
        let mut bench = Bench::new(1, false);
        assert!(!bench.spec().fixed_figure);
        let actions = bench.click_text(&tr!("plot-figure-size-fixed"));
        assert_eq!(
            actions,
            vec![Action::SetFigureFixed {
                graph: bench.graph,
                fixed: true
            }]
        );
        bench.apply(actions);
        assert!(bench.spec().fixed_figure);
        assert!(bench.is_painted(&tr!("plot-figure-size-grows")));
        bench.state.dispatch(Action::Undo);
        assert!(!bench.spec().fixed_figure, "one step");
        bench.session.shutdown().unwrap();
    }

    /// On a regular grid the figure size is typed through `SetFigureTotal`, and
    /// the strip says why X and Y cannot be typed.
    #[test]
    fn a_figure_size_typed_on_a_regular_grid_resizes_the_figure() {
        let mut bench = Bench::new(2, false);
        let before = stokesforge_app_state::frame_sizes::figure_extent(&bench.spec()).unwrap();
        let image = bench.view.layout_image().expect("image");
        let unit = bench.state.length_unit();
        let value = crate::number_field::format_value(
            shown(unit.from_points(image.width), unit),
            unit.decimals(),
        );
        let (output, _) = bench.frame(Vec::new());
        let pos = find_text_center(&output.shapes, &value);
        let ctx = bench.ctx.clone();
        let sent = std::cell::RefCell::new(Vec::new());
        {
            let cell = std::cell::RefCell::new(&mut bench);
            let run = |events: Vec<egui::Event>| {
                let (_, actions) = cell.borrow_mut().frame(events);
                sent.borrow_mut().extend(actions);
            };
            click_at(&ctx, pos, egui::Modifiers::NONE, |_, events| run(events));
            run(vec![egui::Event::Text("40".into())]);
            run(vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            run(Vec::new());
        }
        let sent = sent.into_inner();
        let [Action::SetFigureTotal {
            axis,
            target,
            ratio: None,
            ..
        }] = sent.as_slice()
        else {
            panic!("one SetFigureTotal expected, got {sent:?}");
        };
        assert_eq!(*axis, SizeAxis::Width);
        assert!((target - LengthUnit::Cm.to_points(40.0)).abs() < 0.01);
        bench.apply(sent);
        let after = stokesforge_app_state::frame_sizes::figure_extent(&bench.spec()).unwrap();
        assert!(after.0 > before.0 + 10.0, "{before:?} -> {after:?}");
        bench.session.shutdown().unwrap();
    }

    fn cell_layout(sizes: (&[f64], &[f64])) -> FigureLayout {
        let breaks = |sizes: &[f64]| {
            let mut at = 0.0;
            let mut out = vec![0u32];
            for size in sizes {
                at += size + 40.0;
                out.push(at as u32);
            }
            out
        };
        FigureLayout {
            size: (500, 300),
            title_band: 0,
            column_breaks: breaks(sizes.0),
            row_breaks: breaks(sizes.1),
            column_gutters: sizes.0.iter().map(|_| (30, 10)).collect(),
            row_gutters: sizes.1.iter().map(|_| (10, 30)).collect(),
            frame_widths: sizes.0.to_vec(),
            frame_heights: sizes.1.to_vec(),
            fixed_overflow: false,
            min_left: 0,
            min_top: 0,
            placement: LayerPlacement::Grid,
            free_cells: Vec::new(),
            free_origin: (0.0, 0.0),
            grid_slack: [0; 4],
        }
    }

    /// W typed on a regular grid trades with the right neighbour (the last
    /// column with its left one) and the figure keeps its size; with no
    /// neighbour nothing is traded. Witness: W stored alone.
    #[test]
    fn a_width_typed_on_a_regular_grid_trades_with_a_neighbour() {
        let layout = cell_layout((&[200.0, 100.0], &[150.0]));
        let mut spec = PlotSpec::empty("");
        spec.panels = vec![
            stokesforge_plot::PanelSpec::at(0, 0),
            stokesforge_plot::PanelSpec::at(0, 1),
        ]
        .into();
        let outer = |base: usize, width: f64| GraphBox {
            base,
            outer: FreePage {
                left: 0.0,
                top: 0.0,
                width,
                height: 190.0,
            },
            gutters: (30.0, 10.0, 10.0, 30.0),
            frame: stokesforge_plot::FreeFrame::FALLBACK,
        };
        let (widths, heights) =
            regular_resize(&layout, &spec, &outer(0, 240.0), (290.0, 190.0)).expect("a neighbour");
        assert_eq!(widths, vec![250.0, 50.0]);
        assert_eq!(
            widths.iter().sum::<f64>(),
            300.0,
            "the figure keeps its size"
        );
        assert_eq!(heights, vec![150.0]);
        let (widths, _) = regular_resize(&layout, &spec, &outer(1, 140.0), (190.0, 190.0))
            .expect("the last column trades with its left one");
        assert_eq!(widths, vec![150.0, 150.0]);
        // A height with no row to trade with: nothing.
        assert_eq!(
            regular_resize(&layout, &spec, &outer(0, 240.0), (240.0, 250.0)),
            None
        );
        // A trade stops where the neighbour reaches its minimum, sum kept.
        let (widths, _) =
            regular_resize(&layout, &spec, &outer(0, 240.0), (10_000.0, 190.0)).unwrap();
        assert_eq!(widths.iter().sum::<f64>(), 300.0);
        assert_eq!(widths[1], MIN_FRAME_SIZE);
    }

    /// A number typed in the Left field of the selected box and a click on
    /// another box on the canvas: the number moves the box it was typed for
    /// (#498). Witness: `selected_box()` read after the canvas is painted.
    #[test]
    fn a_left_typed_for_one_box_moves_it_when_another_box_is_clicked() {
        let mut bench = Bench::new(2, true);
        bench.frame(Vec::new());
        let selected = bench.view.selected_box().expect("selected");
        let other = bench
            .view
            .layout_boxes()
            .into_iter()
            .find(|candidate| candidate.base != selected.base)
            .expect("a second box");
        let image = bench.view.layout_image().expect("the page");
        let unit = bench.state.length_unit();
        let left_text = crate::number_field::format_value(
            shown(unit.from_points(selected.outer.left - image.left), unit),
            unit.decimals(),
        );
        let (output, _) = bench.frame(Vec::new());
        // "0" is also a ruler mark: the field is the one on the row of its label.
        let texts = super::super::test_support::texts(&output.shapes);
        let label = texts
            .iter()
            .find(|t| t.text == tr!("layout-mode-left"))
            .expect("the label")
            .center;
        let field = texts
            .iter()
            .find(|t| t.text == left_text && (t.center.y - label.y).abs() < 4.0)
            .expect("the field")
            .center;
        bench.click_text_at(field);
        bench.frame(vec![egui::Event::Text("3".into())]);
        let o = other.outer;
        let at = bench
            .view
            .stored_to_screen(o.left + o.width / 2.0, o.top + o.height / 2.0)
            .expect("on screen");
        let sent = bench.click_text_at(at);
        let moved: Vec<usize> = sent
            .iter()
            .filter_map(|action| match action {
                Action::SetFreeLayout { frames, .. } => Some(frames.iter().map(|f| f.0)),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(moved, vec![selected.base], "only the box typed for moved");
        assert_eq!(
            bench.view.selected_box().map(|b| b.base),
            Some(other.base),
            "the clicked box is the selected one"
        );
        bench.session.shutdown().unwrap();
    }

    /// X and Y are greyed on a regular grid, with a reason; W and H are not
    /// when there is a neighbour. In Free all four are typed.
    #[test]
    fn x_and_y_are_greyed_on_a_regular_grid() {
        let mut bench = Bench::new(2, false);
        let (output, _) = bench.frame(Vec::new());
        // A greyed field paints its value in another colour than a live one.
        let texts = super::super::test_support::texts(&output.shapes);
        let selected = bench.view.selected_box().expect("selected");
        let unit = bench.state.length_unit();
        let shown_text = |points: f64| {
            crate::number_field::format_value(
                shown(unit.from_points(points), unit),
                unit.decimals(),
            )
        };
        let colour_of = |text: String| {
            texts
                .iter()
                .find(|painted| painted.text == text)
                .map(|painted| painted.color)
        };
        let x = colour_of(shown_text(selected.outer.left));
        let width = colour_of(shown_text(selected.outer.width));
        assert!(x.is_some() && width.is_some(), "{x:?} {width:?}");
        assert_ne!(x, width, "X is greyed, W is live");
        assert!(bench.is_painted(&tr!("layout-mode-left")));
        bench.session.shutdown().unwrap();
    }

    /// The strip keeps a label with its field or button on every width: the
    /// two ends of a pair are on the same row. Witness: `keep_together` removed.
    #[test]
    fn a_narrow_strip_never_splits_a_pair() {
        // Narrower than a pair, nothing can keep it on one row.
        for width in (360..=1100).step_by(20) {
            let mut bench = Bench::new(2, false);
            bench.width = width as f32;
            for _ in 0..3 {
                bench.frame(Vec::new());
            }
            let (output, _) = bench.frame(Vec::new());
            let texts = super::super::test_support::texts(&output.shapes);
            for (first, second) in [
                (tr!("layout-mode-snap"), tr!("layout-mode-step-coarse")),
                (
                    tr!("layout-mode-placement"),
                    tr!("layout-mode-placement-regular"),
                ),
                (tr!("plot-figure-size"), tr!("plot-figure-size-grows")),
                (
                    tr!("plot-size-total", unit = "cm"),
                    tr!("plot-size-total-by"),
                ),
            ] {
                let row =
                    |needle: &str| texts.iter().find(|t| t.text == needle).map(|t| t.center.y);
                let (Some(a), Some(b)) = (row(&first), row(&second)) else {
                    continue;
                };
                assert!(
                    (a - b).abs() < 4.0,
                    "{first:?} and {second:?} split at width {width}: {a} vs {b}"
                );
            }
            bench.session.shutdown().unwrap();
        }
    }

    /// In a narrow strip the rows wrap and the reserved height grows with
    /// them; it is the same at every frame once measured. Witness: a fixed
    /// reserve fails the first; a height fed back into its own measurement
    /// fails the second.
    #[test]
    fn a_narrow_strip_reserves_more_height_and_keeps_it_from_frame_to_frame() {
        let heights = |width: f32| {
            let mut bench = Bench::new(2, false);
            bench.width = width;
            let mut seen = Vec::new();
            for _ in 0..6 {
                bench.frame(Vec::new());
                seen.push(controls_height(&bench.ctx, bench.graph));
            }
            bench.session.shutdown().unwrap();
            seen
        };
        let wide = heights(1600.0);
        let narrow = heights(300.0);
        assert!(narrow[5] > wide[5] + 20.0, "{narrow:?} vs {wide:?}");
        assert!(wide[5] >= LAYOUT_CONTROLS_HEIGHT);
        for seen in [&wide, &narrow] {
            assert!(
                seen[2..].windows(2).all(|pair| pair[0] == pair[1]),
                "{seen:?}"
            );
        }
        assert_eq!(narrow[5], narrow[5].round());
    }

    impl Bench {
        /// Pulls the lower right corner of the figure by `by` screen points
        /// with the mouse; the actions the release sent.
        fn pull_figure_corner(&mut self, by: egui::Vec2) -> Vec<Action> {
            let image = self.view.layout_image().expect("image");
            let corner = self
                .view
                .stored_to_screen(image.left + image.width, image.top + image.height)
                .expect("corner")
                + egui::vec2(6.0, 6.0);
            let button = |pressed: bool, pos: egui::Pos2| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            let mut sent = Vec::new();
            sent.extend(
                self.frame(vec![
                    egui::Event::PointerMoved(corner),
                    button(true, corner),
                ])
                .1,
            );
            for step in 1..=5 {
                let pos = corner + by * (step as f32 / 5.0);
                sent.extend(self.frame(vec![egui::Event::PointerMoved(pos)]).1);
            }
            sent.extend(self.frame(vec![button(false, corner + by)]).1);
            sent.extend(self.frame(Vec::new()).1);
            sent
        }
    }

    /// K1: pulling the figure's corner adds empty space, whatever the padlock
    /// of Figure size says: the frames are unchanged and the right slack grows.
    /// Witness: the lock fed to the view again (the frames would scale).
    #[test]
    fn a_figure_corner_pull_adds_space_and_ignores_the_padlock() {
        for locked in [false, true] {
            let mut bench = Bench::new(2, false);
            let spec = bench.spec();
            let (w, h) = stokesforge_app_state::frame_sizes::figure_extent(&spec).unwrap();
            if locked {
                bench
                    .state
                    .set_size_lock(bench.graph, SizeLock::close((w, h)));
            }
            bench.frame(Vec::new());
            let sent = bench.pull_figure_corner(egui::vec2(80.0, 2.0));
            let edits: Vec<&Action> = sent
                .iter()
                .filter(|action| matches!(action, Action::SetGridFrames { .. }))
                .collect();
            let [Action::SetGridFrames {
                widths,
                figure: true,
                slack: Some(slack),
                ..
            }] = edits.as_slice()
            else {
                panic!("one figure action expected, got {sent:?}");
            };
            assert_eq!(widths, &spec.frame_widths, "locked {locked}");
            assert!(slack.right > 40.0 && slack.left == 0.0, "{slack:?}");
            bench.session.shutdown().unwrap();
        }
    }

    /// The ratio lock is the application's, per graph, published to every
    /// graph window and carried to a detached view (the figure's corners no
    /// longer read it, K1): closed in Graph Properties (`set_size_lock`), the
    /// fork of a detached view carries it. Witness: the lock not published.
    #[test]
    fn the_ratio_lock_is_the_applications_and_reaches_the_view_and_a_detached_fork() {
        let mut bench = Bench::new(2, false);
        let image = bench.view.layout_image().expect("image");
        let lock = SizeLock::close((image.width, image.height)).expect("lock");
        bench.state.set_size_lock(bench.graph, Some(lock));
        bench.frame(Vec::new());
        assert_eq!(published_size_lock(&bench.ctx, bench.graph), Some(lock));
        // The fork of a detached view carries it, and a resync refreshes it.
        let mut fork = bench.state.fork_for_detached_view().unwrap();
        assert_eq!(fork.size_lock(bench.graph), Some(lock));
        bench.state.set_size_lock(bench.graph, None);
        fork.sync_detached_sheet_editor(&bench.state);
        assert_eq!(fork.size_lock(bench.graph), None);
        bench.session.shutdown().unwrap();
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Esc leaves the mode of the foreground graph only.
    #[test]
    fn escape_leaves_the_mode_of_the_foreground_graph_only() {
        let mut bench = Bench::new(1, false);
        bench
            .ctx
            .data_mut(|d| d.insert_temp(foreground_id(), None::<NodeId>));
        // `frame` republishes the foreground: paint by hand with it unset.
        let snapshot = bench.state.snapshot().clone();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 700.0),
            )),
            events: vec![key(egui::Key::Escape)],
            ..Default::default()
        };
        let (view, graph) = (&mut bench.view, bench.graph);
        let mut output = bench.ctx.run_ui(input, |ui| {
            handle_keys(ui.ctx(), view, graph, &mut Vec::new());
            super::super::graph_window::graph_view_contents(
                ui,
                view,
                &snapshot,
                graph,
                &mut Vec::new(),
            );
        });
        output.textures_delta.clear();
        assert!(
            bench.view.layout_mode(),
            "a background graph keeps the mode"
        );
        bench.frame(vec![key(egui::Key::Escape)]);
        assert!(!bench.view.layout_mode());
        bench.session.shutdown().unwrap();
    }

    /// A field of the strip with the keyboard: the first Esc gives it back (the
    /// typed text is dropped, the mode stays), the second Esc leaves the mode.
    /// egui drops the focus on the Esc itself, so only the mark of the frame
    /// before tells the two apart. Witness: remove `note_field_focus`.
    #[test]
    fn the_first_escape_gives_a_strip_field_back_and_the_second_leaves() {
        let mut bench = Bench::new(2, false);
        let before = bench.spec();
        let image = bench.view.layout_image().expect("image");
        let unit = bench.state.length_unit();
        let value = crate::number_field::format_value(
            shown(unit.from_points(image.width), unit),
            unit.decimals(),
        );
        let (output, _) = bench.frame(Vec::new());
        let pos = find_text_center(&output.shapes, &value);
        let ctx = bench.ctx.clone();
        {
            let cell = std::cell::RefCell::new(&mut bench);
            click_at(&ctx, pos, egui::Modifiers::NONE, |_, events| {
                cell.borrow_mut().frame(events);
            });
        }
        bench.frame(vec![egui::Event::Text("40".into())]);
        assert!(bench.ctx.text_edit_focused(), "the field has the keyboard");
        bench.frame(vec![key(egui::Key::Escape)]);
        bench.frame(Vec::new());
        assert!(
            bench.view.layout_mode(),
            "the first Esc gives the field back"
        );
        assert!(!bench.ctx.text_edit_focused());
        assert_eq!(bench.spec(), before, "the typed text was dropped");
        bench.frame(vec![key(egui::Key::Escape)]);
        assert!(!bench.view.layout_mode(), "the second Esc leaves the mode");
        bench.session.shutdown().unwrap();
    }
}
