//! Voigt profile, through the complex error function `w(z)` (Weideman
//! algorithm, SIAM J. Numer. Anal. 31, 1994, N = 32): precision ≈ 1e-13 in
//! the whole upper half-plane, with no external table or dependency. The
//! coefficients of the rational approximation are computed once (direct DFT
//! of 4N points, without FFT: a few microseconds).

use std::sync::OnceLock;

const N: usize = 32;

#[derive(Clone, Copy)]
struct C {
    re: f64,
    im: f64,
}

impl C {
    fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }
    fn add(self, o: C) -> C {
        C::new(self.re + o.re, self.im + o.im)
    }
    fn sub(self, o: C) -> C {
        C::new(self.re - o.re, self.im - o.im)
    }
    fn mul(self, o: C) -> C {
        C::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
    fn div(self, o: C) -> C {
        let d = o.re * o.re + o.im * o.im;
        C::new(
            (self.re * o.re + self.im * o.im) / d,
            (self.im * o.re - self.re * o.im) / d,
        )
    }
}

fn l_param() -> f64 {
    (N as f64 / 2.0_f64.sqrt()).sqrt()
}

/// Coefficients `a[1..=N]` (index 0 unused) of the approximation.
fn coefficients() -> &'static [f64; N + 1] {
    static COEFFS: OnceLock<[f64; N + 1]> = OnceLock::new();
    COEFFS.get_or_init(|| {
        let m = 2 * N;
        let m2 = 2 * m;
        let l = l_param();
        // f[0] = 0; f[1..m2] for k = -m+1..=m-1.
        let mut f = vec![0.0; m2];
        for (idx, slot) in f.iter_mut().enumerate().skip(1) {
            let k = idx as f64 - m as f64;
            let theta = k * std::f64::consts::PI / m as f64;
            let t = l * libm::tan(theta / 2.0);
            *slot = libm::exp(-t * t) * (l * l + t * t);
        }
        // fftshift: shift by half a length.
        let shifted: Vec<f64> = (0..m2).map(|j| f[(j + m2 / 2) % m2]).collect();
        let mut a = [0.0; N + 1];
        for (n, slot) in a.iter_mut().enumerate().skip(1) {
            let mut re = 0.0;
            for (j, v) in shifted.iter().enumerate() {
                re += v * libm::cos(2.0 * std::f64::consts::PI * (j * n) as f64 / m2 as f64);
            }
            *slot = re / m2 as f64;
        }
        a
    })
}

/// `w(z) = exp(−z²)·erfc(−iz)` for `Im z ≥ 0`. Returns `(Re, Im)`.
pub fn faddeeva(x: f64, y: f64) -> (f64, f64) {
    let l = l_param();
    let z = C::new(x, y);
    let iz = C::new(-z.im, z.re);
    let num = C::new(l, 0.0).add(iz);
    let den = C::new(l, 0.0).sub(iz);
    let zz = num.div(den);
    let a = coefficients();
    // p = Σ_{m=1..N} a[m]·Z^(m-1), by Horner.
    let mut p = C::new(a[N], 0.0);
    for m in (1..N).rev() {
        p = p.mul(zz).add(C::new(a[m], 0.0));
    }
    let den2 = den.mul(den);
    let w = C::new(2.0, 0.0)
        .mul(p)
        .div(den2)
        .add(C::new(1.0 / std::f64::consts::PI.sqrt(), 0.0).div(den));
    (w.re, w.im)
}

/// Voigt profile **normalized to unit area**: convolution of a
/// Gaussian of full width at half maximum `fwhm_g` and a Lorentzian of
/// full width at half maximum `fwhm_l`, centerd at 0.
///
/// NaN contract: a negative width (invalid domain; Weideman requires
/// `Im z ≥ 0`) or both widths zero (Dirac, no finite value) give
/// NaN. A Gaussian width that is zero or negligible against the Lorentzian
/// (`σ·1e30 < γ`) gives the Lorentzian limit, a zero Lorentzian width
/// the Gaussian limit; the difference is then < 1e-60 in relative terms, and avoids the
/// overflow of `den²` in `faddeeva`. Far from the center (`|x| > 1e8·σ√2`),
/// the Lorentzian tail is used in the same way.
pub fn profile(x: f64, fwhm_g: f64, fwhm_l: f64) -> f64 {
    if fwhm_g < 0.0 || fwhm_l < 0.0 || (fwhm_g == 0.0 && fwhm_l == 0.0) {
        return f64::NAN;
    }
    let sigma = fwhm_g / (2.0 * (2.0 * std::f64::consts::LN_2).sqrt());
    let gamma = fwhm_l / 2.0;
    if sigma <= 0.0 || gamma > sigma * 1e30 {
        // Lorentzian limit.
        return gamma / (std::f64::consts::PI * (x * x + gamma * gamma));
    }
    if gamma <= 0.0 {
        return libm::exp(-x * x / (2.0 * sigma * sigma))
            / (sigma * (2.0 * std::f64::consts::PI).sqrt());
    }
    let s2 = sigma * std::f64::consts::SQRT_2;
    // Tail: |x/s2| > 1e8, the correction to the Lorentzian limit is
    // < 1e-16 in relative terms, and `faddeeva` would overflow on |z|² (#295).
    if x.abs() > 1e8 * s2 {
        return gamma / (std::f64::consts::PI * (x * x + gamma * gamma));
    }
    faddeeva(x / s2, gamma / s2).0 / (sigma * (2.0 * std::f64::consts::PI).sqrt())
}

