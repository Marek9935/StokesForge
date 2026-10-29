//! File › Preferences (#148): the application preferences window.
//!
//! Layout (decisions of 2026-10-29): the **rubrics are vertical tabs on the
//! left** ([`TabStripSide::Left`]) — General, Display, Export — and a rubric
//! with many settings has **horizontal sub-tabs** at the top (Display:
//! Interface · Object Manager · New). Pages are short, without scrolling where
//! possible. **General** holds the language, the file caps, autosave and the
//! analysis defaults; **Display** the formula help icon, the Object Manager
//! lists and the settings of *new* workbooks and graphs; **Export** the text of
//! SVG exports, the PNG resolution and the Copy Graph formats (the order is
//! changed by dragging, `reorder_list.rs`, each format is ticked). The menus keep
//! their own entries, which set the same preferences.
//!
//! The settings apply **right away** (like the Language menu): the
//! window has nothing to validate or cancel and goes through `modal_live_settings`
//! (Close only, S-F2; interactive, so focus on the first field and size
//! per class, S-F9, S-F11). It touches neither the project nor
//! the history; the open/closed state is an interface decision of
//! `AppState`, applying the language stays with `StokesForgeApp`.

use eframe::egui;

use super::dialogs::{modal_live_settings, DialogOutcome, InteractiveSize, SizeClass};
use super::form::{choice, form, ChoiceOption};
use super::object_manager_ui::object_manager_group_label;
use super::reorder_list::{move_before, reorder_move, reorder_row};
use super::tab_strip::{tab_strip, TabStripSide};
use crate::number_field::number_field;
use crate::theme;
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::preferences::{CopyFormat, CopyFormatEntry, ObjectManagerGroup};
use stokesforge_app_state::quantities;
use stokesforge_app_state::state::{Action, AppState, PNG_EXPORT_SCALES};
use stokesforge_app_state::tr;
use stokesforge_render_svg::SvgTextMode;

/// The rubrics, in the order of #148, already delivered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum Page {
    #[default]
    General,
    Display,
    Export,
}

/// The sub-tabs of the Display rubric.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum DisplayTab {
    #[default]
    Interface,
    ObjectManager,
    New,
}

/// Paints the window while it is open. Returns the chosen language, if
/// any; the caller applies it and saves it.
pub(super) fn preferences_window(
    ctx: &egui::Context,
    state: &AppState,
    actions: &mut Vec<Action>,
) -> Option<Language> {
    if !state.preferences_open() {
        return None;
    }
    let mut chosen = None;
    // The open rubric and sub-tab are only a convenience of the window: neither
    // `AppState` nor the preferences file.
    let page_id = egui::Id::new("stokesforge_preferences_page");
    let display_id = egui::Id::new("stokesforge_preferences_display_tab");
    let mut page: Page = ctx.data(|data| data.get_temp(page_id)).unwrap_or_default();
    let mut display_tab: DisplayTab = ctx
        .data(|data| data.get_temp(display_id))
        .unwrap_or_default();
    let outcome = modal_live_settings(
        ctx,
        // A new identifier since the rubrics moved to the left: the size
        // remembered (S-F8) for the old, narrow window must not pin the layout.
        egui::Id::new("stokesforge_preferences_rubrics"),
        &tr!("preferences-title"),
        InteractiveSize::Class(SizeClass::Large),
        |ui| {
            let tabs = [
                (Page::General, tr!("preferences-general")),
                (Page::Display, tr!("preferences-display")),
                (Page::Export, tr!("preferences-export")),
            ];
            ui.horizontal_top(|ui| {
                tab_strip(
                    ui,
                    TabStripSide::Left,
                    &tabs,
                    page,
                    |picked| page = picked,
                    None,
                    || {},
                );
                ui.vertical(|ui| match page {
                    Page::General => chosen = general_page(ui, state, actions),
                    Page::Display => display_page(ui, state, actions, &mut display_tab),
                    Page::Export => export_page(ui, state, actions),
                });
            });
        },
    );
    ctx.data_mut(|data| {
        data.insert_temp(page_id, page);
        data.insert_temp(display_id, display_tab);
    });
    if outcome != DialogOutcome::Open {
        actions.push(Action::ClosePreferences);
    }
    chosen
}

/// The abbreviation of a format, through the catalog like any interface text.
fn format_name(format: CopyFormat) -> String {
    match format {
        CopyFormat::Svg => tr!("preferences-copy-svg"),
        CopyFormat::Png => tr!("preferences-copy-png"),
        CopyFormat::Pdf => tr!("preferences-copy-pdf"),
    }
}

