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
//! [`PlotyxApp::project_manager_panel`], toujours à gauche, jamais recouvert
//! par ni ne recouvrant le [`egui::CentralPanel`] qui porte les fenêtres
//! flottantes (§ Fenêtres flottantes confinées plus bas) — garanti par
//! construction en le peignant *avant* lui dans [`PlotyxApp::ui`], comme la
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
//! peint par [`PlotyxApp::script_panel`].
//!
//! Point d'ordre important avec `egui::Panel` : **le premier panneau ajouté
//! est le plus extérieur**, chaque panneau suivant est inséré à l'intérieur
//! de ce que les précédents laissent. Pour que la console de script coure sur
//! toute la largeur de la fenêtre — y compris sous le Project Manager, pas
//! seulement sous la zone du `CentralPanel` — [`PlotyxApp::script_panel`] doit
//! donc être appelé *avant* [`PlotyxApp::project_manager_panel`] dans
//! [`PlotyxApp::ui`]. L'ordre d'appel complet y est : barre de menu
//! (`egui::Panel::top`) → console de script (`egui::Panel::bottom`) → Project
//! Manager (`egui::Panel::left`) → `CentralPanel` en dernier ; un panneau
//! posé plus tôt réserve sa bande sur toute la largeur/hauteur restante, donc
//! la console de script, postée avant le Project Manager, s'étend bien sous
//! lui plutôt que d'être resserrée entre lui et le bord droit. C'est aussi
//! cet ordre qui donne à `workspace_rect` (§ Fenêtres flottantes confinées)
//! son étendue exacte : ce que ces panneaux fixes laissent, rien de plus.
//!
//! `egui::Panel::bottom` (comme `Panel::top`) **n'est pas redimensionnable
//! par défaut**, à la différence de `left`/`right` — contrairement au Project
//! Manager, [`PlotyxApp::script_panel`] appelle donc explicitement
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
//! est réellement éditable (`plotyx-core` expose déjà son texte).
//!
//! [`PlotyxApp::open_tab`] est le seul endroit qui décide « ouvrir » vs
//! « focaliser » un onglet, quelle que soit sa nature — le même geste que
//! l'ancien `open_graph_tab`, généralisé.
//!
//! # Fenêtres flottantes confinées
//!
//! Les quatre natures de fenêtre ci-dessus s'ouvrent par défaut en fenêtres
//! flottantes ([`egui::Window`]) à l'intérieur du [`egui::CentralPanel`],
//! plutôt qu'amarrées en tuiles plein cadre — une par [`PlotyxApp::
//! floating_tabs`], jamais dupliquée (même invariant que l'ancien dock :
//! [`PlotyxApp::open_tab`] tranche « ouvrir » vs « focaliser »). Ni le
//! Project Manager ni la console de script n'y participent, voir les deux
//! sections précédentes : ce sont des panneaux fixes peints *avant* le
//! `CentralPanel`.
//!
//! **Confinement** : chaque fenêtre reçoit `.constrain_to(workspace_rect)`,
//! où `workspace_rect` est le rectangle que laisse le `CentralPanel` une fois
//! les panneaux fixes peints (`ui.max_rect()` dans sa fermeture). `egui`
//! applique cette borne à la position *et* à la taille, aussi bien à la
//! création qu'à chaque glisser/redimensionnement : une fenêtre flottante ne
//! peut donc **géométriquement pas** chevaucher le Project Manager ni la
//! console de script, quel que soit l'ordre de peinture — pas une garantie
//! de z-order (les fenêtres `egui::Window` peignent de toute façon sur la
//! couche `Order::Foreground`, au-dessus de tout panneau), une garantie de
//! rectangle : son cadre ne peut jamais s'étendre dans la zone que
//! `workspace_rect` exclut.
//!
//! **Cascade** : une fenêtre nouvellement ouverte (pas une simple
//! refocalisation) reçoit une position par défaut décalée par rapport à la
//! précédente ([`PlotyxApp::next_cascade`], remis à zéro tous les huit
//! onglets pour ne pas dériver hors de `workspace_rect`) — l'utilisateur voit
//! immédiatement qu'une nouvelle fenêtre est apparue plutôt que de la trouver
//! exactement superposée à la précédente.
//!
//! **Z-order** : `egui::Window` remonte déjà seule au premier plan au clic
//! (`bring_to_front_on_click`, par défaut). Le seul cas que ce module doit
//! encore gérer à la main est « rouvrir depuis le Project Manager une entrée
//! déjà ouverte doit la ramener au premier plan sans qu'on ait cliqué
//! dedans » : [`PlotyxApp::open_tab`] pose alors [`PlotyxApp::focus_next`],
//! consommé au rendu suivant par un appel à `Context::move_to_top` sur la
//! couche de la fenêtre concernée.
//!
//! # Maximiser un onglet
//!
//! [`PlotyxApp::maximized_tab`] est un état **à côté** de
//! [`PlotyxApp::floating_tabs`], qui ne le touche jamais : quand il vaut
//! `Some`, [`PlotyxApp::ui`] ne peint aucune fenêtre flottante cette
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
//! puisque [`PlotyxApp::project_manager_panel`] est peint avant la zone de
//! maximisation comme avant le `CentralPanel` — un panneau fixe séparé,
//! jamais dédoublé, n'a besoin d'aucun cas particulier ici.
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
//! doc de tête de [`PlotyxApp::show_detached_project_manager`].
//!
//! # Détachement générique des onglets de contenu
//!
//! [`PlotyxApp::detach_tab`]/[`PlotyxApp::reattach_tab`]/
//! [`PlotyxApp::show_detached_tabs`] généralisent le mécanisme ci-dessus à
//! [`Tab::Book`], [`Tab::Graph`], [`Tab::Matrix`] et [`Tab::Note`] — un
//! bouton « Détacher dans une fenêtre » dans l'en-tête de chaque fenêtre
//! flottante (voir § Fenêtres flottantes confinées), symétrique de
//! « Réattacher à la fenêtre principale » côté fenêtre détachée. Plusieurs
//! onglets peuvent être détachés en même temps :
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
//! [`tab_contents`] (la fonction déjà partagée par les fenêtres flottantes et
//! la zone de maximisation) prend `&mut AppState` — un onglet détaché ne peut
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
//! `AppState` correspondante, pas une réécriture : une fenêtre flottante et
//! une fenêtre détachée affichent donc littéralement la même règle, jamais
//! deux versions à tenir en accord.
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
//! SetNoteText`] qu'une fenêtre flottante, appliquée par [`AppState::
//! dispatch`] : le chemin de commande annulable, lui, ne connaît aucune
//! différence entre un onglet flottant et un onglet détaché.
//!
//! [`Tab::Graph`] échappe à cette limite : [`plotyx_render_egui::GraphView`]
//! n'a pas de verrou global à `AppState` à respecter (son cache de formes et
//! sa navigation zoom/pan sont déjà scopés par graphe, dans
//! [`PlotyxApp::graph_views`]) — [`graph_view_contents`] est donc
//! **littéralement** la même fonction pour une fenêtre flottante et pour une
//! fenêtre détachée, la vue elle-même migrant simplement hors de `graph_views` le
//! temps du détachement ([`PlotyxApp::detach_tab`]/[`PlotyxApp::
//! reattach_tab`]) pour rester un `Arc<Mutex<_>>` capturable par la
//! fermeture sans dupliquer le cache.
//!
//! [`Tab::Matrix`] n'a aucun état à faire voyager (espace réservé, voir
//! [`matrix_contents`]) : [`detached_matrix_contents`] est un simple appel à
//! [`node_title_in`].
//!
//! La console de script n'entre pour rien dans ce mécanisme : ce n'est pas
//! une fenêtre de contenu (voir § Console de script ci-dessus), donc rien à
//! détacher ici — le bouton « Détacher dans une fenêtre » n'existe que dans
//! l'en-tête des fenêtres flottantes `Book`/`Graph`/`Matrix`/`Note`.
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

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use egui_extras::{Column as TableColumn, TableBuilder};
use plotyx_core::{ColumnRole, ColumnType, Node, NodeId, Tree};
use plotyx_plot::{AxisSpec, LegendPosition, LineStyle, PlotType, SymbolShape};
use plotyx_render_egui::{GraphView, DEFAULT_FIGURE_SIZE};
use plotyx_session::{Snapshot, WindowGeometry};

use crate::selection::{GridCursor, Selection};
use crate::state::{
    cell_text_in, drop_payload_action, node_title_in, note_text_in, plot_panel_graph, row_count_in,
    series_list_label, visible_columns_in, worksheets_of_in, Action, ActiveTabKind, AppState,
    CellEdit, ColumnDragPayload, GridMenu, HeaderField, MoveDirection, NoteEdit, ProjectTreeNode,
    WindowKind,
};
use crate::theme;
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
/// Ctrl+Maj+S — choisir une destination et y enregistrer.
pub const SHORTCUT_SAVE_AS: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::S,
);
/// Ctrl+O — choisir un projet à ouvrir.
pub const SHORTCUT_OPEN: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O);
/// Ctrl+Entrée — exécuter le script de la console.
pub const SHORTCUT_RUN_SCRIPT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Enter);

/// Ctrl+C — copier la sélection de la grille.
///
/// # Ces cinq raccourcis-ci ne sont **jamais** consommés
///
/// Contrairement à [`SHORTCUT_UNDO`] et compagnie, les cinq constantes qui
/// suivent sont purement **décoratives** : elles n'existent que pour écrire
/// le rappel de raccourci à droite d'une entrée de menu contextuel
/// (`Context::format_shortcut`, qui rend « Ctrl+C » ou « ⌘C » selon la
/// plateforme). Aucun appel à `consume_shortcut` ne les prend, et il ne faut
/// pas essayer : `egui-winit` intercepte Ctrl+C/X/V à la traduction des
/// événements de la fenêtre et n'en laisse jamais passer de `Event::Key`
/// — un essai précédent, fondé sur l'hypothèse inverse, produisait des
/// raccourcis qui ne se déclenchaient jamais. Le bloc clavier de
/// [`grid_contents`] lit les `Event::Copy`/`Event::Cut`/`Event::Paste`
/// correspondants, et c'est là que vit la vraie logique.
pub const SHORTCUT_COPY: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::C);
/// Ctrl+Maj+C — copier en entier les colonnes touchées (décoratif, voir
/// [`SHORTCUT_COPY`]).
pub const SHORTCUT_COPY_FULL_COLUMNS: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::C,
);
/// Ctrl+X — couper (décoratif, voir [`SHORTCUT_COPY`]).
pub const SHORTCUT_CUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::X);
/// Ctrl+V — coller à partir de l'ancre (décoratif, voir [`SHORTCUT_COPY`]).
pub const SHORTCUT_PASTE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::V);
/// Ctrl+Maj+V — coller à l'identique (décoratif, voir [`SHORTCUT_COPY`]).
pub const SHORTCUT_PASTE_ABSOLUTE: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::V,
);

/// Rappel de raccourci pour Retour arrière, écrit en toutes lettres plutôt
/// que via `Context::format_shortcut`.
///
/// egui nomme ses touches en anglais (« Backspace », « Delete ») : les
/// passer par `format_shortcut` mettrait deux mots anglais au milieu d'un
/// menu entièrement français. Les raccourcis à modificateur, eux, se rendent
/// identiquement dans toutes les langues (« Ctrl+C ») et gagnent à passer
/// par `format_shortcut`, qui sait les adapter à macOS.
const SHORTCUT_TEXT_BACKSPACE: &str = "Retour arr.";
/// Pendant de [`SHORTCUT_TEXT_BACKSPACE`] pour Suppr.
const SHORTCUT_TEXT_DELETE: &str = "Suppr";

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
/// Hauteur réservée sous la grille pour le séparateur final et la bande
/// d'onglets de sheet dans [`book_contents`] — généreuse plutôt
/// qu'ajustée au pixel près : [`book_contents`] la soustrait de
/// `ui.available_height()` *avant* de peindre effectivement ce qui suit,
/// donc toute sous-estimation ferait déborder le contenu réel de la
/// hauteur allouée à la grille. À l'intérieur d'une fenêtre flottante
/// (`egui::Window`, redimensionnable, § Fenêtres flottantes confinées de la
/// doc de tête), déborder ainsi ne se contente pas de rogner un pixel :
/// `egui::Resize` mémorise la taille du contenu débordant comme nouvelle
/// taille « désirée » de la fenêtre et ne revient jamais en arrière (elle ne
/// fait que grandir, jamais rétrécir, d'une image sur l'autre) — un
/// débordement constant d'une image à l'autre s'accumule
/// donc frame après frame jusqu'à buter sur `constrain_to` (la fenêtre
/// « explose » en hauteur au premier mouvement de souris qui redessine).
/// Une légère surestimation ici, à l'inverse, ne coûte qu'un mince espace
/// vide au-dessus de la bande d'onglets — sans danger, contrairement à la
/// sous-estimation.
const SHEET_TABS_HEIGHT: f32 = 40.0;
/// Hauteur du seul contrôle conservé sous une figure : le retour au cadrage
/// initial. Réserver sa hauteur empêche `egui::Window` de mémoriser son
/// dépassement comme une nouvelle taille désirée, ce qui ferait changer le
/// rectangle de la figure et invaliderait le cache au moindre survol.
const GRAPH_CONTROLS_HEIGHT: f32 = 28.0;
/// Hauteur de la zone de sortie du script, en points.
const SCRIPT_OUTPUT_HEIGHT: f32 = 140.0;
/// Hauteur par défaut du panneau fixe de la console de script — point de
/// départ seulement, l'utilisateur peut ensuite le redimensionner en
/// glissant son bord (`.resizable(true)`, voir [`PlotyxApp::script_panel`]).
const SCRIPT_PANEL_HEIGHT: f32 = 220.0;
/// Largeur par défaut du panneau Project Manager fixe, et de la fenêtre OS
/// séparée quand il est détaché.
const PROJECT_MANAGER_WIDTH: f32 = 260.0;
/// Taille par défaut des fenêtres flottantes qui n'affichent pas un graphe —
/// voir la doc de tête du module, § Fenêtres flottantes confinées. Rognée à
/// l'aire de travail disponible si elle est plus petite.
const FLOATING_WINDOW_SIZE: egui::Vec2 = egui::vec2(560.0, 420.0);
/// Taille par défaut d'une fenêtre de graphe : la figure de référence
/// (`480×360`), son retour au cadrage initial, l'en-tête et les marges y
/// entrent tous dès l'ouverture.
const GRAPH_WINDOW_SIZE: egui::Vec2 = egui::vec2(560.0, 460.0);
/// Largeur par défaut d'une fenêtre flottante [`Tab::Book`] — le tiers de
/// moins que [`FLOATING_WINDOW_SIZE`] : une grille n'a pas besoin d'être
/// aussi large qu'un graphe pour rester lisible, demande explicite.
const BOOK_WINDOW_WIDTH: f32 = FLOATING_WINDOW_SIZE.x * 2.0 / 3.0;
/// Décalage, en points, entre deux fenêtres flottantes ouvertes l'une après
/// l'autre (cascade) — voir [`PlotyxApp::next_cascade`].
const CASCADE_STEP: f32 = 24.0;
/// Nombre de crans de cascade avant de reboucler à zéro, pour qu'une
/// succession de fenêtres ne dérive jamais hors de l'aire de travail.
const CASCADE_WRAP: u32 = 8;
/// Largeur, en points, de la zone de prise du glisser-déposer de colonnes
/// vers un graphe — une bande le long du bord extérieur gauche ou droit de
/// la grille (voir [`grid_grip_side`], et [`grid_contents`] pour le geste
/// complet).
const COLUMN_GRIP_WIDTH: f32 = 10.0;
/// Largeur par défaut du panneau de configuration des graphiques
/// ([`PlotyxApp::ui`], § panneau de configuration).
const PLOT_CONFIG_PANEL_WIDTH: f32 = 300.0;

