//! Rognage géométrique d'une polyligne contre le rectangle de tracé, en
//! **pixels** — correctif du défaut documenté dans [`crate::paint`]
//! (« Le débordement est déjà traité, par Plotters — mais pas
//! correctement ») et reporté deux fois par `roadmap-rendu.md` (R5, puis
//! R7).
//!
//! # Pourquoi en pixels, et pas en données
//!
//! Un axe logarithmique n'est linéaire que dans l'espace transformé
//! ([`crate::scale::forward`]) ; une droite en coordonnées de données n'y
//! est donc pas forcément une droite en pixels, alors qu'elle **redevient**
//! une droite dès qu'elle est projetée en pixels (la projection pixel ↔
//! transformé est, elle, toujours affine — voir [`crate::interact`]). Une
//! intersection segment/rectangle est une opération sur des droites : elle
//! doit se faire là où le segment est effectivement rectiligne, c'est-à-dire
//! après la projection, jamais avant.
//!
//! # Pourquoi ici, et pas dans un backend
//!
//! Même raison que pour [`crate::paint`] et [`crate::pipeline`] : une seule
//! implémentation, appelée avant que le point n'atteigne
//! `plotters::element::PathElement`, plutôt qu'une discipline à tenir dans
//! chacun des deux `DrawingBackend`.
//!
//! # L'algorithme
//!
//! Liang-Barsky, sur un rectangle aligné aux axes — plus simple à établir
//! correctement que Cohen-Sutherland pour ce cas, et suffisant : le
//! rectangle de tracé n'est jamais incliné. [`clip_segment`] rogne un
//! segment ; [`clip_polyline`] l'applique à chaque paire de sommets
//! consécutifs d'une courbe et recolle les tronçons qui restent adjacents en
//! une seule sous-polyligne, exactement comme [`crate::paint::dashes`]
//! recolle des tronçons de tirets — un segment entièrement hors du
//! rectangle est omis, pas ramené sur le bord.

use plotters_backend::BackendCoord;

/// Le rectangle de tracé d'un panneau, en pixels de backend.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelBounds {
    pub x_min: f64,
    pub y_min: f64,
    pub x_max: f64,
    pub y_max: f64,
}

impl PixelBounds {
    /// Le rectangle d'un panneau tracé, depuis son origine et sa taille en
    /// pixels ([`crate::pipeline::DrawnPanel::pixel_origin`] /
    /// `pixel_size`). `None` si la zone est dégénérée — même garde que
    /// [`crate::interact::PanelTransform::of`].
    pub fn of_panel(origin: (i32, i32), size: (u32, u32)) -> Option<Self> {
        if size.0 == 0 || size.1 == 0 {
            return None;
        }
        Some(Self {
            x_min: origin.0 as f64,
            y_min: origin.1 as f64,
            x_max: origin.0 as f64 + size.0 as f64,
            y_max: origin.1 as f64 + size.1 as f64,
        })
    }
}

