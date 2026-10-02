//! Le tracé des objets graphiques d'un panneau — rectangle, ellipse, ligne,
//! flèche (issue #76) — et leur désignation sous le pointeur.
//!
//! Comme tout le rendu, c'est **ici**, générique sur le backend, et en
//! **pixels** : les deux points de l'objet sont projetés par le pipeline
//! (sans test d'appartenance au cadrage : un rectangle qui déborde du zoom
//! reste tracé, rogné), puis ce module construit la géométrie et la rogne
//! contre le rectangle de tracé ([`crate::clip`]).
//!
//! - le **trait** passe par [`clip_polyline`] : le contour d'un rectangle
//!   dont un côté est hors cadre n'est pas replaqué sur le bord ;
//! - le **remplissage** est le polygone rogné par Sutherland-Hodgman
//!   ([`clip_polygon`]) — un contour fermé de l'ellipse, polygonal à
//!   [`ELLIPSE_SEGMENTS`] côtés, est convexe, ce qui suffit à cet algorithme ;
//! - la **pointe** d'une flèche est un triangle plein, omis quand la pointe
//!   est hors cadre.

use plotters::coord::Shift;
use plotters::element::{PathElement, Polygon};
use plotters::prelude::*;
use plotters_backend::{BackendCoord, DrawingBackend};
use plotyx_plot::{ShapeKind, ShapeSpec};

use crate::clip::{clip_polyline, clip_segment, PixelBounds};
use crate::color::{parse_hex, rgba_or};
use crate::paint::{opaque, rgba, stroke_width};

/// Côtés du polygone qui approche une ellipse.
pub const ELLIPSE_SEGMENTS: usize = 72;

/// Tolérance de désignation autour du trait, en pixels.
const HIT_TOLERANCE: f64 = 4.0;

/// Un objet graphique tel qu'il a été tracé — de quoi le désigner sous le
/// pointeur ([`crate::hit`]) et lui poser ses poignées.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawnShape {
    /// Rang dans `PanelSpec::shapes`.
    pub index: usize,
    /// Sa nature.
    pub kind: ShapeKind,
    /// Premier point, en pixels de backend (non rogné).
    pub p1: (f64, f64),
    /// Second point, en pixels de backend (non rogné).
    pub p2: (f64, f64),
    /// Vrai si l'intérieur est rempli (donc désignable).
    pub filled: bool,
    /// Épaisseur du trait, en pixels.
    pub width: f64,
}

impl DrawnShape {
    /// Vrai si le pixel `(x, y)` désigne l'objet : sur son trait, ou, s'il est
    /// rempli, dedans.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let tolerance = HIT_TOLERANCE + self.width / 2.0;
        match self.kind {
            ShapeKind::Line | ShapeKind::Arrow => {
                distance_to_segment((x, y), self.p1, self.p2) <= tolerance
            }
            ShapeKind::Rectangle => {
                let (left, right) = ordered(self.p1.0, self.p2.0);
                let (top, bottom) = ordered(self.p1.1, self.p2.1);
                let outer = x >= left - tolerance
                    && x <= right + tolerance
                    && y >= top - tolerance
                    && y <= bottom + tolerance;
                if self.filled {
                    return outer;
                }
                let inner = x > left + tolerance
                    && x < right - tolerance
                    && y > top + tolerance
                    && y < bottom - tolerance;
                outer && !inner
            }
            ShapeKind::Ellipse => {
                let (left, right) = ordered(self.p1.0, self.p2.0);
                let (top, bottom) = ordered(self.p1.1, self.p2.1);
                let (a, b) = (
                    ((right - left) / 2.0).max(1.0),
                    ((bottom - top) / 2.0).max(1.0),
                );
                let (dx, dy) = (x - (left + right) / 2.0, y - (top + bottom) / 2.0);
                let radial = ((dx / a).powi(2) + (dy / b).powi(2)).sqrt();
                let slack = tolerance / a.min(b);
                if self.filled {
                    radial <= 1.0 + slack
                } else {
                    (radial - 1.0).abs() <= slack
                }
            }
        }
    }
}

fn ordered(a: f64, b: f64) -> (f64, f64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn distance_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length_sq = dx * dx + dy * dy;
    let t = if length_sq == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / length_sq).clamp(0.0, 1.0)
    };
    ((p.0 - (a.0 + t * dx)).hypot(p.1 - (a.1 + t * dy))).abs()
}

