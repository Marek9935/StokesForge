//! #281 : `Warning::NotConverged` par les chemins qui passent par le solveur
//! (ajustement, ajustement global, pics, reconvolution), son absence sur un
//! ajustement sain, linéaire ou bruité, et sur la déconvolution directe.

use stokesforge_analysis::deconv::{DirectMethod, ReconvSpec};
use stokesforge_analysis::fit::{FitSpec, ParamSetting};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{run, AuxInput, Operation, Outcome, SeriesInput, Warning};
use stokesforge_analysis::peaks::{PeakBackground, PeakSeed, PeakShape, PeaksSpec};

fn gaussian(x: f64, xc: f64, a: f64, fwhm: f64) -> f64 {
    let z = (x - xc) / (fwhm / 2.354_820_045);
    a * (-0.5 * z * z).exp()
}

/// 200 points sur [0, 20], un pic xc = 8, A = 5, fwhm = 3 (l'exemple de #281).
fn pic(bruit: f64) -> SeriesInput {
    let x: Vec<f64> = (0..200).map(|i| 20.0 * f64::from(i) / 199.0).collect();
    let y = x
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            gaussian(v, 8.0, 5.0, 3.0)
                + bruit * (((i as f64 * 0.618_033_988_7).fract() - 0.5) * 2.0)
        })
        .collect();
    SeriesInput { x, y, sigma: None }
}

fn run_one(op: &Operation, input: SeriesInput, aux: Option<&AuxInput>) -> Outcome {
    run(op, &[input], aux).unwrap().remove(0)
}

fn fit(model: Model, initial: &[(usize, f64)], n: usize) -> Operation {
    let mut params = vec![ParamSetting::default(); n];
    for &(j, v) in initial {
        params[j].initial = Some(v);
    }
    Operation::Fit {
        spec: FitSpec { model, params },
        range: None,
        residuals: false,
    }
}

/// Gaussienne (y0, A, xc, fwhm) partie à `xc` et `fwhm` donnés.
fn gaussian_fit(xc: f64, fwhm: f64) -> Operation {
    fit(Model::Gaussian, &[(2, xc), (3, fwhm)], 4)
}

fn peaks(center: f64) -> Operation {
    Operation::Peaks {
        spec: PeaksSpec {
            shape: PeakShape::Gaussian,
            background: PeakBackground::Constant,
            seeds: vec![PeakSeed {
                center,
                height: Some(1.0),
                fwhm: Some(1.0),
            }],
            link_widths: false,
            center_window: None,
        },
        auto: None,
        range: None,
        residuals: false,
    }
}

const ORTHO: [f64; 8] = [1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0];

/// `y = x + k·ORTHO` sur X = 0..7 : un ajustement affine laisse une part
/// SSres/SStot égale à `fraction` (SSres = 8k², SStot = 42 + 8k²).
fn affine_residue(fraction: f64) -> SeriesInput {
    let k = (42.0 * fraction / (8.0 * (1.0 - fraction))).sqrt();
    let x: Vec<f64> = (0..8).map(f64::from).collect();
    let y = x.iter().zip(ORTHO).map(|(x, r)| x + k * r).collect();
    SeriesInput { x, y, sigma: None }
}

/// `a + b·x + 0·c` : « c » est une case libre sans effet.
fn affine_with_dead_slot(shared: bool) -> Operation {
    let params = vec![
        ParamSetting {
            shared,
            ..ParamSetting::default()
        };
        3
    ];
    Operation::Fit {
        spec: FitSpec {
            model: Model::Custom {
                formula: "a + b*x + 0*c".into(),
            },
            params,
        },
        range: None,
        residuals: false,
    }
}

#[test]
fn l_exemple_de_l_issue_281_avertit_non_converge_apres_l_indetermination() {
    let o = run_one(&gaussian_fit(1000.0, 1.0), pic(0.0), None);
    assert_eq!(
        o.warnings,
        vec![Warning::Undetermined(3), Warning::NotConverged]
    );
    let r2 = o.values.iter().find(|v| v.key == "stat.r_squared").unwrap();
    assert!(r2.value < 0.1, "{}", r2.value);
}

