# Sheet: docked / detached views

Read before touching a UI surface (workbook, graph, note, matrix, fixed panel, global shortcut, field added to `AppState`).

## Where the rule lives

- Detached view wiring (shared state, sender identity, global shortcuts, egui IDs): `.claude/rules/windows.md` § Interface: wiring not to forget (detached window, layer, dialog).
- Detached windows and viewport wake-up: `.claude/rules/windows.md` § New graphs and detached windows.
- The `'static` constraint of `show_viewport_deferred`: `docs/application/windows-and-panels.md` § Generic detaching of content tabs ; `docs/application/managers.md` § Project Manager and windows by content.
- Fixed panels (PM, OM, console) attached / detached: `.claude/rules/menus-panels.md` § Object Manager, fixed panels, menus ; `docs/application/windows-and-panels.md` § Collapsible and detached panels (#100).
- Object copy between projects, F2 and Ctrl+D: `.claude/rules/copy.md` § Copying objects and figures.

## Check

- [ ] one writing per content (`book_contents`, `graph_view_contents`; the note and the matrix never detach, #473); otherwise factor, or change both and say so in the report
- [ ] the detached **graph** and **workbook** serve themselves (#475, L1, L2): their closure locks the app through `lock_app_from_viewport`, paints the shared state and applies its actions in its pass (`apply_pass_actions`), with no fork and no queue; only the three panels still copy and queue (until L3); a window that writes wakes the others through `server::wake`; its Reattach and Close are carried out in its own pass on native Wayland (it stops registering itself, `keep_or_close_own_window` / `keep_or_close_own_book_window`, #475 L4) and by the root under X11 and on Windows (`drain_detached_graph_flags`, `drain_detached_book_flags`): check the application's list, never egui's viewport map
- [ ] one cell entry in the whole application (#475, L2): the entry is the state's; a press in another window ends it (`commit_entry_held_elsewhere`), a dialog in the main window refuses that; a write the grid makes while a detached workbook paints runs under that window's own sender identity (`WindowIdentity`, #21), never the main one, so it cannot join a macro of the main window
- [ ] a new `AppState` field works in the detached workbook and graph as in the root (one shared state); in a detached panel it must be published to its copy
- [ ] a global shortcut tried with a graph active and a workbook detached; in a detached window only Ctrl+Z, Ctrl+Y and Ctrl+S act (`read_detached_history_and_save`, read before painting, queued after the window's own actions, #459); any other global key stays in the main window
- [ ] no egui ID computed under `Context::data_mut`
- [ ] a new dialog adds its variant to `OpenDialog` (`state/open_dialog.rs`, exhaustive `kind`) and its predicate to `AppState::open_dialog`; a new detached gesture source drains through `accept_detached_actions` (refused while a dialog is open, band in its window, #471)
- [ ] a new key reader in a detached closure comes after `window_band::stop_keys` (a dialog in the main window removes the key events there, #460); nothing re-reads raw keys before it
- [ ] a new dialog is painted through `show_edit_dialogs` / `confirmations::show` only, by the root, never in a detached closure (#472); a content feature works in every window, a global one stays in the main window
