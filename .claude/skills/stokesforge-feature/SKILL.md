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
6. **Jumeaux (#300, #299) — pour toute fenêtre, dialogue ou geste neuf.**
   Lister les équivalents existants (`docs/fenetres.md`), dire lequel on
   imite, et comparer une capture (`run-stokesforge`) de la fenêtre neuve avec
   celle de référence. Une fenêtre neuve choisit sa **nature** (`DialogNature`, S-F11 : pied, classe de taille, focus) dans `docs/fenetres.md` et entre dans son catalogue. Une fenêtre neuve a sa **maquette validée par
   l'utilisateur avant le code** (E2 : toujours ; formulaire de décisions,
   cf. `stokesforge-bug-report/references/formulaire-de-decisions.md`). Poser
   aussi la question #299 : « ce geste existe-t-il déjà ailleurs, et sous quel
   modèle ? » (`docs/gestes.md` ; règles S-K7 et S-K8 de
   `docs/standard-interface.md`).
7. Annoncer brièvement : ce qui sera réutilisé, la zone modifiée, les
   vérifications prévues. Puis coder sans redemander confirmation pour les
   choix courants.

Une note de chantier (`docs/chantier-*.md`, `docs/chantiers/`) ne prouve ni
qu'une fonctionnalité existe, ni qu'elle reste à faire. **Le code fait foi.**


## 2. Choisir la couche — la question qui coûte le plus cher

