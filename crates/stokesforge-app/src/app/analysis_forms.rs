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

use super::analysis_dialog::{exact_result_label, format_value};
use super::form::{choice, choice_by, ChoiceOption, Form};
use super::hover_help::{help_beside, help_text};
use crate::icons::{icon_button, Icon, IconRole};
use crate::number_field::{number_field, NumberResponse};
use crate::theme;
use crate::theme::FieldWidth;
use stokesforge_app_state::analysis::{
    aux_candidates, aux_for, kind_title, reference_label, series_label, with_unit, AnalysisForm,
    BaselineShape, Category, DirectAlgorithm, Preset,
};
use stokesforge_app_state::quantities;
use stokesforge_app_state::tr;

/// Un nombre réel modifiable, sans bornes mais **fini** (la brique refuse
/// « inf » et « nan » : ils deviendraient `null` dans le fichier et feraient
/// perdre le calcul au rechargement). Son identifiant est celui de sa place
/// dans la rangée.
fn number(ui: &mut egui::Ui, value: &mut f64) -> bool {
    number_response(ui, value).changed
}

/// [`number`], avec la réponse du champ (pour y peindre un refus, N11).
fn number_response(ui: &mut egui::Ui, value: &mut f64) -> NumberResponse {
    let salt = ui.next_auto_id();
    ui.skip_ahead_auto_ids(1);
    number_field(ui, salt, value, &quantities::FREE)
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

/// Un nombre facultatif : un champ texte vide, `auto` ou `a` vaut « automatique ». Le texte en
/// cours de frappe est gardé hors du modèle tant qu'il n'est pas un nombre.
fn optional_number(ui: &mut egui::Ui, id: egui::Id, value: &mut Option<f64>) -> bool {
    optional_number_response(ui, id, value).0
}

/// Ce que dit la saisie d'un nombre facultatif : `Some(None)` pour l'Auto
/// (champ vidé, mot `auto` ou lettre `a`, comme `number_field::read`, #247),
/// `Some(Some(n))` pour un nombre, `None` pour une saisie illisible.
fn parse_optional(text: &str) -> Option<Option<f64>> {
    let trimmed = text.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("auto")
        || trimmed.eq_ignore_ascii_case("a")
    {
        return Some(None);
    }
    stokesforge_core::number_format::parse_input(trimmed).map(Some)
}

/// [`optional_number`], avec la réponse du champ (pour y peindre un refus, N11).
fn optional_number_response(
    ui: &mut egui::Ui,
    id: egui::Id,
    value: &mut Option<f64>,
) -> (bool, egui::Response) {
    let mut text = ui
        .data_mut(|d| d.get_temp::<(Option<f64>, String)>(id))
        .filter(|(shown, _)| shown == value)
        .map(|(_, text)| text)
        .unwrap_or_else(|| value.map(|v| v.to_string()).unwrap_or_default());
    let output = egui::TextEdit::singleline(&mut text)
        .desired_width(theme::FIELD_WIDTH_SHORT)
        .hint_text(tr!("analysis-field-auto"))
        .show(ui);
    let response = output.response.response;
    // Comme `number_field` : prendre le focus sélectionne tout le texte.
    if response.gained_focus() {
        let mut state = output.state;
        let end = egui::text::CCursor::new(text.chars().count());
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                end,
            )));
        state.store(ui.ctx(), response.id);
    }
    let mut changed = false;
    if response.changed() {
        if let Some(next) = parse_optional(&text) {
            changed = next != *value;
            *value = next;
        }
    }
    // Un texte qui n'est pas un nombre, ou qui dit « auto » (`a`, `auto`), est
    // vidé en quittant le champ : la borne redevient « auto » et le champ montre
    // son indication. Pendant la frappe on le tolère : « - » ou « 1e »
    // précèdent un nombre.
    if response.lost_focus() && !matches!(parse_optional(&text), Some(Some(_))) {
        text.clear();
        changed = value.is_some();
        *value = None;
    }
    ui.data_mut(|d| d.insert_temp(id, (*value, text)));
    (changed, response)
}

