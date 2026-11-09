//! The drop zones of a figure (#357, #491, #503, #504): the four edge strips
//! where a drop makes a **new** column or row beside the cell nearest the
//! pointer, and the internal grid lines where it makes one **between** two
//! graphs. One implementation for the two gestures that use it, dropping
//! workbook columns on a graph (`stokesforge-app`) and moving a graph in the
//! Page Layout Mode (`layout_mode`): [`GraphView::drop_place`] decides,
//! [`GraphView::paint_drop_places`] shows. Screen only: nothing here enters the
//! cache or the export.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, StrokeKind};
use stokesforge_core::compose::CellSide;

use crate::GraphView;

/// Width, in points, of the four bands of a figure.
pub const EDGE_BAND_WIDTH: f32 = 36.0;

/// Thickness, in points, of the insertion bar shown where the new cell goes.
pub const INSERTION_BAR_WIDTH: f32 = 5.0;

/// The band width that fits the figure: at most half of its smaller side, so
/// the four bands never overlap more than their corners.
fn fitted_band(figure: Rect, band: f32) -> f32 {
    band.min(figure.width() / 2.0).min(figure.height() / 2.0)
}

/// Which edge band of `figure` the pointer is in (#503, corner rule G1): the
/// band of the **nearest** edge, so a corner is split by its diagonal; on the
/// diagonal itself the right, then the bottom, then the left edge wins. `None`
/// outside every band and outside the figure. Pure, so it is tested without a
/// window.
pub fn edge_drop_side(pointer: Pos2, figure: Rect, band: f32) -> Option<CellSide> {
    if !figure.contains(pointer) {
        return None;
    }
    let band = fitted_band(figure, band);
    [
        // The order is the tie-break: `min_by` keeps the first of equal distances.
        (CellSide::Right, figure.max.x - pointer.x),
        (CellSide::Below, figure.max.y - pointer.y),
        (CellSide::Left, pointer.x - figure.min.x),
        (CellSide::Above, pointer.y - figure.min.y),
    ]
    .into_iter()
    .filter(|(_, distance)| *distance < band)
    .min_by(|a, b| a.1.total_cmp(&b.1))
    .map(|(side, _)| side)
}

/// Where a drop at a pointer position puts its new cell: beside `panel`, on
/// `side` of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropPlace {
    /// The reference cell (its base layer).
    pub panel: usize,
    /// Which side of it the new cell goes.
    pub side: CellSide,
    /// The edge band the pointer is in.
    pub band: Option<CellSide>,
}

impl GraphView {
    /// The drop zone under the egui pixel `pos`, in the **last** drawn figure:
    /// one of the four edge bands (the new cell goes beside the cell nearest the
    /// pointer, see [`edge_drop_side`]). `moving` is the graph being moved, if
    /// any: it is never its own reference, so the band nearest to it gives
    /// `None`.
    pub fn drop_place(&self, pos: Pos2, moving: Option<usize>) -> Option<DropPlace> {
        let display = self.display_rect?;
        if let Some(band) = edge_drop_side(pos, display, EDGE_BAND_WIDTH) {
            let panel = self.nearest_cell(pos)?;
            return (Some(panel) != moving).then_some(DropPlace {
                panel,
                side: band,
                band: Some(band),
            });
        }
        None
    }

    /// The preview of a drop while the pointer drags (#503): the four bands
    /// pale, the one under the pointer filled, and a thick bar at the exact
    /// place the new cell would go (on the side of the reference cell that
    /// faces it).
    pub fn paint_drop_places(&self, painter: &Painter, place: Option<DropPlace>) {
        let Some(figure) = self.display_rect else {
            return;
        };
        let band = fitted_band(figure, EDGE_BAND_WIDTH);
        for side in [
            CellSide::Left,
            CellSide::Right,
            CellSide::Above,
            CellSide::Below,
        ] {
            let active = place.is_some_and(|place| place.band == Some(side));
            let fill = if active {
                edge_band_fill()
            } else {
                edge_band_pale_fill()
            };
            painter.add(Shape::convex_polygon(
                band_corners(figure, band, side).to_vec(),
                fill,
                edge_band_stroke(),
            ));
        }
        let Some(place) = place else { return };
        let Some(cell) = self.cell_rect_screen(place.panel) else {
            return;
        };
        let half = INSERTION_BAR_WIDTH / 2.0;
        let across = |at: f32, low: f32, high: f32| at.clamp(low + half, high - half);
        let bar = match place.side {
            CellSide::Right | CellSide::Left => {
                let at = if place.side == CellSide::Right {
                    cell.max.x
                } else {
                    cell.min.x
                };
                let x = across(at, figure.min.x, figure.max.x);
                Rect::from_min_max(
                    egui::pos2(x - half, cell.min.y),
                    egui::pos2(x + half, cell.max.y),
                )
            }
            CellSide::Below | CellSide::Above => {
                let at = if place.side == CellSide::Below {
                    cell.max.y
                } else {
                    cell.min.y
                };
                let y = across(at, figure.min.y, figure.max.y);
                Rect::from_min_max(
                    egui::pos2(cell.min.x, y - half),
                    egui::pos2(cell.max.x, y + half),
                )
            }
        };
        painter.rect(
            bar,
            1.0,
            edge_band_stroke().color,
            Stroke::NONE,
            StrokeKind::Inside,
        );
    }
}

