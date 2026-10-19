//! #282 : le pas de Levenberg-Marquardt par moindres carrés augmentés (QR) et
//! le plancher de λ à 1e-20 tiennent un X décalé (colonnes du jacobien quasi
//! colinéaires). Références : le modèle `Linear` (moindres carrés directs) et
//! le même ajustement en repère centré.

use stokesforge_analysis::fit::{fit, FitInput, FitReport, FitSpec, ParamSetting};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{run, Operation, SeriesInput};
use stokesforge_analysis::series::Series;
use stokesforge_analysis::AnalysisError;

fn t_grid() -> Vec<f64> {
    (0..60).map(|i| f64::from(i) / 59.0).collect()
}

/// Bruit déterministe d'amplitude `amp`.
fn bruit(i: usize, amp: f64) -> f64 {
    amp * ((i as f64) * 1.7).sin()
}

fn libre() -> Model {
    Model::Custom {
        formula: "a + b*x".into(),
    }
}

fn droite(off: f64, amp: f64) -> FitInput {
    let x: Vec<f64> = t_grid().iter().map(|t| off + t).collect();
    let y: Vec<f64> = t_grid()
        .iter()
        .enumerate()
        .map(|(i, t)| 2.0 + 3.0 * t + bruit(i, amp))
        .collect();
    FitInput {
        series: Series::new(x, y),
        sigma: None,
    }
}

fn spec(model: Model, params: Vec<ParamSetting>) -> FitSpec {
    FitSpec { model, params }
}

fn ajuste(inputs: &[FitInput], model: Model) -> FitReport {
    fit(inputs, &spec(model, vec![])).unwrap()
}

fn proche(a: f64, b: f64, rel: f64, abs: f64) -> bool {
    (a - b).abs() <= abs + rel * a.abs().max(b.abs())
}

const DECALAGES_TENUS: [f64; 3] = [1e3, 1e5, 1e6];

#[test]
fn le_cas_de_l_issue_donne_les_valeurs_du_modele_lineaire() {
    let input = [droite(1e6, 0.0)];
    let libre = ajuste(&input, libre());
    let lin = ajuste(&input, Model::Linear);
    let (l, d) = (&libre.series[0], &lin.series[0]);
    // Libre : (a, b) ; linéaire : (intercept, slope).
    assert!(proche(l.values[1], d.values[1], 0.0, 1e-8), "{l:?} {d:?}");
    assert!(proche(l.values[0], d.values[0], 1e-9, 0.0), "{l:?} {d:?}");
    assert!(proche(l.values[1], 3.0, 0.0, 1e-8));
    assert!(!l.plateau);
}

#[test]
#[ignore = "bug #293"]
fn le_cas_de_l_issue_donne_les_incertitudes_du_modele_lineaire() {
    // Bruité, pour que les incertitudes aient un sens.
    let input = [droite(1e6, 0.01)];
    let libre = ajuste(&input, libre());
    let lin = ajuste(&input, Model::Linear);
    let (l, d) = (&libre.series[0], &lin.series[0]);
    assert_eq!(l.undetermined, 0, "{l:?}");
    for j in 0..2 {
        let (el, ed) = (l.errors[j].unwrap(), d.errors[j].unwrap());
        assert!(proche(el, ed, 1e-2, 0.0), "param {j} : {el} contre {ed}");
    }
}

#[test]
fn les_decalages_tenus_restent_coherents_avec_le_modele_lineaire() {
    for off in DECALAGES_TENUS {
        for amp in [0.0, 0.01] {
            let input = [droite(off, amp)];
            let libre = ajuste(&input, libre());
            let lin = ajuste(&input, Model::Linear);
            let (l, d) = (&libre.series[0], &lin.series[0]);
            let ctx = format!("X = {off:e}, bruit {amp} : {l:?} contre {d:?}");
            assert!(!l.plateau, "{ctx}");
            assert!(proche(l.values[1], d.values[1], 0.0, 1e-6), "{ctx}");
            assert!(proche(l.values[0], d.values[0], 1e-6, 0.0), "{ctx}");
            // χ² : bruité, égal à 1e-6 près ; sans bruit, tous deux ~0.
            assert!(proche(l.chi2, d.chi2, 1e-6, 1e-15 * off * off), "{ctx}");
            assert_eq!(l.points, d.points, "{ctx}");
        }
    }
}

