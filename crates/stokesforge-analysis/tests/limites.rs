//! Les plafonds des réglages et les prérequis d'une opération (#260) : ce que
//! l'interface, `pyst` et le fichier lisent d'une seule source.

use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp, Interpolation};
use stokesforge_analysis::deconv::{DirectMethod, ReconvSpec};
use stokesforge_analysis::expr::ExprError;
use stokesforge_analysis::fit::{FitSpec, ParamSetting};
use stokesforge_analysis::integrate::{IntegrateSpec, Under};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{
    run, AutoDetect, Operation, Readiness, SeriesInput, MAX_COMPONENTS, MAX_DEGREE, MAX_ITERATIONS,
    MAX_PEAKS,
};
use stokesforge_analysis::peaks::{PeakBackground, PeakSeed, PeakShape, PeaksSpec};
use stokesforge_analysis::series::Series;
use stokesforge_analysis::AnalysisError;

fn invalid(name: &str) -> Result<(), AnalysisError> {
    Err(AnalysisError::InvalidParameter { name: name.into() })
}

fn fit(model: Model, params: Vec<ParamSetting>) -> Operation {
    Operation::Fit {
        spec: FitSpec { model, params },
        range: None,
        residuals: false,
    }
}

fn custom(formula: &str) -> Model {
    Model::Custom {
        formula: formula.into(),
    }
}

fn baseline(method: BaselineMethod) -> Operation {
    Operation::Baseline {
        method,
        op: BaselineOp::Subtract,
        keep_baseline: false,
    }
}

fn peaks(max_peaks: usize) -> Operation {
    Operation::Peaks {
        spec: PeaksSpec {
            shape: PeakShape::Gaussian,
            background: PeakBackground::Constant,
            seeds: Vec::new(),
            link_widths: false,
            center_window: None,
        },
        auto: Some(AutoDetect {
            min_prominence: 0.05,
            max_peaks,
        }),
        range: None,
        residuals: false,
    }
}

fn reconvolution(components: usize) -> Operation {
    Operation::Reconvolution {
        spec: ReconvSpec {
            components,
            params: Vec::new(),
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        },
        residuals: false,
    }
}

// ── Plafonds ───────────────────────────────────────────────────────────

/// Un degré lu tel quel des préférences atteint `compile` avant `validate` :
/// sans plafond, `degree + 1` paramètres nommés seraient fabriqués (des
/// milliards). Le refus doit venir avant toute allocation.
#[test]
fn compile_refuse_un_degre_hors_plafond_sans_allouer() {
    for degree in [MAX_DEGREE + 1, u32::MAX] {
        let result = Model::Polynomial { degree }.compile();
        assert_eq!(
            result.err(),
            Some(AnalysisError::InvalidParameter {
                name: "degree".into()
            }),
            "degré {degree}"
        );
    }
    // Le plafond lui-même reste compilable.
    let compiled = Model::Polynomial { degree: MAX_DEGREE }
        .compile()
        .expect("le plafond est permis");
    assert_eq!(compiled.param_count(), MAX_DEGREE as usize + 1);
}

#[test]
fn les_plafonds_sont_ceux_de_la_table_des_grandeurs() {
    assert_eq!(MAX_DEGREE, 9);
    assert_eq!(MAX_ITERATIONS, 5000);
    // Décision du 2026-10-15 : 100 pics partout (le champ en tenait 50).
    assert_eq!(MAX_PEAKS, 100);
    assert_eq!(MAX_COMPONENTS, 4);
}

#[test]
fn validate_accepte_le_plafond_et_refuse_au_dela() {
    assert_eq!(peaks(MAX_PEAKS).validate(), Ok(()));
    assert_eq!(peaks(MAX_PEAKS + 1).validate(), invalid("max_peaks"));
    assert_eq!(reconvolution(MAX_COMPONENTS).validate(), Ok(()));
    assert_eq!(reconvolution(1).validate(), Ok(()));
    assert_eq!(
        reconvolution(MAX_COMPONENTS + 1).validate(),
        invalid("components")
    );
    assert_eq!(reconvolution(0).validate(), invalid("components"));
    let degree = |degree| fit(Model::Polynomial { degree }, Vec::new());
    assert_eq!(degree(MAX_DEGREE).validate(), Ok(()));
    assert_eq!(degree(MAX_DEGREE + 1).validate(), invalid("degree"));
    let richardson = |iterations| Operation::DirectDeconvolution {
        method: DirectMethod::RichardsonLucy { iterations },
        shift: 0.0,
    };
    assert_eq!(richardson(MAX_ITERATIONS).validate(), Ok(()));
    assert_eq!(
        richardson(MAX_ITERATIONS + 1).validate(),
        invalid("iterations")
    );
}