/// La liste des plages `(de, à)`, une rangée par plage.
///
/// Comme [`optional_range`], chaque borne vide (« auto ») prend l'étendue des
/// données `extent`, et un texte illisible est vidé. Les bornes saisies vivent
/// dans l'état du champ (`pending`), car le modèle ne porte que la plage
/// résolue. Une paire inversée est échangée sans bruit.
fn range_rows(
    ui: &mut egui::Ui,
    id: egui::Id,
    ranges: &mut Vec<(f64, f64)>,
    extent: Option<(f64, f64)>,
) {
    type Pending = (Option<f64>, Option<f64>, (f64, f64));
    let mut remove = None;
    for (i, row) in ranges.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let state_id = id.with(("pending", i));
            // `committed` : la plage que ce champ a lui-même écrite ; si le
            // modèle en diffère (tracé à la souris), il fait foi.
            let (mut a, mut b) = match ui.data(|d| d.get_temp::<Pending>(state_id)) {
                Some((a, b, committed)) if committed == *row => (a, b),
                _ => (Some(row.0), Some(row.1)),
            };
            let (changed_a, resp_a) = optional_number_response(ui, id.with(("a", i)), &mut a);
            ui.label(tr!("analysis-field-to"));
            let (changed_b, resp_b) = optional_number_response(ui, id.with(("b", i)), &mut b);
            // Pas d'échange pendant la frappe (il déplacerait le texte sous le
            // curseur) : le modèle reçoit la paire ordonnée, l'affichage la
            // suit quand on quitte les deux champs.
            if (resp_a.lost_focus() || resp_b.lost_focus())
                && !resp_a.has_focus()
                && !resp_b.has_focus()
            {
                if let (Some(x), Some(y)) = (a, b) {
                    if x > y {
                        (a, b) = (Some(y), Some(x));
                    }
                }
            }
            if changed_a || changed_b {
                let lo = a.or(extent.map(|e| e.0)).unwrap_or(row.0);
                let hi = b.or(extent.map(|e| e.1)).unwrap_or(row.1);
                *row = (lo.min(hi), lo.max(hi));
            }
            ui.data_mut(|d| d.insert_temp::<Pending>(state_id, (a, b, *row)));
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
        // Une rangée neuve couvre toute la série (bornes « auto »).
        ranges.push(extent.unwrap_or((0.0, 0.0)));
        let i = ranges.len() - 1;
        ui.data_mut(|d| {
            d.insert_temp::<(Option<f64>, Option<f64>, (f64, f64))>(
                id.with(("pending", i)),
                (None, None, ranges[i]),
            )
        });
    }
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
///
/// Chaque borne se saisit seule : une borne vide (« auto ») prend l'étendue des
/// données `extent` ; sans étendue connue, la plage reste absente tant que les
/// deux bornes ne sont pas données. Une paire inversée est échangée sans bruit.
/// Les bornes saisies vivent dans l'état du champ (`pending`), car le modèle
/// ne porte qu'une plage complète : sans cela, une borne tapée seule serait
/// effacée au rendu suivant.
fn optional_range(
    ui: &mut egui::Ui,
    id: egui::Id,
    range: &mut Option<(f64, f64)>,
    extent: Option<(f64, f64)>,
) {
    type Pending = (Option<f64>, Option<f64>, Option<(f64, f64)>);
    ui.horizontal(|ui| {
        let state_id = id.with("pending");
        // `committed` : la plage que ce champ a lui-même écrite. Si le modèle
        // en diffère (tracé à la souris, bouton retirer, autre formulaire), il
        // fait foi.
        let (mut a, mut b) = match ui.data(|d| d.get_temp::<Pending>(state_id)) {
            Some((a, b, committed)) if committed == *range => (a, b),
            _ => (range.map(|r| r.0), range.map(|r| r.1)),
        };
        let (changed_a, resp_a) = optional_number_response(ui, id.with("a"), &mut a);
        ui.label(tr!("analysis-field-to"));
        let (changed_b, resp_b) = optional_number_response(ui, id.with("b"), &mut b);
        // Pas d'échange pendant la frappe (il déplacerait le texte sous le
        // curseur) : le modèle reçoit la paire ordonnée, l'affichage la suit
        // quand on quitte les deux champs.
        if (resp_a.lost_focus() || resp_b.lost_focus())
            && !resp_a.has_focus()
            && !resp_b.has_focus()
        {
            if let (Some(x), Some(y)) = (a, b) {
                if x > y {
                    (a, b) = (Some(y), Some(x));
                }
            }
        }
        if changed_a || changed_b {
            let lo = a.or(extent.map(|e| e.0));
            let hi = b.or(extent.map(|e| e.1));
            *range = match (lo, hi) {
                (Some(lo), Some(hi)) if a.is_some() || b.is_some() => {
                    Some((lo.min(hi), lo.max(hi)))
                }
                _ => None,
            };
        }
        ui.data_mut(|d| d.insert_temp::<Pending>(state_id, (a, b, *range)));
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

/// Tout le formulaire : entrée, famille, sortie, en une grille à deux
/// colonnes (#227) dont les champs s'alignent d'un bloc à l'autre.
pub(super) fn form_body(
    ui: &mut egui::Ui,
    id: egui::Id,
    form: &mut AnalysisForm,
    tree: &Tree,
    graphs: &[(NodeId, String)],
) {
    super::form::form(ui, id.with("rows"), |rows| {
        input_block(rows, form, tree);
        family_chooser(rows, id, form);
        family_block(rows, id, form, tree);
        output_block(rows, id, form, graphs, tree);
    });
}

/// Une liste de rangées (plages, points, pics) dans la colonne des champs.
fn field_list(rows: &mut Form<'_>, add: impl FnOnce(&mut egui::Ui)) {
    rows.field(|ui| {
        ui.vertical(add);
    });
}

fn input_block(rows: &mut Form<'_>, form: &mut AnalysisForm, tree: &Tree) {
    rows.section(tr!("analysis-block-input"));
    let count = form.series.len();
    rows.ui().horizontal(|ui| {
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
            rows.ui(),
            tr!(
                "analysis-series-position",
                current = i64::try_from(form.current + 1).unwrap_or(i64::MAX),
                total = i64::try_from(count).unwrap_or(i64::MAX)
            ),
        );
    }
}

/// Le sélecteur de la colonne auxiliaire (Reference / Response), en tête du
/// bloc de la famille, sous « Analysis type » (#252).
fn aux_block(rows: &mut Form<'_>, id: egui::Id, form: &mut AnalysisForm, tree: &Tree) {
    if form.needs_aux() {
        let candidates = aux_candidates(tree, &form.series);
        let selected = form
            .aux
            .as_ref()
            .and_then(|a| {
                // Une référence figée (#248) n'est pas une colonne à choisir :
                // elle se lit « copy of … ».
                if tree.is_analysis_copy(a.y) {
                    Some(reference_label(tree, a))
                } else {
                    candidates
                        .iter()
                        .find(|(y, _)| *y == a.y)
                        .map(|(_, label)| label.clone())
                }
            })
            .unwrap_or_else(|| tr!("analysis-field-choose-column"));
        let heading = if form.kind == OperationKind::Correct {
            tr!("analysis-field-reference")
        } else {
            tr!("analysis-field-response")
        };
        // Une liste de colonnes du projet, pas un jeu d'options fixe : liste
        // déroulante quel que soit leur nombre.
        rows.row(heading, |ui| {
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
            rows.note(tr!("analysis-field-no-candidate"));
        }
    }
}

/// Le titre de la famille ; à la création, le choix « Analysis type » parmi les
/// seules familles de la catégorie de la fenêtre (#248), qui en change à chaud.
/// Une catégorie à une seule famille (Fitting, Integrate) n'a rien à choisir.
/// Un calcul rouvert garde sa famille : seule sa méthode se change.
fn family_chooser(rows: &mut Form<'_>, id: egui::Id, form: &mut AnalysisForm) {
    let kinds = Category::of(form.kind).kinds();
    if form.editing.is_some() || kinds.len() < 2 {
        rows.section(kind_title(form.kind));
        return;
    }
    rows.section(tr!("analysis-block-type"));
    let mut kind = form.kind;
    let options: Vec<_> = kinds
        .iter()
        .map(|kind| ChoiceOption::new(*kind, kind_title(*kind)))
        .collect();
    if choice(rows.ui(), id.with("family"), &mut kind, &options) {
        form.set_kind(kind);
        form.tool = form.natural_tool();
    }
}

fn family_block(rows: &mut Form<'_>, id: egui::Id, form: &mut AnalysisForm, tree: &Tree) {
    aux_block(rows, id, form, tree);
    let extent = x_extent(form, tree);
    match form.kind {
        OperationKind::Baseline => baseline_form(rows, id, form, extent),
        OperationKind::Correct => correct_form(rows, id, form),
        OperationKind::Integrate => integrate_form(rows, id, form, extent),
        OperationKind::Fit => fit_form(rows, id, form, tree),
        OperationKind::Peaks => peaks_form(rows, id, form, tree),
        OperationKind::Reconvolution => reconv_form(rows, id, form, tree),
        OperationKind::DirectDeconvolution => direct_form(rows, id, form),
    }
}

fn baseline_form(
    rows: &mut Form<'_>,
    id: egui::Id,
    form: &mut AnalysisForm,
    extent: Option<(f64, f64)>,
) {
    let shapes = [
        ChoiceOption::new(
            BaselineShape::Constant,
            tr!("analysis-baseline-shape-constant"),
        ),
        ChoiceOption::new(
            BaselineShape::ConstantZone,
            tr!("analysis-baseline-shape-constant-zone"),
        ),
        ChoiceOption::new(
            BaselineShape::TwoPoints,
            tr!("analysis-baseline-shape-line"),
        ),
        ChoiceOption::new(BaselineShape::Zones, tr!("analysis-baseline-shape-zones")),
        ChoiceOption::new(
            BaselineShape::Anchors,
            tr!("analysis-baseline-shape-anchors"),
        ),
    ];
    if rows.field(|ui| {
        choice(
            ui,
            id.with("baseline-shape"),
            &mut form.setup.baseline.shape,
            &shapes,
        )
    }) {
        form.tool = form.natural_tool();
    }
    let b = &mut form.setup.baseline;
    match b.shape {
        BaselineShape::Constant => {
            rows.note(tr!("analysis-baseline-hint-constant"));
            let mut points = b.constant_point.into_iter().collect();
            field_list(rows, |ui| point_rows(ui, &mut points));
            b.constant_point = points.first().copied();
        }
        BaselineShape::ConstantZone => {
            rows.note(tr!("analysis-baseline-hint-constant-zone"));
            let mut zones = b.constant_zone.into_iter().collect();
            field_list(rows, |ui| {
                range_rows(ui, id.with("constant-zone"), &mut zones, extent)
            });
            b.constant_zone = zones.last().copied();
        }
        BaselineShape::TwoPoints => {
            rows.note(tr!("analysis-baseline-hint-line"));
            field_list(rows, |ui| point_rows(ui, &mut b.points));
        }
        BaselineShape::Anchors => {
            rows.note(tr!("analysis-baseline-hint-anchors"));
            field_list(rows, |ui| point_rows(ui, &mut b.points));
            rows.field(|ui| {
                choice(
                    ui,
                    id.with("baseline-interpolation"),
                    &mut b.interpolation,
                    &[
                        ChoiceOption::new(
                            Interpolation::Segments,
                            tr!("analysis-baseline-segments"),
                        ),
                        ChoiceOption::new(Interpolation::Spline, tr!("analysis-baseline-spline")),
                    ],
                )
            });
        }
        BaselineShape::Zones => {
            rows.note(tr!("analysis-baseline-hint-zones"));
            field_list(rows, |ui| {
                range_rows(ui, id.with("zones"), &mut b.zones, extent)
            });
            rows.row(tr!("analysis-baseline-degree"), |ui| {
                whole(ui, &mut b.degree, &quantities::POLYNOMIAL_DEGREE)
            });
        }
    }
    rows.field(|ui| {
        choice(
            ui,
            id.with("baseline-op"),
            &mut b.op,
            &[
                ChoiceOption::new(BaselineOp::Subtract, tr!("analysis-baseline-subtract")),
                ChoiceOption::new(BaselineOp::Divide, tr!("analysis-baseline-divide")),
            ],
        )
    });
}

fn correct_form(rows: &mut Form<'_>, id: egui::Id, form: &mut AnalysisForm) {
    rows.note(tr!("analysis-correct-hint"));
    rows.field(|ui| {
        choice(
            ui,
            id.with("correct-op"),
            &mut form.setup.correct,
            &[
                ChoiceOption::new(CorrectOp::Divide, tr!("analysis-correct-divide")),
                ChoiceOption::new(CorrectOp::Multiply, tr!("analysis-correct-multiply")),
                ChoiceOption::new(CorrectOp::Subtract, tr!("analysis-correct-subtract")),
            ],
        )
    });
}

fn integrate_form(
    rows: &mut Form<'_>,
    id: egui::Id,
    form: &mut AnalysisForm,
    extent: Option<(f64, f64)>,
) {
    let i = &mut form.setup.integrate;
    rows.note(tr!("analysis-integrate-hint"));
    field_list(rows, |ui| {
        range_rows(ui, id.with("ranges"), &mut i.ranges, extent)
    });
    rows.field(|ui| {
        choice(
            ui,
            id.with("integrate-under"),
            &mut i.under,
            &[
                ChoiceOption::new(Under::Zero, tr!("analysis-integrate-zero")),
                ChoiceOption::new(Under::Line, tr!("analysis-integrate-line")),
            ],
        )
    });
    rows.checkbox(
        &mut i.cumulative,
        tr!("analysis-integrate-cumulative"),
        None,
    );
    rows.checkbox(&mut i.normalize, tr!("analysis-integrate-normalize"), None);
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

/// L'étendue en X de l'entrée courante : ce que vaut une borne laissée « auto ».
fn x_extent_of(xs: &[f64]) -> Option<(f64, f64)> {
    let lo = xs.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (lo <= hi).then_some((lo, hi))
}

fn x_extent(form: &AnalysisForm, tree: &Tree) -> Option<(f64, f64)> {
    let series = form.series.get(form.current)?;
    x_extent_of(&stokesforge_app_state::analysis::series_points(tree, series).0)
}

fn fit_form(rows: &mut Form<'_>, id: egui::Id, form: &mut AnalysisForm, tree: &Tree) {
    let has_sigma = form.series.iter().any(|s| s.sigma.is_some());
    let multi = form.series.len() > 1;
    let (shown_x, shown_y) = form
        .series
        .get(form.current)
        .map(|s| stokesforge_app_state::analysis::series_points(tree, s))
        .unwrap_or_default();
    let extent = x_extent_of(&shown_x);
    let f = &mut form.setup.fit;
    let models: Vec<ChoiceOption<Model>> = Preset::fit_models()
        .into_iter()
        .map(|model| {
            let name = model_name(&model);
            ChoiceOption::new(model, name)
        })
        .collect();
    let mut model = f.model.clone();
    // Un modèle choisi vaut par sa variante : le degré d'un polynôme ou la
    // formule libre en cours restent ceux du formulaire.
    if rows.row(tr!("analysis-fit-model"), |ui| {
        choice_by(ui, id.with("model"), &mut model, &models, |a, b| {
            std::mem::discriminant(a) == std::mem::discriminant(b)
        })
    }) {
        f.set_model(model);
    }
    if let Some(source) = stokesforge_app_state::analysis::model_formula_source(&f.model) {
        rows.field(|ui| {
            let color = ui.visuals().text_color();
            stokesforge_render_egui::formula_label(ui, source, 15.0, color);
        });
    }
    match &mut f.model {
        Model::Polynomial { degree } => {
            let before = *degree;
            rows.row(tr!("analysis-fit-degree"), |ui| {
                whole(ui, degree, &quantities::POLYNOMIAL_DEGREE)
            });
            if *degree != before {
                f.reset_params();
            }
        }
        Model::Custom { formula } => {
            rows.note(tr!("analysis-fit-custom-hint"));
            let changed = rows.field(|ui| {
                let width = FieldWidth::Full.points(ui);
                ui.add(
                    egui::TextEdit::singleline(formula)
                        .desired_width(width)
                        .hint_text(tr!("analysis-fit-custom-example")),
                )
                .changed()
            });
            if changed {
                f.reset_params();
            }
        }
        _ => {}
    }
    if f.params.len() != f.param_count() {
        f.reset_params();
    }
    if f.param_count() > 0
        && rows.field(|ui| {
            ui.button(tr!("analysis-fit-auto"))
                .on_hover_text(tr!("analysis-fit-auto", "hint"))
                .clicked()
        })
    {
        f.auto_initial(&shown_x, &shown_y);
    }
    params_table(
        rows.ui(),
        id.with("fit-params"),
        &f.param_names(),
        &mut f.params,
        multi,
    );
    rows.row(tr!("analysis-field-range"), |ui| {
        optional_range(ui, id.with("fit-range"), &mut f.range, extent);
    });
    if has_sigma {
        rows.checkbox(
            &mut f.weighted,
            tr!("analysis-fit-weighted"),
            Some(tr!("analysis-fit-weighted", "hint")),
        );
    }
    rows.checkbox(&mut f.residuals, tr!("analysis-fit-residuals"), None);
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
                    help_beside(ui, tr!("analysis-param-shared", "hint"), |ui| {
                        ui.checkbox(&mut p.shared, "")
                    });
                }
                ui.end_row();
            }
        });
}

fn peaks_form(rows: &mut Form<'_>, id: egui::Id, form: &mut AnalysisForm, tree: &Tree) {
    let extent = x_extent(form, tree);
    let p = &mut form.setup.peaks;
    // Trois formes : des radios (C1), plus une liste déroulante.
    rows.row(tr!("analysis-peaks-shape"), |ui| {
        choice(
            ui,
            id.with("shape"),
            &mut p.shape,
            &[
                ChoiceOption::new(PeakShape::Gaussian, tr!("analysis-model-gaussian")),
                ChoiceOption::new(PeakShape::Lorentzian, tr!("analysis-model-lorentzian")),
                ChoiceOption::new(PeakShape::Voigt, tr!("analysis-peaks-voigt")),
            ],
        )
    });
    rows.row(tr!("analysis-peaks-background"), |ui| {
        choice(
            ui,
            id.with("peaks-background"),
            &mut p.background,
            &[
                ChoiceOption::new(PeakBackground::None, tr!("analysis-peaks-bg-none")),
                ChoiceOption::new(PeakBackground::Constant, tr!("analysis-peaks-bg-constant")),
                ChoiceOption::new(PeakBackground::Linear, tr!("analysis-peaks-bg-linear")),
            ],
        )
    });
    rows.checkbox(&mut p.link_widths, tr!("analysis-peaks-link-widths"), None);
    rows.field(|ui| {
        let mut bounded = p.center_window.is_some();
        if help_beside(ui, tr!("analysis-peaks-center-window", "hint"), |ui| {
            ui.checkbox(&mut bounded, tr!("analysis-peaks-center-window"))
                .changed()
        }) {
            p.center_window = bounded.then_some(1.0);
        }
        if let Some(window) = &mut p.center_window {
            number(ui, window);
            *window = window.abs();
        }
    });
    rows.note(tr!("analysis-peaks-hint"));
    let mut remove = None;
    field_list(rows, |ui| {
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
    });
    if let Some(i) = remove {
        p.seeds.remove(i);
    }
    if p.seeds.is_empty() {
        rows.row(tr!("analysis-peaks-prominence"), |ui| {
            let salt = ui.next_auto_id();
            ui.skip_ahead_auto_ids(1);
            number_field(
                ui,
                salt,
                &mut p.prominence_percent,
                &quantities::PROMINENCE_PERCENT,
            );
            help_text(ui, tr!("analysis-peaks-prominence", "hint"));
        });
        rows.row(tr!("analysis-peaks-max"), |ui| {
            whole(ui, &mut p.max_peaks, &quantities::MAX_PEAKS)
        });
    }
    rows.row(tr!("analysis-field-range"), |ui| {
        optional_range(ui, id.with("peaks-range"), &mut p.range, extent);
    });
    rows.checkbox(&mut p.residuals, tr!("analysis-fit-residuals"), None);
}

fn reconv_form(rows: &mut Form<'_>, id: egui::Id, form: &mut AnalysisForm, tree: &Tree) {
    let extent = x_extent(form, tree);
    let r = &mut form.setup.reconv;
    rows.note(tr!("analysis-reconv-hint"));
    let before = r.spec.components;
    rows.row(tr!("analysis-reconv-components"), |ui| {
        whole(ui, &mut r.spec.components, &quantities::COMPONENTS)
    });
    if r.spec.components != before {
        r.resize_params();
    }
    rows.row(tr!("analysis-reconv-shift"), |ui| {
        number(ui, &mut r.spec.shift_initial);
        ui.checkbox(&mut r.spec.fit_shift, tr!("analysis-reconv-fit-shift"));
    });
    r.resize_params();
    let names = r.param_names();
    params_table(
        rows.ui(),
        id.with("reconv-params"),
        &names,
        &mut r.spec.params,
        false,
    );
    rows.row(tr!("analysis-field-range"), |ui| {
        optional_range(ui, id.with("reconv-window"), &mut r.spec.window, extent);
    });
    rows.checkbox(&mut r.residuals, tr!("analysis-fit-residuals"), None);
}

fn direct_form(rows: &mut Form<'_>, id: egui::Id, form: &mut AnalysisForm) {
    let d = &mut form.setup.direct;
    rows.note(tr!("analysis-direct-hint"));
    rows.field(|ui| {
        choice(
            ui,
            id.with("direct-algorithm"),
            &mut d.algorithm,
            &[
                ChoiceOption::new(
                    DirectAlgorithm::RichardsonLucy,
                    tr!("analysis-direct-richardson-lucy"),
                ),
                ChoiceOption::new(DirectAlgorithm::Wiener, tr!("analysis-direct-wiener")),
            ],
        )
    });
    match d.algorithm {
        DirectAlgorithm::RichardsonLucy => {
            rows.row(tr!("analysis-direct-iterations"), |ui| {
                whole(ui, &mut d.iterations, &quantities::ITERATIONS)
            });
        }
        DirectAlgorithm::Wiener => {
            rows.row(tr!("analysis-direct-noise"), |ui| {
                let id = ui.id().with("noise");
                optional_number(ui, id, &mut d.noise)
            });
        }
    }
    rows.row(tr!("analysis-reconv-shift"), |ui| number(ui, &mut d.shift));
}

fn output_block(
    rows: &mut Form<'_>,
    id: egui::Id,
    form: &mut AnalysisForm,
    graphs: &[(NodeId, String)],
    tree: &Tree,
) {
    rows.section(tr!("analysis-block-output"));
    if matches!(form.kind, OperationKind::Baseline | OperationKind::Correct)
        && form.editing.is_none()
    {
        let disabled = form.replacement_disabled(tree);
        if disabled.is_some() {
            form.replace_original = false;
        }
        rows.field(|ui| {
            choice(
                ui,
                id.with("original-data"),
                &mut form.replace_original,
                &[
                    ChoiceOption::new(false, tr!("analysis-output-new-column")),
                    ChoiceOption::new(true, tr!("analysis-output-replace-original"))
                        .disabled(disabled),
                ],
            )
        });
    } else {
        form.replace_original = false;
    }
    if form.replace_original {
        rows.note(tr!("analysis-output-replace-hint"));
        return;
    }
    // Les colonnes que cette famille peut écrire en plus de la principale.
    match form.kind {
        OperationKind::Baseline => {
            rows.checkbox(
                &mut form.setup.baseline.baseline_column,
                tr!("analysis-output-baseline-column"),
                None,
            );
        }
        OperationKind::Peaks => {
            rows.checkbox(
                &mut form.setup.peaks.peak_columns,
                tr!("analysis-output-peak-columns"),
                None,
            );
            rows.checkbox(
                &mut form.setup.peaks.sum_column,
                tr!("analysis-output-sum-column"),
                None,
            );
        }
        _ => {}
    }
    if form.editing.is_none() && !graphs.is_empty() {
        graph_target(rows, id, form, graphs);
    }
    rows.field(|ui| {
        choice(
            ui,
            id.with("keep-results"),
            &mut form.keep_results,
            &[
                ChoiceOption::new(false, tr!("analysis-output-show"))
                    .hint(tr!("analysis-output-show", "hint")),
                ChoiceOption::new(true, tr!("analysis-output-keep"))
                    .hint(tr!("analysis-output-keep", "hint")),
            ],
        )
    });
    if rows.checkbox(
        &mut form.auto_recalc,
        tr!("analysis-output-auto"),
        Some(tr!("analysis-output-auto", "hint")),
    ) {
        form.auto_recalc_chosen = true;
    }
    if !form.auto_recalc && form.series.len() > stokesforge_analysis::ops::HEAVY_BATCH {
        rows.note(tr!("analysis-output-heavy"));
    }
}

/// « Add to graph » (#200) : la case, et le choix du graphique quand plusieurs
/// montrent la courbe. Le graphique retenu reste valide : un choix qui n'est plus
/// parmi les candidats retombe sur le premier. Les graphiques sont des objets du
/// projet, pas un jeu d'options fixe : liste déroulante quel que soit leur nombre.
fn graph_target(
    rows: &mut Form<'_>,
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
    rows.checkbox(
        &mut form.add_to_graph,
        tr!("analysis-output-to-graph"),
        Some(tr!("analysis-output-to-graph", "hint")),
    );
    if form.add_to_graph && graphs.len() > 1 {
        let shown = graphs
            .iter()
            .find(|(id, _)| Some(*id) == form.graph)
            .map(|(_, name)| name.clone())
            .unwrap_or_default();
        rows.field(|ui| {
            egui::ComboBox::from_id_salt(id.with("target-graph"))
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (graph, name) in graphs {
                        ui.selectable_value(&mut form.graph, Some(*graph), name);
                    }
                });
        });
    }
}

