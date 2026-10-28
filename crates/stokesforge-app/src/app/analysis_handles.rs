//! The handles of a calculation on the real graph (#199, mockup M10):
//! anchor points of a baseline, bounds of zones and ranges, starting
//! positions of peaks.
//!
//! They show only while the calculation is **selected** (one of its curves,
//! source or produced column, is the graph's selected curve, or its row in
//! the Results pane is chosen), never at rest nor in the export: they are
//! painted over the figure, not in `stokesforge-render`.
//!
//! Dragging a handle shows the result at once (the recomputed baseline is
//! painted on the fly). Releasing pushes **one**
//! [`Action::MoveCalculationHandle`], so one `UpdateCalculation` and a single
//! history entry per gesture, not per frame. The computation of the columns
//! that follows is the ordinary recalculation.

use eframe::egui;
use stokesforge_core::{NodeId, Tree};
use stokesforge_plot::PlotSpec;
use stokesforge_render_egui::GraphView;

use crate::theme;
use stokesforge_app_state::analysis::{series_rows, AnalysisForm, Edge, Handle, HandleSlot};
use stokesforge_app_state::state::Action;

/// Grab radius of a handle, in egui points.
const GRAB_RADIUS: f32 = 7.0;
/// Maximum number of points of the baseline preview while dragging.
const PREVIEW_POINTS: usize = 1500;

fn selection_id() -> egui::Id {
    egui::Id::new("stokesforge_selected_calculation")
}

/// The calculation chosen in the Results pane (egui memory: interface state,
/// neither `.stf` nor history).
pub(super) fn select_calculation(ctx: &egui::Context, calculation: Option<NodeId>) {
    ctx.data_mut(|d| match calculation {
        Some(id) => {
            d.insert_temp(selection_id(), id);
        }
        None => d.remove::<NodeId>(selection_id()),
    });
}

fn chosen_calculation(ctx: &egui::Context) -> Option<NodeId> {
    ctx.data(|d| d.get_temp::<NodeId>(selection_id()))
}

/// A drag in progress: where the handle is held, in data coordinates.
#[derive(Clone, Copy)]
struct Drag {
    x: f64,
    y: f64,
}

/// The calculation selected on this graph, and the layer to paint it on: that
/// of the first curve that belongs to it (source or produced column).
fn selected_on_graph(
    ctx: &egui::Context,
    view: &GraphView,
    tree: &Tree,
    spec: &PlotSpec,
) -> Option<(NodeId, usize)> {
    let belongs = |calc: NodeId, y: NodeId| {
        tree.calculation(calc).is_some_and(|c| {
            c.series.iter().any(|s| s.y == y)
                || tree
                    .calculation_of_column(y)
                    .is_some_and(|(owner, _)| owner == calc)
        })
    };
    let owner_of = |y: NodeId| {
        tree.calculation_of_column(y).map(|(id, _)| id).or_else(|| {
            tree.calculations()
                .iter()
                .find(|(_, c)| c.series.iter().any(|s| s.y == y))
                .map(|(id, _)| *id)
        })
    };
    let calc = match view.selected_series() {
        Some(target) => {
            let y = spec
                .panels
                .as_slice()
                .get(target.panel)?
                .series
                .get(target.index)?
                .y;
            owner_of(y)?
        }
        None => chosen_calculation(ctx)?,
    };
    spec.panels.iter().enumerate().find_map(|(panel, p)| {
        p.series
            .iter()
            .any(|s| belongs(calc, s.y))
            .then_some((calc, panel))
    })
}

