# UI exploration: prototypes on request, sibling issues, gesture feedback

## Prototype variants (only when the user asks)

Triggers (the user's words): « maquettes », « plusieurs versions », « surprends-moi » (or « prototype »). Never on your own initiative, never for a one-field tweak or a bug fix; a new window without such a request still follows E2 (`docs/windows.md`).

1. **Frame** (orchestrator, about 10 lines): the goal, the data to test with (never committed), the fixed constraints (the twin window, `docs/gestures.md` rows, `docs/ui-principles.md`). Free hand (« surprise me ») unless the user fixes the axes of difference.
2. **Variants**: 3 or 4 throwaway branches, one worktree each (`scripts/chantier.sh`, one suffix letter per variant), the variants differing on the axes. A prototype has no persistence, no migration, no manual, no clean tests; it only has to run.
3. **One command to try**: each variant opens in the real app with `scripts/try.sh <n><letter>`, on the user's own screen, with their preferences. Each variant also carries a five-line sheet on the tracking page: idea, what to try, known gaps.
4. **No screenshot bank, no comparison board for the user**: agent screenshots are the agent's own check (`run-stokesforge`); the user always opens the app.
5. **Verdict per variant**: keep, drop, or take a part. Taking a part means its design, not its behaviour, unless the user says otherwise; restate the part in one sentence before coding it. Record what is kept, dropped and the bugs seen in the issue.
6. **Clean rebuild** on the real branch `feature/<n>-<topic>` (commands, texts, tests, manual): a prototype is never merged.
7. **Cleanup**: after the real merge, delete the prototype worktrees and branches (local and remote). Keep one only if the user asks.

## Sibling issues on one mechanism

Open issues that touch one mechanism (example: #503, #504 and #505, decided separately, ended « incohérent » and cost two days of catch-up) are one package:

- one branch;
- one gesture table (rows: gesture × modifier; columns: object or mode; cell: the effect, on the graph and on the page), filed in `docs/gestures.md`;
- one rule per ambiguity;
- one decision form;
- one trial, with one sheet (entry) per issue.

`perimetre` lists the siblings and drafts the table before the start form.

## Feedback checklist for gesture work

Answer each point in the pre-trial sheet:

- a preview is shown while dragging;
- a blocked case says why (`docs/ui-principles.md`, principle 4);
- two states differ by shape, not only by tone;
- the same modifier means the same thing everywhere (the table above);
- text sizes and colours come from the theme;
- a narrow window and `STOKESFORGE_LANG=en-XA` truncate nothing;
- Escape and undo behave as in the twin surface.
