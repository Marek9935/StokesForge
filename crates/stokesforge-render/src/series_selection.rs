//! Désignation des séries par les pixels réellement tracés (#83), et
//! poignées de la série sélectionnée (#98).
//!
//! Le pipeline fournit ces chemins après décimation et rognage, avec le rang
//! d'origine. Aucun second calcul des axes, aucune résolution au mouvement de
//! souris : axe droit, histogramme et courbe suivent le même chemin que le rendu.
//!
//! # Poignées de sélection (#98)
//!
//! Une série sélectionnée était repeinte en bleu par-dessus son tracé, ce qui
//! masquait sa couleur et son style. Elle est désormais signalée comme dans
//! OriginLab : de petits carrés pleins ([`HANDLE_SIDE`]) posés **le long du
//! tracé** à intervalle fixe ([`HANDLE_SPACING`]), le tracé lui-même n'étant
//! pas touché. L'espacement se mesure en pixels **d'affichage**, le long du
//! chemin réellement tracé — pas par point de donnée —, pour rester lisible
//! quelle que soit la densité des données, la décimation LTTB ou
//! l'agrandissement de la fenêtre ; d'où la conversion `to_display` que
//! l'appelant fournit à [`DrawnSeries::handle_centers`].
//!
//! Ce qui n'a pas de chemin suit la même règle sur des positions discrètes
//! ([`thinned`]) : un nuage de points reçoit ses carrés sur un sous-ensemble
//! régulier de ses marqueurs, des barres au milieu de leur extrémité libre
//! (côté valeur). Les moustaches ne portent pas de poignée : elles servent à
//! désigner la série, pas à la signaler.
//!
//! La couleur est l'encre automatique du fond de la figure
//! ([`crate::pipeline::figure_ink`]) : noire sur fond clair, blanche sur fond
//! sombre, indépendante de la couleur de la série.
//!
//! Les poignées sont un état de **vue**, pas du modèle : elles ne passent pas
//! par le pipeline (la figure, son cache et l'export SVG/PDF/PNG n'en savent
//! rien), mais par [`paint_handles`], générique sur le backend, que l'écran
//! appelle par-dessus la figure en cache.

use crate::{DrawnPlot, PanelTransform};
use plotters::style::RGBColor;
use plotters_backend::{DrawingBackend, DrawingErrorKind};

type Pixel = (i32, i32);

/// Côté d'une poignée de sélection, en pixels d'affichage — indépendant du
/// zoom et de la taille de la fenêtre.
pub const HANDLE_SIDE: f64 = 5.0;

/// Espacement des poignées le long du tracé, en pixels d'affichage :
/// assez serré pour suivre la forme d'une courbe, assez lâche pour ne pas la
/// recouvrir (un carré de 5 px tous les 50 px masque 10 % du trait).
pub const HANDLE_SPACING: f64 = 50.0;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DrawnSeries {
    pub index: usize,
    /// Les tronçons de la ligne tracée, après rognage, au seizième de pixel.
    pub paths: Vec<Vec<(f64, f64)>>,
    /// Les moustaches (barres d'erreur), après rognage : désignables, sans
    /// poignée.
    pub whiskers: Vec<(Pixel, Pixel)>,
    /// Les marqueurs visibles.
    pub points: Vec<Pixel>,
    /// Les barres visibles, en rectangles normalisés (coin haut-gauche, coin
    /// bas-droit), rognés.
    pub bars: Vec<(Pixel, Pixel)>,
    /// Pour chaque barre de `bars`, le milieu de son extrémité libre (côté
    /// valeur, pas côté ligne de base), rogné : la place de sa poignée.
    pub bar_tips: Vec<Pixel>,
}

fn segment_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 {
        0.0
    } else {
        ((p.0 - a.0) * dx + (p.1 - a.1) * dy) / length
    }
    .clamp(0.0, 1.0);
    (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
}

