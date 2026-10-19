//! Déconvolution par une réponse instrumentale mesurée, l'IRF (#190) :
//! **reconvolution** (ajuster un déclin multi-exponentiel convolué par l'IRF) et
//! **déconvolution directe** (Richardson-Lucy par défaut, Wiener en option).
//!
//! Convention de temps, commune aux deux : le signal et l'IRF partagent
//! l'origine de l'axe X (même instrument, même échelle) ; le signal est la
//! réponse d'un système excité à l'instant 0 par l'IRF *décalée de `shift`*.
//! Concrètement `d(tₖ) = Σⱼ hⱼ·u(tₖ − tⱼ)`, où `h` est l'IRF **normalisée à une
//! somme de 1** ré-échantillonnée sur la grille du signal (M15 a A) et `u` la
//! réponse impulsionnelle cherchée. Un signal et une IRF dont les pas
//! diffèrent de plus de 1 % déclenchent un avertissement, pas un refus.

use crate::error::AnalysisError;
use crate::fit::ParamSetting;
use crate::series::{interpolate, Series};
use crate::solver::{solve_problem, Options, Slot, Term};

/// Écart relatif de pas au-delà duquel on avertit (M15 a A).
pub const STEP_TOLERANCE: f64 = 0.01;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DeconvWarnings {
    /// Le pas du signal n'est pas constant (écart > 1 %) : le pas moyen est
    /// utilisé.
    pub non_uniform_signal: bool,
    /// Le pas de l'IRF diffère de celui du signal de plus de 1 %.
    pub irf_step_mismatch: bool,
    /// Points de Y négatifs ramenés à 0 (Richardson-Lucy suppose Y ≥ 0).
    pub negative_clipped: usize,
}

// ── FFT ────────────────────────────────────────────────────────────────

fn fft(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    debug_assert!(n.is_power_of_two());
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = 2.0 * std::f64::consts::PI / len as f64 * if inverse { 1.0 } else { -1.0 };
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
    if inverse {
        for v in re.iter_mut().chain(im.iter_mut()) {
            *v /= n as f64;
        }
    }
}

fn padded_len(n: usize) -> usize {
    (2 * n).next_power_of_two()
}

fn spectrum(a: &[f64], size: usize) -> (Vec<f64>, Vec<f64>) {
    let mut re = vec![0.0; size];
    re[..a.len()].copy_from_slice(a);
    let mut im = vec![0.0; size];
    fft(&mut re, &mut im, false);
    (re, im)
}

/// `(h ⊛ u)ₖ = Σⱼ hⱼ·u(k−j)` pour `k < n`, causal.
pub fn convolve(h: &[f64], u: &[f64]) -> Vec<f64> {
    let n = u.len();
    let size = padded_len(n.max(h.len()));
    let (hr, hi) = spectrum(h, size);
    let (mut ur, mut ui) = spectrum(u, size);
    for k in 0..size {
        let (r, i) = (hr[k] * ur[k] - hi[k] * ui[k], hr[k] * ui[k] + hi[k] * ur[k]);
        ur[k] = r;
        ui[k] = i;
    }
    fft(&mut ur, &mut ui, true);
    ur.truncate(n);
    ur
}

/// Adjoint : `(h ⋆ r)ₖ = Σⱼ hⱼ·r(k+j)`.
fn correlate(h: &[f64], r: &[f64]) -> Vec<f64> {
    let n = r.len();
    let size = padded_len(n.max(h.len()));
    let (hr, hi) = spectrum(h, size);
    let (mut rr, mut ri) = spectrum(r, size);
    for k in 0..size {
        // conj(H)·R
        let (a, b) = (hr[k] * rr[k] + hi[k] * ri[k], hr[k] * ri[k] - hi[k] * rr[k]);
        rr[k] = a;
        ri[k] = b;
    }
    fft(&mut rr, &mut ri, true);
    rr.truncate(n);
    rr
}

// ── IRF sur la grille du signal ────────────────────────────────────────

