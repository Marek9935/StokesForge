# Commandes et historique

`stokesforge-commands` est la **seule** couche qui mute un `Project` en dehors des
tests. C'est la règle d'or du socle (invariant 1), et ce crate en est le point
de passage obligé.

## Une commande est une valeur

Pas un objet à état : elle ne retient ni `Project`, ni `Tree`, ni le moindre
`Arc`. C'est ce qui lui permet de traverser un canal inter-threads telle
quelle, de l'UI vers l'acteur qui détient l'unique `&mut Project`.

Tout ce qu'il faut pour annuler est calculé **à l'application**, par
`History`, jamais porté par la commande.

Liste faisant foi : `stokesforge-commands/src/command.rs` (`Command`,
`Command::is_structural`). Résumé :

| Commande | Nature |
|---|---|
| `SetCells { column, start, values, expected_version }` | données |
| `SetCellsPromoting { column, start, values, expected_version }` | données |
| `AddColumn { sheet, index, name, column }` | structure |
| `DeleteColumn { column }` | structure |
| `Rename { id, name }` | structure |
| `Move { id, new_parent, index }` | structure |
| `SetColumnType { column, column_type }` | données |
| `InsertRows { sheet, … }` | données |
| `DeleteRows { sheet, … }` | données |
| `AddFolder { parent, name }` | structure |
| `AddWorkbook { parent, name }` | structure |
| `AddEmptyWorkbook { parent, name }` | structure |
| `AddWorksheet { book, name }` | structure |
| `DuplicateWorksheet { sheet, name }` | structure (le nom « copie N » est composé par l'application : phrase d'interface) |
| `AddGraph { parent, name, spec }` | structure |
| `AddMatrix { parent, name }` | structure |
| `AddNote { parent, name, text }` | structure |
| `RemoveNode { id }` | structure |
| `SetWindowMeta { id, meta }` | données |
| `SetNoteText { id, text }` | données |
| `SetColumnMeta { column, meta }` | données |
| `SetPlotSpec { graph, spec }` | données |
| `EditPlotSpec { graph, spec, kind }` | données (même delta de spec, motif `PlotEditKind`) |
| `SetSavedColors { colors }` | données |

