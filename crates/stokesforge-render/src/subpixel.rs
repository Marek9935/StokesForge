//! **Sub-pixel** paths, despite Plotters' integer coordinates (issue #91).
//!
//! `DrawingBackend::draw_path` only takes `BackendCoord` (`i32`): a circle
//! whose every vertex is rounded to the pixel has flats and notches, however
//! many sides it has — more sides change nothing, the error is the rounding
//! itself (±0.5 px), mostly visible in vector export.
//!
//! Rather than one more argument through the whole pipeline, a sub-pixel path
//! is an ordinary `draw_path` with two conventions, which **both** backends
//! read (never just one — invariant 5):
//!
//! - the coordinates are **fixed-point**: pixel × [`SCALE`];
//! - the width carries a flag ([`stroke_width`]) that signals it.
//!
//! A backend that ignores the convention (a test backend) draws the path
//! sixteen times too large: this is intended, it shows. The screen backend
//! (`stokesforge-render-egui`) decodes at draw time; the SVG export
//! (`stokesforge-render-svg::subpixel`) rewrites the marked `<polyline>`s
//! once the document is finished, since `plotters-svg` writes only integers.

use plotters::coord::CoordTranslate;
use plotters::element::{CoordMapper, Drawable, PointCollection};
use plotters::style::ShapeStyle;
use plotters_backend::{BackendCoord, DrawingBackend, DrawingErrorKind};

/// Fixed-point factor: 1/16 of a pixel.
pub const SCALE: i32 = 16;

/// Flag of a sub-pixel path width — far above any real width (capped at 20,
/// `ShapeSpec::WIDTH_RANGE`).
const FLAG: u32 = 1 << 24;

/// The width to give to the style of a sub-pixel path.
pub fn stroke_width(width: u32) -> u32 {
    width | FLAG
}

/// The real width of a style, and whether its path is sub-pixel.
pub fn split_width(width: u32) -> (u32, bool) {
    (width & !FLAG, width & FLAG != 0)
}

/// A point in pixels, as fixed-point.
pub fn encode((x, y): (f64, f64)) -> BackendCoord {
    (
        (x * SCALE as f64).round() as i32,
        (y * SCALE as f64).round() as i32,
    )
}

/// A fixed-point point, in pixels.
pub fn decode((x, y): BackendCoord) -> (f64, f64) {
    (x as f64 / SCALE as f64, y as f64 / SCALE as f64)
}

/// The drawing area's pass to the backend **without** `Rect::truncate`: the
/// ordinary Plotters mapping brings every point back into the area's
/// rectangle, which would crush a fixed-point point (sixteen times larger
/// than a pixel) against the corner.
pub struct Unclamped;

impl CoordMapper for Unclamped {
    type Output = BackendCoord;
    fn map<CT: CoordTranslate>(
        coord_trans: &CT,
        from: &CT::From,
        _rect: &plotters::drawing::Rect,
    ) -> BackendCoord {
        coord_trans.translate(from)
    }
}

/// A sub-pixel path: `points` as fixed-point ([`encode`]), the width of
/// `style` already marked ([`stroke_width`]). To be drawn with
/// `DrawingArea::draw`.
pub struct SubpixelPath {
    points: Vec<BackendCoord>,
    style: ShapeStyle,
}

impl SubpixelPath {
    /// A path between these points, in pixels, of this style (real width:
    /// the flag is set here).
    pub fn new(points: &[(f64, f64)], style: ShapeStyle) -> Self {
        Self {
            points: points.iter().copied().map(encode).collect(),
            style: ShapeStyle {
                stroke_width: stroke_width(style.stroke_width),
                ..style
            },
        }
    }
}

impl<'a> PointCollection<'a, BackendCoord, Unclamped> for &'a SubpixelPath {
    type Point = &'a BackendCoord;
    type IntoIter = &'a [BackendCoord];
    fn point_iter(self) -> &'a [BackendCoord] {
        &self.points
    }
}

impl<DB: DrawingBackend> Drawable<DB, Unclamped> for SubpixelPath {
    fn draw<I: Iterator<Item = BackendCoord>>(
        &self,
        points: I,
        backend: &mut DB,
        _parent_dim: (u32, u32),
    ) -> Result<(), DrawingErrorKind<DB::ErrorType>> {
        backend.draw_path(points, &self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_sets_and_clears_without_touching_the_width() {
        for width in [0, 1, 2, 20] {
            assert_eq!(split_width(stroke_width(width)), (width, true));
            assert_eq!(split_width(width), (width, false));
        }
    }

    #[test]
    fn the_fixed_point_keeps_a_sixteenth_of_a_pixel() {
        let point = (12.3125, -4.0625);
        assert_eq!(decode(encode(point)), point);
        assert!((decode(encode((1.01, 2.0))).0 - 1.01).abs() <= 0.5 / SCALE as f64);
    }
}
