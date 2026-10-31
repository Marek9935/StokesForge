# The "To test" page (#413)

Read from step 7 of `stokesforge-verification`, and by `stokesforge-feature` at the start and at the trial. The page lets the user see what is running and what to test, with checkboxes. It is the Artifact https://claude.ai/artifact/UsyyHWZc5cj1enWrRr7yEJ , a page with a database. It lives on claude.ai independently of any session; its source is [to-test-page.html](to-test-page.html) (French, exempt from the French guard). To republish it from any session: `Artifact` publish with `url` set to the address above and this file as `file_path`, `capabilities` omitted so the `db` is kept. Only the orchestrator writes to it, with the `ArtifactData` tool (never the app, never a script). Read before writing: `list` the collection, `set` a new document, `update` an existing one with `if_version`.

## Collections and fields

`chantiers/<n>`: one document per work package in progress.

| Field | Content |
|---|---|
| `n` | issue number (int) |
| `colour` | one of the six squares printed by `scripts/chantier.sh` |
| `name` | short name, in the French the user reads |
| `state` | `questions`, `code`, `essai` (trial) or `fusion` (merge) |
| `waiting` | `false`, `true`, or a short string saying what is awaited |
| `what` | one line |
| `updated` | ISO date |

Written when the work package starts and at each state change; deleted after the merge.

`tests/<n>-<k>`: one document per merge on `main` that changes something usable.

| Field | Content |
|---|---|
| `n`, `colour`, `name` | as above |
| `title` | what is new, one line |
| `where` | array of menu or window steps, English labels as on screen |
| `steps` | array of short imperative steps, in French |
| `expect` | what the user must see |
| `commit`, `date` | merge commit and ISO date |
| `status` | `todo`, `ok` or `retour`; the page writes `ok` / `retour` with `comment` and `reviewed` |
| `comment` | free text the user can write in any state, not only with OK / retour |

A `retour` becomes a GitLab issue through `stokesforge-bug-report`; the entry then gets `issue: <m>`.

## Reminders (W6)

- When an entry is added, say how many are still unchecked.
- When an unchecked entry touches the area of the next decision form, say so before the form.

## Notifications (W9)

`PushNotification` at three moments: start form ready, trial ready, To-test page updated. The message starts with `<square> #<n>`.
