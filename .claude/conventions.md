# Conventions par domaine

Règles détaillées sorties de `CLAUDE.md` pour qu'elles ne soient chargées
qu'au besoin. Elles valent autant que celles de `CLAUDE.md` : lire la section
du domaine **avant** d'écrire du code. La plupart sont aussi verrouillées par
une garde de `stokesforge-app/tests/gardes.rs` ou `libelles.rs` ; une garde verte ne dispense
pas de la règle (elle ne voit que ce qu'elle cherche).

## Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes

**Deux crates, une frontière tenue par le compilateur (#263).** `stokesforge-app-state` porte tout ce que l'application décide sans le dessiner : `state.rs` + `state/` (`AppState`, `Action`), `selection.rs`, `preferences.rs`, `analysis.rs`, `analysis_recent.rs`, `object_manager.rs`, `object_drag.rs`, `history_text.rs`, `quantities.rs`, `user_colors.rs`, `object_format.rs`, `text_format.rs`, `errors.rs`, `i18n.rs` + `i18n/` (les catalogues `.ftl` et `tr!`) et `file_picker.rs` ; il n'a **aucune** dépendance egui/eframe/winit/wgpu (`tests/no_ui_dependency.rs`, liste blanche). `stokesforge-app` garde `main.rs`, `app.rs`, `app/`, `theme.rs`, `icons.rs`, `field_state.rs`, `number_field.rs`, `toolbar.rs`, `list_frame.rs`, `tree_lines.rs`, `color_picker.rs` : tout ce qui nomme un type d'egui. Une logique d'interface qui se décide va dans `stokesforge-app-state` (jamais de `use egui` : le compilateur refuse), une peinture ou un geste dans `stokesforge-app`. Les imports sont explicites (`use stokesforge_app_state::state::Action;`), sans réexportation à la racine, hors `tr!` (`use stokesforge_app_state::tr;`). Le presse-papiers système (`stokesforge-clipboard`) tire winit : l'état ne garde que des octets neutres (`state::FigureBytes`, un `ExportFormat` et ses octets), `app/figure_copy.rs` les traduit en types MIME et met en phrase l'erreur (`clipboard_error_message`). **Piège `cfg(test)`** : `cfg(test)` ne vaut que pour les tests du crate qui le porte. Ce que l'état isole sous `cfg(test)` (chemin des préférences `preferences::default_path`, langue de départ `i18n::startup_language`, `set_object_manager_open_for_test`) l'est donc aussi sous la feature `test-support`, que la dev-dependency de `stokesforge-app` active : sans elle, les tests de l'app (qui bâtissent un `AppState` par `new`) liraient et écriraient le vrai `preferences.json` (verrouillé par `stokesforge-app/tests/isolation_preferences.rs`). Jamais `test-support` dans un build normal ni dans un `[dependencies]`. `fl!` lit l'`i18n.toml` du crate **appelant** : les deux `i18n.toml` fixent le même `domain = "stokesforge_app"` et celui de l'app pointe le catalogue de `stokesforge-app-state/i18n`.

Transitions #72–#73 : Wayland natif est le défaut, `STOKESFORGE_X11=1` force
X11 ; les patches `vendor/winit` et `vendor/smithay-clipboard`, hors workspace,
partagent le récepteur de dépôt et le presse-papiers entre les viewports.
Choix, commandes `rustfmt` ciblées, vérification manuelle et retrait :
`vendor/winit/STOKESFORGE.md`. Le parseur et le canal exacts sont testés par
`stokesforge-app/tests/wayland_{uri,bridge}.rs`.
Le graphique vide passe par `AddGraph` ; `SessionHandle::new_project`, comme
`load`, remplace l'identité et vide l'historique après confirmation dans l'UI.

- `stokesforge-app` : découpage d'`app.rs` (chantier #115, #116–#124 faits) — règle retenue : de **vrais modules** dans `src/app/`, un par surface (table module → rôle dans `docs/application/coquille-et-code.md`, § Découpage du code), à imports explicites, visibilité minimale (`pub(super)` entre modules d'`app`) ; aucun `use …::*` hors du code de test (`aucun_import_global_hors_des_tests`, `tests/gardes.rs`, sans dérogation) ; **`app.rs` ne garde que l'orchestration** de l'image (`StokesForgeApp` et `new`, `ui()` et l'ordre des panneaux, `collect_shortcuts`, application différée des actions, confirmations bloquantes, drainage des fenêtres détachées) — une entrée de menu va dans `app/menus.rs`, du code de fenêtre dans `app/window_*.rs`, une surface nouvelle dans son propre module (modèle : `object_manager.rs`, `toolbar.rs`, `app/dialogs.rs`). Briques communes : `app/dialogs.rs` (`modal_dialog`, `modal_dialog_with`, `modal_wizard`, `modal_read_only`, `modal_confirm`, `modal_save_changes`, `modal_alert`), `app/tab_strip.rs` (`tab_strip`), `app/shortcuts.rs` (`SHORTCUT_*`), `app/tab.rs` (`Tab`), `app/window_chrome.rs` (cadre, `show_window_in_layer`). **Les tests suivent leur code** (module `tests` de la surface ; celui d'`app.rs` n'éprouve que l'orchestration) et sont relus à chaque restructuration : périmètre fixé par l'agent `perimetre` (Opus), tests écrits par Sonnet, témoins vérifiés par mutation (`CLAUDE.md`, « Délégation »). Fonctions géantes redécoupées sur place (#124), jamais avec un déplacement : conserver l’imbrication des Ui/ID egui et l’ordre des événements ; les contextes privés de peinture ne regroupent que les données ou emprunts utiles à l’image. Toutes les gardes qui lisent le code passent par un lecteur unique, `stokesforge-app/tests/support/sources.rs` (tout `src/` **des deux crates**, récursif, les sources de `stokesforge-app-state` sous le préfixe `app-state/` ; fonctions cherchées par leur nom ; jamais `include_str!` ni fichier nommé, #116, #263).
- `stokesforge-app` : tout réglage de couleur passe par `color_picker::color_field` avec l'usage de la couleur (`ColorUsage::Curves`/`Text`/`Background`) et le `PickerEnv` des couleurs enregistrées, jamais par un champ texte `#RRGGBB`. Dans une barre d'outils, l'échantillon est un petit carré précédé d'une icône (`color_icon_button`, #89) ; dans un formulaire, `color_field` (libellé + carré). Couleur vide = automatique, peinte en carré barré d'un trait rouge ; opacité en `#RRGGBBAA` ; le texte et les échelles automatiques suivent `stokesforge_render::color::auto_ink` (blanc sur fond sombre).
- `stokesforge-app` : toute fenêtre passe par la brique commune — `show_window_in_layer` (`app/window_chrome.rs`) pour une fenêtre de contenu, `modal_dialog_with` (`app/dialogs.rs`) pour un dialogue, `modal_wizard` pour un assistant, `modal_read_only` (Close seul) pour une fenêtre qui ne change rien, `modal_confirm` / `modal_save_changes` / `modal_alert` pour une confirmation, le trio « Discard Changes · Cancel · Save » et un échec grave (#229 ; seulement pour ce que Ctrl+Z ne rattrape pas, peints depuis `app/confirmations.rs`) (même cadre `modal_frame`, même barre de titre, même blocage que les propriétés du graphe ; pied ordonné, zone de message — erreur de `fail` tant qu'un dialogue est ouvert, sinon raison du principal grisé portée par `accept_disabled` (#226) —, focus initial et géométrie mémorisée par la brique, #228) ; jamais d'`egui::Window` brute, même pour un assistant à étapes (verrouillé par le test `aucune_fenetre_n_est_une_egui_window_brute`). Un code existant qui déroge n'est pas un modèle.
- `stokesforge-app` : le texte atténué (`.weak()`) est réservé au **secondaire** (détail technique sous une phrase, raison, aide, état vide) ; le contenu qu'un dialogue ou une fenêtre en lecture seule veut faire lire est en texte normal (`docs/standard-interface.md`, « Texte atténué »). Copier le `.weak()` d'un dialogue voisin n'est pas une justification : le détail d'erreur d'une alerte d'enregistrement est atténué parce qu'il est secondaire, pas parce que c'est une alerte.
- `stokesforge-app` : toute icône passe par `icons::icon_image` / `icon_button` (Phosphor duotone, rôle de couleur `IconRole`, info-bulle `tr!` obligatoire), jamais par un glyphe Unicode ni un emoji ; couleur rare, règles dans `docs/icones.md` (verrouillé par `aucune_icone_hors_du_composant`). **Chaque action a son logo** (#110) : icône soignée si elle est dans une barre fixe, logo standard de famille à deux lettres sinon (gabarit à venir) ; ça ne force pas l'affichage dans les menus.
- `stokesforge-app` : tout texte d'interface passe par `tr!` avec une clé du catalogue `en`, rédigée selon `docs/textes-interface.md` ; jamais de littéral (verrouillé par `aucun_libelle_en_dur`, y compris les titres `set_title` et noms visibles `add_filter` des boîtes de fichier, #127 ; extensions et sigles de format techniques préservés). Toute clé s'écrit **dans les deux** catalogues (`en` et `fr`) dans le même commit (complétude bloquante). Aucune dérogation : la garde est absolue (#48), et une clé du catalogue qu'aucun appel n'emploie fait échouer `aucune_cle_du_catalogue_n_est_morte` (`tests/libelles.rs` : le catalogue est dans `stokesforge-app-state`, mais les menus sont écrits dans `stokesforge-app`, d'où la lecture des deux).
- `stokesforge-app` : une barre d'outils est une variante de `toolbar::ToolbarId` dont le contenu s'écrit dans `toolbar::bar_contents` avec des `ToolbarButton` (icône du composant commun, info-bulle `tr!`) qui poussent les `Action` des menus ; jamais un second chemin d'exécution ni un panneau posé à côté. Les barres sont **fixes** (#108) : contenu et place figés dans le code (`ToolbarId::row`), ni déplacement ni mode flottant ; seule leur visibilité (menu View) vit dans `preferences.json`, pas dans le `.stf`. `Tools` et `Text` sont toujours peintes, boutons grisés avec info-bulle sans fenêtre concernée. Le format de texte (`ToolbarId::Text`) écrit des balises Typst dans le texte via `text_format::apply` pour le gras, l'italique, la couleur et le corps d'un passage ; la **police** (et, pour un titre ou la légende, le corps) est un champ de la zone (`TextFont`, `AnnotationSpec::font`, #77/#78), posé par `StokesForgeApp::format_zone` — jamais une balise `#text(font: …)` dans le contenu.
- `stokesforge-app` : aspect commun (#169) — un jeu d'onglets (pages, feuilles, axes) passe par `tab_strip` + `TabStripSide`, jamais par une rangée de boutons ; une entrée de liste ou d'arbre sélectionnable par `list_row` / `ListRowBackground` (`app/list_row.rs`, ligne pleine largeur à filet d'accent) ; le bouton sélectionnable d'egui reste aux choix exclusifs d'un menu, d'une liste déroulante ou du sélecteur de couleur (`aucun_bouton_selectionnable_hors_d_un_menu`). Gras par `theme::bold` seulement (`strong()` interdit) ; titre de section par `theme::section_title`, sans `separator()` juste avant (`aucun_separateur_avant_un_titre_de_section`) ; couleurs d'état et de chrome par `theme.rs` ou les `Visuals`, aucun `Color32::from_…` hors de `theme.rs` et `color_picker.rs` (`aucune_couleur_en_dur_hors_du_theme`) ; un formulaire est une grille à deux colonnes par `app/form.rs` (`form`, `Form::row`/`checkbox`/`note`/`section`, #227), un choix exclusif fixe par `choice` (radios pour 2–3 options, liste dès 4), un choix de menu par `menu_sections::menu_choice` (coche seule), largeurs `theme::FieldWidth`, écarts `theme::SPACE_ITEM`/`SPACE_SECTION` et jamais `add_space(<nombre>)` (`aucun_add_space_litteral_hors_du_theme`).
- `stokesforge-app` : un champ qui s'ouvre sur place passe par `app/inplace_edit.rs` (#224, contrat S-T4 : tout sélectionné, Entrée/clic ailleurs valident, Échap annule, Tab valide et passe au suivant) ; une saisie de nombre se lit par `stokesforge_core::number_format::parse_input` (virgule décimale, #223), jamais `parse::<f64>` ; le séparateur des graduations est `PlotSpec::decimal_comma`.
- `stokesforge-app` : saisie et collage de cellules (#258) : **une seule règle** « texte → valeur de cellule », `stokesforge_core::ValueBlock::from_texts(kind, &[&str]) -> (bloc, promoting)` — la saisie (`commit_cell`) et le collage (`value_block_from_tsv_column`) l'appellent, jamais une seconde écriture dans l'app ni un détour par `MixedCell::from_text` (qui garde `inf`/`nan` lus, règle C2 de la conversion de type). `inf`, `nan` et `1e400` tapés ou collés restent du texte ; les dates ne sont lues que dans une colonne `DateTime`. Un collage qui écrase une colonne `Mixed` appelle la repromotion (`auto_promote_mixed_column_if_all_numeric`) avant la clôture de son groupe, et reste une entrée d'historique.
- `stokesforge-app` : tout nombre saisi passe par `number_field::number_field` et une grandeur de `quantities.rs` (#220) ; ni `DragValue` ni `Slider` (`aucun_champ_numerique_hors_de_la_brique`). Un défaut de saisie (illisible, hors borne) se peint par `field_state` (#219), jamais à la main.
- `stokesforge-app` : standard de comportement (#217) — champs, messages, dialogues, confirmations, grisage, clavier et souris suivent `docs/standard-interface.md`, source unique (codes `S-N1`, `S-F3`… et noms lisibles : index des règles de la page). Chaque section y est *en place* ou *à venir* (#219–#232) : une règle à venir vaut déjà pour tout code neuf ; une brique ou une garde à venir n'existe pas encore. Le commit qui en livre une la passe à *en place* et met à jour les skills.
- `stokesforge-app` : tout arbre déplié (Object Manager, Project Manager) trace ses connecteurs en L par `tree_lines` (`begin_tree`, `tree_children`, `TreeBranch::mark`), jamais par la ligne d'indentation d'egui (#112).
- `stokesforge-app` : l'aspect des barres de défilement (largeur, couleur) vient **uniquement** de `theme::scroll_style`, posé globalement par `theme::install` ; jamais de `ScrollStyle` local (verrouillé par le test `aucun_scroll_style_hors_du_theme`). La place d'une barre se lit par `theme::scroll_bar_allocated_width`.

## Object Manager, panneaux fixes, menus

Object Manager (#81–85) : `object_manager.rs` projette le graphe,
`app/object_manager_ui.rs` peint l'arbre (#121), la sélection
reste dans `GraphView` (même vue en fenêtre détachée). Les textes liés
(titres, légende) n'y sont **jamais** listés (#214) : ils s'éditent par
double-clic sur la figure ou dans les propriétés ; « Objects » ne contient que
les objets libres (texte libre, formes), replié par défaut, absent quand vide.
Les échelles (groupe « Axes » sous le calque, replié par défaut #138, une ligne par axe, case = `AxisSpec::visible`, #111)
ont leur propre bascule jumelle (`Action::SetObjectManagerShowScales`) et ne sont
pas commandées par les cases du graphique/calque. Les filtres vivent dans
`object_tree_with_selection`. Les réglages de vue (#139) passent par le menu
commun `object_manager_settings_menu` (Affichage et fond, grisé si panneau
fermé), les `Action` et `preferences::ObjectManagerSettings`, table unique des
défauts. Les objets ont leur section repliable par graphique/calque ; le repli
initial ne remplace jamais l’état courant d’egui, même après Réinitialiser.
Le modèle détaché transporte tous les réglages et réveille son viewport si
seuls ceux-ci changent. Les bascules réutilisent
`EditPlotSpec`, dont le motif d'historique reste une donnée. Les échantillons
appellent `series_swatch`, partagé avec la légende.

Panneaux fixes (#100) : PM à gauche, OM à droite, console en bas ; visibilité
par View, repli distinct du masquage, pas de flottant interne. Le cadre
`panel_header` (`app/window_chrome.rs`) partage les cellules d’icônes des
fenêtres de contenu avec une police compacte. L’ordre horizontal des boutons
dépend du côté : repli vers l’espace de travail, détachement vers l’extérieur.
La console déployée garde Replier puis Détacher côte à côte à gauche (#142).
Replié : épaisseur `ICON_SIZE` sans marge supplémentaire, seul Déployer est
visible, sans cible ni espace réservé à Détacher ; titre centré dans l’espace
restant (vertical pour PM/OM). Attaché déployé : Détacher et Replier ;
détaché : Rattacher uniquement, sans décoration système. Une fermeture OS
rattache. Réutiliser les contenus de chaque surface et les lectures/requêtes
de `window_detach.rs` ; le moteur Python reste dans l’AppState principal.

- Menus (#114, #152) : la grammaire de `docs/menus.md` est la source unique
  — sections dans l'ordre canonique (principale, créer, presse-papiers,
  organiser, supprimer, sortie, `Properties…` en dernier), séparateurs posés
  **seulement** par `MenuSections::section` (`app/menu_sections.rs`, garde
  `aucun_separateur_brut_dans_un_menu`, sans dérogation), sous-menu
  seulement pour un choix exclusif ou ≥ 3 entrées (jamais 1, 2 à plat),
  profondeur 2, entrée *absente* (sans objet) ou *grisée avec `.disabled`*,
  verbes nus, en-tête grisé réservé au graphe, raccourcis par `SHORTCUT_*`,
  ni icône ni Undo/Redo au clic droit. Les peintures partagées (`object_menu`,
  `object_node_menu`, `object_manager_settings_contents`,
  `columns_menu_contents`/`column_type_role_menus`/`plot_menu`,
  `graph_properties_menu`) peignent le **contenu** : le menu du haut
  l'enveloppe dans un sous-menu qui nomme la cible, le clic droit l'affiche à
  plat. Les cases se limitent aux vraies bascules d'affichage et laissent les
  menus ouverts ; une action ou un choix exclusif ferme la chaîne. Ne pas
  créer de voie de mutation parallèle.

## Graphes neufs et fenêtres détachées

- Composition d'un graphe (#257) : les règles qui fabriquent des séries depuis les colonnes d'un classeur vivent dans `stokesforge_core::compose` (`series_from_columns` pour l'appariement Y→X / Y2→X2 le plus proche **à gauche** et la palette par rang, `x_for` pour l'abscisse seule, `row_series` — le nom traduit est un paramètre, jamais de `tr!` dans core —, `overlay_base`, `second_layer_of`, `current_plot_spec`, `panel_plot_type`), partagées par l'application (`AppState::plot_columns`, `add_series_to_graph`, `add_layer`, `add_rows_to_graph`, `plot_rows` : elles n'émettent que la commande et les messages), `pyst.plot` et l'annexe d'une analyse. Une série neuve passe par `SeriesSpec::from_palette(y, rang)`, jamais `palette_color` / `LineSpec::from_palette` écrits à la main ; un calque superposé se nomme (`OverlaySpec::SECOND_AXES` pour X2/Y2, `TOP_X` pour le calque des lignes — son Y reste à gauche, voulu —, `OverlaySpec::new` pour le reste), jamais un littéral `{ x_top, y_right }`. `pyst.plot(y)` sans `x=` cherche l'abscisse par rôle comme la fenêtre (`docs/python-et-script.md`).
- Graphes neufs (#103) : aucun titre par défaut, démo comprise ; `name` nomme le nœud et sa fenêtre, jamais la figure. Garder la possibilité de saisir un titre, sans masquer `title_visible`. Les titres explicites de `stokesforge-render::sample` sont des témoins de rendu, pas des défauts utilisateur.
- Fenêtres détachées (#104) : publier un instantané ne réveille pas le callback `show_viewport_deferred`. `app/window_detach.rs` compare époque, révision métier et révision des géométries, puis demande le redessin du viewport lecteur ; une mutation appliquée après peinture programme l'image racine de publication (fin de `ui()`, `app.rs`). Tests : `app::window_detach::tests`, `app::tests`. Le Project Manager suit aussi sa sélection, son masquage et l'onglet actif. Aucun réveil périodique au repos.

## Interface : câblage à ne pas oublier (fenêtre détachée, calque, dialogue)

Règles sorties du skill `stokesforge-feature` (F1, #301) : le skill ne porte plus de règle, il renvoie ici et à `docs/`.

- **Vue détachée** : une seule écriture par contenu (`book_contents`, `graph_view_contents`, `note_contents`, `matrix_contents`), appelée des deux côtés ; ne pas en réintroduire une seconde. La vue détachée a **son propre** `AppState` : un état d'interface ajouté à `AppState` (édition en cours, confirmation, renommage…) n'y existe que s'il est recopié par `fork_for_detached_view`, resynchronisé par `sync_detached_snapshot` / `sync_detached_sheet_editor` ou rapatrié par `adopt_detached_book_view` ; sinon le geste marche en flottant et pas en détaché (constaté sur le renommage de feuille, #106). `collect_shortcuts` lit l'entrée du viewport racine : un raccourci global n'agit pas dans une fenêtre OS détachée sans câblage propre, et `active_book` retombe sur « le seul classeur détaché » même quand un graphe est actif — un raccourci de classeur peut y voler une touche à un autre contexte (F2 de l'Object Manager). Les éditeurs et le focus du Project Manager sont indexés par viewport dans la mémoire d'egui : calculer les IDs **avant** un `Context::data_mut`, sans rappel au contexte sous son verrou. Une factorisation supplémentaire est légitime quand elle ne bute pas sur la contrainte `'static` de `show_viewport_deferred` ; le dire dans le compte rendu (« j'ai factorisé X » ou « j'ai dû modifier les deux écritures parce que Y »).
- **Calque** (#167) : un calque s'identifie par son indice dans `PlotSpec::panels`, jamais par `(row, column)` (plusieurs calques se superposent dans la même case, et le rang dans `DrawnPlot::panels` saute les calques trop petits). Une action sans cible cliquée (dépôt, « Send to », Add Text, New Layer) prend le **calque actif** (#161), publié dans le `Context` et lu par `graph_window::published_active_panel`, identique en fenêtre détachée ; un geste sur un objet porte l'indice de son calque jusqu'à `PlotEdit::selected_panel`. Un geste qui ajoute des séries réutilise l'`Action` du dépôt (`state::drop_payload_action` pour des colonnes, `drop_rows_action` pour des lignes à x', #166) : aucune seconde voie d'ajout de série.
- **Échec d'un geste de dialogue** : `AppState::fail` l'écrit d'office dans le pied tant qu'un dialogue de contenu est ouvert (`dialog_error`, S-M7), jamais dans `status` ; un test d'état lit `state.dialog_error()`, et une décision « le geste a-t-il échoué ? » lit `last_action_failed`, jamais `status().is_error`.
- **Menu contextuel de la grille** : « Coller » n'est jamais grisé (sonder le presse-papiers système à chaque image coûterait plus que de tenter au clic) ; les autres écarts délibérés entre menu et clavier sont dans `docs/application/grille.md`, « Menus contextuels de la grille ». Un test de bascule de menu fait deux clics dans le même menu, puis une action après repeinture sans événement : ni la feuille ni ses parents ne doivent rester ouverts (`PopupCloseBehavior::CloseOnClickOutside`).

## Séries et groupes

- Séries (#177–#179) : les séries ajoutées ensemble forment un groupe (`SeriesSpec::group`, `PanelSpec::push_series_batch` — tout nouvel ajout multiple de séries passe par là) ; elles se règlent dans leur propre fenêtre, `app/curve_settings.rs` (onglet `PlotSettingsTab::Series` hors de la bande du graphe), et les nuanciers sont `stokesforge_plot::gradient`. Un groupe (#197) a un **nom** stocké (`PanelSpec::group_names`), reste **contigu** dans `PanelSpec::series` et se manipule par les méthodes de `PanelSpec` (`set_series_group`, `dissolve_group`, `remove_group`, `move_group`…), jamais en réécrivant `group` à la main (le retrait d'une série seule est `PanelSpec::remove_series`, #257, jamais `panel.series.remove` dans l'app) ; il est un nœud parent de l'Object Manager et de la liste des séries, tous deux projetés par `PanelSpec::slots`. Une aide au survol d'un champ passe par `app/hover_help.rs` (`hover_help`, `formula_field` pour une formule), jamais par un mémo repliable.

## Copie d'objets et de figures

- Copie d’objets (#105–106) : `NodeCopy` et `InsertCopy` sont génériques, atomiques, avec identifiants de destination neufs. Noms uniques par dossier, chemins canoniques enracinés ; un nom seul ambigu est refusé. Le presse-papiers système réutilise les conversions `.stf`, sans numéro de version ; graphes entre projets avec colonnes embarquées dans un classeur caché persisté (#107), révélé par Show Data. F2 renomme dans les gestionnaires (#130) et édite dans la grille (#143), selon le focus, jamais le nom des feuilles, Ctrl+D des feuilles reste propre au viewport du classeur.
- Copie de cellules (#259) : le TSV du presse-papiers (format Excel/LibreOffice, cases entre guillemets si tab, saut de ligne ou guillemet) vit dans `stokesforge-core/src/tsv.rs` (`encode`/`decode`), pas dans l'app ; l'app ne choisit que les cases copiées. Une case manquante se vide **avant** la mise en texte (`cell_copy_text_in`), jamais par `.replace(MISSING_TEXT_DISPLAY, "")` après coup (efface les espaces insécables tapés).
- Copie de la figure d'un graphe (#149) : Ctrl+C sur le graphique actif **sans objet sélectionné** et hors champ de saisie, ou Graph › Export › Copy Graph, offre SVG + PNG + PDF ensemble par `stokesforge-clipboard` (`app/figure_copy.rs`) ; le graphique est « Graph » et jamais « Figure » dans l'interface. Ni commande ni historique. Wayland réel, Windows et macOS ne sont pas exécutés par les tests (`docs/application/graphe.md`).

## Analyse (#181)

- Analyse (#181) : un calcul (`Tree::calculations`, lien opération → séries → colonnes de sortie) n'est pas un nœud ; sa création, sa modification et sa suppression sont des commandes annulables (`AddCalculation`/`UpdateCalculation`/`RemoveCalculation`), le recalcul est dérivé (hors historique, comme les formules) et sa trace vit dans `outputs`, jamais sur la colonne. Une colonne calculée est en lecture seule (cellules, type, suppression de cellules ; ses métadonnées hors formule restent libres). Les plafonds des réglages (degré, itérations, bruit) sont vérifiés par `Operation::validate` dans `ops::run`, pour l’interface, `pyst` et le fichier. **Bornes et prérequis (#260) : une seule source, `stokesforge-analysis`.** Les plafonds sont des constantes publiques de `ops` (`MAX_DEGREE`, `MAX_ITERATIONS`, `MAX_PEAKS` = 100, `MAX_COMPONENTS`) que `validate`, `deconv` et `Model::compile` (qui refuse un degré hors plafond, avant toute allocation : un degré lu des préférences ne doit rien fabriquer) lisent, et que la table des grandeurs (`quantities.rs`) lit aussi : jamais un littéral `9`, `50`, `4` ou `5000` dans un `between(…)`. Ce qui manque avant un calcul sans lire de donnée (points, zone, modèle illisible ou sans paramètre) est `Operation::readiness()`, un `enum` de données (`Readiness`) que l'app met en cas par `analysis::issue_of`, `match` exhaustif sans `_` ; le brouillon du formulaire se décrit en `Operation` même incomplet (`draft_operation` : un point ou une zone pas encore posé est un `NaN`). Ce qui lit le `Tree` (`NoSeries`, `NeedsReference`/`NeedsResponse`, `MissingColumn`) reste dans l'app. L'estimation automatique des valeurs de départ est `FitSpec::auto_initial(series, range)` et « oublier les positions » (dernières analyses) `Operation::forget_positions()` : l'app ne réécrit aucune des deux. `pyst` peut lire `readiness()` pour refuser avant le calcul avec la même règle (non câblé aujourd'hui). Le formulaire est `stokesforge-app-state/src/analysis.rs` (sans egui), la fenêtre et le volet Results dans `src/app/analysis_*.rs`/`results_pane.rs`, `pyst.analysis` dans `stokesforge-script/src/analysis.rs`. Catégories et dernières analyses (#248, #249) : le menu range les méthodes par `Category` (`analysis.rs`), une fenêtre par catégorie ; les trois dernières analyses (`analysis_recent.rs`) vivent dans les préférences, jamais dans le `.stf` ; le clic d'une catégorie du clic droit rouvre la dernière méthode de la catégorie (`analysis_menu::category_preset`, publiée dans le `Context`). Les rangées d'en-tête Metadata / x' se règlent par classeur (mémoire d'egui, `grid::header_rows`) et, pour les nouveaux classeurs, par préférence (`workbook_show_*`) : un réglage de vue par document suit ce motif, jamais un drapeau global. La référence d'une `Correct` est figée dans le classeur caché des copies (`stokesforge-core/src/analysis_store.rs`) et les originaux d'un remplacement y sont gardés (`Command::RestoreOriginalData`). Ajouter une opération : crate d'analyse → `compute` → formulaire → clés `analysis-*` en/fr → `pyst.analysis` → `docs/tests.md`. Nom des colonnes de sortie (#256) : **une seule règle**, `stokesforge_core::OutputName` (`of`, `english`), avec `output_unit` (toujours l'unité de la source), `family_english` (commentaire) et `wants_output` (quelle sortie est écrite, `PeakOutputs` pour les deux cases des pics) ; l'app la traduit par `output_name_text_in`, `match` exhaustif sans `_` vers `analysis-output-*`, `pyst` écrit `english()`. Jamais de `format!` de nom de sortie ailleurs ; une nouvelle sortie = une variante, sa clé `en`/`fr`, une ligne dans `tests/output_names.rs` (`rank` et exemples). Le nom reste figé à la création (invariant 11 : le recalcul ne réécrit que les données).


- Corrections définitives (#212) : `ReplaceAnalysisData` écrit les Y et leur
  `CorrectionRecord` dans le même pas d'historique. La trace est séparée des
  calculs, persistée dans `correction_traces`, immuable et conservée après
  saisie ; une colonne corrigée plusieurs fois garde **une seule** donnée
  brute (celle d'avant la première correction, #308) et « Restore original
  data » la rend quelle que soit la trace, en retirant la colonne de toutes ; ne jamais créer un calcul dont la sortie est sa propre source.
  Le choix de destination utilise `ChoiceOption::disabled` pour les sources
  en lecture seule ; les sources tracées plusieurs fois exigent `modal_confirm`.

## Erreurs : texte brut anglais en bas, phrase du catalogue dans l'application

Le `Display` (`#[error(...)]`) des crates du dessous (core, commands,
persistence, import, script, render-svg…) est **de l'anglais brut**, concis et
lisible par un utilisateur du CLI ou de `pyst` : il n'est jamais traduit et aucun
de ces crates ne dépend de Fluent. L'application met chaque erreur affichée en
phrase par `crates/stokesforge-app-state/src/errors.rs` :

- une fonction `error_message(&Erreur) -> String` par famille
  (`errors::model`, `commands`, `session`, `file`, `import`, `formula`,
  `render`, `export`, `script` sous la feature `python`) ;
- chaque variante → une clé `error-<famille>-<cause>` du catalogue `en`, ses
  champs en arguments nommés (`tr!("error-file-corrupt", path = …, detail = …)`) ;
- le `match` est **exhaustif, sans `_`** : une variante nouvelle ne compile pas
  tant qu'on ne l'a pas associée. À défaut de phrase, `errors::fallback` renvoie
  son `Display` anglais (rien ne disparaît) ;
- le test de la famille construit une valeur de chaque variante et vérifie
  phrase non vide, sans jargon (S-R9), sans point final : en ajouter une pour la
  variante nouvelle ;
- un `detail` (cause décrite par le crate d'origine) ou un message du système
  est repris tel quel, en anglais ;
- **jamais** `to_string()` ni `{err}` d'une erreur montrée à l'utilisateur.
- une `std::io::Error` passe par `errors::io::error_message(&err)` (clés
  `error-io-*`), et le message de contexte la reçoit en variable
  (`tr!("error-template-export", cause = …)`). `ErrorKind` est
  `#[non_exhaustive]` : cette famille garde un repli `_` (`error-io-other`),
  seule exception à « sans `_` ».
- les messages de la barre d'état (`AppState::notice` / `warn` / `fail`) passent par
  `tr!` (clés `status-*` pour une information ou un avertissement, `error-*`
  pour un échec) ; la garde `aucun_libelle_en_dur` couvre `notice`, `warn` et
  `fail`. Une exception de script se dit « Script failed. See the console » (S-M10).

Même schéma pour les **noms de colonnes de sortie d'analyse** (#256) : le cœur
rend une donnée (`stokesforge_core::OutputName`, avec `english()`), l'application
la met en phrase par `analysis::output_name_text_in` (`match` exhaustif sans `_`
vers `analysis-output-*`), et `tests/output_names.rs` garde l'accord de
l'anglais du catalogue avec `english()` : changer un de ces textes, c'est changer
les deux.

Rédaction : ce qui s'est passé, puis quoi faire seulement si c'est utile ;
Sentence case, pas de point final sur une ligne unique, ni « please », ni
excuse, ni jargon interne (*nœud, session, macro, commande, arbre*). Un message
faux ou ambigu se corrige dans le catalogue (`en`), pas dans le crate du dessous
— sauf si c'est le texte brut anglais lui-même qui est faux.

## Script `pyst`

- `pyst` : les écritures de #67 réutilisent les commandes, dans une macro `Script` ouverte à la première mutation et avortée sur erreur non rattrapée/interruption. Le moteur possède sa propre identité de client ; les poignées sont revalidées après réservation. `set_values` remplace longueur et cellules sans changer le type ni la provenance ; `set_column` reste strict. `undo()` annule le groupe courant et fait un point de validation. Les objets graphiques (`add_shape`, `remove_shape`, #76) rejoignent également ce groupe, avec relecture du spec sous réservation. Contrat : `stokesforge-script/tests/mutations.rs`, `tests/graphs.rs`, `docs/python-et-script.md`.
- `pyst` : la navigation (`root`, `children`, `info`) lit un seul instantané par appel et conserve son époque dans toutes les poignées rendues. `column` garde sa vue numpy numérique sans copie ; `values` copie les cellules en `float`/`str`/`None`, sans analyser les textes ni confondre vide, chaîne vide et NaN. Contrat : `docs/python-et-script.md`, `stokesforge-script/tests/navigation.rs` (#66).

## Rendu : pièges silencieux

- Rendu : `plotters` est pris **sans ses fonts** (`default-features = false`). Un `DrawingBackend` qui ne surcharge pas `draw_text` **panique** à la première graduation, et un qui ne surcharge pas `estimate_text_size` mesure le texte au nombre d'octets UTF-8 — silencieusement. Toute mesure passe par `stokesforge_render::text` et par nulle part ailleurs (`architecture-choisie.md` §4.3/§4.4).
- Rendu : **une fonctionnalité de tracé n'entre jamais dans un seul backend** — elle va dans `stokesforge-render`, qui est générique sur le backend. Corollaire du même piège : les éléments de Plotters qui mesurent du texte eux-mêmes (`configure_series_labels`, `MultiLineText`) passent par `FontData::estimate_layout`, donc par l'heuristique naïve, **pas** par `estimate_text_size` — la légende est écrite à la main pour cette raison (`stokesforge_render::legend`).
- Rendu : **tout titre, titre d'axe et entrée de légende passe par Typst, les graduations jamais** (issue #32). Le rôle est décidé dans le pipeline par le style (`formula::rich_style`), et les deux backends aiguillent sur `formula::is_rich` à la mesure **et** au dessin — jamais sur la présence de `$`. Ces textes sont donc du markup Typst : un texte repris automatiquement (nom de colonne) doit être échappé en amont.
- Rendu : le levier `TessellationOptions::feathering` vit dans `stokesforge_render_egui::feathering`, en fonction libre sur le `Context`. Il n'est **pas** dans la clé de cache et ne doit pas y entrer (`architecture-choisie.md` §4.2, verrouillé par `stokesforge-render-egui/tests/feathering.rs`). Son déclencheur, le glisser, est câblé dans `stokesforge-render-egui/src/lib.rs` depuis R7.3.
