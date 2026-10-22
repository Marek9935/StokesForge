//! Contrat de `ops::run` : alignement des sorties sur les lignes de l'entrée,
//! lignes écartées comptées, un lot traité série par série, ajustement global.

use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp, Interpolation};
use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::deconv::{DirectMethod, ReconvSpec};
use stokesforge_analysis::fit::{FitSpec, ParamSetting};
use stokesforge_analysis::integrate::{IntegrateSpec, Under};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{
    run, AutoDetect, AuxInput, Operation, OutColumn, Outcome, SeriesInput, Value, Warning,
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
        let y = x.iter().map(|v| a * libm::exp(-v / 2.0)).collect();
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

/// #276 : une plage non finie (pas encore posée) est refusée, au lieu d'intégrer
/// toute la courbe (`min/max` ignorent le NaN) ; `readiness` la dit à poser.
#[test]
fn une_plage_d_integrale_non_finie_est_refusee() {
    use stokesforge_analysis::integrate::integrate;
    use stokesforge_analysis::ops::Readiness;
    use stokesforge_analysis::series::Series;
    for range in [(f64::NAN, f64::NAN), (f64::NAN, 5.0), (5.0, f64::INFINITY)] {
        let spec = IntegrateSpec {
            ranges: vec![(1.0, 2.0), range],
            under: Under::Zero,
            cumulative: false,
            normalize: false,
        };
        let op = Operation::Integrate { spec: spec.clone() };
        let invalid = |r: Result<_, AnalysisError>| matches!(r, Err(AnalysisError::InvalidParameter { ref name }) if name == "ranges");
        assert!(invalid(run(&op, &[line(11, |_| 2.0)], None).map(|_| ())));
        assert!(invalid(op.validate()));
        let x: Vec<f64> = (0..11).map(f64::from).collect();
        assert!(invalid(
            integrate(&Series::new(x.clone(), x), &spec).map(|_| ())
        ));
        assert_eq!(op.readiness(), Readiness::NeedsZone);
    }
}

/// #276 : chaque chemin (`validate`, `readiness`, `run`) voit les bornes non
/// finies seul, sans l'aide des autres ; les plages posées et la liste vide
/// restent acceptées.
#[test]
fn validate_et_readiness_de_l_integrale_se_tiennent_independamment() {
    use stokesforge_analysis::ops::Readiness;
    let (nan, inf) = (f64::NAN, f64::INFINITY);
    let op = |ranges: Vec<(f64, f64)>| Operation::Integrate {
        spec: IntegrateSpec {
            ranges,
            under: Under::Zero,
            cumulative: false,
            normalize: false,
        },
    };
    for r in [
        (nan, nan),
        (nan, 5.0),
        (5.0, nan),
        (inf, 5.0),
        (-inf, 5.0),
        (2.0, inf),
        (2.0, -inf),
    ] {
        let o = op(vec![r]);
        assert_eq!(
            o.validate(),
            Err(AnalysisError::InvalidParameter {
                name: "ranges".into()
            }),
            "{r:?}"
        );
        assert_eq!(o.readiness(), Readiness::NeedsZone, "{r:?}");
        // Mauvaise plage en seconde position : même verdict.
        let o = op(vec![(1.0, 2.0), r]);
        assert!(o.validate().is_err(), "{r:?}");
        assert_eq!(o.readiness(), Readiness::NeedsZone, "{r:?}");
    }
    for ok in [vec![], vec![(1.0, 2.0)], vec![(8.0, 2.0), (0.0, 1.0)]] {
        let o = op(ok.clone());
        assert_eq!(o.validate(), Ok(()), "{ok:?}");
        assert_eq!(o.readiness(), Readiness::Ready, "{ok:?}");
    }
    // Liste vide = toute la courbe, y = x sur [0, 10] : 50.
    let out = run(&op(vec![]), &[line(11, |x| x)], None).unwrap();
    assert_eq!(val(&out[0], "area.total").value, 50.0);
}

#[test]
fn les_pics_detectes_automatiquement_donnent_une_colonne_par_composante() {
    let x: Vec<f64> = (0..400).map(|i| i as f64 * 0.1).collect();
    let g = |v: f64, h: f64, c: f64, w: f64| {
        h * libm::exp(-4.0 * std::f64::consts::LN_2 * ((v - c) / w).powi(2))
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
        .map(|t| libm::exp(-(t - 1.0_f64).powi(2) / (2.0 * 0.1 * 0.1)))
        .collect();
    let aux = AuxInput {
        x: x.clone(),
        y: irf_y.clone(),
    };
    let sum: f64 = irf_y.iter().sum();
    let h: Vec<f64> = irf_y.iter().map(|v| v / sum).collect();
    // Signal : convolution directe de h avec 50·exp(−t/1.5).
    let u: Vec<f64> = x.iter().map(|t| 50.0 * libm::exp(-t / 1.5)).collect();
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

#[test]
fn constante_et_moyenne_de_zone_corrigent_sans_pente_et_gardent_les_lignes() {
    let input = SeriesInput {
        x: vec![0.0, 1.0, 2.0, 3.0, f64::NAN],
        y: vec![9.0, 2.0, f64::NAN, 6.0, 99.0],
        sigma: None,
    };
    for method in [
        BaselineMethod::Constant { point: (-5.0, 4.0) },
        BaselineMethod::ConstantZone { zone: (1.0, 3.0) },
    ] {
        let op = Operation::Baseline {
            method,
            op: BaselineOp::Subtract,
            keep_baseline: true,
        };
        let out = run(&op, std::slice::from_ref(&input), None).unwrap();
        let o = &out[0];
        assert_eq!(o.skipped, 2);
        let result = &col(o, "corrected").data;
        assert_eq!(result.len(), 5);
        assert_eq!(&result[..2], &[5.0, -2.0]);
        assert!(result[2].is_nan() && result[4].is_nan());
        assert_eq!(result[3], 2.0);
        assert_eq!(val(o, "baseline.slope").value, 0.0);
        assert_eq!(val(o, "baseline.intercept").value, 4.0);
    }
    for value in [2.0, 0.0] {
        let op = Operation::Baseline {
            method: BaselineMethod::Constant {
                point: (0.0, value),
            },
            op: BaselineOp::Divide,
            keep_baseline: false,
        };
        let out = run(&op, std::slice::from_ref(&input), None).unwrap();
        if value == 0.0 {
            assert!(col(&out[0], "corrected").data.iter().all(|v| v.is_nan()));
        } else {
            assert_eq!(col(&out[0], "corrected").data[0], 4.5);
        }
    }
}

#[test]
fn deconvolution_signal_non_uniforme_leve_l_avertissement_et_couvre_le_dernier_x() {
    let aux = AuxInput {
        x: (0..40).map(|i| i as f64 * 0.1).collect(),
        y: (0..40)
            .map(|i| libm::exp(-(i as f64 * 0.1 - 1.0).powi(2) / 0.18))
            .collect(),
    };
    let dir = Operation::DirectDeconvolution {
        method: DirectMethod::Wiener { noise: Some(0.01) },
        shift: 0.0,
    };
    let irregulier = SeriesInput {
        x: vec![0.0, 1.0, 2.0, 3.0, 4.4],
        y: vec![1.0, 2.0, 3.0, 2.0, 1.0],
        sigma: None,
    };
    let o = &run(&dir, &[irregulier], Some(&aux)).unwrap()[0];
    assert!(o.warnings.contains(&Warning::SignalStepNotUniform));
    let d = &col(o, "deconvolved").data;
    assert_eq!(d.len(), 5);
    assert!(d.iter().all(|v| v.is_finite()), "{d:?}");
    let regulier = SeriesInput {
        x: vec![0.0, 1.0, 2.0, 3.0, 4.0],
        y: vec![1.0, 2.0, 3.0, 2.0, 1.0],
        sigma: None,
    };
    let o = &run(&dir, &[regulier], Some(&aux)).unwrap()[0];
    assert!(!o.warnings.contains(&Warning::SignalStepNotUniform));
}

// ── C3, C5 (#313) : trous de la référence, divisions par zéro ───────────

fn correction(op: CorrectOp) -> Operation {
    Operation::Correct { op }
}

fn reference(x: &[f64], y: &[f64]) -> AuxInput {
    AuxInput {
        x: x.to_vec(),
        y: y.to_vec(),
    }
}

fn donnees(x: &[f64]) -> SeriesInput {
    SeriesInput {
        x: x.to_vec(),
        y: vec![1.0; x.len()],
        sigma: None,
    }
}

/// Le trou d'une cellule vide au milieu de la référence est enjambé par
/// interpolation : les trois points qui tombent dedans sont comptés.
#[test]
fn un_trou_au_milieu_de_la_reference_est_enjambe_et_dit() {
    let aux = reference(&[1.0, 2.0, 3.0, 4.0], &[1.0, f64::NAN, 3.0, 4.0]);
    let out = run(
        &correction(CorrectOp::Multiply),
        &[donnees(&[1.5, 2.0, 2.5, 3.5])],
        Some(&aux),
    )
    .unwrap();
    let o = &out[0];
    assert_eq!(o.warnings, vec![Warning::BridgedReferenceGap(3)]);
    // Interpolation entre (1, 1) et (3, 3) : y = x.
    assert_eq!(col(o, "corrected").data, vec![1.5, 2.0, 2.5, 3.5]);
}

/// NaN en fin de référence : le domaine se termine avant, rien n'est enjambé.
/// Une ligne de référence sans X n'a pas de position : silence.
#[test]
fn un_trou_en_fin_ou_sans_position_n_est_pas_un_enjambement() {
    let fin = reference(&[1.0, 2.0, 3.0, 4.0], &[1.0, 2.0, 3.0, f64::NAN]);
    let o = &run(
        &correction(CorrectOp::Multiply),
        &[donnees(&[1.5, 2.5, 3.5])],
        Some(&fin),
    )
    .unwrap()[0];
    assert_eq!(o.warnings, vec![]);
    let sans_x = reference(&[1.0, f64::NAN, 3.0], &[1.0, 2.0, 3.0]);
    let o = &run(
        &correction(CorrectOp::Multiply),
        &[donnees(&[1.5, 2.5])],
        Some(&sans_x),
    )
    .unwrap()[0];
    assert_eq!(o.warnings, vec![]);
}

#[test]
fn une_reference_nulle_en_un_point_donne_nan_et_une_division_par_zero() {
    let aux = reference(&[1.0, 2.0, 3.0], &[1.0, 0.0, 1.0]);
    let o = &run(
        &correction(CorrectOp::Divide),
        &[donnees(&[1.0, 2.0, 3.0])],
        Some(&aux),
    )
    .unwrap()[0];
    let c = &col(o, "corrected").data;
    assert!(c[1].is_nan() && c[0] == 1.0 && c[2] == 1.0);
    assert_eq!(o.warnings, vec![Warning::DivisionByZero(1)]);
    // Multiplier par zéro n'est pas une division.
    let o = &run(
        &correction(CorrectOp::Multiply),
        &[donnees(&[1.0, 2.0, 3.0])],
        Some(&aux),
    )
    .unwrap()[0];
    assert_eq!(o.warnings, vec![]);
}

/// Une ligne de base qui passe par zéro : la division est comptée, la
/// soustraction non.
#[test]
fn une_ligne_de_base_passant_par_zero_compte_la_division() {
    let method = BaselineMethod::Line {
        p1: (0.0, 0.0),
        p2: (10.0, 10.0),
    };
    let divide = Operation::Baseline {
        method: method.clone(),
        op: BaselineOp::Divide,
        keep_baseline: false,
    };
    let o = &run(&divide, &[line(11, |x| x + 1.0)], None).unwrap()[0];
    assert!(col(o, "corrected").data[0].is_nan());
    assert_eq!(o.warnings, vec![Warning::DivisionByZero(1)]);
    let subtract = Operation::Baseline {
        method,
        op: BaselineOp::Subtract,
        keep_baseline: false,
    };
    let o = &run(&subtract, &[line(11, |x| x + 1.0)], None).unwrap()[0];
    assert_eq!(o.warnings, vec![]);
}

/// #324 : le lissage garde la longueur de la source (lignes écartées vides et
/// comptées), écrit `smoothed` et la fenêtre employée ; un pas non uniforme
/// n'avertit que pour Savitzky-Golay.
#[test]
fn le_lissage_aligne_sa_sortie_sur_les_lignes_et_avertit_d_un_pas_irregulier() {
    use stokesforge_analysis::smooth::{SmoothMethod, SmoothSpec, SmoothWindow};
    let smooth = |method| Operation::Smooth {
        spec: SmoothSpec {
            method,
            window: SmoothWindow::Points(5),
        },
    };
    let mut inp = line(12, |x| x * x);
    inp.y[3] = f64::NAN;
    let o = &run(&smooth(SmoothMethod::AdjacentAverage), &[inp.clone()], None).unwrap()[0];
    assert_eq!(o.skipped, 1);
    let sortie = &col(o, "smoothed").data;
    assert_eq!(sortie.len(), 12);
    assert!(sortie[3].is_nan(), "la ligne écartée reste vide");
    assert!(sortie[5].is_finite());
    assert_eq!(val(o, "count.window").value, 5.0);
    assert!(o.warnings.is_empty());

    inp.x[6] = 6.4; // pas irrégulier
    let sg = &run(
        &smooth(SmoothMethod::SavitzkyGolay { order: 2 }),
        &[inp.clone()],
        None,
    )
    .unwrap()[0];
    assert!(sg.warnings.contains(&Warning::SignalStepNotUniform));
    let med = &run(&smooth(SmoothMethod::Median), &[inp], None).unwrap()[0];
    assert!(
        med.warnings.is_empty(),
        "la médiane n'a pas besoin d'un pas régulier"
    );

    let hors = Operation::Smooth {
        spec: SmoothSpec {
            method: SmoothMethod::AdjacentAverage,
            window: SmoothWindow::Points(4),
        },
    };
    assert!(matches!(
        run(&hors, &[line(8, |x| x)], None),
        Err(AnalysisError::InvalidParameter { .. })
    ));
}
