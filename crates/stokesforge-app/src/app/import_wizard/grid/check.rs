//! what the left panel asks of the grid and of
//! the state. The review of the doubts (one big question at a time, with the
//! evidence framed in the grid), the correction menu of a value, the focus the
//! panel puts on the grid (the line a sentence comes from) and the clicks the
//! panel turns into [`CheckCmd`]s, applied once the frame is painted.
//!
//! The model (confidence, evidence, questions, templates) lives in
//! `stokesforge_app_state::state::import_check`; the panel itself is
//! `panel.rs`.

use eframe::egui;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind};
use stokesforge_app_state::state::{
    fold_range, kv_of, row_of_line, AppState, GridCmd, Row, CheckKind, CheckView, ChipId, Evidence, PropChoice, ReviewInfo,
};
use stokesforge_app_state::tr;

use super::{fit_text, hatch, row_y, Geom, GridOut, GridUi, View, LETTER_H, ROW_H};
use crate::app::shortcuts::{
    KEY_ANSWER_1, KEY_ANSWER_2, KEY_ANSWER_3, KEY_BACKSPACE, KEY_ENTER, KEY_ESCAPE,
};
use crate::theme;

/// What the left panel asks the grid to show: set each frame by the row, the
/// popup item or the review step under the pointer.
#[derive(Clone, Debug)]
pub(super) struct Focus {
    pub ev: Vec<Evidence>,
    pub color: Color32,
    /// During the review: spotlight and loupe.
    pub review: bool,
}

/// A click of the left panel, applied once the frame is painted.
#[derive(Clone, Debug)]
pub(in crate::app::import_wizard) enum CheckCmd {
    ReviewStart(Option<CheckKind>),
    Answer(usize),
    Skip,
    Back,
    Stop,
    Picked(usize),
    PickCancel,
    SaveOpen,
    ManageOpen,
    NotNow,
    DoneClose,
    UseTemplate(String),
    UseAuto,
    /// « Mark as Checked » on one point of the panel.
    MarkChecked(CheckKind),
}

/// What the panel hovers frames the evidence in the grid, if nothing else
/// already does.
pub(super) fn focus_hint(gs: &mut GridUi, ev: &[Evidence], color: Color32) {
    if gs.focus_next.is_none() && !ev.is_empty() {
        gs.focus_next = Some(Focus {
            ev: ev.to_vec(),
            color,
            review: false,
        });
    }
}

// -- Keys of the review ----------------------------------------------------------

/// Enter, 1/2/3, Backspace and Escape while a review (or a pick) is on.
pub(super) fn review_keys(ctx: &egui::Context, state: &AppState) -> Option<CheckCmd> {
    let reviewing = state.check_review_kind().is_some();
    let picking = state.check_pick().is_some();
    if !reviewing && !picking {
        return None;
    }
    if ctx.text_edit_focused() || egui::Popup::is_any_open(ctx) {
        return None;
    }
    let take = |shortcut: &egui::KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(shortcut));
    if take(&KEY_ESCAPE) {
        return Some(if picking {
            CheckCmd::PickCancel
        } else {
            CheckCmd::Stop
        });
    }
    if !reviewing {
        return None;
    }
    if take(&KEY_ENTER) || take(&KEY_ANSWER_1) {
        return Some(CheckCmd::Answer(0));
    }
    if take(&KEY_ANSWER_2) {
        return Some(CheckCmd::Answer(1));
    }
    if take(&KEY_ANSWER_3) {
        return Some(CheckCmd::Answer(2));
    }
    if take(&KEY_BACKSPACE) {
        return Some(CheckCmd::Back);
    }
    None
}

// -- Applying ------------------------------------------------------------------------

/// Applies a click of the left panel (state released by the painting).
pub(in crate::app::import_wizard) fn apply(state: &mut AppState, cmd: CheckCmd) {
    match cmd {
        CheckCmd::ReviewStart(kind) => match kind {
            Some(k) => state.check_review_start_at(k),
            None => state.check_review_start(),
        },
        CheckCmd::Stop => state.check_review_stop(),
        CheckCmd::Back => state.check_review_back(),
        CheckCmd::Skip => state.check_review_next(None),
        CheckCmd::PickCancel => state.check_set_pick(None),
        CheckCmd::NotNow => state.template_offer_dismiss(),
        CheckCmd::DoneClose => state.check_review_done_close(),
        CheckCmd::SaveOpen => state.template_save_open(),
        CheckCmd::ManageOpen => state.template_manage_open(),
        CheckCmd::UseTemplate(name) => state.template_use(&name),
        CheckCmd::UseAuto => state.template_use_auto(),
        CheckCmd::MarkChecked(kind) => state.check_mark_checked(kind),
        CheckCmd::Picked(line) => {
            state.check_picked(line);
        }
        CheckCmd::Answer(i) => state.check_answer(i),
    }
}