/// Total full width at half maximum of the profile (Olivero & Longbothum, 1977), error
/// < 0.02 %.
pub fn fwhm(fwhm_g: f64, fwhm_l: f64) -> f64 {
    0.5346 * fwhm_l + (0.2166 * fwhm_l * fwhm_l + fwhm_g * fwhm_g).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_values_of_w() {
        // w(0) = 1.
        let (re, im) = faddeeva(0.0, 0.0);
        assert!((re - 1.0).abs() < 1e-12 && im.abs() < 1e-12);
        // w(1) = e⁻¹ + i·(2/√π)·D(1), D(1) = 0.5380795069127684 (Dawson).
        let (re, im) = faddeeva(1.0, 0.0);
        assert!((re - libm::exp(-1.0_f64)).abs() < 1e-12, "re {re}");
        assert!((im - 0.607_157_705_841_2).abs() < 1e-10, "im {im}");
        // w(i) = erfcx(1) = 0.4275835761558070.
        let (re, im) = faddeeva(0.0, 1.0);
        assert!((re - 0.427_583_576_155_807).abs() < 1e-11 && im.abs() < 1e-11);
        // Large modulus: erfcx(y) ≈ (1/(y√π))·(1 − 1/(2y²) + 3/(4y⁴)).
        let y = 50.0_f64;
        let want = (1.0 - 0.5 / (y * y) + 0.75 / y.powi(4)) / (y * std::f64::consts::PI.sqrt());
        let (re, im) = faddeeva(0.0, y);
        assert!(
            (re - want).abs() < 1e-11 && im.abs() < 1e-11,
            "{re} vs {want}"
        );
    }

    #[test]
    fn gaussian_and_lorentzian_limits() {
        let fg = 2.0;
        let sigma = fg / (2.0 * (2.0 * std::f64::consts::LN_2).sqrt());
        let gauss = |x: f64| {
            libm::exp(-x * x / (2.0 * sigma * sigma))
                / (sigma * (2.0 * std::f64::consts::PI).sqrt())
        };
        for x in [0.0, 0.5, 1.7, 4.0] {
            // very small γ: recovers the Gaussian.
            assert!((profile(x, fg, 1e-9) - gauss(x)).abs() < 1e-7, "x={x}");
            assert!((profile(x, fg, 0.0) - gauss(x)).abs() < 1e-14);
        }
        let g = 1.5;
        let lor = |x: f64| (g / 2.0) / (std::f64::consts::PI * (x * x + g * g / 4.0));
        for x in [0.0, 0.5, 3.0] {
            assert!((profile(x, 1e-6, g) - lor(x)).abs() < 1e-5, "x={x}");
            assert!((profile(x, 0.0, g) - lor(x)).abs() < 1e-14);
        }
    }

    #[test]
    fn a_tiny_gaussian_width_gives_the_lorentzian_limit() {
        // Before: den² overflowed (inf/inf = NaN) from fwhm_g ~ 1e-200.
        for x in [0.0, 0.5] {
            let lor = profile(x, 0.0, 1.0);
            let v = profile(x, 1e-200, 1.0);
            assert!((v - lor).abs() < 1e-12 && v.is_finite(), "x {x} : {v}");
        }
        // Ordinary value unchanged.
        assert!((profile(0.0, 1e-100, 1.0) - 0.636_619_772_367_581_3).abs() < 1e-12);
    }

    #[test]
    fn zero_or_negative_total_width_gives_nan() {
        assert!(profile(0.0, 0.0, 0.0).is_nan());
        assert!(profile(1.0, 0.0, 0.0).is_nan());
        assert!(profile(0.5, 1.0, -1.0).is_nan());
        assert!(profile(0.5, -1.0, 1.0).is_nan());
        assert!(profile(0.5, -1.0, -1.0).is_nan());
        // A single zero width stays a valid limit.
        assert!(profile(0.0, 0.0, 1.0).is_finite() && profile(0.0, 1.0, 0.0).is_finite());
    }

    #[test]
    fn the_profile_is_normalized_and_its_width_follows_olivero() {
        let (fg, fl) = (1.2, 0.8);
        // Trapezoid area over ±200 (slow Lorentzian tails).
        let n = 400_000;
        let h = 400.0 / n as f64;
        let mut area = 0.0;
        for i in 0..=n {
            let x = -200.0 + i as f64 * h;
            let w = if i == 0 || i == n { 0.5 } else { 1.0 };
            area += w * profile(x, fg, fl) * h;
        }
        // Remaining tail ≈ 2·(γ/π)/200 ≈ 0.0013 outside the interval.
        assert!((area - 1.0).abs() < 2e-3, "area {area}");
        let peak = profile(0.0, fg, fl);
        let half = fwhm(fg, fl) / 2.0;
        assert!((profile(half, fg, fl) / peak - 0.5).abs() < 5e-3);
    }

    const FG_UNIT: f64 = 2.354_820_045_030_949_3; // σ = 1
    fn gauss_ref(x: f64, fg: f64) -> f64 {
        let s = fg / (2.0 * (2.0 * std::f64::consts::LN_2).sqrt());
        libm::exp(-x * x / (2.0 * s * s)) / (s * (2.0 * std::f64::consts::PI).sqrt())
    }
    fn lorentz_ref(x: f64, fl: f64) -> f64 {
        let g = fl / 2.0;
        g / (std::f64::consts::PI * (x * x + g * g))
    }

    #[test]
    fn voigt_reference_peak_sigma_and_gamma_one() {
        // V(0; σ=1, γ=1) = erfcx(1/√2)/√(2π) = 0.523126 / 2.506628 = 0.208709.
        let v = profile(0.0, FG_UNIT, 2.0);
        assert!((v - 0.208_709).abs() < 1e-6, "{v}");
    }

    #[test]
    fn voigt_equals_the_numerical_convolution() {
        // Independent reference: ∫ G(t)·L(x−t) dt by trapezoids.
        let (fg, fl) = (1.2, 0.8);
        for x in [0.0, 0.7, 2.5] {
            let (n, lo, hi) = (200_000, -12.0, 12.0);
            let h = (hi - lo) / n as f64;
            let mut acc = 0.0;
            for i in 0..=n {
                let t = lo + i as f64 * h;
                let w = if i == 0 || i == n { 0.5 } else { 1.0 };
                acc += w * gauss_ref(t, fg) * lorentz_ref(x - t, fl) * h;
            }
            let v = profile(x, fg, fl);
            // Gaussian tail cut at ±12: negligible (> 10 σ).
            assert!((v - acc).abs() < 1e-5, "x={x} {v} vs {acc}");
        }
    }

    #[test]
    fn pure_limits_against_closed_formulas() {
        for x in [0.0, 0.3, 1.0, 7.0, -2.0] {
            for fl in [0.1, 1.0, 5.0] {
                let v = profile(x, 0.0, fl);
                assert!((v - lorentz_ref(x, fl)).abs() < 1e-15, "L x={x} fl={fl}");
            }
            for fg in [0.1, 1.0, 5.0] {
                let v = profile(x, fg, 0.0);
                assert!((v - gauss_ref(x, fg)).abs() < 1e-15, "G x={x} fg={fg}");
            }
        }
    }

    #[test]
    fn symmetry_positivity_and_decay() {
        for (fg, fl) in [(1.2, 0.8), (0.5, 3.0), (4.0, 0.05)] {
            let mut prev = f64::INFINITY;
            for i in 0..=400 {
                let x = i as f64 * 0.05;
                let v = profile(x, fg, fl);
                assert!(v > 0.0 && v.is_finite(), "({fg},{fl}) x={x} {v}");
                let m = profile(-x, fg, fl);
                assert!((v - m).abs() <= 1e-15 * v.max(1.0), "symmetry x={x}");
                assert!(v <= prev + 1e-15, "({fg},{fl}) increases at x={x}");
                prev = v;
            }
        }
    }

    #[test]
    fn continuity_around_the_1e30_threshold() {
        // σ = 1 (fwhm_g = FG_UNIT); the threshold is γ = σ·1e30, i.e. fwhm_l = 2e30.
        for x in [0.0, 1.0, 1e30] {
            for fl in [2e29, 2e30 * (1.0 - 1e-9), 2e30, 2e30 * (1.0 + 1e-9), 2e31] {
                let v = profile(x, FG_UNIT, fl);
                assert!(v.is_finite() && v > 0.0, "x={x} fl={fl} {v}");
                let ref_l = lorentz_ref(x, fl);
                assert!(
                    (v - ref_l).abs() <= 1e-9 * ref_l,
                    "x={x} fl={fl}: {v} vs {ref_l}"
                );
            }
        }
        let a = profile(0.0, FG_UNIT, 2e30 * (1.0 - 1e-12));
        let b = profile(0.0, FG_UNIT, 2e30 * (1.0 + 1e-12));
        assert!(
            (a - b).abs() <= 1e-9 * a,
            "jump at the threshold {a} vs {b}"
        );
    }

    #[test]
    fn zero_or_negative_widths_combinations() {
        for x in [0.0, 1.0, -3.0] {
            assert!(profile(x, 0.0, 0.0).is_nan());
            assert!(profile(x, -1.0, 0.0).is_nan());
            assert!(profile(x, 0.0, -1.0).is_nan());
            assert!(profile(x, -1.0, -1.0).is_nan());
            assert!(profile(x, -1e-300, 1.0).is_nan());
            assert!(profile(x, 1.0, -1e-300).is_nan());
            // -0.0 == 0.0: both widths zero stays NaN.
            assert!(profile(x, -0.0, -0.0).is_nan());
        }
        assert!(profile(0.0, f64::NEG_INFINITY, 1.0).is_nan());
        assert!(profile(0.0, 1.0, f64::NEG_INFINITY).is_nan());
    }

    #[test]
    fn non_finite_x_gives_nan_or_zero() {
        for (fg, fl) in [(1.0, 1.0), (0.0, 1.0), (1.0, 0.0)] {
            assert!(profile(f64::NAN, fg, fl).is_nan(), "NaN ({fg},{fl})");
            for x in [f64::INFINITY, f64::NEG_INFINITY] {
                let v = profile(x, fg, fl);
                assert!(v == 0.0 || v.is_nan(), "x={x} ({fg},{fl}) {v}");
            }
        }
    }

    #[test]
    fn extreme_widths_without_nan() {
        for (fg, fl) in [
            (1e150, 1.0),
            (1.0, 1e150),
            (1e150, 1e150),
            (1e-150, 1.0),
            (1.0, 1e-150),
            (1e-200, 1.0),
            (1e-300, 1.0),
            (1.0, 1e-300),
            (1e-12, 1.0),
            (1.0, 1e-12),
        ] {
            for x in [0.0, 0.5] {
                let v = profile(x, fg, fl);
                assert!(!v.is_nan() && v >= 0.0, "({fg},{fl}) x={x} {v}");
            }
        }
        // Very different width: the order of magnitude follows the wider one.
        for x in [0.0, 0.5, 2.0] {
            let v = profile(x, 1e-12, 1.0);
            assert!((v - lorentz_ref(x, 1.0)).abs() < 1e-9, "x={x} {v}");
            let w = profile(x, 1.0, 1e-12);
            assert!((w - gauss_ref(x, 1.0)).abs() < 1e-9, "x={x} {w}");
        }
        // Giant widths: peak ≈ 1/(π·γ) / 1/(σ√2π) depending on the dominant one.
        let p = profile(0.0, 1.0, 1e150);
        assert!((p / lorentz_ref(0.0, 1e150) - 1.0).abs() < 1e-9, "{p}");
        let q = profile(0.0, 1e150, 1.0);
        assert!((q / gauss_ref(0.0, 1e150) - 1.0).abs() < 1e-9, "{q}");
    }

    #[test]
    fn both_widths_tiny_far_from_the_center() {
        for (fg, fl) in [(1e-160, 1e-160), (1e-300, 1e-300)] {
            let v = profile(0.5, fg, fl);
            assert!(v.is_finite() && v >= 0.0, "({fg},{fl}) {v}");
            // Lorentzian tail γ/(π·x²), γ = fl/2.
            let expected = fl / 2.0 / (std::f64::consts::PI * 0.25);
            assert!((v / expected - 1.0).abs() < 1e-9, "({fg},{fl}) {v}");
        }
        assert!(profile(1000.0, 1e-300, 1e-300).is_finite());
    }

    #[test]
    fn tiny_ordinary_gaussian_values_unchanged() {
        assert!((profile(0.0, 1e-100, 1.0) - 0.636_619_772_367_581_3).abs() < 1e-12);
        assert!((profile(0.0, 1e-200, 1.0) - 0.636_619_772_367_581_3).abs() < 1e-12);
    }
}
