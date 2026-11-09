//! the left panel of the Import Wizard, built
//! with the building blocks of the application. From top to bottom: the format
//! sentence (each value opens a menu), the block of the points to check, the
//! offer to save a template, « What was found » as readable rows. The model
//! (what was found, how sure) is `stokesforge_app_state::state::import_check`;
//! this file only paints it and turns clicks into [`CheckCmd`]s.

use std::sync::Arc;

use eframe::egui;
use egui::{vec2, Rect, Sense, Stroke};
use stokesforge_app_state::state::{
    AppState, Confidence, FoundGroup, FoundRole, FoundRow, Offer, Piece, CheckFinding, CheckKind,
    CheckView,
};
use stokesforge_app_state::tr;
use stokesforge_import::{DecimalStyle, Encoding, Separator};

use super::check::{self, CheckCmd};
use super::{encoding_label, GridOut, GridUi};
use crate::app::hover_help::hover_help;
use crate::app::list_row::{with_list_row_height, ListRowBackground, LIST_ROW_HEIGHT};
use crate::app::menu_sections::{menu_choice, MenuSections};
use crate::app::{cursors, dialogs};
use crate::icons::{icon_image, Icon, IconRole};
use crate::theme::{self, import_color, ImportColor};

/// What the left panel needs to paint one frame.
pub(super) struct Left<'a> {
    pub state: &'a mut AppState,
    pub view: Arc<CheckView>,
    pub gs: &'a mut GridUi,
    pub out: &'a mut GridOut,
}