fn general_page(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
) -> Option<Language> {
    let mut chosen = None;
    let mut keep_results = state.analysis_keep_results_default();
    let keep_before = keep_results;
    let mut low_r2 = state.analysis_low_r2_dialog();
    let before = low_r2;
    let mut max_file_mb = state.max_file_mb();
    let max_file_before = max_file_mb;
    let mut max_cells_m = state.max_cells_m();
    let max_cells_before = max_cells_m;
    let mut autosave = state.autosave_enabled();
    let autosave_before = autosave;
    let mut autosave_minutes = state.autosave_minutes();
    let autosave_minutes_before = autosave_minutes;
    form(ui, "preferences", |rows| {
        // No section title: the rubric already says "General", and a second label
        // with the same text would make the rubric ambiguous (kittest, screen readers).
        rows.row(tr!("menu-language"), |ui| {
            let mut language = i18n::language();
            let options = [Language::English, Language::French]
                .map(|language| ChoiceOption::new(language, language.native_name()));
            if choice(ui, "preferences_language", &mut language, &options) {
                chosen = Some(language);
            }
        });
        // Cap on the size of a file at opening (A2, #316).
        rows.section(tr!("preferences-files"));
        rows.row(tr!("preferences-max-file-size"), |ui| {
            number_field(
                ui,
                "preferences_max_file_mb",
                &mut max_file_mb,
                &quantities::MAX_FILE_MB,
            );
        });
        rows.note(tr!("preferences-max-file-size-note"));
        rows.row(tr!("preferences-max-cells"), |ui| {
            number_field(
                ui,
                "preferences_max_cells_m",
                &mut max_cells_m,
                &quantities::MAX_CELLS_M,
            );
        });
        rows.note(tr!("preferences-max-cells-note"));
        // Autosave (#317): the period is greyed out if it is switched off.
        rows.checkbox(
            &mut autosave,
            tr!("preferences-autosave"),
            Some(tr!("preferences-autosave", "hint")),
        );
        rows.row(tr!("preferences-autosave-minutes"), |ui| {
            ui.add_enabled_ui(autosave, |ui| {
                number_field(
                    ui,
                    "preferences_autosave_minutes",
                    &mut autosave_minutes,
                    &quantities::AUTOSAVE_MINUTES,
                );
            })
            .response
            .on_disabled_hover_text(tr!("disabled-autosave-off"));
        });
        rows.note(tr!("preferences-autosave-note"));
        // Application preferences (#288, M5); each computation has its own.
        rows.section(tr!("preferences-analysis"));
        rows.row(tr!("preferences-analysis-values"), |ui| {
            choice(
                ui,
                "preferences_analysis_keep",
                &mut keep_results,
                &[
                    ChoiceOption::new(false, tr!("analysis-output-show"))
                        .hint(tr!("analysis-output-show", "hint")),
                    ChoiceOption::new(true, tr!("analysis-output-keep"))
                        .hint(tr!("analysis-output-keep", "hint")),
                ],
            );
        });
        rows.note(tr!("preferences-analysis-values-note"));
        rows.checkbox(
            &mut low_r2,
            tr!("preferences-low-r2-dialog"),
            Some(tr!("preferences-low-r2-dialog", "hint")),
        );
    });
    if keep_results != keep_before {
        actions.push(Action::SetAnalysisKeepResults(keep_results));
    }
    if low_r2 != before {
        actions.push(Action::SetAnalysisLowR2Dialog(low_r2));
    }
    if max_file_mb != max_file_before {
        actions.push(Action::SetMaxFileMb(max_file_mb));
    }
    if max_cells_m != max_cells_before {
        actions.push(Action::SetMaxCellsM(max_cells_m));
    }
    if autosave != autosave_before {
        actions.push(Action::SetAutosave(autosave));
    }
    if autosave_minutes != autosave_minutes_before {
        actions.push(Action::SetAutosaveMinutes(autosave_minutes));
    }
    chosen
}

/// Display: Interface · Object Manager · New, as horizontal sub-tabs.
fn display_page(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    tab: &mut DisplayTab,
) {
    let tabs = [
        (DisplayTab::Interface, tr!("preferences-display-interface")),
        (
            DisplayTab::ObjectManager,
            tr!("preferences-display-object-manager"),
        ),
        (DisplayTab::New, tr!("preferences-display-new")),
    ];
    let current = *tab;
    tab_strip(
        ui,
        TabStripSide::Top,
        &tabs,
        current,
        |picked| *tab = picked,
        None,
        || {},
    );
    ui.add_space(theme::SPACE_ITEM);
    match current {
        DisplayTab::Interface => interface_tab(ui, state, actions),
        DisplayTab::ObjectManager => object_manager_tab(ui, state, actions),
        DisplayTab::New => new_tab(ui, state, actions),
    }
}

