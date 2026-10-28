//! The Results pane of a workbook (#181, mockup M7): below the grid, above
//! the sheet tabs, the values of the "Keep in Results" calculations.
//!
//! Collapsed by default (a single banner with the count), absent while the
//! workbook has no kept result. Expanded: the list of calculations on the
//! left, the detail on the right (one row per series, one column per value)
//! with `Edit`, `Recalculate` and `Delete`. The `This sheet | Workbook`
//! button chooses what the list shows. The open state and the chosen
//! calculation belong to the interface (egui memory): they go neither into
//! the `.stf` nor into the history.

use eframe::egui;
use stokesforge_core::NodeId;

use super::analysis_dialog::{exact_result_label, format_value};
use super::analysis_handles::select_calculation;
use super::empty_state::empty_state;
use super::form::{choice, ChoiceOption};
use super::list_row::list_row;
use crate::icons::{icon_button, Icon, IconRole};
use crate::theme;
use stokesforge_app_state::analysis::{
    kind_title, reference_label, series_label, value_label, value_unit, with_unit,
};
use stokesforge_app_state::r_squared::level_of_value;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

/// Height of the collapsed banner.
pub(super) const COLLAPSED_HEIGHT: f32 = 22.0;
/// Height of the expanded pane.
const EXPANDED_HEIGHT: f32 = 210.0;
/// Width of the list of calculations.
const LIST_WIDTH: f32 = 240.0;

#[derive(Clone, Copy, Default)]
struct PaneState {
    open: bool,
    whole_book: bool,
    selected: Option<NodeId>,
}

fn pane_id(book: NodeId) -> egui::Id {
    egui::Id::new(("stokesforge_results_pane", book.raw()))
}

/// The height the pane takes in the workbook: nothing without a kept result.
pub(super) fn pane_height(ui: &egui::Ui, state: &AppState, book: NodeId) -> f32 {
    if state.kept_calculations(book, None).is_empty()
        && state.correction_traces(book, None).is_empty()
    {
        return 0.0;
    }
    let open = ui
        .data(|d| d.get_temp::<PaneState>(pane_id(book)))
        .unwrap_or_default()
        .open;
    if open {
        EXPANDED_HEIGHT
    } else {
        COLLAPSED_HEIGHT
    }
}

/// Paints the pane in `ui`, whose rectangle has the height of [`pane_height`].
pub(super) fn results_pane(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    book: NodeId,
    sheet: NodeId,
) {
    let id = pane_id(book);
    let mut pane = ui.data(|d| d.get_temp::<PaneState>(id)).unwrap_or_default();
    let total =
        state.kept_calculations(book, None).len() + state.correction_traces(book, None).len();
    let rect = ui.max_rect();
    if !ui.visuals().dark_mode {
        ui.painter()
            .rect_filled(rect, 0.0, theme::results_pane_fill(ui));
    }
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    // Banner of fixed height, content vertically centered (otherwise the title sticks to the top).
    let header = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), COLLAPSED_HEIGHT));
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(header)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            let (icon, tip) = if pane.open {
                (Icon::CaretDown, tr!("results-collapse"))
            } else {
                (Icon::CaretRight, tr!("results-expand"))
            };
            if icon_button(ui, icon, IconRole::Neutral, tip).clicked() {
                pane.open = !pane.open;
            }
            ui.label(theme::bold(tr!(
                "results-title",
                count = i64::try_from(total).unwrap_or(i64::MAX)
            )));
            if pane.open {
                choice(
                    ui,
                    id.with("scope"),
                    &mut pane.whole_book,
                    &[
                        ChoiceOption::new(false, tr!("results-scope-sheet")),
                        ChoiceOption::new(true, tr!("results-scope-book")),
                    ],
                );
            }
        },
    );
    ui.advance_cursor_after_rect(header);
    if pane.open {
        let scope = (!pane.whole_book).then_some(sheet);
        let mut calculations = state.kept_calculations(book, scope);
        calculations.extend(state.correction_traces(book, scope));
        if !calculations.contains(&pane.selected.unwrap_or(NodeId::from_raw(0))) {
            pane.selected = calculations.first().copied();
        }
        // Two areas with bounded rectangles: the list never exceeds its share and
        // the detail wraps in the rest, without widening the window.
        let area = ui.available_rect_before_wrap();
        let list_width = LIST_WIDTH.min(area.width() * 0.45);
        let list_rect = egui::Rect::from_min_size(area.min, egui::vec2(list_width, area.height()));
        let detail_rect = egui::Rect::from_min_max(
            egui::pos2(list_rect.right() + ui.spacing().item_spacing.x, area.top()),
            area.max,
        );
        ui.advance_cursor_after_rect(area);
        {
            ui.scope_builder(egui::UiBuilder::new().max_rect(list_rect), |ui| {
                ui.set_clip_rect(list_rect.intersect(ui.clip_rect()));
                {
                    egui::ScrollArea::vertical()
                        .id_salt(id.with("list"))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if calculations.is_empty() {
                                empty_state(ui, tr!("results-empty"));
                            }
                            for calc in &calculations {
                                let title = row_title(state, *calc);
                                if list_row(ui, None, &title, pane.selected == Some(*calc), false)
                                    .clicked()
                                {
                                    pane.selected = Some(*calc);
                                    select_calculation(ui.ctx(), pane.selected);
                                }
                            }
                        });
                }
            });
            ui.scope_builder(egui::UiBuilder::new().max_rect(detail_rect), |ui| {
                ui.set_clip_rect(detail_rect.intersect(ui.clip_rect()));
                if let Some(calc) = pane.selected {
                    detail(ui, state, actions, id, calc);
                }
            });
        }
    }
    ui.data_mut(|d| d.insert_temp(id, pane));
}

