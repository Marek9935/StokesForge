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

mod dialogs;
mod grid;
mod import_wizard;
mod plot_settings;
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

use dialogs::{modal_dialog, modal_dialog_with, DialogFooter, DialogOutcome};
pub use grid::{grid_grip_side, GripSide};
use grid::{keyboard_focus_outside_grid, metadata_row_visible, set_metadata_row_visible};
use import_wizard::import_window;
use plot_settings::{column_metadata_window, plot_settings_window};
use shortcuts::{
    collect_undo_redo_shortcuts, shortcut_text_delete, SHORTCUT_DUPLICATE_SHEET,
    SHORTCUT_EXPORT_GRAPH, SHORTCUT_IMPORT, SHORTCUT_IMPORT_QWERTY, SHORTCUT_IMPORT_TEXT,
    SHORTCUT_NEW_PROJECT, SHORTCUT_OPEN, SHORTCUT_REDO_Y, SHORTCUT_RENAME_SHEET,
    SHORTCUT_RUN_SCRIPT, SHORTCUT_SAVE, SHORTCUT_SAVE_AS, SHORTCUT_UNDO,
};
use tab::{tab_for_window, tab_node, Tab};
use window_chrome::{
    floating_resize_handles, header_icon, show_window_in_layer, window_header, HeaderPlacement,
    WindowHeaderActions, HEADER_ACTION_WIDTH,
};
use window_detach::{DetachedProjectManagerRequests, DetachedTab, ProjectManagerViewModel};
use window_placement::geometry_rect;
use window_stack::window_nodes;
use workbook::{book_contents, workbook_menu};

use eframe::egui;
use stokesforge_core::{Node, NodeId, Tree};
use stokesforge_render_egui::{GraphView, TextTarget, DEFAULT_FIGURE_SIZE};
use stokesforge_session::{Snapshot, WindowGeometry};

use crate::color_picker::{color_field, ColorUsage, PickerEnv, SavedColorEdit};
use crate::i18n;
use crate::icons::{icon_button, icon_image, Icon, IconRole};
use crate::preferences::Preferences;
use crate::state::{
    active_graph, drop_payload_action, export_file_name, node_title_in, series_list_label, Action,
    ActiveTabKind, AppState, ColumnDragPayload, ImportDestination, PlotAxis, PlotSettingsTab,
    PlotTarget, ProjectTreeNode, SaveTarget, ShapeOrder, WindowKind,
};
use crate::theme;
use crate::toolbar::{BarEnv, ToolbarId, ToolbarLayout, ToolbarRequest, ToolbarState};
use crate::tr;
use stokesforge_render_egui::{AxisSide, PlotComponent};

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

/// Hauteur du seul contrôle conservé sous une figure : le retour au cadrage
/// initial. Réserver sa hauteur garde le rectangle de la figure stable d'une
/// image à l'autre — sans quoi le cache de formes serait invalidé au moindre
/// survol.
const GRAPH_CONTROLS_HEIGHT: f32 = 28.0;
/// Hauteur de la zone de sortie du script, en points.
const SCRIPT_OUTPUT_HEIGHT: f32 = 140.0;
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

/// Inscrit `paths` aux fichiers d'import récents (dossier de config). Un échec
/// d'écriture est sans conséquence : la liste est un confort.
fn remember_recent_imports(paths: &[std::path::PathBuf]) {
    let mut recent = stokesforge_import::RecentFiles::load();
    recent.record(paths);
    let _ = recent.save();
}

/// Emplacement (mémoire temporaire d'egui) des fichiers déposés cette image :
/// `book` est renseigné par le classeur sous le curseur, s'il y en a un.
#[derive(Clone)]
struct FileDropSlot {
    paths: Vec<std::path::PathBuf>,
    book: Option<NodeId>,
}

fn file_drop_slot_id(ctx: &egui::Context) -> egui::Id {
    // La mémoire temporaire est commune aux viewports : un dépôt différé
    // ne doit ni écraser ni consommer celui de la fenêtre principale.
    egui::Id::new("stokesforge_file_drop").with(ctx.viewport_id())
}

/// Ouvre l'emplacement du dépôt de l'image et peint le retour visuel : un
/// contour autour de l'application tant qu'un fichier survole la fenêtre
/// (destination « nouveau classeur » ; un classeur survolé se surligne
/// lui-même, voir [`claim_file_drop`]).
fn begin_file_drop(ctx: &egui::Context) {
    let slot_id = file_drop_slot_id(ctx);
    let (paths, hovering) = ctx.input(|input| {
        (
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect::<Vec<_>>(),
            !input.raw.hovered_files.is_empty(),
        )
    });
    if hovering {
        let rect = ctx.content_rect().shrink(2.0);
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("stokesforge_file_drop_outline"),
        ))
        .rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(2.0, egui::Color32::from_rgb(0x2a, 0x7f, 0xff)),
            egui::StrokeKind::Inside,
        );
    }
    ctx.data_mut(|data| {
        if paths.is_empty() {
            data.remove::<FileDropSlot>(slot_id);
        } else {
            data.insert_temp(slot_id, FileDropSlot { paths, book: None });
        }
    });
}

/// Appelé par le classeur en train de se peindre : surligne sa zone quand un
/// fichier la survole, et réclame le dépôt de l'image s'il a eu lieu sous le
/// curseur (`rect_contains_pointer` tient compte de l'empilement des
/// fenêtres : seule la fenêtre du dessus répond).
fn claim_file_drop(ui: &egui::Ui, book: NodeId) {
    let slot_id = file_drop_slot_id(ui.ctx());
    let rect = ui.max_rect();
    if !ui.rect_contains_pointer(rect) {
        return;
    }
    if ui.ctx().input(|input| !input.raw.hovered_files.is_empty()) {
        ui.painter().rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(3.0, egui::Color32::from_rgb(0x2a, 0x7f, 0xff)),
            egui::StrokeKind::Inside,
        );
    }
    ui.ctx().data_mut(|data| {
        if let Some(slot) = data.get_temp::<FileDropSlot>(slot_id) {
            if slot.book.is_none() {
                data.insert_temp(
                    slot_id,
                    FileDropSlot {
                        book: Some(book),
                        ..slot
                    },
                );
            }
        }
    });
}

/// Transforme le dépôt de l'image en [`Action::DropFiles`].
fn finish_file_drop(ctx: &egui::Context) -> Option<Action> {
    let slot_id = file_drop_slot_id(ctx);
    let slot = ctx.data_mut(|data| {
        let slot = data.get_temp::<FileDropSlot>(slot_id);
        data.remove::<FileDropSlot>(slot_id);
        slot
    })?;
    Some(Action::DropFiles {
        paths: slot.paths,
        target_book: slot.book,
    })
}

/// Ouvre `path` dans une nouvelle fenêtre StokesForge : un second processus de ce
/// même binaire (`stokesforge-app <chemin>`). Un échec de lancement n'est que
/// journalisé — un dépôt raté ne doit rien casser.
fn open_project_in_new_window(path: &std::path::Path) {
    let spawned =
        std::env::current_exe().and_then(|exe| std::process::Command::new(exe).arg(path).spawn());
    if let Err(err) = spawned {
        eprintln!(
            "[stokesforge-app] ouverture de {} impossible : {err}",
            path.display()
        );
    }
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

/// Peint le contenu de l'onglet `tab`, quelle que soit sa provenance — la
/// boucle des fenêtres flottantes ou la zone de maximisation
/// ([`StokesForgeApp::ui`]) l'appellent toutes les deux : **une seule** fonction
/// qui sait peindre un onglet, jamais deux implémentations à tenir en
/// accord. Ne prend plus de `&egui::Context` depuis que la console de script
/// (seule nature à en avoir eu besoin, pour son `request_repaint`) est
/// devenue un panneau fixe — voir [`StokesForgeApp::script_panel`].
fn tab_contents(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    graph_views: &mut HashMap<NodeId, GraphView>,
    tab: Tab,
) {
    match tab {
        Tab::Book(id) => book_contents(ui, state, actions, id),
        Tab::Graph(id) => graph_drop_target(ui, state, actions, graph_views, id),
        Tab::Matrix(id) => matrix_contents(ui, state.tree(), id),
        Tab::Note(id) => note_contents(ui, state, actions, id),
    }
}

/// Enrobe [`graph_contents`] d'une zone de dépôt pour le glisser d'une
/// sélection de colonnes de classeur — pendant, côté cible, de la zone de
/// prise posée par [`grid_contents`] en bord de grille. Appelée par
/// [`tab_contents`], donc identiquement pour un onglet en fenêtre flottante
/// et pour un onglet détaché ([`StokesForgeApp::show_detached_tabs`]) : c'est le
/// seul point
/// d'entrée que les deux utilisent, voir la doc de tête de ce dernier — pas
/// de code de dépôt séparé à maintenir pour le cas détaché.
///
/// Un survol avec une [`ColumnDragPayload`] active dessine un contour sobre
/// autour du graphe (teinte de sélection de `theme`, pas de palette
/// inventée) ; un relâchement au-dessus pousse
/// [`crate::state::drop_payload_action`] dans `actions`, converti en
/// [`Action::AddSeriesToGraph`]. Relâcher ailleurs qu'au-dessus d'un graphe
/// ne pousse rien : `dnd_release_payload` ne répond que si le pointeur est
/// contenu dans ce rectangle au moment du relâchement.
fn graph_drop_target(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    graph_views: &mut HashMap<NodeId, GraphView>,
    graph_id: NodeId,
) {
    let tab_rect = ui.max_rect();
    // Une `Response` couvrant tout l'onglet, même avec `Sense::hover()`,
    // participe à la sélection de cible d'egui et gagne le glissé principal
    // sur le `GraphView`. C'était la différence avec la fenêtre détachée,
    // qui appelle directement `graph_view_contents`, et la cause du pan
    // inopérant dans la fenêtre principale. La cible de dépôt n'a pas besoin
    // d'être un widget : lire le pointeur et la charge d'egui suffit.
    let pointer_in_tab = ui.rect_contains_pointer(tab_rect);
    let hovering =
        pointer_in_tab && egui::DragAndDrop::has_payload_of_type::<ColumnDragPayload>(ui.ctx());
    if pointer_in_tab && ui.input(|input| input.pointer.any_released()) {
        if let Some(payload) = egui::DragAndDrop::take_payload::<ColumnDragPayload>(ui.ctx()) {
            actions.push(drop_payload_action(&payload, graph_id));
        }
    }
    graph_contents(ui, state, actions, graph_views, graph_id);
    if hovering {
        ui.painter().rect_stroke(
            tab_rect,
            4.0,
            egui::Stroke::new(2.5, theme::header_letter_row_selected_fill()),
            egui::StrokeKind::Inside,
        );
    }
}

/// Ce que le rendu de l'arbre du Project Manager accumule ou lit, à travers
/// [`project_manager_contents_from_tree`]/[`project_tree_node_ui`] — un seul
/// paquet plutôt que cinq paramètres, pour les deux mêmes contextes que le
/// reste du module (panneau fixe et fenêtre OS détachée, voir la doc de tête
/// de [`StokesForgeApp::show_detached_project_manager`]).
struct PmInteraction<'a> {
    /// Onglet ouvert ou focalisé par un simple clic — voir
    /// [`StokesForgeApp::open_tab`].
    open_requests: &'a mut Vec<Tab>,
    /// Onglet fermé par un double-clic sur une entrée déjà au premier plan,
    /// ou par « Masquer » sur une entrée maximisée (garde de cohérence, même
    /// esprit que celle de `detach_tab`).
    close_requests: &'a mut Vec<Tab>,
    /// L'onglet actuellement au premier plan — voir la doc de tête du
    /// module, § z-order. `None` si aucune fenêtre flottante n'est ouverte.
    active_tab: Option<Tab>,
    /// Sélection courante dans l'arbre — Ctrl/Cmd-clic en bascule
    /// l'appartenance sans ouvrir l'onglet.
    selection: &'a mut HashSet<Tab>,
    /// Onglets actuellement masqués (voir [`StokesForgeApp::hidden_tabs`]) —
    /// affichés atténués dans l'arbre, et ce que « Afficher »/« Masquer »
    /// modifient pour la sélection courante.
    hidden: &'a mut HashSet<Tab>,
}

#[derive(Clone, Copy)]
enum ObjectManagerRequest {
    Select(crate::object_manager::VisibilityTarget),
    Properties(crate::object_manager::VisibilityTarget),
    Rename(stokesforge_plot::SeriesRef),
}
#[derive(Clone, Copy)]
struct SeriesDrag {
    graph: NodeId,
    source: stokesforge_plot::SeriesRef,
}

/// Ouvre le dialogue des propriétés d'un objet graphique : l'Object Manager
/// (double-clic sur sa ligne) et la figure (double-clic sur l'objet) y vont
/// par ici (#113).
fn open_shape_properties(
    ctx: &egui::Context,
    view: &mut GraphView,
    spec: &stokesforge_plot::PlotSpec,
    target: stokesforge_plot::ShapeRef,
) {
    view.set_tool(stokesforge_render_egui::GraphTool::Pointer);
    view.select_shape(Some(target));
    if let Some(draft) = target.get(spec).cloned() {
        ctx.data_mut(|data| {
            data.insert_temp(
                view.id().with("shape_properties"),
                ShapeProperties { target, draft },
            )
        });
    }
}

/// Libellé d'un nœud d'en-tête de l'Object Manager (graphe, calque, groupes) :
/// un double-clic ouvre ses paramètres, comme sur les autres lignes (#113).
fn manager_header_label(
    ui: &mut egui::Ui,
    text: &str,
    target: crate::object_manager::VisibilityTarget,
    requests: &mut Vec<ObjectManagerRequest>,
) -> egui::Response {
    let response = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
    if response.double_clicked() {
        requests.push(ObjectManagerRequest::Properties(target));
    }
    response
}

fn selected_manager_target(view: &GraphView) -> Option<crate::object_manager::VisibilityTarget> {
    use crate::object_manager::VisibilityTarget as T;
    view.selected_series()
        .map(T::Series)
        .or_else(|| view.selected_annotation().map(T::Annotation))
        .or_else(|| view.selected_shape().map(T::Shape))
        .or_else(|| view.selected_text().map(T::Text))
}
fn select_manager_target(
    view: &mut GraphView,
    target: Option<crate::object_manager::VisibilityTarget>,
    spec: &stokesforge_plot::PlotSpec,
) {
    use crate::object_manager::VisibilityTarget as T;
    match target {
        Some(T::Series(t)) => view.select_series(Some(t), spec),
        Some(T::Annotation(t)) => view.select_annotation(Some(t), spec),
        Some(T::Shape(t)) if t.get(spec).is_some() => {
            view.set_tool(stokesforge_render_egui::GraphTool::Pointer);
            view.select_shape(Some(t));
        }
        Some(T::Text(t)) => view.select_text(Some(t), spec),
        _ => {
            view.select_annotation(None, spec);
        }
    }
}

/// Le grisage et les Action sont identiques dans Graph et au clic droit.
fn object_node_menu(
    ui: &mut egui::Ui,
    graph: NodeId,
    target: crate::object_manager::VisibilityTarget,
    spec: &stokesforge_plot::PlotSpec,
    actions: &mut Vec<Action>,
    requests: &mut Vec<ObjectManagerRequest>,
) {
    use crate::object_manager::VisibilityTarget as T;
    if ui
        .add_enabled(
            target.exists(spec),
            egui::Button::new(tr!("action-properties")),
        )
        .clicked()
    {
        requests.push(ObjectManagerRequest::Properties(target));
        ui.close();
    }
    if let T::Series(series) = target {
        if ui
            .add_enabled(
                target.exists(spec),
                egui::Button::new(tr!("action-rename-series")).shortcut_text("F2"),
            )
            .clicked()
        {
            requests.push(ObjectManagerRequest::Rename(series));
            ui.close();
        }
        for (label, shortcut, index) in [
            (
                tr!("action-move-series-up"),
                "Ctrl+Up",
                series.index.checked_sub(1),
            ),
            (
                tr!("action-move-series-down"),
                "Ctrl+Down",
                series.index.checked_add(1).filter(|i| {
                    spec.panels
                        .get(series.panel)
                        .is_some_and(|p| *i < p.series.len())
                }),
            ),
        ] {
            if ui
                .add_enabled(
                    index.is_some(),
                    egui::Button::new(label).shortcut_text(shortcut),
                )
                .clicked()
            {
                if let Some(index) = index {
                    actions.push(Action::ReorderSeries {
                        graph,
                        source: series,
                        index,
                    });
                }
                ui.close();
            }
        }
    }
    let visibility = target.visibility(spec);
    if ui
        .add_enabled(
            visibility.enabled,
            egui::Button::new(if visibility.checked {
                tr!("action-hide-content")
            } else {
                tr!("action-show-content")
            }),
        )
        .clicked()
    {
        actions.push(Action::SetPlotVisibility {
            graph,
            target,
            visible: !visibility.checked,
        });
        ui.close();
    }
    if ui
        .add_enabled(
            target.removable(spec),
            egui::Button::new(tr!("action-delete-content")).shortcut_text(shortcut_text_delete()),
        )
        .clicked()
    {
        actions.push(Action::RemovePlotObject { graph, target });
        ui.close();
    }
}

fn object_visibility(
    ui: &mut egui::Ui,
    graph: NodeId,
    target: crate::object_manager::VisibilityTarget,
    spec: &stokesforge_plot::PlotSpec,
    actions: &mut Vec<Action>,
) {
    let state = target.visibility(spec);
    let mut checked = state.checked;
    if ui
        .add_enabled(
            state.enabled,
            egui::Checkbox::new(&mut checked, "").indeterminate(state.mixed),
        )
        .on_hover_text(tr!("object-manager-visibility"))
        .changed()
    {
        actions.push(Action::SetPlotVisibility {
            graph,
            target,
            visible: checked,
        });
    }
}

