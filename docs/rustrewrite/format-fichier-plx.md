# Le format `.plx` (HDF5) est-il encore le bon choix pour un cœur Rust ?

Question posée explicitement sans contrainte de rétrocompatibilité — la
réponse serait différente s'il fallait garantir qu'un `.plx` écrit par la
version Python actuelle s'ouvre tel quel côté Rust (voir §5).

**Avis : non, à reconsidérer.** HDF5 était un choix cohérent pour un cœur
Python/numpy (`h5py` est une interface naturelle). Dans un cœur Rust, il
réintroduit exactement la tension déjà nommée dans
`resume-decisions.md` pour Matplotlib — sauf qu'ici, ce n'est pas une
brique optionnelle isolée à la frontière PyO3, c'est la couche de
persistance, traversée à **chaque** ouverture et sauvegarde.

## 1. Pourquoi HDF5 tire vers l'arrière, spécifiquement en Rust

- **Le crate `hdf5` est un lien FFI vers `libhdf5`, pas du Rust natif.**
  Ça remet sur la table exactement le problème qu'on cherchait à éviter en
  quittant Python pour la stabilité de déploiement : une bibliothèque C
  tierce à builder et embarquer pour trois plateformes, avec ses propres
  aléas de compilation (`libhdf5` a une réputation connue de build capricieux
  hors des gestionnaires de paquets système — CMake, dépendances zlib/szip
  optionnelles selon les fonctionnalités activées à la compilation).
- **Thread-safety historique fragile.** `libhdf5` n'est thread-safe que si
  compilée avec l'option dédiée (`--enable-threadsafe`), et cette variante
  sérialise de toute façon les accès par un verrou global — aucun gain de
  parallélisme réel, juste l'absence de plantage. Pas rédhibitoire pour un
  usage mono-fenêtre séquentiel comme aujourd'hui, mais un piège classique
  si un jour le chargement passe sur un thread de fond pendant qu'une
  autre partie du code touche encore le fichier.
- **Verrouillage de fichier capricieux sur certains systèmes de fichiers**
  réseau — non bloquant pour un outil desktop local, mais une source de bugs
  difficiles à diagnostiquer si jamais un utilisateur garde son dossier de
  projets sur un partage réseau.
- **HDF5 n'est même pas exploité pour ce qu'il apporte de spécifique.**
  Voir `writer.py` : l'écriture actuelle est déjà une **réécriture complète**
  à chaque sauvegarde (fichier temporaire + `Path.replace` atomique), jamais
  une mutation incrémentale in-place. Le vrai argument d'HDF5 — modifier un
  dataset au milieu d'un gros fichier sans tout réécrire — n'est donc **pas
  utilisé** par PlotYx aujourd'hui. L'abandonner ne fait perdre aucune
  capacité réellement en service.

## 2. Alternatives évaluées

### a) Zarr — **retenu, remplace la proposition initiale ci-dessous**

Format ouvert déjà établi en calcul scientifique — pas un standard
équivalent à HDF5 en ancienneté, mais un standard **émergent qui remplace
HDF5** dans plusieurs communautés précisément pour les raisons listées au
§1 (imagerie bio/microscopie via OME-Zarr, climat/géospatial via
xarray/dask, cloud-natif). Spécification V3 stabilisée, implémentation Rust
pure et activement maintenue : le crate **`zarrs`** — pas de FFI vers une
bibliothèque C, contrairement à `hdf5`.

Pourquoi c'est objectivement mieux que le conteneur maison proposé plus
bas, sur trois axes :

- **La hiérarchie de groupes est native**, pas à réinventer dans un
  manifeste JSON à part. Un groupe Zarr peut contenir des sous-groupes et
  des tableaux, chacun avec ses propres attributs JSON arbitraires
  (`zarr.json`) — le mapping vers le schéma actuel est quasi mécanique :
  `Folder`/`Workbook`/`Worksheet` → groupes imbriqués, `Column` → tableau,
  tous les attributs déjà listés (`key`, `name`, `type`, `plot_spec_json`,
  etc.) portés tels quels par les attributs du nœud Zarr correspondant.
  C'est *plus proche* du schéma HDF5 existant que mon idée de manifeste
  plat + fichiers séparés — le portage du **design** devient presque
  mécanique, pas une redécouverte.
- **Le chunking résout trois problèmes à la fois**, qu'un fichier binaire
  brut par colonne ne résout pas :
  - lecture partielle d'une colonne à 1M de points sans tout charger (utile
    pour un aperçu/une pagination de la feuille de calcul) ;
  - **croissance incrémentale** — une colonne qui s'allonge (acquisition
    live, cf. le mode « visualiseur léger » PyQtGraph) s'étend chunk par
    chunk, sans réécriture complète comme l'exige un fichier binaire plat ;
  - compression **par chunk**, codecs pluggables (`blosc`, `zstd`, `gzip`)
    déjà implémentés en Rust pur dans `zarrs` — au moins équivalent au gain
    mesuré aujourd'hui (gzip niveau 4 + shuffle, ~40 %), à re-mesurer avec
    `zstd` qui bat généralement gzip à ratio de compression égal pour un
    coût CPU moindre.
- **Le codec de sharding** regroupe de nombreux petits chunks dans un seul
  fichier physique — évite le problème pratique d'« un fichier par colonne »
  qui deviendrait des centaines de petits fichiers sur un classeur chargé.

