//! Les réglages de la fenêtre d'analyse (#181) : trois blocs — l'**entrée**
//! (séries, référence), les **réglages de la famille** et la **sortie**
//! (Show / Keep, recalcul automatique, colonnes écrites).
//!
//! Chaque contrôle écrit dans [`AnalysisForm`] ; rien ici ne décide ni ne
//! calcule (`analysis.rs`), et rien n'envoie d'action : la fenêtre valide le
//! tout d'un coup.

use eframe::egui;
use stokesforge_analysis::baseline::{BaselineOp, Interpolation};
use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::fit::ParamSetting;
use stokesforge_analysis::integrate::Under;
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::OperationKind;
use stokesforge_analysis::peaks::{PeakBackground, PeakShape};
use stokesforge_core::{NodeId, Tree};

use crate::analysis::{
    aux_candidates, aux_for, kind_title, series_label, AnalysisForm, BaselineShape,
    DirectAlgorithm, Preset,
};
use crate::icons::{icon_button, Icon, IconRole};
use crate::number_field::number_field;
use crate::quantities;
use crate::{theme, tr};

/// Un nombre réel modifiable, sans bornes mais **fini** (la brique refuse
/// « inf » et « nan » : ils deviendraient `null` dans le fichier et feraient
/// perdre le calcul au rechargement). Son identifiant est celui de sa place
/// dans la rangée.
fn number(ui: &mut egui::Ui, value: &mut f64) -> bool {
    let salt = ui.next_auto_id();
    ui.skip_ahead_auto_ids(1);
    number_field(ui, salt, value, &quantities::FREE).changed
}

/// Un entier borné par sa grandeur ; même identifiant que [`number`].
fn whole<T: crate::number_field::Number>(
    ui: &mut egui::Ui,
    value: &mut T,
    quantity: &quantities::Quantity,
) -> bool {
    let salt = ui.next_auto_id();
    ui.skip_ahead_auto_ids(1);
    number_field(ui, salt, value, quantity).changed
}

/// Un nombre facultatif : un champ texte vide vaut « automatique ». Le texte en
/// cours de frappe est gardé hors du modèle tant qu'il n'est pas un nombre.
fn optional_number(ui: &mut egui::Ui, id: egui::Id, value: &mut Option<f64>) -> bool {
    let mut text = ui
        .data_mut(|d| d.get_temp::<(Option<f64>, String)>(id))
        .filter(|(shown, _)| shown == value)
        .map(|(_, text)| text)
        .unwrap_or_else(|| value.map(|v| v.to_string()).unwrap_or_default());
    let response = ui.add(
        egui::TextEdit::singleline(&mut text)
            .desired_width(70.0)
            .hint_text(tr!("analysis-field-auto")),
    );
    let mut changed = false;
    if response.changed() {
        let parsed = if text.trim().is_empty() {
            Some(None)
        } else {
            text.trim()
                .replace(',', ".")
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .map(Some)
        };
        if let Some(next) = parsed {
            changed = next != *value;
            *value = next;
        }
    }
    ui.data_mut(|d| d.insert_temp(id, (*value, text)));
    changed
}

/// La liste des plages `(de, à)`, une rangée par plage.
fn range_rows(ui: &mut egui::Ui, id: egui::Id, ranges: &mut Vec<(f64, f64)>) {
    let mut remove = None;
    for (i, (a, b)) in ranges.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            number(ui, a);
            ui.label(tr!("analysis-field-to"));
            number(ui, b);
            if icon_button(
                ui,
                Icon::Delete,
                IconRole::Danger,
                tr!("analysis-field-remove"),
            )
            .clicked()
            {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        ranges.remove(i);
    }
    if ui
        .button(tr!("analysis-field-add-range"))
        .on_hover_text(tr!("analysis-field-add-range", "hint"))
        .clicked()
    {
        let last = ranges.last().map_or(0.0, |r| r.1);
        ranges.push((last, last + 1.0));
    }
    let _ = id;
}

/// La liste des points `(x, y)`, une rangée par point.
fn point_rows(ui: &mut egui::Ui, points: &mut Vec<(f64, f64)>) {
    let mut remove = None;
    for (i, (x, y)) in points.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(tr!("analysis-field-x"));
            number(ui, x);
            ui.label(tr!("analysis-field-y"));
            number(ui, y);
            if icon_button(
                ui,
                Icon::Delete,
                IconRole::Danger,
                tr!("analysis-field-remove"),
            )
            .clicked()
            {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        points.remove(i);
    }
}

