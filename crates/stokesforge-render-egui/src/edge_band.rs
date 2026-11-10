//! The drop zones of a figure (#357, #491, #503, #504): the four edge strips
//! where a drop makes a **new** column or row beside the cell nearest the
//! pointer, and the internal grid lines where it makes one **between** two
//! graphs. One implementation for the two gestures that use it, dropping
//! workbook columns on a graph (`stokesforge-app`) and moving a graph in the
//! Page Layout Mode (`layout_mode`): [`GraphView::drop_place`] decides,
//! [`GraphView::paint_drop_places`] shows. Screen only: nothing here enters the
//! cache or the export.

use egui::{Color32, Painter, Pos2, Rect, Stroke, StrokeKind};
use stokesforge_core::compose::CellSide;
use stokesforge_plot::LayerPlacement;
use stokesforge_render::{grid_line_at, LayoutReach, LineAxis};

use crate::GraphView;

/// Width, in points, of the four bands of a figure: the hit zone **and** what
/// is drawn. A constant of the screen, not of the figure: zooming the data or
/// shrinking the figure to fit its window does not move it (#503).
pub const EDGE_BAND_WIDTH: f32 = 12.0;

/// Floor, in physical screen pixels, of the hit zone of a band: on a screen
/// that lays out below one pixel per point, [`edge_band_width`] widens the
/// band in points so it never drops under this many pixels.
pub const MIN_EDGE_BAND_PX: f32 = 8.0;

/// Gap, in points, between two segments of a band, and between the top or
/// bottom band and the side bands at the corners (drawing only).
const SEGMENT_GAP: f32 = 3.0;

/// Thickness, in points, of the insertion bar shown between two graphs.
pub const INSERTION_BAR_WIDTH: f32 = 4.0;

/// The width of the bands in points at `pixels_per_point`: [`EDGE_BAND_WIDTH`],
/// at least [`MIN_EDGE_BAND_PX`] physical pixels.
pub fn edge_band_width(pixels_per_point: f32) -> f32 {
    if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
        EDGE_BAND_WIDTH.max(MIN_EDGE_BAND_PX / pixels_per_point)
    } else {
        EDGE_BAND_WIDTH
    }
}

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
    /// Index of the segment of that band the pointer is in (see
    /// [`BandSegment`]); `0` for a drop on an internal line.
    pub segment: usize,
}

/// One segment of an edge band: the part of the band beside one row (left and
/// right bands) or one column (top and bottom bands) of a regular grid, which
/// is the place of one reference cell. A figure with free placement has a
/// single segment per band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandSegment {
    /// The band it belongs to.
    pub side: CellSide,
    /// Its hit zone: the segments of a band tile it with no gap.
    pub rect: Rect,
    /// The reference cell of a drop in it; `None` in free placement, where it
    /// is the cell nearest the pointer.
    pub reference: Option<usize>,
}

impl BandSegment {
    /// Distance from `pos` to the segment along the band (0 inside it).
    fn along_distance(&self, pos: Pos2) -> f32 {
        match self.side {
            CellSide::Left | CellSide::Right => {
                (self.rect.min.y - pos.y).max(pos.y - self.rect.max.y)
            }
            CellSide::Above | CellSide::Below => {
                (self.rect.min.x - pos.x).max(pos.x - self.rect.max.x)
            }
        }
        .max(0.0)
    }
}

impl GraphView {
    /// The band width in points on this view's screen, see [`edge_band_width`].
    pub fn edge_band_width(&self) -> f32 {
        edge_band_width(self.cache.key().map_or(1.0, |key| key.pixels_per_point))
    }

    /// The segments of the band of `side` in the last drawn figure, in order
    /// along the band. The one source of both the drawing and the drop: a drop
    /// in the band takes the reference of the segment it lies in
    /// ([`drop_place`](Self::drop_place)). On a regular grid there is one per
    /// row or column, whose reference is the nearest cell (other than `moving`)
    /// to its centre; a segment with no such cell is left out.
    pub fn band_segments(&self, side: CellSide, moving: Option<usize>) -> Vec<BandSegment> {
        let Some(figure) = self.display_rect else {
            return Vec::new();
        };
        let band = fitted_band(figure, self.edge_band_width());
        let zone = band_rect(figure, band, side);
        let vertical = matches!(side, CellSide::Left | CellSide::Right);
        let (low, high) = if vertical {
            (zone.min.y, zone.max.y)
        } else {
            (zone.min.x, zone.max.x)
        };
        let regular = self
            .figure_memo
            .as_ref()
            .zip(self.display_scale())
            .filter(|((_, layout), _)| layout.placement == LayerPlacement::Grid);
        let Some(((_, layout), scale)) = regular else {
            return vec![BandSegment {
                side,
                rect: zone,
                reference: None,
            }];
        };
        let breaks: Vec<f32> = if vertical {
            let title = layout.title_band;
            layout
                .row_breaks
                .iter()
                .map(|b| figure.min.y + (b + title) as f32 * scale)
                .collect()
        } else {
            layout
                .column_breaks
                .iter()
                .map(|b| figure.min.x + *b as f32 * scale)
                .collect()
        };
        let count = breaks.len().saturating_sub(1);
        let mut segments = Vec::new();
        for k in 0..count {
            // The first and last cut stretch to the figure's border (the title
            // band lies above the first row).
            let start = if k == 0 { low } else { breaks[k] }.max(low);
            let end = if k + 1 == count { high } else { breaks[k + 1] }.min(high);
            if end - start < 0.5 {
                continue;
            }
            let rect = if vertical {
                Rect::from_min_max(egui::pos2(zone.min.x, start), egui::pos2(zone.max.x, end))
            } else {
                Rect::from_min_max(egui::pos2(start, zone.min.y), egui::pos2(end, zone.max.y))
            };
            if let Some(reference) = self.nearest_cell_except(rect.center(), moving) {
                segments.push(BandSegment {
                    side,
                    rect,
                    reference: Some(reference),
                });
            }
        }
        segments
    }