/// Paints the left panel in `rect`: one scroll area, the format sentence
/// always on top, then the review, or the offer, the points to check and what
/// was found.
pub(super) fn left_panel(ui: &mut egui::Ui, rect: Rect, l: Left<'_>) {
    let Left {
        state,
        view,
        gs,
        out,
    } = l;
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(rect);
    // Each mode (the list, one doubt, the end of the review) starts at the top.
    let mode = (
        state.check_review_kind().map(|k| k as u8),
        state.check_review_done().is_some(),
    );
    theme::page_scroll_area()
        .id_salt(("stokesforge_import_j_left", mode))
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            ui.set_width(rect.width() - theme::scroll_bar_allocated_width());
            format_row(ui, state, &view, out);
            if let Some(info) = state.check_review_info() {
                check::review_panel(ui, &info, gs, out);
                return;
            }
            if let Some(log) = state.check_review_done() {
                done_card(ui, &view, &log, out);
            }
            if let Some(offer) = state.template_offer() {
                offer_card(ui, &offer, out);
            }
            if let Some(note) = &view.note {
                ui.label(
                    egui::RichText::new(note).color(theme::warning_color(ui.visuals().dark_mode)),
                );
            }
            warnings_block(ui, &view, gs, out);
            found_rows(ui, state, &view, gs, out);
        });
    // A « To check » tag was clicked: its evidence stays framed for a moment.
    if let Some((ev, since)) = gs.pinned.clone() {
        let now = ui.input(|i| i.time);
        if now - since > PIN_SECONDS {
            gs.pinned = None;
        } else {
            check::focus_hint(gs, &ev, theme::accent());
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
}

/// How long the evidence of a clicked « To check » stays framed.
const PIN_SECONDS: f64 = 6.0;

// -- The format sentence -------------------------------------------------------

/// « Name (glyph) »: `Semicolon (;)`.
fn named(name: String, glyph: impl std::fmt::Display) -> String {
    format!("{name} ({glyph})")
}

/// The name of a delimiter, with its character where it has one.
fn separator_name(separator: &Separator) -> String {
    match separator {
        Separator::Semicolon => named(tr!("import-delimiter-semicolon"), ';'),
        Separator::Comma => named(tr!("import-option-comma"), ','),
        Separator::Tab => tr!("import-delimiter-tab"),
        Separator::Whitespace => tr!("import-delimiter-whitespace"),
        Separator::Custom(c) => named(tr!("import-option-other"), c),
        Separator::FixedWidth(_) => tr!("import-delimiter-fixed"),
        Separator::Auto => tr!("common-auto"),
    }
}

/// What the « ? » quotes for a delimiter: its character, else its name.
fn separator_glyph(separator: &Separator) -> String {
    match separator {
        Separator::Semicolon => ";".to_string(),
        Separator::Comma => ",".to_string(),
        Separator::Custom(c) => c.to_string(),
        other => separator_name(other),
    }
}

fn separator_value(separator: &Separator, detected: &Separator) -> String {
    match separator {
        Separator::Auto => tr!("common-auto-value", value = separator_name(detected)),
        other => separator_name(other),
    }
}

fn decimal_name(decimal: DecimalStyle) -> String {
    match decimal {
        DecimalStyle::Comma => named(tr!("import-option-comma"), ','),
        DecimalStyle::Dot => named(tr!("import-option-dot"), '.'),
        DecimalStyle::Auto => tr!("common-auto"),
    }
}

fn decimal_value(decimal: DecimalStyle, detected: DecimalStyle) -> String {
    match decimal {
        DecimalStyle::Auto => tr!("common-auto-value", value = decimal_name(detected)),
        other => decimal_name(other),
    }
}

fn thousands_name(thousands: Option<char>) -> String {
    match thousands {
        None => tr!("import-option-none"),
        Some(' ') => tr!("import-option-space"),
        Some(',') => named(tr!("import-option-comma"), ','),
        Some('.') => named(tr!("import-option-dot"), '.'),
        Some('\'') => named(tr!("import-option-apostrophe"), '\''),
        Some(c) => named(tr!("import-option-other"), c),
    }
}

fn encoding_value(encoding: Encoding, detected_name: &str) -> String {
    match encoding {
        Encoding::Auto => tr!("common-auto-value", value = detected_name.to_string()),
        other => encoding_label(other),
    }
}

/// One cell of the format strip: a small caption over a framed chip that
/// shows the value (a big symbol for a separator) and opens `menu`. The
/// hover text spells out the full name and the current value.
fn format_cell(
    ui: &mut egui::Ui,
    caption: String,
    full_name: String,
    chip: &str,
    value: &str,
    menu: impl FnOnce(&mut egui::Ui),
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(theme::secondary_text(caption));
        let response = ui.add(
            egui::Button::new(theme::format_chip_text(chip))
                .min_size(vec2(34.0, 28.0))
                .corner_radius(6.0),
        );
        let response = cursors::clickable(response).on_hover_text(format!("{full_name}: {value}"));
        egui::Popup::from_toggle_button_response(&response)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(menu);
    });
}

/// What a delimiter chip shows: its character, else its short name.
fn separator_chip(separator: &Separator, detected: &Separator) -> String {
    match separator {
        Separator::Auto => separator_glyph(detected),
        other => separator_glyph(other),
    }
}

fn decimal_chip(decimal: DecimalStyle, detected: DecimalStyle) -> String {
    match if decimal == DecimalStyle::Auto {
        detected
    } else {
        decimal
    } {
        DecimalStyle::Comma => ",".to_string(),
        _ => ".".to_string(),
    }
}

fn thousands_chip(thousands: Option<char>) -> String {
    match thousands {
        None => tr!("import-option-none"),
        Some(' ') => tr!("import-option-space"),
        Some(c) => c.to_string(),
    }
}