/// Une plage facultative : deux champs et un bouton pour la retirer.
fn optional_range(ui: &mut egui::Ui, id: egui::Id, range: &mut Option<(f64, f64)>) {
    ui.horizontal(|ui| {
        let (mut a, mut b) = (range.map(|r| r.0), range.map(|r| r.1));
        let changed_a = optional_number(ui, id.with("a"), &mut a);
        ui.label(tr!("analysis-field-to"));
        let changed_b = optional_number(ui, id.with("b"), &mut b);
        if changed_a || changed_b {
            *range = match (a, b) {
                (Some(a), Some(b)) => Some((a.min(b), a.max(b))),
                _ => None,
            };
        }
        if range.is_some()
            && icon_button(
                ui,
                Icon::Delete,
                IconRole::Danger,
                tr!("analysis-field-clear-range"),
            )
            .clicked()
        {
            *range = None;
        }
    });
}

fn caption(ui: &mut egui::Ui, text: String) {
    ui.label(egui::RichText::new(text).weak());
}

/// Tout le formulaire : entrée, famille, sortie.
pub(super) fn form_body(
    ui: &mut egui::Ui,
    id: egui::Id,
    form: &mut AnalysisForm,
    tree: &Tree,
    graphs: &[(NodeId, String)],
) {
    input_block(ui, id, form, tree);
    ui.add_space(6.0);
    family_chooser(ui, id, form);
    family_block(ui, id, form, tree);
    ui.add_space(6.0);
    output_block(ui, id, form, graphs);
}

fn input_block(ui: &mut egui::Ui, id: egui::Id, form: &mut AnalysisForm, tree: &Tree) {
    theme::section_title(ui, tr!("analysis-block-input"));
    let count = form.series.len();
    ui.horizontal(|ui| {
        if count > 1 {
            if icon_button(
                ui,
                Icon::CaretLeft,
                IconRole::Neutral,
                tr!("analysis-series-previous"),
            )
            .clicked()
                && form.current > 0
            {
                form.current -= 1;
            }
            if icon_button(
                ui,
                Icon::CaretRight,
                IconRole::Neutral,
                tr!("analysis-series-next"),
            )
            .clicked()
                && form.current + 1 < count
            {
                form.current += 1;
            }
        }
        if let Some(series) = form.series.get(form.current) {
            ui.label(series_label(tree, series));
        }
    });
    if count > 1 {
        caption(
            ui,
            tr!(
                "analysis-series-position",
                current = i64::try_from(form.current + 1).unwrap_or(i64::MAX),
                total = i64::try_from(count).unwrap_or(i64::MAX)
            ),
        );
    }
    if form.needs_aux() {
        let candidates = aux_candidates(tree, &form.series);
        let selected = form
            .aux
            .as_ref()
            .and_then(|a| candidates.iter().find(|(y, _)| *y == a.y))
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| tr!("analysis-field-choose-column"));
        let heading = if form.kind == OperationKind::Correct {
            tr!("analysis-field-reference")
        } else {
            tr!("analysis-field-response")
        };
        ui.horizontal(|ui| {
            ui.label(heading);
            egui::ComboBox::from_id_salt(id.with("aux"))
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (y, label) in &candidates {
                        let current = form.aux.as_ref().is_some_and(|a| a.y == *y);
                        if ui.selectable_label(current, label).clicked() {
                            form.aux = Some(aux_for(tree, *y));
                        }
                    }
                });
        });
        if candidates.is_empty() {
            caption(ui, tr!("analysis-field-no-candidate"));
        }
    }
}

/// Le titre de la famille ; à la création, une liste déroulante qui en change à
/// chaud (#202 : le clic droit ouvre la fenêtre sans famille choisie). Un
/// calcul rouvert garde sa famille : seule sa méthode se change.
fn family_chooser(ui: &mut egui::Ui, id: egui::Id, form: &mut AnalysisForm) {
    if form.editing.is_some() {
        theme::section_title(ui, kind_title(form.kind));
        return;
    }
    theme::section_title(ui, tr!("analysis-block-type"));
    let before = form.kind;
    egui::ComboBox::from_id_salt(id.with("family"))
        .selected_text(kind_title(form.kind))
        .show_ui(ui, |ui| {
            for kind in OperationKind::ALL {
                ui.selectable_value(&mut form.kind, kind, kind_title(kind));
            }
        });
    if form.kind != before {
        let chosen = form.kind;
        form.set_kind(chosen);
        form.tool = form.natural_tool();
    }
    ui.add_space(6.0);
}

