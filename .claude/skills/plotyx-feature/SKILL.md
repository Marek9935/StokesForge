---
name: plotyx-feature
description: Implémenter une nouvelle fonctionnalité dans PlotYx (portage Rust, branche rewriterust) — du choix de la couche jusqu'au menu contextuel, au raccourci clavier, aux tests et à la mise à jour de la doc. À utiliser dès que la demande est « ajoute », « implémente », « je voudrais pouvoir… », « il manque… », « fais que… » sur l'application, la grille, les graphes, l'import, les scripts ou le modèle — y compris quand la demande a l'air d'une simple ligne de code, parce que le coût de PlotYx n'est jamais dans la ligne mais dans les quatre surfaces (raccourci, menu, grisage, libellé) et l'entrée d'historique qu'elle oblige à tenir en accord.
---

# Implémenter une fonctionnalité dans PlotYx

PlotYx est un tableur/traceur scientifique façon OriginLab. Une fonctionnalité
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
| change le **modèle** (colonnes, arbre, `PlotSpec`, texte de note…) | une `Command` de `plotyx-commands` | jamais une primitive `plotyx-core` en direct (invariant 1) ; voir le skill **plotyx-commande** |
| change le **cadrage de vue** (zoom, pan) | `ViewState` sur `GraphView` | ce n'est **pas** une mutation du modèle : personne ne veut annuler un zoom par Ctrl+Z |
| est une **décision** d'interface (quoi proposer, quoi griser, comment nommer) | `plotyx-app/src/state.rs` | doit rester testable sans fenêtre |
| est du **dessin ou de l'événement** egui | `plotyx-app/src/app.rs` | rien de ce que contient `app.rs` n'est une décision de produit |
| est une **fonctionnalité de tracé** | `plotyx-render` (générique sur le backend) | jamais dans un seul backend (invariant 5) ; voir le skill **plotyx-rendu** |
| lit un **fichier de données** | `plotyx-import` | reste indépendant de l'UI, de `core`, des commandes et de la session |

La ligne de partage `state.rs` / `app.rs` est la colonne vertébrale du crate
UI : `state` porte toute la logique, pilotée par un enum `Action`, et se teste
en CI (`tests/shell.rs`) ; `app` ne fait que traduire clavier et souris en
`Action` et peindre. Une fenêtre ne se teste pas en intégration continue —
c'est toute la raison du découpage. Si une règle métier se retrouve dans
`app.rs`, elle devient invérifiable.

## 2 bis. Le doublon docké / détaché — à vérifier avant de chiffrer

Les contenus de `plotyx-app` ont longtemps existé **en deux écritures** : une
pour la fenêtre flottante, une pour la fenêtre détachée. Depuis le 2026-09-30,
classeur, graphe, note et matrice n'en ont plus qu'une (`book_contents`,
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
rg "fn <nom>|fn detached_<nom>" crates/plotyx-app/src/app.rs
```

Deux résultats → le changement se fait deux fois, ou mieux : **se factorise**.
Une factorisation supplémentaire est légitime quand elle ne bute pas sur la
contrainte `'static` ci-dessus — un bandeau de boutons qui ne fait que pousser
des `Action` n'y bute pas. Le dire explicitement dans le compte rendu : « j'ai
factorisé X » ou « j'ai dû modifier les deux écritures parce que Y ».

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
celui de PlotYx.

## 2 quater. Sa place dans les menus — à étudier pour **toute** fonctionnalité

Avant de coder, décider explicitement où l'utilisateur **trouvera** la
fonctionnalité, et le dire dans le compte rendu. Deux familles de menus :

- **la barre de menus** (`PlotyxApp::menu_bar`) : « Fichier », « Édition »,
  « Graphique » (présent seulement quand un graphe est l'onglet actif,
  `state::active_graph`) et « Fenêtre ». Une action qui porte sur l'objet
  actif (le graphe, le classeur…) y a presque toujours sa place : c'est là
  qu'un utilisateur d'Origin la cherche. Si aucun menu existant ne convient
  (un menu « Classeur » quand un classeur est actif, par exemple), le créer
  sur le modèle de « Graphique » plutôt que de ranger l'action ailleurs ;
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
Some("Automatique…"), &mut env)`, avec l'usage de la couleur — `Curves` pour
une courbe (l'outil des courbes, palettes nommées), `Text` pour du texte ou
une échelle, `Background` pour un fond — et le `PickerEnv` des couleurs
enregistrées, dont les demandes sont traduites en `Action::SaveColor`/
`RemoveSavedColor` par l'appelant (modèle : `plot_settings_contents`). Une
couleur peut porter une opacité (`#RRGGBBAA`). Dans le modèle, une couleur
vide veut dire « automatique » ; pour du texte ou une
échelle, le rendu la résout par `plotyx_render::color::auto_ink` (blanc sur
fond sombre) — dans `plotyx-render`, jamais dans un backend. Un nouveau
champ de couleur du modèle porte `#[serde(default)]` côté
`plotyx-persistence` (`docs/persistance.md`).

