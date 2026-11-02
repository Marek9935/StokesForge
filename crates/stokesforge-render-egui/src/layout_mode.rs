//! The Page Layout Mode of the graph window (#426d prototype).
//!
//! A view state, never saved: while it is on, the figure is a layout table.
//! Every layer is a **graph box** (its plot area and its axis and title
//! bands) with eight handles and its size written on it, the figure is a
//! **page** whose corners can be pulled, and the primary drag does nothing
//! else (no pan, no zoom, no object drag). All the geometry is in
//! `stokesforge_render::free_gesture` (stored page coordinates); this module
//! only maps pointer events to it, previews the result through
//! [`FramePreview`] and paints the overlay. The overlay is a screen marker: it
//! enters neither the cache key nor the export.

use egui::{
    Color32, Context, CursorIcon, FontId, Painter, PointerButton, Pos2, Rect, Response, Shape,
    Stroke, StrokeKind, Visuals,
};
use stokesforge_plot::{FreeFrame, FreePage, LengthUnit, PlotSpec};
use stokesforge_render::{
    figure_to_stored, graph_boxes, layout_handle_at, moved_box, nudged_box, resized_page,
    scaled_to_page, stored_to_figure, GraphBox, Guide, LayoutHandle, Snap, MAGNET_THRESHOLD_POINTS,
};

use crate::{FramePreview, GraphEvent, GraphView};

/// Reach of a handle on screen, in points either side of an outer boundary.
/// The page corner zones are twice as wide, outside the page.
pub const LAYOUT_TOLERANCE_POINTS: f64 = 6.0;

/// Room the host must leave around the figure, and let its clip rectangle
/// reach, so that the page corner handles can be grabbed outside the page: the
/// interaction area grows by that much in the mode.
pub const LAYOUT_GRAB_MARGIN: f32 = 2.0 * LAYOUT_TOLERANCE_POINTS as f32;

/// Quiet time after the last arrow key before a nudge burst is committed.
const NUDGE_QUIET_SECONDS: f64 = 0.7;

/// Side of the square handles, in screen points.
const HANDLE_SIZE: f32 = 7.0;

/// Below this many screen points between two grid lines, only multiples of
/// the step are drawn.
const MIN_GRID_GAP: f32 = 4.0;

/// The step of an arrow key (and the coarse grid) when no grid step is set, in
/// points: 5 mm for cm and mm, 10 pt, a quarter of an inch.
fn default_step(unit: LengthUnit) -> f64 {
    match unit {
        LengthUnit::Cm | LengthUnit::Mm => LengthUnit::Mm.to_points(5.0),
        LengthUnit::Pt => 10.0,
        LengthUnit::In => LengthUnit::In.to_points(0.25),
    }
}

/// A size in `unit` as written on a box: cm 1 decimal, mm 0, pt 0, in 2.
fn size_text(points: f64, unit: LengthUnit) -> String {
    let decimals = match unit {
        LengthUnit::Cm => 1,
        LengthUnit::Mm | LengthUnit::Pt => 0,
        LengthUnit::In => 2,
    };
    format!("{:.decimals$}", unit.from_points(points))
}

/// What the mode knows after a paint: the boxes with the preview applied, the
/// page, and the selection.
#[derive(Debug, Clone)]
pub(crate) struct LayoutView {
    boxes: Vec<GraphBox>,
    page: FreePage,
    /// Selected layers as base layers, the active one first.
    selected: Vec<usize>,
}

/// A box or page-corner drag in progress. Everything is frozen at the press:
/// the display scale (the figure may grow under the pointer), the page and the
/// boxes.
#[derive(Debug, Clone)]
pub(crate) struct LayoutDrag {
    handle: LayoutHandle,
    boxes: Vec<GraphBox>,
    page: FreePage,
    press: Pos2,
    scale: f32,
    pub(crate) guides: Vec<Guide>,
}

