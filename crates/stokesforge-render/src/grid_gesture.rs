//! The geometry of the Page Layout Mode on a regular grid (#426), without
//! egui: which internal boundary or corner of the grid is under a pixel, and
//! the frame sizes a drag of it ends up with.
//!
//! Positions are **figure pixels** of a [`FigureLayout`] (points, the title
//! band included); the screen layer converts with the display scale it froze at
//! the press. Only the **internal** boundaries between two columns or two rows
//! have a handle; the outer edges of the figure and its own outer corners have
//! none. The line of boundary `j` is read from the rounded breakpoints of the
//! layout (`column_breaks[j]`, `row_breaks[j] + title_band`), the same numbers
//! as [`FigureLayout::cell_rect`], never from a sum of frame sizes.
//!
//! A boundary is named by `n`: the one between column (row) `n` and `n + 1`.

use crate::free_gesture::MIN_PAGE_EXTENT;
use crate::free_gesture::{LayoutReach, BOUNDARY_SIDE_PX};
use crate::pipeline::FigureLayout;
use stokesforge_plot::{LayerPlacement, MAX_FIGURE_EXTENT, MAX_FRAME_SIZE, MIN_FRAME_SIZE};

/// What a pixel grabs on a grid: a boundary per axis, a corner carrying both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridHandle {
    /// The vertical boundary (between two columns), if grabbed.
    pub column: Option<usize>,
    /// The horizontal boundary (between two rows), if grabbed.
    pub row: Option<usize>,
    /// Grabbed by a corner square (else by the band of one line).
    pub corner: bool,
}

/// A graph corner that lies on at least one internal boundary, in figure
/// pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridCorner {
    /// Position across.
    pub x: f64,
    /// Position down, the title band included.
    pub y: f64,
    /// The internal vertical boundary it lies on, if any.
    pub column: Option<usize>,
    /// The internal horizontal boundary it lies on, if any.
    pub row: Option<usize>,
}

/// The lines of a grid layout: `(columns, rows, xs, ys)`, or `None` if the
/// layout is not a regular grid with at least one cell.
fn lines(layout: &FigureLayout) -> Option<(usize, usize, Vec<f64>, Vec<f64>)> {
    if layout.placement != LayerPlacement::Grid {
        return None;
    }
    let (cols, rows) = (
        layout.column_breaks.len().checked_sub(1)?,
        layout.row_breaks.len().checked_sub(1)?,
    );
    if cols == 0 || rows == 0 {
        return None;
    }
    let xs = layout.column_breaks.iter().map(|b| f64::from(*b)).collect();
    let band = f64::from(layout.title_band);
    let ys = layout
        .row_breaks
        .iter()
        .map(|b| f64::from(*b) + band)
        .collect();
    Some((cols, rows, xs, ys))
}

/// The meetings `(row line, column line)` of the graph corners of the
/// `occupied` cells that lie on at least one internal line, sorted, once each.
fn meetings(rows: usize, cols: usize, occupied: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut found: Vec<(usize, usize)> = Vec::new();
    for &(row, column) in occupied {
        if row >= rows || column >= cols {
            continue;
        }
        for j in [row, row + 1] {
            for i in [column, column + 1] {
                if (i > 0 && i < cols || j > 0 && j < rows) && !found.contains(&(j, i)) {
                    found.push((j, i));
                }
            }
        }
    }
    found.sort_unstable();
    found
}

/// The graph corners on at least one internal boundary, one per meeting of
/// lines, for the `occupied` cells `(row, column)` of the grid. A corner on one
/// internal boundary and on the figure edge carries that boundary only; the
/// figure's own outer corners are not listed.
pub fn grid_corners(layout: &FigureLayout, occupied: &[(usize, usize)]) -> Vec<GridCorner> {
    let Some((cols, rows, xs, ys)) = lines(layout) else {
        return Vec::new();
    };
    meetings(rows, cols, occupied)
        .into_iter()
        .map(|(j, i)| GridCorner {
            x: xs[i],
            y: ys[j],
            column: (i > 0 && i < cols).then(|| i - 1),
            row: (j > 0 && j < rows).then(|| j - 1),
        })
        .collect()
}

