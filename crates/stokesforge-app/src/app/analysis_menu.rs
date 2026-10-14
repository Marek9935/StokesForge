//! Le menu Analysis (#181, maquettes M1 et M2) : la barre de menus et le clic
//! droit d'une colonne partagent **un seul contenu**, `analysis_menu_contents`
//! (grammaire de `docs/menus.md`) ; le menu du haut l'enveloppe. Le menu range
//! les méthodes en **catégories** (#248), une fenêtre par catégorie, puis liste
//! les trois dernières analyses lancées (#249). Le clic droit d'une colonne a
//! un sous-menu `Analysis ▸` à une entrée par catégorie.
//!
//! Chaque entrée pousse [`Action::OpenAnalysis`] ou
//! [`Action::OpenGraphAnalysis`] avec la famille et la méthode préchoisies ; la fenêtre (`analysis_dialog.rs`) fait le reste. Tant qu'aucune
//! colonne Y n'est visée, les entrées sont **grisées** avec leur info-bulle
//! `.disabled`, jamais absentes : la cible est pertinente, le geste
//! momentanément impossible. « Recalculate Out of Date » se grise de même quand
//! tout est à jour.
//!
//! Sur une colonne que produit un calcul, le clic droit ajoute les gestes du
//! calcul — modifier, recalculer, détacher, supprimer ([`calculation_section`]).

use eframe::egui;
use stokesforge_analysis::model::Model;
use stokesforge_core::{ColumnRole, NodeId};

use super::menu_sections::MenuSections;
use crate::analysis::{Category, Preset, RecentTarget};
use crate::state::{Action, AppState};
use crate::tr;

/// La cible du menu : colonnes du classeur, ou séries exactes du graphe
/// sélectionné et description cadrée par sa vue (#210).
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
}

impl AnalysisTarget<'_> {
    fn enabled(self, state: &AppState) -> bool {
        match self {
            Self::Columns { columns, .. } => has_y(state, columns),
            Self::Graph { series, .. } => !series.is_empty(),
        }
    }
}

/// Au moins une colonne Y parmi `columns`.
fn has_y(state: &AppState, columns: &[NodeId]) -> bool {
    columns
        .iter()
        .any(|c| matches!(state.column_meta(*c).role, ColumnRole::Y | ColumnRole::Y2))
}

/// Une entrée qui ouvre la fenêtre sur `preset`.
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
            });
        }
        ui.close();
    }
}

/// Le nom d'un modèle d'ajustement dans le menu `Fit ▸`.
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

/// Un sous-menu de catégorie : grisé tant qu'aucune colonne Y n'est visée,
/// avec la raison en info-bulle.
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

/// Ce que `target` devient quand on relance une analyse récente.
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
    }
}

/// Les dernières analyses lancées (#249), sous le dernier filet du menu : un
/// clic ouvre la fenêtre avec leurs réglages, Ctrl+clic les lance sur la
/// sélection courante.
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

/// Le contenu du menu Analysis (#248) : une entrée par catégorie — Data
/// Manipulation, Fitting, Peaks and Deconvolution, Integrate —, le recalcul
/// des périmés, puis les dernières analyses lancées (#249).
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

/// Le sous-menu `Analysis ▸` du clic droit (colonne, courbe) : une entrée par
/// catégorie, qui ouvre la fenêtre de la catégorie (#248). Les méthodes d'une
/// catégorie se choisissent dans sa fenêtre : un sous-menu de méthodes aurait
/// fait trois niveaux, au-delà de la profondeur 2 de la grammaire. Grisé avec
/// `disabled` en info-bulle quand rien n'est analysable.
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

/// Le sous-menu `Analysis ▸` du clic droit d'une colonne (#202, #248).
pub(super) fn analysis_entry(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    target: AnalysisTarget,
) {
    analysis_open_menu(
        ui,
        target.enabled(state),
        tr!("analysis-needs-y"),
        |category| {
            actions.push(match target {
                AnalysisTarget::Columns { sheet, columns } => Action::OpenAnalysis {
                    preset: category.default_preset(),
                    sheet,
                    columns: columns.to_vec(),
                },
                AnalysisTarget::Graph {
                    graph,
                    series,
                    spec,
                } => Action::OpenGraphAnalysis {
                    preset: category.default_preset(),
                    graph,
                    series: series.to_vec(),
                    spec: Box::new(spec.clone()),
                },
            })
        },
    );
}

/// Les gestes d'un calcul, sur une colonne qu'il produit : modifier,
/// recalculer, détacher, supprimer. Absente pour une colonne ordinaire.
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
