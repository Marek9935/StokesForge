//! The analysis window model (#181), without interface: defaults per
//! operation, what is missing to validate, the described operation, the preview
//! and the saved batch (long names M6, requested columns, metadata kept on
//! edit).

use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{Operation, OperationKind, HEAVY_BATCH};
use stokesforge_app_state::analysis::{
    model_formula_source, preset_of, value_unit, with_unit, AnalysisForm, BaselineShape, Issue,
    Preset, DEFAULT_AVERAGE,
};
use stokesforge_app_state::analysis::{Edge, HandleSlot, RangeSet};
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_core::{Column, ColumnMeta, NodeId, Project, SeriesRef};

struct Fixture {
    project: Project,
    x: NodeId,
    y: NodeId,
}

/// A line `y = 2x + 1` laid on 21 points, Y column "Intensity".
fn fixture() -> Fixture {
    i18n::set_language(Language::English);
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let xs: Vec<f64> = (0..21).map(f64::from).collect();
    let ys: Vec<f64> = xs.iter().map(|x| 2.0 * x + 1.0).collect();
    let x = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&xs))
        .unwrap();
    let y = project
        .add_column(sheet, None, Some("B"), Column::from_f64(&ys))
        .unwrap();
    let meta = ColumnMeta {
        long_name: "Intensity".to_string(),
        unit: "counts".to_string(),
        ..ColumnMeta::default()
    };
    project.set_column_meta(y, meta).unwrap();
    Fixture { project, x, y }
}

fn series(f: &Fixture) -> Vec<SeriesRef> {
    vec![SeriesRef {
        x: Some(f.x),
        y: f.y,
        sigma: None,
    }]
}

#[test]
fn a_new_form_starts_on_the_chosen_family_and_method() {
    let f = fixture();
    let form = AnalysisForm::new(&Preset::BaselineLine, series(&f), false);
    assert_eq!(form.kind, OperationKind::Baseline);
    assert_eq!(form.setup.baseline.shape, BaselineShape::TwoPoints);

    let form = AnalysisForm::new(&Preset::BaselinePolynomial, series(&f), false);
    assert_eq!(form.setup.baseline.shape, BaselineShape::Zones);
    assert_eq!(form.setup.baseline.degree, 3);

    let form = AnalysisForm::new(&Preset::Fit(Model::Gaussian), series(&f), false);
    assert_eq!(form.kind, OperationKind::Fit);
    assert_eq!(form.setup.fit.model, Model::Gaussian);
    assert_eq!(form.setup.fit.params.len(), form.setup.fit.param_count());
    assert!(form.setup.fit.param_count() >= 3);
    assert_eq!(form.average, DEFAULT_AVERAGE);
}

#[test]
fn recalculating_automatically_follows_the_operation_until_the_user_chooses() {
    let f = fixture();
    let mut form = AnalysisForm::new(&Preset::Correct, series(&f), false);
    assert!(form.auto_recalc);
    form.set_kind(OperationKind::Reconvolution);
    assert!(!form.auto_recalc, "reconvolution defaults to manual");
    form.set_kind(OperationKind::Fit);
    assert!(form.auto_recalc, "back to automatic while untouched");

    form.auto_recalc = false;
    form.auto_recalc_chosen = true;
    form.set_kind(OperationKind::Integrate);
    assert!(!form.auto_recalc, "a choice made by the user is kept");

    let many: Vec<SeriesRef> = (0..=HEAVY_BATCH).flat_map(|_| series(&f)).collect();
    let form = AnalysisForm::new(&Preset::Fit(Model::Linear), many, false);
    assert!(!form.auto_recalc, "a heavy batch defaults to manual");
}

#[test]
fn what_is_missing_is_named_before_any_computation() {
    let f = fixture();
    let form = AnalysisForm::new(&Preset::Correct, series(&f), false);
    assert_eq!(form.issues(), vec![Issue::NeedsReference]);
    let form = AnalysisForm::new(&Preset::Reconvolution, series(&f), false);
    assert_eq!(form.issues(), vec![Issue::NeedsResponse]);
    let form = AnalysisForm::new(&Preset::Integrate, Vec::new(), false);
    assert_eq!(form.issues(), vec![Issue::NoSeries]);

    let mut form = AnalysisForm::new(&Preset::BaselineLine, series(&f), false);
    assert_eq!(
        form.issues(),
        vec![Issue::NeedsPoints { needed: 2, have: 0 }]
    );
    assert_eq!(form.operation(), None);
    form.setup.baseline.points = vec![(0.0, 1.0)];
    assert_eq!(
        form.issues(),
        vec![Issue::NeedsPoints { needed: 2, have: 1 }]
    );
    form.setup.baseline.points.push((20.0, 41.0));
    assert!(form.issues().is_empty());

    let mut form = AnalysisForm::new(&Preset::BaselinePolynomial, series(&f), false);
    assert_eq!(form.issues(), vec![Issue::NeedsZone]);
    form.setup.baseline.zones.push((0.0, 5.0));
    assert!(form.issues().is_empty());

    let mut form = AnalysisForm::new(&Preset::Fit(Model::Linear), series(&f), false);
    form.setup.fit.set_model(Model::Custom {
        formula: "a*".to_string(),
    });
    assert!(matches!(form.issues().as_slice(), [Issue::BadModel(_)]));
}

