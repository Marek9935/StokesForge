//! La glu `eframe` : traduit clavier et souris en [`Action`] et peint
//! l'[`AppState`]. Aucune logique de modèle ne vit ici — tout ce qui décide
//! est dans [`crate::state`], qui se teste sans fenêtre. Ce module choisit
//! en revanche la **disposition** (fenêtres flottantes confinées, voir § du
//! même nom plus bas) et le **cadre** (menus, raccourcis, sélecteur de
//! fichiers `rfd`) qui l'entourent — des décisions de présentation, pas de
//! modèle, donc légitimement hors de `state`.
//!
//! # Ce que Qt donnait gratuitement et qu'il faut écrire ici
//!
//! Une `QAction` reliée à un `QUndoStack` portait d'un coup le raccourci
//! clavier, l'entrée de menu, son grisage et son libellé (« Annuler
//! *Renommer* »). Ici les quatre sont séparés : le raccourci est déclaré et
//! consommé à la main ([`SHORTCUT_UNDO`] et compagnie), le grisage passe par
//! `add_enabled`, le libellé vient de [`AppState::undo_label`], et rien ne
//! garantit leur cohérence sinon la relecture.
//!
//! # Project Manager, et une fenêtre par contenu
//!
//! Le panneau qui liste l'arbre du projet **n'est pas une fenêtre de
//! contenu** : c'est un [`egui::Panel::left`] fixe, peint par
//! [`StokesForgeApp::project_manager_panel`], toujours à gauche, jamais recouvert
//! par ni ne recouvrant le [`egui::CentralPanel`] qui porte les fenêtres
//! flottantes (§ Fenêtres flottantes confinées plus bas) — garanti par
//! construction en le peignant *avant* lui dans [`StokesForgeApp::ui`], comme la
//! barre de menu (`egui::Panel::top`). Il reste redimensionnable en largeur
//! par glisser du bord (comportement par défaut d'`egui::Panel::left`), mais
//! **figé en position** : pas de bascule vers la droite, pas de
//! repositionnement par glisser — voir § Attacher à gauche ou à droite
//! ci-dessous pour pourquoi, et pour ce qui reste volontairement hors
//! périmètre. Le contenu lui-même ([`project_manager_contents`], l'arbre
//! cliquable du projet) reste un panneau fixe, à part des fenêtres de
//! contenu qu'il ouvre.
//!
//! # Console de script, et l'ordre des panneaux
//!
//! La console de script (édition de [`AppState::script_source`], boutons
//! Exécuter/Arrêter, sortie) suit le même patron que le Project Manager :
//! ce n'est pas une fenêtre de contenu, mais un [`egui::Panel::bottom`] fixe,
//! peint par [`StokesForgeApp::script_panel`].
//!
//! Point d'ordre important avec `egui::Panel` : **le premier panneau ajouté
//! est le plus extérieur**, chaque panneau suivant est inséré à l'intérieur
//! de ce que les précédents laissent. Pour que la console de script coure sur
//! toute la largeur de la fenêtre — y compris sous le Project Manager, pas
//! seulement sous la zone du `CentralPanel` — [`StokesForgeApp::script_panel`] doit
//! donc être appelé *avant* [`StokesForgeApp::project_manager_panel`] dans
//! [`StokesForgeApp::ui`]. L'ordre d'appel complet y est : barre de menu
//! (`egui::Panel::top`) → console de script (`egui::Panel::bottom`) → Project
//! Manager (`egui::Panel::left`) → `CentralPanel` en dernier ; un panneau
//! posé plus tôt réserve sa bande sur toute la largeur/hauteur restante, donc
//! la console de script, postée avant le Project Manager, s'étend bien sous
//! lui plutôt que d'être resserrée entre lui et le bord droit. C'est aussi
//! cet ordre qui donne à `viewport_rect` (§ Fenêtres flottantes confinées)
//! son étendue exacte : ce que ces panneaux fixes laissent, rien de plus —
//! la portion actuellement *visible* de l'espace de travail, à distinguer
//! de sa taille logique fixe (`WORKSPACE_SIZE`).
//!
//! `egui::Panel::bottom` (comme `Panel::top`) **n'est pas redimensionnable
//! par défaut**, à la différence de `left`/`right` — contrairement au Project
//! Manager, [`StokesForgeApp::script_panel`] appelle donc explicitement
//! `.resizable(true)` pour retrouver le glisser du bord en hauteur.
//!
//! Comme pour le Project Manager, aucune bascule de position n'existe ici (il
//! n'y en a jamais eu à basculer : la console a toujours été en bas), et le
//! panneau ne montre aucun mécanisme d'onglet.
//!
//! Les quatre natures de fenêtre que le Project Manager ouvre — [`Tab::Book`],
//! [`Tab::Graph`], [`Tab::Matrix`], [`Tab::Note`] — sont, elles, des fenêtres
//! flottantes (§ suivant). La grille unique d'avant une tâche antérieure
//! (`Tab::Grid`, une seule feuille affichée à la fois pour tout le projet) a
//! disparu : chaque classeur ouvert a désormais son propre onglet
//! [`Tab::Book`], avec sa propre sous-navigation interne entre ses feuilles
//! ([`AppState::worksheets_of`], [`AppState::selected_sheet_in`]) — un
//! classeur peut avoir plusieurs feuilles, la grille d'un onglet ne montre
//! jamais que celle actuellement choisie *dans ce classeur*. [`Tab::Matrix`]
//! et [`Tab::Note`] existent aussi : la matrice est un **espace réservé
//! documenté** (édition hors périmètre, voir [`matrix_contents`]), la note
//! est réellement éditable (`stokesforge-core` expose déjà son texte).
//!
//! [`StokesForgeApp::open_tab`] est le seul endroit qui décide « ouvrir » vs
//! « focaliser » un onglet, quelle que soit sa nature — le même geste que
//! l'ancien `open_graph_tab`, généralisé.
//!
//! # Fenêtres flottantes, pile, maximisation
//!
//! Placement, confinement, cascade, ordre de pile, calques et maximisation :
//! voir la doc de tête de `window_placement.rs` et `window_stack.rs` (#101).
//!
//! # Attacher le Project Manager à gauche ou à droite
//!
//! **Hors périmètre, délibérément.** Une tâche antérieure avait donné au
//! Project Manager une bascule gauche/droite dans le dock (menu « Fenêtre »
//! et clic droit sur son onglet) ; elle a disparu avec le passage à un
//! panneau fixe. Le côté droit et le repositionnement par glisser sont un
//! travail futur volontairement laissé de côté par cette tâche-ci — pas
//! implémentés, même partiellement.
//!
//! # Détachement en fenêtres OS
//!
//! Project Manager et fenêtres de contenu : voir la doc de tête de
//! `window_detach.rs` (#101).
//!
//! # Sélecteur de fichiers natif
//!
//! Le menu « Fichier » ouvre le `rfd::FileDialog` natif pour « Ouvrir… » et
//! « Enregistrer sous… » (modal, comme tout sélecteur de fichiers de bureau
//! — bloquer le rendu le temps que l'utilisateur choisisse un fichier est le
//! comportement attendu, à ne pas confondre avec le blocage qu'évite
//! `RunScript`, qui lui n'a pas de raison de durer). Ctrl+O suit exactement
//! « Ouvrir… », Ctrl+Maj+S « Enregistrer sous… », tandis que Ctrl+S conserve
//! le geste rapide d'enregistrer dans le chemin déjà connu.
//!
//! # Limites assumées du banc
//!
//! - **Les raccourcis sont consommés au niveau du contexte**, donc *avant*
//!   les widgets. Conséquence mesurable : Ctrl+Z pendant la saisie d'une
//!   cellule annule la dernière **commande du modèle**, pas la dernière
//!   frappe dans le champ — l'annulation interne des `TextEdit` d'egui est
//!   masquée. C'est le comportement attendu d'un tableur, et c'est aussi la
//!   limite : le banc ne sait pas distinguer « annuler ma saisie en cours »
//!   de « annuler la dernière commande ».
//! - **Une seule cellule est éditable à la fois** (toutes grilles de tous les
//!   classeurs ouverts confondues). La grille est virtualisée
//!   (`TableBuilder::body().rows()`), il n'est donc pas question de garder un
//!   tampon par cellule : une feuille peut avoir un million de lignes.
//! - **Pas de persistance de la disposition entre deux lancements** —
//!   `StokesForgeApp::new` la reconstruit à chaque démarrage.
//!
//! # La console de script ne fige pas la fenêtre
//!
//! `ScriptEngine::run` **bloque** jusqu'au terme du script ; l'appeler depuis
//! cette boucle de rendu figerait la fenêtre entière, bouton « Arrêter »
//! compris — le défaut le plus visible que cette console puisse avoir. La
//! parade n'est **pas** ici : elle vit dans [`crate::state::AppState::
//! dispatch`], qui traite `RunScript` en lançant le travail sur un fil
//! détaché et en revenant aussitôt (voir la doc de tête de `crate::state`).
//! Ce module n'a donc que deux choses à faire, chaque image : appeler
//! [`AppState::poll_script`] pour récupérer un résultat prêt, et demander un
//! nouveau rendu ([`egui::Context::request_repaint`]) tant qu'un script
//! tourne — sans quoi, egui ne redessinant que sur événement d'entrée, la fin
//! du script n'apparaîtrait qu'au prochain mouvement de souris.

mod contents;
mod dialogs;
mod file_drop;
mod graph_text_edit;
mod graph_window;
mod grid;
mod import_wizard;
mod object_manager_ui;
mod plot_settings;
mod project_manager;
mod script_console;
mod shortcuts;
mod tab;
mod tab_strip;
mod window_chrome;
mod window_detach;
mod window_placement;
mod window_stack;
mod workbook;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dialogs::{modal_dialog_with, DialogFooter, DialogOutcome};
use file_drop::{
    begin_file_drop, claim_file_drop, finish_file_drop, open_project_in_new_window,
    remember_recent_imports,
};
use graph_text_edit::{
    add_annotation_at, annotation_editor_id, open_annotation_editor, open_series_name_editor,
    pending_format_id, pending_zone_font_id, text_editor_id, AnnotationEditor, TextEditTarget,
    TextEditor,
};
use graph_window::{
    disabled_shape_order_buttons, open_shape_properties, request_graph_export, shape_order_buttons,
};
pub use grid::{grid_grip_side, GripSide};
use grid::{keyboard_focus_outside_grid, metadata_row_visible, set_metadata_row_visible};
use import_wizard::import_window;
use object_manager_ui::{
    object_manager_contents, object_node_menu, select_manager_target, selected_manager_target,
    ObjectManagerRequest,
};
use plot_settings::{column_metadata_window, plot_settings_window};
use project_manager::{project_manager_contents_from_tree, PmInteraction};
use script_console::{script_contents, script_header};
use shortcuts::{
    collect_undo_redo_shortcuts, shortcut_text_delete, SHORTCUT_DUPLICATE_SHEET,
    SHORTCUT_EXPORT_GRAPH, SHORTCUT_IMPORT, SHORTCUT_IMPORT_QWERTY, SHORTCUT_IMPORT_TEXT,
    SHORTCUT_NEW_PROJECT, SHORTCUT_OPEN, SHORTCUT_REDO_Y, SHORTCUT_RENAME_SHEET,
    SHORTCUT_RUN_SCRIPT, SHORTCUT_SAVE, SHORTCUT_SAVE_AS, SHORTCUT_UNDO,
};
use tab::{tab_for_window, tab_node, Tab};
use window_chrome::{
    floating_resize_handles, show_window_in_layer, window_header, HeaderPlacement,
    WindowHeaderActions,
};
use window_detach::{DetachedProjectManagerRequests, DetachedTab, ProjectManagerViewModel};
use window_placement::geometry_rect;
use window_stack::window_nodes;
use workbook::{book_contents, workbook_menu};

use eframe::egui;
use stokesforge_core::{Node, NodeId, Tree};
use stokesforge_render_egui::{GraphView, TextTarget, DEFAULT_FIGURE_SIZE};
use stokesforge_session::{Snapshot, WindowGeometry};

use crate::color_picker::SavedColorEdit;
use crate::i18n;
use crate::icons::{icon_button, Icon, IconRole};
use crate::preferences::Preferences;
use crate::state::{
    active_graph, node_title_in, Action, ActiveTabKind, AppState, ImportDestination,
    PlotSettingsTab, SaveTarget,
};
use crate::toolbar::{BarEnv, ToolbarId, ToolbarLayout, ToolbarRequest, ToolbarState};
use crate::tr;

/// Variable d'environnement du mécanisme d'auto-fermeture (voir
/// [`StokesForgeApp::new`]).
pub const AUTOQUIT_ENV: &str = "STOKESFORGE_APP_AUTOQUIT_MS";
/// Variable d'environnement du mécanisme de capture (voir
/// [`StokesForgeApp::screenshot`]).
pub const SCREENSHOT_ENV: &str = "STOKESFORGE_APP_SCREENSHOT";
/// Images laissées passer avant de demander une capture.
///
/// egui résout la taille de ses panneaux sur deux ou trois images : capturer
/// la première montrerait une mise en page transitoire — et, pour un onglet
/// de graphe, un compteur de cache qui n'a pas encore eu l'occasion de
/// servir une image inchangée. On regarde le régime établi, pas le
/// démarrage.
const SCREENSHOT_SETTLE_FRAMES: u32 = 8;

/// Hauteur par défaut du panneau fixe de la console de script — point de
/// départ seulement, l'utilisateur peut ensuite le redimensionner en
/// glissant son bord (`.resizable(true)`, voir [`StokesForgeApp::script_panel`]).
const SCRIPT_PANEL_HEIGHT: f32 = 220.0;
/// Largeur par défaut du panneau Project Manager fixe, et de la fenêtre OS
/// séparée quand il est détaché.
const PROJECT_MANAGER_WIDTH: f32 = 260.0;