/// A choice of a format menu; returns true when it was clicked.
fn choice(ui: &mut egui::Ui, selected: bool, text: String) -> bool {
    let clicked = menu_choice(ui, selected, text).clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// « Delimiter ; · Decimal separator , · Thousands separator none · Encoding
/// Windows-1252 », always visible; the « ? » says how each was found.
fn format_row(ui: &mut egui::Ui, state: &mut AppState, view: &CheckView, out: &mut GridOut) {
    let _ = out;
    let Some(import) = state.import_state() else {
        return;
    };
    let recipe = &import.recipe;
    let detected = &view.detected;
    let sep_text = separator_value(&recipe.separator, &detected.separator);
    let dec_text = decimal_value(recipe.decimal, detected.decimal);
    let thou_text = thousands_name(recipe.thousands);
    let enc_text = encoding_value(recipe.encoding, &view.encoding_name);
    let separator = recipe.separator.clone();
    let decimal = recipe.decimal;
    let thousands = recipe.thousands;
    let encoding = recipe.encoding;
    let sep_chip = separator_chip(&recipe.separator, &detected.separator);
    let dec_chip = decimal_chip(recipe.decimal, detected.decimal);
    let thou_chip = thousands_chip(recipe.thousands);
    let enc_chip = match recipe.encoding {
        Encoding::Auto => view.encoding_name.clone(),
        other => encoding_label(other),
    };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_SECTION;
        {
            {
                format_cell(
                    ui,
                    tr!("import-format-short-delimiter"),
                    tr!("import-format-delimiter"),
                    &sep_chip,
                    &sep_text,
                    |ui| {
                        separator_menu(ui, state, &separator, &detected.separator);
                    },
                );
                format_cell(
                    ui,
                    tr!("import-format-short-decimal"),
                    tr!("import-format-decimal"),
                    &dec_chip,
                    &dec_text,
                    |ui| {
                        let mut sections = MenuSections::new();
                        let mut chosen = None;
                        sections.section(ui, true, |ui| {
                            let auto = decimal_value(DecimalStyle::Auto, detected.decimal);
                            if choice(ui, decimal == DecimalStyle::Auto, auto) {
                                chosen = Some(DecimalStyle::Auto);
                            }
                        });
                        sections.section(ui, true, |ui| {
                            for d in [DecimalStyle::Comma, DecimalStyle::Dot] {
                                if choice(ui, decimal == d, decimal_name(d)) {
                                    chosen = Some(d);
                                }
                            }
                        });
                        if let Some(d) = chosen {
                            if let Some(import) = state.import_state_mut() {
                                import.recipe.decimal = d;
                            }
                            state.check_touch(CheckKind::Decimal);
                        }
                    },
                );
                format_cell(
                    ui,
                    tr!("import-format-short-thousands"),
                    tr!("import-format-thousands"),
                    &thou_chip,
                    &thou_text,
                    |ui| {
                        let mut chosen = None;
                        for t in [None, Some(' '), Some(','), Some('.'), Some('\'')] {
                            if choice(ui, thousands == t, thousands_name(t)) {
                                chosen = Some(t);
                            }
                        }
                        if let Some(t) = chosen {
                            if let Some(import) = state.import_state_mut() {
                                import.recipe.thousands = t;
                            }
                            state.check_touch(CheckKind::Decimal);
                        }
                    },
                );
                format_cell(
                    ui,
                    tr!("import-format-short-encoding"),
                    tr!("import-format-encoding"),
                    &enc_chip,
                    &enc_text,
                    |ui| {
                        let mut sections = MenuSections::new();
                        let mut chosen = None;
                        sections.section(ui, true, |ui| {
                            let auto = encoding_value(Encoding::Auto, &view.encoding_name);
                            if choice(ui, encoding == Encoding::Auto, auto) {
                                chosen = Some(Encoding::Auto);
                            }
                        });
                        sections.section(ui, true, |ui| {
                            for e in [
                                Encoding::Utf8,
                                Encoding::Windows1252,
                                Encoding::Latin1,
                                Encoding::Utf16Le,
                                Encoding::Utf16Be,
                            ] {
                                if choice(ui, encoding == e, encoding_label(e)) {
                                    chosen = Some(e);
                                }
                            }
                        });
                        if let Some(e) = chosen {
                            if let Some(import) = state.import_state_mut() {
                                import.recipe.encoding = e;
                            }
                            state.check_touch(CheckKind::Encoding);
                        }
                    },
                );
            }
        }
        hover_help(ui, |ui| {
            ui.set_max_width(360.0);
            ui.label(theme::bold(tr!("import-help-title")));
            ui.label(tr!(
                "import-help-sep",
                sep = separator_glyph(&separator),
                rows = view.n_rows.to_string(),
                cols = view.n_cols.to_string()
            ));
            if !view.decimal_sample.is_empty() {
                if separator == Separator::Comma {
                    ui.label(tr!(
                        "import-help-decimal-point",
                        sample = view.decimal_sample.clone()
                    ));
                } else {
                    ui.label(tr!(
                        "import-help-decimal",
                        sample = view.decimal_sample.clone(),
                        sep = separator_glyph(&separator)
                    ));
                }
            }
            ui.label(view.encoding_detail.clone());
        });
    });
}

