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
//! cet ordre qui donne à `viewport_rect` (§ Fenêtres flottantes confinées)
//! son étendue exacte : ce que ces panneaux fixes laissent, rien de plus —
//! la portion actuellement *visible* de l'espace de travail, à distinguer
//! de sa taille logique fixe (`WORKSPACE_SIZE`).
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
//! flottantes à l'intérieur du [`egui::CentralPanel`], plutôt qu'amarrées en
//! tuiles plein cadre — une par [`PlotyxApp::floating_tabs`], jamais
//! dupliquée (même invariant que l'ancien dock : [`PlotyxApp::open_tab`]
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
//! exprimées dans ces coordonnées logiques ; [`PlotyxApp::ui`] les convertit
//! en coordonnées écran juste avant la peinture (`workspace_to_screen`), à
//! partir de `viewport_rect` (ce que laissent les panneaux fixes et les
//! barres de défilement) et de [`PlotyxApp::workspace_scroll`] (0,0 au
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
//! précédente ([`PlotyxApp::next_cascade`], remis à zéro tous les huit
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
//! calques, indépendant de [`PlotyxApp::floating_tabs`], qui reste la seule
//! source d'`active_tab` (donc du contour de surbrillance). Sans
//! synchronisation, un clic direct dans une fenêtre la faisait passer au
//! premier plan sans jamais la surligner (B2,
//! `retours-2026-09-23-bugs.md`). [`PlotyxApp::ui`] relit donc
//! `Context::top_layer_id` juste après avoir peint les fenêtres flottantes
//! et ramène le tab trouvé en fin de `floating_tabs` via
//! `sync_floating_tabs_order_to_top_layer` — un cadre de retard tout au plus,
//! `egui` redemandant lui-même un repaint immédiat au clic. Le seul cas que
//! ce module doit encore gérer *avant* qu'`egui` n'ait peint quoi que ce soit
//! est « rouvrir depuis le Project Manager une entrée déjà ouverte doit la
//! ramener au premier plan sans qu'on ait cliqué dedans » :
//! [`PlotyxApp::open_tab`] pose alors [`PlotyxApp::focus_next`], consommé au
//! rendu suivant par un appel à `Context::move_to_top` sur le calque
//! ([`FLOATING_WINDOW_ORDER`]) de la fenêtre concernée. Jusqu'au passage à
//! `egui::Area`, cet appel visait `Order::Foreground` alors que les
//! `egui::Window` vivaient sur `Order::Middle` : il remontait le calque du
//! contour, pas la fenêtre elle-même.
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
//! B8 (`retours-2026-09-23-bugs.md`) : comme pour une fenêtre
//! flottante rétrécie (B4), un `Ui` dont les enfants dépassent son
//! `max_rect` les peint quand même. [`PlotyxApp::show_detached_tabs`] pose
//! donc le contenu des quatre natures dans un même sous-`Ui` borné et rogné,
//! juste avant le `match` qui aiguille vers chacune d'elles.
//!
//! # Pourquoi pas un `ViewModel` par nature de contenu comme pour le PM
//!
//! Le PM n'a qu'une seule forme de contenu (un arbre en lecture seule) ; les
//! quatre natures de fenêtre ici sont franchement différentes (grille
//! éditable virtualisée, graphe interactif avec cache de formes, texte
//! éditable, espace réservé). Plutôt que quatre `ViewModel`/`Requests`
//! disjoints à faire cohabiter dans [`PlotyxApp::detached_tabs`],
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
//! [`Tab::Matrix`] n'a aucun état à faire voyager (espace réservé) :
//! [`matrix_contents`] ne lit que l'arbre et sert tel quel aux deux
//! fenêtres.
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

use crate::color_picker::{color_field, ColorUsage, PickerEnv, SavedColorEdit};
use crate::selection::{GridCursor, Selection};
use crate::state::{
    active_graph, drop_payload_action, export_file_name, node_title_in, series_list_label, Action,
    ActiveTabKind, AppState, ColumnDragPayload, GridMenu, HeaderField, ImportDestination,
    ImportLineAction, ImportStep, MoveDirection, PlotAxis, PlotSettings, PlotSettingsTab,
    PlotTarget, ProjectTreeNode, SaveTarget, WindowKind,
};
use crate::theme;
use crate::tr;
use plotyx_import::{DecimalStyle, HeaderSkip, NameSource, Separator};
use plotyx_render_egui::{AxisSide, PlotComponent};

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

/// Annuler/rétablir dans le viewport actif, racine ou classeur détaché.
fn collect_undo_redo_shortcuts(ctx: &egui::Context, actions: &mut Vec<Action>) {
    ctx.input_mut(|input| {
        // Le rétablissement se teste avant l'annulation : Ctrl+Maj+Z
        // matcherait également Ctrl+Z avec les modificateurs en surplus.
        if input.consume_shortcut(&SHORTCUT_REDO_Z) || input.consume_shortcut(&SHORTCUT_REDO_Y) {
            actions.push(Action::Redo);
        } else if input.consume_shortcut(&SHORTCUT_UNDO) {
            actions.push(Action::Undo);
        }
    });
}

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
/// Ctrl+E — exporter le graphe actif en SVG, PDF ou PNG (issue #34). Sans
/// effet quand l'onglet actif n'est pas un graphe.
pub const SHORTCUT_EXPORT_GRAPH: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::E);
/// Ctrl+" — ouvrir l'assistant d'import. Sur un clavier AZERTY, `"` est la
/// touche du chiffre 3 **sans** Maj : egui ne connaît pas de touche `"` et
/// retombe sur la touche physique, donc le raccourci se consomme comme
/// Ctrl+3. Sur un clavier QWERTY, `"` est Maj+apostrophe
/// ([`SHORTCUT_IMPORT_QWERTY`]).
pub const SHORTCUT_IMPORT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Num3);
/// Ctrl+Maj+' — le même raccourci d'import sur un clavier QWERTY, où `"`
/// s'obtient avec Maj.
pub const SHORTCUT_IMPORT_QWERTY: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::Quote,
);
/// Rappel affiché dans le menu : le caractère, pas le nom de la touche
/// physique.
const SHORTCUT_IMPORT_TEXT: &str = "Ctrl+\"";
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
/// long, unité, commentaire, formule, métadonnées).
const HEADER_HEIGHT_STACKED: f32 = HEADER_LONGNAME_HEIGHT + 5.0 * HEADER_SUBROW_HEIGHT;

/// Hauteur de l'en-tête empilé selon que la ligne « Metadata » est affichée.
fn stacked_header_height(show_metadata: bool) -> f32 {
    if show_metadata {
        HEADER_HEIGHT_STACKED
    } else {
        HEADER_HEIGHT_STACKED - HEADER_SUBROW_HEIGHT
    }
}

/// La ligne « Metadata » de l'en-tête est-elle affichée ? Réglage de vue,
/// gardé dans la mémoire d'egui (un seul `Context` pour tous les viewports :
/// les classeurs flottants et détachés le suivent ensemble), jamais dans le
/// modèle ni l'historique. Affichée par défaut.
fn metadata_row_visible(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(egui::Id::new("plotyx_show_metadata_row")))
        .unwrap_or(true)
}