/// The four corners of the band of `side`: a trapezoid, so that the corners of
/// the figure are split along their diagonal exactly as [`edge_drop_side`]
/// splits them.
fn band_corners(figure: Rect, band: f32, side: CellSide) -> [Pos2; 4] {
    let (min, max) = (figure.min, figure.max);
    let p = egui::pos2;
    match side {
        CellSide::Left => [
            p(min.x, min.y),
            p(min.x + band, min.y + band),
            p(min.x + band, max.y - band),
            p(min.x, max.y),
        ],
        CellSide::Right => [
            p(max.x, min.y),
            p(max.x, max.y),
            p(max.x - band, max.y - band),
            p(max.x - band, min.y + band),
        ],
        CellSide::Above => [
            p(min.x, min.y),
            p(max.x, min.y),
            p(max.x - band, min.y + band),
            p(min.x + band, min.y + band),
        ],
        CellSide::Below => [
            p(min.x, max.y),
            p(min.x + band, max.y - band),
            p(max.x - band, max.y - band),
            p(max.x, max.y),
        ],
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

/// Fill of a band that is not under the pointer: just visible.
pub fn edge_band_pale_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(70, 150, 230, 18)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #503 (G1): each of the four bands gives its side, and a corner goes to
    /// the nearest edge (split by its diagonal, the right and bottom edges
    /// winning the exact diagonal); the interior and the outside give nothing.
    /// Witness: the `Left` entry removed from `edge_drop_side`.
    #[test]
    fn the_four_edge_bands_and_their_corners_follow_the_nearest_edge() {
        let figure = Rect::from_min_max(egui::pos2(100.0, 50.0), egui::pos2(500.0, 350.0));
        let band = 36.0;
        let side = |x: f32, y: f32| edge_drop_side(egui::pos2(x, y), figure, band);
        assert_eq!(side(300.0, 200.0), None, "interior");
        assert_eq!(side(480.0, 200.0), Some(CellSide::Right));
        assert_eq!(side(300.0, 340.0), Some(CellSide::Below));
        assert_eq!(side(110.0, 200.0), Some(CellSide::Left));
        assert_eq!(side(300.0, 60.0), Some(CellSide::Above));
        // Corners: the nearest edge wins.
        assert_eq!(side(480.0, 60.0), Some(CellSide::Above), "top-right, top");
        assert_eq!(side(490.0, 80.0), Some(CellSide::Right), "top-right, right");
        assert_eq!(side(490.0, 340.0), Some(CellSide::Right), "diagonal");
        assert_eq!(side(120.0, 340.0), Some(CellSide::Below), "bottom-left");
        assert_eq!(
            side(110.0, 340.0),
            Some(CellSide::Below),
            "bottom-left diagonal"
        );
        assert_eq!(side(110.0, 60.0), Some(CellSide::Left), "top-left diagonal");
        assert_eq!(side(110.0, 70.0), Some(CellSide::Left), "top-left, left");
        assert_eq!(side(130.0, 60.0), Some(CellSide::Above), "top-left, top");
        assert_eq!(side(300.0, 20.0), None, "outside");
        assert_eq!(side(520.0, 200.0), None, "outside");
        assert_eq!(
            edge_drop_side(egui::pos2(480.0, 200.0), figure, 0.0),
            None,
            "a band of width 0 catches nothing"
        );
    }
}