| Ce que fait la fonctionnalité | Où elle vit | Piège |
|---|---|---|
| change le **modèle** (colonnes, arbre, `PlotSpec`, texte de note…) | une `Command` de `stokesforge-commands` | jamais une primitive `stokesforge-core` en direct (invariant 1) ; voir le skill **stokesforge-commande** |
| est un **réglage de vue propre à un document** (rangées d'en-tête d'un classeur) | mémoire d'egui sous l'identifiant du nœud, défaut des nouveaux documents en préférence publiée dans le `Context` (`grid::header_rows`) | jamais un drapeau global : tous les classeurs le suivraient ; rien dans le modèle ni le `.stf` |
| est un **réglage de vue propre à un nœud qui doit survivre au fichier** (position d'une fenêtre, largeur d'une colonne, #325) | une table `NodeId -> valeur` de l'acteur à côté de `window_geometries` (`Request`, `Snapshot`, `ViewTables` du `.stf`, `AppState` sans rafraîchir l'instantané), qui partage `geometry_revision` | ni commande ni historique ; **jamais** purgée à la mutation (Ctrl+Z doit retrouver la valeur, invariant 7), la sauvegarde filtre les clés mortes ; valeurs hostiles écartées à la lecture (invariant 10) ; pousser **à la fin du geste**, jamais à chaque image ; « remettre au défaut » **retire la clé** (la constante du défaut reste dans l'application, #330), la mise en page de la grille en dépend par sa clé de mise en page |
| **analyse une ligne** (#253) ou prépare des colonnes pour un outil qui n'en lit que | une feuille **transposée liée** (`Command::TransposeRows`, `Tree::transposes()`, `app-state/state/transpose.rs`), puis l'outil existant sur sa colonne | jamais un outil « en lignes » ; groupe `MacroKind::AnalyzeRows` ouvert au geste, clos à Apply, avorté à Cancel de la fenêtre modale ; périmée/modifiée **dérivées**, jamais stockées ; refus qui nomme la **lettre** de la colonne |
| **réordonne** des colonnes d'une feuille (#319) | `stokesforge-app-state::column_move` (ordre cible et suite de `Command::Move`, pur) + `Action::MoveColumns` ; entrées dans la **peinture partagée** `column_type_role_menus` | l'index d'un `Move` se compte **après retrait** ; un enfant `Unknown` sous la feuille n'est pas une colonne visible ; une seule macro (`MacroKind::MoveColumns`) ; aucune fenêtre « colonne utilisée » (les formules suivent, #321) |
| change le **cadrage de vue** (zoom, pan) | `ViewState` sur `GraphView` | ce n'est **pas** une mutation du modèle : personne ne veut annuler un zoom par Ctrl+Z |
| est une **décision** d'interface (quoi proposer, quoi griser, comment nommer) | `stokesforge-app-state/src/state.rs` (crate **sans egui**, #263 : un `use egui` n'y compile pas) | doit rester testable sans fenêtre |
| est du **dessin ou de l'événement** egui | le module de **la surface** dans `stokesforge-app/src/app/` (grille `grid.rs`, classeur `workbook.rs`, graphe `graph_window.rs`, menus `menus.rs`, barres de format `format.rs`, dialogue `plot_settings.rs`…, table complète dans `docs/application/coquille-et-code.md`, § Découpage du code ; fiche [rangement-du-code](references/rangement-du-code.md)) — `app.rs` n'en reçoit que l'appel | rien de ce qu'ils contiennent n'est une décision de produit ; jamais une logique de surface dans `app.rs` |
| est une **fonctionnalité de tracé** | `stokesforge-render` (générique sur le backend) | jamais dans un seul backend (invariant 5) ; voir le skill **stokesforge-rendu** |
| lit un **fichier de données** | `stokesforge-import` | reste indépendant de l'UI, de `core`, des commandes et de la session |

Détail de la table, par thème : [couche-et-graphe](references/couche-et-graphe.md).
Un graphe neuf n'a aucun titre par défaut (#103). La ligne de partage
`stokesforge-app-state` / `stokesforge-app` est tenue par le compilateur
(#263) : `state` décide, `app` peint et traduit clavier et souris en `Action`.

## 2 bis. Les fiches : lues à la demande

Le détail vit en fiches `references/` (F1, #301). **Une fiche ne contient
aucune règle** : elle renvoie à `docs/` ou à `.claude/conventions.md` et liste
les points à vérifier. Lire la fiche du thème touché, puis la source qu'elle cite.

| Fiche | Quand la lire |
|---|---|
| [couche-et-graphe](references/couche-et-graphe.md) | geste continu sur un graphe, calque, ajout de séries, action qui lit le zoom |
| [docke-detache](references/docke-detache.md) | toute surface d'interface ; état ajouté à `AppState` ; raccourci global |
| [rangement-du-code](references/rangement-du-code.md) | module neuf, menu, fenêtre, brique, redécoupe de `app.rs` |
| [aspect-commun](references/aspect-commun.md) | défilement, onglets, listes, gras, titres, formulaire, couleurs d'état |
| [menus](references/menus.md) | **toute** fonctionnalité (où la trouve-t-on ?), toute entrée de menu ou de clic droit |
| [couleurs](references/couleurs.md) | champ de couleur, sélecteur, règle de thème |
| [fenetres-dialogues](references/fenetres-dialogues.md) | fenêtre, dialogue, assistant, confirmation, panneau fixe |
| [icones](references/icones.md) | icône affichée, logo d'une action nouvelle |
| [standard-comportement](references/standard-comportement.md) | champ, message, grisage, nombre, clavier, souris, travail long |
| [quatre-surfaces](references/quatre-surfaces.md) | action nouvelle, raccourci, entrée de clic droit, libellé |
| [object-manager-et-analyse](references/object-manager-et-analyse.md) | Object Manager, sélection d'un graphe, correction d'analyse |

Un thème sans fiche (rendu, import, commandes, Python, textes) a son skill :
**stokesforge-rendu**, **stokesforge-import**, **stokesforge-commande**,
**stokesforge-python**, **stokesforge-textes-interface**.

## 3. Les quatre surfaces d'une action

Une action nouvelle a **quatre** surfaces séparées, que rien ne garantit
cohérentes sinon la relecture : raccourci, entrée de menu, grisage, libellé
(clé du catalogue `en`, `tr!`, jamais un littéral). Lire
[quatre-surfaces](references/quatre-surfaces.md) et [menus](references/menus.md).
Décider explicitement, et dire dans le compte rendu, où l'utilisateur
trouvera la fonctionnalité : rien n'est atteignable seulement au clic droit.

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
- **La page `docs/` du domaine** (`application/`, `rendu.md`,
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

- [ ] domaine identifié, page `docs/` lue, `.claude/rules.md` et `.claude/conventions.md` respectés
- [ ] imprécisions levées **avant** de coder, en une seule question ; pas un plafond déjà arbitré (`questions-ouvertes.md` §11)
- [ ] jumeaux listés et capture comparée ; maquette validée pour une fenêtre neuve
- [ ] fiches des thèmes touchés lues et leurs cases cochées (§ 2 bis du skill)
- [ ] existant réutilisé plutôt que dupliqué ; mutation du modèle -> `Command` annulable (skill **stokesforge-commande**)
- [ ] logique dans `state.rs` (`stokesforge-app-state`), glu egui seulement dans `app` ; rien de nouveau dans `app.rs` hors orchestration
- [ ] quatre surfaces en accord ; place dans les menus étudiée
- [ ] libellés : clés du catalogue + `tr!` ; changement visuel regardé aussi en `STOKESFORGE_LANG=en-XA`
- [ ] test au bon endroit : décision d'`AppState` dans `stokesforge-app-state/tests/shell.rs`, geste peint dans le module `tests` de la surface
- [ ] changement visuel **regardé** (capture), pas seulement testé
- [ ] `//!` du module (le *pourquoi*) + page `docs/` (le *quoi*) ; règle nouvelle -> `conventions.md` ou `docs/`, jamais dans une fiche
- [ ] `fmt` + `clippy` + `cargo test --workspace` verts
