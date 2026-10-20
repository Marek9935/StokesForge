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
use stokesforge_core::number_format::{format_exact, format_with_uncertainty};

use super::analysis_forms::{form_body, value_row};
use super::analysis_preview::{preview, PreviewOutcome};
use super::dialogs::{
    modal_confirm, modal_dialog_with, ConfirmOutcome, DialogFooter, DialogNature, DialogOutcome,
    InteractiveSize, PlainSize,
};
use super::form::{toggle_texts, ChoiceOption};
use crate::theme;
use stokesforge_app_state::analysis::{
    issue_message, kind_title, value_label, value_unit, Category, PreviewTool,
};
use stokesforge_app_state::r_squared::{level_of_value, RSquaredLevel};
use stokesforge_app_state::state::{Action, AppState, CalculationDelete};
use stokesforge_app_state::tr;

/// Largeur de la colonne des réglages.
const FORM_WIDTH: f32 = 330.0;
/// Hauteur réservée aux valeurs calculées sous l'aperçu.
const VALUES_HEIGHT: f32 = 130.0;

/// Une valeur et son erreur type, par le formateur commun (#222) :
/// `12.346 ± 0.052`.
pub(super) fn format_value(value: f64, error: Option<f64>) -> String {
    format_with_uncertainty(value, error)
}

/// La valeur exacte d'un résultat, pour la bulle : `12.3456789 ± 0.0523`.
pub(super) fn exact_text(value: f64, error: Option<f64>) -> String {
    match error {
        Some(error) if error.is_finite() => {
            format!("{} ± {}", format_exact(value), format_exact(error))
        }
        _ => format_exact(value),
    }
}

/// Rend un résultat arrondi (#222) : la valeur exacte s'affiche en bulle au
/// survol, et Ctrl+C sur le résultat survolé copie **la valeur exacte**, avec
/// un point, sans l'unité ni l'incertitude. L'`Event::Copy` est retiré, comme
/// le fait le Project Manager, pour ne pas déclencher aussi la copie du graphe.
pub(super) fn exact_result_label(
    ui: &mut egui::Ui,
    shown: String,
    value: f64,
    error: Option<f64>,
    level: RSquaredLevel,
) -> egui::Response {
    // Un R² faible (#288) se lit à deux signaux, pas à la seule couleur : la
    // teinte du palier, et le gras, avec une ligne de plus dans la bulle.
    let (text, hover) = if level.is_warning() {
        (
            theme::bold(shown).color(theme::r_squared_color(level, ui.visuals().dark_mode)),
            format!(
                "{}\n{}",
                exact_text(value, error),
                tr!("analysis-r2-low-hint")
            ),
        )
    } else {
        (egui::RichText::new(shown), exact_text(value, error))
    };
    let response = ui.label(text).on_hover_text(hover);
    if response.hovered() {
        let copy = ui.input_mut(|input| {
            let before = input.events.len();
            input
                .events
                .retain(|event| !matches!(event, egui::Event::Copy));
            input.events.len() != before
        });
        if copy {
            ui.ctx().copy_text(format_exact(value));
        }
    }
    response
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
        Warning::NotConverged => tr!("analysis-warning-not-converged"),
        Warning::Undetermined(count) => tr!(
            "analysis-warning-undetermined",
            count = i64::try_from(*count).unwrap_or(i64::MAX)
        ),
        Warning::IrfTruncated => tr!("analysis-warning-irf-truncated"),
        Warning::BridgedReferenceGap(count) => tr!(
            "analysis-warning-bridged-reference-gap",
            count = i64::try_from(*count).unwrap_or(i64::MAX)
        ),
        Warning::DivisionByZero(count) => tr!(
            "analysis-warning-division-by-zero",
            count = i64::try_from(*count).unwrap_or(i64::MAX)
        ),
    }
}

