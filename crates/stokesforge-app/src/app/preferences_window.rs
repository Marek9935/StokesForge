//! File › Preferences (#148): the application preferences window.
//!
//! Layout (decisions of 2026-10-29): the **rubrics are vertical tabs on the
//! left** ([`TabStripSide::Left`]) — General, Display, Export, Folders — and a rubric
//! with many settings has **horizontal sub-tabs** at the top (Display:
//! Interface · Object Manager · New). Pages are short, without scrolling where
//! possible. **General** holds the language, the file caps, autosave and the
//! analysis defaults; **Display** the formula help icon, the Object Manager
//! lists and the settings of *new* workbooks and graphs; **Export** the text of
//! SVG exports, the PNG resolution and the Copy Graph formats (the order is
//! changed by dragging, `reorder_list.rs`, each format is ticked); **Folders**
//! shows where StokesForge keeps its own files (configuration folder, with Open
//! Folder and its content; recovery copies) and sets the default folders of the
//! file dialogs: Open and Save, Import, Export (`default_folders.rs` in
//! `stokesforge-app-state` holds the rule). The menus keep
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
use stokesforge_app_state::default_folders::FolderKind;
use stokesforge_app_state::file_picker::PickRequest;
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
    Folders,
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
                (Page::Folders, tr!("preferences-folders")),
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
                    Page::Folders => folders_page(ui, state, actions),
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

/// How often what the page shows about the disk is looked at again.
const FOLDERS_REFRESH: std::time::Duration = std::time::Duration::from_secs(3);

/// What the last look at a folder said, for the text it was about.
#[derive(Clone)]
struct FolderProbe {
    text: String,
    /// Written by the probing thread.
    answer: std::sync::Arc<std::sync::Mutex<Option<bool>>>,
    /// The last answer received: kept while the next look is under way, so the
    /// note does not flicker.
    shown: Option<bool>,
    at: std::time::Instant,
}

/// Does the folder `text` exist? `None` while the first look is under way, and
/// for an empty text. The disk is looked at on a **thread**, never on the
/// interface thread: the folder may sit on a network drive that does not answer.
fn folder_exists(ctx: &egui::Context, id: egui::Id, text: &str) -> Option<bool> {
    if text.is_empty() {
        return None;
    }
    let mut probe: Option<FolderProbe> = ctx.data(|data| data.get_temp(id));
    if let Some(probe) = probe.as_mut() {
        if let Some(answer) = *probe.answer.lock().unwrap_or_else(|err| err.into_inner()) {
            probe.shown = Some(answer);
        }
    }
    let stale = probe
        .as_ref()
        .is_none_or(|probe| probe.text != text || probe.at.elapsed() >= FOLDERS_REFRESH);
    if stale {
        let shown = probe
            .as_ref()
            .filter(|probe| probe.text == text)
            .and_then(|probe| probe.shown);
        let answer = std::sync::Arc::new(std::sync::Mutex::new(None));
        let (worker, folder, wake) = (answer.clone(), text.to_owned(), ctx.clone());
        std::thread::spawn(move || {
            let exists = std::path::Path::new(&folder).is_dir();
            *worker.lock().unwrap_or_else(|err| err.into_inner()) = Some(exists);
            wake.request_repaint();
        });
        probe = Some(FolderProbe {
            text: text.to_owned(),
            answer,
            shown,
            at: std::time::Instant::now(),
        });
    }
    let probe = probe?;
    let shown = probe.shown;
    ctx.data_mut(|data| data.insert_temp(id, probe));
    ctx.request_repaint_after(FOLDERS_REFRESH);
    shown
}

/// The names of the files of the configuration folder, sorted, looked at again
/// every few seconds (a local folder of a few small files).
fn config_files(ctx: &egui::Context, folder: &std::path::Path) -> Vec<String> {
    let id = egui::Id::new(("preferences_config_files", folder));
    let cached: Option<(std::time::Instant, Vec<String>)> = ctx.data(|data| data.get_temp(id));
    if let Some((at, names)) = cached {
        if at.elapsed() < FOLDERS_REFRESH {
            return names;
        }
    }
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    ctx.data_mut(|data| data.insert_temp(id, (std::time::Instant::now(), names.clone())));
    ctx.request_repaint_after(FOLDERS_REFRESH);
    names
}

/// What a known file of the configuration folder holds.
fn config_file_description(name: &str) -> Option<String> {
    Some(match name {
        "preferences.json" => tr!("preferences-folders-file-preferences"),
        "couleurs.json" => tr!("preferences-folders-file-colors"),
        "import_recipes.json" => tr!("preferences-folders-file-templates"),
        "import_recent.json" => tr!("preferences-folders-file-recent"),
        "demo.stf" => tr!("preferences-folders-file-demo"),
        _ => return None,
    })
}