/// #260: the draft is described as an `Operation` and `Operation::readiness`
/// says what is missing, including for the single point or zone.
#[test]
fn a_constant_baseline_names_its_missing_point_or_zone() {
    let f = fixture();
    let mut form = AnalysisForm::new(&Preset::BaselineConstant, series(&f), false);
    assert_eq!(
        form.issues(),
        vec![Issue::NeedsPoints { needed: 1, have: 0 }]
    );
    form.add_point(3.0, 7.0);
    assert!(form.issues().is_empty());
    form.setup.baseline.shape = BaselineShape::ConstantZone;
    assert_eq!(form.issues(), vec![Issue::NeedsZone]);
    form.add_zone(1.0, 5.0);
    assert!(form.issues().is_empty());
}

/// #260: a degree above the cap (read from a file) is a refused prerequisite,
/// not an allocation of billions of parameters.
#[test]
fn a_huge_polynomial_degree_is_an_issue_not_an_allocation() {
    let f = fixture();
    let mut form = AnalysisForm::new(&Preset::Fit(Model::Linear), series(&f), false);
    form.setup.fit.model = Model::Polynomial { degree: u32::MAX };
    assert!(matches!(
        form.issues().as_slice(),
        [Issue::Failed(
            stokesforge_analysis::AnalysisError::InvalidParameter { .. }
        )]
    ));
    assert_eq!(form.setup.fit.param_count(), 0);
    assert_eq!(form.operation(), None);
}

/// The "Auto" button only looks at the fitted range.
#[test]
fn auto_reads_only_the_fitted_range() {
    let f = fixture();
    let mut form = AnalysisForm::new(&Preset::Fit(Model::Linear), series(&f), false);
    let xs: Vec<f64> = (0..50).map(f64::from).collect();
    let ys: Vec<f64> = xs
        .iter()
        .map(|x| {
            if (10.0..=20.0).contains(x) {
                3.0 * x + 1.0
            } else {
                -9000.0
            }
        })
        .collect();
    form.setup.fit.range = Some((20.0, 10.0));
    form.setup.fit.auto_initial(&xs, &ys);
    let initial: Vec<f64> = form
        .setup
        .fit
        .params
        .iter()
        .map(|p| p.initial.expect("a starting value"))
        .collect();
    assert!((initial[0] - 1.0).abs() < 1e-6 && (initial[1] - 3.0).abs() < 1e-6);
    form.setup.fit.range = None;
    form.setup.fit.auto_initial(&xs, &ys);
    assert!((form.setup.fit.params[1].initial.unwrap() - 3.0).abs() > 1.0);
}

#[test]
fn the_operation_reads_only_the_active_family() {
    let f = fixture();
    let mut form = AnalysisForm::new(&Preset::BaselineLine, series(&f), false);
    form.setup.baseline.points = vec![(0.0, 1.0), (20.0, 41.0)];
    form.setup.baseline.op = BaselineOp::Divide;
    form.setup.baseline.baseline_column = true;
    assert_eq!(
        form.operation(),
        Some(Operation::Baseline {
            method: BaselineMethod::Line {
                p1: (0.0, 1.0),
                p2: (20.0, 41.0)
            },
            op: BaselineOp::Divide,
            keep_baseline: true,
        })
    );
    // Changing family loses nothing of the baseline.
    form.set_kind(OperationKind::Integrate);
    assert!(matches!(
        form.operation(),
        Some(Operation::Integrate { .. })
    ));
    form.set_kind(OperationKind::Baseline);
    assert_eq!(form.setup.baseline.points.len(), 2);
}

