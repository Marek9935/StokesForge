//! Import wizard (#2, #3–#8): the six steps (files, format, headers,
//! columns, metadata, destination), the preview in lines colored by role and
//! the labels of the choices. A single entry point for `app.rs`:
//! [`import_window`], a `modal_dialog_with` (never a raw `egui::Window`).
//!
//! Moved out of `app.rs` as is (#118), with one touch-up: the label of a data
//! end (`data_end_label`) is a separate table, testable and guarded by
//! `tests/labels.rs`, instead of a closure of `data_end_row`.
//! The tests are at the end of the file: they paint the wizard without a
//! window and read the painted texts (content, color, position), never the
//! source.

use eframe::egui;
use stokesforge_import::{DecimalStyle, HeaderSkip, NameSource, Separator};

use crate::icons::{icon_button, icon_image, Icon, IconRole};
use crate::number_field::number_field;
use crate::theme::{section_title, FIELD_WIDTH_MEDIUM, FIELD_WIDTH_SHORT};
use stokesforge_app_state::errors;
use stokesforge_app_state::file_picker::PickRequest;
use stokesforge_app_state::quantities;
use stokesforge_app_state::state::{
    Action, AppState, ImportDestination, ImportLineAction, ImportStep,
};
use stokesforge_app_state::tr;

use super::dialogs::{
    modal_confirm, modal_wizard, ConfirmOutcome, DialogFooter, DialogOutcome, InteractiveSize,
    PlainSize, FOOTER_BUTTON_SIZE,
};
use super::form::{choice, choice_by, form, ChoiceOption, Form};
use super::hover_help::{help_beside, help_text};

/// Label of a "Data end" option: that of the variant, whatever text or number
/// it carries.
fn data_end_label(end: &stokesforge_import::DataEnd) -> String {
    use stokesforge_import::DataEnd;
    match end {
        DataEnd::Auto => tr!("import-dataend-auto"),
        DataEnd::None => tr!("import-option-none"),
        DataEnd::FirstBlankLine => tr!("import-dataend-blank"),
        DataEnd::Marker(_) => tr!("import-dataend-marker"),
        DataEnd::LinesAfterHeader(_) => tr!("import-dataend-after-header"),
        DataEnd::LinesBeforeEnd(_) => tr!("import-dataend-before-end"),
    }
}

/// "Data end" setting of the import recipe (issue #4): automatic cut by
/// default, can be turned off. The text or number tied to the chosen option
/// stays in the recipe while switching. No label: the "Data end" section title
/// carries it (#227).
fn data_end_row(ui: &mut egui::Ui, end: &mut stokesforge_import::DataEnd) {
    use stokesforge_import::DataEnd;
    ui.horizontal(|ui| {
        let options = [
            DataEnd::Auto,
            DataEnd::None,
            DataEnd::FirstBlankLine,
            DataEnd::Marker(String::new()),
            DataEnd::LinesAfterHeader(100),
            DataEnd::LinesBeforeEnd(1),
        ]
        .map(|candidate| {
            let option = ChoiceOption::new(candidate.clone(), data_end_label(&candidate));
            if matches!(candidate, DataEnd::Auto) {
                option.hint(tr!("import-dataend-auto", "hint"))
            } else {
                option
            }
        });
        choice_by(
            ui,
            "stokesforge_import_data_end",
            end,
            &options,
            same_variant,
        );
        match end {
            DataEnd::Marker(text) => {
                ui.add(egui::TextEdit::singleline(text).desired_width(FIELD_WIDTH_MEDIUM));
            }
            DataEnd::LinesAfterHeader(n) | DataEnd::LinesBeforeEnd(n) => {
                number_field(ui, "import_n", n, &quantities::COUNT);
            }
            _ => {}
        }
    });
}

/// Two options of the same variant, whatever they carry: choosing one again
/// keeps the text or number already typed.
fn same_variant<T>(a: &T, b: &T) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// Label shown for a [`Separator`] in the import window.
fn separator_label(separator: &Separator) -> String {
    match separator {
        Separator::Comma => tr!("import-option-comma"),
        Separator::Semicolon => tr!("import-delimiter-semicolon"),
        Separator::Tab => tr!("import-delimiter-tab"),
        Separator::Whitespace => tr!("import-delimiter-whitespace"),
        Separator::Custom(_) => tr!("import-option-other"),
        Separator::FixedWidth(_) => tr!("import-delimiter-fixed"),
        Separator::Auto => tr!("common-auto"),
    }
}

/// Label of a forced encoding.
fn encoding_label(encoding: stokesforge_import::Encoding) -> String {
    use stokesforge_import::Encoding;
    // Encoding names are technical names: only "Auto" is translated.
    match encoding {
        Encoding::Auto => tr!("common-auto"),
        Encoding::Utf8 => "UTF-8".to_string(),
        Encoding::Windows1252 => "Windows-1252".to_string(),
        Encoding::Latin1 => "ISO 8859-1".to_string(),
        Encoding::Utf16Le => "UTF-16 LE".to_string(),
        Encoding::Utf16Be => "UTF-16 BE".to_string(),
    }
}

/// Label of a thousands separator.
fn thousands_label(thousands: Option<char>) -> String {
    match thousands {
        None => tr!("import-option-none"),
        Some(' ') => tr!("import-option-space"),
        Some(',') => tr!("import-option-comma"),
        Some('.') => tr!("import-option-dot"),
        Some('\'') => tr!("import-option-apostrophe"),
        Some(_) => tr!("import-option-other"),
    }
}

/// Label shown for a [`DecimalStyle`] in the import window.
fn decimal_label(decimal: DecimalStyle) -> String {
    match decimal {
        DecimalStyle::Dot => tr!("import-option-dot"),
        DecimalStyle::Comma => tr!("import-option-comma"),
        DecimalStyle::Auto => tr!("common-auto"),
    }
}

/// A "from … to …" row assigning a subheader role (see [`import_window`]): a
/// checkbox to enable/disable the role (`None` vs `Some`), two counters for
/// the range. `id_source` tells the three roles
/// (`long_name`/`unit`/`comment`) apart; without it they would share the same
/// auto-generated id (same relative position in their respective `if`
/// branches) and step on each other.
fn subheader_role_row(
    ui: &mut egui::Ui,
    id_source: &str,
    label: String,
    hint: String,
    range: &mut Option<stokesforge_import::SubheaderLineRange>,
) {
    ui.push_id(id_source, |ui| {
        ui.horizontal(|ui| {
            let mut enabled = range.is_some();
            if help_beside(ui, hint, |ui| ui.checkbox(&mut enabled, label).changed()) {
                *range = if enabled {
                    Some(stokesforge_import::SubheaderLineRange::single(1))
                } else {
                    None
                };
            }
            if let Some(r) = range {
                ui.label(tr!("import-header-range-from"));
                number_field(
                    ui,
                    "import_range_from",
                    &mut r.from,
                    &quantities::LINE_NUMBER,
                );
                ui.label(tr!("import-header-range-to"));
                number_field(ui, "import_range_to", &mut r.to, &quantities::LINE_NUMBER);
            }
        });
    });
}

/// Color tied to a [`stokesforge_import::LineRole`] in the preview — blue for
/// a recognized header role, magenta for a comment, orange for the footer,
/// default text color for data: same color-coding spirit as OriginLab's ASCII
/// import wizard (§ preview legend), without reusing its exact hues.
fn line_role_color(ui: &egui::Ui, role: stokesforge_import::LineRole) -> egui::Color32 {
    use stokesforge_import::LineRole;
    match role {
        LineRole::MainHeader
        | LineRole::Subheader
        | LineRole::SubheaderLongName
        | LineRole::SubheaderUnit
        | LineRole::ColumnHeaderRow => crate::theme::import_header_line_color(),
        LineRole::SubheaderComment | LineRole::SubheaderXPrime => {
            crate::theme::import_comment_line_color()
        }
        LineRole::Footer => ui.visuals().warn_fg_color,
        LineRole::Comment | LineRole::Skipped => ui.visuals().weak_text_color(),
        LineRole::Data | LineRole::Blank => ui.visuals().text_color(),
    }
}

/// Color and label of each preview code (colored legend).
fn preview_legend(ui: &mut egui::Ui) {
    use stokesforge_import::LineRole;
    ui.horizontal_wrapped(|ui| {
        for (role, label) in [
            (LineRole::MainHeader, tr!("import-legend-header")),
            (LineRole::SubheaderLongName, tr!("column-long-name")),
            (LineRole::SubheaderUnit, tr!("column-units")),
            (LineRole::SubheaderComment, tr!("column-comments")),
            (LineRole::SubheaderXPrime, tr!("column-x-prime")),
            (LineRole::ColumnHeaderRow, tr!("import-column-names")),
            (LineRole::Data, tr!("import-legend-data")),
            (LineRole::Footer, tr!("import-legend-footer")),
            (LineRole::Comment, tr!("import-legend-comment-line")),
            (LineRole::Skipped, tr!("import-legend-skipped")),
        ] {
            let color = line_role_color(ui, role);
            let code = match role {
                LineRole::Data => "·",
                other => other.code(),
            };
            ui.colored_label(color, format!("{code} {label}"));
        }
    });
}