/// Une valeur calculée sous l'aperçu : son libellé estompé, puis le nombre.
pub(super) fn value_row(
    ui: &mut egui::Ui,
    label: &str,
    value: f64,
    error: Option<f64>,
    unit: &str,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).weak());
        exact_result_label(
            ui,
            with_unit(format_value(value, error), unit),
            value,
            error,
        );
    });
}

#[cfg(test)]
mod tests {
    use eframe::egui;

    use super::super::test_support::{cliquer, contexte, image, texte_exact, textes, touche};
    use super::{parse_optional, range_rows};

    #[test]
    fn un_nombre_facultatif_lit_auto_comme_le_champ_nombre() {
        // #247 : `auto` et `a` valent un champ vidé.
        for texte in ["", "  ", "auto", "Auto", " AUTO ", "a", "A"] {
            assert_eq!(parse_optional(texte), Some(None), "{texte:?}");
        }
        assert_eq!(parse_optional("2.5"), Some(Some(2.5)));
        assert_eq!(parse_optional("abc"), None);
    }

    fn cadres_rouges(shapes: &[egui::epaint::ClippedShape]) -> usize {
        let rouge = crate::theme::field_refused_color();
        shapes
            .iter()
            .filter(|peint| {
                matches!(&peint.shape, egui::Shape::Rect(rect) if rect.stroke.color == rouge)
            })
            .count()
    }