Le graphique vide de « File › Add Graph » (#73) réutilise `AddGraph` avec
`PlotSpec::empty("")` : une zone de tracé, aucune série, même annulation et
persistance qu’un graphique construit depuis des colonnes. « New Project »
remplace l’identité du projet via la session, comme `load` ; il vide
l’historique après confirmation et ne représente pas une commande annulable.

Une commande de création par nature de nœud plutôt qu'un `AddNode` générique
(voir la doc de tête de `command.rs`) ; `RemoveNode` reste générique, comme
`Project::remove`. Les quatre commandes d'édition de contenu
(`SetWindowMeta`/`SetNoteText`/`SetColumnMeta`/`SetPlotSpec`) remplacent leur
cible **en bloc**, à l'image de `SetCells` — pas d'édition par sous-champ.

### Deux écritures de cellules, stricte et tolérante

`SetCells` refuse un bloc qui ne correspond pas au type de la colonne
(`CoreError::TypeMismatch`), et c'est ce que veut un appelant qui sait ce
qu'il écrit — le pont Python, un script. `SetCellsPromoting` **élargit la
colonne** en `ColumnType::Mixed` plutôt que de refuser : c'est la commande de
la saisie en grille, où taper « n/a » dans une colonne de mesures doit
produire une cellule texte, pas une erreur
([`modele-de-donnees.md`](modele-de-donnees.md)).

Le choix du chemin se fait à l'**application**, jamais à l'appel :

| Situation | Delta | Coût |
|---|---|---|
| le bloc convient déjà au type | `Cells` | celui de `SetCells`, à l'identique |
| la colonne est déjà mixte, le bloc non | `Cells` | idem : seul le bloc est converti |
| la colonne n'est pas mixte et le bloc ne lui convient pas | `ColumnType` | un instantané de colonne, comme `SetColumnType` qu'elle contient |

Le troisième chemin n'a lieu qu'**une fois par colonne** : la saisie tolérante
suivante trouve une colonne déjà mixte et retombe sur le deuxième. Et la
conversion et l'écriture tiennent dans la **même** entrée d'historique, donc
un seul `Ctrl+Z` ramène le type *et* la valeur d'avant.

`expected_version` est la condition optimiste : `None` ne vérifie rien (le
comportement de l'UI, qui n'a pas cette discipline) ; `Some(v)` exige que
`Column::version` vaille encore exactement `v` au **moment d'appliquer**,
jamais avant — sans quoi la commande est refusée
(`CoreError::ConcurrentModification`) sans toucher au modèle ni à
l'historique. C'est ce que `stokesforge-script` remplit pour fermer le TOCTOU entre
l'instantané qu'un script a lu et la commande qu'il envoie.

Ce compteur sert aussi au recalcul ciblé des colonnes-formule (#193,
[`session.md`](session.md)) : l'acteur ne recalcule une formule que si la
version d'une de ses sources a changé. Toute écriture de données d'une
colonne — commande, annulation, rétablissement — doit donc recevoir un
numéro neuf du compteur unique du projet (`Project::next_column_version`),
une colonne insérée avec un numéro (`AddColumn`, collage) fait passer ce
compteur au-delà (`Project::insert_node`), et une restauration d'arbre ne
doit ramener que des paires (version, contenu) qui ont réellement coexisté
(`restamp_regressed_versions`). Une commande qui écrirait des cellules
autrement ferait passer une formule périmée pour à jour (invariant 11).

## L'historique est hybride, et c'est une décision de coût

Structure et données n'ont pas le même prix.

| Nature | Ce qui est retenu | Pourquoi |
|---|---|---|
| **structure** (arbre, noms, ordre, ajout/suppression) | deux `Arc<Tree>`, `before` et `after` | l'arbre est persistant : les deux versions partagent tout ce qui n'a pas bougé, garder les deux coûte un pointeur |
| **données** (contenu des colonnes) | un **delta** : anciennes *et* nouvelles valeurs des seules cellules écrites | un instantané retiendrait le buffer entier de la colonne — 8 Mo pour un million de points — à chaque frappe |

Deux conséquences agréables du côté structure : l'arbre `before` retient les
nœuds supprimés, il n'y a donc rien à libérer explicitement ; et un nœud
restauré revient sous son **identifiant d'origine**, puisque `replace_tree` ne
fait pas reculer l'allocateur.

### Ce que l'hybride impose : LIFO strict

Restaurer un arbre entier écrase tout ce qui a été écrit après lui. Une entrée
structurelle ne s'annule donc qu'une fois toutes les entrées postérieures
annulées — garanti par construction, et **dans une macro les entrées
s'annulent strictement en ordre inverse d'insertion**, quelles que soient leurs
natures. C'est ce qui fait tenir l'hybride (invariant 8).

## Deux limites, la plus contraignante gagne

Une limite comptée en **entrées** (`DEFAULT_LIMIT` = 100) ne voit pas le motif
que mesure `tests/memory_pinning.rs` : en alternant une opération de structure
et une écriture sur une même grosse colonne, chaque entrée structurelle épingle
une version complète du buffer que l'écriture suivante remplace. Cent
renommages ne pèsent rien ; une seule suppression de classeur pèse des
centaines de méga-octets.

D'où `byte_limit` (`DEFAULT_BYTE_LIMIT` = 512 Mio), qui borne directement les
octets de charge utile que l'historique est **seul** à maintenir vivants
(`History::retained_bytes`, exact en permanence par suivi incrémental).

Trois règles d'éviction à connaître, toutes délibérées :

- **la dernière entrée restante n'est jamais évincée**, même si elle dépasse
  à elle seule la limite — sinon la limite produit exactement le désagrément
  qu'elle cherche à éviter : plus rien à annuler juste après l'opération
  lourde ;
- l'éviction ne touche **jamais** la pile de rétablissement, qui n'est vidée
  que par le prochain empilement. Une longue branche de `redo` peut donc
  maintenir le total au-dessus du plafond jusqu'à la mutation suivante ;
- `set_limit`/`set_byte_limit` refusent (`StackNotEmpty`) si une pile n'est
  pas vide : décider quoi évincer au milieu d'un historique n'appartient pas à
  la pile.

## Libellés d'historique : une donnée, pas une phrase

Chaque entrée porte un `HistoryLabel` (`stokesforge-commands/src/label.rs`) : une
énumération — l'action et ses paramètres (`InsertRows { count }`,
`Rename { from, to }`, `SetCells { column: Option<ColumnRef> }`…), plus
`Macro(MacroKind)` pour un groupe. `Command::label(&Tree)` le calcule sur
l'arbre **d'avant** (le nom d'un nœud supprimé n'existe plus après) et ne peut
pas échouer : un nœud introuvable donne `None`, que la phrase rend sous une
forme générique. Une colonne se désigne par sa position (`ColumnRef`, lettre de
la grille), jamais par son nom de nœud.