**Fichier unique malgré tout** : Zarr a un mode *zip store* — toute la
hiérarchie de groupes/tableaux/chunks vit dans une seule archive zip, donc
`.plx` reste un fichier unique à l'utilisateur, exactement comme
aujourd'hui, tout en suivant une spec ouverte en dessous.

**Bonus d'interopérabilité, sans en payer le prix** : n'importe quel outil
Zarr existant (`zarr-python`, `xarray`, visionneuses OME-Zarr) pourrait lire
les tableaux bruts si un jour l'export/interop scientifique devient un
besoin réel — sans que PlotYx lui-même ait à lier la moindre bibliothèque
tierce en plus de `zarrs`, contrairement au piège HDF5 initial.

### b) Conteneur maison : zip + manifeste JSON + un fichier par colonne — idée initiale, supplantée par (a)

Une archive zip (crate `zip`, pur Rust — pas de dépendance C) contenant :

```
manifest.json     ← l'arbre entier : structure, métadonnées, plot_spec_json,
                     children_order, clés — exactement le même contenu
                     qu'aujourd'hui, juste en JSON plutôt qu'en attributs HDF5
columns/<key>.bin ← un fichier binaire par colonne : float64 little-endian
                     brut pour numérique/datetime, ou un petit en-tête +
                     values/missing pour le texte (même distinction
                     valeur-manquante-vs-chaîne-vide qu'aujourd'hui)
```

Avantages concrets :
- **Zéro dépendance C lourde** — `zip` + `serde_json` suffisent, tout le
  reste (structure, versionnement) est du Rust applicatif ordinaire.
- **Inspectable sans outil dédié** : un `.plx` devient un zip qu'on peut
  ouvrir avec n'importe quel gestionnaire d'archives pour déboguer — HDF5
  ne donnait pas ça sans `h5dump`/`HDFView`.
- Compression : `zip`/`deflate` nativement disponible ; probablement
  inférieur au gain mesuré aujourd'hui (gzip niveau 4 + `shuffle`, ~40 % sur
  un signal spectroscopique synthétique) — à re-mesurer, pas à supposer
  équivalent. `shuffle` (réordonnancement d'octets par colonne de type
  fixe) reste applicable manuellement avant compression si le gain compte.
- Aucune bibliothèque scientifique C à porter derrière soi — le format de
  fichier ne dépend plus, à terme, d'un composant qu'il faudrait
  potentiellement embarquer en plus du Python figé pour Matplotlib.

### c) Apache Arrow / Parquet (`arrow-rs`, `parquet`) — écarté pour v1

Écosystème Rust réellement natif et mature, typage colonnaire proche de ce
que fait déjà PlotYx. Mais Parquet ne modélise pas nativement une hiérarchie
de groupes (un fichier Parquet = une table) — il faudrait soit un fichier
par feuille/colonne plus un manifeste par-dessus (retombe sur la même forme
que l'option a, avec un format de colonne plus complexe pour un gain
principalement d'interopérabilité avec l'écosystème data science). Ce n'est
pas un besoin exprimé aujourd'hui par PlotYx — à garder en tête si
l'export/import vers pandas/Polars devient un jour un objectif produit
explicite, pas par anticipation.

## 3. Ce qui se garde à l'identique, indépendamment du conteneur choisi

Le contenu conceptuel du schéma actuel reste juste — c'est le contenant
qui change, pas le modèle :

- l'arborescence `Folder → Workbook → Worksheet → Column`, `Graph`/`Matrix`/
  `Note` en feuilles ;
- les **clés opaques UUID**, stables indépendamment du nom/de la position ;
- `plot_spec_json` **opaque**, avec désérialisation qui ignore les champs
  inconnus — le principe qui a permis au schéma HDF5 de ne pas bouger d'une
  ligne en ajoutant Matplotlib doit survivre au changement de conteneur ;
- l'écriture **atomique** (temporaire + renommage) ;
- le **numéro de version en premier**, lu avant tout le reste, avec refus
  propre sur un fichier futur ;
- colonnes de **longueur propre, sans padding** ;
- la distinction **valeur manquante vs chaîne vide** pour le texte.

## 4. Ce qui change

- Pas de dépendance à `libhdf5` à builder/embarquer par plateforme —
  `zarrs` est du Rust pur.
- On quitte un format scientifique ancien (HDF5) pour un format
  scientifique plus récent mais déjà établi (Zarr) — contrairement à un
  conteneur maison, ce n'est **pas** un compromis « on perd la
  reconnaissance d'un format standard » : on la garde, avec un écosystème
  plus jeune mais en croissance active (notamment parce qu'il corrige
  précisément les défauts d'HDF5 listés au §1).
- Le fichier reste inspectable (mode zip store) et l'arbre de groupes reste
  la même forme conceptuelle qu'aujourd'hui — le changement de conteneur
  ne redessine pas le schéma, contrairement à ce qu'aurait exigé un
  manifeste maison plat.

## 5. Rétrocompatibilité — non requise, mais à ne pas fermer inutilement

Le fait de ne pas exiger de rétrocompatibilité ne veut pas dire qu'aucun
utilisateur n'aura de `.plx` HDF5 existant le jour où un cœur Rust
existerait. Pas la peine de maintenir un lecteur HDF5 permanent côté Rust
pour autant — un **script de conversion ponctuel** (Python, lu avec
`h5py`, réécrit dans le nouveau conteneur) suffit, à écrire seulement si et
quand ce besoin se matérialise. Ne pas le construire par anticipation.
