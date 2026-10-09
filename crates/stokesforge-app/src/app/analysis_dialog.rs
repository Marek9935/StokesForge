//! La fenêtre d'analyse (#181, maquettes M3 à M5) et la confirmation de
//! suppression d'un calcul (M8).
//!
//! Une fenêtre **modale** (`modal_dialog_with`) à deux colonnes : à gauche les
//! réglages (`analysis_forms.rs`), à droite l'aperçu cliquable
//! (`analysis_preview.rs`), ses outils et les valeurs calculées. Le
//! formulaire vit dans `AppState` (`analysis_form_mut`) ; la fenêtre ne
//! pousse que deux actions, `ApplyAnalysis` et `CloseAnalysis`. « Create » (ou
//! « Update » en modification) se grise tant qu'un réglage manque ou que le
//! calcul de l'aperçu échoue — la raison est écrite dans l'aperçu.

use eframe::egui;
use stokesforge_analysis::ops::Warning;

use super::analysis_forms::{form_body, value_row};
use super::analysis_preview::{preview, reset_view, PreviewOutcome};
use super::dialogs::{modal_dialog_with, DialogFooter, DialogOutcome};
use crate::analysis::{kind_title, value_label, PreviewTool};
use crate::icons::{icon_button, Icon, IconRole};
use crate::state::{Action, AppState, CalculationDelete};
use crate::{theme, tr};

/// Largeur de la colonne des réglages.
const FORM_WIDTH: f32 = 330.0;
/// Hauteur réservée aux valeurs calculées sous l'aperçu.
const VALUES_HEIGHT: f32 = 130.0;

/// Un nombre lisible : sans zéros inutiles, en notation scientifique hors de
/// `[1e-3, 1e6)`.
pub(super) fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    let magnitude = value.abs();
    if magnitude != 0.0 && !(1e-3..1e6).contains(&magnitude) {
        return format!("{value:.4e}");
    }
    let text = format!("{value:.6}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Une valeur et son erreur type : `12.3 ± 0.1`.
pub(super) fn format_value(value: f64, error: Option<f64>) -> String {
    match error {
        Some(error) if error.is_finite() => {
            format!("{} ± {}", format_number(value), format_number(error))
        }
        _ => format_number(value),
    }
}

/// La phrase d'un avertissement de calcul.
pub(super) fn warning_text(warning: &Warning) -> String {
    match warning {
        Warning::SignalStepNotUniform => tr!("analysis-warning-step-not-uniform"),
        Warning::IrfStepMismatch => tr!("analysis-warning-irf-step"),
        Warning::NegativeClipped(count) => tr!(
            "analysis-warning-negative-clipped",
            count = i64::try_from(*count).unwrap_or(i64::MAX)
        ),
        Warning::Undetermined(count) => tr!(
            "analysis-warning-undetermined",
            count = i64::try_from(*count).unwrap_or(i64::MAX)
        ),
    }
}

/// Les valeurs de la série montrée, sous l'aperçu.
fn values_panel(ui: &mut egui::Ui, outcome: &PreviewOutcome) {
    egui::ScrollArea::vertical()
        .id_salt("stokesforge_analysis_values")
        .max_height(VALUES_HEIGHT)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for warning in &outcome.warnings {
                ui.label(
                    egui::RichText::new(warning_text(warning))
                        .color(theme::preview_result_color(ui.visuals().dark_mode)),
                );
            }
            if outcome.skipped > 0 {
                ui.label(
                    egui::RichText::new(tr!(
                        "analysis-skipped-rows",
                        count = i64::try_from(outcome.skipped).unwrap_or(i64::MAX)
                    ))
                    .weak(),
                );
            }
            for value in &outcome.values {
                value_row(
                    ui,
                    &value_label(&value.key, value.index),
                    &format_value(value.value, value.error),
                );
            }
        });
}

