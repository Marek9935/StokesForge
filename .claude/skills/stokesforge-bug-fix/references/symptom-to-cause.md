# Symptom → where the cause usually lives

Read at step 3 of `stokesforge-bug-fix`. The costly reflex is repairing where the symptom shows. Paths are under `crates/stokesforge-app/src/` unless stated.

| Symptom | Cause usually in |
|---|---|
| wrong value in the grid | `stokesforge-core` (columns, conversion) or the `Command` that wrote it, not `app/grid.rs` |
| Tab or an arrow leaves the grid, or the grid steals a key from a text field | keyboard focus of `app/grid.rs` (#38): `GRID_KEYS`, `claim_grid_focus`, `focus_free`; elsewhere `keyboard_focus_outside_grid`. Tests: `app::grid::tests` |
| grid, sheet tabs or bars misplaced in a workbook | `book_contents` (`app/workbook.rs`), shared floating/detached. Tests: `app::workbook::tests` |
| undo does not restore the previous state | `stokesforge-commands`: the delta computed at apply time, or the structural/data nature of the command |
| state lost after save + reload | `stokesforge-persistence` (`convert.rs`, `schema.rs`): a model field without transport |
| screen ≠ SVG/PDF/PNG export | text measurement (invariant 4) or a feature written in one backend (invariant 5): `stokesforge-rendering` |
| figure does not update | cache key `(revision, rect, pixels_per_point, view_generation)` of `stokesforge-render-egui` |
| detached window up to date only on hover | publication in `app/window_detach.rs` (`snapshot_stamp`, `sync_window_views`, `show_detached_tabs`): compare epoch, revision and geometry revision, then `request_repaint_of`; also test the return to idle without periodic repaint (#104) |
| action wrongly greyed, wrong label, inconsistent menu | `AppState` (`grid_menu`, `undo_label`…), never the egui code |
| fixed panel misplaced | panel order in `StokesForgeApp::ui` (`app.rs`, the only place that decides it); test `fixed_panels_are_placed_in_the_order_of_ui`. Panel contents live in their module (`app/menus.rs`, `app/script_console.rs`, `app/project_manager.rs`, `app/object_manager_ui.rs`) |
| floating window: frame, clipping, handles, z-order | `app/window_chrome.rs`, `app/window_placement.rs`, `app/window_stack.rs`; tests in their module |
| global shortcut without effect, or stealing a key | `collect_shortcuts` (`app.rs`, consumed before widgets) and `app/shortcuts.rs`; in a detached window only undo/redo is wired (`collect_undo_redo_shortcuts`) |
| gesture on a figure, in-place editor, columns dropped on a graph | `app/graph_window.rs` (`graph_view_contents`, `graph_drop_target`, `annotation_interactions`), `app/graph_text_edit.rs`; the decision in `AppState` or `stokesforge-render-egui` |
| series on the wrong layer, « Send to » aims at the wrong graph | layer index (`PlotSpec::panels`, never `(row, column)`, #167), published active layer (`graph_window::published_active_panel`, #161), `app/graph_targets.rs`, `state::drop_payload_action` / `drop_rows_action` |
| dialog painted or validated wrongly | `app/plot_settings.rs`, `app/import_wizard.rs`; shared frame and blocking in `app/dialogs.rs` |
| file drop freezes the window | `state::drop_files` keeps only refusals on the UI thread; reading and import run in `start_import`, applied by `poll_import` (#266) |
| file dropped in the wrong place | `app/file_drop.rs` (who claims the drop) then `state::file_drop_plan` (the decision) |
| works floating, not in a detached window | two writings where one is needed, or an `AppState` field never copied (`fork_for_detached_view`, `sync_detached_snapshot`, `adopt_detached_book_view`): `.claude/conventions.md` § Interface: wiring not to forget (detached window, layer, dialog) |
| interface freezes | something blocking called from the render loop (model: `ScriptEngine::run`, solved by a detached thread + `poll_script`) |
