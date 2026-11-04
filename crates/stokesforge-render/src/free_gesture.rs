//! The geometry of the mouse gestures of free layer placement (#398), without
//! egui: which handle of a layer is under a pixel, where a moved or resized
//! frame ends up, and the magnets that snap its edges to the edges of the other
//! layers.
//!
//! Positions and sizes are **stored page coordinates, in points** (the numbers
//! of [`FreeFrame`]); only [`free_handle_at`] works in figure pixels, on a
//! [`FigureLayout`]. Plain data in, plain data out: the screen layer
//! (`stokesforge-render-egui`) converts screen points to points with the
//! display scale it froze at the press.

use crate::hit::CELL_EDGE_MAX_TOLERANCE;
use crate::pipeline::{FigureLayout, FreeCellRect};
use stokesforge_plot::{
    FreeFrame, FreePage, LayerPlacement, PlotSpec, MAX_FIGURE_EXTENT, MAX_FRAME_SIZE,
    MIN_FRAME_SIZE,
};

/// How close, **in screen points**, an edge of a dragged frame must come to an
/// edge of another layer to snap to it. Converted into points by the caller
/// (`6 / display_scale`), so the magnet keeps the same reach on screen
/// whatever the enlargement of the figure.
pub const MAGNET_THRESHOLD_POINTS: f64 = 6.0;

/// Page Layout Mode, edge shared by two touching graphs: within this many
/// **screen points** of the line, a drag moves both graphs (#426d).
pub const BOUNDARY_BOTH_POINTS: f64 = 2.0;

/// Page Layout Mode, edge shared by two touching graphs: within this many
/// **screen points** of the line (and beyond [`BOUNDARY_BOTH_POINTS`]), a drag
/// moves only the graph on the pointer's side.
pub const BOUNDARY_SIDE_POINTS: f64 = 10.0;

/// Two outer edges count as one shared edge when they are at most this many
/// **stored points** apart (and the same for the overlap needed along them).
pub const TOUCH_TOLERANCE_POINTS: f64 = 1.0;

/// What a pixel grabs on a layer of a free figure (#398).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreeHandle {
    /// Base layer of the cell in [`stokesforge_plot::PlotSpec::panels`].
    pub base: usize,
    /// The left edge of the cell is grabbed.
    pub left: bool,
    /// The right edge.
    pub right: bool,
    /// The top edge.
    pub top: bool,
    /// The bottom edge.
    pub bottom: bool,
}

impl FreeHandle {
    /// True if no edge is grabbed: the drag moves the frame.
    pub fn is_move(&self) -> bool {
        !(self.left || self.right || self.top || self.bottom)
    }
}

/// The handle under the figure pixel `(x, y)` of a free layout, or `None`
/// (also on the grid).
///
/// Cells are looked up **front to back** (`free_cells` is back to front): the
/// first whose outer rectangle contains the point decides, and what lies under
/// it is hidden; a point outside every outer rectangle but within `tolerance`
/// figure pixels (bounded by [`CELL_EDGE_MAX_TOLERANCE`]) of one is still
/// grabbed, front cell first. On the outer boundary (beyond the tick labels, the same
/// strip as the handles of the grid) the edges and corners resize; inside the
/// frame, the layer moves; in the bands between (axes, titles) nothing is
/// grabbed.
pub fn free_handle_at(layout: &FigureLayout, x: f64, y: f64, tolerance: f64) -> Option<FreeHandle> {
    if layout.placement != LayerPlacement::Free {
        return None;
    }
    let tolerance = if tolerance.is_finite() {
        tolerance.clamp(0.0, CELL_EDGE_MAX_TOLERANCE)
    } else {
        0.0
    };
    // First the cells whose outer rectangle holds the point, then, for the strip
    // that straddles the outside of an outer boundary, those it only reaches:
    // two cells that touch each keep their own side of the shared boundary.
    for slack in [0.0, tolerance] {
        for cell in layout.free_cells.iter().rev() {
            let (ox, oy, ow, oh) = cell.outer;
            let (left, top) = (f64::from(ox), f64::from(oy));
            let (right, bottom) = (left + f64::from(ow), top + f64::from(oh));
            let inside =
                x >= left - slack && x <= right + slack && y >= top - slack && y <= bottom + slack;
            if !inside {
                continue;
            }
            return handle_of(cell, (left, top, right, bottom), (x, y), tolerance);
        }
    }
    None
}

/// What the point grabs on `cell`, whose outer rectangle is `outer` as
/// `(left, top, right, bottom)`: an edge or a corner within `tolerance`, the
/// move inside the frame, nothing in the bands between.
fn handle_of(
    cell: &FreeCellRect,
    outer: (f64, f64, f64, f64),
    (x, y): (f64, f64),
    tolerance: f64,
) -> Option<FreeHandle> {
    let (left, top, right, bottom) = outer;
    let handle = FreeHandle {
        base: cell.base,
        left: (x - left).abs() <= tolerance,
        right: (x - right).abs() <= tolerance,
        top: (y - top).abs() <= tolerance,
        bottom: (y - bottom).abs() <= tolerance,
    };
    // A tiny cell: the nearer side wins, never both.
    let handle = FreeHandle {
        right: handle.right && !(handle.left && (x - left).abs() <= (x - right).abs()),
        left: handle.left && !(handle.right && (x - right).abs() < (x - left).abs()),
        bottom: handle.bottom && !(handle.top && (y - top).abs() <= (y - bottom).abs()),
        top: handle.top && !(handle.bottom && (y - bottom).abs() < (y - top).abs()),
        ..handle
    };
    if !handle.is_move() {
        return Some(handle);
    }
    let (fx, fy, fw, fh) = cell.frame;
    let (fx, fy) = (f64::from(fx), f64::from(fy));
    let in_frame = x >= fx && x <= fx + f64::from(fw) && y >= fy && y <= fy + f64::from(fh);
    in_frame.then_some(handle)
}

/// The frame of a gesture: `start` moved by `(dx, dy)` points (a move) or
/// resized from the edges `handle` grabs. The size stays in
/// `[MIN_FRAME_SIZE, MAX_FRAME_SIZE]`; the edge opposite to a left or top edge
/// that is pulled stays where it is, even when the bound stops the size.
pub fn framed_by(start: FreeFrame, handle: &FreeHandle, dx: f64, dy: f64) -> FreeFrame {
    let bounded = |size: f64| size.clamp(MIN_FRAME_SIZE, MAX_FRAME_SIZE);
    let mut frame = start;
    if handle.is_move() {
        frame.left += dx;
        frame.top += dy;
        return frame;
    }
    if handle.right {
        frame.width = bounded(start.width + dx);
    } else if handle.left {
        frame.width = bounded(start.width - dx);
        frame.left = start.left + start.width - frame.width;
    }
    if handle.bottom {
        frame.height = bounded(start.height + dy);
    } else if handle.top {
        frame.height = bounded(start.height - dy);
        frame.top = start.top + start.height - frame.height;
    }
    frame
}

/// A dashed guide of the magnets: a line at `at` (an x for a vertical guide, a
/// y for a horizontal one, stored page coordinates) between `from` and `to`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Guide {
    /// A vertical line (`at` is an x), else a horizontal one.
    pub vertical: bool,
    /// Position of the line across.
    pub at: f64,
    /// Start of the line along itself.
    pub from: f64,
    /// End of the line along itself.
    pub to: f64,
}

/// A frame after the magnets, and the guides to paint.
#[derive(Debug, Clone, PartialEq)]
pub struct Magnetized {
    /// The frame the gesture ends up with.
    pub frame: FreeFrame,
    /// One guide per snapped axis.
    pub guides: Vec<Guide>,
}

/// The offset that brings the nearest of `moving` onto one of `targets`, if one
/// is within `threshold`; and that target. Ties keep the first moving edge.
fn snap_axis(moving: &[f64], targets: &[f64], threshold: f64) -> Option<(f64, f64)> {
    let mut best: Option<(f64, f64)> = None;
    for &edge in moving {
        for &target in targets {
            let offset = target - edge;
            if offset.abs() <= threshold && best.is_none_or(|(o, _)| offset.abs() < o.abs()) {
                best = Some((offset, target));
            }
        }
    }
    best
}

/// [`framed_by`] with the magnets (#398, L3): the edges the gesture moves snap
/// to the left and right (top and bottom) edges of `others`, **each axis
/// separately**; no centers, no outer rectangles. A move carries both edges of
/// the axis, a resize only the edge it pulls. `delta` is rounded to a
/// hundredth of a point first, so a free gesture leaves tidy numbers and a
/// snapped edge lands exactly on its target. `enabled` false (Shift held): no
/// magnet.
pub fn magnetized(
    start: FreeFrame,
    handle: &FreeHandle,
    delta: (f64, f64),
    others: &[FreeFrame],
    threshold: f64,
    enabled: bool,
) -> Magnetized {
    let round = |v: f64| (v * 100.0).round() / 100.0;
    let (mut dx, mut dy) = (round(delta.0), round(delta.1));
    if enabled && threshold > 0.0 {
        let raw = framed_by(start, handle, dx, dy);
        let moves = handle.is_move();
        let x_edges: Vec<f64> = [
            (moves || handle.left).then_some(raw.left),
            (moves || handle.right).then_some(raw.left + raw.width),
        ]
        .into_iter()
        .flatten()
        .collect();
        let y_edges: Vec<f64> = [
            (moves || handle.top).then_some(raw.top),
            (moves || handle.bottom).then_some(raw.top + raw.height),
        ]
        .into_iter()
        .flatten()
        .collect();
        let xs: Vec<f64> = others
            .iter()
            .flat_map(|o| [o.left, o.left + o.width])
            .collect();
        let ys: Vec<f64> = others
            .iter()
            .flat_map(|o| [o.top, o.top + o.height])
            .collect();
        if let Some((offset, _)) = snap_axis(&x_edges, &xs, threshold) {
            dx += offset;
        }
        if let Some((offset, _)) = snap_axis(&y_edges, &ys, threshold) {
            dy += offset;
        }
    }
    let frame = framed_by(start, handle, dx, dy);
    let guides = if enabled {
        guides_of(&frame, handle, others)
    } else {
        Vec::new()
    };
    Magnetized { frame, guides }
}

