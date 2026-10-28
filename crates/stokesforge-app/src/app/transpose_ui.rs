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
        ui.label(tr!("transpose-banner", source = info.source_name.as_str()));
        let warn = theme::stale_color(ui.visuals().dark_mode);
        if info.stale {
            ui.label(egui::RichText::new(tr!("transpose-state-stale")).color(warn));
        }
        if info.modified {
            ui.label(egui::RichText::new(tr!("transpose-state-modified")).color(warn));
        }
        if info.stale || info.modified {
            let (label, hint) = if info.modified {
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
            };
            if ui.button(label).on_hover_text(hint).clicked() {
                actions.push(Action::RecalculateTranspose { sheet });
            }
        }
    });
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
        assert!(!painted(&output, "Recalculate"), "up to date: no button");

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
}