Le crate ne met **rien** en phrase et ne dépend pas de Fluent :

| Où | Rôle |
|---|---|
| `stokesforge-commands` | rend la donnée ; `Display` en **anglais** pour les journaux et le CLI |
| `stokesforge-app/src/history_text.rs` | `history_text` : la variante en phrase, par `tr!` et les clés `history-*` du catalogue `en`, pluriels Fluent ; `match` exhaustif |
| `stokesforge-app/tests/history_text.rs` | le catalogue `en` et le `Display` disent la même chose, variante par variante |

`begin_macro` prend un `MacroKind`, énumération **fermée** (coller, effacer la
sélection, propriétés du graphe, actualiser, importer, créer un classeur ou une
feuille, exécuter un script) : un groupe est une valeur qui se compare (`undo_label ==
Some(HistoryLabel::Macro(MacroKind::Import))`), pas un texte à relire. Un
nouveau groupe ajoute une variante de `MacroKind`, sa clé `history-macro-*` et
sa ligne de `Display`. L'historique n'est pas écrit dans le `.stf`
([`persistance.md`](persistance.md)).

## Macros

`begin_macro` / `end_macro` / `abort_macro` groupent des commandes en une seule
entrée annulable. L'imbrication est permise jusqu'à `MAX_MACRO_DEPTH` (1000) —
une macro plus profonde est refusée plutôt que de faire déborder la pile
d'appels.

Attention : une macro n'empêche jamais le `Project` d'être muté ; elle **diffère
seulement l'entrée d'historique**. C'est pourquoi `save` est refusé pendant une
macro ouverte, et pourquoi une macro encore ouverte à l'arrêt de l'acteur est
**annulée, pas commise** ([`session.md`](session.md)).

Ce que l'identité du client change pendant une macro ouverte — qui peut
exécuter, fermer, avorter — appartient à l'acteur et se lit dans la table de
`stokesforge-session/src/lib.rs`.

## Les erreurs

Les erreurs du modèle remontent **telles quelles** (`CommandError::Core`,
transparente) : une commande qui échoue échoue pour une raison du cœur, pas
pour une raison inventée ici. Les autres variantes décrivent les seuls refus
propres à la pile : macro ouverte, pas de macro, pile non vide, macro
étrangère, macro trop profonde.

## Ce qu'il faut savoir avant de modifier

- Ajouter une commande, c'est aussi décider sa **nature** (`is_structural`) :
  s'en tromper casse l'invariant LIFO sans rien lever.