/// The guides of a frame: for each axis, the edge the gesture moves that lies
/// exactly on an edge of another frame (the one that made it snap, or one it
/// already met), drawn from the top (left) of the higher (leftmost) of the two
/// frames to the bottom (right) of the lower (rightmost).
fn guides_of(frame: &FreeFrame, handle: &FreeHandle, others: &[FreeFrame]) -> Vec<Guide> {
    const SAME: f64 = 1e-6;
    let moves = handle.is_move();
    let mut guides = Vec::new();
    let (right, bottom) = (frame.left + frame.width, frame.top + frame.height);
    let x_edges = [
        (moves || handle.left).then_some(frame.left),
        (moves || handle.right).then_some(right),
    ];
    'x: for edge in x_edges.into_iter().flatten() {
        for other in others {
            for target in [other.left, other.left + other.width] {
                if (target - edge).abs() <= SAME {
                    guides.push(Guide {
                        vertical: true,
                        at: target,
                        from: frame.top.min(other.top),
                        to: bottom.max(other.top + other.height),
                    });
                    break 'x;
                }
            }
        }
    }
    let y_edges = [
        (moves || handle.top).then_some(frame.top),
        (moves || handle.bottom).then_some(bottom),
    ];
    'y: for edge in y_edges.into_iter().flatten() {
        for other in others {
            for target in [other.top, other.top + other.height] {
                if (target - edge).abs() <= SAME {
                    guides.push(Guide {
                        vertical: false,
                        at: target,
                        from: frame.left.min(other.left),
                        to: right.max(other.left + other.width),
                    });
                    break 'y;
                }
            }
        }
    }
    guides
}

// ---------------------------------------------------------------------------
// Page Layout Mode (#426d): graph boxes on a page, all in stored coordinates.
// ---------------------------------------------------------------------------

/// Smallest page extent the page corners can give, in points.
pub const MIN_PAGE_EXTENT: f64 = 72.0;

/// A graph box of the Page Layout Mode: the plot area and its axis and title
/// bands (the outer rectangle of a cell), in stored page coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphBox {
    /// Base layer of the cell in [`PlotSpec::panels`].
    pub base: usize,
    /// The outer rectangle: the exact stored frame expanded by the gutters.
    pub outer: FreePage,
    /// Bands between the outer rectangle and the frame: `(left, top, right,
    /// bottom)`, in points.
    pub gutters: (f64, f64, f64, f64),
    /// The stored frame (stack included).
    pub frame: FreeFrame,
}

impl GraphBox {
    /// The frame that gives this box the outer rectangle `outer`: the
    /// gutters are taken off, the size stays in `[MIN_FRAME_SIZE,
    /// MAX_FRAME_SIZE]`, the stacking key is kept.
    pub fn frame_for(&self, outer: FreePage) -> FreeFrame {
        let (l, t, r, b) = self.gutters;
        FreeFrame {
            left: outer.left + l,
            top: outer.top + t,
            width: (outer.width - l - r).clamp(MIN_FRAME_SIZE, MAX_FRAME_SIZE),
            height: (outer.height - t - b).clamp(MIN_FRAME_SIZE, MAX_FRAME_SIZE),
            stack: self.frame.stack,
        }
    }

    /// The smallest outer size `(width, height)`: the gutters plus
    /// [`MIN_FRAME_SIZE`].
    fn min_outer(&self) -> (f64, f64) {
        let (l, t, r, b) = self.gutters;
        (l + r + MIN_FRAME_SIZE, t + b + MIN_FRAME_SIZE)
    }
}

/// The graph boxes of a free layout, back to front (the order of
/// [`FigureLayout::free_cells`]). Empty on the grid. The frame is the exact
/// stored one; the gutters are measured from the layout in pixels.
pub fn graph_boxes(spec: &PlotSpec, layout: &FigureLayout) -> Vec<GraphBox> {
    if layout.placement != LayerPlacement::Free {
        return Vec::new();
    }
    layout
        .free_cells
        .iter()
        .filter_map(|cell| {
            let frame = spec.panels.get(cell.base)?.sanitized_free_frame();
            let (ox, oy, ow, oh) = cell.outer;
            let (fx, fy, fw, fh) = cell.frame;
            let left = f64::from(fx) - f64::from(ox);
            let top = f64::from(fy) - f64::from(oy);
            let right = f64::from(ox) + f64::from(ow) - (f64::from(fx) + f64::from(fw));
            let bottom = f64::from(oy) + f64::from(oh) - (f64::from(fy) + f64::from(fh));
            let gutters = (left.max(0.0), top.max(0.0), right.max(0.0), bottom.max(0.0));
            Some(GraphBox {
                base: cell.base,
                outer: FreePage {
                    left: frame.left - gutters.0,
                    top: frame.top - gutters.1,
                    width: frame.width + gutters.0 + gutters.2,
                    height: frame.height + gutters.1 + gutters.3,
                },
                gutters,
                frame,
            })
        })
        .collect()
}

/// Figure pixels to stored page coordinates (the title band is above the
/// stored origin).
pub fn figure_to_stored(layout: &FigureLayout, x: f64, y: f64) -> (f64, f64) {
    (
        x + layout.free_origin.0,
        y - f64::from(layout.title_band) + layout.free_origin.1,
    )
}

/// Stored page coordinates to figure pixels; inverse of [`figure_to_stored`].
pub fn stored_to_figure(layout: &FigureLayout, x: f64, y: f64) -> (f64, f64) {
    (
        x - layout.free_origin.0,
        y + f64::from(layout.title_band) - layout.free_origin.1,
    )
}

/// Reach, in stored points, of a graph's two edge bands on the screen: a lone
/// edge, the centre band of a shared edge (moves both graphs) and its wider
/// side band (moves the graph on the pointer's side only). Built from the
/// display scale so that the reach stays the same on screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutReach {
    /// Reach of an edge with no touching neighbour, and of corners.
    pub edge: f64,
    /// Half width of the centre band of a shared edge.
    pub both: f64,
    /// Half width of the whole band of a shared edge.
    pub side: f64,
}

impl LayoutReach {
    /// The reach for `edge_points` screen points on a lone edge, at the display
    /// `scale` (screen points per stored point).
    pub fn at_scale(edge_points: f64, scale: f64) -> Self {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        Self {
            edge: edge_points.max(0.0) / scale,
            both: BOUNDARY_BOTH_POINTS / scale,
            side: BOUNDARY_SIDE_POINTS / scale,
        }
    }
}

/// What a point grabs in the Page Layout Mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutHandle {
    /// A graph box: an edge or corner resizes, the inside moves (`edges.is_move()`).
    Box {
        /// Base layer of the box.
        base: usize,
        /// The edges grabbed; none = move.
        edges: FreeHandle,
    },
    /// One graph's edge on an edge it shares with a touching graph: only that
    /// graph is resized.
    Side {
        /// Base layer of the box.
        base: usize,
        /// The edge grabbed (exactly one).
        edges: FreeHandle,
    },
    /// The line between two touching graphs: both sides move.
    Boundary {
        /// The line is vertical (the graphs are side by side), else horizontal.
        vertical: bool,
        /// Base of the graph on the left (above) of the line.
        low: usize,
        /// Base of the graph on the right (below) of the line.
        high: usize,
        /// Base of the graph on the pointer's side (`low` if exactly on the line).
        near: usize,
    },
    /// A corner of the page: `right` / `bottom` say which one.
    Page {
        /// The right corner (else the left one).
        right: bool,
        /// The bottom corner (else the top one).
        bottom: bool,
    },
}

/// Whether the edge of `graph` (`vertical`: its left or right edge, `far`: the
/// right or bottom one) touches an edge of `other` and the point `along` (the
/// coordinate along the edge) lies in the overlap of the two.
fn touches(graph: &GraphBox, other: &GraphBox, vertical: bool, far: bool, along: f64) -> bool {
    let (g, h) = (graph.outer, other.outer);
    let (gap, lo, hi) = if vertical {
        let gap = if far {
            h.left - (g.left + g.width)
        } else {
            g.left - (h.left + h.width)
        };
        (
            gap,
            g.top.max(h.top),
            (g.top + g.height).min(h.top + h.height),
        )
    } else {
        let gap = if far {
            h.top - (g.top + g.height)
        } else {
            g.top - (h.top + h.height)
        };
        (
            gap,
            g.left.max(h.left),
            (g.left + g.width).min(h.left + h.width),
        )
    };
    gap.abs() <= TOUCH_TOLERANCE_POINTS
        && hi - lo > TOUCH_TOLERANCE_POINTS
        && along >= lo
        && along <= hi
}

/// The resize handle of `graph` at `(x, y)`, if the point is on one of its
/// edges: a corner first (both axes within `reach.edge`), else the nearest edge
/// across. An edge shared with a touching graph is a [`LayoutHandle::Boundary`]
/// within `reach.both` of the shared line and a [`LayoutHandle::Side`] of the
/// graph on the pointer's side within `reach.side`; a lone edge is a plain
/// [`LayoutHandle::Box`] edge within `reach.edge`.
fn edge_hit(
    boxes: &[GraphBox],
    graph: &GraphBox,
    x: f64,
    y: f64,
    reach: LayoutReach,
) -> Option<LayoutHandle> {
    let o = graph.outer;
    let (left, top) = (o.left, o.top);
    let (right, bottom) = (o.left + o.width, o.top + o.height);
    let wide = reach.edge.max(reach.side);
    if x < left - wide || x > right + wide || y < top - wide || y > bottom + wide {
        return None;
    }
    let near = |a: f64, b: f64| (a - b).abs() <= reach.edge;
    let mut edges = FreeHandle {
        base: graph.base,
        left: near(x, left),
        right: near(x, right),
        top: near(y, top),
        bottom: near(y, bottom),
    };
    // A tiny box: the nearer side wins, never both.
    edges = FreeHandle {
        right: edges.right && !(edges.left && (x - left).abs() <= (x - right).abs()),
        left: edges.left && !(edges.right && (x - right).abs() < (x - left).abs()),
        bottom: edges.bottom && !(edges.top && (y - top).abs() <= (y - bottom).abs()),
        top: edges.top && !(edges.bottom && (y - bottom).abs() < (y - top).abs()),
        ..edges
    };
    // A corner of the graph itself always wins.
    if (edges.left || edges.right) && (edges.top || edges.bottom) {
        return Some(LayoutHandle::Box {
            base: graph.base,
            edges,
        });
    }
    let mut best: Option<(f64, LayoutHandle)> = None;
    for (vertical, far) in [(true, false), (true, true), (false, false), (false, true)] {
        let (across, along, edge_at, flag) = match (vertical, far) {
            (true, false) => (x, y, left, edges.left),
            (true, true) => (x, y, right, edges.right),
            (false, false) => (y, x, top, edges.top),
            (false, true) => (y, x, bottom, edges.bottom),
        };
        let neighbour = boxes
            .iter()
            .rev()
            .find(|h| h.base != graph.base && touches(graph, h, vertical, far, along));
        let candidate = match neighbour {
            Some(h) => {
                let h_edge = match (vertical, far) {
                    (true, false) => h.outer.left + h.outer.width,
                    (true, true) => h.outer.left,
                    (false, false) => h.outer.top + h.outer.height,
                    (false, true) => h.outer.top,
                };
                let d = across - (edge_at + h_edge) / 2.0;
                if d.abs() > reach.side {
                    continue;
                }
                let (low, high) = if far {
                    (graph.base, h.base)
                } else {
                    (h.base, graph.base)
                };
                let near_low = d <= 0.0;
                let handle = if d.abs() <= reach.both {
                    LayoutHandle::Boundary {
                        vertical,
                        low,
                        high,
                        near: if near_low { low } else { high },
                    }
                } else {
                    // The edge of the pointer's graph that faces the line.
                    let edges = FreeHandle {
                        base: if near_low { low } else { high },
                        left: vertical && !near_low,
                        right: vertical && near_low,
                        top: !vertical && !near_low,
                        bottom: !vertical && near_low,
                    };
                    LayoutHandle::Side {
                        base: edges.base,
                        edges,
                    }
                };
                (d.abs(), handle)
            }
            None if flag => {
                let single = FreeHandle {
                    base: graph.base,
                    left: vertical && !far,
                    right: vertical && far,
                    top: !vertical && !far,
                    bottom: !vertical && far,
                };
                (
                    (across - edge_at).abs(),
                    LayoutHandle::Box {
                        base: graph.base,
                        edges: single,
                    },
                )
            }
            None => continue,
        };
        if best.is_none_or(|(d, _)| candidate.0 < d) {
            best = Some(candidate);
        }
    }
    best.map(|(_, handle)| handle)
}

