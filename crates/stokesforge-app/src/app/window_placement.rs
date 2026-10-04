//! Placement des fenêtres flottantes : tailles par défaut, cascade, espace
//! de travail à taille fixe (`WORKSPACE_SIZE`), bornage d'un rectangle
//! logique (`clamp_to_workspace`), conversion logique → écran et défilement
//! pour révéler une fenêtre.
//!
//! Déplacé tel quel d'`app.rs` (#101), sans changement de comportement. Les
//! positions et tailles sont en coordonnées **logiques** de l'espace de
//! travail ; la conversion en coordonnées écran n'a lieu qu'au moment de
//! peindre (`workspace_to_screen`).

use eframe::egui;
use stokesforge_session::WindowGeometry;

/// Taille par défaut des fenêtres flottantes qui n'affichent pas un graphe —
/// voir la doc de tête du module, § Fenêtres flottantes confinées. Rognée à
/// l'aire de travail disponible si elle est plus petite.
pub(super) const FLOATING_WINDOW_SIZE: egui::Vec2 = egui::vec2(560.0, 420.0);

/// Taille par défaut d'une fenêtre de graphe : la figure de référence
/// (`480×360`), son retour au cadrage initial, l'en-tête et les marges y
/// entrent tous dès l'ouverture.
pub(super) const GRAPH_WINDOW_SIZE: egui::Vec2 = egui::vec2(560.0, 460.0);

/// Largeur par défaut d'une fenêtre flottante [`Tab::Book`] — le tiers de
/// moins que [`FLOATING_WINDOW_SIZE`] : une grille n'a pas besoin d'être
/// aussi large qu'un graphe pour rester lisible, demande explicite.
pub(super) const BOOK_WINDOW_WIDTH: f32 = FLOATING_WINDOW_SIZE.x * 2.0 / 3.0;

/// Décalage, en points, entre deux fenêtres flottantes ouvertes l'une après
/// l'autre (cascade) — voir [`StokesForgeApp::next_cascade`].
pub(super) const CASCADE_STEP: f32 = 24.0;

/// Nombre de crans de cascade avant de reboucler à zéro, pour qu'une
/// succession de fenêtres ne dérive jamais hors de l'aire de travail.
pub(super) const CASCADE_WRAP: u32 = 8;

/// Rectangle flottant d'une géométrie persistée.
pub(super) fn geometry_rect(geometry: &WindowGeometry) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(geometry.x as f32, geometry.y as f32),
        egui::vec2(geometry.width as f32, geometry.height as f32),
    )
}

/// Taille à peindre pour une fenêtre flottante cette image. La taille logique (`current_size`) telle quelle, sauf le temps
/// d'un repli (B3, `retours-2026-09-23-bugs.md`) : le contenu
/// n'est alors plus peint (seule [`window_header`] l'est), et forcer la
/// taille dépliée peindrait un cadre — et un contour de surbrillance —
/// autour d'une zone vide.
///
/// Réutilisée telle quelle par [`StokesForgeApp::show_detached_tabs`] (B9) pour
/// calculer la taille à envoyer au viewport OS via
/// `ViewportCommand::InnerSize` : même bande d'en-tête, même hauteur
/// repliée — seul change ce qui porte cette taille (le cadre peint par
/// [`show_floating_window`] côté flottant, un viewport OS réel côté détaché).
pub(super) fn floating_window_display_size(
    current_size: egui::Vec2,
    was_collapsed: bool,
    header_height: f32,
    window_margin: egui::Vec2,
) -> egui::Vec2 {
    if was_collapsed {
        egui::vec2(current_size.x, header_height + window_margin.y)
    } else {
        current_size
    }
}

/// Rectangle à retenir dans `window_geometries` (persisté en `.stf`) à
/// partir du rectangle réellement peint cette image. La position vient de
/// ce rectangle, mais la taille doit rester la taille **logique** de la
/// fenêtre : pendant un repli (B3), le rectangle réellement peint est réduit
/// à [`floating_window_display_size`], et le persister tel quel referait
/// réapparaître la fenêtre repliée — y compris une fois dépliée à nouveau —
/// à la prochaine ouverture du projet.
///
/// Réutilisée telle quelle par [`StokesForgeApp::show_detached_tabs`] (B9,
/// `retours-2026-09-23-bugs.md`) avec
/// `DetachedTabViewModel::expanded_size` en guise de taille logique : le
/// rectangle rapporté par le viewport OS pendant un repli est, là aussi,
/// le rectangle réduit que l'`InnerSize` envoyé au bascule a produit, pas
/// la taille à persister.
pub(super) fn floating_window_geometry_to_persist(
    painted_rect: egui::Rect,
    current_size: egui::Vec2,
) -> egui::Rect {
    egui::Rect::from_min_size(painted_rect.min, current_size)
}