/// La fenêtre d'analyse, tant que le formulaire est ouvert.
pub(super) fn analysis_window(
    ctx: &egui::Context,
    state: &mut AppState,
    actions: &mut Vec<Action>,
) {
    let tree = state.tree().clone();
    let revision = state.snapshot().revision;
    let Some(form) = state.analysis_form_mut() else {
        return;
    };
    let id = egui::Id::new("stokesforge_analysis");
    let kind = kind_title(form.kind);
    let title = if form.editing.is_some() {
        tr!("analysis-window-edit", kind = kind.as_str())
    } else {
        tr!("analysis-window-new", kind = kind.as_str())
    };
    let accept_label = if form.editing.is_some() {
        tr!("analysis-apply-update")
    } else {
        tr!("analysis-apply-create")
    };
    let accept_hint = if form.editing.is_some() {
        tr!("analysis-apply-update", "hint")
    } else {
        tr!("analysis-apply-create", "hint")
    };
    let failed_id = id.with("failed");
    let failed = ctx
        .data(|data| data.get_temp::<bool>(failed_id))
        .unwrap_or(false);
    let accept_enabled = form.issues().is_empty() && !failed;

    let outcome = modal_dialog_with(
        ctx,
        id,
        &title,
        egui::vec2(1000.0, 660.0),
        DialogFooter {
            accept_label: &accept_label,
            accept_hint: &accept_hint,
            accept_enabled,
        },
        |_| {},
        |ui| {
            let full = ui.available_size();
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(FORM_WIDTH, full.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("stokesforge_analysis_form")
                            .auto_shrink([false, false])
                            .show(ui, |ui| form_body(ui, id, form, &tree));
                    },
                );
                ui.separator();
                ui.allocate_ui_with_layout(
                    egui::vec2((full.x - FORM_WIDTH - 24.0).max(200.0), full.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.horizontal(|ui| {
                            ui.radio_value(
                                &mut form.tool,
                                PreviewTool::Points,
                                tr!("analysis-tool-points"),
                            )
                            .on_hover_text(tr!("analysis-tool-points", "hint"));
                            ui.radio_value(
                                &mut form.tool,
                                PreviewTool::Zone,
                                tr!("analysis-tool-zone"),
                            )
                            .on_hover_text(tr!("analysis-tool-zone", "hint"));
                            ui.radio_value(
                                &mut form.tool,
                                PreviewTool::Zoom,
                                tr!("analysis-tool-zoom"),
                            )
                            .on_hover_text(tr!("analysis-tool-zoom", "hint"));
                            if icon_button(
                                ui,
                                Icon::ResetZoom,
                                IconRole::Neutral,
                                tr!("analysis-tool-reset"),
                            )
                            .clicked()
                            {
                                reset_view(ui, id);
                            }
                        });
                        let plot_height = (ui.available_height() - VALUES_HEIGHT - 8.0).max(160.0);
                        let result = ui
                            .allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), plot_height),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| preview(ui, id, form, &tree, revision),
                            )
                            .inner;
                        ui.data_mut(|data| data.insert_temp(failed_id, result.issue.is_some()));
                        values_panel(ui, &result);
                    },
                );
            });
        },
    );
    match outcome {
        DialogOutcome::Accept => actions.push(Action::ApplyAnalysis),
        DialogOutcome::Cancel => actions.push(Action::CloseAnalysis),
        DialogOutcome::Open => {}
    }
}