/// Bord de la grille sur lequel se trouve `pointer`, si `pointer` est à
/// `margin` points ou moins de son bord gauche ou droit et dans ses bornes
/// verticales — la zone de prise du glisser-déposer de colonnes vers un
/// graphe.
///
/// Fonction pure sur des coordonnées brutes (`(gauche, haut, droite, bas)`,
/// `(x, y)`), pas sur `egui::Rect`/`egui::Pos2` : testable dans
/// `tests/shell.rs` sans dépendre d'une fenêtre ni d'une passe de rendu —
/// même esprit que le reste de ce module, où la décision est toujours
/// séparée du code de peinture qui l'utilise ([`grid_contents`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GripSide {
    /// Bord gauche de la grille.
    Left,
    /// Bord droit de la grille.
    Right,
}

/// Voir [`GripSide`].
pub fn grid_grip_side(
    grid: (f32, f32, f32, f32),
    pointer: (f32, f32),
    margin: f32,
) -> Option<GripSide> {
    let (left, top, right, bottom) = grid;
    let (x, y) = pointer;
    if y < top || y > bottom {
        return None;
    }
    if x >= left && x <= left + margin {
        Some(GripSide::Left)
    } else if x <= right && x >= right - margin {
        Some(GripSide::Right)
    } else {
        None
    }
}

/// Un onglet, c'est-à-dire une fenêtre de contenu ouverte.
///
/// Ni le Project Manager ni la console de script n'en sont — voir la doc de
/// tête du module, §§ Project Manager et Console de script : ce sont deux
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

/// L'application `eframe` : l'état du banc, la disposition en fenêtres
/// flottantes confinées, et le strict nécessaire au rendu.
pub struct PlotyxApp {
    /// L'état, seul dépositaire de la logique.
    state: AppState,
    /// Les onglets de contenu actuellement ouverts en fenêtre flottante, dans
    /// l'ordre où ils ont été focalisés pour la dernière fois — le dernier
    /// élément est le plus récemment focalisé (voir la doc de tête du
    /// module, § Fenêtres flottantes confinées, sous-section z-order).
    /// Absent le temps d'être maximisé ou détaché en fenêtre OS séparée
    /// (même invariant que `detached_tabs` pour ce second cas).
    floating_tabs: Vec<Tab>,
    /// Rang de cascade attribué à chaque fenêtre flottante actuellement
    /// ouverte, à sa création — sert à calculer sa position par défaut
    /// (`PlotyxApp::ui`) ; ne bouge plus ensuite, y compris si l'utilisateur
    /// déplace la fenêtre (`egui` retient alors sa position lui-même, via son
    /// `Id`). Une entrée disparaît de cette table en même temps que de
    /// `floating_tabs`.
    cascade_rank: HashMap<Tab, u32>,
    /// Prochain rang de cascade à distribuer — incrémenté à chaque nouvelle
    /// fenêtre flottante créée (pas à un simple refocus), remis à zéro tous
    /// les [`CASCADE_WRAP`] pour ne pas dériver hors de l'aire de travail.
    next_cascade: u32,
    /// Onglet à ramener au premier plan au prochain rendu — posé par
    /// [`PlotyxApp::open_tab`] quand l'onglet rouvert était déjà une fenêtre
    /// flottante existante (voir la doc de tête du module, § z-order).
    focus_next: Option<Tab>,
    /// Une vue de graphe (et son cache de formes, phase R4) par graphe
    /// **ouvert** — c'est-à-dire ayant eu un onglet au moins une fois,
    /// l'entrée survit à la fermeture de l'onglet. Un cache par vue est la
    /// règle (`plotyx_render_egui::GraphView`) ; avec plusieurs graphes
    /// affichables simultanément, une seule vue ne suffit plus.
    graph_views: HashMap<NodeId, GraphView>,
    /// Onglets de graphe dont seul le contenu est replié. Cet état de
    /// présentation reste local à l'application : il ne modifie ni le
    /// graphe, ni sa géométrie enregistrée, ni les autres fenêtres.
    collapsed_graph_tabs: HashSet<Tab>,
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
    /// `floating_tabs` tant qu'il l'est (même invariant que `pm_detached`
    /// pour le Project Manager, généralisé à plusieurs onglets simultanés).
    detached_tabs: HashMap<Tab, DetachedTab>,
    /// Position demandée par la barre de titre maison. `egui::Window` ne
    /// permet pas d'ajouter des actions à sa barre native ; la position reste
    /// donc explicite quand cette barre prend en charge le glisser.
    window_positions: HashMap<Tab, egui::Pos2>,
    /// Dernière géométrie envoyée à l'acteur, pour ne pas lui adresser une
    /// requête à chaque image pendant qu'une fenêtre reste immobile.
    saved_window_geometries: HashMap<NodeId, WindowGeometry>,
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
/// boucle principale (seule à porter `&mut AppState` et `&mut
/// PlotyxApp::floating_tabs`) les applique après le rendu — même schéma que
/// les `open_requests`/
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
    /// classeur aurait en fenêtre flottante (il ne peut de toute façon pas
    /// être dans les deux à la fois).
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
    /// Le premier callback d'un viewport différé peut arriver avant que
    /// Wayland/WGPU ait stabilisé sa taille. Une seule image supplémentaire
    /// donne alors à la figure son rectangle réel, sans entretenir une boucle
    /// de rendu quand la fenêtre reste inactive.
    needs_initial_repaint: bool,
}

/// Ce qu'une fenêtre détachée écrit, pour que [`PlotyxApp::ui`] (seule à
/// porter `&mut AppState`) les applique après le rendu — même rôle que
/// [`DetachedProjectManagerRequests`].
#[derive(Debug, Default)]
struct DetachedTabRequests {
    /// Actions produites par cette fenêtre depuis la dernière vidange
    /// (validation de cellule/note, structure de la grille…) — le même
    /// chemin de commande annulable qu'une fenêtre flottante, voir la doc de
    /// tête du module.
    actions: Vec<Action>,
    /// Vrai si « Réattacher à la fenêtre principale » a été actionné depuis
    /// la dernière vidange.
    reattach: bool,
    /// La croix du système ferme la vue détachée sans la réattacher.
    close: bool,
    /// Géométrie rapportée par le viewport OS, en coordonnées écran.
    geometry: Option<WindowGeometry>,
}

impl PlotyxApp {
    /// Construit l'application autour d'un état.
    ///
    /// # Disposition initiale
    ///
    /// Ni le Project Manager ni la console de script n'entrent pour rien
    /// dans ce plan : ce sont deux panneaux fixes peints à part (voir
    /// [`PlotyxApp::project_manager_panel`] et [`PlotyxApp::script_panel`]),
    /// pas des fenêtres flottantes. Si le projet contient au moins un
    /// classeur, son onglet s'ouvre en fenêtre flottante, suivi de l'onglet
    /// du premier graphe s'il y en a un — montrer quelque chose sans qu'on
    /// ait à cliquer, comme l'ancien comportement par défaut. À défaut de
    /// classeur ou de graphe, aucune fenêtre flottante ne s'ouvre (seuls les
    /// deux panneaux fixes et la barre de menu sont visibles).
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

        let mut floating_tabs = Vec::new();
        let mut cascade_rank = HashMap::new();
        let mut next_cascade = 0u32;
        let mut open_initial = |tab: Tab| {
            floating_tabs.push(tab);
            cascade_rank.insert(tab, next_cascade);
            next_cascade = next_cascade.wrapping_add(1);
        };
        if let Some((first_book, _)) = state.books().into_iter().next() {
            open_initial(Tab::Book(first_book));
        }
        if let Some((first_graph, _)) = state.graphs().into_iter().next() {
            open_initial(Tab::Graph(first_graph));
        }
        // Sans classeur ni graphe à montrer, aucune fenêtre flottante ne
        // s'ouvre — ni la console de script ni le Project Manager n'ont leur
        // place ici, ce sont des panneaux fixes.