/// Taille fixe de l'espace de travail (zone MDI qui porte les fenêtres
/// flottantes), indépendante de la taille de la fenêtre principale — avant
/// cette tâche, `workspace_rect` était `ui.max_rect()`, donc rétrécissait
/// avec la fenêtre `eframe` et écrasait les fenêtres flottantes contre ses
/// bords. Un banc « généreux » plutôt qu'ajusté à un écran précis ; devenir
/// réellement réglable par l'utilisateur (persistance, UI de réglage) reste à
/// faire — voir `docs/chantiers`. Sous cette taille, [`StokesForgeApp::ui`] fait
/// défiler l'espace de travail ([`StokesForgeApp::workspace_scroll`]) plutôt que de
/// rétrécir ou tronquer les fenêtres qu'il contient.
pub(super) const WORKSPACE_SIZE: egui::Vec2 = egui::vec2(1920.0, 1080.0);

/// Défilement maximal (par axe) pour qu'une zone visible de taille
/// `viewport_size` reste entièrement dans [`WORKSPACE_SIZE`] — nul sur un axe
/// où l'espace de travail entre déjà en entier, rien n'y défile alors.
pub(super) fn workspace_max_scroll(viewport_size: egui::Vec2) -> egui::Vec2 {
    (WORKSPACE_SIZE - viewport_size).max(egui::Vec2::ZERO)
}

/// Position écran d'un point de l'espace de travail (coordonnées logiques,
/// indépendantes du défilement) — l'origine de la zone visible moins le
/// défilement courant.
pub(super) fn workspace_to_screen(
    pos: egui::Pos2,
    viewport_min: egui::Pos2,
    scroll: egui::Vec2,
) -> egui::Pos2 {
    viewport_min + (pos.to_vec2() - scroll)
}

/// Défilement minimal à appliquer à `scroll` pour que `target` (rectangle
/// dans l'espace de travail) redevienne entièrement visible dans une zone de
/// taille `viewport_size` — inchangé si `target` l'est déjà. Utilisé pour
/// ramener à l'écran une fenêtre déjà ouverte mais actuellement défilée hors
/// vue (clic dans le Project Manager) : contrairement à
/// [`Response::scroll_to_me`] côté grille/onglets de feuille, l'espace de
/// travail n'est pas un `egui::ScrollArea` classique (les fenêtres qu'il
/// porte peignent sur leur propre calque, indépendant de toute zone de
/// défilement ambiante), donc pas de mécanisme natif à réutiliser ici.
pub(super) fn workspace_scroll_to_reveal(
    scroll: egui::Vec2,
    viewport_size: egui::Vec2,
    target: egui::Rect,
) -> egui::Vec2 {
    let max_scroll = workspace_max_scroll(viewport_size);
    let mut revealed = scroll;
    for axis in 0..2 {
        if target.min[axis] < revealed[axis] {
            revealed[axis] = target.min[axis];
        } else if target.max[axis] > revealed[axis] + viewport_size[axis] {
            revealed[axis] = target.max[axis] - viewport_size[axis];
        }
        revealed[axis] = revealed[axis].clamp(0.0, max_scroll[axis]);
    }
    revealed
}

/// Borne un rectangle logique à l'espace de travail : taille ramenée à
/// [`WORKSPACE_SIZE`] au plus, puis position glissée pour qu'il y tienne en
/// entier. C'est la seule borne appliquée aux fenêtres flottantes — egui
/// n'en ajoute pas d'autre (voir [`show_floating_window`]), donc ce que
/// cette fonction rend est exactement ce qui est peint, entouré et persisté.
pub(super) fn clamp_to_workspace(rect: egui::Rect) -> egui::Rect {
    let size = rect.size().min(WORKSPACE_SIZE);
    let max_min = (WORKSPACE_SIZE - size).to_pos2();
    egui::Rect::from_min_size(rect.min.clamp(egui::Pos2::ZERO, max_min), size)
}

#[cfg(test)]
mod tests {
    use super::*;

