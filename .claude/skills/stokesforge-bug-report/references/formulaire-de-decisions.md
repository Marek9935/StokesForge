# Asking the user to decide: slides or a multiple-choice form

Used by every skill that asks questions (feature, bug report, bug fix, commands). The user is not a developer and is dyslexic: they arbitrate **observable behaviour**, never code structure, and decide best one card at a time, in short bullets with a sketch.

## When to ask

Ask, in one batch, before freezing a classification, a report or code, when:

| Situation | Why |
|---|---|
| the request does not say the behaviour **at the edges** (last column, empty selection, detached window, mixed column) | most round trips hide there |
| doing it **breaks an existing gesture** | the user must know what they lose, before |
| a code comment says "deliberate choice" but the request **differs from the rejected option** | a comment closes one alternative, not all |
| the request has several plausible readings that do not cost the same | price the options, let them choose |
| the scope is fuzzy (« revoir toute la logique de X ») | a fix and a redesign diverge there |
| the symptom cannot be reproduced without their screenshot or file | guessing produces a false report |

State what is already established and give concrete options (cost, risk, what breaks), recommendation first. Never an open question that hands the work back. What has an obvious default is not asked: choose it, say so in one line, move on.

"Faithful to what you chose before" is not an argument for an option, unless a general interface rule (`docs/standard-interface.md`, `docs/menus.md`) imposes the consistency. Challenge earlier answers that lead somewhere doubtful (a one-entry submenu…), and check a menu mockup against the submenu rule (`docs/menus.md` § Règles) before proposing it.

Before offering options on an architecture choice, give the figures the user decides on: memory cost, development cost (how many mechanisms to write), and whether the bugs would be detectable. Each card reads on its own: no bare code (« C2 », « A ») without its question; the user does not remember them. Read the user's answers **before** launching any search agent.

## How to ask

- **1 to 4 questions: `AskUserQuestion`** (the "small slides"): one question per slide, 2 to 4 options, recommendation first with « (Recommandé) », the advantage and the drawback of each option in its description, a `preview` sketch when a screen or gesture can be drawn.
- **5 questions or more: the multiple-choice form**, a published Artifact page built from [`formulaire-de-decisions.html`](formulaire-de-decisions.html). Never a list of questions in the terminal.

The page itself stays **in French** (the user reads it); interface texts appear in English, as on screen.

## Each card of the form, in this order

1. **Question**: short, as a title, with priority (P1 risk of loss or silent trap, P2 visible inconsistency, P3 polish) and code (`R1`, `N4`…).
2. **Contexte**: where and when the user meets the case (window, gesture, concrete example) and the **related decisions** by code. Never assume they remember the screen or an earlier answer.
3. **Aujourd'hui (avant)**: what the software does now, **checked in the code**, with a sketch.
4. **Les choix (après)**: recommendation first; each option has "after" bullets, a sketch when drawable, a **+** line and a **−** line — for every option, recommendation included.
5. « Autre / à discuter » and a free comment (built into the template).

Bullets: one idea each, under 12 words when possible, the user's vocabulary (gestures, window names), never a Rust function name unless the user used it.

## Building and publishing the form

1. Copy `formulaire-de-decisions.html` to the scratchpad; replace only the block between `DONNÉES — début` and `DONNÉES — fin` (`TITRE`, `SEC`, `D`), and the `<title>`.
2. A decision: `{c, p, t, l, ctx:[…], av:{pts:[…], m}, o:[O(name, bullets, sketch, plus, minus), …]}`; the first option of `o` is the recommendation.
3. Sketches use the helpers at the top of the script: `f` (field; `err`, `ok`, `warn`, `off`, `ph`), `b` (button; `pri`, `def`, `dan`, `off`), `tip`, `row`, `lab`, `dim`, `cap`, `pair`, `grid`, `win`, `sbar`, `slider`, `sheet`. Add a helper to the template rather than hand-written HTML in the data.
4. `node --check` the extracted script, then publish with `capabilities: {db: {}}`: answers go to the `decisions` collection (code, choix, recommande, commentaire, statut) and stay in the browser if the database is missing.
5. A new batch of decisions is a **new** artifact (new file): its answers are the record of that batch.

## After the answers

1. Read them: `ArtifactData`, `action: list`, `collection: decisions`.
2. Cross-check them against each other and against `CLAUDE.md` and `docs/`: look for **inconsistencies**.
3. For each choice that is not the recommendation: state its drawback and propose an adjustment that keeps its intent.
4. An « Autre » that needs context: fetch it in the code, then ask again.
5. Remaining questions: same thresholds (≤ 4 slides, ≥ 5 a new form).
6. Record the decisions in the issues (« **Cohérence** » line) or the docs concerned. No code without an explicit request.
