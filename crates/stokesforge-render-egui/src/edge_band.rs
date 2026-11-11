//! The drop zones of a figure (#357, #491, #503, #504): the four edge strips
//! where a drop makes a **new** column or row beside the grid row or column it
//! lies against, and the internal grid lines where it makes one **between** two
//! graphs. One implementation for the two gestures that use it, dropping
//! workbook columns on a graph (`stokesforge-app`) and moving a graph in the
//! Page Layout Mode (`layout_mode`): [`GraphView::drop_place`] decides,
//! [`GraphView::paint_drop_places`] shows, from the **same** polygons. A zone
//! is a grid **position**, never a reference graph: a graph, the moving one or
//! a hole sits there or not, the new line opens at that row or column. Screen
//! only: nothing here enters the cache or the export.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, StrokeKind};
use stokesforge_core::compose::CellSide;
use stokesforge_plot::LayerPlacement;
use stokesforge_render::{grid_line_at, LayoutReach, LineAxis};

use crate::GraphView;

/// Width, in points, of the four bands of a figure: the hit zone **and** what
/// is drawn. A constant of the screen, not of the figure: zooming the data or
/// shrinking the figure to fit its window does not move it (#503).
pub const EDGE_BAND_WIDTH: f32 = 24.0;

/// Floor, in physical screen pixels, of the hit zone of a band: on a screen
/// that lays out below one pixel per point, [`edge_band_width`] widens the
/// band in points so it never drops under this many pixels.
pub const MIN_EDGE_BAND_PX: f32 = 8.0;

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

/// The sides in the order they are tried: the exact diagonal of a corner (and a
/// seam between two zones) belongs to the first that holds the pointer.
const SIDE_ORDER: [CellSide; 4] = [
    CellSide::Right,
    CellSide::Below,
    CellSide::Left,
    CellSide::Above,
];

/// The polygon of the part of the band of `side` of `figure` lying between the
/// coordinates `from` and `to` along it (x for the top and bottom bands, y for
/// the others): the four bands are mitred at 45 degrees in the corners, like
/// the corners of a picture frame, so each corner square is shared by the two
/// bands along its diagonal and none takes it whole (#503). Convex. The one
/// shape both painted and hit-tested.
fn band_polygon(figure: Rect, band: f32, side: CellSide, from: f32, to: f32) -> Vec<Pos2> {
    let vertical = matches!(side, CellSide::Left | CellSide::Right);
    let (low, high) = if vertical {
        (figure.min.y, figure.max.y)
    } else {
        (figure.min.x, figure.max.x)
    };
    let (from, to) = (from.max(low), to.min(high));
    // The depth of the band at `t` along it: `band`, but shrinking to nothing
    // toward the ends, where the neighbouring band takes over on the diagonal.
    let depth = |t: f32| band.min(t - low).min(high - t).max(0.0);
    let at = |u: f32, t: f32| match side {
        CellSide::Left => egui::pos2(figure.min.x + u, t),
        CellSide::Right => egui::pos2(figure.max.x - u, t),
        CellSide::Above => egui::pos2(t, figure.min.y + u),
        CellSide::Below => egui::pos2(t, figure.max.y - u),
    };
    let mut cuts = vec![from];
    cuts.extend(
        [low + band, high - band]
            .into_iter()
            .filter(|k| *k > from && *k < to),
    );
    cuts.push(to);
    let mut points = vec![at(0.0, from)];
    points.extend(cuts.iter().map(|t| at(depth(*t), *t)));
    points.push(at(0.0, to));
    points.dedup_by(|a, b| (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4);
    points
}

/// Whether `point` lies in the convex `polygon`, edges included (to a
/// thousandth of a point).
fn polygon_contains(polygon: &[Pos2], point: Pos2) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let (mut left, mut right) = (false, false);
    for (index, a) in polygon.iter().enumerate() {
        let b = polygon[(index + 1) % polygon.len()];
        let edge = b - *a;
        let length = edge.length();
        if length < 1e-6 {
            continue;
        }
        let side = (edge.x * (point.y - a.y) - edge.y * (point.x - a.x)) / length;
        left |= side > 1e-3;
        right |= side < -1e-3;
    }
    !(left && right)
}

