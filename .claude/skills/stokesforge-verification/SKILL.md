---
name: stokesforge-verification
description: Verify a StokesForge change before delivering it — test in the right place, fmt, clippy, tests, python feature, docs, English commit message. Use on « teste » (test), « vérifie » (verify), « c'est bon ? » (is it good?), « commit », « pousse » (push), « j'ai fini » (I'm done), and at the end of any implementation or fix.
---

A green `cargo test --workspace` does not prove what one assumes in this repo. This skill says what it does not prove.

## Steps

1. **Place the tests**: unit, integration, robustness, app logic, painted gesture, witness, reproduction: [references/test-placement.md](references/test-placement.md). Value bar for any new test: `audit-test`.
2. **Run the base checks** of `CLAUDE.md` § Commands (`fmt --check`, `clippy -D warnings`, `cargo test --workspace`, `python3 scripts/french-guard.py`). Long output goes to a file, only the summary is shown (`CLAUDE.md` § Delegation and model choice); reread the file instead of rerunning. In Claude's container a heavy test can fail with `Disk quota exceeded` (`/tmp`): that proves nothing about the code. Push `main` and read the CI (`scripts/ci-status.sh`; after **every** push, step 6), or ask the user to run `! cargo test …`.
3. **Run what the change adds**: `python` feature, ignored reproductions, targeted recalculation, licenses, screenshots, a real window: [references/checks-by-change.md](references/checks-by-change.md). A task that touches an off-default feature compiles and tests it explicitly, or it validates dead code.
4. **Reread the diff**: `git status --short`, then `git diff`; never carry the user's untracked files or work in progress into a commit: `git add <path>` one by one, never `-A`, and say so when a changed file does not come from the task. No test weakened, no architecture guard bypassed (`docs/tests.md` § Guards not to break), arbitrated limits still in place.
5. **Update the docs the change made wrong**: the module `//!` (the *why*), the `docs/` page (the *what*); the user manual `manual/src/` and its screenshots if the change is visible; routing files (`.claude/rules.md`, `.codex/README.md`, `CLAUDE.md`, `.claude/conventions.md`, `AGENTS.md`) if paths, scripts or boundaries moved; the skills concerned if a new rule was installed, in the same commit.
6. **Commit on the branch freely; merge and push by the rule below.** Work on `feature/<n>-<topic>`, merged into `main` with `--no-ff` (commit "Merge #N: … (Closes #N)"), never squashed nor rebased once shared. Message in English (since #368; the older history is French): `<domain>: <imperative verb> <object>` (`stokesforge-app: align window actions`), `Closes #<n>` for a fixed issue. Push `main` (`git push -u origin main`), retrying up to 4 times on a network error (2 s, 4 s, 8 s, 16 s). No merge request unless asked. CI results of a commit: `scripts/ci-status.sh [commit]` (`docs/tests.md` § Using CI day to day). **Never create or push a `v*` tag** without the user's explicit order: a tag starts the publication.
   - **Merge rule (#413).** Green and **not visible** (no screen, gesture or text the user would see): merge and push `main` without asking. **Visible**: the user's OK on the trial (`scripts/try.sh <n>`) is the go-ahead, no separate question.
   - **Sequence, one orchestrator at a time on `main`**, from the main checkout (`git status --short` clean first, else stop): `scripts/main-lock.sh take "<square> #<n>"`, merge `main` into the branch (once, just before delivering) and recheck, `git merge --no-ff`, push, `scripts/main-lock.sh release`. A held lock is waited for or reported, never broken.
   - **Read the CI after every push.** A newer push cancels the running series (`ci.yml` concurrency): a cancelled run is superseded, read the CI of the newest `main` that contains your commit (`git merge-base --is-ancestor`). A red `main` is reported and fixed first, whoever broke it.
7. **Close the issues the diff settles**: list them, put `Closes #<n>` in the commit (« Refs » closes nothing) or `glab issue close <n>` with a comment citing the commit, then check with `glab issue view <n>`. Report which issues are closed and which stay open.
8. **Record what landed**: add the entry of the merge to the To-test page, delete the `chantiers/<n>` document (a package: one `tests/` entry per merged sub-issue, and the `chantiers/<parent>` document only after the last), notify the phone: [references/to-test-page.md](references/to-test-page.md) (page source to republish: [references/to-test-page.html](references/to-test-page.html)).
9. **Report** three things without rounding: what changed, the checks **actually run** (not planned), the validation limits. A failing test is reported with its output; a skipped step is said.

## Clean up after a merge on `main`

After a merge on `main` and a successful push to `main`, remove the task's temporary worktree **and its local branch** without waiting to be asked; removing a worktree does not delete its branch.

- Check the branch is fully merged: `git merge-base --is-ancestor <branch> main`; inspect its `git status --short` and `git worktree list` first; keep any undelivered work and needed files, ignored ones included. Untracked or modified files that block the removal are shown to the user, who decides; no `--force`. List the task's open sub-agents and its `backup-*` branches too.
- From the main checkout on `main`: archive a Codex-managed worktree with `archive_worktree` (identity from `list_artifacts`); remove a clean ordinary worktree with `git worktree remove ../_stokesforge/feature<n>` (the `_stokesforge` folder itself stays); confirm removal before deleting the branch.
- Delete the merged local branch with `git branch -d <branch>`, never `-D` nor a forced removal to get past a failure.
- After the last sub-issue of a package has merged and its worktree is removed (or when the parent closes), run `scripts/chantier.sh --done <parent>`: it frees the package colour, and refuses while a worktree of the package still exists (#419).
- Confirm neither remains and include the cleanup in the report; if it blocks, give the precise cause and keep what is left to deliver.

Only the finished task's temporary resources: never other work's branches or remote branches without an explicit request. A request to keep the worktree or the branch wins.

Done when the base checks and every check the change requires ran with their real result stated, the diff holds only the task's files, and the docs the change made wrong are updated.
