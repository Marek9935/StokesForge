//! Les primitives de tracé, en **pixels** — tâches R5.1 et R5.3.
//!
//! # Pourquoi le tracé descend en pixels plutôt que de rester en données
//!
//! R4 confiait la courbe à `ChartContext::draw_series(LineSeries::new(…))`,
//! ce qui suffisait pour un trait plein. Trois besoins de R5 ne passent plus
//! par là :
//!
//! - **les styles de trait** ([`plotyx_plot::LineStyle`]) : un tiret se
//!   mesure en pixels, pas en unités de données. `DashedLineSeries` de
//!   Plotters ne connaît qu'un motif à deux temps, donc ni `DashDot`, ni un
//!   pointillé dont le point reste rond quelle que soit l'échelle ;
//! - **les marqueurs** ([`plotyx_plot::SymbolShape`]) : Plotters n'a
//!   d'élément tout fait ni pour le carré, ni pour le losange, ni pour la
//!   croix droite ;
//! - **la légende** : celle de Plotters se dimensionne sur
//!   `FontData::estimate_layout`, c'est-à-dire — nos fonts Plotters étant
//!   coupées — sur l'heuristique par octets UTF-8 du §4.4. Le cadre serait
//!   mesuré faux **sans que rien ne le signale**, alors même que le texte
//!   dedans serait dessiné juste. Voir [`crate::legend`].
//!
//! Descendre en pixels ne coûte ni fidélité ni accord entre les deux
//! backends : c'est le même code, dans le pipeline commun, qui calcule les
//! coordonnées entières ; les deux backends ne voient qu'un `draw_path`, un
//! `draw_rect` ou un `draw_circle` déjà résolu.
//!
//! # Le débordement est déjà traité, par Plotters
//!
//! Tout ce qui est peint ici passe par [`plotters::drawing::DrawingArea`],
//! dont le mappage de coordonnées **tronque au rectangle de la zone**
//! (`element/mod.rs` : `rect.truncate(coord_trans.translate(from))`). Un
//! point hors cadrage est donc ramené au bord plutôt que peint sur les
//! graduations. C'est un rognage grossier — un segment qui traverse la zone
//! voit son extrémité déplacée, pas coupée — et c'est le comportement de R4,
//! conservé tel quel : un vrai rognage devient nécessaire avec le zoom, donc
//! en R7.

use plotters::coord::Shift;
use plotters::element::{Circle, PathElement, Polygon, Rectangle};
use plotters::prelude::*;
use plotters_backend::{BackendCoord, DrawingBackend};
use plotyx_plot::{LineSpec, LineStyle, SymbolShape, SymbolSpec};

use crate::color::rgb_or;

/// Couleur de repli d'un trait dont la chaîne du spec est illisible.
pub const SERIES_FALLBACK: (u8, u8, u8) = (0x4c, 0x78, 0xa8);
/// Couleur du texte, des axes et des contours.
pub const INK: (u8, u8, u8) = (0x20, 0x20, 0x20);
/// Fond de repli quand `PlotSpec::background` est vide ou illisible.
pub const PAPER: (u8, u8, u8) = (0xff, 0xff, 0xff);
/// Gris des séries à référence brisée : présentes, et visiblement inertes.
pub const BROKEN: (u8, u8, u8) = (0x9a, 0x9a, 0x9a);

/// Au-delà de ce nombre de tirets pour une seule polyligne, le motif est
/// abandonné au profit d'un trait plein.
///
/// Un garde-fou, pas un réglage : rien ne borne le nombre de points d'une
/// colonne ni la longueur du chemin qui en résulte, et un pointillé de trois
/// pixels sur un chemin d'un million de pixels produirait des centaines de
/// milliers de formes — de quoi faire tomber le rendu pour un détail
/// invisible à cette densité.
pub const MAX_DASHES: usize = 20_000;

/// Une couleur de Plotters depuis un triplet.
pub fn rgb((r, g, b): (u8, u8, u8)) -> RGBColor {
    RGBColor(r, g, b)
}