/// L'application `eframe` : l'état du banc, la disposition en fenêtres
/// flottantes confinées, et le strict nécessaire au rendu.
pub struct StokesForgeApp {
    /// L'état, seul dépositaire de la logique.
    state: AppState,
    /// Les barres d'outils : disposition, visibilité et glisser en cours
    /// (issue #60).
    toolbars: ToolbarState,
    object_manager_floating: bool,
    object_manager_rect: Option<egui::Rect>,
    active_detached: Option<Tab>,
    /// Les onglets de contenu actuellement ouverts en fenêtre flottante, dans
    /// l'ordre où ils ont été focalisés pour la dernière fois — le dernier
    /// élément est le plus récemment focalisé (voir la doc de tête du
    /// module, § Fenêtres flottantes confinées, sous-section z-order).
    /// Absent le temps d'être maximisé ou détaché en fenêtre OS séparée
    /// (même invariant que `detached_tabs` pour ce second cas).
    floating_tabs: Vec<Tab>,
    /// Rang de cascade attribué à chaque fenêtre flottante actuellement
    /// ouverte, à sa création — sert à calculer sa position par défaut
    /// (`StokesForgeApp::ui`) ; ne bouge plus ensuite, y compris si l'utilisateur
    /// déplace la fenêtre (`egui` retient alors sa position lui-même, via son
    /// `Id`). Une entrée disparaît de cette table en même temps que de
    /// `floating_tabs`.
    cascade_rank: HashMap<Tab, u32>,
    /// Prochain rang de cascade à distribuer — incrémenté à chaque nouvelle
    /// fenêtre flottante créée (pas à un simple refocus), remis à zéro tous
    /// les [`CASCADE_WRAP`] pour ne pas dériver hors de l'aire de travail.
    next_cascade: u32,
    /// Onglet à ramener au premier plan au prochain rendu — posé par
    /// [`StokesForgeApp::open_tab`] quand l'onglet rouvert était déjà une fenêtre
    /// flottante existante (voir la doc de tête du module, § z-order).
    focus_next: Option<Tab>,
    /// Une vue de graphe (et son cache de formes, phase R4) par graphe
    /// **ouvert** — c'est-à-dire ayant eu un onglet au moins une fois,
    /// l'entrée survit à la fermeture de l'onglet. Un cache par vue est la
    /// règle (`stokesforge_render_egui::GraphView`) ; avec plusieurs graphes
    /// affichables simultanément, une seule vue ne suffit plus.
    graph_views: HashMap<NodeId, GraphView>,
    /// Onglets (`Graph` ou `Book`, B5 — `retours-2026-09-23-bugs.md`)
    /// dont seul le contenu est replié. Cet état de présentation reste local
    /// à l'application : il ne modifie ni le nœud, ni sa géométrie
    /// enregistrée, ni les autres fenêtres.
    collapsed_tabs: HashSet<Tab>,
    /// Onglet actuellement maximisé, s'il y en a un — voir la doc de tête du
    /// module, § maximiser. Ne peut plus valoir le Project Manager depuis
    /// cette tâche : ce n'est plus un onglet, et un panneau fixe toujours
    /// entièrement visible n'a rien à gagner à être « maximisé ».
    maximized_tab: Option<Tab>,
    /// Vrai quand le Project Manager est détaché en fenêtre OS séparée (voir
    /// la doc de tête du module, § détachement). Dans cet état,
    /// [`StokesForgeApp::project_manager_panel`] n'est simplement pas peint.
    pm_detached: bool,
    /// Vrai quand la console de script est repliée sur sa seule barre de
    /// titre (issue #79). Repliée au démarrage, et non mémorisée : ce n'est
    /// pas une préférence mais un état de la session ; lancer un script la
    /// déplie, pour qu'on voie la sortie.
    script_collapsed: bool,
    /// Ce que la fenêtre OS détachée du Project Manager peint, resynchronisé
    /// depuis `self.state` à chaque image par
    /// [`StokesForgeApp::show_detached_project_manager`] — voir sa doc de tête.
    /// `Arc<Mutex<_>>` uniquement parce que le callback `show_viewport_deferred`
    /// l'exige (`Fn` + `Send + Sync + 'static`, pas de capture de `&mut
    /// self`) ; ce n'est **pas** `AppState` elle-même derrière un verrou,
    /// seulement ce strict sous-ensemble.
    pm_view_model: Arc<Mutex<ProjectManagerViewModel>>,
    /// Ce que la fenêtre OS détachée du Project Manager écrit (clics
    /// d'ouverture, demande de réattachement), drainé chaque image par
    /// [`StokesForgeApp::ui`] — même rôle que `open_requests`/`actions` pour le
    /// reste du banc, juste posé par un callback différé plutôt que collecté
    /// en direct.
    pm_requests: Arc<Mutex<DetachedProjectManagerRequests>>,
    /// Onglets de contenu (`Book`/`Graph`/`Matrix`/`Note`) actuellement
    /// détachés en fenêtre OS séparée — voir la doc de tête du module,
    /// § détachement générique. Une entrée par onglet détaché ; absent de
    /// `floating_tabs` tant qu'il l'est (même invariant que `pm_detached`
    /// pour le Project Manager, généralisé à plusieurs onglets simultanés).
    detached_tabs: HashMap<Tab, DetachedTab>,
    /// Position de chaque fenêtre flottante, en coordonnées logiques de
    /// l'espace de travail, déplacée par la barre de titre maison et bornée
    /// par `clamp_to_workspace` — seule source de vérité de la position.
    window_positions: HashMap<Tab, egui::Pos2>,
    /// Taille logique de chaque fenêtre flottante, modifiée par les
    /// poignées de redimensionnement maison ([`floating_resize_handles`],
    /// 4 côtés + 4 coins) — seule source de vérité de la taille, comme
    /// `window_positions` l'est de la position.
    window_sizes: HashMap<Tab, egui::Vec2>,
    /// Défilement courant de l'espace de travail (zone MDI, taille fixe
    /// [`WORKSPACE_SIZE`]) — voir sa doc. `window_positions`/`window_sizes`
    /// ci-dessus, et la géométrie persistée dans le projet, sont exprimées
    /// dans les coordonnées logiques de cet espace, pas en pixels écran ;
    /// [`StokesForgeApp::ui`] applique ce décalage à la peinture. Transitoire, comme la position de défilement d'un
    /// `egui::ScrollArea` : jamais persisté.
    workspace_scroll: egui::Vec2,
    /// Dernière géométrie (avec son état) envoyée à l'acteur, pour ne pas lui
    /// adresser une requête à chaque image pendant qu'une fenêtre reste
    /// immobile.
    saved_window_geometries: HashMap<NodeId, WindowGeometry>,
    /// Dernier rectangle **flottant** connu de chaque fenêtre, en
    /// coordonnées de l'espace de travail. Il survit à la fermeture, à la
    /// maximisation et au détachement (issue #16) et prime sur l'instantané,
    /// que `AppState::set_window_geometry` ne rafraîchit pas : sans lui, une
    /// fenêtre rouverte reprendrait la géométrie du chargement du projet.
    floating_rects: HashMap<NodeId, egui::Rect>,
    /// Sélection courante dans l'arbre du Project Manager (§6) — Ctrl/Cmd-clic
    /// en bascule l'appartenance ; consommée par « Afficher »/« Masquer ».
    /// Indépendante de `selection.rs`, propre à la grille de données.
    pm_selection: HashSet<Tab>,
    /// Onglets masqués sans être fermés (§6) — même patron que
    /// `collapsed_tabs`, mais l'onglet entier n'est plus peint du tout
    /// (au lieu de replier seulement son contenu). Reste dans
    /// `floating_tabs` : cascade/position/géométrie survivent au masquage.
    hidden_tabs: HashSet<Tab>,
    /// Fichier où déposer une capture de la fenêtre, s'il en est demandé une
    /// (mécanisme de vérification, voir [`StokesForgeApp::screenshot`]).
    screenshot: Option<std::path::PathBuf>,
    /// Images restant à laisser passer avant de demander la capture.
    screenshot_delay: u32,
    /// Délai d'auto-fermeture, s'il est demandé (mécanisme de vérification).
    autoquit: Option<Duration>,
    /// Instant du premier rendu, d'où se mesure le délai d'auto-fermeture.
    first_frame: Option<Instant>,
}

impl StokesForgeApp {
    /// Construit l'application autour d'un état.
    ///
    /// # Disposition initiale
    ///
    /// Ni le Project Manager ni la console de script n'entrent pour rien
    /// dans ce plan : ce sont deux panneaux fixes peints à part (voir
    /// [`StokesForgeApp::project_manager_panel`] et [`StokesForgeApp::script_panel`]),
    /// pas des fenêtres flottantes. Si le projet contient au moins un
    /// classeur, son onglet s'ouvre en fenêtre flottante, suivi de l'onglet
    /// du premier graphe s'il y en a un — montrer quelque chose sans qu'on
    /// ait à cliquer, comme l'ancien comportement par défaut. À défaut de
    /// classeur ou de graphe, aucune fenêtre flottante ne s'ouvre (seuls les
    /// deux panneaux fixes et la barre de menu sont visibles).
    ///
    /// # Auto-fermeture
    ///
    /// Si la variable d'environnement `STOKESFORGE_APP_AUTOQUIT_MS` contient un
    /// entier, la fenêtre se ferme d'elle-même après ce délai en
    /// millisecondes, mesuré depuis le premier rendu. **C'est un mécanisme de
    /// vérification, pas une fonctionnalité** : il permet de prouver en ligne
    /// de commande que la fenêtre s'ouvre et peint réellement — ce qu'aucun
    /// test `cargo` ne peut faire — sans qu'un humain ait à cliquer. Une
    /// valeur illisible est ignorée en silence : le banc ne doit pas refuser
    /// de démarrer à cause d'un outillage de vérification.
    pub fn new(state: AppState) -> Self {
        let autoquit = std::env::var(AUTOQUIT_ENV)
            .ok()
            .and_then(|raw| raw.trim().parse::<u64>().ok())
            .map(Duration::from_millis);

        let saved_window_geometries: HashMap<NodeId, WindowGeometry> =
            (*state.snapshot().window_geometries).clone();
        let floating_rects: HashMap<NodeId, egui::Rect> = saved_window_geometries
            .iter()
            .map(|(node, geometry)| (*node, geometry_rect(geometry)))
            .collect();
        let mut floating_tabs = Vec::new();
        let mut cascade_rank = HashMap::new();
        let mut next_cascade = 0u32;
        let mut collapsed_tabs = HashSet::new();
        let mut maximized_tab = None;
        let mut open_initial = |tab: Tab| {
            floating_tabs.push(tab);
            cascade_rank.insert(tab, next_cascade);
            next_cascade = next_cascade.wrapping_add(1);
        };
        if saved_window_geometries
            .values()
            .any(|geometry| geometry.view.is_some())
        {
            // Projet enregistré avec l'état des fenêtres (issue #16) : on le
            // rétablit tel quel, une fenêtre détachée revenant flottante.
            let mut restored: Vec<(u32, Tab, stokesforge_session::WindowView)> = Vec::new();
            for (node, kind) in window_nodes(&state.project_tree()) {
                let Some(view) = saved_window_geometries.get(&node).and_then(|g| g.view) else {
                    continue;
                };
                if view.mode != stokesforge_session::WindowMode::Closed {
                    restored.push((view.stack, tab_for_window(node, kind), view));
                }
            }
            restored.sort_by_key(|(stack, tab, _)| (*stack, tab_node(*tab).raw()));
            for (_, tab, view) in restored {
                open_initial(tab);
                if view.collapsed {
                    collapsed_tabs.insert(tab);
                }
                if view.mode == stokesforge_session::WindowMode::Maximized {
                    maximized_tab = Some(tab);
                }
            }
        } else {
            if let Some((first_book, _)) = state.books().into_iter().next() {
                open_initial(Tab::Book(first_book));
            }
            if let Some((first_graph, _)) = state.graphs().into_iter().next() {
                open_initial(Tab::Graph(first_graph));
            }
        }
        // Sans classeur ni graphe à montrer, aucune fenêtre flottante ne
        // s'ouvre — ni la console de script ni le Project Manager n'ont leur
        // place ici, ce sont des panneaux fixes.

        Self {
            state,
            toolbars: ToolbarState::default(),
            object_manager_floating: false,
            object_manager_rect: None,
            active_detached: None,
            floating_tabs,
            cascade_rank,
            next_cascade,
            focus_next: None,
            graph_views: HashMap::new(),
            collapsed_tabs,
            maximized_tab,
            pm_detached: false,
            script_collapsed: true,
            pm_view_model: Arc::new(Mutex::new(ProjectManagerViewModel::default())),
            pm_requests: Arc::new(Mutex::new(DetachedProjectManagerRequests::default())),
            detached_tabs: HashMap::new(),
            window_positions: HashMap::new(),
            window_sizes: HashMap::new(),
            workspace_scroll: egui::Vec2::ZERO,
            saved_window_geometries,
            floating_rects,
            pm_selection: HashSet::new(),
            hidden_tabs: HashSet::new(),
            autoquit,
            first_frame: None,
            screenshot: std::env::var_os(SCREENSHOT_ENV).map(std::path::PathBuf::from),
            screenshot_delay: SCREENSHOT_SETTLE_FRAMES,
        }
    }

    /// Ouvre en fenêtre les classeurs ajoutés à la racine depuis que celle-ci
    /// avait `before` enfants (import dans de nouveaux classeurs).
    fn open_new_workbooks_after(&mut self, before: usize) {
        let root = self.state.tree().root();
        let new_books: Vec<NodeId> = self
            .state
            .tree()
            .node(root)
            .map(|node| node.children().iter().skip(before).copied().collect())
            .unwrap_or_default();
        for book in new_books {
            if self.state.tree().node(book).is_some_and(|node| {
                matches!(
                    node.kind(),
                    stokesforge_core::node::NodeKind::Workbook { .. }
                )
            }) {
                self.open_tab(Tab::Book(book));
            }
        }
    }

    /// Ouvre l'assistant d'import sur le classeur actif (maximisé, ou au
    /// premier plan, ou le seul classeur détaché) — menu Fichier et Ctrl+".
    fn open_import_dialog(&mut self) {
        let active_book = self.active_book();
        self.state.open_import(active_book);
    }

    /// Le classeur actif : maximisé, sinon au premier plan, sinon le seul
    /// classeur détaché.
    fn active_book(&self) -> Option<NodeId> {
        let mut detached_books = self.detached_tabs.keys().filter_map(|tab| match tab {
            Tab::Book(book) => Some(*book),
            _ => None,
        });
        let only_detached_book = detached_books
            .next()
            .filter(|_| detached_books.next().is_none());
        self.maximized_tab
            .or_else(|| self.floating_tabs.last().copied())
            .and_then(|tab| match tab {
                Tab::Book(book) => Some(book),
                _ => None,
            })
            .or(only_detached_book)
    }

    /// Consomme les raccourcis au niveau du contexte et en fait des actions.
    fn collect_shortcuts(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if self.state.new_project_pending() || self.state.pending_sheet_delete().is_some() {
            return;
        }
        let mut save_as = false;
        let mut open = false;
        let mut import = false;
        let mut export = false;
        // Les gestes de feuille ne s'appliquent pas pendant une saisie dans un
        // champ, ni pendant l'édition du nom d'un onglet.
        let typing = keyboard_focus_outside_grid(ctx) || self.state.renaming_sheet().is_some();
        let active_sheet = self
            .active_book()
            .and_then(|book| self.state.selected_sheet_in(book));
        ctx.input_mut(|input| {
            if input.consume_shortcut(&SHORTCUT_NEW_PROJECT) && self.state.can_new_project() {
                actions.push(Action::NewProject {
                    pending_edits: false,
                });
            }
            // Ctrl+Maj+S doit être testé avant Ctrl+S : les modificateurs
            // supplémentaires restent compatibles avec ce dernier.
            if input.consume_shortcut(&SHORTCUT_SAVE_AS) {
                save_as = true;
            } else if input.consume_shortcut(&SHORTCUT_SAVE) {
                actions.push(Action::Save);
            }
            if input.consume_shortcut(&SHORTCUT_OPEN) {
                open = true;
            }
            if input.consume_shortcut(&SHORTCUT_EXPORT_GRAPH) {
                export = true;
            }
            if input.consume_shortcut(&SHORTCUT_RUN_SCRIPT) {
                actions.push(Action::RunScript);
            }
            if !typing {
                if let Some(sheet) = active_sheet {
                    if input.consume_shortcut(&SHORTCUT_RENAME_SHEET) {
                        actions.push(Action::BeginRenameSheet(sheet));
                    }
                    if input.consume_shortcut(&SHORTCUT_DUPLICATE_SHEET) {
                        actions.push(Action::DuplicateSheet(sheet));
                    }
                }
            }
            if input.consume_shortcut(&SHORTCUT_IMPORT)
                || input.consume_shortcut(&SHORTCUT_IMPORT_QWERTY)
            {
                import = true;
            }
        });
        if import && self.state.import_state().is_none() {
            self.open_import_dialog();
        }
        collect_undo_redo_shortcuts(ctx, actions);
        if save_as {
            self.choose_save_path(actions);
        }
        if open {
            self.choose_open_path(actions);
        }
        // Le graphe actif, calculé comme pour la barre de menus : le
        // raccourci n'agit que là où le menu « Graphique » existe.
        let active_tab = self.active_content_tab(ctx);
        if let Some(graph) = active_graph(active_tab_kind(active_tab)).filter(|_| export) {
            self.request_graph_export(graph, actions);
        }
    }