fn set_metadata_row_visible(ctx: &egui::Context, visible: bool) {
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::new("plotyx_show_metadata_row"), visible);
    });
}
/// Hauteur fixe du panneau des onglets de feuille sous la grille.
/// [`book_contents`] la réserve avant de construire la table : la hauteur
/// intrinsèque de cette dernière ne peut plus repousser les onglets hors
/// de la fenêtre. Juste la hauteur d'un onglet et un filet : la bande n'a
/// plus de barre de défilement à loger (boutons « < » « > », voir
/// [`sheet_tabs`]), et la barre horizontale de la grille est en dessous.
const TAB_STRIP_HEIGHT: f32 = 24.0;
/// Hauteur du seul contrôle conservé sous une figure : le retour au cadrage
/// initial. Réserver sa hauteur garde le rectangle de la figure stable d'une
/// image à l'autre — sans quoi le cache de formes serait invalidé au moindre
/// survol.
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
/// Hauteur de la bande d'en-tête peinte par [`window_header`] — partagée
/// avec le calcul de la taille repliée d'une fenêtre flottante (B3,
/// `retours-2026-09-23-bugs.md`) : un contour ou une taille
/// forcée qui ne reprend pas cette même valeur redevient trop grand dès que
/// le contenu est masqué.
const WINDOW_HEADER_HEIGHT: f32 = 28.0;
/// Marge intérieure du `Frame` d'une fenêtre de contenu, flottante ou
/// détachée — voir [`floating_window_frame`] et le `Frame` construit dans
/// [`PlotyxApp::show_detached_tabs`]. Sans marge verticale : la bande
/// d'en-tête (`WINDOW_HEADER_HEIGHT`) et les séparateurs du contenu
/// fournissent déjà leur propre espacement, la marge par défaut d'egui
/// (`Frame::window`/`Frame::central_panel`) n'ajoutait donc qu'un bandeau
/// blanc mort au-dessus et en dessous, sans rôle. La marge horizontale reste
/// pour ne pas coller le texte au bord.
const WINDOW_CONTENT_MARGIN: egui::Margin = egui::Margin {
    left: 6,
    right: 6,
    top: 0,
    bottom: 0,
};
/// Nombre de crans de cascade avant de reboucler à zéro, pour qu'une
/// succession de fenêtres ne dérive jamais hors de l'aire de travail.
const CASCADE_WRAP: u32 = 8;
/// Largeur, en points, de la zone de prise du glisser-déposer de colonnes
/// vers un graphe — une bande le long du bord extérieur gauche ou droit de
/// la grille (voir [`grid_grip_side`], et [`grid_contents`] pour le geste
/// complet).
const COLUMN_GRIP_WIDTH: f32 = 10.0;
/// Largeur par défaut de la fenêtre des propriétés d'un graphe
/// ([`plot_settings_window`]).
const PLOT_SETTINGS_WIDTH: f32 = 380.0;
/// Hauteur par défaut de la fenêtre des propriétés d'un graphe. La fenêtre
/// se redimensionne ensuite librement, y compris plus haut que son contenu
/// (voir [`plot_settings_contents`]).
const PLOT_SETTINGS_HEIGHT: f32 = 460.0;

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
    /// [`PlotyxApp::ui`] applique ce décalage à la peinture. Transitoire, comme la position de défilement d'un
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
    /// Copie de [`PlotyxApp::pm_selection`], même resynchronisation que
    /// `tree` — nécessaire pour que la fenêtre détachée du Project Manager
    /// affiche elle aussi la sélection courante et le fond « Masquer » à
    /// jour (§6).
    selected: HashSet<Tab>,
    /// Copie de [`PlotyxApp::hidden_tabs`], même resynchronisation.
    hidden: HashSet<Tab>,
    /// Copie de l'onglet au premier plan, même resynchronisation — pour le
    /// double-clic « fermer si déjà au premier plan » (§4).
    active_tab: Option<Tab>,
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
    /// Dépôts de fichiers, traités par la même session que la fenêtre principale.
    actions: Vec<Action>,
    /// Onglets demandés par un clic dans l'arbre depuis la dernière
    /// vidange ; [`PlotyxApp::ui`] les transfère dans son propre
    /// `open_requests` puis les vide.
    open: Vec<Tab>,
    /// Onglets fermés par un double-clic depuis la dernière vidange (§4) —
    /// même schéma que `open`.
    close: Vec<Tab>,
    /// Vrai si le bouton « Réattacher » ou la fermeture de la fenêtre OS a
    /// été actionné depuis la dernière vidange.
    reattach: bool,
    /// Sélection/masquage tels qu'ils étaient à la fin de la dernière image
    /// peinte par la fenêtre détachée — `None` tant qu'aucune image n'a
    /// encore tourné depuis la dernière vidange. Remplacés en bloc plutôt
    /// qu'en delta, comme `DetachedTabRequests::geometry` (§6).
    selection: Option<HashSet<Tab>>,
    hidden: Option<HashSet<Tab>>,
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
struct DetachedTabViewModel {
    /// Instantané du modèle, resynchronisé à chaque image — voir la doc de
    /// tête du module, § « la lecture du modèle est factorisée ». `None`
    /// seulement avant la toute première synchronisation (jamais observable
    /// depuis l'extérieur de [`PlotyxApp::detach_tab`], qui le pose aussitôt).
    snapshot: Option<Snapshot>,
    /// `Tab::Book`/`Tab::Note` : vue [`AppState`] locale à la fenêtre, qui
    /// rend le contenu commun avec la fenêtre flottante (mêmes
    /// `book_contents`/`note_contents`) tout en gardant sa sélection et ses
    /// saisies en cours.
    view_state: Option<AppState>,
    /// `Tab::Graph` : la vue (cache de formes, zoom/pan), déplacée hors de
    /// [`PlotyxApp::graph_views`] le temps du détachement — voir
    /// [`PlotyxApp::detach_tab`]/[`PlotyxApp::reattach_tab`].
    graph_view: Option<GraphView>,
    /// `Tab::Graph`/`Tab::Book` : repli du contenu, propre à cette fenêtre
    /// détachée — initialisé depuis [`PlotyxApp::collapsed_tabs`] par
    /// `detach_tab`, basculé directement ici (pas de détour par
    /// `DetachedTabRequests`, ce repli n'intéresse que cette fenêtre), et
    /// reversé dans `collapsed_tabs` par `reattach_tab`.
    collapsed: bool,
    /// Le premier callback d'un viewport différé peut arriver avant que
    /// Wayland/WGPU ait stabilisé sa taille. Une seule image supplémentaire
    /// donne alors à la figure son rectangle réel, sans entretenir une boucle
    /// de rendu quand la fenêtre reste inactive.
    needs_initial_repaint: bool,
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
    expanded_size: Option<egui::Vec2>,
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
            let mut restored: Vec<(u32, Tab, plotyx_session::WindowView)> = Vec::new();
            for (node, kind) in window_nodes(&state.project_tree()) {
                let Some(view) = saved_window_geometries.get(&node).and_then(|g| g.view) else {
                    continue;
                };
                if view.mode != plotyx_session::WindowMode::Closed {
                    restored.push((view.stack, tab_for_window(node, kind), view));
                }
            }
            restored.sort_by_key(|(stack, tab, _)| (*stack, tab_node(*tab).raw()));
            for (_, tab, view) in restored {
                open_initial(tab);
                if view.collapsed {
                    collapsed_tabs.insert(tab);
                }
                if view.mode == plotyx_session::WindowMode::Maximized {
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
            floating_tabs,
            cascade_rank,
            next_cascade,
            focus_next: None,
            graph_views: HashMap::new(),
            collapsed_tabs,
            maximized_tab,
            pm_detached: false,
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
                matches!(node.kind(), plotyx_core::node::NodeKind::Workbook { .. })
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
        let mut save_as = false;
        let mut open = false;
        let mut import = false;
        let mut export = false;
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
            if input.consume_shortcut(&SHORTCUT_EXPORT_GRAPH) {
                export = true;
            }
            if input.consume_shortcut(&SHORTCUT_RUN_SCRIPT) {
                actions.push(Action::RunScript);
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
        let active_tab = self
            .maximized_tab
            .or_else(|| self.floating_tabs.last().copied());
        if let Some(graph) = active_graph(active_tab_kind(active_tab)).filter(|_| export) {
            request_graph_export(
                self.graph_views.get(&graph),
                self.state.tree(),
                graph,
                actions,
            );
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
        egui::Panel::top("plotyx_menu_bar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(tr!("menu-file"), |ui| {
                    if ui.button("Nouveau classeur").clicked() {
                        actions.push(Action::AddWorkbook);
                        ui.close();
                    }
                    if ui
                        .add(egui::Button::new("Importer…").shortcut_text(SHORTCUT_IMPORT_TEXT))
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
                            egui::Button::new("Actualiser depuis le fichier source"),
                        )
                        .on_hover_text(
                            "Relit les fichiers d'où viennent les colonnes de la feuille active",
                        )
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
                            egui::Button::new("Régler le dernier import…"),
                        )
                        .on_hover_text(
                            "Reprend les fichiers et le modèle du dernier import dans l'assistant",
                        )
                        .clicked()
                    {
                        actions.push(Action::ReopenLastImport);
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
                if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
                    ui.menu_button("Graphique", |ui| {
                        for (label, tab) in [
                            ("Propriétés du graphe…", PlotSettingsTab::Graph),
                            ("Zone de tracé…", PlotSettingsTab::PlotArea),
                            ("Axes…", PlotSettingsTab::Axes),
                            ("Séries…", PlotSettingsTab::Series),
                            ("Légende…", PlotSettingsTab::Legend),
                            ("Thème…", PlotSettingsTab::Theme),
                        ] {
                            if ui.button(label).clicked() {
                                actions.push(Action::OpenPlotSettings {
                                    graph,
                                    tab,
                                    axis: None,
                                });
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button("Réinitialiser le zoom").clicked() {
                            if let Some(view) = self.graph_views.get_mut(&graph) {
                                view.reset_view();
                            }
                            ui.close();
                        }
                        ui.separator();
                        // Textes libres (issue #35) : le pendant, dans la
                        // barre de menus, du clic droit et de Suppr.
                        let selected = self
                            .graph_views
                            .get(&graph)
                            .and_then(GraphView::selected_annotation);
                        if ui.button("Ajouter un texte").clicked() {
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
                                egui::Button::new("Supprimer le texte sélectionné")
                                    .shortcut_text("Suppr"),
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
                        if ui
                            .add(
                                egui::Button::new("Exporter le graphe…")
                                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_EXPORT_GRAPH)),
                            )
                            .on_hover_text("SVG, PDF ou PNG, avec le zoom courant")
                            .clicked()
                        {
                            request_graph_export(
                                self.graph_views.get(&graph),
                                self.state.tree(),
                                graph,
                                actions,
                            );
                            ui.close();
                        }
                        ui.menu_button("Résolution des exports PNG", |ui| {
                            let current = self.state.png_export_scale();
                            for scale in crate::state::PNG_EXPORT_SCALES {
                                let (w, h) = DEFAULT_FIGURE_SIZE;
                                let label = format!(
                                    "×{scale} ({} × {} px)",
                                    (w as f32 * scale) as u32,
                                    (h as f32 * scale) as u32
                                );
                                if ui.radio(current == scale, label).clicked() {
                                    actions.push(Action::SetPngExportScale(scale));
                                    ui.close();
                                }
                            }
                        });
                    });
                }
                ui.menu_button("Fenêtre", |ui| {
                    let mut show_metadata = metadata_row_visible(ctx);
                    if ui
                        .checkbox(
                            &mut show_metadata,
                            "Afficher la ligne Metadata des colonnes",
                        )
                        .changed()
                    {
                        set_metadata_row_visible(ctx, show_metadata);
                    }
                    ui.separator();
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
    fn status_bar(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
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
                    if self.state.can_reopen_last_import()
                        && self.state.import_state().is_none()
                        && ui.link("Régler l'import…").clicked()
                    {
                        actions.push(Action::ReopenLastImport);
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
    fn project_manager_panel(
        &mut self,
        ui: &mut egui::Ui,
        open_requests: &mut Vec<Tab>,
        close_requests: &mut Vec<Tab>,
        active_tab: Option<Tab>,
    ) {
        egui::Panel::left("plotyx_pm")
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
    fn show_detached_project_manager(&mut self, ctx: &egui::Context, active_tab: Option<Tab>) {
        {
            let mut model = self
                .pm_view_model
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            model.tree = self.state.project_tree();
            model.selected = self.pm_selection.clone();
            model.hidden = self.hidden_tabs.clone();
            model.active_tab = active_tab;
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
                    if ui.button("Réattacher à la fenêtre principale").clicked() {
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
            toggle_collapse_to(&mut self.collapsed_tabs, tab, model.collapsed);
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
        self.floating_rects.insert(tab_node(tab), rect);
    }

    /// Envoie à la session la géométrie flottante de chaque fenêtre connue
    /// avec son état du moment (ouverte/maximisée/fermée, repliée, rang
    /// d'empilement) — uniquement ce qui a changé depuis le dernier envoi.
    /// Une fenêtre détachée compte comme ouverte : le détachement ne se
    /// retient pas, et la position de sa fenêtre OS n'est jamais écrite.
    fn sync_window_views(&mut self) {
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
                plotyx_session::WindowMode::Maximized
            } else if is_open {
                plotyx_session::WindowMode::Open
            } else {
                plotyx_session::WindowMode::Closed
            };
            let collapsed = self.collapsed_tabs.iter().any(|tab| tab_node(*tab) == node);
            let geometry = WindowGeometry {
                x: f64::from(rect.min.x),
                y: f64::from(rect.min.y),
                width: f64::from(rect.width()),
                height: f64::from(rect.height()),
                view: Some(plotyx_session::WindowView {
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
                if let Some(view_state) = model.view_state.as_mut() {
                    view_state.sync_detached_snapshot(state.snapshot().clone());
                }
            }
            let view_model = Arc::clone(&detached.view_model);
            let requests = Arc::clone(&detached.requests);
            let root_context = ctx.clone();
            let tab = *tab;
            let title = format!("PlotYx — {}", tab_title(state, tab));
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
                                        ui.label("Session du classeur indisponible.");
                                    }
                                }
                                Tab::Graph(id) => {
                                    let view =
                                        model.graph_view.get_or_insert_with(GraphView::default);
                                    graph_view_contents(
                                        ui,
                                        view,
                                        &snapshot,
                                        id,
                                        &mut requests.actions,
                                    );
                                }
                                Tab::Matrix(id) => {
                                    matrix_contents(ui, tree, id);
                                }
                                Tab::Note(id) => {
                                    if let Some(view_state) = model.view_state.as_mut() {
                                        note_contents(ui, view_state, &mut requests.actions, id);
                                    } else {
                                        ui.label("Session de la note indisponible.");
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
        // L'onglet au premier plan — source de vérité pour le double-clic du
        // Project Manager (§4) et le surlignage de la fenêtre active (§5) :
        // calculé une fois ici, avant même le panneau fixe du Project
        // Manager qui en a besoin, plutôt qu'à l'intérieur du `CentralPanel`
        // (trop tard pour lui).
        let active_tab = self
            .maximized_tab
            .or_else(|| self.floating_tabs.last().copied());

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
        self.status_bar(ui, &mut actions);
        // Panneau fixe, peint *avant* `PlotyxApp::project_manager_panel` —
        // voir la doc de tête du module, § Console de script, et la doc de
        // tête de `PlotyxApp::script_panel` : c'est cet ordre précis qui lui
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
                    .id_salt("plotyx_workspace")
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
                                    tab_contents(ui, state, &mut actions, graph_views, tab);
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
        self.screenshot(&ctx);
        self.autoquit(&ctx);
    }
}

/// Rectangle flottant d'une géométrie persistée.
fn geometry_rect(geometry: &WindowGeometry) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(geometry.x as f32, geometry.y as f32),
        egui::vec2(geometry.width as f32, geometry.height as f32),
    )
}

/// Les fenêtres de l'arbre du Project Manager, à plat.
fn window_nodes(tree: &[crate::state::ProjectTreeNode]) -> Vec<(NodeId, WindowKind)> {
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

/// Réglage « Fin des données » de la recette d'import (issue #4) : coupure
/// automatique par défaut, désactivable. Le texte ou le nombre associé à
/// l'option choisie reste dans la recette tant qu'on en change.
fn data_end_row(ui: &mut egui::Ui, end: &mut plotyx_import::DataEnd) {
    use plotyx_import::DataEnd;
    let label = |end: &DataEnd| match end {
        DataEnd::Auto => "Automatique (premier X non numérique)",
        DataEnd::None => "Aucune coupure",
        DataEnd::FirstBlankLine => "Première ligne vide",
        DataEnd::Marker(_) => "Marqueur de fin",
        DataEnd::LinesAfterHeader(_) => "N lignes après l'en-tête",
        DataEnd::LinesBeforeEnd(_) => "N lignes avant la fin",
    };
    ui.horizontal(|ui| {
        ui.label("Fin des données :");
        egui::ComboBox::from_id_salt("plotyx_import_data_end")
            .selected_text(label(end))
            .show_ui(ui, |ui| {
                for candidate in [
                    DataEnd::Auto,
                    DataEnd::None,
                    DataEnd::FirstBlankLine,
                    DataEnd::Marker(String::new()),
                    DataEnd::LinesAfterHeader(100),
                    DataEnd::LinesBeforeEnd(1),
                ] {
                    let same = std::mem::discriminant(&*end) == std::mem::discriminant(&candidate);
                    if ui.selectable_label(same, label(&candidate)).clicked() && !same {
                        *end = candidate;
                    }
                }
            });
        match end {
            DataEnd::Marker(text) => {
                ui.text_edit_singleline(text);
            }
            DataEnd::LinesAfterHeader(n) | DataEnd::LinesBeforeEnd(n) => {
                let mut value = *n as i64;
                if ui
                    .add(egui::DragValue::new(&mut value).range(0..=i64::MAX))
                    .changed()
                {
                    *n = value.max(0) as usize;
                }
            }
            _ => {}
        }
    });
}

/// Inscrit `paths` aux fichiers d'import récents (dossier de config). Un échec
/// d'écriture est sans conséquence : la liste est un confort.
fn remember_recent_imports(paths: &[std::path::PathBuf]) {
    let mut recent = plotyx_import::RecentFiles::load();
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
    egui::Id::new("plotyx_file_drop").with(ctx.viewport_id())
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
            egui::Id::new("plotyx_file_drop_outline"),
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

/// Ouvre `path` dans une nouvelle fenêtre PlotYx : un second processus de ce
/// même binaire (`plotyx-app <chemin>`). Un échec de lancement n'est que
/// journalisé — un dépôt raté ne doit rien casser.
fn open_project_in_new_window(path: &std::path::Path) {
    let spawned =
        std::env::current_exe().and_then(|exe| std::process::Command::new(exe).arg(path).spawn());
    if let Err(err) = spawned {
        eprintln!(
            "[plotyx-app] ouverture de {} impossible : {err}",
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

/// Bascule le repli du contenu d'un onglet. Réservé à `Tab::Graph` et
/// `Tab::Book` (B5, `retours-2026-09-23-bugs.md`) — `Matrix`
/// et `Note` n'ont pas ce contrôle et restent donc toujours affichés ; rien
/// dans la peinture du contenu replié (voir la boucle des fenêtres
/// flottantes et détachées dans `PlotyxApp::ui`) ne dépend du type de `tab`,
/// l'extension à ces deux natures n'est donc qu'une question d'icône visible,
/// pas de comportement caché à revérifier.
fn toggle_collapse(collapsed_tabs: &mut HashSet<Tab>, tab: Tab) {
    if !matches!(tab, Tab::Graph(_) | Tab::Book(_)) {
        return;
    }
    if !collapsed_tabs.insert(tab) {
        collapsed_tabs.remove(&tab);
    }
}

/// Pose le repli à une valeur précise plutôt que de le basculer — utilisé
/// par [`PlotyxApp::reattach_tab`] pour reverser dans
/// `collapsed_tabs` l'état propre à la fenêtre détachée
/// (`DetachedTabViewModel::collapsed`) plutôt que de le deviner.
fn toggle_collapse_to(collapsed_tabs: &mut HashSet<Tab>, tab: Tab, collapsed: bool) {
    if !matches!(tab, Tab::Graph(_) | Tab::Book(_)) {
        return;
    }
    if collapsed {
        collapsed_tabs.insert(tab);
    } else {
        collapsed_tabs.remove(&tab);
    }
}

/// Taille à peindre pour une fenêtre flottante cette image. La taille logique (`current_size`) telle quelle, sauf le temps
/// d'un repli (B3, `retours-2026-09-23-bugs.md`) : le contenu
/// n'est alors plus peint (seule [`window_header`] l'est), et forcer la
/// taille dépliée peindrait un cadre — et un contour de surbrillance —
/// autour d'une zone vide.
///
/// Réutilisée telle quelle par [`PlotyxApp::show_detached_tabs`] (B9) pour
/// calculer la taille à envoyer au viewport OS via
/// `ViewportCommand::InnerSize` : même bande d'en-tête, même hauteur
/// repliée — seul change ce qui porte cette taille (le cadre peint par
/// [`show_floating_window`] côté flottant, un viewport OS réel côté détaché).
fn floating_window_display_size(
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

/// Rectangle à retenir dans `window_geometries` (persisté en `.plx`) à
/// partir du rectangle réellement peint cette image. La position vient de
/// ce rectangle, mais la taille doit rester la taille **logique** de la
/// fenêtre : pendant un repli (B3), le rectangle réellement peint est réduit
/// à [`floating_window_display_size`], et le persister tel quel referait
/// réapparaître la fenêtre repliée — y compris une fois dépliée à nouveau —
/// à la prochaine ouverture du projet.
///
/// Réutilisée telle quelle par [`PlotyxApp::show_detached_tabs`] (B9,
/// `retours-2026-09-23-bugs.md`) avec
/// `DetachedTabViewModel::expanded_size` en guise de taille logique : le
/// rectangle rapporté par le viewport OS pendant un repli est, là aussi,
/// le rectangle réduit que l'`InnerSize` envoyé au bascule a produit, pas
/// la taille à persister.
fn floating_window_geometry_to_persist(
    painted_rect: egui::Rect,
    current_size: egui::Vec2,
) -> egui::Rect {
    egui::Rect::from_min_size(painted_rect.min, current_size)
}

/// Trait du contour de surbrillance — la couleur de sélection du thème pour
/// la fenêtre active, le trait par défaut d'egui sinon. Partagé entre
/// [`paint_active_window_outline`] (fenêtres flottantes internes et fenêtres
/// détachées, B1/B3, B7) : même logique de surbrillance des deux côtés, seule
/// la notion de « active » diffère (`is_active` calculé depuis `active_tab`
/// pour les unes, depuis le focus OS du viewport pour les autres).
fn active_window_stroke(style: &egui::Style, is_active: bool) -> egui::Stroke {
    if is_active {
        style.visuals.selection.stroke
    } else {
        style.visuals.window_stroke()
    }
}

/// `Frame` d'une fenêtre flottante — le `Frame::window` habituel, sans
/// recoloration selon l'activité. [`show_floating_window`] le peint sur le
/// rectangle de la fenêtre ; l'état actif est peint par-dessus, sur ce même
/// rectangle, par [`paint_active_window_outline`].
fn floating_window_frame(style: &egui::Style) -> egui::Frame {
    egui::Frame::window(style).inner_margin(WINDOW_CONTENT_MARGIN)
}

/// Peint le contour de surbrillance d'une fenêtre active sur `rect`, le
/// rectangle même de la fenêtre. Les appelants passent un `Painter` de la
/// bonne couche : celle de la fenêtre côté MDI ([`show_floating_window`]),
/// la couche de fond du viewport côté fenêtre OS détachée.
fn paint_active_window_outline(
    painter: &egui::Painter,
    rect: egui::Rect,
    corner_radius: egui::CornerRadius,
    style: &egui::Style,
) {
    painter.rect_stroke(
        rect,
        corner_radius,
        active_window_stroke(style, true),
        egui::StrokeKind::Inside,
    );
}

fn show_window_contents_bounded(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    let rect = ui.available_rect_before_wrap();
    let mut content = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    content.set_clip_rect(ui.clip_rect().intersect(rect));
    add_contents(&mut content);
    ui.advance_cursor_after_rect(rect);
}

/// Zone de défilement horizontale de la grille. Sa propre barre est masquée :
/// la barre visible est déportée **sous les onglets de feuille**
/// ([`show_grid_horizontal_bar`]), comme dans un tableur. Le défilement
/// (molette avec Maj, glisser) reste actif ; un décalage demandé par la barre
/// déportée à l'image précédente (`pending_offset`) est appliqué ici.
fn show_horizontally_scrollable_grid<R>(
    ui: &mut egui::Ui,
    pending_offset: Option<f32>,
    add_grid: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    let mut area = egui::ScrollArea::horizontal()
        .auto_shrink([false, true])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    if let Some(offset) = pending_offset {
        area = area.horizontal_scroll_offset(offset);
    }
    area.show(ui, add_grid)
}

/// Barre de défilement horizontale de la grille, déportée dans la bande
/// réservée **sous les onglets de feuille** (`strip`). Même principe que
/// [`show_grid_vertical_bar`] : une `ScrollArea` horizontale vide, de la
/// largeur de contenu de la grille, qui reflète son décalage ; un glisser sur
/// cette barre est mis en attente (`pending_id`) et appliqué à la grille à
/// l'image suivante ([`show_horizontally_scrollable_grid`]). La bande a la
/// largeur de la grille : pas de mise à l'échelle.
fn show_grid_horizontal_bar<R>(
    ui: &mut egui::Ui,
    pending_id: egui::Id,
    strip: egui::Rect,
    grid: &egui::scroll_area::ScrollAreaOutput<R>,
) {
    if !strip.is_positive() {
        return;
    }
    let grid_offset = grid.state.offset.x;
    let mut bar_ui = ui.new_child(egui::UiBuilder::new().max_rect(strip));
    let bar = egui::ScrollArea::horizontal()
        .id_salt(pending_id)
        .auto_shrink([false, false])
        .horizontal_scroll_offset(grid_offset)
        .show(&mut bar_ui, |ui| {
            ui.allocate_exact_size(egui::vec2(grid.content_size.x, 0.0), egui::Sense::hover());
        });
    let dragged_offset = bar.state.offset.x;
    if (dragged_offset - grid_offset).abs() > 0.5 {
        ui.memory_mut(|memory| memory.data.insert_temp(pending_id, dragged_offset));
        ui.ctx().request_repaint();
    }
}

/// Rapport d'échelle entre la barre verticale de la grille et le corps de
/// la table qu'elle pilote : la barre couvre toute la hauteur de la fenêtre
/// (`bar_height`), le corps défilant seulement la sienne (`body_height`, sous
/// l'en-tête figé). Contenu et décalage de la barre sont ceux de la table
/// multipliés par ce rapport : la poignée garde ainsi la même proportion
/// (visible / total) et la même position relative que si elle ne couvrait
/// que le corps. `1.0` quand le corps n'a pas de hauteur.
fn grid_vertical_bar_scale(bar_height: f32, body_height: f32) -> f32 {
    if body_height > 0.0 {
        bar_height / body_height
    } else {
        1.0
    }
}

/// Barre de défilement verticale de la grille (B10), collée au bord droit de
/// la fenêtre plutôt qu'au bord droit de la table, et sur toute la hauteur
/// de la fenêtre (`strip`, en-tête et onglets de feuille compris).
///
/// La table (`TableBuilder`) défile verticalement elle-même — c'est ce qui
/// fige son en-tête — mais, dessinée dans la zone de défilement horizontale,
/// sa propre barre suivrait la largeur de ses colonnes. Sa barre est donc
/// masquée, et celle-ci en est le miroir : une `ScrollArea` verticale vide,
/// posée dans la bande réservée, dont la hauteur de contenu est celle du
/// corps de la table mise à l'échelle de la barre
/// ([`grid_vertical_bar_scale`]). Le décalage suit la table (molette,
/// glisser dans la table) ; inversement, un glisser sur cette barre est mis
/// en attente dans la mémoire (`pending_id`), ramené à l'échelle de la table,
/// et appliqué à celle-ci à l'image suivante (`grid_contents`,
/// `pending_vertical_offset`).
fn show_grid_vertical_bar(
    ui: &mut egui::Ui,
    pending_id: egui::Id,
    strip: egui::Rect,
    table: &egui::scroll_area::ScrollAreaOutput<()>,
) {
    if !strip.is_positive() {
        return;
    }
    let scale = grid_vertical_bar_scale(strip.height(), table.inner_rect.height());
    let bar_offset = table.state.offset.y * scale;
    let mut bar_ui = ui.new_child(egui::UiBuilder::new().max_rect(strip));
    let bar = egui::ScrollArea::vertical()
        .id_salt(pending_id)
        .auto_shrink([false, false])
        .vertical_scroll_offset(bar_offset)
        .show(&mut bar_ui, |ui| {
            ui.allocate_exact_size(
                egui::vec2(0.0, table.content_size.y * scale),
                egui::Sense::hover(),
            );
        });
    let dragged_offset = bar.state.offset.y;
    if (dragged_offset - bar_offset).abs() > 0.5 {
        let table_offset = dragged_offset / scale;
        ui.memory_mut(|memory| memory.data.insert_temp(pending_id, table_offset));
        ui.ctx().request_repaint();
    }
}

/// Mémoire d'une image à l'autre de la bande d'onglets de feuille : de quoi
/// savoir, **avant** de la dessiner (donc avant de placer les boutons « < »
/// et « > »), si des onglets sont cachés à gauche ou à droite — ce qui ne se
/// constate qu'en dessinant les onglets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct TabStripOverflow {
    hidden_left: bool,
    hidden_right: bool,
}

/// Ce que [`sheet_tabs`] a dessiné : décalage de la bande, onglets cachés,
/// et rectangles des boutons « < » et « > » s'ils sont affichés. Seuls les
/// tests le lisent ; l'interface n'en a pas besoin.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, Default)]
struct TabStripOutput {
    offset: f32,
    overflow: TabStripOverflow,
    arrows: Option<[egui::Rect; 2]>,
    /// Indices du premier et du dernier onglet entièrement visibles.
    fully_visible: Option<(usize, usize)>,
}

/// **La** bande d'onglets de l'application (standard du projet, issue #19) :
/// tout jeu d'onglets — feuilles d'un classeur, pages de la fenêtre des
/// propriétés du graphe — passe par elle, jamais par une rangée de
/// `selectable_label`. Style d'origine (bouton pleine taille, sans
/// troncature) : quand elle ne tient pas dans la largeur disponible, elle
/// défile horizontalement plutôt que de rétrécir ses onglets ou de replier
/// les feuilles cachées dans un menu « … ». Elle n'a **pas** de barre de
/// défilement : deux boutons « < » et « > », à gauche de la bande, font
/// apparaître l'onglet caché suivant de ce côté — et n'apparaissent que si
/// des onglets sont cachés. Changer de feuille sélectionnée (au clavier, par
/// script, ou par clic) fait défiler la bande juste assez pour montrer
/// l'onglet actif et ses voisins ([`scroll_tab_strip_to_selected`]).
fn tab_strip<K: Copy + PartialEq + Send + Sync + 'static>(
    ui: &mut egui::Ui,
    tabs: &[(K, String)],
    selected: K,
    mut on_select: impl FnMut(K),
    add_label: Option<&str>,
    mut on_add: impl FnMut(),
) -> TabStripOutput {
    let add_menu = |ui: &mut egui::Ui, requested_add: &mut bool| {
        if let Some(label) = add_label {
            if ui.button(label).clicked() {
                *requested_add = true;
                ui.close();
            }
        }
    };
    if tabs.is_empty() {
        let mut requested_add = false;
        ui.allocate_response(
            egui::vec2(ui.available_width(), TAB_STRIP_HEIGHT),
            egui::Sense::click(),
        )
        .context_menu(|ui| add_menu(ui, &mut requested_add));
        if requested_add {
            on_add();
        }
        return TabStripOutput::default();
    }
    // Mémorisée par instance de bande (scopée par la pile d'`Id` du parent,
    // donc distincte entre fenêtre flottante et fenêtre détachée) : ne
    // redéclenche le défilement automatique que lorsque la sélection change
    // réellement d'une image à l'autre, pas à chaque image où elle est
    // simplement reconduite — sinon un défilement manuel de l'utilisateur
    // serait annulé en continu.
    let last_selected_id = ui.id().with("tab_strip_last_selected");
    let last_selected: Option<K> = ui.memory(|memory| memory.data.get_temp(last_selected_id));
    let should_scroll_to_selected = last_selected != Some(selected);
    ui.memory_mut(|memory| memory.data.insert_temp(last_selected_id, selected));
    let overflow_id = ui.id().with("tab_strip_overflow");
    let overflow: TabStripOverflow = ui
        .memory(|memory| memory.data.get_temp(overflow_id))
        .unwrap_or_default();

    let mut requested = None;
    let mut requested_add = false;
    // -1 : faire apparaître l'onglet caché à gauche ; +1 : à droite.
    let mut step: i8 = 0;
    let mut new_overflow = TabStripOverflow::default();
    let mut arrows = None;
    let mut offset = 0.0;
    let mut fully_visible: Option<(usize, usize)> = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        if overflow.hidden_left || overflow.hidden_right {
            let arrow =
                |label: &str| egui::Button::new(egui::RichText::new(label).size(12.0)).small();
            let previous = ui
                .add_enabled(overflow.hidden_left, arrow("<"))
                .on_hover_text("Feuilles précédentes");
            let next = ui
                .add_enabled(overflow.hidden_right, arrow(">"))
                .on_hover_text("Feuilles suivantes");
            if previous.clicked() {
                step = -1;
            }
            if next.clicked() {
                step = 1;
            }
            arrows = Some([previous.rect, next.rect]);
        }
        let band_width = ui.available_width();
        offset = egui::ScrollArea::horizontal()
            .id_salt("tab_strip_scroll")
            .auto_shrink([false, true])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let visible = ui.clip_rect();
                    let mut last_hidden_left = None;
                    let mut first_hidden_right = None;
                    let mut tab_rects = Vec::with_capacity(tabs.len());
                    let mut selected_index = None;
                    for (index, (id, name)) in tabs.iter().enumerate() {
                        let tab = egui::Button::new(egui::RichText::new(name).size(12.0))
                            .small()
                            .fill(if selected == *id {
                                ui.visuals().selection.bg_fill
                            } else {
                                egui::Color32::TRANSPARENT
                            });
                        let response = ui.add(tab);
                        response.context_menu(|ui| add_menu(ui, &mut requested_add));
                        if response.clicked() {
                            requested = Some(*id);
                        }
                        tab_rects.push(response.rect);
                        if selected == *id {
                            selected_index = Some(index);
                        }
                        if response.rect.left() >= visible.left() - 0.5
                            && response.rect.right() <= visible.right() + 0.5
                        {
                            let first = fully_visible.map_or(index, |(first, _)| first);
                            fully_visible = Some((first, index));
                        }
                        if response.rect.left() < visible.left() - 0.5 {
                            last_hidden_left = Some(response.clone());
                        }
                        if first_hidden_right.is_none()
                            && response.rect.right() > visible.right() + 0.5
                        {
                            first_hidden_right = Some(response.clone());
                        }
                    }
                    new_overflow = TabStripOverflow {
                        hidden_left: last_hidden_left.is_some(),
                        hidden_right: first_hidden_right.is_some(),
                    };
                    if should_scroll_to_selected {
                        if let Some(index) = selected_index {
                            scroll_tab_strip_to_selected(ui, &tab_rects, index, visible);
                        }
                    }
                    match step {
                        -1 => {
                            if let Some(tab) = &last_hidden_left {
                                tab.scroll_to_me(Some(egui::Align::Min));
                            }
                        }
                        1 => {
                            if let Some(tab) = &first_hidden_right {
                                tab.scroll_to_me(Some(egui::Align::Max));
                            }
                        }
                        _ => {}
                    }
                    let used = ui.cursor().left() - ui.min_rect().left();
                    let empty = ui.allocate_response(
                        egui::vec2((band_width - used).max(1.0), TAB_STRIP_HEIGHT),
                        egui::Sense::click(),
                    );
                    empty.context_menu(|ui| add_menu(ui, &mut requested_add));
                });
            })
            .state
            .offset
            .x;
    });
    ui.memory_mut(|memory| memory.data.insert_temp(overflow_id, new_overflow));
    if step != 0 {
        ui.ctx().request_repaint();
    }
    if let Some(id) = requested {
        on_select(id);
    }
    if requested_add {
        on_add();
    }
    TabStripOutput {
        offset,
        overflow: new_overflow,
        arrows,
        fully_visible,
    }
}

/// Bande d'onglets des feuilles d'un classeur : [`tab_strip`] avec, en plus,
/// « Ajouter une feuille » au clic droit.
fn sheet_tabs(
    ui: &mut egui::Ui,
    sheets: &[(NodeId, String)],
    selected: NodeId,
    on_select: impl FnMut(NodeId),
    on_add: impl FnMut(),
) -> TabStripOutput {
    tab_strip(
        ui,
        sheets,
        selected,
        on_select,
        Some("Ajouter une feuille"),
        on_add,
    )
}

/// Défilement automatique de la bande d'onglets quand la feuille
/// sélectionnée change : **le plus petit décalage** qui rend entièrement
/// visibles l'onglet sélectionné et ses deux voisins immédiats (pour qu'on
/// voie toujours ce qui vient ensuite). Rien ne bouge s'ils le sont déjà ; sinon
/// la bande s'aligne sur le bord dépassé. Si la bande est trop étroite pour
/// ces trois onglets, l'onglet sélectionné est centré.
///
/// Issue #1 : la bande centrait auparavant l'onglet sélectionné à chaque
/// changement — cliquer sur le dernier onglet visible décalait alors de deux
/// onglets ou plus au lieu d'un.
fn scroll_tab_strip_to_selected(
    ui: &egui::Ui,
    tab_rects: &[egui::Rect],
    selected: usize,
    visible: egui::Rect,
) {
    let current = tab_rects[selected];
    let first = tab_rects[selected.saturating_sub(1)];
    let last = tab_rects[(selected + 1).min(tab_rects.len() - 1)];
    let target = first.union(last);
    if target.width() > visible.width() {
        ui.scroll_to_rect(current, Some(egui::Align::Center));
    } else if target.right() > visible.right() + 0.5 {
        ui.scroll_to_rect(target, Some(egui::Align::Max));
    } else if target.left() < visible.left() - 0.5 {
        ui.scroll_to_rect(target, Some(egui::Align::Min));
    }
}

/// Ramène `top_layer_tab` en fin de `floating_tabs` (§ ordre de pile,
/// `active_tab = floating_tabs.last()`), sans rien faire s'il n'y est déjà,
/// s'il n'existe pas, ou si aucune fenêtre flottante n'est au premier plan
/// (B2, `retours-2026-09-23-bugs.md`). `egui` gère seul son
/// propre ordre de calques (un clic direct dans une fenêtre l'amène déjà au
/// premier plan visuellement) ; cette fonction ne fait que réaligner
/// `floating_tabs`, seule source de `active_tab`, dessus.
fn sync_floating_tabs_order_to_top_layer(floating_tabs: &mut Vec<Tab>, top_layer_tab: Option<Tab>) {
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

#[derive(Default)]
struct WindowHeaderActions {
    maximize: bool,
    /// Détacher (placement [`HeaderPlacement::Floating`]) ou réattacher
    /// (placement [`HeaderPlacement::Detached`]) — le glyphe et l'info-bulle
    /// changent avec `placement`, mais c'est toujours l'appelant qui
    /// interprète ce booléen selon le contexte d'où il peint l'en-tête.
    detach: bool,
    close: bool,
    toggle_collapse: bool,
    /// Delta de glissé du titre pendant l'image — utilisé tel quel par
    /// l'appelant MDI (`window_positions`). Sans effet pratique pour
    /// l'appelant détaché, qui préfère `title_drag_started`.
    drag_delta: egui::Vec2,
    /// Vrai à l'image où le glissé du titre démarre — c'est celui que
    /// l'appelant détaché utilise pour déclencher
    /// `ViewportCommand::StartDrag` une seule fois, la fenêtre OS prenant
    /// ensuite la main sur le geste (`drag_delta` ne serait plus fiable une
    /// fois l'OS aux commandes du pointeur).
    title_drag_started: bool,
}

/// D'où [`window_header`] est peint — pilote la couleur des icônes, le
/// glyphe « détacher »/« réattacher » et quels boutons apparaissent. Deux
/// contextes, une seule fonction : c'est la barre elle-même qui doit être
/// identique, pas seulement son apparence générale (voir la doc de tête du
/// module, § détachement générique).
#[derive(Clone, Copy, PartialEq, Eq)]
enum HeaderPlacement {
    /// Fenêtre flottante dans le `CentralPanel` (MDI).
    Floating,
    /// Fenêtre OS séparée (`PlotyxApp::show_detached_tabs`), sans
    /// décoration native — cette barre est tout ce que l'utilisateur a pour
    /// la déplacer, la redimensionner par ses icônes de maximisation, ou la
    /// fermer/réattacher.
    Detached,
    /// Dialogue modal ([`modal_dialog`]) : ni repli, ni détachement, ni
    /// maximisation — le titre, la croix, et le glisser.
    Dialog,
}

/// Une icône d'en-tête peinte au pixel près, comme le titre et le chevron de
/// repli — jamais un `Button` (voir la doc de tête de [`window_header`] pour
/// la raison précise). `rect` est déjà borné à la hauteur de la barre ; le
/// fond arrondi au survol imite l'affordance d'un bouton sans réintroduire
/// son padding natif.
fn header_icon(
    header: &mut egui::Ui,
    base_id: egui::Id,
    id_source: &str,
    rect: egui::Rect,
    glyph: &str,
    tooltip: &str,
    color: egui::Color32,
) -> bool {
    let response = header.interact(rect, base_id.with(id_source), egui::Sense::click());
    if response.hovered() {
        header
            .painter()
            .rect_filled(rect, 4.0, header.visuals().widgets.hovered.bg_fill);
    }
    header.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::TextStyle::Button.resolve(header.style()),
        color,
    );
    response.on_hover_text(tooltip).clicked()
}

/// Taille minimale d'une fenêtre de contenu, flottante ou détachée — assez
/// pour garder l'en-tête (icônes + titre) et un fragment de contenu lisibles,
/// pas une contrainte scientifique.
const MIN_CONTENT_WINDOW_SIZE: egui::Vec2 = egui::vec2(220.0, 140.0);

/// Taille fixe de l'espace de travail (zone MDI qui porte les fenêtres
/// flottantes), indépendante de la taille de la fenêtre principale — avant
/// cette tâche, `workspace_rect` était `ui.max_rect()`, donc rétrécissait
/// avec la fenêtre `eframe` et écrasait les fenêtres flottantes contre ses
/// bords. Un banc « généreux » plutôt qu'ajusté à un écran précis ; devenir
/// réellement réglable par l'utilisateur (persistance, UI de réglage) reste à
/// faire — voir `docs/chantiers`. Sous cette taille, [`PlotyxApp::ui`] fait
/// défiler l'espace de travail ([`PlotyxApp::workspace_scroll`]) plutôt que de
/// rétrécir ou tronquer les fenêtres qu'il contient.
const WORKSPACE_SIZE: egui::Vec2 = egui::vec2(1920.0, 1080.0);

/// Défilement maximal (par axe) pour qu'une zone visible de taille
/// `viewport_size` reste entièrement dans [`WORKSPACE_SIZE`] — nul sur un axe
/// où l'espace de travail entre déjà en entier, rien n'y défile alors.
fn workspace_max_scroll(viewport_size: egui::Vec2) -> egui::Vec2 {
    (WORKSPACE_SIZE - viewport_size).max(egui::Vec2::ZERO)
}

/// Position écran d'un point de l'espace de travail (coordonnées logiques,
/// indépendantes du défilement) — l'origine de la zone visible moins le
/// défilement courant.
fn workspace_to_screen(
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
fn workspace_scroll_to_reveal(
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

/// Calque des fenêtres flottantes : celui qu'egui réserve aux fenêtres
/// qu'on réordonne au clic (`Order::Middle`), au-dessus des panneaux fixes
/// (`Order::Background`) et **sous** les menus, listes déroulantes et
/// infobulles (`Order::Foreground`) — un menu contextuel ouvert dans une
/// fenêtre doit passer devant ses voisines.
const FLOATING_WINDOW_ORDER: egui::Order = egui::Order::Middle;

/// Épaisseur des poignées de redimensionnement, posées **hors** du cadre
/// (voir [`floating_resize_handles`]) et comptées dans la zone cliquable de
/// la fenêtre par [`show_floating_window`].
const RESIZE_BORDER: f32 = 6.0;

/// Borne un rectangle logique à l'espace de travail : taille ramenée à
/// [`WORKSPACE_SIZE`] au plus, puis position glissée pour qu'il y tienne en
/// entier. C'est la seule borne appliquée aux fenêtres flottantes — egui
/// n'en ajoute pas d'autre (voir [`show_floating_window`]), donc ce que
/// cette fonction rend est exactement ce qui est peint, entouré et persisté.
fn clamp_to_workspace(rect: egui::Rect) -> egui::Rect {
    let size = rect.size().min(WORKSPACE_SIZE);
    let max_min = (WORKSPACE_SIZE - size).to_pos2();
    egui::Rect::from_min_size(rect.min.clamp(egui::Pos2::ZERO, max_min), size)
}

/// Peint une fenêtre flottante de l'espace de travail, cadre compris, dans
/// son propre calque. `window_rect` est le rectangle écran de la fenêtre,
/// `viewport_rect` la portion visible de l'espace de travail (ce que les
/// panneaux fixes laissent). Rend `None` quand la fenêtre est entièrement
/// hors de la vue : rien n'est alors peint ni rendu cliquable.
///
/// # Pourquoi une `egui::Area` et pas une `egui::Window`
///
/// `egui::Window` est une boîte de dialogue : barre de titre, glisser,
/// redimensionnement, repli et bornage à elle. Toutes ces fonctions avaient
/// été coupées une à une puis refaites à la main ([`window_header`],
/// [`floating_resize_handles`], [`workspace_to_screen`]…), et chaque image
/// devait accorder deux rectangles : celui qu'on demandait et celui
/// qu'egui recalait. Les bugs 1 et 2 de
/// `retours-2026-09-24-bugs.md` (contour décalé d'une
/// fenêtre recalée, fenêtre peinte par-dessus les panneaux fixes) étaient
/// deux formes de ce désaccord. `egui::Area` est la brique nue sous
/// `Window` : un calque, une position, un ordre au clic — rien d'autre. Le
/// cadre, le contour et le rognage sont posés ici, sur le **même**
/// rectangle.
///
/// # La zone cliquable est la partie visible
///
/// egui choisit la fenêtre sous le pointeur d'après le rectangle de son
/// `Area` (`Memory::layer_id_at`), pas d'après ce qui est peint. L'`Area`
/// est donc taillée à la seule partie visible de la fenêtre (poignées
/// comprises) : une fenêtre à cheval sur le bord de la vue ne capte aucun
/// clic destiné au Project Manager ou à la console, et son dessin est rogné
/// au même bord (`constrain_to(viewport_rect)`, qui donne aussi à `Area` son
/// rectangle de découpe).
fn show_floating_window<R>(
    ctx: &egui::Context,
    id: egui::Id,
    window_rect: egui::Rect,
    viewport_rect: egui::Rect,
    is_active: bool,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
    add_resize_handles: impl FnOnce(&egui::Ui, egui::Rect),
) -> Option<R> {
    show_window_in_layer(
        ctx,
        id,
        FLOATING_WINDOW_ORDER,
        window_rect,
        viewport_rect,
        is_active,
        add_contents,
        add_resize_handles,
    )
}

/// [`show_floating_window`] dans l'ordre de calque `order` : la seule
/// écriture du cadre, du contour et du rognage d'une fenêtre, que se
/// partagent les fenêtres de l'espace de travail (`Order::Middle`) et les
/// dialogues modaux ([`modal_dialog`], au-dessus).
#[allow(clippy::too_many_arguments)]
fn show_window_in_layer<R>(
    ctx: &egui::Context,
    id: egui::Id,
    order: egui::Order,
    window_rect: egui::Rect,
    viewport_rect: egui::Rect,
    is_active: bool,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
    add_resize_handles: impl FnOnce(&egui::Ui, egui::Rect),
) -> Option<R> {
    let area_rect = window_rect.expand(RESIZE_BORDER).intersect(viewport_rect);
    if !window_rect.intersects(viewport_rect) || !area_rect.is_positive() {
        return None;
    }
    let response = egui::Area::new(id)
        .order(order)
        .movable(false)
        .fade_in(false)
        .fixed_pos(area_rect.min)
        .constrain_to(viewport_rect)
        .show(ctx, |ui| {
            // La taille de l'`Area` est celle de son contenu : on la fixe à
            // la partie visible, le contenu réel vivant dans un sous-`Ui` qui
            // ne la fait pas grandir.
            ui.set_min_size(area_rect.size());
            let style = ui.style().clone();
            let frame = floating_window_frame(&style);
            let content_rect = window_rect - frame.total_margin();
            ui.painter().add(frame.paint(content_rect));
            let mut content = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(content_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            content.set_clip_rect(content_rect.intersect(ui.clip_rect()));
            let inner = add_contents(&mut content);
            if is_active {
                paint_active_window_outline(
                    ui.painter(),
                    window_rect,
                    style.visuals.window_corner_radius,
                    &style,
                );
            }
            add_resize_handles(ui, window_rect);
            inner
        });
    Some(response.inner)
}

/// Huit zones fines de redimensionnement pour une fenêtre flottante MDI,
/// posées entièrement à l'**extérieur** de `rect` (sur [`RESIZE_BORDER`])
/// pour ne pas disputer le survol au contenu — les barres de défilement de
/// la grille longent le bord. Elles sont peintes dans le calque de la
/// fenêtre ([`show_floating_window`] compte ce liseré dans sa zone
/// cliquable) : la poignée d'une fenêtre ne passe donc jamais devant une
/// autre fenêtre posée par-dessus.
fn floating_resize_handles(
    ui: &egui::Ui,
    tab: impl std::hash::Hash + std::fmt::Debug,
    rect: egui::Rect,
    position: &mut egui::Pos2,
    size: &mut egui::Vec2,
) {
    const BORDER: f32 = RESIZE_BORDER;
    // (source, zone, curseur, ouest, est, nord, sud) — les quatre derniers
    // indiquent quels côtés ce glissé affecte, combinés pour les coins.
    let zones: [(&str, egui::Rect, egui::CursorIcon, bool, bool, bool, bool); 8] = [
        (
            "n",
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.top() - BORDER),
                egui::pos2(rect.right(), rect.top()),
            ),
            egui::CursorIcon::ResizeNorth,
            false,
            false,
            true,
            false,
        ),
        (
            "s",
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.bottom()),
                egui::pos2(rect.right(), rect.bottom() + BORDER),
            ),
            egui::CursorIcon::ResizeSouth,
            false,
            false,
            false,
            true,
        ),
        (
            "w",
            egui::Rect::from_min_max(
                egui::pos2(rect.left() - BORDER, rect.top()),
                egui::pos2(rect.left(), rect.bottom()),
            ),
            egui::CursorIcon::ResizeWest,
            true,
            false,
            false,
            false,
        ),
        (
            "e",
            egui::Rect::from_min_max(
                egui::pos2(rect.right(), rect.top()),
                egui::pos2(rect.right() + BORDER, rect.bottom()),
            ),
            egui::CursorIcon::ResizeEast,
            false,
            true,
            false,
            false,
        ),
        (
            "nw",
            egui::Rect::from_min_size(
                rect.left_top() - egui::vec2(BORDER, BORDER),
                egui::vec2(BORDER, BORDER),
            ),
            egui::CursorIcon::ResizeNwSe,
            true,
            false,
            true,
            false,
        ),
        (
            "ne",
            egui::Rect::from_min_size(
                egui::pos2(rect.right(), rect.top() - BORDER),
                egui::vec2(BORDER, BORDER),
            ),
            egui::CursorIcon::ResizeNeSw,
            false,
            true,
            true,
            false,
        ),
        (
            "sw",
            egui::Rect::from_min_size(
                egui::pos2(rect.left() - BORDER, rect.bottom()),
                egui::vec2(BORDER, BORDER),
            ),
            egui::CursorIcon::ResizeNeSw,
            true,
            false,
            false,
            true,
        ),
        (
            "se",
            egui::Rect::from_min_size(rect.right_bottom(), egui::vec2(BORDER, BORDER)),
            egui::CursorIcon::ResizeNwSe,
            false,
            true,
            false,
            true,
        ),
    ];
    let base_id = egui::Id::new(("floating_resize_handle", tab));
    for (source, zone, cursor, west, east, north, south) in zones {
        let response = ui.interact(zone, base_id.with(source), egui::Sense::drag());
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(cursor);
        }
        let delta = response.drag_delta();
        if delta == egui::Vec2::ZERO {
            continue;
        }
        if west {
            let new_width = (size.x - delta.x).max(MIN_CONTENT_WINDOW_SIZE.x);
            position.x += size.x - new_width;
            size.x = new_width;
        }
        if east {
            size.x = (size.x + delta.x).max(MIN_CONTENT_WINDOW_SIZE.x);
        }
        if north {
            let new_height = (size.y - delta.y).max(MIN_CONTENT_WINDOW_SIZE.y);
            position.y += size.y - new_height;
            size.y = new_height;
        }
        if south {
            size.y = (size.y + delta.y).max(MIN_CONTENT_WINDOW_SIZE.y);
        }
    }
}

/// Bordure invisible de redimensionnement pour une fenêtre détachée sans
/// décoration (`.with_decorations(false)`, voir la doc de tête de
/// [`PlotyxApp::show_detached_tabs`]) : huit zones fines aux bords/coins du
/// viewport, qui délèguent le redimensionnement à l'OS dès que le glissé
/// démarre (`ViewportCommand::BeginResize`) — même principe que
/// `ViewportCommand::StartDrag` pour le déplacement du titre : aucun calcul
/// de delta à notre charge, l'OS/le gestionnaire de fenêtres prend la main.
fn detached_resize_handles(ui: &egui::Ui) {
    const BORDER: f32 = 6.0;
    let rect = ui.ctx().input(|input| input.content_rect());
    let zones: [(&str, egui::Rect, egui::CursorIcon, egui::ResizeDirection); 8] = [
        (
            "n",
            egui::Rect::from_min_max(
                rect.left_top(),
                egui::pos2(rect.right(), rect.top() + BORDER),
            ),
            egui::CursorIcon::ResizeNorth,
            egui::ResizeDirection::North,
        ),
        (
            "s",
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.bottom() - BORDER),
                rect.right_bottom(),
            ),
            egui::CursorIcon::ResizeSouth,
            egui::ResizeDirection::South,
        ),
        (
            "w",
            egui::Rect::from_min_max(
                rect.left_top(),
                egui::pos2(rect.left() + BORDER, rect.bottom()),
            ),
            egui::CursorIcon::ResizeWest,
            egui::ResizeDirection::West,
        ),
        (
            "e",
            egui::Rect::from_min_max(
                egui::pos2(rect.right() - BORDER, rect.top()),
                rect.right_bottom(),
            ),
            egui::CursorIcon::ResizeEast,
            egui::ResizeDirection::East,
        ),
        (
            "nw",
            egui::Rect::from_min_size(rect.left_top(), egui::vec2(BORDER, BORDER)),
            egui::CursorIcon::ResizeNwSe,
            egui::ResizeDirection::NorthWest,
        ),
        (
            "ne",
            egui::Rect::from_min_size(
                egui::pos2(rect.right() - BORDER, rect.top()),
                egui::vec2(BORDER, BORDER),
            ),
            egui::CursorIcon::ResizeNeSw,
            egui::ResizeDirection::NorthEast,
        ),
        (
            "sw",
            egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.bottom() - BORDER),
                egui::vec2(BORDER, BORDER),
            ),
            egui::CursorIcon::ResizeNeSw,
            egui::ResizeDirection::SouthWest,
        ),
        (
            "se",
            egui::Rect::from_min_size(
                egui::pos2(rect.right() - BORDER, rect.bottom() - BORDER),
                egui::vec2(BORDER, BORDER),
            ),
            egui::CursorIcon::ResizeNwSe,
            egui::ResizeDirection::SouthEast,
        ),
    ];
    let base_id = egui::Id::new("detached_resize_handle");
    for (source, zone, cursor, direction) in zones {
        let response = ui.interact(zone, base_id.with(source), egui::Sense::drag());
        if response.hovered() {
            ui.ctx().set_cursor_icon(cursor);
        }
        if response.drag_started() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        }
    }
}

/// Barre de titre des fenêtres de contenu — la même base de code pour les
/// fenêtres flottantes MDI et les fenêtres OS détachées (`placement`, voir
/// [`HeaderPlacement`]). Les actions et la croix sont peintes de la même
/// façon que le titre (voir [`header_icon`]) et forment un seul groupe
/// visuel ; cela évite aussi une `Area` superposée, qui se décale quand une
/// fenêtre passe derrière une autre.
fn window_header(
    ui: &mut egui::Ui,
    id: egui::Id,
    title: &str,
    tab: Option<Tab>,
    collapsed: bool,
    placement: HeaderPlacement,
) -> WindowHeaderActions {
    const ACTIONS_WIDTH: f32 = 90.0;
    const COLLAPSE_WIDTH: f32 = 24.0;
    const ACTION_WIDTH: f32 = 24.0;

    let mut actions = WindowHeaderActions::default();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), WINDOW_HEADER_HEIGHT),
        egui::Sense::hover(),
    );
    let mut header = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let icon_color = match placement {
        HeaderPlacement::Floating | HeaderPlacement::Dialog => header.visuals().text_color(),
        HeaderPlacement::Detached => theme::gutter_text_color(),
    };
    // B5 (`retours-2026-09-23-bugs.md`) : l'icône de repli
    // était réservée à `Tab::Graph`, sans justification documentée — une
    // extension jamais faite plutôt qu'un choix. `Matrix`/`Note` restent
    // exclus faute de raison de les y ajouter.
    let collapse_width = if matches!(tab, Some(Tab::Graph(_) | Tab::Book(_))) {
        COLLAPSE_WIDTH
    } else {
        0.0
    };
    if collapse_width > 0.0 {
        let collapse_rect =
            egui::Rect::from_min_size(rect.min, egui::vec2(collapse_width, rect.height()));
        let content_label = match tab {
            Some(Tab::Book(_)) => "classeur",
            _ => "graphe",
        };
        let (icon, tooltip) = if collapsed {
            (">", format!("Afficher le contenu du {content_label}"))
        } else {
            ("v", format!("Masquer le contenu du {content_label}"))
        };
        if header_icon(
            &mut header,
            id,
            "collapse",
            collapse_rect,
            icon,
            &tooltip,
            icon_color,
        ) {
            actions.toggle_collapse = true;
        }
    }
    let action_rect = |offset: f32| {
        egui::Rect::from_min_size(
            egui::pos2(rect.right() - offset - ACTION_WIDTH, rect.top() + 2.0),
            egui::vec2(ACTION_WIDTH, rect.height() - 4.0),
        )
    };
    if header_icon(
        &mut header,
        id,
        "close",
        action_rect(2.0),
        "×",
        "Fermer la fenêtre",
        icon_color,
    ) {
        actions.close = true;
    }
    // En MDI, détacher/maximiser restent réservés à Book/Graph (Matrix/Note
    // n'ont pas ce chemin d'UI aujourd'hui, même si `show_detached_tabs` les
    // gère déjà génériquement). En fenêtre détachée, les quatre natures
    // gardent réattacher/maximiser en toutes circonstances — c'est ce qui
    // remplace la barre de titre native de l'OS retirée avec elle
    // (`.with_decorations(false)`), donc pas question d'en perdre une
    // capacité pour certains onglets.
    let show_detach_and_maximize = match placement {
        HeaderPlacement::Floating => matches!(tab, Some(Tab::Book(_) | Tab::Graph(_))),
        HeaderPlacement::Detached => true,
        HeaderPlacement::Dialog => false,
    };
    if show_detach_and_maximize {
        let (detach_glyph, detach_tooltip) = match placement {
            HeaderPlacement::Floating | HeaderPlacement::Dialog => {
                ("↗", "Détacher dans une fenêtre")
            }
            HeaderPlacement::Detached => ("↩", "Réattacher à la fenêtre principale"),
        };
        if header_icon(
            &mut header,
            id,
            "detach",
            action_rect(28.0),
            detach_glyph,
            detach_tooltip,
            icon_color,
        ) {
            actions.detach = true;
        }
        if header_icon(
            &mut header,
            id,
            "maximize",
            action_rect(54.0),
            "⛶",
            "Maximiser la fenêtre",
            icon_color,
        ) {
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
    if drag.drag_started() {
        actions.title_drag_started = true;
    }
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

/// [`tab_title`], à partir d'un `Tree` déjà en main plutôt que d'un
/// `&AppState` vivant — pour l'en-tête peint par une fenêtre détachée, qui
/// n'a que le [`Snapshot`] resynchronisé dans `DetachedTabViewModel`
/// (voir la doc de tête du module, § « la lecture du modèle est
/// factorisée »).
fn tab_title_from_tree(tree: &Tree, tab: Tab) -> String {
    match tab {
        Tab::Book(id) => node_title_in(tree, id, "Classeur"),
        Tab::Graph(id) => node_title_in(tree, id, "Graphe"),
        Tab::Matrix(id) => node_title_in(tree, id, "Matrice"),
        Tab::Note(id) => node_title_in(tree, id, "Note"),
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
        Tab::Matrix(id) => matrix_contents(ui, state.tree(), id),
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
/// de [`PlotyxApp::show_detached_project_manager`]).
struct PmInteraction<'a> {
    /// Onglet ouvert ou focalisé par un simple clic — voir
    /// [`PlotyxApp::open_tab`].
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
    /// Onglets actuellement masqués (voir [`PlotyxApp::hidden_tabs`]) —
    /// affichés atténués dans l'arbre, et ce que « Afficher »/« Masquer »
    /// modifient pour la sélection courante.
    hidden: &'a mut HashSet<Tab>,
}

/// Le corps du Project Manager, à partir d'un arbre déjà construit plutôt
/// que d'un `&AppState` vivant — seul le callback `show_viewport_deferred`
/// de [`PlotyxApp::show_detached_project_manager`] a besoin de cette forme :
/// il ne porte qu'une copie de l'arbre ([`ProjectManagerViewModel`]), jamais
/// `AppState` elle-même. Le panneau fixe ([`PlotyxApp::project_manager_panel`])
/// passe directement `&self.state.project_tree()` — même fonction, deux
/// sources d'arbre.
fn project_manager_contents_from_tree(
    ui: &mut egui::Ui,
    tree: &[ProjectTreeNode],
    interaction: &mut PmInteraction,
) {
    ui.heading("Project Manager");
    ui.separator();
    if !interaction.selection.is_empty() {
        // Barre d'actions groupées, seulement quand elle a un effet — pas de
        // boutons désactivés à regarder tant que rien n'est sélectionné.
        ui.horizontal(|ui| {
            if ui.button("Afficher").clicked() {
                for &tab in interaction.selection.iter() {
                    interaction.hidden.remove(&tab);
                    interaction.open_requests.push(tab);
                }
            }
            if ui.button("Masquer").clicked() {
                for &tab in interaction.selection.iter() {
                    interaction.hidden.insert(tab);
                }
            }
            ui.weak(format!("{} sélectionné(s)", interaction.selection.len()));
        });
        ui.separator();
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        for node in tree {
            project_tree_node_ui(ui, node, interaction);
        }
    });
}

/// Peint récursivement un nœud de [`ProjectTreeNode`] et ses enfants.
fn project_tree_node_ui(
    ui: &mut egui::Ui,
    node: &ProjectTreeNode,
    interaction: &mut PmInteraction,
) {
    match node {
        ProjectTreeNode::Folder { name, children, .. } => {
            egui::CollapsingHeader::new(format!("📁 {name}"))
                .default_open(false)
                .show(ui, |ui| {
                    if children.is_empty() {
                        ui.weak("(vide)");
                    }
                    for child in children {
                        project_tree_node_ui(ui, child, interaction);
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
            let tab = tab_for_window(*id, *kind);
            let selected = interaction.selection.contains(&tab);
            let is_hidden = interaction.hidden.contains(&tab);
            let label = format!("{icon} {name}");
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
                            if is_hidden {
                                ui.add(egui::Button::new(egui::RichText::new(&label).weak()))
                            } else {
                                ui.button(&label)
                            }
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
        if metadata_row_visible(ui.ctx()) {
            metadata_cell_widget(ui, state, actions, column);
        }
    });
}

/// Case « Métadonnées » de l'en-tête : le résumé des métadonnées du fichier
/// d'origine (« 23 clés »), en lecture seule. Un clic ouvre la fenêtre qui les
/// liste ([`column_metadata_window`]). Hors sélection de champ : il n'y a rien
/// à copier, effacer ni éditer.
fn metadata_cell_widget(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    column: NodeId,
) {
    ui.scope(|ui| {
        ui.set_min_height(HEADER_SUBROW_HEIGHT);
        ui.set_max_height(HEADER_SUBROW_HEIGHT);
        let rect = ui.available_rect_before_wrap();
        header_row_separator(ui, rect);
        let summary = state.file_metadata_summary(column);
        if summary.is_empty() {
            return;
        }
        let response = ui.interact(
            rect,
            ui.id().with(("plotyx_metadata_cell", column.raw())),
            egui::Sense::click(),
        );
        ui.put(
            rect,
            egui::Label::new(
                egui::RichText::new(format!("📋 {summary}"))
                    .color(crate::theme::gutter_text_color()),
            )
            .selectable(false)
            .truncate(),
        );
        if response.clicked() {
            actions.push(Action::ViewColumnMetadata { column });
        }
        response.on_hover_text("Métadonnées du fichier importé — cliquer pour consulter");
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
                    .desired_width(f32::INFINITY)
                    // #38 : même filtre que la cellule — voir `GRID_KEYS`.
                    .event_filter(GRID_KEYS),
            );
            // Même piège que [`cell_widget`] : lire `lost_focus()` avant de
            // décider de redemander le focus, sous peine de se le redonner
            // à soi-même juste après qu'Entrée l'ait fait perdre en interne
            // au `TextEdit`, et de ne plus jamais valider.
            let lost_focus = response.lost_focus();
            if !response.has_focus() && !lost_focus {
                response.request_focus();
            }
            // Tab ne fait plus perdre le focus au champ (filtre `GRID_KEYS`) :
            // même lecture explicite que dans [`cell_widget`].
            let tab_pressed =
                response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Tab));
            if tab_pressed {
                response.surrender_focus();
            }
            if lost_focus || tab_pressed {
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
        if metadata_row_visible(ui.ctx()) {
            ui.scope(|ui| {
                ui.set_min_height(HEADER_SUBROW_HEIGHT);
                ui.set_max_height(HEADER_SUBROW_HEIGHT);
                let rect = ui.available_rect_before_wrap();
                ui.painter()
                    .rect_filled(rect, 0.0, gutter_fill(false, false));
                header_row_separator(ui, rect);
                let response = ui.put(
                    rect,
                    egui::Label::new(
                        egui::RichText::new("Metadata")
                            .strong()
                            .color(crate::theme::gutter_text_color()),
                    )
                    .selectable(false)
                    .sense(egui::Sense::click()),
                );
                // Miroir du menu Fenêtre : rien n'est atteignable au seul
                // clic droit.
                response.context_menu(|ui| {
                    if ui.button("Masquer la ligne Metadata").clicked() {
                        set_metadata_row_visible(ui.ctx(), false);
                        ui.close();
                    }
                });
            });
        }
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

/// Contenu commun d'un [`Tab::Book`] flottant ou détaché : la grille
/// ([`grid_contents`]) scopée à la feuille choisie *dans ce classeur*, et
/// la sous-navigation entre les feuilles — des onglets compacts en **bas**
/// de la grille (choix assumé, pas une convention préexistante du crate :
/// aucun autre onglet de sheet de ce genre n'existait à imiter). Les
/// boutons « Insérer/Supprimer 1 ligne » ont disparu : l'ajout de ligne se
/// fait désormais en pressant Entrée sur la dernière cellule d'une colonne
/// ([`Action::CommitCellAndAdvance`]), la suppression par la touche Suppr
/// sur la sélection ([`Action::DeleteSelectedCells`]).
fn book_contents(ui: &mut egui::Ui, state: &mut AppState, actions: &mut Vec<Action>, book: NodeId) {
    claim_file_drop(ui, book);
    let sheets = state.worksheets_of(book);
    let selected = state.selected_sheet_in(book).unwrap_or(book);
    let mut chosen_sheet = None;
    let mut add_sheet = false;
    // La bande d'onglets est retranchée du rectangle du classeur **avant**
    // de construire la grille : réserver une hauteur dans `allocate_ui` ne
    // suffisait pas, sa `ScrollArea` pouvait ensuite faire grandir la taille
    // consommée et repousser les onglets hors du cadre. Pas d'`egui::Panel`
    // pour ce découpage : il remplace le rognage de son parent par le sien
    // au lieu de le combiner, et dessinait les onglets sous la vue quand la
    // fenêtre y était coupée (`show_floating_window`). Deux sous-`Ui` bornés,
    // chacun rogné à l'intersection de son rectangle et du rognage parent.
    let area = ui.available_rect_before_wrap();
    // Le bord droit de la fenêtre est réservé, sur toute sa hauteur, à la
    // barre de défilement verticale (B10) : la zone de défilement
    // horizontale et les onglets de feuille ne portent que le reste, et la
    // barre reste plaquée au bord de la fenêtre quelle que soit la largeur
    // des colonnes. Le bas est réservé à la barre horizontale de la grille,
    // **sous** les onglets de feuille, comme dans un tableur. Du haut vers le
    // bas : grille, onglets, barre horizontale.
    let bar_width = theme::scroll_bar_allocated_width();
    let (main_area, vertical_bar_strip) =
        area.split_left_right_at_x(area.right() - bar_width.min(area.width()));
    let (upper_area, horizontal_bar_strip) =
        main_area.split_top_bottom_at_y(main_area.bottom() - bar_width.min(main_area.height()));
    let tabs_height = TAB_STRIP_HEIGHT.min(upper_area.height());
    let (grid_rect, tabs_rect) =
        upper_area.split_top_bottom_at_y(upper_area.bottom() - tabs_height);
    let mut bounded = |rect: egui::Rect| {
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(ui.clip_rect().intersect(rect));
        child
    };
    let mut tabs_ui = bounded(tabs_rect);
    let mut grid_ui = bounded(grid_rect);
    sheet_tabs(
        &mut tabs_ui,
        &sheets,
        selected,
        |id| chosen_sheet = Some(id),
        || add_sheet = true,
    );
    if sheets.is_empty() {
        grid_ui.centered_and_justified(|ui| {
            ui.weak(format!(
                "« {} » n'a aucune feuille (ou a été supprimé).",
                node_title(state, book, "Classeur")
            ));
        });
    } else {
        let vertical_bar_id = grid_ui.id().with("grid_vertical_bar");
        let horizontal_bar_id = grid_ui.id().with("grid_horizontal_bar");
        let pending_offset: Option<f32> =
            grid_ui.memory_mut(|memory| memory.data.remove_temp(vertical_bar_id));
        let pending_horizontal_offset: Option<f32> =
            grid_ui.memory_mut(|memory| memory.data.remove_temp(horizontal_bar_id));
        let scroll =
            show_horizontally_scrollable_grid(&mut grid_ui, pending_horizontal_offset, |ui| {
                grid_contents(ui, state, actions, selected, pending_offset)
            });
        let mut horizontal_bar_ui = bounded(horizontal_bar_strip);
        show_grid_horizontal_bar(
            &mut horizontal_bar_ui,
            horizontal_bar_id,
            horizontal_bar_strip,
            &scroll,
        );
        if let Some(table) = scroll.inner {
            let mut bar_ui = bounded(vertical_bar_strip);
            show_grid_vertical_bar(&mut bar_ui, vertical_bar_id, vertical_bar_strip, &table);
        }
    }
    ui.advance_cursor_after_rect(area);
    if let Some(id) = chosen_sheet {
        state.dispatch(Action::SelectSheet(id));
    }
    if add_sheet {
        actions.push(Action::AddWorksheet { book });
    }
}

/// Libellé affiché pour un [`Separator`] dans la fenêtre d'import.
fn separator_label(separator: &Separator) -> &'static str {
    match separator {
        Separator::Comma => "Virgule",
        Separator::Semicolon => "Point-virgule",
        Separator::Tab => "Tabulation",
        Separator::Whitespace => "Espaces",
        Separator::Custom(_) => "Autre caractère",
        Separator::FixedWidth(_) => "Largeur fixe",
        Separator::Auto => "Auto",
    }
}

/// Libellé d'un encodage forcé.
fn encoding_label(encoding: plotyx_import::Encoding) -> &'static str {
    use plotyx_import::Encoding;
    match encoding {
        Encoding::Auto => "Automatique",
        Encoding::Utf8 => "UTF-8",
        Encoding::Windows1252 => "Windows-1252",
        Encoding::Latin1 => "ISO 8859-1",
        Encoding::Utf16Le => "UTF-16 (petit-boutiste)",
        Encoding::Utf16Be => "UTF-16 (gros-boutiste)",
    }
}

/// Libellé d'un séparateur de milliers.
fn thousands_label(thousands: Option<char>) -> &'static str {
    match thousands {
        None => "Aucun",
        Some(' ') => "Espace",
        Some(',') => "Virgule",
        Some('.') => "Point",
        Some('\'') => "Apostrophe",
        Some(_) => "Autre",
    }
}

/// Libellé affiché pour un [`DecimalStyle`] dans la fenêtre d'import.
fn decimal_label(decimal: DecimalStyle) -> &'static str {
    match decimal {
        DecimalStyle::Dot => "Point",
        DecimalStyle::Comma => "Virgule",
        DecimalStyle::Auto => "Auto",
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
        LineRole::Comment | LineRole::Skipped => ui.visuals().weak_text_color(),
        LineRole::Data | LineRole::Blank => ui.visuals().text_color(),
    }
}

/// Couleur et libellé de chaque code de l'aperçu (légende colorée).
fn preview_legend(ui: &mut egui::Ui) {
    use plotyx_import::LineRole;
    ui.horizontal_wrapped(|ui| {
        for (role, label) in [
            (LineRole::MainHeader, "en-tête"),
            (LineRole::SubheaderLongName, "nom long"),
            (LineRole::SubheaderUnit, "unité"),
            (LineRole::SubheaderComment, "commentaire"),
            (LineRole::ColumnHeaderRow, "noms de colonnes"),
            (LineRole::Data, "données"),
            (LineRole::Footer, "pied de page"),
            (LineRole::Comment, "commentaire ignoré"),
            (LineRole::Skipped, "ligne ignorée"),
        ] {
            let color = line_role_color(ui, role);
            let code = match role {
                LineRole::Data => "·",
                other => other.code(),
            };
            ui.colored_label(color, format!("{code} {label}"));
        }
    });
}

/// L'aperçu du découpage d'un fichier, aligné en colonnes, une ligne par ligne
/// — voir `plotyx_import::preview` pour ce qui borne son coût. Un clic droit
/// sur une ligne règle l'import à cet endroit (`ImportLineAction`). Une
/// cellule qui deviendrait un texte conservé dans une colonne mixte est
/// annotée en couleur d'avertissement.
fn import_line_preview_widget(
    ui: &mut egui::Ui,
    preview: &plotyx_import::FilePreview,
    auto_cut: bool,
    chosen: &mut Option<(ImportLineAction, usize)>,
) {
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
    egui::ScrollArea::both()
        .id_salt("plotyx_import_preview_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("plotyx_import_preview_grid")
                .num_columns(2)
                .spacing([8.0, 1.0])
                .show(ui, |ui| {
                    for line in &preview.lines {
                        let color = line_role_color(ui, line.role);
                        let number = ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!(
                                    "{:>5} {:<2}",
                                    line.number,
                                    line.role.code()
                                ))
                                .monospace()
                                .color(color),
                            )
                            .sense(egui::Sense::click()),
                        );
                        number.context_menu(|ui| {
                            let on_data = matches!(
                                line.role,
                                plotyx_import::LineRole::Data | plotyx_import::LineRole::Footer
                            );
                            if ui.button("Les données commencent ici").clicked() {
                                *chosen = Some((ImportLineAction::DataStartsHere, line.number));
                                ui.close();
                            }
                            if ui.button("Cette ligne donne les unités").clicked() {
                                *chosen = Some((ImportLineAction::UnitsLine, line.number));
                                ui.close();
                            }
                            if ui
                                .add_enabled(on_data, egui::Button::new("Fin des données ici"))
                                .clicked()
                            {
                                *chosen = Some((ImportLineAction::DataEndsHere, line.number));
                                ui.close();
                            }
                            if ui
                                .add_enabled(
                                    auto_cut,
                                    egui::Button::new("Annuler la coupure automatique"),
                                )
                                .clicked()
                            {
                                *chosen = Some((ImportLineAction::NoAutoCut, line.number));
                                ui.close();
                            }
                        });
                        match &line.cells {
                            Some(cells) => {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 14.0;
                                    for cell in cells {
                                        let cell_color = if cell.is_text {
                                            ui.visuals().warn_fg_color
                                        } else {
                                            ui.visuals().text_color()
                                        };
                                        ui.add_sized(
                                            [90.0, 14.0],
                                            egui::Label::new(
                                                egui::RichText::new(&cell.text)
                                                    .monospace()
                                                    .color(cell_color),
                                            )
                                            .truncate(),
                                        );
                                    }
                                });
                            }
                            None => {
                                ui.label(egui::RichText::new(&line.text).monospace().color(color));
                            }
                        }
                        ui.end_row();
                    }
                });
        });
}