/// The handle under the stored point `(x, y)`, all in points. Order: the page
/// corner handles (a square zone just outside each page corner, `2 * edge`
/// wide); the edges of the `selected` boxes (active first), which win over a box
/// drawn in front of them; then the boxes front to back, where the edges and
/// corners resize (see [`LayoutReach`]) and anywhere else inside the outer
/// rectangle moves. The inside of a selected box never wins over a box in front.
pub fn layout_handle_at(
    boxes: &[GraphBox],
    selected: &[usize],
    page: FreePage,
    x: f64,
    y: f64,
    reach: LayoutReach,
) -> Option<LayoutHandle> {
    let reach = LayoutReach {
        edge: if reach.edge.is_finite() {
            reach.edge.max(0.0)
        } else {
            0.0
        },
        ..reach
    };
    let tolerance = reach.edge;
    let zone = 2.0 * tolerance;
    for right in [false, true] {
        for bottom in [false, true] {
            let corner_x = if right {
                page.left + page.width
            } else {
                page.left
            };
            let corner_y = if bottom {
                page.top + page.height
            } else {
                page.top
            };
            let in_x = if right {
                x > corner_x && x <= corner_x + zone
            } else {
                x < corner_x && x >= corner_x - zone
            };
            let in_y = if bottom {
                y > corner_y && y <= corner_y + zone
            } else {
                y < corner_y && y >= corner_y - zone
            };
            if in_x && in_y {
                return Some(LayoutHandle::Page { right, bottom });
            }
        }
    }
    for base in selected {
        if let Some(graph) = boxes.iter().find(|graph| graph.base == *base) {
            if let Some(handle) = edge_hit(boxes, graph, x, y, reach) {
                return Some(handle);
            }
        }
    }
    // Inside first, then the strip that straddles the outside of a boundary:
    // two boxes that touch each keep their own side.
    for slack in [0.0, tolerance] {
        for graph in boxes.iter().rev() {
            let o = graph.outer;
            let (left, top) = (o.left, o.top);
            let (right, bottom) = (o.left + o.width, o.top + o.height);
            let inside =
                x >= left - slack && x <= right + slack && y >= top - slack && y <= bottom + slack;
            if !inside {
                continue;
            }
            return Some(
                edge_hit(boxes, graph, x, y, reach).unwrap_or(LayoutHandle::Box {
                    base: graph.base,
                    edges: FreeHandle {
                        base: graph.base,
                        left: false,
                        right: false,
                        top: false,
                        bottom: false,
                    },
                }),
            );
        }
    }
    None
}

/// Snapping of a Page Layout Mode gesture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snap {
    /// Grid step in points, lines at `page.left + k * step` and `page.top +
    /// k * step`; `None` = no grid.
    pub grid: Option<f64>,
    /// Reach of the magnets to the edges of the other boxes and the page, in
    /// points.
    pub threshold: f64,
    /// False (Shift held): neither magnets nor grid.
    pub enabled: bool,
}

fn round_hundredth(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The offset that brings the nearest of `moving` onto the grid line
/// `origin + k * step`, if the grid is usable.
fn grid_offset(moving: &[f64], origin: f64, step: Option<f64>) -> Option<f64> {
    let step = step.filter(|s| s.is_finite() && *s > 0.0)?;
    let mut best: Option<f64> = None;
    for &edge in moving {
        let line = origin + ((edge - origin) / step).round() * step;
        let offset = line - edge;
        if best.is_none_or(|b| offset.abs() < b.abs()) {
            best = Some(offset);
        }
    }
    best
}

/// The first moved edge of an axis as a slice, empty when the gesture moves no
/// edge on that axis (a single-edge resize), so that axis gets no grid offset.
fn first_edge(edges: &[f64]) -> &[f64] {
    &edges[..edges.len().min(1)]
}

fn page_as_frame(page: FreePage) -> FreeFrame {
    FreeFrame {
        left: page.left,
        top: page.top,
        width: page.width,
        height: page.height,
        stack: 0,
    }
}

/// Shifts the interval `[low, high]` back inside `[min, max]` (the low side
/// wins when it does not fit).
fn shifted_inside(low: f64, high: f64, min: f64, max: f64) -> f64 {
    let mut shift = 0.0;
    if high > max {
        shift = max - high;
    }
    if low + shift < min {
        shift = min - low;
    }
    shift
}

/// A box moved or resized by a gesture, on its **outer** rectangle: `start`
/// moved by `delta` points, or resized from the `edges` it grabs. With
/// `snap.enabled`, each moved edge snaps first to the edges of `others` and of
/// the page within `snap.threshold` (the guides tell which), otherwise to the
/// nearest grid line; a move snaps its left and top edge to the grid. Then the
/// box is held inside the page (a move shifts back, a resize stops the edge at
/// the page edge). The outer size stays at least gutters + [`MIN_FRAME_SIZE`].
/// Rounded to a hundredth of a point. Returns the frame and the guides.
pub fn moved_box(
    start: &GraphBox,
    edges: &FreeHandle,
    delta: (f64, f64),
    page: FreePage,
    others: &[FreePage],
    snap: Snap,
) -> (FreeFrame, Vec<Guide>) {
    let o = start.outer;
    let moves = edges.is_move();
    let (min_w, min_h) = start.min_outer();
    let (page_r, page_b) = (page.left + page.width, page.top + page.height);
    let (mut dx, mut dy) = (round_hundredth(delta.0), round_hundredth(delta.1));
    // Edges as (left, top, right, bottom) after a delta.
    let rect_after = |dx: f64, dy: f64| -> (f64, f64, f64, f64) {
        let (mut l, mut t, mut r, mut b) = (o.left, o.top, o.left + o.width, o.top + o.height);
        if moves {
            l += dx;
            r += dx;
            t += dy;
            b += dy;
        } else {
            if edges.left {
                l += dx;
            }
            if edges.right {
                r += dx;
            }
            if edges.top {
                t += dy;
            }
            if edges.bottom {
                b += dy;
            }
        }
        (l, t, r, b)
    };
    if snap.enabled {
        let raw = rect_after(dx, dy);
        let x_edges: Vec<f64> = [
            (moves || edges.left).then_some(raw.0),
            (moves || edges.right).then_some(raw.2),
        ]
        .into_iter()
        .flatten()
        .collect();
        let y_edges: Vec<f64> = [
            (moves || edges.top).then_some(raw.1),
            (moves || edges.bottom).then_some(raw.3),
        ]
        .into_iter()
        .flatten()
        .collect();
        let mut xs: Vec<f64> = others
            .iter()
            .flat_map(|p| [p.left, p.left + p.width])
            .collect();
        xs.extend([page.left, page_r]);
        let mut ys: Vec<f64> = others
            .iter()
            .flat_map(|p| [p.top, p.top + p.height])
            .collect();
        ys.extend([page.top, page_b]);
        if snap.threshold > 0.0 {
            // The magnets of the edges of the other boxes and of the page.
            if let Some((offset, _)) = snap_axis(&x_edges, &xs, snap.threshold) {
                dx += offset;
            } else if let Some(offset) = grid_offset(first_edge(&x_edges), page.left, snap.grid) {
                dx += offset;
            }
            if let Some((offset, _)) = snap_axis(&y_edges, &ys, snap.threshold) {
                dy += offset;
            } else if let Some(offset) = grid_offset(first_edge(&y_edges), page.top, snap.grid) {
                dy += offset;
            }
        } else {
            if let Some(offset) = grid_offset(first_edge(&x_edges), page.left, snap.grid) {
                dx += offset;
            }
            if let Some(offset) = grid_offset(first_edge(&y_edges), page.top, snap.grid) {
                dy += offset;
            }
        }
    }
    let (mut l, mut t, mut r, mut b) = rect_after(dx, dy);
    if moves {
        let sx = shifted_inside(l, r, page.left, page_r);
        let sy = shifted_inside(t, b, page.top, page_b);
        l += sx;
        r += sx;
        t += sy;
        b += sy;
    } else {
        // The pulled edge stops at the page edge, then the minimum size holds
        // with the opposite edge fixed.
        if edges.left {
            l = l.max(page.left);
            l = l.min(r - min_w);
        }
        if edges.right {
            r = r.min(page_r);
            r = r.max(l + min_w);
        }
        if edges.top {
            t = t.max(page.top);
            t = t.min(b - min_h);
        }
        if edges.bottom {
            b = b.min(page_b);
            b = b.max(t + min_h);
        }
    }
    let outer = FreePage {
        left: l,
        top: t,
        width: r - l,
        height: b - t,
    };
    let frame = start.frame_for(outer);
    let guides = if snap.enabled {
        let mut targets: Vec<FreeFrame> = others.iter().copied().map(page_as_frame).collect();
        targets.push(page_as_frame(page));
        guides_of(&page_as_frame(outer), edges, &targets)
    } else {
        Vec::new()
    };
    let frame = FreeFrame {
        left: round_hundredth(frame.left),
        top: round_hundredth(frame.top),
        width: round_hundredth(frame.width),
        height: round_hundredth(frame.height),
        stack: frame.stack,
    };
    (frame, guides)
}

/// A box nudged by `delta` points (arrow keys), held inside the page.
pub fn nudged_box(start: &GraphBox, delta: (f64, f64), page: FreePage) -> FreeFrame {
    let o = start.outer;
    let (page_r, page_b) = (page.left + page.width, page.top + page.height);
    let (dx, dy) = (round_hundredth(delta.0), round_hundredth(delta.1));
    let (l, t) = (o.left + dx, o.top + dy);
    let sx = shifted_inside(l, l + o.width, page.left, page_r);
    let sy = shifted_inside(t, t + o.height, page.top, page_b);
    let frame = start.frame_for(FreePage {
        left: l + sx,
        top: t + sy,
        ..o
    });
    FreeFrame {
        left: round_hundredth(frame.left),
        top: round_hundredth(frame.top),
        width: frame.width,
        height: frame.height,
        stack: frame.stack,
    }
}

/// Which graphs a drag of the line between two touching graphs moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoundaryFollow {
    /// Every graph with an edge on that straight line follows, on both sides.
    #[default]
    Line,
    /// Only the two graphs the pointer is between.
    Pair,
}