    /// Barre de menu : « Fichier », « Édition », « Graphique » (seulement
    /// quand l'onglet actif est un graphe, voir [`active_graph`]) et
    /// « Fenêtre ».
    ///
    /// « Graphique » ouvre la fenêtre des propriétés du graphe sur chacun de
    /// ses onglets ([`plot_settings_window`]) — les mêmes pages que le clic
    /// droit sur un composant du graphe ([`graph_view_contents`]), pour
    /// qu'aucun réglage ne soit atteignable **seulement** au clic droit — et
    /// réinitialise le zoom.
    ///
    /// « Fenêtre » porte les deux fonctionnalités qui touchent à la
    /// disposition elle-même plutôt qu'au modèle : maximiser/restaurer
    /// l'onglet actif, et détacher/réattacher le Project Manager en fenêtre
    /// OS séparée — voir la doc de tête du module.
    fn menu_bar(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        actions: &mut Vec<Action>,
        active_tab: Option<Tab>,
    ) {
        let mut object_requests = Vec::new();
        egui::Panel::top("stokesforge_menu_bar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(tr!("menu-file"), |ui| {
                    if ui
                        .add_enabled(
                            self.state.can_new_project(),
                            egui::Button::new(tr!("action-new-project"))
                                .shortcut_text(ctx.format_shortcut(&SHORTCUT_NEW_PROJECT)),
                        )
                        .on_hover_text(tr!("action-new-project", "hint"))
                        .clicked()
                    {
                        actions.push(Action::NewProject {
                            pending_edits: false,
                        });
                        ui.close();
                    }
                    if ui
                        .button(tr!("action-add-workbook"))
                        .on_hover_text(tr!("action-add-workbook", "hint"))
                        .clicked()
                    {
                        actions.push(Action::AddWorkbook);
                        ui.close();
                    }
                    if ui
                        .button(tr!("action-add-graph"))
                        .on_hover_text(tr!("action-add-graph", "hint"))
                        .clicked()
                    {
                        actions.push(Action::AddGraph);
                        ui.close();
                    }
                    if ui
                        .add(
                            egui::Button::new(tr!("action-import"))
                                .shortcut_text(SHORTCUT_IMPORT_TEXT),
                        )
                        .on_hover_text(tr!("action-import", "hint"))
                        .clicked()
                    {
                        self.open_import_dialog();
                        ui.close();
                    }
                    let active_sheet = self
                        .active_book()
                        .and_then(|book| self.state.selected_sheet_in(book));
                    if ui
                        .add_enabled(
                            active_sheet
                                .is_some_and(|sheet| self.state.can_refresh_from_source(sheet)),
                            egui::Button::new(tr!("action-refresh-from-source")),
                        )
                        .on_hover_text(tr!("action-refresh-from-source", "hint"))
                        .clicked()
                    {
                        if let Some(sheet) = active_sheet {
                            actions.push(Action::RefreshFromSource { sheet });
                        }
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            self.state.can_reopen_last_import(),
                            egui::Button::new(tr!("action-reopen-last-import")),
                        )
                        .on_hover_text(tr!("action-reopen-last-import", "hint"))
                        .clicked()
                    {
                        actions.push(Action::ReopenLastImport);
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .add(
                            egui::Button::new(tr!("action-open"))
                                .shortcut_text(ctx.format_shortcut(&SHORTCUT_OPEN)),
                        )
                        .clicked()
                    {
                        self.choose_open_path(actions);
                        ui.close();
                    }
                    if ui
                        .add(
                            egui::Button::new(tr!("action-save"))
                                .shortcut_text(ctx.format_shortcut(&SHORTCUT_SAVE)),
                        )
                        .clicked()
                    {
                        actions.push(Action::Save);
                        ui.close();
                    }
                    if ui
                        .add(
                            egui::Button::new(tr!("action-save-as"))
                                .shortcut_text(ctx.format_shortcut(&SHORTCUT_SAVE_AS)),
                        )
                        .clicked()
                    {
                        self.choose_save_path(actions);
                        ui.close();
                    }
                    ui.separator();
                    ui.menu_button(tr!("menu-language"), |ui| {
                        for language in [i18n::Language::English, i18n::Language::French] {
                            let current = i18n::language() == language;
                            if ui
                                .selectable_label(current, language.native_name())
                                .clicked()
                            {
                                self.choose_language(language);
                                ui.close();
                            }
                        }
                    });
                    ui.separator();
                    if ui.button(tr!("action-quit")).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        ui.close();
                    }
                });
                ui.menu_button(tr!("menu-edit"), |ui| {
                    // Le libellé complet (« Undo Edit Cells in Column A ») et le
                    // grisage sont deux lignes distinctes ici ; Qt les tenait
                    // ensemble dans une seule `QAction`.
                    let undo = self
                        .state
                        .undo_label()
                        .unwrap_or_else(|| tr!("history-undo"));
                    if ui
                        .add_enabled(
                            self.state.can_undo(),
                            egui::Button::new(undo)
                                .shortcut_text(ctx.format_shortcut(&SHORTCUT_UNDO)),
                        )
                        .clicked()
                    {
                        actions.push(Action::Undo);
                        ui.close();
                    }
                    let redo = self
                        .state
                        .redo_label()
                        .unwrap_or_else(|| tr!("history-redo"));
                    if ui
                        .add_enabled(
                            self.state.can_redo(),
                            egui::Button::new(redo)
                                .shortcut_text(ctx.format_shortcut(&SHORTCUT_REDO_Y)),
                        )
                        .clicked()
                    {
                        actions.push(Action::Redo);
                        ui.close();
                    }
                });
                if let Some(book) = self.active_book() {
                    if let Some(sheet) = self.state.selected_sheet_in(book) {
                        ui.menu_button(tr!("menu-workbook"), |ui| {
                            workbook_menu(ui, ctx, &self.state, book, sheet, actions);
                        });
                    }
                }
                if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
                    ui.menu_button(tr!("menu-graph"), |ui| {
                        let spec = crate::state::current_plot_spec(self.state.tree(), graph);
                        let target = self
                            .read_graph_view(graph, selected_manager_target)
                            .flatten();
                        ui.menu_button(tr!("menu-selected-content"), |ui| {
                            if let (Some(target), Some(spec)) = (target, spec.as_ref()) {
                                object_node_menu(
                                    ui,
                                    graph,
                                    target,
                                    spec,
                                    actions,
                                    &mut object_requests,
                                );
                            } else {
                                ui.add_enabled(false, egui::Button::new(tr!("action-properties")));
                                ui.add_enabled(
                                    false,
                                    egui::Button::new(tr!("action-rename-series")),
                                );
                                ui.add_enabled(
                                    false,
                                    egui::Button::new(tr!("action-delete-content")),
                                );
                            }
                        });
                        ui.separator();
                        for (label, hint, tab) in [
                            (
                                tr!("menu-graph-properties"),
                                tr!("menu-graph-properties", "hint"),
                                PlotSettingsTab::Graph,
                            ),
                            (
                                tr!("menu-graph-plot-area"),
                                tr!("menu-graph-plot-area", "hint"),
                                PlotSettingsTab::PlotArea,
                            ),
                            (
                                tr!("menu-graph-axes"),
                                tr!("menu-graph-axes", "hint"),
                                PlotSettingsTab::Axes,
                            ),
                            (
                                tr!("menu-graph-series"),
                                tr!("menu-graph-series", "hint"),
                                PlotSettingsTab::Series,
                            ),
                            (
                                tr!("menu-graph-legend"),
                                tr!("menu-graph-legend", "hint"),
                                PlotSettingsTab::Legend,
                            ),
                            (
                                tr!("menu-graph-theme"),
                                tr!("menu-graph-theme", "hint"),
                                PlotSettingsTab::Theme,
                            ),
                        ] {
                            if ui.button(label).on_hover_text(hint).clicked() {
                                actions.push(Action::OpenPlotSettings {
                                    graph,
                                    tab,
                                    axis: None,
                                });
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui
                            .button(tr!("action-reset-zoom"))
                            .on_hover_text(tr!("action-reset-zoom", "hint"))
                            .clicked()
                        {
                            if let Some(view) = self.graph_views.get_mut(&graph) {
                                view.reset_view();
                            }
                            ui.close();
                        }
                        let moved = crate::state::current_plot_spec(self.state.tree(), graph)
                            .is_some_and(|spec| spec.has_text_anchors());
                        if ui
                            .add_enabled(
                                moved,
                                egui::Button::new(tr!("action-reset-text-positions")),
                            )
                            .on_hover_text(tr!("action-reset-text-positions", "hint"))
                            .on_disabled_hover_text(tr!("action-reset-text-positions", "disabled"))
                            .clicked()
                        {
                            actions.push(Action::ResetTextAnchors { graph });
                            ui.close();
                        }
                        ui.separator();
                        // Textes libres (issue #35) : le pendant, dans la
                        // barre de menus, du clic droit et de Suppr.
                        let selected = self
                            .read_graph_view(graph, GraphView::selected_annotation)
                            .flatten();
                        if ui
                            .button(tr!("action-add-text"))
                            .on_hover_text(tr!("action-add-text", "hint"))
                            .clicked()
                        {
                            let tree = self.state.tree().clone();
                            if let Some(view) = self.graph_views.get_mut(&graph) {
                                if let Some(point) = view.panel_center_data() {
                                    add_annotation_at(ctx, view, &tree, graph, point);
                                }
                            }
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                selected.is_some(),
                                egui::Button::new(tr!("action-delete-text"))
                                    .shortcut_text(shortcut_text_delete()),
                            )
                            .clicked()
                        {
                            if let Some(target) = selected {
                                actions.push(Action::RemoveAnnotation { graph, target });
                            }
                            if let Some(view) = self.graph_views.get_mut(&graph) {
                                view.clear_annotation_selection();
                            }
                            ui.close();
                        }
                        ui.separator();
                        // Objets graphiques (issue #76) : l'ordre
                        // d'empilement et la suppression de l'objet
                        // sélectionné ; les outils de tracé sont dans la barre
                        // d'outils, comme l'outil Texte.
                        let object = self
                            .graph_views
                            .get(&graph)
                            .and_then(GraphView::selected_shape);
                        if let Some(view) = self.graph_views.get_mut(&graph) {
                            let tree = self.state.tree().clone();
                            match object {
                                Some(object) => {
                                    shape_order_buttons(ui, view, &tree, graph, object, actions)
                                }
                                None => disabled_shape_order_buttons(ui),
                            }
                        }
                        ui.separator();
                        if ui
                            .add(
                                egui::Button::new(tr!("action-export-graph"))
                                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_EXPORT_GRAPH)),
                            )
                            .on_hover_text(tr!("action-export-graph", "hint"))
                            .clicked()
                        {
                            self.request_graph_export(graph, actions);
                            ui.close();
                        }
                        ui.menu_button(tr!("menu-graph-png-resolution"), |ui| {
                            let current = self.state.png_export_scale();
                            for scale in crate::state::PNG_EXPORT_SCALES {
                                let (w, h) = DEFAULT_FIGURE_SIZE;
                                let (width, height) =
                                    ((w as f32 * scale) as u32, (h as f32 * scale) as u32);
                                let label = tr!(
                                    "menu-graph-png-resolution-option",
                                    scale = scale,
                                    width = width,
                                    height = height
                                );
                                if ui.radio(current == scale, label).clicked() {
                                    actions.push(Action::SetPngExportScale(scale));
                                    ui.close();
                                }
                            }
                        });
                    });
                }
                ui.menu_button(tr!("menu-view"), |ui| {
                    let mut open = self.state.object_manager_open();
                    if ui
                        .checkbox(&mut open, tr!("action-object-manager"))
                        .on_hover_text(tr!("action-object-manager", "hint"))
                        .changed()
                    {
                        actions.push(Action::SetObjectManagerOpen(open));
                    }
                    if open
                        && ui
                            .button(if self.object_manager_floating {
                                tr!("action-object-manager-dock")
                            } else {
                                tr!("action-object-manager-float")
                            })
                            .clicked()
                    {
                        self.object_manager_floating = !self.object_manager_floating;
                        ui.close();
                    }
                    let mut show_texts = self.state.object_manager_show_texts();
                    if ui
                        .checkbox(&mut show_texts, tr!("object-manager-show-texts"))
                        .on_hover_text(tr!("object-manager-show-texts", "hint"))
                        .changed()
                    {
                        actions.push(Action::SetObjectManagerShowTexts(show_texts));
                    }
                    let mut show_scales = self.state.object_manager_show_scales();
                    if ui
                        .checkbox(&mut show_scales, tr!("object-manager-show-scales"))
                        .on_hover_text(tr!("object-manager-show-scales", "hint"))
                        .changed()
                    {
                        actions.push(Action::SetObjectManagerShowScales(show_scales));
                    }
                    ui.separator();
                    for id in ToolbarId::ALL {
                        let mut visible = self.toolbars.layout().is_visible(id);
                        if ui
                            .checkbox(&mut visible, id.label())
                            .on_hover_text(id.hint())
                            .changed()
                        {
                            self.toolbars.set_visible(id, visible);
                        }
                    }
                });
                ui.menu_button(tr!("menu-window"), |ui| {
                    let mut show_metadata = metadata_row_visible(ctx);
                    if ui
                        .checkbox(&mut show_metadata, tr!("menu-window-show-metadata-row"))
                        .on_hover_text(tr!("menu-window-show-metadata-row", "hint"))
                        .changed()
                    {
                        set_metadata_row_visible(ctx, show_metadata);
                    }
                    ui.separator();
                    if let Some(tab) = self.maximized_tab {
                        if ui
                            .button(tr!(
                                "action-restore-window",
                                title = tab_title(&self.state, tab)
                            ))
                            .on_hover_text(tr!("action-restore-window", "hint"))
                            .clicked()
                        {
                            self.maximized_tab = None;
                            ui.close();
                        }
                    } else {
                        // La fenêtre flottante la plus récemment focalisée —
                        // dernier élément de `floating_tabs`, voir la doc de
                        // tête du module, § z-order. Le Project Manager n'y
                        // figure jamais, ce n'est pas une fenêtre flottante,
                        // donc rien de spécial à exclure ici.
                        let active = self.floating_tabs.last().copied();
                        let can_maximize = active.is_some();
                        if ui
                            .add_enabled(
                                can_maximize,
                                egui::Button::new(tr!("action-maximize-window")),
                            )
                            .on_hover_text(tr!("action-maximize-window", "hint"))
                            .clicked()
                        {
                            self.maximized_tab = active;
                            ui.close();
                        }
                    }
                    ui.separator();
                    if self.pm_detached {
                        if ui
                            .button(tr!("action-reattach-project-manager"))
                            .on_hover_text(tr!("action-reattach-project-manager", "hint"))
                            .clicked()
                        {
                            self.reattach_project_manager();
                            ui.close();
                        }
                    } else if ui
                        .button(tr!("action-detach-project-manager"))
                        .on_hover_text(tr!("action-detach-project-manager", "hint"))
                        .clicked()
                    {
                        self.detach_project_manager();
                        ui.close();
                    }
                    // Pas d'« Attacher à gauche/à droite » ici : le Project
                    // Manager est un panneau fixe, figé à gauche, depuis
                    // cette tâche (voir la doc de tête du module, § attacher
                    // à gauche ou à droite — le côté droit et le
                    // repositionnement par glisser sont un travail futur
                    // délibérément hors périmètre).
                });
            });
        });
        if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
            for request in object_requests {
                self.run_object_manager_request(ctx, graph, request, actions);
            }
        }
    }

    /// Demande le fichier à ouvrir, puis confie le chargement à
    /// [`AppState::dispatch`] par l'action habituelle.
    fn choose_open_path(&mut self, actions: &mut Vec<Action>) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Ouvrir un projet")
            .add_filter("Projet StokesForge", &["stf", "stokesforge"])
            .pick_file()
        {
            self.state.set_path(path.display().to_string());
            actions.push(Action::Load);
        }
    }

    /// Demande la destination de sauvegarde, puis confie l'écriture à
    /// [`AppState::dispatch`] par l'action habituelle.
    /// Applique la langue tout de suite (les textes se redessinent à l'image
    /// suivante) et l'enregistre dans `preferences.json`. Un enregistrement
    /// qui échoue se dit dans la barre d'état ; la langue reste appliquée.
    /// Reprend la disposition des barres d'outils des préférences de
    /// l'utilisateur. Séparé de [`StokesForgeApp::new`] pour que les tests ne lisent
    /// pas le fichier de la machine.
    pub fn load_toolbar_layout(&mut self, preferences: &Preferences) {
        let layout = preferences
            .toolbars()
            .map_or_else(ToolbarLayout::default, ToolbarLayout::from_json);
        self.toolbars = ToolbarState::new(layout);
    }

    /// Enregistre la disposition des barres dans `preferences.json` ; un échec
    /// se dit dans la barre d'état, la disposition reste appliquée.
    fn save_toolbar_layout(&mut self) {
        let layout = self.toolbars.layout().to_json();
        if let Err(err) = Preferences::load().save_toolbars(layout) {
            self.state
                .notice(tr!("status-toolbars-not-saved", detail = err.to_string()));
        }
    }

    /// Ce que la barre contextuelle montre pour la fenêtre au premier plan.
    fn tools_context(&self, active_tab: Option<Tab>) -> crate::toolbar::ToolsContext {
        use crate::toolbar::ToolsContext;
        match active_tab_kind(active_tab) {
            ActiveTabKind::Graph(graph) => ToolsContext::Graph {
                graph,
                tool: self
                    .read_graph_view(graph, GraphView::tool)
                    .unwrap_or_default(),
            },
            ActiveTabKind::Book(book) => {
                let sheet = self.state.selected_sheet_in(book);
                let columns = sheet
                    .and_then(|sheet| {
                        self.state
                            .selection(sheet)
                            .map(|selection| selection.columns(self.state.tree(), sheet))
                    })
                    .unwrap_or_default();
                ToolsContext::Book {
                    book,
                    sheet,
                    columns,
                }
            }
            _ => ToolsContext::None,
        }
    }

    /// Sur quoi la barre de format agit : un champ en édition sur place, sinon
    /// un texte libre sélectionné, sinon rien (issue #63).
    fn format_target(
        &self,
        ctx: &egui::Context,
        active_tab: Option<Tab>,
    ) -> crate::toolbar::FormatTarget {
        use crate::toolbar::FormatTarget;
        let Some(graph) = active_graph(active_tab_kind(active_tab)) else {
            return FormatTarget::None;
        };
        self.read_graph_view(graph, |view| {
            let editing = ctx.data(|data| {
                data.get_temp::<TextEditor>(text_editor_id(view)).is_some()
                    || data
                        .get_temp::<AnnotationEditor>(annotation_editor_id(view))
                        .is_some()
            });
            if editing {
                FormatTarget::Field
            } else if view.selected_annotation().is_some() {
                FormatTarget::Block
            } else if view.selected_text().is_some() {
                FormatTarget::Zone
            } else {
                FormatTarget::None
            }
        })
        .unwrap_or(FormatTarget::None)
    }

    /// Lit les réglages de la zone visée et les défauts du pipeline partagé.
    /// Le brouillon en édition prime sur le modèle, aussi en fenêtre détachée.
    fn text_format_state(
        &self,
        ctx: &egui::Context,
        active_tab: Option<Tab>,
    ) -> crate::toolbar::TextFormatState {
        use crate::toolbar::TextFormatState;
        let Some(graph) = active_graph(active_tab_kind(active_tab)) else {
            return TextFormatState::default();
        };
        let Some(spec) = crate::state::current_plot_spec(self.state.tree(), graph) else {
            return TextFormatState::default();
        };
        let state = self
            .read_graph_view(graph, |view| {
                let annotation_editor =
                    ctx.data(|data| data.get_temp::<AnnotationEditor>(annotation_editor_id(view)));
                let annotation = annotation_editor
                    .as_ref()
                    .map(|editor| &editor.draft)
                    .or_else(|| {
                        view.selected_annotation()
                            .and_then(|target| target.get(&spec))
                    });
                if let Some(annotation) = annotation {
                    return Some(
                        TextFormatState {
                            source: Some(egui::Id::new((
                                graph,
                                annotation_editor
                                    .as_ref()
                                    .map(|e| e.target)
                                    .or_else(|| view.selected_annotation()),
                                annotation_editor.is_some(),
                            ))),
                            family: Some(if annotation.font.trim().is_empty() {
                                stokesforge_render_egui::FontSet::default()
                                    .sans_family_name()
                                    .into()
                            } else {
                                annotation.font.trim().into()
                            }),
                            size: Some(annotation.effective_size()),
                            color: Some(annotation.color.clone()),
                            auto_family: annotation.font.trim().is_empty(),
                            auto_size: annotation.size
                                == stokesforge_plot::AnnotationSpec::DEFAULT_SIZE,
                        }
                        .with_markup(&annotation.text),
                    );
                }
                let editor = ctx.data(|data| data.get_temp::<TextEditor>(text_editor_id(view)));
                let target = editor
                    .as_ref()
                    .map(|editor| match editor.target {
                        TextEditTarget::Title(target) => target,
                        TextEditTarget::LegendEntry(_, (row, column)) => {
                            TextTarget::Legend { row, column }
                        }
                    })
                    .or_else(|| view.selected_text())?;
                let font = target.font(&spec)?;
                let default_size = match target {
                    TextTarget::Title => stokesforge_render_egui::TITLE_SIZE,
                    TextTarget::PanelTitle { .. } => stokesforge_render_egui::PANEL_TITLE_SIZE,
                    TextTarget::AxisTitle { .. } => stokesforge_render_egui::AXIS_DESC_SIZE,
                    TextTarget::Legend { .. } => stokesforge_render_egui::LABEL_SIZE,
                };
                let text = editor
                    .as_ref()
                    .map(|editor| editor.buffer.clone())
                    .or_else(|| target.text(&spec))
                    .unwrap_or_default();
                Some(
                    TextFormatState {
                        source: Some(egui::Id::new((
                            graph,
                            target,
                            editor.as_ref().map(|editor| editor.target),
                        ))),
                        family: Some(
                            font.family()
                                .unwrap_or(
                                    stokesforge_render_egui::FontSet::default().sans_family_name(),
                                )
                                .into(),
                        ),
                        size: Some(font.effective_size(default_size)),
                        color: target.color(&spec),
                        auto_family: font.family().is_none(),
                        auto_size: font.size.is_none(),
                    }
                    .with_markup(&text),
                )
            })
            .flatten();
        state.unwrap_or_default()
    }

    /// L'objet graphique sélectionné du graphe au premier plan, avec son
    /// adresse (issue #64).
    fn selected_object(
        &self,
        active_tab: Option<Tab>,
    ) -> Option<(
        NodeId,
        stokesforge_render_egui::ShapeRef,
        stokesforge_render_egui::ShapeSpec,
    )> {
        let graph = active_graph(active_tab_kind(active_tab))?;
        let target = self
            .read_graph_view(graph, GraphView::selected_shape)
            .flatten()?;
        let shape = crate::state::current_plot_spec(self.state.tree(), graph)
            .and_then(|spec| target.get(&spec).cloned())?;
        Some((graph, target, shape))
    }

    /// Ce que la barre de format d'objet montre.
    fn object_format_state(
        &self,
        active_tab: Option<Tab>,
    ) -> Option<crate::toolbar::ObjectFormatState> {
        let graph = active_graph(active_tab_kind(active_tab))?;
        if let Some(target) = self.selected_series(graph) {
            let spec = crate::state::current_plot_spec(self.state.tree(), graph)?;
            let series = spec.series_at(target)?;
            return Some(crate::toolbar::ObjectFormatState {
                closed: false,
                line_color: series.line.color.clone(),
                fill_color: String::new(),
                line_width: series.line.width,
            });
        }
        let (_, _, shape) = self.selected_object(active_tab)?;
        Some(crate::toolbar::ObjectFormatState {
            closed: shape.kind.is_closed(),
            line_color: shape.line_color,
            fill_color: shape.fill_color,
            line_width: shape.line_width,
        })
    }

    /// Règle l'objet sélectionné : une seule action annulable, comme un
    /// déplacement (`ObjectFormat::apply` dit si quelque chose a changé).
    fn format_object(
        &mut self,
        format: &crate::object_format::ObjectFormat,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
    ) {
        if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
            if let Some(target) = self.selected_series(graph) {
                if let Some(mut spec) = crate::state::current_plot_spec(self.state.tree(), graph) {
                    if spec
                        .series_at_mut(target)
                        .is_some_and(|series| format.apply_series(series))
                    {
                        actions.push(Action::SetPlotSpec { graph, spec });
                    }
                }
                return;
            }
        }
        let Some((graph, target, mut shape)) = self.selected_object(active_tab) else {
            return;
        };
        if format.apply(&mut shape) {
            actions.push(Action::SetShape {
                graph,
                target,
                shape: Box::new(shape),
            });
        }
    }

    /// Règle la police (ou le corps) de la **zone** de texte visée, sans
    /// toucher à son contenu. Rend `true` si la demande a été prise en charge ;
    /// `false` laisse [`StokesForgeApp::format_text`] poser des balises (le corps
    /// d'un passage du champ en édition, la couleur, le gras…).
    ///
    /// - un texte libre en édition : le brouillon de l'éditeur
    ///   ([`apply_pending_zone_font`]) ;
    /// - un titre ou un nom de légende en édition : le titre, ou la légende,
    ///   directement ;
    /// - un titre, un titre d'axe ou la légende sélectionné : idem, police et
    ///   corps ;
    /// - un texte libre sélectionné : sa police ;
    /// - la couleur d'un titre, d'un titre d'axe ou de la légende sélectionné :
    ///   celle de la zone entière (#90) ; en édition, un passage (balise).
    fn format_zone(
        &mut self,
        ctx: &egui::Context,
        format: &crate::text_format::TextFormat,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
    ) -> bool {
        use crate::text_format::TextFormat;
        let Some(graph) = active_graph(active_tab_kind(active_tab)) else {
            return false;
        };
        let Some((annotation_editing, title_editing, selected_text, selected_annotation)) = self
            .read_graph_view(graph, |view| {
                (
                    ctx.data(|data| {
                        data.get_temp::<AnnotationEditor>(annotation_editor_id(view))
                            .is_some()
                    }),
                    ctx.data(|data| data.get_temp::<TextEditor>(text_editor_id(view))),
                    view.selected_text(),
                    view.selected_annotation(),
                )
            })
        else {
            return false;
        };
        // Quelle zone de titre ? (`None` : pas de titre visé)
        let zone = match (&title_editing, selected_text) {
            (Some(editor), _) => Some(match editor.target {
                TextEditTarget::Title(target) => target,
                TextEditTarget::LegendEntry(_, (row, column)) => {
                    stokesforge_render_egui::TextTarget::Legend { row, column }
                }
            }),
            (None, selected) if !annotation_editing => selected,
            _ => None,
        };
        if matches!(format, TextFormat::AutoSize) {
            if annotation_editing {
                self.read_graph_view(graph, |view| {
                    let id = annotation_editor_id(view);
                    ctx.data_mut(|data| {
                        if let Some(mut editor) = data.get_temp::<AnnotationEditor>(id) {
                            editor.draft.size = stokesforge_plot::AnnotationSpec::DEFAULT_SIZE;
                            data.insert_temp(id, editor);
                        }
                    });
                });
                return true;
            }
            if let Some(target) = zone {
                return self.set_zone_font(graph, target, actions, |font| font.size = None);
            }
        }
        if let TextFormat::Font(name) = format {
            let name = name.trim().to_owned();
            if annotation_editing {
                ctx.data_mut(|data| data.insert_temp(pending_zone_font_id(), Some(name)));
                return true;
            }
            if let Some(target) = zone {
                return self.set_zone_font(graph, target, actions, |font| font.family = name);
            }
            if let Some(target) = selected_annotation {
                let Some(mut annotation) =
                    crate::state::current_plot_spec(self.state.tree(), graph)
                        .and_then(|spec| target.get(&spec).cloned())
                else {
                    return true;
                };
                annotation.font = name;
                actions.push(Action::SetAnnotation {
                    graph,
                    target,
                    annotation: Box::new(annotation),
                });
                return true;
            }
            return false;
        }
        // La couleur : celle de la zone d'un titre sélectionné ; dans un champ
        // en édition elle reste une balise sur le passage surligné (#90).
        if let (TextFormat::Color(color), Some(target), None, false) =
            (format, zone, &title_editing, annotation_editing)
        {
            actions.push(Action::SetTextColor {
                graph,
                target,
                color: color.clone(),
            });
            return true;
        }
        // Le corps : celui de la zone d'un titre sélectionné ; dans un champ
        // en édition il reste une balise sur le passage surligné.
        if let (TextFormat::Size(size), Some(target), None, false) =
            (format, zone, &title_editing, annotation_editing)
        {
            if size.is_finite() && *size > 0.0 {
                let size = *size;
                return self.set_zone_font(graph, target, actions, |font| font.size = Some(size));
            }
        }
        false
    }

    /// Pose la police ou le corps de la zone de titre `target` : lit la
    /// police actuelle dans le modèle, la change par `edit`, et pousse
    /// **une** action annulable. `true` toujours (la demande est prise en
    /// charge même si le graphe n'a plus ce texte).
    fn set_zone_font(
        &self,
        graph: NodeId,
        target: stokesforge_render_egui::TextTarget,
        actions: &mut Vec<Action>,
        edit: impl FnOnce(&mut stokesforge_plot::TextFont),
    ) -> bool {
        if let Some(mut font) = crate::state::current_plot_spec(self.state.tree(), graph)
            .and_then(|spec| target.font(&spec))
        {
            edit(&mut font);
            actions.push(Action::SetTextFont {
                graph,
                target,
                font,
            });
        }
        true
    }

    /// Met en forme le texte visé : dans le champ en édition (le champ pose
    /// les balises à son prochain tracé, voir [`apply_pending_format`]), ou
    /// sur tout le texte libre sélectionné — une seule action annulable.
    fn format_text(
        &mut self,
        ctx: &egui::Context,
        format: crate::text_format::TextFormat,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
    ) {
        use crate::text_format::{apply, TextFormat};
        // La police est un attribut de la zone, jamais une balise (issue
        // #78) ; le corps aussi pour un titre ou la légende (issue #77).
        if self.format_zone(ctx, &format, active_tab, actions) {
            return;
        }
        match self.format_target(ctx, active_tab) {
            crate::toolbar::FormatTarget::None | crate::toolbar::FormatTarget::Zone => {}
            crate::toolbar::FormatTarget::Field => {
                ctx.data_mut(|data| data.insert_temp(pending_format_id(), Some(format)));
            }
            crate::toolbar::FormatTarget::Block => {
                let Some(graph) = active_graph(active_tab_kind(active_tab)) else {
                    return;
                };
                let Some(target) = self
                    .read_graph_view(graph, GraphView::selected_annotation)
                    .flatten()
                else {
                    return;
                };
                let Some(mut annotation) =
                    crate::state::current_plot_spec(self.state.tree(), graph)
                        .and_then(|spec| target.get(&spec).cloned())
                else {
                    return;
                };
                // Couleur et corps d'un texte libre sont des champs du modèle.
                match &format {
                    TextFormat::Color(color) => annotation.color = color.clone(),
                    TextFormat::Size(size) => annotation.size = *size,
                    TextFormat::AutoSize => {
                        annotation.size = stokesforge_plot::AnnotationSpec::DEFAULT_SIZE
                    }
                    _ => {
                        let length = annotation.text.chars().count();
                        annotation.text = apply(&annotation.text, 0..length, &format).text;
                    }
                }
                actions.push(Action::SetAnnotation {
                    graph,
                    target,
                    annotation: Box::new(annotation),
                });
            }
        }
    }

    /// Exécute ce qu'un bouton de barre d'outils a demandé : les mêmes
    /// chemins que les entrées de menu correspondantes, jamais une seconde
    /// écriture.
    fn run_toolbar_request(
        &mut self,
        ctx: &egui::Context,
        request: ToolbarRequest,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
    ) {
        match request {
            ToolbarRequest::Action(action) => actions.push(*action),
            ToolbarRequest::SetGraphTool(tool) => {
                if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
                    self.with_graph_view(graph, |view| view.set_tool(tool));
                }
            }
            ToolbarRequest::ResetZoom => {
                if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
                    self.with_graph_view(graph, GraphView::reset_view);
                }
            }
            ToolbarRequest::FormatText(format) => {
                self.format_text(ctx, format, active_tab, actions);
            }
            ToolbarRequest::FormatObject(format) => {
                self.format_object(&format, active_tab, actions);
            }
            ToolbarRequest::Import => self.open_import_dialog(),
            ToolbarRequest::Open => self.choose_open_path(actions),
            ToolbarRequest::SaveAs => self.choose_save_path(actions),
            ToolbarRequest::ExportGraph => {
                if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
                    self.request_graph_export(graph, actions);
                }
            }
        }
    }

    fn choose_language(&mut self, language: i18n::Language) {
        i18n::set_language(language);
        if let Err(err) = Preferences::load().save_language(language.tag()) {
            self.state
                .notice(tr!("status-language-not-saved", detail = err.to_string()));
        }
    }

    fn choose_save_path(&mut self, actions: &mut Vec<Action>) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Enregistrer le projet")
            .add_filter("Projet StokesForge", &["stf", "stokesforge"])
            .set_file_name(suggested_file_name(self.state.path()))
            .save_file()
        {
            self.state.set_path(path.display().to_string());
            actions.push(Action::Save);
        }
    }

    /// Barre d'état : dernier message, révision.
    ///
    /// Les dimensions (lignes × colonnes) qu'elle affichait ont disparu avec
    /// « la » feuille sélectionnée globale : chaque onglet de classeur les
    /// montre pour sa propre feuille, dans [`book_contents`].
    fn status_bar(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::Panel::bottom("stokesforge_status")
            .resizable(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    match self.state.status() {
                        Some(status) if status.is_error => {
                            ui.colored_label(ui.visuals().error_fg_color, &status.text);
                        }
                        Some(status) => {
                            ui.label(&status.text);
                        }
                        None => {
                            ui.weak(tr!("status-ready"));
                        }
                    }
                    if self.state.can_reopen_last_import()
                        && self.state.import_state().is_none()
                        && ui.link(tr!("action-reopen-last-import")).clicked()
                    {
                        actions.push(Action::ReopenLastImport);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak(tr!(
                            "status-revision",
                            revision = self.state.snapshot().revision
                        ));
                    });
                });
            });
    }

    /// Peint la console de script dans son panneau fixe, ancré en bas — voir
    /// la doc de tête du module, § Console de script. Appelé par
    /// [`StokesForgeApp::ui`] *avant* [`StokesForgeApp::project_manager_panel`]
    /// (`egui::Panel::left`) : avec `egui::Panel`, le premier panneau ajouté
    /// est le plus extérieur, donc celui-ci, posé en premier, réserve sa
    /// bande sur toute la largeur de la fenêtre — y compris sous le Project
    /// Manager, pas seulement sous la zone du `CentralPanel` — plutôt que
    /// d'être resserré entre lui et le bord droit. `egui::Panel::bottom`
    /// n'est **pas** redimensionnable par défaut (à la différence de
    /// `left`/`right`) : `.resizable(true)` restaure le glisser du bord en
    /// hauteur. Contrairement au Project
    /// Manager, aucun mode détaché en fenêtre OS séparée ni bascule de
    /// position : la console de script a toujours été en bas, et le reste.
    ///
    /// Repliée (issue #79), la console se réduit à sa barre de titre — un
    /// panneau distinct, non redimensionnable, pour que la hauteur mémorisée
    /// du panneau déplié ne soit pas écrasée.
    fn script_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if actions
            .iter()
            .any(|action| matches!(action, Action::RunScript))
        {
            self.script_collapsed = false;
        }
        if self.script_collapsed {
            egui::Panel::bottom("stokesforge_script_collapsed")
                .resizable(false)
                .show(ui, |ui| {
                    if script_header(ui, &self.state, true) {
                        self.script_collapsed = false;
                    }
                });
            return;
        }
        egui::Panel::bottom("stokesforge_script")
            .resizable(true)
            .default_size(SCRIPT_PANEL_HEIGHT)
            .show(ui, |ui| {
                if script_header(ui, &self.state, false) {
                    self.script_collapsed = true;
                }
                ui.separator();
                script_contents(ui, ctx, &mut self.state, actions);
            });
    }

    /// Peint le Project Manager dans son panneau fixe, ancré à gauche — voir
    /// la doc de tête du module, § Project Manager. Appelé par
    /// [`StokesForgeApp::ui`] *avant* le `CentralPanel`, mais
    /// *après* [`StokesForgeApp::script_panel`] — voir sa doc de tête pour
    /// pourquoi cet ordre précis. Comme [`StokesForgeApp::menu_bar`]
    /// (`egui::Panel::top`) : peindre un panneau ancré avant le reste
    /// garantit par construction qu'il n'est jamais recouvert ni ne recouvre
    /// ce qui vient après. N'est pas appelé quand [`StokesForgeApp::pm_detached`]
    /// vaut `true` — cette image-là, rien ne remplace ce panneau, la vraie
    /// fenêtre OS séparée ([`StokesForgeApp::show_detached_project_manager`]) en
    /// tient lieu. Reste redimensionnable en largeur par glisser du bord
    /// (comportement par défaut d'`egui::Panel::left`) mais figé en
    /// position : pas de bascule vers la droite ni de repositionnement par
    /// glisser, voir la doc de tête du module, § attacher à gauche ou à
    /// droite.
    fn project_manager_panel(
        &mut self,
        ui: &mut egui::Ui,
        open_requests: &mut Vec<Tab>,
        close_requests: &mut Vec<Tab>,
        active_tab: Option<Tab>,
    ) {
        egui::Panel::left("stokesforge_pm")
            .default_size(PROJECT_MANAGER_WIDTH)
            .show(ui, |ui| {
                let tree = self.state.project_tree();
                let mut interaction = PmInteraction {
                    open_requests,
                    close_requests,
                    active_tab,
                    selection: &mut self.pm_selection,
                    hidden: &mut self.hidden_tabs,
                };
                project_manager_contents_from_tree(ui, &tree, &mut interaction);
            });
    }

    fn active_content_tab(&mut self, ctx: &egui::Context) -> Option<Tab> {
        if !ctx.input(|input| input.focused) {
            for (&tab, detached) in &self.detached_tabs {
                if detached.view_model.lock().is_ok_and(|model| model.focused) {
                    self.active_detached = Some(tab);
                }
            }
        }
        self.active_detached
            .filter(|tab| self.detached_tabs.contains_key(tab))
            .or(self.maximized_tab)
            .or_else(|| self.floating_tabs.last().copied())
    }

    /// Les barres et l'export lisent aussi la vue détachée ciblée par le panneau.
    fn read_graph_view<R>(&self, graph: NodeId, read: impl FnOnce(&GraphView) -> R) -> Option<R> {
        if let Some(detached) = self.detached_tabs.get(&Tab::Graph(graph)) {
            let model = detached
                .view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            model.graph_view.as_ref().map(read)
        } else {
            self.graph_views.get(&graph).map(read)
        }
    }

    fn request_graph_export(&mut self, graph: NodeId, actions: &mut Vec<Action>) {
        let tree = self.state.snapshot().tree.clone();
        self.with_graph_view(graph, |view| {
            request_graph_export(Some(view), &tree, graph, actions)
        });
    }

    /// Même vue de sélection pour les graphes flottants et détachés.
    fn with_graph_view<R>(
        &mut self,
        graph: NodeId,
        run: impl FnOnce(&mut GraphView) -> R,
    ) -> Option<R> {
        if let Some(detached) = self.detached_tabs.get(&Tab::Graph(graph)) {
            let mut model = detached
                .view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            Some(run(model.graph_view.get_or_insert_with(GraphView::default)))
        } else {
            Some(run(self.graph_views.entry(graph).or_default()))
        }
    }

    fn selected_series(&self, graph: NodeId) -> Option<stokesforge_plot::SeriesRef> {
        self.read_graph_view(graph, GraphView::selected_series)
            .flatten()
    }

    fn object_manager_panel(
        &mut self,
        ui: &mut egui::Ui,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
    ) {
        if !self.state.object_manager_open() {
            return;
        }
        let ctx = ui.ctx().clone();
        let graph = active_graph(active_tab_kind(active_tab));
        let tree = self.state.snapshot().tree.clone();
        let show_texts = self.state.object_manager_show_texts();
        let show_scales = self.state.object_manager_show_scales();
        let spec = graph.and_then(|graph| crate::state::current_plot_spec(&tree, graph));
        let model = graph.and_then(|graph| {
            crate::object_manager::object_tree_with_selection(
                &tree,
                graph,
                self.read_graph_view(graph, selected_manager_target)
                    .flatten(),
                show_texts,
                show_scales,
            )
        });
        let mut requests = Vec::new();
        let mut close = false;
        let mut toggle_dock = false;
        let mut contents = |ui: &mut egui::Ui| {
            ui.horizontal(|ui| {
                if !self.object_manager_floating {
                    ui.heading(tr!("action-object-manager"));
                }
                toggle_dock |= icon_button(
                    ui,
                    if self.object_manager_floating {
                        Icon::Reattach
                    } else {
                        Icon::Detach
                    },
                    IconRole::Neutral,
                    if self.object_manager_floating {
                        tr!("action-object-manager-dock")
                    } else {
                        tr!("action-object-manager-float")
                    },
                )
                .clicked();
                close |= icon_button(
                    ui,
                    Icon::Close,
                    IconRole::Neutral,
                    tr!("action-object-manager-close"),
                )
                .clicked();
            });
            ui.separator();
            requests = object_manager_contents(
                ui,
                &tree,
                model.as_ref(),
                spec.as_ref(),
                show_texts,
                show_scales,
                actions,
            );
        };
        if self.object_manager_floating {
            let viewport = ctx.input(|input| input.content_rect());
            let rect = self.object_manager_rect.unwrap_or_else(|| {
                egui::Rect::from_min_size(
                    viewport.min + egui::vec2(80.0, 80.0),
                    egui::vec2(330.0, 420.0),
                )
            });
            let id = egui::Id::new("stokesforge_object_manager_float");
            let mut header = WindowHeaderActions::default();
            let mut position = rect.min;
            let mut size = rect.size();
            show_window_in_layer(
                &ctx,
                id,
                egui::Order::Foreground,
                rect,
                viewport,
                false,
                |ui| {
                    header = window_header(
                        ui,
                        id,
                        &tr!("action-object-manager"),
                        None,
                        false,
                        HeaderPlacement::Floating,
                    );
                    contents(ui);
                },
                |ui, rect| floating_resize_handles(ui, id, rect, &mut position, &mut size),
            );
            close |= header.close;
            size = size.min(viewport.size());
            position += header.drag_delta;
            position.x = position.x.clamp(
                viewport.left(),
                (viewport.right() - size.x).max(viewport.left()),
            );
            position.y = position.y.clamp(
                viewport.top(),
                (viewport.bottom() - size.y).max(viewport.top()),
            );
            self.object_manager_rect = Some(egui::Rect::from_min_size(position, size));
        } else {
            egui::Panel::right("stokesforge_object_manager")
                .default_size(300.0)
                .size_range(180.0..=500.0)
                .show(ui, &mut contents);
        }
        if toggle_dock {
            self.object_manager_floating = !self.object_manager_floating;
        }
        if close {
            actions.push(Action::SetObjectManagerOpen(false));
        }
        if let Some(graph) = graph {
            for request in requests {
                self.run_object_manager_request(&ctx, graph, request, actions);
            }
        }
    }

    /// L'arbre et le menu Graph partagent les mêmes gestes de vue et Action.
    fn run_object_manager_request(
        &mut self,
        ctx: &egui::Context,
        graph: NodeId,
        request: ObjectManagerRequest,
        actions: &mut Vec<Action>,
    ) {
        use crate::object_manager::VisibilityTarget as T;
        let tree = self.state.tree().clone();
        let Some(spec) = crate::state::current_plot_spec(&tree, graph) else {
            return;
        };
        self.with_graph_view(graph, |view| match request {
            ObjectManagerRequest::Select(target) => {
                select_manager_target(view, Some(target), &spec)
            }
            ObjectManagerRequest::Rename(series) => {
                open_series_name_editor(ctx, view, &tree, graph, series)
            }
            ObjectManagerRequest::Properties(target) => {
                select_manager_target(view, Some(target), &spec);
                match target {
                    T::Annotation(t) => open_annotation_editor(ctx, view, &tree, graph, t),
                    T::Shape(t) => open_shape_properties(ctx, view, &spec, t),
                    _ => actions.push(Action::OpenObjectProperties { graph, target }),
                }
            }
        });
        ctx.request_repaint();
    }

    /// Dépose une capture de la fenêtre dans le fichier nommé par
    /// `STOKESFORGE_APP_SCREENSHOT`, puis n'en redemande plus.
    ///
    /// **Mécanisme de vérification, pas une fonctionnalité** — exactement
    /// comme l'auto-fermeture, et pour un besoin voisin apparu avec la
    /// phase R4 : sur un chantier de rendu, « la fenêtre s'ouvre et ne
    /// plante pas » ne suffit plus, il faut pouvoir *regarder* ce qu'elle
    /// peint. Les tests couvrent la géométrie (`stokesforge-render-egui`), pas
    /// l'apparence ; et une capture prise par la fenêtre elle-même est le
    /// seul moyen d'y accéder sous Wayland, où aucun outil externe ne peut
    /// photographier une fenêtre donnée.
    ///
    /// Le format est **PPM binaire** (`P6`), sept lignes de code et aucune
    /// dépendance nouvelle. Le convertir en PNG est l'affaire d'un
    /// `magick capture.ppm capture.png` ; faire entrer un encodeur PNG dans
    /// le banc pour un outil de vérification serait payer cher un confort.
    ///
    /// Une erreur d'écriture est signalée sur la sortie d'erreur et rien de
    /// plus : le banc ne doit pas s'arrêter parce qu'un outillage de
    /// vérification a échoué.
    fn screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else {
            return;
        };
        if self.screenshot_delay > 0 {
            self.screenshot_delay -= 1;
            ctx.request_repaint();
            return;
        }
        // Une capture arrive à l'image *suivant* la demande : tant qu'on n'a
        // rien reçu, on redemande.
        let image = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let Some(image) = image else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            ctx.request_repaint();
            return;
        };
        self.screenshot = None;

        let [width, height] = image.size;
        let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
        for pixel in &image.pixels {
            let [r, g, b, _] = pixel.to_srgba_unmultiplied();
            ppm.extend_from_slice(&[r, g, b]);
        }
        match std::fs::write(&path, ppm) {
            Ok(()) => eprintln!("[stokesforge-app] capture écrite : {}", path.display()),
            Err(err) => eprintln!(
                "[stokesforge-app] capture impossible dans {} : {err}",
                path.display()
            ),
        }
    }

    /// Ferme la fenêtre si le délai d'auto-fermeture est écoulé.
    fn autoquit(&mut self, ctx: &egui::Context) {
        let Some(delay) = self.autoquit else {
            return;
        };
        let start = *self.first_frame.get_or_insert_with(Instant::now);
        let elapsed = start.elapsed();
        if elapsed >= delay {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            // Sans événement d'entrée, egui ne redessine pas : il faut lui
            // demander de se réveiller, sinon l'échéance n'arrive jamais.
            ctx.request_repaint_after(delay - elapsed);
        }
    }
}

