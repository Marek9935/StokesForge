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
use stokesforge_render::{grid_line_at, LayoutReach, LineAxis};

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

/// Which edge band of `figure` the pointer is in (#503, corner rule): the left
/// and right bands are full-height and **win** in a corner, the top and bottom
/// bands only cover what lies between them. The right band wins over the left
/// one when both would hold the pointer. `None` outside every band and outside
/// the figure. Pure, so it is tested without a window.
pub fn edge_drop_side(pointer: Pos2, figure: Rect, band: f32) -> Option<CellSide> {
    if !figure.contains(pointer) {
        return None;
    }
    let band = fitted_band(figure, band);
    if figure.max.x - pointer.x < band {
        Some(CellSide::Right)
    } else if pointer.x - figure.min.x < band {
        Some(CellSide::Left)
    } else if figure.max.y - pointer.y < band {
        Some(CellSide::Below)
    } else if pointer.y - figure.min.y < band {
        Some(CellSide::Above)
    } else {
        None
    }
}

/// Where a drop at a pointer position puts its new cell: beside `panel`, on
/// `side` of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropPlace {
    /// The reference cell (its base layer).
    pub panel: usize,
    /// Which side of it the new cell goes.
    pub side: CellSide,
    /// The edge band the pointer is in; `None` for a drop on an internal line.
    pub band: Option<CellSide>,
}

impl GraphView {
    /// The drop zone under the egui pixel `pos`, in the **last** drawn figure:
    /// one of the four edge bands (the new cell goes beside the cell nearest the
    /// pointer, see [`edge_drop_side`]), else an internal line of a regular grid
    /// (#504: between the two cells of the pointer's row or column on either
    /// side of it). `moving` is the graph being moved, if any: it is never its
    /// own reference: a band uses the nearest **other** cell (a hole is never a
    /// cell), and a line next to it uses the cell on its other side with the
    /// opposite side. `None` when there is no such cell, as for everything else.
    pub fn drop_place(&self, pos: Pos2, moving: Option<usize>) -> Option<DropPlace> {
        let display = self.display_rect?;
        if let Some(band) = edge_drop_side(pos, display, EDGE_BAND_WIDTH) {
            let panel = self.nearest_cell_except(pos, moving)?;
            return Some(DropPlace {
                panel,
                side: band,
                band: Some(band),
            });
        }
        let (_, layout) = self.figure_memo.as_ref()?;
        let last = self.last.as_ref()?;
        let at = self.figure_position(pos)?;
        let reach = LayoutReach::standard(f64::from(self.display_scale()?));
        let line = grid_line_at(layout, f64::from(at.x), f64::from(at.y), reach)?;
        let occupant = |row: usize, column: usize| {
            last.panels
                .iter()
                .find(|p| p.overlay.is_none() && (p.row, p.column) == (row, column))
                .map(|p| p.index)
                .filter(|index| Some(*index) != moving)
        };
        let (before, after, near, far) = match line.axis {
            LineAxis::Column => (
                occupant(line.along, line.n),
                occupant(line.along, line.n + 1),
                CellSide::Right,
                CellSide::Left,
            ),
            LineAxis::Row => (
                occupant(line.n, line.along),
                occupant(line.n + 1, line.along),
                CellSide::Below,
                CellSide::Above,
            ),
        };
        let place = |panel, side| DropPlace {
            panel,
            side,
            band: None,
        };
        before
            .map(|panel| place(panel, near))
            .or_else(|| after.map(|panel| place(panel, far)))
    }

    /// The preview of a drop while the pointer drags (#503): the four bands
    /// pale, the one under the pointer filled, and a thick bar at the exact
    /// place the new cell would go (on the side of the reference cell that
    /// faces it, which for a drop between two graphs is the line itself).
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

/// The four corners of the band of `side`, as [`edge_drop_side`] splits them:
/// the left and right bands are full-height rectangles, the top and bottom ones
/// stop at them.
fn band_corners(figure: Rect, band: f32, side: CellSide) -> [Pos2; 4] {
    let (min, max) = (figure.min, figure.max);
    let (left, right) = (min.x + band, max.x - band);
    let p = egui::pos2;
    let (x0, y0, x1, y1) = match side {
        CellSide::Left => (min.x, min.y, left, max.y),
        CellSide::Right => (right, min.y, max.x, max.y),
        CellSide::Above => (left, min.y, right, min.y + band),
        CellSide::Below => (left, max.y - band, right, max.y),
    };
    [p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)]
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

    /// #503 (corner rule): each of the four bands gives its side, and in a
    /// corner the left or right band wins over the top or bottom one (all four
    /// corners); the interior and the outside give nothing.
    /// Witness: the side bands tested after the top and bottom ones (nearest
    /// edge) fails the corner assertions.
    #[test]
    fn the_four_edge_bands_and_the_side_bands_win_the_corners() {
        let figure = Rect::from_min_max(egui::pos2(100.0, 50.0), egui::pos2(500.0, 350.0));
        let band = 36.0;
        let side = |x: f32, y: f32| edge_drop_side(egui::pos2(x, y), figure, band);
        assert_eq!(side(300.0, 200.0), None, "interior");
        assert_eq!(side(480.0, 200.0), Some(CellSide::Right));
        assert_eq!(side(300.0, 340.0), Some(CellSide::Below));
        assert_eq!(side(110.0, 200.0), Some(CellSide::Left));
        assert_eq!(side(300.0, 60.0), Some(CellSide::Above));
        // Corners: the left or right band wins, whatever the nearest edge.
        for (x, y, expected) in [
            (480.0, 60.0, CellSide::Right),
            (490.0, 80.0, CellSide::Right),
            (490.0, 340.0, CellSide::Right),
            (490.0, 345.0, CellSide::Right),
            (120.0, 340.0, CellSide::Left),
            (110.0, 340.0, CellSide::Left),
            (110.0, 60.0, CellSide::Left),
            (130.0, 55.0, CellSide::Left),
        ] {
            assert_eq!(side(x, y), Some(expected), "corner at ({x}, {y})");
        }
        // Just inside the side bands, the top and bottom bands apply.
        assert_eq!(side(300.0, 60.0), Some(CellSide::Above));
        assert_eq!(side(300.0, 340.0), Some(CellSide::Below));
        assert_eq!(
            side(140.0, 60.0),
            Some(CellSide::Above),
            "past the left band"
        );
        assert_eq!(side(300.0, 20.0), None, "outside");
        assert_eq!(side(520.0, 200.0), None, "outside");
        assert_eq!(
            edge_drop_side(egui::pos2(480.0, 200.0), figure, 0.0),
            None,
            "a band of width 0 catches nothing"
        );
    }
}
