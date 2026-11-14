//! The one screen of the import: the
//! file tabs, the recognition line, the left panel (40 % of the width, at
//! least 380 pt), the grid with its File | Result tabs, the « You get » line,
//! and the two footer buttons (Destination, Template).

use eframe::egui;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense};
use stokesforge_app_state::errors;
use stokesforge_app_state::state::{Action, AppState, ImportDestination, ShownFileStatus};
use stokesforge_app_state::tr;
use stokesforge_import::ImportRecipe;

use super::check::{self, CheckCmd};
use super::guide;
use super::panel::{self, Left};
use super::{
    action_bar, build_bands, build_rows, file_grid, fold_range, import_result_table, load_gs,
    make_geom, metrics, refused_card, store_gs, GridOut, GridUi, Sel, View, LETTER_H, SB,
};
use crate::app::cursors;
use crate::app::import_wizard::ImportWizardRequests;
use crate::app::menu_sections::{menu_choice, MenuSections};
use crate::app::tab_strip::{tab_strip, TabStripSide, TAB_STRIP_HEIGHT};
use crate::icons::{icon_button, icon_image, Icon, IconRole};
use crate::theme;

/// The left panel takes 35 % of the width, but never less than this.
const LEFT_MIN_W: f32 = 380.0;
/// The grid keeps at least this much, even in a small window.
const GRID_MIN_W: f32 = 340.0;
/// Height of the « You get » line.
const RECEIPT_H: f32 = 22.0;

// -- File tabs -----------------------------------------------------------------

