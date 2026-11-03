# The "To test" page (#413)

Read from step 7 of `stokesforge-verification`, and by `stokesforge-feature` at the start and at the trial. The page lets the user see what is running and what to test, with checkboxes. It is the Artifact https://claude.ai/artifact/UsyyHWZc5cj1enWrRr7yEJ , a page with a database. It lives on claude.ai independently of any session; its source is [to-test-page.html](to-test-page.html) (French, exempt from the French guard). To republish it from any session: `Artifact` publish with `url` set to the address above and this file as `file_path`, `capabilities` omitted so the `db` is kept. Only the orchestrator writes to it, with the `ArtifactData` tool (never the app, never a script). Read before writing: `list` the collection, `set` a new document, `update` an existing one with `if_version`.

## Collections and fields

`chantiers/<n>`: one document per work package in progress.

| Field | Content |
|---|---|
| `n` | issue number (int) |
| `colour` | one of the six squares printed by `scripts/chantier.sh`; absent while the package is paused |
| `name` | short name, in the French the user reads |
| `state` | `questions`, `code`, `essai` (trial), `fusion` (merge) or `pause` (a package between two sub-issues, no colour) |
| `waiting` | `false`, `true`, or a short string saying what is awaited |
| `wait_kind` | who is awaited: `you` (the user: « T'attend »), `dep` (another work package or step: « Attend »), `ag` (an agent is working). Absent: `waiting` set reads as `you`, `waiting: false` shows nothing |
| `what` | one line |
| `since` | ISO date from which the colour is held (or the package paused): the `since` of `scripts/chantier.sh --status --json` |
| `updated` | ISO date |

Written when the work package starts and at each state change; deleted after the merge. The page shows six rows, one per colour, always in the order 🟦🟩🟧🟪🟥🟨: the document holding the colour, or « libre ». Below them: the paused packages and the folders without a colour.

`controle/dernier`: one document, the last colour check.

| Field | Content |
|---|---|
| `at` | ISO datetime of the check |
| `by` | tag of the session that ran it (`🟦 #441`) |
| `colourless` | array of the worktree folders without a colour (`colourless` of `--status --json`) |

## Colour check

(#441) The page must say the truth about who holds which colour. A session runs the check:

- **when**: at each `lance` (step 2 of the briefing), at each state change of its work package, and at each cleanup (`scripts/chantier.sh --release`, `--pause`, `--done`).
- **how**: run `scripts/chantier.sh --status --json`, `list` the `chantiers` collection, and fix **all** the differences, not only its own work package:
  - a colour held in the JSON with no document: create a placeholder (`state: code` since the real state is unknown, `what`: « état à confirmer par sa session »);
  - a document whose colour is free in the JSON, and which is not a paused package: delete it;
  - a package in the `paused` list: its document gets `state: pause` and no `colour`;
  - `since` of every document comes from the JSON.
- **last**: write `controle/dernier` (`at` now, `by` its tag, `colourless` from the JSON). The page footer shows it.

A package with sub-issues (#419): one `chantiers/<parent>` document for the whole package (`n` = the parent, one colour), whose `what` names the sub-issue in progress (`#219 Rejected fields: code`); it is deleted after the last sub-issue merges. The `name` is the parent's short name.

`tests/<n>-<k>`: one document per small observable check. A visible change writes them **at the trial, before the merge** (§ Trial checklist); any other usable change writes one at the merge.

| Field | Content |
|---|---|
| `n`, `colour`, `name` | as above; for a sub-issue of a package, `n` = the sub-issue number and `name` starts with `#<parent> ›` (`#217 › Rejected fields`) |
| `title` | what is new, one line |
| `where` | array of menu or window steps, English labels as on screen |
| `steps` | array of short imperative steps, in French |
| `expect` | what the user must see |
| `commit`, `date` | branch head at the trial, then the merge commit; ISO date |
| `try` | only before the merge: the command that opens the branch (`scripts/try.sh <n>`); removed at the merge (`{"__delete__": true}`) |
| `status` | `todo`, `ok` or `retour`; the page writes `ok` / `retour` with `comment` and `reviewed` |
| `comment` | free text the user can write in any state, not only with OK / retour |

A `retour` **after** the merge becomes a GitLab issue through `stokesforge-bug-report`; the entry then gets `issue: <m>`. A `retour` **during the trial** is fixed on the branch, then the entry goes back to `todo`.

## Trial checklist

(#423) The user does not guess what to test: before asking for `scripts/try.sh <n>`, write one `tests/<n>-<k>` entry per small check, in one `batch`.

- One behaviour per entry: each decided gesture, each edge case (refusal and its message), and the routes a bench test cannot see (save and reopen, detached window, `STOKESFORGE_LANG=en-XA` when texts changed).
- `steps`: 2 to 4 short French imperatives, from a state the user can reach (the demo project, « crée un dossier »), with the English labels as on screen; `expect`: what is seen, not what the code does.
- `try`: the command to run; `status: todo`.
- Then the message to the user names the page and the number of entries; the terminal does not repeat the list.
- Every entry OK (or the user's plain OK) is the merge go-ahead; at the merge, `commit` becomes the merge commit and `try` is removed.

## Reminders (W6)

- When an entry is added, say how many are still unchecked.
- When an unchecked entry touches the area of the next decision form, say so before the form.

## Notifications (W9)

`PushNotification` at three moments: start form ready, trial ready, To-test page updated. The message starts with `<square> #<n>`.
