//! Décimation LTTB — tâche R5.5.
//!
//! # Ce que la mesure R0 0.3 dit, et ce qu'elle ne dit pas
//!
//! Le seuil de décision était fixé d'avance (8 ms par image de
//! tessellation), il est franchi (22,1 ms), donc le LTTB entre en R5 : on
//! conclut selon le seuil plutôt que de le renégocier après coup. **Mais la
//! même mesure montre que le LTTB ne règle pas le cas mesuré**, et c'est
//! écrit ici plutôt que dilué dans un bilan :
//!
//! | séries | points/série | total | tessellation | ce que le LTTB retire |
//! |---|---|---|---|---|
//! | 50 | 6 000 | 300 k | 18,3 ms | ~73 % des points (≈ 6,5 ms) |
//! | 500 | 600 | 300 k | 22,7 ms | **rien** |
//!
//! 600 points étalés sur 1 600 px sont déjà sous-échantillonnés : il n'y a
//! rien à décimer. Le cas de charge du document — 500 spectres de 600 points
//! — tient sa charge du **nombre de séries**, pas de leur longueur, et le
//! levier qui le couvre est le *feathering* (78 % du coût de tessellation,
//! `stokesforge_render_egui::feathering`), pas celui-ci.
//!
//! Ce module sert donc **l'autre cas**, celui de l'acquisition : quelques
//! séries très longues. Là, il est le bon outil, et le seul qui ne demande
//! rien au reste de l'architecture.
//!
//! # Pourquoi LTTB et pas « un point sur N »
//!
//! *Largest Triangle Three Buckets* choisit, dans chaque tranche, le point
//! qui forme le plus grand triangle avec le point retenu à gauche et la
//! moyenne de la tranche de droite. Concrètement, il **garde les pics** : un
//! artefact d'une seule mesure — exactement ce qu'un spectre sert à voir —
//! survit à la décimation, là où un pas fixe l'efface une fois sur deux.
//!
//! Deux propriétés tenues, et testées :
//!
//! - **aucune valeur inventée** : toute sortie est un point d'entrée, jamais
//!   une moyenne. Un réticule qui désigne un point désigne une mesure réelle
//!   (ce dont la phase R7 aura besoin) ;
//! - **les extrémités sont conservées**, donc le cadrage de l'axe ne bouge
//!   pas du fait de la décimation.
//!
//! # Où il s'applique, et où il ne s'applique pas
//!
//! Dans le pipeline, juste avant de tracer, parce que le seuil dépend de la
//! **largeur en pixels** de la zone de tracé — une information qui n'existe
//! pas avant que Plotters ait réparti les marges. Il s'applique aux séries
//! qui tracent un trait et rien d'autre : pas aux barres (dont le nombre est
//! déjà celui des classes), pas aux séries à barres d'erreur (décimer
//! dissocierait les moustaches de leurs points).

/// Nombre de points en deçà duquel décimer n'a pas de sens.
///
/// L'algorithme lui-même a besoin d'au moins trois points (deux extrémités
/// et une tranche) ; en deçà il rend l'entrée telle quelle.
pub const MIN_POINTS: usize = 3;

/// Combien de points par pixel de large on accepte de garder.
///
/// Deux : un tracé anti-crénelé distingue encore un aller-retour dans un
/// même pixel, et descendre à un ferait perdre l'épaisseur visuelle d'un
/// signal bruité. Au-delà de deux, on paye des sommets que l'écran ne peut
/// pas montrer.
pub const POINTS_PER_PIXEL: usize = 2;

/// Le seuil de décimation pour une zone de tracé de `width` pixels de large.
pub fn threshold_for_width(width: u32) -> usize {
    (width as usize)
        .saturating_mul(POINTS_PER_PIXEL)
        .max(MIN_POINTS)
}

