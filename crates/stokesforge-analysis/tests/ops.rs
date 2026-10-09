//! Contrat de `ops::run` : alignement des sorties sur les lignes de l'entrée,
//! lignes écartées comptées, un lot traité série par série, ajustement global.

use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp, Interpolation};
use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::deconv::{DirectMethod, ReconvSpec};
use stokesforge_analysis::fit::{FitSpec, ParamSetting};
use stokesforge_analysis::integrate::{IntegrateSpec, Under};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{
    run, AutoDetect, AuxInput, Operation, OutColumn, Outcome, SeriesInput, Value,
};
use stokesforge_analysis::peaks::{PeakBackground, PeakShape, PeaksSpec};
use stokesforge_analysis::AnalysisError;

fn col<'a>(o: &'a Outcome, key: &str) -> &'a OutColumn {
    o.columns.iter().find(|c| c.key == key).unwrap_or_else(|| {
        panic!(
            "colonne '{key}' absente : {:?}",
            o.columns.iter().map(|c| c.key).collect::<Vec<_>>()
        )
    })
}

fn val<'a>(o: &'a Outcome, key: &str) -> &'a Value {
    o.values
        .iter()
        .find(|v| v.key == key)
        .unwrap_or_else(|| panic!("valeur '{key}' absente"))
}

fn line(n: usize, f: impl Fn(f64) -> f64) -> SeriesInput {
    let x: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let y = x.iter().map(|&v| f(v)).collect();
    SeriesInput { x, y, sigma: None }
}

#[test]
fn la_ligne_de_base_garde_la_longueur_de_la_source_et_compte_les_lignes_ecartees() {
    let mut inp = line(11, |x| 2.0 + 0.5 * x);
    inp.y[3] = f64::NAN; // cellule vide
    inp.x[7] = f64::NAN; // X manquant
    let op = Operation::Baseline {
        method: BaselineMethod::Line {
            p1: (0.0, 2.0),
            p2: (10.0, 7.0),
        },
        op: BaselineOp::Subtract,
        keep_baseline: true,
    };
    let out = run(&op, &[inp], None).unwrap();
    let o = &out[0];
    assert_eq!(o.skipped, 2);
    let corrected = &col(o, "corrected").data;
    let base = &col(o, "baseline").data;
    assert_eq!(corrected.len(), 11);
    assert!(corrected[3].is_nan() && corrected[7].is_nan());
    for i in [0usize, 1, 2, 4, 5, 6, 8, 9, 10] {
        assert!(corrected[i].abs() < 1e-12, "ligne {i}: {}", corrected[i]);
    }
    // La ligne de base se lit même là où Y est vide, tant que X est connu.
    assert!((base[3] - 3.5).abs() < 1e-12);
    assert!(base[7].is_nan());
    assert!((val(o, "baseline.slope").value - 0.5).abs() < 1e-12);
    // Sans `keep_baseline`, pas de colonne de ligne de base.
    let op2 = Operation::Baseline {
        method: BaselineMethod::Line {
            p1: (0.0, 2.0),
            p2: (10.0, 7.0),
        },
        op: BaselineOp::Divide,
        keep_baseline: false,
    };
    let out2 = run(&op2, &[line(5, |x| 2.0 + 0.5 * x)], None).unwrap();
    assert!(out2[0].columns.iter().all(|c| c.key != "baseline"));
    assert!(col(&out2[0], "corrected")
        .data
        .iter()
        .all(|v| (v - 1.0).abs() < 1e-12));
}

#[test]
fn ligne_de_base_par_ancrages_spline() {
    let inp = line(21, |x| 0.01 * x * x);
    let op = Operation::Baseline {
        method: BaselineMethod::Anchors {
            points: (0..=4)
                .map(|i| (5.0 * i as f64, 0.01 * (5.0 * i as f64).powi(2)))
                .collect(),
            interpolation: Interpolation::Spline,
        },
        op: BaselineOp::Subtract,
        keep_baseline: false,
    };
    let o = &run(&op, &[inp], None).unwrap()[0];
    let max = col(o, "corrected")
        .data
        .iter()
        .fold(0.0_f64, |a, b| a.max(b.abs()));
    assert!(max < 0.05, "écart max {max}");
}

#[test]
fn un_lot_est_traite_serie_par_serie() {
    let a = line(10, |x| 3.0 * x);
    let b = line(10, |x| 4.0 - x);
    let op = Operation::Fit {
        spec: FitSpec {
            model: Model::Linear,
            params: vec![],
        },
        range: None,
        residuals: true,
    };
    let out = run(&op, &[a, b], None).unwrap();
    assert_eq!(out.len(), 2);
    assert!((val(&out[0], "param.slope").value - 3.0).abs() < 1e-10);
    assert!((val(&out[1], "param.slope").value + 1.0).abs() < 1e-10);
    assert!((val(&out[1], "param.intercept").value - 4.0).abs() < 1e-10);
    assert!(col(&out[0], "residuals")
        .data
        .iter()
        .all(|r| r.abs() < 1e-9));
}

