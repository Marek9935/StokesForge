//! Fabriques de test partagées par les tests unitaires du crate (`#[cfg(test)]`).

use stokesforge_plot::AxisScale;

use crate::pipeline::{AxisAreas, DrawnPanel};

/// Un calque tracé minimal : zone de tracé `(0, 0)`–`(100, 100)` px, cadré
/// sur `0..100` des deux côtés, en linéaire, sans axe secondaire. Les
/// champs qui comptent pour un test se règlent ensuite sur la valeur rendue.
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
