# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Context

Branch `main`: port of StokesForge (analysis and plotting of scientific data, OriginLab-style) from Python/Qt to Rust. All the code lives in the Cargo workspace at the root. The old Python/Qt implementation (`legacy-python/`, which served as the behavior spec) was deleted from this branch. It still exists under the tag `legacy-python`.

Code documentation: `docs/`. [`docs/README.md`](docs/README.md) is the entry point and routes by task. `docs/invariants.md` lists the rules not to break; read them before touching rendering, history or the actor. `docs/principes-ui.md` sets the interface principles (E3). `docs/fenetres.md` catalogs the existing windows; check it before adding any new window.

`docs/archive/` keeps the earlier design documents (technical specification, roadmaps 0-5 and R0-R8, robustness reports, explored options). They are no longer maintained, but **code comments cite them by file name** (`architecture-choisie.md` §4.3, `guide-de-correction-robustesse.md` §F6…): every such citation resolves in `docs/archive/`. Where they differ from the code, the code is right. They predate the renaming (#87): they still say PlotYx, `plotyx-*`, `.plx` and `pyx`, which now read StokesForge, `stokesforge-*`, `.stf` and `pyst` (the file `format-fichier-plx.md` keeps its name).

Exception: the bug reports `retours-2026-09-23-bugs.md` and `retours-2026-09-24-bugs.md` (B1…B9, "Bug 1"…), cited in `crates/stokesforge-app/src/app.rs` and `src/app/`, were deleted. All their bugs are fixed. Read them with `git show a6fd933^:docs/chantiers/<file>`. Bugs are now tracked as GitLab issues (`glab issue list --label bug`), no longer in `.md` files.

## Commands

```sh
cargo build --workspace
cargo test --workspace                       # includes stokesforge-robustesse (except #[ignore])
cargo test -p <crate> <test_name>             # a single test
cargo test -p stokesforge-robustesse -- --ignored  # costly benches
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Architecture (crates/, dependencies bottom→top)

`stokesforge-id` (stable keys) + `stokesforge-plot` (graph description, neutral toward the renderer) + `stokesforge-analysis` (pure computation, #181: baselines, correction, integral, fit, peaks, deconvolution; no workspace dependency, no UI, no Python) → `stokesforge-core` (model Project/Folder/Workbook/Worksheet/Column, zero UI dependency) → `stokesforge-commands` (bus of undoable commands) and `stokesforge-persistence` (`.stf` format), two **parallel** crates above `core`/`plot` → `stokesforge-session` (actor that owns the model for its clients, concurrency; depends on commands **and** persistence) → `stokesforge-script`/`stokesforge-python` (PyO3 bridge) → `stokesforge-app-state` (the application state, its Fluent texts and its errors put into sentences: **no** egui/eframe/winit dependency, enforced by the compiler and `tests/no_ui_dependency.rs`, #263) → `stokesforge-app` (UI shell on eframe: paints the state, translates keyboard and mouse into actions; it also keeps the offer of the figure to the clipboard, `stokesforge-clipboard` pulling in winit). `stokesforge-cli import` builds a fresh `Project` directly, without a session. `stokesforge-cli run` (feature `python`) goes through `ScriptEngine` and a headless session, with the same commands and undoable groups as the console. `stokesforge-import` (instrument files) depends on no workspace crate. `stokesforge-importer` turns them into model columns (shared by the application, the CLI and the `pyst.import_file` scripts, without commands). `stokesforge-clipboard` (multi-format offer to the system clipboard: SVG, PNG, PDF of a figure; Wayland through the `vendor/smithay-clipboard` patch, X11, Windows, macOS) depends on no workspace crate. `stokesforge-arch-guard` is the test tool for the dependency guards. The exact graph is in `docs/architecture.md`.

Rendering (phases R0-R8 closed): `stokesforge-render` (single text measurement through `epaint`, Plotters pipeline generic over the backend, and **all the rendering**: series types, axis scales, pixel primitives, clipping, legend, LTTB, interaction, Typst formulas, example graphs) → `stokesforge-render-egui` (`DrawingBackend` backend collecting `egui::Shape`s, cache `(revision, rect, pixels_per_point, view_generation)`, zoom/pan, *feathering* lever) and `stokesforge-render-svg` (SVG export, and PDF and PNG on top). Details: `docs/rendu.md`.

`stokesforge-robustesse`: no production code, depends on the whole workspace. Twenty-two batteries of tests (`b1`…`b22`, see its `src/lib.rs`) that test the base at its limits.

Feature outside `default`: `python` (PyO3 bridge, requires `PYO3_CONFIG_FILE`; see `scripts/python-test.sh`), the only one in the repository. `cargo build --workspace` does not compile it. `scripts/python-test.sh` tests the bridge, scripts, robustness, application and CLI; the CI job `test-python` is automatic and blocking. Typst (scientific notation) is a firm dependency of the three rendering crates: `cargo build/test --workspace` compile and test it.

Loading rules by domain (which crate/doc to load for a task): `.claude/rules.md`.

**Conventions by domain: `.claude/conventions.md`**. Read them before writing code in the domain. They carry the same weight as those of this file:

| Before touching… | Read the section |
|---|---|
| `stokesforge-app` / `stokesforge-app-state` (window, field, icon, color, text, toolbar, tabs, tree, scrolling, Wayland; state, preferences, texts) | "Application" |
| Object Manager, PM/OM/console panels, menus and right-clicks | "Object Manager, fixed panels, menus" |
| graph creation, detached windows | "New graphs and detached windows" |
| series, series groups | "Series and groups" |
| copy/paste of objects, figure copy | "Copying objects and figures" |
| analysis (`stokesforge-analysis`, computations) | "Analysis" |
| `pyst` (writes, navigation) | "`pyst` scripts" |
| errors (`Display`, `error_message`, `error-*` keys, status messages) | "Errors" |
| new window or dialog | `docs/fenetres.md` (catalog: open a sister reference window, #300), `docs/principes-ui.md` (E3) |
| rendering (`stokesforge-render*`) | "Rendering: silent pitfalls" (+ skill `stokesforge-rendu`) |

Project skills: `.claude/skills/`. Eleven skills by type of task
(implement a feature, report then fix a bug, add an undoable command,
rendering, import, Python bridge, verify before committing,
drive the real application, answer about features and
architecture, write an interface text), written in English. Index and
sequences: [`.claude/skills/README.md`](.claude/skills/README.md).

## Conventions

- License (#129): pure GPL-3.0-or-later (#348: the extension exception of 1.0.0 is removed; an additional permission is granted only in writing, case by case, and requires a license CLA for contributors' code). Preserve the licenses of third-party components; a changed dependency or runtime updates the notices and passes `cargo deny check licenses`. Distributions use `scripts/licenses-dist.sh`. External contributions carry the DCO of `CONTRIBUTING.md`; **merge no external contribution before the license CLA** (#348).
- Rust notices: `scripts/licenses-rust.sh` adds the missing upstream copyrights of `vendor/crate-license-notices/`. Their fingerprint ties the report to the exact `Cargo.lock`; any change to the lock requires revalidating these supplements before generating the notices.
- `rustfmt` + `clippy -D warnings` are mandatory (GitHub CI `.github/workflows/ci.yml`).
- CI: `.github/workflows/ci.yml` (Linux and Windows, hosted runners) mirrors
  the GitLab commands. It builds the release packages and creates the
  GitLab release (`publier`, #333). The workflow `gitlab-status.yml` sends back the
  check results via `scripts/gitlab-status.py`, tested without network by `gitlab-status-test.py`.
  GitHub secret `GITLAB_STATUS_TOKEN`, instructions, runner pitfalls and
  maintenance are in `docs/tests.md` ("Using CI day to day"). Results of a commit, read from GitLab: `scripts/ci-statuts.sh [commit]`.
- When a work package installs a new rule (interface standard, pattern to follow, boundary), update **as needed** the skills concerned (`.claude/skills/`) and this file or `.claude/conventions.md`, in the same commit as the code, to keep development consistent. See `.claude/skills/README.md` (cross-cutting rules).
- Decisions for the user to make: up to four, `AskUserQuestion` (one slide per question); **five or more**, a decision form (multiple-choice) published as an Artifact, never a list of questions in the terminal (one card per decision: context, before and after as diagrams, advantage and flaw of each option; the user is dyslexic). The form itself stays in French (the user reads French). Template and procedure: `.claude/skills/stokesforge-bug-report/references/formulaire-de-decisions.md`.
- Bugs and changes are tracked as GitLab issues (`glab issue list`), not in `.md` files. Each issue carries a line "**Cohérence**" that says which skills and docs to update.
- While reporting and while creating or enriching issues, the feedback that follows completes the issues, even when phrased as « ajoute » or « supprime ». Move to code only on an explicit request to implement in the repository; see the context rule of `stokesforge-bug-report`.
- Versions (#333): all crates share the same version `X.Y.Z` (today `1.0.0`), changed by a dedicated commit with `cargo update --workspace` and a section of `CHANGELOG.md`. `main` is the shippable line: fixes and small additions are tagged there directly (`v1.0.1`, `v1.1.0`). A `vN` branch opens only the day a breaking work package would prevent shipping a fix, and receives `main` by regular merge. No promise of code compatibility from one major to the next; only `.stf` files (invariant 12, `docs/persistance.md`) and the `pyst` API (`docs/python-et-script.md`) hold. The rule "the `.stf` format may be broken during development" has been **lifted** since V1. A bug is fixed where it was born (`release/N.x` only to maintain an old major); work on `feature/<n>-<topic>`, `--no-ff` merge into `main`. Tag `vX.Y.Z` → release; criteria and procedure: `docs/criteres-v1.md`. **Never create or push a tag (`v1.0.0-rc1`, `v1.0.0`, any `v*`) without an explicit order from the user**: a tag starts the release, which is visible and hard to undo (GitHub mirror included).
- Commit messages and technical docs are in English (since #368; the earlier history stays in French). `docs/archive/` stays in French. Code comments and French identifiers are being translated crate by crate (#372–#376). The decision form for the user (multiple-choice Artifact) stays in French.
- After a merge and push on `main`, clean up the task's temporary worktree and its merged local branch automatically, unless asked to keep them; follow `stokesforge-verification`, "Clean up after a merge on `main`".
- Every model mutation goes through an undoable command of `stokesforge-commands`, never through a direct `core` primitive.
- A history label is **data** (`stokesforge_commands::HistoryLabel`, closed `MacroKind` for `begin_macro`), never a sentence: the crates below put nothing into sentences, `stokesforge-app-state` does it through `history_text` and the `history-*` keys (#46).
- Errors: the `Display` (`#[error(...)]`) of the crates below is in **raw English**, never translated (CLI, `pyst`, logs). `stokesforge-app-state` puts them into sentences through `errors::<family>::error_message`, which maps each variant to an `error-*` key of the `en` catalog (only exception, `ClipboardError`: `app/figure_copy.rs`, because `stokesforge-clipboard` stays in the shell). An exhaustive `match`, without `_`, forces any new variant to be mapped. Never `to_string()` or `{err}` of an error shown to the user. `std::io::Error`s go through `errors::io::error_message` (keys `error-io-*`). Since `ErrorKind` is `#[non_exhaustive]`, its `match` keeps a `_` fallback (`error-io-other`), the only exception to "without `_`", commented in the code. Status bar messages (`AppState::notice`/`fail`) are `tr!` calls (keys `status-*` / `error-*`), guarded by `aucun_libelle_en_dur` (`stokesforge-app/tests/libelles.rs`, which reads the sources of both crates).

## Known pitfalls

- Before undo/history or the `.stf` format: `docs/commandes-et-historique.md`, `docs/persistance.md`, then `docs/archive/rustrewrite/format-fichier-plx.md` for the options already ruled out.
- `docs/archive/rustrewrite/questions-ouvertes.md` §11: documented caps/counters. Check before "fixing" a behavior that looks like a bug.
- Formula recalculation (#193): targeted by fingerprint (`stokesforge-session/src/recalc.rs`, invariant 11). **Any data read by a formula computation enters the fingerprint** (a column's version covers only type and values, not name, meta or position; membership of a read column in the sheet enters it, the displayed letters do not: a formula keeps its columns by identifier, #321), and **any data write to a column draws a fresh version** per `Project`. Full recalculation remains as the oracle (`FormulaRecalc::Full`, battery `b22`). To extend the mechanism (formula function, #184, #186): tests to write are in `docs/tests.md`, "Targeted recalculation".

## Delegation and model choice

Goal: keep the main session's context small, and reserve Opus for judgment, with short output.

- **Implementation (feature, fix, restructuring)**: Opus only sets the scope, through the `perimetre` agent (`.claude/agents/perimetre.md`: files to touch, test cases and witnesses, pitfalls, 40 lines at most, no code). **Sonnet** implements and writes the tests from this scope, then checks each witness by mutation. No Opus review at the end of a task, unless asked.
- A search that needs reading **a lot** (more than ten files, or not knowing where to look) goes to the `recherche` agent (`.claude/agents/recherche.md`, Haiku). Read a known file yourself, or do a targeted search (one `rg`, two files): an agent's cold start costs more.
- Its answer cites `file:line`; re-check yourself any conclusion of **absence** ("does not exist", "nobody calls") and any excerpt you rely on to write code.
- Never delegated to Haiku: design, data model, commands, invariants, tests (scope by `perimetre`, writing by Sonnet), interface texts, choices to be decided.
- Long output of a command (`cargo test`, `clippy`, `cargo build`): no agent. Redirect the full log to a file (`> "$tmp/test.log" 2>&1`) and show only the summary (`rg -n 'FAILED|panicked|^error|^warning' …`, `tail`); re-read the file if needed rather than rerun.