// -- The correction menu of a value ------------------------------------------------------

/// The menu a click on a value opens: the first and last data line, or the
/// key of the file the value is read from.
#[allow(clippy::too_many_arguments)]
pub(super) fn chip_popup(
    response: &egui::Response,
    state: &mut AppState,
    view: &CheckView,
    kind: CheckKind,
    key: Option<String>,
    id: ChipId,
    gs: &mut GridUi,
    out: &mut GridOut,
) {
    egui::Popup::from_toggle_button_response(response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(300.0);
            match id {
                ChipId::DataFirst | ChipId::DataLast => {
                    let Some((first, last)) = view.data else {
                        return;
                    };
                    let (mut a, mut b) = (first, last);
                    ui.horizontal(|ui| {
                        ui.label(tr!("import-check-data-first"));
                        crate::number_field::number_field(
                            ui,
                            "import_data_first",
                            &mut a,
                            &stokesforge_app_state::quantities::COUNT,
                        );
                        ui.label(tr!("import-check-data-last"));
                        crate::number_field::number_field(
                            ui,
                            "import_data_last",
                            &mut b,
                            &stokesforge_app_state::quantities::COUNT,
                        );
                    });
                    if (a, b) != (first, last) {
                        out.cmds.push(GridCmd::Handle { first: a, last: b });
                        state.check_touch(CheckKind::Data);
                    }
                }
                ChipId::Key(which) => {
                    ui.label(theme::secondary_text(tr!("import-chip-keys")));
                    let choice = match which {
                        CheckKind::XUnit => PropChoice::UnitsX,
                        CheckKind::YUnit => PropChoice::UnitsY,
                        CheckKind::YName => PropChoice::LongNameY,
                        CheckKind::Comment => PropChoice::CommentY,
                        _ => PropChoice::SheetName,
                    };
                    let current = key.clone().unwrap_or_default();
                    let mut chosen: Option<String> = None;
                    // RAW-SCROLL(#347): the list of the keys of the file, in a popup.
                    egui::ScrollArea::vertical()
                        .id_salt("stokesforge_import_keys")
                        .max_width(380.0)
                        .max_height(260.0)
                        .show(ui, |ui| {
                            let mut entries: Vec<&stokesforge_app_state::state::KeyEntry> =
                                view.keys.iter().collect();
                            // Keys that look like units or axes first.
                            entries.sort_by_key(|e| {
                                let up = e.key.to_uppercase();
                                !(up.contains("UNIT")
                                    || up.contains("AXIS")
                                    || up.contains("TITLE"))
                            });
                            for entry in entries {
                                let same = entry.key.eq_ignore_ascii_case(&current);
                                let label = format!(
                                    "{} = {}   ({})",
                                    entry.key,
                                    fit_text(&entry.value, 18),
                                    tr!("import-chip-line", line = entry.line.to_string())
                                );
                                let r = crate::app::menu_sections::menu_choice(ui, same, label);
                                if r.hovered() {
                                    gs.focus_next = Some(Focus {
                                        ev: vec![Evidence {
                                            line: entry.line,
                                            cell: Some(1),
                                        }],
                                        color: theme::accent(),
                                        review: false,
                                    });
                                }
                                if r.clicked() && !same {
                                    chosen = Some(entry.key.clone());
                                }
                            }
                        });
                    let nothing = crate::app::menu_sections::menu_choice(
                        ui,
                        false,
                        tr!("import-chip-nothing"),
                    )
                    .clicked();
                    if let Some(key) = chosen {
                        state.import_set_prop_choice(&key, choice);
                        state.check_touch(which);
                        ui.close();
                    } else if nothing && !current.is_empty() {
                        state.import_set_prop_choice(&current, PropChoice::Nothing);
                        state.check_touch(which);
                        ui.close();
                    }
                }
                _ => {}
            }
            let _ = kind;
        });
}

// -- Review -------------------------------------------------------------------------------

