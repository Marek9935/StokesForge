//! Assistant d'import (#2, #3–#8) : les six étapes (fichiers, format, en-têtes,
//! colonnes, métadonnées, destination), l'aperçu en lignes colorées par rôle
//! et les libellés des choix. Un seul point d'entrée pour `app.rs` :
//! [`import_window`], un `modal_dialog_with` (jamais d'`egui::Window` brute).
//!
//! Sorti d'`app.rs` tel quel (#118).

use eframe::egui;
use stokesforge_import::{DecimalStyle, HeaderSkip, NameSource, Separator};

use crate::icons::{icon_button, Icon, IconRole};
use crate::state::{Action, AppState, ImportDestination, ImportLineAction, ImportStep};
use crate::tr;

use super::dialogs::{modal_dialog_with, DialogFooter, DialogOutcome};

/// Réglage « Fin des données » de la recette d'import (issue #4) : coupure
/// automatique par défaut, désactivable. Le texte ou le nombre associé à
/// l'option choisie reste dans la recette tant qu'on en change.
fn data_end_row(ui: &mut egui::Ui, end: &mut stokesforge_import::DataEnd) {
    use stokesforge_import::DataEnd;
    let label = |end: &DataEnd| match end {
        DataEnd::Auto => tr!("import-dataend-auto"),
        DataEnd::None => tr!("import-option-none"),
        DataEnd::FirstBlankLine => tr!("import-dataend-blank"),
        DataEnd::Marker(_) => tr!("import-dataend-marker"),
        DataEnd::LinesAfterHeader(_) => tr!("import-dataend-after-header"),
        DataEnd::LinesBeforeEnd(_) => tr!("import-dataend-before-end"),
    };
    ui.horizontal(|ui| {
        ui.label(tr!("import-dataend"));
        egui::ComboBox::from_id_salt("stokesforge_import_data_end")
            .selected_text(label(end))
            .show_ui(ui, |ui| {
                for candidate in [
                    DataEnd::Auto,
                    DataEnd::None,
                    DataEnd::FirstBlankLine,
                    DataEnd::Marker(String::new()),
                    DataEnd::LinesAfterHeader(100),
                    DataEnd::LinesBeforeEnd(1),
                ] {
                    let same = std::mem::discriminant(&*end) == std::mem::discriminant(&candidate);
                    let mut item = ui.selectable_label(same, label(&candidate));
                    if matches!(candidate, DataEnd::Auto) {
                        item = item.on_hover_text(tr!("import-dataend-auto", "hint"));
                    }
                    if item.clicked() && !same {
                        *end = candidate;
                    }
                }
            });
        match end {
            DataEnd::Marker(text) => {
                ui.text_edit_singleline(text);
            }
            DataEnd::LinesAfterHeader(n) | DataEnd::LinesBeforeEnd(n) => {
                let mut value = *n as i64;
                if ui
                    .add(egui::DragValue::new(&mut value).range(0..=i64::MAX))
                    .changed()
                {
                    *n = value.max(0) as usize;
                }
            }
            _ => {}
        }
    });
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
            if ui
                .checkbox(&mut enabled, label)
                .on_hover_text(hint)
                .changed()
            {
                *range = if enabled {
                    Some(stokesforge_import::SubheaderLineRange::single(1))
                } else {
                    None
                };
            }
            if let Some(r) = range {
                ui.label(tr!("import-header-range-from"));
                let mut from = r.from as i64;
                if ui
                    .add(egui::DragValue::new(&mut from).range(1..=i64::MAX))
                    .changed()
                {
                    r.from = from.max(1) as usize;
                }
                ui.label(tr!("import-header-range-to"));
                let mut to = r.to as i64;
                if ui
                    .add(egui::DragValue::new(&mut to).range(1..=i64::MAX))
                    .changed()
                {
                    r.to = to.max(1) as usize;
                }
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
        | LineRole::ColumnHeaderRow => egui::Color32::from_rgb(80, 140, 220),
        LineRole::SubheaderComment => egui::Color32::from_rgb(200, 80, 200),
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
        ui.strong(tr!("import-preview-title"));
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
            ui.colored_label(egui::Color32::RED, message);
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
}

fn recipe_combo_text(name: &str, modified: bool) -> String {
    if modified {
        tr!("import-files-template-modified", name = name)
    } else {
        name.to_string()
    }
}

/// Étape 1 : fichiers et modèle.
fn import_step_files(ui: &mut egui::Ui, state: &mut AppState) {
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
    ui.horizontal(|ui| {
        ui.label(tr!("import-files-template"));
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
            .clicked()
        {
            state.request_delete_import_recipe(&current_name);
        }
    });
    ui.horizontal(|ui| {
        if ui
            .button(tr!("import-files-export"))
            .on_hover_text(tr!("import-files-export", "hint"))
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new()
                .set_title(tr!("import-files-export-title"))
                .add_filter(tr!("import-files-template-filter"), &["json"])
                .set_file_name(format!("{current_name}.json"))
                .save_file()
            {
                state.export_import_recipe(&path);
            }
        }
        if ui
            .button(tr!("import-files-import"))
            .on_hover_text(tr!("import-files-import", "hint"))
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new()
                .set_title(tr!("import-files-import-title"))
                .add_filter(tr!("import-files-template-filter"), &["json"])
                .pick_file()
            {
                state.import_import_recipe(&path);
            }
        }
    });
    if modified {
        ui.weak(tr!("import-files-unsaved"));
    }
    if let Some(name) = confirm {
        ui.horizontal(|ui| {
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
    ui.separator();
    ui.horizontal(|ui| {
        if ui
            .button(tr!("import-files-add"))
            .on_hover_text(tr!("import-files-add", "hint"))
            .clicked()
        {
            let mut dialog = rfd::FileDialog::new().set_title(tr!("import-files-add-title"));
            // Le dossier du dernier fichier importé.
            if let Some(dir) = state
                .import_state()
                .and_then(|import| import.recent.last_dir().map(std::path::Path::to_path_buf))
            {
                dialog = dialog.set_directory(dir);
            }
            if let Some(paths) = dialog.pick_files() {
                state.add_import_paths(paths);
            }
        }
        ui.weak(tr!("import-files-drop"));
    });
    let recent: Vec<std::path::PathBuf> = state
        .import_state()
        .map(|import| import.recent.files().to_vec())
        .unwrap_or_default();
    if !recent.is_empty() {
        let mut picked = None;
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
                    ui.colored_label(egui::Color32::RED, message);
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
    ui.horizontal(|ui| {
        ui.label(tr!("import-format-delimiter"))
            .on_hover_text(tr!("import-format-delimiter", "hint"));
        egui::ComboBox::from_id_salt("stokesforge_import_separator_combo")
            .selected_text(separator_label(&recipe.separator))
            .show_ui(ui, |ui| {
                for sep in [
                    Separator::Comma,
                    Separator::Semicolon,
                    Separator::Tab,
                    Separator::Whitespace,
                    Separator::Custom('|'),
                    Separator::FixedWidth(vec![10]),
                    Separator::Auto,
                ] {
                    let same =
                        std::mem::discriminant(&recipe.separator) == std::mem::discriminant(&sep);
                    if ui.selectable_label(same, separator_label(&sep)).clicked() && !same {
                        recipe.separator = sep;
                    }
                }
            });
        match &mut recipe.separator {
            Separator::Custom(c) => {
                let mut text = c.to_string();
                if ui
                    .add(egui::TextEdit::singleline(&mut text).desired_width(24.0))
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
                if ui
                    .add(egui::TextEdit::singleline(&mut text).desired_width(120.0))
                    .on_hover_text(tr!("import-format-widths-hint"))
                    .changed()
                {
                    *widths = text
                        .split(',')
                        .filter_map(|part| part.trim().parse::<usize>().ok())
                        .collect();
                }
            }
            _ => {}
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr!("import-format-decimal"));
        egui::ComboBox::from_id_salt("stokesforge_import_decimal_combo")
            .selected_text(decimal_label(recipe.decimal))
            .show_ui(ui, |ui| {
                for dec in [DecimalStyle::Auto, DecimalStyle::Dot, DecimalStyle::Comma] {
                    if ui
                        .selectable_label(recipe.decimal == dec, decimal_label(dec))
                        .clicked()
                    {
                        recipe.decimal = dec;
                    }
                }
            });
        ui.label(tr!("import-format-thousands"));
        egui::ComboBox::from_id_salt("stokesforge_import_thousands_combo")
            .selected_text(thousands_label(recipe.thousands))
            .show_ui(ui, |ui| {
                for candidate in [None, Some(' '), Some(','), Some('.'), Some('\'')] {
                    if ui
                        .selectable_label(recipe.thousands == candidate, thousands_label(candidate))
                        .clicked()
                    {
                        recipe.thousands = candidate;
                    }
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label(tr!("import-format-missing"));
        let mut text = recipe.missing_tokens.join(", ");
        if ui
            .add(egui::TextEdit::singleline(&mut text).desired_width(140.0))
            .on_hover_text(tr!("import-format-missing", "hint"))
            .changed()
        {
            recipe.missing_tokens = text
                .split(',')
                .map(|token| token.trim().to_string())
                .filter(|token| !token.is_empty())
                .collect();
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr!("import-format-comment-prefix"));
        ui.add(egui::TextEdit::singleline(&mut recipe.comment_prefix).desired_width(60.0))
            .on_hover_text(tr!("import-format-comment-prefix", "hint"));
    });
    ui.horizontal(|ui| {
        use stokesforge_import::BlockSplit;
        ui.label(tr!("import-format-blocks"))
            .on_hover_text(tr!("import-format-blocks", "hint"));
        let current = match &recipe.block_split {
            BlockSplit::None => tr!("import-blocks-single"),
            BlockSplit::BeforeLine(_) => tr!("import-blocks-line-start"),
            BlockSplit::BlankLines => tr!("import-blocks-blank"),
        };
        egui::ComboBox::from_id_salt("stokesforge_import_blocks_combo")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (candidate, label) in [
                    (BlockSplit::None, tr!("import-blocks-single")),
                    (
                        BlockSplit::BeforeLine("TITLE".into()),
                        tr!("import-blocks-line-start"),
                    ),
                    (BlockSplit::BlankLines, tr!("import-blocks-blank")),
                ] {
                    let same = std::mem::discriminant(&recipe.block_split)
                        == std::mem::discriminant(&candidate);
                    if ui.selectable_label(same, label).clicked() && !same {
                        recipe.block_split = candidate;
                    }
                }
            });
        if let BlockSplit::BeforeLine(marker) = &mut recipe.block_split {
            ui.add(egui::TextEdit::singleline(marker).desired_width(80.0));
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr!("import-format-encoding"));
        egui::ComboBox::from_id_salt("stokesforge_import_encoding_combo")
            .selected_text(encoding_label(recipe.encoding))
            .show_ui(ui, |ui| {
                use stokesforge_import::Encoding;
                for candidate in [
                    Encoding::Auto,
                    Encoding::Utf8,
                    Encoding::Windows1252,
                    Encoding::Latin1,
                    Encoding::Utf16Le,
                    Encoding::Utf16Be,
                ] {
                    if ui
                        .selectable_label(recipe.encoding == candidate, encoding_label(candidate))
                        .clicked()
                    {
                        recipe.encoding = candidate;
                    }
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label(tr!("import-format-skip-chars"))
            .on_hover_text(tr!("import-format-skip-chars", "hint"));
        let mut value = recipe.skip_chars as i64;
        if ui
            .add(egui::DragValue::new(&mut value).range(0..=i64::MAX))
            .changed()
        {
            recipe.skip_chars = value.max(0) as usize;
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr!("import-format-skip-lines"))
            .on_hover_text(tr!("import-format-skip-lines", "hint"));
        let mut value = recipe.skip_data_rows as i64;
        if ui
            .add(egui::DragValue::new(&mut value).range(0..=i64::MAX))
            .changed()
        {
            recipe.skip_data_rows = value.max(0) as usize;
        }
    });
}

/// Étape 3 : en-tête (un seul réglage) et fin des données.
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
    // Comme `parse` : le découpage structuré ne compte que s'il a des lignes
    // (l'ancien drapeau `auto_detect` seul ne suffit pas).
    let structured =
        recipe.header_layout.main_header_lines > 0 || recipe.header_layout.subheader_lines > 0;
    let is_marker = matches!(recipe.header, HeaderSkip::UntilMarker(_));
    ui.horizontal(|ui| {
        ui.label(tr!("import-header-mode"));
        if ui
            .radio(!structured && !is_marker, tr!("import-option-none"))
            .clicked()
        {
            recipe.header = HeaderSkip::None;
            recipe.header_layout = stokesforge_import::HeaderLayout::default();
        }
        if ui
            .radio(structured, tr!("import-header-mode-lines"))
            .clicked()
            && !structured
        {
            recipe.header = HeaderSkip::None;
            recipe.header_layout.main_header_lines = 1;
        }
        if ui
            .radio(is_marker, tr!("import-header-mode-marker"))
            .clicked()
            && !is_marker
        {
            recipe.header_layout = stokesforge_import::HeaderLayout::default();
            recipe.header = HeaderSkip::UntilMarker(String::new());
        }
    });
    if let HeaderSkip::UntilMarker(marker) = &mut recipe.header {
        ui.horizontal(|ui| {
            ui.label(tr!("import-header-marker"))
                .on_hover_text(tr!("import-header-marker", "hint"));
            ui.text_edit_singleline(marker);
        });
    }
    if structured {
        ui.horizontal(|ui| {
            ui.label(tr!("import-header-lines"));
            let mut value = recipe.header_layout.main_header_lines as i64;
            if ui
                .add(egui::DragValue::new(&mut value).range(0..=i64::MAX))
                .changed()
            {
                recipe.header_layout.main_header_lines = value.max(0) as usize;
            }
            ui.label(tr!("import-header-subheader-lines"))
                .on_hover_text(tr!("import-header-subheader-lines", "hint"));
            let mut value = recipe.header_layout.subheader_lines as i64;
            if ui
                .add(egui::DragValue::new(&mut value).range(0..=i64::MAX))
                .changed()
            {
                recipe.header_layout.subheader_lines = value.max(0) as usize;
            }
        });
    }
    if ui
        .add_enabled(has_files, egui::Button::new(tr!("import-header-detect")))
        .on_hover_text(tr!("import-header-detect", "hint"))
        .clicked()
    {
        requests.auto_detect = true;
    }
    if recipe.header_layout.subheader_lines > 0 {
        ui.checkbox(
            &mut recipe.subheader_roles.same_line_name_unit,
            tr!("import-header-combined"),
        )
        .on_hover_text(tr!("import-header-combined", "hint"));
        subheader_role_row(
            ui,
            "stokesforge_import_role_long_name",
            tr!("import-header-role-long-name"),
            tr!("import-header-role-long-name", "hint"),
            &mut recipe.subheader_roles.long_name,
        );
        if !recipe.subheader_roles.same_line_name_unit {
            subheader_role_row(
                ui,
                "stokesforge_import_role_unit",
                tr!("import-header-role-units"),
                tr!("import-header-role-units", "hint"),
                &mut recipe.subheader_roles.unit,
            );
        }
        subheader_role_row(
            ui,
            "stokesforge_import_role_comment",
            tr!("import-header-role-comments"),
            tr!("import-header-role-comments", "hint"),
            &mut recipe.subheader_roles.comment,
        );
    }
    ui.checkbox(
        &mut recipe.has_column_header_row,
        tr!("import-header-names-first-line"),
    )
    .on_hover_text(tr!("import-header-names-first-line", "hint"));
    ui.separator();
    data_end_row(ui, &mut recipe.data_end);
    ui.horizontal(|ui| {
        ui.label(tr!("import-header-skip-footer"))
            .on_hover_text(tr!("import-header-skip-footer", "hint"));
        let mut value = recipe.footer_skip_lines as i64;
        if ui
            .add(egui::DragValue::new(&mut value).range(0..=i64::MAX))
            .changed()
        {
            recipe.footer_skip_lines = value.max(0) as usize;
        }
    });
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(tr!("import-column-names"));
        let mut is_filename = matches!(recipe.name_source, NameSource::Filename);
        let mut is_header_key = matches!(recipe.name_source, NameSource::HeaderKey(_));
        if ui
            .radio_value(&mut is_filename, true, tr!("import-header-name-file"))
            .clicked()
        {
            recipe.name_source = NameSource::Filename;
        }
        if ui
            .radio_value(&mut is_header_key, true, tr!("import-header-name-key"))
            .clicked()
        {
            recipe.name_source = NameSource::HeaderKey(String::new());
        }
    });
    if let NameSource::HeaderKey(key) = &mut recipe.name_source {
        ui.horizontal(|ui| {
            ui.label(tr!("import-header-key"));
            ui.text_edit_singleline(key);
        });
    }
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
            ui.colored_label(egui::Color32::RED, message);
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
                        ui.strong(title);
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
    ui.checkbox(
        &mut recipe.keep_header_metadata,
        tr!("import-metadata-keep-header"),
    )
    .on_hover_text(tr!("import-metadata-keep-header", "hint"));
    ui.checkbox(
        &mut recipe.keep_footer_metadata,
        tr!("import-metadata-keep-footer"),
    )
    .on_hover_text(tr!("import-metadata-keep-footer", "hint"));
    ui.weak(tr!("import-metadata-uncheck"));
    ui.separator();
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
                    egui::RichText::new(&line.key).strong()
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
    ui.label(tr!("import-destination"));
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
        );
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
    if matches!(import.destination, ImportDestination::ActiveBook(_)) {
        ui.separator();
        ui.label(tr!("import-destination-mode"));
        use stokesforge_import::ImportMode;
        for (mode, label) in [
            (ImportMode::NewSheet, tr!("import-mode-new-sheet")),
            (ImportMode::ReplaceSheet, tr!("import-mode-replace")),
            (ImportMode::AddColumns, tr!("import-mode-add-columns")),
            (ImportMode::AddRows, tr!("import-mode-add-rows")),
        ] {
            ui.radio_value(&mut import.recipe.import_mode, mode, label);
        }
    }
    if import.paths.len() > 1 {
        ui.separator();
        ui.label(tr!("import-destination-multiple"));
        use stokesforge_import::FilesLayout;
        for (layout, label) in [
            (FilesLayout::SheetPerFile, tr!("import-layout-per-file")),
            (FilesLayout::SideBySide, tr!("import-layout-side-by-side")),
            (FilesLayout::SharedX, tr!("import-layout-shared-x")),
        ] {
            ui.radio_value(&mut import.recipe.files_layout, layout, label);
        }
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(tr!("import-destination-template-name"));
        ui.text_edit_singleline(&mut import.save_as_name);
        if ui.button(tr!("import-destination-save")).clicked() {
            requests.save_recipe = true;
        }
    });
    ui.weak(tr!("import-destination-saved-note"));
    ui.separator();
    ui.label(tr!("import-destination-auto"))
        .on_hover_text(tr!("import-destination-auto", "hint"));
    let auto = &mut import.recipe.auto_match;
    ui.horizontal(|ui| {
        ui.label(tr!("import-auto-extensions"));
        let mut text = auto.extensions.join(", ");
        if ui
            .add(egui::TextEdit::singleline(&mut text).desired_width(120.0))
            .on_hover_text(tr!("import-auto-extensions", "hint"))
            .changed()
        {
            auto.extensions = text
                .split(',')
                .map(|e| e.trim().trim_start_matches('.').to_string())
                .filter(|e| !e.is_empty())
                .collect();
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr!("import-auto-name"));
        ui.add(egui::TextEdit::singleline(&mut auto.name_contains).desired_width(120.0));
    });
    ui.horizontal(|ui| {
        ui.label(tr!("import-auto-content"));
        ui.add(egui::TextEdit::singleline(&mut auto.content_contains).desired_width(160.0))
            .on_hover_text(tr!("import-auto-content", "hint"));
    });
}

/// L'assistant d'import (« Importer… », menu Fichier) en six étapes façon
/// assistant ASCII d'Origin : fichiers et modèle, format, en-tête et fin des
/// données, colonnes, métadonnées, destination. L'aperçu du fichier reste
/// visible à droite à toutes les étapes. « Terminer » importe dès que des
/// fichiers sont choisis, avec le modèle tel qu'il est — import en deux clics
/// pour un format connu. **Dialogue modal de la brique commune**
/// ([`modal_dialog_with`], comme les propriétés du graphe), peint depuis
/// [`StokesForgeApp::ui`] tant que [`AppState::import_state`] est `Some` ; la croix
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
    let outcome = modal_dialog_with(
        ctx,
        egui::Id::new("stokesforge_import_wizard"),
        &tr!("import-window-title"),
        egui::vec2(1000.0, 620.0),
        DialogFooter {
            accept_label: &finish_label,
            accept_hint: &finish_hint,
            accept_enabled: has_files,
        },
        |footer| {
            // Ajoutés de droite à gauche : « Next » puis « Back ».
            if footer
                .add_enabled(
                    step != ImportStep::Destination,
                    egui::Button::new(tr!("import-button-next")).min_size(egui::vec2(84.0, 24.0)),
                )
                .clicked()
            {
                requests.borrow_mut().forward = true;
            }
            if footer
                .add_enabled(
                    step != ImportStep::Files,
                    egui::Button::new(tr!("import-button-back")).min_size(egui::vec2(84.0, 24.0)),
                )
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
                        ui.strong(text);
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
                        ImportStep::Files => import_step_files(ui, state),
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
    let mut requests = requests.into_inner();
    match outcome {
        DialogOutcome::Open => {}
        DialogOutcome::Accept => requests.import = true,
        DialogOutcome::Cancel => requests.cancel = true,
    }

    // Tout ce qui demande l'état entier est appliqué ici, une fois la fenêtre
    // peinte.
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
