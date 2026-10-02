//! Échelles d'axe — tâche R5.2, moitié « logarithmique ».
//!
//! # La décision : transformer les données, pas le type de coordonnée
//!
//! Plotters sait faire un axe logarithmique par lui-même
//! ([`plotters::coord::combinators::LogRange`]), et ce n'est **pas** ce qui
//! est fait ici. La raison tient au pipeline unique du chantier
//! (`architecture-choisie.md` §8) : `LogRange` change le *type* de la
//! coordonnée, donc le type du `ChartContext`. Avec deux axes, deux
//! échelles, et un axe secondaire qui a la sienne, la fonction qui trace un
//! panneau devrait exister en **huit** monomorphisations — ou bien être
//! coupée en huit branches dont chacune redessinerait la même chose. C'est
//! exactement la divergence que le pipeline unique existe pour empêcher, et
//! elle rentrerait par la porte du système de types.
//!
//! Ici, un seul type de coordonnée — `f64` linéaire — et l'échelle est
//! appliquée **aux valeurs** : la zone de tracé raisonne en log₁₀, et seuls
//! deux endroits savent qu'il y a une échelle, l'entrée
//! ([`forward`]) et l'étiquetage ([`format_tick`]). Le prix payé est réel et
//! se dit en une ligne : les graduations sont des **décades** (et leurs
//! sous-multiples si Plotters juge bon d'en intercaler), pas les
//! graduations mineures logarithmiques que `LogRange` sait poser. Aucun
//! autre morceau du rendu — séries, barres, légende, axe secondaire — n'a à
//! savoir laquelle des deux échelles est en vigueur.
//!
//! # Les valeurs non représentables disparaissent, et c'est dit
//!
//! Sur une échelle logarithmique, zéro et les négatifs n'ont pas d'image.
//! Ils sont **retirés** du tracé, comme le sont déjà les `NaN` (la valeur
//! manquante du socle). Ce qui compte est que ce soit fait au même endroit
//! pour les deux backends, et que le cadrage n'en tienne pas compte : une
//! série entièrement négative sur un axe log ne trace rien plutôt que de
//! faire échouer la figure.

use stokesforge_plot::AxisScale;

/// Image d'une valeur dans l'espace où la zone de tracé travaille.
///
/// `None` quand la valeur n'a pas d'image : non finie, ou nulle/négative
/// sur une échelle logarithmique.
pub fn forward(scale: AxisScale, value: f64) -> Option<f64> {
    if !value.is_finite() {
        return None;
    }
    match scale {
        AxisScale::Linear => Some(value),
        AxisScale::Log => {
            if value > 0.0 {
                let t = value.log10();
                t.is_finite().then_some(t)
            } else {
                None
            }
        }
    }
}

/// Retour à l'espace des données. Réciproque de [`forward`] là où celle-ci
/// est définie.
pub fn inverse(scale: AxisScale, t: f64) -> f64 {
    match scale {
        AxisScale::Linear => t,
        AxisScale::Log => 10f64.powf(t),
    }
}

/// Vrai pour une échelle qui demande un étiquetage à part.
pub fn needs_formatter(scale: AxisScale) -> bool {
    matches!(scale, AxisScale::Log)
}

/// Élargit un intervalle logarithmique aux décades entières qui l'encadrent.
///
/// C'est ce qui fait que les graduations tombent sur 1, 10, 100 plutôt que
/// sur 3,16 et 31,6 : Plotters choisit ses points-clés dans l'espace où il
/// travaille — ici log₁₀ — et un intervalle à bornes entières lui fait
/// naturellement poser des pas entiers.
pub fn snap_to_decades((lo, hi): (f64, f64)) -> (f64, f64) {
    if !lo.is_finite() || !hi.is_finite() {
        return (0.0, 1.0);
    }
    let (low, high) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    let mut low = low.floor();
    let mut high = high.ceil();
    if high <= low {
        high = low + 1.0;
    }
    // Un fichier peut porter des bornes démesurées ; au-delà, les
    // graduations ne décrivent plus rien et `powf` sort de `f64`.
    low = low.clamp(-300.0, 300.0);
    high = high.clamp(low + 1.0, 301.0);
    if lo <= hi {
        (low, high)
    } else {
        (high, low)
    }
}

/// Combien de graduations demander pour un intervalle logarithmique donné.
///
/// Une par décade, plafonnée au nombre que Plotters vise par défaut (11) :
/// au-delà il choisit lui-même un pas de plusieurs décades, ce qui est le
/// comportement voulu.
pub fn decade_label_count((lo, hi): (f64, f64)) -> usize {
    let decades = (hi - lo).abs().round() as usize;
    decades.saturating_add(1).clamp(2, 11)
}

