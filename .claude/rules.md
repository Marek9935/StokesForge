# Règles de chargement contextuel

Ce projet n'a qu'un seul domaine actif : le portage Rust (`crates/`).
`legacy-python/` a été supprimé de cette branche (il subsiste sur `main` si
besoin de retrouver l'ancien comportement, mais rien ici n'y fait plus
référence).

Charger uniquement les crates et docs pertinents pour la tâche, pas tout le
workspace.

## Modèle de données — `plotyx-core`, `plotyx-id`, `plotyx-plot`
Charger : les 3 crates. Ignorer : `commands`, `session`, `persistence`,
`app` sauf si la tâche change une API publique qu'ils consomment.

## Bus de commandes / undo — `plotyx-commands`
Charger : `plotyx-commands` + `plotyx-core` en lecture (types manipulés).
Ignorer : `session`, `persistence`, `app`.

## Persistance / format `.plx` — `plotyx-persistence`
Charger : `plotyx-persistence`, `plotyx-core`, `plotyx-plot` +
`docs/rustrewrite/format-fichier-plx.md`. Relire ce doc avant toute
modification de schéma — options déjà écartées avec leurs raisons.

## Acteur / concurrence — `plotyx-session`
Charger : `plotyx-session`, `plotyx-core`, `plotyx-commands`,
`plotyx-persistence`. Ignorer : `python`, `script`, `app` sauf si la tâche
touche l'API exposée à l'acteur.

## Pont Python / API script — `plotyx-python`, `plotyx-script`
Charger : ces 2 crates + `docs/rustrewrite/cahier-des-charges-api-script.md`.
Spécifique PyO3, contre l'interpréteur vendored (`vendor/python/`) — sans
lien avec l'ancienne implémentation Python/Qt (supprimée de cette branche).

## UI — `plotyx-app`
Charger : `plotyx-app`, `plotyx-core`, `plotyx-commands`, `plotyx-session`,
`plotyx-persistence`. Le choix de toolkit UI n'est pas encore fait — ne pas
supposer un framework précis, vérifier `Cargo.toml` de `plotyx-app` en cas
de doute.

## Rendu — `plotyx-render`, `plotyx-render-egui`, `plotyx-render-svg`
Charger : `plotyx-render` **d'abord** — il porte la mesure de texte, le
pipeline Plotters et, depuis R5, tout le rendu (`series`, `scale`, `paint`,
`legend`, `decimate`, `sample`) — puis le crate ciblé, plus `plotyx-plot` et
`plotyx-core::resolve` en lecture. **Une fonctionnalité de tracé ne s'écrit
pas dans un backend** : les deux backends ne diffèrent que par la façon de
poser des pixels, et la seule exception vivante est
`plotyx-render-egui::feathering` (la tessellation n'existe que pour
l'écran). Relire `docs/newarchitecture/architecture-choisie.md` §4 et §5
avant de toucher à la mesure du texte ou au cache : l'accord écran/export
est le seul endroit du chantier où un bug ne lève aucune erreur. Le filet
est `plotyx-render-egui/tests/accord.rs` — s'il change, lire ses deux
familles de témoins avant. `roadmap-rendu.md` dit ce qui appartient à R6
(PDF), R7 (interaction, rognage réel) ou R8 (Typst) plutôt qu'à maintenant.
Ignorer : `commands`, `persistence`, `python`, `script`.

## Robustesse — `plotyx-robustesse` (tests `b1`…`b20`)
Charger : le crate ciblé par le test (ne pas ouvrir les 20 fichiers de
`tests/`) + `plotyx-robustesse/src/lib.rs` (index des batteries) +
`docs/rustrewrite/guide-de-correction-robustesse.md` et
`docs/rustrewrite/questions-ouvertes.md` avant de traiter un comportement
qui ressemble à un bug (plafonds/compteurs déjà documentés comme voulus).
