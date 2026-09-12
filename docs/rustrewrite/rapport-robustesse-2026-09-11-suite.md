# Rapport de robustesse du socle — 2026-09-11, seconde passe (Rust pur)

Statut : **compte rendu daté, non corrigé rétroactivement** — même règle que
`resume-decisions.md` et que le rapport du matin. Il consigne l'état du socle
au commit `b88e9ff` de la branche `rewriterust`, comme lui.

**Suite de** `rapport-robustesse-2026-09-11.md`, pas remplacement : les onze
anomalies A1→C2 y restent décrites et leurs reproductions restent en place. Ce
document ajoute **dix-neuf anomalies**, numérotées à la suite (A7 à A11, B4 à
B9, C3 à C7, D1 à D3), et **douze batteries** (`b10` à `b21`) dans le même
crate, sous la même convention `#[ignore]`.

Il consigne **trois passes** du même après-midi, présentées ensemble parce
qu'elles se lisent ensemble :

1. A7, A8, B4, B5, C3 (batteries `b10`–`b14`) — les cinq stratégies du matin
   appliquées à du terrain Rust neuf ;
2. A9, A10, B6, C4, D1, D2 (batteries `b15`–`b17`) — **ce qu'un tableur
   scientifique rate typiquement** : la traduction valeur ↔ texte destinée à un
   humain, la sémantique d'une série dont les colonnes n'ont pas la même
   longueur, et le coût des gestes courants quand le projet grandit ;
3. A11, B7, B8, C5, C6 (batteries `b18`–`b20`) — **ce qui arrive entre deux
   requêtes et entre deux versions** : ce qui reste servi pendant qu'une macro
   est ouverte, ce qu'un fichier écrit par une version plus récente fait à ce
   lecteur-ci, et deux écrivains sur un même fichier ;
4. B9, C7, D3 (batterie `b21`) — une dernière relecture des fichiers encore
   non balayés, et du **chemin d'écriture** lui-même.

Une catégorie nouvelle apparaît : **D — coût et passage à l'échelle**. Ni
corruption ni plantage, mais deux opérations quotidiennes dont le coût croît
bien plus vite que le projet, avec des reproductions qui ne mesurent pas du
temps (voir §6).

**Périmètre de cette passe.** Rust uniquement : la frontière Python a été jugée
suffisamment éprouvée par les batteries `b7` et `b9` du matin. Rien ici ne
traverse PyO3, et aucune des douze batteries n'exige l'interpréteur vendorisé.
Ce sont donc douze batteries qui tournent partout, tout le temps.

**Cinq des seize anomalies produisent un résultat faux sans lever d'erreur**,
et une sixième (B8) fait disparaître du travail déjà écrit sur disque. Même
priorité que le matin, et pour la même raison.

---

## 1. Récapitulatif

