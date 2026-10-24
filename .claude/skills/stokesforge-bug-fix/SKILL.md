---
name: stokesforge-bug-fix
description: Fix a StokesForge bug — failing reproduction first, repair at the layer of the cause, regression test, issue closed. Use on an explicit request to fix in the code (« corrige ce bug dans le code », « répare #12 »), or to lift a reproduction's #[ignore].
---

Reproduce, then fix. A fix written before the reproduction does not know what it fixes.

**Context gate.** An issue just created, or « corrige » said while collecting feedback, does not start this skill: follow `stokesforge-bug-report` and record it. Change the repo only on an explicit request to fix in the code.

1. **Diagnose.** Not yet triaged → `stokesforge-bug-report`. Rule out the false positives and settle "wired and broken vs never wired" with an `rg` before coding: [`../stokesforge-bug-report/references/diagnosis.md`](../stokesforge-bug-report/references/diagnosis.md).
2. **Reproduce.** Write a test that **fails for the right reason** and watch it fail. An existing `#[ignore]` reproduction is the starting point (`cargo test -p stokesforge-robustesse -- --ignored`). A visual bug is reproduced by **looking** (`run-stokesforge`); keep the before screenshot.
3. **Find the layer of the cause**, not of the symptom: read the domain section of `.claude/conventions.md`, then [references/symptom-to-cause.md](references/symptom-to-cause.md). Scope and delegation: `CLAUDE.md` § Délégation et choix du modèle.
4. **Ask** when the fix breaks an existing gesture or the right behaviour is debatable — a product decision, phrased as observable behaviour: [`../stokesforge-bug-report/references/formulaire-de-decisions.md`](../stokesforge-bug-report/references/formulaire-de-decisions.md).
5. **Fix** within the rules: `AppState::dispatch` never returns an error (it becomes a status message); a wrong message is fixed in the `en` catalogue, not in the source crate's `Display`; a data defect degrades, it does not panic (`docs/invariants.md` § 10. Un défaut de donnée dégrade, il ne plante pas).
6. **Never weaken the net.** An architecture guard that fails means the fix crossed a boundary: redo the fix, not the test (`docs/tests.md` § Les garde-fous à ne pas casser). Read the témoins of `mesure.rs` / `accord.rs` before touching them.
7. **Lock it.** The reproduction becomes the guard (remove `#[ignore]`); the regression test sits at the cause's level; a disagreement between two surfaces gets a test that compares both.
8. **Close it where it is tracked**: commit message with `Closes #<n>`, or `glab issue close <n>` with a comment citing the commit when the push is not on `main`. No issue: create no `.md`.
9. **Verify and report** with `stokesforge-verification`; update the module `//!` ("the previous hypothesis was wrong because…") and the `docs/` page if described behaviour changed.

Done when the reproduction was seen failing then passing, no test was weakened, the issue is closed or its closing is in the commit message, and the report states what was run and what remains unvalidated (native Wayland, `rfd`, drag).