#[test]
fn validate_refuse_une_borne_de_zone_non_finie() {
    let zones = |zones| baseline(BaselineMethod::Zones { zones, degree: 1 });
    assert_eq!(zones(vec![(0.0, 1.0)]).validate(), Ok(()));
    assert_eq!(zones(vec![(f64::NAN, 1.0)]).validate(), invalid("zones"));
    assert_eq!(
        zones(vec![(0.0, 1.0), (2.0, f64::INFINITY)]).validate(),
        invalid("zones")
    );
}

#[test]
fn validate_refuse_une_ancre_non_finie() {
    let anchors = |points| {
        baseline(BaselineMethod::Anchors {
            points,
            interpolation: Interpolation::Segments,
        })
    };
    assert_eq!(anchors(vec![(0.0, 1.0), (2.0, 3.0)]).validate(), Ok(()));
    assert_eq!(
        anchors(vec![(0.0, 1.0), (f64::NAN, 3.0)]).validate(),
        invalid("points")
    );
    assert_eq!(
        anchors(vec![(0.0, 1.0), (2.0, 3.0), (4.0, f64::INFINITY)]).validate(),
        invalid("points")
    );
}

#[test]
fn validate_refuse_un_point_de_droite_non_fini() {
    let line = |p1, p2| baseline(BaselineMethod::Line { p1, p2 });
    assert_eq!(line((0.0, 1.0), (4.0, 3.0)).validate(), Ok(()));
    assert_eq!(
        line((f64::NAN, 1.0), (4.0, 3.0)).validate(),
        invalid("points")
    );
    assert_eq!(
        line((0.0, 1.0), (4.0, f64::INFINITY)).validate(),
        invalid("points")
    );
}

#[test]
fn validate_refuse_un_decalage_initial_non_fini() {
    let shifted = |shift_initial| Operation::Reconvolution {
        spec: ReconvSpec {
            components: 1,
            params: Vec::new(),
            shift_initial,
            fit_shift: true,
            window: None,
        },
        residuals: false,
    };
    assert_eq!(shifted(-0.3).validate(), Ok(()));
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(shifted(bad).validate(), invalid("shift_initial"));
    }
}

#[test]
fn validate_refuse_une_plage_non_finie() {
    let fit_in = |range| match fit(Model::Linear, Vec::new()) {
        Operation::Fit {
            spec, residuals, ..
        } => Operation::Fit {
            spec,
            range,
            residuals,
        },
        _ => unreachable!(),
    };
    let peaks_in = |range| match peaks(3) {
        Operation::Peaks {
            spec,
            auto,
            residuals,
            ..
        } => Operation::Peaks {
            spec,
            auto,
            range,
            residuals,
        },
        _ => unreachable!(),
    };
    assert_eq!(fit_in(Some((0.0, 1.0))).validate(), Ok(()));
    assert_eq!(peaks_in(Some((0.0, 1.0))).validate(), Ok(()));
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(fit_in(Some((bad, 1.0))).validate(), invalid("range"));
        assert_eq!(fit_in(Some((0.0, bad))).validate(), invalid("range"));
        assert_eq!(peaks_in(Some((0.0, bad))).validate(), invalid("range"));
    }
}

#[test]
fn validate_refuse_une_graine_ou_une_proeminence_non_finie() {
    let seeded = |center, height, fwhm| match peaks(3) {
        Operation::Peaks {
            mut spec,
            auto,
            range,
            residuals,
        } => {
            spec.seeds = vec![PeakSeed {
                center,
                height,
                fwhm,
            }];
            Operation::Peaks {
                spec,
                auto: None,
                range,
                residuals: residuals && auto.is_some(),
            }
        }
        _ => unreachable!(),
    };
    assert_eq!(seeded(1.0, Some(2.0), Some(0.5)).validate(), Ok(()));
    assert_eq!(seeded(1.0, None, None).validate(), Ok(()));
    for bad in [f64::NAN, f64::INFINITY] {
        assert_eq!(seeded(bad, None, None).validate(), invalid("seeds"));
        assert_eq!(seeded(1.0, Some(bad), None).validate(), invalid("seeds"));
        assert_eq!(seeded(1.0, None, Some(bad)).validate(), invalid("seeds"));
    }
    // Une largeur imposée nulle ou négative est refusée (#285).
    for bad in [0.0, -1.0] {
        assert_eq!(seeded(1.0, None, Some(bad)).validate(), invalid("seeds"));
    }
    let prominent = |min_prominence| Operation::Peaks {
        spec: match peaks(3) {
            Operation::Peaks { spec, .. } => spec,
            _ => unreachable!(),
        },
        auto: Some(AutoDetect {
            min_prominence,
            max_peaks: 3,
        }),
        range: None,
        residuals: false,
    };
    assert_eq!(prominent(0.1).validate(), Ok(()));
    for bad in [f64::NAN, f64::INFINITY] {
        assert_eq!(prominent(bad).validate(), invalid("min_prominence"));
    }
}