/// Un seul contenu pour Object Manager, quel que soit son emplacement.
fn object_manager_contents(
    ui: &mut egui::Ui,
    tree: &Tree,
    model: Option<&crate::object_manager::ObjectTree>,
    spec: Option<&stokesforge_plot::PlotSpec>,
    show_texts: bool,
    show_scales: bool,
    actions: &mut Vec<Action>,
) -> Vec<ObjectManagerRequest> {
    use crate::object_manager::VisibilityTarget as T;
    let mut requests = Vec::new();
    let (Some(model), Some(spec)) = (model, spec) else {
        ui.label(tr!("object-manager-no-graph"));
        return requests;
    };
    let focus = ui.make_persistent_id(("object_manager_keys", model.graph));
    let mut selected = None;
    // Fond du panneau, posé sous les lignes : son menu contextuel porte la
    // bascule des textes automatiques (même action que le menu View).
    let background = ui.interact(
        ui.available_rect_before_wrap(),
        ui.make_persistent_id(("object_manager_background", model.graph)),
        egui::Sense::click(),
    );
    background.context_menu(|ui| {
        let mut show = show_texts;
        if ui
            .checkbox(&mut show, tr!("object-manager-show-texts"))
            .on_hover_text(tr!("object-manager-show-texts", "hint"))
            .changed()
        {
            actions.push(Action::SetObjectManagerShowTexts(show));
            ui.close();
        }
        let mut show = show_scales;
        if ui
            .checkbox(&mut show, tr!("object-manager-show-scales"))
            .on_hover_text(tr!("object-manager-show-scales", "hint"))
            .changed()
        {
            actions.push(Action::SetObjectManagerShowScales(show));
            ui.close();
        }
    });
    egui::ScrollArea::both().show(ui, |ui| {
        crate::tree_lines::begin_tree(ui);
        let id = ui.make_persistent_id(("object_manager_graph", model.graph));
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
            .show_header(ui, |ui| {
                object_visibility(ui, model.graph, T::Graph, spec, actions);
                let response = manager_header_label(ui, &model.title, T::Graph, &mut requests);
                response.context_menu(|ui| {
                    object_node_menu(ui, model.graph, T::Graph, spec, actions, &mut requests)
                });
            })
            .body(|ui| crate::tree_lines::tree_children(ui, |ui, branch| {
                for object in &model.objects {
                    let y = object_manager_object_row(
                        ui,
                        model.graph,
                        object,
                        spec,
                        focus,
                        actions,
                        &mut requests,
                    );
                    branch.mark(y);
                    if object.selected {
                        selected = Some(object.target);
                    }
                }
                for layer in &model.layers {
                    let id = ui.make_persistent_id((model.graph, layer.index));
                    let (layer_toggle, _, _) = egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        id,
                        true,
                    )
                    .show_header(ui, |ui| {
                        object_visibility(ui, model.graph, T::Panel(layer.index), spec, actions);
                        manager_header_label(
                            ui,
                            &tr!("object-manager-layer", index = (layer.index + 1)),
                            T::Panel(layer.index),
                            &mut requests,
                        )
                            .context_menu(|ui| {
                                object_node_menu(
                                    ui,
                                    model.graph,
                                    T::Panel(layer.index),
                                    spec,
                                    actions,
                                    &mut requests,
                                )
                            });
                    })
                    .body(|ui| crate::tree_lines::tree_children(ui, |ui, branch| {
                        let id = ui.make_persistent_id((model.graph, layer.index, "group"));
                        let (group_toggle, _, _) = egui::collapsing_header::CollapsingState::load_with_default_open(
                            ui.ctx(),
                            id,
                            true,
                        )
                        .show_header(ui, |ui| {
                            object_visibility(
                                ui,
                                model.graph,
                                T::Group(layer.index),
                                spec,
                                actions,
                            );
                            manager_header_label(
                                ui,
                                &tr!("object-manager-group"),
                                T::Group(layer.index),
                                &mut requests,
                            )
                            .context_menu(|ui| {
                                object_node_menu(
                                    ui,
                                    model.graph,
                                    T::Group(layer.index),
                                    spec,
                                    actions,
                                    &mut requests,
                                )
                            });
                        })
                        .body(|ui| crate::tree_lines::tree_children(ui, |ui, branch| {
                            for series in &layer.series {
                                if series.selected {
                                    selected = Some(T::Series(series.target));
                                }
                                let y = ui.push_id(series.target, |ui| {
                                    ui.horizontal(|ui| {
                                        object_visibility(
                                            ui,
                                            model.graph,
                                            T::Series(series.target),
                                            spec,
                                            actions,
                                        );
                                        let (rect, _) = ui.allocate_exact_size(
                                            egui::vec2(28.0, 18.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().extend(
                                            stokesforge_render_egui::series_swatch(
                                                tree,
                                                &series.spec,
                                                rect,
                                            ),
                                        );
                                        let text = egui::RichText::new(&series.label);
                                        let text =
                                            if series.selected { text.strong() } else { text };
                                        let response = ui.add(
                                            egui::Button::selectable(series.selected, text)
                                                .sense(egui::Sense::click_and_drag()),
                                        );
                                        if response.clicked()
                                            || response.secondary_clicked()
                                            || response.drag_started()
                                        {
                                            requests.push(ObjectManagerRequest::Select(T::Series(
                                                series.target,
                                            )));
                                            ui.memory_mut(|memory| memory.request_focus(focus));
                                        }
                                        if response.double_clicked() {
                                            requests.push(ObjectManagerRequest::Properties(
                                                T::Series(series.target),
                                            ));
                                        }
                                        response.dnd_set_drag_payload(SeriesDrag {
                                            graph: model.graph,
                                            source: series.target,
                                        });
                                        let drop_index = response
                                            .dnd_hover_payload::<SeriesDrag>()
                                            .filter(|drag| drag.graph == model.graph)
                                            .and_then(|drag| {
                                                let after = ui
                                                    .input(|i| i.pointer.interact_pos())
                                                    .is_some_and(|p| {
                                                        p.y > response.rect.center().y
                                                    });
                                                crate::object_manager::series_drop(
                                                    spec,
                                                    drag.source,
                                                    series.target,
                                                    after,
                                                )
                                            });
                                        if let Some(index) = drop_index {
                                            let after = ui
                                                .input(|i| i.pointer.interact_pos())
                                                .is_some_and(|p| p.y > response.rect.center().y);
                                            let y = if after {
                                                response.rect.bottom()
                                            } else {
                                                response.rect.top()
                                            };
                                            ui.painter().hline(
                                                response.rect.x_range(),
                                                y,
                                                ui.visuals().selection.stroke,
                                            );
                                            if let Some(drag) =
                                                response.dnd_release_payload::<SeriesDrag>()
                                            {
                                                actions.push(Action::ReorderSeries {
                                                    graph: model.graph,
                                                    source: drag.source,
                                                    index,
                                                });
                                            }
                                        }
                                        response.context_menu(|ui| {
                                            object_node_menu(
                                                ui,
                                                model.graph,
                                                T::Series(series.target),
                                                spec,
                                                actions,
                                                &mut requests,
                                            )
                                        });
                                    })
                                    .response
                                    .rect
                                    .center()
                                    .y
                                })
                                .inner;
                                branch.mark(y);
                            }
                        }));
                        branch.mark(group_toggle.rect.center().y);
                        if !layer.axes.is_empty() {
                            let id = ui.make_persistent_id((model.graph, layer.index, "axes"));
                            let (axes_toggle, _, _) =
                                egui::collapsing_header::CollapsingState::load_with_default_open(
                                    ui.ctx(),
                                    id,
                                    true,
                                )
                                .show_header(ui, |ui| {
                                    object_visibility(
                                        ui,
                                        model.graph,
                                        T::Axes(layer.index),
                                        spec,
                                        actions,
                                    );
                                    manager_header_label(
                                        ui,
                                        &tr!("object-manager-axes"),
                                        T::Axes(layer.index),
                                        &mut requests,
                                    )
                                    .context_menu(|ui| {
                                        object_node_menu(
                                            ui,
                                            model.graph,
                                            T::Axes(layer.index),
                                            spec,
                                            actions,
                                            &mut requests,
                                        )
                                    });
                                })
                                .body(|ui| {
                                    crate::tree_lines::tree_children(ui, |ui, branch| {
                                        for object in &layer.axes {
                                            let y = object_manager_object_row(
                                                ui,
                                                model.graph,
                                                object,
                                                spec,
                                                focus,
                                                actions,
                                                &mut requests,
                                            );
                                            branch.mark(y);
                                            if object.selected {
                                                selected = Some(object.target);
                                            }
                                        }
                                    })
                                });
                            branch.mark(axes_toggle.rect.center().y);
                        }
                        for object in &layer.objects {
                            let y = object_manager_object_row(
                                ui,
                                model.graph,
                                object,
                                spec,
                                focus,
                                actions,
                                &mut requests,
                            );
                            branch.mark(y);
                            if object.selected {
                                selected = Some(object.target);
                            }
                        }
                    }));
                    branch.mark(layer_toggle.rect.center().y);
                }
            }));
    });
    // L'ancrage appartient au panneau ; aucun champ d'édition ne perd ses touches.
    ui.interact(
        egui::Rect::NOTHING,
        focus,
        egui::Sense::focusable_noninteractive(),
    );
    if ui.memory(|m| m.focused()) == Some(focus) {
        if let Some(target) = selected {
            if target.removable(spec)
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete))
            {
                actions.push(Action::RemovePlotObject {
                    graph: model.graph,
                    target,
                });
            }
            if let T::Series(series) = target {
                series_shortcuts(ui.ctx(), model.graph, series, spec, actions, &mut requests);
            }
        }
    }
    requests
}

#[derive(Clone)]
struct ShapeProperties {
    target: stokesforge_plot::ShapeRef,
    draft: stokesforge_plot::ShapeSpec,
}

/// Brouillon seulement : Valider émet une action, Annuler ne touche rien.
fn shape_properties_ui(
    ctx: &egui::Context,
    view: &GraphView,
    graph: NodeId,
    actions: &mut Vec<Action>,
) {
    let id = view.id().with("shape_properties");
    let Some(mut edit) = ctx.data(|data| data.get_temp::<ShapeProperties>(id)) else {
        return;
    };
    let (project, user): (stokesforge_plot::SavedColors, stokesforge_plot::SavedColors) = ctx
        .data(|data| data.get_temp(saved_colors_id()))
        .unwrap_or_default();
    let mut env = PickerEnv {
        project,
        user,
        requests: Vec::new(),
    };
    let outcome = modal_dialog(
        ctx,
        id.with("dialog"),
        &tr!("object-properties-title"),
        egui::vec2(360.0, 280.0),
        |ui| {
            for (coordinate, value) in [
                ("X1", &mut edit.draft.x1),
                ("Y1", &mut edit.draft.y1),
                ("X2", &mut edit.draft.x2),
                ("Y2", &mut edit.draft.y2),
            ] {
                ui.horizontal(|ui| {
                    ui.label(tr!("object-coordinate", coordinate = coordinate));
                    ui.add(egui::DragValue::new(value).speed(0.1));
                });
            }
            color_field(
                ui,
                &tr!("format-object-line-color"),
                &mut edit.draft.line_color,
                ColorUsage::Curves,
                Some(&tr!("format-object-line-color", "hint")),
                &mut env,
            );
            if edit.draft.kind.is_closed() {
                color_field(
                    ui,
                    &tr!("format-object-fill-color"),
                    &mut edit.draft.fill_color,
                    ColorUsage::Background,
                    Some(&tr!("format-object-fill-color", "hint")),
                    &mut env,
                );
            }
            ui.horizontal(|ui| {
                ui.label(tr!("format-object-line-width"));
                ui.add(
                    egui::DragValue::new(&mut edit.draft.line_width)
                        .range(0.0..=100.0)
                        .speed(0.1),
                );
            });
        },
    );
    actions.extend(env.requests.into_iter().map(color_request_action));
    match outcome {
        DialogOutcome::Accept => {
            actions.push(Action::SetShape {
                graph,
                target: edit.target,
                shape: Box::new(edit.draft),
            });
            ctx.data_mut(|data| data.remove::<ShapeProperties>(id));
        }
        DialogOutcome::Cancel => {
            ctx.data_mut(|data| data.remove::<ShapeProperties>(id));
        }
        DialogOutcome::Open => {
            ctx.data_mut(|data| data.insert_temp(id, edit));
        }
    }
}

fn series_shortcuts(
    ctx: &egui::Context,
    graph: NodeId,
    series: stokesforge_plot::SeriesRef,
    spec: &stokesforge_plot::PlotSpec,
    actions: &mut Vec<Action>,
    requests: &mut Vec<ObjectManagerRequest>,
) {
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F2)) {
        requests.push(ObjectManagerRequest::Rename(series));
    }
    for (key, index) in [
        (egui::Key::ArrowUp, series.index.checked_sub(1)),
        (egui::Key::ArrowDown, series.index.checked_add(1)),
    ] {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, key)) {
            if let Some(index) = index.filter(|i| {
                spec.panels
                    .get(series.panel)
                    .is_some_and(|p| *i < p.series.len())
            }) {
                actions.push(Action::ReorderSeries {
                    graph,
                    source: series,
                    index,
                });
            }
        }
    }
}

fn object_manager_object_row(
    ui: &mut egui::Ui,
    graph: NodeId,
    object: &crate::object_manager::ObjectRow,
    spec: &stokesforge_plot::PlotSpec,
    focus: egui::Id,
    actions: &mut Vec<Action>,
    requests: &mut Vec<ObjectManagerRequest>,
) -> f32 {
    ui.push_id(object.target, |ui| {
        ui.horizontal(|ui| {
            object_visibility(ui, graph, object.target, spec, actions);
            let icon = match object.kind {
                crate::object_manager::ObjectKind::Text => Icon::TextTool,
                crate::object_manager::ObjectKind::Axis => Icon::Axis,
                crate::object_manager::ObjectKind::Shape(kind) => match kind {
                    stokesforge_plot::ShapeKind::Rectangle => Icon::ShapeRectangle,
                    stokesforge_plot::ShapeKind::Ellipse => Icon::ShapeEllipse,
                    stokesforge_plot::ShapeKind::Line => Icon::ShapeLine,
                    stokesforge_plot::ShapeKind::Arrow => Icon::ShapeArrow,
                },
            };
            ui.add(icon_image(icon, IconRole::Neutral))
                .on_hover_text(&object.label);
            let text = egui::RichText::new(&object.label);
            let text = if object.selected { text.strong() } else { text };
            let response = ui.selectable_label(object.selected, text);
            if response.clicked() || response.secondary_clicked() {
                requests.push(ObjectManagerRequest::Select(object.target));
                ui.memory_mut(|m| m.request_focus(focus));
            }
            if response.double_clicked() {
                requests.push(ObjectManagerRequest::Properties(object.target));
            }
            response.context_menu(|ui| {
                object_node_menu(ui, graph, object.target, spec, actions, requests)
            });
        })
        .response
        .rect
        .center()
        .y
    })
    .inner
}

/// Le corps du Project Manager, à partir d'un arbre déjà construit plutôt
/// que d'un `&AppState` vivant — seul le callback `show_viewport_deferred`
/// de [`StokesForgeApp::show_detached_project_manager`] a besoin de cette forme :
/// il ne porte qu'une copie de l'arbre ([`ProjectManagerViewModel`]), jamais
/// `AppState` elle-même. Le panneau fixe ([`StokesForgeApp::project_manager_panel`])
/// passe directement `&self.state.project_tree()` — même fonction, deux
/// sources d'arbre.
fn project_manager_contents_from_tree(
    ui: &mut egui::Ui,
    tree: &[ProjectTreeNode],
    interaction: &mut PmInteraction,
) {
    ui.heading(tr!("project-manager-title"));
    ui.separator();
    if !interaction.selection.is_empty() {
        // Barre d'actions groupées, seulement quand elle a un effet — pas de
        // boutons désactivés à regarder tant que rien n'est sélectionné.
        ui.horizontal(|ui| {
            if ui
                .button(tr!("project-manager-show"))
                .on_hover_text(tr!("project-manager-show", "hint"))
                .clicked()
            {
                for &tab in interaction.selection.iter() {
                    interaction.hidden.remove(&tab);
                    interaction.open_requests.push(tab);
                }
            }
            if ui
                .button(tr!("project-manager-hide"))
                .on_hover_text(tr!("project-manager-hide", "hint"))
                .clicked()
            {
                for &tab in interaction.selection.iter() {
                    interaction.hidden.insert(tab);
                }
            }
            ui.weak(tr!(
                "project-manager-selected",
                count = interaction.selection.len()
            ));
        });
        ui.separator();
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        crate::tree_lines::begin_tree(ui);
        for node in tree {
            project_tree_node_ui(ui, node, interaction);
        }
    });
}