/// Grille uniforme du calcul. Un signal à pas régulier la porte telle quelle
/// (`identity`) ; un signal troué (ligne écartée, pas variable) est ré-échantillonné
/// par interpolation linéaire au pas médian : la convolution compte en indices,
/// et un indice n'est un instant que sur une grille uniforme.
#[derive(Clone, Copy)]
struct Grid {
    start: f64,
    step: f64,
    n: usize,
    identity: bool,
}

/// Plafond de la grille ré-échantillonnée (nœuds).
const MAX_GRID: usize = 1 << 22;

fn grid_of(x: &[f64], w: &mut DeconvWarnings) -> Result<Grid, AnalysisError> {
    let n = x.len();
    if n < 3 {
        return Err(AnalysisError::NotEnoughPoints {
            found: n,
            needed: 3,
        });
    }
    let mean = (x[n - 1] - x[0]) / (n - 1) as f64;
    if !(mean > 0.0) || x.windows(2).any(|p| p[1] <= p[0]) {
        return Err(AnalysisError::DuplicateX);
    }
    if x.windows(2)
        .all(|p| ((p[1] - p[0]) - mean).abs() <= STEP_TOLERANCE * mean)
    {
        return Ok(Grid {
            start: x[0],
            step: mean,
            n,
            identity: true,
        });
    }
    w.non_uniform_signal = true;
    let mut steps: Vec<f64> = x.windows(2).map(|p| p[1] - p[0]).collect();
    steps.sort_by(f64::total_cmp);
    let step = steps[steps.len() / 2];
    let nodes = ((x[n - 1] - x[0]) / step).round() as usize + 1;
    if nodes > MAX_GRID {
        return Err(AnalysisError::InvalidParameter {
            name: "signal".into(),
        });
    }
    Ok(Grid {
        start: x[0],
        step,
        n: nodes,
        identity: false,
    })
}

impl Grid {
    /// Y du signal sur la grille.
    fn sample(&self, x: &[f64], y: &[f64]) -> Vec<f64> {
        if self.identity {
            return y.to_vec();
        }
        (0..self.n)
            .map(|j| {
                interpolate(
                    x,
                    y,
                    (self.start + j as f64 * self.step).min(x[x.len() - 1]),
                )
                .unwrap_or(f64::NAN)
            })
            .collect()
    }

    /// Valeurs de la grille relues aux X du signal.
    fn read_back(&self, x: &[f64], on_grid: &[f64]) -> Vec<f64> {
        if self.identity {
            return on_grid.to_vec();
        }
        let gx: Vec<f64> = (0..self.n)
            .map(|j| self.start + j as f64 * self.step)
            .collect();
        x.iter()
            .map(|&v| interpolate(&gx, on_grid, v).unwrap_or(f64::NAN))
            .collect()
    }
}

/// L'IRF normalisée (somme 1) aux instants `start + j·step − shift`.
fn kernel(irf: &Series, grid: &Grid, n: usize, shift: f64) -> Result<Vec<f64>, AnalysisError> {
    let mut h: Vec<f64> = (0..n)
        .map(|j| {
            let t = grid.start + j as f64 * grid.step - shift;
            interpolate(&irf.x, &irf.y, t).unwrap_or(0.0).max(0.0)
        })
        .collect();
    let sum: f64 = h.iter().sum();
    if !(sum > 0.0) || !sum.is_finite() {
        return Err(AnalysisError::EmptyResponse);
    }
    for v in &mut h {
        *v /= sum;
    }
    Ok(h)
}

fn irf_step_mismatch(irf: &Series, grid: &Grid) -> bool {
    if irf.len() < 2 {
        return false;
    }
    let step = (irf.x[irf.len() - 1] - irf.x[0]) / (irf.len() - 1) as f64;
    (step - grid.step).abs() > STEP_TOLERANCE * grid.step
}

