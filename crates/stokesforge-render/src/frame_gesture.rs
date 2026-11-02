//! The geometry of the mouse gestures on the cell boundaries of a regular
//! grid (#426b), without egui: what a grabbed boundary is (a splitter between
//! two columns or rows, or an edge of the figure) and the frame sizes a drag
//! of it ends up with.
//!
//! Sizes are **frame (plot area) sizes in points**, as stored in
//! [`stokesforge_plot::PlotSpec::frame_widths`]; plain data in, plain data out.

use crate::hit::CellEdge;
use crate::pipeline::FigureLayout;
use stokesforge_plot::{MAX_FRAME_SIZE, MIN_FRAME_SIZE};

/// What a boundary of the grid is, along one axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grip {
    /// The boundary between column (row) `n` and `n + 1`: dragging it moves
    /// size from one to the other, the figure keeps its size.
    Splitter(usize),
    /// The right (bottom) edge of the figure: dragging it resizes the whole
    /// figure, every frame in proportion.
    FigureEdge,
}

/// The two parts of a grabbed cell boundary: a corner can carry both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridGrips {
    /// The vertical boundary (widths).
    pub column: Option<Grip>,
    /// The horizontal boundary (heights).
    pub row: Option<Grip>,
}

/// The grips of `edge` on a grid of `columns` x `rows`.
pub fn grid_grips(edge: &CellEdge, columns: usize, rows: usize) -> GridGrips {
    let grip = |index: Option<usize>, count: usize| {
        index.filter(|i| *i < count).map(|i| {
            if i + 1 < count {
                Grip::Splitter(i)
            } else {
                Grip::FigureEdge
            }
        })
    };
    GridGrips {
        column: grip(edge.column, columns),
        row: grip(edge.row, rows),
    }
}

fn bounded(size: f64) -> f64 {
    size.clamp(MIN_FRAME_SIZE, MAX_FRAME_SIZE)
}

/// The sizes of one axis after a drag of `delta` points: a splitter moves
/// `delta` from the next frame to this one (clamped so that both stay in
/// bounds, the sum unchanged); the figure edge scales every frame by the same
/// factor so that the figure grows by `delta` (`bands` is what the figure has
/// besides the frames). Rounded to whole points; a drag under half a point
/// returns `start` untouched.
fn resized_axis(start: &[f64], grip: Grip, bands: f64, delta: f64) -> Vec<f64> {
    let delta = if delta.is_finite() {
        delta.round()
    } else {
        0.0
    };
    if delta == 0.0 {
        return start.to_vec();
    }
    let mut sizes = start.to_vec();
    match grip {
        Grip::Splitter(n) => {
            let (Some(&a), Some(&b)) = (start.get(n), start.get(n + 1)) else {
                return sizes;
            };
            let low = (MIN_FRAME_SIZE - a).max(b - MAX_FRAME_SIZE);
            let high = (MAX_FRAME_SIZE - a).min(b - MIN_FRAME_SIZE);
            let moved = delta.min(high).max(low);
            sizes[n] = a + moved;
            sizes[n + 1] = b - moved;
        }
        Grip::FigureEdge => {
            let sum: f64 = start.iter().sum();
            if sum <= 0.0 {
                return sizes;
            }
            let target = sum + bands + delta;
            let factor = (target - bands) / sum;
            for size in &mut sizes {
                *size = bounded((*size * factor).round());
            }
        }
    }
    sizes
}

/// The frame widths and heights after dragging the boundary `grips` of
/// `layout` by `delta` points `(dx, dy)` from the sizes `start` the gesture
/// began with (a splitter keeps the figure size, the figure edge resizes it
/// as [`crate::pipeline::fit_figure_size`] does for a grid, without a probe).
pub fn resized_frames(
    layout: &FigureLayout,
    grips: &GridGrips,
    start: (&[f64], &[f64]),
    delta: (f64, f64),
) -> (Vec<f64>, Vec<f64>) {
    let bands_w = (f64::from(layout.size.0) - start.0.iter().sum::<f64>()).max(0.0);
    let bands_h = (f64::from(layout.size.1) - start.1.iter().sum::<f64>()).max(0.0);
    let widths = match grips.column {
        Some(grip) => resized_axis(start.0, grip, bands_w, delta.0),
        None => start.0.to_vec(),
    };
    let heights = match grips.row {
        Some(grip) => resized_axis(start.1, grip, bands_h, delta.1),
        None => start.1.to_vec(),
    };
    (widths, heights)
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_plot::LayerPlacement;

    fn layout(size: (u32, u32)) -> FigureLayout {
        FigureLayout {
            size,
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
            placement: LayerPlacement::Grid,
            free_cells: Vec::new(),
            free_origin: (0.0, 0.0),
        }
    }

    fn edge(column: Option<usize>, row: Option<usize>) -> CellEdge {
        CellEdge { column, row }
    }

    #[test]
    fn the_last_column_and_row_are_figure_edges_the_others_splitters() {
        let grips = grid_grips(&edge(Some(0), Some(1)), 2, 2);
        assert_eq!(grips.column, Some(Grip::Splitter(0)));
        assert_eq!(grips.row, Some(Grip::FigureEdge));
        let none = grid_grips(&edge(Some(5), None), 2, 2);
        assert_eq!(none.column, None);
    }

    #[test]
    fn a_splitter_keeps_the_sum_and_stops_at_the_minimum() {
        let lay = layout((800, 300));
        let grips = grid_grips(&edge(Some(0), None), 2, 1);
        let (w, h) = resized_frames(&lay, &grips, (&[300.0, 300.0], &[200.0]), (40.0, 99.0));
        assert_eq!(w, [340.0, 260.0]);
        assert_eq!(h, [200.0], "no row part, no change");
        let (w, _) = resized_frames(&lay, &grips, (&[300.0, 300.0], &[200.0]), (1000.0, 0.0));
        assert_eq!(w, [600.0 - MIN_FRAME_SIZE, MIN_FRAME_SIZE]);
        let (w, _) = resized_frames(&lay, &grips, (&[300.0, 300.0], &[200.0]), (-1000.0, 0.0));
        assert_eq!(w, [MIN_FRAME_SIZE, 600.0 - MIN_FRAME_SIZE]);
    }

    #[test]
    fn the_figure_edge_scales_every_frame_in_proportion() {
        let lay = layout((800, 300));
        let grips = grid_grips(&edge(Some(1), None), 2, 1);
        let (w, _) = resized_frames(&lay, &grips, (&[200.0, 400.0], &[200.0]), (60.0, 0.0));
        assert_eq!(w, [220.0, 440.0]);
        let (w, _) = resized_frames(&lay, &grips, (&[200.0, 400.0], &[200.0]), (-10000.0, 0.0));
        assert_eq!(w, [MIN_FRAME_SIZE, MIN_FRAME_SIZE]);
    }

    #[test]
    fn a_corner_carries_both_axes_and_a_zero_drag_changes_nothing() {
        let lay = layout((800, 330));
        let grips = grid_grips(&edge(Some(1), Some(0)), 2, 2);
        assert_eq!(grips.row, Some(Grip::Splitter(0)));
        let start = (&[300.0, 300.0][..], &[100.0, 100.0][..]);
        let (w, h) = resized_frames(&lay, &grips, start, (30.0, 20.0));
        assert_eq!(w, [315.0, 315.0]);
        assert_eq!(h, [120.0, 80.0]);
        let (w, h) = resized_frames(&lay, &grips, start, (0.2, -0.4));
        assert_eq!((w.as_slice(), h.as_slice()), (start.0, start.1));
    }
}