/// Peint récursivement un nœud de [`ProjectTreeNode`] et ses enfants ; rend
/// le centre vertical de sa rangée, pour les connecteurs de son parent.
fn project_tree_node_ui(
    ui: &mut egui::Ui,
    node: &ProjectTreeNode,
    interaction: &mut PmInteraction,
) -> f32 {
    match node {
        ProjectTreeNode::Folder { id, name, children } => {
            let state_id = ui.make_persistent_id(("project_tree_folder", id));
            let (toggle, _, _) = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                state_id,
                false,
            )
            .show_header(ui, |ui| {
                ui.add(icon_image(Icon::Folder, IconRole::Folder));
                ui.label(name);
            })
            .body(|ui| {
                crate::tree_lines::tree_children(ui, |ui, branch| {
                    if children.is_empty() {
                        branch.mark(ui.weak(tr!("project-folder-empty")).rect.center().y);
                    }
                    for child in children {
                        branch.mark(project_tree_node_ui(ui, child, interaction));
                    }
                });
            });
            toggle.rect.center().y
        }
        ProjectTreeNode::Window { id, name, kind } => {
            let icon = match kind {
                WindowKind::Book => (Icon::Workbook, IconRole::Workbook),
                WindowKind::Graph => (Icon::Graph, IconRole::Graph),
                WindowKind::Matrix => (Icon::Matrix, IconRole::Matrix),
                WindowKind::Note => (Icon::Note, IconRole::Note),
            };
            let tab = tab_for_window(*id, *kind);
            let selected = interaction.selection.contains(&tab);
            let is_hidden = interaction.hidden.contains(&tab);
            let fill = if selected {
                ui.visuals().selection.bg_fill
            } else {
                egui::Color32::TRANSPARENT
            };
            // Le bouton reste un `egui::Button` normal (affordance de survol
            // native) ; seul le fond du `Frame` qui l'entoure porte la
            // teinte de sélection, posée derrière lui. `push_id(tab, ...)`
            // est nécessaire : `Frame::show` fait naître son enfant avec un
            // salt fixe ("child"), identique à chaque itération de cette
            // boucle — sans un salt propre à `tab`, toutes les lignes de
            // l'arbre partageraient le même `Id` de bouton, et un clic
            // finirait attribué à la dernière ligne pressée plutôt qu'à
            // celle réellement cliquée.
            let response = ui
                .push_id(tab, |ui| {
                    egui::Frame::NONE
                        .fill(fill)
                        .inner_margin(2.0)
                        .show(ui, |ui| {
                            let text = if is_hidden {
                                egui::RichText::new(name).weak()
                            } else {
                                egui::RichText::new(name)
                            };
                            ui.add(egui::Button::image_and_text(
                                icon_image(icon.0, icon.1),
                                text,
                            ))
                        })
                        .inner
                })
                .inner;
            if response.clicked() {
                if ui.input(|input| input.modifiers.command) {
                    if !interaction.selection.remove(&tab) {
                        interaction.selection.insert(tab);
                    }
                } else {
                    interaction.open_requests.push(tab);
                }
            }
            if response.double_clicked() && interaction.active_tab == Some(tab) {
                interaction.close_requests.push(tab);
            }
            response.rect.center().y
        }
    }
}

/// Console de script : zone de saisie, boutons « Exécuter »/« Arrêter »,
/// sortie. Contenu de [`StokesForgeApp::script_panel`].
///
/// Ne bloque jamais la fenêtre : voir la doc de tête de ce module.
/// `Action::RunScript`/`InterruptScript` sont poussées comme n'importe
/// quelle autre action, appliquées après le rendu par la boucle
/// principale — c'est [`AppState::dispatch`] qui garantit qu'elles ne
/// bloquent pas.
/// Barre de titre de la console de script, avec le bouton qui la replie ou la
/// déplie (issue #79). Rend `true` si ce bouton a été cliqué.
fn script_header(ui: &mut egui::Ui, state: &AppState, collapsed: bool) -> bool {
    ui.horizontal(|ui| {
        let (icon, tooltip) = if collapsed {
            (Icon::CaretUp, tr!("script-expand"))
        } else {
            (Icon::CaretDown, tr!("script-collapse"))
        };
        // Même cellule de `HEADER_ACTION_WIDTH` que les icônes des fenêtres,
        // l'icône centrée dedans et la cellule centrée sur la ligne du titre
        // (#95) : un `icon_button` collait le chevron à gauche, sans cellule.
        let row_height = ui.text_style_height(&egui::TextStyle::Heading);
        let (cell, _) = ui.allocate_exact_size(
            egui::vec2(HEADER_ACTION_WIDTH, row_height),
            egui::Sense::hover(),
        );
        let toggled = header_icon(ui, ui.id(), "script-toggle", cell, icon, &tooltip);
        ui.heading(tr!("script-title"));
        if !state.script_available() {
            ui.weak(tr!("script-unavailable"));
        }
        toggled
    })
    .inner
}

fn script_contents(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut AppState,
    actions: &mut Vec<Action>,
) {
    let running = state.script_running();
    if running {
        // Sans ça, egui ne redessine que sur événement d'entrée : la fin du
        // script n'apparaîtrait qu'au prochain geste de l'utilisateur.
        ctx.request_repaint();
    }

    let available = state.script_available() && !running;
    let mut source = state.script_source().to_string();
    let edit = ui.add_enabled(
        available,
        egui::TextEdit::multiline(&mut source)
            .code_editor()
            .desired_rows(6)
            .desired_width(f32::INFINITY),
    );
    if edit.changed() {
        state.set_script_source(source);
    }

    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                available,
                egui::Button::new(tr!("script-run"))
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_RUN_SCRIPT)),
            )
            .clicked()
        {
            actions.push(Action::RunScript);
        }
        if ui
            .add_enabled(running, egui::Button::new(tr!("script-stop")))
            .clicked()
        {
            actions.push(Action::InterruptScript);
        }
        if running {
            ui.spinner();
            ui.weak(tr!("script-running"));
        }
    });

    ui.separator();
    egui::ScrollArea::vertical()
        .max_height(SCRIPT_OUTPUT_HEIGHT)
        .show(ui, |ui| match state.script_output() {
            None => {
                ui.weak(tr!("script-not-run"));
            }
            Some(output) => {
                let mut empty = true;
                if !output.stdout.is_empty() {
                    empty = false;
                    ui.label(tr!("script-stdout"));
                    ui.monospace(&output.stdout);
                }
                if !output.stderr.is_empty() {
                    empty = false;
                    ui.label(tr!("script-stderr"));
                    ui.monospace(&output.stderr);
                }
                if let Some(error) = &output.error {
                    empty = false;
                    ui.colored_label(ui.visuals().warn_fg_color, tr!("script-traceback"));
                    ui.monospace(error);
                }
                if empty {
                    ui.weak(tr!("script-no-output"));
                }
            }
        });
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

/// Un graphe ouvert — le **rendu visible** de la phase R4 (tâche 4.6).
/// Contenu d'un [`Tab::Graph`].
///
/// Ce n'est pas une ébauche de la fenêtre de graphe finale : comme tout ce
/// banc, c'est la preuve que l'intégration bout en bout tient. Ce qu'il
/// montre vraiment, et qu'aucun test ne montre :
///
/// - la figure elle-même, avec ses vraies polices et son vrai HiDPI ;
/// - le retour au cadrage initial, pour abandonner un zoom ou un pan sans
///   modifier les données ni la description du graphe.
fn graph_contents(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    graph_views: &mut HashMap<NodeId, GraphView>,
    graph_id: NodeId,
) {
    let view = graph_views.entry(graph_id).or_default();
    graph_view_contents(ui, view, state.snapshot(), graph_id, actions);
}

/// Corps de [`graph_contents`], factorisé sur une [`GraphView`] et un
/// [`Snapshot`] déjà en main plutôt que sur `&AppState`/la table
/// [`StokesForgeApp::graph_views`] : c'est ce qui permet à
/// [`StokesForgeApp::show_detached_tabs`] de peindre un `Tab::Graph` détaché avec
/// **exactement** cette fonction — pas de version dégradée, voir la doc de
/// tête du module. `GraphView` n'a pas le problème de verrou global à
/// `AppState` qu'ont les tampons de saisie de cellule/note (son cache et sa
/// navigation sont déjà scopés par graphe) : nul besoin d'une variante
/// « détachée » séparée, à la différence de [`grid_contents`]/
/// [`note_contents`].
fn graph_view_contents(
    ui: &mut egui::Ui,
    view: &mut GraphView,
    snapshot: &Snapshot,
    graph_id: NodeId,
    actions: &mut Vec<Action>,
) {
    let available = ui.available_size_before_wrap();
    // La figure prend tout ce qui reste après ses contrôles. Réserver leur
    // hauteur complète évite le même mécanisme d'auto-agrandissement qui a
    // déjà touché les fenêtres de classeur : un dépassement ferait changer
    // `rect` à chaque image, donc provoquerait un raté de cache au survol.
    let (outer, _) = ui.allocate_exact_size(
        egui::vec2(available.x, (available.y - GRAPH_CONTROLS_HEIGHT).max(0.0)),
        egui::Sense::hover(),
    );
    // Le rectangle affiché suit l'espace disponible (F11) avec le ratio de
    // `DEFAULT_FIGURE_SIZE` : mise à l'échelle « contain », bornée par la
    // dimension limitante. `paint_scaled` garde la disposition à 480×360 et
    // agrandit ou réduit ensuite les formes, texte compris : aucune
    // graduation n'est recalculée pour la taille de la fenêtre (point 3 de
    // `retours-2026-09-24-bugs.md`). Le sous-Ui borne les
    // interactions sans faire grandir la fenêtre parente.
    let (fig_w, fig_h) = DEFAULT_FIGURE_SIZE;
    let scale = if fig_w > 0 && fig_h > 0 {
        (outer.width() / fig_w as f32)
            .min(outer.height() / fig_h as f32)
            .max(0.0)
    } else {
        0.0
    };
    let rect = egui::Rect::from_center_size(
        outer.center(),
        egui::vec2(fig_w as f32 * scale, fig_h as f32 * scale),
    );
    let mut figure_ui = ui.new_child(egui::UiBuilder::new().max_rect(outer));
    figure_ui.set_clip_rect(ui.clip_rect().intersect(outer));
    // Un fond transparent (« Aucune » couleur, ou opacité partielle) se voit
    // sur du blanc : sans lui, la figure prendrait la couleur de la fenêtre,
    // qui n'est pas celle de l'export. Ce blanc n'est peint **qu'ici**, sous
    // la figure, jamais dans `stokesforge-render` : l'export SVG/PNG/PDF d'un
    // graphe sans fond reste transparent (invariant 5, lu dans l'autre sens ;
    // test `un_fond_sans_couleur_donne_un_png_transparent`).
    let background = crate::state::current_plot_spec(&snapshot.tree, graph_id)
        .map(|spec| spec.background)
        .unwrap_or_default();
    if let Some(backdrop) = display_backdrop(&background) {
        figure_ui.painter().rect_filled(rect, 0.0, backdrop);
    }
    match view.paint_scaled(&figure_ui, rect, snapshot, graph_id) {
        Ok(_) => {
            graph_context_menu(ui, view, &snapshot.tree, graph_id, actions);
            annotation_interactions(ui, view, &snapshot.tree, graph_id, actions);
            if !keyboard_focus_outside_grid(ui.ctx()) {
                if let Some(spec) = crate::state::current_plot_spec(&snapshot.tree, graph_id) {
                    if let Some(series) = view.selected_series() {
                        let mut requests = Vec::new();
                        series_shortcuts(ui.ctx(), graph_id, series, &spec, actions, &mut requests);
                        for request in requests {
                            if let ObjectManagerRequest::Rename(series) = request {
                                open_series_name_editor(
                                    ui.ctx(),
                                    view,
                                    &snapshot.tree,
                                    graph_id,
                                    series,
                                );
                            }
                        }
                        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete))
                        {
                            actions.push(Action::RemovePlotObject {
                                graph: graph_id,
                                target: crate::object_manager::VisibilityTarget::Series(series),
                            });
                        }
                    }
                }
            }
            shape_properties_ui(ui.ctx(), view, graph_id, actions);
            if ui.small_button(tr!("plot-view-reset-zoom")).clicked() {
                view.reset_view();
            }
        }
        Err(err) => {
            ui.colored_label(
                egui::Color32::RED,
                crate::errors::render::error_message(&err),
            );
        }
    }
}

/// Blanc d'affichage sous une figure dont le fond est (semi-)transparent —
/// `None` sous un fond opaque, qui couvre déjà tout. Le fond transparent
/// est donc vu composé sur du blanc, comme l'encre automatique le juge
/// (`stokesforge_render::color::over_white`).
fn display_backdrop(background: &str) -> Option<egui::Color32> {
    crate::color_picker::parse_hex(background)
        .is_some_and(|[_, _, _, alpha]| alpha < 255)
        .then_some(egui::Color32::WHITE)
}

/// Menu contextuel de la figure : un clic droit (sans glisser — le glisser
/// secondaire reste le zoom élastique de `stokesforge-render-egui`) sur un
/// composant du graphe propose ses propriétés, qui ouvrent la fenêtre des
/// propriétés sur l'onglet de ce composant ([`PlotTarget::settings_page`]).
/// Le composant est lu **au moment du clic** et mémorisé : le menu reste
/// ouvert plusieurs images, pendant lesquelles le pointeur bouge.
fn graph_context_menu(
    ui: &egui::Ui,
    view: &mut GraphView,
    tree: &Tree,
    graph_id: NodeId,
    actions: &mut Vec<Action>,
) {
    let Some(response) = view.response().cloned() else {
        return;
    };
    let target_id = response.id.with("plot_target");
    let point_id = response.id.with("plot_point");
    let text_id = response.id.with("plot_text");
    if response.secondary_clicked() {
        let pos = response.interact_pointer_pos();
        let target = pos.map_or(PlotTarget::Background, |pos| {
            view.annotation_at(pos).map_or_else(
                || {
                    view.shape_at(pos)
                        .map_or_else(|| plot_target(view.component_at(pos)), PlotTarget::Object)
                },
                PlotTarget::Annotation,
            )
        });
        let point = pos.and_then(|pos| view.data_at(pos));
        // Titre, titre d'axe, légende ou entrée de légende visé (issue #53) :
        // lu au clic, comme la cible, pour que le menu reste cohérent pendant
        // que le pointeur bouge.
        let text = pos.and_then(|pos| match view.component_at(pos) {
            Some(PlotComponent::Text(target)) => Some(ContextText::Title(target)),
            Some(PlotComponent::LegendEntry { panel, series }) => view
                .last_drawn()
                .and_then(|last| last.panels.get(panel))
                .map(|panel| ContextText::LegendEntry {
                    row: panel.row,
                    column: panel.column,
                    series,
                }),
            _ => None,
        });
        ui.memory_mut(|memory| {
            memory.data.insert_temp(target_id, target);
            memory.data.insert_temp(point_id, point);
            memory.data.insert_temp(text_id, text);
        });
    }
    response.context_menu(|ui| {
        let target: PlotTarget = ui
            .memory(|memory| memory.data.get_temp(target_id))
            .unwrap_or(PlotTarget::Background);
        let point: Option<(usize, usize, f64, f64)> =
            ui.memory(|memory| memory.data.get_temp(point_id)).flatten();
        let text: Option<ContextText> = ui.memory(|memory| memory.data.get_temp(text_id)).flatten();
        ui.weak(target.label());
        ui.separator();
        // Titres, légende et entrées de légende (issue #53) : modifier le
        // texte, remettre à sa place — le pendant du double-clic et du
        // glisser, et du menu *Graph* pour la remise à zéro générale.
        if let Some(text) = text {
            let editable = !matches!(text, ContextText::Title(TextTarget::Legend { .. }));
            if editable && ui.button(tr!("action-edit-text")).clicked() {
                match text {
                    ContextText::Title(target) => {
                        open_text_editor(ui.ctx(), view, tree, graph_id, target)
                    }
                    ContextText::LegendEntry {
                        row,
                        column,
                        series,
                    } => open_legend_entry_editor(
                        ui.ctx(),
                        view,
                        tree,
                        graph_id,
                        row,
                        column,
                        series,
                    ),
                }
                ui.close();
            }
            if let ContextText::Title(target) = text {
                let moved = crate::state::current_plot_spec(tree, graph_id)
                    .is_some_and(|spec| target.anchor(&spec).is_some());
                if ui
                    .add_enabled(moved, egui::Button::new(tr!("action-reset-text-position")))
                    .on_hover_text(tr!("action-reset-text-position", "hint"))
                    .clicked()
                {
                    actions.push(Action::SetTextAnchor {
                        graph: graph_id,
                        target,
                        anchor: None,
                    });
                    ui.close();
                }
            }
            ui.separator();
        }
        // Textes libres (issue #35) : les mêmes gestes que la barre de menus
        // et le clavier (double-clic, Suppr).
        if let PlotTarget::Annotation(annotation) = target {
            if ui.button(tr!("action-edit-text")).clicked() {
                open_annotation_editor(ui.ctx(), view, tree, graph_id, annotation);
                ui.close();
            }
            if ui
                .add(
                    egui::Button::new(tr!("action-delete-text"))
                        .shortcut_text(shortcut_text_delete()),
                )
                .clicked()
            {
                actions.push(Action::RemoveAnnotation {
                    graph: graph_id,
                    target: annotation,
                });
                view.clear_annotation_selection();
                ui.close();
            }
            ui.separator();
        } else if let PlotTarget::Object(object) = target {
            // Objets graphiques (issue #76) : les mêmes gestes que la barre
            // de menus et le clavier (Suppr).
            view.select_shape(Some(object));
            shape_order_buttons(ui, view, tree, graph_id, object, actions);
            ui.separator();
        } else if ui
            .add_enabled(point.is_some(), egui::Button::new(tr!("action-add-text")))
            .on_hover_text(tr!("action-add-text", "hint"))
            .on_disabled_hover_text(tr!("action-add-text", "disabled"))
            .clicked()
        {
            if let Some(point) = point {
                add_annotation_at(ui.ctx(), view, tree, graph_id, point);
            }
            ui.close();
        }
        if ui
            .button(tr!("action-properties"))
            .on_hover_text(tr!("action-properties", "hint"))
            .clicked()
        {
            actions.push(target.open_settings(graph_id));
            ui.close();
        }
        if ui
            .button(tr!("action-reset-zoom"))
            .on_hover_text(tr!("action-reset-zoom", "hint"))
            .clicked()
        {
            view.reset_view();
            ui.close();
        }
        if ui
            .add(
                egui::Button::new(tr!("action-export-graph"))
                    .shortcut_text(ui.ctx().format_shortcut(&SHORTCUT_EXPORT_GRAPH)),
            )
            .on_hover_text(tr!("action-export-graph", "hint"))
            .clicked()
        {
            request_graph_export(Some(view), tree, graph_id, actions);
            ui.close();
        }
    });
}