/// Rogne le segment `a`→`b` (en pixels) au rectangle `bounds` par
/// Liang-Barsky. Rend les deux extrémités du tronçon restant, ou `None` si
/// le segment ne traverse pas le rectangle.
///
/// Un point de `a` ou `b` non fini (`NaN`, infini) fait rejeter le segment
/// plutôt que produire un résultat non fini à son tour — cohérent avec la
/// façon dont [`crate::series::curve_geometry`] traite déjà les valeurs non
/// représentables en amont (elles n'entrent jamais dans la géométrie).
pub fn clip_segment(
    bounds: PixelBounds,
    a: (f64, f64),
    b: (f64, f64),
) -> Option<((f64, f64), (f64, f64))> {
    if !a.0.is_finite() || !a.1.is_finite() || !b.0.is_finite() || !b.1.is_finite() {
        return None;
    }
    let (x0, y0) = a;
    let (x1, y1) = b;
    let dx = x1 - x0;
    let dy = y1 - y0;

    let mut t0 = 0.0f64;
    let mut t1 = 1.0f64;

    // Les quatre demi-plans du rectangle : gauche, droite, haut, bas.
    let edges = [
        (-dx, x0 - bounds.x_min),
        (dx, bounds.x_max - x0),
        (-dy, y0 - bounds.y_min),
        (dy, bounds.y_max - y0),
    ];
    for (p, q) in edges {
        if p == 0.0 {
            // Le segment est parallèle à ce bord : rejeté s'il est du
            // mauvais côté, inchangé sinon.
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let r = q / p;
        if p < 0.0 {
            if r > t1 {
                return None;
            }
            if r > t0 {
                t0 = r;
            }
        } else {
            if r < t0 {
                return None;
            }
            if r < t1 {
                t1 = r;
            }
        }
    }
    if t0 > t1 {
        return None;
    }
    Some(((x0 + t0 * dx, y0 + t0 * dy), (x0 + t1 * dx, y0 + t1 * dy)))
}

/// Rogne une polyligne entière contre `bounds`, en pixels entiers.
///
/// Rend une ou plusieurs sous-polylignes — plusieurs quand la courbe sort du
/// rectangle et y rentre à nouveau, aucune si elle lui est entièrement
/// extérieure. Deux tronçons rognés consécutifs sont recollés en une seule
/// sous-polyligne quand le second commence exactement là où le premier finit
/// (le sommet commun est resté à l'intérieur du rectangle) ; sinon chacun
/// forme sa propre sous-polyligne.
pub fn clip_polyline(bounds: PixelBounds, points: &[BackendCoord]) -> Vec<Vec<BackendCoord>> {
    let mut runs: Vec<Vec<BackendCoord>> = Vec::new();
    for pair in points.windows(2) {
        let a = (pair[0].0 as f64, pair[0].1 as f64);
        let b = (pair[1].0 as f64, pair[1].1 as f64);
        let Some((ca, cb)) = clip_segment(bounds, a, b) else {
            continue;
        };
        let pa = round(ca);
        let pb = round(cb);
        if pa == pb {
            // Le rectangle n'a été effleuré qu'en un point (un coin, ou un
            // segment tangent à un bord) : rien à tracer.
            continue;
        }
        match runs.last_mut() {
            Some(run) if run.last() == Some(&pa) => run.push(pb),
            _ => runs.push(vec![pa, pb]),
        }
    }
    runs
}

/// Comme [`clip_polyline`], sur des sommets en pixels **flottants** : rien
/// n'est arrondi, pour les chemins sous-pixel ([`crate::subpixel`]).
pub fn clip_polyline_f64(bounds: PixelBounds, points: &[(f64, f64)]) -> Vec<Vec<(f64, f64)>> {
    let mut runs: Vec<Vec<(f64, f64)>> = Vec::new();
    for pair in points.windows(2) {
        let Some((ca, cb)) = clip_segment(bounds, pair[0], pair[1]) else {
            continue;
        };
        if ca == cb {
            continue;
        }
        match runs.last_mut() {
            Some(run) if run.last() == Some(&ca) => run.push(cb),
            _ => runs.push(vec![ca, cb]),
        }
    }
    runs
}

fn round((x, y): (f64, f64)) -> BackendCoord {
    (x.round() as i32, y.round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le rectangle 0..100 × 0..100, en pixels — le cas courant des tests.
    fn carre() -> PixelBounds {
        PixelBounds {
            x_min: 0.0,
            y_min: 0.0,
            x_max: 100.0,
            y_max: 100.0,
        }
    }

    #[test]
    fn un_segment_entierement_dedans_est_inchange() {
        let bounds = carre();
        let result = clip_segment(bounds, (10.0, 20.0), (80.0, 90.0)).unwrap();
        assert_eq!(result, ((10.0, 20.0), (80.0, 90.0)));
    }

    #[test]
    fn un_segment_entierement_dehors_est_elimine() {
        let bounds = carre();
        // Toute la droite x = 200 est à droite du rectangle.
        assert!(clip_segment(bounds, (200.0, -50.0), (200.0, 150.0)).is_none());
        // Un segment purement diagonal, entièrement au-delà du coin.
        assert!(clip_segment(bounds, (150.0, 150.0), (300.0, 300.0)).is_none());
    }

    #[test]
    fn un_segment_qui_traverse_le_cadre_est_coupe_au_bon_point() {
        let bounds = carre();
        // De (-50, 50) à (150, 50) : horizontal, coupe x=0 et x=100 à
        // y = 50 des deux côtés — calculable à la main.
        let (start, end) = clip_segment(bounds, (-50.0, 50.0), (150.0, 50.0)).unwrap();
        assert_eq!(start, (0.0, 50.0));
        assert_eq!(end, (100.0, 50.0));
    }

    #[test]
    fn un_segment_diagonal_traverse_est_coupe_a_lintersection_geometrique() {
        let bounds = carre();
        // De (50, 50) [dedans] à (200, 200) [dehors, en diagonale] : la
        // droite y = x sort du carré en (100, 100), pas en (100, quelque
        // chose de tronqué par composante) ni en (quelque chose, 100).
        let (start, end) = clip_segment(bounds, (50.0, 50.0), (200.0, 200.0)).unwrap();
        assert_eq!(start, (50.0, 50.0));
        assert_eq!(end, (100.0, 100.0));
    }

    #[test]
    fn un_segment_qui_touche_juste_un_bord_ne_plante_pas() {
        let bounds = carre();
        // Tangent au bord haut (y = 0) sans jamais entrer dans le carré.
        let result = clip_segment(bounds, (-10.0, 0.0), (110.0, 0.0));
        assert_eq!(result, Some(((0.0, 0.0), (100.0, 0.0))));

        // Un point exactement sur un coin, l'autre loin dehors : le
        // tronçon rogné est un point unique (t0 == t1) — pas un crash, et
        // `clip_polyline` doit alors ne rien produire pour ce segment.
        let bounds_line = clip_segment(bounds, (100.0, 100.0), (200.0, 200.0));
        assert_eq!(bounds_line, Some(((100.0, 100.0), (100.0, 100.0))));
    }

    #[test]
    fn une_polyligne_hors_cadre_disparait_sans_segment_parasite() {
        let bounds = carre();
        let points = [(200, 200), (300, 300), (400, 100)];
        assert!(clip_polyline(bounds, &points).is_empty());
    }

    #[test]
    fn une_polyligne_qui_sort_et_rentre_donne_plusieurs_troncons() {
        let bounds = carre();
        // Dedans, sort à droite, revient dedans par le bas : deux
        // sous-polylignes disjointes, ni l'une ni l'autre plaquée sur un
        // bord.
        let points = [(50, 50), (200, 50), (50, 200), (50, 50)];
        let runs = clip_polyline(bounds, &points);
        assert_eq!(runs.len(), 2, "{runs:?}");
        for run in &runs {
            for &(x, y) in run {
                assert!((0..=100).contains(&x) && (0..=100).contains(&y), "{run:?}");
            }
        }
    }

    #[test]
    fn une_polyligne_entierement_dedans_recolle_tous_ses_troncons() {
        let bounds = carre();
        let points = [(10, 10), (50, 60), (90, 20)];
        let runs = clip_polyline(bounds, &points);
        assert_eq!(runs, vec![vec![(10, 10), (50, 60), (90, 20)]]);
    }

    #[test]
    fn le_rectangle_dune_zone_degeneree_est_absent() {
        assert!(PixelBounds::of_panel((0, 0), (0, 10)).is_none());
        assert!(PixelBounds::of_panel((0, 0), (10, 0)).is_none());
        assert!(PixelBounds::of_panel((5, 5), (10, 10)).is_some());
    }
}
