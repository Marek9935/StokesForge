---
name: stokesforge-feature
description: "Implement a StokesForge feature end to end — layer, menus, toolbars, icons, shortcut, texts, tests, docs. Use for any change made in the repo, even one line (« ajoute » / add, « implémente » / implement, « fais que… » / make it so that…, « traite #N », « lance #N », « lance le package sur les axes » — any phrasing that starts a work package, with or without a number); during issue reporting, only once implementation is explicitly asked."
---

A feature almost always spans several layers. This skill keeps each piece in the right one and none forgotten.

**Context gate.** While the conversation collects feedback or fills issues, an imperative (« ajoute », « enlève ») is content for the issue, not a request to code: follow `stokesforge-bug-report`. Start here only on an explicit request to change the repo.

## 1. Before writing code

**Identify the work (#420).** The user launches in plain words: « lance #392 », « lance l'issue 392 », « lance le package sur la traduction ». With a number, take it. Without one, find it yourself: `glab issue list --per-page 100 -O json` (titles and descriptions of open issues), the parent of a package first. One clear match: say it in one line (« #345 Manuel utilisateur en anglais, lot de 1 sous-issue ») and run `scripts/chantier.sh` at once. In doubt, ask which issue, with the candidates as options (a quick process question: `AskUserQuestion`): several plausible issues, only a closed or neighbouring one (« la doc en anglais »: #346 closed, #345 is the manual), or nothing found (then offer to open the issue, `stokesforge-bug-report`). Never launch on a guess.

Branch `feature/<n>-<topic>`, one per issue, short (an issue labelled `ligne-PostV1.1` starts from `origin/PostV1.1` and is merged into that line, not `main`: commits say `Refs #<n>`, `CLAUDE.md` § Conventions, #480); merge `main` into it once, just before delivering (not at every step; a stacked branch takes its base once, before its own delivery, not at every retour), then merge it into `main` with `--no-ff` (`docs/v1-criteria.md` § Branches and versions). Work in the worktree `../_stokesforge/feature<n>` (`<n>` = issue number), created by `scripts/chantier.sh <n>…`, which you run first, from the main checkout, in the current session, and whose briefing you follow (a package `lance #<parent>` with sub-issues: no worktree until the start form is answered, then `scripts/chantier.sh <parent> --start <sub-issue>` for each sub-issue in turn, all in the one package colour; sub-issues are delivered and merged one by one, in the parent's order, independent ones may overlap; #419): every message to the user then starts with the tag it prints (#418) (from an up-to-date `origin/main`, with the work package colour; idempotent after a `/clear`; `CLAUDE.md` § Conventions, "Worktrees"), never elsewhere. One orchestrator session per package (#517): do not carry a session from one package into the next, and wake an idle orchestrator with `/clear` + `lance #<n>` (the worktree, the record and the briefing come back) rather than resuming a context of hundreds of thousands of tokens. Several orchestrators may work in parallel: before scoping, run `git worktree list`, `scripts/suivi.py status` (who holds which colour and why; paused issues; never edit `.suivi` by hand), `scripts/ci-status.sh` on `main` (a red `main` is reported first), read the issue, **all its comments**, its sibling issues, and the tracking page (`python3 scripts/suivi.py status`, [suivi](../stokesforge-verification/references/suivi.md)); keep out of the other worktrees.

1. `git status --short`, then `git diff` on the target files: preserve the user's work in progress.
2. Read the domain page first, never the whole workspace: `.claude/rules.md` § Routing, `docs/invariants.md`, then the matching `.claude/rules/<domain>.md` (column "Rules file" of the routing). **Read it explicitly with the Read tool**: a path-scoped rule loads only when Read, Edit or Write touches a matching file, never through `rg` or `cat`.
3. Search what exists (`rg`): an `Action`, a `Command`, a helper that already does most of it. Reuse beats writing.
4. A limit that looks wrong may be arbitrated: `docs/archive/rustrewrite/questions-ouvertes.md` §11.
5. Run `perimetre` first (`CLAUDE.md` § Delegation and model choice): its edge cases decide the cards. Resolve the `changes behaviour` ones now, in **one start form** (first card: a one-line understanding of the issue; several issues may share it; sketch or mockup of a new surface inside the form, W4), posed locally with `scripts/suivi.py qcm pose` (« En attente » tab; end the turn, the agent wakes on the answers: `.claude/skills/stokesforge-verification/references/suivi.md` § The agent wakes). Lots that no answer can change start when the form is posed (W8); name the running lots in the report and in `suivi.py set --what`. A question born during coding waits for the single follow-up form: [`../stokesforge-bug-report/references/decision-form.md`](../stokesforge-bug-report/references/decision-form.md) says when and how to ask.
6. New window, dialog or gesture: name the existing twin you imitate (`docs/windows.md`, `docs/gestures.md`), compare screenshots with `run-stokesforge`; a new window gets a mockup the user validates before code.
7. Announce what you reuse, the zone you change and the checks you will run, then code. Scope and delegation: `CLAUDE.md` § Delegation and model choice (agent `perimetre`, Sonnet implements).

**Agent prompts.** Start every implementing or pre-trial agent from the templates and the shared "Bans" block (no sleep/until/`pgrep -f` loops, `type -a cargo` first, no `git stash`, wait with `scripts/wait-for.sh`): [references/agent-prompts.md](references/agent-prompts.md).

**Stacked work packages.** When B needs A's unmerged code, do not wait for A's trial: `git reset --hard <A head>` in B's fresh worktree (no own commit yet), then `perimetre` and Sonnet; merge `main` into B after A lands.

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

Each sheet holds only pointers to the rule (`docs/`, `.claude/rules/`) and a checklist; a rule that exists only here moves to its home first. Read the sheet of every topic you touch, then its sources.

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
- **Trial of a visible change (#413, #423)**: when the change is visible, first write the **trial checklist** on the tracking page: at most 5 guided routes of 2 to 3 minutes, each step « gesture → what you must see », in the user's words, each entry with its `goal` (« On teste : … ») and the last one with its `around` (« Essaie autour : … ») (`test add` refuses a batch without them, #518), with `try: "scripts/try.sh <n>"` (procedure: `.claude/skills/stokesforge-verification/references/suivi.md` § Trial checklist, `suivi.py test add`). Never the checklist only in the terminal. Before the entries, when a gesture or a drawing changes, the **pre-trial** (`.claude/skills/stokesforge-verification/references/suivi.md` § Pre-trial): an agent checks, fixes and re-verifies in the real app, so the user tests only what works. Then ask the user to run `scripts/try.sh <n>` from the main checkout (builds the branch and opens it on their screen, with their own preferences and the demo project open) and to tick the entries on the page, then end the turn (the agent wakes on the answers); with all entries OK, announce it and merge on the user's go-ahead (button or word). After a late fix, rerun the app and look again before delivering, and redo the entries it touches (`test remove`, `test add`); a retour fixed without a new trial is settled before delivering: `suivi.py test settle <id> --commit <fix> --note "…"` (`deliver.sh` refuses otherwise, #496). An « OK + idée » never blocks the merge, but is settled the same way before delivering (now in the same issue, or `--note "#N"` for a new one).
- **Live touch-up (#515)**: while the user is at the screen, a small change (about 30 changed lines, no new window, no new text or command; more lines with the reason stated in the report) is edited by the orchestrator itself, with no Sonnet agent (`CLAUDE.md` § Delegation and model choice). Edit, rebuild (about 13 s), reopen the app with `scripts/try.sh <n>`, and run `cargo nextest run -p stokesforge-app` in the background while the user looks. Clippy, the full tests and the screenshots wait for the delivery. When the user is away, normal delegation.
- Manual: a user-visible feature is described in `manual/src/` in the same commit (rule in `CLAUDE.md`, « Conventions »). **No screenshot regeneration during work or retour rounds**: `scripts/deliver.sh` regenerates them at the merge (#515). Only a NEW shot added to the manifest is rendered on the branch, with its name filter (`scripts/manual-screenshots.sh <name>`).
- `//!` of the module carries the *why*; the `docs/` page describes the code as it is. A new rule goes to `.claude/rules/<domain>.md` or `docs/`, in the same commit.

Finish with `stokesforge-verification`. Done when every touched topic sheet is ticked, the place in the menus is stated in the report, and verification is green.
