# Tests et intégration continue

## Trois familles

| Famille | Où | Ce qu'elle garde |
|---|---|---|
| **tests unitaires** | `#[cfg(test)]` dans `src/` | un comportement local, au plus près de son code |
| **tests d'intégration** | `crates/*/tests/` | une propriété de crate : contrat public, frontière, accord entre chemins |
| **banc de robustesse** | `stokesforge-robustesse/tests/` | le socle aux limites — 21 batteries `b1`…`b21` |

## Les garde-fous à ne pas casser

Quelques tests ne vérifient pas un comportement mais **une propriété
d'architecture**. Ils échouent exprès quand quelqu'un franchit une frontière
par inadvertance, et ce sont eux qu'il faut comprendre avant de les modifier.

| Test | Ce qu'il tient |
|---|---|
| `*/tests/no_ui_dependency.rs` | liste blanche de dépendances : toute addition au `Cargo.toml` doit y être ajoutée explicitement (invariant 2). Lecture commune `stokesforge-arch-guard` : dépendances normales, de build, `target.*` et renommées (`package`) ; `dev-dependencies` exclues ; témoins négatifs dans `stokesforge-arch-guard/src/lib.rs` |
| `stokesforge-render-svg/tests/frontiere.rs` | l'export ne voit ni `egui`, ni `eframe`, ni `winit` — donc se teste sans serveur graphique |
| `stokesforge-render/tests/mesure.rs` | la source unique de mesure de texte (invariant 4) |
| `stokesforge-render-egui/tests/accord.rs` | écran et export placent les mêmes primitives aux mêmes pixels |
| `stokesforge-render-egui/tests/feathering.rs` | le feathering n'entre pas dans la clé de cache (invariant 6) |
| `stokesforge-app/tests/script_bridge_lazy.rs` | l'interpréteur Python ne démarre pas à l'ouverture de la fenêtre |
| `stokesforge-app/tests/libelles.rs` | aucun texte d'interface en dur (`aucun_libelle_en_dur`, garde absolue, sans dérogation, #48) ; la zone #43 (`ZONE_43`) sans aucun littéral, ses fonctions cherchées par leur nom dans toutes les sources — une entrée introuvable échoue ; les dépendances de traduction restent dans `stokesforge-app` ; les témoins qui prouvent que la garde voit un littéral, y compris dans un sous-dossier de `src/` |
| `stokesforge-app/tests/gardes.rs` | aucune `egui::Window` brute (`aucune_fenetre_n_est_une_egui_window_brute`) ; la fenêtre des propriétés passe par `modal_dialog` → `modal_dialog_with` → `show_window_in_layer` ; aucun import global (`use …::*`) hors du code de test (`aucun_import_global_hors_des_tests`, décision 2 de #115, absolue depuis #117) ; les témoins du lecteur de sources |
| `stokesforge-app/tests/support/sources.rs` | pas un test : le **lecteur unique** de toutes les gardes qui lisent le code (`libelles.rs`, `history_text.rs`, `gardes.rs`, et par `src/lib.rs` celles d'`icons`, `theme`, `i18n`). Lit `src/` récursivement ; vue de production sans commentaires ni items `#[cfg(test)]` (chacun, pas seulement le premier ; un `#[cfg(test)] mod x;` rend son fichier entier de test) ; cherche une fonction par son nom, où qu'elle vive (#116). Une garde nouvelle qui lit le code passe par lui, jamais par `include_str!` ni un fichier nommé |
| `stokesforge-app/src/i18n.rs` (tests) | toute clé de `fr` existe dans `en` avec les mêmes arguments ; la pseudo-langue entoure et allonge chaque message ; complétude bloquante (toute clé de `en` existe dans `fr`) ; pluriels et typographie françaises ; une préférence `fr` ouvre l'interface en français ; choix de la langue sans panique ; `tests/language_switch.rs` : la langue change sans redémarrer |
| `stokesforge-commands/tests/memory_pinning.rs` | l'historique n'épingle pas indéfiniment de gros buffers |

### Le témoin, sans lequel un test d'égalité ne prouve rien

C'est la méthode la plus importante du dépôt, et elle vaut au-delà du rendu.

Un test qui compare deux chemins peut passer parce que les deux sont justes —
ou parce que **les deux sont faux de la même façon**. `tests/mesure.rs` et
`tests/accord.rs` portent donc, chacun, des cas qui doivent être **différents** :

- la même figure tracée avec le `SVGBackend` **nu**, celui qui retombe sur
  l'heuristique par octets UTF-8, doit se placer **ailleurs** ;
- pour chaque fonctionnalité ajoutée, une variante du graphe qui la change doit
  produire une figure **différente**.

Sans eux, une enveloppe qui déléguerait `estimate_text_size` par erreur
passerait pour correcte. Si vous touchez à ces tests, lisez leurs témoins
d'abord.

## Le banc de robustesse

`stokesforge-robustesse` ne contient **aucun code de production**. Il dépend de tout
le workspace et porte deux choses :

- **21 batteries** (`tests/b1…b21`) qui éprouvent le socle aux limites :
  valeurs numériques extrêmes (`b1`), persistance (`b2`), fichiers `.stf`
  malformés puis sémantiquement hostiles (`b3`), allocateur d'identifiants
  (`b4`), macros (`b5`), concurrence entre clients (`b6`), frontière
  Rust ↔ Python (`b7`), propriétés tirées au sort par proptest (`b8`), script
  sous contrainte (`b9`), compteur de version (`b10`), portée d'unicité des
  noms (`b11`), commandes acceptées sans effet (`b12`), bornes d'un `PlotSpec`
  venu du disque (`b13`), balayage revérifiant trois invariants après chaque pas
  d'une séquence tirée au sort (`b14`), traduction cellule ↔ texte (`b15`),
  séries aux colonnes de longueurs inégales (`b16`), coût des opérations quand
  le projet grandit (`b17`), ce qui reste possible pendant qu'une macro est
  ouverte (`b18`), fichier écrit par une version plus récente (`b19`), deux
  écrivains sur un même fichier (`b20`), coût et identité sur le chemin
  d'écriture (`b21`) ;
- **deux sondes hors-processus**, `sonde_alloc` et `sonde_pile`, parce que les
  cas qu'elles éprouvent se terminent par un `abort()` que rien ne rattrape :
  un échec d'allocation Rust n'est pas une panique rattrapable, et un
  débordement de pile non plus (le runtime écrit `has overflowed its stack` et
  appelle `abort()` — ni `catch_unwind` ni `join` ne rendent la main, et le
  binaire de test mourrait avec).

`sonde_pile` utilise par défaut une pile de 2 Mio : la taille qu'un
`std::thread` reçoit quand personne ne la fixe, c'est-à-dire celle du fil de
l'acteur.

### Deux natures de test, et comment les distinguer

**Les garde-fous** passent et doivent continuer à passer : ils entrent dans
`cargo test --workspace` comme les autres.

**Les reproductions** échouent délibérément, et sont marquées `#[ignore]` pour
que le pipeline garde sa valeur de signal — un CI rouge en permanence ne dit
plus rien. Une reproduction qui se met à **passer** est le signal attendu : le
correctif est bon, et c'est le moment de retirer son `#[ignore]` pour que le
garde-fou prenne le relais.

La campagne de correction issue des rapports du 2026-09-11 est close : il ne
reste qu'un `#[ignore]`, dans `b9`, dont le motif est écrit en toutes lettres
dans l'attribut (il dépend de l'ordonnancement, et son motif original ne prévoit
pas de boucler sur `ConcurrentWriteError`).

Mesurer plutôt que chronométrer : `b17` et `b21` comptent des **octets
alloués**, pas des millisecondes, pour que l'assertion dise « ce coût ne doit
pas croître avec X » et reste vraie sur n'importe quelle machine.

## Lancer

```sh
cargo test --workspace                        # tout, sauf #[ignore]
cargo test -p <crate> --test <fichier>        # un fichier de tests
cargo test -p stokesforge-robustesse -- --ignored  # reproductions et bancs coûteux
# (Typst est compilé et testé par `cargo test --workspace` ; `--all-features` n'active plus que Python : voir .codex/README.md)
scripts/python-test.sh                        # pont Python, scripts, robustesse, application et CLI
scripts/python-test.sh -- --ignored           # + les reproductions de la frontière Python
```

Les tests de console `script_panel` et de démarrage paresseux
`script_bridge_lazy` exigent aussi la feature Python de `stokesforge-app`.
Avec l'environnement embarqué décrit dans [`.codex/README.md`](../.codex/README.md),
`scripts/python-test.sh` les inclut ; pour les isoler :

```sh
cargo test -p stokesforge-app --features python --test script_panel --test script_bridge_lazy
```

## CI (`.gitlab-ci.yml`)

Un seul étage, `check`, et cinq travaux :

| Travail | Bloquant | Ce qu'il fait |
|---|---|---|
| `fmt` | oui | `cargo fmt --all -- --check` |
| `clippy` | oui | `cargo clippy --workspace --all-targets -- -D warnings` |
| `test` | oui | `cargo test --workspace` |
| `robustesse` | non | `cargo test -p stokesforge-robustesse -- --ignored` |
| `test-python` | oui, automatique | vendorise CPython + numpy, puis `scripts/python-test.sh` |

**Ce qu'un pipeline vert ne prouve pas**, et c'est écrit en tête du fichier CI :
`fmt`, `clippy` et `test` n'activent **jamais** la feature `python`. Tout ce qui
touche à PyO3 y est compilé **vide** — le pont Python, la partie la plus risquée
de l'architecture, n'est ni compilé, ni vérifié par clippy, ni testé par ces
trois travaux.

L'interpréteur n'est pas versionné. `test-python` le télécharge à froid et
le garde en cache selon `vendor/python/lock.env` ; ses artefacts Cargo ont
une clé distincte des travaux Rust purs. Il couvre automatiquement pont,
scripts, robustesse, application et CLI. Les reproductions ignorées et le
spike Matplotlib restent des essais explicites. Aucun Python système n'est
requis.

Typst, lui, n'est plus une feature (issue #29) : c'est une dépendance ferme des
trois crates de rendu, donc `fmt`, `clippy` et `test` le compilent et le
testent. Ce que la CI ne prouve pas pour Typst, c'est l'export : tant que
l'issue #34 n'a pas branché SVG/PDF/PNG dessus, il sort en texte brut.

## Ce qu'il faut savoir avant d'écrire un test

- Un test qui compare deux chemins a besoin d'un **témoin** qui doit différer.
- Un plafond qui ressemble à un bug n'en est probablement pas un : les
  plafonds et compteurs déjà arbitrés sont documentés à l'endroit de leur
  constante, et recensés dans
  `docs/archive/rustrewrite/questions-ouvertes.md` §11.
- La langue de l'interface est un état **global** du processus
  (`i18n::set_language`) : un test unitaire qui doit peindre dans une langue
  précise (`en`, `en-XA`) se relance en sous-processus avec `STOKESFORGE_LANG`
  posé, et vérifie que le fils a bien peint dans cette langue (modèle :
  `chaque_onglet_se_peint_en_anglais_et_en_pseudo_langue`,
  `src/app/plot_settings.rs`, #119).
- Les tests et leurs commentaires sont en français, comme le reste.

**À lire** : `stokesforge-robustesse/src/lib.rs` (l'index des batteries), puis la
tête du fichier `b*` visé.
