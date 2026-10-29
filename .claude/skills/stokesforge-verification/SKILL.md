---
name: stokesforge-verification
description: Verify a StokesForge change before delivering it — test in the right place, fmt, clippy, tests, python feature, docs, English commit message. Use on « teste » (test), « vérifie » (verify), « c'est bon ? » (is it good?), « commit », « pousse » (push), « j'ai fini » (I'm done), and at the end of any implementation or fix.
---

A green `cargo test --workspace` does not prove what one assumes in this repo. This skill says what it does not prove.

## Steps

1. **Place the tests**: unit, integration, robustness, app logic, painted gesture, witness, reproduction: [references/test-placement.md](references/test-placement.md). Value bar for any new test: `audit-test`.
2. **Run the base checks** of `CLAUDE.md` § Commands (`fmt --check`, `clippy -D warnings`, `cargo test --workspace`, `python3 scripts/french-guard.py`). Long output goes to a file, only the summary is shown (`CLAUDE.md` § Delegation and model choice); reread the file instead of rerunning. In Claude's container a heavy test can fail with `Disk quota exceeded` (`/tmp`): that proves nothing about the code. Push `main` and read the CI (`scripts/ci-status.sh`), or ask the user to run `! cargo test …`.
3. **Run what the change adds**: `python` feature, ignored reproductions, targeted recalculation, licenses, screenshots, a real window: [references/checks-by-change.md](references/checks-by-change.md). A task that touches an off-default feature compiles and tests it explicitly, or it validates dead code.
4. **Reread the diff**: `git status --short`, then `git diff`; never carry the user's untracked files or work in progress into a commit: `git add <path>` one by one, never `-A`, and say so when a changed file does not come from the task. No test weakened, no architecture guard bypassed (`docs/tests.md` § Guards not to break), arbitrated limits still in place.
5. **Update the docs the change made wrong**: the module `//!` (the *why*), the `docs/` page (the *what*); the user manual `manual/src/` and its screenshots if the change is visible; routing files (`.claude/rules.md`, `.codex/README.md`, `CLAUDE.md`, `.claude/conventions.md`, `AGENTS.md`) if paths, scripts or boundaries moved; the skills concerned if a new rule was installed, in the same commit.
6. **Commit or push only when asked.** Work on `feature/<n>-<topic>`, merged into `main` with `--no-ff` (commit "Merge #N: … (Closes #N)"), never squashed nor rebased once shared. Message in English (since #368; the older history is French): `<domain>: <imperative verb> <object>` (`stokesforge-app: align window actions`), `Closes #<n>` for a fixed issue. Push `main` (`git push -u origin main`), retrying up to 4 times on a network error (2 s, 4 s, 8 s, 16 s). No merge request unless asked. CI results of a commit: `scripts/ci-status.sh [commit]` (`docs/tests.md` § Using CI day to day). **Never create or push a `v*` tag** without the user's explicit order: a tag starts the publication.
7. **Close the issues the diff settles**: list them, put `Closes #<n>` in the commit (« Refs » closes nothing) or `glab issue close <n>` with a comment citing the commit, then check with `glab issue view <n>`. Report which issues are closed and which stay open.
8. **Report** three things without rounding: what changed, the checks **actually run** (not planned), the validation limits. A failing test is reported with its output; a skipped step is said.

## Clean up after a merge on `main`

After a merge the user asked for and a successful push to `main`, remove the task's temporary worktree **and its local branch** without waiting to be asked; removing a worktree does not delete its branch.

- Check the branch is fully merged: `git merge-base --is-ancestor <branch> main`; inspect its `git status --short` and `git worktree list` first; keep any undelivered work and needed files, ignored ones included. Untracked or modified files that block the removal are shown to the user, who decides; no `--force`. List the task's open sub-agents and its `backup-*` branches too.
- From the main checkout on `main`: archive a Codex-managed worktree with `archive_worktree` (identity from `list_artifacts`); remove a clean ordinary worktree with `git worktree remove <path>`; confirm removal before deleting the branch.
- Delete the merged local branch with `git branch -d <branch>`, never `-D` nor a forced removal to get past a failure.
- Confirm neither remains and include the cleanup in the report; if it blocks, give the precise cause and keep what is left to deliver.

Only the finished task's temporary resources: never other work's branches or remote branches without an explicit request. A request to keep the worktree or the branch wins.

Done when the base checks and every check the change requires ran with their real result stated, the diff holds only the task's files, and the docs the change made wrong are updated.
