//! Peak detection and decomposition into a sum of peaks (#189): Gaussians,
//! Lorentzians or Voigt profiles, with a constant or linear background fitted
//! **at the same time** as the peaks (M14 b).
//!
//! Detection: local maxima filtered by **relative prominence** (height
//! above the higher of the two neighbouring troughs, relative to the Y range)
//! and capped at a maximum number of peaks (M14 a C).
//!
//! The decomposition is an ordinary fit of the [`crate::solver`] engine:
//! linked widths (same slot for all peaks) and bounded positions
//! (center ± window) are slot settings, not code specific to peaks.

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
    /// Parameters of a peak: height, center, then one or two widths.
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

/// A local maximum found by [`detect`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectedPeak {
    pub index: usize,
    pub x: f64,
    pub height: f64,
    pub prominence: f64,
}

/// Local maxima of `y` whose relative prominence reaches `min_relative`,
/// at most `max_peaks`, sorted by increasing X. `y` is read in the given order.
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
            // Possible plateau: take its middle.
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

/// Starting position of a peak.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeakSeed {
    pub center: f64,
    /// `None`: read from the curve.
    pub height: Option<f64>,
    /// `None`: estimated from the peak spacing.
    pub fwhm: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PeaksSpec {
    pub shape: PeakShape,
    pub background: PeakBackground,
    pub seeds: Vec<PeakSeed>,
    /// Same width for all peaks.
    pub link_widths: bool,
    /// The center of a peak stays within `start ± window`. `None`: free.
    pub center_window: Option<f64>,
}

/// Fitted parameters of a peak.
#[derive(Debug, Clone, PartialEq)]
pub struct PeakResult {
    pub height: f64,
    pub height_error: Option<f64>,
    pub center: f64,
    pub center_error: Option<f64>,
    /// Full width at half maximum (Voigt: the total width of the profile).
    pub fwhm: f64,
    pub fwhm_error: Option<f64>,
    /// Voigt only: Gaussian and Lorentzian widths.
    pub fwhm_gauss: Option<f64>,
    pub fwhm_lorentz: Option<f64>,
    pub area: f64,
    /// Share of the area of this peak in the sum of the areas, in %.
    pub area_percent: f64,
}

/// The fitted model, evaluable at any X.
#[derive(Debug, Clone, PartialEq)]
pub struct PeakModel {
    shape: PeakShape,
    background: PeakBackground,
    bg: Vec<f64>,
    /// `(height, center, widths)` per peak.
    peaks: Vec<(f64, f64, Vec<f64>)>,
}

impl PeakModel {
    pub fn peak_count(&self) -> usize {
        self.peaks.len()
    }

    /// Value of peak `i` alone (without background).
    pub fn peak(&self, i: usize, x: f64) -> f64 {
        let (h, c, w) = &self.peaks[i];
        shape_value(self.shape, *h, *c, w, x)
    }

    /// Value of the background at `x`.
    pub fn background(&self, x: f64) -> f64 {
        match self.background {
            PeakBackground::None => 0.0,
            PeakBackground::Constant => self.bg[0],
            PeakBackground::Linear => self.bg[0] + self.bg[1] * x,
        }
    }

    /// Background + sum of the peaks.
    pub fn total(&self, x: f64) -> f64 {
        self.background(x) + (0..self.peaks.len()).map(|i| self.peak(i, x)).sum::<f64>()
    }
}

