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
    /// L'IRF est coupée au-dessus de [`IRF_TRUNCATION_FRACTION`] de son pic au
    /// décalage retenu (C2, #313) : la queue manquante vaut 0 puis le noyau est
    /// renormalisé, ce qui déforme les durées de vie sans que rien le dise.
    pub irf_truncated: bool,
}

/// Part du pic de l'IRF au-dessus de laquelle une coupure (fin de l'IRF, ou
/// points hors de la fenêtre du signal) est signalée : 1 %.
pub const IRF_TRUNCATION_FRACTION: f64 = 0.01;

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
        let (wr, wi) = (libm::cos(ang), libm::sin(ang));
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
    // La grille doit couvrir le dernier X : arrondi supérieur (à l'erreur de
    // flottant près, pour qu'un dernier nœud qui tombe pile ne gagne pas un
    // nœud de plus), sinon la relecture hors grille donnerait NaN.
    let ratio = (x[n - 1] - x[0]) / step;
    let nodes = (ratio - 1e-9 * ratio.max(1.0)).ceil().max(0.0) as usize + 1;
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

/// L'IRF est-elle coupée au décalage `shift` ? Oui si un de ses points hors de
/// la fenêtre `[start − shift, start + (n−1)·step − shift]` du noyau dépasse
/// 1 % du pic, ou si un de ses bords (premier ou dernier point) le dépasse : au
/// delà du bord, l'interpolation lit 0 (C2, #313). Le noyau renormalisé reste.
fn irf_truncated(irf: &Series, grid: &Grid, shift: f64) -> bool {
    let peak = irf.y.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !(peak > 0.0) || irf.is_empty() {
        return false;
    }
    let limit = IRF_TRUNCATION_FRACTION * peak;
    let first = grid.start - shift;
    let last = grid.start + (grid.n.saturating_sub(1)) as f64 * grid.step - shift;
    let outside = irf
        .x
        .iter()
        .zip(&irf.y)
        .any(|(&x, &y)| (x < first || x > last) && y > limit);
    outside || irf.y[0] > limit || irf.y[irf.len() - 1] > limit
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
    warnings.irf_truncated = irf_truncated(&irf, &grid, shift);
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
    /// Arrêt sur un plateau (#281) ou solveur bloqué sans pas accepté (C1, #313) :
    /// pas sur un minimum.
    pub plateau: bool,
    /// Durée de vie moyenne pondérée par les amplitudes `ΣAτ/ΣA`.
    pub tau_amplitude_weighted: f64,
    /// Durée de vie moyenne pondérée par l'intensité `ΣAτ²/ΣAτ`.
    pub tau_intensity_weighted: f64,
    pub warnings: DeconvWarnings,
}

