//! Nuanciers (#178) : des dégradés de couleurs qu'on répartit sur les courbes
//! d'un groupe (#177).
//!
//! Un nuancier est une suite d'**arrêts** `#RRGGBB` (au moins un) ; les
//! couleurs des `n` courbes d'un groupe sont prises à intervalles réguliers
//! entre le premier et le dernier arrêt, quel que soit `n`, par interpolation
//! linéaire des composantes RGB. Les nuanciers fournis sont des données
//! (`GRADIENT_PRESETS`) ; leur nom affiché est une clé du catalogue de
//! l'application, jamais un texte d'ici.

use crate::style::parse_hex;

/// Un nuancier fourni : un identifiant stable (clé de libellé de
/// l'application) et ses arrêts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GradientPreset {
    /// Identifiant stable, en minuscules : `blue-red`.
    pub id: &'static str,
    /// Arrêts `#RRGGBB`, du premier au dernier.
    pub stops: &'static [&'static str],
}

/// Les nuanciers fournis.
pub const GRADIENT_PRESETS: [GradientPreset; 8] = [
    GradientPreset {
        id: "blue-red",
        stops: &["#0000ff", "#ff0000"],
    },
    GradientPreset {
        id: "black-red",
        stops: &["#000000", "#ff0000"],
    },
    GradientPreset {
        id: "black-blue",
        stops: &["#000000", "#0000ff"],
    },
    GradientPreset {
        id: "blue-yellow",
        stops: &["#0000ff", "#ffd700"],
    },
    GradientPreset {
        id: "green-red",
        stops: &["#008000", "#ff0000"],
    },
    GradientPreset {
        id: "blue-white-red",
        stops: &["#0000ff", "#ffffff", "#ff0000"],
    },
    GradientPreset {
        id: "viridis",
        stops: &["#440154", "#3e4989", "#26828e", "#35b779", "#fde725"],
    },
    GradientPreset {
        id: "plasma",
        stops: &["#0d0887", "#9c179e", "#ed7953", "#f0f921"],
    },
];

/// Les `count` couleurs `#RRGGBB` d'un nuancier d'arrêts `stops`, régulièrement
/// espacées entre le premier et le dernier arrêt.
///
/// - `count == 0` : liste vide ; `count == 1` : le premier arrêt ;
/// - un arrêt illisible est ignoré ; sans aucun arrêt lisible : liste vide
///   (l'appelant ne change alors aucune couleur) ;
/// - l'opacité d'un arrêt `#RRGGBBAA` est ignorée : un nuancier colore, il ne
///   rend pas transparent.
pub fn gradient_colors<S: AsRef<str>>(stops: &[S], count: usize) -> Vec<String> {
    let stops: Vec<[u8; 3]> = stops
        .iter()
        .filter_map(|stop| parse_hex(stop.as_ref()))
        .map(|[r, g, b, _]| [r, g, b])
        .collect();
    let Some(first) = stops.first() else {
        return Vec::new();
    };
    (0..count)
        .map(|index| {
            let rgb = if count == 1 || stops.len() == 1 {
                *first
            } else {
                let position = index as f64 / (count - 1) as f64 * (stops.len() - 1) as f64;
                let lower = (position.floor() as usize).min(stops.len() - 2);
                let t = position - lower as f64;
                let mix = |channel: usize| {
                    let (a, b) = (
                        f64::from(stops[lower][channel]),
                        f64::from(stops[lower + 1][channel]),
                    );
                    (a + (b - a) * t).round().clamp(0.0, 255.0) as u8
                };
                [mix(0), mix(1), mix(2)]
            };
            format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_extremites_sont_les_premier_et_dernier_arrets() {
        let colors = gradient_colors(&["#0000ff", "#ff0000"], 5);
        assert_eq!(colors.len(), 5);
        assert_eq!(colors[0], "#0000ff");
        assert_eq!(colors[4], "#ff0000");
        assert_eq!(colors[2], "#800080");
    }

    #[test]
    fn un_arret_intermediaire_est_atteint_exactement() {
        let colors = gradient_colors(&["#000000", "#ffffff", "#ff0000"], 3);
        assert_eq!(colors, ["#000000", "#ffffff", "#ff0000"]);
    }

    #[test]
    fn une_ou_zero_courbe() {
        assert_eq!(gradient_colors(&["#102030", "#ffffff"], 1), ["#102030"]);
        assert!(gradient_colors(&["#102030", "#ffffff"], 0).is_empty());
    }

    #[test]
    fn arrets_illisibles_ignores_et_opacite_ecartee() {
        assert!(gradient_colors(&["nope"], 3).is_empty());
        assert_eq!(
            gradient_colors(&["nope", "#11223380"], 2),
            ["#112233", "#112233"]
        );
    }

    #[test]
    fn chaque_nuancier_fourni_est_lisible() {
        for preset in GRADIENT_PRESETS {
            assert!(preset.stops.len() >= 2, "{}", preset.id);
            assert_eq!(gradient_colors(preset.stops, 4).len(), 4, "{}", preset.id);
        }
    }
}
