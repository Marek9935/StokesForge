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
    ui.horizontal_centered(|ui| {
        ui.add_space(theme::SPACE_ITEM);
        let warn = theme::stale_color(ui.visuals().dark_mode);
        let spacing = ui.spacing().item_spacing.x;
        // What stays whole in a narrow panel, in this order: the button, the box,
        // "Values edited"; the rest (a long reason, then the origin) is cut with an
        // ellipsis and read in full on hover. Otherwise Recalculate / Relink is
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
        let state_text = match (&info.stuck, info.stale) {
            (Some(reason), _) => Some(tr!("transpose-state-stuck", reason = reason.as_str())),
            (None, true) => Some(tr!("transpose-state-stale")),
            (None, false) => None,
        };
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
                // A truncated label shows its full text on hover by itself.
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

    /// A followed sheet that cannot catch up reads Out of date and names the
    /// column (E7).
    #[test]
    fn a_followed_sheet_that_cannot_catch_up_names_the_column() {
        let (session, mut state, source, sheet, _first) = bench();
        let handle = session.handle();
        handle
            .execute(stokesforge_commands::Command::SetTransposeAuto { sheet, auto: true })
            .unwrap();
        handle
            .execute(stokesforge_commands::Command::AddColumn {
                sheet: source,
                index: None,
                name: Some("New".into()),
                column: Column::from_f64(&[1.0, 2.0]),
            })
            .unwrap();
        state.refresh();
        let info = state.transpose_info(sheet).unwrap();
        let stuck = info.stuck.expect("a reason");
        assert!(stuck.contains("Column C"), "{stuck}");
        let ctx = context();
        let mut actions = Vec::new();
        let shapes = image(&ctx, Vec::new(), |ui| {
            transpose_banner(ui, &state, sheet, &mut actions);
        });
        let found = texts(&shapes);
        text_exact(
            &found,
            &tr!("transpose-state-stuck", reason = stuck.as_str()),
        );
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
            let (session, mut state, source, sheet, first) = bench();
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
            handle
                .execute(stokesforge_commands::Command::SetTransposeAuto { sheet, auto: true })
                .unwrap();
            handle
                .execute(stokesforge_commands::Command::AddColumn {
                    sheet: source,
                    index: None,
                    name: Some("A rather long column name".into()),
                    column: Column::from_f64(&[1.0, 2.0]),
                })
                .unwrap();
            state.refresh();
            let output = frame_of(&ctx, egui::vec2(width, 60.0), Vec::new(), |ui| {
                transpose_banner(ui, &state, sheet, &mut actions);
            });
            let button = if state.transpose_info(sheet).unwrap().modified {
                tr!("transpose-relink")
            } else {
                tr!("transpose-recalculate")
            };
            assert!(inside(&output, &button), "the button is cut off");
        }
    }
}