#[test]
fn un_ajustement_sain_ou_bruite_n_avertit_pas() {
    for bruit in [0.0, 0.3] {
        let o = run_one(&gaussian_fit(7.0, 2.0), pic(bruit), None);
        assert_eq!(o.warnings, vec![], "bruit {bruit}");
        // Départ automatique aussi.
        let o = run_one(&fit(Model::Gaussian, &[], 4), pic(bruit), None);
        assert_eq!(o.warnings, vec![], "auto, bruit {bruit}");
    }
}

#[test]
fn un_ajustement_lineaire_ne_signale_jamais_de_plateau_meme_a_r2_nul() {
    // Presque aucun signal affine : R² ≈ 0, mais le chemin linéaire est exact.
    let input = affine_residue(0.99);
    let o = run_one(&fit(Model::Linear, &[], 2), input, None);
    let r2 = o.values.iter().find(|v| v.key == "stat.r_squared").unwrap();
    assert!(r2.value < 0.05, "{}", r2.value);
    assert_eq!(o.warnings, vec![]);
}

#[test]
fn le_seuil_du_plateau_se_voit_de_bout_en_bout_a_une_demie() {
    let sous = run_one(&affine_with_dead_slot(false), affine_residue(0.49), None);
    assert_eq!(sous.warnings, vec![Warning::Undetermined(1)]);
    let dessus = run_one(&affine_with_dead_slot(false), affine_residue(0.51), None);
    assert_eq!(
        dessus.warnings,
        vec![Warning::Undetermined(1), Warning::NotConverged]
    );
}

#[test]
fn un_modele_surparametre_qui_s_ajuste_bien_est_indetermine_sans_non_converge() {
    let o = run_one(&affine_with_dead_slot(false), affine_residue(0.01), None);
    assert_eq!(o.warnings, vec![Warning::Undetermined(1)]);
}

#[test]
fn des_courbes_independantes_avertissent_chacune_pour_elle_seule() {
    let lot = [affine_residue(0.01), affine_residue(0.9)];
    let out = run(&affine_with_dead_slot(false), &lot, None).unwrap();
    assert_eq!(out[0].warnings, vec![Warning::Undetermined(1)]);
    assert_eq!(
        out[1].warnings,
        vec![Warning::Undetermined(1), Warning::NotConverged]
    );
}

#[test]
fn un_ajustement_global_dont_une_seule_courbe_est_sur_un_plateau_avertit() {
    for lot in [
        [affine_residue(0.01), affine_residue(0.9)],
        [affine_residue(0.9), affine_residue(0.01)],
    ] {
        let out = run(&affine_with_dead_slot(true), &lot, None).unwrap();
        for (k, o) in out.iter().enumerate() {
            assert!(o.warnings.contains(&Warning::NotConverged), "courbe {k}");
        }
    }
    let lot = [affine_residue(0.01), affine_residue(0.02)];
    let out = run(&affine_with_dead_slot(true), &lot, None).unwrap();
    for o in &out {
        assert!(!o.warnings.contains(&Warning::NotConverged));
    }
}

#[test]
fn la_decomposition_en_pics_avertit_non_converge_hors_des_donnees_et_pas_sur_le_pic() {
    let o = run_one(&peaks(1000.0), pic(0.0), None);
    assert_eq!(
        o.warnings,
        vec![Warning::Undetermined(3), Warning::NotConverged]
    );
    for bruit in [0.0, 0.3] {
        let o = run_one(&peaks(8.0), pic(bruit), None);
        assert_eq!(o.warnings, vec![], "bruit {bruit}");
    }
}

fn signal_et_reponse() -> (AuxInput, SeriesInput) {
    let step = 0.05;
    let n = 400;
    let x: Vec<f64> = (0..n).map(|i| f64::from(i) * step).collect();
    let irf_y: Vec<f64> = x.iter().map(|t| gaussian(*t, 1.0, 1.0, 0.2)).collect();
    let aux = AuxInput {
        x: x.clone(),
        y: irf_y.clone(),
    };
    let sum: f64 = irf_y.iter().sum();
    let h: Vec<f64> = irf_y.iter().map(|v| v / sum).collect();
    let u: Vec<f64> = x.iter().map(|t| 50.0 * (-t / 1.5).exp()).collect();
    let y = (0..x.len())
        .map(|k| (0..=k).map(|j| h[j] * u[k - j]).sum::<f64>())
        .collect();
    (aux, SeriesInput { x, y, sigma: None })
}