/// The graphs a both-sides drag moves, frozen at the press.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundaryGroup {
    /// The line is vertical (the graphs are side by side), else horizontal.
    pub vertical: bool,
    /// Position of the line across, in stored points.
    pub line: f64,
    /// Bases of the graphs whose far edge (right or bottom) is on the line.
    pub low: Vec<usize>,
    /// Bases of the graphs whose near edge (left or top) is on the line.
    pub high: Vec<usize>,
}

/// The group a drag of the line between `low` and `high` moves. The line is
/// half way between the facing edges of the two. With [`BoundaryFollow::Line`]
/// every box whose right (bottom) edge is within [`TOUCH_TOLERANCE_POINTS`] of
/// the line joins the low side and every box whose left (top) edge is, the
/// high side; a box that merely crosses the line, or sits off it, does not
/// (never transitive: only the line itself counts).
pub fn boundary_group(
    boxes: &[GraphBox],
    vertical: bool,
    low: usize,
    high: usize,
    follow: BoundaryFollow,
) -> BoundaryGroup {
    let edges = |graph: &GraphBox| {
        let o = graph.outer;
        if vertical {
            (o.left, o.left + o.width)
        } else {
            (o.top, o.top + o.height)
        }
    };
    let find = |base: usize| boxes.iter().find(|graph| graph.base == base);
    let line = match (find(low), find(high)) {
        (Some(a), Some(b)) => (edges(a).1 + edges(b).0) / 2.0,
        _ => 0.0,
    };
    match follow {
        BoundaryFollow::Pair => BoundaryGroup {
            vertical,
            line,
            low: vec![low],
            high: vec![high],
        },
        BoundaryFollow::Line => {
            let on = |a: f64| (a - line).abs() <= TOUCH_TOLERANCE_POINTS;
            let mut group = BoundaryGroup {
                vertical,
                line,
                low: Vec::new(),
                high: Vec::new(),
            };
            for graph in boxes {
                let (start, end) = edges(graph);
                if on(end) {
                    group.low.push(graph.base);
                } else if on(start) {
                    group.high.push(graph.base);
                }
            }
            group
        }
    }
}

/// The frames after dragging the line of `group` by `delta_across` points
/// (across the line: x for a vertical one). One position `L` for the line:
/// `line + delta` rounded, snapped (the magnets to the outer edges of the boxes
/// outside the group and of the page, else the grid from the page origin; none
/// with `snap.enabled` false), held inside the page and so that no box of the
/// group goes under its minimum or over its maximum size, then rounded to a
/// hundredth once. Every low box ends with its outer right (bottom) edge on
/// `L` and every high box with its outer left (top) edge on `L`; their far
/// edges, their other axis and their stacking key stay. Returns `(base,
/// frame)` per box (low first) and the guides.
pub fn moved_boundary(
    boxes: &[GraphBox],
    group: &BoundaryGroup,
    delta_across: f64,
    page: FreePage,
    snap: Snap,
) -> (Vec<(usize, FreeFrame)>, Vec<Guide>) {
    let vertical = group.vertical;
    let find = |base: &usize| boxes.iter().find(|graph| graph.base == *base);
    let lows: Vec<&GraphBox> = group.low.iter().filter_map(find).collect();
    let highs: Vec<&GraphBox> = group.high.iter().filter_map(find).collect();
    if lows.is_empty() || highs.is_empty() {
        return (Vec::new(), Vec::new());
    }
    // Outer start and end across, and the gutters at both ends, per axis.
    let start = |g: &GraphBox| if vertical { g.outer.left } else { g.outer.top };
    let end = |g: &GraphBox| {
        if vertical {
            g.outer.left + g.outer.width
        } else {
            g.outer.top + g.outer.height
        }
    };
    let gutters = |g: &GraphBox| {
        if vertical {
            (g.gutters.0, g.gutters.2)
        } else {
            (g.gutters.1, g.gutters.3)
        }
    };
    let min_outer = |g: &GraphBox| {
        let (w, h) = g.min_outer();
        if vertical {
            w
        } else {
            h
        }
    };
    let max_outer = |g: &GraphBox| {
        let (a, b) = gutters(g);
        MAX_FRAME_SIZE + a + b
    };
    let (page_start, page_end) = if vertical {
        (page.left, page.left + page.width)
    } else {
        (page.top, page.top + page.height)
    };
    let delta = if delta_across.is_finite() {
        round_hundredth(delta_across)
    } else {
        0.0
    };
    let mut at = group.line + delta;
    if snap.enabled {
        let in_group = |g: &&GraphBox| group.low.contains(&g.base) || group.high.contains(&g.base);
        let mut targets: Vec<f64> = boxes
            .iter()
            .filter(|g| !in_group(g))
            .flat_map(|g| [start(g), end(g)])
            .collect();
        targets.extend([page_start, page_end]);
        let origin = page_start;
        if snap.threshold > 0.0 {
            if let Some((offset, _)) = snap_axis(&[at], &targets, snap.threshold) {
                at += offset;
            } else if let Some(offset) = grid_offset(&[at], origin, snap.grid) {
                at += offset;
            }
        } else if let Some(offset) = grid_offset(&[at], origin, snap.grid) {
            at += offset;
        }
    }
    // The bounds hold the whole group; if they cross, the line stays.
    let mut lower = page_start;
    let mut upper = page_end;
    for g in &lows {
        lower = lower.max(start(g) + min_outer(g));
        upper = upper.min(start(g) + max_outer(g));
    }
    for g in &highs {
        upper = upper.min(end(g) - min_outer(g));
        lower = lower.max(end(g) - max_outer(g));
    }
    at = if lower > upper {
        group.line
    } else {
        at.max(lower).min(upper)
    };
    let guides = if snap.enabled {
        let in_group = |g: &&GraphBox| group.low.contains(&g.base) || group.high.contains(&g.base);
        let along = |g: &GraphBox| {
            if vertical {
                (g.outer.top, g.outer.top + g.outer.height)
            } else {
                (g.outer.left, g.outer.left + g.outer.width)
            }
        };
        let (mut from, mut to) = (f64::INFINITY, f64::NEG_INFINITY);
        for g in lows.iter().chain(highs.iter()) {
            let (a, b) = along(g);
            from = from.min(a);
            to = to.max(b);
        }
        let mut hit: Option<(f64, f64, f64)> = None;
        for g in boxes.iter().filter(|g| !in_group(g)) {
            let (a, b) = along(g);
            for target in [start(g), end(g)] {
                if hit.is_none() && (target - at).abs() <= 1e-6 {
                    hit = Some((target, a, b));
                }
            }
        }
        if hit.is_none() {
            let (a, b) = if vertical {
                (page.top, page.top + page.height)
            } else {
                (page.left, page.left + page.width)
            };
            for target in [page_start, page_end] {
                if hit.is_none() && (target - at).abs() <= 1e-6 {
                    hit = Some((target, a, b));
                }
            }
        }
        hit.map(|(target, a, b)| Guide {
            vertical,
            at: target,
            from: from.min(a),
            to: to.max(b),
        })
        .into_iter()
        .collect()
    } else {
        Vec::new()
    };
    let at = round_hundredth(at);
    let bounded = |size: f64| size.clamp(MIN_FRAME_SIZE, MAX_FRAME_SIZE);
    let mut frames = Vec::with_capacity(lows.len() + highs.len());
    for g in &lows {
        let (_, far) = gutters(g);
        let mut frame = g.frame;
        if vertical {
            frame.width = bounded(round_hundredth(at - far - frame.left));
        } else {
            frame.height = bounded(round_hundredth(at - far - frame.top));
        }
        frames.push((g.base, frame));
    }
    for g in &highs {
        let (near, _) = gutters(g);
        let mut frame = g.frame;
        if vertical {
            let far_edge = frame.left + frame.width;
            frame.left = round_hundredth(at + near);
            frame.width = bounded(round_hundredth(far_edge - frame.left));
        } else {
            let far_edge = frame.top + frame.height;
            frame.top = round_hundredth(at + near);
            frame.height = bounded(round_hundredth(far_edge - frame.top));
        }
        frames.push((g.base, frame));
    }
    (frames, guides)
}

/// The page after dragging its `corner` (`(right, bottom)`) by `delta`
/// points: the dragged corner moves, the opposite one stays; each extent is
/// at least [`MIN_PAGE_EXTENT`] and at most [`MAX_FIGURE_EXTENT`]. With
/// `snap.enabled` and a grid, the dragged corner snaps to the grid lines of
/// the page origin. Rounded to a hundredth of a point.
pub fn resized_page(
    start: FreePage,
    corner: (bool, bool),
    delta: (f64, f64),
    snap: Snap,
) -> FreePage {
    let (right, bottom) = corner;
    let (dx, dy) = (round_hundredth(delta.0), round_hundredth(delta.1));
    let axis = |origin: f64, extent: f64, d: f64, far: bool| -> (f64, f64) {
        let (mut low, mut high) = (origin, origin + extent);
        if far {
            high += d;
        } else {
            low += d;
        }
        if snap.enabled {
            let edge = if far { high } else { low };
            if let Some(offset) = grid_offset(&[edge], origin, snap.grid) {
                if far {
                    high += offset;
                } else {
                    low += offset;
                }
            }
        }
        if far {
            high = high.clamp(origin + MIN_PAGE_EXTENT, origin + MAX_FIGURE_EXTENT);
        } else {
            low = low.clamp(
                origin + extent - MAX_FIGURE_EXTENT,
                origin + extent - MIN_PAGE_EXTENT,
            );
        }
        (round_hundredth(low), round_hundredth(high - low))
    };
    // Grid lines of the far edge are measured from the page origin; for the
    // near edge the opposite stays, so lines are measured from the page origin
    // too (`origin` below is always the stored left / top).
    let (left, width) = axis(start.left, start.width, dx, right);
    let (top, height) = axis(start.top, start.height, dy, bottom);
    FreePage {
        left,
        top,
        width,
        height,
    }
}