/// The preview of how a file is split, aligned in columns, one row per line —
/// see `stokesforge_import::preview` for what bounds its cost. A right-click
/// on a line sets the import at that place (`ImportLineAction`). A cell that
/// would become a text kept in a mixed column is annotated in the warning
/// color.
fn import_line_preview_widget(
    ui: &mut egui::Ui,
    preview: &stokesforge_import::FilePreview,
    auto_cut: bool,
    chosen: &mut Option<(ImportLineAction, usize)>,
) {
    let shown = preview
        .lines
        .iter()
        .filter(|l| l.role == stokesforge_import::LineRole::Data)
        .count();
    if preview.total_data_lines > shown {
        ui.weak(tr!(
            "import-preview-showing",
            shown = shown,
            total = preview.total_data_lines
        ));
    }
    egui::ScrollArea::both()
        .id_salt("stokesforge_import_preview_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("stokesforge_import_preview_grid")
                .num_columns(2)
                .spacing([8.0, 1.0])
                .show(ui, |ui| {
                    for line in &preview.lines {
                        let color = line_role_color(ui, line.role);
                        let number = ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!(
                                    "{:>5} {:<2}",
                                    line.number,
                                    line.role.code()
                                ))
                                .monospace()
                                .color(color),
                            )
                            .sense(egui::Sense::click()),
                        );
                        number.context_menu(|ui| {
                            let on_data = matches!(
                                line.role,
                                stokesforge_import::LineRole::Data
                                    | stokesforge_import::LineRole::Footer
                            );
                            if ui.button(tr!("import-line-data-start")).clicked() {
                                *chosen = Some((ImportLineAction::DataStartsHere, line.number));
                                ui.close();
                            }
                            if ui.button(tr!("import-line-units")).clicked() {
                                *chosen = Some((ImportLineAction::UnitsLine, line.number));
                                ui.close();
                            }
                            if ui
                                .add_enabled(
                                    on_data,
                                    egui::Button::new(tr!("import-line-data-end")),
                                )
                                .on_disabled_hover_text(tr!("disabled-import-not-data-line"))
                                .clicked()
                            {
                                *chosen = Some((ImportLineAction::DataEndsHere, line.number));
                                ui.close();
                            }
                            if ui
                                .add_enabled(
                                    auto_cut,
                                    egui::Button::new(tr!("import-line-no-auto-end")),
                                )
                                .on_disabled_hover_text(tr!("disabled-import-auto-end-off"))
                                .clicked()
                            {
                                *chosen = Some((ImportLineAction::NoAutoCut, line.number));
                                ui.close();
                            }
                        });
                        match &line.cells {
                            Some(cells) => {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 14.0;
                                    for cell in cells {
                                        let cell_color = if cell.is_text {
                                            ui.visuals().warn_fg_color
                                        } else {
                                            ui.visuals().text_color()
                                        };
                                        ui.add_sized(
                                            [90.0, 14.0],
                                            egui::Label::new(
                                                egui::RichText::new(&cell.text)
                                                    .monospace()
                                                    .color(cell_color),
                                            )
                                            .truncate(),
                                        );
                                    }
                                });
                            }
                            None => {
                                ui.label(egui::RichText::new(&line.text).monospace().color(color));
                            }
                        }
                        ui.end_row();
                    }
                });
        });
}

/// Right pane of the wizard, visible at every step: choice of the file shown,
/// encoding, legend and preview.
fn import_preview_pane(
    ui: &mut egui::Ui,
    state: &mut AppState,
    chosen: &mut Option<(ImportLineAction, usize)>,
) {
    let Some(import) = state.import_state() else {
        return;
    };
    let names: Vec<String> = import
        .paths
        .iter()
        .map(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string()
        })
        .collect();
    let current = import.preview_index.min(names.len().saturating_sub(1));
    let auto_cut = matches!(import.recipe.data_end, stokesforge_import::DataEnd::Auto);
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.label(crate::theme::bold(tr!("import-preview-title")));
        if names.len() > 1 {
            egui::ComboBox::from_id_salt("stokesforge_import_preview_file")
                .selected_text(&names[current])
                .show_ui(ui, |ui| {
                    for (i, name) in names.iter().enumerate() {
                        if ui.selectable_label(i == current, name).clicked() {
                            picked = Some(i);
                        }
                    }
                });
        } else if let Some(name) = names.first() {
            ui.label(name);
        }
        if let Some(encoding) = state.import_encoding() {
            ui.weak(tr!("import-preview-encoding", encoding = encoding));
        }
        if let Some(Ok(preview)) = state.import_line_preview() {
            if preview.block_count > 1 {
                ui.weak(tr!("import-preview-blocks", count = preview.block_count));
            }
        }
    });
    if let Some(i) = picked {
        state.set_import_preview_index(i);
    }
    preview_legend(ui);
    ui.weak(tr!("import-preview-tip"));
    match state.import_line_preview() {
        None => {
            ui.weak(tr!("import-preview-empty"));
        }
        Some(Err(message)) => {
            ui.colored_label(
                ui.visuals().error_fg_color,
                errors::import::read_message(&message),
            );
        }
        Some(Ok(file_preview)) => {
            import_line_preview_widget(ui, &file_preview, auto_cut, chosen);
        }
    }
}

/// What the wizard asks of the application after a frame, once the borrow of
/// the state is released.
#[derive(Default)]
struct ImportWizardRequests {
    save_recipe: bool,
    cancel: bool,
    auto_detect: bool,
    back: bool,
    forward: bool,
    line_action: Option<(ImportLineAction, usize)>,
    import: bool,
    /// File pickers requested (#192), pushed as actions.
    picks: Vec<Action>,
}

fn recipe_combo_text(name: &str, modified: bool) -> String {
    if modified {
        tr!("import-files-template-modified", name = name)
    } else {
        name.to_string()
    }
}

/// Step 1: files and template.
fn import_step_files(ui: &mut egui::Ui, state: &mut AppState, picks: &mut Vec<Action>) {
    // Without a state, the step paints nothing, as before it was split.
    if state.import_state().is_none() {
        return;
    }
    form(ui, "import_files", |rows| {
        import_template_controls(rows, state, picks);
        import_file_choices(rows, state, picks);
        import_file_status_rows(rows.ui(), state);
    });
}

/// Selected template, JSON exchanges and delete confirmation.
fn import_template_controls(rows: &mut Form<'_>, state: &mut AppState, actions: &mut Vec<Action>) {
    let (names, current_name, modified, confirm) = {
        let Some(import) = state.import_state() else {
            return;
        };
        (
            import
                .store
                .recipes()
                .iter()
                .map(|r| r.name.clone())
                .collect::<Vec<_>>(),
            import.recipe.name.clone(),
            state.import_recipe_modified(),
            import.confirm_delete.clone(),
        )
    };
    // Saved templates are data: a dropdown list whatever their number.
    rows.row(tr!("import-files-template"), |ui| {
        egui::ComboBox::from_id_salt("stokesforge_import_recipe_combo")
            .selected_text(recipe_combo_text(&current_name, modified))
            .show_ui(ui, |ui| {
                for name in &names {
                    if ui.selectable_label(*name == current_name, name).clicked() {
                        state.select_import_recipe(name);
                    }
                }
            });
        if ui
            .button(tr!("import-files-reset"))
            .on_hover_text(tr!("import-files-reset", "hint"))
            .clicked()
        {
            state.new_import_recipe();
        }
        // A built-in template cannot be deleted (it would come back at the
        // next launch): save it under another name to modify it.
        let can_delete = names.contains(&current_name)
            && !state
                .import_state()
                .is_some_and(|import| stokesforge_import::RecipeStore::is_builtin(&import.recipe));
        if ui
            .add_enabled(can_delete, egui::Button::new(tr!("import-files-delete")))
            .on_hover_text(tr!("import-files-delete", "hint"))
            .on_disabled_hover_text(tr!("disabled-import-builtin-template"))
            .clicked()
        {
            state.request_delete_import_recipe(&current_name);
        }
    });
    rows.field(|ui| {
        if ui
            .button(tr!("import-files-export"))
            .on_hover_text(tr!("import-files-export", "hint"))
            .clicked()
        {
            actions.push(Action::PickFile(PickRequest::ExportImportRecipe {
                file_name: format!("{current_name}.json"),
            }));
        }
        if ui
            .button(tr!("import-files-import"))
            .on_hover_text(tr!("import-files-import", "hint"))
            .clicked()
        {
            actions.push(Action::PickFile(PickRequest::ImportImportRecipe));
        }
    });
    if modified {
        rows.note(tr!("import-files-unsaved"));
    }
    if let Some(name) = confirm {
        // Ctrl+Z does not bring back a deleted template: modal confirmation
        // (S-F3, #229), painted in the wizard's content to read Enter and
        // Escape before it.
        match modal_confirm(
            rows.ui().ctx(),
            egui::Id::new("stokesforge_import_template_delete"),
            &tr!("import-files-delete-title"),
            PlainSize::Default,
            &tr!("import-files-delete-yes"),
            &tr!("import-files-delete-yes", "hint"),
            |ui| {
                ui.label(tr!("import-files-delete-confirm", name = name));
            },
        ) {
            ConfirmOutcome::Open => {}
            ConfirmOutcome::Confirm => state.confirm_delete_import_recipe(),
            ConfirmOutcome::Cancel => state.cancel_delete_import_recipe(),
        }
    }
}

/// Adding files and choosing among the recently imported files.
fn import_file_choices(rows: &mut Form<'_>, state: &mut AppState, actions: &mut Vec<Action>) {
    rows.section(tr!("import-files-section"));
    rows.ui().horizontal(|ui| {
        if ui
            .button(tr!("import-files-add"))
            .on_hover_text(tr!("import-files-add", "hint"))
            .clicked()
        {
            // The folder of the last imported file.
            let directory = state
                .import_state()
                .and_then(|import| import.recent.last_dir().map(std::path::Path::to_path_buf));
            actions.push(Action::PickFile(PickRequest::AddImportFiles { directory }));
        }
        ui.weak(tr!("import-files-drop"));
    });
    let recent: Vec<std::path::PathBuf> = state
        .import_state()
        .map(|import| import.recent.files().to_vec())
        .unwrap_or_default();
    if !recent.is_empty() {
        let mut picked = None;
        let ui = rows.ui();
        egui::ComboBox::from_id_salt("stokesforge_import_recent")
            .selected_text(tr!("import-files-recent"))
            .show_ui(ui, |ui| {
                for path in &recent {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                    if ui
                        .selectable_label(false, name)
                        .on_hover_text(path.display().to_string())
                        .clicked()
                    {
                        picked = Some(path.clone());
                    }
                }
            });
        if let Some(path) = picked {
            state.add_import_paths(vec![path]);
        }
    }
}

/// Choice of the previewed file and removal, applied after all the rows.
fn import_file_status_rows(ui: &mut egui::Ui, state: &mut AppState) {
    let statuses = state.import_file_statuses();
    let preview_index = state
        .import_state()
        .map_or(0, |import| import.preview_index);
    if statuses.is_empty() {
        ui.weak(tr!("import-files-none"));
    }
    let mut remove = None;
    let mut preview = None;
    for (i, (path, status)) in statuses.iter().enumerate() {
        ui.horizontal(|ui| {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            if ui.radio(i == preview_index, name).clicked() {
                preview = Some(i);
            }
            match status {
                Ok(rows) => {
                    let count = *rows;
                    ui.weak(tr!("import-files-row-count", count = count));
                }
                Err(message) => {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        errors::import::read_message(message),
                    );
                }
            }
            if icon_button(
                ui,
                Icon::Close,
                IconRole::Neutral,
                tr!("import-files-remove"),
            )
            .clicked()
            {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = preview {
        state.set_import_preview_index(i);
    }
    if let Some(i) = remove {
        state.remove_import_path(i);
    }
}

/// Step 2: reading of the columns and the file, grouped by setting.
fn import_step_format(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(import) = state.import_state_mut() else {
        return;
    };
    let recipe = &mut import.recipe;
    form(ui, "import_format", |rows| {
        rows.section(tr!("import-format-delimiter-section"));
        import_delimiter_controls(rows, recipe);
        rows.section(tr!("import-format-numbers-section"));
        import_number_controls(rows, recipe);
        rows.section(tr!("import-format-comments-section"));
        rows.row(tr!("import-format-comment-prefix"), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut recipe.comment_prefix)
                    .desired_width(FIELD_WIDTH_SHORT),
            );
            help_text(ui, tr!("import-format-comment-prefix", "hint"));
        });
        rows.section(tr!("import-format-file-section"));
        import_block_encoding_controls(rows, recipe);
        rows.section(tr!("import-format-skipped-section"));
        rows.row(tr!("import-format-skip-chars"), |ui| {
            number_field(
                ui,
                "import_recipe_skip_chars",
                &mut recipe.skip_chars,
                &quantities::COUNT,
            );
            help_text(ui, tr!("import-format-skip-chars", "hint"));
        });
        rows.row(tr!("import-format-skip-lines"), |ui| {
            number_field(
                ui,
                "import_recipe_skip_data_rows",
                &mut recipe.skip_data_rows,
                &quantities::COUNT,
            );
            help_text(ui, tr!("import-format-skip-lines", "hint"));
        });
    });
}