#[test]
fn les_incertitudes_aux_decalages_moderes_suivent_le_modele_lineaire() {
    // 1e3 et 1e5 : les deux colonnes sont encore jugées indépendantes.
    for off in [1e3, 1e5] {
        let input = [droite(off, 0.01)];
        let libre = ajuste(&input, libre());
        let lin = ajuste(&input, Model::Linear);
        let (l, d) = (&libre.series[0], &lin.series[0]);
        assert_eq!(l.undetermined, 0, "X = {off:e} : {l:?}");
        for j in 0..2 {
            let (el, ed) = (l.errors[j].unwrap(), d.errors[j].unwrap());
            assert!(
                proche(el, ed, 1e-3, 0.0),
                "X = {off:e}, param {j} : {el} / {ed}"
            );
        }
    }
}

#[test]
fn au_dela_le_resultat_est_fini_et_jamais_un_faux_silencieux() {
    // Comportement actuel (#293) : pas de panique, pas de NaN, et soit le
    // χ² du modèle linéaire, soit un avertissement qui dit que le résultat
    // n'est pas sûr. Jamais un résultat faux sans rien dire.
    for off in [1e7, 1e8, 1e10] {
        let x: Vec<f64> = t_grid().iter().map(|t| off + t).collect();
        let y: Vec<f64> = t_grid()
            .iter()
            .enumerate()
            .map(|(i, t)| 2.0 + 3.0 * t + bruit(i, 0.01))
            .collect();
        let op = Operation::Fit {
            spec: spec(libre(), vec![]),
            range: None,
            residuals: false,
        };
        let res = run(&op, &[SeriesInput { x, y, sigma: None }], None);
        let o = match res {
            Ok(mut v) => v.remove(0),
            Err(e) => {
                assert!(
                    matches!(e, AnalysisError::NotConverged { .. }),
                    "X = {off:e} : {e:?}"
                );
                continue;
            }
        };
        for v in &o.values {
            assert!(v.value.is_finite(), "X = {off:e} : {v:?}");
            assert!(v.error.is_none_or(f64::is_finite), "X = {off:e} : {v:?}");
        }
        let chi2 = |o: &stokesforge_analysis::ops::Outcome| {
            let dof = o.values.iter().find(|v| v.key == "stat.dof").unwrap().value;
            let red = o
                .values
                .iter()
                .find(|v| v.key == "stat.reduced_chi2")
                .unwrap()
                .value;
            dof * red
        };
        let lin = ajuste(&[droite(off, 0.01)], Model::Linear);
        let juste = proche(chi2(&o), lin.series[0].chi2, 1e-3, 0.0);
        let prevenu = !o.warnings.is_empty();
        assert!(juste || prevenu, "X = {off:e} : faux et silencieux : {o:?}");
    }
}

#[test]
#[ignore = "bug #293"]
fn au_dela_le_resultat_est_celui_du_modele_lineaire() {
    for off in [1e7, 1e8, 1e10] {
        let input = [droite(off, 0.01)];
        let l = &ajuste(&input, libre()).series[0].clone();
        let d = &ajuste(&input, Model::Linear).series[0].clone();
        assert!(
            proche(l.values[1], d.values[1], 0.0, 1e-3),
            "X = {off:e} : {l:?}"
        );
        assert!(proche(l.chi2, d.chi2, 1e-3, 0.0), "X = {off:e} : {l:?}");
    }
}

fn gaussienne_decalee(off: f64) -> Vec<f64> {
    // y0 = 1, A = 5, xc = off + 0.5, fwhm = 0.3, sans bruit.
    let x: Vec<f64> = (0..120).map(|i| off + f64::from(i) / 119.0).collect();
    let y: Vec<f64> = x
        .iter()
        .map(|x| {
            let u = x - (off + 0.5);
            1.0 + 5.0 * (-4.0 * 2f64.ln() * u * u / 0.09).exp()
        })
        .collect();
    let r = ajuste(
        &[FitInput {
            series: Series::new(x, y),
            sigma: None,
        }],
        Model::Gaussian,
    );
    assert!(!r.series[0].plateau);
    r.series[0].values.clone()
}

