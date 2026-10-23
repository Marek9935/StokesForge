# Documentation StokesForge — branche `main`

Portage de StokesForge (analyse et tracé de données scientifiques, façon OriginLab)
de Python/Qt vers Rust. Tout le code vit dans le workspace Cargo à la racine.

## Ce que cette documentation est, et ce qu'elle n'est pas

Elle **décrit le code tel qu'il est**. Elle ne planifie rien, ne spécifie rien
et n'arbitre rien : pas de cahier des charges ici.

Le raisonnement détaillé — *pourquoi* telle décision plutôt que telle autre,
ce qui a été mesuré, ce qui a été écarté — vit dans les **commentaires de tête
de module** (`//!`), au plus près du code qu'il justifie. Ces pages ne le
recopient pas : elles disent **où le trouver**, et portent seulement ce qui ne
tient dans aucun module parce que ça les traverse tous.

Règle de lecture qui en découle : ces pages orientent, le code fait foi. Une
page qui contredit le code a tort.

## Par où entrer

| Je veux… | Lire |
|---|---|
| comprendre le découpage en crates et pourquoi il est ainsi | [`architecture.md`](architecture.md) |
| savoir ce que je n'ai pas le droit de casser | [`invariants.md`](invariants.md) |
| toucher à l'arbre de projet, aux colonnes, au `PlotSpec` | [`modele-de-donnees.md`](modele-de-donnees.md) |
| toucher à l'undo/redo, aux macros | [`commandes-et-historique.md`](commandes-et-historique.md) |
| toucher au format `.stf`, à `save`/`load` | [`persistance.md`](persistance.md) |
| toucher à l'acteur, à la concurrence, aux instantanés | [`session.md`](session.md) |
| toucher au tracé, aux axes, à l'export SVG/PDF/PNG, aux formules | [`rendu.md`](rendu.md) |
| toucher au pont PyO3 ou à la surface `pyst` des scripts | [`python-et-script.md`](python-et-script.md) |
| reproduire les mesures du prototype Matplotlib hors écran | [`python-matplotlib-spike.md`](python-matplotlib-spike.md) |
| toucher à la fenêtre, aux raccourcis, à la grille | [`application/`](application/README.md) |
| ajouter ou afficher une icône (rôles de couleur, planche-contact) | [`icones.md`](icones.md) |
| ajouter ou ranger une entrée de menu ou de clic droit | [`menus.md`](menus.md) (grammaire), puis [`application/menus-et-barres.md`](application/menus-et-barres.md) § Menus thématiques |
| écrire ou modifier un texte visible (libellé, infobulle, message) | [`textes-interface.md`](textes-interface.md) |
| poser un champ, un message, un dialogue, une confirmation, un grisage, un geste clavier ou souris | [`standard-interface.md`](standard-interface.md) (comportement, #217) |
| créer une fenêtre ou un dialogue neuf, ou poser un geste (molette, double-clic) | [`fenetres.md`](fenetres.md) (catalogue des fenêtres), [`principes-ui.md`](principes-ui.md) (principes d'interface), [`gestes.md`](gestes.md) (matrice des gestes) |
| écrire ou lancer un test, comprendre la CI | [`tests.md`](tests.md) |
| mesurer la fluidité de l'interface (temps d'image, lancer en release) | [`mesure-performance.md`](mesure-performance.md) |
| licence, contributions, interfaces d'extension et notices tierces | [`licence-et-extensions.md`](licence-et-extensions.md) |

`.claude/rules.md` complète ce tableau côté **chargement de contexte** : quels
crates ouvrir, et surtout lesquels ne pas ouvrir, pour chaque domaine.

Une exception à la règle « ces pages ne planifient rien » :
[`chantier-colonnes-mixtes.md`](chantier-colonnes-mixtes.md) est une **note de
passation**, pas une page de référence — ce qu'un chantier a trouvé en chemin
et ce qu'il laisse ouvert, surtout côté test. Elle rejoindra `docs/archive/`
quand sa colonne « reste à faire » sera vide.

`docs/chantiers/` réunit les mêmes notes pour le travail **en attente** :

| Note | Contenu |
|---|---|
| [`retours-2026-09-23-features.md`](chantiers/retours-2026-09-23-features.md) | neuf évolutions à développer, avec les arbitrages déjà rendus |
| [`prevision-import.md`](chantiers/prevision-import.md) | prévisualisation d'import |
| [`analyse-maquettes.md`](chantiers/analyse-maquettes.md) | traitement mathématique (#181) : maquettes M1–M15 et choix du 2026-10-09, reportés dans #181–#190 |

**Les bugs n'y vivent pas** : leur état est suivi dans les issues GitLab du
projet (`glab issue list --label bug`). Les anciens rapports
`retours-*-bugs.md` ont été supprimés une fois leurs bugs corrigés ; les
commentaires du code qui les citent se résolvent dans l'historique git.

Comme les autres notes de passation, elles **planifient** : le code n'y fait
pas encore foi, et un « reste à faire » y demande d'être confronté au code
avant d'être repris.

## Commandes

```sh
cargo build --workspace
cargo test --workspace                        # inclut stokesforge-robustesse, sauf #[ignore]
cargo test -p <crate> <nom_test>              # un seul test
cargo test -p stokesforge-robustesse -- --ignored  # reproductions et bancs coûteux
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Les deux dernières sont obligatoires : la CI (`.gitlab-ci.yml`) les fait
échouer au premier écart.

## Conventions

- Code, commentaires, commits et documentation **en français**.
- Un commentaire de tête de module explique **pourquoi**, pas *quoi* : le
  *quoi* se lit dans la signature juste en dessous.
- Toute mutation du modèle passe par une commande annulable de
  `stokesforge-commands`, jamais par une primitive de `stokesforge-core` en direct.

## `docs/archive/` — l'historique, pas la référence

`docs/archive/` conserve les documents de conception antérieurs : cahier des
charges technique, roadmaps phasées (socle 0-5, rendu R0-R8), rapports de
robustesse, options explorées. Ils ont produit le code actuel et gardent leur
valeur d'archive — notamment parce que **les commentaires du code les citent
par leur nom de fichier** (`architecture-choisie.md` §4.3,
`guide-de-correction-robustesse.md` §F6, `roadmap-rendu.md`…). Toute citation
de ce genre se résout dans `docs/archive/`.

Ils ne sont pas maintenus. Là où ils divergent du code, c'est le code qui a
raison.
