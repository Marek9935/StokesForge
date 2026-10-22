# Session — l'acteur et la concurrence

`stokesforge-session` rend concrète la décision qui tient tout le socle : **le
`Project` vit sur son propre thread**.

L'acteur détient le `&mut Project` **de la session** : un client n'a qu'un
`Arc<Tree>` figé et ne peut muter que par `SessionHandle::execute(Command)`.
« Aucune mutation hors d'une commande » est donc une propriété de propriété
**pour les clients d'une session**, pas une garantie du compilateur pour tout le
code : les primitives publiques de `stokesforge-core` servent aussi à *construire* un
projet avant `Session::spawn`, aux tests, et au CLI (qui manipule un `Project`
avec un `History`, sans session). Hors historique **par choix** : le recalcul des
colonnes-formule (`Project::set_length`/`set_values` dans l'acteur, voir
ci-dessous), la géométrie et l'état des fenêtres, le cadrage de vue.

## Le protocole

Un canal de requêtes **FIFO** (`std::sync::mpsc`, ordre déterministe). Chaque
requête reçoit **une** réponse, sur un canal à usage unique créé par le client
pour cette requête.

L'UI et le fil de script sont deux clients **symétriques** : ils envoient des
requêtes et reçoivent des instantanés immuables. L'UI garde le dernier
`Snapshot` reçu et peint depuis lui **sans verrou** ; elle sait que son
instantané est périmé en comparant la `revision` — un compteur monotone —
jamais en comparant les arbres entre eux.

```rust
Session::spawn(project, history_limit) -> Session   // démarre le fil
session.handle()                       -> SessionHandle  // clonable, à donner aux clients
session.shutdown()                     -> Project        // rend le projet
```

`SessionHandle` : `execute`, `undo`, `redo`, `begin_macro`, `end_macro`,
`abort_macro`, `snapshot`, `save`, `load`, `new_project`.

## Ce qu'un instantané porte

```rust
Snapshot { tree: Arc<Tree>, revision, history: HistoryState, epoch }
```

Le cloner est bon marché : deux `Arc` et une poignée de champs. **L'arbre n'est
jamais recopié, seulement partagé.**

