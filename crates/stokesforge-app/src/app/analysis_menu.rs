//! The Analysis menu (#181, mockups M1 and M2): the menu bar and the
//! right-click of a column share **a single content**, `analysis_menu_contents`
//! (grammar in `docs/menus.md`); the top menu wraps it. The menu arranges the
//! methods in **categories** (#248), one window per category, then lists the
//! last three analyses run (#249). The right-click of a column has an
//! `Analysis ▸` submenu with one entry per category.
//!
//! Each entry pushes [`Action::OpenAnalysis`] or
//! [`Action::OpenGraphAnalysis`] with the family and method preselected; the
//! window (`analysis_dialog.rs`) does the rest. While no Y column is
//! targeted, the entries are **grayed out** with their `.disabled` tooltip,
//! never absent: the target is relevant, the gesture temporarily impossible.
//! "Recalculate Out of Date" is grayed out the same way when everything is
//! up to date.
//!
//! On a column that a calculation produces, the right-click adds the gestures
//! of the calculation: edit, recalculate, detach, delete
//! ([`calculation_section`]).

use eframe::egui;
use stokesforge_analysis::model::Model;
use stokesforge_core::NodeId;

use super::menu_sections::MenuSections;
use stokesforge_app_state::analysis::{Category, Preset, RecentTarget};
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

/// The menu target: columns of the workbook, or exact series of the selected
/// graph and description framed by its view (#210).
#[derive(Clone, Copy)]
pub(super) enum AnalysisTarget<'a> {
    Columns {
        sheet: NodeId,
        columns: &'a [NodeId],
    },
    Graph {
        graph: NodeId,
        series: &'a [stokesforge_plot::SeriesRef],
        spec: &'a stokesforge_plot::PlotSpec,
    },
    /// Whole rows `(sheet, row)` (#253): the analysis goes through the linked
    /// transposed sheet.
    Rows { rows: &'a [(NodeId, usize)] },
}

impl AnalysisTarget<'_> {
    fn enabled(self, state: &AppState) -> bool {
        match self {
            Self::Columns { columns, .. } => has_y(state, columns),
            Self::Graph { series, .. } => !series.is_empty(),
            Self::Rows { rows } => !rows.is_empty(),
        }
    }
}

/// At least one Y column among `columns`.
fn has_y(state: &AppState, columns: &[NodeId]) -> bool {
    columns
        .iter()
        .any(|c| state.column_meta(*c).role.paired_x().is_some())
}

/// An entry that opens the window on `preset`.
fn open_entry(
    ui: &mut egui::Ui,
    actions: &mut Vec<Action>,
    target: Option<AnalysisTarget>,
    enabled: bool,
    label: String,
    hint: String,
    preset: Preset,
) {
    let response = ui
        .add_enabled(enabled, egui::Button::new(label))
        .on_hover_text(hint)
        .on_disabled_hover_text(tr!("analysis-needs-y"));
    if response.clicked() {
        if let Some(target) = target {
            actions.push(match target {
                AnalysisTarget::Columns { sheet, columns } => Action::OpenAnalysis {
                    preset,
                    sheet,
                    columns: columns.to_vec(),
                },
                AnalysisTarget::Graph {
                    graph,
                    series,
                    spec,
                } => Action::OpenGraphAnalysis {
                    preset,
                    graph,
                    series: series.to_vec(),
                    spec: Box::new(spec.clone()),
                },
                AnalysisTarget::Rows { rows } => Action::OpenRowAnalysis {
                    preset,
                    rows: rows.to_vec(),
                },
            });
        }
        ui.close();
    }
}

/// The name of a fit model in the `Fit ▸` menu.
fn fit_label(model: &Model) -> (String, String) {
    match model {
        Model::Linear => (
            tr!("analysis-fit-linear"),
            tr!("analysis-fit-linear", "hint"),
        ),
        Model::Polynomial { .. } => (
            tr!("analysis-fit-polynomial"),
            tr!("analysis-fit-polynomial", "hint"),
        ),
        Model::Exponential => (
            tr!("analysis-fit-exponential"),
            tr!("analysis-fit-exponential", "hint"),
        ),
        Model::ExponentialOffset => (
            tr!("analysis-fit-exponential-offset"),
            tr!("analysis-fit-exponential-offset", "hint"),
        ),
        Model::Exponential2 => (
            tr!("analysis-fit-exponential2"),
            tr!("analysis-fit-exponential2", "hint"),
        ),
        Model::Logarithm => (
            tr!("analysis-fit-logarithm"),
            tr!("analysis-fit-logarithm", "hint"),
        ),
        Model::Power => (tr!("analysis-fit-power"), tr!("analysis-fit-power", "hint")),
        Model::Gaussian => (
            tr!("analysis-fit-gaussian"),
            tr!("analysis-fit-gaussian", "hint"),
        ),
        Model::Lorentzian => (
            tr!("analysis-fit-lorentzian"),
            tr!("analysis-fit-lorentzian", "hint"),
        ),
        Model::Custom { .. } => (
            tr!("analysis-fit-custom"),
            tr!("analysis-fit-custom", "hint"),
        ),
    }
}