fn verifie_gaussienne(off: f64) {
    let v = gaussienne_decalee(off);
    let ctx = format!("X = {off:e} : {v:?}");
    assert!(proche(v[0], 1.0, 0.0, 1e-6), "{ctx}");
    assert!(proche(v[1], 5.0, 0.0, 1e-6), "{ctx}");
    assert!(proche(v[2], off + 0.5, 1e-12, 0.0), "{ctx}");
    assert!(proche(v[3], 0.3, 0.0, 1e-6), "{ctx}");
}

#[test]
fn une_gaussienne_sur_un_x_decale_retrouve_ses_parametres() {
    for off in [0.0, 1e3, 1e5, 3e5] {
        verifie_gaussienne(off);
    }
}

#[test]
#[ignore = "bug #294"]
fn une_gaussienne_decalee_de_un_million_retrouve_ses_parametres() {
    verifie_gaussienne(1e6);
}

#[test]
fn une_exponentielle_sur_un_x_decale_retrouve_ses_parametres() {
    // y = A·exp(−x/τ), x = off + t : le facteur e^(−off/τ) est absorbé par A.
    for (off, tau) in [(100.0, 50.0), (1e3, 500.0)] {
        let x: Vec<f64> = (0..80).map(|i| off + 4.0 * f64::from(i) / 79.0).collect();
        let a = 7.0;
        let y: Vec<f64> = x.iter().map(|x| a * (-x / tau).exp()).collect();
        let r = ajuste(
            &[FitInput {
                series: Series::new(x, y),
                sigma: None,
            }],
            Model::Exponential,
        );
        let v = &r.series[0].values;
        let ctx = format!("X = {off} : {v:?}");
        assert!(proche(v[1], tau, 1e-6, 0.0), "{ctx}");
        assert!(proche(v[0], a, 1e-5, 0.0), "{ctx}");
    }
}

#[test]
fn un_parametre_fixe_reste_exact_et_l_autre_suit_les_moindres_carres() {
    // b fixé à 2,9 : a = moyenne(y − 2,9·x), exacte.
    let input = droite(1e5, 0.01);
    let n = input.series.x.len() as f64;
    let attendu = input
        .series
        .x
        .iter()
        .zip(&input.series.y)
        .map(|(x, y)| y - 2.9 * x)
        .sum::<f64>()
        / n;
    let params = vec![
        ParamSetting::default(),
        ParamSetting {
            initial: Some(2.9),
            fixed: true,
            ..Default::default()
        },
    ];
    let r = fit(&[input], &spec(libre(), params)).unwrap();
    let s = &r.series[0];
    assert_eq!(s.values[1], 2.9);
    assert!(s.errors[1].is_none());
    assert!(
        proche(s.values[0], attendu, 1e-9, 0.0),
        "{s:?} contre {attendu}"
    );
    assert!(s.errors[0].is_some());
}

#[test]
fn une_borne_active_tient_sur_un_x_decale() {
    // Pente vraie 3, bornée à 2,5 : le solveur s'arrête sur la borne, et
    // l'ordonnée est celle des moindres carrés à pente 2,5.
    let input = droite(1e5, 0.0);
    let attendu = input
        .series
        .x
        .iter()
        .zip(&input.series.y)
        .map(|(x, y)| y - 2.5 * x)
        .sum::<f64>()
        / input.series.x.len() as f64;
    let params = vec![
        ParamSetting::default(),
        ParamSetting {
            initial: Some(2.0),
            max: Some(2.5),
            ..Default::default()
        },
    ];
    let r = fit(&[input], &spec(libre(), params)).unwrap();
    let s = &r.series[0];
    assert!(s.values[1] <= 2.5, "{s:?}");
    assert!(proche(s.values[1], 2.5, 0.0, 1e-9), "{s:?}");
    assert!(
        proche(s.values[0], attendu, 1e-6, 0.0),
        "{s:?} contre {attendu}"
    );
}

