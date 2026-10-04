---
name: stokesforge-feature
description: Implémenter une nouvelle fonctionnalité dans StokesForge (portage Rust, branche rewriterust) — du choix de la couche jusqu'au menu contextuel, au raccourci clavier, aux tests et à la mise à jour de la doc. À utiliser dès que la demande est « ajoute », « implémente », « je voudrais pouvoir… », « il manque… », « fais que… » sur l'application, la grille, les graphes, l'import, les scripts ou le modèle — y compris quand la demande a l'air d'une simple ligne de code, parce que le coût de StokesForge n'est jamais dans la ligne mais dans les quatre surfaces (raccourci, menu, grisage, libellé) et l'entrée d'historique qu'elle oblige à tenir en accord.
---

# Implémenter une fonctionnalité dans StokesForge

StokesForge est un tableur/traceur scientifique façon OriginLab. Une fonctionnalité
y touche presque toujours **plus d'une couche** : ce skill sert à ne pas en
oublier une, et surtout à ne pas la câbler au mauvais endroit.

## 1. Avant d'écrire une ligne

Dans cet ordre, sans sauter d'étape :

1. `git status --short` puis `git diff` sur les fichiers visés — l'utilisateur
   a souvent du travail en cours qu'il ne faut ni écraser ni committer.
2. Ouvrir la page `docs/` du domaine, **pas** le workspace entier.
   `.claude/rules.md` dit exactement quoi charger et quoi ne pas ouvrir.
   `docs/invariants.md` vaut pour tous les domaines.
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
| change le **cadrage de vue** (zoom, pan) | `ViewState` sur `GraphView` | ce n'est **pas** une mutation du modèle : personne ne veut annuler un zoom par Ctrl+Z |
| est une **décision** d'interface (quoi proposer, quoi griser, comment nommer) | `stokesforge-app/src/state.rs` | doit rester testable sans fenêtre |
| est du **dessin ou de l'événement** egui | `stokesforge-app/src/app.rs` et ses sous-modules (`window_*.rs`, voir § 2 octies) | rien de ce qu'ils contiennent n'est une décision de produit |
| est une **fonctionnalité de tracé** | `stokesforge-render` (générique sur le backend) | jamais dans un seul backend (invariant 5) ; voir le skill **stokesforge-rendu** |
| lit un **fichier de données** | `stokesforge-import` | reste indépendant de l'UI, de `core`, des commandes et de la session |

