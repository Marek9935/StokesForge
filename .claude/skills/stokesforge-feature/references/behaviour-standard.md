# Sheet: interface behaviour (fields, messages, greying, keyboard)

Read when placing a field, message, dialog, confirmation, greying, keyboard or mouse gesture, displayed number, long task or empty panel. `docs/standard-interface.md` is the single source; start at its rule index, then the gesture's section.

## Where the rule lives

- Rule index: `docs/standard-interface.md` § Index des règles.
- Numeric fields, bounds, steps, wheel: `docs/standard-interface.md` § Champs numériques.
- Refused or clamped input: `docs/standard-interface.md` § Saisie refusée ou ramenée.
- Decimal comma: `docs/standard-interface.md` § Virgule décimale.
- Displayed numbers: `docs/standard-interface.md` § Affichage des nombres.
- Text, in-place editing, Typst: `docs/standard-interface.md` § Champs texte et édition sur place.
- Field help, dimmed text: `docs/standard-interface.md` § Messages et aide.
- Greyed and its reason: `docs/standard-interface.md` § Grisé.
- Keyboard, mouse, cursors: `docs/standard-interface.md` § Clavier et souris ; `docs/gestes.md` § Le modèle.
- Long task, empty panel: `docs/standard-interface.md` § Travaux longs et états vides.
- Building blocks in place / to come: `docs/standard-interface.md` § Briques et gardes.
- A rule that looks wrong: `docs/standard-interface.md` § Une règle qui paraît fausse.
- Overview: `docs/principes-ui.md` § 4. Rien n'est grisé ou refusé en silence.

## Check

- [ ] each field, message or dialog reread against its section
- [ ] a "to come" building block is not called; a "to come" rule is applied with existing means
- [ ] a building block or guard created: switched to "in place" in the page, in the skills, in the same commit
- [ ] number typed by `number_field`, displayed by `number_format`, read by `parse_input`
- [ ] every greyed element says why (`.on_disabled_hover_text`)
- [ ] no `.weak()` on a dialog's main content
