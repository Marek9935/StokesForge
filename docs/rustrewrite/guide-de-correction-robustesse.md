# Guide de correction — anomalies de robustesse du 2026-09-11

**Ce document n'est pas un rapport.** Il n'ajoute aucune anomalie, ne rouvre
aucune décision d'architecture, et ne remplace ni
`rapport-robustesse-2026-09-11.md` (A1→C2) ni
`rapport-robustesse-2026-09-11-suite.md` (A7→D3). Les deux rapports restent
les **compte rendus datés** ; celui-ci est leur **plan de travail** : il dit
*où* aller, *dans quel ordre*, *ce qu'il y a à décider avant de coder*, et
*comment savoir qu'on a fini*.

Trente anomalies, dix-huit fichiers, quarante-six reproductions exécutables.

**Ce qui a changé dans cette révision.** Les deux rapports rangent les
anomalies par gravité, parce que c'est ainsi qu'on rend compte. Ce guide les
rangeait par fichier et par coût, parce que c'est ainsi qu'on travaille. Ni
l'un ni l'autre ne montrait ce qui compte le plus pour décider de l'ordre :
**la plupart de ces trente anomalies ne sont pas trente problèmes, mais neuf
questions posées trente fois**. Une relecture croisée des deux rapports a
dégagé neuf familles de cause ; elles sont désormais la colonne vertébrale du
guide (§2), et l'ordre de travail (§5) en découle au lieu de le contredire.

Aucune anomalie n'a été ajoutée, retirée ni requalifiée. Les corrections de ce
guide lui-même sont listées en §7.4.

---

## 1. Comment on valide un correctif

Le banc (`crates/plotyx-robustesse`) porte deux natures de test, et c'est tout
le protocole :

```text
1.  l'anomalie existe          → sa reproduction est `#[ignore]` et ÉCHOUE
2.  on corrige
3.  la reproduction PASSE      → le correctif est bon
4.  on retire son `#[ignore]`  → elle devient un garde-fou, et entre dans
                                 `cargo test --workspace` comme les autres
```

**Une reproduction qui passe est le seul critère de fin.** Elle n'échoue pas
sur un message vague : chacune porte, dans son assertion, l'entrée exacte,
l'état observé et la raison pour laquelle il est faux.

```sh
# avant de corriger : voir l'anomalie
cargo test -p plotyx-robustesse <nom_du_test> -- --ignored --nocapture

