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
            let t = l * libm::tan(theta / 2.0);
            *slot = libm::exp(-t * t) * (l * l + t * t);
        }
        // fftshift : décalage d'une demi-longueur.
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
///
/// Contrat NaN : une largeur négative (domaine invalide ; Weideman exige
/// `Im z ≥ 0`) ou la double largeur nulle (Dirac, sans valeur finie) donnent
/// NaN. Une largeur gaussienne nulle ou négligeable devant la lorentzienne
/// (`σ·1e30 < γ`) donne la limite lorentzienne, une largeur lorentzienne nulle
/// la limite gaussienne ; l'écart est alors < 1e-60 en relatif, et évite le
/// débordement de `den²` dans `faddeeva`. Loin du centre (`|x| > 1e8·σ√2`),
/// la queue lorentzienne est utilisée de même.
pub fn profile(x: f64, fwhm_g: f64, fwhm_l: f64) -> f64 {
    if fwhm_g < 0.0 || fwhm_l < 0.0 || (fwhm_g == 0.0 && fwhm_l == 0.0) {
        return f64::NAN;
    }
    let sigma = fwhm_g / (2.0 * (2.0 * std::f64::consts::LN_2).sqrt());
    let gamma = fwhm_l / 2.0;
    if sigma <= 0.0 || gamma > sigma * 1e30 {
        // Limite lorentzienne.
        return gamma / (std::f64::consts::PI * (x * x + gamma * gamma));
    }
    if gamma <= 0.0 {
        return libm::exp(-x * x / (2.0 * sigma * sigma))
            / (sigma * (2.0 * std::f64::consts::PI).sqrt());
    }
    let s2 = sigma * std::f64::consts::SQRT_2;
    // Queue : |x/s2| > 1e8, la correction à la limite lorentzienne est
    // < 1e-16 en relatif, et `faddeeva` déborderait sur |z|² (#295).
    if x.abs() > 1e8 * s2 {
        return gamma / (std::f64::consts::PI * (x * x + gamma * gamma));
    }
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
        assert!((re - libm::exp(-1.0_f64)).abs() < 1e-12, "re {re}");
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
            libm::exp(-x * x / (2.0 * sigma * sigma))
                / (sigma * (2.0 * std::f64::consts::PI).sqrt())
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
    fn largeur_gaussienne_infime_donne_la_limite_lorentzienne() {
        // Avant : den² débordait (inf/inf = NaN) dès fwhm_g ~ 1e-200.
        for x in [0.0, 0.5] {
            let lor = profile(x, 0.0, 1.0);
            let v = profile(x, 1e-200, 1.0);
            assert!((v - lor).abs() < 1e-12 && v.is_finite(), "x {x} : {v}");
        }
        // Valeur ordinaire inchangée.
        assert!((profile(0.0, 1e-100, 1.0) - 0.636_619_772_367_581_3).abs() < 1e-12);
    }

    #[test]
    fn largeur_totale_nulle_ou_negative_donne_nan() {
        assert!(profile(0.0, 0.0, 0.0).is_nan());
        assert!(profile(1.0, 0.0, 0.0).is_nan());
        assert!(profile(0.5, 1.0, -1.0).is_nan());
        assert!(profile(0.5, -1.0, 1.0).is_nan());
        assert!(profile(0.5, -1.0, -1.0).is_nan());
        // Une seule largeur nulle reste une limite valide.
        assert!(profile(0.0, 0.0, 1.0).is_finite() && profile(0.0, 1.0, 0.0).is_finite());
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

    const FG_UNITE: f64 = 2.354_820_045_030_949_3; // σ = 1
    fn gauss_ref(x: f64, fg: f64) -> f64 {
        let s = fg / (2.0 * (2.0 * std::f64::consts::LN_2).sqrt());
        libm::exp(-x * x / (2.0 * s * s)) / (s * (2.0 * std::f64::consts::PI).sqrt())
    }
    fn lorentz_ref(x: f64, fl: f64) -> f64 {
        let g = fl / 2.0;
        g / (std::f64::consts::PI * (x * x + g * g))
    }

    #[test]
    fn voigt_pic_de_reference_sigma_et_gamma_unite() {
        // V(0; σ=1, γ=1) = erfcx(1/√2)/√(2π) = 0.523126 / 2.506628 = 0.208709.
        let v = profile(0.0, FG_UNITE, 2.0);
        assert!((v - 0.208_709).abs() < 1e-6, "{v}");
    }

    #[test]
    fn voigt_egale_la_convolution_numerique() {
        // Référence indépendante : ∫ G(t)·L(x−t) dt par trapèzes.
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
            // Queue gaussienne coupée à ±12 : négligeable (> 10 σ).
            assert!((v - acc).abs() < 1e-5, "x={x} {v} vs {acc}");
        }
    }

    #[test]
    fn limites_pures_contre_formules_fermees() {
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
    fn symetrie_positivite_et_decroissance() {
        for (fg, fl) in [(1.2, 0.8), (0.5, 3.0), (4.0, 0.05)] {
            let mut prev = f64::INFINITY;
            for i in 0..=400 {
                let x = i as f64 * 0.05;
                let v = profile(x, fg, fl);
                assert!(v > 0.0 && v.is_finite(), "({fg},{fl}) x={x} {v}");
                let m = profile(-x, fg, fl);
                assert!((v - m).abs() <= 1e-15 * v.max(1.0), "symétrie x={x}");
                assert!(v <= prev + 1e-15, "({fg},{fl}) croît en x={x}");
                prev = v;
            }
        }
    }

    #[test]
    fn continuite_autour_du_seuil_1e30() {
        // σ = 1 (fwhm_g = FG_UNITE) ; le seuil est γ = σ·1e30, soit fwhm_l = 2e30.
        for x in [0.0, 1.0, 1e30] {
            for fl in [2e29, 2e30 * (1.0 - 1e-9), 2e30, 2e30 * (1.0 + 1e-9), 2e31] {
                let v = profile(x, FG_UNITE, fl);
                assert!(v.is_finite() && v > 0.0, "x={x} fl={fl} {v}");
                let ref_l = lorentz_ref(x, fl);
                assert!(
                    (v - ref_l).abs() <= 1e-9 * ref_l,
                    "x={x} fl={fl}: {v} vs {ref_l}"
                );
            }
        }
        let a = profile(0.0, FG_UNITE, 2e30 * (1.0 - 1e-12));
        let b = profile(0.0, FG_UNITE, 2e30 * (1.0 + 1e-12));
        assert!((a - b).abs() <= 1e-9 * a, "saut au seuil {a} vs {b}");
    }

    #[test]
    fn largeurs_nulles_ou_negatives_combinaisons() {
        for x in [0.0, 1.0, -3.0] {
            assert!(profile(x, 0.0, 0.0).is_nan());
            assert!(profile(x, -1.0, 0.0).is_nan());
            assert!(profile(x, 0.0, -1.0).is_nan());
            assert!(profile(x, -1.0, -1.0).is_nan());
            assert!(profile(x, -1e-300, 1.0).is_nan());
            assert!(profile(x, 1.0, -1e-300).is_nan());
            // -0.0 == 0.0 : la double largeur nulle reste NaN.
            assert!(profile(x, -0.0, -0.0).is_nan());
        }
        assert!(profile(0.0, f64::NEG_INFINITY, 1.0).is_nan());
        assert!(profile(0.0, 1.0, f64::NEG_INFINITY).is_nan());
    }

    #[test]
    fn x_non_fini_donne_nan_ou_zero() {
        for (fg, fl) in [(1.0, 1.0), (0.0, 1.0), (1.0, 0.0)] {
            assert!(profile(f64::NAN, fg, fl).is_nan(), "NaN ({fg},{fl})");
            for x in [f64::INFINITY, f64::NEG_INFINITY] {
                let v = profile(x, fg, fl);
                assert!(v == 0.0 || v.is_nan(), "x={x} ({fg},{fl}) {v}");
            }
        }
    }

    #[test]
    fn largeurs_extremes_sans_nan() {
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
        // Largeur très différente : l'ordre de grandeur suit la plus large.
        for x in [0.0, 0.5, 2.0] {
            let v = profile(x, 1e-12, 1.0);
            assert!((v - lorentz_ref(x, 1.0)).abs() < 1e-9, "x={x} {v}");
            let w = profile(x, 1.0, 1e-12);
            assert!((w - gauss_ref(x, 1.0)).abs() < 1e-9, "x={x} {w}");
        }
        // Largeurs géantes : pic ≈ 1/(π·γ) / 1/(σ√2π) selon la dominante.
        let p = profile(0.0, 1.0, 1e150);
        assert!((p / lorentz_ref(0.0, 1e150) - 1.0).abs() < 1e-9, "{p}");
        let q = profile(0.0, 1e150, 1.0);
        assert!((q / gauss_ref(0.0, 1e150) - 1.0).abs() < 1e-9, "{q}");
    }

    #[test]
    fn largeurs_toutes_deux_infimes_loin_du_centre() {
        for (fg, fl) in [(1e-160, 1e-160), (1e-300, 1e-300)] {
            let v = profile(0.5, fg, fl);
            assert!(v.is_finite() && v >= 0.0, "({fg},{fl}) {v}");
            // Queue lorentzienne γ/(π·x²), γ = fl/2.
            let attendu = fl / 2.0 / (std::f64::consts::PI * 0.25);
            assert!((v / attendu - 1.0).abs() < 1e-9, "({fg},{fl}) {v}");
        }
        assert!(profile(1000.0, 1e-300, 1e-300).is_finite());
    }

    #[test]
    fn valeurs_minuscules_de_gaussienne_ordinaires_inchangees() {
        assert!((profile(0.0, 1e-100, 1.0) - 0.636_619_772_367_581_3).abs() < 1e-12);
        assert!((profile(0.0, 1e-200, 1.0) - 0.636_619_772_367_581_3).abs() < 1e-12);
    }
}