fn separator_menu(
    ui: &mut egui::Ui,
    state: &mut AppState,
    current: &Separator,
    detected: &Separator,
) {
    let mut sections = MenuSections::new();
    let mut chosen: Option<Separator> = None;
    sections.section(ui, true, |ui| {
        let auto = separator_value(&Separator::Auto, detected);
        if choice(ui, matches!(current, Separator::Auto), auto) {
            chosen = Some(Separator::Auto);
        }
    });
    sections.section(ui, true, |ui| {
        for sep in super::separator_options()
            .into_iter()
            .filter(|s| !matches!(s, Separator::Auto))
        {
            let selected = super::same_variant(current, &sep);
            let text = match &sep {
                Separator::Custom(_) => format!("{}…", tr!("import-option-other")),
                other => separator_name(other),
            };
            if menu_choice(ui, selected, text).clicked() && !selected {
                chosen = Some(sep);
                if !matches!(
                    chosen,
                    Some(Separator::Custom(_) | Separator::FixedWidth(_))
                ) {
                    ui.close();
                }
            }
        }
    });
    // The character or the widths of the two choices that need one.
    let mut changed = false;
    if let Some(import) = state.import_state_mut() {
        if let Some(sep) = chosen.take() {
            import.recipe.separator = sep;
            changed = true;
        }
        match &mut import.recipe.separator {
            Separator::Custom(c) => {
                ui.horizontal(|ui| {
                    ui.label(tr!("import-grid-other-char"));
                    let mut text = c.to_string();
                    if ui
                        .add(egui::TextEdit::singleline(&mut text).desired_width(40.0))
                        .changed()
                    {
                        if let Some(ch) = text.chars().last() {
                            *c = ch;
                            changed = true;
                        }
                    }
                });
            }
            Separator::FixedWidth(widths) => {
                let mut text = widths
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                if ui
                    .add(egui::TextEdit::singleline(&mut text).desired_width(200.0))
                    .changed()
                {
                    *widths = text
                        .split(',')
                        .filter_map(|p| p.trim().parse::<usize>().ok())
                        .collect();
                    changed = true;
                }
            }
            _ => {}
        }
    }
    if changed {
        state.check_touch(CheckKind::Separator);
    }
}

// -- The points to check -----------------------------------------------------------

/// A sentence of a finding without its chips as buttons: « Line 18 is x′ (the
/// times) ».
fn plain(f: &CheckFinding) -> String {
    f.pieces
        .iter()
        .map(|piece| match piece {
            Piece::Text(text) => text.as_str(),
            Piece::Chip { text, .. } => text.as_str(),
        })
        .collect()
}