/// Épaisseur de trait : `LineSpec::width` est un `f64` libre côté spec (rien
/// ne le borne), `ShapeStyle::stroke_width` est un `u32`.
pub fn stroke_width(width: f64) -> u32 {
    if width.is_finite() && width >= 1.0 {
        // Plafonné : une épaisseur venue d'un fichier peut valoir n'importe
        // quoi, et `polygonize` de Plotters alloue proportionnellement.
        (width.round() as u32).min(64)
    } else {
        1
    }
}

/// Le style de trait d'une série, prêt pour Plotters.
pub fn line_style(line: &LineSpec, broken: bool) -> ShapeStyle {
    let color = if broken {
        BROKEN
    } else {
        rgb_or(&line.color, SERIES_FALLBACK)
    };
    ShapeStyle {
        color: rgb(color).into(),
        filled: false,
        stroke_width: stroke_width(line.width),
    }
}

/// Le motif d'un style de trait, en pixels : longueurs alternées
/// plein/vide. `None` pour un trait continu.
pub fn dash_pattern(style: LineStyle) -> Option<&'static [f64]> {
    match style {
        LineStyle::None | LineStyle::Solid => None,
        LineStyle::Dash => Some(&[6.0, 4.0]),
        LineStyle::Dot => Some(&[1.0, 3.0]),
        LineStyle::DashDot => Some(&[6.0, 3.0, 1.0, 3.0]),
    }
}

/// Découpe une polyligne selon un motif de tirets, en pixels.
///
/// Rend les tronçons **pleins**, dans l'ordre. Un motif dont la période est
/// nulle ou dont le découpage dépasserait [`MAX_DASHES`] rend la polyligne
/// entière, en un seul tronçon : mieux vaut un trait plein qu'un rendu qui
/// s'effondre.
pub fn dashes(points: &[BackendCoord], pattern: &[f64]) -> Vec<Vec<BackendCoord>> {
    let period: f64 = pattern.iter().sum();
    if points.len() < 2 || pattern.is_empty() || !period.is_finite() || period <= 0.0 {
        return vec![points.to_vec()];
    }
    let total: f64 = points
        .windows(2)
        .map(|pair| distance(pair[0], pair[1]))
        .sum();
    if !total.is_finite() || total / period * pattern.len() as f64 > MAX_DASHES as f64 {
        return vec![points.to_vec()];
    }

    let mut runs: Vec<Vec<BackendCoord>> = Vec::new();
    let mut current: Vec<BackendCoord> = vec![points[0]];
    let mut on = true;
    let mut index = 0usize;
    let mut remaining = pattern[0];

    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = distance(a, b);
        if length <= 0.0 {
            continue;
        }
        let mut travelled = 0.0;
        while length - travelled > remaining {
            travelled += remaining;
            let cut = lerp(a, b, travelled / length);
            if on {
                current.push(cut);
                if current.len() >= 2 {
                    runs.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
            } else {
                current = vec![cut];
            }
            on = !on;
            index = (index + 1) % pattern.len();
            remaining = pattern[index];
        }
        remaining -= length - travelled;
        if on {
            current.push(b);
        }
    }
    if on && current.len() >= 2 {
        runs.push(current);
    }
    runs
}

fn distance(a: BackendCoord, b: BackendCoord) -> f64 {
    let dx = (b.0 - a.0) as f64;
    let dy = (b.1 - a.1) as f64;
    dx.hypot(dy)
}

fn lerp(a: BackendCoord, b: BackendCoord, t: f64) -> BackendCoord {
    (
        (a.0 as f64 + (b.0 - a.0) as f64 * t).round() as i32,
        (a.1 as f64 + (b.1 - a.1) as f64 * t).round() as i32,
    )
}

/// Trace une polyligne, en tenant compte du style de trait.
///
/// [`LineStyle::None`] ne dessine rien — c'est ce que la variante dit.
pub fn polyline<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    points: &[BackendCoord],
    line: &LineSpec,
    broken: bool,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    if points.len() < 2 || line.style == LineStyle::None {
        return Ok(());
    }
    let style = line_style(line, broken);
    match dash_pattern(line.style) {
        None => area.draw(&PathElement::new(points.to_vec(), style)),
        Some(pattern) => {
            for run in dashes(points, pattern) {
                if run.len() >= 2 {
                    area.draw(&PathElement::new(run, style))?;
                }
            }
            Ok(())
        }
    }
}

