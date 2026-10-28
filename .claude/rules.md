# Context loading rules

One active domain: the Rust port (`crates/`). `legacy-python/` was removed
from this branch and nothing refers to it any more.

**Load what the task needs, not the workspace.** Each domain below says what to
open, in which order, and above all **what not to open**.

The order is always the same: the `docs/` page **first** (it routes and gives
the invariants), the code after. `docs/invariants.md` applies to all domains —
12 rules, one table; read it before touching rendering, history or the actor.

## Routing

| Task | Read first | Load | Do not open |
|---|---|---|---|
| Tree, columns, `PlotSpec` | `docs/data-model.md` | `stokesforge-core`, `stokesforge-id`, `stokesforge-plot` | `commands`, `session`, `persistence`, `app` |
| Undo/redo, macros | `docs/commands-and-history.md` | `stokesforge-commands` + `stokesforge-core` read-only | `session`, `persistence`, `app` |
| `.stf` format, `save`/`load` | `docs/persistence.md` | `stokesforge-persistence`, `stokesforge-core`, `stokesforge-plot` | `session`, `render*`, `app` |
| Actor, concurrency, snapshots | `docs/session.md` | `stokesforge-session`, `stokesforge-core`, `stokesforge-commands`, `stokesforge-persistence` | `python`, `script`, `app` |
| Plot, axes, export, formulas | `docs/rendering.md` | `stokesforge-render` **first**, then the target backend, + `stokesforge-plot` and `stokesforge-core::resolve` read-only | `commands`, `persistence`, `python`, `script` |
| PyO3 bridge, `pyst` surface | `docs/python-and-scripting.md` | `stokesforge-python`, `stokesforge-script` | `render*`, `persistence` |
| Window, shortcuts, grid | `docs/application/shell-and-code.md` (§ Code split: `app.rs` and `src/app/` (#115) — the module → role table) | `stokesforge-app-state/src/state.rs` (the state without egui, #263) for the decision, then **the single module** of the surface in `src/app/` (grid `grid.rs`, workbook `workbook.rs`, graph `graph_window.rs`, menus `menus.rs`, windows `window_*.rs`…); `app.rs` only for orchestration (panel order, `collect_shortcuts`, deferred application, confirmations); `stokesforge-session`; `stokesforge-render-egui` if the task touches the graph | `render-svg`, `persistence`, the other modules of `src/app/` (except a new window or dialog: see the rule under the table) |
| Menu, right-click (entry, section, disabling) | `docs/menus.md` | `stokesforge-app/src/app/menus.rs` + the shared painting of the surface (`object_menu`, `columns_menu_contents`…), `app/menu_sections.rs` | the other modules of `src/app/` |
| Behaviour of a field, message, dialog, confirmation, disabling, keyboard/mouse gesture | `docs/interface-standard.md` (#217: *in place* / *planned* state of each section) | the module of the surface in `src/app/`, `app/dialogs.rs`, `app/shortcuts.rs`, `theme.rs` | the other modules of `src/app/` |
| Interface text (label, tooltip, message) | `docs/ui-texts.md` + skill `stokesforge-ui-texts` | the single file of `stokesforge-app` or `stokesforge-app-state` that holds the text (catalogs in `stokesforge-app-state/i18n/`) | the rest of the crate, the other crates |
| Robustness test `bN` | `docs/tests.md` | `stokesforge-robustness/src/lib.rs` (index) + the **single** `bN` file concerned + the crate it tests | the 20 other files of `tests/` |
| Cross-cutting question ("where does X live?") | `docs/README.md` | nothing yet | everything |

**New window or dialog (#300, E1).** Choose its **nature** (`DialogNature` in `app/dialogs.rs`, S-F11: it sets the footer, the size class Small / Medium / Large, the initial focus) and add it to the catalog with its `stokesforge_…` identifier. Open the list of existing windows (`docs/windows.md`) and **a reference window of the same type** (its module in `src/app/`: `window_*.rs`, `dialogs.rs`…): imitate a twin, do not invent. These modules then leave the "Do not open" column. Also read `docs/ui-principles.md`.

## The most expensive pitfalls

Only four, because they raise no error and you do not see them
when rereading your own diff.

**Text measurement (rendering).** `plotters` is taken without its fonts. A backend that
does not override `draw_text` panics; one that does not override
`estimate_text_size` measures the text by its number of UTF-8 bytes, **silently**.
All measurement goes through `stokesforge_render::text` and nowhere else. The
safety net is `stokesforge-render-egui/tests/agreement.rs`: if it changes, read its two
families of **witnesses** first. Without them, an equality test passes when both
paths are right, and also when both are wrong in the same
way.

**A plotting feature is not written in a backend.** It goes in
`stokesforge-render`, generic over `DrawingBackend`. The only living exception:
`stokesforge-render-egui::feathering` (tessellation exists only for the screen).
And feathering is **not** in the cache key.

**Zoom and pan are not model mutations.** `ViewState` (`stokesforge_render::view`, without egui) lives alongside, on
`GraphView`, and never touches the `PlotSpec` of the `Tree` — nobody wants to
undo a zoom with `Ctrl+Z`. The rule "every mutation goes through a command"
therefore does not apply to view framing.

**A cap is not necessarily a bug.** Name length, `MAX_NODE_ID`,
`MAX_CELLS`, number of histogram bins, history limits: already
decided, documented next to their constant and listed in
`docs/archive/rustrewrite/questions-ouvertes.md` §11. Check before
"fixing".

## Features: what `cargo build --workspace` does not compile

Only `python` is outside `default`. A task that touches it must build and
test **explicitly**, otherwise it validates dead code. Typst is no longer a
feature: it is a firm dependency of the rendering crates, which `cargo build` and
`cargo test --workspace` build and test.

- `python` → `scripts/python-test.sh` (sets `PYO3_CONFIG_FILE` and the other
  variables; without it, PyO3 does not build). The interpreter is vendored in
  `vendor/python/` and is **not versioned**.

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

## Conventions by domain

The detailed rules by domain (building blocks of `stokesforge-app`, menus,
series, copy, analysis, `pyst`, rendering pitfalls) are in
[`conventions.md`](conventions.md); the table in `CLAUDE.md` says which section
to read. A large search (a lot of reading) goes to the `recherche` agent
(`agents/recherche.md`, Haiku): see `CLAUDE.md`, "Delegation and model choice".
