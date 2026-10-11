//! Assistant d'import (#2, #3–#8) : les six étapes (fichiers, format, en-têtes,
//! colonnes, métadonnées, destination), l'aperçu en lignes colorées par rôle
//! et les libellés des choix. Un seul point d'entrée pour `app.rs` :
//! [`import_window`], un `modal_dialog_with` (jamais d'`egui::Window` brute).
//!
//! Sorti d'`app.rs` tel quel (#118), à une retouche près : le libellé d'une
//! fin des données (`data_end_label`) est une table à part, testable et
//! gardée par `tests/libelles.rs`, au lieu d'une fermeture de `data_end_row`.
//! Les tests sont en fin de fichier : ils peignent l'assistant sans fenêtre et
//! lisent les textes peints (contenu, couleur, position), jamais le source.

use eframe::egui;
use stokesforge_import::{DecimalStyle, HeaderSkip, NameSource, Separator};

use crate::errors;
use crate::file_picker::PickRequest;
use crate::icons::{icon_button, Icon, IconRole};
use crate::number_field::number_field;
use crate::quantities;
use crate::state::{Action, AppState, ImportDestination, ImportLineAction, ImportStep};
use crate::theme::{section_title, FIELD_WIDTH_MEDIUM, FIELD_WIDTH_SHORT};
use crate::tr;

use super::dialogs::{modal_dialog_with, DialogFooter, DialogOutcome};
use super::form::{choice, choice_by, form, ChoiceOption, Form};
use super::hover_help::{help_beside, help_text};

/// Libellé d'une option « Fin des données » : celui de la variante, quel que
/// soit le texte ou le nombre qu'elle porte.
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

/// Réglage « Fin des données » de la recette d'import (issue #4) : coupure
/// automatique par défaut, désactivable. Le texte ou le nombre associé à
/// l'option choisie reste dans la recette tant qu'on en change. Sans libellé :
/// le titre de section « Data end » le porte (#227).
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

/// Deux options de même variante, quel que soit ce qu'elles portent : en
/// reprendre une garde le texte ou le nombre déjà saisi.
fn same_variant<T>(a: &T, b: &T) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// Libellé affiché pour un [`Separator`] dans la fenêtre d'import.
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

/// Libellé d'un encodage forcé.
fn encoding_label(encoding: stokesforge_import::Encoding) -> String {
    use stokesforge_import::Encoding;
    // Les noms d'encodage sont des noms techniques : seul « Auto » se traduit.
    match encoding {
        Encoding::Auto => tr!("common-auto"),
        Encoding::Utf8 => "UTF-8".to_string(),
        Encoding::Windows1252 => "Windows-1252".to_string(),
        Encoding::Latin1 => "ISO 8859-1".to_string(),
        Encoding::Utf16Le => "UTF-16 LE".to_string(),
        Encoding::Utf16Be => "UTF-16 BE".to_string(),
    }
}

/// Libellé d'un séparateur de milliers.
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

/// Libellé affiché pour un [`DecimalStyle`] dans la fenêtre d'import.
fn decimal_label(decimal: DecimalStyle) -> String {
    match decimal {
        DecimalStyle::Dot => tr!("import-option-dot"),
        DecimalStyle::Comma => tr!("import-option-comma"),
        DecimalStyle::Auto => tr!("common-auto"),
    }
}

/// Une ligne « from … to … » d'assignation d'un rôle de sous-en-tête (voir
/// [`import_window`]) : une case pour activer/désactiver le rôle (`None` vs
/// `Some`), deux compteurs pour la plage. `id_source` distingue les trois
/// rôles (`long_name`/`unit`/`comment`) qui, sans lui, partageraient le même
/// id auto-généré (même position relative dans leurs branches `if`
/// respectives) et se marcheraient dessus.
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

/// Couleur associée à un [`stokesforge_import::LineRole`] dans l'aperçu — bleu
/// pour un rôle d'en-tête reconnu, magenta pour un commentaire, orange pour
/// le pied de page, couleur de texte par défaut pour une donnée : même
/// esprit de code couleur que l'assistant d'import ASCII d'OriginLab
/// (§ légende de l'aperçu), sans reprendre ses teintes exactes.
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

/// Couleur et libellé de chaque code de l'aperçu (légende colorée).
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

/// L'aperçu du découpage d'un fichier, aligné en colonnes, une ligne par ligne
/// — voir `stokesforge_import::preview` pour ce qui borne son coût. Un clic droit
/// sur une ligne règle l'import à cet endroit (`ImportLineAction`). Une
/// cellule qui deviendrait un texte conservé dans une colonne mixte est
/// annotée en couleur d'avertissement.
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

/// Volet de droite de l'assistant, visible à toutes les étapes : choix du
/// fichier montré, encodage, légende et aperçu.
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

/// Ce que l'assistant demande à l'application après une image, une fois
/// l'emprunt de l'état relâché.
#[derive(Default)]
struct ImportWizardRequests {
    save_recipe: bool,
    cancel: bool,
    auto_detect: bool,
    back: bool,
    forward: bool,
    line_action: Option<(ImportLineAction, usize)>,
    import: bool,
    /// Sélecteurs de fichiers demandés (#192), poussés comme actions.
    picks: Vec<Action>,
}

fn recipe_combo_text(name: &str, modified: bool) -> String {
    if modified {
        tr!("import-files-template-modified", name = name)
    } else {
        name.to_string()
    }
}