/// The grid cell `(row, column)` under the figure pixel `(x, y)`, holes
/// included (a cell no graph occupies is still a place a graph can be moved
/// to, #491), or `None` outside the grid, in the title band, or for a layout
/// that is not a regular grid. The extents are those of
/// [`FigureLayout::cell_rect`]: the outer cells, which tile the grid below the
/// title band.
pub fn grid_cell_at(layout: &FigureLayout, x: f64, y: f64) -> Option<(usize, usize)> {
    let (cols, rows, xs, ys) = lines(layout)?;
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    let find = |v: &[f64], at: f64, count: usize| (0..count).find(|i| at >= v[*i] && at < v[i + 1]);
    Some((find(&ys, y, rows)?, find(&xs, x, cols)?))
}

/// The handle under the figure pixel `(x, y)`, or `None`. A corner square
/// (the edge reach on both axes, shrunk inwards on small cells like every
/// inner reach, see [`LayoutReach`]) wins; else the whole band of an internal
/// line (up to [`BOUNDARY_SIDE_PX`] screen pixels, each side limited by a
/// quarter of its own column or row) grabs that line, and the nearest line wins
/// where two bands cross. A line is grabbed only where a graph of the
/// `occupied` cells `(row, column)` touches it.
pub fn grid_handle_at(
    layout: &FigureLayout,
    occupied: &[(usize, usize)],
    x: f64,
    y: f64,
    reach: LayoutReach,
) -> Option<GridHandle> {
    let (cols, rows, xs, ys) = lines(layout)?;
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    // The extents of the cells on each side of line `i` of `v`, `None` outside.
    let side = |v: &[f64], i: usize| -> (Option<f64>, Option<f64>) {
        (
            i.checked_sub(1).map(|p| v[i] - v[p]),
            v.get(i + 1).map(|n| *n - v[i]),
        )
    };
    let mut best: Option<(f64, GridCorner)> = None;
    for (j, i) in meetings(rows, cols, occupied) {
        let corner = GridCorner {
            x: xs[i],
            y: ys[j],
            column: (i > 0 && i < cols).then(|| i - 1),
            row: (j > 0 && j < rows).then(|| j - 1),
        };
        let (before_x, after_x) = side(&xs, i);
        let (before_y, after_y) = side(&ys, j);
        let (dx, dy) = (x - corner.x, y - corner.y);
        if reach.within_edge(dx, before_x, after_x) && reach.within_edge(dy, before_y, after_y) {
            let d = dx.hypot(dy);
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, corner));
            }
        }
    }
    if let Some((_, corner)) = best {
        return Some(GridHandle {
            column: corner.column,
            row: corner.row,
            corner: true,
        });
    }
    let scale = if reach.scale.is_finite() && reach.scale > 0.0 {
        reach.scale
    } else {
        1.0
    };
    // The cell index holding a coordinate (clamped to the grid).
    let index_of = |v: &[f64], at: f64, count: usize| -> Option<usize> {
        if at < v[0] || at > v[count] {
            return None;
        }
        Some((0..count).rev().find(|k| v[*k] <= at).unwrap_or(0))
    };
    let band = |v: &[f64], count: usize, at: f64, along: Option<usize>, is_column: bool| {
        let mut found: Option<(f64, usize)> = None;
        for i in 1..count {
            let (before, after) = side(v, i);
            let d = at - v[i];
            let extent = if d < 0.0 { before } else { after }.unwrap_or(0.0);
            let limit = reach.inner_px(BOUNDARY_SIDE_PX, extent);
            if d.abs() * scale > limit {
                continue;
            }
            let Some(k) = along else { continue };
            let touched = if is_column {
                occupied.contains(&(k, i - 1)) || occupied.contains(&(k, i))
            } else {
                occupied.contains(&(i - 1, k)) || occupied.contains(&(i, k))
            };
            if touched && found.is_none_or(|(b, _)| d.abs() < b) {
                found = Some((d.abs(), i - 1));
            }
        }
        found
    };
    let column = band(&xs, cols, x, index_of(&ys, y, rows), true);
    let row = band(&ys, rows, y, index_of(&xs, x, cols), false);
    match (column, row) {
        (Some((dc, n)), Some((dr, _))) if dc <= dr => Some(GridHandle {
            column: Some(n),
            row: None,
            corner: false,
        }),
        (_, Some((_, n))) => Some(GridHandle {
            column: None,
            row: Some(n),
            corner: false,
        }),
        (Some((_, n)), None) => Some(GridHandle {
            column: Some(n),
            row: None,
            corner: false,
        }),
        (None, None) => None,
    }
}

