# The tracking page (#413, #441)

Read from step 6 to 8 of `stokesforge-verification`, and by `stokesforge-feature` at the start and at the trial. One local page, served on this PC by `scripts/suivi.py serve` (`http://127.0.0.1:8441/`, loopback only), two tabs: **En attente** (what the user must do: open questionnaires, trial entries to tick, retours sent, tested and OK) and **Suivi** (the six colours, paused issues, the last ten finished). Source: `scripts/suivi-page.html` (French, exempt from the French guard). The older online page is frozen and links here. Nobody writes the page by hand and no agent writes the state by hand: scripts only.

## Layout of `../_stokesforge/.suivi/` (override: `SF_SUIVI_DIR`)

| Path | Content |
|---|---|
| `issues/<n>.json` | one record per issue or package: colour, last_colour, kind, parent, subs, name, state, wait_kind, waiting, what, since, owner, done_at, legacy |
| `tests/<n>-<k>.json` | one trial entry: title, where, steps, expect, try, commit, date, colour, name, status (`todo`/`ok`/`retour`), comment, reviewed |
| `qcm/<id>/meta.json` | `{issue, title, progress, finished}` of a questionnaire |
| `ended/<session>` | touched by the `SessionEnd` hook |
| `control.json` | result of the last `scripts/suivi.py check` |

## The colour rule

An issue holds its colour while (a) the Claude session that owns it is alive, or (b) one of its questionnaires is open, or (c) one of its trial entries with a `try` field is still `todo`. Otherwise it is **paused**: colour free, worktree kept. On resume it gets its previous colour back if free, else the first free. "Held" is computed when read (a crash pauses by itself). `scripts/suivi.py status [--json]` (or `chantier.sh --status`) prints it; `check` lists inconsistencies and writes `control.json` (the page footer « Vérifié il y a … »). Commands: `claim`, `set --state questions|code|essai|fusion --what … [--wait you|dep|ag --waiting …]`, `pause`, `done`, `release-line`, `check`.

## Trial checklist (#423)

Before asking for `scripts/try.sh <n>`, write the entries of a visible change in one go. The user does not guess what to test.

1. Write a JSON file: one object, or a list, with `title`, `where` (menu or window steps, English labels as on screen), `steps` (2 to 4 short French imperatives from a state the user can reach), `expect` (what is seen, not what the code does), `try` (`scripts/try.sh <n>`). One behaviour per entry: each decided gesture, each edge case (refusal and its message), the routes a bench test cannot see (save and reopen, detached window, `STOKESFORGE_LANG=en-XA` when texts changed).
2. `python3 scripts/suivi.py test add <n> entries.json` creates `tests/<n>-<k>.json`, status `todo`.
3. The message to the user names the page and the number of entries; the terminal does not repeat the list. Every entry ticked OK (or the user's plain OK) is the merge go-ahead. A « Retour » during the trial is fixed on the branch, then the entry goes back to `todo`.
4. At the merge: `python3 scripts/suivi.py test merged <n> <merge commit>` sets `commit` and removes `try`. Any other usable change adds its entry after the merge with `test add` and a `commit` field. `test list [--issue n]` lists them.

A retour **after** the merge becomes a GitLab issue through `stokesforge-bug-report`.

## Reminders (W6)

- When an entry is added, say how many are still unchecked.
- When an unchecked entry touches the area of the next decision form, say so before the form.

## Notifications (W9)

`PushNotification` at three moments: start form ready, trial ready, tracking page updated. The message starts with `<square> #<n>` and carries the local link `http://127.0.0.1:8441/`.

## Server

`scripts/suivi.py serve [--port 8441] [--state DIR]`: single-threaded `http.server` on 127.0.0.1 (exit 1 if the port is taken). `GET /` page, `GET /api/state` (aggregate, ETag), `POST /api/tests/<id>` (status, comment; needs header `X-Suivi: 1`, same-origin `Origin`, own `Host`; ids `[A-Za-z0-9_.-]+` not starting with a dot; body at most 256 KB). `/qcm/<id>` is served by lot C. The user installs it once with `scripts/suivi-install.sh` (user unit `stokesforge-suivi.service`; `scripts/suivi-uninstall.sh` removes it and keeps `.suivi/`). The server keeps the code it started with: after a merge that changes `scripts/suivi.py`, run `systemctl --user restart stokesforge-suivi`. The page file is re-read at each request.

## Migration and `legacy`

`python3 scripts/suivi.py migrate` creates records from the old colour files and `.packages`. Sessions started before the migration have no owner on record, so every migrated issue that still has a worktree (and every package with a colour) gets `legacy: true`: it counts as held until the issue is claimed (`chantier.sh`), paused or done. The page shows it with « état à confirmer par sa session ». `import-online <out_dir>` copies the `tests/*` documents of an `ArtifactData list … out_dir` export of the old online page into `tests/` once (idempotent; a local entry is never overwritten; unsafe ids are rejected).
