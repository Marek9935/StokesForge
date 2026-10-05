//! Détachement en fenêtres OS : les trois panneaux fixes et chaque fenêtre de
//! contenu peuvent vivre dans un viewport dédié (`show_viewport_deferred`).
//! La fermeture de ce callback est `'static` et `Send + Sync` : elle ne peut
//! pas capturer `&mut AppState`, d'où les vues locales (`DetachedTabViewModel`,
//! `ProjectManagerViewModel`) et les requêtes qu'elles renvoient à la racine.
//!
//! Déplacé d'`app.rs` (#101), puis découpé par phase (#124) : publication
//! de l'instantané, peinture et collecte d'événements, commandes de fenêtre.
//! Le payload du callback reste possédé et `'static` ; ses deux verrous
//! gardent la même durée, et les actions métier attendent toujours la racine.
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
//! endroit, puisqu'il est figé à gauche. Déclenchable depuis son en-tête
//! ou le menu « Fenêtre ». Sous Wayland, le compositeur peut ignorer une
//! position de fenêtre
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
//! [`tab_contents`](crate::app::contents::tab_contents) (la fonction déjà partagée par les fenêtres flottantes et
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
//! Ainsi [`book_contents`] et [`grid_contents`](crate::app::grid::grid_contents) sont appelées à l'identique
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
//! # OM et console (#100)
//!
//! Les panneaux détachés ont leur propre lecture et leurs requêtes, comme
//! le PM, mais ne deviennent pas des onglets de contenu. L'OM publie aussi
//! le graphe actif et sa sélection ; la console publie saisie/sortie et état
//! du travail. Le moteur Python reste à la racine. Les requêtes de saisie
//! sont drainées avant publication pour ne pas perdre les dernières frappes.
//! Tous utilisent `detached_panel_contents` : cadre maison sans décoration
//! native, Rattacher uniquement, fermeture OS convertie en rattachement.
//!

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use eframe::egui;
use stokesforge_core::NodeId;
use stokesforge_render_egui::GraphView;
use stokesforge_session::{Snapshot, WindowGeometry};

use crate::state::{Action, AppState, ProjectTreeNode};
use crate::tr;