/// The tabs of the files (one `tab_strip`), the state of the file shown, and
/// the « Add Files… » button.
fn file_row(ui: &mut egui::Ui, state: &mut AppState, requests: &mut ImportWizardRequests) {
    let Some(import) = state.import_state() else {
        return;
    };
    let marks = state.import_file_marks();
    let tabs: Vec<(usize, String)> = import
        .paths
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();
            match marks.get(i).and_then(|m| m.refusal) {
                Some(_) => format!("{name} — {}", tr!("import-status-refused")),
                None => name,
            }
        })
        .enumerate()
        .collect();
    let current = import.preview_index.min(tabs.len().saturating_sub(1));
    let many = tabs.len() > 1;
    let status = state.shown_file_status();
    let mut picked = None;
    let mut removed = false;
    ui.horizontal(|ui| {
        // The room the end of the row needs: the status of the file, the
        // button to remove it and « Add Files… », measured in the language
        // in use (a longer text must not push the button out of the window).
        let measure = |text: &str| {
            let font = egui::TextStyle::Body.resolve(ui.style());
            ui.painter()
                .layout_no_wrap(text.to_string(), font, ui.visuals().text_color())
                .size()
                .x
        };
        let status_text = match status {
            ShownFileStatus::Refused => tr!("import-status-refused"),
            ShownFileStatus::Check(count) => tr!("import-status-check", count = count),
            ShownFileStatus::Ready => tr!("import-status-ready"),
        };
        let reserve = measure(&tr!("import-files-add-files"))
            + measure(&status_text)
            + if many { 2.0 * 24.0 } else { 24.0 }
            + 60.0;
        let width = (ui.available_width() - reserve).max(160.0);
        // A few files: all the tabs. Too many to fit: the first ones, the one
        // shown, and « + N more » that lists every file with its status.
        let font = egui::TextStyle::Body.resolve(ui.style());
        let color = ui.visuals().text_color();
        let widths: Vec<f32> = tabs
            .iter()
            .map(|(_, t)| {
                ui.painter()
                    .layout_no_wrap(t.clone(), font.clone(), color)
                    .size()
                    .x
                    + 26.0
            })
            .collect();
        let overflow = widths.iter().sum::<f32>() > width;
        let nav_w = 2.0 * 24.0;
        let more_w = 110.0;
        let (shown_tabs, hidden) = if overflow {
            let budget = (width - nav_w - more_w).max(80.0);
            let mut vis: Vec<usize> = Vec::new();
            let mut used = 0.0;
            for (i, w) in widths.iter().enumerate() {
                if used + w > budget {
                    break;
                }
                vis.push(i);
                used += w;
            }
            if vis.is_empty() {
                vis.push(0);
            }
            if !vis.contains(&current) {
                if let Some(last) = vis.last_mut() {
                    *last = current;
                }
            }
            let shown: Vec<(usize, String)> = vis.iter().map(|&i| tabs[i].clone()).collect();
            let hidden = tabs.len() - shown.len();
            (shown, hidden)
        } else {
            (tabs.clone(), 0)
        };
        if overflow {
            if icon_button(
                ui,
                Icon::CaretLeft,
                IconRole::Neutral,
                tr!("import-files-prev"),
            )
            .clicked()
                && current > 0
            {
                picked = Some(current - 1);
            }
            if icon_button(
                ui,
                Icon::CaretRight,
                IconRole::Neutral,
                tr!("import-files-next"),
            )
            .clicked()
                && current + 1 < tabs.len()
            {
                picked = Some(current + 1);
            }
        }
        let strip_w = if overflow {
            (width - nav_w - more_w).max(80.0)
        } else {
            width
        };
        ui.allocate_ui_with_layout(
            vec2(strip_w, TAB_STRIP_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                // Two strips live on this screen: tell their scroll areas apart.
                ui.push_id("stokesforge_import_file_tabs", |ui| {
                    tab_strip(
                        ui,
                        TabStripSide::Top,
                        &shown_tabs,
                        current,
                        |i| picked = Some(i),
                        None,
                        || {},
                    );
                });
            },
        );
        if overflow {
            let more = crate::app::import_wizard::templates::caret_button(
                ui,
                tr!("import-files-more", count = hidden),
                Icon::CaretDown,
            )
            .on_hover_text(tr!("import-files-more", "hint"));
            egui::Popup::from_toggle_button_response(&more)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    if let Some(i) = file_list(ui, &tabs, &marks, current) {
                        picked = Some(i);
                        ui.close();
                    }
                });
        }
        // The state of the file shown, by its icon.
        if status == ShownFileStatus::Refused {
            ui.add(icon_image(Icon::Close, IconRole::Danger));
            ui.label(
                egui::RichText::new(tr!("import-status-refused"))
                    .color(ui.visuals().error_fg_color),
            );
        } else if let ShownFileStatus::Check(doubts) = status {
            ui.add(icon_image(Icon::Stale, IconRole::Neutral));
            ui.label(
                egui::RichText::new(tr!("import-status-check", count = doubts))
                    .color(theme::warning_color(ui.visuals().dark_mode)),
            );
        } else {
            ui.add(icon_image(Icon::Check, IconRole::Neutral).tint(theme::help_icon_tint()));
            ui.label(theme::dimmed_text(tr!("import-status-ready")));
        }
        if many
            && icon_button(
                ui,
                Icon::Close,
                IconRole::Neutral,
                tr!("import-files-remove"),
            )
            .clicked()
        {
            removed = true;
        }
        let add = ui
            .add(egui::Button::new(tr!("import-files-add-files")))
            .on_hover_text(tr!("import-files-add-files", "hint"));
        if cursors::clickable(add).clicked() {
            requests.picks.push(Action::PickFile(
                stokesforge_app_state::file_picker::PickRequest::AddImportFiles,
            ));
        }
    });
    if let Some(i) = picked {
        state.set_import_preview_index(i);
    }
    if removed {
        state.remove_import_path(current);
    }
}

