# Sheet: windows and dialogs

Read when creating or changing a content window, dialog, wizard, confirmation or fixed panel.

## Where the rule lives

- Catalogue, natures and size classes (S-F11), types, model window per type, anatomy: `docs/windows.md` § Natures and size classes ; `docs/windows.md` § The types ; `docs/windows.md` § Anatomy of a new window ; `docs/windows.md` § Reference window by type.
- What the building block does alone (size, position, Escape, focus): `docs/windows.md` § Size, position, Esc, focus: what the building block does by itself.
- Dialogs, footer, buttons, errors, confirmations: `docs/interface-standard.md` § Windows and dialogs ; `docs/interface-standard.md` § Confirmations.
- Building blocks (`show_window_in_layer`, `modal_dialog_with`, `modal_wizard`, `modal_live_settings`, `modal_read_only`, `modal_confirm`…): `.claude/conventions.md` § Application (`stokesforge-app` and `stokesforge-app-state`): building blocks and guards, point "every window goes through the common building block".
- Fixed panels: `.claude/conventions.md` § Object Manager, fixed panels, menus ; `docs/application/windows-and-panels.md` § Collapsible and detached panels (#100).
- Failure of a dialog gesture: `.claude/conventions.md` § Interface: wiring not to forget (detached window, layer, dialog).
- History macro of an editing dialog: `docs/commands-and-history.md` § Macros.
- Workspace keyboard under a modal: a new key consumer of the main window checks `dialogs::modal_shown` (#406), see `.claude/conventions.md` § Application.
- Screenshot to compare: `docs/windows.md` § Taking a screenshot.

## Check

- [ ] no raw `egui::Window` (`no_window_is_a_raw_egui_window`), even when a neighbour does it: existing code that deviates is not a model
- [ ] a reference twin named and compared by screenshot; mockup validated by the user before code (new window)
- [ ] editing dialog: gesture in a macro (`begin_macro`), OK closes it, Cancel aborts it
- [ ] greyed main button: `DialogFooter::gated` (tooltip and reason), otherwise `DialogFooter::always`
- [ ] nature chosen (`DialogNature`, S-F11) and a size class; free size only with `// FREE-SIZE(#300): <justification>`
- [ ] row added to `docs/windows.md` with the `stokesforge_…` id (`the_window_catalog_and_the_code_agree`)
- [ ] an undoable deletion asks nothing; otherwise `modal_confirm`