/// Paints the handles of the selected calculation and handles their drag.
pub(super) fn calculation_handles(
    ui: &mut egui::Ui,
    view: &GraphView,
    tree: &Tree,
    graph: NodeId,
    spec: &PlotSpec,
    actions: &mut Vec<Action>,
) {
    let Some((id, panel)) = selected_on_graph(ui.ctx(), view, tree, spec) else {
        return;
    };
    let Some(calc) = tree.calculation(id) else {
        return;
    };
    let Some(plot) = view.plot_rect_screen(panel) else {
        return;
    };
    let form = AnalysisForm::from_calculation(id, calc);
    let handles = form.handles();
    if handles.is_empty() {
        return;
    }
    let painter = ui.painter().with_clip_rect(plot.intersect(ui.clip_rect()));
    let fill = theme::preview_zone_fill();
    let marker = theme::preview_marker_color();
    let outline = egui::Stroke::new(1.0, ui.visuals().text_color());
    let line = egui::Stroke::new(1.0, marker);

    // Where each handle is held: its place, or the point where it is dragged.
    let drags: Vec<Option<Drag>> = handles
        .iter()
        .map(|h| ui.data(|d| d.get_temp::<Drag>(handle_id(graph, h.slot))))
        .collect();
    let place = |h: &Handle, drag: Option<Drag>| -> (f64, Option<f64>) {
        match drag {
            Some(d) => (d.x, h.y.map(|_| d.y)),
            None => (h.x, h.y),
        }
    };
    let screen_x = |x: f64| view.data_to_screen(panel, x, 0.0).map(|p| p.x);

    // The zones: a veil between the two bounds.
    for (a, da) in handles.iter().zip(&drags) {
        let HandleSlot::Edge(set, i, Edge::Lo) = a.slot else {
            continue;
        };
        let Some((b, db)) = handles
            .iter()
            .zip(&drags)
            .find(|(h, _)| h.slot == HandleSlot::Edge(set, i, Edge::Hi))
        else {
            continue;
        };
        if let (Some(x0), Some(x1)) = (screen_x(place(a, *da).0), screen_x(place(b, *db).0)) {
            painter.rect_filled(
                egui::Rect::from_x_y_ranges(x0.min(x1)..=x0.max(x1), plot.y_range()),
                0.0,
                fill,
            );
        }
    }

    let mut moved: Option<(HandleSlot, f64, f64)> = None;
    for (handle, drag) in handles.iter().zip(&drags) {
        let (x, y) = place(handle, *drag);
        let at = match y {
            Some(y) => view.data_to_screen(panel, x, y),
            None => screen_x(x).map(|sx| egui::pos2(sx, plot.top() + GRAB_RADIUS + 2.0)),
        };
        let Some(at) = at else { continue };
        if handle.y.is_none() {
            painter.vline(at.x, plot.y_range(), line);
        }
        let hit = egui::Rect::from_center_size(at, egui::Vec2::splat(GRAB_RADIUS * 2.0));
        // Ctrl or middle click: the gesture is a pan of the graph (#318), the
        // handle lets it through. A handle already held keeps its drag.
        let passes_pan = drag.is_none()
            && ui.input(|i| {
                i.modifiers.command || i.pointer.button_down(egui::PointerButton::Middle)
            });
        let sense = if passes_pan {
            egui::Sense::hover()
        } else {
            egui::Sense::drag()
        };
        let response = ui.interact(hit, handle_id(graph, handle.slot), sense);
        let response = super::cursors::grab_handle(response);
        let hot = response.hovered() || response.dragged();
        painter.circle(
            at,
            if hot { GRAB_RADIUS } else { GRAB_RADIUS - 2.0 },
            marker,
            outline,
        );
        if response.dragged() {
            if let Some((p, nx, ny)) = response
                .interact_pointer_pos()
                .and_then(|pos| view.data_at(pos))
            {
                if p == panel {
                    ui.data_mut(|d| {
                        d.insert_temp(handle_id(graph, handle.slot), Drag { x: nx, y: ny })
                    });
                }
            }
        }
        if response.drag_stopped() {
            if let Some(d) = drag {
                moved = Some((handle.slot, d.x, d.y));
            }
            ui.data_mut(|d| d.remove::<Drag>(handle_id(graph, handle.slot)));
        }
    }

    // Immediate preview: the baseline (or background) recomputed with the
    // held handle.
    if let Some((handle, Some(drag))) = handles
        .iter()
        .zip(&drags)
        .find(|(_, d)| d.is_some())
        .map(|(h, d)| (h, *d))
    {
        let mut moved_form = form.clone();
        if moved_form.move_handle(handle.slot, drag.x, drag.y) {
            paint_helper(ui, view, tree, panel, &moved_form, &painter);
        }
    }

    if let Some((slot, x, y)) = moved {
        actions.push(Action::MoveCalculationHandle {
            calculation: id,
            slot,
            x,
            y,
        });
    }
}

fn handle_id(graph: NodeId, slot: HandleSlot) -> egui::Id {
    egui::Id::new(("stokesforge_calculation_handle", graph.raw(), slot))
}

/// The helper column (baseline, peak background) of the moved form, painted
/// on the figure.
fn paint_helper(
    ui: &egui::Ui,
    view: &GraphView,
    tree: &Tree,
    panel: usize,
    form: &AnalysisForm,
    painter: &egui::Painter,
) {
    let Ok(computed) = form.preview(tree) else {
        return;
    };
    let Some(series) = form.series.get(form.current) else {
        return;
    };
    let (rows_x, _) = series_rows(tree, series);
    let rank = form.preview_rank();
    let stroke = egui::Stroke::new(1.5, theme::preview_helper_color(ui.visuals().dark_mode));
    for column in computed
        .columns
        .iter()
        .filter(|c| c.series == rank && matches!(c.key.as_str(), "baseline" | "background"))
    {
        let step = (column.data.len() / PREVIEW_POINTS).max(1);
        let points: Vec<egui::Pos2> = rows_x
            .iter()
            .zip(&column.data)
            .step_by(step)
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .filter_map(|(x, y)| view.data_to_screen(panel, *x, *y))
            .collect();
        painter.add(egui::Shape::line(points, stroke));
    }
}