        Self {
            state,
            floating_tabs,
            cascade_rank,
            next_cascade,
            focus_next: None,
            graph_views: HashMap::new(),
            collapsed_graph_tabs: HashSet::new(),
            maximized_tab: None,
            pm_detached: false,
            pm_view_model: Arc::new(Mutex::new(ProjectManagerViewModel::default())),
            pm_requests: Arc::new(Mutex::new(DetachedProjectManagerRequests::default())),
            detached_tabs: HashMap::new(),
            window_positions: HashMap::new(),
            saved_window_geometries: HashMap::new(),
            autoquit,
            first_frame: None,
            screenshot: std::env::var_os(SCREENSHOT_ENV).map(std::path::PathBuf::from),
            screenshot_delay: SCREENSHOT_SETTLE_FRAMES,
        }
    }

    /// Consomme les raccourcis au niveau du contexte et en fait des actions.
    fn collect_shortcuts(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let mut save_as = false;
        let mut open = false;
        ctx.input_mut(|input| {
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
        if save_as {
            self.choose_save_path(actions);
        }
        if open {
            self.choose_open_path(actions);
        }
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
                    if ui.button("Nouveau classeur").clicked() {
                        actions.push(Action::AddWorkbook);
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .add(
                            egui::Button::new("Ouvrir…")
                                .shortcut_text(ctx.format_shortcut(&SHORTCUT_OPEN)),
                        )
                        .clicked()
                    {
                        self.choose_open_path(actions);
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
                    if ui
                        .add(
                            egui::Button::new("Enregistrer sous…")
                                .shortcut_text(ctx.format_shortcut(&SHORTCUT_SAVE_AS)),
                        )
                        .clicked()
                    {
                        self.choose_save_path(actions);
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

    /// Demande le fichier à ouvrir, puis confie le chargement à
    /// [`AppState::dispatch`] par l'action habituelle.
    fn choose_open_path(&mut self, actions: &mut Vec<Action>) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Ouvrir un projet")
            .add_filter("Projet PlotYx", &["plx", "plotyx"])
            .pick_file()
        {
            self.state.set_path(path.display().to_string());
            actions.push(Action::Load);
        }
    }

    /// Demande la destination de sauvegarde, puis confie l'écriture à
    /// [`AppState::dispatch`] par l'action habituelle.
    fn choose_save_path(&mut self, actions: &mut Vec<Action>) {
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
    /// Manager, pas seulement sous la zone du `CentralPanel` — plutôt que
    /// d'être resserré entre lui et le bord droit. `egui::Panel::bottom`
    /// n'est **pas** redimensionnable par défaut (à la différence de
    /// `left`/`right`) : `.resizable(true)` restaure le glisser du bord en
    /// hauteur. Contrairement au Project
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
    /// [`PlotyxApp::ui`] *avant* le `CentralPanel`, mais
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
        if let Some(pos) = self.floating_tabs.iter().position(|t| *t == tab) {
            self.floating_tabs.remove(pos);
            self.floating_tabs.push(tab);
            self.focus_next = Some(tab);
        } else {
            self.push_floating_tab(tab);
        }
    }

    /// Ajoute `tab` à [`PlotyxApp::floating_tabs`] avec un nouveau rang de
    /// cascade — partagé par [`PlotyxApp::open_tab`] (première ouverture) et
    /// [`PlotyxApp::reattach_tab`] (retour d'une fenêtre OS séparée), les deux
    /// seuls endroits où une fenêtre flottante apparaît sans exister déjà.
    fn push_floating_tab(&mut self, tab: Tab) {
        self.floating_tabs.push(tab);
        self.cascade_rank.insert(tab, self.next_cascade);
        self.next_cascade = self.next_cascade.wrapping_add(1);
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
        let root_context = ctx.clone();
        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("plotyx_project_manager_window"),
            egui::ViewportBuilder::default()
                .with_title("PlotYx — Project Manager")
                .with_inner_size([PROJECT_MANAGER_WIDTH + 40.0, 480.0]),
            move |ui, _class| {
                let mut opened = Vec::new();
                egui::CentralPanel::default().show(ui, |ui| {
                    if ui.button("Réattacher à la fenêtre principale").clicked() {
                        requests
                            .lock()
                            .unwrap_or_else(|err| err.into_inner())
                            .reattach = true;
                        root_context.request_repaint_of(egui::ViewportId::ROOT);
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

    /// Retire `tab` des fenêtres flottantes et le détache en fenêtre OS
    /// séparée — voir la doc de tête du module, § détachement générique. Sans
    /// effet si `tab` est déjà détaché ou n'est pas une fenêtre flottante
    /// actuellement ouverte. Ni le Project Manager ni la console de script
    /// n'entrent dans ce chemin depuis leur passage en panneaux fixes — voir
    /// [`PlotyxApp::detach_project_manager`] pour le premier ; la seconde n'a
    /// simplement plus de mécanisme de détachement du tout.
    fn detach_tab(&mut self, tab: Tab) {
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
            needs_initial_repaint: true,
        };
        self.detached_tabs.insert(
            tab,
            DetachedTab {
                view_model: Arc::new(Mutex::new(view_model)),
                requests: Arc::new(Mutex::new(DetachedTabRequests::default())),
            },
        );
    }

    /// Retire `tab` de [`PlotyxApp::detached_tabs`] et le repousse en fenêtre
    /// flottante (nouveau rang de cascade). Sans effet s'il n'est pas
    /// détaché. Une éventuelle [`GraphView`] déplacée par
    /// [`PlotyxApp::detach_tab`] revient dans [`PlotyxApp::graph_views`] — le
    /// cache de formes du graphe survit au détachement/réattachement, comme
    /// il survit déjà à la fermeture d'une fenêtre flottante.
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
                let mut view = view;
                view.invalidate_surface();
                self.graph_views.insert(id, view);
            }
        }
        if !self.floating_tabs.contains(&tab) {
            self.push_floating_tab(tab);
        }
    }

    /// Ferme uniquement la vue détachée. Les données restent dans le projet
    /// et le cache du graphe revient dans l'application, comme après la
    /// fermeture d'une fenêtre flottante.
    fn close_detached_tab(&mut self, tab: Tab) {
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
    fn remember_window_geometry(&mut self, tab: Tab, rect: egui::Rect) {
        let node = tab_node(tab);
        let geometry = WindowGeometry {
            x: f64::from(rect.min.x),
            y: f64::from(rect.min.y),
            width: f64::from(rect.width()),
            height: f64::from(rect.height()),
        };
        if self.saved_window_geometries.get(&node) != Some(&geometry) {
            self.state.set_window_geometry(node, geometry);
            self.saved_window_geometries.insert(node, geometry);
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
            let root_context = ctx.clone();
            let tab = *tab;
            let title = format!("PlotYx — {}", tab_title(state, tab));
            // `Tab::Graph` part à une taille qui contient sa figure de
            // référence et son retour au cadrage initial.
            // Le reste (`Book`/`Matrix`/`Note`) partage une taille uniforme
            // montrant 3 colonnes de grille : `grid_contents`/`matrix_contents`
            // utilisent `TableColumn::exact(56.0)` pour la colonne de numéro
            // de ligne puis `TableColumn::initial(110.0)` par colonne de
            // données, soit 56 + 3×110 = 386 ; 420 ajoute une marge pour les
            // bordures et la scrollbar verticale de la `ScrollArea`.
            // `Tab::Note` n'a pas de grille à proprement parler mais reçoit
            // la même taille que « les autres » pour rester cohérente avec
            // eux — c'est explicitement voulu, pas un oubli.
            let size = match tab {
                Tab::Graph(_) => [GRAPH_WINDOW_SIZE.x, GRAPH_WINDOW_SIZE.y],
                _ => [420.0, 360.0],
            };
            let saved_geometry = state.snapshot().window_geometries.get(&tab_node(tab));
            let viewport = if let Some(geometry) = saved_geometry {
                egui::ViewportBuilder::default()
                    .with_title(title)
                    .with_inner_size([geometry.width as f32, geometry.height as f32])
                    .with_position(egui::pos2(geometry.x as f32, geometry.y as f32))
            } else {
                egui::ViewportBuilder::default()
                    .with_title(title)
                    .with_inner_size(size)
            };
            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of(tab),
                viewport,
                move |ui, _class| {
                    let mut model = view_model.lock().unwrap_or_else(|err| err.into_inner());
                    let mut requests = requests.lock().unwrap_or_else(|err| err.into_inner());
                    egui::CentralPanel::default().show(ui, |ui| {
                        // Hauteur bornée explicitement, comme
                        // `window_header` (même piège) : sans elle,
                        // `Layout::right_to_left(Align::Center)` demande à
                        // sa `Ui` enfant toute la hauteur restante pour
                        // pouvoir centrer verticalement dedans — un simple
                        // bouton « ↩ » consommerait alors toute la fenêtre
                        // détachée, laissant 0 pixel de hauteur disponible
                        // au contenu qui suit (la figure d'un `Tab::Graph`,
                        // entre autres) et le rendant invisible.
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), 28.0),
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui
                                    .small_button("↩")
                                    .on_hover_text("Réattacher à la fenêtre principale")
                                    .clicked()
                                {
                                    requests.reattach = true;
                                    root_context.request_repaint_of(egui::ViewportId::ROOT);
                                }
                            },
                        );
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
                    if let Some(rect) = ui.ctx().input(|input| input.viewport().outer_rect) {
                        requests.geometry = Some(WindowGeometry {
                            x: f64::from(rect.min.x),
                            y: f64::from(rect.min.y),
                            width: f64::from(rect.width()),
                            height: f64::from(rect.height()),
                        });
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

        // Sonde un script terminé sur son fil détaché **avant** de peindre :
        // sans notification poussée de l'acteur, c'est le seul endroit qui
        // rafraîchit l'instantané après un script (voir la doc de tête de
        // `crate::state`). Ne bloque jamais, même sans script en cours.
        self.state.poll_script();

        self.collect_shortcuts(&ctx, &mut actions);
        self.menu_bar(ui, &ctx, &mut actions);
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
            // de toute façon pas peint cette image-ci) et le `CentralPanel`.
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
            // Panneau fixe, peint *avant* le `CentralPanel` — voir la doc de
            // tête du module, § Project Manager : c'est ce qui garantit par
            // construction qu'il n'est jamais recouvert par lui ni ne le
            // recouvre, comme la barre de menu (`egui::Panel::top`) peinte
            // plus haut.
            self.project_manager_panel(ui, &mut open_requests);
        }

        if !self.detached_tabs.is_empty() {
            self.show_detached_tabs(&ctx);
            // Même vidange que pour le Project Manager, généralisée à
            // plusieurs onglets : chacun peut avoir accumulé des actions et/ou
            // demandé à être réattaché depuis la dernière image.
            let mut to_reattach: Vec<Tab> = Vec::new();
            let mut to_close: Vec<Tab> = Vec::new();
            let mut geometries: Vec<(Tab, WindowGeometry)> = Vec::new();
            for (tab, detached) in &self.detached_tabs {
                let mut requests = detached
                    .requests
                    .lock()
                    .unwrap_or_else(|err| err.into_inner());
                actions.append(&mut requests.actions);
                if std::mem::take(&mut requests.reattach) {
                    to_reattach.push(*tab);
                }
                if std::mem::take(&mut requests.close) {
                    to_close.push(*tab);
                }
                if let Some(geometry) = requests.geometry.take() {
                    geometries.push((*tab, geometry));
                }
            }
            for (tab, geometry) in geometries {
                let node = tab_node(tab);
                if self.saved_window_geometries.get(&node) != Some(&geometry) {
                    self.state.set_window_geometry(node, geometry);
                    self.saved_window_geometries.insert(node, geometry);
                }
            }
            for tab in to_reattach {
                self.reattach_tab(tab);
            }
            for tab in to_close {
                self.close_detached_tab(tab);
            }
        }

        // Emprunts disjoints des champs (édition 2021) : la disposition et
        // les contenus qu'elle porte ont besoin, chacun, d'un accès distinct
        // à `self`, simultanément le temps de peindre une image.
        let Self {
            state,
            floating_tabs,
            cascade_rank,
            next_cascade,
            focus_next,
            graph_views,
            collapsed_graph_tabs,
            maximized_tab,
            window_positions,
            ..
        } = self;

        // Panneau de configuration des graphiques : un `egui::Panel::right`
        // posé *avant* le `CentralPanel` — comme le Project Manager plus
        // haut — pour que les fenêtres flottantes ne lui disputent jamais
        // l'espace (il fait partie des panneaux fixes qui bornent
        // `workspace_rect`, voir la doc de tête du module, § Fenêtres
        // flottantes confinées). Visible seulement quand l'onglet actif (au
        // sens le plus large : l'onglet maximisé s'il y en a un, sinon la
        // fenêtre flottante la plus récemment focalisée — dernier élément de
        // `floating_tabs`, § z-order) est un `Tab::Graph` — voir
        // `plot_panel_graph`, testée sans fenêtre dans `tests/shell.rs`.
        // Masqué sinon, y compris pendant l'édition d'un autre onglet : pas
        // de panneau vide à regarder pour un classeur ou une note.
        let active_tab = maximized_tab.or_else(|| floating_tabs.last().copied());
        match plot_panel_graph(active_tab_kind(active_tab)) {
            Some(graph) => {
                egui::Panel::right("plotyx_plot_config")
                    .default_size(PLOT_CONFIG_PANEL_WIDTH)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            plot_config_panel(ui, state, graph);
                        });
                    });
            }
            None => state.close_plot_edit(),
        }

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
                        if ui.button("Restaurer la disposition").clicked() {
                            *maximized_tab = None;
                        }
                    });
                });
                ui.separator();
                tab_contents(ui, state, &mut actions, graph_views, tab);
            } else {
                // Ce que les panneaux fixes peints plus haut (menu, outils,
                // statut, script, Project Manager) laissent — voir la doc de
                // tête du module, § Fenêtres flottantes confinées. Chaque
                // fenêtre flottante reçoit `.constrain_to(workspace_rect)` :
                // elle ne peut donc géométriquement jamais chevaucher ces
                // panneaux, quel que soit son déplacement/redimensionnement.
                let workspace_rect = ui.max_rect();
                for &tab in floating_tabs.iter() {
                    let id = egui::Id::new(tab);
                    if *focus_next == Some(tab) {
                        ui.ctx()
                            .move_to_top(egui::LayerId::new(egui::Order::Foreground, id));
                    }
                    let rank = cascade_rank.entry(tab).or_insert_with(|| {
                        let rank = *next_cascade;
                        *next_cascade = next_cascade.wrapping_add(1);
                        rank
                    });
                    let cascade = CASCADE_STEP * (*rank % CASCADE_WRAP) as f32;
                    let default_pos = workspace_rect.min + egui::vec2(cascade, cascade);
                    let natural_size = match tab {
                        Tab::Book(_) => egui::vec2(BOOK_WINDOW_WIDTH, FLOATING_WINDOW_SIZE.y),
                        Tab::Graph(_) => GRAPH_WINDOW_SIZE,
                        _ => FLOATING_WINDOW_SIZE,
                    };
                    let default_size = natural_size.min(workspace_rect.size());
                    let saved_geometry = state.snapshot().window_geometries.get(&tab_node(tab));
                    let saved_pos = saved_geometry
                        .map(|geometry| egui::pos2(geometry.x as f32, geometry.y as f32))
                        .unwrap_or(default_pos);
                    let saved_size = saved_geometry
                        .map(|geometry| egui::vec2(geometry.width as f32, geometry.height as f32))
                        .unwrap_or(default_size)
                        .min(workspace_rect.size());
                    let current_pos = *window_positions.entry(tab).or_insert(saved_pos);
                    let mut open = true;
                    let mut header_actions = WindowHeaderActions::default();
                    let was_collapsed = collapsed_graph_tabs.contains(&tab);
                    egui::Window::new(tab_title(state, tab))
                        .id(id)
                        .title_bar(false)
                        .drag_area(egui::WindowDrag::Off)
                        .current_pos(current_pos)
                        .default_size(saved_size)
                        .constrain_to(workspace_rect)
                        .open(&mut open)
                        .show(ui.ctx(), |ui| {
                            header_actions =
                                window_header(ui, id, &tab_title(state, tab), tab, was_collapsed);
                            if !(was_collapsed ^ header_actions.toggle_collapse) {
                                tab_contents(ui, state, &mut actions, graph_views, tab);
                            }
                        });
                    if header_actions.toggle_collapse {
                        toggle_graph_collapse(collapsed_graph_tabs, tab);
                    }
                    if header_actions.maximize {
                        *maximized_tab = Some(tab);
                    }
                    if header_actions.detach {
                        detach_requests.push(tab);
                    }
                    if header_actions.close {
                        open = false;
                    }
                    if header_actions.drag_delta != egui::Vec2::ZERO {
                        if let Some(position) = window_positions.get_mut(&tab) {
                            *position += header_actions.drag_delta;
                        }
                    }
                    if let Some(rect) = ui.ctx().memory(|memory| memory.area_rect(id)) {
                        geometry_updates.push((tab, rect));
                    }
                    if !open {
                        close_requests.push(tab);
                    }
                }
                *focus_next = None;
            }
        });

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
        for tab in close_requests {
            if let Some(pos) = self.floating_tabs.iter().position(|t| *t == tab) {
                self.floating_tabs.remove(pos);
                self.cascade_rank.remove(&tab);
                self.window_positions.remove(&tab);
            }
            if self.maximized_tab == Some(tab) {
                // Garde de cohérence, comme dans `detach_tab` — ne devrait
                // pas arriver : un onglet maximisé n'est pas peint comme
                // fenêtre flottante, donc son bouton de fermeture natif n'est
                // pas accessible pendant qu'il l'est.
                self.maximized_tab = None;
            }
        }
        for action in actions {
            // `Action::PlotColumns` doit ouvrir le graphe qu'elle crée
            // (chantier « tracer depuis un classeur »). `dispatch` ne rend
            // jamais l'identifiant créé — même contrainte que l'import
            // (`AppState::import_files`) — donc le dossier parent se
            // calcule **avant** l'envoi de la commande, et le graphe se
            // retrouve après coup comme le dernier enfant de ce dossier :
            // `Project::add_graph` l'y insère en dernière position, et
            // `AppState::dispatch` a déjà rafraîchi `self.state`
            // (`AppState::refresh`) au retour de cet appel.
            let plotted_from = if let Action::PlotColumns { sheet, .. } = &action {
                self.state
                    .tree()
                    .node(*sheet)
                    .and_then(Node::parent)
                    .and_then(|book| self.state.tree().node(book).and_then(Node::parent))
            } else {
                None
            };
            let creates_workbook = matches!(action, Action::AddWorkbook);
            let revision_before = self.state.revision();
            self.state.dispatch(action);
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

/// Chaque variante de [`Tab`] correspond à une fenêtre persistée identifiée
/// par son nœud.
fn tab_node(tab: Tab) -> NodeId {
    match tab {
        Tab::Book(id) | Tab::Graph(id) | Tab::Matrix(id) | Tab::Note(id) => id,
    }
}

/// Bascule le repli du contenu d'un graphe. Les autres types d'onglets n'ont
/// pas ce contrôle et restent donc toujours affichés.
fn toggle_graph_collapse(collapsed_graph_tabs: &mut HashSet<Tab>, tab: Tab) {
    if !matches!(tab, Tab::Graph(_)) {
        return;
    }
    if !collapsed_graph_tabs.insert(tab) {
        collapsed_graph_tabs.remove(&tab);
    }
}

#[derive(Default)]
struct WindowHeaderActions {
    maximize: bool,
    detach: bool,
    close: bool,
    toggle_collapse: bool,
    drag_delta: egui::Vec2,
}

/// Barre de titre des fenêtres de contenu. Les actions et la croix sont des
/// boutons `egui` de même nature et forment un seul groupe visuel ; cela évite
/// une `Area` superposée, qui se décale quand une fenêtre passe derrière une
/// autre.
fn window_header(
    ui: &mut egui::Ui,
    id: egui::Id,
    title: &str,
    tab: Tab,
    collapsed: bool,
) -> WindowHeaderActions {
    const ACTIONS_WIDTH: f32 = 90.0;
    const COLLAPSE_WIDTH: f32 = 24.0;
    const ACTION_WIDTH: f32 = 24.0;

    let mut actions = WindowHeaderActions::default();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::hover());
    let mut header = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let collapse_width = if matches!(tab, Tab::Graph(_)) {
        COLLAPSE_WIDTH
    } else {
        0.0
    };
    if collapse_width > 0.0 {
        let collapse_rect =
            egui::Rect::from_min_size(rect.min, egui::vec2(collapse_width, rect.height()));
        let (icon, tooltip) = if collapsed {
            (">", "Afficher le contenu du graphe")
        } else {
            ("v", "Masquer le contenu du graphe")
        };
        header.painter().text(
            collapse_rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            egui::TextStyle::Button.resolve(header.style()),
            header.visuals().text_color(),
        );
        if header
            .interact(
                collapse_rect,
                id.with("graph_collapse"),
                egui::Sense::click(),
            )
            .on_hover_text(tooltip)
            .clicked()
        {
            actions.toggle_collapse = true;
        }
    }
    let action_rect = |offset: f32| {
        egui::Rect::from_min_size(
            egui::pos2(rect.right() - offset - ACTION_WIDTH, rect.top() + 2.0),
            egui::vec2(ACTION_WIDTH, rect.height() - 4.0),
        )
    };
    if header
        .put(action_rect(2.0), egui::Button::new("×").small())
        .on_hover_text("Fermer la fenêtre")
        .clicked()
    {
        actions.close = true;
    }
    if matches!(tab, Tab::Book(_) | Tab::Graph(_)) {
        if header
            .put(action_rect(28.0), egui::Button::new("↗").small())
            .on_hover_text("Détacher dans une fenêtre")
            .clicked()
        {
            actions.detach = true;
        }
        if header
            .put(action_rect(54.0), egui::Button::new("⛶").small())
            .on_hover_text("Maximiser la fenêtre")
            .clicked()
        {
            actions.maximize = true;
        }
    }
    let title_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + collapse_width, rect.top()),
        egui::pos2(rect.right() - ACTIONS_WIDTH, rect.bottom()),
    );
    header.painter().with_clip_rect(title_rect).text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        title,
        egui::TextStyle::Heading.resolve(header.style()),
        header.visuals().text_color(),
    );
    let drag_rect = title_rect;
    let drag = ui.interact(
        drag_rect,
        id.with("content_title_drag"),
        egui::Sense::drag(),
    );
    if drag.dragged() {
        actions.drag_delta = ui.input(|input| input.pointer.delta());
    }
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    actions
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

