//! Détection de pics et décomposition en somme de pics (#189) : gaussiennes,
//! lorentziennes ou profils de Voigt, avec un fond constant ou linéaire ajusté
//! **en même temps** que les pics (M14 b).
//!
//! Détection : maxima locaux filtrés par **proéminence relative** (hauteur
//! au-dessus du plus haut des deux creux voisins, rapportée à l'étendue de Y)
//! et plafonnés à un nombre maximal de pics (M14 a C).
//!
//! La décomposition est un ajustement ordinaire du moteur [`crate::solver`] :
//! les largeurs liées (même case pour tous les pics) et les positions bornées
//! (centre ± fenêtre) sont des réglages de cases, pas du code propre aux pics.

use crate::error::AnalysisError;
use crate::model::peak_guess;
use crate::series::Series;
use crate::solver::{solve_problem, Options, Slot, Term};
use crate::voigt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeakShape {
    Gaussian,
    Lorentzian,
    Voigt,
}

impl PeakShape {
    /// Paramètres d'un pic : hauteur, centre, puis une ou deux largeurs.
    pub fn width_count(self) -> usize {
        match self {
            PeakShape::Voigt => 2,
            _ => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeakBackground {
    None,
    Constant,
    Linear,
}

impl PeakBackground {
    fn param_count(self) -> usize {
        match self {
            PeakBackground::None => 0,
            PeakBackground::Constant => 1,
            PeakBackground::Linear => 2,
        }
    }
}

/// Un maximum local trouvé par [`detect`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectedPeak {
    pub index: usize,
    pub x: f64,
    pub height: f64,
    pub prominence: f64,
}

/// Maxima locaux de `y` dont la proéminence relative atteint `min_relative`,
/// au plus `max_peaks`, rangés par X croissant. `y` est lu dans l'ordre donné.
pub fn detect(series: &Series, min_relative: f64, max_peaks: usize) -> Vec<DetectedPeak> {
    let n = series.len();
    if n < 3 || max_peaks == 0 {
        return Vec::new();
    }
    let y = &series.y;
    let (lo, hi) = y
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| {
            (l.min(*v), h.max(*v))
        });
    let range = hi - lo;
    if !(range > 0.0) {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut i = 1;
    while i < n - 1 {
        if y[i] > y[i - 1] {
            // Plateau éventuel : on prend son milieu.
            let mut j = i;
            while j + 1 < n && y[j + 1] == y[i] {
                j += 1;
            }
            if j + 1 < n && y[j + 1] < y[i] {
                let mid = (i + j) / 2;
                let p = prominence(y, mid);
                if p / range >= min_relative {
                    found.push(DetectedPeak {
                        index: mid,
                        x: series.x[mid],
                        height: y[mid],
                        prominence: p,
                    });
                }
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    found.sort_by(|a, b| b.prominence.total_cmp(&a.prominence));
    found.truncate(max_peaks);
    found.sort_by_key(|p| p.index);
    found
}

fn prominence(y: &[f64], peak: usize) -> f64 {
    let h = y[peak];
    let mut left_min = h;
    for &v in y[..peak].iter().rev() {
        if v > h {
            break;
        }
        left_min = left_min.min(v);
    }
    let mut right_min = h;
    for &v in &y[peak + 1..] {
        if v > h {
            break;
        }
        right_min = right_min.min(v);
    }
    h - left_min.max(right_min)
}

/// Position de départ d'un pic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeakSeed {
    pub center: f64,
    /// `None` : lue sur la courbe.
    pub height: Option<f64>,
    /// `None` : estimée d'après l'espacement des pics.
    pub fwhm: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PeaksSpec {
    pub shape: PeakShape,
    pub background: PeakBackground,
    pub seeds: Vec<PeakSeed>,
    /// Même largeur pour tous les pics.
    pub link_widths: bool,
    /// Le centre d'un pic reste dans `départ ± fenêtre`. `None` : libre.
    pub center_window: Option<f64>,
}

/// Paramètres ajustés d'un pic.
#[derive(Debug, Clone, PartialEq)]
pub struct PeakResult {
    pub height: f64,
    pub height_error: Option<f64>,
    pub center: f64,
    pub center_error: Option<f64>,
    /// Largeur à mi-hauteur (Voigt : la largeur totale du profil).
    pub fwhm: f64,
    pub fwhm_error: Option<f64>,
    /// Voigt seulement : largeurs gaussienne et lorentzienne.
    pub fwhm_gauss: Option<f64>,
    pub fwhm_lorentz: Option<f64>,
    pub area: f64,
    /// Part de l'aire de ce pic dans la somme des aires, en %.
    pub area_percent: f64,
}

/// Le modèle ajusté, évaluable en tout X.
#[derive(Debug, Clone, PartialEq)]
pub struct PeakModel {
    shape: PeakShape,
    background: PeakBackground,
    bg: Vec<f64>,
    /// `(hauteur, centre, largeurs)` par pic.
    peaks: Vec<(f64, f64, Vec<f64>)>,
}

impl PeakModel {
    pub fn peak_count(&self) -> usize {
        self.peaks.len()
    }

    /// Valeur du pic `i` seul (sans fond).
    pub fn peak(&self, i: usize, x: f64) -> f64 {
        let (h, c, w) = &self.peaks[i];
        shape_value(self.shape, *h, *c, w, x)
    }

    /// Valeur du fond en `x`.
    pub fn background(&self, x: f64) -> f64 {
        match self.background {
            PeakBackground::None => 0.0,
            PeakBackground::Constant => self.bg[0],
            PeakBackground::Linear => self.bg[0] + self.bg[1] * x,
        }
    }

    /// Fond + somme des pics.
    pub fn total(&self, x: f64) -> f64 {
        self.background(x) + (0..self.peaks.len()).map(|i| self.peak(i, x)).sum::<f64>()
    }
}

fn shape_value(shape: PeakShape, h: f64, c: f64, w: &[f64], x: f64) -> f64 {
    match shape {
        PeakShape::Gaussian => {
            let u = (x - c) / w[0];
            h * (-4.0 * std::f64::consts::LN_2 * u * u).exp()
        }
        PeakShape::Lorentzian => {
            let u = (x - c) / w[0];
            h / (1.0 + 4.0 * u * u)
        }
        PeakShape::Voigt => {
            let p0 = voigt::profile(0.0, w[0], w[1]);
            h * voigt::profile(x - c, w[0], w[1]) / p0
        }
    }
}

fn area_of(shape: PeakShape, h: f64, w: &[f64]) -> f64 {
    match shape {
        PeakShape::Gaussian => {
            h * w[0] * (std::f64::consts::PI / (4.0 * std::f64::consts::LN_2)).sqrt()
        }
        PeakShape::Lorentzian => h * std::f64::consts::PI * w[0] / 2.0,
        PeakShape::Voigt => h / voigt::profile(0.0, w[0], w[1]),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PeaksFit {
    pub model: PeakModel,
    pub peaks: Vec<PeakResult>,
    /// Paramètres du fond et leurs incertitudes.
    pub background: Vec<(f64, Option<f64>)>,
    pub points: usize,
    pub r_squared: f64,
    pub reduced_chi2: f64,
    pub dof: usize,
    /// Nombre de paramètres libres que les données ne déterminent pas (#203).
    pub undetermined: usize,
    /// Arrêt sur un plateau, pas sur un minimum (#281).
    pub plateau: bool,
}

/// Décompose `series` en pics selon `spec`.
pub fn decompose(series: &Series, spec: &PeaksSpec) -> Result<PeaksFit, AnalysisError> {
    let np = spec.seeds.len();
    if np == 0 {
        return Err(AnalysisError::InvalidParameter {
            name: "peaks".into(),
        });
    }
    // Une largeur imposée nulle ou négative ferait partir le solveur sur sa
    // borne basse : ajustement faux, sans erreur (#285).
    if spec.seeds.iter().any(|p| p.fwhm.is_some_and(|f| f <= 0.0)) {
        return Err(AnalysisError::InvalidParameter {
            name: "seeds".into(),
        });
    }
    let wc = spec.shape.width_count();
    let s = series;
    if s.x.len() < 3 {
        // Une série vide ou presque : rien à lire sous les graines posées.
        return Err(AnalysisError::NotEnoughPoints {
            found: s.x.len(),
            needed: 3,
        });
    }
    let (xmin, xmax) =
        s.x.iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| {
                (l.min(*v), h.max(*v))
            });
    let span = (xmax - xmin).abs();
    let min_width = (span * 1e-6).max(f64::MIN_POSITIVE);

    // Fond de départ : minimum de Y (constant), pente nulle.
    let ymin = s.y.iter().cloned().fold(f64::INFINITY, f64::min);
    let mut slots: Vec<Slot> = Vec::new();
    match spec.background {
        PeakBackground::None => {}
        PeakBackground::Constant => slots.push(Slot::new("y0", ymin)),
        PeakBackground::Linear => {
            slots.push(Slot::new("y0", ymin));
            slots.push(Slot::new("slope", 0.0));
        }
    }
    let bg_n = spec.background.param_count();
    let base_guess = if spec.background == PeakBackground::None {
        0.0
    } else {
        ymin
    };

    // Largeur de départ : demi-espacement moyen, ou largeur lue localement.
    let mut centers: Vec<f64> = spec.seeds.iter().map(|p| p.center).collect();
    centers.sort_by(f64::total_cmp);
    let default_width = if np > 1 {
        let mut min_gap = f64::INFINITY;
        for w in centers.windows(2) {
            min_gap = min_gap.min(w[1] - w[0]);
        }
        (min_gap / 2.0).max(span / 200.0)
    } else {
        peak_guess(&s.x, &s.y).fwhm.max(span / 200.0)
    };

    let mut shared_width_slots: Option<Vec<usize>> = None;
    let mut peak_slots: Vec<(usize, usize, Vec<usize>)> = Vec::new();
    for (i, seed) in spec.seeds.iter().enumerate() {
        let near = nearest_index(&s.x, seed.center);
        let h0 = seed
            .height
            .unwrap_or_else(|| s.y.get(near).copied().unwrap_or(0.0) - base_guess);
        let fwhm0 = seed.fwhm.unwrap_or(default_width);
        let h_slot = slots.len();
        slots.push(Slot::new(format!("A{}", i + 1), h0));
        let c_slot = slots.len();
        let mut cs = Slot::new(format!("xc{}", i + 1), seed.center);
        if let Some(win) = spec.center_window {
            cs.min = seed.center - win.abs();
            cs.max = seed.center + win.abs();
        }
        slots.push(cs);
        let mut ws = Vec::new();
        for k in 0..wc {
            if let (true, Some(shared)) = (spec.link_widths, &shared_width_slots) {
                ws.push(shared[k]);
                continue;
            }
            let name = match (spec.shape, k) {
                (PeakShape::Voigt, 0) => "fwhm_gauss",
                (PeakShape::Voigt, _) => "fwhm_lorentz",
                _ => "fwhm",
            };
            let mut w = Slot::new(
                name,
                if spec.shape == PeakShape::Voigt {
                    fwhm0 / 1.5
                } else {
                    fwhm0
                },
            );
            w.min = min_width;
            slots.push(w);
            ws.push(slots.len() - 1);
        }
        if spec.link_widths && shared_width_slots.is_none() {
            shared_width_slots = Some(ws.clone());
        }
        peak_slots.push((h_slot, c_slot, ws));
    }

    let shape = spec.shape;
    let background = spec.background;
    let (xs, ys) = (&s.x, &s.y);
    let layout = peak_slots.clone();
    let term = Term {
        x: xs,
        y: ys,
        sigma: None,
        eval: Box::new(move |v, out| {
            for (o, &x) in out.iter_mut().zip(xs.iter()) {
                let mut acc = match background {
                    PeakBackground::None => 0.0,
                    PeakBackground::Constant => v[0],
                    PeakBackground::Linear => v[0] + v[1] * x,
                };
                for (h, c, ws) in &layout {
                    let w: Vec<f64> = ws.iter().map(|&k| v[k]).collect();
                    acc += shape_value(shape, v[*h], v[*c], &w, x);
                }
                *o = acc;
            }
        }),
    };
    let sol = solve_problem(&slots, &[term], Options::default())?;

    let mut peaks = Vec::new();
    let mut model_peaks = Vec::new();
    for (h, c, ws) in &peak_slots {
        let w: Vec<f64> = ws.iter().map(|&k| sol.values[k]).collect();
        let height = sol.values[*h];
        let (fwhm, fwhm_error, fg, fl) = if spec.shape == PeakShape::Voigt {
            (voigt::fwhm(w[0], w[1]), None, Some(w[0]), Some(w[1]))
        } else {
            (w[0], sol.errors[ws[0]], None, None)
        };
        peaks.push(PeakResult {
            height,
            height_error: sol.errors[*h],
            center: sol.values[*c],
            center_error: sol.errors[*c],
            fwhm,
            fwhm_error,
            fwhm_gauss: fg,
            fwhm_lorentz: fl,
            area: area_of(spec.shape, height, &w),
            area_percent: 0.0,
        });
        model_peaks.push((height, sol.values[*c], w));
    }
    let total: f64 = peaks.iter().map(|p| p.area).sum();
    for p in &mut peaks {
        p.area_percent = if total != 0.0 {
            100.0 * p.area / total
        } else {
            f64::NAN
        };
    }
    Ok(PeaksFit {
        model: PeakModel {
            shape: spec.shape,
            background: spec.background,
            bg: sol.values[..bg_n].to_vec(),
            peaks: model_peaks,
        },
        peaks,
        background: (0..bg_n).map(|i| (sol.values[i], sol.errors[i])).collect(),
        points: sol.terms[0].points,
        r_squared: sol.terms[0].r_squared,
        reduced_chi2: sol.reduced_chi2,
        dof: sol.dof,
        undetermined: sol.undetermined.iter().filter(|u| **u).count(),
        plateau: sol.plateau,
    })
}

fn nearest_index(xs: &[f64], x: f64) -> usize {
    let mut best = 0;
    for (i, v) in xs.iter().enumerate() {
        if (v - x).abs() < (xs[best] - x).abs() {
            best = i;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(n: usize, a: f64, b: f64) -> Vec<f64> {
        (0..n)
            .map(|i| a + (b - a) * i as f64 / (n - 1) as f64)
            .collect()
    }

    fn gauss(x: f64, h: f64, c: f64, w: f64) -> f64 {
        h * (-4.0 * std::f64::consts::LN_2 * ((x - c) / w).powi(2)).exp()
    }

    fn two_gaussians() -> Series {
        let x = grid(600, 0.0, 60.0);
        let y = x
            .iter()
            .map(|&v| 0.5 + 0.01 * v + gauss(v, 10.0, 20.0, 4.0) + gauss(v, 6.0, 32.0, 5.0))
            .collect();
        Series::new(x, y)
    }

    #[test]
    fn detection_par_proeminence_relative() {
        let s = two_gaussians();
        let found = detect(&s, 0.1, 10);
        assert_eq!(found.len(), 2);
        assert!((found[0].x - 20.0).abs() < 0.2 && (found[1].x - 32.0).abs() < 0.2);
        // Un seuil trop haut écarte le petit pic ; le plafond aussi.
        assert_eq!(detect(&s, 0.7, 10).len(), 1);
        let capped = detect(&s, 0.1, 1);
        assert_eq!(capped.len(), 1);
        assert!(
            (capped[0].x - 20.0).abs() < 0.2,
            "le pic le plus proéminent est gardé"
        );
    }

    #[test]
    fn le_bruit_sous_le_seuil_n_est_pas_un_pic() {
        let x = grid(500, 0.0, 50.0);
        let y: Vec<f64> = x
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                gauss(v, 10.0, 25.0, 4.0) + 0.05 * ((i as f64 * 0.7548776662).fract() - 0.5)
            })
            .collect();
        let s = Series::new(x, y);
        // Témoin : sans seuil, le bruit fabrique des dizaines de maxima.
        assert!(detect(&s, 0.0, 1000).len() > 20);
        assert_eq!(detect(&s, 0.1, 1000).len(), 1);
    }

    #[test]
    fn courbe_plate_ou_trop_courte() {
        assert!(detect(
            &Series::new(vec![0.0, 1.0, 2.0], vec![1.0, 1.0, 1.0]),
            0.1,
            5
        )
        .is_empty());
        assert!(detect(&Series::new(vec![0.0, 1.0], vec![0.0, 1.0]), 0.1, 5).is_empty());
    }

    #[test]
    fn un_plateau_donne_un_seul_pic_au_milieu() {
        let y = vec![0.0, 1.0, 5.0, 5.0, 5.0, 1.0, 0.0];
        let s = Series::new((0..7).map(|i| i as f64).collect(), y);
        let f = detect(&s, 0.1, 5);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].index, 3);
    }

    #[test]
    fn decomposition_de_deux_gaussiennes_avec_fond_lineaire() {
        let s = two_gaussians();
        let spec = PeaksSpec {
            shape: PeakShape::Gaussian,
            background: PeakBackground::Linear,
            seeds: vec![
                PeakSeed {
                    center: 19.0,
                    height: None,
                    fwhm: None,
                },
                PeakSeed {
                    center: 33.0,
                    height: None,
                    fwhm: None,
                },
            ],
            link_widths: false,
            center_window: None,
        };
        let r = decompose(&s, &spec).unwrap();
        assert!(
            (r.background[0].0 - 0.5).abs() < 1e-4,
            "y0 {}",
            r.background[0].0
        );
        assert!((r.background[1].0 - 0.01).abs() < 1e-5);
        let (p1, p2) = (&r.peaks[0], &r.peaks[1]);
        assert!((p1.height - 10.0).abs() < 1e-4 && (p1.center - 20.0).abs() < 1e-4);
        assert!((p1.fwhm - 4.0).abs() < 1e-4);
        assert!((p2.height - 6.0).abs() < 1e-4 && (p2.fwhm - 5.0).abs() < 1e-4);
        // Aire gaussienne : h·w·√(π/(4 ln 2)).
        let k = (std::f64::consts::PI / (4.0 * std::f64::consts::LN_2)).sqrt();
        assert!((p1.area - 10.0 * 4.0 * k).abs() < 1e-3);
        assert!((p1.area_percent + p2.area_percent - 100.0).abs() < 1e-9);
        assert!(r.r_squared > 1.0 - 1e-9);
        // La somme du modèle reproduit la courbe.
        let i = nearest_index(&s.x, 20.0);
        assert!((r.model.total(s.x[i]) - s.y[i]).abs() < 1e-3);
    }

    #[test]
    fn largeurs_liees_donnent_la_meme_largeur() {
        let x = grid(500, 0.0, 50.0);
        let y: Vec<f64> = x
            .iter()
            .map(|&v| gauss(v, 8.0, 15.0, 3.0) + gauss(v, 5.0, 30.0, 3.0))
            .collect();
        let s = Series::new(x, y);
        let spec = PeaksSpec {
            shape: PeakShape::Gaussian,
            background: PeakBackground::None,
            seeds: vec![
                PeakSeed {
                    center: 14.0,
                    height: None,
                    fwhm: None,
                },
                PeakSeed {
                    center: 31.0,
                    height: None,
                    fwhm: None,
                },
            ],
            link_widths: true,
            center_window: None,
        };
        let r = decompose(&s, &spec).unwrap();
        assert_eq!(r.peaks[0].fwhm, r.peaks[1].fwhm);
        assert!((r.peaks[0].fwhm - 3.0).abs() < 1e-4);
        assert!(r.background.is_empty());
    }

    #[test]
    fn centre_borne_par_la_fenetre() {
        let x = grid(300, 0.0, 30.0);
        let y: Vec<f64> = x.iter().map(|&v| gauss(v, 5.0, 15.0, 2.0)).collect();
        let s = Series::new(x, y);
        let spec = PeaksSpec {
            shape: PeakShape::Gaussian,
            background: PeakBackground::None,
            seeds: vec![PeakSeed {
                center: 14.0,
                height: Some(4.0),
                fwhm: Some(2.0),
            }],
            link_widths: false,
            // Le vrai centre (15) est hors de 14 ± 0,5.
            center_window: Some(0.5),
        };
        let r = decompose(&s, &spec).unwrap();
        assert!(
            r.peaks[0].center <= 14.5 + 1e-12,
            "centre {}",
            r.peaks[0].center
        );
    }

    fn seed_spec(height: Option<f64>, fwhm: Option<f64>) -> PeaksSpec {
        PeaksSpec {
            shape: PeakShape::Gaussian,
            background: PeakBackground::None,
            seeds: vec![PeakSeed {
                center: 14.0,
                height,
                fwhm,
            }],
            link_widths: false,
            center_window: None,
        }
    }

    #[test]
    fn graine_a_largeur_nulle_est_refusee_sans_ajustement_faux() {
        let x = grid(300, 0.0, 30.0);
        let y: Vec<f64> = x.iter().map(|&v| gauss(v, 5.0, 15.0, 2.0)).collect();
        let s = Series::new(x, y);
        // Témoin : sans largeur imposée, l'ajustement retrouve le pic.
        let ok = decompose(&s, &seed_spec(None, None)).unwrap();
        assert!((ok.peaks[0].center - 15.0).abs() < 1e-3 && ok.r_squared > 0.999);
        for fwhm in [0.0, -1.0] {
            assert_eq!(
                decompose(&s, &seed_spec(None, Some(fwhm))).unwrap_err(),
                AnalysisError::InvalidParameter {
                    name: "seeds".into()
                }
            );
        }
        // Une hauteur imposée sensée (4.0) reste acceptée et ajuste bien.
        let r = decompose(&s, &seed_spec(Some(4.0), None)).unwrap();
        assert!((r.peaks[0].height - 5.0).abs() < 1e-3 && r.r_squared > 0.999);
    }

    fn une_gaussienne() -> Series {
        let x = grid(300, 0.0, 30.0);
        let y: Vec<f64> = x.iter().map(|&v| gauss(v, 5.0, 15.0, 2.0)).collect();
        Series::new(x, y)
    }

    fn graine_invalide() -> AnalysisError {
        AnalysisError::InvalidParameter {
            name: "seeds".into(),
        }
    }

    #[test]
    fn largeur_imposee_nulle_negative_ou_signee_est_refusee() {
        let s = une_gaussienne();
        for fwhm in [0.0, -0.0, -1.0, -f64::MIN_POSITIVE, f64::NEG_INFINITY] {
            assert_eq!(
                decompose(&s, &seed_spec(None, Some(fwhm))).unwrap_err(),
                graine_invalide(),
                "fwhm = {fwhm:e}"
            );
        }
    }

    #[test]
    fn largeur_imposee_minuscule_ou_non_finie_n_est_pas_jugee_par_decompose() {
        // `decompose` ne refuse que `<= 0` ; le reste (NaN, inf, positif
        // minuscule) est du ressort de `validate` ou du solveur, qui échoue
        // sans rendre de faux ajustement (comportement actuel épinglé).
        let s = une_gaussienne();
        for fwhm in [f64::MIN_POSITIVE, 1e-300, 1e-9, f64::NAN, f64::INFINITY] {
            assert_eq!(
                decompose(&s, &seed_spec(None, Some(fwhm))).unwrap_err(),
                AnalysisError::SingularSystem,
                "fwhm = {fwhm:e}"
            );
        }
    }

    #[test]
    fn sans_largeur_imposee_le_resultat_est_inchange() {
        let r = decompose(&une_gaussienne(), &seed_spec(None, None)).unwrap();
        let p = &r.peaks[0];
        assert!((p.height - 5.0).abs() < 1e-3, "{}", p.height);
        assert!((p.center - 15.0).abs() < 1e-3, "{}", p.center);
        assert!((p.fwhm - 2.0).abs() < 2e-3, "{}", p.fwhm);
        assert!((r.r_squared - 1.0).abs() < 1e-6 && !r.plateau);
    }

    #[test]
    fn largeur_valide_imposee_proche_de_la_vraie_ajuste_bien() {
        let r = decompose(&une_gaussienne(), &seed_spec(None, Some(2.5))).unwrap();
        assert!((r.peaks[0].fwhm - 2.0).abs() < 2e-3 && r.r_squared > 0.999999);
    }

    #[test]
    fn graine_valide_et_graine_fautive_donnent_la_meme_erreur_dans_les_deux_ordres() {
        let s = une_gaussienne();
        let bonne = PeakSeed {
            center: 15.0,
            height: None,
            fwhm: Some(2.0),
        };
        let mauvaise = PeakSeed {
            center: 5.0,
            height: None,
            fwhm: Some(0.0),
        };
        for seeds in [vec![bonne, mauvaise], vec![mauvaise, bonne]] {
            let mut spec = seed_spec(None, None);
            spec.seeds = seeds;
            assert_eq!(decompose(&s, &spec).unwrap_err(), graine_invalide());
        }
    }

    #[test]
    fn plusieurs_graines_valides_retrouvent_leurs_pics() {
        let s = two_gaussians();
        let mut spec = seed_spec(None, None);
        spec.background = PeakBackground::Linear;
        spec.seeds = vec![
            PeakSeed {
                center: 19.0,
                height: None,
                fwhm: Some(3.0),
            },
            PeakSeed {
                center: 33.0,
                height: None,
                fwhm: Some(4.0),
            },
        ];
        let r = decompose(&s, &spec).unwrap();
        let attendu = [(10.0, 20.0, 4.0), (6.0, 32.0, 5.0)];
        for (p, (h, c, w)) in r.peaks.iter().zip(attendu) {
            assert!((p.height - h).abs() < 1e-3, "h {} vs {h}", p.height);
            assert!((p.center - c).abs() < 1e-3, "c {} vs {c}", p.center);
            assert!((p.fwhm - w).abs() < 2e-3, "w {} vs {w}", p.fwhm);
        }
        assert!(r.r_squared > 0.999999 && !r.plateau);
    }

    #[test]
    fn hauteur_imposee_minuscule_est_signalee_non_convergee() {
        // Pas refusée (la hauteur est de signe libre) : le solveur s'arrête
        // sur un plateau, ce que `plateau` (donc `Warning::NotConverged`)
        // signale ; le résultat n'est pas un ajustement.
        let r = decompose(&une_gaussienne(), &seed_spec(Some(1e-9), None)).unwrap();
        assert!(r.plateau && r.r_squared < 1e-6, "{r:?}");
    }

    #[test]
    fn hauteur_imposee_nulle_ou_negative_converge_comme_sans_hauteur() {
        for h in [Some(0.0), Some(-4.0)] {
            let r = decompose(&une_gaussienne(), &seed_spec(h, None)).unwrap();
            let p = &r.peaks[0];
            assert!((p.height - 5.0).abs() < 1e-3 && (p.center - 15.0).abs() < 1e-3);
            assert!(r.r_squared > 0.999999, "{h:?}");
        }
    }

    #[test]
    fn centre_hors_donnees_ou_sur_le_bord_comportement_actuel() {
        // Graine loin du pic : hauteur lue ~0, gradient nul, le centre reste
        // où il est posé (R² négatif, aucun drapeau). Comportement épinglé.
        let s = une_gaussienne();
        for c in [-5.0, 0.0, 30.0] {
            let mut spec = seed_spec(None, None);
            spec.seeds[0].center = c;
            let r = decompose(&s, &spec).unwrap();
            assert_eq!(r.peaks[0].center, c);
            assert!(
                r.peaks[0].height.abs() < 1e-30 && r.r_squared < 0.0,
                "c = {c}"
            );
        }
        let mut spec = seed_spec(None, None);
        spec.seeds[0].center = 100.0;
        assert_eq!(
            decompose(&s, &spec).unwrap_err(),
            AnalysisError::SingularSystem
        );
    }

    #[test]
    fn fenetre_de_centre_nulle_fige_le_centre() {
        // Choix documenté (#285, « écarté ») : `center_window: Some(0.0)`
        // verrouille le centre sur la graine. Ce test est là pour qu'on voie
        // ce choix changer.
        let mut spec = seed_spec(None, None);
        spec.center_window = Some(0.0);
        let r = decompose(&une_gaussienne(), &spec).unwrap();
        assert_eq!(r.peaks[0].center, 14.0);
        assert!(r.r_squared < 0.99);
    }

    #[test]
    fn lorentzienne_et_voigt_retrouvent_leurs_parametres() {
        let x = grid(800, -40.0, 40.0);
        let lor: Vec<f64> = x
            .iter()
            .map(|&v| 7.0 / (1.0 + 4.0 * ((v - 3.0) / 5.0).powi(2)))
            .collect();
        let r = decompose(
            &Series::new(x.clone(), lor),
            &PeaksSpec {
                shape: PeakShape::Lorentzian,
                background: PeakBackground::None,
                seeds: vec![PeakSeed {
                    center: 2.0,
                    height: None,
                    fwhm: None,
                }],
                link_widths: false,
                center_window: None,
            },
        )
        .unwrap();
        let p = &r.peaks[0];
        assert!(
            (p.height - 7.0).abs() < 1e-5
                && (p.center - 3.0).abs() < 1e-5
                && (p.fwhm - 5.0).abs() < 1e-5
        );
        assert!((p.area - 7.0 * std::f64::consts::PI * 5.0 / 2.0).abs() < 1e-3);

        let p0 = voigt::profile(0.0, 3.0, 2.0);
        let vo: Vec<f64> = x
            .iter()
            .map(|&v| 9.0 * voigt::profile(v + 4.0, 3.0, 2.0) / p0)
            .collect();
        let r = decompose(
            &Series::new(x, vo),
            &PeaksSpec {
                shape: PeakShape::Voigt,
                background: PeakBackground::None,
                seeds: vec![PeakSeed {
                    center: -3.0,
                    height: None,
                    fwhm: None,
                }],
                link_widths: false,
                center_window: None,
            },
        )
        .unwrap();
        let p = &r.peaks[0];
        assert!((p.height - 9.0).abs() < 1e-3, "h {}", p.height);
        assert!((p.center + 4.0).abs() < 1e-4);
        assert!(
            (p.fwhm_gauss.unwrap() - 3.0).abs() < 0.01
                && (p.fwhm_lorentz.unwrap() - 2.0).abs() < 0.01
        );
        assert!((p.fwhm - voigt::fwhm(3.0, 2.0)).abs() < 0.01);
    }

    #[test]
    fn sans_pic_c_est_une_erreur() {
        let s = two_gaussians();
        let spec = PeaksSpec {
            shape: PeakShape::Gaussian,
            background: PeakBackground::None,
            seeds: vec![],
            link_widths: false,
            center_window: None,
        };
        assert!(matches!(
            decompose(&s, &spec),
            Err(AnalysisError::InvalidParameter { .. })
        ));
    }
}
