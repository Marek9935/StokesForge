# Conventions par domaine

Règles détaillées sorties de `CLAUDE.md` pour qu'elles ne soient chargées
qu'au besoin. Elles valent autant que celles de `CLAUDE.md` : lire la section
du domaine **avant** d'écrire du code. La plupart sont aussi verrouillées par
une garde de `stokesforge-app/tests/gardes.rs` ; une garde verte ne dispense
pas de la règle (elle ne voit que ce qu'elle cherche).

## Application (`stokesforge-app`) : briques et gardes

Transitions #72–#73 : Wayland natif est le défaut, `STOKESFORGE_X11=1` force
X11 ; les patches `vendor/winit` et `vendor/smithay-clipboard`, hors workspace,
partagent le récepteur de dépôt et le presse-papiers entre les viewports.
Choix, commandes `rustfmt` ciblées, vérification manuelle et retrait :
`vendor/winit/STOKESFORGE.md`. Le parseur et le canal exacts sont testés par
`stokesforge-app/tests/wayland_{uri,bridge}.rs`.
Le graphique vide passe par `AddGraph` ; `SessionHandle::new_project`, comme
`load`, remplace l'identité et vide l'historique après confirmation dans l'UI.

- `stokesforge-app` : découpage d'`app.rs` (chantier #115, #116–#124 faits) — règle retenue : de **vrais modules** dans `src/app/`, un par surface (table module → rôle dans `docs/application.md`, § Découpage du code), à imports explicites, visibilité minimale (`pub(super)` entre modules d'`app`) ; aucun `use …::*` hors du code de test (`aucun_import_global_hors_des_tests`, `tests/gardes.rs`, sans dérogation) ; **`app.rs` ne garde que l'orchestration** de l'image (`StokesForgeApp` et `new`, `ui()` et l'ordre des panneaux, `collect_shortcuts`, application différée des actions, confirmations bloquantes, drainage des fenêtres détachées) — une entrée de menu va dans `app/menus.rs`, du code de fenêtre dans `app/window_*.rs`, une surface nouvelle dans son propre module (modèle : `object_manager.rs`, `toolbar.rs`, `app/dialogs.rs`). Briques communes : `app/dialogs.rs` (`modal_dialog`, `modal_dialog_with`, `modal_wizard`, `modal_read_only`, `modal_confirm`, `modal_save_changes`, `modal_alert`), `app/tab_strip.rs` (`tab_strip`), `app/shortcuts.rs` (`SHORTCUT_*`), `app/tab.rs` (`Tab`), `app/window_chrome.rs` (cadre, `show_window_in_layer`). **Les tests suivent leur code** (module `tests` de la surface ; celui d'`app.rs` n'éprouve que l'orchestration) et sont écrits/relus par un agent Opus à chaque restructuration, témoins vérifiés par mutation. Fonctions géantes redécoupées sur place (#124), jamais avec un déplacement : conserver l’imbrication des Ui/ID egui et l’ordre des événements ; les contextes privés de peinture ne regroupent que les données ou emprunts utiles à l’image. Toutes les gardes qui lisent le code passent par un lecteur unique, `tests/support/sources.rs` (tout `src/`, récursif ; fonctions cherchées par leur nom ; jamais `include_str!` ni fichier nommé, #116).
- `stokesforge-app` : tout réglage de couleur passe par `color_picker::color_field` avec l'usage de la couleur (`ColorUsage::Curves`/`Text`/`Background`) et le `PickerEnv` des couleurs enregistrées, jamais par un champ texte `#RRGGBB`. Dans une barre d'outils, l'échantillon est un petit carré précédé d'une icône (`color_icon_button`, #89) ; dans un formulaire, `color_field` (libellé + carré). Couleur vide = automatique, peinte en carré barré d'un trait rouge ; opacité en `#RRGGBBAA` ; le texte et les échelles automatiques suivent `stokesforge_render::color::auto_ink` (blanc sur fond sombre).
- `stokesforge-app` : toute fenêtre passe par la brique commune — `show_window_in_layer` (`app/window_chrome.rs`) pour une fenêtre de contenu, `modal_dialog_with` (`app/dialogs.rs`) pour un dialogue, `modal_wizard` pour un assistant, `modal_read_only` (Close seul) pour une fenêtre qui ne change rien, `modal_confirm` / `modal_save_changes` / `modal_alert` pour une confirmation, le trio « Discard Changes · Cancel · Save » et un échec grave (#229 ; seulement pour ce que Ctrl+Z ne rattrape pas, peints depuis `app/confirmations.rs`) (même cadre `modal_frame`, même barre de titre, même blocage que les propriétés du graphe ; pied ordonné, zone de message — erreur de `fail` tant qu'un dialogue est ouvert, sinon raison du principal grisé portée par `accept_disabled` (#226) —, focus initial et géométrie mémorisée par la brique, #228) ; jamais d'`egui::Window` brute, même pour un assistant à étapes (verrouillé par le test `aucune_fenetre_n_est_une_egui_window_brute`). Un code existant qui déroge n'est pas un modèle.
- `stokesforge-app` : toute icône passe par `icons::icon_image` / `icon_button` (Phosphor duotone, rôle de couleur `IconRole`, info-bulle `tr!` obligatoire), jamais par un glyphe Unicode ni un emoji ; couleur rare, règles dans `docs/icones.md` (verrouillé par `aucune_icone_hors_du_composant`). **Chaque action a son logo** (#110) : icône soignée si elle est dans une barre fixe, logo standard de famille à deux lettres sinon (gabarit à venir) ; ça ne force pas l'affichage dans les menus.
- `stokesforge-app` : tout texte d'interface passe par `tr!` avec une clé du catalogue `en`, rédigée selon `docs/textes-interface.md` ; jamais de littéral (verrouillé par `aucun_libelle_en_dur`, y compris les titres `set_title` et noms visibles `add_filter` des boîtes de fichier, #127 ; extensions et sigles de format techniques préservés). Toute clé s'écrit **dans les deux** catalogues (`en` et `fr`) dans le même commit (complétude bloquante). Aucune dérogation : la garde est absolue (#48), et une clé du catalogue qu'aucun appel n'emploie fait échouer `aucune_cle_du_catalogue_n_est_morte`.
- `stokesforge-app` : une barre d'outils est une variante de `toolbar::ToolbarId` dont le contenu s'écrit dans `toolbar::bar_contents` avec des `ToolbarButton` (icône du composant commun, info-bulle `tr!`) qui poussent les `Action` des menus ; jamais un second chemin d'exécution ni un panneau posé à côté. Les barres sont **fixes** (#108) : contenu et place figés dans le code (`ToolbarId::row`), ni déplacement ni mode flottant ; seule leur visibilité (menu View) vit dans `preferences.json`, pas dans le `.stf`. `Tools` et `Text` sont toujours peintes, boutons grisés avec info-bulle sans fenêtre concernée. Le format de texte (`ToolbarId::Text`) écrit des balises Typst dans le texte via `text_format::apply` pour le gras, l'italique, la couleur et le corps d'un passage ; la **police** (et, pour un titre ou la légende, le corps) est un champ de la zone (`TextFont`, `AnnotationSpec::font`, #77/#78), posé par `StokesForgeApp::format_zone` — jamais une balise `#text(font: …)` dans le contenu.
- `stokesforge-app` : aspect commun (#169) — un jeu d'onglets (pages, feuilles, axes) passe par `tab_strip` + `TabStripSide`, jamais par une rangée de boutons ; une entrée de liste ou d'arbre sélectionnable par `list_row` / `ListRowBackground` (`app/list_row.rs`, ligne pleine largeur à filet d'accent) ; le bouton sélectionnable d'egui reste aux choix exclusifs d'un menu, d'une liste déroulante ou du sélecteur de couleur (`aucun_bouton_selectionnable_hors_d_un_menu`). Gras par `theme::bold` seulement (`strong()` interdit) ; titre de section par `theme::section_title`, sans `separator()` juste avant (`aucun_separateur_avant_un_titre_de_section`) ; couleurs d'état et de chrome par `theme.rs` ou les `Visuals`, aucun `Color32::from_…` hors de `theme.rs` et `color_picker.rs` (`aucune_couleur_en_dur_hors_du_theme`) ; un formulaire est une grille à deux colonnes par `app/form.rs` (`form`, `Form::row`/`checkbox`/`note`/`section`, #227), un choix exclusif fixe par `choice` (radios pour 2–3 options, liste dès 4), un choix de menu par `menu_sections::menu_choice` (coche seule), largeurs `theme::FieldWidth`, écarts `theme::SPACE_ITEM`/`SPACE_SECTION` et jamais `add_space(<nombre>)` (`aucun_add_space_litteral_hors_du_theme`).
- `stokesforge-app` : un champ qui s'ouvre sur place passe par `app/inplace_edit.rs` (#224, contrat T4 : tout sélectionné, Entrée/clic ailleurs valident, Échap annule, Tab valide et passe au suivant) ; une saisie de nombre se lit par `stokesforge_core::number_format::parse_input` (virgule décimale, #223), jamais `parse::<f64>` ; le séparateur des graduations est `PlotSpec::decimal_comma`.
- `stokesforge-app` : tout nombre saisi passe par `number_field::number_field` et une grandeur de `quantities.rs` (#220) ; ni `DragValue` ni `Slider` (`aucun_champ_numerique_hors_de_la_brique`). Un défaut de saisie (illisible, hors borne) se peint par `field_state` (#219), jamais à la main.
- `stokesforge-app` : standard de comportement (#217) — champs, messages, dialogues, confirmations, grisage, clavier et souris suivent `docs/standard-interface.md`, source unique (codes N1, F3… de #217). Chaque section y est *en place* ou *à venir* (#219–#232) : une règle à venir vaut déjà pour tout code neuf ; une brique ou une garde à venir n'existe pas encore. Le commit qui en livre une la passe à *en place* et met à jour les skills.
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

- Graphes neufs (#103) : aucun titre par défaut, démo comprise ; `name` nomme le nœud et sa fenêtre, jamais la figure. Garder la possibilité de saisir un titre, sans masquer `title_visible`. Les titres explicites de `stokesforge-render::sample` sont des témoins de rendu, pas des défauts utilisateur.
- Fenêtres détachées (#104) : publier un instantané ne réveille pas le callback `show_viewport_deferred`. `app/window_detach.rs` compare époque, révision métier et révision des géométries, puis demande le redessin du viewport lecteur ; une mutation appliquée après peinture programme l'image racine de publication (fin de `ui()`, `app.rs`). Tests : `app::window_detach::tests`, `app::tests`. Le Project Manager suit aussi sa sélection, son masquage et l'onglet actif. Aucun réveil périodique au repos.

## Séries et groupes

- Séries (#177–#179) : les séries ajoutées ensemble forment un groupe (`SeriesSpec::group`, `PanelSpec::push_series_batch` — tout nouvel ajout multiple de séries passe par là) ; elles se règlent dans leur propre fenêtre, `app/curve_settings.rs` (onglet `PlotSettingsTab::Series` hors de la bande du graphe), et les nuanciers sont `stokesforge_plot::gradient`. Un groupe (#197) a un **nom** stocké (`PanelSpec::group_names`), reste **contigu** dans `PanelSpec::series` et se manipule par les méthodes de `PanelSpec` (`set_series_group`, `dissolve_group`, `remove_group`, `move_group`…), jamais en réécrivant `group` à la main ; il est un nœud parent de l'Object Manager et de la liste des séries, tous deux projetés par `PanelSpec::slots`. Une aide au survol d'un champ passe par `app/hover_help.rs` (`hover_help`, `formula_field` pour une formule), jamais par un mémo repliable.

## Copie d'objets et de figures

- Copie d’objets (#105–106) : `NodeCopy` et `InsertCopy` sont génériques, atomiques, avec identifiants de destination neufs. Noms uniques par dossier, chemins canoniques enracinés ; un nom seul ambigu est refusé. Le presse-papiers système réutilise les conversions `.stf`, sans numéro de version ; graphes entre projets avec colonnes embarquées dans un classeur caché persisté (#107), révélé par Show Data. F2 renomme dans les gestionnaires (#130) et édite dans la grille (#143), selon le focus, jamais le nom des feuilles, Ctrl+D des feuilles reste propre au viewport du classeur.
- Copie de la figure d'un graphe (#149) : Ctrl+C sur le graphique actif **sans objet sélectionné** et hors champ de saisie, ou Graph › Export › Copy Graph, offre SVG + PNG + PDF ensemble par `stokesforge-clipboard` (`app/figure_copy.rs`) ; le graphique est « Graph » et jamais « Figure » dans l'interface. Ni commande ni historique. Wayland réel, Windows et macOS ne sont pas exécutés par les tests (`docs/application.md`).

## Analyse (#181)

- Analyse (#181) : un calcul (`Tree::calculations`, lien opération → séries → colonnes de sortie) n'est pas un nœud ; sa création, sa modification et sa suppression sont des commandes annulables (`AddCalculation`/`UpdateCalculation`/`RemoveCalculation`), le recalcul est dérivé (hors historique, comme les formules) et sa trace vit dans `outputs`, jamais sur la colonne. Une colonne calculée est en lecture seule (cellules, type, suppression de cellules ; ses métadonnées hors formule restent libres). Les plafonds des réglages (degré, itérations, bruit) sont vérifiés par `Operation::validate` dans `ops::run`, pour l’interface, `pyst` et le fichier. Le formulaire est `stokesforge-app/src/analysis.rs` (sans egui), la fenêtre et le volet Results dans `src/app/analysis_*.rs`/`results_pane.rs`, `pyst.analysis` dans `stokesforge-script/src/analysis.rs`. Catégories et dernières analyses (#248, #249) : le menu range les méthodes par `Category` (`analysis.rs`), une fenêtre par catégorie ; les trois dernières analyses (`analysis_recent.rs`) vivent dans les préférences, jamais dans le `.stf`. La référence d'une `Correct` est figée dans le classeur caché des copies (`stokesforge-core/src/analysis_store.rs`) et les originaux d'un remplacement y sont gardés (`Command::RestoreOriginalData`). Ajouter une opération : crate d'analyse → `compute` → formulaire → clés `analysis-*` en/fr → `pyst.analysis` → `docs/tests.md`.


- Corrections définitives (#212) : `ReplaceAnalysisData` écrit les Y et leur
  `CorrectionRecord` dans le même pas d'historique. La trace est séparée des
  calculs, persistée dans `correction_traces`, immuable et conservée après
  saisie ; ne jamais créer un calcul dont la sortie est sa propre source.
  Le choix de destination utilise `ChoiceOption::disabled` pour les sources
  en lecture seule ; les sources tracées plusieurs fois exigent `modal_confirm`.

## Script `pyst`

- `pyst` : les écritures de #67 réutilisent les commandes, dans une macro `Script` ouverte à la première mutation et avortée sur erreur non rattrapée/interruption. Le moteur possède sa propre identité de client ; les poignées sont revalidées après réservation. `set_values` remplace longueur et cellules sans changer le type ni la provenance ; `set_column` reste strict. `undo()` annule le groupe courant et fait un point de validation. Les objets graphiques (`add_shape`, `remove_shape`, #76) rejoignent également ce groupe, avec relecture du spec sous réservation. Contrat : `stokesforge-script/tests/mutations.rs`, `tests/graphs.rs`, `docs/python-et-script.md`.
- `pyst` : la navigation (`root`, `children`, `info`) lit un seul instantané par appel et conserve son époque dans toutes les poignées rendues. `column` garde sa vue numpy numérique sans copie ; `values` copie les cellules en `float`/`str`/`None`, sans analyser les textes ni confondre vide, chaîne vide et NaN. Contrat : `docs/python-et-script.md`, `stokesforge-script/tests/navigation.rs` (#66).

## Rendu : pièges silencieux

- Rendu : `plotters` est pris **sans ses fonts** (`default-features = false`). Un `DrawingBackend` qui ne surcharge pas `draw_text` **panique** à la première graduation, et un qui ne surcharge pas `estimate_text_size` mesure le texte au nombre d'octets UTF-8 — silencieusement. Toute mesure passe par `stokesforge_render::text` et par nulle part ailleurs (`architecture-choisie.md` §4.3/§4.4).
- Rendu : **une fonctionnalité de tracé n'entre jamais dans un seul backend** — elle va dans `stokesforge-render`, qui est générique sur le backend. Corollaire du même piège : les éléments de Plotters qui mesurent du texte eux-mêmes (`configure_series_labels`, `MultiLineText`) passent par `FontData::estimate_layout`, donc par l'heuristique naïve, **pas** par `estimate_text_size` — la légende est écrite à la main pour cette raison (`stokesforge_render::legend`).
- Rendu : **tout titre, titre d'axe et entrée de légende passe par Typst, les graduations jamais** (issue #32). Le rôle est décidé dans le pipeline par le style (`formula::rich_style`), et les deux backends aiguillent sur `formula::is_rich` à la mesure **et** au dessin — jamais sur la présence de `$`. Ces textes sont donc du markup Typst : un texte repris automatiquement (nom de colonne) doit être échappé en amont.
- Rendu : le levier `TessellationOptions::feathering` vit dans `stokesforge_render_egui::feathering`, en fonction libre sur le `Context`. Il n'est **pas** dans la clé de cache et ne doit pas y entrer (`architecture-choisie.md` §4.2, verrouillé par `stokesforge-render-egui/tests/feathering.rs`). Son déclencheur, le glisser, est câblé dans `stokesforge-render-egui/src/lib.rs` depuis R7.3.
