//! The geometry of the mouse gestures of free layer placement (#398, #426),
//! without egui: where a moved or resized frame ends up in the Page Layout
//! Mode, and the magnets that snap its edges to the edges of the other layers.
//!
//! Positions and sizes are **stored page coordinates, in points** (the numbers
//! of [`FreeFrame`]); [`figure_to_stored`] and [`stored_to_figure`] convert
//! from and to figure pixels on a [`FigureLayout`]. Plain data in, plain data out: the screen layer
//! (`stokesforge-render-egui`) converts screen points to points with the
//! display scale it froze at the press.

use crate::pipeline::FigureLayout;
use stokesforge_plot::{
    FreeFrame, FreePage, LayerPlacement, PlotSpec, MAX_FIGURE_EXTENT, MAX_FRAME_SIZE,
    MIN_FRAME_SIZE,
};

/// How close, **in screen points**, an edge of a dragged frame must come to an
/// edge of another layer to snap to it. Converted into points by the caller
/// (`6 / display_scale`), so the magnet keeps the same reach on screen
/// whatever the enlargement of the figure.
pub const MAGNET_THRESHOLD_POINTS: f64 = 6.0;

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

// Page Layout Mode (#426): graph boxes on a page, in stored coordinates.

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

/// Extra room kept around the graph boxes by [`page_fitted_to_layers`], in
/// points, on top of the margin every box already carries in its bands (#505,
/// decision S3: the margin of a new figure, so none more).
pub const FIT_PAGE_EXTRA_MARGIN: f64 = 0.0;

/// The page that fits the graph boxes (#505): the bounding box of their outer
/// rectangles (frame, axis bands and the margin they keep), grown by
/// [`FIT_PAGE_EXTRA_MARGIN`] and brought into `[MIN_PAGE_EXTENT,
/// MAX_FIGURE_EXTENT]`. The boxes do not move: the page is what changes.
/// `None` without a box, or if a number is not finite.
pub fn page_fitted_to_layers(boxes: &[GraphBox]) -> Option<FreePage> {
    if boxes.is_empty() {
        return None;
    }
    let (mut left, mut top) = (f64::INFINITY, f64::INFINITY);
    let (mut right, mut bottom) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for graph in boxes {
        let o = graph.outer;
        left = left.min(o.left);
        top = top.min(o.top);
        right = right.max(o.left + o.width);
        bottom = bottom.max(o.top + o.height);
    }
    let all = [left, top, right, bottom];
    if all.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let margin = FIT_PAGE_EXTRA_MARGIN;
    Some(FreePage {
        left: left - margin,
        top: top - margin,
        width: (right - left + 2.0 * margin).clamp(MIN_PAGE_EXTENT, MAX_FIGURE_EXTENT),
        height: (bottom - top + 2.0 * margin).clamp(MIN_PAGE_EXTENT, MAX_FIGURE_EXTENT),
    })
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

/// Smallest page extent the page corners can give, in points.
pub const MIN_PAGE_EXTENT: f64 = 72.0;

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

/// Reach of an edge or corner with no touching neighbour, in screen pixels.
pub const EDGE_REACH_PX: f64 = 6.0;
/// Half width of the centre band of a shared edge, in screen pixels: inside
/// it a drag moves both graphs.
pub const BOUNDARY_CENTRE_PX: f64 = 3.0;
/// Half width of the whole band of a shared edge, in screen pixels: between
/// the centre band and this limit a drag moves only the graph on the
/// pointer's side.
pub const BOUNDARY_SIDE_PX: f64 = 23.0;
/// Share of a graph's outer extent, along an axis, that an inner reach may
/// take: every reach that enters a graph is `min(wanted, extent / 4)`, so the
/// centre half of a graph always moves it.
pub const INNER_REACH_SHARE: f64 = 0.25;
/// Gap or overlap, in points, under which two graphs count as touching.
pub const TOUCH_TOLERANCE_POINTS: f64 = 1.0;

/// The reach of the handles of the Page Layout Mode. The bands are fixed in
/// **screen pixels**; `scale` (screen pixels per stored point, or per figure
/// pixel on a grid) converts them, so that they keep the same size on screen
/// whatever the enlargement of the figure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutReach {
    /// Reach of a lone edge and of corners, in screen pixels.
    pub edge_px: f64,
    /// Screen pixels per point; anything not finite or not positive reads 1.
    pub scale: f64,
}

