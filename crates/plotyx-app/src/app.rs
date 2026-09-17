//! La glu `eframe` : traduit clavier et souris en [`Action`] et peint
//! l'[`AppState`]. Aucune logique de modèle ne vit ici — tout ce qui décide
//! est dans [`crate::state`], qui se teste sans fenêtre. Ce module choisit
//! en revanche la **disposition** (docking `egui_dock`) et le **cadre**
//! (menus, raccourcis, sélecteur de fichiers `rfd`) qui l'entourent — des
//! décisions de présentation, pas de modèle, donc légitimement hors de
//! `state`.
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
//! Le panneau qui liste l'arbre du projet n'est **plus un onglet** du dock
//! depuis cette tâche : c'est un [`egui::Panel::left`] fixe, peint par
//! [`PlotyxApp::project_manager_panel`], toujours à gauche, jamais recouvert
//! par ni ne recouvrant la [`egui_dock::DockArea`] — garanti par
//! construction en le peignant *avant* elle dans [`PlotyxApp::ui`], comme la
//! barre de menu (`egui::Panel::top`). Il reste redimensionnable en largeur
//! par glisser du bord (comportement par défaut d'`egui::Panel::left`), mais
//! **figé en position** : pas de bascule vers la droite, pas de
//! repositionnement par glisser — voir § Attacher à gauche ou à droite
//! ci-dessous pour pourquoi, et pour ce qui reste volontairement hors
//! périmètre. Le contenu lui-même ([`project_manager_contents`], l'arbre
//! cliquable du projet) n'a pas changé : seule sa charpente de disposition a
//! bougé, du dock vers un panneau.
//!
//! # Console de script, et l'ordre des panneaux
//!
//! La console de script (édition de [`AppState::script_source`], boutons
//! Exécuter/Arrêter, sortie) suit depuis cette tâche le même patron que le
//! Project Manager : ce n'est plus un onglet **structurel** du
//! [`egui_dock::DockState`], mais un [`egui::Panel::bottom`] fixe, peint par
//! [`PlotyxApp::script_panel`]. Son contenu ([`script_contents`]) n'a pas
//! changé : seule sa charpente de disposition a bougé, du dock vers un
//! panneau — même mouvement que pour le Project Manager, appliqué au bord du
//! bas plutôt qu'à celui de gauche.
//!
//! Point d'ordre important avec `egui::Panel` : **le premier panneau ajouté
//! est le plus extérieur**, chaque panneau suivant est inséré à l'intérieur
//! de ce que les précédents laissent. Pour que la console de script coure sur
//! toute la largeur de la fenêtre — y compris sous le Project Manager, pas
//! seulement sous la zone de la `DockArea` — [`PlotyxApp::script_panel`] doit
//! donc être appelé *avant* [`PlotyxApp::project_manager_panel`] dans
//! [`PlotyxApp::ui`]. L'ordre d'appel complet y est : barre de menu
//! (`egui::Panel::top`) → console de script (`egui::Panel::bottom`) → Project
//! Manager (`egui::Panel::left`) → `DockArea`/`CentralPanel` en dernier ; un
//! panneau posé plus tôt réserve sa bande sur toute la largeur/hauteur
//! restante, donc la console de script, postée avant le Project Manager,
//! s'étend bien sous lui plutôt que d'être resserrée entre lui et le bord
//! droit.
//!
//! `egui::Panel::bottom` (comme `Panel::top`) **n'est pas redimensionnable
//! par défaut**, à la différence de `left`/`right` — contrairement au Project
//! Manager, [`PlotyxApp::script_panel`] appelle donc explicitement
//! `.resizable(true)` pour retrouver le glisser du bord en hauteur qu'avait
//! la console dans le dock.
//!
//! Comme pour le Project Manager, aucune bascule de position n'existe ici (il
//! n'y en a jamais eu à basculer : la console a toujours été en bas), et le
//! panneau ne montre aucun mécanisme d'onglet — pas de `TabViewer`, pas de
//! tab strip.
//!
//! Les quatre natures de fenêtre que le Project Manager ouvre — [`Tab::Book`],
//! [`Tab::Graph`], [`Tab::Matrix`], [`Tab::Note`] — restent, elles, des
//! onglets du dock. La grille unique d'avant une tâche antérieure
//! (`Tab::Grid`, une seule feuille affichée à la fois pour tout le projet) a
//! disparu : chaque classeur ouvert a désormais son propre onglet
//! [`Tab::Book`], avec sa propre sous-navigation interne entre ses feuilles
//! ([`AppState::worksheets_of`], [`AppState::selected_sheet_in`]) — un
//! classeur peut avoir plusieurs feuilles, la grille d'un onglet ne montre
//! jamais que celle actuellement choisie *dans ce classeur*. [`Tab::Matrix`]
//! et [`Tab::Note`] existent aussi : la matrice est un **espace réservé
//! documenté** (édition hors périmètre, voir [`matrix_contents`]), la note
//! est réellement éditable (`plotyx-core` expose déjà son texte).
//!
//! [`PlotyxApp::open_tab`] est le seul endroit qui décide « ouvrir » vs
//! « focaliser » un onglet, quelle que soit sa nature — le même geste que
//! l'ancien `open_graph_tab`, généralisé.
//!
//! # Disposition dockable
//!
//! Les quatre natures de fenêtre ci-dessus sont des onglets d'un
//! [`egui_dock::DockState`] peint par une [`egui_dock::DockArea`] — ni le
//! Project Manager ni la console de script n'y participent plus, voir les
//! deux sections précédentes. C'est la charpente de disposition décidée par
//! `docs/archive/rustrewrite/roadmap.md` §7 pour les contenus de fenêtre ;
//! elle n'invente ni panneau Properties ni grille à délégués (hors
//! périmètre, voir `docs/application.md`).
//!
//! # Maximiser un onglet
//!
//! `egui_dock` 0.21 n'expose aucune fonction « maximiser » native — pas de
//! méthode qui redimensionnerait un nœud du `DockState` pour qu'il occupe
//! tout l'espace sans toucher à la disposition sous-jacente. Plutôt que de
//! tenter de faire mentir le `DockState` lui-même (un nœud qui « occuperait »
//! tout l'espace resterait un nœud parmi d'autres dans l'arbre de
//! disposition, et il faudrait défaire ce mensonge à la restauration sans
//! perdre l'agencement exact d'avant), [`PlotyxApp::maximized_tab`] est un
//! état **à côté** du `DockState`, qui ne le touche jamais : quand il vaut
//! `Some`, [`PlotyxApp::ui`] ne peint pas du tout la `DockArea` cette
//! image-là — à la place, une zone séparée montre uniquement l'onglet
//! maximisé, plein cadre. Restaurer (`maximized_tab = None`) revient à
//! peindre la `DockArea` normalement : la disposition qu'elle retient n'a
//! jamais bougé pendant la maximisation, donc « restaurer » est exactement
//! « la disposition d'avant », pas un recalcul.
//!
//! Le Project Manager n'a plus de valeur dans `maximized_tab` depuis cette
//! tâche : ce n'est plus un onglet, et un panneau fixe toujours entièrement
//! visible n'a de toute façon rien à gagner à être « maximisé ». Il reste
//! visible sans rien de spécial pendant qu'un *autre* onglet est maximisé,
//! puisque [`PlotyxApp::project_manager_panel`] est peint avant la zone de
//! maximisation comme avant la `DockArea` — un panneau fixe séparé, jamais
//! dédoublé, n'a besoin d'aucun cas particulier ici.
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
//! # Détachement du Project Manager
//!
//! Le flottant `egui_dock` (`Surface::Window`) qu'un onglet pouvait obtenir
//! en le glissant hors du dock n'existe plus pour le Project Manager,
//! puisqu'il n'est plus un onglet — seul reste :
//!
//! **Vraie fenêtre OS séparée** ([`PlotyxApp::pm_detached`],
//! [`PlotyxApp::show_detached_project_manager`]) : `egui::Context::
//! show_viewport_deferred` ouvre un vrai viewport natif, déplaçable sur
//! un autre écran. Quand ce mode est actif, [`PlotyxApp::project_manager_panel`]
//! n'est simplement pas peint cette image-là (au lieu d'un `remove_tab` sur
//! un `DockState` qui ne le porte de toute façon plus) ; le fermer (bouton
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
//! doc de tête de [`PlotyxApp::show_detached_project_manager`].
//!
//! # Détachement générique des onglets de contenu
//!
//! [`PlotyxApp::detach_tab`]/[`PlotyxApp::reattach_tab`]/
//! [`PlotyxApp::show_detached_tabs`] généralisent le mécanisme ci-dessus à
//! [`Tab::Book`], [`Tab::Graph`], [`Tab::Matrix`] et [`Tab::Note`] — un clic
//! droit sur l'onglet (menu contextuel `egui_dock::TabViewer::context_menu`,
//! seul point d'extension de ce genre en 0.21.1 — vérifié dans
//! `egui_dock-0.21.1/src/widgets/tab_viewer.rs`) propose « Détacher dans une
//! fenêtre », symétrique de « Réattacher au dock principal » côté fenêtre
//! détachée. Plusieurs onglets peuvent être détachés en même temps :
//! [`PlotyxApp::detached_tabs`] est une table par [`Tab`] (chaque variante
//! porte déjà un `NodeId`, donc un `ViewportId::from_hash_of(tab)` est
//! unique par construction — pas de collision possible entre deux classeurs
//! détachés simultanément, par exemple).
//!
//! # Pourquoi pas un `ViewModel` par nature de contenu comme pour le PM
//!
//! Le PM n'a qu'une seule forme de contenu (un arbre en lecture seule) ; les
//! quatre natures de fenêtre ici sont franchement différentes (grille
//! éditable virtualisée, graphe interactif avec cache de formes, texte
//! éditable, espace réservé). Plutôt que quatre `ViewModel`/`Requests`
//! disjoints à faire cohabiter dans [`PlotyxApp::detached_tabs`],
//! [`DetachedTabViewModel`] est une seule structure portant un champ par
//! nature de contenu (`selected_sheet`/`cell_edit` pour un classeur,
//! `note_edit` pour une note, `graph_view` pour un graphe) — les champs
//! inutilisés pour la nature du `Tab` qu'elle sert restent simplement à leur
//! valeur par défaut. Moins de types, un seul `Arc<Mutex<_>>` par onglet
//! détaché à faire voyager dans la fermeture, au prix d'un typage un peu
//! large ; le commentaire de tête de chaque champ dit à qui il appartient.
//!
//! # La lecture du modèle est factorisée, l'édition transitoire ne l'est pas
//!
//! [`tab_contents`] (la fonction déjà partagée par le dock normal et la zone
//! de maximisation) prend `&mut AppState` — un onglet détaché ne peut
//! pourtant pas capturer `&mut AppState` dans la fermeture `'static`,
//! `Send + Sync`, exigée par `show_viewport_deferred`, exactement le
//! problème déjà résolu pour le Project Manager (voir plus haut) : mettre
//! `AppState` entière derrière un verrou déborderait le problème à
//! résoudre, pour un onglet comme pour l'autre.
//!
//! Le contournement retenu : [`DetachedTabViewModel::snapshot`], un clone de
//! [`plotyx_session::Snapshot`] (bon marché — deux `Arc`, voir sa doc), est
//! resynchronisé depuis `self.state` à chaque image, **avant** l'appel à
//! `show_viewport_deferred` — même schéma que `ProjectManagerViewModel::tree`.
//! Toute lecture pure (texte de cellule, colonnes visibles, feuilles d'un
//! classeur, texte d'une note) passe par les fonctions libres de
//! `crate::state` factorisées sur un `&Tree` pour l'occasion
//! (`cell_text_in`, `visible_columns_in`, `row_count_in`, `worksheets_of_in`,
//! `note_text_in`, `node_title_in`) — le corps exact de la méthode
//! `AppState` correspondante, pas une réécriture : le dock et une fenêtre
//! détachée affichent donc littéralement la même règle, jamais deux versions
//! à tenir en accord.
//!
//! Ce qui n'est en revanche **pas** partagé avec `AppState`, délibérément :
//! le tampon de saisie transitoire d'une cellule ou d'une note
//! ([`AppState::editing`]/[`AppState::note_editing`]) reste un **verrou
//! unique par fenêtre**, pas un verrou unique pour toute l'application. La
//! limite documentée en tête de crate (« une seule cellule éditable à la
//! fois, toutes grilles confondues ») devient, pour un onglet détaché, « une
//! seule cellule éditable à la fois **dans cette fenêtre détachée** » — une
//! fenêtre OS séparée ne peut de toute façon pas partager le verrou
//! `AppState` de la fenêtre mère sans le mettre lui-même derrière un
//! `Arc<Mutex<_>>`, ce que ce module refuse déjà pour le PM et pour la même
//! raison. `detached_cell_widget`/`detached_note_contents` réimplémentent
//! donc la même petite machine à états (`gained_focus`/`lost_focus`/Échap)
//! que [`cell_widget`]/[`note_contents`], sur `DetachedTabViewModel::
//! cell_edit`/`note_edit` plutôt que sur le champ d'`AppState` — quelques
//! lignes dupliquées, documentées ici plutôt qu'une abstraction
//! supplémentaire (trait, générique) pour un couplage aussi mince. Valider
//! une saisie produit la **même** [`Action::CommitCell`]/[`Action::
//! SetNoteText`] que le dock, appliquée par [`AppState::dispatch`] : le
//! chemin de commande annulable, lui, ne connaît aucune différence entre un
//! onglet docké et un onglet détaché.
//!
//! [`Tab::Graph`] échappe à cette limite : [`plotyx_render_egui::GraphView`]
//! n'a pas de verrou global à `AppState` à respecter (son cache de formes et
//! sa navigation zoom/pan sont déjà scopés par graphe, dans
//! [`PlotyxApp::graph_views`]) — [`graph_view_contents`] est donc
//! **littéralement** la même fonction pour le dock et pour une fenêtre
//! détachée, la vue elle-même migrant simplement hors de `graph_views` le
//! temps du détachement ([`PlotyxApp::detach_tab`]/[`PlotyxApp::
//! reattach_tab`]) pour rester un `Arc<Mutex<_>>` capturable par la
//! fermeture sans dupliquer le cache.
//!
//! [`Tab::Matrix`] n'a aucun état à faire voyager (espace réservé, voir
//! [`matrix_contents`]) : [`detached_matrix_contents`] est un simple appel à
//! [`node_title_in`].
//!
//! La console de script n'entre pour rien dans ce mécanisme depuis cette
//! tâche : ce n'est plus un onglet du dock (voir § Console de script
//! ci-dessus), donc rien à détacher ici — `TabViewer::context_menu` ne
//! propose « Détacher dans une fenêtre » que pour `Book`/`Graph`/`Matrix`/
//! `Note`, seules natures encore portées par le `DockState`.
//!
//! # Sélecteur de fichiers natif
//!
//! [`tool_bar`](PlotyxApp::tool_bar) n'a plus de champ de texte pour le
//! chemin : les boutons « Enregistrer sous… » et « Ouvrir… » ouvrent un
//! `rfd::FileDialog` natif (modal, comme tout sélecteur de fichiers de
//! bureau — bloquer le rendu le temps que l'utilisateur choisisse un fichier
//! est le comportement attendu, à ne pas confondre avec le blocage qu'évite
//! `RunScript`, qui lui n'a pas de raison de durer). Ctrl+S et Ctrl+O
//! (et les entrées de menu équivalentes) continuent d'agir directement sur
//! le chemin déjà connu, sans dialogue : c'est le geste « enregistrer »/
//! « recharger » rapide d'un tableur, distinct de « enregistrer sous ».
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
//!   `PlotyxApp::new` la reconstruit à chaque démarrage.
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

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use egui_dock::{DockArea, DockState, NodePath, Style, TabViewer};
use egui_extras::{Column as TableColumn, TableBuilder};
use plotyx_core::{ColumnType, NodeId, Tree};
use plotyx_render_egui::{DrawnPlot, GraphView, DEFAULT_FIGURE_SIZE};
use plotyx_session::Snapshot;

use crate::selection::Selection;
use crate::state::{
    cell_text_in, node_title_in, note_text_in, row_count_in, visible_columns_in, worksheets_of_in,
    Action, AppState, CellEdit, HeaderField, MoveDirection, NoteEdit, ProjectTreeNode, WindowKind,
};
use plotyx_import::{DecimalStyle, HeaderSkip, NameSource, Separator};