fn family_block(ui: &mut egui::Ui, id: egui::Id, form: &mut AnalysisForm, tree: &Tree) {
    match form.kind {
        OperationKind::Baseline => baseline_form(ui, id, form),
        OperationKind::Correct => correct_form(ui, form),
        OperationKind::Integrate => integrate_form(ui, id, form),
        OperationKind::Fit => fit_form(ui, id, form, tree),
        OperationKind::Peaks => peaks_form(ui, id, form),
        OperationKind::Reconvolution => reconv_form(ui, id, form),
        OperationKind::DirectDeconvolution => direct_form(ui, form),
    }
}

fn baseline_form(ui: &mut egui::Ui, id: egui::Id, form: &mut AnalysisForm) {
    let before = form.setup.baseline.shape;
    {
        let b = &mut form.setup.baseline;
        ui.radio_value(
            &mut b.shape,
            BaselineShape::TwoPoints,
            tr!("analysis-baseline-shape-line"),
        );
        ui.radio_value(
            &mut b.shape,
            BaselineShape::Zones,
            tr!("analysis-baseline-shape-zones"),
        );
        ui.radio_value(
            &mut b.shape,
            BaselineShape::Anchors,
            tr!("analysis-baseline-shape-anchors"),
        );
    }
    if form.setup.baseline.shape != before {
        form.tool = form.natural_tool();
    }
    let b = &mut form.setup.baseline;
    match b.shape {
        BaselineShape::TwoPoints => {
            caption(ui, tr!("analysis-baseline-hint-line"));
            point_rows(ui, &mut b.points);
        }
        BaselineShape::Anchors => {
            caption(ui, tr!("analysis-baseline-hint-anchors"));
            point_rows(ui, &mut b.points);
            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut b.interpolation,
                    Interpolation::Segments,
                    tr!("analysis-baseline-segments"),
                );
                ui.radio_value(
                    &mut b.interpolation,
                    Interpolation::Spline,
                    tr!("analysis-baseline-spline"),
                );
            });
        }
        BaselineShape::Zones => {
            caption(ui, tr!("analysis-baseline-hint-zones"));
            range_rows(ui, id.with("zones"), &mut b.zones);
            ui.horizontal(|ui| {
                ui.label(tr!("analysis-baseline-degree"));
                whole(ui, &mut b.degree, &quantities::POLYNOMIAL_DEGREE);
            });
        }
    }
    ui.horizontal(|ui| {
        ui.radio_value(
            &mut b.op,
            BaselineOp::Subtract,
            tr!("analysis-baseline-subtract"),
        );
        ui.radio_value(
            &mut b.op,
            BaselineOp::Divide,
            tr!("analysis-baseline-divide"),
        );
    });
}

fn correct_form(ui: &mut egui::Ui, form: &mut AnalysisForm) {
    caption(ui, tr!("analysis-correct-hint"));
    ui.horizontal(|ui| {
        ui.radio_value(
            &mut form.setup.correct,
            CorrectOp::Divide,
            tr!("analysis-correct-divide"),
        );
        ui.radio_value(
            &mut form.setup.correct,
            CorrectOp::Multiply,
            tr!("analysis-correct-multiply"),
        );
        ui.radio_value(
            &mut form.setup.correct,
            CorrectOp::Subtract,
            tr!("analysis-correct-subtract"),
        );
    });
}

fn integrate_form(ui: &mut egui::Ui, id: egui::Id, form: &mut AnalysisForm) {
    let i = &mut form.setup.integrate;
    caption(ui, tr!("analysis-integrate-hint"));
    range_rows(ui, id.with("ranges"), &mut i.ranges);
    ui.horizontal(|ui| {
        ui.radio_value(&mut i.under, Under::Zero, tr!("analysis-integrate-zero"));
        ui.radio_value(&mut i.under, Under::Line, tr!("analysis-integrate-line"));
    });
    ui.checkbox(&mut i.cumulative, tr!("analysis-integrate-cumulative"));
    ui.checkbox(&mut i.normalize, tr!("analysis-integrate-normalize"));
}