// -- Textes libres (issue #35) --
//
// L'édition sur place et le glisser ne touchent pas le modèle tant qu'ils
// durent : l'aperçu vit dans la vue (`GraphView::set_annotation_preview`),
// comme le zoom, et la validation envoie **une** `Action::SetAnnotation` —
// une entrée d'historique, Échap jette l'aperçu. L'état de l'éditeur est
// rangé dans la mémoire d'egui sous l'identifiant de la vue : la même
// écriture sert la fenêtre flottante et la fenêtre détachée, sans rien
// faire transiter par `AppState`.

/// L'éditeur sur place d'un texte libre, ouvert sur une vue.
#[derive(Debug, Clone)]
struct AnnotationEditor {
    target: stokesforge_render_egui::AnnotationRef,
    draft: stokesforge_render_egui::AnnotationSpec,
    /// Le texte tel qu'il est dans le modèle ; `None` pour un texte nouveau.
    original: Option<stokesforge_render_egui::AnnotationSpec>,
    /// Donner le focus au champ à la prochaine image (ouverture).
    focus: bool,
    /// Le dernier centre et la dernière taille (écran) du texte droit rendu :
    /// un texte vidé n'est plus tracé, mais le champ doit rester où il est.
    anchor: Option<(egui::Pos2, egui::Vec2)>,
}

/// Un titre ou une entrée de légende visé par le clic droit (issue #53).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextText {
    /// Un titre, un titre d'axe ou la légende entière.
    Title(TextTarget),
    /// La ligne de légende de la série `series` du panneau `(row, column)`.
    LegendEntry {
        row: usize,
        column: usize,
        series: usize,
    },
}

/// Ce que l'éditeur sur place d'un titre modifie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TextEditTarget {
    /// Le texte saisi d'un titre.
    Title(TextTarget),
    /// Le nom, en légende, d'une série.
    LegendEntry(stokesforge_plot::SeriesRef, (usize, usize)),
}

/// L'éditeur sur place d'un titre ou d'un nom de légende (issue #53) : un
/// champ posé sur le texte. Entrée ou clic ailleurs valide, Échap annule —
/// comme l'éditeur d'un texte libre, sans barre de réglages (couleur et
/// taille d'un titre se règlent dans les propriétés du graphe).
#[derive(Debug, Clone)]
struct TextEditor {
    target: TextEditTarget,
    buffer: String,
    /// Le texte tel qu'il était à l'ouverture : valider sans l'avoir changé
    /// n'envoie rien (et ne fige pas un nom dérivé de la colonne).
    original: String,
    /// Donner le focus au champ à la prochaine image (ouverture).
    focus: bool,
    /// Centre et taille (écran) du texte tracé à l'ouverture.
    anchor: (egui::Pos2, egui::Vec2),
}

/// La mise en forme demandée par la barre, en attente du champ en édition.
fn pending_format_id() -> egui::Id {
    egui::Id::new("stokesforge_pending_text_format")
}

/// La police demandée par la barre pour le texte libre en édition : elle
/// s'applique au brouillon de l'éditeur, pas au contenu (issue #78).
fn pending_zone_font_id() -> egui::Id {
    egui::Id::new("stokesforge_pending_zone_font")
}

/// Pose sur `font` la police demandée par la barre de format, s'il y en a une.
fn apply_pending_zone_font(ctx: &egui::Context, font: &mut String) {
    if let Some(Some(name)) =
        ctx.data_mut(|data| data.remove_temp::<Option<String>>(pending_zone_font_id()))
    {
        *font = name;
    }
}

/// Pose la mise en forme demandée par la barre de format sur `buffer`, selon
/// la sélection que le champ `field` a gardée (egui la conserve quand le
/// champ perd le focus au clic sur la barre), puis rend le focus au champ avec
/// la sélection à l'intérieur des balises.
fn apply_pending_format(ctx: &egui::Context, field: egui::Id, buffer: &mut String) {
    use egui::text::{CCursor, CCursorRange};
    let Some(format) = ctx
        .data_mut(|data| {
            data.remove_temp::<Option<crate::text_format::TextFormat>>(pending_format_id())
        })
        .flatten()
    else {
        return;
    };
    let mut state = egui::TextEdit::load_state(ctx, field).unwrap_or_default();
    let length = buffer.chars().count();
    let selection = state.cursor.char_range().map_or(length..length, |range| {
        let [start, end] = range.sorted_cursors();
        start.index.0..end.index.0
    });
    let done = crate::text_format::apply(buffer, selection, &format);
    *buffer = done.text;
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(done.selection.start),
        CCursor::new(done.selection.end),
    )));
    state.store(ctx, field);
    ctx.memory_mut(|memory| memory.request_focus(field));
}

fn text_editor_id(view: &GraphView) -> egui::Id {
    view.id().with("text_editor")
}

/// Ouvre l'éditeur sur le texte d'un titre ; sans effet si le titre n'est
/// pas tracé ou n'existe plus (la légende n'a pas de texte à elle).
fn open_text_editor(
    ctx: &egui::Context,
    view: &GraphView,
    tree: &Tree,
    graph: NodeId,
    target: TextTarget,
) {
    let Some(original) =
        crate::state::current_plot_spec(tree, graph).and_then(|spec| target.text(&spec))
    else {
        return;
    };
    let Some((center, size, _)) = view.text_screen_box(target) else {
        return;
    };
    start_text_editor(
        ctx,
        view,
        TextEditTarget::Title(target),
        original,
        (center, size),
    );
}

/// Ouvre l'éditeur sur le nom, en légende, d'une série.
fn open_legend_entry_editor(
    ctx: &egui::Context,
    view: &GraphView,
    tree: &Tree,
    graph: NodeId,
    row: usize,
    column: usize,
    series: usize,
) {
    let Some(spec) = crate::state::current_plot_spec(tree, graph) else {
        return;
    };
    let Some(panel) = spec
        .panels
        .iter()
        .position(|panel| panel.row == row && panel.column == column)
    else {
        return;
    };
    let address = stokesforge_plot::SeriesRef::new(panel, series);
    let Some(spec_series) = spec.series_at(address) else {
        return;
    };
    let Some(anchor) = view.legend_entry_screen_box(row, column, series) else {
        return;
    };
    let original = crate::state::series_list_label(tree, spec_series);
    start_text_editor(
        ctx,
        view,
        TextEditTarget::LegendEntry(address, (row, column)),
        original,
        anchor,
    );
}

fn open_series_name_editor(
    ctx: &egui::Context,
    view: &GraphView,
    tree: &Tree,
    graph: NodeId,
    series: stokesforge_plot::SeriesRef,
) {
    let Some(spec) = crate::state::current_plot_spec(tree, graph) else {
        return;
    };
    let Some(value) = spec.series_at(series) else {
        return;
    };
    let panel = &spec.panels[series.panel];
    let anchor = view
        .legend_entry_screen_box(panel.row, panel.column, series.index)
        .unwrap_or((
            view.display_rect()
                .map_or(ctx.content_rect().center(), |r| r.center()),
            egui::vec2(180.0, 24.0),
        ));
    start_text_editor(
        ctx,
        view,
        TextEditTarget::LegendEntry(series, (panel.row, panel.column)),
        series_list_label(tree, value),
        anchor,
    );
}

fn start_text_editor(
    ctx: &egui::Context,
    view: &GraphView,
    target: TextEditTarget,
    original: String,
    anchor: (egui::Pos2, egui::Vec2),
) {
    let editor = TextEditor {
        target,
        buffer: original.clone(),
        original,
        focus: true,
        anchor,
    };
    ctx.data_mut(|data| data.insert_temp(text_editor_id(view), editor));
}

/// Peint l'éditeur d'un titre ou d'un nom de légende, et traduit sa
/// validation en **une** action.
fn text_editor_ui(
    ctx: &egui::Context,
    view: &GraphView,
    graph: NodeId,
    mut editor: TextEditor,
    actions: &mut Vec<Action>,
) {
    let id = text_editor_id(view);
    apply_pending_format(ctx, id.with("field"), &mut editor.buffer);
    let (center, size) = editor.anchor;
    let field = egui::Rect::from_center_size(
        center,
        egui::vec2((size.x + 32.0).max(180.0), (size.y + 8.0).max(24.0)),
    );
    let mut accept = false;
    let field_id = id.with("field");
    let area = egui::Area::new(id.with("area"))
        .order(egui::Order::Foreground)
        .fixed_pos(field.min)
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut editor.buffer)
                        .id(field_id)
                        .desired_width(field.width())
                        .hint_text(tr!("plot-text-placeholder")),
                );
                if editor.focus {
                    response.request_focus();
                    editor.focus = false;
                }
                if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    accept = true;
                }
            });
        });
    let cancel = ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let outside = ctx
        .input(|input| {
            input
                .pointer
                .any_pressed()
                .then(|| input.pointer.interact_pos())
                .flatten()
        })
        .is_some_and(|pos| {
            !area.response.rect.contains(pos)
                && !crate::toolbar::over_format_bar(ctx, pos)
                && ctx
                    .layer_id_at(pos)
                    .is_none_or(|layer| layer.order != egui::Order::Foreground)
        });
    if cancel {
        ctx.data_mut(|data| data.remove::<TextEditor>(id));
        return;
    }
    if accept || outside {
        if editor.buffer != editor.original {
            actions.push(match editor.target {
                TextEditTarget::Title(target) => Action::SetTextContent {
                    graph,
                    target,
                    text: editor.buffer.clone(),
                },
                TextEditTarget::LegendEntry(series, _) => Action::SetLegendEntryName {
                    graph,
                    series,
                    name: editor.buffer.clone(),
                },
            });
        }
        ctx.data_mut(|data| data.remove::<TextEditor>(id));
        return;
    }
    ctx.data_mut(|data| data.insert_temp(id, editor));
}

fn annotation_editor_id(view: &GraphView) -> egui::Id {
    view.id().with("annotation_editor")
}

/// Ouvre l'éditeur sur un texte existant ; sans effet si le texte n'existe
/// plus dans le modèle.
fn open_annotation_editor(
    ctx: &egui::Context,
    view: &mut GraphView,
    tree: &Tree,
    graph: NodeId,
    target: stokesforge_render_egui::AnnotationRef,
) {
    let Some(original) =
        crate::state::current_plot_spec(tree, graph).and_then(|spec| target.get(&spec).cloned())
    else {
        return;
    };
    start_annotation_editor(ctx, view, target, original.clone(), Some(original));
}

/// Ajoute un texte au point de données `(row, column, x, y)` et ouvre
/// l'éditeur dessus : rien n'entre dans le modèle avant validation.
fn add_annotation_at(
    ctx: &egui::Context,
    view: &mut GraphView,
    tree: &Tree,
    graph: NodeId,
    (row, column, x, y): (usize, usize, f64, f64),
) {
    let Some(target) = crate::state::current_plot_spec(tree, graph)
        .and_then(|spec| stokesforge_render_egui::AnnotationRef::next_in(&spec, row, column))
    else {
        return;
    };
    let draft = stokesforge_render_egui::AnnotationSpec::new(String::new(), x, y);
    start_annotation_editor(ctx, view, target, draft, None);
}

fn start_annotation_editor(
    ctx: &egui::Context,
    view: &mut GraphView,
    target: stokesforge_render_egui::AnnotationRef,
    draft: stokesforge_render_egui::AnnotationSpec,
    original: Option<stokesforge_render_egui::AnnotationSpec>,
) {
    view.set_annotation_preview(Some((target, draft.clone())));
    let editor = AnnotationEditor {
        target,
        draft,
        original,
        focus: true,
        anchor: None,
    };
    ctx.data_mut(|data| data.insert_temp(annotation_editor_id(view), editor));
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

/// Signaux de la vue, éditeur sur place et Suppr — après la peinture de la
/// figure, dans la fenêtre flottante comme dans la fenêtre détachée.
/// Les trois entrées d'un objet graphique sélectionné : premier plan,
/// arrière-plan, suppression (barre de menus et menu contextuel).
fn shape_order_buttons(
    ui: &mut egui::Ui,
    view: &mut GraphView,
    tree: &Tree,
    graph: NodeId,
    object: stokesforge_render_egui::ShapeRef,
    actions: &mut Vec<Action>,
) {
    let count = crate::state::current_plot_spec(tree, graph)
        .and_then(|spec| {
            spec.panels
                .iter()
                .find(|panel| panel.row == object.row && panel.column == object.column)
                .map(|panel| panel.shapes.len())
        })
        .unwrap_or(0);
    for order in [ShapeOrder::ToFront, ShapeOrder::ToBack] {
        let label = match order {
            ShapeOrder::ToFront => tr!("action-bring-to-front"),
            ShapeOrder::ToBack => tr!("action-send-to-back"),
        };
        let hint = match order {
            ShapeOrder::ToFront => tr!("action-bring-to-front", "hint"),
            ShapeOrder::ToBack => tr!("action-send-to-back", "hint"),
        };
        if ui.button(label).on_hover_text(hint).clicked() {
            actions.push(Action::ReorderShape {
                graph,
                target: object,
                order,
            });
            // Le rang change : la sélection suit l'objet.
            let index = match order {
                ShapeOrder::ToFront => count.saturating_sub(1),
                ShapeOrder::ToBack => 0,
            };
            view.select_shape(Some(stokesforge_render_egui::ShapeRef { index, ..object }));
            ui.close();
        }
    }
    if ui
        .add(egui::Button::new(tr!("action-delete-object")).shortcut_text(shortcut_text_delete()))
        .clicked()
    {
        actions.push(Action::RemoveShape {
            graph,
            target: object,
        });
        view.select_shape(None);
        ui.close();
    }
}

/// Les mêmes entrées, grisées, quand aucun objet n'est sélectionné.
fn disabled_shape_order_buttons(ui: &mut egui::Ui) {
    ui.add_enabled(false, egui::Button::new(tr!("action-bring-to-front")))
        .on_disabled_hover_text(tr!("action-bring-to-front", "disabled"));
    ui.add_enabled(false, egui::Button::new(tr!("action-send-to-back")))
        .on_disabled_hover_text(tr!("action-send-to-back", "disabled"));
    ui.add_enabled(
        false,
        egui::Button::new(tr!("action-delete-object")).shortcut_text(shortcut_text_delete()),
    );
}

fn annotation_interactions(
    ui: &egui::Ui,
    view: &mut GraphView,
    tree: &Tree,
    graph: NodeId,
    actions: &mut Vec<Action>,
) {
    for event in view.take_events() {
        match event {
            stokesforge_render_egui::GraphEvent::PlaceText { row, column, x, y } => {
                add_annotation_at(ui.ctx(), view, tree, graph, (row, column, x, y));
                view.set_tool(stokesforge_render_egui::GraphTool::Pointer);
            }
            stokesforge_render_egui::GraphEvent::EditAnnotation(target) => {
                open_annotation_editor(ui.ctx(), view, tree, graph, target);
            }
            stokesforge_render_egui::GraphEvent::AnnotationMoved { target, annotation } => {
                actions.push(Action::SetAnnotation {
                    graph,
                    target,
                    annotation: Box::new(annotation),
                });
            }
            stokesforge_render_egui::GraphEvent::ShapeChanged { target, shape } => {
                actions.push(Action::SetShape {
                    graph,
                    target,
                    shape: Box::new(shape),
                });
            }
            stokesforge_render_egui::GraphEvent::EditText(target) => {
                open_text_editor(ui.ctx(), view, tree, graph, target);
            }
            stokesforge_render_egui::GraphEvent::EditLegendEntry {
                row,
                column,
                series,
            } => open_legend_entry_editor(ui.ctx(), view, tree, graph, row, column, series),
            stokesforge_render_egui::GraphEvent::OpenAxisProperties { panel, side } => {
                actions.push(
                    plot_target(Some(PlotComponent::Axis { panel, side })).open_settings(graph),
                );
            }
            stokesforge_render_egui::GraphEvent::OpenSeriesProperties(series) => {
                actions.push(Action::OpenObjectProperties {
                    graph,
                    target: crate::object_manager::VisibilityTarget::Series(series),
                });
            }
            stokesforge_render_egui::GraphEvent::OpenShapeProperties(shape) => {
                if let Some(spec) = crate::state::current_plot_spec(tree, graph) {
                    open_shape_properties(ui.ctx(), view, &spec, shape);
                }
            }
            stokesforge_render_egui::GraphEvent::TextMoved { target, anchor } => {
                actions.push(Action::SetTextAnchor {
                    graph,
                    target,
                    anchor: Some(anchor),
                });
            }
        }
    }
    // L'éditeur d'un titre ou d'un nom de légende, s'il y en a un d'ouvert.
    let text_editor: Option<TextEditor> = ui.ctx().data(|data| data.get_temp(text_editor_id(view)));
    if let Some(editor) = text_editor {
        text_editor_ui(ui.ctx(), view, graph, editor, actions);
        return;
    }
    let id = annotation_editor_id(view);
    let editor: Option<AnnotationEditor> = ui.ctx().data(|data| data.get_temp(id));
    match editor {
        Some(editor) => annotation_editor_ui(ui.ctx(), view, graph, editor, actions),
        None => {
            // Suppr retire le texte sélectionné, quand aucun champ n'a le
            // focus (sinon la touche appartient à la saisie).
            let typing = keyboard_focus_outside_grid(ui.ctx());
            if let Some(target) = view.selected_annotation() {
                if !typing
                    && ui.ctx().input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                    })
                {
                    actions.push(Action::RemoveAnnotation { graph, target });
                    view.clear_annotation_selection();
                }
            } else if let Some(target) = view.selected_shape() {
                // Objets graphiques (issue #76) : même geste.
                if !typing
                    && ui.ctx().input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                    })
                {
                    actions.push(Action::RemoveShape { graph, target });
                    view.select_shape(None);
                }
            }
        }
    }
}

