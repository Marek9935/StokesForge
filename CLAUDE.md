# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Interaction language

Interaction language: **French**. Agents use it to talk to the developer: answers, questions, decision forms. Change this line to work in another language. The repository itself (code, docs, skills, commit messages) stays English whatever this line says.

## Context

Branch `main`: port of StokesForge (analysis and plotting of scientific data, OriginLab-style) from Python/Qt to Rust. All the code lives in the Cargo workspace at the root. The old Python/Qt implementation (`legacy-python/`, which served as the behavior spec) was deleted from this branch. It still exists under the tag `legacy-python`.

Code documentation: `docs/`. [`docs/README.md`](docs/README.md) is the entry point and routes by task. `docs/invariants.md` lists the rules not to break; read them before touching rendering, history or the actor. `docs/ui-principles.md` sets the interface principles (E3). `docs/windows.md` catalogs the existing windows; check it before adding any new window.

`docs/archive/` keeps the earlier design documents (specification, roadmaps, robustness reports). They are no longer maintained, but **code comments cite them by file name**: every citation resolves there. Where they differ from the code, the code is right. Details and the deleted bug reports: `.claude/rules.md` § `docs/archive/`.

## Commands

```sh
cargo build --workspace
scripts/test-all.sh                          # whole workspace: nextest + doc tests if cargo-nextest is installed, else cargo test --workspace
cargo test --workspace                       # includes stokesforge-robustness (except #[ignore])
cargo test -p <crate> <test_name>             # a single test
cargo test -p stokesforge-robustness -- --ignored  # costly benches
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
python3 scripts/french-guard.py              # no French outside exempt places / `lang: fr` markers
python3 scripts/cargo-queue-test.py          # the local cargo queue and build modes; install: scripts/cargo-queue-install.sh
```

## Architecture (crates/, dependencies bottom→top)

`stokesforge-id`, `stokesforge-plot` (graph description, neutral toward the renderer) and `stokesforge-analysis` (pure computation, no workspace dependency) → `stokesforge-core` (model Project/Folder/Workbook/Worksheet/Column, zero UI dependency) → `stokesforge-commands` (undoable commands) and `stokesforge-persistence` (`.stf`), two **parallel** crates → `stokesforge-session` (actor that owns the model) → `stokesforge-script`/`stokesforge-python` (PyO3 bridge) → `stokesforge-app-state` (application state, texts, errors put into sentences) → `stokesforge-app` (UI shell on eframe). Rendering: `stokesforge-render` (all the rendering, generic over the backend) → `stokesforge-render-egui` and `stokesforge-render-svg` (SVG, PDF, PNG export).

Boundaries held by the compiler or a guard: `stokesforge-app-state` has **no** egui/eframe/winit dependency (`tests/no_ui_dependency.rs`); `stokesforge-analysis` and `stokesforge-import` depend on no workspace crate; `stokesforge-cli import` builds a `Project` without a session. The exact graph is in `docs/architecture.md`; rendering details in `docs/rendering.md`.

`stokesforge-robustness`: no production code, depends on the whole workspace. Twenty-three batteries of tests (`b1`…`b23`, see its `src/lib.rs`) that test the base at its limits.

Feature outside `default`: `python` (PyO3 bridge, requires `PYO3_CONFIG_FILE`; see `scripts/python-test.sh`), the only one in the repository. `cargo build --workspace` does not compile it. `scripts/python-test.sh` tests the bridge, scripts, robustness, application and CLI; the CI job `test-python` is automatic and blocking. Typst (scientific notation) is a firm dependency of the three rendering crates: `cargo build/test --workspace` compile and test it.

Rules by domain: routing (what to load, what not to open) in `.claude/rules.md`; the rules themselves in `.claude/rules/<domain>.md`, loaded automatically when a matching file is read, edited or written (never by `rg` or `cat`: **Read the file explicitly** before scoping). They carry the same weight as this file. Skills: `.claude/skills/` (index in `.claude/skills/README.md`).

## Conventions

