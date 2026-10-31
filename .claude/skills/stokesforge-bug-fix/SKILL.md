---
name: stokesforge-bug-fix
description: Fix a StokesForge bug — failing reproduction first, repair at the layer of the cause, regression test, issue closed. Use on an explicit request to fix in the code (« corrige ce bug dans le code » / "fix this bug in the code", « répare #12 », « traite #12 », « lance #12 » or a bug named in plain words, identified as in the skill `stokesforge-feature`, section 1), or to lift a reproduction's #[ignore].
---

Reproduce, then fix. A fix written before the reproduction does not know what it fixes.

**Context gate.** An issue just created, or « corrige » said while collecting feedback, does not start this skill: follow `stokesforge-bug-report` and record it. Change the repo only on an explicit request to fix in the code.

1. **Diagnose.** Not yet triaged → `stokesforge-bug-report`. Rule out the false positives and settle "wired and broken vs never wired" with an `rg` before coding: [`../stokesforge-bug-report/references/diagnosis.md`](../stokesforge-bug-report/references/diagnosis.md).
2. **Reproduce.** Write a test that **fails for the right reason** and watch it fail. An existing `#[ignore]` reproduction is the starting point (`cargo test -p stokesforge-robustness -- --ignored`). A visual bug is reproduced by **looking** (`run-stokesforge`); keep the before screenshot.
3. **Find the layer of the cause**, not of the symptom: read the domain section of `.claude/conventions.md`, then [references/symptom-to-cause.md](references/symptom-to-cause.md). Scope and delegation: `CLAUDE.md` § Delegation and model choice. The fix is written on `feature/<n>-<topic>` in the worktree `../_stokesforge/feature<n>`, like a feature (`stokesforge-feature` § 1).
4. **Ask** when the fix breaks an existing gesture or the right behaviour is debatable — a product decision, phrased as observable behaviour: [`../stokesforge-bug-report/references/decision-form.md`](../stokesforge-bug-report/references/decision-form.md).
5. **Fix** within the rules: `AppState::dispatch` never returns an error (it becomes a status message); a wrong message is fixed in the `en` catalogue, not in the source crate's `Display`; a data defect degrades, it does not panic (`docs/invariants.md` § 10. A data defect degrades, it does not crash).
6. **Never weaken the net.** An architecture guard that fails means the fix crossed a boundary: redo the fix, not the test (`docs/tests.md` § Guards not to break). Read the witnesses of `measurement.rs` / `agreement.rs` before touching them.
7. **Lock it.** The reproduction becomes the guard (remove `#[ignore]`); the regression test sits at the cause's level; a disagreement between two surfaces gets a test that compares both.
8. **Close it where it is tracked**: commit message with `Closes #<n>`, or `glab issue close <n>` with a comment citing the commit when the push is not on `main`. No issue: create no `.md`.
9. **Look before saying « fixed »**: a display or gesture defect is not closed without a capture read after the fix (`run-stokesforge`). What the tool cannot show (native Wayland, drag, `rfd`): write « non vérifié à l'écran » and ask the user for a screenshot.
10. **Verify and report** with `stokesforge-verification`; update the module `//!` ("the previous hypothesis was wrong because…") and the `docs/` page if described behaviour changed.

Done when the reproduction was seen failing then passing, no test was weakened, the issue is closed or its closing is in the commit message, and the report states what was run and what remains unvalidated (native Wayland, `rfd`, drag).
