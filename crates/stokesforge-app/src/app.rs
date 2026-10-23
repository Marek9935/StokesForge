//! La glu `eframe` : l'**orchestration** d'une image. [`StokesForgeApp`]
//! porte l'état de présentation (fenêtres flottantes et détachées, vues de
//! graphe, sélection du Project Manager) autour de l'[`AppState`], seul
//! dépositaire de la logique : tout ce qui décide est dans [`stokesforge_app_state::state`],
//! qui se teste sans fenêtre. Ce module ne garde que ce qu'un déplacement
//! casserait (#115, #122) ; chaque surface vit dans son module de `src/app/`,
//! avec sa doc de tête.
//!
//! # Ce qui reste ici
//!
//! [`StokesForgeApp::ui`](eframe::App::ui), et avec elle :
//!
//! - **l'ordre des panneaux**, qui fait la mise en page : avec `egui::Panel`,
//!   le premier panneau posé est le plus extérieur, chaque suivant s'insère
//!   dans ce que les précédents laissent. L'ordre est : barre de menu
//!   (`Panel::top`) → barres d'outils → barre d'état (`Panel::bottom`) →
//!   console de script (`Panel::bottom`) → Project Manager (`Panel::left`,
//!   sauf détaché) → Object Manager (`Panel::right`, ou flottant) →
//!   `CentralPanel` en dernier. La console, posée avant le Project Manager,
//!   court sous lui sur toute la largeur ; le Project Manager, posé avant le
//!   `CentralPanel`, n'est jamais recouvert par les fenêtres flottantes ni ne
//!   les recouvre. C'est aussi cet ordre qui donne à `viewport_rect` son
//!   étendue : ce que les panneaux fixes laissent — la portion *visible* de
//!   l'espace de travail, à distinguer de sa taille logique fixe
//!   (`WORKSPACE_SIZE`). Tenu par `les_panneaux_fixes_se_posent_dans_l_ordre_de_ui` ;
//! - **les emprunts disjoints** : une méthode `&mut self` appelée dans une
//!   fermeture qui emprunte déjà des champs ne compile plus, d'où les champs
//!   passés un à un (`show_workspace`) ;
//! - **l'application différée** : ouvertures, détachements, fermetures et
//!   actions récoltées pendant la peinture ne s'appliquent qu'une fois
//!   l'image peinte (muter `floating_tabs` en plein parcours l'invaliderait),
//!   dans la boucle de dispatch et ses cas liés à l'application
//!   (Enregistrer sans chemin, Nouveau projet avec saisies en cours,
//!   ouverture du graphe tracé ou du classeur importé) ;
//! - **les confirmations bloquantes** (nouveau projet, suppression de
//!   feuille) : elles vident les actions de l'image et coupent les
//!   raccourcis, en un seul endroit ;
//! - **les raccourcis globaux** ([`StokesForgeApp::collect_shortcuts`]),
//!   consommés au niveau du contexte, avant les widgets ;
//! - le **drainage des fenêtres détachées** (Project Manager et onglets de
//!   contenu), et l'image racine programmée quand une commande appliquée
//!   après la peinture change l'instantané ; la publication elle-même, qui
//!   réveille les viewports lecteurs, vit dans `window_detach.rs` (#104).
//!
//! Le module `tests` en fin de fichier n'éprouve que cette orchestration ;
//! chaque surface a ses tests dans son module (les tests suivent leur code,
//! #123). Depuis #124, `ui()` rend cet ordre explicite et délègue les phases
//! de récupération, dialogue et application différée à des méthodes privées.
//! Ces extractions n’ajoutent ni sous-Ui ni identifiant egui.
//!
//! # Où vit le reste
//!
//! La table complète module → rôle est dans `docs/application/coquille-et-code.md`, § Découpage
//! du code.
//!
//! - menus et barre d'état : `menus.rs` ([`StokesForgeApp::menu_bar`],
//!   [`StokesForgeApp::status_bar`]) ; barres de format et barre
//!   contextuelle : `format.rs` ; les barres elles-mêmes : `crate::toolbar` ;
//! - panneaux fixes, avec leur contenu : `project_manager.rs`
//!   (`project_manager_panel`, l'arbre `project_manager_contents_from_tree`),
//!   `script_console.rs` (`script_panel`), `object_manager_ui.rs`
//!   (`object_manager_panel`, `run_object_manager_request`) ;
//! - fenêtres de contenu : `tab.rs` (les quatre natures, [`Tab::Book`],
//!   [`Tab::Graph`], [`Tab::Matrix`], [`Tab::Note`]), `contents.rs`
//!   (`tab_contents`, la note, et `matrix_contents`, espace réservé
//!   documenté), `workbook.rs`, `grid.rs`, `graph_window.rs`,
//!   `graph_text_edit.rs`, `file_drop.rs` ; placement, pile, maximisation et
//!   cadre : `window_placement.rs`, `window_stack.rs`, `window_chrome.rs` ;
//!   détachement en fenêtres OS : `window_detach.rs` ;
//! - dialogues : `dialogs.rs` (brique commune), `import_wizard.rs`,
//!   `plot_settings.rs` ; raccourcis déclarés : `shortcuts.rs`.
//!
//! [`StokesForgeApp::open_tab`] (`window_stack.rs`) est le seul endroit qui
//! décide « ouvrir » vs « focaliser » un onglet, quelle que soit sa nature.
//!
//! # Ce que Qt donnait gratuitement et qu'il faut écrire ici
//!
//! Une `QAction` reliée à un `QUndoStack` portait d'un coup le raccourci
//! clavier, l'entrée de menu, son grisage et son libellé (« Annuler
//! *Renommer* »). Ici les quatre sont séparés : le raccourci est déclaré
//! (`shortcuts.rs`) et consommé à la main ([`StokesForgeApp::collect_shortcuts`]),
//! l'entrée et son grisage (`add_enabled`) sont dans `menus.rs`, le libellé
//! vient de [`AppState::undo_label`], et rien ne garantit leur cohérence sinon
//! la relecture — et les tests de `menus.rs`, qui lisent le grisage de chaque
//! menu de haut niveau.
//!
//! # Panneaux fixes (#100)
//!
//! PM reste à gauche, OM à droite et console en bas. Le repli garde une
//! prise visible ; le masquage se commande depuis View. Les dimensions du
//! panneau déployé sont conservées par des identifiants de bande distincts.
//! Chaque panneau peut se détacher en fenêtre OS avec Rattacher uniquement ;
//! aucune bascule de côté ni flottant interne. `panel_header` et les lectures
//! et requêtes de `window_detach.rs` assurent l'accord entre les deux modes.
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
//! parade n'est **pas** ici : elle vit dans [`stokesforge_app_state::state::AppState::
//! dispatch`], qui traite `RunScript` en lançant le travail sur un fil
//! détaché et en revenant aussitôt (voir la doc de tête de `stokesforge_app_state::state`).
//! Ce module n'a donc que deux choses à faire, chaque image : appeler
//! [`AppState::poll_script`] pour récupérer un résultat prêt, et demander un
//! nouveau rendu ([`egui::Context::request_repaint`]) tant qu'un script
//! tourne — sans quoi, egui ne redessinant que sur événement d'entrée, la fin
//! du script n'apparaîtrait qu'au prochain mouvement de souris.

mod analysis_dialog;
mod analysis_forms;
mod analysis_handles;
mod analysis_menu;
mod analysis_preview;
mod busy;
mod column_use;
mod confirmations;
mod contents;
mod cursors;
mod curve_settings;
mod dialogs;
mod empty_state;
mod figure_copy;
mod file_drop;
mod form;
mod format;
mod graph_targets;
mod graph_text_edit;
mod graph_window;
mod grid;
pub(crate) mod hover_help;
mod import_wizard;
mod inplace_edit;
mod keyboard_shortcuts_window;
mod list_row;
mod menu_sections;
mod menus;
mod object_dnd;
mod object_manager_ui;
#[cfg(test)]
mod panel_tests;
mod plot_settings;
#[cfg(test)]
mod preferences_proto;
mod preferences_window;
mod project_manager;
mod recovery;
mod reorder_list;
mod results_pane;
mod script_console;
pub(crate) mod shortcuts;
mod stale_export;
mod status_history;
mod tab;
pub(crate) mod tab_strip;
#[cfg(test)]
mod test_support;
mod transpose_ui;
mod window_chrome;
mod window_detach;
mod window_placement;
mod window_stack;
mod workbook;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use analysis_dialog::{analysis_replace_window, analysis_window, calculation_delete_window};
use column_use::{column_use_window, full_copy_window};
use file_drop::{
    begin_file_drop, finish_file_drop, open_project_in_new_window, remember_recent_imports,
};
use graph_text_edit::{
    annotation_editor_id, pending_format_id, pending_zone_font_id, text_editor_id,
    AnnotationEditor, TextEditor,
};
use graph_window::request_graph_export;
use grid::keyboard_focus_outside_grid;
use import_wizard::import_window;
use keyboard_shortcuts_window::keyboard_shortcuts_window;
use object_manager_ui::{select_manager_target, selected_manager_target};
use plot_settings::{column_metadata_window, plot_settings_window};
use preferences_window::preferences_window;
use recovery::recovery_window;
use shortcuts::{
    collect_undo_redo_shortcuts, SHORTCUT_DUPLICATE_SHEET, SHORTCUT_EXPORT_GRAPH, SHORTCUT_IMPORT,
    SHORTCUT_IMPORT_QWERTY, SHORTCUT_IMPORT_TEXT, SHORTCUT_NEW_PROJECT, SHORTCUT_OPEN,
    SHORTCUT_REDO_Y, SHORTCUT_RUN_SCRIPT, SHORTCUT_SAVE, SHORTCUT_SAVE_AS, SHORTCUT_UNDO,
};
use stale_export::stale_export_window;
use status_history::{low_r2_list_window, status_history_window};
use stokesforge_app_state::file_picker::{FilePicker, PickRequest};
use tab::{active_tab_kind, tab_for_window, tab_node, Tab};
use transpose_ui::transpose_relink_window;
use window_detach::{
    snapshot_stamp, DetachedProjectManagerRequests, DetachedTab, ProjectManagerViewModel,
};
use window_placement::geometry_rect;
use window_stack::window_nodes;