#[test]
fn un_ajustement_global_a_pente_partagee_sur_un_x_decale() {
    let off = 1e5;
    let mk = |a: f64| {
        let x: Vec<f64> = t_grid().iter().map(|t| off + t).collect();
        let y: Vec<f64> = x.iter().map(|x| a + 3.0 * (x - off)).collect();
        FitInput {
            series: Series::new(x, y),
            sigma: None,
        }
    };
    // a + b·x avec x = off + t : y = a_k + 3t = (a_k − 3·off) + 3x.
    let params = vec![
        ParamSetting::default(),
        ParamSetting {
            shared: true,
            ..Default::default()
        },
    ];
    let r = fit(&[mk(2.0), mk(-1.0)], &spec(libre(), params)).unwrap();
    assert_eq!(r.series.len(), 2);
    for (s, a) in r.series.iter().zip([2.0, -1.0]) {
        assert!(proche(s.values[1], 3.0, 0.0, 1e-6), "{s:?}");
        assert!(proche(s.values[0], a - 3.0 * off, 1e-8, 0.0), "{s:?}");
        assert!(!s.plateau);
    }
    // Pente partagée : la même valeur exacte pour les deux courbes.
    assert_eq!(r.series[0].values[1], r.series[1].values[1]);
}

#[test]
fn le_resultat_en_repere_centre_ne_depend_pas_du_decalage() {
    // « a + b·(x − off) » sur X = off + t est « a + b·t » : mêmes paramètres
    // et même χ² qu'en X = t, à la tolérance près, quel que soit off.
    let ref_ = ajuste(&[droite(0.0, 0.01)], libre());
    let r0 = &ref_.series[0];
    for off in [1e3, 1e5, 1e6, 1e8] {
        let model = Model::Custom {
            formula: format!("a + b*(x - {off:e})"),
        };
        let r = ajuste(&[droite(off, 0.01)], model);
        let s = &r.series[0];
        let ctx = format!("X = {off:e} : {s:?} contre {r0:?}");
        assert!(proche(s.values[0], r0.values[0], 1e-6, 0.0), "{ctx}");
        assert!(proche(s.values[1], r0.values[1], 1e-6, 0.0), "{ctx}");
        assert!(proche(s.chi2, r0.chi2, 1e-6, 0.0), "{ctx}");
        for j in 0..2 {
            assert!(
                proche(s.errors[j].unwrap(), r0.errors[j].unwrap(), 1e-4, 0.0),
                "{ctx}"
            );
        }
    }
}

#[test]
fn un_ajustement_ordinaire_sans_decalage_ne_change_pas() {
    // Non-régression : X ordinaire, modèle libre = modèle linéaire à 1e-9.
    let input = [droite(0.0, 0.05)];
    let l = ajuste(&input, libre());
    let d = ajuste(&input, Model::Linear);
    for j in 0..2 {
        assert!(proche(
            l.series[0].values[j],
            d.series[0].values[j],
            1e-9,
            1e-9
        ));
        assert!(proche(
            l.series[0].errors[j].unwrap(),
            d.series[0].errors[j].unwrap(),
            1e-6,
            0.0
        ));
    }
}

fn parabole(off: f64) -> FitInput {
    let x: Vec<f64> = t_grid().iter().map(|t| off + t).collect();
    let y: Vec<f64> = t_grid()
        .iter()
        .enumerate()
        .map(|(i, t)| 2.0 + 3.0 * t - 1.5 * t * t + bruit(i, 0.01))
        .collect();
    FitInput {
        series: Series::new(x, y),
        sigma: None,
    }
}

#[test]
fn un_polynome_libre_de_degre_deux_atteint_le_chi2_du_modele_polynomial() {
    // Trois colonnes quasi colinéaires : les équations normales (conditionnement
    // au carré) s'arrêtent à un χ² 300 fois trop grand à X = 1e4 ; le QR non.
    // (Les incertitudes de ce cas relèvent de #293.)
    for off in [1e3, 1e4] {
        let input = [parabole(off)];
        let libre = ajuste(
            &input,
            Model::Custom {
                formula: "a + b*x + c*x^2".into(),
            },
        );
        let poly = ajuste(&input, Model::Polynomial { degree: 2 });
        let (l, p) = (&libre.series[0], &poly.series[0]);
        assert!(!l.plateau, "X = {off:e} : {l:?}");
        assert!(
            proche(l.chi2, p.chi2, 1e-4, 0.0),
            "X = {off:e} : {l:?} contre {p:?}"
        );
    }
}