impl eframe::App for StokesForgeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // `Context` est un `Arc` : le cloner coûte un compteur, et évite
        // d'emprunter `ui` en lecture pendant qu'on le peint.
        let ctx = ui.ctx().clone();
        let published_stamp = snapshot_stamp(self.state.snapshot());
        let mut actions: Vec<Action> = Vec::new();
        // Onglets demandés par un clic dans le Project Manager (panneau fixe
        // ou détaché en fenêtre OS) : appliqués après le rendu, comme
        // `actions` — muter `floating_tabs` en plein parcours des fenêtres
        // flottantes l'invaliderait.
        let mut open_requests: Vec<Tab> = Vec::new();
        // Onglets dont l'en-tête de fenêtre flottante a demandé le
        // détachement cette image-ci — appliqués après le rendu, comme
        // `open_requests`.
        let mut detach_requests: Vec<Tab> = Vec::new();
        // Onglets dont la fenêtre flottante a été fermée (bouton natif de la
        // fenêtre) cette image-ci — appliqués après le rendu, même raison.
        let mut close_requests: Vec<Tab> = Vec::new();
        let mut geometry_updates: Vec<(Tab, egui::Rect)> = Vec::new();
        // L'onglet au premier plan — source de vérité pour le double-clic du
        // Project Manager (§4) et le surlignage de la fenêtre active (§5) :
        // calculé une fois ici, avant même le panneau fixe du Project
        // Manager qui en a besoin, plutôt qu'à l'intérieur du `CentralPanel`
        // (trop tard pour lui).
        let active_tab = self.active_content_tab(&ctx);

        // Glisser-déposer de fichiers : les chemins déposés cette image sont
        // posés dans un emplacement que le classeur sous le curseur peut
        // réclamer en se peignant (`book_contents`), puis vidés plus bas.
        begin_file_drop(&ctx);

        // Sonde un script terminé sur son fil détaché **avant** de peindre :
        // sans notification poussée de l'acteur, c'est le seul endroit qui
        // rafraîchit l'instantané après un script (voir la doc de tête de
        // `crate::state`). Ne bloque jamais, même sans script en cours.
        self.state.poll_script();
        // Import en fond : avancement, et création des feuilles à la fin. Les
        // classeurs créés s'ouvrent comme après un import immédiat.
        if self.state.import_progress().is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
            let root = self.state.tree().root();
            let before = self
                .state
                .tree()
                .node(root)
                .map_or(0, |node| node.children().len());
            if self.state.poll_import() {
                self.open_new_workbooks_after(before);
            }
        }
        let imported = self.state.take_imported_paths();
        if !imported.is_empty() {
            remember_recent_imports(&imported);
        }

        publish_saved_colors(&ctx, &self.state);
        self.collect_shortcuts(&ctx, &mut actions);
        self.menu_bar(ui, &ctx, &mut actions, active_tab);
        let bar_env = BarEnv {
            new_project_enabled: self.state.can_new_project(),
            export_enabled: active_graph(active_tab_kind(active_tab)).is_some(),
            object_manager_open: self.state.object_manager_open(),
            tools: self.tools_context(active_tab),
            text_format: self.format_target(&ctx, active_tab),
            text_style: self.text_format_state(&ctx, active_tab),
            object_format: self.object_format_state(active_tab),
            shortcuts: crate::toolbar::ShortcutTexts {
                new_project: ctx.format_shortcut(&SHORTCUT_NEW_PROJECT),
                open: ctx.format_shortcut(&SHORTCUT_OPEN),
                save: ctx.format_shortcut(&SHORTCUT_SAVE),
                save_as: ctx.format_shortcut(&SHORTCUT_SAVE_AS),
                import: SHORTCUT_IMPORT_TEXT.to_string(),
                export: ctx.format_shortcut(&SHORTCUT_EXPORT_GRAPH),
            },
        };
        let mut bar_requests = Vec::new();
        self.toolbars.show(ui, &bar_env, &mut bar_requests);
        for request in bar_requests {
            self.run_toolbar_request(&ctx, request, active_tab, &mut actions);
        }
        if self.toolbars.take_changed() {
            self.save_toolbar_layout();
        }
        self.status_bar(ui, &mut actions);
        // Panneau fixe, peint *avant* `StokesForgeApp::project_manager_panel` —
        // voir la doc de tête du module, § Console de script, et la doc de
        // tête de `StokesForgeApp::script_panel` : c'est cet ordre précis qui lui
        // garantit de courir sur toute la largeur de la fenêtre, y compris
        // sous le Project Manager, plutôt que d'être resserré entre lui et
        // le bord droit.
        self.script_panel(ui, &ctx, &mut actions);

        if self.pm_detached {
            self.show_detached_project_manager(&ctx, active_tab);
            // Vidange ce que la fenêtre OS détachée a écrit depuis la
            // dernière image (voir la doc de tête de
            // `show_detached_project_manager`) — même moment que le reste de
            // `open_requests`, avant de peindre le panneau fixe (qui ne sera
            // de toute façon pas peint cette image-ci) et le `CentralPanel`.
            let (should_reattach, new_selection, new_hidden) = {
                let mut requests = self
                    .pm_requests
                    .lock()
                    .unwrap_or_else(|err| err.into_inner());
                open_requests.append(&mut requests.open);
                close_requests.append(&mut requests.close);
                actions.append(&mut requests.actions);
                (
                    std::mem::take(&mut requests.reattach),
                    requests.selection.take(),
                    requests.hidden.take(),
                )
            };
            if let Some(selection) = new_selection {
                self.pm_selection = selection;
            }
            if let Some(hidden) = new_hidden {
                self.hidden_tabs = hidden;
            }
            if should_reattach {
                self.reattach_project_manager();
            }
        } else {
            // Panneau fixe, peint *avant* le `CentralPanel` — voir la doc de
            // tête du module, § Project Manager : c'est ce qui garantit par
            // construction qu'il n'est jamais recouvert par lui ni ne le
            // recouvre, comme la barre de menu (`egui::Panel::top`) peinte
            // plus haut.
            self.project_manager_panel(ui, &mut open_requests, &mut close_requests, active_tab);
        }

        self.object_manager_panel(ui, active_tab, &mut actions);

        if !self.detached_tabs.is_empty() {
            self.show_detached_tabs(&ctx);
            // Même vidange que pour le Project Manager, généralisée à
            // plusieurs onglets : chacun peut avoir accumulé des actions et/ou
            // demandé à être réattaché depuis la dernière image.
            let mut to_reattach: Vec<Tab> = Vec::new();
            let mut to_close: Vec<Tab> = Vec::new();
            for (tab, detached) in &self.detached_tabs {
                if let Tab::Book(book) = tab {
                    let mut model = detached
                        .view_model
                        .lock()
                        .unwrap_or_else(|err| err.into_inner());
                    if let Some(view_state) = model.view_state.as_mut() {
                        self.state.adopt_detached_book_view(view_state, *book);
                    }
                }
                let mut requests = detached
                    .requests
                    .lock()
                    .unwrap_or_else(|err| err.into_inner());
                // Pendant la fenêtre modale des propriétés du graphe, une
                // macro d'historique est ouverte par la racine : y laisser
                // entrer les actions d'une fenêtre détachée les ferait
                // absorber, puis annuler avec le graphe (issue #21). Elles
                // sont refusées, avec un message.
                if self.state.plot_settings().is_some() {
                    if !requests.actions.is_empty() {
                        requests.actions.clear();
                        self.state.reject_detached_actions();
                    }
                } else {
                    actions.append(&mut requests.actions);
                }
                if std::mem::take(&mut requests.reattach) {
                    to_reattach.push(*tab);
                }
                if std::mem::take(&mut requests.close) {
                    to_close.push(*tab);
                }
            }
            for tab in to_reattach {
                self.reattach_tab(tab);
            }
            for tab in to_close {
                self.close_detached_tab(tab);
            }
        }

        self.show_workspace(
            ui,
            active_tab,
            &mut actions,
            &mut detach_requests,
            &mut close_requests,
            &mut geometry_updates,
        );

        for (tab, rect) in geometry_updates {
            self.remember_window_geometry(tab, rect);
        }

        // Fenêtre d'import (`Action::ImportFiles`, voir la doc de tête de
        // [`import_window`]) : un dialogue transitoire, peint tant que
        // `self.state.import_state()` est `Some`, plutôt qu'une fenêtre
        // flottante de plus — après le `CentralPanel` mais avant la vidange
        // des requêtes ci-dessous, pour que l'`Action::ImportFiles` qu'elle
        // pousse éventuellement soit dispatchée cette image-ci comme toute
        // autre action.
        import_window(&ctx, &mut self.state, &mut actions);
        // Fenêtre des propriétés d'un graphe : même famille de dialogue
        // transitoire que la fenêtre d'import, peinte tant que
        // `self.state.plot_settings()` est `Some`.
        plot_settings_window(&ctx, &mut self.state);
        column_metadata_window(&ctx, &self.state, &mut actions);
        if let Some(action) = finish_file_drop(&ctx) {
            actions.push(action);
        }
        for project in self.state.take_projects_to_open() {
            open_project_in_new_window(&project);
        }

        // Les onglets demandés (Project Manager, panneau fixe ou détaché),
        // les détachements et fermetures demandés par l'en-tête d'une fenêtre
        // flottante, et les actions récoltées par tous les panneaux et
        // onglets ne sont appliqués qu'une fois l'image peinte : muter
        // `floating_tabs` en plein parcours des fenêtres flottantes
        // l'invaliderait.
        for tab in open_requests {
            self.open_tab(tab);
        }
        let detached_this_frame = !detach_requests.is_empty();
        for tab in detach_requests {
            self.detach_tab(tab);
        }
        if detached_this_frame {
            // `show_detached_tabs` a déjà publié les viewports de cette
            // image avant que l'en-tête ne demande le détachement. Sans une
            // seconde image de la racine, eframe peut donc rester au repos
            // sans jamais créer le viewport différé et appeler son callback.
            ctx.request_repaint();
        }
        self.close_floating_tabs(close_requests);
        if self.state.new_project_pending() {
            // Seule la confirmation peut agir sur cette image : clavier,
            // dépôts et requêtes des viewports détachés sont aussi bloqués.
            actions.clear();
            let label = tr!("new-project-discard");
            let hint = tr!("new-project-discard", "hint");
            match modal_dialog_with(
                &ctx,
                egui::Id::new("stokesforge_new_project"),
                &tr!("action-new-project"),
                egui::vec2(480.0, 160.0),
                DialogFooter {
                    accept_label: &label,
                    accept_hint: &hint,
                    accept_enabled: self.state.can_new_project(),
                },
                |_| {},
                |ui| {
                    ui.label(tr!("new-project-confirm"));
                },
            ) {
                DialogOutcome::Accept => actions.push(Action::ConfirmNewProject(true)),
                DialogOutcome::Cancel => actions.push(Action::ConfirmNewProject(false)),
                DialogOutcome::Open => {}
            }
        }
        if let Some(sheet) = self.state.pending_sheet_delete() {
            actions.clear();
            let name = self
                .state
                .tree()
                .node(sheet)
                .map(|node| node.name().to_string())
                .unwrap_or_default();
            let users = self.state.sheet_users(sheet);
            let label = tr!("sheet-delete-accept");
            let hint = tr!("sheet-delete-accept", "hint");
            match modal_dialog_with(
                &ctx,
                egui::Id::new("stokesforge_delete_sheet"),
                &tr!("sheet-delete-title"),
                egui::vec2(480.0, 170.0),
                DialogFooter {
                    accept_label: &label,
                    accept_hint: &hint,
                    accept_enabled: true,
                },
                |_| {},
                |ui| {
                    ui.label(tr!("sheet-delete-confirm", name = name.as_str()));
                    if !users.is_empty() {
                        ui.add_space(6.0);
                        ui.label(tr!("sheet-delete-used", graphs = users.join(", ")));
                    }
                },
            ) {
                DialogOutcome::Accept => actions.push(Action::ConfirmDeleteSheet(true)),
                DialogOutcome::Cancel => actions.push(Action::ConfirmDeleteSheet(false)),
                DialogOutcome::Open => {}
            }
        }
        for action in actions {
            let action = if matches!(action, Action::Save) && self.state.path().is_empty() {
                let mut chosen = Vec::new();
                self.choose_save_path(&mut chosen);
                let Some(action) = chosen.pop() else { continue };
                action
            } else {
                action
            };
            let action = if let Action::NewProject { pending_edits } = action {
                // Une saisie dans un viewport détaché, ou un texte du graphe,
                // n'est pas encore dans l'instantané d'AppState. La signaler
                // pour demander confirmation même si le modèle est enregistré.
                let graph_editing = |view: &GraphView| {
                    ctx.data(|data| {
                        data.get_temp::<TextEditor>(text_editor_id(view)).is_some()
                            || data
                                .get_temp::<AnnotationEditor>(annotation_editor_id(view))
                                .is_some()
                    })
                };
                let pending_edits = pending_edits
                    || self.graph_views.values().any(&graph_editing)
                    || self.detached_tabs.values().any(|detached| {
                        detached.view_model.lock().map_or(true, |model| {
                            model.view_state.as_ref().is_some_and(|state| {
                                state.editing().is_some()
                                    || state.header_editing().is_some()
                                    || state.note_editing().is_some()
                            }) || model.graph_view.as_ref().is_some_and(&graph_editing)
                        })
                    });
                Action::NewProject { pending_edits }
            } else {
                action
            };
            // `Action::PlotColumns` doit ouvrir le graphe qu'elle crée
            // (chantier « tracer depuis un classeur »). `dispatch` ne rend
            // jamais l'identifiant créé — même contrainte que l'import
            // (`AppState::import_files`) — donc le dossier parent se
            // calcule **avant** l'envoi de la commande, et le graphe se
            // retrouve après coup comme le dernier enfant de ce dossier :
            // `Project::add_graph` l'y insère en dernière position, et
            // `AppState::dispatch` a déjà rafraîchi `self.state`
            // (`AppState::refresh`) au retour de cet appel.
            let plotted_from = if matches!(action, Action::AddGraph) {
                Some(self.state.tree().root())
            } else if let Action::PlotColumns { sheet, .. } = &action {
                self.state
                    .tree()
                    .node(*sheet)
                    .and_then(Node::parent)
                    .and_then(|book| self.state.tree().node(book).and_then(Node::parent))
            } else {
                None
            };
            let creates_workbook = matches!(action, Action::AddWorkbook);
            let new_import_books_after = if matches!(
                action,
                Action::ImportFiles {
                    destination: ImportDestination::NewBookPerFile
                        | ImportDestination::NewBookForAll,
                    ..
                } | Action::DropFiles {
                    target_book: None,
                    ..
                }
            ) {
                let root = self.state.tree().root();
                self.state
                    .tree()
                    .node(root)
                    .map(|node| node.children().len())
            } else {
                None
            };
            let revision_before = self.state.revision();
            let epoch_before = self.state.snapshot().epoch;
            let replaces_project = matches!(
                action,
                Action::NewProject { .. } | Action::ConfirmNewProject(true)
            );
            let history_selections: Vec<_> = if matches!(action, Action::Undo | Action::Redo) {
                let graphs =
                    self.graph_views
                        .keys()
                        .copied()
                        .chain(self.detached_tabs.keys().filter_map(|tab| match tab {
                            Tab::Graph(graph) => Some(*graph),
                            _ => None,
                        }));
                graphs
                    .filter_map(|graph| {
                        let target = self
                            .read_graph_view(graph, selected_manager_target)
                            .flatten()?;
                        let previous = crate::state::current_plot_spec(self.state.tree(), graph)?;
                        Some((graph, target, previous))
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let selection_change = match &action {
                Action::ReorderSeries {
                    graph,
                    source,
                    index,
                } => Some((*graph, Some((*source, *index)), None)),
                Action::RemovePlotObject { graph, target } => Some((*graph, None, Some(*target))),
                Action::RemoveAnnotation { graph, target } => Some((
                    *graph,
                    None,
                    Some(crate::object_manager::VisibilityTarget::Annotation(*target)),
                )),
                Action::RemoveShape { graph, target } => Some((
                    *graph,
                    None,
                    Some(crate::object_manager::VisibilityTarget::Shape(*target)),
                )),
                _ => None,
            };
            self.state.dispatch(action);
            if self.state.revision() != revision_before {
                if let Some((graph, moved, removed)) = selection_change {
                    if let Some(spec) = crate::state::current_plot_spec(self.state.tree(), graph) {
                        self.with_graph_view(graph, |view| {
                            if let Some((source, index)) = moved {
                                if let Some(selected) =
                                    view.selected_series().filter(|s| s.panel == source.panel)
                                {
                                    let next = if selected.index == source.index {
                                        index
                                    } else if source.index < selected.index
                                        && selected.index <= index
                                    {
                                        selected.index - 1
                                    } else if index <= selected.index
                                        && selected.index < source.index
                                    {
                                        selected.index + 1
                                    } else {
                                        selected.index
                                    };
                                    view.select_series(
                                        Some(stokesforge_plot::SeriesRef::new(source.panel, next)),
                                        &spec,
                                    );
                                }
                            }
                            if let Some(removed) = removed {
                                let next = crate::object_manager::selection_after_remove(
                                    selected_manager_target(view),
                                    removed,
                                );
                                select_manager_target(view, next, &spec);
                            }
                        });
                    }
                }
            }
            if self.state.revision() != revision_before {
                for (graph, target, previous) in history_selections {
                    if let Some(next) = crate::state::current_plot_spec(self.state.tree(), graph) {
                        let target = crate::object_manager::selection_after_history(
                            &previous,
                            &next,
                            Some(target),
                        );
                        self.with_graph_view(graph, |view| {
                            select_manager_target(view, target, &next)
                        });
                    }
                }
            }
            if replaces_project && self.state.snapshot().epoch != epoch_before {
                // Les NodeId peuvent être réemployés : oublier toutes les
                // vues et géométries de l'ancienne identité avant de peindre.
                for tab in self.detached_tabs.keys() {
                    ctx.send_viewport_cmd_to(
                        egui::ViewportId::from_hash_of(tab),
                        egui::ViewportCommand::Close,
                    );
                }
                self.detached_tabs.clear();
                self.floating_tabs.clear();
                self.cascade_rank.clear();
                self.next_cascade = 0;
                self.focus_next = None;
                self.graph_views.clear();
                self.active_detached = None;
                self.collapsed_tabs.clear();
                self.maximized_tab = None;
                self.window_positions.clear();
                self.window_sizes.clear();
                self.workspace_scroll = egui::Vec2::ZERO;
                self.saved_window_geometries.clear();
                self.floating_rects.clear();
                self.pm_selection.clear();
                self.pm_requests = Arc::new(Mutex::new(DetachedProjectManagerRequests::default()));
                self.hidden_tabs.clear();
                ctx.memory_mut(|memory| {
                    // Les éditeurs sur place et formats différés ne doivent
                    // pas survivre à la fenêtre qu'ils visaient. Garder les
                    // préférences et dimensions des panneaux de l'application.
                    memory.data.remove_by_type::<TextEditor>();
                    memory.data.remove_by_type::<AnnotationEditor>();
                    memory
                        .data
                        .remove::<Option<crate::text_format::TextFormat>>(pending_format_id());
                    memory.data.remove::<Option<String>>(pending_zone_font_id());
                    memory.data.remove_by_type::<egui::scroll_area::State>();
                    if let Some(focused) = memory.focused() {
                        memory.surrender_focus(focused);
                    }
                });
                ctx.request_repaint();
            }
            if self.state.new_project_pending() {
                ctx.request_repaint();
                break;
            }
            if creates_workbook && self.state.revision() != revision_before {
                let root = self.state.tree().root();
                if let Some(book) = self
                    .state
                    .tree()
                    .node(root)
                    .and_then(|node| node.children().last().copied())
                    .filter(|id| self.state.worksheets_of(*id).len() == 1)
                {
                    self.open_tab(Tab::Book(book));
                }
            }
            if let Some(before) = new_import_books_after {
                self.open_new_workbooks_after(before);
            }
            if let Some(parent) = plotted_from {
                // `self.state.revision()` n'a bougé que si `AddGraph` a
                // effectivement été envoyée (voir `AppState::plot_columns`) :
                // sans ce garde, une sélection sans Y (no-op côté modèle)
                // rouvrirait à tort le dernier enfant déjà présent dans
                // `parent`, qui n'a rien à voir avec ce clic.
                if self.state.revision() != revision_before {
                    if let Some(graph) = self
                        .state
                        .tree()
                        .node(parent)
                        .and_then(|node| node.children().last().copied())
                    {
                        self.open_tab(Tab::Graph(graph));
                    }
                }
            }
        }

        self.sync_window_views();
        if snapshot_stamp(self.state.snapshot()) != published_stamp {
            // Les commandes sont appliquées après la publication des viewports.
            // Garantir l'image qui leur transmettra ce nouvel instantané.
            ctx.request_repaint();
        }
        self.screenshot(&ctx);
        self.autoquit(&ctx);
    }
}

/// Les deux compteurs ne suffisent pas après un remplacement de projet :
/// l'époque appartient aussi à l'identité de l'instantané publié.
fn snapshot_stamp(snapshot: &Snapshot) -> (u64, u64, u64) {
    (
        snapshot.epoch,
        snapshot.revision,
        snapshot.geometry_revision,
    )
}

/// Le nom de fichier suggéré par défaut au sélecteur « Enregistrer sous… » :
/// le nom du chemin courant s'il en a un, sinon un nom de repli.
fn suggested_file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "demo.stokesforge".to_string())
}

