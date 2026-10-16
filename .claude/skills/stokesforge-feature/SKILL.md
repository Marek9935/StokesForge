---
name: stokesforge-feature
description: Implémenter une nouvelle fonctionnalité dans StokesForge (portage Rust, branche dev) — du choix de la couche jusqu'au menu contextuel, au raccourci clavier, aux tests et à la mise à jour de la doc. À utiliser quand l'utilisateur demande de réaliser une fonctionnalité dans le dépôt, y compris pour un changement d'une seule ligne. Dans un contexte de signalement ou de création d'issues, « ajoute », « je voudrais pouvoir… », « il manque… » ou « fais que… » restent des demandes à consigner tant que le passage à l'implémentation n'est pas explicite.
---

# Implémenter une fonctionnalité dans StokesForge

StokesForge est un tableur/traceur scientifique façon OriginLab. Une fonctionnalité
y touche presque toujours **plus d'une couche** : ce skill sert à ne pas en
oublier une, et surtout à ne pas la câbler au mauvais endroit.

## Avant de commencer : vérifier le contexte

Si la conversation porte sur la création ou l'enrichissement d'issues, suivre
la [règle de contexte du triage](../stokesforge-bug-report/SKILL.md#avant-tout--conserver-le-contexte-de-signalement).
Une demande d'ajout, une capture ou la validation d'une issue ne déclenche
pas une implémentation opportuniste. Ce skill s'applique alors seulement
après une demande explicite de réaliser le changement dans le dépôt.

## 1. Avant d'écrire une ligne

Dans cet ordre, sans sauter d'étape :

1. `git status --short` puis `git diff` sur les fichiers visés — l'utilisateur
   a souvent du travail en cours qu'il ne faut ni écraser ni committer.
2. Ouvrir la page `docs/` du domaine, **pas** le workspace entier.
   `.claude/rules.md` dit exactement quoi charger et quoi ne pas ouvrir.
   `docs/invariants.md` vaut pour tous les domaines. Lire aussi la section
   du domaine dans `.claude/conventions.md` (table dans `CLAUDE.md`).
3. Chercher l'existant (`rg`) : une fonctionnalité voisine, un helper, un
   `Action`, une `Command` qui fait déjà 80 % du travail. Réutiliser bat
   écrire, et le dépôt a déjà payé pour ses abstractions.
4. **Vérifier que ce n'est pas un plafond déjà arbitré.** Longueur de nom,
   `MAX_NODE_ID`, `MAX_CELLS`, nombre de classes d'histogramme, limites
   d'historique : recensés dans `docs/archive/rustrewrite/questions-ouvertes.md`
   §11. Une « correction » qui les lève est une régression.
5. **Lever les imprécisions maintenant, pas à mi-parcours.** Une demande
   arrivée par une todo ou un vrac de retours est rarement complète : elle dit
   le geste voulu, pas son comportement aux bords. Poser les questions
   ouvertes **avant** d'écrire (voir « Demander plutôt que supposer »), en une
   seule fois.
6. Annoncer brièvement : ce qui sera réutilisé, la zone modifiée, les
   vérifications prévues. Puis coder sans redemander confirmation pour les
   choix courants.

Une note de chantier (`docs/chantier-*.md`, `docs/chantiers/`) ne prouve ni
qu'une fonctionnalité existe, ni qu'elle reste à faire. **Le code fait foi.**

## 2. Choisir la couche — la question qui coûte le plus cher

| Ce que fait la fonctionnalité | Où elle vit | Piège |
|---|---|---|
| change le **modèle** (colonnes, arbre, `PlotSpec`, texte de note…) | une `Command` de `stokesforge-commands` | jamais une primitive `stokesforge-core` en direct (invariant 1) ; voir le skill **stokesforge-commande** |
| est un **réglage de vue propre à un document** (rangées d'en-tête d'un classeur) | mémoire d'egui sous l'identifiant du nœud, défaut des nouveaux documents en préférence publiée dans le `Context` (`grid::header_rows`) | jamais un drapeau global : tous les classeurs le suivraient ; rien dans le modèle ni le `.stf` |
| change le **cadrage de vue** (zoom, pan) | `ViewState` sur `GraphView` | ce n'est **pas** une mutation du modèle : personne ne veut annuler un zoom par Ctrl+Z |
| est une **décision** d'interface (quoi proposer, quoi griser, comment nommer) | `stokesforge-app-state/src/state.rs` (crate **sans egui**, #263 : un `use egui` n'y compile pas) | doit rester testable sans fenêtre |
| est du **dessin ou de l'événement** egui | le module de **la surface** dans `stokesforge-app/src/app/` (grille `grid.rs`, classeur `workbook.rs`, graphe `graph_window.rs`, menus `menus.rs`, barres de format `format.rs`, dialogue `plot_settings.rs`…, table complète dans `docs/application.md`, § Découpage du code ; voir § 2 octies) — `app.rs` n'en reçoit que l'appel | rien de ce qu'ils contiennent n'est une décision de produit ; jamais une logique de surface dans `app.rs` |
| est une **fonctionnalité de tracé** | `stokesforge-render` (générique sur le backend) | jamais dans un seul backend (invariant 5) ; voir le skill **stokesforge-rendu** |
| lit un **fichier de données** | `stokesforge-import` | reste indépendant de l'UI, de `core`, des commandes et de la session |