    /// The drop zone under the egui pixel `pos`, in the **last** drawn figure:
    /// one of the four edge bands (the new cell goes beside the reference of
    /// the band segment under the pointer, see [`band_segments`](Self::band_segments)),
    /// else an internal line of a regular grid
    /// (#504: between the two cells of the pointer's row or column on either
    /// side of it). `moving` is the graph being moved, if any: it is never its
    /// own reference: a band uses the nearest **other** cell (a hole is never a
    /// cell), and a line next to it uses the cell on its other side with the
    /// opposite side. `None` when there is no such cell, as for everything else.
    pub fn drop_place(&self, pos: Pos2, moving: Option<usize>) -> Option<DropPlace> {
        let display = self.display_rect?;
        if let Some(band) = edge_drop_side(pos, display, self.edge_band_width()) {
            let segments = self.band_segments(band, moving);
            let (segment, found) = segments
                .iter()
                .enumerate()
                .min_by(|a, b| a.1.along_distance(pos).total_cmp(&b.1.along_distance(pos)))?;
            let panel = match found.reference {
                Some(panel) => panel,
                None => self.nearest_cell_except(pos, moving)?,
            };
            return Some(DropPlace {
                panel,
                side: band,
                band: Some(band),
                segment,
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
            segment: 0,
        };
        before
            .map(|panel| place(panel, near))
            .or_else(|| after.map(|panel| place(panel, far)))
    }

    /// The preview of a drop while the pointer drags (#503): every band shown
    /// as pale rounded segments, one per graph row or column it lies beside, so
    /// the place of the new cell reads before aiming; the segment under the
    /// pointer filled. The thick bar only marks a drop **between** two graphs
    /// (#504), where no segment says it; in a band the filled segment is the
    /// whole answer.
    pub fn paint_drop_places(&self, painter: &Painter, place: Option<DropPlace>) {
        let Some(figure) = self.display_rect else {
            return;
        };
        let moving = None;
        let band = fitted_band(figure, self.edge_band_width());
        let (left, right) = (figure.min.x + band, figure.max.x - band);
        for side in [
            CellSide::Left,
            CellSide::Right,
            CellSide::Above,
            CellSide::Below,
        ] {
            for (index, segment) in self.band_segments(side, moving).iter().enumerate() {
                let active =
                    place.is_some_and(|place| place.band == Some(side) && place.segment == index);
                let mut rect = segment.rect;
                // The top and bottom bands stop a gap before the side bands.
                if matches!(side, CellSide::Above | CellSide::Below) {
                    rect.min.x = rect.min.x.max(left + SEGMENT_GAP);
                    rect.max.x = rect.max.x.min(right - SEGMENT_GAP);
                }
                let rect = match side {
                    CellSide::Left | CellSide::Right => {
                        rect.shrink2(egui::vec2(0.0, SEGMENT_GAP / 2.0))
                    }
                    _ => rect.shrink2(egui::vec2(SEGMENT_GAP / 2.0, 0.0)),
                };
                if rect.width() <= 0.0 || rect.height() <= 0.0 {
                    continue;
                }
                let (fill, stroke) = if active {
                    (edge_band_fill(), edge_band_stroke())
                } else {
                    (edge_band_pale_fill(), edge_band_pale_stroke())
                };
                painter.rect(rect.shrink(0.5), 3.0, fill, stroke, StrokeKind::Inside);
            }
        }
        let Some(place) = place.filter(|place| place.band.is_none()) else {
            return;
        };
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

/// The hit zone of the band of `side`, as [`edge_drop_side`] splits them: the
/// left and right bands are full-height rectangles, the top and bottom ones
/// stop at them.
fn band_rect(figure: Rect, band: f32, side: CellSide) -> Rect {
    let (min, max) = (figure.min, figure.max);
    let (left, right) = (min.x + band, max.x - band);
    let (x0, y0, x1, y1) = match side {
        CellSide::Left => (min.x, min.y, left, max.y),
        CellSide::Right => (right, min.y, max.x, max.y),
        CellSide::Above => (left, min.y, right, min.y + band),
        CellSide::Below => (left, max.y - band, right, max.y),
    };
    Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1))
}

/// Pale outline of a band segment that is not under the pointer.
pub fn edge_band_pale_stroke() -> Stroke {
    Stroke::new(1.0, Color32::from_rgba_unmultiplied(70, 150, 230, 70))
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

    /// #503: the hit zone is 12 pt at one pixel per point, and never under
    /// [`MIN_EDGE_BAND_PX`] physical pixels on a sparser screen.
    /// Witness: the floor removed from `edge_band_width` (0.5 ppp gives 12 pt =
    /// 6 px).
    #[test]
    fn the_band_is_twelve_points_with_a_floor_in_screen_pixels() {
        assert_eq!(EDGE_BAND_WIDTH, 12.0);
        assert_eq!(edge_band_width(1.0), 12.0);
        assert_eq!(edge_band_width(2.0), 12.0);
        assert_eq!(edge_band_width(0.5), 16.0, "8 px at half a pixel per point");
        assert!(edge_band_width(0.25) * 0.25 >= MIN_EDGE_BAND_PX);
        assert_eq!(edge_band_width(f32::NAN), 12.0);
        let figure = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 300.0));
        let at = |x: f32| edge_drop_side(egui::pos2(x, 150.0), figure, edge_band_width(1.0));
        assert_eq!(at(11.5), Some(CellSide::Left));
        assert_eq!(at(12.5), None, "past 12 pt it is the interior");
    }
}