/// Delimiter and the parameters of its variant (character or fixed widths).
fn import_delimiter_controls(rows: &mut Form<'_>, recipe: &mut stokesforge_import::ImportRecipe) {
    rows.row(tr!("import-format-delimiter"), |ui| {
        let options = [
            Separator::Comma,
            Separator::Semicolon,
            Separator::Tab,
            Separator::Whitespace,
            Separator::Custom('|'),
            Separator::FixedWidth(vec![10]),
            Separator::Auto,
        ]
        .map(|sep| {
            let label = separator_label(&sep);
            ChoiceOption::new(sep, label)
        });
        choice_by(
            ui,
            "stokesforge_import_separator_combo",
            &mut recipe.separator,
            &options,
            same_variant,
        );
        match &mut recipe.separator {
            Separator::Custom(c) => {
                let mut text = c.to_string();
                if ui
                    .add(egui::TextEdit::singleline(&mut text).desired_width(FIELD_WIDTH_SHORT))
                    .changed()
                {
                    if let Some(first) = text.chars().last() {
                        *c = first;
                    }
                }
            }
            Separator::FixedWidth(widths) => {
                let mut text = widths
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                if help_beside(ui, tr!("import-format-widths-hint"), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut text).desired_width(FIELD_WIDTH_MEDIUM))
                        .changed()
                }) {
                    *widths = text
                        .split(',')
                        .filter_map(|part| part.trim().parse::<usize>().ok())
                        .collect();
                }
            }
            _ => {}
        }
        help_text(ui, tr!("import-format-delimiter", "hint"));
    });
}

/// Decimal, thousands and missing values, in their input order.
fn import_number_controls(rows: &mut Form<'_>, recipe: &mut stokesforge_import::ImportRecipe) {
    // Three choices: radio buttons (S-C1).
    rows.row(tr!("import-format-decimal"), |ui| {
        choice(
            ui,
            "stokesforge_import_decimal_combo",
            &mut recipe.decimal,
            &[DecimalStyle::Auto, DecimalStyle::Dot, DecimalStyle::Comma]
                .map(|dec| ChoiceOption::new(dec, decimal_label(dec))),
        )
    });
    rows.row(tr!("import-format-thousands"), |ui| {
        choice(
            ui,
            "stokesforge_import_thousands_combo",
            &mut recipe.thousands,
            &[None, Some(' '), Some(','), Some('.'), Some('\'')]
                .map(|candidate| ChoiceOption::new(candidate, thousands_label(candidate))),
        )
    });
    rows.row(tr!("import-format-date-order"), |ui| {
        use stokesforge_import::DateOrder;
        choice(
            ui,
            "stokesforge_import_date_order_combo",
            &mut recipe.date_order,
            &[
                ChoiceOption::new(DateOrder::DayFirst, tr!("import-date-day-first")),
                ChoiceOption::new(DateOrder::MonthFirst, tr!("import-date-month-first")),
            ],
        )
    });
    rows.row(tr!("import-format-missing"), |ui| {
        let mut text = recipe.missing_tokens.join(", ");
        if help_beside(ui, tr!("import-format-missing", "hint"), |ui| {
            ui.add(egui::TextEdit::singleline(&mut text).desired_width(FIELD_WIDTH_MEDIUM))
                .changed()
        }) {
            recipe.missing_tokens = text
                .split(',')
                .map(|token| token.trim().to_string())
                .filter(|token| !token.is_empty())
                .collect();
        }
    });
}

/// Splitting into blocks and file encoding.
fn import_block_encoding_controls(
    rows: &mut Form<'_>,
    recipe: &mut stokesforge_import::ImportRecipe,
) {
    rows.row(tr!("import-format-blocks"), |ui| {
        use stokesforge_import::BlockSplit;
        // Three splits: radio buttons (S-C1).
        choice_by(
            ui,
            "stokesforge_import_blocks_combo",
            &mut recipe.block_split,
            &[
                ChoiceOption::new(BlockSplit::None, tr!("import-blocks-single")),
                ChoiceOption::new(
                    BlockSplit::BeforeLine("TITLE".into()),
                    tr!("import-blocks-line-start"),
                ),
                ChoiceOption::new(BlockSplit::BlankLines, tr!("import-blocks-blank")),
            ],
            same_variant,
        );
        if let BlockSplit::BeforeLine(marker) = &mut recipe.block_split {
            ui.add(egui::TextEdit::singleline(marker).desired_width(FIELD_WIDTH_SHORT));
        }
        help_text(ui, tr!("import-format-blocks", "hint"));
    });
    rows.row(tr!("import-format-encoding"), |ui| {
        use stokesforge_import::Encoding;
        choice(
            ui,
            "stokesforge_import_encoding_combo",
            &mut recipe.encoding,
            &[
                Encoding::Auto,
                Encoding::Utf8,
                Encoding::Windows1252,
                Encoding::Latin1,
                Encoding::Utf16Le,
                Encoding::Utf16Be,
            ]
            .map(|candidate| ChoiceOption::new(candidate, encoding_label(candidate))),
        )
    });
}

/// The header setting of step 3, derived from the recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderMode {
    None,
    Lines,
    Marker,
}

/// Step 3: header (a single setting), data end and column names, three
/// blocks under their title (S-L3).
fn import_step_header(
    ui: &mut egui::Ui,
    state: &mut AppState,
    requests: &mut ImportWizardRequests,
) {
    let has_files = state
        .import_state()
        .is_some_and(|import| !import.paths.is_empty());
    let Some(import) = state.import_state_mut() else {
        return;
    };
    let recipe = &mut import.recipe;
    form(ui, "import_header", |rows| {
        import_header_controls(rows, recipe, has_files, requests);
        rows.section(tr!("import-dataend"));
        rows.field(|ui| data_end_row(ui, &mut recipe.data_end));
        rows.row(tr!("import-header-skip-footer"), |ui| {
            number_field(
                ui,
                "import_recipe_footer_skip_lines",
                &mut recipe.footer_skip_lines,
                &quantities::COUNT,
            );
            help_text(ui, tr!("import-header-skip-footer", "hint"));
        });
        // Keep the chosen values while greyed out, to find them again when the
        // names no longer come from the first line.
        rows.enabled(!recipe.has_column_header_row, |rows| {
            rows.section(tr!("import-column-names"));
            let mut is_header_key = matches!(recipe.name_source, NameSource::HeaderKey(_));
            if rows.field(|ui| {
                choice(
                    ui,
                    "stokesforge_import_name_source",
                    &mut is_header_key,
                    &[
                        ChoiceOption::new(false, tr!("import-header-name-file")),
                        ChoiceOption::new(true, tr!("import-header-name-key")),
                    ],
                )
            }) {
                recipe.name_source = if is_header_key {
                    NameSource::HeaderKey(String::new())
                } else {
                    NameSource::Filename
                };
            }
            if let NameSource::HeaderKey(key) = &mut recipe.name_source {
                rows.row(tr!("import-header-key"), |ui| {
                    ui.add(egui::TextEdit::singleline(key).desired_width(FIELD_WIDTH_MEDIUM));
                });
            }
        });
    });
}

/// The "Header" block of step 3: mode, lines, subheader roles.
fn import_header_controls(
    rows: &mut Form<'_>,
    recipe: &mut stokesforge_import::ImportRecipe,
    has_files: bool,
    requests: &mut ImportWizardRequests,
) {
    // Like `parse`: the structured split counts only if it has lines (the old
    // `auto_detect` flag alone is not enough).
    let structured =
        recipe.header_layout.main_header_lines > 0 || recipe.header_layout.subheader_lines > 0;
    let is_marker = matches!(recipe.header, HeaderSkip::UntilMarker(_));
    rows.section(tr!("import-header-mode"));
    let mut mode = if is_marker {
        HeaderMode::Marker
    } else if structured {
        HeaderMode::Lines
    } else {
        HeaderMode::None
    };
    if rows.field(|ui| {
        choice(
            ui,
            "stokesforge_import_header_mode",
            &mut mode,
            &[
                ChoiceOption::new(HeaderMode::None, tr!("import-option-none")),
                ChoiceOption::new(HeaderMode::Lines, tr!("import-header-mode-lines")),
                ChoiceOption::new(HeaderMode::Marker, tr!("import-header-mode-marker")),
            ],
        )
    }) {
        match mode {
            HeaderMode::None => {
                recipe.header = HeaderSkip::None;
                recipe.header_layout = stokesforge_import::HeaderLayout::default();
            }
            HeaderMode::Lines => {
                recipe.header = HeaderSkip::None;
                recipe.header_layout.main_header_lines = 1;
            }
            HeaderMode::Marker => {
                recipe.header_layout = stokesforge_import::HeaderLayout::default();
                recipe.header = HeaderSkip::UntilMarker(String::new());
            }
        }
    }
    if let HeaderSkip::UntilMarker(marker) = &mut recipe.header {
        rows.row(tr!("import-header-marker"), |ui| {
            ui.add(egui::TextEdit::singleline(marker).desired_width(FIELD_WIDTH_MEDIUM));
            help_text(ui, tr!("import-header-marker", "hint"));
        });
    }
    if structured {
        rows.row(tr!("import-header-lines"), |ui| {
            number_field(
                ui,
                "import_recipe_header_layout_main_header_lines",
                &mut recipe.header_layout.main_header_lines,
                &quantities::COUNT,
            );
        });
        rows.row(tr!("import-header-subheader-lines"), |ui| {
            number_field(
                ui,
                "import_recipe_header_layout_subheader_lines",
                &mut recipe.header_layout.subheader_lines,
                &quantities::COUNT,
            );
            help_text(ui, tr!("import-header-subheader-lines", "hint"));
        });
    }
    if rows.field(|ui| {
        ui.add_enabled(has_files, egui::Button::new(tr!("import-header-detect")))
            .on_hover_text(tr!("import-header-detect", "hint"))
            .on_disabled_hover_text(tr!("disabled-add-file-first"))
            .clicked()
    }) {
        requests.auto_detect = true;
    }
    if recipe.header_layout.subheader_lines > 0 {
        rows.checkbox(
            &mut recipe.subheader_roles.same_line_name_unit,
            tr!("import-header-combined"),
            Some(tr!("import-header-combined", "hint")),
        );
        rows.field(|ui| {
            subheader_role_row(
                ui,
                "stokesforge_import_role_long_name",
                tr!("import-header-role-long-name"),
                tr!("import-header-role-long-name", "hint"),
                &mut recipe.subheader_roles.long_name,
            )
        });
        if !recipe.subheader_roles.same_line_name_unit {
            rows.field(|ui| {
                subheader_role_row(
                    ui,
                    "stokesforge_import_role_unit",
                    tr!("import-header-role-units"),
                    tr!("import-header-role-units", "hint"),
                    &mut recipe.subheader_roles.unit,
                )
            });
        }
        rows.field(|ui| {
            subheader_role_row(
                ui,
                "stokesforge_import_role_comment",
                tr!("import-header-role-comments"),
                tr!("import-header-role-comments", "hint"),
                &mut recipe.subheader_roles.comment,
            )
        });
        rows.field(|ui| {
            subheader_role_row(
                ui,
                "stokesforge_import_role_x_prime",
                tr!("import-header-role-x-prime"),
                tr!("import-header-role-x-prime", "hint"),
                &mut recipe.subheader_roles.x_prime,
            )
        });
    }
    rows.checkbox(
        &mut recipe.has_column_header_row,
        tr!("import-header-names-first-line"),
        Some(tr!("import-header-names-first-line", "hint")),
    );
}