/// Les valeurs de la série montrée, sous l'aperçu.
fn values_panel(
    ui: &mut egui::Ui,
    outcome: &PreviewOutcome,
    tree: &stokesforge_core::Tree,
    series: Option<&stokesforge_core::SeriesRef>,
) {
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
            if outcome.pending {
                ui.label(egui::RichText::new(tr!("analysis-calculating")).weak());
                return;
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
                    value.value,
                    value.error,
                    level_of_value(&value.key, value.value),
                    &series.map_or_else(String::new, |s| value_unit(tree, s, &value.key)),
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
    let ys: Vec<stokesforge_core::NodeId> = state
        .analysis_form()
        .map(|form| form.series.iter().map(|s| s.y).collect())
        .unwrap_or_default();
    let graphs = state.graphs_plotting(&ys);
    let Some(form) = state.analysis_form_mut() else {
        return;
    };
    let id = egui::Id::new("stokesforge_analysis");
    let series_shown = form.series.get(form.current).cloned();
    let title = if form.editing.is_some() {
        let kind = kind_title(form.kind);
        tr!("analysis-window-edit", kind = kind.as_str())
    } else {
        // La fenêtre est celle d'une catégorie (#248) : son titre ne change pas
        // quand on passe d'une méthode à l'autre.
        let category = Category::of(form.kind).title();
        tr!("analysis-window-new", kind = category.as_str())
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
    let issues = form.issues();
    let accept_enabled = issues.is_empty() && !failed;
    // Grisé, le bouton dit pourquoi (S-G1, S-F10) : dans le pied et dans sa
    // bulle — le premier manque du formulaire, sinon l'aperçu en échec.
    let accept_disabled = match issues.first() {
        Some(issue) => issue_message(issue),
        None => tr!("disabled-analysis-issues"),
    };

    let outcome = modal_dialog_with(
        ctx,
        id,
        &title,
        // TAILLE-LIBRE(#300): deux colonnes (réglages de 330 et aperçu cliquable avec ses outils et ses valeurs) : l'aperçu a besoin de 600 px de large au moins, au-delà de la classe Large
        DialogNature::Draft(InteractiveSize::Free(egui::vec2(1000.0, 660.0))),
        DialogFooter::gated(
            &accept_label,
            &accept_hint,
            accept_enabled,
            &accept_disabled,
        ),
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
                            .show(ui, |ui| form_body(ui, id, form, &tree, &graphs));
                    },
                );
                ui.separator();
                ui.allocate_ui_with_layout(
                    egui::vec2((full.x - FORM_WIDTH - 24.0).max(200.0), full.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        // Les gestes de l'aperçu : l'analyse décide du sien ; seule la
                        // détection de pics en offre deux, à bascule (#270, #299).
                        super::form::form(ui, id.with("tools"), |rows| {
                            if form.tool_choices().len() > 1 {
                                rows.field(|ui| {
                                    // OUTIL-EN-FENETRE(#299): exception de D1/D2 pour les outils
                                    // d'analyse ; les pics demandent un clic (position) et un
                                    // glisser (plage), propres à l'aperçu, hors de la barre Tools.
                                    toggle_texts(
                                        ui,
                                        id.with("tool"),
                                        &mut form.tool,
                                        &[
                                            ChoiceOption::new(
                                                PreviewTool::Points,
                                                tr!("analysis-tool-points"),
                                            )
                                            .hint(tr!("analysis-tool-points", "hint")),
                                            ChoiceOption::new(
                                                PreviewTool::Zone,
                                                tr!("analysis-tool-zone"),
                                            )
                                            .hint(tr!("analysis-tool-zone", "hint")),
                                        ],
                                    );
                                });
                            }
                            // Le geste de l'analyse, puis la navigation, en clair (S-M1).
                            rows.note(match form.tool {
                                PreviewTool::Points => tr!("analysis-tool-points", "hint"),
                                PreviewTool::Zone => tr!("analysis-tool-zone", "hint"),
                            });
                            rows.note(tr!("analysis-preview-navigation"));
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
                        values_panel(ui, &result, &tree, series_shown.as_ref());
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
        DialogNature::Choice(PlainSize::Default),
        DialogFooter::always(&accept, &accept_hint),
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
            ui.add_space(crate::theme::SPACE_ITEM);
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

/// Une source partagée modifie toutes ses courbes : Cancel garde le formulaire.
pub(super) fn analysis_replace_window(
    ctx: &egui::Context,
    state: &AppState,
    actions: &mut Vec<Action>,
) {
    if !state.pending_analysis_replacement() {
        return;
    }
    let outcome = modal_confirm(
        ctx,
        egui::Id::new("stokesforge_replace_analysis"),
        &tr!("analysis-replace-title"),
        PlainSize::Default,
        &tr!("analysis-replace-accept"),
        &tr!("analysis-output-replace-hint"),
        |ui| {
            ui.label(tr!("analysis-replace-confirm"));
        },
    );
    match outcome {
        ConfirmOutcome::Confirm => actions.push(Action::ConfirmAnalysisReplacement(true)),
        ConfirmOutcome::Cancel => actions.push(Action::ConfirmAnalysisReplacement(false)),
        ConfirmOutcome::Open => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{cliquer, contexte, image_de, painted, texte_exact, textes};
    use super::*;
    use stokesforge_analysis::model::Model;
    use stokesforge_app_state::analysis::Preset;
    use stokesforge_core::{Column, ColumnMeta, ColumnRole, NodeId, Project};
    use stokesforge_session::Session;

    #[test]
    fn chaque_avertissement_a_sa_propre_phrase_dans_la_langue_courante() {
        use stokesforge_app_state::i18n::{build_loader, language};
        let loader = build_loader(language());
        let attendu = [
            (
                Warning::SignalStepNotUniform,
                tr!(in &loader; "analysis-warning-step-not-uniform"),
            ),
            (
                Warning::IrfStepMismatch,
                tr!(in &loader; "analysis-warning-irf-step"),
            ),
            (
                Warning::NegativeClipped(4),
                tr!(in &loader; "analysis-warning-negative-clipped", count = 4_i64),
            ),
            (
                Warning::NotConverged,
                tr!(in &loader; "analysis-warning-not-converged"),
            ),
            (
                Warning::Undetermined(3),
                tr!(in &loader; "analysis-warning-undetermined", count = 3_i64),
            ),
            (
                Warning::IrfTruncated,
                tr!(in &loader; "analysis-warning-irf-truncated"),
            ),
            (
                Warning::BridgedReferenceGap(3),
                tr!(in &loader; "analysis-warning-bridged-reference-gap", count = 3_i64),
            ),
            (
                Warning::DivisionByZero(2),
                tr!(in &loader; "analysis-warning-division-by-zero", count = 2_i64),
            ),
        ];
        for (warning, text) in &attendu {
            assert_eq!(&warning_text(warning), text, "{warning:?}");
            assert!(!text.is_empty());
        }
        // Le nombre est bien celui de l'avertissement.
        assert_ne!(
            warning_text(&Warning::Undetermined(3)),
            warning_text(&Warning::Undetermined(4))
        );
        assert_ne!(
            warning_text(&Warning::NegativeClipped(3)),
            warning_text(&Warning::NegativeClipped(4))
        );
        assert_ne!(
            warning_text(&Warning::BridgedReferenceGap(3)),
            warning_text(&Warning::BridgedReferenceGap(4))
        );
        assert_ne!(
            warning_text(&Warning::DivisionByZero(1)),
            warning_text(&Warning::DivisionByZero(2))
        );
    }

    #[test]
    fn les_resultats_ont_trois_chiffres_et_l_incertitude_deux() {
        assert_eq!(format_value(12.3456, None), "12.3");
        assert_eq!(format_value(12.3456, Some(0.0523)), "12.346 ± 0.052");
        assert_eq!(format_value(12.5, Some(f64::NAN)), "12.5");
        assert_eq!(format_value(0.000_012_3, None), "1.23e-5");
        assert_eq!(exact_text(12.3456, Some(0.0523)), "12.3456 ± 0.0523");
        assert_eq!(exact_text(0.1 + 0.2, None), "0.30000000000000004");
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
        let state = AppState::new(session.handle(), "test.stf".into()).unwrap();
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
            let title = tr!(
                "analysis-window-new",
                kind = Category::of(preset.kind()).title()
            );
            assert!(painted(&output, &title), "{preset:?} : titre {title:?}");
            frame(&ctx, &mut state);
            session.shutdown().unwrap();
        }
    }

    /// « Analysis type » ne liste que les familles de la catégorie de la
    /// fenêtre (#248) et en change à chaud ; une catégorie à une seule famille
    /// n'a pas de choix.
    #[test]
    fn analysis_type_ne_liste_que_les_familles_de_la_categorie() {
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
        assert!(painted(&output, &tr!("analysis-block-type")));
        for autre in [
            OperationKind::Fit,
            OperationKind::Integrate,
            OperationKind::Peaks,
        ] {
            assert!(
                !all.iter().any(|t| t.texte == kind_title(autre)),
                "{autre:?} n'est pas une méthode de Data Manipulation"
            );
        }
        let correct = texte_exact(&all, &kind_title(OperationKind::Correct));
        let mut actions = Vec::new();
        cliquer(&ctx, correct.centre, egui::PointerButton::Primary, |ui| {
            analysis_window(ui.ctx(), &mut state, &mut actions);
        });
        frame(&ctx, &mut state);
        assert_eq!(state.analysis_form().unwrap().kind, OperationKind::Correct);
        session.shutdown().unwrap();

        // Témoin : Fitting n'a qu'une famille, donc pas de choix.
        let ctx = contexte();
        let (session, mut state, sheet, ids) = banc();
        state.dispatch(Action::OpenAnalysis {
            preset: Preset::Fit(Model::Linear),
            sheet,
            columns: vec![ids[0], ids[1]],
        });
        frame(&ctx, &mut state);
        let (output, _) = frame(&ctx, &mut state);
        assert!(!painted(&output, &tr!("analysis-block-type")));
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
        // Contexte neuf : l'aperçu garde sa mémoire (résultat, pause de frappe)
        // d'une fenêtre à l'autre.
        let ctx = contexte();
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