/// Volet de droite de l'assistant, visible à toutes les étapes : choix du
/// fichier montré, encodage, légende et aperçu.
fn import_preview_pane(
    ui: &mut egui::Ui,
    state: &mut AppState,
    chosen: &mut Option<(ImportLineAction, usize)>,
) {
    let Some(import) = state.import_state() else {
        return;
    };
    let names: Vec<String> = import
        .paths
        .iter()
        .map(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string()
        })
        .collect();
    let current = import.preview_index.min(names.len().saturating_sub(1));
    let auto_cut = matches!(import.recipe.data_end, plotyx_import::DataEnd::Auto);
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.strong("Aperçu");
        if names.len() > 1 {
            egui::ComboBox::from_id_salt("plotyx_import_preview_file")
                .selected_text(&names[current])
                .show_ui(ui, |ui| {
                    for (i, name) in names.iter().enumerate() {
                        if ui.selectable_label(i == current, name).clicked() {
                            picked = Some(i);
                        }
                    }
                });
        } else if let Some(name) = names.first() {
            ui.label(name);
        }
        if let Some(encoding) = state.import_encoding() {
            ui.weak(format!("encodage : {encoding}"));
        }
        if let Some(Ok(preview)) = state.import_line_preview() {
            if preview.block_count > 1 {
                ui.weak(format!("{} blocs — aperçu du premier", preview.block_count));
            }
        }
    });
    if let Some(i) = picked {
        state.set_import_preview_index(i);
    }
    preview_legend(ui);
    ui.weak("Clic droit sur une ligne pour régler l'import à cet endroit.");
    match state.import_line_preview() {
        None => {
            ui.weak("Choisissez un fichier pour voir un aperçu.");
        }
        Some(Err(message)) => {
            ui.colored_label(egui::Color32::RED, message);
        }
        Some(Ok(file_preview)) => {
            import_line_preview_widget(ui, &file_preview, auto_cut, chosen);
        }
    }
}