/// Décime `points` à au plus `threshold` points, par *Largest Triangle Three
/// Buckets*.
///
/// Rend l'entrée telle quelle — sans allouer autrement que par le `Vec` de
/// sortie — quand elle est déjà assez courte, ou quand le seuil demandé est
/// trop petit pour que l'algorithme ait un sens.
pub fn lttb(points: &[(f64, f64)], threshold: usize) -> Vec<(f64, f64)> {
    if threshold < MIN_POINTS || points.len() <= threshold {
        return points.to_vec();
    }

    let n = points.len();
    let last = n - 1;
    // Largeur d'une tranche, en nombre de points. Fractionnaire à dessein :
    // arrondir ici ferait dériver les tranches et perdrait les derniers
    // points dans la dernière.
    let bucket = (n - 2) as f64 / (threshold - 2) as f64;

    let mut out = Vec::with_capacity(threshold);
    out.push(points[0]);

    // Point retenu à gauche : le sommet commun à tous les triangles de la
    // tranche courante.
    let mut anchor = 0usize;

    for i in 0..threshold - 2 {
        // Moyenne de la tranche **suivante**, qui sert de troisième sommet.
        let next_start = (((i + 1) as f64 * bucket).floor() as usize + 1).min(last);
        let next_end = (((i + 2) as f64 * bucket).floor() as usize + 1).min(n);
        let (mean_x, mean_y) = mean(&points[next_start..next_end.max(next_start)])
            .unwrap_or((points[last].0, points[last].1));

        // Tranche courante, dans laquelle on choisit.
        let start = ((i as f64 * bucket).floor() as usize + 1).min(last);
        let end = (((i + 1) as f64 * bucket).floor() as usize + 1).min(last);

        let (ax, ay) = points[anchor];
        let mut best = start;
        let mut best_area = f64::NEG_INFINITY;
        for (offset, &(x, y)) in points[start..end.max(start)].iter().enumerate() {
            // Deux fois l'aire du triangle (ancre, candidat, moyenne) : le
            // facteur 1/2 est commun à tous les candidats, donc inutile.
            let area = ((ax - mean_x) * (y - ay) - (ax - x) * (mean_y - ay)).abs();
            if area > best_area {
                best_area = area;
                best = start + offset;
            }
        }
        out.push(points[best]);
        anchor = best;
    }

    out.push(points[last]);
    out
}

/// Moyenne d'une tranche, ou `None` si elle est vide.
fn mean(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    if points.is_empty() {
        return None;
    }
    let (mut sx, mut sy) = (0.0, 0.0);
    for (x, y) in points {
        sx += x;
        sy += y;
    }
    let n = points.len() as f64;
    Some((sx / n, sy / n))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rampe(n: usize) -> Vec<(f64, f64)> {
        (0..n).map(|i| (i as f64, (i as f64).sin())).collect()
    }

    #[test]
    fn une_serie_deja_courte_ressort_intacte() {
        let points = rampe(10);
        assert_eq!(lttb(&points, 10), points);
        assert_eq!(lttb(&points, 100), points);
    }

    #[test]
    fn un_seuil_absurde_ne_decime_pas() {
        let points = rampe(1000);
        assert_eq!(lttb(&points, 0).len(), 1000);
        assert_eq!(lttb(&points, 2).len(), 1000);
    }

    #[test]
    fn le_nombre_de_points_est_exactement_le_seuil() {
        for seuil in [3usize, 17, 100, 999] {
            assert_eq!(lttb(&rampe(10_000), seuil).len(), seuil, "seuil {seuil}");
        }
    }

    #[test]
    fn les_extremites_sont_conservees() {
        let points = rampe(5000);
        let decime = lttb(&points, 50);
        assert_eq!(decime.first().copied(), points.first().copied());
        assert_eq!(decime.last().copied(), points.last().copied());
    }

    #[test]
    fn aucune_valeur_nest_inventee() {
        // La propriété qui distingue le LTTB d'un moyennage : tout point
        // rendu est un point mesuré. R7 en dépendra pour la désignation de
        // point.
        let points = rampe(2000);
        let decime = lttb(&points, 61);
        for point in &decime {
            assert!(
                points.contains(point),
                "{point:?} n'est pas un point d'entrée"
            );
        }
    }

    #[test]
    fn labscisse_reste_croissante() {
        let points = rampe(3000);
        let decime = lttb(&points, 120);
        assert!(
            decime.windows(2).all(|p| p[0].0 < p[1].0),
            "le LTTB doit conserver l'ordre de parcours"
        );
    }

    #[test]
    fn un_pic_isole_survit_a_la_decimation() {
        // La raison d'être du LTTB par rapport à « un point sur N » : un
        // artefact d'une seule mesure — ce qu'un spectre sert à voir — ne
        // doit pas disparaître.
        let mut points: Vec<(f64, f64)> = (0..1000).map(|i| (i as f64, 0.0)).collect();
        points[437].1 = 42.0;
        let decime = lttb(&points, 50);
        assert!(
            decime.contains(&(437.0, 42.0)),
            "le pic a été effacé par la décimation"
        );
    }

    #[test]
    fn le_seuil_suit_la_largeur() {
        assert_eq!(threshold_for_width(800), 1600);
        // Une zone de tracé dégénérée ne doit pas donner un seuil de zéro,
        // qui ferait retomber sur « pas de décimation » par accident.
        assert_eq!(threshold_for_width(0), MIN_POINTS);
    }
}