#[test]
fn validate_refuse_un_decalage_ou_une_fenetre_non_finis() {
    let direct = |shift| Operation::DirectDeconvolution {
        method: DirectMethod::RichardsonLucy { iterations: 5 },
        shift,
    };
    assert_eq!(direct(0.5).validate(), Ok(()));
    assert_eq!(direct(f64::NAN).validate(), invalid("shift"));
    let windowed = |window| Operation::Reconvolution {
        spec: ReconvSpec {
            components: 1,
            params: Vec::new(),
            shift_initial: 0.0,
            fit_shift: true,
            window,
        },
        residuals: false,
    };
    assert_eq!(windowed(Some((0.0, 1.0))).validate(), Ok(()));
    assert_eq!(
        windowed(Some((0.0, f64::NAN))).validate(),
        invalid("window")
    );
}

#[test]
fn validate_refuse_un_ajustement_sans_parametre_libre() {
    // Aucun paramètre : l'expression n'en contient pas.
    assert_eq!(
        fit(custom("2 + 3"), Vec::new()).validate(),
        Err(AnalysisError::NoFreeParameter)
    );
    // Tous fixés : le solveur refuserait de la même façon, plus tard.
    let fixed = ParamSetting {
        fixed: true,
        ..ParamSetting::default()
    };
    assert_eq!(
        fit(Model::Linear, vec![fixed.clone(), fixed.clone()]).validate(),
        Err(AnalysisError::NoFreeParameter)
    );
    // Un seul paramètre libre suffit ; les réglages manquants sont libres.
    assert_eq!(
        fit(Model::Linear, vec![fixed.clone(), ParamSetting::default()]).validate(),
        Ok(())
    );
    assert_eq!(fit(Model::Linear, vec![fixed]).validate(), Ok(()));
    assert_eq!(fit(custom("a*x + b"), Vec::new()).validate(), Ok(()));
    // Le refus vient avant tout calcul.
    let input = SeriesInput {
        x: vec![0.0, 1.0, 2.0],
        y: vec![0.0, 1.0, 2.0],
        sigma: None,
    };
    assert_eq!(
        run(&fit(custom("2 + 3"), Vec::new()), &[input], None).err(),
        Some(AnalysisError::NoFreeParameter)
    );
}

// ── Prérequis ──────────────────────────────────────────────────────────

#[test]
fn readiness_dit_ce_qui_manque_a_une_ligne_de_base() {
    let nan = (f64::NAN, f64::NAN);
    // Point constant pas encore posé.
    assert_eq!(
        baseline(BaselineMethod::Constant { point: nan }).readiness(),
        Readiness::NeedsPoints { needed: 1, have: 0 }
    );
    assert_eq!(
        baseline(BaselineMethod::Constant { point: (1.0, 2.0) }).readiness(),
        Readiness::Ready
    );
    // Zone constante pas encore posée.
    assert_eq!(
        baseline(BaselineMethod::ConstantZone { zone: nan }).readiness(),
        Readiness::NeedsZone
    );
    assert_eq!(
        baseline(BaselineMethod::ConstantZone { zone: (0.0, 1.0) }).readiness(),
        Readiness::Ready
    );
    // Droite : le nombre de points posés est dit.
    let line = |p1, p2| baseline(BaselineMethod::Line { p1, p2 }).readiness();
    assert_eq!(
        line(nan, nan),
        Readiness::NeedsPoints { needed: 2, have: 0 }
    );
    assert_eq!(
        line((0.0, 1.0), nan),
        Readiness::NeedsPoints { needed: 2, have: 1 }
    );
    assert_eq!(line((0.0, 1.0), (2.0, 3.0)), Readiness::Ready);
    // Zones de fond.
    let zones = |zones| baseline(BaselineMethod::Zones { zones, degree: 1 }).readiness();
    assert_eq!(zones(Vec::new()), Readiness::NeedsZone);
    assert_eq!(zones(vec![(0.0, 1.0)]), Readiness::Ready);
    assert_eq!(zones(vec![(0.0, 1.0), (nan.0, 2.0)]), Readiness::NeedsZone);
    // Points d'ancrage.
    let anchors = |points: Vec<(f64, f64)>| {
        baseline(BaselineMethod::Anchors {
            points,
            interpolation: Interpolation::Segments,
        })
        .readiness()
    };
    assert_eq!(
        anchors(Vec::new()),
        Readiness::NeedsPoints { needed: 2, have: 0 }
    );
    assert_eq!(
        anchors(vec![(0.0, 1.0)]),
        Readiness::NeedsPoints { needed: 2, have: 1 }
    );
    assert_eq!(anchors(vec![(0.0, 1.0), (2.0, 3.0)]), Readiness::Ready);
    // Une ancre non posée ne compte pas.
    assert_eq!(
        anchors(vec![(0.0, 1.0), nan]),
        Readiness::NeedsPoints { needed: 2, have: 1 }
    );
    assert_eq!(anchors(vec![(0.0, 1.0), (2.0, 3.0), nan]), Readiness::Ready);
}

