//! The Page Layout Mode of the graph window (#426).
//!
//! A view state, never saved and never a placement: entering, leaving or
//! hovering in it sends no event and changes neither the model nor the cache.
//! The mode reads the placement from the figure as drawn.
//!
//! * **Free**: every layer is a **graph box** (its plot area and its axis and
//!   title bands) with its size written on it; the selected boxes show eight
//!   handles, the box under the pointer shows them faintly. The figure is a
//!   **page** whose corners can be pulled. All the geometry is in
//!   `stokesforge_render::free_gesture` (stored page coordinates).
//! * **Regular Grid**: the grid is not converted. Nothing moves and no single
//!   graph is resized; the internal boundaries between columns and rows, and
//!   the graph corners lying on one of them, are the handles. All the geometry
//!   is in `stokesforge_render::grid_gesture` (figure pixels).
//!
//! In both, the primary drag does nothing else (no pan, no zoom, no object
//! drag). This module only maps pointer events to the geometry, previews the
//! result through [`FramePreview`] and paints the overlay. The overlay is a
//! screen marker: it enters neither the cache key nor the export.

use egui::{
    Color32, Context, CursorIcon, FontId, Painter, PointerButton, Pos2, Rect, Response, Shape,
    Stroke, StrokeKind, Visuals,
};
use stokesforge_plot::{FreeFrame, FreePage, LayerPlacement, LengthUnit, PlotSpec};
use stokesforge_render::pipeline::FigureLayout;
use stokesforge_render::{
    boundary_group, figure_to_stored, graph_boxes, grid_corners, grid_handle_at, layout_handle_at,
    locked_corner_delta, moved_boundary, moved_box, nudged_box, resized_frames, resized_page,
    scaled_frames, scaled_to_page, stored_to_figure, BoundaryGroup, GraphBox, GridCorner,
    GridHandle, Guide, LayoutHandle, LayoutReach, Snap, EDGE_REACH_PX, MAGNET_THRESHOLD_POINTS,
};

use crate::{FramePreview, GraphEvent, GraphView};

/// Room the host must leave around the figure, and let its clip rectangle
/// reach, so that the page corner handles can be grabbed outside the page: the
/// interaction area grows by that much in the mode.
pub const LAYOUT_GRAB_MARGIN: f32 = 2.0 * EDGE_REACH_PX as f32;

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

/// A cell of a regular grid: its base layer and its place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GridCell {
    base: usize,
    row: usize,
    column: usize,
}

/// What the mode knows of a regular grid after a paint.
#[derive(Debug, Clone)]
pub(crate) struct GridView {
    columns: usize,
    rows: usize,
    cells: Vec<GridCell>,
    /// Graph corners on an internal boundary, in figure pixels. Empty when the
    /// grid has no frames to edit (compact fallback).
    corners: Vec<GridCorner>,
    /// Height of the title band, to turn a figure pixel into a stored point.
    band: f64,
    /// The layout has a frame width and height for every column and row.
    editable: bool,
}

/// What the mode knows after a paint: the boxes with the preview applied, the
/// page (free only), the grid (regular only), and the selection.
#[derive(Debug, Clone)]
pub(crate) struct LayoutView {
    placement: LayerPlacement,
    /// Free: the boxes back to front. Regular: one per cell, in stored
    /// coordinates (the title band above the origin), for the overlay and the
    /// accessors only.
    boxes: Vec<GraphBox>,
    page: Option<FreePage>,
    /// Regular: the figure as a page (stored coordinates, no origin), whose L
    /// corners scale the whole figure.
    figure: Option<FreePage>,
    grid: Option<GridView>,
    /// Selected layers as base layers, the active one first.
    selected: Vec<usize>,
}

/// What a drag holds, frozen at the press.
#[derive(Debug, Clone)]
enum DragKind {
    Free {
        handle: LayoutHandle,
        boxes: Vec<GraphBox>,
        page: FreePage,
        /// The graphs a both-sides drag moves.
        boundary: Option<BoundaryGroup>,
    },
    Grid {
        handle: GridHandle,
        widths: Vec<f64>,
        heights: Vec<f64>,
        /// Position of the grabbed vertical and horizontal lines at the press,
        /// in figure pixels (title band excluded for the row), for the grid.
        line: (f64, f64),
    },
    /// An L corner of a regular grid: the whole figure scales.
    Figure {
        corner: (bool, bool),
        widths: Vec<f64>,
        heights: Vec<f64>,
        /// Extent of the figure that is not plot area: `(across, down)`.
        bands: (f64, f64),
        /// The image `(width, height)` at the press, for the ratio lock.
        image: (f64, f64),
    },
}

/// A box, boundary or page-corner drag in progress. Everything is frozen at
/// the press: the display scale (the figure may grow under the pointer), the
/// placement and the geometry.
#[derive(Debug, Clone)]
pub(crate) struct LayoutDrag {
    kind: DragKind,
    press: Pos2,
    scale: f32,
    /// The placement the figure was drawn with at the press.
    placement: LayerPlacement,
    pub(crate) guides: Vec<Guide>,
}