/// Variable d'environnement du mécanisme d'auto-fermeture (voir
/// [`PlotyxApp::new`]).
pub const AUTOQUIT_ENV: &str = "PLOTYX_APP_AUTOQUIT_MS";
/// Variable d'environnement du mécanisme de capture (voir
/// [`PlotyxApp::screenshot`]).
pub const SCREENSHOT_ENV: &str = "PLOTYX_APP_SCREENSHOT";
/// Images laissées passer avant de demander une capture.
///
/// egui résout la taille de ses panneaux sur deux ou trois images : capturer
/// la première montrerait une mise en page transitoire — et, pour un onglet
/// de graphe, un compteur de cache qui n'a pas encore eu l'occasion de
/// servir une image inchangée. On regarde le régime établi, pas le
/// démarrage.
const SCREENSHOT_SETTLE_FRAMES: u32 = 8;

/// Ctrl+Z — annuler.
pub const SHORTCUT_UNDO: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
/// Ctrl+Y — rétablir (convention Windows).
pub const SHORTCUT_REDO_Y: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Y);
/// Ctrl+Maj+Z — rétablir (convention Unix / macOS). Les deux sont acceptées.
pub const SHORTCUT_REDO_Z: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::Z,
);
/// Ctrl+S — enregistrer (dans le fichier déjà connu, sans dialogue).
pub const SHORTCUT_SAVE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
/// Ctrl+O — ouvrir (recharger depuis le chemin déjà connu, sans dialogue).
pub const SHORTCUT_OPEN: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O);
/// Ctrl+Entrée — exécuter le script de la console.
pub const SHORTCUT_RUN_SCRIPT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Enter);

/// Hauteur d'une ligne de la grille, en points.
const ROW_HEIGHT: f32 = 20.0;
/// Hauteur de la ligne d'en-tête, en points — fenêtres détachées et grille
/// docquée d'avant l'en-tête empilé (voir [`HEADER_HEIGHT_STACKED`] pour
/// cette dernière).
const HEADER_HEIGHT: f32 = 22.0;
/// Largeur par défaut d'une colonne de données dans la grille docquée. 54px
/// (plutôt que les 110px historiques) : demande explicite, pour loger
/// davantage de colonnes à l'écran — l'utilisateur peut toujours
/// redimensionner à la souris (`.resizable(true)`).
const DEFAULT_COLUMN_WIDTH: f32 = 54.0;
/// Largeur minimale d'une colonne de données après redimensionnement manuel.
/// Sous l'ancien plancher (56px) pour ne pas empêcher un retour à
/// [`DEFAULT_COLUMN_WIDTH`] une fois qu'on l'a élargie puis rétrécie.
const MIN_COLUMN_WIDTH: f32 = 28.0;
/// Largeur par défaut de la gouttière des numéros de ligne dans la grille
/// docquée — élargie (l'ancienne valeur, 56px, coupait déjà les libellés
/// d'en-tête « Long Name »/« Comments ») et redimensionnable comme les
/// colonnes de données plutôt que figée (`TableColumn::exact`).
const ROW_NUMBER_COLUMN_WIDTH: f32 = 90.0;
/// Largeur minimale de la gouttière des numéros de ligne après
/// redimensionnement manuel — assez pour garder `#` et quelques chiffres
/// lisibles.
const MIN_ROW_NUMBER_COLUMN_WIDTH: f32 = 32.0;
/// Hauteur d'une rangée secondaire de l'en-tête empilé (lettre, unité,
/// commentaire, formule).
const HEADER_SUBROW_HEIGHT: f32 = 18.0;
/// Hauteur de la rangée du nom long, légèrement plus haute — c'est le nom
/// principal de la colonne.
const HEADER_LONGNAME_HEIGHT: f32 = 20.0;
/// Hauteur totale de l'en-tête empilé de la grille docquée (lettre, nom
/// long, unité, commentaire, formule).
const HEADER_HEIGHT_STACKED: f32 = HEADER_LONGNAME_HEIGHT + 4.0 * HEADER_SUBROW_HEIGHT;
/// Hauteur de la bande d'onglets de sheet, sous la grille docquée.
const SHEET_TABS_HEIGHT: f32 = 24.0;
/// Hauteur de la zone de sortie du script, en points.
const SCRIPT_OUTPUT_HEIGHT: f32 = 140.0;
/// Hauteur par défaut du panneau fixe de la console de script — point de
/// départ seulement, l'utilisateur peut ensuite le redimensionner en
/// glissant son bord (`.resizable(true)`, voir [`PlotyxApp::script_panel`]).
const SCRIPT_PANEL_HEIGHT: f32 = 220.0;
/// Largeur par défaut du panneau Project Manager fixe, et de la fenêtre OS
/// séparée quand il est détaché.
const PROJECT_MANAGER_WIDTH: f32 = 260.0;

/// Un onglet du plan de disposition.
///
/// Ni le Project Manager ni la console de script n'en sont plus depuis les
/// deux tâches successives qui les ont sortis du dock — voir la doc de tête
/// du module, §§ Project Manager et Console de script : ce sont deux
/// `egui::Panel` fixes, peints respectivement par
/// [`PlotyxApp::project_manager_panel`] et [`PlotyxApp::script_panel`].
/// [`Tab::Book`], [`Tab::Graph`], [`Tab::Matrix`], [`Tab::Note`] portent un
/// [`NodeId`] : une instance par fenêtre **ouverte**, focalisée plutôt que
/// dupliquée si on la rouvre — voir [`PlotyxApp::open_tab`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Tab {
    /// Un classeur ouvert, identifié par son nœud. Contient la grille,
    /// scopée à la feuille actuellement choisie *dans ce classeur*.
    Book(NodeId),
    /// Un graphe ouvert, identifié par son nœud.
    Graph(NodeId),
    /// Une matrice ouverte, identifiée par son nœud. Contenu **espace
    /// réservé** — voir [`matrix_contents`].
    Matrix(NodeId),
    /// Une note ouverte, identifiée par son nœud.
    Note(NodeId),
}

/// L'application `eframe` : l'état du banc, la disposition dockable, et le
/// strict nécessaire au rendu.
pub struct PlotyxApp {
    /// L'état, seul dépositaire de la logique.
    state: AppState,
    /// Le plan de disposition : quels onglets, où, dans quel agencement de
    /// panneaux amarrables. Persistant d'une image à l'autre — c'est ce qui
    /// retient les glissers-déposés de l'utilisateur. Ne porte plus ni le
    /// Project Manager ni la console de script (voir la doc de tête du
    /// module, §§ Project Manager et Console de script) : uniquement les
    /// quatre natures de fenêtre de contenu.
    dock_state: DockState<Tab>,
    /// Une vue de graphe (et son cache de formes, phase R4) par graphe
    /// **ouvert** — c'est-à-dire ayant eu un onglet au moins une fois,
    /// l'entrée survit à la fermeture de l'onglet. Un cache par vue est la
    /// règle (`plotyx_render_egui::GraphView`) ; avec plusieurs graphes
    /// affichables simultanément, une seule vue ne suffit plus.
    graph_views: HashMap<NodeId, GraphView>,
    /// Onglet actuellement maximisé, s'il y en a un — voir la doc de tête du
    /// module, § maximiser. Ne peut plus valoir le Project Manager depuis
    /// cette tâche : ce n'est plus un onglet, et un panneau fixe toujours
    /// entièrement visible n'a rien à gagner à être « maximisé ».
    maximized_tab: Option<Tab>,
    /// Vrai quand le Project Manager est détaché en fenêtre OS séparée (voir
    /// la doc de tête du module, § détachement). Dans cet état,
    /// [`PlotyxApp::project_manager_panel`] n'est simplement pas peint.
    pm_detached: bool,
    /// Ce que la fenêtre OS détachée du Project Manager peint, resynchronisé
    /// depuis `self.state` à chaque image par
    /// [`PlotyxApp::show_detached_project_manager`] — voir sa doc de tête.
    /// `Arc<Mutex<_>>` uniquement parce que le callback `show_viewport_deferred`
    /// l'exige (`Fn` + `Send + Sync + 'static`, pas de capture de `&mut
    /// self`) ; ce n'est **pas** `AppState` elle-même derrière un verrou,
    /// seulement ce strict sous-ensemble.
    pm_view_model: Arc<Mutex<ProjectManagerViewModel>>,
    /// Ce que la fenêtre OS détachée du Project Manager écrit (clics
    /// d'ouverture, demande de réattachement), drainé chaque image par
    /// [`PlotyxApp::ui`] — même rôle que `open_requests`/`actions` pour le
    /// reste du banc, juste posé par un callback différé plutôt que collecté
    /// en direct.
    pm_requests: Arc<Mutex<DetachedProjectManagerRequests>>,
    /// Onglets de contenu (`Book`/`Graph`/`Matrix`/`Note`) actuellement
    /// détachés en fenêtre OS séparée — voir la doc de tête du module,
    /// § détachement générique. Une entrée par onglet détaché ; absent de
    /// `dock_state` tant qu'il l'est (même invariant que `pm_detached` pour
    /// le Project Manager, généralisé à plusieurs onglets simultanés).
    detached_tabs: HashMap<Tab, DetachedTab>,
    /// Fichier où déposer une capture de la fenêtre, s'il en est demandé une
    /// (mécanisme de vérification, voir [`PlotyxApp::screenshot`]).
    screenshot: Option<std::path::PathBuf>,
    /// Images restant à laisser passer avant de demander la capture.
    screenshot_delay: u32,
    /// Délai d'auto-fermeture, s'il est demandé (mécanisme de vérification).
    autoquit: Option<Duration>,
    /// Instant du premier rendu, d'où se mesure le délai d'auto-fermeture.
    first_frame: Option<Instant>,
}

/// Ce que la fenêtre OS détachée du Project Manager a besoin de peindre —
/// strictement ça, et rien de plus. Voir la doc de tête de
/// [`PlotyxApp::show_detached_project_manager`] pour pourquoi ce détour
/// plutôt que de mettre `AppState` entière derrière un verrou.
#[derive(Debug, Default, Clone)]
struct ProjectManagerViewModel {
    /// Copie de [`AppState::project_tree`], resynchronisée à chaque image
    /// par [`PlotyxApp::show_detached_project_manager`] avant l'appel à
    /// `show_viewport_deferred` — `AppState` reste la seule source de
    /// vérité, ceci n'en est qu'un instantané pour le callback différé.
    tree: Vec<ProjectTreeNode>,
}

/// Ce que la fenêtre OS détachée du Project Manager écrit, pour que la
/// boucle principale (seule à porter `&mut AppState` et `&mut DockState`)
/// les applique après le rendu — même schéma que les `open_requests`/
/// `actions` locaux à [`PlotyxApp::ui`], juste posés par un callback différé
/// (potentiellement rappelé à une image où la fenêtre mère ne peint pas)
/// plutôt que collectés en direct pendant la peinture.
#[derive(Debug, Default)]
struct DetachedProjectManagerRequests {
    /// Onglets demandés par un clic dans l'arbre depuis la dernière
    /// vidange ; [`PlotyxApp::ui`] les transfère dans son propre
    /// `open_requests` puis les vide.
    open: Vec<Tab>,
    /// Vrai si le bouton « Réattacher » ou la fermeture de la fenêtre OS a
    /// été actionné depuis la dernière vidange.
    reattach: bool,
}

/// Un onglet de contenu détaché en fenêtre OS séparée — voir la doc de tête
/// du module, § détachement générique. Même patron que le Project Manager
/// (`pm_view_model`/`pm_requests`), répété une fois par onglet plutôt qu'une
/// fois pour toute l'appli : [`PlotyxApp::detached_tabs`] en porte une par
/// entrée.
struct DetachedTab {
    /// Ce que la fenêtre détachée peint, resynchronisé depuis `self.state`
    /// (et `self.graph_views` pour un graphe) à chaque image par
    /// [`PlotyxApp::show_detached_tabs`].
    view_model: Arc<Mutex<DetachedTabViewModel>>,
    /// Ce que la fenêtre détachée écrit, drainé chaque image par
    /// [`PlotyxApp::ui`].
    requests: Arc<Mutex<DetachedTabRequests>>,
}

/// Ce qu'une fenêtre détachée a besoin de peindre. Une seule structure pour
/// les quatre natures détachables plutôt que quatre `ViewModel` disjoints —
/// voir la doc de tête du module pour le compromis. Chaque champ, hormis
/// `snapshot`, n'a de sens que pour une nature de [`Tab`] particulière ; le
/// commentaire le dit.
#[derive(Debug, Default)]
struct DetachedTabViewModel {
    /// Instantané du modèle, resynchronisé à chaque image — voir la doc de
    /// tête du module, § « la lecture du modèle est factorisée ». `None`
    /// seulement avant la toute première synchronisation (jamais observable
    /// depuis l'extérieur de [`PlotyxApp::detach_tab`], qui le pose aussitôt).
    snapshot: Option<Snapshot>,
    /// `Tab::Book` : feuille actuellement choisie **dans cette fenêtre
    /// détachée** — sa propre sous-navigation, indépendante de celle que le
    /// classeur aurait dans le dock (il ne peut de toute façon pas être dans
    /// les deux à la fois).
    selected_sheet: Option<NodeId>,
    /// `Tab::Book` : saisie de cellule en cours, propre à cette fenêtre — pas
    /// le tampon global d'`AppState`, voir la doc de tête du module.
    cell_edit: Option<CellEdit>,
    /// `Tab::Note` : saisie de note en cours, propre à cette fenêtre — même
    /// raison que `cell_edit`.
    note_edit: Option<NoteEdit>,
    /// `Tab::Graph` : la vue (cache de formes, zoom/pan), déplacée hors de
    /// [`PlotyxApp::graph_views`] le temps du détachement — voir
    /// [`PlotyxApp::detach_tab`]/[`PlotyxApp::reattach_tab`].
    graph_view: Option<GraphView>,
}

/// Ce qu'une fenêtre détachée écrit, pour que [`PlotyxApp::ui`] (seule à
/// porter `&mut AppState`) les applique après le rendu — même rôle que
/// [`DetachedProjectManagerRequests`].
#[derive(Debug, Default)]
struct DetachedTabRequests {
    /// Actions produites par cette fenêtre depuis la dernière vidange
    /// (validation de cellule/note, structure de la grille…) — le même
    /// chemin de commande annulable que le dock, voir la doc de tête du
    /// module.
    actions: Vec<Action>,
    /// Vrai si « Réattacher au dock principal » ou la fermeture de la
    /// fenêtre OS a été actionné depuis la dernière vidange.
    reattach: bool,
}

impl PlotyxApp {
    /// Construit l'application autour d'un état.
    ///
    /// # Disposition initiale
    ///
    /// Ni le Project Manager ni la console de script n'entrent pour rien
    /// dans ce plan : ce sont deux panneaux fixes peints à part (voir
    /// [`PlotyxApp::project_manager_panel`] et [`PlotyxApp::script_panel`]),
    /// pas des onglets du `DockState` construit ici. Si le projet contient au
    /// moins un classeur, son onglet occupe le dock, suivi de l'onglet du
    /// premier graphe s'il y en a un — montrer quelque chose sans qu'on ait à
    /// cliquer, comme l'ancien comportement par défaut. À défaut de classeur
    /// ou de graphe, le dock démarre vide (seuls les deux panneaux fixes et
    /// la barre de menu sont visibles). Tout ceci n'est qu'un point de
    /// départ : `egui_dock` retient ensuite les redispositions de
    /// l'utilisateur.
    ///
    /// # Auto-fermeture
    ///
    /// Si la variable d'environnement `PLOTYX_APP_AUTOQUIT_MS` contient un
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

        let mut opened = Vec::new();
        if let Some((first_book, _)) = state.books().into_iter().next() {
            opened.push(Tab::Book(first_book));
        }
        if let Some((first_graph, _)) = state.graphs().into_iter().next() {
            opened.push(Tab::Graph(first_graph));
        }
        // Sans classeur ni graphe à montrer, le dock démarre simplement
        // vide — ni la console de script ni le Project Manager n'y ont plus
        // leur place depuis leur passage en panneaux fixes.
        let dock_state = DockState::new(opened);