#[test]
fn readiness_dit_ce_qui_manque_a_un_ajustement() {
    assert_eq!(fit(Model::Linear, Vec::new()).readiness(), Readiness::Ready);
    assert!(matches!(
        fit(custom("a*"), Vec::new()).readiness(),
        Readiness::BadExpression(ExprError { .. })
    ));
    assert_eq!(
        fit(custom("2 + 3"), Vec::new()).readiness(),
        Readiness::NoParameter
    );
    // Un degré hors plafond (lu d'un fichier) est un réglage refusé, sans
    // allocation : jamais l'abandon de l'application.
    assert_eq!(
        fit(Model::Polynomial { degree: u32::MAX }, Vec::new()).readiness(),
        Readiness::Invalid(AnalysisError::InvalidParameter {
            name: "degree".into()
        })
    );
}

#[test]
fn les_autres_operations_sont_toujours_pretes() {
    let integrate = Operation::Integrate {
        spec: IntegrateSpec {
            ranges: Vec::new(),
            under: Under::Zero,
            cumulative: false,
            normalize: false,
        },
    };
    for op in [
        integrate,
        peaks(6),
        reconvolution(1),
        Operation::DirectDeconvolution {
            method: DirectMethod::Wiener { noise: None },
            shift: 0.0,
        },
    ] {
        assert_eq!(op.readiness(), Readiness::Ready, "{op:?}");
    }
}

// ── Oublier les positions ──────────────────────────────────────────────

fn set() -> ParamSetting {
    ParamSetting {
        initial: Some(3.0),
        fixed: true,
        min: Some(-1.0),
        max: Some(9.0),
        shared: true,
    }
}

#[test]
fn forget_positions_efface_les_plages_et_les_reglages_par_parametre() {
    let mut op = Operation::Fit {
        spec: FitSpec {
            model: Model::Linear,
            params: vec![set(), set()],
        },
        range: Some((0.0, 5.0)),
        residuals: true,
    };
    op.forget_positions();
    // `initial`, `fixed`, `min`, `max` et `shared` : tout revient au défaut,
    // la longueur et le reste (résidus) sont gardés.
    assert_eq!(
        op,
        Operation::Fit {
            spec: FitSpec {
                model: Model::Linear,
                params: vec![ParamSetting::default(); 2],
            },
            range: None,
            residuals: true,
        }
    );

    let mut op = Operation::Integrate {
        spec: IntegrateSpec {
            ranges: vec![(0.0, 1.0)],
            under: Under::Zero,
            cumulative: true,
            normalize: true,
        },
    };
    op.forget_positions();
    assert_eq!(
        op,
        Operation::Integrate {
            spec: IntegrateSpec {
                ranges: Vec::new(),
                under: Under::Zero,
                cumulative: true,
                normalize: true,
            },
        }
    );

    let mut op = peaks(6);
    if let Operation::Peaks { spec, range, .. } = &mut op {
        spec.seeds = vec![PeakSeed {
            center: 1.0,
            height: Some(2.0),
            fwhm: None,
        }];
        spec.center_window = Some(0.5);
        spec.link_widths = true;
        *range = Some((0.0, 9.0));
    }
    op.forget_positions();
    let Operation::Peaks {
        spec, auto, range, ..
    } = &op
    else {
        panic!("{op:?}");
    };
    assert!(spec.seeds.is_empty());
    assert_eq!(*range, None);
    // Les réglages de méthode restent.
    assert_eq!(spec.center_window, Some(0.5));
    assert!(spec.link_widths);
    assert_eq!(auto.map(|a| a.max_peaks), Some(6));

    let mut op = Operation::Reconvolution {
        spec: ReconvSpec {
            components: 2,
            params: vec![set(); 6],
            shift_initial: 0.25,
            fit_shift: true,
            window: Some((1.0, 2.0)),
        },
        residuals: false,
    };
    op.forget_positions();
    assert_eq!(
        op,
        Operation::Reconvolution {
            spec: ReconvSpec {
                components: 2,
                params: vec![ParamSetting::default(); 6],
                shift_initial: 0.25,
                fit_shift: true,
                window: None,
            },
            residuals: false,
        }
    );
}