#[test]
fn a_form_rebuilt_from_its_operation_describes_the_same_operation() {
    let mut f = fixture();
    let mut form = AnalysisForm::new(&Preset::Fit(Model::Linear), series(&f), false);
    form.setup.fit.residuals = true;
    form.setup.fit.range = Some((2.0, 18.0));
    let op = form.operation().unwrap();
    assert_eq!(preset_of(&op), Preset::Fit(Model::Linear));

    let draft = form.draft(f.project.tree()).unwrap();
    assert_eq!(draft.operation, op);
    let id = f.project.add_calculation(draft).unwrap();
    let calc = f.project.tree().calculation(id).unwrap().clone();
    let again = AnalysisForm::from_calculation(id, &calc);
    assert_eq!(again.operation(), Some(op));
    assert_eq!(again.editing, Some(id));
    assert!(again.setup.fit.residuals);
}

#[test]
fn the_saved_batch_names_columns_after_their_source() {
    let f = fixture();
    let mut form = AnalysisForm::new(&Preset::BaselineLine, series(&f), true);
    form.setup.baseline.points = vec![(0.0, 1.0), (20.0, 41.0)];
    form.setup.baseline.baseline_column = true;
    let draft = form.draft(f.project.tree()).unwrap();
    let names: Vec<(&str, &str)> = draft
        .outputs
        .iter()
        .map(|o| (o.key.as_str(), o.meta.long_name.as_str()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("corrected", "Intensity (B-corr)"),
            ("baseline", "Baseline of B")
        ]
    );
    assert!(draft.outputs.iter().all(|o| o.meta.unit == "counts"));
    assert!(draft.outputs.iter().all(|o| o.meta.comment == "Correction"));
    // The laid line is exactly the curve: the correction is zero.
    assert!(draft.outputs[0].data.iter().all(|v| v.abs() < 1e-9));
    assert!(draft.keep_results);
    assert_eq!(draft.results.len(), 1);

    form.setup.baseline.baseline_column = false;
    form.keep_results = false;
    let draft = form.draft(f.project.tree()).unwrap();
    assert_eq!(draft.outputs.len(), 1, "the baseline column is not asked");
    assert!(draft.results.is_empty(), "Show: no results are kept");
}

#[test]
fn the_preview_computes_the_help_columns_even_when_they_are_not_written() {
    let f = fixture();
    let mut form = AnalysisForm::new(&Preset::BaselineLine, series(&f), false);
    form.setup.baseline.points = vec![(0.0, 1.0), (20.0, 41.0)];
    let preview = form.preview(f.project.tree()).unwrap();
    assert!(preview.columns.iter().any(|c| c.key == "baseline"));
    let draft = form.draft(f.project.tree()).unwrap();
    assert!(draft.outputs.iter().all(|o| o.key != "baseline"));
}

#[test]
fn a_missing_input_column_is_an_issue_not_a_panic() {
    let f = fixture();
    let mut form = AnalysisForm::new(&Preset::Integrate, series(&f), false);
    form.series[0].y = NodeId::from_raw(999_999);
    assert_eq!(form.preview(f.project.tree()), Err(Issue::MissingColumn));
}

#[test]
fn editing_keeps_the_metadata_the_user_gave_to_an_output() {
    let f = fixture();
    let series = series(&f);
    let mut project = f.project;
    let mut form = AnalysisForm::new(&Preset::BaselineLine, series, false);
    form.setup.baseline.points = vec![(0.0, 1.0), (20.0, 41.0)];
    let id = project
        .add_calculation(form.draft(project.tree()).unwrap())
        .unwrap();
    let calc = project.tree().calculation(id).unwrap().clone();
    let out = calc.outputs[0].column.unwrap();
    let mut meta = project
        .tree()
        .node(out)
        .unwrap()
        .as_column()
        .unwrap()
        .meta()
        .clone();
    meta.long_name = "My own name".to_string();
    project.set_column_meta(out, meta).unwrap();

    let mut edit = AnalysisForm::from_calculation(id, project.tree().calculation(id).unwrap());
    edit.setup.baseline.op = BaselineOp::Divide;
    let draft = edit.draft(project.tree()).unwrap();
    assert_eq!(draft.outputs[0].meta.long_name, "My own name");
}

