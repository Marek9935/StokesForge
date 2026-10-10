//! Pile des fenêtres de contenu (désignées par l'onglet [`Tab`], de `tab.rs`) :
//! l'ordre de pile `floating_tabs` (le dernier est l'onglet actif), son
//! alignement sur les calques d'egui, le masquage, le repli, la maximisation
//! et le dessin de l'espace de travail (`show_workspace`).
//!
//! Un clic reçu par une fenêtre reprend explicitement le suivi du Project
//! Manager (#154), même si cette fenêtre était déjà active. Le suivi passif
//! conserve les multi-sélections pour les actions groupées du panneau ; seul
//! le geste sur le contenu les remplace, après vérification du calque touché.
//!
//! Déplacé tel quel d'`app.rs` (#101), puis redécoupé sur place (#124) :
//! maximisation, défilement et peinture d’une fenêtre partagent uniquement
//! les emprunts disjoints de l’image, sans ajouter de sous-Ui.
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
//! exprimées dans ces coordonnées logiques ; [`StokesForgeApp::ui`](eframe::App::ui) les convertit
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
//! `retours-2026-09-23-bugs.md`). [`StokesForgeApp::ui`](eframe::App::ui) relit donc
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
//! `Some`, [`StokesForgeApp::ui`](eframe::App::ui) ne peint aucune fenêtre flottante cette
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

use std::collections::{HashMap, HashSet};

use eframe::egui;
use stokesforge_core::NodeId;
use stokesforge_render_egui::GraphView;

use crate::state::{Action, AppState, WindowKind};
use crate::tr;

use super::contents::tab_contents;
use super::tab::tab_title;
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
use super::StokesForgeApp;

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

/// Emprunts disjoints limités à une peinture : ni nouvel état ni nouvelle Ui.
/// L’extraction de chaque fenêtre ne peut prendre `&mut StokesForgeApp`
/// pendant que les contenus empruntent déjà son AppState et ses GraphView.
struct Workspace<'a> {
    state: &'a mut AppState,
    floating_tabs: &'a mut Vec<Tab>,
    cascade_rank: &'a mut HashMap<Tab, u32>,
    next_cascade: &'a mut u32,
    focus_next: &'a mut Option<Tab>,
    active_detached: &'a mut Option<Tab>,
    pm_selection: &'a mut HashSet<Tab>,
    graph_views: &'a mut HashMap<NodeId, GraphView>,
    collapsed_tabs: &'a mut HashSet<Tab>,
    maximized_tab: &'a mut Option<Tab>,
    window_positions: &'a mut HashMap<Tab, egui::Pos2>,
    window_sizes: &'a mut HashMap<Tab, egui::Vec2>,
    workspace_scroll: &'a mut egui::Vec2,
    hidden_tabs: &'a mut HashSet<Tab>,
    floating_rects: &'a mut HashMap<NodeId, egui::Rect>,
    actions: &'a mut Vec<Action>,
    detach_requests: &'a mut Vec<Tab>,
    close_requests: &'a mut Vec<Tab>,
    geometry_updates: &'a mut Vec<(Tab, egui::Rect)>,
}

