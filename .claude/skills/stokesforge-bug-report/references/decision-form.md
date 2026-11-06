# Asking the user to decide: the multiple-choice form

Used by every skill that asks questions (feature, bug report, bug fix, commands). The user is not a developer and is dyslexic: they arbitrate **observable behaviour**, never code structure, and decide best one card at a time, in short bullets with a sketch.

## When to ask

Ask, in one start batch, before freezing a classification, a report or code, when:

| Situation | Why |
|---|---|
| the request does not say the behaviour **at the edges** (last column, empty selection, detached window, mixed column) | most round trips hide there |
| doing it **breaks an existing gesture** | the user must know what they lose, before |
| a code comment says "deliberate choice" but the request **differs from the rejected option** | a comment closes one alternative, not all |
| the request has several plausible readings that do not cost the same | price the options, let them choose |
| the scope is fuzzy (« revoir toute la logique de X ») | a fix and a redesign diverge there |
| the symptom cannot be reproduced without their screenshot or file | guessing produces a false report |

State what is already established and give concrete options (cost, risk, what breaks), recommendation first. Never an open question that hands the work back. What has an obvious default is not asked: choose it, say so in one line, move on.

"Faithful to what you chose before" is not an argument for an option, unless a general interface rule (`docs/interface-standard.md`, `docs/menus.md`) imposes the consistency. Challenge earlier answers that lead somewhere doubtful (a one-entry submenu…), and check a menu mockup against the submenu rule (`docs/menus.md` § Rules) before proposing it.

Before offering options on an architecture choice, give the figures the user decides on: memory cost, development cost (how many mechanisms to write), and whether the bugs would be detectable. Each card reads on its own: no bare code (« C2 », « A ») without its question; the user does not remember them. Read the user's answers **before** launching any search agent.

## When: one start batch, one follow-up batch (#413)

- **Start**: every decision known before coding goes in **one** form, posed once `perimetre` has returned (its `changes behaviour` edge cases become cards; `stated by the issue` and `obvious default` are not asked, the default is said in one line). Several issues share one form; each card carries its issue tag (colour square, number, short name). **First card, always: the understanding card**: one line saying what the issue asks, options « yes, that is it » / « no, it is rather… », `p:1`, the first of `D`, not `tech` (the sort is stable by kind then priority, so it stays first). Lots that no answer can change (no answer can alter their files or behaviour) start when the form is posed; the report and `suivi.py set --what` name the running lots.
- **Follow-up**: a question born during coding is not decided silently and not asked at once. It waits for the **single follow-up form**: at the visual trial (`scripts/try.sh <n>`) when the change is visible, before the merge otherwise. Work that does not depend on the answer goes on meanwhile.
- The orchestrator decides the purely technical choices alone only if the user said so for that kind of choice; by default everything is asked, technical cards grouped last.

## How to ask