        Self {
            state,
            dock_state,
            graph_views: HashMap::new(),
            maximized_tab: None,
            pm_detached: false,
            pm_view_model: Arc::new(Mutex::new(ProjectManagerViewModel::default())),
            pm_requests: Arc::new(Mutex::new(DetachedProjectManagerRequests::default())),
            detached_tabs: HashMap::new(),
            autoquit,
            first_frame: None,
            screenshot: std::env::var_os(SCREENSHOT_ENV).map(std::path::PathBuf::from),
            screenshot_delay: SCREENSHOT_SETTLE_FRAMES,
        }
    }

    /// Consomme les raccourcis au niveau du contexte et en fait des actions.
    fn collect_shortcuts(&self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        ctx.input_mut(|input| {
            if input.consume_shortcut(&SHORTCUT_SAVE) {
                actions.push(Action::Save);
            }
            if input.consume_shortcut(&SHORTCUT_OPEN) {
                actions.push(Action::Load);
            }
            if input.consume_shortcut(&SHORTCUT_RUN_SCRIPT) {
                actions.push(Action::RunScript);
            }
            // Le rétablissement se teste avant l'annulation : sans cela,
            // Ctrl+Maj+Z serait avalé par le raccourci Ctrl+Z, dont les
            // modificateurs sont compatibles.
            if input.consume_shortcut(&SHORTCUT_REDO_Z) || input.consume_shortcut(&SHORTCUT_REDO_Y)
            {
                actions.push(Action::Redo);
            } else if input.consume_shortcut(&SHORTCUT_UNDO) {
                actions.push(Action::Undo);
            }
        });
    }

    /// Barre de menu : « Fichier », « Édition » et « Fenêtre ».
    ///
    /// « Fenêtre » porte les deux fonctionnalités qui touchent à la
    /// disposition elle-même plutôt qu'au modèle : maximiser/restaurer
    /// l'onglet actif, et détacher/réattacher le Project Manager en fenêtre
    /// OS séparée — voir la doc de tête du module.
    fn menu_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, actions: &mut Vec<Action>) {
        egui::Panel::top("plotyx_menu_bar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("Fichier", |ui| {
                    if ui
                        .add(
                            egui::Button::new("Ouvrir")
                                .shortcut_text(ctx.format_shortcut(&SHORTCUT_OPEN)),
                        )
                        .clicked()
                    {
                        actions.push(Action::Load);
                        ui.close();
                    }
                    if ui
                        .add(
                            egui::Button::new("Enregistrer")
                                .shortcut_text(ctx.format_shortcut(&SHORTCUT_SAVE)),
                        )
                        .clicked()
                    {
                        actions.push(Action::Save);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Quitter").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        ui.close();
                    }
                });
                ui.menu_button("Édition", |ui| {
                    // Le libellé complet (« Annuler Saisie dans A ») et le
                    // grisage sont deux lignes distinctes ici ; Qt les tenait
                    // ensemble dans une seule `QAction`.
                    let undo = self
                        .state
                        .undo_label()
                        .unwrap_or_else(|| "Annuler".to_string());
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
                        .unwrap_or_else(|| "Rétablir".to_string());
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
                ui.menu_button("Fenêtre", |ui| {
                    if let Some(tab) = self.maximized_tab {
                        if ui
                            .button(format!("Restaurer « {} »", tab_title(&self.state, tab)))
                            .clicked()
                        {
                            self.maximized_tab = None;
                            ui.close();
                        }
                    } else {
                        // `find_active_focused` lit l'onglet actif du nœud
                        // focalisé du dock — celui que l'utilisateur regarde
                        // en ce moment, même sans avoir cliqué dedans cette
                        // image-ci. Le Project Manager n'y figure jamais
                        // depuis cette tâche (ce n'est plus un onglet), donc
                        // rien de spécial à exclure ici.
                        let active = self.dock_state.find_active_focused().map(|(_, tab)| *tab);
                        let can_maximize = active.is_some();
                        if ui
                            .add_enabled(
                                can_maximize,
                                egui::Button::new("Maximiser l'onglet actif"),
                            )
                            .clicked()
                        {
                            self.maximized_tab = active;
                            ui.close();
                        }
                    }
                    ui.separator();
                    if self.pm_detached {
                        if ui.button("Réattacher le Project Manager").clicked() {
                            self.reattach_project_manager();
                            ui.close();
                        }
                    } else if ui
                        .button("Détacher le Project Manager en fenêtre séparée")
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
    }

    /// Bandeau du chemin de fichier (lecture seule) et des actions
    /// globales. « Enregistrer sous… » et « Ouvrir… » passent par un
    /// sélecteur de fichiers natif (`rfd`) ; voir la doc de tête du module.
    ///
    /// Les actions de structure (ajouter une colonne, insérer/supprimer des
    /// lignes) ne vivent plus ici depuis que la grille est scopée par
    /// classeur (`Tab::Book`) : il n'y a plus de « la » feuille affichée sur
    /// laquelle un bouton global pourrait agir sans ambiguïté quand
    /// plusieurs classeurs sont ouverts en même temps. Elles vivent
    /// désormais dans [`book_contents`], à côté de la grille qu'elles
    /// modifient.
    fn tool_bar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::Panel::top("plotyx_tool_bar")
            .resizable(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Fichier :");
                    ui.weak(self.state.path());
                    if ui.button("Enregistrer sous…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("Enregistrer le projet")
                            .add_filter("Projet PlotYx", &["plx", "plotyx"])
                            .set_file_name(suggested_file_name(self.state.path()))
                            .save_file()
                        {
                            self.state.set_path(path.display().to_string());
                            actions.push(Action::Save);
                        }
                    }
                    if ui.button("Enregistrer").clicked() {
                        actions.push(Action::Save);
                    }
                    if ui.button("Ouvrir…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("Ouvrir un projet")
                            .add_filter("Projet PlotYx", &["plx", "plotyx"])
                            .pick_file()
                        {
                            self.state.set_path(path.display().to_string());
                            actions.push(Action::Load);
                        }
                    }
                });
            });
    }

    /// Barre d'état : dernier message, révision.
    ///
    /// Les dimensions (lignes × colonnes) qu'elle affichait ont disparu avec
    /// « la » feuille sélectionnée globale : chaque onglet de classeur les
    /// montre pour sa propre feuille, dans [`book_contents`].
    fn status_bar(&self, ui: &mut egui::Ui) {
        egui::Panel::bottom("plotyx_status")
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
                            ui.weak("prêt");
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak(format!("révision {}", self.state.snapshot().revision));
                    });
                });
            });
    }

    /// Peint la console de script dans son panneau fixe, ancré en bas — voir
    /// la doc de tête du module, § Console de script. Appelé par
    /// [`PlotyxApp::ui`] *avant* [`PlotyxApp::project_manager_panel`]
    /// (`egui::Panel::left`) : avec `egui::Panel`, le premier panneau ajouté
    /// est le plus extérieur, donc celui-ci, posé en premier, réserve sa
    /// bande sur toute la largeur de la fenêtre — y compris sous le Project
    /// Manager, pas seulement sous la zone de la `DockArea` — plutôt que
    /// d'être resserré entre lui et le bord droit. `egui::Panel::bottom`
    /// n'est **pas** redimensionnable par défaut (à la différence de
    /// `left`/`right`) : `.resizable(true)` restaure le glisser du bord en
    /// hauteur qu'avait la console dans le dock. Contrairement au Project
    /// Manager, aucun mode détaché en fenêtre OS séparée ni bascule de
    /// position : la console de script a toujours été en bas, et le reste.
    fn script_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, actions: &mut Vec<Action>) {
        egui::Panel::bottom("plotyx_script")
            .resizable(true)
            .default_size(SCRIPT_PANEL_HEIGHT)
            .show(ui, |ui| {
                script_contents(ui, ctx, &mut self.state, actions);
            });
    }

    /// Peint le Project Manager dans son panneau fixe, ancré à gauche — voir
    /// la doc de tête du module, § Project Manager. Appelé par
    /// [`PlotyxApp::ui`] *avant* la `DockArea`/le `CentralPanel`, mais
    /// *après* [`PlotyxApp::script_panel`] — voir sa doc de tête pour
    /// pourquoi cet ordre précis. Comme [`PlotyxApp::menu_bar`]
    /// (`egui::Panel::top`) : peindre un panneau ancré avant le reste
    /// garantit par construction qu'il n'est jamais recouvert ni ne recouvre
    /// ce qui vient après. N'est pas appelé quand [`PlotyxApp::pm_detached`]
    /// vaut `true` — cette image-là, rien ne remplace ce panneau, la vraie
    /// fenêtre OS séparée ([`PlotyxApp::show_detached_project_manager`]) en
    /// tient lieu. Reste redimensionnable en largeur par glisser du bord
    /// (comportement par défaut d'`egui::Panel::left`) mais figé en
    /// position : pas de bascule vers la droite ni de repositionnement par
    /// glisser, voir la doc de tête du module, § attacher à gauche ou à
    /// droite.
    fn project_manager_panel(&mut self, ui: &mut egui::Ui, open_requests: &mut Vec<Tab>) {
        egui::Panel::left("plotyx_pm")
            .default_size(PROJECT_MANAGER_WIDTH)
            .show(ui, |ui| {
                project_manager_contents(ui, &self.state, open_requests);
            });
    }

    /// Ouvre l'onglet `tab`, ou le focalise s'il est déjà ouvert.
    ///
    /// Le seul endroit qui décide entre les deux, pour toute nature de
    /// fenêtre (classeur, graphe, matrice, note) : cliquer une seconde fois
    /// la même entrée du Project Manager ne doit jamais dupliquer son
    /// onglet, seulement l'amener au premier plan.
    fn open_tab(&mut self, tab: Tab) {
        if let Some(path) = self.dock_state.find_tab(&tab) {
            let _ = self.dock_state.set_active_tab(path);
            self.dock_state
                .set_focused_node_and_surface(path.node_path());
        } else {
            self.dock_state.push_to_focused_leaf(tab);
        }
    }

    /// Bascule en mode détaché : le prochain rendu ouvre le Project Manager
    /// dans sa propre fenêtre OS ([`PlotyxApp::show_detached_project_manager`])
    /// au lieu du panneau fixe ([`PlotyxApp::project_manager_panel`]). Sans
    /// effet s'il l'est déjà.
    fn detach_project_manager(&mut self) {
        self.pm_detached = true;
    }

    /// Bascule hors du mode détaché : le panneau fixe réapparaît au prochain
    /// rendu, toujours à gauche puisqu'il est figé en position. Sans effet
    /// s'il n'est pas détaché.
    fn reattach_project_manager(&mut self) {
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
    ///   [`PlotyxApp::ui`] la draine juste après cet appel pour produire les
    ///   mêmes `open_requests`/`Action` qu'avant, sans dupliquer la logique
    ///   de [`PlotyxApp::open_tab`] ni appeler
    ///   [`PlotyxApp::reattach_project_manager`] depuis l'intérieur d'un
    ///   callback qui n'a pas accès à `&mut self`.
    fn show_detached_project_manager(&mut self, ctx: &egui::Context) {
        {
            let mut model = self
                .pm_view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            model.tree = self.state.project_tree();
        }
        let view_model = Arc::clone(&self.pm_view_model);
        let requests = Arc::clone(&self.pm_requests);
        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("plotyx_project_manager_window"),
            egui::ViewportBuilder::default()
                .with_title("PlotYx — Project Manager")
                .with_inner_size([PROJECT_MANAGER_WIDTH + 40.0, 480.0]),
            move |ui, _class| {
                let mut opened = Vec::new();
                egui::CentralPanel::default().show(ui, |ui| {
                    if ui.button("Réattacher au dock principal").clicked() {
                        requests
                            .lock()
                            .unwrap_or_else(|err| err.into_inner())
                            .reattach = true;
                    }
                    ui.separator();
                    // Lecture d'une copie plutôt que du verrou tenu pendant
                    // toute la peinture : `project_manager_contents_from_tree`
                    // ne doit pas retenir `view_model` verrouillé le temps
                    // d'un `ScrollArea` entier.
                    let tree = view_model
                        .lock()
                        .unwrap_or_else(|err| err.into_inner())
                        .tree
                        .clone();
                    project_manager_contents_from_tree(ui, &tree, &mut opened);
                });
                if !opened.is_empty() {
                    requests
                        .lock()
                        .unwrap_or_else(|err| err.into_inner())
                        .open
                        .append(&mut opened);
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
    }

    /// Retire `tab` du dock et le détache en fenêtre OS séparée — voir la
    /// doc de tête du module, § détachement générique. Sans effet si `tab`
    /// est déjà détaché ou n'existe pas dans le dock. Ni le Project Manager
    /// ni la console de script n'entrent dans ce chemin depuis leur passage
    /// en panneaux fixes — voir [`PlotyxApp::detach_project_manager`] pour le
    /// premier ; la seconde n'a simplement plus de mécanisme de détachement
    /// du tout.
    fn detach_tab(&mut self, tab: Tab) {
        if self.detached_tabs.contains_key(&tab) {
            return;
        }
        if let Some(path) = self.dock_state.find_tab(&tab) {
            self.dock_state.remove_tab(path);
        } else {
            return;
        }
        if self.maximized_tab == Some(tab) {
            // Un onglet maximisé qui n'est plus dans le dock serait un état
            // incohérent : redescendre proprement avant de le détacher.
            self.maximized_tab = None;
        }
        let graph_view = match tab {
            Tab::Graph(id) => Some(self.graph_views.remove(&id).unwrap_or_default()),
            _ => None,
        };
        let selected_sheet = match tab {
            Tab::Book(id) => self.state.selected_sheet_in(id),
            _ => None,
        };
        let view_model = DetachedTabViewModel {
            snapshot: Some(self.state.snapshot().clone()),
            selected_sheet,
            cell_edit: None,
            note_edit: None,
            graph_view,
        };
        self.detached_tabs.insert(
            tab,
            DetachedTab {
                view_model: Arc::new(Mutex::new(view_model)),
                requests: Arc::new(Mutex::new(DetachedTabRequests::default())),
            },
        );
    }

    /// Retire `tab` de [`PlotyxApp::detached_tabs`] et le repousse dans le
    /// dock (au nœud focalisé, ou en nouvelle feuille s'il n'y en a pas).
    /// Sans effet s'il n'est pas détaché. Une éventuelle [`GraphView`]
    /// déplacée par [`PlotyxApp::detach_tab`] revient dans
    /// [`PlotyxApp::graph_views`] — le cache de formes du graphe survit au
    /// détachement/réattachement, comme il survit déjà à la fermeture d'un
    /// onglet docké.
    fn reattach_tab(&mut self, tab: Tab) {
        let Some(detached) = self.detached_tabs.remove(&tab) else {
            return;
        };
        {
            let mut model = detached
                .view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            if let (Tab::Graph(id), Some(view)) = (tab, model.graph_view.take()) {
                self.graph_views.insert(id, view);
            }
        }
        if self.dock_state.find_tab(&tab).is_none() {
            self.dock_state.push_to_focused_leaf(tab);
        }
    }

    /// Peint chaque onglet de [`PlotyxApp::detached_tabs`] dans sa propre
    /// fenêtre OS, via `show_viewport_deferred` — voir la doc de tête du
    /// module, § détachement générique, et [`PlotyxApp::
    /// show_detached_project_manager`] pour la justification du choix
    /// `_deferred`/`Fn` déjà faite pour le Project Manager (identique ici).
    ///
    /// Ne draine **pas** les requêtes produites : [`PlotyxApp::ui`] le fait
    /// juste après, une fois cette fonction revenue, exactement comme pour
    /// `pm_requests`.
    fn show_detached_tabs(&mut self, ctx: &egui::Context) {
        let Self {
            state,
            detached_tabs,
            ..
        } = self;
        for (tab, detached) in detached_tabs.iter() {
            {
                let mut model = detached
                    .view_model
                    .lock()
                    .unwrap_or_else(|err| err.into_inner());
                model.snapshot = Some(state.snapshot().clone());
            }
            let view_model = Arc::clone(&detached.view_model);
            let requests = Arc::clone(&detached.requests);
            let tab = *tab;
            let title = format!("PlotYx — {}", tab_title(state, tab));
            // `Tab::Graph` garde un ratio 4:3 (déjà celui de 800×600). Le
            // reste (`Book`/`Matrix`/`Note`) partage une taille uniforme
            // montrant 3 colonnes de grille : `grid_contents`/`matrix_contents`
            // utilisent `TableColumn::exact(56.0)` pour la colonne de numéro
            // de ligne puis `TableColumn::initial(110.0)` par colonne de
            // données, soit 56 + 3×110 = 386 ; 420 ajoute une marge pour les
            // bordures et la scrollbar verticale de la `ScrollArea`.
            // `Tab::Note` n'a pas de grille à proprement parler mais reçoit
            // la même taille que « les autres » pour rester cohérente avec
            // eux — c'est explicitement voulu, pas un oubli.
            let size = match tab {
                Tab::Graph(_) => [800.0, 600.0],
                _ => [420.0, 360.0],
            };
            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of(tab),
                egui::ViewportBuilder::default()
                    .with_title(title)
                    .with_inner_size(size),
                move |ui, _class| {
                    let mut model = view_model.lock().unwrap_or_else(|err| err.into_inner());
                    let mut requests = requests.lock().unwrap_or_else(|err| err.into_inner());
                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("Réattacher au dock principal").clicked() {
                                requests.reattach = true;
                            }
                        });
                        ui.separator();
                        let Some(snapshot) = model.snapshot.clone() else {
                            // Ne devrait jamais arriver : `detach_tab` pose le
                            // premier instantané avant toute image détachée.
                            return;
                        };
                        let tree = &snapshot.tree;
                        match tab {
                            Tab::Book(id) => {
                                detached_book_contents(ui, tree, &mut model, &mut requests, id);
                            }
                            Tab::Graph(id) => {
                                let view = model.graph_view.get_or_insert_with(GraphView::default);
                                graph_view_contents(ui, view, &snapshot, id);
                            }
                            Tab::Matrix(id) => {
                                detached_matrix_contents(ui, tree, id);
                            }
                            Tab::Note(id) => {
                                detached_note_contents(ui, tree, &mut model, &mut requests, id);
                            }
                        }
                    });
                    if ui.ctx().input(|input| input.viewport().close_requested()) {
                        requests.reattach = true;
                    }
                },
            );
        }
    }

    /// Dépose une capture de la fenêtre dans le fichier nommé par
    /// `PLOTYX_APP_SCREENSHOT`, puis n'en redemande plus.
    ///
    /// **Mécanisme de vérification, pas une fonctionnalité** — exactement
    /// comme l'auto-fermeture, et pour un besoin voisin apparu avec la
    /// phase R4 : sur un chantier de rendu, « la fenêtre s'ouvre et ne
    /// plante pas » ne suffit plus, il faut pouvoir *regarder* ce qu'elle
    /// peint. Les tests couvrent la géométrie (`plotyx-render-egui`), pas
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
            Ok(()) => eprintln!("[plotyx-app] capture écrite : {}", path.display()),
            Err(err) => eprintln!(
                "[plotyx-app] capture impossible dans {} : {err}",
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

impl eframe::App for PlotyxApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // `Context` est un `Arc` : le cloner coûte un compteur, et évite
        // d'emprunter `ui` en lecture pendant qu'on le peint.
        let ctx = ui.ctx().clone();
        let mut actions: Vec<Action> = Vec::new();
        // Onglets demandés par un clic dans le Project Manager (panneau fixe
        // ou détaché en fenêtre OS) : appliqués après le rendu, comme
        // `actions` — muter `dock_state` en plein parcours de la `DockArea`
        // l'invaliderait.
        let mut open_requests: Vec<Tab> = Vec::new();
        // Onglets dont le menu contextuel du dock a demandé le détachement
        // cette image-ci — appliqués après le rendu, comme `open_requests`
        // (voir [`AppTabViewer::detach_requests`]).
        let mut detach_requests: Vec<Tab> = Vec::new();

        // Sonde un script terminé sur son fil détaché **avant** de peindre :
        // sans notification poussée de l'acteur, c'est le seul endroit qui
        // rafraîchit l'instantané après un script (voir la doc de tête de
        // `crate::state`). Ne bloque jamais, même sans script en cours.
        self.state.poll_script();

        self.collect_shortcuts(&ctx, &mut actions);
        self.menu_bar(ui, &ctx, &mut actions);
        self.tool_bar(ui, &mut actions);
        self.status_bar(ui);
        // Panneau fixe, peint *avant* `PlotyxApp::project_manager_panel` —
        // voir la doc de tête du module, § Console de script, et la doc de
        // tête de `PlotyxApp::script_panel` : c'est cet ordre précis qui lui
        // garantit de courir sur toute la largeur de la fenêtre, y compris
        // sous le Project Manager, plutôt que d'être resserré entre lui et
        // le bord droit.
        self.script_panel(ui, &ctx, &mut actions);

        if self.pm_detached {
            self.show_detached_project_manager(&ctx);
            // Vidange ce que la fenêtre OS détachée a écrit depuis la
            // dernière image (voir la doc de tête de
            // `show_detached_project_manager`) — même moment que le reste de
            // `open_requests`, avant de peindre le panneau fixe (qui ne sera
            // de toute façon pas peint cette image-ci) et la `DockArea`.
            let should_reattach = {
                let mut requests = self
                    .pm_requests
                    .lock()
                    .unwrap_or_else(|err| err.into_inner());
                open_requests.append(&mut requests.open);
                std::mem::take(&mut requests.reattach)
            };
            if should_reattach {
                self.reattach_project_manager();
            }
        } else {
            // Panneau fixe, peint *avant* la `DockArea`/le `CentralPanel` —
            // voir la doc de tête du module, § Project Manager : c'est ce qui
            // garantit par construction qu'il n'est jamais recouvert par elle
            // ni ne la recouvre, comme la barre de menu (`egui::Panel::top`)
            // peinte plus haut.
            self.project_manager_panel(ui, &mut open_requests);
        }

        if !self.detached_tabs.is_empty() {
            self.show_detached_tabs(&ctx);
            // Même vidange que pour le Project Manager, généralisée à
            // plusieurs onglets : chacun peut avoir accumulé des actions et/ou
            // demandé à être réattaché depuis la dernière image.
            let mut to_reattach: Vec<Tab> = Vec::new();
            for (tab, detached) in &self.detached_tabs {
                let mut requests = detached
                    .requests
                    .lock()
                    .unwrap_or_else(|err| err.into_inner());
                actions.append(&mut requests.actions);
                if std::mem::take(&mut requests.reattach) {
                    to_reattach.push(*tab);
                }
            }
            for tab in to_reattach {
                self.reattach_tab(tab);
            }
        }

        // Emprunts disjoints des champs (édition 2021) : la disposition et
        // les contenus qu'elle amarre ont besoin, chacun, d'un accès distinct
        // à `self`, simultanément le temps de peindre une image.
        let Self {
            state,
            dock_state,
            graph_views,
            maximized_tab,
            ..
        } = self;
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(tab) = *maximized_tab {
                // § maximiser (doc de tête du module) : la `DockArea` normale
                // n'est pas peinte du tout cette image-ci. Le Project
                // Manager n'a plus besoin d'aucun traitement particulier
                // ici : son panneau fixe est peint plus haut, avant ce
                // `CentralPanel`, qu'un autre onglet soit maximisé ou non.
                ui.horizontal(|ui| {
                    ui.heading(tab_title(state, tab));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Restaurer la disposition").clicked() {
                            *maximized_tab = None;
                        }
                    });
                });
                ui.separator();
                tab_contents(ui, state, &mut actions, graph_views, tab);
            } else {
                let mut viewer = AppTabViewer {
                    state,
                    actions: &mut actions,
                    graph_views,
                    detach_requests: &mut detach_requests,
                };
                // `overlay` est le rectangle/l'icône qu'`egui_dock` peint
                // pendant un glisser pour indiquer où l'onglet atterrirait
                // (`egui_dock::style::OverlayStyle`) — rendu entièrement
                // transparent, globalement (pas de crochet par-onglet dans
                // cette version d'`egui_dock`). Hérité de l'époque où le
                // Project Manager avait des boutons « Attacher à gauche/à
                // droite » avec lesquels ce rectangle aurait fait doublon ;
                // conservé tel quel, une zone de dépôt visible n'apporterait
                // rien de plus pour les onglets de contenu restants. Le
                // glisser-déposer natif d'`egui_dock` continue de fonctionner
                // à l'identique : seul son rendu de zone de dépôt disparaît.
                let mut style = Style::from_egui(ui.style());
                style.overlay.selection_color = egui::Color32::TRANSPARENT;
                style.overlay.button_color = egui::Color32::TRANSPARENT;
                style.overlay.button_border_stroke = egui::Stroke::NONE;
                style.overlay.hovered_leaf_highlight.color = egui::Color32::TRANSPARENT;
                style.overlay.hovered_leaf_highlight.stroke = egui::Stroke::NONE;
                DockArea::new(dock_state)
                    .style(style)
                    .show_inside(ui, &mut viewer);
            }
        });

        // Fenêtre d'import (`Action::ImportFiles`, voir la doc de tête de
        // [`import_window`]) : un dialogue transitoire, peint tant que
        // `self.state.import_state()` est `Some`, plutôt qu'un onglet du
        // dock — après la `DockArea`/le `CentralPanel` mais avant la
        // vidange des requêtes ci-dessous, pour que l'`Action::ImportFiles`
        // qu'elle pousse éventuellement soit dispatchée cette image-ci comme
        // toute autre action.
        import_window(&ctx, &mut self.state, &mut actions);

        // Les onglets demandés (Project Manager, panneau fixe ou détaché), les
        // détachements demandés par le menu contextuel, et les actions
        // récoltées par tous les panneaux et onglets ne sont appliqués
        // qu'une fois l'image peinte : muter le plan de disposition ou le
        // modèle en plein rendu invaliderait ce que la `DockArea` est en
        // train de parcourir.
        for tab in open_requests {
            self.open_tab(tab);
        }
        for tab in detach_requests {
            self.detach_tab(tab);
        }
        for action in actions {
            self.state.dispatch(action);
        }

        self.screenshot(&ctx);
        self.autoquit(&ctx);
    }
}

