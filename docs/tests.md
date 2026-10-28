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
file, and a GPL-2.0-only dependency outside the policy. These
probes use temporary directories and do not modify the sources.

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
[`licence-et-extensions.md`](licence-et-extensions.md).

## Three families

| Family | Where | What it guards |
|---|---|---|
| **unit tests** | `#[cfg(test)]` in `src/` | a local behavior, as close as possible to its code |
| **integration tests** | `crates/*/tests/` | a crate property: public contract, boundary, agreement between paths |
| **robustness bench** | `stokesforge-robustesse/tests/` | the foundation at its limits — 22 batteries `b1`…`b22` |

## Guards not to break

A few tests do not check a behavior but **an architecture property**. They
fail on purpose when someone crosses a boundary by accident, and they are the
ones to understand before changing them.

| Test | What it holds |
|---|---|
| `*/tests/no_ui_dependency.rs` (eleven crates, including `stokesforge-app-state` since #263: no `egui`, no `eframe`, no `winit`, no `stokesforge-clipboard`) | dependency allow-list: every addition to `Cargo.toml` must be added to it explicitly (invariant 2). Shared reading `stokesforge-arch-guard`: normal, build, `target.*` and renamed (`package`) dependencies; `dev-dependencies` excluded; negative witnesses in `stokesforge-arch-guard/src/lib.rs` |
| `stokesforge-persistence/tests/witness_files.rs` (S4, #323) | the file `tests/witnesses/v1/project.stf` is read back to give exactly the witness project (all node, column and series types, non-default values), and saving the project gives back its bytes: the written format does not drift without a decision. Witness: renaming a field of `schema.rs` or `recipe_record.rs` fails both tests. Regeneration: see `persistance.md`, "Version rule: every V1 file can be read back up to V3" |
| `stokesforge-arch-guard/tests/skill_references.rs` (F1, #301) | in all project skills (`.claude/skills/stokesforge-*` and `run-stokesforge`, `SKILL.md` and `references/*.md`), every reference "`fichier.md` § Title" names a file and a heading that exist, a `§` without a path is refused; every link `](references/x)` of a `SKILL.md` points to an existing file and no file of `references/` is orphaned. Witnesses on fake trees (missing heading or file, `§` without a path, dead link, orphan, third-party skill ignored) |
| `stokesforge-arch-guard/tests/standard_codes.rs` (#301) | every `S-XN` code cited (docs, skills, comments, tests) appears in the rule index of `docs/standard-interface.md`; witness on an unknown code |
| `stokesforge-arch-guard/tests/config_dir.rs` (#332) | `ProjectDirs::from` is called only by `stokesforge-import/src/config_dir.rs` (comment lines ignored): every profile path goes through `config_dir()`, which tests redirect with `STOKESFORGE_CONFIG_DIR`. Witness: a fake source that calls it is refused, a comment is not; proven mutant: the call added to `recent.rs` fails the guard |
| `stokesforge-render-svg/tests/no_ui_dependency.rs` | the export sees neither `egui`, nor `eframe`, nor `winit` — so it can be tested without a graphics server |
| `stokesforge-render/tests/measurement.rs` | the single source of text measurement (invariant 4) |
| `stokesforge-render/src/{view,interact}.rs` (modules `tests`) | interaction without egui (#262): framing of a gesture (zoom, pan, log scale, inverted axis, overlaid layers, axis alone, unknown panel), reset, nearest point (curve, out of range, hidden or right-hand series, layer of the spec); proven mutants (pan sign, box filter, linked layer, radius, hidden series); geometry of the analysis preview (#261): `PanelTransform::new` (fractional origin, inverted log, zoom, rectangle), initial frame (`fit_range`, `axis_frame`), `snap_to_curve` (n, edge, `ys` too short) and `snap_to_curve_zone` (averaged zone: odd and even n, truncated edge, unsorted x values, #334) and `scale::axis_ticks` (same points as the pipeline, decades, comma, bound without an image); proven mutants (rounded origin, `1 - fy`, `> 0` filter in log, half window, `label_count` fixed at 5, forgotten comma, `abs` of the right-click tolerance, `.min(ys.len())`, half window of the zone, zone bounds taken at the extremities); snap circle (#334): `snap_radius` (one point = 6 px, half side, asymmetric zone, inverted X axis, cap, non-finite pixel) and the preview bench (the circle grows with N, text = real number of points at the edge); proven mutants (`abs`, `max` → `min`, floor, cap, hard-coded radius 6.0, N instead of the real number); `app/analysis_preview.rs` (`zone_tests`): zone drag, right-click on an inverted X axis; preview navigation (#270, `Banc` bench frame by frame): wheel alone has no effect, Ctrl + wheel and pinch (data under the pointer kept), zoom per axis, Shift + wheel = average without zoom, Ctrl + drag (frame translated, no zone, inverted X axis, per axis), Ctrl + click without a point, double-click reframes, peaks (click = seed, drag = range); proven mutants (pan sign, wheel alone, `!command` of the click, axis bounds, drag mode, wheel reading under Shift, inverted factor, double-click); `app/form.rs`: surrounded texts (`toggle_texts`: solid background of the current option, toggle, click on the current one); `stokesforge-app-state/tests/analysis.rs`: `tool_choices` (only peaks have the choice) |
| `stokesforge-render-egui/tests/agreement.rs` | screen and export place the same primitives at the same pixels; ticks inside / on both sides / cut / minor ticks on the four sides (#344), each different from the default figure; proven mutant (`ticks` ignored) |
| `stokesforge-render-svg/tests/mirrors.rs` | "Advanced" settings of the tick marks (#344): major tick mark at 10 against 5 by default, minor Auto = half of the effective major, color of the tick marks only (line and numbers keep theirs), thickness, lengths NaN / negative / 1e30 and thicknesses 0 / NaN / 1e9 bounded without panic; proven mutants (`custom_ticks` without the thickness, half fixed at 3, color ignored, thickness ignored, length not read); mirror axes (#176) and ticks (#344) seen in the export: bare line (line kept, no ticks, no numbers), inside / both sides (outer half = Plotters tick), numbers unchanged without ticks, neighbor left at default; default figure identical to the byte; proven mutants (`ticks` ignored, sign of the direction, asymmetric `Both`, length, gap of the numbers) |
| `stokesforge-render-egui/tests/{interaction,cache_session}.rs` | graph gestures replayed frame by frame (#318): plain drag has no effect (framing, generation, cache), Ctrl or middle click + drag = pan (background, axis alone, Zoom and shape tools, above a text, title or shape), Ctrl read at the press, Ctrl + click without text or shape, feathering cut only during a real gesture; proven mutants (pan on plain drag, middle button, object guard, rectangle guard, text and plot guard, feathering) |
| `stokesforge-render-egui/tests/feathering.rs` | feathering does not enter the cache key (invariant 6) |
| `stokesforge-app-state/tests/script_bridge_lazy.rs` | the Python interpreter does not start when the window opens |
| `stokesforge-app/tests/libelles.rs` | no hard-coded interface text (`aucun_libelle_en_dur`, absolute guard, no exemption, #48), including the `set_title` titles and visible `add_filter` names of file boxes (#127; technical extensions and format acronyms preserved) and the free functions `section_title`, `section_heading`, `table_header`, `secondary_text`, `bold`, `list_row`, `list_row_text` (#169, #170); the #43 zone (`ZONE_43`) with no literal at all, its functions looked up by name in all sources — a missing entry fails; the translation dependencies stay in `stokesforge-app` and `stokesforge-app-state`; `aucune_cle_du_catalogue_n_est_morte` (a key of `en` unused in the sources of both crates, witness `la_garde_des_cles_mortes_lit_les_deux_crates`); the `history-*` keys used; `notice`/`fail` of `app-state/state.rs` read (`la_garde_lit_les_messages_d_etat_du_crate_d_etat`); the witnesses that prove the guard sees a literal, including in a subfolder of `src/` |
| `stokesforge-app/tests/gardes.rs` | no bare `egui::Window` (`aucune_fenetre_n_est_une_egui_window_brute`); the properties window goes through `modal_dialog` → `modal_dialog_with` → `modal_frame` → `show_window_in_layer` (like `modal_wizard`, the import wizard, `modal_live_settings` and `modal_read_only`, #228, then `modal_confirm`, `modal_save_changes` and `modal_alert`, #229); no global import (`use …::*`) outside test code (`aucun_import_global_hors_des_tests`, decision 2 of #115, absolute since #117); no `ui.separator()` in a menu outside the section building block (`aucun_separateur_brut_dans_un_menu`, #155, absolute); egui selectable button only in a menu, a drop-down list or the color picker (`aucun_bouton_selectionnable_hors_d_un_menu`, #169); no color `Color32::from_…` or named tint outside `theme.rs` and `color_picker.rs` (`aucune_couleur_en_dur_hors_du_theme`, #169); no `ui.separator()` just before a section title or a text level (`section_heading`, `table_header`) (`aucun_separateur_avant_un_titre_de_section`, #169, #170); no `bold(…).small()` outside `theme.rs` (`aucun_gras_petit_hors_du_theme`, witness `la_garde_du_gras_petit_voit_un_bold_small`, #170); neither `DragValue` nor `Slider` (`aucun_champ_numerique_hors_de_la_brique`, #220, absolute); no bare bubble (`on_hover_text`/`on_hover_ui`) on a box, a radio, a text or number field, a field label outside a menu (`aucune_bulle_nue_sur_un_champ_de_formulaire`, #225, absolute — the reminder goes through `help_beside`/`help_text`); every `add_enabled` carries `.on_disabled_hover_text` (`aucun_element_grise_sans_sa_raison`, #226, absolute); no `add_space(<nombre>)` outside `theme.rs` (`aucun_add_space_litteral_hors_du_theme`, #227, absolute — `theme::SPACE_ITEM` / `SPACE_SECTION`); a `rfd::` only in `file_picker.rs` of `stokesforge-app-state` (`aucun_selecteur_rfd_hors_du_module`, #192, witness `la_garde_du_selecteur_voit_un_rfd_hors_du_module`); no integration test (of the two `tests/` folders) calls `AppState::new` (`aucun_test_d_integration_n_utilise_appstate_new`); the old guard "state modules do not name egui" (`MODULES_SANS_EGUI`, #255) is removed: since #263 the compiler and `stokesforge-app-state/tests/no_ui_dependency.rs` hold the boundary; no free size of an interactive window without `// TAILLE-LIBRE(#300): <justification>` in the 12 preceding lines (`aucune_taille_libre_sans_justification`, #300, S-F11, witness `la_garde_des_tailles_voit_une_taille_libre_nue`, proven mutation: marker removed); every `stokesforge_…` identifier passed to a `modal_*` appears in `docs/fenetres.md` and every line of the catalog has its call, without reading the sizes (`le_catalogue_des_fenetres_et_le_code_concordent`, #300, witness `la_garde_du_catalogue_voit_un_ecart`, proven mutation: catalog line removed); the witnesses of the source reader |
| `stokesforge-app/src/app/dialogs.rs` (module `tests`) | window natures (#300, S-F11): `chaque_nature_fixe_son_pied_sa_taille_et_son_focus` (table nature → footer, default class Small 480 x 170 / Medium 480 x 420 / Large 640 x 500, focus; the field of the variant wins), `le_focus_initial_suit_la_nature` (confirmation → Cancel, save and alert → primary, never the content field; read-only → nothing; witness: the draft goes to the field), `un_pied_grise_montre_sa_raison_et_non_sa_bulle` (`DialogFooter::gated` / `always`); S-F8 witness kept (`un_dialogue_rouvert_reprend_sa_taille_et_sa_place`); proven mutants (Small and Medium swapped, confirmation moved to the first field) |
| `stokesforge-app/src/theme.rs` (module `tests`) | no `ScrollStyle` outside the theme (`aucun_scroll_style_hors_du_theme`); no egui `strong()`, all bold through `theme::bold` (`aucun_strong_d_egui_hors_du_theme`, #169); `bold` really widens the text with the embedded fonts; three text levels (#170): `chaque_niveau_de_texte_pose_graisse_et_taille` (section title bold at normal size, table header bold at small size, secondary dimmed at small size) and `section_title_peint_le_niveau_titre_de_section`; proven mutants (`.small()` removed from the header or the secondary, bold removed from the title) |
| `stokesforge-app/src/app/{list_row,object_manager_ui,project_manager,curve_settings,import_wizard}.rs` (tests) | single row height and step trail (#170, points 5 and 8): `toute_ligne_de_liste_a_la_hauteur_unique` (with or without an icon, selected or not = `LIST_ROW_HEIGHT`, a bare label is lower), `la_rangee_composee_prend_la_hauteur_unique_et_la_rend`, `le_fond_reserve_impose_la_hauteur_minimale`, `groupe_serie_et_objet_ont_la_meme_hauteur_de_ligne` (text centered in the row), `un_dossier_a_la_hauteur_d_une_fenetre` (constant step folder/window), `la_ligne_serie_a_la_hauteur_unique_avec_sa_case`, `le_fil_d_etapes_n_a_plus_de_chevron_de_texte`; proven mutants (height not imposed on the row, on the folder header, on `list_row_sensed`, text chevron restored) |
| `stokesforge-app/src/{theme,app/file_drop,app/graph_window}.rs` (tests) | single drop stroke (#170, point 2, S-X5): `le_trait_de_depot_est_le_vert_de_l_accent` (color = accent, green dominant), `le_survol_d_un_fichier_entoure_la_fenetre_et_le_classeur_survole` (window and workbook at the `theme::drop_stroke` stroke, nothing without hover), `deposer_des_colonnes_sur_le_graphe_ajoute_une_serie_et_ailleurs_rien` (same stroke on the graph); proven mutant: blue put back in `drop_stroke` |
| `stokesforge-app/src/color_picker.rs` (tests) | "Square \| Wheel" as tabs (#170, point 7): `carre_et_roue_sont_des_onglets_soulignes` (accent stroke under the active tab and not under the other, no `selection.bg_fill` background behind a label, click on Wheel changes the view and keeps `stokesforge_picker_shape`); the fallback test (`le_selecteur_est_replie_par_defaut_et_se_deplie_a_la_demande`) still reads "Square" as collapsed text; proven mutant: `selectable_value` restored |
| `stokesforge-app/src/app/graph_window.rs` (tests) | axis bounds published to the properties panel (#170, point 1, delivered by #221): `les_bornes_publiees_sont_celles_du_trace_en_ordre_croissant` (inverted axis plotted backwards, published bounds ascending, unknown layer = `None`), `les_champs_de_bornes_grises_montrent_les_bornes_et_leur_raison_au_survol` (logarithmic axis from 1 to 1000: "1000" shown, disabled, and `plot-axes-auto-range.disabled` on hover only); proven mutants: `ascending` removed, `on_disabled_hover_text` removed, published bounds replaced by the logarithms |
| `stokesforge-app/src/app/project_manager.rs` (tests), `stokesforge-app-state/tests/project_objects.rs` | selectable folders (#170, point 4): `un_clic_sur_un_dossier_le_selectionne_et_le_deplie`, `un_clic_sur_un_dossier_ouvert_le_selectionne_sans_le_replier`, `un_double_clic_replie_le_dossier` (closed or open at the start), `le_triangle_replie_un_dossier_ouvert`, `un_dossier_selectionne_porte_fond_filet_et_survol_comme_une_fenetre`, `suppr_et_f2_agissent_sur_un_dossier_selectionne` (`DeleteObjects([dossier])`, in-place rename), `ctrl_v_avec_un_dossier_selectionne_colle_dans_le_dossier` and `le_parent_du_collage_est_le_dossier_seul_ou_la_racine`, `ctrl_c_sur_un_dossier_passe_par_l_etat_qui_le_refuse`, the right-click of a folder (Copy and Duplicate disabled, no action); state side `copier_ou_dupliquer_un_dossier_est_refuse_avec_sa_raison` (nothing on the clipboard, tree and history intact, warning `disabled-copy-folder`, witness: a note can be copied); proven mutants: folder insertion removed from the selection, click that toggles instead of only expanding, double-click without collapse, click without expand, `paste_parent` always at the root, Copy not disabled, `selected` of the folder row forced to false, guard `refuse_folder_copy` removed (copy, duplication) |
| `stokesforge-app/src/app/{project_manager,object_manager_ui,list_row}.rs` (tests) | keyboard navigation of the managers (#170, point 6, S-K9): `deux_fois_bas_atteignent_la_troisieme_ligne_visible` (collapsed folder skipped, bounds), `sans_focus_les_fleches_ne_touchent_pas_au_panneau`, `une_fleche_du_panneau_n_atteint_pas_la_grille` (↓ consumed before the grid), `maj_et_alt_bas_ne_deplacent_pas_la_selection`, `entree_ouvre_la_fenetre_et_bascule_le_dossier`, `la_ligne_du_curseur_clavier_a_le_contour_de_focus`, `la_ligne_voisine_est_bornee_et_sans_curseur_on_part_d_un_bout` (`neighbour`); Object Manager side `fleches_et_entree_parcourent_les_lignes_de_l_object_manager` (neighbor Select, Properties on Enter, Ctrl+↓ = `MoveSeries` without selection) and `la_ligne_selectionnee_a_le_contour_de_focus_quand_le_panneau_est_focalise`; proven mutants: navigation on `order` instead of the visible rows, arrow read without being consumed, Shift or Alt+↓ variant not consumed, Enter removed, focus outline removed (PM and OM), neighbor not bounded, neighbor without a cursor |
| `stokesforge-app/tests/support/sources.rs` | not a test: the **single reader** of all guards that read the code (`libelles.rs`, `gardes.rs`, and through `src/lib.rs` those of `icons` and `theme`). Reads `src/` of **both** crates recursively (`stokesforge-app` and `stokesforge-app-state`, the latter under the prefix `app-state/`: `app-state/state.rs`); production view without comments or `#[cfg(test)]` items (each one, not only the first; a `#[cfg(test)] mod x;` makes its whole file a test file); finds a function by its name, wherever it lives (#116). A new guard that reads the code goes through it, never through `include_str!` or a named file |
| `stokesforge-app/src/field_state.rs` (tests) | field state (#219): pure decision of the cycle (`decide`), red/orange frame and bubble read in the painted shapes, old value given back on exit, orange cleared after 2 s, witness "valid input → no frame"; proven mutants (color, field kept open, deadline, exit without change, bubble) |
| `app/busy.rs`, `app/empty_state.rs`, `stokesforge-app-state/tests/shell.rs` (#232) | indicator absent before 0.5 s then the word, Stop only after 3 s and only if the work can be interrupted; empty-state phrase dimmed and centered; import Stop: nothing created, no history entry, a later import runs to the end |
| `stokesforge-app/tests/gardes.rs`, `app/shortcuts.rs`, `app/keyboard_shortcuts_window.rs`, `project_manager.rs` (tests) | keyboard and mouse (#231): guards `aucun_consume_key_hors_du_registre` and `aucun_curseur_hors_du_module_des_curseurs` (witnesses), every consumed constant appears in `HELP`, shortcuts window, Help menu, double-click in the Project Manager (activates, never closes, two Ctrl-clicks do not open) |
| `stokesforge-script/tests/frozen_api.rs` (#331, feature `python`) | guard of the **frozen V1 API** of `pyst`: **reads** the `pyst-api` blocks of `docs/python-et-script.md` (no copy) and compares them with the built module — names and kind of `pyst` / `pyst.analysis`, `inspect.signature` of each function (clear refusal if one fails, otherwise the test would pass vacuously), `Handle` without a public attribute, docstrings, option keys (`plot`, `set_graph`, `set_column_meta`, read in the unknown-key message), keys of the returned dictionaries (`info`, `series`, `series_groups`, analysis results, `readiness`) and enumerated values. It replaces `la_surface_expose_vingt_huit_appels_et_quatre_erreurs`. Witnesses by mutation: call added, parameter renamed, default changed, dict key added, getter on `Handle`, option key added, line removed from the doc. Limit: an additional value *accepted as input* (not listed) is seen only if it is also returned; the defaults `panel=(0, 0)` and `png_scale=4.0` are matched to the PyO3 `Ellipsis`, so their value is not read back |
| `stokesforge-app/src/number_field.rs` (tests) | "number" building block (#220): reading (refusal, bounds, integers), step and large steps, value list on the keyboard (Alt+↓, Enter, Esc), click = all selected, wheel on hover without a focus that follows the list (`step_preset`: off the list, extremities), field without a list = scrolling window, full arrow in the frame (`list_frame`: distinct theme backgrounds), consistency of the quantity table; in `plot_settings`: input validated on Enter, wheel notches = one history entry, Esc gives back the value then cancels the dialog |
| `stokesforge-app/src/app/dialogs.rs` (tests) | dialog building block (#228): button order read in the shapes (dialog, wizard, read-only), pure decision of the footer (`footer_message`: the error comes before the reason) and its painting (the reason returns when the error disappears, active button witness), initial focus on the first text field and no focus without a field, Esc that leaves a field then cancels, geometry returned on close and taken back on open; proven mutants (initial focus, Esc); state side (`tests/shell.rs`): geometry written to the preferences outside the history, error in the footer of the open dialog only. Confirmations (#229): Enter and Esc = Cancel in `modal_confirm` (the click of the verb confirms), Enter = Save and Esc = Cancel in the trio, Discard Changes on the left, verb red on hover then on keyboard focus (Tab), pointer gone to rest; `modal_alert`: Enter = Close, action by its button; proven mutants (Enter of the confirmation, red on focus) |
| `stokesforge-app`: confirmations on the state and window side (#229) | `stokesforge-app-state/tests/shell.rs`: New Project / Quit / Open… ask only if there is work to lose, Save saves then continues (and a failure or a closed picker abandons the gesture), unreadable project: path and project kept (S-M8), failed save (S-M8), export whose added extension would target an existing file; sheet and objects deleted without confirmation, message that names the graphs (`tests/project_objects.rs`); `app::tests::fermer_la_fenetre_demande_avant_de_perdre_le_travail` (CancelClose, then Close after Discard); `import_wizard::tests::supprimer_un_modele_passe_par_une_confirmation_modale` (Enter cancels without validating the wizard) |
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

`stokesforge-robustesse` contains **no production code**. It depends on the
whole workspace and carries two things:

- **22 batteries** (`tests/b1…b22`) that test the foundation at its limits:
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
  (`b22`, see "Targeted recalculation (#193) — and what to test when relying on it" below);
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
`#[ignore]` remains in `stokesforge-robustesse`. The last one, in `b9`
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

## Targeted recalculation (#193) — and what to test when relying on it

Since #193, the actor rewrites a formula column only if a piece of data of its
calculation has changed (`stokesforge-session/src/recalc.rs`, [`session.md`](session.md)).
This kind of optimization has only one failure mode, and it is
**silent**: one piece of data forgotten in the fingerprint, and the displayed value
stays wrong without any error. The tests below are made for that
case, and everything that reuses the mechanism must rely on them (recomputed
analyses #184, references between sheets #186,
new formula functions).

### What holds it today

| Test | What it holds |
|---|---|
| `stokesforge-robustesse/tests/b22_targeted_recalc.rs` | **the oracle**: two twin sessions, one targeted, the other in full recalculation (`FormulaRecalc::Full`, the code from before #193), receive the same randomly drawn steps; same answers and same trees required after each step. Plus, at each step: the version ↔ content doctrine **in the actor** (recalculation writes, macros, reloads included), and the "targeted" witness — an unrelated command changes the version of no column. A final coverage check requires formulas that are computed, not computed and spared, and **at least twenty passes per kind of step**: a gesture that is always refused would prove nothing. The draw is replayable identically: no target is chosen in the order of `Tree::iter` (hash table with a random seed) |
| `stokesforge-session/tests/recalc_targeted.rs` | one named case per component of the fingerprint (stored formula, sheet membership and version of the sources, length of the result, own version, outcome), the gestures that make them change (move within the sheet or to another, deletion then undo, cycle created elsewhere, deletion of cells in a formula column, aborted macro, duplicated sheet…), stability of a chain and of a cycle, forgetting of the memo by `Load` and `NewProject` on a coincidence of identifiers and versions that is **manufactured and checked**. Each "not rewritten" assertion has its witness in full mode |
| `stokesforge-session/src/lib.rs` (`tests`), `src/recalc.rs` (`tests`) | interruptible recalculation (#239): Stop cancels the command and allows redo (stop set in advance on the next recalculation, hence deterministic), Stop after a command **without effect** undoes nothing (`History::execute_recorded`), the death of the actor (panic simulated by `Request::PanicForTest`, or in the middle of a recalculation) frees the gauge and makes `actor_alive` false (#310), late Stop without effect on the next one, stop between two columns then next pass that finishes without rewriting what is already written. `stokesforge-app-state/tests/shell.rs` (`heavy`): the same gesture seen from the interface on a heavy project, with its witness without Stop, and the question "save?" asked at close during the recalculation |
| Axis direction and axis settings (#344) | `AxisSpec::orient`: table (auto or explicit × inverted × order of the bounds, bounds missing or not finite, idempotence) in `stokesforge-plot/src/axis.rs`; identical rendering before and after `orient` (`an_oriented_axis_is_drawn_like_the_original_axis`, `stokesforge-render/tests/geometry.rs`); opening a file with `inverted` and bounds (-1, 10): bounds (10, -1), `inverted` true, empty history, file untouched (`load_swaps_the_bounds_of_an_inverted_axis_without_history`, `stokesforge-session/tests/session.rs`); `pyst.set_graph` (`set_graph_orients_the_axes_from_the_order_of_the_bounds`, `stokesforge-script/tests/set_graph.rs`); axes page (`stokesforge-app/src/app/plot_settings.rs`, module `tests`): no more "Inverted axis" / "Auto range" / "Closed frame" boxes, "Advanced" block closed then opened on click (title or arrow), input of a bound under Auto = two bounds fixed in **one** history entry, From > To inverts the axis, Auto button = data while keeping the direction, From shows the high bound of an inverted axis; proven mutants (`orient` without swapping the bounds, `load_project` without `orient_graphs`, input that does not fix, Auto button that loses the direction, click on the arrow of "Advanced" counted twice) |
| `stokesforge-session/tests/analysis.rs` (`on_load_no_calculation_is_rerun_and_the_fingerprint_says_whether_it_is_stale`, `trust_on_load_applies_only_to_the_load`) | A1 (#316): on opening no calculation (automatic or manual) is rerun, in both modes (`Full` as well as `Targeted`, otherwise the oracle diverges); input modified outside the application (file bytes) → `Manual` and file values kept, then `recalculate` makes them right; intact file → up to date; the `trust_loaded` flag is single-use (a calculation without a memo after `AddCalculation` is still recalculated). Mutants: flag removed, fingerprint comparison always true, always false |
| `stokesforge-session/tests/formula.rs` | the behavior of formulas itself (before #193, unchanged) |
| `stokesforge-robustesse/tests/b14_sweep.rs` | the version ↔ content doctrine on `History` alone, on which the whole mechanism rests — including for a column added with a number already used |
| `stokesforge-core` (`project.rs`, `an_inserted_column_moves_the_counter_past_its_version`) | inserting a column moves the counter beyond its version |

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
| recalculate everything (the "targeted" is no longer targeted) | the "targeted" cases and their witnesses, `b22` (property 3) |
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
`BadExpression`, `NoParameter`, `Invalid`); `forget_positions` erases
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
| `forget_positions` keeps `shared` | `forget_positions_clears_the_ranges_and_the_per_parameter_settings`, `rouvrir_une_entree_oublie_aussi_les_reglages_fixes_et_partages` |
| `MAX_PEAKS` at 50 | `the_caps_are_those_of_the_quantities_table`, `les_champs_d_analyse_suivent_les_plafonds_du_crate_d_analyse` |
| `auto_initial` without range filter | `auto_initial_follows_the_model_on_the_points_of_the_range`, `auto_reads_only_the_fitted_range` |
| `validate` without "zero free parameters" | `validate_refuses_a_fit_without_a_free_parameter` |
| `validate` lets a non-finite range, seed, prominence, shift or window through (#278) | `validate_refuses_a_non_finite_range`, `validate_refuses_a_non_finite_seed_or_prominence`, `validate_refuses_a_non_finite_shift_or_window`, `validate_refuses_each_non_finite_field_and_accepts_the_finite_one_that_round_trips` (field-by-field table, NaN/±inf, round trip of the finite value) |
| `add_/update_calculation` without `Operation::validate` (#278) | `a_non_finite_operation_is_refused_on_add_and_on_update` (core), `every_non_finite_operation_is_refused_without_touching_the_tree` (twelve families, add/update/replace, tree and identifiers intact), `the_session_refuses_a_non_finite_operation_without_changing_anything` (session) |
| unreadable calculation discarded without a report (#278) | `an_unreadable_calculation_is_named_by_the_report_and_the_rest_loads` (persistence), `loading_a_file_with_an_unreadable_calculation_warns` (app-state), `several_discarded_calculations_are_all_listed_in_order_and_the_sound_ones_load`, `a_file_without_a_discarded_calculation_gives_an_empty_report`, `an_identifier_out_of_bounds_discards_the_calculation` (bound, all id fields), `loading_relays_the_calculations_dropped_by_persistence` (session), `the_discarded_calculations_warning_counts_and_abbreviates` (singular/plural, 5 ids then "…", fr/en), `une_entree_a_nombre_non_fini_ne_se_relit_jamais_non_finie` (preferences); #309: `an_invalid_shape_discards_and_names_the_calculation`, `a_discarded_trace_is_counted`, `an_unreadable_correction_trace_is_counted_by_the_report` (persistence), `loading_relays_the_dropped_correction_traces` (session), `mark_loaded_avertit_des_calculs_ecartes_du_rapport` (app-state), `a_duplicate_identifier_keeps_the_first_and_counts_the_second` (persistence, F4), `le_rapport_compte_les_traces_et_les_calculs_dans_un_seul_message` (F1), `run_project_warns_about_discards_then_runs_the_script` (CLI, feature python, F2), confirmation on save (F3, app-state `shell.rs`): `enregistrer_sur_la_source_avec_ecarts_demande_puis_ecrit`, `refuser_l_ecrasement_n_ecrit_rien_et_oublie_le_geste`, `quitter_enregistrer_accepter_enregistre_puis_quitte`, `enregistrer_sous_ailleurs_ne_demande_rien_mais_revenir_a_la_source_oui`, `un_fichier_propre_ou_un_nouveau_projet_n_ouvre_aucune_question`, `le_chargement_du_demarrage_pose_le_repere_de_la_source`, `le_demarrage_rend_le_rapport_des_calculs_ecartes` (main); `a_nan_edited_by_hand_in_the_file_discards_and_reports_the_calculation` (persistence, #291: `validate` called on load) |

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
| manual: comparison by content (`seen`) removed | `b22` (full recalculation rewrites the formulas, hence their versions) |
| guard of the calculation → formula → calculation loops removed | `b22` (property 3: an unrelated command rewrites columns) |
| manual: definition change not "trusted" (`Trust`) | `editing_a_stale_manual_calculation_updates_it_without_making_it_stale_again` alone |

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

## Running

```sh
cargo test --workspace                        # tout, sauf #[ignore]
cargo test -p <crate> --test <fichier>        # un fichier de tests
cargo test -p stokesforge-robustesse -- --ignored  # reproductions et bancs coûteux
# (Typst est compilé et testé par `cargo test --workspace` ; `--all-features` n'active plus que Python : voir .codex/README.md)
scripts/python-test.sh                        # pont Python, scripts, robustesse, application et CLI
scripts/python-test.sh -- --ignored           # + les reproductions de la frontière Python
```

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
| `fmt` | yes | `cargo fmt --all -- --check`, plus `scripts/release-notes-test.sh`, `scripts/release-tag-test.sh` and `scripts/versions-alignees.sh` (all crates in the same version `X.Y.Z`, #349/#350; only the `[package]` section is read, internal dependencies `stokesforge-x = { version = … }` equal to this version, `Cargo.lock` up to date, `FORMAT_VERSION` = N requires `tests/witnesses/vN/project.stf`, #351/#352/#353) |
| `licenses` | yes | policy and generation of third-party notices |
| `clippy` | yes | `cargo clippy --workspace --all-targets -- -D warnings` |
| `test` | yes | `cargo test --workspace` |
| `robustesse` | no | `cargo test -p stokesforge-robustesse -- --ignored` |
| `test-python` | yes, automatic | vendors CPython + numpy, then `scripts/python-test.sh` |
| `release-linux` (GitHub) | on a tag `vX.Y.Z` or `vX.Y.Z-rcN` (on an `-rcN`, `publier` creates the release then deletes it; no release remains) | checks the tag (`scripts/release-tag-sur-main.sh`, `scripts/release-tag-croissant.sh`, `scripts/release-notes.sh`), assembles and tests `scripts/app-dist.sh`, uploads the archive to the GitLab package registry, without a GitHub release (#333) |
| `test-python-windows`, `dist-windows` (GitHub, `windows-latest`) | automatic on `main`, `vN`, `release/**` and on a tag (not on `feature/**`), blocking (#333) | same checks on a clean machine; on a tag, `dist-windows` uploads the installer to the GitLab registry (`scripts/release-televerser.sh windows`) |
| `publier` (GitHub) | on a tag `vX.Y.Z` or `vX.Y.Z-rcN`, after all blocking checks and the two package uploads | GitLab release with the notes of the CHANGELOG (#333); an `-rcN` creates then deletes the release (trial); procedure and criteria: [`criteres-v1.md`](criteres-v1.md) |

The Linux jobs run on the pushed branches `main`, `feature/**`,
`v[0-9]*`, `release/**` and on the tags `v*`. `release-linux` and `publier`
refuse a tag whose commit is an ancestor neither of `origin/main` nor of a
`release/*` branch (`scripts/release-tag-sur-main.sh`, #354), or that does not
exceed the existing tags (`scripts/release-tag-croissant.sh`, #355):
a final tag must exceed every existing final; an `-rcN` must exceed the
`-rc` tags of its version and not target a version that is already final; a commit that
is only on `release/N.x` is compared only with the tags of major N. The distribution
scripts build with `--locked`.
Consequence: rerunning the pipeline of an old tag fails as soon as a more recent
tag exists. An existing tag set again on another target cannot be detected
in CI; only the protection of the `v*` tags of GitLab holds it.

**Settings outside the repository** (to be done by hand, nothing in the code sets them):
- `main` protected, no force-push, on GitLab and on GitHub;
- `v*` tags protected (creation reserved to the Maintainer role) on GitLab;
- `main` default branch on GitLab: this is what makes "Closes #N"
  work.

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
(`scripts/versions-alignees.sh`, at every push, not only at the tag), including internal dependencies, `Cargo.lock` and the `.stf` format witness, without cargo.

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

**Reading the result.**

```sh
scripts/ci-statuts.sh [commit]      # HEAD par défaut ; exige glab connecté
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
These jobs, like `release-linux` and `publier`, not being in the list of
`gitlab-status.py`, their result is read on GitHub, not in `ci-statuts.sh`.
The GitHub API **without login** is limited to 60 requests per hour and per
address: do not poll it in a loop (a CI wait is done on GitLab,
`glab`, or at a long interval); beyond that, it answers 403 until the next hour.

**Tag and mirror.** A tag pushed to GitLab sometimes arrives late on GitHub
(CI starts there only on its arrival). Force the push: mirror synchronization
button, or `glab api -X POST projects/86104040/remote_mirrors/<id>/sync`.

**Package upload.** `scripts/release-televerser.sh` passes the token to
`curl` through `-K -` (standard input), not through process substitution: under
Git Bash (Windows) the latter fails (curl code 26). The token never appears
in the arguments of the command.

**Reproducing a Linux failure at home.** The runner (`ubuntu-latest`) differs from a
development machine in three ways that have already caused failures:

- **A newer stable Rust** than the machine's (`rust-toolchain.toml` asks for
  `stable`): a new Clippy lint can make `clippy` fail without anything
  having changed. Fix the code, do not pin the version.
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
`autosave::pendant_un_recalcul_l_echeance_saute`. About 1 GB must be free
in `TMPDIR`; on a full `/tmp` (`Disk quota exceeded`), set `TMPDIR` on
disk. The application reports the error instead of losing data: this is
not a bug. The assertions of these tests join the messages of the status
bar (`status_messages`), where the write error is read. The size of the project
is not reduced: it sets the duration of the recalculation that the tests observe
(wait of 200 ms exceeded).

**Keeping the two CIs in agreement.** Every command changed in `.gitlab-ci.yml` or
`ci.yml` is carried over to the other (see the table above). A new Linux
job is also added to `JOBS` in `scripts/gitlab-status.py`, otherwise its
result is not published. Known exception: `release-linux`, `publier` and the
Windows jobs of GitHub do not appear there; their failure is read on GitHub
(annotations readable by the API without login).

### GitHub results visible in GitLab

`.github/workflows/gitlab-status.yml` triggers when `CI` is finished,
even on failure or after cancelation. `scripts/gitlab-status.py` publishes the six
statuses `github-actions/<job>` on the tested SHA, in the same GitLab
pipeline, with a link to the GitHub logs of each job. The results
arrive after the whole CI has ended; the logs and artifacts
stay on GitHub. Old pipelines are not corrected retroactively.

A failed `robustesse` reproduction is marked `skipped`, with the description
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
`scripts/ci-statuts.sh`. Renaming either repository requires changing them.

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
`python3 scripts/gitlab-status-test.py` (also run by the `fmt` job, with `scripts/release-notes-test.sh`, which tests `release-notes.sh` and `versions-alignees.sh` on a fake repository). They cover the results, the
tolerated reproduction (including a failed step under a tolerated conclusion),
cancelations, absent jobs, excluded PRs/forks, old
attempts, pagination, pipeline selection and HTTP retries.

## What to know before writing a test

- A test that compares two paths needs a **witness** that must differ.
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
  `verrou_captures()` (`app/preferences_proto.rs`); a new capture file
  must share this lock (move it to a common module).
- The interface language is a **global** state of the process
  (`i18n::set_language`): a unit test that must paint in a
  given language (`en`, `en-XA`) reruns in a subprocess with `STOKESFORGE_LANG`
  set, and checks that the child did paint in this language (model:
  `chaque_onglet_se_peint_en_anglais_et_en_pseudo_langue`,
  `src/app/plot_settings.rs`, #119).
- An egui menu is tested really opened (click on its title), and is read
  in the accessibility tree that egui publishes (`ctx.enable_accesskit()`,
  `platform_output.accesskit_update`): label, disabled state, checked box
  and rectangle of each entry, without assuming a position or comparing a
  color (model: `entrees`, `src/app/menus.rs`, #122).
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
  egui image, key, three-frame click, painted text, `colonnes_xy`) lives
  in `src/app/test_support.rs` (#128), imported by name; a helper specific to
  one surface stays `pub(in crate::app)` in the test module that carries it
  (`graph_window::tests::graphe`, `project_manager::tests::click_at`). An
  assertion of emptiness ("the table is emptied") needs the witness that shows that
  the table was full before (model:
  `ctrl_n_confirme_et_oublie_les_vues_du_projet_precedent`, #123).
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
  `stokesforge-app/tests/isolation_preferences.rs`); the integration tests
  (`tests/`) build their
  state with `AppState::new_with_preferences(…, Preferences::load_from(None))`,
  never `AppState::new` (guard in `stokesforge-app/tests/gardes.rs`, which reads
  the two `tests/` folders). The write of the file
  is atomic (temporary + rename).
- A test does not read the machine's import templates or recent files either
  (`import_recipes.json`, `import_recent.json`): a saved template "Jasco"
  came before the built-in template and made the automatic choice fail
  (#195). It redirects the configuration folder to a temporary
  folder by setting `STOKESFORGE_CONFIG_DIR` **and**
  `XDG_CONFIG_HOME` (#332: `directories` honors only the latter,
  and only under Linux; under Windows the tests wrote to the real
  `%APPDATA%\StokesForge\config\` and fought over the file, "Access is
  denied") — under a lock if the test restores the values
  (`stokesforge-import/src/store.rs`, `tests/builtin.rs`,
  `stokesforge-app-state/tests/shell.rs`), or once per process
  before any bench (`hermetic_configuration`,
  `stokesforge-script/tests/script.rs`; `isolated`,
  `stokesforge-cli/tests/import.rs`). Single rule `config_dir`; pure forms
  `config_dir_from`/`recovery_dir_from` tested without `set_var`
  (`stokesforge-import/src/config_dir.rs`, proven mutants: empty filter,
  `recovery` suffix, variable ignored, `data_dir` replaced by `config_dir`);
  end to end, `l_isolation_redirige_le_dossier_de_config_et_l_enregistrement_y_ecrit`
  (`shell.rs`, proven mutant: `ProjectDirs` put back in `store.rs`).
  Known limit: `UserColors::load()` (`state.rs`) has no `cfg(test)`
  isolation, a test that saves a user color would write to
  the real `couleurs.json`.
- `stokesforge-clipboard` (#149): the decision of X11 answers (`x11::answer`) and the Windows/macOS format names are tested without a server; the X11 offer is tested against a **real server** (`tests/x11.rs`, `#[ignore]`, `DISPLAY` required, XWayland is enough: `cargo test -p stokesforge-clipboard --test x11 -- --ignored`). Windows is tested likewise against the **real clipboard** (`tests/windows.rs`, `#[ignore]`, interactive desktop required: `cargo test -p stokesforge-clipboard --test windows -- --ignored`; witness: a `windows_format_name` that returns the MIME for PNG fails the read-back of the "PNG" format). Wayland and macOS have no automatic test: Wayland requires a compositor and a real input (serial), macOS is only compiled (`cargo check --target …`). An X11 test of a recipient does not test `response_type` of a `SelectionNotify`: an event coming from `SendEvent` carries the bit 0x80.
- Tests and their comments are in French, like the rest.

**To read**: `stokesforge-robustesse/src/lib.rs` (the index of the batteries), then the
head of the targeted `b*` file.

## Re-split of interface functions (#124)

The surface benches keep the same events and rectangles: menus,
grid, properties pages, import wizard, Object Manager and tab
strip. The added cases check in particular the range targeted by the type
menu, the keeping of the format parameters, the layer and series actually
edited, the independent collapse of the tree groups and the order of the callbacks.
The detached callback of the note collects its edit and its close before their
application by the root.

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
| `membership_editor`: the selection no longer follows the moved series (`*index = moved` removed) | `la_liste_deroulante_fait_rejoindre_un_groupe_et_l_editeur_suit_la_serie` (`app/curve_settings.rs`) |
| graph: the "group selected" branch skipped, Del targets the first series | `les_raccourcis_d_un_groupe_selectionne_visent_le_groupe` (`app/graph_window.rs`) |
| Ctrl+↑/↓ without the `can_move_group` bound | same |
| F2 no longer opens the group rename | same |
| `.stf`: empty group names kept on load | `an_empty_group_name_is_dropped_at_load` (`stokesforge-persistence/src/convert.rs`) |
| `.stf`: `#[serde(default)]` removed from `group_names` (old file unreadable) | `a_layer_without_group_names_reads_back_with_its_groups` |

## Copy/paste of cells (#259)

| Mutant (what is broken) | Seen by |
|---|---|
| `tsv::needs_quotes` always false (plain `join("\t")`) | `stokesforge-core/src/tsv.rs` (round trip tab, line break, quote); `a_multiline_text_cell_pastes_into_another_session_without_shifting_anything` (`stokesforge-app-state/tests/shell.rs`) |
| `decode` without quote handling | same + `a_single_multiline_text_cell_pastes_as_one_cell` |
| `decode` without `\r\n` | `a_windows_spreadsheet_ends_its_rows_with_crlf`, `external_clipboard_from_text_lit_le_format_des_tableurs` |
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
| band offset ignored (axis of layer 3 on that of layer 2) | `the_axes_of_several_overlaid_layers_stack_outward` (`geometry.rs`), `an_offset_band_picks_only_its_layer` (`hit.rs`) |
| constructor `OverlaySpec::new` inverted, `TOP_X` that also shows Y | `named_layers_say_which_axes_they_display` (`stokesforge-plot/src/panel.rs`) |
| `PanelSpec::remove_series` that removes nothing or accepts an out-of-bounds rank | `remove_series_removes_the_designated_series_and_no_other`; `object_manager_supprime_une_courbe_sans_toucher_aux_donnees` (`stokesforge-app-state/tests/shell.rs`) |
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
| a formula that reads a deleted column is listed; one in which only a letter slides is not (#321: reading by identifier) | `supprimer_une_colonne_lue_par_une_formule_ouvre_la_liste_puis_close_ne_fait_rien`, `les_lettres_qui_glissent_ne_comptent_pas_seule_la_colonne_lue_compte` (the list of any formula as soon as a column leaves: red) |
| a column without a reader opens nothing | `une_colonne_sans_lecteur_n_ouvre_rien` |
| an analysis calculation is named by its family and its first output | `un_calcul_d_analyse_est_liste_par_sa_famille_et_sa_premiere_sortie` |
| a calculation whose output disappears with the gesture is not listed | `supprimer_un_classeur_emporte_ses_calculs_sans_rien_signaler` (without the exclusion: red) |
| several columns, one list, a single Undo | `undo_rend_la_colonne_et_la_formule_en_un_pas_et_plusieurs_colonnes_font_une_liste` |
| Close gives nothing back | `supprimer_une_colonne_lue_par_une_formule_ouvre_la_liste_puis_close_ne_fait_rien` |
| Ctrl+Shift+C asks the question only if a column has a formula; Values empties the Formula row | `ctrl_maj_c_sur_une_colonne_formule_pose_la_question`, `ctrl_maj_c_sans_formule_copie_directement` |

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
| input: unknown letter, self-reading, modified `#REF` refused in `app-state` (`header_edit_refusal`), never a syntax error; unmodified `#REF` writes nothing; `formula_issue` for the box | `shell.rs` (`a_formula_reading_its_own_column_is_refused_while_typing`, `a_ref_formula_is_refused_only_when_its_text_is_changed`, `a_formula_naming_a_missing_column_…`) |
| paste: the refused formula alone is not pasted, the status bar says the refusal after the writes; Ctrl+Shift+C copies letters re-read in the destination sheet | `a_refused_formula_in_a_paste_…`, `copied_formulas_travel_as_letters_…` |
| `pyst`: letters on read and write, refusal of an unknown letter and of `#REF`, another field does not touch the formula | `script/tests/navigation.rs` (`les_formules_se_lisent_…`), `mutations.rs` |

Limit: the red frame, the bubble and the field that stays open (grid) cannot be tested without a window: the decision (`header_edit_refusal`, `inplace_edit::resolve`) can, the painting is re-read on screen.

## Column width (#325)

`NodeId -> f32` table of the actor, outside the model and outside the history, saved in the `.stf`. Each rule has its witness, checked by mutation.

| Rule | Witness |
|---|---|
| setting a width advances only `geometry_revision`; the same width, NaN, ≤ 0, ∞: ignored | `session/tests/column_widths.rs` `setting_a_width_only_advances_geometry_revision` (going through `revision`: red) |
| the width of a deleted column comes back with Ctrl+Z | `the_width_comes_back_with_the_column_after_undo` (purging widths at the mutation: red) |
| `.stf` round trip; `NewProject` empties | `the_widths_make_the_file_round_trip` (not writing the field: red) |
| writing does not emit the key of a vanished column | `saving_does_not_write_the_width_of_a_deleted_column`, `persistence` `save_drops_dead_keys_and_invalid_widths` (removing the filter: red) |
| file without the field → empty table; orphan key, NaN, ∞, 0, negative discarded one by one; identifier > `MAX_NODE_ID` refused | `persistence` `a_file_without_the_widths_block_*`, `hostile_column_widths_*`, `a_column_width_key_above_the_id_ceiling_*`, `robustesse/b3` `b3_hostile_column_widths` (reading without a filter, without `check_node_id`: red) |
| the state sets the width through the actor without history, including a detached view | `app-state/tests/column_width.rs` |
| the `.stf` gives its width to the startup snapshot | `main.rs` `le_demarrage_restaure_les_largeurs_de_colonne_du_fichier` |
| the dragged handle pushes the width only on release | `grid::tests::un_redimensionnement_a_la_main_envoie_la_largeur_a_la_fin_seulement` |
| opening and painting sends nothing, does not mark the project modified | `ouvrir_la_feuille_n_envoie_aucune_largeur` (pushing the rendered width at every frame: red) |
| Add Column: widths kept, the new one at 54; Delete then Ctrl+Z | `la_largeur_de_chaque_colonne_survit_a_l_ajout_d_une_colonne`, `supprimer_puis_annuler_rend_la_colonne_et_ses_voisines_a_leur_largeur` (width by position: red; without `reset()` when the list changes with equal count: red) |

| the default width is the same floating, maximized, then restored (#337) | `window_stack::tests::la_largeur_par_defaut_d_une_colonne_est_la_meme_flottante_et_maximisee` (not forgetting the table state after the measurement pass of the `Window`: red, 58 instead of 62) |

The width is read in the bench on the flat fill of the column letter (`largeur_peinte`). The grid bench paints `book_contents` outside any `Window`: it does not see the measurement pass, hence the test at the level of `show_workspace`.

### Reset to default (#330)

| Guard | Test (witness) |
|---|---|
| reset: key removed, `geometry_revision` +1 only if it existed, neither `revision` nor history | `session/tests/column_widths.rs` `resetting_to_default_removes_the_key_and_advances_only_if_it_existed` (unconditional `+= 1`: red) |
| reset then save: no more key in the file | `a_column_reset_to_default_is_no_longer_in_the_file` (setting 54 instead of removing: red) |
| the action removes the width from the local copy before refresh | `app-state/tests/column_width.rs` `l_action_de_remise_retire_la_largeur_sans_historique` (key kept in the local copy: red) |
| B reset to 54, A and C keep theirs | `grid::tests::remettre_une_colonne_au_defaut_ne_touche_pas_les_autres` (widths outside the layout key: red) |
| the gutter keeps its width after a drag and a column reset, returns to 90 for the sheet | `un_glisser_de_colonne_garde_la_largeur_de_la_gouttiere` (gutter not remembered: red) |
| entry after Role ▸, disabled without a width, whole selection | `l_entree_de_remise_suit_role_et_se_grise_sans_largeur`, `la_remise_traite_toute_la_selection` (`add_enabled(true)`: red; targeting only the clicked column: red) |
| sheet entry: tab, Workbook ▸ Sheet, Workbook ▸ Columns | `le_clic_droit_de_l_onglet_remet_toute_la_feuille`, `menus::tests::la_remise_des_largeurs_est_dans_workbook_sheet_et_columns` (entry always active, or a single column targeted for the sheet or the tab: red) |

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
| the selection covers the same columns after the gesture | `la_selection_reste_sur_les_colonnes_deplacees` (emptying the selection: red) |
| `Move ▸` between the clipboard and `Type ▸`, four entries; all-or-nothing disabling (A alone, C alone, A and C, whole rows); the active entry pushes `MoveColumns` | `grid::tests::move_est_place_*`, `les_entrees_de_move_sont_grisees_*`, `une_entree_de_move_pousse_*` (disabling removed, entry removed: red) |
| same submenu and same disabling in Workbook › Columns | `menus::tests::le_menu_colonnes_porte_move_*` (removing `Move ▸` from the shared painting: red) |
| the width follows the moved column (#325) | `grid::tests::la_largeur_suit_la_colonne_deplacee` (storage by position: red) |

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

`crates/stokesforge-app/src/demo/tests.rs` (module of the **binary**: `cargo test -p stokesforge-app --bin stokesforge-app`). Each test has its witness, checked by mutation (seed changed, title given to a graph, column deleted under a graph, amplitude of the Gaussian set changed, `recalculate` removed from `session_for_project`): unique window names across the whole project, no graph title (#103), each graph resolved without a broken reference and drawn, logarithmic axes on strictly positive data (invariant 10), each analysis set finds its known parameters through `core::compute` (`Attendu`, tolerance), the eight families of `OperationKind::ALL` covered (including smoothing, `Smooth` set), noise with a fixed seed pinned, formula columns computed from startup, `.stf` round trip through the session (clean report, same specs), all `PlotType` (without `Unknown`).

## Analyzing rows (#253)

One layer, one file, one witness tested by mutation:

- `stokesforge-core/tests/transpose.rs`: creation (name, place, columns, metadata),
  reuse of a row, short column, refusal naming the letter without creating anything,
  text, name collision, derived states (stale: value, x', column added or
  deleted; modified: only a value edit), update (new version,
  deleted column recreated), life of the link (rename, deletion, copy). Mutants:
  `stale` forced to false, fingerprint of the rows ignored, `prune_transposes` removed.
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