/// The list of every file, grouped by kind, with a filter: ✔ same kind as
/// the file shown, ⚠ different kind, ✖ refused. Returns the file picked.
fn file_list(
    ui: &mut egui::Ui,
    tabs: &[(usize, String)],
    marks: &[stokesforge_app_state::state::FileMark],
    current: usize,
) -> Option<usize> {
    let filter_id = egui::Id::new("stokesforge_import_files_filter");
    let mut filter: String = ui
        .data(|d| d.get_temp::<String>(filter_id))
        .unwrap_or_default();
    ui.set_width(380.0);
    let edit = ui.add(
        egui::TextEdit::singleline(&mut filter)
            .hint_text(tr!("import-files-filter"))
            .desired_width(f32::INFINITY),
    );
    if !edit.has_focus() && filter.is_empty() {
        edit.request_focus();
    }
    ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
    let needle = filter.to_lowercase();
    let mut picked = None;
    let groups = [
        (Icon::Check, tr!("import-files-same"), 0u8),
        (Icon::Stale, tr!("import-files-other"), 1),
        (Icon::Close, tr!("import-files-refused"), 2),
    ];
    let group_of = |i: usize| -> u8 {
        match marks.get(i) {
            Some(m) if m.refusal.is_some() => 2,
            Some(m) if m.same_kind || m.shown => 0,
            _ => 1,
        }
    };
    let mut any = false;
    // RAW-SCROLL(#347): the list of the files, in a popup.
    egui::ScrollArea::vertical()
        .id_salt("stokesforge_import_files_list")
        .max_height(320.0)
        .show(ui, |ui| {
            for (icon, title, group) in groups {
                let members: Vec<&(usize, String)> = tabs
                    .iter()
                    .filter(|(i, name)| {
                        group_of(*i) == group && name.to_lowercase().contains(&needle)
                    })
                    .collect();
                if members.is_empty() {
                    continue;
                }
                any = true;
                ui.label(theme::dimmed_text(format!("{title} ({})", members.len())));
                for (i, name) in members {
                    ui.horizontal(|ui| {
                        let role = if group == 2 {
                            IconRole::Danger
                        } else {
                            IconRole::Neutral
                        };
                        ui.add(icon_image(icon, role));
                        if crate::app::menu_sections::menu_choice(ui, *i == current, name.clone())
                            .clicked()
                        {
                            picked = Some(*i);
                        }
                    });
                }
            }
            if !any {
                ui.label(theme::dimmed_text(tr!("import-files-none")));
            }
        });
    if picked.is_some() {
        ui.data_mut(|d| d.insert_temp(filter_id, String::new()));
    }
    picked
}

// -- File | Result tabs, fold, help ------------------------------------------------------

fn grid_tab_row(
    ui: &mut egui::Ui,
    state: &mut AppState,
    rect: Rect,
    gs: &mut GridUi,
    fold_possible: bool,
) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let tabs = [
        (false, tr!("import-grid-view-file")),
        (true, tr!("import-grid-view-result")),
    ];
    let mut result_view = gs.result_view;
    child.allocate_ui_with_layout(
        vec2(160.0, TAB_STRIP_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.push_id("stokesforge_import_view_tabs", |ui| {
                tab_strip(
                    ui,
                    TabStripSide::Top,
                    &tabs,
                    gs.result_view,
                    |v| result_view = v,
                    None,
                    || {},
                );
            });
        },
    );
    gs.result_view = result_view;
    child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui
            .button(tr!("import-guide-show"))
            .on_hover_text(tr!("import-guide-show", "hint"))
            .clicked()
        {
            state.import_guide_show();
            guide::restart(gs);
        }
        if fold_possible && !gs.result_view {
            let label = if gs.unfolded {
                tr!("import-grid-fold")
            } else {
                tr!("import-grid-unfold")
            };
            if ui.button(label).clicked() {
                gs.unfolded = !gs.unfolded;
            }
        }
    });
}

// -- The screen ----------------------------------------------------------------------------

pub(in crate::app::import_wizard) fn import_screen(
    ui: &mut egui::Ui,
    state: &mut AppState,
    requests: &mut ImportWizardRequests,
) {
    import_screen_inner(ui, state, requests);
}