/// Display › Interface: the formula syntax help icon of the text fields.
fn interface_tab(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<Action>) {
    let mut help = state.show_formula_help();
    let before = help;
    form(ui, "preferences_display_interface", |rows| {
        rows.checkbox(
            &mut help,
            tr!("preferences-show-formula-help"),
            Some(tr!("preferences-show-formula-help", "hint")),
        );
    });
    if help != before {
        actions.push(Action::SetShowFormulaHelp(help));
    }
}

/// Display › Object Manager: what the panel lists and which groups start
/// expanded (#139). The same settings as View › Panels › Object Manager and the
/// right-click on the panel background, so the same actions.
fn object_manager_tab(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<Action>) {
    let settings = state.object_manager_settings();
    form(ui, "preferences_display_object_manager", |rows| {
        rows.section(tr!("preferences-om-listed"));
        let mut show = settings.show_scales;
        if rows.checkbox(
            &mut show,
            tr!("object-manager-show-axes"),
            Some(tr!("object-manager-show-axes", "hint")),
        ) {
            actions.push(Action::SetObjectManagerShowScales(show));
        }
        let mut show = settings.show_objects;
        if rows.checkbox(
            &mut show,
            tr!("object-manager-show-objects"),
            Some(tr!("object-manager-show-objects", "hint")),
        ) {
            actions.push(Action::SetObjectManagerShowObjects(show));
        }
        rows.section(tr!("object-manager-default-expansion"));
        rows.note(tr!("object-manager-default-expansion", "hint"));
        for group in ObjectManagerGroup::ALL {
            let mut open = settings.group_open(group);
            if rows.checkbox(&mut open, object_manager_group_label(group), None) {
                actions.push(Action::SetObjectManagerGroupOpen { group, open });
            }
        }
        rows.field(|ui| {
            if ui
                .button(tr!("object-manager-reset"))
                .on_hover_text(tr!("object-manager-reset", "hint"))
                .clicked()
            {
                actions.push(Action::ResetObjectManagerSettings);
            }
        });
    });
}

/// Display › New: the header rows of **new** workbooks (open workbooks keep
/// their own setting: Workbook menu, # checkbox) and the decimal separator of
/// **new** graphs (each graph keeps its own, in Graph Properties).
fn new_tab(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<Action>) {
    let (mut metadata, mut x_prime) = state.workbook_header_defaults();
    let before = (metadata, x_prime);
    let mut comma = state.new_graph_decimal_comma();
    let comma_before = comma;
    form(ui, "preferences_display_new", |rows| {
        rows.section(tr!("preferences-new-workbooks"));
        rows.note(tr!("preferences-new-workbooks-note"));
        rows.field(|ui| {
            ui.checkbox(&mut metadata, tr!("preferences-show-metadata-row"));
        });
        rows.field(|ui| {
            ui.checkbox(&mut x_prime, tr!("preferences-show-x-prime-row"));
        });
        rows.section(tr!("preferences-new-graphs"));
        rows.note(tr!("preferences-new-graphs-note"));
        rows.checkbox(
            &mut comma,
            tr!("preferences-new-graph-decimal-comma"),
            Some(tr!("preferences-new-graph-decimal-comma", "hint")),
        );
    });
    if (metadata, x_prime) != before {
        actions.push(Action::SetWorkbookHeaderDefaults { metadata, x_prime });
    }
    if comma != comma_before {
        actions.push(Action::SetNewGraphDecimalComma(comma));
    }
}

