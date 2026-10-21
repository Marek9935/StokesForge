//! Bruit pseudo-aléatoire **à graine fixe** de la démonstration.
//!
//! Pas de dépendance neuve (`rand`) : un xorshift64* local suffit à rendre des
//! mesures « réalistes » et, surtout, **les mêmes à chaque lancement**. Les
//! tests d'analyse (`demo::tests`) retrouvent les paramètres connus à une
//! tolérance, et le test de déterminisme épingle des valeurs : changer la
//! graine ou l'algorithme les fait échouer, volontairement.

/// Générateur xorshift64* : état non nul, période 2⁶⁴ − 1.
pub(super) struct Bruit {
    etat: u64,
}

impl Bruit {
    /// Une graine nulle serait un point fixe du générateur : elle est remplacée.
    pub(super) fn new(graine: u64) -> Self {
        Self {
            etat: if graine == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                graine
            },
        }
    }

    fn suivant(&mut self) -> u64 {
        let mut x = self.etat;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.etat = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Un nombre uniforme dans `[0, 1)` (53 bits de mantisse).
    pub(super) fn uniforme(&mut self) -> f64 {
        (self.suivant() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Un nombre d'espérance 0 et d'écart-type `sigma` : somme de douze
    /// uniformes (théorème central limite), sans `ln` ni `cos` à discuter.
    pub(super) fn gauss(&mut self, sigma: f64) -> f64 {
        let somme: f64 = (0..12).map(|_| self.uniforme()).sum();
        (somme - 6.0) * sigma
    }

    /// `valeurs` bruitées d'un écart-type `sigma`.
    pub(super) fn bruite(&mut self, valeurs: &[f64], sigma: f64) -> Vec<f64> {
        valeurs.iter().map(|v| v + self.gauss(sigma)).collect()
    }
}