/// Which way an internal grid line runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineAxis {
    /// A vertical line, between two columns.
    Column,
    /// A horizontal line, between two rows.
    Row,
}

/// The internal line under a pixel (#504), with where along it the pixel is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridLine {
    /// Which way the line runs.
    pub axis: LineAxis,
    /// The line sits between column (row) `n` and `n + 1`.
    pub n: usize,
    /// The row (for a column line) or the column (for a row line) the pixel is
    /// in, along the line.
    pub along: usize,
}

/// The internal line of a regular grid under the figure pixel `(x, y)`, or
/// `None` (#504): the same reach as the resize handles ([`BOUNDARY_SIDE_PX`]
/// screen pixels on each side, each side limited by a quarter of its own column
/// or row), the nearest line where two bands cross (a column line on a tie). The
/// outer edges of the grid, the title band and anything outside the grid have no
/// line. Unlike [`grid_handle_at`] a line needs no graph beside it: whether a
/// drop there has a neighbour is the caller's question.
pub fn grid_line_at(layout: &FigureLayout, x: f64, y: f64, reach: LayoutReach) -> Option<GridLine> {
    let (cols, rows, xs, ys) = lines(layout)?;
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    if x < xs[0] || x >= xs[cols] || y < ys[0] || y >= ys[rows] {
        return None;
    }
    let scale = if reach.scale.is_finite() && reach.scale > 0.0 {
        reach.scale
    } else {
        1.0
    };
    // The cell index holding a coordinate.
    let index_of = |v: &[f64], at: f64, count: usize| (0..count).rev().find(|k| v[*k] <= at);
    let nearest = |v: &[f64], count: usize, at: f64| -> Option<(f64, usize)> {
        let mut found: Option<(f64, usize)> = None;
        for i in 1..count {
            let d = at - v[i];
            let extent = if d < 0.0 {
                v[i] - v[i - 1]
            } else {
                v[i + 1] - v[i]
            };
            if d.abs() * scale > reach.inner_px(BOUNDARY_SIDE_PX, extent) {
                continue;
            }
            if found.is_none_or(|(b, _)| d.abs() < b) {
                found = Some((d.abs(), i - 1));
            }
        }
        found
    };
    let column = nearest(&xs, cols, x);
    let row = nearest(&ys, rows, y);
    match (column, row) {
        (Some((dc, n)), Some((dr, _))) if dc <= dr => Some((LineAxis::Column, n)),
        (_, Some((_, n))) => Some((LineAxis::Row, n)),
        (Some((_, n)), None) => Some((LineAxis::Column, n)),
        (None, None) => None,
    }
    .and_then(|(axis, n)| {
        let along = match axis {
            LineAxis::Column => index_of(&ys, y, rows)?,
            LineAxis::Row => index_of(&xs, x, cols)?,
        };
        Some(GridLine { axis, n, along })
    })
}

