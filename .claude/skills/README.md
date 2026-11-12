# Project skills

Each folder holds a `SKILL.md`: its `description` is the trigger, its body the steps, its `references/` the detail read on demand. Skills use the docs, they never copy them: `docs/README.md` routes, `.claude/rules.md` says what to load, `.claude/rules/` holds the rules by domain (path-scoped files).

| Skill | For | Triggers on |
|---|---|---|
| [`stokesforge-feature`](stokesforge-feature/SKILL.md) | implement a feature end to end | a change to make in the repo; « lance #N » or a work package named in plain words |
| [`stokesforge-bug-report`](stokesforge-bug-report/SKILL.md) | triage feedback into GitLab issues | « ça plante », a batch of remarks, an issue |
| [`stokesforge-bug-fix`](stokesforge-bug-fix/SKILL.md) | fix: reproduce first, repair at the cause | an explicit request to fix in the code |
| [`stokesforge-command`](stokesforge-command/SKILL.md) | add an undoable command | any mutation of the project |
| [`stokesforge-rendering`](stokesforge-rendering/SKILL.md) | plots, axes, export, Typst | `stokesforge-render*`, a visual symptom |
| [`stokesforge-import`](stokesforge-import/SKILL.md) | instrument files, recipes, preview | a badly read CSV/TXT/ASC |
| [`stokesforge-python`](stokesforge-python/SKILL.md) | PyO3 bridge, `pyst`, console | a script, the `python` feature |
| [`stokesforge-ui-texts`](stokesforge-ui-texts/SKILL.md) | visible texts by the glossary | any label, tooltip or message |
| [`stokesforge-verification`](stokesforge-verification/SKILL.md) | test placement, checks, commit | « vérifie », « commit », end of a task |
| [`run-stokesforge`](run-stokesforge/SKILL.md) | launch, drive and capture the real app on an invisible screen | « lance l'app », « screenshot » |
| [`stokesforge-explain`](stokesforge-explain/SKILL.md) | answer a chemist: features and architecture | « est-ce que je peux… », « comment ça marche » |

## How they chain

```text
question ──► stokesforge-explain
symptom  ──► stokesforge-bug-report ──(explicit request)──► stokesforge-bug-fix ──┐
request  ──► stokesforge-feature ──► command / rendering / import / python / ui-texts ──┴─► stokesforge-verification ──► pre-trial if a gesture or drawing changes ──► merge (green, or the user's word after a trial all OK)
```

`run-stokesforge` sits beside verification for any change to see or handle in the real app; its screen is invisible, so agents run in parallel, and a change of gesture or drawing gets a pre-trial by an agent in the real app before the user's trial (`stokesforge-verification/references/suivi.md` § Pre-trial). Several orchestrators may work at once: `scripts/chantier.sh <n>…`, run by the orchestrator in the current session, creates or finds the worktrees, assigns the colours and prints the briefing, `scripts/try.sh <n>` opens a branch on the user's screen for the trial, the local tracking page (`http://127.0.0.1:8441/`, `scripts/suivi.py serve`) lists what to answer and test (the agent wakes on the answers, stores the merge message, and the page's « Fusionner » button runs the merge itself (or the user's word in the terminal)), who holds which colour, and carries the one CI watcher (nobody polls the CI) (`stokesforge-verification/references/suivi.md`). Asking the user to decide (one start form, one follow-up form, always the form even for one question, posed locally with `scripts/suivi.py qcm pose`, the published Artifact as fallback): [`stokesforge-bug-report/references/decision-form.md`](stokesforge-bug-report/references/decision-form.md). Delegation (agents `perimetre` and `recherche` in `.claude/agents/`): `CLAUDE.md`, "Delegation and model choice".

## Keeping them true

- A change that installs a new rule updates the skills concerned in the same commit (`CLAUDE.md`, « Conventions »); a skill that notes a deviation says it must be fixed, never that it exists.
- A user-visible feature updates the manual `manual/src/` in the same commit (`CLAUDE.md`, « Conventions »); `stokesforge-feature` and `stokesforge-verification` carry the step.
- A skill cites a path where the code lives now. References such as "`x.md` § Title" are checked by `crates/stokesforge-arch-guard/tests/skill_references.rs`, which also refuses an unlinked `references/` file and a description a YAML parser would cut (unquoted ` #` or `: `), longer than 1024 characters or holding a tag.
- A new skill earns its place when a domain has traps that raise no error; otherwise `.claude/rules.md` is enough. Format and description rules: `skill-manager` (50–1024 characters, precise trigger terms, one skill per trigger).