/// The list row, short enough to fit in the list: "Baseline · Spectrum/B"
/// (operation, sheet and source column), "(out of date)" if needed.
fn row_title(state: &AppState, calc: NodeId) -> String {
    let tree = state.tree();
    let (kind, first, stale) = match tree.correction_traces().get(&calc) {
        Some(trace) => (trace.operation.kind(), trace.series.first(), false),
        None => match tree.calculation(calc) {
            Some(c) => (c.operation.kind(), c.series.first(), c.stale.is_some()),
            None => return String::new(),
        },
    };
    let name = |id: NodeId| tree.node(id).map(|n| n.name().to_string());
    let place = first
        .and_then(|s| {
            let sheet = tree.node(s.y).and_then(|n| n.parent()).and_then(name);
            Some(match sheet {
                Some(sheet) => format!("{sheet}/{}", name(s.y)?),
                None => name(s.y)?,
            })
        })
        .unwrap_or_default();
    let title = tr!(
        "results-row",
        kind = kind_title(kind),
        place = place.as_str()
    );
    if stale {
        tr!("results-stale-suffix", title = title.as_str())
    } else {
        title
    }
}

/// The detail of a calculation: its gestures, then the series × value table.
fn detail(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    id: egui::Id,
    calc_id: NodeId,
) {
    if let Some(trace) = state.tree().correction_traces().get(&calc_id) {
        ui.horizontal_wrapped(|ui| {
            ui.label(theme::bold(tr!(
                "analysis-correction-trace",
                kind = kind_title(trace.operation.kind())
            )));
            if ui
                .add_enabled(
                    !trace.originals.is_empty(),
                    egui::Button::new(tr!("action-restore-original-data")),
                )
                .on_hover_text(tr!("action-restore-original-data", "hint"))
                .on_disabled_hover_text(tr!("action-restore-original-data", "disabled"))
                .clicked()
            {
                actions.push(Action::RestoreOriginalData(calc_id));
            }
        });
        ui.label(tr!("results-replace-hint"));
        egui::ScrollArea::vertical()
            .id_salt(id.with(("trace", calc_id.raw())))
            .show(ui, |ui| {
                for series in &trace.series {
                    ui.label(series_label(state.tree(), series));
                }
                if let Some(aux) = &trace.aux {
                    ui.label(tr!("analysis-field-reference"));
                    ui.label(reference_label(state.tree(), aux));
                }
                correction_settings(ui, &trace.operation);
            });
        return;
    }
    let Some(calc) = state.tree().calculation(calc_id) else {
        return;
    };
    // The frozen reference of a correction appears only through this line (#248).
    let reference = calc
        .aux
        .as_ref()
        .filter(|_| calc.operation.kind() == stokesforge_analysis::ops::OperationKind::Correct)
        .map(|aux| reference_label(state.tree(), aux));
    ui.horizontal_wrapped(|ui| {
        ui.label(theme::bold(kind_title(calc.operation.kind())));
        if ui
            .button(tr!("action-calculation-edit"))
            .on_hover_text(tr!("action-calculation-edit", "hint"))
            .clicked()
        {
            actions.push(Action::EditCalculation(calc_id));
        }
        if ui
            .add_enabled(
                calc.stale.is_some(),
                egui::Button::new(tr!("action-calculation-recalculate")),
            )
            .on_hover_text(tr!("action-calculation-recalculate", "hint"))
            .on_disabled_hover_text(tr!("action-calculation-recalculate", "disabled"))
            .clicked()
        {
            actions.push(Action::RecalculateCalculations(vec![calc_id]));
        }
        if ui
            .button(tr!("action-calculation-delete"))
            .on_hover_text(tr!("action-calculation-delete", "hint"))
            .clicked()
        {
            actions.push(Action::RequestDeleteCalculation(calc_id));
        }
    });
    if let Some(reference) = reference {
        ui.label(tr!(
            "analysis-reference-line",
            reference = reference.as_str()
        ));
    }
    // The table columns: each (key, rank) seen, in order of appearance.
    let mut columns: Vec<(String, Option<usize>)> = Vec::new();
    for result in &calc.results {
        for value in &result.values {
            let entry = (value.key.clone(), value.index);
            if !columns.contains(&entry) {
                columns.push(entry);
            }
        }
    }
    if columns.is_empty() {
        ui.label(egui::RichText::new(tr!("results-no-values")).weak());
        return;
    }
    egui::ScrollArea::both()
        .id_salt(id.with(("table", calc_id.raw())))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new(id.with(("grid", calc_id.raw())))
                .striped(true)
                .show(ui, |ui| {
                    ui.label("");
                    for (key, index) in &columns {
                        ui.label(theme::table_header(value_label(key, *index)));
                    }
                    ui.end_row();
                    for (rank, series) in calc.series.iter().enumerate() {
                        ui.label(series_label(state.tree(), series));
                        let values = calc.results.get(rank);
                        for (key, index) in &columns {
                            let found = values.and_then(|r| {
                                r.values.iter().find(|v| &v.key == key && v.index == *index)
                            });
                            match found {
                                Some(v) => {
                                    exact_result_label(
                                        ui,
                                        with_unit(
                                            format_value(v.value, v.error),
                                            &value_unit(state.tree(), series, key),
                                        ),
                                        v.value,
                                        v.error,
                                        level_of_value(&v.key, v.value),
                                    );
                                }
                                None => {
                                    ui.label("");
                                }
                            }
                        }
                        ui.end_row();
                    }
                });
        });
}

