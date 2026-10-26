---
name: stokesforge-verification
description: Verify a StokesForge change before delivering it — test in the right place, fmt, clippy, tests, python feature, docs, French commit message. Use on « teste », « vérifie », « c'est bon ? », « commit », « pousse », « j'ai fini », and at the end of any implementation or fix.
---

A green `cargo test --workspace` does not prove what one assumes in this repo. This skill says what it does not prove.

## Steps

1. **Place the tests**: unit, integration, robustness, app logic, painted gesture, témoin, reproduction: [references/test-placement.md](references/test-placement.md). Value bar for any new test: `audit-test`.
2. **Run the base checks** of `CLAUDE.md` § Commandes (`fmt --check`, `clippy -D warnings`, `cargo test --workspace`). Long output goes to a file, only the summary is shown (`CLAUDE.md` § Délégation et choix du modèle); reread the file instead of rerunning.
3. **Run what the change adds**: `python` feature, ignored reproductions, targeted recalculation, licences, screenshots, a real window: [references/checks-by-change.md](references/checks-by-change.md). A task that touches an off-default feature compiles and tests it explicitly, or it validates dead code.
4. **Reread the diff**: `git status --short`, then `git diff`; never carry the user's untracked files or work in progress into a commit. No test weakened, no architecture guard bypassed (`docs/tests.md` § Les garde-fous à ne pas casser), arbitrated limits still in place.
5. **Update the docs the change made wrong**: the module `//!` (the *why*), the `docs/` page (the *what*); routing files (`.claude/rules.md`, `.codex/README.md`, `CLAUDE.md`, `.claude/conventions.md`, `AGENTS.md`) if paths, scripts or boundaries moved; the skills concerned if a new rule was installed, in the same commit.
6. **Commit or push only when asked.** Work on `feature/<n>-<sujet>`, merged into `main` with `--no-ff` (commit « Fusion #N : … (Closes #N) »), never squashed nor rebased once shared. Message in French, as practised in the history: `<domaine> : <verbe au présent> <objet>` (`stokesforge-app : aligne les actions de fenêtre`), `Closes #<n>` for a fixed issue. Push `main` (`git push -u origin main`), retrying up to 4 times on a network error (2 s, 4 s, 8 s, 16 s). No merge request unless asked. CI results of a commit: `scripts/ci-statuts.sh [commit]` (`docs/tests.md` § Utiliser la CI au quotidien).
7. **Report** three things without rounding: what changed, the checks **actually run** (not planned), the validation limits. A failing test is reported with its output; a skipped step is said.

## Clean up after a merge on `main`

After a merge the user asked for and a successful push to `main`, remove the task's temporary worktree **and its local branch** without waiting to be asked; removing a worktree does not delete its branch.

- Check the branch is fully merged: `git merge-base --is-ancestor <branch> main`; inspect its `git status --short` and `git worktree list` first; keep any undelivered work and needed files, ignored ones included.
- From the main checkout on `main`: archive a Codex-managed worktree with `archive_worktree` (identity from `list_artifacts`); remove a clean ordinary worktree with `git worktree remove <path>`; confirm removal before deleting the branch.
- Delete the merged local branch with `git branch -d <branch>`, never `-D` nor a forced removal to get past a failure.
- Confirm neither remains and include the cleanup in the report; if it blocks, give the precise cause and keep what is left to deliver.

Only the finished task's temporary resources: never other work's branches or remote branches without an explicit request. A request to keep the worktree or the branch wins.

Done when the base checks and every check the change requires ran with their real result stated, the diff holds only the task's files, and the docs the change made wrong are updated.