L'instantané porte aussi, hors de l'arbre, `window_geometries` et
`column_widths` (#325, largeur des colonnes de feuille par `NodeId`), avec
`geometry_revision`, compteur commun de ces deux tables : une pose de largeur
n'avance ni `revision` ni l'historique. Seule la table des géométries est purgée
à la mutation ; celle des largeurs **ne l'est pas** (Ctrl+Z rend la colonne
supprimée avec sa largeur, invariant 7), la sauvegarde filtre les clés mortes.
`Load` et `NewProject` remplacent ou vident les deux.
`Request::ResetColumnWidths { columns }` (#330, `SessionHandle::reset_column_widths`)
**retire les clés** des colonnes (le défaut est une constante de l'application,
la session n'écrit rien) ; `geometry_revision` n'avance que si une clé a
vraiment été retirée ; ni `revision` ni historique.

`HistoryState` donne à l'UI ce qu'il lui faut pour ses menus — `can_undo`,
`can_redo`, les deux libellés, `in_macro` — sans exposer la pile.

`epoch` est distinct de `revision` et répond à une question que `revision` ne
peut pas trancher : **est-ce encore le même projet ?** C'est un compteur local
à l'acteur, jamais persisté, incrémenté quand un `Load` ou `NewProject` réussit.
Les deux incrémentent **aussi** `revision` : elle reste monotone sur toute la vie de
la session. Ce qu'elle ne dit pas, c'est que le projet a été remplacé ; `epoch`,
lui, a changé. `stokesforge-script` s'en
sert pour détecter qu'une poignée désigne un `NodeId` d'un projet qui n'est plus
celui qui l'a produite — avant même de chercher ce `NodeId` dans l'arbre, où il
pourrait, par coïncidence, désigner un autre objet.

## `save`, `load` et `new_project`

`save` et `load` passent par `stokesforge-persistence`
([`persistance.md`](persistance.md)).

- `save` écrit le projet courant **sans toucher** à la révision ni à
  l'historique ;
- `load` **remplace** le projet courant et **réinitialise l'historique**
  (`undo`/`redo` redeviennent vides). Un ancien instantané n'a alors plus aucun
  rapport avec l'arbre courant, y compris ses identifiants de nœud, qui
  repartent de la valeur persistée dans le fichier.

`new_project` remplace le projet par `Project1` vide, sans entrée annulable.
Comme `load`, il vide l'historique et les géométries, et avance révision,
époque et révision de géométrie. La confirmation appartient à l'application,
pas à l'acteur.

Les trois sont refusés pendant une macro ouverte. En cas d'échec, ni le projet
ni l'historique ne sont modifiés.

## Une macro ouverte, et l'identité de l'émetteur

Chaque `SessionHandle` porte une identité stable (`ClientId`, privée au crate).
Deux `.clone()` d'une même poignée restent **le même** émetteur — un script qui
clone sa poignée pour un thread reste « lui-même » ; deux appels
**indépendants** à `Session::handle()` sont deux émetteurs **distincts**, même
dans le même processus. `SessionHandle::with_new_identity()` donne une poignée
vers la même session avec une identité **neuve** : c'est ce qu'emploient les
vues détachées (`AppState::fork_for_detached_view`, dans `stokesforge-app-state`) et le moteur de script, pour
que leurs écritures soient refusées (`ForeignMacro`) pendant une macro ouverte
par l'interface — au lieu d'y être absorbées puis annulées avec elle (issue
#21).

Cette identité existe pour une raison précise : sans elle, le travail d'un
client s'infiltrait dans la macro d'un autre, et un `abort_macro` pouvait
défaire silencieusement le travail déjà validé de quelqu'un d'autre.

| Requête | Pendant une macro ouverte |
|---|---|
| `Execute` | **refusée** (`ForeignMacro`) si l'émetteur n'est pas le propriétaire ; absorbée dans la macro sinon |
| `Undo` / `Redo` | refusées pour **quiconque** (`MacroOpen`) — annuler pendant une macro, la sienne ou celle d'un autre, laisserait un état incohérent |
| `BeginMacro` | **refusée** si une macro est déjà ouverte par un *autre* émetteur ; un même émetteur peut imbriquer la sienne |
| `EndMacro` | **refusée** si l'émetteur n'est pas le propriétaire (`NotInMacro` reste réservé au cas où aucune macro n'est ouverte) |
| `AbortMacro` | **refusée** si l'émetteur n'est pas le propriétaire |
| `Save` / `Load` / `NewProject` | refusées pour quiconque — écrire l'état intermédiaire d'une macro que quelqu'un pourrait encore avorter reste interdit à tous |
| `Snapshot` | toujours servie : lecture pure |
| `Shutdown` | annule (plutôt que de commettre) toute macro encore ouverte, niveau par niveau |

Le dernier point mérite son détail : une macro n'empêche jamais `Project` d'être
muté — elle ne fait que **différer l'entrée d'historique**. Sans garde, un
`Shutdown` rendait le projet avec ces mutations **commises en silence**, sans
qu'elles soient jamais devenues une entrée annulable. Une macro ouverte à
l'arrêt est donc désormais annulée.

La table complète, avec ce qu'elle corrige et pourquoi, est en tête de
`stokesforge-session/src/lib.rs`.

## Deux invariants tenus par construction

1. **L'acteur ne touche jamais à Python.** Ce crate ne dépend d'aucun crate
   Python, et `tests/no_ui_dependency.rs` force une décision consciente à toute
   nouvelle dépendance. C'est le troisième pilier de l'invariant 9 : sans lui,
   le blocage mutuel entre GIL et acteur serait structurel.
2. **Aucun objet non-`Send` ne traverse le canal.** Requêtes et réponses sont
   du Rust pur, `Send + 'static` — vérifié par une assertion de compilation
   (`assert_send`).

## Le principe qui gouverne les refus

Tout ce qu'un client sait du projet est une copie **potentiellement périmée**.
Donc : toute hypothèse doit voyager dans la requête et être vérifiée par
l'acteur **au moment de l'appliquer**, jamais avant.

C'est le même principe que `Command::SetCells { expected_version }`
([`commandes-et-historique.md`](commandes-et-historique.md)) : ne rien vérifier
côté client, tout vérifier côté acteur.

## Le recalcul des colonnes-formule (#193)

Après chaque `Execute`, `Undo`, `Redo`, `AbortMacro` réussi et après chaque
`Load`, l'acteur passe sur les colonnes-formule (`src/recalc.rs`). Il ne
**réécrit** que celles dont une donnée du calcul a changé depuis leur dernier
calcul : il garde pour chacune une *empreinte* — la formule stockée (texte et
identifiants de colonnes, #321), la version de chaque colonne lue de la feuille
(ou son absence : supprimée, ou dans une autre feuille), longueur du résultat,
version de la colonne elle-même, issue (calculée, illisible, prise dans un
cycle) — et la compare à l'état courant. Une
commande sans rapport (une couleur de courbe, un dossier) ne réécrit donc
aucune colonne ; modifier A ne réécrit que ce qui lit A, puis, par la
version de ce qui vient d'être réécrit, ce qui le lit à son tour.

**Longueur du résultat (#194).** Une colonne-formule a la longueur de la
plus longue des colonnes qu'elle lit ; au-delà de la longueur d'une colonne
plus courte, la ligne vaut `NaN`. À défaut de colonne lue — constante,
formule illisible, prise dans un cycle —, elle prend la longueur de la plus
longue colonne **ordinaire** (sans formule) de la feuille, jamais celle d'une
autre colonne-formule : une formule ne s'allonge donc ni avec une colonne
qu'elle ne lit pas, ni avec une autre formule, et redevient courte quand on
annule l'allongement de sa source.

| | |
|---|---|
| où vit le mémo | dans l'acteur, à côté du `Project` (`Recalculator`) : ni historique, ni `.stf` |
| ce qui le vide | `Load` (les valeurs d'un fichier ne sont pas crues sur parole) et `NewProject` |
| sur quoi il repose | la doctrine des versions : un numéro ne désigne jamais deux contenus d'une même colonne (type + valeurs, pas les métadonnées) |
| l'oracle | `FormulaRecalc::Full`, le recalcul complet d'avant #193, par `Session::spawn_with_recalc` — jamais en production |
| ce qui le tient | `tests/recalc_cible.rs` (un cas nommé par composante, avec témoin) et `stokesforge-robustesse` `b22` (deux sessions jumelles, mêmes arbres à chaque pas) |

### Recalcul interruptible (#239)

`SessionHandle::execute_interruptible` répond dès la commande appliquée, puis
recalcule les formules en pouvant être arrêté ; `execute` reste de bout en
bout bloquant (scripts, CLI, tests). Seul `AppState::execute` l'emploie.

| Pièce | Rôle |
|---|---|
| `gauge.rs` (`RecalcGauge`) | seul état partagé **hors du canal FIFO** : numéro du recalcul en cours, drapeau Stop, condition « fini ». Pendant un recalcul l'acteur ne lit plus ses requêtes : un Stop ou un instantané passés par le canal attendraient la fin |
| `SessionHandle::actor_alive` (#310) | le fil de l'acteur tourne-t-il encore ? Une garde `Drop` de `actor::run` appelle `RecalcGauge::abandon` à la sortie de la boucle, **panic compris** : `running` repasse à `None` (plus d'indicateur de recalcul éternel), les attentes sont réveillées, `actor_alive` devient faux. Le modèle, lui, est perdu avec le fil : l'application écrit une copie de secours du dernier instantané (`stokesforge-app-state::recovery`) et annonce la mort par un dialogue bloquant (`AlertKind::SessionLost`). La sauvegarde automatique (#317) passe aussi par l'acteur : `SessionHandle::save` vers l'emplacement de secours du processus, sans entrée d'historique ni changement de révision, refusée pendant une macro (l'application saute alors l'échéance) |
| `SessionHandle::recalc_running`, `wait_recalc`, `stop_recalc(id)`, `take_recalc_end` | lecture et écriture de la jauge, sans l'acteur, donc sans jamais bloquer |
| `Recalculator::run_interruptible` | contrôle l'arrêt entre deux feuilles, deux colonnes, deux calculs, deux tours — jamais au milieu d'une colonne. Une passe interrompue laisse un état cohérent : les colonnes écrites sont dans le mémo, les autres seront vues périmées |
| Stop | `history.undo` (comme Ctrl+Z : la commande reste rétablissable) puis un recalcul complet normal ; `RecalcEnd::Stopped`. Seulement si la commande a rangé une entrée (`History::execute_recorded`) : une commande sans effet n'en range pas, Stop ne défait alors **rien** (jamais l'action précédente) et le recalcul s'achève (`Done`). Dans une macro l'annulation est refusée : la commande reste, le recalcul s'achève (`Done`) |

Le numéro évite qu'un Stop tardif arrête le recalcul suivant. La révision
rendue par `execute_interruptible` est celle d'avant recalcul ; **la fin du
recalcul en ajoute une** (les formules changent sous la même révision, et un
cache de rendu indexé par révision resterait périmé). Les autres requêtes
(`undo`, `save`, `snapshot`…) attendent toujours la fin du recalcul : côté
interface, `AppState::refresh` saute l'instantané tant qu'un recalcul court.
Seul `Execute` est interruptible ; `Undo`, `Redo`, `Load` recalculent d'un bloc.

La règle à ne pas casser : **toute donnée qui entre dans le calcul d'une
formule entre dans son empreinte**. Un calcul qui lirait une donnée de plus
sans l'y ajouter afficherait une valeur fausse sans erreur. Les tests à
écrire quand on étend le mécanisme (nouvelle fonction de formule, référence
entre feuilles #186, analyses recalculées #184) :
[`tests.md`](tests.md), « Recalcul ciblé ».

## Ce qu'il faut savoir avant de modifier

- Ajouter une requête, c'est décider sa ligne dans la table ci-dessus. Une
  requête mutante qui n'y figure pas est un trou.
- `tests/session.rs` porte les cas nommés ; côté robustesse, `b6`
  (concurrence entre clients), `b18` (ce qui reste possible pendant qu'une
  macro est ouverte) et `b22` (recalcul ciblé contre recalcul complet).
- Une requête qui remplace le projet doit vider le mémo du recalcul
  (`Recalculator::forget`), comme `Load` et `NewProject`.

**À lire** : `stokesforge-session/src/lib.rs` (tête, et sa table), `src/actor.rs`,
`src/recalc.rs`, `src/gauge.rs`, `src/snapshot.rs`.
