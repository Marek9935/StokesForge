//! The windows of the templates of the import, nested over it: « Save as
//! Template » (a name and the conditions a file must meet, each one unchecked
//! to loosen it), « Import Templates » (the list, rename, conditions in plain
//! words, the answers remembered per kind of file, Forget Template…) and the
//! confirmation of the forgetting (a store change is not undoable, S-F3).
//! This file only paints [`AppState`]; the decisions are `import_template.rs`
//! of `stokesforge-app-state`.

use eframe::egui;
use stokesforge_app_state::state::{
    condition_sentence, AppState, AutoCondition, Fingerprint, TemplateRow,
};
use stokesforge_app_state::tr;
use stokesforge_import::{Condition, SaveIssue};

use crate::app::dialogs::{
    modal_confirm, modal_dialog_with, modal_live_settings, ConfirmOutcome, DialogFooter,
    DialogNature, DialogOutcome, InteractiveSize, PlainSize,
};
use crate::app::empty_state::empty_state;
use crate::app::form::{form, Form};
use crate::app::list_row::{with_list_row_height, ListRowBackground};
use crate::app::menu_sections::menu_choice;
use crate::icons::{icon_button, Icon, IconRole};
use crate::theme;

/// The windows that are open, painted after the wizard so they sit over it.
pub(super) fn windows(ctx: &egui::Context, state: &mut AppState) {
    // The confirmation first: it opens over the manager and must read Enter and
    // Escape before it.
    forget_window(ctx, state);
    save_window(ctx, state);
    manage_window(ctx, state);
}

fn save_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut draft) = state.template_save_draft().cloned() else {
        return;
    };
    let issue = state.template_save_issue();
    let replaces = state.template_save_replaces();
    let unchecked = state.to_check_count();
    let fp = state.import_fingerprint();
    let label = tr!("import-template-save-button");
    let hint = tr!("import-template-save-button", "hint");
    let reason = match issue {
        Some(SaveIssue::NoName) => tr!("import-template-save-reason-name"),
        Some(SaveIssue::NoCondition) => tr!("import-template-save-reason-trigger"),
        None => String::new(),
    };
    let outcome = modal_dialog_with(
        ctx,
        egui::Id::new("stokesforge_import_template_save"),
        &tr!("import-template-save-title"),
        DialogNature::Draft(InteractiveSize::Default),
        DialogFooter::gated(&label, &hint, issue.is_none(), &reason),
        |_| {},
        |ui| {
            form(ui, "stokesforge_import_template_save", |rows| {
                rows.row(tr!("import-template-save-name"), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut draft.name).desired_width(260.0));
                });
                if replaces {
                    rows.field(|ui| {
                        ui.label(
                            egui::RichText::new(tr!("import-template-save-replaces"))
                                .color(theme::warning_color(ui.visuals().dark_mode)),
                        );
                    });
                }
                if unchecked > 0 {
                    rows.field(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(tr!(
                                    "import-template-save-unchecked",
                                    count = unchecked
                                ))
                                .color(theme::warning_color(ui.visuals().dark_mode)),
                            )
                            .wrap(),
                        );
                    });
                }
                for warning in &draft.warnings {
                    rows.field(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(warning.clone())
                                    .color(theme::warning_color(ui.visuals().dark_mode)),
                            )
                            .wrap(),
                        );
                    });
                }
                rows.section(tr!("import-template-save-used"));
                for automatic in &mut draft.automatic {
                    rows.checkbox(&mut automatic.on, automatic.sentence.clone(), None);
                }
                rows.field(|ui| {
                    ui.label(theme::secondary_text(tr!("import-template-save-note")));
                });
                rows.section(tr!("import-condition-yours"));
                conditions_editor(rows, &mut draft.custom, fp.as_deref(), &[]);
                rows.section(tr!("import-template-save-sets"));
                for rule in &draft.rules {
                    rows.field(|ui| {
                        ui.add(egui::Label::new(format!("• {rule}")).wrap());
                    });
                }
            });
        },
    );
    if let Some(slot) = state.template_save_draft_mut() {
        *slot = draft;
    }
    match outcome {
        DialogOutcome::Open => {}
        DialogOutcome::Accept => state.template_save_commit(),
        DialogOutcome::Cancel => state.template_save_cancel(),
    }
}