/// The question of the review: progress, the title, what was found in the file
/// (hover frames it), the big answers (Enter takes the first, 1 2 3).
pub(super) fn review_panel(
    ui: &mut egui::Ui,
    info: &ReviewInfo,
    gs: &mut GridUi,
    out: &mut GridOut,
) {
    let dark = ui.visuals().dark_mode;
    let warn = theme::warning_color(dark);
    let accent = theme::accent();
    let Some(q) = &info.finding.question else {
        return;
    };
    // The evidence in the grid, with spotlight and loupe.
    gs.focus_next = Some(Focus {
        ev: info.finding.evidence.clone(),
        color: warn,
        review: true,
    });
    // Progress dots.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for i in 0..info.total {
            let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
            let c = r.center();
            if i == info.index {
                ui.painter().circle_filled(c, 5.5, accent);
            } else if i < info.index {
                ui.painter()
                    .circle_filled(c, 5.0, theme::import_sure_color(dark));
            } else {
                ui.painter().circle_stroke(
                    c,
                    5.0,
                    Stroke::new(
                        1.5,
                        ui.visuals()
                            .widgets
                            .noninteractive
                            .fg_stroke
                            .color
                            .gamma_multiply(0.6),
                    ),
                );
            }
        }
        ui.label(theme::secondary_text(tr!(
            "import-check-review-of",
            n = (info.index + 1).to_string(),
            total = info.total.to_string()
        )));
    });
    ui.add_space(theme::SPACE_ITEM);
    ui.add(egui::Label::new(theme::bold(q.title.clone()).size(17.0)).wrap());
    ui.add(egui::Label::new(egui::RichText::new(&q.explain).size(13.0)).wrap());
    if !q.support.is_empty() {
        ui.add_space(theme::SPACE_ITEM);
        ui.label(theme::table_header(tr!("import-check-support-title")));
        let hover = ui.input(|i| i.pointer.hover_pos());
        for (ev, text) in &q.support {
            let r = egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .corner_radius(5.0)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(egui::Label::new(egui::RichText::new(text.clone())).wrap());
                });
            if hover.is_some_and(|p| r.response.rect.contains(p)) {
                gs.focus_next = Some(Focus {
                    ev: vec![*ev],
                    color: accent,
                    review: true,
                });
                ui.painter().rect_stroke(
                    r.response.rect,
                    5.0,
                    Stroke::new(1.5, accent),
                    StrokeKind::Inside,
                );
            }
        }
    }
    ui.add_space(theme::SPACE_SECTION);
    for (i, answer) in q.answers.iter().enumerate() {
        let first = i == 0;
        let width = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(vec2(width, 44.0), Sense::click());
        let hovered = resp.hovered();
        let (fill, stroke, text_color) = if first {
            (
                accent.gamma_multiply(if hovered { 1.0 } else { 0.85 }),
                Stroke::NONE,
                theme::text_on_accent(),
            )
        } else {
            (
                if hovered {
                    ui.visuals().widgets.hovered.bg_fill
                } else {
                    Color32::TRANSPARENT
                },
                Stroke::new(1.5, accent.gamma_multiply(0.7)),
                ui.visuals().strong_text_color(),
            )
        };
        ui.painter()
            .rect(rect, 9.0, fill, stroke, StrokeKind::Inside);
        let font = FontId::proportional(13.5);
        let galley =
            ui.painter()
                .layout(answer.label.clone(), font, text_color, rect.width() - 64.0);
        ui.painter().galley(
            pos2(rect.left() + 12.0, rect.center().y - galley.size().y / 2.0),
            galley,
            text_color,
        );
        let hint = if first {
            tr!("import-check-key-enter")
        } else {
            (i + 1).to_string()
        };
        ui.painter().text(
            pos2(rect.right() - 12.0, rect.center().y),
            Align2::RIGHT_CENTER,
            hint,
            FontId::proportional(11.0),
            if first {
                text_color.gamma_multiply(0.85)
            } else {
                ui.visuals()
                    .widgets
                    .noninteractive
                    .fg_stroke
                    .color
                    .gamma_multiply(0.6)
            },
        );
        if resp.clicked() {
            out.check.push(CheckCmd::Answer(i));
        }
        let _ = super::cursors::clickable(resp);
    }
    ui.add_space(theme::SPACE_SECTION);
    ui.horizontal(|ui| {
        if ui.button(tr!("import-check-skip")).clicked() {
            out.check.push(CheckCmd::Skip);
        }
        if ui.button(tr!("import-check-stop")).clicked() {
            out.check.push(CheckCmd::Stop);
        }
        if info.index > 0 && ui.button(tr!("import-check-back")).clicked() {
            out.check.push(CheckCmd::Back);
        }
    });
    ui.add_space(theme::SPACE_ITEM);
    ui.label(theme::secondary_text(tr!("import-check-keys-hint")));
}

