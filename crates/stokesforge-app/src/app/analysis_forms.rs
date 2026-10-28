//! The settings of the analysis window (#181): three blocks, the **input**
//! (series, reference), the **family settings** and the **output** (Show /
//! Keep, automatic recalculation, columns written).
//!
//! Each control writes into [`AnalysisForm`]; nothing here decides or
//! computes (`analysis.rs`), and nothing sends an action: the window
//! validates everything at once.

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
    BaselineShape, Category, DirectAlgorithm, Preset, SmoothAlgorithm, SmoothUnit,
};
use stokesforge_app_state::quantities;
use stokesforge_app_state::r_squared::RSquaredLevel;
use stokesforge_app_state::tr;

/// An editable real number, without bounds but **finite** (the building block
/// refuses "inf" and "nan": they would become `null` in the file and lose the
/// calculation on reload). Its identifier is that of its place in the row.
fn number(ui: &mut egui::Ui, value: &mut f64) -> bool {
    number_response(ui, value).changed
}

/// [`number`], with the field's response (to paint a refusal on it, S-N11).
fn number_response(ui: &mut egui::Ui, value: &mut f64) -> NumberResponse {
    let salt = ui.next_auto_id();
    ui.skip_ahead_auto_ids(1);
    number_field(ui, salt, value, &quantities::FREE)
}

/// An integer bounded by its magnitude; same identifier as [`number`].
fn whole<T: crate::number_field::Number>(
    ui: &mut egui::Ui,
    value: &mut T,
    quantity: &quantities::Quantity,
) -> bool {
    let salt = ui.next_auto_id();
    ui.skip_ahead_auto_ids(1);
    number_field(ui, salt, value, quantity).changed
}

/// An optional number: an empty text field, `auto` or `a` means "automatic".
/// The text being typed is kept outside the model while it is not a number.
fn optional_number(ui: &mut egui::Ui, id: egui::Id, value: &mut Option<f64>) -> bool {
    optional_number_response(ui, id, value).0
}

/// What the input of an optional number says: `Some(None)` for Auto (emptied
/// field, word `auto` or letter `a`, like `number_field::read`, #247),
/// `Some(Some(n))` for a number, `None` for an unreadable input.
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

/// [`optional_number`], with the field's response (to paint a refusal on it, S-N11).
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
    // Like `number_field`: taking focus selects all the text.
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
    // A text that is not a number, or that says "auto" (`a`, `auto`), is
    // emptied on leaving the field: the bound becomes "auto" again and the
    // field shows its hint. While typing it is tolerated: "-" or "1e"
    // come before a number.
    if response.lost_focus() && !matches!(parse_optional(&text), Some(Some(_))) {
        text.clear();
        changed = value.is_some();
        *value = None;
    }
    ui.data_mut(|d| d.insert_temp(id, (*value, text)));
    (changed, response)
}

/// The list of ranges `(from, to)`, one row per range.
///
/// Like [`optional_range`], each empty bound ("auto") takes the extent of the
/// data `extent`, and an unreadable text is emptied. The typed bounds live in
/// the field state (`pending`), because the model carries only the resolved
/// range. A reversed pair is swapped silently.
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
            // `committed`: the range this field wrote itself; if the model
            // differs from it (drawn with the mouse), the model prevails.
            let (mut a, mut b) = match ui.data(|d| d.get_temp::<Pending>(state_id)) {
                Some((a, b, committed)) if committed == *row => (a, b),
                _ => (Some(row.0), Some(row.1)),
            };
            let (changed_a, resp_a) = optional_number_response(ui, id.with(("a", i)), &mut a);
            ui.label(tr!("analysis-field-to"));
            let (changed_b, resp_b) = optional_number_response(ui, id.with(("b", i)), &mut b);
            // No swap while typing (it would move the text under the
            // cursor): the model receives the ordered pair, the display
            // follows when both fields are left.
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
        // A new row covers the whole series ("auto" bounds).
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

/// The list of points `(x, y)`, one row per point.
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