use super::contents::{matrix_contents, note_contents};
use super::file_drop::{begin_file_drop, finish_file_drop};
use super::graph_window::graph_view_contents;
use super::object_manager_ui::{
    object_manager_contents, selected_manager_target, ObjectManagerRequest,
};
use super::project_manager::{
    project_manager_contents_from_tree, PmInteraction, PROJECT_MANAGER_WIDTH,
};
use super::script_console::{script_model_contents, ScriptConsoleModel};
use super::shortcuts::collect_undo_redo_shortcuts;
use super::tab::{tab_node, Tab};
use super::window_chrome::{
    detached_resize_handles, paint_active_window_outline, panel_header,
    show_window_contents_bounded, window_header, HeaderPlacement, PanelPlacement,
    WindowHeaderActions, MIN_CONTENT_WINDOW_SIZE, WINDOW_CONTENT_MARGIN, WINDOW_HEADER_HEIGHT,
};
use super::window_placement::{floating_window_display_size, GRAPH_WINDOW_SIZE};
use super::window_stack::toggle_collapse_to;
use super::workbook::book_contents;
use super::{snapshot_stamp, tab_title, tab_title_from_tree, StokesForgeApp};

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
    pub(super) root: Option<NodeId>,
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
/// `actions` locaux à [`StokesForgeApp::ui`](eframe::App::ui), juste posés par un callback différé
/// (potentiellement rappelé à une image où la fenêtre mère ne peint pas)
/// plutôt que collectés en direct pendant la peinture.
#[derive(Debug, Default)]
pub(super) struct DetachedProjectManagerRequests {
    /// Dépôts de fichiers, traités par la même session que la fenêtre principale.
    pub(super) actions: Vec<Action>,
    /// Onglets demandés par un clic dans l'arbre depuis la dernière
    /// vidange ; [`StokesForgeApp::ui`](eframe::App::ui) les transfère dans son propre
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

/// L'OM détaché reçoit la projection du graphe actif, même sans événement
/// dans son viewport. Les gestes restent associés au graphe qui était peint.
#[derive(Clone)]
pub(super) struct ObjectManagerModel {
    pub(super) stamp: (u64, u64, u64),
    pub(super) tree: Arc<stokesforge_core::Tree>,
    pub(super) graph: Option<NodeId>,
    pub(super) selection: Option<crate::object_manager::VisibilityTarget>,
    pub(super) model: Option<crate::object_manager::ObjectTree>,
    pub(super) spec: Option<stokesforge_plot::PlotSpec>,
    pub(super) show_texts: bool,
    pub(super) show_scales: bool,
}

#[derive(Default)]
struct ObjectManagerRequests {
    gestures: Vec<(NodeId, ObjectManagerRequest)>,
    actions: Vec<Action>,
    reattach: bool,
}

pub(super) struct DetachedObjectManager {
    model: Arc<Mutex<ObjectManagerModel>>,
    requests: Arc<Mutex<ObjectManagerRequests>>,
}

#[derive(Default)]
struct ScriptConsoleRequests {
    source: Option<String>,
    actions: Vec<Action>,
    reattach: bool,
}

pub(super) struct DetachedScriptConsole {
    model: Arc<Mutex<ScriptConsoleModel>>,
    requests: Arc<Mutex<ScriptConsoleRequests>>,
}

/// Les panneaux détachés ont le cadre maison et une seule action de fenêtre.
/// Une fermeture demandée par le compositeur devient un rattachement.
fn detached_panel_contents(
    ui: &mut egui::Ui,
    title: &str,
    contents: impl FnOnce(&mut egui::Ui),
) -> bool {
    let mut reattach = false;
    egui::CentralPanel::default()
        .frame(
            egui::Frame::central_panel(ui.style())
                .inner_margin(WINDOW_CONTENT_MARGIN)
                .stroke(ui.visuals().window_stroke()),
        )
        .show(ui, |ui| {
            let header = panel_header(ui, title, PanelPlacement::Detached, false);
            reattach = header.detach;
            if header.title_drag_started {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            show_window_contents_bounded(ui, contents);
        });
    if ui.ctx().input(|input| input.focused) {
        paint_active_window_outline(
            &ui.ctx().layer_painter(egui::LayerId::background()),
            ui.ctx().input(|input| input.content_rect()),
            egui::CornerRadius::ZERO,
            ui.style(),
        );
    }
    detached_resize_handles(ui);
    if ui.ctx().input(|input| input.viewport().close_requested()) {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::CancelClose);
        reattach = true;
    }
    reattach
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
    /// [`StokesForgeApp::ui`](eframe::App::ui).
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

/// Ce qu'une fenêtre détachée écrit, pour que [`StokesForgeApp::ui`](eframe::App::ui) (seule à
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
    pub(super) fn drain_object_manager(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let Some(view) = &self.om_view else {
            return;
        };
        let (gestures, reattach) = {
            let mut requests = view.requests.lock().unwrap_or_else(|err| err.into_inner());
            actions.append(&mut requests.actions);
            (
                std::mem::take(&mut requests.gestures),
                std::mem::take(&mut requests.reattach),
            )
        };
        for (graph, request) in gestures {
            self.run_object_manager_request(ctx, graph, request, actions);
        }
        if reattach {
            self.om_detached = false;
            // La lecture détachée ne doit plus retenir un ancien arbre entier.
            self.om_view = None;
        }
    }

    pub(super) fn show_detached_object_manager(
        &mut self,
        ctx: &egui::Context,
        next: ObjectManagerModel,
    ) {
        let view = self.om_view.get_or_insert_with(|| DetachedObjectManager {
            model: Arc::new(Mutex::new(next.clone())),
            requests: Arc::default(),
        });
        let changed = {
            let mut model = view.model.lock().unwrap_or_else(|err| err.into_inner());
            let changed = model.stamp != next.stamp
                || model.graph != next.graph
                || model.selection != next.selection
                || model.show_texts != next.show_texts
                || model.show_scales != next.show_scales;
            *model = next;
            changed
        };
        let model = Arc::clone(&view.model);
        let requests = Arc::clone(&view.requests);
        let root = ctx.clone();
        let id = egui::ViewportId::from_hash_of("stokesforge_object_manager_window");
        ctx.show_viewport_deferred(
            id,
            egui::ViewportBuilder::default()
                .with_title(tr!("action-object-manager"))
                .with_decorations(false)
                .with_inner_size([330.0, 480.0])
                .with_min_inner_size([220.0, 140.0]),
            move |ui, _class| {
                let model = model.lock().unwrap_or_else(|err| err.into_inner()).clone();
                let mut actions = Vec::new();
                let mut gestures = Vec::new();
                let reattach = detached_panel_contents(ui, &tr!("action-object-manager"), |ui| {
                    gestures = object_manager_contents(
                        ui,
                        &model.tree,
                        model.model.as_ref(),
                        model.spec.as_ref(),
                        model.show_texts,
                        model.show_scales,
                        &mut actions,
                    );
                });
                if reattach || !actions.is_empty() || !gestures.is_empty() {
                    let mut requests = requests.lock().unwrap_or_else(|err| err.into_inner());
                    requests.reattach |= reattach;
                    requests.actions.append(&mut actions);
                    if let Some(graph) = model.graph {
                        requests
                            .gestures
                            .extend(gestures.into_iter().map(|gesture| (graph, gesture)));
                    }
                    root.request_repaint_of(egui::ViewportId::ROOT);
                }
            },
        );
        if changed {
            ctx.request_repaint_of(id);
        }
    }

    pub(super) fn detach_script_console(&mut self) {
        self.script_detached = true;
        // Le travail Python ne migre jamais ; ce lecteur ne possède que la saisie et la sortie.
        self.script_view = Some(DetachedScriptConsole {
            model: Arc::new(Mutex::new(ScriptConsoleModel::from_state(&self.state))),
            requests: Arc::default(),
        });
    }

    pub(super) fn drain_script_console(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let Some(view) = &self.script_view else {
            return;
        };
        let (source, reattach) = {
            let mut requests = view.requests.lock().unwrap_or_else(|err| err.into_inner());
            actions.append(&mut requests.actions);
            (
                requests.source.take(),
                std::mem::take(&mut requests.reattach),
            )
        };
        if let Some(source) = source {
            self.state.set_script_source(source);
        }
        if reattach {
            self.script_detached = false;
            self.script_view = None;
            ctx.request_repaint();
        }
    }

    pub(super) fn show_detached_script_console(&mut self, ctx: &egui::Context) {
        let Some(view) = &self.script_view else {
            return;
        };
        let next = ScriptConsoleModel::from_state(&self.state);
        let changed = {
            let mut model = view.model.lock().unwrap_or_else(|err| err.into_inner());
            let changed = *model != next;
            // Une saisie arrivée après le drainage ne doit pas être écrasée
            // par une publication de l'ancien tampon dans cette même image.
            let requests = view.requests.lock().unwrap_or_else(|err| err.into_inner());
            if requests.source.is_none() {
                model.source = next.source;
            }
            model.output = next.output;
            model.running = next.running;
            model.available = next.available;
            changed
        };
        let model = Arc::clone(&view.model);
        let requests = Arc::clone(&view.requests);
        let root = ctx.clone();
        let id = egui::ViewportId::from_hash_of("stokesforge_script_window");
        ctx.show_viewport_deferred(
            id,
            egui::ViewportBuilder::default()
                .with_title(tr!("script-title"))
                .with_decorations(false)
                .with_inner_size([700.0, 440.0])
                .with_min_inner_size([300.0, 220.0]),
            move |ui, _class| {
                let mut model = model.lock().unwrap_or_else(|err| err.into_inner());
                let source = model.source.clone();
                let mut actions = Vec::new();
                let ctx = ui.ctx().clone();
                if ctx.input_mut(|input| {
                    input.consume_shortcut(&super::shortcuts::SHORTCUT_RUN_SCRIPT)
                }) {
                    actions.push(Action::RunScript);
                }
                let reattach = detached_panel_contents(ui, &tr!("script-title"), |ui| {
                    if !model.available {
                        ui.weak(tr!("script-unavailable"));
                    }
                    script_model_contents(ui, &ctx, &mut model, &mut actions);
                });
                if source != model.source || reattach || !actions.is_empty() {
                    let mut requests = requests.lock().unwrap_or_else(|err| err.into_inner());
                    if source != model.source {
                        requests.source = Some(model.source.clone());
                    }
                    requests.reattach |= reattach;
                    requests.actions.append(&mut actions);
                    root.request_repaint_of(egui::ViewportId::ROOT);
                }
            },
        );
        if changed {
            ctx.request_repaint_of(id);
        }
    }

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
    ///   [`StokesForgeApp::ui`](eframe::App::ui) la draine avant la publication pour produire les
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
            model.root = Some(self.state.tree().root());
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
                .with_inner_size([PROJECT_MANAGER_WIDTH + 40.0, 480.0])
                .with_decorations(false)
                .with_min_inner_size([220.0, 140.0]),
            move |ui, _class| {
                begin_file_drop(ui.ctx());
                let mut opened = Vec::new();
                let mut closed = Vec::new();
                let mut actions = Vec::new();
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
                let selected_before = model.selected.clone();
                let hidden_before = model.hidden.clone();
                let reattach = detached_panel_contents(ui, &tr!("project-manager-title"), |ui| {
                    let mut interaction = PmInteraction {
                        actions: &mut actions,
                        root: model.root.unwrap_or(NodeId::from_raw(1)),
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
                    if reattach
                        || !opened.is_empty()
                        || !closed.is_empty()
                        || model.selected != selected_before
                        || model.hidden != hidden_before
                    {
                        root_context.request_repaint_of(egui::ViewportId::ROOT);
                    }
                    requests.reattach |= reattach;
                    if !actions.is_empty() {
                        root_context.request_repaint_of(egui::ViewportId::ROOT);
                    }
                    requests.actions.append(&mut actions);
                    requests.open.append(&mut opened);
                    requests.close.append(&mut closed);
                    if model.selected != selected_before {
                        requests.selection = Some(model.selected);
                    }
                    if model.hidden != hidden_before {
                        requests.hidden = Some(model.hidden);
                    }
                    if let Some(action) = finish_file_drop(ui.ctx()) {
                        requests.actions.push(action);
                        root_context.request_repaint_of(egui::ViewportId::ROOT);
                    }
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
    /// [`StokesForgeApp::detach_project_manager`] et
    /// [`StokesForgeApp::detach_script_console`] pour leurs chemins propres.
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
    /// Ne draine **pas** les requêtes produites : [`StokesForgeApp::ui`](eframe::App::ui) le fait
    /// juste après, une fois cette fonction revenue, exactement comme pour
    /// `pm_requests`.
    pub(super) fn show_detached_tabs(&mut self, ctx: &egui::Context) {
        let Self {
            state,
            detached_tabs,
            ..
        } = self;
        for (tab, detached) in detached_tabs.iter() {
            let changed = detached.publish_snapshot(state);
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
            let payload = DetachedTabViewport {
                tab,
                id,
                size,
                view_model,
                requests,
                root_context,
            };
            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of(tab),
                viewport,
                move |ui, _class| payload.show(ui),
            );
            if changed {
                ctx.request_repaint_of(egui::ViewportId::from_hash_of(tab));
            }
        }
    }
}

/// Données possédées du callback différé : aucun emprunt à l'application ne
/// survit à la publication du viewport. Les phases ne créent aucun sous-Ui.
struct DetachedTabViewport {
    tab: Tab,
    id: egui::Id,
    size: [f32; 2],
    view_model: Arc<Mutex<DetachedTabViewModel>>,
    requests: Arc<Mutex<DetachedTabRequests>>,
    root_context: egui::Context,
}

/// État de l'image lu avant les clics, puis actions de l'en-tête peint.
struct DetachedTabFrame {
    was_collapsed: bool,
    is_active: bool,
    header_actions: WindowHeaderActions,
    inner_margin: egui::Margin,
}

impl DetachedTab {
    /// Publie d'abord l'instantané et la vue locale ; le réveil reste après
    /// l'enregistrement du callback, dans `show_detached_tabs`.
    fn publish_snapshot(&self, state: &AppState) -> bool {
        let snapshot = state.snapshot();
        let mut model = self
            .view_model
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let changed = model.snapshot.as_ref().map(snapshot_stamp) != Some(snapshot_stamp(snapshot))
            || model
                .view_state
                .as_ref()
                .is_some_and(|view| view.renaming_sheet() != state.renaming_sheet());
        model.snapshot = Some(snapshot.clone());
        if let Some(view_state) = model.view_state.as_mut() {
            view_state.sync_detached_snapshot(snapshot.clone());
            view_state.sync_detached_sheet_editor(state);
        }
        changed
    }
}

impl DetachedTabViewport {
    /// Les deux verrous restent tenus pendant toute l'image, comme avant le
    /// découpage : aucun drainage racine ne s'intercale entre ses phases.
    fn show(&self, ui: &mut egui::Ui) {
        let root_context = &self.root_context;
        begin_file_drop(ui.ctx());
        let mut model = self
            .view_model
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let mut requests = self.requests.lock().unwrap_or_else(|err| err.into_inner());
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
        let (header_actions, frame) =
            self.paint_contents(ui, &mut model, &mut requests, was_collapsed);
        self.collect_actions(ui, &mut requests);
        self.apply_window_actions(
            ui,
            &mut model,
            &mut requests,
            DetachedTabFrame {
                was_collapsed,
                is_active,
                header_actions,
                inner_margin: frame.inner_margin,
            },
        );
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
    }

    fn paint_contents(
        &self,
        ui: &mut egui::Ui,
        model: &mut DetachedTabViewModel,
        requests: &mut DetachedTabRequests,
        was_collapsed: bool,
    ) -> (WindowHeaderActions, egui::Frame) {
        let tab = self.tab;
        let id = self.id;
        let root_context = &self.root_context;
        let mut header_actions = WindowHeaderActions::default();
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
                        let view = model.graph_view.get_or_insert_with(GraphView::default);
                        let before = selected_manager_target(view);
                        graph_view_contents(ui, view, &snapshot, id, &mut requests.actions);
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
        (header_actions, frame)
    }

    fn collect_actions(&self, ui: &mut egui::Ui, requests: &mut DetachedTabRequests) {
        let tab = self.tab;
        let root_context = &self.root_context;
        // Les raccourcis de l'édition sont lus dans le contexte
        // du viewport qui a le focus, comme pour la racine.
        if matches!(tab, Tab::Book(_)) {
            collect_undo_redo_shortcuts(ui.ctx(), &mut requests.actions);
        }
        if let Some(mut action) = finish_file_drop(ui.ctx()) {
            // Même destination qu'un classeur flottant, y compris
            // si sa grille est repliée et n'a pas réclamé le dépôt.
            if let (Tab::Book(book), Action::DropFiles { target_book, .. }) = (tab, &mut action) {
                *target_book = Some(book);
            }
            requests.actions.push(action);
        }
        if !requests.actions.is_empty() {
            root_context.request_repaint_of(egui::ViewportId::ROOT);
        }
    }

    fn apply_window_actions(
        &self,
        ui: &mut egui::Ui,
        model: &mut DetachedTabViewModel,
        requests: &mut DetachedTabRequests,
        frame: DetachedTabFrame,
    ) {
        let DetachedTabFrame {
            was_collapsed,
            is_active,
            header_actions,
            inner_margin,
        } = frame;
        let size = self.size;
        let root_context = &self.root_context;
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
                inner_margin.sum(),
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
    }
}

#[cfg(test)]
mod tests {
    //! Le détachement en fenêtres OS, piloté par les callbacks de viewport
    //! qu'egui publie : la publication de l'instantané qui réveille les
    //! lecteurs (#104), l'état des fenêtres envoyé à la session (#16), les
    //! dépôts de fichiers isolés de la racine, la figure reconstruite dans le
    //! viewport, le rognage (B8), le repli du viewport OS (B9) et le contour
    //! de focus (B7). Déplacés d'`app.rs` avec leur code (#123).
    use std::time::Duration;

    use super::super::graph_window::graph_drop_target;
    use super::super::graph_window::tests::graphe;
    use super::super::grid::role_label;
    use super::super::project_manager::tests::find_text_center;
    use super::super::window_chrome::tests::find_icon_centers;
    use super::*;
    use crate::theme;
    use stokesforge_core::{Column, ColumnRole, Project};
    use stokesforge_render_egui::DEFAULT_FIGURE_SIZE;
    use stokesforge_session::Session;

    fn vue_persistee(session: &Session, node: NodeId) -> WindowGeometry {
        *session
            .handle()
            .snapshot()
            .unwrap()
            .window_geometries
            .get(&node)
            .expect("géométrie envoyée à la session")
    }

    /// #124 : le payload possédé d'une note conserve sa saisie locale ; la
    /// perte de focus produit une requête, puis seule la racine l'applique.
    /// La fermeture OS est elle aussi une requête, sans réattachement.
    #[test]
    fn une_note_detachee_differe_son_edition_et_sa_fermeture() {
        let mut project = Project::new("Note détachée").unwrap();
        let note = project
            .add_note(project.root(), Some("Journal"), "Début")
            .unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let tab = Tab::Note(note);
        app.open_tab(tab);
        app.detach_tab(tab);
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        context.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&context);
        egui_extras::install_image_loaders(&context);
        let root_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let viewport_id = egui::ViewportId::from_hash_of(tab);
        let mut output = context.run_ui(root_input.clone(), |ui| app.show_detached_tabs(ui.ctx()));
        let callback = output.viewport_output[&viewport_id]
            .viewport_ui_cb
            .clone()
            .unwrap();
        output.textures_delta.clear();
        let paint = |events: Vec<egui::Event>, close: bool| {
            let mut input = root_input.clone();
            input.viewport_id = viewport_id;
            input.viewports.insert(
                viewport_id,
                egui::ViewportInfo {
                    parent: Some(egui::ViewportId::ROOT),
                    events: if close {
                        vec![egui::ViewportEvent::Close]
                    } else {
                        Vec::new()
                    },
                    ..Default::default()
                },
            );
            input.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(420.0, 360.0),
            ));
            input.events = events;
            let mut output = context.run_ui(input, |ui| callback(ui));
            output.textures_delta.clear();
            output
        };
        paint(Vec::new(), false);
        let first = paint(Vec::new(), false);
        let dedans = find_text_center(&first.shapes, "Début") + egui::vec2(80.0, 0.0);
        let pointer = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        paint(
            vec![egui::Event::PointerMoved(dedans), pointer(dedans, true)],
            false,
        );
        paint(vec![pointer(dedans, false)], false);
        paint(vec![egui::Event::Text(" suite".into())], false);
        {
            let model = app.detached_tabs[&tab].view_model.lock().unwrap();
            assert_eq!(
                model
                    .view_state
                    .as_ref()
                    .unwrap()
                    .note_editing()
                    .unwrap()
                    .buffer,
                "Début suite"
            );
        }
        assert!(app.detached_tabs[&tab]
            .requests
            .lock()
            .unwrap()
            .actions
            .is_empty());
        assert_eq!(app.state.note_text(note).as_deref(), Some("Début"));

        let dehors = egui::pos2(200.0, 15.0);
        paint(
            vec![egui::Event::PointerMoved(dehors), pointer(dehors, true)],
            false,
        );
        paint(vec![pointer(dehors, false)], false);
        let actions = std::mem::take(&mut app.detached_tabs[&tab].requests.lock().unwrap().actions);
        assert_eq!(
            actions,
            vec![Action::SetNoteText {
                note,
                text: "Début suite".into()
            }]
        );
        assert_eq!(
            app.state.note_text(note).as_deref(),
            Some("Début"),
            "la requête attend la racine"
        );
        for action in actions {
            app.state.dispatch(action);
        }
        assert_eq!(app.state.note_text(note).as_deref(), Some("Début suite"));
        app.state.dispatch(Action::Undo);
        assert_eq!(app.state.note_text(note).as_deref(), Some("Début"));

        assert!(!app.detached_tabs[&tab].requests.lock().unwrap().close);
        paint(Vec::new(), true);
        let requests = app.detached_tabs[&tab].requests.lock().unwrap();
        assert!(requests.close, "la croix OS doit revenir à la racine");
        assert!(!requests.reattach, "la croix ferme la vue détachée");
        assert!(
            app.detached_tabs.contains_key(&tab),
            "la fermeture attend aussi son drainage"
        );
    }

    /// #104 : aucune entrée de souris dans les viewports lecteurs ; la
    /// publication d'un nouvel instantané suffit à demander leur redessin.
    #[test]
    fn une_mutation_reveille_les_fenetres_detachees_au_repos() {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book = app.state.books()[0].0;
        let sheet = app.state.selected_sheet_in(book).unwrap();
        let column = app.state.visible_columns(sheet)[0].0;
        app.detach_tab(Tab::Book(book));
        app.detach_tab(Tab::Graph(graph));
        app.detach_project_manager();
        let ids = [
            egui::ViewportId::from_hash_of(Tab::Book(book)),
            egui::ViewportId::from_hash_of(Tab::Graph(graph)),
            egui::ViewportId::from_hash_of("stokesforge_project_manager_window"),
        ];
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let paint = |app: &mut StokesForgeApp| {
            let mut output = ctx.run_ui(input.clone(), |ui| {
                app.show_detached_tabs(ui.ctx());
                app.show_detached_project_manager(ui.ctx(), None);
            });
            output.textures_delta.clear();
            output
        };
        let settle = |app: &mut StokesForgeApp| {
            for _ in 0..8 {
                let output = paint(app);
                for id in ids {
                    let callback = output.viewport_output[&id].viewport_ui_cb.as_ref().unwrap();
                    let mut child = input.clone();
                    child.viewport_id = id;
                    child.viewports.insert(
                        id,
                        egui::ViewportInfo {
                            parent: Some(egui::ViewportId::ROOT),
                            ..Default::default()
                        },
                    );
                    ctx.run_ui(child, |ui| callback(ui)).textures_delta.clear();
                }
            }
            let output = paint(app);
            for id in ids {
                assert_eq!(
                    output.viewport_output[&id].repaint_delay,
                    Duration::MAX,
                    "le viewport {id:?} doit dormir au repos"
                );
            }
        };
        settle(&mut app);
        app.state.dispatch(Action::CommitCell {
            column,
            row: 0,
            text: "42".into(),
        });
        let output = paint(&mut app);
        for id in ids {
            assert_eq!(
                output.viewport_output[&id].repaint_delay,
                Duration::ZERO,
                "la mutation doit réveiller {id:?}"
            );
        }
        settle(&mut app);
        app.state.set_window_geometry(
            graph,
            WindowGeometry {
                x: 10.0,
                y: 20.0,
                width: 500.0,
                height: 400.0,
                view: None,
            },
        );
        app.state.refresh();
        let output = paint(&mut app);
        for id in ids {
            assert_eq!(
                output.viewport_output[&id].repaint_delay,
                Duration::ZERO,
                "la géométrie doit réveiller {id:?}"
            );
        }
        settle(&mut app);
        app.pm_selection.insert(Tab::Book(book));
        let output = paint(&mut app);
        assert_eq!(
            output.viewport_output[&ids[2]].repaint_delay,
            Duration::ZERO,
            "une sélection de navigation réveille le Project Manager"
        );
        for id in &ids[..2] {
            assert_eq!(output.viewport_output[id].repaint_delay, Duration::MAX);
        }
        settle(&mut app);
        session.shutdown().unwrap();
    }

    /// Issue #16 : l'état de chaque fenêtre (ouverte/maximisée/fermée, repliée,
    /// rang d'empilement) part à la session avec sa géométrie flottante, et
    /// cette dernière ne bouge ni à la maximisation, ni à la fermeture, ni au
    /// détachement.
    #[test]
    fn l_etat_des_fenetres_est_envoye_a_la_session_sans_toucher_a_la_geometrie_flottante() {
        use stokesforge_session::WindowMode;
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book = app.state.books()[0].0;
        let rect = egui::Rect::from_min_size(egui::pos2(40.0, 60.0), egui::vec2(500.0, 300.0));
        app.remember_window_geometry(Tab::Graph(graph), rect);
        app.remember_window_geometry(Tab::Book(book), rect.translate(egui::vec2(9.0, 9.0)));
        app.sync_window_views();

        let open = vue_persistee(&session, graph);
        assert_eq!(
            (open.x, open.y, open.width, open.height),
            (40.0, 60.0, 500.0, 300.0)
        );
        let view = open.view.unwrap();
        assert_eq!(view.mode, WindowMode::Open);
        assert!(!view.collapsed);
        // Le graphe est ouvert après le classeur : il est au-dessus.
        assert!(view.stack > vue_persistee(&session, book).view.unwrap().stack);

        // Replié + maximisé : état retenu, rectangle inchangé.
        app.collapsed_tabs.insert(Tab::Graph(graph));
        app.maximized_tab = Some(Tab::Graph(graph));
        app.sync_window_views();
        let maximized = vue_persistee(&session, graph);
        assert_eq!(maximized.view.unwrap().mode, WindowMode::Maximized);
        assert!(maximized.view.unwrap().collapsed);
        assert_eq!((maximized.x, maximized.width), (open.x, open.width));

        // Détachée : toujours « ouverte », rectangle flottant intact.
        app.maximized_tab = None;
        app.detach_tab(Tab::Graph(graph));
        app.sync_window_views();
        let detached = vue_persistee(&session, graph);
        assert_eq!(detached.view.unwrap().mode, WindowMode::Open);
        assert_eq!((detached.x, detached.y), (open.x, open.y));

        // Fermée : la géométrie flottante reste celle d'avant.
        app.detached_tabs.clear();
        app.sync_window_views();
        let closed = vue_persistee(&session, graph);
        assert_eq!(closed.view.unwrap().mode, WindowMode::Closed);
        assert_eq!(
            (closed.x, closed.y, closed.width, closed.height),
            (40.0, 60.0, 500.0, 300.0)
        );
        // Rouverte : c'est ce rectangle-là que la boucle de peinture relit.
        assert_eq!(app.floating_rects[&graph], rect);
    }

    /// Les callbacks détachés émettent la même action et ne consomment pas
    /// le dépôt en attente dans le viewport racine (mémoire egui partagée).
    #[test]
    fn les_depots_detaches_gardent_leur_destination_et_sont_isoles_de_la_racine() {
        #[derive(Debug)]
        struct FakeFile(std::path::PathBuf);
        impl egui::DroppedFile for FakeFile {
            fn path(&self) -> &std::path::Path {
                &self.0
            }
            fn bytes(&self) -> Result<Vec<u8>, String> {
                Ok(Vec::new())
            }
        }
        for kind in ["classeur", "graphe", "explorateur"] {
            let (session, graph) = graphe();
            let state = AppState::new(session.handle(), "test.stokesforge".into()).unwrap();
            let mut app = StokesForgeApp::new(state);
            let book = app.state.books()[0].0;
            let tab = if kind == "classeur" {
                Tab::Book(book)
            } else {
                Tab::Graph(graph)
            };
            let viewport = if kind == "explorateur" {
                app.detach_project_manager();
                egui::ViewportId::from_hash_of("stokesforge_project_manager_window")
            } else {
                app.detach_tab(tab);
                egui::ViewportId::from_hash_of(tab)
            };
            let context = egui::Context::default();
            context.set_embed_viewports(false);
            crate::theme::install(&context);
            let mut root_input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 600.),
                )),
                ..Default::default()
            };
            root_input.dropped_files = vec![Arc::new(FakeFile("racine.csv".into()))];
            let mut output = context.run_ui(root_input.clone(), |ui| {
                begin_file_drop(ui.ctx());
                if kind == "explorateur" {
                    app.show_detached_project_manager(ui.ctx(), None);
                } else {
                    app.show_detached_tabs(ui.ctx());
                }
            });
            let callback = output.viewport_output[&viewport]
                .viewport_ui_cb
                .clone()
                .unwrap();
            output.textures_delta.clear();
            let mut detached_input = root_input.clone();
            detached_input.viewport_id = viewport;
            detached_input.viewports.insert(
                viewport,
                egui::ViewportInfo {
                    parent: Some(egui::ViewportId::ROOT),
                    ..Default::default()
                },
            );
            detached_input.dropped_files = vec![Arc::new(FakeFile("detache.csv".into()))];
            if kind == "classeur" {
                // #124 : dans une même image, le raccourci rejoint la file
                // avant le dépôt. L'ordre est observable au drainage racine.
                detached_input.events.push(egui::Event::Key {
                    key: egui::Key::Z,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND.plus(egui::Modifiers::CTRL),
                });
            }
            context
                .run_ui(detached_input, |ui| callback(ui))
                .textures_delta
                .clear();
            let actions = if kind == "explorateur" {
                std::mem::take(&mut app.pm_requests.lock().unwrap().actions)
            } else {
                std::mem::take(&mut app.detached_tabs[&tab].requests.lock().unwrap().actions)
            };
            let target = (kind == "classeur").then_some(book);
            let mut attendues = Vec::new();
            if kind == "classeur" {
                attendues.push(Action::Undo);
            }
            attendues.push(Action::DropFiles {
                paths: vec!["detache.csv".into()],
                target_book: target,
            });
            assert_eq!(actions, attendues);
            let mut root_action = None;
            root_input.dropped_files.clear();
            context
                .run_ui(root_input, |ui| {
                    root_action = finish_file_drop(ui.ctx());
                })
                .textures_delta
                .clear();
            assert_eq!(
                root_action,
                Some(Action::DropFiles {
                    paths: vec!["racine.csv".into()],
                    target_book: None
                })
            );
        }
    }

    #[test]
    fn un_graphe_detache_reconstruit_ses_formes_dans_son_viewport() {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        context.set_fonts(stokesforge_render_egui::font_definitions());
        assert!(app.floating_tabs.contains(&Tab::Graph(graph)));

        // Peindre d'abord la vue flottante : le détachement doit transférer
        // une `GraphView` déjà remplie, puis la reconstruire dans les
        // coordonnées du viewport OS distinct.
        let mut root_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 700.0),
            )),
            ..Default::default()
        };
        let mut output = context.run_ui(root_input.clone(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                graph_drop_target(ui, &app.state, &mut Vec::new(), &mut app.graph_views, graph);
            });
        });
        output.textures_delta.clear();
        assert!(
            !app.graph_views[&graph].cache().shapes().is_empty(),
            "la vue flottante de départ doit avoir construit sa figure"
        );

        app.detach_tab(Tab::Graph(graph));
        let viewport_id = egui::ViewportId::from_hash_of(Tab::Graph(graph));
        output = context.run_ui(root_input.clone(), |ui| {
            app.show_detached_tabs(ui.ctx());
        });
        let callback = output
            .viewport_output
            .get(&viewport_id)
            .and_then(|viewport| viewport.viewport_ui_cb.clone());
        output.textures_delta.clear();
        let callback = callback.expect("le détachement doit publier un callback de viewport");

        root_input.viewport_id = viewport_id;
        root_input.viewports.insert(
            viewport_id,
            egui::ViewportInfo {
                parent: Some(egui::ViewportId::ROOT),
                ..Default::default()
            },
        );
        root_input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            GRAPH_WINDOW_SIZE,
        ));
        output = context.run_ui(root_input, |ui| callback(ui));
        output.textures_delta.clear();
        assert_eq!(
            output.viewport_output[&viewport_id].repaint_delay,
            Duration::ZERO,
            "le premier callback doit programmer l'image de stabilisation du viewport"
        );

        let model = app.detached_tabs[&Tab::Graph(graph)]
            .view_model
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        assert!(
            !model.needs_initial_repaint,
            "le premier callback doit demander une unique image de stabilisation"
        );
        let view = model.graph_view.as_ref().unwrap();
        assert!(
            !view.cache().shapes().is_empty(),
            "le viewport détaché ne doit pas servir le cache vidé du viewport racine"
        );
        let display = view.display_rect().unwrap();
        let transform = egui::emath::TSTransform::new(
            display.min.to_vec2(),
            display.width() / DEFAULT_FIGURE_SIZE.0 as f32,
        );
        assert!(
            output
                .shapes
                .iter()
                .any(|painted| view.cache().shapes().iter().any(|shape| {
                    let mut expected = shape.clone();
                    expected.transform(transform);
                    painted.shape == expected
                })),
            "le viewport détaché doit réellement recevoir les formes du graphe"
        );
        // Régression : le bouton « ↩ » de réattachement, non borné en
        // hauteur, pouvait consommer toute la fenêtre détachée (voir la
        // doc de tête de la ligne qui l'alloue) — la figure se retrouvait
        // alors hors du rectangle visible, invisible bien que son cache
        // fût rempli (ce que les deux assertions ci-dessus ne détectent
        // pas : un `Shape` reste dans la liste même hors clip).
        let figure_rect = display;
        assert!(
            figure_rect.max.y <= GRAPH_WINDOW_SIZE.y,
            "la figure doit rester dans la fenêtre détachée, pas {figure_rect:?} pour une fenêtre de {GRAPH_WINDOW_SIZE:?}"
        );
    }

    // B8 (`retours-2026-09-23-bugs.md`) : même mécanisme que
    // B4 (`le_contenu_qui_deborde_est_rogne_au_rectangle_de_la_fenetre`),
    // jamais porté à `show_detached_tabs` — un classeur détaché à plusieurs
    // colonnes déborde du viewport OS sans rognage contrôlé, tronqué au pixel
    // par la fenêtre OS elle-même plutôt que par le code applicatif.
    // Scaffolding calqué sur `un_graphe_detache_reconstruit_ses_formes_dans_
    // son_viewport` : détacher pour de vrai, peindre le callback de viewport
    // publié par `show_detached_tabs` dans un `screen_rect` volontairement
    // plus petit que le contenu, puis vérifier le `clip_rect` réellement
    // envoyé à egui pour chaque en-tête de colonne peint.
    #[test]
    fn le_classeur_detache_est_rogne_au_rectangle_du_viewport() {
        let mut project = Project::new("Débordement").unwrap();
        let root = project.root();
        let book = project.add_empty_workbook(root, None).unwrap();
        let sheet = project.add_worksheet(book, None).unwrap();
        for i in 0..10 {
            project
                .add_column(
                    sheet,
                    None,
                    Some(&format!("Colonne {i}")),
                    Column::from_f64(&[0.0, 1.0]),
                )
                .unwrap();
        }
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        context.set_fonts(stokesforge_render_egui::font_definitions());

        let book_tab = Tab::Book(book);
        assert!(
            app.floating_tabs.contains(&book_tab),
            "StokesForgeApp::new doit ouvrir le premier classeur en fenêtre flottante"
        );
        app.detach_tab(book_tab);

        let viewport_id = egui::ViewportId::from_hash_of(book_tab);
        let root_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 700.0),
            )),
            ..Default::default()
        };
        let mut output = context.run_ui(root_input.clone(), |ui| {
            app.show_detached_tabs(ui.ctx());
        });
        let callback = output
            .viewport_output
            .get(&viewport_id)
            .and_then(|viewport| viewport.viewport_ui_cb.clone())
            .expect("le détachement doit publier un callback de viewport");
        output.textures_delta.clear();

        // Fenêtre OS volontairement plus étroite que les dix colonnes
        // nommées et leurs en-têtes — comme le rétrécissement natif de la
        // reproduction du rapport de bug.
        let window_size = egui::vec2(200.0, 150.0);
        let window_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, window_size);
        let mut viewport_input = root_input;
        viewport_input.viewport_id = viewport_id;
        viewport_input.viewports.insert(
            viewport_id,
            egui::ViewportInfo {
                parent: Some(egui::ViewportId::ROOT),
                ..Default::default()
            },
        );
        viewport_input.screen_rect = Some(window_rect);
        let mut output = context.run_ui(viewport_input, |ui| callback(ui));
        output.textures_delta.clear();

        // La partie sur laquelle egui n'offre aucune garantie propre, c'est
        // le rectangle de rognage *logique* : le backend borne de toute façon
        // le tampon au viewport OS, donc même sans le correctif, aucune forme
        // n'a un `clip_rect` qui déborde `window_rect`. La régression que B8
        // corrige est plus fine mais tout aussi réelle : sans
        // `ui.set_clip_rect(...)` posé sur le `Ui` du `CentralPanel`, tout le
        // contenu (en-têtes de colonne, grille, onglets) hérite du rognage
        // *par défaut* du panneau — le rectangle plein du viewport, marge du
        // `Frame` comprise — au lieu du rectangle de contenu réellement
        // disponible une fois cette marge retirée. Un en-tête tout au bord
        // droit se retrouve alors rogné pile au bord du viewport plutôt qu'au
        // bord du panneau, ce qui le fait empiéter sur la bordure/marge du
        // `Frame` — la même famille de symptôme que la capture du rapport de
        // bug (troncature qui ignore la géométrie du contenu). Ce test
        // verrouille donc : le rognage logique de tout contenu doit être
        // strictement plus petit que `window_rect`, pas égal à lui.
        //
        // Ce qu'il ne prouve pas (relu par mutation, #123) : le classeur pose
        // aujourd'hui son propre rognage (zone défilable de la grille, bande
        // d'onglets), si bien que retirer celui de
        // `show_window_contents_bounded` ne change rien ici. La borne commune
        // a son témoin à part, sans contenu qui se rogne lui-même :
        // `le_contenu_borne_est_rogne_sous_l_en_tete_et_ne_repousse_pas_le_cadre`
        // (`window_chrome.rs`).
        let letter_label = format!("A({})", role_label(ColumnRole::Y));
        let content_labels = [
            "#".to_string(),
            tr!("column-long-name"),
            tr!("column-units"),
            letter_label.clone(),
            "Sheet1".to_string(),
        ];
        let mut checked = 0;
        for painted in &output.shapes {
            let egui::Shape::Text(shape) = &painted.shape else {
                continue;
            };
            if !content_labels.contains(&shape.galley.job.text) {
                continue;
            }
            checked += 1;
            assert!(
                window_rect.contains_rect(painted.clip_rect),
                "« {} » a un rectangle de découpe ({:?}) qui déborde du viewport \
                 ({window_rect:?}) — B8 n'est pas corrigé",
                shape.galley.job.text,
                painted.clip_rect,
            );
            assert!(
                painted.clip_rect.width() < window_rect.width()
                    && painted.clip_rect.height() < window_rect.height(),
                "« {} » a un rectangle de découpe ({:?}) égal au viewport complet \
                 ({window_rect:?}) — rien ne distingue ce rognage de son absence : \
                 B8 n'est pas corrigé",
                shape.galley.job.text,
                painted.clip_rect,
            );
        }
        assert_eq!(
            checked,
            content_labels.len(),
            "le classeur détaché doit avoir peint tous les libellés attendus \
             (en-tête empilé et onglet de feuille) : {:?}",
            output
                .shapes
                .iter()
                .filter_map(|painted| match &painted.shape {
                    egui::Shape::Text(shape) => Some(shape.galley.job.text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        );
        let position_of = |label: &str| {
            output
                .shapes
                .iter()
                .find_map(|painted| match &painted.shape {
                    egui::Shape::Text(shape) if shape.galley.job.text == label => {
                        let center = shape.pos + shape.galley.rect.center().to_vec2();
                        painted.clip_rect.contains(center).then_some(center)
                    }
                    _ => None,
                })
        };
        let tab_y = position_of("Sheet1")
            .expect("l'onglet détaché doit être visible")
            .y;
        let header_y = position_of(&letter_label)
            .expect("l'en-tête détaché doit être visible")
            .y;
        assert!(
            tab_y > header_y,
            "l'onglet doit rester sous la grille détachée"
        );
    }

    // B9 (`retours-2026-09-23-bugs.md`) : le correctif B3
    // réduit la taille demandée à egui pour une fenêtre flottante interne
    // pendant un repli, mais une fenêtre détachée est un viewport OS réel —
    // rien n'envoyait de `ViewportCommand::InnerSize` quand son chevron de
    // repli bascule, donc l'OS gardait la grande taille dépliée. Ce test
    // pilote le callback du viewport détaché (même schéma que
    // `un_graphe_detache_reconstruit_ses_formes_dans_son_viewport`), clique
    // le chevron de repli, et vérifie la géométrie/l'état — pas
    // l'apparence : les tests de rendu ne couvrent pas ce mécanisme (voir
    // le rapport, § « Dans ton rapport final »).
    #[test]
    fn une_fenetre_detachee_repliee_reduit_puis_restaure_son_viewport_os() {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        context.set_fonts(stokesforge_render_egui::font_definitions());
        egui_extras::install_image_loaders(&context);

        let root_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 700.0),
            )),
            ..Default::default()
        };
        context
            .run_ui(root_input.clone(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    graph_drop_target(ui, &app.state, &mut Vec::new(), &mut app.graph_views, graph);
                });
            })
            .textures_delta
            .clear();

        app.detach_tab(Tab::Graph(graph));
        let tab = Tab::Graph(graph);
        let viewport_id = egui::ViewportId::from_hash_of(tab);
        let mut output = context.run_ui(root_input.clone(), |ui| {
            app.show_detached_tabs(ui.ctx());
        });
        let callback = output.viewport_output[&viewport_id]
            .viewport_ui_cb
            .clone()
            .expect("le détachement doit publier un callback de viewport");
        output.textures_delta.clear();

        // Rectangle « OS » simulé : le test en a la main, comme le ferait
        // le vrai backend en rapportant, image après image, la taille
        // réellement appliquée par la fenêtre native.
        let outer_rect = std::cell::Cell::new(egui::Rect::from_min_size(
            egui::pos2(100.0, 80.0),
            GRAPH_WINDOW_SIZE,
        ));

        let paint = |events: Vec<egui::Event>| -> egui::FullOutput {
            let mut input = root_input.clone();
            input.viewport_id = viewport_id;
            input.viewports.insert(
                viewport_id,
                egui::ViewportInfo {
                    parent: Some(egui::ViewportId::ROOT),
                    outer_rect: Some(outer_rect.get()),
                    ..Default::default()
                },
            );
            input.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                outer_rect.get().size(),
            ));
            input.events = events;
            let mut output = context.run_ui(input, |ui| callback(ui));
            output.textures_delta.clear();
            output
        };

        // Image de stabilisation (Wayland, voir `needs_initial_repaint`) :
        // consomme le repaint initial et amorce `expanded_size` à la
        // taille de départ.
        paint(Vec::new());
        // Seconde image : les textures SVG des icônes sont prêtes.
        let first = paint(Vec::new());
        let icon_pos = *find_icon_centers(&first.shapes)
            .first()
            .expect("la flèche de repli doit être peinte, en haut à gauche");

        // Un clic (glissé nul, pression puis relâchement) sur le chevron de
        // repli, réparti sur deux images comme le veut la détection de clic
        // d'egui — voir `click_at`.
        paint(vec![egui::Event::PointerMoved(icon_pos)]);
        paint(vec![egui::Event::PointerButton {
            pos: icon_pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        let collapsed_output = paint(vec![egui::Event::PointerButton {
            pos: icon_pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);

        let expected_collapsed_height = WINDOW_HEADER_HEIGHT + WINDOW_CONTENT_MARGIN.sum().y;
        let commands = &collapsed_output.viewport_output[&viewport_id].commands;
        assert!(
            commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::InnerSize(size)
                    if (size.x - GRAPH_WINDOW_SIZE.x).abs() < f32::EPSILON
                        && (size.y - expected_collapsed_height).abs() < f32::EPSILON
            )),
            "le repli doit réduire le viewport OS à la hauteur de sa bande d'en-tête, \
             commandes={commands:?}"
        );
        {
            let model = app.detached_tabs[&tab]
                .view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            assert!(model.collapsed, "le repli doit être appliqué au modèle");
        }

        // L'OS applique la commande : l'image suivante rapporte le petit
        // rectangle. Plus rien n'en est persisté depuis l'issue #16 : la
        // fenêtre OS détachée n'a pas de géométrie retenue (seule la
        // géométrie flottante l'est, voir `sync_window_views`).
        outer_rect.set(egui::Rect::from_min_size(
            outer_rect.get().min,
            egui::vec2(GRAPH_WINDOW_SIZE.x, expected_collapsed_height),
        ));
        paint(Vec::new());

        // Dépli : même chevron (glyphe différent, même rectangle), même
        // schéma de clic — doit retrouver exactement la taille d'avant
        // repli, pas une taille par défaut.
        paint(vec![egui::Event::PointerMoved(icon_pos)]);
        paint(vec![egui::Event::PointerButton {
            pos: icon_pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        let expanded_output = paint(vec![egui::Event::PointerButton {
            pos: icon_pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        let commands = &expanded_output.viewport_output[&viewport_id].commands;
        assert!(
            commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::InnerSize(size)
                    if (size.x - GRAPH_WINDOW_SIZE.x).abs() < f32::EPSILON
                        && (size.y - GRAPH_WINDOW_SIZE.y).abs() < f32::EPSILON
            )),
            "le dépli doit restaurer exactement la taille d'avant repli, commandes={commands:?}"
        );
        {
            let model = app.detached_tabs[&tab]
                .view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            assert!(!model.collapsed, "le dépli doit être appliqué au modèle");
        }
    }

    // B1/B3 suite, puis B7 (`retours-2026-09-23-bugs.md`) :
    // les fenêtres détachées (viewports OS séparés) n'avaient aucun contour
    // de surbrillance — `active_tab`/`floating_tabs` ne les couvrent pas,
    // seul le focus OS du viewport a un sens pour elles. Même logique que
    // les fenêtres flottantes (`active_window_stroke`), câblée sur
    // `RawInput::focused` plutôt que sur `active_tab`. B7 a changé *où* ce
    // contour actif est peint : plus sur le trait du `Frame` (dont la
    // géométrie suit le contenu et se faisait rogner par
    // `CentralPanel::show_inside_dyn` sur les bords qui débordent), mais à
    // part, sur `input.content_rect()` — ce test vérifie maintenant la
    // couleur *et* la géométrie de ce contour.
    #[test]
    fn la_fenetre_detachee_recolore_son_contour_selon_le_focus_os() {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        context.set_fonts(stokesforge_render_egui::font_definitions());

        let root_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 700.0),
            )),
            ..Default::default()
        };
        context
            .run_ui(root_input.clone(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    graph_drop_target(ui, &app.state, &mut Vec::new(), &mut app.graph_views, graph);
                });
            })
            .textures_delta
            .clear();

        app.detach_tab(Tab::Graph(graph));
        let viewport_id = egui::ViewportId::from_hash_of(Tab::Graph(graph));
        let mut output = context.run_ui(root_input.clone(), |ui| {
            app.show_detached_tabs(ui.ctx());
        });
        let callback = output.viewport_output[&viewport_id]
            .viewport_ui_cb
            .clone()
            .expect("le détachement doit publier un callback de viewport");
        output.textures_delta.clear();

        // Toutes les formes rectangulaires dont le trait n'est pas de
        // largeur nulle — depuis B7, il peut y en avoir deux : le trait par
        // défaut du `Frame` du panneau central, et le contour actif peint à
        // part par-dessus (`paint_active_window_outline`) quand la fenêtre a
        // le focus OS.
        fn stroke_shapes(
            shapes: &[egui::epaint::ClippedShape],
        ) -> Vec<(egui::Color32, egui::Rect)> {
            fn find(shape: &egui::Shape, out: &mut Vec<(egui::Color32, egui::Rect)>) {
                match shape {
                    egui::Shape::Rect(rect) if rect.stroke.width > 0.0 => {
                        out.push((rect.stroke.color, rect.rect));
                    }
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| find(s, out)),
                    _ => {}
                }
            }
            let mut out = Vec::new();
            shapes
                .iter()
                .for_each(|painted| find(&painted.shape, &mut out));
            out
        }

        let paint_focused = |focused: bool| {
            let mut input = root_input.clone();
            input.viewport_id = viewport_id;
            input.focused = focused;
            input.viewports.insert(
                viewport_id,
                egui::ViewportInfo {
                    parent: Some(egui::ViewportId::ROOT),
                    ..Default::default()
                },
            );
            input.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                GRAPH_WINDOW_SIZE,
            ));
            let mut output = context.run_ui(input, |ui| callback(ui));
            output.textures_delta.clear();
            stroke_shapes(&output.shapes)
        };

        let style = egui::Style::default();
        let content_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, GRAPH_WINDOW_SIZE);

        // D'autres formes de la fenêtre (bordure du graphe tracé, etc.) ont
        // leur propre trait, sans rapport avec l'activité — on cherche donc
        // la présence du bon contour plutôt que l'exclusivité de sa couleur.
        let inactive_strokes = paint_focused(false);
        assert!(
            inactive_strokes.iter().any(|(color, rect)| *color
                == style.visuals.window_stroke().color
                && *rect == content_rect),
            "sans focus OS, le Frame du panneau central doit peindre le trait par défaut \
             d'egui sur {content_rect:?} : {inactive_strokes:?}"
        );
        assert!(
            !inactive_strokes
                .iter()
                .any(|(color, _)| *color == style.visuals.selection.stroke.color),
            "sans focus OS, aucun contour ne doit reprendre la couleur de sélection du thème : \
             {inactive_strokes:?}"
        );

        let active_strokes = paint_focused(true);
        let active_outline = active_strokes
            .iter()
            .find(|(color, _)| *color == style.visuals.selection.stroke.color)
            .expect(
                "avec le focus OS, la fenêtre détachée doit peindre un contour reprenant la \
                 couleur de sélection du thème",
            );
        assert_eq!(
            active_outline.1, content_rect,
            "le contour actif doit longer exactement le rectangle du viewport OS \
             (`input.content_rect()`, {content_rect:?}), pas un rectangle recalculé par egui \
             depuis le contenu — B7 n'est pas corrigé (trouvé {:?})",
            active_outline.1,
        );
    }
}