/// Reconvolution à une exponentielle (bkg, A1, tau1, shift) où l'amplitude A1
/// est clouée à 0 : le modèle ne fait plus rien de `tau1`.
fn reconvolution_amplitude_nulle() -> Operation {
    let mut params = vec![ParamSetting::default(); 4];
    params[1] = ParamSetting {
        initial: Some(0.0),
        min: Some(0.0),
        max: Some(0.0),
        ..ParamSetting::default()
    };
    Operation::Reconvolution {
        spec: ReconvSpec {
            components: 1,
            params,
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        },
        residuals: false,
    }
}

#[test]
fn la_reconvolution_saine_n_avertit_pas() {
    let (aux, sig) = signal_et_reponse();
    let op = Operation::Reconvolution {
        spec: ReconvSpec {
            components: 1,
            params: vec![],
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        },
        residuals: false,
    };
    let o = run_one(&op, sig, Some(&aux));
    assert_eq!(o.warnings, vec![]);
}

#[test]
fn la_reconvolution_expose_le_plateau_du_solveur() {
    // Couche calcul : le drapeau est bien rempli (chemin direct).
    use stokesforge_analysis::deconv::reconvolve;
    use stokesforge_analysis::series::Series;
    let (aux, sig) = signal_et_reponse();
    let Operation::Reconvolution { spec, .. } = reconvolution_amplitude_nulle() else {
        unreachable!()
    };
    let r = reconvolve(
        &Series::new(sig.x.clone(), sig.y.clone()),
        &Series::new(aux.x.clone(), aux.y.clone()),
        &spec,
    )
    .unwrap();
    assert_eq!(r.undetermined, 2);
    assert!(r.plateau);
}

#[test]
fn la_reconvolution_sur_un_plateau_avertit_non_converge_par_ops_run() {
    let (aux, sig) = signal_et_reponse();
    let o = run_one(&reconvolution_amplitude_nulle(), sig, Some(&aux));
    assert_eq!(
        o.warnings,
        vec![Warning::Undetermined(2), Warning::NotConverged]
    );
}

/// Reconvolution à deux exponentielles (bkg, A1, tau1, A2, tau2, shift) dont
/// seule l'amplitude A2 est clouée à 0 : `tau2` est indéterminé, mais le
/// signal (une seule exponentielle) reste très bien ajusté.
fn reconvolution_seconde_amplitude_nulle() -> Operation {
    let mut params = vec![ParamSetting::default(); 6];
    params[3] = ParamSetting {
        initial: Some(0.0),
        min: Some(0.0),
        max: Some(0.0),
        ..ParamSetting::default()
    };
    Operation::Reconvolution {
        spec: ReconvSpec {
            components: 2,
            params,
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        },
        residuals: false,
    }
}

#[test]
fn la_reconvolution_avertit_indetermine_seul_par_ops_run() {
    let (aux, sig) = signal_et_reponse();
    let o = run_one(&reconvolution_seconde_amplitude_nulle(), sig, Some(&aux));
    assert_eq!(o.warnings, vec![Warning::Undetermined(2)]);
}

#[test]
fn la_reconvolution_garde_l_ordre_et_n_a_pas_de_doublon() {
    // Pas de l'IRF différent de celui du signal : `IrfStepMismatch` (déconvolution)
    // s'ajoute après `Undetermined` et `NotConverged`, sans les dupliquer.
    let (mut aux, sig) = signal_et_reponse();
    aux.x.iter_mut().for_each(|x| *x *= 1.5);
    let o = run_one(&reconvolution_amplitude_nulle(), sig, Some(&aux));
    assert_eq!(
        o.warnings,
        vec![
            Warning::Undetermined(2),
            Warning::NotConverged,
            Warning::IrfStepMismatch
        ]
    );
}

#[test]
fn la_deconvolution_directe_n_utilise_pas_le_solveur_et_n_avertit_pas_de_non_convergence() {
    let (aux, sig) = signal_et_reponse();
    let op = Operation::DirectDeconvolution {
        method: DirectMethod::RichardsonLucy { iterations: 20 },
        shift: 0.0,
    };
    let o = run_one(&op, sig, Some(&aux));
    assert!(!o.warnings.contains(&Warning::NotConverged));
    assert!(!o
        .warnings
        .iter()
        .any(|w| matches!(w, Warning::Undetermined(_))));
}
