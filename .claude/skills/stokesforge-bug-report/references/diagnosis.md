# Diagnosis: false positives, search, report template

Shared by `stokesforge-bug-report` (to classify) and `stokesforge-bug-fix` (before fixing). Fixing a false positive is a regression, not a fix.

## Three false positives

- **Arbitrated limit.** Name length, `MAX_NODE_ID`, `MAX_CELLS`, histogram bins, history limits: documented at their constant and listed in `docs/archive/rustrewrite/questions-ouvertes.md` §11. A refusal at the bound is not a bug.
- **Assumed limit.** `stokesforge-app` is still called an integration bench, but its capabilities move fast. Check each limit in `docs/application/README.md` **and in the code**; never reuse a remembered list of missing features.
- **Deliberate degradation.** A grey series annotated "broken reference", an unknown enum variant kept as is, a Typst formula shown as raw text: expected (`docs/invariants.md` § 10. A data defect degrades, it does not crash).

## A documented choice closes one option, not all

A comment saying "this choice is deliberate" is not enough to file the request as a change request. Read *which* alternative was rejected and *why*. When the user asks for a third option the comment never examined, neither "bug" nor "évolution" is right: it is a question for the user.

## Wired and broken vs never wired

Before writing "the mechanism does not exist", search for the intent:

```sh
rg "Action::<Name>|Command::<Name>|fn <name>" crates/stokesforge-app/src/ crates/stokesforge-app-state/src/
```

- An `Action`, `Command` or function already carries the behaviour → the symptom is **a condition that fails**; look for why it does not fire.
- A `Command` exists in `stokesforge-commands` but nothing in the app dispatches it → "add it to the menu" is an `Action` + `dispatch` + a test, not a line of UI.

The two diagnoses send you to opposite places.

## Docked / detached duplicate

A content once had two writings, floating and detached. Workbook, graph, note and matrix are unified; a report that cites only one path of a reintroduced duplicate gets half a fix. Rules: `.claude/conventions.md` § Interface: wiring not to forget (detached window, layer, dialog).

## Wrong or ambiguous error message

The displayed text comes from the catalogue (`crates/stokesforge-app-state/i18n/en/stokesforge_app.ftl`, `error-*` key) and the variant → key mapping in `crates/stokesforge-app-state/src/errors.rs`. Locate the message **by its key**, and say whether the catalogue sentence, the mapping, or the raw `detail` of the source crate is at fault. Chain: `.claude/conventions.md` § Errors: raw English below, catalog sentence in the application.

## Report template (non-obvious bugs only)

The template's field names are French on purpose (issue text language); kept as is.

<!-- lang: fr begin -->
```markdown
## <symptom in one sentence, from the user's point of view>

**Reproduction** (from a new project unless stated)
1. …

**Attendu** : …
**Observé** : …

**Portée** : <crate(s) and verified `path:line` — both writings if duplicated>
**Nature** : résultat faux | panique | blocage | lenteur | incohérence entre surfaces
**Écarté** : <limit / assumed limit / deliberate degradation checked>
**Gravité** : perte de données | bloquant | gênant | cosmétique
**Cohérence** : <skills and docs to update>
```
<!-- lang: fr end -->

Add **Cause probable** only when established, with its `path:line`. « Non localisé » beats an attractive wrong hypothesis; a visual cause stated without looking at the window has no place here. Name an invariant only if one is at stake.

## GitLab, not Markdown

The state of a bug lives only in the project's GitLab issues (`origin` = `gitlab.com:askrzypczak/stokesforge`). A dated `.md` does not know a commit made it false.

- Create: `glab issue create --title "<symptom>" --label bug --description "<template>"` (`--label évolution` for a request). One issue per bug.
- Recap of open bugs: `glab issue list --label bug`. A bug believed open without an issue is checked (`git log --all --grep`, unmerged branches, the user) before being repeated.
- `glab` missing or not authenticated: say so and suggest `! glab auth login`; never fall back silently to a `.md`.

## A test can hold the bug open

A reproduction fails on purpose and carries `#[ignore = "#<issue> : <reason>"]`. When it starts passing, remove `#[ignore]`: it becomes the guard.

| Scope of the bug | Where the test goes |
|---|---|
| local behaviour of a function | `#[cfg(test)]` in the `src/` concerned |
| public contract or crate boundary | `crates/<crate>/tests/` |
| the core at its limits | the `bN` battery of `stokesforge-robustness` |
| interface logic | `crates/stokesforge-app-state/tests/shell.rs` |

A purely visual symptom is not testable in CI: say so rather than write a test that proves nothing.