/// Folders: where StokesForge keeps its own files, and the default folders of the
/// file dialogs (#148). A default is typed (written on Enter or when the field
/// loses the focus) or picked with Browse…; empty means "the last folder used".
fn folders_page(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<Action>) {
    let ctx = ui.ctx().clone();
    form(ui, "preferences_folders", |rows| {
        rows.section(tr!("preferences-folders-config"));
        rows.note(tr!("preferences-folders-config-note"));
        let config = state.config_folder();
        rows.row(tr!("preferences-folders-location"), |ui| match &config {
            Some(folder) => {
                ui.add(egui::Label::new(folder.display().to_string()).selectable(true));
            }
            None => {
                ui.weak(tr!("preferences-folders-unavailable"));
            }
        });
        if let Some(folder) = &config {
            rows.field(|ui| {
                if ui
                    .button(tr!("preferences-folders-open"))
                    .on_hover_text(tr!("preferences-folders-open", "hint"))
                    .clicked()
                {
                    actions.push(Action::OpenFolder(folder.clone()));
                }
            });
            let names = config_files(&ctx, folder);
            rows.row(tr!("preferences-folders-files"), |ui| {
                ui.vertical(|ui| {
                    if names.is_empty() {
                        ui.weak(tr!("preferences-folders-empty"));
                    }
                    for name in &names {
                        match config_file_description(name) {
                            Some(what) => ui.label(format!("{name} · {what}")),
                            None => ui.label(name),
                        };
                    }
                });
            });
        }
        rows.row(tr!("preferences-folders-recovery"), |ui| {
            match state.recovery_folder() {
                Some(folder) => {
                    ui.add(egui::Label::new(folder.display().to_string()).selectable(true));
                }
                None => {
                    ui.weak(tr!("preferences-folders-unavailable"));
                }
            }
        });
        rows.note(tr!("preferences-folders-recovery-note"));

        rows.section(tr!("preferences-folders-default"));
        rows.note(tr!("preferences-folders-default-note"));
        for (kind, label) in [
            (FolderKind::Projects, tr!("preferences-folders-projects")),
            (FolderKind::Import, tr!("preferences-folders-import")),
            (FolderKind::Export, tr!("preferences-folders-export")),
        ] {
            let saved = state.default_folder(kind).unwrap_or("").to_owned();
            let mut missing = false;
            rows.row(label, |ui| {
                missing = default_folder_field(ui, &ctx, kind, &saved, actions);
            });
            if missing {
                rows.note(tr!("preferences-folders-not-found"));
            }
        }
    });
}