fn shape_value(shape: PeakShape, h: f64, c: f64, w: &[f64], x: f64) -> f64 {
    match shape {
        PeakShape::Gaussian => {
            let u = (x - c) / w[0];
            h * libm::exp(-4.0 * std::f64::consts::LN_2 * u * u)
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
    /// Background parameters and their uncertainties.
    pub background: Vec<(f64, Option<f64>)>,
    pub points: usize,
    pub r_squared: f64,
    pub reduced_chi2: f64,
    pub dof: usize,
    /// Number of free parameters that the data do not determine (#203).
    pub undetermined: usize,
    /// Stop on a plateau (#281) or solver blocked without an accepted step (C1, #313):
    /// not on a minimum.
    pub plateau: bool,
}

/// Decomposes `series` into peaks according to `spec`.
pub fn decompose(series: &Series, spec: &PeaksSpec) -> Result<PeaksFit, AnalysisError> {
    let np = spec.seeds.len();
    if np == 0 {
        return Err(AnalysisError::InvalidParameter {
            name: "peaks".into(),
        });
    }
    // A zero or negative imposed width would send the solver to its
    // lower bound: a wrong fit, without error (#285).
    if spec.seeds.iter().any(|p| p.fwhm.is_some_and(|f| f <= 0.0)) {
        return Err(AnalysisError::InvalidParameter {
            name: "seeds".into(),
        });
    }
    let wc = spec.shape.width_count();
    let s = series;
    if s.x.len() < 3 {
        // An empty or almost empty series: nothing to read under the placed seeds.
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

    // Starting background: minimum of Y (constant), zero slope.
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

    // Starting width: mean half-spacing, or locally read width.
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
        plateau: sol.plateau || sol.stalled,
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
        h * libm::exp(-4.0 * std::f64::consts::LN_2 * ((x - c) / w).powi(2))
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
    fn detection_by_relative_prominence() {
        let s = two_gaussians();
        let found = detect(&s, 0.1, 10);
        assert_eq!(found.len(), 2);
        assert!((found[0].x - 20.0).abs() < 0.2 && (found[1].x - 32.0).abs() < 0.2);
        // A threshold that is too high drops the small peak; so does the cap.
        assert_eq!(detect(&s, 0.7, 10).len(), 1);
        let capped = detect(&s, 0.1, 1);
        assert_eq!(capped.len(), 1);
        assert!(
            (capped[0].x - 20.0).abs() < 0.2,
            "the most prominent peak is kept"
        );
    }

    #[test]
    fn noise_below_the_threshold_is_not_a_peak() {
        let x = grid(500, 0.0, 50.0);
        let y: Vec<f64> = x
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                gauss(v, 10.0, 25.0, 4.0) + 0.05 * ((i as f64 * 0.7548776662).fract() - 0.5)
            })
            .collect();
        let s = Series::new(x, y);
        // Witness: without a threshold, the noise produces dozens of maxima.
        assert!(detect(&s, 0.0, 1000).len() > 20);
        assert_eq!(detect(&s, 0.1, 1000).len(), 1);
    }

    #[test]
    fn flat_or_too_short_curve() {
        assert!(detect(
            &Series::new(vec![0.0, 1.0, 2.0], vec![1.0, 1.0, 1.0]),
            0.1,
            5
        )
        .is_empty());
        assert!(detect(&Series::new(vec![0.0, 1.0], vec![0.0, 1.0]), 0.1, 5).is_empty());
    }

    #[test]
    fn a_plateau_gives_a_single_peak_in_the_middle() {
        let y = vec![0.0, 1.0, 5.0, 5.0, 5.0, 1.0, 0.0];
        let s = Series::new((0..7).map(|i| i as f64).collect(), y);
        let f = detect(&s, 0.1, 5);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].index, 3);
    }

    #[test]
    fn decomposition_of_two_gaussians_with_linear_background() {
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
        // Gaussian area: h·w·√(π/(4 ln 2)).
        let k = (std::f64::consts::PI / (4.0 * std::f64::consts::LN_2)).sqrt();
        assert!((p1.area - 10.0 * 4.0 * k).abs() < 1e-3);
        assert!((p1.area_percent + p2.area_percent - 100.0).abs() < 1e-9);
        assert!(r.r_squared > 1.0 - 1e-9);
        // The sum of the model reproduces the curve.
        let i = nearest_index(&s.x, 20.0);
        assert!((r.model.total(s.x[i]) - s.y[i]).abs() < 1e-3);
    }

    #[test]
    fn linked_widths_give_the_same_width() {
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
    fn center_bounded_by_the_window() {
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
            // The true center (15) is outside 14 ± 0.5.
            center_window: Some(0.5),
        };
        let r = decompose(&s, &spec).unwrap();
        assert!(
            r.peaks[0].center <= 14.5 + 1e-12,
            "center {}",
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
    fn a_seed_with_zero_width_is_refused_without_a_wrong_fit() {
        let x = grid(300, 0.0, 30.0);
        let y: Vec<f64> = x.iter().map(|&v| gauss(v, 5.0, 15.0, 2.0)).collect();
        let s = Series::new(x, y);
        // Witness: without an imposed width, the fit recovers the peak.
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
        // A sensible imposed height (4.0) is still accepted and fits well.
        let r = decompose(&s, &seed_spec(Some(4.0), None)).unwrap();
        assert!((r.peaks[0].height - 5.0).abs() < 1e-3 && r.r_squared > 0.999);
    }

    fn a_gaussian() -> Series {
        let x = grid(300, 0.0, 30.0);
        let y: Vec<f64> = x.iter().map(|&v| gauss(v, 5.0, 15.0, 2.0)).collect();
        Series::new(x, y)
    }

    fn invalid_seed() -> AnalysisError {
        AnalysisError::InvalidParameter {
            name: "seeds".into(),
        }
    }

    #[test]
    fn zero_negative_or_signed_imposed_width_is_refused() {
        let s = a_gaussian();
        for fwhm in [0.0, -0.0, -1.0, -f64::MIN_POSITIVE, f64::NEG_INFINITY] {
            assert_eq!(
                decompose(&s, &seed_spec(None, Some(fwhm))).unwrap_err(),
                invalid_seed(),
                "fwhm = {fwhm:e}"
            );
        }
    }

    #[test]
    fn tiny_or_non_finite_imposed_width_is_not_judged_by_decompose() {
        // `decompose` only refuses `<= 0`; the rest (NaN, inf, tiny
        // positive) is up to `validate` or the solver, which fails
        // without returning a wrong fit (current behavior pinned). An
        // infinite width gives a non-finite slope: model not evaluable (C6).
        let s = a_gaussian();
        for fwhm in [f64::MIN_POSITIVE, 1e-300, 1e-9, f64::NAN, f64::INFINITY] {
            let expected = if fwhm.is_infinite() {
                AnalysisError::ModelNotEvaluable
            } else {
                AnalysisError::SingularSystem
            };
            assert_eq!(
                decompose(&s, &seed_spec(None, Some(fwhm))).unwrap_err(),
                expected,
                "fwhm = {fwhm:e}"
            );
        }
    }

    #[test]
    fn without_an_imposed_width_the_result_is_unchanged() {
        let r = decompose(&a_gaussian(), &seed_spec(None, None)).unwrap();
        let p = &r.peaks[0];
        assert!((p.height - 5.0).abs() < 1e-3, "{}", p.height);
        assert!((p.center - 15.0).abs() < 1e-3, "{}", p.center);
        assert!((p.fwhm - 2.0).abs() < 2e-3, "{}", p.fwhm);
        assert!((r.r_squared - 1.0).abs() < 1e-6 && !r.plateau);
    }

    #[test]
    fn a_valid_imposed_width_close_to_the_true_one_fits_well() {
        let r = decompose(&a_gaussian(), &seed_spec(None, Some(2.5))).unwrap();
        assert!((r.peaks[0].fwhm - 2.0).abs() < 2e-3 && r.r_squared > 0.999999);
    }

    #[test]
    fn a_valid_seed_and_a_faulty_seed_give_the_same_error_in_both_orders() {
        let s = a_gaussian();
        let good_seed = PeakSeed {
            center: 15.0,
            height: None,
            fwhm: Some(2.0),
        };
        let bad_seed = PeakSeed {
            center: 5.0,
            height: None,
            fwhm: Some(0.0),
        };
        for seeds in [vec![good_seed, bad_seed], vec![bad_seed, good_seed]] {
            let mut spec = seed_spec(None, None);
            spec.seeds = seeds;
            assert_eq!(decompose(&s, &spec).unwrap_err(), invalid_seed());
        }
    }

    #[test]
    fn several_valid_seeds_recover_their_peaks() {
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
        let expected = [(10.0, 20.0, 4.0), (6.0, 32.0, 5.0)];
        for (p, (h, c, w)) in r.peaks.iter().zip(expected) {
            assert!((p.height - h).abs() < 1e-3, "h {} vs {h}", p.height);
            assert!((p.center - c).abs() < 1e-3, "c {} vs {c}", p.center);
            assert!((p.fwhm - w).abs() < 2e-3, "w {} vs {w}", p.fwhm);
        }
        assert!(r.r_squared > 0.999999 && !r.plateau);
    }

    #[test]
    fn a_tiny_imposed_height_is_reported_as_not_converged() {
        // Not refused (the height has a free sign): the solver stops
        // on a plateau, which `plateau` (hence `Warning::NotConverged`)
        // reports; the result is not a fit.
        let r = decompose(&a_gaussian(), &seed_spec(Some(1e-9), None)).unwrap();
        assert!(r.plateau && r.r_squared < 1e-6, "{r:?}");
    }

    #[test]
    fn a_zero_or_negative_imposed_height_converges_like_without_a_height() {
        for h in [Some(0.0), Some(-4.0)] {
            let r = decompose(&a_gaussian(), &seed_spec(h, None)).unwrap();
            let p = &r.peaks[0];
            assert!((p.height - 5.0).abs() < 1e-3 && (p.center - 15.0).abs() < 1e-3);
            assert!(r.r_squared > 0.999999, "{h:?}");
        }
    }

    #[test]
    fn center_outside_the_data_or_on_the_edge_current_behavior() {
        // Seed far from the peak: height read ~0, zero gradient, the center stays
        // where it is placed (negative R², no flag). Pinned behavior.
        let s = a_gaussian();
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
    fn a_zero_center_window_freezes_the_center() {
        // Documented choice (#285, "dismissed"): `center_window: Some(0.0)`
        // locks the center on the seed. This test is here so that a change
        // of this choice shows.
        let mut spec = seed_spec(None, None);
        spec.center_window = Some(0.0);
        let r = decompose(&a_gaussian(), &spec).unwrap();
        assert_eq!(r.peaks[0].center, 14.0);
        assert!(r.r_squared < 0.99);
    }

    #[test]
    fn lorentzian_and_voigt_recover_their_parameters() {
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
    fn without_a_peak_it_is_an_error() {
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
