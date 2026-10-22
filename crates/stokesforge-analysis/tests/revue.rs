//! Garde-fous issus de la relecture du chantier #181 : entrées vides sans panique,
//! ajustement indépendant de l'échelle des Y, statistiques par courbe d'un lot,
//! plafonds des réglages.

use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp, Interpolation};
use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::deconv::{DirectMethod, ReconvSpec};
use stokesforge_analysis::fit::{FitSpec, ParamSetting};
use stokesforge_analysis::integrate::{IntegrateSpec, Under};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{run, AuxInput, Operation, Outcome, SeriesInput};
use stokesforge_analysis::peaks::{PeakBackground, PeakSeed, PeakShape, PeaksSpec};
use stokesforge_analysis::AnalysisError;

fn value(o: &Outcome, key: &str) -> f64 {
    o.values
        .iter()
        .find(|v| v.key == key)
        .unwrap_or_else(|| panic!("valeur '{key}' absente"))
        .value
}

fn input(x: Vec<f64>, y: Vec<f64>) -> SeriesInput {
    SeriesInput { x, y, sigma: None }
}

fn every_operation() -> Vec<Operation> {
    vec![
        Operation::Baseline {
            method: BaselineMethod::Anchors {
                points: vec![(0.0, 0.0), (1.0, 1.0)],
                interpolation: Interpolation::Segments,
            },
            op: BaselineOp::Subtract,
            keep_baseline: true,
        },
        Operation::Baseline {
            method: BaselineMethod::Zones {
                zones: vec![(0.0, 1.0)],
                degree: 1,
            },
            op: BaselineOp::Subtract,
            keep_baseline: false,
        },
        Operation::Correct {
            op: CorrectOp::Divide,
        },
        Operation::Integrate {
            spec: IntegrateSpec {
                ranges: vec![(0.0, 1.0)],
                under: Under::Zero,
                cumulative: true,
                normalize: true,
            },
        },
        Operation::Fit {
            spec: FitSpec {
                model: Model::Linear,
                params: vec![],
            },
            range: None,
            residuals: true,
        },
        Operation::Peaks {
            spec: PeaksSpec {
                shape: PeakShape::Gaussian,
                background: PeakBackground::Constant,
                seeds: vec![PeakSeed {
                    center: 1.0,
                    height: Some(1.0),
                    fwhm: None,
                }],
                link_widths: false,
                center_window: None,
            },
            auto: None,
            range: None,
            residuals: true,
        },
        Operation::DirectDeconvolution {
            method: DirectMethod::RichardsonLucy { iterations: 5 },
            shift: 0.0,
        },
        Operation::DirectDeconvolution {
            method: DirectMethod::Wiener { noise: None },
            shift: 0.0,
        },
    ]
}

#[test]
fn une_serie_vide_ou_minuscule_ne_fait_jamais_paniquer() {
    let aux = AuxInput {
        x: vec![0.0, 1.0, 2.0],
        y: vec![1.0, 2.0, 1.0],
    };
    for n in 0..4 {
        let series = input((0..n).map(f64::from).collect(), vec![1.0; n as usize]);
        for op in every_operation() {
            // Ok ou Err, peu importe : l'acteur de session n'a pas de
            // `catch_unwind`, une panique y est fatale.
            let _ = run(&op, std::slice::from_ref(&series), Some(&aux));
        }
        let _ = run(
            &Operation::Reconvolution {
                spec: ReconvSpec {
                    components: 1,
                    params: vec![],
                    shift_initial: 0.0,
                    fit_shift: true,
                    window: None,
                },
                residuals: false,
            },
            std::slice::from_ref(&series),
            Some(&aux),
        );
    }
}

#[test]
fn des_pics_poses_a_la_main_sur_une_colonne_videe_sont_une_erreur() {
    let op = every_operation().remove(5);
    assert!(matches!(
        run(&op, &[input(vec![], vec![])], None),
        Err(AnalysisError::NotEnoughPoints { .. })
    ));
}

#[test]
fn l_ajustement_ne_depend_pas_de_l_echelle_des_y() {
    // y0 + A·exp(−x/τ), τ = 2, ajusté avec les valeurs initiales automatiques.
    for scale in [1e-12, 1e-9, 1e-3, 1.0, 1e6] {
        let x: Vec<f64> = (0..60).map(|i| f64::from(i) * 0.25).collect();
        let y: Vec<f64> = x
            .iter()
            .map(|&v| scale * (0.3 + 2.0 * libm::exp(-v / 2.0)))
            .collect();
        let op = Operation::Fit {
            spec: FitSpec {
                model: Model::ExponentialOffset,
                params: vec![],
            },
            range: None,
            residuals: false,
        };
        let out =
            run(&op, &[input(x, y)], None).unwrap_or_else(|e| panic!("échelle {scale} : {e}"));
        let tau = value(&out[0], "param.tau");
        assert!((tau - 2.0).abs() < 1e-4, "échelle {scale} : τ = {tau}");
    }
}

#[test]
fn un_lot_independant_porte_les_degres_de_liberte_de_chaque_courbe() {
    let curves: Vec<SeriesInput> = (0..3)
        .map(|k| {
            let x: Vec<f64> = (0..20).map(f64::from).collect();
            let y = x
                .iter()
                .enumerate()
                .map(|(i, v)| f64::from(k) + 0.5 * v + if i % 2 == 0 { 0.01 } else { -0.01 })
                .collect();
            input(x, y)
        })
        .collect();
    let op = Operation::Fit {
        spec: FitSpec {
            model: Model::Linear,
            params: vec![ParamSetting::default()],
        },
        range: None,
        residuals: false,
    };
    let out = run(&op, &curves, None).unwrap();
    for o in &out {
        assert_eq!(value(o, "stat.dof"), 18.0, "20 points − 2 paramètres");
    }
}

#[test]
fn les_reglages_hors_plafond_sont_refuses_par_run() {
    let series = input((0..30).map(f64::from).collect(), vec![1.0; 30]);
    let poly = |degree| Operation::Fit {
        spec: FitSpec {
            model: Model::Polynomial { degree },
            params: vec![],
        },
        range: None,
        residuals: false,
    };
    assert!(matches!(
        run(&poly(1_000_000_000), std::slice::from_ref(&series), None),
        Err(AnalysisError::InvalidParameter { .. })
    ));
    let aux = AuxInput {
        x: vec![0.0, 1.0, 2.0],
        y: vec![0.0, 1.0, 0.0],
    };
    for method in [
        DirectMethod::RichardsonLucy { iterations: 10_000 },
        DirectMethod::Wiener { noise: Some(0.0) },
        DirectMethod::Wiener { noise: Some(-1.0) },
    ] {
        let op = Operation::DirectDeconvolution { method, shift: 0.0 };
        assert!(matches!(
            run(&op, std::slice::from_ref(&series), Some(&aux)),
            Err(AnalysisError::InvalidParameter { .. })
        ));
    }
}