/// Les sommets, en pixels, du contour fermé d'un rectangle ou d'une ellipse
/// entre `p1` et `p2` (le dernier sommet n'est pas répété). Vide pour une
/// ligne ou une flèche.
pub fn outline(kind: ShapeKind, p1: (f64, f64), p2: (f64, f64)) -> Vec<(f64, f64)> {
    let (left, right) = ordered(p1.0, p2.0);
    let (top, bottom) = ordered(p1.1, p2.1);
    match kind {
        ShapeKind::Rectangle => vec![(left, top), (right, top), (right, bottom), (left, bottom)],
        ShapeKind::Ellipse => {
            let (cx, cy) = ((left + right) / 2.0, (top + bottom) / 2.0);
            let (a, b) = ((right - left) / 2.0, (bottom - top) / 2.0);
            (0..ELLIPSE_SEGMENTS)
                .map(|i| {
                    let angle = std::f64::consts::TAU * i as f64 / ELLIPSE_SEGMENTS as f64;
                    (cx + a * angle.cos(), cy + b * angle.sin())
                })
                .collect()
        }
        ShapeKind::Line | ShapeKind::Arrow => Vec::new(),
    }
}

/// La pointe d'une flèche : un triangle dont le sommet est `tip`, ouvert vers
/// `tail`. Longueur et largeur croissent avec l'épaisseur du trait. `None` si
/// la flèche est réduite à un point.
pub fn arrow_head(tail: (f64, f64), tip: (f64, f64), width: f64) -> Option<[(f64, f64); 3]> {
    let (dx, dy) = (tip.0 - tail.0, tip.1 - tail.1);
    let length = dx.hypot(dy);
    if !length.is_finite() || length < 1.0 {
        return None;
    }
    let (ux, uy) = (dx / length, dy / length);
    // Jamais plus longue que la moitié de la flèche : une petite flèche reste
    // une flèche.
    let head = (8.0 + 3.0 * width).min(length / 2.0);
    let half = head * 0.4;
    let base = (tip.0 - ux * head, tip.1 - uy * head);
    Some([
        tip,
        (base.0 - uy * half, base.1 + ux * half),
        (base.0 + uy * half, base.1 - ux * half),
    ])
}

/// Rogne un polygone contre le rectangle `bounds` (Sutherland-Hodgman).
pub fn clip_polygon(bounds: PixelBounds, polygon: &[(f64, f64)]) -> Vec<(f64, f64)> {
    type Edge = (
        fn((f64, f64), PixelBounds) -> bool,
        fn((f64, f64), (f64, f64), PixelBounds) -> (f64, f64),
    );
    fn at_x(a: (f64, f64), b: (f64, f64), x: f64) -> (f64, f64) {
        let t = (x - a.0) / (b.0 - a.0);
        (x, a.1 + t * (b.1 - a.1))
    }
    fn at_y(a: (f64, f64), b: (f64, f64), y: f64) -> (f64, f64) {
        let t = (y - a.1) / (b.1 - a.1);
        (a.0 + t * (b.0 - a.0), y)
    }
    let edges: [Edge; 4] = [
        (|p, bd| p.0 >= bd.x_min, |a, b, bd| at_x(a, b, bd.x_min)),
        (|p, bd| p.0 <= bd.x_max, |a, b, bd| at_x(a, b, bd.x_max)),
        (|p, bd| p.1 >= bd.y_min, |a, b, bd| at_y(a, b, bd.y_min)),
        (|p, bd| p.1 <= bd.y_max, |a, b, bd| at_y(a, b, bd.y_max)),
    ];
    let mut current: Vec<(f64, f64)> = polygon
        .iter()
        .copied()
        .filter(|p| p.0.is_finite() && p.1.is_finite())
        .collect();
    for (inside, cut) in edges {
        if current.is_empty() {
            break;
        }
        let mut next = Vec::with_capacity(current.len() + 2);
        for (i, &b) in current.iter().enumerate() {
            let a = current[(i + current.len() - 1) % current.len()];
            match (inside(a, bounds), inside(b, bounds)) {
                (true, true) => next.push(b),
                (true, false) => next.push(cut(a, b, bounds)),
                (false, true) => {
                    next.push(cut(a, b, bounds));
                    next.push(b);
                }
                (false, false) => {}
            }
        }
        current = next;
    }
    current
}

fn rounded(point: (f64, f64)) -> BackendCoord {
    (point.0.round() as i32, point.1.round() as i32)
}