#[test]
fn preview_gestures_write_into_the_active_family() {
    let f = fixture();
    // Line through two points: the third replaces the oldest.
    let mut form = AnalysisForm::new(&Preset::BaselineLine, series(&f), false);
    for p in [(1.0, 1.0), (2.0, 2.0), (3.0, 3.0)] {
        form.add_point(p.0, p.1);
    }
    assert_eq!(form.setup.baseline.points, vec![(2.0, 2.0), (3.0, 3.0)]);
    // Anchors: sorted by X.
    let mut form = AnalysisForm::new(&Preset::BaselineAnchors, series(&f), false);
    form.add_point(5.0, 1.0);
    form.add_point(1.0, 2.0);
    assert_eq!(form.setup.baseline.points, vec![(1.0, 2.0), (5.0, 1.0)]);
    assert!(form.remove_near(5.2, 0.5));
    assert!(!form.remove_near(9.0, 0.5), "too far");
    assert_eq!(form.setup.baseline.points, vec![(1.0, 2.0)]);
    // Zones: bounds put back in order, an empty zone is ignored.
    let mut form = AnalysisForm::new(&Preset::BaselinePolynomial, series(&f), false);
    form.add_zone(8.0, 2.0);
    form.add_zone(4.0, 4.0);
    assert_eq!(form.setup.baseline.zones, vec![(2.0, 8.0)]);
    assert!(
        form.remove_near(5.0, 0.0),
        "a zone is removed by a point inside it"
    );
    // Fit range: only one.
    let mut form = AnalysisForm::new(&Preset::Fit(Model::Linear), series(&f), false);
    form.add_zone(1.0, 3.0);
    form.add_zone(5.0, 9.0);
    assert_eq!(form.setup.fit.range, Some((5.0, 9.0)));
    // Peaks: sorted positions.
    let mut form = AnalysisForm::new(&Preset::Peaks, series(&f), false);
    form.add_point(7.0, 1.0);
    form.add_point(3.0, 1.0);
    let centers: Vec<f64> = form.setup.peaks.seeds.iter().map(|s| s.center).collect();
    assert_eq!(centers, vec![3.0, 7.0]);
}

#[test]
fn the_natural_tool_follows_the_family() {
    use stokesforge_app_state::analysis::PreviewTool;
    let f = fixture();
    let tool = |preset: Preset| AnalysisForm::new(&preset, series(&f), false).tool;
    assert_eq!(tool(Preset::BaselineAnchors), PreviewTool::Points);
    assert_eq!(tool(Preset::BaselinePolynomial), PreviewTool::Zone);
    assert_eq!(tool(Preset::Integrate), PreviewTool::Zone);
    assert_eq!(tool(Preset::Peaks), PreviewTool::Points);
}

/// #270, #299: only peak detection offers the choice between a click (the
/// position of a peak) and a drag (its range); the other analyses have no
/// gesture to choose, theirs is `natural_tool`.
#[test]
fn only_peaks_offer_a_choice_of_gesture() {
    use stokesforge_app_state::analysis::PreviewTool;
    let f = fixture();
    let choices = |preset: Preset| AnalysisForm::new(&preset, series(&f), false).tool_choices();
    assert_eq!(
        choices(Preset::Peaks),
        &[PreviewTool::Points, PreviewTool::Zone]
    );
    for preset in [
        Preset::BaselineAnchors,
        Preset::BaselinePolynomial,
        Preset::BaselineConstant,
        Preset::Integrate,
        Preset::Fit(Model::Linear),
    ] {
        assert!(choices(preset.clone()).is_empty(), "{preset:?}");
    }
    let mut form = AnalysisForm::new(&Preset::Peaks, series(&f), false);
    assert!(form.tool_choices().contains(&form.natural_tool()));
    form.set_kind(stokesforge_analysis::ops::OperationKind::Integrate);
    assert!(
        form.tool_choices().is_empty(),
        "the choice follows the family"
    );
}

#[test]
fn reference_candidates_exclude_the_processed_series_and_pair_their_x() {
    use stokesforge_app_state::analysis::{aux_candidates, aux_for, series_points};
    let mut f = fixture();
    let sheet = f.project.tree().node(f.y).unwrap().parent().unwrap();
    let other = f
        .project
        .add_column(sheet, None, Some("C"), Column::from_f64(&[1.0, 1.0, 1.0]))
        .unwrap();
    f.project
        .set_column_meta(
            f.x,
            ColumnMeta {
                role: stokesforge_core::ColumnRole::X,
                ..ColumnMeta::default()
            },
        )
        .unwrap();
    let tree = f.project.tree();
    let candidates = aux_candidates(tree, &series(&f));
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].0, other);
    assert!(candidates[0].1.contains('C'), "{}", candidates[0].1);
    assert_eq!(aux_for(tree, other).x, Some(f.x));
    let (xs, ys) = series_points(tree, &series(&f)[0]);
    assert_eq!((xs.len(), ys.len()), (21, 21));
}

