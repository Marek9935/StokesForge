//! The edge bands of a figure (#357, #491): the right and bottom strips where
//! a drop makes a **new** column or row beside the cell nearest the pointer.
//! One implementation for the two gestures that use it, dropping workbook
//! columns on a graph (`stokesforge-app`) and moving a graph in the Page Layout
//! Mode (`layout_mode`). Screen only: nothing here enters the cache or the
//! export.

use egui::{Color32, Painter, Pos2, Rect, Stroke, StrokeKind};
use stokesforge_core::compose::CellSide;

/// Width, in points, of the right and bottom bands of a figure.
pub const EDGE_BAND_WIDTH: f32 = 36.0;

/// Which edge band of `figure` the pointer is in: the right band gives
/// [`CellSide::Right`], the bottom band [`CellSide::Below`], and the
/// bottom-right corner, in both, gives `Right`. `None` elsewhere. Pure, so it is
/// tested without a window.
pub fn edge_drop_side(pointer: Pos2, figure: Rect, band: f32) -> Option<CellSide> {
    if !figure.contains(pointer) {
        return None;
    }
    if pointer.x >= figure.max.x - band {
        Some(CellSide::Right)
    } else if pointer.y >= figure.max.y - band {
        Some(CellSide::Below)
    } else {
        None
    }
}

/// Outline of an edge band. Not red: red is reserved for errors.
pub fn edge_band_stroke() -> Stroke {
    Stroke::new(1.0, Color32::from_rgb(70, 150, 230))
}

/// Fill of the edge band under the pointer, see [`edge_band_stroke`].
pub fn edge_band_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(70, 150, 230, 60)
}

/// The two bands, outlined, the one under the pointer (`active`) filled.
pub fn paint_edge_bands(painter: &Painter, figure: Rect, active: Option<CellSide>) {
    let right = Rect::from_min_max(
        egui::pos2(figure.max.x - EDGE_BAND_WIDTH, figure.min.y),
        figure.max,
    );
    let below = Rect::from_min_max(
        egui::pos2(figure.min.x, figure.max.y - EDGE_BAND_WIDTH),
        egui::pos2(figure.max.x - EDGE_BAND_WIDTH, figure.max.y),
    );
    for (band, side) in [(right, CellSide::Right), (below, CellSide::Below)] {
        let fill = if active == Some(side) {
            edge_band_fill()
        } else {
            Color32::TRANSPARENT
        };
        painter.rect(band, 0.0, fill, edge_band_stroke(), StrokeKind::Inside);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #357: the right band gives a cell to the right, the bottom band a cell
    /// below, the bottom-right corner (in both) the right; the interior and
    /// the outside of the figure give nothing.
    #[test]
    fn the_edge_bands_of_the_figure_decide_where_the_new_cell_goes() {
        let figure = Rect::from_min_max(egui::pos2(100.0, 50.0), egui::pos2(500.0, 350.0));
        let band = 36.0;
        let side = |x: f32, y: f32| edge_drop_side(egui::pos2(x, y), figure, band);
        assert_eq!(side(300.0, 200.0), None, "interior");
        assert_eq!(side(480.0, 200.0), Some(CellSide::Right));
        assert_eq!(
            side(480.0, 60.0),
            Some(CellSide::Right),
            "top of the right band"
        );
        assert_eq!(side(300.0, 340.0), Some(CellSide::Below));
        assert_eq!(side(490.0, 340.0), Some(CellSide::Right), "corner: right");
        assert_eq!(side(300.0, 20.0), None, "outside");
        assert_eq!(side(520.0, 200.0), None, "outside");
        assert_eq!(
            edge_drop_side(egui::pos2(480.0, 200.0), figure, 0.0),
            None,
            "a band of width 0 catches nothing"
        );
    }
}
