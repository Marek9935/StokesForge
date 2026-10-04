//! Détachement en fenêtres OS : le Project Manager et chaque fenêtre de
//! contenu peuvent vivre dans un viewport dédié (`show_viewport_deferred`).
//! La fermeture de ce callback est `'static` et `Send + Sync` : elle ne peut
//! pas capturer `&mut AppState`, d'où les vues locales (`DetachedTabViewModel`,
//! `ProjectManagerViewModel`) et les requêtes qu'elles renvoient à la racine.
//!
//! Déplacé tel quel d'`app.rs` (#101), sans changement de comportement.
//!
//! # Repris de la doc de tête d'`app.rs`
//!
//! # Détachement du Project Manager
//!
//! Le flottant `egui_dock` (`Surface::Window`) qu'un onglet pouvait obtenir
//! en le glissant hors du dock n'existe plus pour le Project Manager,
//! puisqu'il n'est plus un onglet — seul reste :
//!
//! **Vraie fenêtre OS séparée** ([`StokesForgeApp::pm_detached`],
//! [`StokesForgeApp::show_detached_project_manager`]) : `egui::Context::
//! show_viewport_deferred` ouvre un vrai viewport natif, déplaçable sur
//! un autre écran. Quand ce mode est actif, [`StokesForgeApp::project_manager_panel`]
//! n'est simplement pas peint cette image-là ; le fermer (bouton
//! de la fenêtre OS, ou « Réattacher ») repose `pm_detached` à `false`, ce
//! qui fait réapparaître le panneau au prochain rendu — toujours au même
//! endroit, puisqu'il est figé à gauche. Déclenchable du menu « Fenêtre » du
//! haut. Sous Wayland, le compositeur peut ignorer une position de fenêtre
//! demandée par l'application — la même réserve que documente déjà
//! `docs/application.md` pour `rfd` ; cette tâche ne demande explicitement
//! aucune position (elle laisse le compositeur placer la fenêtre), donc n'en
//! souffre pas directement, mais la limite vaut d'être connue avant d'y
//! ajouter un jour une position mémorisée.
//!
//! **`show_viewport_deferred`, pas `_immediate`** : constaté
//! interactivement (Fedora/GNOME/Wayland, Mutter), la fenêtre détachée via
//! `show_viewport_immediate` restait draggable mais visuellement confinée
//! au rectangle de la fenêtre mère, même réduite — pas seulement
//! l'absence de positionnement absolu ci-dessus, autre chose empêchait la
//! fenêtre de sortir du cadre. `show_viewport_immediate` documente
//! lui-même « pause the current viewport and render the child viewport in
//! its own window » (rendu couplé des deux fenêtres), et `egui`
//! recommande explicitement `show_viewport_deferred` pour une vraie
//! fenêtre indépendante, repeinte indépendamment de la fenêtre parente —
//! un candidat plausible pour ce symptôme, **pas une preuve**. Détail du
//! choix, de l'incertitude assumée et du schéma de synchronisation
//! (`ProjectManagerViewModel`, `DetachedProjectManagerRequests`) dans la
//! doc de tête de [`StokesForgeApp::show_detached_project_manager`].
//!
//! # Détachement générique des onglets de contenu
//!
//! [`StokesForgeApp::detach_tab`]/[`StokesForgeApp::reattach_tab`]/
//! [`StokesForgeApp::show_detached_tabs`] généralisent le mécanisme ci-dessus à
//! [`Tab::Book`], [`Tab::Graph`], [`Tab::Matrix`] et [`Tab::Note`] — un
//! bouton « Détacher dans une fenêtre » dans l'en-tête de chaque fenêtre
//! flottante (voir § Fenêtres flottantes confinées), symétrique de
//! « Réattacher à la fenêtre principale » côté fenêtre détachée. Plusieurs
//! onglets peuvent être détachés en même temps :
//! [`StokesForgeApp::detached_tabs`] est une table par [`Tab`] (chaque variante
//! porte déjà un `NodeId`, donc un `ViewportId::from_hash_of(tab)` est
//! unique par construction — pas de collision possible entre deux classeurs
//! détachés simultanément, par exemple).
//!
//! B8 (`retours-2026-09-23-bugs.md`) : comme pour une fenêtre
//! flottante rétrécie (B4), un `Ui` dont les enfants dépassent son
//! `max_rect` les peint quand même. [`StokesForgeApp::show_detached_tabs`] pose
//! donc le contenu des quatre natures dans un même sous-`Ui` borné et rogné,
//! juste avant le `match` qui aiguille vers chacune d'elles.
//!
//! # Pourquoi pas un `ViewModel` par nature de contenu comme pour le PM
//!
//! Le PM n'a qu'une seule forme de contenu (un arbre en lecture seule) ; les
//! quatre natures de fenêtre ici sont franchement différentes (grille
//! éditable virtualisée, graphe interactif avec cache de formes, texte
//! éditable, espace réservé). Plutôt que quatre `ViewModel`/`Requests`
//! disjoints à faire cohabiter dans [`StokesForgeApp::detached_tabs`],
//! [`DetachedTabViewModel`] est une seule structure portant un champ par
//! nature de contenu (`view_state` pour un classeur ou une note,
//! `graph_view` pour un graphe) — les champs
//! inutilisés pour la nature du `Tab` qu'elle sert restent simplement à leur
//! valeur par défaut. Moins de types, un seul `Arc<Mutex<_>>` par onglet
//! détaché à faire voyager dans la fermeture, au prix d'un typage un peu
//! large ; le commentaire de tête de chaque champ dit à qui il appartient.
//!
//! # Le classeur partage son rendu entre les deux modes
//!
//! [`tab_contents`] (la fonction déjà partagée par les fenêtres flottantes et
//! la zone de maximisation) prend `&mut AppState` — un onglet détaché ne peut
//! pourtant pas capturer `&mut AppState` dans la fermeture `'static`,
//! `Send + Sync`, exigée par `show_viewport_deferred`, exactement le
//! problème déjà résolu pour le Project Manager (voir plus haut) : mettre
//! `AppState` entière derrière un verrou déborderait le problème à
//! résoudre, pour un onglet comme pour l'autre.
//!
//! Le contournement retenu : [`DetachedTabViewModel::snapshot`], un clone de
//! [`stokesforge_session::Snapshot`] (bon marché — deux `Arc`, voir sa doc), est
//! resynchronisé depuis `self.state` à chaque image, **avant** l'appel à
//! `show_viewport_deferred` — même schéma que `ProjectManagerViewModel::tree`.
//! Une publication dont l'époque, la révision métier ou celle des géométries
//! change réveille le viewport destinataire (#104) : publier l'instantané
//! seul ne déclenche pas le callback différé. Au repos, aucun réveil périodique.
//! Une mutation appliquée après la peinture programme aussi une nouvelle
//! image racine pour publier cet état avant le redessin des lecteurs.
//! Toute lecture pure (texte de cellule, colonnes visibles, feuilles d'un
//! classeur, texte d'une note) passe par les fonctions libres de
//! `crate::state` factorisées sur un `&Tree` pour l'occasion
//! (`cell_text_in`, `visible_columns_in`, `row_count_in`, `worksheets_of_in`,
//! `note_text_in`, `node_title_in`) — le corps exact de la méthode
//! `AppState` correspondante, pas une réécriture : une fenêtre flottante et
//! une fenêtre détachée affichent donc littéralement la même règle, jamais
//! deux versions à tenir en accord.
//!
//! Le classeur détaché possède une vue [`AppState`] locale construite par
//! [`AppState::fork_for_detached_view`]. Elle conserve sélection, presse-papier
//! et tampons de saisie, mais lit le même instantané de session que la racine.
//! Ainsi [`book_contents`] et [`grid_contents`] sont appelées à l'identique
//! dans les deux modes : onglets, en-têtes, largeurs et gestes ne peuvent
//! plus dériver dans deux implémentations. Les actions mutantes rejoignent
//! la boucle principale après [`AppState::adopt_detached_book_view`] ; elles
//! gardent donc l'unique voie de commande annulable. La note suit le même
//! motif : [`note_contents`] sur la vue locale, dont le tampon de saisie
//! ([`AppState::note_editing`]) reste propre à la fenêtre.
//!
//! [`Tab::Graph`] échappe à cette limite : [`stokesforge_render_egui::GraphView`]
//! n'a pas de verrou global à `AppState` à respecter (son cache de formes et
//! sa navigation zoom/pan sont déjà scopés par graphe, dans
//! [`StokesForgeApp::graph_views`]) — [`graph_view_contents`] est donc
//! **littéralement** la même fonction pour une fenêtre flottante et pour une
//! fenêtre détachée, la vue elle-même migrant simplement hors de `graph_views` le
//! temps du détachement ([`StokesForgeApp::detach_tab`]/[`StokesForgeApp::
//! reattach_tab`]) pour rester un `Arc<Mutex<_>>` capturable par la
//! fermeture sans dupliquer le cache.
//!
//! [`Tab::Matrix`] n'a aucun état à faire voyager (espace réservé) :
//! [`matrix_contents`] ne lit que l'arbre et sert tel quel aux deux
//! fenêtres.
//!
//! La console de script n'entre pour rien dans ce mécanisme : ce n'est pas
//! une fenêtre de contenu (voir § Console de script ci-dessus), donc rien à
//! détacher ici — le bouton « Détacher dans une fenêtre » n'existe que dans
//! l'en-tête des fenêtres flottantes `Book`/`Graph`/`Matrix`/`Note`.
//!

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use eframe::egui;
use stokesforge_core::NodeId;
use stokesforge_render_egui::GraphView;
use stokesforge_session::{Snapshot, WindowGeometry};

