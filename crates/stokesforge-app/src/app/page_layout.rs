//! The shell of the Page Layout Mode (#426d prototype): what surrounds the
//! figure while the mode is on. The mode itself (boxes, handles, drags,
//! nudges) lives in `stokesforge-render-egui` (`GraphView`); this module owns
//! the rulers, the inspector strip, the keys, the grid state of a graph window
//! (view state, never saved) and the requests of the menu and of the toolbar.

use eframe::egui;
use stokesforge_app_state::quantities::Quantity;
use stokesforge_app_state::state::Action;
use stokesforge_app_state::tr;
use stokesforge_core::NodeId;
use stokesforge_plot::{FreePage, LayerPlacement, LengthUnit, PlotSpec, MIN_FRAME_SIZE};
use stokesforge_render::{scaled_to_page, GraphBox, MIN_PAGE_EXTENT};
use stokesforge_render_egui::GraphView;

use super::grid::keyboard_focus_outside_grid;
use super::StokesForgeApp;
use crate::number_field::number_field;

/// Width of the rulers, in screen points.
pub(super) const RULER_WIDTH: f32 = 18.0;

/// Height reserved under the figure for the inspector strip in the mode: two
/// rows of controls.
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
    ctx.request_repaint();
}

fn foreground_id() -> egui::Id {
    egui::Id::new("stokesforge_layout_foreground_graph")
}

fn unit_id() -> egui::Id {
    egui::Id::new("stokesforge_layout_length_unit")
}

fn foreground_graph(ctx: &egui::Context) -> Option<NodeId> {
    ctx.data(|data| data.get_temp::<Option<NodeId>>(foreground_id()))
        .flatten()
}

pub(super) fn length_unit(ctx: &egui::Context) -> LengthUnit {
    ctx.data(|data| data.get_temp(unit_id()))
        .unwrap_or_default()
}

/// The grid of a graph window in the mode: on by default, coarse step.
#[derive(Clone, Copy, Debug, PartialEq)]
struct GridState {
    on: bool,
    fine: bool,
}

impl Default for GridState {
    fn default() -> Self {
        Self {
            on: true,
            fine: false,
        }
    }
}

fn grid_id(graph: NodeId) -> egui::Id {
    egui::Id::new(("stokesforge_layout_grid", graph))
}

fn grid_state(ctx: &egui::Context, graph: NodeId) -> GridState {
    ctx.data(|data| data.get_temp(grid_id(graph)))
        .unwrap_or_default()
}