/// Le nom de fichier suggéré par défaut au sélecteur « Enregistrer sous… » :
/// le nom du chemin courant s'il en a un, sinon un nom de repli.
fn suggested_file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "demo.plotyx".to_string())
}

/// Traduit une nature de fenêtre du Project Manager en l'onglet qui l'ouvre.
fn tab_for_window(id: NodeId, kind: WindowKind) -> Tab {
    match kind {
        WindowKind::Book => Tab::Book(id),
        WindowKind::Graph => Tab::Graph(id),
        WindowKind::Matrix => Tab::Matrix(id),
        WindowKind::Note => Tab::Note(id),
    }
}

/// Titre d'un onglet : le nom du nœud qu'il porte, ou un texte « supprimé »
/// explicite s'il a disparu de l'arbre entre-temps (fermeture depuis un
/// autre onglet, ou par un script) — un onglet resté ouvert sur un nœud
/// disparu ne doit jamais paniquer ni disparaître silencieusement
/// (invariant 10).
fn tab_title(state: &AppState, tab: Tab) -> String {
    match tab {
        Tab::Book(id) => node_title(state, id, "Classeur"),
        Tab::Graph(id) => node_title(state, id, "Graphe"),
        Tab::Matrix(id) => node_title(state, id, "Matrice"),
        Tab::Note(id) => node_title(state, id, "Note"),
    }
}

/// Le nom du nœud `id`, ou `"{missing} {id} (supprimé)"` s'il n'existe plus.
fn node_title(state: &AppState, id: NodeId, missing: &str) -> String {
    state
        .tree()
        .node(id)
        .map(|node| node.name().to_string())
        .unwrap_or_else(|| format!("{missing} {id} (supprimé)"))
}

/// Peint le contenu de l'onglet `tab`, quelle que soit sa provenance —
/// [`AppTabViewer::ui`] (dock normal) ou la zone de maximisation
/// ([`PlotyxApp::ui`]) l'appellent toutes les deux : **une seule** fonction
/// qui sait peindre un onglet, jamais deux implémentations à tenir en
/// accord. Ne prend plus de `&egui::Context` depuis que la console de script
/// (seule nature à en avoir eu besoin, pour son `request_repaint`) est sortie
/// du dock — voir [`PlotyxApp::script_panel`].
fn tab_contents(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    graph_views: &mut HashMap<NodeId, GraphView>,
    tab: Tab,
) {
    match tab {
        Tab::Book(id) => book_contents(ui, state, actions, id),
        Tab::Graph(id) => graph_contents(ui, state, graph_views, id),
        Tab::Matrix(id) => matrix_contents(ui, state, id),
        Tab::Note(id) => note_contents(ui, state, actions, id),
    }
}

/// Le panneau Project Manager lui-même : l'arbre du projet
/// ([`AppState::project_tree`]), dossiers dépliables et fenêtres cliquables.
/// Cliquer une fenêtre pousse l'onglet qui l'ouvre dans `open_requests` (une
/// ouverture ou une focalisation, tranchée par [`PlotyxApp::open_tab`] après
/// le rendu). Fonction libre plutôt que méthode : appelée à la fois depuis le
/// dock normal, la zone de maximisation, et la fenêtre OS séparée — trois
/// contextes, un seul rendu à tenir en accord.
fn project_manager_contents(ui: &mut egui::Ui, state: &AppState, open_requests: &mut Vec<Tab>) {
    let tree = state.project_tree();
    project_manager_contents_from_tree(ui, &tree, open_requests);
}

/// Le corps de [`project_manager_contents`], à partir d'un arbre déjà
/// construit plutôt que d'un `&AppState` vivant. Seul le callback
/// `show_viewport_deferred` de
/// [`PlotyxApp::show_detached_project_manager`] a besoin de cette forme :
/// il ne porte qu'une copie de l'arbre ([`ProjectManagerViewModel`]), jamais
/// `AppState` elle-même — voir la doc de tête de cette méthode.
fn project_manager_contents_from_tree(
    ui: &mut egui::Ui,
    tree: &[ProjectTreeNode],
    open_requests: &mut Vec<Tab>,
) {
    ui.heading("Project Manager");
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for node in tree {
            project_tree_node_ui(ui, node, open_requests);
        }
    });
}

/// Peint récursivement un nœud de [`ProjectTreeNode`] et ses enfants.
fn project_tree_node_ui(ui: &mut egui::Ui, node: &ProjectTreeNode, open_requests: &mut Vec<Tab>) {
    match node {
        ProjectTreeNode::Folder { name, children, .. } => {
            egui::CollapsingHeader::new(format!("📁 {name}"))
                .default_open(false)
                .show(ui, |ui| {
                    if children.is_empty() {
                        ui.weak("(vide)");
                    }
                    for child in children {
                        project_tree_node_ui(ui, child, open_requests);
                    }
                });
        }
        ProjectTreeNode::Window { id, name, kind } => {
            let icon = match kind {
                WindowKind::Book => "📗",
                WindowKind::Graph => "📈",
                WindowKind::Matrix => "🔢",
                WindowKind::Note => "📝",
            };
            if ui.button(format!("{icon} {name}")).clicked() {
                open_requests.push(tab_for_window(*id, *kind));
            }
        }
    }
}

/// Suffixe affiché sous le nom d'une colonne pour rappeler son type.
fn type_suffix(column_type: ColumnType) -> &'static str {
    match column_type {
        ColumnType::Numeric => "num",
        ColumnType::DateTime => "date",
        ColumnType::Text => "texte",
        ColumnType::Mixed => "mixte",
    }
}

