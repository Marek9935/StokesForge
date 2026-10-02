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

- Le développement actif est le workspace Rust dans `crates/`. Lire
  `Cargo.toml`, les manifestes concernés et `rust-toolchain.toml` pour les
  outils, versions et features effectifs.
- `docs/README.md` oriente ; `docs/invariants.md` décrit les frontières à
  préserver ; les commentaires de tête de module expliquent les décisions.
  Le code et les tests décrivent l'état implémenté. Une note de chantier ou
  `docs/archive/` ne prouve pas qu'une fonctionnalité existe ou reste à faire.
- Quand un travail installe une règle nouvelle (standard d'interface, motif à
  suivre, frontière), mettre à jour au besoin les skills concernés
  (`.claude/skills/`) et `CLAUDE.md` dans le même commit que le code.
- Les skills de projet dans `.agents/skills/` renvoient aux sources maintenues
  dans `.claude/skills/`. Consulter le skill pertinent selon la table de
  `.codex/README.md` et ses références utiles, sans charger toute la collection.
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
- Le rendu partagé appartient à `stokesforge-render`. Préserver l'accord entre
  l'écran et les exports, ainsi que la mesure de texte unique.
- Object Manager projette l'instantané dans `object_manager.rs` ; la sélection
  reste dans `GraphView`, y compris en fenêtre détachée. La visibilité passe
  par `SetPlotSpec` et les vignettes réutilisent la légende du rendu partagé.
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

## Pour terminer une modification

Exécuter les vérifications adaptées au changement, relire le diff et mettre
à jour la documentation concernée. Ne pas masquer une régression en
affaiblissant un test d'architecture. Rapporter ce qui a changé, les contrôles
réellement exécutés et les éventuelles limites de validation.

Si le changement fait évoluer les outils, les chemins, les scripts ou les
frontières du projet, mettre aussi à jour `.codex/README.md` et ces consignes
pour que la prochaine session parte d'informations exactes.