/// Traduit le `Tab` (privé à ce module) que voit `PlotyxApp::ui` vers
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

/// Le nom du nœud `id`, ou `"{missing} {id} (supprimé)"` s'il n'existe plus.
fn node_title(state: &AppState, id: NodeId, missing: &str) -> String {
    state
        .tree()
        .node(id)
        .map(|node| node.name().to_string())
        .unwrap_or_else(|| format!("{missing} {id} (supprimé)"))
}

/// Peint le contenu de l'onglet `tab`, quelle que soit sa provenance — la
/// boucle des fenêtres flottantes ou la zone de maximisation
/// ([`PlotyxApp::ui`]) l'appellent toutes les deux : **une seule** fonction
/// qui sait peindre un onglet, jamais deux implémentations à tenir en
/// accord. Ne prend plus de `&egui::Context` depuis que la console de script
/// (seule nature à en avoir eu besoin, pour son `request_repaint`) est
/// devenue un panneau fixe — voir [`PlotyxApp::script_panel`].
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
        Tab::Matrix(id) => matrix_contents(ui, state, id),
        Tab::Note(id) => note_contents(ui, state, actions, id),
    }
}

/// Enrobe [`graph_contents`] d'une zone de dépôt pour le glisser d'une
/// sélection de colonnes de classeur — pendant, côté cible, de la zone de
/// prise posée par [`grid_contents`] en bord de grille. Appelée par
/// [`tab_contents`], donc identiquement pour un onglet en fenêtre flottante
/// et pour un onglet détaché ([`PlotyxApp::show_detached_tabs`]) : c'est le
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
    graph_contents(ui, state, graph_views, graph_id);
    if hovering {
        ui.painter().rect_stroke(
            tab_rect,
            4.0,
            egui::Stroke::new(2.5, theme::header_letter_row_selected_fill()),
            egui::StrokeKind::Inside,
        );
    }
}

/// Le panneau Project Manager lui-même : l'arbre du projet
/// ([`AppState::project_tree`]), dossiers dépliables et fenêtres cliquables.
/// Cliquer une fenêtre pousse l'onglet qui l'ouvre dans `open_requests` (une
/// ouverture ou une focalisation, tranchée par [`PlotyxApp::open_tab`] après
/// le rendu). Fonction libre plutôt que méthode : appelée à la fois depuis le
/// panneau fixe, la zone de maximisation, et la fenêtre OS séparée — trois
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
        ColumnType::Mixed => "mixte",
    }
}

/// Suffixe affiché entre parenthèses après la lettre de colonne pour rappeler
/// son rôle de tracé ([`plotyx_core::ColumnRole`]) — `A(x)`, `B(y)`… Ce rôle
/// n'est pour l'instant lu par rien d'autre que la persistance
/// (`plotyx-persistence`) : aucune résolution de série ne s'en sert encore
/// (voir `plotyx_core::resolve`, qui détermine X/Y autrement) — l'afficher et
/// le rendre modifiable ici n'a donc, pour l'instant, qu'une valeur
/// informative, en avance sur son câblage au rendu.
fn role_suffix(role: ColumnRole) -> &'static str {
    match role {
        ColumnRole::X => "x",
        ColumnRole::Y => "y",
        ColumnRole::Z => "z",
        ColumnRole::XError => "xerr",
        ColumnRole::YError => "yerr",
        ColumnRole::Label => "label",
        ColumnRole::None => "aucun",
    }
}

/// Lettre de colonne façon tableur, en base 26 : `0 → "A"`, `25 → "Z"`,
/// `26 → "AA"`, `701 → "ZZ"`… — l'ordre de la feuille, pas un nom stocké
/// dans le modèle.
///
/// Simple alias de [`plotyx_core::spreadsheet_name`], qui portait déjà cet
/// algorithme (elle nomme les colonnes créées sans nom explicite). Ce module
/// en avait sa propre copie, de sorte que la lettre affichée en en-tête et
/// celle qui sert de nom par défaut à une colonne neuve venaient de deux
/// implémentations distinctes — et, depuis que les libellés d'historique
/// désignent eux aussi une colonne par sa lettre
/// (`plotyx_commands::Command::label`), il en aurait fallu une troisième.
fn column_letter(index: usize) -> String {
    plotyx_core::spreadsheet_name(index)
}

/// Vrai si `column` est la dernière colonne visible de `sheet` — condition
/// commune aux deux endroits où Tab doit ajouter une colonne plutôt que
/// revenir en début de ligne suivante (voir `Action::AddColumnAndSelect`) :
/// le raccourci clavier global de `grid_contents` et la validation d'édition
/// de `cell_widget`.
fn is_last_visible_column(state: &AppState, sheet: NodeId, column: NodeId) -> bool {
    state
        .visible_columns(sheet)
        .last()
        .is_some_and(|(last, ..)| *last == column)
}