/// Le nom d'un modèle dans la liste déroulante.
fn model_name(model: &Model) -> String {
    match model {
        Model::Linear => tr!("analysis-model-linear"),
        Model::Polynomial { .. } => tr!("analysis-model-polynomial"),
        Model::Exponential => tr!("analysis-model-exponential"),
        Model::ExponentialOffset => tr!("analysis-model-exponential-offset"),
        Model::Exponential2 => tr!("analysis-model-exponential2"),
        Model::Logarithm => tr!("analysis-model-logarithm"),
        Model::Power => tr!("analysis-model-power"),
        Model::Gaussian => tr!("analysis-model-gaussian"),
        Model::Lorentzian => tr!("analysis-model-lorentzian"),
        Model::Custom { .. } => tr!("analysis-model-custom"),
    }
}

fn fit_form(ui: &mut egui::Ui, id: egui::Id, form: &mut AnalysisForm, tree: &Tree) {
    let has_sigma = form.series.iter().any(|s| s.sigma.is_some());
    let multi = form.series.len() > 1;
    let (shown_x, shown_y) = form
        .series
        .get(form.current)
        .map(|s| crate::analysis::series_points(tree, s))
        .unwrap_or_default();
    let f = &mut form.setup.fit;
    ui.horizontal(|ui| {
        ui.label(tr!("analysis-fit-model"));
        egui::ComboBox::from_id_salt(id.with("model"))
            .selected_text(model_name(&f.model))
            .show_ui(ui, |ui| {
                for model in Preset::fit_models() {
                    let same = std::mem::discriminant(&model) == std::mem::discriminant(&f.model);
                    if ui.selectable_label(same, model_name(&model)).clicked() && !same {
                        f.set_model(model);
                    }
                }
            });
    });
    if let Some(source) = crate::analysis::model_formula_source(&f.model) {
        let color = ui.visuals().text_color();
        stokesforge_render_egui::formula_label(ui, source, 15.0, color);
    }
    match &mut f.model {
        Model::Polynomial { degree } => {
            let before = *degree;
            ui.horizontal(|ui| {
                ui.label(tr!("analysis-fit-degree"));
                whole(ui, degree, &quantities::POLYNOMIAL_DEGREE);
            });
            if *degree != before {
                f.reset_params();
            }
        }
        Model::Custom { formula } => {
            caption(ui, tr!("analysis-fit-custom-hint"));
            let response = ui.add(
                egui::TextEdit::singleline(formula)
                    .desired_width(f32::INFINITY)
                    .hint_text(tr!("analysis-fit-custom-example")),
            );
            if response.changed() {
                f.reset_params();
            }
        }
        _ => {}
    }
    if f.params.len() != f.param_count() {
        f.reset_params();
    }
    if f.param_count() > 0
        && ui
            .button(tr!("analysis-fit-auto"))
            .on_hover_text(tr!("analysis-fit-auto", "hint"))
            .clicked()
    {
        f.auto_initial(&shown_x, &shown_y);
    }
    params_table(
        ui,
        id.with("fit-params"),
        &f.param_names(),
        &mut f.params,
        multi,
    );
    ui.horizontal(|ui| {
        ui.label(tr!("analysis-field-range"));
        optional_range(ui, id.with("fit-range"), &mut f.range);
    });
    if has_sigma {
        ui.checkbox(&mut f.weighted, tr!("analysis-fit-weighted"))
            .on_hover_text(tr!("analysis-fit-weighted", "hint"));
    }
    ui.checkbox(&mut f.residuals, tr!("analysis-fit-residuals"));
}

/// Le tableau des paramètres d'un ajustement : valeur de départ, fixé, bornes
/// et, pour un lot, partagé entre les séries (ajustement global).
fn params_table(
    ui: &mut egui::Ui,
    id: egui::Id,
    names: &[String],
    params: &mut [ParamSetting],
    shareable: bool,
) {
    if names.is_empty() {
        return;
    }
    egui::Grid::new(id)
        .num_columns(6)
        .striped(true)
        .show(ui, |ui| {
            ui.label("");
            ui.label(tr!("analysis-param-initial"));
            ui.label(tr!("analysis-param-fixed"));
            ui.label(tr!("analysis-param-min"));
            ui.label(tr!("analysis-param-max"));
            if shareable {
                ui.label(tr!("analysis-param-shared"));
            }
            ui.end_row();
            for (i, (name, p)) in names.iter().zip(params.iter_mut()).enumerate() {
                ui.label(name);
                optional_number(ui, id.with(("initial", i)), &mut p.initial);
                ui.checkbox(&mut p.fixed, "");
                optional_number(ui, id.with(("min", i)), &mut p.min);
                optional_number(ui, id.with(("max", i)), &mut p.max);
                if shareable {
                    ui.checkbox(&mut p.shared, "")
                        .on_hover_text(tr!("analysis-param-shared", "hint"));
                }
                ui.end_row();
            }
        });
}

