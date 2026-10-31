---
name: stokesforge-feature
description: "Implement a StokesForge feature end to end — layer, menus, toolbars, icons, shortcut, texts, tests, docs. Use for any change made in the repo, even one line (« ajoute » / add, « implémente » / implement, « fais que… » / make it so that…, « traite #N », « lance #N », « lance le package sur les axes » — any phrasing that starts a work package, with or without a number); during issue reporting, only once implementation is explicitly asked."
---

A feature almost always spans several layers. This skill keeps each piece in the right one and none forgotten.

**Context gate.** While the conversation collects feedback or fills issues, an imperative (« ajoute », « enlève ») is content for the issue, not a request to code: follow `stokesforge-bug-report`. Start here only on an explicit request to change the repo.

## 1. Before writing code

**Identify the work (#420).** The user launches in plain words: « lance #392 », « lance l'issue 392 », « lance le package sur la traduction ». With a number, take it. Without one, find it yourself: `glab issue list --per-page 100 -O json` (titles and descriptions of open issues), the parent of a package first. One clear match: say it in one line (« #345 Manuel utilisateur en anglais, lot de 1 sous-issue ») and run `scripts/chantier.sh` at once. In doubt, ask which issue, with the candidates as options (a quick process question: `AskUserQuestion`): several plausible issues, only a closed or neighbouring one (« la doc en anglais »: #346 closed, #345 is the manual), or nothing found (then offer to open the issue, `stokesforge-bug-report`). Never launch on a guess.

Branch `feature/<n>-<topic>`, one per issue, short; merge `main` into it once, just before delivering (not at every step), then merge it into `main` with `--no-ff` (`docs/v1-criteria.md` § Branches and versions). Work in the worktree `../_stokesforge/feature<n>` (`<n>` = issue number), created by `scripts/chantier.sh <n>…`, which you run first, from the main checkout, in the current session, and whose briefing you follow (a package `lance #<parent>` with sub-issues: no worktree until the start form is answered, then `scripts/chantier.sh <parent> --start <sub-issue>` for each sub-issue in turn, all in the one package colour; sub-issues are delivered and merged one by one, in the parent's order, independent ones may overlap; #419): every message to the user then starts with the tag it prints (#418) (from an up-to-date `origin/main`, with the work package colour; idempotent after a `/clear`; `CLAUDE.md` § Conventions, "Worktrees"), never elsewhere. Several orchestrators may work in parallel: before scoping, run `git worktree list`, `scripts/ci-status.sh` on `main` (a red `main` is reported first), read the issue, **all its comments**, its sibling issues, and the `chantiers` collection of the To-test page ([to-test-page](../stokesforge-verification/references/to-test-page.md)); keep out of the other worktrees.

1. `git status --short`, then `git diff` on the target files: preserve the user's work in progress.
2. Read the domain page first, never the whole workspace: `.claude/rules.md` § Routing, `docs/invariants.md`, then the domain section of `.claude/conventions.md` (table in `CLAUDE.md`).
3. Search what exists (`rg`): an `Action`, a `Command`, a helper that already does most of it. Reuse beats writing.
4. A limit that looks wrong may be arbitrated: `docs/archive/rustrewrite/questions-ouvertes.md` §11.
5. Resolve edge-case ambiguities now, in **one start form** (several issues may share it; sketch or mockup of a new surface inside the form, W4), then work on what the answers do not change while waiting (W8). A question born during coding waits for the single follow-up form: [`../stokesforge-bug-report/references/decision-form.md`](../stokesforge-bug-report/references/decision-form.md) says when and how to ask.
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
- **Trial of a visible change (#413)**: when the change is visible, ask the user to run `scripts/try.sh <n>` from the main checkout (builds the branch and opens it on their screen, with their own preferences and the demo project open); their OK is the merge go-ahead. After a late fix, rerun the app and look again before delivering. The merge adds an entry to the To-test page (`stokesforge-verification`).
- Manual: a user-visible feature is described in `manual/src/` in the same commit (rule in `CLAUDE.md`, « Conventions »); a changed interface regenerates its screenshot.
- `//!` of the module carries the *why*; the `docs/` page describes the code as it is. A new rule goes to `.claude/conventions.md` or `docs/`, in the same commit.

Finish with `stokesforge-verification`. Done when every touched topic sheet is ticked, the place in the menus is stated in the report, and verification is green.
