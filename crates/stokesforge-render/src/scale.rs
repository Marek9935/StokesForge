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

use stokesforge_core::number_format::format_tick as format_linear_tick;
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

/// Nombre de graduations que Plotters vise par défaut sur un axe.
pub const DEFAULT_LABELS: usize = 11;

/// Combien de graduations demander : une par décade sur une échelle
/// logarithmique ([`decade_label_count`]), le défaut de Plotters sinon.
pub fn label_count(scale: AxisScale, range: (f64, f64)) -> usize {
    match scale {
        AxisScale::Linear => DEFAULT_LABELS,
        AxisScale::Log => decade_label_count(range),
    }
}

/// Les graduations que Plotters posera sur un intervalle linéaire.
pub fn linear_key_points(range: (f64, f64), labels: usize) -> Vec<f64> {
    use plotters::coord::ranged1d::{BoldPoints, Ranged};
    use plotters::coord::types::RangedCoordf64;
    if !range.0.is_finite() || !range.1.is_finite() {
        return Vec::new();
    }
    RangedCoordf64::from(range.0..range.1).key_points(BoldPoints(labels))
}

/// Le plus petit nombre de décimales qui écrit toutes les graduations d'un
/// axe sans les altérer : 0 quand elles sont toutes entières.
fn decimals_for(points: &[f64]) -> usize {
    (0..=10)
        .find(|&decimals| {
            let factor = 10f64.powi(decimals as i32);
            points.iter().all(|value| {
                let scaled = value * factor;
                (scaled - scaled.round()).abs() <= 1e-6 * scaled.abs().max(1.0)
            })
        })
        .unwrap_or(10)
}

/// Comment écrire les graduations d'un axe : le même nombre de décimales
/// partout, aucune quand toutes sont entières (« 0, 100, 200 », pas
/// « 0.0, 100.0, 200.0 » que donne le formatage par défaut de Plotters).
#[derive(Debug, Clone, Copy)]
pub struct TickFormat {
    scale: AxisScale,
    decimals: usize,
    comma: bool,
}

impl TickFormat {
    /// Le format de l'axe de cet intervalle (dans l'espace transformé).
    pub fn new(scale: AxisScale, range: (f64, f64)) -> Self {
        let decimals = match scale {
            AxisScale::Linear => decimals_for(&linear_key_points(range, label_count(scale, range))),
            AxisScale::Log => 0,
        };
        Self {
            scale,
            decimals,
            comma: false,
        }
    }

    /// Écrit la virgule comme séparateur décimal (#223, réglage du graphe).
    pub fn with_decimal_comma(mut self, comma: bool) -> Self {
        self.comma = comma;
        self
    }

    /// L'étiquette d'une graduation, dans l'espace transformé.
    pub fn format(&self, t: f64) -> String {
        let text = match self.scale {
            AxisScale::Linear => format_linear_tick(t, self.decimals),
            AxisScale::Log => format_decade(t),
        };
        if self.comma {
            text.replace('.', ",")
        } else {
            text
        }
    }
}