fn manage_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(manage) = state.template_manage().cloned() else {
        return;
    };
    let rows = state.template_rows();
    let answers = state.remembered_answers();
    let fp = state.import_fingerprint();
    let selected: Option<&TemplateRow> = manage
        .selected
        .as_ref()
        .and_then(|name| rows.iter().find(|r| &r.name == name))
        .or_else(|| rows.first());
    let addable = selected
        .map(|s| state.template_addable_conditions(&s.name))
        .unwrap_or_default();
    let mut new_conditions: Option<Vec<Condition>> = None;
    let mut pick: Option<String> = None;
    let mut rename = manage.rename.clone();
    let mut commit_rename = false;
    let mut forget = false;
    let mut use_automatically = false;
    let mut forget_answer: Option<(String, String)> = None;
    let outcome = modal_live_settings(
        ctx,
        egui::Id::new("stokesforge_import_templates"),
        &tr!("import-template-manage-title"),
        InteractiveSize::Class(crate::app::dialogs::SizeClass::Large),
        |ui| {
            if rows.is_empty() && answers.is_empty() {
                empty_state(ui, tr!("import-template-manage-empty"));
                return;
            }
            ui.horizontal_top(|ui| {
                let height = ui.available_height();
                ui.allocate_ui_with_layout(
                    egui::vec2(200.0, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(200.0);
                        theme::page_scroll_area()
                            .id_salt("stokesforge_import_templates_list")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for row in &rows {
                                    let is_selected = selected.is_some_and(|s| s.name == row.name);
                                    let background = ListRowBackground::reserve(ui);
                                    let inner = with_list_row_height(ui, |ui| {
                                        let text = if is_selected {
                                            theme::bold(row.name.clone())
                                        } else {
                                            egui::RichText::new(row.name.clone())
                                        };
                                        ui.add(egui::Label::new(text).truncate())
                                    });
                                    let rect = inner.rect;
                                    let response = ui.interact(
                                        rect,
                                        egui::Id::new((
                                            "stokesforge_import_templates_row",
                                            &row.name,
                                        )),
                                        egui::Sense::click(),
                                    );
                                    background.paint(ui, rect, is_selected, response.has_focus());
                                    if crate::app::cursors::clickable(response).clicked() {
                                        pick = Some(row.name.clone());
                                    }
                                }
                            });
                    },
                );
                ui.vertical(|ui| {
                    // The long condition sentences wrap instead of being cut.
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    // RAW-SCROLL(#347): the editor and the remembered answers can be taller than the window.
                    theme::page_scroll_area()
                        .id_salt("stokesforge_import_templates_editor_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            form(ui, "stokesforge_import_templates_editor", |f| {
                                if let Some(current) = selected {
                                    f.row(tr!("import-template-save-name"), |ui| {
                                        let r = ui.add(
                                            egui::TextEdit::singleline(&mut rename)
                                                .desired_width(240.0),
                                        );
                                        // Enter renames and does not close the window.
                                        if r.lost_focus()
                                            && ui.input_mut(|i| {
                                                i.consume_shortcut(
                                                    &crate::app::shortcuts::KEY_ENTER,
                                                )
                                            })
                                        {
                                            commit_rename = true;
                                        }
                                    });
                                    if current.not_automatic {
                                        f.section(tr!("import-template-not-automatic"));
                                        f.field(|ui| {
                                            ui.add(
                                                egui::Label::new(theme::secondary_text(tr!(
                                                    "import-template-not-automatic-note"
                                                )))
                                                .wrap(),
                                            );
                                        });
                                        f.field(|ui| {
                                            if ui
                                                .button(tr!("import-template-use-automatically"))
                                                .on_hover_text(tr!(
                                                    "import-template-use-automatically",
                                                    "hint"
                                                ))
                                                .clicked()
                                            {
                                                use_automatically = true;
                                            }
                                        });
                                    }
                                    f.section(tr!("import-template-save-used"));
                                    let mut conditions: Vec<Condition> = current
                                        .conditions
                                        .iter()
                                        .map(|c| c.condition.clone())
                                        .collect();
                                    if conditions_editor(
                                        f,
                                        &mut conditions,
                                        fp.as_deref(),
                                        &addable,
                                    ) {
                                        new_conditions = Some(conditions);
                                    }
                                    f.section(tr!("import-template-save-sets"));
                                    for rule in &current.rules {
                                        f.field(|ui| {
                                            ui.add(egui::Label::new(format!("• {rule}")).wrap());
                                        });
                                    }
                                    f.section(tr!("import-template-manage-danger"));
                                    f.field(|ui| {
                                        ui.scope(|ui| {
                                            theme::destructive_button_visuals(ui.visuals_mut());
                                            let button = egui::Button::new(tr!(
                                                "import-template-forget-button"
                                            ));
                                            if ui
                                                .add(button)
                                                .on_hover_text(tr!(
                                                    "import-template-forget-button",
                                                    "hint"
                                                ))
                                                .clicked()
                                            {
                                                forget = true;
                                            }
                                        });
                                    });
                                }
                                remembered_section(f, &answers, &mut forget_answer);
                            });
                        });
                });
            });
        },
    );
    if let Some(name) = pick {
        state.template_manage_select(&name);
    }
    if let Some(current) = selected.map(|s| s.name.clone()) {
        if let Some(m) = state.template_manage_mut() {
            m.rename = rename;
        }
        if commit_rename {
            let new = state
                .template_manage()
                .map(|m| m.rename.clone())
                .unwrap_or_default();
            state.template_rename(&current, &new);
        }
        if let Some(conditions) = new_conditions {
            state.template_set_conditions(&current, conditions);
        }
        if use_automatically {
            state.template_use_automatically(&current);
        }
        if forget {
            state.template_forget_ask(&current);
        }
    }
    if let Some((kind, doubt)) = forget_answer {
        state.forget_remembered_answer(&kind, &doubt);
    }
    if outcome != DialogOutcome::Open {
        state.template_manage_close();
    }
}