/// The sizes of one axis after dragging the splitter `n` (between frame `n`
/// and `n + 1`) by `delta` points: the delta, rounded to whole points, moves
/// from the next frame to this one (or back), clamped so that both stay in
/// `[MIN_FRAME_SIZE, MAX_FRAME_SIZE]` and the **sum** is unchanged, so the
/// figure keeps its size. A frame already out of range is never pushed
/// further out. A `delta` that is not finite, or an `n` out of range, returns
/// `start` untouched.
pub fn splitter(start: &[f64], n: usize, delta: f64) -> Vec<f64> {
    let mut sizes = start.to_vec();
    let (Some(&a), Some(&b)) = (start.get(n), start.get(n + 1)) else {
        return sizes;
    };
    if !delta.is_finite() {
        return sizes;
    }
    let low = ((MIN_FRAME_SIZE - a).max(b - MAX_FRAME_SIZE)).min(0.0);
    let high = ((MAX_FRAME_SIZE - a).min(b - MIN_FRAME_SIZE)).max(0.0);
    let moved = delta.round().clamp(low, high);
    sizes[n] = a + moved;
    sizes[n + 1] = (a + b) - sizes[n];
    sizes
}

/// The frame widths and heights after dragging `handle` by `delta` points
/// `(dx, dy)` from the sizes the gesture began with: each grabbed boundary is a
/// [`splitter`] of its axis, so a corner moves the column and the row together.
pub fn resized_frames(
    handle: &GridHandle,
    widths: &[f64],
    heights: &[f64],
    delta: (f64, f64),
) -> (Vec<f64>, Vec<f64>) {
    let widths = match handle.column {
        Some(n) => splitter(widths, n, delta.0),
        None => widths.to_vec(),
    };
    let heights = match handle.row {
        Some(n) => splitter(heights, n, delta.1),
        None => heights.to_vec(),
    };
    (widths, heights)
}

/// The sizes of one axis after the figure's edge `far` (right or bottom, else
/// left or top) is pulled by `delta` points. `bands` is the part of the
/// figure's extent that is not plot area (axis bands, margins, titles): it
/// keeps its size, and **one** factor scales every frame so that the
/// extent follows the pointer. The factor is bounded so that every frame stays
/// in `[MIN_FRAME_SIZE, MAX_FRAME_SIZE]` and the extent in
/// `[MIN_PAGE_EXTENT, MAX_FIGURE_EXTENT]`: the frames keep their proportions
/// all the way to the stop (a per-frame clamp would not). Rounded to a
/// hundredth of a point. Untouched if `delta` is not finite or the frames are
/// empty or already out of range.
pub fn scaled_axis(sizes: &[f64], bands: f64, delta: f64, far: bool) -> Vec<f64> {
    let total: f64 = sizes.iter().sum();
    if !delta.is_finite() || !bands.is_finite() || sizes.is_empty() || total <= 0.0 {
        return sizes.to_vec();
    }
    let wanted = total + if far { delta } else { -delta };
    let (mut low, mut high) = (
        (MIN_PAGE_EXTENT - bands) / total,
        (MAX_FIGURE_EXTENT - bands) / total,
    );
    for size in sizes.iter().filter(|size| **size > 0.0) {
        low = low.max(MIN_FRAME_SIZE / size);
        high = high.min(MAX_FRAME_SIZE / size);
    }
    if low > high {
        return sizes.to_vec();
    }
    let factor = (wanted / total).clamp(low, high);
    sizes
        .iter()
        .map(|size| ((size * factor * 100.0).round() / 100.0).clamp(MIN_FRAME_SIZE, MAX_FRAME_SIZE))
        .collect()
}

/// The frame widths and heights after pulling the figure's corner
/// `(right, bottom)` by `delta`: [`scaled_axis`] on each axis, with the bands
/// `(across, down)` of the figure.
pub fn scaled_frames(
    widths: &[f64],
    heights: &[f64],
    bands: (f64, f64),
    corner: (bool, bool),
    delta: (f64, f64),
) -> (Vec<f64>, Vec<f64>) {
    (
        scaled_axis(widths, bands.0, delta.0, corner.0),
        scaled_axis(heights, bands.1, delta.1, corner.1),
    )
}