    // B3 (`retours-2026-09-23-bugs.md`) : une fenêtre repliée
    // gardait un contour à sa taille dépliée, un grand cadre vert autour
    // d'une zone vide — parce que la fenêtre était forcée (`.fixed_size`) à
    // sa taille dépliée même quand seule sa bande d'en-tête est peinte.
    #[test]
    fn la_taille_demandee_a_egui_se_reduit_a_len_tete_pendant_un_repli() {
        let current_size = egui::vec2(400.0, 300.0);
        let window_margin = egui::vec2(12.0, 12.0);

        let deplie = floating_window_display_size(current_size, false, 28.0, window_margin);
        assert_eq!(
            deplie, current_size,
            "dépliée, la fenêtre garde sa taille logique"
        );

        let replie = floating_window_display_size(current_size, true, 28.0, window_margin);
        assert_eq!(
            replie,
            egui::vec2(400.0, 40.0),
            "repliée, seule la bande d'en-tête (+ marges du Frame) doit être demandée à egui"
        );
        assert!(
            replie.y < current_size.y,
            "la taille demandée doit être strictement plus petite que la taille dépliée"
        );
        // Seule la marge verticale compte : la largeur repliée ne dépend
        // jamais de la marge horizontale.
        assert_eq!(
            floating_window_display_size(current_size, true, 28.0, egui::vec2(50.0, 12.0)),
            replie
        );
    }

    #[test]
    fn workspace_to_screen_decale_de_lorigine_de_la_vue_et_du_defilement() {
        let viewport_min = egui::pos2(40.0, 20.0);
        assert_eq!(
            workspace_to_screen(
                egui::pos2(500.0, 400.0),
                viewport_min,
                egui::vec2(300.0, 150.0)
            ),
            egui::pos2(240.0, 270.0)
        );
        // Sans défilement, l'espace de travail et l'écran ne diffèrent que
        // par l'origine de la zone visible — comportement identique à
        // l'ancien `workspace_rect.min + position`.
        assert_eq!(
            workspace_to_screen(egui::pos2(10.0, 5.0), viewport_min, egui::Vec2::ZERO),
            viewport_min + egui::vec2(10.0, 5.0)
        );
    }

    #[test]
    fn workspace_max_scroll_est_nul_quand_lespace_de_travail_tient_deja() {
        assert_eq!(
            workspace_max_scroll(WORKSPACE_SIZE),
            egui::Vec2::ZERO,
            "la zone visible fait déjà la taille de l'espace de travail : rien à défiler"
        );
        assert_eq!(
            workspace_max_scroll(WORKSPACE_SIZE * 2.0),
            egui::Vec2::ZERO,
            "une zone visible plus grande que l'espace de travail ne doit jamais produire \
             de défilement négatif"
        );
        let viewport = egui::vec2(800.0, 600.0);
        assert_eq!(workspace_max_scroll(viewport), WORKSPACE_SIZE - viewport);
        // Chaque axe pour lui-même : une vue plus large que l'espace de
        // travail mais moins haute ne défile que verticalement.
        assert_eq!(
            workspace_max_scroll(egui::vec2(WORKSPACE_SIZE.x + 100.0, 600.0)),
            egui::vec2(0.0, WORKSPACE_SIZE.y - 600.0)
        );
    }

    #[test]
    fn workspace_scroll_to_reveal_ne_bouge_pas_si_deja_visible() {
        let viewport_size = egui::vec2(800.0, 600.0);
        let scroll = egui::vec2(100.0, 50.0);
        let visible = egui::Rect::from_min_size(egui::pos2(200.0, 100.0), egui::vec2(300.0, 200.0));
        assert_eq!(
            workspace_scroll_to_reveal(scroll, viewport_size, visible),
            scroll,
            "une fenêtre déjà entièrement dans la zone visible ne doit pas faire défiler"
        );
        // Une fenêtre qui longe exactement les bords de la vue est visible
        // elle aussi : aucun défilement d'un point.
        let bord_a_bord = egui::Rect::from_min_size(scroll.to_pos2(), viewport_size);
        assert_eq!(
            workspace_scroll_to_reveal(scroll, viewport_size, bord_a_bord),
            scroll
        );
    }