/// Titre d'un onglet : le nom du nœud qu'il porte, ou un texte « supprimé »
/// explicite s'il a disparu de l'arbre entre-temps (fermeture depuis un
/// autre onglet, ou par un script) — un onglet resté ouvert sur un nœud
/// disparu ne doit jamais paniquer ni disparaître silencieusement
/// (invariant 10).
fn tab_title(state: &AppState, tab: Tab) -> String {
    match tab {
        Tab::Book(id) => node_title(state, id, || tr!("window-kind-workbook")),
        Tab::Graph(id) => node_title(state, id, || tr!("window-kind-graph")),
        Tab::Matrix(id) => node_title(state, id, || tr!("window-kind-matrix")),
        Tab::Note(id) => node_title(state, id, || tr!("window-kind-note")),
    }
}

/// [`tab_title`], à partir d'un `Tree` déjà en main plutôt que d'un
/// `&AppState` vivant — pour l'en-tête peint par une fenêtre détachée, qui
/// n'a que le [`Snapshot`] resynchronisé dans `DetachedTabViewModel`
/// (voir la doc de tête du module, § « la lecture du modèle est
/// factorisée »).
fn tab_title_from_tree(tree: &Tree, tab: Tab) -> String {
    match tab {
        Tab::Book(id) => node_title_in(tree, id, || tr!("window-kind-workbook")),
        Tab::Graph(id) => node_title_in(tree, id, || tr!("window-kind-graph")),
        Tab::Matrix(id) => node_title_in(tree, id, || tr!("window-kind-matrix")),
        Tab::Note(id) => node_title_in(tree, id, || tr!("window-kind-note")),
    }
}