/// Ce que l'assistant demande à l'application après une image, une fois
/// l'emprunt de l'état relâché.
#[derive(Default)]
struct ImportWizardRequests {
    save_recipe: bool,
    cancel: bool,
    auto_detect: bool,
    back: bool,
    forward: bool,
    line_action: Option<(ImportLineAction, usize)>,
    import: bool,
}

fn recipe_combo_text(name: &str, modified: bool) -> String {
    if modified {
        format!("{name} (modifié)")
    } else {
        name.to_string()
    }
}

/// Étape 1 : fichiers et modèle.
fn import_step_files(ui: &mut egui::Ui, state: &mut AppState) {
    let (names, current_name, modified, confirm) = {
        let Some(import) = state.import_state() else {
            return;
        };
        (
            import
                .store
                .recipes()
                .iter()
                .map(|r| r.name.clone())
                .collect::<Vec<_>>(),
            import.recipe.name.clone(),
            state.import_recipe_modified(),
            import.confirm_delete.clone(),
        )
    };
    ui.horizontal(|ui| {
        ui.label("Modèle :");
        egui::ComboBox::from_id_salt("plotyx_import_recipe_combo")
            .selected_text(recipe_combo_text(&current_name, modified))
            .show_ui(ui, |ui| {
                for name in &names {
                    if ui.selectable_label(*name == current_name, name).clicked() {
                        state.select_import_recipe(name);
                    }
                }
            });
        if ui.button("Nouveau").clicked() {
            state.new_import_recipe();
        }
        // Un modèle fourni d'office ne se supprime pas (il reviendrait au
        // lancement suivant) : on l'enregistre sous un autre nom pour le
        // modifier.
        let can_delete = names.contains(&current_name)
            && !state
                .import_state()
                .is_some_and(|import| plotyx_import::RecipeStore::is_builtin(&import.recipe));
        if ui
            .add_enabled(can_delete, egui::Button::new("Supprimer…"))
            .clicked()
        {
            state.request_delete_import_recipe(&current_name);
        }
    });
    ui.horizontal(|ui| {
        if ui.button("Exporter le modèle…").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Exporter le modèle d'import")
                .add_filter("Modèle d'import", &["json"])
                .set_file_name(format!("{current_name}.json"))
                .save_file()
            {
                state.export_import_recipe(&path);
            }
        }
        if ui.button("Importer un modèle…").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Importer un modèle d'import")
                .add_filter("Modèle d'import", &["json"])
                .pick_file()
            {
                state.import_import_recipe(&path);
            }
        }
    });
    if modified {
        ui.weak("Ce modèle a été modifié sans être enregistré (étape Destination).");
    }
    if let Some(name) = confirm {
        ui.horizontal(|ui| {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("Supprimer le modèle « {name} » ?"),
            );
            if ui.button("Supprimer").clicked() {
                state.confirm_delete_import_recipe();
            }
            if ui.button("Garder").clicked() {
                state.cancel_delete_import_recipe();
            }
        });
    }
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Ajouter des fichiers…").clicked() {
            let mut dialog = rfd::FileDialog::new().set_title("Choisir des fichiers à importer");
            // Le dossier du dernier fichier importé.
            if let Some(dir) = state
                .import_state()
                .and_then(|import| import.recent.last_dir().map(std::path::Path::to_path_buf))
            {
                dialog = dialog.set_directory(dir);
            }
            if let Some(paths) = dialog.pick_files() {
                state.add_import_paths(paths);
            }
        }
        ui.weak("ou glissez-les dans l'application");
    });
    let recent: Vec<std::path::PathBuf> = state
        .import_state()
        .map(|import| import.recent.files().to_vec())
        .unwrap_or_default();
    if !recent.is_empty() {
        let mut picked = None;
        egui::ComboBox::from_id_salt("plotyx_import_recent")
            .selected_text("Fichiers récents…")
            .show_ui(ui, |ui| {
                for path in &recent {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                    if ui
                        .selectable_label(false, name)
                        .on_hover_text(path.display().to_string())
                        .clicked()
                    {
                        picked = Some(path.clone());
                    }
                }
            });
        if let Some(path) = picked {
            state.add_import_paths(vec![path]);
        }
    }
    let statuses = state.import_file_statuses();
    let preview_index = state
        .import_state()
        .map_or(0, |import| import.preview_index);
    if statuses.is_empty() {
        ui.weak("Aucun fichier choisi.");
    }
    let mut remove = None;
    let mut preview = None;
    for (i, (path, status)) in statuses.iter().enumerate() {
        ui.horizontal(|ui| {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            if ui.radio(i == preview_index, name).clicked() {
                preview = Some(i);
            }
            match status {
                Ok(rows) => {
                    ui.weak(format!("{rows} ligne(s)"));
                }
                Err(message) => {
                    ui.colored_label(egui::Color32::RED, message);
                }
            }
            if ui.small_button("×").on_hover_text("Retirer").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = preview {
        state.set_import_preview_index(i);
    }
    if let Some(i) = remove {
        state.remove_import_path(i);
    }
}

/// Étape 2 : séparateur et décimale.
fn import_step_format(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(import) = state.import_state_mut() else {
        return;
    };
    let recipe = &mut import.recipe;
    ui.horizontal(|ui| {
        ui.label("Séparateur :");
        egui::ComboBox::from_id_salt("plotyx_import_separator_combo")
            .selected_text(separator_label(&recipe.separator))
            .show_ui(ui, |ui| {
                for sep in [
                    Separator::Comma,
                    Separator::Semicolon,
                    Separator::Tab,
                    Separator::Whitespace,
                    Separator::Custom('|'),
                    Separator::FixedWidth(vec![10]),
                    Separator::Auto,
                ] {
                    let same =
                        std::mem::discriminant(&recipe.separator) == std::mem::discriminant(&sep);
                    if ui.selectable_label(same, separator_label(&sep)).clicked() && !same {
                        recipe.separator = sep;
                    }
                }
            });
        match &mut recipe.separator {
            Separator::Custom(c) => {
                let mut text = c.to_string();
                if ui
                    .add(egui::TextEdit::singleline(&mut text).desired_width(24.0))
                    .changed()
                {
                    if let Some(first) = text.chars().last() {
                        *c = first;
                    }
                }
            }
            Separator::FixedWidth(widths) => {
                let mut text = widths
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                if ui
                    .add(egui::TextEdit::singleline(&mut text).desired_width(120.0))
                    .on_hover_text("Largeur de chaque colonne, sauf la dernière (ex. 10,8,8)")
                    .changed()
                {
                    *widths = text
                        .split(',')
                        .filter_map(|part| part.trim().parse::<usize>().ok())
                        .collect();
                }
            }
            _ => {}
        }
    });
    ui.horizontal(|ui| {
        ui.label("Séparateur décimal :");
        egui::ComboBox::from_id_salt("plotyx_import_decimal_combo")
            .selected_text(decimal_label(recipe.decimal))
            .show_ui(ui, |ui| {
                for dec in [DecimalStyle::Auto, DecimalStyle::Dot, DecimalStyle::Comma] {
                    if ui
                        .selectable_label(recipe.decimal == dec, decimal_label(dec))
                        .clicked()
                    {
                        recipe.decimal = dec;
                    }
                }
            });
        ui.label("Milliers :");
        egui::ComboBox::from_id_salt("plotyx_import_thousands_combo")
            .selected_text(thousands_label(recipe.thousands))
            .show_ui(ui, |ui| {
                for candidate in [None, Some(' '), Some(','), Some('.'), Some('\'')] {
                    if ui
                        .selectable_label(recipe.thousands == candidate, thousands_label(candidate))
                        .clicked()
                    {
                        recipe.thousands = candidate;
                    }
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("Valeurs manquantes (séparées par des virgules) :");
        let mut text = recipe.missing_tokens.join(", ");
        if ui
            .add(egui::TextEdit::singleline(&mut text).desired_width(140.0))
            .on_hover_text("Jetons lus comme une cellule vide : --, n/a, *, NaN…")
            .changed()
        {
            recipe.missing_tokens = text
                .split(',')
                .map(|token| token.trim().to_string())
                .filter(|token| !token.is_empty())
                .collect();
        }
    });
    ui.horizontal(|ui| {
        ui.label("Préfixe des lignes de commentaire :");
        ui.add(egui::TextEdit::singleline(&mut recipe.comment_prefix).desired_width(60.0))
            .on_hover_text(
                "Les lignes de données qui commencent par ce préfixe sont ignorées (#, ;, //…)",
            );
    });
    ui.horizontal(|ui| {
        use plotyx_import::BlockSplit;
        ui.label("Plusieurs blocs dans le fichier :");
        let current = match &recipe.block_split {
            BlockSplit::None => "Non",
            BlockSplit::BeforeLine(_) => "À chaque ligne commençant par…",
            BlockSplit::BlankLines => "Séparés par des lignes vides",
        };
        egui::ComboBox::from_id_salt("plotyx_import_blocks_combo")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (candidate, label) in [
                    (BlockSplit::None, "Non"),
                    (
                        BlockSplit::BeforeLine("TITLE".into()),
                        "À chaque ligne commençant par…",
                    ),
                    (BlockSplit::BlankLines, "Séparés par des lignes vides"),
                ] {
                    let same = std::mem::discriminant(&recipe.block_split)
                        == std::mem::discriminant(&candidate);
                    if ui.selectable_label(same, label).clicked() && !same {
                        recipe.block_split = candidate;
                    }
                }
            });
        if let BlockSplit::BeforeLine(marker) = &mut recipe.block_split {
            ui.add(egui::TextEdit::singleline(marker).desired_width(80.0));
        }
    });
    ui.horizontal(|ui| {
        ui.label("Encodage :");
        egui::ComboBox::from_id_salt("plotyx_import_encoding_combo")
            .selected_text(encoding_label(recipe.encoding))
            .show_ui(ui, |ui| {
                use plotyx_import::Encoding;
                for candidate in [
                    Encoding::Auto,
                    Encoding::Utf8,
                    Encoding::Windows1252,
                    Encoding::Latin1,
                    Encoding::Utf16Le,
                    Encoding::Utf16Be,
                ] {
                    if ui
                        .selectable_label(recipe.encoding == candidate, encoding_label(candidate))
                        .clicked()
                    {
                        recipe.encoding = candidate;
                    }
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("Caractères à retirer en tête de chaque ligne de données :");
        let mut value = recipe.skip_chars as i64;
        if ui
            .add(egui::DragValue::new(&mut value).range(0..=i64::MAX))
            .changed()
        {
            recipe.skip_chars = value.max(0) as usize;
        }
    });
    ui.horizontal(|ui| {
        ui.label("Lignes de données à ignorer au début :");
        let mut value = recipe.skip_data_rows as i64;
        if ui
            .add(egui::DragValue::new(&mut value).range(0..=i64::MAX))
            .changed()
        {
            recipe.skip_data_rows = value.max(0) as usize;
        }
    });
}

/// Étape 3 : en-tête (un seul réglage) et fin des données.
fn import_step_header(
    ui: &mut egui::Ui,
    state: &mut AppState,
    requests: &mut ImportWizardRequests,
) {
    let has_files = state
        .import_state()
        .is_some_and(|import| !import.paths.is_empty());
    let Some(import) = state.import_state_mut() else {
        return;
    };
    let recipe = &mut import.recipe;
    // Comme `parse` : le découpage structuré ne compte que s'il a des lignes
    // (l'ancien drapeau `auto_detect` seul ne suffit pas).
    let structured =
        recipe.header_layout.main_header_lines > 0 || recipe.header_layout.subheader_lines > 0;
    let is_marker = matches!(recipe.header, HeaderSkip::UntilMarker(_));
    ui.horizontal(|ui| {
        ui.label("En-tête :");
        if ui.radio(!structured && !is_marker, "Aucun").clicked() {
            recipe.header = HeaderSkip::None;
            recipe.header_layout = plotyx_import::HeaderLayout::default();
        }
        if ui.radio(structured, "Nombre de lignes").clicked() && !structured {
            recipe.header = HeaderSkip::None;
            recipe.header_layout.main_header_lines = 1;
        }
        if ui.radio(is_marker, "Jusqu'à un marqueur").clicked() && !is_marker {
            recipe.header_layout = plotyx_import::HeaderLayout::default();
            recipe.header = HeaderSkip::UntilMarker(String::new());
        }
    });
    if let HeaderSkip::UntilMarker(marker) = &mut recipe.header {
        ui.horizontal(|ui| {
            ui.label("Marqueur (dernière ligne de l'en-tête) :");
            ui.text_edit_singleline(marker);
        });
    }
    if structured {
        ui.horizontal(|ui| {
            ui.label("Lignes d'en-tête :");
            let mut value = recipe.header_layout.main_header_lines as i64;
            if ui
                .add(egui::DragValue::new(&mut value).range(0..=i64::MAX))
                .changed()
            {
                recipe.header_layout.main_header_lines = value.max(0) as usize;
            }
            ui.label("dont sous-en-tête :");
            let mut value = recipe.header_layout.subheader_lines as i64;
            if ui
                .add(egui::DragValue::new(&mut value).range(0..=i64::MAX))
                .changed()
            {
                recipe.header_layout.subheader_lines = value.max(0) as usize;
            }
        });
    }
    if ui
        .add_enabled(has_files, egui::Button::new("Détecter depuis le fichier"))
        .on_hover_text("Recalcule le nombre de lignes d'en-tête et de sous-en-tête")
        .clicked()
    {
        requests.auto_detect = true;
    }
    if recipe.header_layout.subheader_lines > 0 {
        ui.checkbox(
            &mut recipe.subheader_roles.same_line_name_unit,
            "Extraire noms longs et unités de la même ligne",
        );
        subheader_role_row(
            ui,
            "plotyx_import_role_long_name",
            "Noms longs, lignes de sous-en-tête :",
            &mut recipe.subheader_roles.long_name,
        );
        if !recipe.subheader_roles.same_line_name_unit {
            subheader_role_row(
                ui,
                "plotyx_import_role_unit",
                "Unités, lignes de sous-en-tête :",
                &mut recipe.subheader_roles.unit,
            );
        }
        subheader_role_row(
            ui,
            "plotyx_import_role_comment",
            "Commentaires, lignes de sous-en-tête :",
            &mut recipe.subheader_roles.comment,
        );
    }
    ui.checkbox(
        &mut recipe.has_column_header_row,
        "La première ligne de données donne les noms de colonnes",
    );
    ui.separator();
    data_end_row(ui, &mut recipe.data_end);
    ui.horizontal(|ui| {
        ui.label("Lignes à ignorer tout en bas du fichier :");
        let mut value = recipe.footer_skip_lines as i64;
        if ui
            .add(egui::DragValue::new(&mut value).range(0..=i64::MAX))
            .changed()
        {
            recipe.footer_skip_lines = value.max(0) as usize;
        }
    });
    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Nom des séries :");
        let mut is_filename = matches!(recipe.name_source, NameSource::Filename);
        let mut is_header_key = matches!(recipe.name_source, NameSource::HeaderKey(_));
        if ui
            .radio_value(&mut is_filename, true, "Nom de fichier")
            .clicked()
        {
            recipe.name_source = NameSource::Filename;
        }
        if ui
            .radio_value(&mut is_header_key, true, "Clé d'en-tête")
            .clicked()
        {
            recipe.name_source = NameSource::HeaderKey(String::new());
        }
    });
    if let NameSource::HeaderKey(key) = &mut recipe.name_source {
        ui.horizontal(|ui| {
            ui.label("Clé :");
            ui.text_edit_singleline(key);
        });
    }
}

/// Libellé de la nature choisie pour une colonne.
fn column_kind_label(kind: plotyx_import::ColumnKind) -> &'static str {
    match kind {
        plotyx_import::ColumnKind::Auto => "Nombres",
        plotyx_import::ColumnKind::Text => "Texte",
        plotyx_import::ColumnKind::DateTime => "Date-heure",
    }
}

/// Libellé d'un rôle de colonne choisi à l'étape « Colonnes ».
fn column_role_label(role: plotyx_import::ColumnRoleChoice) -> &'static str {
    use plotyx_import::ColumnRoleChoice;
    match role {
        ColumnRoleChoice::Auto => "Auto",
        ColumnRoleChoice::X => "X",
        ColumnRoleChoice::Y => "Y",
        ColumnRoleChoice::XError => "Erreur X",
        ColumnRoleChoice::YError => "Erreur Y",
        ColumnRoleChoice::Ignore => "Ignorer",
    }
}

/// Étape 4 : le résultat, colonne par colonne, avec le rôle de chacune.
fn import_step_columns(ui: &mut egui::Ui, state: &mut AppState) {
    use plotyx_import::ColumnRoleChoice;
    let columns = match state.import_column_previews() {
        None => {
            ui.weak("Choisissez un fichier à l'étape 1.");
            return;
        }
        Some(Err(message)) => {
            ui.colored_label(egui::Color32::RED, message);
            return;
        }
        Some(Ok(columns)) => columns,
    };
    let Some(import) = state.import_state_mut() else {
        return;
    };
    if import.recipe.column_roles.len() < columns.len() {
        import
            .recipe
            .column_roles
            .resize(columns.len(), ColumnRoleChoice::Auto);
    }
    if import.recipe.column_kinds.len() < columns.len() {
        import
            .recipe
            .column_kinds
            .resize(columns.len(), plotyx_import::ColumnKind::Auto);
    }
    egui::ScrollArea::both()
        .id_salt("plotyx_import_columns_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("plotyx_import_columns_grid")
                .striped(true)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    for title in [
                        "#", "Nom", "Nom long", "Unité", "Lu comme", "Type", "Rôle", "Valeurs",
                    ] {
                        ui.strong(title);
                    }
                    ui.end_row();
                    for (index, column) in columns.iter().enumerate() {
                        let ignored = import.recipe.column_role(index) == ColumnRoleChoice::Ignore;
                        ui.label((index + 1).to_string());
                        ui.label(&column.name);
                        ui.label(&column.long_name);
                        ui.label(&column.unit);
                        let kind = import.recipe.column_kind(index);
                        egui::ComboBox::from_id_salt(("plotyx_import_column_kind", index))
                            .selected_text(column_kind_label(kind))
                            .show_ui(ui, |ui| {
                                for choice in [
                                    plotyx_import::ColumnKind::Auto,
                                    plotyx_import::ColumnKind::Text,
                                    plotyx_import::ColumnKind::DateTime,
                                ] {
                                    if ui
                                        .selectable_label(kind == choice, column_kind_label(choice))
                                        .clicked()
                                    {
                                        import.recipe.column_kinds[index] = choice;
                                    }
                                }
                            });
                        ui.label(if column.mixed { "Mixte" } else { "Numérique" });
                        let effective = import.recipe.column_role(index);
                        egui::ComboBox::from_id_salt(("plotyx_import_column_role", index))
                            .selected_text(column_role_label(effective))
                            .show_ui(ui, |ui| {
                                for choice in [
                                    ColumnRoleChoice::X,
                                    ColumnRoleChoice::Y,
                                    ColumnRoleChoice::XError,
                                    ColumnRoleChoice::YError,
                                    ColumnRoleChoice::Ignore,
                                ] {
                                    if ui
                                        .selectable_label(
                                            effective == choice,
                                            column_role_label(choice),
                                        )
                                        .clicked()
                                    {
                                        import.recipe.column_roles[index] = choice;
                                    }
                                }
                            });
                        let samples = column.samples.join("  ");
                        if ignored {
                            ui.weak(samples);
                        } else {
                            ui.label(samples);
                        }
                        ui.end_row();
                    }
                });
        });
}

