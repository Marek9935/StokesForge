# Cahier des charges technique — socle Rust et pont Python

Statut : **spécification arrêtée le 2026-09-08**, implémentation non
commencée au-delà du squelette de phase 0. Ce document fait foi sur
l'architecture ; il complète `roadmap.md` (qui dit *quand*) en disant
*comment* et *pourquoi pas autrement*. Comme `resume-decisions.md` et
`format-fichier-plx.md`, il conserve les options écartées et leur raison.

**Revue technique du 2026-09-08.** Les quatre décisions du §2 sont
confirmées ; **aucune n'est rouverte**. Ont été corrigés : l'argument
d'écartement de `slotmap` (§4.1, faux dans sa formulation, juste dans sa
conclusion), les hypothèses du chiffrage mémoire (§2.1.1), les noms d'API
PyO3 (§8.2), le mécanisme exact de la vue numpy et son contrat de sûreté
(§8.3), et les pièges d'empaquetage (§8.6.1). Ce qui reste à trancher est
regroupé, expliqué sans prérequis Rust, dans `questions-ouvertes.md`.

**Amendement du 2026-09-10 — le renderer.** Le §12 (hors périmètre) est
amendé sur son premier point : le système de graphes ne sera **pas** rendu
par Matplotlib, mais par **Plotters** avec un backend egui écrit par nous.
Décision et chiffrage complets dans `docs/newarchitecture/`. Portée exacte
de cet amendement, à ne pas élargir :

- **aucune** des quatre décisions du §2 n'est rouverte — PyO3 (§2.3) est
  conservé et sa justification ne change pas, elle était déjà autonome
  depuis le 2026-09-08 (§2.5) ;
- **aucune** ligne du socle construit n'est touchée : le renderer était
  déjà hors périmètre, et les deux crates qu'il aurait occupés (`plot`,
  `render-mpl`) n'ont jamais été écrits ;
- ce qui disparaît, c'est le renderer Python — **pas** la console de
  script, ni la surface `plotyx.api`, ni le zéro-copie du §8.3.

**Objet du socle** : disposer de tout ce qu'il faut pour **ouvrir le
chantier de l'interface**, et de rien de plus. Le modèle, l'annulation, le
fichier, une fenêtre, et surtout la **tuyauterie Rust ↔ Python** éprouvée.
Ni système de graphes, ni UI réelle : ces deux chantiers doivent pouvoir
démarrer sur un socle qui ne bougera plus sous eux.

---

## 1. Périmètre

| Dans le socle | Hors socle |
|---|---|
| Modèle de données (§4) | Système de graphes : `PlotSpec`, renderer (Plotters — voir l'amendement) |
| Historique, Ctrl+Z / Ctrl+Y (§5) | Surface complète de `plotyx.api` (v1) |
| Acteur possédant le modèle (§6) | Import de fichiers |
| Persistance minimale (§7) | Format `.plx` final (Zarr) |
| **Pont Rust ↔ Python (§8)** | UI réelle : docking, grille éditable, Properties |
| Coquille UI minimale (§9) | Mode « visualiseur léger » |

### 1.1 Le critère de fin, en une phrase

**Cible resserrée le 2026-09-08 : Linux d'abord, seul.** Le développement
et la validation du socle se concentrent sur Linux pur — Windows est une
**étape ultérieure**, pas une contrainte à porter dès la première ligne.
Ce n'est pas un abandon de la cible Windows (§8.6 la garde comme condition
de clôture du socle), c'est un ordre : prouver l'architecture sur une
seule plateforme avant de payer le coût du portage sur la seconde.

Le socle est terminé quand, **depuis un binaire empaqueté sur Linux** :

> une cellule éditée dans la fenêtre s'annule par Ctrl+Z et se rétablit par
> Ctrl+Y ; Ctrl+S écrit un fichier que Ctrl+O rouvre à l'identique ; et un
> script Python lit une colonne **sans copie**, en écrit une autre **via le
> bus** — et **Ctrl+Z annule ce que le script vient d'écrire**.

Cette dernière clause est le vrai test : elle traverse d'un coup l'acteur,
le GIL, la durée de vie des buffers partagés et l'historique. Tant qu'elle
ne passe pas, le pont n'est pas prouvé.

**Windows n'est pas hors périmètre, il est reporté après ce critère.** Une
fois la clause ci-dessus vraie sur Linux, le même hello-world et le même
test d'intégration doivent être rejoués sur Windows (§8.6) avant de
déclarer le socle définitivement clos — sans ça, l'objectif « binaire
cross-platform » de `resume-decisions.md` §2 resterait non vérifié. Rien
dans les quatre décisions du §2 ne dépend de la plateforme ; ce
resserrement ne les rouvre pas, il change seulement l'ordre dans lequel
elles se prouvent.

---

## 2. Les quatre décisions d'architecture

Décisions prises le 2026-09-08. **Le rétrécissement du périmètre n'en
rouvre aucune** — §2.5 dit pourquoi, décision par décision.

### 2.1 Undo — hybride structure / données

**Décision.** L'historique manipule deux natures d'entrée :

- **structure** (arbre, noms, ordre, métadonnées) → *snapshot persistant* :
  muter produit un nouvel arbre qui partage tout l'inchangé, annuler remet
  l'ancien pointeur ;
- **données** (contenu des colonnes) → *delta* : la commande enregistre les
  anciennes **et** les nouvelles valeurs des seules cellules écrites — les
  anciennes pour annuler, les nouvelles pour rétablir. N'enregistrer que
  les anciennes, comme le disait la rédaction initiale, rend le **redo**
  impossible dès que la commande d'origine a été relâchée.

