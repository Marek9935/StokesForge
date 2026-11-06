# The tracking page (#413, #441)

Read from step 6 to 8 of `stokesforge-verification`, and by `stokesforge-feature` at the start and at the trial. One local page, served on this PC by `scripts/suivi.py serve` (`http://127.0.0.1:8441/`, loopback only), two tabs: **En attente** (what the user must do: open questionnaires, trial entries to tick, retours sent, tested and OK) and **Suivi** (the six colours, paused issues, the last ten finished). Source: `scripts/suivi-page.html` (French, exempt from the French guard). The older online page is frozen and links here. Nobody writes the page by hand and no agent writes the state by hand: scripts only.

## Layout of `../_stokesforge/.suivi/` (override: `SF_SUIVI_DIR`)

| Path | Content |
|---|---|
| `issues/<n>.json` | one record per issue or package: colour, last_colour, kind, parent, subs, name, state, wait_kind, waiting, what, since, owner, done_at, legacy |
| `tests/<n>-<k>.json` | one trial entry: title, intro, where, steps, expect, try, commit, date, colour, name, status (`todo`/`ok`/`retour`), comment, reviewed |
| `qcm/<id>/meta.json` | `{issue, title, created, cards, finished, finished_at}` of a questionnaire (`id` = `<issue>-<k>`) |
| `qcm/<id>/data.js`, `answers/<code>.json` | the decisions (injected into the form template when served) and one record per answered card |
| `removed-tests/` | archive of entries removed by `test remove`; numbers are not reused |
| `ended/<session>` | touched by the `SessionEnd` hook |
| `heard/<root>.json` | `{session, seen}`: the answers already reported to the agent (written only by `suivi.py`) |
| `merge/<root>.json` | `{at, key}`: the user's merge request from the card button; removed by `test merged` |
| `waits/<root>.json`, `waits/log` | the registered wait `{pid, pid_start, session, started}`; SIGTERM/SIGHUP log |
| `control.json` | result of the last `scripts/suivi.py check` |
| `labels.json` | state of the GitLab labels pass (#467): checked_at, ok, reason, synced_at, demote, waiting |
| `tags.json` | tags already seen (the first run seeds), tags pending, and per tag and issue what was done (comment, card id, closed) |
| `tagwait/<n>.json` | `tag-wait` declaration: kind `rc`/`final`, jobs, human text, added_at (set by the service once the label is on GitLab) |
| `ci.json` | last reading of the CI of `main` by the server's watcher (#452): sha, verdict, names, reason, last_red, checked_at |

## The colour rule

An issue holds its colour while (a) the Claude session that owns it is alive, or (b) one of its questionnaires is open, or (c) one of its trial entries with a `try` field is still `todo`. Otherwise it is **paused**: colour free, worktree kept. On resume it gets its previous colour back if free, else the first free. "Held" is computed when read (a crash pauses by itself). `scripts/suivi.py status [--json]` (or `chantier.sh --status`) prints it; `check` lists inconsistencies and writes `control.json` (the page footer « Vérifié il y a … »). Any `set` erases `--wait`/`--waiting` unless it restates them. The card's line is computed: orange « À toi : … » from open forms, entries to tick and a `--wait you` note (joined with « · »), else grey: « L'agent travaille », « Agent à l'arrêt depuis N min » (hours above 60), « Session ouverte » (no activity data) or « En pause » (no line when the colour is held with no live agent, e.g. `legacy`). Use `--wait you --waiting "…"` only for an action just the user can do that the page cannot see (for example a command to run): it turns the card orange and is erased by the next `set`; never for a form or a trial. A note of kind `dep` or `ag` stays muted text. The server reads the agent's activity by `stat` only (never the transcripts): the newest mtime of `~/.claude/projects/*/<session>.jsonl` and `*/<session>/subagents/*.jsonl`, as `agent_idle_min` in whole minutes (seam `SF_CLAUDE_PROJECTS`); 5 minutes or more reads as stopped. Limit: up to 5 minutes of lag, and a long silent command can read as stopped. Commands: `claim`, `set --state questions|code|essai|fusion --what … [--wait you|dep|ag --waiting …]`, `pause`, `done`, `release-line`, `check`.

## Decision forms, local (#441)

The start and follow-up forms are posed here, not published: `python3 scripts/suivi.py qcm pose <n> data.js --title "…"` (checks and counts the cards, creates `qcm/<n>-<k>/`, prints the id and `http://127.0.0.1:8441/qcm/<id>`). The form appears in « En attente » with its progress; the user answers there (each answer is a file), then presses « J'ai fini » at the bottom of the summary, which sets `finished` and frees the colour unless something else holds it. Agents read the answers with `suivi.py qcm answers <id> [--json]` and list forms with `qcm list [--open] [--issue n]`. Template and card rules: `stokesforge-bug-report/references/decision-form.md`. After `qcm pose` the agent ends its turn: it is woken on the answers (§ The agent wakes). The published Artifact form stays as the fallback (user away from the PC, or asked).

## Trial checklist

(#423) Before asking for `scripts/try.sh <n>`, write the entries of a visible change in one go. The user does not guess what to test.

Entries are **guided routes**, at most 5 unmerged per issue (`test add` refuses beyond; `--force`, justified in the report). One entry is one story of 2 to 3 minutes. Group what only the user can judge into routes (look, gesture, feel, what the driver cannot show, see Pre-trial below); the detailed edge-case list (refusal and its message, save and reopen, `STOKESFORGE_LANG=en-XA` when texts changed) stays with the pre-trial agent, not in the entries. Be didactic.

1. Write a JSON file: one object, or a list, with `title` (« Parcours 1/3 (3 min) »), optional `intro` (first sentence: what changes), `where` (menu or window steps, English labels as on screen), `steps` (short French imperatives, each « gesture → what you must see »), `expect` (tick OK if every step showed it), `try` (`scripts/try.sh <n>`).
2. `python3 scripts/suivi.py test add <n> entries.json` creates `tests/<n>-<k>.json`, status `todo`.
3. The message to the user names the page and the number of entries; the terminal does not repeat the list. Ticks alone are not the go-ahead (§ The agent wakes, merge). After a « Retour », fix on the branch and redo the trial: `test remove <n> <id>... [--answered]` (only your own issue or its parent, never a merged entry; an answered entry needs `--answered`, its comment is printed back and named in your report; archived in `removed-tests/`), then `test add`.
4. At the merge, `scripts/deliver.sh` runs `test merged <n> <commit>` (sets `commit`, removes `try`); it refuses a commit that is unknown, not on `main`, without the issue's branch while it exists, or not citing #<n>. A repeat with the same commit is « already recorded »; another commit needs `--replace <old>` (`test merged <n> <new> --replace <old>`). Any other usable change adds its entry after the merge with `test add` and a `commit` field. `test list [--issue n]` lists them.

## The agent wakes (#454)

After `qcm pose` or `test add` the agent launches nothing and ends its turn; it never asks the user to say they finished. A `PostToolUse` hook of `.claude/settings.json` runs `suivi.py hook-wait` after any `suivi.py` call on an issue the session owns: it waits on local files and wakes the idle agent. After a wake the agent reads the answers (`qcm answers`, `test list`) and acts; any later `suivi.py` call on the issue re-arms the wait by itself. Without an active hook, fall back to `suivi.py wait <n>` as a background Bash command. Only the session that owns the issue arms.

- **Wake policy** (`should_wake`, the one place): a Retour, a comment on an OK, a finished form (« J'ai fini »), a merge request from the card button, or no `todo` left. A plain OK with entries left wakes nothing. A Retour or comment waits 180 s after the last change of any entry (`SF_SUIVI_WAIT_SETTLE`); a finished form, a merge request or no `todo` left wakes at once.
- **Merge**: all entries OK does not merge by itself. The agent wakes, checks, announces it is ready. The go-ahead is then the button « Fusionner » on the card (card line « À toi : lancer la fusion ») or the user's word in the terminal. A click writes `merge/<root>.json`; the wait (still armed after all OK until `test merged`) wakes the agent with `#<n> merge requested on the page by the user: this is the go-ahead`, and it delivers with `scripts/deliver.sh` with no further question. Any later change of an entry (Retour, un-tick, comment) voids the request and a re-tick does not bring it back; after a click the card shows « Fusion demandée ». After a merge that changes `suivi.py`, restart the service (`systemctl --user restart stokesforge-suivi`) or the button gets a 404 from the old server.
- **Card**: « L’agent écoute » / « il repart à ta réponse » (waiting), « écris-lui après ta réponse » (agent stopped), « Réponse reçue : lance #n » (paused, unread answer).
- **Safety net**: `hook-news` (SessionStart, UserPromptSubmit) prints unread answers; `chantier.sh` runs `suivi.py news <n>` on a claim. A wait never holds a colour.
- **Limits**: a closed terminal cannot be woken (the answer waits for the next `lance`); a real wait is not proven beyond a few minutes; the hook timeout is 72 h.

## Slowness seen at the trial (#474)

`scripts/try.sh` builds in debug by default (build time), and debug draws far slower than release (#340: about 23 frames/s against 120). When a trial reports a slowness, the agent first asks the user to rerun with `scripts/try.sh --release <n>` and to say how loaded the machine was (other builds, tests or apps running); it diagnoses only what survives in release.

## Pre-trial (#452)

Before `test add`, for a visible change, a Sonnet agent (the pre-trial agent) works in the branch's worktree on the invisible screen (`run-stokesforge`):

1. It walks the detailed checks: each edge case of the start form, refusals and their messages, save and reopen, `STOKESFORGE_LANG=en-XA` when texts changed.
2. It fixes what it finds on the branch, reruns the tests, then **re-verifies the fix in the real app**. Only then the orchestrator writes the entries (`test add`) and asks for `scripts/try.sh <n>`: the user tests only what works.
3. The detailed list and the captures live outside the worktree (`../_stokesforge/<n>-pre-trial.md`): `chantier.sh --release` refuses untracked files. Captures go with the request to the user.
4. The message to the user names what the driver cannot show: native Wayland, `rfd` file chooser, detached OS window, OS drag and drop, multi-screen, scale other than 1, real GPU look, and « is it what I wanted ».

The user's trial stays mandatory for a visible change; the pre-trial replaces none of it, and the orchestrator's own re-verification before the merge is not weakened. The limit of 5 entries is unchanged.

A retour **after** the merge becomes a GitLab issue through `stokesforge-bug-report`.

## Reminders (W6)

- When an entry is added, say how many are still unchecked.
- When an unchecked entry touches the area of the next decision form, say so before the form.

## Notifications (W9)

`PushNotification` at three moments: start form ready, trial ready, tracking page updated. A form or a trial still notifies once; the user answers on the page, not in the terminal. The message starts with `<square> #<n>` and carries the local link `http://127.0.0.1:8441/`.

## Server

`scripts/suivi.py serve [--port 8441] [--state DIR]`: single-threaded `http.server` on 127.0.0.1 (exit 1 if the port is taken). `GET /` page, `GET /api/state` (aggregate, ETag), `POST /api/tests/<id>` (status, comment; needs header `X-Suivi: 1`, same-origin `Origin`, own `Host`; ids `[A-Za-z0-9_.-]+` not starting with a dot; body at most 256 KB). `GET /qcm/<id>` serves the form template with the QCM's `data.js` injected (404 if unknown), `GET /api/qcm/<id>/answers`, `POST /api/qcm/<id>/answer` (one card record) and `POST /api/qcm/<id>/finish` follow the same checks as the tests POST. The user installs it once with `scripts/suivi-install.sh` (user unit `stokesforge-suivi.service`, started with the GNOME session (`graphical-session.target`, like the dictation services) at the lowest CPU and I/O priority, system `/usr/bin/python3`; rerun it after a change of the script; `scripts/suivi-uninstall.sh` removes it and keeps `.suivi/`). The server keeps the code it started with: after a merge that changes `scripts/suivi.py`, run `systemctl --user restart stokesforge-suivi`. The page file is re-read at each request. A change to the CI watcher needs the same restart.

## CI watcher (#452)

`serve` also runs a daemon thread that reads the CI of the newest commit of `main` about every 120 s, so nobody polls it by hand. The line "CI rouge / verte / en cours / inconnue sur main" sits above the tabs on both pages, and `/api/state` carries it as `ci` (`verdict`, `names`, `reason`, `sha`, `last_red`; never `checked_at`, which would defeat the ETag). The page file is the display; the verdict is computed by `scripts/suivi.py`:

- Jobs: `scripts/ci-status.sh --head` (the head of `main` read through `glab`, not the local clone). Linux and Windows jobs all come back as GitLab statuses (`JOBS` of `scripts/gitlab-status.py`, which includes `test-windows`, `test-python-windows`, `dist-windows`). Green only when every job of that list except `robustness` is present and SUCCESS. A FAILED job (Windows too) is red, with its name. No pipeline yet is pending, then unknown after an hour. CANCELED, a `glab` failure, a timeout, empty or unreadable output is unknown. Only a green verdict is final for a SHA; a red head is read again at each poll, since the CI may be re-run on the same commit.
- A red stays in `last_red` (shown while the new head is pending or unknown) until a newer head is green. The watcher sends no desktop notification: the page is its only signal.
- `ci.json` missing, corrupt, or older than 3 polls reads as unknown. The colour rule and the `pending` count do not depend on the CI.
- Seams for tests: `SF_SUIVI_CI=off` (no thread), `SF_SUIVI_CI_POLL`, `SF_SUIVI_CI_TIMEOUT`.

## Build modes (#476)

A row under the CI line sets how builds behave on this PC: « FullIA » (user away, no CPU cap), « Développement » (user at the PC, work keeps moving), « Arrière-plan » (builds really limited), plus a checkbox « Auto » (off by default: battery gives « Arrière-plan », 10 minutes without input gives « FullIA », otherwise the chosen mode). One sentence under the buttons describes the effective mode from the `MODES` table of `scripts/cargo-queue.py`, where all the numbers live; a second line names the running build and how many wait. Without the installed shim the row is one grey line naming `scripts/cargo-queue-install.sh`; a stale installed copy adds a warning to rerun it.

`/api/state` carries `build_queue` (`{"installed": false}` when absent; start timestamps, never durations, so the ETag holds). `POST /api/build-mode` takes `{"mode": "full|dev|background"}` and/or `{"auto": true|false}` behind the same guard as the other writes (400 unknown mode or bad body, 409 not installed) and applies the slice limits at once, also to running builds. A build keeps the job count (`jobs` of the mode) it started with; only weight, memory, quota and affinity change live. `serve` runs a 30 s thread for Auto only on the real state (seam `SF_SUIVI_BUILD_AUTO=on|off`). After a merge that changes `suivi.py`, restart the service to get the control.

## GitLab labels and tag report (#467)

`serve` runs a second thread (about 120 s, state `labels.json`) so the GitLab issue list shows where each issue stands. It runs **only for the real state folder**: with `SF_SUIVI_DIR` or `--state` it stays off unless `SF_SUIVI_LABELS=on` (`off` forces it off), so no test can write to GitLab.

- **`en-cours` / `en-pause`**: same reading as the page (`held_by`): a live owner, an open form or a trial to tick with no agent, or `legacy` is `en-cours`; a record that holds nothing is `en-pause`; done, unstarted or no record is none. A package parent is `en-cours` while one of its sub-issues is held; a paused sub beside a held sibling is `en-pause`. The tracking state wins: a hand edit is undone at the next pass. Held to paused waits 10 minutes (`SF_SUIVI_LABELS_SETTLE`, seconds) so a `/clear` does not flap; promotion, done and closure are immediate. A closed issue loses every label of ours and is not labelled again; reopened, it is.
- **Waiting labels**: `attente-rc` (an `-rcN` or the final tag proves it) and `attente-final` (final tag only). The label alone is the membership, so a hand-placed one works. A waiting label hides `en-pause`; `en-cours` stays if an agent resumes. The service adds one only for a `tag-wait` declaration (once) and removes them at closure.
- **Writes**: one `PUT` per changed issue with `add_labels` / `remove_labels` (never `labels=`, which replaces the user's labels) and, for a closure, `state_event=close`. Reads per pass: the issues carrying each of our four labels, then the record numbers by `iids[]`, all `state=all`. Any failed, timed-out or empty read writes nothing and tries again at the next poll; the page and `/api/state` do not change (ETag).
- **Tags**: the service reads `repository/tags` on GitLab each pass (never `releases`: an `-rcN` release is created then deleted). The first run only records what exists. At a new tag, each open issue with a label that tag handles (an rc tag: `attente-rc`; a final tag: both) gets a comment with its job results and, unless a job failed or lacks proof, a card in « En attente » (a trial entry without `try`: it holds no colour). The card OK closes the issue and drops both labels in one `PUT`; Retour keeps it open. A failed job: comment, label kept, no card. A later tag gives a new comment and card; never two for the same tag and issue, restart included (`tags.json`).
- **Job results** come from the GitHub public API (`actions/runs?event=push&branch=<tag>`, no token, 60 requests per hour, read every 10 minutes while a tag is undecided; seams `SF_SUIVI_TAGJOBS_API`, `SF_SUIVI_TAGJOBS_POLL`, `SF_SUIVI_TAG_CAP`) and `pages` from the GitLab pipeline of the tag. Only the run of the tag's own commit counts. A job missing, still running, skipped or unreadable (403) is no proof; a running job or an unreadable GitHub delays the report (6 h at most). A matrix job (`name (leg)`) is ok only if every leg succeeded; a failed step of a `continue-on-error` job counts as failed. A hand-placed label has no declaration: all release jobs are reported and the card says « relis l'issue ».
- **Declaring**: `suivi.py tag-wait <n> rc|final [--jobs a,b] [--human "…"]` (or `deliver.sh --wait-tag`), then the label appears within about 2 minutes.
- **By hand**: `suivi.py labels sync [--dry-run]` runs one label pass (`--dry-run` prints the `PUT`s and writes nothing); `suivi.py tag-check <tag> --dry-run` reads GitLab and GitHub and prints what the service would report for that tag, writing nothing: run it before a tag. `suivi.py status` ends with the last sync (time, ok or failed with the reason) and the issues that wait.
- **Restart rule**: the service is a user unit; after changing `scripts/suivi.py` restart it (`systemctl --user restart stokesforge-suivi.service`, see `scripts/suivi-install.sh`), else the old code keeps running.

## Migration and `legacy`

`python3 scripts/suivi.py migrate` creates records from the old colour files and `.packages`. Sessions started before the migration have no owner on record, so every migrated issue that still has a worktree (and every package with a colour) gets `legacy: true`: it counts as held until the issue is claimed (`chantier.sh`), paused or done. The page shows it with « état à confirmer par sa session ». `import-online <out_dir>` copies the `tests/*` documents of an `ArtifactData list … out_dir` export of the old online page into `tests/` once (idempotent; a local entry is never overwritten; unsafe ids are rejected).
