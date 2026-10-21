---
name: stokesforge-commande
description: Ajouter ou modifier une commande annulable de StokesForge (`stokesforge-commands`) — la seule voie de mutation du modèle. À utiliser dès qu'une tâche doit créer, supprimer, renommer, déplacer ou modifier quoi que ce soit dans le projet (colonne, feuille, classeur, graphe, note, matrice, dossier, `PlotSpec`, métadonnées de fenêtre), ou dès qu'une méthode `pub` mutante est ajoutée à `stokesforge-core`. Couvre la chaîne complète : primitive core → variante de `Command` → delta d'historique → transport `.stf` → acteur de session → `Action` d'interface, et les tests qui tiennent chaque maillon.
---

# Ajouter une commande annulable

**Invariant 1 : aucune mutation du modèle hors d'une commande.** Ce n'est pas
une convention à surveiller mais une propriété de propriété : l'acteur de
`stokesforge-session` détient le `&mut Project` **de la session** (construction d'un projet, tests et CLI passent par les primitives de `core`), un client n'a
qu'un `Arc<Tree>` figé. Pour muter, il faut passer par
`SessionHandle::execute(Command)` — donc par la pile d'historique, donc être
annulable.

Corollaire à connaître avant de commencer : ajouter une méthode `pub` mutante à
`stokesforge-core` **sans** commande correspondante ouvre un chemin d'écriture non
annulable. La batterie `b21` de `stokesforge-robustesse` le vérifie et échouera.

Cycle de vie (#73) : `SessionHandle::new_project`, comme `load`, remplace
l'identité et vide historique/géométries ; ce n'est pas une commande de
mutation du projet courant. Refus d'une macro ouverte, nouvelle époque pour
invalider les références Python ; confirmation par `AppState`. Un graphique
vide, lui, est une mutation annulable : `Command::AddGraph` existe déjà.

## Lire avant d'écrire

`docs/commandes-et-historique.md` d'abord, puis
`stokesforge-commands/src/command.rs` (doc de tête + `Command::is_structural`) —
c'est la liste qui fait foi, pas le tableau d'une page de doc. Si le format
`.stf` est touché : `docs/persistance.md`, puis
`docs/archive/rustrewrite/format-fichier-plx.md` pour les options déjà
écartées.

## Ce qu'est une commande

**Une valeur, pas un objet à état.** Elle ne retient ni `Project`, ni `Tree`,
ni le moindre `Arc` : c'est ce qui lui permet de traverser un canal
inter-threads telle quelle, de l'UI vers l'acteur.

Tout ce qu'il faut pour annuler est calculé **à l'application**, par `History`,
jamais porté par la commande. Une commande qui transporterait son propre
« undo » serait un objet à état déguisé.

Conventions déjà arbitrées, à suivre plutôt qu'à rejouer :

- **une commande de création par nature de nœud** (`AddFolder`, `AddWorkbook`,
  `AddWorksheet`, `AddGraph`, `AddMatrix`, `AddNote`) plutôt qu'un `AddNode`
  générique ; `RemoveNode` reste générique, comme `Project::remove` ;
- **les commandes d'édition de contenu remplacent leur cible en bloc**
  (`SetWindowMeta`, `SetNoteText`, `SetColumnMeta`, `SetPlotSpec`, à l'image de
  `SetCells`) — pas d'édition par sous-champ, qui multiplierait les variantes
  et les deltas sans rien gagner ; `EditPlotSpec` réutilise le delta
  `SetPlotSpec` et ajoute seulement un motif fermé `PlotEditKind` pour
  l'historique des gestes du graphe (#84–85) ;
- **`expected_version`** est la condition optimiste des écritures de cellules :
  `None` ne vérifie rien (le comportement de l'UI), `Some(v)` exige que
  `Column::version` vaille encore `v` **au moment d'appliquer** — c'est ce qui
  ferme le TOCTOU entre l'instantané qu'un script a lu et la commande qu'il
  envoie.
- **toute écriture de données d'une colonne tire une version neuve**
  (`Project::next_column_version`, comme `set_values`, `set_length`,
  `set_column_type`, `insert_rows`…), et une colonne **insérée** avec un
  numéro déjà porté fait passer le compteur au-delà (`Project::insert_node`,
  que tout ajout de nœud doit emprunter) : le recalcul ciblé des formules
  (#193, invariant 11) compare des versions et croit à jour une formule dont
  une source aurait changé sous un ancien numéro. Une nouvelle commande qui écrit
  des cellules passe donc par ces primitives, jamais par la `Column` en
  direct ; si elle peut viser une colonne-formule, un cas nommé dans
  `stokesforge-session/tests/recalc_cible.rs` vérifie que la formule la
  réécrit (`docs/tests.md`, « Recalcul ciblé », B).
- **une formule retient ses colonnes par identifiant** (`ColumnFormula`, #321) :
  une commande qui insère, supprime ou déplace des colonnes ne change jamais ce
  qu'une formule lit, et n'a pas à réécrire de texte. Une commande qui pose
  `ColumnMeta::formula` reçoit une `ColumnFormula` déjà construite (lettres vers
  identifiants par `Tree::formula_from_letters`, **au-dessus** de la commande :
  `SetColumnMeta` ne valide jamais les références). Une copie de colonnes ou de
  feuille renumérote les identifiants lus (`Project::insert_copy`).

## Structurelle ou données — la question qui décide du coût

L'historique est **hybride** : la **structure** est retenue par instantanés
d'arbre, les **données** par deltas. `Command::is_structural` tranche, et ce
choix n'est pas cosmétique.

De cette asymétrie découle l'**invariant 8, LIFO strict** : restaurer un arbre
entier écrase tout ce qui a été écrit après lui, donc une entrée structurelle
ne s'annule qu'une fois toutes les entrées postérieures annulées. Dans une
macro, les entrées s'annulent strictement en ordre inverse d'insertion, quelles
que soient leurs natures.

Classer une commande de données comme structurelle coûte un instantané d'arbre
à chaque appel ; l'inverse casse l'annulation. En cas de doute, regarder ce que
fait une commande voisine déjà arbitrée — par exemple `SetCellsPromoting`, qui
change *le type* d'une colonne et reste « données » parce que son delta est un
instantané de **colonne**, pas d'arbre, et qui fait tenir conversion et
écriture dans **une seule** entrée d'historique (un `Ctrl+Z` ramène le type
*et* la valeur d'avant).

## La chaîne complète

Dans cet ordre, en s'arrêtant aux maillons que la commande touche réellement :

1. **`stokesforge-core`** — la primitive de mutation (`&mut self` sur `Project`).
   Chercher d'abord : elle existe peut-être déjà.
2. **`stokesforge-commands/src/command.rs`** — la variante de `Command`, son
   classement dans `is_structural`, son libellé d'historique.
   **Le libellé est une donnée, pas une phrase** (#46) : une variante de
   `HistoryLabel` (`src/label.rs`) avec ses paramètres, jamais une `String`.
   Ajouter la variante, sa ligne de `Display` (anglais, journaux et CLI), et
   l'exemple de `stokesforge-commands/tests/labels.rs` (le test exhaustif échoue
   sinon). Un groupe ouvert par `begin_macro` est un `MacroKind` fermé : une
   nouvelle nature de groupe ajoute une variante, pas un texte (`MoveColumns`,
   #319 : une colonne = un `execute` seul, plusieurs = une macro ; `MoveObjects`
   reste celle du Project Manager).
   **Piège de libellé** : une colonne a deux identités, et une seule est
   visible. Son nom de nœud (« Temps ») est une clé du modèle ; la grille
   n'affiche que sa **lettre de position** (A, B, C…). Un libellé d'historique
   doit désigner une colonne par sa position (`ColumnRef`, depuis
   `command::column_ref` ; lettre par `stokesforge_core::spreadsheet_name`), sinon il
   nomme un mot introuvable à l'écran. Les autres natures de nœud gardent leur
   nom réel — celui-là *est* affiché, dans le Project Manager.
3. **`stokesforge-commands/src/history.rs`** — le delta calculé à l'application, et
   son annulation. Une commande de données qui **déplace des lignes** réécrit
   dans le même delta les références de ligne qui en dépendent (séries
   « ligne » des graphes : champ `plots` de `InsertRows`/`DeleteRows`, #307) :
   specs avant/après capturés à l'application, écrits par `set_plot_spec`, pas
   d'inverse recalculé.
4. **`stokesforge-persistence`** — seulement si la commande écrit un état qui doit
   survivre au rechargement. `convert.rs`/`schema.rs` ont leurs propres types
   de transport : `stokesforge-plot` ne dépend pas de `serde` (invariant 3), la
   conversion se fait ici. Une variante d'énumération inconnue à la lecture est
   capturée telle quelle dans un `RawVariant` — la perte est bornée à ce nœud,
   jamais au fichier (invariant 10).
5. **`stokesforge-session`** — en général rien à faire : l'acteur applique n'importe
   quelle `Command`. Vérifier seulement si la commande a besoin d'un nouveau
   type de réponse.
6. **`stokesforge-app`** — une variante d'`Action`, son traitement dans
   `AppState::dispatch`, puis le câblage UI (skill **stokesforge-feature** pour les
   quatre surfaces : raccourci, menu, grisage, libellé). **La phrase
   d'historique** s'écrit ici : la clé `history-*` dans
   `stokesforge-app-state/i18n/en/stokesforge_app.ftl` (Title Case, verbe, pluriel par sélecteur Fluent —
   skill **stokesforge-textes-interface**) et la branche de
   `stokesforge-app-state/src/history_text.rs` (`match` exhaustif : une variante sans phrase ne compile
   pas). `tests/history_text.rs` vérifie que le catalogue `en` et le `Display`
   disent la même chose : le mettre d'accord.

**Ne pas oublier le rafraîchissement** : l'instantané détenu par l'UI est figé
(l'arbre est persistant). Il faut le redemander à l'acteur après toute mutation
— c'est ce que fait `refresh`, appelé par `dispatch`.

## Tests à prévoir

| Maillon | Test |
|---|---|
| annulation/rétablissement | aller-retour complet : état avant == état après undo |
| nature structurelle | l'ordre LIFO tient dans une macro (`stokesforge-commands/tests/history.rs`) |
| persistance | aller-retour `.stf` (`stokesforge-persistence/tests/roundtrip.rs`) |
| chemin d'écriture non annulable | `b21` doit rester vert |
| écriture de données de colonne | le geste ajouté aux tirages de `b14` (version ↔ contenu) et de `b22` (recalcul ciblé contre complet) |
| mémoire | `stokesforge-commands/tests/memory_pinning.rs` : l'historique n'épingle pas indéfiniment de gros buffers |
| interface | `stokesforge-app-state/tests/shell.rs` pour l'`Action` correspondante |

Le banc de robustesse a des batteries dédiées qu'il vaut mieux étendre que
dupliquer : `b5` (macros), `b12` (commandes acceptées sans effet), `b14`
(balayage revérifiant trois invariants après chaque pas d'une séquence tirée au
sort), `b18` (ce qui reste possible pendant qu'une macro est ouverte). Index
dans `stokesforge-robustesse/src/lib.rs`.

## Checklist

- [ ] `docs/commandes-et-historique.md` et la doc de tête de `command.rs` lues
- [ ] la primitive `core` existe (ou est ajoutée avec sa commande, sinon `b21` échoue)
- [ ] classement structurelle/données assumé et cohérent avec le coût du delta
- [ ] libellé d'historique : variante de `HistoryLabel` (+ `Display`, exemple de `tests/labels.rs`, clé `history-*`, branche de `history_text.rs`), colonne désignée par sa **position**, pas son nom de nœud
- [ ] transport `.stf` ajouté si l'état doit survivre au rechargement
- [ ] `Action` + `dispatch` + rafraîchissement de l'instantané côté UI
- [ ] tests : undo/redo, macro LIFO, aller-retour `.stf`, `b21`
- [ ] écriture de cellules : version neuve par `Project`, geste ajouté à `b14` et `b22`
- [ ] `fmt` + `clippy` + `cargo test --workspace` verts

## Copie générique (#105–106)

Réutiliser `NodeCopy::capture` et `Command::InsertCopy` pour les nouvelles
natures : elles sont copiables par défaut. Ne transporter ni arbre complet ni
allocateur source dans une commande. `Project::insert_copy` alloue chaque ID
au collage et restaure l'arbre en cas d'échec ; `DuplicateWorksheet` l'utilise
également. Les groupes utilisent les `MacroKind::*Objects` ; les graphes gardent
leurs références dans le même projet ; entre projets, `NodeCopy::into_external`
active les données embarquées et le remplacement de toutes les références (#107). Le
classeur caché est persisté et révélé par `SetWindowMeta`.

## Correction définitive d'une analyse (#212)

Réutiliser `ReplaceAnalysisData` pour remplacer la source Y d'une ligne de base
ou d'une correction par référence. La trace immuable est un `CorrectionRecord`
à part des calculs ; valeurs et trace sont annulées ensemble. Exiger les
versions de toutes les entrées lues, préserver longueur/lignes manquantes,
refuser les corrections contradictoires d'un même Y. Dans un formulaire,
griser le choix avec `ChoiceOption::disabled` et sa raison ; la confirmation
des sources partagées réutilise `modal_confirm`.
