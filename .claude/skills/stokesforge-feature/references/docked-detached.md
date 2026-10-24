# Sheet: docked / detached views

Read before touching a UI surface (workbook, graph, note, matrix, fixed panel, global shortcut, field added to `AppState`).

## Where the rule lives

- Detached view wiring (copied state, global shortcuts, egui IDs): `.claude/conventions.md` § Interface : câblage à ne pas oublier (fenêtre détachée, calque, dialogue).
- Detached windows and viewport wake-up: `.claude/conventions.md` § Graphes neufs et fenêtres détachées.
- The `'static` constraint of `show_viewport_deferred`: `docs/application/fenetres-et-panneaux.md` § Détachement générique des onglets de contenu ; `docs/application/gestionnaires.md` § Project Manager et fenêtres par contenu.
- Fixed panels (PM, OM, console) attached / detached: `.claude/conventions.md` § Object Manager, panneaux fixes, menus ; `docs/application/fenetres-et-panneaux.md` § Panneaux rétractables et détachés (#100).
- Object copy between projects, F2 and Ctrl+D: `.claude/conventions.md` § Copie d'objets et de figures.

## Check

- [ ] one writing per content (`book_contents`, `graph_view_contents`, `note_contents`, `matrix_contents`); otherwise factor, or change both and say so in the report
- [ ] a new `AppState` field also exists in the detached view when it must apply there
- [ ] a global shortcut tried with a graph active and a workbook detached
- [ ] no egui ID computed under `Context::data_mut`
