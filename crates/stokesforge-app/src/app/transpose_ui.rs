//! Linked transposed sheet (#253): the banner that flags it at the top of the
//! grid, with its state (out of date, values modified) and the button that
//! brings it up to date, and the confirmation that comes before overwriting
//! edited values. Creation and computation live in `stokesforge-app-state`
//! (`state/transpose.rs`); here, only painting and gestures.

use eframe::egui;
use stokesforge_core::NodeId;

use super::dialogs::{modal_confirm, ConfirmOutcome, PlainSize};
use super::tab_strip::TAB_STRIP_HEIGHT;
use crate::theme;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

/// Height of the banner, the same as the sheet tabs.
pub(super) const BANNER_HEIGHT: f32 = TAB_STRIP_HEIGHT;

/// Vertical room between the first row and the second line, and under it.
const REASON_PAD: f32 = 2.0;

/// The second line of a followed sheet that cannot catch up (#463, G1): the
/// whole sentence, wrapped to `width`, never cut. `None` when nothing is wrong.
fn reason_galley(
    ui: &egui::Ui,
    state: &AppState,
    sheet: NodeId,
    width: f32,
) -> Option<std::sync::Arc<egui::Galley>> {
    let reason = state.transpose_info(sheet)?.stuck?;
    let color = theme::stale_color(ui.visuals().dark_mode);
    let font = egui::TextStyle::Body.resolve(ui.style());
    let wrap = (width - 2.0 * theme::SPACE_ITEM).max(24.0);
    Some(ui.painter().layout(
        tr!("transpose-state-stuck", reason = reason.as_str()),
        font,
        color,
        wrap,
    ))
}

/// Height the banner needs in a panel `width` wide: one row, plus the second
/// line while the sheet cannot catch up. The caller reserves it.
pub(super) fn banner_height(ui: &egui::Ui, state: &AppState, sheet: NodeId, width: f32) -> f32 {
    match reason_galley(ui, state, sheet, width) {
        Some(galley) => BANNER_HEIGHT + galley.size().y + 2.0 * REASON_PAD,
        None => BANNER_HEIGHT,
    }
}