/// Rayon d'un marqueur, en pixels.
fn radius(size: f64) -> i32 {
    if size.is_finite() && size >= 2.0 {
        ((size / 2.0).round() as i32).min(64)
    } else {
        1
    }
}

/// Pose un marqueur sur chaque point.
pub fn symbols<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    points: &[BackendCoord],
    symbol: &SymbolSpec,
    broken: bool,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    if symbol.shape == SymbolShape::None {
        return Ok(());
    }
    for &point in points {
        symbol_at(area, point, symbol, broken)?;
    }
    Ok(())
}

/// Pose un marqueur à un endroit donné — aussi ce que la légende dessine
/// dans sa vignette, d'où la fonction séparée.
pub fn symbol_at<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    (x, y): BackendCoord,
    symbol: &SymbolSpec,
    broken: bool,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    if symbol.shape == SymbolShape::None {
        return Ok(());
    }
    let r = radius(symbol.size);
    let fill_color = if broken {
        BROKEN
    } else {
        rgb_or(&symbol.color, SERIES_FALLBACK)
    };
    let edge_color = if broken {
        BROKEN
    } else {
        rgb_or(symbol.effective_edge_color(), fill_color)
    };
    let fill = ShapeStyle {
        color: rgb(fill_color).into(),
        filled: true,
        stroke_width: 1,
    };
    let edge = ShapeStyle {
        color: rgb(edge_color).into(),
        filled: false,
        stroke_width: 1,
    };

    match symbol.shape {
        SymbolShape::None => {}
        SymbolShape::Circle => {
            area.draw(&Circle::new((x, y), r as u32, fill))?;
            area.draw(&Circle::new((x, y), r as u32, edge))?;
        }
        SymbolShape::Square => {
            let corners = [(x - r, y - r), (x + r, y + r)];
            area.draw(&Rectangle::new(corners, fill))?;
            area.draw(&Rectangle::new(corners, edge))?;
        }
        SymbolShape::Triangle => {
            let vertices = vec![(x, y - r), (x + r, y + r), (x - r, y + r)];
            area.draw(&Polygon::new(vertices.clone(), fill))?;
            area.draw(&PathElement::new(closed(vertices), edge))?;
        }
        SymbolShape::Diamond => {
            let vertices = vec![(x, y - r), (x + r, y), (x, y + r), (x - r, y)];
            area.draw(&Polygon::new(vertices.clone(), fill))?;
            area.draw(&PathElement::new(closed(vertices), edge))?;
        }
        SymbolShape::Plus => {
            area.draw(&PathElement::new(vec![(x - r, y), (x + r, y)], edge))?;
            area.draw(&PathElement::new(vec![(x, y - r), (x, y + r)], edge))?;
        }
        SymbolShape::Cross => {
            area.draw(&PathElement::new(
                vec![(x - r, y - r), (x + r, y + r)],
                edge,
            ))?;
            area.draw(&PathElement::new(
                vec![(x - r, y + r), (x + r, y - r)],
                edge,
            ))?;
        }
    }
    Ok(())
}

fn closed(mut vertices: Vec<BackendCoord>) -> Vec<BackendCoord> {
    if let Some(&first) = vertices.first() {
        vertices.push(first);
    }
    vertices
}

/// Demi-largeur d'un embout de moustache, en pixels.
const WHISKER_CAP: i32 = 3;

/// Trace les moustaches de barres d'erreur, embouts compris.
pub fn whiskers<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    segments: &[(BackendCoord, BackendCoord)],
    line: &LineSpec,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    let style = ShapeStyle {
        color: rgb(rgb_or(&line.color, SERIES_FALLBACK)).into(),
        filled: false,
        stroke_width: 1,
    };
    for &(a, b) in segments {
        area.draw(&PathElement::new(vec![a, b], style))?;
        // L'embout est perpendiculaire à la moustache ; l'orientation se lit
        // sur le segment lui-même plutôt que d'être transportée depuis la
        // préparation, où elle serait un champ de plus à tenir juste.
        let vertical = (b.0 - a.0).abs() <= (b.1 - a.1).abs();
        for end in [a, b] {
            let cap = if vertical {
                vec![(end.0 - WHISKER_CAP, end.1), (end.0 + WHISKER_CAP, end.1)]
            } else {
                vec![(end.0, end.1 - WHISKER_CAP), (end.0, end.1 + WHISKER_CAP)]
            };
            area.draw(&PathElement::new(cap, style))?;
        }
    }
    Ok(())
}