Une action qui a besoin de l'**état de vue** (le zoom d'une `GraphView`, qui
vit dans `app.rs`, jamais dans `AppState`) : `app.rs` le lit et le **met dans
l'`Action`**, `state.rs` fait le travail et le dit dans la barre d'état. Modèle :
`Action::ExportGraph { path, spec }`, dont le spec est déjà cadré par
`GraphView::framed_spec` (issue #34) — testable dans `tests/shell.rs` sans
fenêtre, et identique depuis une fenêtre détachée.

Un **geste continu sur un graphe** (glisser, édition sur place) n'émet pas une
action par image : l'aperçu vit dans la `GraphView`, l'état d'un éditeur dans
la mémoire d'egui sous `GraphView::id()` (donc identique en fenêtre détachée),
et une seule action part à la validation. Modèle : textes libres, issue #35
(`app::annotation_interactions`). Le champ de saisie d'un tel éditeur s'écrit
**dans le cadre du rendu** (centre, corps, sans rotation : une fonction pure de
placement, testée sans fenêtre — `stokesforge_render_egui::annotation_field`, issue
#37), et ne masque pas l'aperçu quand on règle autre chose que le texte (#36).

La ligne de partage `state.rs` / `app.rs` est la colonne vertébrale du crate
UI : `state` porte toute la logique, pilotée par un enum `Action`, et se teste
en CI (`tests/shell.rs`) ; `app` ne fait que traduire clavier et souris en
`Action` et peindre. Une fenêtre ne se teste pas en intégration continue —
c'est toute la raison du découpage. Si une règle métier se retrouve dans
`app.rs`, elle devient invérifiable.

Un graphe neuf n'a aucun titre par défaut (#103), démo de l'application
comprise : garder `PlotSpec::title` vide, sans masquer `title_visible` ni
reprendre le nom du nœud comme titre. Le titre reste saisissable par les
propriétés. Les titres explicites de `stokesforge-render::sample` servent
aux tests et bancs de rendu ; ils ne définissent pas le défaut utilisateur.

## 2 bis. Le doublon docké / détaché — à vérifier avant de chiffrer

Les contenus de `stokesforge-app` ont longtemps existé **en deux écritures** : une
pour la fenêtre flottante, une pour la fenêtre détachée. Depuis le 2026-09-30,
classeur, graphe, note et matrice n'en ont plus qu'une (`book_contents`,
de `src/app/workbook.rs`,
`graph_view_contents`, `note_contents`, `matrix_contents`) — **ne pas en
réintroduire** : une nouvelle nature de fenêtre s'écrit une fois et s'appelle
des deux côtés, sur le motif du classeur (vue `AppState` locale,
`fork_for_detached_view`).

La cause est documentée dans `docs/application.md` : la fermeture `'static`,
`Send + Sync` de `show_viewport_deferred` ne peut pas capturer `&mut AppState`.
Elle est contournée par une vue `AppState` locale à la fenêtre détachée, qui
renvoie ses actions à la racine.

Avant de toucher à une surface d'interface :

```sh
rg "fn <nom>|fn detached_<nom>" crates/stokesforge-app/src/
```

Deux résultats → le changement se fait deux fois, ou mieux : **se factorise**.

Une seule écriture ne suffit pas : la vue détachée a **son propre**
`AppState`. Un état d'interface ajouté à `AppState` (édition en cours,
confirmation, renommage…) n'y existe que s'il est recopié par
`fork_for_detached_view`, resynchronisé par `sync_detached_snapshot` ou
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

## 2 octies. Où ranger du code d'interface (`app.rs` et ses modules)

`app.rs` est en cours de découpage (chantier #115, sous-tickets #116–#124,
chacun **sur feu vert seulement** ; #116 à #119 faits). Cible décidée le 2026-10-04 : de vrais
modules dans `src/app/` (imports explicites, interface `pub(crate)` minimale,
pas de `use super::*` hors des modules de tests), des tests qui suivent leur
code et sont relus et améliorés à chaque déplacement par un agent Opus
(outil `Agent`, modèle `opus`, briefé avec le contexte du module), et les
fonctions géantes redécoupées à part (#124), jamais dans le même commit
qu'un déplacement. État actuel, dans `src/app/` (vrais modules, imports
explicites, `pub(super)` pour ce que partagent les modules d'`app`) :
- briques communes (#117) : `dialogs.rs` (`modal_dialog`, `modal_dialog_with`,
  `DialogFooter`), `tab_strip.rs` (`tab_strip`, `tab_strip_with`, `TabOps`),
  `shortcuts.rs` (`SHORTCUT_*`, `shortcut_text_*`), `tab.rs` (`Tab`) ;
- fenêtres de contenu : `window_chrome.rs` (cadre commun, `show_window_in_layer`),
  `window_placement.rs`, `window_stack.rs` (pile, repli, maximisation),
  `window_detach.rs` ;
- dialogues : `import_wizard.rs` (#118, `import_window`), `plot_settings.rs`
  (#119, `plot_settings_window`, ses onglets et éditeurs, `column_metadata_window`).

D'ici la fin du chantier :
- du code de fenêtre (placement, pile, détachement) va dans le
  `app/window_*.rs` concerné, pas dans `app.rs` ; une brique commune nouvelle
  rejoint `src/app/` avec ses tests ;
- une nouvelle surface d'interface **volumineuse** (assistant, dialogue,
  panneau) s'écrit d'emblée dans son propre module à imports explicites, sur
  le modèle de `object_manager.rs` ou `toolbar.rs`, plutôt que d'allonger
  `app.rs` ;
- restent dans `app.rs` : l'ordre des panneaux dans `ui()`, les emprunts
  disjoints de `StokesForgeApp`, l'application différée des actions après la
  peinture, les confirmations bloquantes, `collect_shortcuts` (liste et
  raisons dans #115) ;
- les gardes qui lisent le code parcourent tout `src/`, sous-dossiers
  compris (lecteur unique `tests/support/sources.rs`, #116) : un module
  déplacé reste gardé sans rien déclarer. Une fonction qu'une garde attend
  (`import_window`, `modal_dialog_with`, une entrée de `ZONE_43`…) et qu'on
  renomme fait échouer la garde : mettre la garde à jour, ne pas la
  contourner ;
- aucun `use …::*` hors du code de test (`aucun_import_global_hors_des_tests`,
  `tests/gardes.rs`, garde absolue depuis #117) : un module nouveau nomme ce
  qu'il importe.

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

## 2 quater. Sa place dans les menus — à étudier pour **toute** fonctionnalité

Avant de coder, décider explicitement où l'utilisateur **trouvera** la
fonctionnalité, et le dire dans le compte rendu. Deux familles de menus :

- **la barre de menus** (`StokesForgeApp::menu_bar`) : *File*, *Edit*,
  *Graph* (présent seulement quand un graphe est l'onglet actif,
  `state::active_graph`) et *Window* (noms anglais, clés `menu-*`). Une action qui porte sur l'objet
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
et hexadécimal seulement ; « + Custom » déplie nuancier, opacité,
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
onglets ; `sheet_tabs` (feuilles) et la fenêtre des propriétés du graphe en
sont des usages. Un dialogue modal ne **peint pas de voile** sur le reste de
l'application (issue #20) : il le bloque seulement.

**Aucune exception, y compris pour un assistant à plusieurs étapes.**
L'assistant d'import (`import_window`) est un dialogue modal comme les
propriétés du graphe : `modal_dialog_with(ctx, id, titre, taille, DialogFooter
{ accept_label, accept_hint, accept_enabled }, boutons_du_pied, contenu)`. Le
bouton par défaut peut changer de libellé et se griser (*Finish* grisé sans
fichier) ; des boutons supplémentaires se posent à gauche de *Cancel* par la
fermeture `boutons_du_pied` (*Back*, *Next*). Entrée valide,
Échap annule. **Avant d'écrire une fenêtre, dialogue ou assistant : partir de
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

## 3. Les quatre surfaces d'une action — ce que Qt donnait gratuitement

Une `QAction` reliée à un `QUndoStack` portait d'un coup le raccourci, l'entrée
de menu, son grisage et son libellé. Ici les quatre sont **séparés**, et rien
ne garantit leur cohérence sinon la relecture. C'est le premier point à
vérifier quand une action est ajoutée :

1. **le raccourci** — une constante `SHORTCUT_*` dans `app/shortcuts.rs`,
   déclarée là *et* consommée à la main dans `collect_shortcuts` (`app.rs`) (`input.consume_shortcut`) ;
2. **l'entrée de menu** (barre de menus et/ou menu contextuel), avec son
   `shortcut_text` pour rappeler le raccourci à droite ;
3. **le grisage** (`add_enabled`), calculé depuis `AppState` ;
4. **le libellé**, qui vient de `state` (`AppState::undo_label`,
   `GridMenu::target_label`…), jamais écrit en dur dans `app.rs` quand une
   contextualisation existe déjà. **Tout libellé est une clé du catalogue
   `en`** (`crates/stokesforge-app/i18n/en/stokesforge_app.ftl`), lue par `tr!("clé")` —
   jamais un littéral, même pour un bouton de passage (verrouillé par
   `aucun_libelle_en_dur`, `tests/libelles.rs`). Rédiger le texte selon le
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
  comme `app::grid::system_clipboard_text`).
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

Le menu contextuel de la grille suit trois règles qu'il ne faut pas casser :

- **Il est le miroir exact du clavier.** `app::grid::grid_edit_menu` n'offre aucune
  action qui n'ait pas d'équivalent en raccourci. Ajouter une entrée de menu
  sans geste clavier équivalent, c'est créer une fonctionnalité que seul le
  clic droit atteint — décider explicitement si c'est voulu.
- **La décision vit dans `AppState::grid_menu` → `GridMenu`**, pas dans le code
  egui. C'est ce qui la rend vérifiable sans fenêtre dans `tests/shell.rs` :
  quels gestes ont un sens dans cette zone (`GridZone::Header`/`Data`/
  `Columns`/`Rows`), lesquels griser, comment accorder les libellés en nombre.
- **Les entrées sont des verbes nus** : « Copier », pas « Copier la cellule ».
  Le clic droit vient de se poser sur ce qu'il vise et le raccourci est déjà
  rappelé à droite ; nommer la cible allonge sans rien apprendre.
  `target_label` reste disponible pour les surfaces où l'on ne voit *pas* ce
  qu'on vise (barre de menus, ligne de statut, confirmation).

Un clic droit **hors** de la sélection la réduit à la case visée ; **dedans**,
il la laisse intacte — même garde (`response.secondary_clicked() && !selected`)
dans toutes les zones, sinon on rend impossible une action sur plusieurs
colonnes sélectionnées.

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

## 4. Tests — et, si c'est visuel, regarder

Toute logique ajoutée dans `state.rs` se teste dans
`crates/stokesforge-app/tests/shell.rs` — sans fenêtre, sans serveur graphique.

**Les tests couvrent la géométrie, pas l'apparence.** Un changement visuel
(contour, alignement, largeur, troncature, surbrillance, icône) n'est pas
vérifié par une suite verte : il se vérifie en regardant la fenêtre.
`stokesforge-app` sait se photographier — procédure dans
[`../stokesforge-bug-report/references/voir-l-interface.md`](../stokesforge-bug-report/references/voir-l-interface.md).
Une capture avant et une après valent mieux qu'une affirmation.
Une fonction pure extraite exprès pour être testable (`app::grid::grid_grip_side`,
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
- [ ] code de fenêtre dans le `window_*.rs` concerné ; surface volumineuse dans son propre module (§2 octies, chantier #115)
- [ ] barres de défilement : aucun style local, place lue via `theme::scroll_bar_allocated_width`
- [ ] icônes : composant `icons.rs`, info-bulle `tr!`, seulement si elle apporte quelque chose (`docs/icones.md`)
- [ ] couleurs : `color_picker::color_field` avec l'usage et le `PickerEnv`, vide = automatique
- [ ] existant réutilisé plutôt que dupliqué
- [ ] mutation du modèle → `Command` annulable (skill **stokesforge-commande**)
- [ ] logique dans `state.rs`, glu egui seulement dans `app.rs`
- [ ] place dans la barre de menus **et** les menus contextuels étudiée (§2 quater) ; rien n'est accessible seulement au clic droit
- [ ] les quatre surfaces en accord : raccourci, menu, grisage, libellé
- [ ] libellés : clés du catalogue `en` + `tr!`, aucun littéral (`aucun_libelle_en_dur`) ; un changement visuel est regardé aussi en `STOKESFORGE_LANG=en-XA`
- [ ] menu contextuel = miroir du clavier, décision dans `GridMenu`
- [ ] test dans `tests/shell.rs` (ou le crate concerné)
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

Les textes et objets sélectionnés lisent aussi `with_graph_view` /
`read_graph_view` (propriétés et barres de format incluses). Le menu
contextuel et Graph › Selection partagent `object_node_menu`. Réordonner une
série reste dans son groupe ; le rang final est décidé par `series_drop`,
puis une seule `Action::ReorderSeries` part au relâchement. Les propriétés
conservent le calque exact dans `PlotEdit::selected_panel`.