**Ce que ça achète.** Les inverses disparaissent là où ils sont difficiles
à écrire — déplacements, collisions de noms, suppression/restauration — et
avec eux **toute la machinerie de durée de vie des objets supprimés**
(décision 4 de la phase 2 Python : miroir de pile, `release()`, gestion de
l'éviction, tests `gc`/`weakref`). L'ancien arbre retient l'objet
supprimé ; il n'y a rien à libérer explicitement. Les inverses restants
sont ceux qu'on ne rate pas : réécrire les anciennes valeurs de N cellules.

**Ce que ça coûte.** Deux natures d'entrée dans une seule pile, avec une
discipline d'ordre à tenir (§5.3). Et l'hybride **n'élimine pas** la classe
de bugs « inverse faux » : il la confine à la moitié facile. Le test de
propriété du §10 reste obligatoire.

**Écarté — delta pur** (le port mécanique de Python) : un seul mécanisme,
les dix types de commandes et leurs tests se transposent presque tels
quels, mémoire minimale. Rejeté parce qu'il oblige à réécrire la machinerie
de durée de vie ci-dessus, et parce que ce projet a déjà produit
empiriquement un bug de cette classe (`insert_rows` / `delete_rows` non
inverses, `roadmap.md` §2.4) sur la moitié structurelle — celle que
l'hybride supprime.

**Écarté — snapshot intégral** (modèle entièrement persistant, undo
impossible à écrire faux) : séduisant, mais tenable seulement si éditer une
cellule ne copie pas toute la colonne, donc il impose de **chunker** les
colonnes. Or une colonne chunkée ne se voit pas comme un seul tableau
numpy : cela casserait le zéro-copie du §8.3, qui est justement le livrable
central du socle. Motif de rejet : **la perte du zéro-copie, pas la RAM** —
les chiffres du §2.1.1 montrent que la mémoire ne départage pas.

#### 2.1.1 Ce que coûte réellement l'historique en mémoire

Projet lourd de référence : 50 colonnes × 1M points = **400 Mo de
données**, arbre de ~2000 nœuds, 100 entrées d'historique.

**Hypothèses de calcul** — explicitées le 2026-09-08, à revérifier si le
modèle change :

- 50 × 1 000 000 × 8 octets (`f64`) = 400 Mo. ✔
- un nœud est rangé **derrière un `Arc`** (§4.2) : cloner l'arbre recopie
  ~2000 paires `(NodeId, Arc<Node>)` de 16 octets et incrémente ~2000
  compteurs, **sans** recopier les noms, unités, étiquettes et
  commentaires. Table à ~50 % de remplissage → ~4096 alvéoles × 16 o ≈
  64 Ko, **~100 Ko** avec les à-côtés ;
- une entrée de données d'une cellule retient `(NodeId, index, ancienne
  valeur, nouvelle valeur)` = 32 o, plus l'indirection de l'entrée :
  **~40 o**, et non 16 — 16 o ne suffisait qu'en n'enregistrant pas la
  valeur nouvelle, ce que §2.1 vient de corriger ;
- l'`after` d'une entrée est le `before` de la suivante — **le même
  `Arc`**. Une pile de 100 entrées structurelles retient 101 arbres
  distincts, pas 200.

| | entrée structurelle | entrée de données (1 cellule) | pile de 100 |
|---|---|---|---|
| Delta pur | ~200 o | ~40 o | < 1 Mo |
| **Hybride (retenu)** | taille de l'arbre, ~100 Ko | ~40 o | **~10 Mo** |
| Snapshot intégral + chunks | ~100 Ko | 1 chunk : 32 à 512 Ko | 3 à 50 Mo |

Les ~200 o du delta pur et les ~100 Ko de l'hybride **excluent tous deux
les charges utiles retenues** : une colonne supprimée reste vivante
derrière son `Arc` dans les deux schémas, à l'identique. ~~Ce n'est pas ce
qui les départage~~ — voir le piège commun plus bas.

> ⚠️ **Mesuré le 2026-09-08, et cette dernière phrase était fausse.** Elle
> n'est vraie que d'une colonne **supprimée**. Elle est fausse d'une colonne
> **modifiée** : une entrée `Structural` retient un arbre entier, donc les
> buffers de **toutes** les colonnes tels qu'ils étaient à cet instant. Si
> une colonne est réécrite ensuite, la copie-à-l'écriture en crée une
> nouvelle version — et l'ancienne reste vivante tant que l'entrée est sur
> la pile. **En alternant opérations de structure et écritures, chaque
> entrée structurelle épingle une version complète du buffer.**
>
> Mesure (colonne d'un million de `f64` ≈ 7,6 Mo, 20 itérations,
> `crates/plotyx-commands/tests/memory_pinning.rs`, deux méthodes
> indépendantes — comptage de `Weak` promouvables et allocateur compteur —
> concordantes à moins de 1 %) :
>
> | Scénario | Versions vivantes | Retenu | Rapport |
> |---|---|---|---|
> | Écritures seules, 20 fois | 1 | 38 Mo | 1,0 × |
> | **Renommage + écriture alternés, 20 fois** | **21** | **191 Mo** | **5,0 ×** |
> | Idem, historique borné à 5 entrées | 3 | 53 Mo | 1,4 × |
> | Idem, après annulation complète | 24 | 183 Mo | 4,8 × |
> | Mêmes mutations **sans** historique | 1 | 38 Mo | 1,0 × |
>
> Extrapolé à la limite par défaut de 100 entrées, le pire cas de ce motif
> retient **~800 Mo** pour une seule colonne d'un million de points, là où
> un historique à deltas purs n'en retiendrait aucun. **C'est un coût propre
> à l'hybride**, absent du chiffrage initial, et il ne se voit pas sur la
> ligne « entrée structurelle ~100 Ko » du tableau ci-dessus : les 100 Ko
> sont la **carte**, pas ce que la carte fait vivre.
>
> Trois nuances, toutes vérifiées et qui bornent la portée :
> - **seules les colonnes réellement écrites** sont épinglées en plusieurs
>   versions ; les colonnes témoins, jamais touchées, restent à une seule
>   version dans tous les scénarios — le partage physique des nœuds
>   inchangés est confirmé ;
> - **annuler ne libère rien**, et c'est correct : l'entrée passe sur la
>   pile de rétablissement, elle n'est pas détruite ;
> - **l'éviction libère bien**, immédiatement et exactement (scénario
>   témoin : 45,8 → 38,2 Mo, soit les 7,6 Mo attendus).
>
> **Ce que ça ne remet pas en cause** : l'hybride reste le bon choix, pour
> la raison qui l'a fait retenir — il supprime la machinerie de durée de vie
> des objets supprimés, là où le bug d'inverse faux s'était réellement
> produit côté Python. Ce que ça change, c'est le **garde-fou** : une limite
> comptée en entrées ne protège pas de ce motif. `questions-ouvertes.md`
> question 7, désormais chiffrée.

> ⚠️ **Les ~100 Ko ne sont vrais que si les nœuds sont derrière un `Arc`.**
> Avec une `HashMap<NodeId, Node>` possédant directement ses `String`,
> chaque clone recopie ~2000 nœuds **et leurs ~10 000 chaînes** : ~1 Mo et
> autant d'allocations par mutation, soit **~100 Mo** pour la pile de 100.
> Toujours pas fatal face aux 400 Mo — la conclusion du §2.1 tient — mais
> un facteur 10 perdu gratuitement, et une allocation lourde payée **à
> chaque frappe** (§5.1). D'où la précision apportée au §4.2.

Face aux 400 Mo de données, aucune de ces piles n'est un problème.
L'historique est **entièrement en RAM** dans les trois cas, comme
aujourd'hui avec `QUndoStack` ; aucun déversement sur disque n'est prévu.

Le seul vrai piège mémoire est **commun aux trois** : supprimer un classeur
de 400 Mo garde ces 400 Mo vivants tant que l'entrée d'annulation est sur
la pile. C'est nécessaire — sans ça, pas d'annulation. Le garde-fou est la
limite d'historique (§5.4).

### 2.2 Propriété du modèle — acteur sur thread dédié

**Décision.** Le `Project` vit sur son propre thread, dans `plotyx-session`.
L'UI et le thread de script sont deux clients symétriques : ils envoient
des requêtes et reçoivent des snapshots immuables.

**Pourquoi, dans ce périmètre précis.** Le pont Python est le livrable, et
le script s'exécute sur un thread dédié (jamais le thread UI). Si le modèle
appartenait au thread UI, chaque appel du script devrait traverser ce
thread et attendre son tour — on éprouverait alors une tuyauterie qui n'est
pas celle qu'on livrera. L'acteur fait du script un client de première
classe dès le premier jour.

**Bénéfice structurel.** La règle d'or — aucune mutation hors du bus —
cesse d'être une convention testée pour devenir une **propriété de
propriété** : l'acteur détient le seul `&mut Project` du processus. Le test
Python `test_no_mutation_outside_command.py` n'a plus d'équivalent à
écrire ; il n'a plus d'objet.

**Ce que ça coûte.** Une centaine de lignes de plomberie — prix bas
précisément parce que la structure est persistante : l'UI garde son dernier
`Arc` d'arbre et peint sans jamais solliciter l'acteur.

**Écarté — modèle sur le thread UI + workers** (ce que fait Python) : plus
simple, mais couple le script à la boucle de rendu, dans un périmètre dont
le script est justement l'objet.

**Écarté — `Arc<RwLock<Project>>`** : contention et deadlocks, et rendu
inutile par le modèle persistant — on partage des snapshots immuables, pas
un verrou.

### 2.3 Python embarqué — PyO3, un objectif du socle

**Décision.** Interpréteur CPython **embarqué dans le processus** via PyO3,
vendored et figé, propriété exclusive du crate `plotyx-python` (§8).

Dans le périmètre initial, cette décision découlait du renderer Matplotlib.
Le renderer étant sorti du socle, elle ne découle plus de rien — elle
**est** un objectif : « Python accessible » fait partie de ce qui doit être
solide avant d'ouvrir le chantier de l'interface.

**Écarté — sous-processus Python + IPC** : isolation des crashs meilleure,
mais un aller-retour par appel rend inutilisable une console interactive,
et la vue numpy sans copie du §8.3 devient impossible (les buffers ne sont
plus dans le même espace d'adressage). Le sous-processus reste pertinent
pour un export ponctuel, cf. §12.

**Écarté — langage de script natif** (Rhai, Lua) : supprimerait tout le
poids Python, mais `cahier-des-charges-api-script.md` §8 fait de la
compatibilité des scripts déjà écrits et de l'accès à numpy un contrat.

### 2.4 Découpage — crates fins

**Décision.** Sept crates (§3), frontières imposées par le compilateur.

**Pourquoi.** C'est le mécanisme qui rend vérifiable l'équivalent Rust de
« aucun Qt dans `core` » : aucun PyO3, aucun egui dans le cœur. Et tout ce
qui touche à Python vit derrière un *feature flag*, ce qui garde
`cargo test` et la CI utilisables **sur une machine sans Python installé** —
non négociable pour ne pas rendre le socle otage de son chantier le plus
lourd.

**Écarté — 3-4 crates** : moins de friction, mais plus rien n'empêche PyO3
ou egui de remonter dans le cœur, et la CI exigerait Python partout.

### 2.5 Le rétrécissement ne rouvre aucune des quatre

| Décision | Tenait par | Après rétrécissement |
|---|---|---|
| Undo hybride | zéro-copie vers Matplotlib **et** vers les scripts | tient : le zéro-copie vers les scripts est le livrable §8.3 |
| Acteur | thread de script exigé par l'API | tient, et se renforce : le script est l'objet du socle |
| PyO3 | conséquence du renderer | tient, comme **objectif** et non plus comme conséquence (§2.3) |
| Crates fins | frontières vérifiables | tient, mais passe de 9 à **7** : `plot` et `render-mpl` sortent |

---

## 3. Carte des crates

| Crate | Rôle | Dépend de | Python ? |
|---|---|---|---|
| `plotyx-core` | Modèle : arbre, nœuds, colonnes. Aucune mutation publique. | — | non |
| `plotyx-commands` | Commandes et pile d'historique à deux natures | `core` | non |
| `plotyx-persistence` | `save` / `load` d'un `Project` complet | `core` | non |
| `plotyx-session` | **L'acteur** : possède le `Project`, sert les clients | `core`, `commands`, `persistence` | non |
| `plotyx-python` | Interpréteur embarqué, GIL, conversion de tableaux. **Ne connaît pas le modèle.** | — | **oui** |
| `plotyx-script` | Module `pyx`, thread de script, surface-témoin | `session`, `python` | **oui** |
| `plotyx-app` | Coquille egui + binaire | tous | via features |

`plotyx-cli` (squelette de phase 0) reste comme binaire headless de test.

**Frontière à tenir** : `plotyx-python` ignore tout du modèle — il ne sait
que faire vivre un interpréteur et convertir des tableaux.

> **Amendement du 2026-09-10.** Cette frontière était aussi justifiée par un
> futur `render-mpl` qui se serait posé dessus sans passer par
> `plotyx-script`. Ce consommateur-là n'existera pas : le rendu est
> désormais Rust natif et **ne touche pas à Python**. La frontière reste
> néanmoins bonne telle quelle — elle est ce qui garantit que l'acteur ne
> peut pas nommer un type Python (§6.3), et cet argument-là ne dépendait
> pas du renderer.

---

## 4. Modèle — `plotyx-core`

### 4.1 Identité des nœuds — compteur monotone, **pas** `slotmap`

**Écart assumé avec `roadmap.md` §0.4**, qui proposait `slotmap` /
`generational-arena`. **La décision ne change pas ; son argument est
corrigé le 2026-09-08** — la rédaction initiale parlait d'une « collision
d'identité », précisément ce que les générations de `slotmap` empêchent.

*Ce qui était faux.* Une clé `slotmap` est une paire `(alvéole, version)`.
Retirer un nœud incrémente la version de l'alvéole : l'ancienne clé devient
**définitivement invalide** et le reste même si l'alvéole est réattribuée à
un autre nœud. Il n'y a donc jamais deux objets répondant à la même clé —
c'est exactement ce que le versionnement est fait d'empêcher.

*Ce qui reste vrai, et suffit à écarter l'arène* — trois raisons
indépendantes :

1. **La clé ne survit pas à l'annulation.** `SlotMap` choisit la clé à
   l'insertion ; aucune API ne permet de réinsérer *à une clé donnée*. Un
   nœud supprimé puis restauré par `undo` reçoit donc une clé **neuve**, et
   toute poignée conservée par un script devient morte. Or
   `cahier-des-charges-api-script.md` §3 en fait un contrat explicite :
   « une clé résolue vers un objet supprimé puis restauré par `undo` reste
   valide ». C'est ce contrat, et non une collision, que l'arène casse.
2. **Le seul remède rouvre ce que §2.1 avait fermé.** Conserver la clé
   impose de ne jamais appeler `remove` tant qu'une entrée d'historique
   retient le nœud — donc de reconstruire la comptabilité de durée de vie
   que l'undo hybride venait justement de supprimer (`roadmap.md` §2.3,
   décision 4 de la phase 2 Python).
3. **Une arène est un conteneur mutable unique**, à contre-emploi d'un
   arbre partagé par `Arc` : la rendre persistante revient à la cloner
   entière, c'est-à-dire à faire exactement ce que fait déjà une `HashMap`,
   avec un crate de plus.

**Retenu** : `NodeId(u64)`, compteur monotone, **jamais réutilisé**, dans
une `HashMap<NodeId, Arc<Node>>` (§4.2). Une restauration réinsère le nœud
sous son identifiant d'origine, sans arbitrage possible. La valeur haute du
compteur est **persistée avec le fichier** (§7) ; au chargement, le
compteur repart à `max(valeur persistée, plus grand NodeId lu) + 1` — lire
le maximum effectif est une ceinture gratuite contre un fichier écrit par
une version boguée. Un `u64` monotone ne déborde pas : à un million de
nœuds créés par seconde, il tient un demi-million d'années.

*Collision réelle, gardée en mémoire* : elle existe bel et bien avec une
arène à **index nu** (un `Vec` et une liste de trous, **sans** version).
C'est ce cas-là que la rédaction initiale décrivait, et que le compteur
monotone écarte trivialement.

Ce `NodeId` tient les trois rôles : clé d'arbre, clé opaque exposée aux
scripts (`cahier-des-charges-api-script.md` §3), et — plus tard —
référence de série dans un `PlotSpec`. Ce cahier des charges API décrit
encore cette clé comme un « UUID stable », héritage d'`ObjectRegistry` :
côté script, seule l'**opacité** est contractuelle, pas la forme. Un `u64`
la respecte ; la mention « UUID » est à corriger le jour où ce document
sera rouvert (`questions-ouvertes.md`, question 1).

*Alternative en réserve* : UUID v4, seulement si l'identité doit un jour
survivre à la fusion de deux projets distincts. Besoin inexistant, et un
`u64` est plus petit et plus rapide.

### 4.2 Arbre

`Arc<Tree>`, où `Tree` contient la `HashMap<NodeId, Arc<Node>>` et la
racine. Muter = cloner l'arbre (`Arc::make_mut` sur l'arbre) puis remplacer
les seuls nœuds touchés (`Arc::make_mut` sur ces nœuds-là).

**Le nœud est derrière un `Arc`, pas rangé par valeur** — précision ajoutée
le 2026-09-08 par la revue technique : c'est elle qui rend vrai le chiffre
du §2.1.1. Cloner la carte recopie alors ~2000 paires de 16 octets et
incrémente ~2000 compteurs — ~100 Ko, **zéro allocation par nœud**. Avec
des `Node` possédés par valeur, le même clone recopierait aussi les
~10 000 `String` de noms, unités, étiquettes et commentaires : facteur 10
en mémoire *et* en nombre d'allocations, payé **à chaque mutation**, y
compris à chaque frappe dans une cellule (§5.1). La formule « les charges
utiles étant derrière `Arc` » de la rédaction initiale ne visait que les
colonnes ; elle doit couvrir le nœud entier.

Structure conservée du modèle Python : `Folder → Workbook → Worksheet →
Column`, `Graph` / `Matrix` / `Note` en nœuds frères du `Workbook`,
grammaire de chemins `[Book1]Sheet1!ColA` reprise de `core/paths.py`. Les
nœuds `Graph` existent dans le modèle **sans** rien savoir d'un rendu — le
système de graphes viendra se poser dessus.

*Écarté* : `im` / `rpds` (cartes persistantes, clone en O(log n)). Inutile
à cette échelle — un document se compte en milliers de nœuds — et c'est une
dépendance de moins.

### 4.3 Colonnes — contiguës, jamais chunkées

| Type | Représentation | Valeur manquante |
|---|---|---|
| Numérique / date-heure | `Arc<Vec<f64>>` | `f64::NAN` |
| Texte | `Arc<Vec<Option<String>>>` | `None`, distinct de `Some("")` |

La contiguïté est une **exigence contractuelle** : elle conditionne le
zéro-copie du §8.3. Aucune évolution ne doit la remettre en cause sans
rouvrir §2.1.

Écriture par `Arc::make_mut` : la copie n'a lieu **que** si quelqu'un
d'autre détient encore ce buffer. ~~En édition courante, sans lecteur
concurrent, l'écriture est en place et gratuite.~~

> **Précision apportée par la phase 1 (2026-09-08).** La phrase barrée
> était trop optimiste : dans l'architecture retenue, il y a **toujours** un
> détenteur concurrent — l'UI garde le dernier `Snapshot { tree }` (§6.2),
> et cet arbre retient le nœud colonne, qui retient le buffer. Chaque
> écriture de cellule copie donc **le buffer de la colonne touchée**
> (8 Mo pour 1M de points, de l'ordre de la milliseconde), pas seulement
> ~100 Ko de carte. Ce n'est pas un coût mémoire cumulé : l'entrée
> d'historique `Data` ne retient pas l'ancien buffer, et l'UI le relâche en
> rafraîchissant son snapshot à la réponse suivante. C'est un coût **CPU
> O(n) par écriture**, borné à une colonne. Acceptable pour le socle, à
> **mesurer en phase 4** sur une colonne à 1M de points ; si c'est trop, la
> parade est côté client (ne pas détenir le buffer pendant une frappe), pas
> côté modèle — le chunking reste écarté (§2.1). L'écriture est bel et bien
> en place et gratuite quand aucun snapshot n'est vivant, ce que le test
> `critere_19` de `plotyx-core` vérifie.
>
> **Mesuré en phase 4 (2026-09-08), et la parade annoncée ne marche pas.**
> Une écriture d'une cellule dans une colonne d'un million de points coûte
> **≈ 0,55 ms**, que l'interface détienne un instantané **ou non** : les deux
> scénarios sont indiscernables. La raison n'est pas l'interface, c'est le
> bus : `History::execute` clone `Arc<Tree>` **avant** d'appliquer toute
> commande — pour calculer le libellé sur l'arbre d'avant, et pour pouvoir
> restaurer en cas d'échec. Ce clone local suffit à déclencher la cascade de
> copie-à-l'écriture jusqu'au buffer, pour **tout** client, y compris un
> script Python qui n'affiche rien.
>
> Conséquence : la copie de 8 Mo par écriture est aujourd'hui un coût
> **structurel de l'historique**, pas une conséquence de l'interface, et la
> phrase « gratuite sans lecteur concurrent » n'est vraie que du modèle nu,
> jamais du chemin réel. Le rendre vrai à nouveau est possible et peu
> coûteux — calculer le libellé sans retenir de clone, et ne pas garder
> l'arbre d'avant pour les commandes de **données** (les primitives du cœur
> garantissent déjà de ne rien changer en cas d'échec, et c'est testé) —
> mais ce n'est **pas fait** : à 0,55 ms par frappe, la mesure ne le rend
> pas urgent, et le gain ne profiterait qu'aux clients qui ne détiennent
> aucun instantané, donc au pont Python plus qu'à l'interface.

Longueurs hétérogènes tolérées entre colonnes d'une même feuille
(décision 2 de la phase 1 Python — l'argument ne dépend pas du langage).

### 4.4 Invariants à tester dès la phase 1

1. Une colonne de longueur L n'a pas de ligne au-delà de L : insertion et
   suppression de lignes ignorent **le même ensemble** de colonnes trop
   courtes. C'est l'invariant dont la violation a produit le bug
   `insert_rows` / `delete_rows` côté Python — à porter comme test, pas à
   redécouvrir.
2. Unicité des noms entre frères d'un même parent.
3. Aucun `NodeId` réutilisé sur toute la vie d'un projet, rechargement
   compris.

---

## 5. Historique — `plotyx-commands`

### 5.1 La pile à deux natures

```
enum Entry {
    Structural { before: Arc<Tree>, after: Arc<Tree>, label: String },
    Data       { delta: Box<dyn DataDelta>, label: String },
    Macro      { entries: Vec<Entry>, label: String },
}
```

`before` et `after` partagent presque tout : garder les deux coûte un
pointeur, et le redo devient symétrique de l'undo. L'`after` d'une entrée
**est** le `before` de la suivante — le même `Arc`, pas une seconde copie
(§2.1.1).

**Une écriture de cellules produit elle aussi un nouvel arbre.** La colonne
vit dans un nœud, le nœud dans la carte : `Arc::make_mut` remonte la
chaîne. Ce que l'hybride sépare n'est donc pas « l'arbre change / ne change
pas », mais **ce que l'entrée retient** : `Structural` retient deux arbres,
`Data` ne retient que des valeurs et les rejoue dans l'arbre courant. C'est
ce qui garde une frappe à ~100 Ko de churn (§4.2) au lieu d'un arbre retenu
par frappe.

**Invariant LIFO — non négociable.** Une entrée `Structural` ne peut être
annulée que si **toutes** les entrées postérieures l'ont déjà été.
Restaurer un arbre entier écrase tout ce qui a été écrit après lui : hors
de l'ordre de pile, l'annulation d'un snapshot avale silencieusement des
deltas. La pile le garantit par construction ; le §5.3 énonce la même règle
là où elle n'est **pas** gratuite — à l'intérieur d'une macro.

### 5.2 Commandes

Primitives du socle : `SetCells`, `AddColumn`, `DeleteColumn`, `Rename`,
`Move`, `SetColumnType`, `InsertRows`, `DeleteRows`. Les commandes de
`PlotSpec` viendront avec le système de graphes.

**Application avant empilement** : la commande est appliquée par le bus et
n'est empilée que si elle réussit. Un échec (nom en double, index hors
bornes) remonte en `Err` et la pile n'a pas bougé. Reprise de la décision 3
de la phase 2 Python, dont la justification change : Python s'en servait
pour éviter qu'une exception traverse la frontière C++ de Qt ; ici c'est
simplement le comportement correct d'une API à `Result`.

`SetColumnType` reste le **seul** cas à snapshot de données documenté : la
conversion de type est destructrice dans les deux sens.

**Pas de coalescence.** Deux `SetCells` consécutifs sur la même colonne
restent deux entrées distinctes dans l'historique — décidé le 2026-09-08,
tranché plutôt que laissé ouvert. Le comportement « tableur » (fusionner
des frappes successives en une seule entrée) exigerait un critère de
regroupement (fenêtre de temps ? même cellule ? même appelant ?) qui
n'apporte rien à ce socle et qui se décidera, si besoin, avec l'UI réelle
où ces frappes ont lieu. Les quatre critères possibles, et celui d'entre eux
qui toucherait au protocole du §6, sont recensés dans
`questions-ouvertes.md`, question 5 — pour ne pas rejouer la discussion à
zéro le jour venu.

### 5.3 Macros — la discipline qui fait tenir l'hybride

Une macro s'annule en rejouant ses entrées **strictement en ordre inverse
d'insertion**, quelles que soient leurs natures. C'est la seule règle qui
fait composer snapshots de structure et deltas de données : un snapshot
restauré hors d'ordre annulerait silencieusement un delta appliqué après
lui.

À tester explicitement sur une macro mixte — une modification de structure
et une écriture de cellules dans la même macro, dans les deux ordres.

`abort_macro()` : dépiler, annuler, jeter. Aucun contournement nécessaire —
la contrainte Qt qui imposait côté Python de pousser une commande se
déclarant obsolète pendant son `redo()` (décision 6) n'existe pas ici.

> **Précision de la phase 2 (2026-09-08) — une macro avortée ne laisse
> aucune trace, y compris sur la branche de rétablissement.** `QUndoStack`
> détruit la pile de redo dès `beginMacro()` ; avorter la macro ne la
> rendait pas. Ici, la pile de redo est mise de côté à l'ouverture de la
> macro la plus externe, **restaurée** si cette macro est avortée ou se
> ferme vide, jetée si elle aboutit avec au moins une entrée (comme pour
> toute commande aboutie). Une macro imbriquée avortée ne touche qu'à ses
> propres entrées. Coût : une dizaine de lignes ; bénéfice : « annuler,
> commencer un groupe, changer d'avis » ne perd plus le rétablissement.
>
> Deux autres choix de la même phase, consignés pour ne pas être rejoués :
> une commande **valide sans effet** (insérer des lignes au-delà de toutes
> les colonnes, réécrire une cellule à l'identique) **reste une entrée**
> d'historique — sinon la position du curseur d'historique dépendrait des
> données ; et le libellé d'une commande est calculé sur l'arbre **d'avant**
> son application (le nom renommé y est encore), ce qui donne à
> `AddColumn` un libellé fixe « Ajouter une colonne ».

### 5.4 Limite d'historique

Configurable, bornée par défaut. Seul garde-fou contre le piège mémoire du
§2.1.1. Une entrée évincée relâche ses `Arc` ; les buffers qu'elle seule
retenait sont libérés à ce moment.

La **valeur** par défaut, **tranchée le 2026-09-09** : une **limite double**,
la plus contraignante des deux gagnant, avec éviction silencieuse de l'entrée
la plus ancienne.

| | Constante | Valeur | Ce qu'elle protège |
|---|---|---|---|
| entrées | `DEFAULT_LIMIT` | **100** | la profondeur d'annulation attendue |
| octets retenus | `DEFAULT_BYTE_LIMIT` | **512 Mo** | le motif mesuré au §2.1.1, que la limite en entrées laisse passer |

Ce qui est compté : les octets de charge utile que l'historique est **seul**
à maintenir vivants — les buffers atteignables depuis les entrées et
**absents de l'arbre courant**. Un buffer encore utilisé par l'arbre courant
n'est pas un coût de l'historique. Chaque entrée calcule son empreinte
(adresse du buffer → octets) **une fois, à l'empilement** — un arbre est
immuable, donc son ensemble de buffers ne change jamais — et le total est
l'union des empreintes des deux piles moins l'arbre courant. Coût mesuré :
**~10 µs par empilement** à la limite par défaut. Les adresses ne servent
que d'identité, jamais de déréférencement.

Deux règles à ne pas perdre : **on n'évince jamais la dernière entrée**,
même si elle dépasse à elle seule le plafond — une opération lourde doit
rester annulable juste après avoir été faite, sinon la limite produit
exactement le désagrément qu'elle cherche à éviter ; et l'éviction ne touche
jamais la branche de rétablissement, qui peut donc maintenir le total
au-dessus du plafond jusqu'à la commande suivante.

**Efficacité vérifiée** (`tests/memory_pinning.rs`, scénario 7) : le motif
qui épinglait 21 versions du buffer et 191 Mo en retient **9 et 61 Mo** sous
un plafond de 64 Mo. La comptabilité interne et une mesure indépendante par
`Weak` promouvables concordent à **0,00 %**. Une macro
compte pour une entrée ; `0` = illimité (tests). La revue du 2026-09-08
relève qu'une limite comptée en *entrées* ne protège pas du cas coûteux :
une seule suppression de classeur pèse 400 Mo, cent renommages ne pèsent
rien — la limite en octets reste à rouvrir seulement si un cas réel de
saturation apparaît.

---

## 6. Session — `plotyx-session`, l'acteur

### 6.1 Protocole

Un canal de requêtes FIFO — donc ordre déterministe — une réponse par
requête :

```
enum Request {
    Execute { command: Command, reply: Reply<Result<Revision, CommandError>> },
    Undo { reply: … }, Redo { reply: … },
    BeginMacro { label: String }, EndMacro, AbortMacro,
    Save { path: PathBuf, reply: … }, Load { path: PathBuf, reply: … },
    Snapshot { reply: Reply<Snapshot> },
}