use eframe::egui;
use stokesforge_core::{Node, NodeId};
use stokesforge_render_egui::GraphView;
use stokesforge_session::WindowGeometry;

use crate::color_picker::publish_saved_colors;
use crate::toolbar::{BarEnv, ToolbarLayout, ToolbarState};
use stokesforge_app_state::errors;
use stokesforge_app_state::i18n;
use stokesforge_app_state::preferences::Preferences;
use stokesforge_app_state::state::{
    active_graph, Action, AppState, ImportDestination, UnsavedChoice,
};
use stokesforge_app_state::tr;

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

/// L'application `eframe` : l'état du banc, la disposition en fenêtres
/// flottantes confinées, et le strict nécessaire au rendu.
pub struct StokesForgeApp {
    /// L'état, seul dépositaire de la logique.
    state: AppState,
    /// Sélecteur de fichiers natif en cours, sur un fil à part (#192).
    file_picker: FilePicker,
    /// Dernier titre envoyé à la fenêtre (S-R15) : `Title` ne repart qu'au changement.
    window_title: String,
    /// Les barres d'outils : disposition, visibilité et glisser en cours
    /// (issue #60).
    toolbars: ToolbarState,
    om_detached: bool,
    om_collapsed: bool,
    om_view: Option<window_detach::DetachedObjectManager>,
    /// Visibilité et repli sont des états de vue, hors historique et format .stf.
    pm_open: bool,
    pm_collapsed: bool,
    script_open: bool,
    script_detached: bool,
    script_view: Option<window_detach::DetachedScriptConsole>,
    active_detached: Option<Tab>,
    /// Les onglets de contenu actuellement ouverts en fenêtre flottante, dans
    /// l'ordre où ils ont été focalisés pour la dernière fois — le dernier
    /// élément est le plus récemment focalisé (voir la doc de tête de
    /// `window_stack.rs`, § Fenêtres flottantes confinées).
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
    /// les [`CASCADE_WRAP`](crate::app::window_placement::CASCADE_WRAP) pour ne pas dériver hors de l'aire de travail.
    next_cascade: u32,
    /// Onglet à ramener au premier plan au prochain rendu — posé par
    /// [`StokesForgeApp::open_tab`] quand l'onglet rouvert était déjà une fenêtre
    /// flottante existante (voir la doc de tête de `window_stack.rs`).
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
    /// Onglet actuellement maximisé, s'il y en a un — voir la doc de tête de
    /// `window_stack.rs`, § Maximiser un onglet. Ne peut plus valoir le Project Manager depuis
    /// cette tâche : ce n'est plus un onglet, et un panneau fixe toujours
    /// entièrement visible n'a rien à gagner à être « maximisé ».
    maximized_tab: Option<Tab>,
    /// Vrai quand le Project Manager est détaché en fenêtre OS séparée (voir
    /// la doc de tête de `window_detach.rs`, § Détachement du Project Manager). Dans cet état,
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
    /// [`StokesForgeApp::ui`](eframe::App::ui) — même rôle que `open_requests`/`actions` pour le
    /// reste du banc, juste posé par un callback différé plutôt que collecté
    /// en direct.
    pm_requests: Arc<Mutex<DetachedProjectManagerRequests>>,
    /// Onglets de contenu (`Book`/`Graph`/`Matrix`/`Note`) actuellement
    /// détachés en fenêtre OS séparée — voir la doc de tête de
    /// `window_detach.rs`, § Détachement générique des onglets de contenu. Une entrée par onglet détaché ; absent de
    /// `floating_tabs` tant qu'il l'est (même invariant que `pm_detached`
    /// pour le Project Manager, généralisé à plusieurs onglets simultanés).
    detached_tabs: HashMap<Tab, DetachedTab>,
    /// Position de chaque fenêtre flottante, en coordonnées logiques de
    /// l'espace de travail, déplacée par la barre de titre maison et bornée
    /// par `clamp_to_workspace` — seule source de vérité de la position.
    window_positions: HashMap<Tab, egui::Pos2>,
    /// Taille logique de chaque fenêtre flottante, modifiée par les
    /// poignées de redimensionnement maison ([`floating_resize_handles`](crate::app::window_chrome::floating_resize_handles),
    /// 4 côtés + 4 coins) — seule source de vérité de la taille, comme
    /// `window_positions` l'est de la position.
    window_sizes: HashMap<Tab, egui::Vec2>,
    /// Défilement courant de l'espace de travail (zone MDI, taille fixe
    /// [`WORKSPACE_SIZE`](crate::app::window_placement::WORKSPACE_SIZE)) — voir sa doc. `window_positions`/`window_sizes`
    /// ci-dessus, et la géométrie persistée dans le projet, sont exprimées
    /// dans les coordonnées logiques de cet espace, pas en pixels écran ;
    /// [`StokesForgeApp::ui`](eframe::App::ui) applique ce décalage à la peinture. Transitoire, comme la position de défilement d'un
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
    /// Dernier contenu actif observé pour suivre la sélection simple (#146).
    pm_last_active_tab: Option<Tab>,
    /// Fichier où déposer une capture de la fenêtre, s'il en est demandé une
    /// (mécanisme de vérification, voir [`StokesForgeApp::screenshot`]).
    screenshot: Option<std::path::PathBuf>,
    /// Images restant à laisser passer avant de demander la capture.
    screenshot_delay: u32,
    /// Délai d'auto-fermeture, s'il est demandé (mécanisme de vérification).
    autoquit: Option<Duration>,
    /// Instant du premier rendu, d'où se mesure le délai d'auto-fermeture.
    first_frame: Option<Instant>,
    /// La fenêtre a demandé à se fermer (croix, Alt+F4, Quit) : à poser à
    /// l'état comme `Action::Quit` (#229).
    close_requested: bool,
    /// L'état a libéré la fermeture : la prochaine demande n'est plus retenue.
    quit_confirmed: bool,
}

impl StokesForgeApp {
    /// Construit l'application autour d'un état.
    ///
    /// # Disposition initiale
    ///
    /// Ni le Project Manager ni la console de script n'entrent pour rien
    /// dans ce plan : ce sont des panneaux fixes peints à part (voir
    /// [`StokesForgeApp::project_manager_panel`] et [`StokesForgeApp::script_panel`]),
    /// pas des fenêtres flottantes. Si le projet contient au moins un
    /// classeur, son onglet s'ouvre en fenêtre flottante, suivi de l'onglet
    /// du premier graphe s'il y en a un — montrer quelque chose sans qu'on
    /// ait à cliquer, comme l'ancien comportement par défaut. À défaut de
    /// classeur ou de graphe, aucune fenêtre flottante ne s'ouvre (seuls les
    /// panneaux fixes et la barre de menu sont visibles).
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

