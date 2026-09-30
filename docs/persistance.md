# Persistance — le format `.plx`

## Frontière figée, format provisoire

Deux fonctions, un sens chacune, un `Project` complet en entrée/sortie et rien
d'autre :

```rust
save(project: &Project, path: &Path) -> PersistenceResult<()>;
load(path: &Path)                    -> PersistenceResult<Project>;
```

Aucun appelant ne connaît le format sur disque. Il sera remplacé plus tard
(Zarr est envisagé) sans que rien au-dessus n'ait à changer. Le chargement est
**intégral** : tout le projet tient en mémoire, il n'y a pas de chargement
partiel ni différé.

La session utilise les variantes `save_with_geometries` et
`load_with_geometries`, qui transportent aussi une table
`NodeId -> WindowGeometry` (position et taille en points logiques). Ce bloc
`window_geometries` appartient au même fichier `.plx` ou `.plotyx`, mais reste
séparé de l'arbre annulable. Un ancien fichier sans ce bloc se charge avec
une table vide. L'application transmet les changements de géométrie à
l'acteur et recharge cette table aussi bien depuis le menu Fichier qu'au
démarrage avec un chemin de projet.

Chaque `WindowGeometry` porte aussi, en option, un `WindowView` (issue #16) :
mode `Open`/`Maximized`/`Closed`, repli, rang d'empilement parmi les fenêtres
ouvertes. `x`/`y`/`width`/`height` y restent toujours la géométrie
**flottante** ; le détachement n'est jamais enregistré (une fenêtre détachée
est écrite `Open`) et la position d'une fenêtre OS détachée non plus. `view`
absent = disposition initiale par défaut ; un mode inconnu se lit `Closed`.

## Le format ne s'écrit pas dans le modèle

`plotyx-core` ne dérive aucun `Serialize`/`Deserialize`. Ce crate définit ses
**propres structures de transport** (`schema.rs`, privées au crate) et écrit à
la main les conversions vers/depuis le modèle (`convert.rs`).

Le schéma sur disque est donc un objet explicite, versionné indépendamment des
types du cœur : renommer un champ du modèle sans casser les fichiers déjà
écrits ne coûtera qu'une fonction de conversion à ajuster.

Conséquence directe pour qui ajoute un champ au modèle : **il n'apparaîtra pas
dans le fichier** tant que `schema.rs` et `convert.rs` n'auront pas été
touchés. Ce n'est pas un oubli du format, c'est son point.

## Disposition du fichier

```text
octets 0..4   signature "PLYX"
octets 4..8   FORMAT_VERSION, u32 little-endian
octets 8..    corps MessagePack (FileBody), rmp_serde::to_vec_named
```

Le corps liste les nœuds en **parcours préfixe depuis la racine**
(`Tree::walk_all`) : deux sauvegardes successives du même projet produisent des
fichiers identiques **octet à octet**.

Les buffers de colonnes numériques échappent à l'encodage MessagePack
générique : ils sont écrits en octets bruts little-endian via `serde_bytes`
(8 octets par `f64`), pas comme un tableau d'éléments. Un million de points
pèse 8 Mo sur disque, pas un multiple gonflé par l'encodage par élément.

## Cinq propriétés tenues dès la version 1

Les rajouter après coup coûterait une migration.

1. **Écriture atomique.** `save` écrit dans un fichier temporaire du même
   répertoire que la destination, puis le renomme sur la cible. Un lecteur ne
   voit jamais un fichier à moitié écrit ; une écriture qui échoue ne laisse
   aucune trace sur la destination.
2. **Numéro de version en premier**, lu et vérifié **avant** de toucher au
   corps. Fichier plus récent → `UnsupportedVersion` ; fichier qui n'est pas un
   PlotYx → `NotAPlotyxFile`. Jamais de tentative de décoder un corps
   incompris.
3. **Valeur manquante ≠ chaîne vide.** `None` contre `Some("")` pour le texte ;
   les `NaN` numériques sont écrits et relus **bit à bit**
   (`to_le_bytes`/`from_le_bytes` conservent le motif binaire exact).
4. **Compteur de `NodeId` persisté.** `next_id_hint` est le **prochain**
   identifiant à attribuer, pas le plus grand attribué. Au chargement, le
   compteur repart à `max(valeur persistée, plus grand id lu + 1)` : un fichier
   écrit par une version boguée ne peut pas le faire reculer.
5. **Champs inconnus ignorés à la lecture.** `rmp_serde::to_vec_named`
   sérialise chaque structure comme une **map nommée**, ce qui rend le fichier
   auto-descriptif. Tout champ ajouté après la version 1 porte
   `#[serde(default)]`, pour qu'un fichier plus ancien qui ne le porte pas
   reste lisible.

Le choix de MessagePack découle directement de la propriété 5 : `bincode`
encode les structures en tuples **positionnels** — ajouter un champ y décale
tout ce qui suit, en retirer un est tout aussi cassant.

## Variantes inconnues : la perte est bornée au nœud

La propriété 5 ne couvrait que les **champs**, pas les **variantes**
d'énumération. `serde` dérive un décodage d'enum externally-tagged qui ne sait
pas se rattraper : un nom de variante inconnu faisait échouer l'enum, donc le
nœud, donc **tout le fichier**.

`KindRecord` et `PlotTypeRecord` écrivent maintenant leur
`Serialize`/`Deserialize` à la main (`unknown.rs`). Le principe : décoder
d'abord la valeur qui suit le nom de variante en un arbre générique
(`rmpv::Value`), la réencoder, puis *essayer* de la redécoder comme une
variante connue — via `rmp_serde`, jamais `rmpv::ext::from_value` (ce dernier
représente les enums par **indice**, convention différente de celle que ce
format écrit). Si l'essai échoue, le nom et le contenu bruts sont conservés
dans un `plotyx_plot::RawVariant`.