fn import_screen_inner(
    ui: &mut egui::Ui,
    state: &mut AppState,
    requests: &mut ImportWizardRequests,
) {
    let ctx = ui.ctx().clone();
    let mut gs = load_gs(&ctx);
    // A new file: back to the top, folded.
    let shown = state.import_shown_path();
    if gs.last_path != shown {
        gs.last_path = shown;
        gs.pan = egui::Vec2::ZERO;
        gs.unfolded = false;
        gs.sel = Sel::None;
        gs.extras.clear();
        gs.bar_rect = Rect::NOTHING;
        gs.focus = None;
        gs.focus_line = None;
        gs.pinned = None;
        gs.auto_why = None;
    }
    file_row(ui, state, requests);
    if state.import_state().is_none_or(|i| i.paths.is_empty()) {
        ui.add_space(theme::SPACE_SECTION);
        ui.label(theme::dimmed_text(tr!("import-grid-no-file")));
        store_gs(&ctx, gs);
        return;
    }

    let mut out = GridOut::default();
    if let Some(refusal) = state.import_refusal() {
        let avail = ui.available_rect_before_wrap();
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(avail));
        refused_card(&mut child, refusal);
        ui.allocate_rect(avail, Sense::hover());
        store_gs(&ctx, gs);
        return;
    }

    state.check_review_sync();
    let check_view = state.import_check();
    let map_rc = state.import_file_map();
    if let Some(cmd) = check::review_keys(&ctx, state) {
        out.check.push(cmd);
    }
    gs.focus_next = None;
    if let Some(view) = &check_view {
        check::recognition_line(ui, view, &mut gs);
    }
    let recipe = state
        .import_state()
        .map(|i| i.recipe.clone())
        .unwrap_or_else(|| ImportRecipe::new(String::new()));
    let props = state.import_prop_rows();

    let avail = ui.available_rect_before_wrap();
    let content = Rect::from_min_max(
        avail.min,
        pos2(
            avail.right(),
            (avail.bottom() - RECEIPT_H - theme::SPACE_ITEM).max(avail.top() + 120.0),
        ),
    );
    let left_w = (content.width() * 0.35)
        .max(LEFT_MIN_W)
        .min((content.width() - GRID_MIN_W).max(LEFT_MIN_W.min(content.width() * 0.5)));
    let left_rect = Rect::from_min_size(content.min, vec2(left_w, content.height()));
    let right = Rect::from_min_max(
        pos2(left_rect.right() + theme::SPACE_SECTION, content.top()),
        content.max,
    );
    let tabs_rect = Rect::from_min_size(right.min, vec2(right.width(), TAB_STRIP_HEIGHT + 2.0));
    let body = Rect::from_min_max(pos2(right.left(), tabs_rect.bottom()), right.max);
    let receipt_rect = Rect::from_min_max(
        pos2(avail.left(), content.bottom() + theme::SPACE_ITEM),
        avail.max,
    );

    // The left panel first: what it hovers tells the grid what to show.
    if let Some(view) = &check_view {
        panel::left_panel(
            ui,
            left_rect,
            Left {
                state,
                view: view.clone(),
                gs: &mut gs,
                out: &mut out,
            },
        );
    }
    gs.focus = gs.focus_next.take();
    gs.pick_active = state.check_pick().is_some();
    let pick = state.check_pick();

    let fold_possible = match map_rc.as_deref() {
        Some(Ok(map)) => fold_range(map).is_some(),
        _ => false,
    };
    grid_tab_row(ui, state, tabs_rect, &mut gs, fold_possible);

    let mut guide_targets = guide::Targets {
        grid: body,
        panel: left_rect,
        line_number: None,
        value: None,
    };
    match map_rc.as_deref() {
        None => {}
        Some(Err(message)) => {
            ui.painter().text(
                body.left_top() + vec2(10.0, 20.0),
                Align2::LEFT_CENTER,
                errors::import::read_message(message),
                FontId::proportional(13.0),
                ui.visuals().error_fg_color,
            );
        }
        Some(Ok(map)) => {
            if gs.result_view {
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
                import_result_table(&mut child, &*state);
            } else {
                let dark = ui.visuals().dark_mode;
                let font = FontId::monospace(12.0);
                let small = FontId::monospace(10.5);
                let cw = ui
                    .painter()
                    .layout_no_wrap("M".into(), font.clone(), Color32::WHITE)
                    .size()
                    .x;
                check::focus_scroll(&mut gs, map, body.height() - LETTER_H - SB);
                let rows = build_rows(map, gs.unfolded, gs.frozen_fold);
                check::apply_focus_scroll(&mut gs, &rows);
                let metrics = metrics(&ctx, map, &recipe, cw);
                let marker = recipe.main_until_marker.clone();
                let bands = build_bands(&rows, map, marker.as_deref());
                let spans = state.import_spans();
                let view = View {
                    map,
                    recipe: &recipe,
                    info: None,
                    props: &props,
                    rows: &rows,
                    metrics: &metrics,
                    marker,
                    bands,
                    dark,
                    font,
                    small,
                    cw,
                    spans,
                };
                file_grid(ui, body, &view, &mut gs, &mut out);
                super::auto_why_popup(ui, &view, &mut gs, &mut out);
                // Action bar (drawn above the grid) and context menu.
                let mut scroll = gs.pan;
                let g = make_geom(
                    body,
                    rows.len(),
                    *view.metrics.colx.last().unwrap_or(&0.0),
                    &mut scroll,
                );
                action_bar(ui, &g, &view, &mut gs, &mut out);
                if let Some(kind) = pick {
                    check::pick_banner(ui, &g, kind);
                }
                (guide_targets.line_number, guide_targets.value) = guide::line_targets(
                    &g,
                    &view,
                    &gs,
                    check_view
                        .as_ref()
                        .map(|v| v.keys.as_slice())
                        .unwrap_or(&[]),
                );
            }
        }
    }
    if let Some(view) = &check_view {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(receipt_rect));
        check::receipt_line(&mut child, view);
    }
    ui.allocate_rect(avail, Sense::hover());
    guide::show(&ctx, state, &mut gs, &guide_targets);

    requests.grid.append(&mut out.cmds);
    requests.checks.append(&mut out.check);
    requests.auto.append(&mut out.auto);
    store_gs(&ctx, gs);
}

