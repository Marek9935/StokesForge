# Context loading rules

**Load what the task needs, not the workspace.** Each row of Routing says what to
open, in which order, and above all **what not to open**.

The order is always the same: the `docs/` page **first** (it routes and gives
the invariants), the code after. `docs/invariants.md` applies to all domains:
read it before touching rendering, history or the actor.

**Rules files.** The rules by domain live in `.claude/rules/<domain>.md` (one file per domain, listed in the "Rules file" column below). Each carries a `paths:` frontmatter and is loaded automatically **only when a matching file is read, edited or written with the Read, Edit or Write tools**: a `rg` or `cat` in Bash loads nothing, so Read the matching file explicitly before scoping or writing code. A rule file is binding like `CLAUDE.md`; most of its rules are also locked by a guard, and a green guard does not excuse you from the rule (it only sees what it looks for). This file stays outside `.claude/rules/` on purpose: a file there without `paths:` would load at every start.

## Routing

| Task | Read first | Load | Do not open | Rules file |
|---|---|---|---|---|
| Tree, columns, `PlotSpec` | `docs/data-model.md` | `stokesforge-core`, `stokesforge-id`, `stokesforge-plot` | `commands`, `session`, `persistence`, `app` | - |
| Undo/redo, macros | `docs/commands-and-history.md` | `stokesforge-commands` + `stokesforge-core` read-only | `session`, `persistence`, `app` | `copy.md` |
| `.stf` format, `save`/`load` | `docs/persistence.md` | `stokesforge-persistence`, `stokesforge-core`, `stokesforge-plot` | `session`, `render*`, `app` | `analysis.md` |
| Actor, concurrency, snapshots | `docs/session.md` | `stokesforge-session`, `stokesforge-core`, `stokesforge-commands`, `stokesforge-persistence` | `python`, `script`, `app` | `formulas.md` |
| Plot, axes, export, formulas | `docs/rendering.md` | `stokesforge-render` **first**, then the target backend, + `stokesforge-plot` and `stokesforge-core::resolve` read-only | `commands`, `persistence`, `python`, `script` | `rendering.md`, `series.md` |
| PyO3 bridge, `pyst` surface | `docs/python-and-scripting.md` | `stokesforge-python`, `stokesforge-script` | `render*`, `persistence` | `pyst.md` |
| Window, shortcuts, grid | `docs/application/shell-and-code.md` (§ Code split: `app.rs` and `src/app/` (#115) — the module → role table) | `stokesforge-app-state/src/state.rs` (the state without egui, #263) for the decision, then **the single module** of the surface in `src/app/` (grid `grid.rs`, workbook `workbook.rs`, graph `graph_window.rs`, menus `menus.rs`, windows `window_*.rs`…); `app.rs` only for orchestration (panel order, `collect_shortcuts`, deferred application, confirmations); `stokesforge-session`; `stokesforge-render-egui` if the task touches the graph | `render-svg`, `persistence`, the other modules of `src/app/` (except a new window or dialog: see the rule under the table) | `app.md`, `windows.md` |
| Menu, right-click (entry, section, disabling) | `docs/menus.md` | `stokesforge-app/src/app/menus.rs` + the shared painting of the surface (`object_menu`, `columns_menu_contents`…), `app/menu_sections.rs` | the other modules of `src/app/` | `menus-panels.md` |
| Behaviour of a field, message, dialog, confirmation, disabling, keyboard/mouse gesture | `docs/interface-standard.md` (#217: *in place* / *planned* state of each section) | the module of the surface in `src/app/`, `app/dialogs.rs`, `app/shortcuts.rs`, `theme.rs` | the other modules of `src/app/` | `app.md`, `errors.md` |
| Interface text (label, tooltip, message) | `docs/ui-texts.md` + skill `stokesforge-ui-texts` | the single file of `stokesforge-app` or `stokesforge-app-state` that holds the text (catalogs in `stokesforge-app-state/i18n/`) | the rest of the crate, the other crates | `app.md`, `errors.md` |
| Robustness test `bN` | `docs/tests.md` | `stokesforge-robustness/src/lib.rs` (index) + the **single** `bN` file concerned + the crate it tests | the 20 other files of `tests/` | - |
| Cross-cutting question ("where does X live?") | `docs/README.md` | nothing yet | everything | - |

**New window or dialog (#300, E1).** Choose its **nature** (`DialogNature` in `app/dialogs.rs`, S-F11: it sets the footer, the size class Small / Medium / Large, the initial focus) and add it to the catalog with its `stokesforge_…` identifier. Open the list of existing windows (`docs/windows.md`) and **a reference window of the same type** (its module in `src/app/`: `window_*.rs`, `dialogs.rs`…): imitate a twin, do not invent. These modules then leave the "Do not open" column. Also read `docs/ui-principles.md`.

## `docs/archive/`

Code comments cite documents by their file name
(`architecture-choisie.md` §4.3, `guide-de-correction-robustesse.md` §F6,
`roadmap-rendu.md`…): they are all in `docs/archive/`. Not maintained —
useful for the *why* of a decision and for options already ruled out,
never as a description of the current state. Two are worth reading before acting:
`format-fichier-plx.md` before any schema change, and
`questions-ouvertes.md` before dealing with behaviour that looks like a bug.

The phases cited here and there (0-5 for the foundation, R0-R8 for rendering) are
**all closed**: an `R6`/`R7`/`R8` in a comment names work
done, not work to come.

They predate the renaming (#87): they still say PlotYx, `plotyx-*`, `.plx` and `pyx`, which now read StokesForge, `stokesforge-*`, `.stf` and `pyst` (the file `format-fichier-plx.md` keeps its name). Where they differ from the code, the code is right.

The bug reports `retours-2026-09-23-bugs.md` and `retours-2026-09-24-bugs.md` (B1…B9, "Bug 1"…), cited in `crates/stokesforge-app/src/app.rs` and `src/app/`, were deleted; all their bugs are fixed. Read them with `git show a6fd933^:docs/chantiers/<file>`. Bugs are tracked as GitLab issues (`glab issue list --label bug`).