/// The banner of `sheet` if it is a linked transposed sheet: where it comes
/// from, what changed, and Recalculate (Relink when values were edited).
/// The caller has reserved its room when `state.transpose_info(sheet)` exists.
pub(super) fn transpose_banner(
    ui: &mut egui::Ui,
    state: &AppState,
    sheet: NodeId,
    actions: &mut Vec<Action>,
) {
    let Some(info) = state.transpose_info(sheet) else {
        return;
    };
    // The heights are computed by `banner_height`: no spacing of its own between
    // the two lines.
    ui.spacing_mut().item_spacing.y = 0.0;
    let width = ui.available_width();
    let second_line = reason_galley(ui, state, sheet, width);
    let row = egui::vec2(width, BANNER_HEIGHT);
    let layout = egui::Layout::left_to_right(egui::Align::Center);
    ui.allocate_ui_with_layout(row, layout, |ui| {
        ui.add_space(theme::SPACE_ITEM);
        let warn = theme::stale_color(ui.visuals().dark_mode);
        let spacing = ui.spacing().item_spacing.x;
        // What stays whole in a narrow panel, in this order: the button, the box,
        // "Values edited"; the origin is cut with an ellipsis and read in full on
        // hover (a reason that cannot be followed has its own second line). Otherwise Recalculate / Relink is
        // pushed out of the window.
        let button = (info.stale || info.modified).then(|| {
            if info.modified {
                (
                    tr!("transpose-relink"),
                    tr!("transpose-relink-hint", source = info.source_name.as_str()),
                )
            } else {
                (
                    tr!("transpose-recalculate"),
                    tr!(
                        "transpose-recalculate-hint",
                        source = info.source_name.as_str()
                    ),
                )
            }
        });
        let button_width = button.as_ref().map_or(0.0, |(label, _)| {
            text_width(ui, label) + 2.0 * ui.spacing().button_padding.x + spacing
        });
        let modified = tr!("transpose-state-modified");
        let modified_width = if info.modified {
            text_width(ui, &modified) + spacing
        } else {
            0.0
        };
        let auto_label = tr!("transpose-auto");
        let box_width = ui.spacing().icon_width
            + ui.spacing().icon_spacing
            + text_width(ui, &auto_label)
            + spacing;
        let state_text = (info.stuck.is_none() && info.stale).then(|| tr!("transpose-state-stale"));
        // The origin keeps at least this much for the states.
        let state_floor = if state_text.is_some() { 24.0 } else { 0.0 } + 3.0 * spacing + 16.0;
        ui.scope(|ui| {
            ui.set_max_width(
                (ui.available_width() - button_width - modified_width - box_width - state_floor)
                    .max(24.0),
            );
            ui.add(
                egui::Label::new(tr!("transpose-banner", source = info.source_name.as_str()))
                    .truncate(),
            );
        });
        // The box (#463). Greyed when the source calculates from this sheet:
        // the reason is its own disabled tooltip and names the column (S-G1, S-G2).
        let sheet_name = state
            .tree()
            .node(sheet)
            .map(|node| node.name().to_string())
            .unwrap_or_default();
        let mut auto = info.auto;
        let blocked = !info.auto && info.auto_blocked.is_some();
        let reason = info.auto_blocked.clone().unwrap_or_default();
        let response = ui
            .add_enabled(!blocked, egui::Checkbox::new(&mut auto, auto_label))
            .on_disabled_hover_text(reason);
        let response = if blocked {
            response
        } else {
            response.on_hover_text(tr!(
                "transpose-auto",
                "hint",
                sheet = sheet_name.as_str(),
                source = info.source_name.as_str()
            ))
        };
        if response.changed() {
            actions.push(Action::SetTransposeAuto { sheet, auto });
        }
        if let Some(text) = state_text {
            ui.scope(|ui| {
                ui.set_max_width((ui.available_width() - button_width - modified_width).max(0.0));
                ui.add(egui::Label::new(egui::RichText::new(text).color(warn)).truncate());
            });
        }
        if info.modified {
            ui.label(egui::RichText::new(modified).color(warn));
        }
        if let Some((label, hint)) = button {
            if ui.button(label).on_hover_text(hint).clicked() {
                actions.push(Action::RecalculateTranspose { sheet });
            }
        }
    });
    if let Some(galley) = second_line {
        ui.add_space(REASON_PAD);
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(width, galley.size().y), egui::Sense::hover());
        let color = theme::stale_color(ui.visuals().dark_mode);
        ui.painter()
            .galley(rect.min + egui::vec2(theme::SPACE_ITEM, 0.0), galley, color);
    }
}

/// Width of `text` in the body font.
fn text_width(ui: &egui::Ui, text: &str) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    ui.painter()
        .layout_no_wrap(text.to_owned(), font, egui::Color32::WHITE)
        .size()
        .x
}