/// Label of the kind chosen for a column.
fn column_kind_label(kind: stokesforge_import::ColumnKind) -> String {
    match kind {
        stokesforge_import::ColumnKind::Auto => tr!("column-type-numeric"),
        stokesforge_import::ColumnKind::Text => tr!("column-type-text"),
        stokesforge_import::ColumnKind::DateTime => tr!("column-type-datetime"),
    }
}

/// Label of a column role chosen at the "Columns" step.
fn column_role_label(role: stokesforge_import::ColumnRoleChoice) -> String {
    use stokesforge_import::ColumnRoleChoice;
    match role {
        ColumnRoleChoice::Auto => tr!("common-auto"),
        ColumnRoleChoice::X => "X".to_string(),
        ColumnRoleChoice::Y => "Y".to_string(),
        ColumnRoleChoice::XError => tr!("column-role-x-error"),
        ColumnRoleChoice::YError => tr!("column-role-y-error"),
        ColumnRoleChoice::Ignore => tr!("import-option-none"),
    }
}

/// Step 4: the result, column by column, with the role of each.
fn import_step_columns(ui: &mut egui::Ui, state: &mut AppState) {
    use stokesforge_import::ColumnRoleChoice;
    let columns = match state.import_column_previews() {
        None => {
            ui.weak(tr!("import-columns-choose-file"));
            return;
        }
        Some(Err(message)) => {
            ui.colored_label(
                ui.visuals().error_fg_color,
                errors::import::read_message(&message),
            );
            return;
        }
        Some(Ok(columns)) => columns,
    };
    let Some(import) = state.import_state_mut() else {
        return;
    };
    if import.recipe.column_roles.len() < columns.len() {
        import
            .recipe
            .column_roles
            .resize(columns.len(), ColumnRoleChoice::Auto);
    }
    if import.recipe.column_kinds.len() < columns.len() {
        import
            .recipe
            .column_kinds
            .resize(columns.len(), stokesforge_import::ColumnKind::Auto);
    }
    egui::ScrollArea::both()
        .id_salt("stokesforge_import_columns_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("stokesforge_import_columns_grid")
                .striped(true)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    for title in [
                        "#".to_string(),
                        tr!("import-columns-name"),
                        tr!("column-long-name"),
                        tr!("column-units"),
                        tr!("import-columns-read-as"),
                        tr!("import-columns-type"),
                        tr!("column-role"),
                        tr!("import-columns-values"),
                    ] {
                        ui.label(crate::theme::table_header(title));
                    }
                    ui.end_row();
                    for (index, column) in columns.iter().enumerate() {
                        let ignored = import.recipe.column_role(index) == ColumnRoleChoice::Ignore;
                        ui.label((index + 1).to_string());
                        ui.label(&column.name);
                        ui.label(&column.long_name);
                        ui.label(&column.unit);
                        let kind = import.recipe.column_kind(index);
                        egui::ComboBox::from_id_salt(("stokesforge_import_column_kind", index))
                            .selected_text(column_kind_label(kind))
                            .show_ui(ui, |ui| {
                                for choice in [
                                    stokesforge_import::ColumnKind::Auto,
                                    stokesforge_import::ColumnKind::Text,
                                    stokesforge_import::ColumnKind::DateTime,
                                ] {
                                    if ui
                                        .selectable_label(kind == choice, column_kind_label(choice))
                                        .clicked()
                                    {
                                        import.recipe.column_kinds[index] = choice;
                                    }
                                }
                            });
                        ui.label(if column.mixed {
                            tr!("column-type-mixed")
                        } else {
                            tr!("column-type-numeric")
                        });
                        let effective = import.recipe.column_role(index);
                        egui::ComboBox::from_id_salt(("stokesforge_import_column_role", index))
                            .selected_text(column_role_label(effective))
                            .show_ui(ui, |ui| {
                                for choice in [
                                    ColumnRoleChoice::X,
                                    ColumnRoleChoice::Y,
                                    ColumnRoleChoice::XError,
                                    ColumnRoleChoice::YError,
                                    ColumnRoleChoice::Ignore,
                                ] {
                                    if ui
                                        .selectable_label(
                                            effective == choice,
                                            column_role_label(choice),
                                        )
                                        .clicked()
                                    {
                                        import.recipe.column_roles[index] = choice;
                                    }
                                }
                            });
                        let samples = column.samples.join("  ");
                        if ignored {
                            ui.weak(samples);
                        } else {
                            ui.label(samples);
                        }
                        ui.end_row();
                    }
                });
        });
}

/// Step 5: the metadata to keep.
fn import_step_metadata(ui: &mut egui::Ui, state: &mut AppState) {
    let available = state.import_available_metadata();
    let Some(import) = state.import_state_mut() else {
        return;
    };
    let recipe = &mut import.recipe;
    help_beside(ui, tr!("import-metadata-keep-header", "hint"), |ui| {
        ui.checkbox(
            &mut recipe.keep_header_metadata,
            tr!("import-metadata-keep-header"),
        )
    });
    help_beside(ui, tr!("import-metadata-keep-footer", "hint"), |ui| {
        ui.checkbox(
            &mut recipe.keep_footer_metadata,
            tr!("import-metadata-keep-footer"),
        )
    });
    section_title(ui, tr!("import-metadata-lines-section"));
    ui.weak(tr!("import-metadata-uncheck"));
    let is_excluded = |recipe: &stokesforge_import::ImportRecipe, key: &str| {
        recipe
            .excluded_metadata
            .iter()
            .any(|name| name.trim().eq_ignore_ascii_case(key.trim()))
    };
    egui::ScrollArea::vertical()
        .id_salt("stokesforge_import_metadata_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for line in &available {
                if line.key.is_empty() {
                    ui.weak(&line.value);
                    continue;
                }
                let mut kept = !is_excluded(recipe, &line.key);
                let text = if line.is_section() {
                    crate::theme::bold(&line.key)
                } else {
                    egui::RichText::new(format!("{} = {}", line.key, line.value))
                };
                if ui.checkbox(&mut kept, text).changed() {
                    if kept {
                        recipe
                            .excluded_metadata
                            .retain(|name| !name.trim().eq_ignore_ascii_case(line.key.trim()));
                    } else {
                        recipe.excluded_metadata.push(line.key.clone());
                    }
                }
            }
            if available.is_empty() {
                ui.weak(tr!("import-metadata-none"));
            }
        });
}

/// Step 6: destination, saving the template.
fn import_step_destination(
    ui: &mut egui::Ui,
    state: &mut AppState,
    requests: &mut ImportWizardRequests,
) {
    let Some(import) = state.import_state_mut() else {
        return;
    };
    form(ui, "import_destination", |rows| {
        rows.section(tr!("import-destination"));
        // Three destinations, one of which may be missing (no active
        // workbook): radio buttons one under the other, placed here to grey
        // out the first.
        rows.field(|ui| {
            ui.vertical(|ui| {
                if let Some(book) = import.active_book {
                    ui.radio_value(
                        &mut import.destination,
                        ImportDestination::ActiveBook(book),
                        tr!("import-destination-active"),
                    );
                } else {
                    ui.add_enabled(
                        false,
                        egui::RadioButton::new(false, tr!("import-destination-active-none")),
                    )
                    .on_disabled_hover_text(tr!("disabled-import-no-active-book"));
                }
                ui.radio_value(
                    &mut import.destination,
                    ImportDestination::NewBookPerFile,
                    tr!("import-destination-new-per-file"),
                );
                ui.radio_value(
                    &mut import.destination,
                    ImportDestination::NewBookForAll,
                    tr!("import-destination-new-all"),
                );
            });
        });
        if matches!(import.destination, ImportDestination::ActiveBook(_)) {
            rows.section(tr!("import-destination-mode"));
            use stokesforge_import::ImportMode;
            // Four modes: a dropdown list (S-C1).
            rows.field(|ui| {
                choice(
                    ui,
                    "stokesforge_import_mode",
                    &mut import.recipe.import_mode,
                    &[
                        ChoiceOption::new(ImportMode::NewSheet, tr!("import-mode-new-sheet")),
                        ChoiceOption::new(ImportMode::ReplaceSheet, tr!("import-mode-replace")),
                        ChoiceOption::new(ImportMode::AddColumns, tr!("import-mode-add-columns")),
                        ChoiceOption::new(ImportMode::AddRows, tr!("import-mode-add-rows")),
                    ],
                )
            });
        }
        if import.paths.len() > 1 {
            rows.section(tr!("import-destination-multiple"));
            use stokesforge_import::FilesLayout;
            rows.field(|ui| {
                choice(
                    ui,
                    "stokesforge_import_files_layout",
                    &mut import.recipe.files_layout,
                    &[
                        ChoiceOption::new(FilesLayout::SheetPerFile, tr!("import-layout-per-file")),
                        ChoiceOption::new(
                            FilesLayout::SideBySide,
                            tr!("import-layout-side-by-side"),
                        ),
                        ChoiceOption::new(FilesLayout::SharedX, tr!("import-layout-shared-x")),
                    ],
                )
            });
        }
        rows.section(tr!("import-destination-template-section"));
        rows.row(tr!("import-destination-template-name"), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut import.save_as_name)
                    .desired_width(FIELD_WIDTH_MEDIUM),
            );
            if ui.button(tr!("import-destination-save")).clicked() {
                requests.save_recipe = true;
            }
        });
        rows.note(tr!("import-destination-saved-note"));
        rows.section(tr!("import-destination-auto"))
            .on_hover_text(tr!("import-destination-auto", "hint"));
        let auto = &mut import.recipe.auto_match;
        rows.row(tr!("import-auto-extensions"), |ui| {
            let mut text = auto.extensions.join(", ");
            if help_beside(ui, tr!("import-auto-extensions", "hint"), |ui| {
                ui.add(egui::TextEdit::singleline(&mut text).desired_width(FIELD_WIDTH_MEDIUM))
                    .changed()
            }) {
                auto.extensions = text
                    .split(',')
                    .map(|e| e.trim().trim_start_matches('.').to_string())
                    .filter(|e| !e.is_empty())
                    .collect();
            }
        });
        rows.row(tr!("import-auto-name"), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut auto.name_contains)
                    .desired_width(FIELD_WIDTH_MEDIUM),
            );
        });
        rows.row(tr!("import-auto-content"), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut auto.content_contains)
                    .desired_width(FIELD_WIDTH_MEDIUM),
            );
            help_text(ui, tr!("import-auto-content", "hint"));
        });
    });
}