use crate::state::{Action, AppState, ProjectTreeNode};
use crate::tr;

use super::shortcuts::collect_undo_redo_shortcuts;
use super::tab::{tab_node, Tab};
use super::window_chrome::{
    detached_resize_handles, paint_active_window_outline, show_window_contents_bounded,
    window_header, HeaderPlacement, WindowHeaderActions, MIN_CONTENT_WINDOW_SIZE,
    WINDOW_CONTENT_MARGIN, WINDOW_HEADER_HEIGHT,
};
use super::window_placement::{floating_window_display_size, GRAPH_WINDOW_SIZE};
use super::window_stack::toggle_collapse_to;
use super::{
    begin_file_drop, book_contents, finish_file_drop, graph_view_contents, matrix_contents,
    note_contents, project_manager_contents_from_tree, selected_manager_target, snapshot_stamp,
    tab_title, tab_title_from_tree, PmInteraction, StokesForgeApp, PROJECT_MANAGER_WIDTH,
};

/// Ce que la fenêtre OS détachée du Project Manager a besoin de peindre —
/// strictement ça, et rien de plus. Voir la doc de tête de
/// [`StokesForgeApp::show_detached_project_manager`] pour pourquoi ce détour
/// plutôt que de mettre `AppState` entière derrière un verrou.
#[derive(Debug, Default, Clone)]
pub(super) struct ProjectManagerViewModel {
    /// Dernier instantané publié ; une publication modifiée réveille le viewport.
    pub(super) snapshot_stamp: Option<(u64, u64, u64)>,
    /// Copie de [`AppState::project_tree`], resynchronisée à chaque image
    /// par [`StokesForgeApp::show_detached_project_manager`] avant l'appel à
    /// `show_viewport_deferred` — `AppState` reste la seule source de
    /// vérité, ceci n'en est qu'un instantané pour le callback différé.
    pub(super) tree: Vec<ProjectTreeNode>,
    /// Copie de [`StokesForgeApp::pm_selection`], même resynchronisation que
    /// `tree` — nécessaire pour que la fenêtre détachée du Project Manager
    /// affiche elle aussi la sélection courante et le fond « Masquer » à
    /// jour (§6).
    pub(super) selected: HashSet<Tab>,
    /// Copie de [`StokesForgeApp::hidden_tabs`], même resynchronisation.
    pub(super) hidden: HashSet<Tab>,
    /// Copie de l'onglet au premier plan, même resynchronisation — pour le
    /// double-clic « fermer si déjà au premier plan » (§4).
    pub(super) active_tab: Option<Tab>,
}

