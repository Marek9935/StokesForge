# Application : la coquille et le découpage du code

Suite du sommaire [`README.md`](README.md). Contenu : le principe du banc, les deux crates (`stokesforge-app-state` / `stokesforge-app`) et la table module → rôle de `src/app/`.

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

### Deux crates : l'état et la coquille (#263)

L'application est coupée en deux crates, et c'est le compilateur qui tient la
frontière. **`stokesforge-app-state`** (sans egui) reçoit tout ce que
l'application décide sans le dessiner : `state.rs` + `state/` (`AppState`,
`Action`), `selection.rs`, `preferences.rs`, `analysis.rs`,
`analysis_recent.rs`, `object_manager.rs`, `object_drag.rs`,
`history_text.rs`, `quantities.rs`, `user_colors.rs`, `object_format.rs`,
`text_format.rs`, `errors.rs`, `i18n.rs` (avec les catalogues `i18n/`) et
`file_picker.rs`. **`stokesforge-app`** garde `main.rs`, `app.rs`, `app/`,
`theme.rs`, `icons.rs`, `field_state.rs`, `number_field.rs`, `toolbar.rs`,
`list_frame.rs`, `tree_lines.rs` et `color_picker.rs` : tout ce qui nomme un type
d'egui, et l'offre de la figure au presse-papiers (`stokesforge-clipboard` tire
winit, donc ne peut pas descendre). Dans les chemins de ce document, un module
sans préfixe est dans `stokesforge-app/src/` s'il est sous `app/` ou nommé
`theme`, `icons`, `field_state`, `number_field`, `toolbar`, `list_frame`,
`tree_lines`, `color_picker`, et dans `stokesforge-app-state/src/` sinon.

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
| `app/menus.rs` | barre de menu (`menu_bar` appelle `file_menu`, `edit_menu`, `workbook_menu`, `graph_menu`, `view_menu`, `window_menu`) et barre d'état (`status_bar`, `status_line`) |
| `app/status_history.rs` | fenêtre en lecture seule des 50 derniers messages de la barre d'état, horodatés en UTC (#230) |
| `app/analysis_menu.rs` | menu Analysis (#181) : `analysis_menu_contents` (barre de menus), `analysis_open_menu` / `analysis_entry` (clic droit : sous-menu `Analysis ▸`, une entrée par catégorie, #248), `recent_entries` (trois dernières analyses, #249 ; état dans `analysis_recent.rs`), `calculation_section` (Edit / Recalculate / Detach / Delete sur une colonne calculée) |
| `app/stale_export.rs` | confirmation d'un export ou d'une copie de figure qui montre des séries pas à jour (#198) |
| `app/analysis_dialog.rs` | fenêtre d'analyse (`modal_dialog_with`, deux colonnes : formulaire + aperçu) et confirmation de suppression d'un calcul (`calculation_delete_window`) |
| `app/analysis_forms.rs` | les formulaires par famille (`form_body`, `value_row`, tableau des paramètres) |
| `app/analysis_handles.rs` | poignées déplaçables d'un calcul sur le graphe du projet (#199) : visibles quand le calcul est sélectionné (sa courbe, ou sa ligne du volet Results), un glisser = un `MoveCalculationHandle` |
| `app/analysis_preview.rs` | l'aperçu peint à la main (la peinture seule) : gestes Point / Zone, navigation comme le graphe (Ctrl + molette, Ctrl + glisser, double-clic), aimant (Maj), moyenne (Maj + molette), bande des résidus, cache en mémoire temporaire d'egui ; sa géométrie et ses graduations viennent de `stokesforge-render` (#261) |
| `app/results_pane.rs` | volet Results sous la grille d'un classeur : compteur, portée Feuille / Classeur, liste et tableau série × valeur |
| `app/format.rs` | barres de format de texte et d'objet, barre contextuelle : `format_target`, `text_format_state`, `object_format_state`, `tools_context`, `format_zone`, `format_text`, `format_object`, `run_toolbar_request` (les barres elles-mêmes : `crate::toolbar`) |
| `app/project_manager.rs` | Project Manager : `project_manager_panel` (panneau fixe) et l'arbre `project_manager_contents_from_tree`, commun au panneau et à la fenêtre détachée |
| `app/curve_settings.rs` | fenêtre des propriétés des séries (#177–#179) : liste, groupe, style commun, nuanciers (`curve_settings_page`) ; ouverte comme onglet `PlotSettingsTab::Series` de `plot_settings_window` |
| `app/object_manager_ui.rs` | Object Manager peint : `object_manager_panel`, `run_object_manager_request`, lignes, cases, menus, glisser des séries, groupes et objets libres par la poignée (le modèle : `crate::object_manager`) |
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
| `app/inplace_edit.rs` | **le** contrat de l'édition sur place (#224) : ouverture sélectionnée, Entrée/Tab/clic ailleurs valident, Échap annule ; décision pure `resolve` |
| `app/import_wizard.rs` | assistant d'import : `import_window`, ses six étapes ; choix du modèle/fichiers et réglages du format en sections, l'aperçu coloré par rôle, les libellés des choix |
| `app/plot_settings.rs` | propriétés du graphe : `plot_settings_window`, ses onglets, un éditeur par page, l'aide à la syntaxe des formules (`formula_field`, `formula_help`) ; `column_metadata_window` |
| `app/hover_help.rs` | brique commune de l'aide au survol d'un champ (#197) : `hover_help`, l'icône grisée à droite du champ dont la bulle montre le rappel |
| `field_state.rs` (racine du crate, comme `color_picker.rs`) | brique commune de l'état d'un champ (#219) : `FieldIssue` (refusé rouge / ramené orange), `show_issue`, mémoire de 2 s (`flag`, `active`), cycle d'un champ texte validé (`edit_text`, `decide`, `Verdict`) |
| `number_field.rs` (racine du crate) et `quantities.rs` (dans `stokesforge-app-state`) | brique « nombre » (#220) : `number_field` (champ texte, flèche pleine dans le cadre si la grandeur a des valeurs, flèches du clavier, molette au survol qui suit la liste, liste dans une `egui::Area`) et la table des grandeurs (`Quantity`) ; construite sur `field_state::edit_text_with_arrow` ; `list_frame.rs` : flèche et fonds partagés avec les listes déroulantes (#234) |
| `app/confirmations.rs` | dialogues bloquants de l'application (#229) : « Discard Changes · Cancel · Save », échec grave (S-M8), écrasement d'un export, choix d'axe d'un dépôt de lignes ; classement confirmation / choix en tête du module |
| `app/dialogs.rs` | brique commune des dialogues modaux : `modal_dialog`, `modal_dialog_with`, `modal_wizard` (assistant), `modal_read_only` (Close seul), `modal_confirm` (Cancel par défaut, verbe destructeur), `modal_save_changes` (trio, Entrée = Save), `modal_alert` (S-M8), corps commun `modal_frame` ; pied ordonné (S-F6), zone de message (erreur de `AppState::dialog_error`, sinon raison du principal grisé), Échap à deux niveaux, géométrie par sorte de dialogue dans les préférences et focus initial sur le premier champ (#228) ; `sync_dialogs`, appelé par `show_edit_dialogs`, relie pied, préférences et état |
| `app/form.rs` | brique commune du formulaire (#227) : `form` et `Form` (grille à deux colonnes : `row`, `field`, `checkbox`, `color`, `note`, `section`, `enabled`), choix exclusif fixe `choice` / `choice_by` (radios pour 2–3 options, liste dès 4, `choice_layout`) |
| `app/tab_strip.rs` | brique commune de la bande d'onglets : `tab_strip`, `tab_strip_with` (`TabOps`, `TabEvent`) |
| `app/menu_sections.rs` | brique commune des menus : `MenuSections::section` (seul trait de séparation, #155), `menu_choice` (coche seule d'un choix exclusif, S-C3, #227) |
| `app/list_row.rs` | brique commune de la ligne de liste sélectionnable (#169) : `list_row`, `ListRowBackground`, `list_row_label`, `list_row_text`, `LIST_ROW_MARK` ; Project Manager, Object Manager, liste des séries des propriétés |
| `app/shortcuts.rs` | **registre unique des touches** (#231) : constantes `SHORTCUT_*` et `KEY_*`, leurs rappels, tableau `HELP` qui engendre Help › Keyboard Shortcuts, consommation d'annuler/rétablir dans le viewport actif (`collect_undo_redo_shortcuts`) |
| `app/preferences_window.rs` | File › Preferences (#148) : rubriques General (langue), Display (rangées Metadata et x' des nouveaux classeurs, `Action::SetWorkbookHeaderDefaults`) et Export (formats de Copy Graph : ordre au glisser, cases, `Action::SetCopyFormats`, préférence `copy_formats`), appliquées tout de suite, `modal_read_only` |
| `app/object_dnd.rs` | état du glisser des séries, groupes et objets libres, partagé par l'Object Manager (aperçu, une action au relâchement) et la fenêtre des séries (brouillon modifié en direct) ; décisions pures dans `stokesforge_app_state::object_drag` (`step`, `finish`) : entrer dans un groupe, en sortir, échanger des séries seules, déplacer un groupe en bloc, réordonner texte ou forme dans sa catégorie ; zone « fin de la liste » pendant le glisser |
| `app/reorder_list.rs` | brique commune d'une liste réordonnée au glisser : poignée `Grip`, ligne qui suit le pointeur, lignes qui s'écartent en direct (`reorder_row`, `reorder_move`, `move_before`) ; seule la poignée se saisit, pour ne pas voler le clic des cases |
| `app/keyboard_shortcuts_window.rs` | Help › Keyboard Shortcuts (#231, S-K5) : fenêtre en lecture seule, engendrée de `HELP` |
| `app/busy.rs`, `app/empty_state.rs` | briques « travail en cours » (indicateur après 0,5 s, progression, Stop après 3 s ; #232 ; import en fond et recalcul de formules #239) et « état vide » (phrase grisée centrée qui dit le geste) |
| `app/cursors.rs` | seul module qui nomme un `CursorIcon` (#231, S-X2) : main ouverte/fermée, flèches de redimensionnement, doigt des éléments cliquables qui ne sont pas des boutons |
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