/// An optional range: two fields and a button to remove it.
///
/// Each bound is typed alone: an empty bound ("auto") takes the extent of the
/// data `extent`; with no known extent, the range stays absent until both
/// bounds are given. A reversed pair is swapped silently. The typed bounds
/// live in the field state (`pending`), because the model carries only a
/// complete range: without that, a bound typed alone would be erased at the
/// next render.
fn optional_range(
    ui: &mut egui::Ui,
    id: egui::Id,
    range: &mut Option<(f64, f64)>,
    extent: Option<(f64, f64)>,
) {
    type Pending = (Option<f64>, Option<f64>, Option<(f64, f64)>);
    ui.horizontal(|ui| {
        let state_id = id.with("pending");
        // `committed`: the range this field wrote itself. If the model
        // differs from it (drawn with the mouse, remove button, another form),
        // the model prevails.
        let (mut a, mut b) = match ui.data(|d| d.get_temp::<Pending>(state_id)) {
            Some((a, b, committed)) if committed == *range => (a, b),
            _ => (range.map(|r| r.0), range.map(|r| r.1)),
        };
        let (changed_a, resp_a) = optional_number_response(ui, id.with("a"), &mut a);
        ui.label(tr!("analysis-field-to"));
        let (changed_b, resp_b) = optional_number_response(ui, id.with("b"), &mut b);
        // No swap while typing (it would move the text under the cursor): the
        // model receives the ordered pair, the display follows when both
        // fields are left.
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

/// The whole form: input, family, output, in a two-column grid (#227) whose
/// fields align from one block to the next.
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

/// A list of rows (ranges, points, peaks) in the fields column.
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

/// The selector of the auxiliary column (Reference / Response), at the top of
/// the family block, under "Analysis type" (#252).
fn aux_block(rows: &mut Form<'_>, id: egui::Id, form: &mut AnalysisForm, tree: &Tree) {
    if form.needs_aux() {
        let candidates = aux_candidates(tree, &form.series);
        let selected = form
            .aux
            .as_ref()
            .and_then(|a| {
                // A frozen reference (#248) is not a column to choose: it
                // reads "copy of ...".
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
        // A list of project columns, not a fixed set of options: a dropdown
        // whatever their number.
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

/// The title of the family; on creation, the "Analysis type" choice among
/// only the families of the window's category (#248), which changes them
/// live. A category with a single family (Fitting, Integrate) has nothing to
/// choose. A reopened calculation keeps its family: only its method changes.
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
        OperationKind::Smooth => smooth_form(rows, id, form),
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

/// The name of a model in the dropdown.
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

/// The X extent of the current input: what a bound left "auto" is worth.
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
    // A chosen model is worth its variant: the degree of a polynomial or the
    // free formula in progress stay those of the form.
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

/// The table of the parameters of a fit: starting value, fixed, bounds and,
/// for a batch, shared between the series (global fit).
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
    // Three shapes: radios (S-C1), plus a dropdown.
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

fn smooth_form(rows: &mut Form<'_>, id: egui::Id, form: &mut AnalysisForm) {
    let s = &mut form.setup.smooth;
    rows.note(tr!("analysis-smooth-hint"));
    rows.field(|ui| {
        choice(
            ui,
            id.with("smooth-algorithm"),
            &mut s.algorithm,
            &[
                ChoiceOption::new(
                    SmoothAlgorithm::AdjacentAverage,
                    tr!("analysis-smooth-choice-adjacent"),
                ),
                ChoiceOption::new(
                    SmoothAlgorithm::SavitzkyGolay,
                    tr!("analysis-smooth-choice-savitzky-golay"),
                ),
                ChoiceOption::new(
                    SmoothAlgorithm::Median,
                    tr!("analysis-smooth-choice-median"),
                ),
            ],
        )
    });
    rows.field(|ui| {
        choice(
            ui,
            id.with("smooth-unit"),
            &mut s.unit,
            &[
                ChoiceOption::new(SmoothUnit::Points, tr!("analysis-smooth-unit-points")),
                ChoiceOption::new(SmoothUnit::XUnits, tr!("analysis-smooth-unit-x")),
            ],
        )
    });
    match s.unit {
        SmoothUnit::Points => {
            rows.row(tr!("analysis-smooth-window-points"), |ui| {
                let mut points = s.points;
                let changed = whole(ui, &mut points, &quantities::SMOOTH_WINDOW_POINTS);
                if changed {
                    s.set_points(points);
                }
                changed
            });
        }
        SmoothUnit::XUnits => {
            rows.row(tr!("analysis-smooth-window-x"), |ui| {
                let salt = ui.next_auto_id();
                ui.skip_ahead_auto_ids(1);
                number_field(ui, salt, &mut s.x_width, &quantities::SMOOTH_WINDOW_X).changed
            });
        }
    }
    if s.algorithm == SmoothAlgorithm::SavitzkyGolay {
        rows.row(tr!("analysis-smooth-order"), |ui| {
            whole(ui, &mut s.order, &quantities::SMOOTH_ORDER)
        });
    }
}

fn output_block(
    rows: &mut Form<'_>,
    id: egui::Id,
    form: &mut AnalysisForm,
    graphs: &[(NodeId, String)],
    tree: &Tree,
) {
    rows.section(tr!("analysis-block-output"));
    if matches!(
        form.kind,
        OperationKind::Baseline | OperationKind::Correct | OperationKind::Smooth
    ) && form.editing.is_none()
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
    // The columns this family can write besides the main one.
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
    rows.checkbox(
        &mut form.low_r2_dialog,
        tr!("analysis-output-low-r2"),
        Some(tr!("analysis-output-low-r2", "hint")),
    );
    if !form.auto_recalc && form.series.len() > stokesforge_analysis::ops::HEAVY_BATCH {
        rows.note(tr!("analysis-output-heavy"));
    }
}

/// "Add to graph" (#200): the box, and the choice of the graph when several
/// show the curve. The retained graph stays valid: a choice that is no longer
/// among the candidates falls back to the first. Graphs are objects of the
/// project, not a fixed set of options: a dropdown whatever their number.
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

/// A value computed below the preview: its dimmed label, then the number.
pub(super) fn value_row(
    ui: &mut egui::Ui,
    label: &str,
    value: f64,
    error: Option<f64>,
    level: RSquaredLevel,
    unit: &str,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).weak());
        exact_result_label(
            ui,
            with_unit(format_value(value, error), unit),
            value,
            error,
            level,
        );
    });
}

#[cfg(test)]
mod tests {
    use eframe::egui;

    use super::super::test_support::{click, context, image, key, text_exact, texts};
    use super::{parse_optional, range_rows};

    #[test]
    fn an_optional_number_reads_auto_like_the_number_field() {
        // #247: `auto` and `a` mean an emptied field.
        for text in ["", "  ", "auto", "Auto", " AUTO ", "a", "A"] {
            assert_eq!(parse_optional(text), Some(None), "{text:?}");
        }
        assert_eq!(parse_optional("2.5"), Some(Some(2.5)));
        assert_eq!(parse_optional("abc"), None);
    }

    fn red_frames(shapes: &[egui::epaint::ClippedShape]) -> usize {
        let red = crate::theme::field_refused_color();
        shapes
            .iter()
            .filter(|painted_shape| {
                matches!(&painted_shape.shape, egui::Shape::Rect(rect) if rect.stroke.color == red)
            })
            .count()
    }

    /// Types `typed` in the "to" field of the range `(1, 5)`, validates with Enter,
    /// and returns the range and the red frames of the next frame.
    fn type_in_to(typed: &str) -> ((f64, f64), usize) {
        let context = context();
        let mut ranges = vec![(1.0, 5.0)];
        let id = egui::Id::new("trial");
        let paint = |ranges: &mut Vec<(f64, f64)>, events: Vec<egui::Event>| {
            image(&context, events, |ui| {
                range_rows(ui, id, ranges, None);
            })
        };
        paint(&mut ranges, vec![]);
        let shapes = paint(&mut ranges, vec![]);
        let field = text_exact(&texts(&shapes), "5").center;
        click(&context, field, egui::PointerButton::Primary, |ui| {
            range_rows(ui, id, &mut ranges, None);
        });
        paint(&mut ranges, vec![egui::Event::Text(typed.into())]);
        paint(&mut ranges, vec![key(egui::Key::Enter)]);
        let shapes = paint(&mut ranges, vec![]);
        (ranges[0], red_frames(&shapes))
    }

    /// "from" <= "to": a reversed pair is swapped silently,
    /// without a red frame.
    #[test]
    fn a_reversed_range_is_swapped_silently() {
        assert_eq!(type_in_to("0"), ((0.0, 1.0), 0));
    }

    /// An optional range accepts a single bound: typing "min" first does not
    /// erase it, and the bound left "auto" takes the extent of the data.
    #[test]
    fn a_bound_typed_alone_is_not_erased() {
        let context = context();
        let mut range: Option<(f64, f64)> = None;
        let id = egui::Id::new("optional");
        let extent = Some((0.0, 100.0));
        let paint = |range: &mut Option<(f64, f64)>, events: Vec<egui::Event>| {
            image(&context, events, |ui| {
                super::optional_range(ui, id, range, extent);
            })
        };
        paint(&mut range, vec![]);
        let shapes = paint(&mut range, vec![]);
        let field = texts(&shapes)
            .into_iter()
            .find(|t| t.text == "auto")
            .expect("the \"from\" field is empty")
            .center;
        click(&context, field, egui::PointerButton::Primary, |ui| {
            super::optional_range(ui, id, &mut range, extent);
        });
        paint(&mut range, vec![egui::Event::Text("30".into())]);
        let shapes = paint(&mut range, vec![]);
        assert_eq!(range, Some((30.0, 100.0)));
        assert!(
            texts(&shapes).iter().any(|t| t.text == "30"),
            "the typed bound stays displayed"
        );
    }

    #[test]
    fn an_ordered_range_is_applied() {
        assert_eq!(type_in_to("9"), ((1.0, 9.0), 0));
    }

    /// Typing two digits in "to" under a larger "from" bound: the typed text
    /// stays in place while typing (no swap under the cursor, which gave
    /// "6960"), and the model receives the ordered pair.
    #[test]
    fn typing_under_a_larger_bound_does_not_swap_while_typing() {
        let context = context();
        let mut ranges = vec![(660.0, 700.0)];
        let id = egui::Id::new("typing");
        let paint = |ranges: &mut Vec<(f64, f64)>, events: Vec<egui::Event>| {
            image(&context, events, |ui| {
                range_rows(ui, id, ranges, None);
            })
        };
        paint(&mut ranges, vec![]);
        let shapes = paint(&mut ranges, vec![]);
        let field = text_exact(&texts(&shapes), "700").center;
        click(&context, field, egui::PointerButton::Primary, |ui| {
            range_rows(ui, id, &mut ranges, None);
        });
        paint(&mut ranges, vec![egui::Event::Text("9".into())]);
        let shapes = paint(&mut ranges, vec![egui::Event::Text("9".into())]);
        let shown: Vec<String> = texts(&shapes).into_iter().map(|t| t.text).collect();
        assert!(shown.contains(&"660".to_string()), "{shown:?}");
        assert!(shown.contains(&"99".to_string()), "{shown:?}");
        assert_eq!(ranges[0], (99.0, 660.0));
    }

    #[test]
    fn painting_a_constant_does_not_destroy_the_settings_of_the_other_shapes() {
        use stokesforge_app_state::analysis::{AnalysisForm, BaselineShape, Preset};
        let context = context();
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
                super::super::form::form(ui, "constant", |rows| {
                    super::baseline_form(rows, egui::Id::new("constant"), &mut form, None);
                });
            });
            assert_eq!(form.setup.baseline.points, points);
            assert_eq!(form.setup.baseline.zones, zones);
            assert_eq!(form.setup.baseline.constant_point, Some((7.0, 8.0)));
            assert_eq!(form.setup.baseline.constant_zone, Some((7.0, 9.0)));
        }
    }

    /// #252: the reference column is chosen under "Analysis type", in the
    /// family block, like the Baseline settings, not under the input.
    #[test]
    fn the_reference_comes_under_the_analysis_type() {
        use stokesforge_app_state::analysis::{AnalysisForm, Preset};
        use stokesforge_core::Project;
        let context = context();
        let project = Project::new("trial").unwrap();
        let tree = project.tree().clone();
        let mut form = AnalysisForm::new(&Preset::Correct, vec![], false);
        let shapes = image(&context, vec![], |ui| {
            super::form_body(ui, egui::Id::new("trial"), &mut form, &tree, &[]);
        });
        let painted = texts(&shapes);
        let y = |label: String| text_exact(&painted, &label).center.y;
        let input = y(stokesforge_app_state::tr!("analysis-block-input"));
        let type_ = y(stokesforge_app_state::tr!("analysis-block-type"));
        let reference = y(stokesforge_app_state::tr!("analysis-field-reference"));
        assert!(input < type_, "Input above Analysis type");
        assert!(type_ < reference, "Reference below Analysis type");
    }
}