/// The drag `delta` of a figure corner `(right, bottom)` once the ratio lock is
/// closed (#426): the image `(width, height)` keeps `ratio` = height / width.
/// The axis that moved most **relatively** leads and the other follows, so the
/// result changes the extent along both axes by the amounts that keep the
/// ratio. A `delta` or `ratio` that is not usable is returned untouched.
pub fn locked_corner_delta(
    image: (f64, f64),
    corner: (bool, bool),
    delta: (f64, f64),
    ratio: f64,
) -> (f64, f64) {
    let usable = image.0 > 0.0
        && image.1 > 0.0
        && ratio.is_finite()
        && ratio > 0.0
        && delta.0.is_finite()
        && delta.1.is_finite();
    if !usable {
        return delta;
    }
    // Signed growth of each extent for this corner.
    let sign = |far: bool| if far { 1.0 } else { -1.0 };
    let (grow_x, grow_y) = (sign(corner.0) * delta.0, sign(corner.1) * delta.1);
    if grow_x.abs() / image.0 >= grow_y.abs() / image.1 {
        let wanted = (image.0 + grow_x).max(0.0) * ratio - image.1;
        (delta.0, sign(corner.1) * wanted)
    } else {
        let wanted = (image.1 + grow_y).max(0.0) / ratio - image.0;
        (sign(corner.0) * wanted, delta.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2x2 grid with a 40 point title band and unequal gutters: columns
    /// 0..300 and 300..500, rows 40..240 and 240..490.
    fn grid() -> FigureLayout {
        FigureLayout {
            size: (500, 490),
            title_band: 40,
            column_breaks: vec![0, 300, 500],
            row_breaks: vec![0, 200, 450],
            column_gutters: vec![(70, 10), (50, 20)],
            row_gutters: vec![(15, 45), (10, 30)],
            frame_widths: vec![220.0, 130.0],
            frame_heights: vec![140.0, 210.0],
            fixed_overflow: false,
            min_left: 0,
            min_top: 0,
            placement: LayerPlacement::Grid,
            free_cells: Vec::new(),
            free_origin: (0.0, 0.0),
        }
    }

    const ALL: [(usize, usize); 4] = [(0, 0), (0, 1), (1, 0), (1, 1)];

    fn at(layout: &FigureLayout, x: f64, y: f64) -> Option<GridHandle> {
        grid_handle_at(layout, &ALL, x, y, LayoutReach::standard(1.0))
    }

    #[test]
    fn a_pixel_names_its_cell_hole_or_not_and_nothing_in_the_title_band_or_outside() {
        let layout = grid();
        assert_eq!(grid_cell_at(&layout, 10.0, 50.0), Some((0, 0)));
        assert_eq!(grid_cell_at(&layout, 400.0, 100.0), Some((0, 1)));
        // Rows are 40..240 and 240..490: the row line is the bottom of cell_rect.
        assert_eq!(grid_cell_at(&layout, 10.0, 239.0), Some((0, 0)));
        assert_eq!(grid_cell_at(&layout, 10.0, 240.0), Some((1, 0)));
        assert_eq!(grid_cell_at(&layout, 499.0, 489.0), Some((1, 1)));
        assert_eq!(grid_cell_at(&layout, 10.0, 39.0), None, "title band");
        assert_eq!(
            grid_cell_at(&layout, 500.0, 100.0),
            None,
            "right of the grid"
        );
        assert_eq!(grid_cell_at(&layout, 10.0, 490.0), None, "below the grid");
        assert_eq!(grid_cell_at(&layout, -1.0, 100.0), None);
        assert_eq!(grid_cell_at(&layout, f64::NAN, 100.0), None);
        let mut free = grid();
        free.placement = LayerPlacement::Free;
        assert_eq!(grid_cell_at(&free, 10.0, 50.0), None, "free placement");
    }

    /// #504: a pixel near an internal line names it and where along it it is;
    /// a crossing gives the nearest line; the title band and the outer edges
    /// give nothing. Witness: the loop `1..count` of `nearest` turned into
    /// `0..count` (the outer edge becomes a line).
    #[test]
    fn a_pixel_near_an_internal_line_names_it_and_the_outer_edges_have_none() {
        let layout = grid();
        let reach = LayoutReach::standard(1.0);
        let line = |x: f64, y: f64| grid_line_at(&layout, x, y, reach);
        assert_eq!(
            line(310.0, 100.0),
            Some(GridLine {
                axis: LineAxis::Column,
                n: 0,
                along: 0
            })
        );
        assert_eq!(
            line(290.0, 300.0),
            Some(GridLine {
                axis: LineAxis::Column,
                n: 0,
                along: 1
            })
        );
        assert_eq!(
            line(100.0, 250.0),
            Some(GridLine {
                axis: LineAxis::Row,
                n: 0,
                along: 0
            })
        );
        assert_eq!(line(150.0, 100.0), None, "inside a cell");
        // Crossing at (300, 240): 5 points from the column line, 2 from the row.
        assert_eq!(line(305.0, 242.0).map(|l| l.axis), Some(LineAxis::Row));
        assert_eq!(line(302.0, 245.0).map(|l| l.axis), Some(LineAxis::Column));
        assert_eq!(line(5.0, 100.0), None, "left outer edge");
        assert_eq!(line(495.0, 100.0), None, "right outer edge");
        assert_eq!(line(100.0, 45.0), None, "top outer edge");
        assert_eq!(line(100.0, 485.0), None, "bottom outer edge");
        assert_eq!(line(300.0, 20.0), None, "title band");
        assert_eq!(line(f64::NAN, 100.0), None);
    }

    /// The row line is `row_breaks[j] + title_band`, which is the bottom of
    /// `cell_rect`. Witness: the title band forgotten (the line 40 points
    /// higher, so the pointer on the real line is out of every band).
    #[test]
    fn the_row_line_sits_under_the_title_band_where_cell_rect_ends() {
        let layout = grid();
        let (_, y, _, h) = layout.cell_rect(0, 0).expect("cell");
        let bottom = f64::from(y + h);
        assert_eq!(bottom, 240.0);
        // Mid-column, away from corners.
        let on_line = at(&layout, 150.0, bottom).expect("the line");
        assert_eq!((on_line.column, on_line.row), (None, Some(0)));
        assert!(!on_line.corner);
        // The whole band: 23 px on either side, none one title band lower.
        assert!(at(&layout, 150.0, bottom + 23.0).is_some());
        assert_eq!(at(&layout, 150.0, bottom + 40.0), None);
        assert_eq!(at(&layout, 150.0, bottom - 40.0), None);
        // And the column line at x = 300 over the same height.
        let column = at(&layout, 300.0, 120.0).expect("column line");
        assert_eq!((column.column, column.row), (Some(0), None));
    }

    /// The middle of a 2x2 carries both lines, the corner on the figure edge
    /// carries its one line, the figure's corners and outer edges nothing.
    /// Witness: the figure corners listed among the corners.
    #[test]
    fn corners_carry_every_internal_line_they_lie_on_and_outer_edges_none() {
        let layout = grid();
        let middle = at(&layout, 302.0, 238.0).expect("middle");
        assert_eq!(
            (middle.column, middle.row, middle.corner),
            (Some(0), Some(0), true)
        );
        let right = at(&layout, 498.0, 242.0).expect("right edge corner");
        assert_eq!((right.column, right.row), (None, Some(0)));
        let top = at(&layout, 301.0, 42.0).expect("top edge corner");
        assert_eq!((top.column, top.row), (Some(0), None));
        // The figure's own corners and outer edges: nothing.
        for (x, y) in [
            (2.0, 42.0),
            (498.0, 42.0),
            (2.0, 488.0),
            (498.0, 488.0),
            (0.0, 150.0),
            (500.0, 300.0),
            (150.0, 40.0),
            (150.0, 490.0),
        ] {
            assert_eq!(at(&layout, x, y), None, "({x}, {y})");
        }
        let corners = grid_corners(&layout, &ALL);
        assert_eq!(corners.len(), 5, "centre plus four edge meetings");
        assert!(corners
            .iter()
            .all(|c| c.column.is_some() || c.row.is_some()));
        // A single row has no internal row line: its corners carry columns only.
        let mut one = grid();
        one.row_breaks = vec![0, 450];
        one.row_gutters = vec![(15, 45)];
        let corners = grid_corners(&one, &[(0, 0), (0, 1)]);
        assert_eq!(corners.len(), 2);
        assert!(corners
            .iter()
            .all(|c| c.column == Some(0) && c.row.is_none()));
    }

    /// Where two bands cross away from a corner, the nearest line wins.
    #[test]
    fn the_nearest_line_wins_where_two_bands_cross() {
        let layout = grid();
        // 15 px from the column line, 20 from the row line: not a corner.
        let handle = at(&layout, 315.0, 220.0).expect("a band");
        assert_eq!((handle.column, handle.row), (Some(0), None));
        let handle = at(&layout, 285.0, 252.0).expect("a band");
        assert_eq!((handle.column, handle.row), (None, Some(0)));
    }

    /// A line is grabbed only beside a graph; a hole in the grid has none.
    #[test]
    fn a_line_beside_no_graph_has_no_band() {
        let layout = grid();
        let only_top_row = [(0, 0), (0, 1)];
        let reach = LayoutReach::standard(1.0);
        assert!(grid_handle_at(&layout, &only_top_row, 300.0, 120.0, reach).is_some());
        assert_eq!(
            grid_handle_at(&layout, &only_top_row, 300.0, 350.0, reach),
            None
        );
    }

    /// The bands are measured in screen pixels and shrink on a small column.
    #[test]
    fn the_band_follows_the_display_scale_and_shrinks_on_a_narrow_column() {
        let layout = grid();
        for scale in [1.0, 2.0, 0.5] {
            let reach = LayoutReach::standard(scale);
            let on = |px: f64| grid_handle_at(&layout, &ALL, 300.0 + px / scale, 120.0, reach);
            assert!(on(23.0).is_some() && on(-23.0).is_some(), "scale {scale}");
            assert!(on(23.1).is_none() && on(-23.1).is_none(), "scale {scale}");
        }
        // A 60 point first column: its side reaches 15 pixels.
        let mut narrow = grid();
        narrow.column_breaks = vec![0, 60, 500];
        let reach = LayoutReach::standard(1.0);
        let on = |dx: f64| grid_handle_at(&narrow, &ALL, 60.0 + dx, 120.0, reach);
        assert!(on(-15.0).is_some());
        assert!(on(-15.5).is_none());
        assert!(on(23.0).is_some());
    }

    /// A drag with a hostile delta keeps the sum and stops at the bounds.
    /// Witness: only the minimum clamped, or the sum recomputed from clamped
    /// values separately.
    #[test]
    fn a_splitter_keeps_the_sum_and_stops_at_36_and_2000() {
        let start = [300.0, 400.0, 500.0];
        let sum = |v: &[f64]| v.iter().sum::<f64>();
        for delta in [-1e9, -337.0, -1.0, 0.0, 1.0, 1e9] {
            let out = splitter(&start, 0, delta);
            assert_eq!(sum(&out), sum(&start), "delta {delta}");
            assert_eq!(out[2], 500.0);
            assert!(out.iter().all(|v| (36.0..=2000.0).contains(v)), "{out:?}");
        }
        assert_eq!(&splitter(&start, 0, -1e9)[..2], &[36.0, 664.0]);
        assert_eq!(&splitter(&start, 0, 1e9)[..2], &[664.0, 36.0]);
        // The upper bound: 1500 + 600 cannot become 2000 + 100 beyond 2000.
        let wide = [1500.0, 600.0];
        assert_eq!(splitter(&wide, 0, 1e9), vec![2000.0, 100.0]);
        let wide = [600.0, 1500.0];
        assert_eq!(splitter(&wide, 0, -1e9), vec![100.0, 2000.0]);
        // Not finite, or out of range: untouched.
        for delta in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(splitter(&start, 0, delta), start.to_vec());
        }
        assert_eq!(splitter(&start, 2, 10.0), start.to_vec());
        // Whole points.
        assert_eq!(&splitter(&start, 1, 10.4)[1..], &[410.0, 490.0]);
        // A frame already out of range is not pushed further out.
        assert_eq!(splitter(&[20.0, 300.0], 0, -5.0), vec![20.0, 300.0]);
        assert_eq!(splitter(&[20.0, 300.0], 0, 5.0), vec![25.0, 295.0]);
    }

    /// A corner moves the column and the row together.
    #[test]
    fn a_corner_drag_moves_both_axes() {
        let handle = GridHandle {
            column: Some(0),
            row: Some(0),
            corner: true,
        };
        let (w, h) = resized_frames(&handle, &[220.0, 130.0], &[140.0, 210.0], (30.0, -20.0));
        assert_eq!((w, h), (vec![250.0, 100.0], vec![120.0, 230.0]));
        let row_only = GridHandle {
            column: None,
            row: Some(0),
            corner: true,
        };
        let (w, h) = resized_frames(&row_only, &[220.0, 130.0], &[140.0, 210.0], (30.0, 10.0));
        assert_eq!((w, h), (vec![220.0, 130.0], vec![150.0, 200.0]));
    }

    /// One factor scales every frame, bands keep their size, near corners
    /// scale too, and the stop keeps the proportions. Witness: a per-frame
    /// clamp.
    #[test]
    fn the_figure_scales_every_frame_by_one_factor_and_stops_together() {
        let w = [200.0, 400.0];
        // Extent 600 + 100 bands: +70 makes 670 of frames, factor 670/600.
        let out = scaled_axis(&w, 100.0, 70.0, true);
        assert_eq!(out, vec![223.33, 446.67]);
        assert_eq!(
            scaled_axis(&w, 100.0, 70.0, false),
            scaled_axis(&w, 100.0, -70.0, true)
        );
        // Far inward: the smallest frame stops at 36, the other keeps 2:1.
        let out = scaled_axis(&w, 100.0, -1e9, true);
        assert_eq!(out, vec![36.0, 72.0]);
        let out = scaled_axis(&w, 100.0, 1e9, true);
        assert_eq!(out, vec![1000.0, 2000.0]);
        assert_eq!(scaled_axis(&w, 100.0, f64::NAN, true), w.to_vec());
    }

    /// With the lock closed the axis that moved most relatively leads, for
    /// every corner. Witness: the horizontal axis always leading.
    #[test]
    fn a_locked_corner_keeps_the_ratio_with_the_leading_axis() {
        // 400 x 200, ratio 0.5.
        let image = (400.0, 200.0);
        // Far corner, x leads: +40 wide (10 %) asks +20 high.
        assert_eq!(
            locked_corner_delta(image, (true, true), (40.0, 3.0), 0.5),
            (40.0, 20.0)
        );
        // Far corner, y leads: +30 high (15 %) asks +60 wide.
        assert_eq!(
            locked_corner_delta(image, (true, true), (5.0, 30.0), 0.5),
            (60.0, 30.0)
        );
        // Near corner: growing means moving left and up (negative deltas).
        assert_eq!(
            locked_corner_delta(image, (false, false), (-40.0, 0.0), 0.5),
            (-40.0, -20.0)
        );
        // Mixed corner: top-right, pulled inward along y leads.
        assert_eq!(
            locked_corner_delta(image, (true, false), (0.0, 40.0), 0.5),
            (-80.0, 40.0)
        );
        assert_eq!(
            locked_corner_delta(image, (true, true), (f64::NAN, 1.0), 0.5).1,
            1.0
        );
    }
}