/// The answers to doubts remembered per kind of file, each with « Forget »:
/// the doubt comes back for that kind of file the next time. Forgetting one
/// is not destructive (answering again restores it), so there is no
/// confirmation.
fn remembered_section(
    f: &mut Form<'_>,
    answers: &[stokesforge_app_state::state::RememberedAnswer],
    forget: &mut Option<(String, String)>,
) {
    f.section(tr!("import-answers-title"));
    if answers.is_empty() {
        f.field(|ui| {
            ui.label(theme::secondary_text(tr!("import-answers-empty")));
        });
        return;
    }
    f.field(|ui| {
        ui.label(theme::secondary_text(tr!("import-answers-note")));
    });
    let mut last_kind: Option<&str> = None;
    for answer in answers {
        if last_kind != Some(answer.kind.as_str()) {
            f.field(|ui| {
                ui.add(egui::Label::new(theme::bold(answer.kind_text.clone())).wrap());
            });
            last_kind = Some(answer.kind.as_str());
        }
        f.field(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("• {}", answer.doubt_text));
                if ui
                    .button(tr!("import-answers-forget"))
                    .on_hover_text(tr!("import-answers-forget", "hint"))
                    .clicked()
                {
                    *forget = Some((answer.kind.clone(), answer.doubt.clone()));
                }
            });
        });
    }
}

fn forget_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(name) = state.template_forget_pending().map(str::to_owned) else {
        return;
    };
    // The store change is not undoable: a confirmation (S-F3).
    match modal_confirm(
        ctx,
        egui::Id::new("stokesforge_import_template_forget"),
        &tr!("import-template-forget-title"),
        PlainSize::Default,
        &tr!("import-template-forget-yes"),
        &tr!("import-template-forget-yes", "hint"),
        |ui| {
            ui.add(
                egui::Label::new(tr!("import-template-forget-confirm", name = name.clone())).wrap(),
            );
        },
    ) {
        ConfirmOutcome::Open => {}
        ConfirmOutcome::Confirm => state.template_forget_confirm(),
        ConfirmOutcome::Cancel => state.template_forget_cancel(),
    }
}

// -- Conditions added by the user ---------------------------------------------