        let window_title = state.window_title();
        Self {
            state,
            file_picker: FilePicker::default(),
            window_title,
            toolbars: ToolbarState::default(),
            om_detached: false,
            om_collapsed: false,
            om_view: None,
            pm_open: true,
            pm_collapsed: false,
            script_open: true,
            script_detached: false,
            script_view: None,
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
            pm_last_active_tab: None,
            autoquit,
            first_frame: None,
            close_requested: false,
            quit_confirmed: false,
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
        if self.state.blocking_dialog() {
            return;
        }
        let mut save_as = false;
        let mut open = false;
        let mut import = false;
        let mut export = false;
        // Les gestes de feuille ne s'appliquent pas pendant une saisie dans un
        // champ, ni pendant l'édition du nom d'un onglet.
        let typing = keyboard_focus_outside_grid(ctx)
            || self.state.renaming_sheet().is_some()
            || project_manager::has_focus(ctx);
        let active_sheet = self.active_content_tab(ctx).and_then(|tab| match tab {
            Tab::Book(book) => self.state.selected_sheet_in(book),
            _ => None,
        });
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

    /// Demande le fichier à ouvrir : le sélecteur s'ouvre sur un fil à part
    /// (#192), la réponse revient par [`StokesForgeApp::poll_file_picker`].
    fn choose_open_path(&mut self, actions: &mut Vec<Action>) {
        actions.push(Action::PickFile(PickRequest::OpenProject));
    }

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
            self.state.notice(tr!(
                "status-toolbars-not-saved",
                detail = errors::io::error_message(&err)
            ));
        }
    }

    /// Applique la langue tout de suite (les textes se redessinent à l'image
    /// suivante) et l'enregistre dans `preferences.json`. Un enregistrement
    /// qui échoue se dit dans la barre d'état ; la langue reste appliquée.
    fn choose_language(&mut self, language: i18n::Language) {
        i18n::set_language(language);
        if let Err(err) = Preferences::load().save_language(language.tag()) {
            self.state.notice(tr!(
                "status-language-not-saved",
                detail = errors::io::error_message(&err)
            ));
        }
    }

    /// Demande la destination de sauvegarde ; même circuit qu'à l'ouverture.
    fn choose_save_path(&mut self, actions: &mut Vec<Action>) {
        actions.push(Action::PickFile(PickRequest::SaveProjectAs {
            suggested: suggested_file_name(self.state.path()),
        }));
    }

    /// Démarre les sélecteurs demandés, puis applique la réponse de celui qui
    /// vient de finir (#192). Ne bloque jamais.
    fn poll_file_picker(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if self.file_picker.is_open() {
            // Filet : le fil réveille l'interface, ceci couvre un réveil perdu.
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        let Some((request, picked)) = self.file_picker.poll() else {
            return;
        };
        let Some(paths) = picked else {
            // Sans fichier choisi, un geste qui attendait l'enregistrement
            // (réponse Save, #229) est abandonné.
            if matches!(request, PickRequest::SaveProjectAs { .. }) {
                actions.push(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
            }
            return;
        };
        let first = paths.first().cloned();
        match request {
            PickRequest::OpenProject => {
                if let Some(path) = first {
                    actions.push(Action::OpenProject {
                        path: path.display().to_string(),
                        pending_edits: false,
                    });
                }
            }
            PickRequest::SaveProjectAs { .. } => {
                if let Some(path) = first {
                    actions.push(Action::SaveAs {
                        path: path.display().to_string(),
                    });
                }
            }
            PickRequest::ExportGraph { spec, .. } => {
                if let Some(path) = first {
                    actions.push(Action::ExportGraph { path, spec });
                }
            }
            PickRequest::ExportImportRecipe { .. } => {
                if let Some(path) = first {
                    self.state.export_import_recipe(&path);
                }
            }
            PickRequest::ImportImportRecipe => {
                if let Some(path) = first {
                    self.state.import_import_recipe(&path);
                }
            }
            PickRequest::AddImportFiles { .. } => self.state.add_import_paths(paths),
        }
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

    /// Ferme la fenêtre quand l'état a accepté Quit (#229).
    fn close_when_ready(&mut self, ctx: &egui::Context) {
        if self.state.take_quit_ready() {
            self.quit_confirmed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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
            // Outillage de vérification : ne demande rien (#229).
            self.quit_confirmed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            // Sans événement d'entrée, egui ne redessine pas : il faut lui
            // demander de se réveiller, sinon l'échéance n'arrive jamais.
            ctx.request_repaint_after(delay - elapsed);
        }
    }
}

impl eframe::App for StokesForgeApp {
    /// Une fermeture de la fenêtre (croix, Alt+F4, File › Quit) passe par la
    /// question « Discard Changes · Cancel · Save » (S-F3, #229) : retenue ici
    /// (aussi fenêtre réduite, où seul `logic` tourne), elle devient
    /// `Action::Quit`, et la fenêtre se ferme quand l'état la libère.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.quit_confirmed && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_requested = true;
        }
        // Sauvegarde automatique (#317) : ici et non dans `ui()`, qui ne tourne
        // pas fenêtre réduite ; le réveil est programmé sur la prochaine
        // échéance, aucun redessin périodique au repos.
        if let Some(delay) = self.state.poll_autosave(Instant::now()) {
            ctx.request_repaint_after(delay);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // `Context` est un `Arc` : le clone évite l’emprunt de lecture du Ui peint.
        let ctx = ui.ctx().clone();
        self.sync_window_title_and_status(&ctx);
        let published_stamp = snapshot_stamp(self.state.snapshot());
        let mut actions = Vec::new();
        // Ces files ne s’appliquent qu’après la peinture de la pile flottante.
        let mut open_requests = Vec::new();
        let mut detach_requests = Vec::new();
        let mut close_requests = Vec::new();
        let mut geometry_updates = Vec::new();
        let active_tab = self.active_content_tab(&ctx);
        self.follow_project_manager_active_tab(&ctx, active_tab);

        begin_file_drop(&ctx);
        self.poll_background_jobs(&ctx);
        self.poll_file_picker(&ctx, &mut actions);
        if std::mem::take(&mut self.close_requested) {
            actions.push(Action::Quit {
                pending_edits: false,
            });
        }
        publish_saved_colors(&ctx, &self.state);
        let (metadata, x_prime) = self.state.workbook_header_defaults();
        grid::publish_header_row_defaults(&ctx, metadata, x_prime);
        analysis_menu::publish_last_presets(&ctx, &self.state);
        graph_targets::publish(&ctx, &self.floating_tabs, self.detached_tabs.keys());
        self.collect_shortcuts(&ctx, &mut actions);
        // Ctrl+C sur le graphique actif, avant que panneaux et contenus ne lisent
        // l’`Event::Copy` (#149).
        self.request_active_figure_copy(&ctx, active_tab, &mut actions);

        // L’ordre de ces appels détermine l’emprise des panneaux egui (#115).
        self.menu_bar(ui, &ctx, &mut actions, active_tab);
        self.show_toolbars(ui, &ctx, active_tab, &mut actions);
        self.status_bar(ui, &mut actions);
        self.script_panel(ui, &ctx, &mut actions);
        self.show_project_manager(
            ui,
            &ctx,
            active_tab,
            &mut open_requests,
            &mut close_requests,
            &mut actions,
        );
        self.object_manager_panel(ui, active_tab, &mut actions);
        self.drain_detached_tabs(&ctx, &mut actions);
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
        self.show_edit_dialogs(&ctx, &mut actions);
        if let Some(action) = finish_file_drop(&ctx) {
            actions.push(action);
        }
        for project in self.state.take_projects_to_open() {
            open_project_in_new_window(&project);
        }
        self.apply_window_requests(&ctx, open_requests, detach_requests, close_requests);
        self.blocking_confirmations(&ctx, &mut actions);
        self.dispatch_actions(&ctx, actions);
        self.close_when_ready(&ctx);
        self.offer_figure(frame);

        self.sync_window_views();
        if snapshot_stamp(self.state.snapshot()) != published_stamp {
            // Une commande appliquée après la publication réveille les viewports.
            ctx.request_repaint();
        }
        self.screenshot(&ctx);
        self.autoquit(&ctx);
    }
}

impl StokesForgeApp {
    /// Récupère les travaux terminés avant que les panneaux lisent l’instantané.
    fn poll_background_jobs(&mut self, ctx: &egui::Context) {
        // Sonde un script terminé sur son fil détaché **avant** de peindre :
        // sans notification poussée de l'acteur, c'est le seul endroit qui
        // rafraîchit l'instantané après un script (voir la doc de tête de
        // `stokesforge_app_state::state`). Ne bloque jamais, même sans script en cours.
        self.state.poll_script();
        // Le travail continue aussi quand sa console est masquée ou repliée.
        if self.state.script_running() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        // Recalcul de formules que la commande n'a pas attendu (#239) : on
        // reprend la main à sa fin ; en attendant, l'image suivante vient.
        self.state.poll_recalc();
        if self.state.recalc_work().is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
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
    }

    /// Peint les barres au même rang entre menus et statut, sans créer de sous-Ui.
    fn show_toolbars(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
    ) {
        let bar_env = BarEnv {
            root: Some(self.state.tree().root()),
            undo_label: self.state.undo_label(),
            redo_label: self.state.redo_label(),
            new_project_enabled: self.state.can_new_project(),
            save_blocked: self.state.script_running(),
            export_enabled: active_graph(active_tab_kind(active_tab)).is_some(),
            tools: self.tools_context(active_tab),
            text_format: self.format_target(ctx, active_tab),
            text_style: self.text_format_state(ctx, active_tab),
            object_format: self.object_format_state(active_tab),
            shortcuts: crate::toolbar::ShortcutTexts {
                new_project: ctx.format_shortcut(&SHORTCUT_NEW_PROJECT),
                open: ctx.format_shortcut(&SHORTCUT_OPEN),
                save: ctx.format_shortcut(&SHORTCUT_SAVE),
                save_as: ctx.format_shortcut(&SHORTCUT_SAVE_AS),
                import: SHORTCUT_IMPORT_TEXT.to_string(),
                export: ctx.format_shortcut(&SHORTCUT_EXPORT_GRAPH),
                undo: ctx.format_shortcut(&SHORTCUT_UNDO),
                redo: ctx.format_shortcut(&SHORTCUT_REDO_Y),
            },
        };
        let mut bar_requests = Vec::new();
        self.toolbars.show(ui, &bar_env, &mut bar_requests);
        for request in bar_requests {
            self.run_toolbar_request(ctx, request, active_tab, actions);
        }
        if self.toolbars.take_changed() {
            self.save_toolbar_layout();
        }
    }

    /// Le panneau ou son viewport détaché occupe une seule étape de l’image.
    fn show_project_manager(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        active_tab: Option<Tab>,
        open_requests: &mut Vec<Tab>,
        close_requests: &mut Vec<Tab>,
        actions: &mut Vec<Action>,
    ) {
        if self.pm_detached {
            // Drainer avant la publication évite de publier une sélection périmée.
            // Vidange ce que la fenêtre OS détachée a écrit depuis la
            // dernière image (voir la doc de tête de
            // `show_detached_project_manager`) — même moment que le reste de
            // `open_requests`, avant de peindre le panneau fixe (qui ne sera
            // de toute façon pas peint cette image-ci) et le `CentralPanel`.
            let (should_reattach, new_selection, mut new_close) = {
                let mut requests = self
                    .pm_requests
                    .lock()
                    .unwrap_or_else(|err| err.into_inner());
                open_requests.append(&mut requests.open);
                actions.append(&mut requests.actions);
                (
                    std::mem::take(&mut requests.reattach),
                    requests.selection.take(),
                    std::mem::take(&mut requests.close),
                )
            };
            if let Some(selection) = new_selection {
                self.pm_selection = selection;
            }
            close_requests.append(&mut new_close);
            if should_reattach {
                self.reattach_project_manager();
                ctx.request_repaint();
            }
        }
        if !self.pm_open {
            return;
        }
        if self.pm_detached {
            self.show_detached_project_manager(ctx, active_tab);
        } else {
            // Panneau fixe, peint *avant* le `CentralPanel` — voir la doc de
            // tête du module, § Ce qui reste ici : c'est ce qui garantit par
            // construction qu'il n'est jamais recouvert par lui ni ne le
            // recouvre, comme la barre de menu (`egui::Panel::top`) peinte
            // plus haut.
            self.project_manager_panel(ui, open_requests, close_requests, actions);
        }
    }

    /// Les requêtes détachées restent drainées avant l’espace de travail.
    fn drain_detached_tabs(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if !self.detached_tabs.is_empty() {
            self.show_detached_tabs(ctx);
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
    }

    /// Titre de la fenêtre à jour (S-R15) et effacement des informations
    /// échues (S-M5) : un seul redessin est programmé, à l'échéance, jamais de
    /// réveil périodique au repos.
    fn sync_window_title_and_status(&mut self, ctx: &egui::Context) {
        let title = self.state.window_title();
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
        if let Some(remaining) = self.state.expire_status(std::time::Instant::now()) {
            ctx.request_repaint_after(remaining);
        }
    }

    /// Peint les dialogues après le CentralPanel, avant l’application différée.
    fn show_edit_dialogs(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        // Fenêtre d'import (`Action::ImportFiles`, voir la doc de tête de
        // [`import_window`]) : un dialogue transitoire, peint tant que
        // `self.state.import_state()` est `Some`, plutôt qu'une fenêtre
        // flottante de plus — après le `CentralPanel` mais avant la vidange
        // des requêtes ci-dessous, pour que l'`Action::ImportFiles` qu'elle
        // pousse éventuellement soit dispatchée cette image-ci comme toute
        // autre action.
        import_window(ctx, &mut self.state, actions);
        // Fenêtre des propriétés d'un graphe : même famille de dialogue
        // transitoire que la fenêtre d'import, peinte tant que
        // `self.state.plot_settings()` est `Some`.
        plot_settings_window(ctx, &mut self.state);
        column_metadata_window(ctx, &self.state, actions);
        status_history_window(ctx, &self.state, actions);
        low_r2_list_window(ctx, &self.state, actions);
        keyboard_shortcuts_window(ctx, &self.state, actions);
        if let Some(language) = preferences_window(ctx, &self.state, actions) {
            self.choose_language(language);
        }
        analysis_window(ctx, &mut self.state, actions);
        calculation_delete_window(ctx, &self.state, actions);
        analysis_replace_window(ctx, &self.state, actions);
        transpose_relink_window(ctx, &self.state, actions);
        stale_export_window(ctx, &self.state, actions);
        column_use_window(ctx, &self.state, actions);
        full_copy_window(ctx, &self.state, actions);
        recovery_window(ctx, &self.state, actions);
        dialogs::sync_dialogs(ctx, &self.state, actions);
    }

    /// Applique les changements de pile seulement après la peinture des fenêtres.
    fn apply_window_requests(
        &mut self,
        ctx: &egui::Context,
        open_requests: Vec<Tab>,
        detach_requests: Vec<Tab>,
        close_requests: Vec<Tab>,
    ) {
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
        // Même chemin que la croix : la fenêtre flottante ou détachée se ferme.
        for tab in &close_requests {
            self.close_detached_tab(*tab);
        }
        self.close_floating_tabs(close_requests);
    }

    /// Les onglets ouverts, flottants ou détachés (#327).
    fn open_tabs(&self) -> HashSet<Tab> {
        self.floating_tabs
            .iter()
            .chain(self.detached_tabs.keys())
            .copied()
            .collect()
    }

    /// Une confirmation remplace toutes les actions déjà récoltées dans cette image.
    fn blocking_confirmations(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if self.state.blocking_dialog() {
            // Seul le dialogue peut agir sur cette image : clavier, dépôts et
            // requêtes des viewports détachés sont aussi bloqués.
            actions.clear();
        }
        confirmations::show(ctx, &self.state, actions);
    }

    /// La saisie des vues externes compte dans la question « Discard Changes ·
    /// Cancel · Save » de New Project, Open… et Quit (#229).
    fn prepare_new_project_action(&self, ctx: &egui::Context, action: Action) -> Action {
        if let Action::NewProject { pending_edits }
        | Action::OpenProject { pending_edits, .. }
        | Action::Quit { pending_edits } = action
        {
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
            match action {
                Action::OpenProject { path, .. } => Action::OpenProject {
                    path,
                    pending_edits,
                },
                Action::Quit { .. } => Action::Quit { pending_edits },
                _ => Action::NewProject { pending_edits },
            }
        } else {
            action
        }
    }

    /// Une demande de confirmation arrête aussi les actions restantes de l’image.
    fn dispatch_actions(&mut self, ctx: &egui::Context, actions: Vec<Action>) {
        for action in actions {
            // Sans destination, « Enregistrer » demande d'abord où écrire ; la
            // réponse du sélecteur pousse ensuite `Action::Save` (#192). La
            // réponse Save à « Discard Changes · Cancel · Save » (#229) fait
            // de même, une fois que l'état a noté le geste à poursuivre.
            let action = if matches!(action, Action::ConfirmUnsaved(UnsavedChoice::Save))
                && self.state.path().is_empty()
            {
                self.state.dispatch(action);
                Action::Save
            } else {
                action
            };
            let action = if matches!(action, Action::Save) && self.state.path().is_empty() {
                Action::PickFile(PickRequest::SaveProjectAs {
                    suggested: suggested_file_name(self.state.path()),
                })
            } else {
                action
            };
            if let Action::PickFile(request) = action {
                let wake = ctx.clone();
                self.file_picker
                    .start(request, move || wake.request_repaint());
                continue;
            }
            let action = self.prepare_new_project_action(ctx, action);
            if self.dispatch_action(ctx, action) {
                break;
            }
        }
    }

    /// Capture les effets de présentation avant la commande, puis les applique à son résultat.
    fn dispatch_action(&mut self, ctx: &egui::Context, action: Action) -> bool {
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
        // *Add Folder* (#326) : même motif que `plotted_from` — le dossier neuf
        // est le dernier enfant de son parent, et son nom s'édite sur place.
        let folder_parent = if let Action::AddFolder { parent } = &action {
            Some(*parent)
        } else {
            None
        };
        let pm_viewport = if self.pm_detached {
            project_manager::viewport_id()
        } else {
            egui::ViewportId::ROOT
        };
        let new_import_books_after = if matches!(
            action,
            Action::ImportFiles {
                destination: ImportDestination::NewBookPerFile | ImportDestination::NewBookForAll,
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
        // Enregistrer peut poursuivre un New Project ou un Open… (#229).
        let replaces_project = matches!(
            action,
            Action::NewProject { .. }
                | Action::OpenProject { .. }
                | Action::Load
                | Action::Save
                | Action::SaveAs { .. }
                | Action::ConfirmUnsaved(_)
        );
        // L'historique et les gestes qui changent le rang des séries ou la
        // composition d'un groupe (#197, dont sa suppression : plusieurs
        // séries partent à la fois) font suivre la sélection par contenu.
        let history_selections: Vec<_> = if matches!(
            action,
            Action::Undo
                | Action::Redo
                | Action::MoveSeries { .. }
                | Action::MoveSeriesGroup { .. }
                | Action::PlaceSeries { .. }
                | Action::PlaceSeriesGroup { .. }
                | Action::ReorderObject { .. }
                | Action::UngroupSeries { .. }
                | Action::RemovePlotObject {
                    target: stokesforge_app_state::object_manager::VisibilityTarget::SeriesGroup(_),
                    ..
                }
        ) {
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
                    let previous =
                        stokesforge_core::compose::current_plot_spec(self.state.tree(), graph)?;
                    Some((graph, target, previous))
                })
                .collect()
        } else {
            Vec::new()
        };
        let selection_change = match &action {
            Action::RemovePlotObject { graph, target } => Some((*graph, *target)),
            Action::RemoveAnnotation { graph, target } => Some((
                *graph,
                stokesforge_app_state::object_manager::VisibilityTarget::Annotation(*target),
            )),
            Action::RemoveShape { graph, target } => Some((
                *graph,
                stokesforge_app_state::object_manager::VisibilityTarget::Shape(*target),
            )),
            _ => None,
        };
        self.state.dispatch(action);
        if let Some(text) = self.state.take_object_clipboard_text() {
            ctx.copy_text(text);
        }
        for (source, destination, duplicate) in self.state.take_copied_objects() {
            let source_open = self
                .floating_tabs
                .iter()
                .chain(self.detached_tabs.keys())
                .any(|tab| tab_node(*tab) == source);
            if !duplicate || source_open {
                if let Some((id, kind)) = window_nodes(&self.state.project_tree())
                    .into_iter()
                    .find(|(id, _)| *id == destination)
                {
                    self.open_tab(tab_for_window(id, kind));
                }
            }
        }
        let removed: Vec<_> = self
            .floating_tabs
            .iter()
            .chain(self.detached_tabs.keys())
            .copied()
            .filter(|tab| {
                self.state.tree().node(tab_node(*tab)).is_none_or(
                    |node| matches!(node.kind(), stokesforge_core::NodeKind::Workbook { meta, .. } if meta.hidden),
                )
            })
            .collect();
        for tab in &removed {
            if self.detached_tabs.remove(tab).is_some() {
                ctx.send_viewport_cmd_to(
                    egui::ViewportId::from_hash_of(tab),
                    egui::ViewportCommand::Close,
                );
            }
            self.pm_selection.remove(tab);
            self.collapsed_tabs.remove(tab);
            self.graph_views.remove(&tab_node(*tab));
            if self.active_detached == Some(*tab) {
                self.active_detached = None;
            }
        }
        self.close_floating_tabs(removed);

        if self.state.revision() != revision_before {
            if let Some((graph, removed)) = selection_change {
                self.update_selection_after_action(graph, removed);
            }
        }
        if self.state.revision() != revision_before {
            for (graph, target, previous) in history_selections {
                if let Some(next) =
                    stokesforge_core::compose::current_plot_spec(self.state.tree(), graph)
                {
                    let target = stokesforge_app_state::object_manager::selection_after_history(
                        &previous,
                        &next,
                        Some(target),
                    );
                    self.with_graph_view(graph, |view| select_manager_target(view, target, &next));
                }
            }
        }
        if replaces_project && self.state.snapshot().epoch != epoch_before {
            self.forget_project_views(ctx);
        }
        if self.state.blocking_dialog() {
            ctx.request_repaint();
            return true;
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
        if let Some(parent) = folder_parent {
            if self.state.revision() != revision_before {
                if let Some(folder) = self
                    .state
                    .tree()
                    .node(parent)
                    .and_then(|node| node.children().last().copied())
                {
                    project_manager::begin_rename_of_new_folder(ctx, pm_viewport, folder, parent);
                }
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
        false
    }

    /// La suppression fait suivre la sélection seulement si la commande a agi.
    fn update_selection_after_action(
        &mut self,
        graph: NodeId,
        removed: stokesforge_app_state::object_manager::VisibilityTarget,
    ) {
        if let Some(spec) = stokesforge_core::compose::current_plot_spec(self.state.tree(), graph) {
            self.with_graph_view(graph, |view| {
                let next = stokesforge_app_state::object_manager::selection_after_remove(
                    selected_manager_target(view),
                    removed,
                );
                select_manager_target(view, next, &spec);
            });
        }
    }

    /// Un remplacement d’identité oublie les vues qui pourraient viser des NodeId réemployés.
    fn forget_project_views(&mut self, ctx: &egui::Context) {
        project_manager::forget_views(ctx);
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
        self.pm_last_active_tab = None;
        self.pm_requests = Arc::new(Mutex::new(DetachedProjectManagerRequests::default()));
        ctx.memory_mut(|memory| {
            // Les éditeurs sur place et formats différés ne doivent
            // pas survivre à la fenêtre qu'ils visaient. Garder les
            // préférences et dimensions des panneaux de l'application.
            memory.data.remove_by_type::<TextEditor>();
            memory.data.remove_by_type::<AnnotationEditor>();
            memory
                .data
                .remove::<Option<stokesforge_app_state::text_format::TextFormat>>(
                    pending_format_id(),
                );
            memory.data.remove::<Option<String>>(pending_zone_font_id());
            memory.data.remove_by_type::<egui::scroll_area::State>();
            if let Some(focused) = memory.focused() {
                memory.surrender_focus(focused);
            }
        });
        ctx.request_repaint();
    }
}

/// Le nom de fichier suggéré par défaut au sélecteur « Enregistrer sous… » :
/// le nom du chemin courant s'il en a un, sinon un nom de repli.
fn suggested_file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "demo.stf".to_string())
}

#[cfg(test)]
mod tests {
    //! L'orchestration seule (#115, #123) : l'ordre des panneaux, la
    //! publication programmée après une commande détachée, la confirmation
    //! bloquante du nouveau projet, les raccourcis globaux, la restauration
    //! des fenêtres par [`StokesForgeApp::new`], et la mesure d'une image
    //! complète. Le détachement se teste dans `window_detach.rs`, chaque
    //! surface dans son module.
    use super::graph_window::tests::graphe;
    use super::grid::{header_rows, set_header_rows, HeaderRows};
    use super::*;
    use crate::theme;
    use stokesforge_app_state::state::RowAxis;
    use stokesforge_core::{Column, Project};
    use stokesforge_plot::{PanelSpec, PlotSpec, SeriesSpec};
    use stokesforge_session::Session;

    /// Le Project Manager suit la fenêtre sur laquelle on appuie dès la
    /// pression, souris maintenue, et surligne sa ligne dans l'image réclamée
    /// par la pression, sans attendre un mouvement de souris.
    #[test]
    fn la_selection_du_project_manager_suit_la_pression_sur_une_fenetre() {
        use eframe::App as _;
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let book = window_stack::window_nodes(&state.project_tree())
            .into_iter()
            .find(|(_, kind)| *kind == stokesforge_app_state::state::WindowKind::Book)
            .map(|(id, _)| Tab::Book(id))
            .unwrap();
        let tab = Tab::Graph(graph);
        let mut app = StokesForgeApp::new(state);
        app.floating_tabs = vec![book, tab];
        app.window_positions.insert(tab, egui::pos2(0.0, 0.0));
        app.window_positions.insert(book, egui::pos2(640.0, 300.0));
        let ctx = egui::Context::default();
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let mut frame = eframe::Frame::_new_kittest();
        let mut now = 0.0;
        let mut run = |app: &mut StokesForgeApp, events: Vec<egui::Event>| {
            now += 0.05;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    time: Some(now),
                    events,
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut frame),
            );
            output.textures_delta.clear();
            output
        };
        for _ in 0..5 {
            run(&mut app, Vec::new());
        }
        assert_eq!(app.pm_selection, HashSet::from([tab]));
        let output = run(&mut app, Vec::new());
        let title = |output: &egui::FullOutput, wanted: &str, left: bool| {
            output.shapes.iter().find_map(|sh| match &sh.shape {
                egui::Shape::Text(t)
                    if t.galley.job.text == wanted && (t.pos.x < 300.0) == left =>
                {
                    Some(t.pos)
                }
                _ => None,
            })
        };
        let on_book =
            title(&output, "Book1", false).expect("titre du classeur") + egui::vec2(10.0, 30.0);
        run(&mut app, vec![egui::Event::PointerMoved(on_book)]);
        let mut output = run(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: on_book,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        // Seules les images que l'application réclame ont lieu, comme sous eframe.
        let mut extra = 0;
        while output.viewport_output[&egui::ViewportId::ROOT].repaint_delay
            == std::time::Duration::ZERO
            && extra < 10
        {
            output = run(&mut app, Vec::new());
            extra += 1;
        }
        assert_eq!(app.pm_selection, HashSet::from([book]));
        let row = title(&output, "Book1", true).expect("ligne du classeur").y;
        let highlighted: Vec<f32> = output
            .shapes
            .iter()
            .filter_map(|sh| match &sh.shape {
                egui::Shape::Rect(r) if r.fill == theme::list_row_selected_fill() => {
                    Some(r.rect.center().y)
                }
                _ => None,
            })
            .collect();
        assert!(
            matches!(highlighted.as_slice(), [y] if (y - row).abs() < 15.0),
            "seule la ligne du classeur est surlignée : {highlighted:?} / {row}"
        );
        session.shutdown().unwrap();
    }

    /// Un clic sur une ligne du Project Manager met à jour le surlignage de la
    /// ligne et le contour de la fenêtre dans l'image réclamée par le clic,
    /// sans attendre un mouvement de souris.
    #[test]
    fn un_clic_dans_le_project_manager_se_repeint_sans_bouger_la_souris() {
        use eframe::App as _;
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let book = window_stack::window_nodes(&state.project_tree())
            .into_iter()
            .find(|(_, kind)| *kind == stokesforge_app_state::state::WindowKind::Book)
            .map(|(id, _)| Tab::Book(id))
            .unwrap();
        let tab = Tab::Graph(graph);
        let mut app = StokesForgeApp::new(state);
        app.floating_tabs = vec![book, tab];
        app.window_positions.insert(tab, egui::pos2(0.0, 0.0));
        app.window_positions.insert(book, egui::pos2(640.0, 300.0));
        let ctx = egui::Context::default();
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let mut frame = eframe::Frame::_new_kittest();
        let mut now = 0.0;
        let mut run = |app: &mut StokesForgeApp, events: Vec<egui::Event>| {
            now += 0.05;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    time: Some(now),
                    events,
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut frame),
            );
            output.textures_delta.clear();
            output
        };
        for _ in 0..5 {
            run(&mut app, Vec::new());
        }
        let output = run(&mut app, Vec::new());
        let row = project_manager::tests::find_text_center(&output.shapes, "Book1");
        let button = |pressed| egui::Event::PointerButton {
            pos: row,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run(&mut app, vec![egui::Event::PointerMoved(row)]);
        run(&mut app, vec![button(true)]);
        let mut output = run(&mut app, vec![button(false)]);
        // Seules les images que l'application réclame ont lieu, comme sous eframe.
        let mut extra = 0;
        while output.viewport_output[&egui::ViewportId::ROOT].repaint_delay
            == std::time::Duration::ZERO
            && extra < 10
        {
            output = run(&mut app, Vec::new());
            extra += 1;
        }
        assert_eq!(app.floating_tabs.last(), Some(&book));
        assert_eq!(app.pm_selection, HashSet::from([book]));
        let highlighted: Vec<f32> = output
            .shapes
            .iter()
            .filter_map(|sh| match &sh.shape {
                egui::Shape::Rect(r) if r.fill == theme::list_row_selected_fill() => {
                    Some(r.rect.center().y)
                }
                _ => None,
            })
            .collect();
        assert!(
            matches!(highlighted.as_slice(), [y] if (y - row.y).abs() < 15.0),
            "la ligne du classeur est surlignée sans mouvement de souris : {highlighted:?} / {}",
            row.y
        );
        session.shutdown().unwrap();
    }

    /// Piège 1 de #115 : l'ordre des panneaux dans `ui()` fait la mise en
    /// page. Barre de menu en haut et barre d'état en bas, sur toute la
    /// largeur ; la console de script, posée avant le Project Manager, court
    /// sous lui sur toute la largeur ; Project Manager à gauche et Object
    /// Manager à droite s'arrêtent au-dessus d'elle.
    #[test]
    fn les_panneaux_fixes_se_posent_dans_l_ordre_de_ui() {
        use eframe::App as _;
        use egui::containers::panel::PanelState;
        let (session, _graph) = graphe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        app.script_collapsed = false;
        app.state.set_object_manager_open_for_test(true);
        let ctx = egui::Context::default();
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let mut frame = eframe::Frame::_new_kittest();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        for _ in 0..4 {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut frame),
            )
            .textures_delta
            .clear();
        }
        let rect = |id: &str| {
            PanelState::load(&ctx, egui::Id::new(id))
                .unwrap_or_else(|| panic!("panneau {id} absent"))
                .outer_rect
        };
        let near = |a: f32, b: f32| (a - b).abs() <= 1.0;
        let menu = rect("stokesforge_menu_bar");
        let status = rect("stokesforge_status");
        let script = rect("stokesforge_script");
        let pm = rect("stokesforge_pm");
        let om = rect("stokesforge_object_manager");
        for (name, panel) in [("menu", menu), ("état", status), ("console", script)] {
            assert!(
                near(panel.left(), screen.left()) && near(panel.right(), screen.right()),
                "{name} sur toute la largeur : {panel:?}"
            );
        }
        assert!(near(menu.top(), screen.top()));
        assert!(near(status.bottom(), screen.bottom()));
        assert!(
            script.bottom() <= status.top() + 1.0,
            "console sur la barre d'état"
        );
        assert!(
            near(pm.left(), screen.left()),
            "Project Manager à gauche : {pm:?}"
        );
        assert!(
            near(om.right(), screen.right()),
            "Object Manager à droite : {om:?}"
        );
        for (name, panel) in [("Project Manager", pm), ("Object Manager", om)] {
            assert!(
                panel.bottom() <= script.top() + 1.0,
                "{name} au-dessus de la console : {panel:?} / {script:?}"
            );
            assert!(panel.top() >= menu.bottom() - 1.0, "{name} sous le menu");
        }
        assert!(pm.right() <= om.left(), "{pm:?} puis {om:?}");
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

    /// L'arrêt du dispatch protège aussi les actions déjà collectées après Ctrl+N.
    #[test]
    fn la_confirmation_arrete_les_actions_suivantes_de_l_image() {
        let (session, _) = graphe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book = app.state.books()[0].0;
        let sheet = app.state.selected_sheet_in(book).unwrap();
        let column = app.state.visible_columns(sheet)[0].0;
        let context = egui::Context::default();
        let commit = || Action::CommitCell {
            column,
            row: 0,
            text: "42".into(),
        };
        let before = app.state.cell_text(column, 0);
        assert_ne!(before, "42", "témoin : la saisie change bien la valeur");
        app.dispatch_actions(
            &context,
            vec![
                Action::NewProject {
                    pending_edits: true,
                },
                commit(),
            ],
        );
        assert!(app.state.unsaved_pending().is_some());
        assert_eq!(app.state.cell_text(column, 0), before);

        // Témoin : une fois la confirmation annulée, la même saisie agit.
        app.dispatch_actions(
            &context,
            vec![Action::ConfirmUnsaved(UnsavedChoice::Cancel), commit()],
        );
        assert!(app.state.unsaved_pending().is_none());
        assert_eq!(app.state.cell_text(column, 0), "42");
        session.shutdown().unwrap();
    }

    /// #166 : un dépôt de lignes sur un graphe qui a déjà un X ouvre le dialogue
    /// bloquant d'axe — « Créer un axe » est le choix par défaut (Entrée),
    /// « Utiliser l'axe présent » le second bouton — et suspend les autres
    /// actions de l'image ; sans question, rien n'est peint.
    #[test]
    fn deposer_des_lignes_sur_un_graphe_avec_x_ouvre_le_dialogue_d_axe() {
        use super::test_support::painted;
        use eframe::App as _;
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book = app.state.books()[0].0;
        let sheet = app.state.selected_sheet_in(book).unwrap();
        let columns = app.state.visible_columns(sheet);
        app.state.dispatch(Action::SetColumnXPrime {
            column: columns[0].0,
            text: "3".into(),
        });
        let ctx = egui::Context::default();
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let mut frame = eframe::Frame::_new_kittest();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        let mut paint = |app: &mut StokesForgeApp, events: Vec<egui::Event>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut frame),
            );
            output.textures_delta.clear();
            output
        };
        let before = app.state.plot_spec(graph).unwrap();
        paint(&mut app, Vec::new());
        assert!(!painted(
            &paint(&mut app, Vec::new()),
            &tr!("row-drop-title")
        ));

        app.dispatch_actions(
            &ctx,
            vec![Action::AddRowsToGraph {
                graph,
                panel: 0,
                sheet,
                rows: vec![0],
                axis: RowAxis::Ask,
            }],
        );
        assert_eq!(app.state.pending_row_drop(), Some((graph, 1)));
        assert_eq!(
            app.state.plot_spec(graph).unwrap(),
            before,
            "rien avant la réponse"
        );
        paint(&mut app, Vec::new());
        let output = paint(&mut app, Vec::new());
        for (label, texte) in [
            ("titre", tr!("row-drop-title")),
            ("créer un axe", tr!("row-drop-new-axis")),
            ("axe présent", tr!("row-drop-present-axis")),
        ] {
            assert!(painted(&output, &texte), "{label} absent du dialogue");
        }

        // Entrée = bouton par défaut : « Créer un axe ».
        paint(
            &mut app,
            vec![super::test_support::touche(egui::Key::Enter)],
        );
        assert_eq!(app.state.pending_row_drop(), None);
        let spec = app.state.plot_spec(graph).unwrap();
        assert_eq!(spec.panels.len(), 2, "nouveau calque à axe X propre");
        assert_eq!(spec.panels[1].series[0].row, Some(0));
        session.shutdown().unwrap();
    }

    /// #326 : Add Folder ouvre le nom du dossier neuf en édition dans le
    /// panneau à l'écran (fixe ou détaché) ; un échec n'ouvre rien.
    #[test]
    fn add_folder_ouvre_le_nom_du_dossier_neuf_en_edition() {
        let (session, _) = graphe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        let root = app.state.tree().root();
        let fixe = egui::ViewportId::ROOT;
        let detache = project_manager::viewport_id();

        app.dispatch_actions(&context, vec![Action::AddFolder { parent: root }]);
        let folder = *app
            .state
            .tree()
            .node(root)
            .unwrap()
            .children()
            .last()
            .unwrap();
        assert_eq!(app.state.tree().node(folder).unwrap().name(), "Folder1");
        assert_eq!(project_manager::renaming_in(&context, fixe), vec![folder]);
        assert!(project_manager::renaming_in(&context, detache).is_empty());

        // Panneau détaché : c'est lui qui reçoit l'édition.
        app.pm_detached = true;
        app.dispatch_actions(&context, vec![Action::AddFolder { parent: folder }]);
        let inner = *app
            .state
            .tree()
            .node(folder)
            .unwrap()
            .children()
            .last()
            .unwrap();
        assert_eq!(project_manager::renaming_in(&context, detache), vec![inner]);
        assert_eq!(
            project_manager::renaming_in(&context, fixe),
            vec![folder],
            "le panneau fixe garde son édition d'avant"
        );

        // Témoin : un parent disparu n'ouvre aucune édition.
        let ghost = egui::Context::default();
        app.dispatch_actions(&ghost, vec![Action::DeleteObjects(vec![folder])]);
        app.dispatch_actions(&ghost, vec![Action::AddFolder { parent: folder }]);
        assert!(project_manager::renaming_in(&ghost, detache).is_empty());
        session.shutdown().unwrap();
    }

    #[test]
    fn supprimer_ferme_les_vues_et_annuler_ne_les_rouvre_pas() {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        let book = app.state.books()[0].0;
        app.open_tab(Tab::Graph(graph));
        app.detach_tab(Tab::Graph(graph));
        assert!(app.detached_tabs.contains_key(&Tab::Graph(graph)));
        app.dispatch_actions(&context, vec![Action::DeleteObjects(vec![graph])]);
        assert!(!app.detached_tabs.contains_key(&Tab::Graph(graph)));
        assert!(!app.state.tree().contains(graph));
        app.dispatch_actions(&context, vec![Action::Undo]);
        assert!(app.state.tree().contains(graph));
        assert!(!app.floating_tabs.contains(&Tab::Graph(graph)));
        assert!(!app.detached_tabs.contains_key(&Tab::Graph(graph)));
        app.open_tab(Tab::Book(book));
        assert!(app.floating_tabs.contains(&Tab::Book(book)), "témoin");
        // Une suppression annulable ne demande rien (S-F3, #229).
        app.dispatch_actions(&context, vec![Action::DeleteObjects(vec![book])]);
        assert!(!app.state.tree().contains(book));
        assert!(!app.floating_tabs.contains(&Tab::Book(book)));
        app.dispatch_actions(&context, vec![Action::Undo]);
        assert!(app.state.tree().contains(book));
        assert!(!app.floating_tabs.contains(&Tab::Book(book)));
    }

    /// #197 : A, [B C], D. Supprimer ou déplacer un groupe change le rang de
    /// plusieurs séries à la fois ; la sélection suit son contenu.
    fn graphe_a_groupe() -> (Session, NodeId) {
        let (session, graph) = graphe();
        let handle = session.handle();
        let snapshot = handle.snapshot().unwrap();
        let stokesforge_core::NodeKind::Graph { spec, .. } =
            snapshot.tree.node(graph).unwrap().kind()
        else {
            panic!("graphe attendu");
        };
        let mut spec = spec.clone();
        let panel = spec.panels.get_mut(0).unwrap();
        let model = panel.series[0].clone();
        panel.series = ["A", "B", "C", "D"]
            .iter()
            .enumerate()
            .map(|(rank, name)| {
                let mut s = model.clone();
                s.name = (*name).to_owned();
                s.group = (rank == 1 || rank == 2).then_some(1);
                s
            })
            .collect();
        handle
            .execute(stokesforge_commands::Command::SetPlotSpec { graph, spec })
            .unwrap();
        (session, graph)
    }

    fn nom_selectionne(app: &StokesForgeApp, graph: NodeId) -> Option<String> {
        let rank = app.selected_series(graph)?;
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph)?;
        Some(spec.panels[rank.panel].series[rank.index].name.clone())
    }

    fn selectionner(app: &mut StokesForgeApp, graph: NodeId, index: usize) {
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| {
            view.select_series(Some(stokesforge_plot::SeriesRef::new(0, index)), &spec);
        });
    }

    #[test]
    fn supprimer_un_groupe_fait_suivre_la_serie_selectionnee() {
        let context = egui::Context::default();
        let group = stokesforge_plot::SeriesGroupRef::new(0, 1);
        let remove = |graph| Action::RemovePlotObject {
            graph,
            target: stokesforge_app_state::object_manager::VisibilityTarget::SeriesGroup(group),
        };
        for (selected, expected) in [(3, Some("D")), (0, Some("A")), (1, None)] {
            let (session, graph) = graphe_a_groupe();
            let state = AppState::new(session.handle(), String::new()).unwrap();
            let mut app = StokesForgeApp::new(state);
            selectionner(&mut app, graph, selected);
            app.dispatch_actions(&context, vec![remove(graph)]);
            let spec =
                stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
            assert_eq!(spec.panels[0].series.len(), 2, "groupe et séries supprimés");
            assert_eq!(
                nom_selectionne(&app, graph).as_deref(),
                expected,
                "série {selected} sélectionnée avant la suppression"
            );
            session.shutdown().unwrap();
        }
    }

    #[test]
    fn deplacer_un_groupe_fait_suivre_la_serie_selectionnee() {
        let context = egui::Context::default();
        let (session, graph) = graphe_a_groupe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        selectionner(&mut app, graph, 2);
        app.dispatch_actions(
            &context,
            vec![Action::MoveSeriesGroup {
                graph,
                group: stokesforge_plot::SeriesGroupRef::new(0, 1),
                up: false,
            }],
        );
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        let names: Vec<&str> = spec.panels[0]
            .series
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(names, ["A", "D", "B", "C"], "témoin : le groupe a bougé");
        assert_eq!(nom_selectionne(&app, graph).as_deref(), Some("C"));
        session.shutdown().unwrap();
    }

    #[test]
    fn dupliquer_un_classeur_conserve_son_etat_ouvert_ou_ferme() {
        let (session, _) = graphe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        let book = app.state.books()[0].0;
        app.close_floating_tabs(vec![Tab::Book(book)]);
        app.dispatch_actions(&context, vec![Action::DuplicateObjects(vec![book])]);
        let closed = app
            .state
            .books()
            .into_iter()
            .find(|(_, name)| name == "Book1 copy 1")
            .unwrap()
            .0;
        assert!(!app.floating_tabs.contains(&Tab::Book(closed)));
        app.open_tab(Tab::Book(book));
        app.dispatch_actions(&context, vec![Action::DuplicateObjects(vec![book])]);
        let opened = app
            .state
            .books()
            .into_iter()
            .find(|(_, name)| name == "Book1 copy 2")
            .unwrap()
            .0;
        assert!(app.floating_tabs.contains(&Tab::Book(opened)));
    }

    /// S-F3 (#229) : fermer la fenêtre (croix, Alt+F4, File › Quit) est retenu
    /// et devient `Action::Quit` ; avec du travail non enregistré, la question
    /// « Discard Changes · Cancel · Save » s'ouvre et la fenêtre reste ;
    /// Discard la libère, et la demande suivante n'est plus retenue.
    #[test]
    fn fermer_la_fenetre_demande_avant_de_perdre_le_travail() {
        use eframe::App as _;
        let (session, _) = graphe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        theme::install(&context);
        egui_extras::install_image_loaders(&context);
        let mut frame = eframe::Frame::_new_kittest();
        let mut image = |app: &mut StokesForgeApp, close: bool| {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 700.0),
                )),
                ..Default::default()
            };
            if close {
                input
                    .viewports
                    .entry(egui::ViewportId::ROOT)
                    .or_default()
                    .events
                    .push(egui::ViewportEvent::Close);
            }
            let mut output = context.run_ui(input, |ui| {
                app.logic(ui.ctx(), &mut frame);
                app.ui(ui, &mut frame);
            });
            output.textures_delta.clear();
            output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .map(|viewport| viewport.commands.clone())
                .unwrap_or_default()
        };
        let commandes = image(&mut app, true);
        assert!(commandes.contains(&egui::ViewportCommand::CancelClose));
        assert!(!commandes.contains(&egui::ViewportCommand::Close));
        assert_eq!(
            app.state.unsaved_pending(),
            Some(&stokesforge_app_state::state::UnsavedGesture::Quit)
        );
        app.dispatch_actions(
            &context,
            vec![Action::ConfirmUnsaved(UnsavedChoice::Discard)],
        );
        let commandes = image(&mut app, false);
        assert!(
            commandes.contains(&egui::ViewportCommand::Close),
            "Discard : la fenêtre se ferme"
        );
        let commandes = image(&mut app, true);
        assert!(
            !commandes.contains(&egui::ViewportCommand::CancelClose),
            "la fermeture libérée n'est plus retenue"
        );
        session.shutdown().unwrap();
    }

    #[test]
    fn la_confirmation_peinte_vide_les_actions_de_l_image() {
        let (session, _) = graphe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        theme::install(&context);
        egui_extras::install_image_loaders(&context);
        let mut actions = vec![Action::AddGraph];
        context
            .run_ui(egui::RawInput::default(), |ui| {
                app.blocking_confirmations(ui.ctx(), &mut actions);
            })
            .textures_delta
            .clear();
        assert_eq!(
            actions.len(),
            1,
            "sans confirmation, l'action reste collectée"
        );
        app.state.dispatch(Action::NewProject {
            pending_edits: true,
        });
        context
            .run_ui(egui::RawInput::default(), |ui| {
                app.blocking_confirmations(ui.ctx(), &mut actions);
            })
            .textures_delta
            .clear();
        assert!(
            actions.is_empty(),
            "seul le dialogue peut agir dans cette image"
        );
        session.shutdown().unwrap();
    }

    #[test]
    fn le_dispatch_n_ouvre_un_graphe_que_si_la_commande_en_cree_un() {
        let (session, _) = graphe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let book = app.state.books()[0].0;
        let sheet = app.state.selected_sheet_in(book).unwrap();
        app.floating_tabs.clear();
        let context = egui::Context::default();
        let before = app.state.revision();
        app.dispatch_actions(
            &context,
            vec![Action::PlotColumns {
                sheet,
                columns: Vec::new(),
                plot_type: stokesforge_plot::PlotType::Line,
            }],
        );
        assert_eq!(app.state.revision(), before);
        assert!(
            app.floating_tabs.is_empty(),
            "pas de réouverture du dernier enfant"
        );

        app.dispatch_actions(&context, vec![Action::AddGraph]);
        assert!(app.state.revision() > before, "témoin : création effective");
        let graph = *app
            .state
            .tree()
            .node(app.state.tree().root())
            .unwrap()
            .children()
            .last()
            .unwrap();
        assert_eq!(app.floating_tabs, vec![Tab::Graph(graph)]);
        session.shutdown().unwrap();
    }

    #[test]
    fn ctrl_n_confirme_et_oublie_les_vues_du_projet_precedent() {
        use eframe::App as _;
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        // Le classeur détaché, le graphe flottant : les deux sortes de vue
        // (fenêtre OS, `GraphView` de la racine) existent avant Ctrl+N.
        let book = app.state.books()[0].0;
        app.detach_tab(Tab::Book(book));
        let context = egui::Context::default();
        set_header_rows(
            &context,
            book,
            HeaderRows {
                metadata: false,
                x_prime: false,
            },
        );
        context.set_embed_viewports(false);
        theme::install(&context);
        egui_extras::install_image_loaders(&context);
        let mut frame = eframe::Frame::_new_kittest();
        let mut paint = |app: &mut StokesForgeApp, events| {
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
            output
        };
        paint(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::N,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
        );
        // Le dialogue est maintenant peint (visible à la seconde image) : on
        // y cherche Discard Changes.
        paint(&mut app, Vec::new());
        let output = paint(&mut app, Vec::new());
        let discard = super::test_support::textes(&output.shapes)
            .into_iter()
            .find(|texte| texte.texte == tr!("unsaved-discard"))
            .expect("Discard Changes peint")
            .centre;
        assert_eq!(
            app.state.unsaved_pending(),
            Some(&stokesforge_app_state::state::UnsavedGesture::NewProject)
        );
        assert!(app.state.tree().node(graph).is_some());
        // Témoins : sans eux, les assertions de vide plus bas passeraient
        // aussi sur des tables qui n'ont jamais rien contenu.
        assert!(
            app.graph_views.contains_key(&graph),
            "vue du graphe flottant"
        );
        assert!(app.floating_rects.contains_key(&graph));
        assert!(app.floating_tabs.contains(&Tab::Graph(graph)));
        assert!(app.detached_tabs.contains_key(&Tab::Book(book)));
        paint(&mut app, vec![egui::Event::PointerMoved(discard)]);
        let bouton = |pressed| egui::Event::PointerButton {
            pos: discard,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        paint(&mut app, vec![bouton(true)]);
        let output = paint(&mut app, vec![bouton(false)]);
        assert_eq!(app.state.tree().len(), 1);
        assert!(app.floating_tabs.is_empty());
        assert!(app.detached_tabs.is_empty());
        assert!(app.graph_views.is_empty());
        assert!(app.floating_rects.is_empty());
        assert!(!header_rows(&context, Some(book)).metadata);
        assert!(
            output.viewport_output[&egui::ViewportId::from_hash_of(Tab::Book(book))]
                .commands
                .contains(&egui::ViewportCommand::Close)
        );
        session.shutdown().unwrap();
    }

    /// Issue #16 : après enregistrement et rechargement, les fenêtres
    /// reviennent dans leur état — maximisée, repliée, fermée, et dans leur
    /// ordre d'empilement.
    #[test]
    fn l_etat_des_fenetres_survit_a_l_enregistrement_et_au_rechargement() {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
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
        let path = directory.path().join("fenetres.stf");
        session.handle().save(&path).unwrap();

        let reloaded = Session::spawn(Project::new("Vide").unwrap(), None).unwrap();
        reloaded.handle().load(&path).unwrap();
        let state = AppState::new(reloaded.handle(), "test.stf".to_string()).unwrap();
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

    /// Ctrl+" ouvre l'assistant d'import, sur un clavier AZERTY (touche du 3
    /// sans Maj) comme QWERTY (Maj+apostrophe), et pas pendant qu'il est déjà
    /// ouvert.
    #[test]
    fn ctrl_guillemet_ouvre_l_assistant_d_import() {
        let (session, _graph) = graphe();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
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
            // Témoin : l'assistant déjà ouvert n'est pas rouvert à neuf par
            // un second Ctrl+" — la liste de fichiers en cours survit.
            let fichier = std::path::PathBuf::from("en-cours.csv");
            app.state
                .import_state_mut()
                .unwrap()
                .paths
                .push(fichier.clone());
            let mut output = context.run_ui(press(key, modifiers), |ui| {
                app.collect_shortcuts(ui.ctx(), &mut actions);
            });
            output.textures_delta.clear();
            assert_eq!(
                app.state.import_state().map(|import| import.paths.clone()),
                Some(vec![fichier]),
                "{key:?} ne doit pas rouvrir l'assistant déjà ouvert"
            );
            app.state.cancel_import();
        }
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
        let state = AppState::new(session.handle(), "mesure.stf".to_string()).unwrap();
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

    #[test]
    fn les_donnees_embarquees_restent_fermees_au_demarrage_et_apres_undo() {
        let mut project = stokesforge_core::Project::new("Test").unwrap();
        let root = project.root();
        let book = project.add_workbook(root, None).unwrap();
        project
            .set_window_meta(
                book,
                stokesforge_core::WindowMeta {
                    hidden: true,
                    ..Default::default()
                },
            )
            .unwrap();
        let sheet = project.tree().children(book).unwrap()[0];
        let column = project
            .add_column(sheet, None, None, stokesforge_core::Column::from_f64(&[1.]))
            .unwrap();
        let mut panel = stokesforge_plot::PanelSpec::default();
        panel.series.push(stokesforge_plot::SeriesSpec::new(column));
        let graph = project
            .add_graph(
                root,
                None,
                stokesforge_plot::PlotSpec::default()
                    .with_panels(stokesforge_plot::Panels::one(panel)),
            )
            .unwrap();
        let session = stokesforge_session::Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        assert_eq!(app.floating_tabs, vec![Tab::Graph(graph)]);
        let ctx = egui::Context::default();
        app.dispatch_action(&ctx, Action::ShowGraphData { graph });
        assert!(app.floating_tabs.contains(&Tab::Book(book)));
        app.dispatch_action(&ctx, Action::Undo);
        assert!(!app.floating_tabs.contains(&Tab::Book(book)));
        session.shutdown().unwrap();
    }
}
