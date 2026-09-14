# Rapport de robustesse du socle — 2026-09-11

Statut : **compte rendu daté, non corrigé rétroactivement** — même règle que
`resume-decisions.md`. Il consigne l'état du socle au commit `b88e9ff` de la
branche `rewriterust`. Les anomalies qu'il décrit ne sont **pas** corrigées
par ce document ; chacune porte un identifiant (A1, B2, …) qui est aussi
l'étiquette de sa reproduction dans `crates/plotyx-robustesse`.

**Ce que ce document n'est pas.** Ni une revue d'architecture — aucune des
quatre décisions du §2 du `cahier-des-charges-technique.md` n'est mise en
cause, et rien ici ne les rouvre — ni une liste de souhaits. Uniquement des
comportements observés, chacun reproductible par un test nommé.

Onze anomalies, dont **six produisent un résultat faux sans lever la moindre
erreur**. C'est cette catégorie qui est traitée en premier : un plantage se
remarque, une corruption silencieuse se découvre trois mois plus tard dans un
graphe publié.

---

## 0. Comment lire ce document, et le banc qui va avec

Le banc vit dans `crates/plotyx-robustesse`, membre du workspace depuis ce
jour. Il ne contient aucun code de production. Il porte **deux natures de
test**, et la distinction est la clé de tout ce qui suit :

| Nature | État | Où elle est lancée |
|---|---|---|
| **Garde-fou** | passe, et doit continuer à passer | `cargo test --workspace`, comme tout le reste |
| **Reproduction** | échoue délibérément | `#[ignore]`, travail CI `robustesse` (non bloquant) |

Une reproduction n'est pas un test cassé : c'est l'énoncé exécutable d'une
anomalie. Son attribut porte son identifiant et sa raison :

```rust
#[test]
#[ignore = "reproduction A2 — le contrôle de longueur du pont porte sur un instantané périmé"]
fn b6_toctou_entre_instantane_et_commande() { … }
```