/// Étape 5 : les métadonnées à garder.
fn import_step_metadata(ui: &mut egui::Ui, state: &mut AppState) {
    let available = state.import_available_metadata();
    let Some(import) = state.import_state_mut() else {
        return;
    };
    let recipe = &mut import.recipe;
    ui.checkbox(
        &mut recipe.keep_header_metadata,
        "Conserver l'en-tête du fichier",
    );
    ui.checkbox(
        &mut recipe.keep_footer_metadata,
        "Conserver aussi le pied de page",
    );
    ui.weak("Décochez une clé (ou une section entière) pour ne pas la garder.");
    ui.separator();
    let is_excluded = |recipe: &plotyx_import::ImportRecipe, key: &str| {
        recipe
            .excluded_metadata
            .iter()
            .any(|name| name.trim().eq_ignore_ascii_case(key.trim()))
    };
    egui::ScrollArea::vertical()
        .id_salt("plotyx_import_metadata_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for line in &available {
                if line.key.is_empty() {
                    ui.weak(&line.value);
                    continue;
                }
                let mut kept = !is_excluded(recipe, &line.key);
                let text = if line.is_section() {
                    egui::RichText::new(&line.key).strong()
                } else {
                    egui::RichText::new(format!("{} = {}", line.key, line.value))
                };
                if ui.checkbox(&mut kept, text).changed() {
                    if kept {
                        recipe
                            .excluded_metadata
                            .retain(|name| !name.trim().eq_ignore_ascii_case(line.key.trim()));
                    } else {
                        recipe.excluded_metadata.push(line.key.clone());
                    }
                }
            }
            if available.is_empty() {
                ui.weak("Aucune métadonnée dans ce fichier avec ces réglages.");
            }
        });
}

/// Étape 6 : destination, enregistrement du modèle.
fn import_step_destination(
    ui: &mut egui::Ui,
    state: &mut AppState,
    requests: &mut ImportWizardRequests,
) {
    let Some(import) = state.import_state_mut() else {
        return;
    };
    ui.label("Destination :");
    if let Some(book) = import.active_book {
        ui.radio_value(
            &mut import.destination,
            ImportDestination::ActiveBook(book),
            "Classeur actif (une feuille par fichier)",
        );
    } else {
        ui.add_enabled(
            false,
            egui::RadioButton::new(false, "Classeur actif (aucun classeur actif)"),
        );
    }
    ui.radio_value(
        &mut import.destination,
        ImportDestination::NewBookPerFile,
        "Un nouveau classeur par fichier",
    );
    ui.radio_value(
        &mut import.destination,
        ImportDestination::NewBookForAll,
        "Un nouveau classeur pour tous les fichiers",
    );
    if matches!(import.destination, ImportDestination::ActiveBook(_)) {
        ui.separator();
        ui.label("Dans le classeur actif :");
        use plotyx_import::ImportMode;
        for (mode, label) in [
            (ImportMode::NewSheet, "Nouvelle feuille"),
            (ImportMode::ReplaceSheet, "Remplacer la feuille active"),
            (
                ImportMode::AddColumns,
                "Ajouter des colonnes à la feuille active",
            ),
            (
                ImportMode::AddRows,
                "Ajouter des lignes à la suite de la feuille active",
            ),
        ] {
            ui.radio_value(&mut import.recipe.import_mode, mode, label);
        }
    }
    if import.paths.len() > 1 {
        ui.separator();
        ui.label("Plusieurs fichiers :");
        use plotyx_import::FilesLayout;
        for (layout, label) in [
            (FilesLayout::SheetPerFile, "Une feuille par fichier"),
            (
                FilesLayout::SideBySide,
                "Une seule feuille, côte à côte (X Y X Y…)",
            ),
            (
                FilesLayout::SharedX,
                "Une seule feuille, avec le X du premier fichier partagé",
            ),
        ] {
            ui.radio_value(&mut import.recipe.files_layout, layout, label);
        }
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Nom du modèle :");
        ui.text_edit_singleline(&mut import.save_as_name);
        if ui.button("Enregistrer le modèle").clicked() {
            requests.save_recipe = true;
        }
    });
    ui.weak("Le modèle enregistré est proposé à la prochaine importation.");
    ui.separator();
    ui.label("Choix automatique de ce modèle (tous les critères remplis doivent correspondre) :");
    let auto = &mut import.recipe.auto_match;
    ui.horizontal(|ui| {
        ui.label("Extensions :");
        let mut text = auto.extensions.join(", ");
        if ui
            .add(egui::TextEdit::singleline(&mut text).desired_width(120.0))
            .on_hover_text("csv, txt, asc…")
            .changed()
        {
            auto.extensions = text
                .split(',')
                .map(|e| e.trim().trim_start_matches('.').to_string())
                .filter(|e| !e.is_empty())
                .collect();
        }
    });
    ui.horizontal(|ui| {
        ui.label("Le nom contient :");
        ui.add(egui::TextEdit::singleline(&mut auto.name_contains).desired_width(120.0));
    });
    ui.horizontal(|ui| {
        ui.label("Le début du fichier contient :");
        ui.add(egui::TextEdit::singleline(&mut auto.content_contains).desired_width(160.0))
            .on_hover_text("Une signature, comme ORIGIN;JASCO");
    });
}

/// L'assistant d'import (« Importer… », menu Fichier) en six étapes façon
/// assistant ASCII d'Origin : fichiers et modèle, format, en-tête et fin des
/// données, colonnes, métadonnées, destination. L'aperçu du fichier reste
/// visible à droite à toutes les étapes. « Terminer » importe dès que des
/// fichiers sont choisis, avec le modèle tel qu'il est — import en deux clics
/// pour un format connu. **Dialogue modal de la brique commune**
/// ([`modal_dialog_with`], comme les propriétés du graphe), peint depuis
/// [`PlotyxApp::ui`] tant que [`AppState::import_state`] est `Some` ; la croix
/// et Échap valent « Annuler », Entrée « Terminer ».
fn import_window(ctx: &egui::Context, state: &mut AppState, actions: &mut Vec<Action>) {
    let Some(step) = state.import_state().map(|import| import.step) else {
        return;
    };
    let has_files = state
        .import_state()
        .is_some_and(|import| !import.paths.is_empty());
    // Les deux fermetures de la brique (contenu, boutons du pied) écrivent
    // dans les mêmes demandes, l'une après l'autre : `RefCell`.
    let requests = std::cell::RefCell::new(ImportWizardRequests::default());
    let outcome = modal_dialog_with(
        ctx,
        egui::Id::new("plotyx_import_wizard"),
        "Importer des fichiers",
        egui::vec2(1000.0, 620.0),
        DialogFooter {
            accept_label: "Terminer",
            accept_hint: "Importer maintenant avec les réglages actuels (Entrée)",
            accept_enabled: has_files,
        },
        |footer| {
            // Ajoutés de droite à gauche : « Suivant » puis « Précédent ».
            if footer
                .add_enabled(
                    step != ImportStep::Destination,
                    egui::Button::new("Suivant >").min_size(egui::vec2(84.0, 24.0)),
                )
                .clicked()
            {
                requests.borrow_mut().forward = true;
            }
            if footer
                .add_enabled(
                    step != ImportStep::Files,
                    egui::Button::new("< Précédent").min_size(egui::vec2(84.0, 24.0)),
                )
                .clicked()
            {
                requests.borrow_mut().back = true;
            }
        },
        |ui| {
            let mut requests = requests.borrow_mut();
            ui.horizontal_wrapped(|ui| {
                for (i, candidate) in ImportStep::ALL.iter().enumerate() {
                    if i > 0 {
                        ui.weak("›");
                    }
                    let text = format!("{} {}", i + 1, candidate.title());
                    if *candidate == step {
                        ui.strong(text);
                    } else {
                        ui.weak(text);
                    }
                }
            });
            ui.separator();
            let body_height = ui.available_height().max(240.0);
            ui.columns(2, |columns| {
                columns[0].set_min_height(body_height);
                columns[0].set_max_height(body_height);
                egui::ScrollArea::vertical()
                    .id_salt("plotyx_import_step_scroll")
                    .max_height(body_height)
                    .auto_shrink([false, false])
                    .show(&mut columns[0], |ui| match step {
                        ImportStep::Files => import_step_files(ui, state),
                        ImportStep::Format => import_step_format(ui, state),
                        ImportStep::Header => import_step_header(ui, state, &mut requests),
                        ImportStep::Columns => import_step_columns(ui, state),
                        ImportStep::Metadata => import_step_metadata(ui, state),
                        ImportStep::Destination => {
                            import_step_destination(ui, state, &mut requests)
                        }
                    });
                columns[1].set_min_height(body_height);
                columns[1].set_max_height(body_height);
                import_preview_pane(&mut columns[1], state, &mut requests.line_action);
            });
        },
    );
    let mut requests = requests.into_inner();
    match outcome {
        DialogOutcome::Open => {}
        DialogOutcome::Accept => requests.import = true,
        DialogOutcome::Cancel => requests.cancel = true,
    }

    // Tout ce qui demande l'état entier est appliqué ici, une fois la fenêtre
    // peinte.
    if requests.save_recipe {
        state.save_import_recipe();
    }
    if requests.auto_detect {
        state.auto_detect_import_header();
    }
    if let Some((action, number)) = requests.line_action {
        state.import_line_action(action, number);
    }
    if requests.back {
        state.import_step_back();
    }
    if requests.forward {
        state.import_step_forward();
    }
    if requests.import {
        if let Some(import) = state.import_state() {
            actions.push(Action::StartImport {
                destination: import.destination,
                recipe: import.recipe.clone(),
                paths: import.paths.clone(),
            });
        }
        state.cancel_import();
    } else if requests.cancel {
        state.cancel_import();
    }
}

/// Touches que la grille réclame à egui tant qu'elle a le focus clavier (#38) :
/// Tab et les flèches. egui les traite au **début** de l'image
/// (`Focus::begin_pass`) pour déplacer son propre focus de widget en widget,
/// sauf si le widget focalisé les réclame par ce filtre — les lire ensuite par
/// `key_pressed`, comme le faisait la grille, ne retire rien : la sélection
/// avançait (ou pas), et le focus partait quand même dans le reste de
/// l'interface. Échap reste à egui : il rend le focus.
const GRID_KEYS: egui::EventFilter = egui::EventFilter {
    tab: true,
    horizontal_arrows: true,
    vertical_arrows: true,
    escape: false,
};

/// Clé mémoire egui du dernier ancrage de focus qu'une grille a réclamé
/// ([`claim_grid_focus`]) — de quoi reconnaître, depuis le reste de
/// l'application, que le focus clavier est à la grille et non à un champ texte.
fn grid_focus_owner_key() -> egui::Id {
    egui::Id::new("plotyx_grid_focus_owner")
}

/// Identifiant du widget d'ancrage du focus de la grille de `sheet` — un par
/// feuille et par `Ui`, pour que le classeur flottant et sa copie détachée
/// ne se disputent pas le même identifiant.
fn grid_focus_id(ui: &egui::Ui, sheet: NodeId) -> egui::Id {
    ui.id().with(("grid_focus", sheet.raw()))
}

/// La grille prend le focus clavier : sa sélection reçoit désormais Tab et les
/// flèches (voir [`GRID_KEYS`]).
fn claim_grid_focus(ctx: &egui::Context, anchor: egui::Id) {
    ctx.memory_mut(|memory| memory.request_focus(anchor));
    ctx.data_mut(|data| data.insert_temp(grid_focus_owner_key(), anchor));
}