#[test]
fn la_plage_de_l_ajustement_limite_la_courbe_ajustee() {
    let inp = line(21, |x| 2.0 * x);
    let op = Operation::Fit {
        spec: FitSpec {
            model: Model::Linear,
            params: vec![],
        },
        range: Some((5.0, 10.0)),
        residuals: false,
    };
    let o = &run(&op, &[inp], None).unwrap()[0];
    let fit = &col(o, "fit").data;
    assert!(fit[4].is_nan() && fit[11].is_nan());
    assert!((fit[5] - 10.0).abs() < 1e-9 && (fit[10] - 20.0).abs() < 1e-9);
    assert_eq!(val(o, "stat.points").value, 6.0);
}

#[test]
fn une_plage_vide_est_une_erreur() {
    let op = Operation::Fit {
        spec: FitSpec {
            model: Model::Linear,
            params: vec![],
        },
        range: Some((100.0, 200.0)),
        residuals: false,
    };
    assert_eq!(
        run(&op, &[line(10, |x| x)], None).unwrap_err(),
        AnalysisError::EmptyRange
    );
}

#[test]
fn l_ajustement_global_partage_un_parametre_entre_les_series() {
    let mk = |a: f64| {
        let x: Vec<f64> = (0..80).map(|i| i as f64 * 0.1).collect();
        let y = x.iter().map(|v| a * (-v / 2.0).exp()).collect();
        SeriesInput { x, y, sigma: None }
    };
    let op = Operation::Fit {
        spec: FitSpec {
            model: Model::Exponential,
            params: vec![
                ParamSetting::default(),
                ParamSetting {
                    shared: true,
                    ..Default::default()
                },
            ],
        },
        range: None,
        residuals: false,
    };
    let out = run(&op, &[mk(1.0), mk(3.0)], None).unwrap();
    assert_eq!(
        val(&out[0], "param.tau").value,
        val(&out[1], "param.tau").value
    );
    assert!((val(&out[1], "param.A").value - 3.0).abs() < 1e-5);
}

#[test]
fn la_correction_exige_une_reference_et_compte_les_points_hors_domaine() {
    let inp = line(6, |_| 10.0);
    let op = Operation::Correct {
        op: CorrectOp::Divide,
    };
    assert_eq!(
        run(&op, std::slice::from_ref(&inp), None).unwrap_err(),
        AnalysisError::EmptyReference
    );
    let aux = AuxInput {
        x: vec![1.0, 4.0],
        y: vec![2.0, 2.0],
    };
    let o = &run(&op, &[inp], Some(&aux)).unwrap()[0];
    let c = &col(o, "corrected").data;
    assert!(c[0].is_nan() && c[5].is_nan());
    assert_eq!(c[2], 5.0);
    assert_eq!(val(o, "count.out_of_reference").value, 2.0);
}

#[test]
fn l_integrale_donne_aires_total_et_colonnes() {
    let inp = line(11, |_| 2.0);
    let op = Operation::Integrate {
        spec: IntegrateSpec {
            ranges: vec![(0.0, 4.0), (6.0, 10.0)],
            under: Under::Zero,
            cumulative: true,
            normalize: true,
        },
    };
    let o = &run(&op, &[inp], None).unwrap()[0];
    assert_eq!(val(o, "area").value, 8.0);
    assert_eq!(o.values.iter().filter(|v| v.key == "area").count(), 2);
    assert_eq!(val(o, "area.total").value, 16.0);
    assert_eq!(col(o, "cumulative").data[10], 20.0);
    assert!((col(o, "normalized").data[0] - 0.125).abs() < 1e-12);
}

#[test]
fn les_pics_detectes_automatiquement_donnent_une_colonne_par_composante() {
    let x: Vec<f64> = (0..400).map(|i| i as f64 * 0.1).collect();
    let g = |v: f64, h: f64, c: f64, w: f64| {
        h * (-4.0 * std::f64::consts::LN_2 * ((v - c) / w).powi(2)).exp()
    };
    let y = x
        .iter()
        .map(|&v| 1.0 + g(v, 10.0, 12.0, 3.0) + g(v, 6.0, 28.0, 4.0))
        .collect();
    let inp = SeriesInput { x, y, sigma: None };
    let op = Operation::Peaks {
        spec: PeaksSpec {
            shape: PeakShape::Gaussian,
            background: PeakBackground::Constant,
            seeds: vec![],
            link_widths: false,
            center_window: None,
        },
        auto: Some(AutoDetect {
            min_prominence: 0.1,
            max_peaks: 5,
        }),
        range: None,
        residuals: true,
    };
    let o = &run(&op, &[inp], None).unwrap()[0];
    let peaks: Vec<&OutColumn> = o.columns.iter().filter(|c| c.key == "peak").collect();
    assert_eq!(peaks.len(), 2);
    assert_eq!(peaks[0].index, Some(1));
    assert_eq!(val(o, "peak.height").index, Some(1));
    assert!((val(o, "background.y0").value - 1.0).abs() < 1e-4);
    // La somme (fond compris) colle à la courbe : résidus ≈ 0.
    assert!(col(o, "residuals").data.iter().all(|r| r.abs() < 1e-4));
    assert!(col(o, "background")
        .data
        .iter()
        .all(|b| (b - 1.0).abs() < 1e-4));
}

