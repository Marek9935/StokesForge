---
name: stokesforge-bug-report
description: Triage and record StokesForge feedback as GitLab issues — repro, bug or change request, layer, open questions. Use when the user reports « ça plante » (it crashes), « c'est faux » (it's wrong), « bizarre », « il manque… » (…is missing), says « bug report », « ajoute / ouvre une issue » (open an issue), « ticket », « chantier » (work package), « questionnaire », sends a batch of remarks, or creates or fills an issue.
---

A useful report can be replayed, and each line is a located cause or a question. The costly mistake is classifying with confidence what needed a question.

## Keep the reporting context

While the conversation is about reports or issues, stay in that context across messages. An extra screenshot or an imperative (« ajoute », « corrige », « supprime cette icône ») describes a change to record in the issue; it does not authorise touching the app, even for one line. Reading code to locate the problem is fine; changing code, tests or docs is not. Move to `stokesforge-bug-fix` or `stokesforge-feature` only on an explicit request to change the repo (« implémente #152 maintenant », « corrige ce bug dans le code »). Validating an issue's content is not that request.

## Steps

1. **Look first if it is visual** (position, outline, alignment, truncation, refresh). Never conclude from code alone: capture with `run-stokesforge`; when the state is out of its reach (drag, detached OS window, native Wayland), ask the user for a screenshot.
2. **Batch of remarks**: one triage table first (remark in the user's words → `bug` / `évolution` / `à trancher` → lot), grouped by cause or code zone. Expand the full template only for non-obvious bugs. Change requests are restated faithfully, never designed here: only flag what lacks precision.
3. **Rule out false positives** before writing: arbitrated limit, assumed limit, deliberate degradation, and the documented choice that closes only one option. Detail and the "wired vs never wired" search: [references/diagnosis.md](references/diagnosis.md).
4. **Compare against the rule**: a menu against `docs/menus.md` § Rules, a field, message or dialog against `docs/interface-standard.md` § Rule index; name the violated rule (code `S-…`). A section marked *à venir* already has its issue: complete it.
5. **Reduce and locate**: replayable gestures, minimised one at a time, layer found with `.claude/rules.md` § Routing. A disagreement between two surfaces (screen/export, docked/detached, menu/shortcut) points to two paths that should be one.
6. **Ask rather than decide** when the request differs from the rejected option, when bug vs product choice is arguable, when scope is fuzzy, or when a fix breaks a gesture: [`references/decision-form.md`](references/decision-form.md) (always the published form, even for one question, template [references/decision-form.html](references/decision-form.html); `AskUserQuestion` only for a quick process question).
7. **Record in GitLab**, never in a `.md`: template and `glab` commands in [references/diagnosis.md](references/diagnosis.md). Each issue carries a « **Cohérence** » line naming the skills and docs to update. Create issues without asking for confirmation; never start the code. Before flagging a point, check it was not already settled (memory, existing issues): a decided question is not reopened.

Stop at the report and the questions. Done when every remark has a row, every non-obvious bug has a replayable repro and a located layer or « non localisé », and the issues exist in GitLab.