/// Lettre de colonne façon tableur, en base 26 : `0 → "A"`, `25 → "Z"`,
/// `26 → "AA"`, `701 → "ZZ"`… — l'ordre de la feuille, pas un nom stocké
/// dans le modèle (aucune notion de lettre n'existe côté `plotyx-core`, voir
/// la doc de tête de [`crate::selection`]).
fn column_letter(mut index: usize) -> String {
    let mut letters = Vec::new();
    loop {
        letters.push((b'A' + (index % 26) as u8) as char);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    letters.iter().rev().collect()
}

/// En-tête empilée d'une colonne : lettre (clic → sélectionne la colonne
/// entière), puis nom long, unité, commentaire et formule — les quatre
/// champs de [`plotyx_core::ColumnMeta`], jusqu'ici stockés sans jamais être
/// montrés côté UI. Chacun bascule affichage/édition au double-clic, comme
/// [`cell_widget`].
///
/// La rangée des lettres porte un fond distinct
/// ([`crate::theme::header_letter_row_fill`]) — elle seule sélectionne la
/// colonne au clic, les quatre rangées de champs qui suivent n'affichent
/// plus qu'un libellé grisé répété par colonne (leur nom de champ vit une
/// seule fois dans la gouttière des numéros de ligne, voir
/// [`row_number_header_widget`]) : elles n'affichent que la **valeur** du
/// champ, vide si le champ ne l'est pas.
fn column_header_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    column: NodeId,
    column_index: usize,
    column_type: ColumnType,
) {
    ui.vertical(|ui| {
        let letter = column_letter(column_index);
        let is_column_selected = state.is_column_selected(sheet, column);
        ui.scope(|ui| {
            ui.set_min_height(HEADER_SUBROW_HEIGHT);
            ui.set_max_height(HEADER_SUBROW_HEIGHT);
            let rect = ui.available_rect_before_wrap();
            let fill = if is_column_selected {
                ui.visuals().selection.bg_fill.linear_multiply(0.35)
            } else {
                crate::theme::header_letter_row_fill()
            };
            ui.painter().rect_filled(rect, 0.0, fill);
            header_row_separator(ui, rect);
            // Toute la largeur de la rangée sélectionne la colonne — pas
            // seulement la lettre — au clic principal comme au clic
            // secondaire (`response.clicked()`/`secondary_clicked()`
            // couvrent tous deux `Sense::click()`) : le clic droit doit
            // pouvoir sélectionner *et* ouvrir le menu contextuel en un
            // seul geste, sans exiger que la colonne soit déjà
            // sélectionnée au préalable.
            let id = ui.id().with(("plotyx_column_letter_row", column.raw()));
            let response = ui.interact(rect, id, egui::Sense::click());
            ui.put(
                rect,
                egui::Label::new(format!("{letter}  {}", type_suffix(column_type)))
                    .selectable(false),
            );
            if response.clicked() || response.secondary_clicked() {
                state.select_column(sheet, column);
            }
            // Choix manuel du type de colonne, au clic droit — conversion
            // destructrice assumée (`Action::SetColumnType`/`Command::
            // SetColumnType`, `plotyx-core::Column::set_column_type`) :
            // texte non numérique effacé en passant en `Numeric`, comme
            // pour toute saisie directe dans une colonne numérique.
            response.context_menu(|ui| {
                for candidate in [ColumnType::Numeric, ColumnType::Text, ColumnType::Mixed] {
                    if ui
                        .selectable_label(column_type == candidate, type_suffix(candidate))
                        .clicked()
                    {
                        if candidate != column_type {
                            actions.push(Action::SetColumnType {
                                column,
                                column_type: candidate,
                            });
                        }
                        ui.close();
                    }
                }
            });
        });
        header_field_widget(
            ui,
            state,
            actions,
            sheet,
            column,
            HeaderField::LongName,
            HEADER_LONGNAME_HEIGHT,
            is_column_selected,
        );
        header_field_widget(
            ui,
            state,
            actions,
            sheet,
            column,
            HeaderField::Unit,
            HEADER_SUBROW_HEIGHT,
            is_column_selected,
        );
        header_field_widget(
            ui,
            state,
            actions,
            sheet,
            column,
            HeaderField::Comment,
            HEADER_SUBROW_HEIGHT,
            is_column_selected,
        );
        header_field_widget(
            ui,
            state,
            actions,
            sheet,
            column,
            HeaderField::Formula,
            HEADER_SUBROW_HEIGHT,
            is_column_selected,
        );
    });
}

/// Ligne de séparation fine sous une rangée de l'en-tête empilé (lettre, ou
/// l'un des quatre champs) : sans elle, les quatre rangées de champs
/// n'avaient aucun remplissage propre et se confondaient visuellement les
/// unes dans les autres (contrairement aux lignes de données, zébrées par
/// `TableBuilder::striped`).
fn header_row_separator(ui: &egui::Ui, rect: egui::Rect) {
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
}

/// Une rangée éditable de l'en-tête empilé, pour un seul champ de
/// [`plotyx_core::ColumnMeta`] — le même schéma sélection/édition que
/// [`cell_widget`] pour une cellule de données : simple clic sélectionne
/// (surlignage identique), double-clic édite ; piloté par [`AppState::
/// header_editing`]/[`AppState::select_header_field`] plutôt que par
/// [`AppState::editing`]/[`AppState::select_cell`].
///
/// N'affiche que la **valeur** du champ, vide si le champ l'est — pas de
/// placeholder répété par colonne : le libellé du champ (« Long Name »,
/// « Units », « Comments », « F(x)= ») vit une seule fois dans la gouttière
/// des numéros de ligne, voir [`row_number_header_widget`].
#[allow(clippy::too_many_arguments)]
fn header_field_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    column: NodeId,
    field: HeaderField,
    height: f32,
    column_selected: bool,
) {
    let _ = actions; // la validation passe par `AppState::commit_header_edit`, appelé directement (mutation transitoire, comme `commit_edit`).
    ui.scope(|ui| {
        ui.set_min_height(height);
        ui.set_max_height(height);
        let rect = ui.available_rect_before_wrap();
        header_row_separator(ui, rect);
        let editing_here = state
            .header_editing()
            .is_some_and(|edit| edit.column == column && edit.field == field);
        if editing_here {
            let Some(edit) = state.header_editing_mut() else {
                return;
            };
            let response = ui.add(
                egui::TextEdit::singleline(&mut edit.buffer)
                    .frame(egui::Frame::NONE)
                    .desired_width(f32::INFINITY),
            );
            // Même piège que [`cell_widget`] : lire `lost_focus()` avant de
            // décider de redemander le focus, sous peine de se le redonner
            // à soi-même juste après qu'Entrée l'ait fait perdre en interne
            // au `TextEdit`, et de ne plus jamais valider.
            let lost_focus = response.lost_focus();
            if !response.has_focus() && !lost_focus {
                response.request_focus();
            }
            if lost_focus {
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    state.cancel_header_edit();
                } else {
                    state.commit_header_edit();
                }
            }
            return;
        }
        let text = header_field_text(state, column, field);
        let id = ui.id().with((column.raw(), field as u8));
        let response = ui.interact(rect, id, egui::Sense::click());
        // Surlignée si ce champ précis est sélectionné, ou si c'est toute la
        // colonne qui l'est (sélection faite depuis la rangée des lettres,
        // voir `column_header_widget`) : la sélection d'une colonne doit se
        // voir sur ses 5 rangées d'en-tête, pas seulement sur la rangée des
        // lettres.
        let selected = column_selected || state.is_header_field_selected(sheet, column, field);
        if selected {
            ui.painter().rect_filled(
                rect,
                0.0,
                ui.visuals().selection.bg_fill.linear_multiply(0.35),
            );
        }
        ui.put(
            rect,
            egui::Label::new(egui::RichText::new(text).weak())
                .selectable(false)
                .truncate(),
        );
        if response.double_clicked() {
            state.begin_header_edit(column, field);
        } else if response.clicked() {
            state.select_header_field(sheet, column, field);
        }
    });
}

/// Gouttière des numéros de ligne, côté en-tête empilée : la lettre `#`
/// habituelle sur la rangée des lettres (inchangée, seule la lettre de
/// colonne y sélectionne quoi que ce soit), puis — aligné avec chacune des
/// quatre rangées de champs de [`column_header_widget`] — le **libellé** du
/// champ concerné (« Long Name », « Units », « Comments », « F(x)= »,
/// repris de l'ancienne UI Qt), une seule fois plutôt que répété en grisé
/// dans chaque colonne. Les cellules par colonne n'affichent plus que la
/// valeur (voir [`header_field_widget`]).
fn row_number_header_widget(ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        ui.scope(|ui| {
            ui.set_min_height(HEADER_SUBROW_HEIGHT);
            ui.set_max_height(HEADER_SUBROW_HEIGHT);
            let rect = ui.available_rect_before_wrap();
            ui.painter()
                .rect_filled(rect, 0.0, crate::theme::header_letter_row_fill());
            header_row_separator(ui, rect);
            ui.strong("#");
        });
        row_number_header_label(ui, HEADER_LONGNAME_HEIGHT, "Long Name");
        row_number_header_label(ui, HEADER_SUBROW_HEIGHT, "Units");
        row_number_header_label(ui, HEADER_SUBROW_HEIGHT, "Comments");
        row_number_header_label(ui, HEADER_SUBROW_HEIGHT, "F(x)=");
    });
}

/// Une rangée de la gouttière (voir [`row_number_header_widget`]) : le
/// libellé d'un champ, à la hauteur exacte de la rangée de
/// [`column_header_widget`] qu'elle longe.
fn row_number_header_label(ui: &mut egui::Ui, height: f32, label: &str) {
    ui.scope(|ui| {
        ui.set_min_height(height);
        ui.set_max_height(height);
        let rect = ui.available_rect_before_wrap();
        header_row_separator(ui, rect);
        ui.strong(label);
    });
}

/// Valeur actuelle d'un champ d'en-tête, telle qu'affichée hors édition.
fn header_field_text(state: &AppState, column: NodeId, field: HeaderField) -> String {
    let meta = state.column_meta(column);
    match field {
        HeaderField::LongName => meta.long_name,
        HeaderField::Unit => meta.unit,
        HeaderField::Comment => meta.comment,
        HeaderField::Formula => meta.formula.unwrap_or_default(),
    }
}

/// Contenu d'un [`Tab::Book`] : la barre d'actions de structure, la grille
/// ([`grid_contents`]) scopée à la feuille choisie *dans ce classeur*, et
/// la sous-navigation entre les feuilles — des onglets compacts en **bas**
/// de la grille (choix assumé, pas une convention préexistante du crate :
/// aucun autre onglet de sheet de ce genre n'existait à imiter). Les
/// boutons « Insérer/Supprimer 1 ligne » ont disparu : l'ajout de ligne se
/// fait désormais en pressant Entrée sur la dernière cellule d'une colonne
/// ([`Action::CommitCellAndAdvance`]), la suppression par la touche Suppr
/// sur la sélection ([`Action::DeleteSelectedCells`]).
fn book_contents(ui: &mut egui::Ui, state: &mut AppState, actions: &mut Vec<Action>, book: NodeId) {
    let sheets = state.worksheets_of(book);
    if sheets.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.weak(format!(
                "« {} » n'a aucune feuille (ou a été supprimé).",
                node_title(state, book, "Classeur")
            ));
        });
        return;
    }
    let selected = state.selected_sheet_in(book).unwrap_or(sheets[0].0);

    ui.horizontal(|ui| {
        if ui.button("Ajouter une colonne").clicked() {
            actions.push(Action::AddColumn { sheet: selected });
        }
        if ui.button("Importer…").clicked() {
            state.open_import(book);
        }
        ui.separator();
        ui.weak(format!(
            "{} lignes × {} colonnes",
            state.row_count(selected),
            state.visible_columns(selected).len()
        ));
    });
    ui.separator();

    // La grille prend tout l'espace restant sauf la bande d'onglets du bas,
    // réservée manuellement : on n'est pas au niveau du `Context` ici (un
    // sous-`Ui` scopé à cet onglet de classeur), `egui::TopBottomPanel`
    // n'est donc pas utilisable.
    let grid_height = (ui.available_height() - SHEET_TABS_HEIGHT).max(0.0);
    ui.allocate_ui(egui::vec2(ui.available_width(), grid_height), |ui| {
        grid_contents(ui, state, actions, selected);
    });
    ui.separator();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (id, name) in &sheets {
            let tab = egui::Button::new(egui::RichText::new(name).size(12.0))
                .small()
                .fill(if selected == *id {
                    ui.visuals().selection.bg_fill
                } else {
                    egui::Color32::TRANSPARENT
                });
            if ui.add(tab).clicked() {
                actions.push(Action::SelectSheet(*id));
            }
        }
    });
}

/// Libellé affiché pour un [`Separator`] dans la fenêtre d'import.
fn separator_label(separator: Separator) -> &'static str {
    match separator {
        Separator::Comma => "Virgule",
        Separator::Semicolon => "Point-virgule",
        Separator::Tab => "Tabulation",
        Separator::Whitespace => "Espaces",
        Separator::Auto => "Auto",
    }
}

/// Libellé affiché pour un [`DecimalStyle`] dans la fenêtre d'import.
fn decimal_label(decimal: DecimalStyle) -> &'static str {
    match decimal {
        DecimalStyle::Dot => "Point",
        DecimalStyle::Comma => "Virgule",
    }
}

/// Une ligne « from … to … » d'assignation d'un rôle de sous-en-tête (voir
/// [`import_window`]) : une case pour activer/désactiver le rôle (`None` vs
/// `Some`), deux compteurs pour la plage. `id_source` distingue les trois
/// rôles (`long_name`/`unit`/`comment`) qui, sans lui, partageraient le même
/// id auto-généré (même position relative dans leurs branches `if`
/// respectives) et se marcheraient dessus.
fn subheader_role_row(
    ui: &mut egui::Ui,
    id_source: &str,
    label: &str,
    range: &mut Option<plotyx_import::SubheaderLineRange>,
) {
    ui.push_id(id_source, |ui| {
        ui.horizontal(|ui| {
            let mut enabled = range.is_some();
            if ui.checkbox(&mut enabled, label).changed() {
                *range = if enabled {
                    Some(plotyx_import::SubheaderLineRange::single(1))
                } else {
                    None
                };
            }
            if let Some(r) = range {
                ui.label("de");
                let mut from = r.from as i64;
                if ui
                    .add(egui::DragValue::new(&mut from).range(1..=i64::MAX))
                    .changed()
                {
                    r.from = from.max(1) as usize;
                }
                ui.label("à");
                let mut to = r.to as i64;
                if ui
                    .add(egui::DragValue::new(&mut to).range(1..=i64::MAX))
                    .changed()
                {
                    r.to = to.max(1) as usize;
                }
            }
        });
    });
}

/// Couleur associée à un [`plotyx_import::LineRole`] dans l'aperçu — bleu
/// pour un rôle d'en-tête reconnu, magenta pour un commentaire, orange pour
/// le pied de page, couleur de texte par défaut pour une donnée : même
/// esprit de code couleur que l'assistant d'import ASCII d'OriginLab
/// (§ légende de l'aperçu), sans reprendre ses teintes exactes.
fn line_role_color(ui: &egui::Ui, role: plotyx_import::LineRole) -> egui::Color32 {
    use plotyx_import::LineRole;
    match role {
        LineRole::MainHeader
        | LineRole::Subheader
        | LineRole::SubheaderLongName
        | LineRole::SubheaderUnit
        | LineRole::ColumnHeaderRow => egui::Color32::from_rgb(80, 140, 220),
        LineRole::SubheaderComment => egui::Color32::from_rgb(200, 80, 200),
        LineRole::Footer => ui.visuals().warn_fg_color,
        LineRole::Data | LineRole::Blank => ui.visuals().text_color(),
    }
}

/// L'aperçu en direct du découpage d'un fichier, une ligne par ligne — voir
/// [`import_window`] pour où il s'insère et `plotyx_import::preview` pour ce
/// qui borne son coût. Cellule par cellule seulement pour les lignes de
/// données (dans la fenêtre `preview_lines`) : une cellule qui deviendrait un
/// texte conservé dans une colonne mixte plutôt qu'un `NaN` muet est
/// annotée, pour que ce typage par cellule (chantier « colonnes à types
/// mixtes ») se voie ici plutôt que de surprendre après coup.
fn import_line_preview_widget(ui: &mut egui::Ui, preview: &plotyx_import::FilePreview) {
    let shown = preview
        .lines
        .iter()
        .filter(|l| l.role == plotyx_import::LineRole::Data)
        .count();
    if preview.total_data_lines > shown {
        ui.weak(format!(
            "{shown} ligne(s) de données montrée(s) sur {} au total.",
            preview.total_data_lines
        ));
    }
    egui::ScrollArea::vertical()
        .max_height(320.0)
        .show(ui, |ui| {
            for line in &preview.lines {
                ui.horizontal(|ui| {
                    let color = line_role_color(ui, line.role);
                    ui.colored_label(color, format!("{:>5} {:<2}", line.number, line.role.code()));
                    match &line.cells {
                        Some(cells) => {
                            for cell in cells {
                                let cell_color = if cell.is_text {
                                    ui.visuals().warn_fg_color
                                } else {
                                    ui.visuals().text_color()
                                };
                                ui.colored_label(cell_color, &cell.text);
                            }
                        }
                        None => {
                            ui.colored_label(color, &line.text);
                        }
                    }
                });
            }
        });
}

