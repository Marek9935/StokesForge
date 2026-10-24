# Sheet: an action's four surfaces and its place in the menus

Read for every feature before coding (decide where the user finds it), and whenever adding an action, a shortcut, a menu or right-click entry. An action has four surfaces nothing keeps consistent but review: shortcut, menu entry, greying, label.

## Where the rule lives

- Grammar (sections, submenus, absent / greyed, bare verbs, separators): `docs/menus.md` § Règles ; `docs/menus.md` § Ordre canonique des sections ; `docs/menus.md` § Ajouter une entrée.
- Content and envelope: `docs/menus.md` § Le principe : contenu et enveloppe.
- Shared painters, operational summary: `.claude/conventions.md` § Object Manager, panneaux fixes, menus.
- Menu bar and thematic menus: `docs/application/menus-et-barres.md` § Menus thématiques (#114).
- Grid context menu, deliberate menu / keyboard gaps: `docs/application/grille.md` § Menus contextuels de la grille.
- What Qt gave for free: `docs/application/etat-et-console.md` § Ce que Qt donnait gratuitement, et qu'il faut écrire ici.
- Shortcut registry, `SHORTCUT_*`, cursors: `docs/standard-interface.md` § Clavier et souris ; `docs/gestes.md` § Le modèle.
- Tab, arrows and `GRID_KEYS`: `docs/application/grille.md` § Tab et flèches appartiennent à la grille (#38).
- Labels: `docs/textes-interface.md` § 1. Règles (skill `stokesforge-textes-interface`).

## Check

- [ ] place decided and stated in the report (menu bar and context menus); nothing reachable only by right-click
- [ ] entry in a section, separators only by `MenuSections::section`; absent (no object) or greyed with `.disabled`
- [ ] test in the `mod tests` of `app/menus.rs` (greying read through the accessibility tree); toggles: two clicks in the same menu
- [ ] grid menu: decision in `AppState::grid_menu`, tested in `stokesforge-app-state/tests/shell.rs`
- [ ] shortcut collisions: `rg "SHORTCUT_" crates/stokesforge-app/src/app/shortcuts.rs`; shortcut in the registry, a `HELP` row and `help-*` keys in en/fr
- [ ] menu entry with its `shortcut_text`; greying computed from `AppState`
- [ ] label from `state` and a `tr!` key of the `en` catalogue
- [ ] shortcut tried while editing a cell (Ctrl+Z undoes the model, not the keystroke)
