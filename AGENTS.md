# Codex instructions — StokesForge

## Interaction language

Interaction language: **French**. Agents use it to talk to the developer: answers, questions, decision forms. Change this line to work in another language. The repository itself (code, docs, skills, commit messages) stays English whatever this line says.

## Before any implementation

Read [.codex/README.md](.codex/README.md), then follow its reading path
for the domain concerned. **Inspect what exists before writing code**:
documentation, modules, callers, tests, dependencies and available scripts.
Look for a similar implementation and reuse the repository's tools.
Do not introduce a library, a utility or a new layer without
checking what already meets the need.

Start with `git status --short` and examine the relevant differences.
Preserve the user's changes and untracked files.
Before changing anything, state briefly the components to reuse, the area
to change and the planned checks. Then proceed without asking for
confirmation on the usual choices covered by the request.

## Standing landmarks

- While reporting and while creating or enriching issues, the feedback that
  follows completes the issues, even when phrased as « ajoute »,
  « enlève » or « supprime ». Move to code only on an explicit request to
  implement in the repository; see the context rule of the skill
  `stokesforge-bug-report`.
- The repository license is pure GPL-3.0-or-later (#348, exception removed;
  before the first external contribution is merged, decide whether a CLA is needed, #361). Keep the third-party notices and update them with
  dependencies or the runtime; `scripts/licenses-dist.sh` ships them and the
  CI job `licenses` checks the policy with `cargo deny check licenses`.
  The supplements in `vendor/crate-license-notices/` are tied to the Cargo
  lock; a change to `Cargo.lock` requires revalidating them before
  distribution.
- Active development is the Rust workspace in `crates/`. Read
  `Cargo.toml`, the manifests concerned and `rust-toolchain.toml` for the
  effective tools, versions and features.
- `docs/README.md` routes; `docs/invariants.md` describes the boundaries to
  preserve; module header comments explain the decisions.
  The code and tests describe the implemented state. A work package note or
  `docs/archive/` does not prove that a feature exists or remains to be done.
- When a work package installs a new rule (interface standard, pattern to
  follow, boundary), update as needed the skills concerned
  (`.claude/skills/`) and `CLAUDE.md` or `.claude/conventions.md` in the same
  commit as the code.
- The project skills in `.agents/skills/` point to the sources maintained
  in `.claude/skills/`. Consult the relevant skill following the table in
  `.codex/README.md` and its useful references, without loading the whole collection.
  The links also share the reference files and scripts: an update by
  Claude applies directly to Codex, with no copy to synchronize.
- The detailed conventions are shared in `.claude/conventions.md`:
  read the section of the domain before writing code (table in `CLAUDE.md`).
  `.claude/rules.md` specifies the reading scope. The skill `run-stokesforge`
  drives and captures the real application on Linux for interactive checks;
  its X11 path under Xwayland validates neither native Wayland,
  nor `rfd`, nor drag.
- Behavior standard (#217): fields, messages, dialogs, confirmations,
  disabled state (greyed out), keyboard and mouse follow `docs/interface-standard.md`, the single
  source. Section *planned* (#219–#232): the rule already applies to all
  new code, but its building block or guard does not exist yet.
- Menus and right-clicks (#152): the grammar of `docs/menus.md` is the single
  source (sections in canonical order, separators only through
  `MenuSections::section` — guard
  `no_raw_separator_in_a_menu` —, submenu for an exclusive choice or
  ≥ 3 entries, entry absent or disabled with `.disabled`, bare verbs,
  `Properties…` last). The shared painting code draws the content; the
  top menu wraps it, the right-click shows it flat.
- Business mutations go through undoable commands and the session
  actor. Respect the documented exceptions for view state and
  geometry; do not create a parallel business write path.
- `new_project`, like `load`, is an identity replacement by the actor,
  with history cleared and an open macro refused; the confirmation lives
  in the application. The empty graph reuses `Command::AddGraph`.
- The Wayland repository shares the clipboard receiver through the patches
  `vendor/winit` and `vendor/smithay-clipboard` (#72), outside the workspace: read
  `vendor/winit/STOKESFORGE.md` before changing or updating these
  dependencies/eframe. `STOKESFORGE_X11=1` forces the X11 fallback.
- Import from the application, the CLI and the scripts shares the plans and the
  conversion of `stokesforge-importer`; `pyst.import_file` writes through the commands
  of its session, GIL released, in an undoable macro.
- Object copies (#105–106) reuse `NodeCopy` / `InsertCopy`, with no
  allocator and no source project carried over. Every kind is copyable by default;
  IDs are created in the destination and a failure restores the tree. Names are
  unique per folder, canonical paths are rooted; graphs between projects
  carry their columns in a hidden persisted workbook (#107), revealed by Show
  Data. F2 renames in the managers (#130) and edits in the grid (#143), depending on focus; never the name of sheets.
- Collapsed Python, PM and OM panels: only Expand is visible, with no target
  or space reserved for Detach (`panel_header`). Detaching by button
  requires an expanded panel; an OS window keeps Reattach.
- Shared rendering belongs to `stokesforge-render`. Preserve the agreement between
  screen and exports, and the single text measurement.
  The large-curves bench (#141) stays manual (`#[ignore]`, `--release`),
  with no dedicated API; its protocol and measurements live in `docs/rendering.md`.
- Object Manager projects the snapshot in `object_manager.rs`; the selection
  stays in `GraphView`, including in a detached window. Visibility goes
  through `EditPlotSpec` (same delta as `SetPlotSpec`) and thumbnails reuse
  the legend of the shared rendering.
- Follow the local naming conventions. Commit messages and technical docs
  (`docs/`) are in English (since #368; the earlier history stays in French);
  `docs/archive/` stays in French. French elsewhere only in exempt places or behind a
  `lang: fr` marker (`scripts/french-guard.py`, `docs/tests.md`). New code, comments, test names and
  identifiers are written in English from now on, even in a crate not yet
  translated; existing French ones are translated crate by crate (#372–#376). Explain non-obvious
  decisions as close to the code as possible.
- Check the features actually exercised: the default Cargo commands
  do not validate the Python bridge (feature `python`). Typst is compiled and tested
  by default. `scripts/python-test.sh` also covers the application and the CLI,
  and the CI job `test-python` is automatic and blocking. The CLI
  distribution is checked with `scripts/cli-dist.sh`; Matplotlib remains an isolated spike
  (`scripts/python-matplotlib-spike.sh`). The adapted commands are in
  `.codex/README.md`.
- CI: Linux runs on the GitHub mirror, Windows on GitLab. Keep the
  commands of `.gitlab-ci.yml` and `.github/workflows/ci.yml` together; the
  reporter `gitlab-status.yml` / `scripts/gitlab-status.py` sends the Linux
  statuses back to GitLab (GitHub secret `GITLAB_STATUS_TOKEN`, procedure
  in `docs/tests.md`). `STOKESFORGE_CI_LINUX=gitlab` re-enables the
  Linux GitLab jobs; without a Linux runner, leave them disabled.

## Finishing a change

- After a merge and push on `main`, clean up the task's temporary worktree and its merged local branch automatically, unless asked to keep them; follow `stokesforge-verification`, "Clean up after a merge on `main`".
- Worktrees and parallel orchestrators (#413): a task's worktree lives in `../_stokesforge/feature<n>` (a folder beside the main checkout, `<n>` = the issue number), created by `scripts/chantier.sh <n>` from an up-to-date `origin/main` (branch `feature/<n>-<topic>`, no upstream tracking). Never a `StokesForge-<n>` folder beside the checkout, nor the harness folder `.claude/worktrees/`. Several orchestrator sessions may run at once: the session is named `<square> #<n> <short name>`; its colour is the first free of 🟦🟩🟧🟪🟥🟨 at launch, kept until the merge and freed by `git worktree remove`. Merges happen only in the main checkout, under the `main` lock (`scripts/main-lock.sh`); `scripts/try.sh <n>` opens a branch on the user's screen. Rules in `.claude/skills/stokesforge-verification/SKILL.md`, step 6.

Run the checks suited to the change, re-read the diff and update
the documentation concerned. Do not hide a regression by
weakening an architecture test. Report what changed, the checks
actually run and any validation limits.

If the change alters the project's tools, paths, scripts or
boundaries, also update `.codex/README.md` and these instructions
so that the next session starts from exact information.