impl LayoutReach {
    /// The reach of `edge_px` screen pixels on a lone edge at the display
    /// `scale`.
    pub fn at_scale(edge_px: f64, scale: f64) -> Self {
        Self { edge_px, scale }
    }

    /// The standard reach ([`EDGE_REACH_PX`]) at the display `scale`.
    pub fn standard(scale: f64) -> Self {
        Self::at_scale(EDGE_REACH_PX, scale)
    }

    fn scale(&self) -> f64 {
        if self.scale.is_finite() && self.scale > 0.0 {
            self.scale
        } else {
            1.0
        }
    }

    fn edge_px(&self) -> f64 {
        if self.edge_px.is_finite() {
            self.edge_px.max(0.0)
        } else {
            0.0
        }
    }

    /// The reach of a lone edge or a corner in points.
    pub fn edge_points(&self) -> f64 {
        self.edge_px() / self.scale()
    }

    /// A reach that enters a graph, in screen pixels: `wanted_px`, but at most
    /// a quarter of the graph's outer `extent` (points) on that axis.
    pub fn inner_px(&self, wanted_px: f64, extent: f64) -> f64 {
        wanted_px.min(extent.max(0.0) * self.scale() * INNER_REACH_SHARE)
    }

    /// Whether the signed distance `d` (points, negative on the `before`
    /// side, positive on the `after` side) of a pointer from a line lies
    /// within the edge reach. A side that is a graph of outer extent `before`
    /// (`after`) points reaches only a quarter of it; a side with `None` is
    /// outside every graph and reaches the full distance.
    pub fn within_edge(&self, d: f64, before: Option<f64>, after: Option<f64>) -> bool {
        let extent = if d < 0.0 { before } else { after };
        let px = d.abs() * self.scale();
        match extent {
            Some(e) => px <= self.inner_px(self.edge_px(), e),
            None => px <= self.edge_px(),
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
    /// graph is resized, on the pointer's side of the line.
    Side {
        /// Base layer of the box.
        base: usize,
        /// The edge grabbed (exactly one): the one that faces the line.
        edges: FreeHandle,
    },
    /// The line between two touching graphs: the two of them move together.
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
/// edges: a corner first (both axes within the edge reach), else the nearest
/// edge across. An edge shared with a touching graph is a
/// [`LayoutHandle::Boundary`] within the centre band of the shared line and a
/// [`LayoutHandle::Side`] of the graph on the pointer's side within the whole
/// band; a lone edge is a plain [`LayoutHandle::Box`] edge within the edge
/// reach. Every reach that enters a graph shrinks with its size
/// ([`INNER_REACH_SHARE`]); each side of a shared line uses its own graph's
/// extent and the centre band takes the smaller of the two.
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
    let wide = reach.edge_points().max(BOUNDARY_SIDE_PX / reach.scale());
    if x < left - wide || x > right + wide || y < top - wide || y > bottom + wide {
        return None;
    }
    // A reach is measured from the edge: outward in full, inward shrunk.
    let mut edges = FreeHandle {
        base: graph.base,
        left: reach.within_edge(x - left, None, Some(o.width)),
        right: reach.within_edge(x - right, Some(o.width), None),
        top: reach.within_edge(y - top, None, Some(o.height)),
        bottom: reach.within_edge(y - bottom, Some(o.height), None),
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
    let scale = reach.scale();
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
                let (low_box, high_box) = if far { (graph, h) } else { (h, graph) };
                let extent = |g: &GraphBox| {
                    if vertical {
                        g.outer.width
                    } else {
                        g.outer.height
                    }
                };
                let (low_extent, high_extent) = (extent(low_box), extent(high_box));
                let near_low = d <= 0.0;
                let side_limit = reach.inner_px(
                    BOUNDARY_SIDE_PX,
                    if near_low { low_extent } else { high_extent },
                );
                let centre_limit = reach
                    .inner_px(BOUNDARY_CENTRE_PX, low_extent)
                    .min(reach.inner_px(BOUNDARY_CENTRE_PX, high_extent));
                let d_px = d.abs() * scale;
                if d_px > side_limit {
                    continue;
                }
                let (low, high) = (low_box.base, high_box.base);
                let handle = if d_px <= centre_limit {
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
/// corner handles (a square zone just outside each page corner, twice the edge
/// reach wide); the edges of the `selected` boxes (active first), which win over a box
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
    let tolerance = reach.edge_points();
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

/// The graphs a both-sides drag moves, frozen at the press: the pair the
/// pointer is between, and only them (there is no whole-line following in the
/// Free placement).
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

/// The group a drag of the line between `low` and `high` moves: those two
/// graphs. The line is half way between the facing edges of the two.
pub fn boundary_group(
    boxes: &[GraphBox],
    vertical: bool,
    low: usize,
    high: usize,
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
    BoundaryGroup {
        vertical,
        line,
        low: vec![low],
        high: vec![high],
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
/// points (#505, K1): the two page edges that meet there move, the opposite
/// corner stays, and **the graph boxes are not touched**: the page gains or
/// loses empty space. Inward it stops at the bounding box of the outer
/// rectangles of the `boxes` (a page already smaller than the graphs is never
/// pushed further in); the extent stays in `[MIN_PAGE_EXTENT,
/// MAX_FIGURE_EXTENT]`. With `snap.enabled` and a grid, the dragged edge snaps
/// to the grid lines of the page origin before the bounds. Rounded to a
/// hundredth of a point.
pub fn moved_page_corner(
    boxes: &[GraphBox],
    start: FreePage,
    corner: (bool, bool),
    delta: (f64, f64),
    snap: Snap,
) -> FreePage {
    let (dx, dy) = (round_hundredth(delta.0), round_hundredth(delta.1));
    let (mut left, mut top) = (f64::INFINITY, f64::INFINITY);
    let (mut right, mut bottom) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for graph in boxes {
        let o = graph.outer;
        left = left.min(o.left);
        top = top.min(o.top);
        right = right.max(o.left + o.width);
        bottom = bottom.max(o.top + o.height);
    }
    // `(near, far)` limits the dragged edge may not cross going inward.
    let axis = |origin: f64, extent: f64, d: f64, far: bool, bbox: (f64, f64)| -> (f64, f64) {
        if extent.is_nan() || extent <= 0.0 || !d.is_finite() {
            return (origin, extent);
        }
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
        if bbox.0 <= bbox.1 {
            if far {
                high = high.max(bbox.1.min(origin + extent));
            } else {
                low = low.min(bbox.0.max(origin));
            }
        }
        let new_extent = (high - low).clamp(MIN_PAGE_EXTENT, MAX_FIGURE_EXTENT);
        let low = if far {
            origin
        } else {
            origin + extent - new_extent
        };
        (round_hundredth(low), round_hundredth(new_extent))
    };
    let (left, width) = axis(start.left, start.width, dx, corner.0, (left, right));
    let (top, height) = axis(start.top, start.height, dy, corner.1, (top, bottom));
    FreePage {
        left,
        top,
        width,
        height,
    }
}

/// The frames of **Fit Layout to Page** in Free (#505, K1), the inverse of
/// [`page_fitted_to_layers`]: the graph boxes are mapped affinely from their
/// bounding box to the `page` ([`scaled_to_page`]), bands frozen. With `uniform`
/// (the Figure size padlock is closed) one factor, the smaller of the two,
/// scales both axes from the top-left corner of the page, so the leftover
/// space stays on its right or bottom. `None` without a box, or when the boxes
/// already fill the page (nothing to do, no undo step).
pub fn boxes_fitted_to_page(
    boxes: &[GraphBox],
    page: FreePage,
    uniform: bool,
) -> Option<Vec<(usize, FreeFrame)>> {
    let from = page_fitted_to_layers(boxes)?;
    if from.width <= 0.0 || from.height <= 0.0 {
        return None;
    }
    let mut to = page;
    if uniform {
        let factor = (page.width / from.width).min(page.height / from.height);
        to.width = from.width * factor;
        to.height = from.height * factor;
    }
    let frames = scaled_to_page(boxes, from, to);
    let unchanged = frames.iter().all(|(base, frame)| {
        boxes
            .iter()
            .find(|graph| graph.base == *base)
            .is_some_and(|graph| {
                let old = graph.frame;
                (old.left - frame.left).abs() < 0.005
                    && (old.top - frame.top).abs() < 0.005
                    && (old.width - frame.width).abs() < 0.005
                    && (old.height - frame.height).abs() < 0.005
            })
    });
    (!unchanged).then_some(frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::FreeCellRect;

    fn frame(left: f64, top: f64, width: f64, height: f64) -> FreeFrame {
        FreeFrame {
            left,
            top,
            width,
            height,
            stack: 0,
        }
    }

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
            grid_slack: [0; 4],
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
        let r = moved_page_corner(&[], p, (true, true), (50.0, -30.0), no_snap());
        assert_eq!(r, page(10.0, 20.0, 850.0, 570.0));
        let r = moved_page_corner(&[], p, (false, false), (40.0, 25.0), no_snap());
        assert_eq!((r.left, r.top), (50.0, 45.0));
        assert_eq!((r.left + r.width, r.top + r.height), (810.0, 620.0));
        // Minimum 72 each way, opposite corner kept.
        let r = moved_page_corner(&[], p, (true, true), (-5000.0, -5000.0), no_snap());
        assert_eq!((r.width, r.height), (72.0, 72.0));
        let r = moved_page_corner(&[], p, (false, true), (5000.0, 0.0), no_snap());
        assert_eq!(r.left + r.width, 810.0);
        assert_eq!(r.width, 72.0);
        // Grid: the dragged corner lands on a line from the page origin.
        let g = Snap {
            grid: Some(50.0),
            threshold: 0.0,
            enabled: true,
        };
        let r = moved_page_corner(&[], p, (true, true), (47.0, 3.0), g);
        assert_eq!((r.width, r.height), (850.0, 600.0));
        let off = Snap {
            enabled: false,
            ..g
        };
        assert_eq!(
            moved_page_corner(&[], p, (true, true), (47.0, 3.0), off).width,
            847.0
        );
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

    // ---- Boundaries between touching boxes (#426) ----

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

    /// Bands in screen pixels: up to 3 move both graphs, up to 23 move the
    /// pointer's side, beyond is a plain move. Witness: the old constants (2
    /// and 10), `<` for `<=`, reach not divided by the display scale.
    #[test]
    fn the_bands_across_a_shared_edge_are_measured_in_screen_pixels() {
        let boxes = pair();
        for scale in [1.0, 2.0, 0.5] {
            let at = |px: f64| handle(&boxes, &[], 200.0 + px / scale, 150.0, scale);
            let boundary = |near| {
                Some(LayoutHandle::Boundary {
                    vertical: true,
                    low: 0,
                    high: 1,
                    near,
                })
            };
            assert_eq!(at(-3.0), boundary(0), "scale {scale}");
            assert_eq!(at(3.0), boundary(1), "scale {scale}");
            assert_eq!(at(0.0), boundary(0));
            // Side bands: the single edge that faces the line, pointing out.
            let Some(LayoutHandle::Side { base, edges }) = at(-3.1) else {
                panic!("a side at scale {scale}");
            };
            assert!(base == 0 && edges.right && !edges.left, "scale {scale}");
            assert!(
                matches!(at(3.1), Some(LayoutHandle::Side { base: 1, edges }) if edges.left),
                "scale {scale}"
            );
            assert!(
                matches!(at(23.0), Some(LayoutHandle::Side { base: 1, edges }) if edges.left),
                "scale {scale}"
            );
            assert!(matches!(
                at(-23.0),
                Some(LayoutHandle::Side { base: 0, .. })
            ));
            assert!(is_move(at(-23.1), 0), "scale {scale}");
            assert!(is_move(at(23.1), 1), "scale {scale}");
        }
    }

    /// A small graph keeps a zone that moves it: every reach into a graph is
    /// at most a quarter of its outer extent, the centre band included, and
    /// each side of the line uses its own graph. Witness: the shrink removed.
    #[test]
    fn a_small_graph_shrinks_its_side_band_and_keeps_a_zone_to_move_it() {
        // Box 0 is 40 wide (limit 10), box 1 is 200 wide (limit 23).
        let boxes = [
            ob(0, 160.0, 100.0, 40.0, 100.0),
            ob(1, 200.0, 100.0, 200.0, 100.0),
        ];
        let at = |d: f64| handle(&boxes, &[], 200.0 + d, 150.0, 1.0);
        assert!(matches!(at(-3.0), Some(LayoutHandle::Boundary { .. })));
        assert!(matches!(
            at(-10.0),
            Some(LayoutHandle::Side { base: 0, .. })
        ));
        // Beyond 10 points inside the 40-wide box: it moves, not a side.
        assert!(is_move(at(-10.1), 0));
        assert!(is_move(at(-30.0), 0));
        // The other side still reaches 23.
        assert!(matches!(at(23.0), Some(LayoutHandle::Side { base: 1, .. })));
        // A 8-wide extent: the centre band itself shrinks to 2, from one side.
        let tiny = [
            ob(0, 192.0, 100.0, 8.0, 100.0),
            ob(1, 200.0, 100.0, 200.0, 100.0),
        ];
        let at = |d: f64| handle(&tiny, &[], 200.0 + d, 150.0, 1.0);
        assert!(matches!(at(2.0), Some(LayoutHandle::Boundary { .. })));
        assert!(!matches!(at(2.5), Some(LayoutHandle::Boundary { .. })));
        assert!(!matches!(at(-2.5), Some(LayoutHandle::Boundary { .. })));
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

    /// A line moves the two graphs the pointer is between and nobody else,
    /// even when other boxes have an edge on the same straight line. Witness:
    /// whole-line following restored.
    #[test]
    fn the_group_of_a_line_is_the_pair_only() {
        let mut boxes = quad().to_vec();
        boxes.push(ob(4, 100.0, 400.0, 100.0, 100.0));
        let group = boundary_group(&boxes, true, 0, 1);
        assert_eq!((group.low.clone(), group.high.clone()), (vec![0], vec![1]));
        assert_eq!(group.line, 200.0);
        let group = boundary_group(&boxes, true, 2, 3);
        assert_eq!((group.low, group.high), (vec![2], vec![3]));
        // The pointer picks the row.
        let row = |y| match handle(&quad(), &[], 200.0, y, 1.0) {
            Some(LayoutHandle::Boundary { low, high, .. }) => (low, high),
            other => panic!("a boundary, got {other:?}"),
        };
        assert_eq!(row(150.0), (0, 1));
        assert_eq!(row(250.0), (2, 3));
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
        boundary_group(boxes, vertical, 0, 1)
    }

    fn outer_right(frame: &FreeFrame) -> f64 {
        frame.left + frame.width + 10.0
    }

    /// A move writes both boxes of the pair from one line, leaves the other
    /// axis and the stacking key alone, and never touches a box that merely
    /// has an edge on the same line.
    #[test]
    fn dragging_the_line_moves_both_sides_and_keeps_the_other_axis() {
        let mut boxes = quad().to_vec();
        let group = line_of(&boxes, true);
        let (frames, _) = moved(&boxes, &group, 3.0, no_snap());
        assert_eq!(frames.len(), 2);
        boxes.truncate(2);
        let (frames, guides) = moved(&boxes, &group, 3.0, snap());
        assert!(guides.is_empty());
        assert_eq!(frames.len(), 2);
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
        let group = boundary_group(&boxes, true, 0, 1);
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

    /// The line stops where the narrower of the two boxes stops, in both
    /// directions and on both axes.
    #[test]
    fn the_line_stops_where_the_tighter_box_of_the_pair_stops() {
        // Low box 0 starts at 100 (min outer 56: the line stops at 156), high
        // box 1 ends at 300 (the line stops at 244).
        let vertical = vec![
            ob(0, 100.0, 100.0, 100.0, 100.0),
            ob(1, 200.0, 100.0, 100.0, 100.0),
        ];
        for vertical_line in [true, false] {
            let boxes = if vertical_line {
                vertical.clone()
            } else {
                transposed(&vertical)
            };
            let group = line_of(&boxes, vertical_line);
            assert_eq!(group.low.len() + group.high.len(), 2);
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
        let group = boundary_group(&boxes, true, 0, 1);
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
            let group = boundary_group(&boxes, true, 0, 1);
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

#[cfg(test)]
mod page_corner_tests {
    use super::*;

    fn boxed(base: usize, left: f64, top: f64, width: f64, height: f64) -> GraphBox {
        let gutters = (50.0, 10.0, 10.0, 30.0);
        GraphBox {
            base,
            outer: FreePage {
                left,
                top,
                width,
                height,
            },
            gutters,
            frame: FreeFrame {
                left: left + gutters.0,
                top: top + gutters.1,
                width: width - gutters.0 - gutters.2,
                height: height - gutters.1 - gutters.3,
                stack: 0,
            },
        }
    }

    fn no_snap_at_all() -> Snap {
        Snap {
            grid: None,
            threshold: 0.0,
            enabled: false,
        }
    }

    /// K1: a page corner pulled inward stops at the bounding box of the graphs
    /// (outer rectangles), on both sides; outward it grows freely. The boxes
    /// are not an output. Witness: no bounding box limit (the page goes to the
    /// 72 point minimum).
    #[test]
    fn a_page_corner_pulled_inward_stops_at_the_graphs() {
        let page = FreePage {
            left: 0.0,
            top: 0.0,
            width: 800.0,
            height: 400.0,
        };
        // The graphs occupy x 100..700, y 50..300.
        let boxes = [
            boxed(0, 100.0, 50.0, 300.0, 250.0),
            boxed(1, 400.0, 50.0, 300.0, 250.0),
        ];
        let snap = no_snap_at_all();
        let far = moved_page_corner(&boxes, page, (true, true), (-100_000.0, -100_000.0), snap);
        assert_eq!(
            (far.left, far.top, far.width, far.height),
            (0.0, 0.0, 700.0, 300.0)
        );
        let near = moved_page_corner(&boxes, page, (false, false), (100_000.0, 100_000.0), snap);
        assert_eq!(
            (
                near.left,
                near.top,
                near.left + near.width,
                near.top + near.height
            ),
            (100.0, 50.0, 800.0, 400.0)
        );
        let out = moved_page_corner(&boxes, page, (false, true), (-40.0, 25.0), snap);
        assert_eq!(
            (out.left, out.top, out.width, out.height),
            (-40.0, 0.0, 840.0, 425.0)
        );
        // A page already smaller than the graphs is not pushed further in.
        let small = FreePage {
            left: 0.0,
            top: 0.0,
            width: 500.0,
            height: 400.0,
        };
        let same = moved_page_corner(&boxes, small, (true, true), (-50.0, 0.0), snap);
        assert_eq!(same.width, 500.0);
    }

    /// K1: Fit Layout to Page maps the bounding box of the graphs to the page,
    /// one factor per axis; a closed padlock keeps one factor and the page
    /// corner; nothing to do gives `None`. Witness: the `uniform` branch
    /// removed.
    #[test]
    fn the_boxes_fill_the_page_and_the_padlock_keeps_their_proportions() {
        let boxes = [boxed(0, 100.0, 50.0, 300.0, 200.0)];
        let page = FreePage {
            left: 0.0,
            top: 0.0,
            width: 800.0,
            height: 500.0,
        };
        // The bounding box (100, 50, 300 x 200) fills the whole page.
        let frames = boxes_fitted_to_page(&boxes, page, false).expect("something to do");
        let scaled = frames[0].1;
        // Outer 300 x 200, bands 60 across and 40 down: frame = outer - bands.
        let (sx, sy) = (800.0 / 300.0, 500.0 / 200.0);
        assert!(
            (scaled.width - (300.0 * sx - 60.0)).abs() < 0.02,
            "{scaled:?}"
        );
        assert!(
            (scaled.height - (200.0 * sy - 40.0)).abs() < 0.02,
            "{scaled:?}"
        );
        // Padlock closed: the smaller factor (2.5) on both axes, from the
        // top-left corner of the page.
        let uniform = boxes_fitted_to_page(&boxes, page, true).expect("something to do");
        let u = uniform[0].1;
        assert!((u.width - (300.0 * sy - 60.0)).abs() < 0.02, "{u:?}");
        assert!((u.height - (200.0 * sy - 40.0)).abs() < 0.02, "{u:?}");
        // Already filling the page: nothing to do.
        let exact = page_fitted_to_layers(&boxes).expect("a box");
        assert!(boxes_fitted_to_page(&boxes, exact, false).is_none());
        assert!(boxes_fitted_to_page(&[], page, false).is_none());
    }
}
