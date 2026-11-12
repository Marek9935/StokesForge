# Agent prompt templates (#517)

Two short prompts for the agents an orchestrator starts: the implementer (Sonnet) and the pre-trial agent (Sonnet). Fill the `<...>` fields, keep the rest. A prompt names goal, place, files, witness, bans and a bounded report; the scope from `perimetre` supplies the files and witnesses, so do not paste the issue.

## Bans (put this block in every prompt)

- Work only in the worktree `<path>`; never in the main checkout or another worktree. Never `git stash` (the stack is shared by every worktree): set work aside with a commit or `git diff > file`.
- Never merge, push, tag, deliver, or run `scripts/cargo-queue-install.sh`.
- Run `type -a cargo` first: the first line must be `~/.local/share/stokesforge/cargo-queue/bin/cargo`, else put that directory at the head of `PATH` (a terminal opened before the install silently bypasses the queue).
- Never `sleep`, never an `until` or `while` polling loop, never `pgrep -f` (it matches its own command line and blocks for hours, #471), never `pkill -f`. Run a long command (`cargo test`, `clippy`, `build`) in the background with its log in a file and wait for its notification. To wait for a marker in a log, use `scripts/wait-for.sh [--pid PID] <log> '<regex>' [timeout_s]` with a Bash timeout of 600000 ms (exit 0 match, 1 timeout, 3 the process ended).
- Never close an application launched for the user.
- Anything that needs the real machine (a hand gesture, the user's screen, an install) is a command block for the user, a trial entry, never a simulation or a workaround.
- Repository text (code, docs, commits) in English; `python3 scripts/french-guard.py` must print 0 problems.

## Implementer

```
Implement GitLab issue #<n> in the worktree <path> (branch <branch>). Work only there.
Goal: <one or two sentences, the behaviour after the change>.
Read first: `glab issue view <n>`, CLAUDE.md, <the rules file / skill of the domain>, and the scope below.
Scope (from perimetre): files to touch: <list>. Do not touch: <list>.
Tests with their witness: <case 1: what it proves, the line to break so that it fails>; ... For each test, break the code, see it fail, restore.
Run: <the exact commands, in the background, log in a file>.
Bans: <paste the Bans block>
Commits: subject ends with (#<n>); message ends with the attribution lines of the session.
Report: at most <12> lines: what changed, tests and witnesses (which failed when broken), commits, anything not done. No code unless it is the exact text asked for.
```

## Pre-trial (invisible screen)

```
Walk the detailed checks of #<n> (a pre-trial agent exists only when a gesture or a drawing changes) in the real application on the invisible screen (skill run-stokesforge), in the worktree <path>. Fix what you find on the branch, rerun the tests, then re-verify each fix in the real app.
Checks to walk: <each edge case of the start form, refusals and their messages, STOKESFORGE_LANG=en-XA when texts changed>. Play every sheet of the trial literally, step by step, before `test add`; for a drag or resize play the gesture and its reverse with no modifier, then Shift, Ctrl, Alt.
Capture budget: the screenshots are your own check, never shown to the user. One full frame per checkpoint (`driver.py ss f.png`), then only crops and shrinks: `ss f.png --crop WxH+X+Y --max-width 800`. Do not re-read an unchanged image. Stop the stack (`driver.py stop`) at the end.
Bans: <paste the Bans block>
Report: at most <15> lines: each check with ok / fixed / cannot (and why), the fixes with their commit, the tests rerun, what the user must still try by hand.
```