// -- Recognition line, receipt ---------------------------------------------------------------------

/// « Recognised: JASCO V-770 · UV spectrum · kinetic (240 curves) · your
/// template « … » ».
pub(super) fn recognition_line(ui: &mut egui::Ui, view: &CheckView, gs: &mut GridUi) {
    let fp = &view.fingerprint;
    let ctx = ui.ctx().clone();
    ui.horizontal(|ui| {
        if fp.recognised {
            ui.add(
                crate::icons::icon_image(
                    crate::icons::Icon::Check,
                    crate::icons::IconRole::Neutral,
                )
                .tint(theme::help_icon_tint()),
            );
        }
        ui.label(theme::dimmed_text(if fp.recognised {
            tr!("import-recognised")
        } else {
            tr!("import-check-not-recognised")
        }));
        let response = ui.add(egui::Button::new(theme::bold(&fp.display)).frame(false));
        fingerprint_popup(&ctx, &response, view, gs);
        if let Some(name) = &view.template {
            ui.label(format!(
                "· {}",
                tr!("import-template-note-badge", name = name.clone())
            ));
        }
    });
}

/// « You get: … », under the content.
pub(super) fn receipt_line(ui: &mut egui::Ui, view: &CheckView) {
    ui.add(egui::Label::new(theme::secondary_text(view.receipt.clone())).truncate());
}

// -- In the grid ----------------------------------------------------------------------------------

/// The small popup that lists how the instrument was recognised: open while
/// the pointer is over the chip or the popup, each item frames its line.
/// The small popup that lists how the instrument was recognised: open while
/// the pointer is over the chip or the popup, each item frames its line.
fn fingerprint_popup(
    ctx: &egui::Context,
    chip: &egui::Response,
    view: &CheckView,
    gs: &mut GridUi,
) {
    let fp = &view.fingerprint;
    if fp.evidence.is_empty() {
        return;
    }
    let id = egui::Id::new("h_fingerprint_popup");
    let prev: Option<Rect> = ctx.data(|d| d.get_temp(id));
    let pointer = ctx.input(|i| i.pointer.hover_pos());
    let near = pointer
        .is_some_and(|p| chip.rect.contains(p) || prev.is_some_and(|r| r.expand(2.0).contains(p)));
    if !near {
        ctx.data_mut(|d| d.remove::<Rect>(id));
        return;
    }
    let area = egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(chip.rect.left_bottom() - vec2(0.0, 1.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(360.0);
                ui.label(theme::bold(tr!("import-fingerprint-title")));
                let hover = ui.input(|i| i.pointer.hover_pos());
                for (ev, label) in &fp.evidence {
                    let text = format!(
                        "{}   ({})",
                        label,
                        tr!("import-chip-line", line = ev.line.to_string())
                    );
                    let r =
                        ui.add(egui::Label::new(egui::RichText::new(text)).sense(Sense::hover()));
                    if hover.is_some_and(|p| r.rect.expand2(vec2(40.0, 1.0)).contains(p)) {
                        gs.focus_next = Some(Focus {
                            ev: vec![*ev],
                            color: theme::accent(),
                            review: false,
                        });
                    }
                }
            });
        });
    ctx.data_mut(|d| d.insert_temp(id, area.response.rect));
}

/// Scrolls so that the first line of the focus sits at a third of the grid,
/// unless it is already visible. Called once per change of target.
pub(super) fn focus_scroll(gs: &mut GridUi, map: &stokesforge_import::FileMap, view_h: f32) {
    let Some(focus) = gs.focus.clone() else {
        // The review ended: back to the top of the file.
        if matches!(gs.focus_line, Some((_, true))) {
            gs.pan.y = 0.0;
        }
        gs.focus_line = None;
        return;
    };
    let Some(first) = focus.ev.first() else {
        return;
    };
    let line = first.line.saturating_sub(1);
    if gs.focus_line == Some((line, focus.review)) {
        return;
    }
    gs.focus_line = Some((line, focus.review));
    if !gs.unfolded {
        if let Some((a, b)) = fold_range(map) {
            if line >= a && line <= b {
                gs.unfolded = true;
            }
        }
    }
    gs.focus_scroll_to = Some((line, view_h));
    if focus.review {
        gs.pan.x = 0.0;
    }
}