impl Workspace<'_> {
    fn activate_clicked_tab(&mut self, ctx: &egui::Context, tab: Tab) {
        *self.active_detached = None;
        self.pm_selection.clear();
        self.pm_selection.insert(tab);
        super::project_manager::follow_selection_anchor(ctx, Some(tab));
        // Le panneau et son viewport détaché ont déjà été publiés cette image.
        ctx.request_repaint();
    }

    fn show_maximized(&mut self, ui: &mut egui::Ui, tab: Tab) {
        // § maximiser (doc de tête du module) : aucune fenêtre
        // flottante n'est peinte du tout cette image-ci. Le Project
        // Manager n'a besoin d'aucun traitement particulier ici : son
        // panneau fixe est peint plus haut, avant ce `CentralPanel`,
        // qu'un autre onglet soit maximisé ou non.
        ui.horizontal(|ui| {
            ui.heading(tab_title(self.state, tab));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(tr!(
                        "action-restore-window",
                        title = tab_title(self.state, tab)
                    ))
                    .on_hover_text(tr!("action-restore-window", "hint"))
                    .clicked()
                {
                    *self.maximized_tab = None;
                }
            });
        });
        ui.separator();
        if window_pointer_pressed(ui.ctx(), ui.layer_id(), ui.max_rect()) {
            self.activate_clicked_tab(ui.ctx(), tab);
        }
        tab_contents(ui, self.state, self.actions, self.graph_views, tab);
    }

    fn show_floating(&mut self, ui: &mut egui::Ui, active_tab: Option<Tab>) {
        // Ce que les panneaux fixes peints plus haut (menu, outils,
        // statut, script, Project Manager) laissent — voir la doc de
        // tête du module, § Fenêtres flottantes confinées. L'espace
        // de travail a une taille fixe (`WORKSPACE_SIZE`), indépendante
        // de `viewport_rect` : sous cette taille, il défile
        // (`self.workspace_scroll`) plutôt que d'écraser les fenêtres
        // qu'il porte contre les bords du `CentralPanel`.
        // `self.window_positions`/`self.window_sizes`, et la géométrie
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
        if let Some(tab) = *self.focus_next {
            if let (Some(&position), Some(&size)) =
                (self.window_positions.get(&tab), self.window_sizes.get(&tab))
            {
                let target = egui::Rect::from_min_size(position, size);
                *self.workspace_scroll = workspace_scroll_to_reveal(
                    *self.workspace_scroll,
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
            .scroll_offset(*self.workspace_scroll)
            .show(ui, |ui| {
                ui.allocate_exact_size(WORKSPACE_SIZE, egui::Sense::hover());
            });
        *self.workspace_scroll = workspace_scroll_output.state.offset;
        // Ce qui reste visible une fois les barres de défilement
        // posées : les fenêtres flottantes sont rognées à ce
        // rectangle, pour ne jamais recouvrir ces barres.
        let viewport_rect = workspace_scroll_output.inner_rect;

        for index in 0..self.floating_tabs.len() {
            // La pile n’est réordonnée qu’après son parcours, comme avant #124.
            let tab = self.floating_tabs[index];
            self.show_floating_tab(ui, tab, active_tab, viewport_rect);
        }
        let refocused = self.focus_next.take().is_some();

        // B2 (`retours-2026-09-23-bugs.md`) :
        // `egui::Area` amène déjà la fenêtre cliquée au premier plan
        // de son propre ordre de calques dès qu'on clique dedans
        // (`Sense::CLICK`, indépendamment du Project Manager) — mais
        // `self.floating_tabs`, seule source de `active_tab` (donc de
        // `is_active`/la surbrillance, voir plus haut), ne le sait
        // pas : un clic direct amenait la fenêtre au premier plan
        // sans jamais la surligner. On resynchronise ici sur ce
        // qu'egui vient d'établir, une fois la mutation de
        // `self.floating_tabs` de nouveau sûre (la boucle qui l'itère est
        // refermée).
        //
        // Une réouverture depuis le Project Manager vient de demander
        // `move_to_top` : egui ne la reflète dans `top_layer_id` qu'à l'image
        // suivante. Resynchroniser maintenant remettrait l'ancienne fenêtre en
        // fin de pile (et la sélection du Project Manager avec elle) ; on
        // attend donc l'image suivante, réclamée ici.
        if refocused {
            ui.ctx().request_repaint();
            return;
        }
        let top_layer_tab = ui.ctx().top_layer_id().and_then(|top| {
            self.floating_tabs
                .iter()
                .copied()
                .find(|&tab| egui::Id::new(tab) == top.id)
        });
        sync_floating_tabs_order_to_top_layer(self.floating_tabs, top_layer_tab);
    }

    fn show_floating_tab(
        &mut self,
        ui: &mut egui::Ui,
        tab: Tab,
        active_tab: Option<Tab>,
        viewport_rect: egui::Rect,
    ) {
        if self.hidden_tabs.contains(&tab) {
            // Masqué depuis le Project Manager (§6) : l'onglet
            // reste dans `self.floating_tabs` (cascade/position/
            // géométrie conservées), seul son rendu est sauté.
            return;
        }
        let id = egui::Id::new(tab);
        if *self.focus_next == Some(tab) {
            ui.ctx()
                .move_to_top(egui::LayerId::new(FLOATING_WINDOW_ORDER, id));
        }
        let rank = self.cascade_rank.entry(tab).or_insert_with(|| {
            let rank = *self.next_cascade;
            *self.next_cascade = self.next_cascade.wrapping_add(1);
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
        let saved_rect = self
            .floating_rects
            .get(&tab_node(tab))
            .copied()
            .or_else(|| {
                self.state
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
        self.window_positions.entry(tab).or_insert(saved_pos);
        let current_size = *self.window_sizes.entry(tab).or_insert(saved_size);
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
            self.window_positions[&tab],
            current_size,
        ));
        self.window_positions.insert(tab, logical_rect.min);
        let mut header_actions = WindowHeaderActions::default();
        let was_collapsed = self.collapsed_tabs.contains(&tab);
        let is_active = Some(tab) == active_tab;
        // Fenêtre repliée (B3) : `window_header` ne peint que sa
        // bande d'en-tête, `tab_contents` est sauté — peindre la
        // fenêtre à `current_size` dessinerait un cadre (et un
        // contour) à la taille dépliée autour d'une zone vide.
        // `current_size` reste la taille logique conservée dans
        // `self.window_sizes`/persistée ; seul le rectangle peint
        // cette image se réduit.
        let frame = floating_window_frame(ui.style());
        let display_size = floating_window_display_size(
            current_size,
            was_collapsed,
            WINDOW_HEADER_HEIGHT,
            frame.inner_margin.sum(),
        );
        let window_rect = egui::Rect::from_min_size(
            workspace_to_screen(logical_rect.min, viewport_rect.min, *self.workspace_scroll),
            display_size,
        );
        let title = tab_title(self.state, tab);
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
                        tab_contents(ui, self.state, self.actions, self.graph_views, tab);
                    });
                }
            },
            |ui, rect| {
                // Poignées maison (§3) : mutent `self.window_positions`/
                // `self.window_sizes` directement ; la prochaine image
                // les bornera puis les peindra.
                if let (Some(position), Some(size)) = (
                    self.window_positions.get_mut(&tab),
                    self.window_sizes.get_mut(&tab),
                ) {
                    floating_resize_handles(ui, tab, rect, position, size);
                }
            },
        );
        if shown.is_none() {
            return;
        }
        if window_pointer_pressed(
            ui.ctx(),
            egui::LayerId::new(FLOATING_WINDOW_ORDER, id),
            window_rect.intersect(viewport_rect),
        ) {
            self.activate_clicked_tab(ui.ctx(), tab);
        }
        if header_actions.toggle_collapse {
            toggle_collapse(self.collapsed_tabs, tab);
        }
        if header_actions.maximize {
            *self.maximized_tab = Some(tab);
        }
        if header_actions.detach {
            self.detach_requests.push(tab);
        }
        if header_actions.drag_delta != egui::Vec2::ZERO {
            if let Some(position) = self.window_positions.get_mut(&tab) {
                *position += header_actions.drag_delta;
            }
        }
        // Géométrie persistée : le rectangle logique borné de
        // cette image, taille logique comprise (pas la taille
        // repliée, voir `floating_window_geometry_to_persist`).
        self.geometry_updates.push((
            tab,
            floating_window_geometry_to_persist(logical_rect, current_size),
        ));
        if header_actions.close {
            self.close_requests.push(tab);
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
            pm_selection,
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

        let mut workspace = Workspace {
            state,
            floating_tabs,
            cascade_rank,
            next_cascade,
            focus_next,
            active_detached,
            pm_selection,
            graph_views,
            collapsed_tabs,
            maximized_tab,
            window_positions,
            window_sizes,
            workspace_scroll,
            hidden_tabs,
            floating_rects,
            actions,
            detach_requests,
            close_requests,
            geometry_updates,
        };
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(tab) = *workspace.maximized_tab {
                workspace.show_maximized(ui, tab);
            } else {
                workspace.show_floating(ui, active_tab);
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

#[cfg(test)]
mod tests {
    use super::super::window_chrome::show_window_in_layer;
    use super::*;
    use crate::state::ProjectTreeNode;

    /// #154 : le clic explicite reprend le suivi, même sans changement d'onglet.
    #[test]
    fn le_clic_workspace_remplace_la_multiselection_du_project_manager() {
        for (maximized, already_active, header) in [
            (false, false, false),
            (false, true, false),
            (true, false, false),
            (true, true, false),
            (false, false, true),
            (false, true, true),
            (true, false, true),
            (true, true, true),
        ] {
            let (session, graph) = crate::app::graph_window::tests::graphe();
            let state = AppState::new(session.handle(), String::new()).unwrap();
            let book = window_nodes(&state.project_tree())
                .into_iter()
                .find(|(_, kind)| *kind == WindowKind::Book)
                .map(|(id, _)| Tab::Book(id))
                .unwrap();
            let tab = Tab::Graph(graph);
            let mut app = StokesForgeApp::new(state);
            app.floating_tabs = if already_active {
                vec![book, tab]
            } else {
                vec![tab, book]
            };
            app.window_positions.insert(tab, egui::pos2(30.0, 30.0));
            app.window_positions.insert(book, egui::pos2(750.0, 30.0));
            app.maximized_tab = maximized.then_some(tab);
            app.pm_selection = HashSet::from([tab, book]);
            let context = egui::Context::default();
            context.set_fonts(stokesforge_render_egui::font_definitions());
            crate::theme::install(&context);
            egui_extras::install_image_loaders(&context);
            let mut paint = |events| {
                context
                    .run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(1400.0, 900.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            let active = app
                                .maximized_tab
                                .or_else(|| app.floating_tabs.last().copied());
                            app.follow_project_manager_active_tab(ui.ctx(), active);
                            app.show_workspace(
                                ui,
                                active,
                                &mut Vec::new(),
                                &mut Vec::new(),
                                &mut Vec::new(),
                                &mut Vec::new(),
                            );
                        },
                    )
                    .textures_delta
                    .clear();
                app.pm_selection.clone()
            };
            for _ in 0..3 {
                assert_eq!(
                    paint(Vec::new()),
                    HashSet::from([tab, book]),
                    "le repos conserve les actions groupées"
                );
            }
            if !maximized {
                let empty = egui::pos2(1350.0, 850.0);
                paint(vec![egui::Event::PointerMoved(empty)]);
                assert_eq!(
                    paint(vec![egui::Event::PointerButton {
                        pos: empty,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    }]),
                    HashSet::from([tab, book]),
                    "le vide du workspace conserve la sélection"
                );
                paint(vec![egui::Event::PointerButton {
                    pos: empty,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
            let pos = egui::pos2(
                150.0,
                if header {
                    if maximized {
                        15.0
                    } else {
                        45.0
                    }
                } else {
                    150.0
                },
            );
            assert_eq!(
                paint(vec![egui::Event::PointerMoved(pos)]),
                HashSet::from([tab, book]),
                "le survol conserve la sélection"
            );
            let selection = paint(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }]);
            assert_eq!(
                selection,
                HashSet::from([tab]),
                "maximisée={maximized}, déjà active={already_active}, en-tête={header}"
            );
            session.shutdown().unwrap();
        }
    }

    #[test]
    fn une_fenetre_masquee_ne_peint_rien_et_conserve_sa_place_dans_la_pile() {
        let (session, graph) = crate::app::graph_window::tests::graphe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let tab = Tab::Graph(graph);
        app.floating_tabs = vec![tab];
        app.hidden_tabs.insert(tab);
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        crate::theme::install(&context);
        egui_extras::install_image_loaders(&context);
        let mut actions = Vec::new();
        let mut detach = Vec::new();
        let mut close = Vec::new();
        let mut geometries = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 900.0),
            )),
            ..Default::default()
        };
        for hidden in [true, false] {
            if !hidden {
                app.hidden_tabs.remove(&tab);
            }
            geometries.clear();
            for _ in 0..4 {
                context
                    .run_ui(input.clone(), |ui| {
                        app.show_workspace(
                            ui,
                            Some(tab),
                            &mut actions,
                            &mut detach,
                            &mut close,
                            &mut geometries,
                        );
                    })
                    .textures_delta
                    .clear();
            }
            assert_eq!(app.floating_tabs, vec![tab], "le masquage garde la pile");
            assert_eq!(
                geometries.is_empty(),
                hidden,
                "témoin : visible, la fenêtre publie sa géométrie"
            );
            assert_eq!(
                app.graph_views.contains_key(&graph),
                !hidden,
                "le contenu n'est peint que visible"
            );
        }
        assert!(actions.is_empty());
        assert!(detach.is_empty());
        assert!(close.is_empty());
        session.shutdown().unwrap();
    }

    #[test]
    fn le_repli_ne_concerne_que_longlet_vise() {
        let node = NodeId::from_raw(1);
        let graph_tab = Tab::Graph(node);
        let mut collapsed = HashSet::new();

        toggle_collapse(&mut collapsed, graph_tab);
        assert_eq!(collapsed, HashSet::from([graph_tab]));

        // B5 (`retours-2026-09-23-bugs.md`) : `Tab::Book` a
        // aussi ce contrôle depuis ce correctif, suivi indépendamment du
        // graphe même quand son identifiant de nœud coïncide (test de
        // présentation).
        let book_tab = Tab::Book(node);
        toggle_collapse(&mut collapsed, book_tab);
        assert_eq!(collapsed, HashSet::from([graph_tab, book_tab]));

        toggle_collapse(&mut collapsed, graph_tab);
        assert_eq!(collapsed, HashSet::from([book_tab]));

        toggle_collapse(&mut collapsed, book_tab);
        assert!(collapsed.is_empty());
    }

    #[test]
    fn le_repli_reste_refuse_aux_matrices_et_aux_notes() {
        let node = NodeId::from_raw(1);
        let mut collapsed = HashSet::new();

        toggle_collapse(&mut collapsed, Tab::Matrix(node));
        assert!(
            collapsed.is_empty(),
            "une matrice n'a pas ce contrôle : rien à replier"
        );

        toggle_collapse(&mut collapsed, Tab::Note(node));
        assert!(
            collapsed.is_empty(),
            "une note n'a pas ce contrôle : rien à replier"
        );

        // Le refus ne touche pas au repli des autres onglets.
        toggle_collapse(&mut collapsed, Tab::Graph(node));
        toggle_collapse(&mut collapsed, Tab::Matrix(node));
        assert_eq!(collapsed, HashSet::from([Tab::Graph(node)]));
    }

    /// [`toggle_collapse_to`] (réattachement d'une fenêtre détachée) pose la
    /// valeur demandée au lieu de basculer : la reposer deux fois ne la
    /// retourne pas. Mêmes natures que [`toggle_collapse`].
    #[test]
    fn le_repli_pose_une_valeur_sans_basculer() {
        let node = NodeId::from_raw(1);
        let mut collapsed = HashSet::new();

        toggle_collapse_to(&mut collapsed, Tab::Book(node), true);
        toggle_collapse_to(&mut collapsed, Tab::Book(node), true);
        assert_eq!(collapsed, HashSet::from([Tab::Book(node)]));

        toggle_collapse_to(&mut collapsed, Tab::Book(node), false);
        toggle_collapse_to(&mut collapsed, Tab::Book(node), false);
        assert!(collapsed.is_empty());

        toggle_collapse_to(&mut collapsed, Tab::Matrix(node), true);
        toggle_collapse_to(&mut collapsed, Tab::Note(node), true);
        assert!(
            collapsed.is_empty(),
            "matrices et notes n'ont pas de repli, même posé"
        );
    }

    /// Les fenêtres de l'arbre, à plat, dans l'ordre de l'arbre (le contenu
    /// d'un dossier à sa place) ; les dossiers eux-mêmes n'en sont pas.
    #[test]
    fn les_fenetres_de_l_arbre_sont_mises_a_plat_dans_l_ordre() {
        let window = |raw, kind| ProjectTreeNode::Window {
            id: NodeId::from_raw(raw),
            name: format!("{raw}"),
            kind,
        };
        let tree = vec![
            window(1, WindowKind::Book),
            ProjectTreeNode::Folder {
                id: NodeId::from_raw(2),
                name: "Dossier".to_string(),
                children: vec![
                    window(3, WindowKind::Graph),
                    ProjectTreeNode::Folder {
                        id: NodeId::from_raw(4),
                        name: "Vide".to_string(),
                        children: Vec::new(),
                    },
                    window(5, WindowKind::Note),
                ],
            },
            window(6, WindowKind::Matrix),
        ];
        assert_eq!(
            window_nodes(&tree),
            vec![
                (NodeId::from_raw(1), WindowKind::Book),
                (NodeId::from_raw(3), WindowKind::Graph),
                (NodeId::from_raw(5), WindowKind::Note),
                (NodeId::from_raw(6), WindowKind::Matrix),
            ]
        );
    }

    // B2 (`retours-2026-09-23-bugs.md`) : `sync_floating_tabs_order_to_top_layer`
    // teste la logique pure de réalignement.

    #[test]
    fn le_tab_au_premier_plan_est_ramene_en_fin_de_floating_tabs() {
        let tab_a = Tab::Graph(NodeId::from_raw(1));
        let tab_b = Tab::Graph(NodeId::from_raw(2));
        let tab_c = Tab::Book(NodeId::from_raw(3));
        let mut floating_tabs = vec![tab_a, tab_b, tab_c];

        // `tab_c` est déjà en fin de liste (donc déjà « actif ») : rien à
        // faire.
        sync_floating_tabs_order_to_top_layer(&mut floating_tabs, Some(tab_c));
        assert_eq!(floating_tabs, vec![tab_a, tab_b, tab_c]);

        // `tab_a` est au premier plan (clic direct) sans être en fin de
        // liste : il doit passer en dernier, les autres gardant leur ordre
        // relatif (l'ordre de pile des fenêtres de dessous).
        sync_floating_tabs_order_to_top_layer(&mut floating_tabs, Some(tab_a));
        assert_eq!(floating_tabs, vec![tab_b, tab_c, tab_a]);
    }

    #[test]
    fn aucun_calque_au_premier_plan_ou_tab_inconnu_laisse_floating_tabs_intact() {
        let tab_a = Tab::Graph(NodeId::from_raw(1));
        let tab_b = Tab::Graph(NodeId::from_raw(2));
        let tab_inconnu = Tab::Graph(NodeId::from_raw(3));
        let mut floating_tabs = vec![tab_a, tab_b];

        sync_floating_tabs_order_to_top_layer(&mut floating_tabs, None);
        assert_eq!(floating_tabs, vec![tab_a, tab_b]);

        sync_floating_tabs_order_to_top_layer(&mut floating_tabs, Some(tab_inconnu));
        assert_eq!(
            floating_tabs,
            vec![tab_a, tab_b],
            "un tab qui n'est plus dans floating_tabs (fermé entretemps) ne doit rien perturber"
        );
    }

    // B2, bout à bout : un clic direct dans une fenêtre flottante qui n'est
    // pas déjà en fin de `floating_tabs` amène egui à réordonner son propre
    // calque (`Area`, `Sense::CLICK`) *avant* que `floating_tabs` ne le
    // sache — c'est exactement l'écart que corrige
    // `sync_floating_tabs_order_to_top_layer`. Ce test peint deux fenêtres
    // avec les mêmes `egui::Id` que la production (`egui::Id::new(tab)`),
    // simule un clic dans celle qui n'est pas en fin de liste, puis vérifie
    // que la résolution utilisée en production (`ctx.top_layer_id()` +
    // recherche du tab correspondant) place bien `tab_a` au premier plan —
    // et que sans la resynchronisation, `floating_tabs.last()`
    // (`active_tab`) resterait sur `tab_b`, périmé.
    #[test]
    fn un_clic_direct_dans_une_fenetre_en_arriere_plan_la_ramene_active() {
        let tab_a = Tab::Graph(NodeId::from_raw(1));
        let tab_b = Tab::Graph(NodeId::from_raw(2));
        let mut floating_tabs = vec![tab_a, tab_b];
        let context = egui::Context::default();

        let rect_a = egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(100.0, 100.0));
        let rect_b = egui::Rect::from_min_size(egui::pos2(200.0, 20.0), egui::vec2(100.0, 100.0));

        let paint = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                let viewport = ui.ctx().content_rect();
                for (tab, rect) in [(tab_a, rect_a), (tab_b, rect_b)] {
                    show_floating_window(
                        ui.ctx(),
                        egui::Id::new(tab),
                        rect,
                        viewport,
                        false,
                        |_| {},
                        |_, _| {},
                    );
                }
            });
            output.textures_delta.clear();
        };
        let top_layer_tab = |floating_tabs: &[Tab]| {
            context.top_layer_id().and_then(|top| {
                floating_tabs
                    .iter()
                    .copied()
                    .find(|&tab| egui::Id::new(tab) == top.id)
            })
        };

        // Image de repos : `floating_tabs` place `tab_b` en dernier, comme
        // s'il avait été ouvert (ou cliqué) en dernier légitimement — et egui
        // aussi, puisqu'il l'a peinte en dernier.
        paint(vec![]);
        assert_eq!(top_layer_tab(&floating_tabs), Some(tab_b));

        // Clic dans `tab_a`, qui n'est pas en fin de `floating_tabs` — sur
        // deux images distinctes, comme les autres tests d'interaction
        // (déplacer la souris, puis presser) : `layer_id_at` doit pouvoir
        // s'appuyer sur une position de pointeur déjà connue.
        let click_pos = rect_a.center();
        paint(vec![egui::Event::PointerMoved(click_pos)]);
        assert_eq!(
            top_layer_tab(&floating_tabs),
            Some(tab_b),
            "survoler une fenêtre ne la ramène pas au premier plan"
        );
        paint(vec![egui::Event::PointerButton {
            pos: click_pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        }]);

        let top = top_layer_tab(&floating_tabs);
        assert_eq!(
            top,
            Some(tab_a),
            "egui doit avoir amené la fenêtre cliquée au premier plan de son propre ordre \
             de calques, indépendamment de floating_tabs"
        );

        // Sans la resynchronisation (B2), `active_tab` resterait périmé sur
        // `tab_b` malgré le clic.
        assert_eq!(
            floating_tabs.last().copied(),
            Some(tab_b),
            "avant resynchronisation, floating_tabs (donc active_tab) ignore encore le clic"
        );

        sync_floating_tabs_order_to_top_layer(&mut floating_tabs, top);
        assert_eq!(
            floating_tabs.last().copied(),
            Some(tab_a),
            "après resynchronisation, la fenêtre cliquée doit devenir active_tab \
             (donc se surligner)"
        );
    }

    /// #85 : un clic dans l'Object Manager flottant (calque au-dessus) ne
    /// réactive pas la fenêtre de contenu qu'il recouvre ; un clic sur la
    /// partie découverte de cette même fenêtre, si (témoin).
    #[test]
    fn cliquer_le_gestionnaire_flottant_ne_reactive_pas_le_contenu_derriere() {
        let ctx = egui::Context::default();
        let content_id = egui::Id::new("object_manager_content_test");
        let manager_id = egui::Id::new("object_manager_float_test");
        let content = egui::Rect::from_min_size(egui::pos2(40.0, 40.0), egui::vec2(400.0, 300.0));
        let manager = egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(200.0, 200.0));
        let paint = |events: Vec<egui::Event>, expected: bool| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let viewport = ui.ctx().content_rect();
                    show_window_in_layer(
                        &ctx,
                        manager_id,
                        egui::Order::Foreground,
                        manager,
                        viewport,
                        false,
                        |_| {},
                        |_, _| {},
                    );
                    show_floating_window(
                        &ctx,
                        content_id,
                        content,
                        viewport,
                        false,
                        |_| {},
                        |_, _| {},
                    );
                    assert_eq!(
                        window_pointer_pressed(
                            &ctx,
                            egui::LayerId::new(FLOATING_WINDOW_ORDER, content_id),
                            content
                        ),
                        expected
                    );
                },
            );
            output.textures_delta.clear();
        };
        let press = |pos| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        };
        paint(Vec::new(), false);
        paint(vec![egui::Event::PointerMoved(manager.center())], false);
        paint(vec![press(manager.center())], false);
        paint(
            vec![egui::Event::PointerButton {
                pos: manager.center(),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            false,
        );
        let exposed = egui::pos2(70.0, 70.0);
        paint(vec![egui::Event::PointerMoved(exposed)], false);
        paint(vec![press(exposed)], true);
    }
}