/// Contenu du presse-papier **système**, lu à la demande — pour « Coller »
/// pris dans un menu contextuel, qui n'a pas d'`egui::Event::Paste` à se
/// mettre sous la dent.
///
/// Ctrl+V fonctionne sans ça parce qu'`egui-winit` lit lui-même le
/// presse-papier et pousse le texte dans l'événement ; egui, lui, n'expose
/// aucune API de **lecture** (seulement `Context::copy_text`, en écriture).
/// D'où cette lecture directe par `arboard` — la bibliothèque qu'`egui-winit`
/// utilise déjà en interne, donc déjà présente dans l'arbre de dépendances.
///
/// Une nouvelle poignée à chaque appel, plutôt qu'une gardée en vie : c'est
/// l'**écriture** qui exige sous X11 de garder le processus propriétaire du
/// presse-papier vivant, jamais la lecture, qui est un aller-retour unique.
/// Et un menu s'ouvre trop rarement pour que ce coût compte — raison pour
/// laquelle « Coller » n'est d'ailleurs jamais grisé : savoir s'il y a
/// quelque chose à coller demanderait cette lecture à *chaque image* où le
/// menu est ouvert. On tente, et on le dit si ça ne donne rien.
///
/// `None` si le presse-papier est inaccessible (pas de serveur d'affichage,
/// portail refusé…) ou s'il ne contient pas de texte (une image, par
/// exemple).
fn system_clipboard_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// Bloc d'édition commun à tous les menus contextuels de la grille : les
/// raccourcis clavier de [`grid_contents`], contextualisés à ce que la
/// sélection courante touche réellement (voir [`AppState::grid_menu`] et
/// [`GridMenu`]).
///
/// Le même bloc, dans le même ordre, sur les trois zones — cellule de
/// données, champ d'en-tête, rangée des lettres : seuls les libellés
/// changent, et « Supprimer et décaler » disparaît dans l'en-tête, où Suppr
/// n'a rien à faire remonter. Le menu de la rangée des lettres y ajoute
/// ensuite ce qui lui est propre (type et rôle de colonne, voir
/// [`column_header_widget`]).
///
/// N'ouvre rien si [`AppState::grid_menu`] rend `None` (plus de sélection
/// valide) : l'appelant doit alors s'abstenir de poser un menu, plutôt que
/// d'en poser un vide.
fn grid_edit_menu(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    menu: &GridMenu,
) {
    let ctx = ui.ctx().clone();

    if ui
        .add(egui::Button::new("Couper").shortcut_text(ctx.format_shortcut(&SHORTCUT_CUT)))
        .clicked()
    {
        // Couper = copier puis vider, exactement comme le bloc clavier :
        // `copy_selection` pose le presse-papier interne *et* rend le texte
        // à pousser au presse-papier système.
        if let Some(text) = state.copy_selection(sheet) {
            ctx.copy_text(text);
        }
        state.forget_delete_repeat(sheet);
        actions.push(Action::ClearSelectedCells { sheet });
        ui.close();
    }
    if ui
        .add(egui::Button::new("Copier").shortcut_text(ctx.format_shortcut(&SHORTCUT_COPY)))
        .clicked()
    {
        if let Some(text) = state.copy_selection(sheet) {
            ctx.copy_text(text);
        }
        ui.close();
    }
    if ui
        .add(
            egui::Button::new("Copier avec l'en-tête")
                .shortcut_text(ctx.format_shortcut(&SHORTCUT_COPY_FULL_COLUMNS)),
        )
        .clicked()
    {
        if let Some(text) = state.copy_full_columns(sheet) {
            ctx.copy_text(text);
        }
        ui.close();
    }

    ui.separator();

    if ui
        .add(egui::Button::new("Coller").shortcut_text(ctx.format_shortcut(&SHORTCUT_PASTE)))
        .clicked()
    {
        state.forget_delete_repeat(sheet);
        match system_clipboard_text() {
            Some(text) => actions.push(Action::Paste { sheet, text }),
            None => state.notice("Presse-papier vide ou illisible".to_string()),
        }
        ui.close();
    }
    if ui
        .add_enabled(
            menu.paste_absolute_enabled,
            egui::Button::new("Coller à l'identique")
                .shortcut_text(ctx.format_shortcut(&SHORTCUT_PASTE_ABSOLUTE)),
        )
        .on_disabled_hover_text("Copiez d'abord depuis la grille : un texte externe n'a pas de position d'origine à rejouer.")
        .clicked()
    {
        state.forget_delete_repeat(sheet);
        actions.push(Action::PasteAbsolute { sheet });
        ui.close();
    }

    ui.separator();

    if ui
        .add(egui::Button::new("Vider").shortcut_text(SHORTCUT_TEXT_BACKSPACE))
        .clicked()
    {
        state.forget_delete_repeat(sheet);
        actions.push(Action::ClearSelectedCells { sheet });
        ui.close();
    }
    if menu.offers_delete_shift
        && ui
            .add(
                egui::Button::new("Supprimer et décaler").shortcut_text(SHORTCUT_TEXT_DELETE),
            )
            .on_hover_text(
                "Suppr vide à la première pression et ne supprime qu'à la seconde ; cette entrée supprime tout de suite.",
            )
            .clicked()
    {
        // Passer par l'action directement, sans `consume_delete_key` : un
        // menu contextuel est un geste explicite et désigné, là où la
        // double pression sur Suppr n'existe que pour protéger d'une frappe
        // réflexe. Le compteur en cours est quand même oublié, pour qu'une
        // pression sur Suppr juste après ne parte pas du mauvais temps.
        state.forget_delete_repeat(sheet);
        actions.push(Action::DeleteSelectedCells { sheet });
        ui.close();
    }

    ui.separator();

    if ui
        .add_enabled(
            state.can_undo(),
            egui::Button::new("Annuler").shortcut_text(ctx.format_shortcut(&SHORTCUT_UNDO)),
        )
        .clicked()
    {
        actions.push(Action::Undo);
        ui.close();
    }
    if ui
        .add_enabled(
            state.can_redo(),
            egui::Button::new("Rétablir").shortcut_text(ctx.format_shortcut(&SHORTCUT_REDO_Y)),
        )
        .clicked()
    {
        actions.push(Action::Redo);
        ui.close();
    }
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
        // Sans ceci, l'espacement par défaut d'egui entre enfants d'un
        // `vertical` (`Spacing::item_spacing.y`) laisse un mince filet blanc
        // entre chaque rangée empilée — la couleur de fond ne remplit alors
        // pas vraiment la case, elle y flotte.
        ui.spacing_mut().item_spacing.y = 0.0;
        let letter = column_letter(column_index);
        let is_column_selected = state.is_column_selected(sheet, column);
        let touched = state.column_touched_by_selection(sheet, column);
        let role = state.column_meta(column).role;
        ui.scope(|ui| {
            ui.set_min_height(HEADER_SUBROW_HEIGHT);
            ui.set_max_height(HEADER_SUBROW_HEIGHT);
            let rect = ui.available_rect_before_wrap();
            ui.painter()
                .rect_filled(rect, 0.0, gutter_fill(is_column_selected, touched));
            header_row_separator(ui, rect);
            // Toute la largeur de la rangée sélectionne la colonne — pas
            // seulement la lettre —, glisser ou Maj-clic étend à plusieurs
            // colonnes (même logique que la multi-sélection de cellules de
            // données, voir `cell_widget`), au clic principal comme au clic
            // secondaire (`response.secondary_clicked()` couvre aussi
            // `Sense::click_and_drag()`) : le clic droit doit pouvoir
            // sélectionner *et* ouvrir le menu contextuel en un seul geste,
            // sans exiger que la colonne soit déjà sélectionnée au
            // préalable.
            let id = ui.id().with(("plotyx_column_letter_row", column.raw()));
            let response = ui.interact(rect, id, egui::Sense::click_and_drag());
            ui.put(
                rect,
                egui::Label::new(
                    egui::RichText::new(format!("{letter}({})", role_suffix(role).to_uppercase()))
                        .color(crate::theme::gutter_text_color()),
                )
                .selectable(false),
            );
            let shift = ui.input(|i| i.modifiers.shift);
            if response.drag_started() || (response.clicked() && !shift) {
                state.select_column(sheet, column);
            } else if response.clicked() && shift {
                state.extend_column_selection(sheet, column);
            } else if response.secondary_clicked() && !is_column_selected {
                // `&& !is_column_selected` : un clic droit *dans* une plage
                // déjà posée doit la laisser intacte, seul un clic droit en
                // dehors la réduit à cette colonne. Sans cette garde, le
                // clic droit réduisait **toujours** la plage à une seule
                // colonne — ce que le commentaire de `target_columns`
                // ci-dessous supposait pourtant déjà faux : changer le type
                // ou le rôle de trois colonnes sélectionnées d'un coup était
                // donc impossible, la sélection s'étant effondrée entre le
                // clic et l'ouverture du menu. Même règle que dans
                // `cell_widget`/`header_field_widget`/`row_number_widget`.
                state.select_column(sheet, column);
            } else if response.contains_pointer()
                && ui.input(|i| i.pointer.primary_down())
                && state.selection(sheet).is_some()
            {
                // Même geste que pour les cellules de données : étendre
                // pendant le glisser, pas seulement au clic de départ (voir
                // le commentaire détaillé de `cell_widget`).
                state.extend_column_selection(sheet, column);
            }
            // Colonnes visées par le menu contextuel : toute la plage
            // sélectionnée si `column` en fait partie (menu ouvert sur une
            // sélection multiple déjà posée), sinon `column` seule — un
            // clic droit hors de la plage sélectionnée l'a déjà réduite à
            // elle-même ci-dessus.
            let target_columns: Vec<NodeId> = state
                .selection(sheet)
                .map(|selection| selection.columns(state.tree(), sheet))
                .filter(|columns| columns.contains(&column))
                .unwrap_or_else(|| vec![column]);
            response.context_menu(|ui| {
                // Le bloc d'édition d'abord (les gestes du quotidien, les
                // mêmes que sur une cellule ou un champ d'en-tête), puis ce
                // qui n'existe que sur la rangée des lettres : type et rôle
                // de colonne, qui décrivent la colonne plutôt qu'ils
                // n'agissent sur son contenu. Calcul paresseux, pour la
                // même raison que dans `cell_widget`.
                if let Some(menu) = state.grid_menu(sheet) {
                    grid_edit_menu(ui, state, actions, sheet, &menu);
                    ui.separator();
                }
                // Choix manuel du type de colonne — conversion destructrice
                // assumée (`Action::SetColumnType`/`Command::
                // SetColumnType`, `plotyx-core::Column::set_column_type`) :
                // texte non numérique effacé en passant en `Numeric`, comme
                // pour toute saisie directe dans une colonne numérique.
                for candidate in [ColumnType::Numeric, ColumnType::Mixed] {
                    if ui
                        .selectable_label(column_type == candidate, type_suffix(candidate))
                        .clicked()
                    {
                        if candidate != column_type {
                            for &target in &target_columns {
                                actions.push(Action::SetColumnType {
                                    column: target,
                                    column_type: candidate,
                                });
                            }
                        }
                        ui.close();
                    }
                }
                // Rôle de tracé, dans un sous-menu séparé : encore sans
                // effet sur la résolution de série (voir `role_suffix`),
                // mais un seul et même endroit pour tout ce qui décrit une
                // colonne à l'utilisateur.
                ui.menu_button("Rôle", |ui| {
                    for candidate in [
                        ColumnRole::X,
                        ColumnRole::Y,
                        ColumnRole::Z,
                        ColumnRole::XError,
                        ColumnRole::YError,
                        ColumnRole::Label,
                        ColumnRole::None,
                    ] {
                        if ui
                            .selectable_label(role == candidate, role_suffix(candidate))
                            .clicked()
                        {
                            if candidate != role {
                                for &target in &target_columns {
                                    actions.push(Action::SetColumnRole {
                                        column: target,
                                        role: candidate,
                                    });
                                }
                            }
                            ui.close();
                        }
                    }
                });
                // Créer un graphe depuis les colonnes visées — chantier
                // « tracer depuis un classeur ». La règle d'appariement X/Y
                // (chaque Y prend la colonne X la plus proche à sa gauche)
                // vit dans `AppState::plot_columns`/`series_from_columns` ;
                // ce menu ne fait que choisir le type de tracé et pousser
                // l'action sur `target_columns`, comme les deux sous-menus
                // ci-dessus.
                ui.menu_button("Tracer", |ui| {
                    for (label, plot_type) in [
                        ("Ligne", PlotType::Line),
                        ("Points", PlotType::Scatter),
                        ("Ligne + points", PlotType::LineScatter),
                        ("Barres", PlotType::Bar),
                    ] {
                        if ui.button(label).clicked() {
                            actions.push(Action::PlotColumns {
                                sheet,
                                columns: target_columns.clone(),
                                plot_type,
                            });
                            ui.close();
                        }
                    }
                });
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

/// Fond d'une case de la gouttière de la grille, à trois paliers — voir
/// [`crate::theme::header_letter_row_fill`]/[`crate::theme::
/// header_letter_row_touched_fill`]/[`crate::theme::
/// header_letter_row_selected_fill`] : repos, concernée par la sélection en
/// cours sans l'être directement, ou sélectionnée elle-même (le palier le
/// plus fort l'emporte). Partagé par [`column_header_widget`], [`row_number_
/// widget`], [`header_row_label_widget`] et le « # » de [`row_number_header_
/// widget`].
fn gutter_fill(selected: bool, touched: bool) -> egui::Color32 {
    if selected {
        crate::theme::header_letter_row_selected_fill()
    } else if touched {
        crate::theme::header_letter_row_touched_fill()
    } else {
        crate::theme::header_letter_row_fill()
    }
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
    // `actions` ne sert pas à la validation de la saisie — elle passe par
    // `AppState::commit_header_edit`, appelé directement (mutation
    // transitoire, comme `commit_edit`) — mais bien au menu contextuel
    // ci-dessous, qui pousse des actions comme partout ailleurs.
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
                    // `consume_key`, pas `key_pressed` : Tab valide *et*
                    // déplace la sélection tout de suite (`move_selection_tab`/
                    // `AddColumnAndSelectHeaderField` ci-dessous), avant que
                    // le bloc de raccourcis clavier de `grid_contents` (qui
                    // tourne après, une fois l'édition refermée par ce
                    // `return`) ne revoie passer le même Tab — même piège
                    // que `cell_widget`.
                    let shift = ui.input(|i| i.modifiers.shift);
                    let tab = ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Tab)
                    });
                    state.commit_header_edit();
                    if tab
                        && !shift
                        && field == HeaderField::Formula
                        && is_last_visible_column(state, sheet, column)
                    {
                        // Tab valide sur le champ Formule de la dernière
                        // colonne : même geste que Tab en dernière colonne
                        // d'une ligne de données — ajoute une colonne plutôt
                        // que d'entrer dans les données (voir
                        // `Action::AddColumnAndSelectHeaderField`).
                        actions.push(Action::AddColumnAndSelectHeaderField { sheet, field });
                    } else if tab {
                        state.move_selection_tab(sheet, shift);
                    }
                }
            }
            return;
        }
        let text = header_field_text(state, column, field);
        let id = ui.id().with((column.raw(), field as u8));
        let response = ui.interact(rect, id, egui::Sense::click_and_drag());
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
            return;
        }
        // Même logique de multi-sélection à la souris que pour les
        // cellules de données (clic/glisser/Maj-clic) — voir le
        // commentaire détaillé de `cell_widget`.
        let shift = ui.input(|i| i.modifiers.shift);
        let dragging = response.contains_pointer()
            && ui.input(|i| i.pointer.primary_down())
            && state.selection(sheet).is_some();
        if response.drag_started() || (response.clicked() && !shift) {
            state.select_header_field(sheet, column, field);
        } else if (response.clicked() && shift) || dragging {
            state.extend_header_field_selection(sheet, column, field);
        } else if response.secondary_clicked() && !selected {
            // Même règle que la rangée des lettres : un clic droit *dans*
            // la sélection ouvre le menu sur elle sans la toucher ; un clic
            // droit en dehors la réduit d'abord à la case visée, pour que
            // le menu agisse sur ce qu'on vient de désigner et pas sur une
            // sélection oubliée ailleurs.
            state.select_header_field(sheet, column, field);
        }
        // Calcul paresseux, pour la même raison que dans `cell_widget`.
        response.context_menu(|ui| {
            if let Some(menu) = state.grid_menu(sheet) {
                grid_edit_menu(ui, state, actions, sheet, &menu);
            }
        });
    });
}

/// Gouttière des numéros de ligne, côté en-tête empilée : le « # » sur la
/// rangée des lettres — un clic sélectionne tout le tableau (voir
/// [`AppState::select_all`]), même geste qu'un clic sur une lettre de
/// colonne ou un numéro de ligne mais pour l'ensemble — puis, aligné avec
/// chacune des quatre rangées de champs de [`column_header_widget`], le
/// **libellé** du champ concerné (« Long Name », « Units », « Comments »,
/// « F(x)= », repris de l'ancienne UI Qt), une seule fois plutôt que répété
/// en grisé dans chaque colonne. Les cellules par colonne n'affichent plus
/// que la valeur (voir [`header_field_widget`]).
///
/// Homogène de haut en bas avec le reste de la gouttière : chaque rangée
/// porte le même fond que le « # » ([`gutter_fill`]) — les quatre rangées de
/// libellés restaient auparavant transparentes, seul le « # » du dessus
/// rompant le fond de la colonne. Chaque libellé sélectionne désormais son
/// champ pour **toutes** les colonnes visibles de la feuille (glisser/
/// Maj-clic étend à plusieurs champs), symétrique de [`row_number_widget`]
/// côté données — voir [`header_row_label_widget`].
fn row_number_header_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.scope(|ui| {
            ui.set_min_height(HEADER_SUBROW_HEIGHT);
            ui.set_max_height(HEADER_SUBROW_HEIGHT);
            let selected = state.is_all_selected(sheet);
            let rect = ui.available_rect_before_wrap();
            ui.painter()
                .rect_filled(rect, 0.0, gutter_fill(selected, false));
            header_row_separator(ui, rect);
            let id = ui.id().with("plotyx_select_all");
            let response = ui.interact(rect, id, egui::Sense::click());
            ui.put(
                rect,
                egui::Label::new(
                    egui::RichText::new("#")
                        .strong()
                        .color(crate::theme::gutter_text_color()),
                )
                .selectable(false),
            );
            if response.clicked() || (response.secondary_clicked() && !selected) {
                // `&& !selected` : un clic droit alors que tout est déjà
                // sélectionné ouvre le menu sans y toucher — même règle que
                // dans les autres cases de la gouttière (voir
                // `column_header_widget`).
                state.select_all(sheet);
            }
            response.context_menu(|ui| {
                if let Some(menu) = state.grid_menu(sheet) {
                    grid_edit_menu(ui, state, actions, sheet, &menu);
                }
            });
        });
        header_row_label_widget(
            ui,
            state,
            actions,
            sheet,
            HeaderField::LongName,
            HEADER_LONGNAME_HEIGHT,
            "Long Name",
        );
        header_row_label_widget(
            ui,
            state,
            actions,
            sheet,
            HeaderField::Unit,
            HEADER_SUBROW_HEIGHT,
            "Units",
        );
        header_row_label_widget(
            ui,
            state,
            actions,
            sheet,
            HeaderField::Comment,
            HEADER_SUBROW_HEIGHT,
            "Comments",
        );
        header_row_label_widget(
            ui,
            state,
            actions,
            sheet,
            HeaderField::Formula,
            HEADER_SUBROW_HEIGHT,
            "F(x)=",
        );
    });
}