/// Cadre un axe linéaire de calque superposé (#174) pour que ses graduations
/// tombent **en face** de celles de l'axe du calque recouvert.
///
/// `base` est l'intervalle de l'axe recouvert, `wanted` celui qui ne ferait
/// qu'envelopper les données du calque ; tous deux dans le même sens. On
/// cherche un pas « rond » et une position de la première graduation tels que
/// l'intervalle obtenu contienne `wanted`, compte autant de graduations que
/// `base` et les pose aux mêmes fractions de l'axe. Chaque candidat est
/// vérifié avec les graduations que Plotters posera réellement : on ne devine
/// pas son algorithme. `None` si rien ne convient (l'appelant garde alors
/// `wanted`).
pub fn aligned_range(base: (f64, f64), wanted: (f64, f64)) -> Option<(f64, f64)> {
    let inverted = base.0 > base.1;
    if (wanted.0 > wanted.1) != inverted {
        return None;
    }
    let (b0, b1) = if inverted { (base.1, base.0) } else { base };
    let (d0, d1) = if inverted {
        (wanted.1, wanted.0)
    } else {
        wanted
    };
    if ![b0, b1, d0, d1].iter().all(|v| v.is_finite()) || b1 <= b0 || d1 <= d0 {
        return None;
    }
    let base_points = linear_key_points((b0, b1), DEFAULT_LABELS);
    if base_points.len() < 2 {
        return None;
    }
    let base_step = base_points[1] - base_points[0];
    if base_step <= 0.0 {
        return None;
    }
    // Les graduations de base, en fraction de l'axe.
    let fractions: Vec<f64> = base_points.iter().map(|t| (t - b0) / (b1 - b0)).collect();
    let steps_in_axis = (b1 - b0) / base_step;
    let first_offset = (base_points[0] - b0) / base_step;

    let smallest = (d1 - d0) / steps_in_axis;
    let decade = 10f64.powf(smallest.log10().floor());
    const MANTISSAS: [f64; 11] = [1.0, 1.25, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 7.5, 8.0];
    for exponent in [1.0, 10.0, 100.0] {
        for mantissa in MANTISSAS {
            let step = decade * exponent * mantissa;
            let span = steps_in_axis * step;
            if span < d1 - d0 {
                continue;
            }
            // Première graduation `j · step`, placée à `first_offset` pas du
            // bord bas : le bord bas vaut `(j − first_offset) · step`.
            let lowest = (d1 - span) / step + first_offset;
            let highest = d0 / step + first_offset;
            let (lo_j, hi_j) = ((lowest - 1e-9).ceil(), (highest + 1e-9).floor());
            if lo_j > hi_j {
                continue;
            }
            let j = ((lowest + highest) / 2.0).round().clamp(lo_j, hi_j);
            let low = (j - first_offset) * step;
            let high = low + span;
            let points = linear_key_points((low, high), DEFAULT_LABELS);
            let aligned = points.len() == fractions.len()
                && points
                    .iter()
                    .zip(&fractions)
                    .all(|(t, f)| ((t - low) / (high - low) - f).abs() < 1e-6);
            if aligned {
                return Some(if inverted { (high, low) } else { (low, high) });
            }
        }
    }
    None
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

    fn ticks(range: (f64, f64)) -> Vec<String> {
        let format = TickFormat::new(AxisScale::Linear, range);
        linear_key_points(range, DEFAULT_LABELS)
            .into_iter()
            .map(|t| format.format(t))
            .collect()
    }

    #[test]
    fn des_graduations_entieres_s_ecrivent_sans_decimale() {
        let labels = ticks((-40.0, 840.0));
        assert!(labels.contains(&"0".to_owned()), "{labels:?}");
        assert!(labels.contains(&"200".to_owned()), "{labels:?}");
        assert!(labels.iter().all(|l| !l.contains('.')), "{labels:?}");
    }

    #[test]
    fn des_graduations_fractionnaires_gardent_le_meme_nombre_de_decimales() {
        let labels = ticks((-0.6, 0.6));
        assert!(labels.contains(&"0.0".to_owned()), "{labels:?}");
        assert!(labels.contains(&"-0.2".to_owned()), "{labels:?}");
        assert!(labels
            .iter()
            .all(|l| l.ends_with(|c: char| c.is_ascii_digit())));
    }

    #[test]
    fn un_zero_n_a_pas_de_signe() {
        assert_eq!(format_linear_tick(-0.0, 0), "0");
        assert_eq!(format_linear_tick(-1e-12, 1), "0.0");
        assert_eq!(format_linear_tick(-2.0, 0), "-2");
    }

    #[test]
    fn un_axe_aligne_pose_ses_graduations_en_face() {
        let base = (-40.0, 840.0);
        let wanted = (-6.0, 126.0);
        let range = aligned_range(base, wanted).expect("un cadrage aligné");
        assert!(range.0 <= wanted.0 && range.1 >= wanted.1, "{range:?}");
        let fractions = |r: (f64, f64)| -> Vec<f64> {
            linear_key_points(r, DEFAULT_LABELS)
                .iter()
                .map(|t| (t - r.0) / (r.1 - r.0))
                .collect()
        };
        let (a, b) = (fractions(base), fractions(range));
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert!((x - y).abs() < 1e-6, "{a:?} / {b:?}");
        }
    }

    #[test]
    fn l_alignement_respecte_le_sens_de_l_axe() {
        let inverted = aligned_range((840.0, -40.0), (126.0, -6.0)).expect("aligné");
        assert!(inverted.0 > inverted.1);
        assert_eq!(aligned_range((-40.0, 840.0), (126.0, -6.0)), None);
    }

    #[test]
    fn la_virgule_remplace_le_point_dans_les_graduations() {
        let lineaire = TickFormat::new(AxisScale::Linear, (0.0, 1.0));
        let virgule = lineaire.with_decimal_comma(true);
        assert_eq!(lineaire.format(0.5), "0.5");
        assert_eq!(virgule.format(0.5), "0,5");
        // Une échelle log écrit aussi ses puissances décimales avec la virgule.
        let log = TickFormat::new(AxisScale::Log, (-2.0, 2.0)).with_decimal_comma(true);
        assert_eq!(log.format(-1.0), "0,1");
    }
}