#[test]
fn value_labels_name_known_keys_and_keep_unknown_ones() {
    use stokesforge_app_state::analysis::value_label;
    i18n::set_language(Language::English);
    assert_eq!(value_label("param.tau", None), "tau");
    assert_eq!(value_label("stat.r_squared", None), "R²");
    assert_eq!(value_label("peak.center", Some(2)), "Peak 2: center");
    assert_eq!(value_label("something.new", None), "something.new");
}

#[test]
fn editing_a_weighted_fit_keeps_its_weighting() {
    let mut f = fixture();
    let sheet = f.project.tree().parent(f.y).unwrap().unwrap();
    let sigma = f
        .project
        .add_column(sheet, None, Some("C"), Column::from_f64(&[0.1; 21]))
        .unwrap();
    let with_sigma = vec![SeriesRef {
        x: Some(f.x),
        y: f.y,
        sigma: Some(sigma),
    }];
    let mut form = AnalysisForm::new(&Preset::Fit(Model::Linear), with_sigma, false);
    form.setup.fit.weighted = true;
    let id = f
        .project
        .add_calculation(form.draft(f.project.tree()).unwrap())
        .unwrap();
    let calc = f.project.tree().calculation(id).unwrap().clone();
    let again = AnalysisForm::from_calculation(id, &calc);
    assert!(again.setup.fit.weighted, "weighting survives Edit");
    assert_eq!(again.effective_series(), form.effective_series());
}

#[test]
fn changing_the_components_keeps_the_shift_settings_at_the_end() {
    let f = fixture();
    let mut form = AnalysisForm::new(&Preset::Reconvolution, series(&f), false);
    let last = form.setup.reconv.spec.params.len() - 1;
    form.setup.reconv.spec.params[last].fixed = true;
    form.setup.reconv.spec.params[last].initial = Some(0.25);
    let marked = form.setup.reconv.spec.params[last].clone();
    for components in [2, 3, 1] {
        form.setup.reconv.spec.components = components;
        form.setup.reconv.resize_params();
        let params = &form.setup.reconv.spec.params;
        assert_eq!(params.len(), 2 + 2 * components);
        assert_eq!(params.last().unwrap(), &marked, "{components} components");
        assert_ne!(params[params.len() - 2], marked);
    }
}

#[test]
fn the_preview_is_not_recomputed_for_the_tool_or_the_average() {
    use stokesforge_app_state::analysis::PreviewTool;
    let f = fixture();
    let form = AnalysisForm::new(&Preset::BaselineLine, series(&f), false);
    let mut other = form.clone();
    other.tool = PreviewTool::Zone;
    other.average = form.average + 3;
    other.keep_results = !form.keep_results;
    assert!(
        form.same_preview(&other),
        "tool and average do not change the preview"
    );
    other.setup.baseline.points.push((1.0, 1.0));
    assert!(!form.same_preview(&other), "a placed point changes it");
}

#[test]
fn the_three_exponentials_are_three_distinct_fit_models() {
    let models = Preset::fit_models();
    for wanted in [
        Model::Exponential,
        Model::ExponentialOffset,
        Model::Exponential2,
    ] {
        assert!(
            models.contains(&wanted),
            "{wanted:?} missing from the Fit menu"
        );
    }
    // Each variant has its own number of parameters: choosing one in the
    // window does not run another one (#204).
    let counts: Vec<usize> = [
        Model::Exponential,
        Model::ExponentialOffset,
        Model::Exponential2,
    ]
    .iter()
    .map(|m| {
        AnalysisForm::new(&Preset::Fit(m.clone()), Vec::new(), false)
            .setup
            .fit
            .param_count()
    })
    .collect();
    assert_eq!(counts, [2, 3, 5]);
}

#[test]
fn the_center_window_of_the_peaks_reaches_the_operation_and_comes_back() {
    let f = fixture();
    let mut form = AnalysisForm::new(&Preset::Peaks, series(&f), false);
    form.setup.peaks.center_window = Some(0.75);
    let Some(Operation::Peaks { spec, .. }) = form.operation() else {
        panic!("a peaks operation is expected");
    };
    assert_eq!(spec.center_window, Some(0.75));
    let op = form.operation().unwrap();
    let mut reopened = AnalysisForm::new(&preset_of(&op), series(&f), false);
    reopened.setup.load(&op);
    assert_eq!(reopened.setup.peaks.center_window, Some(0.75));
    // Witness: without a window, the positions stay free.
    form.setup.peaks.center_window = None;
    let Some(Operation::Peaks { spec, .. }) = form.operation() else {
        panic!("a peaks operation is expected");
    };
    assert_eq!(spec.center_window, None);
}

