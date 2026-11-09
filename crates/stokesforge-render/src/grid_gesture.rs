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
use stokesforge_plot::{
    GridSlack, LayerPlacement, MAX_FIGURE_EXTENT, MAX_FRAME_SIZE, MIN_FRAME_SIZE,
};

/// A side of the grid, for its outer edges (#505).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeSide {
    /// The left edge of the first column.
    Left,
    /// The top edge of the first row.
    Top,
    /// The right edge of the last column.
    Right,
    /// The bottom edge of the last row.
    Bottom,
}

/// The outer edge of a column or row of a grid (#505): the side it lies on and
/// the index of the column (left, right) or row (top, bottom) it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OuterEdge {
    /// The side of the grid.
    pub side: EdgeSide,
    /// Column index for `Left` / `Right`, row index for `Top` / `Bottom`.
    pub index: usize,
}

/// What a pixel grabs on a grid: a boundary per axis, a corner carrying both,
/// or an outer edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridHandle {
    /// The outer edge of one column or row, if grabbed (then `column` and `row`
    /// are `None` and `corner` is false).
    pub edge: Option<OuterEdge>,
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
    let mut xs: Vec<f64> = layout.column_breaks.iter().map(|b| f64::from(*b)).collect();
    let band = f64::from(layout.title_band);
    let mut ys: Vec<f64> = layout
        .row_breaks
        .iter()
        .map(|b| f64::from(*b) + band)
        .collect();
    // #505: the grid stops where the slack starts.
    let [left, top, right, bottom] = layout.grid_slack.map(f64::from);
    xs[0] += left;
    *xs.last_mut()? -= right;
    ys[0] += top;
    *ys.last_mut()? -= bottom;
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
            edge: None,
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
    // #505: the four outer edges, over a graph. Inside the grid the whole band
    // reach applies; on the slack side it is limited by the slack itself (the
    // L corners own the outside of the image).
    let [slack_left, slack_top, slack_right, slack_bottom] = layout.grid_slack.map(f64::from);
    let outer =
        |v: &[f64], at: f64, along: Option<usize>, is_column: bool, far: bool, slack: f64| {
            let line = if far { v[v.len() - 1] } else { v[0] };
            let d = at - line;
            let inside = if far { d <= 0.0 } else { d >= 0.0 };
            let extent = if inside {
                let k = if far { v.len() - 2 } else { 0 };
                v[k + 1] - v[k]
            } else {
                slack
            };
            let limit = reach.inner_px(BOUNDARY_SIDE_PX, extent);
            if d.abs() * scale > limit {
                return None;
            }
            let k = along?;
            let cell = match (is_column, far) {
                (true, false) => (k, 0),
                (true, true) => (k, cols - 1),
                (false, false) => (0, k),
                (false, true) => (rows - 1, k),
            };
            occupied.contains(&cell).then_some(d.abs())
        };
    let along_row = index_of(&ys, y, rows);
    let along_column = index_of(&xs, x, cols);
    let edges = [
        (
            EdgeSide::Left,
            0,
            outer(&xs, x, along_row, true, false, slack_left),
        ),
        (
            EdgeSide::Right,
            cols - 1,
            outer(&xs, x, along_row, true, true, slack_right),
        ),
        (
            EdgeSide::Top,
            0,
            outer(&ys, y, along_column, false, false, slack_top),
        ),
        (
            EdgeSide::Bottom,
            rows - 1,
            outer(&ys, y, along_column, false, true, slack_bottom),
        ),
    ];
    let nearest_edge = edges
        .iter()
        .filter_map(|(side, index, d)| d.map(|d| (d, *side, *index)))
        .min_by(|a, b| a.0.total_cmp(&b.0));
    let inner = match (column, row) {
        (Some((dc, n)), Some((dr, _))) if dc <= dr => Some((dc, Some(n), None)),
        (_, Some((dr, n))) => Some((dr, None, Some(n))),
        (Some((dc, n)), None) => Some((dc, Some(n), None)),
        (None, None) => None,
    };
    match (inner, nearest_edge) {
        (Some((di, _, _)), Some((de, side, index))) if de < di => Some(edge_handle(side, index)),
        (Some((_, column, row)), _) => Some(GridHandle {
            edge: None,
            column,
            row,
            corner: false,
        }),
        (None, Some((_, side, index))) => Some(edge_handle(side, index)),
        (None, None) => None,
    }
}