#[test]
fn forget_positions_garde_la_definition_d_une_ligne_de_base() {
    let method = BaselineMethod::Anchors {
        points: vec![(0.0, 1.0), (2.0, 3.0)],
        interpolation: Interpolation::Spline,
    };
    let mut op = baseline(method.clone());
    op.forget_positions();
    assert_eq!(op, baseline(method));
    // Ni un degré énorme n'est compilé : rien n'est alloué.
    let mut op = fit(Model::Polynomial { degree: u32::MAX }, vec![set()]);
    op.forget_positions();
    assert_eq!(
        op,
        fit(
            Model::Polynomial { degree: u32::MAX },
            vec![ParamSetting::default()]
        )
    );
}

// ── Estimation automatique ─────────────────────────────────────────────

fn droite() -> Series {
    let x: Vec<f64> = (0..50).map(f64::from).collect();
    let y = x.iter().map(|x| 300.0 * x + 40.0).collect();
    Series::new(x, y)
}

#[test]
fn auto_initial_suit_le_modele_sur_les_points_de_la_plage() {
    let series = droite();
    let mut spec = FitSpec {
        model: Model::Linear,
        params: Vec::new(),
    };
    // Sans plage : la droite entière ; les réglages sont ramenés à la longueur
    // du modèle.
    spec.auto_initial(&series, None);
    let expected = spec.model.compile().unwrap().guess(&series);
    let initial: Vec<Option<f64>> = spec.params.iter().map(|p| p.initial).collect();
    assert_eq!(
        initial,
        expected.iter().map(|v| Some(*v)).collect::<Vec<_>>()
    );
    assert!((expected[0] - 40.0).abs() < 1e-6 && (expected[1] - 300.0).abs() < 1e-6);

    // Une plage ne garde que ses points, bornes incluses, dans n'importe quel
    // ordre : sur ce modèle exact le départ ne change pas, donc on la teste
    // avec des Y qui cassent la droite hors de la plage.
    let mut broken = series.clone();
    for (x, y) in broken.x.iter().zip(broken.y.iter_mut()) {
        if *x < 10.0 || *x > 20.0 {
            *y = -5_000.0;
        }
    }
    let mut spec = FitSpec {
        model: Model::Linear,
        params: Vec::new(),
    };
    spec.auto_initial(&broken, Some((20.0, 10.0)));
    let initial: Vec<f64> = spec.params.iter().map(|p| p.initial.unwrap()).collect();
    assert!((initial[0] - 40.0).abs() < 1e-6, "{initial:?}");
    assert!((initial[1] - 300.0).abs() < 1e-6, "{initial:?}");
    // Sans plage, les points cassés tirent l'estimation ailleurs (témoin).
    let mut spec = FitSpec {
        model: Model::Linear,
        params: Vec::new(),
    };
    spec.auto_initial(&broken, None);
    assert!((spec.params[1].initial.unwrap() - 300.0).abs() > 1.0);
}

#[test]
fn auto_initial_ignore_les_parametres_fixes_et_les_modeles_illisibles() {
    let series = droite();
    let mut spec = FitSpec {
        model: Model::Linear,
        params: vec![
            ParamSetting {
                initial: Some(7.0),
                fixed: true,
                ..ParamSetting::default()
            },
            ParamSetting::default(),
        ],
    };
    spec.auto_initial(&series, None);
    assert_eq!(spec.params[0].initial, Some(7.0));
    assert!((spec.params[1].initial.unwrap() - 300.0).abs() < 1e-6);

    // Un modèle qui ne se compile pas : rien ne bouge.
    let mut bad = FitSpec {
        model: custom("a*"),
        params: vec![ParamSetting::default()],
    };
    bad.auto_initial(&series, None);
    assert_eq!(bad.params, vec![ParamSetting::default()]);
}