// -- The footer buttons ------------------------------------------------------------------------

/// A secondary button of the footer that opens a menu, with its caret.
fn menu_button(ui: &mut egui::Ui, text: String) -> egui::Response {
    // The caret is an icon at the right of the label; the label leaves room for it.
    let response = ui.add(egui::Button::new(format!("{text}\u{2003}\u{2002}")));
    let size = crate::icons::ICON_SIZE;
    let rect = egui::Rect::from_center_size(
        pos2(response.rect.right() - size, response.rect.center().y),
        vec2(size, size),
    );
    icon_image(Icon::CaretUp, IconRole::Neutral).paint_at(ui, rect);
    cursors::clickable(response)
}

/// `To: New Workbook` and `Template: Auto · JASCO V-770`, each with its menu
/// (S-F6: the secondary buttons of a draft sit on the left of the footer).
pub(in crate::app::import_wizard) fn footer_buttons(
    ui: &mut egui::Ui,
    state: &mut AppState,
    requests: &mut ImportWizardRequests,
) {
    let destination = match state.import_state().map(|i| i.destination) {
        Some(ImportDestination::ActiveBook(_)) => tr!("import-destination-active"),
        Some(ImportDestination::NewBookPerFile) => tr!("import-destination-each"),
        Some(ImportDestination::NewBookForAll) | None => tr!("import-destination-new"),
    };
    let template = match (state.template_in_use(), state.import_check()) {
        (Some(name), _) => tr!("import-footer-template-yours", name = name),
        (None, Some(v)) if v.fingerprint.recognised => tr!(
            "import-footer-template-auto",
            name = v.fingerprint.display.clone()
        ),
        _ => tr!("common-auto"),
    };
    let button = menu_button(ui, tr!("import-footer-to", value = destination));
    egui::Popup::from_toggle_button_response(&button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| destination_menu(ui, state));
    let button = menu_button(ui, tr!("import-footer-template", value = template));
    egui::Popup::from_toggle_button_response(&button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| template_menu(ui, state, requests));
}

