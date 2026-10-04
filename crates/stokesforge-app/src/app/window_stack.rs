//! Pile des fenêtres de contenu : l'onglet [`Tab`] qui désigne une fenêtre,
//! l'ordre de pile `floating_tabs` (le dernier est l'onglet actif), son
//! alignement sur les calques d'egui, le masquage, le repli, la maximisation
//! et le dessin de l'espace de travail (`show_workspace`).
//!
//! Déplacé tel quel d'`app.rs` (#101), sans changement de comportement.
//!
//! # Repris de la doc de tête d'`app.rs`
//!
//! # Fenêtres flottantes confinées
//!
//! Les quatre natures de fenêtre ci-dessus s'ouvrent par défaut en fenêtres
//! flottantes à l'intérieur du [`egui::CentralPanel`], plutôt qu'amarrées en
//! tuiles plein cadre — une par [`StokesForgeApp::floating_tabs`], jamais
//! dupliquée (même invariant que l'ancien dock : [`StokesForgeApp::open_tab`]
//! tranche « ouvrir » vs « focaliser »). Ni le Project Manager ni la console
//! de script n'y participent, voir les deux sections précédentes : ce sont
//! des panneaux fixes peints *avant* le `CentralPanel`.
//!
//! **Une `egui::Area` par fenêtre, pas une `egui::Window`** : voir la doc de
//! [`show_floating_window`]. `egui::Window` est une boîte de dialogue ; ses
//! fonctions (barre de titre, glisser, redimensionnement, bornage) avaient
//! toutes été coupées et refaites ici, et chaque image devait accorder le
//! rectangle demandé avec celui qu'egui recalait — la source commune des
//! bugs de contour et de débordement des rapports du 2026-09-23 et du
//! 2026-09-24. Désormais ce module possède seul la géométrie : cadre, contour,
//! rognage, poignées et géométrie persistée viennent du même rectangle.
//!
//! **Espace de travail défilable** : l'aire qui porte les fenêtres
//! flottantes a une taille logique **fixe** (`WORKSPACE_SIZE`), indépendante
//! de la fenêtre `eframe` — avant cette tâche, cette aire était `ui.max_rect()`
//! du `CentralPanel`, donc rétrécissait avec la fenêtre principale et
//! écrasait les fenêtres flottantes contre ses bords. `window_positions`/
//! `window_sizes` (ci-dessous) et la géométrie persistée dans le projet sont
//! exprimées dans ces coordonnées logiques ; [`StokesForgeApp::ui`] les convertit
//! en coordonnées écran juste avant la peinture (`workspace_to_screen`), à
//! partir de `viewport_rect` (ce que laissent les panneaux fixes et les
//! barres de défilement) et de [`StokesForgeApp::workspace_scroll`] (0,0 au
//! démarrage). Sous `WORKSPACE_SIZE`, `viewport_rect` ne montre donc qu'une
//! portion de l'espace de travail — un `egui::ScrollArea` invisible (rien
//! peint dans son contenu, seulement réservé à `WORKSPACE_SIZE`) donne
//! gratuitement poignées de défilement/molette et alimente
//! `workspace_scroll` ; les fenêtres flottantes, elles, peignent chacune sur
//! leur propre calque et ne peuvent pas vivre à l'intérieur de cette zone de
//! défilement, d'où la conversion manuelle.
//!
//! **Confinement** : la position logique de chaque fenêtre est bornée à
//! l'espace de travail par [`clamp_to_workspace`], et réécrite telle quelle —
//! c'est la seule borne. Le rendu, lui, est rogné à `viewport_rect` par
//! [`show_floating_window`], pour le dessin **et** pour les clics : une
//! fenêtre ne peut donc ni peindre par-dessus le Project Manager, la console
//! ou les barres de défilement, ni y capter un clic, même à cheval sur le
//! bord de la vue. Entièrement hors de la portion visible (l'utilisateur a
//! défilé ailleurs), elle n'est pas peinte du tout cette image-là ; redéfiler
//! la fait réapparaître telle quelle, à la même position logique.
//!
//! **Cascade** : une fenêtre nouvellement ouverte (pas une simple
//! refocalisation) reçoit une position par défaut décalée par rapport à la
//! précédente ([`StokesForgeApp::next_cascade`], remis à zéro tous les huit
//! onglets pour ne pas dériver hors de `WORKSPACE_SIZE`) — l'utilisateur voit
//! immédiatement qu'une nouvelle fenêtre est apparue plutôt que de la trouver
//! exactement superposée à la précédente.
//!
//! **Refocaliser une fenêtre défilée hors vue** : rouvrir depuis le Project
//! Manager une entrée déjà ouverte (§ z-order, juste en dessous) ajuste aussi
//! `workspace_scroll` si besoin (`workspace_scroll_to_reveal`), pour que la
//! fenêtre visée redevienne visible plutôt que de simplement remonter dans un
//! calque qu'on ne voit pas.
//!
//! **Z-order** : l'`egui::Area` d'une fenêtre remonte déjà seule au premier
//! plan au clic (`Sense::CLICK` par défaut) — mais dans son propre ordre de
//! calques, indépendant de [`StokesForgeApp::floating_tabs`], qui reste la seule
//! source d'`active_tab` (donc du contour de surbrillance). Sans
//! synchronisation, un clic direct dans une fenêtre la faisait passer au
//! premier plan sans jamais la surligner (B2,
//! `retours-2026-09-23-bugs.md`). [`StokesForgeApp::ui`] relit donc
//! `Context::top_layer_id` juste après avoir peint les fenêtres flottantes
//! et ramène le tab trouvé en fin de `floating_tabs` via
//! `sync_floating_tabs_order_to_top_layer` — un cadre de retard tout au plus,
//! `egui` redemandant lui-même un repaint immédiat au clic. Le seul cas que
//! ce module doit encore gérer *avant* qu'`egui` n'ait peint quoi que ce soit
//! est « rouvrir depuis le Project Manager une entrée déjà ouverte doit la
//! ramener au premier plan sans qu'on ait cliqué dedans » :
//! [`StokesForgeApp::open_tab`] pose alors [`StokesForgeApp::focus_next`], consommé au
//! rendu suivant par un appel à `Context::move_to_top` sur le calque
//! ([`FLOATING_WINDOW_ORDER`]) de la fenêtre concernée. Jusqu'au passage à
//! `egui::Area`, cet appel visait `Order::Foreground` alors que les
//! `egui::Window` vivaient sur `Order::Middle` : il remontait le calque du
//! contour, pas la fenêtre elle-même.
//!
//! # Maximiser un onglet
//!
//! [`StokesForgeApp::maximized_tab`] est un état **à côté** de
//! [`StokesForgeApp::floating_tabs`], qui ne le touche jamais : quand il vaut
//! `Some`, [`StokesForgeApp::ui`] ne peint aucune fenêtre flottante cette
//! image-là — à la place, une zone séparée montre uniquement l'onglet
//! maximisé, plein cadre du `CentralPanel`. Restaurer (`maximized_tab =
//! None`) revient à peindre les fenêtres flottantes normalement : leurs
//! positions/tailles (retenues par `egui` via leur `Id`, pas par ce module)
//! n'ont jamais bougé pendant la maximisation, donc « restaurer » est
//! exactement « comme avant », pas un recalcul.
//!
//! Le Project Manager n'a jamais de valeur dans `maximized_tab` : ce n'est
//! pas une fenêtre flottante, et un panneau fixe toujours entièrement
//! visible n'a de toute façon rien à gagner à être « maximisé ». Il reste
//! visible sans rien de spécial pendant qu'un *autre* onglet est maximisé,
//! puisque [`StokesForgeApp::project_manager_panel`] est peint avant la zone de
//! maximisation comme avant le `CentralPanel` — un panneau fixe séparé,
//! jamais dédoublé, n'a besoin d'aucun cas particulier ici.
//!