/// Trace des barres pleines, contour compris.
///
/// Le contour n'est pas une coquetterie : deux barres voisines de même
/// couleur sont indistinguables sans lui, et c'est le cas nominal d'un
/// histogramme.
pub fn bars<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    rects: &[(BackendCoord, BackendCoord)],
    line: &LineSpec,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    let color = rgb_or(&line.color, SERIES_FALLBACK);
    let fill = ShapeStyle {
        color: rgb(color).into(),
        filled: true,
        stroke_width: 1,
    };
    let edge = ShapeStyle {
        color: rgb(INK).into(),
        filled: false,
        stroke_width: 1,
    };
    for &(a, b) in rects {
        area.draw(&Rectangle::new([a, b], fill))?;
        area.draw(&Rectangle::new([a, b], edge))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_trait_plein_na_pas_de_motif() {
        assert!(dash_pattern(LineStyle::Solid).is_none());
        assert!(dash_pattern(LineStyle::None).is_none());
        assert_eq!(dash_pattern(LineStyle::Dash), Some(&[6.0, 4.0][..]));
    }

    #[test]
    fn le_decoupage_en_tirets_alterne_plein_et_vide() {
        // 100 px de long, motif 6 plein / 4 vide : dix périodes, donc dix
        // tronçons pleins (le dernier tombe juste sur la fin).
        let ligne = [(0, 0), (100, 0)];
        let runs = dashes(&ligne, &[6.0, 4.0]);
        assert_eq!(runs.len(), 10, "{runs:?}");
        assert_eq!(runs[0], vec![(0, 0), (6, 0)]);
        assert_eq!(runs[1], vec![(10, 0), (16, 0)]);
        assert!(runs.iter().all(|run| run.len() == 2));
    }

    #[test]
    fn le_motif_traverse_les_sommets_de_la_polyligne() {
        // Le motif ne doit pas se réinitialiser à chaque sommet, sinon un
        // signal bruité redeviendrait un trait plein.
        let ligne = [(0, 0), (3, 0), (6, 0), (100, 0)];
        let brise = dashes(&ligne, &[6.0, 4.0]);
        let droit = dashes(&[(0, 0), (100, 0)], &[6.0, 4.0]);
        assert_eq!(brise.len(), droit.len());
    }

    #[test]
    fn un_motif_absurde_retombe_sur_un_trait_plein() {
        let ligne = [(0, 0), (100, 0)];
        assert_eq!(dashes(&ligne, &[]).len(), 1);
        assert_eq!(dashes(&ligne, &[0.0, 0.0]).len(), 1);
    }

    #[test]
    fn un_chemin_immense_retombe_sur_un_trait_plein() {
        // Le garde-fou de MAX_DASHES : sans lui, un pointillé sur un chemin
        // d'un million de pixels produirait des centaines de milliers de
        // formes.
        let ligne = [(0, 0), (10_000_000, 0)];
        assert_eq!(dashes(&ligne, &[1.0, 3.0]).len(), 1);
    }

    #[test]
    fn lepaisseur_est_bornee_des_deux_cotes() {
        assert_eq!(stroke_width(0.0), 1);
        assert_eq!(stroke_width(f64::NAN), 1);
        assert_eq!(stroke_width(2.4), 2);
        assert_eq!(stroke_width(1e9), 64);
    }

    #[test]
    fn un_rayon_de_marqueur_est_borne_aussi() {
        assert_eq!(radius(7.0), 4);
        assert_eq!(radius(0.0), 1);
        assert_eq!(radius(f64::INFINITY), 1);
        assert_eq!(radius(1e6), 64);
    }

    #[test]
    fn une_serie_brisee_prend_le_gris_et_pas_sa_couleur() {
        let line = LineSpec {
            color: "#ff0000".into(),
            ..LineSpec::default()
        };
        assert_eq!(line_style(&line, true).color.rgb(), BROKEN);
        assert_eq!(line_style(&line, false).color.rgb(), (0xff, 0, 0));
    }
}