/// Ce que la fenêtre OS détachée du Project Manager écrit, pour que la
/// boucle principale (seule à porter `&mut AppState` et `&mut
/// StokesForgeApp::floating_tabs`) les applique après le rendu — même schéma que
/// les `open_requests`/
/// `actions` locaux à [`StokesForgeApp::ui`], juste posés par un callback différé
/// (potentiellement rappelé à une image où la fenêtre mère ne peint pas)
/// plutôt que collectés en direct pendant la peinture.
#[derive(Debug, Default)]
pub(super) struct DetachedProjectManagerRequests {
    /// Dépôts de fichiers, traités par la même session que la fenêtre principale.
    pub(super) actions: Vec<Action>,
    /// Onglets demandés par un clic dans l'arbre depuis la dernière
    /// vidange ; [`StokesForgeApp::ui`] les transfère dans son propre
    /// `open_requests` puis les vide.
    pub(super) open: Vec<Tab>,
    /// Onglets fermés par un double-clic depuis la dernière vidange (§4) —
    /// même schéma que `open`.
    pub(super) close: Vec<Tab>,
    /// Vrai si le bouton « Réattacher » ou la fermeture de la fenêtre OS a
    /// été actionné depuis la dernière vidange.
    pub(super) reattach: bool,
    /// Sélection/masquage tels qu'ils étaient à la fin de la dernière image
    /// peinte par la fenêtre détachée — `None` tant qu'aucune image n'a
    /// encore tourné depuis la dernière vidange. Remplacés en bloc plutôt
    /// qu'en delta, comme `DetachedTabRequests::geometry` (§6).
    pub(super) selection: Option<HashSet<Tab>>,
    pub(super) hidden: Option<HashSet<Tab>>,
}

/// Un onglet de contenu détaché en fenêtre OS séparée — voir la doc de tête
/// du module, § détachement générique. Même patron que le Project Manager
/// (`pm_view_model`/`pm_requests`), répété une fois par onglet plutôt qu'une
/// fois pour toute l'appli : [`StokesForgeApp::detached_tabs`] en porte une par
/// entrée.
pub(super) struct DetachedTab {
    /// Ce que la fenêtre détachée peint, resynchronisé depuis `self.state`
    /// (et `self.graph_views` pour un graphe) à chaque image par
    /// [`StokesForgeApp::show_detached_tabs`].
    pub(super) view_model: Arc<Mutex<DetachedTabViewModel>>,
    /// Ce que la fenêtre détachée écrit, drainé chaque image par
    /// [`StokesForgeApp::ui`].
    pub(super) requests: Arc<Mutex<DetachedTabRequests>>,
}

/// Ce qu'une fenêtre détachée a besoin de peindre. Une seule structure pour
/// les quatre natures détachables plutôt que quatre `ViewModel` disjoints —
/// voir la doc de tête du module pour le compromis. Chaque champ, hormis
/// `snapshot`, n'a de sens que pour une nature de [`Tab`] particulière ; le
/// commentaire le dit.
pub(super) struct DetachedTabViewModel {
    /// Instantané du modèle, resynchronisé à chaque image — voir la doc de
    /// tête du module, § « la lecture du modèle est factorisée ». `None`
    /// seulement avant la toute première synchronisation (jamais observable
    /// depuis l'extérieur de [`StokesForgeApp::detach_tab`], qui le pose aussitôt).
    pub(super) snapshot: Option<Snapshot>,
    /// `Tab::Book`/`Tab::Note` : vue [`AppState`] locale à la fenêtre, qui
    /// rend le contenu commun avec la fenêtre flottante (mêmes
    /// `book_contents`/`note_contents`) tout en gardant sa sélection et ses
    /// saisies en cours.
    pub(super) view_state: Option<AppState>,
    /// `Tab::Graph` : la vue (cache de formes, zoom/pan), déplacée hors de
    /// [`StokesForgeApp::graph_views`] le temps du détachement — voir
    /// [`StokesForgeApp::detach_tab`]/[`StokesForgeApp::reattach_tab`].
    pub(super) graph_view: Option<GraphView>,
    /// `Tab::Graph`/`Tab::Book` : repli du contenu, propre à cette fenêtre
    /// détachée — initialisé depuis [`StokesForgeApp::collapsed_tabs`] par
    /// `detach_tab`, basculé directement ici (pas de détour par
    /// `DetachedTabRequests`, ce repli n'intéresse que cette fenêtre), et
    /// reversé dans `collapsed_tabs` par `reattach_tab`.
    pub(super) collapsed: bool,
    /// Le premier callback d'un viewport différé peut arriver avant que
    /// Wayland/WGPU ait stabilisé sa taille. Une seule image supplémentaire
    /// donne alors à la figure son rectangle réel, sans entretenir une boucle
    /// de rendu quand la fenêtre reste inactive.
    pub(super) needs_initial_repaint: bool,
    /// Dernière taille OS connue de la fenêtre **hors repli** (B9,
    /// `retours-2026-09-23-bugs.md`) : mise à jour à chaque
    /// image tant que `collapsed` est faux, gelée pendant un repli. Sert à
    /// la fois de cible pour le `ViewportCommand::InnerSize` envoyé au
    /// dépli et de taille à persister dans `window_geometries` pendant un
    /// repli — même rôle que `current_size`/`window_sizes` côté fenêtre
    /// flottante (B3), mais porté par le modèle plutôt que par une table de
    /// l'app puisqu'une fenêtre détachée n'a pas d'équivalent. `None`
    /// seulement avant la première image où le viewport rapporte une
    /// taille (ou si la fenêtre a été détachée déjà repliée et jamais
    /// dépliée depuis).
    pub(super) expanded_size: Option<egui::Vec2>,
    pub(super) focused: bool,
}