use std::collections::HashSet;

use eframe::egui;
use stokesforge_core::NodeId;

use crate::state::{Action, WindowKind};
use crate::tr;

use super::tab::{tab_node, Tab};
use super::window_chrome::{
    floating_resize_handles, floating_window_frame, show_floating_window,
    show_window_contents_bounded, window_header, HeaderPlacement, WindowHeaderActions,
    FLOATING_WINDOW_ORDER, MIN_CONTENT_WINDOW_SIZE, WINDOW_HEADER_HEIGHT,
};
use super::window_placement::{
    clamp_to_workspace, floating_window_display_size, floating_window_geometry_to_persist,
    geometry_rect, workspace_scroll_to_reveal, workspace_to_screen, BOOK_WINDOW_WIDTH,
    CASCADE_STEP, CASCADE_WRAP, FLOATING_WINDOW_SIZE, GRAPH_WINDOW_SIZE, WORKSPACE_SIZE,
};
use super::{tab_contents, tab_title, StokesForgeApp};

/// Un clic ne réactive le contenu que sur son calque : Object Manager
/// flottant peut le recouvrir tout en gardant un autre graphe comme cible.
pub(super) fn window_pointer_pressed(
    ctx: &egui::Context,
    layer: egui::LayerId,
    rect: egui::Rect,
) -> bool {
    ctx.input(|input| input.pointer.any_pressed()) && ctx.rect_contains_pointer(layer, rect)
}