/// Applies a pending scroll once the rows are known.
pub(super) fn apply_focus_scroll(gs: &mut GridUi, rows: &[Row]) {
    let Some((line, view_h)) = gs.focus_scroll_to.take() else {
        return;
    };
    let r = row_of_line(rows, line);
    let top = r as f32 * ROW_H;
    let visible = top >= gs.pan.y && top + ROW_H <= gs.pan.y + view_h;
    if !visible {
        gs.pan.y = (top - view_h / 3.0).max(0.0);
    }
}

/// The focus of the left column in the grid: tinted line, framed cell, and
/// during the review the spotlight and the loupe.
pub(super) fn paint_focus(
    ui: &egui::Ui,
    g: &Geom,
    v: &View<'_>,
    gs: &GridUi,
    value_rect: &dyn Fn(&super::Kv, usize) -> Rect,
) {
    let Some(focus) = &gs.focus else {
        return;
    };
    let painter = ui.painter().clone();
    let clip = Rect::from_min_max(
        pos2(g.nums.left() - 2.0, g.rows_top),
        pos2(g.grid.right(), g.rows_top + g.view_h),
    );
    let p = painter.with_clip_rect(clip);
    let rows = v.rows;
    let map = v.map;
    let sx = gs.pan.x;
    let lines: Vec<usize> = focus.ev.iter().map(|e| e.line.saturating_sub(1)).collect();
    // Spotlight first (under the frames).
    if focus.review && !lines.is_empty() {
        let lo = lines.iter().copied().min().unwrap_or(0).saturating_sub(4);
        let hi = lines.iter().copied().max().unwrap_or(0) + 4;
        let r_lo = row_of_line(rows, lo);
        let r_hi = row_of_line(rows, hi.min(map.line_count().saturating_sub(1)));
        let y_lo = row_y(g, gs.pan, r_lo);
        let y_hi = row_y(g, gs.pan, r_hi) + ROW_H;
        let dim = ui.visuals().panel_fill.gamma_multiply(0.78);
        let full_x = (g.nums.left() - 2.0, g.grid.right());
        if y_lo > g.rows_top {
            p.rect_filled(
                Rect::from_min_max(
                    pos2(full_x.0, g.rows_top),
                    pos2(full_x.1, y_lo.min(g.rows_top + g.view_h)),
                ),
                0.0,
                dim,
            );
        }
        if y_hi < g.rows_top + g.view_h {
            p.rect_filled(
                Rect::from_min_max(
                    pos2(full_x.0, y_hi.max(g.rows_top)),
                    pos2(full_x.1, g.rows_top + g.view_h),
                ),
                0.0,
                dim,
            );
        }
    }
    for (k, ev) in focus.ev.iter().enumerate() {
        let i = ev.line.saturating_sub(1);
        if i >= map.line_count() {
            continue;
        }
        let r = row_of_line(rows, i);
        if !matches!(rows.get(r), Some(Row::Line(l)) if *l == i) {
            continue;
        }
        let y = row_y(g, gs.pan, r);
        if y + ROW_H < g.rows_top || y > g.rows_top + g.view_h {
            continue;
        }
        let band = Rect::from_min_max(
            pos2(g.nums.left() - 2.0, y),
            pos2(g.grid.right(), y + ROW_H),
        );
        p.rect_filled(band, 0.0, focus.color.gamma_multiply(0.22));
        p.rect_stroke(
            band,
            0.0,
            Stroke::new(if focus.review && k == 0 { 2.0 } else { 1.2 }, focus.color),
            StrokeKind::Inside,
        );
        if let Some(c) = ev.cell {
            let ncols = v.metrics.widths.len();
            let rect = match kv_of(map, v.recipe, i) {
                Some(kv)
                    if kv.cell == c
                        && map.roles.get(i) != Some(&stokesforge_import::LineRole::Data) =>
                {
                    value_rect(&kv, r)
                }
                _ if c < ncols => {
                    let x = g.grid.left() + v.metrics.colx[c] - sx;
                    let w = v.metrics.widths[c];
                    Rect::from_min_size(pos2(x - 2.0, y + 1.0), vec2(w + 4.0, ROW_H - 2.0))
                }
                _ => continue,
            };
            p.rect_stroke(
                rect,
                3.0,
                Stroke::new(2.0, focus.color),
                StrokeKind::Outside,
            );
        }
    }
    // Loupe: the first cells of the evidence line, larger.
    if focus.review {
        if let Some(ev) = focus.ev.first() {
            let i = ev.line.saturating_sub(1);
            if i < map.line_count() {
                let r = row_of_line(rows, i);
                let y = row_y(g, gs.pan, r);
                paint_loupe(ui, g, v, i, y, focus.color);
            }
        }
    }
}

