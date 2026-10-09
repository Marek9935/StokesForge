//! #203 : un ajustement que les données ne déterminent pas est dit, pas rendu
//! comme un succès ; #205 : une ligne à σ vide ou nul est écartée et comptée.

use stokesforge_analysis::fit::FitSpec;
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{run, Operation, Outcome, SeriesInput, Warning};
use stokesforge_analysis::AnalysisError;

fn fit_op(model: Model) -> Operation {
    Operation::Fit {
        spec: FitSpec {
            model,
            params: vec![],
        },
        range: None,
        residuals: false,
    }
}

fn error_of(o: &Outcome, key: &str) -> Option<f64> {
    o.values.iter().find(|v| v.key == key).unwrap().error
}

fn run_one(op: &Operation, input: SeriesInput) -> Result<Outcome, AnalysisError> {
    run(op, &[input], None).map(|mut v| v.remove(0))
}

#[test]
fn un_modele_dont_un_parametre_est_sans_effet_avertit_sans_taire_les_autres() {
    // y = a + b·x + 0·c : « c » n'influence pas le modèle.
    let x: Vec<f64> = (0..40).map(f64::from).collect();
    let y: Vec<f64> = x
        .iter()
        .map(|v| 1.0 + 0.5 * v + 0.01 * (v * 7.0).sin())
        .collect();
    let op = fit_op(Model::Custom {
        formula: "a + b*x + 0*c".into(),
    });
    let o = run_one(&op, SeriesInput { x, y, sigma: None }).unwrap();
    assert!(
        o.warnings.contains(&Warning::Undetermined(1)),
        "{:?}",
        o.warnings
    );
    assert!(error_of(&o, "param.c").is_none());
    assert!(error_of(&o, "param.a").is_some() && error_of(&o, "param.b").is_some());
}

#[test]
fn des_x_tous_egaux_rendent_la_pente_indeterminee() {
    let x = vec![2.0; 10];
    let y: Vec<f64> = (0..10).map(|i| 3.0 + 0.01 * f64::from(i % 3)).collect();
    let o = run_one(&fit_op(Model::Linear), SeriesInput { x, y, sigma: None }).unwrap();
    assert!(
        o.warnings.contains(&Warning::Undetermined(1)),
        "{:?}",
        o.warnings
    );
}

#[test]
fn aucun_parametre_determine_est_refuse() {
    let x: Vec<f64> = (0..10).map(f64::from).collect();
    let y = vec![1.0; 10];
    let op = fit_op(Model::Custom {
        formula: "0*a + 0*b".into(),
    });
    assert_eq!(
        run_one(&op, SeriesInput { x, y, sigma: None }).unwrap_err(),
        AnalysisError::SingularSystem
    );
}

#[test]
fn un_ajustement_bien_pose_n_avertit_pas() {
    let x: Vec<f64> = (0..30).map(f64::from).collect();
    let y: Vec<f64> = x
        .iter()
        .map(|v| 2.0 + 0.3 * v + 0.01 * (v * 3.0).sin())
        .collect();
    let o = run_one(&fit_op(Model::Linear), SeriesInput { x, y, sigma: None }).unwrap();
    assert!(o.warnings.is_empty());
}

#[test]
fn une_ligne_a_sigma_vide_ou_nul_est_ecartee_et_comptee() {
    let x: Vec<f64> = (0..20).map(f64::from).collect();
    let y: Vec<f64> = x.iter().map(|v| 1.0 + 2.0 * v).collect();
    let mut sigma = vec![0.5; 20];
    sigma[3] = f64::NAN;
    sigma[7] = 0.0;
    let o = run_one(
        &fit_op(Model::Linear),
        SeriesInput {
            x,
            y,
            sigma: Some(sigma),
        },
    )
    .unwrap();
    assert_eq!(o.skipped, 2);
    let points = o
        .values
        .iter()
        .find(|v| v.key == "stat.points")
        .unwrap()
        .value;
    assert_eq!(points, 18.0);
    let slope = o
        .values
        .iter()
        .find(|v| v.key == "param.slope")
        .unwrap()
        .value;
    assert!((slope - 2.0).abs() < 1e-9);
}