/// Les fenêtres de l'arbre du Project Manager, à plat.
pub(super) fn window_nodes(tree: &[crate::state::ProjectTreeNode]) -> Vec<(NodeId, WindowKind)> {
    let mut out = Vec::new();
    for node in tree {
        match node {
            crate::state::ProjectTreeNode::Folder { children, .. } => {
                out.extend(window_nodes(children));
            }
            crate::state::ProjectTreeNode::Window { id, kind, .. } => out.push((*id, *kind)),
        }
    }
    out
}

/// Bascule le repli du contenu d'un onglet. Réservé à `Tab::Graph` et
/// `Tab::Book` (B5, `retours-2026-09-23-bugs.md`) — `Matrix`
/// et `Note` n'ont pas ce contrôle et restent donc toujours affichés ; rien
/// dans la peinture du contenu replié (voir la boucle des fenêtres
/// flottantes et détachées dans `StokesForgeApp::ui`) ne dépend du type de `tab`,
/// l'extension à ces deux natures n'est donc qu'une question d'icône visible,
/// pas de comportement caché à revérifier.
pub(super) fn toggle_collapse(collapsed_tabs: &mut HashSet<Tab>, tab: Tab) {
    if !matches!(tab, Tab::Graph(_) | Tab::Book(_)) {
        return;
    }
    if !collapsed_tabs.insert(tab) {
        collapsed_tabs.remove(&tab);
    }
}

/// Pose le repli à une valeur précise plutôt que de le basculer — utilisé
/// par [`StokesForgeApp::reattach_tab`] pour reverser dans
/// `collapsed_tabs` l'état propre à la fenêtre détachée
/// (`DetachedTabViewModel::collapsed`) plutôt que de le deviner.
pub(super) fn toggle_collapse_to(collapsed_tabs: &mut HashSet<Tab>, tab: Tab, collapsed: bool) {
    if !matches!(tab, Tab::Graph(_) | Tab::Book(_)) {
        return;
    }
    if collapsed {
        collapsed_tabs.insert(tab);
    } else {
        collapsed_tabs.remove(&tab);
    }
}