/// The conditions as editable rows (one per condition, each removable), then
/// « Add condition ». `addable` are the detector's conditions the list does not
/// carry yet; the other suggestions (number of columns, header lines, name and
/// extension) come from the file shown. Returns true when something changed.
fn conditions_editor(
    f: &mut Form<'_>,
    conditions: &mut Vec<Condition>,
    fp: Option<&Fingerprint>,
    addable: &[AutoCondition],
) -> bool {
    let mut changed = false;
    let mut remove: Option<usize> = None;
    for (i, cond) in conditions.iter_mut().enumerate() {
        f.field(|ui| {
            ui.horizontal_wrapped(|ui| {
                changed |= condition_row(ui, i, cond);
                if icon_button(
                    ui,
                    Icon::Close,
                    IconRole::Neutral,
                    tr!("import-condition-remove"),
                )
                .clicked()
                {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        conditions.remove(i);
        changed = true;
    }
    let mut added: Option<Condition> = None;
    f.field(|ui| {
        let button = caret_button(ui, tr!("import-condition-add"), Icon::CaretDown);
        egui::Popup::from_toggle_button_response(&button)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_width(260.0);
                for automatic in addable {
                    if menu_choice(ui, false, automatic.sentence.clone()).clicked() {
                        added = Some(automatic.condition.clone());
                        ui.close();
                    }
                }
                if !addable.is_empty() {
                    ui.separator();
                }
                let facts = fp.map(|f| &f.facts);
                let n_cols = facts.and_then(|f| f.n_cols).unwrap_or(2).max(1);
                if menu_choice(ui, false, tr!("import-condition-add-columns")).clicked() {
                    added = Some(Condition::Columns {
                        at_least: false,
                        n: n_cols,
                    });
                    ui.close();
                }
                let name = facts.map(|f| f.file_name.clone()).unwrap_or_default();
                let stem = match name.rsplit_once('.') {
                    Some((a, _)) => a.to_string(),
                    None => name.clone(),
                };
                let ext = facts.map(|f| f.extension.clone()).unwrap_or_default();
                if menu_choice(ui, false, tr!("import-condition-add-name")).clicked() {
                    added = Some(Condition::FileName { text: stem });
                    ui.close();
                }
                if menu_choice(ui, false, tr!("import-condition-add-ext")).clicked() {
                    added = Some(Condition::Extension { exts: vec![ext] });
                    ui.close();
                }
                if let Some(facts) = facts {
                    ui.separator();
                    ui.label(theme::dimmed_text(tr!("import-condition-pick-key")));
                    // RAW-SCROLL(#347): the header lines of the file, in a popup.
                    egui::ScrollArea::vertical()
                        .id_salt("stokesforge_import_cond_keys")
                        .max_height(240.0)
                        .show(ui, |ui| {
                            for (key, value) in &facts.keys {
                                let text = format!("{key} = {}", short(value, 28));
                                if menu_choice(ui, false, text).clicked() {
                                    added = Some(Condition::HeaderKey {
                                        key: key.clone(),
                                        value: value.clone(),
                                        contains: false,
                                    });
                                    ui.close();
                                }
                            }
                        });
                }
            });
    });
    if let Some(c) = added {
        conditions.push(c);
        changed = true;
    }
    changed
}

fn short(text: &str, chars: usize) -> String {
    if text.chars().count() <= chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// One condition as editable words: « Line DATA TYPE [equals ▾] [value] ».
fn condition_row(ui: &mut egui::Ui, index: usize, cond: &mut Condition) -> bool {
    let mut changed = false;
    match cond {
        Condition::Columns { at_least, n } => {
            let mut text = if *at_least {
                tr!("import-condition-atleast")
            } else {
                tr!("import-condition-equals")
            };
            ui.label(theme::bold(tr!("import-condition-add-columns")));
            egui::ComboBox::from_id_salt(("stokesforge_cond_mode", index))
                .selected_text(std::mem::take(&mut text))
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(at_least, false, tr!("import-condition-equals"))
                        .changed();
                    changed |= ui
                        .selectable_value(at_least, true, tr!("import-condition-atleast"))
                        .changed();
                });
            let before = *n;
            crate::number_field::number_field(
                ui,
                ("stokesforge_cond_columns", index),
                n,
                &stokesforge_app_state::quantities::COUNT,
            );
            if *n == 0 {
                *n = 1;
            }
            changed |= *n != before;
            ui.label(tr!("import-condition-columns-unit"));
        }
        Condition::HeaderKey {
            key,
            value,
            contains,
        } => {
            ui.label(theme::bold(format!(
                "{} {key}",
                tr!("import-condition-key-is")
            )));
            egui::ComboBox::from_id_salt(("stokesforge_cond_mode", index))
                .selected_text(if *contains {
                    tr!("import-condition-contains")
                } else {
                    tr!("import-condition-equals")
                })
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(contains, false, tr!("import-condition-equals"))
                        .changed();
                    changed |= ui
                        .selectable_value(contains, true, tr!("import-condition-contains"))
                        .changed();
                });
            changed |= ui
                .add(egui::TextEdit::singleline(value).desired_width(150.0))
                .changed();
        }
        Condition::FileName { text } => {
            ui.label(theme::bold(tr!("import-condition-name-is")));
            changed |= ui
                .add(egui::TextEdit::singleline(text).desired_width(180.0))
                .changed();
        }
        Condition::Extension { exts } => {
            ui.label(theme::bold(format!("{} .", tr!("import-condition-ext-is"))));
            // Several extensions are typed with commas: « csv, txt ».
            let mut text = exts.join(", ");
            if ui
                .add(egui::TextEdit::singleline(&mut text).desired_width(100.0))
                .changed()
            {
                *exts = text
                    .split(',')
                    .map(|e| e.trim().trim_start_matches('.').to_string())
                    .collect();
                changed = true;
            }
        }
        // The detector's own conditions read as their sentence; remove and
        // re-add them from « Add condition ».
        other => {
            ui.add(egui::Label::new(condition_sentence(other)).wrap());
        }
    }
    changed
}

/// A standard button whose label ends with a small caret icon: it opens a menu.
pub(super) fn caret_button(ui: &mut egui::Ui, text: String, caret: Icon) -> egui::Response {
    let response = ui.add(egui::Button::new(format!("{text}\u{2003}\u{2002}")));
    let size = crate::icons::ICON_SIZE;
    let rect = egui::Rect::from_center_size(
        egui::pos2(response.rect.right() - size, response.rect.center().y),
        egui::vec2(size, size),
    );
    crate::icons::icon_image(caret, IconRole::Neutral).paint_at(ui, rect);
    crate::app::cursors::clickable(response)
}