/// The field and Browse… of one default folder; returns `true` if the folder
/// written in it is known not to exist.
fn default_folder_field(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    kind: FolderKind,
    saved: &str,
    actions: &mut Vec<Action>,
) -> bool {
    let buffer_id = egui::Id::new(("preferences_folder_buffer", kind));
    let mut text: String = ctx
        .data(|data| data.get_temp(buffer_id))
        .unwrap_or_else(|| saved.to_owned());
    let response = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id_salt(("preferences_folder_field", kind))
            .desired_width(220.0),
    );
    if response.has_focus() {
        ctx.data_mut(|data| data.insert_temp(buffer_id, text.clone()));
    } else {
        ctx.data_mut(|data| data.remove::<String>(buffer_id));
        if response.lost_focus() && text != saved {
            actions.push(Action::SetDefaultFolder {
                kind,
                folder: text.clone(),
            });
        }
    }
    if ui
        .button(tr!("preferences-folders-browse"))
        .on_hover_text(tr!("preferences-folders-browse", "hint"))
        .clicked()
    {
        actions.push(Action::PickFile(PickRequest::ChooseDefaultFolder { kind }));
    }
    folder_exists(ctx, buffer_id.with("probe"), saved) == Some(false)
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
        /// Every action the window pushed, in order.
        seen: Vec<Action>,
    }

    fn bench() -> egui_kittest::Harness<'static, Bench> {
        bench_with(stokesforge_app_state::preferences::Preferences::load_from(
            None,
        ))
    }

    fn bench_with(
        preferences: stokesforge_app_state::preferences::Preferences,
    ) -> egui_kittest::Harness<'static, Bench> {
        let session = stokesforge_session::Session::spawn(
            stokesforge_core::Project::new("Prefs").unwrap(),
            None,
        )
        .unwrap();
        let mut state =
            AppState::new_with_preferences(session.handle(), "test.stf".to_string(), preferences)
                .unwrap();
        state.dispatch(Action::OpenPreferences);
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(760.0, 600.0))
            .build_ui_state(
                |ui, bench: &mut Bench| {
                    let mut actions = Vec::new();
                    preferences_window(ui.ctx(), &bench.state, &mut actions);
                    for action in actions {
                        bench.seen.push(action.clone());
                        bench.state.dispatch(action);
                    }
                },
                Bench {
                    _session: session,
                    state,
                    seen: Vec::new(),
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

    // -- Folders rubric (#148, lot C) --

    /// A bench whose preferences file lives in a fresh folder holding two of the
    /// application's files.
    fn folders_bench() -> (tempfile::TempDir, egui_kittest::Harness<'static, Bench>) {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("preferences.json");
        std::fs::write(&file, "{}").unwrap();
        std::fs::write(dir.path().join("couleurs.json"), "{}").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "x").unwrap();
        let harness = bench_with(stokesforge_app_state::preferences::Preferences::load_from(
            Some(file),
        ));
        (dir, harness)
    }

    fn go_to_folders(harness: &mut egui_kittest::Harness<'static, Bench>) {
        harness.get_by_label(&tr!("preferences-folders")).click();
        harness.run();
    }

    /// The rubric shows the configuration folder (the one of the preferences file),
    /// its files with what they hold, the recovery row and the three default
    /// folders.
    #[test]
    fn the_folders_rubric_shows_the_configuration_folder_its_files_and_the_three_rows() {
        let (dir, mut harness) = folders_bench();
        go_to_folders(&mut harness);
        harness.get_by_label(&dir.path().display().to_string());
        harness.get_by_label(&tr!("preferences-folders-open"));
        harness.get_by_label(&format!(
            "preferences.json · {}",
            tr!("preferences-folders-file-preferences")
        ));
        harness.get_by_label(&format!(
            "couleurs.json · {}",
            tr!("preferences-folders-file-colors")
        ));
        harness.get_by_label("notes.txt");
        harness.get_by_label(&tr!("preferences-folders-recovery"));
        harness.get_by_label(&tr!("preferences-folders-projects"));
        harness.get_by_label(&tr!("preferences-folders-import"));
        harness.get_by_label(&tr!("preferences-folders-export"));
        assert_eq!(
            harness
                .query_all_by_label(&tr!("preferences-folders-browse"))
                .count(),
            3,
            "one Browse… per default folder"
        );
    }

    /// Open Folder asks the shell to open the configuration folder.
    #[test]
    fn open_folder_asks_for_the_configuration_folder() {
        let (dir, mut harness) = folders_bench();
        go_to_folders(&mut harness);
        harness
            .get_by_label(&tr!("preferences-folders-open"))
            .click();
        harness.run();
        assert!(harness
            .state()
            .seen
            .contains(&Action::OpenFolder(dir.path().to_path_buf())));
    }

    /// Browse… asks for a folder picker of its own kind.
    #[test]
    fn browse_asks_for_a_folder_of_its_kind() {
        let (_dir, mut harness) = folders_bench();
        go_to_folders(&mut harness);
        let label = tr!("preferences-folders-browse");
        let browse: Vec<_> = harness.query_all_by_label(&label).collect();
        browse[1].click();
        harness.run();
        assert!(harness.state().seen.contains(&Action::PickFile(
            PickRequest::ChooseDefaultFolder {
                kind: FolderKind::Import
            }
        )));
    }

    /// Typing in a field writes the default when the field loses the focus (Enter).
    #[test]
    fn a_typed_default_folder_is_written_on_enter() {
        use egui::accesskit::Role;
        let (_dir, mut harness) = folders_bench();
        go_to_folders(&mut harness);
        let fields: Vec<_> = harness.query_all_by_role(Role::TextInput).collect();
        assert_eq!(fields.len(), 3);
        fields[2].focus();
        harness.run();
        harness
            .query_all_by_role(Role::TextInput)
            .nth(2)
            .unwrap()
            .type_text("/figures");
        harness.run();
        assert_eq!(
            harness.state().state.default_folder(FolderKind::Export),
            None,
            "not written while typing"
        );
        harness.key_press(egui::Key::Enter);
        harness.run();
        assert_eq!(
            harness.state().state.default_folder(FolderKind::Export),
            Some("/figures")
        );
        assert_eq!(
            harness.state().state.default_folder(FolderKind::Projects),
            None
        );
    }

    /// A default that is not a folder gets the note "Folder not found"; one that is
    /// does not. The disk is looked at on a thread: the frames are repeated until
    /// it has answered.
    #[test]
    fn a_missing_default_folder_gets_a_note_and_an_existing_one_does_not() {
        let (dir, mut harness) = folders_bench();
        go_to_folders(&mut harness);
        let note = tr!("preferences-folders-not-found");
        harness
            .state_mut()
            .state
            .dispatch(Action::SetDefaultFolder {
                kind: FolderKind::Import,
                folder: dir.path().join("absent").display().to_string(),
            });
        harness
            .state_mut()
            .state
            .dispatch(Action::SetDefaultFolder {
                kind: FolderKind::Export,
                folder: dir.path().display().to_string(),
            });
        let mut found = false;
        for _ in 0..200 {
            harness.run();
            if harness.query_by_label(&note).is_some() {
                found = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(found, "the note never appeared");
        assert_eq!(
            harness.query_all_by_label(&note).count(),
            1,
            "witness: only the missing folder has it"
        );
    }

    // -- The shell side: answers of the pickers, Open Folder --

    fn app_on_file(
        file: &std::path::Path,
    ) -> (stokesforge_session::Session, crate::app::StokesForgeApp) {
        let session = stokesforge_session::Session::spawn(
            stokesforge_core::Project::new("Folders").unwrap(),
            None,
        )
        .unwrap();
        let state = AppState::new_with_preferences(
            session.handle(),
            String::new(),
            stokesforge_app_state::preferences::Preferences::load_from(Some(file.to_path_buf())),
        )
        .unwrap();
        (session, crate::app::StokesForgeApp::new(state))
    }

    /// Polls the (fake) picker until it has answered.
    fn answer(
        app: &mut crate::app::StokesForgeApp,
        request: PickRequest,
        picked: Option<Vec<std::path::PathBuf>>,
    ) {
        app.file_picker.start_fake(request, picked);
        let ctx = egui::Context::default();
        for _ in 0..500 {
            let mut actions = Vec::new();
            app.poll_file_picker(&ctx, &mut actions);
            if !app.file_picker.is_open() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the fake picker never answered");
    }

    /// A chosen file feeds the memory of its kind; a cancelled dialog changes
    /// nothing; choosing a default folder sets it (and feeds nothing).
    #[test]
    fn the_answers_of_the_pickers_feed_the_memory_cancel_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (session, mut app) = app_on_file(&dir.path().join("preferences.json"));
        let open_candidates = |app: &crate::app::StokesForgeApp| {
            app.state.folder_candidates(&PickRequest::OpenProject)
        };
        answer(&mut app, PickRequest::OpenProject, None);
        assert_eq!(open_candidates(&app).session_last, None, "cancelled");
        answer(
            &mut app,
            PickRequest::OpenProject,
            Some(vec![std::path::PathBuf::from("/work/a.stf")]),
        );
        assert_eq!(
            open_candidates(&app).session_last,
            Some(std::path::PathBuf::from("/work"))
        );
        answer(&mut app, PickRequest::OpenProject, None);
        assert_eq!(
            open_candidates(&app).session_last,
            Some(std::path::PathBuf::from("/work")),
            "a later cancel keeps it"
        );
        answer(
            &mut app,
            PickRequest::ChooseDefaultFolder {
                kind: FolderKind::Export,
            },
            Some(vec![std::path::PathBuf::from("/figs")]),
        );
        assert_eq!(app.state.default_folder(FolderKind::Export), Some("/figs"));
        assert_eq!(open_candidates(&app).session_last, Some("/work".into()));
        session.shutdown().unwrap();
    }

    thread_local! {
        static OPENED: std::cell::RefCell<Vec<std::path::PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    fn record_open(folder: &std::path::Path) -> std::io::Result<()> {
        OPENED.with(|opened| opened.borrow_mut().push(folder.to_path_buf()));
        Ok(())
    }

    fn refuse_open(_: &std::path::Path) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "no",
        ))
    }

    /// `Action::OpenFolder` goes to the shell's opener; its failure is told in the
    /// status bar.
    #[test]
    fn open_folder_goes_to_the_opener_and_a_failure_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let (session, mut app) = app_on_file(&dir.path().join("preferences.json"));
        let ctx = egui::Context::default();
        app.folder_opener = record_open;
        app.dispatch_actions(&ctx, vec![Action::OpenFolder(dir.path().to_path_buf())]);
        OPENED.with(|opened| assert_eq!(*opened.borrow(), [dir.path().to_path_buf()]));
        assert!(app.state.status().is_none_or(|status| !status.is_error()));
        app.folder_opener = refuse_open;
        app.dispatch_actions(&ctx, vec![Action::OpenFolder(dir.path().to_path_buf())]);
        let status = app.state.status().expect("a message");
        assert!(status.is_error());
        assert!(status
            .text
            .contains(&stokesforge_app_state::errors::io::error_message(
                &std::io::Error::new(std::io::ErrorKind::PermissionDenied, "no")
            )));
        session.shutdown().unwrap();
    }

    /// The real opener creates a missing folder first (no program is started
    /// when the folder cannot be created).
    #[test]
    fn the_real_opener_creates_the_folder_and_fails_cleanly_when_it_cannot() {
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("file");
        std::fs::write(&blocked, "x").unwrap();
        assert!(crate::app::open_folder_in_file_manager(&blocked.join("inside")).is_err());
    }
}