/// A burst of arrow-key nudges not yet committed.
#[derive(Debug, Clone)]
struct Nudge {
    start: Vec<GraphBox>,
    page: FreePage,
    total: (f64, f64),
    last: f64,
}

/// The state of the mode on a [`GraphView`].
#[derive(Debug, Default)]
pub(crate) struct LayoutState {
    pub(crate) mode: bool,
    grid: Option<f64>,
    unit: LengthUnit,
    pub(crate) drag: Option<LayoutDrag>,
    nudge: Option<Nudge>,
    pub(crate) view: Option<LayoutView>,
}

impl LayoutState {
    /// Drops the gesture in progress (the layers were renumbered).
    pub(crate) fn forget_gestures(&mut self) {
        self.drag = None;
        self.nudge = None;
        self.view = None;
    }

    /// One nudge or grid step in points.
    fn step(&self) -> f64 {
        self.grid
            .filter(|step| step.is_finite() && *step > 0.0)
            .unwrap_or_else(|| default_step(self.unit))
    }
}

fn differs(a: FreeFrame, b: FreeFrame) -> bool {
    (a.left - b.left).abs() >= 0.005
        || (a.top - b.top).abs() >= 0.005
        || (a.width - b.width).abs() >= 0.005
        || (a.height - b.height).abs() >= 0.005
}

fn page_differs(a: FreePage, b: FreePage) -> bool {
    (a.left - b.left).abs() >= 0.005
        || (a.top - b.top).abs() >= 0.005
        || (a.width - b.width).abs() >= 0.005
        || (a.height - b.height).abs() >= 0.005
}

fn luma(color: Color32) -> u32 {
    u32::from(color.r()) * 3 + u32::from(color.g()) * 6 + u32::from(color.b())
}

/// The selection colour of the interface, the darker of its two shades so that
/// it reads on the white of the figure in both themes.
fn accent(visuals: &Visuals) -> Color32 {
    let fill = visuals.selection.bg_fill;
    let stroke = visuals.selection.stroke.color;
    let darker = if luma(fill) <= luma(stroke) {
        fill
    } else {
        stroke
    };
    // A pale theme selection (light green) would not read on white: darken it
    // until white text on it stays legible.
    if luma(darker) > 1000 {
        let shade = |v: u8| (u16::from(v) * 55 / 100) as u8;
        Color32::from_rgb(shade(darker.r()), shade(darker.g()), shade(darker.b()))
    } else {
        darker
    }
}

fn resize_cursor(left_right: bool, top_bottom: bool, same_diagonal: bool) -> CursorIcon {
    match (left_right, top_bottom) {
        (false, false) => CursorIcon::Move,
        (true, false) => CursorIcon::ResizeHorizontal,
        (false, true) => CursorIcon::ResizeVertical,
        (true, true) if same_diagonal => CursorIcon::ResizeNwSe,
        (true, true) => CursorIcon::ResizeNeSw,
    }
}

fn handle_cursor(handle: &LayoutHandle) -> CursorIcon {
    match handle {
        LayoutHandle::Page { right, bottom } => resize_cursor(true, true, right == bottom),
        LayoutHandle::Box { edges, .. } => resize_cursor(
            edges.left || edges.right,
            edges.top || edges.bottom,
            edges.left == edges.top,
        ),
    }
}

impl GraphView {
    /// Turns the Page Layout Mode on or off (#426d). Leaving it commits a
    /// pending nudge burst and drops the gesture in progress. The mode only
    /// acts on a free placement; the shell switches a grid graph to it first.
    pub fn set_layout_mode(&mut self, on: bool) {
        if self.layout.mode == on {
            return;
        }
        if on {
            self.nav_drag = None;
            self.drag_box = None;
            self.annotation_drag = None;
            self.text_drag = None;
            self.shape_drag = None;
            self.free_drag = None;
            self.frame_drag = None;
            self.layout.mode = true;
            return;
        }
        self.commit_nudge();
        if self.layout.drag.take().is_some() {
            self.set_frame_preview(None);
        }
        self.layout.mode = false;
        self.layout.view = None;
    }