fn set_grid_state(ctx: &egui::Context, graph: NodeId, state: GridState) {
    ctx.data_mut(|data| data.insert_temp(grid_id(graph), state));
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

/// Feeds the view of the frame: the unit of the sizes written on the boxes and
/// the snapping grid.
pub(super) fn feed_view(ctx: &egui::Context, view: &mut GraphView, graph: NodeId) {
    let unit = length_unit(ctx);
    let grid = grid_state(ctx, graph);
    view.set_length_unit(unit);
    view.set_layout_grid(grid.on.then(|| grid_step_points(unit, grid.fine)));
}

// ---------------------------------------------------------------------------
// The application side: toggle, publication
// ---------------------------------------------------------------------------

impl StokesForgeApp {
    /// Publishes what the graph windows need to know about the application
    /// (the foreground graph, the length unit) and runs the toggles the menus
    /// asked for.
    pub(super) fn run_layout_mode_requests(
        &mut self,
        ctx: &egui::Context,
        graph: Option<NodeId>,
        actions: &mut Vec<Action>,
    ) {
        ctx.data_mut(|data| {
            data.insert_temp(foreground_id(), graph);
            data.insert_temp(unit_id(), self.state.length_unit());
        });
        let requests: Vec<NodeId> = ctx.data_mut(|data| {
            std::mem::take(data.get_temp_mut_or_default::<Vec<NodeId>>(toggle_requests_id()))
        });
        for graph in requests {
            self.toggle_layout_mode(graph, None, actions);
        }
    }

    /// Turns the Page Layout Mode of `graph` on or off (`None`: toggles).
    /// Entering on a grid graph switches it to Free placement first (its own
    /// undo step) and says so in the status bar.
    pub(super) fn toggle_layout_mode(
        &mut self,
        graph: NodeId,
        on: Option<bool>,
        actions: &mut Vec<Action>,
    ) {
        let now = self
            .read_graph_view(graph, GraphView::layout_mode)
            .unwrap_or(false);
        let want = on.unwrap_or(!now);
        if want == now {
            return;
        }
        if want {
            let grid = stokesforge_core::compose::current_plot_spec(self.state.tree(), graph)
                .is_some_and(|spec| spec.layer_placement != LayerPlacement::Free);
            if grid {
                actions.push(Action::SetLayerPlacement { graph, free: true });
                self.state.notice(tr!("status-layout-mode-free"));
            }
        }
        self.with_graph_view(graph, |view| view.set_layout_mode(want));
    }
}

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

/// Esc leaves the mode, `G` toggles the grid, the arrows nudge (Shift: ten
/// steps); consumed, for the foreground graph only and only when no text field
/// has the keyboard.
pub(super) fn handle_keys(ui: &egui::Ui, view: &mut GraphView, graph: NodeId) {
    let ctx = ui.ctx();
    if !view.layout_mode()
        || foreground_graph(ctx) != Some(graph)
        || keyboard_focus_outside_grid(ctx)
        || super::project_manager::owns_keyboard(ctx)
        || egui::Popup::is_any_open(ctx)
    {
        return;
    }
    use super::shortcuts::{
        KEY_ARROW_DOWN, KEY_ARROW_LEFT, KEY_ARROW_RIGHT, KEY_ARROW_UP, KEY_ESCAPE, KEY_LAYOUT_GRID,
        KEY_SHIFT_ARROW_DOWN, KEY_SHIFT_ARROW_LEFT, KEY_SHIFT_ARROW_RIGHT, KEY_SHIFT_ARROW_UP,
    };
    let (escape, grid, steps, now) = ui.input_mut(|input| {
        let escape = input.consume_shortcut(&KEY_ESCAPE);
        let grid = input.consume_shortcut(&KEY_LAYOUT_GRID);
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
        (escape, grid, steps, input.time)
    });
    if escape {
        view.set_layout_mode(false);
        return;
    }
    if grid {
        let mut state = grid_state(ctx, graph);
        state.on = !state.on;
        set_grid_state(ctx, graph, state);
        feed_view(ctx, view, graph);
    }
    if steps != (0, 0) {
        view.nudge(steps, now);
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
    let (Some(page), Some(scale)) = (view.layout_page(), view.screen_scale()) else {
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
// Display scale
// ---------------------------------------------------------------------------

/// A small weak "{percent} %" in the bottom-right corner of the figure area.
pub(super) fn paint_display_scale(ui: &egui::Ui, area: egui::Rect, scale: Option<f32>) {
    let Some(scale) = scale.filter(|scale| scale.is_finite() && *scale > 0.0) else {
        return;
    };
    let percent = (scale * 100.0).round() as i64;
    ui.painter().text(
        area.right_bottom() - egui::vec2(6.0, 3.0),
        egui::Align2::RIGHT_BOTTOM,
        tr!("plot-display-scale", percent = percent),
        egui::FontId::proportional(10.0),
        ui.visuals().weak_text_color(),
    );
}

// ---------------------------------------------------------------------------
// Inspector strip
// ---------------------------------------------------------------------------

fn length_quantity(unit: LengthUnit, min_points: f64, max_points: f64) -> Quantity {
    Quantity::real(unit.step(), unit.step() * 10.0, unit.decimals())
        .between(unit.from_points(min_points), unit.from_points(max_points))
        .short()
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

/// The inspector strip under the figure: the figure size, the numbers of the
/// selected box, the plot area, the arrangement menus and the grid. Every
/// numeric field commits on Enter, on leaving the field or at a step (never
/// each frame) as one [`Action::SetFreeLayout`].
pub(super) fn inspector(
    ui: &mut egui::Ui,
    view: &GraphView,
    spec: &PlotSpec,
    graph: NodeId,
    actions: &mut Vec<Action>,
) {
    let ctx = ui.ctx().clone();
    let unit = length_unit(&ctx);
    let symbol = unit.symbol();
    let page = view.layout_page();
    ui.spacing_mut().item_spacing.x = 4.0;
    ui.label(tr!("layout-mode-figure"));
    if let Some(page) = page {
        let mut size = (unit.from_points(page.width), unit.from_points(page.height));
        let quantity = length_quantity(unit, MIN_PAGE_EXTENT, stokesforge_plot::MAX_FIGURE_EXTENT);
        let mut changed = number_field(ui, ("layout_figure", 0), &mut size.0, &quantity).changed;
        ui.label(tr!("layout-mode-by"));
        changed |= number_field(ui, ("layout_figure", 1), &mut size.1, &quantity).changed;
        ui.label(symbol);
        if changed {
            let wanted = FreePage {
                width: unit.to_points(size.0),
                height: unit.to_points(size.1),
                ..page
            };
            let new_page = wanted.sanitized().unwrap_or(page);
            let frames = scaled_to_page(&view.layout_boxes(), page, new_page);
            actions.push(Action::SetFreeLayout {
                graph,
                frames,
                page: Some(new_page),
            });
        }
    }
    ui.separator();
    let active = view.active_panel(spec);
    let extra = view.extra_layers(spec);
    if let (Some(page), Some(selected)) = (page, view.selected_box()) {
        ui.label(tr!("layout-mode-layer", n = (active + 1).to_string()));
        let o = selected.outer;
        let mut fields = [
            unit.from_points(o.left - page.left),
            unit.from_points(o.top - page.top),
            unit.from_points(o.width),
            unit.from_points(o.height),
        ];
        let position = length_quantity(unit, 0.0, page.width.max(page.height));
        let size = length_quantity(unit, MIN_FRAME_SIZE, page.width.max(page.height));
        let mut changed = false;
        for (index, (label, quantity)) in [
            (tr!("layout-mode-left"), &position),
            (tr!("layout-mode-top"), &position),
            (tr!("layout-mode-width"), &size),
            (tr!("layout-mode-height"), &size),
        ]
        .into_iter()
        .enumerate()
        {
            ui.label(label);
            changed |=
                number_field(ui, ("layout_box", index), &mut fields[index], quantity).changed;
        }
        if changed {
            let outer = box_in_page(
                &selected,
                page,
                (
                    unit.to_points(fields[0]),
                    unit.to_points(fields[1]),
                    unit.to_points(fields[2]),
                    unit.to_points(fields[3]),
                ),
            );
            actions.push(Action::SetFreeLayout {
                graph,
                frames: vec![(selected.base, selected.frame_for(outer))],
                page: Some(page),
            });
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
            unit = symbol
        )));
        ui.separator();
    }
    let cells = stokesforge_core::compose::arrange_targets(spec, active, &extra).len();
    ui.add_enabled_ui(cells >= 2, |ui| {
        ui.menu_button(tr!("menu-layout-align"), |ui| {
            super::menus::align_items(ui, graph, active, &extra, actions);
        });
        ui.menu_button(tr!("layout-mode-same-size"), |ui| {
            super::menus::same_size_items(ui, graph, active, &extra, cells, actions);
        });
    });
    ui.add_enabled_ui(cells >= 3, |ui| {
        ui.menu_button(tr!("layout-mode-distribute"), |ui| {
            super::menus::distribute_items(ui, graph, active, &extra, cells, actions);
        });
    });
    ui.separator();
    let mut grid = grid_state(&ctx, graph);
    let before = grid;
    if ui
        .add(egui::Button::new(tr!("layout-mode-grid")).selected(grid.on))
        .on_hover_text(tr!("layout-mode-grid", "hint"))
        .clicked()
    {
        grid.on = !grid.on;
    }
    if ui
        .add(egui::Button::new(tr!("layout-mode-grid-coarse")).selected(!grid.fine))
        .on_hover_text(tr!("layout-mode-grid-coarse", "hint"))
        .clicked()
    {
        grid.fine = false;
    }
    if ui
        .add(egui::Button::new(tr!("layout-mode-grid-fine")).selected(grid.fine))
        .on_hover_text(tr!("layout-mode-grid-fine", "hint"))
        .clicked()
    {
        grid.fine = true;
    }
    if grid != before {
        set_grid_state(&ctx, graph, grid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn run_with(
        view: &mut GraphView,
        graph: NodeId,
        snapshot: &stokesforge_session::Snapshot,
        foreground: bool,
        events: Vec<egui::Event>,
    ) {
        let ctx = egui::Context::default();
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        ctx.data_mut(|data| data.insert_temp(foreground_id(), foreground.then_some(graph)));
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 700.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            super::super::graph_window::graph_view_contents(
                ui,
                view,
                snapshot,
                graph,
                &mut Vec::new(),
            );
        });
        output.textures_delta.clear();
    }

    #[test]
    fn escape_leaves_the_mode_of_the_foreground_graph_only() {
        let (session, graph) = super::super::graph_window::tests::plotted_graph();
        let snapshot = session.handle().snapshot().unwrap();
        let mut view = GraphView::new();
        view.set_layout_mode(true);
        run_with(
            &mut view,
            graph,
            &snapshot,
            false,
            vec![key(egui::Key::Escape)],
        );
        assert!(view.layout_mode(), "a background graph keeps the mode");
        run_with(
            &mut view,
            graph,
            &snapshot,
            true,
            vec![key(egui::Key::Escape)],
        );
        assert!(!view.layout_mode());
        session.shutdown().unwrap();
    }
}