- `tests/no_ui_dependency.rs` liste les dépendances autorisées du crate.
- `tests/undo_random.rs` tire des séquences au sort ; `tests/history.rs` porte
  les cas nommés ; `tests/memory_pinning.rs` mesure l'épinglage mémoire.
  Côté robustesse : `b5` (macros), `b12` (commandes acceptées sans effet),
  `b18` (ce qui reste possible pendant qu'une macro est ouverte), `b21`
  (coût et identité sur le chemin d'écriture).

**À lire** : `stokesforge-commands/src/lib.rs` et `src/history.rs` (têtes).

Object Manager (#81–85) réutilise le delta de `SetPlotSpec` par
`Command::EditPlotSpec { graph, spec, kind }` : un remplacement en bloc,
avec le motif `PlotEditKind` dans `HistoryLabel::EditPlotSpec`. Visibilité,
renommage, ordre et suppression ont leurs clés `history-plot-*`, sans phrase
transportée par l'acteur. Le menu et l'arbre émettent les mêmes `Action`.
Une cible périmée, un rang inchangé ou une valeur identique n'envoie aucune
commande. Ouvrir le panneau, choisir un contenu ou ouvrir ses propriétés
relève de l'état de vue ; les modifications validées sont annulables.

## Copie d'objets (#105–106)

`NodeCopy::capture` fige un sous-arbre sans conserver le projet ni son allocateur.
`Command::InsertCopy` le recrée par `Project::insert_copy`, avec de nouveaux
identifiants alloués dans la destination. Toute nature est copiable par défaut :
la copie conserve son contenu et reconstruit ses enfants, sans liste blanche.
Un échec restaure l'arbre ; le compteur d'identifiants ne recule jamais.
`DuplicateWorksheet` réutilise cette primitive atomique et replace la feuille
juste après l'originale, y compris si des natures inconnues y sont conservées.

Les gestes multiples sont groupés par `MacroKind::{PasteObjects,
DuplicateObjects, DeleteObjects, RenameObjects, MoveObjects}` ; un échec
intermédiaire avorte le groupe entier. Ces libellés restent des données et ne
sont pas enregistrés dans le `.stf`.

Pour les graphes inter-projets (#107), la copie porte aussi les dépendances
figées ; `NodeCopy::into_external` active leur insertion dans `InsertCopy`.
Les classeurs cachés et les graphes sont créés atomiquement dans la même
entrée structurelle, avec remplacement des références. Dans le même projet,
les dépendances ne sont pas insérées. La révélation du classeur réutilise
`SetWindowMeta`, donc son delta et son annulation.

## Calculs d'analyse (#181)

`AddCalculation`, `UpdateCalculation` et `RemoveCalculation` sont des commandes
**structurelles** (instantané de l'arbre) : un calcul crée, réécrit ou supprime des
colonnes de sortie en même temps que son enregistrement, et l'annulation doit tout
rendre d'un bloc — une entrée d'historique par geste, comme une macro, sans en
ouvrir une. Libellés : `HistoryLabel::AddCalculation(kind)`,
`UpdateCalculation(kind)`, `RemoveCalculation { kind, with_columns }` (la famille
d'opération est une donnée, `OperationKind`).

- `RemoveCalculation { with_columns: false }` « détache » : les colonnes restent,
  ordinaires et de nouveau éditables.
- Une colonne produite par un calcul est **en lecture seule** pour les commandes de
  cellules, de type et de suppression de cellules (`CoreError::CalculatedColumnReadOnly`,
  `check_writable_cells` et `check_not_calculated`). Seuls restent permis le nom, l'unité
  et les autres métadonnées (hors formule) ; un recalcul dont la sortie refuserait quand
  même le résultat marque le calcul `Failed(OutputUnwritable)`.
- Le recalcul automatique (`Project::refresh_calculation`, côté session) ne passe pas
  par l'historique : c'est une mise à jour dérivée, comme le recalcul des formules.

## Retirer une formule (#236)

Vider le champ Formule d'une colonne **supprime aussi ses valeurs** : une
macro `MacroKind::RemoveFormula` (`SetColumnMeta` + `DeleteCells` de toute la
colonne), une seule entrée d'historique, que Ctrl+Z rend d'un coup. Toutes les
voies (saisie vidée, Retour arrière, coller, plusieurs colonnes) passent par
`AppState::set_meta_dropping_formula_data`. Le clic droit offre « Remove
Formula, Keep Data » (`Action::RemoveFormulaKeepData`), même macro sans
suppression des valeurs.

## Remplacement d'une correction (#212)

`ReplaceAnalysisData { draft, expected_versions }` remplace en place les Y
d'une ligne de base ou d'une correction par référence, et ajoute une trace
immuable `CorrectionRecord`. Un instantané structurel garde valeurs et trace
ensemble : un seul Annuler retire le geste, Rétablir le restaure. Aucun lien de
calcul n'est conservé ; les colonnes restent éditables et leurs graphiques
suivent leurs nouvelles valeurs. Les versions de toutes les entrées lues
sont exigées avant écriture. Une colonne à formule ou calculée est refusée.

`RestoreOriginalData { trace }` (#248, structurelle) rend aux colonnes
remplacées leur original, gardé dans le classeur caché des copies : il écrase
les saisies faites depuis, et un Annuler le défait. Les copies (référence
figée d'une correction, originaux) vivent dans **un seul classeur caché par
projet** (`stokesforge-core/src/analysis_store.rs`) ; une copie est retirée avec
la dernière analyse ou trace qui la cite. Toutes ces écritures sont dans la
commande qui les provoque : annuler ramène l'arbre, copies comprises.

La longueur de Y reste identique. Les lignes dont X ou Y est non fini gardent
leur cellule originale ; un NaN produit sur une ligne valide (division par zéro,
référence hors plage) reste visible. Plusieurs séries partageant Y doivent
produire des valeurs identiques : sinon rien n'est écrit. Chaque remplacement
garde sa propre trace, y compris après une saisie ordinaire. Supprimer toutes
ses sources retire la trace ; annuler leur suppression la restaure.