**Un menu déroulant dans une fenêtre surgissante exige une `egui::Area`
à soi, pas `egui::Popup`** : egui ne retient qu'une fenêtre surgissante
ouverte par viewport, donc la liste d'un `ComboBox` refermerait la fenêtre
parente (issue #9). `color_button` tient l'état ouvert/fermé lui-même et
ferme sur Échap ou clic extérieur ; s'en inspirer.

**Le sélecteur s'ouvre replié** (issue #10) : palette, couleurs enregistrées
et hexadécimal seulement ; « + Personnalisée… » déplie nuancier, opacité,
RVB/TSL et Enregistrer. Tout nouvel élément lourd du sélecteur va dans la
partie dépliée (`if custom`), et l'état se remet à zéro à la fermeture
(`set_picker_open`).

**Couleur globale ou thème** (issue #11) : une règle qui touche plusieurs
champs de couleur vit dans `plotyx-plot::theme` (`PlotSpec::set_text_color`,
`set_axes_color`, `apply_theme`), pas dans l'interface. Elle écrit dans les
champs locaux — pas de champ « global » dans le modèle — donc le dernier
réglage gagne ; un nouveau champ de couleur du modèle doit y être ajouté.

## 2 sexies. Fenêtres et dialogues

Toute fenêtre de l'application se construit par la brique commune
(`show_window_in_layer` : cadre, contour actif, rognage ; `window_header`
pour la barre de titre), **jamais** par une `egui::Window` brute. Un dialogue
qui doit bloquer le reste de l'interface passe par `modal_dialog` (fenêtre des
propriétés du graphe) : calque plein écran qui capte les clics, contour rouge
foncé qui clignote sur un clic bloqué, boutons Valider (défaut, Entrée) /
Annuler (croix, Échap) rendus par `DialogOutcome`. Les gestes d'un dialogue
d'édition vivent dans une macro d'historique (`begin_macro`) : Valider la
ferme (une entrée), Annuler l'avorte (modèle : `cancel_plot_settings`). **Tout jeu d'onglets passe par `tab_strip`** (issue #19), la bande des
onglets de feuille rendue générique : défilement horizontal sans barre,
boutons « < » « > » quand des onglets sont cachés, défilement minimal vers
l'onglet choisi. Ne jamais écrire une rangée de `selectable_label` pour des
onglets ; `sheet_tabs` (feuilles) et la fenêtre des propriétés du graphe en
sont des usages. Un dialogue modal ne **peint pas de voile** sur le reste de
l'application (issue #20) : il le bloque seulement.

La fenêtre d'import
(`import_window`) est encore une `egui::Window` brute (issues #13/#14).

## 3. Les quatre surfaces d'une action — ce que Qt donnait gratuitement

Une `QAction` reliée à un `QUndoStack` portait d'un coup le raccourci, l'entrée
de menu, son grisage et son libellé. Ici les quatre sont **séparés**, et rien
ne garantit leur cohérence sinon la relecture. C'est le premier point à
vérifier quand une action est ajoutée :

1. **le raccourci** — une constante `SHORTCUT_*` dans `app.rs`, déclarée *et*
   consommée à la main (`input.consume_shortcut`) ;
2. **l'entrée de menu** (barre de menus et/ou menu contextuel), avec son
   `shortcut_text` pour rappeler le raccourci à droite ;
3. **le grisage** (`add_enabled`), calculé depuis `AppState` ;
4. **le libellé**, qui vient de `state` (`AppState::undo_label`,
   `GridMenu::target_label`…), jamais écrit en dur dans `app.rs` quand une
   contextualisation existe déjà.

### Ajouter un raccourci clavier

- Vérifier d'abord les collisions : `rg "SHORTCUT_" crates/plotyx-app/src/app.rs`.
- **Ctrl+C / Ctrl+X / Ctrl+V ne sont jamais reçus comme `Event::Key`** :
  `egui-winit` les intercepte à la traduction des événements de fenêtre.
  Les constantes `SHORTCUT_COPY`/`CUT`/`PASTE`/`COPY_FULL_COLUMNS`/
  `PASTE_ABSOLUTE` sont **purement décoratives** — elles n'existent que pour
  écrire le rappel à droite d'une entrée de menu. Rien ne les consomme, et un
  essai antérieur fondé sur l'hypothèse inverse produisait des raccourcis qui
  ne se déclenchaient jamais. Si le geste doit passer par le presse-papier
  système, c'est `Event::Copy`/`Event::Paste` (ou `arboard` en lecture directe,
  comme `app::system_clipboard_text`).
- Les raccourcis sont consommés **au niveau du contexte**, donc avant les
  widgets : Ctrl+Z pendant la saisie d'une cellule annule la dernière commande
  du modèle, pas la dernière frappe. C'est voulu (comportement de tableur).

### Ajouter une entrée de menu contextuel

Le menu contextuel de la grille suit trois règles qu'il ne faut pas casser :

- **Il est le miroir exact du clavier.** `app::grid_edit_menu` n'offre aucune
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
`crates/plotyx-app/tests/shell.rs` — sans fenêtre, sans serveur graphique.

**Les tests couvrent la géométrie, pas l'apparence.** Un changement visuel
(contour, alignement, largeur, troncature, surbrillance, icône) n'est pas
vérifié par une suite verte : il se vérifie en regardant la fenêtre.
`plotyx-app` sait se photographier — procédure dans
[`../plotyx-bug-report/references/voir-l-interface.md`](../plotyx-bug-report/references/voir-l-interface.md).
Une capture avant et une après valent mieux qu'une affirmation.
Une fonction pure extraite exprès pour être testable (`app::grid_grip_side`,
`state::active_graph`, `state::drop_payload_action`) est le motif idiomatique
du dépôt quand une décision doit vivre près de l'UI : extraire, puis tester.

Détail des familles de tests, du principe du **témoin** et des garde-fous
d'architecture : skill **plotyx-verification** et `docs/tests.md`.

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

Enchaîner sur le skill **plotyx-verification** : `fmt`, `clippy -D warnings`,
`cargo test --workspace`, plus les features hors défaut si le changement les
touche (`python`, `typst`), relecture du diff, message de commit en français.

## Checklist

- [ ] domaine identifié, page `docs/` lue, `.claude/rules.md` respecté
- [ ] imprécisions de la demande levées **avant** de coder, en une seule question
- [ ] ce n'est pas un plafond déjà arbitré (`questions-ouvertes.md` §11)
- [ ] doublon docké/détaché vérifié (`rg "fn <nom>|fn detached_<nom>"`)
- [ ] barres de défilement : aucun style local, place lue via `theme::scroll_bar_allocated_width`
- [ ] couleurs : `color_picker::color_field` avec l'usage et le `PickerEnv`, vide = automatique
- [ ] existant réutilisé plutôt que dupliqué
- [ ] mutation du modèle → `Command` annulable (skill **plotyx-commande**)
- [ ] logique dans `state.rs`, glu egui seulement dans `app.rs`
- [ ] place dans la barre de menus **et** les menus contextuels étudiée (§2 quater) ; rien n'est accessible seulement au clic droit
- [ ] les quatre surfaces en accord : raccourci, menu, grisage, libellé
- [ ] menu contextuel = miroir du clavier, décision dans `GridMenu`
- [ ] test dans `tests/shell.rs` (ou le crate concerné)
- [ ] changement visuel **regardé** (capture), pas seulement testé
- [ ] `//!` du module mis à jour (le *pourquoi*) + page `docs/` (le *quoi*)
- [ ] `fmt` + `clippy` + `cargo test --workspace` verts