/// Peint l'éditeur sur place : le champ de texte posé sur le texte, et
/// au-dessus une barre couleur / taille / rotation. Entrée ou clic ailleurs
/// valide, Échap annule.
fn annotation_editor_ui(
    ctx: &egui::Context,
    view: &mut GraphView,
    graph: NodeId,
    mut editor: AnnotationEditor,
    actions: &mut Vec<Action>,
) {
    let id = annotation_editor_id(view);
    apply_pending_format(ctx, id.with("field"), &mut editor.draft.text);
    apply_pending_zone_font(ctx, &mut editor.draft.font);
    // Le cadre du texte rendu ; à défaut (texte neuf, ou vidé donc plus
    // tracé), le dernier connu, puis le centre de la figure.
    if let Some((center, size, _)) = view.annotation_screen_box(editor.target) {
        editor.anchor = Some((center, size));
    }
    let (center, size) = editor.anchor.unwrap_or_else(|| {
        let center = view
            .display_rect()
            .map_or(egui::Pos2::ZERO, |rect| rect.center());
        (center, egui::Vec2::ZERO)
    });
    // Le champ s'écrit dans le cadre du rendu (#37) : centré, au corps du
    // texte, droit même si le texte est tourné.
    let field = stokesforge_render_egui::annotation_field(
        center,
        size,
        editor.draft.effective_size(),
        view.display_scale().unwrap_or(1.0),
    );

    let (project, user): (stokesforge_plot::SavedColors, stokesforge_plot::SavedColors) = ctx
        .data(|data| data.get_temp(saved_colors_id()))
        .unwrap_or_default();
    let mut env = PickerEnv {
        project,
        user,
        requests: Vec::new(),
    };

    let mut accept = false;
    let toolbar = egui::Area::new(id.with("toolbar"))
        .order(egui::Order::Foreground)
        .fixed_pos(field.rect.min - egui::vec2(0.0, 36.0))
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    crate::color_picker::color_icon_button(
                        ui,
                        (crate::icons::Icon::TextColor, &tr!("format-color")),
                        ("annotation_color", id),
                        &mut editor.draft.color,
                        ColorUsage::Text,
                        Some(&tr!("plot-color-auto-contrast")),
                        &mut env,
                    );
                    ui.label(tr!("plot-text-size"));
                    let (low, high) = stokesforge_render_egui::AnnotationSpec::SIZE_RANGE;
                    ui.add(
                        egui::DragValue::new(&mut editor.draft.size)
                            .range(low..=high)
                            .speed(0.2),
                    );
                    ui.label(tr!("plot-text-rotation"));
                    ui.add(
                        egui::DragValue::new(&mut editor.draft.rotation)
                            .range(-180.0..=180.0)
                            .speed(1.0),
                    );
                    if ui
                        .button(tr!("plot-text-ok"))
                        .on_hover_text(tr!("plot-text-ok", "hint"))
                        .clicked()
                    {
                        accept = true;
                    }
                });
            });
        });
    // Tant qu'on tape, le champ droit couvre le texte ; dès qu'il perd le
    // focus (on règle la rotation ou la taille), il devient transparent et
    // l'aperçu — tourné — se voit (#36).
    let field_id = id.with("field");
    let typing = editor.focus || ctx.memory(|memory| memory.has_focus(field_id));
    let text = egui::Area::new(id.with("text"))
        .order(egui::Order::Foreground)
        .fixed_pos(field.rect.min)
        .constrain(true)
        .show(ctx, |ui| {
            let visuals = ui.visuals().clone();
            let outline = if typing {
                visuals.selection.stroke
            } else {
                visuals.widgets.inactive.bg_stroke
            };
            if typing {
                ui.painter()
                    .rect_filled(field.rect, 2.0, visuals.extreme_bg_color);
            }
            ui.painter()
                .rect_stroke(field.rect, 2.0, outline, egui::StrokeKind::Inside);
            let padding = (field.font_size * 0.3).round() as i8;
            let response = ui.put(
                field.rect,
                egui::TextEdit::singleline(&mut editor.draft.text)
                    .id(field_id)
                    .font(egui::FontId::proportional(field.font_size))
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::same(padding))
                    .desired_width(field.rect.width())
                    .horizontal_align(egui::Align::Center)
                    .vertical_align(egui::Align::Center)
                    .text_color(if typing {
                        visuals.text_color()
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .hint_text(tr!("plot-text-placeholder")),
            );
            if editor.focus {
                response.request_focus();
                editor.focus = false;
            }
            if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                accept = true;
            }
        });

    actions.extend(env.requests.into_iter().map(color_request_action));

    // Échap : le sélecteur de couleur ouvert le consomme d'abord pour lui.
    let cancel = ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    // Un clic hors de l'éditeur (et hors de toute couche surgissante, comme
    // le sélecteur de couleur) valide.
    let pressed_at = ctx.input(|input| {
        input
            .pointer
            .any_pressed()
            .then(|| input.pointer.interact_pos())
            .flatten()
    });
    let outside = pressed_at.is_some_and(|pos| {
        !toolbar.response.rect.contains(pos)
            && !text.response.rect.contains(pos)
            && !crate::toolbar::over_format_bar(ctx, pos)
            && ctx
                .layer_id_at(pos)
                .is_none_or(|layer| layer.order != egui::Order::Foreground)
    });

    if cancel {
        view.set_annotation_preview(None);
        ctx.data_mut(|data| data.remove::<AnnotationEditor>(id));
        return;
    }
    if accept || outside {
        commit_annotation_editor(view, graph, &editor, actions);
        ctx.data_mut(|data| data.remove::<AnnotationEditor>(id));
        return;
    }
    view.set_annotation_preview(Some((editor.target, editor.draft.clone())));
    ctx.data_mut(|data| data.insert_temp(id, editor));
}

/// Valide l'éditeur : une seule action, et seulement si quelque chose a
/// changé. Un texte vidé est retiré (un texte vide n'est pas tracé, il ne
/// pourrait plus être désigné).
fn commit_annotation_editor(
    view: &mut GraphView,
    graph: NodeId,
    editor: &AnnotationEditor,
    actions: &mut Vec<Action>,
) {
    let empty = editor.draft.text.trim().is_empty();
    match (&editor.original, empty) {
        (None, true) => view.set_annotation_preview(None),
        (Some(_), true) => {
            actions.push(Action::RemoveAnnotation {
                graph,
                target: editor.target,
            });
            view.hold_preview();
        }
        (Some(original), false) if *original == editor.draft => {
            view.set_annotation_preview(None);
        }
        _ => {
            actions.push(Action::SetAnnotation {
                graph,
                target: editor.target,
                annotation: Box::new(editor.draft.clone()),
            });
            view.hold_preview();
        }
    }
}

/// Demande où exporter le graphe, puis confie l'export à
/// [`AppState::dispatch`] (issue #34). Le spec exporté est celui que la vue
/// montre, zoom compris ([`GraphView::framed_spec`]) ; sans vue (graphe pas
/// encore peint), le cadrage enregistré. Le format se lit sur l'extension
/// choisie.
fn request_graph_export(
    view: Option<&GraphView>,
    tree: &Tree,
    graph: NodeId,
    actions: &mut Vec<Action>,
) {
    let Some(spec) = crate::state::current_plot_spec(tree, graph) else {
        return;
    };
    let spec = match view {
        Some(view) => view.framed_spec(&spec),
        None => spec,
    };
    if let Some(path) = rfd::FileDialog::new()
        .set_title(tr!("export-dialog-title"))
        .add_filter("SVG", &["svg"])
        .add_filter("PDF", &["pdf"])
        .add_filter("PNG", &["png"])
        .set_file_name(export_file_name(tree, graph))
        .save_file()
    {
        actions.push(Action::ExportGraph {
            path,
            spec: Box::new(spec),
        });
    }
}

/// Traduit le composant désigné par le rendu en cible d'interface. Seul le
/// premier panneau est éditable (pas de multi-panneaux), l'indice de
/// panneau est donc ignoré.
fn plot_target(component: Option<PlotComponent>) -> PlotTarget {
    let axis = |side| match side {
        AxisSide::Bottom => PlotAxis::Bottom,
        AxisSide::Left => PlotAxis::Left,
        AxisSide::Right => PlotAxis::Right,
    };
    match component {
        None => PlotTarget::Background,
        Some(PlotComponent::Title) => PlotTarget::Title,
        Some(PlotComponent::PlotArea { .. }) => PlotTarget::PlotArea,
        Some(PlotComponent::Axis { side, .. }) => PlotTarget::Axis(axis(side)),
        Some(PlotComponent::AxisTitle { side, .. }) => PlotTarget::AxisTitle(axis(side)),
        // Le menu contextuel lit le texte libre par `GraphView::annotation_at`,
        // qui donne son adresse dans le spec ; ce repli ne sert pas.
        Some(PlotComponent::Annotation { .. }) => PlotTarget::PlotArea,
        // Idem pour un objet graphique (`GraphView::shape_at`).
        Some(PlotComponent::Shape { .. }) => PlotTarget::PlotArea,
        // Titres et légende (issue #53) : leurs propriétés sont celles du
        // composant, comme avant qu'ils soient désignables en propre.
        Some(PlotComponent::Text(target)) => match target {
            TextTarget::Title | TextTarget::PanelTitle { .. } => PlotTarget::Title,
            TextTarget::AxisTitle { side, .. } => PlotTarget::AxisTitle(match side {
                stokesforge_plot::AxisSide::Bottom => PlotAxis::Bottom,
                stokesforge_plot::AxisSide::Left => PlotAxis::Left,
                stokesforge_plot::AxisSide::Right | stokesforge_plot::AxisSide::Top => {
                    PlotAxis::Right
                }
            }),
            TextTarget::Legend { .. } => PlotTarget::Legend,
        },
        Some(PlotComponent::LegendEntry { .. }) => PlotTarget::Legend,
    }
}

/// Contenu d'un [`Tab::Matrix`] — **espace réservé documenté**. L'édition de
/// matrice est explicitement hors périmètre de cette tâche
/// (`docs/application.md`) : la fenêtre s'ouvre et se focalise exactement
/// comme les trois autres natures, mais ne montre qu'un message plutôt qu'un
/// éditeur. `stokesforge-core::NodeKind::Matrix` n'a aujourd'hui aucun contenu
/// (juste des métadonnées de fenêtre) — il n'y a donc littéralement rien
/// d'autre à afficher tant que cette nature n'a pas de valeurs.
///
/// Une seule écriture pour la fenêtre flottante et la fenêtre détachée : elle
/// ne lit que l'arbre, que les deux ont sous la main (`state.tree()` d'un
/// côté, l'instantané du viewport de l'autre).
fn matrix_contents(ui: &mut egui::Ui, _tree: &Tree, _matrix: NodeId) {
    ui.centered_and_justified(|ui| {
        ui.weak(tr!("matrix-unavailable"));
    });
}

/// Contenu d'un [`Tab::Note`] : un éditeur multi-lignes, sur le même principe
/// de saisie différée que [`cell_widget`] — le tampon ([`crate::state::
/// NoteEdit`]) n'atteint le modèle qu'à la perte de focus, jamais à chaque
/// frappe (sinon l'historique compterait une entrée par caractère). Échap
/// abandonne la saisie sans rien écrire, comme pour une cellule.
fn note_contents(ui: &mut egui::Ui, state: &mut AppState, actions: &mut Vec<Action>, note: NodeId) {
    let Some(current_name) = state.tree().node(note).map(|node| node.name().to_string()) else {
        ui.centered_and_justified(|ui| {
            ui.weak(tr!("note-deleted"));
        });
        return;
    };
    ui.heading(current_name);
    ui.separator();

    let editing_here = state.note_editing().is_some_and(|edit| edit.note == note);
    let mut buffer = if editing_here {
        state
            .note_editing()
            .map(|edit| edit.buffer.clone())
            .unwrap_or_default()
    } else {
        state.note_text(note).unwrap_or_default()
    };

    let response = ui.add(
        egui::TextEdit::multiline(&mut buffer)
            .desired_rows(20)
            .desired_width(f32::INFINITY),
    );

    if response.gained_focus() && !editing_here {
        state.begin_note_edit(note);
    }
    if state.note_editing().is_some_and(|edit| edit.note == note) {
        if let Some(edit) = state.note_editing_mut() {
            edit.buffer = buffer;
        }
    }

    if response.lost_focus() {
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            state.cancel_note_edit();
        } else {
            let pending = state
                .note_editing()
                .filter(|edit| edit.note == note)
                .map(|edit| edit.buffer.clone());
            if let Some(text) = pending {
                state.cancel_note_edit();
                actions.push(Action::SetNoteText { note, text });
            }
        }
    }
}

#[cfg(test)]
mod formula_memo_tests {
    use stokesforge_render_egui::formula;

    /// Chaque exemple du mémo de syntaxe compile vraiment en Typst : un mémo
    /// qui montre une syntaxe fausse enseignerait une erreur.
    #[test]
    fn chaque_exemple_du_memo_compile() {
        let fonts = formula::resolve_fonts(&stokesforge_render_egui::FontChoice::default())
            .expect("polices");
        for (source, _) in super::plot_settings::formula_memo_rows() {
            for example in source.split("  ") {
                let (sans, math) = fonts.clone();
                formula::compile(example, 14.0, [0, 0, 0, 255], sans, math)
                    .unwrap_or_else(|error| panic!("{example:?} ne compile pas : {error}"));
            }
        }
    }
}

#[cfg(test)]
mod graph_layout_tests {
    use super::grid::{
        grid_contents, grid_vertical_bar_scale, role_label, show_horizontally_scrollable_grid,
        stacked_header_height, HEADER_SUBROW_HEIGHT,
    };
    use super::plot_settings::theme_editor;
    use super::window_chrome::tests::find_icon_centers;
    use super::window_chrome::{show_floating_window, WINDOW_CONTENT_MARGIN, WINDOW_HEADER_HEIGHT};
    use super::window_placement::GRAPH_WINDOW_SIZE;
    use super::*;
    use egui_extras::{Column as TableColumn, TableBuilder};
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