    /// Whether the Page Layout Mode is on.
    pub fn layout_mode(&self) -> bool {
        self.layout.mode
    }

    /// Sets the snapping grid step in points (`None`: no grid).
    pub fn set_layout_grid(&mut self, step: Option<f64>) {
        self.layout.grid = step.filter(|step| step.is_finite() && *step > 0.0);
    }

    /// The snapping grid step in points, `None` when the grid is off.
    pub fn layout_grid(&self) -> Option<f64> {
        self.layout.grid
    }

    /// The unit the sizes written on the boxes are shown in.
    pub fn set_length_unit(&mut self, unit: LengthUnit) {
        self.layout.unit = unit;
    }

    /// The page, in stored page coordinates (mode on, free layout, after a
    /// paint); with the preview of the gesture in progress.
    pub fn layout_page(&self) -> Option<FreePage> {
        self.layout.view.as_ref().map(|view| view.page)
    }

    /// The graph boxes back to front, with the preview of the gesture in
    /// progress applied (empty outside the mode).
    pub fn layout_boxes(&self) -> Vec<GraphBox> {
        self.layout
            .view
            .as_ref()
            .map_or_else(Vec::new, |view| view.boxes.clone())
    }

    /// The graph box of the active layer, with the preview applied.
    pub fn selected_box(&self) -> Option<GraphBox> {
        let view = self.layout.view.as_ref()?;
        let base = *view.selected.first()?;
        view.boxes.iter().find(|graph| graph.base == base).copied()
    }

    /// The screen position of the stored page point `(x, y)`, in points.
    pub fn stored_to_screen(&self, x: f64, y: f64) -> Option<Pos2> {
        let (_, layout) = self.figure_memo.as_ref()?;
        let display = self.display_rect?;
        let scale = self.display_scale()?;
        let (fx, fy) = stored_to_figure(layout, x, y);
        Some(display.min + egui::vec2(fx as f32, fy as f32) * scale)
    }

    /// Screen points per stored point (the display scale of the figure).
    pub fn screen_scale(&self) -> Option<f32> {
        self.display_scale()
    }

    /// Moves the selected layers (the active one and the others) by `steps`
    /// grid steps (the default step of the unit when the grid is off), held
    /// inside the page, previewed. The burst is committed as **one**
    /// [`GraphEvent::LayoutEdited`] after 0.7 s without a nudge (`now` and the
    /// frame clock are the same clock, `egui::InputState::time`), when the mode
    /// is left or when a drag starts.
    pub fn nudge(&mut self, steps: (i32, i32), now: f64) {
        if !self.layout.mode || steps == (0, 0) || self.layout.drag.is_some() {
            return;
        }
        let Some(view) = self.layout.view.clone() else {
            return;
        };
        if self.layout.nudge.is_none() {
            let start: Vec<GraphBox> = view
                .selected
                .iter()
                .filter_map(|base| view.boxes.iter().find(|graph| graph.base == *base).copied())
                .collect();
            if start.is_empty() {
                return;
            }
            self.layout.nudge = Some(Nudge {
                start,
                page: view.page,
                total: (0.0, 0.0),
                last: now,
            });
        }
        let step = self.layout.step();
        let Some(burst) = self.layout.nudge.as_mut() else {
            return;
        };
        burst.total.0 += f64::from(steps.0) * step;
        burst.total.1 += f64::from(steps.1) * step;
        burst.last = now;
        let frees: Vec<(usize, FreeFrame)> = burst
            .start
            .iter()
            .map(|graph| (graph.base, nudged_box(graph, burst.total, burst.page)))
            .collect();
        // Keep the total at what the reference box achieved, so that keys
        // against the page edge do not pile up an invisible overshoot.
        if let (Some(first), Some((_, frame))) = (burst.start.first(), frees.first()) {
            burst.total = (frame.left - first.frame.left, frame.top - first.frame.top);
        }
        let page = burst.page;
        self.set_frame_preview(Some(FramePreview {
            frees,
            page: Some(page),
            ..FramePreview::default()
        }));
    }

