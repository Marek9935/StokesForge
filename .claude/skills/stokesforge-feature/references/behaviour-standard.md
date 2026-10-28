# Sheet: interface behaviour (fields, messages, greying, keyboard)

Read when placing a field, message, dialog, confirmation, greying, keyboard or mouse gesture, displayed number, long task or empty panel. `docs/standard-interface.md` is the single source; start at its rule index, then the gesture's section.

## Where the rule lives

- Rule index: `docs/standard-interface.md` § Rule index.
- Numeric fields, bounds, steps, wheel: `docs/standard-interface.md` § Numeric fields.
- Refused or clamped input: `docs/standard-interface.md` § Input refused or brought back.
- Decimal comma: `docs/standard-interface.md` § Decimal comma.
- Displayed numbers: `docs/standard-interface.md` § Number display.
- Text, in-place editing, Typst: `docs/standard-interface.md` § Text fields and in-place editing.
- Field help, dimmed text: `docs/standard-interface.md` § Messages and help.
- Greyed and its reason: `docs/standard-interface.md` § Disabled.
- Keyboard, mouse, cursors: `docs/standard-interface.md` § Keyboard and mouse ; `docs/gestes.md` § The model.
- Long task, empty panel: `docs/standard-interface.md` § Long tasks and empty states.
- Building blocks in place / to come: `docs/standard-interface.md` § Building blocks and guards.
- A rule that looks wrong: `docs/standard-interface.md` § A rule that looks wrong.
- Overview: `docs/principes-ui.md` § 4. Nothing is greyed out or refused silently.

## Check

- [ ] each field, message or dialog reread against its section
- [ ] a "to come" building block is not called; a "to come" rule is applied with existing means
- [ ] a building block or guard created: switched to "in place" in the page, in the skills, in the same commit
- [ ] number typed by `number_field`, displayed by `number_format`, read by `parse_input`
- [ ] every greyed element says why (`.on_disabled_hover_text`)
- [ ] no `.weak()` on a dialog's main content
