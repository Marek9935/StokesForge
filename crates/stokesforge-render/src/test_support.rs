//! Test factories shared by the crate's unit tests (`#[cfg(test)]`).

use stokesforge_plot::AxisScale;

use crate::pipeline::{AxisAreas, DrawnPanel};

/// A minimal drawn layer: plot area `(0, 0)`–`(100, 100)` px, framed on
/// `0..100` on both sides, linear, no secondary axis. The fields that matter
/// to a test are then set on the returned value.
pub(crate) fn panel(index: usize, row: usize, column: usize) -> DrawnPanel {
    DrawnPanel {
        index,
        overlay: None,
        row,
        column,
        x_range: (0.0, 100.0),
        y_range: (0.0, 100.0),
        y2_range: None,
        pixel_origin: (0, 0),
        pixel_size: (100, 100),
        cell_origin: (0, 0),
        cell_size: (100, 100),
        x_scale: AxisScale::Linear,
        y_scale: AxisScale::Linear,
        y2_scale: AxisScale::Linear,
        drawn_series: 0,
        broken_series: 0,
        unknown_series: 0,
        legend_entries: 0,
        decimated_points: 0,
        axis_areas: AxisAreas::default(),
        annotations: Vec::new(),
        shapes: Vec::new(),
        series_geometry: Vec::new(),
        texts: Vec::new(),
        legend_rows: Vec::new(),
    }
}