    /// Issue #8 : un fichier lâché sur un classeur est réclamé par ce classeur
    /// (et seulement lui), un fichier lâché ailleurs n'a pas de classeur, et un
    /// survol sans dépôt n'émet aucune action. Sans fenêtre : les événements
    /// `hovered_files`/`dropped_files` sont injectés dans l'entrée d'egui.
    #[test]
    fn un_depot_est_reclame_par_le_classeur_sous_le_curseur() {
        let (session, _graph) = graphe();
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let book = state.books()[0].0;
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
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
        let file: egui::DroppedFileHandle = std::sync::Arc::new(FakeFile("mesure.csv".into()));
        // Un classeur occupe (100,100)-(400,400) ; le curseur est dedans ou dehors.
        let frame = |pointer: egui::Pos2, dropped: bool| -> Option<Action> {
            let mut input = egui::RawInput {
                screen_rect: Some(screen),
                events: vec![egui::Event::PointerMoved(pointer)],
                ..Default::default()
            };
            if dropped {
                input.dropped_files = vec![file.clone()];
            }
            let mut result = None;
            let mut output = context.run_ui(input, |ui| {
                let ctx = ui.ctx().clone();
                begin_file_drop(&ctx);
                egui::Area::new(egui::Id::new("faux_classeur"))
                    .fixed_pos(egui::pos2(100.0, 100.0))
                    .show(&ctx, |ui| {
                        ui.allocate_ui(egui::vec2(300.0, 300.0), |ui| {
                            ui.set_min_size(egui::vec2(300.0, 300.0));
                            claim_file_drop(ui, book);
                        });
                    });
                result = finish_file_drop(&ctx);
            });
            output.textures_delta.clear();
            result
        };
        // Deux images de chauffe (l'Area n'est interactive qu'après la première).
        frame(egui::pos2(200.0, 200.0), false);
        frame(egui::pos2(200.0, 200.0), false);
        assert_eq!(frame(egui::pos2(200.0, 200.0), false), None, "pas de dépôt");
        let on_book = frame(egui::pos2(200.0, 200.0), true);
        assert!(
            matches!(&on_book, Some(Action::DropFiles { target_book: Some(b), paths }) if *b == book && paths.len() == 1),
            "{on_book:?}"
        );
        let elsewhere = frame(egui::pos2(700.0, 500.0), true);
        assert!(
            matches!(
                &elsewhere,
                Some(Action::DropFiles {
                    target_book: None,
                    ..
                })
            ),
            "{elsewhere:?}"
        );
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

    /// La ligne « Metadata » est affichée par défaut, se masque et se réaffiche
    /// (réglage de vue partagé par tous les viewports), et l'en-tête empilé
    /// perd exactement une sous-ligne quand elle est masquée.
    #[test]
    fn la_ligne_metadata_se_masque_et_se_reaffiche() {
        let context = egui::Context::default();
        assert!(metadata_row_visible(&context));
        set_metadata_row_visible(&context, false);
        assert!(!metadata_row_visible(&context));
        assert_eq!(
            stacked_header_height(true) - stacked_header_height(false),
            HEADER_SUBROW_HEIGHT
        );
        set_metadata_row_visible(&context, true);
        assert!(metadata_row_visible(&context));
    }

    /// #17 : la grille reste peinte sous un dialogue (Propriétés du graphe,
    /// import…) ; tant qu'un champ texte y a le focus, Retour arrière et
    /// Suppr lui appartiennent — la grille ne doit ni les consommer ni vider
    /// la cellule sélectionnée.
    #[test]
    fn un_champ_texte_focalise_garde_retour_arriere_et_suppr() {
        let mut project = Project::new("Clavier").unwrap();
        let root = project.root();
        let book = project.add_empty_workbook(root, None).unwrap();
        let sheet = project.add_worksheet(book, None).unwrap();
        let column = project
            .add_column(sheet, None, None, Column::from_f64(&[1.0, 2.0]))
            .unwrap();
        let session = Session::spawn(project, None).unwrap();
        let mut state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        state.select_cell(sheet, column, 0);
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut titre = String::from("Titre");
        let mut actions = Vec::new();
        for frame in 0..3 {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                ..Default::default()
            };
            if frame == 2 {
                for key in [egui::Key::Backspace, egui::Key::Delete] {
                    input.events.push(egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
            }
            let mut output = context.run_ui(input, |ui| {
                let field = egui::Id::new("champ_dialogue");
                egui::Area::new(egui::Id::new("dialogue")).show(ui.ctx(), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut titre).id(field));
                });
                ui.ctx().memory_mut(|memory| memory.request_focus(field));
                actions.clear();
                grid_contents(ui, &mut state, &mut actions, sheet, None);
            });
            output.textures_delta.clear();
        }
        assert!(
            !actions.iter().any(|action| matches!(
                action,
                Action::ClearSelectedCells { .. } | Action::DeleteSelectedCells { .. }
            )),
            "la grille a volé Retour arrière/Suppr au champ texte : {actions:?}",
        );
    }

    /// #11 : l'onglet « Thème » se dessine (thèmes, fond, texte, axes) sans
    /// rien modifier tant qu'on n'agit pas.
    #[test]
    fn l_onglet_theme_se_dessine_sans_rien_modifier() {
        let mut spec = stokesforge_plot::PlotSpec::empty("Figure")
            .with_panels(stokesforge_plot::PanelSpec::at(0, 0));
        spec.set_text_color("#336699");
        let avant = spec.clone();
        let mut env = PickerEnv {
            project: Default::default(),
            user: Default::default(),
            requests: Vec::new(),
        };
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut commit = true;
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            commit = theme_editor(ui, &mut spec, &mut env);
        });
        output.textures_delta.clear();
        assert!(!commit);
        assert_eq!(spec, avant);
        assert!(!output.shapes.is_empty());
    }

    /// #12 : un fond sans couleur ou semi-transparent s'affiche sur du
    /// blanc ; un fond opaque n'a pas besoin de dessous.
    #[test]
    fn le_blanc_d_affichage_ne_double_que_les_fonds_transparents() {
        assert_eq!(
            display_backdrop(stokesforge_plot::TRANSPARENT),
            Some(egui::Color32::WHITE)
        );
        assert_eq!(display_backdrop("#12345680"), Some(egui::Color32::WHITE));
        assert_eq!(display_backdrop("#123456"), None);
    }

    // Un classeur coupé par le bas de la vue : sa bande d'onglets de
    // feuille, posée au bas de la fenêtre, ne doit pas être dessinée sous la
    // vue (sur la barre de défilement ou la console). `egui::Panel` remplace
    // le rognage de son parent au lieu de le combiner : c'est lui qui
    // faisait déborder les onglets.
    #[test]
    fn un_classeur_coupe_par_le_bas_de_la_vue_ne_dessine_rien_dessous() {
        let (session, _graph) = graphe();
        let mut state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let book = state
            .project_tree()
            .iter()
            .find_map(|node| match node {
                ProjectTreeNode::Window {
                    id,
                    kind: WindowKind::Book,
                    ..
                } => Some(*id),
                _ => None,
            })
            .unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let id = egui::Id::new("classeur_coupe");
        let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1400.0, 400.0));
        let window_rect =
            egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(500.0, 400.0));
        let mut actions = Vec::new();
        let mut output = None;
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1000.0),
                )),
                ..Default::default()
            };
            let mut frame = context.run_ui(input, |ui| {
                show_floating_window(
                    ui.ctx(),
                    id,
                    window_rect,
                    viewport,
                    false,
                    |ui| book_contents(ui, &mut state, &mut actions, book),
                    |_, _| {},
                );
            });
            frame.textures_delta.clear();
            output = Some(frame);
        }
        let output = output.unwrap();
        assert!(!output.shapes.is_empty());
        // Une forme dont le rognage est vide (onglets entièrement sous la
        // vue) ne dessine rien : seule compte une découpe non vide.
        for painted in output
            .shapes
            .iter()
            .filter(|painted| painted.clip_rect.is_positive())
        {
            assert!(
                viewport.contains_rect(painted.clip_rect),
                "une forme du classeur est découpée hors de la vue : {:?}",
                painted.clip_rect,
            );
        }
    }

    /// Rectangles pleins peints par `shapes`, à plat, avec leur rognage.
    fn rectangles_et_rognages(
        shapes: &[egui::epaint::ClippedShape],
    ) -> Vec<(egui::Rect, egui::Rect)> {
        fn visiter(
            shape: &egui::Shape,
            clip: egui::Rect,
            sortie: &mut Vec<(egui::Rect, egui::Rect)>,
        ) {
            match shape {
                egui::Shape::Rect(rect) => sortie.push((rect.rect, clip)),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        visiter(shape, clip, sortie);
                    }
                }
                _ => {}
            }
        }
        let mut sortie = Vec::new();
        for painted in shapes {
            visiter(&painted.shape, painted.clip_rect, &mut sortie);
        }
        sortie
    }

    /// Écran de 300 x 320 : `book_contents` d'un classeur de 12 colonnes
    /// (bien plus larges que l'écran) et 200 lignes, peint six images, la
    /// cinquième (indice 4) recevant `events`. Renvoie l'écran et les
    /// rectangles (avec rognage) de la dernière image.
    fn peindre_classeur_large(
        events: Vec<egui::Event>,
    ) -> (egui::Rect, Vec<(egui::Rect, egui::Rect)>) {
        let mut project = Project::new("Large").unwrap();
        let root = project.root();
        let book = project.add_empty_workbook(root, None).unwrap();
        let sheet = project.add_worksheet(book, None).unwrap();
        let valeurs: Vec<f64> = (0..200).map(f64::from).collect();
        for _ in 0..12 {
            project
                .add_column(sheet, None, None, Column::from_f64(&valeurs))
                .unwrap();
        }
        let session = Session::spawn(project, None).unwrap();
        let mut state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&context);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 320.0));
        let mut actions = Vec::new();
        let mut derniere = None;
        for frame_index in 0..6 {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events: if frame_index == 4 {
                    events.clone()
                } else {
                    Vec::new()
                },
                ..Default::default()
            };
            let mut frame = context.run_ui(input, |ui| {
                book_contents(ui, &mut state, &mut actions, book);
            });
            frame.textures_delta.clear();
            derniere = Some(frame);
        }
        (screen, rectangles_et_rognages(&derniere.unwrap().shapes))
    }

    /// La poignée de la barre verticale (pas sa piste) : mince, courte, dans la bande de
    /// droite de l'écran.
    fn poignees_de_barre(
        screen: egui::Rect,
        rectangles: &[(egui::Rect, egui::Rect)],
    ) -> Vec<(egui::Rect, egui::Rect)> {
        let largeur_barre = theme::scroll_bar_allocated_width();
        rectangles
            .iter()
            .copied()
            .filter(|(rect, _)| {
                rect.width() <= largeur_barre
                    && rect.height() >= 11.9
                    && rect.height() < 100.0
                    && rect.left() >= screen.right() - largeur_barre - 0.5
            })
            .collect()
    }

    /// B10 : la barre de
    /// défilement verticale de la grille est plaquée au bord droit de la
    /// fenêtre, même quand les colonnes débordent (elle suivait auparavant
    /// la largeur des colonnes et sortait de la vue).
    #[test]
    fn la_barre_verticale_de_la_grille_reste_au_bord_droit_de_la_fenetre() {
        let (screen, rectangles) = peindre_classeur_large(Vec::new());
        let poignees = poignees_de_barre(screen, &rectangles);
        assert!(
            !poignees.is_empty(),
            "la poignée de la barre verticale doit être dans la bande de droite de la fenêtre \
             ({screen:?}) ; rectangles peints : {rectangles:?}",
        );
        for (rect, clip) in poignees {
            assert!(
                screen.contains_rect(rect) && clip.is_positive(),
                "la poignée {rect:?} sort de la fenêtre {screen:?} (rognage {clip:?})"
            );
        }
    }

    /// La barre déportée au bord de la fenêtre suit le défilement de la
    /// table (molette sur la table) : sa poignée descend.
    #[test]
    fn la_barre_verticale_de_la_grille_suit_la_molette_sur_la_table() {
        let (screen, repos) = peindre_classeur_large(Vec::new());
        let avant = poignees_de_barre(screen, &repos);
        let (_, defile) = peindre_classeur_large(vec![
            egui::Event::PointerMoved(egui::pos2(100.0, 150.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -120.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        let apres = poignees_de_barre(screen, &defile);
        assert_eq!(avant.len(), 1, "une poignée au repos : {avant:?}");
        assert_eq!(apres.len(), 1, "une poignée après défilement : {apres:?}");
        assert!(
            apres[0].0.top() > avant[0].0.top() + 1.0,
            "la poignée doit descendre : {:?} -> {:?}",
            avant[0].0,
            apres[0].0
        );
    }

    #[test]
    fn la_barre_verticale_de_la_grille_est_a_lechelle_du_corps_de_la_table() {
        assert_eq!(grid_vertical_bar_scale(320.0, 160.0), 2.0);
        assert_eq!(grid_vertical_bar_scale(320.0, 0.0), 1.0);
    }

    /// La barre verticale couvre toute la hauteur de la fenêtre, pas
    /// seulement le corps de la table : sa piste va du haut au bas de
    /// l'écran, en-tête figé et onglets de feuille compris.
    #[test]
    fn la_barre_verticale_de_la_grille_couvre_toute_la_hauteur_de_la_fenetre() {
        let (screen, rectangles) = peindre_classeur_large(Vec::new());
        let largeur_barre = theme::scroll_bar_allocated_width();
        let pistes: Vec<_> = rectangles
            .iter()
            .filter(|(rect, _)| {
                rect.width() <= largeur_barre
                    && rect.left() >= screen.right() - largeur_barre - 0.5
                    && rect.height() >= screen.height() - largeur_barre
            })
            .collect();
        assert!(
            !pistes.is_empty(),
            "une piste de barre verticale sur toute la hauteur de {screen:?} ; \
             rectangles peints : {rectangles:?}"
        );
    }

    /// La barre horizontale de la grille est déportée **sous** les onglets de
    /// feuille : sa piste occupe le bas de la fenêtre, sur presque toute sa
    /// largeur (moins la bande de la barre verticale).
    #[test]
    fn la_barre_horizontale_de_la_grille_est_sous_les_onglets() {
        let (screen, rectangles) = peindre_classeur_large(Vec::new());
        let hauteur_barre = theme::scroll_bar_allocated_width();
        let pistes: Vec<_> = rectangles
            .iter()
            .filter(|(rect, _)| {
                rect.height() <= hauteur_barre
                    && rect.top() >= screen.bottom() - hauteur_barre - 0.5
                    && rect.width() >= screen.width() - 2.0 * hauteur_barre
            })
            .collect();
        assert!(
            !pistes.is_empty(),
            "une piste de barre horizontale en bas de {screen:?}, sous les onglets ; \
             rectangles peints : {rectangles:?}"
        );
    }

    #[test]
    fn la_grille_deborde_horizontalement_dans_sa_zone_defilable() {
        let context = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(220.0, 180.0),
            )),
            ..Default::default()
        };
        let mut scroll = None;
        let mut output = context.run_ui(input, |ui| {
            scroll = Some(show_horizontally_scrollable_grid(ui, None, |ui| {
                TableBuilder::new(ui)
                    .columns(TableColumn::initial(110.0).at_least(56.0), 5)
                    .auto_shrink([false, false])
                    .header(20.0, |mut header| {
                        for _ in 0..5 {
                            header.col(|ui| {
                                ui.label("Colonne");
                            });
                        }
                    })
                    .body(|_| {});
            }));
        });
        output.textures_delta.clear();
        let scroll = scroll.expect("la grille doit être dessinée");
        assert!(
            scroll.content_size.x > scroll.inner_rect.width(),
            "les colonnes doivent pouvoir défiler dans le rectangle visible : {:?} > {:?}",
            scroll.content_size.x,
            scroll.inner_rect.width()
        );
    }

    #[test]
    fn le_rendu_garde_son_ratio_et_se_met_a_l_echelle_dans_la_zone_disponible() {
        // F11 : le graphe conserve le ratio de `DEFAULT_FIGURE_SIZE` mais est
        // mis à l'échelle « contain » — maximisé dans la zone disponible, en
        // prenant la dimension limitante (largeur ou hauteur) — plutôt que
        // peint à taille fixe et rogné. Point 3 du rapport du 2026-09-24 :
        // c'est un **zoom** du rendu de référence, jamais une nouvelle mise
        // en page à la taille affichée — les formes mises en cache doivent
        // être identiques quelle que soit la taille de la fenêtre.
        let (session, graph) = graphe();
        let snapshot = session.handle().snapshot().unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut view = GraphView::new();
        let (fig_w, fig_h) = DEFAULT_FIGURE_SIZE;
        let mut reference_shapes: Option<Vec<egui::Shape>> = None;

        for size in [
            GRAPH_WINDOW_SIZE,
            egui::vec2(320.0, 260.0),
            egui::vec2(200.0, 100.0),
        ] {
            let contents = egui::Rect::from_min_size(egui::pos2(300.0, 300.0), size);
            let visible = egui::Rect::from_min_max(
                contents.min,
                contents.max - egui::vec2(0.0, GRAPH_CONTROLS_HEIGHT),
            );
            let scale = (visible.width() / fig_w as f32).min(visible.height() / fig_h as f32);
            let expected_size = egui::vec2(fig_w as f32 * scale, fig_h as f32 * scale);
            for frame in 0..3 {
                // La molette se trouve sous la zone visible, hors de la
                // figure et de ses contrôles : elle ne doit pas la zoomer.
                let pos = egui::pos2(visible.center().x, visible.bottom() + 10.0);
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 1200.0),
                    )),
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, 4.0),
                            modifiers: egui::Modifiers::NONE,
                            phase: egui::TouchPhase::Move,
                        },
                    ],
                    ..Default::default()
                };
                let misses = view.cache().misses();
                let mut output = context.run_ui(input, |ui| {
                    let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(contents));
                    content_ui.set_clip_rect(contents);
                    graph_view_contents(
                        &mut content_ui,
                        &mut view,
                        &snapshot,
                        graph,
                        &mut Vec::new(),
                    );
                });
                output.textures_delta.clear();
                let rect = view.cache().key().unwrap().rect;
                assert!(
                    (rect.size() - egui::vec2(fig_w as f32, fig_h as f32)).length() < 0.1,
                    "la disposition doit rester à la taille de référence, obtenu {:?} pour une \
                     zone de {size:?}",
                    rect.size()
                );
                let display = view.display_rect().unwrap();
                assert!(
                    (display.size() - expected_size).length() < 0.1,
                    "taille affichée attendue pour une zone de {size:?} : {expected_size:?}, \
                     obtenu {:?}",
                    display.size()
                );
                assert!(
                    visible.contains_rect(display),
                    "la figure mise à l'échelle doit tenir dans la zone visible"
                );
                assert_eq!(
                    view.view().generation(),
                    0,
                    "interaction hors de la figure et de ses contrôles"
                );
                if frame > 0 {
                    assert_eq!(view.cache().misses(), misses, "recalcul sans changement");
                }
                // Vérifier les formes réellement envoyées à egui, pas
                // seulement le rectangle calculé avant le rendu.
                assert!(!view.cache().shapes().is_empty());
                if let Some(reference) = &reference_shapes {
                    assert_eq!(
                        view.cache().shapes(),
                        reference.as_slice(),
                        "la taille affichée ne doit pas redisposer la figure (zone de {size:?})"
                    );
                } else {
                    reference_shapes = Some(view.cache().shapes().to_vec());
                }
                let transform = egui::emath::TSTransform::new(
                    display.min.to_vec2(),
                    display.width() / fig_w as f32,
                );
                for shape in view.cache().shapes() {
                    let mut displayed = shape.clone();
                    // Le texte est remis en page à la taille affichée (#18) :
                    // même transformation que `GraphView::paint_scaled`. Un
                    // texte Typst (titres, légende : #32) y change de
                    // texture, rastérisée à la taille affichée (#31) : un
                    // maillage se compare donc sur sa géométrie seule, à un
                    // pixel près : il est calé sur la grille des pixels
                    // physiques à l'écran (#86).
                    stokesforge_render_egui::scaled::transform_shape(
                        &context,
                        &mut displayed,
                        transform,
                        &mut |_| None,
                    );
                    let same = |painted: &egui::Shape| match (painted, &displayed) {
                        (egui::Shape::Mesh(a), egui::Shape::Mesh(b)) => {
                            a.indices == b.indices
                                && a.vertices.len() == b.vertices.len()
                                && a.vertices.iter().zip(&b.vertices).all(|(p, q)| {
                                    (p.pos - q.pos).abs().max_elem() <= 1.0 && p.uv == q.uv
                                })
                        }
                        (a, b) => a == b,
                    };
                    assert!(
                        output.shapes.iter().any(|painted| same(&painted.shape)
                            && visible.contains_rect(painted.clip_rect)),
                        "forme du graphe hors de la zone visible"
                    );
                }
            }
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
                graph_contents(ui, &app.state, &mut Vec::new(), &mut app.graph_views, graph);
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

    #[test]
    fn clic_droit_dans_le_vide_de_grille_et_sur_les_onglets_propose_les_ajouts() {
        let mut project = Project::new("Menus").unwrap();
        let book = project.add_workbook(project.root(), None).unwrap();
        let sheet = project.tree().node(book).unwrap().children()[0];
        project
            .add_column(sheet, None, None, Column::from_f64(&[1.0]))
            .unwrap();
        let session = Session::spawn(project, None).unwrap();

        for (on_tabs, label) in [
            (false, tr!("workbook-column-add")),
            (true, tr!("workbook-worksheet-add")),
        ] {
            let context = egui::Context::default();
            let mut state =
                AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
            let mut actions = Vec::new();
            {
                let mut paint = |events: Vec<egui::Event>| {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600.0, 500.0),
                        )),
                        events,
                        ..Default::default()
                    };
                    let mut output = context.run_ui(input, |ui| {
                        book_contents(ui, &mut state, &mut actions, book)
                    });
                    output.textures_delta.clear();
                    output
                };
                let first = paint(Vec::new());
                let tab_is_visible = first.shapes.iter().any(|painted| match &painted.shape {
                    egui::Shape::Text(shape) if shape.galley.job.text == "Sheet1" => painted
                        .clip_rect
                        .contains(shape.pos + shape.galley.rect.center().to_vec2()),
                    _ => false,
                });
                assert!(
                    tab_is_visible,
                    "la bande d'onglets doit rester dans la fenêtre"
                );
                let pos = if on_tabs {
                    find_text_center(&first.shapes, "Sheet1")
                } else {
                    egui::pos2(450.0, 180.0)
                };
                paint(vec![egui::Event::PointerMoved(pos)]);
                paint(vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }]);
                paint(vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
                let output = paint(Vec::new());
                assert!(
                    output.shapes.iter().any(|painted| match &painted.shape {
                        egui::Shape::Text(shape) => shape.galley.job.text == *label,
                        _ => false,
                    }),
                    "le menu contextuel doit proposer {label} : {:?}",
                    output
                        .shapes
                        .iter()
                        .filter_map(|painted| match &painted.shape {
                            egui::Shape::Text(shape) => Some(shape.galley.job.text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                );
                let menu_pos = find_text_center(&output.shapes, &label);
                paint(vec![egui::Event::PointerMoved(menu_pos)]);
                paint(vec![egui::Event::PointerButton {
                    pos: menu_pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }]);
                paint(vec![egui::Event::PointerButton {
                    pos: menu_pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
            if on_tabs {
                assert!(
                    matches!(actions.as_slice(), [Action::AddWorksheet { book: id }] if *id == book)
                );
            } else {
                assert!(
                    matches!(actions.as_slice(), [Action::AddColumn { sheet: id }] if *id == sheet)
                );
            }
        }
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
                    graph_contents(ui, &app.state, &mut Vec::new(), &mut app.graph_views, graph);
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
                    graph_contents(ui, &app.state, &mut Vec::new(), &mut app.graph_views, graph);
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

    #[test]
    fn le_pan_principal_atteint_la_figure_sous_la_zone_de_depot() {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut graph_views = HashMap::new();
        let mut actions = Vec::new();
        let contents =
            egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(560.0, 540.0));

        let mut paint = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1200.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(contents));
                content_ui.set_clip_rect(contents);
                graph_drop_target(
                    &mut content_ui,
                    &state,
                    &mut actions,
                    &mut graph_views,
                    graph,
                );
            });
            output.textures_delta.clear();
        };

        let start = egui::pos2(380.0, 322.0);
        paint(vec![egui::Event::PointerMoved(start)]);
        let end = start + egui::vec2(30.0, -20.0);
        paint(vec![egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        paint(vec![egui::Event::PointerMoved(end)]);
        paint(vec![egui::Event::PointerMoved(
            end + egui::vec2(20.0, -10.0),
        )]);

        assert!(
            graph_views.get(&graph).unwrap().view().generation() > 0,
            "la cible de dépôt ne doit pas intercepter le pan de la figure"
        );
    }

    #[test]
    fn le_pan_principal_atteint_la_figure_dans_la_fenetre_flottante() {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut graph_views = HashMap::new();
        let mut actions = Vec::new();
        let tab = Tab::Graph(graph);
        let id = egui::Id::new(("content_window", tab));
        let panel_center = std::cell::Cell::new(None);

        let mut paint = |events: Vec<egui::Event>| {
            let is_initial_frame = events.is_empty();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1200.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                show_floating_window(
                    ui.ctx(),
                    id,
                    egui::Rect::from_min_size(egui::pos2(100.0, 100.0), GRAPH_WINDOW_SIZE),
                    ui.ctx().content_rect(),
                    true,
                    |ui| {
                        let _ = window_header(
                            ui,
                            id,
                            "Figure",
                            Some(tab),
                            false,
                            HeaderPlacement::Floating,
                        );
                        graph_drop_target(ui, &state, &mut actions, &mut graph_views, graph);
                    },
                    |_, _| {},
                );
            });
            output.textures_delta.clear();
            if is_initial_frame {
                let view = graph_views.get(&graph).unwrap();
                let panel = &view.last_drawn().unwrap().panels[0];
                let figure = view.display_rect().unwrap();
                let scale = figure.width() / DEFAULT_FIGURE_SIZE.0 as f32;
                panel_center.set(Some(egui::pos2(
                    figure.min.x
                        + (panel.pixel_origin.0 as f32 + panel.pixel_size.0 as f32 / 2.0) * scale,
                    figure.min.y
                        + (panel.pixel_origin.1 as f32 + panel.pixel_size.1 as f32 / 2.0) * scale,
                )));
            }
        };

        paint(Vec::new());
        let start = panel_center.get().unwrap();
        paint(vec![egui::Event::PointerMoved(start)]);
        let end = start + egui::vec2(30.0, -20.0);
        paint(vec![egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        paint(vec![egui::Event::PointerMoved(end)]);
        paint(vec![egui::Event::PointerMoved(
            end + egui::vec2(20.0, -10.0),
        )]);

        assert!(
            graph_views.get(&graph).unwrap().view().generation() > 0,
            "la fenetre flottante ne doit pas intercepter le pan de la figure"
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

    /// Trouve le centre du texte peint contenant `needle` dans `shapes` —
    /// utilisé pour cliquer une entrée du Project Manager sans supposer sa
    /// position exacte (mise en page auto). Même technique que
    /// `window_chrome::tests::len_tete_du_graphe_garde_son_titre_au_centre_et_sa_fleche_a_gauche`.
    fn find_text_center(shapes: &[egui::epaint::ClippedShape], needle: &str) -> egui::Pos2 {
        shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) if shape.galley.job.text.contains(needle) => {
                    Some(shape.pos + shape.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("aucun texte contenant {needle:?} n'a été peint"))
    }

    /// Un clic (glissé nul, pression puis relâchement au même endroit) sur
    /// `pos`, réparti sur deux images comme le veut la détection de clic
    /// d'egui — voir la doc de tête de
    /// `le_pan_principal_atteint_la_figure_dans_la_fenetre_flottante` pour le
    /// même schéma appliqué à un glissé.
    fn click_at<R>(
        context: &egui::Context,
        pos: egui::Pos2,
        modifiers: egui::Modifiers,
        mut paint: impl FnMut(&egui::Context, Vec<egui::Event>) -> R,
    ) {
        // `PointerButton::modifiers` n'alimente pas `InputState::modifiers`
        // (voir `InputState::begin_pass`, qui ne le lit que sur
        // `Event::ModifiersChanged`/`Event::Key`) : sans cet événement
        // explicite, `ui.input(|i| i.modifiers.command)` resterait toujours
        // faux, quels que soient les `modifiers` posés sur les événements
        // de pointeur eux-mêmes.
        paint(
            context,
            vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::PointerMoved(pos),
            ],
        );
        paint(
            context,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            }],
        );
        paint(
            context,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }],
        );
        if modifiers != egui::Modifiers::NONE {
            paint(
                context,
                vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)],
            );
        }
    }

    /// Construit l'arbre d'un projet Book+Graph (le fixture [`graphe`],
    /// resynchronisé sur `AppState`) — commun aux tests de double-clic et de
    /// sélection multiple du Project Manager.
    fn project_manager_fixture() -> (Session, Tab, Tab, Vec<ProjectTreeNode>) {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let tree = state.project_tree();
        let book_id = tree
            .iter()
            .find_map(|node| match node {
                ProjectTreeNode::Window {
                    id,
                    kind: WindowKind::Book,
                    ..
                } => Some(*id),
                _ => None,
            })
            .expect("le projet du fixture porte un classeur");
        (session, Tab::Book(book_id), Tab::Graph(graph), tree)
    }

    #[test]
    fn le_double_clic_ferme_lentree_deja_au_premier_plan_mais_pas_une_autre() {
        let (_session, book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let open_requests = std::cell::RefCell::new(Vec::new());
        let close_requests = std::cell::RefCell::new(Vec::new());
        let selection = std::cell::RefCell::new(HashSet::new());
        let hidden = std::cell::RefCell::new(HashSet::new());

        let paint = |context: &egui::Context, events: Vec<egui::Event>| -> egui::FullOutput {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 400.0),
                )),
                events,
                ..Default::default()
            };
            let mut interaction = PmInteraction {
                open_requests: &mut open_requests.borrow_mut(),
                close_requests: &mut close_requests.borrow_mut(),
                active_tab: Some(graph_tab),
                selection: &mut selection.borrow_mut(),
                hidden: &mut hidden.borrow_mut(),
            };
            let mut output = context.run_ui(input, |ui| {
                project_manager_contents_from_tree(ui, &tree, &mut interaction);
            });
            output.textures_delta.clear();
            output
        };

        let first = paint(&context, Vec::new());
        let graph_pos = find_text_center(&first.shapes, "Graph1");
        let book_pos = find_text_center(&first.shapes, "Book");

        // Double-clic sur l'entrée déjà au premier plan (`active_tab` =
        // `graph_tab`) : doit fermer, pas seulement (re)focaliser.
        click_at(&context, graph_pos, egui::Modifiers::NONE, &paint);
        click_at(&context, graph_pos, egui::Modifiers::NONE, &paint);
        assert!(
            close_requests.borrow().contains(&graph_tab),
            "le double-clic sur l'onglet déjà au premier plan doit le fermer, close_requests={:?}",
            close_requests.borrow()
        );

        // Double-clic sur une entrée qui n'est *pas* au premier plan
        // (`active_tab` reste `graph_tab`) : ouvre/focalise, ne ferme rien.
        close_requests.borrow_mut().clear();
        click_at(&context, book_pos, egui::Modifiers::NONE, &paint);
        click_at(&context, book_pos, egui::Modifiers::NONE, &paint);
        assert!(
            !close_requests.borrow().contains(&book_tab),
            "le double-clic sur une entrée qui n'est pas au premier plan ne doit pas la fermer"
        );
        assert!(
            open_requests.borrow().contains(&book_tab),
            "le double-clic doit tout de même ouvrir/focaliser l'entrée, open_requests={:?}",
            open_requests.borrow()
        );
    }

    #[test]
    fn ctrl_clic_selectionne_sans_ouvrir() {
        let (_session, _book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let open_requests = std::cell::RefCell::new(Vec::new());
        let close_requests = std::cell::RefCell::new(Vec::new());
        let selection = std::cell::RefCell::new(HashSet::new());
        let hidden = std::cell::RefCell::new(HashSet::new());

        let paint = |context: &egui::Context, events: Vec<egui::Event>| -> egui::FullOutput {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 400.0),
                )),
                events,
                ..Default::default()
            };
            let mut interaction = PmInteraction {
                open_requests: &mut open_requests.borrow_mut(),
                close_requests: &mut close_requests.borrow_mut(),
                active_tab: None,
                selection: &mut selection.borrow_mut(),
                hidden: &mut hidden.borrow_mut(),
            };
            let mut output = context.run_ui(input, |ui| {
                project_manager_contents_from_tree(ui, &tree, &mut interaction);
            });
            output.textures_delta.clear();
            output
        };

        let first = paint(&context, Vec::new());
        let graph_pos = find_text_center(&first.shapes, "Graph1");
        click_at(&context, graph_pos, egui::Modifiers::COMMAND, &paint);

        assert_eq!(
            *selection.borrow(),
            HashSet::from([graph_tab]),
            "Ctrl/Cmd-clic doit ajouter l'entrée à la sélection"
        );
        assert!(
            open_requests.borrow().is_empty(),
            "Ctrl/Cmd-clic ne doit jamais ouvrir l'onglet, open_requests={:?}",
            open_requests.borrow()
        );
    }

    #[test]
    fn masquer_et_afficher_agissent_sur_toute_la_selection() {
        let (_session, book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let open_requests = std::cell::RefCell::new(Vec::new());
        let close_requests = std::cell::RefCell::new(Vec::new());
        // Sélection posée directement plutôt que reconstruite par deux
        // Ctrl-clics successifs (voir `ctrl_clic_selectionne_sans_ouvrir`
        // pour ce comportement-là, testé isolément) : ce test porte sur ce
        // que « Masquer »/« Afficher » font d'une sélection déjà en place,
        // pas sur la façon dont elle s'est constituée.
        let selection = std::cell::RefCell::new(HashSet::from([graph_tab, book_tab]));
        let hidden = std::cell::RefCell::new(HashSet::new());

        let paint = |context: &egui::Context, events: Vec<egui::Event>| -> egui::FullOutput {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 400.0),
                )),
                events,
                ..Default::default()
            };
            let mut interaction = PmInteraction {
                open_requests: &mut open_requests.borrow_mut(),
                close_requests: &mut close_requests.borrow_mut(),
                active_tab: None,
                selection: &mut selection.borrow_mut(),
                hidden: &mut hidden.borrow_mut(),
            };
            let mut output = context.run_ui(input, |ui| {
                project_manager_contents_from_tree(ui, &tree, &mut interaction);
            });
            output.textures_delta.clear();
            output
        };

        let with_toolbar = paint(&context, Vec::new());
        let hide_pos = find_text_center(&with_toolbar.shapes, &tr!("project-manager-hide"));
        click_at(&context, hide_pos, egui::Modifiers::NONE, &paint);
        assert_eq!(
            *hidden.borrow(),
            HashSet::from([graph_tab, book_tab]),
            "« Masquer » doit s'appliquer à toute la sélection, hidden={:?}",
            hidden.borrow()
        );

        let with_hidden_rows = paint(&context, Vec::new());
        let show_pos = find_text_center(&with_hidden_rows.shapes, &tr!("project-manager-show"));
        click_at(&context, show_pos, egui::Modifiers::NONE, &paint);
        assert!(
            hidden.borrow().is_empty(),
            "« Afficher » doit vider le masquage de toute la sélection, hidden={:?}",
            hidden.borrow()
        );
        assert!(
            open_requests.borrow().contains(&graph_tab)
                && open_requests.borrow().contains(&book_tab),
            "« Afficher » doit aussi garantir que chaque onglet est ouvert, open_requests={:?}",
            open_requests.borrow()
        );
    }
}