    /// Tape `saisie` dans le champ « à » de la plage `(1, 5)`, valide par Entrée,
    /// et rend la plage et les cadres rouges de l'image suivante.
    fn saisir_dans_a(saisie: &str) -> ((f64, f64), usize) {
        let context = contexte();
        let mut plages = vec![(1.0, 5.0)];
        let id = egui::Id::new("essai");
        let peindre = |plages: &mut Vec<(f64, f64)>, events: Vec<egui::Event>| {
            image(&context, events, |ui| {
                range_rows(ui, id, plages, None);
            })
        };
        peindre(&mut plages, vec![]);
        let formes = peindre(&mut plages, vec![]);
        let champ = texte_exact(&textes(&formes), "5").centre;
        cliquer(&context, champ, egui::PointerButton::Primary, |ui| {
            range_rows(ui, id, &mut plages, None);
        });
        peindre(&mut plages, vec![egui::Event::Text(saisie.into())]);
        peindre(&mut plages, vec![touche(egui::Key::Enter)]);
        let formes = peindre(&mut plages, vec![]);
        (plages[0], cadres_rouges(&formes))
    }

    /// « de » ≤ « à » : une paire mise à l'envers est échangée sans bruit,
    /// sans cadre rouge.
    #[test]
    fn une_plage_inversee_est_echangee_sans_bruit() {
        assert_eq!(saisir_dans_a("0"), ((0.0, 1.0), 0));
    }