#[test]
fn the_area_unit_is_the_product_of_the_units_and_only_when_both_exist() {
    let mut f = fixture();
    let x_unit = |project: &mut Project, x: NodeId, unit: &str| {
        let meta = ColumnMeta {
            unit: unit.to_string(),
            ..ColumnMeta::default()
        };
        project.set_column_meta(x, meta).unwrap();
    };
    let s = series(&f).remove(0);
    assert_eq!(
        value_unit(f.project.tree(), &s, "area"),
        "",
        "X without a unit"
    );
    x_unit(&mut f.project, f.x, "nm");
    assert_eq!(value_unit(f.project.tree(), &s, "area"), "nm·counts");
    assert_eq!(value_unit(f.project.tree(), &s, "area.total"), "nm·counts");
    assert_eq!(value_unit(f.project.tree(), &s, "peak.area"), "nm·counts");
    assert_eq!(
        value_unit(f.project.tree(), &s, "param.tau"),
        "",
        "not an area"
    );
    assert_eq!(with_unit("12".into(), "nm·counts"), "12 nm·counts");
    assert_eq!(with_unit("12".into(), ""), "12");
}

#[test]
fn auto_fills_the_starting_values_of_a_free_model_from_the_data() {
    let f = fixture();
    let mut form = AnalysisForm::new(
        &Preset::Fit(Model::Custom {
            formula: "a*x + b".into(),
        }),
        series(&f),
        false,
    );
    let xs: Vec<f64> = (0..50).map(f64::from).collect();
    let ys: Vec<f64> = xs.iter().map(|x| 300.0 * x + 40.0).collect();
    assert!(form.setup.fit.params.iter().all(|p| p.initial.is_none()));
    form.setup.fit.auto_initial(&xs, &ys);
    let initial: Vec<f64> = form
        .setup
        .fit
        .params
        .iter()
        .map(|p| p.initial.expect("a starting value"))
        .collect();
    assert_eq!(initial.len(), 2);
    assert!(initial.iter().all(|v| v.is_finite()));
    assert!(
        initial.iter().any(|v| v.abs() > 10.0),
        "the start follows the data scale, not 'all at 1': {initial:?}"
    );
    // A fixed parameter keeps its value.
    form.setup.fit.params[0].fixed = true;
    form.setup.fit.params[0].initial = Some(7.0);
    form.setup.fit.auto_initial(&xs, &ys);
    assert_eq!(form.setup.fit.params[0].initial, Some(7.0));
}

#[test]
fn every_builtin_model_formula_compiles_in_typst() {
    use stokesforge_render::formula;
    let fonts = formula::resolve_fonts(&stokesforge_render::FontChoice::default()).expect("fonts");
    for model in Preset::fit_models() {
        match model_formula_source(&model) {
            Some(source) => {
                let (sans, math) = fonts.clone();
                formula::compile(source, 14.0, [0, 0, 0, 255], sans, math)
                    .unwrap_or_else(|error| panic!("{model:?}: {source:?}: {error}"));
            }
            None => assert!(
                matches!(model, Model::Custom { .. }),
                "only the free model has no fixed formula: {model:?}"
            ),
        }
    }
}