/// #38 : Tab et les flèches appartiennent à la grille. egui les traite au
/// début de l'image (`Focus::begin_pass`) pour déplacer **son** focus, sauf si
/// le widget focalisé les réclame par un `EventFilter` : sans filtre, Tab
/// faisait sortir le focus du tableau vers le reste de l'interface au lieu
/// d'avancer la sélection de cellule en cellule.
///
/// Banc sans fenêtre : un bouton voisin (« avant », premier widget focalisable
/// venu de l'image, donc celui qu'egui choisirait sur un Tab) puis un
/// classeur. [`book_contents`] est le contenu commun du classeur flottant et
/// du classeur détaché : tester ce chemin couvre les deux fenêtres.
#[cfg(test)]
mod grid_keyboard_tests {
    use super::*;
    use crate::selection::{GridCursor, Selection};
    use crate::state::HeaderField;
    use stokesforge_core::{Column, Project};
    use stokesforge_session::Session;

    struct Banc {
        _session: Session,
        context: egui::Context,
        state: AppState,
        actions: Vec<Action>,
        book: NodeId,
        sheet: NodeId,
        columns: Vec<NodeId>,
        voisin_focalise: bool,
        dernier: Option<egui::FullOutput>,
    }

    impl Banc {
        /// Trois colonnes de quatre lignes, valeurs 101..104, 201..204, 301..304.
        fn new() -> Self {
            let mut project = Project::new("Clavier").unwrap();
            let root = project.root();
            let book = project.add_empty_workbook(root, None).unwrap();
            let sheet = project.add_worksheet(book, None).unwrap();
            let columns = (1..=3)
                .map(|c| {
                    let valeurs: Vec<f64> = (1..=4).map(|r| f64::from(c * 100 + r)).collect();
                    project
                        .add_column(sheet, None, None, Column::from_f64(&valeurs))
                        .unwrap()
                })
                .collect();
            let session = Session::spawn(project, None).unwrap();
            let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
            let context = egui::Context::default();
            context.set_fonts(stokesforge_render_egui::font_definitions());
            theme::install(&context);
            let mut banc = Banc {
                _session: session,
                context,
                state,
                actions: Vec::new(),
                book,
                sheet,
                columns,
                voisin_focalise: false,
                dernier: None,
            };
            for _ in 0..3 {
                banc.image(Vec::new());
            }
            banc
        }