// ── Déconvolution directe ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DirectMethod {
    RichardsonLucy {
        iterations: usize,
    },
    /// `noise` : rapport bruit/signal `K` du filtre `H*/(|H|²+K)`. `None` :
    /// estimé sur le spectre du signal.
    Wiener {
        noise: Option<f64>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Deconvolved {
    /// Réponse impulsionnelle estimée, un point par point du signal.
    pub values: Vec<f64>,
    pub warnings: DeconvWarnings,
    /// Wiener : le `K` employé (donné ou estimé).
    pub noise_ratio: Option<f64>,
}

pub fn deconvolve(
    signal: &Series,
    irf: &Series,
    shift: f64,
    method: DirectMethod,
) -> Result<Deconvolved, AnalysisError> {
    if irf.is_empty() {
        return Err(AnalysisError::EmptyResponse);
    }
    let sig = signal.sorted();
    let irf = irf.sorted();
    let mut warnings = DeconvWarnings::default();
    let grid = grid_of(&sig.x, &mut warnings)?;
    warnings.irf_step_mismatch = irf_step_mismatch(&irf, &grid);
    let n = grid.n;
    let h = kernel(&irf, &grid, n, shift)?;
    let gy = grid.sample(&sig.x, &sig.y);
    match method {
        DirectMethod::RichardsonLucy { iterations } => {
            let d: Vec<f64> = gy
                .iter()
                .map(|v| {
                    if *v < 0.0 {
                        warnings.negative_clipped += 1;
                        0.0
                    } else {
                        *v
                    }
                })
                .collect();
            let mean = d.iter().sum::<f64>() / n as f64;
            if !(mean > 0.0) {
                return Err(AnalysisError::InvalidParameter {
                    name: "signal".into(),
                });
            }
            let mut u = vec![mean; n];
            for _ in 0..iterations.max(1) {
                let est = convolve(&h, &u);
                let ratio: Vec<f64> = d
                    .iter()
                    .zip(&est)
                    .map(|(d, e)| if *e > 1e-12 * mean { d / e } else { 0.0 })
                    .collect();
                let corr = correlate(&h, &ratio);
                for (ui, c) in u.iter_mut().zip(&corr) {
                    *ui *= c;
                }
            }
            Ok(Deconvolved {
                values: grid.read_back(&sig.x, &u),
                warnings,
                noise_ratio: None,
            })
        }
        DirectMethod::Wiener { noise } => {
            let size = padded_len(n);
            let (dr, di) = spectrum(&gy, size);
            let (hr, hi) = spectrum(&h, size);
            let k = noise.unwrap_or_else(|| estimate_noise_ratio(&dr, &di));
            let mut ur = vec![0.0; size];
            let mut ui = vec![0.0; size];
            for f in 0..size {
                let den = hr[f] * hr[f] + hi[f] * hi[f] + k;
                // D·conj(H)/den
                ur[f] = (dr[f] * hr[f] + di[f] * hi[f]) / den;
                ui[f] = (di[f] * hr[f] - dr[f] * hi[f]) / den;
            }
            fft(&mut ur, &mut ui, true);
            ur.truncate(n);
            Ok(Deconvolved {
                values: grid.read_back(&sig.x, &ur),
                warnings,
                noise_ratio: Some(k),
            })
        }
    }
}

/// `K = P_bruit / P_signal` : puissance moyenne du quart haut du spectre
/// (supposé du bruit) sur celle du dixième bas, bruit retranché.
fn estimate_noise_ratio(re: &[f64], im: &[f64]) -> f64 {
    let size = re.len();
    let half = size / 2;
    let power = |k: usize| re[k] * re[k] + im[k] * im[k];
    let lo_end = (half / 10).max(2);
    let hi_start = half * 3 / 4;
    let p_noise = (hi_start..half).map(power).sum::<f64>() / (half - hi_start).max(1) as f64;
    let p_low = (1..lo_end).map(power).sum::<f64>() / (lo_end - 1).max(1) as f64;
    let p_signal = (p_low - p_noise).max(p_noise * 1e-3).max(f64::MIN_POSITIVE);
    (p_noise / p_signal).clamp(1e-8, 1e2)
}

// ── Reconvolution ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ReconvSpec {
    /// Nombre d'exponentielles (1 à 4).
    pub components: usize,
    /// Réglages dans l'ordre `bkg, A1, tau1, A2, tau2, …, shift`.
    pub params: Vec<ParamSetting>,
    /// Valeur initiale du décalage de l'IRF.
    pub shift_initial: f64,
    /// Ajuster le décalage (sinon il reste fixé à `shift_initial`).
    pub fit_shift: bool,
    /// Plage de X sur laquelle on ajuste (le modèle est calculé sur toute la
    /// courbe, car la convolution lit l'historique).
    pub window: Option<(f64, f64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reconvolution {
    pub names: Vec<String>,
    pub values: Vec<f64>,
    pub errors: Vec<Option<f64>>,
    /// Courbe ajustée (modèle ⊛ IRF), un point par point du signal trié.
    pub fitted: Vec<f64>,
    /// Le signal trié par X croissant sur lequel `fitted` est aligné.
    pub x: Vec<f64>,
    pub points: usize,
    pub r_squared: f64,
    pub reduced_chi2: f64,
    pub dof: usize,
    /// Nombre de paramètres libres que les données ne déterminent pas (#203).
    pub undetermined: usize,
    /// Arrêt sur un plateau, pas sur un minimum (#281).
    pub plateau: bool,
    /// Durée de vie moyenne pondérée par les amplitudes `ΣAτ/ΣA`.
    pub tau_amplitude_weighted: f64,
    /// Durée de vie moyenne pondérée par l'intensité `ΣAτ²/ΣAτ`.
    pub tau_intensity_weighted: f64,
    pub warnings: DeconvWarnings,
}

pub fn reconvolve(
    signal: &Series,
    irf: &Series,
    spec: &ReconvSpec,
) -> Result<Reconvolution, AnalysisError> {
    if !(1..=crate::ops::MAX_COMPONENTS).contains(&spec.components) {
        return Err(AnalysisError::InvalidParameter {
            name: "components".into(),
        });
    }
    if irf.is_empty() {
        return Err(AnalysisError::EmptyResponse);
    }
    let sig = signal.sorted();
    let irf = irf.sorted();
    let mut warnings = DeconvWarnings::default();
    let grid = grid_of(&sig.x, &mut warnings)?;
    warnings.irf_step_mismatch = irf_step_mismatch(&irf, &grid);
    let n = sig.len();
    let nc = spec.components;

    // Indices des points de la fenêtre d'ajustement.
    let (wlo, whi) = spec
        .window
        .map_or((f64::NEG_INFINITY, f64::INFINITY), |(a, b)| {
            (a.min(b), a.max(b))
        });
    let win: Vec<usize> = (0..n)
        .filter(|&i| sig.x[i] >= wlo && sig.x[i] <= whi)
        .collect();
    let wx: Vec<f64> = win.iter().map(|&i| sig.x[i]).collect();
    let wy: Vec<f64> = win.iter().map(|&i| sig.y[i]).collect();

    // Valeurs initiales d'après la décroissance après le maximum.
    let (imax, ymax) =
        sig.y.iter().enumerate().fold(
            (0, f64::NEG_INFINITY),
            |b, (i, v)| if *v > b.1 { (i, *v) } else { b },
        );
    let tail = (n / 20).max(1);
    let bkg0 = sig.y[n - tail..].iter().sum::<f64>() / tail as f64;
    let after = Series::new(sig.x[imax..].to_vec(), sig.y[imax..].to_vec());
    let tau_guess = {
        let m = crate::model::Model::ExponentialOffset.compile()?;
        let g = m.guess(&after);
        if g[2].is_finite() && g[2] > 0.0 {
            g[2]
        } else {
            (sig.x[n - 1] - sig.x[imax]).abs() / 3.0
        }
    };
    let mut names = vec!["bkg".to_string()];
    let mut init = vec![bkg0.max(0.0)];
    for i in 0..nc {
        names.push(format!("A{}", i + 1));
        init.push((ymax - bkg0).max(1e-12) / nc as f64);
        names.push(format!("tau{}", i + 1));
        init.push(tau_guess * (0.4 + 1.2 * i as f64));
    }
    names.push("shift".to_string());
    init.push(spec.shift_initial);

    let span = (sig.x[n - 1] - sig.x[0]).abs();
    let slots: Vec<Slot> = names
        .iter()
        .enumerate()
        .map(|(j, name)| {
            let s = spec.params.get(j).cloned().unwrap_or_default();
            let is_tau = name.starts_with("tau");
            let is_shift = name == "shift";
            Slot {
                name: name.clone(),
                initial: s.initial.unwrap_or(init[j]),
                fixed: s.fixed || (is_shift && !spec.fit_shift),
                min: s.min.unwrap_or(if is_tau {
                    span * 1e-6
                } else {
                    f64::NEG_INFINITY
                }),
                max: s.max.unwrap_or(f64::INFINITY),
            }
        })
        .collect();

    let step = grid.step;
    let gn = grid.n;
    let model = move |v: &[f64]| -> Result<Vec<f64>, AnalysisError> {
        let h = kernel(&irf, &grid, gn, v[1 + 2 * nc])?;
        let mut out = vec![v[0]; gn];
        for i in 0..nc {
            let (a, tau) = (v[1 + 2 * i], v[2 + 2 * i]);
            let decay = (-step / tau).exp();
            let mut c = 0.0;
            for (k, o) in out.iter_mut().enumerate() {
                c = decay * c + h[k];
                *o += a * c;
            }
        }
        Ok(out)
    };
    let term = Term {
        x: &wx,
        y: &wy,
        sigma: None,
        eval: Box::new(|v, out| match model(v) {
            Ok(on_grid) => {
                let full = grid.read_back(&sig.x, &on_grid);
                for (o, &i) in out.iter_mut().zip(&win) {
                    *o = full[i];
                }
            }
            Err(_) => out.iter_mut().for_each(|o| *o = f64::NAN),
        }),
    };
    let sol = solve_problem(&slots, &[term], Options::default())?;
    let fitted = grid.read_back(&sig.x, &model(&sol.values)?);

    let (mut sa, mut sat, mut sat2) = (0.0, 0.0, 0.0);
    for i in 0..nc {
        let (a, t) = (sol.values[1 + 2 * i], sol.values[2 + 2 * i]);
        sa += a;
        sat += a * t;
        sat2 += a * t * t;
    }
    Ok(Reconvolution {
        names,
        values: sol.values,
        errors: sol.errors,
        fitted,
        x: sig.x.clone(),
        points: sol.terms[0].points,
        r_squared: sol.terms[0].r_squared,
        reduced_chi2: sol.reduced_chi2,
        dof: sol.dof,
        undetermined: sol.undetermined.iter().filter(|u| **u).count(),
        plateau: sol.plateau,
        tau_amplitude_weighted: sat / sa,
        tau_intensity_weighted: sat2 / sat,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(n: usize, step: f64) -> Vec<f64> {
        (0..n).map(|i| i as f64 * step).collect()
    }

    /// IRF gaussienne centrée en `c`, largeur σ.
    fn irf_series(n: usize, step: f64, c: f64, sigma: f64) -> Series {
        let x = grid(n, step);
        let y = x
            .iter()
            .map(|t| (-(t - c).powi(2) / (2.0 * sigma * sigma)).exp())
            .collect();
        Series::new(x, y)
    }

    /// Signal = (IRF normalisée) ⊛ Σ Aᵢ·exp(−t/τᵢ) + fond.
    fn synth(irf: &Series, step: f64, a: &[(f64, f64)], bkg: f64) -> Series {
        let n = irf.len();
        let sum: f64 = irf.y.iter().sum();
        let h: Vec<f64> = irf.y.iter().map(|v| v / sum).collect();
        let u: Vec<f64> = (0..n)
            .map(|i| {
                a.iter()
                    .map(|(am, tau)| am * (-(i as f64) * step / tau).exp())
                    .sum()
            })
            .collect();
        let d = convolve(&h, &u);
        Series::new(grid(n, step), d.iter().map(|v| v + bkg).collect())
    }

    #[test]
    fn fft_aller_retour_et_convolution_directe() {
        let h = [0.5, 0.3, 0.2];
        let u = [1.0, 2.0, 3.0, 4.0, 5.0];
        let c = convolve(&h, &u);
        let mut want = vec![0.0; 5];
        for k in 0..5 {
            for j in 0..=k.min(2) {
                want[k] += h[j] * u[k - j];
            }
        }
        for (a, b) in c.iter().zip(&want) {
            assert!((a - b).abs() < 1e-12, "{a} vs {b}");
        }
        // L'adjoint vérifie ⟨h⊛u, r⟩ = ⟨u, h⋆r⟩.
        let r = [0.3, -1.0, 2.0, 0.5, 1.5];
        let lhs: f64 = c.iter().zip(&r).map(|(a, b)| a * b).sum();
        let hr = correlate(&h, &r);
        let rhs: f64 = u.iter().zip(&hr).map(|(a, b)| a * b).sum();
        assert!((lhs - rhs).abs() < 1e-12);
    }

    #[test]
    fn reconvolution_retrouve_un_declin_biexponentiel() {
        let step = 0.05;
        let irf = irf_series(800, step, 2.0, 0.15);
        let signal = synth(&irf, step, &[(100.0, 1.2), (50.0, 4.0)], 3.0);
        let spec = ReconvSpec {
            components: 2,
            params: vec![],
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        };
        let r = reconvolve(&signal, &irf, &spec).unwrap();
        let get = |name: &str| r.values[r.names.iter().position(|n| n == name).unwrap()];
        let mut taus = [get("tau1"), get("tau2")];
        taus.sort_by(f64::total_cmp);
        assert!((taus[0] - 1.2).abs() < 1e-3, "tau {taus:?}");
        assert!((taus[1] - 4.0).abs() < 1e-3);
        assert!((get("bkg") - 3.0).abs() < 1e-3);
        assert!(r.r_squared > 1.0 - 1e-9);
        assert!(!r.warnings.irf_step_mismatch && !r.warnings.non_uniform_signal);
        // Moyenne pondérée par les amplitudes : (100·1.2 + 50·4)/150.
        assert!((r.tau_amplitude_weighted - 320.0 / 150.0).abs() < 1e-2);
    }

    #[test]
    fn le_decalage_est_ajuste_quand_la_case_est_cochee() {
        let step = 0.05;
        let irf = irf_series(600, step, 2.0, 0.15);
        // Signal fabriqué avec une IRF décalée de +0,3.
        let shifted = irf_series(600, step, 2.3, 0.15);
        let signal = synth(&shifted, step, &[(80.0, 2.0)], 1.0);
        let mk = |fit_shift: bool| ReconvSpec {
            components: 1,
            params: vec![],
            shift_initial: 0.0,
            fit_shift,
            window: None,
        };
        let fixed = reconvolve(&signal, &irf, &mk(false)).unwrap();
        let free = reconvolve(&signal, &irf, &mk(true)).unwrap();
        let shift = |r: &Reconvolution| *r.values.last().unwrap();
        assert_eq!(shift(&fixed), 0.0);
        assert!((shift(&free) - 0.3).abs() < 5e-3, "shift {}", shift(&free));
        // Témoin : décalage fixé à tort = moins bon ajustement.
        assert!(free.r_squared > fixed.r_squared);
        assert!(fixed.errors.last().unwrap().is_none());
    }

    #[test]
    fn la_fenetre_n_ecarte_pas_l_historique_de_la_convolution() {
        let step = 0.05;
        let irf = irf_series(600, step, 2.0, 0.15);
        let signal = synth(&irf, step, &[(60.0, 2.5)], 0.0);
        let spec = ReconvSpec {
            components: 1,
            params: vec![],
            shift_initial: 0.0,
            fit_shift: false,
            window: Some((4.0, 25.0)),
        };
        let r = reconvolve(&signal, &irf, &spec).unwrap();
        assert!((r.values[2] - 2.5).abs() < 1e-3, "tau {}", r.values[2]);
        // Fenêtre [4, 25] au pas 0,05 : 421 points ajustés sur les 600.
        assert!((420..=422).contains(&r.points), "{} points", r.points);
        assert_eq!(r.fitted.len(), signal.len());
    }

    #[test]
    fn avertissement_quand_les_pas_different() {
        let step = 0.05;
        let irf = irf_series(400, step * 1.5, 2.0, 0.2);
        let signal = synth(&irf_series(400, step, 2.0, 0.15), step, &[(10.0, 2.0)], 0.0);
        let spec = ReconvSpec {
            components: 1,
            params: vec![],
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        };
        let r = reconvolve(&signal, &irf, &spec).unwrap();
        assert!(r.warnings.irf_step_mismatch);
    }

    #[test]
    fn richardson_lucy_resserre_le_declin() {
        let step = 0.05;
        let irf = irf_series(512, step, 1.0, 0.3);
        let signal = synth(&irf, step, &[(100.0, 2.0)], 0.0);
        let r = deconvolve(
            &signal,
            &irf,
            0.0,
            DirectMethod::RichardsonLucy { iterations: 200 },
        )
        .unwrap();
        // L'estimée suit exp(−t/2) : ratio à t=2 vs t=0 proche de e⁻¹.
        let u = &r.values;
        let (i0, i1) = (0, (2.0 / step) as usize);
        let ratio = u[i1] / u[i0];
        assert!((ratio - (-1.0_f64).exp()).abs() < 0.08, "ratio {ratio}");
        assert!(u.iter().all(|v| *v >= 0.0), "positivité de Richardson-Lucy");
        assert!(r.noise_ratio.is_none());
    }

    #[test]
    fn plus_d_iterations_rapprochent_de_la_verite() {
        let step = 0.05;
        let irf = irf_series(512, step, 1.0, 0.3);
        let signal = synth(&irf, step, &[(100.0, 2.0)], 0.0);
        let err = |it: usize| {
            let r = deconvolve(
                &signal,
                &irf,
                0.0,
                DirectMethod::RichardsonLucy { iterations: it },
            )
            .unwrap();
            (0..200)
                .map(|i| (r.values[i] - 100.0 * (-(i as f64) * step / 2.0).exp()).powi(2))
                .sum::<f64>()
        };
        assert!(err(200) < err(5), "{} vs {}", err(200), err(5));
    }

    #[test]
    fn wiener_estime_son_niveau_de_bruit_et_accepte_celui_de_l_utilisateur() {
        let step = 0.05;
        let irf = irf_series(512, step, 1.0, 0.3);
        let signal = synth(&irf, step, &[(100.0, 2.0)], 0.0);
        let auto = deconvolve(&signal, &irf, 0.0, DirectMethod::Wiener { noise: None }).unwrap();
        let k = auto.noise_ratio.unwrap();
        assert!(k > 0.0 && k <= 1e2);
        let fixed = deconvolve(
            &signal,
            &irf,
            0.0,
            DirectMethod::Wiener { noise: Some(1e-4) },
        )
        .unwrap();
        assert_eq!(fixed.noise_ratio, Some(1e-4));
        // Avec peu de bruit, Wiener décroît comme le vrai déclin (le front de
        // montée en t = 0 est lissé : on compare deux points après lui).
        let u = &fixed.values;
        let (i0, i1) = ((1.0 / step) as usize, (3.0 / step) as usize);
        assert!(
            (u[i1] / u[i0] - (-1.0_f64).exp()).abs() < 0.1,
            "{}",
            u[i1] / u[i0]
        );
    }

    #[test]
    fn irf_nulle_ou_signal_negatif() {
        let step = 0.05;
        let signal = Series::new(grid(100, step), vec![1.0; 100]);
        let zero = Series::new(grid(100, step), vec![0.0; 100]);
        assert_eq!(
            deconvolve(&signal, &zero, 0.0, DirectMethod::Wiener { noise: None }).unwrap_err(),
            AnalysisError::EmptyResponse
        );
        assert_eq!(
            deconvolve(
                &signal,
                &Series::default(),
                0.0,
                DirectMethod::Wiener { noise: None }
            )
            .unwrap_err(),
            AnalysisError::EmptyResponse
        );
        let irf = irf_series(100, step, 1.0, 0.2);
        let neg = Series::new(grid(100, step), vec![-1.0; 100]);
        assert!(matches!(
            deconvolve(
                &neg,
                &irf,
                0.0,
                DirectMethod::RichardsonLucy { iterations: 5 }
            ),
            Err(AnalysisError::InvalidParameter { .. })
        ));
    }

    #[test]
    fn composantes_hors_de_1_a_4() {
        let irf = irf_series(100, 0.05, 1.0, 0.2);
        let s = synth(&irf, 0.05, &[(10.0, 1.0)], 0.0);
        let spec = ReconvSpec {
            components: 5,
            params: vec![],
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        };
        assert!(matches!(
            reconvolve(&s, &irf, &spec),
            Err(AnalysisError::InvalidParameter { .. })
        ));
    }
}