/// Export: how a graph file is written (SVG text, PNG resolution — the same
/// settings as Graph › Export) then Copy Graph: the formats offered together,
/// in the order in which applications see them. A setting changes right away
/// (`Action::SetCopyFormats`).
fn export_page(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<Action>) {
    let ctx = ui.ctx().clone();
    let mut entries: Vec<CopyFormatEntry> = state.copy_formats().to_vec();
    let before = entries.clone();
    let offered = entries.iter().filter(|entry| entry.on).count();
    let mut svg_mode = state.svg_text_mode();
    let svg_before = svg_mode;
    let mut scale = state.png_export_scale();
    let scale_before = scale;
    form(ui, "preferences_export", |rows| {
        rows.section(tr!("preferences-export-graph"));
        rows.row(tr!("menu-graph-svg-text"), |ui| {
            choice(
                ui,
                "preferences_svg_text",
                &mut svg_mode,
                &[
                    ChoiceOption::new(SvgTextMode::Text, tr!("menu-graph-svg-text-text"))
                        .hint(tr!("menu-graph-svg-text-text", "hint")),
                    ChoiceOption::new(SvgTextMode::Outlines, tr!("menu-graph-svg-text-outlines"))
                        .hint(tr!("menu-graph-svg-text-outlines", "hint")),
                ],
            );
        });
        rows.row(tr!("menu-graph-png-resolution"), |ui| {
            let options = PNG_EXPORT_SCALES.map(|option| {
                ChoiceOption::new(
                    option,
                    tr!("preferences-png-scale-option", scale = (option as u32)),
                )
            });
            choice(ui, "preferences_png_scale", &mut scale, &options);
        });
        rows.note(tr!("preferences-export-graph-note"));
        rows.section(tr!("preferences-copy-graph"));
        rows.note(tr!("preferences-copy-graph-note"));
        let ids: Vec<egui::Id> = entries
            .iter()
            .map(|entry| egui::Id::new(("preferences_copy_format", entry.format)))
            .collect();
        let mut over = None;
        for (index, id) in ids.iter().enumerate() {
            let entry = &mut entries[index];
            let name = format_name(entry.format);
            // The last offered format cannot be unticked: one must be offered.
            let last_offered = entry.on && offered == 1;
            rows.field(|ui| {
                let hovered = reorder_row(
                    ui,
                    *id,
                    tr!("preferences-copy-move", format = name.as_str()),
                    &tr!("preferences-copy-drag"),
                    |ui| {
                        ui.set_min_width(220.0);
                        ui.weak(format!("{}.", index + 1));
                        ui.add_enabled(
                            !last_offered,
                            egui::Checkbox::new(&mut entry.on, name.as_str()),
                        )
                        .on_disabled_hover_text(tr!("disabled-copy-last-format"));
                    },
                );
                if hovered {
                    over = Some(index);
                }
            });
        }
        if let Some((from, to)) = reorder_move(&ctx, &ids, over) {
            move_before(&mut entries, from, to);
        }
    });
    if svg_mode != svg_before {
        actions.push(Action::SetSvgTextMode(svg_mode));
    }
    if scale != scale_before {
        actions.push(Action::SetPngExportScale(scale));
    }
    if entries != before {
        actions.push(Action::SetCopyFormats(entries));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    /// A frame of the window, open or not; returns the painted texts.
    fn painted_texts(open: bool) -> Vec<String> {
        let session = stokesforge_session::Session::spawn(
            stokesforge_core::Project::new("Prefs").unwrap(),
            None,
        )
        .unwrap();
        let mut state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        if open {
            state.dispatch(Action::OpenPreferences);
        }
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let mut actions = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 700.0),
                )),
                ..Default::default()
            },
            |_| {
                preferences_window(&ctx, &state, &mut actions);
            },
        );
        output.textures_delta.clear();
        session.shutdown().unwrap();
        output
            .shapes
            .iter()
            .filter_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) => Some(shape.galley.job.text.clone()),
                _ => None,
            })
            .collect()
    }

    /// Closed, the window paints nothing; open, it shows its title, its
    /// section and the language.
    #[test]
    fn the_window_shows_the_language_when_it_is_open() {
        assert!(painted_texts(false).is_empty());
        let texts = painted_texts(true);
        assert!(texts.contains(&tr!("preferences-title")), "{texts:?}");
        assert!(texts.contains(&tr!("preferences-general")), "{texts:?}");
        assert!(texts.contains(&tr!("menu-language")), "{texts:?}");
    }

    /// The bench of the real window: an application state without a preferences
    /// file, the actions replayed on every frame.
    struct Bench {
        _session: stokesforge_session::Session,
        state: AppState,
    }

    fn bench() -> egui_kittest::Harness<'static, Bench> {
        let session = stokesforge_session::Session::spawn(
            stokesforge_core::Project::new("Prefs").unwrap(),
            None,
        )
        .unwrap();
        let mut state = AppState::new_with_preferences(
            session.handle(),
            "test.stf".to_string(),
            stokesforge_app_state::preferences::Preferences::load_from(None),
        )
        .unwrap();
        state.dispatch(Action::OpenPreferences);
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(760.0, 600.0))
            .build_ui_state(
                |ui, bench: &mut Bench| {
                    let mut actions = Vec::new();
                    preferences_window(ui.ctx(), &bench.state, &mut actions);
                    for action in actions {
                        bench.state.dispatch(action);
                    }
                },
                Bench {
                    _session: session,
                    state,
                },
            );
        harness
            .ctx
            .set_fonts(stokesforge_render_egui::font_definitions());
        crate::theme::install(&harness.ctx);
        harness.run();
        harness
    }

    fn order(harness: &egui_kittest::Harness<'static, Bench>) -> Vec<(CopyFormat, bool)> {
        harness
            .state()
            .state
            .copy_formats()
            .iter()
            .map(|entry| (entry.format, entry.on))
            .collect()
    }

    fn go_to_export(harness: &mut egui_kittest::Harness<'static, Bench>) {
        harness.get_by_label(&tr!("preferences-export")).click();
        harness.run();
    }

    /// Unticking a format removes it from the offer; the last one offered stays ticked.
    #[test]
    fn unticking_a_format_removes_it_and_the_last_one_stays() {
        let mut harness = bench();
        go_to_export(&mut harness);
        harness.get_by_label("PNG").click();
        harness.run();
        harness.get_by_label("PDF").click();
        harness.run();
        assert_eq!(
            order(&harness),
            [
                (CopyFormat::Svg, true),
                (CopyFormat::Png, false),
                (CopyFormat::Pdf, false)
            ]
        );
        harness.get_by_label("SVG").click();
        harness.run();
        assert!(harness.state().state.copy_formats()[0].on, "last offered");
    }

    /// Dragging "3. PDF" above "1. SVG": the offered order changes.
    #[test]
    fn dragging_a_row_changes_the_offered_order() {
        let mut harness = bench();
        go_to_export(&mut harness);
        let from = harness
            .get_by_label(&tr!("preferences-copy-move", format = "PDF"))
            .rect()
            .center();
        let first = harness
            .get_by_label(&tr!("preferences-copy-move", format = "SVG"))
            .rect();
        harness.hover_at(from);
        harness.step();
        harness.drag_at(from);
        harness.step();
        harness.hover_at(from - egui::vec2(0.0, 12.0));
        harness.step();
        harness.hover_at(egui::pos2(first.center().x, first.top() + 2.0));
        harness.step();
        harness.step();
        harness.drop_at(egui::pos2(first.center().x, first.top() + 2.0));
        harness.run();
        let formats: Vec<CopyFormat> = order(&harness).into_iter().map(|(f, _)| f).collect();
        assert_eq!(formats, [CopyFormat::Pdf, CopyFormat::Svg, CopyFormat::Png]);
    }

    /// Autosave (#317): the checkbox (ticked by default) and the period field
    /// are painted in General; unticking the box switches it off and
    /// greys out the field, ticking it again restores it.
    #[test]
    fn the_general_section_paints_the_autosave() {
        let mut harness = bench();
        harness.get_by_label(&tr!("preferences-autosave"));
        harness.get_by_label(&tr!("preferences-autosave-minutes"));
        let active_fields = |h: &egui_kittest::Harness<'static, Bench>| {
            use egui::accesskit::Role;
            use egui_kittest::kittest::NodeT;
            [Role::TextInput, Role::SpinButton]
                .into_iter()
                .flat_map(|role| h.query_all_by_role(role))
                .filter(|node| !node.accesskit_node().is_disabled())
                .count()
        };
        let active = active_fields(&harness);
        assert!(harness.state().state.autosave_enabled());
        harness.get_by_label(&tr!("preferences-autosave")).click();
        harness.run();
        assert!(!harness.state().state.autosave_enabled());
        assert_eq!(
            active_fields(&harness),
            active - 1,
            "the period field is greyed out"
        );
        harness.get_by_label(&tr!("preferences-autosave")).click();
        harness.run();
        assert!(harness.state().state.autosave_enabled());
        assert_eq!(active_fields(&harness), active);
    }

    /// Display section: Metadata is ticked and x' unticked by default; ticking
    /// x' sets the new workbooks.
    #[test]
    fn the_display_section_sets_the_rows_of_new_workbooks() {
        let mut harness = bench();
        go_to_display(&mut harness, &tr!("preferences-display-new"));
        assert_eq!(
            harness.state().state.workbook_header_defaults(),
            (true, false)
        );
        harness
            .get_by_label(&tr!("preferences-show-x-prime-row"))
            .click();
        harness.run();
        harness
            .get_by_label(&tr!("preferences-show-metadata-row"))
            .click();
        harness.run();
        assert_eq!(
            harness.state().state.workbook_header_defaults(),
            (false, true)
        );
    }

    fn go_to_display(harness: &mut egui_kittest::Harness<'static, Bench>, sub_tab: &str) {
        harness.get_by_label(&tr!("preferences-display")).click();
        harness.run();
        harness.get_by_label(sub_tab).click();
        harness.run();
    }

    /// The rubrics are a vertical strip at the left of the content (not a
    /// horizontal strip above it): the three labels share one column, and the
    /// content starts to their right.
    #[test]
    fn the_rubrics_are_a_column_at_the_left_of_the_content() {
        let harness = bench();
        let general = harness.get_by_label(&tr!("preferences-general")).rect();
        let display = harness.get_by_label(&tr!("preferences-display")).rect();
        let export = harness.get_by_label(&tr!("preferences-export")).rect();
        assert!(display.top() >= general.bottom(), "stacked");
        assert!(export.top() >= display.bottom(), "stacked");
        assert!((general.left() - display.left()).abs() < 1.0, "one column");
        let language = harness.get_by_label(&tr!("menu-language")).rect();
        assert!(language.left() >= general.right(), "content on the right");
    }

    /// Display › Object Manager: the lists and the expansion defaults are the
    /// ones of the panel's own menu (same state), and Reset restores them.
    #[test]
    fn the_object_manager_sub_tab_sets_the_lists_and_resets() {
        let mut harness = bench();
        go_to_display(&mut harness, &tr!("preferences-display-object-manager"));
        let initial = harness.state().state.object_manager_settings();
        assert!(!initial.show_scales);
        harness
            .get_by_label(&tr!("object-manager-show-axes"))
            .click();
        harness.run();
        harness
            .get_by_label(&tr!("object-manager-expand-layers"))
            .click();
        harness.run();
        let changed = harness.state().state.object_manager_settings();
        assert!(changed.show_scales, "Show Axes ticked");
        assert!(!changed.layers_open, "Layers no longer expanded by default");
        harness.get_by_label(&tr!("object-manager-reset")).click();
        harness.run();
        assert_eq!(harness.state().state.object_manager_settings(), initial);
    }

    /// The sub-tab is remembered when the rubric is left and reopened.
    #[test]
    fn the_display_sub_tab_is_remembered() {
        let mut harness = bench();
        go_to_display(&mut harness, &tr!("preferences-display-new"));
        harness.get_by_label(&tr!("preferences-export")).click();
        harness.run();
        harness.get_by_label(&tr!("preferences-display")).click();
        harness.run();
        harness.get_by_label(&tr!("preferences-show-x-prime-row"));
    }

    /// Export: the SVG text mode and the PNG resolution are choices of this rubric.
    #[test]
    fn the_export_rubric_sets_the_svg_text_and_the_png_resolution() {
        let mut harness = bench();
        go_to_export(&mut harness);
        assert_eq!(harness.state().state.png_export_scale(), 4.0);
        harness.get_by_label("×2").click();
        harness.run();
        assert_eq!(harness.state().state.png_export_scale(), 2.0);
        harness
            .get_by_label(&tr!("menu-graph-svg-text-outlines"))
            .click();
        harness.run();
        assert_eq!(
            harness.state().state.svg_text_mode(),
            stokesforge_render_svg::SvgTextMode::Outlines
        );
    }

    /// General › Analysis: the default "Values" choice (M5) and, in Display,
    /// the formula help and the decimal comma of new graphs.
    #[test]
    fn the_analysis_default_the_formula_help_and_the_new_graph_comma_are_settable() {
        let mut harness = bench();
        assert!(!harness.state().state.analysis_keep_results_default());
        harness.get_by_label(&tr!("analysis-output-keep")).click();
        harness.run();
        assert!(harness.state().state.analysis_keep_results_default());

        go_to_display(&mut harness, &tr!("preferences-display-interface"));
        assert!(harness.state().state.show_formula_help());
        harness
            .get_by_label(&tr!("preferences-show-formula-help"))
            .click();
        harness.run();
        assert!(!harness.state().state.show_formula_help());

        go_to_display(&mut harness, &tr!("preferences-display-new"));
        assert!(!harness.state().state.new_graph_decimal_comma());
        harness
            .get_by_label(&tr!("preferences-new-graph-decimal-comma"))
            .click();
        harness.run();
        assert!(harness.state().state.new_graph_decimal_comma());
    }
}