    /// Commits the pending nudge burst as one event, if it moved anything.
    fn commit_nudge(&mut self) {
        let Some(burst) = self.layout.nudge.take() else {
            return;
        };
        let preview = self.frame_preview.clone();
        match preview {
            Some(preview)
                if preview.frees.iter().any(|(base, frame)| {
                    burst
                        .start
                        .iter()
                        .find(|graph| graph.base == *base)
                        .is_none_or(|graph| differs(graph.frame, *frame))
                }) =>
            {
                self.events.push(GraphEvent::LayoutEdited {
                    frames: preview.frees,
                    page: Some(burst.page),
                });
                self.hold_preview();
            }
            _ => self.set_frame_preview(None),
        }
    }

    /// Commits a burst that has been quiet long enough, and asks for the
    /// repaint that will check again.
    fn tick_nudge(&mut self, context: &Context) {
        let Some(last) = self.layout.nudge.as_ref().map(|burst| burst.last) else {
            return;
        };
        let now = context.input(|input| input.time);
        if now - last >= NUDGE_QUIET_SECONDS {
            self.commit_nudge();
        } else {
            context.request_repaint_after(std::time::Duration::from_secs_f64(
                NUDGE_QUIET_SECONDS - (now - last) + 0.01,
            ));
        }
    }

    /// The boxes and the page as the figure is laid out now, with the preview
    /// applied. `None` unless the figure was laid out in free placement.
    fn layout_geometry(&self, spec: &PlotSpec) -> Option<(Vec<GraphBox>, FreePage)> {
        let (_, layout) = self.figure_memo.as_ref()?;
        let page = layout.free_page_rect()?;
        let boxes = match &self.frame_preview {
            Some(preview) => {
                let mut previewed = spec.clone();
                preview.apply(&mut previewed);
                graph_boxes(&previewed, layout)
            }
            None => graph_boxes(spec, layout),
        };
        Some((boxes, page))
    }

    /// What the overlay and the accessors read, computed once per paint.
    pub(crate) fn compute_layout_view(&self, spec: &PlotSpec) -> Option<LayoutView> {
        let (boxes, page) = self.layout_geometry(spec)?;
        let bases = spec.cell_bases();
        let mut selected = Vec::new();
        if let Some(active) = bases.get(self.active_panel(spec)) {
            selected.push(*active);
        }
        selected.extend(self.extra_layers(spec));
        Some(LayoutView {
            boxes,
            page,
            selected,
        })
    }

    /// The handle under the screen position `pos`, from the figure as drawn.
    fn layout_handle_under(
        &self,
        pos: Pos2,
        boxes: &[GraphBox],
        page: FreePage,
    ) -> Option<LayoutHandle> {
        let (_, layout) = self.figure_memo.as_ref()?;
        let scale = f64::from(self.display_scale().filter(|scale| *scale > 0.0)?);
        let figure = self.figure_position(pos)?;
        let (x, y) = figure_to_stored(layout, f64::from(figure.x), f64::from(figure.y));
        layout_handle_at(boxes, page, x, y, LAYOUT_TOLERANCE_POINTS / scale)
    }