    /// Une plage facultative accepte une borne seule : taper « min » d'abord ne
    /// l'efface pas, et la borne laissée « auto » prend l'étendue des données.
    #[test]
    fn une_borne_tapee_seule_n_est_pas_effacee() {
        let context = contexte();
        let mut plage: Option<(f64, f64)> = None;
        let id = egui::Id::new("facultative");
        let extent = Some((0.0, 100.0));
        let peindre = |plage: &mut Option<(f64, f64)>, events: Vec<egui::Event>| {
            image(&context, events, |ui| {
                super::optional_range(ui, id, plage, extent);
            })
        };
        peindre(&mut plage, vec![]);
        let formes = peindre(&mut plage, vec![]);
        let champ = textes(&formes)
            .into_iter()
            .find(|t| t.texte == "auto")
            .expect("le champ « de » est vide")
            .centre;
        cliquer(&context, champ, egui::PointerButton::Primary, |ui| {
            super::optional_range(ui, id, &mut plage, extent);
        });
        peindre(&mut plage, vec![egui::Event::Text("30".into())]);
        let formes = peindre(&mut plage, vec![]);
        assert_eq!(plage, Some((30.0, 100.0)));
        assert!(
            textes(&formes).iter().any(|t| t.texte == "30"),
            "la borne tapée reste affichée"
        );
    }