/// Vrai si un widget **autre qu'une grille** a le focus clavier — typiquement
/// un champ texte, qui garde alors Entrée, Suppr, Tab… Remplace le test
/// `focused().is_some()` partout où la grille, elle, peut légitimement tenir le
/// focus (#38) : sans lui, une grille focalisée ferait passer ses voisins pour
/// des champs en cours de saisie.
fn keyboard_focus_outside_grid(ctx: &egui::Context) -> bool {
    let Some(focused) = ctx.memory(|memory| memory.focused()) else {
        return false;
    };
    let owner: Option<egui::Id> = ctx.data(|data| data.get_temp(grid_focus_owner_key()));
    owner != Some(focused)
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
/// `TextEdit` en cours de saisie — la fenêtre d'import est un dialogue modal
/// flottant peint par-dessus cette grille, toujours présente dessous tant
/// qu'il est ouvert (voir la garde plus bas, dans le corps de la fonction).
fn grid_contents(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    pending_vertical_offset: Option<f32>,
) -> Option<egui::scroll_area::ScrollAreaOutput<()>> {
    let columns = state.visible_columns(sheet);
    let row_count = state.row_count(sheet);
    if columns.is_empty() {
        let empty = ui.centered_and_justified(|ui| {
            ui.weak("Aucune colonne dans cette feuille.");
        });
        empty.response.context_menu(|ui| {
            if ui.button("Ajouter une colonne").clicked() {
                actions.push(Action::AddColumn { sheet });
                ui.close();
            }
        });
        return None;
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
    // Ancrage du focus clavier de la grille (#38) : un widget focalisable
    // **seulement tant qu'il a le focus** (sinon un Tab tombant sur un
    // clavier sans focus l'y ferait entrer de lui-même). Il porte le filtre
    // d'événements [`GRID_KEYS`] — voir la doc de cette constante. La grille
    // le réclame par [`claim_grid_focus`] après un appui dans son rectangle
    // ou la fin d'une édition (plus bas) ; egui le lui retire seul à un appui
    // ailleurs ou si la grille cesse d'être peinte. Le filtre n'est pris en
    // compte qu'à l'image qui suit la prise de focus (`set_focus_lock_filter`
    // exige `had_focus_last_frame`) : un Tab dans l'image même du clic n'est
    // pas une frappe réaliste.
    let focus_anchor = grid_focus_id(ui, sheet);
    let anchor_focused = ui.memory(|memory| memory.has_focus(focus_anchor));
    ui.interact(
        grid_rect,
        focus_anchor,
        if anchor_focused {
            egui::Sense::focusable_noninteractive()
        } else {
            egui::Sense::hover()
        },
    );
    if anchor_focused {
        ui.memory_mut(|memory| memory.set_focus_lock_filter(focus_anchor, GRID_KEYS));
    }
    let was_editing = state.editing().is_some() || state.header_editing().is_some();
    let mut last_column_right = grid_rect.left();
    let header_height = stacked_header_height(metadata_row_visible(ui.ctx()));
    let mut table = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(TableColumn::initial(ROW_NUMBER_COLUMN_WIDTH).at_least(MIN_ROW_NUMBER_COLUMN_WIDTH))
        .columns(
            TableColumn::initial(DEFAULT_COLUMN_WIDTH).at_least(MIN_COLUMN_WIDTH),
            columns.len(),
        )
        .min_scrolled_height(0.0)
        // La table occupe l'espace laissé par le panneau d'onglets, même
        // quand sa feuille contient peu de lignes : la grille garde ainsi
        // sa hauteur et son défilement vertical.
        .auto_shrink([false, false])
        // Sa barre verticale n'est pas dessinée ici : dans la zone de
        // défilement horizontale, elle suivrait le bord droit de la table et
        // sortirait de la vue sur une grille large (B10). Le défilement
        // (molette, glisser) reste actif ; la barre visible est portée par
        // [`show_grid_vertical_bar`], collée au bord droit de la fenêtre.
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    if let Some(offset) = pending_vertical_offset {
        table = table.vertical_scroll_offset(offset);
    }
    let table_output = table
        .header(header_height, |mut header| {
            header.col(|ui| {
                row_number_header_widget(ui, state, actions, sheet);
            });
            for (index, (id, _, column_type)) in columns.iter().enumerate() {
                header.col(|ui| {
                    last_column_right = ui.max_rect().right();
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

    // La portion libre après la dernière colonne est une cible distincte
    // des cellules et de leurs menus. La borne vient de l'en-tête réellement
    // disposé (y compris les largeurs redimensionnées), pas des largeurs par défaut.
    if last_column_right < grid_rect.right() {
        let rect = egui::Rect::from_min_max(
            egui::pos2(last_column_right, grid_rect.top()),
            grid_rect.right_bottom(),
        );
        let response = ui.interact(
            rect,
            ui.id().with(("add_column_blank", sheet)),
            egui::Sense::click(),
        );
        response.context_menu(|ui| {
            if ui.button("Ajouter une colonne").clicked() {
                actions.push(Action::AddColumn { sheet });
                ui.close();
            }
        });
    }

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

    // Prise du focus clavier (#38), **avant** le bloc de raccourcis, pour que
    // la touche qui suit un clic soit déjà celle de la grille. Deux cas : un
    // appui dans le rectangle de la grille (clic, glisser, menu contextuel),
    // sauf s'il ouvre ou prolonge une édition, dont le champ texte réclame le
    // focus lui-même ; et la fin d'une édition qui a rendu le focus à
    // personne (Entrée, Tab — voir [`cell_widget`] —, Échap) : sans lui, le
    // Tab suivant repartirait dans le reste de l'interface. Un focus tenu par
    // un autre widget (clic dans un autre champ) n'est jamais repris.
    let editing_now = state.editing().is_some() || state.header_editing().is_some();
    // Appui **ou** relâchement : egui rend le focus de l'ancrage au clic
    // (`SurrenderFocusOn::Clicks`, son réglage par défaut), donc au
    // relâchement, parce que l'ancrage n'est pas sous le curseur ; l'appui
    // seul ne suffit pas pour un clic simple.
    let pressed_in_grid = ui
        .input(|input| input.pointer.any_pressed() || input.pointer.any_released())
        && ui.rect_contains_pointer(grid_rect);
    let edit_just_ended =
        was_editing && !editing_now && ui.ctx().memory(|memory| memory.focused()).is_none();
    if !editing_now && (pressed_in_grid || edit_just_ended) {
        claim_grid_focus(ui.ctx(), focus_anchor);
    }
    let focus_free = ui
        .ctx()
        .memory(|memory| memory.focused())
        .is_none_or(|focused| focused == focus_anchor);

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
    //
    // #17 : même vol, cette fois dans le dialogue des propriétés du graphe —
    // la liste de conditions par fenêtre ne pouvait que rater la suivante.
    // `focus_free` (aucun widget n'a le focus clavier, ou c'est l'ancrage de
    // **cette** grille — voir `GRID_KEYS`, #38 ; `egui_wants_keyboard_input`
    // ne le distinguait pas d'un champ texte) couvre d'un coup ce dialogue,
    // l'import et tout futur dialogue ; `import_state` reste pour le cas d'un
    // import ouvert sans champ focalisé.
    if state.editing().is_none()
        && state.header_editing().is_none()
        && state.import_state().is_none()
        && focus_free
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
    Some(table_output)
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
    actions: &mut Vec<Action>,
    graph_views: &mut HashMap<NodeId, GraphView>,
    graph_id: NodeId,
) {
    let view = graph_views.entry(graph_id).or_default();
    graph_view_contents(ui, view, state.snapshot(), graph_id, actions);
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
    // la figure, jamais dans `plotyx-render` : l'export SVG/PNG/PDF d'un
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
            if ui.small_button("réinitialiser le zoom").clicked() {
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
/// (`plotyx_render::color::over_white`).
fn display_backdrop(background: &str) -> Option<egui::Color32> {
    crate::color_picker::parse_hex(background)
        .is_some_and(|[_, _, _, alpha]| alpha < 255)
        .then_some(egui::Color32::WHITE)
}

/// Menu contextuel de la figure : un clic droit (sans glisser — le glisser
/// secondaire reste le zoom élastique de `plotyx-render-egui`) sur un
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
    if response.secondary_clicked() {
        let pos = response.interact_pointer_pos();
        let target = pos.map_or(PlotTarget::Background, |pos| {
            view.annotation_at(pos).map_or_else(
                || plot_target(view.component_at(pos)),
                PlotTarget::Annotation,
            )
        });
        let point = pos.and_then(|pos| view.data_at(pos));
        ui.memory_mut(|memory| {
            memory.data.insert_temp(target_id, target);
            memory.data.insert_temp(point_id, point);
        });
    }
    response.context_menu(|ui| {
        let target: PlotTarget = ui
            .memory(|memory| memory.data.get_temp(target_id))
            .unwrap_or(PlotTarget::Background);
        let point: Option<(usize, usize, f64, f64)> =
            ui.memory(|memory| memory.data.get_temp(point_id)).flatten();
        ui.weak(target.label());
        ui.separator();
        // Textes libres (issue #35) : les mêmes gestes que la barre de menus
        // et le clavier (double-clic, Suppr).
        if let PlotTarget::Annotation(annotation) = target {
            if ui.button("Modifier le texte").clicked() {
                open_annotation_editor(ui.ctx(), view, tree, graph_id, annotation);
                ui.close();
            }
            if ui
                .add(egui::Button::new("Supprimer").shortcut_text("Suppr"))
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
        } else if ui
            .add_enabled(point.is_some(), egui::Button::new("Ajouter un texte"))
            .on_disabled_hover_text("Cliquer dans une zone de tracé")
            .clicked()
        {
            if let Some(point) = point {
                add_annotation_at(ui.ctx(), view, tree, graph_id, point);
            }
            ui.close();
        }
        if ui.button("Propriétés…").clicked() {
            actions.push(target.open_settings(graph_id));
            ui.close();
        }
        if ui.button("Réinitialiser le zoom").clicked() {
            view.reset_view();
            ui.close();
        }
        if ui
            .add(
                egui::Button::new("Exporter le graphe…")
                    .shortcut_text(ui.ctx().format_shortcut(&SHORTCUT_EXPORT_GRAPH)),
            )
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
    target: plotyx_render_egui::AnnotationRef,
    draft: plotyx_render_egui::AnnotationSpec,
    /// Le texte tel qu'il est dans le modèle ; `None` pour un texte nouveau.
    original: Option<plotyx_render_egui::AnnotationSpec>,
    /// Donner le focus au champ à la prochaine image (ouverture).
    focus: bool,
    /// Le dernier centre et la dernière taille (écran) du texte droit rendu :
    /// un texte vidé n'est plus tracé, mais le champ doit rester où il est.
    anchor: Option<(egui::Pos2, egui::Vec2)>,
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
    target: plotyx_render_egui::AnnotationRef,
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
        .and_then(|spec| plotyx_render_egui::AnnotationRef::next_in(&spec, row, column))
    else {
        return;
    };
    let draft = plotyx_render_egui::AnnotationSpec::new("Texte", x, y);
    start_annotation_editor(ctx, view, target, draft, None);
}

fn start_annotation_editor(
    ctx: &egui::Context,
    view: &mut GraphView,
    target: plotyx_render_egui::AnnotationRef,
    draft: plotyx_render_egui::AnnotationSpec,
    original: Option<plotyx_render_egui::AnnotationSpec>,
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
fn saved_colors_id() -> egui::Id {
    egui::Id::new("plotyx_saved_colors_for_graphs")
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
fn annotation_interactions(
    ui: &egui::Ui,
    view: &mut GraphView,
    tree: &Tree,
    graph: NodeId,
    actions: &mut Vec<Action>,
) {
    for event in view.take_events() {
        match event {
            plotyx_render_egui::GraphEvent::EditAnnotation(target) => {
                open_annotation_editor(ui.ctx(), view, tree, graph, target);
            }
            plotyx_render_egui::GraphEvent::AnnotationMoved { target, annotation } => {
                actions.push(Action::SetAnnotation {
                    graph,
                    target,
                    annotation: Box::new(annotation),
                });
            }
        }
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
    let field = plotyx_render_egui::annotation_field(
        center,
        size,
        editor.draft.effective_size(),
        view.display_scale().unwrap_or(1.0),
    );

    let (project, user): (plotyx_plot::SavedColors, plotyx_plot::SavedColors) = ctx
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
                    crate::color_picker::color_button(
                        ui,
                        ("annotation_color", id),
                        &mut editor.draft.color,
                        ColorUsage::Text,
                        Some("Automatique (contraste avec le fond)"),
                        &mut env,
                    );
                    ui.label("Taille");
                    let (low, high) = plotyx_render_egui::AnnotationSpec::SIZE_RANGE;
                    ui.add(
                        egui::DragValue::new(&mut editor.draft.size)
                            .range(low..=high)
                            .speed(0.2),
                    );
                    ui.label("Rotation");
                    ui.add(
                        egui::DragValue::new(&mut editor.draft.rotation)
                            .range(-180.0..=180.0)
                            .speed(1.0)
                            .suffix("°"),
                    );
                    if ui.button("OK").on_hover_text("Valider (Entrée)").clicked() {
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
                    .hint_text("Texte, $formule$"),
            );
            if editor.focus {
                response.request_focus();
                editor.focus = false;
            }
            if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                accept = true;
            }
        });

    for request in env.requests {
        actions.push(match request.edit {
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
        });
    }

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
        .set_title("Exporter le graphe")
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
    }
}

/// Fenêtre des propriétés d'un graphe : un dialogue flottant (même famille
/// que [`import_window`]), ouvert par le clic droit sur un composant du
/// graphe ou par le menu « Graphique », et organisé en onglets thématiques
/// ([`PlotSettingsTab`]). Elle remplace l'ancien panneau fixe à droite de
/// l'espace de travail, qui prenait de la place en permanence dès qu'un
/// graphe était actif.
///
/// Édite le [`plotyx_plot::PlotSpec`] au travers d'un brouillon transitoire
/// ([`crate::state::PlotEdit`]), jamais directement.
///
/// **Stratégie anti-inflation d'historique** : chaque widget met à jour le
/// brouillon en direct (retour visuel immédiat), et `commit` — un simple
/// booléen local, mis à `true` par n'importe quel widget qui vient de finir
/// une interaction — ne déclenche [`AppState::commit_plot_edit`] qu'une
/// seule fois, à la fin de la fonction. Trois familles de widgets, trois
/// façons de finir une interaction :
/// - **texte** (titres, libellés) : `lost_focus()`,
///   comme `note_contents`/`cell_widget` — jamais à chaque frappe ;
/// - **glissé numérique** (épaisseur de trait, taille de marqueur, bornes
///   d'axe) : `drag_stopped() || lost_focus()` ;
/// - **case à cocher / menu déroulant** : un seul événement discret par
///   clic, validé tout de suite.
///
/// Fermer la fenêtre valide ce qui diverge encore du modèle
/// ([`AppState::close_plot_settings`]).
///
/// **Un seul panneau, pas de multi-panneaux** : seul `PlotSpec::panels`[0]
/// est éditable ici. Éditer le premier panneau revient, pour l'instant, à
/// éditer *le* graphe.
fn plot_settings_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(settings) = state.plot_settings() else {
        return;
    };
    // Cloné avant d'emprunter `state` en écriture pour le brouillon : un
    // `Arc<Tree>` cloné est un compteur de références, pas une copie.
    let tree = state.tree().clone();
    let Some(node) = tree.node(settings.graph) else {
        state.cancel_plot_settings();
        return;
    };
    let title = format!("Propriétés — {}", node.name());
    state.begin_plot_edit(settings.graph);
    let outcome = modal_dialog(
        ctx,
        egui::Id::new("plotyx_plot_settings"),
        &title,
        egui::vec2(PLOT_SETTINGS_WIDTH, PLOT_SETTINGS_HEIGHT),
        |ui| plot_settings_contents(ui, state, &tree, settings),
    );
    match outcome {
        DialogOutcome::Open => {}
        DialogOutcome::Accept => state.close_plot_settings(),
        DialogOutcome::Cancel => state.cancel_plot_settings(),
    }
}

/// Fenêtre de consultation des métadonnées de fichier d'une colonne (issue
/// #5) : les lignes clé / valeur, sections comprises, en lecture seule. Même
/// brique modale que les propriétés du graphe ; « Valider » et « Annuler »
/// la ferment sans rien changer.
fn column_metadata_window(ctx: &egui::Context, state: &AppState, actions: &mut Vec<Action>) {
    let Some(column) = state.column_metadata_view() else {
        return;
    };
    let meta = state.column_meta(column);
    let lines = meta.file_metadata;
    let title = if meta.long_name.is_empty() {
        "Métadonnées du fichier".to_string()
    } else {
        format!("Métadonnées — {}", meta.long_name)
    };
    let outcome = modal_dialog(
        ctx,
        egui::Id::new("plotyx_column_metadata"),
        &title,
        egui::vec2(520.0, 420.0),
        |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("plotyx_column_metadata_grid")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        for line in &lines {
                            if line.is_section() {
                                ui.label(egui::RichText::new(&line.key).strong());
                                ui.label("");
                            } else if line.key.is_empty() {
                                ui.label("");
                                ui.label(&line.value);
                            } else {
                                ui.label(&line.key);
                                ui.label(&line.value);
                            }
                            ui.end_row();
                        }
                    });
            });
        },
    );
    if outcome != DialogOutcome::Open {
        actions.push(Action::CloseColumnMetadata);
    }
}

/// Issue d'une image de [`modal_dialog`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DialogOutcome {
    /// Toujours ouvert.
    Open,
    /// « Valider » (bouton par défaut : Entrée).
    Accept,
    /// « Annuler », croix ou Échap.
    Cancel,
}

/// Hauteur de la bande des boutons « Valider » / « Annuler ».
const DIALOG_FOOTER_HEIGHT: f32 = 38.0;

/// Durée d'une alerte de clic bloqué ([`modal_dialog`]) : deux impulsions.
const MODAL_FLASH_SECONDS: f64 = 0.6;

/// Position et taille d'un dialogue modal, gardées en mémoire egui le temps
/// qu'il est ouvert.
#[derive(Clone, Copy)]
struct DialogGeometry {
    pos: egui::Pos2,
    size: egui::Vec2,
}

/// Dialogue **modal** : au premier plan, le reste de l'interface ne reçoit
/// plus aucun clic, et un clic ailleurs fait clignoter le contour du
/// dialogue en rouge foncé ([`theme::blocked_flash_color`]) pour le
/// désigner. Il se ferme par « Valider » (bouton par défaut, Entrée hors
/// saisie de texte) ou par « Annuler » (croix, Échap) : voir [`DialogOutcome`].
///
/// Même brique que les fenêtres de contenu : [`show_window_in_layer`] (cadre,
/// contour actif, rognage), [`window_header`] (placement
/// [`HeaderPlacement::Dialog`]) et [`floating_resize_handles`]. Ne diffère
/// que par l'ordre de calque (`Foreground`, au-dessus des fenêtres en
/// `Middle`) et par le calque plein écran qui capte les clics. Les menus
/// déroulants et le sélecteur de couleur ouverts depuis le dialogue restent
/// utilisables : ils sont créés après lui, donc au-dessus.
///
/// Limite : les fenêtres OS détachées sont d'autres viewports, que ce calque
/// ne couvre pas.
fn modal_dialog(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    default_size: egui::Vec2,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    modal_dialog_with(
        ctx,
        id,
        title,
        default_size,
        DialogFooter::default(),
        |_| {},
        add_contents,
    )
}

/// Le pied de page d'un [`modal_dialog_with`] : libellé et grisage du bouton par
/// défaut. Par défaut « Valider », toujours actif — ce que les propriétés du
/// graphe emploient.
struct DialogFooter<'a> {
    accept_label: &'a str,
    accept_hint: &'a str,
    accept_enabled: bool,
}

impl Default for DialogFooter<'_> {
    fn default() -> Self {
        Self {
            accept_label: "Valider",
            accept_hint: "Garder les modifications (Entrée)",
            accept_enabled: true,
        }
    }
}

/// [`modal_dialog`] avec un bouton par défaut personnalisé (`footer`) et des
/// boutons supplémentaires à gauche d'« Annuler » (`extra_buttons`, ajoutés
/// de droite à gauche comme les autres) : c'est ainsi que l'assistant d'import
/// garde « Précédent / Suivant » tout en étant **la même fenêtre** que les
/// propriétés du graphe — même cadre, même barre de titre, même blocage du
/// reste de l'interface.
fn modal_dialog_with(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    default_size: egui::Vec2,
    footer_spec: DialogFooter<'_>,
    extra_buttons: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    let screen = ctx.input(|input| input.content_rect());
    let geometry_id = id.with("geometry");
    let flash_id = id.with("flash_start");
    let stored = ctx.data(|data| data.get_temp::<DialogGeometry>(geometry_id));
    let just_opened = stored.is_none();
    let mut geometry = stored.unwrap_or_else(|| {
        let size = default_size.min(screen.size());
        DialogGeometry {
            pos: screen.center() - size / 2.0,
            size,
        }
    });
    geometry.size = geometry
        .size
        .max(MIN_CONTENT_WINDOW_SIZE)
        .min(screen.size());
    geometry.pos = geometry
        .pos
        .clamp(screen.min, (screen.max - geometry.size).max(screen.min));
    let layer = egui::LayerId::new(egui::Order::Foreground, id);

    // Le calque qui capte les clics : tout le reste de l'interface est sous lui.
    // Il est **invisible** : pas de voile sombre sur le reste de l'application,
    // pour qu'on continue de voir le rendu du graphe pendant qu'on règle ses
    // propriétés (issue #20). Le clignotement du contour dit déjà que le
    // reste est bloqué.
    let backdrop = egui::Area::new(id.with("backdrop"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .sense(egui::Sense::click_and_drag())
        .show(ctx, |ui| {
            ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
        });
    let blocked_click =
        backdrop.response.contains_pointer() && ctx.input(|input| input.pointer.any_pressed());
    let now = ctx.input(|input| input.time);
    if blocked_click {
        ctx.data_mut(|data| data.insert_temp(flash_id, now));
    }
    if blocked_click || just_opened {
        ctx.move_to_top(layer);
    }
    let flash = ctx
        .data(|data| data.get_temp::<f64>(flash_id))
        .map(|start| now - start)
        .filter(|elapsed| *elapsed < MODAL_FLASH_SECONDS)
        .map(|elapsed| {
            ctx.request_repaint();
            // Deux impulsions sur la durée : 0 → 1 → 0, deux fois.
            0.5 * (1.0 - (std::f64::consts::TAU * elapsed / (MODAL_FLASH_SECONDS / 2.0)).cos())
        });

    let popup_open = egui::Popup::is_any_open(ctx);
    let window_rect = egui::Rect::from_min_size(geometry.pos, geometry.size);
    let mut header_actions = WindowHeaderActions::default();
    let mut button_outcome = DialogOutcome::Open;
    let mut position = geometry.pos;
    let mut size = geometry.size;
    show_window_in_layer(
        ctx,
        id,
        egui::Order::Foreground,
        window_rect,
        screen,
        true,
        |ui| {
            header_actions = window_header(ui, id, title, None, false, HeaderPlacement::Dialog);
            show_window_contents_bounded(ui, |ui| {
                let full = ui.available_rect_before_wrap();
                let split = (full.bottom() - DIALOG_FOOTER_HEIGHT).max(full.top());
                let mut body = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(egui::Rect::from_min_max(
                            full.min,
                            egui::pos2(full.right(), split),
                        ))
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                body.set_clip_rect(ui.clip_rect().intersect(body.max_rect()));
                add_contents(&mut body);
                let mut footer = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(egui::Rect::from_min_max(
                            egui::pos2(full.left(), split),
                            full.max,
                        ))
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                );
                footer.painter().hline(
                    full.x_range(),
                    split,
                    footer.visuals().widgets.noninteractive.bg_stroke,
                );
                // Premier ajouté = le plus à droite : « Valider », par défaut.
                let accept = egui::Button::new(footer_spec.accept_label)
                    .fill(footer.visuals().selection.bg_fill)
                    .min_size(egui::vec2(84.0, 24.0));
                if footer
                    .add_enabled(footer_spec.accept_enabled, accept)
                    .on_hover_text(footer_spec.accept_hint)
                    .clicked()
                {
                    button_outcome = DialogOutcome::Accept;
                }
                let cancel = egui::Button::new("Annuler").min_size(egui::vec2(84.0, 24.0));
                if footer
                    .add(cancel)
                    .on_hover_text("Revenir à l'état d'ouverture (Échap)")
                    .clicked()
                {
                    button_outcome = DialogOutcome::Cancel;
                }
                extra_buttons(&mut footer);
            });
        },
        |ui, rect| {
            floating_resize_handles(ui, id, rect, &mut position, &mut size);
            if let Some(intensity) = flash {
                let style = ui.style();
                ui.painter().rect_stroke(
                    rect,
                    style.visuals.window_corner_radius,
                    egui::Stroke::new(
                        2.5,
                        theme::blocked_flash_color().gamma_multiply(intensity as f32),
                    ),
                    egui::StrokeKind::Inside,
                );
            }
        },
    );
    position += header_actions.drag_delta;
    geometry = DialogGeometry {
        pos: position,
        size,
    };
    ctx.data_mut(|data| data.insert_temp(geometry_id, geometry));

    // Échap : consommé par le sélecteur de couleur ou une liste ouverts (ils
    // le lisent avant), sinon il annule. Entrée valide, sauf pendant une
    // saisie de texte (où elle termine la saisie).
    let escape = !popup_open
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let enter = !popup_open
        && !keyboard_focus_outside_grid(ctx)
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    let outcome = if header_actions.close || escape {
        DialogOutcome::Cancel
    } else if enter && footer_spec.accept_enabled {
        DialogOutcome::Accept
    } else {
        button_outcome
    };
    if outcome != DialogOutcome::Open {
        ctx.data_mut(|data| {
            data.remove::<DialogGeometry>(geometry_id);
            data.remove::<f64>(flash_id);
        });
    }
    outcome
}

/// Corps de [`plot_settings_window`] : la bande d'onglets, puis la page de
/// l'onglet choisi.
fn plot_settings_contents(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tree: &Tree,
    settings: PlotSettings,
) {
    let mut chosen_tab = settings.tab;
    let mut chosen_axis = settings.axis;
    // Même bande que les onglets de feuille (`tab_strip`, issue #19) : elle
    // défile au lieu de rétrécir quand la fenêtre est étroite.
    let tabs: Vec<(PlotSettingsTab, String)> = PlotSettingsTab::ALL
        .iter()
        .map(|tab| (*tab, tab.label().to_string()))
        .collect();
    tab_strip(ui, &tabs, settings.tab, |tab| chosen_tab = tab, None, || {});
    ui.separator();

    let mut commit = false;
    // Couleurs enregistrées, lues avant d'emprunter le brouillon ; les
    // demandes du sélecteur (enregistrer, retirer) sont traduites en actions
    // après.
    let mut env = PickerEnv {
        project: state.saved_colors(SaveTarget::Project),
        user: state.saved_colors(SaveTarget::User),
        requests: Vec::new(),
    };
    if let Some(edit) = state
        .plot_edit_mut()
        .filter(|edit| edit.graph == settings.graph)
    {
        let spec = &mut edit.spec;
        // La page occupe toute la hauteur de la fenêtre, même quand son
        // contenu est plus court : sans `auto_shrink(false)`, une
        // `egui::Window` redimensionnable se recale sur la hauteur de son
        // contenu, et on ne pouvait pas l'agrandir au-delà. Le même réglage
        // la fait défiler quand la fenêtre est plus petite que la page.
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match chosen_tab {
                PlotSettingsTab::Graph => {
                    commit |= text_field(ui, "Titre", &mut spec.title);
                    formula_memo(ui, "plot_settings_memo_graph");
                    commit |= color_field(
                        ui,
                        "Couleur du titre",
                        &mut spec.title_color,
                        ColorUsage::Text,
                        Some("Automatique (contraste avec le fond)"),
                        &mut env,
                    );
                    ui.add_space(6.0);
                    commit |= color_field(
                        ui,
                        "Fond",
                        &mut spec.background,
                        ColorUsage::Background,
                        Some("Automatique (blanc)"),
                        &mut env,
                    );
                    ui.weak(
                        "Sur un fond sombre, le texte, les axes et la légende en couleur \
                         automatique passent en blanc.",
                    );
                }
                PlotSettingsTab::PlotArea => {
                    let Some(panel) = spec.panels.get_mut(0) else {
                        return;
                    };
                    commit |= text_field(ui, "Titre du panneau", &mut panel.title);
                    formula_memo(ui, "plot_settings_memo_panel");
                    ui.add_space(4.0);
                    ui.strong("Grille");
                    commit |= ui
                        .checkbox(&mut panel.bottom.grid, "Lignes verticales (graduations X)")
                        .changed();
                    commit |= ui
                        .checkbox(&mut panel.left.grid, "Lignes horizontales (graduations Y)")
                        .changed();
                }
                PlotSettingsTab::Axes => {
                    let Some(panel) = spec.panels.get_mut(0) else {
                        return;
                    };
                    let mut axes = vec![PlotAxis::Bottom, PlotAxis::Left];
                    if panel.uses_right_axis() || chosen_axis == PlotAxis::Right {
                        axes.push(PlotAxis::Right);
                    }
                    ui.horizontal(|ui| {
                        for axis in axes {
                            if ui
                                .selectable_label(chosen_axis == axis, axis.label())
                                .clicked()
                            {
                                chosen_axis = axis;
                            }
                        }
                    });
                    ui.separator();
                    let axis = match chosen_axis {
                        PlotAxis::Bottom => &mut panel.bottom,
                        PlotAxis::Left => &mut panel.left,
                        PlotAxis::Right => &mut panel.right,
                    };
                    commit |= axis_editor(ui, axis, &mut env);
                }
                PlotSettingsTab::Series => {
                    let Some(panel) = spec.panels.get_mut(0) else {
                        return;
                    };
                    if panel.series.is_empty() {
                        ui.weak("Ce graphe ne trace aucune série.");
                        return;
                    }
                    if edit.selected_series >= panel.series.len() {
                        edit.selected_series = panel.series.len() - 1;
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("plot_settings_series_list")
                        .max_height(140.0)
                        .show(ui, |ui| {
                            for (index, series) in panel.series.iter().enumerate() {
                                let label = series_list_label(tree, series);
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
                        commit |= series_editor(ui, series, &mut env);
                    }
                }
                PlotSettingsTab::Theme => {
                    commit |= theme_editor(ui, spec, &mut env);
                }
                PlotSettingsTab::Legend => {
                    let Some(panel) = spec.panels.get_mut(0) else {
                        return;
                    };
                    commit |= legend_editor(ui, &mut panel.legend);
                }
            });
    }
    if commit {
        state.commit_plot_edit();
    }
    for request in env.requests {
        state.dispatch(match request.edit {
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
        });
    }
    if let Some(current) = state.plot_settings_mut() {
        current.tab = chosen_tab;
        current.axis = chosen_axis;
    }
}

/// Onglet « Thème » : fond, couleur du texte et couleur des axes, réglés
/// globalement (issue #11), plus des thèmes prédéfinis. Le dernier réglage
/// écrase ce qui avait été réglé une à une : voir [`plotyx_plot::theme`].
/// Rend `true` si le brouillon doit être validé.
fn theme_editor(ui: &mut egui::Ui, spec: &mut plotyx_plot::PlotSpec, env: &mut PickerEnv) -> bool {
    let mut commit = false;
    ui.strong("Thèmes");
    ui.horizontal(|ui| {
        for theme in &plotyx_plot::THEMES {
            if ui.button(theme.name).clicked() {
                spec.apply_theme(theme);
                commit = true;
            }
        }
    });
    ui.add_space(6.0);
    ui.strong("Couleurs globales");
    commit |= color_field(
        ui,
        "Fond",
        &mut spec.background,
        ColorUsage::Background,
        Some("Automatique (blanc)"),
        env,
    );
    let mut text = spec.theme_text_color().to_string();
    let text_committed = color_field(
        ui,
        "Texte",
        &mut text,
        ColorUsage::Text,
        Some("Automatique (contraste avec le fond)"),
        env,
    );
    if text != spec.theme_text_color() {
        spec.set_text_color(&text);
    }
    commit |= text_committed;
    let mut axes = spec.theme_axes_color().to_string();
    let axes_committed = color_field(
        ui,
        "Axes et échelles",
        &mut axes,
        ColorUsage::Text,
        Some("Automatique (contraste avec le fond)"),
        env,
    );
    if axes != spec.theme_axes_color() {
        spec.set_axes_color(&axes);
    }
    commit |= axes_committed;
    ui.weak(
        "Une couleur choisie ici est écrite partout : titre du graphe, titres d'axes, \
         lignes, graduations et grilles de chaque axe. Elle remplace les couleurs déjà \
         réglées une à une dans les autres onglets.",
    );
    commit
}

/// Le mémo de syntaxe des textes du graphe (issue #33), replié par défaut,
/// sous les champs de titre, de libellé et de nom de série.
///
/// Ces textes sont composés par Typst et lus comme du **markup** (issue
/// #32) : un chimiste venu d'Origin n'en connaît pas la syntaxe, et un `_`
/// ou un `$` isolé change le rendu sans prévenir. Une section repliable
/// plutôt qu'une bulle d'aide : la fenêtre des propriétés est un dialogue
/// modal, où une fenêtre surgissante de plus se disputerait l'unique place
/// qu'egui leur réserve (issue #9).
fn formula_memo(ui: &mut egui::Ui, id_salt: &str) {
    egui::CollapsingHeader::new("Écrire une formule")
        .id_salt(id_salt)
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new(id_salt)
                .num_columns(2)
                .spacing([12.0, 2.0])
                .show(ui, |ui| {
                    for (source, effet) in FORMULA_MEMO {
                        ui.monospace(*source);
                        ui.weak(*effet);
                        ui.end_row();
                    }
                });
            ui.weak(
                "Le texte entre $ … $ est une formule ; le reste est du texte. \
                 Syntaxe complète : typst.app/docs/reference/math.",
            );
        });
}

/// Les lignes du mémo de [`formula_memo`] : ce qu'on tape, ce qu'on obtient.
const FORMULA_MEMO: &[(&str, &str)] = &[
    ("$lambda$  $Delta$", "lettres grecques λ, Δ"),
    ("$x_2$  $\"Ru(bpy)\"_3$", "indice"),
    ("$x^2$  $\"cm\"^(-1)$", "exposant"),
    ("$Delta H = sum_i n_i$", "formule complète"),
    ("$\"L mol\"^(-1)$", "texte droit dans une formule"),
    ("*gras*  _italique_", "mise en forme du texte"),
    ("\\$  \\_  \\*  \\#", "le caractère lui-même"),
];

/// Un champ texte libellé, validé à la perte de focus. Rend `true` si le
/// brouillon doit être validé — voir la doc de tête de
/// [`plot_settings_window`].
fn text_field(ui: &mut egui::Ui, label: &str, value: &mut String) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value)
    })
    .inner
    .lost_focus()
}