    /// The gestures of the mode: selection by click, box and page corner drags,
    /// cursors. Called instead of every other gesture of the figure.
    pub(crate) fn handle_layout_mode(
        &mut self,
        response: &Response,
        context: &Context,
        spec: &PlotSpec,
    ) {
        self.tick_nudge(context);
        let Some((boxes, page)) = self.layout_geometry(spec) else {
            return;
        };
        if response.drag_stopped_by(PointerButton::Primary) {
            self.finish_layout_drag();
            return;
        }
        let pointer = response.interact_pointer_pos();
        if response.clicked() || response.secondary_clicked() {
            let shift = context.input(|input| input.modifiers.shift);
            if let Some(LayoutHandle::Box { base, .. }) =
                pointer.and_then(|pos| self.layout_handle_under(pos, &boxes, page))
            {
                if response.clicked() && shift {
                    self.toggle_extra_layer(base, spec);
                } else if response.clicked() {
                    self.set_active_panel(base, spec);
                    self.clear_extra_layers();
                } else if !self.layer_in_selection(base, spec) {
                    // A right-click inside the selection keeps it.
                    self.set_active_panel(base, spec);
                    self.clear_extra_layers();
                }
            }
        }
        if response.dragged_by(PointerButton::Primary) {
            self.layout_drag(response, context, spec, &boxes, page);
        }
        let cursor = match &self.layout.drag {
            Some(drag) => Some(handle_cursor(&drag.handle)),
            None if response.hovered() => response
                .hover_pos()
                .and_then(|pos| self.layout_handle_under(pos, &boxes, page))
                .map(|handle| handle_cursor(&handle)),
            None => None,
        };
        if let Some(icon) = cursor {
            context.set_cursor_icon(icon);
        }
    }

    /// Starts and follows a drag of a box or a page corner; previews it.
    fn layout_drag(
        &mut self,
        response: &Response,
        context: &Context,
        spec: &PlotSpec,
        boxes: &[GraphBox],
        page: FreePage,
    ) {
        let Some(pos) = response.interact_pointer_pos() else {
            return;
        };
        if self.layout.drag.is_none() {
            let origin = context
                .input(|input| input.pointer.press_origin())
                .unwrap_or(pos - response.drag_delta());
            let Some(handle) = self.layout_handle_under(origin, boxes, page) else {
                return;
            };
            let Some(scale) = self.display_scale().filter(|scale| *scale > 0.0) else {
                return;
            };
            self.commit_nudge();
            if let LayoutHandle::Box { base, .. } = handle {
                if !self.layer_in_selection(base, spec) {
                    self.set_active_panel(base, spec);
                    self.clear_extra_layers();
                }
            }
            self.layout.drag = Some(LayoutDrag {
                handle,
                boxes: boxes.to_vec(),
                page,
                press: origin,
                scale,
                guides: Vec::new(),
            });
        }
        let Some(drag) = self.layout.drag.clone() else {
            return;
        };
        let delta = (pos - drag.press) / drag.scale;
        let delta = (f64::from(delta.x), f64::from(delta.y));
        let shift = context.input(|input| input.modifiers.shift);
        let snap = Snap {
            grid: self.layout.grid,
            threshold: MAGNET_THRESHOLD_POINTS / f64::from(drag.scale),
            enabled: !shift,
        };
        let (frees, shown_page, guides) = match drag.handle {
            LayoutHandle::Box { base, edges } => {
                let Some(start) = drag.boxes.iter().find(|graph| graph.base == base) else {
                    return;
                };
                let others: Vec<FreePage> = drag
                    .boxes
                    .iter()
                    .filter(|graph| graph.base != base)
                    .map(|graph| graph.outer)
                    .collect();
                let (frame, guides) = moved_box(start, &edges, delta, drag.page, &others, snap);
                (vec![(base, frame)], drag.page, guides)
            }
            LayoutHandle::Page { right, bottom } => {
                let new_page = resized_page(drag.page, (right, bottom), delta, snap);
                (
                    scaled_to_page(&drag.boxes, drag.page, new_page),
                    new_page,
                    Vec::new(),
                )
            }
        };
        if let Some(active) = self.layout.drag.as_mut() {
            active.guides = guides;
        }
        self.set_frame_preview(Some(FramePreview {
            frees,
            page: Some(shown_page),
            ..FramePreview::default()
        }));
    }