struct Snapshot { tree: Arc<Tree>, revision: u64 }
```

La `Revision` est un compteur monotone : un client sait si son snapshot est
périmé sans comparer les arbres.

> **Précisions de la phase 2 (2026-09-08).** Le `Snapshot` porte aussi
> l'état de l'historique (`can_undo`, `can_redo`, libellés, `in_macro`) :
> c'est ce dont l'UI a besoin pour griser et libeller ses actions (§9) sans
> requête supplémentaire. La révision s'incrémente à chaque changement
> effectif (commande réussie, undo/redo effectif, ouverture/fermeture/
> abandon de macro), jamais sur une erreur ni sur un undo à vide. `Undo`
> répond `Option<Revision>` : `None` = rien à annuler, ce n'est pas une
> erreur (même position que `questions-ouvertes.md` question 2). Une macro
> est **globale à la session** — question 9 du même document.

### 6.2 Clients

- **UI** : envoie des requêtes, conserve le dernier `Snapshot`, peint
  depuis lui sans verrou. Une réponse déclenche
  `egui::Context::request_repaint()`.

> **Trou relevé par la phase 4 (2026-09-08) : rien ne prévient un client
> qu'un autre a modifié le modèle.** Le protocole n'a que des requêtes et
> leurs réponses ; il n'a **aucune notification poussée**. Une interface qui
> ne rafraîchit son instantané qu'après ses propres requêtes reste donc
> figée, sans le savoir, dès qu'un second client écrit — et le second
> client, c'est précisément le fil de script du §8.4, c'est-à-dire l'objet
> du socle. Le critère du §1.1 reste atteignable (la fenêtre rafraîchit
> après avoir lancé le script), mais un script qui écrit progressivement
> n'apparaîtrait pas.
>
> **Parade retenue pour le socle, sans changer le protocole** : l'interface
> demande un `Snapshot` à chaque image et compare la `revision` — c'est
> exactement ce pour quoi ce compteur existe, et le coût d'une réponse est
> de deux `Arc` clonés. C'est ce que la coquille de la phase 4 devra faire
> quand la zone de script arrivera (tâche 5.6).
>
> **Ce que la tâche 5.6 a effectivement fait, et sa limite (2026-09-09)** :
> le script tourne sur un fil détaché, la fenêtre sonde le résultat à chaque
> image et rafraîchit son instantané à la fin — donc la grille reflète bien
> ce qu'un script a écrit, sans action de l'utilisateur. Mais **cette parade
> suppose que des images se produisent**, et une interface en mode réactif
> n'en produit pas au repos. Elle tient parce que la fenêtre demande
> explicitement à se réveiller pendant qu'un script tourne. Elle ne
> couvrirait **pas** un client extérieur écrivant pendant que la fenêtre est
> inactive — cas qui n'existe pas dans le socle (un seul processus, un seul
> script à la fois), mais qu'il faudra reconsidérer si un jour un second
> processus parle à l'acteur.
>
> **Ce qu'il ne faut pas faire par réflexe** : ajouter un canal
> d'abonnement. Ce serait réintroduire l'abonné qui mute pendant qu'il est
> notifié — la classe de bug que l'acteur supprime par construction
> (`roadmap.md` §1.2-1.3). Si le sondage par image devenait trop cher, la
> forme à retenir est un canal **de révisions seules** (`u64`), jamais de
> données, que le client utilise pour décider de demander un instantané.
- **Script** : bloque sur la réponse, et **relâche le GIL pendant
  l'attente** (§8.2). C'est la subtilité la plus facile à rater du socle.

### 6.3 Invariant non négociable

**L'acteur ne touche jamais à Python.** Il ne détient jamais le GIL, ne
construit jamais d'objet Python, n'appelle jamais `plotyx-python`. C'est ce
qui rend impossible le cycle « le script tient le GIL et attend l'acteur,
l'acteur attend le GIL ». La frontière est vérifiable : `plotyx-session` ne
dépend pas de `plotyx-python` (§3).

#### 6.3.1 Le chemin caché — `Drop`

Le raisonnement ci-dessus était incomplet, complété le 2026-09-08. Une
requête ou une réponse qui transporterait un objet Python (`Py<…>`) serait
**détruite sur le thread de l'acteur** ; or détruire un objet Python
décrémente son compteur de références, ce qui est une opération Python.
Aucun appel visible à `plotyx-python`, et pourtant le GIL en jeu. D'où la
règle complémentaire, qui ferme l'invariant :

> **Aucun objet Python ne traverse le canal** — ni à l'aller, ni au retour,
> ni comme champ enfoui d'une commande.

C'est la **deuxième** justification de la copie à l'écriture (§8.3), et
elle est indépendante de la première : un `Vec<f64>` copié est du Rust pur,
il se détruit n'importe où. Deux garde-fous la tiennent sans discipline
humaine : `plotyx-session` ne dépend pas de `plotyx-python` (le type ne
serait même pas nommable), et le marqueur `Ungil` exigé par
`Python::detach` (§8.2) rejette **à la compilation** toute valeur liée au
GIL qui tenterait de franchir l'attente.

Les deux autres chemins ont été vérifiés et sont clos : les **erreurs**
(`CommandError` est un `thiserror` de `plotyx-commands`, traduit en
exception Python par `plotyx-script`, sur le thread de script, GIL en
main) et les **journaux** (l'acteur écrit via `tracing`, jamais via
`sys.stdout` ; la capture de `print` du §8.4 est locale au thread de
script).

---

## 7. Persistance — `plotyx-persistence`

Frontière **figée dès maintenant**, format volontairement provisoire :

```
fn save(project: &Project, path: &Path) -> Result<(), PersistenceError>;
fn load(path: &Path) -> Result<Project, PersistenceError>;
```

Une fonction par sens, un `Project` complet, rien d'autre. Aucun appelant
ne connaît le format — c'est ce qui permettra de remplacer le provisoire
(~~`serde` + `bincode`~~ **`serde` + MessagePack via `rmp-serde`**, voir
ci-dessous) par le Zarr de `format-fichier-plx.md` sans rien casser
au-dessus.

> **Correction de la phase 3 (2026-09-08) — pas `bincode`.** La propriété
> « champs inconnus ignorés à la lecture » exige un conteneur
> **auto-descriptif** (qui nomme ses champs) ; `bincode` est positionnel,
> donc un champ ajouté casse tout fichier antérieur. Retenu : MessagePack
> (`rmp-serde`, structures écrites comme des maps nommées), avec les
> buffers de colonnes en octets bruts little-endian (`serde_bytes`, 8 octets
> par `f64`, `NaN` préservés bit à bit). Disposition : 4 octets magiques
> `PLYX`, version `u32` LE, corps MessagePack. Tout champ ajouté après la
> version 1 porte `#[serde(default)]`.