/// La confirmation de suppression d'un calcul (M8) : `Delete` garde les
/// colonnes comme colonnes ordinaires, `Delete with Columns` les supprime
/// aussi, `Cancel` ne fait rien.
pub(super) fn calculation_delete_window(
    ctx: &egui::Context,
    state: &AppState,
    actions: &mut Vec<Action>,
) {
    let Some(id) = state.pending_calculation_delete() else {
        return;
    };
    let name = state.calculation_title(id);
    let columns = state
        .tree()
        .calculation(id)
        .map_or(0, |calc| calc.live_outputs(state.tree()).count());
    let accept = tr!("analysis-delete-accept");
    let accept_hint = tr!("analysis-delete-accept", "hint");
    let mut with_columns = false;
    let outcome = modal_dialog_with(
        ctx,
        egui::Id::new("stokesforge_delete_calculation"),
        &tr!("analysis-delete-title"),
        egui::vec2(520.0, 190.0),
        DialogFooter {
            accept_label: &accept,
            accept_hint: &accept_hint,
            accept_enabled: true,
        },
        |ui| {
            if ui
                .button(tr!("analysis-delete-with-columns"))
                .on_hover_text(tr!("analysis-delete-with-columns", "hint"))
                .clicked()
            {
                with_columns = true;
            }
        },
        |ui| {
            ui.label(tr!("analysis-delete-confirm", name = name.as_str()));
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(tr!(
                    "analysis-delete-columns",
                    count = i64::try_from(columns).unwrap_or(i64::MAX)
                ))
                .weak(),
            );
        },
    );
    if with_columns {
        actions.push(Action::ConfirmDeleteCalculation(
            CalculationDelete::WithColumns,
        ));
        return;
    }
    match outcome {
        DialogOutcome::Accept => {
            actions.push(Action::ConfirmDeleteCalculation(
                CalculationDelete::Calculation,
            ));
        }
        DialogOutcome::Cancel => {
            actions.push(Action::ConfirmDeleteCalculation(CalculationDelete::Cancel));
        }
        DialogOutcome::Open => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{cliquer, contexte, image_de, painted, texte_exact, textes};
    use super::*;
    use crate::analysis::Preset;
    use stokesforge_analysis::model::Model;
    use stokesforge_core::{Column, ColumnMeta, ColumnRole, NodeId, Project};
    use stokesforge_session::Session;

    #[test]
    fn les_nombres_sont_lisibles() {
        assert_eq!(format_number(12.5), "12.5");
        assert_eq!(format_number(3.0), "3");
        assert_eq!(format_number(0.0), "0");
        assert_eq!(format_number(0.000_012_3), "1.2300e-5");
        assert_eq!(format_number(2.5e8), "2.5000e8");
        assert_eq!(format_number(f64::NAN), "NaN");
        assert_eq!(format_value(12.5, Some(0.25)), "12.5 ± 0.25");
        assert_eq!(format_value(12.5, None), "12.5");
        assert_eq!(format_value(12.5, Some(f64::NAN)), "12.5");
    }

    /// Une feuille X, Y, Y' : `y = 2x + 1` et une référence constante.
    fn banc() -> (Session, AppState, NodeId, Vec<NodeId>) {
        let mut project = Project::new("Analyse").unwrap();
        let book = project.add_workbook(project.root(), Some("Book")).unwrap();
        let sheet = project.tree().children(book).unwrap()[0];
        let xs: Vec<f64> = (0..21).map(f64::from).collect();
        let ys: Vec<f64> = xs.iter().map(|x| 2.0 * x + 1.0).collect();
        let reference = vec![2.0; 21];
        let mut ids = Vec::new();
        for (name, data) in [("A", &xs), ("B", &ys), ("C", &reference)] {
            ids.push(
                project
                    .add_column(sheet, None, Some(name), Column::from_f64(data))
                    .unwrap(),
            );
        }
        project
            .set_column_meta(
                ids[0],
                ColumnMeta {
                    role: ColumnRole::X,
                    ..ColumnMeta::default()
                },
            )
            .unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stokesforge".into()).unwrap();
        (session, state, sheet, ids)
    }

    fn frame(ctx: &egui::Context, state: &mut AppState) -> (egui::FullOutput, Vec<Action>) {
        let mut actions = Vec::new();
        let output = image_de(ctx, egui::vec2(1200.0, 800.0), Vec::new(), |ui| {
            analysis_window(ui.ctx(), state, &mut actions);
        });
        (output, actions)
    }

    #[test]
    fn chaque_famille_se_peint_sans_planter_et_titre_sa_fenetre() {
        let ctx = contexte();
        let presets = [
            Preset::BaselineLine,
            Preset::BaselinePolynomial,
            Preset::BaselineAnchors,
            Preset::Correct,
            Preset::Integrate,
            Preset::Fit(Model::Linear),
            Preset::Fit(Model::Polynomial { degree: 3 }),
            Preset::Fit(Model::Custom {
                formula: "a*x+b".to_string(),
            }),
            Preset::Peaks,
            Preset::Reconvolution,
            Preset::DirectDeconvolution,
        ];
        for preset in presets {
            let (session, mut state, sheet, ids) = banc();
            state.dispatch(Action::OpenAnalysis {
                preset: preset.clone(),
                sheet,
                columns: vec![ids[0], ids[1]],
            });
            // Une fenêtre egui ne se peint qu'à la deuxième image (mesure) ;
            // la troisième éprouve le calcul mémorisé de l'aperçu.
            frame(&ctx, &mut state);
            let (output, _) = frame(&ctx, &mut state);
            let title = tr!("analysis-window-new", kind = kind_title(preset.kind()));
            assert!(painted(&output, &title), "{preset:?} : titre {title:?}");
            frame(&ctx, &mut state);
            session.shutdown().unwrap();
        }
    }

    /// La fenêtre ouverte depuis le clic droit (#202) change de famille à chaud.
    #[test]
    fn la_liste_operation_change_de_famille_a_chaud() {
        use stokesforge_analysis::ops::OperationKind;
        let ctx = contexte();
        let (session, mut state, sheet, ids) = banc();
        state.dispatch(Action::OpenAnalysis {
            preset: Preset::BaselineLine,
            sheet,
            columns: vec![ids[0], ids[1]],
        });
        frame(&ctx, &mut state);
        let (output, _) = frame(&ctx, &mut state);
        let all = textes(&output.shapes);
        let combo = texte_exact(&all, &kind_title(OperationKind::Baseline));
        let mut actions = Vec::new();
        cliquer(&ctx, combo.centre, egui::PointerButton::Primary, |ui| {
            analysis_window(ui.ctx(), &mut state, &mut actions);
        });
        let (output, _) = frame(&ctx, &mut state);
        let all = textes(&output.shapes);
        let fit = texte_exact(&all, &kind_title(OperationKind::Fit));
        cliquer(&ctx, fit.centre, egui::PointerButton::Primary, |ui| {
            analysis_window(ui.ctx(), &mut state, &mut actions);
        });
        frame(&ctx, &mut state);
        assert_eq!(state.analysis_form().unwrap().kind, OperationKind::Fit);
        session.shutdown().unwrap();
    }

    #[test]
    fn creer_se_grise_tant_qu_un_reglage_manque_puis_valide() {
        let ctx = contexte();
        let (session, mut state, sheet, ids) = banc();
        state.dispatch(Action::OpenAnalysis {
            preset: Preset::Fit(Model::Linear),
            sheet,
            columns: vec![ids[0], ids[1]],
        });
        frame(&ctx, &mut state);
        let (output, _) = frame(&ctx, &mut state);
        let create = tr!("analysis-apply-create");
        let pos = texte_exact(&textes(&output.shapes), &create).centre;
        let mut actions = Vec::new();
        cliquer(&ctx, pos, egui::PointerButton::Primary, |ui| {
            analysis_window(ui.ctx(), &mut state, &mut actions);
        });
        assert!(actions.contains(&Action::ApplyAnalysis), "{actions:?}");
        session.shutdown().unwrap();

        // Une correction sans référence : « Create » est grisé, rien ne part.
        let (session, mut state, sheet, ids) = banc();
        state.dispatch(Action::OpenAnalysis {
            preset: Preset::Correct,
            sheet,
            columns: vec![ids[0], ids[1]],
        });
        frame(&ctx, &mut state);
        let (output, _) = frame(&ctx, &mut state);
        assert!(painted(&output, &tr!("analysis-issue-needs-reference")));
        let pos = texte_exact(&textes(&output.shapes), &create).centre;
        let mut actions = Vec::new();
        cliquer(&ctx, pos, egui::PointerButton::Primary, |ui| {
            analysis_window(ui.ctx(), &mut state, &mut actions);
        });
        assert!(!actions.contains(&Action::ApplyAnalysis), "{actions:?}");
        session.shutdown().unwrap();
    }

    #[test]
    fn un_clic_sur_l_apercu_pose_un_point_d_ancrage() {
        let ctx = contexte();
        let (session, mut state, sheet, ids) = banc();
        state.dispatch(Action::OpenAnalysis {
            preset: Preset::BaselineAnchors,
            sheet,
            columns: vec![ids[0], ids[1]],
        });
        frame(&ctx, &mut state);
        frame(&ctx, &mut state);
        let mut actions = Vec::new();
        cliquer(
            &ctx,
            egui::pos2(800.0, 300.0),
            egui::PointerButton::Primary,
            |ui| analysis_window(ui.ctx(), &mut state, &mut actions),
        );
        let points = state.analysis_form().unwrap().setup.baseline.points.len();
        assert_eq!(points, 1, "un point posé par le clic");
        // Le clic droit au même endroit le retire.
        cliquer(
            &ctx,
            egui::pos2(800.0, 300.0),
            egui::PointerButton::Secondary,
            |ui| analysis_window(ui.ctx(), &mut state, &mut actions),
        );
        assert!(state
            .analysis_form()
            .unwrap()
            .setup
            .baseline
            .points
            .is_empty());
        session.shutdown().unwrap();
    }

    #[test]
    fn la_confirmation_de_suppression_propose_trois_issues() {
        let ctx = contexte();
        let (session, mut state, sheet, ids) = banc();
        state.dispatch(Action::OpenAnalysis {
            preset: Preset::Integrate,
            sheet,
            columns: vec![ids[0], ids[1]],
        });
        state
            .analysis_form_mut()
            .unwrap()
            .setup
            .integrate
            .cumulative = true;
        state.dispatch(Action::ApplyAnalysis);
        let calc = *state.tree().calculations().keys().next().unwrap();
        state.dispatch(Action::RequestDeleteCalculation(calc));
        let mut actions = Vec::new();
        let mut output = egui::FullOutput::default();
        for _ in 0..2 {
            output = image_de(&ctx, egui::vec2(1200.0, 800.0), Vec::new(), |ui| {
                calculation_delete_window(ui.ctx(), &state, &mut actions);
            });
        }
        for label in [
            tr!("analysis-delete-accept"),
            tr!("analysis-delete-with-columns"),
            tr!("dialog-cancel"),
        ] {
            assert!(painted(&output, &label), "bouton {label:?}");
        }
        let pos = texte_exact(
            &textes(&output.shapes),
            &tr!("analysis-delete-with-columns"),
        )
        .centre;
        cliquer(&ctx, pos, egui::PointerButton::Primary, |ui| {
            calculation_delete_window(ui.ctx(), &state, &mut actions);
        });
        assert_eq!(
            actions,
            [Action::ConfirmDeleteCalculation(
                CalculationDelete::WithColumns
            )]
        );
        session.shutdown().unwrap();
    }
}