/// `num / den`, `NaN` si le dénominateur est nul (jamais ±∞, C5, #313) : des
/// amplitudes de somme nulle ne définissent pas de durée de vie moyenne.
fn ratio_or_nan(num: f64, den: f64) -> f64 {
    if den == 0.0 {
        f64::NAN
    } else {
        num / den
    }
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
    let model = |v: &[f64]| -> Result<Vec<f64>, AnalysisError> {
        let h = kernel(&irf, &grid, gn, v[1 + 2 * nc])?;
        let mut out = vec![v[0]; gn];
        for i in 0..nc {
            let (a, tau) = (v[1 + 2 * i], v[2 + 2 * i]);
            let decay = libm::exp(-step / tau);
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

    warnings.irf_truncated = irf_truncated(&irf, &grid, sol.values[1 + 2 * nc]);
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
        plateau: sol.plateau || sol.stalled,
        tau_amplitude_weighted: ratio_or_nan(sat, sa),
        tau_intensity_weighted: ratio_or_nan(sat2, sat),
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fit::ParamSetting;

    fn grid(n: usize, step: f64) -> Vec<f64> {
        (0..n).map(|i| i as f64 * step).collect()
    }

    /// IRF gaussienne centrée en `c`, largeur σ.
    fn irf_series(n: usize, step: f64, c: f64, sigma: f64) -> Series {
        let x = grid(n, step);
        let y = x
            .iter()
            .map(|t| libm::exp(-(t - c).powi(2) / (2.0 * sigma * sigma)))
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
                    .map(|(am, tau)| am * libm::exp(-(i as f64) * step / tau))
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
        assert!(!r.warnings.irf_truncated);
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
        assert!((ratio - libm::exp(-1.0_f64)).abs() < 0.08, "ratio {ratio}");
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
                .map(|i| (r.values[i] - 100.0 * libm::exp(-(i as f64) * step / 2.0)).powi(2))
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
            (u[i1] / u[i0] - libm::exp(-1.0_f64)).abs() < 0.1,
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

    #[test]
    fn grille_non_uniforme_couvre_le_dernier_x() {
        // Dernier X (4,4) entre deux nœuds du pas médian : le dernier point
        // ne doit être ni NaN (directe) ni NonFiniteModel (réconvolution).
        let irf = irf_series(40, 0.1, 1.0, 0.3);
        let x = vec![0.0, 1.0, 2.0, 3.0, 4.4];
        let signal = Series::new(x.clone(), vec![1.0, 2.0, 3.0, 2.0, 1.0]);
        for method in [
            DirectMethod::RichardsonLucy { iterations: 20 },
            DirectMethod::Wiener { noise: Some(0.01) },
        ] {
            let d = deconvolve(&signal, &irf, 0.0, method).unwrap();
            assert!(d.warnings.non_uniform_signal);
            assert!(
                d.values.iter().all(|v| v.is_finite()),
                "{method:?}: {:?}",
                d.values
            );
        }
        let signal = Series::new(vec![0.0, 1.0, 2.0, 3.0, 4.2], vec![1.0, 2.0, 3.0, 2.0, 1.0]);
        let spec = ReconvSpec {
            components: 1,
            params: vec![],
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        };
        let r = reconvolve(&signal, &irf, &spec);
        assert!(
            !matches!(r, Err(AnalysisError::NonFiniteModel)),
            "{:?}",
            r.err()
        );
    }

    // ── #284 : la grille non uniforme couvre le dernier X ──────────────

    fn warns() -> DeconvWarnings {
        DeconvWarnings::default()
    }

    /// Nombre de nœuds de la grille d'un signal non uniforme `x`.
    fn noeuds(x: &[f64]) -> usize {
        grid_of(x, &mut warns()).unwrap().n
    }

    /// `[0, 1, 2, 3, 4 + f]` : pas médian 1, dernier pas `1 + f`.
    fn x_fraction(f: f64) -> Vec<f64> {
        vec![0.0, 1.0, 2.0, 3.0, 4.0 + f]
    }

    /// Pas de 0,1 sur [0 ; `fin`] avec un trou (le point 5 est écarté) : pas médian 0,1.
    fn pas_dixieme_avec_trou(fin: f64) -> Vec<f64> {
        let mut x: Vec<f64> = (0..40)
            .filter(|&i| i != 5)
            .map(|i| i as f64 * 0.1)
            .collect();
        x.push(fin);
        x
    }

    #[test]
    fn grille_etendue_pile_entiere_ne_gagne_pas_de_noeud() {
        // 4,0 / 0,1 = 40 pas : 41 nœuds, pas 42, malgré l'erreur de flottant.
        let x = pas_dixieme_avec_trou(4.0);
        let mut w = warns();
        let g = grid_of(&x, &mut w).unwrap();
        assert!(w.non_uniform_signal && !g.identity);
        assert!((g.step - 0.1).abs() < 1e-12, "{}", g.step);
        assert_eq!(g.n, 41);
        // Pas de 1 et dernier X entier.
        assert_eq!(noeuds(&[0.0, 1.0, 2.0, 4.0, 6.0, 7.0, 8.0]), 9);
    }

    #[test]
    fn grille_tolerance_relative_autour_du_pile() {
        // Juste au-dessus, sous la tolérance 1e-9 : toujours 41 nœuds.
        assert_eq!(noeuds(&pas_dixieme_avec_trou(4.0 * (1.0 + 1e-12))), 41);
        // Juste au-dessus, au-delà de la tolérance : le dernier X n'est plus couvert par 41.
        assert_eq!(noeuds(&pas_dixieme_avec_trou(4.0 * (1.0 + 1e-6))), 42);
        // Juste au-dessous : 41 (le dernier nœud 4,0 couvre 4,0·(1 − 1e-6)).
        assert_eq!(noeuds(&pas_dixieme_avec_trou(4.0 * (1.0 - 1e-6))), 41);
        assert_eq!(noeuds(&pas_dixieme_avec_trou(4.0 * (1.0 - 1e-12))), 41);
        // Une tolérance trop large (1e-3) donnerait 41 ici : le pas du dessus tranche.
        assert_eq!(noeuds(&pas_dixieme_avec_trou(4.0 * (1.0 + 1e-4))), 42);
    }

    #[test]
    fn grille_fraction_sous_a_et_au_dessus_de_la_demie() {
        // Le dernier nœud est toujours le premier ≥ dernier X : 6 nœuds (0…5).
        for f in [0.1, 0.4, 0.5, 0.6, 0.999] {
            assert_eq!(noeuds(&x_fraction(f)), 6, "fraction {f}");
        }
        // Pile entier (dernier X = 5) : 6 nœuds, pas 7.
        assert_eq!(noeuds(&x_fraction(1.0)), 6);
        // Dernier pas plus court que le pas médian : arrondi aussi vers le haut.
        for (last, n) in [(3.4, 5), (3.5, 5), (3.6, 5), (3.01, 5)] {
            assert_eq!(noeuds(&[0.0, 1.0, 2.0, 3.0, last]), n, "dernier X {last}");
        }
        // Le dernier nœud ≥ dernier X et < dernier X + 1 pas, sur toute la plage.
        for k in 1..=99 {
            let x = x_fraction(k as f64 / 100.0);
            let g = grid_of(&x, &mut warns()).unwrap();
            let last_node = g.start + (g.n - 1) as f64 * g.step;
            assert!(last_node >= x[4] - 1e-12, "fraction {k}%");
            assert!(last_node < x[4] + g.step, "fraction {k}%");
        }
    }

    #[test]
    fn grille_de_trois_points_non_uniformes() {
        // Pas 1 et 1,5 : le médian est le plus grand (indice 1 sur 2) ; 2,5 / 1,5 → 2 pas.
        let g = grid_of(&[0.0, 1.0, 2.5], &mut warns()).unwrap();
        assert!(!g.identity);
        assert_eq!((g.step, g.n), (1.5, 3));
        // Deux points : refus.
        assert_eq!(
            grid_of(&[0.0, 1.0], &mut warns()).err().unwrap(),
            AnalysisError::NotEnoughPoints {
                found: 2,
                needed: 3
            }
        );
        // Des signaux de trois points se déconvoluent sans NaN.
        let irf = irf_series(40, 0.1, 1.0, 0.3);
        let signal = Series::new(vec![0.0, 1.0, 2.5], vec![1.0, 3.0, 1.0]);
        for method in [
            DirectMethod::RichardsonLucy { iterations: 5 },
            DirectMethod::Wiener { noise: Some(0.01) },
        ] {
            let d = deconvolve(&signal, &irf, 0.0, method).unwrap();
            assert_eq!(d.values.len(), 3);
            assert!(d.values.iter().all(|v| v.is_finite()), "{:?}", d.values);
        }
    }

    #[test]
    fn grille_premier_et_dernier_x_relus_sans_nan() {
        // read_back d'une fonction affine des nœuds rend exactement l'affine aux X,
        // dernier X (4,4) compris ; le premier X est le nœud 0.
        let x = x_fraction(0.4);
        let g = grid_of(&x, &mut warns()).unwrap();
        let on_grid: Vec<f64> = (0..g.n)
            .map(|j| 2.0 * (g.start + j as f64 * g.step) + 1.0)
            .collect();
        let back = g.read_back(&x, &on_grid);
        for (b, v) in back.iter().zip(&x) {
            assert!((b - (2.0 * v + 1.0)).abs() < 1e-12, "x={v} : {b}");
        }
        assert_eq!(back[0], 1.0);
        assert!((back[4] - 9.8).abs() < 1e-12);
    }

    #[test]
    fn grille_sample_plafonne_au_dernier_x() {
        // Nœuds 0…5, dernier X 4,4 : le nœud 5 est lu à 4,4 (pas d'extrapolation, pas de NaN).
        let x = x_fraction(0.4);
        let y: Vec<f64> = x.iter().map(|v| 3.0 * v + 2.0).collect();
        let g = grid_of(&x, &mut warns()).unwrap();
        let s = g.sample(&x, &y);
        assert_eq!(s.len(), 6);
        for (j, v) in s.iter().enumerate().take(5) {
            assert!((v - (3.0 * j as f64 + 2.0)).abs() < 1e-12, "nœud {j}");
        }
        assert!((s[5] - (3.0 * 4.4 + 2.0)).abs() < 1e-12, "{}", s[5]);
        // Régulier (identity) : copie telle quelle.
        let xr = grid(6, 0.5);
        let yr = vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0];
        let gr = grid_of(&xr, &mut warns()).unwrap();
        assert!(gr.identity);
        assert_eq!(gr.sample(&xr, &yr), yr);
        assert_eq!(gr.read_back(&xr, &yr), yr);
    }

    #[test]
    fn deconvolution_au_dernier_x_egale_la_grille_reguliere_equivalente() {
        // Signal non uniforme [0,1,2,3,4,4] → dernier X 4,4 ; son équivalent régulier
        // porte le signal rééchantillonné sur les nœuds 0…5 (le nœud 5 plafonné
        // à 4,4). Les valeurs relues doivent en être l'interpolation linéaire.
        let irf = irf_series(40, 0.1, 1.0, 0.3);
        let x = x_fraction(0.4);
        let y = vec![1.0, 2.0, 3.0, 2.0, 1.0];
        let signal = Series::new(x.clone(), y.clone());
        let interp = |t: f64| interpolate(&x, &y, t.min(4.4)).unwrap();
        let reg_y: Vec<f64> = (0..6).map(|j| interp(j as f64)).collect();
        let regulier = Series::new((0..6).map(|j| j as f64).collect(), reg_y);
        for method in [
            DirectMethod::RichardsonLucy { iterations: 30 },
            DirectMethod::Wiener { noise: Some(0.01) },
        ] {
            let d = deconvolve(&signal, &irf, 0.0, method).unwrap();
            let r = deconvolve(&regulier, &irf, 0.0, method).unwrap();
            assert!(!r.warnings.non_uniform_signal);
            let u = &r.values;
            let attendu = [u[0], u[1], u[2], u[3], u[4] + 0.4 * (u[5] - u[4])];
            for (i, (a, e)) in d.values.iter().zip(&attendu).enumerate() {
                assert!(a.is_finite(), "{method:?} point {i}");
                assert!(
                    (a - e).abs() <= 1e-9 * e.abs().max(1.0),
                    "{method:?} point {i} : {a} contre {e}"
                );
            }
        }
    }

    #[test]
    fn reconvolution_non_uniforme_relit_le_dernier_x() {
        // Signal exponentiel, dernier X entre deux nœuds : le modèle relu aux X du
        // signal est fini partout, dernier point compris.
        let irf = irf_series(40, 0.1, 1.0, 0.3);
        let mut x: Vec<f64> = (0..=40)
            .filter(|i| i % 7 != 3)
            .map(|i| i as f64 * 0.1)
            .collect();
        x.push(4.04);
        let y: Vec<f64> = x.iter().map(|t| 0.2 + 3.0 * libm::exp(-t / 1.2)).collect();
        let signal = Series::new(x.clone(), y);
        let spec = ReconvSpec {
            components: 1,
            params: vec![],
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        };
        let r = reconvolve(&signal, &irf, &spec).unwrap();
        assert!(r.warnings.non_uniform_signal);
        assert_eq!(r.fitted.len(), x.len());
        assert!(r.fitted.iter().all(|v| v.is_finite()), "{:?}", r.fitted);
        // Régulier : pas d'avertissement.
        let reg = synth(&irf_series(400, 0.05, 2.0, 0.15), 0.05, &[(10.0, 2.0)], 0.0);
        let r = reconvolve(&reg, &irf, &spec).unwrap();
        assert!(!r.warnings.non_uniform_signal);
    }

    #[test]
    fn grille_reguliere_identity_est_inchangee() {
        // Un pas régulier (à 1 % près) ne rééchantillonne rien : mêmes valeurs bit à
        // bit qu'un signal exactement régulier de mêmes extrémités.
        let irf = irf_series(40, 0.1, 1.0, 0.3);
        let n = 30;
        let exact: Vec<f64> = (0..n).map(|i| i as f64 * 0.1).collect();
        let mut jitter = exact.clone();
        for (i, v) in jitter.iter_mut().enumerate().take(n - 1).skip(1) {
            *v += if i % 2 == 0 { 0.004 } else { -0.004 } * 0.1;
        }
        let y: Vec<f64> = (0..n).map(|i| 1.0 + ((i * 7) % 5) as f64).collect();
        let a = Series::new(exact.clone(), y.clone());
        let b = Series::new(jitter.clone(), y);
        let g = grid_of(&jitter, &mut warns()).unwrap();
        assert!(g.identity);
        assert_eq!(g.n, n);
        for method in [
            DirectMethod::RichardsonLucy { iterations: 15 },
            DirectMethod::Wiener { noise: None },
        ] {
            let da = deconvolve(&a, &irf, 0.0, method).unwrap();
            let db = deconvolve(&b, &irf, 0.0, method).unwrap();
            assert!(!da.warnings.non_uniform_signal && !db.warnings.non_uniform_signal);
            assert_eq!(da.values, db.values, "{method:?}");
            assert_eq!(da.noise_ratio, db.noise_ratio);
        }
    }

    #[test]
    fn avertissement_pas_uniforme_seulement_en_non_uniforme() {
        let irf = irf_series(40, 0.1, 1.0, 0.3);
        let m = DirectMethod::Wiener { noise: Some(0.01) };
        let y = vec![1.0, 2.0, 3.0, 2.0, 1.0];
        let nu = deconvolve(&Series::new(x_fraction(0.4), y.clone()), &irf, 0.0, m).unwrap();
        assert!(nu.warnings.non_uniform_signal);
        let reg = deconvolve(&Series::new(grid(5, 1.0), y), &irf, 0.0, m).unwrap();
        assert!(!reg.warnings.non_uniform_signal);
    }

    #[test]
    fn pas_de_l_irf_plus_grand_que_l_etendue_du_signal() {
        // IRF à pas 10 sur 20 unités, signal sur 4,4 : refus propre ou valeurs finies,
        // jamais NaN ; l'écart de pas est signalé.
        let irf = Series::new(vec![0.0, 10.0, 20.0], vec![1.0, 0.5, 0.1]);
        let signal = Series::new(x_fraction(0.4), vec![1.0, 2.0, 3.0, 2.0, 1.0]);
        for method in [
            DirectMethod::RichardsonLucy { iterations: 10 },
            DirectMethod::Wiener { noise: Some(0.01) },
        ] {
            let d = deconvolve(&signal, &irf, 0.0, method).unwrap();
            assert!(d.warnings.irf_step_mismatch && d.warnings.non_uniform_signal);
            assert!(d.values.iter().all(|v| v.is_finite()), "{:?}", d.values);
        }
    }

    #[test]
    fn grilles_non_uniformes_aleatoires_toutes_finies() {
        // Générateur congruentiel figé : mêmes grilles à chaque exécution.
        let mut etat: u64 = 0x2545_F491_4F6C_DD1D;
        let mut alea = move || {
            etat = etat
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (etat >> 11) as f64 / (1u64 << 53) as f64
        };
        let irf = irf_series(40, 0.1, 1.0, 0.3);
        let mut non_uniformes = 0;
        for cas in 0..25 {
            let n = 4 + (alea() * 30.0) as usize;
            let mut t = 0.0;
            let mut x = vec![t];
            for _ in 1..n {
                t += 0.03 + 0.17 * alea();
                x.push(t);
            }
            let y: Vec<f64> = x
                .iter()
                .map(|t| 0.5 + libm::exp(-t / 1.5) + 0.1 * alea())
                .collect();
            let signal = Series::new(x.clone(), y);
            let g = grid_of(&x, &mut warns()).unwrap();
            let dernier = g.start + (g.n - 1) as f64 * g.step;
            assert!(dernier >= x[n - 1] - 1e-9, "cas {cas}");
            assert!(dernier < x[n - 1] + g.step, "cas {cas}");
            non_uniformes += usize::from(!g.identity);
            for method in [
                DirectMethod::RichardsonLucy { iterations: 10 },
                DirectMethod::Wiener { noise: Some(0.01) },
            ] {
                let d = deconvolve(&signal, &irf, 0.0, method).unwrap();
                assert_eq!(d.values.len(), n);
                assert!(
                    d.values.iter().all(|v| v.is_finite()),
                    "cas {cas} {method:?} : {:?}",
                    d.values
                );
            }
        }
        assert!(non_uniformes >= 20, "{non_uniformes} grilles non uniformes");
    }

    /// IRF de pic 1 en x = 3 (x = 0..=9), dont les deux bords valent `first` et
    /// `last`.
    fn irf_with_edges(first: f64, last: f64) -> Series {
        let mut y = vec![0.0, 0.4, 0.8, 1.0, 0.8, 0.4, 0.2, 0.1, 0.05, 0.0];
        y[0] = first;
        y[9] = last;
        Series::new(grid(10, 1.0), y)
    }

    fn signal_grid() -> Grid {
        Grid {
            start: 0.0,
            step: 1.0,
            n: 100,
            identity: true,
        }
    }

    /// C2 (#313) : le noyau est coupé si l'IRF s'arrête au-dessus de 1 % de son
    /// pic. 5 % et 1,1 % avertissent, 0,9 % et 0,5 % non.
    #[test]
    fn une_irf_coupee_au_dessus_d_un_pour_cent_de_son_pic_est_signalee() {
        let g = signal_grid();
        for (edge, want) in [(0.05, true), (0.011, true), (0.009, false), (0.005, false)] {
            assert_eq!(
                irf_truncated(&irf_with_edges(0.0, edge), &g, 0.0),
                want,
                "bord de fin {edge}"
            );
            assert_eq!(
                irf_truncated(&irf_with_edges(edge, 0.0), &g, 0.0),
                want,
                "bord de début {edge}"
            );
        }
    }

    /// Un point de l'IRF hors de la fenêtre du noyau, au-dessus de 1 %, est
    /// perdu ; sous 1 %, non. Le décalage déplace la fenêtre.
    #[test]
    fn un_point_d_irf_hors_de_la_fenetre_est_signale_selon_le_decalage() {
        let g = signal_grid();
        let irf = irf_with_edges(0.0, 0.0);
        assert!(!irf_truncated(&irf, &g, 0.0));
        // Décalage −2,5 : la fenêtre commence en 2,5 ; le point x = 2 (0,8) est hors.
        assert!(irf_truncated(&irf, &g, -2.5));
        // Décalage −9 : seul x = 9 (0) reste ; x = 8 (0,05) est au-dessus de 1 %.
        assert!(irf_truncated(&irf, &g, -9.0));
        // Décalage positif : la fenêtre s'étend avant 0 et après 99 − shift.
        assert!(irf_truncated(&irf, &g, 95.0));
    }

    #[test]
    fn la_deconvolution_directe_dit_une_irf_coupee_et_garde_la_renormalisation() {
        let step = 1.0;
        let irf = irf_with_edges(0.0, 0.05);
        let signal = Series::new(
            grid(64, step),
            (0..64).map(|i| libm::exp(-(i as f64) / 8.0)).collect(),
        );
        let m = DirectMethod::Wiener { noise: Some(0.01) };
        let d = deconvolve(&signal, &irf, 0.0, m).unwrap();
        assert!(d.warnings.irf_truncated);
        let ok = deconvolve(&signal, &irf_with_edges(0.0, 0.0), 0.0, m).unwrap();
        assert!(!ok.warnings.irf_truncated);
    }

    /// C5 (#313) : des amplitudes de somme nulle ne définissent pas de durée
    /// de vie moyenne : NaN, jamais ±∞.
    #[test]
    fn un_denominateur_nul_donne_nan_et_pas_l_infini() {
        assert!(ratio_or_nan(3.0, 0.0).is_nan());
        assert!(ratio_or_nan(-3.0, 0.0).is_nan());
        assert_eq!(ratio_or_nan(6.0, 3.0), 2.0);
    }

    /// C5 de bout en bout : deux amplitudes fixées à +5 et −5 (somme nulle) ne
    /// donnent aucune durée de vie moyenne.
    #[test]
    fn des_amplitudes_de_somme_nulle_donnent_des_durees_moyennes_nan() {
        let step = 0.05;
        let irf = irf_series(400, step, 1.0, 0.1);
        let signal = synth(&irf, step, &[(10.0, 1.0)], 1.0);
        let fixed = |v: f64| ParamSetting {
            initial: Some(v),
            fixed: true,
            ..ParamSetting::default()
        };
        let spec = ReconvSpec {
            components: 2,
            // bkg, A1, tau1, A2, tau2, shift
            params: vec![
                ParamSetting::default(),
                fixed(5.0),
                fixed(1.0),
                fixed(-5.0),
                fixed(2.0),
                fixed(0.0),
            ],
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        };
        let r = reconvolve(&signal, &irf, &spec).unwrap();
        assert!(r.tau_amplitude_weighted.is_nan(), "{r:?}");
    }
}