fn paint_loupe(ui: &egui::Ui, g: &Geom, v: &View<'_>, i: usize, row_top: f32, color: Color32) {
    let tokens = v.map.tokens(i, v.recipe);
    let cell_w = 92.0;
    let n = (((g.grid.width() - 24.0) / cell_w).floor() as usize).clamp(4, 10);
    let card_w = cell_w * n as f32 + 20.0;
    let card_h = 66.0;
    let x = (g.grid.left() + 14.0)
        .min(g.grid.right() - card_w - 4.0)
        .max(g.grid.left());
    let mut y = row_top + ROW_H + 8.0;
    if y + card_h > g.rows_top + g.view_h {
        y = (row_top - card_h - 8.0).max(g.rows_top + 2.0);
    }
    let card = Rect::from_min_size(pos2(x, y), vec2(card_w, card_h));
    let painter = ui.painter().with_clip_rect(g.body);
    // A soft shadow, then the card.
    painter.rect_filled(
        card.translate(vec2(2.0, 3.0)),
        8.0,
        ui.visuals().text_color().gamma_multiply(0.18),
    );
    painter.rect_filled(card, 8.0, ui.visuals().window_fill());
    painter.rect_stroke(card, 8.0, Stroke::new(2.0, color), StrokeKind::Inside);
    painter.text(
        card.left_top() + vec2(10.0, 8.0),
        Align2::LEFT_TOP,
        tr!("import-check-loupe", line = (i + 1).to_string()),
        FontId::proportional(10.5),
        ui.visuals().weak_text_color(),
    );
    let font = FontId::monospace(18.0);
    let char_w = painter
        .layout_no_wrap("M".into(), font.clone(), Color32::WHITE)
        .size()
        .x;
    for c in 0..n {
        let cell = Rect::from_min_size(
            pos2(card.left() + 10.0 + c as f32 * cell_w, card.top() + 24.0),
            vec2(cell_w - 4.0, 34.0),
        );
        let text = tokens.get(c).map_or("", |t| t.trim());
        if text.is_empty() {
            painter.rect_filled(cell, 4.0, ui.visuals().faint_bg_color);
            hatch(&painter.with_clip_rect(cell), cell, color);
            painter.text(
                cell.center(),
                Align2::CENTER_CENTER,
                tr!("import-check-loupe-empty"),
                FontId::proportional(12.0),
                ui.visuals().strong_text_color(),
            );
        } else {
            painter.rect_filled(cell, 4.0, ui.visuals().extreme_bg_color);
            let chars = ((cell.width() - 8.0) / char_w).floor().max(1.0) as usize;
            painter.text(
                cell.center(),
                Align2::CENTER_CENTER,
                fit_text(text, chars),
                font.clone(),
                ui.visuals().text_color(),
            );
        }
        painter.rect_stroke(
            cell,
            4.0,
            Stroke::new(1.0, color.gamma_multiply(0.8)),
            StrokeKind::Inside,
        );
    }
    let _ = LETTER_H;
}

/// The banner shown over the grid while a line is awaited.
pub(super) fn pick_banner(ui: &egui::Ui, g: &Geom, kind: CheckKind) {
    let text = match kind {
        CheckKind::XPrime => tr!("import-check-pick-times"),
        _ => tr!("import-check-pick-last"),
    };
    let rect = Rect::from_min_size(
        pos2(g.grid.left(), g.rows_top + 4.0),
        vec2(g.grid.width().min(520.0), 30.0),
    );
    let accent = theme::accent();
    let p = ui.painter().with_clip_rect(g.body);
    p.rect_filled(rect, 8.0, accent);
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(13.5),
        theme::text_on_accent(),
    );
}