**Pourquoi `#[ignore]` plutôt que laisser le pipeline au rouge.** Les deux
options perdent quelque chose. Quinze tests rouges en permanence retirent au
pipeline toute valeur de signal — au bout d'une semaine, plus personne ne
regarde, et la seizième régression passe inaperçue. Les supprimer en échange
d'une liste en prose perd la reproduction, c'est-à-dire la seule chose qui
distingue un rapport d'une opinion. Le compromis retenu garde les deux : les
reproductions restent du code exécutable, et un travail CI dédié
(`allow_failure: true`, à l'image de `test-python`) les affiche à chaque
pipeline sans le bloquer.

**Quand une reproduction passe au vert**, le correctif est bon : retirer son
`#[ignore]`, et le garde-fou prend le relais dans le travail `test`.

---

## 1. Périmètre éprouvé

Neuf batteries, 94 cas, exécutées en local contre les crates du dépôt et
l'interpréteur vendorisé.

| Batterie | Fichier | Ce qu'elle éprouve |
|---|---|---|
| b1 | `b1_limites.rs` | valeurs limites : débordements, collections vides, `NaN`, `±inf`, dénormaux, `usize::MAX` |
| b2 | `b2_persistance.rs` | `.plx` malformés : corps aléatoires, troncatures, bits retournés ; fidélité de l'aller-retour |
| b3 | `b3_fichiers_hostiles.rs` | `.plx` **sémantiquement** hostiles : MessagePack valide, structure malveillante |
| b4 | `b4_allocateur.rs` | l'allocateur d'identifiants à la frontière `u64` |
| b5 | `b5_macros.rs` | macros imbriquées, avortement, pile de rétablissement sauvegardée |
| b6 | `b6_concurrence.rs` | clients concurrents de l'acteur de session |
| b7 | `b7_frontiere_python.rs` | conversions de types Rust ↔ Python, remontée des erreurs *(feature `python`)* |
| b8 | `b8_proprietes.rs` | huit propriétés × 2 000 cas (proptest) |
| b9 | `b9_script_sous_contrainte.rs` | fil de script sous contrainte : course, interruption, vues retenues *(feature `python`)* |

Plus une sonde hors-processus, `src/bin/sonde_alloc.rs` : le cas qu'elle
éprouve se termine par un `abort()` que rien ne rattrape, il ne pouvait donc
pas vivre dans un binaire de test.

**Environnement.** rustc 1.98.1 · CPython 3.12.14 vendorisé · numpy 2.5.3 ·
PyO3 0.29.2 · rust-numpy 0.29.0 · Linux x86_64. La suite de tests du dépôt
était verte au départ (≈ 300 cas) et l'est restée du début à la fin.

---

## 2. Récapitulatif

| Id | Anomalie | Composant | Sévérité |
|---|---|---|---|
| [A1](#a1) | Un tableau numpy masqué écrit les valeurs que le masque cachait | `plotyx-python` | **haute** |
| [A2](#a2) | `pyx.set_column` allonge la colonne malgré sa promesse | `plotyx-script` | **haute** |
| [A3](#a3) | Une poignée survit à un rechargement et désigne un autre objet | `plotyx-session` | **haute** |
| [A4](#a4) | La macro d'un client défait le travail validé d'un autre | `plotyx-session` | **haute** |
| [A5](#a5) | `histogram` rend des bornes `NaN` et un comptage faux | `plotyx-core` | moyenne |
| [A6](#a6) | `bar_width` rend la largeur demandée sans la filtrer | `plotyx-core` | faible |
| [B1](#b1) | L'allocateur d'identifiants déborde, puis recule | `plotyx-core` | **haute** |
| [B2](#b2) | `PlotSpec::rows` déborde sur une position venue d'un fichier | `plotyx-plot` | moyenne |
| [B3](#b3) | Un paramètre non borné fait `abort()` le processus | `plotyx-core` | moyenne |
| [C1](#c1) | La version de format d'un `PlotSpec` n'est jamais vérifiée | `plotyx-persistence` | faible |
| [C2](#c2) | Un texte hors bornes devient `+inf` en silence | `plotyx-core` | faible |

**Latence.** A5, A6 et B2 portent sur du code que rien n'appelle encore —
`histogram`, `bar_width` et `rows()`/`columns()` ne sont consommés que par
leurs tests. Ce sont précisément les fonctions que le chantier de rendu
(`docs/newarchitecture/roadmap-rendu.md`) va appeler : elles coûtent trois
lignes aujourd'hui, une enquête le jour où un graphe sort faux.

---

## 3. A — Corruptions silencieuses

Aucune erreur levée, aucune trace, un résultat faux. Rien dans l'application
ne permet aujourd'hui de les remarquer.

<a id="a1"></a>
### A1 — Un tableau numpy masqué écrit les valeurs que le masque cachait

**Sévérité : haute.** `crates/plotyx-python/src/array.rs:185`
(`copy_array_to_vec`). Reproductions :
`b9_tableau_masque_ecrase_les_cellules_manquantes_par_des_valeurs`,
`b7_tableau_masque_perd_son_masque`.

Un `np.ma.masked_array` est une sous-classe d'`ndarray`. Les trois contrôles
du pont passent donc tous — c'est bien un tableau, `ndim == 1`, dtype
`float64` — et `as_array()` lit le buffer `.data`, c'est-à-dire exactement ce
que le masque est censé cacher.

```python
m = np.ma.masked_where(qualite < 3, mesures)
# masque       : [False, True, False, True, False]
# vu de Python : [10.0 --  30.0 --  50.0]
# moyenne      : 30.0
pyx.set_column(h, m)          # accepté, sans exception
```

```text
modèle : [10.0, 20.0, 30.0, 40.0, 50.0]
                ^^^^        ^^^^   les deux mesures écartées, écrites comme des données
```

Ce n'est pas un cas tordu : `np.ma` est *l'*idiome numpy pour « mesure à
écarter », et le domaine de PlotYx est précisément la mesure scientifique. Le
modèle a par ailleurs une valeur manquante parfaitement définie (`NaN` pour
les colonnes numériques, cf. la doc de tête de `column.rs`) dans laquelle le
masque aurait dû se traduire.

**Piste.** Trancher explicitement, dans un sens ou dans l'autre : refuser le
tableau masqué (`InvalidValueError`, en testant `np.ma.isMaskedArray`), ou le
convertir côté hôte par `m.filled(np.nan)` — le masque devient alors la valeur
manquante du modèle, ce qui est le sens du domaine. Ce qu'il ne faut pas
garder, c'est la troisième voie actuelle : accepter en ignorant.

**Note de conception.** Le module refuse déjà explicitement `int64` « plutôt
que de convertir en silence — une conversion implicite cacherait une perte de
précision au-delà de 2⁵³ ». Le raisonnement s'applique mot pour mot au masque,
qui est une perte d'information de même nature.

<a id="a2"></a>
### A2 — `pyx.set_column` allonge la colonne malgré sa promesse

**Sévérité : haute.** `crates/plotyx-script/src/bridge.rs:133-160`.
Reproductions : `b6_toctou_entre_instantane_et_commande` (déterministe),
`b9_course_entre_script_et_interface_sur_la_longueur` (bout en bout).

La surface promet, et sa documentation insiste : « la longueur doit être
**exactement** celle de la colonne […] Toute autre longueur lève
`InvalidValueError`, **sans envoyer la moindre commande** : la révision du
projet ne bouge pas. »

Le contrôle est fait sur un `Snapshot`. Entre `session.snapshot()` et
`session.execute()` il y a deux allers-retours de canal, pendant lesquels
l'acteur sert d'autres clients — c'est le principe même du bus FIFO.
`Command::SetCells` n'a, lui, aucune notion de longueur attendue : il étend la
colonne sans broncher.

```text
// déterministe, au niveau session (b6)
snapshot       → colonne de 10 lignes, 10 valeurs préparées
autre client   → DeleteRows(at: 5, count: 5)     → colonne à 5 lignes
execute        → SetCells(start: 0, 10 valeurs)  → colonne à 10 lignes

// bout en bout, moteur de script réel contre l'interface (b9)
400 écritures de script × insertions/suppressions de ligne concurrentes
dérive observée sur trois essais consécutifs : +5, +6, +53 lignes
exceptions levées : 0
```

**Piste.** Toute vérification faite hors du fil de l'acteur est périmée par
construction — c'est le corollaire direct de la décision « l'acteur détient le
seul `&mut Project` du processus ». Porter l'attente **dans** la commande : un
champ `expected_len: Option<usize>` sur `SetCells`, ou une commande
`ReplaceColumnValues` dédiée, que l'acteur vérifie au moment de l'appliquer.
Le pont ne fait alors plus que traduire l'erreur, ce qui est son rôle.

**Portée au-delà du pont.** Le même motif — lire un instantané, décider, puis
envoyer — est disponible à tout client de `SessionHandle`. Le pont est le
premier à l'utiliser, il ne sera pas le dernier.

<a id="a3"></a>
### A3 — Une poignée survit à un rechargement et désigne un autre objet

**Sévérité : haute.** `crates/plotyx-session/src/actor.rs` (`load_project`) et
`crates/plotyx-script/src/surface.rs` (`Handle`). Reproduction :
`b6_poignee_survit_a_un_load_et_designe_un_autre_objet`.

`load` remplace le projet, et les identifiants repartent des valeurs du
fichier. Deux projets de même forme réutilisent donc les mêmes `NodeId`. Une
poignée détenue par un script pointe, après un *Fichier > Ouvrir*, sur un nœud
du **nouveau** projet — et l'écriture est acceptée.

```text
h = poignée sur [Book1]Sheet1!C0   (nœud 4, valeurs [0, 0, 0, 0, 0])
session.load("autre.plx")          // autre projet, même forme

lecture par h  → [-7.0, -8.0, -9.0, -10.0, -11.0]   // données de l'autre projet
écriture par h → Ok                                  // aucun BrokenReferenceError
```

La documentation de `Handle` énumère ce à quoi une poignée survit — renommage,
déplacement, suppression **suivie d'une annulation** — et le seul cas de
rupture qu'elle nomme est « entre la suppression et l'annulation ». Le
rechargement n'y figure ni comme survie ni comme rupture. La doc de
`plotyx-session` est plus juste (« un ancien instantané n'a alors plus aucun
rapport avec l'arbre courant, y compris ses identifiants de nœud ») mais n'en
tire aucune conséquence exécutable.

**Piste.** Une identité de projet — un compteur d'époque incrémenté à chaque
`load` — portée par `Snapshot` et estampillée dans `Handle`. Toute poignée
d'une époque antérieure lève `BrokenReferenceError`, qui est déjà le
vocabulaire annoncé et dont la description (« ou d'un projet rechargé »)
couvre littéralement ce cas. **La `revision` ne suffit pas** : elle dit qu'un
instantané est périmé, pas qu'une identité a changé de sens.

<a id="a4"></a>
### A4 — La macro d'un client défait le travail validé d'un autre

**Sévérité : haute.** `crates/plotyx-session/src/actor.rs`
(`Request::AbortMacro`). Reproduction :
`b6_macro_dun_client_avale_le_travail_dun_autre`.

La limite est documentée en tête de `plotyx-session` : une macro est globale à
la session, et les commandes d'autres clients y sont « absorbées comme si
elles en faisaient partie ». Sa **conséquence** ne l'est pas.

```text
UI     : begin_macro("import")
script : execute(SetCells …) → Ok(revision 4)   // écriture validée
         lecture → 42.0
UI     : abort_macro()                          // l'utilisateur renonce à l'import
script : lecture → 1000.0                       // travail perdu, sans un mot
```

Le client victime a reçu un `Ok` avec un numéro de révision. Rien, dans le
protocole, ne lui signale ensuite que cette révision a été défaite.

**Piste.** L'identité de l'émetteur dans le protocole — la documentation du
crate l'envisage déjà et en donne le dessin. À défaut, refuser
`Err(MacroOpen)` la commande d'un émetteur qui n'a pas ouvert la macro en
cours : un refus visible vaut mieux qu'une absorption muette. Et dans
l'immédiat, à coût nul : écrire la conséquence dans la limite documentée, qui
décrit aujourd'hui le mécanisme sans dire ce qu'il fait perdre.

<a id="a5"></a>
### A5 — `histogram` rend des bornes `NaN` et un comptage faux

**Sévérité : moyenne** (latent). `crates/plotyx-core/src/resolve.rs:426-435`.
Reproduction : `b1_histogram_intervalle_infini_ne_produit_pas_de_nan`.

Quand l'étendue des données déborde `f64`, `width = last_edge - first_edge`
vaut `+inf`. La première borne devient `first + inf × 0`, soit `NaN`, et
toutes les positions `(v - first) / inf` s'écrasent sur 0 : tous les points
tombent dans la première classe.

```rust
histogram(&[-f64::MAX, f64::MAX], NonZeroU32::new(4).unwrap())
// edges  = [NaN, inf, inf, inf, inf]
// counts = [2.0, 0.0, 0.0, 0.0]
```

Le cas dégénéré `min == max` est déjà traité juste au-dessus, avec la bonne
référence (« `numpy.histogram` élargit un intervalle dégénéré de 0,5 de chaque
côté plutôt que de diviser par une largeur nulle »). C'est l'autre extrémité
du même axe qui manque.

**Piste.** Tester `width.is_finite()` après la soustraction et, sinon,
calculer les bornes en deux moitiés (`first / 2` et `last / 2`, puis doubler
le pas) ou refuser explicitement.

<a id="a6"></a>
### A6 — `bar_width` rend la largeur demandée sans la filtrer

**Sévérité : faible** (latent). `crates/plotyx-core/src/resolve.rs:362`.
Reproduction : `b1_bar_width_demandee_absurde_est_filtree`.

La documentation de la fonction annonce « celle demandée **si elle est
positive**, sinon 80 % du pas médian ». Le code rend `requested`
inconditionnellement.

```text
bar_width(Some(&[0.0, 1.0, 2.0, 3.0]), Some(-5.0))  → -5.0
                                        Some(0.0)   →  0.0
                                        Some(NaN)   →  NaN
                                        Some(inf)   →  inf
```

Le commentaire de la fonction explique que le passage à `Option<f64>` a rendu
le test `> 0` inutile. Il a en réalité supprimé le test de **sentinelle** (le
zéro qui signifiait « déduis-la »), pas celui de **validité**.

**Piste.** Filtrer (`requested.filter(|w| w.is_finite() && *w > 0.0)`), ce qui
rend la doc exacte ; ou confier l'invariant au type, dans l'esprit du
`NonZeroU32` retenu pour `histogram_bins`. La solution à ne pas garder est
celle où la doc et le code disent deux choses différentes.

---

## 4. B — Plantage et état incohérent

<a id="b1"></a>
### B1 — L'allocateur d'identifiants déborde, puis recule

**Sévérité : haute.** `crates/plotyx-core/src/id.rs:43` (`IdAllocator::next`).
Reproductions : `b4_allocateur_sature_puis_deborde`,
`b4_projet_charge_avec_identifiant_max_puis_creation`,
`b3_identifiant_zero_et_max` (par le disque).

Un fichier `.plx` contenant un nœud d'identifiant `u64::MAX` est **accepté**
au chargement : rien ne borne les identifiants à la lecture. `IdAllocator::
resume` sature alors à `u64::MAX` — comportement voulu, et un test unitaire du
crate le vérifie. Mais ce test ne regarde que `peek()` ; le premier `next()`
exécute `self.next += 1` sur `u64::MAX`.

```text
// debug
Project::add_folder(...)
  → panicked at crates/plotyx-core/src/id.rs:43: attempt to add with overflow
  → la panique a lieu SUR LE FIL DE L'ACTEUR : la session passe Disconnected
    et le projet ouvert n'est plus joignable

// release
peek() après le premier next() : 0              // le compteur RECULE
add_folder → Err(structure : nœud 18446744073709551615 est déjà présent
             dans l'arbre)                      // et pour toujours
```

Le compteur qui recule contredit directement l'invariant central du type,
écrit dans sa propre documentation : « compteur monotone, jamais réutilisé sur
toute la vie d'un projet, rechargement compris ».

Le test existant s'appelle `resume_does_not_panic_at_the_u64_boundary` et
commente : « le crate s'interdit toute panique ». L'intention est la bonne ;
c'est la couverture qui s'arrête un appel trop tôt.

**Piste — deux verrous, pas un.**

1. Dans `next()`, un `checked_add` et une décision explicite sur l'épuisement :
   rendre `CoreResult<NodeId>` plutôt que paniquer ou boucler.
2. À la lecture du fichier, un plafond sur les identifiants **et** sur
   `next_id_hint`. Un projet réel n'atteint jamais 2⁶⁴ nœuds : une valeur qui
   en approche est la signature d'un fichier corrompu, pas d'un projet
   légitime — exactement le raisonnement qui a déjà produit
   `ConvertError::ZeroHistogramBins`.

<a id="b2"></a>
### B2 — `PlotSpec::rows` déborde sur une position venue d'un fichier

**Sévérité : moyenne** (latent). `crates/plotyx-plot/src/spec.rs:140` et
`:147` ; cast sans borne dans `crates/plotyx-persistence/src/convert.rs:396-397`.
Reproductions : `b3_panneau_en_position_extreme` (par le disque),
`b1_plotspec_rows_columns_sur_position_extreme`.

La conversion depuis le fichier fait `row: record.row as usize` sans aucune
borne, et `rows()` calcule `max + 1`. Un `.plx` déclarant un panneau en
`(u64::MAX, u64::MAX)` est accepté sans broncher ; c'est la première lecture
de la grille qui casse.

```text
chargement du fichier  → Ok
spec.panels[0].row     → 18446744073709551615

spec.rows()  // debug   → panicked at spec.rs:140: attempt to add with overflow
spec.rows()  // release → 0        // une grille de zéro ligne
```

**Piste.** `saturating_add(1)` ferme la panique, mais le vrai correctif est en
amont : borner `row` et `column` à la conversion, avec une variante de
`ConvertError` dédiée — le traitement déjà réservé à `ZeroHistogramBins`, dans
le même fichier, qui montre que le réflexe est en place ailleurs.

<a id="b3"></a>
### B3 — Un paramètre non borné fait `abort()` le processus

**Sévérité : moyenne.** `crates/plotyx-core/src/column.rs:274` (`write_block`),
`:495` (`set_length`), `:541` (`insert_rows`). Sonde :
`crates/plotyx-robustesse/src/bin/sonde_alloc.rs`.

`start` et `count` dimensionnent directement une allocation. Un échec
d'allocation en Rust n'est **pas** une panique rattrapable : c'est un
`abort()`. Ni l'acteur, ni l'interface, ni un `catch_unwind` ne peuvent
intervenir — le processus meurt avec tout le travail non sauvegardé.

```text
SetCells { column, start: 10_000_000_000, values: [7.0] }  sur une colonne de 3 lignes

memory allocation of 80000000000 bytes failed
Aborted (core dumped)          // code de sortie 134, pas une panique

idem : set_length(n) · insert_rows(sheet, at, count)
```

**Atteignabilité, honnêtement.** La coquille UI actuelle code `count: 1` en
dur (`crates/plotyx-app/src/app.rs:248`) : rien n'est atteignable depuis
l'interface d'aujourd'hui. Mais `Command` est la frontière publique du bus —
« une valeur, sans état ni emprunt, faite pour traverser un canal
inter-threads » — et `start`/`count` y sont des `usize` libres. La première
boîte de dialogue « insérer N lignes » ouvre le chemin.

**Piste.** Un plafond de lignes explicite et nommé dans `Project` (ou à
l'entrée de `History::apply`), rendant `IndexOutOfBounds` au-delà. Une limite
arbitraire mais documentée vaut infiniment mieux qu'un `abort()` : elle laisse
une boîte de dialogue et un projet intact.

---

## 5. C — Format et compatibilité

Deux décisions à prendre plutôt que deux bogues : dans les deux cas le
comportement actuel est défendable, mais il n'est écrit nulle part.

<a id="c1"></a>
### C1 — La version de format d'un `PlotSpec` n'est jamais vérifiée

**Sévérité : faible.** `crates/plotyx-persistence/src/convert.rs:425`.
Reproduction : `b3_version_de_format_de_spec_inconnue_est_acceptee_en_silence`.

L'en-tête du fichier est bien contrôlé : une version supérieure est refusée
« sans que son corps soit lu ». Le `format_version` du `PlotSpec`, lui, est
relu et recopié tel quel. Un spec de version 999 se charge en silence alors
que le lecteur connaît la version 1.

Or ce champ existe pour une raison précise, écrite dans `spec.rs` : il est
« indépendant de la version de schéma du fichier projet : un `PlotSpec` peut
aussi voyager par presse-papiers ou par modèle sans passer par un fichier ».
Un spec qui voyage hors fichier ne bénéficie d'aucun contrôle d'en-tête.

**Piste.** Appliquer au spec la règle déjà appliquée au fichier : refuser
au-delà de `SPEC_FORMAT_VERSION`. Sans quoi le champ est du poids mort qui
donne une fausse assurance de compatibilité — le jour où une v2 change le sens
d'un champ, un lecteur v1 lira du v2 comme si de rien n'était.

<a id="c2"></a>
### C2 — Un texte hors bornes devient `+inf` en silence

**Sévérité : faible.** `crates/plotyx-core/src/column.rs`
(`set_column_type`, branche `Text → Float`). Reproduction :
`b1_conversion_texte_vers_numerique_puis_retour`.

```text
"1e400" → inf     "inf" → inf     "NaN" → NaN (manquant)
"  2.5  " → 2.5   "" → NaN        "pas un nombre" → NaN
```

C'est le comportement de `str::parse::<f64>`, et aussi celui de `float()` en
Python : cohérent avec l'écosystème. Mais `inf` n'est pas « manquant » au sens
du modèle, et rien ne signale la perte.

À l'inverse — et c'est le résultat rassurant du même test — l'aller-retour
`Numérique → Texte → Numérique` s'est révélé **parfaitement fidèle** :
`-0.0`, dénormaux (`1e-320`), `f64::MAX`, `f64::MIN` et `0.1 + 0.2` reviennent
tous bit à bit identiques.

**Piste.** Trancher et l'écrire dans la table de conversion de
`set_column_type`, qui documente déjà les autres cas. Si `inf` doit rester
légitime, le dire ; sinon, `.filter(|v| v.is_finite())` avant de retomber sur
`NaN`.

---

## 6. Ce qui a tenu

Autant que les anomalies, ces résultats disent où le socle est solide — et où
il est inutile de revenir.

**La persistance face à la corruption.** 2 000 corps aléatoires, toutes les
troncatures d'un fichier valide octet par octet, et chaque bit retourné à
chaque position : aucune panique, aucun fichier tronqué accepté.

**La fidélité de l'aller-retour disque.** Charge utile d'un `NaN` conservée
bit à bit (`0x7ff4_0000_dead_beef`), `-0.0`, dénormaux, compteur `version`,
texte vide distinct d'absent, octet nul et emoji dans une chaîne. Deux
sauvegardes du même projet sont identiques octet à octet, comme promis.

**Les structures d'arbre hostiles.** Cycle de parenté, nœud son propre parent,
enfant listé deux fois, colonne rangée sous un dossier, deux fenêtres
homonymes, buffer flottant de 13 octets, type de colonne contredit par ses
valeurs, noms invalides : tous refusés, chacun avec un message qui nomme le
fautif. Et aucune boucle infinie sur un cycle — la validation croisée
parent ↔ enfants l'attrape avant que `walk` ne soit appelé.

**Les macros de l'historique.** Huit scénarios sur huit : avortement sans
trace avec pile de rétablissement restaurée, imbrication à 64 niveaux, macro
vide, commandes refusées en son sein, `undo`/`redo`/`clear` correctement
refusés pendant. C'est la partie la plus retorse de l'historique — `saved_redo`
et l'imbrication — et elle ne bronche pas.

**La discipline de type Python → Rust.** Treize types erronés sur treize
refusés : liste, entier, chaîne, `None`, `int64`, `float32`, 2-D, scalaire,
0-D, complexe, objets, longueurs fausses — et le cas attendu au piège, le
tableau **big-endian `>f8`**, dont le `dtype` est équivalent mais l'ordre
inverse. `is_equiv_to` le rejette correctement ; l'hypothèse d'une lecture à
l'envers était fausse.

**Le zéro-copie et sa clause de sûreté.** Vue en lecture seule
(`writeable False`, base `FloatBufferOwner`), stable sous 200 écritures
concurrentes, 300 vues retenues simultanément sans incident. La garantie par
`Arc::make_mut` tient : jamais un accès invalide, seulement la valeur périmée
documentée.

**L'interruption d'un script.** Les deux moitiés du mécanisme fonctionnent —
boucle Python pure, et boucle qui avale les exceptions mais touche à `pyx`. Le
moteur reste utilisable ensuite, sans débordement de l'interruption sur le
script suivant.

**La sérialisation des clients de l'acteur.** 1 600 commandes concurrentes
depuis 8 fils : aucune perdue, révision exacte. Quatre fils annulant en même
temps annulent exactement les 100 crans. Un instantané reste figé sous 500
écritures.

**Les propriétés tirées au sort.** Huit propriétés × 2 000 cas : aller-retour
de chemin, idempotence de la forme canonique, `path_of` ∘ `resolve_path` =
identité, neutralité de écriture + annulation, insertion de lignes +
annulation, changement de type + annulation, aller-retour disque, noms
automatiques sans collision.

**La profondeur et la largeur.** 5 000 dossiers imbriqués sauvegardés puis
rechargés — la validation est itérative, la pile ne saute pas. 2 000 colonnes
dans une feuille rechargées sans ralentissement notable.

---

## 7. Rejouer le banc

```sh
# garde-fous — entrent dans la suite habituelle, doivent rester verts
cargo test --workspace

# reproductions d'anomalies — échouent tant que l'anomalie n'est pas corrigée
cargo test -p plotyx-robustesse -- --ignored

# frontière Python (garde-fous) — mêmes trois variables que d'habitude
scripts/python-test.sh

# frontière Python (reproductions A1, A2)
scripts/python-test.sh -- --ignored

# B3 : hors test, la sonde provoque un abort() volontaire
( ulimit -v 4194304; cargo run --bin plotyx-sonde-alloc -- insert_rows 10000000000 )
```

Chaque reproduction échoue sur l'assertion qui décrit le comportement
**attendu**, avec l'entrée exacte et l'état observé dans son message : un
correctif se valide en la faisant passer.

---

## 8. Deux constats annexes

Ni l'un ni l'autre n'est une anomalie ; les deux méritent d'être su.

**`cargo fmt --all -- --check` échoue déjà sur le dépôt, avant ce banc.**
Treize emplacements dans huit fichiers, tous antérieurs au commit `b88e9ff` :
`plotyx-app/tests/shell.rs`, `plotyx-core/src/{column,lib,project,resolve}.rs`,
`plotyx-core/tests/project_model.rs`,
`plotyx-persistence/{src/lib.rs,tests/roundtrip.rs}`. Le travail CI `fmt` est
donc rouge sur l'image `rust:1` (qui suit le stable courant). Ces fichiers
n'ont **pas** été reformatés ici : c'est hors du périmètre de ce banc, et un
reformatage de masse noierait la revue. `cargo fmt -p plotyx-robustesse` a été
appliqué au seul crate ajouté.

**`DeleteRows` puis `InsertRows` au niveau feuille n'est pas neutre.** Le
comportement est documenté (`Project::insert_rows` : « Une colonne trop courte
pour avoir une ligne `at` est ignorée : les longueurs sont libres entre
colonnes d'une même feuille, et allonger une colonne courte inventerait des
données »), et l'annulation, elle, est bien symétrique — elle passe par les
primitives par colonne, précisément pour cette raison. Mais pour l'utilisateur
qui enchaîne deux actions distinctes — supprimer la ligne 0, puis insérer une
ligne 0 — une colonne d'une seule ligne disparaît définitivement : la
suppression la vide, et l'insertion ne la rattrape plus. Observé
incidemment en montant le banc. À vérifier le jour où l'interface exposera ces
deux commandes côte à côte.

---

## Voir aussi

- `guide-de-correction-robustesse.md` — le **plan de travail** des trente
  anomalies des deux rapports : neuf familles de cause, l'ordre qui minimise
  le travail refait, et ce qu'il y a à décider avant d'écrire la première
  ligne. Ce rapport reste le compte rendu ; le guide dit quoi en faire.
- `cahier-des-charges-technique.md` — fait foi sur l'architecture ; aucune de
  ses décisions n'est mise en cause ici.
- `questions-ouvertes.md` — A3 (identité de projet) et A4 (identité de
  l'émetteur) y ont leur place naturelle si elles sont reportées.
- `format-fichier-plx.md` — B1, B2 et C1 portent toutes trois sur ce que le
  lecteur accepte sans le borner.
- `crates/plotyx-robustesse/src/lib.rs` — les deux natures de test, et la
  convention `#[ignore]`.
