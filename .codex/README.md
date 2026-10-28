# Developing and maintaining StokesForge with Codex

This guide is the lasting context of the project. The `AGENTS.md` file at the
root asks to read it before any implementation. A document placed
only in the `.codex/` folder of the repository is not a root instruction
loaded automatically: see the
[official documentation](https://learn.chatgpt.com/docs/agent-configuration/agents-md).

## 1. Survey the ground before coding

1. Read `git status --short`, then `git diff` on the files concerned.
   Identify the existing changes before working on them.
2. Read `docs/README.md`, then the page of the domain below. Consult
   `docs/invariants.md` before changing the contracts of the model, history,
   the session, rendering or the Python bridge.
   Also read the useful section of [`.claude/conventions.md`](../.claude/conventions.md)
   before writing in the domain; the table "Before touching… | Read the section" of
   [`CLAUDE.md`](../CLAUDE.md) points to the right section. [`.claude/rules.md`](../.claude/rules.md)
   sets the reading scope for each task.
3. Search with `rg` and `rg --files` for the symbols, uses, similar
   components and existing tests. Read the `//!` comments of the modules
   concerned and follow the path from the caller to the implementation.
4. Check the `Cargo.toml` files concerned, `Cargo.lock`, `rust-toolchain.toml`,
   the relevant scripts and `.gitlab-ci.yml`. Use the versions and the
   APIs actually present. To inspect the workspace without building:
   `cargo metadata --no-deps --format-version 1 --offline`.
5. Identify what will be reused and choose the checks **before**
   changing. Extend the component responsible for the behaviour; justify
   any new dependency or abstraction by an observed gap.

Load the useful domain first, then widen to callers and dependents
when the contract requires it. The sources are authoritative on the implemented state; the
user's requests define the expected change. Report and
fix the relevant documentation divergences without turning the task
into a general overhaul.

### Project skills shared with Claude

The eleven skills maintained in [`.claude/skills/`](../.claude/skills/README.md), written in English,
are also available to Codex through the links in `.agents/skills/`.
They are the same files: changing the source skill in `.claude/skills/`
updates both assistants. For a given task, read the relevant `SKILL.md`
and only its useful references, after the reading path
above; do not load all eleven systematically.

The links share the whole folder, including `references/` and the scripts.
The skills point to sections of `docs/` or of `.claude/conventions.md`
instead of copying the rules; the topic sheets of
`stokesforge-feature` (#301) hold only these pointers: read only
those of the topic touched, then the cited sources. Maintain the rules in
these sources; the test `stokesforge-arch-guard/tests/skill_references.rs`
checks the pointers of all project skills.

| Need | Skill to consult |
|---|---|
| Add a feature | [`stokesforge-feature`](../.agents/skills/stokesforge-feature/SKILL.md) |
| Diagnose a symptom, triage feedback | [`stokesforge-bug-report`](../.agents/skills/stokesforge-bug-report/SKILL.md) |
| Fix a reproduced bug | [`stokesforge-bug-fix`](../.agents/skills/stokesforge-bug-fix/SKILL.md) |
| Change the model or add an undoable action | [`stokesforge-command`](../.agents/skills/stokesforge-command/SKILL.md) |
| Plotting, axes, legends and exports | [`stokesforge-rendering`](../.agents/skills/stokesforge-rendering/SKILL.md) |
| CSV/TXT/ASC import and preview | [`stokesforge-import`](../.agents/skills/stokesforge-import/SKILL.md) |
| PyO3 bridge, scripts and Python console | [`stokesforge-python`](../.agents/skills/stokesforge-python/SKILL.md) |
| Tests, final verification and commit | [`stokesforge-verification`](../.agents/skills/stokesforge-verification/SKILL.md) |
| Launch, drive and capture the real application on Linux | [`run-stokesforge`](../.agents/skills/run-stokesforge/SKILL.md) |
| Explain what the software can do and how it is split | [`stokesforge-explain`](../.agents/skills/stokesforge-explain/SKILL.md) |
| Write or change a visible text | [`stokesforge-ui-texts`](../.agents/skills/stokesforge-ui-texts/SKILL.md) |

The skills chain as needed: `stokesforge-feature` or
`stokesforge-bug-report` → `stokesforge-bug-fix` if a fix in the code is explicitly requested →
`stokesforge-verification` after a change. Add `stokesforge-command`,
`stokesforge-rendering`, `stokesforge-import` or `stokesforge-python` only for the domains
touched. In Codex, you can invoke a skill explicitly with
`$stokesforge-feature` (not Claude's `/stokesforge-feature` syntax).
Tool mentions specific to Claude, such as `AskUserQuestion`, name
the intent of the workflow: use the question tool available in Codex
if a user decision is really needed. In case of divergence,
follow the user's request, `AGENTS.md`, this guide and the current code.

`run-stokesforge` completes verification for interactive scenarios
(menus, dialogs, input, undo). Its shared driver is
`.claude/skills/run-stokesforge/driver.py`; read the captures with the image
tool available in Codex. It runs on X11 under Xwayland and validates
neither native Wayland, nor the `rfd` picker, nor drag. The windows opened
for verification are closed at the end of the scenario, as the skill says.

While creating or enriching issues, keep the reporting context
between messages: « ajoute », « enlève » or « supprime »
describe the changes to record. Change the application only on an
explicit request to move to implementation in the repository, following the
[triage rule](../.agents/skills/stokesforge-bug-report/SKILL.md#keep-the-reporting-context).

## 2. Where to look

The paths in this table are relative to the repository root.

| Domain | Starting documentation | Code to examine |
|---|---|---|
| Split and dependencies | `docs/architecture.md` | `Cargo.toml`, crate manifests |
| Tree, columns, identifiers, graph description | `docs/data-model.md` | `stokesforge-core`, `stokesforge-id`, `stokesforge-plot` |
| Object copy and clipboard | `docs/application/managers.md` (§ Project Manager and windows by content), `docs/persistence.md` (§ Object clipboard (#105)) | `stokesforge-core/src/copy.rs`, `Project::insert_copy`, `Command::InsertCopy`, `stokesforge-persistence/src/clipboard.rs`, `stokesforge-app/src/app/project_manager.rs` |
| Undo/redo and macros | `docs/commands-and-history.md` | `stokesforge-commands`, the `stokesforge-core` methods called |
| Save, load and `.stf` format | `docs/persistence.md` | `stokesforge-persistence`, model and graph types |
| Actor, concurrency, snapshots | `docs/session.md` | `stokesforge-session`, commands and persistence called |
| Plotting, axes, legends, export, Typst | `docs/rendering.md` | `stokesforge-render` first, then `stokesforge-render-egui` / `stokesforge-render-svg`, `stokesforge-core/src/resolve.rs` |
| Menus and right-clicks | `docs/menus.md` (grammar), `docs/application/menus-and-toolbars.md` (§ Themed menus (#114)) | `stokesforge-app/src/app/menu_sections.rs`, `menus.rs`, `object_manager_ui.rs`, `project_manager.rs`, `grid.rs`, `graph_window.rs`, `tab_strip.rs` |
| Interface behaviour (fields, messages, dialogs, disabling, keyboard) | `docs/interface-standard.md` (#217, *in place* / *planned* sections) | the module of the surface in `src/app/`, `app/dialogs.rs`, `app/shortcuts.rs`, `theme.rs` |
| Window, grid, shortcuts | `docs/application/shell-and-code.md` (§ Code split: `app.rs` and `src/app/` (#115)) | `stokesforge-app-state/src/state.rs`, the module of the surface in `src/app/` (`app.rs` for orchestration only), `selection.rs`, `object_manager.rs` (also in `stokesforge-app-state/src/`), `stokesforge-app/src/theme.rs`; `stokesforge-session` |
| New window or dialog, interface gestures | `docs/windows.md`, `docs/ui-principles.md`, `docs/gestures.md` | a reference window of the same nature in `stokesforge-app/src/app/`, `app/dialogs.rs` (`DialogNature`) |
| Interface smoothness | `docs/performance-measurement.md` | `scripts/perf/`, `.claude/skills/run-stokesforge/driver.py` (`SF_APP` to choose the binary); measure in release or compare debug/release |
| CSV/TXT/ASC import and preview | `docs/import-preview-work-package.md` (handover) | `stokesforge-import/src/{recipe,parse,preview,store}.rs`, integration in `stokesforge-app-state/src/state.rs` and `src/app/import_wizard.rs`, `stokesforge-import/tests/fixtures.rs` |
| Embedded Python and `pyst` scripts | `docs/python-and-scripting.md`, `crates/stokesforge-python/README.md` | `stokesforge-python`, `stokesforge-script`, `scripts/python-*.sh` |
| Tests and robustness | `docs/tests.md` | tests of the crate; index `stokesforge-robustness/src/lib.rs`, then the battery `bN` concerned |
| Linux CI on GitHub, Windows on GitLab and results feedback | `docs/tests.md` (§ CI (GitHub Actions for Linux and Windows) and GitHub results visible in GitLab) | `.gitlab-ci.yml`, `.github/workflows/{ci,gitlab-status}.yml`, `scripts/gitlab-status{,-test}.py` |
| Distribution | `crates/stokesforge-python/README.md`, `docs/application/state-and-console.md` | `scripts/app-dist.sh`, `scripts/cli-dist.sh`, `scripts/pyhello-dist.sh`, `vendor/python/lock.env` |
| License, contributions and extension interfaces | `docs/license-and-extensions.md`, `CONTRIBUTING.md` | `LICENSE`, `deny.toml`, `about.toml`, `scripts/licenses-{tools.env,rust.sh,dist.sh,test.sh}`, `scripts/licenses.hbs`, `vendor/{crate-license-notices,epaint-default-fonts,python/licenses-{linux,windows}}/` |

Crate names mean `crates/<name>/`. The root manifest includes
`stokesforge-import`, even if some older pages do not list it.
The `stokesforge-cli` CLI also lets you check the foundation without an interface.

The documents in `docs/archive/` explain the historical decisions.
Old references to `architecture-choisie.md`, `format-fichier-plx.md`
or `questions-ouvertes.md` resolve there. The notes `docs/*-work-package.md` and
`docs/work-packages/` need a check against the code before taking up a
"remaining to do". `legacy-python/` was removed from this branch;
the old implementation stays available under the `legacy-python` tag.

## 3. Reuse the existing boundaries

- Clients submit business mutations through
  `SessionHandle::execute(Command)`; the actor owns the model, clients
  read snapshots. Preserve undo/redo, macros, revisions and identifiers.
- The project license is pure GPL-3.0-or-later, with no additional
  permission (#348). Third-party components keep their licenses; do not
  apply the project GPL to them by replacing their notices.
- `SessionHandle::new_project`, like `load`, replaces the project identity
  and empties the history and the geometries; it refuses an open macro.
  The confirmation before losing work belongs to `AppState`.
- Object copy reuses `NodeCopy` / `InsertCopy`: content frozen at the Copy
  gesture, identifiers created in the destination, atomic rollback. Names
  are unique per folder and canonical paths are rooted. The system
  text transport reuses the `.stf` conversions, without a version
  number; graphs between projects carry their columns in a persisted hidden
  workbook (#107), revealed by Show Data. F2 renames in the managers and edits in the grid (#143), depending on the focus,
  never the name of the sheets; Ctrl+D of sheets at the workbook viewport (#130).
- Copying a figure to an external application (#149) goes through
  `stokesforge-clipboard` (SVG, PNG, PDF offered together; Wayland through the patch
  `vendor/smithay-clipboard`) and `app/figure_copy.rs`, never through `copy_text`.
- In the application, reuse `Action` / `AppState::dispatch` for the
  actions concerned. Keep the testable logic in `state.rs` (crate `stokesforge-app-state`, without egui) and the
  translation of events and the display in the module of the surface
  (`src/app/`), with `app.rs` keeping only the orchestration of the frame.
- Import stays independent of the UI, of `core`, of commands and of the
  session. `stokesforge-importer` converts its results to the model columns
  for the application, the CLI and `pyst.import_file`; scripts write through
  the commands of their session, GIL released, in an undoable macro.
  Reuse recipes, parser and preview before creating a second path.
- Shared plot features go in `stokesforge-render`, generic
  over the backend. Text measurements go through `stokesforge_render::text`.
  Plotters without its fonts requires the overrides `draw_text` and
  `estimate_text_size`; its legend helpers can bypass these measurements.
- Zoom/pan belong to `ViewState`, not to a mutation of the `PlotSpec`.
  Feathering stays out of the cache key; `view_generation` stays in it.
- Preserve the boundary tests (`no_ui_dependency.rs`).
  A new dependency must respect the architecture before updating
  an allowlist.
- No Python object crosses the actor channel; the script releases the
  GIL while waiting. The interpreter starts at the first script
  run, without blocking the UI thread.
- The `pyst` surface is frozen (#331): any change to the public contract updates
  the `pyst-api` blocks of `docs/python-and-scripting.md` in the same commit.
  Read the skill `stokesforge-python`; `stokesforge-script/tests/frozen_api.rs`
  guards the agreement between these blocks and the API.
- Preserve the distinctions between missing values, empty text, numbers
  and mixed cells. For saved files, reread the conversions,
  the unknown values and the compatibility tests before changing the schema.

## 4. Choose the right checks

Commands to run from the root. The Rust toolchain is declared `stable`,
with `rustfmt` and `clippy`, in `rust-toolchain.toml`.

For a Rust change, start with the tests of the crate and the behaviour
touched (`<crate>` and `<file>` are to be replaced):

```sh
cargo test -p <crate>
cargo test -p <crate> --test <file>
```

Before delivering a Rust change, repeat the CI checks:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Linux runs on GitHub; Windows stays on GitLab. The separate workflow
`gitlab-status.yml` sends the Linux statuses back after `CI` ends, with the
GitHub secret `GITLAB_STATUS_TOKEN`; configuration in `docs/tests.md`.
A change to the reporter is checked with
`python3 scripts/gitlab-status-test.py` and validation of the YAML files. The Linux
jobs of `.gitlab-ci.yml` are enabled only with `STOKESFORGE_CI_LINUX=gitlab`.

`cargo build --workspace` builds the default targets. Avoid a
reformat unrelated to the task. For a documentation-only change,
checking the paths, the commands and the diff is enough; there is
no need to rebuild the workspace.

### Optional features

The `python` feature is not enabled by default (it is the only one outside
`default`). A check without the feature does not validate the code it guards.

Typst is no longer a feature (issue #29): it is a firm dependency of the three
rendering crates. `cargo test --workspace` and `cargo clippy --workspace
--all-targets -- -D warnings` build and test it; no targeted command
is needed. `--all-features` now enables only Python and needs its
environment.

For Python, first examine the interpreter in
`vendor/python/extracted/python/` and read the scripts. If needed,
`scripts/python-vendor.sh` downloads the versions declared in
`vendor/python/lock.env`. Then `scripts/python-test.sh` configures PyO3, the
rpath and the embedded prefix, and tests `stokesforge-python`, `stokesforge-script`,
`stokesforge-robustness` (serially), `stokesforge-app-state`, `stokesforge-app` and `stokesforge-cli` with `python`.
The CI job `test-python` runs this script automatically and blocks on
failure. The `#[ignore]` reproductions still need `-- --ignored`.
Apply the same environment to the Clippy checks with
`--features python`. Do not use the system Python as a substitute.
The optional Matplotlib spike is replayed with `scripts/python-matplotlib-spike.sh`
in a separate copy of the runtime; see `docs/python-matplotlib-spike.md`.

### Specific checks

- Rendering: check measurement, screen/SVG agreement and witnesses that must differ;
  exercise the exports concerned and the features changed.
  The manual bench for large curves (#141),
  `stokesforge-render-egui/tests/curve_bench.rs`, runs with `--release`
  and `--ignored`; protocol, command and measurements in `docs/rendering.md`.
- UI: test the logic through `AppState`; for keyboard, focus or
  presentation, complete with a check in the real application
  with `run-stokesforge` if a display is available. For texts,
  also capture with `STOKESFORGE_LANG=en-XA` (catalog and truncation).
  State the X11 limits of the driver and what stays unchecked.
- Wayland (#72): the patches `vendor/winit` and `vendor/smithay-clipboard`
  share the drop and clipboard receiver between viewports.
  Both copies are outside the workspace. The parser and the exact channel are
  tested by `stokesforge-app/tests/wayland_{uri,bridge}.rs`. Complete `fmt`
  with the targeted commands of `vendor/winit/STOKESFORGE.md`; do not
  reformat upstream files unrelated to the fix.
  Wayland is the default; `STOKESFORGE_X11=1` forces the X11 fallback. The manual
  protocol, the alternatives and the removal are in `vendor/winit/STOKESFORGE.md`.
- Robustness: read the reason of each `#[ignore]` before running it.
  `cargo test -p stokesforge-robustness -- --ignored` replays these cases; some
  may be costly or depend on scheduling. Do not remove an
  `ignore` without checking that the test becomes a reliable guard.
- Distribution: read the scripts before running them; they rebuild their
  output in `dist/`. `app-dist.sh` needs an X11/Xwayland display for its
  window check; `cli-dist.sh` checks the packaged CLI without a screen,
  with an emptied and trapped environment. Use these scripts as part of work
  on packaging, not as a simple documentation check.
- Licenses (#129): install the versions of `scripts/licenses-tools.env`
  following `docs/license-and-extensions.md`, then `cargo deny --locked check licenses`.
  The CI job `licenses` also generates the notices with `cargo-about`, all
  platforms and features. Distributions call `scripts/licenses-dist.sh`
  and refuse outdated Python notices or a runtime without its texts.
  An update of `vendor/python/lock.env` refreshes `vendor/python/licenses-{linux,windows}/`
  from the matching full archive, and the numpy notices come from
  the wheel actually packaged. Keep `deny.toml` and `about.toml` in agreement.
  `scripts/licenses-rust.sh` adds the missing upstream copyrights kept
  in `vendor/crate-license-notices/`; a change to `Cargo.lock`
  requires rereading them and refreshing their fingerprints before generation.

## 5. Leave a consistent base for the next session

Reread `git diff --check`, the diff of the changed files and `git status --short`
(new files do not appear in a plain `git diff`). Do not
include personal data, local demo projects or
generated artifacts in a change unrelated to them.

Update the `docs/` page concerned when the behaviour or a contract
changes, and the module comments when the reasoning changes.
Update this guide if the tools or paths change. Avoid copying into it the
volatile details already maintained in the code or the manifests.

In the report, separate the tests run, the failures met
and the checks not done. Do not announce a Python
or visual validation from the default Rust tests alone.

After a merge and push on `main`, delivery includes removing the temporary
worktree and its merged local branch, following
[the cleanup in the verification skill](../.agents/skills/stokesforge-verification/SKILL.md#clean-up-after-a-merge-on-main).
