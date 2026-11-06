# Sheet: docked / detached views

Read before touching a UI surface (workbook, graph, note, matrix, fixed panel, global shortcut, field added to `AppState`).

## Where the rule lives

- Detached view wiring (copied state, global shortcuts, egui IDs): `.claude/conventions.md` § Interface: wiring not to forget (detached window, layer, dialog).
- Detached windows and viewport wake-up: `.claude/conventions.md` § New graphs and detached windows.
- The `'static` constraint of `show_viewport_deferred`: `docs/application/windows-and-panels.md` § Generic detaching of content tabs ; `docs/application/managers.md` § Project Manager and windows by content.
- Fixed panels (PM, OM, console) attached / detached: `.claude/conventions.md` § Object Manager, fixed panels, menus ; `docs/application/windows-and-panels.md` § Collapsible and detached panels (#100).
- Object copy between projects, F2 and Ctrl+D: `.claude/conventions.md` § Copying objects and figures.

## Check

- [ ] one writing per content (`book_contents`, `graph_view_contents`, `note_contents`, `matrix_contents`); otherwise factor, or change both and say so in the report
- [ ] a new `AppState` field also exists in the detached view when it must apply there
- [ ] a global shortcut tried with a graph active and a workbook detached
- [ ] no egui ID computed under `Context::data_mut`
- [ ] a new dialog adds its variant to `OpenDialog` (`state/open_dialog.rs`, exhaustive `kind`) and its predicate to `AppState::open_dialog`; a new detached gesture source drains through `accept_detached_actions` (refused while a dialog is open, band in its window, #471)
- [ ] a new dialog is painted through `show_edit_dialogs` / `confirmations::show` only, by the root, never in a detached closure (#472); a content feature works in every window, a global one stays in the main window