/// The frame that says what is not sure: a warning border, a light warning
/// fill, one sentence per doubt, what the file does not say, and the
/// « Check Now » button that starts the review.
fn warnings_block(ui: &mut egui::Ui, view: &CheckView, gs: &mut GridUi, out: &mut GridOut) {
    let dark = ui.visuals().dark_mode;
    let warn = theme::warning_color(dark);
    let doubts: Vec<&CheckFinding> = view.doubts();
    let missing: Vec<&CheckFinding> = view
        .findings
        .iter()
        .filter(|f| f.confidence == Confidence::Missing)
        .collect();
    if doubts.is_empty() && missing.is_empty() {
        return;
    }
    ui.add_space(theme::SPACE_SECTION);
    egui::Frame::new()
        .fill(theme::import_warning_fill(dark))
        .stroke(Stroke::new(1.5, warn))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !doubts.is_empty() {
                ui.label(
                    theme::section_heading(tr!("import-check-warn-title", count = doubts.len()))
                        .size(16.0)
                        .color(warn),
                );
                ui.add_space(theme::SPACE_ITEM);
                for f in &doubts {
                    let r = ui.add(egui::Label::new(plain(f)).wrap().sense(Sense::hover()));
                    if r.hovered() {
                        check::focus_hint(gs, &f.evidence, theme::accent());
                    }
                    if !f.detail.is_empty() {
                        ui.label(theme::secondary_text(f.detail.clone()));
                    }
                }
            }
            for f in &missing {
                let r = ui.add(egui::Label::new(plain(f)).wrap().sense(Sense::hover()));
                r.on_hover_text(tr!("import-check-missing-tip"));
            }
            if !doubts.is_empty() {
                ui.add_space(theme::SPACE_SECTION);
                let button = egui::Button::new(theme::bold(tr!("import-check-warn-check-now")))
                    .fill(ui.visuals().selection.bg_fill)
                    .min_size(vec2(dialogs::FOOTER_BUTTON_SIZE.x + 24.0, 28.0));
                if cursors::clickable(ui.add(button)).clicked() {
                    out.check.push(CheckCmd::ReviewStart(None));
                }
            }
        });
}

/// The card of the end of a review: what was decided.
fn done_card(ui: &mut egui::Ui, view: &CheckView, log: &[String], out: &mut GridOut) {
    let dark = ui.visuals().dark_mode;
    let green = theme::import_sure_color(dark);
    ui.add_space(theme::SPACE_SECTION);
    egui::Frame::new()
        .stroke(Stroke::new(1.0, green))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if view.n_check == 0 {
                    ui.add(icon_image(Icon::Check, IconRole::Neutral));
                    ui.label(theme::section_heading(tr!("import-check-done-title")).color(green));
                } else {
                    ui.label(
                        theme::section_heading(tr!(
                            "import-check-done-left",
                            count = view.n_check.to_string()
                        ))
                        .color(green),
                    );
                }
            });
            for line in log {
                ui.label(format!("• {line}"));
            }
            if ui.button(tr!("import-check-done-close")).clicked() {
                out.check.push(CheckCmd::DoneClose);
            }
        });
}

/// « Save these settings for JASCO V-770 files? »
fn offer_card(ui: &mut egui::Ui, offer: &Offer, out: &mut GridOut) {
    ui.add_space(theme::SPACE_SECTION);
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(theme::section_heading(tr!(
                "import-template-offer-title",
                name = offer.display.clone()
            )));
            ui.label(theme::secondary_text(tr!(
                "import-template-offer-name",
                name = offer.name.clone()
            )));
            ui.add_space(theme::SPACE_ITEM);
            ui.horizontal(|ui| {
                let save = egui::Button::new(tr!("import-template-offer-save"))
                    .fill(ui.visuals().selection.bg_fill);
                let r = ui.add(save).on_hover_ui(|ui| {
                    ui.set_max_width(320.0);
                    ui.label(theme::bold(tr!("import-template-offer-note-tip")));
                    for rule in &offer.rules {
                        ui.label(format!("• {rule}"));
                    }
                });
                if cursors::clickable(r).clicked() {
                    out.check.push(CheckCmd::SaveOpen);
                }
                if ui.button(tr!("import-template-offer-later")).clicked() {
                    out.check.push(CheckCmd::NotNow);
                }
            });
            let manage = ui.add(egui::Button::new(tr!("import-template-offer-manage")).frame(false));
            if cursors::clickable(manage).clicked() {
                out.check.push(CheckCmd::ManageOpen);
            }
        });
}

