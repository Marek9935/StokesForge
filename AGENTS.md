# Consignes Codex — StokesForge

## Avant toute implémentation

Lire [.codex/README.md](.codex/README.md), puis suivre son parcours de lecture
pour le domaine concerné. **Inspecter l'existant avant d'écrire du code** :
documentation, modules, appelants, tests, dépendances et scripts disponibles.
Rechercher une implémentation analogue et réutiliser les outils du dépôt.
Ne pas introduire une bibliothèque, un utilitaire ou une nouvelle couche sans
avoir vérifié ce qui répond déjà au besoin.

Commencer par `git status --short` et examiner les différences pertinentes.
Préserver les modifications et les fichiers non suivis de l'utilisateur.
Avant de modifier, indiquer brièvement les composants à réutiliser, la zone
à changer et les vérifications prévues ; poursuivre sans demander une
confirmation pour les choix courants couverts par la demande.

## Repères permanents

- Pendant le signalement et la création ou l'enrichissement d'issues, les
  retours suivants complètent les issues, même formulés comme « ajoute »,
  « enlève » ou « supprime ». Ne passer au code que sur demande explicite
  d'implémentation dans le dépôt ; voir la règle de contexte du skill
  `stokesforge-bug-report`.
- La licence du dépôt est GPL-3.0-or-later avec la permission de
  `LICENSE-EXCEPTION`. Toute nouvelle interface couverte s'ajoute par commit
  explicite à la liste fermée de `docs/licence-et-extensions.md` (`pyst`
  aujourd'hui). Conserver les notices tierces et les actualiser avec les
  dépendances ou le runtime ; `scripts/licenses-dist.sh` les livre et le
  travail CI `licenses` contrôle la politique par `cargo deny check licenses`.
  Les compléments de `vendor/crate-license-notices/` sont liés au verrou
  Cargo ; une modification de `Cargo.lock` exige leur revalidation avant
  distribution.
- Le développement actif est le workspace Rust dans `crates/`. Lire
  `Cargo.toml`, les manifestes concernés et `rust-toolchain.toml` pour les
  outils, versions et features effectifs.
- `docs/README.md` oriente ; `docs/invariants.md` décrit les frontières à
  préserver ; les commentaires de tête de module expliquent les décisions.
  Le code et les tests décrivent l'état implémenté. Une note de chantier ou
  `docs/archive/` ne prouve pas qu'une fonctionnalité existe ou reste à faire.
- Quand un travail installe une règle nouvelle (standard d'interface, motif à
  suivre, frontière), mettre à jour au besoin les skills concernés
  (`.claude/skills/`) et `CLAUDE.md` ou `.claude/conventions.md` dans le même
  commit que le code.
- Les skills de projet dans `.agents/skills/` renvoient aux sources maintenues
  dans `.claude/skills/`. Consulter le skill pertinent selon la table de
  `.codex/README.md` et ses références utiles, sans charger toute la collection.
  Les liens partagent aussi les fiches et les scripts : une mise à jour par
  Claude s'applique directement à Codex, sans copie à synchroniser.
- Les conventions détaillées sont partagées dans `.claude/conventions.md` :
  lire la section du domaine avant d'écrire du code (table dans `CLAUDE.md`).
  `.claude/rules.md` précise le périmètre de lecture. Le skill `run-stokesforge`
  pilote et capture l'application réelle sous Linux pour les vérifications
  interactives ; son chemin X11 sous Xwayland ne valide ni Wayland natif,
  ni `rfd`, ni le glisser.
- Standard de comportement (#217) : champs, messages, dialogues, confirmations,
  grisage, clavier et souris suivent `docs/standard-interface.md`, source
  unique. Section *à venir* (#219–#232) : la règle vaut déjà pour tout code
  neuf, mais sa brique ou sa garde n'existe pas encore.
- Menus et clics droits (#152) : la grammaire de `docs/menus.md` est la source
  unique (sections dans l'ordre canonique, séparateurs par
  `MenuSections::section` seulement — garde
  `aucun_separateur_brut_dans_un_menu` —, sous-menu pour un choix exclusif ou
  ≥ 3 entrées, entrée absente ou grisée avec `.disabled`, verbes nus,
  `Properties…` en dernier). Les peintures partagées peignent le contenu ; le
  menu du haut l'enveloppe, le clic droit l'affiche à plat.
- Les mutations métier passent par les commandes annulables et l'acteur de
  session. Respecter les exceptions documentées pour l'état de vue et la
  géométrie ; ne pas créer de voie d'écriture métier parallèle.
- `new_project`, comme `load`, est un remplacement d'identité par l'acteur,
  avec historique vidé et macro ouverte refusée ; la confirmation vit dans
  l'application. Le graphique vide réutilise `Command::AddGraph`.
- Le dépôt Wayland partage le récepteur du presse-papiers via les patches
  `vendor/winit` et `vendor/smithay-clipboard` (#72), hors workspace : lire
  `vendor/winit/STOKESFORGE.md` avant de modifier ou mettre à jour ces
  dépendances/eframe. `STOKESFORGE_X11=1` force le secours X11.
- L'import de l'application, du CLI et des scripts partage les plans et la
  conversion de `stokesforge-importer` ; `pyst.import_file` écrit par les commandes
  de sa session, GIL relâché, dans une macro annulable.
- Les copies d’objets (#105–106) réutilisent `NodeCopy` / `InsertCopy`, sans
  allocateur ni projet source transporté. Toute nature est copiable par défaut ;
  les IDs sont créés dans la destination et un échec restaure l’arbre. Unicité
  des noms par dossier, chemins canoniques enracinés ; les graphes entre projets
  emportent leurs colonnes dans un classeur caché persisté (#107), révélé par Show
  Data. F2 renomme dans les gestionnaires (#130) et édite dans la grille (#143), selon le focus ; jamais le nom des feuilles.
- Panneaux Python, PM et OM repliés : seul Déployer est visible, sans cible
  ni espace réservé à Détacher (`panel_header`). Le détachement par bouton
  exige un panneau déployé ; une fenêtre OS garde Rattacher.
- Le rendu partagé appartient à `stokesforge-render`. Préserver l'accord entre
  l'écran et les exports, ainsi que la mesure de texte unique.
  Le banc des grandes courbes (#141) reste manuel (`#[ignore]`, `--release`),
  sans API dédiée ; son protocole et les mesures vivent dans `docs/rendu.md`.
- Object Manager projette l'instantané dans `object_manager.rs` ; la sélection
  reste dans `GraphView`, y compris en fenêtre détachée. La visibilité passe
  par `EditPlotSpec` (même delta que `SetPlotSpec`) et les vignettes réutilisent
  la légende du rendu partagé.
- Suivre les conventions locales de nommage ; écrire les commentaires,
  la documentation et les messages de commit en français. Expliquer les
  décisions non évidentes au plus près du code.
- Vérifier les features réellement exercées : les commandes Cargo par défaut
  ne valident pas le pont Python (feature `python`). Typst est compilé et testé
  par défaut. `scripts/python-test.sh` couvre aussi l'application et le CLI,
  et le travail CI `test-python` est automatique et bloquant. La distribution
  CLI se vérifie par `scripts/cli-dist.sh` ; Matplotlib reste un spike isolé
  (`scripts/python-matplotlib-spike.sh`). Les commandes adaptées sont dans
  `.codex/README.md`.
- CI : Linux tourne sur le miroir GitHub, Windows sur GitLab. Maintenir les
  commandes de `.gitlab-ci.yml` et `.github/workflows/ci.yml` ensemble ; le
  rapporteur `gitlab-status.yml` / `scripts/gitlab-status.py` renvoie les
  statuts Linux vers GitLab (secret GitHub `GITLAB_STATUS_TOKEN`, procédure
  dans `docs/tests.md`). `STOKESFORGE_CI_LINUX=gitlab` réactive les travaux
  Linux GitLab ; sans runner Linux, les laisser désactivés.

## Pour terminer une modification

- Après fusion et push sur `main`, nettoyer automatiquement le worktree temporaire de la tâche et sa branche locale fusionnée, sauf conservation demandée ; suivre `stokesforge-verification`, « Clean up after a merge on `main` ».

Exécuter les vérifications adaptées au changement, relire le diff et mettre
à jour la documentation concernée. Ne pas masquer une régression en
affaiblissant un test d'architecture. Rapporter ce qui a changé, les contrôles
réellement exécutés et les éventuelles limites de validation.

Si le changement fait évoluer les outils, les chemins, les scripts ou les
frontières du projet, mettre aussi à jour `.codex/README.md` et ces consignes
pour que la prochaine session parte d'informations exactes.
