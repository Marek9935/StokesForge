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
use stokesforge_plot::{FreeFrame, LayerPlacement, MAX_FRAME_SIZE, MIN_FRAME_SIZE};

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
}