/// A burst of arrow-key nudges not yet committed.
#[derive(Debug, Clone)]
struct Nudge {
    start: Vec<GraphBox>,
    page: FreePage,
    total: (f64, f64),
    last: f64,
    placement: LayerPlacement,
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
    /// Height / width the page corners keep while the shell's ratio lock is
    /// closed (fed by the shell, never stored).
    ratio_lock: Option<f64>,
    /// Bases of the graphs under the pointer, to show their handles faintly.
    hovered: Vec<usize>,
}

impl LayoutState {
    /// Drops the gesture in progress (the layers were renumbered).
    pub(crate) fn forget_gestures(&mut self) {
        self.drag = None;
        self.nudge = None;
        self.view = None;
        self.hovered.clear();
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

/// Whether two vectors of sizes differ by half a point or more somewhere
/// (the grid preview is rounded to whole points).
fn sizes_differ(a: &[f64], b: &[f64]) -> bool {
    a.len() != b.len() || a.iter().zip(b).any(|(x, y)| (x - y).abs() >= 0.5)
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
    // The theme selection may be translucent and pale (light green): take it
    // opaque and darken it until white text on it stays legible.
    let [r, g, b, _] = darker.to_srgba_unmultiplied();
    let mut opaque = Color32::from_rgb(r, g, b);
    while luma(opaque) > 900 {
        let shade = |v: u8| (u16::from(v) * 80 / 100) as u8;
        opaque = Color32::from_rgb(shade(opaque.r()), shade(opaque.g()), shade(opaque.b()));
    }
    opaque
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

/// How the handles of a box are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HandleLook {
    /// No handles.
    None,
    /// Faint: the box under the pointer, already grabbable.
    Faint,
    /// Solid: a selected box.
    Solid,
}

/// The handles of a box: solid when selected, faint when it is under the
/// pointer and no drag is in progress, else none.
pub(crate) fn handle_look(selected: bool, hovered: bool, dragging: bool) -> HandleLook {
    if selected {
        HandleLook::Solid
    } else if hovered && !dragging {
        HandleLook::Faint
    } else {
        HandleLook::None
    }
}

/// The graph a click or a drag on `handle` selects: the box itself, or the one
/// on the pointer's side of a line.
fn handle_target(handle: &LayoutHandle) -> Option<usize> {
    match handle {
        LayoutHandle::Box { base, .. } | LayoutHandle::Side { base, .. } => Some(*base),
        LayoutHandle::Boundary { near, .. } => Some(*near),
        LayoutHandle::Page { .. } => None,
    }
}

/// The graphs a handle shows faintly under the pointer.
fn handle_graphs(handle: &LayoutHandle) -> Vec<usize> {
    match handle {
        LayoutHandle::Box { base, .. } | LayoutHandle::Side { base, .. } => vec![*base],
        LayoutHandle::Boundary { low, high, .. } => vec![*low, *high],
        LayoutHandle::Page { .. } => Vec::new(),
    }
}

fn handle_cursor(handle: &LayoutHandle) -> CursorIcon {
    match handle {
        LayoutHandle::Boundary { vertical, .. } => {
            if *vertical {
                CursorIcon::ResizeHorizontal
            } else {
                CursorIcon::ResizeVertical
            }
        }
        LayoutHandle::Side { edges, .. } => {
            if edges.right {
                CursorIcon::ResizeEast
            } else if edges.left {
                CursorIcon::ResizeWest
            } else if edges.bottom {
                CursorIcon::ResizeSouth
            } else {
                CursorIcon::ResizeNorth
            }
        }
        LayoutHandle::Page { right, bottom } => resize_cursor(true, true, right == bottom),
        LayoutHandle::Box { edges, .. } => resize_cursor(
            edges.left || edges.right,
            edges.top || edges.bottom,
            edges.left == edges.top,
        ),
    }
}

/// The cursor of a grid handle: one axis for one line, diagonal for a corner on
/// two.
fn grid_cursor(handle: &GridHandle) -> CursorIcon {
    match (handle.column.is_some(), handle.row.is_some()) {
        (true, true) => CursorIcon::ResizeNwSe,
        (true, false) => CursorIcon::ResizeHorizontal,
        (false, true) => CursorIcon::ResizeVertical,
        (false, false) => CursorIcon::Default,
    }
}

/// The regular grid of `layout` for `spec`: the cells, the corners and whether
/// the layout has frames to edit. `None` unless the figure was drawn on a
/// grid.
fn grid_view(spec: &PlotSpec, layout: &FigureLayout) -> Option<(GridView, Vec<GraphBox>)> {
    if layout.placement != LayerPlacement::Grid {
        return None;
    }
    let columns = layout.column_breaks.len().checked_sub(1)?;
    let rows = layout.row_breaks.len().checked_sub(1)?;
    let bases = spec.cell_bases();
    let mut cells: Vec<GridCell> = Vec::new();
    for (index, panel) in spec.panels.iter().enumerate() {
        let (row, column) = (panel.row, panel.column);
        if row < rows
            && column < columns
            && !cells.iter().any(|c| (c.row, c.column) == (row, column))
        {
            if let Some(&base) = bases.get(index) {
                cells.push(GridCell { base, row, column });
            }
        }
    }
    let editable = columns > 0
        && rows > 0
        && layout.frame_widths.len() == columns
        && layout.frame_heights.len() == rows;
    let band = f64::from(layout.title_band);
    let boxes = cells
        .iter()
        .filter_map(|cell| {
            let (x, y, w, h) = layout.cell_rect(cell.row, cell.column)?;
            let outer = FreePage {
                left: f64::from(x),
                top: f64::from(y) - band,
                width: f64::from(w),
                height: f64::from(h),
            };
            let frame = match layout.frame_rect(cell.row, cell.column) {
                Some((fx, fy, fw, fh)) => FreeFrame {
                    left: f64::from(fx),
                    top: f64::from(fy) - band,
                    width: f64::from(fw),
                    height: f64::from(fh),
                    stack: 0,
                },
                None => FreeFrame {
                    left: outer.left,
                    top: outer.top,
                    width: outer.width,
                    height: outer.height,
                    stack: 0,
                },
            };
            Some(GraphBox {
                base: cell.base,
                outer,
                gutters: (
                    frame.left - outer.left,
                    frame.top - outer.top,
                    outer.left + outer.width - (frame.left + frame.width),
                    outer.top + outer.height - (frame.top + frame.height),
                ),
                frame,
            })
        })
        .collect();
    let corners = if editable {
        let occupied: Vec<(usize, usize)> = cells.iter().map(|c| (c.row, c.column)).collect();
        grid_corners(layout, &occupied)
    } else {
        Vec::new()
    };
    Some((
        GridView {
            columns,
            rows,
            cells,
            corners,
            band,
            editable,
        },
        boxes,
    ))
}

impl GraphView {
    /// Turns the Page Layout Mode on or off (#426). It is a view state: nothing
    /// is sent and nothing in the model changes, whatever the placement.
    /// Leaving it commits a pending nudge burst and drops the gesture in
    /// progress.
    pub fn set_layout_mode(&mut self, on: bool) {
        if self.layout.mode == on {
            return;
        }
        if on {
            self.nav_drag = None;
            self.drag_box = None;
            self.annotation_drag = None;
            self.text_drag = None;
            self.bar_drag = None;
            self.shape_drag = None;
            self.layout.mode = true;
            return;
        }
        self.commit_nudge();
        if self.layout.drag.take().is_some() {
            self.set_frame_preview(None);
        }
        self.layout.mode = false;
        self.layout.view = None;
        self.layout.hovered.clear();
    }

    /// Whether the Page Layout Mode is on.
    pub fn layout_mode(&self) -> bool {
        self.layout.mode
    }

    /// Commits the pending arrow-key burst now, if there is one: the shell
    /// calls it before it sends anything that changes the placement or the
    /// layers, so that the edit reaches the model first.
    pub fn commit_layout_edit(&mut self) {
        self.commit_nudge();
    }

    /// Sets the snapping grid step in points (`None`: no grid).
    pub fn set_layout_grid(&mut self, step: Option<f64>) {
        self.layout.grid = step.filter(|step| step.is_finite() && *step > 0.0);
    }

    /// The snapping grid step in points, `None` when the grid is off.
    pub fn layout_grid(&self) -> Option<f64> {
        self.layout.grid
    }

    /// Closes the ratio lock of the figure's corners with `ratio` = height /
    /// width of the image, or opens it (`None`). Fed by the shell each frame.
    pub fn set_figure_ratio_lock(&mut self, ratio: Option<f64>) {
        self.layout.ratio_lock = ratio.filter(|ratio| ratio.is_finite() && *ratio > 0.0);
    }

    /// Whether a box, boundary or corner drag is in progress.
    pub fn layout_dragging(&self) -> bool {
        self.layout.drag.is_some()
    }

    /// The unit the sizes written on the boxes are shown in.
    pub fn set_length_unit(&mut self, unit: LengthUnit) {
        self.layout.unit = unit;
    }

    /// The placement the figure is drawn with, read in the mode after a paint.
    pub fn layout_placement(&self) -> Option<LayerPlacement> {
        self.layout.view.as_ref().map(|view| view.placement)
    }

    /// The page, in stored page coordinates (mode on, free layout, after a
    /// paint); with the preview of the gesture in progress.
    pub fn layout_page(&self) -> Option<FreePage> {
        self.layout.view.as_ref().and_then(|view| view.page)
    }

    /// The whole image in stored page coordinates, title band included: the
    /// page of a free figure, the figure of a regular grid (whose origin is the
    /// top of the first row, so the image starts one title band above it).
    /// `None` outside the mode or before the first paint.
    pub fn layout_image(&self) -> Option<FreePage> {
        let view = self.layout.view.as_ref()?;
        if let Some(page) = view.page {
            return Some(page);
        }
        let figure = view.figure?;
        let band = view.grid.as_ref().map_or(0.0, |grid| grid.band);
        Some(FreePage {
            left: figure.left,
            top: figure.top - band,
            width: figure.width,
            height: figure.height + band,
        })
    }

    /// The graph boxes back to front, with the preview of the gesture in
    /// progress applied (empty outside the mode). On a regular grid, one per
    /// cell in stored coordinates (the title band above the origin).
    pub fn layout_boxes(&self) -> Vec<GraphBox> {
        self.layout
            .view
            .as_ref()
            .map_or_else(Vec::new, |view| view.boxes.clone())
    }

    /// The graph corners of a regular grid that carry a handle, in figure
    /// pixels (empty outside the mode, on a free figure, or when the grid has
    /// no frames to edit).
    pub fn layout_grid_corners(&self) -> Vec<GridCorner> {
        self.layout
            .view
            .as_ref()
            .and_then(|view| view.grid.as_ref())
            .map_or_else(Vec::new, |grid| grid.corners.clone())
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

    /// Moves the selected layers of a free figure by `steps` grid steps (the
    /// default step of the unit when the grid is off), held inside the page,
    /// previewed. The burst is committed as **one** [`GraphEvent::LayoutEdited`]
    /// after 0.7 s without a nudge (`now` and the frame clock are the same
    /// clock, `egui::InputState::time`), when the mode is left, when a drag
    /// starts or on [`GraphView::commit_layout_edit`]. A regular grid is never
    /// nudged.
    pub fn nudge(&mut self, steps: (i32, i32), now: f64) {
        if !self.layout.mode || steps == (0, 0) || self.layout.drag.is_some() {
            return;
        }
        let Some(view) = self.layout.view.clone() else {
            return;
        };
        let Some(page) = view.page.filter(|_| view.placement == LayerPlacement::Free) else {
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
                page,
                total: (0.0, 0.0),
                last: now,
                placement: view.placement,
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

    /// Drops the gesture in progress if the figure is no longer drawn with the
    /// placement it began with (an Undo, a toggle): its geometry no longer
    /// means anything. Called once per paint, before the view is computed.
    pub(crate) fn reconcile_layout_mode(&mut self, drawn: LayerPlacement) {
        let stale_drag = self
            .layout
            .drag
            .as_ref()
            .is_some_and(|drag| drag.placement != drawn);
        let stale_nudge = self
            .layout
            .nudge
            .as_ref()
            .is_some_and(|burst| burst.placement != drawn);
        if stale_drag || stale_nudge {
            self.layout.drag = None;
            self.layout.nudge = None;
            self.layout.hovered.clear();
            self.set_frame_preview(None);
        }
    }

    /// The selected layers as base layers, the active one first.
    fn selected_bases(&self, spec: &PlotSpec) -> Vec<usize> {
        let bases = spec.cell_bases();
        let mut selected = Vec::new();
        if let Some(active) = bases.get(self.active_panel(spec)) {
            selected.push(*active);
        }
        selected.extend(self.extra_layers(spec));
        selected
    }

    /// What the overlay and the accessors read, computed once per paint from
    /// the figure as drawn (the preview included). `None` before the first
    /// layout.
    pub(crate) fn compute_layout_view(&self, spec: &PlotSpec) -> Option<LayoutView> {
        let (_, layout) = self.figure_memo.as_ref()?;
        let selected = self.selected_bases(spec);
        match layout.placement {
            LayerPlacement::Free => {
                let page = layout.free_page_rect()?;
                Some(LayoutView {
                    placement: LayerPlacement::Free,
                    boxes: self.free_boxes(spec, layout),
                    page: Some(page),
                    figure: None,
                    grid: None,
                    selected,
                })
            }
            LayerPlacement::Grid => {
                let (grid, boxes) = grid_view(spec, layout)?;
                Some(LayoutView {
                    placement: LayerPlacement::Grid,
                    boxes,
                    page: None,
                    figure: Some(FreePage {
                        left: 0.0,
                        top: 0.0,
                        width: f64::from(layout.size.0),
                        height: f64::from(layout.size.1.saturating_sub(layout.title_band)),
                    }),
                    grid: Some(grid),
                    selected,
                })
            }
        }
    }

    /// The boxes of a free layout, with the preview applied.
    fn free_boxes(&self, spec: &PlotSpec, layout: &FigureLayout) -> Vec<GraphBox> {
        match &self.frame_preview {
            Some(preview) => {
                let mut previewed = spec.clone();
                preview.apply(&mut previewed);
                graph_boxes(&previewed, layout)
            }
            None => graph_boxes(spec, layout),
        }
    }

    /// The reach of the handles at the display scale.
    fn layout_reach(&self) -> Option<LayoutReach> {
        let scale = f64::from(self.display_scale().filter(|scale| *scale > 0.0)?);
        Some(LayoutReach::standard(scale))
    }

    /// The handle of a free figure under the screen position `pos`.
    fn free_handle_under(
        &self,
        pos: Pos2,
        boxes: &[GraphBox],
        page: FreePage,
        spec: &PlotSpec,
    ) -> Option<LayoutHandle> {
        let (_, layout) = self.figure_memo.as_ref()?;
        let reach = self.layout_reach()?;
        let figure = self.figure_position(pos)?;
        let (x, y) = figure_to_stored(layout, f64::from(figure.x), f64::from(figure.y));
        let selected = self.selected_bases(spec);
        layout_handle_at(boxes, &selected, page, x, y, reach)
    }

    /// The L corner of a regular grid's figure under the screen position
    /// `pos`: `(right, bottom)`. The zones sit outside the figure.
    fn figure_corner_under(&self, pos: Pos2, view: &LayoutView) -> Option<(bool, bool)> {
        let figure = view.figure?;
        // A visible handle must do something: a grid with no frames to edit
        // (compact fallback) has none.
        if view.grid.as_ref().is_some_and(|grid| !grid.editable) {
            return None;
        }
        let (_, layout) = self.figure_memo.as_ref()?;
        let reach = self.layout_reach()?;
        let at = self.figure_position(pos)?;
        let (x, y) = figure_to_stored(layout, f64::from(at.x), f64::from(at.y));
        let zone = 2.0 * reach.edge_points();
        for right in [false, true] {
            for bottom in [false, true] {
                let cx = figure.left + if right { figure.width } else { 0.0 };
                let cy = figure.top + if bottom { figure.height } else { 0.0 };
                let in_x = if right {
                    x > cx && x <= cx + zone
                } else {
                    x < cx && x >= cx - zone
                };
                let in_y = if bottom {
                    y > cy && y <= cy + zone
                } else {
                    y < cy && y >= cy - zone
                };
                if in_x && in_y {
                    return Some((right, bottom));
                }
            }
        }
        None
    }

    /// The handle of a regular grid under the screen position `pos`.
    fn grid_handle_under(&self, pos: Pos2, grid: &GridView) -> Option<GridHandle> {
        if !grid.editable {
            return None;
        }
        let (_, layout) = self.figure_memo.as_ref()?;
        let reach = self.layout_reach()?;
        let figure = self.figure_position(pos)?;
        let occupied: Vec<(usize, usize)> = grid.cells.iter().map(|c| (c.row, c.column)).collect();
        grid_handle_at(
            layout,
            &occupied,
            f64::from(figure.x),
            f64::from(figure.y),
            reach,
        )
    }

    /// The base of the cell of a regular grid under the screen position `pos`.
    fn grid_cell_under(&self, pos: Pos2, grid: &GridView) -> Option<usize> {
        let (_, layout) = self.figure_memo.as_ref()?;
        let figure = self.figure_position(pos)?;
        grid.cells
            .iter()
            .find(|cell| {
                layout
                    .cell_rect(cell.row, cell.column)
                    .is_some_and(|(x, y, w, h)| {
                        figure.x >= x as f32
                            && figure.x < (x + w) as f32
                            && figure.y >= y as f32
                            && figure.y < (y + h) as f32
                    })
            })
            .map(|cell| cell.base)
    }

    /// The cells whose band a grid handle is on, shown faintly.
    fn grid_handle_cells(&self, handle: &GridHandle, grid: &GridView) -> Vec<usize> {
        grid.cells
            .iter()
            .filter(|cell| {
                handle
                    .column
                    .is_some_and(|n| cell.column == n || cell.column == n + 1)
                    || handle
                        .row
                        .is_some_and(|n| cell.row == n || cell.row == n + 1)
            })
            .map(|cell| cell.base)
            .collect()
    }

    /// The gestures of the mode: selection by click, drags of the handles of
    /// the placement the figure was drawn with, cursors. Called instead of
    /// every other gesture of the figure.
    pub(crate) fn handle_layout_mode(
        &mut self,
        response: &Response,
        context: &Context,
        spec: &PlotSpec,
    ) {
        self.tick_nudge(context);
        self.layout.hovered.clear();
        let Some(view) = self.layout.view.clone() else {
            return;
        };
        if response.drag_stopped_by(PointerButton::Primary) {
            self.finish_layout_drag();
            return;
        }
        let pointer = response.interact_pointer_pos();
        if response.clicked() || response.secondary_clicked() {
            let shift = context.input(|input| input.modifiers.shift);
            let target = pointer.and_then(|pos| self.click_target(pos, &view, spec));
            if let Some(base) = target {
                if response.clicked() && shift && view.placement == LayerPlacement::Free {
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
            self.layout_drag(response, context, spec, &view);
        }
        let cursor = match &self.layout.drag {
            Some(drag) => Some(match &drag.kind {
                DragKind::Free { handle, .. } => handle_cursor(handle),
                DragKind::Grid { handle, .. } => grid_cursor(handle),
                DragKind::Figure { corner, .. } => resize_cursor(true, true, corner.0 == corner.1),
            }),
            None if response.hovered() => self.hover_cursor(response.hover_pos(), &view, spec),
            None => None,
        };
        if let Some(icon) = cursor {
            context.set_cursor_icon(icon);
        }
    }

    /// The graph a click on `pos` selects.
    fn click_target(&self, pos: Pos2, view: &LayoutView, spec: &PlotSpec) -> Option<usize> {
        match (&view.grid, view.page) {
            (Some(grid), _) => self.grid_cell_under(pos, grid),
            (None, Some(page)) => self
                .free_handle_under(pos, &view.boxes, page, spec)
                .as_ref()
                .and_then(handle_target),
            (None, None) => None,
        }
    }

    /// The cursor under the pointer, and the graphs to show faintly.
    fn hover_cursor(
        &mut self,
        hover: Option<Pos2>,
        view: &LayoutView,
        spec: &PlotSpec,
    ) -> Option<CursorIcon> {
        let pos = hover?;
        if let Some(corner) = self.figure_corner_under(pos, view) {
            return Some(resize_cursor(true, true, corner.0 == corner.1));
        }
        if let Some(grid) = &view.grid {
            let handle = self.grid_handle_under(pos, grid);
            self.layout.hovered = match &handle {
                Some(handle) => self.grid_handle_cells(handle, grid),
                None => self.grid_cell_under(pos, grid).into_iter().collect(),
            };
            return handle.map(|handle| grid_cursor(&handle));
        }
        let page = view.page?;
        let handle = self.free_handle_under(pos, &view.boxes, page, spec)?;
        self.layout.hovered = handle_graphs(&handle);
        Some(handle_cursor(&handle))
    }

    /// Starts and follows a drag of a handle; previews it.
    fn layout_drag(
        &mut self,
        response: &Response,
        context: &Context,
        spec: &PlotSpec,
        view: &LayoutView,
    ) {
        let Some(pos) = response.interact_pointer_pos() else {
            return;
        };
        if self.layout.drag.is_none() && !self.begin_layout_drag(response, context, spec, view, pos)
        {
            return;
        }
        let Some(drag) = self.layout.drag.clone() else {
            return;
        };
        let delta = (pos - drag.press) / drag.scale;
        let delta = (f64::from(delta.x), f64::from(delta.y));
        let shift = context.input(|input| input.modifiers.shift);
        match &drag.kind {
            DragKind::Free {
                handle,
                boxes,
                page,
                boundary,
            } => {
                let snap = Snap {
                    grid: self.layout.grid,
                    threshold: MAGNET_THRESHOLD_POINTS / f64::from(drag.scale),
                    enabled: !shift,
                };
                let delta = match (handle, self.layout.ratio_lock) {
                    (LayoutHandle::Page { right, bottom }, Some(ratio)) => locked_corner_delta(
                        (page.width, page.height),
                        (*right, *bottom),
                        delta,
                        ratio,
                    ),
                    _ => delta,
                };
                self.drag_free(handle, boxes, *page, boundary.as_ref(), delta, snap);
            }
            DragKind::Figure {
                corner,
                widths,
                heights,
                bands,
                image,
            } => {
                let delta = match self.layout.ratio_lock {
                    Some(ratio) => locked_corner_delta(*image, *corner, delta, ratio),
                    None => delta,
                };
                let (new_widths, new_heights) =
                    scaled_frames(widths, heights, *bands, *corner, delta);
                self.set_frame_preview(Some(FramePreview {
                    grid: Some((new_widths, new_heights)),
                    ..FramePreview::default()
                }));
            }
            DragKind::Grid {
                handle,
                widths,
                heights,
                line,
            } => {
                let grid = self.layout.grid.filter(|_| !shift);
                let snapped = |start: f64, moved: f64| match grid {
                    Some(step) => ((start + moved) / step).round() * step - start,
                    None => moved,
                };
                let delta = (snapped(line.0, delta.0), snapped(line.1, delta.1));
                let (new_widths, new_heights) = resized_frames(handle, widths, heights, delta);
                self.set_frame_preview(Some(FramePreview {
                    grid: Some((new_widths, new_heights)),
                    ..FramePreview::default()
                }));
            }
        }
    }

    /// Freezes the handle under the press; `false` if there is none.
    fn begin_layout_drag(
        &mut self,
        response: &Response,
        context: &Context,
        spec: &PlotSpec,
        view: &LayoutView,
        pos: Pos2,
    ) -> bool {
        let origin = context
            .input(|input| input.pointer.press_origin())
            .unwrap_or(pos - response.drag_delta());
        let Some(scale) = self.display_scale().filter(|scale| *scale > 0.0) else {
            return false;
        };
        let kind = match (&view.grid, view.page) {
            (Some(_), _) if self.figure_corner_under(origin, view).is_some() => {
                let Some(corner) = self.figure_corner_under(origin, view) else {
                    return false;
                };
                let Some((_, layout)) = self.figure_memo.as_ref() else {
                    return false;
                };
                let sum = |sizes: &[f64]| sizes.iter().sum::<f64>();
                let extent = (
                    f64::from(layout.size.0),
                    f64::from(layout.size.1.saturating_sub(layout.title_band)),
                );
                DragKind::Figure {
                    corner,
                    widths: layout.frame_widths.clone(),
                    heights: layout.frame_heights.clone(),
                    bands: (
                        extent.0 - sum(&layout.frame_widths),
                        extent.1 - sum(&layout.frame_heights),
                    ),
                    image: (f64::from(layout.size.0), f64::from(layout.size.1)),
                }
            }
            (Some(grid), _) => {
                let Some(handle) = self.grid_handle_under(origin, grid) else {
                    return false;
                };
                let Some((_, layout)) = self.figure_memo.as_ref() else {
                    return false;
                };
                let line = (
                    handle
                        .column
                        .and_then(|n| layout.column_breaks.get(n + 1))
                        .map_or(0.0, |b| f64::from(*b)),
                    handle
                        .row
                        .and_then(|n| layout.row_breaks.get(n + 1))
                        .map_or(0.0, |b| f64::from(*b)),
                );
                DragKind::Grid {
                    handle,
                    widths: layout.frame_widths.clone(),
                    heights: layout.frame_heights.clone(),
                    line,
                }
            }
            (None, Some(page)) => {
                let Some(handle) = self.free_handle_under(origin, &view.boxes, page, spec) else {
                    return false;
                };
                let boundary = match handle {
                    LayoutHandle::Boundary {
                        vertical,
                        low,
                        high,
                        ..
                    } => Some(boundary_group(&view.boxes, vertical, low, high)),
                    _ => None,
                };
                if let Some(base) = handle_target(&handle) {
                    let selected = match handle {
                        LayoutHandle::Boundary { low, high, .. } => {
                            self.layer_in_selection(low, spec)
                                || self.layer_in_selection(high, spec)
                        }
                        _ => self.layer_in_selection(base, spec),
                    };
                    if !selected {
                        self.set_active_panel(base, spec);
                        self.clear_extra_layers();
                    }
                }
                DragKind::Free {
                    handle,
                    boxes: view.boxes.clone(),
                    page,
                    boundary,
                }
            }
            (None, None) => return false,
        };
        self.commit_nudge();
        self.layout.drag = Some(LayoutDrag {
            kind,
            press: origin,
            scale,
            placement: view.placement,
            guides: Vec::new(),
        });
        true
    }

    /// The preview of a drag on a free figure.
    fn drag_free(
        &mut self,
        handle: &LayoutHandle,
        boxes: &[GraphBox],
        page: FreePage,
        boundary: Option<&BoundaryGroup>,
        delta: (f64, f64),
        snap: Snap,
    ) {
        let (frees, shown_page, guides) = match *handle {
            LayoutHandle::Box { base, edges } | LayoutHandle::Side { base, edges } => {
                let Some(start) = boxes.iter().find(|graph| graph.base == base) else {
                    return;
                };
                let others: Vec<FreePage> = boxes
                    .iter()
                    .filter(|graph| graph.base != base)
                    .map(|graph| graph.outer)
                    .collect();
                let (frame, guides) = moved_box(start, &edges, delta, page, &others, snap);
                (vec![(base, frame)], page, guides)
            }
            LayoutHandle::Boundary { vertical, .. } => {
                let Some(group) = boundary else {
                    return;
                };
                let across = if vertical { delta.0 } else { delta.1 };
                let (frames, guides) = moved_boundary(boxes, group, across, page, snap);
                (frames, page, guides)
            }
            LayoutHandle::Page { right, bottom } => {
                let new_page = resized_page(boxes, page, (right, bottom), delta, snap);
                (scaled_to_page(boxes, page, new_page), new_page, Vec::new())
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

    /// The release: one event if something moved.
    fn finish_layout_drag(&mut self) {
        let Some(drag) = self.layout.drag.take() else {
            return;
        };
        let preview = self.frame_preview.clone();
        let grid_event = |widths: &[f64], heights: &[f64], figure: bool| match &preview {
            Some(FramePreview {
                grid: Some((new_widths, new_heights)),
                ..
            }) if sizes_differ(widths, new_widths) || sizes_differ(heights, new_heights) => {
                Some(GraphEvent::GridFramesEdited {
                    widths: new_widths.clone(),
                    heights: new_heights.clone(),
                    figure,
                })
            }
            _ => None,
        };
        let event = match (&drag.kind, preview.clone()) {
            (
                DragKind::Grid {
                    widths, heights, ..
                },
                _,
            ) => grid_event(widths, heights, false),
            (
                DragKind::Figure {
                    widths, heights, ..
                },
                _,
            ) => grid_event(widths, heights, true),
            (DragKind::Free { boxes, page, .. }, Some(preview))
                if preview.page.is_some_and(|shown| page_differs(shown, *page))
                    || preview.frees.iter().any(|(base, frame)| {
                        boxes
                            .iter()
                            .find(|graph| graph.base == *base)
                            .is_none_or(|graph| differs(graph.frame, *frame))
                    }) =>
            {
                Some(GraphEvent::LayoutEdited {
                    frames: preview.frees,
                    page: preview.page,
                })
            }
            _ => None,
        };
        match event {
            Some(event) => {
                self.events.push(event);
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
        let scale = self.display_scale().unwrap_or(1.0);
        if let Some(page) = view.page.or(view.figure) {
            let corner = |x: f64, y: f64| self.stored_to_screen(x, y);
            if let (Some(a), Some(b)) = (
                corner(page.left, page.top),
                corner(page.left + page.width, page.top + page.height),
            ) {
                let page_rect = Rect::from_two_pos(a, b);
                self.paint_grid(painter, page, page_rect, scale);
                painter.rect_stroke(
                    page_rect,
                    0.0,
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(110, 110, 125, 170)),
                    StrokeKind::Outside,
                );
                let dragged_corner = match self.layout.drag.as_ref().map(|drag| &drag.kind) {
                    Some(DragKind::Free {
                        handle: LayoutHandle::Page { right, bottom },
                        ..
                    }) => Some((*right, *bottom)),
                    Some(DragKind::Figure { corner, .. }) => Some(*corner),
                    _ => None,
                };
                // A grid with no frames to edit has no page corners: a visible
                // handle must do something.
                let has_corners = view.grid.as_ref().is_none_or(|grid| grid.editable);
                if has_corners {
                    paint_page_corners(painter, page_rect, dragged_corner, accent);
                }
            }
        }

        let in_selection = |graph: &GraphBox| view.selected.contains(&graph.base);
        let dragging = self.layout.drag.is_some();
        for selected in [false, true] {
            for graph in view.boxes.iter().filter(|g| in_selection(g) == selected) {
                let hovered = self.layout.hovered.contains(&graph.base);
                let look = handle_look(selected, hovered, dragging);
                self.paint_box(painter, view, graph, selected, look, accent);
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
    fn paint_grid(&self, painter: &Painter, page: FreePage, page_rect: Rect, scale: f32) {
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
        while at <= page.width {
            if let Some(top) = self.stored_to_screen(page.left + at, page.top) {
                painter.line_segment([top, Pos2::new(top.x, page_rect.bottom())], stroke);
            }
            at += step;
        }
        let mut at = 0.0;
        while at <= page.height {
            if let Some(left) = self.stored_to_screen(page.left, page.top + at) {
                painter.line_segment([left, Pos2::new(page_rect.right(), left.y)], stroke);
            }
            at += step;
        }
    }

    /// One graph box: outline, dashed plot area, size label, and the handles
    /// of its placement as `look` says.
    fn paint_box(
        &self,
        painter: &Painter,
        view: &LayoutView,
        graph: &GraphBox,
        selected: bool,
        look: HandleLook,
        accent: Color32,
    ) {
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
        let cx = outer.center().x;
        if look != HandleLook::None {
            let (squares, pills) = self.handle_marks(view, graph, outer);
            let (fill, line) = if look == HandleLook::Faint {
                (
                    Color32::WHITE.gamma_multiply(0.45),
                    colour.gamma_multiply(0.45),
                )
            } else {
                (Color32::WHITE, colour)
            };
            let outline = Stroke::new(1.0, line);
            for at in squares {
                let square = Rect::from_center_size(at, egui::vec2(HANDLE_SIZE, HANDLE_SIZE));
                painter.rect(square, 0.0, fill, outline, StrokeKind::Middle);
            }
            for (at, size) in pills {
                let edge_pill = Rect::from_center_size(at, size);
                painter.rect(edge_pill, 2.0, fill, outline, StrokeKind::Middle);
            }
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

    /// The handle marks of a box on screen: the square handles (centres) and
    /// the pills (centre and size). Free: eight handles. Regular: the pills of
    /// its internal boundaries and the squares of its corners that carry a
    /// handle.
    fn handle_marks(
        &self,
        view: &LayoutView,
        graph: &GraphBox,
        outer: Rect,
    ) -> (Vec<Pos2>, Vec<(Pos2, egui::Vec2)>) {
        let (cx, cy) = (outer.center().x, outer.center().y);
        let top = (Pos2::new(cx, outer.top()), egui::vec2(18.0, 4.0));
        let right = (Pos2::new(outer.right(), cy), egui::vec2(4.0, 18.0));
        let bottom = (Pos2::new(cx, outer.bottom()), egui::vec2(18.0, 4.0));
        let left = (Pos2::new(outer.left(), cy), egui::vec2(4.0, 18.0));
        let corners = [
            (outer.left_top(), graph.outer.left, graph.outer.top),
            (
                outer.right_top(),
                graph.outer.left + graph.outer.width,
                graph.outer.top,
            ),
            (
                outer.right_bottom(),
                graph.outer.left + graph.outer.width,
                graph.outer.top + graph.outer.height,
            ),
            (
                outer.left_bottom(),
                graph.outer.left,
                graph.outer.top + graph.outer.height,
            ),
        ];
        let Some(grid) = &view.grid else {
            return (
                corners.iter().map(|(at, _, _)| *at).collect(),
                vec![top, right, bottom, left],
            );
        };
        let Some(cell) = grid.cells.iter().find(|c| c.base == graph.base) else {
            return (Vec::new(), Vec::new());
        };
        if !grid.editable {
            return (Vec::new(), Vec::new());
        }
        let mut pills = Vec::new();
        if cell.row > 0 {
            pills.push(top);
        }
        if cell.column + 1 < grid.columns {
            pills.push(right);
        }
        if cell.row + 1 < grid.rows {
            pills.push(bottom);
        }
        if cell.column > 0 {
            pills.push(left);
        }
        let squares = corners
            .iter()
            .filter(|(_, x, y)| {
                grid.corners
                    .iter()
                    .any(|c| (c.x - x).abs() < 0.5 && (c.y - (y + grid.band)).abs() < 0.5)
            })
            .map(|(at, _, _)| *at)
            .collect();
        (squares, pills)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Selected graphs are solid, the hovered one faint, and no faint handle
    /// shows during a drag. Witness: the drag condition dropped.
    #[test]
    fn handles_are_solid_when_selected_and_faint_when_hovered_outside_a_drag() {
        assert_eq!(handle_look(true, false, false), HandleLook::Solid);
        assert_eq!(handle_look(true, true, true), HandleLook::Solid);
        assert_eq!(handle_look(false, true, false), HandleLook::Faint);
        assert_eq!(handle_look(false, true, true), HandleLook::None);
        assert_eq!(handle_look(false, false, false), HandleLook::None);
    }

    /// A corner on two lines gets the diagonal cursor, one line its own axis.
    #[test]
    fn grid_cursors_follow_the_lines_a_handle_holds() {
        let at = |column, row| GridHandle {
            column,
            row,
            corner: true,
        };
        assert_eq!(grid_cursor(&at(Some(0), Some(0))), CursorIcon::ResizeNwSe);
        assert_eq!(
            grid_cursor(&at(Some(0), None)),
            CursorIcon::ResizeHorizontal
        );
        assert_eq!(grid_cursor(&at(None, Some(1))), CursorIcon::ResizeVertical);
    }
}