// -- What was found --------------------------------------------------------------------

fn group_title(group: FoundGroup) -> String {
    match group {
        FoundGroup::Data => tr!("import-found-group-data"),
        FoundGroup::Columns => tr!("import-found-group-columns"),
        FoundGroup::Sheet => tr!("import-found-group-sheet"),
    }
}

fn role_color(dark: bool, role: FoundRole) -> Option<egui::Color32> {
    match role {
        FoundRole::None => None,
        FoundRole::LongName => Some(import_color(dark, ImportColor::LongName)),
        FoundRole::Units => Some(import_color(dark, ImportColor::Units)),
        FoundRole::Comments => Some(import_color(dark, ImportColor::Comments)),
        FoundRole::XPrime => Some(import_color(dark, ImportColor::XPrime)),
    }
}

/// Width of the right column of a row: the « To check » tag and its ✓.
const TAG_COLUMN_W: f32 = 118.0;
/// Width of the icon column of a row.
const ICON_COLUMN_W: f32 = 18.0;
/// Spare width at the end of a row, against rounding and the scroll bar.
const ROW_SLACK: f32 = 6.0;

/// « What was found » as rows grouped under their titles. Hovering a row
/// frames its line in the grid; clicking its value opens the correction.
/// Every row has the same columns (icon, label, value, tag), so a long value
/// is cut with an ellipsis and never pushes the tag out of the panel.
fn found_rows(
    ui: &mut egui::Ui,
    state: &mut AppState,
    view: &Arc<CheckView>,
    gs: &mut GridUi,
    out: &mut GridOut,
) {
    let label_w = (ui.available_width() * 0.30).clamp(90.0, 140.0);
    let mut last: Option<FoundGroup> = None;
    for (index, row) in view.rows.iter().enumerate() {
        if last != Some(row.group) {
            // Room between two groups: the title alone separates them.
            ui.add_space(theme::SPACE_SECTION * 2.0);
            theme::section_title(ui, group_title(row.group));
            ui.add_space(theme::SPACE_ITEM);
            last = Some(row.group);
        }
        found_row(ui, state, view, row, index, label_w, gs, out);
    }
}