/// Trace un objet entre les pixels `p1` et `p2`, rogné à `bounds` s'il y en a
/// un. `ink` est la couleur d'un trait automatique.
pub fn draw_shape<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    shape: &ShapeSpec,
    p1: (f64, f64),
    p2: (f64, f64),
    bounds: Option<PixelBounds>,
    ink: (u8, u8, u8),
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    let stroke = ShapeStyle {
        color: rgba(rgba_or(&shape.line_color, opaque(ink))),
        filled: false,
        stroke_width: stroke_width(shape.effective_width()),
    };
    if shape.kind.is_closed() {
        let polygon = outline(shape.kind, p1, p2);
        if let Some(fill) = parse_hex(&shape.fill_color) {
            let visible = match bounds {
                Some(bounds) => clip_polygon(bounds, &polygon),
                None => polygon.clone(),
            };
            if visible.len() >= 3 {
                let style = ShapeStyle {
                    color: rgba(fill),
                    filled: true,
                    stroke_width: 1,
                };
                area.draw(&Polygon::new(
                    visible.into_iter().map(rounded).collect::<Vec<_>>(),
                    style,
                ))?;
            }
        }
        let mut closed: Vec<BackendCoord> = polygon.into_iter().map(rounded).collect();
        if let Some(&first) = closed.first() {
            closed.push(first);
        }
        match bounds {
            Some(bounds) => {
                for run in clip_polyline(bounds, &closed) {
                    area.draw(&PathElement::new(run, stroke))?;
                }
            }
            None => area.draw(&PathElement::new(closed, stroke))?,
        }
        return Ok(());
    }
    // Ligne ou flèche.
    let segment = match bounds {
        Some(bounds) => clip_segment(bounds, p1, p2),
        None => Some((p1, p2)),
    };
    if let Some((a, b)) = segment {
        if rounded(a) != rounded(b) {
            area.draw(&PathElement::new(vec![rounded(a), rounded(b)], stroke))?;
        }
    }
    if shape.kind == ShapeKind::Arrow {
        let tip_visible = bounds.is_none_or(|bounds| {
            p2.0 >= bounds.x_min
                && p2.0 <= bounds.x_max
                && p2.1 >= bounds.y_min
                && p2.1 <= bounds.y_max
        });
        if let (true, Some(head)) = (tip_visible, arrow_head(p1, p2, shape.effective_width())) {
            let style = ShapeStyle {
                color: stroke.color,
                filled: true,
                stroke_width: 1,
            };
            area.draw(&Polygon::new(head.map(rounded).to_vec(), style))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn carre() -> PixelBounds {
        PixelBounds {
            x_min: 0.0,
            y_min: 0.0,
            x_max: 100.0,
            y_max: 100.0,
        }
    }

    fn drawn(kind: ShapeKind, filled: bool) -> DrawnShape {
        DrawnShape {
            index: 0,
            kind,
            p1: (10.0, 10.0),
            p2: (50.0, 40.0),
            filled,
            width: 1.5,
        }
    }

    #[test]
    fn un_rectangle_vide_se_designe_sur_son_trait_pas_dedans() {
        let rect = drawn(ShapeKind::Rectangle, false);
        assert!(rect.contains(10.0, 25.0), "côté gauche");
        assert!(rect.contains(30.0, 40.0), "côté bas");
        assert!(!rect.contains(30.0, 25.0), "intérieur vide");
        assert!(!rect.contains(80.0, 25.0), "dehors");
        assert!(drawn(ShapeKind::Rectangle, true).contains(30.0, 25.0));
    }

    #[test]
    fn une_ellipse_se_designe_sur_son_contour() {
        let ellipse = drawn(ShapeKind::Ellipse, false);
        assert!(ellipse.contains(50.0, 25.0), "extrémité droite");
        assert!(ellipse.contains(30.0, 10.0), "sommet");
        assert!(!ellipse.contains(30.0, 25.0), "centre");
        assert!(!ellipse.contains(10.0, 10.0), "coin de la boîte");
        assert!(drawn(ShapeKind::Ellipse, true).contains(30.0, 25.0));
    }

    #[test]
    fn une_ligne_se_designe_a_quelques_pixels() {
        let line = drawn(ShapeKind::Line, false);
        assert!(line.contains(30.0, 25.0), "milieu du segment");
        assert!(line.contains(30.0, 28.0));
        assert!(!line.contains(30.0, 40.0));
        assert!(!line.contains(80.0, 55.0), "au-delà de l'extrémité");
    }

    #[test]
    fn le_polygone_est_rogne_au_cadre() {
        let rect = outline(ShapeKind::Rectangle, (-50.0, 20.0), (50.0, 200.0));
        let clipped = clip_polygon(carre(), &rect);
        assert!(!clipped.is_empty());
        for (x, y) in &clipped {
            assert!(
                (0.0..=100.0).contains(x) && (0.0..=100.0).contains(y),
                "{x},{y}"
            );
        }
        let hors = outline(ShapeKind::Rectangle, (200.0, 200.0), (300.0, 300.0));
        assert!(clip_polygon(carre(), &hors).is_empty());
    }

    #[test]
    fn la_pointe_est_en_bout_de_fleche_et_jamais_plus_longue_que_la_moitie() {
        let head = arrow_head((0.0, 0.0), (100.0, 0.0), 1.5).unwrap();
        assert_eq!(head[0], (100.0, 0.0));
        assert!(head[1].0 < 100.0 && head[2].0 < 100.0);
        assert!((head[1].1 + head[2].1).abs() < 1e-9, "symétrique");
        let petite = arrow_head((0.0, 0.0), (6.0, 0.0), 1.5).unwrap();
        assert!(petite[1].0 >= 3.0 - 1e-9, "pointe bornée à la moitié");
        assert!(arrow_head((5.0, 5.0), (5.0, 5.0), 1.5).is_none());
    }
}