/// Ce qu'une fenêtre détachée écrit, pour que [`StokesForgeApp::ui`] (seule à
/// porter `&mut AppState`) les applique après le rendu — même rôle que
/// [`DetachedProjectManagerRequests`].
#[derive(Debug, Default)]
pub(super) struct DetachedTabRequests {
    /// Actions produites par cette fenêtre depuis la dernière vidange
    /// (validation de cellule/note, structure de la grille…) — le même
    /// chemin de commande annulable qu'une fenêtre flottante, voir la doc de
    /// tête du module.
    pub(super) actions: Vec<Action>,
    /// Vrai si « Réattacher à la fenêtre principale » a été actionné depuis
    /// la dernière vidange.
    pub(super) reattach: bool,
    /// La croix du système ferme la vue détachée sans la réattacher.
    pub(super) close: bool,
}

impl StokesForgeApp {
    /// Bascule en mode détaché : le prochain rendu ouvre le Project Manager
    /// dans sa propre fenêtre OS ([`StokesForgeApp::show_detached_project_manager`])
    /// au lieu du panneau fixe ([`StokesForgeApp::project_manager_panel`]). Sans
    /// effet s'il l'est déjà.
    pub(super) fn detach_project_manager(&mut self) {
        self.pm_detached = true;
    }

    /// Bascule hors du mode détaché : le panneau fixe réapparaît au prochain
    /// rendu, toujours à gauche puisqu'il est figé en position. Sans effet
    /// s'il n'est pas détaché.
    pub(super) fn reattach_project_manager(&mut self) {
        self.pm_detached = false;
    }