/// Which edge band of `figure` the pointer is in (#503): the four bands are
/// mitred at 45 degrees, so in a corner square the nearer edge decides and the
/// exact diagonal goes to the right, then bottom, then left, then top band.
/// `None` outside every band and outside the figure. Pure, and the same
/// polygons as the drawing, so it is tested without a window.
pub fn edge_drop_side(pointer: Pos2, figure: Rect, band: f32) -> Option<CellSide> {
    if !figure.contains(pointer) {
        return None;
    }
    let band = fitted_band(figure, band);
    if band <= 0.0 {
        return None;
    }
    SIDE_ORDER.into_iter().find(|side| {
        let (from, to) = match side {
            CellSide::Left | CellSide::Right => (figure.min.y, figure.max.y),
            CellSide::Above | CellSide::Below => (figure.min.x, figure.max.x),
        };
        polygon_contains(&band_polygon(figure, band, *side, from, to), pointer)
    })
}

/// What a drop zone is anchored to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropAnchor {
    /// A position of a regular grid: the new line opens beside the cell
    /// `(row, column)`, whether a graph or a hole is there.
    Cell {
        /// Row of the position.
        row: usize,
        /// Column of the position.
        column: usize,
    },
    /// A layer, on a figure with free placement (no grid): the cell nearest the
    /// pointer.
    Panel(usize),
}

/// Where a drop at a pointer position puts its new cell: beside the anchor, on
/// `side` of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropPlace {
    /// The grid position (or, in free placement, the layer) it is beside.
    pub anchor: DropAnchor,
    /// Which side of it the new cell goes.
    pub side: CellSide,
    /// The edge band the pointer is in; `None` for a drop on an internal line.
    pub band: Option<CellSide>,
    /// Index of the segment of that band the pointer is in (see
    /// [`BandSegment`]); `0` for a drop on an internal line.
    pub segment: usize,
}

/// One segment of an edge band: the part of the band beside one row (left and
/// right bands) or one column (top and bottom bands) of a regular grid. A figure
/// with free placement has a single segment per band.
#[derive(Debug, Clone, PartialEq)]
pub struct BandSegment {
    /// The band it belongs to.
    pub side: CellSide,
    /// Its shape, mitred in the corners: what is painted **and** what is hit.
    pub polygon: Vec<Pos2>,
    /// The grid position of a drop in it; `None` in free placement, where it is
    /// the cell nearest the pointer.
    pub cell: Option<(usize, usize)>,
}

impl GraphView {
    /// The band width in points on this view's screen, see [`edge_band_width`].
    pub fn edge_band_width(&self) -> f32 {
        edge_band_width(self.cache.key().map_or(1.0, |key| key.pixels_per_point))
    }

