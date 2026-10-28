---
name: stokesforge-ui-texts
description: Write or change any visible StokesForge text by the glossary — menu, button, field, tab, window title, tooltip, status or error message, Fluent key in en and fr. Use before adding, changing or translating any user-facing text (« libellé », « infobulle », « message », « traduis »).
---

Single source: `docs/ui-texts.md` (glossary, rules R1–R16, before/after examples, decisions). This skill says **when** and **in what order**; it never copies the glossary. The rules R1–R16 of the text guide are not the codes `S-R1`… of the behaviour standard.

## When

Any task that adds, changes or removes a text the user sees: menu, context menu, button, tab, field, check box, tooltip, status bar, window title, displayed error, history label, axis, unit or quantity name — and a text StokesForge writes from a file or a column.

## Steps

1. **Find the term** in `docs/ui-texts.md` § 2. Glossary. It exists: use it as is, never a refused word. It is missing: propose it to the user (English, French, one-sentence definition, refused words); never invent a synonym.
2. **Write English first** (R1), then French, applying `docs/ui-texts.md` § 1. Rules: case by element (R3), verb for a command (R4), « Show X » for visibility (R5), unit in brackets (R6), short label and precise tooltip (R7), what happened then what to do (R9), no « (s) » (R10).
3. **Where and when a text appears** (greying reason, dialog footer, clamped input, empty state, confirmation) is fixed by `docs/interface-standard.md` § Rule index; menu wording by `docs/menus.md` § Rules (bare verbs, `Properties…` direct, no icon in a menu).
4. **Write the key** in `crates/stokesforge-app-state/i18n/en/stokesforge_app.ftl`, then the same key with the same arguments in `fr/` in the same commit; use it through `tr!`, never a literal. Mechanics (attributes `.hint` / `.disabled`, plurals, history labels, pseudo-language, guards): [references/fluent-mechanics.md](references/fluent-mechanics.md).
5. **Errors** of the crates below are raw English `Display`; the app phrases them through `errors::<family>::error_message` and `error-*` keys: `.claude/conventions.md` § Errors: raw English below, catalog sentence in the application.
6. **A new rule or term** goes into the glossary in the same commit.
7. **Look at it**: capture with `STOKESFORGE_LANG=en-XA` (`run-stokesforge`) — a text without `[[ ]]` escaped the catalogue, a missing `]]` is truncated.

A crate below `stokesforge-app-state` never holds a sentence for the user (renderer, core): it takes the texts as data (`SeriesLabel`) or from a struct the application supplies (`stokesforge_render::FigureTexts`, built by `figure_texts::figure_texts()`), with a fixed English default for script and CLI exports (#369).

Names chosen by the user or by default (*Book1*, *Sheet1*) are never translated. One notion = one key, shared across zones (`common-auto`, `column-type-*`…), except `history-*` keys and words whose French agreement changes.

Done when every new text has its key in `en` and `fr`, follows the glossary, passes `no_hardcoded_label`, `no_catalog_key_is_dead` and the `fr = en` completeness test, and was looked at in `en-XA` when visible.
