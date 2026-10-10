---
name: stokesforge-verification
description: Écrire un test StokesForge au bon endroit, et vérifier un changement avant de le livrer — `fmt`, `clippy -D warnings`, `cargo test --workspace`, plus la feature hors défaut (`python`) exercée par le travail CI test-python, mise à jour de la doc, message de commit en français sur la branche `dev`. À utiliser dès que la demande est « teste », « vérifie », « c'est bon ? », « commit », « pousse », « j'ai fini », ou en fin de toute tâche d'implémentation ou de correction — un `cargo test` vert ne prouve pas ce qu'on croit dans ce dépôt, et ce skill dit exactement ce qu'il ne prouve pas.
---

# Vérifier un changement StokesForge

## Les commandes, et ce qu'elles ne prouvent pas

```sh
cargo fmt --all -- --check                    # bloquant en CI
cargo clippy --workspace --all-targets -- -D warnings   # bloquant en CI
cargo test --workspace                        # tout, sauf #[ignore]
cargo test -p <crate> --test <fichier>        # un fichier de tests
cargo test -p stokesforge-robustesse -- --ignored  # reproductions et bancs coûteux
scripts/python-test.sh                        # pont, scripts, robustesse, application et CLI
```

**Les travaux `fmt`, `clippy` et `test` n'activent pas `python`.** Le travail
`test-python` est automatique et bloquant ; il teste les cinq crates du chemin
Python. Clippy Python exige encore l'environnement embarqué explicitement.
`fmt` vérifie la mise en forme, pas la compilation de ce chemin. Typst, lui,
n'est plus une feature (issue #29) : `cargo test --workspace` le couvre.
`--all-features` n'active plus que Python et exige son environnement. Les commandes et
prérequis de référence sont dans [`.codex/README.md`](../../../.codex/README.md),
§ « Fonctionnalités optionnelles ».

Donc : **une tâche qui touche une feature hors défaut doit la compiler et la
tester explicitement**, sinon elle valide du code mort.

