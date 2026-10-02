//! Des chemins au **sous-pixel**, malgré des coordonnées de Plotters entières
//! (issue #91).
//!
//! `DrawingBackend::draw_path` ne prend que des `BackendCoord` (`i32`) : un
//! cercle dont chaque sommet est arrondi au pixel a des plats et des crans,
//! quel que soit son nombre de côtés — plus de côtés n'y change rien, l'erreur
//! est l'arrondi lui-même (±0,5 px), visible surtout à l'export vectoriel.
//!
//! Plutôt qu'un trait de plus sur tout le pipeline, un chemin sous-pixel est
//! un `draw_path` ordinaire à deux conventions, que les **deux** backends
//! lisent (jamais un seul — invariant 5) :
//!
//! - les coordonnées sont des **points fixes** : pixel × [`SCALE`] ;
//! - l'épaisseur porte un drapeau ([`stroke_width`]) qui le signale.
//!
//! Un backend qui ignore la convention (un backend de test) trace le chemin
//! seize fois trop grand : c'est voulu, ça se voit. Le backend écran
//! (`stokesforge-render-egui`) décode au tracé ; l'export SVG
//! (`stokesforge-render-svg::subpixel`) réécrit les `<polyline>` marquées une
//! fois le document fini, `plotters-svg` n'écrivant que des entiers.

use plotters::coord::CoordTranslate;
use plotters::element::{CoordMapper, Drawable, PointCollection};
use plotters::style::ShapeStyle;
use plotters_backend::{BackendCoord, DrawingBackend, DrawingErrorKind};

/// Facteur du point fixe : 1/16 de pixel.
pub const SCALE: i32 = 16;

/// Drapeau d'une épaisseur de chemin sous-pixel — très au-dessus de toute
/// épaisseur réelle (bornée à 20, `ShapeSpec::WIDTH_RANGE`).
const FLAG: u32 = 1 << 24;

/// L'épaisseur à donner au style d'un chemin sous-pixel.
pub fn stroke_width(width: u32) -> u32 {
    width | FLAG
}

/// L'épaisseur réelle d'un style, et si son chemin est sous-pixel.
pub fn split_width(width: u32) -> (u32, bool) {
    (width & !FLAG, width & FLAG != 0)
}

/// Un point en pixels, en point fixe.
pub fn encode((x, y): (f64, f64)) -> BackendCoord {
    (
        (x * SCALE as f64).round() as i32,
        (y * SCALE as f64).round() as i32,
    )
}

/// Un point fixe, en pixels.
pub fn decode((x, y): BackendCoord) -> (f64, f64) {
    (x as f64 / SCALE as f64, y as f64 / SCALE as f64)
}

/// Le passage de l'aire de dessin au backend **sans** `Rect::truncate` : la
/// correspondance ordinaire de Plotters ramène chaque point dans le rectangle
/// de l'aire, ce qui écraserait un point fixe (seize fois plus grand qu'un
/// pixel) contre le coin.
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

/// Un chemin sous-pixel : `points` en points fixes ([`encode`]), l'épaisseur
/// de `style` déjà marquée ([`stroke_width`]). À tracer par
/// `DrawingArea::draw`.
pub struct SubpixelPath {
    points: Vec<BackendCoord>,
    style: ShapeStyle,
}

impl SubpixelPath {
    /// Un chemin entre ces points, en pixels, de ce style (épaisseur réelle :
    /// le drapeau est posé ici).
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
    fn le_drapeau_se_pose_et_se_retire_sans_toucher_l_epaisseur() {
        for width in [0, 1, 2, 20] {
            assert_eq!(split_width(stroke_width(width)), (width, true));
            assert_eq!(split_width(width), (width, false));
        }
    }

    #[test]
    fn le_point_fixe_garde_le_seizieme_de_pixel() {
        let point = (12.3125, -4.0625);
        assert_eq!(decode(encode(point)), point);
        assert!((decode(encode((1.01, 2.0))).0 - 1.01).abs() <= 0.5 / SCALE as f64);
    }
}
