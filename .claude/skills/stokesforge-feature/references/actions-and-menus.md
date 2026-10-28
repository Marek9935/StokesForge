# Sheet: an action's four surfaces and its place in the menus

Read for every feature before coding (decide where the user finds it), and whenever adding an action, a shortcut, a menu or right-click entry. An action has four surfaces nothing keeps consistent but review: shortcut, menu entry, greying, label.

## Where the rule lives

- Grammar (sections, submenus, absent / greyed, bare verbs, separators): `docs/menus.md` § Rules ; `docs/menus.md` § Canonical order of sections ; `docs/menus.md` § Adding an entry.
- Content and envelope: `docs/menus.md` § The principle: content and envelope.
- Shared painters, operational summary: `.claude/conventions.md` § Object Manager, fixed panels, menus.
- Menu bar and thematic menus: `docs/application/menus-and-toolbars.md` § Themed menus (#114).
- Grid context menu, deliberate menu / keyboard gaps: `docs/application/grid.md` § Grid context menus.
- What Qt gave for free: `docs/application/state-and-console.md` § What Qt gave for free, and must be written here.
- Shortcut registry, `SHORTCUT_*`, cursors: `docs/interface-standard.md` § Keyboard and mouse ; `docs/gestures.md` § The model.
- Tab, arrows and `GRID_KEYS`: `docs/application/grid.md` § Tab and arrows belong to the grid (#38).
- Labels: `docs/ui-texts.md` § 1. Rules (skill `stokesforge-ui-texts`).

## Check

- [ ] place decided and stated in the report (menu bar and context menus); nothing reachable only by right-click
- [ ] entry in a section, separators only by `MenuSections::section`; absent (no object) or greyed with `.disabled`
- [ ] test in the `mod tests` of `app/menus.rs` (greying read through the accessibility tree); toggles: two clicks in the same menu
- [ ] grid menu: decision in `AppState::grid_menu`, tested in `stokesforge-app-state/tests/shell.rs`
- [ ] shortcut collisions: `rg "SHORTCUT_" crates/stokesforge-app/src/app/shortcuts.rs`; shortcut in the registry, a `HELP` row and `help-*` keys in en/fr
- [ ] menu entry with its `shortcut_text`; greying computed from `AppState`
- [ ] label from `state` and a `tr!` key of the `en` catalogue
- [ ] shortcut tried while editing a cell (Ctrl+Z undoes the model, not the keystroke)