    /// The segments of the band of `side` in the last drawn figure, in order
    /// along the band. The one source of both the drawing and the drop
    /// ([`drop_place`](Self::drop_place)). On a regular grid there is one per
    /// row (left and right bands) or column (top and bottom bands), whose
    /// position is that row or column at the grid's outer column or row; it
    /// does not depend on what sits there.
    pub fn band_segments(&self, side: CellSide) -> Vec<BandSegment> {
        let Some(figure) = self.display_rect else {
            return Vec::new();
        };
        let band = fitted_band(figure, self.edge_band_width());
        let vertical = matches!(side, CellSide::Left | CellSide::Right);
        let (low, high) = if vertical {
            (figure.min.y, figure.max.y)
        } else {
            (figure.min.x, figure.max.x)
        };
        let regular = self
            .figure_memo
            .as_ref()
            .zip(self.display_scale())
            .filter(|((_, layout), _)| layout.placement == LayerPlacement::Grid);
        let Some(((_, layout), scale)) = regular else {
            return vec![BandSegment {
                side,
                polygon: band_polygon(figure, band, side, low, high),
                cell: None,
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
        let rows = layout.row_breaks.len().saturating_sub(1);
        let columns = layout.column_breaks.len().saturating_sub(1);
        let count = breaks.len().saturating_sub(1);
        let mut segments = Vec::new();
        for k in 0..count {
            // The first and last cut stretch to the figure's border (the title
            // band lies above the first row).
            let start = if k == 0 { low } else { breaks[k] }.max(low);
            let end = if k + 1 == count { high } else { breaks[k + 1] }.min(high);
            if end - start < 0.01 {
                continue;
            }
            let cell = match side {
                CellSide::Left => (k, 0),
                CellSide::Right => (k, columns.saturating_sub(1)),
                CellSide::Above => (0, k),
                CellSide::Below => (rows.saturating_sub(1), k),
            };
            segments.push(BandSegment {
                side,
                polygon: band_polygon(figure, band, side, start, end),
                cell: Some(cell),
            });
        }
        segments
    }

    /// The drop zone under the egui pixel `pos`, in the **last** drawn figure:
    /// one of the four edge bands (the new cell opens a column or row at the
    /// grid position of the band segment under the pointer, see
    /// [`band_segments`](Self::band_segments); in free placement it goes beside
    /// the cell nearest the pointer other than `moving`), else an internal line
    /// of a regular grid. `None` elsewhere.
    pub fn drop_place(&self, pos: Pos2, moving: Option<usize>) -> Option<DropPlace> {
        let display = self.display_rect?;
        self.band_drop_place(pos, display, moving)
    }

    fn band_drop_place(
        &self,
        pos: Pos2,
        display: Rect,
        moving: Option<usize>,
    ) -> Option<DropPlace> {
        if !display.contains(pos) {
            return None;
        }
        for side in SIDE_ORDER {
            for (segment, found) in self.band_segments(side).iter().enumerate() {
                if !polygon_contains(&found.polygon, pos) {
                    continue;
                }
                let anchor = match found.cell {
                    Some((row, column)) => DropAnchor::Cell { row, column },
                    None => DropAnchor::Panel(self.nearest_cell_except(pos, moving)?),
                };
                return Some(DropPlace {
                    anchor,
                    side,
                    band: Some(side),
                    segment,
                });
            }
        }
        // An internal line of a regular grid: a position too, the cell on its
        // near side with the line's side, a graph there or not (#504).
        let (_, layout) = self.figure_memo.as_ref()?;
        let at = self.figure_position(pos)?;
        let reach = LayoutReach::standard(f64::from(self.display_scale()?));
        let line = grid_line_at(layout, f64::from(at.x), f64::from(at.y), reach)?;
        let (row, column, side) = match line.axis {
            LineAxis::Column => (line.along, line.n, CellSide::Right),
            LineAxis::Row => (line.n, line.along, CellSide::Below),
        };
        Some(DropPlace {
            anchor: DropAnchor::Cell { row, column },
            side,
            band: None,
            segment: 0,
        })
    }

    /// The preview of a drop while the pointer drags (#503): every band shown
    /// as pale mitred segments, one per graph row or column it lies beside, so
    /// the place of the new cell reads before aiming; the segment under the
    /// pointer filled. A drop **between** two graphs (#504) fills a bar as
    /// thick as a band, centred on the line.
    pub fn paint_drop_places(&self, painter: &Painter, place: Option<DropPlace>) {
        let Some(figure) = self.display_rect else {
            return;
        };
        for side in SIDE_ORDER {
            for (index, segment) in self.band_segments(side).iter().enumerate() {
                let active =
                    place.is_some_and(|place| place.band == Some(side) && place.segment == index);
                if segment.polygon.len() < 3 {
                    continue;
                }
                let (fill, stroke) = if active {
                    (edge_band_fill(), edge_band_stroke())
                } else {
                    (edge_band_pale_fill(), edge_band_pale_stroke())
                };
                painter.add(Shape::convex_polygon(segment.polygon.clone(), fill, stroke));
            }
        }
        let Some(place) = place.filter(|place| place.band.is_none()) else {
            return;
        };
        let cell = match place.anchor {
            DropAnchor::Cell { row, column } => self
                .figure_memo
                .as_ref()
                .zip(self.display_scale())
                .and_then(|((_, layout), scale)| {
                    let (x, y, w, h) = layout.cell_rect(row, column)?;
                    let min = figure.min + egui::vec2(x as f32, y as f32) * scale;
                    Some(Rect::from_min_size(
                        min,
                        egui::vec2(w as f32, h as f32) * scale,
                    ))
                }),
            DropAnchor::Panel(panel) => self.cell_rect_screen(panel),
        };
        let Some(cell) = cell else {
            return;
        };
        let half = fitted_band(figure, self.edge_band_width()) / 2.0;
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
            3.0,
            edge_band_fill(),
            edge_band_stroke(),
            StrokeKind::Inside,
        );
    }
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

    fn full_polygon(figure: Rect, band: f32, side: CellSide) -> Vec<Pos2> {
        let (from, to) = match side {
            CellSide::Left | CellSide::Right => (figure.min.y, figure.max.y),
            _ => (figure.min.x, figure.max.x),
        };
        band_polygon(figure, band, side, from, to)
    }

    /// #503 (mitred corners): each of the four bands gives its side; in a
    /// corner square the side of the 45 degree diagonal decides (the exact
    /// diagonal goes right, bottom, left, top in that order); the interior and
    /// the outside give nothing; and the polygon that is **drawn** for the
    /// answer holds the pointer, so hit zone and drawing cannot disagree.
    /// Witness: the left and right bands made full-height rectangles again (the
    /// old rule) fails the corner points that lie on the top or bottom side of
    /// the diagonal.
    #[test]
    fn the_bands_meet_on_the_corner_diagonals_and_the_hit_follows_the_drawing() {
        let figure = Rect::from_min_max(egui::pos2(100.0, 50.0), egui::pos2(500.0, 350.0));
        let band = 36.0;
        let side = |x: f32, y: f32| edge_drop_side(egui::pos2(x, y), figure, band);
        assert_eq!(side(300.0, 200.0), None, "interior");
        assert_eq!(side(480.0, 200.0), Some(CellSide::Right));
        assert_eq!(side(300.0, 340.0), Some(CellSide::Below));
        assert_eq!(side(110.0, 200.0), Some(CellSide::Left));
        assert_eq!(side(300.0, 60.0), Some(CellSide::Above));
        for (x, y, expected) in [
            // Top-left: dx 30 / dy 5 and dx 5 / dy 30, then the tie.
            (130.0, 55.0, CellSide::Above),
            (105.0, 80.0, CellSide::Left),
            (110.0, 60.0, CellSide::Left),
            // Top-right.
            (480.0, 55.0, CellSide::Above),
            (495.0, 80.0, CellSide::Right),
            (490.0, 60.0, CellSide::Right),
            // Bottom-right.
            (480.0, 345.0, CellSide::Below),
            (495.0, 320.0, CellSide::Right),
            // Bottom-left.
            (130.0, 345.0, CellSide::Below),
            (105.0, 320.0, CellSide::Left),
            (110.0, 340.0, CellSide::Below),
        ] {
            let pointer = egui::pos2(x, y);
            assert_eq!(side(x, y), Some(expected), "corner at ({x}, {y})");
            assert!(
                polygon_contains(&full_polygon(figure, band, expected), pointer),
                "the drawn polygon of {expected:?} holds ({x}, {y})"
            );
            for other in SIDE_ORDER.into_iter().filter(|s| *s != expected) {
                let strictly = !polygon_contains(&full_polygon(figure, band, other), pointer);
                let on_diagonal = (x - figure.min.x - (y - figure.min.y)).abs() < 1e-3
                    || (figure.max.x - x - (y - figure.min.y)).abs() < 1e-3
                    || (figure.max.x - x - (figure.max.y - y)).abs() < 1e-3
                    || (x - figure.min.x - (figure.max.y - y)).abs() < 1e-3;
                assert!(
                    strictly || on_diagonal,
                    "({x}, {y}) is drawn in {other:?} too, off the diagonal"
                );
            }
        }
        assert_eq!(side(300.0, 20.0), None, "outside");
        assert_eq!(side(520.0, 200.0), None, "outside");
        assert_eq!(
            edge_drop_side(egui::pos2(480.0, 200.0), figure, 0.0),
            None,
            "a band of width 0 catches nothing"
        );
    }

    /// #503: the hit zone is 24 pt at one pixel per point, and never under
    /// [`MIN_EDGE_BAND_PX`] physical pixels on a sparser screen.
    /// Witness: the floor removed from `edge_band_width` (0.25 ppp gives 24 pt =
    /// 6 px), or the constant back at 12.
    #[test]
    fn the_band_is_twenty_four_points_with_a_floor_in_screen_pixels() {
        assert_eq!(EDGE_BAND_WIDTH, 24.0);
        assert_eq!(edge_band_width(1.0), 24.0);
        assert_eq!(edge_band_width(2.0), 24.0);
        assert_eq!(
            edge_band_width(0.25),
            32.0,
            "8 px at a quarter pixel per point"
        );
        assert!(edge_band_width(0.1) * 0.1 >= MIN_EDGE_BAND_PX);
        assert_eq!(edge_band_width(f32::NAN), 24.0);
        let figure = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 300.0));
        let at = |x: f32| edge_drop_side(egui::pos2(x, 150.0), figure, edge_band_width(1.0));
        assert_eq!(at(23.5), Some(CellSide::Left));
        assert_eq!(at(24.5), None, "past 24 pt it is the interior");
    }
}