#[allow(clippy::too_many_arguments)]
fn found_row(
    ui: &mut egui::Ui,
    state: &mut AppState,
    view: &Arc<CheckView>,
    row: &FoundRow,
    index: usize,
    label_w: f32,
    gs: &mut GridUi,
    out: &mut GridOut,
) {
    let dark = ui.visuals().dark_mode;
    let warn = theme::warning_color(dark);
    let doubt = row.confidence == Confidence::Check;
    let background = ListRowBackground::reserve(ui);
    let total_w = ui.available_width();
    let gap = ui.spacing().item_spacing.x;
    // The tag column is reserved first, so a long value can only eat into
    // the value column; the slack keeps the ✓ clear of the scroll bar.
    let value_w =
        (total_w - ICON_COLUMN_W - label_w - TAG_COLUMN_W - 3.0 * gap - ROW_SLACK).max(40.0);
    let inner = ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.visuals_mut().indent_has_left_vline = false;
        let mut value_response = None;
        with_list_row_height(ui, |ui| {
            ui.horizontal(|ui| {
                // Icon column.
                ui.allocate_ui_with_layout(
                    vec2(ICON_COLUMN_W, LIST_ROW_HEIGHT),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_width(ICON_COLUMN_W);
                        if doubt {
                            ui.add(icon_image(Icon::Stale, IconRole::Neutral));
                        } else {
                            ui.add(
                                icon_image(Icon::Check, IconRole::Neutral)
                                    .tint(theme::help_icon_tint()),
                            );
                        }
                    },
                );
                // Label column.
                ui.allocate_ui_with_layout(
                    vec2(label_w, LIST_ROW_HEIGHT),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_width(label_w);
                        ui.add(egui::Label::new(row.label.clone()).truncate());
                    },
                );
                // Value column: cut with an ellipsis, the whole text on hover.
                ui.allocate_ui_with_layout(
                    vec2(value_w, LIST_ROW_HEIGHT),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_width(value_w);
                        ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                        let mut text = theme::bold(row.value.clone());
                        if let Some(color) = role_color(dark, row.role) {
                            text = text.color(color);
                        }
                        let full_w = ui
                            .painter()
                            .layout_no_wrap(
                                row.value.clone(),
                                egui::TextStyle::Body.resolve(ui.style()),
                                ui.visuals().text_color(),
                            )
                            .size()
                            .x;
                        let cut = full_w > value_w - 12.0;
                        let r = if row.chip.is_some() {
                            let r = ui.add(egui::Button::new(text).frame(false).truncate());
                            let r = cursors::clickable(r);
                            value_response = Some(r.clone());
                            r
                        } else {
                            ui.add(egui::Label::new(text).truncate().sense(Sense::hover()))
                        };
                        if cut {
                            r.on_hover_text(row.value.clone());
                        }
                    },
                );
                // Tag column: « To check » and « Mark as Checked ».
                ui.allocate_ui_with_layout(
                    vec2(TAG_COLUMN_W, LIST_ROW_HEIGHT),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_width(TAG_COLUMN_W);
                        if !doubt {
                            return;
                        }
                        let tag = egui::Button::new(
                            egui::RichText::new(tr!("import-check-tag-check"))
                                .color(warn)
                                .size(theme::SECONDARY_TEXT_SIZE),
                        )
                        .small();
                        let tag = cursors::clickable(ui.add(tag))
                            .on_hover_text(tr!("import-check-tag-check-go"));
                        if tag.clicked() {
                            // Shows the evidence; settles nothing.
                            let now = ui.input(|i| i.time);
                            gs.pinned = Some((row.evidence.clone(), now));
                        }
                        if crate::icons::icon_button(
                            ui,
                            Icon::Check,
                            IconRole::Neutral,
                            tr!("import-check-mark-checked"),
                        )
                        .clicked()
                        {
                            out.check.push(CheckCmd::MarkChecked(row.kind));
                        }
                    },
                );
            });
        });
        if !row.source.is_empty() {
            ui.indent(("stokesforge_import_j_src", index), |ui| {
                ui.add(egui::Label::new(theme::secondary_text(row.source.clone())).wrap());
            });
        }
        ui.add_space(theme::SPACE_ITEM);
        value_response
    });
    let rect = inner.response.rect;
    background.paint(ui, rect, false, false);
    // A hairline under the row keeps the rows apart.
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
    if let (Some(response), Some(chip)) = (inner.inner, row.chip) {
        let key = key_of(state, view, row);
        check::chip_popup(&response, state, view, row.kind, key, chip, gs, out);
    }
    let hovered = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| rect.contains(p))
        && !egui::Popup::is_any_open(ui.ctx());
    if hovered {
        check::focus_hint(gs, &row.evidence, theme::accent());
    }
}

/// The key of the file a row reads its value from (for the correction menu).
fn key_of(state: &AppState, view: &CheckView, row: &FoundRow) -> Option<String> {
    if row.kind == CheckKind::Comment {
        return state.import_state().and_then(|i| {
            i.recipe
                .header_props
                .iter()
                .find(|p| p.field == stokesforge_import::PropField::Comment)
                .map(|p| p.key.clone())
        });
    }
    view.finding(row.kind).and_then(|f| f.key.clone())
}