# après : la même commande doit passer, puis
cargo test --workspace            # rien d'autre ne doit avoir bougé
```

**Ne jamais supprimer une reproduction.** Si un correctif rend une anomalie
sans objet (le code disparaît, la décision change), la reproduction devient un
garde-fou de la **nouvelle** règle : on réécrit son assertion et son
commentaire, on ne l'efface pas.

**Les deux sondes sont hors du banc pour une raison.** `plotyx-sonde-alloc` et
`plotyx-sonde-pile` éprouvent des cas qui se terminent par un `abort()` — échec
d'allocation, débordement de pile — que ni `catch_unwind` ni `join` ne
rattrapent. Elles n'ont pas d'assertion : on les lance à la main, on lit le
code de sortie.

---

## 2. Neuf familles — ce qui ne doit être décidé qu'une fois

Chaque famille réunit des anomalies qui **partagent une cause, une question ou
un artefact**, et qui coûtent plus cher traitées séparément qu'ensemble. Le
critère d'admission est strict : il ne suffit pas que deux anomalies se
ressemblent, il faut que les traiter à part fasse écrire deux fois le même
raisonnement, ou fasse répondre deux fois — et peut-être différemment — à la
même question.

Les familles **se recoupent**, et c'est une information : une anomalie qui
appartient à deux familles est une charnière, et son ordre de traitement n'est
pas indifférent. Les recoupements sont notés (½) et commentés.

---

### F1 — La valeur manquante : le modèle en a une, les frontières ne s'en servent pas

> **L'invariant.** Le modèle possède exactement une valeur manquante — `NaN`
> pour les colonnes numériques, `None` pour le texte — et sa documentation de
> tête (`column.rs`) la définit. Toute frontière qui reçoit une valeur non
> représentable doit soit la traduire dans celle-là, soit la refuser. Aucune
> troisième voie.

| Id | Ce que la frontière fait de la valeur non représentable | Nature |
|---|---|---|
| A1 | un `np.ma.masked_array` : les valeurs masquées sont écrites **comme des données** | décision |
| A9 | `Some("")` (chaîne vide présente) est relu comme `None` (absent) : la distinction est **détruite** | décision ½ |
| A10 | une colonne d'erreur trop courte **tronque le signal** au lieu de se compléter par `NaN` | décision |
| C2 | `"1e400"` devient `+inf` : ni une valeur, ni la valeur manquante | décision |

**Ce qui se décide une fois.** *Quand une valeur d'entrée n'est pas
représentable telle quelle, devient-elle la valeur manquante du modèle, ou
est-elle refusée ?* Une seule réponse, appliquée aux quatre. Le dépôt a déjà
tranché ce type de question une fois, et dans le sens « refuser » : le module
Python refuse `int64` « plutôt que de convertir en silence — une conversion
implicite cacherait une perte de précision au-delà de 2⁵³ ». Le rapport du
matin note que ce raisonnement s'applique **mot pour mot** au masque.

**Ce que coûte de les séparer.** Le guide listait A1, A9, A10 et C2 comme
quatre questions indépendantes de §3.2. Quatre questions posées à quatre
moments différents, ce sont quatre paragraphes de documentation et un risque
réel d'**incohérence** : répondre « refuser » pour le masque numpy et
« convertir » pour le texte hors bornes est parfaitement possible si personne
ne les voit côte à côte — et le résultat est un modèle dont la règle d'entrée
dépend de la porte empruntée.

**Ce qu'on ne voit qu'ensemble.** A10 est la seule des quatre où la valeur
manquante est *disponible et non utilisée* : le correctif proposé (les
barres d'erreur se complètent par `NaN` au lieu de raccourcir `x`/`y`) ne
demande aucune convention nouvelle, seulement d'employer celle qui existe.
C'est donc la plus facile des quatre, alors que sa sévérité la place au milieu.

---

### F2 — Les estampilles : quatre compteurs, quatre emplacements, quatre règles

> **L'invariant.** Un nombre qui répond à « ceci a-t-il changé ? » ou à « est-ce
> toujours le même objet ? » doit être monotone, ne jamais être restauré, ne
> jamais être dupliqué — et, selon ce qu'il garantit, traverser le disque ou
> justement ne pas le traverser.

| Id | L'estampille en cause | Ce qui est cassé | Nature |
|---|---|---|---|
| B1 | `IdAllocator` (projet, **hors de l'arbre**, persisté en `next_id_hint`) | déborde sur `u64::MAX`, puis **recule** | méc. ½ + décision ½ |
| B9 | `IdAllocator` | `#[derive(Clone)]` sur `Project` la **duplique** : deux projets, mêmes identifiants | méc. |
| A7 | `Column::version` (par colonne, **dans l'arbre**, persisté par nœud) | `replace_tree` la **restaure** : deux contenus sous un même numéro | chantier |
| A3 | `Revision` (session, dans l'acteur, non persistée) | ne dit pas qu'une **identité** a changé : une poignée survit à un `load` | chantier |
| B8 | *(aucune)* | rien dans le fichier ne dit qu'il a changé depuis qu'on l'a lu | chantier |

**Le dépôt sait déjà faire.** La documentation de tête de `project.rs` écrit le
patron en toutes lettres : l'allocateur vit **hors de l'arbre** « pour qu'une
annulation qui restaure un ancien arbre (`replace_tree`) ne fasse pas reculer
le compteur ». `Column::version` a fait le choix inverse — elle vit *dans*
l'arbre — sans que la conséquence soit examinée. A7 est littéralement B1 vu
dans l'autre sens : là le compteur recule par débordement, ici par
restauration.

**Ce qui se décide une fois.** La **doctrine des estampilles** : combien il y
en a, où chacune vit, et laquelle traverse le disque. Trois lignes à écrire
dans le `cahier-des-charges-technique.md`, et cinq anomalies suivent.

| Estampille | Portée | Persistée ? | Ce qu'elle garantit |
|---|---|---|---|
| identifiants | projet | oui (`next_id_hint`) | un `NodeId` n'est jamais réutilisé |
| version de contenu (A7) | colonne, **attribuée** par le projet | **oui** — sans quoi un lecteur retrouve d'anciens numéros après un rechargement | une version ne désigne jamais deux contenus |
| compteur de sauvegarde (B8) | fichier | **oui**, par construction | le fichier n'est plus celui qu'on a chargé |
| époque de projet (A3) | session | **non** — voir ci-dessous | ce `NodeId` vient d'un autre projet |

**Ce qu'on ne voit qu'ensemble.** Trois des cinq demandent d'ajouter un `u64`,
et le réflexe naturel est de les traiter pareil — or **l'époque d'A3 est la
seule qui ne doit surtout pas être persistée**. Une époque écrite dans le
fichier revient égale quand on rouvre le même fichier, ou pire, quand on ouvre
deux fichiers de même forme — c'est-à-dire exactement le cas qu'A3 doit
attraper. Traitées à part, et sur le modèle des deux autres, l'époque serait
persistée « par cohérence » et A3 resterait ouverte sous un correctif qui a
l'air d'un correctif. C'est le genre de faute qu'on ne commet pas quand les
trois tiennent dans le même tableau.

Second effet du regroupement : A7 et B8 répondent à la **même question à deux
échelles** (« cela a-t-il changé depuis que j'ai regardé ? », pour une colonne
et pour un fichier). Il vaut la peine de vérifier, avant d'écrire deux
compteurs, si une estampille globale de projet ne rend pas les deux services —
son plus haut numéro émis, écrit dans l'en-tête, est déjà un compteur de
sauvegarde.

---

### F3 — Ce que le lecteur accepte du disque sans le borner

> **L'invariant.** Tout scalaire lu d'un fichier est validé contre une borne
> nommée ou contre un ensemble connu ; une valeur hors borne produit une erreur
> **dédiée**, qui dit ce qu'il faut faire.

| Id | Ce qui entre sans contrôle | Où | Nature |
|---|---|---|---|
| B1 ½ | identifiants de nœud et `next_id_hint`, jusqu'à 2⁶⁴ | `lib.rs` · `load` | décision |
| B2 | `record.row as usize`, `record.column as usize` | `convert.rs:396` | méc. ½ |
| B4 ½ | `histogram_bins`, borné par le bas seulement | `convert.rs` · `record_to_series_spec` | décision |
| C1 | le `format_version` d'un `PlotSpec`, jamais comparé | `convert.rs:425` | décision |
| C5 | une variante d'enum inconnue perd **tout** le fichier, sous le mot « corrompu » | `schema.rs` (7 enums) + `lib.rs` | méc. ½ + chantier ½ |
| — | un en-tête de version `0` — un numéro qui n'a jamais existé — est lu comme du v1 | `lib.rs` | *(sans identifiant, §7.3)* |

**Le dépôt sait déjà faire, une fois.** `ConvertError::ZeroHistogramBins` est
le seul endroit du lecteur qui refuse une valeur absurde avec une erreur
nommée. Les deux rapports le citent **cinq fois** comme le modèle à suivre —
cinq fois, pour cinq anomalies, dans le même fichier.

**Ce qui se décide une fois.** Un seul passage sur `convert.rs` et `lib.rs`,
une seule extension de `ConvertError`, une seule mise à jour de
`format-fichier-plx.md`. La suite du rapport le nomme déjà — « B4 rejoint B1,
B2 et C1 : quatre anomalies sur ce que le lecteur accepte sans le borner » —
mais dans une ligne de « Voir aussi », c'est-à-dire à l'endroit où une
observation meurt.

**Ce que coûte de les séparer.** L'ancien ordre plaçait B2, C5(niveau 1) et
B1(½) en vague 1 et B4(½), C1 en vague 4 : le même fichier de deux cents lignes
ouvert trois jours différents, `ConvertError` étendu trois fois, sa
documentation réécrite trois fois. Et le piège déjà consigné pour B2 — « se
corrige aux deux bouts le même jour » — dit exactement cela, pour une anomalie
sur cinq.

**Ce qu'on ne voit qu'ensemble.** **C1 et C5 se contrarient.** C1 resserre :
un `PlotSpec` de version supérieure est refusé. C5(niveau 2) relâche : un
fichier d'une version future s'ouvre en perdant du style plutôt que des
mesures. Écrites dans cet ordre, la première rend la seconde inatteignable — le
spec est refusé avant que la dégradation gracieuse ne puisse jouer. La règle du
numéro de version (**un champ n'oblige pas à le bouger, une variante
l'oblige**) doit être écrite avant l'une comme avant l'autre.

---

### F4 — Les plafonds, et l'`abort()` qu'ils ferment

> **L'invariant, déjà écrit trois fois dans les deux rapports.** « Une limite
> arbitraire mais documentée vaut infiniment mieux qu'un `abort()` » : elle
> laisse une boîte de dialogue et un projet intact.

| Id | Ce qui n'est borné nulle part | Comment cela finit | Nature |
|---|---|---|---|
| B3 | `start` / `count` d'une commande de lignes | échec d'allocation → `abort()` | décision |
| B4 | `histogram_bins` (jusqu'à 2³²) | 68 Gio demandés → `abort()` | décision ½ |
| B7 | la profondeur d'imbrication des macros | débordement de pile à 10 891 → `abort()` | chantier |
| C7 ½ | la longueur d'un nom (et de `unit`, `long_name`, `comment`, `display_format`, du texte d'une note) | rien ne casse — le nom de cent mille caractères traverse le disque | décision ½ |
| B1 ½ | les identifiants à la lecture | voir F3 | décision ½ |
| B2 ½ | `row` / `column` d'un panneau à la lecture | voir F3 | méc. ½ |

**Trois des six se terminent par un `abort()`**, par deux mécanismes différents
(allocation pour B3 et B4, pile pour B7) et une seule conséquence : ni
`catch_unwind`, ni l'acteur, ni `JoinHandle::join` ne rendent la main. Le
processus meurt avec tout le travail non sauvegardé. C'est la seule catégorie
du banc que l'application ne peut pas amortir, quoi qu'elle fasse.

**Ce qui se décide une fois.** La **table des bornes** : lignes par feuille,
classes par histogramme, profondeur de macro, longueur d'un nom, plafond
d'identifiant, coordonnées d'un panneau. Six nombres, une constante nommée
chacun, un endroit.

**Ce que coûte de les séparer.** Six nombres arbitraires choisis six jours
différents par six arguments différents, aucun endroit où un relecteur puisse
les lire ensemble — et le sixième choisi sans connaître les cinq premiers.

**Ce qu'on ne voit qu'ensemble.** B7 était rangé en « chantier » (option 2 :
rendre `revert` et `Drop` itératifs, du code délicat sur un enum récursif). Vue
depuis cette famille, son **option 1** est de la même nature exacte que B3 et
B4 : une constante nommée et un `Err`. Elle change une signature
(`begin_macro` doit rendre un `Result`), ce qui est son seul vrai coût — et
c'est le même arbitrage que celui déjà retenu pour B3. B7 n'est un chantier que
si l'on tient à lever la limite plutôt qu'à la nommer.

---

### F5 — Une commande acceptée a un effet

> **L'invariant.** Une commande qui rend `Ok` a changé quelque chose. Une
> commande qui ne change rien n'occupe **pas** un cran d'annulation.

| Id | Couche | Ce qui est accepté sans effet — ou avec le mauvais | Nature |
|---|---|---|---|
| B6 | `plotyx-app` | traverser une cellule sans rien taper écrit : une révision, un cran, un incrément de version | méc. |
| B5 | `plotyx-core` + `plotyx-commands` | une commande de lignes sans cible réussit et s'empile ; et `at == len` n'ajoute jamais de ligne | décision |
| C3 | `plotyx-core` | un bloc **vide** écrit au-delà de la fin allonge quand même la colonne | méc. |
| — | `plotyx-commands` | `Rename` vers le nom courant s'empile : « Renommer M en M » | *(sans identifiant ni reproduction, §7.3)* |

**Trois couches, une seule règle.** C'est la même phrase à trois hauteurs :
l'interface n'émet pas de commande quand rien n'a changé (B6), l'historique
n'empile pas d'entrée quand le delta n'a touché aucune colonne (B5, Rename), le
cœur ne modifie rien quand on lui demande d'écrire zéro valeur (C3).

**Ce qui se décide une fois.** *Refus, ou succès sans entrée d'historique ?*
Le rapport pose la question sous B5 seulement ; elle vaut pour les quatre. Le
moins cher (ne pas empiler) ne change pas la surface publique du bus ; le plus
net (`Err(IndexOutOfBounds)`) fait remonter un message au lieu d'un faux
succès.

**Ce que coûte de les séparer.** L'ancien ordre plaçait C3 en vague 1 comme
« mécanique » et B5/B6 en vague 2 comme « une décision » : la règle s'écrit
alors deux fois, une fois en code et une fois en prose, et le cas `Rename` —
consigné dans le corps de la section B5, présent dans aucun index, couvert par
aucune reproduction — se perd entre les deux.

**Ce qu'on ne voit qu'ensemble, et qui est un piège.** `Request::BeginMacro`
fait **délibérément** avancer la révision sans toucher au modèle, et
`actor.rs` le documente : « l'état de l'historique (`in_macro`) a changé, donc
la révision avance même sans mutation du modèle ». La règle de cette famille
porte donc sur l'**entrée d'historique**, pas sur le mouvement d'un compteur —
un correctif qui viserait « aucun compteur ne bouge sans mutation » casserait
un comportement voulu et documenté.

---

### F6 — Le protocole de session : ce qu'un client croit d'un état qu'il ne détient pas

> **L'invariant, qui est un corollaire d'une décision d'architecture.** L'acteur
> détient le seul `&mut Project` du processus. Donc tout ce qu'un client sait du
> projet est une copie qui peut déjà être périmée, et **toute attente doit
> voyager dans la requête** pour être vérifiée au moment de l'appliquer.

| Id | Ce que le client croit à tort | Ce qui manque dans la requête | Nature |
|---|---|---|---|
| A2 | « la colonne fait toujours 10 lignes » | `expected_version` sur `SetCells`, vérifié contre `Column::version` | chantier ½ (F2) |
| A3 ½ | « ma poignée désigne toujours le même nœud » | l'époque de projet (voir F2) | chantier |
| A4 | « ma révision validée tient » | l'identité de l'émetteur | chantier |
| A11 | « sauvegarder pendant une macro est sans risque » | une garde `in_macro`, comme `Load` en a une | méc. |
| C6 | « Annuler est possible » — l'instantané le dit, l'acteur le refuse | rien : `in_macro` est déjà transporté, et non lu | méc. |

**Les deux rapports disent eux-mêmes que ce sont des familles, pas des cas.**
Sous A2 : « Le même motif — lire un instantané, décider, puis envoyer — est
disponible à tout client de `SessionHandle`. Le pont est le premier à
l'utiliser, il ne sera pas le dernier. » Sous A11 : « La question n'est donc pas
"faut-il garder `Save` ?" mais "lesquelles des huit requêtes une macro ouverte
doit-elle suspendre ?" ». Deux appels explicites à traiter le motif ; le guide
les rangeait en vague 1 (A11, C6) et en vague 5 (A2, A3, A4).

**Ce qui se décide une fois.** Une **table en tête de `plotyx-session`** : pour
chacune des huit requêtes du protocole, ce qu'une macro ouverte en fait, et ce
que l'identité de l'émetteur y change. Aujourd'hui une seule requête sur huit
consulte `in_macro()`.

**Ce qu'on ne voit qu'ensemble.** Les trois lignes d'A11 (recopier la garde de
`load_project` sur `Request::Save`) sont **un bon correctif et une mauvaise
conclusion** : elles font passer la reproduction, et referment la question pour
quiconque lit le journal des correctifs. Elles doivent être posées comme
*palliatif déclaré*, avec la table à écrire encore ouverte — sinon A4 rouvrira
dans six mois un débat que la vague 1 aura donné l'impression d'avoir clos. Même
remarque pour `Session::shutdown`, qui commet en silence les mutations d'une
macro ouverte : c'est la même absence de garde, à la sortie, et elle n'a pas de
reproduction (§7.3).

**A2 — décidé : vérifier la version, pas la longueur, et retenter plutôt que
refuser sec.** La piste initiale (`expected_len`) ne détecte qu'un changement
de *longueur* ; une écriture concurrente qui laisse la longueur intacte mais
change des valeurs la traverse sans bruit. Le correctif retenu compare plutôt
`Column::version` — qui change à **toute** modification de la colonne, quelle
qu'elle soit — entre l'instant de l'instantané et l'instant où l'acteur
applique la commande :

1. `SetCells` porte un `expected_version: Option<u64>` ; l'acteur vérifie qu'il
   coïncide avec `Column::version` **au moment d'appliquer**, pas au moment de
   la lecture — c'est le même déplacement de la vérification qu'exigeait déjà
   `expected_len`, seul le champ comparé change.
2. Si la version a bougé, l'acteur refuse ; le pont (`plotyx-script/bridge.rs`)
   relit un instantané frais, **recalcule**, et retente — jusqu'à **cinq**
   tentatives.
3. Au sixième échec, `pyx.set_column` lève une exception dédiée
   (`ConcurrentWriteError`) plutôt que de retenter indéfiniment — un script
   qui se heurte en boucle à un autre écrivain doit recevoir une erreur
   nommée, pas un blocage. Documenté dans
   `cahier-des-charges-api-script.md` (§4 et §7), pour que les auteurs de
   script la rencontrent dans le contrat avant de la rencontrer en
   production.

**La dépendance que cela crée envers F2 est stricte, pas une commodité
d'ordonnancement.** Comparer une version n'a de sens que si une version ne
désigne jamais deux contenus différents — c'est exactement ce qu'A7 casse
aujourd'hui (§F2). Construire ce correctif avant qu'A7 ne soit fermée
donnerait à un script une fausse assurance : une version qui coïncide par
coïncidence après une annulation ferait passer une écriture qui aurait dû être
retentée. L'ordre du §5 (F2 avant F6, vague 6) n'est donc plus seulement une
économie de raisonnement pour A3 : il est désormais une précondition de
correction pour A2.

---

### F7 — Le coût recalculé intégralement à chaque appel

> **L'invariant.** Le coût d'un geste courant ne croît pas avec la taille du
> projet ni avec celle de l'historique.

| Id | Ce qui est refait à chaque appel | Mesure | Nature |
|---|---|---|---|
| D1 | chaque candidat de nom comparé à tous les noms pris — O(n³) pour la feuille | 4 000 colonnes : 17,8 s contre 191 ms | méc. |
| D2 | `find_window` construit la liste de **tous** les nœuds du projet | ~32 octets par nœud, à chaque résolution de chemin | méc. ½ + décision ½ |
| D3 | l'union des empreintes de tout l'historique, plus un parcours de l'arbre | 5 000 colonnes : 8,2 ms **par frappe** | méc. |

**Une cause commune pour deux des trois** : `walk_all()` / `windows()` appelés
dans une boucle. `next_window_name` est littéralement D1 et D2 à la fois —
elle appelle `tree.windows()` à **chaque** candidat de nom.

**Ce qui se décide une fois.** Une seule chose, et elle est chiffrée : *quelle
taille de projet la résolution de chemin doit-elle viser ?* En dessous de
quelques milliers de nœuds, l'option 2 de D2 (itérer au lieu de collecter,
quelques lignes, aucun invariant nouveau) suffit ; au-delà, seul l'index
`nom de fenêtre → NodeId` change la classe de complexité, au prix de quatre
points de maintenance. C'est une décision de produit.

**Pourquoi cette famille passe en premier (§5).** C'est la seule des neuf qui
ne demande **aucun arbitrage sémantique**, et la seule dont les reproductions
comptent des **octets** plutôt que des millisecondes : elles sont
déterministes, donc elles deviennent des garde-fous de CI le jour même du
correctif. Le gain se lit dans le test, pas dans un chronomètre.

---

### F8 — La frontière valeur ↔ texte : une seule table, deux directions

> **L'invariant.** Une valeur du modèle a **une** forme écrite, et cette forme
> est produite et relue au même endroit, quel que soit l'appelant.

| Id | Direction | Ce qui manque | Nature |
|---|---|---|---|
| C4 | valeur → texte | pas de notation scientifique : 30 caractères pour 1,6·10⁻¹⁹, et ×7,6 de mémoire à la conversion | décision |
| C2 ½ | texte → valeur | `"1e400"` devient `inf` ; est-ce une valeur ou une valeur manquante ? (voir F1) | décision ½ |
| A9 ½ | valeur → texte | rien ne distingue à l'écran une chaîne vide présente d'une valeur absente | décision ½ |

**Deux appelants, aucun lien.** `AppState::cell_text` (affichage) et
`Column::set_column_type` (conversion `Float → Text`) formatent tous deux un
`f64` par son `Display`, qui en Rust n'émet jamais d'exposant. Ce sont deux
sites, une seule règle absente. `cell_text` annonce d'ailleurs déjà où la règle
devra vivre : « le vrai formatage appartiendra à `ColumnMeta::display_format`,
que cette phase ne lit pas ».

**Ce que coûte de les séparer.** L'ancien ordre plaçait C4 en vague 2 et C2 en
vague 4 : la règle d'écriture fixée avant la règle de lecture, à deux endroits,
sans rien qui les oblige à s'accorder. Or c'est précisément le défaut que C4
décrit — « sans quoi l'affichage et la conversion diront deux choses
différentes de la même valeur ».

**À savoir, et c'est rassurant.** Ce n'est pas une perte de précision :
l'aller-retour `f64 → texte → f64` a été vérifié bit à bit dans les deux
rapports (`-0.0`, dénormaux, `f64::MAX`, `0.1 + 0.2`). C'est un choix de forme
qui n'a jamais été fait explicitement.

---

### F9 — Les noms : une règle resserrée rend illisibles des fichiers déjà écrits

> **L'invariant.** Ce qu'un nom a le droit d'être — ses caractères, sa longueur,
> sa portée d'unicité — est dit **une fois**, et le chargeur refuse exactement
> ce que l'API de mutation refuse.

| Id | La règle absente ou contredite | Nature |
|---|---|---|
| A8 | la **portée d'unicité** : trois endroits du dépôt y répondent différemment (`check_name_available`, `validate_loaded_names`, `resolve_parsed`) | chantier |
| C7 | les **caractères** (`\0`, `\n`, `U+202E`, `U+200B`) et la **longueur** (aucune) | décision |

**Le même effet de bord, et c'est ce qui les réunit.** Les deux correctifs
**resserrent** : ils rendront illisibles des fichiers que le lecteur accepte
aujourd'hui — un dossier et une note homonymes pour A8, un nom de cent mille
caractères ou porteur d'un octet nul pour C7. Les deux demandent donc la même
chose, et il serait absurde de l'inventer deux fois : **que fait-on des
fichiers déjà écrits ?** Migration au chargement, ou refus avec un message qui
nomme le fautif.

**Ce qui se décide une fois.** La règle complète des noms (portée + caractères
+ longueur), puis **un seul mécanisme de rattrapage** pour les fichiers
antérieurs. Le rapport ne demande la migration que sous A8 ; C7 en a exactement
le même besoin, et personne ne l'a écrit.

**Le piège, déjà consigné.** Aligner `validate_loaded_names` sur
`check_name_available` en **relâchant** ce dernier ferait disparaître la
reproduction sans corriger l'anomalie : c'est la résolution de chemin qui
deviendrait ambiguë. Le sens du correctif est de refuser plus.

---

### Une échéance, pas une cause : le réveil du rendu

Cinq anomalies ne partagent pas un correctif mais une **date limite**. Elles
portent toutes sur du code que rien n'appelle encore, et toutes sur le chemin
que le chantier de rendu (`docs/newarchitecture/roadmap-rendu.md`) empruntera :

| Id | Ce que le rendu appellera | Ce qu'il obtiendra aujourd'hui |
|---|---|---|
| A5 | `resolve::histogram` sur une étendue qui déborde `f64` | bornes `NaN`, tous les points dans la première classe |
| A6 | `resolve::bar_width` | la largeur demandée, fût-elle négative, nulle, `NaN` ou infinie |
| A10 ½ | `resolve_series` avec une colonne d'erreur plus courte | une courbe de 1 000 points tracée avec 3, et `ok() = true` |
| B2 ½ | `PlotSpec::rows()` / `columns()` | une panique en debug, une grille de zéro ligne en release |
| B4 ½ | `histogram_bins` venu d'un fichier | 68 Gio demandés, `abort()` |

Le rapport du matin signale la latence pour A5, A6 et B2 ; la suite pour A10 et
B4 ; ni l'un ni l'autre ne dit que ce sont les mêmes cinq. Leur règle
d'ordonnancement n'est pas « les corriger ensemble » — ce sont cinq correctifs
indépendants — mais **aucune des cinq ne doit être encore ouverte quand le
chantier de rendu commence**. Elles coûtent trois lignes aujourd'hui, une
enquête le jour où un graphe sort faux.

### Les deux qui n'appartiennent à rien

**A5** et **A6** n'ont pas de famille de cause : deux fonctions de `resolve.rs`,
trois lignes chacune, indépendantes de tout le reste. C'est une information,
pas un oubli : elles peuvent être corrigées n'importe quand, par n'importe qui,
sans lire ce guide — à la seule condition de l'être avant le rendu.

### Table de correspondance

```text
A1  F1                     B1  F2 · F3 · F4        C1  F3
A2  F2 · F6                B2  F3 · F4 · rendu     C2  F1 · F8
A3  F2 · F6                B3  F4                  C3  F5
A4  F6                     B4  F3 · F4 · rendu     C4  F8
A5  — · rendu              B5  F5                  C5  F3
A6  — · rendu              B6  F5                  C6  F6
A7  F2                     B7  F4                  C7  F4 · F9
A8  F9                     B8  F2                  D1  F7
A9  F1 · F5 · F8           B9  F2                  D2  F7
A10 F1 · rendu                                     D3  F7
A11 F6
```

Les charnières — les anomalies de deux familles ou plus — sont **A2** et
**A3** (F2 donne la forme de son estampille, F6 la consomme : F2 d'abord — pour
A2 ce n'est plus une commodité mais une précondition de correction, §F6),
**A9** (F5 donne le correctif, F1 et F8 donnent la convention : F5 d'abord),
**B1**, **B2** et **B4** (le plafond se décide en F4, s'applique en F3 : la
table des bornes d'abord), et **C7** (le nombre vient de F4, la migration de
F9).

---

## 3. Les trente anomalies, par fichier

C'est la table à ouvrir quand on entre dans un fichier : elle dit tout ce qu'il
y a à y faire, d'un coup. « ½ » signale une anomalie dont le correctif se
partage entre plusieurs fichiers.

### `plotyx-core`

| Fichier · fonction | Id | Fam. | Sév. | En une phrase | Reproduction |
|---|---|---|---|---|---|
| `column.rs` · `write_block`, `set_length`, `insert_rows` | B3 | F4 | moy. | un `start`/`count` non borné dimensionne l'allocation → `abort()` | *sonde* `plotyx-sonde-alloc` |
| `column.rs` · `write_block` | C3 | F5 | faible | un bloc **vide** au-delà de la fin allonge quand même la colonne | `b12_setcells_dun_bloc_vide_ne_change_rien` |
| `column.rs` · `set_column_type` (`Text→Float`) | C2 | F1·F8 | faible | `"1e400"` devient `+inf` en silence | `b1_conversion_texte_vers_numerique_puis_retour` |
| `column.rs` · `set_column_type` (`Float→Text`) ½ | C4 | F8 | faible | pas d'exposant : ×7,6 de mémoire, 30 caractères pour 1,6·10⁻¹⁹ | `b15_convertir_en_texte_ne_multiplie_pas_la_memoire` |
| `column.rs` · doc de `Column::version` ½ | A7 | F2 | **haute** | le contrat que l'historique ne tient pas (voir `history.rs`) | `b10_le_compteur_de_version_ne_recule_jamais` |
| `id.rs:43` · `IdAllocator::next` | B1 | F2·F3·F4 | **haute** | débordement sur `u64::MAX`, puis le compteur **recule** | `b4_allocateur_sature_puis_deborde` |
| `name.rs` · `check_name` | C7 | F4·F9 | faible | accepte `\0`, `\n`, les marques de direction ; aucune borne de longueur | `b21_les_noms_de_controle_sont_refuses` |
| `project.rs:41` · `#[derive(Clone)]` | B9 | F2 | faible | deux copies attribuent les mêmes identifiants | `b21_deux_projets_nattribuent_jamais_le_meme_identifiant` |
| `project.rs` · `row_targets` ½ | B5 | F5 | moy. | une commande de lignes sans cible réussit sans rien faire | `b12_une_commande_sans_effet_noccupe_pas_un_cran_dannulation` |
| `project.rs` · `next_column_name`, `next_child_name`, `next_window_name` | D1 | F7 | moy. | nom automatique en O(n³) : 4 000 colonnes = 17,8 s | `b17_nommer_une_colonne_automatiquement_ne_coute_pas_plus_que_la_nommer` |
| `resolve.rs:426` · `histogram` | A5 | rendu | moy. | étendue infinie → bornes `NaN` et comptage faux | `b1_histogram_intervalle_infini_ne_produit_pas_de_nan` |
| `resolve.rs:362` · `bar_width` | A6 | rendu | faible | rend la largeur demandée sans la filtrer, doc contredite | `b1_bar_width_demandee_absurde_est_filtree` |
| `resolve.rs` · `resolve_series` | A10 | F1·rendu | moy. | une colonne d'erreur courte tronque les **données** ; vide, elle efface la série | `b16_une_colonne_derreur_courte_ne_tronque_pas_les_donnees` |
| `resolve.rs` · `histogram` ½ | B4 | F3·F4·rendu | moy. | `bins` non borné → 68 Gio demandés, `abort()` | *sonde* `plotyx-sonde-alloc histogram_bins` |
| `tree.rs` · `check_name_available`, `validate_loaded_names` | A8 | F9 | **haute** | une fenêtre prend le nom d'un dossier frère ; le dossier devient introuvable | `b11_path_of_se_re_resout_toujours_sur_le_meme_noeud` |
| `tree.rs` · `find_window` → `windows` → `walk_all` | D2 | F7 | moy. | `resolve_path` alloue en O(nœuds du projet), à chaque appel | `b17_resoudre_un_chemin_ne_depend_pas_de_la_taille_du_projet` |

### `plotyx-plot`

| Fichier · fonction | Id | Fam. | Sév. | En une phrase | Reproduction |
|---|---|---|---|---|---|
| `spec.rs:140`, `:147` · `rows`, `columns` | B2 | F3·F4·rendu | moy. | `max + 1` déborde sur une position venue d'un fichier | `b1_plotspec_rows_columns_sur_position_extreme` |
| `series.rs` · `SeriesSpec::histogram_bins` ½ | B4 | F3·F4·rendu | moy. | `NonZeroU32` borne le zéro, rien ne borne le haut | `b13_le_type_lui_meme_borne_le_nombre_de_classes` |

### `plotyx-commands`

| Fichier · fonction | Id | Fam. | Sév. | En une phrase | Reproduction |
|---|---|---|---|---|---|
| `history.rs` · `revert`, branche `Entry::Structural` | A7 | F2 | **haute** | restaure les compteurs de version : deux contenus sous un même numéro | `b10_deux_contenus_ne_partagent_jamais_un_numero_de_version` |
| `history.rs` · `revert` + `Drop` de `Entry` | B7 | F4 | moy. | imbrication non bornée : `abort()` à 10 891 niveaux | `b18_limbrication_des_macros_est_bornee` |
| `history.rs` · `apply` (`InsertRows`/`DeleteRows`) ½ | B5 | F5 | moy. | empile une entrée pour une commande qui n'a rien touché | `b12_une_commande_sans_effet_noccupe_pas_un_cran_dannulation` |
| `history.rs` · `apply`, branche `Rename` | — | F5 | faible | « Renommer M en M » s'empile aussi — **sans identifiant, sans reproduction** (§7.3) | *(aucune)* |
| `history.rs` · `evict`, `recompute_retained_bytes` | D3 | F7 | moy. | chaque commande refait l'union de tout l'historique et parcourt l'arbre | `b21_le_cout_dune_ecriture_ne_depend_pas_de_lhistorique` |

### `plotyx-persistence`

| Fichier · fonction | Id | Fam. | Sév. | En une phrase | Reproduction |
|---|---|---|---|---|---|
| `convert.rs:396` · `record_to_panel_spec` ½ | B2 | F3·F4 | moy. | `record.row as usize` sans borne | `b3_panneau_en_position_extreme` |
| `convert.rs` · `record_to_series_spec` ½ | B4 | F3·F4 | moy. | `histogram_bins` relu sans plafond | `b13_histogram_bins_venu_du_disque_est_borne` |
| `convert.rs:425` · `record_to_plot_spec` | C1 | F3 | faible | le `format_version` d'un `PlotSpec` n'est jamais vérifié | `b3_version_de_format_de_spec_inconnue_est_acceptee_en_silence` |
| `schema.rs` (7 enums) + `lib.rs` · `convert_error` | C5 | F3 | moy. | une variante inconnue perd **tout** le fichier, et l'erreur dit « corrompu » | `b19_un_type_de_trace_inconnu_ne_perd_pas_les_donnees` |
| `lib.rs` · `load`, contrôle d'en-tête | — | F3 | faible | une version `0`, qui n'a jamais existé, est lue comme du v1 — **sans identifiant** (§7.3) | *(aucune)* |
| `lib.rs` · `save` ½ | B8 | F2 | moy. | aucune détection d'un fichier modifié ailleurs : écrasement silencieux | `b20_sauvegarder_sur_un_fichier_modifie_ailleurs_est_refuse` |

### `plotyx-session`

| Fichier · fonction | Id | Fam. | Sév. | En une phrase | Reproduction |
|---|---|---|---|---|---|
| `actor.rs` · `Request::Save` | A11 | F6 | **haute** | sauvegarde pendant une macro : le disque garde ce que l'annulation défait | `b18_la_sauvegarde_est_refusee_pendant_une_macro` |
| `actor.rs` · `Session::shutdown` | — | F6 | **haute** | commet en silence les mutations d'une macro ouverte — **sans reproduction** (§7.3) | *(aucune)* |
| `actor.rs` · `load_project` + `plotyx-script/surface.rs` · `Handle` | A3 | F2·F6 | **haute** | une poignée survit à un rechargement et désigne un autre objet | `b6_poignee_survit_a_un_load_et_designe_un_autre_objet` |
| `actor.rs` · `Request::AbortMacro` | A4 | F6 | **haute** | la macro d'un client défait le travail validé d'un autre | `b6_macro_dun_client_avale_le_travail_dun_autre` |
| `actor.rs` · `build_snapshot` ½ | C6 | F6 | faible | annonce `can_undo` pendant une macro, où `undo` est refusé | `b18_linterface_nannonce_pas_une_annulation_impossible` |

### `plotyx-script` · `plotyx-python`

| Fichier · fonction | Id | Fam. | Sév. | En une phrase | Reproduction |
|---|---|---|---|---|---|
| `bridge.rs:133` · `set_column` | A2 | F6 | **haute** | contrôle de longueur sur un instantané périmé : la colonne s'allonge | `b6_toctou_entre_instantane_et_commande` |
| `array.rs:185` · `copy_array_to_vec` | A1 | F1 | **haute** | un `np.ma.masked_array` écrit les valeurs que le masque cachait | `b7_tableau_masque_perd_son_masque` |

### `plotyx-app`

| Fichier · fonction | Id | Fam. | Sév. | En une phrase | Reproduction |
|---|---|---|---|---|---|
| `app.rs` · `cell_widget` + `state.rs` · `commit_edit` | B6 | F5 | moy. | traverser une cellule sans rien taper écrit quand même | `b15_traverser_une_cellule_sans_rien_taper_necrit_pas` |
| `state.rs` · `cell_text` + `commit_cell` | A9 | F1·F5·F8 | **haute** | une chaîne vide traversée devient une valeur absente | `b15_une_chaine_vide_survit_a_la_traversee_de_sa_cellule` |
| `state.rs` · `cell_text` ½ | C4 | F8 | faible | une mesure en 10⁻¹⁹ s'affiche sur 30 caractères | `b15_une_mesure_scientifique_tient_dans_une_cellule` |
| `state.rs` · `can_undo` ½ | C6 | F6 | faible | recopie `can_undo` sans consulter `in_macro` | `b18_linterface_nannonce_pas_une_annulation_impossible` |
| `app.rs:248` ½ | B5 | F5 | moy. | « Insérer 1 ligne » code `at: 0` en dur, inopérant sur une feuille vide | `b12_une_ligne_peut_etre_ajoutee_a_la_fin_dune_feuille` |
| `state.rs` · `save` ½ | B8 | F2 | moy. | sauvegarde sans vérifier que le fichier n'a pas bougé | `b20_sauvegarder_sur_un_fichier_modifie_ailleurs_est_refuse` |

---

## 4. Les trois natures de travail

La sévérité dit ce qui fait mal, la famille dit ce qui se décide ensemble ;
cette section dit ce qui **coûte**.

**Mécanique** (13) — la modification est déterminée, il n'y a rien à décider :
A6 · A9½ · A11 · B1½ · B2½ · B6 · B9 · C3 · C5½ · C6 · D1 · D2½ · D3.

**Une décision, puis mécanique** (13) — la question est courte, la réponse tient
en une phrase, le code qui suit est petit :
A1 · A5 · A9½ · A10 · B1½ · B2½ · B3 · B4 · B5 · C1 · C2 · C4 · C7.

**Chantier** (9) — un invariant, un type, ou le protocole :
A2 · A3 · A4 · A7 · A8 · B7 · B8 · C5½ · D2½.

**Comment lire les décomptes.** 13 + 13 + 9 = 35 entrées pour trente
anomalies : cinq d'entre elles — **A9, B1, B2, C5, D2** — ont une moitié dans
une nature et l'autre moitié dans une autre, et c'est précisément ce que le
signe ½ veut dire. Aucune ne doit être considérée comme close tant que sa
moitié la plus chère ne l'est pas : le `checked_add` de B1 ferme la panique, pas
le chemin d'entrée ; le niveau 1 de C5 corrige le mot, pas la perte du fichier.

Les treize décisions courtes appartiennent à `questions-ouvertes.md`, pas à un
correctif discret — mais **groupées par famille** : F1 en pose une pour quatre
anomalies, F4 une table de six nombres, F5 une seule alternative pour quatre
cas. Ce sont **cinq questions, pas treize**.

### Les douze lignes à écrire

Treize moitiés mécaniques, douze lignes : **A9 et B6 partagent la même**.

| Id | Fam. | Ce qu'on écrit |
|---|---|---|
| A6 | rendu | `requested.filter(\|w\| w.is_finite() && *w > 0.0)` dans `bar_width` |
| A11 | F6 | la garde de `load_project` recopiée sur `Request::Save` (et sur `Shutdown`) — **palliatif déclaré**, voir F6 |
| B1 ½ | F2 | `checked_add` dans `IdAllocator::next`, qui rend `CoreResult<NodeId>` |
| B2 ½ | F4 | `saturating_add(1)` dans `rows`/`columns` |
| B6 | F5 | ne pas émettre de commande quand le tampon de saisie n'a pas changé (corrige aussi A9) |
| B9 | F2 | retirer `#[derive(Clone)]` de `Project` — rien ne l'utilise |
| C3 | F5 | sortie anticipée de `write_block` quand `new_values.is_empty()` (⚠ §6) |
| C5 ½ | F3 | reconnaître « unknown variant » et rendre autre chose que `Corrupt` |
| C6 | F6 | `can_undo: history.can_undo() && !history.in_macro()` dans `build_snapshot` |
| D1 | F7 | un `HashSet` des noms pris, et partir de `children.len()` plutôt que de 0 |
| D2 ½ | F7 | `find_window` itère au lieu de `collect` (supprime l'allocation, pas le parcours) |
| D3 | F7 | un majorant en O(entrées) sur `Pinned`, le calcul exact seulement s'il franchit la limite |

---

## 5. Un ordre suggéré

L'ordre ci-dessous n'est pas une priorité de produit — celle-là appartient à
qui décide. C'est l'ordre qui minimise le travail refait : **une famille à la
fois**, sa question tranchée avant sa première ligne de code, et les charnières
placées du côté qui donne la forme plutôt que du côté qui la consomme.

**Vague 1 — F7, le coût, plus trois correctifs qui n'engagent rien.**
D1 · D2(option 2) · D3 · A6 · C6 · A11.
F7 est la seule famille sans aucun arbitrage sémantique, et la seule dont les
reproductions comptent des **octets** : ses quatre reproductions deviennent le
jour même des garde-fous de CI qui mesurent le coût réel d'une écriture. S'y
ajoutent, sans rien engager : A6 (trois lignes, sans famille) ; C6 (une
conjonction, vraie quoi que décide F6, puisque `undo` **est** refusé pendant
une macro) ; et A11, dont les trois lignes ferment une corruption de sévérité
haute — mais **comme palliatif déclaré**, la table des huit requêtes de F6
restant ouverte (vague 6). Sept reproductions au vert.

**Vague 2 — F5, une commande acceptée a un effet.**
B6 + A9(option 1) **ensemble** — c'est **un seul correctif** qui fait passer
deux reproductions — puis C3, B5, et le cas `Rename` qui n'a pas
d'identifiant. Une décision courte (refus, ou succès sans entrée), quatre
correctifs, cinq reproductions. Attention au piège de `BeginMacro` (§6) : la règle porte sur
l'entrée d'historique, pas sur le mouvement d'un compteur. La convention
d'affichage d'une chaîne vide (F1/F8) n'est **pas** requise ici et se décide
après.

**Vague 3 — F4 puis F3 : la table des bornes, puis le lecteur.**
D'abord six nombres décidés d'un coup (lignes, classes, profondeur de macro,
longueur de nom, plafond d'identifiant, coordonnées de panneau), puis **un seul
passage** sur `convert.rs` / `lib.rs` / `schema.rs` :
B3 · B4 · B7(option 1) · B1(½2) · B2(les deux bouts le même jour) ·
C1 · C5(niveau 1) · l'en-tête de version `0`.
Décider C1 **et** C5 avant d'écrire l'un ou l'autre : le premier rend le second
inatteignable s'il passe en premier (§F3).

**Vague 4 — F1 et F8 : la valeur manquante et sa forme écrite.**
Une question — *non représentable : refusée, ou manquante ?* — puis A1 · C2 ·
A10 · C4 · A9(option 2), plus A5 qui pose la même question au cas dégénéré de
l'histogramme. C'est la vague où se fixe la table de conversion valeur ↔ texte,
lue par `cell_text` **et** par `set_column_type`.

**Vague 5 — F9, les noms, et la migration.**
A8 · C7. La règle complète d'abord (portée + caractères + longueur), le
mécanisme de rattrapage des fichiers antérieurs ensuite — **un seul** pour les
deux. C'est la première vague qui rend illisible un fichier aujourd'hui
acceptable : elle mérite sa propre revue.

**Vague 6 — F2 puis F6 : les estampilles, puis le protocole.**
A7 · B8 · B9(si pas déjà fait), qui fixent la doctrine des estampilles et
donnent à A3 la forme de la sienne — **non persistée**, à la différence des
deux autres. Puis A3 · A2 · A4, et la table des huit requêtes qui referme pour
de bon la question que le palliatif d'A11 (vague 1) avait seulement masquée —
`Session::shutdown` comprise, qui n'a pas de reproduction (§7.3).

### Avant le chantier de rendu

Quelle que soit la vague où elles tombent, **A5, A6, A10, B2 et B4 doivent être
closes avant** que `docs/newarchitecture/roadmap-rendu.md` ne commence. Dans
l'ordre ci-dessus, A6 est en vague 1, B2 et B4 en vague 3, A5 et A10 en
vague 4 : l'échéance est tenue si le rendu ne démarre pas avant la fin de la
vague 4.

---

## 6. Neuf pièges de correction

Chacun a été vérifié sur le dépôt ; ce ne sont pas des précautions de principe.

**C3 — ne pas faire *échouer* un bloc vide, le rendre neutre.**
`History::revert_delta` appelle `set_values(start, before)` avec un `before`
**vide** chaque fois qu'on annule une écriture faite au-delà de la fin de la
colonne (vérifié : `SetCells { start: 5 }` sur une colonne de 3 lignes produit
exactement ce delta). Un correctif qui rendrait `Err` sur un bloc vide casserait
l'annulation. La sortie anticipée doit rendre `Ok` avec un delta neutre.

**F5 — `BeginMacro` fait avancer la révision sans mutation, exprès.**
`actor.rs` le documente : « l'état de l'historique (`in_macro`) a changé, donc
la révision avance même sans mutation du modèle ». La règle de F5 porte sur
l'**entrée d'historique**, jamais sur le mouvement d'un compteur ; formulée
trop largement, elle casse un comportement voulu.

**D3 — ne pas supprimer la comptabilité, seulement sa fréquence.**
`tests/memory_pinning.rs` (1 113 lignes, hors banc) vérifie que les octets
annoncés sont les bons, et la limite en octets existe pour un motif mesuré
(alterner une opération de structure et une écriture épingle une version
complète du buffer). Le majorant proposé **sur-estime**, donc ne peut jamais
manquer une éviction : c'est ce qui rend l'optimisation sûre.

**F9 / A8 — la règle doit être *resserrée*, pas relâchée.**
Aligner `validate_loaded_names` sur `check_name_available` en relâchant ce
dernier ferait disparaître la reproduction sans corriger l'anomalie : c'est la
résolution de chemin qui deviendrait ambiguë. Le sens du correctif est de
refuser plus, ce qui rendra illisibles des fichiers aujourd'hui acceptés — d'où
la migration à prévoir, et la décision à prendre avant d'écrire la ligne. **C7
a le même besoin** et ne le dit nulle part.

**F2 / A7 — le compteur reste hors de `PartialEq`.**
L'exclusion est délibérée et motivée (« annuler une écriture est *une écriture
de plus* ») ; la propriété centrale de `undo_random.rs` en dépend. Une
estampille globale change *qui attribue le numéro*, pas *qui le compare*. Et
elle doit traverser le disque : `next_id_hint` montre le patron exact.

**F2 / A3 — une époque persistée réintroduit l'anomalie.**
C'est le piège symétrique du précédent. L'estampille d'A7 et le compteur d'A11
doivent traverser le disque ; **l'époque de projet ne le doit pas**. Écrite dans
le fichier, elle revient égale quand on rouvre le même fichier ou un fichier de
même forme — le cas même qu'elle est censée attraper.

**F3 — C1 avant C5(niveau 2) rend le second inatteignable.**
Refuser un `PlotSpec` de version supérieure (C1) écarte le fichier avant que la
dégradation gracieuse (C5, niveau 2) n'ait la moindre chance de jouer. La règle
du numéro de version — **un champ ne l'oblige pas à bouger, une variante
l'oblige** — se décide avant les deux.

**B5 — le niveau feuille et le niveau colonne sont deux chemins distincts.**
`Project::insert_rows` (feuille) n'est appelée que par `History::apply` ;
l'annulation et le rétablissement passent par `insert_column_rows` /
`delete_column_rows` (colonne). Rendre une erreur au niveau feuille ne touche
donc pas le chemin d'annulation — vérifié.

**B1 — deux verrous, pas un.**
Le `checked_add` dans `next()` (mécanique, vague 1) ferme la panique ; il ne
ferme pas le chemin d'entrée. Tant qu'un `.plx` peut porter un identifiant
proche de 2⁶⁴, un projet chargé reste inutilisable. Le plafond à la lecture
appartient à F3, et il est de la vague 3.

---

## 7. Ce que le banc ne peut pas voir

Quatre constats qui ne sont pas des anomalies, mais qui changent la confiance
qu'on accorde au vert.

### 7.1 Deux garde-fous verts qui ne peuvent pas voir ce qu'ils semblent couvrir

**`undo_random.rs` est aveugle à A7, par construction et pour toujours.** Sa
propriété centrale — `undoing_everything_restores_the_initial_tree_bit_for_bit`
— compare des arbres via le `PartialEq` de `Column`, qui **exclut délibérément
le compteur de version**. Elle ne pouvait pas voir A7 ; le correctif d'A7 ne la
rendra pas capable de le voir. Seules `b10` et `b14` le peuvent.

**La huitième propriété de `b8_proprietes` est verte parce que son générateur
évite le cas fautif.** `b8_chemin_canonique_se_reresout` affirme
`path_of ∘ resolve_path = identité` ; A8 la rend **fausse** sur des arbres que
l'API publique construit. Elle passe parce que son générateur nomme toujours la
fenêtre `Book1` et ne tire jamais de `Move` — donc jamais un dossier et une
fenêtre homonymes. Le générateur de `b14` le fait, et la propriété tombe.

**La leçon vaut pour les sept autres.** Une propriété verte dont le générateur
exclut le cas fautif est un faux garde-fou : elle donne l'assurance sans la
couverture. `b14` a trouvé A7 **et** A8 sans qu'on lui dise où regarder,
précisément parce qu'il ne vise rien. Les sept autres propriétés de `b8`
méritent le même examen — non pas « passent-elles ? » mais « que leur générateur
ne tire-t-il jamais ? ».

### 7.2 Une anomalie sans reproduction, par nécessité

**B3** est la seule des trente dont aucune reproduction n'existe dans le banc :
son cas se termine par un `abort()` que ni `catch_unwind` ni `join` ne
rattrapent, il ne pouvait donc pas vivre dans un binaire de test. Elle est
couverte par `plotyx-sonde-alloc`, qui n'a pas d'assertion : on lit le code de
sortie. B4 et B7 ont *aussi* une sonde, mais elles ont en plus une reproduction
qui éprouve l'autre moitié du chemin (le fichier pour B4, la borne pour B7).

Conséquence pratique : **la mesure d'avancement (§8) ne verra jamais B3
descendre.** Sa clôture se constate à la main.

### 7.3 Quatre constats consignés dans le corps d'une section, et nulle part ailleurs

Chacun est décrit dans un rapport, aucun ne porte d'identifiant, aucun n'a de
reproduction. Ils appartiennent pourtant à une famille, et disparaîtront sans
elle.

| Le constat | Où il est écrit | Famille |
|---|---|---|
| `Session::shutdown` pendant une macro ouverte **commet** ses mutations en silence, sans entrée d'historique et sans qu'`end_macro` ait été appelé | suite, §A11 | F6 |
| `Command::Rename` vers le nom courant rend `Ok` sans toucher à l'arbre et empile « Renommer M en M » | suite, §B5 | F5 |
| un en-tête de version `0` — un numéro qui n'a jamais existé — est accepté et lu comme du v1 (le contrôle ne compare que `version > FORMAT_VERSION`) | suite, §C5 | F3 |
| aucune borne de longueur sur `unit`, `long_name`, `comment`, `display_format`, ni sur le texte d'une note — C7 ne couvre que le **nom** | suite, §C7 | F4 |

Deux autres constats annexes n'ont pas de famille et restent à leur place :
`DeleteRows` puis `InsertRows` au niveau feuille n'est pas neutre (matin, §8 —
à revoir le jour où l'interface expose les deux côte à côte), et
`cargo fmt --all -- --check` échoue déjà sur treize emplacements antérieurs au
banc (matin §8, suite §10 — hors périmètre, délibérément).

### 7.4 Ce que cette révision du guide a corrigé

Aucune anomalie n'a changé. Trois erreurs de ce guide, en revanche :

- **B6 n'apparaissait dans aucune des trois natures de travail** (§4) : elle
  était listée en §3 et en §5, et manquait au recensement par coût. Rétablie.
- **Les décomptes de §4 ne tombaient pas juste** : 11 + 9 + 7 = 27 pour trente
  anomalies, et les moitiés n'étaient comptées ni d'un côté ni de l'autre de
  façon régulière. Ils sont désormais 13 + 13 + 9 = 35 entrées pour 30
  anomalies, les cinq moitiés partagées (A9, B1, B2, C5, D2) étant comptées
  **des deux côtés** et nommées comme telles.
- **« seize fichiers »** : ils sont **dix-huit**, vérifiés un par un
  (`plotyx-script/src/surface.rs`, cité dans la ligne d'A3, et
  `plotyx-persistence/src/lib.rs` manquaient au compte).

---

## 8. Où en est le banc

```text
21 batteries · 46 reproductions · 2 sondes hors-processus

reproductions rejouables sans la feature `python`   : 43
reproductions derrière la feature `python` (A1, A2) :  3

29 des 30 anomalies ont au moins une reproduction ; B3 n'en a pas (§7.2)

cargo test --workspace                       → vert (garde-fous seuls)
cargo test -p plotyx-robustesse -- --ignored → 43 échecs attendus
scripts/python-test.sh -- --ignored          → 3 échecs attendus
```

Le travail CI `robustesse` (`allow_failure: true`) affiche la liste à chaque
pipeline. **Le nombre de reproductions rouges est la mesure d'avancement** :
il doit descendre, et chaque descente s'accompagne d'un `#[ignore]` retiré.

Reproductions closes par vague, si l'ordre du §5 est suivi :

```text
vague 1  (F7 + A6, C6, A11)     7 reproductions   →  39 rouges
vague 2  (F5)                   5                 →  34
vague 3  (F4 puis F3)          11                 →  23      (B3 n'en a pas)
vague 4  (F1 et F8)             8                 →  15
vague 5  (F9)                   5                 →  10
vague 6  (F2 puis F6)          10                 →   0
```

---

## Voir aussi

- `rapport-robustesse-2026-09-11.md` — A1 à C2, onze anomalies, batteries 1 à 9,
  frontière Python comprise.
- `rapport-robustesse-2026-09-11-suite.md` — A7 à D3, dix-neuf anomalies,
  batteries 10 à 21 ; c'est là que vivent les traces, les mesures et les options
  chiffrées.
- `questions-ouvertes.md` — les décisions du §4 y ont leur place, **groupées par
  famille** : une question pour F1, une table de six nombres pour F4, une
  alternative pour F5, une table de huit requêtes pour F6, un seuil de taille de
  projet pour F7, une règle de formatage pour F8, une règle de nommage et sa
  migration pour F9.
- `cahier-des-charges-api-script.md` — §4 et §7 portent désormais le correctif
  décidé d'A2 : `expected_version`, cinq tentatives, `ConcurrentWriteError`.
  C'est le contrat qu'un auteur de script lit ; le *pourquoi* reste ici.
- `cahier-des-charges-technique.md` — fait foi sur l'architecture ; aucune des
  trente anomalies ne met en cause ses quatre décisions. C'est en revanche là
  que doit s'écrire la doctrine des estampilles (F2), qui en est une cinquième.
- `format-fichier-plx.md` — F3 tout entière, plus le compteur de sauvegarde de
  B8 et la règle « un champ n'oblige pas à bouger le numéro de version, une
  variante l'oblige ».
- `docs/newarchitecture/roadmap-rendu.md` — les cinq anomalies de l'échéance
  (A5, A6, A10, B2, B4).
- `crates/plotyx-robustesse/src/lib.rs` — les deux natures de test, et la
  convention `#[ignore]`.