    #[test]
    fn une_plage_ordonnee_est_appliquee() {
        assert_eq!(saisir_dans_a("9"), ((1.0, 9.0), 0));
    }

    /// Taper deux chiffres dans « à » sous une borne « de » plus grande : le
    /// texte tapé reste en place pendant la frappe (pas d'échange sous le
    /// curseur, qui donnait « 6960 »), et le modèle reçoit la paire ordonnée.
    #[test]
    fn taper_sous_une_borne_plus_grande_n_echange_pas_pendant_la_frappe() {
        let context = contexte();
        let mut plages = vec![(660.0, 700.0)];
        let id = egui::Id::new("frappe");
        let peindre = |plages: &mut Vec<(f64, f64)>, events: Vec<egui::Event>| {
            image(&context, events, |ui| {
                range_rows(ui, id, plages, None);
            })
        };
        peindre(&mut plages, vec![]);
        let formes = peindre(&mut plages, vec![]);
        let champ = texte_exact(&textes(&formes), "700").centre;
        cliquer(&context, champ, egui::PointerButton::Primary, |ui| {
            range_rows(ui, id, &mut plages, None);
        });
        peindre(&mut plages, vec![egui::Event::Text("9".into())]);
        let formes = peindre(&mut plages, vec![egui::Event::Text("9".into())]);
        let affiches: Vec<String> = textes(&formes).into_iter().map(|t| t.texte).collect();
        assert!(affiches.contains(&"660".to_string()), "{affiches:?}");
        assert!(affiches.contains(&"99".to_string()), "{affiches:?}");
        assert_eq!(plages[0], (99.0, 660.0));
    }