    /// Peint le Project Manager dans sa propre fenêtre OS native, via un
    /// viewport `eframe`/`egui` **différé** — voir la doc de tête du module,
    /// § détachement.
    ///
    /// # Pourquoi `show_viewport_deferred`, pas `_immediate`
    ///
    /// Ce module utilisait `show_viewport_immediate` jusqu'ici — le choix le
    /// plus simple, jugé suffisant tant que ce panneau n'anime rien en
    /// continu. Constat interactif de l'utilisateur (Fedora/GNOME/Wayland,
    /// compositeur Mutter) : la fenêtre détachée était bien draggable, mais
    /// restait **confinée visuellement au rectangle de la fenêtre mère**,
    /// même celle-ci réduite (pas plein écran) — pas seulement l'absence de
    /// positionnement absolu propre à Wayland (déjà documentée ci-dessus
    /// pour `rfd`), autre chose empêchait la fenêtre de sortir du cadre.
    /// `egui::Context::show_viewport_immediate` documente lui-même : « This
    /// call will pause the current viewport and render the child viewport
    /// in its own window » — ce couplage de rendu synchronisé entre les deux
    /// fenêtres est un candidat plausible pour mal interagir avec le modèle
    /// de frame de Wayland. `egui::Context::show_viewport_deferred` est la
    /// forme que la doc `egui` recommande explicitement pour une vraie
    /// fenêtre indépendante (« To avoid this, use show_viewport_deferred
    /// instead »), repeinte indépendamment de la fenêtre parente.
    ///
    /// **C'est un test, pas une garantie.** Rien ne prouve que le symptôme
    /// observé côté Mutter vienne de ce couplage de rendu plutôt que d'autre
    /// chose (un comportement du compositeur vis-à-vis des sous-fenêtres en
    /// général, par exemple). Si l'utilisateur constate après coup que la
    /// fenêtre détachée reste toujours confinée, ce sera à documenter comme
    /// limite connue — à côté de la réserve Wayland déjà notée pour la
    /// position — plutôt qu'à retenter une autre bascule sans piste
    /// nouvelle.
    ///
    /// # Callback `Fn`, pas de `&mut self`
    ///
    /// `show_viewport_deferred` exige `impl Fn(&mut Ui, ViewportClass) +
    /// Send + Sync + 'static` : pas de capture de `&mut self`, pas même
    /// d'emprunt classique de `AppState` — le callback peut survivre à cette
    /// image et être rappelé seul, à une image ultérieure de la fenêtre
    /// détachée pendant que la fenêtre mère dort. Mettre `AppState` entière
    /// derrière un verrou pour lui donner ce dont elle a besoin
    /// déborderait largement du problème à résoudre (dispatch, boucle de
    /// rendu principale) ; à la place, seul le strict nécessaire au panneau
    /// voyage :
    ///
    /// - [`ProjectManagerViewModel`] (l'arbre du projet) est resynchronisé
    ///   ici depuis `self.state` — la vraie source de vérité — à chaque
    ///   image, **avant** l'appel à `show_viewport_deferred` ;
    /// - [`DetachedProjectManagerRequests`] recueille dans l'autre sens ce
    ///   que le callback produit (clics d'ouverture, réattachement demandé) ;
    ///   [`StokesForgeApp::ui`] la draine juste après cet appel pour produire les
    ///   mêmes `open_requests`/`Action` qu'avant, sans dupliquer la logique
    ///   de [`StokesForgeApp::open_tab`] ni appeler
    ///   [`StokesForgeApp::reattach_project_manager`] depuis l'intérieur d'un
    ///   callback qui n'a pas accès à `&mut self`.
    pub(super) fn show_detached_project_manager(
        &mut self,
        ctx: &egui::Context,
        active_tab: Option<Tab>,
    ) {
        let changed = {
            let mut model = self
                .pm_view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            let stamp = snapshot_stamp(self.state.snapshot());
            let changed = model.snapshot_stamp != Some(stamp)
                || model.selected != self.pm_selection
                || model.hidden != self.hidden_tabs
                || model.active_tab != active_tab;
            model.snapshot_stamp = Some(stamp);
            model.tree = self.state.project_tree();
            model.selected = self.pm_selection.clone();
            model.hidden = self.hidden_tabs.clone();
            model.active_tab = active_tab;
            changed
        };
        let view_model = Arc::clone(&self.pm_view_model);
        let requests = Arc::clone(&self.pm_requests);
        let root_context = ctx.clone();
        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("stokesforge_project_manager_window"),
            egui::ViewportBuilder::default()
                .with_title(tr!("window-title-project-manager"))
                .with_inner_size([PROJECT_MANAGER_WIDTH + 40.0, 480.0]),
            move |ui, _class| {
                begin_file_drop(ui.ctx());
                let mut opened = Vec::new();
                let mut closed = Vec::new();
                // Lecture d'une copie plutôt que du verrou tenu pendant
                // toute la peinture : `project_manager_contents_from_tree`
                // ne doit pas retenir `view_model` verrouillé le temps
                // d'un `ScrollArea` entier. La sélection/le masquage sont
                // mutés sur ces copies puis renvoyés en bloc après coup
                // (`selection`/`hidden` sur `DetachedProjectManagerRequests`).
                let mut model = view_model
                    .lock()
                    .unwrap_or_else(|err| err.into_inner())
                    .clone();
                egui::CentralPanel::default().show(ui, |ui| {
                    if ui
                        .button(tr!("action-reattach-project-manager"))
                        .on_hover_text(tr!("action-reattach-project-manager", "hint"))
                        .clicked()
                    {
                        requests
                            .lock()
                            .unwrap_or_else(|err| err.into_inner())
                            .reattach = true;
                        root_context.request_repaint_of(egui::ViewportId::ROOT);
                    }
                    ui.separator();
                    let mut interaction = PmInteraction {
                        open_requests: &mut opened,
                        close_requests: &mut closed,
                        active_tab: model.active_tab,
                        selection: &mut model.selected,
                        hidden: &mut model.hidden,
                    };
                    project_manager_contents_from_tree(ui, &model.tree, &mut interaction);
                });
                {
                    let mut requests = requests.lock().unwrap_or_else(|err| err.into_inner());
                    requests.open.append(&mut opened);
                    requests.close.append(&mut closed);
                    requests.selection = Some(model.selected);
                    requests.hidden = Some(model.hidden);
                    if let Some(action) = finish_file_drop(ui.ctx()) {
                        requests.actions.push(action);
                        root_context.request_repaint_of(egui::ViewportId::ROOT);
                    }
                }
                // L'utilisateur a cliqué le bouton de fermeture de cette
                // fenêtre OS : on la referme proprement en la réattachant au
                // dock plutôt que de laisser le panneau disparaître sans
                // recours.
                if ui.ctx().input(|input| input.viewport().close_requested()) {
                    requests
                        .lock()
                        .unwrap_or_else(|err| err.into_inner())
                        .reattach = true;
                }
            },
        );
        if changed {
            ctx.request_repaint_of(egui::ViewportId::from_hash_of(
                "stokesforge_project_manager_window",
            ));
        }
    }

    /// Retire `tab` des fenêtres flottantes et le détache en fenêtre OS
    /// séparée — voir la doc de tête du module, § détachement générique. Sans
    /// effet si `tab` est déjà détaché ou n'est pas une fenêtre flottante
    /// actuellement ouverte. Ni le Project Manager ni la console de script
    /// n'entrent dans ce chemin depuis leur passage en panneaux fixes — voir
    /// [`StokesForgeApp::detach_project_manager`] pour le premier ; la seconde n'a
    /// simplement plus de mécanisme de détachement du tout.
    pub(super) fn detach_tab(&mut self, tab: Tab) {
        if self.detached_tabs.contains_key(&tab) {
            return;
        }
        if let Some(pos) = self.floating_tabs.iter().position(|t| *t == tab) {
            self.floating_tabs.remove(pos);
            self.cascade_rank.remove(&tab);
        } else {
            return;
        }
        if self.maximized_tab == Some(tab) {
            // Un onglet maximisé qui ne serait plus une fenêtre flottante
            // serait un état incohérent : redescendre proprement avant de le
            // détacher.
            self.maximized_tab = None;
        }
        let graph_view = match tab {
            Tab::Graph(id) => {
                let mut view = self.graph_views.remove(&id).unwrap_or_default();
                view.invalidate_surface();
                Some(view)
            }
            _ => None,
        };
        self.window_positions.remove(&tab);
        self.window_sizes.remove(&tab);
        let view_state = match tab {
            Tab::Book(_) | Tab::Note(_) => self.state.fork_for_detached_view().ok(),
            _ => None,
        };
        let view_model = DetachedTabViewModel {
            snapshot: Some(self.state.snapshot().clone()),
            view_state,
            graph_view,
            needs_initial_repaint: true,
            collapsed: self.collapsed_tabs.contains(&tab),
            focused: false,
            expanded_size: None,
        };
        self.detached_tabs.insert(
            tab,
            DetachedTab {
                view_model: Arc::new(Mutex::new(view_model)),
                requests: Arc::new(Mutex::new(DetachedTabRequests::default())),
            },
        );
    }

    /// Retire `tab` de [`StokesForgeApp::detached_tabs`] et le repousse en fenêtre
    /// flottante (nouveau rang de cascade). Sans effet s'il n'est pas
    /// détaché. Une éventuelle [`GraphView`] déplacée par
    /// [`StokesForgeApp::detach_tab`] revient dans [`StokesForgeApp::graph_views`] — le
    /// cache de formes du graphe survit au détachement/réattachement, comme
    /// il survit déjà à la fermeture d'une fenêtre flottante.
    pub(super) fn reattach_tab(&mut self, tab: Tab) {
        let Some(detached) = self.detached_tabs.remove(&tab) else {
            return;
        };
        {
            let mut model = detached
                .view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            if let (Tab::Graph(id), Some(view)) = (tab, model.graph_view.take()) {
                let mut view = view;
                view.invalidate_surface();
                self.graph_views.insert(id, view);
            }
            toggle_collapse_to(&mut self.collapsed_tabs, tab, model.collapsed);
        }
        if !self.floating_tabs.contains(&tab) {
            self.push_floating_tab(tab);
        }
    }

    /// Ferme uniquement la vue détachée. Les données restent dans le projet
    /// et le cache du graphe revient dans l'application, comme après la
    /// fermeture d'une fenêtre flottante.
    pub(super) fn close_detached_tab(&mut self, tab: Tab) {
        let Some(detached) = self.detached_tabs.remove(&tab) else {
            return;
        };
        let mut model = detached
            .view_model
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        if let (Tab::Graph(id), Some(view)) = (tab, model.graph_view.take()) {
            let mut view = view;
            view.invalidate_surface();
            self.graph_views.insert(id, view);
        }
    }

    /// Envoie la géométrie réellement retenue par egui après un déplacement
    /// ou redimensionnement. Cette information de vue est persistante mais
    /// reste hors de l'historique métier.
    pub(super) fn remember_window_geometry(&mut self, tab: Tab, rect: egui::Rect) {
        self.floating_rects.insert(tab_node(tab), rect);
    }

    /// Envoie à la session la géométrie flottante de chaque fenêtre connue
    /// avec son état du moment (ouverte/maximisée/fermée, repliée, rang
    /// d'empilement) — uniquement ce qui a changé depuis le dernier envoi.
    /// Une fenêtre détachée compte comme ouverte : le détachement ne se
    /// retient pas, et la position de sa fenêtre OS n'est jamais écrite.
    pub(super) fn sync_window_views(&mut self) {
        let open_order: Vec<NodeId> = self
            .floating_tabs
            .iter()
            .map(|tab| tab_node(*tab))
            .collect();
        let mut detached: Vec<NodeId> = self.detached_tabs.keys().map(|t| tab_node(*t)).collect();
        detached.sort_by_key(|node| node.raw());
        let nodes: Vec<NodeId> = self.floating_rects.keys().copied().collect();
        for node in nodes {
            let rect = self.floating_rects[&node];
            let stack = open_order
                .iter()
                .position(|n| *n == node)
                .or_else(|| {
                    detached
                        .iter()
                        .position(|n| *n == node)
                        .map(|i| open_order.len() + i)
                })
                .unwrap_or(0);
            let is_open = open_order.contains(&node) || detached.contains(&node);
            let mode = if self.maximized_tab.map(tab_node) == Some(node) {
                stokesforge_session::WindowMode::Maximized
            } else if is_open {
                stokesforge_session::WindowMode::Open
            } else {
                stokesforge_session::WindowMode::Closed
            };
            let collapsed = self.collapsed_tabs.iter().any(|tab| tab_node(*tab) == node);
            let geometry = WindowGeometry {
                x: f64::from(rect.min.x),
                y: f64::from(rect.min.y),
                width: f64::from(rect.width()),
                height: f64::from(rect.height()),
                view: Some(stokesforge_session::WindowView {
                    mode,
                    collapsed,
                    stack: u32::try_from(stack).unwrap_or(u32::MAX),
                }),
            };
            if self.saved_window_geometries.get(&node) != Some(&geometry) {
                self.state.set_window_geometry(node, geometry);
                self.saved_window_geometries.insert(node, geometry);
            }
        }
    }

    /// Peint chaque onglet de [`StokesForgeApp::detached_tabs`] dans sa propre
    /// fenêtre OS, via `show_viewport_deferred` — voir la doc de tête du
    /// module, § détachement générique, et [`StokesForgeApp::
    /// show_detached_project_manager`] pour la justification du choix
    /// `_deferred`/`Fn` déjà faite pour le Project Manager (identique ici).
    ///
    /// Ne draine **pas** les requêtes produites : [`StokesForgeApp::ui`] le fait
    /// juste après, une fois cette fonction revenue, exactement comme pour
    /// `pm_requests`.
    pub(super) fn show_detached_tabs(&mut self, ctx: &egui::Context) {
        let Self {
            state,
            detached_tabs,
            ..
        } = self;
        for (tab, detached) in detached_tabs.iter() {
            let changed = {
                let mut model = detached
                    .view_model
                    .lock()
                    .unwrap_or_else(|err| err.into_inner());
                let changed = model.snapshot.as_ref().map(snapshot_stamp)
                    != Some(snapshot_stamp(state.snapshot()));
                model.snapshot = Some(state.snapshot().clone());
                if let Some(view_state) = model.view_state.as_mut() {
                    view_state.sync_detached_snapshot(state.snapshot().clone());
                }
                changed
            };
            let view_model = Arc::clone(&detached.view_model);
            let requests = Arc::clone(&detached.requests);
            let root_context = ctx.clone();
            let tab = *tab;
            let title = tr!("window-title-detached", name = tab_title(state, tab));
            // `Tab::Graph` part à une taille qui contient sa figure de
            // référence et son retour au cadrage initial.
            // Le reste (`Book`/`Matrix`/`Note`) partage une taille uniforme.
            // La grille du classeur emploie maintenant les mêmes largeurs
            // et en-têtes que dans la fenêtre flottante.
            // `Tab::Note` n'a pas de grille à proprement parler mais reçoit
            // la même taille que « les autres » pour rester cohérente avec
            // eux — c'est explicitement voulu, pas un oubli.
            let size = match tab {
                Tab::Graph(_) => [GRAPH_WINDOW_SIZE.x, GRAPH_WINDOW_SIZE.y],
                _ => [420.0, 360.0],
            };
            // Sans décoration native : la barre maison (`window_header`,
            // placement `Detached`) est tout ce que l'utilisateur a pour
            // déplacer/redimensionner/fermer cette fenêtre — voir la doc de
            // tête du module, § détachement générique, et
            // [`detached_resize_handles`] pour le redimensionnement.
            // Ni position ni taille mémorisées : la fenêtre OS détachée n'a
            // pas de géométrie persistée (issue #16), seule la géométrie
            // flottante l'est.
            let viewport = egui::ViewportBuilder::default()
                .with_title(title)
                .with_decorations(false)
                .with_inner_size(size)
                // Le repli B9 doit pouvoir réduire la hauteur sous le minimum
                // habituel. Sous cette largeur, les onglets de feuille (taille
                // naturelle) défilent au lieu de rétrécir — voir `sheet_tabs`.
                .with_min_inner_size([MIN_CONTENT_WINDOW_SIZE.x, WINDOW_HEADER_HEIGHT]);
            let id = egui::Id::new(tab);
            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of(tab),
                viewport,
                move |ui, _class| {
                    begin_file_drop(ui.ctx());
                    let mut model = view_model.lock().unwrap_or_else(|err| err.into_inner());
                    let mut requests = requests.lock().unwrap_or_else(|err| err.into_inner());
                    let was_collapsed = model.collapsed;
                    // B9 (`retours-2026-09-23-bugs.md`) : la
                    // taille OS « logique » (hors repli) se mémorise nous-
                    // mêmes plutôt que de se déduire du rectangle rapporté
                    // cette image — pendant un repli ce rectangle est
                    // volontairement réduit (voir plus bas), et le geler ici
                    // est ce qui permet à la fois de retrouver l'exacte
                    // taille d'avant repli au dépli et de ne pas persister la
                    // taille repliée (même piège que
                    // `floating_window_geometry_to_persist`, B3). Lu avant
                    // tout traitement du clic de cette image : `was_collapsed`
                    // reflète l'état d'entrée, pas un éventuel bascule de
                    // cette image même.
                    if !was_collapsed {
                        if let Some(rect) = ui.ctx().input(|input| input.viewport().outer_rect) {
                            model.expanded_size = Some(rect.size());
                        }
                    }
                    let mut header_actions = WindowHeaderActions::default();
                    // Surbrillance (B1/B3 suite) : une fenêtre détachée est
                    // un viewport OS séparé, donc « active » n'a de sens
                    // qu'au sens du focus OS — `active_tab`/`floating_tabs`
                    // ne le couvrent pas. `RawInput::focused` est déjà
                    // rapporté par le backend par viewport.
                    let is_active = ui.input(|input| input.focused);
                    if model.focused != is_active {
                        model.focused = is_active;
                        root_context.request_repaint_of(egui::ViewportId::ROOT);
                    }
                    // Trait par défaut (inactif) uniquement — le contour actif
                    // n'est plus recoloré ici (B7,
                    // `retours-2026-09-23-bugs.md`) : la
                    // géométrie de ce `Frame` suit `content_ui.min_rect()`
                    // (`containers/frame.rs`), et `CentralPanel::show_inside_dyn`
                    // rogne son affichage à `outer_rect` *avant* de le peindre
                    // — le débordement logique de contenu (bande d'outils qui
                    // ne s'enroule pas) faisait donc disparaître le contour
                    // sur les bords qui débordent, au lieu de le montrer.
                    // L'état actif est peint à part, après `.show()`, sur
                    // `input.content_rect()` — le rectangle du viewport OS
                    // réel, pas celui qu'egui recalcule.
                    let frame = egui::Frame::central_panel(ui.style())
                        .inner_margin(WINDOW_CONTENT_MARGIN)
                        .stroke(ui.style().visuals.window_stroke());
                    egui::CentralPanel::default().frame(frame).show(ui, |ui| {
                        let Some(snapshot) = model.snapshot.clone() else {
                            // Ne devrait jamais arriver : `detach_tab` pose le
                            // premier instantané avant toute image détachée.
                            return;
                        };
                        header_actions = window_header(
                            ui,
                            id,
                            &tab_title_from_tree(&snapshot.tree, tab),
                            Some(tab),
                            was_collapsed,
                            HeaderPlacement::Detached,
                        );
                        if was_collapsed ^ header_actions.toggle_collapse {
                            return;
                        }
                        // Même borne logique pour les quatre natures : leur
                        // contenu peut déborder, mais il ne doit modifier ni
                        // le Frame ni le rectangle de découpe du viewport.
                        show_window_contents_bounded(ui, |ui| {
                            let tree = &snapshot.tree;
                            match tab {
                                Tab::Book(id) => {
                                    if let Some(view_state) = model.view_state.as_mut() {
                                        book_contents(ui, view_state, &mut requests.actions, id);
                                    } else {
                                        ui.label(tr!("detached-workbook-unavailable"));
                                    }
                                }
                                Tab::Graph(id) => {
                                    let view =
                                        model.graph_view.get_or_insert_with(GraphView::default);
                                    let before = selected_manager_target(view);
                                    graph_view_contents(
                                        ui,
                                        view,
                                        &snapshot,
                                        id,
                                        &mut requests.actions,
                                    );
                                    if before != selected_manager_target(view) {
                                        root_context.request_repaint_of(egui::ViewportId::ROOT);
                                    }
                                }
                                Tab::Matrix(id) => {
                                    matrix_contents(ui, tree, id);
                                }
                                Tab::Note(id) => {
                                    if let Some(view_state) = model.view_state.as_mut() {
                                        note_contents(ui, view_state, &mut requests.actions, id);
                                    } else {
                                        ui.label(tr!("detached-note-unavailable"));
                                    }
                                }
                            }
                        });
                    });
                    // Les raccourcis de l'édition sont lus dans le contexte
                    // du viewport qui a le focus, comme pour la racine.
                    if matches!(tab, Tab::Book(_)) {
                        collect_undo_redo_shortcuts(ui.ctx(), &mut requests.actions);
                    }
                    if let Some(mut action) = finish_file_drop(ui.ctx()) {
                        // Même destination qu'un classeur flottant, y compris
                        // si sa grille est repliée et n'a pas réclamé le dépôt.
                        if let (Tab::Book(book), Action::DropFiles { target_book, .. }) =
                            (tab, &mut action)
                        {
                            *target_book = Some(book);
                        }
                        requests.actions.push(action);
                    }
                    if !requests.actions.is_empty() {
                        root_context.request_repaint_of(egui::ViewportId::ROOT);
                    }
                    // État de repli qui s'applique à partir de cette image
                    // (avant même l'affectation à `model.collapsed`
                    // ci-dessous) — calculé une fois, servant à la fois à
                    // la cible du `ViewportCommand::InnerSize`, à la garde
                    // des poignées de redimensionnement et à la géométrie
                    // persistée en fin de fermeture.
                    let collapsed_now = was_collapsed ^ header_actions.toggle_collapse;
                    // Contour actif (B7) : peint sur le rectangle du
                    // viewport OS réel (`input.content_rect()`, déjà utilisé
                    // par `detached_resize_handles` juste en dessous), pas
                    // sur celui qu'egui recalcule depuis le contenu — voir la
                    // doc du `Frame` ci-dessus. `LayerId::background()` :
                    // même couche que `CentralPanel`, peint après elle donc
                    // par-dessus ; un seul viewport détaché ici, pas d'autre
                    // fenêtre dont l'ordre serait à disputer.
                    if is_active {
                        let content_rect = ui.ctx().input(|input| input.content_rect());
                        paint_active_window_outline(
                            &ui.ctx().layer_painter(egui::LayerId::background()),
                            content_rect,
                            egui::CornerRadius::ZERO,
                            ui.style(),
                        );
                    }
                    if header_actions.toggle_collapse {
                        model.collapsed = collapsed_now;
                    }
                    // B9 : contrairement aux poignées maison des fenêtres
                    // flottantes internes (sans conséquence pendant un repli,
                    // puisqu'elles ne mutent qu'une taille purement logique),
                    // celles-ci pilotent le viewport OS réel via
                    // `ViewportCommand::BeginResize` — les laisser actives
                    // pendant un repli permettrait à l'utilisateur
                    // d'agrandir la fenêtre repliée en direct, une taille
                    // qui ne serait ni affichée (le contenu reste sauté) ni
                    // mémorisée (`expanded_size` est gelée pendant un repli)
                    // et donc perdue au premier dépli.
                    if !collapsed_now {
                        detached_resize_handles(ui);
                    }
                    // B9 : une fenêtre détachée est un viewport OS réel — à
                    // la différence d'une fenêtre flottante interne, où
                    // `.fixed_size(display_size)` (B3) suffit à réduire ce
                    // qui est demandé à egui, rien ne réduit ici la taille
                    // du viewport tant qu'on ne le demande pas explicitement
                    // à l'OS. Réutilise le même calcul que
                    // `floating_window_display_size` (même hauteur de
                    // bande d'en-tête) : au repli, la largeur mémorisée et
                    // la hauteur de l'en-tête ; au dépli, la taille
                    // mémorisée telle quelle. Envoyé uniquement à l'image du
                    // bascule (pas à chaque image : un `InnerSize` répété
                    // pendant qu'un utilisateur redimensionne la fenêtre à
                    // la main la ferait sauter en arrière).
                    if header_actions.toggle_collapse {
                        let expanded_size = model
                            .expanded_size
                            .unwrap_or_else(|| egui::vec2(size[0], size[1]));
                        let target = floating_window_display_size(
                            expanded_size,
                            collapsed_now,
                            WINDOW_HEADER_HEIGHT,
                            frame.inner_margin.sum(),
                        );
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::InnerSize(target));
                    }
                    if header_actions.title_drag_started {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if header_actions.maximize {
                        let is_maximized = ui
                            .ctx()
                            .input(|input| input.viewport().maximized)
                            .unwrap_or(false);
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                    }
                    if header_actions.detach {
                        requests.reattach = true;
                        root_context.request_repaint_of(egui::ViewportId::ROOT);
                    }
                    if header_actions.close {
                        requests.close = true;
                        root_context.request_repaint_of(egui::ViewportId::ROOT);
                    }
                    if std::mem::take(&mut model.needs_initial_repaint) {
                        // `show_viewport_deferred` crée la fenêtre après
                        // l'image racine. Sous Wayland, le premier callback
                        // peut donc voir une surface encore sans taille
                        // définitive ; redessiner une fois reconstruit la
                        // figure avec le rectangle configuré par WGPU.
                        ui.ctx().request_repaint();
                    }
                    if ui.ctx().input(|input| input.viewport().close_requested()) {
                        requests.close = true;
                        root_context.request_repaint_of(egui::ViewportId::ROOT);
                    }
                },
            );
            if changed {
                ctx.request_repaint_of(egui::ViewportId::from_hash_of(tab));
            }
        }
    }
}