/// A category submenu: grayed out while no Y column is targeted, with the
/// reason in a tooltip.
fn category_menu(
    ui: &mut egui::Ui,
    enabled: bool,
    category: Category,
    contents: impl FnOnce(&mut egui::Ui),
) {
    ui.add_enabled_ui(enabled, |ui| {
        ui.menu_button(category.title(), contents)
            .response
            .on_disabled_hover_text(tr!("analysis-needs-y"));
    });
}

/// What `target` becomes when a recent analysis is rerun.
fn recent_target(target: AnalysisTarget) -> RecentTarget {
    match target {
        AnalysisTarget::Columns { sheet, columns } => RecentTarget::Columns {
            sheet,
            columns: columns.to_vec(),
        },
        AnalysisTarget::Graph {
            graph,
            series,
            spec,
        } => RecentTarget::Graph {
            graph,
            series: series.to_vec(),
            spec: Box::new(spec.clone()),
        },
        AnalysisTarget::Rows { rows } => RecentTarget::Rows {
            rows: rows.to_vec(),
        },
    }
}

/// The last analyses run (#249), below the last separator of the menu: a
/// click opens the window with their settings, Ctrl+click runs them on the
/// current selection.
fn recent_entries(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    target: Option<AnalysisTarget>,
    enabled: bool,
) {
    for (index, recent) in state.analysis_recent().iter().enumerate() {
        let label = tr!(
            "analysis-recent-entry",
            rank = i64::try_from(index + 1).unwrap_or(i64::MAX),
            label = recent.label()
        );
        let response = ui
            .add_enabled(enabled, egui::Button::new(label))
            .on_hover_text(recent.hint())
            .on_disabled_hover_text(tr!("analysis-needs-y"));
        if response.clicked() {
            if let Some(target) = target {
                let run = ui.input(|input| input.modifiers.command);
                actions.push(Action::OpenRecentAnalysis {
                    index,
                    run,
                    target: recent_target(target),
                });
            }
            ui.close();
        }
    }
}

/// The content of the Analysis menu (#248): one entry per category (Data
/// Manipulation, Fitting, Peaks and Deconvolution, Integrate), the
/// recalculation of out-of-date results, then the last analyses run (#249).
pub(super) fn analysis_menu_contents(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    target: Option<AnalysisTarget>,
) {
    let enabled = target.is_some_and(|t| t.enabled(state));
    let stale = state.stale_calculations();
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        category_menu(ui, enabled, Category::DataManipulation, |ui| {
            for (label, hint, preset) in [
                (
                    tr!("analysis-baseline-constant"),
                    tr!("analysis-baseline-constant", "hint"),
                    Preset::BaselineConstant,
                ),
                (
                    tr!("analysis-baseline-linear"),
                    tr!("analysis-baseline-linear", "hint"),
                    Preset::BaselineLine,
                ),
                (
                    tr!("analysis-baseline-polynomial"),
                    tr!("analysis-baseline-polynomial", "hint"),
                    Preset::BaselinePolynomial,
                ),
                (
                    tr!("analysis-baseline-anchors"),
                    tr!("analysis-baseline-anchors", "hint"),
                    Preset::BaselineAnchors,
                ),
                (
                    tr!("action-analysis-correct"),
                    tr!("action-analysis-correct", "hint"),
                    Preset::Correct,
                ),
                (
                    tr!("analysis-smooth-adjacent"),
                    tr!("analysis-smooth-adjacent", "hint"),
                    Preset::SmoothAdjacent,
                ),
                (
                    tr!("analysis-smooth-savitzky-golay"),
                    tr!("analysis-smooth-savitzky-golay", "hint"),
                    Preset::SmoothSavitzkyGolay,
                ),
                (
                    tr!("analysis-smooth-median"),
                    tr!("analysis-smooth-median", "hint"),
                    Preset::SmoothMedian,
                ),
            ] {
                open_entry(ui, actions, target, enabled, label, hint, preset);
            }
        });
        category_menu(ui, enabled, Category::Fitting, |ui| {
            for model in Preset::fit_models() {
                let (label, hint) = fit_label(&model);
                open_entry(
                    ui,
                    actions,
                    target,
                    enabled,
                    label,
                    hint,
                    Preset::Fit(model),
                );
            }
        });
        category_menu(ui, enabled, Category::PeaksAndDeconvolution, |ui| {
            for (label, hint, preset) in [
                (
                    tr!("action-analysis-peaks"),
                    tr!("action-analysis-peaks", "hint"),
                    Preset::Peaks,
                ),
                (
                    tr!("action-analysis-reconvolution"),
                    tr!("action-analysis-reconvolution", "hint"),
                    Preset::Reconvolution,
                ),
                (
                    tr!("action-analysis-direct"),
                    tr!("action-analysis-direct", "hint"),
                    Preset::DirectDeconvolution,
                ),
            ] {
                open_entry(ui, actions, target, enabled, label, hint, preset);
            }
        });
        open_entry(
            ui,
            actions,
            target,
            enabled,
            tr!("action-analysis-integrate"),
            tr!("action-analysis-integrate", "hint"),
            Preset::Integrate,
        );
    });
    sections.section(ui, true, |ui| {
        if ui
            .add_enabled(
                !stale.is_empty(),
                egui::Button::new(tr!("action-analysis-recalculate-stale")),
            )
            .on_hover_text(tr!("action-analysis-recalculate-stale", "hint"))
            .on_disabled_hover_text(tr!("action-analysis-recalculate-stale", "disabled"))
            .clicked()
        {
            actions.push(Action::RecalculateCalculations(Vec::new()));
            ui.close();
        }
    });
    sections.section(ui, !state.analysis_recent().is_empty(), |ui| {
        recent_entries(ui, state, actions, target, enabled);
    });
}