        fn image(&mut self, events: Vec<egui::Event>) {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(700.0, 420.0),
                )),
                events,
                ..Default::default()
            };
            self.actions.clear();
            let (state, actions, book) = (&mut self.state, &mut self.actions, self.book);
            let mut voisin = false;
            let mut sortie = self.context.run_ui(input, |ui| {
                voisin = ui.button("avant").has_focus();
                book_contents(ui, state, actions, book);
            });
            self.voisin_focalise = voisin;
            sortie.textures_delta.clear();
            self.dernier = Some(sortie);
        }

        fn touche(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
            // `ModifiersChanged` explicite : un `Event::Key` ne met pas à jour
            // `InputState::modifiers`, que la grille lit pour Maj.
            let touche = |pressed| egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            };
            self.image(vec![egui::Event::ModifiersChanged(modifiers), touche(true)]);
            self.image(vec![
                touche(false),
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            ]);
            // Une image de plus, sans événement : le focus rendu à un autre
            // widget par `end_pass` se verrait ici.
            self.image(Vec::new());
        }

        fn tab(&mut self) {
            self.touche(egui::Key::Tab, egui::Modifiers::NONE);
        }

        fn maj_tab(&mut self) {
            self.touche(egui::Key::Tab, egui::Modifiers::SHIFT);
        }

        /// Clic gauche sur la cellule peinte portant `texte`, comme à la souris.
        fn clic_sur(&mut self, texte: &str) {
            let position = self
                .dernier
                .as_ref()
                .unwrap()
                .shapes
                .iter()
                .find_map(|peint| match &peint.shape {
                    egui::Shape::Text(shape) if shape.galley.job.text == texte => {
                        Some(shape.pos + shape.galley.rect.center().to_vec2())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("aucune cellule peinte ne porte {texte:?}"));
            self.image(vec![egui::Event::PointerMoved(position)]);
            for pressed in [true, false] {
                self.image(vec![egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
            // Laisse passer les images de retombée (focus, relâchement).
            self.image(Vec::new());
        }

        fn curseur(&self) -> Option<GridCursor> {
            self.state
                .selection(self.sheet)
                .and_then(Selection::grid_cursor)
        }
    }

    #[test]
    fn tab_hors_edition_avance_la_selection_sans_sortir_de_la_grille() {
        let mut banc = Banc::new();
        banc.clic_sur("101");
        assert_eq!(
            banc.curseur(),
            Some(GridCursor::Data(banc.columns[0], 0)),
            "le clic sélectionne la cellule"
        );
        banc.tab();
        assert_eq!(
            banc.curseur(),
            Some(GridCursor::Data(banc.columns[1], 0)),
            "Tab avance d'une colonne"
        );
        assert!(
            !banc.voisin_focalise,
            "Tab a fait sortir le focus de la grille vers le widget voisin"
        );
        banc.tab();
        assert_eq!(banc.curseur(), Some(GridCursor::Data(banc.columns[2], 0)));
        banc.maj_tab();
        assert_eq!(
            banc.curseur(),
            Some(GridCursor::Data(banc.columns[1], 0)),
            "Maj+Tab recule"
        );
        assert!(!banc.voisin_focalise);
    }

    #[test]
    fn les_fleches_deplacent_la_selection_et_gardent_le_focus_dans_la_grille() {
        let mut banc = Banc::new();
        banc.clic_sur("102");
        for (key, attendu) in [
            (egui::Key::ArrowRight, GridCursor::Data(banc.columns[1], 1)),
            (egui::Key::ArrowDown, GridCursor::Data(banc.columns[1], 2)),
            (egui::Key::ArrowLeft, GridCursor::Data(banc.columns[0], 2)),
            (egui::Key::ArrowUp, GridCursor::Data(banc.columns[0], 1)),
        ] {
            banc.touche(key, egui::Modifiers::NONE);
            assert_eq!(banc.curseur(), Some(attendu), "{key:?}");
            assert!(!banc.voisin_focalise, "{key:?} a déplacé le focus egui");
        }
    }

    #[test]
    fn tab_pendant_l_edition_valide_et_garde_le_focus_dans_la_grille() {
        let mut banc = Banc::new();
        banc.clic_sur("101");
        // Une frappe sur une cellule sélectionnée ouvre l'édition.
        banc.image(vec![egui::Event::Text("7".to_string())]);
        banc.image(Vec::new());
        banc.image(Vec::new());
        assert!(banc.state.editing().is_some(), "la frappe ouvre l'édition");
        banc.tab();
        assert!(banc.state.editing().is_none(), "Tab valide la saisie");
        assert_eq!(
            banc.curseur(),
            Some(GridCursor::Data(banc.columns[1], 0)),
            "Tab valide *et* avance"
        );
        // Et la grille garde la main pour les Tab suivants, hors édition.
        banc.tab();
        assert_eq!(
            banc.curseur(),
            Some(GridCursor::Data(banc.columns[2], 0)),
            "après la validation, Tab continue d'avancer dans la grille"
        );
        assert!(!banc.voisin_focalise);
    }

    /// #52 : Tab sur la dernière colonne ajoute une colonne quel que soit le
    /// champ d'en-tête, hors édition comme en édition — plus seulement Formule.
    #[test]
    fn tab_en_derniere_colonne_ajoute_une_colonne_pour_chaque_champ_d_en_tete() {
        for editer in [false, true] {
            for field in [
                HeaderField::LongName,
                HeaderField::Unit,
                HeaderField::Comment,
                HeaderField::Formula,
            ] {
                let mut banc = Banc::new();
                let (sheet, derniere) = (banc.sheet, banc.columns[2]);
                // Le clic donne le focus à la grille (sinon Tab n'est pas à elle).
                banc.clic_sur("301");
                banc.state.select_header_field(sheet, derniere, field);
                if editer {
                    banc.state.begin_header_edit(derniere, field);
                }
                for _ in 0..3 {
                    banc.image(Vec::new());
                }
                banc.image(vec![
                    egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                    egui::Event::Key {
                        key: egui::Key::Tab,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
                assert!(
                    banc.actions.iter().any(|action| matches!(
                        action,
                        Action::AddColumnAndSelectHeaderField { field: f, .. } if *f == field
                    )),
                    "Tab sur {field:?} (édition : {editer}) doit ajouter une colonne"
                );
            }
        }
    }

    #[test]
    fn tab_pendant_l_edition_d_un_champ_d_en_tete_valide_et_reste_dans_la_grille() {
        let mut banc = Banc::new();
        let (sheet, colonne) = (banc.sheet, banc.columns[0]);
        banc.state
            .select_header_field(sheet, colonne, HeaderField::LongName);
        banc.state.begin_header_edit(colonne, HeaderField::LongName);
        for _ in 0..3 {
            banc.image(Vec::new());
        }
        assert!(banc.state.header_editing().is_some());
        banc.tab();
        assert!(banc.state.header_editing().is_none(), "Tab valide le champ");
        assert_eq!(
            banc.curseur(),
            Some(GridCursor::Header(banc.columns[1], HeaderField::LongName)),
            "Tab valide *et* passe à la colonne suivante"
        );
        banc.tab();
        assert_eq!(
            banc.curseur(),
            Some(GridCursor::Header(banc.columns[2], HeaderField::LongName)),
            "puis continue hors édition"
        );
        assert!(!banc.voisin_focalise);
    }

    /// Le focus de la grille ne se retient pas : un clic ailleurs le rend, et
    /// Tab reprend alors son rôle d'egui (déplacer le focus), sans toucher la
    /// sélection de la grille.
    #[test]
    fn un_clic_hors_de_la_grille_lui_retire_tab() {
        let mut banc = Banc::new();
        banc.clic_sur("101");
        assert!(!keyboard_focus_outside_grid(&banc.context));
        assert!(
            banc.context.memory(|memory| memory.focused()).is_some(),
            "un clic dans la grille lui donne le focus clavier"
        );
        banc.clic_sur("avant");
        let avant = banc.curseur();
        banc.tab();
        assert_eq!(
            banc.curseur(),
            avant,
            "Tab, hors de la grille, ne déplace pas sa sélection"
        );
    }

    /// Le focus de la grille n'est pas un champ texte pour le reste de
    /// l'application (Entrée d'un dialogue, Suppr d'une annotation) : seul un
    /// autre widget focalisé l'est.
    #[test]
    fn le_focus_de_la_grille_n_est_pas_un_champ_en_cours_de_saisie() {
        let mut banc = Banc::new();
        assert!(!keyboard_focus_outside_grid(&banc.context), "aucun focus");
        banc.clic_sur("101");
        assert!(
            !keyboard_focus_outside_grid(&banc.context),
            "focus de la grille"
        );
        banc.context
            .memory_mut(|memory| memory.request_focus(egui::Id::new("champ_etranger")));
        assert!(keyboard_focus_outside_grid(&banc.context), "focus étranger");
    }

    /// Le focus clavier d'un champ étranger à la grille (ici un champ texte
    /// d'un dialogue) reste le sien : ni Tab ni flèche ne déplacent la
    /// sélection.
    #[test]
    fn un_champ_etranger_focalise_garde_tab_et_les_fleches() {
        let mut banc = Banc::new();
        banc.clic_sur("101");
        let champ = egui::Id::new("champ_etranger");
        let avant = banc.curseur();
        let mut texte = String::new();
        for key in [egui::Key::ArrowRight, egui::Key::Tab] {
            // Trois images pour que le champ soit posé et focalisé, puis la touche.
            for frame in 0..4 {
                let events = if frame == 3 {
                    vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }]
                } else {
                    Vec::new()
                };
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(700.0, 420.0),
                    )),
                    events,
                    ..Default::default()
                };
                let (state, actions, book) = (&mut banc.state, &mut banc.actions, banc.book);
                let mut sortie = banc.context.run_ui(input, |ui| {
                    egui::Area::new(egui::Id::new("dialogue")).show(ui.ctx(), |ui| {
                        ui.add(egui::TextEdit::singleline(&mut texte).id(champ));
                    });
                    if frame < 3 {
                        ui.ctx().memory_mut(|memory| memory.request_focus(champ));
                    }
                    book_contents(ui, state, actions, book);
                });
                sortie.textures_delta.clear();
            }
            assert_eq!(
                banc.curseur(),
                avant,
                "{key:?} : la grille a volé la touche au champ texte focalisé"
            );
        }
    }
}

#[cfg(test)]
mod pending_format_tests {
    use super::*;
    use crate::text_format::TextFormat;
    use egui::text::{CCursor, CCursorRange};

    fn context_with_selection(field: egui::Id, range: std::ops::Range<usize>) -> egui::Context {
        let ctx = egui::Context::default();
        let mut state = egui::text_edit::TextEditState::default();
        state.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(range.start),
            CCursor::new(range.end),
        )));
        state.store(&ctx, field);
        ctx
    }

    /// #63 : la mise en forme demandée par la barre s'applique au surlignage
    /// que le champ a gardé, puis la sélection se place dans les balises.
    #[test]
    fn la_mise_en_forme_en_attente_s_applique_au_surlignage_du_champ() {
        let field = egui::Id::new("champ");
        let ctx = context_with_selection(field, 4..7);
        ctx.data_mut(|data| data.insert_temp(pending_format_id(), Some(TextFormat::Bold)));
        let mut buffer = "Abs 280 nm".to_owned();
        apply_pending_format(&ctx, field, &mut buffer);
        assert_eq!(buffer, "Abs #strong[280] nm");
        let state = egui::text_edit::TextEditState::load(&ctx, field).unwrap();
        let [start, end] = state.cursor.char_range().unwrap().sorted_cursors();
        assert_eq!((start.index.0, end.index.0), (12, 15));
        // La demande est consommée : une seconde image ne la rejoue pas.
        apply_pending_format(&ctx, field, &mut buffer);
        assert_eq!(buffer, "Abs #strong[280] nm");
    }

    #[test]
    fn sans_surlignage_le_curseur_recoit_les_balises() {
        let field = egui::Id::new("champ");
        let ctx = context_with_selection(field, 3..3);
        ctx.data_mut(|data| data.insert_temp(pending_format_id(), Some(TextFormat::Subscript)));
        let mut buffer = "H2O".to_owned();
        apply_pending_format(&ctx, field, &mut buffer);
        assert_eq!(buffer, "H2O#sub[]");
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

    #[test]
    fn supprimer_un_voisin_ne_selectionne_pas_le_suivant() {
        use crate::object_manager::{selection_after_remove, VisibilityTarget as T};
        use stokesforge_plot::{AnnotationRef, ShapeRef};
        let a = AnnotationRef {
            row: 1,
            column: 2,
            index: 3,
        };
        let s = ShapeRef {
            row: 1,
            column: 2,
            index: 3,
        };
        for (selected, removed, expected) in [
            (
                T::Series(SeriesRef::new(1, 3)),
                T::Series(SeriesRef::new(1, 0)),
                Some(T::Series(SeriesRef::new(1, 2))),
            ),
            (
                T::Annotation(a),
                T::Annotation(AnnotationRef { index: 0, ..a }),
                Some(T::Annotation(AnnotationRef { index: 2, ..a })),
            ),
            (
                T::Shape(s),
                T::Shape(ShapeRef { index: 0, ..s }),
                Some(T::Shape(ShapeRef { index: 2, ..s })),
            ),
        ] {
            assert_eq!(selection_after_remove(Some(selected), removed), expected);
            assert_eq!(selection_after_remove(Some(selected), selected), None);
        }
    }
    #[test]
    fn glisser_une_serie_emet_un_seul_deplacement_au_rang_final() {
        let mut project = Project::new("Objets").unwrap();
        let root = project.root();
        let mut panel = PanelSpec::default();
        for name in ["Premier", "Deuxième", "Dernier"] {
            let mut series = SeriesSpec::new(NodeId::from_raw(900));
            series.name = name.into();
            panel.series.push(series);
        }
        let spec = PlotSpec::default().with_panels(panel);
        let graph = project.add_graph(root, None, spec.clone()).unwrap();
        let model =
            crate::object_manager::object_tree(project.tree(), graph, Some(SeriesRef::new(0, 0)))
                .unwrap();
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let image = |events| {
            let mut actions = Vec::new();
            let mut requests = Vec::new();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600.0, 500.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    requests = object_manager_contents(
                        ui,
                        project.tree(),
                        Some(&model),
                        Some(&spec),
                        false,
                        false,
                        &mut actions,
                    );
                },
            );
            output.textures_delta.clear();
            (output, actions, requests)
        };
        image(Vec::new());
        let (output, _, _) = image(Vec::new());
        let text_center = |text: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(shape) if shape.galley.text() == text => {
                        Some(shape.visual_bounding_rect().center())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("texte absent : {text}"))
        };
        let from = text_center("Premier");
        let to = text_center("Dernier") + egui::vec2(0.0, 7.0);
        let pointer = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        image(vec![egui::Event::PointerMoved(from)]);
        image(vec![pointer(from, true)]);
        let (_, actions, _) = image(vec![egui::Event::PointerMoved(from + egui::vec2(0.0, 8.0))]);
        assert!(actions.is_empty(), "l'aperçu n'écrit pas le modèle");
        image(vec![egui::Event::PointerMoved(to)]);
        let (_, actions, _) = image(vec![pointer(to, false)]);
        assert_eq!(actions.len(), 1, "une commande au relâchement");
        assert!(matches!(
            actions[0],
            Action::ReorderSeries {
                source: SeriesRef { panel: 0, index: 0 },
                index: 2,
                ..
            }
        ));
        let (_, actions, _) = image(Vec::new());
        assert!(
            actions.is_empty(),
            "le dépôt ne se répète pas à l'image suivante"
        );
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let (_, actions, requests) = image(vec![key(egui::Key::F2, egui::Modifiers::NONE)]);
        assert!(actions.is_empty());
        assert!(requests.iter().any(|r| matches!(
            r,
            ObjectManagerRequest::Rename(SeriesRef { panel: 0, index: 0 })
        )));
        let (_, actions, _) = image(vec![key(egui::Key::ArrowDown, egui::Modifiers::CTRL)]);
        assert!(matches!(
            actions.as_slice(),
            [Action::ReorderSeries {
                source: SeriesRef { panel: 0, index: 0 },
                index: 1,
                ..
            }]
        ));
    }
    #[test]
    fn undo_redo_suit_la_serie_reordonnee_et_garde_une_edition_de_style() {
        use crate::object_manager::{
            reorder_series, selection_after_history, VisibilityTarget as T,
        };
        let mut before = PlotSpec::default();
        for name in ["A", "B", "C"] {
            let mut series = SeriesSpec::new(NodeId::from_raw(900));
            series.name = name.into();
            before.panels.get_mut(0).unwrap().series.push(series);
        }
        let mut after = before.clone();
        assert!(reorder_series(&mut after, SeriesRef::new(0, 0), 2));
        assert_eq!(
            selection_after_history(&after, &before, Some(T::Series(SeriesRef::new(0, 2)))),
            Some(T::Series(SeriesRef::new(0, 0)))
        );
        assert_eq!(
            selection_after_history(&before, &after, Some(T::Series(SeriesRef::new(0, 0)))),
            Some(T::Series(SeriesRef::new(0, 2)))
        );
        let mut formatted = after.clone();
        formatted.panels.get_mut(0).unwrap().series[2].line.width = 4.0;
        assert_eq!(
            selection_after_history(&formatted, &after, Some(T::Series(SeriesRef::new(0, 2)))),
            Some(T::Series(SeriesRef::new(0, 2)))
        );
    }
}