/// Une rangée de la gouttière côté en-tête (voir [`row_number_header_widget`]) :
/// le libellé d'un champ, à la hauteur exacte de la rangée de
/// [`column_header_widget`] qu'elle longe — et, depuis que la sélection de
/// champ entier existe, un vrai geste de sélection, miroir exact de
/// [`row_number_widget`] pour les lignes de données et de
/// [`column_header_widget`] pour les colonnes : toute la largeur du libellé
/// sélectionne le champ pour **toutes** les colonnes visibles de la feuille
/// (glisser/Maj-clic étend à plusieurs champs), le clic droit ouvre le même
/// menu contextuel d'édition ([`grid_edit_menu`]).
fn header_row_label_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    field: HeaderField,
    height: f32,
    label: &str,
) {
    ui.scope(|ui| {
        ui.set_min_height(height);
        ui.set_max_height(height);
        let selected = state.is_header_row_selected(sheet, field);
        let rect = ui.available_rect_before_wrap();
        ui.painter()
            .rect_filled(rect, 0.0, gutter_fill(selected, false));
        header_row_separator(ui, rect);
        let id = ui.id().with(("plotyx_header_row_label", field as u8));
        let response = ui.interact(rect, id, egui::Sense::click_and_drag());
        ui.put(
            rect,
            egui::Label::new(
                egui::RichText::new(label)
                    .strong()
                    .color(crate::theme::gutter_text_color()),
            )
            .selectable(false),
        );
        let shift = ui.input(|i| i.modifiers.shift);
        if response.drag_started() || (response.clicked() && !shift) {
            state.select_header_row(sheet, field);
        } else if response.clicked() && shift {
            state.extend_header_row_selection(sheet, field);
        } else if response.contains_pointer()
            && ui.input(|i| i.pointer.primary_down())
            && state.selection(sheet).is_some()
        {
            // Même geste que pour les cellules de données et la rangée des
            // lettres — étendre pendant le glisser, pas seulement au clic de
            // départ (voir le commentaire détaillé de `cell_widget`).
            state.extend_header_row_selection(sheet, field);
        } else if response.secondary_clicked() && !selected {
            // Clic droit hors de la sélection : la réduire à ce champ avant
            // d'ouvrir le menu — même règle que dans les trois autres zones.
            state.select_header_row(sheet, field);
        }
        // `grid_menu` **dans** la fermeture, jamais avant — même raison que
        // dans `cell_widget`/`row_number_widget` : calculée dehors, elle
        // serait recalculée à chaque image pour chacune des 4 rangées.
        response.context_menu(|ui| {
            if let Some(menu) = state.grid_menu(sheet) {
                grid_edit_menu(ui, state, actions, sheet, &menu);
            }
        });
    });
}

