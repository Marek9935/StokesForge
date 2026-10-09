//! Profil de Voigt, par la fonction d'erreur complexe `w(z)` (algorithme de
//! Weideman, SIAM J. Numer. Anal. 31, 1994, N = 32) : précision ≈ 1e-13 dans
//! tout le demi-plan supérieur, sans table externe ni dépendance. Les
//! coefficients de l'approximation rationnelle sont calculés une fois (DFT
//! directe de 4N points, sans FFT : quelques microsecondes).

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

/// Coefficients `a[1..=N]` (indice 0 inutilisé) de l'approximation.
fn coefficients() -> &'static [f64; N + 1] {
    static COEFFS: OnceLock<[f64; N + 1]> = OnceLock::new();
    COEFFS.get_or_init(|| {
        let m = 2 * N;
        let m2 = 2 * m;
        let l = l_param();
        // f[0] = 0 ; f[1..m2] pour k = -m+1..=m-1.
        let mut f = vec![0.0; m2];
        for (idx, slot) in f.iter_mut().enumerate().skip(1) {
            let k = idx as f64 - m as f64;
            let theta = k * std::f64::consts::PI / m as f64;
            let t = l * (theta / 2.0).tan();
            *slot = (-t * t).exp() * (l * l + t * t);
        }
        // fftshift : décalage d'une demi-longueur.
        let shifted: Vec<f64> = (0..m2).map(|j| f[(j + m2 / 2) % m2]).collect();
        let mut a = [0.0; N + 1];
        for (n, slot) in a.iter_mut().enumerate().skip(1) {
            let mut re = 0.0;
            for (j, v) in shifted.iter().enumerate() {
                re += v * (2.0 * std::f64::consts::PI * (j * n) as f64 / m2 as f64).cos();
            }
            *slot = re / m2 as f64;
        }
        a
    })
}

/// `w(z) = exp(−z²)·erfc(−iz)` pour `Im z ≥ 0`. Renvoie `(Re, Im)`.
pub fn faddeeva(x: f64, y: f64) -> (f64, f64) {
    let l = l_param();
    let z = C::new(x, y);
    let iz = C::new(-z.im, z.re);
    let num = C::new(l, 0.0).add(iz);
    let den = C::new(l, 0.0).sub(iz);
    let zz = num.div(den);
    let a = coefficients();
    // p = Σ_{m=1..N} a[m]·Z^(m-1), par Horner.
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

/// Profil de Voigt **normalisé à l'unité d'aire** : convolution d'une
/// gaussienne de largeur à mi-hauteur `fwhm_g` et d'une lorentzienne de
/// largeur à mi-hauteur `fwhm_l`, centrée en 0.
pub fn profile(x: f64, fwhm_g: f64, fwhm_l: f64) -> f64 {
    let sigma = fwhm_g / (2.0 * (2.0 * std::f64::consts::LN_2).sqrt());
    let gamma = fwhm_l / 2.0;
    if sigma <= 0.0 {
        // Limite lorentzienne.
        return gamma / (std::f64::consts::PI * (x * x + gamma * gamma));
    }
    if gamma <= 0.0 {
        return (-x * x / (2.0 * sigma * sigma)).exp()
            / (sigma * (2.0 * std::f64::consts::PI).sqrt());
    }
    let s2 = sigma * std::f64::consts::SQRT_2;
    faddeeva(x / s2, gamma / s2).0 / (sigma * (2.0 * std::f64::consts::PI).sqrt())
}

/// Largeur à mi-hauteur totale du profil (Olivero & Longbothum, 1977), écart
/// < 0,02 %.
pub fn fwhm(fwhm_g: f64, fwhm_l: f64) -> f64 {
    0.5346 * fwhm_l + (0.2166 * fwhm_l * fwhm_l + fwhm_g * fwhm_g).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valeurs_de_reference_de_w() {
        // w(0) = 1.
        let (re, im) = faddeeva(0.0, 0.0);
        assert!((re - 1.0).abs() < 1e-12 && im.abs() < 1e-12);
        // w(1) = e⁻¹ + i·(2/√π)·D(1), D(1) = 0.5380795069127684 (Dawson).
        let (re, im) = faddeeva(1.0, 0.0);
        assert!((re - (-1.0_f64).exp()).abs() < 1e-12, "re {re}");
        assert!((im - 0.607_157_705_841_2).abs() < 1e-10, "im {im}");
        // w(i) = erfcx(1) = 0.4275835761558070.
        let (re, im) = faddeeva(0.0, 1.0);
        assert!((re - 0.427_583_576_155_807).abs() < 1e-11 && im.abs() < 1e-11);
        // Grand module : erfcx(y) ≈ (1/(y√π))·(1 − 1/(2y²) + 3/(4y⁴)).
        let y = 50.0_f64;
        let want = (1.0 - 0.5 / (y * y) + 0.75 / y.powi(4)) / (y * std::f64::consts::PI.sqrt());
        let (re, im) = faddeeva(0.0, y);
        assert!(
            (re - want).abs() < 1e-11 && im.abs() < 1e-11,
            "{re} vs {want}"
        );
    }

    #[test]
    fn limites_gaussienne_et_lorentzienne() {
        let fg = 2.0;
        let sigma = fg / (2.0 * (2.0 * std::f64::consts::LN_2).sqrt());
        let gauss = |x: f64| {
            (-x * x / (2.0 * sigma * sigma)).exp() / (sigma * (2.0 * std::f64::consts::PI).sqrt())
        };
        for x in [0.0, 0.5, 1.7, 4.0] {
            // γ tout petit : retrouve la gaussienne.
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
    fn le_profil_est_normalise_et_sa_largeur_suit_olivero() {
        let (fg, fl) = (1.2, 0.8);
        // Aire par trapèzes sur ±200 (queues lorentziennes lentes).
        let n = 400_000;
        let h = 400.0 / n as f64;
        let mut area = 0.0;
        for i in 0..=n {
            let x = -200.0 + i as f64 * h;
            let w = if i == 0 || i == n { 0.5 } else { 1.0 };
            area += w * profile(x, fg, fl) * h;
        }
        // Reste de queue ≈ 2·(γ/π)/200 ≈ 0.0013 hors intervalle.
        assert!((area - 1.0).abs() < 2e-3, "aire {area}");
        let peak = profile(0.0, fg, fl);
        let half = fwhm(fg, fl) / 2.0;
        assert!((profile(half, fg, fl) / peak - 0.5).abs() < 5e-3);
    }
}
