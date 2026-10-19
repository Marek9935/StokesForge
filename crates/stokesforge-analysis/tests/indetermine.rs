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
    // Bien ajusté : une case sans effet n'est pas un plateau (#281).
    assert!(!o.warnings.contains(&Warning::NotConverged));
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

/// #281 : un départ hors des données ne « converge » pas en silence.
fn gaussian_input(noise: f64) -> SeriesInput {
    let x: Vec<f64> = (0..200).map(|i| 20.0 * f64::from(i) / 199.0).collect();
    let y = x
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let z = (v - 8.0) / (3.0 / 2.354_820_045);
            5.0 * (-0.5 * z * z).exp()
                + noise * (((i as f64 * 0.618_033_988_7).fract() - 0.5) * 2.0)
        })
        .collect();
    SeriesInput { x, y, sigma: None }
}

fn gaussian_fit(initial: &[(usize, f64)]) -> Operation {
    use stokesforge_analysis::fit::ParamSetting;
    let mut params = vec![ParamSetting::default(); 4];
    for &(j, v) in initial {
        params[j].initial = Some(v);
    }
    Operation::Fit {
        spec: FitSpec {
            model: Model::Gaussian,
            params,
        },
        range: None,
        residuals: false,
    }
}

#[test]
fn un_depart_hors_des_donnees_avertit_non_converge() {
    // xc = 1000, fwhm = 1 : le pic est hors de la plage, R² ≈ 0.
    let op = gaussian_fit(&[(2, 1000.0), (3, 1.0)]);
    let o = run_one(&op, gaussian_input(0.0)).unwrap();
    assert!(
        o.warnings.contains(&Warning::NotConverged),
        "{:?}",
        o.warnings
    );
}

#[test]
fn un_ajustement_sain_ou_bruite_ne_previent_pas() {
    for noise in [0.0, 0.3] {
        let o = run_one(&gaussian_fit(&[]), gaussian_input(noise)).unwrap();
        assert!(o.warnings.is_empty(), "bruit {noise} : {:?}", o.warnings);
    }
}

#[test]
fn un_modele_libre_converge_sur_un_x_decale() {
    // X = 1e6 + t : la colonne « 1 » et la colonne « x » du jacobien sont
    // quasi colinéaires (cosinus 1 − 1e-13) ; le modèle linéaire (moindres
    // carrés directs) réussit, le modèle libre doit réussir aussi.
    let t: Vec<f64> = (0..60).map(|i| f64::from(i) / 59.0).collect();
    let x: Vec<f64> = t.iter().map(|v| 1e6 + v).collect();
    let y: Vec<f64> = t.iter().map(|v| 2.0 + 3.0 * v).collect();
    let op = fit_op(Model::Custom {
        formula: "a + b*x".into(),
    });
    let o = run_one(&op, SeriesInput { x, y, sigma: None }).unwrap();
    let b = o.values.iter().find(|v| v.key == "param.b").unwrap().value;
    assert!((b - 3.0).abs() < 1e-3, "b = {b}");
    assert!(!o.warnings.contains(&Warning::NotConverged));
}