/// The kept settings are shown as values, with no possible edit.
fn correction_settings(ui: &mut egui::Ui, operation: &stokesforge_analysis::ops::Operation) {
    use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp, Interpolation};
    use stokesforge_analysis::correct::CorrectOp;
    use stokesforge_analysis::ops::Operation;
    let pair = |a: f64, b: f64| format!("({}, {})", format_value(a, None), format_value(b, None));
    match operation {
        Operation::Baseline { method, op, .. } => {
            ui.label(match op {
                BaselineOp::Subtract => tr!("analysis-baseline-subtract"),
                BaselineOp::Divide => tr!("analysis-baseline-divide"),
            });
            match method {
                BaselineMethod::Constant { point } => {
                    ui.label(tr!("analysis-baseline-constant"));
                    ui.label(pair(point.0, point.1));
                }
                BaselineMethod::ConstantZone { zone } => {
                    ui.label(tr!("analysis-baseline-shape-constant-zone"));
                    ui.label(pair(zone.0, zone.1));
                }
                BaselineMethod::Line { p1, p2 } => {
                    ui.label(tr!("analysis-baseline-shape-line"));
                    ui.label(pair(p1.0, p1.1));
                    ui.label(pair(p2.0, p2.1));
                }
                BaselineMethod::Zones { zones, degree } => {
                    ui.label(tr!("analysis-baseline-shape-zones"));
                    ui.label(tr!("analysis-baseline-degree"));
                    ui.label(degree.to_string());
                    for p in zones {
                        ui.label(pair(p.0, p.1));
                    }
                }
                BaselineMethod::Anchors {
                    points,
                    interpolation,
                } => {
                    ui.label(tr!("analysis-baseline-anchors"));
                    ui.label(match interpolation {
                        Interpolation::Segments => tr!("analysis-baseline-segments"),
                        Interpolation::Spline => tr!("analysis-baseline-spline"),
                    });
                    for p in points {
                        ui.label(pair(p.0, p.1));
                    }
                }
            }
        }
        Operation::Correct { op } => {
            ui.label(match op {
                CorrectOp::Subtract => tr!("analysis-correct-subtract"),
                CorrectOp::Divide => tr!("analysis-correct-divide"),
                CorrectOp::Multiply => tr!("analysis-correct-multiply"),
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{click, context, frame_of, painted, text_exact, texts};
    use super::*;
    use stokesforge_app_state::analysis::Preset;
    use stokesforge_core::{Column, ColumnMeta, ColumnRole, Project};
    use stokesforge_session::Session;

    /// A workbook with an integral kept in the results.
    fn bench() -> (Session, AppState, NodeId, NodeId) {
        let mut project = Project::new("Results").unwrap();
        let book = project.add_workbook(project.root(), Some("Book")).unwrap();
        let sheet = project.tree().children(book).unwrap()[0];
        let xs: Vec<f64> = (0..11).map(f64::from).collect();
        let x = project
            .add_column(sheet, None, Some("A"), Column::from_f64(&xs))
            .unwrap();
        let y = project
            .add_column(sheet, None, Some("B"), Column::from_f64(&xs))
            .unwrap();
        project
            .set_column_meta(
                x,
                ColumnMeta {
                    role: ColumnRole::X,
                    ..ColumnMeta::default()
                },
            )
            .unwrap();
        let session = Session::spawn(project, None).unwrap();
        let mut state = AppState::new(session.handle(), "test.stf".into()).unwrap();
        state.dispatch(Action::OpenAnalysis {
            preset: Preset::Integrate,
            sheet,
            columns: vec![x, y],
        });
        {
            let form = state.analysis_form_mut().unwrap();
            form.keep_results = true;
            form.setup.integrate.cumulative = true;
        }
        state.dispatch(Action::ApplyAnalysis);
        (session, state, book, sheet)
    }

    fn frame(
        ctx: &egui::Context,
        state: &AppState,
        book: NodeId,
        sheet: NodeId,
        actions: &mut Vec<Action>,
    ) -> egui::FullOutput {
        frame_of(ctx, egui::vec2(900.0, 400.0), Vec::new(), |ui| {
            results_pane(ui, state, actions, book, sheet);
        })
    }

    #[test]
    fn the_pane_is_collapsed_with_its_count_then_expands_into_a_table() {
        let ctx = context();
        let (session, state, book, sheet) = bench();
        assert_eq!(state.kept_calculations(book, None).len(), 1);
        assert_eq!(state.kept_calculations(book, Some(sheet)).len(), 1);
        assert_eq!(
            state
                .kept_calculations(book, Some(NodeId::from_raw(999_999)))
                .len(),
            0
        );
        let mut actions = Vec::new();
        frame(&ctx, &state, book, sheet, &mut actions);
        let output = frame(&ctx, &state, book, sheet, &mut actions);
        assert!(painted(&output, &tr!("results-title", count = 1)));
        assert!(!painted(&output, &tr!("results-scope-book")), "collapsed");

        // The expand button is left of the title.
        let title = text_exact(&texts(&output.shapes), &tr!("results-title", count = 1)).center;
        click(
            &ctx,
            egui::pos2(title.x - 40.0, title.y),
            egui::PointerButton::Primary,
            |ui| results_pane(ui, &state, &mut actions, book, sheet),
        );
        let output = frame(&ctx, &state, book, sheet, &mut actions);
        assert!(painted(&output, &tr!("results-scope-book")), "expanded");
        assert!(painted(&output, &value_label("area.total", None)));
        let edit = text_exact(&texts(&output.shapes), &tr!("action-calculation-edit")).center;
        click(&ctx, edit, egui::PointerButton::Primary, |ui| {
            results_pane(ui, &state, &mut actions, book, sheet)
        });
        let calc = *state.tree().calculations().keys().next().unwrap();
        assert!(
            actions.contains(&Action::EditCalculation(calc)),
            "{actions:?}"
        );
        session.shutdown().unwrap();
    }

    /// #248: the list is single; each row carries its category first.
    #[test]
    fn each_row_carries_its_category_first() {
        let ctx = context();
        let (session, state, book, sheet) = bench();
        let mut actions = Vec::new();
        ctx.data_mut(|d| {
            d.insert_temp(
                pane_id(book),
                PaneState {
                    open: true,
                    ..PaneState::default()
                },
            )
        });
        frame(&ctx, &state, book, sheet, &mut actions);
        let output = frame(&ctx, &state, book, sheet, &mut actions);
        let calc = *state.tree().calculations().keys().next().unwrap();
        let expected = row_title(&state, calc);
        assert!(painted(&output, &expected), "{expected}");
        session.shutdown().unwrap();
    }

    #[test]
    fn the_scope_is_chosen_with_two_radios_side_by_side() {
        let ctx = context();
        let (session, state, book, sheet) = bench();
        let mut actions = Vec::new();
        ctx.data_mut(|d| {
            d.insert_temp(
                pane_id(book),
                PaneState {
                    open: true,
                    ..PaneState::default()
                },
            )
        });
        frame(&ctx, &state, book, sheet, &mut actions);
        let output = frame(&ctx, &state, book, sheet, &mut actions);
        let sheet_label = text_exact(&texts(&output.shapes), &tr!("results-scope-sheet")).center;
        let book_label = text_exact(&texts(&output.shapes), &tr!("results-scope-book")).center;
        assert!((sheet_label.y - book_label.y).abs() < 1.0, "side by side");
        click(&ctx, book_label, egui::PointerButton::Primary, |ui| {
            results_pane(ui, &state, &mut actions, book, sheet)
        });
        let pane = ctx
            .data(|d| d.get_temp::<PaneState>(pane_id(book)))
            .unwrap();
        assert!(pane.whole_book);
        session.shutdown().unwrap();
    }
}