/// Édite un [`AxisSpec`], en trois groupes : le titre de l'axe (libellé,
/// unité), son échelle (logarithmique, inversée, bornes) et son affichage
/// (axe, grille). Rend `true` si le brouillon doit être validé — voir la doc
/// de tête de [`plot_settings_window`].
fn axis_editor(ui: &mut egui::Ui, axis: &mut AxisSpec, env: &mut PickerEnv) -> bool {
    let mut commit = false;
    ui.strong("Titre de l'axe");
    commit |= text_field(ui, "Libellé", &mut axis.label);
    commit |= text_field(ui, "Unité", &mut axis.unit);
    formula_memo(ui, "plot_settings_memo_axis");
    commit |= color_field(
        ui,
        "Couleur du titre",
        &mut axis.title_color,
        ColorUsage::Text,
        Some("Automatique (contraste avec le fond)"),
        env,
    );

    ui.add_space(6.0);
    ui.strong("Échelle");
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

    ui.add_space(6.0);
    ui.strong("Affichage");
    commit |= ui.checkbox(&mut axis.visible, "Axe affiché").changed();
    commit |= ui.checkbox(&mut axis.grid, "Grille").changed();
    commit |= color_field(
        ui,
        "Couleur de l'échelle",
        &mut axis.tick_color,
        ColorUsage::Text,
        Some("Automatique (contraste avec le fond)"),
        env,
    );
    ui.weak("Ligne d'axe, graduations et leurs libellés ; la grille la reprend, atténuée.");
    commit
}

/// Édite la série sélectionnée : nom, type de tracé, visibilité, présence en
/// légende, style de trait, style de marqueur. Rend `true` si le brouillon
/// doit être validé — voir la doc de tête de [`plot_settings_window`].
///
/// [`PlotType::Unknown`] (C5, `guide-de-correction-robustesse.md` §F3) ne
/// figure **jamais** dans la liste proposée : c'est le type de tracé d'une
/// version plus récente de PlotYx, et le proposer laisserait croire qu'on
/// peut y revenir alors que rien ne sait le reconstruire. Une série qui le
/// porte s'affiche quand même, en lecture seule pour ce seul champ — le
/// reste (nom, visibilité...) reste éditable normalement.
fn series_editor(
    ui: &mut egui::Ui,
    series: &mut plotyx_plot::SeriesSpec,
    env: &mut PickerEnv,
) -> bool {
    let mut commit = false;

    let name_response = ui.horizontal(|ui| {
        ui.label("Nom");
        ui.text_edit_singleline(&mut series.name)
    });
    commit |= name_response.inner.lost_focus();
    ui.weak("Nom vide = celui de la colonne tracée, affiché tel quel.");
    formula_memo(ui, "plot_settings_memo_series");

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
    commit |= color_field(
        ui,
        "Couleur",
        &mut series.line.color,
        ColorUsage::Curves,
        None,
        env,
    );

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

    commit |= color_field(
        ui,
        "Couleur du marqueur",
        &mut series.symbol.color,
        ColorUsage::Curves,
        Some("Comme le trait"),
        env,
    );

    commit
}

