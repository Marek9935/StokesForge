# Project skills

Each folder holds a `SKILL.md`: its `description` is the trigger, its body the steps, its `references/` the detail read on demand. Codex reads the same folders through the links of `.agents/skills/`; `.claude/skills/` is the source. Skills use the docs, they never copy them: `docs/README.md` routes, `.claude/rules.md` says what to load, `.claude/conventions.md` holds the rules by domain.

| Skill | For | Triggers on |
|---|---|---|
| [`stokesforge-feature`](stokesforge-feature/SKILL.md) | implement a feature end to end | a change to make in the repo |
| [`stokesforge-bug-report`](stokesforge-bug-report/SKILL.md) | triage feedback into GitLab issues | « ça plante », a batch of remarks, an issue |
| [`stokesforge-bug-fix`](stokesforge-bug-fix/SKILL.md) | fix: reproduce first, repair at the cause | an explicit request to fix in the code |
| [`stokesforge-commande`](stokesforge-commande/SKILL.md) | add an undoable command | any mutation of the project |
| [`stokesforge-rendu`](stokesforge-rendu/SKILL.md) | plots, axes, export, Typst | `stokesforge-render*`, a visual symptom |
| [`stokesforge-import`](stokesforge-import/SKILL.md) | instrument files, recipes, preview | a badly read CSV/TXT/ASC |
| [`stokesforge-python`](stokesforge-python/SKILL.md) | PyO3 bridge, `pyst`, console | a script, the `python` feature |
| [`stokesforge-textes-interface`](stokesforge-textes-interface/SKILL.md) | visible texts by the glossary | any label, tooltip or message |
| [`stokesforge-verification`](stokesforge-verification/SKILL.md) | test placement, checks, commit | « vérifie », « commit », end of a task |
| [`run-stokesforge`](run-stokesforge/SKILL.md) | launch, drive and capture the real app | « lance l'app », « screenshot » |
| [`stokesforge-expliquer`](stokesforge-expliquer/SKILL.md) | answer a chemist: features and architecture | « est-ce que je peux… », « comment ça marche » |

## How they chain

```text
question ──► stokesforge-expliquer
symptom  ──► stokesforge-bug-report ──(explicit request)──► stokesforge-bug-fix ──┐
request  ──► stokesforge-feature ──► commande / rendu / import / python / textes ──┴─► stokesforge-verification ──► commit (on request)
```

`run-stokesforge` sits beside verification for any change to see or handle in the real app. Asking the user to decide (≤ 4 questions: slides; ≥ 5: a published form): [`stokesforge-bug-report/references/formulaire-de-decisions.md`](stokesforge-bug-report/references/formulaire-de-decisions.md). Delegation (agents `perimetre` and `recherche` in `.claude/agents/`): `CLAUDE.md`, "Delegation and model choice".

## Keeping them true

- A change that installs a new rule updates the skills concerned in the same commit (`CLAUDE.md`, « Conventions »); a skill that notes a deviation says it must be fixed, never that it exists.
- A skill cites a path where the code lives now. References such as "`x.md` § Title" are checked by `crates/stokesforge-arch-guard/tests/skill_references.rs`, which also refuses an unlinked `references/` file.
- A new skill earns its place when a domain has traps that raise no error; otherwise `.claude/rules.md` is enough. Format and description rules: `skill-manager`.
