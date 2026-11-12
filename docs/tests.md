# Tests and continuous integration

## Licenses and notices (#129)

The automatic, blocking job `licenses` installs the versions in
`scripts/licenses-tools.env`, runs `cargo deny --locked check licenses`, then
produces the Rust notices with `cargo-about`. Resolution covers all platforms
and features, including `python`, without compiling: this check does not
validate the Python bridge. The artifact keeps the texts generated from the
lock file of this pipeline.
`scripts/licenses-test.sh` checks the refusal witnesses: runtime without a
notice, Python notice set that is stale or from another platform (Linux and
Windows, `STOKESFORGE_LICENSES_PLATFORM`), supplements for another Cargo lock
file and a GPL-2.0-only dependency outside the policy. These probes use temporary directories and do not modify the sources.

**Windows distribution (#332).** `scripts/app-dist-windows.sh` is its own
test: packaged window opened then closed, `stokesforge-selftest` in an
emptied environment (PATH reduced to System32) and a trapped one (`PYTHONPATH`,
`PYTHONHOME`), then two refusal witnesses (an executable alone without
`python312.dll`, and a moved `lib/python`, must fail), the installer built
then installed in a folder with a space and an accent, selftest run again from
the installation, `.stf` keys checked, full uninstall. What these
trials do not prove: startup on a machine without the Visual C++ runtime
installed (the development machine has one in System32) and, without a parent
terminal, the display of the messages of `console_windows::attach_parent`.

Distributions call `scripts/licenses-dist.sh`, which adds the texts
of StokesForge, the Rust, OFL and Phosphor notices, those of the patches, as
well as CPython, its native libraries and numpy. It refuses a Python lock file
that differs from the versioned notice set and a runtime without the CPython/numpy
licenses. Policy and commands:
[`license-and-extensions.md`](license-and-extensions.md).

## Three families

| Family | Where | What it guards |
|---|---|---|
| **unit tests** | `#[cfg(test)]` in `src/` | a local behavior, as close as possible to its code |
| **integration tests** | `crates/*/tests/` | a crate property: public contract, boundary, agreement between paths |
| **robustness bench** | `stokesforge-robustness/tests/` | the foundation at its limits — 24 batteries `b1`…`b24` |

## Guards not to break

A few tests do not check a behavior but **an architecture property**. They
fail on purpose when someone crosses a boundary by accident, and they are the
ones to understand before changing them.

| Test | What it holds |
|---|---|
| `*/tests/no_ui_dependency.rs` (eleven crates, including `stokesforge-app-state` since #263: no `egui`, no `eframe`, no `winit`, no `stokesforge-clipboard`) | dependency allow-list: every addition to `Cargo.toml` must be added to it explicitly (invariant 2). Shared reading `stokesforge-arch-guard`: normal, build, `target.*` and renamed (`package`) dependencies; `dev-dependencies` excluded; negative witnesses in `stokesforge-arch-guard/src/lib.rs` |
| `stokesforge-persistence/tests/witness_files.rs` (S4, #323) | the file `tests/witnesses/v1/project.stf` is read back to give exactly the witness project (all node, column and series types, non-default values), and saving the project gives back its bytes: the written format does not drift without a decision. Witness: renaming a field of `schema.rs` or `recipe_record.rs` fails both tests. Regeneration: see `persistence.md`, "Version rule: every V1 file can be read back up to V3" |
| `stokesforge-arch-guard/tests/skill_references.rs` (F1, #301) | in all project skills (`.claude/skills/stokesforge-*` and `run-stokesforge`, `SKILL.md` and `references/*.md`), every reference "`fichier.md` § Title" names a file and a heading that exist, a `§` without a path is refused; every link `](references/x)` of a `SKILL.md` points to an existing file and no file of `references/` is orphaned. Witnesses on fake trees (missing heading or file, `§` without a path, dead link, orphan, third-party skill ignored) |
| `stokesforge-arch-guard/tests/standard_codes.rs` (#301) | every `S-XN` code cited (docs, skills, comments, tests) appears in the rule index of `docs/interface-standard.md`; witness on an unknown code |
| `stokesforge-logo-gen/tests/generate.rs` (#388) | the icon generator on synthetic SVG in a temporary folder (never the real logo): all eight sizes of both trees with exact dimensions, half-opacity fill gives alpha near 128 with unscaled colour (premultiplied pixmap demultiplied), the `.ico` has the ICONDIR type 1 and seven PNG frames 16 to 256 each decoding at its declared size, 256 stored as 0, no 512; two runs write identical bytes; refusals with status 1 and nothing written: off-centre drawing, opaque outer ring (tile, full bleed), `feDisplacementMap` or `feImage`, missing path. Proven mutants: 512 dropped, pixmap written premultiplied, single `.ico` frame, a changing byte added to the PNG, each refusal check removed |
| `stokesforge-app/src/app_logo.rs` (tests, #388) | no file: the window icon is `Some` and empty, no logo; valid PNG at the literal installed path: dimensions kept; corrupt bytes degrade like an absent file; the main builder has the icon and `app_id` `stokesforge`; the texture keeps straight alpha. Proven mutants: no icon without a logo, wrong path, `unwrap` on the decoding |
| `stokesforge-app/tests/guards.rs` `no_viewport_without_identity`, `the_application_id_is_the_desktop_name` (#388) | no `ViewportBuilder::default()` outside `app_logo.rs` (every window has the `app_id`); `APP_ID` is the name of the desktop entry and of the icon `install-linux.sh` writes. Witness on fake sources; proven mutants: `window_detach.rs` back to a bare builder, constant renamed |
| `stokesforge-app/src/app/about_window.rs` (tests, #388) | without a logo no image is painted and the texts are those of before; with one, exactly one image of 96 pt above the version text and Copy still clickable (the window grows by the logo room). Proven mutants: image always painted, `Some` ignored |
| `stokesforge-arch-guard/tests/config_dir.rs` (#332) | `ProjectDirs::from` is called only by `stokesforge-import/src/config_dir.rs` (comment lines ignored): every profile path goes through `config_dir()`, which tests redirect with `STOKESFORGE_CONFIG_DIR`. Witness: a fake source that calls it is refused, a comment is not; proven mutant: the call added to `recent.rs` fails the guard |
| `scripts/french-guard.py` (French guard, #377) | the repository is written in English: every tracked text file is scanned and a line that looks French (at least two French stop words, or accented French words) fails. **Exempt by path**: `*/i18n/fr/*`, `assets/logo/` (the logo's working archive, #443), `docs/archive/`, `test/import/`, `decision-form.html`, the guard's own test, and the `CHANGELOG.md` sections up to `1.0.0`. `vendor/` is no longer exempt (#380): our files there are in English, and a future upstream file with French needs a `lang: fr` marker or an `EXEMPT_FILES` entry. **Exempt by content**: «…» spans (also across two lines), `` `code` `` spans, URLs. **Markers** for deliberate French elsewhere, after a comment opener (`//`, `#`, `<!--`, `/*`, `;`): `lang: fr` on the line or alone on the line above; a block `lang: fr begin` … `lang: fr end` (unclosed = error); for a Markdown table `<!-- lang: fr columns: French, Refused -->` just above it exempts only the named columns (used by the glossary tables of `ui-texts.md`). File names are not checked. Local run: `python3 scripts/french-guard.py` (`--help` for paths), exit 1 and `path:line: excerpt` per hit. Witnesses: `python3 scripts/french-guard-test.py` (false positives, markers, exemptions, exit codes, git edge cases); run by the `fmt` job |
| `stokesforge-render-svg/tests/no_ui_dependency.rs` | the export sees neither `egui`, nor `eframe`, nor `winit` — so it can be tested without a graphics server |
| `stokesforge-render/tests/measurement.rs` | the single source of text measurement (invariant 4) |
| `stokesforge-render/src/{view,interact}.rs` (modules `tests`) | interaction without egui (#262): framing of a gesture (zoom, pan, log scale, inverted axis, overlaid layers, axis alone, unknown panel), reset, nearest point (curve, out of range, hidden or right-hand series, layer of the spec); proven mutants (pan sign, box filter, linked layer, radius, hidden series); geometry of the analysis preview (#261): `PanelTransform::new` (fractional origin, inverted log, zoom, rectangle), initial frame (`fit_range`, `axis_frame`), `snap_to_curve` (n, edge, `ys` too short) and `snap_to_curve_zone` (averaged zone: odd and even n, truncated edge, unsorted x values, #334) and `scale::axis_ticks` (same points as the pipeline, decades, comma, bound without an image); proven mutants (rounded origin, `1 - fy`, `> 0` filter in log, half window, `label_count` fixed at 5, forgotten comma, `abs` of the right-click tolerance, `.min(ys.len())`, half window of the zone, zone bounds taken at the extremities); snap circle (#334): `snap_radius` (one point = 6 px, half side, asymmetric zone, inverted X axis, cap, non-finite pixel) and the preview bench (the circle grows with N, text = real number of points at the edge); proven mutants (`abs`, `max` → `min`, floor, cap, hard-coded radius 6.0, N instead of the real number); `app/analysis_preview.rs` (`zone_tests`): zone drag, right-click on an inverted X axis; preview navigation (#270, `Bench` bench frame by frame): wheel alone has no effect, Ctrl + wheel and pinch (data under the pointer kept), zoom per axis, Shift + wheel = average without zoom, Ctrl + drag (frame translated, no zone, inverted X axis, per axis), Ctrl + click without a point, double-click reframes, peaks (click = seed, drag = range); proven mutants (pan sign, wheel alone, `!command` of the click, axis bounds, drag mode, wheel reading under Shift, inverted factor, double-click); `app/form.rs`: surrounded texts (`toggle_texts`: solid background of the current option, toggle, click on the current one); `stokesforge-app-state/tests/analysis.rs`: `tool_choices` (only peaks have the choice) |
| `stokesforge-render-egui/tests/agreement.rs` | screen and export place the same primitives at the same pixels; ticks inside / on both sides / cut / minor ticks on the four sides (#344), each different from the default figure; proven mutant (`ticks` ignored) |
| `stokesforge-render-egui/tests/typst.rs` (`every_text_of_a_figure_keeps_a_live_texture_at_every_scale`, #521, #508 point 1) | through `ScaledShapes::get` at scales 1, 0.5, 1.37 and 2 (twice), every Typst text of a figure that repeats a legend text, a title and an annotation keeps a live texture, and the number of meshes equals the Typst groups of the SVG in text mode |
| `stokesforge-render-svg/tests/mirrors.rs` | "Advanced" settings of the tick marks (#344): major tick mark at 10 against 5 by default, minor Auto = half of the effective major, color of the tick marks only (line and numbers keep theirs), thickness, lengths NaN / negative / 1e30 and thicknesses 0 / NaN / 1e9 bounded without panic; proven mutants (`custom_ticks` without the thickness, half fixed at 3, color ignored, thickness ignored, length not read); mirror axes (#176) and ticks (#344) seen in the export: bare line (line kept, no ticks, no numbers), inside / both sides (outer half = Plotters tick), numbers unchanged without ticks, neighbor left at default; default figure identical to the byte; proven mutants (`ticks` ignored, sign of the direction, asymmetric `Both`, length, gap of the numbers) |
| `stokesforge-render-egui/tests/{interaction,cache_session}.rs` | graph gestures replayed frame by frame (#318): plain drag has no effect (framing, generation, cache), Ctrl or middle click + drag = pan (background, axis alone, Zoom and shape tools, above a text, title or shape), Ctrl read at the press, Ctrl + click without text or shape, feathering cut only during a real gesture; proven mutants (pan on plain drag, middle button, object guard, rectangle guard, text and plot guard, feathering) |
| `stokesforge-render-egui/tests/{cell_handles,layout_mode}.rs` (#426) | outside the Page Layout Mode no drag resizes or moves a graph, grid or Free (old cell and figure edges, layer edges and plot area: no event, default cursor, model and layout unchanged: `no_layout_drag_outside_the_mode`), while the drags of a free text, a shape, the title (and its width handle) and the legend still work on a Free graph; Shift + click and click selection; in the mode, the guides of a drag (`the_guides_of_a_box_drag_show_while_the_pointer_is_down`) |
| `stokesforge-render-egui/tests/feathering.rs` | feathering does not enter the cache key (invariant 6) |
| `stokesforge-app-state/tests/script_bridge_lazy.rs` | the Python interpreter does not start when the window opens |
| `stokesforge-app/tests/labels.rs` | no hard-coded interface text (`no_hardcoded_label`, absolute guard, no exemption, #48), including the `set_title` titles and visible `add_filter` names of file boxes (#127; technical extensions and format acronyms preserved) and the free functions `section_title`, `section_heading`, `table_header`, `secondary_text`, `dimmed_text`, `bold`, `list_row`, `list_row_text` (#169, #170, #392); the #43 zone (`ZONE_43`) with no literal at all, its functions looked up by name in all sources — a missing entry fails; the translation dependencies stay in `stokesforge-app` and `stokesforge-app-state`; `no_catalog_key_is_dead` (a key of `en` unused in the sources of both crates, witness `the_dead_keys_guard_reads_both_crates`); the `history-*` keys used; `notice`/`fail` of `app-state/state.rs` read (`the_guard_reads_the_state_messages_of_the_state_crate`); the witnesses that prove the guard sees a literal, including in a subfolder of `src/` |
| `stokesforge-app/tests/guards.rs` | no bare `egui::Window` (`no_window_is_a_raw_egui_window`); the properties window goes through `modal_dialog` → `modal_dialog_with` → `modal_frame` → `show_window_in_layer` (like `modal_wizard`, the import window, `modal_live_settings` and `modal_read_only`, #228, then `modal_confirm`, `modal_save_changes` and `modal_alert`, #229); no global import (`use …::*`) outside test code (`no_global_import_outside_tests`, decision 2 of #115, absolute since #117); no `ui.separator()` in a menu outside the section building block (`no_raw_separator_in_a_menu`, #155, absolute); egui selectable button only in a menu, a drop-down list or the color picker (`no_selectable_button_outside_a_menu`, #169); no color `Color32::from_…` or named tint outside `theme.rs` and `color_picker.rs` (`no_hardcoded_color_outside_the_theme`, #169); no `ui.separator()` just before a section title or a text level (`section_heading`, `table_header`) (`no_separator_before_a_section_title`, #169, #170); no `bold(…).small()` outside `theme.rs` (`no_small_bold_outside_the_theme`, witness `the_small_bold_guard_sees_a_bold_small`, #170); neither `DragValue` nor `Slider` (`no_numeric_field_outside_the_building_block`, #220, absolute); no bare bubble (`on_hover_text`/`on_hover_ui`) on a box, a radio, a text or number field, a field label outside a menu (`no_bare_tooltip_on_a_form_field`, #225, absolute — the reminder goes through `help_beside`/`help_text`); every `add_enabled` carries `.on_disabled_hover_text` (`no_disabled_element_without_its_reason`, #226, absolute); no `add_space(<nombre>)` outside `theme.rs` (`no_literal_add_space_outside_the_theme`, #227, absolute — `theme::SPACE_ITEM` / `SPACE_SECTION`); no `.weak()` outside `theme.rs` (`no_weak_outside_the_theme`, #392, absolute — `theme::secondary_text` / `dimmed_text`; witness `the_weak_guard_sees_a_weak_written_in_place`, proven mutant: `.weak()` back in the worksheet header text fails it); a `rfd::` only in `file_picker.rs` of `stokesforge-app-state` (`no_rfd_picker_outside_the_module`, #192, witness `the_picker_guard_sees_an_rfd_outside_the_module`); no integration test (of the two `tests/` folders) calls `AppState::new` (`no_integration_test_uses_appstate_new`); the old guard "state modules do not name egui" (`MODULES_SANS_EGUI`, #255) is removed: since #263 the compiler and `stokesforge-app-state/tests/no_ui_dependency.rs` hold the boundary; no free size of an interactive window without `// FREE-SIZE(#300): <justification>` in the 12 preceding lines (`no_free_size_without_justification`, #300, S-F11, witness `the_sizes_guard_sees_a_bare_free_size`, proven mutation: marker removed); every `stokesforge_…` identifier passed to a `modal_*` appears in `docs/windows.md` and every line of the catalog has its call, without reading the sizes (`the_window_catalog_and_the_code_agree`, #300, witness `the_catalog_guard_sees_a_deviation`, proven mutation: catalog line removed); the witnesses of the source reader |
| `stokesforge-app/tests/manual_labels.rs` (#363) | every menu or submenu entry built by the functions of `MENU_FUNCTIONS` (menu bar, Sheet and Columns menus, sheet-tab and column context menus; 102 entries) has its English label (catalog `en`) written in bold, alone or as an item of a menu path, in a page of `manual/src/`, the generated pages `reference/interface-texts.md` and `reference/shortcuts.md` excluded. `NOT_IN_MANUAL` (key, reason) holds the exceptions; a stale key or a renamed function fails. Not covered: the context menus of the Project Manager and of the graph components (action tables). Proven mutations: a label un-bolded in the manual fails naming key and label; a stale exception fails |
| `stokesforge-app/src/app/dialogs.rs` (module `tests`) | window natures (#300, S-F11): `each_nature_fixes_its_footer_size_and_focus` (table nature → footer, default class Small 480 x 170 / Medium 480 x 420 / Large 640 x 500, focus; the field of the variant wins), `the_initial_focus_follows_the_nature` (confirmation → Cancel, save and alert → primary, never the content field; read-only → nothing; witness: the draft goes to the field), `a_greyed_out_footer_shows_its_reason_and_not_its_tooltip` (`DialogFooter::gated` / `always`); S-F8 witness kept (`a_reopened_dialog_takes_back_its_size_and_position`); proven mutants (Small and Medium swapped, confirmation moved to the first field) |
| `stokesforge-app/src/theme.rs` (module `tests`) | no `ScrollStyle` outside the theme (`no_scroll_style_outside_the_theme`); `a_page_scroll_area_adds_an_end_margin` (#347: content = block + `SPACE_SECTION`, mutant: margin 0 fails); the guard `no_raw_scroll_area_without_justification` and its witness `the_scroll_area_guard_sees_a_bare_area` live in `tests/guards.rs` (mutant: `plot_settings.rs` back to a raw area fails); no egui `strong()`, all bold through `theme::bold` (`no_egui_strong_outside_the_theme`, #169); `bold` really widens the text with the embedded fonts; four text levels (#170, #392): `each_text_level_sets_weight_and_size` (section title bold at normal size, table header bold at small size, secondary dimmed at 11 pt, slightly smaller than body, dimmed text muted at normal size and not bold) and `section_title_paints_the_section_heading_level`; proven mutants (`.small()` removed from the header, secondary back to `.small()`, bold removed from the title, `.small()` added to or `.weak()` removed from `dimmed_text`) |
| `stokesforge-app/src/app/{list_row,object_manager_ui,project_manager,curve_settings}.rs` (tests) | single row height (#170, points 5 and 8): `every_list_row_has_the_single_height` (with or without an icon, selected or not = `LIST_ROW_HEIGHT`, a bare label is lower), `the_composed_row_takes_the_single_height_and_hands_it_back`, `the_reserved_background_imposes_the_minimum_height`, `group_series_and_object_have_the_same_row_height` (text centered in the row), `a_folder_has_the_height_of_a_window` (constant step folder/window), `the_series_row_has_the_single_height_with_its_checkbox`; proven mutants (height not imposed on the row, on the folder header, on `list_row_sensed`) |
| `stokesforge-app/src/app/import_wizard.rs` and `import_wizard/grid.rs` (tests, bench `import_wizard/test_bench.rs`) (#488) | the one-screen import: `the_one_screen_paints_its_panel_and_its_footer`, `finish_imports_the_recipe_as_it_is_and_cancel_imports_nothing` (Enter imports each file with its recipe, without a file nothing, Escape closes the guide first and the window second, Cancel imports nothing), `the_guide_shows_until_dont_show_again_and_show_guide_brings_it_back`, `the_advanced_fold_is_closed_by_default_and_a_change_shows_as_a_sentence`, `deleting_a_template_goes_through_a_modal_confirmation`, `clicking_the_auto_button_never_deletes_the_choice` (deleted only by the explicit Drop button of the card), `a_point_to_check_is_marked_beside_its_sentence`, `a_tie_between_templates_offers_the_other_one`, `manage_templates_lists_the_remembered_answers_and_forgets_one`, `the_grid_paints_only_the_visible_rows_and_columns` (630 lines x 241 columns, synthetic) and `only_the_visible_rows_are_laid_out`, `the_role_bar_closes_when_the_pointer_leaves_and_when_the_grid_scrolls`, `long_texts_of_the_left_panel_are_never_cut` (pseudo-locale `en-XA`, window of the least width: « Mark as Checked » and the « To check » tags end inside the panel); every decision (what is a doubt, what a role does, which template fits) is tested in `stokesforge-app-state` |
| `stokesforge-app/src/{theme,app/file_drop,app/graph_window,app/window_detach,app/window_stack}.rs` (tests) | single drop stroke on the target window's border (#170 point 2, #386, S-X5): `the_drop_target_stroke_is_a_light_blue`, `a_file_over_the_workspace_outlines_the_application_alone`, `a_file_over_a_workbook_lights_up_its_window_border_alone`, `the_drop_border_replaces_the_active_outline`, `the_drop_target_does_not_outlive_its_frame`, `a_hover_outlines_the_book_under_the_real_cursor`, `a_file_over_a_maximized_workbook_lights_up_its_area`, `a_file_over_a_detached_workbook_lights_up_the_viewport_border`, `dropping_columns_on_the_graph_adds_a_series_and_elsewhere_nothing` (stroke on the window rect); proven mutants: accent green back, main outline without the claim condition, no claim marker, marker not reset, both outlines painted, marker ignored by the detached tab / maximized area / graph |
| `stokesforge-app` `app/main_dialogs.rs` (#472) | every dialog opens in the main window, on the `test_support::bench` bench (a root and one detached graph, driven like eframe): painted by the root only, woken by the commands sent to it, one `Focus` per opening, the band of the window of the gesture only (5 s), OK reaches the detached window in the same pass, a dialog opened while the root is unfocused paints in full and takes typing, Ctrl+Z in a dialog field, a confirmation drops the detached action, closing the window leaves the dialog, a small window does not change the remembered size |
| `stokesforge-app` `app/detached_keys.rs` (#459) | Ctrl+Z, Ctrl+Y, Ctrl+S and Ctrl+Q in the five detached windows, on `Bench::with_every_window` (graph, workbook, three panels, a project with a file): one step per press in each window, one Save and the file written, Ctrl+Shift+S saves nothing, no file yet queues Save, typed text of a cell and of the Project Manager name field stays while the command is undone, the keys are read before painting and queued after the window's own actions (a strip click undone in its own frame), a dialog in the main window refuses the keys with the cursor in a cell, an empty history says so in that window only, Page Layout Mode keeps its mode on Ctrl+S, Ctrl+Q closes nothing and says where Quit is, the #460 stop for a detached workbook grid and Project Manager; the console field case is `python`-only (the field is disabled without it); #518 replaces the routine trial steps (Ctrl+Z/Y, save and reopen) with `ctrl_s_with_a_pending_cell_entry_saves_it_in_its_cell_and_nowhere_else` (docked and detached, reopened file compared cell by cell), `switching_sheet_tab_with_a_pending_cell_entry_commits_it_docked` (its detached twin is ignored for #524) and `the_same_keys_give_the_same_result_docked_and_detached` (absolute values at every step, docked = detached), on `Bench::with_docked_workbook` / `with_every_window`; witnesses: `commit_view_entries`, `commit_open_entries` in save, the grid's `commit_edit`, `collect_undo_redo_shortcuts`, the key order after the window's actions, Save dropped |
| `stokesforge-app/tests/wayland_covered_root.rs` + `examples/wayland_covered_root.rs` (#457, manual, `#[ignore = "manual: …"]`) | on native Wayland a root window fully covered by a maximised child window keeps draining the requests the child queues (probe: root `ui` passes, requests drained, child passes). Starts its own headless `mutter` (`dbus-run-session`), runs the example with `WGPU_BACKEND=gl`, kills the whole process group also on failure; skips with a message when `mutter` or `dbus-run-session` is absent. Run: `cargo test -p stokesforge-app --test wayland_covered_root -- --ignored --nocapture`. Second test: with no request the covered root stays asleep (#104). Third test: with two children, A small and repainting and B maximised over everything, the root never reads `occluded == Some(true)` for A (only the root opts in) and still drains B's requests (witness: every window reporting gives `occluded_true=40`; no opt-in gives `root_ui=1 drained=1`). Witness: with the frame-starvation patch of `vendor/winit` disabled, the first fails (`root_ui=1 queued=25 drained=1`). Under the Vulkan backend this compositor stops answering frame callbacks to every window, hence GL |
| `stokesforge-app/src/color_picker.rs` (tests) | "Square \| Wheel" as tabs (#170, point 7): `square_and_wheel_are_underlined_tabs` (accent stroke under the active tab and not under the other, no `selection.bg_fill` background behind a label, click on Wheel changes the view and keeps `stokesforge_picker_shape`); the fallback test (`the_picker_is_folded_by_default_and_unfolds_on_demand`) still reads "Square" as collapsed text; proven mutant: `selectable_value` restored |
| `stokesforge-app/src/app/graph_window.rs` (tests) | axis bounds published to the properties panel (#170, point 1, delivered by #221): `published_bounds_are_those_of_the_draw_in_ascending_order` (inverted axis plotted backwards, published bounds ascending, unknown layer = `None`), `les_champs_de_bornes_grises_montrent_les_bornes_et_leur_raison_au_survol` (logarithmic axis from 1 to 1000: "1000" shown, disabled, and `plot-axes-auto-range.disabled` on hover only); proven mutants: `ascending` removed, `on_disabled_hover_text` removed, published bounds replaced by the logarithms |
| `stokesforge-app/src/app/project_manager.rs` (tests), `stokesforge-app-state/tests/project_objects.rs` | selectable folders (#170, point 4): `a_click_on_a_folder_selects_and_expands_it`, `a_click_on_an_open_folder_selects_it_without_collapsing_it`, `a_double_click_collapses_the_folder` (closed or open at the start), `the_triangle_collapses_an_open_folder`, `a_selected_folder_carries_background_bar_and_hover_like_a_window`, `delete_and_f2_act_on_a_selected_folder` (`DeleteObjects([dossier])`, in-place rename), `ctrl_v_with_a_selected_folder_pastes_into_the_folder` and `the_paste_parent_is_the_single_folder_or_the_root`, `ctrl_c_on_a_folder_copies_the_folder`, the right-click of a folder (Copy and Duplicate push their action, #391); state side (#391) `a_copied_folder_pasted_into_another_folder_reads_its_own_workbook`, `pasting_with_the_copied_folder_selected_puts_the_copy_beside_it`, `duplicating_a_folder_is_one_history_entry_beside_the_original`, `a_folder_and_a_window_inside_it_make_a_single_copy`, `the_copied_graph_is_not_a_user_of_the_original_workbook`; core side `project_model.rs` module `folder_copy` (graph, graph before its workbook, graph reading outside, formulas inside and outside, calculation, transposed sheet), persistence `clipboard.rs` (folder with a calculation, external paste with no hidden workbook and `#REF`), session `recalc_targeted.rs` (`a_pasted_folder_recalculates_its_own_formulas_and_not_the_original`), battery `b22` step `PasteFolder`; proven mutants: folder insertion removed from the selection, click that toggles instead of only expanding, double-click without collapse, click without expand, `paste_parent` always at the root, Copy not disabled, `selected` of the folder row forced to false, remap only for an external copy, graph remap inside the node loop, reservation for everything outside, embedded inside columns, calculation output or transposed source not renumbered, calculations dropped from the clipboard record, paste-beside redirect removed |
| `stokesforge-app/src/app/{project_manager,object_manager_ui,list_row}.rs` (tests) | keyboard navigation of the managers (#170, point 6, S-K9): `two_downs_reach_the_third_visible_row` (collapsed folder skipped, bounds), `without_focus_the_arrows_do_not_touch_the_panel`, `a_panel_arrow_does_not_reach_the_grid` (↓ consumed before the grid), `shift_arrows_extend_and_shrink_the_range_and_alt_down_does_nothing`, `left_and_right_fold_unfold_and_walk_the_tree_like_a_file_explorer`, `left_and_right_do_not_reach_the_grid`, and (#423) `a_click_on_a_folder_selects_it_without_expanding_it`, `a_double_click_toggles_the_folder_once`, `a_click_on_the_selected_name_never_renames_it_only_f2_does`, `ctrl_x_cuts_the_selection_only_with_the_focus`, `a_cut_row_is_dimmed_and_a_cleared_mark_restores_it`, `a_cut_row_fades_its_icon_and_a_closed_window_does_not`, app-state `a_cut_pastes_once_a_second_paste_does_nothing_until_a_new_copy`, `a_paste_into_a_closed_folder_opens_it`; app-state `tests/project_objects.rs` cut and paste = move (`cut_then_paste_of_the_same_text_moves_the_node_in_one_history_entry`, text from elsewhere, descendant and name refusals), `enter_opens_the_window_and_toggles_the_folder`, `the_keyboard_cursor_row_has_the_focus_outline`, `the_neighbor_row_is_bounded_and_without_a_cursor_we_start_from_one_end` (`neighbour`); Object Manager side `arrows_and_enter_walk_through_the_object_manager_rows` (neighbor Select, Properties on Enter, Ctrl+↓ = `MoveSeries` without selection) and `the_selected_row_has_the_focus_outline_when_the_panel_is_focused`; proven mutants (#423): open or expand on a plain click, close instead of toggle on double-click, ← / → handlers, Shift+arrow as a plain arrow, Cut event, dimming, reveal after paste, menu Cut, Alt+↓ not consumed, text equality of the cut, mark not cleared by Copy, descendant and name-taken checks, move not run; proven mutants: navigation on `order` instead of the visible rows, arrow read without being consumed, Shift or Alt+↓ variant not consumed, Enter removed, focus outline removed (PM and OM), neighbor not bounded, neighbor without a cursor |
| `stokesforge-app/tests/support/sources.rs` | not a test: the **single reader** of all guards that read the code (`labels.rs`, `guards.rs`, and through `src/lib.rs` those of `icons` and `theme`). Reads `src/` of **both** crates recursively (`stokesforge-app` and `stokesforge-app-state`, the latter under the prefix `app-state/`: `app-state/state.rs`); production view without comments or `#[cfg(test)]` items (each one, not only the first; a `#[cfg(test)] mod x;` makes its whole file a test file); finds a function by its name, wherever it lives (#116). A new guard that reads the code goes through it, never through `include_str!` or a named file |
| `stokesforge-app/src/field_state.rs` (tests) | field state (#219): pure decision of the cycle (`decide`), red/orange frame and bubble read in the painted shapes, old value given back on exit, orange cleared after 2 s, witness "valid input → no frame"; proven mutants (color, field kept open, deadline, exit without change, bubble) |
| `app/busy.rs`, `app/empty_state.rs`, `stokesforge-app-state/tests/shell.rs` (#232) | indicator absent before 0.5 s then the word, Stop only after 3 s and only if the work can be interrupted; empty-state phrase dimmed (`dimmed_text`) and centered; import Stop: nothing created, no history entry, a later import runs to the end |
| `stokesforge-app/tests/guards.rs`, `app/shortcuts.rs`, `app/keyboard_shortcuts_window.rs`, `project_manager.rs` (tests) | keyboard and mouse (#231): guards `no_consume_key_outside_the_registry` and `no_cursor_outside_the_cursors_module` (witnesses), every consumed constant appears in `HELP`, shortcuts window, Help menu, double-click in the Project Manager (activates, never closes, two Ctrl-clicks do not open) |
| `stokesforge-script/tests/frozen_api.rs` (#331, feature `python`) | guard of the **frozen V1 API** of `pyst`: **reads** the `pyst-api` blocks of `docs/python-and-scripting.md` (no copy) and compares them with the built module — names and kind of `pyst` / `pyst.analysis`, `inspect.signature` of each function (clear refusal if one fails, otherwise the test would pass vacuously), `Handle` without a public attribute, docstrings, option keys (`plot`, `set_graph`, `set_column_meta`, read in the unknown-key message), keys of the returned dictionaries (`info`, `series`, `series_groups`, analysis results, `readiness`) and enumerated values. It replaces `la_surface_expose_vingt_huit_appels_et_quatre_erreurs`. Witnesses by mutation: call added, parameter renamed, default changed, dict key added, getter on `Handle`, option key added, line removed from the doc. The test `the_api_version_of_the_module_is_the_constant` checks `pyst.__api_version__` against `PYST_API_VERSION` (#356). Limit: an additional value *accepted as input* (not listed) is seen only if it is also returned; the defaults `panel=(0, 0)` and `png_scale=4.0` are matched to the PyO3 `Ellipsis`, so their value is not read back |
| `stokesforge-app/src/number_field.rs` (tests) | "number" building block (#220): reading (refusal, bounds, integers), step and large steps, value list on the keyboard (Alt+↓, Enter, Esc), click = all selected, wheel on hover without a focus that follows the list (`step_preset`: off the list, extremities), field without a list = scrolling window, full arrow in the frame (`list_frame`: distinct theme backgrounds), consistency of the quantity table; in `plot_settings`: input validated on Enter, wheel notches = one history entry, Esc gives back the value then cancels the dialog |
| `stokesforge-app/src/app/dialogs.rs` (tests) | dialog building block (#228): button order read in the shapes (dialog, wizard, read-only), pure decision of the footer (`footer_message`: the error comes before the reason) and its painting (the reason returns when the error disappears, active button witness), initial focus on the first text field and no focus without a field, Esc that leaves a field then cancels, geometry returned on close and taken back on open; proven mutants (initial focus, Esc); state side (`tests/shell.rs`): geometry written to the preferences outside the history, error in the footer of the open dialog only. Confirmations (#229): Enter and Esc = Cancel in `modal_confirm` (the click of the verb confirms), Enter = Save and Esc = Cancel in the trio, Discard Changes on the left, verb red on hover then on keyboard focus (Tab), pointer gone to rest; `modal_alert`: Enter = Close, action by its button; proven mutants (Enter of the confirmation, red on focus) |
| `stokesforge-app`: confirmations on the state and window side (#229) | `stokesforge-app-state/tests/shell.rs`: New Project / Quit / Open… ask only if there is work to lose, Save saves then continues (and a failure or a closed picker abandons the gesture), unreadable project: path and project kept (S-M8), failed save (S-M8), export whose added extension would target an existing file; sheet and objects deleted without confirmation, message that names the graphs (`tests/project_objects.rs`); `app::tests::closing_the_window_asks_before_losing_work` (CancelClose, then Close after Discard); `import_wizard::tests::deleting_a_template_goes_through_a_modal_confirmation` (Enter cancels without validating the import) |
| `stokesforge-core/src/calc.rs` (module `output_name_tests`), `stokesforge-app-state/tests/output_names.rs`, `stokesforge-script/tests/analysis.rs` (#256) | names of analysis output columns: the rule (`OutputName::of`, `output_unit`, `wants_output`, `family_english`) is tested in core; the **agreement test** checks that the `en` catalog says `OutputName::english()` for each variant (list of examples guarded by an exhaustive `rank()`) and `analysis-kind-*` = `family_english`; `pyst` gives the same name, unit and comment as the interface, and `peak_columns`/`sum_column` filter like the boxes. A new `OutputName` variant does not compile without an `analysis-output-*` key |
| `stokesforge-app-state/tests/shell.rs` (figure copy) and `stokesforge-app/src/app/figure_copy.rs` (tests), #149/#263 | the state prepares neutral bytes (`FigureBytes`) in the order of the preferences (SVG, PNG, PDF by default; order and unchecked boxes followed), without touching the model; the window translates each `ExportFormat` into its MIME type (`mime_of`, witness: two swapped types fail the test) and each `ClipboardError` into a sentence that repeats the detail (`clipboard_error_message`); the system's refusal replaces the "copied" message |
| `stokesforge-app-state/src/i18n.rs` (tests) | every key of `fr` exists in `en` with the same arguments; the pseudo-language surrounds and lengthens every message; blocking completeness (every key of `en` exists in `fr`); French plurals and typography; an `fr` preference opens the interface in French; language choice without panic; `tests/language_switch.rs`: the language changes without restarting |
| `stokesforge-commands/tests/memory_pinning.rs` | the history does not pin big buffers indefinitely |

### The witness, without which an equality test proves nothing

This is the most important method in the repository, and it applies beyond
rendering.

A test that compares two paths can pass because both are right —
or because **both are wrong in the same way**. So `tests/measurement.rs` and
`tests/agreement.rs` each carry cases that must be **different**:

- the same figure plotted with the **bare** `SVGBackend`, the one that falls
  back on the UTF-8 byte heuristic, must be placed **elsewhere**;
- for each added feature, a variant of the graph that changes it must
  produce a **different** figure.

Without them, a wrapper that delegated `estimate_text_size` by mistake
would pass as correct. If you touch these tests, read their witnesses
first.

## The robustness bench

`stokesforge-robustness` contains **no production code**. It depends on the
whole workspace and carries two things:

- **24 batteries** (`tests/b1…b24`) that test the foundation at its limits:
  extreme numeric values (`b1`), persistence (`b2`), malformed then
  semantically hostile `.stf` files (`b3`), identifier allocator
  (`b4`), macros (`b5`), concurrency between clients (`b6`), Rust ↔ Python
  boundary (`b7`), properties drawn at random by proptest (`b8`), script
  under constraint (`b9`), version counter (`b10`), scope of name
  uniqueness (`b11`), commands accepted without effect (`b12`), bounds of a
  `PlotSpec` coming from disk (`b13`), sweep rechecking three invariants after
  each step of a randomly drawn sequence (`b14`), cell ↔ text translation
  (`b15`), series with columns of unequal lengths (`b16`), cost of operations
  as the project grows (`b17`), what remains possible while a macro is
  open (`b18`), file written by a newer version (`b19`), two
  writers on the same file (`b20`), cost and identity on the write
  path (`b21`), targeted recalculation of formulas against full recalculation
  (`b22`, see "Targeted recalculation (#193) — and what to test when relying on it" below),
  then the same oracle for structural gestures (renames, moves, removals,
  pastes, transposes, column type changes) plus an independent re-evaluation
  of every formula from a saved and reloaded copy (`b23`, #412), then every
  analysis operation on hostile series (`b24`, #522: `NaN`, infinities, `1e300`,
  subnormals, X offset by `1e6`, duplicate or unsorted X, zero to two points;
  `run` never panics, and a ready and valid operation is not refused as an
  invalid parameter on clean data);
- **two out-of-process probes**, `alloc_probe` and `stack_probe`, because the
  cases they test end with an `abort()` that nothing catches:
  a Rust allocation failure is not a catchable panic, and a stack
  overflow is not either (the runtime writes `has overflowed its stack` and
  calls `abort()` — neither `catch_unwind` nor `join` gives control back, and the
  test binary would die with it).

`stack_probe` uses a 2 MiB stack by default: the size an
`std::thread` gets when nobody sets it, that is, that of the actor thread.

### Two natures of test, and how to tell them apart

**The guards** pass and must keep passing: they go into
`cargo test --workspace` like the others.

**The reproductions** fail deliberately, and are marked `#[ignore]` so
that the pipeline keeps its value as a signal — a permanently red CI says
nothing anymore. A reproduction that starts to **pass** is the expected signal: the
fix is good, and it is time to remove its `#[ignore]` so that the
guard takes over.

The correction campaign that came from the reports of 2026-09-11 is closed: no
`#[ignore]` remains in `stokesforge-robustness`. The last one, in `b9`
(`b9_race_between_script_and_interface_on_the_length`), came from two defects of the
test: the script did not catch `ConcurrentWriteError` (the contract refuses
the write of a column changed since the read), and the "interface" ignored
that the actor refuses its commands while the script's macro is open
(`ForeignMacro`), hence a drift of 999 rows on Linux CI. The oracle now counts
what the interface really succeeded in doing (#333, "zero red" criterion).

Measure rather than time: `b17` and `b21` count **allocated bytes**,
not milliseconds, so that the assertion says "this cost must not
grow with X" and stays true on any machine.
Accepted exception (#316): `b17_loading_a_folder_with_many_children_is_not_quadratic`
bounds a time (10 s, very wide) because a quadratic CPU cost allocates nothing.
The refusals of `MAX_COLUMN_ROWS` on reading are tested with `check_rows` and tiny
limits (`convert.rs`), never with 10^8 rows.

### Slow tests (#412)

A test that takes more than a couple of seconds in debug lives in a `mod slow`
of its file, so that its path starts with `slow::`: `cargo test -- --skip slow::`
leaves them out, `cargo test slow::` runs only them. This prefix is THE marker
(no other name, no `#[ignore]`: a slow test still runs in CI); it prepares a
fast/slow split of the CI jobs. Today: the two long-recalculation gestures of
`stokesforge-app-state/tests/formula_gestures.rs` (80 000 rows x 150 formula
columns, a 2.5 s recalculation, i.e. 12 times `RECALC_WAIT`; they assert that
the recalculation is still running, which is their precondition).

## Targeted recalculation (#193) — and what to test when relying on it

Since #193, the actor rewrites a formula column only if a piece of data of its
calculation has changed (`stokesforge-session/src/recalc.rs`, [`session.md`](session.md)).
This kind of optimization has only one failure mode, and it is
**silent**: one piece of data forgotten in the fingerprint, and the displayed value
stays wrong without any error. The tests below are made for that
case, and everything that reuses the mechanism must rely on them (recomputed
analyses #184, references between sheets #186,
new formula functions). A **row-wise** function (`sqrt`, `round`, `if`, #401)
adds no input to the fingerprint: it reads only the columns it names, which are
already in it (`referenced_columns` walks every node; `b22` draws such
formulas). A function that reads something else (another sheet, the row
number) does. A whole-column function (#402) reads the columns it names **and**
the X paired with them (`at`, `integral`, range forms): that X is in the
fingerprint (identifier and version) and in the dependency order; the
named cases are in `recalc_targeted.rs` (`moving_the_paired_x_recalculates_at`,
`a_role_change_pairing_another_x_recalculates_at`,
`editing_the_unnamed_x_recalculates_at`,
`an_x_computed_by_a_formula_is_recalculated_first`,
`an_x_reading_a_column_it_is_the_x_of_is_a_cycle`,
`an_unrelated_command_rewrites_no_column_function`) and `b22` draws the column
functions with a `SetRole` step. A formula that reads **another sheet or
workbook** (#403) adds the remote column, its paired X and their membership in
the sheet they must be in: named cases in `recalc_targeted.rs`
(`editing_the_remote_column_recalculates_the_reader`,
`a_role_change_in_the_remote_sheet_recalculates_the_reader`,
`deleting_the_remote_sheet_gives_ref_then_undo_restores_the_values`,
`moving_the_remote_column_to_a_third_sheet_gives_ref`,
`a_chain_across_sheets_is_calculated_in_one_pass`,
`a_loop_across_sheets_stays_not_calculated_and_stable`,
`the_result_keeps_the_length_of_its_own_sheet`), `analysis.rs` (a loop through a
calculation and a remote formula), the `stokesforge-core/src/sheet_refs.rs`
tests (syntax, quotes, lookup, names, loops) and `b22`, which draws remote
formulas, and saves and reloads them through the `.stf` side field.

### What holds it today

| Test | What it holds |
|---|---|
| `stokesforge-robustness/tests/b22_targeted_recalc.rs` | **the oracle**: two twin sessions, one targeted, the other in full recalculation (`FormulaRecalc::Full`, the code from before #193), receive the same randomly drawn steps (transposes and their refresh included, #412; since #439 the fixture holds a sheet with a **formula Y column that has an x'**, and a sheet holding one can be transposed); same answers required after each step and the trees **compared exactly, transposed links and column versions included** (the twins number alike since #439), plus their undo histories. Plus, at each step: the version ↔ content doctrine **in the actor** (recalculation writes, macros, reloads included), and the "targeted" witness — an unrelated command evaluates no formula column (`Snapshot::formula_evaluations`, #439: a pass that finds the same content keeps the version, so the version no longer shows an evaluation) and changes the version of no column, while the full twin evaluates every formula column (the witness of the counter, with a coverage count). Since #433 the formula step stores what `Tree::formula_keeping` reads (nothing is refused), a step keeps `A+<next letter>` (`SetPending`), and after **every** step, undo and redo included, `Tree::pending_bindings` must be empty (the fixed point of the binder, invariant 11), with a coverage count of bindings. The named case `b22_an_abort_over_a_retyped_formula_column_leaves_nothing_for_the_next_pass` (#448) plays the minimal sequence the fixed draw never reaches, so property 3 judges the step after an abort over a retyped formula column. Since #463 the draw also toggles the box of a linked sheet (`ToggleAuto`), edits a Y column of a source (`EditSource`) and types in a linked column (`TypeInRow`): the twins must agree while sheets follow their source, with a coverage count of follows (`>= 20`). A final coverage check requires formulas that are computed, not computed and spared, and **at least twenty passes per kind of step**: a gesture that is always refused would prove nothing. `spared` is counted from the evaluation counter (formula columns present minus those the pass evaluated). The draw is replayable identically: no target is chosen in the order of `Tree::iter` (hash table with a random seed) |
| `stokesforge-session/tests/recalc_targeted.rs` | one named case per component of the fingerprint (stored formula, sheet membership and version of the sources, length of the result, own version, outcome), the gestures that make them change (move within the sheet or to another, deletion then undo, cycle created elsewhere, deletion of cells in a formula column, aborted macro, a macro or plain undo/redo replayed over a derived column that a pass retyped or resized (#448, in both modes), duplicated sheet…), stability of a chain and of a cycle, forgetting of the memo by `Load` and `NewProject` on a coincidence of identifiers and versions that is **manufactured and checked**. Each "not rewritten" assertion is proved by the count of formula columns evaluated (`Snapshot::formula_evaluations`, #439: 0 for an unrelated command in targeted mode, more in full mode as the witness) and keeps its version assertion; #439 adds the cases of the pass keeping or restoring a number (`a_formula_column_…`, formula recomputed to the same numbers, zero to negative zero) |
| `stokesforge-session/src/lib.rs` (`tests`), `src/recalc.rs` (`tests`) | interruptible recalculation (#239): Stop cancels the command and allows redo (stop set in advance on the next recalculation, hence deterministic), Stop after a command **without effect** undoes nothing (`History::execute_recorded`), the death of the actor (a revert that fails over an ordinary column retyped by `Request::EditForTest`: the revision advances and the pass runs, #448; panic simulated by `Request::PanicForTest`, or in the middle of a recalculation) frees the gauge and makes `actor_alive` false (#310), late Stop without effect on the next one, stop between two columns then next pass that finishes without rewriting what is already written. `stokesforge-app-state/tests/shell.rs` (`heavy`): the same gesture seen from the interface on a heavy project, with its witness without Stop, and the question "save?" asked at close during the recalculation |
| Axis direction and axis settings (#344) | `AxisSpec::orient`: table (auto or explicit × inverted × order of the bounds, bounds missing or not finite, idempotence) in `stokesforge-plot/src/axis.rs`; identical rendering before and after `orient` (`an_oriented_axis_is_drawn_like_the_original_axis`, `stokesforge-render/tests/geometry.rs`); opening a file with `inverted` and bounds (-1, 10): bounds (10, -1), `inverted` true, empty history, file untouched (`load_swaps_the_bounds_of_an_inverted_axis_without_history`, `stokesforge-session/tests/session.rs`); `pyst.set_graph` (`set_graph_orients_the_axes_from_the_order_of_the_bounds`, `stokesforge-script/tests/set_graph.rs`); axes page (`stokesforge-app/src/app/plot_settings.rs`, module `tests`): no more "Inverted axis" / "Auto range" / "Closed frame" boxes, "Advanced" block closed then opened on click (title or arrow), input of a bound under Auto = two bounds fixed in **one** history entry, From > To inverts the axis, Auto button = data while keeping the direction, From shows the high bound of an inverted axis; proven mutants (`orient` without swapping the bounds, `load_project` without `orient_graphs`, input that does not fix, Auto button that loses the direction, click on the arrow of "Advanced" counted twice) |
| `stokesforge-session/tests/analysis.rs` (`on_load_no_calculation_is_rerun_and_the_fingerprint_says_whether_it_is_stale`, `trust_on_load_applies_only_to_the_load`) | A1 (#316): on opening no calculation (automatic or manual) is rerun, in both modes (`Full` as well as `Targeted`, otherwise the oracle diverges); input modified outside the application (file bytes) → `Manual` and file values kept, then `recalculate` makes them right; intact file → up to date; the `trust_loaded` flag is single-use (a calculation without a memo after `AddCalculation` is still recalculated). Mutants: flag removed, fingerprint comparison always true, always false Also #448: undoing a deletion, or aborting a macro, over an output column the recalculation shortened, in both modes (`undoing_a_deletion_over_an_output_the_rerun_shortened`, `aborting_a_macro_over_an_automatic_output_the_pass_shortened`), the "Out of date" mark of a manual calculation kept across a further undo and cleared by Recalculate, and the box ON staying up to date (`undoing_over_an_automatic_output_the_pass_shortened_stays_up_to_date`); `stokesforge-commands/tests/calculations.rs` (`a_misfit_marks_only_a_manual_calculation_and_keeps_a_failure`) holds the rule at History level. |
| `stokesforge-session/tests/formula.rs` | the behavior of formulas itself (before #193, unchanged) |
| `stokesforge-robustness/tests/b23_formula_structure_oracle.rs` | the `b22` oracle for the gestures `b22` does not draw (renames, moves, removals, object pastes, transposed sheets, column type changes), plus an independent re-evaluation of every formula in a fresh `Full` session after save and load. Since #439 a sheet with a formula Y column that has an x' can be transposed, the trees are compared exactly (links and versions included) with the histories, and coverage counts require a reload over an up-to-date link over a formula column and a Transpose / RecalculateTranspose over it that stores no entry (`cases >= 64`). Since #433 it types formulas through `Tree::formula_keeping`, draws `SetPending`, `AddColumn` and `MoveColumn`, and asserts the fixed point of the binder (`pending_bindings` empty) after every step, reload included, with a coverage count of bindings. Since #493 sheets with no x' at all (index mode) can be transposed too (listed after the others, 96 cases by default so the binder count keeps its floor). Since #463 it draws `ToggleAuto`, `EditSource` and `TypeInRow` too, and counts the follows of checked sheets (`>= 20`) and the typed values that stop one (`cases >= 64`) |
| `stokesforge-core/tests/formula_evaluation.rs` | the evaluator against a reference evaluator (operators, functions, literals, numeric precision; seeded proptests); `logx` of exact powers and of values close to one (#429); number literals out of the `f64` range refused at entry (`1e400`, `1e-400`, a 400-digit integer) while subnormals, written zeros and `f64::MAX` pass and a stored formula holding `1e400` still compiles (witnesses: no check, subnormals or every zero refused, check moved into `tokenize_text`); the shared check `number_format::check_range` (unit tests; witnesses: comma not normalised, `abs() < MIN_POSITIVE`, mantissa = whole text, every zero refused, digit guard removed so `inf`/`nan` are refused); a data cell typed by hand refuses the same numbers (`stokesforge-app-state/tests/shell.rs`, `cell_edit_refusal`: Numeric, Mixed and Date/Time cells, a stored text `1e400` reopened passes, a pasted text block holding such a number is refused as a whole (`external_paste_range_error`: Numeric and Mixed, a last cell of a second column, the entry row, `inf`/`0e400`/subnormals accepted, a date in a Date/Time column accepted, an internal copy of a stored text `1e400` still pastes; witnesses: no scan, scan limited to Numeric, scan of the first column only, own range check instead of `check_range`), no row added from the last row or the entry row; witnesses: no check in `commit_cell`, check limited to Numeric, comparison with the displayed text dropped, check moved into `ValueBlock::from_texts`) and `grid.rs` pins the tint, the open cell and the unchanged entry row; `stokesforge-app-state/tests/formula_gestures.rs` pins the sentence and the open field |
| `stokesforge-core/tests/formula_column_functions.rs` | `at`, `max`, `min`, `mean`, `sum`, `std`, `integral` and their range forms; NIST StRD certified values (`tests/data/nist-strd/`, US government work, public domain, unmodified); a `bench:` of per-row bounds; infinite bounds read "to the end" and `integral` over equal finite bounds is 0, on literal and column bounds (#433) |
| `stokesforge-analysis/tests/erf_boost.rs` | `erf` and `erfc` (`libm`, #429) against every row of the Boost.Math tables `erf_data.ipp`, `erf_small_data.ipp`, `erf_large_data.ipp` (`tests/data/boost-erf/`, tag `boost-1.87.0`, Boost Software License 1.0, unmodified, licence file and README next to them), parsed from the C++ text; ordered-bits ulp distance, subnormals included, at most 2 ulps (observed worst: `erf` 1, `erfc` 2). Witnesses: the former series (`erfc = 1 - erf` below 2.5, Maclaurin `erf`) fails it with up to 97 976 ulps |
| `stokesforge-session/tests/formula_structure.rs` | formulas across structural gestures (copy, paste, transpose, type change, removal, move, rename, cycles, row edits, reloads), in `Targeted` and `Full`; the one-row floor of a formula that names no column of its sheet, and the same-sheet copy of a lone formula column (#433) |
| `stokesforge-app-state/tests/formula_gestures.rs` | formula gestures seen from the application state (full-column paste, header paste, undo, long recalculation) |
| `stokesforge-session/tests/formula_witness.rs` | golden test (#412): the V1 `.stf` witness (`stokesforge-persistence/tests/witnesses/v1/project.stf`, sheet `Formules`: one formula column per family) is loaded, recalculated in `Targeted` and `Full`, and its **literal** formula texts and **literal** values (computed by hand, compared with `==`) are checked. Witnesses: a renamed function in `functions.rs`, a changed text in `witness_project()` (regenerated), a left-associative `^` each fail it. `at` on a grid point (#427) is pinned in `formula_column_functions.rs`. `round` ties (#428) are pinned in `formula_evaluation.rs` and against Python's `decimal`. `logx` of powers, `erf` and `mean` of 0.1s (#429) are pinned in `formula_evaluation.rs`, `erf_boost.rs` and `formula_column_functions.rs` |
| `stokesforge-commands/tests/undo_random.rs` | the undo/redo round trip also draws `SetFormula` and `RemoveFormula` steps (`SetColumnMeta`); a coverage count requires formulas to be really set. Witness: skipping `set_column_meta` in the undo arm of `DataDelta::ColumnMeta` when the meta before had no formula |
| `stokesforge-persistence/tests/formula_roundtrip.rs` | `.stf` round trip of column formulas, local and remote |
| `stokesforge-script/tests/formulas.rs` | column formulas through `pyst` (feature `python`, run by `scripts/python-test.sh`); `round` against Python's `decimal` (`Decimal(repr(x))` quantized half up, exact equality, #428); differential check of the numeric functions against Python's `math` and `statistics`, one ULP tolerance per function (4 ulps for the transcendental ones, exact for `sqrt`, `floor`, `ceil`, `sign`, `mod`); `erf`/`erfc` relative error and `sum`/`mean`/`std` at 2 ulps are held by `erf_erfc_and_the_aggregates_meet_their_target_tolerance` (#429). Witness: `atan` multiplied by `1 + 8 epsilon` fails it |
| `stokesforge-core/src/sheet_refs.rs` (`tests`), `stokesforge-app-state/tests/formula_sheets.rs`, `stokesforge-persistence/tests/remote_formula.rs` | formulas that read other sheets (#403, batch 1): reading `Sheet3!B` / `[Book2]Sheet5!B`, case, quotes, workbook lookup (own folder, then project, path when ambiguous), the refusals (outside a column function, remote X, workbook without sheet), sibling names that differ only by case, loops (local, remote, through a paired X) refused at typing and framed when a file holds one; a remote formula saved and reloaded keeps its sheet (`formula_remote`, batch 2), forged files (sheet id beyond the bound refused, missing or non-sheet target and bad part index degrade to `#REF`), a project without remote reference writes no such field, and the object clipboard keeps or voids the reference (`project_objects.rs`) |
| `stokesforge-robustness/tests/b14_sweep.rs` | the version ↔ content doctrine on `History` alone, on which the whole mechanism rests — including for a column added with a number already used |
| `stokesforge-core` (`project.rs`, `an_inserted_column_moves_the_counter_past_its_version`) | inserting a column moves the counter beyond its version |

Ignored tests state why in their reason string: `#[ignore = "bug: #N …"]` is a
known bug (the issue number is the handle; lift the ignore when it is fixed),
`#[ignore = "design: #433 point N …"]` is behavior awaiting a decision, and
`#[ignore = "bench: …"]` is a timing bench run by hand, `#[ignore = "manual: …"]` is a check that needs a real compositor or device (run by hand, see its row above). The proptests of these
files use a fixed seed (`rng_seed: Fixed(412)`) so that a failure replays.

The oracle does not see an error **common** to both modes (calculation order,
evaluation, output type): they share all the code except the decision
"clean / to recalculate". The named cases and `formula.rs` guard
the calculation itself.

### Proven mutants (#193)

Each component was broken in turn by a temporary mutation of the
code, removed before delivery; the table says what was broken and which
tests see it.

| Mutant (what is broken) | Seen by |
|---|---|
| formula text removed from the fingerprint | `changing_the_formula_text_recalculates`, `b22` |
| length of the result removed (revised by #194: for a computed formula, it follows from the versions of the sources; it matters only for the other outcomes and constants) | `a_cycle_follows_the_length_of_the_sheet`, `a_constant_follows_the_ordinary_columns`, `an_unreadable_formula_follows_the_sheet_then_gets_corrected`, `b22` |
| length removed for a blocked formula only | `a_cycle_follows_the_length_of_the_sheet`, `b22` |
| own version ignored (on both sides) | `a_formula_column_modified_directly_is_recalculated`, `a_blocked_or_unreadable_formula_modified_directly_is_rewritten`, `b22` |
| own version ignored for a blocked, or unreadable, formula only | `a_blocked_or_unreadable_formula_modified_directly_is_rewritten`, `b22` |
| own version noted **before** the write, or never | the "targeted" cases (`an_unrelated_command_rewrites_no_formula`, `a_recalculated_chain_is_stable_at_the_next_command`, `a_stable_cycle_is_not_rewritten`…), `b22` (property 3) |
| versions of the sources removed | eight named cases (propagation, aborted macro, promoted source…), `b22` |
| resolution of identifiers **by position** instead of by identifier (#321, bug #320) | `moving_a_source_within_the_sheet_the_formula_follows`, `an_analysis_that_inserts_its_outputs_does_not_change_what_a_formula_reads` |
| sheet membership missing from the fingerprint (the version of a column that is read is taken in the whole tree) | `moving_a_source_to_another_sheet` — not `b22` |
| an automatic `Manual` calculation read from a file is recalculated by full mode (`Manual` arm of `update_calculation`) | `an_automatic_calculation_stale_on_load_waits_for_a_gesture_even_in_full_mode`, `b22`; the **content** comparison (`Renumber`): `a_stale_automatic_calculation_reading_a_formula_stays_stale_in_full_mode` |
| memo not cleared by `Load` | `loading_forgets_the_memo_of_the_replaced_project` alone — the draw does not manufacture a coincidence between projects |
| memo not cleared by `NewProject` | `a_new_project_forgets_the_memo_of_the_old_one` alone (a paste as first request: otherwise the purge of an ordinary pass is enough) |
| insertion of a column without moving the counter beyond its version (`Project::insert_node`) | `a_column_added_with_its_version_does_not_make_that_number_serve_again`, the unit test of `core`, `b14` — not `b22`, whose draw does not land on the number |
| version of the paired X removed from the fingerprint (#402) | `editing_the_unnamed_x_recalculates_at`, `an_x_computed_by_a_formula_is_recalculated_first`, `b22` |
| paired X removed from the fingerprint altogether (#402) | `moving_the_paired_x_recalculates_at`, `a_role_change_pairing_another_x_recalculates_at`, `editing_the_unnamed_x_recalculates_at`, `an_x_computed_by_a_formula_is_recalculated_first`, `a_column_without_x_shows_dashes_until_one_appears` |
| identifier of the paired X removed, version kept (#402) | **nobody, by construction**: a version names one content in the whole project, so a different X has a different version. The identifier stays for readability |
| paired X missing from the dependency order and from the cycles (#402) | `an_x_computed_by_a_formula_is_recalculated_first`, `an_x_reading_a_column_it_is_the_x_of_is_a_cycle` — not `b22` (a chain through a formula X is too rare in the draw) |
| remote column or paired X missing from the fingerprint (#403) | `editing_the_remote_column_recalculates_the_reader`, `a_role_change_in_the_remote_sheet_recalculates_the_reader`, `b22` |
| membership read as "exists in the tree" instead of "is in the sheet it must be in" (#403) | `moving_the_remote_column_to_a_third_sheet_gives_ref`, `a_deleted_sheet_or_a_moved_column_shows_ref` |
| one order per sheet instead of one for the project (#403) | `a_chain_across_sheets_is_calculated_in_one_pass` |
| a remote column counted in the length of the result (#403) | `the_result_keeps_the_length_of_its_own_sheet` |
| cycles and calculations kept to the own sheet (#403) | `a_loop_across_sheets_stays_not_calculated_and_stable`, `a_loop_through_a_calculation_and_a_remote_formula_waits_for_a_gesture`, `loops_are_found_project_wide` |
| loop check without the paired X (#403) | `a_loop_through_a_paired_x_is_found` |
| kept refusal not tied to the revision (#403) | `the_kept_refusal_follows_the_project` |
| copy renumbers a remote reference like a local one, or keeps it on an external paste (#403) | `duplicating_a_sheet_keeps_its_remote_references`, `duplicating_a_workbook_makes_the_copy_read_its_own_sheet`, `an_external_paste_turns_a_remote_reference_into_ref` |
| siblings compared exactly instead of ignoring case, or a name generator that does not skip them (#403) | `siblings_may_not_differ_only_by_case`, `automatic_names_skip_a_name_taken_up_to_case` |
| name generator that ignores a sibling folder (`Book1`, `Graph1`, `MBook1`, `Note1`), so an unnamed creation fails with `DuplicateName` (#437) | `generator_skips_a_sibling_folder`, `generator_skips_a_sibling_folder_ignoring_case`, `generator_skips_a_window_and_a_sibling_folder_together`, `generator_skips_a_folder_in_the_target_parent_for_graphs`, `the_automatic_name_generator_skips_a_sibling_folder_of_the_same_name` (`b11`), `unnamed_matrix_and_note_skip_a_sibling_folder_of_the_same_name`, `an_unnamed_workbook_skips_a_sibling_folder_without_error`, `an_unnamed_graph_skips_a_sibling_folder_without_error`, `an_external_graph_paste_skips_a_sibling_folder_for_its_hidden_workbook` |
| name generator scoped to the wrong parent (the root, or the whole project, instead of the target parent) (#437) | `generator_ignores_a_folder_of_another_parent`, `generator_is_scoped_to_the_target_parent`, `generator_still_skips_a_window_of_another_parent`, `a_folder_of_another_parent_does_not_shift_the_generated_number` (`b11`), `an_unnamed_graph_in_the_selected_folder_skips_the_folder_it_holds` |
| name generator that panics on an unknown parent (#437) | `generator_with_a_removed_parent_fails_without_panicking` |
| recalculate everything (the "targeted" is no longer targeted) | the "targeted" cases and their witnesses (count of evaluated columns, #439), `b22` (property 3, by the count) |
| presence in the memo instead of fingerprint equality | fifteen named cases, `b22` |
| self-reference | refused on input (`Tree::formula_from_letters`: `SelfReference`, test of `core`); a fabricated file that carries it ends up in the cycle, never in `evaluate_formula` |
| memo purge removed | **nobody, by construction**: an identifier is never reassigned in a project, and `forget` covers replacement. Memory hygiene |

A first version put in the fingerprint of a blocked formula the
columns it names: no mutant saw it missing, and for good reason,
a `NaN` block does not read them. It was removed — that is the use of this
table: a component that no test sees is dead or not covered.
The critical review of #193 (independent agent) added three lines: the
"per outcome" components, which only `b22` saw, received their named
cases, and the insertion of a numbered column, a real latent false "clean",
was fixed in `core`.

### Proven mutants (#194, result length)

#194 changed **the calculation**, not the "clean / to recalculate" decision:
a formula has the length of the longest column it reads, failing that
that of the longest ordinary column. These mutants touch code
common to both modes: `b22` cannot see them, and does not. Only
the named cases hold them — every new calculation rule must therefore have
its own.

| Mutant (what is broken) | Seen by |
|---|---|
| back to the length of the sheet (all columns together) | `the_result_follows_the_length_of_the_columns_read`, `lengthening_an_unread_column_does_not_change_the_formula`, `moving_a_source_to_another_sheet`, `a_formula_column_modified_directly_is_recalculated`, `an_unreadable_formula_follows_the_sheet_then_gets_corrected` — not `b22` |
| columns read ignored (always the length of the ordinary ones) | `the_result_follows_the_length_of_the_columns_read`, `lengthening_an_unread_column_does_not_change_the_formula`, `moving_a_source_to_another_sheet`, `an_unreadable_formula_follows_the_sheet_then_gets_corrected` — not `b22` |
| formula columns counted in the fallback length | `a_constant_follows_the_ordinary_columns`, `an_unreadable_formula_follows_the_sheet_then_gets_corrected` — not `b22` |

### Reliability of fits (#203, #205)

`stokesforge-analysis/tests/undetermined.rs`: parameter without effect (warns, the
others keep their uncertainty), X all equal (slope undetermined), no determined
parameter (`SingularSystem`), well-posed fit (no
warning), line with empty or zero σ discarded and counted.

### Fits on a shifted X (#282, #293, #294)

`stokesforge-analysis/tests/x_offset.rs`: free line `a + b*x` and Gaussian
on an X shifted from 1e3 to 1e10, compared with the `Linear` model and the centered frame
(values **and** uncertainties). Witnesses by mutation: without refinement of the
difference step, `a_gaussian_offset_by_one_million_recovers_its_parameters` and
`a_noisy_gaussian_offset_by_one_million_has_the_centerd_frame_uncertainties`
fail; without the undamped verification step,
`a_tiny_damped_step_is_not_a_convergence` (`solver.rs`) and
`beyond_that_the_result_is_the_linear_model_one` fail; with the old
indetermination threshold (1e-6), `the_issue_case_gives_the_linear_model_uncertainties`
fails. Beyond X ≈ 3e7, the slope of a free line is declared undetermined
(uncertainty omitted, warning): the noise of the finite differences (≈ 2e-10)
no longer separates the weak direction there.

### Numerical determinism of analysis (#332)

`stokesforge-analysis/tests/determinism.rs`: results frozen **to the bit** (`to_bits()`) for all platforms, because the transcendental functions go through the `libm` crate; Linux CI confirms them. Peak decomposition of the series in steps of five rows (20 values, R² included); the sawtooth of #290 converges on no platform (`NotConverged { iterations: 400 }`); five expression values (`exp`, `ln`, `sin`, `pow`, `atan2`) and a sweep of 256 abscissas per function (FNV-1a fingerprint of the bits). Guard: `crates/stokesforge-analysis/clippy.toml` (`disallowed-methods`, checked by `cargo clippy -p stokesforge-analysis --all-targets -- -D warnings`). Proven mutants: `x.exp()` reintroduced in `peaks.rs` (clippy fails); `Func::Exp` computed by `exp2(a * LOG2_E)` (the five isolated values did not see it, the sweep did: hence the sweep).

### Bounds, prerequisites and settings of analysis (#260)

`stokesforge-analysis/tests/limits.rs`: `Model::compile` refuses a degree beyond
the cap (`u32::MAX`) without allocating anything (the test runs under `ulimit -v` to
see the failure of a `compile` without a cap rather than a hang); `MAX_*` and
their edges in `validate` (100 peaks, not 50); `validate` refuses a fit
without a free parameter (none, or all fixed); `Operation::readiness`
has one case per variant (`NeedsPoints` with its `have`, `NeedsZone`,
`BadExpression` (also a number literal out of the `f64` range, #438: checked before
`NoParameter`, absent from `validate` and `compile` so stored models keep running), `NoParameter`, `Invalid`); `forget_positions` erases
exactly ranges, positions, `initial`/`fixed`/`min`/`max`/`shared`, reconvolution
window, and keeps the method; `FitSpec::auto_initial` filters the range
(bounds in disorder), ignores fixed parameters and unreadable models.
`stokesforge-app`: `number_field.rs` (the "Max peaks" field goes up to 100 and
each analysis quantity follows the constant of the crate), `tests/analysis.rs` (single point
or zone missing, huge degree = prerequisite refused, "Auto" reads the
range), `analysis_recent.rs` (reopening forgets `fixed`/`shared`, a huge
degree in the preferences fabricates nothing).

| Mutant (what is broken) | Seen by |
|---|---|
| `compile` without a cap | `compile_refuses_a_degree_beyond_the_cap_without_allocating` |
| `readiness` forgets `NeedsZone` | `readiness_says_what_a_baseline_lacks` |
| `forget_positions` keeps `shared` | `forget_positions_clears_the_ranges_and_the_per_parameter_settings`, `reopening_an_entry_also_forgets_fixed_and_shared_settings` |
| `MAX_PEAKS` at 50 | `the_caps_are_those_of_the_quantities_table`, `analysis_fields_follow_the_caps_of_the_analysis_crate` |
| `auto_initial` without range filter | `auto_initial_follows_the_model_on_the_points_of_the_range`, `auto_reads_only_the_fitted_range` |
| `validate` without "zero free parameters" | `validate_refuses_a_fit_without_a_free_parameter` |
| `validate` lets a non-finite range, seed, prominence, shift or window through (#278) | `validate_refuses_a_non_finite_range`, `validate_refuses_a_non_finite_seed_or_prominence`, `validate_refuses_a_non_finite_shift_or_window`, `validate_refuses_each_non_finite_field_and_accepts_the_finite_one_that_round_trips` (field-by-field table, NaN/±inf, round trip of the finite value) |
| `add_/update_calculation` without `Operation::validate` (#278) | `a_non_finite_operation_is_refused_on_add_and_on_update` (core), `every_non_finite_operation_is_refused_without_touching_the_tree` (twelve families, add/update/replace, tree and identifiers intact), `the_session_refuses_a_non_finite_operation_without_changing_anything` (session) |
| unreadable calculation discarded without a report (#278) | `an_unreadable_calculation_is_named_by_the_report_and_the_rest_loads` (persistence), `loading_a_file_with_an_unreadable_calculation_warns` (app-state), `several_discarded_calculations_are_all_listed_in_order_and_the_sound_ones_load`, `a_file_without_a_discarded_calculation_gives_an_empty_report`, `an_identifier_out_of_bounds_discards_the_calculation` (bound, all id fields), `loading_relays_the_calculations_dropped_by_persistence` (session), `the_discarded_calculations_warning_counts_and_abbreviates` (singular/plural, 5 ids then "…", fr/en), `an_entry_with_a_non_finite_number_is_never_read_back_non_finite` (preferences); #309: `an_invalid_shape_discards_and_names_the_calculation`, `a_discarded_trace_is_counted`, `an_unreadable_correction_trace_is_counted_by_the_report` (persistence), `loading_relays_the_dropped_correction_traces` (session), `mark_loaded_warns_about_the_calculations_discarded_by_the_report` (app-state), `a_duplicate_identifier_keeps_the_first_and_counts_the_second` (persistence, F4), `the_report_counts_the_records_and_the_calculations_in_a_single_message` (F1), `run_project_warns_about_discards_then_runs_the_script` (CLI, feature python, F2), confirmation on save (F3, app-state `shell.rs`): `saving_to_the_source_with_discards_asks_then_writes`, `refusing_the_overwrite_writes_nothing_and_forgets_the_gesture`, `quit_save_accept_saves_then_quits`, `save_as_elsewhere_asks_nothing_but_going_back_to_the_source_does`, `a_clean_file_or_a_new_project_opens_no_question`, `the_startup_load_sets_the_source_mark`, `startup_returns_the_report_of_discarded_calculations` (main); `a_nan_edited_by_hand_in_the_file_discards_and_reports_the_calculation` (persistence, #291: `validate` called on load) |

### Out-of-date series on the graph (#198)

`stokesforge-core/tests/calculations.rs` (`une_serie_tracee_depuis_un_calcul_perime…`:
flag set, cleared on recalculation, turned off by `stale_warning`);
`stokesforge-persistence/tests/calculations.rs` and `schema.rs` (round trip of the
setting, old file = active); `stokesforge-app/src/app/stale_export.rs`
(the four answers of the confirmation, SVG greyed `#9a9a9a` or not, graph without
warning exported without asking, Object Manager row marked).

### Constant baseline (#211)

`stokesforge-analysis/src/baseline.rs` and `tests/ops.rs` compare the free
point with the average of a zone, keep an exactly zero slope, check
the inclusive bounds, the discarded rows and the division by zero. Non-finite
settings and the zone without data are refused.
`stokesforge-app-state/tests/analysis.rs` checks the single point and zone,
their handles and the reopening of the calculation;
`stokesforge-persistence/tests/calculations.rs` goes through the `.stf` with the
two variants. `stokesforge-script/tests/analysis.rs` exercises the same
methods through `pyst`, their refusals and the cancelation of the script (feature `python`).

### Proven mutants (#181, analysis calculations)

Same mechanism, another memo (`CalcFingerprint`, `CalcMemo`). `b22` plays the steps
`AjouterCalcul` (fit or correction, automatic or manual), `RetirerCalcul`
and `RecalculerPerimes` next to the formulas; `stokesforge-session/tests/analysis.rs`
names the behaviors.

| Mutant (what is broken) | Seen by |
|---|---|
| versions of the inputs removed from the fingerprint of an automatic calculation | `b22`, `an_automatic_calculation_follows_its_inputs` |
| manual: comparison by content (`seen`) removed, at either call site | `retyping_the_same_values_in_an_input_leaves_a_calculation_as_it_was` alone (#439: `b22` no longer sees it, a pass keeps the version of unchanged content) |
| guard of the calculation → formula → calculation loops removed | `b22` (property 3: an unrelated command rewrites or evaluates columns) |
| manual: replaced calculation record not "trusted" (`Trust`, `CalcMemo::record`) | `editing_a_stale_manual_calculation_updates_it_without_making_it_stale_again`, `undoing_and_redoing_an_update_of_a_stale_manual_calculation_follows_the_gesture`, `an_update_with_the_same_recipe_after_an_edit_that_left_the_outputs_unchanged_clears_the_mark` |
| manual: a change of the output versions read as a gesture (#458) | `deleting_a_row_marks_a_manual_calculation_and_keeps_its_values`, `inserting_a_row_marks_a_manual_calculation`, the macro, undo/redo and save/load cases of the same section |
| manual: mark cleared on renumbering, or `Failed` overwritten by `Manual` | `undo_and_redo_of_a_row_deletion_leave_the_mark_until_recalculate`, `a_failed_manual_calculation_stays_failed_when_a_row_is_deleted` |
| manual: a mark on any move of an output version (row removed only from outputs moved to another sheet, decision 458-2) | `a_row_removed_only_from_the_moved_outputs_of_a_manual_calculation_does_not_mark_it` |
| output versions removed from `CalcDefinition` (automatic) | `a_row_removed_only_from_the_moved_outputs_of_an_automatic_calculation_recomputes_them` |

### Proven mutants (#313, analysis warnings and errors)

Pure calculation, outside the fingerprint: the warnings come from `ops::run`, which
targeted and full recalculation call identically (`b22` and `b19` stay
green). The cases are named in `stokesforge-analysis` (`solver.rs`, `deconv.rs`,
`correct.rs` in `mod tests`, `tests/plateau.rs`, `tests/ops.rs`), `stokesforge-core/tests/calculations.rs`
(C4) and the three transport tables of `Warning` (persistence, `analysis_dialog.rs`,
`analysis_warning_text.rs`).

| Mutant (what is broken) | Seen by |
|---|---|
| stop without an accepted step always "converged" (`stalled` never raised) | `a_solver_with_no_accepted_step_and_a_steep_slope_is_flagged_stalled`, `a_solver_stuck_on_a_sharp_point_warns_not_converged` |
| non-finite test removed before the `max` fold (slope `NaN` read as "converged") | `a_non_finite_slope_is_a_non_evaluable_model_error` (solver and `tests/plateau.rs`), `tiny_or_non_finite_imposed_width_is_not_judged_by_decompose` |
| non-finite derivative reset to 0 ("no effect" box) | same cases (the error becomes `SingularSystem` again) |
| exact-fit threshold removed (cosine read on rounding noise) | `a_sound_or_noisy_fit_does_not_warn` (`tests/undetermined.rs`) |
| cut IRF: 1% threshold moved (0.5% or 2%), edges or points outside the window ignored | `an_irf_cut_above_one_percent_of_its_peak_is_reported` (0.5 / 0.9 / 1.1 / 5%), `an_irf_point_outside_the_window_is_reported_depending_on_the_shift`, `direct_deconvolution_reports_a_cut_irf_and_keeps_the_renormalization` |
| hole in the reference: count removed, point exactly on a kept point counted | `a_hole_in_the_reference_is_bridged_and_counted`, `a_gap_in_the_middle_of_the_reference_is_bridged_and_reported`, `a_gap_at_the_end_or_without_a_position_is_not_a_bridge` |
| division by zero: count of every non-finite (overflow included), baseline not counted, `±inf` of `sat/sa` | `division_by_a_zero_reference_is_counted_not_the_overflow`, `a_baseline_passing_through_zero_counts_the_division`, `a_zero_denominator_gives_nan_and_not_infinity`, `amplitudes_with_a_zero_sum_give_nan_mean_lifetimes` |
| length of X replaced by the longest of the columns (C4) | `the_output_has_the_length_of_x_when_x_is_given`, `without_x_the_output_has_the_length_of_y` |
| "source length" refusal restored for an output shorter than Y | `replacement_with_y_longer_than_x_keeps_the_cells_beyond_x` |

Known equivalent mutant: `h >= lo && h <= hi` in `bridges_hole` (a hole
at the same abscissa as a kept point assumes a duplicated X).

### Adding a feature that depends on it: the tests to write

**A. One more piece of data enters the calculation of a formula** (function
`row()`, reference by column name, unit, column of another sheet
#186, analysis parameter…).

1. Add it to the fingerprint (`Fingerprint`, or a variant of `Outcome`):
   *every piece of data the calculation reads enters it*. Beware of what the version
   of a column **does not cover**: it changes only with the type and the
   values, never with the name, the unit, the formula, the position or the
   sheet — such a piece of data is compared directly.
2. A named case in `recalc_targeted.rs` that changes **only** this piece of data and
   requires the new value.
3. A step in the draw of `b22` that makes it change. Without it, the oracle cannot
   see the omission: it compares only what the draw produces.
4. Mutation: removing the component must fail the named case **and**
   `b22`. A component that no test sees missing is either dead (remove
   it) or not covered (add the step) — write it in the table
   above in either case.

**B. A new way to write the data of a column** (primitive of
`core`, command, import, Python bridge, recalculation of another kind).

1. It draws a new number from `Project::next_column_version`: without it, the
   same number can designate two contents, and targeted recalculation believes a
   formula is up to date. A path that **inserts** an already numbered column (file,
   clipboard, snapshot kept by a client) goes through
   `Project::insert_node`, which moves the counter beyond it.
2. Add it to the draws of `b14` (doctrine on `History`) and of `b22`
   (doctrine and oracle in the actor).
3. If it can target a formula column (like `DeleteCells` and
   `SetColumnType` today), a named case: the column is rewritten by its
   formula.

Replacement by a correction (#212) is exercised by the step
`RemplacerCorrection` in `b14` and `b22`: ordinary numeric column,
constant subtracted by `compute`, `ReplaceAnalysisData` command with the
versions of its inputs. `b14` also checks the trace in the `.stf` round trip;
`b22` requires at least twenty passes for this gesture, keeping its targeted/full
oracle and its "unrelated" witness. The concurrency guards
take the own versions of each twin, whose counters can
diverge; the calculated result submitted to both stays identical.

**C. A new derived calculation reuses the mechanism** (analyses #184).

1. Same fingerprint rule (A.1), same pass order: no write after
   a reader has taken its fingerprint, otherwise a second pass rewrites and
   the "targeted" is no longer targeted.
2. Keep a full mode that shares **all** the code except the decision, and
   a twin battery on the model of `b22`, with its three properties
   (oracle, version ↔ content, unrelated command without rewrite) and its
   coverage check **per kind of step**. The draw must stay
   replayable: never choose a target in the order of `Tree::iter`, and
   do not compare versions on either side of a load (the
   counter restarts from the file there, a coincidence proves nothing there).
3. The named cases of the "targeted" family with their **witness** in full
   mode: without a witness, an unchanged version proves nothing.
4. A stable error result (cycle, fit that does not converge) is not
   rewritten at every pass: a named case checks it.
5. The memo is cleared when the project is replaced (`Load`, `NewProject`): a named
   case manufactures a coincidence of identifiers and versions, **checks**
   it, then requires the recalculation.
6. If the calculation is expensive (fit), measure that an unrelated command does not
   rerun it — by counting, like `b17`/`b21`, not by timing.
7. Rerun the mutants of each component, and complete the table.

**D. Touching the pass itself** (order of the steps, caching of the parsed
formulas, new step): rerun `b22` with more cases
(`runner(…)`) and the mutants; a step that writes after another has taken
its fingerprint is seen by the "unrelated" property of `b22` and by
`a_recalculated_chain_is_stable_at_the_next_command`.

## Randomised sweeps (#522)

The proptest batteries (`b14`, `b22`, `b23`, `b24`, and `undo_random` in
`stokesforge-commands`) draw from a fixed ChaCha seed, so `test-all` is
reproducible. Two environment variables change that, nothing else does:

- `SF_PROPTEST_SEED` (an unsigned integer) replaces the seed;
- `SF_PROPTEST_SCALE` (an integer from 1) multiplies the number of cases. `b2`
  also reads it: its sweep over the witness `.stf` hits 300 evenly spaced
  positions per unit of scale (both ends of the byte flipped from scale 2).

Neither set: the draws are exactly the fixed ones. An unreadable value fails
the test naming the variable. A failing case prints `replay with
SF_PROPTEST_SEED=… SF_PROPTEST_SCALE=…`; rerun with the same two values. The
weekly workflow runs all of it at scale 8 with a new seed each week. `B23_CASES`
is gone: the scale replaces it. The parsing is tested in `b14_sweep.rs`; the
helpers are in `stokesforge-robustness/tests/common/mod.rs`.

## Running

```sh
scripts/test-all.sh                           # everything, except #[ignore]: nextest + doc tests if cargo-nextest is installed, else cargo test --workspace (no argument)
cargo test --workspace                        # everything, except #[ignore]
cargo test -p <crate> --test <file>          # one test file
cargo test -p stokesforge-robustness -- --ignored  # reproductions and costly benches
SF_PROPTEST_SEED=7 SF_PROPTEST_SCALE=8 cargo test -p stokesforge-robustness --test b24_analysis_sweep  # randomised sweeps, see below
# (Typst is compiled and tested by `cargo test --workspace`; `--all-features` now only enables Python: see .codex/README.md)
scripts/python-test.sh                        # Python bridge, scripts, robustness, application and CLI
scripts/python-test.sh -- --ignored           # + the reproductions of the Python boundary
```

`scripts/test-all.sh` uses cargo-nextest when `cargo nextest --version` works (install: `cargo install cargo-nextest --locked`), then runs the doc tests, which nextest skips. Under nextest each test is its own process, so an in-process lock no longer excludes anything: `manual_screenshots.rs`, `preferences_proto.rs` and `memory_pinning.rs` hold such locks; if one flakes, give it a nextest test group rather than a retry. After a green run on a clean tree (untracked files included) whose HEAD did not move, it writes the stamp `<git common dir>/sf-tested/<HEAD sha>` (empty file; stamps older than 30 days are pruned); `scripts/deliver.sh` refuses a branch head without that stamp (#515). Filters stay `cargo test -p <crate> <name>`. The dev profile keeps line tables only (`[profile.dev] debug = "line-tables-only"` in `Cargo.toml`): a panic still prints `file:line`; to inspect variables, build with `CARGO_PROFILE_DEV_DEBUG=true`.

The console tests `script_panel` and the lazy-start tests
`script_bridge_lazy` (in `stokesforge-app-state/tests/`) also require the Python feature of `stokesforge-app-state`.
With the embedded environment described in [`.codex/README.md`](../.codex/README.md),
`scripts/python-test.sh` includes them; to run them alone:

```sh
cargo test -p stokesforge-app-state --features python --test script_panel --test script_bridge_lazy
```

## CI (GitHub Actions for Linux and Windows)

The Linux checks run on the GitHub mirror
(`.github/workflows/ci.yml`), Windows too (hosted runner `windows-latest`):
GitHub runs everything, release included. `.gitlab-ci.yml` keeps only the
reference definition of the Linux jobs, which the two configurations
follow together; they are disabled there by default so as not to wait for an absent
runner. `STOKESFORGE_CI_LINUX=gitlab` re-enables them if a
Linux runner is registered; Windows is not affected by this choice.

| Job | Blocking | What it does |
|---|---|---|
| `fmt` | yes | `cargo fmt --all -- --check`, plus `scripts/french-guard-test.py` and `scripts/french-guard.py` (#377), `scripts/release-notes-test.sh`, `scripts/linux-packages-test.sh` (pure part of the RPM/DEB packaging, #390: version mapping `v1.2.3-rc4` -> `1.2.3~rc4`, a fake nfpm records the staging tree; witnesses by mutation: `rc` mapped to `.` or kept `-`, `install.sh` staged, MimeType dropped, `Icon=` forced, icons (apps and the `.stf` mimetypes icon) not copied or their entries dropped, the publication switch back to 0 (#388), a write outside the staging folder by `--stage-integration`, publication guard without icon removed, publication switch inverted in `release-publish.sh`), `scripts/logo-dist-test.sh` (#388: `scripts/logo-dist.sh` with a fake generator, nothing reads the real logo; no opt-in writes nothing and does not call the generator, an opt-in with `assets/logo/` removed exits 1, an opt-in installs apps, mimetypes, scalable SVG and the notice on Linux and only the 256 PNG and the project `.ico` on Windows; `ci.yml` sets `STOKESFORGE_DIST_LOGO` in exactly `release-linux` and `dist-windows`; no `.ico` and no `hicolor` raster is committed; witnesses by mutation: test on the variable removed, silent fall-through without the folder, notice skipped, whole tree copied for Windows, opt-in removed from a job, an `.ico` added), `scripts/glibc-floor-test.sh` (glibc floor check on copies of system ELF files and a text file, #389; witnesses by mutation: comparison inverted, non-ELF files not skipped (kills the ELF count), `.so` files not scanned), `scripts/install-linux-test.sh` (installer of the Linux archive on fake archives in a temporary HOME containing a space, `--system` under `STOKESFORGE_INSTALL_ROOT`, #387; witnesses by mutation: MimeType line dropped, unquoted Exec, `ln -sf` without `-n`, previous version not removed, kept version linked through `current`, same-version guard, `rm -rf` of the data folder on uninstall (kills `recovery/`), body outside `main`, hard-coded `~/.local/share`, missing `command -v` guard, unconditional `Icon=`, the `.stf` type icon neither copied nor removed (#388), version taken from the folder name, system uninstall forgetting the MIME file, rpm/dpkg check removed or inverted, older-version prompt bypassed), `scripts/release-tag-test.sh` and `scripts/versions-aligned.sh` (all crates in the same version `X.Y.Z`, #349/#350; only the `[package]` section is read, internal dependencies `stokesforge-x = { version = … }` equal to this version, `Cargo.lock` up to date, `FORMAT_VERSION` = N requires `tests/witnesses/vN/project.stf`, #351/#352/#353; the pyst API version `PYST_API_VERSION` requires its snapshot `tests/api-witnesses/vX.Y.txt` equal to the doc, follows the bump rules, API major not above the software major, and snapshots of released tags unchanged, #356; the job fetches the tags) |
| `licenses` | yes | policy and generation of third-party notices |
| `clippy` | yes | `cargo clippy --workspace --all-targets -- -D warnings` |
| `test` | yes | `cargo test --workspace` |
| `robustness` | no | `cargo test -p stokesforge-robustness -- --ignored` |
| `test-python` | yes, automatic | vendors CPython + numpy, then `scripts/python-test.sh` |
| `release-linux` (GitHub) | on a tag `vX.Y.Z` or `vX.Y.Z-rcN` (on an `-rcN`, `publier` creates the release then deletes it; no release remains) | checks the tag (`scripts/release-tag-on-main.sh`, `scripts/release-tag-increasing.sh`, `scripts/release-notes.sh`), runs on `ubuntu-22.04` (glibc 2.35 = the floor, #389), assembles and tests `scripts/app-dist.sh`, with `STOKESFORGE_DIST_LOGO=1` (#388: the icons are generated, installed with their notice, and the packaged window must not log a logo it cannot decode), checks `scripts/glibc-floor.sh dist/stokesforge-app` (no ELF file above the floor in `scripts/glibc-floor.env`), runs `scripts/install-linux-test.sh --archive` on the real archive, then `sudo ./install.sh --system` and `--system --uninstall` (#387), uploads the archive to the GitLab package registry, without a GitHub release (#333); builds the RPM and DEB (`scripts/release-linux-packages.sh`, #390) and keeps them with the archive as the artifact `linux-packages` |
| `linux-packages` | yes | #390: `scripts/nfpm-install.sh` (pinned nfpm, `sha256sum -c`), then `scripts/linux-packages-test.sh --nfpm`: the real nfpm packs a fake distribution and `dpkg-deb` / `rpm` read the packages back (versions, architecture, dependencies by soname and by package, nothing from `lib/python`, root/root owners, nothing group or world writable, `/opt/stokesforge/` owned as a directory, `/usr/bin/stokesforge` link, libc dependency equal to `GLIBC_FLOOR` of `scripts/glibc-floor.env` (#486), source modification times kept; witnesses by mutation: one dependency dropped, tree entry replaced by a glob). Not killed: forcing the modification times (nfpm 2.47 ignores `file_info.mtime` on a tree and `SOURCE_DATE_EPOCH`). Since #388 it also runs the real generator on the real logo sources (`STOKESFORGE_DIST_LOGO=1 scripts/logo-dist.sh generate`, on `feature/**` too): the official sources pass and `00-final/stokesforge-logo-clair.svg` (on a tile) is refused, which locks the framing of the icon |
| `manual` | yes | `scripts/manual-build.sh`: the mdBook manual (`manual/`) builds (#362); mdBook version pinned in `scripts/manual-tools.env` |
| `test-linux-packages` (GitHub) | on a tag, after `release-linux`; matrix `fedora:42` (RPM), `debian:12` and `ubuntu:22.04` (DEB) (#390) | `scripts/linux-packages-install-test.sh` in the container, as root: installs the package with its dependencies only, `stokesforge --version`, `desktop-file-validate`, `.stf` type registered by the distribution triggers (`mimeinfo.cache`, `globs2`), window under `xvfb-run` with Mesa (exit 0), `stokesforge-selftest`, `install.sh --system --yes` of the archive refused, removal leaves nothing, package refused over an `install.sh --system` installation (preinst), reinstall, every file under `/opt/stokesforge` readable by others and every folder traversable (#486), then `rpmlint` / `lintian` with the committed suppression lists `scripts/linux-packages/rpmlint.toml` and `lintian-suppress.txt` (the bundled Python's files are filtered, by path for rpmlint; `non-readable` and the libc dependency are fixed, not filtered). The Fedora release is pinned: change it on purpose. The switch is 1 (#388): a failure here and in the package steps of `release-linux` blocks the release (`continue-on-error` is read from the switch; at 0 it would only show). The install test also checks, when the package carries the logo, that the app and `.stf` icons are installed and removed with the package |
| `upload-linux-packages` (GitHub) | on a tag, after `test-linux-packages` | uploads the RPM and DEB (`scripts/release-upload.sh rpm\|deb`) only when `PUBLISH_LINUX_PACKAGES=1` in `scripts/linux-packages.env` (gated on the steps, not on the job, so that `publier` does not skip); `release-publish.sh` links them under the same switch |
| `release-manual` (GitHub) | on a tag `vX.Y.Z` or `vX.Y.Z-rcN` | same tag guards as `release-linux`; builds the manual with the version in its title and uploads `StokesForge-X.Y.Z-manual.tar.gz` to the GitLab package registry (`scripts/release-upload.sh manual`, #362) |
| `test-python-windows`, `dist-windows` (GitHub, `windows-latest`) | automatic on `main`, `vN`, `release/**` and on a tag (not on `feature/**`), blocking (#333); published to GitLab like the Linux jobs (`JOBS` of `scripts/gitlab-status.py`, #452) | same checks on a clean machine; `dist-windows` sets `STOKESFORGE_DIST_LOGO=1` (#388, one line to change): the `.exe` icon (`STOKESFORGE_EXE_ICON`, `build.rs`), the setup icon and the `.stf` icon, checked in the installation and the registry, and the installer script is compiled once more with no logo define (`/O-`); on a tag, `dist-windows` uploads the installer to the GitLab registry (`scripts/release-upload.sh windows`) |
| `test-windows` (GitHub, `windows-latest`) | automatic on `main`, `vN`, `release/**` and on a tag (not on `feature/**`), blocking; GitHub only, not in `.gitlab-ci.yml`, but published to GitLab through `JOBS` of `scripts/gitlab-status.py` (#412, #452) | `cargo test -p stokesforge-core -p stokesforge-analysis -p stokesforge-session -p stokesforge-persistence -p stokesforge-commands`: the formula engine on Windows (float formatting, paths, stack depth); the first errors go in a `::error` annotation |
| Workflow `toolchain-weekly.yml` (GitHub, `ubuntu-latest`) | informative, blocks nothing; Monday 05:17 UTC on `main` and `workflow_dispatch`; separate from `ci.yml` so that `gitlab-status.yml` does not post it (#412) | matrix `stable` / `beta` (`RUSTUP_TOOLCHAIN` overrides `rust-toolchain.toml`): the depth-cap tests in release mode (`stokesforge-core` lib `deep depth`, `formula_evaluation deeply_nested`, `recalc_targeted too_deep`, `formula_roundtrip deeper_than_the_cap`), plus `cargo test --workspace` and clippy `-D warnings` on beta; a second job `sweeps` (#522) runs `b14`, `b22`, `b23`, `b2`, `b24` and `undo_random` at `SF_PROPTEST_SCALE=8` with `SF_PROPTEST_SEED` = the run number (debug profile, overflow checks on), then `memory_pinning` and `perf_write_under_snapshot` with `--ignored` in release. Read with `scripts/toolchain-weekly-status.sh` (public GitHub API, no `gh`) |
| `publier` (GitHub) | on a tag `vX.Y.Z` or `vX.Y.Z-rcN`, after all blocking checks and the package uploads | GitLab release with the notes of the CHANGELOG (#333), linking the installer, the Linux archive and the manual; an `-rcN` creates then deletes the release (trial); procedure and criteria: [`v1-criteria.md`](v1-criteria.md); the notes end with a line "Built with rustc … (Linux) and rustc … (Windows)" (outputs `rustc` of `release-linux` and `dist-windows`, `scripts/rustc-note.sh`), and `release-publish.sh` refuses to publish if one is missing (#412) |
| `pages` (GitLab, instance runners) | only in a pipeline started by `publier` (`scripts/release-publish.sh`, variable `STOKESFORGE_PUBLISH_MANUAL=1`) on a **final** tag `vX.Y.Z`, never an `-rcN`, never in the pipeline started at push | unpacks `StokesForge-X.Y.Z-manual.tar.gz` from the GitLab package registry into `public/` (GitLab Pages, `https://askrzypczak.gitlab.io/stokesforge/`); only the latest manual is served (#362). If the pipeline cannot be created, check that the protection rule of the tags `v*` still allows the token's role to create tags (see "Settings outside the repository" below) |

The Linux jobs run on the pushed branches `main`, `PostV1.1` (integration line, #480), `feature/**`,
`v[0-9]*`, `release/**` and on the tags `v*`. `release-linux` and `publier`
refuse a tag whose commit is an ancestor neither of `origin/main` nor of a
`release/*` branch (`scripts/release-tag-on-main.sh`, #354), or that does not
exceed the existing tags (`scripts/release-tag-increasing.sh`, #355):
a final tag must exceed every existing final; an `-rcN` must exceed the
`-rc` tags of its version and not target a version that is already final; a commit that
is only on `release/N.x` is compared only with the tags of major N. The distribution
scripts build with `--locked`.
Consequence: rerunning the pipeline of an old tag fails as soon as a more recent
tag exists. An existing tag set again on another target cannot be detected
in CI; only the protection of the `v*` tags of GitLab holds it.

**Settings outside the repository** (to be done by hand, nothing in the code sets them):
- `main` protected on GitLab: force-push and deletion by push refused for everyone, push and merge
  for Maintainers and above (read it: `glab api projects/86104040/protected_branches`). A rewrite
  on purpose means removing the rule first (Settings, Repository, Protected branches);
- `main` is **not** protected on GitHub: GitHub only runs the CI, and the mirror from GitLab
  is the only writer. A rule there would stop the mirror if the two ever diverged (#481);
- `v*` tags protected on GitLab: moving or deleting one by push is refused for everyone, deletion
  only through the GitLab web page or API (read it: `glab api projects/86104040/protected_tags`).
  Creation is allowed to **Developers + Maintainers**, not Maintainers alone:
  `GITLAB_RELEASE_TOKEN` has the Developer role, and GitLab creates a release or runs a pipeline
  on a protected tag only for a user allowed to create that tag (#481);
- `main` default branch on GitLab: this is what makes "Closes #N"
  work;
- GitLab, Settings, CI/CD, Runners: instance runners enabled on the project, for
  the `pages` job only (all the other jobs are gated by `STOKESFORGE_CI_LINUX`
  or run on GitHub); GitLab may ask to validate the account first (#362);
- GitLab, Deploy, Pages: "Use unique domain" unchecked, so that the URL is
  `https://askrzypczak.gitlab.io/stokesforge/`; Pages visibility "Everyone".
- GitLab, Settings, CI/CD, Variables: "Minimum role to use pipeline variables" at
  **Developer** (API field `ci_pipeline_variables_minimum_override_role`). `publier` starts
  the `pages` pipeline with the variable `STOKESFORGE_PUBLISH_MANUAL=1`, and
  `GITLAB_RELEASE_TOKEN` has the Developer role. With the GitLab default "No one allowed",
  the release is created, then `publier` fails with curl exit 22 and the manual is not
  published (v1.1.0): set the role, then start the pipeline by hand on the tag
  (`POST projects/:id/pipeline`, `ref` = the tag, same variable).

**What a green pipeline does not prove**, and it is written at the top of the CI file:
`fmt`, `clippy` and `test` **never** enable the `python` feature. Everything that
touches PyO3 is compiled **empty** there — the Python bridge, the riskiest part
of the architecture, is neither compiled, nor checked by clippy, nor tested by these
three jobs.

The interpreter is not versioned. `test-python` downloads it cold and
keeps it in cache according to `vendor/python/lock.env`; its Cargo artifacts have
a key distinct from the pure Rust jobs. It automatically covers bridge,
scripts, robustness, application and CLI. The ignored reproductions and the
Matplotlib spike remain explicit trials. No system Python is
required.

Typst is no longer a feature (issue #29): Clippy compiles it, the tests of the
workspace exercise it, exports included (#34). `fmt` compiles nothing; it also checks the alignment of crate versions
(`scripts/versions-aligned.sh`, at every push, not only at the tag), including internal dependencies, `Cargo.lock` and the `.stf` format witness, without cargo.

### Using CI day to day

**Who does what.** GitLab is the reference repository; a mirror copies it to
`github.com/Marek9935/StokesForge`, where `ci.yml` runs as soon as a `push` of `main`
(or a tag `v*`) arrives. A second workflow, `GitLab results`, then sends
the results back to the GitLab pipeline of the same commit. **So only GitLab is consulted**;
the full logs stay on GitHub (link in each status).

**Pushing.** `git push origin main` (GitLab). Count about fifteen minutes
for Linux CI to finish, then a few moments for the report. A more recent push
on the same reference cancels the running series. Pull requests start
GitHub CI but report nothing to GitLab (intended, see below).

**Integration line (#480).** An issue whose record carries `line` (`suivi.py claim --line`, read by
`suivi.py line <n>` and `sf_line_of`) starts from `origin/<line>`, is delivered into `<line>` by
`deliver.sh` (in the permanent worktree `../_stokesforge/line-<line>`, the main checkout untouched, the
same `main-lock.sh` lock, no closing keyword and no `--wait-tag`) and is released against the line.
`line.sh create|sync|land` make the line, bring `main` into it and land it (the declaration is the GitLab label
`ligne-<line>`, never in `LABEL_ALL`; `land` waits for the tag `v1.1.0` on `main` and for the trial entries of
issue #480 that try the line, and closes the issues from the merge commit through a fake `glab` in the tests).
The tracking page has one row per line. The CI watcher reads each line's head
too (`ci-line-<name>.json`, `lines` of `/api/state`). `workflow-test.sh` and `suivi-test.py` cover each
rule with a witness by mutation.

**Several orchestrators on `main` (#413).** `ci.yml` cancels a running series when a newer push
arrives (`concurrency`): a cancelled run is *superseded*, not failed. Read the CI of the newest
`main` that contains your commit (`git merge-base --is-ancestor <commit> origin/main`), not of your
own commit. To keep pushes orderly, merge and push `main` with
`scripts/deliver.sh <n> <message-file>` (#449): it takes the `main-lock.sh` lock itself and releases
it on every exit, stops at the first failure, merges `--no-ff` with the message file, pushes
without tags (4 retries) and only then writes the real merge commit on the issue's trial entries
(`suivi.py test merged`); a rerun after a failure does not merge twice, and exit 3 means `main` moved
and was merged into the branch (recheck, rerun). It stops before the CI and `--release`. The scripts of this workflow
(`chantier.sh`, `try.sh`, `main-lock.sh`, `deliver.sh`, `line.sh`, `workflow-lib.sh`) are tested by
`scripts/workflow-test.sh`, run by the `fmt` job, and the colour rule by `scripts/suivi-test.py`
(same job). `scripts/try.sh` builds in debug; `scripts/try.sh --release <n>` (#474) builds and runs the release binary of the same worktree, because debug draws about 23 frames/s against 120 in release (#340): rerun in release before diagnosing a slowness seen at a trial. Besides the colours, briefing and locks, `workflow-test.sh` covers
(#449) `deliver.sh` in its own sandbox with a bare fake remote (happy path, refusals, held lock, fetch,
merge and push failures with retries, rerun, record failure, moved `main`, conflict) and
(#441) `lance` recording the owner session (`CLAUDE_CODE_SESSION_ID`, `CLAUDE_PID`) and a rerun in
a new session keeping the colour, one record per issue (sibling worktrees `feature<n>b`, detached HEAD),
`chantier.sh --status [--json]` (six colours in order, `since`/`days` with the
`SF_TODAY` seam, colourless worktrees, paused issues), `--pause` of a package (refused while a
worktree exists, frees the colour, keeps the list, `--start` takes the first free again) and of a plain
issue (worktree kept, previous colour back on resume), the
package `since` date surviving a rerun, `suivi.py check` clean at the end, and `--release` (one fixed line; refuses an
unmerged branch, modified or untracked files, ignored files other than `target/`,
a run from a worktree or off `main`; never `-D` nor `--force`).

`scripts/suivi-test.py` also pins the GitLab labels and the tag report (#467) with a fake `glab` (it logs every call and applies every `PUT` to its own state) and a local stand-in for the GitHub API: the mapping of `label_wanted` (every hold reason, a package parent and its subs, no `git` call), one `PUT` per changed issue with `add_labels` / `remove_labels` and never `labels=`, closed issues, a failed, timed-out or empty read writing nothing, the 10-minute demotion delay, a waiting label hiding `en-pause`, the gate (no `issues` call with `SF_SUIVI_DIR` or `--state` unless `SF_SUIVI_LABELS=on`, ETag steady), the first-run seed, rc versus final tags, a restart not commenting twice, a run of another commit ignored, missing, running or unreadable jobs giving no proof, matrix legs, the card and its OK closing the issue. `workflow-test.sh` pins `deliver.sh --wait-tag` (refused with `Closes #n`, records the wait without calling `glab`) and the cleanup warning of `chantier.sh --release`.

`scripts/suivi-test.py` also pins the screenshots of the tracking page (#456): the raw upload routes (formats by magic bytes, refused SVG, text and short bodies, guard before the route, size limits), the strict GET routes, the `images` field of the trial and QCM POSTs (unlisted files deleted, bad lists refused), the wake and CLI lines, `test remove` archiving, the global 512 MB cap (small cap through `SF_SUIVI_MAX_IMAGES_TOTAL`: oldest first, new file kept, identical re-upload refreshed, archives counted, no JSON rewrite, dangling names reported and dropped on a save only when recorded), and the page and form through `node` blocks (`IMAGES`, `QIMG`: filters, patches, URLs, static checks that no answer is rebuilt without its images). Re-render survival, paste and drop have no harness: the pre-trial checks them. Each case was proved by a mutation.

`scripts/suivi-test.py` (stdlib `unittest`, throwaway repository and state folder, the server on port 0) also pins the CI watcher (#452: verdict per job list, fake `glab` on `PATH`, Windows jobs in the blocking list, red kept until a newer green, staleness, `SF_SUIVI_CI=off`) and the local server (POST needs `X-Suivi`, foreign `Origin` or `Host` is 403, unsafe id 400, body over 256 KB 413, no `Access-Control-*`, loopback only, ETag 304), `test add|merged|remove|list` (merge-commit check, `--replace`, the 5-entry limit and `--force`, removal by the owning session and its archive), `set` erasing the agent note, the page's `cardLine` (whose turn it is, working or stopped agent from `agent_idle_min`), `import-online` and the `legacy` hold of `migrate`, and parses the page script with `node --check`. It pins the rule
of `scripts/suivi.py`: a live owner holds (pid exists, same start time, no newer `ended/<sid>`); a
dead pid, a reused pid or an ended session pauses; an open QCM or a `todo` trial entry with a `try`
field holds, a finished QCM, an `ok` entry or one without `try` does not; resume takes the previous
colour when free, else the first free; the seventh claim fails; parallel claims get distinct colours
(lock); the `session-end` hook always exits 0 and writes nothing on bad input; `migrate` is
idempotent; the `release-line` reasons and `check`. Each case was proved by a mutation (drop the
clause, see the test fail, restore).

`suivi.py wait <n>` (#454) is pinned by `scripts/suivi-test.py` with real background processes (poll 0.05 s, `--timeout` as the guard): the events (a trial entry with `try` whose status or comment changed, a form once finished and again when its answers change), the memory in `.suivi/heard/` (what was already reported is not reported again, a new session hears everything once, nothing is remembered before the line is printed), the exits decided from the record (`done`, `paused`, `taken`, `session gone`, `nothing open`, `timeout`), the registration in `.suivi/waits/` (a second wait of the same session says `already listening`, another session exits 1, a package shares one registration, SIGTERM is logged), no `git` call after start, and `status` carrying `listening` and `unread` without the wait ever holding a colour. The wake policy is the single function `should_wake` (seam `SF_SUIVI_WAIT_SETTLE`, default 180 s): a plain OK with entries still `todo` wakes nothing and stays unseen; a finished form or no `todo` left wakes at once and reports the earlier silent OKs; a Retour or a comment waits the delay after the last change of any entry, each change restarting it, and a misclick un-ticked in time wakes nothing. `hook-news` (the `SessionStart` and `UserPromptSubmit` hooks, always exit 0, no git call, one run measured at about 60 ms) is pinned for bad input, an owner hearing each event once with the "arm the wait" hint, a new session listing dead owners with unread answers without recording, and a live owner's issue staying silent. `hook-wait` (the `PostToolUse` hook on `Bash` with `asyncRewake`, run as the waiting process) is pinned with the hook JSON on stdin: a `qcm pose` or `test add` command arms the wait and an event exits 2 with the text on stderr (stdout empty, last line saying the wait resumes at the next `suivi.py` call), every other outcome (unrelated command, `suivi.py status`, another session's issue, a wait already listening, nothing open, bad or empty JSON, an argparse error, an exception inside the wait) exits 0 silent, the session comes from stdin and not from the environment, and the issue number is parsed from every command form. The forms of an issue (#479) are pinned too: `qcm pose` refuses a second open form (finished ones never count, `--force` overrides), `qcm withdraw` and the page's « Retirer » move the whole folder to `removed-qcm/` (owner matrix, `--answered`, finished refused, numbers never reused, answers readable afterwards, late answer or finish answered 410 and never recreating the folder), a page withdrawal wakes the wait once and a CLI one does not, `check` reports two open forms, and a card repeating an answered question of a withdrawn form is refused. The merge button (« Fusionner ») is pinned in three places: `POST /api/issue/<n>/merge` (409 unless every `try` entry is OK now, the usual guards, idempotent, voided by any later change of an entry and not back on its own), the wait (a valid request wakes at once and only once, a merge ready and not delivered keeps the wait armed, `hook-wait` arms in that state, `test merged` deletes the request and the wait then ends), and the page (`cardLine` fields, `mergeUi` showing the button, the grey note, or nothing); the colour rule is unchanged (all OK and no agent frees the colour). A settled retour (#496, `test settle` or the page's « Classer ») is pinned in `scripts/suivi-test.py` and `scripts/workflow-test.sh`: status stays `retour` and only the page's own answers clear `settled` (a new Retour or an OK clears it, a note keeps it), an agent's settle wakes no wait while the user's does, a settled retour counts as OK for `merge_ready`, `merge_key` and `cardLine`, the page splits « En traitement » (retours and ideas not settled) from « Traités » and shows « Classer » on an unsettled retour only, `check` lists unsettled retours of done issues, and `deliver.sh` refuses an unsettled retour. The trial framing and the third tick are pinned too (#518): `test add` refuses an entry with `try` and no `goal` (a `kind: measure` or an entry without `try` is exempt) and a batch with no `around`; « OK + idée » is an `idea` on an `ok` entry (400 with any other status, dropped by the next status, counted OK by `merge_ready` and `cardLine`, listed by `test unsettled --ideas` and `check`, settled by `test settle`, waking the wait like a comment); the page's `trialFrame` and `imgPatch('idea')` run in `node`; `workflow-test.sh` pins `deliver.sh` printing the unsettled ideas without refusing and `try.sh <n> <file.stf>` (copy in the run line, a missing file or a `.csv` refused). Decision forms (#520): `qcm pose` refuses more than 8 cards (a list card counts as one, `--force` overrides with a `forced` log line), a list card without `lines` or with `o`, and code words in card texts (sketches are not read; a long bullet only warns); `qcm-log.tsv` gets `refused`, `forced`, `posed` and one `first-answer` per form (blank without `created_ts`, never failing the command); `qcm answers` prints `  to discuss:` lines; the template's `LIST` block (record, label, reload by line text) runs in `node`. The page's `cardLine` is tested for the words « L'agent écoute », « écris-lui après ta réponse », « Réponse reçue : lance #n » and « écrire à l'agent pour fusionner » (all `try` entries OK), with an old server keeping the old texts.

`scripts/driver-test.py` (#452, stdlib, no display, no Xlib) pins the `run-stokesforge` driver: display reservation by locked file (distinct numbers across threads and processes, a dead holder frees its number), mode choice, process-group `stop`, modifier clicks and the drag event plan. The driver runs on an **invisible screen** (headless mutter and Xwayland, nothing on the user's desktop); `SF_VISIBLE=1` forces the visible one. It handles no OS drag and drop (drags inside the window work). `scripts/time-report-test.py` tests `scripts/time-report.py` (below). Both run in the `fmt` job.

**Nobody polls the CI (#452).** The orchestrator reports at the merge, closes and releases without waiting for the CI. The watcher of `suivi.py serve` (a thread of the local tracking server, about every 120 s) reads the head of `main` and shows "CI rouge / verte / en cours / inconnue" above the tabs of the tracking page; a red `main` is reported and fixed first. The CI is still read by hand for a tag or release and for a change to CI itself. The next sub-issue of a package does not wait. Details and verdict rules: `.claude/skills/stokesforge-verification/references/suivi.md` § CI watcher.

**Reading the result.**

```sh
scripts/ci-status.sh [commit]      # HEAD by default; requires a logged-in glab
scripts/ci-status.sh --head        # the newest commit of main on GitLab, jobs only (what the watcher reads)
```

Or in the GitLab interface: `Build > Pipelines`, stage `external`. One line per
job; on failure, the faulty step and the first error are in the
description, and a **comment on the commit** gives about fifty lines of
log (the script displays them). "No pipeline for this commit" means that
GitHub CI has not finished or that the mirror has not yet received the commit.

**Windows** is automatic, on GitHub: a push on `main` or a version tag
starts `test-python-windows` (checks) and `dist-windows` (installer) on a
hosted `windows-latest` runner, in parallel with Linux (Git Bash,
`shell: bash`: same scripts as everywhere). They are blocking, `publier` waits
for them; on failure, the first errors are set as an **annotation** (readable by
the API without login, see below). No GitLab runner serves there anymore: the
author's machine no longer needs to be on (the old GitLab runner
`windows` and its jobs, #333, are removed). `test-python-windows` runs the
tests in parallel; only the captures (`egui_kittest` + wgpu) exclude each other
(see below). The job sets `core.longpaths` before the checkout:
without it, `git clean` fails on `.cargo/registry` ("Filename too long").
`test-windows`, `test-python-windows` and `dist-windows` are in the list of
`gitlab-status.py` (#452): their result and failure comment reach GitLab and
`ci-status.sh` like the Linux jobs, and the watcher of the tracking page reads them
there. On a `feature/**` push they are skipped by their `if`; the reporter posts
them as skipped (and also as skipped, never failed, if GitHub did not list them on
such a ref). `release-linux` and `publier` stay outside the list: their result is
read on GitHub. The GitHub API **without login** is limited to 60 requests per
hour and per address: do not poll it in a loop; beyond that, it answers 403 until
the next hour.

**Tag and mirror.** A tag pushed to GitLab sometimes arrives late on GitHub
(CI starts there only on its arrival). Force the push: mirror synchronization
button, or `glab api -X POST projects/86104040/remote_mirrors/<id>/sync`.

**Package upload.** `scripts/release-upload.sh` passes the token to
`curl` through `-K -` (standard input), not through process substitution: under
Git Bash (Windows) the latter fails (curl code 26). The token never appears
in the arguments of the command.

**Reproducing a Linux failure at home.** The runner (`ubuntu-latest`; `release-linux` uses `ubuntu-22.04`) differs from a
development machine in three ways that have already caused failures:

- **A newer stable Rust** than the machine's (`rust-toolchain.toml` asks for
  `stable`): a new Clippy lint can make `clippy` fail without anything
  having changed. Fix the code, do not pin the version.
  The weekly workflow `toolchain-weekly.yml` (#412) runs beta ahead of time
  (workspace tests, clippy) and the depth-cap tests in release mode on stable
  and beta, so a new compiler is caught before it becomes the stable that
  `ci.yml` uses.
- **No GPU**: the captures (`egui_kittest` + wgpu) look for a Vulkan
  adapter. `ci.yml` installs the software driver `mesa-vulkan-drivers libvulkan1`
  (jobs `test` and `test-python`).
- **Development packages missing**: `test-python` compiles `wayland-sys` with
  pkg-config and installs `libwayland-dev`. An error `failed to run custom build
  command for …-sys` signals a missing package: add it to the `apt-get` step
  of the job concerned in `ci.yml`.

To replay in a fresh Ubuntu: an `ubuntu:24.04` container with
`build-essential pkg-config curl git`, `rustup` (minimal profile) and the packages
above, mounting the repository read-only and copying its source elsewhere.
**Put `CARGO_TARGET_DIR` on disk**, never under `/tmp`: it is a tmpfs in
RAM, and a build of the workspace weighs more than 10 GB there.

**Room for the heavy tests of `stokesforge-app-state`** (#342). Three tests
write the "heavy project" of `tests/shell.rs` (`heavy_formula_project`, 150
columns of 200,000 values, about 240 MB) in `tempfile::tempdir()`, hence
under `TMPDIR`: `saving_during_a_heavy_recalculation_clears_the_unsaved_mark`,
`a_dead_session_raises_one_blocking_alert_with_a_recovery_copy` and
`autosave::during_a_recalculation_the_deadline_is_skipped`. About 1 GB must be free
in `TMPDIR`; on a full `/tmp` (`Disk quota exceeded`), set `TMPDIR` on
disk. The application reports the error instead of losing data: this is
not a bug. The assertions of these tests join the messages of the status
bar (`status_messages`), where the write error is read. The size of the project
is not reduced: it sets the duration of the recalculation that the tests observe
(wait of 200 ms exceeded).

**Keeping the two CIs in agreement.** Every command changed in `.gitlab-ci.yml` or
`ci.yml` is carried over to the other (see the table above). A new Linux
job is also added to `JOBS` in `scripts/gitlab-status.py`, otherwise its
result is not published. Known exception: `release-linux` and `publier` do not
appear there; their failure is read on GitHub (annotations readable by the API
without login). The Windows jobs are in the list since #452.

### GitHub results visible in GitLab

`.github/workflows/gitlab-status.yml` triggers when `CI` is finished,
even on failure or after cancelation. `scripts/gitlab-status.py` publishes the
statuses `github-actions/<job>` on the tested SHA, in the same GitLab
pipeline, with a link to the GitHub logs of each job. The results
arrive after the whole CI has ended; the logs and artifacts
stay on GitHub. Old pipelines are not corrected retroactively.

A failed `robustness` reproduction is marked `skipped`, with the description
"Non-blocking failure of the reproductions": the external statuses API offers
no `allow_failure` parameter. The other failures are red, a cancelation
stays canceled, and a missing job is never declared successful.

For a failed job, the status description gives the faulty step and the
first cause read in its GitHub log (`rustc`/`clippy` error with its
file, test panic with its message, script error), truncated to 255
characters. The log is read with `GITHUB_TOKEN` (`actions: read`); if it is
unavailable, the step alone is published, and the report never fails because of it.

The same failure is also posted as a **comment on the GitLab commit**: about
fifty lines around the first cause (10 before, the rest after) and
the link to the full log. Read with
`glab api projects/86104040/repository/commits/<sha>/comments`, like the
statuses by GraphQL (the REST API does not list external statuses):
`pipeline(iid){stages{nodes{name jobs{nodes{name status detailedStatus{label}}}}}}`.

When `licenses` succeeds, the report `THIRD-PARTY-LICENSES.txt` (GitHub artifact
`third-party-licenses`) is deposited in the GitLab generic package registry:
package `third-party-licenses`, version = first 12 characters of the SHA.
A file of about 600 KB per push of `main` consumes the storage quota of the
project; the full logs are deliberately not deposited. GitHub download
redirects are followed without sending the token again. A refused comment or
deposit is reported on stderr without failing the
report: the statuses are already published.

The reporter ignores pull requests and forks, and reads its script from
`main`, never from the code of a PR. It also ignores an attempt for which a more recent
rerun exists. A publication outage makes the workflow
`GitLab results` fail on GitHub, without turning the test results.

The GitHub repository `Marek9935/StokesForge` and the GitLab project identifier
(`86104040`) are written in `scripts/gitlab-status.py` (`REPOSITORY`, `GITLAB`),
in `gitlab-status.yml` (condition `github.repository`) and in
`scripts/ci-status.sh`. Renaming either repository requires changing them.

**Maintenance.** The GitLab project token of the GitHub secret `GITLAB_RELEASE_TOKEN`
(Developer role, scope `api`; package upload and creation of the
release, #333) also **expires**: after the deadline, `release-linux`, `dist-windows`
and `publier` fail. Recreate it in GitLab (`Paramètres > Jetons
d'accès`) and replace the secret. The GitLab token `github-ci-status` **expires**: at its deadline,
`GitLab results` fails on GitHub ("Reporting to GitLab failed") and no
status arrives anymore. Create a new token (step 1) and replace the secret
`GITLAB_STATUS_TOKEN` (step 2). The GitHub token of the mirror (step 0) **also expires**
(created on 17 October 2026, deadline 17 October 2027): after that time, the mirror
no longer pushes and CI never starts. Regenerate it on GitHub (`Regenerate
token`), then paste its value again in the GitLab mirror. A missing right (especially
`Workflows`) has the same effect: the mirror fails silently on the GitHub side.

Initial configuration (once only):

0. **GitLab → GitHub mirror.** On GitHub, create a **fine-grained** token
   (`Settings > Developer settings > Personal access tokens > Fine-grained`),
   limited to the single repository `Marek9935/StokesForge`, with these rights:
   `Contents` read and write, **`Workflows` read and write** (without it,
   GitHub refuses any push that touches `.github/workflows/`), `Metadata` read
   only (mandatory). In GitLab, `Paramètres > Dépôt > Dépôts miroirs`
   (`Settings > Repository > Mirroring repositories`): direction "Push"
   to `https://<utilisateur>:<jeton>@github.com/Marek9935/StokesForge.git`
   (GitLab then hides the credentials). The synchronization button of the
   line forces an immediate push. On GitHub, `main` must be the default branch (a `workflow_run`
   triggers only from it) and `Settings > Actions > General` must allow
   actions.
1. In the **GitLab account settings**, create a dedicated personal token
   (`Access > Personal access tokens > Legacy token`), named
   `github-ci-status`, with scope `api` and an expiration date. The account must
   be able to publish statuses in the StokesForge project. A project token
   with Developer role and scope `api` can be used if the plan allows it.
2. In the **settings of the GitHub repository** `Marek9935/StokesForge`, open
   `Secrets and variables > Actions > New repository secret`. Exact name:
   `GITLAB_STATUS_TOKEN`. Value: the GitLab token, typed directly here.
   No token is committed or asked for in the conversation.
3. Push a GitLab commit and wait for its mirror, or rerun GitHub CI.
   The workflow `GitLab results` then publishes the states in
   `GitLab > Build > Pipelines`, stage `external`.

Reporter tests, without network or real token:
`python3 scripts/gitlab-status-test.py` (also run by the `fmt` job, with `scripts/release-notes-test.sh`, which tests `release-notes.sh` and `versions-aligned.sh` on a fake repository). They cover the results, the
tolerated reproduction (including a failed step under a tolerated conclusion),
cancelations, absent jobs, excluded PRs/forks, old
attempts, pagination, pipeline selection and HTTP retries.

## Intermittent tests (#447)

`a_missing_default_folder_gets_a_note_and_an_existing_one_does_not` (`crates/stokesforge-app/src/app/preferences_window.rs`) failed once on CI, on `main` at `f6a4c832` (2026-11-04), and never since. It was never reproduced: 40 runs alone, 10 runs of the whole library on 2 cores, 17 runs on 1 core with up to 150 concurrent programs, 0 failures.

Measured on 2026-11-05 through `scripts/ci-status.sh`: of the 67 commits after `f6a4c832`, 7 have a `test` result, all green; of the 63 after `fe450cb1` (which added the diagnostics), 6 have one, all green. The other commits have no CI run (only pushed commits get one).

Ruled out: the process-wide language (no in-memory test leaves English), an overwritten setting (preferences are in memory), environment variables. Weakened: a wait that was too short (it counted frames, so it slowed down with the machine; it now waits up to 30 s of real time).

If it comes back: read the failure message in the CI log. It gives the duration, the frame count, the page on screen, the saved folder and the state of the disk probe. Then reopen #447 with that message. Do not loosen the test before reading it.

## Local build queue and build modes (#476)

`scripts/cargo-queue.py` is a `cargo` shim for the development PC (installed by the user with `scripts/cargo-queue-install.sh`, removed by `scripts/cargo-queue-uninstall.sh`; CI never has it). Heavy subcommands (`build`, `check`, `clippy`, `test`, `nextest`, `bench`, `run`...) wait in a local queue, then run in the user slice `sfbuild.slice`. `fmt`, `metadata`, `--version`, `--help` and `rust-analyzer` calls pass straight through; `SF_CARGO_QUEUE=off cargo ...` skips it by hand, `SF_CARGO_QUEUE=first` goes ahead of the other waiters (it still waits for a slot). A command typed by the user in their own terminal (stderr is a tty and `CLAUDECODE` is unset) starts at once, without waiting for a slot: it keeps a ticket, so the page shows it and it counts as running (agents wait behind it, at most one extra build), and it runs in the slice with the mode's jobs. Agents and redirected stderr wait. The three modes (`full`, `dev`, `background`) and their numbers live only in the `MODES` table of the script; the user switches them on the tracking page (`references/suivi.md`) or with `cargo-queue mode ...`. FullIA also switches the GNOME power profile to `performance` on mains (#477, field `profile` of the table; `dev` and `background` make no call at all): it puts back the profile found on entry when the mode is left, unless the user changed it meanwhile; while FullIA lasts the 30 s tick follows the power source (battery: the found profile comes back, mains again: `performance` is set again; the record decides and the user's own change always wins, no record means no entry from the tick); the memory is `profile.json` in the state folder, written under `mode.lock`. `scripts/cargo-queue-test.py` tests it with a fake cargo, a fake `systemd-run`, `systemctl`, `gdbus` and `busctl`, and real `flock`s (in the `fmt` job). **No test may touch the real power profile**: every test has a canary `busctl` whose log must stay empty unless the test opts in (`enable_bus`), the link is off by default as soon as `SF_CARGO_QUEUE_DIR` is set (seam `SF_CARGO_QUEUE_PROFILE=on|off`, `SF_CARGO_QUEUE_PROFILE_TIMEOUT`), and the state folder is always a temporary one.

Classes, small lane, wait log (#517). Each queued call is classified by `classify(args)` (table `CLASSES`; `SF_CARGO_QUEUE_CLASS=small|big` overrides, another value warns once and is ignored). **small**: `build|check|clippy|run` with one or two `-p/--package`, or `test|nextest run` with exactly one `-p` plus a narrowing (a test-name filter, a word after `--`, `--test NAME`, `--lib`, `-E`). **big**: everything else (`--workspace`, `--all`, `--exclude`, `--release`, a profile other than dev/test, no `-p`, three or more `-p`, `bench|doc|install|miri|fix|rustc|rustdoc`, an unknown `--flag` followed by a bare word: when in doubt, big). The ticket name stays `w-<rank>-<ns>-<pid>`: rank 0 typed or `first`, 1 small, 2 big (3 is kept for the `low` class of #523). Waiters start by (rank, arrival); a big waiter that has waited `SF_CARGO_QUEUE_AGE` seconds (default 300) counts as rank 1. **Small lane**, `background` mode only (one slot plus one small lane; a third concurrent build was ruled out by the #476 measurements): the first small waiter may start in the extra slot when no ticket already runs in the lane, `MemAvailable` is at least 4 GiB (seam `SF_CARGO_QUEUE_MEMINFO`) and no running ticket shares its build root (`CARGO_TARGET_DIR`, else the nearest ancestor of the cwd holding `Cargo.lock`). It never preempts. A waiting call prints every 30 s (seam `SF_CARGO_QUEUE_PRINT`) `cargo-queue: waiting 2m10s, 2 ahead (position 3, class small, mode background, 1 at a time + 1 small lane)`, the list of who is ahead only when it changes, and `started after 95 s (lane extra)` when it waited a second or more. `status` shows the class. `<state folder>/waits.tsv` gets one line per ticket, no header, tab separated: end time (UTC), wait seconds, `started|gave-up`, lane `normal|extra|typed`, class, mode, tickets ahead at entry, pid, cwd, command (rotated to `waits.tsv.1` above 1 MiB; a `kill -9` writes nothing; a pass-through call writes nothing). Tests: `TestPriority` in `scripts/cargo-queue-test.py`.

`scripts/wait-for.sh [--pid PID] [--lines N] <log> <regex> [timeout_s]` waits for a log marker without polling (`tail -n +1 -F` piped to `grep -E -m1`, no `pgrep`, no `sleep`): exit 0 match, 1 timeout (default 540 s, clamped to 590 s), 2 usage or bad regex, 3 the `--pid` process ended; it prints the match or the timeout and the last lines of the log. Tested by `bash scripts/wait-for-test.sh` (`fmt` job).

What it changes for tests and for reading times:

- **Time-sensitive tests** (those that measure a duration, #447, #469) may fail while the PC is busy or the mode is `background`. Read the failure first (its message gives the measure); do not loosen a test up front.
- **The queue wait counts** in `time-report.py`'s "cargo test" and build time.
- **Foreground limit**: a foreground Bash command is cut at 10 minutes, waiting included. A heavy cargo goes in the background with its log in a file.
- **Lock order**: the `main` lock (`scripts/main-lock.sh`) first, then the queue; the queue never waits on `main`. `scripts/deliver.sh` may wait in the queue while it holds the `main` lock.
- `scripts/try.sh` builds with `SF_CARGO_QUEUE=off`: a trial is for the user at the PC and must not wait behind agent builds.
- The build runs at the slice weight, not `nice`: an unprivileged process cannot lower its nice value again, so a build could not return to full speed when the mode changes.

## Time report (#452)

`python3 scripts/time-report.py [--since DATE] [--until DATE] [--logs DIR] [--power FILE] [--json]` says where the working time of the Claude sessions goes: per activity (model, tool, cargo test, wait for the user, wait for CI, ...) and per issue. Dates are UTC (`2026-10-31` or `2026-10-31T20:00`). `--logs` defaults to the session logs of this repository's main checkout. Privacy: the logs are private, the tool prints aggregates only (hours, counts, issue numbers), never a prompt, reply, command, title or description; a malformed line is skipped silently.

`--power FILE` is a JSON object `{"suspend": [[start, end], ...], "off": [[start, end], ...]}` in epoch seconds; time spent suspended or off is taken out. Without it the windows are read from `journalctl` (suspend entries, gaps between boots); if neither is available the report says so.

Limits to remember when reading it:

- A background task that ends with no notification is not counted as running; its time falls into another category.
- "Wait" includes `AskUserQuestion` and the wait of a session just opened.
- The reason for a wait (form, trial, CI, ...) is a heuristic: the first classifier was right on 25 of 40 sampled waits in the #452 audit. Treat it as an order of magnitude.
- "cargo test" includes the compilation.

The study was a baseline: re-measure around 2026-11-18 to see what #452 changed.

## What to know before writing a test

- A test that compares two paths needs a **witness** that must differ.
- The interface language is a process global and tests run in parallel: a
  test that calls `set_language`, or asserts a text of a given language
  through `tr!`/`error_message`, takes `let _language =
  i18n::test_language(Language::English);` for its whole body (never a local
  mutex, never a bare `set_language`). A test that does not care takes nothing.
- A cap that looks like a bug is probably not one: the
  caps and counters already decided are documented where their
  constant is, and listed in
  `docs/archive/rustrewrite/questions-ouvertes.md` §11.
- **Screenshots** (`harness.snapshot`, `egui_kittest`, references in
  `crates/*/tests/snapshots/`): the references come from **Linux**. Another
  graphics driver rasterizes thin text differently at the pixel level (seen
  under Windows: `preferences_proto_analysis`, 1 pixel over the threshold out of 573
  differences). `kittest.toml` at the root sets the tolerance per system: Linux
  keeps the strict default (threshold 0.6, 0 pixels over the threshold), Windows and macOS
  tolerate `max_failed_pixels = 10`. A real regression touches
  hundreds. **Never** regenerate a reference (`UPDATE_SNAPSHOTS=1`) outside
  Linux: it would stop matching CI. A capture failure is a
  `panicked` at the `Drop` of the bench, not a crash; the test leaves `*.new.png` and
  `*.diff.png` next to the reference (not to be committed).
- **Captures in parallel under Windows**: two simultaneous wgpu instances
  crash the software rasterizer `d3d10warp.dll` (`0xc0000005`, #333; faulty
  module read in the Windows event log, three times, one of them on the
  user's machine). Any test that calls `snapshot` first takes
  `screenshot_lock()` (`app/preferences_proto.rs`); a new capture file
  must share this lock (move it to a common module).
- The interface language is a **global** state of the process
  (`i18n::set_language`): a unit test that must paint in a
  given language (`en`, `en-XA`) reruns in a subprocess with `STOKESFORGE_LANG`
  set, and checks that the child did paint in this language (model:
  `each_tab_draws_in_english_and_in_the_pseudo_locale`,
  `src/app/plot_settings.rs`, #119).
  Second user of the pattern: `src/app/layout_sweep.rs` (#521) sweeps the
  dialogs that open with a bare action (Graph Properties tabs, Preferences,
  Shortcuts, Formula Functions, About, Status history) and the shell in `en`,
  `fr` and `en-XA`; `en` and `fr` fail on a text that is elided or cut by its
  clip rectangle (known defects in `KNOWN_FAILURES`), `en-XA` only counts.
- A number typed for one layer and a pick of another layer in the combo of the
  Graph Properties window: `a_number_typed_for_one_layer_never_reaches_another_when_the_layer_is_picked`
  (`src/app/plot_settings.rs`, #521).
- An egui menu is tested really opened (click on its title), and is read
  in the accessibility tree that egui publishes (`ctx.enable_accesskit()`,
  `platform_output.accesskit_update`): label, disabled state, checked box
  and rectangle of each entry, without assuming a position or comparing a
  color (model: `entries`, `src/app/menus.rs`, #122).
- Accessibility (#170): layer badges and window handles
  publish their name and role through `Response::widget_info`. The tests of
  `graph_window.rs` read the number and the two selection states of the
  badges; those of `window_chrome.rs` read the eight directions of the
  floating and detached handles. Removing the publication of a surface must
  fail its test: an assertion on an empty list is not enough.
- In `stokesforge-app` and `stokesforge-app-state`, a test follows the code it tests (#115): a
  decision of `AppState` in `stokesforge-app-state/tests/shell.rs` (compiles without egui), a painted gesture in the module
  `tests` of its surface (`src/app/<surface>.rs`); that of `app.rs` tests
  only the orchestration (panel order, deferred application,
  confirmations, global shortcuts, window restoration). A test
  helper that several surfaces would copy (context with fonts and theme,
  egui image, key, three-frame click, painted text, `xy_columns`) lives
  in `src/app/test_support.rs` (#128), imported by name; a helper specific to
  one surface stays `pub(in crate::app)` in the test module that carries it
  (`graph_window::tests::graphe`, `project_manager::tests::click_at`). An
  assertion of emptiness ("the table is emptied") needs the witness that shows that
  the table was full before (model:
  `ctrl_n_confirms_and_forgets_the_views_of_the_previous_project`, #123).
- A test does not dispatch an action that writes the machine's `preferences.json`
  (`SetObjectManagerOpen`, `SetObjectManagerShowTexts`…): it sets the
  state in memory (`AppState::set_object_manager_open_for_test`), or only
  checks the pushed action. To exercise the setting actions (#139),
  `AppState::new_with_preferences` injects a `Preferences::load_from` on
  a file of a temporary folder, without modifying the global environment.
- Rule (#250): a test never writes to the real preferences. The unit
  tests of `stokesforge-app-state` have none (`default_path` returns `None`
  under `cfg(test)`); those of `stokesforge-app`, which build an `AppState` with
  `new`, have none either thanks to the `test-support` feature, which the
  dev-dependency of the app enables (`cfg(test)` does not cross the boundary of the
  crate, #263; removing the feature fails
  `stokesforge-app/tests/preferences_isolation.rs`); the integration tests
  (`tests/`) build their
  state with `AppState::new_with_preferences(…, Preferences::load_from(None))`,
  never `AppState::new` (guard in `stokesforge-app/tests/guards.rs`, which reads
  the two `tests/` folders). The write of the file
  is atomic (temporary + rename).
- No import template exists in production code (#378). Tests that need some
  copy `test/import/test_recipes.json` (a `RecipeStore` file) into their
  redirected configuration folder as `import_recipes.json`; the others leave
  it empty.
- A test does not read the machine's import templates or recent files either
  (`import_recipes.json`, `import_recent.json`): a saved template "Jasco"
  came before the template that recognizes the file and made the automatic choice fail
  (#195). It redirects the configuration folder to a temporary
  folder by setting `STOKESFORGE_CONFIG_DIR` **and**
  `XDG_CONFIG_HOME` (#332: `directories` honors only the latter,
  and only under Linux; under Windows the tests wrote to the real
  `%APPDATA%\StokesForge\config\` and fought over the file, "Access is
  denied") — under a lock if the test restores the values
  (`stokesforge-import/src/store.rs`, `tests/test_recipes.rs`,
  `stokesforge-app-state/tests/shell.rs`), or once per process
  before any bench (`hermetic_configuration`,
  `stokesforge-script/tests/script.rs`; `isolated`,
  `stokesforge-cli/tests/import.rs`). Single rule `config_dir`; pure forms
  `config_dir_from`/`recovery_dir_from` tested without `set_var`
  (`stokesforge-import/src/config_dir.rs`, proven mutants: empty filter,
  `recovery` suffix, variable ignored, `data_dir` replaced by `config_dir`);
  end to end, `the_isolation_redirects_the_config_folder_and_saving_writes_there`
  (`shell.rs`, proven mutant: `ProjectDirs` put back in `store.rs`).
  Known limit: `UserColors::load()` (`state.rs`) has no `cfg(test)`
  isolation, a test that saves a user color would write to
  the real `couleurs.json`.
- `stokesforge-clipboard` (#149): the decision of X11 answers (`x11::answer`) and the Windows/macOS format names are tested without a server; the X11 offer is tested against a **real server** (`tests/x11.rs`, `#[ignore]`, `DISPLAY` required, XWayland is enough: `cargo test -p stokesforge-clipboard --test x11 -- --ignored`). Windows is tested likewise against the **real clipboard** (`tests/windows.rs`, `#[ignore]`, interactive desktop required: `cargo test -p stokesforge-clipboard --test windows -- --ignored`; witness: a `windows_format_name` that returns the MIME for PNG fails the read-back of the "PNG" format). Wayland and macOS have no automatic test: Wayland requires a compositor and a real input (serial), macOS is only compiled (`cargo check --target …`). An X11 test of a recipient does not test `response_type` of a `SelectionNotify`: an event coming from `SendEvent` carries the bit 0x80.
- Tests and their comments are in English, like the rest of the repository.

**To read**: `stokesforge-robustness/src/lib.rs` (the index of the batteries), then the
head of the targeted `b*` file.

## Re-split of interface functions (#124)

The surface benches keep the same events and rectangles: menus,
grid, properties pages, import window, Object Manager and tab
strip. The added cases check in particular the range targeted by the type
menu, the keeping of the format parameters, the layer and series actually
edited, the independent collapse of the tree groups and the order of the callbacks.
The detached callback of the workbook collects its close before its
application by the root; a note and a matrix never detach (#473).

In `app.rs`, the extracted phases are also tested directly: a
confirmation empties the collected actions, stops the dispatch of those that
follow, and a plot command without effect opens no tab. In
`window_stack.rs`, hiding a graph keeps its place but skips content and
geometry publication; the visible witness finds them again.

The new cases are tested by temporary mutations removed before
delivery. The before/after comparisons of egui shapes (text, positions,
colors, clipping) complete the gesture tests; they do not replace an
interactive validation of detached OS windows or native pickers.

## Series groups (#196, #197, #209)

Gestures go through egui events, never through a direct call of the
`PanelSpec` method they trigger: a test that calls
`set_series_group` itself does not see the editor lose its series.

| Mutant (what is broken) | Seen by |
|---|---|
| `membership_editor`: the selection no longer follows the moved series (`*index = moved` removed) | `the_dropdown_joins_a_group_and_the_editor_follows_the_series` (`app/curve_settings.rs`) |
| graph: the "group selected" branch skipped, Del targets the first series | `shortcuts_of_a_selected_group_target_the_group` (`app/graph_window.rs`) |
| Ctrl+↑/↓ without the `can_move_group` bound | same |
| F2 no longer opens the group rename | same |
| `.stf`: empty group names kept on load | `an_empty_group_name_is_dropped_at_load` (`stokesforge-persistence/src/convert.rs`) |
| `.stf`: `#[serde(default)]` removed from `group_names` (old file unreadable) | `a_layer_without_group_names_reads_back_with_its_groups` |

## Copy/paste of cells (#259)

| Mutant (what is broken) | Seen by |
|---|---|
| `tsv::needs_quotes` always false (plain `join("\t")`) | `stokesforge-core/src/tsv.rs` (round trip tab, line break, quote); `a_multiline_text_cell_pastes_into_another_session_without_shifting_anything` (`stokesforge-app-state/tests/shell.rs`) |
| `decode` without quote handling | same + `a_single_multiline_text_cell_pastes_as_one_cell` |
| `decode` without `\r\n` | `a_windows_spreadsheet_ends_its_rows_with_crlf`, `external_clipboard_from_text_reads_the_spreadsheet_format` |
| `cell_copy_text_in` that removes `MISSING_TEXT_DISPLAY` afterwards | `a_typed_non_breaking_space_survives_copy_and_paste` |
| absent cell read as empty (`unwrap_or("")`, no more `value_block_keeping_absent_cells`) | `a_pasted_row_shorter_than_the_others_keeps_the_existing_cell`, `a_short_row_pasted_over_a_mixed_column_keeps_its_cell_in_one_undo` (#311) |
| present and empty cell treated as absent | `a_present_but_empty_pasted_cell_is_still_a_missing_value`, `pure_data_copy_paste_keeps_working_without_header_markers` |
| block not cut after the last present cell (column lengthened) | `a_short_last_row_does_not_lengthen_the_shorter_column` |
| status `status-paste-columns-dropped` removed / set without overflow | `a_paste_wider_than_the_sheet_reports_the_dropped_columns`, `a_paste_that_fits_the_sheet_width_reports_nothing` |
| `looks_like_thousands_separator` widened to `\d+,\d+` (or to `1,5`) | `the_thousands_form_is_recognized_exactly` (`stokesforge-core/src/number_format.rs`) |
| notice `status-cell-decimal-comma` removed, or set on paste | `typing_a_thousands_looking_number_in_a_numeric_cell_notices_the_decimal_comma`, `the_decimal_comma_notice_also_applies_to_a_mixed_cell`, `pasting_a_thousands_looking_number_does_not_notice` (#311) |
| discarded header rows not counted (`status-paste-fields-dropped`) / combined message broken | `a_header_field_paste_anchored_low_counts_the_rows_beyond_formula`, `a_header_field_paste_anchored_on_long_name_reports_nothing`, `columns_and_header_fields_dropped_together_give_one_combined_notice` (#311) |
| "2026" in Date/Time read as anything other than 2026 seconds | `from_texts_datetime_reads_a_bare_number_as_seconds` (`stokesforge-core/src/column.rs`) |

Paste replays the internal structure when the text is the one that was pushed:
to test the "external text" path, paste into **another** session.

## Cell input and paste: a single rule (#258)

| Mutant (what is broken) | Seen by |
|---|---|
| the widened block goes through `MixedCell::from_text` again (date lost, `inf` read as a number) | `from_texts_datetime_keeps_the_date_as_a_value_in_a_widened_block`, `from_texts_numeric_keeps_infinity_and_overflow_as_text`, `from_texts_on_a_whole_column_equals_cell_by_cell_for_the_three_types` (`stokesforge-core/src/column.rs`); `a_pasted_date_keeps_its_value_when_the_column_widens_to_mixed` (`stokesforge-app-state/tests/shell.rs`) |
| the `Mixed` branch reads `inf` / `nan` as a number (without `from_texts`) | `from_texts_mixed_reads_neither_infinity_nor_nan`; `typing_or_pasting_inf_and_nan_in_a_mixed_column_keeps_text` |
| `parse_f64_text` instead of `parse_input` | `from_texts_mixed_reads_neither_infinity_nor_nan`, `from_texts_on_a_whole_column_equals_cell_by_cell_for_the_three_types`, `typing_or_pasting_inf_and_nan_in_a_mixed_column_keeps_text` |
| guard `kind == DateTime` removed (`Numeric` reads dates) | `from_texts_numeric_does_not_read_dates` |
| empty cell read as `Text("")` instead of missing | `from_texts_empty_cell_is_a_missing_value_not_a_text`, `pasting_numbers_over_a_mixed_column_makes_it_numeric_again_in_one_undo` |
| repromotion after paste removed (at the three paste points) | `pasting_numbers_over_a_mixed_column_makes_it_numeric_again_in_one_undo`, `a_multi_column_paste_with_a_repromotion_is_one_undo`, `single_value_and_full_column_pastes_also_repromote_a_mixed_column` |
| repromotion called after `end_grouped_macro` (two history entries) | `a_multi_column_paste_with_a_repromotion_is_one_undo`, `undo_state_is_fresh_right_after_a_grouped_paste` |
| own macro opened even without repromotion | `a_paste_that_leaves_a_mixed_column_mixed_is_a_plain_entry` |

`SetCellsPromoting` (stokesforge-commands) is unchanged: only the block passed to it
now comes from `from_texts`.

## Composition of graphs (#257)

| Mutant (what is broken) | Seen by |
|---|---|
| pairing with the nearest X on the **right** (or the first on the left) | `each_y_takes_the_nearest_x_to_its_left_even_if_unselected`, `a_y2_pairs_with_x2_and_goes_to_the_second_layer`, `x_for_follows_the_same_rule_as_series_from_columns` (`stokesforge-core/src/compose.rs`) |
| `x_for` no longer distinguishes Y2 → X2 | `x_for_follows_the_same_rule_as_series_from_columns` |
| palette without rank offset (`start_index`, "line" series, rank of the panel in `pyst`) | `the_palette_follows_the_rank_with_the_offset_of_each_layer`, `row_series_take_the_palette_of_the_rank_and_the_given_name`; `several_columns_form_a_group_and_a_single_one_does_not` (`stokesforge-script/tests/graphs.rs`) |
| second layer confused with the line layer, or base layer looked up among the overlaid ones | `the_second_layer_is_the_two_axes_one_not_the_rows_one`, `an_overlay_layer_covers_the_last_ordinary_layer_of_its_cell` |
| `spec_has_x` without the "line" series; non-finite x' accepted | `the_graph_has_an_x_through_an_x_column_or_a_row_series`, `x_prime_columns_are_those_whose_value_is_finite` |
| `y3_and_y4_pair_with_their_x_and_go_to_their_layer`: Y3 paired with X2, or placed in layer 2 | `stokesforge-core/src/compose.rs` |
| axis line strip (#385): depth 0 excluded, strip dropped, tolerance doubled or not divided by the display scale, no clamp on a tiny plot, strip for `size == 0` or `offset > 0`, farthest edge in a corner, overlay mapped to Right, curve before axis on double-click | `the_axis_line_is_a_strip_inward_from_the_edge`, `no_strip_without_an_axis_area`, `the_nearest_edge_wins_in_a_corner`, `an_overlay_strip_maps_to_its_spec_sides`, `a_stacked_band_has_no_strip`, `a_tiny_plot_keeps_an_interior`, `the_tolerance_is_given_by_the_caller` (`hit.rs`); `a_pixel_picks_the_graph_component_underneath` (`geometry.rs`); `double_clicking_the_axis_line_opens_the_axis_even_under_a_curve` (`cache_session.rs`); `axis_gestures_open_the_right_layer` (`graph_window.rs`) |
| band offset ignored (axis of layer 3 on that of layer 2) | `the_axes_of_several_overlaid_layers_stack_outward` (`geometry.rs`), `an_offset_band_picks_only_its_layer` (`hit.rs`) |
| constructor `OverlaySpec::new` inverted, `TOP_X` that also shows Y | `named_layers_say_which_axes_they_display` (`stokesforge-plot/src/panel.rs`) |
| `PanelSpec::remove_series` that removes nothing or accepts an out-of-bounds rank | `remove_series_removes_the_designated_series_and_no_other`; `object_manager_deletes_a_curve_without_touching_the_data` (`stokesforge-app-state/tests/shell.rs`) |
| `pyst.plot(y)` without `x=` that keeps the row index despite an X column on the left | `plot_without_x_takes_the_x_column_on_the_left_by_role` (`stokesforge-script/tests/graphs.rs`, feature `python`) |
| `aux_for` that loses the paired X | `reference_candidates_exclude_the_processed_series_and_pair_their_x` (`stokesforge-app-state/tests/analysis.rs`) |

The app tests (`plot_columns_*`, `add_layer_*`, `plot_rows_*`) guard the
wiring: `compose` returns the series, the app sends them in one command.

### Low R² (#288)

`app-state/src/r_squared.rs` holds the levels (0.95 / 0.8 / 0.6 exactly,
negative, `NaN`). `app-state/tests/analysis_state.rs` (end of the file) holds the
box: opened on apply (values kept or not) and on automatic
recalculation, silent for a good R², preference off, calculation setting off,
preview, opening of a project, alert already there, several calculations. Proven
mutants: alert guard, preference, calculation setting (two places),
fingerprint, strict bounds.

Grouped box and badge (#290): `app-state/tests/analysis_state.rs` (end of the
file) holds the state "low R² of the project" — counted despite the calculation
setting off, the preference off, an alert already open; kept after the
box; absent for a "Show" calculation and for a good R²; back to 0 after a
recalculation that raises R² or the deletion of the calculation (and restored by undo);
counted per series; two types in the fixed order; list open/closed without
touching `alert`. `app-state/tests/low_r2_text.rs` holds the text (merge and
order of the groups, singular/plural fr+en, each type named). Proven
mutants: calculation setting that filters the badge, count per calculation instead
of per series, no refresh at the snapshot, order of the types,
automatic closing of the list. The painting of the badge (position on the right,
tint, click) has no test: only the action it pushes is guarded
by the state. The egui painting (bold, tint, bubble) has no
image test: only `theme::r_squared_color` is tested.

### Final replacement of a correction (#212)

`commands/tests/calculations.rs` holds the data/trace atomicity, undo/redo,
length and positions, version conflict, refusal of read-only columns
and contradictory corrections on a shared Y. `persistence/tests/calculations.rs`
guards the successive traces and the allocator after reload.
`session/tests/analysis.rs` checks that dependent calculations follow the
replacement and its undo. `stokesforge-app-state/tests/analysis_state.rs` exercises the
confirmation of shared sources; `app::form::tests` refuses the disabled
options in radios and lists. `script/tests/analysis.rs` checks the
return of the original handle and the Script macro with `replace_original=True`.

## Column used by a calculation (#306)

`stokesforge-app-state/tests/column_use.rs`. Each rule has its witness, checked by mutation.

| Rule | Witness |
|---|---|
| a formula that reads a deleted column is listed; one in which only a letter slides is not (#321: reading by identifier) | `deleting_a_column_read_by_a_formula_opens_the_list_then_close_does_nothing`, `sliding_letters_do_not_count_only_the_read_column_does` (the list of any formula as soon as a column leaves: red) |
| a column without a reader opens nothing | `a_column_without_a_reader_opens_nothing` |
| an analysis calculation is named by its family and its first output | `an_analysis_calculation_is_listed_by_its_family_and_its_first_output` |
| a calculation whose output disappears with the gesture is not listed | `deleting_a_workbook_takes_its_calculations_with_it_without_signaling_anything` (without the exclusion: red) |
| several columns, one list, a single Undo | `undo_gives_back_the_column_and_the_formula_in_one_step_and_several_columns_make_one_list` |
| Close gives nothing back | `deleting_a_column_read_by_a_formula_opens_the_list_then_close_does_nothing` |
| Ctrl+Shift+C asks the question only if a column has a formula; Values empties the Formula row | `ctrl_shift_c_on_a_formula_column_asks_the_question`, `ctrl_shift_c_without_a_formula_copies_directly` |

Limits: moving a column (#319) opens no window: the witness is `column_move.rs` ("Moving columns", further down); its effects on formulas are also held at the session level (`recalc_targeted.rs`); the hidden reference of a correction (`aux`) is excluded by construction, without a dedicated witness.

## Formulas by identifier (#321, #320)

A formula keeps its columns by identifier; letters exist only at the boundaries. Each rule has its witness, checked by mutation.

| Rule | Witness |
|---|---|
| letters to identifiers: refusal of an unknown letter, of the column itself, of `#REF`; the exponent is not a column | `stokesforge-core`, `formula::tests::from_letters_*` |
| `#REF` on display for a column outside the sheet | `a_column_outside_the_sheet_is_written_ref` |
| duplicating a sheet makes the copy read its own columns; a broken reference stays `#REF`; a read identifier is never reassigned on load | `project::tests` (`dupliquer_une_feuille_…`, `une_reference_rompue_…`, `from_tree_ne_reattribue_pas_…`) |
| an analysis that inserts its outputs in the middle does not change what a formula reads (#320); move, delete then undo | `analysis.rs` (`une_analyse_qui_insere_…`), `recalc_targeted.rs` (`deplacer_une_source_*`, `supprimer_une_source_donne_ref_…`) |
| `.stf`: `formula_parts` round trip, id outside the sheet loaded as `#REF`, out-of-bounds id refused, old `formula` key ignored | `roundtrip.rs` (`a_formula_*`, `a_legacy_letters_formula_…`) |
| input (#433): the field always writes; unknown letter, self-reading, modified `#REF` are kept as typed and framed (`formula_issue`), flagged while typing (`header_edit_issue`), never a syntax error while typing; unmodified `#REF` writes nothing | `shell.rs` (`a_formula_reading_its_own_column_is_flagged_while_typing_and_kept`, `a_ref_formula_is_kept_as_typed_when_its_text_is_changed`, `a_formula_naming_a_missing_column_is_kept_when_typed_and_when_pasted`), `formula_gestures.rs` (`badly_written_formulas_are_kept_framed_…`, `a_kept_formula_that_does_not_parse_tells_the_user_why`, `a_kept_formula_is_undone_with_the_old_formula_and_values`, `a_kept_formula_in_a_sheet_of_no_row_is_framed`, `a_formula_that_compiles_is_not_framed_when_its_names_became_ambiguous`). Witnesses by mutation: no `compile` branch in `formula_issue`, re-reading every formula, frame depending on the rows |
| keeping reading (#433): `A+Z` is `[Column(A), "+Z"]` and follows a move, `A*1e400` does not compile, strict `A +` is refused, own column and broken quote or bracket stay as typed | `sheet_refs.rs` tests (`a_kept_reading_…`, `a_kept_number_out_of_range_does_not_compile`, `the_strict_entry_refuses_…`); witnesses: whole text stored, no literal split, `compile()?` dropped |
| decimal comma (#433, S-R17): `max(B,1,5)` and `max(B,330,350)` rewritten with spaces and warned, orange for 10 s, not red; `max(B, 1, 5)`, `at`, `round`, `atan2`, `if` untouched | `formula_entry.rs` tests, `formula_gestures.rs` (`a_comma_range_is_rewritten_and_warned_not_framed`, `the_comma_rewrite_leaves_other_calls_as_typed_and_red`), `field_state.rs` (`a_rewritten_text_stays_orange_for_ten_seconds`), `grid.rs` (`enter_on_an_unreadable_formula_…`, `a_rewritten_comma_range_paints_the_cell_orange`); witnesses: space test dropped, range-form test dropped, 2 s constant, orange branch removed, Enter staying open on a refusal |
| empty cells (#433): an unparsable or blocked formula leaves empty cells on a Mixed column, no `---` | `recalc_targeted.rs` (`a_kept_unreadable_formula_on_a_mixed_column_has_empty_cells_not_dashes`, the cycle cases), `session/tests/formula.rs`; witness: statuses kept |
| binding (#433, Q3): a kept formula binds inside the history entry of the gesture that creates its column, sheet or workbook (add, move in, rename, delete a duplicate, load); undo unbinds, redo replays, one entry only, the label is the gesture's; the snapshot reports it once; `#REF+E` waits, a binding that would loop stays kept until the loop is broken, several bind in identifier order, a bound column moved later gives no `#REF`; a Stop undoes gesture and binding | `recalc_targeted.rs` (`a_kept_formula_binds_in_the_entry_of_the_gesture_…`, `moving_a_column_into_the_sheet_binds_…`, `renaming_a_sheet_binds_…`, `deleting_a_duplicate_workbook_binds_…`, `a_formula_with_a_ref_waits_for_all_its_names`, `a_binding_that_would_loop_…`, `several_kept_formulas_bind_in_identifier_order_…`, `a_bound_column_can_be_moved_without_a_ref`), `session/src/lib.rs` (`stop_undoes_the_gesture_and_the_binding_it_made`), `formula_structure.rs` (`a_kept_formula_binds_when_the_file_is_loaded`), `sheet_refs.rs` (`pending_bindings_come_in_identifier_order`), `commands/tests/history.rs` (`absorb_into_last_…`), `formula_gestures.rs` (`a_kept_formula_that_binds_says_which_column_it_now_reads_once`, `several_bindings_in_one_gesture_are_counted`, `a_kept_formula_typed_over_data_is_undone_with_the_data`). Proven mutants: binding written outside the history, binding as an entry of its own, no scan, scan only on `AddColumn`, scan skipped on `Move` (named case, `b22` and `b23`), no loop check, partial binding, `Tree::iter` order, load scan removed, `resync_pinned` removed, report not retracted on Stop, binding after the pass (Stop), no status message, no `values_before` |
| kept formula saved and reopened: same parts, same reason, still uncomputed | `formula_roundtrip.rs` (`a_kept_formula_round_trips_…`) |
| paste (#433, Q2 reversed): a formula that cannot be read is kept like a typed one (framed, column emptied, never the copied values, one Undo restores the data, binds when the missing object appears), the other columns go through, the status bar names the columns that do not compute then the first reason; Ctrl+Shift+C copies letters re-read in the destination sheet. Witnesses: strict reading, copied values written | `a_refused_formula_among_accepted_ones_…`, `a_pasted_unreadable_formula_is_kept_and_empties_the_column`, `a_pasted_kept_formula_binds_when_its_column_appears`, `several_unreadable_pasted_formulas_are_named_…`, `copied_formulas_travel_as_letters_…` |
| paste (#432): the body of a column whose formula the paste itself wrote (and accepted) is not written, so a successful Formulas paste does not end on the read-only error; the other columns keep their frozen values, a Formulas copy on a data cell (entry row included) acts as on the column's F(x)= field (S1), a Values copy on a data cell is unchanged, a refused formula ends on its own sentence, one Undo restores it all. Witnesses: removing the skip fails `a_successful_formulas_paste_does_not_end_on_an_error`; skipping the bodies of a whole Formulas copy fails `a_formulas_paste_of_several_columns_keeps_the_values_of_the_ordinary_ones`; not remapping a data-cell anchor fails `a_formulas_paste_on_a_data_cell_*`, skipping the entry row fails `..._on_the_entry_row_*`, remapping only formula columns fails `..._of_two_columns_on_a_data_cell_*`, remapping Values copies fails `a_values_paste_on_a_data_cell_is_unchanged` | `formula_gestures.rs` (`a_formulas_paste_*`, `a_values_paste_onto_a_formula_column_*`, `a_refused_formula_among_accepted_ones_*`) |
| `pyst`: letters on read and write, every bad formula (`A +`, unknown letter, `#REF`, loop...) raises and writes nothing, a comma range stays as typed, another field does not touch the formula | `script/tests/navigation.rs` (`les_formules_se_lisent_…`), `mutations.rs`, `formulas.rs` (`every_badly_written_formula_raises_and_writes_nothing`) |

Limit: the red frame, the orange state and the bubble (grid) are tested on the painted shapes of the bench, not on a real window: the decision (`header_edit_issue`, `formula_issue`, `formula_entry::read`) is tested without one, the painting is re-read on screen.

## Column width (#325)

`NodeId -> f32` table of the actor, outside the model and outside the history, saved in the `.stf`. Each rule has its witness, checked by mutation.

| Rule | Witness |
|---|---|
| setting a width advances only `geometry_revision`; the same width, NaN, ≤ 0, ∞: ignored | `session/tests/column_widths.rs` `setting_a_width_only_advances_geometry_revision` (going through `revision`: red) |
| the width of a deleted column comes back with Ctrl+Z | `the_width_comes_back_with_the_column_after_undo` (purging widths at the mutation: red) |
| `.stf` round trip; `NewProject` empties | `the_widths_make_the_file_round_trip` (not writing the field: red) |
| writing does not emit the key of a vanished column | `saving_does_not_write_the_width_of_a_deleted_column`, `persistence` `save_drops_dead_keys_and_invalid_widths` (removing the filter: red) |
| file without the field → empty table; orphan key, NaN, ∞, 0, negative discarded one by one; identifier > `MAX_NODE_ID` refused | `persistence` `a_file_without_the_widths_block_*`, `hostile_column_widths_*`, `a_column_width_key_above_the_id_ceiling_*`, `robustness/b3` `b3_hostile_column_widths` (reading without a filter, without `check_node_id`: red) |
| the state sets the width through the actor without history, including a detached view | `app-state/tests/column_width.rs` |
| the `.stf` gives its width to the startup snapshot | `main.rs` `startup_restores_the_column_widths_of_the_file` |
| the dragged handle pushes the width only on release | `grid::tests::a_manual_resize_sends_the_width_at_the_end_only` |
| opening and painting sends nothing, does not mark the project modified | `opening_the_worksheet_sends_no_width` (pushing the rendered width at every frame: red) |
| Add Column: widths kept, the new one at 54; Delete then Ctrl+Z | `each_column_width_survives_adding_a_column`, `delete_then_undo_gives_the_column_and_its_neighbors_their_width_back` (width by position: red; without `reset()` when the list changes with equal count: red) |

| the default width is the same floating, maximized, then restored (#337) | `window_stack::tests::the_default_width_of_a_column_is_the_same_floating_and_maximized` (not forgetting the table state after the measurement pass of the `Window`: red, 58 instead of 62) |

The width is read in the bench on the flat fill of the column letter (`painted_width`). The grid bench paints `book_contents` outside any `Window`: it does not see the measurement pass, hence the test at the level of `show_workspace`.

### Reset to default (#330)

| Guard | Test (witness) |
|---|---|
| reset: key removed, `geometry_revision` +1 only if it existed, neither `revision` nor history | `session/tests/column_widths.rs` `resetting_to_default_removes_the_key_and_advances_only_if_it_existed` (unconditional `+= 1`: red) |
| reset then save: no more key in the file | `a_column_reset_to_default_is_no_longer_in_the_file` (setting 54 instead of removing: red) |
| the action removes the width from the local copy before refresh | `app-state/tests/column_width.rs` `the_reset_action_removes_the_width_without_history` (key kept in the local copy: red) |
| B reset to 54, A and C keep theirs | `grid::tests::resetting_a_column_to_default_does_not_touch_the_others` (widths outside the layout key: red) |
| the gutter keeps its width after a drag and a column reset, returns to 90 for the sheet | `a_column_drag_keeps_the_gutter_width` (gutter not remembered: red) |
| entry after Role ▸, disabled without a width, whole selection | `the_reset_entry_follows_role_and_is_grayed_out_without_a_width`, `the_reset_handles_the_whole_selection` (`add_enabled(true)`: red; targeting only the clicked column: red) |
| sheet entry: tab, Workbook ▸ Sheet, Workbook ▸ Columns | `the_tab_right_click_resets_the_whole_sheet`, `menus::tests::the_width_reset_is_in_workbook_sheet_and_columns` (entry always active, or a single column targeted for the sheet or the tab: red) |

## Moving columns (#319)

Submenu `Move ▸` (First · Left · Right · Last). Each rule has its witness, checked by mutation.

| Rule | Witness |
|---|---|
| Left/Right keep the gap (`A [B] C [D] E` → `B A D C E` / `A C B E D`), First/Last gather in order | `column_move::tests::left_keeps_the_gap_*`, `first_and_last_gather_*` (tightening the block: red) |
| all or nothing: `[A] C` and `[A] [C]` Left → `None`; gesture without effect → `None` | `a_column_that_cannot_move_greys_*`, `a_gesture_that_changes_nothing_*` (moving only C: red) |
| the replayed `Move` sequence (index after removal) gives the target order, all selections of six columns, four directions; only the selected columns move | `the_steps_reach_the_target_order_for_every_selection` (index +1: red) |
| an `Unknown` child keeps its place, index in the full list | `an_unknown_child_keeps_its_place_*` (index = visible rank: red) |
| a single Undo for several columns, label "Move Columns"; one column: `Move “B”` | `app-state/tests/column_move.rs` `deux_colonnes_deplacees_*` (without a macro: red), `une_colonne_seule_*` |
| no "Column in use" window, formula values unchanged | `deplacer_une_colonne_lue_par_une_formule_*` (calling `open_column_use`: red) |
| the status bar says the label of the **group** ("Move Columns") at closing, like undo, for any group closed by `end_grouped_macro` (the last step does not stay displayed); a single column keeps `Move “B”` | `la_barre_d_etat_dit_le_libelle_du_groupe_*` (removing the replacement in `end_grouped_macro`: red) |
| the selection covers the same columns after the gesture | `the_selection_stays_on_the_moved_columns` (emptying the selection: red) |
| `Move ▸` between the clipboard and `Type ▸`, four entries; all-or-nothing disabling (A alone, C alone, A and C, whole rows); the active entry pushes `MoveColumns` | `grid::tests::move_est_place_*`, `les_entrees_de_move_sont_grisees_*`, `une_entree_de_move_pousse_*` (disabling removed, entry removed: red) |
| same submenu and same disabling in Workbook › Columns | `menus::tests::le_menu_colonnes_porte_move_*` (removing `Move ▸` from the shared painting: red) |
| the width follows the moved column (#325) | `grid::tests::the_width_follows_the_moved_column` (storage by position: red) |

Limit: the look of the submenu and the text of the bubbles are re-read on screen.

## Constrained polynomial on a shifted X (#296)

`fit::tests::constrained_polynomial_on_very_shifted_x_is_flagged`: flagged (`shifted_x`) for a fixed polynomial or one with an initial value on X ≈ 1e4; unflagged witnesses: centered X, moderate shift (1e2), unconstrained polynomial (centered QR). `.stf` transport: `shifted_x` in `persistence/src/calculations.rs` (round-trip and frozen-name tests).

## Demo project (`demo/`)

**Startup (#333).** `stokesforge-app-state/src/startup.rs` (`tests`): the
decision first opening / later launches / forced demo /
existing or absent path, without a window; `preferences.rs`: the marker
`first_run_done` (absent or unreadable = to do, written while keeping the unknown
fields). `main.rs` (`startup_tests`, `cargo test -p stokesforge-app --bin
stokesforge-app`): `demo.stf` written then read back without discarding anything, in-memory fallback
if the folder cannot be created, empty project without a workbook. Witnesses
proven by mutation: the file write removed, the marker ignored.
`scripts/app-dist.sh` and `app-dist-windows.sh` give a throwaway configuration
folder to the packaged window and then require `demo.stf` and the
marker: the first production opening is tested at each
distribution.

`crates/stokesforge-app/src/demo/tests.rs` (module of the **binary**: `cargo test -p stokesforge-app --bin stokesforge-app`). Each test has its witness, checked by mutation (seed changed, title given to a graph, column deleted under a graph, amplitude of the Gaussian set changed, `recalculate` removed from `session_for_project`): unique window names across the whole project, no graph title (#103), each graph resolved without a broken reference and drawn, logarithmic axes on strictly positive data (invariant 10), each analysis set finds its known parameters through `core::compute` (`Expected`, tolerance), the eight families of `OperationKind::ALL` covered (including smoothing, `Smooth` set), noise with a fixed seed pinned, formula columns computed from startup, `.stf` round trip through the session (clean report, same specs), all `PlotType` (without `Unknown`).

## Generated manual pages (#364)

Two pages of the manual's Reference part are generated, never edited by hand:
`manual/src/reference/shortcuts.md` (from the help table `HELP` of
`app/shortcuts.rs`, keys spelled with Ctrl in `<kbd>`) and
`manual/src/reference/interface-texts.md` (English and French labels of menus,
context menus, window and dialog titles, tabs and toolbars, read from the `en` and
`fr` catalogs). The generator is the test module
`crates/stokesforge-app/src/app/manual_pages.rs`. The ordinary test
`the_generated_manual_pages_are_up_to_date` rebuilds both pages and fails when a
committed page differs, naming the command to run; `the_generated_pages_cover_the_help_and_the_menus`
checks that every `HELP` row and the main menu entries are in them. After changing a
shortcut or a menu, window or tab label:

```sh
UPDATE_MANUAL_PAGES=1 cargo test -p stokesforge-app --lib manual_pages
```

and commit the regenerated pages with the change. The French column is exempt from
`scripts/french-guard.py` by the `<!-- lang: fr columns: French -->` marker above the table.

The same module keeps the **inline icons** of the manual (#367). A page shows an
icon with `<img class="icon" src="../images/icons/FILE.svg" alt="">` before the bold
label of the button. The files of `manual/src/images/icons/` are byte-identical copies
of `crates/stokesforge-app/assets/icons/FILE`, plus a copy of the icons' `LICENSE` (the
MIT license asks for the notice with copies). The test
`the_manual_icons_are_copies_of_the_application_icons` scans `manual/src/**/*.md` for
`images/icons/<file>` and fails when a referenced icon does not exist in the assets, or
is missing or different in the manual, when a copy is no longer referenced, or when
`LICENSE` differs. The command above also copies and removes the files. The style of
`img.icon` (size, background on the dark themes) is `manual/icons.css`, loaded by
`additional-css` in `manual/book.toml`.

## Manual screenshots (#365)

**When**: at the merge, by `scripts/deliver.sh` (#515), never during work or retour rounds. If the branch touches `crates/`, `assets/`, `manual/screenshots.json` or `Cargo.lock`, deliver runs `scripts/manual-screenshots.sh --no-python` (all shots except the Python console one) in the branch worktree before taking the main lock, commits the changed images and stamps that commit. A failure refuses the delivery. A NEW shot is the exception: render it on the branch with its name filter to look at it.

The images of the user manual (`manual/src/images/*.png`) are not taken by hand:
`scripts/manual-screenshots.sh` renders the real application headless on the demo
project (`egui_kittest` + wgpu, English, light palette, 1440 x 780 points at 2x)
through the ignored test `render_manual_screenshots`
(`crates/stokesforge-app/src/app/manual_screenshots.rs`, table `SHOTS`). It is local
only: it needs a GPU adapter (or a software one such as lavapipe), so it is not in
the CI.

- **Reproducible**: the same code gives the same pixels (kittest renderer options
  `PREDICTABLE`, fixed demo, temporary configuration folder, pointer sent away, no
  hover or caret). A PNG is rewritten only if its decoded pixels differ from the file
  on disk, so a second run changes no image; `git status` then lists what really changed.
- **Manifest**: `manual/screenshots.json` has one entry per image (`image`, `page`,
  `shows`, and `"feature": "python"` for the console). The ordinary test
  `the_manifest_and_the_shots_agree` keeps the entries equal to `SHOTS` and checks
  that their files exist.
- **Not illustrated**: what depends on the operating system (system file chooser, file
  dragged from the file manager, detached OS window) has no screenshot.
- **Python console**: `scripting-console` needs the embedded interpreter. The script runs
  the bench a second time with the `python` feature (environment of `scripts/python-test.sh`);
  without the vendored interpreter it skips that shot and says so.
- **Dialogs**: a crop is exactly the window frame (`dialog_rect`). A dialog whose content
  scrolls is made taller with `grow_dialog`; the analysis values list has a test seam
  (`VALUES_HEIGHT_OVERRIDE`). Fit and peaks shots add a fixed pseudo-random scatter to the
  spectrum (an exact curve gives 17-digit values).
- **Adding an image**: one entry in `SHOTS` (setup through `Action`s and labelled widgets,
  never pixel coordinates), one entry in the manifest, then the script, then look at the PNG.

## Analyzing rows (#253)

One layer, one file, one witness tested by mutation:

- `stokesforge-core/tests/transpose.rs`: creation (name, place, columns, metadata),
  reuse of a row, short column, refusal naming the letter without creating anything,
  text, name collision, derived states (stale: value, x', column added or
  deleted; modified: only a value edit), update (new version,
  deleted column recreated), life of the link (rename, deletion, copy). Mutants:
  `stale` forced to false, fingerprint of the rows ignored, `prune_transposes` removed.
  Since #463 (box "Recalculate automatically", checked by default) the same
  file holds `follow_transpose` (rewrites a changed source only when checked,
  keeps the version of a column that comes out the same, never overwrites a typed
  value nor recreates a deleted column, names the unfollowable column) and the
  rule that the source may not calculate from its followed sheet (direct
  formula, label column, chain through another sheet, analysis landing in the
  source; another sheet reading it stays allowed). Mutants: default `false`,
  `overwrite` instead of a comparison, `modified` ignored, link edge dropped
  from `column_dependencies`. The link cases of `formula_structure.rs` below
  uncheck the box in their fixtures (`uncheck_auto`): they pin the snapshot.
  New cases there, both modes: the sheet follows and one Ctrl+Z takes both back,
  the unchecked sheet stays a snapshot, a typed value stops the whole sheet until
  Relink or Ctrl+Z, a retyped identical source value leaves the rows untouched,
  a deleted Row column stays deleted, an unfollowable source keeps the numbers
  and resumes, a change travels source formula → followed row → formula of
  another sheet → automatic fit in one pass, a sheet out of date in the file is
  not rewritten until its source changes (load hold), a stored forbidden formula
  is blanked and blocks following; `commands/tests/transpose.rs`: the toggle is
  one entry, rechecking is refused with no entry, a macro over a followed link
  column finishes (`is_derived`), Recalculate and Analyze Rows are refused with no
  entry on a checked sheet with a reader and allowed once unchecked or cleared (#484); `persistence`: round trip of both states, a
  record without the field is unchecked; `b3`: a followed sheet read by its own
  source does not loop.
- `stokesforge-session/tests/formula_structure.rs` (#430, #439, links over formula columns,
  both modes): up to date after save, load and unrelated commands (stale again once the
  source is edited); stale in the file stays stale; formula values that differ from the file's
  make it stale; up to date in the file stays so after load and an unrelated command, in
  `Full` too; unrelated edit undone after a load. #439: `Transpose, AddFolder, Transpose, Undo`
  and `…RecalculateTranspose, Undo` take back the folder (a no-op stores no entry);
  writing the same value by a command is stale and Recalculate clears it; change then Ctrl+Z, or put back
  by hand, stays stale (accepted limit); a formula recomputed to the same numbers (reads
  another sheet) is not stale, to other numbers it is; zero to negative zero is written with
  a fresh version; the pass keeps the number of a formula column recomputed to the same
  content and restores a damaged one with a new number, the twins numbering alike.
  Mutants: the pass always writes (`transposing_again_…`, `recalculating_an_up_to_date_…`,
  `a_formula_recomputed_to_the_same_numbers_…`, the load cases, nine "targeted" cases,
  `b22`, `b23`); the pass compares lengths only; IEEE comparison instead of bits
  (`a_formula_going_from_zero_to_negative_zero_…`); "no version on an identical write"
  widened to `Project::set_values` (`writing_the_same_value_…` in `formula_structure.rs` and
  `stokesforge-app-state/tests/row_analysis.rs`; the mutant must also undo the version bump
  that `Column::set_values` makes itself); `FormulaRecalc::Targeted => true` in
  `is_stale` (the moved witnesses and `b22`, through the evaluation count); evaluation count
  never incremented. `b23`: a reload keeps the state (stale, modified) of every link of the
  targeted twin, and its draw transposes a sheet holding a formula column.
- `stokesforge-commands/tests/transpose.rs`: one undoable entry per command, refusal without
  an entry, `AnalyzeRows` group undone as a block; `tests/labels.rs` and
  `stokesforge-app-state/tests/history_text.rs`: the two labels.
- `stokesforge-persistence/tests/transposes.rs`: round trip of the links and derived states;
  `hostile_links_are_dropped_one_by_one` (orphan link, out of bounds).
- `stokesforge-app-state/tests/row_analysis.rs`: a single Ctrl+Z undoes sheet and calculation,
  Cancel leaves nothing of it (and no group left open), refusal by letter, second row
  in the same sheet, graph curve, confirmation before overwriting. Mutants: the group
  abort on Close and its closing on Apply.
- `stokesforge-app/src/app/transpose_ui.rs`: the embedded font carries "ᵀ"
  (proportional only: the monospace one does not have it), the strip and its buttons, the
  confirmation.