    /// The release: one [`GraphEvent::LayoutEdited`] if something moved.
    fn finish_layout_drag(&mut self) {
        let Some(drag) = self.layout.drag.take() else {
            return;
        };
        let moved = self.frame_preview.clone().filter(|preview| {
            preview
                .page
                .is_some_and(|page| page_differs(page, drag.page))
                || preview.frees.iter().any(|(base, frame)| {
                    drag.boxes
                        .iter()
                        .find(|graph| graph.base == *base)
                        .is_none_or(|graph| differs(graph.frame, *frame))
                })
        });
        match moved {
            Some(preview) => {
                self.events.push(GraphEvent::LayoutEdited {
                    frames: preview.frees,
                    page: preview.page,
                });
                self.hold_preview();
            }
            None => self.set_frame_preview(None),
        }
    }

    /// Paints the layout table over the figure: grid, page, boxes with their
    /// handles and sizes, magnet guides. Screen only.
    pub(crate) fn paint_layout_overlay(&self, painter: &Painter, visuals: &Visuals) {
        let Some(view) = &self.layout.view else {
            return;
        };
        let accent = accent(visuals);
        let corner = |x: f64, y: f64| self.stored_to_screen(x, y);
        let Some(page_rect) = (|| {
            Some(Rect::from_two_pos(
                corner(view.page.left, view.page.top)?,
                corner(
                    view.page.left + view.page.width,
                    view.page.top + view.page.height,
                )?,
            ))
        })() else {
            return;
        };
        let scale = self.display_scale().unwrap_or(1.0);

        self.paint_grid(painter, view, page_rect, scale);
        painter.rect_stroke(
            page_rect,
            0.0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(110, 110, 125, 170)),
            StrokeKind::Outside,
        );
        let dragged_corner = match self.layout.drag.as_ref().map(|drag| drag.handle) {
            Some(LayoutHandle::Page { right, bottom }) => Some((right, bottom)),
            _ => None,
        };
        paint_page_corners(painter, page_rect, dragged_corner, accent);

