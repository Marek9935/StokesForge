//! Le menu Analysis (#181, maquettes M1 et M2) : la barre de menus et le clic
//! droit d'une colonne partagent **un seul contenu**, `analysis_menu_contents`
//! (grammaire de `docs/menus.md`) ; le menu du haut l'enveloppe. Le clic droit
//! d'une colonne n'a qu'une entrée `Analysis…` qui ouvre la fenêtre (#202).
//!
//! Chaque entrée pousse [`Action::OpenAnalysis`] avec la famille et la méthode
//! préchoisies ; la fenêtre (`analysis_dialog.rs`) fait le reste. Tant qu'aucune
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
use crate::analysis::Preset;
use crate::state::{Action, AppState};
use crate::tr;

/// La cible du menu : la feuille et les colonnes sélectionnées.
#[derive(Clone, Copy)]
pub(super) struct AnalysisTarget<'a> {
    pub(super) sheet: NodeId,
    pub(super) columns: &'a [NodeId],
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
            actions.push(Action::OpenAnalysis {
                preset,
                sheet: target.sheet,
                columns: target.columns.to_vec(),
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

/// Le contenu du menu Analysis : lignes de base, correction, intégrale,
/// ajustement, pics et déconvolution, puis le recalcul des périmés.
pub(super) fn analysis_menu_contents(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    target: Option<AnalysisTarget>,
) {
    let enabled = target.is_some_and(|t| has_y(state, t.columns));
    let stale = state.stale_calculations();
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        ui.add_enabled_ui(enabled, |ui| {
            ui.menu_button(tr!("analysis-menu-baseline"), |ui| {
                for (label, hint, preset) in [
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
                ] {
                    open_entry(ui, actions, target, enabled, label, hint, preset);
                }
            })
            .response
            .on_disabled_hover_text(tr!("analysis-needs-y"));
        });
        open_entry(
            ui,
            actions,
            target,
            enabled,
            tr!("action-analysis-correct"),
            tr!("action-analysis-correct", "hint"),
            Preset::Correct,
        );
        open_entry(
            ui,
            actions,
            target,
            enabled,
            tr!("action-analysis-integrate"),
            tr!("action-analysis-integrate", "hint"),
            Preset::Integrate,
        );
        ui.add_enabled_ui(enabled, |ui| {
            ui.menu_button(tr!("analysis-menu-fit"), |ui| {
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
            })
            .response
            .on_disabled_hover_text(tr!("analysis-needs-y"));
        });
        ui.add_enabled_ui(enabled, |ui| {
            ui.menu_button(tr!("analysis-menu-peaks"), |ui| {
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
            })
            .response
            .on_disabled_hover_text(tr!("analysis-needs-y"));
        });
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
}

/// L'entrée `Analysis…` du clic droit d'une colonne (#202) : ouvre la fenêtre
/// d'analyse, dont la liste « Analysis type » choisit la famille. Un sous-menu
/// aurait fait trois niveaux (Analysis ▸ Fit ▸ modèle), au-delà de la
/// profondeur 2 de la grammaire ; le menu du haut garde les entrées par méthode.
pub(super) fn analysis_entry(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    target: AnalysisTarget,
) {
    open_entry(
        ui,
        actions,
        Some(target),
        has_y(state, target.columns),
        tr!("action-analysis-open"),
        tr!("action-analysis-open", "hint"),
        Preset::BaselineLine,
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
