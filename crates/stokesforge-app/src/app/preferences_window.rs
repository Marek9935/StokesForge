//! File › Preferences (#148): the application preferences window,
//! in three tabs: **General** (the language, also settable through
//! File › Language, and the analysis), **Display** (new workbooks) and
//! **Export** (the Copy Graph formats: the order is changed by dragging,
//! `reorder_list.rs`, each format is ticked). The other sections of the
//! inventory of #148 (Folders and Paths) will be added one by one, with
//! their questions settled.
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
use super::reorder_list::{move_before, reorder_move, reorder_row};
use super::tab_strip::{tab_strip, TabStripSide};
use crate::number_field::number_field;
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::preferences::{CopyFormat, CopyFormatEntry};
use stokesforge_app_state::quantities;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

/// The sections, in the order of #148, already delivered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum Page {
    #[default]
    General,
    Display,
    Export,
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
    // The open section is only a convenience of the window: neither `AppState`
    // nor the preferences file.
    let page_id = egui::Id::new("stokesforge_preferences_page");
    let mut page: Page = ctx.data(|data| data.get_temp(page_id)).unwrap_or_default();
    let outcome = modal_live_settings(
        ctx,
        egui::Id::new("stokesforge_preferences"),
        &tr!("preferences-title"),
        InteractiveSize::Class(SizeClass::Medium),
        |ui| {
            let tabs = [
                (Page::General, tr!("preferences-general")),
                (Page::Display, tr!("preferences-display")),
                (Page::Export, tr!("preferences-export")),
            ];
            tab_strip(
                ui,
                TabStripSide::Top,
                &tabs,
                page,
                |picked| page = picked,
                None,
                || {},
            );
            match page {
                Page::General => chosen = general_page(ui, state, actions),
                Page::Display => display_page(ui, state, actions),
                Page::Export => export_page(ui, state, actions),
            }
        },
    );
    ctx.data_mut(|data| data.insert_temp(page_id, page));
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
        rows.section(tr!("preferences-general"));
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
        // Application preference (#288); each computation has its own.
        rows.section(tr!("preferences-analysis"));
        rows.checkbox(
            &mut low_r2,
            tr!("preferences-low-r2-dialog"),
            Some(tr!("preferences-low-r2-dialog", "hint")),
        );
    });
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

/// The header rows of **new** workbooks; open workbooks
/// keep their own setting (Workbook menu, # checkbox).
fn display_page(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<Action>) {
    let (mut metadata, mut x_prime) = state.workbook_header_defaults();
    let before = (metadata, x_prime);
    form(ui, "preferences_display", |rows| {
        rows.section(tr!("preferences-new-workbooks"));
        rows.note(tr!("preferences-new-workbooks-note"));
        rows.field(|ui| {
            ui.checkbox(&mut metadata, tr!("preferences-show-metadata-row"));
        });
        rows.field(|ui| {
            ui.checkbox(&mut x_prime, tr!("preferences-show-x-prime-row"));
        });
    });
    if (metadata, x_prime) != before {
        actions.push(Action::SetWorkbookHeaderDefaults { metadata, x_prime });
    }
}

/// Copy Graph: the formats offered together, in the order in which applications
/// see them. A setting changes right away (`Action::SetCopyFormats`).
fn export_page(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<Action>) {
    let ctx = ui.ctx().clone();
    let mut entries: Vec<CopyFormatEntry> = state.copy_formats().to_vec();
    let before = entries.clone();
    let offered = entries.iter().filter(|entry| entry.on).count();
    form(ui, "preferences_export", |rows| {
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
        harness.get_by_label(&tr!("preferences-display")).click();
        harness.run();
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
}