/// Traduit le `Tab` (privé à ce module) que voit `StokesForgeApp::ui` vers
/// [`ActiveTabKind`], le miroir public que `crate::state` peut lire sans
/// connaître `Tab` — seul point de contact entre les deux, voir la doc de
/// tête d'[`ActiveTabKind`].
fn active_tab_kind(tab: Option<Tab>) -> ActiveTabKind {
    match tab {
        Some(Tab::Book(id)) => ActiveTabKind::Book(id),
        Some(Tab::Graph(id)) => ActiveTabKind::Graph(id),
        Some(Tab::Matrix(id)) => ActiveTabKind::Matrix(id),
        Some(Tab::Note(id)) => ActiveTabKind::Note(id),
        None => ActiveTabKind::None,
    }
}

/// Le nom du nœud `id`, ou `"{missing} {id} (deleted)"` s'il n'existe plus.
fn node_title(state: &AppState, id: NodeId, missing: impl FnOnce() -> String) -> String {
    state
        .tree()
        .node(id)
        .map(|node| node.name().to_string())
        .unwrap_or_else(|| tr!("window-deleted", kind = missing(), id = id.to_string()))
}

/// Peint l'icône « prise » du glisser-déposer de colonnes ([`grid_contents`])
/// à côté du curseur — une petite grille de points plutôt qu'un glyphe de
/// police : la mesure de texte de l'appli passe entièrement par
/// `stokesforge_render::text` pour le rendu de graphe (voir `CLAUDE.md`, § Pièges
/// connus), et rien ne garantit qu'un glyphe « poignée » soit seulement
/// présent dans la police par défaut d'egui — un pictogramme dessiné à la
/// main évite la question.
fn paint_drag_handle_icon(painter: &egui::Painter, at: egui::Pos2, color: egui::Color32) {
    const DOT_RADIUS: f32 = 1.6;
    const SPACING: f32 = 5.0;
    for row in 0..3 {
        for col in 0..2 {
            let offset = egui::vec2(col as f32 * SPACING, row as f32 * SPACING);
            painter.circle_filled(at + offset, DOT_RADIUS, color);
        }
    }
}