/// Overwriting edited values in the linked columns: confirmation first
/// (Ctrl+Z undoes the gesture, but the user is warned first).
pub(super) fn transpose_relink_window(
    ctx: &egui::Context,
    state: &AppState,
    actions: &mut Vec<Action>,
) {
    let Some(sheet) = state.pending_transpose_relink() else {
        return;
    };
    let name = state
        .tree()
        .node(sheet)
        .map(|node| node.name().to_string())
        .unwrap_or_default();
    let source = state
        .transpose_info(sheet)
        .map(|info| info.source_name)
        .unwrap_or_default();
    let outcome = modal_confirm(
        ctx,
        egui::Id::new("stokesforge_transpose_relink"),
        &tr!("transpose-relink-title"),
        PlainSize::Default,
        &tr!("transpose-relink"),
        &tr!("transpose-relink-hint", source = source.as_str()),
        |ui| {
            ui.label(tr!("transpose-relink-confirm", sheet = name.as_str()));
        },
    );
    match outcome {
        ConfirmOutcome::Confirm => actions.push(Action::ConfirmTransposeRelink(true)),
        ConfirmOutcome::Cancel => actions.push(Action::ConfirmTransposeRelink(false)),
        ConfirmOutcome::Open => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{click, context, frame_of, image, painted, text_exact, texts};
    use super::*;
    use stokesforge_core::{Column, ColumnMeta, Project, ValueBlock, TRANSPOSE_MARK};
    use stokesforge_session::Session;

    /// The name "Sheet1ᵀ" is written with a character the embedded font must
    /// carry: otherwise an empty square appears in the middle of the interface.
    #[test]
    fn the_embedded_font_shows_the_superscript_t() {
        let ctx = context();
        frame_of(&ctx, egui::vec2(100.0, 100.0), Vec::new(), |_| {});
        // The proportional font carries sheet names (tabs, managers, titles).
        // The monospace font (console) does not carry it.
        let has = ctx
            .fonts_mut(|fonts| fonts.has_glyph(&egui::FontId::proportional(14.0), TRANSPOSE_MARK));
        assert!(has, "no glyph for {TRANSPOSE_MARK}");
    }

    /// A source sheet with x' and its transpose; `source` is then modified to
    /// make it out of date.
    fn bench() -> (Session, AppState, NodeId, NodeId, NodeId) {
        let mut project = Project::new("T").unwrap();
        let book = project.add_workbook(project.root(), Some("Book")).unwrap();
        let source = project.tree().children(book).unwrap()[0];
        let mut first = None;
        for i in 0..2 {
            let mut column = Column::from_f64(&[1.0, 2.0]);
            column.set_meta(ColumnMeta {
                x_prime: Some(f64::from(i)),
                ..ColumnMeta::default()
            });
            let id = project
                .add_column(source, None, Some(&format!("S{i}")), column)
                .unwrap();
            first.get_or_insert(id);
        }
        let sheet = project.transpose_rows(source, &[0]).unwrap();
        // These cases pin the snapshot banner (#253): the box is unchecked.
        project.set_transpose_auto(sheet, false).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "t.stf".into()).unwrap();
        (session, state, source, sheet, first.unwrap())
    }

    #[test]
    fn the_banner_states_the_origin_and_offers_recalculate_only_when_needed() {
        let (session, mut state, _source, sheet, first) = bench();
        let ctx = context();
        let mut actions = Vec::new();
        let output = frame_of(&ctx, egui::vec2(700.0, 60.0), Vec::new(), |ui| {
            transpose_banner(ui, &state, sheet, &mut actions);
        });
        assert!(painted(&output, "Transposed from Sheet1"));
        assert!(
            !output.shapes.iter().any(|p| matches!(&p.shape,
                egui::Shape::Text(t) if t.galley.job.text == tr!("transpose-recalculate"))),
            "up to date: no button"
        );

        session
            .handle()
            .execute(stokesforge_commands::Command::SetCells {
                column: first,
                start: 0,
                values: ValueBlock::Float(vec![9.0]),
                expected_version: None,
            })
            .unwrap();
        state.refresh();
        let shapes = image(&ctx, Vec::new(), |ui| {
            transpose_banner(ui, &state, sheet, &mut actions);
        });
        let found = texts(&shapes);
        text_exact(&found, &tr!("transpose-state-stale"));
        let button = text_exact(&found, &tr!("transpose-recalculate")).center;
        click(&ctx, button, egui::PointerButton::Primary, |ui| {
            transpose_banner(ui, &state, sheet, &mut actions);
        });
        assert_eq!(actions, vec![Action::RecalculateTranspose { sheet }]);
    }

    #[test]
    fn an_edited_value_offers_to_relink() {
        let (session, mut state, _source, sheet, _first) = bench();
        let row = state.tree().children(sheet).unwrap()[1];
        session
            .handle()
            .execute(stokesforge_commands::Command::SetCells {
                column: row,
                start: 0,
                values: ValueBlock::Float(vec![-1.0]),
                expected_version: None,
            })
            .unwrap();
        state.refresh();
        let ctx = context();
        let mut actions = Vec::new();
        let shapes = image(&ctx, Vec::new(), |ui| {
            transpose_banner(ui, &state, sheet, &mut actions);
        });
        let found = texts(&shapes);
        text_exact(&found, &tr!("transpose-state-modified"));
        text_exact(&found, &tr!("transpose-relink"));
        assert!(found.iter().all(|t| t.text != tr!("transpose-recalculate")));
    }

    #[test]
    fn the_confirmation_opens_only_on_request() {
        let (_session, mut state, _source, sheet, _first) = bench();
        let ctx = context();
        let mut actions = Vec::new();
        let output = frame_of(&ctx, egui::vec2(900.0, 600.0), Vec::new(), |ui| {
            transpose_relink_window(ui.ctx(), &state, &mut actions);
        });
        assert!(!painted(&output, &tr!("transpose-relink-title")));
        // Edits a linked value, then asks for the update.
        let row = state.tree().children(sheet).unwrap()[1];
        _session
            .handle()
            .execute(stokesforge_commands::Command::SetCells {
                column: row,
                start: 0,
                values: ValueBlock::Float(vec![-1.0]),
                expected_version: None,
            })
            .unwrap();
        state.refresh();
        state.dispatch(Action::RecalculateTranspose { sheet });
        let mut shown = false;
        for _ in 0..3 {
            let shapes = image(&ctx, Vec::new(), |ui| {
                transpose_relink_window(ui.ctx(), &state, &mut actions);
            });
            shown |= texts(&shapes)
                .iter()
                .any(|t| t.text == tr!("transpose-relink-title"));
        }
        assert!(shown, "the confirmation opens");
    }

    // -- #463: Recalculate automatically --------------------------------------

    /// A checked sheet shows the box checked; clicking it sends the action that
    /// unchecks. Witness: the banner without the box.
    #[test]
    fn the_box_is_checked_by_default_and_a_click_unchecks_it() {
        let (session, mut state, _source, sheet, _first) = bench();
        session
            .handle()
            .execute(stokesforge_commands::Command::SetTransposeAuto { sheet, auto: true })
            .unwrap();
        state.refresh();
        assert!(state.transpose_info(sheet).unwrap().auto);
        let ctx = context();
        let mut actions = Vec::new();
        let shapes = image(&ctx, Vec::new(), |ui| {
            transpose_banner(ui, &state, sheet, &mut actions);
        });
        let found = texts(&shapes);
        let label = text_exact(&found, &tr!("transpose-auto"));
        click(&ctx, label.center, egui::PointerButton::Primary, |ui| {
            transpose_banner(ui, &state, sheet, &mut actions);
        });
        assert_eq!(
            actions,
            vec![Action::SetTransposeAuto { sheet, auto: false }]
        );
    }

    /// The source calculates from the unchecked sheet: the box is greyed and a
    /// click does nothing; the reason names the column (S-G1, S-G2). Witness: a
    /// box that is always enabled.
    #[test]
    fn the_box_is_greyed_while_the_source_calculates_from_the_sheet() {
        let (session, mut state, source, sheet, _first) = bench();
        let tree = state.tree().clone();
        let reader = *tree.children(source).unwrap().last().unwrap();
        let name = tree.node(sheet).unwrap().name().to_string();
        let text = format!("max('{name}'!B)");
        let formula = tree.formula_from_letters(reader, &text).unwrap();
        session
            .handle()
            .execute(stokesforge_commands::Command::SetColumnMeta {
                column: reader,
                meta: ColumnMeta {
                    formula: Some(formula),
                    ..ColumnMeta::default()
                },
            })
            .unwrap();
        state.refresh();
        let info = state.transpose_info(sheet).unwrap();
        let reason = info.auto_blocked.expect("a reason");
        assert!(reason.contains("Column B of Sheet1"), "{reason}");
        let ctx = context();
        let mut actions = Vec::new();
        let shapes = image(&ctx, Vec::new(), |ui| {
            transpose_banner(ui, &state, sheet, &mut actions);
        });
        let found = texts(&shapes);
        let label = text_exact(&found, &tr!("transpose-auto"));
        click(&ctx, label.center, egui::PointerButton::Primary, |ui| {
            transpose_banner(ui, &state, sheet, &mut actions);
        });
        assert!(actions.is_empty(), "{actions:?}");
    }

    /// A followed sheet whose source got a column without x'.
    fn stuck_bench(column_name: &str) -> (Session, AppState, NodeId) {
        let (session, mut state, source, sheet, _first) = bench();
        let handle = session.handle();
        handle
            .execute(stokesforge_commands::Command::SetTransposeAuto { sheet, auto: true })
            .unwrap();
        handle
            .execute(stokesforge_commands::Command::AddColumn {
                sheet: source,
                index: None,
                name: Some(column_name.into()),
                column: Column::from_f64(&[1.0, 2.0]),
            })
            .unwrap();
        state.refresh();
        (session, state, sheet)
    }

    /// The galley of the painted text `wanted`, with its top-left corner.
    fn galley_of(
        output: &egui::FullOutput,
        wanted: &str,
    ) -> (egui::Pos2, std::sync::Arc<egui::Galley>) {
        let found: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|p| match &p.shape {
                egui::Shape::Text(t) if t.galley.job.text == wanted => {
                    Some((t.pos, t.galley.clone()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(found.len(), 1, "{wanted:?} must be painted once");
        found.into_iter().next().unwrap()
    }

    /// A followed sheet that cannot catch up writes Out of date and names the
    /// column in full on a second line, under the banner row, in orange (G1).
    /// Witness: the sentence left on the first row.
    #[test]
    fn a_followed_sheet_that_cannot_catch_up_names_the_column_on_a_second_line() {
        let (_session, state, sheet) = stuck_bench("New");
        let stuck = state
            .transpose_info(sheet)
            .unwrap()
            .stuck
            .expect("a reason");
        assert!(stuck.contains("Column C"), "{stuck}");
        let sentence = tr!("transpose-state-stuck", reason = stuck.as_str());
        let ctx = context();
        let mut actions = Vec::new();
        let width = 700.0;
        let output = frame_of(&ctx, egui::vec2(width, 120.0), Vec::new(), |ui| {
            transpose_banner(ui, &state, sheet, &mut actions);
        });
        let (pos, galley) = galley_of(&output, &sentence);
        assert!(
            pos.y >= BANNER_HEIGHT,
            "second line under the row: {}",
            pos.y
        );
        assert_eq!(galley.rows.len(), 1, "one line when there is room");
        let first = galley.job.sections[0].format.color;
        assert_eq!(
            first,
            theme::stale_color(ctx.global_style().visuals.dark_mode)
        );
        let button = galley_of(&output, &tr!("transpose-recalculate")).0;
        assert!(
            button.y < BANNER_HEIGHT,
            "the button stays on the first row"
        );
        // The first row no longer carries a truncated copy of it.
        let copies = output
            .shapes
            .iter()
            .filter(|p| {
                matches!(&p.shape,
                egui::Shape::Text(t) if t.galley.job.text.contains("Out of date"))
            })
            .count();
        assert_eq!(copies, 1, "the sentence is painted once");
    }

    /// Nothing wrong: the banner is one row, as before. Witness: a second line
    /// (or its room) reserved all the time.
    #[test]
    fn a_banner_with_no_problem_stays_on_one_row() {
        let (_session, mut state, _source, sheet, _first) = bench();
        let ctx = context();
        let ui_height = |state: &AppState| {
            let mut height = 0.0;
            frame_of(&ctx, egui::vec2(700.0, 120.0), Vec::new(), |ui| {
                height = banner_height(ui, state, sheet, 700.0);
            });
            height
        };
        assert_eq!(ui_height(&state), BANNER_HEIGHT);
        // Out of date without a reason (box unchecked, source edited): one row.
        let first = state
            .tree()
            .children(state.transpose_info(sheet).unwrap().source)
            .unwrap()[0];
        _session
            .handle()
            .execute(stokesforge_commands::Command::SetCells {
                column: first,
                start: 0,
                values: ValueBlock::Float(vec![9.0]),
                expected_version: None,
            })
            .unwrap();
        state.refresh();
        assert!(state.transpose_info(sheet).unwrap().stale);
        assert_eq!(ui_height(&state), BANNER_HEIGHT);
        let mut actions = Vec::new();
        let output = frame_of(&ctx, egui::vec2(700.0, 120.0), Vec::new(), |ui| {
            transpose_banner(ui, &state, sheet, &mut actions);
        });
        let (pos, _) = galley_of(&output, &tr!("transpose-state-stale"));
        assert!(pos.y < BANNER_HEIGHT, "plain Out of date stays on the row");
        // The problem arises: the room grows by the second line, and goes back.
        let (_s2, stuck_state, stuck_sheet) = stuck_bench("New");
        let mut with_line = 0.0;
        frame_of(&ctx, egui::vec2(700.0, 120.0), Vec::new(), |ui| {
            with_line = banner_height(ui, &stuck_state, stuck_sheet, 700.0);
        });
        assert!(with_line > BANNER_HEIGHT + 10.0, "{with_line}");
    }

    /// In a narrow panel the button still fits when both states show, and a long
    /// reason is cut instead of pushing it out of the window. Witness: states
    /// laid out before the button without measuring it.
    #[test]
    fn the_button_stays_inside_a_narrow_banner() {
        for width in [520.0_f32, 460.0] {
            let inside = |output: &egui::FullOutput, wanted: &str| {
                output.shapes.iter().any(|p| match &p.shape {
                    egui::Shape::Text(t) if t.galley.job.text == wanted => {
                        t.pos.x + t.galley.size().x <= width
                    }
                    _ => false,
                })
            };
            // Out of date and Values edited together, box unchecked.
            let (session, mut state, _source, sheet, first) = bench();
            let row = state.tree().children(sheet).unwrap()[1];
            let handle = session.handle();
            handle
                .execute(stokesforge_commands::Command::SetCells {
                    column: row,
                    start: 0,
                    values: ValueBlock::Float(vec![-1.0]),
                    expected_version: None,
                })
                .unwrap();
            handle
                .execute(stokesforge_commands::Command::SetCells {
                    column: first,
                    start: 0,
                    values: ValueBlock::Float(vec![9.0]),
                    expected_version: None,
                })
                .unwrap();
            state.refresh();
            let info = state.transpose_info(sheet).unwrap();
            assert!(info.stale && info.modified);
            let ctx = context();
            let mut actions = Vec::new();
            let output = frame_of(&ctx, egui::vec2(width, 60.0), Vec::new(), |ui| {
                transpose_banner(ui, &state, sheet, &mut actions);
            });
            assert!(
                inside(&output, &tr!("transpose-relink")),
                "Relink is cut off"
            );
            assert!(inside(&output, &tr!("transpose-state-modified")));

            // A followed sheet with a column that cannot be followed: long reason.
            let (_session, state, sheet) = stuck_bench("A rather long column name");
            let (mut top, mut needed) = (0.0, 0.0);
            let output = frame_of(&ctx, egui::vec2(width, 120.0), Vec::new(), |ui| {
                top = ui.max_rect().top();
                needed = banner_height(ui, &state, sheet, ui.available_width());
                transpose_banner(ui, &state, sheet, &mut actions);
            });
            let button = tr!("transpose-recalculate");
            assert!(inside(&output, &button), "the button is cut off");
            // The long sentence is on a second line, whole (it wraps), inside.
            let stuck = state
                .transpose_info(sheet)
                .unwrap()
                .stuck
                .expect("a reason");
            let sentence = tr!("transpose-state-stuck", reason = stuck.as_str());
            let (pos, galley) = galley_of(&output, &sentence);
            assert!(pos.y >= top + BANNER_HEIGHT, "second line");
            assert!(pos.x + galley.size().x <= width, "the sentence is cut off");
            assert!(
                pos.y + galley.size().y <= top + needed,
                "room reserved for all lines"
            );
        }
    }
}