fn last_preset_id(category: Category) -> egui::Id {
    egui::Id::new(("stokesforge_analysis_last_preset", category))
}

/// Publishes, on every frame, the last method run of each category (taken
/// from the recent analyses, #249): the only channel shared by the menus,
/// the right-click of curves and that of the graph, which do not have the
/// state at hand.
pub(super) fn publish_last_presets(ctx: &egui::Context, state: &AppState) {
    ctx.data_mut(|data| {
        for category in Category::ALL {
            let preset = state
                .analysis_recent()
                .iter()
                .find(|recent| Category::of(recent.operation.kind()) == category)
                .map_or_else(|| category.default_preset(), |recent| recent.preset());
            data.insert_temp(last_preset_id(category), preset);
        }
    });
}

/// The method preselected when opening the window of a category without
/// choosing one: the last one run in that category, else its default.
pub(super) fn category_preset(ctx: &egui::Context, category: Category) -> Preset {
    ctx.data(|data| data.get_temp::<Preset>(last_preset_id(category)))
        .unwrap_or_else(|| category.default_preset())
}

/// The `Analysis ▸` submenu of the right-click (column, curve): one entry per
/// category, which opens the window of the category (#248). The methods of a
/// category are chosen in its window: a submenu of methods would have made
/// three levels, beyond depth 2 of the grammar. Grayed out with `disabled` as
/// tooltip when nothing can be analyzed.
pub(super) fn analysis_open_menu(
    ui: &mut egui::Ui,
    enabled: bool,
    disabled: String,
    mut open: impl FnMut(Category),
) {
    ui.add_enabled_ui(enabled, |ui| {
        ui.menu_button(tr!("menu-analysis"), |ui| {
            for (category, label, hint) in [
                (
                    Category::DataManipulation,
                    tr!("action-analysis-open-data-manipulation"),
                    tr!("action-analysis-open-data-manipulation", "hint"),
                ),
                (
                    Category::Fitting,
                    tr!("action-analysis-open-fitting"),
                    tr!("action-analysis-open-fitting", "hint"),
                ),
                (
                    Category::PeaksAndDeconvolution,
                    tr!("action-analysis-open-peaks"),
                    tr!("action-analysis-open-peaks", "hint"),
                ),
                (
                    Category::Integrate,
                    tr!("action-analysis-open-integrate"),
                    tr!("action-analysis-open-integrate", "hint"),
                ),
            ] {
                if ui.button(label).on_hover_text(hint).clicked() {
                    open(category);
                    ui.close();
                }
            }
        })
        .response
        .on_disabled_hover_text(disabled);
    });
}