#[test]
fn handles_expose_points_edges_and_seeds_and_move_them() {
    let f = fixture();
    // Anchors: X and Y move, the points stay sorted.
    let mut form = AnalysisForm::new(&Preset::BaselineAnchors, series(&f), false);
    form.add_point(1.0, 2.0);
    form.add_point(5.0, 1.0);
    let handles = form.handles();
    assert_eq!(handles.len(), 2);
    assert_eq!(handles[1].y, Some(1.0));
    assert!(form.move_handle(HandleSlot::Point(0), 7.0, 3.0));
    assert_eq!(form.setup.baseline.points, vec![(5.0, 1.0), (7.0, 3.0)]);
    assert!(!form.move_handle(HandleSlot::Point(9), 1.0, 1.0), "gone");
    assert!(!form.move_handle(HandleSlot::Point(0), f64::NAN, 1.0));

    // Zones: two bounds per zone, never crossed nor reduced to a point.
    let mut form = AnalysisForm::new(&Preset::BaselinePolynomial, series(&f), false);
    form.add_zone(2.0, 8.0);
    let edge = |e| HandleSlot::Edge(RangeSet::BaselineZones, 0, e);
    assert_eq!(form.handles().len(), 2);
    assert!(form.move_handle(edge(Edge::Lo), 4.0, 0.0));
    assert_eq!(form.setup.baseline.zones, vec![(4.0, 8.0)]);
    assert!(form.move_handle(edge(Edge::Lo), 10.0, 0.0));
    assert_eq!(
        form.setup.baseline.zones,
        vec![(8.0, 10.0)],
        "order restored"
    );
    assert!(
        !form.move_handle(edge(Edge::Hi), 8.0, 0.0),
        "zero-width range"
    );

    // Integral, fit, peaks: ranges and positions.
    let mut form = AnalysisForm::new(&Preset::Integrate, series(&f), false);
    form.add_zone(1.0, 3.0);
    assert!(form.move_handle(HandleSlot::Edge(RangeSet::Integrate, 0, Edge::Hi), 6.0, 0.0));
    assert_eq!(form.setup.integrate.ranges, vec![(1.0, 6.0)]);
    let mut form = AnalysisForm::new(&Preset::Fit(Model::Linear), series(&f), false);
    assert!(form.handles().is_empty(), "no range, no handle");
    form.add_zone(1.0, 3.0);
    assert!(form.move_handle(HandleSlot::Edge(RangeSet::Fit, 0, Edge::Lo), 0.5, 0.0));
    assert_eq!(form.setup.fit.range, Some((0.5, 3.0)));
    let mut form = AnalysisForm::new(&Preset::Peaks, series(&f), false);
    form.add_point(3.0, 1.0);
    form.add_point(7.0, 1.0);
    assert!(form.move_handle(HandleSlot::Seed(0), 9.0, 0.0));
    let centers: Vec<f64> = form.setup.peaks.seeds.iter().map(|s| s.center).collect();
    assert_eq!(centers, vec![7.0, 9.0]);
}

#[test]
fn constant_single_point_or_zone_and_exact_reopening() {
    let f = fixture();
    let mut form = AnalysisForm::new(&Preset::BaselineConstant, series(&f), false);
    assert!(form.operation_for(false).is_none());
    form.add_point(3.0, 7.0);
    form.add_point(8.0, 17.0);
    assert_eq!(form.setup.baseline.constant_point, Some((8.0, 17.0)));
    assert_eq!(form.handles().len(), 1);
    assert!(form.move_handle(HandleSlot::Point(0), 10.0, 3.0));
    let operation = form.operation_for(false).unwrap();
    assert!(matches!(
        operation,
        Operation::Baseline {
            method: BaselineMethod::Constant { point: (10.0, 3.0) },
            ..
        }
    ));
    let mut restored = AnalysisForm::new(&preset_of(&operation), series(&f), false);
    restored.setup.load(&operation);
    assert_eq!(restored.operation_for(false), Some(operation));
    form.setup.baseline.shape = BaselineShape::ConstantZone;
    assert!(form.operation_for(false).is_none());
    form.add_zone(1.0, 5.0);
    form.add_zone(9.0, 3.0);
    assert_eq!(form.setup.baseline.constant_zone, Some((3.0, 9.0)));
    assert_eq!(form.handles().len(), 2);
    let operation = form.operation_for(false).unwrap();
    restored.setup.load(&operation);
    assert_eq!(restored.operation_for(false), Some(operation));
    assert!(form.remove_near(4.0, 0.1));
    assert!(form.operation_for(false).is_none());
}

#[test]
fn changing_the_constant_shape_keeps_the_anchors_and_the_zones() {
    let f = fixture();
    let mut form = AnalysisForm::new(&Preset::BaselineAnchors, series(&f), false);
    for (x, y) in [(1.0, 3.0), (2.0, 5.0), (3.0, 7.0)] {
        form.add_point(x, y);
    }
    let anchors = form.operation_for(false).unwrap();
    form.setup.baseline.shape = BaselineShape::Constant;
    assert!(
        form.operation_for(false).is_none(),
        "the constant has its own point"
    );
    form.add_point(4.0, 9.0);
    assert!(form.move_handle(HandleSlot::Point(0), 5.0, 11.0));
    let constant = form.operation_for(false).unwrap();
    form.setup.baseline.shape = BaselineShape::Anchors;
    assert_eq!(form.operation_for(false), Some(anchors));
    form.setup.baseline.shape = BaselineShape::Zones;
    form.add_zone(1.0, 3.0);
    form.add_zone(7.0, 9.0);
    let zones = form.operation_for(false).unwrap();
    form.setup.baseline.shape = BaselineShape::ConstantZone;
    assert!(
        form.operation_for(false).is_none(),
        "the constant has its own zone"
    );
    form.add_zone(4.0, 6.0);
    assert!(form.move_handle(
        HandleSlot::Edge(RangeSet::BaselineZones, 0, Edge::Hi),
        8.0,
        0.0
    ));
    let constant_zone = form.operation_for(false).unwrap();
    form.setup.baseline.shape = BaselineShape::Zones;
    assert_eq!(form.operation_for(false), Some(zones.clone()));
    form.setup.baseline.shape = BaselineShape::Constant;
    assert_eq!(form.operation_for(false), Some(constant));
    assert!(form.remove_near(5.0, 0.01));
    form.setup.baseline.shape = BaselineShape::ConstantZone;
    assert_eq!(form.operation_for(false), Some(constant_zone));
    assert!(form.remove_near(5.0, 0.01));
    form.setup.baseline.shape = BaselineShape::Zones;
    assert_eq!(form.operation_for(false), Some(zones));
    // The line still uses the first two kept anchors.
    form.setup.baseline.shape = BaselineShape::TwoPoints;
    assert!(matches!(
        form.operation_for(false),
        Some(Operation::Baseline {
            method: BaselineMethod::Line {
                p1: (1.0, 3.0),
                p2: (2.0, 5.0)
            },
            ..
        })
    ));
}