    #[test]
    fn workspace_scroll_to_reveal_ramene_une_fenetre_hors_vue_a_lecran() {
        let viewport_size = egui::vec2(800.0, 600.0);
        let scroll = egui::Vec2::ZERO;

        // Hors vue à droite/en bas : défiler juste assez pour longer le bord
        // le plus proche de la fenêtre visée, pas la centrer arbitrairement.
        let apres_droite =
            egui::Rect::from_min_size(egui::pos2(1200.0, 50.0), egui::vec2(100.0, 80.0));
        let revealed = workspace_scroll_to_reveal(scroll, viewport_size, apres_droite);
        assert_eq!(revealed.x, apres_droite.max.x - viewport_size.x);
        assert_eq!(revealed.y, 0.0, "l'axe vertical était déjà visible");
        let target_screen = egui::Rect::from_min_size(
            apres_droite.min.to_vec2().to_pos2() - revealed,
            apres_droite.size(),
        );
        assert!(
            egui::Rect::from_min_size(egui::Pos2::ZERO, viewport_size).contains_rect(target_screen),
            "après défilement, la fenêtre doit tenir dans la zone visible : {target_screen:?}"
        );

        // Hors vue à gauche/en haut (défilement déjà avancé).
        let scroll = egui::vec2(500.0, 400.0);
        let avant_gauche =
            egui::Rect::from_min_size(egui::pos2(50.0, 20.0), egui::vec2(100.0, 80.0));
        let revealed = workspace_scroll_to_reveal(scroll, viewport_size, avant_gauche);
        assert_eq!(revealed.x, avant_gauche.min.x);
        assert_eq!(revealed.y, avant_gauche.min.y);

        // Toujours borné à [0, max_scroll], même pour une cible qui déborde
        // l'espace de travail (fenêtre plus grande que lui, cas dégénéré) :
        // le défilement s'arrête exactement à la borne, sans la dépasser.
        let max_scroll = workspace_max_scroll(viewport_size);
        let hors_bornes =
            egui::Rect::from_min_size(WORKSPACE_SIZE.to_pos2(), egui::vec2(50.0, 50.0));
        let revealed = workspace_scroll_to_reveal(egui::Vec2::ZERO, viewport_size, hors_bornes);
        assert_eq!(revealed, max_scroll);
    }

    #[test]
    fn la_geometrie_persistee_garde_la_taille_logique_meme_repliee() {
        // Le rectangle réellement peint cette image (`area_rect`) est celui
        // d'une fenêtre repliée — plus petit que sa taille logique.
        let painted_rect =
            egui::Rect::from_min_size(egui::pos2(50.0, 60.0), egui::vec2(400.0, 40.0));
        let current_size = egui::vec2(400.0, 300.0);

        let persisted = floating_window_geometry_to_persist(painted_rect, current_size);

        assert_eq!(
            persisted,
            egui::Rect::from_min_size(painted_rect.min, current_size),
            "la géométrie sauvegardée doit garder la taille dépliée, sinon un projet rechargé \
             rouvrirait la fenêtre minuscule même après avoir cliqué sur « déplier »"
        );
    }

    // Bug 1 (`retours-2026-09-24-bugs.md`), la cause : la
    // position était bornée deux fois, par egui (`constrain_to`) et pas par
    // nous, si bien que le contour suivait la position demandée et le cadre
    // la position recalée. Il n'y a plus qu'une borne, celle-ci.
    #[test]
    fn une_position_hors_de_lespace_de_travail_est_bornee_par_une_seule_fonction() {
        let size = egui::vec2(500.0, 400.0);
        assert_eq!(
            clamp_to_workspace(egui::Rect::from_min_size(egui::pos2(-300.0, 80.0), size)),
            egui::Rect::from_min_size(egui::pos2(0.0, 80.0), size),
        );
        assert_eq!(
            clamp_to_workspace(egui::Rect::from_min_size(egui::pos2(1800.0, 1000.0), size)).max,
            WORKSPACE_SIZE.to_pos2(),
        );
        assert_eq!(
            clamp_to_workspace(egui::Rect::from_min_size(
                egui::pos2(10.0, 10.0),
                egui::vec2(5000.0, 5000.0),
            )),
            egui::Rect::from_min_size(egui::Pos2::ZERO, WORKSPACE_SIZE),
        );
        // Le témoin : une fenêtre déjà dans l'espace de travail ne bouge pas
        // d'un point, même collée à son coin opposé.
        for legale in [
            egui::Rect::from_min_size(egui::pos2(120.0, 80.0), size),
            egui::Rect::from_min_size((WORKSPACE_SIZE - size).to_pos2(), size),
        ] {
            assert_eq!(clamp_to_workspace(legale), legale);
        }
    }
}
