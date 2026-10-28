---
name: stokesforge-feature
description: Implement a StokesForge feature end to end — layer, menus, toolbars, icons, shortcut, texts, tests, docs. Use for any change made in the repo, even one line (« ajoute » / add, « implémente » / implement, « fais que… » / make it so that…, « traite #N » for a change-request issue); during issue reporting, only once implementation is explicitly asked.
---

A feature almost always spans several layers. This skill keeps each piece in the right one and none forgotten.

**Context gate.** While the conversation collects feedback or fills issues, an imperative (« ajoute », « enlève ») is content for the issue, not a request to code: follow `stokesforge-bug-report`. Start here only on an explicit request to change the repo.

## 1. Before writing code

Branch `feature/<n>-<topic>`, one per issue, short; merge `main` into it, then merge it into `main` with `--no-ff` (`docs/v1-criteria.md` § Branches and versions). Work in a worktree created from an up-to-date `main`; run `git worktree list` first, other agents may work in parallel, and keep out of their files.

1. `git status --short`, then `git diff` on the target files: preserve the user's work in progress.
2. Read the domain page first, never the whole workspace: `.claude/rules.md` § Routing, `docs/invariants.md`, then the domain section of `.claude/conventions.md` (table in `CLAUDE.md`).
3. Search what exists (`rg`): an `Action`, a `Command`, a helper that already does most of it. Reuse beats writing.
4. A limit that looks wrong may be arbitrated: `docs/archive/rustrewrite/questions-ouvertes.md` §11.
5. Resolve edge-case ambiguities now, in one batch: [`../stokesforge-bug-report/references/decision-form.md`](../stokesforge-bug-report/references/decision-form.md) says when and how to ask.
6. New window, dialog or gesture: name the existing twin you imitate (`docs/windows.md`, `docs/gestures.md`), compare screenshots with `run-stokesforge`; a new window gets a mockup the user validates before code.
7. Announce what you reuse, the zone you change and the checks you will run, then code. Scope and delegation: `CLAUDE.md` § Delegation and model choice (agent `perimetre`, Sonnet implements).

A work note (`docs/*-work-package.md`, `docs/work-packages/`) proves nothing: the code is the truth.

## 2. Choose the layer

| The feature… | Lives in | Trap |
|---|---|---|
| changes the model | a `Command` (`stokesforge-command`) | never a `core` primitive directly (invariant 1) |
| is a per-document view setting | egui memory under the node id; default for new documents in preferences (`grid::header_rows`) | never a global flag; nothing in the `.stf` |
| is a per-node view setting that survives the file (#325) | a `NodeId -> value` table beside `window_geometries` (`ViewTables`, `geometry_revision`) | no command; never purged on mutation (invariant 7); pushed at gesture end; reset = remove the key |
| analyses a row (#253) | a linked transposed sheet (`Command::TransposeRows`), then the existing tool | never a "by rows" tool; `MacroKind::AnalyzeRows` |
| reorders columns (#319) | `stokesforge-app-state::column_move` + `Action::MoveColumns` | `Move` index counted after removal; one macro |
| changes zoom or pan | `ViewState` on `GraphView` | not a model mutation |
| is a UI decision (offer, grey out, name) | `stokesforge-app-state/src/state.rs` (no egui, #263) | must stay testable without a window |
| is egui drawing or events | the surface's module in `stokesforge-app/src/app/` | `app.rs` only orchestrates |
| is a plotting feature | `stokesforge-render` (`stokesforge-rendering`) | never one backend (invariant 5) |
| reads a data file | `stokesforge-import` (`stokesforge-import`) | independent of UI, core, commands, session |

## 3. Topic sheets

Each sheet holds only pointers to the rule (`docs/`, `.claude/conventions.md`) and a checklist; a rule that exists only here moves to its home first. Read the sheet of every topic you touch, then its sources.

| Sheet | Read when |
|---|---|
| [layer-and-graph](references/layer-and-graph.md) | graph gesture, layer, adding series, action that reads the zoom |
| [docked-detached](references/docked-detached.md) | any UI surface, new `AppState` field, global shortcut |
| [code-layout](references/code-layout.md) | new module, menu, window, building block, splitting `app.rs` |
| [appearance](references/appearance.md) | scrolling, tabs, lists, bold, titles, forms, colours, icons |
| [actions-and-menus](references/actions-and-menus.md) | every feature (where is it found?), new action, shortcut, menu or right-click entry |
| [windows-dialogs](references/windows-dialogs.md) | window, dialog, wizard, confirmation, fixed panel |
| [behaviour-standard](references/behaviour-standard.md) | field, message, greying, number, keyboard, mouse, long task |
| [object-manager-analysis](references/object-manager-analysis.md) | Object Manager, graph selection, analysis correction |

Texts: `stokesforge-ui-texts`. Commands, rendering, import, Python: their own skills.

## 4. Test, look, document

- `AppState` logic: `crates/stokesforge-app-state/tests/shell.rs`; a painted gesture: the surface module's `tests`. Placement and witnesses: `stokesforge-verification`.
- Tests cover geometry, not appearance: a visual change is **looked at** (`run-stokesforge`), before and after, also with `STOKESFORGE_LANG=en-XA`.
- `//!` of the module carries the *why*; the `docs/` page describes the code as it is. A new rule goes to `.claude/conventions.md` or `docs/`, in the same commit.

Finish with `stokesforge-verification`. Done when every touched topic sheet is ticked, the place in the menus is stated in the report, and verification is green.