/// Étape 1 : fichiers et modèle.
fn import_step_files(ui: &mut egui::Ui, state: &mut AppState, picks: &mut Vec<Action>) {
    // Sans état, l'étape ne peint rien, comme avant son découpage.
    if state.import_state().is_none() {
        return;
    }
    form(ui, "import_files", |rows| {
        import_template_controls(rows, state, picks);
        import_file_choices(rows, state, picks);
        import_file_status_rows(rows.ui(), state);
    });
}

/// Modèle sélectionné, échanges JSON et confirmation de suppression.
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
    // Les modèles enregistrés sont des données : liste déroulante quel que
    // soit leur nombre.
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
        // Un modèle fourni d'office ne se supprime pas (il reviendrait au
        // lancement suivant) : on l'enregistre sous un autre nom pour le
        // modifier.
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
        rows.field(|ui| {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr!("import-files-delete-confirm", name = name),
            );
            if ui.button(tr!("import-files-delete-yes")).clicked() {
                state.confirm_delete_import_recipe();
            }
            if ui.button(tr!("import-files-delete-keep")).clicked() {
                state.cancel_delete_import_recipe();
            }
        });
    }
}

/// Ajout de fichiers et choix parmi les fichiers récemment importés.
fn import_file_choices(rows: &mut Form<'_>, state: &mut AppState, actions: &mut Vec<Action>) {
    rows.section(tr!("import-files-section"));
    rows.ui().horizontal(|ui| {
        if ui
            .button(tr!("import-files-add"))
            .on_hover_text(tr!("import-files-add", "hint"))
            .clicked()
        {
            // Le dossier du dernier fichier importé.
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

/// Choix du fichier prévisualisé et retrait, appliqués après toutes les rangées.
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

/// Étape 2 : séparateur et décimale.
fn import_step_format(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(import) = state.import_state_mut() else {
        return;
    };
    let recipe = &mut import.recipe;
    form(ui, "import_format", |rows| {
        import_delimiter_controls(rows, recipe);
        import_number_controls(rows, recipe);
        rows.row(tr!("import-format-comment-prefix"), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut recipe.comment_prefix)
                    .desired_width(FIELD_WIDTH_SHORT),
            );
            help_text(ui, tr!("import-format-comment-prefix", "hint"));
        });
        import_block_encoding_controls(rows, recipe);
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

/// Délimiteur et paramètres de sa variante (caractère ou largeurs fixes).
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

/// Décimale, milliers et valeurs manquantes, dans leur ordre de saisie.
fn import_number_controls(rows: &mut Form<'_>, recipe: &mut stokesforge_import::ImportRecipe) {
    // Trois choix : des radios (C1).
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

/// Découpage en blocs et encodage du fichier.
fn import_block_encoding_controls(
    rows: &mut Form<'_>,
    recipe: &mut stokesforge_import::ImportRecipe,
) {
    rows.row(tr!("import-format-blocks"), |ui| {
        use stokesforge_import::BlockSplit;
        // Trois découpages : des radios (C1).
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

/// Le réglage d'en-tête de l'étape 3, déduit de la recette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderMode {
    None,
    Lines,
    Marker,
}

/// Étape 3 : en-tête (un seul réglage), fin des données et noms des colonnes,
/// trois blocs sous leur titre (L3).
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
        // Garder les valeurs choisies pendant le grisage pour les retrouver
        // quand les noms ne viennent plus de la première ligne.
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

/// Le bloc « Header » de l'étape 3 : mode, lignes, rôles des sous-en-têtes.
fn import_header_controls(
    rows: &mut Form<'_>,
    recipe: &mut stokesforge_import::ImportRecipe,
    has_files: bool,
    requests: &mut ImportWizardRequests,
) {
    // Comme `parse` : le découpage structuré ne compte que s'il a des lignes
    // (l'ancien drapeau `auto_detect` seul ne suffit pas).
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

/// Libellé de la nature choisie pour une colonne.
fn column_kind_label(kind: stokesforge_import::ColumnKind) -> String {
    match kind {
        stokesforge_import::ColumnKind::Auto => tr!("column-type-numeric"),
        stokesforge_import::ColumnKind::Text => tr!("column-type-text"),
        stokesforge_import::ColumnKind::DateTime => tr!("column-type-datetime"),
    }
}

/// Libellé d'un rôle de colonne choisi à l'étape « Colonnes ».
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

/// Étape 4 : le résultat, colonne par colonne, avec le rôle de chacune.
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
                        ui.label(crate::theme::bold(title));
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

/// Étape 5 : les métadonnées à garder.
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

/// Étape 6 : destination, enregistrement du modèle.
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
        // Trois destinations dont une peut manquer (aucun classeur actif) :
        // radios l'un sous l'autre, posés ici pour griser la première.
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
            // Quatre modes : une liste déroulante (C1).
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

/// L'assistant d'import (« Importer… », menu Fichier) en six étapes façon
/// assistant ASCII d'Origin : fichiers et modèle, format, en-tête et fin des
/// données, colonnes, métadonnées, destination. L'aperçu du fichier reste
/// visible à droite à toutes les étapes. « Terminer » importe dès que des
/// fichiers sont choisis, avec le modèle tel qu'il est — import en deux clics
/// pour un format connu. **Dialogue modal de la brique commune**
/// ([`modal_dialog_with`], comme les propriétés du graphe), peint depuis
/// [`StokesForgeApp::ui`](eframe::App::ui) tant que [`AppState::import_state`] est `Some` ; la croix
/// et Échap valent « Annuler », Entrée « Terminer ».
pub(super) fn import_window(ctx: &egui::Context, state: &mut AppState, actions: &mut Vec<Action>) {
    let Some(step) = state.import_state().map(|import| import.step) else {
        return;
    };
    let has_files = state
        .import_state()
        .is_some_and(|import| !import.paths.is_empty());
    // Les deux fermetures de la brique (contenu, boutons du pied) écrivent
    // dans les mêmes demandes, l'une après l'autre : `RefCell`.
    let requests = std::cell::RefCell::new(ImportWizardRequests::default());
    let (finish_label, finish_hint) = (
        tr!("import-button-finish"),
        tr!("import-button-finish", "hint"),
    );
    let add_file_first = tr!("disabled-add-file-first");
    let outcome = modal_dialog_with(
        ctx,
        egui::Id::new("stokesforge_import_wizard"),
        &tr!("import-window-title"),
        egui::vec2(1000.0, 620.0),
        DialogFooter {
            accept_label: &finish_label,
            accept_hint: &finish_hint,
            accept_enabled: has_files,
            accept_disabled: Some(&add_file_first),
        },
        |footer| {
            // Ajoutés de droite à gauche : « Next » puis « Back ».
            if footer
                .add_enabled(
                    step != ImportStep::Destination,
                    egui::Button::new(tr!("import-button-next")).min_size(egui::vec2(84.0, 24.0)),
                )
                .on_disabled_hover_text(tr!("disabled-import-last-step"))
                .clicked()
            {
                requests.borrow_mut().forward = true;
            }
            if footer
                .add_enabled(
                    step != ImportStep::Files,
                    egui::Button::new(tr!("import-button-back")).min_size(egui::vec2(84.0, 24.0)),
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
                        ui.weak("›");
                    }
                    let text = format!("{} {}", i + 1, candidate.title());
                    if *candidate == step {
                        // Comme l'onglet actif : gras et accent.
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
    // Les aperçus se recalculent après une pause de frappe (T1, #224).
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

    // Tout ce qui demande l'état entier est appliqué ici, une fois la fenêtre
    // peinte.
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
    use crate::state::{Action, AppState, ImportDestination, ImportLineAction, ImportStep};
    use crate::tr;

    use super::super::test_support::{
        cliquer, contexte, distincts, image, texte_exact, textes, touche, Texte,
    };

    // -- Outillage --------------------------------------------------------

    /// Sérialise les tests de ce module qui ouvrent l'assistant :
    /// `XDG_CONFIG_HOME` est un état du *processus*, et `cargo test` fait
    /// tourner les tests en parallèle (même motif que
    /// `tests/shell.rs::isolate_recipe_store`).
    static CONFIG_ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Redirige `XDG_CONFIG_HOME` vers un dossier temporaire tant que la
    /// valeur vit : `AppState::open_import` lit les modèles et les fichiers
    /// récents de la machine (`RecipeStore`, `RecentFiles`), ce qui rendrait
    /// le test dépendant du poste. `Drop` restaure la valeur d'avant, même
    /// sur panique.
    struct ConfigIsolee {
        avant: Option<std::ffi::OsString>,
        _dossier: tempfile::TempDir,
        _verrou: MutexGuard<'static, ()>,
    }

    impl Drop for ConfigIsolee {
        fn drop(&mut self) {
            match self.avant.take() {
                Some(valeur) => std::env::set_var("XDG_CONFIG_HOME", valeur),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
        }
    }

    fn isoler_la_config() -> ConfigIsolee {
        let verrou = CONFIG_ENV_LOCK
            .lock()
            .unwrap_or_else(|empoisonne| empoisonne.into_inner());
        let dossier = tempfile::tempdir().unwrap();
        let avant = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", dossier.path());
        ConfigIsolee {
            avant,
            _dossier: dossier,
            _verrou: verrou,
        }
    }

    /// Un fichier Jasco : un en-tête jusqu'au repère `XYDATA`, trois lignes
    /// de données à deux colonnes, un pied de page.
    const JASCO: &str =
        "TITLE;S13\nXUNITS;NANOMETERS\nXYDATA\n800;1\n799;2\n798;3\n\nData points;3\n";

    /// L'assistant ouvert sur un projet vide, sans classeur actif, avec
    /// `fichiers` déjà choisis ; la recette lit `JASCO` (point-virgule,
    /// en-tête jusqu'à `XYDATA`). Les champs gardent en vie ce dont l'état
    /// dépend.
    struct Banc {
        state: AppState,
        fichiers: Vec<PathBuf>,
        _session: Session,
        _dossier: tempfile::TempDir,
        _config: ConfigIsolee,
    }

    fn banc(avec_fichier: bool) -> Banc {
        let config = isoler_la_config();
        let session = Session::spawn(Project::new("Import").unwrap(), None).unwrap();
        let mut state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let dossier = tempfile::tempdir().unwrap();
        let mut fichiers = Vec::new();
        if avec_fichier {
            let fichier = dossier.path().join("jasco.csv");
            std::fs::write(&fichier, JASCO).unwrap();
            fichiers.push(fichier);
        }
        state.open_import(None::<stokesforge_core::NodeId>);
        state.set_import_paths(fichiers.clone());
        let import = state.import_state_mut().unwrap();
        import.recipe.separator = Separator::Semicolon;
        import.recipe.header = HeaderSkip::UntilMarker("XYDATA".into());
        Banc {
            state,
            fichiers,
            _session: session,
            _dossier: dossier,
            _config: config,
        }
    }

    fn est_peint(textes: &[Texte], cherche: &str) -> bool {
        textes.iter().any(|t| t.texte == cherche)
    }

    /// Peint l'assistant trois fois (mise en page stabilisée, dialogue
    /// centré) puis une quatrième avec `events` ; rend les textes de la
    /// dernière image.
    fn peindre_assistant(
        context: &egui::Context,
        state: &mut AppState,
        actions: &mut Vec<Action>,
        events: Vec<egui::Event>,
    ) -> Vec<Texte> {
        for _ in 0..3 {
            image(context, Vec::new(), |ui| {
                import_window(ui.ctx(), state, actions)
            });
        }
        textes(&image(context, events, |ui| {
            import_window(ui.ctx(), state, actions)
        }))
    }

    // -- L'assistant entier -----------------------------------------------

    #[test]
    fn noms_de_colonnes_inactifs_avec_noms_en_premiere_ligne() {
        let mut banc = banc(true);
        banc.state.import_state_mut().unwrap().step = ImportStep::Header;
        banc.state
            .import_state_mut()
            .unwrap()
            .recipe
            .has_column_header_row = true;
        let context = contexte();
        let mut actions = Vec::new();
        let peints = peindre_assistant(&context, &mut banc.state, &mut actions, Vec::new());
        let cle = texte_exact(&peints, &tr!("import-header-name-key")).centre;
        cliquer(&context, cle, egui::PointerButton::Primary, |ui| {
            import_window(ui.ctx(), &mut banc.state, &mut actions)
        });
        assert_eq!(
            banc.state.import_state().unwrap().recipe.name_source,
            stokesforge_import::NameSource::Filename,
            "la section désactivée doit ignorer les clics"
        );

        let case = texte_exact(&peints, &tr!("import-header-names-first-line")).centre;
        cliquer(&context, case, egui::PointerButton::Primary, |ui| {
            import_window(ui.ctx(), &mut banc.state, &mut actions)
        });
        assert!(
            !banc
                .state
                .import_state()
                .unwrap()
                .recipe
                .has_column_header_row
        );
        let actifs = peindre_assistant(&context, &mut banc.state, &mut actions, Vec::new());
        assert_ne!(
            texte_exact(&peints, &tr!("import-column-names")).couleur,
            texte_exact(&actifs, &tr!("import-column-names")).couleur,
            "le titre doit refléter le grisage"
        );
        let cle = texte_exact(&actifs, &tr!("import-header-name-key")).centre;
        cliquer(&context, cle, egui::PointerButton::Primary, |ui| {
            import_window(ui.ctx(), &mut banc.state, &mut actions)
        });
        assert_eq!(
            banc.state.import_state().unwrap().recipe.name_source,
            stokesforge_import::NameSource::HeaderKey(String::new()),
            "décocher réactive la section"
        );
        banc.state.import_state_mut().unwrap().recipe.name_source =
            stokesforge_import::NameSource::HeaderKey("TITLE".into());
        let actifs = peindre_assistant(&context, &mut banc.state, &mut actions, Vec::new());
        let case = texte_exact(&actifs, &tr!("import-header-names-first-line")).centre;
        cliquer(&context, case, egui::PointerButton::Primary, |ui| {
            import_window(ui.ctx(), &mut banc.state, &mut actions)
        });
        let grises = peindre_assistant(&context, &mut banc.state, &mut actions, Vec::new());
        let champ = texte_exact(&grises, "TITLE").centre;
        cliquer(&context, champ, egui::PointerButton::Primary, |ui| {
            import_window(ui.ctx(), &mut banc.state, &mut actions)
        });
        peindre_assistant(
            &context,
            &mut banc.state,
            &mut actions,
            vec![egui::Event::Text("modification".into())],
        );
        let fichier = texte_exact(&grises, &tr!("import-header-name-file")).centre;
        cliquer(&context, fichier, egui::PointerButton::Primary, |ui| {
            import_window(ui.ctx(), &mut banc.state, &mut actions)
        });
        assert_eq!(
            banc.state.import_state().unwrap().recipe.name_source,
            stokesforge_import::NameSource::HeaderKey("TITLE".into()),
            "le champ et les deux choix restent inactifs, sans perdre la clé"
        );
    }

    /// Issue #6 : chaque étape de l'assistant se peint sans paniquer, montre
    /// son titre en surbrillance (les autres en retrait), **son** contenu et
    /// pas celui des autres étapes, et l'aperçu du fichier à droite, quelle
    /// que soit l'étape. Peindre n'importe rien et ne ferme pas l'assistant.
    #[test]
    fn chaque_etape_de_l_assistant_d_import_se_peint_avec_son_apercu() {
        let mut banc = banc(true);
        let context = contexte();
        let visuals = context.global_style().visuals.clone();
        let (fort, faible) = (visuals.strong_text_color(), visuals.weak_text_color());
        assert_ne!(fort, faible, "témoin : surbrillance et retrait diffèrent");
        // Un libellé propre à chaque étape.
        let propres = [
            (ImportStep::Files, tr!("import-files-template")),
            (ImportStep::Format, tr!("import-format-delimiter")),
            (ImportStep::Header, tr!("import-dataend")),
            (ImportStep::Columns, tr!("import-columns-read-as")),
            (ImportStep::Metadata, tr!("import-metadata-keep-header")),
            (ImportStep::Destination, tr!("import-destination")),
        ];
        assert_eq!(propres.len(), ImportStep::ALL.len());
        for step in ImportStep::ALL {
            assert_eq!(banc.state.import_state().unwrap().step, step);
            let colonnes_avant = banc.state.import_state().unwrap().recipe.column_roles.len();
            let mut actions = Vec::new();
            let peints = peindre_assistant(&context, &mut banc.state, &mut actions, Vec::new());
            for (i, candidate) in ImportStep::ALL.iter().enumerate() {
                let fil = texte_exact(&peints, &format!("{} {}", i + 1, candidate.title()));
                let attendu = if *candidate == step { fort } else { faible };
                assert_eq!(
                    fil.couleur, attendu,
                    "{step:?} : fil d'Ariane de {candidate:?}"
                );
                assert_eq!(
                    fil.gras,
                    *candidate == step,
                    "{step:?} : seule l'étape courante est en gras ({candidate:?})"
                );
            }
            for (proprietaire, libelle) in &propres {
                assert_eq!(
                    est_peint(&peints, libelle),
                    *proprietaire == step,
                    "{step:?} : {libelle:?} appartient à {proprietaire:?}"
                );
            }
            // L'aperçu : la ligne d'en-tête brute et une cellule de données.
            assert!(
                peints.iter().any(|t| t.texte.contains("TITLE;S13")),
                "{step:?} : aperçu absent"
            );
            assert!(est_peint(&peints, "800"), "{step:?} : cellule absente");
            if step == ImportStep::Columns {
                // Un rôle et une nature par colonne lue : la liste
                // déroulante de la colonne 2 écrit `column_roles[1]`.
                assert!(colonnes_avant < 2, "témoin : la recette part sans rôles");
                let recipe = &banc.state.import_state().unwrap().recipe;
                assert_eq!(recipe.column_roles.len(), 2);
                assert_eq!(recipe.column_kinds.len(), 2);
            }
            assert!(actions.is_empty(), "peindre n'importe rien");
            banc.state.import_step_forward();
        }
        assert!(
            banc.state.import_state().is_some(),
            "l'assistant reste ouvert"
        );
    }

    /// « Terminer » (Entrée, bouton par défaut) importe dès que des fichiers
    /// sont choisis, avec la recette telle qu'elle est — import en deux
    /// clics — et ferme l'assistant ; sans fichier, Entrée ne fait rien
    /// (témoin). Échap vaut « Annuler » : fermé, rien d'importé, même avec
    /// des fichiers.
    #[test]
    fn terminer_importe_la_recette_telle_quelle_et_annuler_n_importe_rien() {
        let mut complet = banc(true);
        let recette = complet.state.import_state().unwrap().recipe.clone();
        assert_eq!(recette.separator, Separator::Semicolon);
        let mut actions = Vec::new();
        peindre_assistant(
            &contexte(),
            &mut complet.state,
            &mut actions,
            vec![touche(egui::Key::Enter)],
        );
        assert_eq!(
            actions,
            vec![Action::StartImport {
                destination: ImportDestination::NewBookForAll,
                recipe: recette,
                paths: complet.fichiers.clone(),
            }]
        );
        assert!(complet.state.import_state().is_none(), "Terminer ferme");
        drop(complet);

        let mut vide = banc(false);
        let context = contexte();
        let mut actions = Vec::new();
        peindre_assistant(
            &context,
            &mut vide.state,
            &mut actions,
            vec![touche(egui::Key::Enter)],
        );
        assert!(actions.is_empty(), "sans fichier, rien à importer");
        assert!(vide.state.import_state().is_some(), "ni fermé");
        drop(vide);

        let mut annule = banc(true);
        let mut actions = Vec::new();
        peindre_assistant(
            &contexte(),
            &mut annule.state,
            &mut actions,
            vec![touche(egui::Key::Escape)],
        );
        assert!(actions.is_empty(), "Annuler n'importe rien");
        assert!(annule.state.import_state().is_none(), "Annuler ferme");
    }

    /// Les boutons du pied : « Suivant » avance d'une étape, « Précédent »
    /// recule — pas l'inverse.
    #[test]
    fn suivant_et_precedent_changent_d_etape() {
        let mut banc = banc(true);
        let context = contexte();
        let mut actions = Vec::new();
        for (bouton, attendue) in [
            (tr!("import-button-next"), ImportStep::Format),
            (tr!("import-button-next"), ImportStep::Header),
            (tr!("import-button-back"), ImportStep::Format),
        ] {
            let peints = peindre_assistant(&context, &mut banc.state, &mut actions, Vec::new());
            let pos = texte_exact(&peints, &bouton).centre;
            cliquer(&context, pos, egui::PointerButton::Primary, |ui| {
                import_window(ui.ctx(), &mut banc.state, &mut actions)
            });
            assert_eq!(
                banc.state.import_state().unwrap().step,
                attendue,
                "{bouton}"
            );
        }
        assert!(actions.is_empty());
    }

    /// Le clic droit dans l'aperçu de l'assistant atteint la recette, une
    /// fois l'image peinte : « Les données commencent ici » sur la ligne 4
    /// (`800;1`) remplace le repère `XYDATA` par trois lignes d'en-tête.
    #[test]
    fn le_clic_droit_dans_l_apercu_de_l_assistant_regle_la_recette() {
        let mut banc = banc(true);
        let context = contexte();
        let mut actions = Vec::new();
        let peints = peindre_assistant(&context, &mut banc.state, &mut actions, Vec::new());
        let ligne = numero(&peints, 4).centre;
        cliquer(&context, ligne, egui::PointerButton::Secondary, |ui| {
            import_window(ui.ctx(), &mut banc.state, &mut actions)
        });
        let peints = textes(&image(&context, Vec::new(), |ui| {
            import_window(ui.ctx(), &mut banc.state, &mut actions)
        }));
        let entree = texte_exact(&peints, &tr!("import-line-data-start")).centre;
        cliquer(&context, entree, egui::PointerButton::Primary, |ui| {
            import_window(ui.ctx(), &mut banc.state, &mut actions)
        });
        let recipe = &banc.state.import_state().unwrap().recipe;
        assert_eq!(recipe.header, HeaderSkip::None);
        assert_eq!(recipe.header_layout.main_header_lines, 3);
        assert!(actions.is_empty());
    }

    // -- Les libellés des choix ---------------------------------------------

    /// Chaque liste déroulante de l'assistant montre des choix qu'on
    /// distingue ; le libellé suit la variante, pas la valeur qu'elle porte
    /// (un séparateur « autre » reste « autre » quel que soit son caractère).
    #[test]
    fn les_choix_d_une_meme_liste_ont_des_libelles_distincts() {
        distincts(
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
        distincts(
            &[DecimalStyle::Auto, DecimalStyle::Dot, DecimalStyle::Comma],
            |d| decimal_label(*d),
        );
        // Les milliers : les cinq choix proposés, plus un caractère lu dans
        // un modèle, qui s'affiche « autre » sans se confondre avec eux.
        distincts(
            &[None, Some(' '), Some(','), Some('.'), Some('\''), Some('_')],
            |t| thousands_label(*t),
        );
        distincts(
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
        distincts(
            &[ColumnKind::Auto, ColumnKind::Text, ColumnKind::DateTime],
            |k| column_kind_label(*k),
        );
        distincts(
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
        distincts(
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
            data_end_label(&DataEnd::Marker("FIN".into()))
        );
        assert_eq!(
            data_end_label(&DataEnd::LinesAfterHeader(100)),
            data_end_label(&DataEnd::LinesAfterHeader(3))
        );
    }

    /// Une notion, un mot (`docs/textes-interface.md`) : la virgule, le point,
    /// « Auto » et « aucun » s'écrivent pareil d'une liste de l'assistant à
    /// l'autre.
    #[test]
    fn une_meme_notion_porte_le_meme_libelle_d_une_liste_a_l_autre() {
        let virgule = separator_label(&Separator::Comma);
        assert_eq!(decimal_label(DecimalStyle::Comma), virgule);
        assert_eq!(thousands_label(Some(',')), virgule);
        assert_eq!(thousands_label(Some('.')), decimal_label(DecimalStyle::Dot));
        let auto = separator_label(&Separator::Auto);
        assert_eq!(decimal_label(DecimalStyle::Auto), auto);
        assert_eq!(encoding_label(Encoding::Auto), auto);
        assert_eq!(column_role_label(ColumnRoleChoice::Auto), auto);
        let aucun = thousands_label(None);
        assert_eq!(column_role_label(ColumnRoleChoice::Ignore), aucun);
        assert_eq!(data_end_label(&DataEnd::None), aucun);
        // Un séparateur personnalisé est un « autre », comme un séparateur
        // de milliers inconnu.
        assert_eq!(
            separator_label(&Separator::Custom('|')),
            thousands_label(Some('_'))
        );
    }

    /// La liste des modèles dit qu'une recette a été modifiée sans être
    /// enregistrée, sans perdre le nom du modèle ; non modifiée, le nom seul.
    #[test]
    fn le_modele_modifie_se_signale_dans_la_liste_des_modeles() {
        assert_eq!(recipe_combo_text("Jasco", false), "Jasco");
        let modifie = recipe_combo_text("Jasco", true);
        assert_ne!(modifie, "Jasco");
        assert!(modifie.contains("Jasco"), "{modifie:?}");
    }

    // -- La fin des données -------------------------------------------------

    /// Ouvre la liste « Fin des données » puis clique le choix `libelle`.
    fn choisir_fin(context: &egui::Context, fin: &mut DataEnd, libelle: &str) {
        let peints = textes(&image(context, Vec::new(), |ui| data_end_row(ui, fin)));
        let liste = texte_exact(&peints, &data_end_label(fin)).centre;
        cliquer(context, liste, egui::PointerButton::Primary, |ui| {
            data_end_row(ui, fin)
        });
        let peints = textes(&image(context, Vec::new(), |ui| data_end_row(ui, fin)));
        // La liste ouverte : le choix voulu, ailleurs que sur le bouton fermé.
        let choix = peints
            .iter()
            .find(|t| t.texte == libelle && t.centre != liste)
            .unwrap_or_else(|| panic!("{libelle:?} absent de la liste ouverte"))
            .centre;
        cliquer(context, choix, egui::PointerButton::Primary, |ui| {
            data_end_row(ui, fin)
        });
    }

    /// Issue #4 : la fin des données montre le champ de l'option choisie
    /// (repère, nombre de lignes) ; reprendre la même option garde le texte
    /// saisi, en changer prend la valeur par défaut de la nouvelle (témoin :
    /// le clic atteint bien la liste).
    #[test]
    fn la_fin_des_donnees_garde_sa_valeur_tant_qu_on_ne_change_pas_d_option() {
        let context = contexte();
        let mut fin = DataEnd::Marker("FIN".into());
        let peints = textes(&image(&context, Vec::new(), |ui| {
            data_end_row(ui, &mut fin)
        }));
        assert!(est_peint(&peints, "FIN"), "le repère est dans son champ");

        choisir_fin(&context, &mut fin, &tr!("import-dataend-marker"));
        assert_eq!(
            fin,
            DataEnd::Marker("FIN".into()),
            "même option : texte gardé"
        );

        choisir_fin(&context, &mut fin, &tr!("import-dataend-after-header"));
        assert_eq!(fin, DataEnd::LinesAfterHeader(100), "autre option");
        let peints = textes(&image(&context, Vec::new(), |ui| {
            data_end_row(ui, &mut fin)
        }));
        assert!(est_peint(&peints, "100"), "le nombre est dans son champ");
        assert!(!est_peint(&peints, "FIN"));

        choisir_fin(&context, &mut fin, &tr!("import-dataend-auto"));
        assert_eq!(fin, DataEnd::Auto);
        let peints = textes(&image(&context, Vec::new(), |ui| {
            data_end_row(ui, &mut fin)
        }));
        assert!(!est_peint(&peints, "100"), "Auto n'a pas de champ");
    }

    // -- L'aperçu -----------------------------------------------------------

    /// Un aperçu de trois lignes : un en-tête, une donnée à deux cellules
    /// dont la seconde serait gardée en texte, un pied de page.
    fn apercu(total_data_lines: usize) -> FilePreview {
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

    /// Le numéro de la ligne `numero` dans l'aperçu : aligné à droite sur
    /// cinq caractères, puis son code de rôle (pas « 4 Colonnes », l'étape
    /// du fil d'Ariane).
    fn numero(peints: &[Texte], numero: usize) -> &Texte {
        let numero = numero.to_string();
        peints
            .iter()
            .find(|t| t.texte.get(..5).map(str::trim_start) == Some(numero.as_str()))
            .unwrap_or_else(|| panic!("ligne {numero} absente"))
    }

    /// Clic droit sur la ligne `ligne` de l'aperçu, puis clic sur l'entrée
    /// `entree` du menu : ce que l'aperçu demande alors.
    fn clic_droit(ligne: usize, entree: &str, auto_cut: bool) -> Option<(ImportLineAction, usize)> {
        let context = contexte();
        let preview = apercu(1);
        let mut chosen = None;
        let mut peindre =
            |ui: &mut egui::Ui| import_line_preview_widget(ui, &preview, auto_cut, &mut chosen);
        let peints = textes(&image(&context, Vec::new(), &mut peindre));
        let pos = numero(&peints, ligne).centre;
        cliquer(&context, pos, egui::PointerButton::Secondary, &mut peindre);
        let peints = textes(&image(&context, Vec::new(), &mut peindre));
        let pos = texte_exact(&peints, entree).centre;
        cliquer(&context, pos, egui::PointerButton::Primary, &mut peindre);
        chosen
    }

    /// Le clic droit sur une ligne de l'aperçu règle l'import à cet endroit,
    /// en rapportant le numéro de la ligne. « Fin des données ici » n'a de
    /// sens que sur une donnée ou le pied de page, « Annuler la coupure
    /// automatique » que si elle est active : grisés sinon, ils ne
    /// demandent rien (le même clic actif étant le témoin).
    #[test]
    fn le_clic_droit_sur_une_ligne_regle_l_import_a_cet_endroit() {
        let debut = tr!("import-line-data-start");
        let unites = tr!("import-line-units");
        let fin = tr!("import-line-data-end");
        let sans_coupure = tr!("import-line-no-auto-end");
        for (ligne, entree, auto_cut, attendu) in [
            (2, &debut, true, Some(ImportLineAction::DataStartsHere)),
            (1, &unites, true, Some(ImportLineAction::UnitsLine)),
            (2, &fin, true, Some(ImportLineAction::DataEndsHere)),
            (3, &fin, true, Some(ImportLineAction::DataEndsHere)),
            (1, &fin, true, None),
            (3, &sans_coupure, true, Some(ImportLineAction::NoAutoCut)),
            (3, &sans_coupure, false, None),
        ] {
            assert_eq!(
                clic_droit(ligne, entree, auto_cut),
                attendu.map(|action| (action, ligne)),
                "ligne {ligne}, {entree:?}, coupure auto {auto_cut}"
            );
        }
    }

    /// L'aperçu colore chaque ligne selon son rôle (numéro et texte brut) et
    /// signale en couleur d'avertissement une cellule qui serait gardée en
    /// texte — une cellule numérique de la même ligne, non (témoin). Le
    /// nombre de lignes montrées n'est dit que s'il en manque.
    #[test]
    fn l_apercu_colore_les_lignes_par_role_et_les_cellules_gardees_en_texte() {
        let context = contexte();
        let visuals = context.global_style().visuals.clone();
        let mut chosen = None;
        let preview = apercu(1);
        let peints = textes(&image(&context, Vec::new(), |ui| {
            import_line_preview_widget(ui, &preview, true, &mut chosen)
        }));
        assert_eq!(texte_exact(&peints, "abc").couleur, visuals.warn_fg_color);
        assert_eq!(texte_exact(&peints, "1.5").couleur, visuals.text_color());
        let couleur_de = |role| {
            let mut couleur = egui::Color32::PLACEHOLDER;
            image(&context, Vec::new(), |ui| {
                couleur = line_role_color(ui, role)
            });
            couleur
        };
        let entete = couleur_de(LineRole::MainHeader);
        let pied = couleur_de(LineRole::Footer);
        let donnee = couleur_de(LineRole::Data);
        assert_eq!(texte_exact(&peints, "TITLE").couleur, entete);
        assert_eq!(numero(&peints, 1).couleur, entete);
        assert_eq!(texte_exact(&peints, "END").couleur, pied);
        assert_eq!(numero(&peints, 3).couleur, pied);
        assert_eq!(numero(&peints, 2).couleur, donnee);
        // Rien ne manque : pas de résumé, ni « 1 sur 1 ».
        let complet = tr!("import-preview-showing", shown = 1, total = 1);
        assert!(!est_peint(&peints, &complet));
        let resume = tr!("import-preview-showing", shown = 1, total = 40);
        assert!(!est_peint(&peints, &resume));

        let tronque = apercu(40);
        let peints = textes(&image(&context, Vec::new(), |ui| {
            import_line_preview_widget(ui, &tronque, true, &mut chosen)
        }));
        texte_exact(&peints, &resume);
        assert_eq!(chosen, None, "peindre ne règle rien");
    }

    /// Le code couleur de l'aperçu (façon assistant ASCII d'OriginLab) :
    /// une couleur pour toute ligne d'en-tête reconnue, une pour les
    /// commentaires de sous-en-tête, une pour le pied de page, une pour les
    /// lignes ignorées, et le texte ordinaire pour les données — cinq
    /// familles, toutes distinctes dans le thème de l'application.
    #[test]
    fn chaque_famille_de_lignes_a_sa_couleur_dans_l_apercu() {
        let context = contexte();
        let mut couleurs = Vec::new();
        image(&context, Vec::new(), |ui| {
            couleurs = [
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
        let couleur = |role| couleurs.iter().find(|(r, _)| *r == role).unwrap().1;
        let familles = [
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
        for famille in &familles {
            for role in famille {
                assert_eq!(couleur(*role), couleur(famille[0]), "{role:?}");
            }
        }
        distincts(&familles, |famille| format!("{:?}", couleur(famille[0])));
    }
    /// Repartir d'un modèle vierge réinitialise la recette, tout en gardant
    /// les fichiers déjà choisis pour le prochain aperçu.
    #[test]
    fn nouveau_modele_reinitialise_la_recette_et_garde_les_fichiers() {
        let mut banc = banc(true);
        let context = contexte();
        let avant = banc.state.import_state().unwrap().recipe.clone();
        assert_ne!(
            avant,
            stokesforge_import::ImportRecipe::new(tr!("import-template-untitled")),
            "témoin : la recette initiale porte nos réglages Jasco"
        );
        let peints = textes(&image(&context, Vec::new(), |ui| {
            super::import_step_files(ui, &mut banc.state, &mut Vec::new())
        }));
        let position = texte_exact(&peints, &tr!("import-files-reset")).centre;
        cliquer(&context, position, egui::PointerButton::Primary, |ui| {
            super::import_step_files(ui, &mut banc.state, &mut Vec::new())
        });
        let import = banc.state.import_state().unwrap();
        assert_eq!(
            import.recipe,
            stokesforge_import::ImportRecipe::new(tr!("import-template-untitled"))
        );
        assert_eq!(import.paths, banc.fichiers);
        assert!(
            !import.paths.is_empty(),
            "témoin : des fichiers sont conservés"
        );
    }

    /// Le champ associé à un délimiteur conserve sa valeur en reprenant la
    /// même variante. Changer d'option prouve que le clic atteint la liste.
    #[test]
    fn reprendre_le_delimiteur_garde_ses_parametres() {
        for initial in [Separator::Custom('@'), Separator::FixedWidth(vec![4, 8])] {
            let mut banc = banc(false);
            let context = contexte();
            banc.state.import_state_mut().unwrap().recipe.separator = initial.clone();
            choisir_delimiteur(&context, &mut banc.state, &separator_label(&initial));
            assert_eq!(banc.state.import_state().unwrap().recipe.separator, initial);
            choisir_delimiteur(
                &context,
                &mut banc.state,
                &separator_label(&Separator::Comma),
            );
            assert_eq!(
                banc.state.import_state().unwrap().recipe.separator,
                Separator::Comma,
                "témoin : le choix d'une autre option écrit bien la recette"
            );
        }
    }

    fn choisir_delimiteur(context: &egui::Context, state: &mut AppState, libelle: &str) {
        let actuel = separator_label(&state.import_state().unwrap().recipe.separator);
        let peints = textes(&image(context, Vec::new(), |ui| {
            super::import_step_format(ui, state)
        }));
        let liste = texte_exact(&peints, &actuel).centre;
        cliquer(context, liste, egui::PointerButton::Primary, |ui| {
            super::import_step_format(ui, state)
        });
        let peints = textes(&image(context, Vec::new(), |ui| {
            super::import_step_format(ui, state)
        }));
        // La liste ouverte se peint par-dessus le formulaire, donc en dernier :
        // un même libellé peut aussi être celui d'un radio (« Comma » de la
        // décimale).
        let choix = peints
            .iter()
            .rfind(|texte| texte.texte == libelle && texte.centre != liste)
            .unwrap_or_else(|| panic!("{libelle:?} absent de la liste ouverte"))
            .centre;
        cliquer(context, choix, egui::PointerButton::Primary, |ui| {
            super::import_step_format(ui, state)
        });
    }
}