impl DrawnSeries {
    pub fn distance(&self, pixel: (f64, f64)) -> f64 {
        let paths = self
            .paths
            .iter()
            .flat_map(|path| path.windows(2))
            .map(|s| segment_distance(pixel, s[0], s[1]));
        let whiskers = self.whiskers.iter().map(|&(a, b)| {
            segment_distance(pixel, (a.0 as f64, a.1 as f64), (b.0 as f64, b.1 as f64))
        });
        let points = self
            .points
            .iter()
            .map(|&p| segment_distance(pixel, (p.0 as f64, p.1 as f64), (p.0 as f64, p.1 as f64)));
        let bars = self.bars.iter().map(|&(a, b)| {
            let x = pixel.0.clamp(a.0 as f64, b.0 as f64);
            let y = pixel.1.clamp(a.1 as f64, b.1 as f64);
            (pixel.0 - x).hypot(pixel.1 - y)
        });
        paths
            .chain(whiskers)
            .chain(points)
            .chain(bars)
            .fold(f64::INFINITY, f64::min)
    }

    /// Centres des poignées de sélection, en pixels d'affichage.
    ///
    /// `to_display` convertit un pixel de la figure en pixel d'affichage
    /// (l'écran agrandit ou réduit la figure) : l'espacement
    /// [`HANDLE_SPACING`] est mesuré **après** cette conversion. Une ligne
    /// porte ses poignées le long de ses tronçons ([`along_path`]) ; sans
    /// ligne, les marqueurs puis les barres en reçoivent un sous-ensemble
    /// régulier ([`thinned`]).
    pub fn handle_centers(&self, to_display: impl Fn((f64, f64)) -> (f64, f64)) -> Vec<(f64, f64)> {
        if !self.paths.is_empty() {
            return self
                .paths
                .iter()
                .flat_map(|path| {
                    let path: Vec<_> = path.iter().map(|&p| to_display(p)).collect();
                    along_path(&path, HANDLE_SPACING)
                })
                .collect();
        }
        if !self.points.is_empty() {
            return thinned(
                self.points
                    .iter()
                    .map(|&p| to_display((p.0 as f64, p.1 as f64))),
                HANDLE_SPACING,
            );
        }
        thinned(
            self.bar_tips
                .iter()
                .map(|&p| to_display((p.0 as f64, p.1 as f64))),
            HANDLE_SPACING,
        )
    }
}

/// Points posés le long d'une polyligne, tous les `spacing` pixels de
/// longueur parcourue : le premier sur le départ, puis un à chaque
/// `spacing`, et l'arrivée en plus si la dernière poignée en est à plus d'un
/// demi-intervalle — les deux bouts d'un tronçon sont ainsi toujours
/// marqués. Un tronçon d'un seul point (ou de longueur nulle) en reçoit un.
pub fn along_path(path: &[(f64, f64)], spacing: f64) -> Vec<(f64, f64)> {
    let Some(&first) = path.first() else {
        return Vec::new();
    };
    let mut handles = vec![first];
    // Distance parcourue depuis la dernière poignée.
    let mut since = 0.0;
    for pair in path.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = (b.0 - a.0).hypot(b.1 - a.1);
        if !length.is_finite() || length == 0.0 {
            continue;
        }
        // Abscisse, sur ce segment, de la prochaine poignée.
        let mut next = spacing - since;
        while next <= length {
            let t = next / length;
            handles.push((a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)));
            next += spacing;
        }
        since = length - (next - spacing);
    }
    let last = path[path.len() - 1];
    if since > spacing / 2.0 {
        handles.push(last);
    }
    handles
}

/// Sous-ensemble régulier de positions discrètes (marqueurs, sommets de
/// barres), dans l'ordre : la première est gardée, puis toute position à
/// au moins `spacing` pixels de la dernière gardée.
pub fn thinned(positions: impl IntoIterator<Item = (f64, f64)>, spacing: f64) -> Vec<(f64, f64)> {
    let mut kept: Vec<(f64, f64)> = Vec::new();
    for p in positions {
        let far = kept
            .last()
            .is_none_or(|last| (p.0 - last.0).hypot(p.1 - last.1) >= spacing);
        if far {
            kept.push(p);
        }
    }
    kept
}