/// La fenêtre d'import (« Importer… », voir [`book_contents`]) : dialogue
/// transitoire (`egui::Window`, pas un onglet dockable), peinte depuis
/// [`PlotyxApp::ui`] tant que [`AppState::import_state`] est `Some`. Ferme sur
/// la croix native de la fenêtre exactement comme sur « Annuler » — les deux
/// abandonnent sans rien importer.
fn import_window(ctx: &egui::Context, state: &mut AppState, actions: &mut Vec<Action>) {
    if state.import_state().is_none() {
        return;
    }
    let mut still_open = true;
    // Ramassés pendant la fermeture pour être appliqués **après** `.show`,
    // jamais depuis l'intérieur des blocs ci-dessous qui emprûntent `import`
    // (`&mut`/`&` issus de `state`) : `save_import_recipe`/`cancel_import`
    // ont besoin de `&mut AppState` entier (pour `self.fail`/`self.notice`),
    // ce que le compilateur refuse tant qu'`import` est encore emprunté.
    let mut want_save = false;
    let mut want_cancel = false;
    let mut want_auto_detect = false;
    let mut pending_import: Option<(NodeId, plotyx_import::ImportRecipe, Vec<std::path::PathBuf>)> =
        None;
    egui::Window::new("Importer des fichiers")
        .collapsible(false)
        .resizable(true)
        .default_width(480.0)
        .open(&mut still_open)
        .show(ctx, |ui| {
            // -- Modèles enregistrés --------------------------------------
            let recipe_names: Vec<String> = state
                .import_state()
                .map(|import| {
                    import
                        .store
                        .recipes()
                        .iter()
                        .map(|r| r.name.clone())
                        .collect()
                })
                .unwrap_or_default();
            let current_name = state
                .import_state()
                .map(|import| import.recipe.name.clone())
                .unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label("Modèle :");
                egui::ComboBox::new("plotyx_import_recipe_combo", "")
                    .selected_text(current_name.clone())
                    .show_ui(ui, |ui| {
                        for name in &recipe_names {
                            if ui.selectable_label(*name == current_name, name).clicked() {
                                state.select_import_recipe(name);
                            }
                        }
                    });
                if ui.button("Nouveau modèle").clicked() {
                    state.new_import_recipe();
                }
                let can_delete =
                    !current_name.is_empty() && recipe_names.iter().any(|n| n == &current_name);
                if can_delete && ui.button("Supprimer ce modèle").clicked() {
                    state.delete_import_recipe(&current_name);
                }
            });
            ui.separator();

            // -- Champs de la recette, choix des fichiers -----------------
            // Emprunt `&mut` scopé à ce seul bloc : rien ici n'appelle
            // d'autre méthode de `state` que des mutations directes de
            // `import` — voir la note en tête de fonction.
            let Some(import) = state.import_state_mut() else {
                return;
            };
            ui.horizontal(|ui| {
                ui.label("Séparateur :");
                egui::ComboBox::new("plotyx_import_separator_combo", "")
                    .selected_text(separator_label(import.recipe.separator))
                    .show_ui(ui, |ui| {
                        for sep in [
                            Separator::Comma,
                            Separator::Semicolon,
                            Separator::Tab,
                            Separator::Whitespace,
                            Separator::Auto,
                        ] {
                            if ui
                                .selectable_label(
                                    import.recipe.separator == sep,
                                    separator_label(sep),
                                )
                                .clicked()
                            {
                                import.recipe.separator = sep;
                            }
                        }
                    });
                ui.label("Décimal :");
                egui::ComboBox::new("plotyx_import_decimal_combo", "")
                    .selected_text(decimal_label(import.recipe.decimal))
                    .show_ui(ui, |ui| {
                        for dec in [DecimalStyle::Dot, DecimalStyle::Comma] {
                            if ui
                                .selectable_label(import.recipe.decimal == dec, decimal_label(dec))
                                .clicked()
                            {
                                import.recipe.decimal = dec;
                            }
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.label("En-tête :");
                let mut is_none = matches!(import.recipe.header, HeaderSkip::None);
                let mut is_lines = matches!(import.recipe.header, HeaderSkip::Lines(_));
                let mut is_marker = matches!(import.recipe.header, HeaderSkip::UntilMarker(_));
                if ui.radio_value(&mut is_none, true, "Aucun").clicked() {
                    import.recipe.header = HeaderSkip::None;
                }
                if ui.radio_value(&mut is_lines, true, "N lignes").clicked() {
                    import.recipe.header = HeaderSkip::Lines(1);
                }
                if ui
                    .radio_value(&mut is_marker, true, "Jusqu'à un marqueur")
                    .clicked()
                {
                    import.recipe.header = HeaderSkip::UntilMarker(String::new());
                }
            });
            match &mut import.recipe.header {
                HeaderSkip::Lines(n) => {
                    ui.horizontal(|ui| {
                        ui.label("Lignes à ignorer :");
                        let mut value = *n as i64;
                        if ui
                            .add(egui::DragValue::new(&mut value).range(0..=i64::MAX))
                            .changed()
                        {
                            *n = value.max(0) as usize;
                        }
                    });
                }
                HeaderSkip::UntilMarker(marker) => {
                    ui.horizontal(|ui| {
                        ui.label("Marqueur :");
                        ui.text_edit_singleline(marker);
                    });
                }
                HeaderSkip::None => {}
            }

            ui.separator();
            // -- En-tête structuré (principal/sous-en-tête) + rôles -------
            // façon assistant d'import ASCII d'OriginLab — voir la doc de
            // tête de `plotyx_import::preview` et
            // `docs/chantier-import-previsualisation.md`. N'entre en jeu que
            // si l'utilisateur y touche (`main_header_lines`/
            // `subheader_lines` > 0) : réglé à zéro, l'« En-tête » classique
            // ci-dessus continue de tout piloter, sans régression pour une
            // recette qui ne s'en sert pas.
            ui.label("Découpage structuré (optionnel, façon assistant OriginLab) :");
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut import.recipe.header_layout.auto_detect,
                    "Détection automatique",
                );
                if ui
                    .add_enabled(
                        !import.paths.is_empty(),
                        egui::Button::new("Réévaluer maintenant"),
                    )
                    .clicked()
                {
                    want_auto_detect = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Lignes d'en-tête principal :");
                let mut value = import.recipe.header_layout.main_header_lines as i64;
                if ui
                    .add(egui::DragValue::new(&mut value).range(0..=i64::MAX))
                    .changed()
                {
                    import.recipe.header_layout.main_header_lines = value.max(0) as usize;
                }
                ui.label("Lignes de sous-en-tête :");
                let mut value = import.recipe.header_layout.subheader_lines as i64;
                if ui
                    .add(egui::DragValue::new(&mut value).range(0..=i64::MAX))
                    .changed()
                {
                    import.recipe.header_layout.subheader_lines = value.max(0) as usize;
                }
            });
            if import.recipe.header_layout.subheader_lines > 0 {
                ui.checkbox(
                    &mut import.recipe.subheader_roles.same_line_name_unit,
                    "Extraire noms longs et unités de la même ligne",
                );
                subheader_role_row(
                    ui,
                    "plotyx_import_role_long_name",
                    "Noms longs, lignes de sous-en-tête :",
                    &mut import.recipe.subheader_roles.long_name,
                );
                if !import.recipe.subheader_roles.same_line_name_unit {
                    subheader_role_row(
                        ui,
                        "plotyx_import_role_unit",
                        "Unités, lignes de sous-en-tête :",
                        &mut import.recipe.subheader_roles.unit,
                    );
                }
                subheader_role_row(
                    ui,
                    "plotyx_import_role_comment",
                    "Commentaires, lignes de sous-en-tête :",
                    &mut import.recipe.subheader_roles.comment,
                );
            }
            ui.horizontal(|ui| {
                ui.label("Caractères à retirer en tête de chaque ligne de données :");
                let mut value = import.recipe.skip_chars as i64;
                if ui
                    .add(egui::DragValue::new(&mut value).range(0..=i64::MAX))
                    .changed()
                {
                    import.recipe.skip_chars = value.max(0) as usize;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Pied de page (lignes à ignorer) :");
                let mut value = import.recipe.footer_skip_lines as i64;
                if ui
                    .add(egui::DragValue::new(&mut value).range(0..=i64::MAX))
                    .changed()
                {
                    import.recipe.footer_skip_lines = value.max(0) as usize;
                }
            });

            ui.checkbox(
                &mut import.recipe.has_column_header_row,
                "La première ligne restante donne les noms de colonnes",
            );

            ui.horizontal(|ui| {
                ui.label("Nom des séries :");
                let mut is_filename = matches!(import.recipe.name_source, NameSource::Filename);
                let mut is_header_key =
                    matches!(import.recipe.name_source, NameSource::HeaderKey(_));
                if ui
                    .radio_value(&mut is_filename, true, "Nom de fichier")
                    .clicked()
                {
                    import.recipe.name_source = NameSource::Filename;
                }
                if ui
                    .radio_value(&mut is_header_key, true, "Clé d'en-tête")
                    .clicked()
                {
                    import.recipe.name_source = NameSource::HeaderKey(String::new());
                }
            });
            if let NameSource::HeaderKey(key) = &mut import.recipe.name_source {
                ui.horizontal(|ui| {
                    ui.label("Clé :");
                    ui.text_edit_singleline(key);
                });
            }

            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Nom du modèle :");
                ui.text_edit_singleline(&mut import.save_as_name);
                if ui.button("Enregistrer le modèle").clicked() {
                    want_save = true;
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Choisir des fichiers…").clicked() {
                    if let Some(paths) = rfd::FileDialog::new()
                        .set_title("Choisir des fichiers à importer")
                        .pick_files()
                    {
                        import.paths = paths;
                    }
                }
                ui.weak(if import.paths.is_empty() {
                    "Aucun fichier choisi".to_string()
                } else {
                    import
                        .paths
                        .iter()
                        .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
                        .collect::<Vec<_>>()
                        .join(", ")
                });
            });
            // Fin du bloc emprûntant `import` : nécessaire pour que
            // `state.import_preview()` ci-dessous (qui reprend `&AppState`)
            // et le second bloc plus bas (qui reprend `import` pour lire
            // seulement, voir sa note) ne se heurtent pas à l'emprunt
            // encore actif ici.

            ui.separator();
            // -- Aperçu en direct du découpage (premier fichier choisi) ---
            // Le point qui compte de ce chantier : chaque changement de
            // réglage ci-dessus retexte cet aperçu immédiatement, sans étape
            // de validation — voir la doc de tête de
            // `plotyx_import::preview` pour ce qui garde ça praticable sur
            // un gros fichier. Remplace l'ancien tableau de valeurs
            // numériques (dix premières lignes, aucune indication de
            // découpage) qui ne montrait jamais l'en-tête.
            ui.horizontal(|ui| {
                ui.label("Lignes d'aperçu :");
                let mut value = import.recipe.preview_lines as i64;
                if ui
                    .add(egui::DragValue::new(&mut value).range(1..=10_000))
                    .changed()
                {
                    import.recipe.preview_lines = value.max(1) as usize;
                }
            });
            ui.weak(
                "Codes : MH = en-tête principal, SH = sous-en-tête, \
                 L = nom long, U = unité, C = commentaire, S = noms courts, \
                 F = pied de page.",
            );
            // Fin du bloc emprûntant `import` : nécessaire pour que
            // `state.import_line_preview()` ci-dessous (qui reprend
            // `&AppState`) et le second bloc plus bas ne se heurtent pas à
            // l'emprunt encore actif ici.
            match state.import_line_preview() {
                None => {
                    ui.weak("Choisis un fichier pour voir un aperçu.");
                }
                Some(Err(message)) => {
                    ui.colored_label(egui::Color32::RED, message);
                }
                Some(Ok(file_preview)) => {
                    import_line_preview_widget(ui, &file_preview);
                }
            }

            ui.separator();
            // Second bloc, emprunt en lecture seule cette fois : juste
            // besoin de lire `book`/`recipe`/`paths` pour composer
            // l'`Action`, jamais de les muter.
            let Some(import) = state.import_state() else {
                return;
            };
            let has_files = !import.paths.is_empty();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(has_files, egui::Button::new("Importer"))
                    .clicked()
                {
                    pending_import =
                        Some((import.book, import.recipe.clone(), import.paths.clone()));
                }
                if ui.button("Annuler").clicked() {
                    want_cancel = true;
                }
            });
        });

    // Tout ce qui devait modifier `state` en entier (pas seulement
    // `import`) est appliqué ici, une fois `.show` terminé et son emprunt
    // relâché — voir la note en tête de fonction.
    if want_save {
        state.save_import_recipe();
    }
    if want_auto_detect {
        state.auto_detect_import_header();
    }
    if let Some((book, recipe, paths)) = pending_import {
        actions.push(Action::ImportFiles {
            book,
            recipe,
            paths,
        });
        state.cancel_import();
    } else if want_cancel || !still_open {
        state.cancel_import();
    }
}