/// Ramène `top_layer_tab` en fin de `floating_tabs` (§ ordre de pile,
/// `active_tab = floating_tabs.last()`), sans rien faire s'il n'y est déjà,
/// s'il n'existe pas, ou si aucune fenêtre flottante n'est au premier plan
/// (B2, `retours-2026-09-23-bugs.md`). `egui` gère seul son
/// propre ordre de calques (un clic direct dans une fenêtre l'amène déjà au
/// premier plan visuellement) ; cette fonction ne fait que réaligner
/// `floating_tabs`, seule source de `active_tab`, dessus.
pub(super) fn sync_floating_tabs_order_to_top_layer(
    floating_tabs: &mut Vec<Tab>,
    top_layer_tab: Option<Tab>,
) {
    let Some(top_layer_tab) = top_layer_tab else {
        return;
    };
    if let Some(pos) = floating_tabs.iter().position(|&t| t == top_layer_tab) {
        if pos + 1 != floating_tabs.len() {
            let tab = floating_tabs.remove(pos);
            floating_tabs.push(tab);
        }
    }
}

impl StokesForgeApp {
    /// Ouvre l'onglet `tab`, ou le focalise s'il est déjà ouvert.
    ///
    /// Le seul endroit qui décide entre les deux, pour toute nature de
    /// fenêtre (classeur, graphe, matrice, note) : cliquer une seconde fois
    /// la même entrée du Project Manager ne doit jamais dupliquer son
    /// onglet, seulement l'amener au premier plan.
    pub(super) fn open_tab(&mut self, tab: Tab) {
        self.active_detached = None;
        if let Some(pos) = self.floating_tabs.iter().position(|t| *t == tab) {
            self.floating_tabs.remove(pos);
            self.floating_tabs.push(tab);
            self.focus_next = Some(tab);
        } else {
            self.push_floating_tab(tab);
        }
    }

    /// Ajoute `tab` à [`StokesForgeApp::floating_tabs`] avec un nouveau rang de
    /// cascade — partagé par [`StokesForgeApp::open_tab`] (première ouverture) et
    /// [`StokesForgeApp::reattach_tab`] (retour d'une fenêtre OS séparée), les deux
    /// seuls endroits où une fenêtre flottante apparaît sans exister déjà.
    pub(super) fn push_floating_tab(&mut self, tab: Tab) {
        self.floating_tabs.push(tab);
        self.cascade_rank.insert(tab, self.next_cascade);
        self.next_cascade = self.next_cascade.wrapping_add(1);
    }
}