fn peaks_form(ui: &mut egui::Ui, id: egui::Id, form: &mut AnalysisForm) {
    let p = &mut form.setup.peaks;
    ui.horizontal(|ui| {
        ui.label(tr!("analysis-peaks-shape"));
        egui::ComboBox::from_id_salt(id.with("shape"))
            .selected_text(match p.shape {
                PeakShape::Gaussian => tr!("analysis-model-gaussian"),
                PeakShape::Lorentzian => tr!("analysis-model-lorentzian"),
                PeakShape::Voigt => tr!("analysis-peaks-voigt"),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut p.shape,
                    PeakShape::Gaussian,
                    tr!("analysis-model-gaussian"),
                );
                ui.selectable_value(
                    &mut p.shape,
                    PeakShape::Lorentzian,
                    tr!("analysis-model-lorentzian"),
                );
                ui.selectable_value(&mut p.shape, PeakShape::Voigt, tr!("analysis-peaks-voigt"));
            });
    });
    ui.horizontal(|ui| {
        ui.label(tr!("analysis-peaks-background"));
        ui.radio_value(
            &mut p.background,
            PeakBackground::None,
            tr!("analysis-peaks-bg-none"),
        );
        ui.radio_value(
            &mut p.background,
            PeakBackground::Constant,
            tr!("analysis-peaks-bg-constant"),
        );
        ui.radio_value(
            &mut p.background,
            PeakBackground::Linear,
            tr!("analysis-peaks-bg-linear"),
        );
    });
    ui.checkbox(&mut p.link_widths, tr!("analysis-peaks-link-widths"));
    ui.horizontal(|ui| {
        let mut bounded = p.center_window.is_some();
        if ui
            .checkbox(&mut bounded, tr!("analysis-peaks-center-window"))
            .on_hover_text(tr!("analysis-peaks-center-window", "hint"))
            .changed()
        {
            p.center_window = bounded.then_some(1.0);
        }
        if let Some(window) = &mut p.center_window {
            number(ui, window);
            *window = window.abs();
        }
    });
    caption(ui, tr!("analysis-peaks-hint"));
    let mut remove = None;
    for (i, seed) in p.seeds.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(tr!(
                "analysis-peaks-seed",
                index = i64::try_from(i + 1).unwrap_or(i64::MAX)
            ));
            number(ui, &mut seed.center);
            if icon_button(
                ui,
                Icon::Delete,
                IconRole::Danger,
                tr!("analysis-field-remove"),
            )
            .clicked()
            {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        p.seeds.remove(i);
    }
    if p.seeds.is_empty() {
        ui.horizontal(|ui| {
            ui.label(tr!("analysis-peaks-prominence"));
            let salt = ui.next_auto_id();
            ui.skip_ahead_auto_ids(1);
            number_field(
                ui,
                salt,
                &mut p.prominence_percent,
                &quantities::PROMINENCE_PERCENT,
            )
            .response
            .on_hover_text(tr!("analysis-peaks-prominence", "hint"));
        });
        ui.horizontal(|ui| {
            ui.label(tr!("analysis-peaks-max"));
            whole(ui, &mut p.max_peaks, &quantities::MAX_PEAKS);
        });
    }
    ui.horizontal(|ui| {
        ui.label(tr!("analysis-field-range"));
        optional_range(ui, id.with("peaks-range"), &mut p.range);
    });
    ui.checkbox(&mut p.residuals, tr!("analysis-fit-residuals"));
}

fn reconv_form(ui: &mut egui::Ui, id: egui::Id, form: &mut AnalysisForm) {
    let r = &mut form.setup.reconv;
    caption(ui, tr!("analysis-reconv-hint"));
    ui.horizontal(|ui| {
        ui.label(tr!("analysis-reconv-components"));
        let before = r.spec.components;
        whole(ui, &mut r.spec.components, &quantities::COMPONENTS);
        if r.spec.components != before {
            r.resize_params();
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr!("analysis-reconv-shift"));
        number(ui, &mut r.spec.shift_initial);
        ui.checkbox(&mut r.spec.fit_shift, tr!("analysis-reconv-fit-shift"));
    });
    r.resize_params();
    let names = r.param_names();
    params_table(
        ui,
        id.with("reconv-params"),
        &names,
        &mut r.spec.params,
        false,
    );
    ui.horizontal(|ui| {
        ui.label(tr!("analysis-field-range"));
        optional_range(ui, id.with("reconv-window"), &mut r.spec.window);
    });
    ui.checkbox(&mut r.residuals, tr!("analysis-fit-residuals"));
}