/// La grille, en lignes **virtualisées** : seules les lignes visibles sont
/// construites, sans quoi une feuille d'un million de lignes bloquerait
/// chaque image. Scopée à `sheet` — voir [`book_contents`], son seul
/// appelant.
///
/// En-tête empilée (lettre A/B/C, nom long, unité, commentaire, formule —
/// voir [`column_header_widget`]), sélection de cellules/colonnes à la
/// souris (voir [`cell_widget`]), et raccourcis Ctrl+C/X/V/Suppr/Retour
/// arrière scopés à cette grille : ils ne sont consommés que si aucune
/// cellule ni aucun champ d'en-tête n'est en édition, **ni la fenêtre
/// d'import ouverte**, pour ne jamais voler le copier/coller natif d'un
/// `TextEdit` en cours de saisie — la fenêtre d'import est un `egui::Window`
/// flottant peint par-dessus cette grille, toujours présente dessous tant
/// qu'il est ouvert (voir la garde plus bas, dans le corps de la fonction).
fn grid_contents(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
) {
    let columns = state.visible_columns(sheet);
    let row_count = state.row_count(sheet);
    if columns.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.weak("Aucune colonne dans cette feuille.");
        });
        return;
    }
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(TableColumn::initial(ROW_NUMBER_COLUMN_WIDTH).at_least(MIN_ROW_NUMBER_COLUMN_WIDTH))
        .columns(
            TableColumn::initial(DEFAULT_COLUMN_WIDTH).at_least(MIN_COLUMN_WIDTH),
            columns.len(),
        )
        .min_scrolled_height(0.0)
        // B3 — la grille doit occuper tout l'espace vertical réservé par
        // [`book_contents`] (`grid_height`), pas seulement celui que son
        // contenu requiert : sans quoi `Ui::allocate_ui` (qui n'alloue au
        // parent que l'espace réellement utilisé par l'enfant) laisse un
        // vide entre la grille et le bas du panneau, et les onglets de
        // feuille se retrouvent collés sous une grille trop courte plutôt
        // que plaqués en bas de la fenêtre.
        .auto_shrink([false, false])
        .header(HEADER_HEIGHT_STACKED, |mut header| {
            header.col(|ui| {
                row_number_header_widget(ui);
            });
            for (index, (id, _, column_type)) in columns.iter().enumerate() {
                header.col(|ui| {
                    column_header_widget(ui, state, actions, sheet, *id, index, *column_type);
                });
            }
        })
        .body(|body| {
            body.rows(ROW_HEIGHT, row_count, |mut row| {
                let index = row.index();
                row.col(|ui| {
                    ui.weak(format!("{}", index + 1));
                });
                for (id, _, _) in &columns {
                    row.col(|ui| {
                        cell_widget(ui, state, actions, sheet, *id, index);
                    });
                }
            });
        });

    // Raccourcis clavier de la grille — voir la doc de tête de cette
    // fonction pour la garde d'édition.
    //
    // `state.import_state().is_none()` est la troisième condition de cette
    // garde, ajoutée avec la refonte de la fenêtre d'import : `consume_key`
    // ci-dessous agit sur l'`InputState` du `Context` entier, pas sur celui
    // d'une `Ui` — donc pas seulement sur cette grille. Tant que la fenêtre
    // d'import (un `egui::Window` flottant, peint *après* le dock — voir
    // `PlotyxApp::update`) est ouverte, cette grille continue d'être peinte
    // dessous à chaque image ; sans cette condition, Retour arrière/Suppr
    // tapés dans un champ texte de la fenêtre d'import étaient retirés d'ici
    // avant même que ce champ ait pu les voir : plus aucune suppression de
    // caractère possible sans passer par une sélection (qui, elle, remplace
    // le texte sélectionné via l'insertion d'`Event::Text`, jamais
    // consommée ici) — c'était le bug connu du champ de texte du module
    // d'import, corrigé ici plutôt que dans la fenêtre elle-même parce que
    // c'est ici qu'il naissait.
    if state.editing().is_none()
        && state.header_editing().is_none()
        && state.import_state().is_none()
    {
        // Taper sur une cellule ou un champ d'en-tête sélectionné (simple
        // clic, pas encore en édition) démarre l'édition en remplaçant tout
        // son contenu par ce qui est tapé — comportement standard de
        // tableur, le même pour les deux. Ne se déclenche que pour une
        // sélection réduite à une seule cellule ([`Selection::single_cell`])
        // côté données : taper sur une plage ne doit jamais écraser
        // plusieurs cellules à la fois — un champ d'en-tête, lui, n'est
        // jamais une plage. `egui::Event::Text` ne porte que du texte
        // imprimable — jamais une touche de contrôle ou de navigation, qui
        // n'apparaît que dans `events` comme un `Event::Key` — donc pas de
        // filtrage supplémentaire à faire ici.
        let typed: String = ui.input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect()
        });
        if !typed.is_empty() {
            if let Some((column, row)) = state.selection(sheet).and_then(Selection::single_cell) {
                state.begin_edit(column, row);
                if let Some(edit) = state.editing_mut() {
                    edit.buffer = typed;
                }
            } else if let Some((column, field)) = state.selected_header_field(sheet) {
                state.begin_header_edit(column, field);
                if let Some(edit) = state.header_editing_mut() {
                    edit.buffer = typed;
                }
            }
        }
        // Déplacement clavier d'une cellule déjà sélectionnée, sans
        // édition en cours (pendant l'édition, ces mêmes touches sont
        // gérées par `cell_widget`, qui valide *puis* déplace : voir sa
        // doc). Flèches directionnelles + Maj pour étendre le rectangle ;
        // Tab/Maj+Tab avec retour à la ligne aux bords ; Entrée/Maj+Entrée
        // comme équivalents non-éditants de Bas/Haut. Sans effet si la
        // sélection courante n'est pas un rectangle de cellules (colonne
        // entière ou champ d'en-tête) — voir `Selection::cursor`.
        let shift = ui.input(|i| i.modifiers.shift);
        let (up, down, left, right, tab, enter) = ui.input(|input| {
            (
                input.key_pressed(egui::Key::ArrowUp),
                input.key_pressed(egui::Key::ArrowDown),
                input.key_pressed(egui::Key::ArrowLeft),
                input.key_pressed(egui::Key::ArrowRight),
                input.key_pressed(egui::Key::Tab),
                input.key_pressed(egui::Key::Enter),
            )
        });
        if up {
            state.move_selection(sheet, MoveDirection::Up, shift);
        }
        if down {
            state.move_selection(sheet, MoveDirection::Down, shift);
        }
        if left {
            state.move_selection(sheet, MoveDirection::Left, shift);
        }
        if right {
            state.move_selection(sheet, MoveDirection::Right, shift);
        }
        if tab {
            state.move_selection_tab(sheet, shift);
        }
        if enter && shift {
            state.move_selection(sheet, MoveDirection::Up, false);
        } else if enter {
            // Entrée sur la dernière ligne de la feuille, cellule déjà
            // vide et jamais éditée dans cette frappe : `move_selection`
            // seul ne ferait rien (`Down` est borné à `row_count - 1`,
            // voir sa doc). Même geste que `CommitCellAndAdvance` sur une
            // cellule qu'on vient de remplir — ajoute la ligne, puis
            // sélectionne tout de suite la cellule qui y correspondra
            // (elle sera créée avant le prochain rendu, voir `cell_widget`).
            let at_last_row = state
                .selection(sheet)
                .and_then(Selection::cursor)
                .is_some_and(|(_, row)| row + 1 >= state.row_count(sheet));
            if at_last_row {
                if let Some((column, row)) = state.selection(sheet).and_then(Selection::cursor) {
                    actions.push(Action::InsertRows {
                        sheet,
                        at: row + 1,
                        count: 1,
                    });
                    state.select_cell(sheet, column, row + 1);
                }
            } else {
                state.move_selection(sheet, MoveDirection::Down, false);
            }
        }
        // Ctrl+C/X/V n'apparaissent **jamais** comme un `egui::Event::Key`
        // ordinaire (donc jamais via `consume_key`) : `egui-winit` les
        // intercepte lui-même à la traduction des événements de la
        // fenêtre et pousse à la place un `Event::Copy`/`Event::Cut`, ou un
        // `Event::Paste(String)` qui porte déjà le contenu du presse-papier
        // système (lu par `egui-winit`, via `arboard` en interne — inutile
        // de le relire nous-mêmes). Retour arrière/Suppr, eux, n'ont pas de
        // modificateur et restent des `Event::Key` ordinaires, capturés via
        // `consume_key` comme avant.
        let (mut copy, mut cut, mut paste_text) = (false, false, None);
        ui.input(|input| {
            for event in &input.events {
                match event {
                    egui::Event::Copy => copy = true,
                    egui::Event::Cut => cut = true,
                    egui::Event::Paste(text) => paste_text = Some(text.clone()),
                    _ => {}
                }
            }
        });
        let (delete, backspace) = ui.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::NONE, egui::Key::Delete),
                input.consume_key(egui::Modifiers::NONE, egui::Key::Backspace),
            )
        });
        if let Some((column, field)) = state.selected_header_field(sheet) {
            // Un champ d'en-tête sélectionné détourne les mêmes raccourcis
            // vers lui plutôt que vers la sélection de cellules — les deux
            // sont mutuellement exclusives (une seule `Selection` par
            // feuille).
            if copy || cut {
                ui.ctx().copy_text(header_field_text(state, column, field));
            }
            if cut || delete || backspace {
                actions.push(header_field_action(column, field, String::new()));
            }
            if let Some(text) = paste_text {
                actions.push(header_field_action(column, field, text));
            }
        } else {
            if copy || cut {
                if let Some(tsv) = state.selected_cells_tsv(sheet) {
                    ui.ctx().copy_text(tsv);
                }
            }
            if cut || backspace {
                actions.push(Action::ClearSelectedCells { sheet });
            }
            if let Some(text) = paste_text {
                actions.push(Action::PasteTsv { sheet, text });
            }
            if delete {
                actions.push(Action::DeleteSelectedCells { sheet });
            }
        }
    }
}

/// Traduit une valeur de champ d'en-tête en l'[`Action`] qui l'écrit —
/// partagé par le raccourci clavier (Ctrl+X/Suppr/Retour arrière effacent
/// via `text: String::new()`, Ctrl+V colle le texte du presse-papier tel
/// quel) et, indirectement, par [`AppState::commit_header_edit`] côté
/// double-clic. Une formule vide (après `trim`) redevient `None` plutôt
/// qu'une chaîne vide, comme `commit_header_edit`.
fn header_field_action(column: NodeId, field: HeaderField, text: String) -> Action {
    match field {
        HeaderField::LongName => Action::SetColumnLongName { column, text },
        HeaderField::Unit => Action::SetColumnUnit { column, text },
        HeaderField::Comment => Action::SetColumnComment { column, text },
        HeaderField::Formula => Action::SetColumnFormula {
            column,
            formula: (!text.trim().is_empty()).then_some(text),
        },
    }
}

/// Console de script : zone de saisie, boutons « Exécuter »/« Arrêter »,
/// sortie. Contenu de [`PlotyxApp::script_panel`].
///
/// Ne bloque jamais la fenêtre : voir la doc de tête de ce module.
/// `Action::RunScript`/`InterruptScript` sont poussées comme n'importe
/// quelle autre action, appliquées après le rendu par la boucle
/// principale — c'est [`AppState::dispatch`] qui garantit qu'elles ne
/// bloquent pas.
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

    ui.horizontal(|ui| {
        ui.heading("Script Python");
        if !state.script_available() {
            ui.weak(
                "— compilé sans le pont Python : la surface pyx n'est pas disponible dans ce binaire.",
            );
        }
    });
    ui.separator();

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
                egui::Button::new("Exécuter")
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_RUN_SCRIPT)),
            )
            .clicked()
        {
            actions.push(Action::RunScript);
        }
        if ui
            .add_enabled(running, egui::Button::new("Arrêter"))
            .clicked()
        {
            actions.push(Action::InterruptScript);
        }
        if running {
            ui.spinner();
            ui.weak("exécution en cours…");
        }
    });

    ui.separator();
    egui::ScrollArea::vertical()
        .max_height(SCRIPT_OUTPUT_HEIGHT)
        .show(ui, |ui| match state.script_output() {
            None => {
                ui.weak("Aucune exécution pour l'instant.");
            }
            Some(output) => {
                let mut empty = true;
                if !output.stdout.is_empty() {
                    empty = false;
                    ui.label("Sortie standard :");
                    ui.monospace(&output.stdout);
                }
                if !output.stderr.is_empty() {
                    empty = false;
                    ui.label("Sortie d'erreur :");
                    ui.monospace(&output.stderr);
                }
                if let Some(error) = &output.error {
                    empty = false;
                    ui.colored_label(ui.visuals().warn_fg_color, "Trace d'appels :");
                    ui.monospace(error);
                }
                if empty {
                    ui.weak("(sortie vide)");
                }
            }
        });
}

/// Un graphe ouvert — le **rendu visible** de la phase R4 (tâche 4.6).
/// Contenu d'un [`Tab::Graph`].
///
/// Ce n'est pas une ébauche de la fenêtre de graphe finale : comme tout ce
/// banc, c'est la preuve que l'intégration bout en bout tient. Ce qu'il
/// montre vraiment, et qu'aucun test ne montre :
///
/// - la figure elle-même, avec ses vraies polices et son vrai HiDPI ;
/// - le compteur de cache, qui rend visible la tâche R4.3. Au repos,
///   `succès` monte et `calculs` ne bouge plus : une image sans changement
///   ne rejoue pas le pipeline Plotters. Redimensionner l'onglet, ou
///   modifier une cellule, fait repartir `calculs` d'un cran — c'est la clé
///   `(revision, rect, pixels_per_point)` au travail.
fn graph_contents(
    ui: &mut egui::Ui,
    state: &AppState,
    graph_views: &mut HashMap<NodeId, GraphView>,
    graph_id: NodeId,
) {
    let view = graph_views.entry(graph_id).or_default();
    graph_view_contents(ui, view, state.snapshot(), graph_id);
}

/// Corps de [`graph_contents`], factorisé sur une [`GraphView`] et un
/// [`Snapshot`] déjà en main plutôt que sur `&AppState`/la table
/// [`PlotyxApp::graph_views`] : c'est ce qui permet à
/// [`PlotyxApp::show_detached_tabs`] de peindre un `Tab::Graph` détaché avec
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
) {
    let available = ui.available_size_before_wrap();
    // R7 : la navigation (molette, glissé, rectangle élastique) a besoin de
    // bien plus que `hover()` — `GraphView::paint` fait sa propre
    // `ui.interact` sur ce rectangle avec `click_and_drag()` ; le `Sense`
    // posé ici doit au moins ne pas lui faire concurrence.
    let (outer, _) = ui.allocate_exact_size(
        egui::vec2(available.x, (available.y - 24.0).max(120.0)),
        egui::Sense::click_and_drag(),
    );
    // Taille de figure fixe (`DEFAULT_FIGURE_SIZE`), centrée dans l'espace
    // disponible plutôt qu'élastique à l'onglet : le graphe ne suit pas le
    // redimensionnement de la fenêtre, voir la doc de tête de
    // `plotyx_render::pipeline::DEFAULT_FIGURE_SIZE`.
    let (fig_w, fig_h) = DEFAULT_FIGURE_SIZE;
    let rect = egui::Rect::from_center_size(outer.center(), egui::vec2(fig_w as f32, fig_h as f32));
    match view.paint(ui, rect, snapshot, graph_id) {
        Ok(_) => {
            let cache = view.cache();
            ui.weak(format!(
                "cache : {} succès / {} calculs — {} formes",
                cache.hits(),
                cache.misses(),
                cache.shapes().len()
            ));
            if let Some(drawn) = view.last_drawn() {
                ui.weak(figure_summary(drawn));
            }
            if let Some(hit) = view.hovered_point() {
                ui.weak(format!(
                    "point désigné : panneau {}, série {}, indice {} — ({:.4}, {:.4}), à {:.1} px",
                    hit.panel, hit.series, hit.point_index, hit.data.0, hit.data.1, hit.distance
                ));
            }
            if ui.small_button("réinitialiser le zoom").clicked() {
                view.reset_view();
            }
        }
        Err(err) => {
            ui.colored_label(egui::Color32::RED, format!("rendu : {err}"));
        }
    }
}