/// #324: smoothing is in the "Data Manipulation" category, its window in
/// points is always odd, its draft describes the operation of the settings
/// and reopens on the same method.
#[test]
fn a_smoothing_form_keeps_an_odd_window_and_round_trips() {
    use stokesforge_analysis::smooth::{SmoothMethod, SmoothWindow};
    use stokesforge_app_state::analysis::{Category, SmoothUnit};
    let mut f = fixture();
    assert_eq!(
        Category::of(OperationKind::Smooth),
        Category::DataManipulation
    );
    assert!(Category::DataManipulation
        .kinds()
        .contains(&OperationKind::Smooth));

    let mut form = AnalysisForm::new(&Preset::SmoothSavitzkyGolay, series(&f), true);
    assert_eq!(form.kind, OperationKind::Smooth);
    form.setup.smooth.set_points(10);
    assert_eq!(form.setup.smooth.points, 11, "an even window is raised");
    form.setup.smooth.set_points(2);
    assert_eq!(form.setup.smooth.points, 3, "clamped to the minimum");
    form.setup.smooth.set_points(10_000);
    assert_eq!(form.setup.smooth.points, 1001, "clamped to the cap");
    form.setup.smooth.set_points(9);

    let op = form.operation().unwrap();
    let Operation::Smooth { spec } = &op else {
        panic!("{op:?}");
    };
    assert_eq!(spec.method, SmoothMethod::SavitzkyGolay { order: 2 });
    assert_eq!(spec.window, SmoothWindow::Points(9));
    assert_eq!(preset_of(&op), Preset::SmoothSavitzkyGolay);

    // In X units, the window is the one of the width field.
    form.setup.smooth.unit = SmoothUnit::XUnits;
    form.setup.smooth.x_width = 4.0;
    let Some(Operation::Smooth { spec }) = form.operation() else {
        panic!("lissage attendu");
    };
    assert_eq!(spec.window, SmoothWindow::XUnits(4.0));
    form.setup.smooth.unit = SmoothUnit::Points;

    // The batch writes the smoothed column, named after its source, with the
    // unit of the latter; a line smoothed by SG stays the line.
    let draft = form.draft(f.project.tree()).unwrap();
    assert_eq!(draft.outputs.len(), 1);
    assert_eq!(draft.outputs[0].key, "smoothed");
    assert_eq!(draft.outputs[0].meta.long_name, "Intensity (smoothed)");
    assert_eq!(draft.outputs[0].meta.unit, "counts");
    assert_eq!(draft.outputs[0].meta.comment, "Smoothing");
    let ys: Vec<f64> = (0..21).map(|x| 2.0 * f64::from(x) + 1.0).collect();
    for (row, y) in ys.iter().enumerate().take(17).skip(4) {
        assert!((draft.outputs[0].data[row] - y).abs() < 1e-9, "row {row}");
    }

    let id = f.project.add_calculation(draft).unwrap();
    let calc = f.project.tree().calculation(id).unwrap().clone();
    let again = AnalysisForm::from_calculation(id, &calc);
    assert_eq!(again.operation(), Some(op));
    assert_eq!(again.setup.smooth.points, 9);

    // Smoothing can replace the data, like a correction.
    let fresh = AnalysisForm::new(&Preset::SmoothMedian, series(&f), false);
    assert_eq!(fresh.replacement_disabled(f.project.tree()), None);
}