/// Valeur actuelle d'un champ d'en-tête, telle qu'affichée hors édition.
fn header_field_text(state: &AppState, column: NodeId, field: HeaderField) -> String {
    state.header_field_display_text(column, field)
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
    // Sans ceci, l'espacement vertical par défaut d'egui entre les rangées
    // de la table (`Spacing::item_spacing.y`, dont `TableBuilder` se sert en
    // interne pour séparer les lignes) laisse un mince filet blanc entre
    // chacune — la couleur de fond de la gouttière ne remplit alors pas
    // vraiment la case, elle y flotte (numéros de ligne notamment,
    // sensibles à l'œil car peints d'un aplat plein plutôt qu'un simple
    // texte). `item_spacing.x` reste intact : il porte l'espacement entre
    // colonnes, dont dépendent les poignées de redimensionnement.
    ui.spacing_mut().item_spacing.y = 0.0;
    // Capturé *avant* la table, dont `.auto_shrink([false, false])` occupe
    // justement tout cet espace disponible : sert de référence à la zone de
    // prise du glisser-déposer de colonnes plus bas ([`grid_grip_side`]).
    let grid_rect = ui.available_rect_before_wrap();
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
                row_number_header_widget(ui, state, actions, sheet);
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
                    row_number_widget(ui, state, actions, sheet, index);
                });
                for (id, _, _) in &columns {
                    row.col(|ui| {
                        cell_widget(ui, state, actions, sheet, *id, index);
                    });
                }
            });
        });

    // Glisser une sélection de colonnes vers un graphe ouvert (chantier
    // « glisser une sélection vers un graphe », suite du menu contextuel
    // « Tracer » — voir `docs/application.md`). La zone de prise est une
    // mince bande sur le bord *extérieur* gauche ou droit de la grille
    // entière (pas de chaque colonne) : `left_rect`/`right_rect` ci-dessous
    // sont volontairement confinés à `COLUMN_GRIP_WIDTH` plutôt que de
    // couvrir `grid_rect` en entier — un `ui.interact` sur tout `grid_rect`
    // shadowerait le glisser d'extension de sélection de `cell_widget`
    // partout dans la grille, pas seulement sur ses bords, ce que la tâche
    // interdit explicitement. N'apparaît que si `sheet` a déjà une sélection
    // couvrant au moins une colonne ([`AppState::column_drag_payload`]) :
    // sans ça, pas de zone de prise à afficher ni de payload à glisser.
    if let Some(payload) = state.column_drag_payload(sheet) {
        let margin = COLUMN_GRIP_WIDTH;
        let left_rect = egui::Rect::from_min_max(
            grid_rect.left_top(),
            egui::pos2(grid_rect.left() + margin, grid_rect.bottom()),
        );
        let right_rect = egui::Rect::from_min_max(
            egui::pos2(grid_rect.right() - margin, grid_rect.top()),
            grid_rect.right_bottom(),
        );
        let left_id = ui.id().with(("column_grip_left", sheet));
        let right_id = ui.id().with(("column_grip_right", sheet));
        let left_response = ui.interact(left_rect, left_id, egui::Sense::drag());
        let right_response = ui.interact(right_rect, right_id, egui::Sense::drag());
        // `grid_grip_side` reprend la même géométrie que `left_rect`/
        // `right_rect` ci-dessus, mais en fonction pure testée hors d'egui
        // (`tests/shell.rs`) — décision unique, pas refaite en filigrane via
        // `.hovered()`. Le bord exact (gauche ou droite) ne change rien à
        // l'icône affichée, seule sa présence compte ici.
        let pointer = ui.ctx().pointer_hover_pos();
        let grip = pointer.and_then(|pointer| {
            grid_grip_side(
                (
                    grid_rect.left(),
                    grid_rect.top(),
                    grid_rect.right(),
                    grid_rect.bottom(),
                ),
                (pointer.x, pointer.y),
                margin,
            )
        });
        if let (Some(_), Some(pointer)) = (grip, pointer) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            paint_drag_handle_icon(
                ui.painter(),
                pointer + egui::vec2(12.0, -12.0),
                theme::gutter_text_color(),
            );
        }
        if left_response.drag_started() {
            left_response.dnd_set_drag_payload(payload.clone());
        } else if right_response.drag_started() {
            right_response.dnd_set_drag_payload(payload);
        }
    }

    // Raccourcis clavier de la grille — voir la doc de tête de cette
    // fonction pour la garde d'édition.
    //
    // `state.import_state().is_none()` est la troisième condition de cette
    // garde, ajoutée avec la refonte de la fenêtre d'import : `consume_key`
    // ci-dessous agit sur l'`InputState` du `Context` entier, pas sur celui
    // d'une `Ui` — donc pas seulement sur cette grille. Tant que la fenêtre
    // d'import (un `egui::Window` flottant, peint *après* les fenêtres
    // flottantes de contenu — voir `PlotyxApp::update`) est ouverte, cette
    // grille continue d'être peinte
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
        if tab && !shift {
            // Tab sur la dernière colonne d'une ligne de données, ou sur le
            // champ Formule (le dernier des quatre) de la dernière colonne
            // en en-tête : même geste — ajoute une colonne plutôt que de
            // revenir en début de ligne suivante ou d'entrer dans les
            // données, puis sélectionne tout de suite la case qui y
            // correspondra (voir `Action::AddColumnAndSelect`/
            // `Action::AddColumnAndSelectHeaderField`, qui ne peuvent la
            // poser qu'une fois la colonne réellement créée, contrairement à
            // une ligne). Les trois autres champs d'en-tête (Long Name,
            // Units, Comments) restent inchangés : Tab y circule d'abord
            // colonne par colonne pour un même champ, y compris sur la
            // dernière colonne — seul Formule marque la vraie fin de
            // l'en-tête.
            let cursor = state.selection(sheet).and_then(Selection::grid_cursor);
            let needs_new_column = match cursor {
                Some(GridCursor::Data(column, _)) => is_last_visible_column(state, sheet, column),
                Some(GridCursor::Header(column, HeaderField::Formula)) => {
                    is_last_visible_column(state, sheet, column)
                }
                _ => false,
            };
            if needs_new_column {
                match cursor {
                    Some(GridCursor::Data(_, row)) => {
                        actions.push(Action::AddColumnAndSelect { sheet, row });
                    }
                    Some(GridCursor::Header(_, field)) => {
                        actions.push(Action::AddColumnAndSelectHeaderField { sheet, field });
                    }
                    None => {}
                }
            } else {
                state.move_selection_tab(sheet, false);
            }
        } else if tab {
            state.move_selection_tab(sheet, true);
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
            let data_cursor = state
                .selection(sheet)
                .and_then(Selection::grid_cursor)
                .and_then(|cursor| match cursor {
                    GridCursor::Data(column, row) => Some((column, row)),
                    GridCursor::Header(..) => None,
                });
            let at_last_row = data_cursor.is_some_and(|(_, row)| row + 1 >= state.row_count(sheet));
            if at_last_row {
                if let Some((column, row)) = data_cursor {
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
        // ordinaire (donc jamais via `consume_key`/`consume_shortcut`) :
        // `egui-winit` les intercepte lui-même à la traduction des
        // événements de la fenêtre et pousse à la place un `Event::Copy`/
        // `Event::Cut`/`Event::Paste(String)` (ce dernier porte déjà le
        // contenu du presse-papier système, lu par `egui-winit` via
        // `arboard` en interne — inutile de le relire nous-mêmes). Ça vaut
        // aussi pour Ctrl+Maj+C/Ctrl+Maj+V : `egui-winit::is_copy_command`/
        // `is_paste_command` ne testent que `modifiers.command`, jamais
        // `shift` (voir `egui-winit/src/lib.rs`), donc Maj enfoncée ou non
        // produit le **même** `Event::Copy`/`Event::Paste` — jamais un
        // `Event::Key` à capturer par un raccourci séparé (un essai
        // précédent le supposait à tort : `consume_shortcut` sur un
        // raccourci Ctrl+Maj+C/V ne matchait jamais). On distingue donc les
        // deux en lisant `input.modifiers.shift` au moment de l'événement —
        // toujours à jour à ce point, `ModifiersChanged` précédant la
        // frappe de la touche elle-même. Retour arrière/Suppr, eux, n'ont
        // pas de modificateur et restent des `Event::Key` ordinaires,
        // capturés via `consume_key` comme avant. Suppr est le seul des
        // deux à ne pas dispatcher une action fixe : `AppState::
        // consume_delete_key` décide si c'est un vidage ou une suppression
        // selon qu'il s'agit ou non de la deuxième pression consécutive sur
        // la même sélection (voir sa doc).
        let (mut copy, mut cut, mut paste_text, mut copy_full_columns, mut paste_absolute) =
            (false, false, None, false, false);
        ui.input(|input| {
            let shift = input.modifiers.shift;
            for event in &input.events {
                match event {
                    egui::Event::Copy if shift => copy_full_columns = true,
                    egui::Event::Copy => copy = true,
                    egui::Event::Cut => cut = true,
                    egui::Event::Paste(_) if shift => paste_absolute = true,
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
        // Toute la matrice de comportement (une seule case copiée devient du
        // texte brut, un rectangle devient relatif, Ctrl+Maj+C promeut les
        // colonnes touchées en entier, un collage se décale sur l'ancre ou
        // rejoue ses rangs d'origine…) vit dans `AppState` — voir sa doc de
        // tête, section copier/coller. Cette couche ne fait que traduire
        // clavier/souris en appels, la même façon quelle que soit la sorte
        // de sélection courante : plus de branche séparée pour un champ
        // d'en-tête sélectionné, `AppState::copy_selection`/`AppState::
        // paste`/`Action::ClearSelectedCells`/`Action::DeleteSelectedCells`
        // savent déjà tous distinguer en-tête et données par eux-mêmes.
        if copy_full_columns {
            if let Some(text) = state.copy_full_columns(sheet) {
                ui.ctx().copy_text(text);
            }
        } else if copy || cut {
            if let Some(text) = state.copy_selection(sheet) {
                ui.ctx().copy_text(text);
            }
        }
        if cut || backspace {
            // Une autre mutation de la grille s'intercale entre deux
            // pressions sur Suppr : la deuxième doit repartir au premier
            // temps (vider), pas supprimer par surprise — voir la doc de
            // `AppState::consume_delete_key`.
            state.forget_delete_repeat(sheet);
            actions.push(Action::ClearSelectedCells { sheet });
        }
        if delete {
            if let Some(action) = state.consume_delete_key(sheet) {
                actions.push(action);
            }
        }
        if paste_absolute {
            state.forget_delete_repeat(sheet);
            actions.push(Action::PasteAbsolute { sheet });
        } else if let Some(text) = paste_text {
            state.forget_delete_repeat(sheet);
            actions.push(Action::Paste { sheet, text });
        }
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

/// Peint l'icône « prise » du glisser-déposer de colonnes ([`grid_contents`])
/// à côté du curseur — une petite grille de points plutôt qu'un glyphe de
/// police : la mesure de texte de l'appli passe entièrement par
/// `plotyx_render::text` pour le rendu de graphe (voir `CLAUDE.md`, § Pièges
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
    // La figure prend tout ce qui reste après ses contrôles. Réserver leur
    // hauteur complète évite le même mécanisme d'auto-agrandissement qui a
    // déjà touché les fenêtres de classeur : un dépassement ferait changer
    // `rect` à chaque image, donc provoquerait un raté de cache au survol.
    let (outer, _) = ui.allocate_exact_size(
        egui::vec2(available.x, (available.y - GRAPH_CONTROLS_HEIGHT).max(0.0)),
        egui::Sense::hover(),
    );
    // La géométrie scientifique reste fixe, même si la fenêtre est trop
    // petite. Seule la partie visible est découpée. Le sous-Ui borne aussi
    // les interactions : une partie masquée ne doit pas capter la souris
    // sur les commandes voisines. Ne pas propager le rectangle de la figure
    // à la disposition du parent, sous peine de faire grandir la fenêtre.
    let (fig_w, fig_h) = DEFAULT_FIGURE_SIZE;
    let rect = egui::Rect::from_center_size(outer.center(), egui::vec2(fig_w as f32, fig_h as f32));
    let mut figure_ui = ui.new_child(egui::UiBuilder::new().max_rect(outer));
    figure_ui.set_clip_rect(ui.clip_rect().intersect(outer));
    match view.paint(&figure_ui, rect, snapshot, graph_id) {
        Ok(_) => {
            if ui.small_button("réinitialiser le zoom").clicked() {
                view.reset_view();
            }
        }
        Err(err) => {
            ui.colored_label(egui::Color32::RED, format!("rendu : {err}"));
        }
    }
}

/// Panneau de configuration des graphiques (chantier 3/3 de « tracer depuis
/// un classeur », voir `docs/application.md`) : édite le [`plotyx_plot::
/// PlotSpec`] du graphe actif au travers d'un brouillon transitoire
/// ([`crate::state::PlotEdit`]), jamais directement — même famille que
/// [`NoteEdit`]/`HeaderEdit`, généralisée à un objet entier plutôt qu'à un
/// seul champ (voir la doc de tête de `PlotEdit`).
///
/// **Stratégie anti-inflation d'historique** : chaque widget met à jour le
/// brouillon en direct (retour visuel immédiat), et `commit` — un simple
/// booléen local, mis à `true` par n'importe quel widget qui vient de finir
/// une interaction — ne déclenche [`AppState::commit_plot_edit`] qu'une
/// seule fois, à la fin de la fonction. Trois familles de widgets, trois
/// façons de finir une interaction :
/// - **texte** (titres, libellés, couleurs en `#RRGGBB`) : `lost_focus()`,
///   comme `note_contents`/`cell_widget` — jamais à chaque frappe ;
/// - **glissé numérique** (épaisseur de trait, taille de marqueur, bornes
///   d'axe) : `drag_stopped() || lost_focus()` — la valeur change à chaque
///   pixel de glisser (retour visuel), mais ne s'envoie qu'au relâchement ou
///   à la validation clavier ;
/// - **case à cocher / menu déroulant** (échelle, visibilité, type de
///   tracé, position de légende...) : un seul événement discret par clic,
///   validé tout de suite — rien à débouncer.
///
/// **Un seul panneau, pas de multi-panneaux** : `plotyx-plot` autorise
/// plusieurs [`plotyx_plot::PanelSpec`] par graphe (`row`/`column` dans une
/// grille), mais aucune UI de ce chantier n'en crée encore un second —
/// seul `PlotSpec::panels`[0] est donc éditable ici. Éditer le premier
/// panneau revient, pour l'instant, à éditer *le* graphe.
fn plot_config_panel(ui: &mut egui::Ui, state: &mut AppState, graph: NodeId) {
    // Cloné avant d'emprunter `state` en écriture pour le brouillon : un
    // `Arc<Tree>` cloné est un compteur de références, pas une copie de
    // l'arbre — voir `AppState::tree`.
    let tree = state.tree().clone();
    let Some(node) = tree.node(graph) else {
        state.close_plot_edit();
        ui.weak("Le graphe a été supprimé.");
        return;
    };
    let name = node.name().to_string();

    state.begin_plot_edit(graph);
    let Some(edit) = state.plot_edit_mut() else {
        return;
    };
    if edit.graph != graph {
        // Ne devrait pas arriver : `begin_plot_edit` vient de le garantir.
        // Filet plutôt qu'un `unwrap` — un brouillon désaccordé ne doit pas
        // faire paniquer le panneau (invariant 10).
        return;
    }

    let mut commit = false;

    ui.heading(&name);
    ui.separator();

    ui.strong("Graphe");
    let title_response = ui.horizontal(|ui| {
        ui.label("Titre");
        ui.text_edit_singleline(&mut edit.spec.title)
    });
    commit |= title_response.inner.lost_focus();

    ui.add_space(6.0);
    ui.separator();

    let Some(panel) = edit.spec.panels.get_mut(0) else {
        // Ne peut pas arriver : `Panels` garantit au moins un panneau par
        // construction (`plotyx-plot/src/spec.rs`). Filet, même raison.
        return;
    };

    ui.strong("Panneau (le premier — pas de multi-panneaux dans ce chantier)");
    let panel_title_response = ui.horizontal(|ui| {
        ui.label("Titre");
        ui.text_edit_singleline(&mut panel.title)
    });
    commit |= panel_title_response.inner.lost_focus();

    ui.add_space(6.0);
    ui.separator();
    ui.strong("Axes");
    commit |= axis_editor(ui, "axis-bottom", "Bas (X)", &mut panel.bottom);
    commit |= axis_editor(ui, "axis-left", "Gauche (Y)", &mut panel.left);
    if panel.uses_right_axis() {
        commit |= axis_editor(ui, "axis-right", "Droite (Y secondaire)", &mut panel.right);
    }

    ui.add_space(6.0);
    ui.separator();
    ui.strong("Séries");
    if panel.series.is_empty() {
        ui.weak("Ce panneau ne trace aucune série.");
    } else {
        if edit.selected_series >= panel.series.len() {
            edit.selected_series = panel.series.len() - 1;
        }
        egui::ScrollArea::vertical()
            .id_salt("plot_config_series_list")
            .max_height(140.0)
            .show(ui, |ui| {
                for (index, series) in panel.series.iter().enumerate() {
                    let label = series_list_label(&tree, series);
                    if ui
                        .selectable_label(edit.selected_series == index, label)
                        .clicked()
                    {
                        edit.selected_series = index;
                    }
                }
            });
        ui.separator();
        if let Some(series) = panel.series.get_mut(edit.selected_series) {
            commit |= series_editor(ui, series);
        }
    }

    ui.add_space(6.0);
    ui.separator();
    ui.strong("Légende");
    commit |= legend_editor(ui, &mut panel.legend);

    if commit {
        state.commit_plot_edit();
    }
}

/// Édite un [`AxisSpec`] : libellé, unité, échelle, visibilité, grille,
/// inversion, bornes. `id_salt` distingue les widgets des quatre axes
/// possibles d'un même panneau (`egui::ComboBox`/`CollapsingHeader` ont
/// besoin d'un identifiant stable, faute de quoi deux axes dépliés en même
/// temps se marcheraient dessus). Rend `true` si le brouillon doit être
/// validé — voir la doc de tête de [`plot_config_panel`].
fn axis_editor(ui: &mut egui::Ui, id_salt: &str, title: &str, axis: &mut AxisSpec) -> bool {
    let mut commit = false;
    egui::CollapsingHeader::new(title)
        .id_salt(id_salt)
        .default_open(false)
        .show(ui, |ui| {
            let label_response = ui.horizontal(|ui| {
                ui.label("Libellé");
                ui.text_edit_singleline(&mut axis.label)
            });
            commit |= label_response.inner.lost_focus();

            let unit_response = ui.horizontal(|ui| {
                ui.label("Unité");
                ui.text_edit_singleline(&mut axis.unit)
            });
            commit |= unit_response.inner.lost_focus();

            let mut log_scale = axis.scale == plotyx_plot::AxisScale::Log;
            if ui
                .checkbox(&mut log_scale, "Échelle logarithmique")
                .changed()
            {
                axis.scale = if log_scale {
                    plotyx_plot::AxisScale::Log
                } else {
                    plotyx_plot::AxisScale::Linear
                };
                commit = true;
            }
            commit |= ui.checkbox(&mut axis.visible, "Axe affiché").changed();
            commit |= ui.checkbox(&mut axis.grid, "Grille").changed();
            commit |= ui.checkbox(&mut axis.inverted, "Axe inversé").changed();
            commit |= ui
                .checkbox(&mut axis.auto_range, "Bornes automatiques")
                .changed();

            if !axis.auto_range {
                let mut min = axis.range_min.unwrap_or(0.0);
                let mut max = axis.range_max.unwrap_or(0.0);
                let min_response = ui.horizontal(|ui| {
                    ui.label("Min");
                    ui.add(egui::DragValue::new(&mut min).speed(0.1))
                });
                let max_response = ui.horizontal(|ui| {
                    ui.label("Max");
                    ui.add(egui::DragValue::new(&mut max).speed(0.1))
                });
                axis.range_min = Some(min);
                axis.range_max = Some(max);
                let min_inner = min_response.inner;
                let max_inner = max_response.inner;
                commit |= min_inner.drag_stopped() || min_inner.lost_focus();
                commit |= max_inner.drag_stopped() || max_inner.lost_focus();
            }
        });
    commit
}

/// Édite la série sélectionnée : nom, type de tracé, visibilité, présence en
/// légende, style de trait, style de marqueur. Rend `true` si le brouillon
/// doit être validé — voir la doc de tête de [`plot_config_panel`].
///
/// [`PlotType::Unknown`] (C5, `guide-de-correction-robustesse.md` §F3) ne
/// figure **jamais** dans la liste proposée : c'est le type de tracé d'une
/// version plus récente de PlotYx, et le proposer laisserait croire qu'on
/// peut y revenir alors que rien ne sait le reconstruire. Une série qui le
/// porte s'affiche quand même, en lecture seule pour ce seul champ — le
/// reste (nom, visibilité...) reste éditable normalement.
fn series_editor(ui: &mut egui::Ui, series: &mut plotyx_plot::SeriesSpec) -> bool {
    let mut commit = false;

    let name_response = ui.horizontal(|ui| {
        ui.label("Nom");
        ui.text_edit_singleline(&mut series.name)
    });
    commit |= name_response.inner.lost_focus();
    ui.weak("Nom vide = dérivé de la colonne tracée.");

    ui.horizontal(|ui| {
        ui.label("Type de tracé");
        match &series.plot_type {
            PlotType::Unknown(_) => {
                ui.weak("(type inconnu de cette version, non modifiable)");
            }
            current => {
                let mut selected = current.clone();
                egui::ComboBox::new("plot_config_series_type", "")
                    .selected_text(plot_type_label(&selected))
                    .show_ui(ui, |ui| {
                        for candidate in [
                            PlotType::Scatter,
                            PlotType::Line,
                            PlotType::LineScatter,
                            PlotType::Bar,
                            PlotType::Histogram,
                            PlotType::ErrorBar,
                        ] {
                            let text = plot_type_label(&candidate);
                            let is_selected = candidate == selected;
                            if ui.selectable_label(is_selected, text).clicked() {
                                selected = candidate;
                            }
                        }
                    });
                if selected != series.plot_type {
                    series.plot_type = selected;
                    commit = true;
                }
            }
        }
    });

    commit |= ui.checkbox(&mut series.visible, "Visible").changed();
    commit |= ui
        .checkbox(&mut series.show_in_legend, "Présente dans la légende")
        .changed();

    ui.separator();
    ui.label("Trait");
    let line_color_response = ui.horizontal(|ui| {
        ui.label("Couleur (#RRGGBB)");
        ui.text_edit_singleline(&mut series.line.color)
    });
    commit |= line_color_response.inner.lost_focus();

    ui.horizontal(|ui| {
        ui.label("Style");
        let mut style = series.line.style;
        egui::ComboBox::new("plot_config_line_style", "")
            .selected_text(line_style_label(style))
            .show_ui(ui, |ui| {
                for candidate in [
                    LineStyle::None,
                    LineStyle::Solid,
                    LineStyle::Dash,
                    LineStyle::Dot,
                    LineStyle::DashDot,
                ] {
                    ui.selectable_value(&mut style, candidate, line_style_label(candidate));
                }
            });
        if style != series.line.style {
            series.line.style = style;
            commit = true;
        }
    });

    let width_response = ui.horizontal(|ui| {
        ui.label("Épaisseur (pt)");
        ui.add(
            egui::DragValue::new(&mut series.line.width)
                .speed(0.1)
                .range(0.0..=20.0),
        )
    });
    let width_inner = width_response.inner;
    commit |= width_inner.drag_stopped() || width_inner.lost_focus();

    ui.separator();
    ui.label("Marqueur");
    ui.horizontal(|ui| {
        ui.label("Forme");
        let mut shape = series.symbol.shape;
        egui::ComboBox::new("plot_config_symbol_shape", "")
            .selected_text(symbol_shape_label(shape))
            .show_ui(ui, |ui| {
                for candidate in [
                    SymbolShape::None,
                    SymbolShape::Circle,
                    SymbolShape::Square,
                    SymbolShape::Triangle,
                    SymbolShape::Diamond,
                    SymbolShape::Plus,
                    SymbolShape::Cross,
                ] {
                    ui.selectable_value(&mut shape, candidate, symbol_shape_label(candidate));
                }
            });
        if shape != series.symbol.shape {
            series.symbol.shape = shape;
            commit = true;
        }
    });

    let size_response = ui.horizontal(|ui| {
        ui.label("Taille (pt)");
        ui.add(
            egui::DragValue::new(&mut series.symbol.size)
                .speed(0.1)
                .range(0.0..=40.0),
        )
    });
    let size_inner = size_response.inner;
    commit |= size_inner.drag_stopped() || size_inner.lost_focus();

    let symbol_color_response = ui.horizontal(|ui| {
        ui.label("Couleur (#RRGGBB, vide = celle du trait)");
        ui.text_edit_singleline(&mut series.symbol.color)
    });
    commit |= symbol_color_response.inner.lost_focus();

    commit
}

/// Édite un [`plotyx_plot::LegendSpec`] : visibilité, coin d'ancrage. Rend
/// `true` si le brouillon doit être validé — voir la doc de tête de
/// [`plot_config_panel`].
fn legend_editor(ui: &mut egui::Ui, legend: &mut plotyx_plot::LegendSpec) -> bool {
    let mut commit = false;
    commit |= ui
        .checkbox(&mut legend.visible, "Légende affichée")
        .changed();

    ui.horizontal(|ui| {
        ui.label("Position");
        let mut position = legend.position;
        egui::ComboBox::new("plot_config_legend_position", "")
            .selected_text(legend_position_label(position))
            .show_ui(ui, |ui| {
                for candidate in [
                    LegendPosition::TopLeft,
                    LegendPosition::TopRight,
                    LegendPosition::BottomLeft,
                    LegendPosition::BottomRight,
                ] {
                    ui.selectable_value(&mut position, candidate, legend_position_label(candidate));
                }
            });
        if position != legend.position {
            legend.position = position;
            commit = true;
        }
    });

    commit
}

/// Libellé français d'un [`PlotType`], y compris [`PlotType::Unknown`] (par
/// prudence : ce bras existe pour que la fonction reste totale, mais
/// `series_editor` ne l'affiche jamais — un type inconnu ne passe pas par le
/// menu déroulant qui appelle cette fonction).
fn plot_type_label(plot_type: &PlotType) -> String {
    match plot_type {
        PlotType::Scatter => "Points".to_string(),
        PlotType::Line => "Ligne".to_string(),
        PlotType::LineScatter => "Ligne + points".to_string(),
        PlotType::Bar => "Barres".to_string(),
        PlotType::Histogram => "Histogramme".to_string(),
        PlotType::ErrorBar => "Ligne + barres d'erreur".to_string(),
        PlotType::Unknown(_) => "Type inconnu".to_string(),
    }
}

/// Libellé français d'un [`LineStyle`].
fn line_style_label(style: LineStyle) -> &'static str {
    match style {
        LineStyle::None => "Aucun",
        LineStyle::Solid => "Plein",
        LineStyle::Dash => "Tirets",
        LineStyle::Dot => "Pointillés",
        LineStyle::DashDot => "Tiret-point",
    }
}

/// Libellé français d'un [`SymbolShape`].
fn symbol_shape_label(shape: SymbolShape) -> &'static str {
    match shape {
        SymbolShape::None => "Aucun",
        SymbolShape::Circle => "Disque",
        SymbolShape::Square => "Carré",
        SymbolShape::Triangle => "Triangle",
        SymbolShape::Diamond => "Losange",
        SymbolShape::Plus => "Croix droite",
        SymbolShape::Cross => "Croix oblique",
    }
}

/// Libellé français d'une [`LegendPosition`].
fn legend_position_label(position: LegendPosition) -> &'static str {
    match position {
        LegendPosition::TopLeft => "Haut gauche",
        LegendPosition::TopRight => "Haut droite",
        LegendPosition::BottomLeft => "Bas gauche",
        LegendPosition::BottomRight => "Bas droite",
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

/// Case de la gouttière des numéros de ligne, dans le **corps** de la table
/// — miroir exact de la rangée des lettres de colonne
/// ([`column_header_widget`]) mais pour les lignes : toute la largeur de la
/// case sélectionne **toute la ligne** (glisser/Maj-clic étend à plusieurs
/// lignes, même logique de multi-sélection que [`cell_widget`]/
/// [`column_header_widget`]) et ouvre le même menu contextuel d'édition
/// ([`grid_edit_menu`]).
///
/// Fond à trois paliers comme la rangée des lettres ([`gutter_fill`]) —
/// repos, touchée par une sélection de données sans l'être elle-même
/// ([`AppState::row_touched_by_selection`]), ou directement sélectionnée —
/// pour que la gouttière se reconnaisse comme une zone de contrôle au
/// premier coup d'œil, et que la ligne d'une cellule sélectionnée se
/// remarque même sans avoir cliqué la gouttière elle-même. Cohérent avec le
/// choix déjà fait pour `column_header_widget` et pour le « # » de
/// [`row_number_header_widget`] juste au-dessus.
fn row_number_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    row: usize,
) {
    let selected = state.is_row_selected(sheet, row);
    let touched = state.row_touched_by_selection(sheet, row);
    let rect = ui.available_rect_before_wrap();
    ui.painter()
        .rect_filled(rect, 0.0, gutter_fill(selected, touched));
    let id = ui.id().with(("plotyx_row_number", row));
    let response = ui.interact(rect, id, egui::Sense::click_and_drag());
    ui.put(
        rect,
        egui::Label::new(
            egui::RichText::new(format!("{}", row + 1)).color(crate::theme::gutter_text_color()),
        )
        .selectable(false),
    );
    let shift = ui.input(|i| i.modifiers.shift);
    if response.drag_started() || (response.clicked() && !shift) {
        state.select_row(sheet, row);
    } else if response.clicked() && shift {
        state.extend_row_selection(sheet, row);
    } else if response.contains_pointer()
        && ui.input(|i| i.pointer.primary_down())
        && state.selection(sheet).is_some()
    {
        // Même geste que pour les cellules de données et la rangée des
        // lettres — étendre pendant le glisser, pas seulement au clic de
        // départ (voir le commentaire détaillé de `cell_widget`).
        state.extend_row_selection(sheet, row);
    } else if response.secondary_clicked() && !selected {
        // Clic droit hors de la sélection : la réduire à cette ligne avant
        // d'ouvrir le menu — même règle que pour une cellule ou un champ
        // d'en-tête (voir `cell_widget`/`header_field_widget`).
        state.select_row(sheet, row);
    }
    // `grid_menu` **dans** la fermeture, jamais avant — même raison que dans
    // `cell_widget` : calculée dehors, elle serait recalculée à chaque image
    // pour chaque ligne visible.
    response.context_menu(|ui| {
        if let Some(menu) = state.grid_menu(sheet) {
            grid_edit_menu(ui, state, actions, sheet, &menu);
        }
    });
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
                    } else if tab && !shift && is_last_visible_column(state, sheet, column) {
                        // Même geste que dans le bloc de raccourcis hors
                        // édition : Tab valide sur la dernière colonne ajoute
                        // une colonne plutôt que de revenir en début de ligne
                        // suivante (voir `Action::AddColumnAndSelect`).
                        actions.push(Action::AddColumnAndSelect { sheet, row });
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
        //
        // Alignement à la tableur, dans le **corps** seulement (jamais la
        // gouttière ni l'en-tête, qui ne portent pas de valeur de colonne) :
        // les nombres à droite, le texte à gauche — `AppState::
        // cell_is_numeric` tranche par cellule, pas par colonne, pour
        // qu'une colonne `Mixed` aligne chaque ligne selon sa propre valeur.
        //
        // `Label::halign` seul ne suffit pas ici : `ui.put` place le widget
        // dans un `Layout::centered_and_justified`, qui justifie le texte
        // (l'étire pour remplir la largeur) avant que `halign` n'ait voix au
        // chapitre — le résultat restait visuellement centré. Un vrai
        // `Layout` directionnel (`right_to_left`/`left_to_right`), comme
        // celui déjà posé sur toute la table (`cell_layout` du
        // `TableBuilder`), aligne réellement le texte contre le bord voulu.
        let layout = if state.cell_is_numeric(column, row) {
            egui::Layout::right_to_left(egui::Align::Center)
        } else {
            egui::Layout::left_to_right(egui::Align::Center)
        };
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(egui::vec2(4.0, 0.0)))
                .layout(layout),
            |ui| {
                ui.add(egui::Label::new(text).selectable(false).truncate());
            },
        );

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
        } else if response.secondary_clicked() && !selected {
            // Clic droit hors de la sélection : la réduire à cette cellule
            // avant d'ouvrir le menu — même règle que dans l'en-tête et sur
            // la rangée des lettres (voir `header_field_widget`).
            state.select_cell(sheet, column, row);
        }
        // `grid_menu` **dans** la fermeture, jamais avant : elle ne tourne
        // que tant que le menu est ouvert. Calculée dehors, elle serait
        // recalculée à chaque image pour *chaque cellule visible* — et elle
        // parcourt l'arbre (`Selection::columns`), ce qui en ferait un coût
        // quadratique en nombre de colonnes sur une grille qui ne fait que
        // s'afficher.
        response.context_menu(|ui| {
            if let Some(menu) = state.grid_menu(sheet) {
                grid_edit_menu(ui, state, actions, sheet, &menu);
            }
        });
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

#[cfg(test)]
mod graph_layout_tests {
    use super::*;
    use plotyx_core::{Column, Project};
    use plotyx_plot::{PanelSpec, PlotSpec, SeriesSpec};
    use plotyx_session::Session;

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

    #[test]
    fn le_repli_ne_concerne_que_le_contenu_du_graphe_vise() {
        let (_session, graph) = graphe();
        let graph_tab = Tab::Graph(graph);
        let mut collapsed = HashSet::new();

        toggle_graph_collapse(&mut collapsed, graph_tab);
        assert_eq!(collapsed, HashSet::from([graph_tab]));

        // Un autre type de contenu ne peut pas être replié par la flèche du
        // graphe, même si son identifiant de nœud coïncide dans ce test de
        // présentation.
        toggle_graph_collapse(&mut collapsed, Tab::Book(graph));
        assert_eq!(collapsed, HashSet::from([graph_tab]));

        toggle_graph_collapse(&mut collapsed, graph_tab);
        assert!(collapsed.is_empty());
    }

    #[test]
    fn len_tete_du_graphe_garde_son_titre_au_centre_et_sa_fleche_a_gauche() {
        let (_session, graph) = graphe();
        let context = egui::Context::default();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 120.0),
                )),
                ..Default::default()
            },
            |ui| {
                let _ = window_header(
                    ui,
                    egui::Id::new("graph_header_centered_title"),
                    "Graph1",
                    Tab::Graph(graph),
                    false,
                );
            },
        );
        output.textures_delta.clear();

        let text_shape = |text: &str| {
            output
                .shapes
                .iter()
                .find_map(|painted| match &painted.shape {
                    egui::Shape::Text(shape) if shape.galley.job.text == text => Some(shape),
                    _ => None,
                })
        };
        let title = text_shape("Graph1").expect("le titre doit être peint");
        let title_center = title.pos.x + title.galley.rect.center().x;
        assert!(
            (title_center - 200.0).abs() < f32::EPSILON,
            "le titre doit être centré dans l'en-tête, pas après la flèche"
        );
        let collapse = text_shape("v").expect("la flèche de repli doit être peinte");
        assert!(
            collapse.pos.x + collapse.galley.rect.center().x < 24.0,
            "la flèche de repli doit rester dans le coin supérieur gauche"
        );
    }

    #[test]
    fn le_rendu_reste_fixe_et_decoupe_meme_dans_une_petite_zone() {
        let (session, graph) = graphe();
        let snapshot = session.handle().snapshot().unwrap();
        let context = egui::Context::default();
        context.set_fonts(plotyx_render_egui::font_definitions());
        let mut view = GraphView::new();

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
            for frame in 0..3 {
                // La molette se trouve sous la zone visible, sur une partie
                // tronquée de la figure : elle ne doit pas zoomer celle-ci.
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
                    graph_view_contents(&mut content_ui, &mut view, &snapshot, graph);
                });
                output.textures_delta.clear();
                let rect = view.cache().key().unwrap().rect;
                assert_eq!(rect.size(), egui::vec2(480.0, 360.0));
                assert_eq!(
                    view.view().generation(),
                    0,
                    "interaction sur une partie masquée"
                );
                if frame > 0 {
                    assert_eq!(view.cache().misses(), misses, "recalcul sans changement");
                }
                // Vérifier les formes réellement envoyées à egui, pas
                // seulement le rectangle calculé avant le rendu.
                assert!(!view.cache().shapes().is_empty());
                for shape in view.cache().shapes() {
                    assert!(
                        output.shapes.iter().any(|painted| painted.shape == *shape
                            && visible.contains_rect(painted.clip_rect)),
                        "forme du graphe sans découpage"
                    );
                }
            }
        }
    }

    #[test]
    fn un_graphe_detache_reconstruit_ses_formes_dans_son_viewport() {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.plotyx".to_string()).unwrap();
        let mut app = PlotyxApp::new(state);
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        context.set_fonts(plotyx_render_egui::font_definitions());
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
                graph_contents(ui, &app.state, &mut app.graph_views, graph);
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
        assert!(
            output
                .shapes
                .iter()
                .any(|painted| view.cache().shapes().contains(&painted.shape)),
            "le viewport détaché doit réellement recevoir les formes du graphe"
        );
        // Régression : le bouton « ↩ » de réattachement, non borné en
        // hauteur, pouvait consommer toute la fenêtre détachée (voir la
        // doc de tête de la ligne qui l'alloue) — la figure se retrouvait
        // alors hors du rectangle visible, invisible bien que son cache
        // fût rempli (ce que les deux assertions ci-dessus ne détectent
        // pas : un `Shape` reste dans la liste même hors clip).
        let figure_rect = view.cache().key().unwrap().rect;
        assert!(
            figure_rect.max.y <= GRAPH_WINDOW_SIZE.y,
            "la figure doit rester dans la fenêtre détachée, pas {figure_rect:?} pour une fenêtre de {GRAPH_WINDOW_SIZE:?}"
        );
    }

    #[test]
    fn le_pan_principal_atteint_la_figure_sous_la_zone_de_depot() {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.plotyx".to_string()).unwrap();
        let context = egui::Context::default();
        context.set_fonts(plotyx_render_egui::font_definitions());
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
        let state = AppState::new(session.handle(), "test.plotyx".to_string()).unwrap();
        let context = egui::Context::default();
        context.set_fonts(plotyx_render_egui::font_definitions());
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
                egui::Window::new("Figure")
                    .id(id)
                    .title_bar(false)
                    .drag_area(egui::WindowDrag::Off)
                    .current_pos(egui::pos2(100.0, 100.0))
                    .default_size(egui::vec2(560.0, 540.0))
                    .show(ui.ctx(), |ui| {
                        let _ = window_header(ui, id, "Figure", tab, false);
                        graph_drop_target(ui, &state, &mut actions, &mut graph_views, graph);
                    });
            });
            output.textures_delta.clear();
            if is_initial_frame {
                let view = graph_views.get(&graph).unwrap();
                let panel = &view.last_drawn().unwrap().panels[0];
                let figure = view.cache().key().unwrap().rect;
                panel_center.set(Some(egui::pos2(
                    figure.min.x + panel.pixel_origin.0 as f32 + panel.pixel_size.0 as f32 / 2.0,
                    figure.min.y + panel.pixel_origin.1 as f32 + panel.pixel_size.1 as f32 / 2.0,
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
}