| Le changement touche… | Lancer en plus |
|---|---|
| `stokesforge-python`, `stokesforge-script`, la surface `pyst` | `scripts/python-test.sh` (pose `PYO3_CONFIG_FILE`) |
| l'intégration Python de `stokesforge-app` ou le CLI `run` | `scripts/python-test.sh` couvre ces crates ; appliquer son environnement à Clippy ; `scripts/cli-dist.sh` si l'empaquetage CLI change |
| les formules Typst, `stokesforge-render::formula` | rien de plus : `cargo test --workspace` les couvre (dépendance ferme), export SVG/PDF/PNG compris (#34) ; le texte Typst d'un SVG reste du texte `<text>` par défaut (#162, `stokesforge-render-svg/tests/typst.rs`) |
| une reproduction `#[ignore]` de robustesse | `cargo test -p stokesforge-robustesse -- --ignored` |
| le recalcul des formules (`stokesforge-session/src/recalc.rs`), une donnée lue par une formule, une écriture de colonne | `b22` et `recalc_cible.rs` passent dans `cargo test --workspace`, mais un oracle vert ne prouve rien si le tirage ne produit pas le geste : suivre `docs/tests.md`, « Recalcul ciblé » (pas ajouté au tirage, cas nommé avec témoin, mutation de la composante) |
| une dépendance, une ressource ou le runtime distribué | `cargo deny --locked check licenses` et génération des notices ; outils figés et packaging dans `docs/licence-et-extensions.md` ; actualiser les notices natives avec le verrou Python |
| l'**apparence** de l'interface (contour, alignement, largeur, icône) | une capture : `STOKESFORGE_APP_SCREENSHOT` + `STOKESFORGE_APP_AUTOQUIT_MS`, voir [`../stokesforge-bug-report/references/voir-l-interface.md`](../stokesforge-bug-report/references/voir-l-interface.md). Les tests couvrent la géométrie, pas l'apparence. Pour un texte d'interface (nouveau libellé, zone convertie), refaire la capture avec `STOKESFORGE_LANG=en-XA` : un texte sans `[[ ]]` a échappé au catalogue, un `]]` manquant est un texte tronqué |
| une vraie fenêtre (Wayland, `rfd`, multi-écran, glisser, fenêtre détachée) | rien : **aucun outil non interactif ne peut valider ça**. Le dire dans le compte rendu, ou demander une capture à l'utilisateur. |

## Où poser un test

| Famille | Où | Ce qu'elle garde |
|---|---|---|
| unitaire | `#[cfg(test)]` dans `src/` | un comportement local, au plus près de son code |
| intégration | `crates/*/tests/` | une propriété de crate : contrat public, frontière, accord entre chemins |
| robustesse | `stokesforge-robustesse/tests/b1`…`b22` | le socle aux limites (index dans son `src/lib.rs`) |

Logique d'interface : `stokesforge-app/tests/shell.rs`, sans fenêtre. Si une
décision doit vivre près de l'UI, l'extraire en **fonction pure** puis la
tester — c'est l'idiome du dépôt (`app::grid::column_grip_bands`,
`state::active_graph`, `state::drop_payload_action`). Un geste **peint** (clic,
touche, disposition lue dans les formes) se teste dans le module `tests` de sa
surface (`src/app/<surface>.rs`), sans fenêtre ; le module `tests` d'`app.rs`
n'éprouve que l'orchestration (ordre des panneaux, application différée,
confirmations, raccourcis globaux). Les tests suivent leur code : déplacer une
fonction, c'est déplacer ses tests. Une aide de test partagée entre modules
d'`app` est `pub(in crate::app)` dans le module de tests qui la porte
(`graph_window::tests::graphe`).

### Le témoin — sans lui, un test d'égalité ne prouve rien

C'est la méthode la plus importante du dépôt, et elle vaut au-delà du rendu.
Un test qui compare deux chemins peut passer parce que les deux sont justes —
ou parce que **les deux sont faux de la même façon**. D'où des cas qui doivent
rester **différents** : la même figure tracée par le backend nu doit se placer
ailleurs ; une variante du graphe qui change la fonctionnalité doit produire
une figure différente.

Avant de modifier `stokesforge-render/tests/mesure.rs` ou
`stokesforge-render-egui/tests/accord.rs`, **lire leurs témoins**.

### Garde-fous d'architecture — ne jamais les affaiblir

Ils ne vérifient pas un comportement mais une propriété de frontière. S'ils
échouent, c'est le changement qui est à refaire, pas le test.

| Test | Ce qu'il tient |
|---|---|
| `*/tests/no_ui_dependency.rs` | liste blanche des dépendances du crate, lues par `stokesforge-arch-guard` (#24) : `[dependencies]`, `[build-dependencies]`, variantes conditionnelles `[target.…]`, optionnelles, nom réel des renommées ; `[dev-dependencies]` exclues (invariant 2). Un crate sans ce fichier n'est pas gardé |
| `stokesforge-render-svg/tests/frontiere.rs` | l'export se teste sans serveur graphique |
| `stokesforge-render/tests/mesure.rs` | la source unique de mesure de texte (invariant 4) |
| `stokesforge-render-egui/tests/accord.rs` | écran et export aux mêmes pixels |
| `stokesforge-render-egui/tests/feathering.rs` | le feathering hors de la clé de cache (invariant 6) |
| `stokesforge-app/tests/script_bridge_lazy.rs` | l'interpréteur Python ne démarre pas à l'ouverture de la fenêtre |
| `stokesforge-app/tests/libelles.rs` | `aucun_libelle_en_dur` : aucun texte d'interface en dur. Absolue, sans dérogation ; `aucune_cle_du_catalogue_n_est_morte` (`src/i18n.rs`) : une clé de `en` sans emploi échoue |
| `stokesforge-app/tests/gardes.rs` | aucune `egui::Window` brute ; la fenêtre des propriétés passe par la brique commune ; aucun `use …::*` hors du code de test (`aucun_import_global_hors_des_tests`, absolue) ; aucun séparateur brut dans un menu ; aspect commun #169 : bouton sélectionnable seulement dans un menu, une liste déroulante ou le sélecteur de couleur, aucune couleur en dur hors de `theme.rs` / `color_picker.rs`, aucun filet juste avant un titre de section |
| `stokesforge-app/src/theme.rs` (tests) | aucun `ScrollStyle` hors du thème ; aucun `strong()` d'egui, tout gras par `theme::bold` |
| `stokesforge-app/tests/support/sources.rs` | pas un test : le lecteur **unique** des gardes qui lisent le code (récursif sur `src/`, vue de production, recherche de fonction par nom, #116). Une garde nouvelle qui lit le code passe par lui — jamais `include_str!("app.rs")` ni un fichier nommé — et pose son témoin dans un autre fichier qu'`app.rs` |
| `stokesforge-app/src/i18n.rs` (tests) | complétude `fr ⊆ en` avec les mêmes arguments, pseudo-langue `en-XA`, pluriels et typographie françaises ; **bloquant** : une clé ajoutée à `en` s'ajoute à `fr` dans le même commit |
| `stokesforge-commands/tests/memory_pinning.rs` | l'historique n'épingle pas de gros buffers |

Garde **en place** du standard d'interface (#217) : `aucun_champ_numerique_hors_de_la_brique`
(`stokesforge-app/tests/gardes.rs`, #220 : ni `DragValue` ni `Slider`, sans
dérogation).

Gardes **à venir** du standard d'interface (#217) : bulle nue sur un champ (#225), `add_enabled` sans
raison (#226), `add_space` littéral hors de `theme.rs` (#227), `consume_key`
hors de `app/shortcuts.rs` (#231). Elles **n'existent pas encore** : ne pas
les citer comme vertes. Le commit qui en écrit une la déplace dans le tableau
ci-dessus et la passe à *en place* dans
[`docs/standard-interface.md`](../../../docs/standard-interface.md).

### Reproduction vs garde-fou

Une **reproduction** échoue délibérément et porte `#[ignore]` avec son motif
écrit dans l'attribut — pour que le pipeline garde sa valeur de signal. Quand
elle se met à passer, retirer `#[ignore]` : le garde-fou prend le relais.

Mesurer plutôt que chronométrer : `b17` et `b21` comptent des **octets
alloués**, pas des millisecondes, pour que l'assertion reste vraie sur
n'importe quelle machine. Suivre ce principe pour tout nouveau test de coût.

## Relire le diff

- `git status --short` puis `git diff` : ne jamais emporter dans un commit les
  fichiers non suivis ni les modifications en cours de l'utilisateur.
- Aucun test affaibli pour faire passer le changement.
- Les plafonds documentés (`questions-ouvertes.md` §11) toujours en place.
- Documentation à jour **là où le changement l'a rendue fausse** : le `//!` du
  module (le *pourquoi* : ce qui a été essayé, écarté, imposé par une
  contrainte) et la page `docs/` du domaine (le *quoi*, qui oriente).
  Si des chemins, scripts ou frontières ont bougé : `.claude/rules.md`,
  `.codex/README.md`, `CLAUDE.md`, `AGENTS.md`.
- Code, commentaires, tests, doc et commits en **français**.

## Committer

Branche de développement : **`dev`**. Ne jamais pousser ailleurs sans
autorisation explicite.

Format des messages, tel qu'il est pratiqué dans l'historique — `<domaine> :
<verbe au présent> <objet>`, le domaine étant un crate ou un thème :

```
stokesforge-app : corrige le graphe invisible dans une fenêtre détachée
stokesforge-app : aligne les actions de fenêtre
formule : distingue case vide (Missing) et échec de calcul (Failed) affiché "---"
docs : note de passation du chantier « prévisualisation d'import »
```

Pousser avec `git push -u origin dev`. En cas d'échec réseau, réessayer
jusqu'à 4 fois (2s, 4s, 8s, 16s). Ne pas ouvrir de *merge request* sans demande
explicite.

## Nettoyer après une fusion sur `dev`

Après une fusion demandée par l'utilisateur et un push réussi sur `dev`,
nettoyer automatiquement le worktree temporaire **et sa branche locale**
(`codex/issue-*` ou autre nom utilisé pour cette tâche), sans attendre une
nouvelle demande. Retirer un worktree ne supprime pas sa branche : les deux
opérations font partie de la livraison.

- Vérifier que la branche de tâche est entièrement intégrée :
  `git merge-base --is-ancestor <branche> dev`. Examiner son
  `git status --short` et `git worktree list` avant le retrait ; conserver
  tout travail non livré et les fichiers nécessaires, y compris ignorés.
- Depuis le dépôt principal sur `dev`, archiver le worktree géré par Codex
  avec `archive_worktree` (identité fournie par `list_artifacts`) ; pour un
  worktree Git ordinaire propre, utiliser `git worktree remove <chemin>`.
  Vérifier que le retrait est terminé avant de supprimer la branche.
- Supprimer la branche locale fusionnée avec `git branch -d <branche>`.
  Ne pas utiliser `-D` ni forcer le retrait pour contourner un échec.
- Vérifier que ni le worktree ni la branche ne restent, puis inclure ce
  nettoyage dans le compte rendu. Si le nettoyage bloque, expliquer la
  cause précise et conserver ce qui reste à livrer.

Ce nettoyage concerne les ressources temporaires de la tâche achevée.
Ne pas supprimer les branches d'autres travaux ni les branches distantes
sans demande explicite. Une demande de conserver le worktree ou la branche
prime sur ce nettoyage.

## Rendre compte

Trois choses, sans les arrondir : **ce qui a changé**, **les contrôles
réellement exécutés** (pas ceux prévus), et **les limites de validation**. Si
un test échoue, le dire avec sa sortie ; si une étape a été sautée, le dire.
Un résultat vert sans les features concernées ne valide pas ces chemins.

## Checklist

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] features hors défaut touchées → contrôles ciblés `python`, y compris `stokesforge-app` si concerné
- [ ] aucun garde-fou d'architecture affaibli ; témoins intacts
- [ ] texte d'interface : clés dans `en`, complétude `fr = en` verte (clé écrite dans les **deux** catalogues), `aucun_libelle_en_dur` et `aucune_cle_du_catalogue_n_est_morte` verts (aucune dérogation, aucune clé orpheline), capture en `en-XA` si le texte est visuel
- [ ] `#[ignore]` retiré si une reproduction passe désormais
- [ ] doc à jour (`//!`, page `docs/`, et fichiers de routage si les chemins ont bougé)
- [ ] diff relu, travail en cours de l'utilisateur préservé
- [ ] commit en français, branche `dev`
- [ ] après fusion et push sur `dev` : worktree temporaire retiré et branche locale fusionnée supprimée, sauf conservation demandée