/// Le carré d'une poignée centrée sur `center`, en coins entiers (Plotters
/// raisonne en entiers) : côté [`HANDLE_SIDE`] exactement.
pub fn handle_square((x, y): (f64, f64)) -> (Pixel, Pixel) {
    let half = HANDLE_SIDE / 2.0;
    let left = (x - half).round() as i32;
    let top = (y - half).round() as i32;
    let side = HANDLE_SIDE as i32;
    ((left, top), (left + side, top + side))
}

/// Peint les poignées de sélection, des carrés pleins couleur `ink`, sur
/// n'importe quel backend — écrit une fois, ici, comme le reste du rendu
/// (invariant 5). Les centres sont dans le repère du backend.
pub fn paint_handles<DB: DrawingBackend>(
    backend: &mut DB,
    centers: &[(f64, f64)],
    ink: (u8, u8, u8),
) -> Result<(), DrawingErrorKind<DB::ErrorType>> {
    let color = RGBColor(ink.0, ink.1, ink.2);
    for &center in centers {
        let (upper_left, bottom_right) = handle_square(center);
        backend.draw_rect(upper_left, bottom_right, &color, true)?;
    }
    Ok(())
}

/// Indice du calque et rang d'origine dans le calque déclaré. À égalité, le dernier tracé gagne.
pub fn series_at(plot: &DrawnPlot, pixel: (f64, f64)) -> Option<(usize, usize)> {
    for panel in plot.panels.iter().rev() {
        if !PanelTransform::of(panel)?.contains(pixel.0, pixel.1) {
            continue;
        }
        let mut best = None;
        let mut distance = 5.0;
        for series in panel.series_geometry.iter().rev() {
            let candidate = series.distance(pixel);
            if candidate < distance {
                distance = candidate;
                best = Some((panel.index, series.index));
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn les_segments_et_les_barres_se_designent_entre_leurs_points() {
        let line = DrawnSeries {
            paths: vec![vec![(0.0, 0.0), (100.0, 100.0)]],
            ..Default::default()
        };
        assert!(line.distance((50.0, 50.0)) < 0.1);
        assert!(line.distance((20.0, 80.0)) > 30.0);
        let bars = DrawnSeries {
            bars: vec![((10, 20), (30, 70))],
            ..Default::default()
        };
        assert_eq!(bars.distance((20.0, 40.0)), 0.0);
        assert_eq!(bars.distance((40.0, 40.0)), 10.0);
        let whiskers = DrawnSeries {
            whiskers: vec![((10, 0), (10, 40))],
            ..Default::default()
        };
        assert_eq!(whiskers.distance((10.0, 20.0)), 0.0);
        assert!(DrawnSeries::default().distance((0.0, 0.0)).is_infinite());
    }

    #[test]
    fn les_poignees_sont_espacees_le_long_du_trace_pas_par_point() {
        // Un coude : 0 → 120 en x, puis 80 vers le bas. Longueur 200.
        let path = [(0.0, 0.0), (120.0, 0.0), (120.0, 80.0)];
        let handles = along_path(&path, 50.0);
        let expected = [
            (0.0, 0.0),
            (50.0, 0.0),
            (100.0, 0.0),
            // 20 px restants du premier segment + 30 px sur le second.
            (120.0, 30.0),
            (120.0, 80.0),
        ];
        assert_eq!(handles.len(), expected.len(), "{handles:?}");
        for (got, want) in handles.iter().zip(expected) {
            assert!(close(*got, want), "{got:?} ≠ {want:?}");
        }
        // Le même tracé découpé en cent points donne les mêmes poignées :
        // l'espacement ne dépend pas de la densité des données.
        let dense: Vec<_> = (0..=60)
            .map(|i| (i as f64 * 2.0, 0.0))
            .chain((1..=40).map(|i| (120.0, i as f64 * 2.0)))
            .collect();
        let dense_handles = along_path(&dense, 50.0);
        assert_eq!(dense_handles.len(), handles.len());
        for (got, want) in dense_handles.iter().zip(&handles) {
            assert!(close(*got, *want), "{got:?} ≠ {want:?}");
        }
    }

    #[test]
    fn l_arrivee_n_est_marquee_que_si_elle_est_loin_de_la_derniere_poignee() {
        // 110 px : poignées à 0, 50, 100 ; l'arrivée (10 px plus loin) ne
        // double pas la dernière.
        assert_eq!(along_path(&[(0.0, 0.0), (110.0, 0.0)], 50.0).len(), 3);
        // 140 px : l'arrivée est à 40 px de la poignée à 100, elle est marquée.
        let handles = along_path(&[(0.0, 0.0), (140.0, 0.0)], 50.0);
        assert_eq!(handles.len(), 4);
        assert!(close(handles[3], (140.0, 0.0)));
        // Un tronçon réduit à un point en reçoit une ; un chemin vide aucune.
        assert_eq!(along_path(&[(3.0, 4.0)], 50.0), vec![(3.0, 4.0)]);
        assert_eq!(
            along_path(&[(3.0, 4.0), (3.0, 4.0)], 50.0),
            vec![(3.0, 4.0)]
        );
        assert!(along_path(&[], 50.0).is_empty());
    }

    #[test]
    fn l_espacement_se_mesure_en_pixels_d_affichage() {
        let series = DrawnSeries {
            paths: vec![vec![(0.0, 0.0), (200.0, 0.0)]],
            ..Default::default()
        };
        let native = series.handle_centers(|(x, y)| (x, y));
        let doubled = series.handle_centers(|(x, y)| (x * 2.0, y * 2.0));
        assert_eq!(native.len(), 5);
        // Figure affichée deux fois plus grande : deux fois plus de poignées,
        // toujours à HANDLE_SPACING les unes des autres à l'écran.
        assert_eq!(doubled.len(), 9);
        assert!(close(doubled[1], (HANDLE_SPACING, 0.0)));
    }

    #[test]
    fn un_nuage_et_des_barres_portent_un_sous_ensemble_regulier() {
        // Marqueurs tous les 10 px : un sur cinq.
        let scatter = DrawnSeries {
            points: (0..=20).map(|i| (i * 10, 0)).collect(),
            ..Default::default()
        };
        let handles = scatter.handle_centers(|(x, y)| (x, y));
        assert_eq!(
            handles,
            vec![
                (0.0, 0.0),
                (50.0, 0.0),
                (100.0, 0.0),
                (150.0, 0.0),
                (200.0, 0.0)
            ]
        );
        // Barres de 60 px de pas : chacune la sienne, sur son extrémité libre.
        let bars = DrawnSeries {
            bars: vec![((0, 10), (40, 100)), ((60, 100), (100, 150))],
            bar_tips: vec![(20, 10), (80, 150)],
            ..Default::default()
        };
        assert_eq!(
            bars.handle_centers(|(x, y)| (x, y)),
            vec![(20.0, 10.0), (80.0, 150.0)]
        );
        // Une ligne avec marqueurs : les poignées suivent la ligne, pas les
        // marqueurs.
        let line_and_symbols = DrawnSeries {
            paths: vec![vec![(0.0, 0.0), (100.0, 0.0)]],
            points: vec![(0, 0), (7, 0), (100, 0)],
            ..Default::default()
        };
        assert_eq!(
            line_and_symbols.handle_centers(|(x, y)| (x, y)),
            vec![(0.0, 0.0), (50.0, 0.0), (100.0, 0.0)]
        );
        // Les moustaches seules n'en portent pas.
        let whiskers = DrawnSeries {
            whiskers: vec![((0, 0), (0, 100))],
            ..Default::default()
        };
        assert!(whiskers.handle_centers(|(x, y)| (x, y)).is_empty());
    }

    #[test]
    fn une_poignee_est_un_carre_de_cote_fixe_centre() {
        let ((left, top), (right, bottom)) = handle_square((100.0, 40.0));
        assert_eq!(right - left, HANDLE_SIDE as i32);
        assert_eq!(bottom - top, HANDLE_SIDE as i32);
        let center = ((left + right) as f64 / 2.0, (top + bottom) as f64 / 2.0);
        assert!((center.0 - 100.0).abs() <= 0.5 && (center.1 - 40.0).abs() <= 0.5);
    }
}