/// Les couleurs enregistrées, publiées chaque image par la racine pour que
/// le sélecteur de couleur de l'éditeur sur place les propose aussi depuis
/// une fenêtre détachée (qui n'a pas `AppState`).
pub(crate) fn saved_colors_id() -> egui::Id {
    egui::Id::new("stokesforge_saved_colors_for_graphs")
}

fn color_request_action(request: crate::color_picker::SavedColorRequest) -> Action {
    match request.edit {
        SavedColorEdit::Save => Action::SaveColor {
            usage: request.usage,
            color: request.color,
            target: request.target,
        },
        SavedColorEdit::Remove => Action::RemoveSavedColor {
            usage: request.usage,
            color: request.color,
            target: request.target,
        },
    }
}

fn publish_saved_colors(ctx: &egui::Context, state: &AppState) {
    let colors = (
        state.saved_colors(SaveTarget::Project),
        state.saved_colors(SaveTarget::User),
    );
    ctx.data_mut(|data| data.insert_temp(saved_colors_id(), colors));
}

#[cfg(test)]
mod graph_layout_tests {
    use super::graph_window::graph_drop_target;
    use super::grid::role_label;
    use super::project_manager::tests::{click_at, find_text_center};
    use super::window_chrome::tests::find_icon_centers;
    use super::window_chrome::{WINDOW_CONTENT_MARGIN, WINDOW_HEADER_HEIGHT};
    use super::window_placement::GRAPH_WINDOW_SIZE;
    use super::*;
    use crate::theme;
    use stokesforge_core::{Column, ColumnRole, Project};
    use stokesforge_plot::{PanelSpec, PlotSpec, SeriesSpec};
    use stokesforge_session::Session;

    fn graphe() -> (Session, NodeId) {
        let mut project = Project::new("Découpage").unwrap();
        let root = project.root();
        let book = project.add_empty_workbook(root, None).unwrap();
        let sheet = project.add_worksheet(book, None).unwrap();
        let x = project
            .add_column(sheet, None, None, Column::from_f64(&[0.0, 1.0]))
            .unwrap();
        let y = project
            .add_column(sheet, None, None, Column::from_f64(&[0.0, 1.0]))
            .unwrap();
        let mut panel = PanelSpec::at(0, 0);
        panel.series.push(SeriesSpec::new_xy(x, y));
        let graph = project
            .add_graph(root, None, PlotSpec::empty("Figure").with_panels(panel))
            .unwrap();
        (Session::spawn(project, None).unwrap(), graph)
    }

    fn vue_persistee(session: &Session, node: NodeId) -> WindowGeometry {
        *session
            .handle()
            .snapshot()
            .unwrap()
            .window_geometries
            .get(&node)
            .expect("géométrie envoyée à la session")
    }