/// The handle of one outer edge.
fn edge_handle(side: EdgeSide, index: usize) -> GridHandle {
    GridHandle {
        edge: Some(OuterEdge { side, index }),
        column: None,
        row: None,
        corner: false,
    }
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

/// The frames and the slack after pulling the outer edge `edge` of the grid by
/// `delta` points `(dx, dy)` from the sizes the gesture began with (#505).
///
/// **Only the row or column of that edge changes.** Pulled inward by `p`, its
/// frame shrinks by `p` and the slack of that side grows by `p`: frame plus
/// slack is constant, so the page keeps its size. Pulled outward by `q`, the
/// frame grows by `q`; the slack on that side is eaten first, then the page
/// grows by what is left. The movement is rounded to whole points and bounded
/// so that the frame stays in `[MIN_FRAME_SIZE, MAX_FRAME_SIZE]`. A `delta`
/// that is not finite, or an `index` out of range, returns the start untouched.
pub fn resized_edge(
    edge: OuterEdge,
    widths: &[f64],
    heights: &[f64],
    slack: GridSlack,
    delta: (f64, f64),
) -> (Vec<f64>, Vec<f64>, GridSlack) {
    let untouched = || (widths.to_vec(), heights.to_vec(), slack);
    let (sizes, outward) = match edge.side {
        EdgeSide::Left => (widths, -delta.0),
        EdgeSide::Right => (widths, delta.0),
        EdgeSide::Top => (heights, -delta.1),
        EdgeSide::Bottom => (heights, delta.1),
    };
    let Some(&frame) = sizes.get(edge.index) else {
        return untouched();
    };
    if !outward.is_finite() {
        return untouched();
    }
    // A frame already out of range is never pushed further out.
    let low = (MIN_FRAME_SIZE - frame).min(0.0);
    let high = (MAX_FRAME_SIZE - frame).max(0.0);
    let moved = outward.round().clamp(low, high);
    let side_slack = match edge.side {
        EdgeSide::Left => slack.left,
        EdgeSide::Top => slack.top,
        EdgeSide::Right => slack.right,
        EdgeSide::Bottom => slack.bottom,
    };
    let new_slack = (side_slack - moved).max(0.0);
    let mut sizes = sizes.to_vec();
    sizes[edge.index] = frame + moved;
    let mut slack = slack;
    match edge.side {
        EdgeSide::Left => slack.left = new_slack,
        EdgeSide::Top => slack.top = new_slack,
        EdgeSide::Right => slack.right = new_slack,
        EdgeSide::Bottom => slack.bottom = new_slack,
    }
    match edge.side {
        EdgeSide::Left | EdgeSide::Right => (sizes, heights.to_vec(), slack),
        EdgeSide::Top | EdgeSide::Bottom => (widths.to_vec(), sizes, slack),
    }
}

/// The frames and the slack after dragging an internal line **alone**, with
/// Shift held at the press (#505, G9): the row above (column left of) each
/// grabbed line takes the whole change, every other row (column) keeps its
/// size, and the empty space on the far edge (bottom, right) absorbs the
/// difference exactly as for [`resized_edge`] of that far edge: the slack
/// shrinks or grows first, the page grows only when the slack is used up.
/// A corner holding a column and a row line applies the rule to both axes.
pub fn resized_line_alone(
    handle: &GridHandle,
    widths: &[f64],
    heights: &[f64],
    slack: GridSlack,
    delta: (f64, f64),
) -> (Vec<f64>, Vec<f64>, GridSlack) {
    let mut out = (widths.to_vec(), heights.to_vec(), slack);
    if let Some(index) = handle.column {
        let edge = OuterEdge {
            side: EdgeSide::Right,
            index,
        };
        out = resized_edge(edge, &out.0, &out.1, out.2, (delta.0, 0.0));
    }
    if let Some(index) = handle.row {
        let edge = OuterEdge {
            side: EdgeSide::Bottom,
            index,
        };
        out = resized_edge(edge, &out.0, &out.1, out.2, (0.0, delta.1));
    }
    out
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
            grid_slack: [0; 4],
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
    /// carries its one line, the figure's corners are not listed among the
    /// corners and its outer edges carry an edge, never a line (#505).
    /// Witness: the figure corners listed among the corners.
    #[test]
    fn corners_carry_every_internal_line_they_lie_on_and_outer_edges_no_line() {
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
        // The figure's own corners and outer edges: an edge, no internal line.
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
            let handle = at(&layout, x, y).unwrap_or_else(|| panic!("({x}, {y})"));
            assert!(handle.edge.is_some(), "({x}, {y})");
            assert_eq!(
                (handle.column, handle.row, handle.corner),
                (None, None, false)
            );
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

    fn edge_at(
        layout: &FigureLayout,
        occupied: &[(usize, usize)],
        x: f64,
        y: f64,
    ) -> Option<OuterEdge> {
        grid_handle_at(layout, occupied, x, y, LayoutReach::standard(1.0)).and_then(|h| h.edge)
    }

    /// #505: the bottom edge over an occupied cell names the last row; over a
    /// hole of the last row, nothing. Witness: no outer edge at all; the
    /// occupied check dropped.
    #[test]
    fn an_outer_edge_is_grabbed_over_a_graph_only() {
        let layout = grid();
        let bottom = edge_at(&layout, &ALL, 100.0, 489.0).expect("bottom edge");
        assert_eq!(
            bottom,
            OuterEdge {
                side: EdgeSide::Bottom,
                index: 1
            }
        );
        let right = edge_at(&layout, &ALL, 499.0, 300.0).expect("right edge");
        assert_eq!(
            right,
            OuterEdge {
                side: EdgeSide::Right,
                index: 1
            }
        );
        let top = edge_at(&layout, &ALL, 400.0, 41.0).expect("top edge");
        assert_eq!(
            top,
            OuterEdge {
                side: EdgeSide::Top,
                index: 0
            }
        );
        let left = edge_at(&layout, &ALL, 1.0, 100.0).expect("left edge");
        assert_eq!(
            left,
            OuterEdge {
                side: EdgeSide::Left,
                index: 0
            }
        );
        // A hole at (1, 1): nothing on the bottom edge under it.
        let holey = [(0, 0), (0, 1), (1, 0)];
        assert_eq!(edge_at(&layout, &holey, 400.0, 489.0), None);
        assert!(edge_at(&layout, &holey, 100.0, 489.0).is_some());
        // Far from every edge: nothing.
        assert_eq!(edge_at(&layout, &ALL, 100.0, 300.0), None);
    }

    /// #505: with slack on the bottom the grid stops above the image edge; the
    /// edge is grabbed at the outline, and not at the image edge.
    #[test]
    fn the_outer_edge_sits_at_the_outline_when_there_is_slack() {
        let mut layout = grid();
        layout.grid_slack = [0, 0, 0, 60];
        assert_eq!(layout.grid_outline(), Some((0, 40, 500, 430)));
        let at_outline = edge_at(&layout, &ALL, 100.0, 430.0).expect("outline");
        assert_eq!(at_outline.side, EdgeSide::Bottom);
        assert_eq!(edge_at(&layout, &ALL, 100.0, 489.0), None);
        assert_eq!(grid_cell_at(&layout, 100.0, 450.0), None, "in the slack");
    }

    /// #505: the bottom edge pulled 50 up keeps the sum of frame and slack.
    /// Witness: `scaled_axis` instead (every frame would move).
    #[test]
    fn an_outer_edge_pulled_inward_shrinks_one_frame_and_makes_slack() {
        let edge = OuterEdge {
            side: EdgeSide::Bottom,
            index: 1,
        };
        let (w, h, slack) = resized_edge(
            edge,
            &[220.0, 130.0],
            &[140.0, 210.0],
            GridSlack::default(),
            (0.0, -50.0),
        );
        assert_eq!((w, h), (vec![220.0, 130.0], vec![140.0, 160.0]));
        assert_eq!(
            slack,
            GridSlack {
                bottom: 50.0,
                ..GridSlack::default()
            }
        );
        // The top edge: pulling down shrinks row 0 and makes slack on top.
        let top = OuterEdge {
            side: EdgeSide::Top,
            index: 0,
        };
        let (_, h, slack) = resized_edge(
            top,
            &[220.0, 130.0],
            &[140.0, 210.0],
            GridSlack::default(),
            (0.0, 40.0),
        );
        assert_eq!(h, vec![100.0, 210.0]);
        assert_eq!(slack.top, 40.0);
    }

    /// #505 G9: Shift on an internal line changes only the row before it; the
    /// bottom slack absorbs the difference; outward past the slack the figure
    /// grows. Witness: `splitter` instead (the next row would trade).
    #[test]
    fn a_line_dragged_alone_changes_one_row_and_the_slack_absorbs() {
        let row = GridHandle {
            edge: None,
            column: None,
            row: Some(0),
            corner: false,
        };
        let heights = [100.0, 100.0, 100.0];
        let (w, h, slack) =
            resized_line_alone(&row, &[200.0], &heights, GridSlack::default(), (0.0, -30.0));
        assert_eq!((w, h), (vec![200.0], vec![70.0, 100.0, 100.0]));
        assert_eq!(slack.bottom, 30.0);
        // Back out beyond the slack: the row grows, the slack is eaten, the
        // figure grows by the rest.
        let start = GridSlack {
            bottom: 10.0,
            ..GridSlack::default()
        };
        let (_, h, slack) = resized_line_alone(&row, &[200.0], &heights, start, (0.0, 25.0));
        assert_eq!(h, vec![125.0, 100.0, 100.0]);
        assert_eq!(slack.bottom, 0.0);
        // A column line takes the same rule on the right.
        let column = GridHandle {
            column: Some(0),
            row: None,
            ..row
        };
        let (w, h, slack) = resized_line_alone(
            &column,
            &[100.0, 100.0],
            &[50.0],
            GridSlack::default(),
            (-20.0, 0.0),
        );
        assert_eq!((w, h), (vec![80.0, 100.0], vec![50.0]));
        assert_eq!(slack.right, 20.0);
    }

    /// #505: pulled outward, the slack goes first, then the frame keeps growing
    /// and so does the page. Witness: a negative slack allowed.
    #[test]
    fn an_outer_edge_pulled_outward_eats_the_slack_then_grows_the_figure() {
        let edge = OuterEdge {
            side: EdgeSide::Bottom,
            index: 1,
        };
        let start = GridSlack {
            bottom: 50.0,
            ..GridSlack::default()
        };
        let (_, h, slack) =
            resized_edge(edge, &[220.0, 130.0], &[140.0, 160.0], start, (0.0, 80.0));
        assert_eq!(h, vec![140.0, 240.0]);
        assert_eq!(slack.bottom, 0.0);
        // Frame + slack changed by 30: the figure grew by 30.
        assert_eq!(h[1] + slack.bottom - (160.0 + 50.0), 30.0);
        // Within the slack the figure keeps its size.
        let (_, h, slack) =
            resized_edge(edge, &[220.0, 130.0], &[140.0, 160.0], start, (0.0, 20.0));
        assert_eq!((h[1], slack.bottom), (180.0, 30.0));
    }

    /// #505: bad input and the frame bounds.
    #[test]
    fn an_outer_edge_drag_is_bounded_and_ignores_garbage() {
        let edge = OuterEdge {
            side: EdgeSide::Right,
            index: 0,
        };
        let none = GridSlack::default();
        let (w, _, slack) = resized_edge(edge, &[200.0, 100.0], &[100.0], none, (-1e9, 0.0));
        assert_eq!(w[0], MIN_FRAME_SIZE);
        assert_eq!(slack.right, 200.0 - MIN_FRAME_SIZE);
        let (w, _, _) = resized_edge(edge, &[200.0, 100.0], &[100.0], none, (1e9, 0.0));
        assert_eq!(w[0], MAX_FRAME_SIZE);
        let same = resized_edge(edge, &[200.0], &[100.0], none, (f64::NAN, 0.0));
        assert_eq!(same, (vec![200.0], vec![100.0], none));
        let far = OuterEdge {
            side: EdgeSide::Bottom,
            index: 9,
        };
        assert_eq!(
            resized_edge(far, &[200.0], &[100.0], none, (0.0, 5.0)),
            (vec![200.0], vec![100.0], none)
        );
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
            edge: None,
            column: Some(0),
            row: Some(0),
            corner: true,
        };
        let (w, h) = resized_frames(&handle, &[220.0, 130.0], &[140.0, 210.0], (30.0, -20.0));
        assert_eq!((w, h), (vec![250.0, 100.0], vec![120.0, 230.0]));
        let row_only = GridHandle {
            edge: None,
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