#[test]
fn des_pics_sans_graine_ni_detection_sont_refuses() {
    let op = Operation::Peaks {
        spec: PeaksSpec {
            shape: PeakShape::Gaussian,
            background: PeakBackground::None,
            seeds: vec![],
            link_widths: false,
            center_window: None,
        },
        auto: None,
        range: None,
        residuals: false,
    };
    assert!(matches!(
        run(&op, &[line(50, |x| x)], None),
        Err(AnalysisError::InvalidParameter { .. })
    ));
}

#[test]
fn reconvolution_et_deconvolution_directe_de_bout_en_bout() {
    let step = 0.05;
    let n = 400;
    let x: Vec<f64> = (0..n).map(|i| i as f64 * step).collect();
    let irf_y: Vec<f64> = x
        .iter()
        .map(|t| (-(t - 1.0_f64).powi(2) / (2.0 * 0.1 * 0.1)).exp())
        .collect();
    let aux = AuxInput {
        x: x.clone(),
        y: irf_y.clone(),
    };
    let sum: f64 = irf_y.iter().sum();
    let h: Vec<f64> = irf_y.iter().map(|v| v / sum).collect();
    // Signal : convolution directe de h avec 50·exp(−t/1.5).
    let u: Vec<f64> = x.iter().map(|t| 50.0 * (-t / 1.5).exp()).collect();
    let y: Vec<f64> = (0..n)
        .map(|k| (0..=k).map(|j| h[j] * u[k - j]).sum::<f64>())
        .collect();
    let mut inp = SeriesInput {
        x: x.clone(),
        y,
        sigma: None,
    };
    inp.y[10] = f64::NAN; // ligne écartée : la sortie reste alignée
    let op = Operation::Reconvolution {
        spec: ReconvSpec {
            components: 1,
            params: vec![],
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        },
        residuals: true,
    };
    let o = &run(&op, &[inp.clone()], Some(&aux)).unwrap()[0];
    assert_eq!(o.skipped, 1);
    assert!((val(o, "param.tau1").value - 1.5).abs() < 1e-3);
    assert!(col(o, "fit").data[10].is_finite() && col(o, "residuals").data[10].is_nan());
    assert_eq!(col(o, "fit").data.len(), n);

    let dir = Operation::DirectDeconvolution {
        method: DirectMethod::RichardsonLucy { iterations: 50 },
        shift: 0.0,
    };
    let mut clean_inp = inp;
    clean_inp.y[10] = 0.5 * (clean_inp.y[9] + clean_inp.y[11]);
    let o = &run(&dir, &[clean_inp], Some(&aux)).unwrap()[0];
    assert_eq!(col(o, "deconvolved").data.len(), n);
    assert!(col(o, "deconvolved").data.iter().all(|v| v.is_finite()));
    // Sans IRF, erreur dédiée.
    assert_eq!(
        run(&dir, &[line(50, |x| x + 1.0)], None).unwrap_err(),
        AnalysisError::EmptyResponse
    );
}

#[test]
fn aucune_entree() {
    let op = Operation::Integrate {
        spec: IntegrateSpec {
            ranges: vec![],
            under: Under::Zero,
            cumulative: false,
            normalize: false,
        },
    };
    assert!(run(&op, &[], None).is_err());
    assert!(!op.needs_aux());
}

#[test]
fn familles_et_recalcul_automatique_par_defaut() {
    use stokesforge_analysis::ops::OperationKind;
    for k in OperationKind::ALL {
        assert_eq!(OperationKind::from_key(k.key()), Some(k));
    }
    assert_eq!(OperationKind::from_key("inconnue"), None);
    let recon = Operation::Reconvolution {
        spec: ReconvSpec {
            components: 1,
            params: vec![],
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        },
        residuals: false,
    };
    let lin = Operation::Fit {
        spec: FitSpec {
            model: Model::Linear,
            params: vec![],
        },
        range: None,
        residuals: false,
    };
    assert!(!recon.auto_recalc_by_default(1));
    assert!(lin.auto_recalc_by_default(20));
    assert!(
        !lin.auto_recalc_by_default(21),
        "un lot de plus de 20 séries est lourd"
    );
    assert_eq!(lin.kind(), OperationKind::Fit);
}