    #[test]
    fn peindre_une_constante_ne_detruit_pas_les_reglages_des_autres_formes() {
        use stokesforge_app_state::analysis::{AnalysisForm, BaselineShape, Preset};
        let context = contexte();
        let mut form = AnalysisForm::new(&Preset::BaselineAnchors, vec![], false);
        let points = vec![(1.0, 2.0), (3.0, 4.0), (5.0, 6.0)];
        let zones = vec![(1.0, 2.0), (3.0, 4.0)];
        form.setup.baseline.points = points.clone();
        form.setup.baseline.zones = zones.clone();
        form.setup.baseline.constant_point = Some((7.0, 8.0));
        form.setup.baseline.constant_zone = Some((7.0, 9.0));
        for shape in [
            BaselineShape::Constant,
            BaselineShape::ConstantZone,
            BaselineShape::Anchors,
            BaselineShape::Zones,
        ] {
            form.setup.baseline.shape = shape;
            image(&context, vec![], |ui| {
                super::super::form::form(ui, "constante", |rows| {
                    super::baseline_form(rows, egui::Id::new("constante"), &mut form, None);
                });
            });
            assert_eq!(form.setup.baseline.points, points);
            assert_eq!(form.setup.baseline.zones, zones);
            assert_eq!(form.setup.baseline.constant_point, Some((7.0, 8.0)));
            assert_eq!(form.setup.baseline.constant_zone, Some((7.0, 9.0)));
        }
    }

    /// #252 : la colonne de référence se choisit sous « Analysis type », dans
    /// le bloc de la famille, comme les réglages de la Baseline — pas sous
    /// l'entrée.
    #[test]
    fn la_reference_vient_sous_le_type_d_analyse() {
        use stokesforge_app_state::analysis::{AnalysisForm, Preset};
        use stokesforge_core::Project;
        let context = contexte();
        let project = Project::new("essai").unwrap();
        let tree = project.tree().clone();
        let mut form = AnalysisForm::new(&Preset::Correct, vec![], false);
        let formes = image(&context, vec![], |ui| {
            super::form_body(ui, egui::Id::new("essai"), &mut form, &tree, &[]);
        });
        let peints = textes(&formes);
        let y = |libelle: String| texte_exact(&peints, &libelle).centre.y;
        let entree = y(stokesforge_app_state::tr!("analysis-block-input"));
        let type_ = y(stokesforge_app_state::tr!("analysis-block-type"));
        let reference = y(stokesforge_app_state::tr!("analysis-field-reference"));
        assert!(entree < type_, "Input au-dessus d'Analysis type");
        assert!(type_ < reference, "Reference sous Analysis type");
    }
}
