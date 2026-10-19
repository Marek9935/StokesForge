//! Le niveau d'alerte d'un R² (#288), **sans egui** : l'état dit « orange »,
//! « rouge » ou « boîte de dialogue » ; `stokesforge-app` choisit la teinte
//! (`theme::r_squared_color`) et peint la boîte.
//!
//! Les paliers se cumulent : un R² sous [`DIALOG_BELOW`] est aussi rouge et
//! aussi orange. Les bornes sont strictes (`0,95` pile n'avertit pas) ; un R²
//! négatif — ajustement pire qu'une droite horizontale — relève du palier le
//! plus bas. `NaN` (pas de valeur) n'avertit de rien : on n'alerte pas sur ce
//! qu'on ne sait pas mesurer.

use stokesforge_core::Calculation;

/// La clé des résultats qui porte R² (ajustements et pics).
pub const R_SQUARED_KEY: &str = "stat.r_squared";

/// Sous cette valeur (stricte), R² s'écrit en orange.
pub const ORANGE_BELOW: f64 = 0.95;
/// Sous cette valeur (stricte), R² s'écrit en rouge.
pub const RED_BELOW: f64 = 0.8;
/// Sous cette valeur (stricte), R² ouvre aussi la boîte de dialogue, si ni la
/// préférence de l'application ni le réglage du calcul ne l'ont coupée.
pub const DIALOG_BELOW: f64 = 0.6;

/// Le palier atteint par un R², du plus doux au plus fort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RSquaredLevel {
    /// Aucun avertissement (bon R², ou `NaN`).
    Good,
    /// `R² < 0,95`.
    Orange,
    /// `R² < 0,8`.
    Red,
    /// `R² < 0,6` : rouge, et boîte de dialogue.
    Dialog,
}

impl RSquaredLevel {
    /// Le palier de `r_squared`.
    pub fn of(r_squared: f64) -> Self {
        if r_squared.is_nan() {
            Self::Good
        } else if r_squared < DIALOG_BELOW {
            Self::Dialog
        } else if r_squared < RED_BELOW {
            Self::Red
        } else if r_squared < ORANGE_BELOW {
            Self::Orange
        } else {
            Self::Good
        }
    }

    /// La valeur se peint-elle en couleur (orange ou plus) ?
    pub fn is_warning(self) -> bool {
        self >= Self::Orange
    }

    /// La valeur se peint-elle en rouge (rouge ou plus) ?
    pub fn is_red(self) -> bool {
        self >= Self::Red
    }
}

/// Le palier d'une valeur calculée : celui de son R² si `key` est
/// [`R_SQUARED_KEY`], sinon [`RSquaredLevel::Good`].
pub fn level_of_value(key: &str, value: f64) -> RSquaredLevel {
    if key == R_SQUARED_KEY {
        RSquaredLevel::of(value)
    } else {
        RSquaredLevel::Good
    }
}

/// Les R² du calcul qui franchissent le palier de la boîte, par série, en bits
/// (une empreinte exacte : deux recalculs au même résultat n'avertissent pas
/// deux fois). Vide : aucun défaut.
pub fn dialog_fingerprint(calc: &Calculation) -> Vec<(usize, u64)> {
    let mut out = Vec::new();
    for (rank, result) in calc.results.iter().enumerate() {
        for value in &result.values {
            if level_of_value(&value.key, value.value) == RSquaredLevel::Dialog {
                out.push((rank, value.value.to_bits()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_paliers_ont_des_bornes_strictes() {
        use RSquaredLevel::*;
        let cas = [
            (1.0, Good),
            (0.95, Good),
            (0.9499, Orange),
            (0.8, Orange),
            (0.7999, Red),
            (0.6, Red),
            (0.5999, Dialog),
            (0.0, Dialog),
            (-3.0, Dialog),
            (f64::NEG_INFINITY, Dialog),
            (f64::NAN, Good),
        ];
        for (r2, attendu) in cas {
            assert_eq!(RSquaredLevel::of(r2), attendu, "R² = {r2}");
        }
    }

    #[test]
    fn les_paliers_se_cumulent() {
        let dialog = RSquaredLevel::of(0.1);
        assert!(dialog.is_warning() && dialog.is_red());
        let orange = RSquaredLevel::of(0.9);
        assert!(orange.is_warning() && !orange.is_red());
        assert!(!RSquaredLevel::of(0.99).is_warning());
    }

    #[test]
    fn seule_la_cle_r_carre_est_jugee() {
        assert_eq!(level_of_value("param.tau", 0.1), RSquaredLevel::Good);
        assert_eq!(level_of_value(R_SQUARED_KEY, 0.1), RSquaredLevel::Dialog);
    }
}