fn direct_form(ui: &mut egui::Ui, form: &mut AnalysisForm) {
    let d = &mut form.setup.direct;
    caption(ui, tr!("analysis-direct-hint"));
    ui.horizontal(|ui| {
        ui.radio_value(
            &mut d.algorithm,
            DirectAlgorithm::RichardsonLucy,
            tr!("analysis-direct-richardson-lucy"),
        );
        ui.radio_value(
            &mut d.algorithm,
            DirectAlgorithm::Wiener,
            tr!("analysis-direct-wiener"),
        );
    });
    match d.algorithm {
        DirectAlgorithm::RichardsonLucy => {
            ui.horizontal(|ui| {
                ui.label(tr!("analysis-direct-iterations"));
                whole(ui, &mut d.iterations, &quantities::ITERATIONS);
            });
        }
        DirectAlgorithm::Wiener => {
            ui.horizontal(|ui| {
                ui.label(tr!("analysis-direct-noise"));
                let id = ui.id().with("noise");
                optional_number(ui, id, &mut d.noise);
            });
        }
    }
    ui.horizontal(|ui| {
        ui.label(tr!("analysis-reconv-shift"));
        number(ui, &mut d.shift);
    });
}

fn output_block(
    ui: &mut egui::Ui,
    id: egui::Id,
    form: &mut AnalysisForm,
    graphs: &[(NodeId, String)],
) {
    theme::section_title(ui, tr!("analysis-block-output"));
    // Les colonnes que cette famille peut écrire en plus de la principale.
    match form.kind {
        OperationKind::Baseline => {
            ui.checkbox(
                &mut form.setup.baseline.baseline_column,
                tr!("analysis-output-baseline-column"),
            );
        }
        OperationKind::Peaks => {
            ui.checkbox(
                &mut form.setup.peaks.peak_columns,
                tr!("analysis-output-peak-columns"),
            );
            ui.checkbox(
                &mut form.setup.peaks.sum_column,
                tr!("analysis-output-sum-column"),
            );
        }
        _ => {}
    }
    if form.editing.is_none() && !graphs.is_empty() {
        graph_target(ui, id, form, graphs);
    }
    ui.radio_value(&mut form.keep_results, false, tr!("analysis-output-show"))
        .on_hover_text(tr!("analysis-output-show", "hint"));
    ui.radio_value(&mut form.keep_results, true, tr!("analysis-output-keep"))
        .on_hover_text(tr!("analysis-output-keep", "hint"));
    if ui
        .checkbox(&mut form.auto_recalc, tr!("analysis-output-auto"))
        .on_hover_text(tr!("analysis-output-auto", "hint"))
        .changed()
    {
        form.auto_recalc_chosen = true;
    }
    if !form.auto_recalc && form.series.len() > stokesforge_analysis::ops::HEAVY_BATCH {
        caption(ui, tr!("analysis-output-heavy"));
    }
}

/// « Add to graph » (#200) : la case, et le choix du graphique quand plusieurs
/// montrent la courbe. Le graphique retenu reste valide : un choix qui n'est plus
/// parmi les candidats retombe sur le premier.
fn graph_target(
    ui: &mut egui::Ui,
    id: egui::Id,
    form: &mut AnalysisForm,
    graphs: &[(NodeId, String)],
) {
    if !form
        .graph
        .is_some_and(|g| graphs.iter().any(|(id, _)| *id == g))
    {
        form.graph = graphs.first().map(|(id, _)| *id);
    }
    ui.checkbox(&mut form.add_to_graph, tr!("analysis-output-to-graph"))
        .on_hover_text(tr!("analysis-output-to-graph", "hint"));
    if form.add_to_graph && graphs.len() > 1 {
        let shown = graphs
            .iter()
            .find(|(id, _)| Some(*id) == form.graph)
            .map(|(_, name)| name.clone())
            .unwrap_or_default();
        egui::ComboBox::from_id_salt(id.with("target-graph"))
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for (graph, name) in graphs {
                    ui.selectable_value(&mut form.graph, Some(*graph), name);
                }
            });
    }
}

/// Une valeur calculée sous l'aperçu : son libellé estompé, puis le nombre.
pub(super) fn value_row(ui: &mut egui::Ui, label: &str, text: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).weak());
        ui.label(text);
    });
}