/// Contenu d'un [`Tab::Matrix`] — **espace réservé documenté**. L'édition de
/// matrice est explicitement hors périmètre de cette tâche
/// (`docs/application.md`) : la fenêtre s'ouvre et se focalise exactement
/// comme les trois autres natures, mais ne montre qu'un message plutôt qu'un
/// éditeur. `plotyx-core::NodeKind::Matrix` n'a aujourd'hui aucun contenu
/// (juste des métadonnées de fenêtre) — il n'y a donc littéralement rien
/// d'autre à afficher tant que cette nature n'a pas de valeurs.
fn matrix_contents(ui: &mut egui::Ui, state: &AppState, matrix: NodeId) {
    let name = node_title(state, matrix, "Matrice");
    ui.centered_and_justified(|ui| {
        ui.weak(format!(
            "« {name} » — édition de matrice non implémentée dans ce banc \
             (hors périmètre ; `plotyx-core` n'expose pas encore de contenu \
             de matrice à éditer)."
        ));
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
            ui.weak(format!(
                "« {} » a été supprimée.",
                node_title(state, note, "Note")
            ));
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

/// Ce que la figure a réellement tracé, en une ligne.
///
/// Ajouté en R5 : avec les panneaux multiples, les types de tracé et la
/// décimation, « la figure s'affiche » ne dit plus grand-chose. Les quatre
/// compteurs du `DrawnPlot` disent lesquels des chemins de la phase ont été
/// empruntés — et c'est le seul endroit où on peut les lire sur des données
/// réelles plutôt que sur une fixture de test.
fn figure_summary(drawn: &DrawnPlot) -> String {
    let mut ligne = format!(
        "figure : {} panneau(x), {} série(s) tracée(s)",
        drawn.panels.len(),
        drawn.drawn_series
    );
    if drawn.broken_series > 0 {
        ligne.push_str(&format!(
            " — {} référence(s) brisée(s)",
            drawn.broken_series
        ));
    }
    if drawn.unknown_series > 0 {
        ligne.push_str(&format!(
            " — {} type(s) de tracé inconnu(s)",
            drawn.unknown_series
        ));
    }
    if drawn.decimated_points > 0 {
        ligne.push_str(&format!(" — {} points décimés", drawn.decimated_points));
    }
    ligne
}

/// Peint une cellule et récolte les actions qu'elle produit.
///
/// Une seule cellule à la fois porte un vrai tampon (celui de
/// [`AppState::editing`]) ; toutes les autres sont peintes comme un `Label`
/// surligné si sélectionné, sans jamais accaparer le focus clavier — ce qui
/// garde le coût mémoire de la grille indépendant du nombre de lignes, et
/// laisse le clic simple à la sélection plutôt qu'à l'édition.
///
/// - Simple clic (ou glisser à partir d'ici) : sélectionne cette cellule,
///   ancre d'un futur rectangle.
/// - Shift-clic, ou survol pendant qu'un glisser est en cours ailleurs dans
///   la grille : étend la sélection jusqu'ici.
/// - Double-clic : ouvre l'édition.
///
/// Le contenu n'est envoyé au modèle qu'à la **perte de focus** de l'édition
/// — ce que provoque aussi la touche Entrée — jamais à chaque frappe : sinon
/// l'historique compterait une entrée par caractère. Perdre le focus par
/// Entrée pousse [`Action::CommitCellAndAdvance`] plutôt que [`Action::
/// CommitCell`] : c'est elle qui ajoute une ligne quand on était sur la
/// dernière de la colonne.
fn cell_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    column: NodeId,
    row: usize,
) {
    // L'identifiant est ancré sur (colonne, ligne) et non sur la position à
    // l'écran : avec des lignes virtualisées, faire défiler déplacerait sinon
    // le focus d'une cellule à l'autre.
    ui.push_id((column.raw(), row), |ui| {
        let editing_here = state
            .editing()
            .is_some_and(|edit| edit.column == column && edit.row == row);

        if editing_here {
            let Some(edit) = state.editing_mut() else {
                return;
            };
            let response = ui.add(
                egui::TextEdit::singleline(&mut edit.buffer)
                    .frame(egui::Frame::NONE)
                    .desired_width(f32::INFINITY),
            );
            // `Response::has_focus`/`lost_focus` sont recalculés à chaque
            // appel à partir de l'état *courant* du focus (pas un instantané
            // figé) : il faut donc lire `lost_focus()` **avant** de
            // potentiellement redemander le focus, sous peine de se
            // redonner nous-mêmes le focus qu'on vient de perdre (Entrée
            // fait perdre le focus au `TextEdit` en interne) et de faire
            // rentrer `lost_focus()` à `false` — plus aucune validation
            // n'aurait alors lieu, la cellule restant bloquée en édition et,
            // avec elle, `AppState::editing()` bloquant à son tour tous les
            // raccourcis clavier de la grille (voir la garde en tête de
            // `grid_contents`).
            let lost_focus = response.lost_focus();
            // Sans ceci, un `TextEdit` qui vient d'apparaître (édition
            // ouverte par double-clic ou, depuis le bug 1, par une frappe
            // sur une cellule sélectionnée) ne reçoit le focus clavier
            // qu'après un clic dessus — egui ne le lui donne pas
            // automatiquement à l'apparition.
            if !response.has_focus() && !lost_focus {
                response.request_focus();
            }
            if !lost_focus {
                return;
            }
            // Échap referme la saisie sans rien écrire ; toute autre perte
            // de focus (clic ailleurs, Tab, Entrée) valide — et, pour
            // Entrée/Maj+Entrée/Tab/Maj+Tab, déplace aussi la sélection vers
            // la cellule suivante, comme dans tout tableur (déplacement
            // clavier complet : voir le bloc de raccourcis hors édition dans
            // `grid_contents` pour les flèches directionnelles, qui ne
            // s'appliquent qu'à une cellule déjà sélectionnée mais pas en
            // édition — `TextEdit` capture lui-même les flèches pour
            // déplacer le curseur de texte pendant la saisie).
            if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                state.cancel_edit();
                return;
            }
            let shift = ui.input(|i| i.modifiers.shift);
            // `consume_key`, pas `key_pressed` : cette cellule valide *et*
            // déplace la sélection tout de suite (`select_cell`/
            // `move_selection*` ci-dessous), avant que le bloc de
            // raccourcis clavier de `grid_contents` (qui tourne après, une
            // fois `state.editing()` redevenu `None` par le `cancel_edit`
            // ci-dessous) ne voie passer la même image. Sans consommer la
            // touche ici, ce bloc plus tard dans la même image relisait le
            // même Entrée/Tab (`key_pressed` ne retire rien des
            // événements) et déplaçait la sélection une deuxième fois —
            // Entrée sur la ligne 2 atterrissait sur la ligne 4, pas 3.
            // `Modifiers::NONE` matche Entrée avec ou sans Maj
            // (`Modifiers::matches_logically` ignore Maj/Alt en trop quand
            // le motif ne les demande pas).
            let enter =
                ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            let tab =
                ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
            let pending = state
                .editing()
                .filter(|edit| edit.column == column && edit.row == row)
                .map(|edit| edit.buffer.clone());
            if let Some(text) = pending {
                state.cancel_edit();
                if enter && !shift {
                    // `CommitCellAndAdvance` ajoute déjà une ligne quand
                    // `row` était la dernière de la colonne ; sélectionner
                    // `row + 1` sans attendre que cette commande (appliquée
                    // après le rendu de cette image) ait créé la ligne est
                    // sûr : soit elle existait déjà, soit elle est sur le
                    // point de l'être.
                    actions.push(Action::CommitCellAndAdvance {
                        column,
                        row,
                        text,
                        sheet,
                    });
                    state.select_cell(sheet, column, row + 1);
                } else {
                    actions.push(Action::CommitCell { column, row, text });
                    if enter {
                        // Maj+Entrée : remonte d'une ligne, jamais d'ajout.
                        state.move_selection(sheet, MoveDirection::Up, false);
                    } else if tab {
                        state.move_selection_tab(sheet, shift);
                    }
                }
            }
            return;
        }

        let text = state.cell_text(column, row);
        let selected = state.is_selected(sheet, column, row);
        let rect = ui.available_rect_before_wrap();
        let response = ui.interact(rect, ui.id().with("cell"), egui::Sense::click_and_drag());
        if selected {
            ui.painter().rect_filled(
                rect,
                0.0,
                ui.visuals().selection.bg_fill.linear_multiply(0.35),
            );
        }
        // `.truncate()` plutôt que le retour à la ligne par défaut d'un
        // `Label` : une valeur trop large pour sa colonne est coupée à
        // droite (avec un léger fondu), jamais renvoyée sur une seconde
        // ligne qui casserait la hauteur uniforme des rangées de la table.
        ui.put(rect, egui::Label::new(text).selectable(false).truncate());

        if response.double_clicked() {
            state.begin_edit(column, row);
            return;
        }
        let shift = ui.input(|i| i.modifiers.shift);
        if response.drag_started() || (response.clicked() && !shift) {
            state.select_cell(sheet, column, row);
        } else if response.clicked() && shift {
            state.extend_selection(sheet, column, row);
        } else if response.contains_pointer()
            && ui.input(|i| i.pointer.primary_down())
            && state.selection(sheet).is_some()
        {
            // `response.dragged()` ne se déclenche que sur le widget qui a
            // *commencé* le glisser — jamais sur les cellules simplement
            // survolées ensuite —, d'où ce test manuel plutôt que
            // `dragged()` pour étendre la sélection pendant qu'on glisse.
            //
            // `response.hovered()` ne convient pas non plus : tant qu'un
            // glisser est en cours *ailleurs* (sur la cellule de départ),
            // egui ne considère plus que ce widget d'origine comme
            // « survolé » — aucune autre cellule ne redevient `hovered()`
            // pendant tout le glisser, même sous le curseur (`egui::
            // Context::interact`, doc de `InteractionSnapshot::hovered` :
            // « While dragging or clicking something, nothing else is
            // hovered »). `contains_pointer()` reste vrai indépendamment de
            // ce qui est en train d'être glissé ailleurs — c'est le test
            // qu'il faut ici.
            state.extend_selection(sheet, column, row);
        }
    });
}

/// Contenu d'un `Tab::Book` **détaché** : le pendant de [`book_contents`],
/// opérant sur un `&Tree` (venant de `DetachedTabViewModel::snapshot`) et sur
/// `DetachedTabViewModel::selected_sheet`/`cell_edit` plutôt que sur
/// `&mut AppState` — voir la doc de tête du module pour pourquoi ce n'est
/// pas littéralement `book_contents`. Mêmes gestes, même
/// `Action::AddColumn`/`InsertRows`/`DeleteRows`/`CommitCell` poussés dans
/// `requests.actions` plutôt que dans un `&mut Vec<Action>` direct.
fn detached_book_contents(
    ui: &mut egui::Ui,
    tree: &Tree,
    model: &mut DetachedTabViewModel,
    requests: &mut DetachedTabRequests,
    book: NodeId,
) {
    let sheets = worksheets_of_in(tree, book);
    if sheets.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.weak(format!(
                "« {} » n'a aucune feuille (ou a été supprimé).",
                node_title_in(tree, book, "Classeur")
            ));
        });
        return;
    }
    let selected = model
        .selected_sheet
        .filter(|id| sheets.iter().any(|(sheet_id, _)| sheet_id == id))
        .unwrap_or(sheets[0].0);
    model.selected_sheet = Some(selected);

    ui.horizontal(|ui| {
        for (id, name) in &sheets {
            if ui.selectable_label(selected == *id, name).clicked() {
                model.selected_sheet = Some(*id);
                requests.actions.push(Action::SelectSheet(*id));
            }
        }
    });
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Ajouter une colonne").clicked() {
            requests.actions.push(Action::AddColumn { sheet: selected });
        }
        if ui.button("Insérer 1 ligne").clicked() {
            // Même correctif B5 que `book_contents` : insérer en fin de
            // feuille, pas `at: 0` en dur (`Project::insert_rows` exige
            // `at < len`, une colonne neuve a `len == 0`).
            requests.actions.push(Action::InsertRows {
                sheet: selected,
                at: row_count_in(tree, selected),
                count: 1,
            });
        }
        if ui.button("Supprimer 1 ligne").clicked() {
            requests.actions.push(Action::DeleteRows {
                sheet: selected,
                at: 0,
                count: 1,
            });
        }
        ui.separator();
        ui.weak(format!(
            "{} lignes × {} colonnes",
            row_count_in(tree, selected),
            visible_columns_in(tree, selected).len()
        ));
    });
    ui.separator();
    detached_grid_contents(ui, tree, model, requests, selected);
}

/// Pendant détaché de [`grid_contents`] — mêmes principes (virtualisation),
/// sur `&Tree`/`DetachedTabViewModel` plutôt que `&mut AppState`.
fn detached_grid_contents(
    ui: &mut egui::Ui,
    tree: &Tree,
    model: &mut DetachedTabViewModel,
    requests: &mut DetachedTabRequests,
    sheet: NodeId,
) {
    let columns = visible_columns_in(tree, sheet);
    let row_count = row_count_in(tree, sheet);
    if columns.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.weak("Aucune colonne dans cette feuille.");
        });
        return;
    }
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(TableColumn::exact(56.0))
        .columns(TableColumn::initial(110.0).at_least(56.0), columns.len())
        .min_scrolled_height(0.0)
        .header(HEADER_HEIGHT, |mut header| {
            header.col(|ui| {
                ui.strong("#");
            });
            for (_, name, column_type) in &columns {
                header.col(|ui| {
                    ui.strong(name);
                    ui.weak(type_suffix(*column_type));
                });
            }
        })
        .body(|body| {
            body.rows(ROW_HEIGHT, row_count, |mut row| {
                let index = row.index();
                row.col(|ui| {
                    ui.weak(format!("{}", index + 1));
                });
                for (id, _, _) in &columns {
                    row.col(|ui| {
                        detached_cell_widget(ui, tree, &mut model.cell_edit, requests, *id, index);
                    });
                }
            });
        });
}

/// Pendant détaché de [`cell_widget`] — même machine à états
/// (`gained_focus`/`lost_focus`/Échap), sur `cell_edit: &mut Option<CellEdit>`
/// propre à cette fenêtre plutôt que sur `AppState::editing`. Voir la doc de
/// tête du module, § « la lecture du modèle est factorisée, l'édition
/// transitoire ne l'est pas », pour pourquoi ce doublon minime est assumé
/// plutôt qu'abstrait.
fn detached_cell_widget(
    ui: &mut egui::Ui,
    tree: &Tree,
    cell_edit: &mut Option<CellEdit>,
    requests: &mut DetachedTabRequests,
    column: NodeId,
    row: usize,
) {
    let inner = ui.push_id((column.raw(), row), |ui| {
        let editing_here = cell_edit
            .as_ref()
            .is_some_and(|edit| edit.column == column && edit.row == row);

        if editing_here {
            let edit = cell_edit.as_mut()?;
            let response = ui.add(
                egui::TextEdit::singleline(&mut edit.buffer)
                    .frame(egui::Frame::NONE)
                    .desired_width(f32::INFINITY),
            );
            Some((response, None))
        } else {
            let original = cell_text_in(tree, column, row);
            let mut shown = original.clone();
            let response = ui.add(
                egui::TextEdit::singleline(&mut shown)
                    .frame(egui::Frame::NONE)
                    .desired_width(f32::INFINITY),
            );
            Some((response, Some((original, shown))))
        }
    });

    let Some((response, typed)) = inner.inner else {
        return;
    };

    if response.gained_focus() {
        *cell_edit = Some(CellEdit {
            column,
            row,
            buffer: cell_text_in(tree, column, row),
        });
        // Reprend une frappe déjà atterrie dans la chaîne jetable de cette
        // image, comme `cell_widget` — sinon le premier caractère serait
        // perdu.
        if let Some((original, shown)) = typed {
            if shown != original {
                if let Some(edit) = cell_edit.as_mut() {
                    edit.buffer = shown;
                }
            }
        }
        return;
    }

    if !response.lost_focus() {
        return;
    }

    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        *cell_edit = None;
        return;
    }

    let pending = cell_edit
        .as_ref()
        .filter(|edit| edit.column == column && edit.row == row)
        .map(|edit| edit.buffer.clone());
    if let Some(text) = pending {
        *cell_edit = None;
        requests
            .actions
            .push(Action::CommitCell { column, row, text });
    }
}

/// Pendant détaché de [`matrix_contents`] — même espace réservé, sur `&Tree`
/// plutôt que sur `&AppState`.
fn detached_matrix_contents(ui: &mut egui::Ui, tree: &Tree, matrix: NodeId) {
    let name = node_title_in(tree, matrix, "Matrice");
    ui.centered_and_justified(|ui| {
        ui.weak(format!(
            "« {name} » — édition de matrice non implémentée dans ce banc \
             (hors périmètre ; `plotyx-core` n'expose pas encore de contenu \
             de matrice à éditer)."
        ));
    });
}

/// Pendant détaché de [`note_contents`] — même machine à états, sur
/// `note_edit: &mut Option<NoteEdit>` propre à cette fenêtre plutôt que sur
/// `AppState::note_editing`. Voir la doc de tête du module.
fn detached_note_contents(
    ui: &mut egui::Ui,
    tree: &Tree,
    model: &mut DetachedTabViewModel,
    requests: &mut DetachedTabRequests,
    note: NodeId,
) {
    let Some(current_name) = tree.node(note).map(|node| node.name().to_string()) else {
        ui.centered_and_justified(|ui| {
            ui.weak(format!(
                "« {} » a été supprimée.",
                node_title_in(tree, note, "Note")
            ));
        });
        return;
    };
    ui.heading(current_name);
    ui.separator();

    let editing_here = model
        .note_edit
        .as_ref()
        .is_some_and(|edit| edit.note == note);
    let mut buffer = if editing_here {
        model
            .note_edit
            .as_ref()
            .map(|edit| edit.buffer.clone())
            .unwrap_or_default()
    } else {
        note_text_in(tree, note).unwrap_or_default()
    };

    let response = ui.add(
        egui::TextEdit::multiline(&mut buffer)
            .desired_rows(20)
            .desired_width(f32::INFINITY),
    );

    if response.gained_focus() && !editing_here {
        model.note_edit = Some(NoteEdit {
            note,
            buffer: note_text_in(tree, note).unwrap_or_default(),
        });
    }
    if model
        .note_edit
        .as_ref()
        .is_some_and(|edit| edit.note == note)
    {
        if let Some(edit) = model.note_edit.as_mut() {
            edit.buffer = buffer;
        }
    }

    if response.lost_focus() {
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            model.note_edit = None;
        } else {
            let pending = model
                .note_edit
                .as_ref()
                .filter(|edit| edit.note == note)
                .map(|edit| edit.buffer.clone());
            if let Some(text) = pending {
                model.note_edit = None;
                requests.actions.push(Action::SetNoteText { note, text });
            }
        }
    }
}

/// Peint le contenu de chaque onglet du plan de disposition.
///
/// Emprunte les champs de [`PlotyxApp`] plutôt que l'application entière
/// (voir `PlotyxApp::ui`) : c'est ce qui permet à la `DockArea`
/// (`&mut dock_state`) et à ce visiteur (`&mut state`, `&mut graph_views`)
/// de coexister le temps d'une image. Ni le Project Manager ni la console de
/// script n'entrent dans ce visiteur : ce ne sont plus des onglets du dock
/// (voir la doc de tête du module, §§ Project Manager et Console de script)
/// — leurs propres clics sont recueillis à part, par
/// [`PlotyxApp::project_manager_panel`] et [`PlotyxApp::script_panel`].
struct AppTabViewer<'a> {
    state: &'a mut AppState,
    actions: &'a mut Vec<Action>,
    graph_views: &'a mut HashMap<NodeId, GraphView>,
    /// Onglets dont le menu contextuel a demandé le détachement depuis la
    /// dernière vidange — appliqués par [`PlotyxApp::ui`] une fois la
    /// `DockArea` peinte (muter `dock_state` en plein parcours
    /// l'invaliderait, même raison que `open_requests`).
    detach_requests: &'a mut Vec<Tab>,
}

impl TabViewer for AppTabViewer<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        egui::Id::new(tab)
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab_title(self.state, *tab).into()
    }

    // `is_closeable` n'est plus surchargé depuis que la console de script est
    // sortie du dock : les quatre natures restantes (`Book`/`Graph`/
    // `Matrix`/`Note`) se ferment et se rouvrent toutes librement — c'est le
    // Project Manager, via `PlotyxApp::open_tab`, qui rouvre — donc la valeur
    // par défaut du trait (`true`) convient sans exception à filtrer.
    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        tab_contents(ui, self.state, self.actions, self.graph_views, *tab);
    }

    /// Chaque onglet du dock propose « Détacher dans une fenêtre »
    /// (symétrique de « Réattacher au dock principal » côté fenêtre
    /// détachée) — les quatre natures restantes le sont toutes, depuis que la
    /// console de script est sortie du dock. Ne touche `dock_state` ni
    /// `PlotyxApp` directement : voir [`AppTabViewer::detach_requests`].
    fn context_menu(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab, _path: NodePath) {
        if ui.button("Détacher dans une fenêtre").clicked() {
            self.detach_requests.push(*tab);
            ui.close();
        }
    }
}