C'est ce qui borne la perte à **ce seul nœud** (ou cette seule série) au lieu
du fichier entier, et ce qui fait qu'une variante inconnue **survit à deux
sauvegardes successives à l'identique** (batterie `b20`). `RawVariant` ne
connaît ni `serde` ni MessagePack : il ne porte que des octets opaques, déjà
encodés tels que lus, et seul ce crate sait les réinterpréter.

Un détail qui ne se voit pas d'ici mais sans lequel rien de tout cela ne
tient : le nœud dégradé doit ensuite **passer la validation d'arbre**. Une
nature non reconnue compte comme fenêtre, donc un dossier l'acceptait ; un
classeur et une feuille, non — et une colonne dégradée sous sa feuille faisait
donc échouer tout le fichier, exactement là où la dégradation devait servir.
Les trois conteneurs l'acceptent depuis (`plotyx_core::NodeKind::accepts_child`,
batterie `b19`).

Le compromis est assumé et documenté : le détour par `rmpv::Value` copie les
octets d'une colonne une ou deux fois de plus qu'un décodage direct. Rien ne
mesure aujourd'hui le coût de `load`/`save` ; c'est un coût à surveiller si le
format finit par porter des colonnes bien plus grandes.

## Les colonnes à types mixtes : ajout additif, aucune migration

`ColumnType::Mixed` ([`modele-de-donnees.md`](modele-de-donnees.md)) a ajouté
une variante à `TypeRecord` et une à `ValuesRecord`. **`FORMAT_VERSION` n'a pas
bougé, et il n'y a aucune fonction de migration.** Les deux sens s'appuient sur
des mécanismes déjà en place :

| Sens | Ce qui se passe |
|---|---|
| ancien fichier, lecteur d'aujourd'hui | rien n'a changé pour lui — les trois variantes historiques et leurs encodages sont intacts, les colonnes homogènes s'écrivent octet pour octet comme avant |
| fichier d'aujourd'hui, lecteur d'hier | le nom de variante `Mixed` lui est inconnu, son décodage strict de la colonne échoue, `unknown.rs` la range en `KindRecord::Unknown` : il perd **cette colonne**, pas le fichier, et une ré-écriture la restitue à l'identique |

La seconde ligne est ce qui rend le choix de ne pas incrémenter
`FORMAT_VERSION` défendable, et c'est pour cela qu'elle est **testée**
(`b19_un_type_de_colonne_inconnu_ne_perd_que_cette_colonne`) plutôt que
supposée. Incrémenter aurait eu un coût sans contrepartie : *tous* les
fichiers écrits ensuite, colonne mixte ou pas, seraient devenus illisibles par
les lecteurs antérieurs.

### Pourquoi trois tableaux parallèles et pas un tableau d'énumérations

`ValuesRecord::Mixed` porte `{ tags, numbers, texts }` : un octet d'étiquette
par ligne (`0` vide, `1` nombre, `2` texte), puis les seules valeurs qui
existent — les nombres en little-endian brut comme `Float`, les textes à la
suite.

La raison est la même que celle qui fait écrire les colonnes numériques en
octets bruts : `to_vec_named` sérialise une énumération par le **nom** de sa
variante. Un `Vec<MixedCellRecord>` aurait écrit la chaîne `"Number"` — sept
octets — devant chacun des huit octets d'une valeur. Avec les tableaux
parallèles, une colonne d'un million de points majoritairement numérique pèse
9 Mo au lieu de 16, et les `NaN` comme les `-0.0` traversent bit à bit.

Le prix est une cohérence à vérifier à la lecture : les étiquettes doivent
annoncer exactement ce que les deux tableaux portent. Toute incohérence —
étiquette inconnue comprise — est un `Corrupt`, jamais un décalage silencieux
de toutes les lignes suivantes. Même politique que `BadFloatLength`.

## Bornes de lecture

`MAX_NODE_ID` = 10¹². Tout identifiant de nœud — ou `next_id_hint` — au-delà
est refusé **dès la lecture**. Très loin de `u64::MAX`, où `IdAllocator::next`
refuserait de toute façon, mais bien assez tôt pour qu'un fichier falsifié ne
puisse pas amener en mémoire un projet inutilisable (batterie `b4`).

## Ce qu'il faut savoir avant de modifier

- **Relire `docs/archive/rustrewrite/format-fichier-plx.md`** avant de toucher
  au schéma : plusieurs options y sont déjà écartées avec leurs raisons.
- Tout nouveau champ porte `#[serde(default)]`. Sans lui, le fichier de la
  veille devient illisible.
  Exemple : les couleurs de texte (`PlotSpecRecord::title_color`,
  `AxisSpecRecord::title_color`/`tick_color`) — un fichier plus ancien se
  relit avec des chaînes vides, c'est-à-dire en couleur automatique (test
  `un_spec_anterieur_aux_couleurs_de_texte_se_relit_en_automatique`). Ou
  `FileBody::saved_colors` (couleurs enregistrées du projet), placé avant
  `window_geometries` et `save_counter`, qui doit rester le dernier champ.
- `tests/roundtrip.rs` vérifie l'aller-retour ; côté robustesse, `b2`
  (persistance), `b3` (fichiers malformés puis sémantiquement hostiles), `b13`
  (bornes d'un `PlotSpec` venu du disque), `b19` (fichier écrit par une version
  plus récente), `b20` (deux écrivains sur un même fichier).

**À lire** : `plotyx-persistence/src/lib.rs`, `src/schema.rs`,
`src/unknown.rs` (têtes).