Une action qui a besoin de l'**état de vue** (le zoom d'une `GraphView`, qui
vit dans `StokesForgeApp::graph_views` ou la vue détachée, jamais dans
`AppState`) : la couche `app` le lit et le **met dans l'`Action`**, `state.rs` (de `stokesforge-app-state`) fait le travail et le dit dans la barre d'état. Modèle :
`Action::ExportGraph { path, spec }`, dont le spec est déjà cadré par
`GraphView::framed_spec` (issue #34) — testable dans `stokesforge-app-state/tests/shell.rs` sans
fenêtre, et identique depuis une fenêtre détachée.

Un **geste continu sur un graphe** (glisser, édition sur place) n'émet pas une
action par image : l'aperçu vit dans la `GraphView`, l'état d'un éditeur dans
la mémoire d'egui sous `GraphView::id()` (donc identique en fenêtre détachée),
et une seule action part à la validation. Modèle : textes libres, issue #35
(`app::graph_window::annotation_interactions` ; les éditeurs sur place dans
`app/graph_text_edit.rs`). Le champ de saisie d'un tel éditeur s'écrit
**dans le cadre du rendu** (centre, corps, sans rotation : une fonction pure de
placement, testée sans fenêtre — `stokesforge_render_egui::annotation_field`, issue
#37), et ne masque pas l'aperçu quand on règle autre chose que le texte (#36).

**Un calque s'identifie par son indice dans `PlotSpec::panels`** (#167),
jamais par `(row, column)` : plusieurs calques se superposent dans la même
case (`OverlaySpec`, Graph › New Layer), et le rang dans `DrawnPlot::panels`
saute les calques trop petits. Une action sans cible cliquée (dépôt, « Send
to », Add Text, New Layer) prend le **calque actif** du graphe (#161), publié
dans le `Context` et lu par `graph_window::published_active_panel` — identique
en fenêtre détachée ; un geste sur un objet (double-clic sur un axe, Object
Manager) porte l'indice de son calque jusqu'à `PlotEdit::selected_panel`.

Un geste qui ajoute des séries à un graphe **réutilise l'`Action` du dépôt** :
`state::drop_payload_action` (colonnes) ou `drop_rows_action` (lignes à x',
#166). « Send to » (`app/graph_targets.rs`, #165) en est le modèle — la liste
des graphiques ouverts est publiée dans le `Context` à chaque image, seul canal
commun aux viewports. Aucune seconde voie d'ajout de série.

La ligne de partage `stokesforge-app-state` / `stokesforge-app` (`app.rs` et `src/app/`) est la colonne
vertébrale de l'interface, tenue par le compilateur (#263) : `state` porte toute la logique, pilotée par un enum
`Action`, et se teste en CI (`stokesforge-app-state/tests/shell.rs`) ; `app` ne fait que traduire
clavier et souris en `Action` et peindre. Une fenêtre ne se teste pas en
intégration continue — c'est toute la raison du découpage. Si une règle
métier se retrouve dans `app`, elle devient invérifiable.

Un graphe neuf n'a aucun titre par défaut (#103), démo de l'application
comprise : garder `PlotSpec::title` vide, sans masquer `title_visible` ni
reprendre le nom du nœud comme titre. Le titre reste saisissable par les
propriétés. Les titres explicites de `stokesforge-render::sample` servent
aux tests et bancs de rendu ; ils ne définissent pas le défaut utilisateur.

## 2 bis. Le doublon docké / détaché — à vérifier avant de chiffrer

Les contenus de `stokesforge-app` ont longtemps existé **en deux écritures** : une
pour la fenêtre flottante, une pour la fenêtre détachée. Depuis le 2026-09-30,
classeur, graphe, note et matrice n'en ont plus qu'une (`book_contents`,
de `src/app/workbook.rs`, `graph_view_contents` dans `app/graph_window.rs`,
`note_contents` et `matrix_contents` dans `app/contents.rs`) — **ne pas en
réintroduire** : une nouvelle nature de fenêtre s'écrit une fois et s'appelle
des deux côtés, sur le motif du classeur (vue `AppState` locale,
`fork_for_detached_view`).

La cause est documentée dans `docs/application.md` : la fermeture `'static`,
`Send + Sync` de `show_viewport_deferred` ne peut pas capturer `&mut AppState`.
Elle est contournée par une vue `AppState` locale à la fenêtre détachée, qui
renvoie ses actions à la racine.

Avant de toucher à une surface d'interface :

```sh
rg "fn <nom>|fn detached_<nom>" crates/stokesforge-app/src/ crates/stokesforge-app-state/src/
```

Deux résultats → le changement se fait deux fois, ou mieux : **se factorise**.

Une seule écriture ne suffit pas : la vue détachée a **son propre**
`AppState`. Un état d'interface ajouté à `AppState` (édition en cours,
confirmation, renommage…) n'y existe que s'il est recopié par
`fork_for_detached_view`, resynchronisé par `sync_detached_snapshot` /
`sync_detached_sheet_editor` ou
rapatrié par `adopt_detached_book_view` ; sinon le geste marche en flottant
et pas en détaché (constaté sur le renommage de feuille, #106). De même,
`collect_shortcuts` lit l'entrée du viewport racine : un raccourci global
n'agit pas dans une fenêtre OS détachée sans câblage propre, et
`active_book` retombe sur « le seul classeur détaché » même quand un graphe
est actif — un raccourci de classeur peut y voler une touche à un autre
contexte (F2 de l'Object Manager).
Une factorisation supplémentaire est légitime quand elle ne bute pas sur la
contrainte `'static` ci-dessus — un bandeau de boutons qui ne fait que pousser
des `Action` n'y bute pas. Le dire explicitement dans le compte rendu : « j'ai
factorisé X » ou « j'ai dû modifier les deux écritures parce que Y ».

F2 renomme dans les gestionnaires (#130) et édite la case en haut à gauche
de la sélection dans la grille (#143), cellule de données ou champ d'en-tête,
selon le focus ; jamais le nom des feuilles. Ctrl+D des
feuilles se lit dans le viewport du classeur, sans secours vers un classeur
détaché quand un graphe est actif. Les éditeurs et le focus du Project Manager
sont indexés par viewport dans la mémoire d'egui ; calculer les IDs **avant**
un `Context::data_mut`, sans rappel au contexte sous son verrou.

Copier un objet (#105) réutilise `NodeCopy` / `InsertCopy`, quelle que soit sa
nature. Les données sont figées au geste Copier ; la publication système passe
par egui, le collage par `RequestPaste`. Un nom est unique dans le dossier
parent, le chemin canonique porte ce dossier. Un graphe dans son projet garde
ses références ; entre projets, ses colonnes sont embarquées dans un classeur caché
persisté (#107), révélé par Show Data.

## 2 octies. Où ranger du code d'interface (`app.rs` et ses modules)

`app.rs` a été découpé en **vrais modules** dans `src/app/` (chantier #115,
#116–#124 : déplacements puis redécoupe des fonctions géantes, chacune dans
son propre commit et jamais avec un déplacement). La table module → rôle est dans
`docs/application.md`, § Découpage du code. Les règles, tenues par des gardes
ou par la relecture :

- **un module par surface** : grille `grid.rs`, classeur `workbook.rs`,
  graphe `graph_window.rs` et `graph_text_edit.rs`, Object Manager peint
  `object_manager_ui.rs`, Project Manager `project_manager.rs`, console
  `script_console.rs`, dépôt `file_drop.rs`, contenus partagés `contents.rs`,
  menus et barre d'état `menus.rs`, barres de format `format.rs`, dialogues
  `import_wizard.rs` et `plot_settings.rs`, « Send to » `graph_targets.rs`,
  copie de la figure `figure_copy.rs` ; fenêtres : `window_chrome.rs`
  (cadre commun), `window_placement.rs`, `window_stack.rs` (pile, repli,
  maximisation, `open_tab`), `window_detach.rs` ; briques communes :
  `dialogs.rs`, `tab_strip.rs`, `list_row.rs`, `menu_sections.rs`,
  `shortcuts.rs`, `tab.rs` ; aides de test : `test_support.rs`. Une entrée de menu
  s'écrit dans `menus.rs`, un bouton de barre dans `crate::toolbar` /
  `format.rs`, du code de fenêtre dans le `window_*.rs` concerné — jamais dans
  `ui()` ;
- **imports explicites**, aucun `use …::*` hors du code de test
  (`aucun_import_global_hors_des_tests`, `tests/gardes.rs`, absolue) ;
  visibilité minimale : `pub(super)` pour ce que partagent les modules
  d'`app`. Un champ de `StokesForgeApp` reste privé (un module enfant d'`app`
  le voit) ; seule une méthode appelée depuis un autre module devient
  `pub(super)`. Sens des dépendances : `app.rs` → surfaces → briques communes ;
- **restent dans `app.rs`** (l'orchestration, liste et raisons dans #115) :
  `StokesForgeApp` et `new`, l'ordre des panneaux dans `ui()` (tenu par
  `les_panneaux_fixes_se_posent_dans_l_ordre_de_ui`), les emprunts disjoints,
  l'application différée des actions après la peinture, les confirmations
  bloquantes, `collect_shortcuts` et le drainage des fenêtres détachées ;
- **redécouper sur place** : les phases privées de `ui()` restent dans
  `app.rs`, un menu dans `menus.rs`, les sections de grille, dialogues et
  arbres dans leur surface. Une extraction n'ajoute aucun `ui.scope` /
  `push_id` : conserver les mêmes fermetures, identifiants egui et ordre des
  événements. Si les emprunts disjoints empêchent une méthode sur `&mut self`,
  regrouper seulement les champs utiles dans un contexte privé de peinture,
  limité à l'image ; aucun nouvel état métier. Comparer les formes et les
  captures avant/après, et exercer les décisions extraites avec leurs témoins ;
- **une surface nouvelle** s'écrit d'emblée dans son propre module à imports
  explicites et doc de tête (modèle : `object_manager.rs`, `toolbar.rs`,
  `app/dialogs.rs`), puis s'ajoute à la table de `docs/application.md` ;
- **les tests suivent leur code** : le test d'une surface vit dans le module
  `tests` de son fichier (accès au privé par `use super::*`, seul endroit où
  il est permis) ; celui d'`app.rs` n'éprouve que l'orchestration. Une aide de
  test partagée est `pub(in crate::app)` dans le module de tests qui la porte
  (`graph_window::tests::graphe`, `project_manager::tests::click_at`,
  `window_chrome::tests::find_icon_centers`). À chaque restructuration, les
  tests sont relus : l'agent `perimetre` (Opus) fixe les cas et leurs
  témoins, Sonnet les écrit et vérifie chaque témoin par mutation ;
- les gardes qui lisent le code parcourent tout `src/`, sous-dossiers
  compris, ceux des deux crates app (lecteur unique `stokesforge-app/tests/support/sources.rs`, #116, #263) : un module
  déplacé reste gardé sans rien déclarer. Une fonction qu'une garde attend
  (`import_window`, `modal_dialog_with`, une entrée de `ZONE_43`…) et qu'on
  renomme fait échouer la garde : mettre la garde à jour, ne pas la
  contourner.

## 2 ter. Barres de défilement : le standard du thème

Toute `egui::ScrollArea` (ou `TableBuilder`) hérite du style global posé par
`theme::install` (`theme::scroll_style`). Ne **jamais** écrire
`ScrollStyle::solid()` ni `spacing_mut().scroll = …` ailleurs que dans
`theme.rs` : le test `aucun_scroll_style_hors_du_theme` échoue. Pour réserver
la place d'une barre (barre déportée, bande latérale), lire
`theme::scroll_bar_allocated_width()`. Changer l'aspect de toutes les barres =
changer les constantes `SCROLL_BAR_*` de `theme.rs`, et rien d'autre. Dans un
test egui qui dépend de l'aspect ou de la place des barres, appeler
`theme::install(&context)` : un `Context::default()` a le style d'egui, pas
celui de StokesForge.

## 2 ter bis. Onglets, listes, gras, couleurs : l'aspect commun (#169)

Quatre rôles, quatre aspects, chacun par sa brique (tableau complet dans
`docs/application.md`, § Onglets, listes et sélection) :

- **jeu d'onglets** (pages, feuilles, axes, toute rangée qui change la page
  affichée dessous) : `tab_strip` / `tab_strip_with` avec `TabStripSide`
  (`Top` si la bande est au-dessus du contenu, `Bottom` sinon) — jamais une
  rangée de `selectable_label` ;
- **entrée de liste ou d'arbre** sélectionnable : `list_row`
  (`src/app/list_row.rs`), ou, pour une ligne composée de plusieurs widgets
  (case, échantillon, nom), `ListRowBackground::reserve` avant les widgets,
  `list_row_label` pour le nom, `ListRowBackground::paint` après — jamais un
  bouton sélectionnable au fond arrondi ;
- **choix exclusif** dans un menu, une liste déroulante (`show_ui`) ou le
  sélecteur de couleur : `selectable_label` / `selectable_value` d'egui, et
  nulle part ailleurs (garde `aucun_bouton_selectionnable_hors_d_un_menu`) ;
- **bascule d'outil** d'une barre : `Button::selected` (`toolbar::tool_button`).
- **formulaire** (#227) : `form(ui, sel, |rows| …)` de `app/form.rs`, une
  grille à deux colonnes — `rows.row(libellé, |ui| champ)`, cases et notes
  dans la colonne des champs (`rows.checkbox`, `rows.note`, `rows.field`),
  couleur par `rows.color`, titre par `rows.section` (jamais un trait seul
  entre deux blocs, L3), rangées grisées par `rows.enabled` ; pas de section
  repliable (L5 : onglets). Largeurs : `theme::FieldWidth` (`Short` ~80,
  `Medium` ~160, `Full`) ; écarts : `theme::SPACE_ITEM` / `SPACE_SECTION`,
  jamais `add_space(<nombre>)` (`aucun_add_space_litteral_hors_du_theme`) ;
- **choix exclusif fixe** dans un formulaire : `choice` / `choice_by`
  (`ChoiceOption`, rappel `hint` facultatif) décide seul radios (2–3 options,
  côte à côte si chaque libellé tient en 3 mots) ou liste déroulante (dès 4) ;
  une liste de **données** (colonnes, graphiques, modèles) reste une
  `ComboBox`. Dans un **menu** : `menu_sections::menu_choice`, coche seule,
  jamais un radio (C3) ; une **bascule** est une case à cocher, libellé à
  droite (C5).

Gras : `theme::bold` seulement ; `RichText::strong` / `ui.strong` ne
changent que la couleur et sont interdits (`aucun_strong_d_egui_hors_du_theme`).
Titre de section de formulaire : `theme::section_title(ui, tr!(…))`, qui pose
l'espace et le gras ; il sépare seul ses groupes — pas de `ui.separator()`
juste avant (`aucun_separateur_avant_un_titre_de_section`), pas de libellé
ordinaire qui fait office de titre. Couleurs d'état et de chrome (accent,
survol, filet, dépôt, erreur) : une fonction de `theme.rs` ou les `Visuals`
qu'il pose (`error_fg_color`, `warn_fg_color`, `selection`) ; aucun
`Color32::from_…` ni teinte nommée (`RED`…) hors de `theme.rs` et
`color_picker.rs` (`aucune_couleur_en_dur_hors_du_theme`). Une teinte qui
manque s'ajoute à `theme.rs`, nommée par son rôle.

## 2 quater. Sa place dans les menus — à étudier pour **toute** fonctionnalité

Avant de coder, décider explicitement où l'utilisateur **trouvera** la
fonctionnalité, et le dire dans le compte rendu. Deux familles de menus :

- **la barre de menus** (`StokesForgeApp::menu_bar`, `app/menus.rs`, #122) :
  *File*, *Edit*, *Workbook* (présent seulement quand un classeur est au
  premier plan), *Graph* (présent seulement quand un graphe est l'onglet
  actif, `state::active_graph`), *View* et *Window* (noms anglais, clés
  `menu-*`). Une entrée nouvelle se teste dans le `mod tests` de
  `app/menus.rs` : chaque menu de haut niveau y est ouvert pour de vrai, et
  son grisage lu par l'arbre d'accessibilité d'egui (`entrees`, AccessKit). Une action qui porte sur l'objet
  actif (le graphe, le classeur…) y a presque toujours sa place : c'est là
  qu'un utilisateur d'Origin la cherche. Si aucun menu existant ne convient
  (un menu *Workbook* quand un classeur est actif, par exemple), le créer
  sur le modèle de *Graph* plutôt que de ranger l'action ailleurs ;
- **les menus contextuels** (clic droit sur la grille, les onglets de
  feuille, un composant du graphe…).

Règle : **rien ne doit être atteignable seulement au clic droit.** Une entrée
de menu contextuel a son équivalent dans la barre de menus (ou un raccourci,
pour la grille). Le clic droit est un raccourci vers ce que la barre de
menus offre déjà, pas un chemin exclusif. Si la fonctionnalité n'a
délibérément sa place dans aucun menu, l'écrire et dire pourquoi.

### La grammaire des menus (#152) — toute nouvelle entrée s'y range

[`docs/menus.md`](../../../docs/menus.md) est la **source unique** : ordre
canonique des sections (principale ─ créer ─ presse-papiers ─ organiser ─
supprimer ─ sortie ─ `Properties…` en dernier), sous-menu seulement pour un
choix exclusif ou ≥ 3 entrées (jamais 1 ; 2 à plat), profondeur 2, *absent*
(sans objet pour la cible) ou *grisé avec `.disabled`* (momentanément
impossible), verbes nus, en-tête grisé réservé au graphe, raccourci par
`SHORTCUT_*`, pas d'icône, pas d'Undo/Redo au clic droit. Avant d'ajouter une
entrée : **choisir sa section**, la peindre dans la **peinture partagée** de la
surface derrière `MenuSections::section(ui, présente, contenu)`
(`app/menu_sections.rs`) — seule voie pour un trait de séparation
(`aucun_separateur_brut_dans_un_menu`, aucune dérogation).

**Contenu et enveloppe** : les peintures partagées peignent le *contenu* ; le
menu du haut l'enveloppe dans un sous-menu qui nomme la cible (*Selection*,
*Sheet*, *Columns*), le clic droit l'affiche à plat. Pas de seconde voie.

### Sous-menus et fermeture (#114)

Les listes longues se regroupent par thème : mêmes noms et clés au menu du
haut et au clic droit. Réutiliser `object_menu`, `object_node_menu`,
`object_manager_settings_contents`, `columns_menu_contents` /
`column_type_role_menus` / `plot_menu`, `graph_properties_menu`
plutôt que recopier les entrées. Les cases restent pour les vraies bascules
d'affichage, qui **gardent les menus ouverts** (choix de l'utilisateur).
Les actions et les choix exclusifs ferment toute la chaîne par `ui.close()`.
`MenuBar` et les menus contenant des bascules utilisent
`PopupCloseBehavior::CloseOnClickOutside`, transmis aux sous-menus.
Tester une bascule avec deux clics dans le même menu et une action après
repeinture sans événement : ni la feuille ni ses parents ne doivent rester.

## 2 quinquies. Couleurs : le sélecteur commun

Un réglage de couleur ne s'écrit jamais comme un champ texte `#RRGGBB` :
`color_picker::color_field(ui, libellé, &mut chaîne, ColorUsage::…,
Some(&tr!("…")), &mut env)`, avec l'usage de la couleur — `Curves` pour
une courbe (l'outil des courbes, palettes nommées), `Text` pour du texte ou
une échelle, `Background` pour un fond — et le `PickerEnv` des couleurs
enregistrées, dont les demandes sont traduites en `Action::SaveColor`/
`RemoveSavedColor` par l'appelant (modèle : `plot_settings_contents`, dans
`src/app/plot_settings.rs`). Une
couleur peut porter une opacité (`#RRGGBBAA`). Dans le modèle, une couleur
vide veut dire « automatique » ; pour du texte ou une
échelle, le rendu la résout par `stokesforge_render::color::auto_ink` (blanc sur
fond sombre) — dans `stokesforge-render`, jamais dans un backend. Un nouveau
champ de couleur du modèle porte `#[serde(default)]` côté
`stokesforge-persistence` (`docs/persistance.md`).

**Un menu déroulant dans une fenêtre surgissante exige une `egui::Area`
à soi, pas `egui::Popup`** : egui ne retient qu'une fenêtre surgissante
ouverte par viewport, donc la liste d'un `ComboBox` refermerait la fenêtre
parente (issue #9). `color_button` tient l'état ouvert/fermé lui-même et
ferme sur Échap ou clic extérieur ; s'en inspirer.

**Le sélecteur s'ouvre replié** (issue #10) : palette, couleurs enregistrées
et hexadécimal seulement ; la case « Custom color » (C5, #227) déplie nuancier, opacité,
RGB/HSL et Save Color. Tout nouvel élément lourd du sélecteur va dans la
partie dépliée (`if custom`), et l'état se remet à zéro à la fermeture
(`set_picker_open`).

**Couleur globale ou thème** (issue #11) : une règle qui touche plusieurs
champs de couleur vit dans `stokesforge-plot::theme` (`PlotSpec::set_text_color`,
`set_axes_color`, `apply_theme`), pas dans l'interface. Elle écrit dans les
champs locaux — pas de champ « global » dans le modèle — donc le dernier
réglage gagne ; un nouveau champ de couleur du modèle doit y être ajouté.

## 2 sexies. Fenêtres et dialogues

Toute fenêtre de l'application se construit par la brique commune
(`show_window_in_layer` : cadre, contour actif, rognage ; `window_header`
pour la barre de titre ; `src/app/window_chrome.rs`), **jamais** par une
`egui::Window` brute. Un dialogue qui doit bloquer le reste de l'interface
passe par `modal_dialog` (`src/app/dialogs.rs` ; fenêtre des
propriétés du graphe) : calque plein écran qui capte les clics, contour rouge
foncé qui clignote sur un clic bloqué, boutons Valider (défaut, Entrée) /
Annuler (croix, Échap) rendus par `DialogOutcome`. Les gestes d'un dialogue
d'édition vivent dans une macro d'historique (`begin_macro`) : Valider la
ferme (une entrée), Annuler l'avorte (modèle : `cancel_plot_settings`). **Tout jeu d'onglets passe par `tab_strip`** (`src/app/tab_strip.rs`, issue #19), la bande des
onglets de feuille rendue générique : défilement horizontal sans barre,
flèches (icônes `CaretLeft`/`CaretRight`) quand des onglets sont cachés, défilement minimal vers
l'onglet choisi. Ne jamais écrire une rangée de `selectable_label` pour des
onglets ; `sheet_tabs` (feuilles), les pages de la fenêtre des propriétés du
graphe et ses axes en sont des usages. Aspect, listes, gras et couleurs d'état :
§2 ter bis. Un dialogue modal ne **peint pas de voile** sur le reste de
l'application (issue #20) : il le bloque seulement.

Les panneaux fixes (#100) passent par `panel_header` dans le même module :
PM à gauche, OM à droite, console en bas. L’ordre horizontal des boutons
dépend du côté : repli vers l’espace de travail, détachement vers l’extérieur.
La console déployée garde Replier puis Détacher côte à côte à gauche (#142).
Les trois panneaux repliés ne montrent que Déployer, sans cible invisible
ni place réservée à Détacher ; le titre occupe l’espace restant.
Les bandes repliées font `ICON_SIZE` d’épaisseur, sans marge de panneau ni
place réservée au séparateur ; Déployer centré, titre vertical centré sous
le bouton.
Visibilité par View, repli distinct
du masquage ; en-tête attaché déployé = Détacher et Replier, replié = Déployer ; fenêtre OS
sans décoration native = Rattacher uniquement. Une fermeture OS rattache.
Ne pas réintroduire de flottant interne pour un panneau ni de bouton Fermer
ou Maximiser. Réutiliser les lectures/requêtes de `window_detach.rs` ; le
moteur de script reste dans l’AppState principal.

**Aucune exception, y compris pour un assistant à plusieurs étapes.**
Trois formes, un seul corps (`modal_frame`, #228) :
`modal_dialog_with(ctx, id, titre, taille, DialogFooter { accept_label,
accept_hint, accept_enabled, accept_disabled }, secondaires, contenu)` — pied `[secondaires]
[message] [Cancel] [principal]` (F6) ; `modal_wizard(…, boutons_d_étape, …)` —
`[Cancel] [message] [Back · Next] [Finish]`, l'assistant d'import (boutons
d'étape ajoutés de droite à gauche) ; `modal_read_only(ctx, id, titre, taille,
contenu)` — un seul bouton Close (F2), pour une fenêtre qui ne change rien.
Entrée valide, Échap annule — sauf l'Échap qui sort d'un champ texte (F7).
**Bouton principal grisé** : `accept_disabled` est sa **raison** (clé
`disabled-*`, ce qu'il faut faire, #226), écrite dans le pied et en bulle
(F10) ; `accept_hint` reste la bulle du bouton actif.
**Erreur d'un geste du dialogue** : `AppState::fail` l'écrit d'office dans le
pied tant qu'un dialogue de contenu est ouvert (`dialog_error`, M7) — jamais
`status` ; un test d'état lit `state.dialog_error()`, et une décision « le
geste a-t-il échoué ? » lit `last_action_failed`, jamais `status().is_error`.
Confirmations (#229) : `modal_confirm` (Cancel par défaut ; le verbe
seulement par son bouton), `modal_save_changes` (« Discard Changes · Cancel ·
Save », Entrée = Save), `modal_alert` (M8) — voir la liste des réflexes plus
bas ; un **choix** (axe d'un dépôt, calcul avec ou sans colonnes) reste un
`modal_dialog_with`.
Taille et position (F8) et premier champ focalisé (F9) sont faits par la
brique : rien à écrire, mais un premier champ de saisie doit être un vrai
`TextEdit` du dialogue (c'est ce que le focus initial cherche). **Avant d'écrire une fenêtre, dialogue ou assistant : partir de
`modal_dialog_with` (dialogue) ou `show_window_in_layer` (fenêtre de contenu).**
Si la brique manque d'un réglage, l'étendre (c'est ce qui a été fait pour
l'import) plutôt que de la contourner. Un code existant qui déroge n'est
**jamais** une raison d'en écrire un autre : c'est une dette à rembourser
(aucune `egui::Window` brute ne subsiste ; le test
`aucune_fenetre_n_est_une_egui_window_brute` le verrouille).

## 2 septies. Icônes : le composant commun

Une icône s'affiche par `icons::icon_image` / `icon_button` (taille unique,
info-bulle `tr!` obligatoire), jamais par un glyphe Unicode ni un emoji. **N'en
*afficher* que si elle apporte quelque chose** (action répétée reconnaissable,
type d'objet) ; un mot vaut mieux qu'une icône ambiguë.

**Mais chaque action a son logo** (règle de #110, `docs/icones.md`) : une
action nouvelle, affichée ou non, a une identité visuelle prévue — pour la
future barre personnalisée. Action d'une **barre fixe** : icône Phosphor
soignée (procédure de `docs/icones.md`). **Autre action** : logo standard de sa
famille avec **deux lettres** en surimpression (gabarit à venir avec le
registre d'actions) ; tant qu'il n'existe pas, **nommer le logo prévu dans le
compte rendu** au lieu de l'omettre. Ça ne force pas à l'afficher dans les menus. Couleur : `IconRole::Neutral`
par défaut, un rôle coloré seulement pour un type d'objet, une action
destructive ou un état à repérer. Une icône nouvelle : `docs/icones.md`
(planche-contact à faire valider). Verrou : `aucune_icone_hors_du_composant`.

## 2 novies. Comportement de l'interface : le standard (#217)

[`docs/standard-interface.md`](../../../docs/standard-interface.md) est la
**source unique** de la façon dont réagissent champs, messages, dialogues,
clavier et souris. Le lire avant de poser un champ, un message, un dialogue,
une confirmation, un grisage ou un geste clavier/souris. Réflexes :

- un nombre ou un texte s'applique à Entrée ou en quittant le champ, Échap rend
  l'ancienne valeur, une entrée d'historique par validation ; une saisie
  illisible est **refusée** (cadre rouge, bulle), une valeur hors bornes
  **ramenée** (cadre orange) — jamais d'effacement ni de correction muette.
  Brique *en place* (#219) : `field_state` — `edit_text` pour un champ qui
  lit une saisie, `show_issue` pour un défaut connu (Typst…), teintes
  `theme::field_refused_color` / `field_clamped_color` ; jamais un cadre peint à
  la main ;
- un champ qui **s'ouvre sur place** (renommer, éditer une cellule, un titre)
  passe par `app/inplace_edit.rs` (#224, T4) : `show` (ou `begin` + `finish`
  quand la mise en page est posée par l'appelant), `resolve` pour la décision
  pure ; tout le texte sélectionné à l'ouverture, Entrée / clic ailleurs
  valident, Échap annule, Tab valide et passe au suivant, nom refusé = champ
  ouvert sous cadre rouge — jamais un `TextEdit` dont on relit à la main
  `lost_focus` et Entrée. Un aperçu (analyse, import) se recalcule après 0,3 s
  de pause ;
- une **virgule décimale** se lit par `stokesforge_core::number_format::parse_input`
  (champs nombre, cellules ; une virgule seule et sans point), jamais par
  `str::parse::<f64>` sur une saisie ; le séparateur d'un graphe est
  `PlotSpec::decimal_comma` (#223) ;
- un **nombre** se saisit par `number_field::number_field" avec une grandeur de
  `quantities.rs` (bornes, pas, décimales, valeurs du `▼`) — jamais `DragValue`
  ni `Slider` (`aucun_champ_numerique_hors_de_la_brique`) ; une grandeur neuve
  s'ajoute à la table, l'unité va dans le libellé ;
  molette (#233, #235) : seulement sur un champ à liste, au survol, un cran =
  valeur proposée voisine ; aspect (#234) : `list_frame`, flèche pleine dans le
  cadre, fond blanc si éditable, gris si liste seule (fonds du thème) ;
- **deux champs liés** (« de » ≤ « à », #240, N11) : la paire inversée est
  refusée (ancienne valeur rendue, `field_state::flag` sur le **second** champ),
  jamais permutée ni ramenée ; **pas pour Min/Max d'un axe**, qui peut être
  inversé. Un texte Typst fautif est signalé par `formula_field`
  (`formula::diagnose`, champ quitté) : tout champ de texte riche passe par lui ;
- le **rappel d'un champ** (case, radio, texte, nombre) est l'icône « ? » :
  `app/hover_help.rs` (`help_beside` autour du champ, `help_text` après) ; jamais
  `on_hover_text` sur un champ (`aucune_bulle_nue_sur_un_champ_de_formulaire`,
  #225) ; la bulle nue reste aux boutons, icônes, onglets, menus ;
- un **nombre affiché** passe par `stokesforge_core::number_format`
  (`format_results` 3 chiffres, `format_grid` 6, `format_exact` pour éditer et
  copier, `format_with_uncertainty`, `format_tick`) — jamais un `format!`
  local (#222) ; la grille peint `AppState::cell_display`, édite et copie
  `cell_text` ;
- tout élément grisé dit pourquoi et quoi faire : `.on_disabled_hover_text(tr!(…))` sur l'élément lui-même (clé `disabled-*` ou `.disabled` de l'action), jamais sur un groupe ; garde `aucun_element_grise_sans_sa_raison` (#226) ;
- une erreur dans un dialogue va dans son pied, jamais dans la barre d'état ;
- une suppression annulable ne demande pas de confirmation ; ce que Ctrl+Z
  ne rattrape pas passe par `modal_confirm` (Cancel par défaut : Entrée et
  Échap l'actionnent, verbe destructeur rouge au survol/focus), un projet non
  enregistré par `modal_save_changes` (Entrée = Save), un échec grave hors
  fenêtre par `modal_alert` (M8 : Close + action) — peints depuis
  `app/confirmations.rs`, ou dans le contenu du dialogue qu'ils recouvrent ;
- un **travail qui dure** passe par `app/busy.rs` (`work_in_progress` : indicateur et mot après 0,5 s, barre si le total est connu, Stop après 3 s si le travail s'interrompt ; #232), jamais un `ui.spinner()` seul ; un travail qui passe par la session s'écrit `execute_interruptible` (recalcul de formules, #239 : Stop = annuler la commande), jamais un second chemin ; un **panneau vide** par `app/empty_state.rs` (`empty_state` : phrase grisée centrée qui dit ce qui manque et le geste pour le créer), jamais un `ui.weak(…)` nu ;
- aucun slider, unité dans le libellé, pas de bulle nue sur un champ (icône
  « ? » de `hover_help`) ;
- un **formulaire** se pose par `app/form.rs` (grille à deux colonnes, choix
  radio/liste par `choice`, #227), un choix de menu par `menu_choice`, un
  écart par `theme::SPACE_ITEM` / `SPACE_SECTION`.

**État des briques.** La page tient un tableau *en place* / *à venir* (issues
#219–#232). Une brique *à venir* (brique « nombre », formulaire en grille,
éditeur sur place…) **n'existe pas encore** : ne pas
l'appeler. Une règle *à venir* est déjà la règle pour tout code neuf ou
modifié ; l'écrire avec les moyens existants, sans recopier un voisin qui
déroge. Le commit qui crée une brique ou une garde la passe à *en place* dans
la page, l'ajoute à ce skill et à la table des gardes de
`stokesforge-verification`, dans le même commit.

## 3. Les quatre surfaces d'une action — ce que Qt donnait gratuitement

Une `QAction` reliée à un `QUndoStack` portait d'un coup le raccourci, l'entrée
de menu, son grisage et son libellé. Ici les quatre sont **séparés**, et rien
ne garantit leur cohérence sinon la relecture. C'est le premier point à
vérifier quand une action est ajoutée :

1. **le raccourci** — une constante `SHORTCUT_*` (ou `KEY_*` pour une touche
   nue) dans `app/shortcuts.rs`, **registre unique** (#231, K1 : jamais de
   `consume_key` ni de `KeyboardShortcut::new` ailleurs, garde
   `aucun_consume_key_hors_du_registre`), consommée par `input.consume_shortcut`
   (`collect_shortcuts` dans `app.rs` pour un raccourci global), **et** une
   ligne de `HELP` pour Help › Keyboard Shortcuts, avec ses clés `help-*`
   en/fr. Curseur : `app/cursors.rs` (`grab`, `clickable`, `RESIZE_*`), jamais
   un `CursorIcon::` écrit sur place ; double-clic (K3) : texte ou nom → édition
   sur place, objet → Properties, fond d'un graphe → vue réinitialisée,
   Project Manager → active, ne ferme jamais ;
2. **l'entrée de menu** (barre de menus et/ou menu contextuel), avec son
   `shortcut_text` pour rappeler le raccourci à droite ;
3. **le grisage** (`add_enabled`), calculé depuis `AppState` ;
4. **le libellé**, qui vient de `state` (`AppState::undo_label`,
   `GridMenu::target_label`…), jamais écrit en dur dans la couche `app` quand une
   contextualisation existe déjà. **Tout libellé est une clé du catalogue
   `en`** (`crates/stokesforge-app-state/i18n/en/stokesforge_app.ftl`), lue par `tr!("clé")` —
   jamais un littéral, même pour un bouton de passage (verrouillé par
   `aucun_libelle_en_dur`, `stokesforge-app/tests/libelles.rs`). Rédiger le texte selon le
   skill **stokesforge-textes-interface** (mécanique : ajout d'une clé, pseudo-langue,
   garde). Une action a **un** libellé : la clé `action-*`, partagée par la barre de
   menus, le menu contextuel et la barre d'état ; son infobulle est l'attribut
   `.hint` (`.disabled` si elle dit pourquoi l'entrée est grisée). La garde est absolue : aucune dérogation.

### Ajouter un raccourci clavier

- Vérifier d'abord les collisions : `rg "SHORTCUT_" crates/stokesforge-app/src/app/shortcuts.rs`.
- **Ctrl+C / Ctrl+X / Ctrl+V ne sont jamais reçus comme `Event::Key`** :
  `egui-winit` les intercepte à la traduction des événements de fenêtre.
  Les constantes `SHORTCUT_COPY`/`CUT`/`PASTE`/`COPY_FULL_COLUMNS`/
  `PASTE_ABSOLUTE` sont **purement décoratives** — elles n'existent que pour
  écrire le rappel à droite d'une entrée de menu. Rien ne les consomme, et un
  essai antérieur fondé sur l'hypothèse inverse produisait des raccourcis qui
  ne se déclenchaient jamais. Si le geste doit passer par le presse-papier
  système, c'est `Event::Copy`/`Event::Paste` (ou `arboard` en lecture directe,
  comme `app::grid::system_clipboard_text`). Pour offrir autre chose que du
  texte au presse-papiers (image, plusieurs types MIME), ni egui ni `arboard`
  ne suffisent : `stokesforge-clipboard` (#149, `app/figure_copy.rs` pour le
  modèle : décider du geste avant les panneaux, consommer l'`Event::Copy`,
  pousser une `Action`, offrir depuis la fenêtre racine).
- Les raccourcis sont consommés **au niveau du contexte**, donc avant les
  widgets : Ctrl+Z pendant la saisie d'une cellule annule la dernière commande
  du modèle, pas la dernière frappe. C'est voulu (comportement de tableur).

- **Tab, Maj+Tab et les flèches appartiennent à la grille** (#38) : egui les
  prend au début de l'image pour déplacer son focus, et `key_pressed` n'y
  change rien. Un raccourci de grille qui lit l'une de ces touches ne marche
  que parce que `grid_contents` (`src/app/grid.rs`, où vivent aussi les tests
  de ces touches, `app::grid::tests`) tient le focus clavier par un ancrage portant
  `GRID_KEYS` ; un nouveau champ de saisie dans la grille doit poser
  `TextEdit::event_filter(GRID_KEYS)` et lire Tab lui-même. Pour savoir si
  l'on « tape dans un champ », utiliser `keyboard_focus_outside_grid`, jamais
  `focused().is_some()` ni `egui_wants_keyboard_input` (la grille focalisée les
  rendrait vrais). Voir `docs/application.md`, « Tab et flèches appartiennent
  à la grille ».

### Ajouter une entrée de menu contextuel

Le menu contextuel de la grille suit ces règles, en plus de la grammaire de
[`docs/menus.md`](../../../docs/menus.md) :

- **Chaque entrée garde son équivalent** : un raccourci (le bloc de
  `grid_contents`), la barre de menus ou la barre d'outils. Un geste atteignable
  **seulement** au clic droit est à décider explicitement. (Plot ▸, Add Column,
  Type ▸ et Role ▸ n'ont pas de raccourci : leur équivalent est le menu
  *Workbook*.)
- **La décision vit dans `AppState::grid_menu` → `GridMenu`**, pas dans le code
  egui. C'est ce qui la rend vérifiable sans fenêtre dans `stokesforge-app-state/tests/shell.rs` :
  quels gestes ont un sens dans cette zone (`GridZone::Header`/`Data`/
  `Columns`/`Rows`), lesquels griser, comment accorder les libellés en nombre.
- **Les entrées sont des verbes nus** — règle de toute la grammaire, pas
  seulement de la grille : « Copier », pas « Copier la cellule ». Le clic droit
  vient de se poser sur ce qu'il vise et le raccourci est déjà rappelé à droite.
  `target_label` reste disponible pour les surfaces où l'on ne voit *pas* ce
  qu'on vise (barre de menus, ligne de statut, confirmation) ; l'en-tête grisé
  qui nomme la cible n'existe qu'au clic droit du **graphe**.
- **Undo / Redo n'y figurent pas** : Edit et le clavier.

Un clic droit **hors** de la sélection la remplace par l'élément visé ;
**dedans**, il la laisse intacte — même garde
(`response.secondary_clicked() && !selected`) dans la grille, le graphe (une
courbe cliquée est sélectionnée) et les arbres, sinon on rend impossible une
action sur plusieurs colonnes sélectionnées.

Écarts délibérés entre menu et clavier déjà en place (les connaître avant d'en
inventer un nouveau) : « Supprimer et décaler » supprime tout de suite dans le
menu alors que Suppr vide d'abord ; « Coller » n'est jamais grisé parce que
sonder le presse-papier système à chaque image coûterait plus que de tenter au
clic.

## 3 bis. Demander plutôt que supposer

L'utilisateur de ce dépôt **n'est pas développeur**. Il décrit un geste et un
résultat attendu, pas une implémentation — et c'est très bien ainsi : lui
demander d'arbitrer entre deux structures de code est une perte de temps pour
tout le monde. En revanche, il est le seul à pouvoir trancher entre deux
**comportements observables**.

Appeler `AskUserQuestion`, en une seule fois, dès qu'un de ces cas apparaît :

| Situation | Pourquoi |
|---|---|
| la demande ne dit pas le comportement **aux bords** (dernière colonne, sélection vide, fenêtre détachée, colonne mixte) | c'est là que se cachent les trois quarts des allers-retours |
| réaliser la demande **casse un geste existant** | il doit savoir ce qu'il perd avant, pas après |
| un commentaire du code dit « choix volontaire » mais la demande **diffère de l'option rejetée** | un commentaire ferme *une* alternative, pas toutes — ne pas conclure seul, dans un sens ni dans l'autre |
| la demande a plusieurs lectures plausibles et elles ne coûtent pas la même chose | chiffrer les options et laisser choisir |
| le périmètre est flou (« revoir toute la logique de X ») | l'écart entre un correctif et une refonte se joue là |

Poser la question avec ce qui est **déjà établi** et des options concrètes —
coût, risque, ce que ça casse. Jamais une question ouverte qui renvoie le
travail. Et ne pas demander ce qui a un défaut évident : là, choisir, le dire
en une ligne, avancer.

À partir de **six choix** à faire trancher, publier le formulaire de
décisions au lieu d'`AskUserQuestion` :
[`../stokesforge-bug-report/references/formulaire-de-decisions.md`](../stokesforge-bug-report/references/formulaire-de-decisions.md).

## 4. Tests — et, si c'est visuel, regarder

Toute logique ajoutée dans `state.rs` (`stokesforge-app-state`) se teste dans
`crates/stokesforge-app-state/tests/shell.rs` — sans fenêtre, sans serveur graphique.

**Les tests couvrent la géométrie, pas l'apparence.** Un changement visuel
(contour, alignement, largeur, troncature, surbrillance, icône) n'est pas
vérifié par une suite verte : il se vérifie en regardant la fenêtre.
`stokesforge-app` sait se photographier — procédure dans
[`../stokesforge-bug-report/references/voir-l-interface.md`](../stokesforge-bug-report/references/voir-l-interface.md).
Une capture avant et une après valent mieux qu'une affirmation.
Une fonction pure extraite exprès pour être testable (`app::grid::column_grip_bands`,
`state::active_graph`, `state::drop_payload_action`) est le motif idiomatique
du dépôt quand une décision doit vivre près de l'UI : extraire, puis tester.

Détail des familles de tests, du principe du **témoin** et des garde-fous
d'architecture : skill **stokesforge-verification** et `docs/tests.md`.

## 5. Mettre à jour la documentation

Deux endroits, deux rôles distincts :

- **Le commentaire de tête de module (`//!`)** porte le *pourquoi* : ce qui a
  été essayé, ce qui a été écarté, la contrainte technique qui a imposé la
  forme retenue. C'est là que va le raisonnement, au plus près du code qu'il
  justifie. Le *quoi* se lit dans la signature juste en dessous.
- **La page `docs/` du domaine** (`application.md`, `rendu.md`,
  `modele-de-donnees.md`…) décrit le code **tel qu'il est** et oriente. Elle ne
  planifie rien, ne spécifie rien, ne recopie pas le raisonnement des `//!` :
  elle dit où le trouver, et porte ce qui traverse plusieurs modules.

Mettre à jour aussi, **seulement si le changement les rend faux** :
`docs/invariants.md` (une règle nouvelle qui traverse plusieurs crates),
`.claude/rules.md` et `.codex/README.md` (si des chemins, scripts ou
frontières ont bougé), `docs/tests.md` (un nouveau garde-fou d'architecture).

Tout en **français**, comme le reste du dépôt.

## 6. Finir

Enchaîner sur le skill **stokesforge-verification** : `fmt`, `clippy -D warnings`,
`cargo test --workspace`, plus la feature hors défaut si le changement la
touche (`python`), relecture du diff, message de commit en français.

## Checklist

- [ ] domaine identifié, page `docs/` lue, `.claude/rules.md` respecté
- [ ] imprécisions de la demande levées **avant** de coder, en une seule question
- [ ] ce n'est pas un plafond déjà arbitré (`questions-ouvertes.md` §11)
- [ ] doublon docké/détaché vérifié (`rg "fn <nom>|fn detached_<nom>"`)
- [ ] état d'`AppState` nouveau recopié dans la vue détachée s'il doit y valoir ; raccourci global essayé avec un graphe actif et un classeur détaché
- [ ] dessin et événements dans le module de la surface (`src/app/<surface>.rs`), code de fenêtre dans le `window_*.rs` concerné, rien de nouveau dans `app.rs` hors orchestration ; surface nouvelle dans son propre module, à imports explicites, ajoutée à la table de `docs/application.md` (§2 octies)
- [ ] barres de défilement : aucun style local, place lue via `theme::scroll_bar_allocated_width`
- [ ] onglets par `tab_strip`, listes par `list_row`, titres par `theme::section_title`, gras par `theme::bold`, couleurs d'état par `theme.rs`, formulaire par `app/form.rs` (grille, `choice`), écarts par `theme::SPACE_*` (§2 ter bis)
- [ ] icônes : composant `icons.rs`, info-bulle `tr!`, seulement si elle apporte quelque chose (`docs/icones.md`) ; logo prévu de l'action nommé dans le compte rendu (#110)
- [ ] calque visé par son indice dans `PlotSpec::panels` (calque actif), jamais par `(row, column)` ; ajout de séries par l'`Action` du dépôt
- [ ] couleurs : `color_picker::color_field` avec l'usage et le `PickerEnv`, vide = automatique
- [ ] champs, messages, dialogues, grisage, clavier conformes à `docs/standard-interface.md` (§2 novies) ; brique *à venir* non appelée ; brique ou garde créée → passée à *en place* dans la page et les skills
- [ ] existant réutilisé plutôt que dupliqué
- [ ] mutation du modèle → `Command` annulable (skill **stokesforge-commande**)
- [ ] logique dans `state.rs` (`stokesforge-app-state`), glu egui seulement dans la couche `app` (`app.rs` et `src/app/`)
- [ ] place dans la barre de menus **et** les menus contextuels étudiée (§2 quater) ; rien n'est accessible seulement au clic droit
- [ ] les quatre surfaces en accord : raccourci, menu, grisage, libellé
- [ ] libellés : clés du catalogue `en` + `tr!`, aucun littéral (`aucun_libelle_en_dur`) ; un changement visuel est regardé aussi en `STOKESFORGE_LANG=en-XA`
- [ ] entrée de menu rangée dans une section de la grammaire (`docs/menus.md`), séparateurs par `MenuSections::section` seulement, *absente* ou *grisée avec `.disabled`* ; menu contextuel de la grille : décision dans `GridMenu`
- [ ] test au bon endroit : décision d'`AppState` dans `stokesforge-app-state/tests/shell.rs`, geste peint dans le module `tests` de la surface (`src/app/<surface>.rs`), autre crate sinon
- [ ] changement visuel **regardé** (capture), pas seulement testé
- [ ] `//!` du module mis à jour (le *pourquoi*) + page `docs/` (le *quoi*)
- [ ] `fmt` + `clippy` + `cargo test --workspace` verts

### Panneau d'objets et sélection des contenus (#81–85)

Object Manager projette l'instantané (`object_manager::object_tree_with_selection`) et lit la
sélection de `GraphView` : jamais de sélection parallèle dans le panneau.
`with_graph_view` atteint la même vue flottante ou détachée. Les cases de
visibilité passent par `Action::SetPlotVisibility`, puis `EditPlotSpec` (delta du spec partagé, motif `PlotEditKind`) ;
ouverture et sélection restent hors historique. L'échantillon de série appelle
`stokesforge_render_egui::series_swatch` (vignette partagée de la légende).

Les réglages de vue (#139) réutilisent `object_manager_settings_menu` depuis
Affichage et le fond du panneau (grisé si fermé), les `Action` et la table
`preferences::ObjectManagerSettings`. Le repli initial, y compris après
Réinitialiser, ne remplace jamais un état egui déjà mémorisé. La section Objets
est propre au graphique ou au calque ; ne pas la confondre avec les axes.
Le modèle du viewport détaché transporte tous les réglages et doit se réveiller
sur leur seul changement. Pour tester leurs actions, injecter un fichier
temporaire avec `AppState::new_with_preferences`, jamais la configuration réelle. Un test de `stokesforge-app` qui bâtit un `AppState` par `new` n'atteint pas la configuration réelle grâce à la feature `test-support` (#263, `isolation_preferences.rs`).

Les textes et objets sélectionnés lisent aussi `with_graph_view` /
`read_graph_view` (propriétés et barres de format incluses). Le menu
contextuel et Graph › Selection partagent `object_node_menu`
(`app/object_manager_ui.rs`, l'arbre peint, à côté du modèle
`object_manager.rs`). Série, groupe et objet libre se
glissent par leur poignée (`reorder_list::reorder_row_with`) : les lignes
s'écartent en direct (aperçu de spec dans `app/object_dnd.rs`, décisions pures
dans `crate::object_drag`) et une seule action part au relâchement
(`PlaceSeries`, `PlaceSeriesGroup`, `ReorderObject`) ; entrer dans un groupe,
en sortir et échanger des séries seules passent par la même voie, la fenêtre
des séries modifiant son brouillon directement. Les propriétés
conservent le calque exact dans `PlotEdit::selected_panel`.

## Correction définitive d'une analyse (#212)

Réutiliser `ReplaceAnalysisData` pour remplacer la source Y d'une ligne de base
ou d'une correction par référence. La trace immuable est un `CorrectionRecord`
à part des calculs ; valeurs et trace sont annulées ensemble. Exiger les
versions de toutes les entrées lues, préserver longueur/lignes manquantes,
refuser les corrections contradictoires d'un même Y. Dans un formulaire,
griser le choix avec `ChoiceOption::disabled` et sa raison ; la confirmation
des sources partagées réutilise `modal_confirm`.