/// The `Analysis ▸` submenu of the right-click of a column (#202, #248).
pub(super) fn analysis_entry(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    target: AnalysisTarget,
) {
    let ctx = ui.ctx().clone();
    analysis_open_menu(
        ui,
        target.enabled(state),
        tr!("analysis-needs-y"),
        |category| {
            let preset = category_preset(&ctx, category);
            actions.push(match target {
                AnalysisTarget::Columns { sheet, columns } => Action::OpenAnalysis {
                    preset: preset.clone(),
                    sheet,
                    columns: columns.to_vec(),
                },
                AnalysisTarget::Graph {
                    graph,
                    series,
                    spec,
                } => Action::OpenGraphAnalysis {
                    preset: preset.clone(),
                    graph,
                    series: series.to_vec(),
                    spec: Box::new(spec.clone()),
                },
                AnalysisTarget::Rows { rows } => Action::OpenRowAnalysis {
                    preset: preset.clone(),
                    rows: rows.to_vec(),
                },
            })
        },
    );
}

/// The gestures of a calculation, on a column it produces: edit, recalculate,
/// detach, delete. Absent for an ordinary column.
pub(super) fn calculation_section(
    ui: &mut egui::Ui,
    sections: &mut MenuSections,
    state: &AppState,
    actions: &mut Vec<Action>,
    columns: &[NodeId],
) {
    let calculation = columns
        .iter()
        .find_map(|c| state.tree().calculation_of_column(*c))
        .map(|(id, _)| id);
    sections.section(ui, calculation.is_some(), |ui| {
        let Some(id) = calculation else {
            return;
        };
        let stale = state
            .tree()
            .calculation(id)
            .is_some_and(|calc| calc.stale.is_some());
        if ui
            .button(tr!("action-calculation-edit"))
            .on_hover_text(tr!("action-calculation-edit", "hint"))
            .clicked()
        {
            actions.push(Action::EditCalculation(id));
            ui.close();
        }
        if ui
            .add_enabled(
                stale,
                egui::Button::new(tr!("action-calculation-recalculate")),
            )
            .on_hover_text(tr!("action-calculation-recalculate", "hint"))
            .on_disabled_hover_text(tr!("action-calculation-recalculate", "disabled"))
            .clicked()
        {
            actions.push(Action::RecalculateCalculations(vec![id]));
            ui.close();
        }
        if ui
            .button(tr!("action-calculation-detach"))
            .on_hover_text(tr!("action-calculation-detach", "hint"))
            .clicked()
        {
            actions.push(Action::DetachCalculation(id));
            ui.close();
        }
        if ui
            .button(tr!("action-calculation-delete"))
            .on_hover_text(tr!("action-calculation-delete", "hint"))
            .clicked()
        {
            actions.push(Action::RequestDeleteCalculation(id));
            ui.close();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_analysis::ops::Operation;
    use stokesforge_app_state::analysis_recent::RecentAnalysis;

    /// Without history, the category opens its default; after an analysis
    /// run, it reopens that method, and the other categories keep their
    /// default.
    #[test]
    fn a_category_reopens_its_last_method() {
        let session = stokesforge_session::Session::spawn(
            stokesforge_core::Project::new("Recent").unwrap(),
            None,
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut preferences = stokesforge_app_state::preferences::Preferences::load_from(Some(
            dir.path().join("preferences.json"),
        ));
        let ctx = egui::Context::default();
        let state = |preferences: &stokesforge_app_state::preferences::Preferences| {
            AppState::new_with_preferences(
                session.handle(),
                "test.stf".to_string(),
                preferences.clone(),
            )
            .unwrap()
        };
        publish_last_presets(&ctx, &state(&preferences));
        assert_eq!(
            category_preset(&ctx, Category::DataManipulation),
            Preset::BaselineLine
        );
        let recent = RecentAnalysis {
            operation: Operation::Baseline {
                method: stokesforge_analysis::baseline::BaselineMethod::Zones {
                    zones: vec![(0.0, 1.0)],
                    degree: 3,
                },
                op: stokesforge_analysis::baseline::BaselineOp::Subtract,
                keep_baseline: false,
            },
            replace_original: false,
        };
        preferences.save_analysis_recent(&[recent]).unwrap();
        publish_last_presets(&ctx, &state(&preferences));
        assert_eq!(
            category_preset(&ctx, Category::DataManipulation),
            Preset::BaselinePolynomial
        );
        assert_eq!(
            category_preset(&ctx, Category::Integrate),
            Preset::Integrate
        );
        session.shutdown().unwrap();
    }
}
