# Application

## `stokesforge-app` — la coquille UI

**Ce que ce crate n'est pas** : une ébauche de l'interface finale. La grille
éditable à délégués, le panneau de propriétés et un système de graphes complet
(sélection multiple, réorganisation, fermeture persistée…) seront repris
entièrement plus tard ; rien ici ne doit les préempter.

Le banc est volontairement pauvre. Son rôle reste de prouver que
l'intégration bout en bout tient — modèle → commande → annulation → sauvegarde
→ rechargement, dans une vraie fenêtre — **tant qu'une intégration ratée ne
coûte qu'une réécriture de ce crate**, et non celle de tout ce qu'on aurait
bâti par-dessus. Deux fondations techniques sont posées au-dessus de ce
principe (voir plus bas) : la disposition est en **fenêtres flottantes
confinées** (`egui::Area`), et Save/Load passent par un vrai sélecteur de
fichiers natif (`rfd`) — ce ne sont que des charpentes, pas les panneaux
définitifs qu'elles accueilleront.

Toolkit : `eframe` / `egui` (disposition et rendu), `rfd` (sélecteur de
fichiers natif).

### Découpage du code : `app.rs` et `src/app/` (#115)

`app.rs` ne garde que l'**orchestration** de l'image ; chaque surface
d'interface est un vrai module de `src/app/` (chantier #115, #116–#123) :
imports explicites, visibilité minimale (`pub(super)` entre modules d'`app`),
aucun `use …::*` hors du code de test (garde `aucun_import_global_hors_des_tests`),
une doc de tête par module. Sens des dépendances : `app.rs` → modules de
surface → briques communes (`dialogs`, `tab_strip`, `list_row`, `window_chrome`,
`crate::icons`…). Les tests internes suivent le code qu'ils éprouvent : un
test vit dans le module de sa surface, et le module `tests` d'`app.rs`
n'éprouve que l'orchestration. Les gardes qui lisent le code parcourent tout
`src/`, sous-dossiers compris, et cherchent leurs fonctions par leur nom
(#116) : voir `docs/tests.md`.

| Fichier | Rôle |
|---|---|
| `app.rs` | orchestration : `StokesForgeApp` et ses champs, `new` (disposition initiale, restauration de l'état des fenêtres), `ui()` et l'**ordre des panneaux**, `collect_shortcuts` (raccourcis globaux, consommés avant les widgets), application différée des actions et ouvertures après la peinture, confirmations bloquantes (nouveau projet, suppression de feuille), drainage des fenêtres détachées, capture et auto-fermeture de vérification. Aucun module de `src/app/` ne lui emprunte d'aide : ce qu'ils partagent vit dans `tab.rs`, `window_detach.rs` ou `crate::color_picker` (#128) |
| `app/menus.rs` | barre de menu (`menu_bar` appelle `file_menu`, `edit_menu`, `workbook_menu`, `graph_menu`, `view_menu`, `window_menu`) et barre d'état (`status_bar`) |
| `app/analysis_menu.rs` | menu Analysis (#181) : `analysis_menu_contents` (barre de menus), `analysis_entry` (clic droit de colonne : une entrée `Analysis…`, #202), `calculation_section` (Edit / Recalculate / Detach / Delete sur une colonne calculée) |
| `app/analysis_dialog.rs` | fenêtre d'analyse (`modal_dialog_with`, deux colonnes : formulaire + aperçu) et confirmation de suppression d'un calcul (`calculation_delete_window`) |
| `app/analysis_forms.rs` | les formulaires par famille (`form_body`, `value_row`, tableau des paramètres) |
| `app/analysis_preview.rs` | l'aperçu peint à la main : outils Points / Zone / Zoom, aimant (Maj), moyenne (Maj + molette), bande des résidus, cache en mémoire temporaire d'egui |
| `app/results_pane.rs` | volet Results sous la grille d'un classeur : compteur, portée Feuille / Classeur, liste et tableau série × valeur |
| `app/format.rs` | barres de format de texte et d'objet, barre contextuelle : `format_target`, `text_format_state`, `object_format_state`, `tools_context`, `format_zone`, `format_text`, `format_object`, `run_toolbar_request` (les barres elles-mêmes : `crate::toolbar`) |
| `app/project_manager.rs` | Project Manager : `project_manager_panel` (panneau fixe) et l'arbre `project_manager_contents_from_tree`, commun au panneau et à la fenêtre détachée |
| `app/curve_settings.rs` | fenêtre des propriétés des séries (#177–#179) : liste, groupe, style commun, nuanciers (`curve_settings_page`) ; ouverte comme onglet `PlotSettingsTab::Series` de `plot_settings_window` |
| `app/object_manager_ui.rs` | Object Manager peint : `object_manager_panel`, `run_object_manager_request`, lignes, cases, menus, glisser des séries (le modèle : `crate::object_manager`) |
| `app/script_console.rs` | console de script : `script_panel`, lecture `ScriptConsoleModel`, contenu commun attaché/détaché, saisie, Exécuter/Arrêter, sortie |
| `app/tab.rs` | `Tab`, l'onglet d'une fenêtre de contenu (classeur, graphe, matrice, note), ses traductions vers le modèle (`tab_for_window`, `tab_node`), son titre (`tab_title`, `tab_title_from_tree`, nœud supprimé compris) et son miroir lisible par `crate::state` (`active_tab_kind`) |
| `app/contents.rs` | contenus partagés flottant/détaché : `tab_contents`, `note_contents`, `matrix_contents` (espace réservé) |
| `app/workbook.rs` | classeur : `book_contents` (contenu commun flottant/détaché), `sheet_tabs`, `workbook_menu` |
| `app/grid.rs` | grille d'une feuille : `grid_contents` prépare la table, peint ses en-têtes et son corps virtualisé, puis traite le clavier avec `grid_keyboard` (après les cellules : les éditeurs consomment d'abord Tab/Entrée et reprennent le focus), en-tête empilé, gouttière, cellules, `grid_edit_menu`, focus clavier (`GRID_KEYS`, `keyboard_focus_outside_grid`), barres déportées, `column_grip_bands` (prise du glisser de colonnes), libellés de type et de rôle |
| `app/figure_copy.rs` | copier la figure du graphique actif vers une application externe (#149) : le geste Ctrl+C (`request_figure_copy`, `take_figure_copy_gesture`), l'entrée de menu (`request_graph_figure_copy`) et l'offre au système (`offer_figure`) |
| `app/graph_targets.rs` | « Send to » : entrées du clic droit d'une sélection de colonnes et du menu Classeur qui ajoutent la sélection à un graphique ouvert, attaché ou détaché (#165) ; les graphiques ouverts y sont publiés dans le `Context` à chaque image |
| `app/graph_window.rs` | fenêtre de graphe : `graph_view_contents` (commun flottant/détaché), `graph_drop_target`, menu contextuel de la figure, `annotation_interactions`, ordre et propriétés des objets, `request_graph_export` |
| `app/graph_text_edit.rs` | éditeurs sur place (texte libre, titre, nom de légende) et mise en forme en attente demandée par la barre de format |
| `app/file_drop.rs` | dépôt de fichiers : `begin_file_drop`, `claim_file_drop`, `finish_file_drop` (un emplacement par viewport), `open_project_in_new_window` |
| `app/import_wizard.rs` | assistant d'import : `import_window`, ses six étapes ; choix du modèle/fichiers et réglages du format en sections, l'aperçu coloré par rôle, les libellés des choix |
| `app/plot_settings.rs` | propriétés du graphe : `plot_settings_window`, ses onglets, un éditeur par page, l'aide à la syntaxe des formules (`formula_field`, `formula_help`) ; `column_metadata_window` |
| `app/hover_help.rs` | brique commune de l'aide au survol d'un champ (#197) : `hover_help`, l'icône grisée à droite du champ dont la bulle montre le rappel |
| `app/dialogs.rs` | brique commune des dialogues modaux : `modal_dialog`, `modal_dialog_with` |
| `app/tab_strip.rs` | brique commune de la bande d'onglets : `tab_strip`, `tab_strip_with` (`TabOps`, `TabEvent`) |
| `app/list_row.rs` | brique commune de la ligne de liste sélectionnable (#169) : `list_row`, `ListRowBackground`, `list_row_label`, `list_row_text`, `LIST_ROW_MARK` ; Project Manager, Object Manager, liste des séries des propriétés |
| `app/shortcuts.rs` | constantes `SHORTCUT_*` et leurs rappels, consommation d'annuler/rétablir dans le viewport actif (`collect_undo_redo_shortcuts`) |
| `app/window_chrome.rs` | cadre commun des fenêtres et panneaux : `window_header`, `panel_header`, `floating_window_frame`, contour actif, poignées, `show_window_in_layer`, `show_window_contents_bounded` |
| `app/window_placement.rs` | placement : tailles par défaut, cascade, espace de travail fixe (`WORKSPACE_SIZE`), `clamp_to_workspace`, conversion logique → écran |
| `app/window_stack.rs` | pile des fenêtres flottantes : `open_tab` (seul endroit qui décide ouvrir ou focaliser), ordre et calques egui, masquage, repli, maximisation, `show_workspace` |
| `app/test_support.rs` | (compilé en test seulement) aides de test communes aux surfaces : `contexte`, `image`/`image_de`, `touche`, `cliquer`, `painted`, `textes`/`texte_exact`, `distincts`, `colonnes_xy` (#128) |
| `app/window_detach.rs` | détachement en fenêtres OS : `show_detached_project_manager`, `show_detached_object_manager`, `show_detached_script_console`, `detach_tab`/`reattach_tab`, `show_detached_tabs` et la publication de l'instantané qui réveille les viewports (#104, `snapshot_stamp` en est l'empreinte), `sync_window_views` (état des fenêtres envoyé à la session, #16) |

Une nouvelle surface volumineuse s'écrit dans son propre module, sur ce
modèle ; `app.rs` n'y gagne que l'appel. Depuis #124, `ui()` décrit les
étapes de l'image : travaux en fond, raccourcis, panneaux dans l'ordre ci-dessus,
espace de travail, dialogues, requêtes de fenêtres, confirmations et dispatch.
Les menus, la grille (en-têtes, clavier, corps et glisser), l'Object Manager
(en-têtes, calques, séries, axes et objets), les pages des propriétés et les
étapes fichiers/format de l'import sont découpés sur place. Le détachement
sépare publication, contenu et requêtes ; l'espace de travail sépare
maximisation, défilement et peinture d'une fenêtre ; la bande d'onglets sépare
menus, renommage, défilement et callbacks.

Une extraction conserve l'imbrication des `Ui`, les identifiants persistés et
l'ordre de consommation des événements. Les petits contextes privés de
peinture regroupent uniquement les données ou emprunts disjoints nécessaires
à une image ; ils n'ajoutent ni état métier ni sous-`Ui`. Les ouvertures et
fermetures restent appliquées après le parcours de la pile. Une redécoupe
future reste dans un commit séparé d'un déplacement.

### Analyse : fenêtre, menu, volet Results (#181)

L'interface du traitement mathématique est répartie ainsi :

- `src/analysis.rs` — **le modèle de formulaire** `AnalysisForm` (sans egui) :
  famille, réglages de chaque famille (tous conservés quand on change de
  famille), séries, auxiliaire (référence, réponse), `issues()` (ce qui bloque
  Apply), `operation_for(for_preview)`, `preview(tree)`, `draft(tree)` (noms
  longs des colonnes de sortie, M6), `from_calculation` (édition), les gestes
  de l'aperçu (`add_point`, `add_zone`, `remove_near`). Testé sans fenêtre
  (`tests/analysis.rs`).
- `src/state/analysis.rs` — l'état et les `Action` (`OpenAnalysis`,
  `EditCalculation`, `ApplyAnalysis`, `RecalculateCalculations`,
  `DetachCalculation`, `RequestDeleteCalculation`/`ConfirmDeleteCalculation`) ;
  Apply passe par `AddCalculation`/`UpdateCalculation` et garde la fenêtre
  ouverte sur erreur.
- Les surfaces de `src/app/` : `analysis_menu`, `analysis_dialog`,
  `analysis_forms`, `analysis_preview`, `results_pane` (table des modules plus haut).
- L'aperçu est peint à la main (pas de graphe du projet) et **calculé sur le fil
  de l'interface** : acceptable aux tailles actuelles, suivi en issue.
- Colonnes périmées : grisées dans la grille, lettre teintée (`theme::stale_color`)
  et info-bulle ; le volet Results (replié par défaut) liste les calculs
  conservés (portée Feuille / Classeur) avec Edit / Recalculate / Delete.
- Les erreurs viennent de `errors::analysis::error_message` (match exhaustif).
- Fenêtre d'ajustement (#204) : trois exponentielles distinctes (`Exponential`,
  `Exponential with Plateau`, `Two Exponentials`, mêmes noms que `pyst` :
  `exponential`, `exponential_offset`, `exponential2`) ; la formule du modèle
  est composée par Typst (`model_formula_source` →
  `stokesforge_render_egui::formula_label`, cache et polices gardés dans la
  mémoire d'egui) ; `Auto` remplit les valeurs de départ par
  `CompiledModel::guess` (pour le modèle libre, un balayage par paramètre qui
  suit l'échelle des données) ; une aire s'affiche avec le produit des unités de
  X et de Y quand les deux existent (`value_unit`) ; la fenêtre des pics borne
  les centres (`center_window`, aussi `pyst.analysis.peaks`).
- Le choix du type d'analyse se fait dans la fenêtre (liste « Analysis type »,
  `family_chooser`) tant qu'elle crée un calcul ; un calcul rouvert garde sa
  famille (#202).
- Lignes écartées et comptées (`skipped`) : X ou Y non fini, et, en ajustement
  pondéré, σ vide, nul ou négatif (#205) — jamais un échec de tout l'ajustement.
  Un paramètre que les données ne déterminent pas (colonne du jacobien nulle ou
  combinaison des précédentes) n'a pas d'incertitude et lève
  `Warning::Undetermined(n)` ; si aucun paramètre libre n'est déterminé,
  l'ajustement est refusé (`SingularSystem`, #203).

### Menus thématiques (#114)

Les menus du haut et les clics droits rangent les actions partagées sous
les mêmes noms. Les raccourcis et les barres d'outils gardent leurs actions.
Les cases restent réservées aux bascules d'affichage ; elles laissent la
chaîne de menus ouverte. Une action ou un choix exclusif (langue, type,
rôle, résolution) ferme toute la chaîne. `MenuBar` utilise
`CloseOnClickOutside` ; le fond de l'Object Manager et ses sous-menus de réglages
utilisent le même comportement. Les actions ferment explicitement
par `Ui::close`, dont egui propage la demande aux menus parents.

| Menu | Organisation |
|---|---|
| File | New → New Project, Add Workbook, Add Graph ; Import → Import…, Refresh from Source, Reopen Last Import… ; Open…, Save, Save As… ; Language ; Quit |
| Edit | Undo, Redo ; Selection → Copy, Paste, Rename, Duplicate, Delete des objets du Project Manager |
| Workbook (classeur actif) | Sheet → Add Worksheet, Rename, Duplicate, Delete ; Columns → Add Column, Type, Role ; Plot → Line, Scatter, Line + Symbol, Bar ; Show Metadata Row |
| Graph (graphique actif) | Show Data ; Add Text ; Selection → le menu de l'objet sélectionné (grisé sans sélection) ; Reset Text Positions ; Top / Right Mirror Axis (cases, calque actif ordinaire, #176) ; Reset Zoom ; Export → Export Graph…, Copy Graph, SVG Text, PNG Resolution ; Properties → Graph Properties…, Plot Area…, Axes…, Series…, Legend…, Theme… |
| View | Panels → Project Manager, Script Console ; Object Manager → Show, puis les réglages du fond (Show graph texts, Show Axes, Expand by Default, Reset Settings) ; Toolbars → File, Tools, Text format |
| Window | Maximize Window / Restore ; Detach / Reattach Project Manager |

La grammaire des sections, des séparateurs et des sous-menus est dans
[`docs/menus.md`](menus.md) (#152) ; chaque menu range ses entrées par
`MenuSections::section` (`app/menu_sections.rs`), qui ne pose un trait
qu'entre deux sections présentes. Les peintures partagées peignent le
**contenu** ; le menu du haut l'enveloppe dans un sous-menu qui nomme la
cible, le clic droit l'affiche à plat.

Clics droits : le Project Manager réutilise `object_menu` à plat (une fenêtre :
Show · Hide ─ Copy · Paste ─ Rename · Duplicate ─ Delete ; un dossier : Paste ─
Rename ─ Delete, le renommage s'éditant sur place). Un onglet de feuille
propose Add Worksheet ─ Rename · Duplicate ─ Delete, la bande vide Add
Worksheet. Les lettres de colonnes : Plot ▸ · Add Column ─ presse-papiers ─
Type ▸ · Role ▸ ─ Clear · Delete, au premier niveau (`column_properties_menu`),
la grille vide Add Column ; les cellules, lignes et champs d'en-tête un seul
groupe presse-papiers puis Clear · Delete, sans Undo/Redo.

Le fond de l'Object Manager affiche à plat les réglages de
`object_manager_settings_contents` (aussi sous View → Object Manager) ; ses
nœuds et la figure partagent `object_node_menu`, **le** menu d'un objet
(courbe : Show Data ─ Rename, Move Up/Down, Hide ─ Delete ─ Properties… ;
texte, axe, objet : voir `docs/menus.md`). Le clic droit de la figure ouvre ce
menu pour l'objet visé — une courbe est sélectionnée par le clic droit — et,
sur le fond ou la zone de tracé, Add Text, Insert Object ▸ (la forme est posée
au point cliqué, `GraphView::insert_shape_at`), Export ▸ et Properties… ; Reset
Zoom n'y figure plus. Les contenus et cibles sont identiques en fenêtres
flottantes et détachées.

Les filtres de l'arbre ne changent pas la visibilité des objets du graphique.
Les cases directement dans l'arbre restent des contrôles de visibilité du
modèle, indépendants de ces filtres. L'inventaire des réglages globaux et les
questions de persistance pour la future fenêtre Preferences sont suivis dans
l'issue #148 ; cette refonte n'ajoute pas cette fenêtre.

### Fenêtres flottantes confinées

Le Project Manager, l’Object Manager et la console Python sont des panneaux
fixes à gauche, à droite et en bas (#100). Leur visibilité se commande par
View ; aucun en-tête ne porte de bouton Fermer. L'ordre des
panneaux fait la mise en page egui (le premier posé est le plus extérieur) ;
il est décidé dans `StokesForgeApp::ui`, et nulle part ailleurs : barre de
menu (`Panel::top`, `app/menus.rs`), barres d'outils (`toolbar.rs`), barre
d'état (`Panel::bottom`, `app/menus.rs`), console de script (`Panel::bottom`,
`app/script_console.rs`, posée avant le Project Manager pour courir sous lui
sur toute la largeur), Project Manager (`Panel::left`,
`app/project_manager.rs`, sauf détaché), Object Manager (`Panel::right`,
`app/object_manager_ui.rs`, sauf détaché), puis le `CentralPanel`. Le test
`les_panneaux_fixes_se_posent_dans_l_ordre_de_ui` (`app.rs`) tient cet ordre
par les rectangles des panneaux. Les quatre autres
natures de fenêtre de contenu — `Tab::Book` (classeur), `Tab::Graph`,
`Tab::Matrix`, `Tab::Note`, chacune portant un `NodeId` — s'ouvrent par défaut
en fenêtres flottantes à l'intérieur du `CentralPanel`, une par
`StokesForgeApp::floating_tabs` (ordre de pile : `app/window_stack.rs`). Chacune est une `egui::Area` nue
(`show_floating_window`), pas une `egui::Window` : l'application possède seule
sa géométrie, et le cadre, le contour actif, le rognage, les poignées et la
géométrie persistée viennent du même rectangle. Les `egui::Window` d'avant
avaient toutes leurs fonctions coupées puis refaites à la main, et chaque
image devait accorder le rectangle demandé avec celui qu'egui recalait —
d'où les bugs de contour décalé et de débordement sur les panneaux
(retours du 2026-09-24, points 1 et 2 — rapport supprimé, dans l'historique git).

L'aire qui les porte a une taille logique **fixe** (`WORKSPACE_SIZE`,
1920×1080), indépendante de la taille de la fenêtre principale : en dessous,
elle défile (`StokesForgeApp::workspace_scroll`) plutôt que d'écraser les fenêtres
qu'elle contient contre ses bords. `window_positions`/`window_sizes` et la
géométrie persistée dans le projet sont exprimées dans ces coordonnées
logiques, bornées à l'espace de travail par `clamp_to_workspace` (la seule
borne), puis converties en coordonnées écran à la peinture. Le rendu est
rogné à la portion visible de l'espace de travail (hors panneaux fixes et
barres de défilement), pour le dessin comme pour les clics : une fenêtre à
cheval sur le bord de la vue ne peint ni ne capte rien sous le Project
Manager ou la console. Entièrement hors de la vue, elle n'est pas peinte du
tout, et redéfiler la fait réapparaître à sa position logique inchangée. Rouvrir depuis le Project Manager une entrée déjà ouverte
mais défilée hors vue fait défiler l'espace de travail jusqu'à elle. Une
fenêtre nouvellement ouverte apparaît en cascade par rapport à la précédente
plutôt que superposée à elle. La taille fixe est pour l'instant une constante
du code, pas encore réglable par l'utilisateur (persistance, UI de réglage) —
à faire.

Un onglet par fenêtre **ouverte**, quelle que soit sa nature : `StokesForgeApp::
open_tab` est le seul endroit qui décide « ouvrir » vs « focaliser » (cliquer
une seconde fois la même entrée du Project Manager ne duplique jamais son
onglet, elle est ramenée au premier plan). Un onglet `Tab::Graph` par graphe
ouvert garde son propre `stokesforge_render_egui::GraphView` (donc son propre
cache de formes — `StokesForgeApp::graph_views` est une table indexée par
`NodeId`). Son contenu réserve la hauteur des commandes sous la figure : le
rectangle rendu reste stable au survol, donc le
cache ne se reconstruit pas et la fenêtre ne dérive pas en hauteur. Une
nouvelle fenêtre de graphe part assez grande pour contenir la figure de
référence (480×360), ces contrôles et son en-tête ; si l'utilisateur la
rétrécit, la figure conserve sa taille et sa géométrie. Elle reste centrée,
et ses parties hors de la zone allouée sont tronquées par le découpage
d'egui, pour le dessin comme pour les interactions. Elle ne recouvre donc
ni l'en-tête ni les commandes placées sous elle. Les compteurs de cache et
le résumé technique de la figure ne sont plus affichés.

La barre de titre maison centre le nom du contenu ; il n'est rogné que
symétriquement, par la plus large des réserves réellement occupées — flèche de
repli à gauche, icônes peintes à droite (un dialogue ne réserve que la croix,
#50). Celle d'un graphe garde à
gauche la flèche de repli d'une fenêtre egui : elle masque ou réaffiche le
seul contenu de ce graphe, sans modifier le projet, la géométrie enregistrée
ni les autres fenêtres.

Chaque fenêtre flottante peut aussi être maximisée (plein cadre du
`CentralPanel`, `StokesForgeApp::maximized_tab`) ou détachée dans une vraie fenêtre
OS séparée (`StokesForgeApp::detach_tab`/`reattach_tab`, pour le multi-écran).

Le menu File propose la création d'un nouveau classeur (Add Workbook) et l'import de
fichiers. La position et
la taille des fenêtres de contenu sont enregistrées dans le projet (voir
« Géométrie des fenêtres »). L'éditeur de matrice reste un espace réservé.

### Nouveau projet et graphique vide (#73)

**File › New › New Project** (« Fichier › Nouveau projet », Ctrl+N) remplace le
projet courant dans la même fenêtre. La barre Fichier offre le même geste.
Un projet contenant du travail non enregistré, y compris une saisie locale
dans une fenêtre détachée ou un texte du graphique, demande confirmation ; Annuler,
Échap ou la croix conservent le projet. Le repère est conservateur : toute
révision depuis l'enregistrement, y compris une annulation, ou une géométrie
modifiée demande confirmation. Il ne conserve aucun ancien arbre en mémoire.
Un nouveau projet encore vide ne demande pas de confirmation.

Le remplacement passe par `SessionHandle::new_project`, vide l'historique et
les géométries et change l'époque pour invalider les anciennes références
Python. Les fenêtres de contenu, caches et sélections du projet précédent
sont oubliés, et ses fenêtres détachées se ferment. La visibilité des barres
et la langue sont conservées. Le geste est grisé pendant un script, un import
en cours ou une macro ouverte. Le premier **Save** (« Enregistrer ») demande
une destination, puis les suivants utilisent ce fichier.

**File › New › Add Graph** (« Fichier › Ajouter un graphique ») et la barre Fichier
créent un graphique vide à la racine : une zone de tracé, aucune série ni titre.
Sa fenêtre s'ouvre au premier plan. Il passe par la commande `AddGraph` existante :
Annuler le retire, Rétablir retrouve la même identité, et le `.stf` le conserve.
Ce geste n'a pas de raccourci supplémentaire ; il reste accessible au menu.

### Glisser-déposer de fichiers

Déposer des fichiers sur la fenêtre principale émet `Action::DropFiles`. Le
classeur flottant sous le curseur réclame le dépôt en se peignant
(`app::file_drop::claim_file_drop`) : il reçoit une feuille par fichier, sinon un nouveau
classeur est créé. Les `.stf` ne sont jamais importés : `AppState` les met en
file (`take_projects_to_open`) et `app.rs` lance un second processus
(`file_drop::open_project_in_new_window`)
`stokesforge-app <chemin>`. Chaque fichier utilise le modèle qui le reconnaît,
sinon le dernier modèle utilisé dans cette session, sinon la recette permissive.
Il passe par la conversion et l'application communes à l'assistant — une
seule entrée « Importer » ; un fichier qui ne donne aucune valeur numérique
ouvre à la place la fenêtre d'import pré-remplie. Décision pure :
`state::file_drop_plan`. Le dépôt crée toujours de nouvelles feuilles, même
si la recette du dernier import remplaçait ou fusionnait une feuille. Le lien
« Reopen Last Import… » dans la barre d'état et File › Import › Reopen Last
Import… reprennent ses fichiers dans l'assistant.

Le même geste est câblé dans les fenêtres détachées : sur un classeur,
la destination reste ce classeur ; sur un graphe, une note, une matrice ou le
Project Manager, un nouveau classeur. Les dépôts sont isolés par viewport et
leurs actions reviennent à la session principale. Un dépôt est refusé tant
que les propriétés du graphe sont ouvertes, pour garder leur macro
d'historique indépendante de l'import. Les événements de dépôt sont
testés sans fenêtre. Le dépôt réel depuis Fichiers en Wayland natif est
confirmé par l'utilisateur le 2026-10-02 ; les fenêtres OS détachées et
les autres cas du protocole ci-dessous restent à valider manuellement.

### Assistant d'import

« Importer… » ouvre un assistant en six étapes façon Origin (`import_window`,
`ImportStep` ; module `src/app/import_wizard.rs`, #118, seul point d'entrée
appelé par `app.rs`), **dialogue modal de la brique commune** (`modal_dialog_with` :
même cadre, même barre de titre, même blocage du reste de l'interface que les
propriétés du graphe ; *Finish* = bouton par défaut, *Back* et *Next* dans le
pied de page) : fichiers et modèle, format, en-tête et fin des données,
colonnes, métadonnées, destination. Tous ses textes passent par `tr!` (clés
`import-*` du catalogue `en`, rédigées selon `docs/textes-interface.md` § 2.4 :
*Delimiter*, *Template*, *Subheader*…) ; les noms de modèles et le contenu du
fichier restent des données, jamais traduites. L'aperçu du fichier reste à droite à
toutes les étapes ; un clic droit sur une ligne règle l'import à cet endroit
(`AppState::import_line_action`). *Finish* importe dès l'étape 1 avec le
modèle tel quel. Les octets des fichiers sont lus une fois (cache par chemin,
`ImportState::bytes`) ; chaque fichier montre avant l'import ses lignes lues ou
son erreur. L'en-tête est un seul réglage (`ImportRecipe::unify_header` ramène
`HeaderSkip::Lines` au découpage structuré) ; les rôles de colonne choisis à
l'étape 4 vivent dans la recette (`column_roles`). La section *Column names*
est grisée et inactive quand *Names in first line* est coché : les noms
viennent alors de cette ligne. Sinon, les choix *Filename* et *Header key*
sont actifs ; leurs valeurs sont conservées pendant le grisage. Un dépôt de fichiers pendant
que l'assistant est ouvert les ajoute à sa liste. *Finish* lance
`Action::StartImport` : l'analyse des fichiers tourne sur un fil de fond (avec
progression « Import en cours… k/n », `AppState::poll_import` appelé à chaque
image) et les feuilles se créent à la fin, en une seule entrée « Importer ».
Raccourci : **Ctrl+"** (`SHORTCUT_IMPORT` : touche du 3 sans Maj sur AZERTY,
`SHORTCUT_IMPORT_QWERTY` : Ctrl+Maj+' sur QWERTY). Fichiers récents et dossier
mémorisé : `stokesforge_import::RecentFiles` (`import_recent.json`, à côté des
recettes). « File › Import › Reopen Last Import… » (`Action::ReopenLastImport`)
rouvre l'assistant sur les fichiers et le modèle du dernier import — en
l'annulant d'abord s'il est encore au sommet de l'historique, jamais un travail
postérieur.

Le témoin « modifié » du modèle compare les réglages au modèle chargé ou au
dernier enregistrement réussi (`AppState::import_recipe_modified`). Enregistrer
le modèle retire ce témoin ; un échec d'écriture conserve la référence
précédente et signale l'erreur.

**Où vont les colonnes.** Dans le classeur actif, le mode d'import
(`ImportMode`) est : nouvelle feuille, remplacer la feuille active, ajouter des
colonnes, ou ajouter des lignes (la colonne *j* du fichier prolonge la colonne
*j* de la feuille). Plusieurs fichiers se répartissent selon `FilesLayout` : une
feuille par fichier, une seule feuille côte à côte (X Y X Y…), ou une seule
feuille avec le X du premier fichier partagé. Le plan (`plan_import`) est
calculé puis exécuté dans la même macro « Importer ». Par défaut, la première
colonne de chaque fichier reçoit le rôle X et les suivantes le rôle Y ; ces
rôles sont conservés lors du rétablissement de l'import après annulation.

**Fin des données.** La coupure automatique s'arme au premier X numérique,
puis coupe au premier X non numérique (ligne vide comprise). Le texte et les
lignes vides qui précèdent ce premier nombre sont conservés ; un texte dans Y
ne coupe pas. Le préfixe `skip_chars` est retiré avant ce test ; avec le
séparateur espaces, X est le premier jeton non vide. L'étape « En-tête et fin
des données » propose aussi « Aucune coupure », première ligne vide, marqueur,
N lignes après l'en-tête ou N lignes avant la fin. Le clic droit sur l'aperçu
permet d'annuler la coupure automatique ; l'aperçu et l'import utilisent le
même découpage (`stokesforge_import::layout`, `data_end`).

**Plusieurs blocs.** `BlockSplit` découpe un fichier qui enchaîne plusieurs
spectres (à chaque ligne commençant par un texte comme `TITLE`, ou aux lignes
vides) : `parse_blocks` rend un tableau par bloc, chacun avec son en-tête, et
l'import crée une feuille par bloc (`fichier_1`, `fichier_2`…) ou les répartit
selon `FilesLayout`. L'aperçu montre le premier bloc et le nombre de blocs.

**Actualiser.** Chaque colonne importée garde sa source
(`ColumnMeta::import_source` : chemin, recette en JSON, index de colonne).
« File › Import › Refresh from Source » (`Action::RefreshFromSource`)
relit les fichiers et réécrit les valeurs **en place** : les colonnes gardent
leur identité, les graphes restent valides. Les lignes de la feuille suivent le
plus long des nouveaux contenus ; elles ne raccourcissent que si toutes les
colonnes sont actualisées.

### Glisser-déposer : le backend de fenêtre

Sous Linux, winit choisit désormais **Wayland natif par défaut** lorsqu'il
est disponible (#72). `STOKESFORGE_X11=1` force X11/XWayland en secours.
`STOKESFORGE_WAYLAND=1` n'est plus nécessaire. eframe/egui-winit restent en
0.36.2 ; les patches Cargo sélectionnent winit 0.30.13 et smithay-clipboard
0.7.3 dans `vendor/` en attendant le support amont stable.

Le module transitoire reçoit `text/uri-list` sur **le même récepteur que le
presse-papiers**. Ce worker est partagé par tous les viewports : créer un
second récepteur vole le dépôt au premier sur Mutter, comme l'a montré le
premier prototype. Un canal faible remet à winit les chemins et la position,
sans objet Wayland ni nouveau fil. Il émet les événements habituels pour chaque
fenêtre native, donc le chemin de dépôt de l'application et des fenêtres
détachées reste commun. Le pipe est lu sans attente bloquante, avec une liste
d'URI limitée à 1 Mio et un délai de 10 secondes. La position au lâcher reste
celle utilisée pour choisir le classeur. Voir
[`vendor/winit/STOKESFORGE.md`](../vendor/winit/STOKESFORGE.md) pour le choix
technique, les alternatives, le protocole manuel GNOME/KDE/wlroots et le
retrait lors du passage à winit 0.31.

Le défaut du premier prototype a été reproduit avec un dépôt réel depuis
Fichiers sur GNOME. Après correction, l'utilisateur confirme le fonctionnement
du dépôt en Wayland natif le 2026-10-02.
**Les fenêtres détachées et KDE/wlroots restent à valider manuellement** ;
les tests automatiques couvrent le parseur, le canal, sa durée de vie et la
décision d'import, pas le compositeur.

### Ligne « Metadata » de l'en-tête

Elle se masque et se réaffiche (Classeur › Afficher la ligne Metadata des
colonnes, ou clic droit sur son libellé) : réglage de vue gardé dans la mémoire
d'egui, partagé par les classeurs flottants et détachés, jamais dans le modèle.

### Débordement des fenêtres de contenu

Toutes les fenêtres de contenu flottantes et détachées partagent un rectangle
extérieur unique : leur contenu est peint dans un sous-espace borné qui ne
peut agrandir le cadre au-delà des poignées de redimensionnement. La règle
intérieure dépend de la nature du contenu. Dans un classeur, la grille et la
bande d'onglets de feuille défilent chacune horizontalement, indépendamment,
lorsqu'elles dépassent. Du haut vers le bas : grille, onglets de feuille,
barre horizontale de la grille ; la barre verticale occupe le bord droit sur
toute la hauteur. La bande d'onglets est retranchée du bas du classeur
avant la construction de la grille, qui ne peut donc plus la repousser hors du
cadre (`book_contents`) ; ce découpage n'utilise pas `egui::Panel`, qui
remplace le rognage de son parent au lieu de le combiner et dessinait les
onglets sous la vue d'une fenêtre coupée. Les onglets
gardent leur taille naturelle (`sheet_tabs`, enveloppe de `tab_strip`, `crates/stokesforge-app/src/app/workbook.rs`) —
ils ne rétrécissent pas et ne se replient pas dans un menu « … ». La bande n'a
**pas** de barre de défilement : quand des onglets sont cachés, deux boutons
« < » et « > » apparaissent à sa gauche et font apparaître l'onglet caché
suivant de ce côté (chacun grisé s'il n'y a rien à montrer de son côté) ; ils
disparaissent quand tout tient. Changer de feuille (au clic, au clavier ou
par script) fait défiler la bande **du plus petit décalage** qui montre
entièrement l'onglet sélectionné et ses deux voisins — cliquer sur le dernier
onglet visible ne révèle qu'un onglet de plus (`scroll_tab_strip_to_selected`,
issue #1). Aspect (#169) : onglets plats, sans cadre ; l'actif est écrit
en gras et dans l'accent et porte un trait d'accent du côté du contenu (`TabStripSide` :
sous le libellé pour les pages des propriétés, au-dessus pour les feuilles),
un filet gris sépare la bande du contenu. Les axes de la page « Axes » des
propriétés sont aussi un jeu d'onglets (`tab_strip`, `Top`).

#### Onglets, listes et sélection : quatre aspects, quatre rôles (#169)

| Rôle | Brique | Aspect |
|---|---|---|
| jeu d'onglets (feuilles, pages, axes) | `tab_strip` / `tab_strip_with` + `TabStripSide` | plat, actif en gras et accent, trait du côté du contenu |
| entrée de liste ou d'arbre (Project Manager, Object Manager, liste des séries des propriétés) | `app/list_row.rs` : `list_row`, ou `ListRowBackground` + `list_row_label` pour une ligne composée | ligne pleine largeur, fond pâle d'accent, filet d'accent à gauche, gras |
| choix exclusif dans un menu, une liste déroulante ou le sélecteur de couleur | `selectable_label` / `selectable_value` d'egui | fond de bouton sélectionné |
| bascule d'outil d'une barre | `Button::selected` (`toolbar::tool_button`) | bouton enfoncé |

Garde `aucun_bouton_selectionnable_hors_d_un_menu` (`tests/gardes.rs`) : un
bouton sélectionnable d'egui hors d'un menu, d'un `show_ui` ou d'une fonction
`*picker*`/`*popup*` fait échouer les tests.

Le gras est **toujours** `theme::bold` (axe `wght` de Noto Sans Variable,
graisse 600) : `RichText::strong` d'egui ne graisse rien, il est interdit
(`aucun_strong_d_egui_hors_du_theme`). Un titre de section de formulaire
(propriétés du graphe, assistant d'import) passe par `theme::section_title`
(espace de 6 pt puis gras) et sépare seul ses groupes, sans `ui.separator()`
juste avant (`aucun_separateur_avant_un_titre_de_section`) ; l'étape courante
de l'assistant d'import et l'élément sélectionné d'une liste sont aussi en
gras. Une police sans axe `wght` (paire Fira ou Libertinus, police système)
reste en graisse normale, sans erreur. Toute couleur d'état ou de chrome
vient de `theme.rs` (`accent`, `tab_rule_color`, `tab_hover_mark_color`,
`list_row_selected_fill`, `list_row_hover_fill`, `file_drop_outline_color`…)
ou des `Visuals` qu'il pose (`error_fg_color`, `warn_fg_color`,
`selection`) : aucun `Color32::from_…` ni teinte nommée hors de `theme.rs` et
`color_picker.rs` (`aucune_couleur_en_dur_hors_du_theme`). Les notes
et matrices restent bornées au rectangle commun. Dans un
graphe (`graph_view_contents`,
`crates/stokesforge-app/src/app/graph_window.rs`), la figure garde le ratio de
`DEFAULT_FIGURE_SIZE` (`stokesforge-render::pipeline`, 4:3) mais se met à l'échelle
« contain » dans l'espace disponible — maximisée en prenant la dimension
limitante, largeur ou hauteur, aussi bien en agrandissant qu'en rétrécissant
(F11). Aucun rognage ni défilement pour le graphe : la figure entière reste
toujours visible. C'est un **zoom** : la disposition (graduations, marges,
légende, mesures de texte) est toujours calculée à la taille de référence
`DEFAULT_FIGURE_SIZE` (480×360), puis le résultat entier, texte et épaisseurs
compris, est agrandi ou réduit uniformément (`GraphView::paint_scaled`,
`stokesforge-render-egui`). Redimensionner la fenêtre ne redispose donc rien, et
ne reconstruit pas le cache de formes. La règle est identique en fenêtre
flottante et détachée, qui partagent la même fonction.

Gestes sur une feuille (#106, retrait de F2 selon #130) : double-clic sur l'onglet ou menu
Renommer (édition en place, `AppState::renaming_sheet`, validée par Entrée ou
en quittant le champ, abandonnée par Échap) ; Ctrl+D ou menu Dupliquer (« … copie
N », premier numéro libre, la copie est affichée) ; Supprimer par le menu seul,
avec confirmation qui nomme les graphes qui en tracent des colonnes, grisé
sur la dernière feuille. Nom vide ou déjà pris : ancien nom conservé et message,
aucune entrée d'historique. Mêmes gestes dans le menu *Workbook* (miroir du clic
droit, `workbook_menu`) ; la bande les offre par `tab_strip_with`
(`TabOps`/`TabEvent`), qui ne touche jamais au modèle.

Pour une nouvelle nature de fenêtre, conserver cette borne commune puis
définir avec l'utilisateur quelles zones défilent et quelles zones sont
rognées avant de brancher son contenu.

Le `Frame` de ces fenêtres n'a pas de marge verticale
(`WINDOW_CONTENT_MARGIN`, `crates/stokesforge-app/src/app/window_chrome.rs`) : la bande d'en-tête
et les séparateurs du contenu fournissent déjà leur propre espacement, une
marge par défaut n'ajoutait qu'un bandeau blanc mort au-dessus et en dessous.
La marge horizontale reste. Toutes les barres de défilement suivent le
standard de `theme::scroll_style` (voir § Thème).

La barre horizontale de la grille est **déportée sous les onglets de
feuille**, comme dans un tableur : la zone de défilement de la grille
(`show_horizontally_scrollable_grid`) masque sa propre barre, et
`show_grid_horizontal_bar` pose dans la bande réservée en bas de la fenêtre
une `ScrollArea` horizontale vide qui la reflète ; un glisser sur cette barre
est appliqué à la grille à l'image suivante.

La barre verticale de la grille est **déportée** : la table défile elle-même
(ce qui fige son en-tête) mais sa barre est masquée, car dans la zone de
défilement horizontale elle suivrait la largeur des colonnes et sortirait de
la vue. `book_contents` réserve une bande au bord droit de la fenêtre, sur
toute sa hauteur (en-tête et onglets de feuille compris), et
`show_grid_vertical_bar` y pose une `ScrollArea` verticale vide qui reflète la
table, contenu et décalage mis à l'échelle de la barre
(`grid_vertical_bar_scale`) ; un glisser sur cette barre est appliqué à la
table à l'image suivante (`grid_contents`, `pending_vertical_offset`).

### Géométrie et état des fenêtres

Pour chaque fenêtre de contenu, la session retient (et le `.stokesforge`
enregistre) sa **géométrie flottante** — position et taille dans l'espace de
travail — et son **état** : ouverte, maximisée ou fermée, repliée, rang
d'empilement (`WindowGeometry::view`, voir `docs/persistance.md`). Les deux
partent ensemble à la fin de chaque image (`StokesForgeApp::sync_window_views`),
seulement quand ils changent. La géométrie flottante ne bouge jamais quand la
fenêtre est fermée, maximisée ou détachée : `StokesForgeApp::floating_rects` la
conserve et prime sur l'instantané, que `AppState::set_window_geometry` ne
rafraîchit pas.

Le **détachement ne se retient pas** : une fenêtre détachée est enregistrée
ouverte et revient flottante au rechargement ; la position et la taille de sa
fenêtre OS ne sont ni écrites ni relues. Au démarrage, `StokesForgeApp::new`
rétablit les fenêtres ouvertes (dans l'ordre d'empilement), la maximisée et
les repliées ; sans état enregistré, il ouvre le premier classeur et le
premier graphe.

### Project Manager et fenêtres par contenu

Le menu contextuel des fenêtres (à plat) et le menu *Edit* › Selection proposent
Copier, Coller, Renommer, Dupliquer et Supprimer (#105) ; un dossier propose
Coller, Renommer (sur place) et Supprimer (#157). Un clic sélectionne et ouvre l'objet ; Ctrl
bascule une cible sans ouvrir, Maj sélectionne une plage parmi les lignes visibles.
F2, Ctrl+D, Ctrl+C, Ctrl+V et Suppr s'appliquent à cette sélection quand le
Project Manager a le focus, aussi en fenêtre détachée. Les renommages multiples
saisissent les noms successivement, puis valident une seule entrée annulable ;
Échap abandonne l'ensemble. Le champ se valide par Entrée ou perte du focus.
Afficher et Masquer sont dans le menu contextuel d'une entrée et agissent sur
toute la sélection. L'arbre commence directement sous l'en-tête du panneau,
sans bandeau de compteur ni boutons intermédiaires.

La sélection utilise la forme arrondie des boutons, sans fond rectangulaire
ajouté autour des entrées (#144), y compris dans le Project Manager détaché.

La sélection simple suit les changements de fenêtre de contenu active (#146),
qu'elle soit flottante, maximisée ou détachée. Une sélection multiple est
conservée pendant les changements passifs et reste la cible des actions du panneau.
Un clic simple sur une entrée, ou un clic dans une fenêtre du workspace
(flottante ou maximisée), remplace la sélection multiple par la fenêtre visée
et réactive le suivi (#154), même si cette fenêtre est déjà active. Le survol,
le clic dans le vide et les panneaux recouvrant le contenu ne la remplacent pas.
Quand la fenêtre active reprend la sélection, elle devient aussi l'ancre de
Maj-clic dans le panneau fixe et détaché : la prochaine plage part donc de
cette fenêtre, et non de la dernière entrée cliquée dans Project Manager.
`follow_project_manager_active_tab` assure le suivi passif avant les menus ;
`window_stack` traite le clic explicite et demande la republication au Project
Manager détaché.

Les noms sont uniques dans le dossier parent. Les copies portent le premier
suffixe « copy N » / « copie N » libre et de nouveaux identifiants pour toute
leur descendance. Un graphe copié dans son projet garde ses références aux
colonnes originales. Un classeur dupliqué conserve son état ouvert ou fermé ;
les graphes qui l'utilisent ne sont pas dupliqués. Supprimer ferme les fenêtres
flottantes et détachées ; annuler restaure les objets sans rouvrir les fenêtres.
Les classeurs et les sélections multiples demandent confirmation, en nommant
les graphes utilisateurs ; une note, matrice ou graphe seul se supprime directement.

Copier capture le contenu au moment du geste, puis l'écrit dans le presse-papiers
système ; le collage lit celui-ci, sans relecture des objets source. Les classeurs,
notes et matrices se transfèrent entre projets et entre processus ; les graphes
entre projets emportent leurs colonnes dans un classeur caché (#107).
Coller dans le menu d'un dossier y place les copies. La grille refuse ce transport d'objets au lieu de le
transformer en cellules. `Action::MoveObjects` prépare le déplacement entre
futurs dossiers en un seul groupe annulable ; le geste Couper reste à câbler lors
de leur introduction.



Le Project Manager affiche l'arbre du projet **arrêté** à `Workbook`,
`Graph`, `Matrix`, `Note` : ces quatre natures sont des feuilles cliquables de
ce panneau, jamais développées davantage (pas de descente dans les feuilles
d'un classeur, pas de descente dans le contenu d'un graphe) ; les dossiers
sont des nœuds dépliables, respectant la hiérarchie réelle de l'arbre.
`AppState::project_tree` construit cette vue en généralisant le principe déjà
posé par `AppState::graphs`/`AppState::sheets` (interroger l'instantané, ne
jamais dupliquer l'accès à l'arbre) plutôt que de le réinventer : elle rend
une structure légère (`ProjectTreeNode`, dossiers et fenêtres typées avec leur
`NodeId` et leur nom), sans dépendance à egui.

Cliquer une fenêtre l'ouvre ou la focalise, pour les quatre natures :

- **`Tab::Book`** remplace l'ancien `Tab::Grid` unique et global : une fenêtre
  par classeur ouvert, avec une sous-navigation interne entre ses feuilles
  (`AppState::worksheets_of`, `AppState::selected_sheet_in` — une carte
  `Workbook -> Worksheet` plutôt qu'une sélection globale unique, pour que
  deux classeurs ouverts simultanément n'interfèrent jamais l'un avec
  l'autre). La grille elle-même (virtualisation, une seule cellule éditable à
  la fois, `MISSING_TEXT_DISPLAY`…) est inchangée dans son principe ; elle est
  seulement scopée à « la feuille actuellement choisie dans ce classeur »
  plutôt qu'à une sélection globale — `AppState::visible_columns` et
  `AppState::row_count` prennent désormais un `NodeId` de feuille explicite.
- **`Tab::Note`** est réellement éditable : `stokesforge-core` expose déjà le texte
  d'une note (`NodeKind::Note::text`) et une commande annulable
  (`Command::SetNoteText`). L'éditeur suit le même principe de saisie
  différée qu'une cellule de grille (`AppState::NoteEdit`, symétrique de
  `CellEdit` : le tampon n'atteint le modèle qu'à la perte de focus).
- **`Tab::Matrix`** est un **espace réservé documenté** : la fenêtre s'ouvre et
  se focalise comme les trois autres, mais ne montre qu'un message explicite
  — `stokesforge-core::NodeKind::Matrix` n'a aujourd'hui aucun contenu à éditer, et
  l'éditeur de matrice complet est explicitement hors périmètre de cette
  tâche.

### Données d’un graphique collé entre projets (#107)

Le geste Copier fige aussi les colonnes X, Y, erreur X et erreur Y référencées
par toutes les séries et tous les panneaux. Un collage dans un autre projet
crée un classeur caché distinct par graphique et par collage, sans fusion ;
les feuilles source sont conservées avec des noms distincts si nécessaire.
Les colonnes non utilisées ne sont pas emportées. Les colonnes calculées
transportent leurs résultats figés, sans leur formule : le nouvel agencement
des colonnes et les dépendances omises ne doivent pas recalculer ces valeurs. Un collage dans le même
projet conserve ses références d’origine et ne crée aucun classeur.

Graphique › Afficher les données (Show Data), également au clic droit du
graphique, révèle et ouvre les classeurs référencés. Le masquage des données
embarquées est une métadonnée persistée du classeur, distincte des fenêtres
masquées de l’interface ; la révélation est annulable. Un collage entier
s’annule en un geste. Une référence déjà rompue reste rompue, même si la
destination comporte une colonne de même identifiant.

Le texte du presse-papiers est limité à 64 Mio (hexadécimal compris) ; le
sérialiseur s’arrête avant le dépassement et l’erreur est affichée sans
remplacer le presse-papiers. Le collage d’un transport trop grand est refusé.

### Maximiser et détacher une fenêtre

Les fenêtres de contenu proposent dans leur en-tête des icônes pour
maximiser, détacher et fermer, accompagnées d'infobulles. Le classeur et le
graphe utilisent les mêmes commandes. La maximisation remplit le
`CentralPanel` ; le Project Manager et la console restent accessibles.
Restaurer rend à la fenêtre sa disposition flottante. Le menu Window
permet aussi de maximiser ou restaurer la fenêtre active.

### Connecteurs d'arbre en L (#112)

Les arbres du Project Manager et de l'Object Manager n'utilisent plus la ligne
d'indentation d'egui (qui descendait jusqu'en bas du corps d'un nœud). Ils
passent par un seul composant, `tree_lines.rs` : `begin_tree` coupe
`indent_has_left_vline` à la racine, chaque corps de nœud s'écrit dans
`tree_children`, et chaque enfant direct déclare le centre de sa rangée
(`TreeBranch::mark`). Le trait vertical va du haut du corps au dernier enfant
direct (`└`), chaque enfant reçoit un trait horizontal (`├`/`└`), et le trait
continue à travers le sous-arbre d'un enfant tant qu'un frère suit. Les
traits sont peints au `Painter` (couleur `noninteractive.bg_stroke`, alignés
sur le pixel). Un nœud qui contient un arbre doit renvoyer le centre de sa
rangée à son parent ; limite : le vertical part du haut du corps, pas du bas
de la flèche du parent.

### Panneaux rétractables et détachés (#100)

Les trois panneaux partagent `panel_header` (`app/window_chrome.rs`) : titre
compact, Détacher, Replier/Déployer. L'ordre des boutons dépend du côté :
Replier/Déployer vers l'espace de travail, Détacher vers le bord extérieur
pour les gestionnaires. La console garde Replier/Déployer puis Détacher
côte à côte à gauche quand elle est déployée (#142). Les trois panneaux
repliés ne montrent que Déployer : Détacher est masqué et sa place est rendue
au titre. Il faut déployer le panneau pour accéder à son bouton Détacher.
La bande repliée de la console porte une séparation supérieure sur toute la largeur, du même style que celles
des bandes des gestionnaires.
Les bandes verticales repliées gardent Déployer en haut et le titre dessous.
PM et OM se replient en une bande latérale avec titre vertical centré dans
l’espace disponible sous le bouton ; la console se replie en une barre
horizontale. Les trois bandes font 16 points d’épaisseur, comme leurs icônes,
centrées sans marge de panneau supplémentaire. Le panneau déployé reste
redimensionnable
(largeur de PM/OM, hauteur de la console). Les bandes de repli ont des
identifiants egui distincts : leur taille n’écrase pas la dimension déployée.
Le repli laisse le panneau visible ; View le masque ou le réaffiche. Le
masquage conserve également le repli et le détachement. La visibilité de
l’OM reste une préférence ; celles du PM et de la console sont locales à
l’application, comme leur repli. Le mode flottant interne de l’OM est retiré.

Détacher ouvre une vraie fenêtre système sans décoration native, avec le
même cadre maison et un seul bouton Rattacher. Son titre permet le glisser,
ses bords le redimensionnement. Une demande de fermeture du compositeur
est annulée puis traitée comme un rattachement ; elle ne masque pas le
panneau. Le menu Window garde aussi Detach / Reattach Project Manager.

Les fenêtres séparées utilisent `show_viewport_deferred` : leur rendu est
indépendant de celui de la fenêtre principale. Le callback échange un
instantané de lecture et des requêtes avec celle-ci, via deux
`Arc<Mutex<_>>`, sans déplacer `AppState` ni créer une seconde voie de
mutation métier. Les requêtes réveillent explicitement la fenêtre
principale pour être traitées même quand elle est inactive.
Dans l'autre sens, publier un instantané modifié réveille explicitement le
viewport lecteur (#104) : époque du projet, révision du modèle ou révision
des géométries. Le Project Manager est aussi réveillé si sa sélection, son
masquage ou l’onglet actif change. L’OM reçoit le graphe actif, sa sélection
et les filtres à chaque publication : il suit aussi un graphe détaché et
se réveille sans interaction dans sa propre fenêtre. Les gestes remontent
avec l’identifiant du graphe qui était peint. La console reçoit la saisie,
la sortie et l’état d’exécution (`ScriptConsoleModel`) ; les nouvelles frappes
sont drainées avant publication. Le moteur et le travail Python restent dans
l’`AppState` principal : détacher, rattacher, replier ou masquer la console
ne les interrompt pas. La racine sonde le résultat tant qu’un script tourne,
y compris quand la console est masquée. Une commande appliquée en fin d'image
programme l'image racine qui publiera son résultat ; aucun réveil périodique
n'est ajouté au repos. Les tests sans fenêtre vérifient la demande ciblée
et le retour au repos ; les tests transversaux des trois panneaux sont dans
`app/panel_tests.rs` (clics, tailles, callbacks différés, console sous Python).
Le redessin réel des fenêtres OS reste à vérifier manuellement.

### Détachement générique des onglets de contenu

`StokesForgeApp::detach_tab`/`reattach_tab`/`show_detached_tabs` généralisent le
mécanisme ci-dessus à `Tab::Book`, `Tab::Graph`, `Tab::Matrix` et `Tab::Note` :
l'icône de détachement ouvre une fenêtre OS ; celle-ci propose une icône de
réattachement. Fermer une fenêtre de contenu détachée ferme uniquement sa
vue, sans la réattacher ni supprimer les données du projet. Elle reste
réouvrable depuis le Project Manager. Plusieurs contenus peuvent être
détachés simultanément, chacun dans son propre `ViewportId`.

**Tailles par défaut des fenêtres détachées** (`show_detached_tabs`) :
`Tab::Graph` ouvre en 560×540, ce qui contient la figure de référence, ses
commandes et son en-tête. Le reste (`Book`/`Matrix`/`Note`) partage une taille
uniforme de 420×360. La grille du classeur conserve les mêmes largeurs et
en-têtes dans les fenêtres flottantes et détachées ; une fenêtre étroite
défile horizontalement. La note et la matrice gardent la même taille par
cohérence avec les autres contenus.

**Une seule structure de `ViewModel`, pas une par nature de contenu.** Le PM
n'a qu'une forme de contenu (un arbre en lecture seule) ; les quatre natures
détachables ici sont franchement différentes (grille éditable virtualisée,
graphe interactif avec cache de formes, texte éditable, espace réservé).
`DetachedTabViewModel` porte un champ par nature (`view_state` pour un
classeur ou une note, `graph_view` pour un graphe) plutôt que quatre types disjoints à faire cohabiter dans la table des
onglets détachés — les champs inutilisés pour la nature du `Tab` servi
restent à leur valeur par défaut.

**La lecture du modèle est factorisée, l'édition transitoire ne l'est pas.**
`tab_contents` (`app/contents.rs`, avec `note_contents` et `matrix_contents` ;
partagée par les fenêtres flottantes et la zone de maximisation)
prend `&mut AppState`, que la fermeture `'static`, `Send + Sync` de
`show_viewport_deferred` ne peut pas capturer — le même problème déjà résolu
pour le PM (mettre `AppState` entière derrière un verrou déborderait le
problème à résoudre, pour un onglet comme pour l'autre). Le contournement :
`DetachedTabViewModel::snapshot`, un clone de `Snapshot` (bon marché),
resynchronisé depuis `self.state` à chaque image avant l'appel — même schéma
que `ProjectManagerViewModel::tree`. Toute lecture pure passe par des
fonctions libres de `crate::state`, factorisées sur un `&Tree` pour
l'occasion (`cell_text_in`, `visible_columns_in`, `row_count_in`,
`worksheets_of_in`, `note_text_in`, `node_title_in`) : le corps exact de la
méthode `AppState` correspondante, pas une réécriture — le dock et une
fenêtre détachée affichent donc littéralement la même règle.

Le classeur détaché a un `AppState` de vue local, synchronisé avec l'instantané
de la racine. Il appelle les mêmes `book_contents`/`grid_contents` que le
classeur flottant ; disposition, en-tête, sélection, menus et raccourcis de
grille partagent ainsi le même code. Sa sélection et sa saisie restent locales
à la fenêtre. Avant de traiter ses actions, la racine reprend leur contexte
de grille, puis applique les mêmes commandes annulables. La note suit le même
motif : sa fenêtre détachée est une vue `AppState` locale (construite par
`fork_for_detached_view`) et appelle `note_contents`, comme la fenêtre
flottante ; seul le tampon de saisie (`note_editing`) est local à la fenêtre,
et la validation part en `Action::SetNoteText`, donc en commande annulable.
L'espace réservé de la matrice, `matrix_contents`, ne lit que l'arbre et sert
tel quel aux deux fenêtres : plus aucune fonction de contenu n'est propre au
détaché.

`Tab::Graph` échappe à cette limite : `GraphView` n'a pas de verrou global à
`AppState` à respecter (son cache de formes et sa navigation zoom/pan sont
déjà scopés par graphe, dans `StokesForgeApp::graph_views`) — `graph_view_contents`
est donc **littéralement** la même fonction pour une fenêtre flottante et une fenêtre
détachée ; la vue elle-même migre simplement hors de `graph_views` le temps
du détachement pour rester capturable par la fermeture sans dupliquer le
cache. `Tab::Matrix` n'a aucun état à faire voyager (espace réservé).

`Tab::Script` n'est pas détachable : sa saisie (`AppState::script_source`)
est un champ direct d'`AppState`, pas un tampon différé comme
`CellEdit`/`NoteEdit`, et son cycle de vie (fil détaché, `poll_script` à
appeler image après image) est bien plus couplé à `AppState` que les quatre
autres natures — le généraliser demanderait de faire voyager aussi l'état
d'exécution du script, hors périmètre de cette tâche.

### Barres d'outils (#59, #60)

`src/toolbar.rs`. Des barres de boutons-icônes, **fixes** (#108) : leur
contenu et leur place sont figés dans le logiciel, dans le panneau
`stokesforge_toolbars` sous la barre de menu. Ni poignée, ni glisser-déposer,
ni barre flottante ou verticale, ni logique de fenêtre étroite. Disposition en
dur (`ToolbarId::row`) : rangée 0 = Fichier, Outils ; rangée 1 = Texte, large,
seule sur sa rangée. La barre Object Manager est supprimée (#109) : le panneau
se commande par View et son en-tête. Chaque barre se
masque par le menu View ; seule cette **visibilité** est une préférence
(`preferences.json`), jamais une donnée du `.stf`. Un ancien champ `placement`
est ignoré à la lecture. Le `//!` du module dit pourquoi le panneau est un
`egui::Panel::top` (ni `Window` brute, ni `show_window_in_layer`).

- `ToolbarLayout` (pur, testé) : visibilité par barre et rangées visibles.
  `ToolbarState` n'est que la glu egui.
- Un bouton est un `ToolbarButton` : icône du composant commun, info-bulle
  (libellé + raccourci + `.hint`), grisage. Il pousse la même `Action` que le
  menu — jamais une seconde écriture. Le contenu de chaque barre est
  `toolbar::bar_contents` ; un geste qui n'est pas une `Action` (sélecteur de
  fichier, assistant d'import, export) remonte en `ToolbarRequest`, exécuté par
  `StokesForgeApp::run_toolbar_request` (`app/format.rs`) avec les méthodes
  mêmes des menus. Barre
  Fichier (#61, #73) : nouveau projet, classeur, graphique vide, import,
  ouvrir, enregistrer, enregistrer sous,
  exporter (grisé sans graphe au premier plan).
- Barre contextuelle `Tools` (#62), **toujours affichée** (#93) : son contenu
  suit la fenêtre au premier plan (`ToolsContext`, calculé par
  `StokesForgeApp::tools_context`, `app/format.rs`). Graphe : les outils Pointeur / Zoom / Texte
  (`GraphTool` de la vue, bouton enfoncé pour l'outil actif), réinitialiser le
  zoom, propriétés. Classeur : ajouter feuille, ajouter colonne, tracer la
  sélection (courbe, nuage, barres ; grisé sans colonne sélectionnée). Matrice,
  note, aucune fenêtre : les outils du graphe restent là, **grisés**, avec
  l'info-bulle `toolbar-tools.disabled` (la barre ne change pas de forme).
  Après un séparateur, elle porte le **format de l'objet** sélectionné (#64,
  fusionné avec elle par #93 ; `object_format_contents`), grisé sans objet
  sélectionné. Elle se masque encore par le menu View.
- Barre `Text` (format du texte, #63), **toujours affichée** (#108) sur sa
  propre rangée : sans graphe au premier plan ses boutons sont grisés, avec
  l'info-bulle `toolbar-tools.disabled` (comme `Tools`, #93). Police, corps, couleur, gras,
  italique, indice, exposant, formule. Les textes du graphe étant déjà du
  markup Typst, gras, italique, indice, exposant, formule, couleur et corps
  d'un passage **écrivent des balises dans le texte** (`text_format::apply`,
  pur). La **police** n'est plus une balise (#78) : c'est un champ de la zone
  entière (`TextFont`, `AnnotationSpec::font`), posé par `StokesForgeApp::format_zone`
  (`app/format.rs`) — brouillon de l'éditeur pour un texte libre en édition
  (`pending_zone_font_id`), `Action::SetTextFont` pour un titre ou la légende,
  `Action::SetAnnotation` pour un texte libre sélectionné. Un titre, un titre
  d'axe ou la légende **sélectionné** (`FormatTarget::Zone`, #77) ne prend que
  la police, le corps et la **couleur de la zone entière** (`Action::SetTextColor`, #90 ;
  les autres boutons sont grisés). Cible (`FormatTarget`, `StokesForgeApp::format_target`) : un champ en
  édition sur place (texte libre, titre, titre d'axe, légende) → la demande
  attend dans la mémoire d'egui (`pending_format_id`) et le champ la pose à
  son prochain tracé sur le surlignage qu'egui a gardé, ou insère les balises
  au curseur (`apply_pending_format`) ; sinon un texte libre sélectionné →
  tout le texte, en **une** `Action::SetAnnotation` (couleur et corps sont les
  champs du modèle) ; sinon les boutons sont grisés. Gras, italique, indice,
  exposant et formule se défont en les reposant. Un clic sur la barre ne ferme
  pas l'éditeur (`toolbar::over_format_bar`). Le formateur ne pose pas
  d'alignement : les textes tiennent sur une ligne.
  Les listes affichent la police et le corps de la zone sélectionnée (#97),
  ou ceux du brouillon pendant l'édition, y compris en fenêtre détachée.
  Un corps hérité affiche sa taille effective issue du pipeline partagé ;
  l'entrée « Auto » est alors cochée. La choisir retire le corps explicite
  d'un titre ou de la légende, et rétablit le corps par défaut d'un texte libre.
  L'entrée « Graph font » est cochée pour une police héritée, tandis que le
  champ affiche le nom de la police du graphe. « Auto » réutilise la clé
  commune `common-auto` des deux langues.
  Un réglage local de police ou de corps dans le markup affiche le libellé
  neutre « Font » ou « Size » : la barre résume la zone sans interpréter les
  expressions Typst calculées. Le parseur distingue ces réglages du texte
  brut, des commentaires et des blocs de code brut.
  Le sélecteur affiche aussi la couleur de cette zone (#99), jamais le dernier
  choix fait sur une autre zone. Un passage portant une couleur locale donne
  un échantillon neutre. En édition, le brouillon du sélecteur dure seulement
  jusqu'à la validation du geste, qui pose une seule balise sur le passage.
- Visibilité : menu **View › Toolbars**, une case par barre. Emplacement et visibilité
  sont dans `preferences.json` (champ `toolbars`), pas dans le `.stf` ; la
  lecture ignore ce qu'elle ne comprend pas. `StokesForgeApp::new` ne lit pas le
  fichier (les tests), `main` appelle `load_toolbar_layout`.
- Les barres ne sont peintes que dans la fenêtre principale, pas dans les
  fenêtres OS détachées.

### Thème

L'appli **ne suit pas** le thème clair/sombre du système : `stokesforge_app::theme`
pose, une seule fois au démarrage (`main.rs`, dans la fermeture de création
d'`eframe::run_native`, aux côtés du réglage des polices), une palette maison
dérivée de `egui::Visuals::light()` — jamais du sombre. `Context::set_theme`
verrouille la préférence sur `Theme::Light` (empêche egui de re-suivre le
système à la moindre notification `winit`), puis `Context::set_visuals`
applique la palette (`theme::visuals`) : accent vert forêt (sélection,
liens, contours des widgets actifs/survolés), coins légèrement moins arrondis
que le défaut egui. Le reste (gris de fond, contrastes de texte) vient tel
quel du thème clair d'egui, déjà calibré pour la lisibilité.

Deux raisons à ce choix, documentées en tête de `theme.rs` : `stokesforge-render`
dessine les tracés sur un fond clair implicite (axes, grilles, texte noir) —
un thème sombre romprait ce contraste sans qu'aucun code de rendu n'en soit
informé ; et un thème qui change avec le système est un thème qu'on ne peut
pas garantir en capture d'écran ni en support.

**Barres de défilement : un seul standard.** Toutes les barres de
l'application (espace de travail, grille, panneaux, fenêtres d'import et de
script, fenêtres détachées) ont le même aspect : 10 points d'épaisseur,
poignée gris foncé atténuée, qui passe à la couleur de survol puis à l'accent
pendant le glisser. `theme::scroll_style` le définit, `theme::install` le pose
une fois sur le style global d'egui ; changer les constantes `SCROLL_BAR_*` de
`theme.rs` change toutes les barres d'un coup. Règle pour le code futur : ne
jamais construire ni modifier un `ScrollStyle` hors de `theme.rs` ; une zone
qui doit réserver la place d'une barre la lit par
`theme::scroll_bar_allocated_width`. Le test `aucun_scroll_style_hors_du_theme`
fait échouer la CI sinon.

**Icônes.** Phosphor duotone, un seul composant (`icons.rs` : `icon_image`,
`icon_button`, taille unique, info-bulle obligatoire), couleur « neutre par
défaut, rare ensuite » : règles et marche à suivre dans [`icones.md`](icones.md).
Le test `aucune_icone_hors_du_composant` interdit tout chargement d'icône
ailleurs.

### Interface multilingue (Fluent)

L'interface s'écrit **d'abord en anglais** ; le français en est une traduction
(chantier #39). Seule l'application traduit : Fluent (`i18n-embed`,
`i18n-embed-fl`, `rust-embed`, plus `fluent-syntax` pour la pseudo-langue) est
une dépendance de `stokesforge-app` et d'aucun autre crate — `core`, `commands`,
`session` et les autres rendent des données structurées que l'application met
en phrase (#46, #47). Le pourquoi du choix de Fluent (clé et arguments vérifiés
à la compilation) est en tête de `src/i18n.rs`.

| Où | Quoi |
|---|---|
| `crates/stokesforge-app/i18n/en/stokesforge_app.ftl` | le catalogue de référence ; une clé absente d'ici n'existe pas |
| `crates/stokesforge-app/i18n/fr/stokesforge_app.ftl` | la traduction française, complète (#49) : même ensemble de clés que `en`, même commit |
| `crates/stokesforge-app/i18n.toml` | `fallback_language = "en"`, dossier des catalogues |
| `src/i18n.rs` | chargeur, langue courante, macro `tr!` ; `src/i18n/pseudo.rs` : la pseudo-langue |
| `src/preferences.rs` | `preferences.json`, même dossier de configuration que `couleurs.json` |
| `src/history_text.rs` | les libellés d'historique (`HistoryLabel`) en phrases, clés `history-*` (#46) |
| `tests/libelles.rs` | la garde `aucun_libelle_en_dur` (absolue : aucune dérogation) |

**`tr!` est l'unique point d'entrée** : `tr!("menu-file")`, ou avec arguments
nommés `tr!("history-insert-rows", count = n)`. Il enveloppe `fl!` ; une clé ou un
argument inconnu de `en` ne compile pas. Aucun littéral dans un libellé. Pour l'infobulle d'un contrôle (attribut `.hint` de sa clé) : `tr!("project-manager-show", "hint")`. Un test
qui compare une langue précise passe son chargeur : `tr!(in loader; "clé")`
(`build_loader`), pour ne pas dépendre de `STOKESFORGE_LANG` ; un test qui cherche un texte peint le demande à `tr!`, jamais un littéral.

**Menus et actions (#42)** : une action a **un** libellé, la clé `action-*`, partagé
par la barre de menus, les menus contextuels, la barre d'état et le bouton de
fenêtre détachée (`action-reopen-last-import`, `action-reset-zoom`…) ; les
menus eux-mêmes sont `menu-*`. L'infobulle (l'effet, pas le raccourci, que le
menu dessine déjà) est l'attribut `.hint` (`tr!("action-save", "hint")`) ;
l'infobulle d'une entrée grisée, qui dit pourquoi, est `.disabled`. Les noms
de touches (`shortcut-backspace`, `shortcut-delete`) sont aussi des clés. 
La barre de menus et la barre d'état sont `StokesForgeApp::menu_bar` et
`StokesForgeApp::status_bar` (`app/menus.rs`, #122) ; leurs tests ouvrent
chaque menu de haut niveau pour de vrai et lisent libellés, grisage et cases
à cocher dans l'arbre d'accessibilité qu'egui publie (AccessKit), sans
supposer de position.

**Langue** : la variable `STOKESFORGE_LANG` (`en`, `fr`, `en-XA`), puis la
préférence `language` de `preferences.json`, sinon l'anglais. La langue du
système n'est **pas** suivie (décision du 2026-10-01). Une valeur inconnue ou
un fichier corrompu passe au niveau suivant, sans panique (invariant 10).
Menu **Fichier › Langue** : change la langue tout de suite (`set_language`, les
textes se redessinent à l'image suivante) et l'écrit dans `preferences.json` ;
`STOKESFORGE_LANG` l'emporte encore au prochain démarrage.

**Pseudo-langue `en-XA`** : fabriquée à la volée depuis `en` (accents, `[[ ]]`,
+30 %), sans fichier. `STOKESFORGE_LANG=en-XA` : un texte sans `[[ ]]` n'est pas passé
par le catalogue, un `]]` manquant signale un texte tronqué.

**Garde `aucun_libelle_en_dur`** (`tests/libelles.rs`) : aucun littéral textuel
dans `label`, `button`, `menu_button`, `checkbox`, `radio_value`,
`selectable_value`, `selectable_label`, `on_hover_text`, `heading`, ni dans les
`Button::new(…)`/`RichText::new(…)`, ni dans les tables `fn label()`/`fn title()`.
Elle est **absolue** : aucune dérogation (les 213 littéraux d'avant #41 ont été
convertis, #42 à #45, et la liste supprimée à #48). Elle voit aussi `strong`,
`weak`, `small`, `monospace`, `colored_label`, et ignore les champs de format
(`{code}`). Les boîtes de fichier natives sont gardées aussi (#127) :
`set_title` pour le titre, `add_filter` pour le nom visible du filtre ; les
extensions et les sigles de format (`SVG`, `PDF`, `PNG`) restent techniques.
Les titres d'ouverture et d'enregistrement ainsi que le filtre du projet
passent par `tr!`, dans la langue courante au moment d'ouvrir la boîte.
La zone #43 (propriétés du graphe, éditeur de texte, pied des
dialogues, export) reste gardée plus strictement (`ZONE_43` : aucun littéral
textuel dans ses fonctions, quelle que soit la forme de l'appel). Une clé du
catalogue `en` qu'aucun source de `src/` n'emploie fait échouer
`aucune_cle_du_catalogue_n_est_morte`. Mécanique détaillée : skill
`stokesforge-textes-interface`.

**Erreurs affichées (#47).** Les crates du dessous rendent des erreurs
structurées dont le `Display` (`#[error(...)]`) est en **anglais brut** — ce que
lisent le CLI, `pyst` et les journaux — et que personne ne traduit. L'application
les met en phrase par `src/errors.rs` : une fonction `error_message` par
famille (`model` pour `CoreError`, `commands`, `session`, `file` pour
`PersistenceError`, `import`, `formula`, `render`, `export`, et `script` avec la
feature `python`), qui associe chaque variante à une clé `error-*` du catalogue
`en` avec ses champs en arguments. Le `match` est exhaustif, sans `_` : une
variante nouvelle ne compile pas tant qu'elle n'est pas associée (ou renvoyée
volontairement à son `Display` par `errors::fallback`). Le test de chaque
famille construit une valeur de chaque variante et vérifie que la phrase n'est
pas vide, sans jargon interne, sans point final. Toute erreur montrée à
l'utilisateur passe par ces fonctions, jamais par `to_string()` ni `{err}`.
Les champs `detail` (cause décrite par le crate d'origine) et les messages du
système restent en anglais dans la phrase. `stokesforge-importer` rend un
`ImportFailure` structuré (fichier + cause) pour la même raison.

### Sélecteur de fichiers natif

Le bandeau de fichier a été supprimé : Ouvrir, Enregistrer et Enregistrer
sous sont réunis dans le menu File. Les clics et leurs raccourcis
partagent le même routage. Ouvrir (Ctrl+O) affiche un `rfd::FileDialog` ;
Enregistrer sous (Ctrl+Maj+S) demande une destination ; Enregistrer
(Ctrl+S) utilise la destination courante.

**Fil à part (#192).** Tous les sélecteurs (Open, Save As, export du graphe,
assistant d'import) passent par `Action::PickFile(PickRequest)` et
`file_picker.rs` : `rfd` s'exécute sur un fil dédié, la réponse revient par un
canal sondé à chaque image (`StokesForgeApp::poll_file_picker`). Avant, l'appel
synchrone sur le fil d'egui gelait l'application quand le sélecteur se fermait
sans que le portail ne réponde (`wait_for_response` de `rfd` n'a pas de délai) ;
désormais, au pire, la demande reste en attente. Un seul sélecteur suivi à la fois : une nouvelle demande remplace
l'ancienne, sinon un sélecteur fermé sans réponse du portail bloquerait
Ouvrir ensuite (le fil abandonné reste bloqué dans `rfd`, sans effet).
Garde : `aucun_selecteur_rfd_hors_du_module`. À valider sur un vrai bureau
GNOME/Wayland (clic sur l'application pendant le sélecteur) ; sur macOS, les
sélecteurs hors fil principal restent à vérifier. Le sélecteur n'est pas encore
attaché à la fenêtre (`set_parent`), pas nécessaire au dégel.

`rfd` (feature par défaut `xdg-portal` + `wayland`) compile et se lie sans
portail système présent : le portail (`xdg-desktop-portal`, D-Bus) n'est
qu'une dépendance d'exécution. Vérifié par une compilation `cargo build` et
`cargo build --release` réussies dans un bac à sable sans session graphique
garantie. L'ouverture réelle du sélecteur sur un vrai bureau Wayland reste à
valider interactivement — ce qu'aucun outil non interactif de ce dépôt ne
peut faire à la place d'un humain devant un écran.

### Deux modules, une ligne de partage

| Module | Contenu | Testable en CI |
|---|---|---|
| `state` | **toute la logique**, sans egui, pilotée par un enum d'`Action` | oui (`tests/shell.rs`) |
| `app` | la glu `eframe` : traduit clavier et souris en actions, peint l'état | non |

Une fenêtre ne se teste pas en intégration continue. C'est toute la raison du
découpage : **rien de ce que contient `app` n'est une décision de produit**.

`Action` : `Undo`, `Redo`, `Save`, `Load`, `SelectSheet`, `CommitCell`,
`AddWorkbook`, `AddColumn`, `InsertRows`, `DeleteRows`, `SetNoteText`, `RunScript`,
`InterruptScript`. `AddColumn`/`InsertRows`/`DeleteRows` portent désormais un
`NodeId` de feuille explicite (`sheet`) : depuis que la grille est scopée par
classeur (`Tab::Book`), il n'y a plus de « la » feuille affichée sur laquelle
une action implicite pourrait agir sans ambiguïté quand plusieurs classeurs
sont ouverts en même temps.

### Deux règles de conduite

1. **`AppState::dispatch` ne rend jamais d'erreur.** Toute erreur devient un
   `StatusMessage` affiché à l'utilisateur. Un banc d'essai qui meurt sur une
   saisie invalide ne prouve rien. Nuance portée par le type : « impossible
   pour l'instant » (annuler à vide) n'est **pas** une erreur ; un chemin
   illisible en est une.
2. **L'instantané détenu est figé** (l'arbre est persistant). Il faut le
   redemander à l'acteur après chaque mutation — ce que fait `refresh`, appelé
   par `dispatch` après toute action ayant pu changer le modèle.

### La console de script ne fige pas la fenêtre

`ScriptEngine::run` **bloque** jusqu'au terme du script. L'appeler depuis la
boucle de rendu figerait la fenêtre entière, bouton « Stop » compris — le
défaut le plus visible qu'une console de script puisse avoir.

La parade vit dans `state`, pas dans `app`. Trois décisions, prises là et
nulle part ailleurs :

1. **L'interpréteur ne démarre jamais à l'ouverture de la fenêtre.**
   `AppState::new` ne touche pas à `ScriptEngine` ; seul le premier
   `RunScript` le construit. Le temps de démarrage de l'interpréteur est l'un
   des motifs du portage — le perdre à l'ouverture annulerait le bénéfice.
   Verrouillé par `tests/script_bridge_lazy.rs`.
2. **`RunScript` ne bloque jamais l'appelant** : le travail part sur un **fil
   détaché** (création paresseuse du moteur comprise) et l'appel revient
   aussitôt.
3. **`poll_script` est à appeler à chaque image** : il sonde le résultat par
   `try_recv` et l'applique dès qu'il est prêt, rafraîchissement de l'instantané
   compris. C'est ce qui rend `dispatch` sûr à appeler depuis la boucle de
   rendu pour **toutes** les actions.

Sans la feature `python`, la console reste visible mais désactivée
(`AppState::script_available`). `ScriptOutput` est une copie **volontairement
indépendante** de `stokesforge_script::ScriptOutcome`, pour que la surface publique
de `state` soit la même avec et sans la feature.

Une exception de script n'est pas une erreur d'application : sa trace Python
complète s'affiche, sans jamais marquer le statut en erreur.

### Ce que Qt donnait gratuitement, et qu'il faut écrire ici

Une `QAction` reliée à un `QUndoStack` portait d'un coup le raccourci clavier,
l'entrée de menu, son grisage et son libellé (« Annuler *Renommer* »). Ici les
quatre sont séparés : le raccourci est déclaré et consommé à la main
(`SHORTCUT_UNDO` et compagnie), le grisage passe par `add_enabled`, le libellé
vient de `AppState::undo_label` — et **rien ne garantit leur cohérence sinon la
relecture**. C'est le point à vérifier en premier quand une action est ajoutée.

### Menus contextuels de la grille

Le classeur n'a plus de bandeau d'actions au-dessus de la grille. Un clic droit
dans l'espace vide à droite de la dernière colonne propose « Ajouter une
colonne » ; un clic droit sur un onglet de feuille propose « Ajouter une
feuille », « Renommer », « Dupliquer » et « Supprimer », sur la partie vide de
sa bande « Ajouter une feuille » seule — directement, sans sous-menu (#158). La bande reste sous la grille dans les
fenêtres flottantes et détachées. « Importer… » se trouve dans le menu File.
Son dialogue choisit entre le classeur actif, un nouveau classeur par fichier
et un seul nouveau classeur pour tout le lot. Chaque fichier valide crée sa
propre feuille ; les nouveaux classeurs ne sont créés qu'après analyse
réussie. Le lot est une seule entrée d'historique annulable.

Les cinq zones de la grille — cellule de données, champ d'en-tête d'une seule
colonne, rangée des lettres, gouttière des numéros de ligne, libellés de champ
entier dans cette même gouttière — portent le **même** bloc d'édition au clic
droit (`app::grid::grid_edit_menu`) : un groupe presse-papiers (Couper, Copier,
Copier avec l'en-tête, Coller, Coller à l'identique), puis Vider et Supprimer
et décaler. Undo et Redo n'y sont plus (Edit et le clavier, #158) ; chaque
entrée garde son équivalent au clavier, miroir du bloc de raccourcis de
`grid_contents`. La rangée des lettres y entoure ce bloc de ce qui lui est
propre : Tracer ▸ et Ajouter une colonne avant, Type ▸ et Rôle ▸ après le
presse-papiers (ils décrivent la colonne plutôt qu'ils n'agissent sur son
contenu).

Les entrées sont des verbes **nus** : « Copier », pas « Copier la cellule » ;
« Annuler », pas « Annuler Edit Cells in Column A ». Le clic droit vient de
se poser sur ce qu'il vise, et le raccourci est déjà rappelé à droite de
l'entrée — nommer la cible en plus allongeait chaque ligne sans rien
apprendre. `GridMenu::target_label`/`copy_full_columns_label` et
`AppState::undo_label`/`redo_label` restent **disponibles et testés** pour les
surfaces où l'on ne voit *pas* ce qu'on vise (la barre de menus les utilise
déjà pour Annuler/Rétablir, et une ligne de statut ou une confirmation en
auront besoin) ; ils ne sont simplement pas appelés par le menu contextuel.

La **décision** (quels gestes ont un sens ici, lesquels griser, comment
nommer la cible le jour où c'est utile) vit dans `AppState::grid_menu` →
`GridMenu`, pas dans le code egui : c'est ce qui la rend vérifiable sans
fenêtre dans `tests/shell.rs`, comme le reste de la matrice copier/coller.
Deux contextualisations s'y jouent :

- l'accord en nombre de `target_label` (« la cellule » / « les champs » /
  « les colonnes » / « les lignes »), avec deux pluriels distincts — celui des
  **cases** (`GridMenu::plural`) et celui des **colonnes**
  (`GridMenu::multi_column`) : trois cellules empilées sont plurielles sans que
  « Copier la colonne entière » doive passer au pluriel.
  `GridZone::Rows` (la gouttière) fait exception à `plural` : ses libellés
  parlent de **lignes**, pas de cases, donc une seule ligne sélectionnée reste
  « la ligne » même sur une feuille à plusieurs colonnes — `plural` n'y compte
  que le nombre de lignes couvertes, jamais `multi_column` ;
- « Supprimer et décaler » **n'apparaît pas dans l'en-tête** : un champ n'est
  pas répété par ligne, Suppr s'y contente de vider, et l'offrir ferait deux
  entrées pour un seul effet — vrai aussi bien pour un champ d'une seule
  colonne que pour un champ entier sur toutes les colonnes (`GridZone::
  Header` couvre les deux, voir plus bas). Il apparaît en revanche pour une
  plage de lignes entières : supprimer une ligne fait remonter ce qui suit
  dans toutes les colonnes, exactement comme un rectangle de cellules.

### Sélection multiple de la grille (#159)

`selection.rs` porte la sélection de chaque feuille — état d'interface pur,
hors historique. Elle est **d'une seule sorte** à la fois (colonnes, lignes
ou cases), et la frontière en-tête/données reste infranchissable ; chaque
sorte est un **ensemble** et non plus une plage `{ ancre, curseur }` :

| Sorte | Contenu | Gestes |
|---|---|---|
| colonnes (`Columns`) | `base` (colonnes posées une à une) ∪ plage `ancre..=curseur` | clic · Ctrl+clic bascule la colonne · Maj+clic remplace tout par la plage ancre → cible · Ctrl+Maj+clic et glisser ajoutent la plage |
| lignes (`FullRows`) | intervalles de lignes (même modèle) | mêmes gestes sur les numéros de ligne |
| cases (`Rows`) | rectangles déjà posés (`extra`) + dernier rectangle `ancre`–`curseur` | Ctrl+clic ajoute ou retire une case (le rectangle est découpé autour) · Ctrl+glisser ajoute un rectangle · Maj+clic étend le dernier rectangle |

Un Ctrl+clic d'une autre sorte, ou de l'autre côté de la frontière
en-tête/données, repose une sélection simple. Les champs d'en-tête entiers
(`HeaderRows`) restent des plages simples. Retirer le dernier élément lève la
sélection.

Les consommateurs raisonnent par **blocs** (`Selection::blocks`, un bloc =
colonnes de la feuille × plage de rangs) ; l'ordre est toujours celui de la
feuille (gauche → droite, haut → bas), jamais celui des clics.

- **Case du coin « # »** et **Ctrl+A** (grille focalisée, hors édition ;
  Edit › Select All) : `Action::SelectAll`, toutes les colonnes visibles,
  en-tête compris, même sans ligne ; un second clic sur la case, ou Échap dans
  la grille, lève la sélection. La case a une info-bulle et le curseur « main ».
- **Copier / Couper** : colonnes non contiguës bout à bout dans l'ordre de la
  feuille ; blocs de cases qui partagent leurs lignes (côte à côte) ou leurs
  colonnes (empilés) — sinon Copy et Cut sont grisés (`GridMenu::copy_enabled`,
  `Selection::copy_layout`). Copy with Header suit les colonnes touchées.
- **Coller / Coller à l'identique** : grisés sur une sélection non contiguë
  (`GridMenu::paste_enabled`, cible ambiguë) ; le collage au clavier est refusé
  avec un message.
- **Effacer** vide chaque bloc, en un seul groupe annulable. **Supprimer et
  remonter** n'est offert que pour un bloc unique (des trous rendraient la
  remontée ambiguë) ; Suppr se contente alors de vider.
- **Clic droit** : dans la sélection, le menu agit sur toute la sélection, même
  non contiguë ; hors de la sélection, il la remplace par la case visée (#152).
- **Tracer** reçoit la liste ordonnée des colonnes sélectionnées (A en X, C et
  E en Y…). Type ▸ et Role ▸ passent par `Action::SetColumnsType`/`SetColumnsRole` : un seul
  pas d'annulation (`MacroKind::SetColumnsType`/`SetColumnsRole`) pour toutes les colonnes.

Pendant un glisser, l'extension au survol ne s'applique qu'aux cases où
l'appui n'a **pas** commencé (`press_started_in`) : sinon un Ctrl+clic, qui
bascule au relâchement, aurait déjà étendu la sélection pendant l'appui.

### Tracer depuis un classeur

La rangée des lettres offre, en plus des types (Numeric, Mixed) et du
sous-menu « Role », un sous-menu « Plot » (Line, Scatter, Line + Symbol, Bar) qui crée un graphe à
partir de la sélection de colonnes courante — même cible que les deux autres
sous-menus (`target_columns` : toute la plage sélectionnée si la colonne
cliquée en fait partie, sinon cette colonne seule). Le graphe créé est
ajouté sans titre dans le dossier qui contient le classeur (`Command::AddGraph`,
pas d'entrée séparée pour la feuille) et s'ouvre aussitôt dans son propre onglet.

L'appariement des colonnes en séries suit la convention Origin plutôt qu'un
choix arbitraire : parmi les colonnes visées, chacune de rôle Y
(`ColumnRole::Y`, `ColumnMeta::role`) devient une série, appariée à la
colonne X la plus proche à sa **gauche** dans la feuille — en comptant aussi
les colonnes X non sélectionnées, puisque l'abscisse commune à plusieurs
courbes n'a généralement pas de raison d'être re-cliquée à chaque tracé. Une
colonne Y sans X à sa gauche garde une abscisse implicite (l'indice de
ligne) ; une colonne de tout autre rôle (X, Z, barre d'erreur, étiquette,
aucun) n'engendre jamais de série à elle seule. Une sélection sans aucune
colonne Y ne crée donc aucun graphe — un clic droit sur une plage de
colonnes purement X, par exemple, ne fait rien plutôt que d'ouvrir un graphe
vide. Chaque série prend la couleur de palette de son rang
(`SeriesSpec::from_palette`).

Cette règle vit dans `AppState::plot_columns`/`series_from_columns`
(`stokesforge-app/src/state.rs`), pas dans `app.rs` : le menu contextuel ne fait
que choisir le type de tracé et pousser `Action::PlotColumns`. Une seconde
action, `Action::AddSeriesToGraph`, applique la même règle d'appariement mais
relit le `PlotSpec` courant d'un graphe existant pour y **ajouter** des
séries (`Command::SetPlotSpec`) sans toucher à celles déjà présentes — elle
sert le glisser-déposer d'un classeur vers un graphe ouvert, décrit
ci-dessous.

### Glisser une sélection de colonnes vers un graphe

Second chemin vers `Action::AddSeriesToGraph`, complémentaire du sous-menu
« Plot » : on **attrape le bord** d'un bloc de colonnes sélectionnées, comme la
sélection d'Excel, et on le lâche sur un graphe ouvert (#160).

**Précondition.** Une sélection de **colonnes entières** (rangée des lettres,
case du coin, Ctrl+A ; non contiguë comprise, #159) :
`AppState::column_drag_payload` rend `None` pour des cellules, des lignes ou un
champ d'en-tête. Le geste emporte **toutes** les colonnes sélectionnées, dans
l'ordre de la feuille.

**Prise.** `app::grid::column_grip_bands` (fonction pure, testée seule) pose
une bande fine (`COLUMN_GRIP_HALF_WIDTH`, 3 pt de part et d'autre) sur chaque
bord extérieur d'un bloc de colonnes sélectionnées qui se suivent — deux
paires pour deux blocs séparés —, du premier champ d'en-tête au bas de la
grille. La **rangée des lettres en est exclue** : ses bords de colonne servent
au redimensionnement (`egui_extras`), qui garde la priorité. Les bandes sont
posées après la table (donc au-dessus des cellules) mais n'existent que sur ces
bords : le glisser d'extension de sélection de `cell_widget` garde toute la
surface des cellules. Curseur `Grab` au survol, `Grabbing` pendant le glisser ;
un repère (pictogramme du composant d'icônes + nombre de colonnes) suit le
pointeur ; Échap annule (`egui::DragAndDrop::clear_payload`).

**Charge utile.** `state::ColumnDragPayload { sheet, columns }` typée — pas une
chaîne —, portée par le mécanisme natif d'egui 0.36
(`egui::Response::dnd_set_drag_payload`, `egui::DragAndDrop`).

**Cible.** Chaque `Tab::Graph` est enrobé par `app::graph_window::graph_drop_target`
(appelée par `tab_contents`, seul point d'entrée commun au dock et aux
onglets détachés) : un survol avec une `ColumnDragPayload` active dessine un
contour sobre autour du graphe ; un relâchement au-dessus convertit la charge
en `Action::AddSeriesToGraph` via `state::drop_payload_action`. Relâcher
ailleurs ne fait rien. L'action porte le **calque** visé (`panel`) et
**n'impose pas de type de tracé** (`plot_type: None`) : les séries prennent
celui de la **dernière série** du calque, `Line` pour un calque vide
(`state::panel_plot_type`). Une seule commande annulable (`SetPlotSpec`).
Le calque visé est le **calque actif** du graphe (#161, ci-dessous) :
`graph_drop_target` et « Send to » lisent `published_active_panel`.

### Ligne x' et glisser de lignes vers un graphe (#164, #166)

**Ligne x'.** Cinquième champ de l'en-tête empilé, **au-dessus** du nom long
(`HeaderField::XPrime`, rang -5 ; `HeaderField::COUNT` donne le nombre de
champs, `rank`/`from_rank` la correspondance avec l'axe en-tête/données) : une
valeur **numérique** par colonne (`ColumnMeta::x_prime`), saisie comme les
autres champs — un texte vide l'efface, un texte qui n'est pas un nombre fini
est refusé (barre d'état, rien n'est écrit). `Action::SetColumnXPrime`, copier,
coller, vider et Tab suivent les autres champs ; la mutation reste
`Command::SetColumnMeta` (un pas d'annulation). **Import** : rôle de
sous-en-tête `x_prime` (`SubheaderRoles::x_prime`, ligne « x' » de l'assistant,
code `X'` de l'aperçu), lu avec le format numérique de la recette.
**Metadata** : la fenêtre des métadonnées d'une colonne propose, devant chaque
valeur numérique du fichier, « Utiliser comme x' » (le mini sélecteur : la clé
choisie alimente la ligne x' de cette colonne).

**Glisser de lignes.** Même geste que pour les colonnes, sur des **lignes
entières** (gouttière des numéros) : `AppState::row_drag_payload` ne l'offre que
si la feuille porte au moins une valeur x'. Prise par le bord haut ou bas d'un
bloc (`app::grid::row_grip_bands`, pure, les bords se lisent sur la sélection
et non sur les lignes peintes ; la gouttière reste libre), curseurs `Grab` /
`Grabbing`, repère « n lignes », Échap. Charge `state::RowDragPayload`, cible
`graph_drop_target` → `Action::AddRowsToGraph { axis: RowAxis::Ask }`. **Une
ligne = une série** (`SeriesSpec::row`), nommée « Row n » à la création. Axe :
un graphe **sans X** (aucune série à abscisse de colonne ni série ligne) reçoit
les séries sur l'axe X principal du calque actif, sans question ; un graphe
**avec X** pose le dialogue d'`StokesForgeApp::blocking_confirmations`
(`modal_dialog_with`) — « Create Axis » (défaut) ajoute un calque superposé à
axe X en haut (`OverlaySpec { x_top, y_right: false }`), « Use Present Axis »
garde le calque visé, « Cancel » abandonne (`Action::ConfirmRowDrop`). Une
seule `SetPlotSpec` : un seul pas d'annulation, calque compris. **Menus** : sur
une sélection de lignes entières d'une feuille à x' (clic droit sur un numéro de
ligne, menu Classeur), Plot crée un graphe de lignes (`Action::PlotRows`, mêmes
types de tracé) et Send to envoie les lignes (même question d'axe qu'au dépôt) ;
le menu des colonnes n'est plus proposé dans ce cas. Sans x', une sélection de
lignes garde le menu des colonnes qu'elles traversent.

**Calques superposés (#167).** Graph › New Layer (Top X, Right Y, Top X Right Y)
ajoute, par `Action::AddLayer`, un `PanelSpec::overlay_on` au calque de base du
calque actif ; les colonnes de rôle **Y2** (apparié au **X2** le plus proche à
sa gauche) partent, à la création du graphe (Plot) comme au dépôt ou à « Send
to », sur le second calque (axe X en haut, axe Y à droite, échelles propres),
réutilisé s'il existe. Le zoom, le déplacement et le rectangle de zoom portent sur les
**échelles** : le geste, exprimé en pixels de la zone de tracé (commune aux
calques d'une case), s'applique à toutes les échelles propres de tous les
calques de la case (X, Y et y2 du calque ordinaire ; X en haut / Y à droite
d'un calque superposé), si bien que les graduations restent alignées. Une
échelle liée n'a pas de cadrage à elle (`Framing` par échelle, `Gesture`,
`GraphView::commit_gesture`). **Un seul axe** : la molette (Ctrl) et le glisser
au-dessus d'un axe (ligne et graduations, pas son titre) ne cadrent que cette
échelle (`commit_axis_gesture`) ; la cible d'un glisser est figée à sa première
image (`NavDrag`), le curseur peut donc quitter l'axe. Les autres échelles, liées
ou non, ne bougent pas. Le rectangle de zoom reste un geste de zone de tracé. Rendu : `docs/rendu.md`.

**Calque actif (#161).** `GraphView::active_panel(spec)` : le dernier calque
sélectionné, état d'interface hors `.stf` et hors historique, porté par la vue
(donc identique en fenêtre détachée), revalidé à la lecture (calque disparu :
le premier). Il change par un clic, un clic droit ou un début de glisser dans
la zone de tracé d'un calque, par la sélection d'un objet du calque (figure ou
Object Manager, `follow_selection`), par un clic sur le nœud du calque, de son
groupe ou de ses axes dans l'Object Manager, et par le **repère numéroté** peint
en haut à gauche de la fenêtre (`layer_badges`, jamais dans l'export ni la
copie). La fenêtre publie le calque actif dans la mémoire d'egui
(`published_active_panel`) pour les menus qui n'ont pas la vue. Suivent le
calque actif : le dépôt de colonnes, « Send to » et Graph › Add Text ; les
gestes qui désignent un point (clic droit, outils) visent leur calque.

**Entre deux fenêtres système : pas de glisser (#165).** Le dépôt marche au sein
d'une même fenêtre OS (dock ou fenêtre détachée), pas de l'une à l'autre : sous
Wayland la fenêtre source garde le pointeur tant que le bouton est pris, le
graphique de l'autre fenêtre ne voit ni survol ni relâchement, et un vrai
glisser-déposer `wl_data_device` coûterait plusieurs jours. Décision : aucun
glisser inter-fenêtres, remplacé par **« Send to <graphique> »**
(`app/graph_targets.rs`), la même `Action::AddSeriesToGraph` que le dépôt, vers
**chaque graphique ouvert**, attaché ou détaché. Il se trouve à côté de « Plot »,
dans le clic droit des lettres de colonnes, dans celui de toute case d'une
sélection de **colonnes ou de lignes entières** (cellule, numéro de ligne,
champ d'en-tête, coin : même menu que les lettres, appliqué aux colonnes de la
sélection, `selection_context_menu`) et dans le menu Classeur. À partir de trois
graphiques, un sous-menu « Send to Graph » ; sans graphique ouvert, l'entrée est
absente.

### Propriétés des graphiques

Les réglages d'un graphe vivent dans une **fenêtre flottante à onglets**
(`app::plot_settings::plot_settings_window`), un **dialogue modal** (`modal_dialog`, `src/app/dialogs.rs`) :
au premier plan, le reste de l'interface ne reçoit plus aucun clic, et un
clic ailleurs fait clignoter son contour en rouge foncé
(`theme::blocked_flash_color`). Deux boutons en bas : **OK** (par défaut,
Entrée hors saisie de texte) et **Cancel** (aussi la croix et Échap) ; leurs
textes viennent du catalogue (`dialog-ok`, `dialog-cancel`). Les
gestes s'appliquent en direct au graphe (aperçu) mais dans une **macro
d'historique** ouverte à l'ouverture de la fenêtre : OK la ferme — une
seule entrée, un Ctrl+Z défait tout (`AppState::close_plot_settings`) ;
Cancel l'avorte et restaure le graphe d'avant
(`AppState::cancel_plot_settings`). Pendant la macro, annuler/rétablir et
enregistrer sont refusés (`stokesforge-commands`), ainsi que les écritures d'une
fenêtre détachée (autre identité d'émetteur : refusées par la session) et les
actions qu'elle remonte à la racine (`reject_detached_actions`). Toute
opération groupée se clôt par `AppState::end_grouped_macro`, qui rafraîchit
l'instantané (sinon `can_undo`/libellé restent périmés, issue #22). Elle est construite
par la même brique que les fenêtres de contenu (`show_window_in_layer`,
`window_header`, `floating_resize_handles`) — même cadre, même barre de
titre — et non par une `egui::Window` brute ; seuls l'ordre de calque et le
calque plein écran qui capte les clics la distinguent. Ce calque est
**invisible** : aucun voile sombre sur le reste de l'application, pour continuer
à voir le rendu du graphe pendant le réglage (issue #20). Ses onglets sont la
même bande que ceux des feuilles (`tab_strip`, issue #19) : c'est le standard
du projet pour tout jeu d'onglets. Les fenêtres OS
détachées, autres viewports, ne sont pas couvertes par ce blocage. Elle
n'occupe aucune place tant qu'elle est fermée.
Elle remplace l'ancien panneau fixe à droite de l'espace de travail, affiché
en permanence dès qu'un graphe était actif. Son état (quel graphe, quel
onglet, quel axe) est `state::PlotSettings`, dans `AppState` ; on l'ouvre
par `Action::OpenPlotSettings { graph, tab, axis }`.

**Double-clic sur la figure** (#113) : sur un axe, une courbe ou un objet
graphique, il ouvre leurs paramètres (`GraphEvent::OpenAxisProperties`,
`OpenSeriesProperties`, `OpenShapeProperties`, traduits par
`graph_window::annotation_interactions`) ; sur un texte, il garde l'édition sur place ;
dans la zone de tracé nue, il réinitialise le zoom. Dans l'Object Manager,
toute ligne (graphe, calque, groupes, séries, axes, objets) ouvre ses
paramètres au double-clic. Sur un graphe à plusieurs calques, le double-clic
sur un axe et son menu contextuel ouvrent l'onglet Axes du calque visé,
avec le même `Action::OpenObjectProperties` que l'Object Manager. L'adresse
est celle du calque dans la grille, indépendante du rang des panneaux tracés.

**Titres, titres d'axe et légende** (issue #53) : se désignent comme un texte
libre. Double-clic sur un titre ou un titre d'axe → éditeur sur place
(`app::graph_text_edit::text_editor_ui`, un champ ; Entrée ou clic ailleurs valide, Échap
annule) ; sur une entrée de légende → renomme la série (`SetLegendEntryName`,
vide = nom dérivé de la colonne) ; sur le cadre de la légende, rien (elle n'a
pas de texte à elle). Un glisser déplace le texte (ancré aux données : il suit
le zoom) ; un clic droit propose « Edit Text » et « Reset Position » (grisé tant
que le texte est à sa place), et « Graph » → « Text » → « Reset Text Positions » remet tout
d'un coup. Chaque geste validé est **une** `Action` (`SetTextAnchor`,
`ResetTextAnchors`, `SetTextContent`, `SetLegendEntryName`), donc un
`SetPlotSpec`, une entrée d'historique ; la position est enregistrée dans le
`.stf` et vaut à l'export.

**Textes libres** (issue #35) : clic droit dans une zone de tracé →
« Add Text » au point cliqué, ou « Graph » → « Text » → « Add Text »
(au centre du premier panneau). Le texte s'édite **sur place** : un champ posé
sur le texte, et au-dessus une barre couleur / taille / rotation
(`app::graph_text_edit::annotation_editor_ui`) ; Entrée ou clic ailleurs valide, Échap annule.
Le champ s'écrit **dans le cadre du rendu** (issue #37) : centré sur le centre
du texte rendu, à la taille de police du texte (corps × échelle d'affichage de
la figure), droit même si le texte est tourné — son placement est la fonction
pure `stokesforge_render_egui::annotation_field`. Un texte vidé n'étant plus tracé,
l'éditeur garde le dernier cadre connu. Tant qu'on tape, le champ a un léger
fond ; dès qu'il perd le focus (on règle la rotation ou la taille), il devient
transparent et l'aperçu **tourné** se voit (issue #36) ; un clic dans son cadre
redonne la main à la saisie. La taille finale diffère un peu après
recompilation par Typst (une formule `$…$` se compose autrement que sa source).
Double-clic ou clic droit → « Modifier le texte » rouvrent l'éditeur ; un clic
sélectionne (contour bleu), Suppr ou « Supprimer » le retire ; un glisser le
déplace. L'aperçu vit dans la vue (`GraphView::set_annotation_preview`) et
l'état de l'éditeur dans la mémoire d'egui, sous l'identifiant de la vue : même
écriture pour la fenêtre flottante et la fenêtre détachée, rien dans
`AppState`. Chaque geste validé est **une** `Action::SetAnnotation` ou
`RemoveAnnotation`, donc un `SetPlotSpec` ou `EditPlotSpec`, une entrée d'historique. Les couleurs
enregistrées sont publiées chaque image dans la mémoire d'egui
(`color_picker::publish_saved_colors`, lue par `color_picker::saved_colors_id`) pour que le sélecteur de l'éditeur les propose aussi
dans une fenêtre détachée.

**Objets graphiques** (issue #76) : quatre outils de la barre « Outils »
(`GraphTool::Rectangle`, `Ellipse`, `Line`, `Arrow`) ; un clic insère une
forme de taille par défaut (#153) : rectangle carré et ellipse circulaire de
48 × 48 px, ligne ou flèche horizontale de 80 px, en pixels de figure à partir
du point cliqué. Glisser trace librement l'objet (aperçu dans la vue) ; le
clic ou relâchement rend la main à l'outil Pointeur et sélectionne l'objet.
Avec le Pointeur : un clic sélectionne (contour et poignées),
glisser le corps déplace, glisser une poignée redimensionne, Suppr retire.
Maintenir Maj pendant le redimensionnement conserve les proportions à l'écran
au début du geste : le coin opposé reste fixe, une poignée de côté conserve
le côté opposé et le centre de l'autre dimension. Une ligne ou flèche garde
sa direction initiale. Sans Maj, les dimensions restent libres ; une forme
initialement plate reste libre faute de rapport largeur/hauteur défini.
Ces gestes sont communs aux graphes flottants et détachés. Menu
« Graph » et clic droit sur l'objet : « Bring to Front », « Send to Back »,
« Delete Object » (`graph_window::shape_order_buttons`). Chaque geste validé est **une**
`Action::SetShape` / `RemoveShape` / `ReorderShape`, donc un `SetPlotSpec` ou `EditPlotSpec`. La
partie « format d'objet » de la barre `Tools` (issue #64, fusionnée par #93 ;
`object_format.rs`) règle l'objet sélectionné : couleur de trait, couleur de remplissage (grisée pour
une ligne ou une flèche ; l'opacité est l'alpha du sélecteur, `#RRGGBBAA`),
épaisseur. Chaque réglage est un champ du modèle posé par une `Action::SetShape`
(`StokesForgeApp::format_object`, `app/format.rs`). Côté script : `pyst.add_shape` / `pyst.remove_shape` (`docs/python-et-script.md`).

**Exporter le graphe** (issue #34) : menu « Graph » → « Export » → « Export
Graph… », Ctrl+E (graphe actif seulement), ou clic droit sur la figure (y
compris dans une fenêtre détachée). Un sélecteur `rfd` (SVG, PDF, PNG) propose
le nom du graphe (`state::export_file_name`) ; le format suit l'extension,
`.svg` si elle manque. `app::graph_window::request_graph_export` lit le zoom de la vue
(`GraphView::framed_spec`) et pousse `Action::ExportGraph { path, spec }` ;
`AppState` écrit le fichier (`stokesforge-render-svg`, texte Typst compris) et le dit
dans la barre d'état. Aucune commande : le modèle n'est pas touché. La
résolution PNG (×1, ×2, ×4 ; ×4 par défaut, soit 1920 × 1440 px) se règle dans
« Graph » → « Export » → « PNG Resolution » (`Action::SetPngExportScale`,
préférence de session). Le texte des SVG exportés (#162) se règle dans
« Graph » → « Export » → « SVG Text » : **Texte** (défaut ; `<text>`
sélectionnable, police nommée mais non embarquée) ou **Contours** ; le choix est
retenu d'un lancement à l'autre (`preferences.json`, `Action::SetSvgTextMode`,
ni commande ni historique ni `.stf`) et ne vaut que pour le SVG.

**Copier le graphe vers une autre application** (issue #149) : menu « Graph »
→ « Export » → « Copy Graph », ou **Ctrl+C** quand une fenêtre de graphique est active au
premier plan (flottante ou détachée), qu'**aucun objet** n'y est sélectionné
(série, texte libre, forme, titre ou légende : `selected_manager_target`, la
sélection de l'Object Manager est celle de la vue) et qu'aucun champ de saisie
n'a le focus (`keyboard_focus_outside_grid`). Le presse-papiers reçoit la
**même figure en trois représentations**, offertes ensemble — SVG, PNG, PDF,
dans cet ordre de préférence ; c'est l'application destinataire qui choisit, et
beaucoup préfèrent le PNG. Le cadrage est celui de la vue affichée (zoom et
déplacement, `GraphView::framed_spec`), la taille, les polices et le fond ceux de
l'export vers un fichier, et le PNG suit « PNG Resolution ». Le Ctrl+C d'une
cellule, d'une feuille ou du Project Manager est inchangé : le Project Manager
retire lui-même son `Event::Copy` quand il a le focus, et `figure_copy` passe
avant les panneaux et les contenus, de sorte qu'une grille visible derrière la
fenêtre de graphique ne reçoit pas le même événement.

Chaîne : `app::figure_copy::request_figure_copy` (la racine, depuis `ui()`,
ou le viewport d'un graphique détaché) pousse `Action::CopyGraphFigure { spec }` ;
`AppState::copy_graph_figure` compose la figure **une fois**
(`stokesforge_render_svg::export_bytes_many`) et prépare les représentations
(`take_figure_clipboard`) ; la fenêtre racine les offre ensuite
(`StokesForgeApp::offer_figure`, qui seule connaît son affichage par
`eframe::Frame`) par `stokesforge-clipboard`. Un format qui échoue seul est omis
et nommé dans le message ; si aucun n'est prêt, ou si le système refuse, la barre
d'état donne la cause. Aucune commande : ni le modèle ni l'historique ne
bougent.

Pourquoi un crate à part : ni egui (`Context::copy_text`) ni `arboard` ne savent
offrir `image/svg+xml` ni `application/pdf`, ni plusieurs types à la fois.
`stokesforge-clipboard` porte un backend par affichage : **Wayland** par
`Clipboard::store_formats` du patch `vendor/smithay-clipboard` (le worker déjà
partagé avec egui-winit, voir `vendor/winit/STOKESFORGE.md`), **X11** par un
propriétaire de la sélection `CLIPBOARD` (`TARGETS`, un type par
représentation, pas d'`INCR` : une représentation plus grosse qu'une requête X
n'est pas offerte), **Windows** (un format enregistré par représentation :
`PNG`, `image/svg+xml`, `application/pdf`) et **macOS** (`NSPasteboard`, UTI
`public.svg-image`, `public.png`, `com.adobe.pdf`). Sous Wayland le
compositeur n'accepte la sélection que de la fenêtre focalisée qui vient de
recevoir une entrée : l'offre part donc du geste, et sinon elle est perdue sans
erreur, comme le texte d'egui-winit.

Vérifié : la composition et l'ordre des trois formats, le geste (sélection,
focus, onglet actif, événement consommé) et l'entrée de menu par des tests ;
X11 contre un vrai serveur (`cargo test -p stokesforge-clipboard --test x11 --
--ignored`, XWayland suffit). **Non exécutés par la CI ni ici** : Wayland réel,
Windows et macOS (ces deux derniers compilent seulement,
`cargo check -p stokesforge-clipboard --target x86_64-pc-windows-gnu` et
`aarch64-apple-darwin`). Contrôle manuel Wayland : lancer l'application,
sélectionner un graphique au premier plan sans rien y sélectionner, Ctrl+C, puis
`wl-paste -l` doit lister `image/svg+xml`, `image/png` et `application/pdf`
(`wl-paste -t image/png > figure.png` pour lire une représentation).

**Onglets** (`state::PlotSettingsTab`, clés `plot-tab-*`, en anglais) :
« General » (titre et sa couleur, fond), « Plot Area » (titre du calque,
quadrillage), « Axes » (un axe à la fois, choisi en tête de page : titre de
l'axe — grandeur et unités —, échelle, affichage, couleur de l'axe),
« Legend » et « Theme ». Les séries n'ont plus d'onglet (#179) : elles ont leur
propre fenêtre, **Series Properties**, décrite plus bas. Les textes de toute la fenêtre sont des clés `plot-*`
du catalogue `en` (infobulle en attribut `.hint`), selon le glossaire
`docs/textes-interface.md` ; les noms saisis (titres, noms de série, thèmes)
sont des données, jamais traduits.

**Textes du graphe** : titre, titre du panneau, libellé et unité d'axe, nom de
série sont composés par Typst et lus comme du **markup** (`docs/rendu.md`
§ Typst). Chacun de ces champs (`formula_field`) porte à sa droite une petite
icône d'aide grisée (#197, `app/hover_help.rs`) dont la bulle, au survol, rappelle
la syntaxe (`formula_help`, issue #33) : `$lambda$`, `$x_2$`, `$"cm"^(-1)$`,
`*bold*`, `\$` pour un dollar, et nomme Typst avec l'adresse de sa référence.
Elle remplace le mémo repliable « Formula syntax » : le lien avec le champ se
voit et l'aide ne prend plus de hauteur. Tout champ futur qui accepte une
formule passe par `formula_field` ; une autre aide au survol, par `hover_help`.
Le test `chaque_exemple_du_memo_compile` vérifie que chaque exemple compile. Un
nom de série laissé vide reprend celui de la colonne, affiché tel quel.

**Onglet « Thème »** : deux thèmes prédéfinis (`stokesforge_plot::THEMES` : Clair,
Sombre) et trois couleurs globales — fond, texte, axes. Une
couleur globale est **écrite dans tous les champs locaux** (titre du graphe,
titre et échelle de chaque axe de chaque panneau ;
`PlotSpec::set_text_color`/`set_axes_color`/`apply_theme`) : le dernier
réglage écrase ce qui avait été réglé une à une, et rien n'est mémorisé en
plus (le format `.stf` ne change pas). Le tout reste dans le brouillon de la
fenêtre : un seul pas d'historique à la validation. Les thèmes laissent le
texte et les axes en automatique, donc ils suivent le fond.

**Sélecteur de couleur** (`crate::color_picker`) : toute couleur de la
fenêtre se choisit par un bouton-échantillon (`color_field`/`color_button`)
qui ouvre une fenêtre surgissante **repliée** : palette, « Auto »
quand la couleur peut l'être, couleurs enregistrées et saisie hexadécimale
(`#RRGGBB`, ou `#RRGGBBAA` avec l'opacité). Le bouton **« + Custom »**
déplie le reste — Save Color et sa destination, nuancier (carré saturation ×
luminosité avec barre de teinte, ou **roue chromatique** avec barre de
valeur), **opacité**, saisie RGB et HSL. Le sélecteur se rouvre toujours
replié (`color_picker::custom_id`). Cette fenêtre est une `egui::Area` gérée
par `color_button` (et non un `egui::Popup`, dont egui n'autorise qu'un
exemplaire ouvert : la liste d'un menu déroulant le refermerait) ; elle
se ferme sur Échap ou sur un clic hors d'elle et de ses listes. Une couleur transparente se montre sur un
damier (dans l'échantillon du sélecteur). **Une palette par usage** (`stokesforge_plot::ColorUsage`) :

- **courbes** : un outil à part, avec un choix de palettes nommées
  (`stokesforge_plot::CURVE_PALETTES` : StokesForge, Tableau 10, Okabe-Ito lisible par
  les daltoniens, ColorBrewer Set1/Dark2/Paired, dégradés Viridis, Plasma,
  gris), chacune avec ses nuances foncées et claires. Les palettes vivent
  dans `stokesforge-plot`, pour qu'une évolution future colore plusieurs courbes
  d'un coup (palette ou dégradé) ;
- **texte** : encres lisibles ;
- **fonds** : en premier **« No color »** (`#ffffff00`,
  `stokesforge_plot::TRANSPARENT`), qui est aussi le **fond par défaut** d'un
  graphe neuf (`PlotSpec::default`) : la figure exportée n'a pas de fond,
  pour s'intégrer à une autre présentation ; à l'écran, elle se montre sur
  un fond **blanc** peint par l'application sous la figure
  (`app::graph_window::display_backdrop`), jamais exporté. Puis des fonds clairs, et des fonds
  sombres (le texte automatique passe en blanc, voir `docs/rendu.md`). Un
  fond vide (fichiers écrits avant ce défaut) reste blanc.

**Couleurs enregistrées** : « Enregistrer » garde la couleur courante
**dans le projet** (par défaut — commande annulable `SetSavedColors`,
enregistrée dans le `.stf`) ou **pour l'utilisateur** (fichier
`couleurs.json` du dossier de configuration de StokesForge, à côté des recettes
d'import, hors historique — `crate::user_colors`), au choix
(`state::SaveTarget`). Une couleur enregistrée ne réapparaît que dans la
palette de son usage ; clic droit dessus pour la retirer
(`Action::SaveColor`/`RemoveSavedColor`). Le sélecteur ne touche ni au
modèle ni au disque : il dépose des demandes dans un `PickerEnv`, que
`plot_settings_contents` traduit en actions.

La couleur est validée à la fin du geste (clic sur un échantillon, fin de
glisser, champ validé, fermeture), jamais pixel par pixel. Tout nouveau
réglage de couleur passe par ce sélecteur, avec l'usage qui lui correspond.

**Trois façons de l'ouvrir** :

- **clic droit sur un composant du graphe** (`app::graph_window::graph_context_menu`) :
  titre, zone de tracé, axe, titre d'axe, courbe, légende. Le menu est celui
  de l'objet visé (`object_node_menu`), dont « Properties… » ouvre la page qui
  règle ce composant (`state::PlotTarget::settings_page`) ; sur le fond et la
  zone de tracé : Add Text, Insert Object, Export, Properties…. Le composant
  sous le pointeur est trouvé par `stokesforge_render::hit::component_at`, sur la
  géométrie réellement allouée par Plotters (zone de tracé, aires de
  graduation, bande du titre), pas sur une reconstitution. Le clic droit
  **sans** glisser ouvre le menu ; le glisser secondaire reste le zoom
  élastique. Même chemin en fenêtre flottante et détachée
  (`graph_view_contents`) ;
- **menu « Graph »** de la barre de menus, présent seulement quand
  l'onglet actif est un graphe (`state::active_graph`, testée sans fenêtre) :
  un sous-menu « Properties » avec une entrée par onglet, et « Reset Zoom ». Aucun réglage n'est
  donc atteignable **seulement** au clic droit ;
- rouvrir la fenêtre sur un autre graphe valide d'abord le brouillon du
  précédent.

Fermer la fenêtre valide ce qui diverge encore du modèle
(`AppState::close_plot_settings`), par exemple un champ texte qui n'a pas
encore perdu le focus.

**Édite le `PlotSpec` au travers d'un brouillon, jamais directement.**
`state::PlotEdit { graph, spec, selected_series }` est un `PlotSpec` entier
tenu hors de l'historique jusqu'à validation — le même principe que
`NoteEdit`/`HeaderEdit` (saisie différée, tampon local, commande envoyée
seulement à la validation), généralisé à un objet entier plutôt qu'à un seul
champ : le panneau mélange une douzaine de petits widgets (titre, axes,
style de chaque série, légende) qui doivent tous voir et modifier le **même**
brouillon. `AppState::begin_plot_edit(graph)`, appelée à chaque image par
`app::plot_settings::plot_settings_window` avant de peindre quoi que ce soit, n'écrase le
brouillon que s'il n'y en a pas encore ou qu'il portait sur un autre graphe
— repeindre la même image sur le même graphe ne doit jamais effacer une
saisie en cours, et changer de graphe ne doit jamais laisser fuiter le
brouillon du précédent. Un brouillon **sans saisie en cours** (égal à
`PlotEdit::base`, le spec du modèle dont il est parti) suit en revanche le
modèle : après Annuler, Rétablir, un script ou un glisser-déposer de
colonne, la fenêtre montre le spec à jour au lieu de valeurs périmées
qu'une validation suivante réécrirait par-dessus l'annulation. `AppState::commit_plot_edit` envoie
`Action::SetPlotSpec { graph, spec }` → `Command::SetPlotSpec` — mais
seulement si le brouillon diverge du spec courant du modèle (comparaison
`PartialEq` sur `PlotSpec`, qui dérive proprement jusqu'aux feuilles) :
sans cette garde, relâcher un curseur qui n'a pas bougé ou perdre le focus
d'un champ texte non modifié empilerait une entrée d'historique identique à
la précédente.

**Stratégie anti-inflation d'historique**, trois familles de widgets liées
chacune à un déclencheur de validation différent, plutôt qu'un commit à
chaque frappe/pixel :

- **texte** (titres, libellés d'axe, couleurs en `#RRGGBB`) : `lost_focus()`
  — le champ est modifié en direct dans le brouillon à chaque frappe (retour
  visuel immédiat), mais `commit_plot_edit` n'est appelé qu'à la perte de
  focus, exactement comme `note_contents`/`cell_widget` ;
- **glissé numérique** (épaisseur de trait, taille de marqueur, bornes
  d'axe manuelles, `egui::DragValue`) : `drag_stopped() || lost_focus()` —
  la valeur change à chaque pixel de glisser dans le brouillon, mais ne
  s'envoie qu'au relâchement de la souris ou à la validation clavier ;
- **case à cocher / menu déroulant** (échelle, visibilité, grille, type de
  tracé, position de légende...) : un seul événement discret par clic,
  validé tout de suite — rien à débouncer, il n'y a par construction jamais
  de rafale d'événements `changed()` pour ces widgets-là.

Chaque widget de `app::plot_settings::plot_settings_contents` (et de ses fonctions auxiliaires
`axis_editor`/`series_editor`/`legend_editor`) accumule dans un simple
booléen local `commit`, mis à `true` par n'importe quel widget qui vient de
finir une interaction ; `commit_plot_edit` n'est appelé qu'une seule fois, à
la toute fin de la fonction, jamais widget par widget.

**Ce qu'édite la fenêtre**, en ne touchant que des champs qui existent
réellement dans `stokesforge-plot` (lu avant d'écrire, comme demandé) :

- **Graphe** : `PlotSpec::title`, `background` ;
- **Zone de tracé** — `PlotSpec::panels`[0] seulement, voir plus bas — :
  `PanelSpec::title`, grille des axes du bas et de gauche ;
- **Axes** : pour l'axe du bas, celui de gauche, et celui de droite si
  `PanelSpec::uses_right_axis()` (l'axe du haut, masqué par défaut et sans
  UI dans ce banc pour l'activer, n'a pas de section) : `AxisSpec::label`,
  `unit`, `scale` (`AxisScale::Linear`/`Log`), `visible`, `grid`,
  `inverted`, `auto_range`, et `range_min`/`range_max` quand `auto_range`
  est décoché ;
- **Séries** (#179 : fenêtre à part, `app/curve_settings.rs` ; `PlotSettingsTab::Series`
  n'est plus dans `PlotSettingsTab::ALL` mais choisit cette page dans
  `plot_settings_window` — même brouillon, autre titre `curve-window-title`, autre
  identité egui et autre taille ; ouverte par le double-clic sur une courbe de la
  figure ou sur sa ligne de l'Object Manager, `Properties…` et Graph › Properties ›
  Series…). Liste à gauche rangée comme l'Object Manager (#197, `PanelSpec::slots`) :
  une série seule est une ligne `list_row`, un groupe est un **nœud repliable**
  (réduire ne masque aucune courbe) dont les membres sont en branche
  `tree_lines` ; Move Up / Move Down changent le rang dans le groupe (ou sautent
  un groupe entier), réglages à droite : le **groupe** de la série
  (liste déroulante Aucun / nom du groupe / New group, et « Leave Group » qui la
  détache ; groupe créé à l'ajout simultané, #177 ; entrer dans un groupe place
  la série à la suite de ses membres), puis les gestes du groupe (`group_editor`) :
  son **nom** (champ validé à la perte de focus, vide = « Group n »), Move Group
  Up / Down, **Ungroup** (les séries restent, simples, avec leur style) et
  **Delete Group** (le groupe et ses séries) ; et, pour une série
  groupée, la **portée** (`EditScope`) en clair — « This series only » ou
  « Whole group (n series) » (par défaut) — avec une phrase qui dit ce qui
  s'applique. Portée groupe : style et épaisseur du trait, forme et taille du
  symbole se recopient dans chaque membre (`propagate_style`) ; les réglages
  propres à une série (nom, type, visibilité, légende, couleurs) sont **grisés**
  avec l'info-bulle `curve-scope-locked` ; le **nuancier** (#178) est actif.
  Portée série : seule la série bouge (c'est l'exception), le nuancier est
  grisé (info-bulle). Série sans groupe : tout est actif, nuancier grisé
  (« mettez la série dans un groupe »). Nuanciers : fournis
  (`GRADIENT_PRESETS`), enregistrés (projet ou utilisateur, suffixe « (Project) »
  / « (User) », bouton Delete grisé hors nuancier enregistré) ou personnalisé à
  N couleurs (nom + cible + Save), aperçu, « Apply to Group » qui répartit les
  couleurs dans l'ordre de la liste. Le choix en cours est un état de fenêtre ;
  seules les couleurs appliquées et les nuanciers enregistrés entrent dans le
  modèle. Dans l'Object Manager, le nœud sous le calque s'appelle « Series » ; un
  groupe y est un **nœud parent comme les autres** (#197) : triangle de repli
  (repli initial = celui de « Series », #139), case de visibilité mixte qui ne
  commande que ses membres (`VisibilityTarget::SeriesGroup`), nom éditable sur
  place (F2 ou Rename, Entrée valide, Échap annule), sélection (la `GraphView`
  garde `selected_group`, valable tant que la série qui le désigne est
  sélectionnée ; les poignées de toutes ses courbes sont peintes), double-clic
  = fenêtre des séries ouverte sur sa première série, clic droit
  (`object_node_menu` : Select Members, Rename, Move Up / Down, Ungroup, Hide,
  Delete, Properties…). Suppr supprime le groupe **et ses séries** (le geste
  distinct de la dissolution) ; Ctrl+↑/↓ le déplace en bloc. Actions :
  `RenameSeriesGroup`, `UngroupSeries`, `MoveSeriesGroup`, `MoveSeries`,
  `RemovePlotObject`.
  Ancienne description de la liste : (`egui::ScrollArea` bornée en hauteur,
  `selectable_label` par série, libellé `state::series_list_label` : le nom
  de la série s'il est renseigné, sinon le nom long de la colonne Y, sinon
  son nom de nœud, sinon « (référence rompue) » — le même calcul que
  `label_for` en privé dans `stokesforge-core/src/resolve.rs`, dupliqué plutôt
  que rendu public parce que c'est une règle d'affichage, pas une règle de
  résolution) — puis, pour la série choisie : `SeriesSpec::name`,
  `plot_type` (`PlotType`, toutes les variantes sauf `Unknown`, voir plus
  bas), `visible`, `show_in_legend`, `line` (`LineSpec::color`, `style`,
  `width`) et `symbol` (`SymbolSpec::shape`, `size`, `color`) ;
- **Légende** : `LegendSpec::visible`, `position`.

**`PlotType::Unknown` ne figure jamais dans le menu déroulant**, mais une
série qui le porte s'affiche quand même — le champ type de tracé devient un
texte en lecture seule (« type inconnu de cette version, non modifiable »),
le reste (nom, visibilité, style...) reste éditable normalement. C'est
l'invariant 10 (« un défaut de donnée dégrade, il ne plante pas ») appliqué
à cette fenêtre : proposer `Unknown` dans la liste laisserait croire qu'on peut
y revenir, alors que rien ne sait reconstruire une variante dont ce lecteur
ne connaît que le nom et des octets bruts (C5, niveau 2,
`guide-de-correction-robustesse.md` §F3).

**Ce qui n'est pas fait**, explicitement hors périmètre de ce chantier :

- **pas de largeur de barre ni de nombre de classes d'histogramme** —
  `SeriesSpec::bar_width`/`histogram_bins` existent dans `stokesforge-plot` mais
  n'ont pas de widget ici, laissé à une itération suivante ;
- **pas d'ajout/suppression de série ou de panneau depuis la fenêtre** —
  ça reste le rôle du sous-menu « Plot » et du glisser-déposer déjà
  décrits plus haut ;
- **zoom/pan n'y entrent pas, et ne doivent jamais y entrer** —
  `stokesforge_render_egui::ViewState` vit sur `GraphView`, hors du `PlotSpec`
  (`docs/invariants.md`, `.claude/rules.md`) : cette fenêtre n'a et n'aura
  aucun champ de navigation, une confusion entre les deux romprait la
  frontière rendu/modèle que tout le reste du chantier de rendu tient.

### Une colonne a deux identités, une seule est visible

Son **nom de nœud** (`Node::name` — « Temps », « Signal », ou le « A »/« B »
automatique de `Project::next_column_name`) est une clé du modèle : il résout
une colonne depuis un script (`pyst.resolve("[Book1]Sheet1!Temps")`), l'adresse
dans une spec de graphe, la retrouve dans un `.stf`. La grille ne l'affiche
**nulle part** : elle montre la lettre de **position** (A, B, C…) et les
quatre champs de `ColumnMeta` (Nom long, Unité, Commentaire, Formule), qui
sont encore autre chose.

`stokesforge_commands::Command::label` construisait ses libellés d'historique sur
le nom de nœud, et annonçait donc « Saisie dans Temps » pour une colonne que
l'utilisateur ne connaît que comme « A » — un mot introuvable à l'écran, et
carrément trompeur quand les deux divergent (un nœud nommé `T` en troisième
position s'affiche « C »). Le libellé désigne désormais la colonne par sa
position (`ColumnRef`, depuis #46 une donnée et non une phrase) : « Edit Cells
in Column A ». Les autres natures de nœud
(feuille, classeur, graphe, note) gardent leur nom réel — celui-là *est*
affiché, dans le Project Manager.

La correspondance position → lettre est `stokesforge_core::spreadsheet_name`,
rendue publique pour l'occasion : c'est la même fonction qui nomme les
colonnes créées sans nom explicite, qui peint l'en-tête de la grille
(`app::grid::column_letter` n'en est plus qu'un alias) et qui écrit les libellés
d'historique. Trois copies de cet algorithme valaient trois façons d'appeler
la même colonne.

La gouttière (`app::grid::row_number_widget`) est le miroir exact de la rangée des
lettres (`app::grid::column_header_widget`) : toute sa largeur sélectionne, au clic,
**toute la ligne** — les colonnes visibles de la feuille, à cette ligne, quelle
que soit leur longueur propre (une colonne plus courte que la ligne visée
reste touchée par la sélection, seule sa portion réellement écrite est
affectée par Vider/Supprimer). `AppState::select_row`/`extend_row_selection`/
`is_row_selected` en sont les pendants de `select_column`/
`extend_column_selection`/`is_column_selected`, et `SelectionKind::FullRows`
(`crate::selection`) le pendant de `SelectionKind::Columns` — même règle de
non-franchissement de frontière : un glisser qui part de la gouttière et croise
une cellule ou la rangée des lettres repose une sélection simple plutôt que
d'étendre silencieusement sans effet, et réciproquement.

### La gouttière côté en-tête : homogène, et elle aussi sélectionnable

Côté en-tête empilée (`app::grid::row_number_header_widget`), la gouttière portait
un « # » décoratif sur fond `header_letter_row_fill`, puis quatre rangées de
libellés (« Long Name », « Units », « Comments », « F(x)= ») restées
**transparentes** et inertes — la colonne de gauche n'était donc homogène ni
visuellement (le fond s'arrêtait au « # ») ni fonctionnellement (rien n'y
était cliquable). Les deux sont corrigés ensemble : chaque rangée de libellé
porte désormais le même fond que le « # » (ou le surlignage de sélection),
et un clic sur un libellé (`app::grid::header_row_label_widget`) sélectionne ce
champ pour **toutes** les colonnes visibles de la feuille — symétrique de
`row_number_widget` côté données. `AppState::select_header_row`/
`extend_header_row_selection`/`is_header_row_selected` en sont les pendants
de `select_row`/`extend_row_selection`/`is_row_selected`, et
`SelectionKind::HeaderRows` (`crate::selection`) le pendant de `FullRows`
pour la zone d'en-tête : elle touche toutes les colonnes et, symétriquement,
ne touche jamais les données (`row_range()` y vaut toujours `None`).

Ce n'est **pas** la même chose qu'un champ sélectionné sur une seule colonne
(`app::grid::header_field_widget`, `Selection::header_field`) : celui-ci reste
strictement à une colonne, glisser peut l'étendre à d'autres colonnes mais
jamais d'un seul geste à *toutes* les colonnes de la feuille — les deux
widgets, les deux constructeurs et les deux gestes restent distincts, même
si les deux finissent dans la même zone de menu (`GridZone::Header`) une fois
posés : `cleared_header_fields`/`copy_selection`/`paste` sont tous les trois
écrits en termes de `full_rank_range()`/`columns()`, qui rendent déjà la
bonne réponse pour les deux sortes de sélection sans code dédié
supplémentaire.

Le « # » lui-même sélectionne tout le tableau au clic (`AppState::
select_all`), comme la case en coin de la plupart des tableurs : une
sélection `SelectionKind::FullRows` couvrant `0..=row_count - 1` (`Selection::
all_rows`), qui touche déjà toutes les colonnes visibles sans qu'il faille de
sorte de sélection dédiée — `is_all_selected` en teste la couverture exacte
pour teindre le « # » en retour.

Les libellés de la gouttière restent sur une seule ligne, même en français,
en pseudo-langue ou après réduction de sa largeur (#125). Si la place
manque, ils sont tronqués avec des points de suspension ; le survol affiche
le texte complet. Ils ne débordent pas sur les rangées voisines.

### Une palette à trois paliers pour la gouttière, et une teinte propre à son texte

La gouttière (rangée des lettres, « # », numéros de ligne, libellés de champ)
utilisait le même vert que le reste de l'accent de marque, à pleine
saturation pour son texte (`Visuals::strong_text_color()`, qui vaut
`widgets.active.fg_stroke.color`, donc littéralement `ACCENT`) — trop
appuyé pour une zone censée rester en retrait des données. `theme::
gutter_text_color` lui donne une teinte propre, nettement plus sombre,
indépendante de la couleur globale des éléments « actifs » pour ne pas
déteindre sur des boutons ou onglets sans rapport.

Le fond, lui, gagne un troisième palier. `app::grid::gutter_fill(selected,
touched)` choisit entre `theme::header_letter_row_fill` (repos),
`header_letter_row_touched_fill` (la colonne ou la ligne d'une cellule de
données sélectionnée, sans être elle-même cliquée) et
`header_letter_row_selected_fill` (sélectionnée directement) — le repère
« ta ligne/colonne est concernée » d'un tableur, porté par `Selection::
touches_column`/`touches_row` (`AppState::column_touched_by_selection`/
`row_touched_by_selection`), écrites en termes de `columns()`/`row_range()`
donc valables pour **toute** sorte de sélection sans code dédié
supplémentaire. Les 3 rangées de la gouttière qui en profitent :
`column_header_widget`, `row_number_widget`, et le « # » de
`row_number_header_widget` (`touched` toujours faux pour lui — rien ne
« touche » le tableau entier sans le sélectionner en entier).

Enfin, `ui.spacing_mut().item_spacing.y = 0.0` avant de construire la table
et dans les deux empilements verticaux de la gouttière côté en-tête
(`column_header_widget`, `row_number_header_widget`) : sans ça, l'espacement
par défaut d'egui entre rangées empilées laissait un mince filet blanc entre
chaque case peinte d'un aplat, qui la faisait paraître flotter plutôt que
remplir vraiment sa case.

### Tab en dernière colonne : même geste qu'Entrée en dernière ligne

Entrée sur la dernière ligne d'une feuille ajoute une ligne et y déplace la
sélection (`Action::CommitCellAndAdvance` en édition, ou le raccourci
clavier direct hors édition) — Tab sur la dernière colonne d'une ligne de
données fait maintenant de même pour les colonnes plutôt que de revenir au
début de la ligne suivante (l'ancien comportement de `move_selection_tab`
aux bords).

L'asymétrie entre les deux axes tient à l'identité de la cible : une ligne
n'est qu'un indice, donc sélectionner `(colonne existante, row + 1)` *avant*
que la commande d'ajout ne soit appliquée (elle ne l'est qu'après le rendu de
l'image, voir `StokesForgeApp::ui`, `app.rs`) est sûr — la ligne existera à temps.
Une colonne neuve, elle, n'a pas encore de `NodeId` tant que `Command::
AddColumn` n'a pas tourné : la sélectionner à l'avance n'a pas de sens.
`Action::AddColumnAndSelect { sheet, row }` fait donc les deux d'un coup
dans son propre traitement (`AppState::add_column_and_select`) : ajoute la
colonne, *puis* — une fois `self.snapshot` rafraîchi par `execute` — sélectionne
`row` dans la dernière colonne visible. Câblé aux deux mêmes endroits que Tab
normal : le raccourci clavier direct de `grid_contents`, et la validation
d'édition de `cell_widget` — `app::grid::is_last_visible_column` factorise la
condition commune aux deux.

Câblé aussi **côté en-tête** (`Action::AddColumnAndSelectHeaderField`,
`AppState::add_column_and_select_header_field`), pour **chacun** des quatre
champs (Long Name, Units, Comments, Formule) de la dernière colonne, en
édition comme hors édition (#52) : Tab y ajoute une colonne et sélectionne le
même champ dans la colonne neuve. Jusqu'à #52, seul Formule le faisait — Tab
circulait d'abord colonne par colonne pour un même champ ; l'utilisateur a levé
cette décision, au prix du retour automatique au champ suivant de la première
colonne quand on remplit l'en-tête champ par champ.

### Tab et flèches appartiennent à la grille (#38)

egui traite Tab et les flèches **au début de l'image** (`Focus::begin_pass`)
pour déplacer son propre focus de widget en widget, sauf si le widget
focalisé les réclame par un `EventFilter`. La grille les lisait par
`key_pressed`, qui ne retire rien : egui donnait le focus au premier widget
focalisable venu (souvent hors de la grille), et dès qu'un widget avait le
focus la garde des raccourcis (`egui_wants_keyboard_input`) coupait net la
sélection de la grille.
Même chose en édition : le `TextEdit` d'une cellule ou d'un champ d'en-tête
perdait le focus sur Tab.

La règle, dans `grid_contents` (donc la même pour le classeur flottant et le
classeur détaché, tous deux par `book_contents`) :

- **Un ancrage de focus par grille** (`grid_focus_id`) : un widget posé sur le
  rectangle de la grille, focalisable *seulement tant qu'il a le focus* (sinon
  un Tab sans focus y entrerait de lui-même), qui porte le filtre
  `GRID_KEYS` (Tab, flèches ; pas Échap, qui rend le focus). La grille le
  réclame (`claim_grid_focus`) à un appui ou un relâchement dans son
  rectangle, et à la fin d'une édition qui a rendu le focus à personne.
  egui le retire seul à un clic ailleurs : Tab y reprend alors son rôle
  habituel, sans toucher la sélection.
- **En édition**, les champs de cellule et d'en-tête posent le même filtre
  (`TextEdit::event_filter(GRID_KEYS)`) : Tab ne les fait plus perdre le
  focus, ils le lisent eux-mêmes (`tab_pressed`), valident, rendent le focus
  et laissent la grille le reprendre.
- **Garde des raccourcis** : `focus_free` (aucun focus, ou celui de cette
  grille). Le reste de l'application, qui testait `focused().is_some()` pour
  savoir si l'on tape dans un champ (Entrée d'un dialogue, Suppr d'une
  annotation), passe par `keyboard_focus_outside_grid`.

F2 ouvre le même éditeur qu'un double-clic, avec le contenu initial conservé
(#143), uniquement lorsque l'ancrage de la grille a le focus. Sur une plage
de données ou de champs d'en-tête, il édite la case en haut à gauche, dans
l'ordre visible des colonnes. Nom long, Unité, Commentaire et Formule utilisent
leur éditeur existant ; les colonnes entières ne déclenchent pas cette édition.
Le renommage par F2 reste propre aux gestionnaires focalisés.

Limite : le filtre ne s'applique qu'à l'image qui suit la prise de focus
(`set_focus_lock_filter` exige `had_focus_last_frame`) — une touche tombant
dans l'image même du clic n'est pas une frappe réaliste. Verrouillé par
les tests de `app/grid.rs` (`app::grid::tests`). Le focus d'egui ne se voit pas dans une capture
(`STOKESFORGE_APP_SCREENSHOT`) : à confirmer sur une vraie fenêtre.

### Alignement du corps : nombres à droite, texte à gauche

`cell_widget` aligne chaque cellule du **corps** de la grille comme un
tableur — jamais la gouttière ni l'en-tête, qui n'affichent pas de valeur de
colonne. `AppState::cell_is_numeric` tranche par cellule, pas par colonne :
sur une colonne `Mixed`, deux lignes voisines peuvent s'aligner
différemment, l'une à droite (nombre saisi), l'autre à gauche (texte). Une
valeur manquante (`NaN` ou texte vide/absent) compte comme non numérique et
reste à gauche — un vide n'a pas de raison d'être poussé à droite, et ça
évite qu'une colonne clairsemée ait l'air de sauter d'un bord à l'autre
d'une ligne à l'autre.

`Label::halign` seul ne suffit **pas** ici, malgré ce que sa doc laisse
penser : `ui.put` place le `Label` dans un `Layout::centered_and_justified`,
qui justifie le texte (l'étire pour remplir la largeur) avant que `halign`
n'ait voix au chapitre — le résultat restait visuellement centré, quel que
soit `halign`. Un vrai `Layout` directionnel (`right_to_left`/
`left_to_right`, `Align::Center` pour l'axe vertical), posé via
`ui.scope_builder` sur un `max_rect` légèrement rétréci — le même mécanisme
que celui déjà posé sur toute la table (`cell_layout` du `TableBuilder`) —
aligne réellement le texte contre le bord voulu.

Un clic droit **hors** de la sélection la réduit d'abord à la case visée, un
clic droit **dedans** la laisse intacte — pour que le menu agisse toujours sur
ce qu'on vient de désigner — même garde (`response.secondary_clicked() &&
!selected`) dans les quatre zones. `column_header_widget` ne l'avait pas :
son clic droit réduisait **toujours** la plage à la lettre visée, ce qui
rendait impossible de changer d'un coup le type ou le rôle de plusieurs
colonnes sélectionnées, alors même que le commentaire de son `target_columns`
supposait déjà l'inverse. Corrigé en même temps que l'arrivée de la
gouttière, qui a rendu l'écart visible.

Deux écarts délibérés entre le menu et le clavier :

- **« Supprimer et décaler » supprime tout de suite**, là où la touche Suppr
  vide à la première pression et ne supprime qu'à la seconde. La double
  pression n'existe que pour protéger d'une frappe réflexe sur une touche
  facile d'accès ; un menu contextuel est un geste explicite et désigné.
- **« Coller » n'est jamais grisé.** egui n'expose aucune API de *lecture* du
  presse-papier (Ctrl+V ne marche que parce qu'`egui-winit` lit lui-même le
  presse-papier avant de pousser l'`Event::Paste`), donc `app::grid::
  system_clipboard_text` le lit directement via `arboard` — la bibliothèque
  qu'`egui-winit` emploie déjà en interne. Savoir s'il y a quelque chose à
  coller exigerait cette lecture à *chaque image* où le menu est ouvert : on
  tente au clic, et on pose une notice si ça ne donne rien. « Coller à
  l'identique », lui, est grisable sans coût — il rejoue le presse-papier
  **interne**, dont on connaît l'état.

Enfin, les cinq constantes `SHORTCUT_COPY`/`SHORTCUT_CUT`/`SHORTCUT_PASTE`/
`SHORTCUT_COPY_FULL_COLUMNS`/`SHORTCUT_PASTE_ABSOLUTE` sont **purement
décoratives** : elles n'existent que pour écrire le rappel de raccourci à
droite d'une entrée de menu. Rien ne les consomme, et il ne faut pas essayer —
`egui-winit` intercepte Ctrl+C/X/V à la traduction des événements de la
fenêtre et n'en laisse jamais passer de `Event::Key` (un essai précédent,
fondé sur l'hypothèse inverse, produisait des raccourcis qui ne se
déclenchaient jamais).

### Limites assumées du banc

- **Les raccourcis sont consommés au niveau du contexte**, donc *avant* les
  widgets. Ctrl+Z pendant la saisie d'une cellule annule la dernière **commande
  du modèle**, pas la dernière frappe : c'est le comportement attendu d'un
  tableur, et c'est aussi la limite — le banc ne sait pas distinguer les deux
  intentions.
- **Une seule cellule éditable à la fois**, toutes grilles **dockées**
  confondues (`AppState::editing`). La grille est virtualisée
  (`TableBuilder::body().rows()`) : une feuille peut avoir un million de
  lignes, il n'est pas question de garder un tampon par cellule. Le même
  principe vaut pour l'édition d'une note (`AppState::NoteEdit`), pour la même
  raison de cohérence plutôt que de nécessité stricte. Un onglet **détaché**
  a son propre tampon, indépendant de celui du dock et des autres onglets
  détachés (voir « Détachement générique des onglets de contenu ») —
  contrainte technique de la fermeture `'static` de `show_viewport_deferred`,
  pas une extension délibérée de la limite.
- **Les fenêtres ouvertes et leur mode ne sont pas restaurés** : le
  démarrage ouvre le premier classeur et le premier graphe. Les positions
  et tailles enregistrées sont réutilisées pour les contenus, mais la
  maximisation et le détachement ne constituent pas un plan de disposition
  persisté.

### Un détail qui a l'air d'un détail

`MISSING_TEXT_DISPLAY` vaut `\u{00A0}` — un espace insécable — pour afficher une
valeur texte **absente**. Il occupe la cellule comme un blanc ordinaire (rien ne
saute aux yeux) tout en restant, en mémoire, une chaîne différente de `""`, donc
distincte d'une chaîne vide **présente**. Un caractère visible (`—`, `∅`, `?`)
aurait été une réponse légitime à « qu'est-ce qu'un utilisateur pourrait vouloir
taper ici ? » ; un espace insécable n'est accessible à la frappe sur aucun
clavier usuel.

Il ne vaut que pour les colonnes **texte**, où une chaîne vide présente existe
et doit rester indiscernable à l'œil. Une colonne **mixte** porte
l'information dans le type de sa cellule : `cell_text_in` la lit par
`Column::cell` (et non `Column::get`, qui confondrait les deux cas) et affiche
une cellule vide comme une cellule numérique vide, `""`.

### Taper ce qui n'est pas un nombre

Une saisie non numérique dans une colonne numérique **élargit la colonne**
plutôt que d'être refusée : `commit_cell` envoie `Command::SetCellsPromoting`,
la colonne devient `ColumnType::Mixed`, la cellule porte le texte, les autres
gardent leurs nombres ([`commandes-et-historique.md`](commandes-et-historique.md)).
Conversion et écriture tiennent dans une seule entrée d'historique : un
`Ctrl+Z` ramène le type *et* la valeur d'avant.

Avant les colonnes à types mixtes, la saisie était refusée avec un message
d'erreur, et noter un « n/a » sur une ligne obligeait à convertir toute la
colonne en texte — donc à rendre tout le reste intraçable. Sur une colonne
déjà mixte, plus rien n'est promu : chaque cellule décide pour elle-même, et
un nombre tapé y reste un nombre.

### Binaire

`stokesforge-app [chemin]`. Si `chemin` existe, le projet est chargé depuis ce
fichier ; sinon un projet de démonstration est construit et `chemin` devient la
cible d'un futur enregistrement.

Les colonnes du projet de démonstration utilisent les noms automatiques
`A`, `B`, `C`… ; le script prérempli lit `[Book1]Sheet1!A`, la première
colonne visible dans la grille. Les noms descriptifs ne sont pas utilisés
comme clés cachées de la démonstration.

Le graphe de démonstration suit le même défaut que les créations par le menu,
le tracé de colonnes et `pyst.plot` sans option `title` : aucun titre (#103).
Un titre vide n'est pas dessiné et ne réserve aucune bande au-dessus du tracé.
Le titre reste saisissable dans **Graph › Properties › Graph Properties… › General** ; un titre
présent reste éditable au double-clic sur la figure.

Deux variables d'environnement servent aux tests et aux captures :
`STOKESFORGE_APP_AUTOQUIT_MS` et `STOKESFORGE_APP_SCREENSHOT`.

### Tests

`tests/shell.rs` (la boucle complète, sans fenêtre), `tests/script_panel.rs`,
`tests/script_bridge_lazy.rs` (l'interpréteur ne démarre pas tout seul),
`tests/perf_write_under_snapshot.rs`, `tests/libelles.rs` (la garde de
traduction, voir « Interface multilingue »).

## `stokesforge-cli`

Binaire sans fenêtre : `import` construit un projet neuf depuis des fichiers
d'appareil, `recipes` liste les modèles et `run` (feature `python`) exécute
un fichier Python via une session headless. Les mêmes appels `pyst`, commandes
et groupes annulables sont disponibles dans le CLI et la console ; voir
[`python-et-script.md`](python-et-script.md). Le paquet Linux autonome se
construit et se vérifie par `scripts/cli-dist.sh`.

Les distributions de l'application et du CLI livrent également les licences
de StokesForge et les notices tierces (ressources, patches, CPython et numpy)
par `scripts/licenses-dist.sh`. Installer ses outils avant l'assemblage ;
voir [`licence-et-extensions.md`](licence-et-extensions.md).

## Règles communes avec le rendu

Deux règles ne sont **pas** réécrites dans l'interface (issue #25) :
la syntaxe des couleurs `#RRGGBB[AA]` (`stokesforge_plot::parse_hex`, dont
`color_picker::parse_hex` n'est qu'un adaptateur) et le nom affiché d'une série
(`stokesforge_core::series_label`, appelée par la légende **et** par la liste des
séries des propriétés du graphe via `series_list_label`). Les transformations
RGB/HSL restent propres au sélecteur.

## Gestionnaire d'objets (Object Manager, #81–85)

Affichage › Gestionnaire d'objets › Afficher ouvre l'arbre du
graphique actif : graphique › calques (`PlotSpec::panels`) › groupe de tracés ›
séries, ainsi que les objets rares, textes libres et objets graphiques
(rectangle, ellipse, ligne, flèche), quand il y en a. Les **textes
automatiques** (titre du graphique et des calques, titres d'axes, légende) sont
cachés par défaut (#102) : une seule bascule, « Afficher les textes du
graphique » (Affichage › Gestionnaire d'objets et
clic droit sur le fond du panneau, même
`Action::SetObjectManagerShowTexts`), les liste. C'est une préférence
d'interface (`object_manager_show_texts` dans `preferences.json`, hors `.stf`),
et le filtre vit dans `object_tree_with_selection`. Un texte caché n'apparaît
pas même sélectionné dans la figure. Les **axes** (#111, #147) suivent le même
motif avec leur propre bascule, « Afficher les axes »
(`Action::SetObjectManagerShowScales`, préférence `object_manager_show_scales`,
même sous-menu Affichage et dans le menu du fond) : non listés par défaut, ils
ajoutent sous chaque calque, après le groupe de tracés, un groupe « Axes »
(`VisibilityTarget::Axes`, `LayerRow::axes`) qui réunit une ligne par axe — les
axes que la figure trace : bas, gauche, droite et haut, ces deux derniers
étant, sur un calque ordinaire, des **axes miroirs** (#176) — avec
une seule case qui commande `AxisSpec::visible` (ligne d'axe, graduations et
libellés d'un coup ; le titre garde son `title_visible`) ; pour le haut, et la
droite quand aucune série n'y est rattachée, elle commande `AxisSpec::mirror`
(`AxisRef::is_mirror`), et se grise quand un calque superposé de la case
occupe ce côté (`AxisRef::blocked`). La case du groupe
« Axes » commande ses axes et reste mixte tant qu'un miroir est décoché ; ses
Propriétés ouvrent l'onglet Axes (pages Haut et Droite : `mirror_axis_editor`,
« afficher l'axe miroir » et « afficher les étiquettes de graduation », ni
titre ni couleur). Un **calque superposé** (#171)
ne liste que les axes qu'il affiche (`object_manager::axis_sides`) : son axe X
nommé « X (haut) » si `x_top`, son axe Y nommé « Y (droite) » si `y_right`,
chacun commandant l'`AxisSpec` `bottom` / `left` qui le porte ; ses lignes
`right` / `top`, mortes, n'existent pas. Le groupe est **replié par
défaut** (#138) : un clic sur sa flèche déplie les lignes d'axe, sans toucher
au modèle. Les cases du graphique et du
calque ne comptent que les **enfants listés** (#173), regroupés dans `Shown` :
les textes automatiques sans la bascule « Show graph texts », les axes sans
« Show Axes » ; un enfant non listé garde son état. Axes listés, ils comptent
(calque et graphique mixtes si un miroir est décoché) ; la case du groupe de
tracés n'a que ses séries (`VisibilityTarget::contents_shown`,
`visibility_shown`, `set_shown`). Activer une bascule recalcule les parents. `Action::SetPlotVisibility` lit la bascule dans l'état. Le panneau est redimensionnable à droite ;
son bouton Détacher ouvre une fenêtre système, Rattacher le replace à droite. Il est
visible par défaut ; l'ouverture ou la fermeture choisie se mémorise dans
`preferences.json`, hors du projet et de l'historique.

Les réglages de vue (#139) sont regroupés dans **Affichage › Gestionnaire d’objets**,
aussi accessibles à plat au clic droit sur le fond (#152), même sans
graphique actif. Les deux filtres sont au premier niveau ; **Déplier par
défaut** règle graphique, calques, groupes de tracés, axes et objets. Les objets
sont regroupés par graphique ou calque, sans modifier leur visibilité.
Le menu est grisé lorsque le panneau est fermé. **Réinitialiser** rétablit les
filtres cachés, les axes repliés et les autres groupes dépliés ; les replis déjà
choisis restent libres et conservés. Les valeurs initiales ne s'appliquent
qu'aux groupes sans état egui, jamais à un groupe déjà peint ou manipulé.
`preferences::ObjectManagerSettings` est la table unique des défauts ; les
réglages vont dans `preferences.json`, hors `.stf` et historique. Les anciens
champs des deux filtres restent lus et écrits pour la compatibilité.
`object_manager_settings_menu` et `object_manager_contents` servent aussi le
viewport détaché ; une modification des réglages le réveille même si le
projet et la sélection n'ont pas changé.

Chaque série garde son nom dérivé de colonne et son échantillon de légende
(trait, couleur, épaisseur, marqueur ou barre). Les cases des parents modifient
ensemble leurs contenus ; le groupe commande seulement les séries. Une case
indéterminée signale une visibilité partielle. Masquer conserve données,
style, texte, position et rang ; une série masquée sort du tracé, de la légende
et des bornes automatiques. Les cases du graphique et du calque commandent toujours le contenu complet,
textes cachés compris. Les titres ont leur propre `title_visible` :
masquer un titre d'axe laisse l'axe en place. Une bascule écrit une seule
commande `EditPlotSpec`, avec le motif d'historique `PlotEditKind::Visibility`.

Cliquer un contenu dans l'arbre ou sur le tracé synchronise la sélection, en
gras dans l'arbre ; la série est surlignée sur la figure. Un contenu masqué
reste sélectionnable dans l'arbre. La partie format d'objet de la barre `Tools` règle le trait
d'une série ou d'un objet ; celle de texte règle le texte sélectionné. Le
panneau et ces barres suivent aussi une fenêtre de graphique détachée.

Le clic droit dans l'arbre ou la figure et Graphique › Sélection partagent
le même menu (`object_node_menu`, #152) : Afficher les données (courbe),
Renommer (F2), Déplacer vers le haut/bas (Ctrl+Haut/Bas), Afficher/Masquer,
Supprimer (Suppr) et Propriétés… en dernier, selon la cible. Un clic simple sur une ligne donne
le focus clavier au panneau : F2, Ctrl+flèches et Suppr agissent dès ce clic,
sans glisser préalable (#126). Le trait coloré et le nom de la série offrent
les mêmes gestes (sélection, clic droit, double-clic et glisser, #151).
Supprimer une série ne supprime aucune colonne de données ni les autres
courbes ; Annuler/Rétablir restaure sa place et ses réglages. Un clic ailleurs
rend ce focus.

Le glisser-déposer d'une série change son rang dans le groupe de son calque, avec un repère avant/après la ligne
visée ; un dépôt sur un autre calque est refusé. Le relâchement envoie une
seule action. La sélection suit le contenu déplacé, y compris après
Annuler/Rétablir, et une suppression décale les adresses des voisins sélectionnés.

Les propriétés choisissent la série et le calque exacts, même masqués ; le
sélecteur de calque est commun aux pages de la fenêtre. Un texte libre
réutilise son éditeur sur place. Un objet graphique ouvre un brouillon de
propriétés (coordonnées, trait, remplissage) : Valider envoie une action,
Annuler ne change rien. Les parents ont leurs propriétés et leur visibilité ;
la suppression concerne les feuilles, et le renommage les séries.

`object_manager.rs` porte la projection, la visibilité et les règles de
réordonnancement ; `app/object_manager_ui.rs` (#121) peint le contenu commun
ancré/flottant (`object_manager_contents`, menus, cases, glisser, raccourcis)
et porte la glu de sélection vers `GraphView` (`select_manager_target`,
`selected_manager_target`) ; ce qu'il demande à la vue remonte en
`ObjectManagerRequest`, exécuté après la peinture par
`StokesForgeApp::run_object_manager_request` (même module, #122, qui porte
aussi le panneau `object_manager_panel`) — le menu « Graph › Selection » passe par le même chemin. Les décisions de mutation restent dans
`AppState::dispatch`, avec les commandes de la session.