| Id | Anomalie | Composant | Sévérité |
|---|---|---|---|
| [A7](#a7) | Le compteur de version d'une colonne recule, puis réattribue un numéro déjà utilisé | `plotyx-commands` / `plotyx-core` | **haute** |
| [A8](#a8) | Une fenêtre prend le nom d'un dossier frère ; le dossier devient introuvable par son chemin | `plotyx-core` | **haute** |
| [B4](#b4) | `histogram_bins` n'est borné que par le bas : un fichier de 1 ko fait `abort()` le processus | `plotyx-plot` / `plotyx-persistence` / `plotyx-core` | moyenne |
| [B5](#b5) | Une commande de lignes qui ne touche aucune colonne réussit et occupe un cran d'annulation | `plotyx-core` / `plotyx-commands` / `plotyx-app` | moyenne |
| [C3](#c3) | `SetCells` d'un bloc **vide** au-delà de la fin allonge quand même la colonne | `plotyx-core` | faible |
| [A9](#a9) | Traverser une cellule texte contenant une chaîne **vide** la transforme en valeur absente | `plotyx-app` | **haute** |
| [A10](#a10) | Une colonne d'erreur courte tronque les **données** ; vide, elle fait disparaître la série sans que rien ne le signale | `plotyx-core` | moyenne |
| [B6](#b6) | Traverser une cellule **sans rien saisir** écrit quand même : une révision, un cran d'annulation, un incrément de version | `plotyx-app` | moyenne |
| [C4](#c4) | Pas de notation scientifique : une mesure en 10⁻¹⁹ s'affiche sur 30 caractères, et la conversion vers texte coûte 7,6× | `plotyx-app` / `plotyx-core` | faible |
| [D1](#d1) | Le nom **automatique** d'une colonne est cherché en O(n³) pour la feuille : 4 000 colonnes = 17,8 s | `plotyx-core` | moyenne |
| [D2](#d2) | `resolve_path` alloue proportionnellement au nombre **total** de nœuds du projet | `plotyx-core` | moyenne |
| [A11](#a11) | `save` pendant une macro ouverte écrit sur disque un état que l'annulation va défaire | `plotyx-session` | **haute** |
| [B7](#b7) | Rien ne borne l'imbrication des macros : au-delà de 10 890 niveaux, l'annulation déborde la pile et fait `abort()` | `plotyx-commands` | moyenne |
| [B8](#b8) | Deux instances sur le même fichier : la seconde sauvegarde efface la première, sans un mot | `plotyx-persistence` | moyenne |
| [C5](#c5) | Une variante d'enum inconnue rend le fichier **entier** illisible, et l'erreur annonce « corrompu » | `plotyx-persistence` | moyenne |
| [C6](#c6) | L'instantané annonce `can_undo` pendant une macro, où toute annulation est refusée | `plotyx-session` / `plotyx-app` | faible |
| [D3](#d3) | Chaque commande parcourt tout l'arbre et refait l'union des empreintes de tout l'historique | `plotyx-commands` | moyenne |
| [C7](#c7) | `check_name` accepte l'octet nul, les sauts de ligne, les marques de direction, et n'a aucune borne de longueur | `plotyx-core` | faible |
| [B9](#b9) | `Project` dérive `Clone`, ce qui duplique l'allocateur : deux projets attribuent alors les mêmes identifiants | `plotyx-core` | faible |

**Atteignabilité.** Contrairement aux anomalies latentes du matin (A5, A6, B2
portaient sur du code que rien n'appelle encore), **quinze des dix-neuf sont
atteignables aujourd'hui** : A7, A8, A11, B5, B8, C3, C6, D1 et D2 par le bus
de commandes et les requêtes de session — donc par l'interface et par le pont
Python indifféremment — A9, B6 et C4 au clavier dans la grille du banc d'essai,
et C5 par un simple fichier. B5 est atteignable en trois clics, A9 et B6 en une
tabulation, B8 en ouvrant deux fois le même projet. Seuls A10 et B4 attendent
le chantier de rendu pour faire du dégât — mais le fichier qui porte B4, lui,
se charge déjà. D3 se paie à **chaque** commande, donc à chaque frappe. B7 et B9 sont les
seules dont l'atteignabilité demande un geste que personne ne fait aujourd'hui
(leurs sections le disent).

## 2. Les douze batteries ajoutées

| Batterie | Fichier | Ce qu'elle éprouve | Garde-fous | Reproductions |
|---|---|---|---|---|
| b10 | `b10_compteur_de_version.rs` | le contrat de `Column::version` au niveau du bus | 5 | 2 (A7) |
| b11 | `b11_portee_des_noms.rs` | les trois portées d'unicité des noms, et leur désaccord | 4 | 3 (A8) |
| b12 | `b12_commandes_sans_effet.rs` | les commandes acceptées qui ne font pas ce qu'elles disent | 3 | 3 (B5, C3) |
| b13 | `b13_bornes_du_spec.rs` | ce qu'un `PlotSpec` venu du disque peut demander au rendu | 2 | 2 (B4) |
| b14 | `b14_balayage.rs` | trois invariants revérifiés **après chaque pas** d'une séquence tirée au sort | 1 | 2 (A7, A8) |
| b15 | `b15_cellules_et_saisie.rs` | la traduction cellule ↔ texte, dans les deux sens | 3 | 4 (A9, B6, C4) |
| b16 | `b16_series_resolues.rs` | les séries dont les colonnes n'ont pas la même longueur | 3 | 2 (A10) |
| b17 | `b17_cout_des_operations.rs` | le coût des opérations courantes quand le projet grandit | 2 | 2 (D1, D2) |
| b18 | `b18_macro_ouverte.rs` | ce qui reste servi pendant qu'une macro est ouverte | 3 | 3 (A11, B7, C6) |
| b19 | `b19_compatibilite_du_format.rs` | ce qu'un `.plx` écrit par une version plus récente fait à ce lecteur | 2 | 2 (C5) |
| b20 | `b20_fichier_partage.rs` | deux écrivains, un fichier | 2 | 2 (B8) |
| b21 | `b21_cout_et_identite.rs` | le chemin d'écriture : son coût, et l'identité des nœuds | 2 | 4 (D3, C7, B9) |

Plus un cas ajouté à la sonde d'allocation (`plotyx-sonde-alloc
histogram_bins`) et **une seconde sonde hors-processus**,
`plotyx-sonde-pile` : un débordement de pile n'est pas une panique non plus —
le runtime écrit `has overflowed its stack` et appelle `abort()`, si bien que
ni `catch_unwind` ni `JoinHandle::join` ne rendent la main. Le cas de B7 ne
pouvait donc pas vivre dans un binaire de test : il l'aurait emporté tout
entier.

`b15` ajoute `plotyx-app` aux dépendances de développement du banc :
`AppState` ne dépend pas d'egui (c'est ce qui rend `plotyx-app/tests/shell.rs`
possible sans serveur graphique), et c'est le seul endroit du dépôt où une
valeur du modèle devient une chaîne destinée à un humain puis redevient une
valeur.

**b14 mérite un mot.** C'est le seul test du banc qui ne vise rien en
particulier : il joue des séquences de commandes tirées au sort (écritures,
structure, conversions, lignes, `undo`, `redo`) et revérifie à chaque pas trois
propriétés que le dépôt énonce ailleurs — version ↔ contenu, `path_of` ∘
`resolve_path` = identité, aller-retour disque. **C'est lui qui a trouvé A7 et
A8**, sans qu'on lui dise où regarder ; les reproductions déterministes de
`b10` et `b11` ont été écrites *après*, pour que le correctif se valide sur un
cas nommé plutôt que sur un tirage.

---

## 3. A — Corruptions silencieuses

<a id="a7"></a>
### A7 — Le compteur de version d'une colonne recule, puis réattribue un numéro déjà utilisé

**Sévérité : haute.** `crates/plotyx-commands/src/history.rs`
(`History::revert`, branche `Entry::Structural`) ; contrat énoncé dans
`crates/plotyx-core/src/column.rs` (`Column::version`). Reproductions :
`b10_le_compteur_de_version_ne_recule_jamais`,
`b10_deux_contenus_ne_partagent_jamais_un_numero_de_version`,
`b14_une_version_ne_designe_jamais_deux_contenus`.

`Column::version` est le seul mécanisme du socle qui réponde à « cette colonne
a-t-elle changé depuis la dernière fois que je l'ai lue ? » sans relire les
valeurs. Sa documentation fixe elle-même le sens de l'approximation tolérée :

> Le sens de l'approximation est délibéré : un faux « a changé » coûte un
> recalcul inutile, un faux « n'a pas changé » afficherait un résultat faux.
> **Seule la première erreur est tolérable.**

C'est exactement la seconde qui se produit. Une entrée d'historique
**structurelle** retient l'arbre d'avant (`Arc<Tree>`), donc ses colonnes,
donc leurs compteurs. L'annuler, c'est `replace_tree(before)` — et les
compteurs reviennent avec.

```text
A = [1, 2, 3]                       version 0
SetCells A ← 9.0    (données)       version 1   [9, 2, 3]
Rename A → "A2"     (structure)     version 1   ← l'instantané `before` retient A@1
SetCells A ← 5.0    (données)       version 2   [5, 2, 3]   ← un lecteur retient (2, [5,2,3])
undo                (données)       version 4   [9, 2, 3]
undo                (structure)     version 1   [9, 2, 3]   ← le compteur RECULE, 4 → 1
SetCells A ← 7.0    (données)       version 2   [7, 2, 3]   ← version 2, AUTRE contenu
```

Le lecteur qui avait retenu `(2, [5, 2, 3])` relit `2`, conclut « rien n'a
changé », et affiche `[5, 2, 3]` sur un modèle qui vaut `[7, 2, 3]`.

**Pourquoi rien ne l'a vu.** `PartialEq` de `Column` exclut délibérément le
compteur — c'est documenté et bien motivé (« annuler une écriture est *une
écriture de plus* »). Conséquence non prévue : la propriété centrale du bus,
`undoing_everything_restores_the_initial_tree_bit_for_bit`
(`plotyx-commands/tests/undo_random.rs`), compare des arbres **dont le
compteur est invisible**. Elle ne pouvait pas voir A7, et elle ne le pourra
jamais.

**Le socle a déjà tranché ce problème — pour les identifiants.** La
documentation de tête de `project.rs` :

> L'allocateur vit ici, **hors de l'arbre**, pour qu'une annulation qui
> restaure un ancien arbre (`replace_tree`) ne fasse pas reculer le compteur :
> un identifiant n'est jamais réutilisé sur toute la vie d'un projet.

Le compteur de version a fait le choix inverse — il vit *dans* l'arbre — sans
que la conséquence soit examinée. A7 est littéralement B1 vu dans l'autre
sens : là le compteur reculait par débordement, ici par restauration.

**Piste — trois options, chiffrées.**

| Option | Coût mémoire | Coût de développement | Détectabilité d'une régression |
|---|---|---|---|
| **1. Estampille globale** — un second compteur monotone sur `Project`, hors de l'arbre ; `Column::version` devient une estampille *attribuée* au lieu d'un `+= 1` interne | 8 octets par projet, plus 8 octets par colonne déjà payés | faible : un champ sur `Project`, une signature de mutateur, un `u64` de plus dans l'en-tête du fichier (à côté de `next_id_hint`, qui montre le patron) | **haute** — la propriété devient structurelle : une estampille n'est jamais réémise, donc jamais réutilisée pour un autre contenu, même si l'arbre restauré en ramène une ancienne. `b10`/`b14` deviennent des garde-fous |
| **2. Ré-estampiller à la restauration** — `replace_tree` parcourt l'arbre et rafraîchit les compteurs | 8 octets par projet | moyen : un parcours O(colonnes) à chaque annulation structurelle (2 000 colonnes = une passe de table de hachage, négligeable devant le reste) | **moyenne** — tout chemin futur qui restaure un arbre sans passer par `replace_tree` reperd l'invariant, en silence |
| **3. Documenter la limite** — écrire que le compteur ne vaut qu'entre deux annulations | nul | nul | **nulle** — le lecteur n'a alors plus aucun moyen fiable de savoir s'il est périmé, ce qui vide le champ de sa raison d'être |

L'option 1 est celle qui a déjà fait ses preuves dans ce dépôt, sur le même
problème, pour l'`IdAllocator`. **Ce qu'il ne faut pas garder, c'est un
compteur dont la documentation promet un invariant que le code ne tient pas.**

**Latence, honnêtement.** Aucun code de production ne lit `version()`
aujourd'hui en dehors de la persistance. Mais le champ existe pour le pont
Python et pour le rendu — c'est-à-dire pour les deux clients qui, par
construction, lisent sans copier et ont besoin de savoir qu'ils sont périmés.
Le jour où l'un des deux s'en sert, A7 devient un graphe faux à l'écran.

<a id="a8"></a>
### A8 — Une fenêtre prend le nom d'un dossier frère ; le dossier devient introuvable par son chemin

**Sévérité : haute.** `crates/plotyx-core/src/tree.rs` :
`check_name_available` (branche `is_window`), `validate_loaded_names`,
`resolve_parsed` via `child_by_name`. Reproductions :
`b11_une_fenetre_ne_prend_pas_le_nom_dun_dossier_frere`,
`b11_path_of_se_re_resout_toujours_sur_le_meme_noeud`,
`b11_le_chargeur_refuse_ce_que_lapi_de_mutation_refuse`,
`b14_le_chemin_canonique_redesigne_toujours_son_noeud`.

Un dossier est le **seul** parent du modèle qui accueille deux catégories
d'enfants (`accepts_child` : des dossiers *et* des fenêtres). « Ce nom est-il
libre ici ? » y a donc deux réponses possibles — et trois endroits du dépôt y
répondent différemment.

```rust
// tree.rs, check_name_available
if kind.is_window() {
    for id in self.windows() { /* unicité GLOBALE des noms de fenêtre */ }
    return Ok(());          // ← sort sans jamais regarder les frères
}
// … puis, pour tout le reste, comparaison à TOUS les frères
```

D'où une asymétrie que seul l'ordre de création révèle :

```text
add_folder(racine, "X")   → Ok(nœud 2)
add_note(racine, "X")     → Ok(nœud 3)                  ← ACCEPTÉ
   … mais dans l'autre ordre :
add_note(racine, "X")     → Ok
add_folder(racine, "X")   → Err(DuplicateName("X dans R"))
```

Tant que le dossier précède la fenêtre dans la liste d'enfants, rien ne se
voit : `child_by_name` tombe sur lui. Un `Move` — l'une des huit primitives du
socle, qui accepte ce déplacement pour la même raison — suffit à inverser
l'ordre :

```text
Move(nœud 3, racine, index 0)   → Ok
path_of(nœud 2)                 → "/X"
resolve_path("/X")              → Err(Path("dossier introuvable : X"))
tree.contains(nœud 2)           → true      ← le dossier est bel et bien là
```

`child_by_name` rend le **premier** enfant homonyme quelle que soit sa nature ;
le filtre `is_folder` de `resolve_parsed` — ajouté précisément pour qu'une
fenêtre homonyme ne soit pas prise pour un dossier — le transforme alors en
`None`, c'est-à-dire en « dossier introuvable ». Le filtre fait bien son
travail ; c'est la garde d'unicité en amont qui manque.

**Ce que cela casse nommément.** La huitième propriété de `b8_proprietes`,
`path_of ∘ resolve_path = identité`, est **fausse** sur des arbres que l'API
publique construit. Son générateur ne tire pas de noms partagés entre dossiers
et fenêtres ; celui de `b14` le fait, et la propriété tombe.

**Troisième désaccord, au chargement.** `validate_loaded_names` ne compare
qu'**à catégorie égale** (`if category == NameCategory::Window { continue; }`).
Le chargeur accepte donc une note « X » *suivie* d'un dossier « X », paire que
`Project::add_folder` refuse. Un fichier peut ouvrir un projet qu'aucune
commande ne saurait reproduire — ni réparer, puisque tout renommage vers « X »
sera refusé.

**Piste.** Faire dire la même chose aux trois endroits, et une seule fois : la
portée d'unicité d'un nom entre frères est **le parent**, la portée
supplémentaire d'une fenêtre est **le projet**. Concrètement, dans
`check_name_available`, ne pas sortir après la boucle globale des fenêtres mais
poursuivre par la comparaison aux frères ; et aligner `validate_loaded_names`
sur la même règle. Quelques lignes — mais **c'est une décision, pas une
correction** : la règle resserrée refusera des fichiers aujourd'hui acceptés,
ce qui appelle soit une migration, soit un message qui nomme le doublon. Ce
choix-là appartient à `questions-ouvertes.md`, pas à un correctif discret.

<a id="a9"></a>
### A9 — Traverser une cellule texte contenant une chaîne vide la transforme en valeur absente

**Sévérité : haute.** `crates/plotyx-app/src/state.rs` (`AppState::cell_text`
et `AppState::commit_cell`), `crates/plotyx-app/src/app.rs` (`cell_widget`).
Reproduction : `b15_une_chaine_vide_survit_a_la_traversee_de_sa_cellule`.

Le modèle distingue `Some(String::new())` (chaîne vide **présente**) de `None`
(valeur **absente**) — c'est écrit dans la documentation de tête de
`column.rs`, et c'est le troisième des cinq critères que `plotyx-persistence`
tient depuis la version 1 du format : « Valeur manquante vs chaîne vide ». La
grille confond les deux, **dans les deux sens** :

```rust
// state.rs, cell_text — les deux valeurs rendent la même chaîne
Cell::Text(None)        => String::new(),
Cell::Text(Some(text))  => text,          // « » si la chaîne est vide

// state.rs, commit_cell — « » ne peut redevenir que None
ColumnType::Text => if text.is_empty() { ValueBlock::Text(vec![None]) } else { … }
```

Et `cell_widget` valide à **toute** perte de focus sauf Échap (« clic ailleurs,
Tab, Entrée »), que le tampon ait changé ou non. Il suffit donc de passer sur
la cellule :

```text
avant  : T[1] = Text(Some(""))      T[2] = Text(None)
affiché: T[1] = « »                 T[2] = « »           ← indiscernables
l'utilisateur clique dans T[1], puis ailleurs
après  : T[1] = Text(None)                               ← la distinction est perdue
```

La valeur n'est pas seulement mal affichée : elle est **réécrite**. Un fichier
sauvegardé après cette traversée a perdu une information que le format
promettait de conserver, et rien — ni message, ni marque — ne l'a signalé.

**Pourquoi c'est atteignable sans le vouloir.** Combiné à [B6](#b6), tabuler à
travers une ligne suffit : chaque cellule texte vide traversée est réécrite.
Un aller-retour au clavier dans une feuille importée depuis un CSV où la
colonne distingue « champ vide » et « champ absent » efface la distinction sur
toute la ligne.

**Note.** `Cell::is_missing` tient déjà `Some("")` pour manquant — c'est
documenté, et c'est cohérent pour l'**affichage**. Le défaut n'est pas là : il
est dans le fait qu'une lecture suivie d'une écriture ne soit pas neutre.

**Piste — trois options, chiffrées.**

| Option | Coût mémoire | Coût de développement | Détectabilité |
|---|---|---|---|
| **1. Ne pas écrire quand le tampon n'a pas changé** (corrige aussi B6) | nul | faible : une comparaison dans `cell_widget` ou `commit_cell`, avant d'émettre la commande | **haute** — l'invariant « lire puis reposer est neutre » se teste en une ligne, et `b15` devient son garde-fou |
| **2. Rendre la distinction visible** — une marque discrète pour la chaîne vide présente, et une saisie qui permette de la produire | nul | moyen : une convention d'affichage à inventer, donc à décider | moyenne — dépend de la convention retenue |
| **3. Abandonner la distinction** dans tout le socle (`None` seul) | −24 octets par cellule texte vide | élevé : touche le modèle, le format, la persistance et son critère 3 | haute, mais c'est un changement de format |

L'option 1 corrige A9 **et** B6 d'un même geste, sans rien décider sur la
convention d'affichage. C'est le correctif à faire d'abord ; l'option 2 reste
ouverte ensuite.

<a id="a10"></a>
### A10 — Une colonne d'erreur courte tronque les données ; vide, elle fait disparaître la série

**Sévérité : moyenne** (latent jusqu'au rendu, mais la sémantique est fixée
maintenant). `crates/plotyx-core/src/resolve.rs` (`resolve_series`).
Reproductions : `b16_une_colonne_derreur_courte_ne_tronque_pas_les_donnees`,
`b16_une_colonne_derreur_vide_ne_fait_pas_disparaitre_la_serie`.

**Ce n'est pas une régression du port.** `legacy-python/src/plotyx/plot/
resolve.py` fait exactement pareil, et son commentaire nomme même le cas visé :
« Colonnes de longueur libre : on trace le préfixe commun plutôt que de refuser
le tracé — c'est le cas courant d'une colonne d'erreur plus courte que le
signal. » La décision est héritée, assumée, documentée. Ce qui ne l'est pas,
c'est sa conséquence chiffrée.

```rust
// resolve.rs — les quatre colonnes pèsent d'un poids égal dans la longueur
let lengths: Vec<usize> = [&x, &y, &x_error, &y_error].into_iter().flatten()
    .map(|values| values.len()).collect();
let n = lengths.iter().copied().min().unwrap_or(0);
```

```text
Y a 1000 points, dY en a 3
  → série résolue : 3 points, ok() = true, truncated = true, missing = []
Y a 1000 points, dY est vide
  → série résolue : 0 point, ok() = false, broken() = FALSE, missing = []
```

Deux choses distinctes, et la seconde est la plus grave :

1. **Une annotation raccourcit les données.** `x` et `y` sont des coordonnées
   appariées : les tronquer ensemble a du sens. Une barre d'erreur est une
   annotation **par point** ; une annotation absente devrait valoir `NaN` — la
   valeur manquante que le modèle possède déjà — et non raccourcir le signal.
   Ici une courbe de 1000 points est tracée avec 3, et `ok()` rend `true`.
2. **La série disparaît sans être détectable.** Avec `dY` vide, la série n'a
   plus aucun point, mais `broken()` rend `false` et `missing` est vide :
   `ResolvedPlot::broken_series()` — le **seul** diagnostic que l'API offre —
   ne la listera pas. C'est littéralement l'état que la documentation de tête
   du module promet d'éviter : « jamais une série silencieusement vide ».
   Ajouter une colonne d'erreur à une série et oublier de la remplir suffit.

Le contraste avec ce que le module fait **bien** est instructif : une colonne
d'erreur supprimée, ou de type texte, donne `broken() = true` et
`missing = [YError]` (garde-fous `b16_…_supprimee_…` et `b16_…_de_type_texte_…`,
tous deux verts). Le seul cas non couvert est celui où la colonne existe, est
numérique, et est simplement plus courte.

**Piste.** Séparer les deux rôles dans le calcul de `n` : `x` et `y` fixent la
longueur, `x_error` et `y_error` s'y ajustent (complétées par `NaN`,
tronquées si plus longues). `truncated` garde alors son sens d'origine — les
**coordonnées** ne s'accordaient pas — et le cas « barre d'erreur incomplète »
devient un état nommé à part, que le rendu peut afficher. À défaut, et à coût
nul : faire entrer ce cas dans `broken()`/`missing`, pour qu'il cesse d'être
invisible.

<a id="a11"></a>
### A11 — `save` pendant une macro ouverte écrit sur disque un état que l'annulation va défaire

**Sévérité : haute.** `crates/plotyx-session/src/actor.rs`
(`Request::Save`). Reproduction : `b18_la_sauvegarde_est_refusee_pendant_une_macro`.

Une macro promet l'atomicité : ses commandes deviennent **une seule** entrée
d'historique, et `abort_macro` « ne doit laisser aucune trace ». L'acteur garde
la macro ouverte **entre deux requêtes** ; pendant ce temps, les six autres
requêtes du protocole restent servies. `Request::Load` le sait — il refuse
avant même de toucher au disque. `Request::Save`, dans le même `match`, trois
branches plus haut, ne consulte pas `history.in_macro()`.

```rust
// actor.rs — Load se garde…
fn load_project(…) -> SessionResult<()> {
    if history.in_macro() {
        return Err(SessionError::Command(CommandError::MacroOpen("load")));
    }
    …
}

// …Save ne se garde pas.
Request::Save { path, reply } => {
    let outcome = plotyx_persistence::save(&project, &path).map_err(SessionError::from);
    let _ = reply.send(outcome);
}
```

```text
begin_macro("import d'un fichier de mesures")
execute(SetCells A ← 99.0)        → Ok
save("manip.plx")                 → Ok            ← rien ne le refuse
    le fichier contient           : [99.0, 2.0, 3.0]
abort_macro()                     → Ok
    le modèle contient            : [1.0, 2.0, 3.0]
```

Le disque garde l'état intermédiaire d'une opération que l'utilisateur a
explicitement abandonnée, et **aucune annulation ne peut plus l'atteindre** :
l'historique a effacé la macro, le modèle est revenu en arrière, seul le
fichier se souvient. C'est le pire des trois : l'écran dit une chose, le
fichier en dit une autre, et rien ne signale l'écart.

**Ce n'est pas seulement `save`.** `Session::shutdown` pendant une macro
ouverte rend le `Project` avec les mutations de la macro **appliquées**, sans
entrée d'historique et sans que `end_macro` ou `abort_macro` ait jamais été
appelé : le travail est commis en silence. C'est la même absence de garde, à la
sortie.

**Portée au-delà de ces deux-là.** Le protocole compte huit requêtes ;
`in_macro()` n'en garde qu'une. La question n'est donc pas « faut-il garder
`Save` ? » mais « lesquelles des huit une macro ouverte doit-elle suspendre ? »
— et la réponse mérite d'être écrite en tête de `plotyx-session`, à côté de la
limite déjà documentée sur l'identité de l'émetteur (A4).

**Piste — trois options, chiffrées.**

| Option | Coût mémoire | Coût de développement | Détectabilité |
|---|---|---|---|
| **1. Refuser `Save` en macro**, comme `Load` | nul | trois lignes, symétriques de celles de `load_project` | **haute** — `b18` devient garde-fou, et la symétrie des deux branches se relit d'un coup d'œil |
| **2. Sauvegarder l'état d'avant la macro** — l'instantané `Arc<Tree>` retenu à `begin_macro` existe déjà dans `saved_redo`… non, il n'y est pas : il faudrait le retenir | un `Arc<Tree>` par macro ouverte, soit un pointeur | moyen : il faut décider ce que « sauvegarder » veut dire quand le modèle affiché n'est pas celui écrit | faible — deux états coexistent, et l'utilisateur doit comprendre lequel part sur le disque |
| **3. Fermer la macro d'office avant d'écrire** | nul | faible | moyenne — mais cela retire à l'utilisateur le droit d'abandonner, ce qui est exactement ce que la macro lui promettait |

L'option 1 est la seule qui garde la promesse de la macro intacte, et c'est
celle que le crate applique déjà à l'autre requête qui touche au disque.

---

## 4. B — Plantage et état incohérent

<a id="b4"></a>
### B4 — `histogram_bins` n'est borné que par le bas : un fichier de 1 ko fait `abort()` le processus

**Sévérité : moyenne.** `crates/plotyx-plot/src/series.rs`
(`SeriesSpec::histogram_bins`), `crates/plotyx-persistence/src/convert.rs`
(`record_to_series_spec`), `crates/plotyx-core/src/resolve.rs` (`histogram`).
Reproductions : `b13_histogram_bins_venu_du_disque_est_borne`,
`b13_le_type_lui_meme_borne_le_nombre_de_classes`. Sonde :
`plotyx-sonde-alloc histogram_bins`.

La décision R2 a rendu « zéro classe » inexprimable (`NonZeroU32`), et la
conversion refuse le zéro d'un fichier (`ConvertError::ZeroHistogramBins`).
L'autre extrémité de l'intervalle n'est bornée **nulle part** : ni par le type,
ni par la conversion, ni par la fonction qui le consomme.

```text
projet d'un graphe, une série d'histogramme, histogram_bins = u32::MAX
save  → fichier de 1 110 octets
load  → Ok, sans le moindre avertissement
spec.panels[0].series[0].histogram_bins = 4294967295

resolve::histogram(&[1.0, 2.0, 3.0], 4294967295)
  edges  : Vec<f64> de 4 294 967 296 éléments  →  34 359 738 368 octets
  counts : Vec<f64> de 4 294 967 295 éléments  →  34 359 738 360 octets

memory allocation of 34359738368 bytes failed
Aborted (core dumped)          // code de sortie 134, pas une panique
```

C'est le même mécanisme que B3 — un échec d'allocation Rust est un `abort()`,
que ni `catch_unwind`, ni l'acteur, ni l'interface ne peuvent rattraper — mais
par un chemin différent, et à un endroit où le correctif est plus simple : ici
le paramètre fautif tient dans un `u32` d'un fichier de 1 ko, alors que B3
demandait un `usize` improbable venu du bus.

**Piste.** Le raisonnement qui a déjà produit `ConvertError::ZeroHistogramBins`
s'applique mot pour mot à l'autre borne : un histogramme réel n'a jamais plus
de classes que l'écran n'a de pixels, une valeur qui approche 2³² est la
signature d'un fichier corrompu. Deux variantes, du moins cher au plus sûr :

1. **Plafond à la conversion** — une variante de `ConvertError` de plus, trois
   lignes dans `record_to_series_spec`. Ferme le chemin du disque, laisse
   ouvert celui du presse-papiers et des modèles, que `spec.rs` annonce
   pourtant explicitement (« un `PlotSpec` peut aussi voyager par
   presse-papiers ou par modèle sans passer par un fichier »).
2. **Un type `BinCount` borné** dans `plotyx-plot`, dans l'esprit exact de
   `NonZeroU32` : l'état illégal cesse de s'écrire, quel que soit le chemin
   emprunté. Coût : un newtype et ses conversions ; bénéfice : le plafond vit
   au même endroit que la règle qu'il exprime.

<a id="b5"></a>
### B5 — Une commande de lignes qui ne touche aucune colonne réussit et occupe un cran d'annulation

**Sévérité : moyenne.** `crates/plotyx-core/src/project.rs` (`row_targets`),
`crates/plotyx-commands/src/history.rs` (`apply`, branches `InsertRows` /
`DeleteRows`), `crates/plotyx-app/src/app.rs:248` et `state.rs` (`rows`).
Reproductions : `b12_une_commande_sans_effet_noccupe_pas_un_cran_dannulation`,
`b12_une_ligne_peut_etre_ajoutee_a_la_fin_dune_feuille`.

`row_targets` ne retient que les colonnes telles que `at < len`. Le choix est
**documenté et défendable** au niveau du cœur (« allonger une colonne courte
inventerait des données ») ; ce qui ne l'est pas, c'est ce qu'en font les deux
couches au-dessus.

```text
feuille neuve, une colonne neuve (donc vide)
Command::InsertRows { at: 0, count: 1 }   → Ok(())
longueur de la colonne  : 0        (inchangée)
lignes de la feuille    : 0        (inchangées)
entrées d'historique    : 1        « Insérer 1 ligne »
```

Trois conséquences, par ordre de visibilité :

1. **Le bouton « Insérer 1 ligne » du banc est inopérant sur une feuille
   neuve**, et annonce un succès : `app.rs:248` code `at: 0` en dur, et
   `AppState::execute` pose le libellé de la commande comme message de statut
   dès qu'elle rend `Ok`.
2. **Le menu Annuler propose une action qui ne défera rien**, et l'entrée
   consomme un cran des cent de `DEFAULT_LIMIT` — elle peut donc évincer une
   entrée réelle.
3. **Aucune commande du socle n'ajoute une ligne à la fin d'une feuille.**
   `Column::insert_rows` accepte pourtant `at ∈ 0..=len` (l'égalité insère en
   fin, c'est écrit dans sa documentation) ; le filtre `at < len` du niveau
   feuille l'interdit, en silence. `InsertRows { at: row_count, count: 1 }`
   rend `Ok` et ne fait rien.

**Le même motif ailleurs, à coût nul.** `Command::Rename { id, name }` vers le
nom courant rend `Ok` sans toucher à l'arbre (`Project::rename` sort avant
l'`Arc::make_mut`, vérifié : l'`Arc` est le même après), et empile pourtant une
entrée « Renommer M en M ». Aucune donnée en jeu, mais le même cran perdu et le
même succès annoncé.

**Piste — deux décisions à prendre, pas une.**

- *Une commande acceptée doit avoir un effet, ou être refusée.* Le moins
  cher : dans `History::apply`, ne pas empiler une entrée dont le delta n'a
  touché aucune colonne (`columns.is_empty()` / `removed.is_empty()`), et
  laisser la commande rendre `Ok` — l'utilisateur ne voit alors plus d'entrée
  fantôme. Le plus net : rendre `Err(IndexOutOfBounds)` quand aucune colonne de
  la feuille ne possède la ligne `at`, ce qui fait remonter un message au lieu
  d'un faux succès. La seconde change la surface publique du bus ; la première
  non.
- *`at == len` doit-il ajouter une ligne en fin de feuille ?* C'est ce
  qu'attend l'utilisateur d'un tableur, c'est ce qu'accepte déjà la primitive
  par colonne, et c'est aujourd'hui impossible. Trancher et l'écrire dans la
  documentation de `Project::insert_rows`, qui décrit le filtre sans dire ce
  qu'il rend inatteignable.

<a id="b6"></a>
### B6 — Traverser une cellule sans rien saisir écrit quand même

**Sévérité : moyenne.** `crates/plotyx-app/src/app.rs` (`cell_widget`),
`crates/plotyx-app/src/state.rs` (`commit_edit`). Reproduction :
`b15_traverser_une_cellule_sans_rien_taper_necrit_pas`.

`cell_widget` ouvre la saisie à la prise de focus (`begin_edit`, tampon
initialisé au texte affiché) et la valide à **toute** perte de focus sauf
Échap — sans jamais comparer le tampon à ce qu'il contenait au départ.

```text
trois tabulations dans une colonne, sans une frappe
  révision            0 → 3
  version de la colonne 0 → 3
  historique          « Saisie dans N » × 3
  valeurs             inchangées
```

Trois conséquences, les mêmes que [B5](#b5) mais par un chemin plus fréquent :
le menu Annuler se remplit d'actions qui ne défont rien, l'historique de cent
crans se vide de ses vraies entrées, et le compteur qui dit aux lecteurs que la
colonne a changé s'incrémente alors que rien n'a bougé — un faux « a changé »,
c'est-à-dire le sens d'erreur que `Column::version` tolère, mais payé
gratuitement à chaque tabulation.

Et combiné à [A9](#a9), la traversée n'est pas seulement bruyante : elle est
**destructrice** sur une cellule texte vide.

**Piste.** Comparer le tampon à la valeur de départ avant d'émettre la
commande — dans `cell_widget`, qui détient déjà l'original (il le compare pour
récupérer la première frappe), ou dans `AppState::commit_edit`, qui peut le
relire. C'est l'option 1 de [A9](#a9) : un seul correctif pour les deux.

<a id="b7"></a>
### B7 — Rien ne borne l'imbrication des macros : au-delà de 10 890 niveaux, l'annulation fait `abort()`

**Sévérité : moyenne.** `crates/plotyx-commands/src/history.rs`
(`History::revert`, branche `Entry::Macro`, et le `Drop` dérivé de `Entry`).
Reproduction : `b18_limbrication_des_macros_est_bornee`. Sonde :
`plotyx-sonde-pile`.

La documentation dit « Les macros s'imbriquent », et rien ne borne cette
imbrication. `begin_macro` ne rend même pas de `Result`. Or `Entry::Macro`
porte un `Vec<Entry>`, et deux parcours de cette structure sont **récursifs** :

```rust
Entry::Macro { entries, .. } => {
    for entry in entries.iter().rev() {
        Self::revert(project, entry)?;      // un cadre de pile par niveau
    }
    Ok(())
}
```

Le `Drop` dérivé de `Entry` l'est aussi — et c'est lui que `History::clear()`
déclenche, c'est-à-dire ce que l'acteur appelle **à chaque `load`**.

```text
sonde, pile de 2 Mio (celle qu'un std::thread reçoit quand personne ne la fixe,
donc celle du fil de l'acteur : Session::spawn n'appelle pas stack_size)

  construire 200 000 niveaux           : OK          ← tout est itératif à la construction
  annuler                              : passe à 10 890, abort() à 10 891
  clear()  (ce que `load` appelle)     : passe à 26 147, abort() à 26 148
  lâcher l'historique (Drop)           : passe à 26 145, abort() à 26 146

  même geste sur une pile de 8 Mio     : annuler passe à 43 658, abort() à 43 659
```

Le coût est parfaitement linéaire : ~192 octets de pile par niveau pour
`revert`, ~80 pour `Drop`. Et **un débordement de pile n'est pas une
panique** : le runtime écrit `has overflowed its stack` et appelle `abort()`.
Ni `catch_unwind`, ni l'acteur, ni `JoinHandle::join` ne peuvent intervenir —
le processus meurt avec tout le travail non sauvegardé. C'est la même nature de
fin que B3, par un autre chemin.

**Atteignabilité, honnêtement.** Personne n'imbrique dix mille macros à la
main, et la surface `pyx` n'expose pas `begin_macro`. Aujourd'hui, seul du code
Rust tenant un `SessionHandle` peut y arriver. Mais `begin_macro` est une
requête publique du protocole, sans borne et sans `Result`, et la profondeur
n'est pas un paramètre que l'appelant surveille : une boucle qui ouvre une
macro par pas — un import ligne à ligne, un glisser-déposer qui en ouvre une
par image — y arrive sans que personne l'ait voulu. Le banc du matin a éprouvé
64 niveaux et conclu que l'imbrication « ne bronche pas » ; c'est vrai à 64.

**Piste — deux options, chiffrées.**

| Option | Coût mémoire | Coût de développement | Détectabilité |
|---|---|---|---|
| **1. Une borne explicite** — `begin_macro` rend `Err` au-delà d'une profondeur nommée (1 000 laisse quatre fois la marge du pire geste mesuré) | nul | faible, mais change la signature de `begin_macro` et de `Request::BeginMacro` | **haute** — la borne est une constante nommée, `b18` la vérifie, et le message dit ce qui est refusé |
| **2. Rendre `revert` et `Drop` itératifs** — une pile explicite dans les deux, `Drop` manuel sur `Entry` | une pile de travail proportionnelle à la profondeur, sur le tas | élevé : un `Drop` manuel sur un enum récursif est du code délicat, et il faut aussi couvrir `reapply` et `footprint_of` | moyenne — lève la limite, mais rien ne signale une régression qui la réintroduirait |

L'option 1 échange une limite arbitraire contre un refus visible — exactement
l'arbitrage déjà retenu pour B3 (« une limite arbitraire mais documentée vaut
infiniment mieux qu'un `abort()` »).

<a id="b8"></a>
### B8 — Deux instances sur le même fichier : la seconde sauvegarde efface la première

**Sévérité : moyenne.** `crates/plotyx-persistence/src/lib.rs` (`save`),
`crates/plotyx-app/src/state.rs` (`AppState::save`). Reproductions :
`b20_sauvegarder_sur_un_fichier_modifie_ailleurs_est_refuse`,
`b20_un_fichier_porte_de_quoi_detecter_un_ecrasement`.

`save` tient sa promesse d'**atomicité** : temporaire dans le même répertoire
puis `rename`, donc jamais de fichier à moitié écrit, et un échec ne touche pas
la cible. Les deux garde-fous de `b20` le vérifient. Mais l'atomicité protège
de l'**interruption**, pas de la **concurrence** — et rien dans le dépôt ne
remarque qu'un fichier a changé depuis qu'on l'a lu : ni verrou, ni date de
modification retenue, ni empreinte, ni compteur de sauvegarde.

```text
A ouvre manip.plx et le sauvegarde       fichier : [1, 2, 3]
B charge manip.plx, écrit, sauvegarde    fichier : [42, 2, 3]
A, qui n'a rien vu, écrit et sauvegarde  → Ok(())
                                         fichier : [1, 7, 3]
```

Le 42 de B a disparu. A n'a rien su, B ne saura rien : c'est la « mise à jour
perdue » classique de tout logiciel qui édite des documents. Deux instances de
PlotYx sur le même `.plx` — deux fenêtres, un collègue sur un partage réseau,
ou simplement un script qui écrit pendant qu'une session est ouverte —
suffisent.

**Le format n'aide pas non plus après coup.** L'en-tête tient en huit octets
(signature + numéro de version) et le corps ne porte que `root`,
`next_id_hint` et les nœuds : ni date, ni compteur de sauvegarde, ni identité
d'écrivain. Même *a posteriori*, rien ne permet de dire « ce fichier n'est plus
celui que j'ai chargé ». `Column::version` est bien persisté, mais il est **par
colonne** et recule quand un instantané est restauré ([A7](#a7)) : il ne peut
pas servir d'estampille de fichier.

**Piste.** Un compteur de sauvegarde monotone dans `FileBody` — quatre à huit
octets, `#[serde(default)]`, donc **sans bouger le numéro de version du
format** — suffit à transformer l'écrasement silencieux en refus explicite :
`save` relit l'en-tête de la cible, compare, et rend une erreur dédiée si elle
a avancé. L'interface en fait alors un « le fichier a changé sur le disque :
recharger, ou écraser ? », qui est la question que l'utilisateur veut qu'on lui
pose. Un verrou de fichier (`flock`) est l'autre voie ; elle coûte davantage,
ne marche pas partout (partages réseau), et ne dit rien après un arrêt brutal.

**À noter, positivement.** Le cas qui pourrait vraiment corrompre — deux
processus écrivant au même instant — est déjà couvert par l'écriture atomique :
l'un des deux fichiers gagne entièrement, jamais un mélange des deux. Ce qui
manque est la **détection**, pas l'intégrité.

<a id="b9"></a>
### B9 — `Project` dérive `Clone` : deux copies attribuent les mêmes identifiants

**Sévérité : faible** (aucun appelant aujourd'hui). `crates/plotyx-core/src/project.rs:41`.
Reproduction : `b21_deux_projets_nattribuent_jamais_le_meme_identifiant`.

```rust
#[derive(Debug, Clone)]
pub struct Project {
    tree: Arc<Tree>,
    ids: IdAllocator,
}
```

L'arbre est partagé (un `Arc`, donc gratuit) ; l'allocateur, lui, est
**dupliqué**. Les deux copies repartent du même compteur :

```text
a.add_folder(root, "M")        → nœud 2
let mut b = a.clone();
a.add_folder(root, "X")        → nœud 3
b.add_folder(root, "Y")        → nœud 3        ← deux nœuds différents, un seul identifiant
```

C'est l'exact contraire de ce que le type promet — « compteur monotone, jamais
réutilisé sur toute la vie d'un projet, rechargement compris » — et de ce qui
motive l'emplacement de l'allocateur : la documentation de tête de
`project.rs` explique qu'il vit **hors de l'arbre** pour qu'une annulation ne
le fasse pas reculer. Un `derive` de trois caractères ouvre l'autre porte.

**Atteignabilité, honnêtement.** Aucun appelant du dépôt ne clone un
`Project` : `git grep` ne trouve que `project.tree().clone()`, qui est le
clonage de l'`Arc<Tree>` et n'a rien à voir. C'est une porte ouverte, pas une
fuite constatée. Elle mérite d'être fermée pendant qu'elle ne coûte rien :
c'est le genre d'API qu'un futur « dupliquer le projet » ou « ouvrir dans une
seconde fenêtre » emprunterait sans se méfier — et le symptôme
(deux nœuds sous un même identifiant) serait alors exactement **A3** du
matin (`rapport-robustesse-2026-09-11.md`), mais sans fichier pour l'expliquer.

**Piste.** Retirer le `derive(Clone)` — rien ne l'utilise — ou, si une copie
doit exister un jour, l'écrire à la main en **avançant** l'allocateur de la
copie au-delà de celui de l'original, et le documenter. Le `derive` est le
seul choix des trois qui ne dise rien.

---

## 5. C — Comportement à écrire, ou à corriger

<a id="c3"></a>
### C3 — `SetCells` d'un bloc vide au-delà de la fin allonge quand même la colonne

**Sévérité : faible.** `crates/plotyx-core/src/column.rs` (`write_block`).
Reproduction : `b12_setcells_dun_bloc_vide_ne_change_rien`.

```text
A = [1.0, 2.0, 3.0]
Command::SetCells { column: A, start: 10, values: Float([]) }   → Ok(())
A = [1.0, 2.0, 3.0, NaN, NaN, NaN, NaN, NaN, NaN, NaN]          // longueur 10
```

`write_block` fait `vec.resize_with(start, missing)` **avant** de découvrir
qu'il n'y a rien à écrire. Une commande qui n'écrit aucune valeur ajoute donc
sept lignes manquantes.

L'annulation, elle, est correcte (le delta porte `previous_length = 3`, et
`set_length` rétablit), et le chemin n'est pas atteignable depuis le pont
Python, dont `set_column` exige une longueur exacte. Reste qu'une écriture de
zéro valeur change les données, et que rien ne le dit.

**Piste.** Trois lignes : sortir de `write_block` sans rien faire quand
`new_values.is_empty()`. Ou, si l'allongement est voulu, le dire dans la
documentation de `Command::SetCells`, qui annonce aujourd'hui « en allongeant
la colonne si besoin » — un « si besoin » que zéro valeur ne devrait pas
déclencher.

<a id="c4"></a>
### C4 — Pas de notation scientifique : affichage illisible, et conversion vers texte 7,6× plus chère

**Sévérité : faible** (mais sur la forme de valeur qui est la raison d'être du
logiciel). `crates/plotyx-app/src/state.rs` (`cell_text`),
`crates/plotyx-core/src/column.rs` (`set_column_type`, branche `Float → Text`).
Reproductions : `b15_une_mesure_scientifique_tient_dans_une_cellule`,
`b15_convertir_en_texte_ne_multiplie_pas_la_memoire`.

Les deux endroits formatent un `f64` par son `Display`, qui en Rust **n'émet
jamais d'exposant** : il écrit la forme décimale complète, la plus courte qui
se relise à l'identique.

```text
valeur                      Display          notation {:e}
charge de l'électron        30 caractères    15 caractères   1.602176634e-19
nombre d'Avogadro           24 caractères    13 caractères   6.02214076e23
f64::MIN_POSITIVE          326 caractères    23 caractères   2.2250738585072014e-308
f64::MAX                   309 caractères    22 caractères
```

Une colonne de la grille fait 110 px : une quinzaine de caractères. Toute
mesure physique usuelle est donc tronquée à l'écran en une suite de zéros
indiscernables les unes des autres.

**La même absence d'exposant a un coût mémoire, mesuré.** `set_column_type`
vers `Text` écrit `value.to_string()` par cellule :

```text
50 000 mesures en 1,6 × 10⁻¹⁹ A
  avant (f64)   :   400 000 octets
  après (texte) : 3 036 560 octets     → 7,6×
```

Le facteur dépend de l'ordre de grandeur des données, pas de leur nombre : il
monte vers 40× pour des dénormaux, et l'historique en retient une copie de plus
(`DataDelta::ColumnType` est le seul delta de données qui soit un instantané —
c'est assumé, la conversion étant destructrice dans les deux sens). Sur un
million de points, une conversion vers texte demande donc quelques centaines de
mégaoctets là où la notation scientifique en demanderait quelques dizaines.

**À noter : l'aller-retour reste fidèle.** `Display` garantit la relecture
exacte, et le rapport du matin l'a vérifié bit à bit (`-0.0`, dénormaux,
`f64::MAX`, `0.1 + 0.2`). Ce n'est donc pas une perte de précision — c'est un
choix de forme, qui n'a jamais été fait explicitement.

**Piste.** Un formatage unique, au même endroit pour les deux usages : forme
décimale tant qu'elle tient dans une largeur donnée, `{:e}` au-delà. La
documentation de `cell_text` annonce déjà que « le vrai formatage appartiendra
à `ColumnMeta::display_format`, que cette phase ne lit pas » — c'est là que la
règle a sa place, et `set_column_type` doit s'y brancher aussi, sans quoi
l'affichage et la conversion diront deux choses différentes de la même valeur.

<a id="c5"></a>
### C5 — Une variante d'enum inconnue rend le fichier entier illisible, et l'erreur annonce « corrompu »

**Sévérité : moyenne.** `crates/plotyx-persistence/src/schema.rs` (les sept
enums du schéma), `crates/plotyx-persistence/src/lib.rs` (classement de
l'erreur). Reproductions : `b19_une_variante_inconnue_ne_perd_pas_tout_le_fichier`,
`b19_un_type_de_trace_inconnu_ne_perd_pas_les_donnees`.

Le format porte **deux** mécanismes de compatibilité, et ils ne disent pas la
même chose sur le numéro de version :

- l'**en-tête** — un fichier de version supérieure est refusé proprement
  (`UnsupportedVersion`), sans qu'un octet du corps soit décodé. Garde-fou
  `b19_un_numero_de_version_superieur_est_refuse_proprement` : vert ;
- les **maps nommées** de MessagePack — un champ inconnu est ignoré, un champ
  absent prend son `#[serde(default)]`. Garde-fou
  `b19_un_champ_inconnu_est_bien_ignore` : vert lui aussi. C'est ce mécanisme
  qui doit permettre, dit la documentation, « d'ajouter des données sans casser
  les fichiers déjà écrits » — donc **sans toucher au numéro de version**.

Le second couvre les **champs**. Il ne couvre pas les **variantes**, et rien ne
le dit :

```text
un projet valide de bout en bout : un classeur, une feuille, 8 000 octets de
mesures, et un graphe dont l'unique série porte plot_type = "Violin"

  load()  → fichier corrompu … : unknown variant `Violin`,
            expected one of `Scatter`, `Line`, `LineScatter`, `Bar`,
            `Histogram`, `ErrorBar`

les mesures sont perdues avec le reste
```

Deux choses ne vont pas, et la seconde est réparable en trois lignes :

1. **La granularité.** Un seul nœud — ou un seul champ enfoui dans le spec d'un
   seul graphe — emporte le fichier entier. Sept enums du schéma exposent la
   même surface : `KindRecord`, `TypeRecord`, `RoleRecord`, `PlotTypeRecord`,
   `LineStyleRecord`, `SymbolShapeRecord`, `LegendPositionRecord`,
   `AxisScaleRecord`. Ajouter une seule valeur à l'une d'elles rend illisible,
   pour tout lecteur antérieur, **tout fichier qui l'utilise**.
2. **Le mot.** L'erreur rendue est `PersistenceError::Corrupt`, « fichier
   corrompu » : elle envoie l'utilisateur chercher une sauvegarde alors que son
   fichier est intact, et que la bonne réponse est « mettez PlotYx à jour ».

C'est le pendant exact de **C1** du matin (`rapport-robustesse-2026-09-11.md`) — le `format_version` d'un
`PlotSpec` jamais vérifié — vu de l'autre côté : là un lecteur v1 accepte du v2
en silence, ici il refuse tout un fichier en criant au mauvais mot.

**Piste — deux niveaux, à décider séparément.**

| Option | Coût de développement | Ce qu'elle achète |
|---|---|---|
| **1. Dire la vérité** — distinguer, au classement de l'erreur, « variante inconnue » de « octets illisibles », et rendre `UnsupportedVersion` (ou une variante dédiée) plutôt que `Corrupt` | faible : `rmp_serde` donne déjà le message « unknown variant `X` » ; il suffit de le reconnaître | le message dit ce qu'il faut faire. Rien d'autre ne change |
| **2. Dégrader gracieusement** — un `#[serde(other)]` sur les enums de style (un type de tracé inconnu retombe sur `Line`), et une variante `Inconnu` sur `KindRecord` qui garde le nœud sans le comprendre | élevé : `#[serde(other)]` ne marche que sur les enums sans données, donc `KindRecord` demande un `Deserialize` écrit à la main | un fichier d'une version future s'ouvre en perdant du style, pas des mesures |

**Et une décision d'écriture, gratuite dès aujourd'hui** : écrire, dans la doc
de `schema.rs`, que la règle du numéro de version diffère selon ce qu'on
ajoute — un **champ** n'oblige pas à le bouger, une **variante** l'oblige. Le
fichier de format (`format-fichier-plx.md`) est l'endroit.

**Note annexe, sans gravité.** Un fichier dont l'en-tête annonce la version
`0` — un numéro qui n'a jamais existé — est accepté et lu comme du v1 : le
contrôle ne compare que `version > FORMAT_VERSION`. Sans conséquence connue,
mais c'est un numéro de moins pour repérer un fichier fabriqué de travers.

<a id="c6"></a>
### C6 — L'instantané annonce « Annuler » pendant une macro, où toute annulation est refusée

**Sévérité : faible.** `crates/plotyx-session/src/actor.rs` (`build_snapshot`),
`crates/plotyx-app/src/state.rs` (`AppState::can_undo`, `undo_label`).
Reproduction : `b18_linterface_nannonce_pas_une_annulation_impossible`.

```text
pendant une macro ouverte
  can_undo   = true
  undo_label = Some("Saisie dans A")
  in_macro   = true
  undo()     → Err(Command(MacroOpen("undo")))
```

`HistoryState` porte déjà `in_macro` — l'information est là, transportée,
inutilisée. C'est `can_undo` qui ne la consulte pas, et `AppState::can_undo`
qui le recopie tel quel : le menu propose une action qui échouera à coup sûr,
et l'utilisateur reçoit un message d'erreur pour avoir cliqué sur ce qu'on lui
offrait.

La documentation de `StatusMessage` sépare pourtant explicitement les deux
cas — « impossible pour l'instant » (annuler à vide) n'est **pas** une erreur,
alors qu'un chemin illisible en est une. Ici, une troisième situation —
« impossible tant qu'une macro est ouverte » — se présente sous le masque de la
seconde.

**Piste.** `can_undo: history.can_undo() && !history.in_macro()` dans
`build_snapshot` (et l'équivalent pour `can_redo`), ce qui grise l'entrée de
menu ; ou, si l'on préfère garder `HistoryState` comme un reflet fidèle de la
pile, laisser `can_undo` tel quel et faire consulter `in_macro` par
`AppState::can_undo`. Le premier est plus sûr — tout client de l'acteur en
profite, pas seulement le banc.

<a id="c7"></a>
### C7 — `check_name` accepte l'octet nul, les sauts de ligne et les marques de direction

**Sévérité : faible.** `crates/plotyx-core/src/name.rs` (`check_name`).
Reproduction : `b21_les_noms_de_controle_sont_refuses`.

La validation d'un nom tient en trois tests : non vide, sans espace de bordure,
sans caractère réservé par la grammaire de chemins (`[`, `]`, `!`, `/`). Tout
le reste passe :

```text
add_folder("A\nB")        → Ok    path_of → "/A\nB"
add_folder("A\tB")        → Ok    path_of → "/A\tB"
add_folder("A\0B")        → Ok    path_of → "/A\0B"
add_folder("A\u{202e}B")  → Ok    path_of → "/A\u{202e}B"   (inversion de sens)
add_folder("A\u{200b}B")  → Ok    path_of → "/A\u{200b}B"   (espace de largeur nulle)
add_folder("x" × 100 000) → Ok    path_of → 100 001 caractères
```

Ces noms sont acceptés, persistés, et ressortent tels quels de `path_of` et de
chaque message d'erreur qui les cite. Conséquences, par ordre de nuisance :

- un **octet nul** ou un **saut de ligne** casse tout affichage tabulaire (la
  grille, l'explorateur de feuilles) et toute ligne de journal qui nomme
  l'objet ;
- une **marque d'inversion de sens** (`U+202E`) retourne visuellement le reste
  du chemin dans n'importe quelle interface qui le rend — deux fichiers
  différents peuvent alors s'afficher à l'identique ;
- un **espace de largeur nulle** rend deux noms visuellement identiques mais
  distincts pour le modèle : l'unicité des noms de fenêtre devient
  invérifiable à l'œil ;
- **aucune borne de longueur** n'existe, ni ici ni sur `unit`, `long_name`,
  `comment`, `display_format`, ni sur le texte d'une note. Un nom de cent
  mille caractères traverse le disque sans broncher.

**Ce n'est pas une faille de sécurité**, et ce rapport ne la présente pas comme
telle : le périmètre est le fonctionnement du logiciel, et le dommage est un
affichage cassé, pas un accès obtenu.

**Piste.** Refuser toute `char` de catégorie `Cc` (contrôle) et `Cf`
(formatage — c'est celle des marques de direction), ce qui est trois lignes
avec `char::is_control()` et un test sur la plage `U+200B..U+200F` /
`U+202A..U+202E` ; et poser une borne de longueur nommée (256 caractères est
généreux pour un nom d'objet). La table des caractères réservés existe déjà
(`RESERVED_NAME_CHARS`) : c'est le bon endroit, et le test unitaire de
`name.rs` est déjà écrit dans la forme qu'il faut.

---

## 6. D — Coût et passage à l'échelle

Ni corruption, ni plantage : deux opérations quotidiennes dont le coût croît
bien plus vite que le projet. Elles entrent dans ce rapport au titre de la
quatrième stratégie du cahier des charges de ce banc — « épuisement de
ressources en usage normal, sur des volumes réalistes mais élevés ».

**Comment ces deux-là se reproduisent.** Un budget en millisecondes dépend de
la machine, de la charge et du profil de compilation : mis en CI, il devient
soit trop lâche pour dire quelque chose, soit instable. `b17` installe donc un
**allocateur global compteur** (pour son seul binaire de test, et par fil pour
rester parallélisable) et mesure des **octets alloués**, qui sont
déterministes. La forme de l'assertion suit : jamais « moins de X octets »,
toujours « ce coût ne doit pas croître avec la taille du projet ». Les chiffres
en millisecondes ci-dessous sont des mesures hors test, en `--release`, données
pour l'ordre de grandeur.

<a id="d1"></a>
### D1 — Le nom automatique d'une colonne est cherché en O(n³) pour la feuille

**Sévérité : moyenne.** `crates/plotyx-core/src/project.rs`
(`next_column_name`, et `next_child_name` / `next_window_name` qui ont la même
forme). Reproduction :
`b17_nommer_une_colonne_automatiquement_ne_coute_pas_plus_que_la_nommer`.

`next_column_name` essaie `A`, `B`, … dans l'ordre et compare **chaque
candidat à tous les noms déjà pris**. Pour la k-ième colonne, c'est O(k²) ;
pour la feuille entière, O(n³).

```text
octets alloués (déterministes, mesurés par b17)
  100 colonnes : nom automatique     296 438   nom imposé    32 788    (9×)
  200 colonnes : nom automatique   1 141 470   nom imposé    65 004   (18×)
  400 colonnes : nom automatique   4 480 542   nom imposé   129 420   (35×)
                                                  le facteur DOUBLE quand n double

temps, hors test, en --release
    500 colonnes :      44 ms
  1 000 colonnes :     278 ms
  2 000 colonnes :   1 822 ms
  4 000 colonnes :  17 809 ms        contre 191 ms à nom imposé — 93×
```

`Project::add_column(sheet, _, None, _)` est exactement ce que fait le bouton
« Ajouter une colonne » du banc, et ce que fera un script qui crée ses colonnes
sans les nommer. Une acquisition de 4 000 voies — un spectromètre, une caméra
multicanaux — bloque l'application dix-huit secondes ; 10 000 la bloqueraient
plusieurs minutes.

**À ne pas confondre avec ce que le matin a mesuré.** Le rapport du matin note
« 2 000 colonnes dans une feuille rechargées sans ralentissement notable » :
c'est vrai, et c'est l'autre côté. Le **chargement** reconstruit l'arbre depuis
des noms déjà connus (`from_nodes`) et ne cherche aucun nom libre. Seule la
**création** paie.

**Piste.** Le nom libre se trouve en O(1) amorti avec l'ensemble des noms pris
construit une fois (`HashSet<&str>` sur les enfants de la feuille, puis un
essai par candidat), ou mieux : en repartant du **nombre d'enfants** plutôt que
de zéro — `spreadsheet_name(children.len())` est libre dans le cas courant, et
la boucle ne sert plus que de filet. Les trois fonctions
(`next_column_name`, `next_child_name`, `next_window_name`) ont la même forme
et le même correctif ; `next_window_name` est la pire des trois, puisqu'elle
appelle `tree.windows()` — donc `walk_all()` sur tout le projet — à **chaque
candidat** (2 000 classeurs : 2 256 ms).

<a id="d2"></a>
### D2 — `resolve_path` alloue proportionnellement au nombre total de nœuds du projet

**Sévérité : moyenne.** `crates/plotyx-core/src/tree.rs` (`find_window` →
`windows()` → `walk_all()`). Reproduction :
`b17_resoudre_un_chemin_ne_depend_pas_de_la_taille_du_projet`.

Résoudre `[Book1]Sheet1!c0` devrait coûter la profondeur du chemin : trouver
une fenêtre, une feuille, une colonne. Le nom d'une fenêtre étant unique à
l'échelle du projet, la recherche se fait par `find_window`, qui construit la
liste de **toutes** les fenêtres — donc parcourt et alloue **tout l'arbre** :

```rust
pub fn windows(&self) -> Vec<NodeId> {
    self.walk_all()                       // un Vec de TOUS les nœuds
        .into_iter()
        .filter(|id| … is_window())       // puis un second Vec
        .collect()
}
```

```text
octets alloués par résolution (déterministes, mesurés par b17)
     18 nœuds :     725 octets
  1 008 nœuds :  32 725 octets        ← strictement proportionnel, ~32 o/nœud

temps par résolution, hors test, en --release
    601 nœuds :   16 µs
  3 001 nœuds :   83 µs
 12 001 nœuds :  372 µs
 50 001 nœuds : 1 776 µs
```

Le chemin résolu est le même dans tous les cas, et la fenêtre visée est le
**premier** enfant de la racine : ce n'est pas un coût de recherche, c'est le
parcours complet, payé à chaque appel.

**Où cela se paie vraiment.** `Tree::resolve` est la porte d'entrée du pont
Python : `pyx.resolve(chemin)` la traverse à chaque appel, et c'est le geste
que fait un script avant chaque accès. Un script qui touche 10 000 colonnes
d'un projet de 50 000 nœuds passe **dix-huit secondes** en résolution de
chemins, sans qu'une seule valeur ait été lue. `check_name_available` paie la
même chose à chaque création de fenêtre (c'est la moitié de D1), et
`Project::path_of` s'en approche.

**Piste — deux options, chiffrées.**

| Option | Coût mémoire | Coût de développement | Détectabilité |
|---|---|---|---|
| **1. Un index `nom de fenêtre → NodeId`** dans `Tree`, maintenu par les quatre mutations qui touchent un nom de fenêtre (insertion, suppression, renommage, déplacement) | ~40 octets par fenêtre — quelques kilo-octets pour un projet énorme | moyen : quatre points de maintenance, et l'index doit survivre à `from_nodes` et à `Arc::make_mut` | **haute** si `b17` reste en garde-fou : tout chemin de mutation qui oublierait l'index ferait diverger la résolution, et `b14` le verrait |
| **2. Ne plus allouer : itérer au lieu de collecter** — `find_window` parcourt l'arbre en s'arrêtant au premier nom qui correspond, sans construire de `Vec` | nul | faible : quelques lignes, aucun invariant nouveau | moyenne — supprime les allocations, pas le parcours : le coût reste O(N), simplement 30 fois moins cher |

L'option 2 est le correctif immédiat et sans risque ; l'option 1 est le seul
qui change la classe de complexité, et elle ne vaut le coût de maintenance que
si les projets visés dépassent quelques milliers de nœuds. **Ce chiffre-là est
une décision de produit** : elle appartient à `questions-ouvertes.md`, pas à ce
rapport.

<a id="d3"></a>
### D3 — Chaque commande parcourt tout l'arbre et refait l'union des empreintes de tout l'historique

**Sévérité : moyenne.** `crates/plotyx-commands/src/history.rs`
(`History::evict`, `History::recompute_retained_bytes`,
`current_tree_pointers`). Reproductions :
`b21_le_cout_dune_ecriture_ne_depend_pas_de_lhistorique`,
`b21_le_cout_dune_ecriture_ne_depend_pas_du_nombre_de_colonnes`.

C'est la seule anomalie de ce rapport qui se paie sur le **chemin le plus
chaud du dépôt** : chaque commande, donc chaque frappe dans la grille, chaque
`pyx.set_column`.

```rust
fn push_pinned(&mut self, pinned: Pinned, project: &Project) {
    …
    self.undo.push(pinned);
    self.evict(project);                       // ← à chaque empilement
}

fn evict(&mut self, project: &Project) {
    self.recompute_retained_bytes(project);    // ← inconditionnel
    …
}

fn recompute_retained_bytes(&mut self, project: &Project) {
    let mut union = Footprint::new();
    for pinned in self.undo.iter().chain(self.redo.iter()) {
        union.extend(…);                       // O(entrées × colonnes par entrée)
    }
    let current = current_tree_pointers(project.tree());   // O(nœuds du projet)
    …
}
```

Une entrée **structurelle** retient deux arbres, donc l'empreinte de **toutes**
les colonnes du projet. Cent entrées structurelles dans un projet de deux mille
colonnes, et chaque commande refait l'union de deux cent mille éléments, puis
parcourt l'arbre entier pour la soustraction.

```text
octets alloués pour écrire UN nombre dans UNE cellule (déterministes, b21)
     50 colonnes,   0 entrée  :     3 839
     50 colonnes, 100 entrées :    45 533        ← ×12, l'historique seul
    500 colonnes,   0 entrée  :    37 503        ← ×10, le projet seul
    500 colonnes, 100 entrées :   159 481
  2 000 colonnes, 100 entrées :   357 145        ← un tiers de mégaoctet par frappe

temps, hors test, en --release, une écriture SetCells
    100 colonnes : historique vide     7,2 µs   —  plein     242,7 µs   (×34)
  1 000 colonnes : historique vide    41,3 µs   —  plein   1 637,5 µs   (×40)
  5 000 colonnes : historique vide   190,3 µs   —  plein   8 162,6 µs   (×43)
```

**Huit millisecondes pour taper un nombre** dans un projet de cinq mille
colonnes. Et le coût est payé même quand il ne peut servir à rien : avec un
historique vide, ou avec `byte_limit = 0` (illimité), la valeur recalculée
n'est consultée par aucune éviction.

**Ce n'est pas la comptabilité qui est fausse** — elle est juste, et le rapport
du matin a vérifié qu'elle l'était (`tests/memory_pinning.rs`). C'est sa
**fréquence** : une mesure exacte, refaite intégralement à chaque commande,
pour répondre à une question (« faut-il évincer ? ») dont la réponse est non
dans la quasi-totalité des cas.

**Piste — une borne supérieure bon marché, l'exact seulement si elle est
franchie.** Chaque entrée connaît déjà son empreinte au moment de son
empilement ; en retenir aussi **la somme** (un `usize` de plus dans `Pinned`)
donne un majorant du total retenu en O(entrées), sans toucher aux colonnes ni à
l'arbre. Si ce majorant ne dépasse pas `byte_limit`, aucune éviction n'est
possible : il n'y a rien à recalculer. Le calcul exact — l'union puis la
soustraction — ne tourne que lorsque le majorant franchit la limite, ou
lorsqu'un appelant lit [`History::retained_bytes`].

| Option | Coût mémoire | Coût de développement | Détectabilité |
|---|---|---|---|
| **1. Majorant en O(entrées), exact à la demande** | 8 octets par entrée (800 octets pour la pile par défaut) | faible : un champ sur `Pinned`, une garde au début d'`evict` | **haute** — `b21` devient garde-fou et mesure le coût réel d'une écriture ; le majorant ne peut que sur-estimer, donc jamais manquer une éviction |
| **2. Total maintenu incrémentalement** — ajouter à l'empilement, retrancher à l'éviction | 8 octets par pile | moyen : la déduplication par adresse contre l'arbre courant ne s'incrémente pas — il faudrait accepter que le total devienne approché | faible — un total approché qui dérive ne se remarque pas |
| **3. Ne rien changer et documenter le coût** | nul | nul | nulle — et le coût croît avec chaque projet réel |

L'option 1 garde la mesure exacte là où elle est lue, et supprime son coût là
où elle ne sert à rien. C'est le même arbitrage que celui déjà retenu pour
l'empreinte elle-même : « calculée une fois pour toutes au moment de
l'empilement ».

> **Clôture (2026-09-12).** Les deux moitiés de D3 sont maintenant fermées.
> La première (`History::evict`/`recompute_retained_bytes` recalculés en
> entier à chaque commande) a fermé avec l'option 1 ci-dessus — voir
> `b21_le_cout_dune_ecriture_ne_depend_pas_de_lhistorique`, désormais un
> garde-fou vert. La seconde, révisée après coup (voir
> `guide-de-correction-robustesse.md` §8) : une fois le majorant en place, le
> coût par colonne persistait ailleurs — `History::execute` clonait
> `project.tree()` dans `before` et le gardait vivant pendant toute
> l'application de la commande, ce qui forçait `Arc::make_mut` (vu par
> `Project::set_values` et consorts) à cloner la `Tree` entière plutôt que de
> muter en place. Fermée en cessant de cloner `before` pour les commandes de
> **données** (label et empreinte de `just_orphaned_by_write` capturés par
> emprunt, avant la mutation) — les commandes **structurelles** continuent de
> le cloner, leur `Entry::Structural` en a un besoin réel. Voir
> `History::execute` et `History::apply_data` dans `history.rs`, et
> `b21_le_cout_dune_ecriture_ne_depend_pas_du_nombre_de_colonnes`, désormais
> lui aussi un garde-fou vert.

---

## 7. Ce qui a tenu, cette fois

**L'aller-retour disque sous séquence tirée au sort.** Le garde-fou de `b14`
sauvegarde et recharge le projet **après chaque pas** de 8 tirages de 40 à 60
commandes, et compare deux choses : l'arbre (égalité binaire, `NaN` compris) et
les compteurs de version de toutes les colonnes. Aucun écart, sur environ 400
allers-retours. Le format traverse tout ce que le bus sait produire —
conversions de type, colonnes de longueurs libres, suppressions, annulations —
sans perdre un bit.

**Le compteur de version, colonne par colonne.** Tant qu'on reste dans la
colonne, le contrat est exactement celui qu'annonce sa documentation :
il avance à chaque écriture, ne bouge pas pour `set_meta`, ne bouge pas sur un
refus, avance sur un changement de type, et ne bouge pas pour les colonnes
voisines. A7 n'est pas un défaut de la colonne : c'est ce que l'historique
structurel lui fait par-dessus.

**Les lignes, sur des colonnes de longueurs inégales.** `DeleteRows` avec un
`count` qui déborde la fin de la colonne la plus courte est rabattu sans
erreur, et l'annulation rend à chaque colonne exactement ce qu'elle avait —
`b12_delete_rows_reste_annulable_sur_des_colonnes_de_longueurs_differentes`.
C'est le cas que l'asymétrie des longueurs rend retors, et il ne bronche pas.

**L'écriture au-delà de la fin.** `SetCells` à `start > len` remplit bien le
trou de valeurs manquantes, écrit la valeur au bon indice, et s'annule
exactement (`b12_setcells_ecrit_bien_au_dela_de_la_fin`). Seul le cas du bloc
**vide** dérape (C3).

**L'unicité globale des noms de fenêtre.** Deux fenêtres homonymes sont
refusées où qu'elles soient dans l'arbre — y compris dans deux dossiers
différents — à la mutation comme au chargement. C'est la moitié de la règle qui
tient ; A8 porte sur l'autre.

**Le code de production reste sans `unwrap`.** Une relecture de tous les
`unwrap`/`expect` des six crates du socle ne trouve que des tests. Les seuls
paniques atteignables restent celles des débordements arithmétiques déjà
consignées (B1, B2).

**Le format tient l'échelle, et il la tient bien.** Un million de lignes sur
deux colonnes : fichier de **15,3 Mio**, soit exactement **1,00×** les données
brutes (2 × 10⁶ × 8 octets) — l'enveloppe MessagePack ne coûte rien, parce que
les buffers flottants la contournent en octets bruts. **17 ms** pour
sauvegarder, **14 ms** pour recharger, en `--release`. C'est la décision
« `serde_bytes` plutôt qu'un tableau MessagePack » qui paie, et elle paie
exactement ce qu'elle annonçait.

**L'aller-retour d'une saisie numérique.** Ouvrir une cellule, la refermer sans
rien changer, et retrouver la valeur **bit à bit** : vérifié pour la charge de
l'électron, le nombre d'Avogadro et `0.1 + 0.2`
(`b15_une_saisie_numerique_fait_laller_retour`). Le `Display` de `f64` est
illisible ([C4](#c4)) mais il est exact — la valeur ne se dégrade pas à chaque
passage. Une cellule numérique vidée devient bien `NaN` et non zéro ; un texte
qui n'est pas un nombre est refusé **sans rien écrire**, avec un message.

**Les références vraiment rompues sont vraiment détectables.** Une colonne
d'erreur supprimée, ou de type texte, donne `broken() = true` et
`missing = [YError]` — le mécanisme fait ce qu'il promet. [A10](#a10) ne porte
que sur le seul cas qu'il ne voit pas : la colonne qui existe, qui est
numérique, et qui est simplement plus courte.

**Écrire dans une colonne ne dépend pas de la taille du projet.** Le garde-fou
`b17_ecrire_dans_une_colonne_ne_depend_pas_de_la_taille_du_projet` mesure
`set_values` dans un projet de 18 nœuds et dans un de 1 008 : même coût. Le
chemin d'écriture, lui, est bien en O(bloc écrit) — [D2](#d2) ne concerne que
la **résolution** qui le précède.

**L'écriture atomique tient sa promesse, et elle tient la bonne.** Une
sauvegarde qui échoue (répertoire parent inexistant) ne laisse **aucun**
fichier derrière elle, et deux sauvegardes du même projet sont identiques octet
à octet — les deux garde-fous de `b20`. L'atomicité protège de l'interruption,
et elle le fait bien ; [B8](#b8) porte sur la concurrence, qui est un autre
problème et demande un autre mécanisme.

**Les deux mécanismes de compatibilité du format font ce qu'ils promettent.**
Un champ inconnu est bien silencieusement ignoré, un numéro de version
supérieur bien refusé avant que le corps ne soit touché (les deux garde-fous de
`b19`). [C5](#c5) ne porte pas sur un mécanisme qui échoue, mais sur un
troisième cas — la variante inconnue — qu'aucun des deux ne couvre.

**Construire une imbrication de macros ne déborde jamais.** `begin_macro` et
`end_macro` sont itératifs : la sonde en construit **200 000 niveaux** sur une
pile de 2 Mio sans incident. [B7](#b7) ne concerne que la traversée, et le
garde-fou `b18_une_macro_de_profondeur_raisonnable_sannule_dun_bloc` vérifie
que mille niveaux s'annulent d'un bloc, correctement.

**La comptabilité mémoire de l'historique est juste.** [D3](#d3) porte sur sa
fréquence, jamais sur son résultat : le garde-fou
`b21_le_cout_dune_ecriture_ne_depend_pas_de_la_longueur_de_la_colonne` vérifie
que le coût d'une écriture suit bien ce qu'on écrit, et
`tests/memory_pinning.rs` (hors banc) vérifiait déjà que les octets annoncés
sont les bons.

**Les noms que la grammaire réserve restent refusés.** Vide, bords blancs,
`[`, `]`, `!`, `/` : les huit cas du test unitaire de `name.rs` passent aussi
par le banc (`b21_les_noms_reserves_par_la_grammaire_restent_refuses`).
[C7](#c7) ne porte que sur ce qui n'a jamais été dans la liste.

**`load` se garde d'une macro ouverte, et l'abandon nettoie le modèle.** Les
deux garde-fous de `b18` le montrent : c'est parce que l'un des deux chemins
vers le disque est déjà correct que [A11](#a11) se lit comme une asymétrie et
non comme une décision.

---

## 8. Une précision sur une anomalie du matin

**A5 est plus atteignable que sa reproduction ne le laisse croire.** La
reproduction du matin utilise `[-f64::MAX, f64::MAX]`, ce qui donne l'impression
d'un cas de laboratoire. Ce n'est pas la valeur extrême qui déclenche le
défaut, c'est le **débordement de la soustraction** `last_edge - first_edge` :
`[1e308, -1e308]` suffit, et produit exactement les mêmes bornes `NaN` et le
même comptage faux.

```text
histogram(&[1e308, -1e308], 4)
  edges  = [NaN, inf, inf, inf, inf]
  counts = [2.0, 0.0, 0.0, 0.0]
```

Deux mesures physiques de signes opposés au-delà de 8,98 × 10³⁰⁷ suffisent. Ce
n'est toujours pas courant, mais ce n'est plus « seulement aux bornes du type ».
Rien à changer au correctif proposé (`width.is_finite()`), seulement à
l'estimation de sa priorité.

---

## 9. Rejouer le banc

Rien ne change par rapport au matin : les cinq nouvelles batteries entrent dans
les mêmes deux commandes, et le travail CI `robustesse` les affiche sans qu'il
y ait rien à y ajouter.

```sh
# garde-fous — entrent dans la suite habituelle, doivent rester verts
cargo test --workspace

# reproductions d'anomalies — échouent tant que l'anomalie n'est pas corrigée
cargo test -p plotyx-robustesse -- --ignored

# les douze batteries de ces quatre passes, seules
for b in b10_compteur_de_version b11_portee_des_noms b12_commandes_sans_effet \
         b13_bornes_du_spec b14_balayage b15_cellules_et_saisie \
         b16_series_resolues b17_cout_des_operations b18_macro_ouverte \
         b19_compatibilite_du_format b20_fichier_partage b21_cout_et_identite ; do
    cargo test -p plotyx-robustesse --test "$b" -- --ignored
done

# B4 : hors test, la sonde provoque un abort() volontaire
( ulimit -v 4194304; cargo run --bin plotyx-sonde-alloc -- histogram_bins 4294967295 )

# B7 : hors test aussi — un débordement de pile emporte le processus entier.
# `construire` montre que bâtir l'imbrication ne déborde jamais ; les trois
# autres gestes la parcourent. Le troisième argument est la pile en octets
# (défaut : 2 Mio, celle du fil de l'acteur).
cargo run --release --bin plotyx-sonde-pile -- 200000 construire   # FIN NORMALE
cargo run --release --bin plotyx-sonde-pile --  10891 annuler      # abort()
cargo run --release --bin plotyx-sonde-pile --  26148 vider        # abort()
```

Aucune des douze batteries n'exige la feature `python` ni l'interpréteur
vendorisé. `b17` et `b21` installent chacune un allocateur global compteur : il
ne vaut que pour leur propre binaire de test, et ne change rien au reste du
dépôt.

---

## 10. Deux constats annexes

**`cargo fmt --all -- --check` échoue toujours sur le dépôt**, pour les treize
emplacements antérieurs que le rapport du matin liste déjà. Rien n'a été
reformaté ici non plus, pour la même raison ; `cargo fmt -p plotyx-robustesse`
et `cargo clippy -p plotyx-robustesse --all-targets -- -D warnings` sont
propres sur les douze fichiers ajoutés et les deux sondes.

**La suite de tests du dépôt est restée verte du début à la fin.** `cargo test
--workspace` passe avant comme après (garde-fous compris, reproductions
`#[ignore]`) ; les 15 reproductions `#[ignore]` du banc sont désormais 46, dont
43 se rejouent sans la feature `python` (contre 12 auparavant).

---

## Voir aussi

- `guide-de-correction-robustesse.md` — le **plan de travail** des trente
  anomalies des deux rapports : neuf familles de cause, l'ordre qui minimise
  le travail refait, et ce qu'il y a à décider avant d'écrire la première
  ligne. Ce rapport reste le compte rendu ; le guide dit quoi en faire.
- `rapport-robustesse-2026-09-11.md` — la première passe du même jour ; A1 à C2,
  batteries 1 à 9, frontière Python comprise.
- `cahier-des-charges-technique.md` — fait foi sur l'architecture ; aucune de
  ses décisions n'est mise en cause ici non plus.
- `questions-ouvertes.md` — quatre points y ont leur place naturelle, parce que
  ce sont des décisions de produit et non des correctifs : A8 (portée d'unicité
  des noms), la seconde moitié de B5 (`at == len` ajoute-t-il une ligne ?), la
  convention d'affichage d'une chaîne vide présente (A9, option 2) et la taille
  de projet que la résolution de chemin doit viser (D2, option 1), et
  lesquelles des huit requêtes de session une macro ouverte doit suspendre
  (A11, dont `Save` n'est que le premier cas).
- `docs/newarchitecture/roadmap-rendu.md` — A10 et B4 sont les deux anomalies
  que ce chantier réveillera : la première fixe la sémantique d'une série
  incomplète, la seconde la borne d'un paramètre que le rendu allouera.
- `format-fichier-plx.md` — B4 rejoint B1, B2 et C1 : quatre anomalies sur ce
  que le lecteur accepte sans le borner. C5 et B8 y ont aussi leur place, mais
  côté **écriture** : la règle du numéro de version selon qu'on ajoute un champ
  ou une variante, et le compteur de sauvegarde qui manque à l'en-tête.
- `crates/plotyx-robustesse/src/lib.rs` — les deux natures de test, et la
  convention `#[ignore]`.