    /// #97 : les champs de la barre affichent le format de la zone visée.
    #[test]
    fn la_barre_affiche_la_police_et_le_corps_du_titre_selectionne() {
        use eframe::App as _;
        let (session, graph) = graphe();
        let mut state = AppState::new(session.handle(), String::new()).unwrap();
        state.dispatch(Action::SetTextFont {
            graph,
            target: TextTarget::Title,
            font: stokesforge_plot::TextFont {
                family: "Fira Sans".into(),
                size: Some(28.0),
            },
        });
        let mut app = StokesForgeApp::new(state);
        let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
        app.open_tab(Tab::Graph(graph));
        app.with_graph_view(graph, |view| {
            view.select_text(Some(TextTarget::Title), &spec)
        });
        let ctx = egui::Context::default();
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let mut frame = eframe::Frame::_new_kittest();
        let mut output = egui::FullOutput::default();
        for _ in 0..3 {
            output.textures_delta.clear();
            output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut frame),
            );
        }
        output.textures_delta.clear();
        find_text_center(&output.shapes, "Fira Sans");
        find_text_center(&output.shapes, "28");
        session.shutdown().unwrap();
    }

    /// #99 : le même choix dans le sélecteur agit sur deux zones successives.
    #[test]
    fn la_meme_couleur_s_applique_a_deux_zones_successives_par_la_barre() {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let ctx = egui::Context::default();
        theme::install(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        egui_extras::install_image_loaders(&ctx);
        let paint = |app: &mut StokesForgeApp, ctx: &egui::Context, events| {
            let env = BarEnv {
                text_format: app.format_target(ctx, Some(Tab::Graph(graph))),
                text_style: app.text_format_state(ctx, Some(Tab::Graph(graph))),
                ..Default::default()
            };
            let mut requests = Vec::new();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.horizontal(|ui| {
                        crate::toolbar::bar_contents(ui, ToolbarId::Text, &env, &mut requests)
                    });
                },
            );
            output.textures_delta.clear();
            let mut actions = Vec::new();
            for request in requests {
                app.run_toolbar_request(ctx, request, Some(Tab::Graph(graph)), &mut actions);
            }
            for action in actions {
                app.state.dispatch(action);
            }
            output
        };
        let swatch = |output: &egui::FullOutput, palette: bool| {
            output
                .shapes
                .iter()
                .find_map(|painted| match &painted.shape {
                    egui::Shape::Rect(rect)
                        if if palette {
                            rect.fill == egui::Color32::from_rgb(0x8b, 0x1a, 0x1a)
                                && rect.rect.width() > 12.0
                        } else {
                            (rect.rect.width() - 12.0).abs() < 0.01 && rect.fill.a() > 0
                        } =>
                    {
                        Some(rect.rect.center())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("échantillon peint (palette={palette})"))
        };
        for target in [
            TextTarget::Title,
            TextTarget::AxisTitle {
                row: 0,
                column: 0,
                side: stokesforge_plot::AxisSide::Bottom,
            },
        ] {
            let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
            app.with_graph_view(graph, |view| view.select_text(Some(target), &spec));
            paint(&mut app, &ctx, Vec::new());
            paint(&mut app, &ctx, Vec::new());
            let output = paint(&mut app, &ctx, Vec::new());
            let button = swatch(&output, false);
            click_at(&ctx, button, egui::Modifiers::NONE, |ctx, events| {
                paint(&mut app, ctx, events)
            });
            paint(&mut app, &ctx, Vec::new());
            let output = paint(&mut app, &ctx, Vec::new());
            let red = swatch(&output, true);
            click_at(&ctx, red, egui::Modifiers::NONE, |ctx, events| {
                paint(&mut app, ctx, events)
            });
            let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
            assert_eq!(
                target.color(&spec).as_deref(),
                Some("#8b1a1a"),
                "la zone {target:?} reçoit le rouge"
            );
        }
        session.shutdown().unwrap();
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

    #[test]
    fn une_commande_detachee_programme_la_publication_de_son_resultat() {
        use eframe::App as _;
        let (session, _graph) = graphe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book = app.state.books()[0].0;
        let sheet = app.state.selected_sheet_in(book).unwrap();
        let column = app.state.visible_columns(sheet)[0].0;
        let tab = Tab::Book(book);
        app.detach_tab(tab);
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        egui_extras::install_image_loaders(&ctx);
        let mut frame = eframe::Frame::_new_kittest();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 700.0),
            )),
            ..Default::default()
        };
        for _ in 0..8 {
            ctx.run_ui(input.clone(), |ui| app.ui(ui, &mut frame))
                .textures_delta
                .clear();
        }
        let mut idle = ctx.run_ui(input.clone(), |ui| app.ui(ui, &mut frame));
        idle.textures_delta.clear();
        assert_eq!(
            idle.viewport_output[&egui::ViewportId::ROOT].repaint_delay,
            Duration::MAX
        );
        app.detached_tabs[&tab]
            .requests
            .lock()
            .unwrap()
            .actions
            .push(Action::CommitCell {
                column,
                row: 0,
                text: "42".into(),
            });
        let mut output = ctx.run_ui(input.clone(), |ui| app.ui(ui, &mut frame));
        output.textures_delta.clear();
        assert_eq!(
            output.viewport_output[&egui::ViewportId::ROOT].repaint_delay,
            Duration::ZERO,
            "la commande a changé l'instantané après la peinture"
        );
        assert_eq!(app.state.cell_text(column, 0), "42");
        let mut output = ctx.run_ui(input, |ui| app.ui(ui, &mut frame));
        output.textures_delta.clear();
        assert_eq!(
            output.viewport_output[&egui::ViewportId::from_hash_of(tab)].repaint_delay,
            Duration::ZERO
        );
        assert_eq!(
            app.detached_tabs[&tab]
                .view_model
                .lock()
                .unwrap()
                .snapshot
                .as_ref()
                .unwrap()
                .revision,
            app.state.revision(),
            "l'image programmée publie le résultat"
        );
        session.shutdown().unwrap();
    }

    #[test]
    fn ctrl_n_confirme_et_oublie_les_vues_du_projet_precedent() {
        use eframe::App as _;
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        app.detach_tab(Tab::Graph(graph));
        let context = egui::Context::default();
        set_metadata_row_visible(&context, false);
        context.set_embed_viewports(false);
        theme::install(&context);
        egui_extras::install_image_loaders(&context);
        let mut frame = eframe::Frame::_new_kittest();
        {
            let mut paint = |events| {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 700.0),
                    )),
                    events,
                    ..Default::default()
                };
                let mut output = context.run_ui(input, |ui| app.ui(ui, &mut frame));
                output.textures_delta.clear();
            };
            paint(vec![egui::Event::Key {
                key: egui::Key::N,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }]);
            paint(Vec::new()); // le dialogue est maintenant peint et peut recevoir Entrée
        }
        assert!(app.state.new_project_pending());
        assert!(app.state.tree().node(graph).is_some());
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 700.0),
            )),
            events: vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| app.ui(ui, &mut frame));
        output.textures_delta.clear();
        assert_eq!(app.state.tree().len(), 1);
        assert!(app.floating_tabs.is_empty());
        assert!(app.detached_tabs.is_empty());
        assert!(app.graph_views.is_empty());
        assert!(app.floating_rects.is_empty());
        assert!(!metadata_row_visible(&context));
        assert!(
            output.viewport_output[&egui::ViewportId::from_hash_of(Tab::Graph(graph))]
                .commands
                .contains(&egui::ViewportCommand::Close)
        );
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

    /// Issue #16 : après enregistrement et rechargement, les fenêtres
    /// reviennent dans leur état — maximisée, repliée, fermée, et dans leur
    /// ordre d'empilement.
    #[test]
    fn l_etat_des_fenetres_survit_a_l_enregistrement_et_au_rechargement() {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book = app.state.books()[0].0;
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(400.0, 300.0));
        app.remember_window_geometry(Tab::Graph(graph), rect);
        app.remember_window_geometry(Tab::Book(book), rect);
        // Le graphe est sous le classeur et replié ; le classeur est maximisé.
        app.floating_tabs = vec![Tab::Graph(graph), Tab::Book(book)];
        app.collapsed_tabs.insert(Tab::Graph(graph));
        app.maximized_tab = Some(Tab::Book(book));
        app.sync_window_views();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fenetres.stokesforge");
        session.handle().save(&path).unwrap();

        let reloaded = Session::spawn(Project::new("Vide").unwrap(), None).unwrap();
        reloaded.handle().load(&path).unwrap();
        let state = AppState::new(reloaded.handle(), "test.stokesforge".to_string()).unwrap();
        let app = StokesForgeApp::new(state);
        assert_eq!(app.maximized_tab, Some(Tab::Book(book)));
        assert!(
            app.floating_tabs.contains(&Tab::Graph(graph)),
            "détachée → flottante"
        );
        assert_eq!(
            app.floating_tabs.last(),
            Some(&Tab::Book(book)),
            "le classeur, au-dessus, reste au-dessus"
        );
        assert!(app.collapsed_tabs.contains(&Tab::Graph(graph)));
        assert!(
            app.detached_tabs.is_empty(),
            "le détachement ne se retient pas"
        );
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
            assert_eq!(
                actions,
                vec![Action::DropFiles {
                    paths: vec!["detache.csv".into()],
                    target_book: target
                }]
            );
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

    /// Le lien de la barre d'état produit exactement l'action du menu Fichier.
    #[test]
    fn le_lien_de_statut_reprend_le_dernier_import() {
        let (session, _) = graphe();
        let mut state = AppState::new(session.handle(), "test.stokesforge".into()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("mesure.csv");
        std::fs::write(&file, "1,10\n2,20\n").unwrap();
        state.dispatch(Action::ImportFiles {
            destination: crate::state::ImportDestination::NewBookForAll,
            recipe: stokesforge_import::ImportRecipe::new("Sans titre"),
            paths: vec![file],
        });
        let app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        crate::theme::install(&context);
        let mut actions = Vec::new();
        let mut paint = |events| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 600.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.status_bar(ui, &mut actions),
            );
            output.textures_delta.clear();
            output
        };
        let output = paint(Vec::new());
        let pos = find_text_center(&output.shapes, &tr!("action-reopen-last-import"));
        paint(vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            paint(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            }]);
        }
        assert_eq!(actions, vec![Action::ReopenLastImport]);
    }

    /// Ctrl+" ouvre l'assistant d'import, sur un clavier AZERTY (touche du 3
    /// sans Maj) comme QWERTY (Maj+apostrophe), et pas pendant qu'il est déjà
    /// ouvert.
    #[test]
    fn ctrl_guillemet_ouvre_l_assistant_d_import() {
        let (session, _graph) = graphe();
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let press = |key: egui::Key, modifiers: egui::Modifiers| egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        };
        for (key, modifiers) in [
            (egui::Key::Num3, egui::Modifiers::COMMAND),
            (
                egui::Key::Quote,
                egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            ),
        ] {
            assert!(app.state.import_state().is_none());
            let mut actions = Vec::new();
            let mut output = context.run_ui(press(key, modifiers), |ui| {
                app.collect_shortcuts(ui.ctx(), &mut actions);
            });
            output.textures_delta.clear();
            assert!(app.state.import_state().is_some(), "{key:?}");
            app.state.cancel_import();
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

    /// Mesure (pas une vérification) : temps d'une image complète — passe
    /// egui + tessellation — au repos puis pendant le déplacement d'une
    /// fenêtre flottante par sa barre de titre.
    /// `cargo test -p stokesforge-app --release --lib mesure_deplacement -- --ignored --nocapture`
    #[test]
    #[ignore = "mesure de performance, à lancer à la main"]
    fn mesure_deplacement_de_fenetre() {
        use eframe::App as _;
        let mut project = Project::new("Mesure").unwrap();
        let root = project.root();
        let book = project.add_workbook(root, Some("Book1")).unwrap();
        let sheet = project
            .tree()
            .child_by_name(book, "Sheet1")
            .unwrap()
            .unwrap();
        let xs: Vec<f64> = (0..=120).map(|i| 400.0 + i as f64 * 2.5).collect();
        let ys: Vec<f64> = xs
            .iter()
            .map(|x| 0.85 * (-((x - 520.0) / 38.0).powi(2)).exp() + 0.04)
            .collect();
        let x = project
            .add_column(sheet, None, Some("Lambda"), Column::from_f64(&xs))
            .unwrap();
        let y = project
            .add_column(sheet, None, Some("Absorbance"), Column::from_f64(&ys))
            .unwrap();
        let mut panel = PanelSpec::at(0, 0);
        panel.series.push(SeriesSpec::new_xy(x, y));
        let graph = project
            .add_graph(
                root,
                Some("Graph1"),
                PlotSpec::empty("Spectre").with_panels(panel),
            )
            .unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "mesure.stokesforge".to_string()).unwrap();
        let mut app = StokesForgeApp::new(state);
        app.open_tab(Tab::Book(book));
        app.open_tab(Tab::Graph(graph));

        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        egui_extras::install_image_loaders(&context);
        crate::theme::install(&context);
        let mut frame = eframe::Frame::_new_kittest();
        let mut events: Vec<egui::Event> = Vec::new();
        let mut now = 0.0;
        let mut frame_ms = |events: Vec<egui::Event>| -> (f64, f64, egui::FullOutput) {
            now += 1.0 / 60.0;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                time: Some(now),
                events,
                ..Default::default()
            };
            let t0 = std::time::Instant::now();
            let mut output = context.run_ui(input, |ui| app.ui(ui, &mut frame));
            let t1 = std::time::Instant::now();
            let primitives = context.tessellate(output.shapes.clone(), output.pixels_per_point);
            let t2 = std::time::Instant::now();
            std::hint::black_box(&primitives);
            output.textures_delta.clear();
            (
                (t1 - t0).as_secs_f64() * 1e3,
                (t2 - t1).as_secs_f64() * 1e3,
                output,
            )
        };
        let report = |label: &str, mut samples: Vec<(f64, f64)>| {
            samples.sort_by(|a, b| (a.0 + a.1).total_cmp(&(b.0 + b.1)));
            let n = samples.len() as f64;
            let ui_avg = samples.iter().map(|s| s.0).sum::<f64>() / n;
            let tess_avg = samples.iter().map(|s| s.1).sum::<f64>() / n;
            let (median, worst) = (samples[samples.len() / 2], samples[samples.len() - 1]);
            eprintln!(
                "{label:<28} ui moy {ui_avg:6.2} ms | tessellation moy {tess_avg:6.2} ms | \
                 médiane {:6.2} | pire {:6.2}",
                median.0 + median.1,
                worst.0 + worst.1
            );
        };

        // Chauffe (chargement des textures, caches de rendu).
        let mut last = frame_ms(Vec::new()).2;
        for _ in 0..10 {
            last = frame_ms(Vec::new()).2;
        }
        let idle: Vec<(f64, f64)> = (0..60)
            .map(|_| {
                let (a, b, _) = frame_ms(Vec::new());
                (a, b)
            })
            .collect();
        report("au repos", idle);

        // Déplacement de la fenêtre du graphe (au premier plan) par son titre.
        // Le titre de la fenêtre, pas l'entrée du Project Manager (à gauche).
        let grab = last
            .shapes
            .iter()
            .filter_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) if shape.galley.job.text == "Graph1" => {
                    Some(shape.pos + shape.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .find(|pos| pos.x > 300.0)
            .expect("titre de fenêtre peint");
        events.push(egui::Event::PointerMoved(grab));
        frame_ms(std::mem::take(&mut events));
        events.push(egui::Event::PointerButton {
            pos: grab,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        frame_ms(std::mem::take(&mut events));
        let mut pos = grab;
        let moving: Vec<(f64, f64)> = (0..60)
            .map(|i| {
                pos += egui::vec2(if i < 30 { 6.0 } else { -6.0 }, 2.0);
                let (a, b, _) = frame_ms(vec![egui::Event::PointerMoved(pos)]);
                (a, b)
            })
            .collect();
        report("fenêtre graphe en mouvement", moving);
        let after = frame_ms(Vec::new()).2;
        eprintln!(
            "formes peintes : {} ; titre {grab:?} -> {:?}",
            after.shapes.len(),
            after
                .shapes
                .iter()
                .filter_map(|painted| match &painted.shape {
                    egui::Shape::Text(shape) if shape.galley.job.text == "Graph1" => {
                        Some(shape.pos + shape.galley.rect.center().to_vec2())
                    }
                    _ => None,
                })
                .find(|pos| pos.x > 300.0)
        );
        frame_ms(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
    }
}

#[cfg(test)]
mod text_format_state_tests {
    use super::*;
    use crate::text_format::TextFormat;
    use stokesforge_core::Project;
    use stokesforge_plot::{AnnotationRef, AnnotationSpec, AxisSide, PlotSpec, TextFont};
    use stokesforge_session::Session;

    fn setup() -> (Session, StokesForgeApp, NodeId, egui::Context) {
        let mut project = Project::new("Formats").unwrap();
        let mut spec = PlotSpec::empty("Titre");
        spec.panels
            .get_mut(0)
            .unwrap()
            .annotations
            .push(AnnotationSpec::new("Texte", 0.0, 0.0));
        let graph = project.add_graph(project.root(), None, spec).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        (
            session,
            StokesForgeApp::new(state),
            graph,
            egui::Context::default(),
        )
    }

    #[test]
    fn les_corps_herites_et_auto_suivent_la_selection_flottante_et_detachee() {
        for detached in [false, true] {
            let (session, mut app, graph, ctx) = setup();
            if detached {
                app.detach_tab(Tab::Graph(graph));
            }
            for (target, size) in [
                (TextTarget::Title, stokesforge_render_egui::TITLE_SIZE),
                (
                    TextTarget::PanelTitle { row: 0, column: 0 },
                    stokesforge_render_egui::PANEL_TITLE_SIZE,
                ),
                (
                    TextTarget::AxisTitle {
                        row: 0,
                        column: 0,
                        side: AxisSide::Bottom,
                    },
                    stokesforge_render_egui::AXIS_DESC_SIZE,
                ),
                (
                    TextTarget::Legend { row: 0, column: 0 },
                    stokesforge_render_egui::LABEL_SIZE,
                ),
            ] {
                let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
                app.with_graph_view(graph, |view| view.select_text(Some(target), &spec));
                let inherited = app.text_format_state(&ctx, Some(Tab::Graph(graph)));
                assert_eq!(inherited.size, Some(size));
                assert_eq!(
                    inherited.family.as_deref(),
                    Some(stokesforge_render_egui::FontSet::default().sans_family_name())
                );
                assert!(inherited.auto_size && inherited.auto_family);
                app.state.dispatch(Action::SetTextFont {
                    graph,
                    target,
                    font: TextFont {
                        family: "Fira Sans".into(),
                        size: Some(28.0),
                    },
                });
                let explicit = app.text_format_state(&ctx, Some(Tab::Graph(graph)));
                assert_eq!(explicit.size, Some(28.0));
                assert_eq!(explicit.family.as_deref(), Some("Fira Sans"));
                assert!(!explicit.auto_size && !explicit.auto_family);
                let mut actions = Vec::new();
                app.format_text(
                    &ctx,
                    TextFormat::AutoSize,
                    Some(Tab::Graph(graph)),
                    &mut actions,
                );
                assert_eq!(actions.len(), 1);
                app.state.dispatch(actions.pop().unwrap());
                let reset = app.text_format_state(&ctx, Some(Tab::Graph(graph)));
                assert_eq!(reset.size, Some(size));
                assert!(reset.auto_size);
                assert_eq!(
                    reset.family, explicit.family,
                    "Auto ne change pas la police"
                );
                app.state.dispatch(Action::Undo);
                assert_eq!(
                    app.text_format_state(&ctx, Some(Tab::Graph(graph))),
                    explicit
                );
                app.state.dispatch(Action::Undo);
            }
            session.shutdown().unwrap();
        }
    }

    #[test]
    fn la_barre_lit_le_brouillon_et_auto_retablit_le_corps_du_texte_libre() {
        let (session, mut app, graph, ctx) = setup();
        let target = AnnotationRef {
            row: 0,
            column: 0,
            index: 0,
        };
        let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
        let mut draft = target.get(&spec).unwrap().clone();
        draft.font = "Libertinus Sans".into();
        draft.size = 36.0;
        let id = app
            .with_graph_view(graph, |view| {
                view.select_annotation(Some(target), &spec);
                annotation_editor_id(view)
            })
            .unwrap();
        ctx.data_mut(|data| {
            data.insert_temp(
                id,
                AnnotationEditor {
                    target,
                    draft,
                    original: target.get(&spec).cloned(),
                    focus: false,
                    anchor: None,
                },
            )
        });
        let style = app.text_format_state(&ctx, Some(Tab::Graph(graph)));
        assert_eq!(style.family.as_deref(), Some("Libertinus Sans"));
        assert_eq!(style.size, Some(36.0));
        assert_eq!(style.color.as_deref(), Some(""));
        let mut actions = Vec::new();
        app.format_text(
            &ctx,
            TextFormat::AutoSize,
            Some(Tab::Graph(graph)),
            &mut actions,
        );
        assert!(
            actions.is_empty(),
            "seul le brouillon change avant validation"
        );
        assert_eq!(
            app.text_format_state(&ctx, Some(Tab::Graph(graph))).size,
            Some(AnnotationSpec::DEFAULT_SIZE)
        );
        ctx.data_mut(|data| data.remove::<AnnotationEditor>(id));
        assert!(
            app.text_format_state(&ctx, Some(Tab::Graph(graph)))
                .auto_family
        );
        session.shutdown().unwrap();
    }

    #[test]
    fn les_formats_locaux_affichent_un_etat_neutre() {
        let (session, mut app, graph, ctx) = setup();
        app.state.dispatch(Action::SetTextContent {
            graph,
            target: TextTarget::Title,
            text: "Titre #text(size: 28pt, font: \"Fira Sans\", fill: red)[local]".into(),
        });
        let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| {
            view.select_text(Some(TextTarget::Title), &spec)
        });
        let style = app.text_format_state(&ctx, Some(Tab::Graph(graph)));
        assert!(style.family.is_none() && style.size.is_none());
        assert!(style.color.is_none());
        assert!(!style.auto_family && !style.auto_size);
        session.shutdown().unwrap();
    }
}

#[cfg(test)]
mod object_manager_tests {
    use super::*;
    use stokesforge_core::Project;
    use stokesforge_plot::{PanelSpec, PlotSpec, SeriesRef, SeriesSpec};
    use stokesforge_session::Session;

    #[test]
    fn la_selection_et_le_format_rejoignent_la_meme_vue_detachee() {
        let mut project = Project::new("Objets").unwrap();
        let root = project.root();
        let mut panel = PanelSpec::default();
        panel.series.push(SeriesSpec::new(NodeId::from_raw(100)));
        let spec = PlotSpec::empty("Objets").with_panels(panel);
        let graph = project.add_graph(root, None, spec.clone()).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let target = SeriesRef::new(0, 0);
        app.with_graph_view(graph, |view| view.select_series(Some(target), &spec));
        app.detach_tab(Tab::Graph(graph));
        assert!(!app.graph_views.contains_key(&graph));
        assert_eq!(app.selected_series(graph), Some(target));
        let ctx = egui::Context::default();
        app.detached_tabs[&Tab::Graph(graph)]
            .view_model
            .lock()
            .unwrap()
            .focused = true;
        let mut output = ctx.run_ui(
            egui::RawInput {
                focused: false,
                ..Default::default()
            },
            |_| {
                assert_eq!(app.active_content_tab(&ctx), Some(Tab::Graph(graph)));
            },
        );
        output.textures_delta.clear();
        // Revenir dans le panneau conserve la cible native du graphe.
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
            assert_eq!(app.active_content_tab(&ctx), Some(Tab::Graph(graph)));
        });
        output.textures_delta.clear();
        let mut actions = Vec::new();
        app.format_object(
            &crate::object_format::ObjectFormat::LineWidth(4.0),
            Some(Tab::Graph(graph)),
            &mut actions,
        );
        assert_eq!(actions.len(), 1);
        app.state.dispatch(actions.pop().unwrap());
        assert_eq!(
            app.object_format_state(Some(Tab::Graph(graph)))
                .unwrap()
                .line_width,
            4.0
        );
        app.state.dispatch(Action::Undo);
        assert_eq!(
            crate::state::current_plot_spec(app.state.tree(), graph).unwrap(),
            spec
        );
        app.reattach_tab(Tab::Graph(graph));
        assert_eq!(app.selected_series(graph), Some(target));
        assert_eq!(app.graph_views[&graph].selected_series(), Some(target));
    }
    #[test]
    fn les_objets_masques_et_le_format_suivent_la_vue_detachee() {
        use crate::object_manager::VisibilityTarget as T;
        use stokesforge_plot::{
            AnnotationRef, AnnotationSpec, ShapeKind, ShapeRef, ShapeSpec, TextTarget,
        };
        let mut project = Project::new("Objets").unwrap();
        let root = project.root();
        let mut spec = PlotSpec::empty("Titre");
        spec.title_visible = false;
        let panel = spec.panels.get_mut(0).unwrap();
        panel
            .annotations
            .push(AnnotationSpec::new("Texte", 0.0, 0.0));
        panel.annotations[0].visible = false;
        panel
            .shapes
            .push(ShapeSpec::new(ShapeKind::Rectangle, 0.0, 0.0, 1.0, 1.0));
        panel.shapes[0].visible = false;
        let graph = project.add_graph(root, None, spec).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        app.detach_tab(Tab::Graph(graph));
        let ctx = egui::Context::default();
        let text = AnnotationRef {
            row: 0,
            column: 0,
            index: 0,
        };
        let shape = ShapeRef {
            row: 0,
            column: 0,
            index: 0,
        };
        let mut actions = Vec::new();
        for target in [
            T::Annotation(text),
            T::Shape(shape),
            T::Text(TextTarget::Title),
        ] {
            app.run_object_manager_request(
                &ctx,
                graph,
                ObjectManagerRequest::Select(target),
                &mut actions,
            );
            assert_eq!(
                app.read_graph_view(graph, selected_manager_target)
                    .flatten(),
                Some(target)
            );
            assert!(
                !app.graph_views.contains_key(&graph),
                "aucune seconde vue créée"
            );
        }
        assert_eq!(
            app.format_target(&ctx, Some(Tab::Graph(graph))),
            crate::toolbar::FormatTarget::Zone
        );
        app.format_text(
            &ctx,
            crate::text_format::TextFormat::Size(28.0),
            Some(Tab::Graph(graph)),
            &mut actions,
        );
        assert!(matches!(
            actions.pop(),
            Some(Action::SetTextFont {
                target: TextTarget::Title,
                ..
            })
        ));
        app.run_object_manager_request(
            &ctx,
            graph,
            ObjectManagerRequest::Select(T::Shape(shape)),
            &mut actions,
        );
        assert!(app.object_format_state(Some(Tab::Graph(graph))).is_some());
        app.run_object_manager_request(
            &ctx,
            graph,
            ObjectManagerRequest::Select(T::Annotation(text)),
            &mut actions,
        );
        assert_eq!(
            app.format_target(&ctx, Some(Tab::Graph(graph))),
            crate::toolbar::FormatTarget::Block
        );
        app.reattach_tab(Tab::Graph(graph));
        assert_eq!(
            selected_manager_target(&app.graph_views[&graph]),
            Some(T::Annotation(text))
        );
        assert!(actions.is_empty(), "la sélection n'envoie aucune commande");
    }
}
