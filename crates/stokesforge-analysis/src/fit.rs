//! Ajustement d'un modèle à une ou plusieurs courbes (#188), avec paramètres
//! fixés, bornés, pondération par un écart-type Y, et **ajustement global**
//! (paramètres partagés entre les courbes, M13 b).

use crate::error::AnalysisError;
use crate::linalg::least_squares;
use crate::model::{CompiledModel, Model};
use crate::series::Series;
use crate::solver::{solve_problem, Options, Slot, Term};

/// Réglage d'un paramètre du modèle, dans l'ordre de [`CompiledModel::params`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParamSetting {
    /// `None` : valeur initiale automatique.
    pub initial: Option<f64>,
    pub fixed: bool,
    /// `None` : borne par défaut du modèle.
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// Ajustement global : une seule valeur pour toutes les courbes.
    pub shared: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FitSpec {
    pub model: Model,
    /// Réglages par paramètre ; les paramètres au-delà de la liste ont les
    /// réglages par défaut.
    pub params: Vec<ParamSetting>,
}

/// Une courbe à ajuster : points finis, déjà restreints à la plage voulue.
#[derive(Debug, Clone, PartialEq)]
pub struct FitInput {
    pub series: Series,
    /// Écart-type de chaque Y (pondération `1/σ`), de même longueur.
    pub sigma: Option<Vec<f64>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FittedSeries {
    pub values: Vec<f64>,
    /// `None` : paramètre fixé, ou incertitude non calculable.
    pub errors: Vec<Option<f64>>,
    pub points: usize,
    pub r_squared: f64,
    /// χ² pondéré de cette courbe.
    pub chi2: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FitReport {
    pub param_names: Vec<String>,
    pub series: Vec<FittedSeries>,
    /// Degrés de liberté et χ² réduit de l'ajustement (global : de l'ensemble).
    pub dof: usize,
    pub reduced_chi2: f64,
    pub iterations: usize,
}

fn validate_sigma(input: &FitInput) -> Result<(), AnalysisError> {
    if let Some(s) = &input.sigma {
        if s.len() != input.series.len() || s.iter().any(|v| !(*v > 0.0) || !v.is_finite()) {
            return Err(AnalysisError::InvalidParameter {
                name: "sigma".into(),
            });
        }
    }
    Ok(())
}

fn setting(spec: &FitSpec, j: usize) -> ParamSetting {
    spec.params.get(j).cloned().unwrap_or_default()
}

/// Ajuste le modèle. Sans paramètre partagé, chaque courbe est ajustée
/// séparément ; dès qu'un paramètre est partagé, toutes le sont ensemble.
pub fn fit(inputs: &[FitInput], spec: &FitSpec) -> Result<FitReport, AnalysisError> {
    let model = spec.model.compile()?;
    if inputs.is_empty() {
        return Err(AnalysisError::NotEnoughPoints {
            found: 0,
            needed: model.param_count(),
        });
    }
    for i in inputs {
        validate_sigma(i)?;
    }
    let k = model.param_count();
    let global = (0..k).any(|j| setting(spec, j).shared) && inputs.len() > 1;
    if global {
        return fit_together(inputs, spec, &model);
    }
    // Courbes indépendantes : rapport agrégé (dof et χ² réduit sommés).
    let mut series = Vec::new();
    let (mut dof, mut chi2_sum, mut iterations) = (0usize, 0.0, 0usize);
    for input in inputs {
        let (one, d, it) = fit_one(input, spec, &model)?;
        dof += d;
        chi2_sum += one.chi2;
        iterations = iterations.max(it);
        series.push(one);
    }
    Ok(FitReport {
        param_names: model.params().iter().map(|p| p.name.clone()).collect(),
        series,
        dof,
        reduced_chi2: if dof > 0 {
            chi2_sum / dof as f64
        } else {
            f64::NAN
        },
        iterations,
    })
}

/// Une courbe seule : moindres carrés exacts si le modèle est linéaire et sans
/// contrainte, sinon Levenberg-Marquardt.
fn fit_one(
    input: &FitInput,
    spec: &FitSpec,
    model: &CompiledModel,
) -> Result<(FittedSeries, usize, usize), AnalysisError> {
    let k = model.param_count();
    let constrained = (0..k).any(|j| {
        let s = setting(spec, j);
        s.fixed || s.min.is_some() || s.max.is_some() || s.initial.is_some()
    });
    if spec.model.is_linear_in_params() && !constrained {
        if let Some(r) = fit_linear(input, model) {
            return Ok((r.0, r.1, 1));
        }
    }
    let guess = model.guess(&input.series);
    let slots: Vec<Slot> = (0..k)
        .map(|j| {
            let s = setting(spec, j);
            let def = &model.params()[j];
            Slot {
                name: def.name.clone(),
                initial: s.initial.unwrap_or(guess[j]),
                fixed: s.fixed,
                min: s.min.unwrap_or(def.min),
                max: s.max.unwrap_or(def.max),
            }
        })
        .collect();
    let x = &input.series.x;
    let sol = {
        let term = Term {
            x,
            y: &input.series.y,
            sigma: input.sigma.as_deref(),
            eval: Box::new(|p, out| {
                for (o, &xi) in out.iter_mut().zip(x.iter()) {
                    *o = model.eval(p, xi);
                }
            }),
        };
        solve_problem(&slots, &[term], Options::default())?
    };
    Ok((
        FittedSeries {
            values: sol.values,
            errors: sol.errors,
            points: sol.terms[0].points,
            r_squared: sol.terms[0].r_squared,
            chi2: sol.chi2,
        },
        sol.dof,
        sol.iterations,
    ))
}

/// Moindres carrés linéaires exacts (QR), pondérés par `1/σ`.
fn fit_linear(input: &FitInput, model: &CompiledModel) -> Option<(FittedSeries, usize)> {
    let s = &input.series;
    let n = s.len();
    let k = model.param_count();
    if n < k || (matches!(model.model(), Model::Logarithm) && s.x.iter().any(|v| *v <= 0.0)) {
        return None;
    }
    let mut a = model.design_matrix(&s.x)?;
    let mut y = s.y.clone();
    if let Some(sig) = &input.sigma {
        for i in 0..n {
            let w = 1.0 / sig[i];
            for j in 0..k {
                a.set(i, j, a.get(i, j) * w);
            }
            y[i] *= w;
        }
    }
    let ls = least_squares(&a, &y)?;
    let fitted: Vec<f64> =
        s.x.iter()
            .map(|&x| model.eval(&ls.coefficients, x))
            .collect();
    let chi2: f64 = (0..n)
        .map(|i| {
            let w = input.sigma.as_ref().map_or(1.0, |sg| 1.0 / sg[i]);
            ((s.y[i] - fitted[i]) * w).powi(2)
        })
        .sum();
    let dof = n - k;
    let scale = if dof > 0 { chi2 / dof as f64 } else { f64::NAN };
    let errors = (0..k)
        .map(|j| {
            let v = ls.unscaled_covariance.get(j, j) * scale;
            (v.is_finite() && v >= 0.0).then(|| v.sqrt())
        })
        .collect();
    let mean = s.y.iter().sum::<f64>() / n as f64;
    let ss_tot: f64 = s.y.iter().map(|v| (v - mean).powi(2)).sum();
    let ss_res: f64 = (0..n).map(|i| (s.y[i] - fitted[i]).powi(2)).sum();
    Some((
        FittedSeries {
            values: ls.coefficients,
            errors,
            points: n,
            r_squared: if ss_tot > 0.0 {
                1.0 - ss_res / ss_tot
            } else {
                f64::NAN
            },
            chi2,
        },
        dof,
    ))
}

/// Ajustement global : une case par paramètre partagé, une case par courbe pour
/// les autres.
fn fit_together(
    inputs: &[FitInput],
    spec: &FitSpec,
    model: &CompiledModel,
) -> Result<FitReport, AnalysisError> {
    let k = model.param_count();
    let guesses: Vec<Vec<f64>> = inputs.iter().map(|i| model.guess(&i.series)).collect();
    let mut slots: Vec<Slot> = Vec::new();
    // map[serie][param] = indice de la case.
    let mut map = vec![vec![0usize; k]; inputs.len()];
    for j in 0..k {
        let s = setting(spec, j);
        let def = &model.params()[j];
        let mk = |initial: f64| Slot {
            name: def.name.clone(),
            initial: s.initial.unwrap_or(initial),
            fixed: s.fixed,
            min: s.min.unwrap_or(def.min),
            max: s.max.unwrap_or(def.max),
        };
        if s.shared {
            // Valeur de départ : moyenne des estimations des courbes.
            let mean = guesses.iter().map(|g| g[j]).sum::<f64>() / inputs.len() as f64;
            slots.push(mk(mean));
            for m in map.iter_mut() {
                m[j] = slots.len() - 1;
            }
        } else {
            for (i, m) in map.iter_mut().enumerate() {
                slots.push(mk(guesses[i][j]));
                m[j] = slots.len() - 1;
            }
        }
    }
    let terms: Vec<Term<'_>> = inputs
        .iter()
        .enumerate()
        .map(|(i, input)| {
            let idx = map[i].clone();
            let x = &input.series.x;
            Term {
                x,
                y: &input.series.y,
                sigma: input.sigma.as_deref(),
                eval: Box::new(move |vals, out| {
                    let p: Vec<f64> = idx.iter().map(|&s| vals[s]).collect();
                    for (o, &xi) in out.iter_mut().zip(x.iter()) {
                        *o = model.eval(&p, xi);
                    }
                }),
            }
        })
        .collect();
    let sol = solve_problem(&slots, &terms, Options::default())?;
    let series = inputs
        .iter()
        .enumerate()
        .map(|(i, _)| FittedSeries {
            values: map[i].iter().map(|&s| sol.values[s]).collect(),
            errors: map[i].iter().map(|&s| sol.errors[s]).collect(),
            points: sol.terms[i].points,
            r_squared: sol.terms[i].r_squared,
            chi2: {
                // χ² de la courbe seule.
                let t = &terms[i];
                let mut f = vec![0.0; t.x.len()];
                (t.eval)(&sol.values, &mut f);
                (0..t.x.len())
                    .map(|r| ((t.y[r] - f[r]) * t.sigma.map_or(1.0, |s| 1.0 / s[r])).powi(2))
                    .sum()
            },
        })
        .collect();
    Ok(FitReport {
        param_names: model.params().iter().map(|p| p.name.clone()).collect(),
        series,
        dof: sol.dof,
        reduced_chi2: sol.reduced_chi2,
        iterations: sol.iterations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(n: usize, a: f64, b: f64) -> Vec<f64> {
        (0..n)
            .map(|i| a + (b - a) * i as f64 / (n - 1) as f64)
            .collect()
    }

    fn input(f: impl Fn(f64) -> f64, x: Vec<f64>) -> FitInput {
        let y = x.iter().map(|&v| f(v)).collect();
        FitInput {
            series: Series::new(x, y),
            sigma: None,
        }
    }

    fn spec(model: Model) -> FitSpec {
        FitSpec {
            model,
            params: vec![],
        }
    }

    #[test]
    fn droite_exacte_par_moindres_carres_lineaires() {
        let i = input(|x| 1.5 - 0.25 * x, grid(20, 0.0, 10.0));
        let r = fit(&[i], &spec(Model::Linear)).unwrap();
        let s = &r.series[0];
        assert!((s.values[0] - 1.5).abs() < 1e-12 && (s.values[1] + 0.25).abs() < 1e-12);
        assert!((s.r_squared - 1.0).abs() < 1e-12);
        assert_eq!(r.dof, 18);
        assert_eq!(r.iterations, 1, "chemin exact, sans itération");
    }

    #[test]
    fn polynome_de_degre_3() {
        let i = input(
            |x| 1.0 - 2.0 * x + 0.5 * x * x + 0.1 * x * x * x,
            grid(40, -3.0, 3.0),
        );
        let r = fit(&[i], &spec(Model::Polynomial { degree: 3 })).unwrap();
        let want = [1.0, -2.0, 0.5, 0.1];
        for (a, b) in r.series[0].values.iter().zip(want) {
            assert!((a - b).abs() < 1e-9, "{a} vs {b}");
        }
    }

    #[test]
    fn logarithme_refuse_les_x_negatifs_puis_ajuste_sans_cheminer_par_le_lineaire() {
        // x ≤ 0 présent : le chemin exact est écarté, Levenberg-Marquardt
        // échoue proprement (modèle non fini au départ).
        let i = input(|x| 2.0 + 3.0 * x.abs().ln(), grid(11, -5.0, 5.0));
        assert!(fit(&[i], &spec(Model::Logarithm)).is_err());
        let ok = input(|x| 2.0 + 3.0 * x.ln(), grid(30, 0.5, 20.0));
        let r = fit(&[ok], &spec(Model::Logarithm)).unwrap();
        assert!((r.series[0].values[1] - 3.0).abs() < 1e-10);
    }

    #[test]
    fn exponentielle_bruitee_dans_les_incertitudes() {
        let x = grid(300, 0.0, 12.0);
        let noise = |i: usize| ((i as f64 * 0.618_033_988_749_895).fract() - 0.5) * 0.1;
        let y: Vec<f64> = x
            .iter()
            .enumerate()
            .map(|(i, v)| 1.0 + 4.0 * (-v / 2.0).exp() + noise(i))
            .collect();
        let i = FitInput {
            series: Series::new(x, y),
            sigma: None,
        };
        let r = fit(&[i], &spec(Model::ExponentialOffset)).unwrap();
        let s = &r.series[0];
        assert!((s.values[2] - 2.0).abs() < 0.1, "tau {}", s.values[2]);
        let e = s.errors[2].unwrap();
        // L'écart au vrai τ est de l'ordre de l'incertitude annoncée.
        assert!(
            (s.values[2] - 2.0).abs() < 6.0 * e,
            "τ={} ±{e}",
            s.values[2]
        );
        assert!(s.r_squared > 0.99);
    }

    #[test]
    fn gaussienne_avec_parametre_fixe_et_bornes() {
        let x = grid(200, 0.0, 20.0);
        let i = input(
            |v| 1.0 + 5.0 * (-4.0 * std::f64::consts::LN_2 * ((v - 8.0) / 3.0).powi(2)).exp(),
            x,
        );
        let mut sp = spec(Model::Gaussian);
        sp.params = vec![
            ParamSetting {
                fixed: true,
                initial: Some(1.0),
                ..Default::default()
            },
            ParamSetting::default(),
            ParamSetting {
                min: Some(5.0),
                max: Some(12.0),
                ..Default::default()
            },
            ParamSetting::default(),
        ];
        let r = fit(&[i], &sp).unwrap();
        let s = &r.series[0];
        assert_eq!(s.values[0], 1.0);
        assert!(s.errors[0].is_none());
        assert!((s.values[1] - 5.0).abs() < 1e-5 && (s.values[2] - 8.0).abs() < 1e-5);
        assert!((s.values[3] - 3.0).abs() < 1e-5);
    }

    #[test]
    fn modele_libre() {
        let x = grid(100, 0.0, 5.0);
        let i = input(|v| 2.0 * (v * 1.3).sin() + 0.5, x);
        let r = fit(
            &[i],
            &FitSpec {
                model: Model::Custom {
                    formula: "a*sin(w*x) + c".into(),
                },
                params: vec![
                    ParamSetting {
                        initial: Some(1.5),
                        ..Default::default()
                    },
                    ParamSetting {
                        initial: Some(1.2),
                        ..Default::default()
                    },
                    ParamSetting::default(),
                ],
            },
        )
        .unwrap();
        let v = &r.series[0].values;
        assert!(
            (v[0] - 2.0).abs() < 1e-6 && (v[1] - 1.3).abs() < 1e-6 && (v[2] - 0.5).abs() < 1e-6
        );
    }

    #[test]
    fn ajustement_global_partage_tau_entre_trois_courbes() {
        let x = grid(120, 0.0, 10.0);
        let amplitudes = [2.0, 5.0, 9.0];
        let inputs: Vec<FitInput> = amplitudes
            .iter()
            .map(|a| input(|v| a * (-v / 3.0).exp(), x.clone()))
            .collect();
        let mut sp = spec(Model::Exponential);
        sp.params = vec![
            ParamSetting::default(),
            ParamSetting {
                shared: true,
                ..Default::default()
            },
        ];
        let r = fit(&inputs, &sp).unwrap();
        for (s, a) in r.series.iter().zip(amplitudes) {
            assert!((s.values[0] - a).abs() < 1e-5, "A={}", s.values[0]);
            assert!((s.values[1] - 3.0).abs() < 1e-5);
        }
        // Un seul τ pour les trois courbes.
        assert_eq!(r.series[0].values[1], r.series[2].values[1]);
        // 360 points, 4 paramètres libres (3 A + 1 τ).
        assert_eq!(r.dof, 356);
    }

    #[test]
    fn le_global_tient_ou_les_courbes_seules_divergent() {
        // Témoin : deux courbes bruitées dont le τ réel est identique ; l'ajustement
        // global a une incertitude sur τ plus petite que chacune seule.
        let x = grid(60, 0.0, 6.0);
        let mk = |a: f64, phase: f64| {
            let y: Vec<f64> = x
                .iter()
                .enumerate()
                .map(|(i, v)| a * (-v / 2.0).exp() + 0.05 * ((i as f64 * phase).fract() - 0.5))
                .collect();
            FitInput {
                series: Series::new(x.clone(), y),
                sigma: None,
            }
        };
        let inputs = [mk(3.0, 0.618_033_988_7), mk(4.0, 0.414_213_562_3)];
        let alone = fit(&inputs, &spec(Model::Exponential)).unwrap();
        let mut sp = spec(Model::Exponential);
        sp.params = vec![
            ParamSetting::default(),
            ParamSetting {
                shared: true,
                ..Default::default()
            },
        ];
        let together = fit(&inputs, &sp).unwrap();
        let e_alone = alone.series[0].errors[1].unwrap();
        let e_together = together.series[0].errors[1].unwrap();
        assert!(e_together < e_alone, "{e_together} vs {e_alone}");
    }

    #[test]
    fn ponderation_par_sigma() {
        let x = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let y = vec![0.0, 1.0, 2.0, 3.0, 30.0];
        let i = FitInput {
            series: Series::new(x, y),
            sigma: Some(vec![0.1, 0.1, 0.1, 0.1, 1e4]),
        };
        let r = fit(&[i], &spec(Model::Linear)).unwrap();
        assert!((r.series[0].values[1] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn sigma_invalide() {
        let i = FitInput {
            series: Series::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0]),
            sigma: Some(vec![1.0, 0.0, 1.0]),
        };
        assert!(matches!(
            fit(&[i], &spec(Model::Linear)),
            Err(AnalysisError::InvalidParameter { .. })
        ));
    }

    #[test]
    fn trop_peu_de_points() {
        let i = input(|x| x, vec![0.0, 1.0]);
        assert!(fit(&[i], &spec(Model::Polynomial { degree: 3 })).is_err());
    }
}