impl StokesForgeApp {
    /// Peint l'espace de travail (`CentralPanel`) : la fenêtre maximisée, ou
    /// les fenêtres flottantes dans l'ordre de `floating_tabs` avec leur
    /// cascade, leur repli, leur masquage et leurs poignées. Les demandes de
    /// détachement, de fermeture et de géométrie sont rendues à l'appelant, qui
    /// les applique une fois l'image peinte (muter `floating_tabs` en plein
    /// parcours l'invaliderait).
    pub(super) fn show_workspace(
        &mut self,
        ui: &mut egui::Ui,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
        detach_requests: &mut Vec<Tab>,
        close_requests: &mut Vec<Tab>,
        geometry_updates: &mut Vec<(Tab, egui::Rect)>,
    ) {
        // Emprunts disjoints des champs (édition 2021) : la disposition et
        // les contenus qu'elle porte ont besoin, chacun, d'un accès distinct
        // à `self`, simultanément le temps de peindre une image.
        let Self {
            state,
            floating_tabs,
            cascade_rank,
            next_cascade,
            focus_next,
            active_detached,
            graph_views,
            collapsed_tabs,
            maximized_tab,
            window_positions,
            window_sizes,
            workspace_scroll,
            hidden_tabs,
            floating_rects,
            ..
        } = self;

        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(tab) = *maximized_tab {
                // § maximiser (doc de tête du module) : aucune fenêtre
                // flottante n'est peinte du tout cette image-ci. Le Project
                // Manager n'a besoin d'aucun traitement particulier ici : son
                // panneau fixe est peint plus haut, avant ce `CentralPanel`,
                // qu'un autre onglet soit maximisé ou non.
                ui.horizontal(|ui| {
                    ui.heading(tab_title(state, tab));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(tr!("action-restore-window", title = tab_title(state, tab)))
                            .on_hover_text(tr!("action-restore-window", "hint"))
                            .clicked()
                        {
                            *maximized_tab = None;
                        }
                    });
                });
                ui.separator();
                if window_pointer_pressed(ui.ctx(), ui.layer_id(), ui.max_rect()) {
                    *active_detached = None;
                }
                tab_contents(ui, state, actions, graph_views, tab);
            } else {
                // Ce que les panneaux fixes peints plus haut (menu, outils,
                // statut, script, Project Manager) laissent — voir la doc de
                // tête du module, § Fenêtres flottantes confinées. L'espace
                // de travail a une taille fixe (`WORKSPACE_SIZE`), indépendante
                // de `viewport_rect` : sous cette taille, il défile
                // (`workspace_scroll`) plutôt que d'écraser les fenêtres
                // qu'il porte contre les bords du `CentralPanel`.
                // `window_positions`/`window_sizes`, et la géométrie
                // persistée dans le projet, sont donc en coordonnées
                // logiques de cet espace — bornées à lui par
                // `clamp_to_workspace`, puis converties en coordonnées écran
                // juste avant la peinture (`workspace_to_screen`). Une
                // fenêtre ne sort donc jamais de l'espace de travail ; elle
                // peut sortir de sa portion visible, auquel cas elle est
                // rognée au bord de la vue, ou pas peinte du tout si elle
                // en sort entièrement (`show_floating_window`).
                let viewport_rect = ui.max_rect();

                // Une fenêtre déjà ouverte mais actuellement défilée hors vue
                // redevient visible au clic qui la refocalise (Project
                // Manager, double-clic…) — lu avant le défilement de cette
                // image pour que ça prenne effet dès maintenant, pas à la
                // suivante.
                if let Some(tab) = *focus_next {
                    if let (Some(&position), Some(&size)) =
                        (window_positions.get(&tab), window_sizes.get(&tab))
                    {
                        let target = egui::Rect::from_min_size(position, size);
                        *workspace_scroll = workspace_scroll_to_reveal(
                            *workspace_scroll,
                            viewport_rect.size(),
                            target,
                        );
                    }
                }

                // Un `egui::ScrollArea` invisible (rien n'est peint dans son
                // contenu, juste réservé à `WORKSPACE_SIZE`) plutôt qu'un
                // défilement maison : il donne gratuitement poignées de
                // glissement, molette et bornage, au style commun des barres
                // de défilement (`theme::scroll_style`, posé globalement). Les fenêtres flottantes ne sont
                // **pas** peintes à l'intérieur de sa fermeture : chacune
                // peint sur son propre calque (`show_floating_window`),
                // indépendant de toute zone de défilement ambiante — seul le
                // décalage qui en résulte sert, appliqué à la main ci-dessous.
                let workspace_scroll_output = egui::ScrollArea::both()
                    .id_salt("stokesforge_workspace")
                    .auto_shrink([false, false])
                    .scroll_offset(*workspace_scroll)
                    .show(ui, |ui| {
                        ui.allocate_exact_size(WORKSPACE_SIZE, egui::Sense::hover());
                    });
                *workspace_scroll = workspace_scroll_output.state.offset;
                // Ce qui reste visible une fois les barres de défilement
                // posées : les fenêtres flottantes sont rognées à ce
                // rectangle, pour ne jamais recouvrir ces barres.
                let viewport_rect = workspace_scroll_output.inner_rect;

                for &tab in floating_tabs.iter() {
                    if hidden_tabs.contains(&tab) {
                        // Masqué depuis le Project Manager (§6) : l'onglet
                        // reste dans `floating_tabs` (cascade/position/
                        // géométrie conservées), seul son rendu est sauté.
                        continue;
                    }
                    let id = egui::Id::new(tab);
                    if *focus_next == Some(tab) {
                        ui.ctx()
                            .move_to_top(egui::LayerId::new(FLOATING_WINDOW_ORDER, id));
                    }
                    let rank = cascade_rank.entry(tab).or_insert_with(|| {
                        let rank = *next_cascade;
                        *next_cascade = next_cascade.wrapping_add(1);
                        rank
                    });
                    let cascade = CASCADE_STEP * (*rank % CASCADE_WRAP) as f32;
                    // Coordonnées logiques de l'espace de travail, pas
                    // coordonnées écran — voir la doc juste au-dessus.
                    let default_pos = egui::pos2(cascade, cascade);
                    let natural_size = match tab {
                        Tab::Book(_) => egui::vec2(BOOK_WINDOW_WIDTH, FLOATING_WINDOW_SIZE.y),
                        Tab::Graph(_) => GRAPH_WINDOW_SIZE,
                        _ => FLOATING_WINDOW_SIZE,
                    };
                    let default_size = natural_size.min(WORKSPACE_SIZE);
                    // Le dernier rectangle flottant connu de l'application
                    // prime sur l'instantané (périmé après un déplacement).
                    let saved_rect = floating_rects.get(&tab_node(tab)).copied().or_else(|| {
                        state
                            .snapshot()
                            .window_geometries
                            .get(&tab_node(tab))
                            .map(geometry_rect)
                    });
                    let saved_pos = saved_rect.map_or(default_pos, |rect| rect.min);
                    let saved_size = saved_rect
                        .map_or(default_size, |rect| rect.size())
                        .max(MIN_CONTENT_WINDOW_SIZE)
                        .min(WORKSPACE_SIZE);
                    window_positions.entry(tab).or_insert(saved_pos);
                    let current_size = *window_sizes.entry(tab).or_insert(saved_size);
                    // Seule source de vérité de la position : bornée ici, à
                    // l'espace de travail, et réécrite telle quelle. egui ne
                    // recale plus rien de son côté (plus de `constrain_to` sur
                    // l'espace de travail), donc le rectangle peint, le
                    // contour, les poignées et la géométrie persistée
                    // viennent tous du même rectangle — c'est l'écart entre
                    // un rectangle demandé et un rectangle recalé par egui
                    // qui produisait le bug 1 de
                    // `retours-2026-09-24-bugs.md`.
                    let logical_rect = clamp_to_workspace(egui::Rect::from_min_size(
                        window_positions[&tab],
                        current_size,
                    ));
                    window_positions.insert(tab, logical_rect.min);
                    let mut header_actions = WindowHeaderActions::default();
                    let was_collapsed = collapsed_tabs.contains(&tab);
                    let is_active = Some(tab) == active_tab;
                    // Fenêtre repliée (B3) : `window_header` ne peint que sa
                    // bande d'en-tête, `tab_contents` est sauté — peindre la
                    // fenêtre à `current_size` dessinerait un cadre (et un
                    // contour) à la taille dépliée autour d'une zone vide.
                    // `current_size` reste la taille logique conservée dans
                    // `window_sizes`/persistée ; seul le rectangle peint
                    // cette image se réduit.
                    let frame = floating_window_frame(ui.style());
                    let display_size = floating_window_display_size(
                        current_size,
                        was_collapsed,
                        WINDOW_HEADER_HEIGHT,
                        frame.inner_margin.sum(),
                    );
                    let window_rect = egui::Rect::from_min_size(
                        workspace_to_screen(logical_rect.min, viewport_rect.min, *workspace_scroll),
                        display_size,
                    );
                    let title = tab_title(state, tab);
                    // Hors de la portion visible : `None`, ni peinte ni
                    // interactive — « cachée », pas écrasée. Partiellement
                    // visible : rognée au bord de `viewport_rect`, pour le
                    // dessin comme pour les clics (bug 2 du même rapport).
                    let shown = show_floating_window(
                        ui.ctx(),
                        id,
                        window_rect,
                        viewport_rect,
                        is_active,
                        |ui| {
                            header_actions = window_header(
                                ui,
                                id,
                                &title,
                                Some(tab),
                                was_collapsed,
                                HeaderPlacement::Floating,
                            );
                            if !(was_collapsed ^ header_actions.toggle_collapse) {
                                // Le sous-Ui borné empêche le contenu de
                                // déborder de la fenêtre. La grille seule
                                // reçoit un défilement horizontal (F8).
                                show_window_contents_bounded(ui, |ui| {
                                    tab_contents(ui, state, actions, graph_views, tab);
                                });
                            }
                        },
                        |ui, rect| {
                            // Poignées maison (§3) : mutent `window_positions`/
                            // `window_sizes` directement ; la prochaine image
                            // les bornera puis les peindra.
                            if let (Some(position), Some(size)) =
                                (window_positions.get_mut(&tab), window_sizes.get_mut(&tab))
                            {
                                floating_resize_handles(ui, tab, rect, position, size);
                            }
                        },
                    );
                    if shown.is_none() {
                        continue;
                    }
                    if window_pointer_pressed(
                        ui.ctx(),
                        egui::LayerId::new(FLOATING_WINDOW_ORDER, id),
                        window_rect.intersect(viewport_rect),
                    ) {
                        *active_detached = None;
                    }
                    if header_actions.toggle_collapse {
                        toggle_collapse(collapsed_tabs, tab);
                    }
                    if header_actions.maximize {
                        *maximized_tab = Some(tab);
                    }
                    if header_actions.detach {
                        detach_requests.push(tab);
                    }
                    if header_actions.drag_delta != egui::Vec2::ZERO {
                        if let Some(position) = window_positions.get_mut(&tab) {
                            *position += header_actions.drag_delta;
                        }
                    }
                    // Géométrie persistée : le rectangle logique borné de
                    // cette image, taille logique comprise (pas la taille
                    // repliée, voir `floating_window_geometry_to_persist`).
                    geometry_updates.push((
                        tab,
                        floating_window_geometry_to_persist(logical_rect, current_size),
                    ));
                    if header_actions.close {
                        close_requests.push(tab);
                    }
                }
                *focus_next = None;

                // B2 (`retours-2026-09-23-bugs.md`) :
                // `egui::Area` amène déjà la fenêtre cliquée au premier plan
                // de son propre ordre de calques dès qu'on clique dedans
                // (`Sense::CLICK`, indépendamment du Project Manager) — mais
                // `floating_tabs`, seule source de `active_tab` (donc de
                // `is_active`/la surbrillance, voir plus haut), ne le sait
                // pas : un clic direct amenait la fenêtre au premier plan
                // sans jamais la surligner. On resynchronise ici sur ce
                // qu'egui vient d'établir, une fois la mutation de
                // `floating_tabs` de nouveau sûre (la boucle qui l'itère est
                // refermée).
                let top_layer_tab = ui.ctx().top_layer_id().and_then(|top| {
                    floating_tabs
                        .iter()
                        .copied()
                        .find(|&tab| egui::Id::new(tab) == top.id)
                });
                sync_floating_tabs_order_to_top_layer(floating_tabs, top_layer_tab);
            }
        });
    }

    /// Applique les fermetures demandées par l'en-tête des fenêtres flottantes.
    pub(super) fn close_floating_tabs(&mut self, close_requests: Vec<Tab>) {
        for tab in close_requests {
            if let Some(pos) = self.floating_tabs.iter().position(|t| *t == tab) {
                self.floating_tabs.remove(pos);
                self.cascade_rank.remove(&tab);
                self.window_positions.remove(&tab);
                self.window_sizes.remove(&tab);
            }
            if self.maximized_tab == Some(tab) {
                // Garde de cohérence, comme dans `detach_tab` — ne devrait
                // pas arriver : un onglet maximisé n'est pas peint comme
                // fenêtre flottante, donc son bouton de fermeture natif n'est
                // pas accessible pendant qu'il l'est.
                self.maximized_tab = None;
            }
        }
    }
}
