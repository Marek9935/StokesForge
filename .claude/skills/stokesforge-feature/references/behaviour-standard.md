# Sheet: interface behaviour (fields, messages, greying, keyboard)

Read when placing a field, message, dialog, confirmation, greying, keyboard or mouse gesture, displayed number, long task or empty panel. `docs/interface-standard.md` is the single source; start at its rule index, then the gesture's section.

## Where the rule lives

- Rule index: `docs/interface-standard.md` § Rule index.
- A draft field is drawn under `field_state::for_target(<target>)`; a target pick (tab, row) is applied after the page it governs is drawn (`pick_pending` / `pick_allowed`), so a typed number lands on its own target (#498).
- Numeric fields, bounds, steps, wheel: `docs/interface-standard.md` § Numeric fields.
- Refused or clamped input: `docs/interface-standard.md` § Input refused or brought back.
- Decimal comma: `docs/interface-standard.md` § Decimal comma.
- Displayed numbers: `docs/interface-standard.md` § Number display.
- Text, in-place editing, Typst: `docs/interface-standard.md` § Text fields and in-place editing.
- Field help, dimmed text: `docs/interface-standard.md` § Messages and help.
- Greyed and its reason: `docs/interface-standard.md` § Disabled.
- Keyboard, mouse, cursors: `docs/interface-standard.md` § Keyboard and mouse ; `docs/gestures.md` § The model.
- Long task, empty panel: `docs/interface-standard.md` § Long tasks and empty states.
- Building blocks in place / to come: `docs/interface-standard.md` § Building blocks and guards.
- A rule that looks wrong: `docs/interface-standard.md` § A rule that looks wrong.
- Overview: `docs/ui-principles.md` § 4. Nothing is greyed out or refused silently.

## Check

- [ ] each field, message or dialog reread against its section
- [ ] a "to come" building block is not called; a "to come" rule is applied with existing means
- [ ] a building block or guard created: switched to "in place" in the page, in the skills, in the same commit
- [ ] number typed by `number_field`, displayed by `number_format`, read by `parse_input`
- [ ] every greyed element says why (`.on_disabled_hover_text`)
- [ ] no `.weak()` on a dialog's main content