- **The decisions of a work package** (start form and follow-up form), even a single question: the multiple-choice form, below, **posed locally**: `scripts/suivi.py qcm pose <n> data.js --title "…"` (#441). It appears in the « En attente » tab of the tracking page (`http://127.0.0.1:8441/`) and opens at `/qcm/<id>`.
- **`AskUserQuestion`** only for a quick question outside a work package's decisions (a yes/no on process): one question, 2 to 4 options, recommendation first with « (Recommandé) » ("(Recommended)" in the interaction language), advantage and drawback in each description.
- **The multiple-choice form** is built from [`decision-form.html`](decision-form.html), one template with two storage modes: served by `suivi.py serve` on `127.0.0.1` (answers go to files, « J'ai fini » marks it finished and wakes the agent), or published as an Artifact (answers go to the `decisions` collection). The **published Artifact form is the fallback**: the user is away from the PC (phone, other machine), the tracking server is down, or they ask for it. Never a list of questions in the terminal.

The page is written in the **interaction language** (`CLAUDE.md`, "Interaction language"). The template `decision-form.html` is in French and exempt from the French guard; when the interaction language is not French, translate its wording when filling it. Interface texts appear in English, as on screen.

## Each card of the form, in this order

1. **Question**: short, as a title, with priority (P1 risk of loss or silent trap, P2 visible inconsistency, P3 polish) and code (`R1`, `N4`…).
2. **Contexte**: where and when the user meets the case (window, gesture, concrete example) and the **related decisions** by code. Never assume they remember the screen or an earlier answer.
3. **Aujourd'hui (avant)**: what the software does now, **checked in the code**, with a sketch.
4. **«Les choix (après)»**: recommendation first; each option has "after" bullets, a sketch when drawable, a **+** line and a **−** line — for every option, recommendation included.
5. « Autre / à discuter » and a free comment (built into the template).

Bullets: one idea each, under 12 words when possible, the user's vocabulary (gestures, window names), never a Rust function name unless the user used it.

## Building and publishing the form

1. Write only the data, in a `data.js` in the scratchpad: what goes between `DONNÉES — début` and `DONNÉES — fin` (`TITRE`, `SEC`, `ISSUES`, `INTRO`, `D`). Never copy or edit the template for a local form: `suivi.py` injects the data into it and sets the `<title>`. For the Artifact fallback, copy `decision-form.html` to the scratchpad and replace that block and the `<title>` by hand.
2. A decision: `{c, p, t, l, k, is, ctx:[…], av:{pts:[…], m}, o:[O(name, bullets, sketch, plus, minus), …]}`; the first option of `o` is the recommendation. `k` is the kind: `look`, `gesture`, `syntax`, `other` (default) or `tech`. `is` is the key of the issue in `ISSUES = {key: {n, name, colour}}` (the colour square of `scripts/chantier.sh`); the card shows « 🟦 #413 name » at the top. Order of the cards: look, gesture, syntax and other first (by priority), then the `tech` cards grouped last under « en général tu prends la recommandation », with the existing « take all recommendations » button.
   Start form of a **package** (a parent with sub-issues, #419): one form for the whole package; the first card is the understanding card, the second the list and order of the sub-issues (the user removes, adds or reorders), then the cards grouped by sub-issue, each with `is` pointing to an `ISSUES` entry named `#<parent> › #<sub> <name>` (same colour square for all).
   `INTRO` (HTML, optional) is a block shown above the cards: the issues of the batch with their squares, and a `flow(...)` diagram when a sequence helps (strings are steps, `'→'` an arrow, `[text, 'you'|'ag'|'ok'|'bad']` a coloured step).
3. Sketches use the helpers at the top of the script: `f` (field; `err`, `ok`, `warn`, `off`, `ph`), `b` (button; `pri`, `def`, `dan`, `off`), `tip`, `row`, `lab`, `dim`, `cap`, `pair`, `grid`, `win`, `sbar`, `slider`, `sheet`, `tree` (Project Manager rows: depth, text, state), `slots` (the six work-package colours: rows `[square, title, state, waiting, kind you|dep|ag, since]`, `[square]` alone = free). Add a helper to the template rather than hand-written HTML in the data.
4. **Local (default)**: `python3 scripts/suivi.py qcm pose <issue> data.js --title "…"` runs `node --check` on the page with the data injected, evaluates the data, counts the cards, refuses on an error, creates `../_stokesforge/.suivi/qcm/<issue>-<k>/` and prints the id and the URL. The issue holds its colour while the form is open. `?view=sum` opens the summary directly. In local mode a card also takes screenshots (Ctrl+V or a button; 5 at most, 10 MB each; a drop is handled but the hints do not name it, because a real drag did nothing at the #456 trial, see #483): they are stored with the answer (`images`), not in the data file, and an image on an unanswered card counts as an « Autre » comment. The online Artifact form has no screenshot (it shows one grey line).
5. **Fallback (Artifact)**: `node --check` the extracted script, then publish with `capabilities: {db: {}}`: answers go to the `decisions` collection (code, issue, choix, recommande, commentaire, statut) and stay in the browser if the database is missing.
6. A new batch of decisions is a **new** form (`pose` again, or a new artifact): its answers are the record of that batch.

## After the answers

1. Read them: `python3 scripts/suivi.py qcm answers <id> [--json]` (usable after a `/clear`; `qcm list --open` finds the id). After posing the form, end the turn: « J'ai fini » at the bottom of the summary marks it finished, wakes the agent and frees the issue's colour if nothing else holds it. Do not ask the user to say they finished, and do not read the answers before, unless asked. Wake rules: [suivi.md](../../stokesforge-verification/references/suivi.md). A card's screenshots are listed under its answer as `  image: <absolute path>` (`image_paths` in `--json`); open the file to look at it, and say so when a deleted one reads « removed by the size limit ». Artifact fallback: `ArtifactData`, `action: list`, `collection: decisions`.
2. Cross-check them against each other and against `CLAUDE.md` and `docs/`: look for **inconsistencies**.
3. For each choice that is not the recommendation: state its drawback and propose an adjustment that keeps its intent.
4. An « Autre » that needs context: fetch it in the code, then ask again.
5. Remaining questions: they wait for the single follow-up form (see « When: one start batch, one follow-up batch »), not for a new round per question.
6. Record the decisions in the issues (« **Cohérence** » line) or the docs concerned. No code without an explicit request.