/// Every box mapped affinely from the page `from` to the page `to`: its outer
/// rectangle follows, the frame is [`GraphBox::frame_for`] of it. `(base,
/// frame)` per box, in the order given.
pub fn scaled_to_page(boxes: &[GraphBox], from: FreePage, to: FreePage) -> Vec<(usize, FreeFrame)> {
    let ratio = |new: f64, old: f64| if old > 0.0 { new / old } else { 1.0 };
    let (sx, sy) = (ratio(to.width, from.width), ratio(to.height, from.height));
    boxes
        .iter()
        .map(|graph| {
            let o = graph.outer;
            let mapped = FreePage {
                left: to.left + (o.left - from.left) * sx,
                top: to.top + (o.top - from.top) * sy,
                width: o.width * sx,
                height: o.height * sy,
            };
            (graph.base, graph.frame_for(mapped))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(left: f64, top: f64, width: f64, height: f64) -> FreeFrame {
        FreeFrame {
            left,
            top,
            width,
            height,
            stack: 0,
        }
    }

    const MOVE: FreeHandle = FreeHandle {
        base: 0,
        left: false,
        right: false,
        top: false,
        bottom: false,
    };

    fn layout(cells: Vec<FreeCellRect>) -> FigureLayout {
        FigureLayout {
            size: (800, 400),
            title_band: 0,
            column_breaks: Vec::new(),
            row_breaks: Vec::new(),
            column_gutters: Vec::new(),
            row_gutters: Vec::new(),
            frame_widths: Vec::new(),
            frame_heights: Vec::new(),
            fixed_overflow: false,
            min_left: 0,
            min_top: 0,
            placement: LayerPlacement::Free,
            free_cells: cells,
            free_origin: (0.0, 0.0),
        }
    }

    fn cell(base: usize, outer: (u32, u32, u32, u32), frame: (u32, u32, u32, u32)) -> FreeCellRect {
        FreeCellRect {
            base,
            row: 0,
            column: base,
            outer,
            frame,
        }
    }

    #[test]
    fn the_resize_edges_follow_the_outer_boundary_and_the_frame_moves() {
        let layout = layout(vec![cell(0, (10, 10, 300, 200), (50, 20, 240, 160))]);
        let at = |x, y| free_handle_at(&layout, x, y, 4.0);
        let right = at(310.0, 100.0).expect("right edge");
        assert!(right.right && !right.left && !right.top && !right.bottom);
        let corner = at(12.0, 208.0).expect("corner");
        assert!(corner.left && corner.bottom && !corner.right && !corner.top);
        // Straddles the boundary: 3 px outside still grabs it.
        assert!(at(8.0, 100.0).is_some_and(|h| h.left));
        assert!(at(100.0, 100.0).is_some_and(|h| h.is_move()));
        // The axis band between the outer boundary and the frame: nothing.
        assert_eq!(at(30.0, 100.0), None);
        assert_eq!(at(500.0, 100.0), None);
    }

    #[test]
    fn the_front_cell_takes_the_handle_and_hides_what_is_under_it() {
        // Cell 1 is drawn after (in front of) cell 0 and overlaps it.
        let layout = layout(vec![
            cell(0, (10, 10, 300, 200), (50, 20, 240, 160)),
            cell(1, (200, 10, 300, 200), (240, 20, 240, 160)),
        ]);
        let shared = free_handle_at(&layout, 150.0, 100.0, 4.0).expect("frame of cell 0");
        assert_eq!(shared.base, 0);
        // In the frame of 0 AND in the band of 1: 1 hides it.
        assert_eq!(free_handle_at(&layout, 220.0, 100.0, 4.0), None);
        // The right edge of 0 lies in the frame of 1: it moves 1, not 0.
        let hidden = free_handle_at(&layout, 310.0, 100.0, 4.0).expect("frame of cell 1");
        assert_eq!((hidden.base, hidden.is_move()), (1, true));
        // The grid has no handle.
        let mut grid = layout.clone();
        grid.placement = LayerPlacement::Grid;
        assert_eq!(free_handle_at(&grid, 150.0, 100.0, 4.0), None);
    }

    /// Two layers that touch each keep their own side of the shared boundary:
    /// the left one's right edge can be grabbed from its side. Witness: one
    /// pass on the widened rectangles gives the front cell both sides.
    #[test]
    fn touching_layers_share_their_boundary() {
        let layout = layout(vec![
            cell(0, (0, 0, 480, 360), (76, 12, 392, 284)),
            cell(1, (480, 0, 480, 360), (556, 12, 392, 284)),
        ]);
        let left = free_handle_at(&layout, 477.0, 100.0, 4.0).expect("right edge of 0");
        assert_eq!((left.base, left.right), (0, true));
        let right = free_handle_at(&layout, 483.0, 100.0, 4.0).expect("left edge of 1");
        assert_eq!((right.base, right.left), (1, true));
        // Just outside the figure: still grabbed.
        assert!(free_handle_at(&layout, 962.0, 100.0, 4.0).is_some_and(|h| h.right));
    }

    #[test]
    fn resizing_keeps_the_opposite_edge_and_the_bounds() {
        let start = frame(100.0, 50.0, 300.0, 200.0);
        let right = FreeHandle {
            right: true,
            ..MOVE
        };
        let left = FreeHandle { left: true, ..MOVE };
        let moved = framed_by(start, &right, 40.0, 99.0);
        assert_eq!((moved.left, moved.width, moved.top), (100.0, 340.0, 50.0));
        let pulled = framed_by(start, &left, -30.0, 0.0);
        assert_eq!((pulled.left, pulled.width), (70.0, 330.0));
        // Beyond the minimum: width 36, the right edge (400) does not move.
        let squeezed = framed_by(start, &left, 500.0, 0.0);
        assert_eq!(squeezed.width, MIN_FRAME_SIZE);
        assert_eq!(squeezed.left + squeezed.width, 400.0);
        // Beyond the maximum.
        let huge = framed_by(start, &right, 1e6, 0.0);
        assert_eq!(huge.width, MAX_FRAME_SIZE);
        let top = FreeHandle { top: true, ..MOVE };
        let squeezed = framed_by(start, &top, 0.0, 1e6);
        assert_eq!(squeezed.height, MIN_FRAME_SIZE);
        assert_eq!(squeezed.top + squeezed.height, 250.0);
        let corner = framed_by(
            start,
            &FreeHandle {
                right: true,
                bottom: true,
                ..MOVE
            },
            10.0,
            20.0,
        );
        assert_eq!((corner.width, corner.height), (310.0, 220.0));
        let whole = framed_by(start, &MOVE, 7.0, -3.0);
        assert_eq!((whole.left, whole.top, whole.width), (107.0, 47.0, 300.0));
    }

    #[test]
    fn a_frame_edge_snaps_to_an_edge_of_another_frame_within_the_threshold() {
        let other = frame(400.0, 300.0, 100.0, 100.0);
        let start = frame(100.0, 0.0, 200.0, 100.0);
        // Right edge would sit at 397: 3 pt from the other's left edge (400).
        let near = magnetized(start, &MOVE, (97.0, 0.0), &[other], 6.0, true);
        assert_eq!(near.frame.left + near.frame.width, 400.0);
        assert_eq!(near.frame.left, 200.0);
        assert!(near.guides.iter().any(|g| g.vertical && g.at == 400.0));
        // 8 pt away: free.
        let far = magnetized(start, &MOVE, (92.0, 0.0), &[other], 6.0, true);
        assert_eq!(far.frame.left, 192.0);
        assert!(far.guides.is_empty());
        // Shift: no magnet even at 3 pt.
        let shift = magnetized(start, &MOVE, (97.0, 0.0), &[other], 6.0, false);
        assert_eq!(shift.frame.left, 197.0);
        assert!(shift.guides.is_empty());
    }

    #[test]
    fn the_magnets_use_the_frames_not_the_centers_and_each_axis_apart() {
        let other = frame(400.0, 300.0, 100.0, 100.0);
        let start = frame(0.0, 0.0, 100.0, 100.0);
        // A 200-wide frame moved by 353 has its centre (x 453) 3 pt from the
        // centre of the other (x 450) while its edges are 47 pt from the
        // other's: the centres do not attract.
        let wide = frame(0.0, 0.0, 200.0, 100.0);
        let centre = magnetized(wide, &MOVE, (353.0, 0.0), &[other], 6.0, true);
        assert_eq!(centre.frame.left, 353.0);
        assert!(centre.guides.is_empty());
        // X snaps while Y is free: the top is 20 pt from the other's top.
        let x_only = magnetized(start, &MOVE, (301.0, 280.0), &[other], 6.0, true);
        assert_eq!(x_only.frame.left + x_only.frame.width, 400.0);
        assert_eq!(x_only.frame.top, 280.0);
        assert_eq!(x_only.guides.len(), 1);
        // A resize snaps only the pulled edge.
        let right = FreeHandle {
            right: true,
            ..MOVE
        };
        let resized = magnetized(start, &right, (297.0, 0.0), &[other], 6.0, true);
        assert_eq!(resized.frame.left, 0.0);
        assert_eq!(resized.frame.width, 400.0);
    }

    // ---- Page Layout Mode (#426d) ----

    fn page(left: f64, top: f64, width: f64, height: f64) -> FreePage {
        FreePage {
            left,
            top,
            width,
            height,
        }
    }

    /// A box with gutters (60, 10, 10, 40) around the frame `(l, t, w, h)`.
    fn gbox(base: usize, l: f64, t: f64, w: f64, h: f64) -> GraphBox {
        let gutters = (60.0, 10.0, 10.0, 40.0);
        GraphBox {
            base,
            outer: page(l - 60.0, t - 10.0, w + 70.0, h + 50.0),
            gutters,
            frame: frame(l, t, w, h),
        }
    }

    const PAGE: FreePage = FreePage {
        left: 0.0,
        top: 0.0,
        width: 800.0,
        height: 600.0,
    };

    fn no_snap() -> Snap {
        Snap {
            grid: None,
            threshold: 6.0,
            enabled: false,
        }
    }

    fn snap() -> Snap {
        Snap {
            grid: None,
            threshold: 6.0,
            enabled: true,
        }
    }

    #[test]
    fn graph_boxes_expand_the_exact_frame_by_the_gutters_back_to_front() {
        use stokesforge_plot::{PanelSpec, Panels};
        let panels: Vec<PanelSpec> = [(0usize, 100.5), (1, 400.25)]
            .iter()
            .map(|&(column, left)| PanelSpec {
                free_frame: Some(frame(left, 20.0, 200.0, 100.0)),
                ..PanelSpec::at(0, column)
            })
            .collect();
        let spec = PlotSpec {
            layer_placement: LayerPlacement::Free,
            ..PlotSpec::empty("").with_panels(Panels::new(panels))
        };
        let mut lay = layout(vec![
            cell(0, (40, 10, 270, 150), (100, 20, 200, 100)),
            cell(1, (340, 10, 270, 150), (400, 20, 200, 100)),
        ]);
        lay.free_origin = (0.0, 0.0);
        let boxes = graph_boxes(&spec, &lay);
        assert_eq!(boxes.len(), 2);
        assert_eq!(boxes[0].base, 0);
        assert_eq!(boxes[0].gutters, (60.0, 10.0, 10.0, 40.0));
        // The frame keeps its stored decimals, the outer is expanded from it.
        assert_eq!(boxes[0].frame.left, 100.5);
        assert_eq!(boxes[0].outer, page(40.5, 10.0, 270.0, 150.0));
        let mut grid = lay.clone();
        grid.placement = LayerPlacement::Grid;
        assert!(graph_boxes(&spec, &grid).is_empty());
    }

    #[test]
    fn stored_and_figure_coordinates_round_trip_through_the_title_band() {
        let mut lay = layout(Vec::new());
        lay.title_band = 30;
        lay.free_origin = (24.0, 38.0);
        assert_eq!(figure_to_stored(&lay, 100.0, 80.0), (124.0, 88.0));
        assert_eq!(stored_to_figure(&lay, 124.0, 88.0), (100.0, 80.0));
    }

    #[test]
    fn frame_for_takes_the_gutters_off_and_keeps_the_minimum_and_the_stack() {
        let mut b = gbox(0, 100.0, 50.0, 200.0, 100.0);
        b.frame.stack = 4;
        let f = b.frame_for(page(40.0, 40.0, 300.0, 120.0));
        assert_eq!(
            (f.left, f.top, f.width, f.height, f.stack),
            (100.0, 50.0, 230.0, 70.0, 4)
        );
        let tiny = b.frame_for(page(0.0, 0.0, 10.0, 10.0));
        assert_eq!((tiny.width, tiny.height), (MIN_FRAME_SIZE, MIN_FRAME_SIZE));
    }

    #[test]
    fn layout_handles_page_corners_first_then_boxes_front_to_back() {
        let a = gbox(0, 100.0, 50.0, 200.0, 100.0); // outer 40..310 x 40..190
        let b = gbox(1, 200.0, 50.0, 200.0, 100.0); // overlaps a
        let boxes = [a, b];
        let at = |x, y| layout_handle_at(&boxes, &[], PAGE, x, y, LayoutReach::at_scale(4.0, 1.0));
        // Outside the bottom-right page corner, within 8 pt.
        assert_eq!(
            at(804.0, 604.0),
            Some(LayoutHandle::Page {
                right: true,
                bottom: true
            })
        );
        assert_eq!(
            at(-3.0, -3.0),
            Some(LayoutHandle::Page {
                right: false,
                bottom: false
            })
        );
        // Inside the page near its corner: not a page handle.
        assert_eq!(at(5.0, 5.0), None);
        // Overlap: the front box (1) wins, inside it moves.
        let Some(LayoutHandle::Box { base, edges }) = at(250.0, 100.0) else {
            panic!("a box");
        };
        assert_eq!(base, 1);
        assert!(edges.is_move());
        // The outer boundary of box 0 where box 1 does not reach: resize.
        let Some(LayoutHandle::Box { base, edges }) = at(41.0, 100.0) else {
            panic!("a box");
        };
        assert!(base == 0 && edges.left && !edges.top);
        // A corner grabs both axes.
        let Some(LayoutHandle::Box { edges, .. }) = at(42.0, 42.0) else {
            panic!("a box");
        };
        assert!(edges.left && edges.top);
        // The whole outer rectangle moves, bands included (not only the frame).
        let Some(LayoutHandle::Box { edges, .. }) = at(60.0, 100.0) else {
            panic!("a box");
        };
        assert!(edges.is_move());
    }

    #[test]
    fn moving_a_box_snaps_to_other_boxes_and_the_page_unless_disabled() {
        let start = gbox(0, 100.0, 50.0, 200.0, 100.0); // outer x 40..310
        let other = page(500.0, 300.0, 100.0, 100.0);
        // Right outer edge would be 497: 3 pt from the other's left edge.
        let (f, guides) = moved_box(
            &start,
            &FreeHandle {
                base: 0,
                left: false,
                right: false,
                top: false,
                bottom: false,
            },
            (187.0, 0.0),
            PAGE,
            &[other],
            snap(),
        );
        assert_eq!(f.left + f.width + 10.0, 500.0);
        assert!(guides.iter().any(|g| g.vertical && g.at == 500.0));
        // Shift: no snap, no guide.
        let (f, guides) = moved_box(
            &start,
            &FreeHandle {
                base: 0,
                left: false,
                right: false,
                top: false,
                bottom: false,
            },
            (187.0, 0.0),
            PAGE,
            &[other],
            no_snap(),
        );
        assert_eq!(f.left, 287.0);
        assert!(guides.is_empty());
    }

    #[test]
    fn a_move_snaps_its_top_left_to_the_grid() {
        let start = gbox(0, 100.0, 50.0, 200.0, 100.0); // outer left 40, top 40
        let grid = Snap {
            grid: Some(10.0),
            threshold: 0.0,
            enabled: true,
        };
        let (f, _) = moved_box(
            &start,
            &FreeHandle {
                base: 0,
                left: false,
                right: false,
                top: false,
                bottom: false,
            },
            (13.0, 27.0),
            PAGE,
            &[],
            grid,
        );
        // Outer left 53 -> 50, top 67 -> 70 (page origin 0): frame = outer + gutters.
        assert_eq!((f.left, f.top), (110.0, 80.0));
    }

    #[test]
    fn a_single_edge_resize_snaps_only_its_own_axis_to_the_grid() {
        let start = gbox(0, 100.0, 50.0, 200.0, 100.0); // outer 40..310 x 40..190
        let edge = |left, right, top, bottom| FreeHandle {
            base: 0,
            left,
            right,
            top,
            bottom,
        };
        for threshold in [6.0, 0.0] {
            let grid = Snap {
                grid: Some(10.0),
                threshold,
                enabled: true,
            };
            let run = |h: FreeHandle, d: (f64, f64)| moved_box(&start, &h, d, PAGE, &[], grid).0;
            // Frame starts at (100, 50) with size 200 x 100; outer edges snap to multiples of 10.
            let f = run(edge(false, false, true, false), (7.0, 13.0)); // top 53 -> 50
            assert_eq!(
                (f.left, f.width, f.top, f.height),
                (100.0, 200.0, 60.0, 90.0)
            );
            let f = run(edge(false, false, false, true), (7.0, 13.0)); // bottom 203 -> 200
            assert_eq!(
                (f.left, f.width, f.top, f.height),
                (100.0, 200.0, 50.0, 110.0)
            );
            let f = run(edge(true, false, false, false), (13.0, 7.0)); // left 53 -> 50
            assert_eq!(
                (f.left, f.width, f.top, f.height),
                (110.0, 190.0, 50.0, 100.0)
            );
            let f = run(edge(false, true, false, false), (13.0, 7.0)); // right 323 -> 320
            assert_eq!(
                (f.left, f.width, f.top, f.height),
                (100.0, 210.0, 50.0, 100.0)
            );
        }
    }

    #[test]
    fn a_move_is_shifted_back_inside_the_page_and_a_resize_stops_at_its_edge() {
        let start = gbox(0, 100.0, 50.0, 200.0, 100.0);
        let mv = FreeHandle {
            base: 0,
            left: false,
            right: false,
            top: false,
            bottom: false,
        };
        let (f, _) = moved_box(&start, &mv, (-1000.0, -1000.0), PAGE, &[], no_snap());
        assert_eq!((f.left - 60.0, f.top - 10.0), (0.0, 0.0));
        let (f, _) = moved_box(&start, &mv, (1000.0, 1000.0), PAGE, &[], no_snap());
        assert_eq!(
            (f.left + f.width + 10.0, f.top + f.height + 40.0),
            (800.0, 600.0)
        );
        let right = FreeHandle { right: true, ..mv };
        let (f, _) = moved_box(&start, &right, (1000.0, 0.0), PAGE, &[], no_snap());
        assert_eq!(f.left + f.width + 10.0, 800.0);
        assert_eq!(f.left, 100.0, "the opposite edge stays");
        let left = FreeHandle { left: true, ..mv };
        let (f, _) = moved_box(&start, &left, (-1000.0, 0.0), PAGE, &[], no_snap());
        assert_eq!(f.left - 60.0, 0.0);
        assert_eq!(f.left + f.width, 300.0);
        // Minimum outer size = gutters + MIN_FRAME_SIZE.
        let (f, _) = moved_box(&start, &left, (1000.0, 0.0), PAGE, &[], no_snap());
        assert_eq!(f.width, MIN_FRAME_SIZE);
        assert_eq!(f.left + f.width, 300.0);
        assert_eq!(f.stack, 0);
    }

    #[test]
    fn a_nudge_moves_and_is_held_inside_the_page() {
        let start = gbox(0, 100.0, 50.0, 200.0, 100.0);
        let f = nudged_box(&start, (10.0, -5.0), PAGE);
        assert_eq!(
            (f.left, f.top, f.width, f.height),
            (110.0, 45.0, 200.0, 100.0)
        );
        let f = nudged_box(&start, (-500.0, 0.0), PAGE);
        assert_eq!(f.left, 60.0);
        let f = nudged_box(&start, (0.0, 5000.0), PAGE);
        assert_eq!(f.top + f.height + 40.0, 600.0);
    }

    #[test]
    fn dragging_a_page_corner_moves_it_and_keeps_the_opposite_one() {
        let p = page(10.0, 20.0, 800.0, 600.0);
        let r = resized_page(p, (true, true), (50.0, -30.0), no_snap());
        assert_eq!(r, page(10.0, 20.0, 850.0, 570.0));
        let r = resized_page(p, (false, false), (40.0, 25.0), no_snap());
        assert_eq!((r.left, r.top), (50.0, 45.0));
        assert_eq!((r.left + r.width, r.top + r.height), (810.0, 620.0));
        // Minimum 72 each way, opposite corner kept.
        let r = resized_page(p, (true, true), (-5000.0, -5000.0), no_snap());
        assert_eq!((r.width, r.height), (72.0, 72.0));
        let r = resized_page(p, (false, true), (5000.0, 0.0), no_snap());
        assert_eq!(r.left + r.width, 810.0);
        assert_eq!(r.width, 72.0);
        // Grid: the dragged corner lands on a line from the page origin.
        let g = Snap {
            grid: Some(50.0),
            threshold: 0.0,
            enabled: true,
        };
        let r = resized_page(p, (true, true), (47.0, 3.0), g);
        assert_eq!((r.width, r.height), (850.0, 600.0));
        let off = Snap {
            enabled: false,
            ..g
        };
        assert_eq!(resized_page(p, (true, true), (47.0, 3.0), off).width, 847.0);
    }

    #[test]
    fn scaling_to_a_page_maps_the_outer_rectangles_and_keeps_the_gutters() {
        let a = gbox(0, 100.0, 50.0, 200.0, 100.0); // outer 40,40 270x150
        let from = page(0.0, 0.0, 800.0, 600.0);
        let to = page(0.0, 0.0, 400.0, 600.0);
        let out = scaled_to_page(&[a], from, to);
        assert_eq!(out.len(), 1);
        let (base, f) = out[0];
        assert_eq!(base, 0);
        // Outer left 40 -> 20, outer width 270 -> 135: frame left 80, width 65.
        assert_eq!((f.left, f.width), (80.0, 65.0));
        assert_eq!((f.top, f.height), (50.0, 100.0));
        // Identity map.
        let same = scaled_to_page(&[a], from, from);
        assert_eq!(same[0].1, a.frame);
    }

    // ---- Boundaries between touching boxes (#426d) ----

    /// A box whose outer rectangle is given; gutters 10 on every side.
    fn ob(base: usize, left: f64, top: f64, width: f64, height: f64) -> GraphBox {
        GraphBox {
            base,
            outer: page(left, top, width, height),
            gutters: (10.0, 10.0, 10.0, 10.0),
            frame: FreeFrame {
                left: left + 10.0,
                top: top + 10.0,
                width: width - 20.0,
                height: height - 20.0,
                stack: 7,
            },
        }
    }

    fn reach(scale: f64) -> LayoutReach {
        LayoutReach::at_scale(6.0, scale)
    }

    fn handle(
        boxes: &[GraphBox],
        selected: &[usize],
        x: f64,
        y: f64,
        scale: f64,
    ) -> Option<LayoutHandle> {
        layout_handle_at(boxes, selected, PAGE, x, y, reach(scale))
    }

    fn pair() -> [GraphBox; 2] {
        [
            ob(0, 0.0, 100.0, 200.0, 100.0),
            ob(1, 200.0, 100.0, 200.0, 100.0),
        ]
    }

    /// Four boxes meeting at (200, 200): 0 top left, 1 top right, 2 bottom
    /// left, 3 bottom right.
    fn quad() -> [GraphBox; 4] {
        [
            ob(0, 0.0, 100.0, 200.0, 100.0),
            ob(1, 200.0, 100.0, 200.0, 100.0),
            ob(2, 0.0, 200.0, 200.0, 100.0),
            ob(3, 200.0, 200.0, 200.0, 100.0),
        ]
    }

    fn is_move(handle: Option<LayoutHandle>, wanted: usize) -> bool {
        matches!(handle, Some(LayoutHandle::Box { base, edges }) if base == wanted && edges.is_move())
    }

    /// Bands in screen points: 2 move both, up to 10 move the pointer's side,
    /// beyond is a plain move. Witness: `<` for `<=`; reach not divided by the
    /// display scale.
    #[test]
    fn the_bands_across_a_shared_edge_are_measured_in_screen_points() {
        let boxes = pair();
        for scale in [1.0, 2.0, 0.5] {
            let both = BOUNDARY_BOTH_POINTS / scale;
            let side = BOUNDARY_SIDE_POINTS / scale;
            let at = |d: f64| handle(&boxes, &[], 200.0 + d, 150.0, scale);
            let boundary = |near| {
                Some(LayoutHandle::Boundary {
                    vertical: true,
                    low: 0,
                    high: 1,
                    near,
                })
            };
            assert_eq!(at(-both), boundary(0), "scale {scale}");
            assert_eq!(at(both), boundary(1), "scale {scale}");
            assert_eq!(at(0.0), boundary(0));
            let Some(LayoutHandle::Side { base, edges }) = at(-(both + 0.01)) else {
                panic!("a side at scale {scale}");
            };
            assert!(base == 0 && edges.right && !edges.left, "scale {scale}");
            assert!(
                matches!(at(side), Some(LayoutHandle::Side { base: 1, edges }) if edges.left),
                "scale {scale}"
            );
            assert!(matches!(
                at(-side),
                Some(LayoutHandle::Side { base: 0, .. })
            ));
            assert!(is_move(at(-(side + 0.01)), 0), "scale {scale}");
            assert!(is_move(at(side + 0.01), 1), "scale {scale}");
        }
    }

    /// Horizontal lines work the same way.
    #[test]
    fn a_horizontal_shared_edge_has_the_same_bands() {
        let boxes = [
            ob(0, 100.0, 0.0, 100.0, 200.0),
            ob(1, 100.0, 200.0, 100.0, 200.0),
        ];
        let at = |d: f64| handle(&boxes, &[], 150.0, 200.0 + d, 1.0);
        assert_eq!(
            at(1.0),
            Some(LayoutHandle::Boundary {
                vertical: false,
                low: 0,
                high: 1,
                near: 1
            })
        );
        assert!(matches!(at(-5.0), Some(LayoutHandle::Side { base: 0, edges }) if edges.bottom));
        assert!(matches!(at(5.0), Some(LayoutHandle::Side { base: 1, edges }) if edges.top));
    }

    /// A corner keeps its single box and diagonal even inside the bands.
    /// Witness: the bands tested before the corners.
    #[test]
    fn a_corner_wins_over_the_bands() {
        let boxes = pair();
        // 1 pt across the shared edge, 3 pt from its end.
        let Some(LayoutHandle::Box { base, edges }) = handle(&boxes, &[], 199.0, 103.0, 1.0) else {
            panic!("a corner");
        };
        assert!(base == 0 && edges.right && edges.top && !edges.left && !edges.bottom);
        let quad = quad();
        for (x, y, base, right, bottom) in [
            (197.0, 197.0, 0, true, true),
            (203.0, 197.0, 1, false, true),
            (197.0, 203.0, 2, true, false),
            (203.0, 203.0, 3, false, false),
        ] {
            let Some(LayoutHandle::Box { base: got, edges }) = handle(&quad, &[], x, y, 1.0) else {
                panic!("a corner at ({x}, {y})");
            };
            assert_eq!(got, base);
            assert_eq!((edges.right, edges.bottom), (right, bottom));
            assert_eq!((edges.left, edges.top), (!right, !bottom));
        }
    }

    /// Away from the corners, the nearer of the two lines wins. Witness:
    /// always trying the vertical line first.
    #[test]
    fn the_nearest_line_wins_in_a_four_box_meeting() {
        let quad = quad();
        assert_eq!(
            handle(&quad, &[], 208.0, 201.0, 1.0),
            Some(LayoutHandle::Boundary {
                vertical: false,
                low: 1,
                high: 3,
                near: 3
            })
        );
        assert_eq!(
            handle(&quad, &[], 201.0, 208.0, 1.0),
            Some(LayoutHandle::Boundary {
                vertical: true,
                low: 2,
                high: 3,
                near: 3
            })
        );
    }

    /// An edge with no touching neighbour keeps the plain reach. Witness: the
    /// band reach applied to every edge.
    #[test]
    fn a_lone_edge_keeps_its_plain_reach() {
        let boxes = [ob(0, 0.0, 100.0, 200.0, 100.0)];
        let Some(LayoutHandle::Box { edges, .. }) = handle(&boxes, &[], 194.0, 150.0, 1.0) else {
            panic!("an edge");
        };
        assert!(edges.right && !edges.left && !edges.top && !edges.bottom);
        assert!(is_move(handle(&boxes, &[], 193.5, 150.0, 1.0), 0));
    }

    /// Only the shared part of a longer edge is a boundary. Witness: the
    /// overlap ignored.
    #[test]
    fn the_unshared_part_of_a_longer_edge_stays_plain() {
        let boxes = [
            ob(0, 0.0, 100.0, 200.0, 200.0),
            ob(1, 200.0, 100.0, 200.0, 100.0),
        ];
        assert!(is_move(handle(&boxes, &[], 192.0, 250.0, 1.0), 0));
        let Some(LayoutHandle::Box { edges, .. }) = handle(&boxes, &[], 195.0, 250.0, 1.0) else {
            panic!("a plain edge");
        };
        assert!(edges.right);
        assert!(matches!(
            handle(&boxes, &[], 192.0, 150.0, 1.0),
            Some(LayoutHandle::Side { base: 0, .. })
        ));
    }

    /// Touching tolerance is one stored point; corner-to-corner does not
    /// count; a real overlap is front to back. Witness: tolerance widened;
    /// `abs` dropped.
    #[test]
    fn touching_means_a_gap_or_overlap_of_at_most_a_point() {
        let a = ob(0, 0.0, 100.0, 200.0, 100.0);
        for offset in [0.8, -0.8] {
            let boxes = [a, ob(1, 200.0 + offset, 100.0, 200.0, 100.0)];
            assert!(
                matches!(
                    handle(&boxes, &[], 200.0 + offset / 2.0, 150.0, 1.0),
                    Some(LayoutHandle::Boundary { .. })
                ),
                "offset {offset}"
            );
        }
        let apart = [a, ob(1, 201.5, 100.0, 200.0, 100.0)];
        let Some(LayoutHandle::Box { base, edges }) = handle(&apart, &[], 199.0, 150.0, 1.0) else {
            panic!("a plain edge");
        };
        assert!(base == 0 && edges.right);
        let diagonal = [
            ob(0, 0.0, 0.0, 100.0, 100.0),
            ob(1, 100.0, 100.0, 100.0, 100.0),
        ];
        let Some(LayoutHandle::Box { edges, .. }) = handle(&diagonal, &[], 99.0, 50.0, 1.0) else {
            panic!("a plain edge");
        };
        assert!(edges.right);
        let overlapping = [a, ob(1, 170.0, 100.0, 200.0, 100.0)];
        assert!(is_move(handle(&overlapping, &[], 185.0, 150.0, 1.0), 1));
    }

    /// A box in front of the line takes the pointer. Witness: the boundary
    /// search before the front to back pass.
    #[test]
    fn a_third_box_over_the_line_takes_the_pointer() {
        let boxes = [
            ob(0, 0.0, 100.0, 200.0, 100.0),
            ob(1, 200.0, 100.0, 200.0, 100.0),
            ob(2, 150.0, 100.0, 100.0, 100.0),
        ];
        assert!(is_move(handle(&boxes, &[], 196.0, 150.0, 1.0), 2));
    }

    /// The selected box's edges win over a box in front, its inside does not.
    /// Witness: no selected step; or the selected step returning moves.
    #[test]
    fn a_selected_box_keeps_its_edges_under_a_box_in_front() {
        let boxes = [
            ob(0, 0.0, 0.0, 300.0, 200.0),
            ob(1, 100.0, 150.0, 100.0, 100.0),
        ];
        let on_edge = handle(&boxes, &[0], 150.0, 199.0, 1.0);
        let Some(LayoutHandle::Box { base, edges }) = on_edge else {
            panic!("an edge");
        };
        assert!(base == 0 && edges.bottom);
        assert!(is_move(handle(&boxes, &[0], 150.0, 170.0, 1.0), 1));
        assert!(is_move(handle(&boxes, &[], 150.0, 199.0, 1.0), 1));
    }

    fn base_set(list: &[usize]) -> Vec<usize> {
        let mut sorted = list.to_vec();
        sorted.sort_unstable();
        sorted
    }

    /// `Line` takes every box with an edge on the line; `Pair` the two. Witness:
    /// the arms swapped; the line test widened.
    #[test]
    fn the_group_of_a_line_follows_the_setting() {
        let mut boxes = quad().to_vec();
        let group = boundary_group(&boxes, true, 0, 1, BoundaryFollow::Line);
        assert_eq!(base_set(&group.low), vec![0, 2]);
        assert_eq!(base_set(&group.high), vec![1, 3]);
        assert_eq!(group.line, 200.0);
        // A fifth box with no neighbour, a sixth that crosses the line.
        boxes.push(ob(4, 100.0, 400.0, 100.0, 100.0));
        boxes.push(ob(5, 150.0, 500.0, 100.0, 100.0));
        let group = boundary_group(&boxes, true, 0, 1, BoundaryFollow::Line);
        assert_eq!(base_set(&group.low), vec![0, 2, 4]);
        assert_eq!(base_set(&group.high), vec![1, 3]);
        let pair = boundary_group(&boxes, true, 0, 1, BoundaryFollow::Pair);
        assert_eq!((pair.low, pair.high), (vec![0], vec![1]));
        // The pointer picks the row.
        let row = |y| match handle(&quad(), &[], 200.0, y, 1.0) {
            Some(LayoutHandle::Boundary { low, high, .. }) => (low, high),
            other => panic!("a boundary, got {other:?}"),
        };
        assert_eq!(row(150.0), (0, 1));
        assert_eq!(row(250.0), (2, 3));
        // A bottom row 30 points further right is another line.
        let mut offset = quad().to_vec();
        offset[2] = ob(2, 0.0, 200.0, 230.0, 100.0);
        offset[3] = ob(3, 230.0, 200.0, 200.0, 100.0);
        let group = boundary_group(&offset, true, 0, 1, BoundaryFollow::Line);
        assert_eq!((group.low, group.high), (vec![0], vec![1]));
    }

    fn moved(
        boxes: &[GraphBox],
        group: &BoundaryGroup,
        delta: f64,
        snap: Snap,
    ) -> (Vec<(usize, FreeFrame)>, Vec<Guide>) {
        moved_boundary(boxes, group, delta, PAGE, snap)
    }

    fn line_of(boxes: &[GraphBox], vertical: bool) -> BoundaryGroup {
        boundary_group(boxes, vertical, 0, 1, BoundaryFollow::Line)
    }

    fn outer_right(frame: &FreeFrame) -> f64 {
        frame.left + frame.width + 10.0
    }

    /// A move writes every box of the group from one line, and leaves the other
    /// axis and the stacking key alone.
    #[test]
    fn dragging_the_line_moves_both_sides_and_keeps_the_other_axis() {
        let boxes = quad();
        let group = line_of(&boxes, true);
        let (frames, guides) = moved(&boxes, &group, 3.0, snap());
        assert!(guides.is_empty());
        assert_eq!(frames.len(), 4);
        for (base, frame) in &frames {
            let old = boxes.iter().find(|b| b.base == *base).expect("box").frame;
            assert_eq!(
                (frame.top, frame.height, frame.stack),
                (old.top, old.height, old.stack)
            );
            if group.low.contains(base) {
                assert_eq!(frame.left, old.left);
                assert_eq!(outer_right(frame), 203.0);
            } else {
                assert_eq!(frame.left - 10.0, 203.0);
                assert_eq!(frame.left + frame.width, old.left + old.width);
            }
        }
    }

    /// Witness: the group's own edges kept as magnet targets.
    #[test]
    fn the_magnet_ignores_the_group_and_catches_other_boxes() {
        let boxes = [
            ob(0, 0.0, 100.0, 200.0, 100.0),
            ob(1, 200.0, 100.0, 200.0, 100.0),
            ob(2, 206.0, 300.0, 100.0, 100.0),
        ];
        let group = boundary_group(&boxes, true, 0, 1, BoundaryFollow::Line);
        assert_eq!(base_set(&group.low), vec![0]);
        let (frames, guides) = moved(&boxes, &group, 3.0, snap());
        assert_eq!(outer_right(&frames[0].1), 206.0);
        assert!(guides.iter().any(|g| g.vertical && g.at == 206.0));
        let (frames, _) = moved(&boxes[..2], &group, 3.0, snap());
        assert_eq!(outer_right(&frames[0].1), 203.0, "no other box: free");
        let (frames, guides) = moved(&boxes, &group, 3.0, no_snap());
        assert_eq!(outer_right(&frames[0].1), 203.0);
        assert!(guides.is_empty());
    }

    /// The grid is measured from the page origin, not from zero.
    #[test]
    fn the_line_lands_on_a_grid_line_from_the_page_origin() {
        let boxes = pair();
        let group = line_of(&boxes, true);
        let grid = Snap {
            grid: Some(50.0),
            threshold: 0.0,
            enabled: true,
        };
        let shifted = page(17.0, 0.0, 800.0, 600.0);
        let (frames, _) = moved_boundary(&boxes, &group, 3.0, shifted, grid);
        assert_eq!(outer_right(&frames[0].1), 217.0);
        let (frames, _) = moved_boundary(&boxes, &group, 3.0, PAGE, grid);
        assert_eq!(outer_right(&frames[0].1), 200.0);
        // With magnets on, a grid still applies when no edge is near.
        let both = Snap {
            threshold: 6.0,
            ..grid
        };
        let (frames, _) = moved_boundary(&boxes, &group, 21.0, shifted, both);
        assert_eq!(outer_right(&frames[0].1), 217.0);
    }

    /// Transposes a vertical-line fixture into a horizontal one.
    fn transposed(boxes: &[GraphBox]) -> Vec<GraphBox> {
        boxes
            .iter()
            .map(|b| {
                ob(
                    b.base,
                    b.outer.top,
                    b.outer.left,
                    b.outer.height,
                    b.outer.width,
                )
            })
            .collect()
    }

    /// The whole line stops at the tightest box of the group, in both
    /// directions and on both axes. Witness: clamping from the pair only, or
    /// per box.
    #[test]
    fn the_line_stops_where_the_tightest_box_of_the_group_stops() {
        // Low box 2 is the narrow one on the left (min outer 56 from 100),
        // high box 3 the narrow one on the right (ends at 300).
        let vertical = vec![
            ob(0, 0.0, 100.0, 200.0, 100.0),
            ob(1, 200.0, 100.0, 200.0, 100.0),
            ob(2, 100.0, 200.0, 100.0, 100.0),
            ob(3, 200.0, 200.0, 100.0, 100.0),
        ];
        for vertical_line in [true, false] {
            let boxes = if vertical_line {
                vertical.clone()
            } else {
                transposed(&vertical)
            };
            let group = line_of(&boxes, vertical_line);
            assert_eq!(group.low.len() + group.high.len(), 4);
            for (delta, wanted) in [(-500.0, 156.0), (500.0, 244.0)] {
                let (frames, _) = moved(&boxes, &group, delta, no_snap());
                let (lows, highs): (Vec<_>, Vec<_>) = frames
                    .iter()
                    .partition(|(base, _)| group.low.contains(base));
                let edge = |frame: &FreeFrame, far: bool| match (vertical_line, far) {
                    (true, true) => frame.left + frame.width + 10.0,
                    (true, false) => frame.left - 10.0,
                    (false, true) => frame.top + frame.height + 10.0,
                    (false, false) => frame.top - 10.0,
                };
                for (_, frame) in &lows {
                    assert!((edge(frame, true) - wanted).abs() < 0.006, "{frame:?}");
                }
                for (_, frame) in &highs {
                    assert!((edge(frame, false) - wanted).abs() < 0.006, "{frame:?}");
                }
            }
        }
    }

    /// The page edges and the maximum size also stop the line.
    #[test]
    fn the_page_edges_and_the_maximum_size_stop_the_line() {
        // Boxes that stick out of the page: the page holds the line.
        let boxes = [
            ob(0, -100.0, 100.0, 300.0, 100.0),
            ob(1, 200.0, 100.0, 800.0, 100.0),
        ];
        let group = line_of(&boxes, true);
        let (frames, _) = moved(&boxes, &group, -500.0, no_snap());
        assert_eq!(outer_right(&frames[0].1), 0.0);
        let (frames, _) = moved(&boxes, &group, 5000.0, no_snap());
        assert_eq!(frames[1].1.left - 10.0, 800.0);
        // Maximum size: low box 1500 wide cannot grow beyond 2000 + gutters.
        let big = Snap {
            enabled: false,
            ..snap()
        };
        let wide = page(0.0, 0.0, 20_000.0, 600.0);
        let boxes = [
            ob(0, 0.0, 100.0, 1500.0, 100.0),
            ob(1, 1500.0, 100.0, 1800.0, 100.0),
        ];
        let group = line_of(&boxes, true);
        let (frames, _) = moved_boundary(&boxes, &group, 3000.0, wide, big);
        assert_eq!(frames[0].1.width, MAX_FRAME_SIZE);
        // And the high box cannot grow past its maximum when the line goes left.
        let boxes = [
            ob(0, 0.0, 100.0, 1800.0, 100.0),
            ob(1, 1800.0, 100.0, 1500.0, 100.0),
        ];
        let group = line_of(&boxes, true);
        let (frames, _) = moved_boundary(&boxes, &group, -3000.0, wide, big);
        assert_eq!(frames[1].1.width, MAX_FRAME_SIZE);
    }

    /// Both boxes at their minimum and overlapping a little: the bounds cross,
    /// the line stays. Witness: `f64::clamp`, which panics.
    #[test]
    fn crossing_bounds_leave_the_line_where_it_is() {
        let boxes = [
            ob(0, 0.0, 100.0, 56.0, 100.0),
            ob(1, 55.2, 100.0, 56.0, 100.0),
        ];
        let group = boundary_group(&boxes, true, 0, 1, BoundaryFollow::Pair);
        for delta in [-30.0, 0.0, 30.0] {
            let (frames, _) = moved(&boxes, &group, delta, no_snap());
            assert_eq!(frames.len(), 2);
            for (_, frame) in &frames {
                assert!(frame.width >= MIN_FRAME_SIZE && frame.width.is_finite());
            }
        }
    }

    /// One rounded line: the two sides meet to the hundredth, the far edges
    /// stay, and the result is still a boundary. Witness: rounding per box.
    #[test]
    fn the_two_sides_meet_after_the_rounding() {
        let step = Snap {
            grid: Some(7.777),
            threshold: 0.0,
            enabled: true,
        };
        for (low_left, high_left, tolerance) in [(0.0, 200.0, 1e-9), (0.1237, 200.3081, 0.005)] {
            let boxes = [
                ob(0, low_left, 100.0, 200.0, 100.0),
                ob(1, high_left, 100.0, 200.0, 100.0),
            ];
            let group = boundary_group(&boxes, true, 0, 1, BoundaryFollow::Pair);
            let (frames, _) = moved(&boxes, &group, 3.337, step);
            let low_right = outer_right(&frames[0].1);
            let new_left = frames[1].1.left - 10.0;
            assert!(
                (low_right - new_left).abs() <= tolerance,
                "{low_right} vs {new_left} (tolerance {tolerance})"
            );
            let old_far = boxes[1].frame.left + boxes[1].frame.width;
            assert!((frames[1].1.left + frames[1].1.width - old_far).abs() <= 0.005);
            assert_eq!(frames[0].1.left, boxes[0].frame.left);
            // Feed the result back: still a boundary.
            let fed: Vec<GraphBox> = frames
                .iter()
                .zip(&boxes)
                .map(|((_, f), b)| GraphBox {
                    outer: page(f.left - 10.0, f.top - 10.0, f.width + 20.0, f.height + 20.0),
                    frame: *f,
                    ..*b
                })
                .collect();
            let mid = (low_right + new_left) / 2.0;
            assert!(matches!(
                handle(&fed, &[], mid, 150.0, 1.0),
                Some(LayoutHandle::Boundary { .. })
            ));
        }
    }
}