Propriétés obligatoires **même en version provisoire**, reprises de
`format-fichier-plx.md` §3 — les rajouter après coup sur des fichiers déjà
écrits coûte une migration :

- écriture **atomique** (fichier temporaire + renommage) ;
- **numéro de version en premier**, lu avant tout le reste, refus propre
  d'un fichier plus récent que le lecteur ;
- distinction valeur manquante / chaîne vide préservée ;
- **valeur haute du compteur de `NodeId`** persistée (§4.1) ;
- champs inconnus **ignorés** à la lecture. C'est ce principe qui a permis
  au schéma HDF5 de ne pas bouger d'une ligne lors de l'ajout de
  Matplotlib ; il doit survivre au changement de langage, et c'est lui qui
  permettra d'ajouter les `PlotSpec` plus tard sans casser les fichiers du
  socle.

### 7.1 Le couplage caché : la forme du schéma, pas la signature

La revue du 2026-09-08 confirme que la **signature** est suffisante — aucun
appelant ne peut apprendre le format à travers elle. Deux couplages
subsistent pourtant sous la signature, et se ferment maintenant, pas au
moment de basculer sur Zarr :

1. **Le format ne doit pas s'écrire dans le modèle.** `plotyx-persistence`
   ne peut pas implémenter `Serialize` pour des types de `plotyx-core`
   (règle d'orphelin) : soit `plotyx-core` porte les dérivations `serde`,
   soit `plotyx-persistence` définit ses propres structures de transport.
   **Retenu : les structures de transport dans `plotyx-persistence`.** Le
   coût est une poignée de conversions à écrire ; le gain est que le schéma
   sur disque devient un objet explicite, versionné indépendamment des
   types en mémoire — et que le passage à Zarr, qui demande de toute façon
   une correspondance écrite à la main, ne rouvre pas `plotyx-core`.
   *Écarté — dérivations `serde` dans `plotyx-core`* : plus court à écrire,
   mais fait entrer dans le cœur des noms de champs qui deviennent un
   contrat de fichier, et rend toute renommée de champ interne
   silencieusement incompatible avec les fichiers déjà écrits.
2. **`load` rend tout, tout de suite.** La signature engage une sémantique
   de chargement **intégral en mémoire** — celle d'aujourd'hui côté Python,
   et celle du format provisoire. Elle est compatible avec Zarr comme
   conteneur, mais **elle n'expose pas** ce que Zarr apporte de propre
   (`format-fichier-plx.md` §2a) : lecture partielle d'une colonne à 1M de
   points, croissance chunk par chunk pour l'acquisition live. Ces usages
   demanderont une **entrée supplémentaire**, pas un changement de celle-ci.
   Ce n'est donc pas une dette : le socle assume le chargement intégral,
   et le jour où la lecture paresseuse devient un besoin, elle s'ajoute à
   côté. Consigné comme tel dans `questions-ouvertes.md` (question 4),
   parce que c'est le genre de limite qu'on redécouvre autrement au pire
   moment.

---

## 8. Le pont Rust ↔ Python — le cœur du socle

### 8.1 Propriété et cycle de vie

`plotyx-python` est le **seul** crate à initialiser l'interpréteur et le
seul à exposer l'accès au GIL.

- Interpréteur **vendored et figé** (`python-build-standalone`), wheels
  numpy épinglées, un jeu par plateforme. Pas de Python système, pas de
  `pip` exposé, interpréteur pointé sur le préfixe embarqué **et isolé de
  l'environnement de l'utilisateur** (§8.6.1 — la simple variable
  `PYTHONHOME` n'y suffit pas), auto-initialisation de PyO3 désactivée
  (feature `auto-initialize` non activée : c'est elle qui rend possible
  l'initialisation paresseuse ci-dessous).
- **Initialisation paresseuse, obligatoire.** L'interpréteur n'est chargé
  qu'au premier besoin réel (ouverture de la console). Une fenêtre et une
  feuille de calcul doivent s'ouvrir **sans** interpréteur : sans cette
  règle, le gain de temps de démarrage — l'un des motifs du portage — est
  annulé dès le lancement.
- **Pas de finalisation** de CPython à l'extinction : la finalisation est
  une source connue de plantages en sortie, et n'apporte rien à un
  processus qui se termine.

### 8.2 GIL — trois règles

**Noms d'API vérifiés sur PyO3 ≥ 0.25**, qui a renommé toute la famille GIL
en prévision du Python sans GIL (3.13 *free-threaded*), où « prendre le
GIL » n'a plus de sens universel. Les anciens noms restent appelables mais
sont **dépréciés** ; ce document écrit les noms actuels :

| Rôle | Nom actuel | Ancien nom, déprécié |
|---|---|---|
| Prendre le GIL le temps d'un bloc | `Python::attach` | `Python::with_gil` |
| **Relâcher** le GIL le temps d'un bloc | `Python::detach` | `Python::allow_threads` |
| Démarrer l'interpréteur | `Python::initialize` | `prepare_freethreaded_python` |

1. **Le thread UI ne touche jamais à Python.** Dans le socle, il n'en a
   aucun besoin ; l'y autoriser ferait dépendre la fluidité du rendu de la
   disponibilité du GIL.
2. **Le thread de script relâche le GIL pendant qu'il attend l'acteur**
   (`Python::detach`). Sans ça, il affame tout autre usage de Python
   pendant chaque aller-retour. La signature réelle est
   `fn detach<T, F>(self, f: F) -> T where F: Ungil + FnOnce() -> T, T: Ungil` :
   le marqueur `Ungil` **interdit à la compilation** qu'une valeur liée au
   GIL entre dans le bloc d'attente ou en ressorte. Cette règle n'est donc
   pas une discipline à tenir mais une erreur de compilation si on la
   manque — et elle vérifie au passage la règle « aucun objet Python sur le
   canal » du §6.3.1.
3. **L'acteur ne prend jamais le GIL** (§6.3) — la règle qui rend le
   deadlock structurellement impossible.

**Limite assumée, à documenter plutôt qu'à prétendre résoudre** : un script
exécutant une longue boucle Python pure garde le GIL. C'est inhérent à
CPython et déjà vrai aujourd'hui ; c'est pourquoi l'interruption est un
mécanisme à concevoir (§8.4), pas une case à cocher. À noter en renfort :
`Python::initialize` démarre l'interpréteur **avec la gestion des signaux
désactivée**. Aucun `KeyboardInterrupt` ne sera donc levé tout seul — c'est
souhaitable (pas de compétition avec la boucle d'événements de l'UI) mais
ça confirme que l'arrêt d'un script est **entièrement à notre charge**.

### 8.3 Tableaux — l'asymétrie est une décision

| Sens | Mécanisme | Copie ? |
|---|---|---|
| Rust → Python (lecture) | tableau numpy **empruntant** le `Arc<Vec<f64>>`, `writeable = False` | **non** |
| Python → Rust (écriture) | copie du tableau numpy dans un `Vec<f64>` neuf | **oui** |

La lecture sans copie est le livrable ; l'écriture avec copie est un
**choix**, pas une concession : le modèle doit posséder ses buffers, et
adopter de la mémoire allouée par l'allocateur Python ferait dépendre la
durée de vie d'une colonne de celle d'un objet Python.

> **Ce que la mesure de 5.1a change à l'argument (2026-09-08).** Copier un
> million de `f64` coûte **0,8 ms** dans un sens et **0,5 ms** dans l'autre
> (~10 Go/s). Face au budget de rendu de 141 ms d'une figure Matplotlib, la
> copie est du bruit : **le zéro-copie ne se justifie donc pas par le temps,
> mais par la mémoire** — chaque lecture dupliquerait la colonne, et un
> script qui lit dix colonnes de 8 Mo en dupliquerait 80. La décision ne
> change pas (elle est de toute façon contractuelle, cahier des charges API
> §2.3 : « une colonne lue renvoie une vue »), mais **l'argument à défendre
> n'est plus celui écrit en §2.1**. À chiffrer en Mo, pas en ms, le jour où
> quelqu'un voudra la rouvrir.
>
> **Amendement du 2026-09-10.** Le point de comparaison des 141 ms
> disparaît avec le renderer Matplotlib, et le zéro-copie perd un de ses
> deux consommateurs — il ne sert plus qu'aux scripts. **La décision ne
> bouge pas pour autant** : elle tenait déjà par la mémoire et par le
> contrat de l'API, deux arguments que le changement de renderer ne touche
> pas. Rien à défaire, c'est construit et testé.
>
> Piège d'implémentation relevé au passage : `to_vec()` **échoue sur un
> tableau numpy non contigu**. Dès qu'un script passe une vue tranchée
> (`v[::2]`) à `pyx.set_column`, il faut un chemin explicite de compactage,
> pas une conversion directe — à traiter en tâche 5.5. Le contrat de
`cahier-des-charges-api-script.md` §2.3 est respecté : la lecture est une
vue, l'écriture passe par une commande.

#### 8.3.1 Le mécanisme exact et son contrat de sûreté

Vérifié sur `rust-numpy` le 2026-09-08. La bibliothèque distingue bien les
constructeurs qui **copient** (`PyArray::from_slice`,
`ToPyArray::to_pyarray`) de celui qui **emprunte** un buffer Rust :

```
unsafe fn borrow_from_array<S>(array: &ArrayBase<S, D>,
                               container: Bound<'py, PyAny>) -> Bound<'py, Self>
```

Trois points, tous à traiter comme du contrat et non comme du détail :

- **La fonction est `unsafe`**, et sa clause est explicite : *le
  `container` ne doit pas être détruit avant le tableau numpy, et le buffer
  ne doit être ni déplacé ni réalloué tant que le `container` vit.*
- **Le `container` est un `#[pyclass]` qui détient un clone du
  `Arc<Vec<f64>>`.** Pas un pointeur, pas une référence empruntée : un
  `Arc` cloné. Il devient l'attribut `base` du tableau numpy et il est le
  seul lien qui maintienne le compte de références au-dessus de 1.
- **C'est ce clone qui rend l'écriture concurrente sûre**, via
  `Arc::make_mut` (§4.3) : tant qu'une vue existe, le compte vaut ≥ 2, donc
  toute écriture côté modèle **alloue un buffer neuf** et laisse l'ancien
  intact. Le buffer sous la vue n'est ni déplacé, ni réalloué, ni libéré —
  la clause ci-dessus est honorée par construction, pas par vigilance.

**Correction de rédaction (2026-09-08).** La version précédente disait que,
sans `base`, `Arc::make_mut` « libérerait le buffer sous la vue ». C'est
l'inverse : sans `base`, le compte reste à 1 et `make_mut` **écrit en
place**. Les deux fautes sont donc distinctes, et aucune n'est visible dans
un test qui se contente de relire des valeurs juste après l'écriture :

| Sans le `base` qui clone l'`Arc` | Ce qui se passe |
|---|---|
| écriture de cellules | `make_mut` mute en place → la vue Python lit un buffer modifié sous elle (course de données) |
| insertion de lignes | le `Vec` grandit → **réallocation** → la vue pointe sur de la mémoire libérée |
| suppression de la colonne | le `Vec` est détruit → même chose, en pire |

**Ce que la vue devient — à assumer, pas à corriger.** Une vue prise avant
une écriture reste sur l'**ancien** buffer : le script continue de lire des
valeurs périmées, silencieusement. C'est le comportement correct — une vue
est un instantané, pas un abonnement — mais c'est un piège d'usage à
documenter dans l'API de script.

**Test dédié, dans ces termes exacts** : prendre une vue, écrire dans la
colonne depuis l'acteur, puis vérifier que **(a)** la vue relit sans
planter les **anciennes** valeurs, **(b)** un nouvel appel `pyx.column()`
rend les nouvelles, **(c)** insérer des lignes pendant que la vue vit ne
plante pas, **(d)** le tout sous **`valgrind` ou AddressSanitizer** — un
usage après libération ne s'observe pas de façon fiable par des assertions,
il se voit à l'instrumentation. À noter : **`miri` n'est pas utilisable
ici** (il n'exécute pas d'appels FFI, donc rien qui traverse PyO3) ; il
reste l'outil du reste du modèle, où tout est du Rust pur.

> **Résultat de la tâche 5.2 (2026-09-08).** Le mécanisme est construit et
> vérifié par 19 tests. Points acquis, à ne pas rejouer :
>
> - **l'emprunt est réel** : l'adresse du buffer que numpy expose
>   (`__array_interface__["data"][0]`) est **exactement** celle du `Vec`
>   Rust. C'est le test décisif ; sans lui, une copie silencieuse passerait
>   inaperçue derrière des valeurs justes ;
> - (a), (b), (c) et la **survie au `drop` côté Rust** passent : le
>   conteneur détient bien la dernière référence ;
> - le **compteur de références est déterministe** (1 avant, ≥ 2 pendant,
>   1 après destruction de la vue) — `FloatBufferOwner` ne forme aucun
>   cycle, donc le comptage immédiat de CPython suffit, sans dépendre du
>   ramasse-miettes ;
> - **la copie de retour gère les tableaux non contigus** : une tranche
>   `v[::2]` passe par un chemin de compactage explicite. C'était le piège
>   annoncé plus haut ;
> - **mesures** : emprunt **≈ 5 à 9 µs**, copie **≈ 2 ms** pour un million
>   de flottants — un rapport de 240 à 370. (La tâche 5.1a mesurait la copie
>   à 0,8 ms depuis le binaire empaqueté ; l'écart tient à la méthode de
>   mesure, pas à un changement de mécanisme. Dans les deux cas la
>   conclusion du §8.3 tient : la copie est du bruit devant les 141 ms d'un
>   rendu, et **c'est la mémoire, pas le temps, qui justifie le
>   zéro-copie**.)
>
> **Le point (d) a été exécuté le 2026-09-09** — `valgrind` installé, puis
> `scripts/python-memcheck.sh` (avec `PYTHONMALLOC=malloc`, sans
> `--leak-check`, en cherchant les lectures et écritures invalides) :
> **19 tests, `ERROR SUMMARY: 0 errors from 0 contexts`**. Aucune lecture ni
> écriture invalide. La sûreté mémoire du pont ne repose donc plus sur le
> seul raisonnement — ce que ce paragraphe demandait précisément d'éviter.
> Les allocations « encore joignables » relevées par l'outil (quelques
> mégaoctets) sont les objets internes de CPython, attendues puisqu'on
> n'appelle jamais `Py_Finalize` (§8.1) ; « definitely lost » est à zéro.
>
> **Angle mort relevé pour la surface de script (5.5)** : deux vues prises
> de part et d'autre d'une écriture pointent silencieusement sur deux
> buffers aux valeurs différentes. Rien ne signale à un script qu'il compare
> des instantanés désynchronisés. C'est la politique retenue, mais le risque
> croît avec le nombre de vues qu'un script accumule — argument de plus pour
> la règle « ne gardez pas une vue plus longtemps qu'un calcul ».

**`writeable = False` est un garde-fou, pas une frontière.** Il attrape la
faute d'inattention (`col[0] = 3` dans un script) et documente l'intention.
Il ne résiste pas à un script déterminé, et n'a pas à le faire : le modèle
de confiance du `cahier-des-charges-api-script.md` §6 donne déjà les pleins
pouvoirs à l'interpréteur.

### 8.4 Thread de script

- **Un script à la fois** dans le socle ; pas de scripts concurrents.
- **Un seul fil, créé une fois pour la vie du processus** — précision
  apportée par la tâche 5.1a le 2026-09-08. Le fil qui initialise
  l'interpréteur devient le **fil principal** de CPython ; comme le fil UI
  ne touche jamais à Python (§8.2 règle 1), c'est nécessairement celui-ci.
  Un fil créé *par exécution de script* ferait donc mourir le fil principal
  de CPython avec le premier script. Le fil de script est un **service
  permanent** qui attend du travail, pas un fil jetable.
- `print()` et tracebacks capturés et rendus au client, jamais perdus dans
  la sortie du processus.
- **Interruption coopérative** : un drapeau vérifié par les appels de
  l'API, plus un hook de trace pour le code Python pur. Le mécanisme est à
  concevoir ici, dans le socle — pas au moment de la console complète : il
  conditionne la forme du thread.

### 8.5 Surface-témoin — quatre appels, pas la v1

Un interpréteur qui démarre ne prouve rien. La surface minimale qui
traverse **tout** le pont :

| Appel | Ce qu'il éprouve |
|---|---|
| `pyx.resolve(path)` | clés opaques, grammaire de chemins |
| `pyx.column(handle)` | vue numpy sans copie, `Arc` maintenu vivant (§8.3) |
| `pyx.set_column(handle, values)` | mutation via l'acteur et le bus, GIL relâché pendant l'attente |
| `pyx.undo()` | délégation à la pile d'historique depuis Python |

C'est **explicitement pas** la surface v1 de
`cahier-des-charges-api-script.md`, qui reste le contrat pour plus tard,
inchangé. Ces quatre-là ne sont pas choisis pour leur utilité mais parce
qu'ensemble elles rendent le critère du §1.1 vérifiable.

Vocabulaire d'erreurs limité à ce qu'elles peuvent lever :
`UnknownPathError`, `BrokenReferenceError`, `InvalidValueError` — noms
repris tels quels du cahier des charges API, pour ne pas avoir à les
renommer ensuite.

**Trou identifié le 2026-09-08** : aucune des trois ne couvre `pyx.undo()`
sur une pile vide, et le cahier des charges API §7 n'en prévoit pas
davantage. Une pile vide n'est pas une erreur — c'est le cas que l'UI
traduit par un bouton grisé. `questions-ouvertes.md`, question 2, à
trancher avec la tâche 5.5.

### 8.6 L'empaquetage se prouve en premier — sur Linux d'abord

**Exigence de séquencement, pas de confort.** Avant tout autre code du
pont, un *hello-world* doit être empaqueté et lancé **sur Linux** : binaire
Rust + interpréteur vendored + numpy, avec un aller simple et un retour de
tableau mesurés. **Le même hello-world sur Windows est un jalon distinct,
volontairement après** (§1.1) — pas sauté, pas simultané.

C'est l'application du « point de vigilance n°3 » du roadmap Python (le
packaging se valide maintenant, pas à la fin) au risque le plus lourd de
cette architecture. Découvrir un blocage d'empaquetage après avoir écrit
tout le pont coûterait le pont — resserrer d'abord sur Linux limite ce
risque à une plateforme pendant que le reste du pont s'écrit, au lieu de le
porter en double dès le premier jour.

#### 8.6.1 Pièges connus, à traiter dans le hello-world

Recensés le 2026-09-08 : ils sont documentés, reproductibles, et aucun
n'est rédhibitoire — mais chacun est capable de coûter une journée s'il est
découvert sans être attendu. Le hello-world de 5.1 doit les traverser tous.
**La colonne Windows reste dans ce tableau pour ne pas être redécouverte
au moment du portage** — elle ne conditionne rien avant cette étape-là.

| # | Piège | Ce qu'il faut faire (Linux, maintenant) | Ce qui attend (Windows, plus tard) |
|---|---|---|---|
| 1 | **PyO3 ne trouve pas `libpython` à la compilation** : le `sysconfig` livré par `python-build-standalone` n'est pas celui d'une installation ordinaire, et la détection automatique de PyO3 échoue dessus | fournir un `PYO3_CONFIG_FILE` écrit à la main (version, `lib_dir`, `lib_name`, `pointer_width`), figé dans le dépôt et pointé par le script de build | même mécanisme, valeurs Windows du build vendored à substituer |
| 2 | **Le binaire ne trouve pas `libpython` à l'exécution.** | `libpython3.x.so.1.0` doit être trouvable : `-C link-arg=-Wl,-rpath,$ORIGIN/lib` — un `rpath` relatif, **pas** un `LD_LIBRARY_PATH` dans un script de lancement | `python3x.dll` doit être à côté de l'exécutable |
| 3 | **Liaison statique de `libpython`** : elle paraît plus propre, mais les modules d'extension compilés (numpy en tête) attendent des symboles exportés par une `libpython` **partagée** | prendre la distribution partagée (`install_only`) et l'embarquer, ne pas chercher le binaire unique | identique |
| 4 | **`PYTHONHOME` seul n'isole rien.** Il désigne le préfixe, mais laisse entrer `PYTHONPATH`, le `site-packages` utilisateur (`~/.local/lib/python3.x/…`) et les `.pth` du système — un utilisateur ayant un autre numpy installé peut le voir masquer le nôtre | initialiser en **mode isolé** (l'équivalent de `python -I`) via `PyConfig` : `isolated = 1`, `home` et `module_search_paths` renseignés explicitement | même config, chemins Windows |
| 5 | **`Py_SetPythonHome` / `Py_SetProgramName` sont dépréciés depuis CPython 3.11 et supprimés en 3.13** ; et depuis 3.11, l'initialisation par `PyConfig` **ne lit plus `PYTHONPATH` par défaut** | passer par `Py_InitializeFromConfig` + `PyConfig` (accessible via `pyo3::ffi`), pas par les anciens setters ni par des variables d'environnement posées avant l'init | identique |
| 6 | **numpy est lié à une version d'interpréteur précise** (ABI CPython, et ABI numpy 1.x vs 2.x) | figer ensemble, dans un même verrou : version de l'interpréteur, roues numpy, version de `rust-numpy` | même verrou, roues Windows en plus |
| 7 | **Pas de `Py_Finalize`** (§8.1) — conséquence à connaître : les `atexit` Python ne tournent pas, les fichiers temporaires créés par une future pile Matplotlib ne sont pas nettoyés à la sortie | acceptable ici ; à revoir seulement si un jour du code Python doit écrire quelque chose en sortant | identique |
| **8** | **Le fil qui initialise garde le GIL.** Après `Py_InitializeFromConfig`, le fil appelant **détient** le GIL. Sans le relâcher, tout `Python::attach` depuis un autre fil se bloque — invisible en mono-fil, mortel dès qu'il y a un fil de script (§8.4) | appeler `PyEval_SaveThread()` **immédiatement après** l'initialisation | identique |
| **9** | **Ne pas supprimer les `__pycache__`** de la distribution pour gagner de la place : la stdlib se recompile alors à chaque démarrage (init mesurée à **250 ms** au lieu de **50 ms**), et une installation en lecture seule ne peut même pas reconstituer le cache | garder les `.pyc` livrés ; retirer plutôt `bin/` et `pip` | identique |

> **Ajoutés le 2026-09-08 par la tâche 5.1a**, qui a traversé les neuf.
> Deux constats de la même tâche, à ne pas redécouvrir :
> **PyO3 0.29 n'offre aucune API sûre** pour une configuration
> d'initialisation personnalisée — `Python::initialize()` appelle
> `Py_InitializeEx(0)`, qui lit l'environnement, donc l'isolation du piège 4
> **oblige** à passer par `pyo3::ffi`, dans deux fonctions `unsafe` confinées ;
> et le fil qui initialise devient le **fil principal** de CPython, ce qui
> impose que ce soit le fil de script (§8.4), créé **une fois pour la vie du
> processus** et non par exécution de script.
>
> **Mesures Linux (release, environnement nettoyé)** : démarrage de
> l'interpréteur avec import de numpy **≈ 51 ms** ; copie d'un million de
> `f64` Rust → numpy **≈ 0,8 ms** ; retour numpy → `Vec<f64>` **≈ 0,5 ms** ;
> distribution complète **307 Mo**, dont **209 Mo** de `libpython` non
> *strippée* — ramenés à **128 Mo** (hello-world) et **155 Mo**
> (application) par le passage à `install_only_stripped` le 2026-09-09
> (`questions-ouvertes.md`, question 10, tranchée).
>
> **Contrôle instrumenté, exécuté le 2026-09-09** : `valgrind` sur toute la
> suite du zéro-copie — **19 tests, `ERROR SUMMARY: 0 errors from 0
> contexts`**, aucune lecture ni écriture invalide. La réserve du §8.3.1 est
> levée : la sûreté mémoire du pont ne repose plus sur le seul raisonnement.
> Le test qui compare la vitesse de l'emprunt à celle de la copie se tait
> sous instrumentation (`valgrind` ralentit inégalement : rapport apparent
> de 9 contre 46 en exécution ordinaire) ; l'assertion reste active hors
> instrumentation, où elle protège le zéro-copie contre une régression
> silencieuse en copie.

**Non couvert volontairement** : macOS, comme Windows au sens de « pas
maintenant ». Le piège spécifique connu côté macOS (l'`install_name` des
bibliothèques `python-build-standalone` ne contient pas `@rpath`, ce qui
rend le `rpath` de l'exécutable inopérant) est noté ici pour ne pas être
redécouvert le jour où macOS entrerait dans le périmètre — aucune cible
macOS n'est prévue pour l'instant.

---

## 9. Coquille UI minimale — `plotyx-app`

Banc d'essai, **pas** une ébauche de l'UI finale : elle sera reprise
entièrement en conversation, et rien ici ne doit la préempter.

- fenêtre `eframe`/`egui`, grille brute affichant une `Worksheet` ;
- **Ctrl+Z** (annuler), **Ctrl+Y** et **Ctrl+Shift+Z** (rétablir — les deux
  conventions), **Ctrl+S** / **Ctrl+O** ;
- état des actions à la main : grisé quand la pile est vide, libellé
  « Annuler *Renommer* » depuis le `label` de l'entrée. Rien de cela n'est
  fourni par un framework en Rust, contrairement à `QUndoStack` +
  `QAction` qui le tenaient à jour tout seuls ;
- une zone de saisie de script minimale, suffisante pour exercer §8.5.

> **Défaut d'environnement relevé le 2026-09-09, à ne pas rediagnostiquer.**
> Sous le **Wayland natif** de cette machine (GNOME/Mutter), un réveil
> différé (`egui::Context::request_repaint_after`) **ne réveille jamais** la
> boucle d'événements : le processus reste dans `ep_poll` indéfiniment. Le
> même binaire sous **Xwayland** (`DISPLAY` posé, `WAYLAND_DISPLAY` retiré)
> s'ouvre et se ferme normalement. Vérifié des deux côtés, et vérifié aussi
> que le code applicatif demande bien ce réveil : **le défaut est en amont**,
> dans `winit`/`eframe`, pas chez nous.
>
> Portée réelle : cela n'affecte que ce qui dépend d'une **échéance** sans
> entrée utilisateur — donc le mécanisme d'auto-fermeture qui sert à prouver
> en ligne de commande que la fenêtre s'ouvre, et tout futur rafraîchissement
> périodique (§6.2). Les scripts d'empaquetage lancent donc la vérification
> sous Xwayland. À rejouer au moment du chantier de l'UI réelle : si le
> défaut persiste, un rafraîchissement périodique ne sera pas une option
> sous Wayland natif, et il faudra le réveil explicite plutôt que l'échéance.

---

## 10. Conventions transverses

**Erreurs.** `thiserror` et un type nommé par crate ; `anyhow` uniquement
dans `plotyx-app`. Aucun `unwrap()` ni `panic!` dans les bibliothèques hors
invariant documenté — lint clippy activé.

**Tests.**

- Propriété d'undo : N commandes aléatoires → tout annuler → égalité avec
  l'état initial, via `proptest` (qui réduit automatiquement un
  contre-exemple). C'est le filet qui rattrape la moitié « delta »
  conservée par l'hybride.
- Cas hors de portée de ce test, à couvrir explicitement : abandon de macro
  à mi-chemin, redo après annulation partielle, éviction par la limite
  d'historique, macro mixte (§5.3).
- Aller-retour de persistance (§7).
- Durée de vie d'une vue numpy sous écriture concurrente (§8.3).
- Le critère du §1.1, en test d'intégration bout en bout.
- Garde-fou de dépendances déjà en place
  (`crates/plotyx-core/tests/no_ui_dependency.rs`), à étendre aux autres
  crates non-Python.
- **La CI doit rester verte sans Python installé** : tout ce qui touche à
  PyO3 derrière un feature flag.

**Langue.** Identifiants en anglais, commentaires et documentation en
français, messages utilisateur en français — comme le code Python actuel.

**Budgets.** Ouverture d'une fenêtre et d'une feuille **sans**
initialisation de l'interpréteur (§8.1). Les budgets de rendu hérités de la
phase 6 (141 ms pour 1M points, 0,2 ms pour le réticule) ne concernent pas
ce socle : ils reviendront avec le système de graphes.

---

## 11. Séquencement

`roadmap.md` détaille les phases 0 à 5. Ordre imposé par ce document :

| Phase | Contenu | Verrou |
|---|---|---|
| 0 | Squelette Cargo, CI | fait |
| 1 | Modèle (§4) | **fait le 2026-09-08** — `plotyx-core`, 116+ tests, zéro dépendance hors `thiserror` |
| 2 | Historique + acteur (§5, §6) | après 1 |
| 3 | Persistance (§7) | après 1 ; valide le modèle par aller-retour disque |
| 4 | Coquille UI (§9) | après 2 et 3 |
| 5 | **Pont Python (§8)** | empaquetage (§8.6) et zéro-copie (§8.3) **avant** le reste de la phase |

La phase 5 peut démarrer en parallèle de 3-4 : `plotyx-python` ne dépend
d'aucun des deux. Seule sa surface-témoin (§8.5) attend l'acteur.

---

## 12. Hors périmètre — décisions déjà prises pour la suite

Ces chantiers ne sont pas dans le socle, mais leurs décisions sont
arrêtées : les rouvrir demanderait une raison nouvelle.

- **Système de graphes** : `PlotSpec` neutre (port de `plotyx.plot`), rendu
  par **Plotters** avec un backend `DrawingBackend` **écrit par nous** sur
  egui — décision du 2026-09-10, `docs/newarchitecture/`. Trois crates :
  `plotyx-plot` (spec neutre, sans dépendance de dessin), `plotyx-render-egui`
  (backend + cache), `plotyx-render-svg` (export SVG, puis PDF via
  `svg2pdf`). Le backend collecte dans un `Vec<Shape>` — **pas** dans un
  `Painter` — et met en cache sur la clé `(revision, rect,
  pixels_per_point)` : le renderer devient un client de plus du protocole
  du §6.1. Deux points de vigilance, qui remplacent ceux du plan
  Matplotlib : `estimate_text_size` doit mesurer par le même chemin que
  celui qui dessine — **une seule `epaint::Fonts` partagée par les deux
  backends**, constructible sans `Context` donc utilisable en export
  headless — sous peine d'axes mal placés **sans erreur** ; et le cache
  économise le pipeline Plotters, pas la tessellation egui (mesurer avant
  d'ajouter un LTTB).
  > *Écarté le 2026-09-10 — **Matplotlib embarqué**, sortie
  > `FigureCanvasAgg.buffer_rgba()` en texture egui. Ses deux points de
  > vigilance (budget de 141 ms adossé au zéro-copie du §8.3 ; réticule à
  > 0,2 ms tenable seulement par mise à jour **partielle** de la texture)
  > disparaissent avec lui, faute de texture. Ce qu'on perd en échange —
  > échelles log, placement des ticks, légendes, export mature, trente ans
  > de cas limites — est la vraie facture de la bascule, et elle est
  > assumée : `docs/newarchitecture/options-explorees.md`.*
- **Surface `plotyx.api` v1** : `cahier-des-charges-api-script.md`, sans
  modification. La surface-témoin du §8.5 en est un sous-ensemble strict.
- **Format `.plx` final** : Zarr (`zarrs`), `format-fichier-plx.md`.
- **Import de fichiers** : les 10 fixtures de
  `legacy-python/tests/fixtures/import/` sont la spécification.
- **Export ponctuel par sous-processus** : écarté pour la console (§2.3),
  mais reste pertinent pour un export lourd — la raison du rejet (latence
  par appel) ne s'applique pas à une opération unique.

---

## 13. Risques et questions ouvertes

**Risques, par gravité décroissante.**

1. **Empaquetage de la pile Python par plateforme** — le seul dont l'échec
   invaliderait l'architecture. Mitigé par §8.6, pas éliminé. Le
   resserrement du 2026-09-08 (Linux d'abord, §1.1) réduit l'exposition
   pendant le développement du pont, mais **ne réduit pas le risque
   lui-même** : le hello-world Windows reste à traverser avant de pouvoir
   dire l'architecture prouvée cross-platform.
2. **Zéro-copie et durée de vie sous vue numpy** (§8.3) — le contrat central
   du socle. À mesurer et à tester, pas à supposer.
3. **Ordre d'annulation dans les macros mixtes** (§5.3) — la seule
   fragilité conceptuelle introduite par l'undo hybride.
4. **Interruption d'un script en boucle Python pure** (§8.2) — limite
   connue de CPython ; le mécanisme du §8.4 la contient sans la supprimer.

**Décisions différées sans risque pour le socle** — tranchées comme
« pas ici », pas laissées ouvertes :

- **Coalescence des éditions** : explicitement écartée du socle, cf. §5.2.
- **Mode « visualiseur léger »** (ex-PyQtGraph, garder ou abandonner) :
  décidé le 2026-09-08 de reporter ce choix au chantier de l'UI réelle,
  sans risque pour le socle — aucun crate ci-dessus n'en dépend, et son
  seul argument différenciant (`useOpenGL`) n'est de toute façon toujours
  activé nulle part aujourd'hui (`resume-decisions.md` §8). À trancher
  quand l'UI se construira, pas avant.

**Questions non tranchées** : rassemblées dans `questions-ouvertes.md`,
rédigé pour être lu **sans connaissance de Rust** (il s'ouvre sur un
lexique Rust → Python). Aucune ne rouvre les quatre décisions du §2, mais
chacune finira par se poser :

| # | Question | Où | À trancher |
|---|---|---|---|
| 1 | « UUID » du cahier API face au `NodeId(u64)` | §4.1 | avant `plotyx.api` v1 |
| 2 | `pyx.undo()` sur une pile vide | §8.5 | tâche 5.5 |
| 3 | Ce qu'on dit d'une vue numpy devenue périmée | §8.3.1 | tâche 5.2 |
| 4 | Lecture partielle, non exposée par la frontière | §7.1 | quand l'acquisition live devient un besoin |
| 5 | Coalescence des éditions — critères recensés | §5.2 | chantier de l'UI réelle |
| 6 | Sort du mode « visualiseur léger » | ci-dessus | chantier de l'UI réelle |
| 7 | **Valeur** de la limite d'historique, en entrées ou en octets | §5.4 | phase 2 |
| 8 | Calendrier réel, toujours non chiffré | — | avant engagement |