/// Édite un [`plotyx_plot::LegendSpec`] : visibilité, coin d'ancrage. Rend
/// `true` si le brouillon doit être validé — voir la doc de tête de
/// [`plot_settings_window`].
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
///
/// Une seule écriture pour la fenêtre flottante et la fenêtre détachée : elle
/// ne lit que l'arbre, que les deux ont sous la main (`state.tree()` d'un
/// côté, l'instantané du viewport de l'autre).
fn matrix_contents(ui: &mut egui::Ui, tree: &Tree, matrix: NodeId) {
    let name = node_title_in(tree, matrix, "Matrice");
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
                    .desired_width(f32::INFINITY)
                    // #38 : Tab ne doit pas faire sortir le focus du champ
                    // vers le reste de l'interface — voir `GRID_KEYS` ; c'est
                    // ce widget qui le lit (`tab_pressed`) pour valider.
                    .event_filter(GRID_KEYS),
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
            // Avec le filtre `GRID_KEYS`, Tab ne fait plus perdre le focus au
            // champ (c'était lui, auparavant, qui signalait la validation) :
            // on lit la touche, et on rend le focus nous-mêmes — la grille le
            // reprend à la fin de l'édition (`grid_contents`).
            let tab_pressed =
                response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Tab));
            if tab_pressed {
                response.surrender_focus();
            }
            if !lost_focus && !tab_pressed {
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

#[cfg(test)]
mod formula_memo_tests {
    use plotyx_render_egui::formula;

    /// Chaque exemple du mémo de syntaxe compile vraiment en Typst : un mémo
    /// qui montre une syntaxe fausse enseignerait une erreur.
    #[test]
    fn chaque_exemple_du_memo_compile() {
        let fonts =
            formula::resolve_fonts(&plotyx_render_egui::FontChoice::default()).expect("polices");
        for (source, _) in super::FORMULA_MEMO {
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

    fn vue_persistee(session: &Session, node: NodeId) -> WindowGeometry {
        *session
            .handle()
            .snapshot()
            .unwrap()
            .window_geometries
            .get(&node)
            .expect("géométrie envoyée à la session")
    }

    /// Issue #16 : l'état de chaque fenêtre (ouverte/maximisée/fermée, repliée,
    /// rang d'empilement) part à la session avec sa géométrie flottante, et
    /// cette dernière ne bouge ni à la maximisation, ni à la fermeture, ni au
    /// détachement.
    #[test]
    fn l_etat_des_fenetres_est_envoye_a_la_session_sans_toucher_a_la_geometrie_flottante() {
        use plotyx_session::WindowMode;
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.plotyx".to_string()).unwrap();
        let mut app = PlotyxApp::new(state);
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
        let state = AppState::new(session.handle(), "test.plotyx".to_string()).unwrap();
        let mut app = PlotyxApp::new(state);
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
        let path = directory.path().join("fenetres.plotyx");
        session.handle().save(&path).unwrap();

        let reloaded = Session::spawn(Project::new("Vide").unwrap(), None).unwrap();
        reloaded.handle().load(&path).unwrap();
        let state = AppState::new(reloaded.handle(), "test.plotyx".to_string()).unwrap();
        let app = PlotyxApp::new(state);
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

    /// Issue #6 : chaque étape de l'assistant d'import se peint sans
    /// paniquer, montre son titre en surbrillance et l'aperçu du fichier à
    /// droite, quelle que soit l'étape.
    #[test]
    fn chaque_etape_de_l_assistant_d_import_se_peint_avec_son_apercu() {
        // Hermétique : ni les modèles ni les fichiers récents de la machine.
        let config = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", config.path());
        let (session, _graph) = graphe();
        let mut state = AppState::new(session.handle(), "test.plotyx".to_string()).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("jasco.csv");
        std::fs::write(
            &file,
            "TITLE;S13\nXUNITS;NANOMETERS\nXYDATA\n800;1\n799;2\n798;3\n\nData points;3\n",
        )
        .unwrap();
        state.open_import(None::<NodeId>);
        state.set_import_paths(vec![file]);
        if let Some(import) = state.import_state_mut() {
            import.recipe.separator = Separator::Semicolon;
            import.recipe.header = HeaderSkip::UntilMarker("XYDATA".into());
        }
        let context = egui::Context::default();
        context.set_fonts(plotyx_render_egui::font_definitions());
        crate::theme::install(&context);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 900.0),
            )),
            ..Default::default()
        };
        for step in crate::state::ImportStep::ALL {
            let mut actions = Vec::new();
            let mut output: Option<egui::FullOutput> = None;
            for _ in 0..3 {
                if let Some(mut previous) = output.take() {
                    previous.textures_delta.clear();
                }
                output = Some(context.run_ui(input.clone(), |ui| {
                    import_window(ui.ctx(), &mut state, &mut actions);
                }));
            }
            let mut output = output.unwrap();
            output.textures_delta.clear();
            let shapes = output.shapes;
            find_text_center(&shapes, step.title());
            find_text_center(&shapes, "TITLE;S13");
            assert!(actions.is_empty(), "peindre n'importe rien");
            state.import_step_forward();
        }
        assert!(state.import_state().is_some(), "l'assistant reste ouvert");
        std::env::remove_var("XDG_CONFIG_HOME");
    }

    /// Issue #8 : un fichier lâché sur un classeur est réclamé par ce classeur
    /// (et seulement lui), un fichier lâché ailleurs n'a pas de classeur, et un
    /// survol sans dépôt n'émet aucune action. Sans fenêtre : les événements
    /// `hovered_files`/`dropped_files` sont injectés dans l'entrée d'egui.
    #[test]
    fn un_depot_est_reclame_par_le_classeur_sous_le_curseur() {
        let (session, _graph) = graphe();
        let state = AppState::new(session.handle(), "test.plotyx".to_string()).unwrap();
        let book = state.books()[0].0;
        let context = egui::Context::default();
        context.set_fonts(plotyx_render_egui::font_definitions());
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
            let state = AppState::new(session.handle(), "test.plotyx".into()).unwrap();
            let mut app = PlotyxApp::new(state);
            let book = app.state.books()[0].0;
            let tab = if kind == "classeur" {
                Tab::Book(book)
            } else {
                Tab::Graph(graph)
            };
            let viewport = if kind == "explorateur" {
                app.detach_project_manager();
                egui::ViewportId::from_hash_of("plotyx_project_manager_window")
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
        let mut state = AppState::new(session.handle(), "test.plotyx".into()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("mesure.csv");
        std::fs::write(&file, "1,10\n2,20\n").unwrap();
        state.dispatch(Action::ImportFiles {
            destination: crate::state::ImportDestination::NewBookForAll,
            recipe: plotyx_import::ImportRecipe::new("Sans titre"),
            paths: vec![file],
        });
        let app = PlotyxApp::new(state);
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
        let pos = find_text_center(&output.shapes, "Régler l'import…");
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
        let state = AppState::new(session.handle(), "test.plotyx".to_string()).unwrap();
        let mut app = PlotyxApp::new(state);
        let context = egui::Context::default();
        context.set_fonts(plotyx_render_egui::font_definitions());
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
        let mut state = AppState::new(session.handle(), "test.plotyx".to_string()).unwrap();
        state.select_cell(sheet, column, 0);
        let context = egui::Context::default();
        context.set_fonts(plotyx_render_egui::font_definitions());
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
        let mut spec =
            plotyx_plot::PlotSpec::empty("Figure").with_panels(plotyx_plot::PanelSpec::at(0, 0));
        spec.set_text_color("#336699");
        let avant = spec.clone();
        let mut env = PickerEnv {
            project: Default::default(),
            user: Default::default(),
            requests: Vec::new(),
        };
        let context = egui::Context::default();
        context.set_fonts(plotyx_render_egui::font_definitions());
        let mut commit = true;
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            commit = theme_editor(ui, &mut spec, &mut env);
        });
        output.textures_delta.clear();
        assert!(!commit);
        assert_eq!(spec, avant);
        assert!(!output.shapes.is_empty());
    }

    /// #19 : les onglets de la fenêtre des propriétés du graphe sont la
    /// bande commune (`tab_strip`) : tous les onglets sont peints, et un clic
    /// sur l'un d'eux le choisit.
    #[test]
    fn les_onglets_des_proprietes_sont_la_bande_commune() {
        let context = egui::Context::default();
        context.set_fonts(plotyx_render_egui::font_definitions());
        let tabs: Vec<(PlotSettingsTab, String)> = PlotSettingsTab::ALL
            .iter()
            .map(|tab| (*tab, tab.label().to_string()))
            .collect();
        let mut chosen = None;
        let run = |events: Vec<egui::Event>, chosen: &mut Option<PlotSettingsTab>| {
            let input = egui::RawInput {
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 200.0),
                )),
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                tab_strip(
                    ui,
                    &tabs,
                    PlotSettingsTab::Graph,
                    |tab| *chosen = Some(tab),
                    None,
                    || {},
                );
            });
            output.textures_delta.clear();
            output
        };
        run(vec![], &mut chosen);
        let output = run(vec![], &mut chosen);
        fn position(output: &egui::FullOutput, needle: &str) -> Option<egui::Pos2> {
            fn walk(shape: &egui::Shape, needle: &str) -> Option<egui::Pos2> {
                match shape {
                    egui::Shape::Text(text) if text.galley.text() == needle => {
                        Some(text.visual_bounding_rect().center())
                    }
                    egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| walk(s, needle)),
                    _ => None,
                }
            }
            output
                .shapes
                .iter()
                .find_map(|clipped| walk(&clipped.shape, needle))
        }
        for tab in PlotSettingsTab::ALL {
            assert!(position(&output, tab.label()).is_some(), "{}", tab.label());
        }
        let theme = position(&output, "Thème").unwrap();
        let click = |pressed| egui::Event::PointerButton {
            pos: theme,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        run(vec![egui::Event::PointerMoved(theme)], &mut chosen);
        run(vec![click(true)], &mut chosen);
        run(vec![click(false)], &mut chosen);
        assert_eq!(chosen, Some(PlotSettingsTab::Theme));
    }

    /// #12 : un fond sans couleur ou semi-transparent s'affiche sur du
    /// blanc ; un fond opaque n'a pas besoin de dessous.
    #[test]
    fn le_blanc_d_affichage_ne_double_que_les_fonds_transparents() {
        assert_eq!(
            display_backdrop(plotyx_plot::TRANSPARENT),
            Some(egui::Color32::WHITE)
        );
        assert_eq!(display_backdrop("#12345680"), Some(egui::Color32::WHITE));
        assert_eq!(display_backdrop("#123456"), None);
    }

    /// #14 : tant qu'un dialogue modal est ouvert, un clic sur le reste de
    /// l'interface n'a aucun effet et déclenche le clignotement du contour ;
    /// Échap le ferme ; sans dialogue, le même clic agit.
    #[test]
    fn un_dialogue_modal_bloque_l_arriere_plan_et_clignote() {
        let id = egui::Id::new("dialogue_test");
        let clicks = std::cell::Cell::new(0);
        let closed = std::cell::Cell::new(DialogOutcome::Open);
        let run = |context: &egui::Context, modal: bool, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                if ui.button("fond").clicked() {
                    clicks.set(clicks.get() + 1);
                }
                if modal {
                    let close =
                        modal_dialog(ui.ctx(), id, "Test", egui::vec2(300.0, 200.0), |ui| {
                            ui.label("contenu");
                        });
                    if close != DialogOutcome::Open {
                        closed.set(close);
                    }
                }
            });
            output.textures_delta.clear();
        };
        let click = |context: &egui::Context, modal: bool| {
            let at = egui::pos2(20.0, 14.0);
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            run(context, modal, vec![egui::Event::PointerMoved(at)]);
            run(context, modal, vec![button(true)]);
            run(context, modal, vec![button(false)]);
        };

        let free = egui::Context::default();
        run(&free, false, vec![]);
        click(&free, false);
        assert_eq!(clicks.get(), 1, "sans dialogue, le bouton répond");

        let context = egui::Context::default();
        for _ in 0..3 {
            run(&context, true, vec![]);
        }
        click(&context, true);
        assert_eq!(clicks.get(), 1, "le dialogue a bloqué le clic");
        assert!(
            context
                .data(|data| data.get_temp::<f64>(id.with("flash_start")))
                .is_some(),
            "le clic bloqué déclenche le clignotement"
        );
        assert_eq!(closed.get(), DialogOutcome::Open);
        run(
            &context,
            true,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert_eq!(closed.get(), DialogOutcome::Cancel, "Échap annule");

        // Valider est le bouton par défaut : Entrée (hors saisie de texte).
        closed.set(DialogOutcome::Open);
        let again = egui::Context::default();
        for _ in 0..3 {
            run(&again, true, vec![]);
        }
        run(
            &again,
            true,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert_eq!(closed.get(), DialogOutcome::Accept, "Entrée valide");
    }

    /// #13 : la fenêtre des propriétés n'écrit pas son propre cadre — elle
    /// passe par la brique des fenêtres de contenu (`show_window_in_layer`,
    /// `window_header`) ; `egui::Window` n'y est plus employé.
    #[test]
    fn la_fenetre_des_proprietes_est_construite_comme_les_autres() {
        let source = include_str!("app.rs");
        let start = source.find("fn plot_settings_window").expect("fonction");
        let end = source[start..]
            .find("fn plot_settings_contents")
            .expect("fin");
        let body = &source[start..start + end];
        assert!(!body.contains("egui::Window::new"), "plus de Window brute");
        assert!(body.contains("modal_dialog("));
        let dialog = source.find("fn modal_dialog").expect("brique");
        let dialog = &source[dialog..dialog + 6000];
        assert!(dialog.contains("show_window_in_layer("));
        assert!(dialog.contains("window_header("));
    }

    /// Règle du projet : **toute** fenêtre de l'application passe par la brique
    /// commune (`show_window_in_layer`, via `modal_dialog_with` pour un dialogue),
    /// jamais par une `egui::Window` brute. L'assistant d'import a déjà dérogé
    /// à cette règle une fois : ce garde-fou l'empêche de se reproduire.
    #[test]
    fn aucune_fenetre_n_est_une_egui_window_brute() {
        let source = include_str!("app.rs");
        let production = &source[..source.find("#[cfg(test)]").expect("module de tests")];
        let construct = ["egui::Window", "::new("].concat();
        assert!(
            !production.contains(&construct),
            "une fenêtre est construite par `egui::Window` brute : passer par \
             `show_window_in_layer` / `modal_dialog_with` (voir `plotyx-feature`)"
        );
        let start = production.find("fn import_window").expect("fonction");
        let body = &production[start..start + 4000];
        assert!(
            body.contains("modal_dialog_with("),
            "l'assistant d'import est un dialogue modal"
        );
    }

    #[test]
    fn le_repli_ne_concerne_que_longlet_vise() {
        let (_session, graph) = graphe();
        let graph_tab = Tab::Graph(graph);
        let mut collapsed = HashSet::new();

        toggle_collapse(&mut collapsed, graph_tab);
        assert_eq!(collapsed, HashSet::from([graph_tab]));

        // B5 (`retours-2026-09-23-bugs.md`) : `Tab::Book` a
        // aussi ce contrôle depuis ce correctif, suivi indépendamment du
        // graphe même quand son identifiant de nœud coïncide (test de
        // présentation).
        let book_tab = Tab::Book(graph);
        toggle_collapse(&mut collapsed, book_tab);
        assert_eq!(collapsed, HashSet::from([graph_tab, book_tab]));

        toggle_collapse(&mut collapsed, graph_tab);
        assert_eq!(collapsed, HashSet::from([book_tab]));

        toggle_collapse(&mut collapsed, book_tab);
        assert!(collapsed.is_empty());
    }

    #[test]
    fn le_repli_reste_refuse_aux_matrices_et_aux_notes() {
        let (_session, graph) = graphe();
        let mut collapsed = HashSet::new();

        toggle_collapse(&mut collapsed, Tab::Matrix(graph));
        assert!(
            collapsed.is_empty(),
            "une matrice n'a pas ce contrôle : rien à replier"
        );

        toggle_collapse(&mut collapsed, Tab::Note(graph));
        assert!(
            collapsed.is_empty(),
            "une note n'a pas ce contrôle : rien à replier"
        );
    }

    // B7 (`retours-2026-09-23-bugs.md`) : jusque-là, la
    // surbrillance recolorait le trait du `Frame` de la fenêtre
    // (`floating_window_frame`) plutôt que de peindre un second contour par
    // dessus — mais ce trait suit `content_ui.min_rect()`, pas la taille
    // demandée, et débordait donc quand le contenu dépassait (bande
    // d'outils qui ne s'enroule pas). Le `Frame` ne code plus l'activité :
    // il garde toujours le trait par défaut d'egui, quel que soit l'état.
    #[test]
    fn le_frame_de_la_fenetre_flottante_garde_toujours_le_trait_par_defaut() {
        let style = egui::Style::default();

        let frame = floating_window_frame(&style);

        assert_eq!(
            frame.stroke.color,
            style.visuals.window_stroke().color,
            "le Frame ne recolore plus selon l'activité (B7) : le contour actif est peint à \
             part par `paint_active_window_outline`, sur le rectangle demandé plutôt que celui \
             qu'egui recalcule depuis le contenu"
        );
    }

    // Le trait recoloré vit maintenant dans `active_window_stroke`, utilisé
    // par [`paint_active_window_outline`] plutôt que par le `Frame` — même
    // logique de distinction active/inactive qu'avant B7, déplacée d'une
    // fonction à l'autre.
    #[test]
    fn active_window_stroke_distingue_toujours_actif_et_inactif() {
        let style = egui::Style::default();

        let inactif = active_window_stroke(&style, false);
        let actif = active_window_stroke(&style, true);

        assert_eq!(
            inactif.color,
            style.visuals.window_stroke().color,
            "une fenêtre inactive garde le trait par défaut d'egui"
        );
        assert_eq!(
            actif.color, style.visuals.selection.stroke.color,
            "une fenêtre active reprend la couleur de sélection du thème"
        );
        assert_ne!(
            actif.color, inactif.color,
            "sans couleurs distinctes, rien ne montrerait quelle fenêtre est active"
        );
    }

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
        // l'espace de travail (fenêtre plus grande que lui, cas dégénéré).
        let max_scroll = workspace_max_scroll(viewport_size);
        let hors_bornes =
            egui::Rect::from_min_size(WORKSPACE_SIZE.to_pos2(), egui::vec2(50.0, 50.0));
        let revealed = workspace_scroll_to_reveal(egui::Vec2::ZERO, viewport_size, hors_bornes);
        assert!(revealed.x <= max_scroll.x && revealed.y <= max_scroll.y);
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

    /// Peint des fenêtres flottantes par le chemin de production
    /// ([`show_floating_window`]), sur une image, et rend la sortie d'egui.
    /// Deux images au moins sont nécessaires : la première est la passe de
    /// mesure d'une `Area` neuve, qui ne peint rien.
    fn peindre_fenetres(
        context: &egui::Context,
        fenetres: &[(egui::Id, egui::Rect)],
        viewport: egui::Rect,
        events: Vec<egui::Event>,
        contenu: impl Fn(&mut egui::Ui),
    ) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 1000.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| {
            for &(id, rect) in fenetres {
                show_floating_window(ui.ctx(), id, rect, viewport, true, &contenu, |_, _| {});
            }
        });
        output.textures_delta.clear();
        output
    }

    /// Contenu plus large que n'importe quelle fenêtre de test — comme un
    /// classeur à plusieurs colonnes.
    fn contenu_debordant(ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for index in 0..10 {
                ui.label(format!("colonne {index}"));
                ui.add_space(80.0);
            }
        });
    }

    /// Tous les rectangles peints, `Shape::Vec` dépliés (le cadre d'egui
    /// arrive en `Vec[ombre, rectangle]`).
    fn rectangles_peints(shapes: &[egui::epaint::ClippedShape]) -> Vec<egui::epaint::RectShape> {
        fn deplier(shape: &egui::Shape, out: &mut Vec<egui::epaint::RectShape>) {
            match shape {
                egui::Shape::Rect(rect) => out.push(rect.clone()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| deplier(shape, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for painted in shapes {
            deplier(&painted.shape, &mut out);
        }
        out
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
    }

    // Bug 1, la conséquence, et B1/B3/B4/B7 du rapport du 2026-09-23 : le
    // cadre, le contour actif, la zone cliquable et le rognage du contenu
    // suivent tous le même rectangle, que le contenu soit court ou déborde.
    #[test]
    fn le_cadre_le_contour_et_la_zone_cliquable_suivent_le_meme_rectangle() {
        for deborde in [false, true] {
            let context = egui::Context::default();
            let id = egui::Id::new(("meme_rectangle", deborde));
            let window_rect =
                egui::Rect::from_min_size(egui::pos2(100.0, 80.0), egui::vec2(220.0, 180.0));
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
            let contenu = |ui: &mut egui::Ui| {
                if deborde {
                    contenu_debordant(ui);
                } else {
                    ui.label("contenu court");
                }
            };
            peindre_fenetres(&context, &[(id, window_rect)], viewport, vec![], contenu);
            let output =
                peindre_fenetres(&context, &[(id, window_rect)], viewport, vec![], contenu);

            let style = egui::Style::default();
            let rectangles = rectangles_peints(&output.shapes);
            let cadre = rectangles
                .iter()
                .find(|rect| rect.fill == style.visuals.window_fill())
                .expect("le cadre doit être peint");
            let contour = rectangles
                .iter()
                .find(|rect| rect.stroke.color == style.visuals.selection.stroke.color)
                .expect("le contour actif doit être peint");
            assert_eq!(cadre.rect, window_rect, "deborde={deborde}");
            assert_eq!(contour.rect, window_rect, "deborde={deborde}");
            assert_eq!(
                context.memory(|memory| memory.area_rect(id)),
                Some(window_rect.expand(RESIZE_BORDER)),
                "la zone cliquable est la fenêtre plus ses poignées, deborde={deborde}"
            );
            for painted in &output.shapes {
                if let egui::Shape::Text(text) = &painted.shape {
                    assert!(
                        window_rect.contains_rect(painted.clip_rect),
                        "{:?} déborde de la fenêtre : découpe {:?}",
                        text.galley.job.text,
                        painted.clip_rect,
                    );
                }
            }
        }
    }

    // Bug 2 (`retours-2026-09-24-bugs.md`) : une fenêtre à
    // cheval sur le bord de la vue (espace de travail défilé) peignait
    // par-dessus le Project Manager, et y captait les clics.
    // Un classeur coupé par le bas de la vue : sa bande d'onglets de
    // feuille, posée au bas de la fenêtre, ne doit pas être dessinée sous la
    // vue (sur la barre de défilement ou la console). `egui::Panel` remplace
    // le rognage de son parent au lieu de le combiner : c'est lui qui
    // faisait déborder les onglets.
    #[test]
    fn un_classeur_coupe_par_le_bas_de_la_vue_ne_dessine_rien_dessous() {
        let (session, _graph) = graphe();
        let mut state = AppState::new(session.handle(), "test.plotyx".to_string()).unwrap();
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
        context.set_fonts(plotyx_render_egui::font_definitions());
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

    #[test]
    fn une_fenetre_a_cheval_sur_le_bord_de_la_vue_est_rognee_et_laisse_passer_les_clics() {
        let context = egui::Context::default();
        let id = egui::Id::new("a_cheval");
        // La vue commence à x = 400 (à gauche : le Project Manager).
        let viewport = egui::Rect::from_min_max(egui::pos2(400.0, 0.0), egui::pos2(1400.0, 1000.0));
        let window_rect =
            egui::Rect::from_min_size(egui::pos2(250.0, 100.0), egui::vec2(500.0, 400.0));
        let fenetres = [(id, window_rect)];
        peindre_fenetres(&context, &fenetres, viewport, vec![], contenu_debordant);
        let output = peindre_fenetres(&context, &fenetres, viewport, vec![], contenu_debordant);

        assert!(!output.shapes.is_empty());
        for painted in &output.shapes {
            assert!(
                viewport.contains_rect(painted.clip_rect),
                "une forme de la fenêtre est découpée hors de la vue : {:?}",
                painted.clip_rect,
            );
        }
        let calque = egui::LayerId::new(FLOATING_WINDOW_ORDER, id);
        assert_ne!(
            context.layer_id_at(egui::pos2(300.0, 300.0)),
            Some(calque),
            "sous le Project Manager, la partie cachée de la fenêtre ne doit capter aucun clic"
        );
        assert_eq!(context.layer_id_at(egui::pos2(500.0, 300.0)), Some(calque));
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
        let mut state = AppState::new(session.handle(), "test.plotyx".to_string()).unwrap();
        let context = egui::Context::default();
        context.set_fonts(plotyx_render_egui::font_definitions());
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
    fn les_onglets_de_feuille_defilent_plutot_que_de_retrecir() {
        let sheets: Vec<(NodeId, String)> = (0..10)
            .map(|index| (NodeId::from_raw(index + 1), format!("Feuille {index}")))
            .collect();
        let context = egui::Context::default();
        context.set_fonts(plotyx_render_egui::font_definitions());
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(200.0, 60.0),
            )),
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| {
            ui.allocate_ui(egui::vec2(200.0, TAB_STRIP_HEIGHT), |ui| {
                sheet_tabs(ui, &sheets, sheets[0].0, |_| {}, || {});
            });
        });
        output.textures_delta.clear();
        // Dix onglets en taille naturelle ne tiennent pas dans 200 points :
        // la bande doit défiler plutôt que rétrécir ses boutons ou replier
        // les feuilles cachées dans un menu.
        assert!(
            !output.shapes.is_empty(),
            "les onglets visibles doivent être peints"
        );
    }

    /// Peint une bande d'onglets dans 400 points de large (cinq onglets
    /// « Feuille N » environ, sur dix), à l'instant
    /// `*time + 0.1`, avec `events`. Le défilement automatique est animé
    /// (`Response::scroll_to_me`) : il s'étale sur `ScrollAnimation::duration`
    /// (jusqu'à 0.3 s par défaut) ; faire avancer `RawInput::time`
    /// explicitement simule ce laps de temps sans dépendre de l'horloge réelle.
    fn peindre_onglets(
        context: &egui::Context,
        sheets: &[(NodeId, String)],
        selected: NodeId,
        time: &mut f64,
        events: Vec<egui::Event>,
    ) -> TabStripOutput {
        *time += 0.1;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 60.0),
            )),
            time: Some(*time),
            events,
            ..Default::default()
        };
        let mut sortie = TabStripOutput::default();
        let mut output = context.run_ui(input, |ui| {
            sortie = sheet_tabs(ui, sheets, selected, |_| {}, || {});
        });
        output.textures_delta.clear();
        sortie
    }

    fn dix_feuilles() -> (egui::Context, Vec<(NodeId, String)>) {
        let sheets = (0..10)
            .map(|index| (NodeId::from_raw(index + 1), format!("Feuille {index}")))
            .collect();
        let context = egui::Context::default();
        context.set_fonts(plotyx_render_egui::font_definitions());
        theme::install(&context);
        (context, sheets)
    }

    #[test]
    fn selectionner_un_onglet_hors_vue_fait_defiler_la_bande_pour_le_montrer() {
        let (context, sheets) = dix_feuilles();
        let mut time = 0.0;
        // Sélectionner d'abord la première feuille, laisser le défilement se
        // stabiliser, puis basculer sur la dernière : hors vue, elle doit
        // déclencher un nouveau défilement (`should_scroll_to_selected`).
        let mut offset_before = 0.0;
        for _ in 0..3 {
            offset_before =
                peindre_onglets(&context, &sheets, sheets[0].0, &mut time, Vec::new()).offset;
        }
        let mut offset_after = 0.0;
        for _ in 0..8 {
            offset_after =
                peindre_onglets(&context, &sheets, sheets[9].0, &mut time, Vec::new()).offset;
        }
        assert!(
            offset_after > offset_before,
            "sélectionner un onglet hors vue doit faire défiler la bande vers lui : {offset_before:?} -> {offset_after:?}"
        );
    }

    /// Issue #1 : cliquer sur le dernier onglet entièrement visible décale la
    /// bande juste assez pour montrer son voisin de droite — un onglet de
    /// plus, pas deux (l'ancien comportement centrait l'onglet sélectionné).
    /// Symétriquement vers la gauche.
    #[test]
    fn selectionner_le_dernier_onglet_visible_ne_decale_que_d_un_onglet() {
        let (context, sheets) = dix_feuilles();
        let mut time = 0.0;
        let mut sortie = TabStripOutput::default();
        for _ in 0..4 {
            sortie = peindre_onglets(&context, &sheets, sheets[0].0, &mut time, Vec::new());
        }
        let (_, dernier) = sortie.fully_visible.expect("des onglets visibles");
        assert!(
            dernier + 2 < sheets.len(),
            "il faut des onglets cachés à droite"
        );
        for _ in 0..8 {
            sortie = peindre_onglets(&context, &sheets, sheets[dernier].0, &mut time, Vec::new());
        }
        let (premier_apres, dernier_apres) = sortie.fully_visible.unwrap();
        assert_eq!(
            dernier_apres,
            dernier + 1,
            "seul le voisin de droite de l'onglet sélectionné doit apparaître : {sortie:?}"
        );

        // Vers la gauche : sélectionner le premier onglet visible montre son
        // voisin de gauche, et rien de plus.
        for _ in 0..8 {
            sortie = peindre_onglets(
                &context,
                &sheets,
                sheets[premier_apres].0,
                &mut time,
                Vec::new(),
            );
        }
        assert_eq!(
            sortie.fully_visible.unwrap().0,
            premier_apres - 1,
            "seul le voisin de gauche doit apparaître : {sortie:?}"
        );

        // Un onglet sélectionné loin de la vue (par script) redevient visible.
        for _ in 0..8 {
            sortie = peindre_onglets(&context, &sheets, sheets[9].0, &mut time, Vec::new());
        }
        assert_eq!(sortie.fully_visible.unwrap().1, 9, "{sortie:?}");
    }

    /// Les boutons « < » et « > » n'apparaissent que si des onglets sont
    /// cachés ; « > » fait défiler la bande vers la droite, puis « < »
    /// devient actif.
    #[test]
    fn les_fleches_d_onglets_n_apparaissent_que_si_des_feuilles_sont_cachees() {
        let (context, sheets) = dix_feuilles();
        let mut time = 0.0;
        let mut sortie = TabStripOutput::default();
        for _ in 0..3 {
            sortie = peindre_onglets(&context, &sheets, sheets[0].0, &mut time, Vec::new());
        }
        assert_eq!(
            sortie.overflow,
            TabStripOverflow {
                hidden_left: false,
                hidden_right: true
            }
        );
        let [_, next] = sortie
            .arrows
            .expect("des feuilles cachées : flèches affichées");
        let clic = next.center();
        peindre_onglets(
            &context,
            &sheets,
            sheets[0].0,
            &mut time,
            vec![
                egui::Event::PointerMoved(clic),
                egui::Event::PointerButton {
                    pos: clic,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        peindre_onglets(
            &context,
            &sheets,
            sheets[0].0,
            &mut time,
            vec![egui::Event::PointerButton {
                pos: clic,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        for _ in 0..6 {
            sortie = peindre_onglets(&context, &sheets, sheets[0].0, &mut time, Vec::new());
        }
        assert!(
            sortie.offset > 0.0,
            "« > » doit faire défiler la bande : {sortie:?}"
        );
        assert!(
            sortie.overflow.hidden_left,
            "« < » devient utile : {sortie:?}"
        );

        // Deux feuilles tiennent dans la bande : pas de flèches.
        let (context, _) = dix_feuilles();
        let deux = &sheets[..2];
        for _ in 0..3 {
            sortie = peindre_onglets(&context, deux, deux[0].0, &mut time, Vec::new());
        }
        assert!(
            sortie.arrows.is_none(),
            "aucune feuille cachée : pas de flèches"
        );
    }

    // B2 (`retours-2026-09-23-bugs.md`) : `sync_floating_tabs_order_to_top_layer`
    // teste la logique pure de réalignement.

    #[test]
    fn le_tab_au_premier_plan_est_ramene_en_fin_de_floating_tabs() {
        let tab_a = Tab::Graph(NodeId::from_raw(1));
        let tab_b = Tab::Graph(NodeId::from_raw(2));
        let mut floating_tabs = vec![tab_a, tab_b];

        // `tab_b` est déjà en fin de liste (donc déjà « actif ») : rien à
        // faire.
        sync_floating_tabs_order_to_top_layer(&mut floating_tabs, Some(tab_b));
        assert_eq!(floating_tabs, vec![tab_a, tab_b]);

        // `tab_a` est au premier plan (clic direct) sans être en fin de
        // liste : il doit passer en dernier.
        sync_floating_tabs_order_to_top_layer(&mut floating_tabs, Some(tab_a));
        assert_eq!(floating_tabs, vec![tab_b, tab_a]);
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
            context.run_ui(input, |ui| {
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
            })
        };

        // Image de repos : `floating_tabs` place `tab_b` en dernier, comme
        // s'il avait été ouvert (ou cliqué) en dernier légitimement.
        paint(vec![]).textures_delta.clear();

        // Clic dans `tab_a`, qui n'est pas en fin de `floating_tabs` — sur
        // deux images distinctes, comme les autres tests d'interaction du
        // fichier (déplacer la souris, puis presser) : `layer_id_at` doit
        // pouvoir s'appuyer sur une position de pointeur déjà connue.
        let click_pos = rect_a.center();
        paint(vec![egui::Event::PointerMoved(click_pos)])
            .textures_delta
            .clear();
        let mut output = paint(vec![egui::Event::PointerButton {
            pos: click_pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        }]);
        output.textures_delta.clear();

        let top_layer_tab = context.top_layer_id().and_then(|top| {
            floating_tabs
                .iter()
                .copied()
                .find(|&tab| egui::Id::new(tab) == top.id)
        });
        assert_eq!(
            top_layer_tab,
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

        sync_floating_tabs_order_to_top_layer(&mut floating_tabs, top_layer_tab);
        assert_eq!(
            floating_tabs.last().copied(),
            Some(tab_a),
            "après resynchronisation, la fenêtre cliquée doit devenir active_tab \
             (donc se surligner)"
        );
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
                    Some(Tab::Graph(graph)),
                    false,
                    HeaderPlacement::Floating,
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

        // Régression : les icônes d'action étaient posées via `Button::
        // small()`, un widget avec son propre cadre/padding qui ne
        // s'aligne pas pixel pour pixel sur le titre peint brut — d'où un
        // désalignement visible malgré des rects nominalement centrés sur
        // la même ligne (voir la doc de tête de `window_header`).
        // `header_icon` peint désormais tout de la même façon : les centres
        // verticaux doivent coïncider exactement.
        let title_center_y = title.pos.y + title.galley.rect.center().y;
        let close_center_y = {
            let close = text_shape("×").expect("le bouton fermer doit être peint");
            close.pos.y + close.galley.rect.center().y
        };
        assert!(
            (title_center_y - close_center_y).abs() < f32::EPSILON,
            "le titre et les icônes d'action doivent partager le même centre vertical"
        );
    }

    // B5 (`retours-2026-09-23-bugs.md`) : l'icône « Masquer le
    // contenu de… » était réservée à `Tab::Graph` sans raison documentée —
    // vérifie qu'elle est bien peinte pour `Tab::Book`, et toujours absente
    // pour `Tab::Matrix`/`Tab::Note`, qui n'ont pas ce contrôle.
    #[test]
    fn licone_de_repli_est_peinte_pour_les_classeurs_mais_pas_les_matrices() {
        let paint_and_find_collapse_arrow = |tab: Tab| -> bool {
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
                        egui::Id::new("collapse_icon_presence"),
                        "Titre",
                        Some(tab),
                        false,
                        HeaderPlacement::Floating,
                    );
                },
            );
            output.textures_delta.clear();
            output.shapes.iter().any(|painted| {
                matches!(&painted.shape, egui::Shape::Text(shape) if shape.galley.job.text == "v")
            })
        };

        let node = NodeId::from_raw(1);
        assert!(
            paint_and_find_collapse_arrow(Tab::Book(node)),
            "un classeur doit porter l'icône de repli"
        );
        assert!(
            paint_and_find_collapse_arrow(Tab::Graph(node)),
            "un graphe doit garder l'icône de repli"
        );
        assert!(
            !paint_and_find_collapse_arrow(Tab::Matrix(node)),
            "une matrice n'a pas ce contrôle"
        );
        assert!(
            !paint_and_find_collapse_arrow(Tab::Note(node)),
            "une note n'a pas ce contrôle"
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
        context.set_fonts(plotyx_render_egui::font_definitions());
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
                    // maillage se compare donc sur sa géométrie seule.
                    plotyx_render_egui::scaled::transform_shape(
                        &context,
                        &mut displayed,
                        transform,
                        &mut |_| None,
                    );
                    let same = |painted: &egui::Shape| match (painted, &displayed) {
                        (egui::Shape::Mesh(a), egui::Shape::Mesh(b)) => {
                            a.vertices == b.vertices && a.indices == b.indices
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
        let state = AppState::new(session.handle(), "test.plotyx".to_string()).unwrap();
        let mut app = PlotyxApp::new(state);
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        context.set_fonts(plotyx_render_egui::font_definitions());

        let book_tab = Tab::Book(book);
        assert!(
            app.floating_tabs.contains(&book_tab),
            "PlotyxApp::new doit ouvrir le premier classeur en fenêtre flottante"
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
        let content_labels = ["#", "Long Name", "Units", "A(Y)", "Sheet1"];
        let mut checked = 0;
        for painted in &output.shapes {
            let egui::Shape::Text(shape) = &painted.shape else {
                continue;
            };
            if !content_labels.contains(&shape.galley.job.text.as_str()) {
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
        let header_y = position_of("A(Y)")
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
            (false, "Ajouter une colonne"),
            (true, "Ajouter une feuille"),
        ] {
            let context = egui::Context::default();
            let mut state = AppState::new(session.handle(), "test.plotyx".to_string()).unwrap();
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
                        egui::Shape::Text(shape) => shape.galley.job.text == label,
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
                let menu_pos = find_text_center(&output.shapes, label);
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
        let state = AppState::new(session.handle(), "test.plotyx".to_string()).unwrap();
        let mut app = PlotyxApp::new(state);
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        context.set_fonts(plotyx_render_egui::font_definitions());

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
        let first = paint(Vec::new());
        let icon_pos = find_text_center(&first.shapes, "v");

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
        let state = AppState::new(session.handle(), "test.plotyx".to_string()).unwrap();
        let mut app = PlotyxApp::new(state);
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        context.set_fonts(plotyx_render_egui::font_definitions());

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

    /// Régression (§3, redimensionnement par les 4 côtés) : `egui::Window`
    /// coupe `resize_left`/`resize_top` en interne dès que
    /// `.drag_area(WindowDrag::Off)` est posé (nécessaire au glissé de la
    /// barre de titre maison) — les deux dépendent d'`Area::is_movable()`,
    /// qu'`Off` force à `false`, sans moyen de les garder actifs séparément
    /// (voir la doc de tête de `PlotyxApp::window_sizes`). D'où
    /// [`floating_resize_handles`], qui remplace entièrement le
    /// redimensionnement natif. Ce test glisse sa poignée ouest et vérifie
    /// que la position *et* la taille suivent, sur deux images (la seconde
    /// sans événement, pour vérifier l'absence de saut en arrière).
    #[test]
    fn le_redimensionnement_par_le_bord_gauche_deplace_et_retrecit() {
        let tab = Tab::Graph(NodeId::from_raw(1));
        let context = egui::Context::default();
        let state = std::cell::Cell::new((egui::pos2(300.0, 300.0), egui::vec2(400.0, 300.0)));

        let paint = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1200.0),
                )),
                events,
                ..Default::default()
            };
            let (mut position, mut size) = state.get();
            let rect = egui::Rect::from_min_size(position, size);
            let mut output = context.run_ui(input, |ui| {
                floating_resize_handles(ui, tab, rect, &mut position, &mut size);
            });
            output.textures_delta.clear();
            state.set((position, size));
        };

        paint(Vec::new());
        let (position, size) = state.get();
        let west_handle = egui::pos2(position.x - 3.0, position.y + size.y / 2.0);
        paint(vec![egui::Event::PointerMoved(west_handle)]);
        paint(vec![egui::Event::PointerButton {
            pos: west_handle,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        let dragged_to = west_handle + egui::vec2(-40.0, 0.0);
        paint(vec![egui::Event::PointerMoved(dragged_to)]);

        let (position, size) = state.get();
        assert!(
            (position.x - 260.0).abs() < f32::EPSILON,
            "le bord ouest doit déplacer le coin de la fenêtre de la même distance que le glissé, position={position:?}"
        );
        assert!(
            (size.x - 440.0).abs() < f32::EPSILON,
            "le bord ouest doit élargir la fenêtre d'autant que le coin recule, size={size:?}"
        );

        paint(vec![egui::Event::PointerButton {
            pos: dragged_to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);

        let settled = state.get();
        // Image suivante sans aucun événement : rien ne doit plus bouger —
        // contrairement au bug natif, il n'y a ici aucun mécanisme qui
        // pourrait faire sauter la fenêtre en arrière (`position`/`size`
        // sont notre propre état, pas une mémoire interne qu'un autre
        // appel pourrait écraser).
        paint(Vec::new());
        assert_eq!(state.get(), settled);
    }

    /// Trouve le centre du texte peint contenant `needle` dans `shapes` —
    /// utilisé pour cliquer une entrée du Project Manager sans supposer sa
    /// position exacte (mise en page auto). Même technique que
    /// `len_tete_du_graphe_garde_son_titre_au_centre_et_sa_fleche_a_gauche`.
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
        let state = AppState::new(session.handle(), "test.plotyx".to_string()).unwrap();
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
        let hide_pos = find_text_center(&with_toolbar.shapes, "Masquer");
        click_at(&context, hide_pos, egui::Modifiers::NONE, &paint);
        assert_eq!(
            *hidden.borrow(),
            HashSet::from([graph_tab, book_tab]),
            "« Masquer » doit s'appliquer à toute la sélection, hidden={:?}",
            hidden.borrow()
        );

        let with_hidden_rows = paint(&context, Vec::new());
        let show_pos = find_text_center(&with_hidden_rows.shapes, "Afficher");
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
    use plotyx_core::{Column, Project};
    use plotyx_session::Session;

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
            let state = AppState::new(session.handle(), "test.plotyx".to_string()).unwrap();
            let context = egui::Context::default();
            context.set_fonts(plotyx_render_egui::font_definitions());
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