/// The import wizard ("Import…", File menu) in six steps in the style of
/// Origin's ASCII wizard: files and template, format, header and data end,
/// columns, metadata, destination. The file preview stays visible on the right
/// at every step. "Finish" imports as soon as files are chosen, with the
/// template as it is — import in two clicks for a known format. **Modal
/// dialog of the common building block** ([`modal_dialog_with`], like the
/// graph properties), painted from [`StokesForgeApp::ui`](eframe::App::ui)
/// as long as [`AppState::import_state`] is `Some`; the close cross and Escape
/// mean "Cancel", Enter "Finish". Wizard footer ([`modal_wizard`], S-F6):
/// Cancel on the left, Back · Next · Finish on the right.
pub(super) fn import_window(ctx: &egui::Context, state: &mut AppState, actions: &mut Vec<Action>) {
    let Some(step) = state.import_state().map(|import| import.step) else {
        return;
    };
    let has_files = state
        .import_state()
        .is_some_and(|import| !import.paths.is_empty());
    // The two closures of the building block (content, footer buttons) write
    // into the same requests, one after the other: `RefCell`.
    let requests = std::cell::RefCell::new(ImportWizardRequests::default());
    // When greyed out, Finish says why (S-G1, S-F10): in the footer and in its
    // bubble (`DialogFooter::gated`).
    let (finish_label, finish_hint) = (
        tr!("import-button-finish"),
        tr!("import-button-finish", "hint"),
    );
    let add_file_first = tr!("disabled-add-file-first");
    let outcome = modal_wizard(
        ctx,
        egui::Id::new("stokesforge_import_wizard"),
        &tr!("import-window-title"),
        // FREE-SIZE(#300): six steps with a file preview on the right at every step; the preview table needs more than 600 px of width, beyond the Large class
        InteractiveSize::Free(egui::vec2(1000.0, 620.0)),
        DialogFooter::gated(&finish_label, &finish_hint, has_files, &add_file_first),
        |footer| {
            // Added right to left, against Finish: "Next" then "Back" (S-F6:
            // Back · Next · Finish grouped on the right).
            if footer
                .add_enabled(
                    step != ImportStep::Destination,
                    egui::Button::new(tr!("import-button-next")).min_size(FOOTER_BUTTON_SIZE),
                )
                .on_disabled_hover_text(tr!("disabled-import-last-step"))
                .clicked()
            {
                requests.borrow_mut().forward = true;
            }
            if footer
                .add_enabled(
                    step != ImportStep::Files,
                    egui::Button::new(tr!("import-button-back")).min_size(FOOTER_BUTTON_SIZE),
                )
                .on_disabled_hover_text(tr!("disabled-import-first-step"))
                .clicked()
            {
                requests.borrow_mut().back = true;
            }
        },
        |ui| {
            let mut requests = requests.borrow_mut();
            ui.horizontal_wrapped(|ui| {
                for (i, candidate) in ImportStep::ALL.iter().enumerate() {
                    if i > 0 {
                        // Decorative separator of the step trail: a dimmed
                        // icon, no gesture so no tooltip.
                        ui.add(
                            icon_image(Icon::CaretRight, IconRole::Neutral)
                                .tint(crate::theme::help_icon_tint()),
                        );
                    }
                    let text = format!("{} {}", i + 1, candidate.title());
                    if *candidate == step {
                        // Like the active tab: bold and accent.
                        ui.label(crate::theme::bold(text).color(crate::theme::accent()));
                    } else {
                        ui.weak(text);
                    }
                }
            });
            ui.separator();
            let body_height = ui.available_height().max(240.0);
            ui.columns(2, |columns| {
                columns[0].set_min_height(body_height);
                columns[0].set_max_height(body_height);
                egui::ScrollArea::vertical()
                    .id_salt("stokesforge_import_step_scroll")
                    .max_height(body_height)
                    .auto_shrink([false, false])
                    .show(&mut columns[0], |ui| match step {
                        ImportStep::Files => import_step_files(ui, state, &mut requests.picks),
                        ImportStep::Format => import_step_format(ui, state),
                        ImportStep::Header => import_step_header(ui, state, &mut requests),
                        ImportStep::Columns => import_step_columns(ui, state),
                        ImportStep::Metadata => import_step_metadata(ui, state),
                        ImportStep::Destination => {
                            import_step_destination(ui, state, &mut requests)
                        }
                    });
                columns[1].set_min_height(body_height);
                columns[1].set_max_height(body_height);
                import_preview_pane(&mut columns[1], state, &mut requests.line_action);
            });
        },
    );
    // Previews are recomputed after a pause in typing (S-T1, #224).
    let now = ctx.input(|input| input.time);
    if let Some(wait) = state
        .import_state_mut()
        .and_then(|import| import.debounce_preview(ctx.text_edit_focused(), now))
    {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait + 0.01));
    }
    let mut requests = requests.into_inner();
    match outcome {
        DialogOutcome::Open => {}
        DialogOutcome::Accept => requests.import = true,
        DialogOutcome::Cancel => requests.cancel = true,
    }

    // Everything that needs the whole state is applied here, once the window
    // is painted.
    actions.append(&mut requests.picks);
    if requests.save_recipe {
        state.save_import_recipe();
    }
    if requests.auto_detect {
        state.auto_detect_import_header();
    }
    if let Some((action, number)) = requests.line_action {
        state.import_line_action(action, number);
    }
    if requests.back {
        state.import_step_back();
    }
    if requests.forward {
        state.import_step_forward();
    }
    if requests.import {
        if let Some(import) = state.import_state() {
            actions.push(Action::StartImport {
                destination: import.destination,
                recipe: import.recipe.clone(),
                paths: import.paths.clone(),
            });
        }
        state.cancel_import();
    } else if requests.cancel {
        state.cancel_import();
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Mutex, MutexGuard};

    use eframe::egui;
    use stokesforge_core::Project;
    use stokesforge_import::{
        ColumnKind, ColumnRoleChoice, DataEnd, DecimalStyle, Encoding, FilePreview, HeaderSkip,
        LineRole, PreviewCell, PreviewLine, Separator,
    };
    use stokesforge_session::Session;

    use super::{
        column_kind_label, column_role_label, data_end_label, data_end_row, decimal_label,
        encoding_label, import_line_preview_widget, import_window, line_role_color,
        recipe_combo_text, separator_label, thousands_label,
    };
    use stokesforge_app_state::state::{
        Action, AppState, ImportDestination, ImportLineAction, ImportStep,
    };
    use stokesforge_app_state::tr;

    use super::super::test_support::{
        click, context, distinct, image, key, text_exact, texts, Text,
    };

    // -- Tooling ----------------------------------------------------------

    /// Serializes the tests of this module that open the wizard: the config
    /// folder is *process* state, and `cargo test` runs the tests in parallel
    /// (same reason as `tests/shell.rs::isolate_recipe_store`).
    static CONFIG_ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Redirects the config folder (`STOKESFORGE_CONFIG_DIR`, #332, and
    /// `XDG_CONFIG_HOME`) to a temporary folder as long as the value lives:
    /// `AppState::open_import` reads the machine's templates and recent files
    /// (`RecipeStore`, `RecentFiles`), which would make the test depend on the
    /// workstation. `Drop` restores the earlier values, even on panic.
    struct IsolatedConfig {
        before: [(&'static str, Option<std::ffi::OsString>); 2],
        _directory: tempfile::TempDir,
        _guard: MutexGuard<'static, ()>,
    }

    impl Drop for IsolatedConfig {
        fn drop(&mut self) {
            for (name, value) in self.before.iter_mut() {
                match value.take() {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }

    fn isolate_the_config() -> IsolatedConfig {
        let guard = CONFIG_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let directory = tempfile::tempdir().unwrap();
        let names = [
            stokesforge_import::config_dir::CONFIG_DIR_VARIABLE,
            "XDG_CONFIG_HOME",
        ];
        let before = names.map(|name| (name, std::env::var_os(name)));
        for name in names {
            std::env::set_var(name, directory.path());
        }
        IsolatedConfig {
            before,
            _directory: directory,
            _guard: guard,
        }
    }

    /// A Jasco file: a header up to the `XYDATA` marker, three data lines with
    /// two columns, a footer.
    const JASCO: &str =
        "TITLE;S13\nXUNITS;NANOMETERS\nXYDATA\n800;1\n799;2\n798;3\n\nData points;3\n";

    /// The wizard open on an empty project, with no active workbook, with
    /// `files` already chosen; the recipe reads `JASCO` (semicolon, header up
    /// to `XYDATA`). The fields keep alive what the state depends on.
    struct Bench {
        state: AppState,
        files: Vec<PathBuf>,
        _session: Session,
        _directory: tempfile::TempDir,
        _config: IsolatedConfig,
    }

    fn bench(with_file: bool) -> Bench {
        let config = isolate_the_config();
        let session = Session::spawn(Project::new("Import").unwrap(), None).unwrap();
        let mut state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let mut files = Vec::new();
        if with_file {
            let file = directory.path().join("jasco.csv");
            std::fs::write(&file, JASCO).unwrap();
            files.push(file);
        }
        state.open_import(None::<stokesforge_core::NodeId>);
        state.set_import_paths(files.clone());
        let import = state.import_state_mut().unwrap();
        import.recipe.separator = Separator::Semicolon;
        import.recipe.header = HeaderSkip::UntilMarker("XYDATA".into());
        Bench {
            state,
            files,
            _session: session,
            _directory: directory,
            _config: config,
        }
    }

    fn is_painted(texts: &[Text], needle: &str) -> bool {
        texts.iter().any(|t| t.text == needle)
    }

    /// Paints the wizard three times (layout settled, dialog centered) then a
    /// fourth time with `events`; returns the texts of the last frame.
    fn paint_wizard(
        ctx: &egui::Context,
        state: &mut AppState,
        actions: &mut Vec<Action>,
        events: Vec<egui::Event>,
    ) -> Vec<Text> {
        for _ in 0..3 {
            image(ctx, Vec::new(), |ui| {
                import_window(ui.ctx(), state, actions)
            });
        }
        texts(&image(ctx, events, |ui| {
            import_window(ui.ctx(), state, actions)
        }))
    }

    // -- The whole wizard ---------------------------------------------------

    /// The step trail separates its steps with an icon, no longer with a text
    /// chevron (#170, point 8). Witness: the trail is painted (each step
    /// carries its rank), only the chevron is missing.
    #[test]
    fn the_step_trail_has_no_text_chevron_any_more() {
        let mut bench = bench(true);
        let ctx = context();
        let mut actions = Vec::new();
        let painted = paint_wizard(&ctx, &mut bench.state, &mut actions, Vec::new());
        for (i, wizard_step) in ImportStep::ALL.iter().enumerate() {
            let expected = format!("{} {}", i + 1, wizard_step.title());
            assert!(
                is_painted(&painted, &expected),
                "step not painted: {expected}"
            );
        }
        assert!(
            !painted.iter().any(|t| t.text.contains('\u{203A}')),
            "a text chevron is still painted: {painted:?}"
        );
    }

    #[test]
    fn column_names_are_inactive_with_names_on_the_first_line() {
        let mut bench = bench(true);
        bench.state.import_state_mut().unwrap().step = ImportStep::Header;
        bench
            .state
            .import_state_mut()
            .unwrap()
            .recipe
            .has_column_header_row = true;
        let ctx = context();
        let mut actions = Vec::new();
        let painted = paint_wizard(&ctx, &mut bench.state, &mut actions, Vec::new());
        let key_pos = text_exact(&painted, &tr!("import-header-name-key")).center;
        click(&ctx, key_pos, egui::PointerButton::Primary, |ui| {
            import_window(ui.ctx(), &mut bench.state, &mut actions)
        });
        assert_eq!(
            bench.state.import_state().unwrap().recipe.name_source,
            stokesforge_import::NameSource::Filename,
            "the disabled section must ignore clicks"
        );

        let checkbox = text_exact(&painted, &tr!("import-header-names-first-line")).center;
        click(&ctx, checkbox, egui::PointerButton::Primary, |ui| {
            import_window(ui.ctx(), &mut bench.state, &mut actions)
        });
        assert!(
            !bench
                .state
                .import_state()
                .unwrap()
                .recipe
                .has_column_header_row
        );
        let enabled_texts = paint_wizard(&ctx, &mut bench.state, &mut actions, Vec::new());
        assert_ne!(
            text_exact(&painted, &tr!("import-column-names")).color,
            text_exact(&enabled_texts, &tr!("import-column-names")).color,
            "the title must reflect the greying out"
        );
        let key_pos = text_exact(&enabled_texts, &tr!("import-header-name-key")).center;
        click(&ctx, key_pos, egui::PointerButton::Primary, |ui| {
            import_window(ui.ctx(), &mut bench.state, &mut actions)
        });
        assert_eq!(
            bench.state.import_state().unwrap().recipe.name_source,
            stokesforge_import::NameSource::HeaderKey(String::new()),
            "unchecking re-enables the section"
        );
        bench.state.import_state_mut().unwrap().recipe.name_source =
            stokesforge_import::NameSource::HeaderKey("TITLE".into());
        let enabled_texts = paint_wizard(&ctx, &mut bench.state, &mut actions, Vec::new());
        let checkbox = text_exact(&enabled_texts, &tr!("import-header-names-first-line")).center;
        click(&ctx, checkbox, egui::PointerButton::Primary, |ui| {
            import_window(ui.ctx(), &mut bench.state, &mut actions)
        });
        let greyed = paint_wizard(&ctx, &mut bench.state, &mut actions, Vec::new());
        let field = text_exact(&greyed, "TITLE").center;
        click(&ctx, field, egui::PointerButton::Primary, |ui| {
            import_window(ui.ctx(), &mut bench.state, &mut actions)
        });
        paint_wizard(
            &ctx,
            &mut bench.state,
            &mut actions,
            vec![egui::Event::Text("modification".into())],
        );
        let file = text_exact(&greyed, &tr!("import-header-name-file")).center;
        click(&ctx, file, egui::PointerButton::Primary, |ui| {
            import_window(ui.ctx(), &mut bench.state, &mut actions)
        });
        assert_eq!(
            bench.state.import_state().unwrap().recipe.name_source,
            stokesforge_import::NameSource::HeaderKey("TITLE".into()),
            "the field and the two choices stay inactive, without losing the key"
        );
    }

    /// The Format groups are named by real titles, in the order of the
    /// controls, rather than separated by unlabeled lines.
    #[test]
    fn the_import_format_names_the_setting_groups() {
        let mut bench = bench(false);
        let ctx = context();
        let recipe_before = bench.state.import_state().unwrap().recipe.clone();
        for _ in 0..2 {
            image(&ctx, Vec::new(), |ui| {
                super::import_step_format(ui, &mut bench.state)
            });
        }
        let shapes = image(&ctx, Vec::new(), |ui| {
            super::import_step_format(ui, &mut bench.state)
        });
        let painted = texts(&shapes);
        let groups = [
            (
                tr!("import-format-delimiter-section"),
                tr!("import-format-delimiter"),
            ),
            (
                tr!("import-format-numbers-section"),
                tr!("import-format-decimal"),
            ),
            (
                tr!("import-format-comments-section"),
                tr!("import-format-comment-prefix"),
            ),
            (
                tr!("import-format-file-section"),
                tr!("import-format-blocks"),
            ),
            (
                tr!("import-format-skipped-section"),
                tr!("import-format-skip-chars"),
            ),
        ];
        let mut previous = f32::NEG_INFINITY;
        for (title, field) in groups {
            let title = painted
                .iter()
                .find(|text| text.text == title && text.bold)
                .unwrap_or_else(|| panic!("section title missing: {title}"));
            let field = painted
                .iter()
                .find(|text| text.text == field && !text.bold)
                .unwrap_or_else(|| panic!("ordinary label missing: {field}"));
            assert!(title.center.y > previous);
            assert!(field.center.y > title.center.y);
            previous = field.center.y;
        }
        assert_eq!(bench.state.import_state().unwrap().recipe, recipe_before);
    }

    /// Issue #6: each step of the wizard paints without panicking, shows its
    /// title highlighted (the others dimmed), **its** content and not that of
    /// the other steps, and the file preview on the right, whatever the step.
    /// Painting imports nothing and does not close the wizard.
    #[test]
    fn each_step_of_the_import_wizard_paints_with_its_preview() {
        let mut bench = bench(true);
        let ctx = context();
        let visuals = ctx.global_style().visuals.clone();
        let (strong, weak) = (visuals.strong_text_color(), visuals.weak_text_color());
        assert_ne!(strong, weak, "witness: highlight and dimming differ");
        // A label specific to each step.
        let own_labels = [
            (ImportStep::Files, tr!("import-files-template")),
            (ImportStep::Format, tr!("import-format-delimiter")),
            (ImportStep::Header, tr!("import-dataend")),
            (ImportStep::Columns, tr!("import-columns-read-as")),
            (ImportStep::Metadata, tr!("import-metadata-keep-header")),
            (ImportStep::Destination, tr!("import-destination")),
        ];
        assert_eq!(own_labels.len(), ImportStep::ALL.len());
        for step in ImportStep::ALL {
            assert_eq!(bench.state.import_state().unwrap().step, step);
            let columns_before = bench
                .state
                .import_state()
                .unwrap()
                .recipe
                .column_roles
                .len();
            let mut actions = Vec::new();
            let painted = paint_wizard(&ctx, &mut bench.state, &mut actions, Vec::new());
            for (i, candidate) in ImportStep::ALL.iter().enumerate() {
                let trail = text_exact(&painted, &format!("{} {}", i + 1, candidate.title()));
                let expected = if *candidate == step { strong } else { weak };
                assert_eq!(
                    trail.color, expected,
                    "{step:?}: breadcrumb of {candidate:?}"
                );
                assert_eq!(
                    trail.bold,
                    *candidate == step,
                    "{step:?}: only the current step is bold ({candidate:?})"
                );
            }
            for (owner, label) in &own_labels {
                assert_eq!(
                    is_painted(&painted, label),
                    *owner == step,
                    "{step:?}: {label:?} belongs to {owner:?}"
                );
            }
            // The preview: the raw header line and a data cell.
            assert!(
                painted.iter().any(|t| t.text.contains("TITLE;S13")),
                "{step:?}: preview missing"
            );
            assert!(is_painted(&painted, "800"), "{step:?}: cell missing");
            if step == ImportStep::Columns {
                // A role and a kind per column read: the dropdown list of
                // column 2 writes `column_roles[1]`.
                assert!(
                    columns_before < 2,
                    "witness: the recipe starts without roles"
                );
                let recipe = &bench.state.import_state().unwrap().recipe;
                assert_eq!(recipe.column_roles.len(), 2);
                assert_eq!(recipe.column_kinds.len(), 2);
            }
            assert!(actions.is_empty(), "painting imports nothing");
            bench.state.import_step_forward();
        }
        assert!(
            bench.state.import_state().is_some(),
            "the wizard stays open"
        );
    }

    /// "Finish" (Enter, default button) imports as soon as files are chosen,
    /// with the recipe as it is — import in two clicks — and closes the
    /// wizard; without a file, Enter does nothing (witness). Escape means
    /// "Cancel": closed, nothing imported, even with files.
    #[test]
    fn finish_imports_the_recipe_as_it_is_and_cancel_imports_nothing() {
        let mut complete = bench(true);
        let recipe = complete.state.import_state().unwrap().recipe.clone();
        assert_eq!(recipe.separator, Separator::Semicolon);
        let mut actions = Vec::new();
        paint_wizard(
            &context(),
            &mut complete.state,
            &mut actions,
            vec![key(egui::Key::Enter)],
        );
        assert_eq!(
            actions,
            vec![Action::StartImport {
                destination: ImportDestination::NewBookForAll,
                recipe,
                paths: complete.files.clone(),
            }]
        );
        assert!(complete.state.import_state().is_none(), "Finish closes");
        drop(complete);

        let mut empty = bench(false);
        let ctx = context();
        let mut actions = Vec::new();
        paint_wizard(
            &ctx,
            &mut empty.state,
            &mut actions,
            vec![key(egui::Key::Enter)],
        );
        assert!(actions.is_empty(), "without a file, nothing to import");
        assert!(empty.state.import_state().is_some(), "nor closed");
        drop(empty);

        let mut cancelled = bench(true);
        let mut actions = Vec::new();
        paint_wizard(
            &context(),
            &mut cancelled.state,
            &mut actions,
            vec![key(egui::Key::Escape)],
        );
        assert!(actions.is_empty(), "Cancel imports nothing");
        assert!(cancelled.state.import_state().is_none(), "Cancel closes");
    }

    /// The footer buttons: "Next" moves forward one step, "Back" moves back —
    /// not the reverse.
    #[test]
    fn next_and_back_change_the_step() {
        let mut bench = bench(true);
        let ctx = context();
        let mut actions = Vec::new();
        for (button, expected) in [
            (tr!("import-button-next"), ImportStep::Format),
            (tr!("import-button-next"), ImportStep::Header),
            (tr!("import-button-back"), ImportStep::Format),
        ] {
            let painted = paint_wizard(&ctx, &mut bench.state, &mut actions, Vec::new());
            let pos = text_exact(&painted, &button).center;
            click(&ctx, pos, egui::PointerButton::Primary, |ui| {
                import_window(ui.ctx(), &mut bench.state, &mut actions)
            });
            assert_eq!(
                bench.state.import_state().unwrap().step,
                expected,
                "{button}"
            );
        }
        assert!(actions.is_empty());
    }

    /// A right-click in the wizard's preview reaches the recipe, once the
    /// frame is painted: "Start Data Here" on line 4 (`800;1`) replaces the
    /// `XYDATA` marker with three header lines.
    #[test]
    fn right_click_in_the_wizard_preview_sets_the_recipe() {
        let mut bench = bench(true);
        let ctx = context();
        let mut actions = Vec::new();
        let painted = paint_wizard(&ctx, &mut bench.state, &mut actions, Vec::new());
        let line = line_number(&painted, 4).center;
        click(&ctx, line, egui::PointerButton::Secondary, |ui| {
            import_window(ui.ctx(), &mut bench.state, &mut actions)
        });
        let painted = texts(&image(&ctx, Vec::new(), |ui| {
            import_window(ui.ctx(), &mut bench.state, &mut actions)
        }));
        let entry = text_exact(&painted, &tr!("import-line-data-start")).center;
        click(&ctx, entry, egui::PointerButton::Primary, |ui| {
            import_window(ui.ctx(), &mut bench.state, &mut actions)
        });
        let recipe = &bench.state.import_state().unwrap().recipe;
        assert_eq!(recipe.header, HeaderSkip::None);
        assert_eq!(recipe.header_layout.main_header_lines, 3);
        assert!(actions.is_empty());
    }

    // -- The labels of the choices ------------------------------------------

    /// Each dropdown list of the wizard shows choices that can be told apart;
    /// the label follows the variant, not the value it carries (an "Other"
    /// separator stays "Other" whatever its character).
    #[test]
    fn the_choices_of_one_list_have_distinct_labels() {
        distinct(
            &[
                Separator::Comma,
                Separator::Semicolon,
                Separator::Tab,
                Separator::Whitespace,
                Separator::Custom('|'),
                Separator::FixedWidth(vec![10]),
                Separator::Auto,
            ],
            separator_label,
        );
        assert_eq!(
            separator_label(&Separator::Custom('|')),
            separator_label(&Separator::Custom('!'))
        );
        assert_eq!(
            separator_label(&Separator::FixedWidth(vec![10])),
            separator_label(&Separator::FixedWidth(vec![4, 8]))
        );
        distinct(
            &[DecimalStyle::Auto, DecimalStyle::Dot, DecimalStyle::Comma],
            |d| decimal_label(*d),
        );
        // Thousands: the five choices offered, plus a character read from a
        // template, which shows as "Other" without being confused with them.
        distinct(
            &[None, Some(' '), Some(','), Some('.'), Some('\''), Some('_')],
            |t| thousands_label(*t),
        );
        distinct(
            &[
                Encoding::Auto,
                Encoding::Utf8,
                Encoding::Windows1252,
                Encoding::Latin1,
                Encoding::Utf16Le,
                Encoding::Utf16Be,
            ],
            |e| encoding_label(*e),
        );
        distinct(
            &[ColumnKind::Auto, ColumnKind::Text, ColumnKind::DateTime],
            |k| column_kind_label(*k),
        );
        distinct(
            &[
                ColumnRoleChoice::Auto,
                ColumnRoleChoice::X,
                ColumnRoleChoice::Y,
                ColumnRoleChoice::XError,
                ColumnRoleChoice::YError,
                ColumnRoleChoice::Ignore,
            ],
            |r| column_role_label(*r),
        );
        distinct(
            &[
                DataEnd::Auto,
                DataEnd::None,
                DataEnd::FirstBlankLine,
                DataEnd::Marker(String::new()),
                DataEnd::LinesAfterHeader(100),
                DataEnd::LinesBeforeEnd(1),
            ],
            data_end_label,
        );
        assert_eq!(
            data_end_label(&DataEnd::Marker(String::new())),
            data_end_label(&DataEnd::Marker("END".into()))
        );
        assert_eq!(
            data_end_label(&DataEnd::LinesAfterHeader(100)),
            data_end_label(&DataEnd::LinesAfterHeader(3))
        );
    }

    /// One notion, one word (`docs/textes-interface.md`): the comma, the dot,
    /// "Auto" and "None" are written the same from one list of the wizard to
    /// another.
    #[test]
    fn one_notion_carries_the_same_label_from_one_list_to_another() {
        let comma = separator_label(&Separator::Comma);
        assert_eq!(decimal_label(DecimalStyle::Comma), comma);
        assert_eq!(thousands_label(Some(',')), comma);
        assert_eq!(thousands_label(Some('.')), decimal_label(DecimalStyle::Dot));
        let auto = separator_label(&Separator::Auto);
        assert_eq!(decimal_label(DecimalStyle::Auto), auto);
        assert_eq!(encoding_label(Encoding::Auto), auto);
        assert_eq!(column_role_label(ColumnRoleChoice::Auto), auto);
        let none_label = thousands_label(None);
        assert_eq!(column_role_label(ColumnRoleChoice::Ignore), none_label);
        assert_eq!(data_end_label(&DataEnd::None), none_label);
        // A custom separator is an "Other", like an unknown thousands
        // separator.
        assert_eq!(
            separator_label(&Separator::Custom('|')),
            thousands_label(Some('_'))
        );
    }

    /// The template list says a recipe was modified without being saved,
    /// without losing the template name; unmodified, the name alone.
    #[test]
    fn the_modified_template_is_flagged_in_the_template_list() {
        assert_eq!(recipe_combo_text("Jasco", false), "Jasco");
        let modified = recipe_combo_text("Jasco", true);
        assert_ne!(modified, "Jasco");
        assert!(modified.contains("Jasco"), "{modified:?}");
    }

    // -- The data end ------------------------------------------------------

    /// Opens the "Data end" list then clicks the choice `libelle`.
    fn choose_end(ctx: &egui::Context, end: &mut DataEnd, label: &str) {
        let painted = texts(&image(ctx, Vec::new(), |ui| data_end_row(ui, end)));
        let list = text_exact(&painted, &data_end_label(end)).center;
        click(ctx, list, egui::PointerButton::Primary, |ui| {
            data_end_row(ui, end)
        });
        let painted = texts(&image(ctx, Vec::new(), |ui| data_end_row(ui, end)));
        // The open list: the wanted choice, somewhere other than the closed button.
        let choice = painted
            .iter()
            .find(|t| t.text == label && t.center != list)
            .unwrap_or_else(|| panic!("{label:?} missing from the open list"))
            .center;
        click(ctx, choice, egui::PointerButton::Primary, |ui| {
            data_end_row(ui, end)
        });
    }

    /// Issue #4: the data end shows the field of the chosen option (marker,
    /// number of lines); choosing the same option again keeps the typed text,
    /// changing it takes the new option's default value (witness: the click
    /// does reach the list).
    #[test]
    fn the_data_end_keeps_its_value_until_the_option_changes() {
        let ctx = context();
        let mut end = DataEnd::Marker("END".into());
        let painted = texts(&image(&ctx, Vec::new(), |ui| data_end_row(ui, &mut end)));
        assert!(is_painted(&painted, "END"), "the marker is in its field");

        choose_end(&ctx, &mut end, &tr!("import-dataend-marker"));
        assert_eq!(end, DataEnd::Marker("END".into()), "same option: text kept");

        choose_end(&ctx, &mut end, &tr!("import-dataend-after-header"));
        assert_eq!(end, DataEnd::LinesAfterHeader(100), "other option");
        let painted = texts(&image(&ctx, Vec::new(), |ui| data_end_row(ui, &mut end)));
        assert!(is_painted(&painted, "100"), "the number is in its field");
        assert!(!is_painted(&painted, "END"));

        choose_end(&ctx, &mut end, &tr!("import-dataend-auto"));
        assert_eq!(end, DataEnd::Auto);
        let painted = texts(&image(&ctx, Vec::new(), |ui| data_end_row(ui, &mut end)));
        assert!(!is_painted(&painted, "100"), "Auto has no field");
    }

    // -- The preview -------------------------------------------------------

    /// A preview of three lines: a header, a data line with two cells of which
    /// the second would be kept as text, a footer.
    fn preview_of(total_data_lines: usize) -> FilePreview {
        FilePreview {
            lines: vec![
                PreviewLine {
                    number: 1,
                    role: LineRole::MainHeader,
                    text: "TITLE".into(),
                    cells: None,
                },
                PreviewLine {
                    number: 2,
                    role: LineRole::Data,
                    text: "1.5;abc".into(),
                    cells: Some(vec![
                        PreviewCell {
                            text: "1.5".into(),
                            is_text: false,
                        },
                        PreviewCell {
                            text: "abc".into(),
                            is_text: true,
                        },
                    ]),
                },
                PreviewLine {
                    number: 3,
                    role: LineRole::Footer,
                    text: "END".into(),
                    cells: None,
                },
            ],
            total_data_lines,
            block_count: 1,
        }
    }

    /// The number of line `numero` in the preview: right-aligned on five
    /// characters, then its role code (not "4 Columns", the breadcrumb step).
    fn line_number(painted: &[Text], line_number: usize) -> &Text {
        let line_number = line_number.to_string();
        painted
            .iter()
            .find(|t| t.text.get(..5).map(str::trim_start) == Some(line_number.as_str()))
            .unwrap_or_else(|| panic!("line {line_number} missing"))
    }

    /// Right-click on line `ligne` of the preview, then click on the menu
    /// entry `entree`: what the preview then asks for.
    fn right_click(line: usize, entry: &str, auto_cut: bool) -> Option<(ImportLineAction, usize)> {
        let ctx = context();
        let preview = preview_of(1);
        let mut chosen = None;
        let mut paint =
            |ui: &mut egui::Ui| import_line_preview_widget(ui, &preview, auto_cut, &mut chosen);
        let painted = texts(&image(&ctx, Vec::new(), &mut paint));
        let pos = line_number(&painted, line).center;
        click(&ctx, pos, egui::PointerButton::Secondary, &mut paint);
        let painted = texts(&image(&ctx, Vec::new(), &mut paint));
        let pos = text_exact(&painted, entry).center;
        click(&ctx, pos, egui::PointerButton::Primary, &mut paint);
        chosen
    }

    /// A right-click on a line of the preview sets the import at that place,
    /// reporting the line number. "End Data Here" only makes sense on a data
    /// line or the footer, "Disable Auto Data End" only if it is active:
    /// greyed out otherwise, they ask for nothing (the same click when active
    /// being the witness).
    #[test]
    fn right_click_on_a_line_sets_the_import_at_that_place() {
        let start = tr!("import-line-data-start");
        let units = tr!("import-line-units");
        let end = tr!("import-line-data-end");
        let without_cut = tr!("import-line-no-auto-end");
        for (line, entry, auto_cut, expected) in [
            (2, &start, true, Some(ImportLineAction::DataStartsHere)),
            (1, &units, true, Some(ImportLineAction::UnitsLine)),
            (2, &end, true, Some(ImportLineAction::DataEndsHere)),
            (3, &end, true, Some(ImportLineAction::DataEndsHere)),
            (1, &end, true, None),
            (3, &without_cut, true, Some(ImportLineAction::NoAutoCut)),
            (3, &without_cut, false, None),
        ] {
            assert_eq!(
                right_click(line, entry, auto_cut),
                expected.map(|action| (action, line)),
                "line {line}, {entry:?}, auto cut {auto_cut}"
            );
        }
    }

    /// The preview colors each line by its role (number and raw text) and
    /// flags in the warning color a cell that would be kept as text — a
    /// numeric cell of the same line is not (witness). The number of lines
    /// shown is stated only if some are missing.
    #[test]
    fn the_preview_colors_lines_by_role_and_cells_kept_as_text() {
        let ctx = context();
        let visuals = ctx.global_style().visuals.clone();
        let mut chosen = None;
        let preview = preview_of(1);
        let painted = texts(&image(&ctx, Vec::new(), |ui| {
            import_line_preview_widget(ui, &preview, true, &mut chosen)
        }));
        assert_eq!(text_exact(&painted, "abc").color, visuals.warn_fg_color);
        assert_eq!(text_exact(&painted, "1.5").color, visuals.text_color());
        let color_of = |role| {
            let mut color = egui::Color32::PLACEHOLDER;
            image(&ctx, Vec::new(), |ui| color = line_role_color(ui, role));
            color
        };
        let header = color_of(LineRole::MainHeader);
        let footer = color_of(LineRole::Footer);
        let data = color_of(LineRole::Data);
        assert_eq!(text_exact(&painted, "TITLE").color, header);
        assert_eq!(line_number(&painted, 1).color, header);
        assert_eq!(text_exact(&painted, "END").color, footer);
        assert_eq!(line_number(&painted, 3).color, footer);
        assert_eq!(line_number(&painted, 2).color, data);
        // Nothing is missing: no summary, no "1 of 1".
        let complete = tr!("import-preview-showing", shown = 1, total = 1);
        assert!(!is_painted(&painted, &complete));
        let summary = tr!("import-preview-showing", shown = 1, total = 40);
        assert!(!is_painted(&painted, &summary));

        let truncated = preview_of(40);
        let painted = texts(&image(&ctx, Vec::new(), |ui| {
            import_line_preview_widget(ui, &truncated, true, &mut chosen)
        }));
        text_exact(&painted, &summary);
        assert_eq!(chosen, None, "painting sets nothing");
    }

    /// The preview's color code (OriginLab ASCII wizard style): one color for
    /// every recognized header line, one for the subheader comments, one for
    /// the footer, one for the ignored lines, and the ordinary text color for
    /// the data — five families, all distinct in the application theme.
    #[test]
    fn each_family_of_lines_has_its_color_in_the_preview() {
        let ctx = context();
        let mut colors = Vec::new();
        image(&ctx, Vec::new(), |ui| {
            colors = [
                LineRole::MainHeader,
                LineRole::Subheader,
                LineRole::SubheaderLongName,
                LineRole::SubheaderUnit,
                LineRole::ColumnHeaderRow,
                LineRole::SubheaderComment,
                LineRole::SubheaderXPrime,
                LineRole::Footer,
                LineRole::Comment,
                LineRole::Skipped,
                LineRole::Data,
                LineRole::Blank,
            ]
            .map(|role| (role, line_role_color(ui, role)))
            .to_vec();
        });
        let color = |role| colors.iter().find(|(r, _)| *r == role).unwrap().1;
        let families = [
            vec![
                LineRole::MainHeader,
                LineRole::Subheader,
                LineRole::SubheaderLongName,
                LineRole::SubheaderUnit,
                LineRole::ColumnHeaderRow,
            ],
            vec![LineRole::SubheaderComment, LineRole::SubheaderXPrime],
            vec![LineRole::Footer],
            vec![LineRole::Comment, LineRole::Skipped],
            vec![LineRole::Data, LineRole::Blank],
        ];
        for family in &families {
            for role in family {
                assert_eq!(color(*role), color(family[0]), "{role:?}");
            }
        }
        distinct(&families, |family| format!("{:?}", color(family[0])));
    }

    /// Starting again from a blank template resets the recipe, while keeping
    /// the files already chosen for the next preview.
    #[test]
    fn new_template_resets_the_recipe_and_keeps_the_files() {
        let mut bench = bench(true);
        let ctx = context();
        let before = bench.state.import_state().unwrap().recipe.clone();
        assert_ne!(
            before,
            stokesforge_import::ImportRecipe::new(tr!("import-template-untitled")),
            "witness: the initial recipe carries our Jasco settings"
        );
        let painted = texts(&image(&ctx, Vec::new(), |ui| {
            super::import_step_files(ui, &mut bench.state, &mut Vec::new())
        }));
        let position = text_exact(&painted, &tr!("import-files-reset")).center;
        click(&ctx, position, egui::PointerButton::Primary, |ui| {
            super::import_step_files(ui, &mut bench.state, &mut Vec::new())
        });
        let import = bench.state.import_state().unwrap();
        assert_eq!(
            import.recipe,
            stokesforge_import::ImportRecipe::new(tr!("import-template-untitled"))
        );
        assert_eq!(import.paths, bench.files);
        assert!(!import.paths.is_empty(), "witness: files are kept");
    }

    /// S-F3–S-F5 (#229): deleting a template (Ctrl+Z does not bring it back)
    /// goes through a modal confirmation over the wizard, Cancel by default:
    /// Enter cancels it without validating the wizard (which stays open); the
    /// verb deletes. No more "Keep".
    #[test]
    fn deleting_a_template_goes_through_a_modal_confirmation() {
        let mut bench = bench(true);
        let ctx = context();
        let mut actions = Vec::new();
        bench.state.request_delete_import_recipe("Jasco");
        let painted = paint_wizard(&ctx, &mut bench.state, &mut actions, Vec::new());
        assert!(is_painted(&painted, &tr!("import-files-delete-title")));
        text_exact(&painted, &tr!("import-files-delete-yes"));
        paint_wizard(
            &ctx,
            &mut bench.state,
            &mut actions,
            vec![key(egui::Key::Enter)],
        );
        let import = bench.state.import_state().expect("the wizard stays open");
        assert!(import.confirm_delete.is_none(), "Enter = Cancel");
        assert!(actions.is_empty(), "the wizard validated nothing");

        bench.state.request_delete_import_recipe("Jasco");
        let painted = paint_wizard(&ctx, &mut bench.state, &mut actions, Vec::new());
        let verb = text_exact(&painted, &tr!("import-files-delete-yes")).center;
        click(&ctx, verb, egui::PointerButton::Primary, |ui| {
            import_window(ui.ctx(), &mut bench.state, &mut actions)
        });
        assert!(
            bench
                .state
                .import_state()
                .expect("open")
                .confirm_delete
                .is_none(),
            "the verb answered"
        );
    }

    /// The field tied to a delimiter keeps its value when the same variant is
    /// chosen again. Changing option proves the click reaches the list.
    #[test]
    fn choosing_the_delimiter_again_keeps_its_parameters() {
        for initial in [Separator::Custom('@'), Separator::FixedWidth(vec![4, 8])] {
            let mut bench = bench(false);
            let ctx = context();
            bench.state.import_state_mut().unwrap().recipe.separator = initial.clone();
            choose_delimiter(&ctx, &mut bench.state, &separator_label(&initial));
            assert_eq!(
                bench.state.import_state().unwrap().recipe.separator,
                initial
            );
            choose_delimiter(&ctx, &mut bench.state, &separator_label(&Separator::Comma));
            assert_eq!(
                bench.state.import_state().unwrap().recipe.separator,
                Separator::Comma,
                "witness: choosing another option does write the recipe"
            );
        }
    }

    fn choose_delimiter(ctx: &egui::Context, state: &mut AppState, label: &str) {
        let current = separator_label(&state.import_state().unwrap().recipe.separator);
        let painted = texts(&image(ctx, Vec::new(), |ui| {
            super::import_step_format(ui, state)
        }));
        let list = text_exact(&painted, &current).center;
        click(ctx, list, egui::PointerButton::Primary, |ui| {
            super::import_step_format(ui, state)
        });
        let painted = texts(&image(ctx, Vec::new(), |ui| {
            super::import_step_format(ui, state)
        }));
        // The open list is painted over the form, so last: the same label can
        // also be that of a radio (the decimal's "Comma").
        let choice = painted
            .iter()
            .rfind(|text| text.text == label && text.center != list)
            .unwrap_or_else(|| panic!("{label:?} missing from the open list"))
            .center;
        click(ctx, choice, egui::PointerButton::Primary, |ui| {
            super::import_step_format(ui, state)
        });
    }
}