- License: pure GPL-3.0-or-later (an additional permission is granted only in writing, case by case, and needs the rights on all the code concerned). `assets/logo/` is under the same GPL; no change to the logo without the user's order (a design choice). Packaged builds carry it: `scripts/logo-dist.sh` (opt-in `STOKESFORGE_DIST_LOGO=1`), the app reads the installed copy at run time (`app_logo.rs`). A changed dependency or runtime updates the notices and passes `cargo deny check licenses` (`scripts/licenses-dist.sh`, `scripts/licenses-rust.sh`: revalidate the `vendor/crate-license-notices/` supplements after any `Cargo.lock` change). External contributions carry the DCO of `CONTRIBUTING.md`; **before merging the first external contribution, decide whether a license CLA or an additional permission is needed** (`docs/license-and-extensions.md`).
- `rustfmt` + `clippy -D warnings` are mandatory.
- CI (`.github/workflows/ci.yml`, Linux and Windows) mirrors the GitLab commands and builds the release packages; instructions, runner pitfalls and maintenance: `docs/tests.md` ("Using CI day to day"). Results of a commit: `scripts/ci-status.sh [commit]`. The orchestrator does not poll the CI: it reports at the merge, closes and releases; it reads the CI only for a tag or release and for a change to CI itself.
- When a work package installs a new rule, update the skills concerned and this file or the matching `.claude/rules/<domain>.md` in the same commit as the code. **One rule added = one rule removed or turned into a script, test or setting**, reviewed at each workflow change.
- Decisions for the user are **always** the decision form (one start batch, one follow-up batch per work package), never a list of questions in the terminal: `.claude/skills/stokesforge-bug-report/references/decision-form.md`. The form is written in the interaction language.
- Bugs and changes are tracked as GitLab issues (`glab issue list`), not in `.md` files. Each issue carries a line "**Cohérence**" that says which skills and docs to update.
- While reporting and while creating or enriching issues, the feedback that follows completes the issues, even when phrased as « ajoute » or « supprime ». Move to code only on an explicit request to implement in the repository; see the context rule of `stokesforge-bug-report`.
- Versions: all crates share the same version `X.Y.Z`, changed by a dedicated commit with `cargo update --workspace` and a section of `CHANGELOG.md`. `main` is the shippable line: fixes and small additions are tagged there directly. A `vN` branch opens only the day a breaking work package would prevent shipping a fix, and receives `main` by regular merge. No promise of code compatibility from one major to the next; only `.stf` files hold (invariant 12, `docs/persistence.md`). The `pyst` API is **alpha** (`pyst.__api_version__` = `0.x`), checked by CI against a frozen snapshot (`docs/python-and-scripting.md`). A bug is fixed where it was born; work on `feature/<n>-<topic>`, `--no-ff` merge into `main` (automatic once green, except a visible change, whose go-ahead is the user's (the « Fusionner » button or their word) once the trial entries are OK; rule in `stokesforge-verification`, step 6). **Never create or push a tag (any `v*`) without an explicit order from the user**: a tag starts the release, which is visible and hard to undo. Settings ask for approval on `git tag`; criteria and procedure: `docs/v1-criteria.md`.
- Commit messages and technical docs are in English (the earlier history stays in French). `docs/archive/` stays in French. French elsewhere only in exempt places or behind a `lang: fr` marker (`scripts/french-guard.py`, `docs/tests.md`). New code, comments, test names and identifiers are in English.
- After a merge and push on `main`, clean up the task's worktree and merged branch automatically, unless asked to keep them (`stokesforge-verification`, "Clean up after a merge on `main`").
- Worktrees: a task's worktree lives in `../_stokesforge/feature<n>`, created by `scripts/chantier.sh <n>` (never a `StokesForge-<n>` folder, nor `.claude/worktrees/`). Every message of an orchestrator on a work package starts with its colour tag `<square> #<n> <short name>`. Merges happen only in the main checkout through `scripts/deliver.sh <n> <message-file>`. Never `git stash` (denied in settings: the stash stack is shared by every worktree). Colour rule, lifecycle, integration lines: `.claude/skills/stokesforge-verification/references/suivi.md` § Work packages and worktrees.
- X11 is not supported: "it works, but is untested"; no card, trial or probe for it (`docs/windows.md` § Platforms).
- User manual: every user-visible feature updates `manual/src/` in the same commit, with labels copied from the `en` catalog. Screenshots are regenerated only at the merge, by `scripts/deliver.sh`, never during work or retour rounds; a NEW shot added to `manual/screenshots.json` is rendered on the branch with `scripts/manual-screenshots.sh <name>`. `stokesforge-app/tests/manual_labels.rs` refuses a menu entry the manual does not name.
- Every model mutation goes through an undoable command of `stokesforge-commands`, never through a direct `core` primitive.
- A history label is **data** (`stokesforge_commands::HistoryLabel`, closed `MacroKind`), never a sentence: `stokesforge-app-state` phrases it (`history_text`, `history-*` keys).
- Errors: the `Display` of the crates below is raw English, never translated; `stokesforge-app-state` puts it into sentences through `errors::<family>::error_message` and `error-*` keys. Never `to_string()` or `{err}` of an error shown to the user. Chain and exceptions: `.claude/rules/errors.md`.

## Known pitfalls

- Before undo/history or the `.stf` format: `docs/commands-and-history.md`, `docs/persistence.md`, then `docs/archive/rustrewrite/format-fichier-plx.md` for the options already ruled out.
- `docs/archive/rustrewrite/questions-ouvertes.md` §11: documented caps/counters. Check before "fixing" a behavior that looks like a bug.
- Formula recalculation is targeted by fingerprint (invariant 11): any data a formula reads enters the fingerprint. Rules: `.claude/rules/formulas.md`.

## Delegation and model choice

Goal: keep the main session's context small, and reserve Opus for judgment, with short output.

- Several orchestrator sessions may coexist; one issue is one batch, split only beyond about one agent-day. A subagent may spawn one more level of subagents, not two. Before scoping, read the issue, **all its comments**, the sibling issues and the live worktrees (`git worktree list`).
- **Implementation (feature, fix, restructuring)**: Opus only sets the scope, through the `perimetre` agent (`.claude/agents/perimetre.md`: files to touch, test cases and witnesses, pitfalls, 40 lines at most, no code). **Sonnet** implements and writes the tests from this scope (the scope comes before the start form: its edge cases become cards), in the worktree `../_stokesforge/feature<n>`, then checks each witness by mutation. No Opus review at the end of a task, unless asked. **Exception, live touch-up (#515)**: while the user is at the screen, the orchestrator edits itself, with no Sonnet agent, when the change is about 30 changed lines at most, opens no new window and adds no new text (Fluent key) or command. More lines are allowed with the reason stated in the report (e.g. only a piece of interface, no logic, low risk of side effects). When the user is away, normal delegation applies. Procedure: `.claude/skills/stokesforge-feature/SKILL.md` § 4
- A search that needs reading **a lot** (more than ten files, or not knowing where to look) goes to the `recherche` agent (`.claude/agents/recherche.md`, Haiku). Read a known file yourself, or do a targeted search (one `rg`, two files): an agent's cold start costs more.
- Its answer cites `file:line`; re-check yourself any conclusion of **absence** ("does not exist", "nobody calls") and any excerpt you rely on to write code.
- Never delegated to Haiku: design, data model, commands, invariants, tests (scope by `perimetre`, writing by Sonnet), interface texts, choices to be decided.
- Heavy cargo may wait in the local queue (#476), and a foreground Bash command is cut at 10 minutes, waiting included: run `cargo test`, `clippy`, `cargo build` in the background with the log in a file, and read the summary from it (wait for a log marker with `scripts/wait-for.sh`, never a `sleep`/`pgrep -f` loop). `SF_CARGO_QUEUE=off` skips the queue (only for a trial or an emergency).
- Long output of a command (`cargo test`, `clippy`, `cargo build`): no agent. Redirect the full log to a file (`> "$tmp/test.log" 2>&1`) and show only the summary (`rg -n 'FAILED|panicked|^error|^warning' …`, `tail`); re-read the file if needed rather than rerun.