        let in_selection = |graph: &GraphBox| view.selected.contains(&graph.base);
        for selected in [false, true] {
            for graph in view.boxes.iter().filter(|g| in_selection(g) == selected) {
                self.paint_box(painter, graph, selected, accent);
            }
        }
        for guide in self.magnet_guides() {
            let ends = if guide.vertical {
                [(guide.at, guide.from), (guide.at, guide.to)]
            } else {
                [(guide.from, guide.at), (guide.to, guide.at)]
            };
            let points: Vec<Pos2> = ends
                .iter()
                .filter_map(|(x, y)| self.stored_to_screen(*x, *y))
                .collect();
            if points.len() == 2 {
                painter.extend(Shape::dashed_line(
                    &points,
                    Stroke::new(1.0, Color32::from_rgb(232, 62, 140)),
                    5.0,
                    3.0,
                ));
            }
        }
    }

    /// The faint grid lines on the page.
    fn paint_grid(&self, painter: &Painter, view: &LayoutView, page_rect: Rect, scale: f32) {
        let Some(step) = self.layout.grid else {
            return;
        };
        let gap = step as f32 * scale;
        if !gap.is_finite() || gap <= 0.0 {
            return;
        }
        let every = (MIN_GRID_GAP / gap).ceil().max(1.0) as u32;
        let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(100, 100, 125, 25));
        let step = step * f64::from(every);
        let mut at = 0.0;
        while at <= view.page.width {
            if let Some(top) = self.stored_to_screen(view.page.left + at, view.page.top) {
                painter.line_segment([top, Pos2::new(top.x, page_rect.bottom())], stroke);
            }
            at += step;
        }
        let mut at = 0.0;
        while at <= view.page.height {
            if let Some(left) = self.stored_to_screen(view.page.left, view.page.top + at) {
                painter.line_segment([left, Pos2::new(page_rect.right(), left.y)], stroke);
            }
            at += step;
        }
    }

    /// One graph box: outline, dashed plot area, eight handles, size pill.
    fn paint_box(&self, painter: &Painter, graph: &GraphBox, selected: bool, accent: Color32) {
        let rect_of = |r: FreePage| {
            Some(Rect::from_two_pos(
                self.stored_to_screen(r.left, r.top)?,
                self.stored_to_screen(r.left + r.width, r.top + r.height)?,
            ))
        };
        let Some(outer) = rect_of(graph.outer) else {
            return;
        };
        let frame = graph.frame;
        let plot = rect_of(FreePage {
            left: frame.left,
            top: frame.top,
            width: frame.width,
            height: frame.height,
        });
        let colour = if selected {
            accent
        } else {
            Color32::from_rgb(105, 105, 120)
        };
        let width = if selected { 1.5 } else { 1.0 };
        painter.rect_stroke(outer, 0.0, Stroke::new(width, colour), StrokeKind::Inside);
        if let Some(plot) = plot {
            let points = vec![
                plot.left_top(),
                plot.right_top(),
                plot.right_bottom(),
                plot.left_bottom(),
                plot.left_top(),
            ];
            painter.extend(Shape::dashed_line(
                &points,
                Stroke::new(1.0, colour.gamma_multiply(0.7)),
                4.0,
                3.0,
            ));
        }
        let (cx, cy) = (outer.center().x, outer.center().y);
        let handles = [
            outer.left_top(),
            Pos2::new(cx, outer.top()),
            outer.right_top(),
            Pos2::new(outer.right(), cy),
            outer.right_bottom(),
            Pos2::new(cx, outer.bottom()),
            outer.left_bottom(),
            Pos2::new(outer.left(), cy),
        ];
        for at in handles {
            let square = Rect::from_center_size(at, egui::vec2(HANDLE_SIZE, HANDLE_SIZE));
            painter.rect_filled(square, 1.0, Color32::WHITE);
            painter.rect_stroke(square, 1.0, Stroke::new(1.0, colour), StrokeKind::Inside);
        }
        let text = format!(
            "{} \u{d7} {} {}",
            size_text(graph.outer.width, self.layout.unit),
            size_text(graph.outer.height, self.layout.unit),
            self.layout.unit.symbol()
        );
        let galley = painter.layout_no_wrap(text, FontId::proportional(11.0), Color32::WHITE);
        let pill = Rect::from_center_size(
            Pos2::new(cx, outer.top() + 8.0 + galley.size().y / 2.0),
            galley.size() + egui::vec2(10.0, 4.0),
        );
        if pill.width() + 2.0 * HANDLE_SIZE <= outer.width() {
            let fill = if selected {
                accent
            } else {
                Color32::from_rgba_unmultiplied(45, 45, 55, 225)
            };
            painter.rect_filled(pill, 4.0, fill);
            painter.galley(pill.center() - galley.size() / 2.0, galley, Color32::WHITE);
        }
    }
}

/// The four L-shaped page corner handles, outside the page.
fn paint_page_corners(
    painter: &Painter,
    page: Rect,
    dragged: Option<(bool, bool)>,
    accent: Color32,
) {
    const GAP: f32 = 3.0;
    const ARM: f32 = 12.0;
    for right in [false, true] {
        for bottom in [false, true] {
            let (sx, sy) = (
                if right { 1.0 } else { -1.0 },
                if bottom { 1.0 } else { -1.0 },
            );
            let corner = Pos2::new(
                if right { page.right() } else { page.left() },
                if bottom { page.bottom() } else { page.top() },
            );
            let vertex = corner + egui::vec2(sx * GAP, sy * GAP);
            let colour = if dragged == Some((right, bottom)) {
                accent
            } else {
                Color32::from_rgb(85, 85, 100)
            };
            painter.add(Shape::line(
                vec![
                    vertex + egui::vec2(-sx * ARM, 0.0),
                    vertex,
                    vertex + egui::vec2(0.0, -sy * ARM),
                ],
                Stroke::new(2.5, colour),
            ));
        }
    }
}
