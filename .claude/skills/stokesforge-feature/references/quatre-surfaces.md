# Fiche : les quatre surfaces d'une action (raccourci, menu, grisage, libellé), raccourcis, menu contextuel

Quand la lire : on ajoute une action, un raccourci clavier ou une entrée de clic droit.

## Où lire la règle

- Ce que Qt donnait gratuitement et qu'il faut écrire ici : `docs/application.md` § Ce que Qt donnait gratuitement, et qu'il faut écrire ici.
- Registre des raccourcis, `SHORTCUT_*`, curseurs : `docs/standard-interface.md` § Clavier et souris ; `docs/gestes.md` § Le modèle.
- Ctrl+C / X / V non reçus comme `Event::Key`, constantes décoratives, presse-papiers système : `docs/application.md` § Ce que Qt donnait gratuitement, et qu'il faut écrire ici ; copie de figure : `.claude/conventions.md` § Copie d'objets et de figures.
- Tab, flèches et `GRID_KEYS` : `docs/application.md` § Tab et flèches appartiennent à la grille (#38).
- Menu contextuel de la grille : `docs/application.md` § Menus contextuels de la grille ; grammaire : `docs/menus.md` § Règles.
- Libellés : clés `action-*` (un libellé par action), skill `stokesforge-textes-interface`, `docs/textes-interface.md` ; garde `aucun_libelle_en_dur`.

## À vérifier

- [ ] collisions de raccourci : `rg "SHORTCUT_" crates/stokesforge-app/src/app/shortcuts.rs`
- [ ] raccourci dans le registre, ligne de `HELP` et clés `help-*` en/fr
- [ ] entrée de menu avec son `shortcut_text`
- [ ] grisage calculé depuis `AppState`
- [ ] libellé issu de `state` (`undo_label`, `target_label`...) et clé du catalogue `en` + `tr!`
- [ ] raccourci essayé pendant la saisie d'une cellule (Ctrl+Z annule le modèle, pas la frappe)
- [ ] entrée de clic droit : équivalent en barre de menus, clavier ou barre d'outils