fn destination_menu(ui: &mut egui::Ui, state: &mut AppState) {
    use stokesforge_import::{FilesLayout, ImportMode};
    let Some(import) = state.import_state_mut() else {
        return;
    };
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        let current = import.destination;
        if menu_choice(
            ui,
            current == ImportDestination::NewBookForAll,
            tr!("import-destination-new"),
        )
        .clicked()
        {
            import.destination = ImportDestination::NewBookForAll;
            ui.close();
        }
        if menu_choice(
            ui,
            current == ImportDestination::NewBookPerFile,
            tr!("import-destination-each"),
        )
        .clicked()
        {
            import.destination = ImportDestination::NewBookPerFile;
            ui.close();
        }
        match import.active_book {
            Some(book) => {
                if menu_choice(
                    ui,
                    matches!(current, ImportDestination::ActiveBook(_)),
                    tr!("import-destination-active"),
                )
                .clicked()
                {
                    import.destination = ImportDestination::ActiveBook(book);
                    ui.close();
                }
            }
            None => {
                ui.add_enabled_ui(false, |ui| {
                    menu_choice(ui, false, tr!("import-destination-active"));
                })
                .response
                .on_disabled_hover_text(tr!("disabled-import-no-active-book"));
            }
        }
    });
    sections.section(
        ui,
        matches!(import.destination, ImportDestination::ActiveBook(_)),
        |ui| {
            ui.label(theme::dimmed_text(tr!("import-destination-mode")));
            for (mode, text) in [
                (ImportMode::NewSheet, tr!("import-mode-new-sheet")),
                (ImportMode::ReplaceSheet, tr!("import-mode-replace")),
                (ImportMode::AddColumns, tr!("import-mode-add-columns")),
                (ImportMode::AddRows, tr!("import-mode-add-rows")),
            ] {
                if menu_choice(ui, import.recipe.import_mode == mode, text).clicked() {
                    import.recipe.import_mode = mode;
                    ui.close();
                }
            }
        },
    );
    sections.section(ui, import.paths.len() > 1, |ui| {
        ui.label(theme::dimmed_text(tr!("import-destination-multiple")));
        for (layout, text) in [
            (FilesLayout::SheetPerFile, tr!("import-layout-per-file")),
            (FilesLayout::SideBySide, tr!("import-layout-side-by-side")),
            (FilesLayout::SharedX, tr!("import-layout-shared-x")),
        ] {
            if menu_choice(ui, import.recipe.files_layout == layout, text).clicked() {
                import.recipe.files_layout = layout;
                ui.close();
            }
        }
    });
}

fn template_menu(ui: &mut egui::Ui, state: &mut AppState, requests: &mut ImportWizardRequests) {
    let rows = state.template_rows();
    let in_use = state.template_in_use();
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        if menu_choice(ui, in_use.is_none(), tr!("common-auto")).clicked() {
            requests.checks.push(CheckCmd::UseAuto);
            ui.close();
        }
        for row in &rows {
            let text = row.name.to_string();
            let r = menu_choice(ui, row.in_use, text).on_hover_ui(|ui| {
                ui.set_max_width(320.0);
                for rule in &row.rules {
                    ui.label(format!("• {rule}"));
                }
            });
            if r.clicked() {
                requests
                    .checks
                    .push(CheckCmd::UseTemplate(row.name.clone()));
                ui.close();
            }
        }
    });
    sections.section(ui, true, |ui| {
        if ui.button(tr!("import-template-menu-save")).clicked() {
            requests.checks.push(CheckCmd::SaveOpen);
            ui.close();
        }
        if ui.button(tr!("import-template-menu-manage")).clicked() {
            requests.checks.push(CheckCmd::ManageOpen);
            ui.close();
        }
    });
}
