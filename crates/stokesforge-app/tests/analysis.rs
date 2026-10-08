//! Le modèle de la fenêtre d'analyse (#181), sans interface : défauts par
//! opération, ce qui manque pour valider, l'opération décrite, l'aperçu et le
//! lot enregistré (noms longs M6, colonnes demandées, métadonnées conservées en
//! modification).

use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{Operation, OperationKind, HEAVY_BATCH};
use stokesforge_app::analysis::{
    preset_of, AnalysisForm, BaselineShape, Issue, Preset, DEFAULT_AVERAGE,
};
use stokesforge_app::i18n::{self, Language};
use stokesforge_core::{Column, ColumnMeta, NodeId, Project, SeriesRef};

struct Fixture {
    project: Project,
    x: NodeId,
    y: NodeId,
}

/// Une droite `y = 2x + 1` posée sur 21 points, colonne Y « Intensity ».
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
    // Changer de famille ne perd rien de la ligne de base.
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
    // La droite posée est exactement la courbe : la correction est nulle.
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