/// Étiquette d'une graduation, dans l'espace transformé.
///
/// Pour une échelle linéaire, rend la valeur telle quelle — la fonction
/// existe pour que l'appelant n'ait pas à se demander laquelle appeler, mais
/// le pipeline ne pose de formateur que pour les échelles qui en ont besoin
/// ([`needs_formatter`]), afin de laisser le formatage par défaut de
/// Plotters intact là où il convient.
pub fn format_tick(scale: AxisScale, t: f64) -> String {
    match scale {
        AxisScale::Linear => format_plain(t),
        AxisScale::Log => format_decade(t),
    }
}

/// `10^t`, écrit comme un lecteur l'écrirait.
///
/// Décimal tant que ça reste lisible (de 0,0001 à 1 000 000), puissance de
/// dix au-delà. Un exposant non entier — Plotters peut en produire si
/// l'intervalle est étroit — rend la valeur elle-même.
fn format_decade(t: f64) -> String {
    if !t.is_finite() {
        return String::new();
    }
    let rounded = t.round();
    if (t - rounded).abs() > 1e-9 {
        return format_plain(inverse(AxisScale::Log, t));
    }
    let exponent = rounded as i32;
    if (-4..=6).contains(&exponent) {
        format_plain(10f64.powi(exponent))
    } else {
        format!("1e{exponent}")
    }
}

/// Un nombre sans zéros inutiles, et sans notation scientifique surprise.
fn format_plain(value: f64) -> String {
    if !value.is_finite() {
        return String::new();
    }
    let text = format!("{value}");
    // `{}` sur un `f64` bascule en notation scientifique au-delà de 1e16 ;
    // les valeurs que ce chemin voit sont bornées par `snap_to_decades`.
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_lineaire_est_lidentite() {
        assert_eq!(forward(AxisScale::Linear, -3.5), Some(-3.5));
        assert_eq!(inverse(AxisScale::Linear, -3.5), -3.5);
    }

    #[test]
    fn le_log_refuse_ce_qui_na_pas_dimage() {
        assert_eq!(forward(AxisScale::Log, 0.0), None);
        assert_eq!(forward(AxisScale::Log, -1.0), None);
        assert_eq!(forward(AxisScale::Log, f64::NAN), None);
        assert_eq!(forward(AxisScale::Log, f64::INFINITY), None);
        assert_eq!(forward(AxisScale::Log, 100.0), Some(2.0));
    }

    #[test]
    fn le_non_fini_disparait_aussi_en_lineaire() {
        // Même règle des deux côtés : ce qui n'a pas de place sur l'axe n'y
        // va pas, et ce n'est pas une erreur.
        assert_eq!(forward(AxisScale::Linear, f64::NAN), None);
        assert_eq!(forward(AxisScale::Linear, f64::NEG_INFINITY), None);
    }

    #[test]
    fn les_bornes_log_sacrochent_aux_decades() {
        assert_eq!(snap_to_decades((0.3, 2.7)), (0.0, 3.0));
        // Un intervalle dégénéré doit rester un intervalle.
        assert_eq!(snap_to_decades((2.0, 2.0)), (2.0, 3.0));
        // Un axe inversé garde son sens.
        assert_eq!(snap_to_decades((2.7, 0.3)), (3.0, 0.0));
    }

    #[test]
    fn les_bornes_demesurees_sont_bornees() {
        let (lo, hi) = snap_to_decades((-1e300, 1e300));
        assert!(lo >= -300.0 && hi <= 301.0 && lo < hi, "{lo}..{hi}");
        assert!(inverse(AxisScale::Log, hi).is_finite() || hi > 300.0);
    }

    #[test]
    fn une_graduation_log_se_lit_comme_un_nombre() {
        assert_eq!(format_tick(AxisScale::Log, 0.0), "1");
        assert_eq!(format_tick(AxisScale::Log, 2.0), "100");
        assert_eq!(format_tick(AxisScale::Log, -2.0), "0.01");
        assert_eq!(format_tick(AxisScale::Log, 9.0), "1e9");
    }

    #[test]
    fn une_graduation_log_non_entiere_rend_sa_valeur() {
        let etiquette = format_tick(AxisScale::Log, 0.5);
        assert!(etiquette.starts_with("3.16"), "{etiquette}");
    }

    #[test]
    fn le_nombre_de_graduations_suit_les_decades() {
        assert_eq!(decade_label_count((0.0, 3.0)), 4);
        assert_eq!(decade_label_count((0.0, 40.0)), 11);
        assert_eq!(decade_label_count((1.0, 1.0)), 2);
    }
}
