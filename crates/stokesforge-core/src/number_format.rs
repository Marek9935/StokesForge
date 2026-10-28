//! **Le** formateur de nombres affichés (#222, `docs/standard-interface.md`,
//! § Number display) : grille, champs, volet Results et graduations
//! passent tous par ici, jamais par un `format!` local.
//!
//! Trois niveaux, une seule règle de forme :
//!
//! - [`format_results`] : 3 chiffres significatifs (résultats, graduations) ;
//! - [`format_grid`] : 6 chiffres significatifs (grille, champs) ;
//! - [`format_exact`] : la valeur entière, point décimal — pour l'édition, le
//!   copier et la bulle des résultats. C'est la forme du modèle
//!   ([`crate::column::format_f64_for_display`]) : elle reparse à l'identique.
//!
//! La notation `e` (`1.23e-5`) prend le relais hors de [1e−4, 1e6), mesuré sur
//! la valeur **arrondie** : `999999.9` à 3 chiffres s'écrit `1e6`, pas
//! `1000000`. Les zéros de fin ne s'écrivent jamais, `-0` non plus. L'exposant
//! typographique (1×10⁻⁵) est l'affaire de la figure (Typst), pas d'ici.
//!
//! La **lecture** d'un nombre saisi est [`parse_input`] (#223) : la virgule y
//! vaut point décimal ; l'interface réécrit toujours avec le point.
//!
//! Pur : aucune dépendance d'interface, aucune locale — le séparateur est
//! toujours le point.

use crate::column::format_f64_for_display;

/// Chiffres significatifs des résultats et des graduations.
pub const RESULT_DIGITS: usize = 3;
/// Chiffres significatifs de la grille et des champs.
pub const GRID_DIGITS: usize = 6;
/// Chiffres significatifs d'une incertitude.
pub const UNCERTAINTY_DIGITS: usize = 2;

/// Exposant décimal minimal de la forme positionnelle (inclus : 1e−4).
const POSITIONAL_MIN_EXP: i32 = -4;
/// Exposant décimal maximal de la forme positionnelle (inclus : 9,99e5).
const POSITIONAL_MAX_EXP: i32 = 5;

/// Lit **un seul nombre fini** saisi à la main (#223, S-N4/S-N5 → S-R6) : « , » et
/// « . » sont deux écritures du séparateur décimal. Le texte est rogné ; la
/// virgule est lue comme un point quand elle est **seule** et sans point
/// (`1,5` → 1.5, mais `1,000` → 1 : effet assumé ; `1,000.5` ou `1,2,3` sont
/// refusés). `None` si le texte n'est pas un nombre fini.
///
/// Réservé aux champs d'un nombre et aux cellules : les formules et les listes
/// de valeurs gardent le point, la virgule y sépare.
pub fn parse_input(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    let value = if trimmed.matches(',').count() == 1 && !trimmed.contains('.') {
        trimmed.replacen(',', ".", 1).parse::<f64>().ok()?
    } else {
        trimmed.parse::<f64>().ok()?
    };
    value.is_finite().then_some(value)
}

/// Vrai si `text` (rogné) a exactement la forme `\d{1,3},\d{3}` : une saisie
/// que [`parse_input`] lit comme une décimale (`1,234` → 1.234) alors qu'un
/// lecteur anglophone y verrait un millier (#311, S-R6). Sert à l'avis discret
/// de la barre d'état après la saisie d'une cellule ; ne change pas la lecture.
pub fn looks_like_thousands_separator(text: &str) -> bool {
    let Some((head, tail)) = text.trim().split_once(',') else {
        return false;
    };
    (1..=3).contains(&head.len())
        && tail.len() == 3
        && head.bytes().all(|b| b.is_ascii_digit())
        && tail.bytes().all(|b| b.is_ascii_digit())
}

/// Résultats et graduations : 3 chiffres significatifs.
pub fn format_results(value: f64) -> String {
    format_significant(value, RESULT_DIGITS)
}

/// Grille et champs : 6 chiffres significatifs.
pub fn format_grid(value: f64) -> String {
    format_significant(value, GRID_DIGITS)
}

/// La valeur exacte, avec un point : ce qu'on édite et ce qu'on copie.
pub fn format_exact(value: f64) -> String {
    format_f64_for_display(value)
}

/// Une graduation d'axe linéaire : toutes celles d'un axe partagent le même
/// nombre de décimales (`0.0, 0.2, 0.4`, jamais `0, 0.2, 0.4`), sauf hors de
/// [1e−4, 1e6) où chacune passe en notation `e` à 3 chiffres ([`format_results`]).
/// Un zéro n'a pas de signe. Non fini : texte vide, rien à graduer.
pub fn format_tick(value: f64, decimals: usize) -> String {
    if !value.is_finite() {
        return String::new();
    }
    let text = format!("{value:.decimals$}");
    // Ce qui s'arrondit à zéro à la précision de l'axe EST le zéro (les
    // graduations s'accumulent avec du bruit : 5.5e-17 à la place de 0).
    if text
        .trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.')
    {
        return text.trim_start_matches('-').to_owned();
    }
    if (1e-4..1e6).contains(&value.abs()) {
        text
    } else {
        format_results(value)
    }
}

/// `value` arrondie à `digits` chiffres significatifs (au moins 1), sans zéro
/// inutile ; notation `e` hors de [1e−4, 1e6). Les chiffres **avant** la
/// virgule ne sont jamais arrondis en forme positionnelle (`1234.5` → `1234`,
/// pas `1230`) : seule la partie décimale est tronquée. `NaN` et ±∞ s'écrivent comme
/// `f64::to_string`.
pub fn format_significant(value: f64, digits: usize) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    if value == 0.0 {
        return "0".into();
    }
    let digits = digits.max(1);
    let (mantissa, exponent) = split_scientific(value, digits - 1);
    if (POSITIONAL_MIN_EXP..=POSITIONAL_MAX_EXP).contains(&exponent) {
        let decimals = (digits as i32 - 1 - exponent).max(0) as usize;
        trim_zeros(format!("{value:.decimals$}"))
    } else {
        format!("{}e{exponent}", trim_zeros(mantissa))
    }
}

/// Une valeur et son incertitude : l'incertitude à 2 chiffres significatifs,
/// la valeur arrondie à la même décimale (`12.346 ± 0.052`). Hors de
/// [1e−4, 1e6), les deux partagent l'exposant du plus grand des deux :
/// `(1.2346 ± 0.0052)e-5`.
///
/// Sans incertitude exploitable (absente, nulle, non finie) ou pour une valeur
/// non finie : [`format_results`].
pub fn format_with_uncertainty(value: f64, uncertainty: Option<f64>) -> String {
    let Some(error) = uncertainty
        .map(f64::abs)
        .filter(|e| e.is_finite() && *e > 0.0)
    else {
        return format_results(value);
    };
    if !value.is_finite() {
        return format_results(value);
    }
    let (_, error_exp) = split_scientific(error, UNCERTAINTY_DIGITS - 1);
    // Décimale du dernier chiffre significatif de l'incertitude.
    let last_decimal = UNCERTAINTY_DIGITS as i32 - 1 - error_exp;
    // L'exposant commun est celui du plus grand des deux : une valeur minuscule
    // devant une grande incertitude s'écrit en positionnel (`0.00 ± 0.50`).
    let value_exp = if value == 0.0 {
        error_exp
    } else {
        split_scientific(value, 14).1.max(error_exp)
    };
    if (POSITIONAL_MIN_EXP..=POSITIONAL_MAX_EXP).contains(&value_exp) {
        let (v, e) = (fixed(value, last_decimal), fixed(error, last_decimal));
        return format!("{v} ± {e}");
    }
    let scale = 10f64.powi(value_exp);
    let (v, e) = (
        fixed(value / scale, last_decimal + value_exp),
        fixed(error / scale, last_decimal + value_exp),
    );
    format!("({v} ± {e})e{value_exp}")
}

/// `(mantisse, exposant)` de `value` arrondie à `fraction_digits` décimales de
/// mantisse (`1.23e-5` → `("1.23", -5)`) — l'arrondi de Rust, qui renormalise
/// (`9.996` à 3 chiffres → `1.00e1`).
fn split_scientific(value: f64, fraction_digits: usize) -> (String, i32) {
    let text = format!("{value:.fraction_digits$e}");
    let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
    (mantissa.to_owned(), exponent.parse().unwrap_or(0))
}

/// `value` à `decimals` décimales ; une décimale négative arrondit aux
/// dizaines, centaines… (`12345` à −2 → `12300`). Jamais « -0 ».
fn fixed(value: f64, decimals: i32) -> String {
    let text = if decimals >= 0 {
        format!("{value:.*}", decimals as usize)
    } else {
        let step = 10f64.powi(-decimals);
        format!("{:.0}", (value / step).round() * step)
    };
    match text.strip_prefix('-') {
        Some(rest) if rest.chars().all(|c| c == '0' || c == '.') => rest.to_owned(),
        _ => text,
    }
}

/// Retire les zéros de fin d'une partie décimale, et le point qui resterait.
fn trim_zeros(mut text: String) -> String {
    if text.contains('.') {
        let kept = text.trim_end_matches('0').trim_end_matches('.').len();
        text.truncate(kept);
    }
    if text == "-0" {
        text = "0".into();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trois_chiffres_significatifs() {
        assert_eq!(format_results(12.3456), "12.3");
        assert_eq!(format_results(1234.5), "1234");
        assert_eq!(format_results(0.123456), "0.123");
        assert_eq!(format_results(100.0), "100");
        assert_eq!(format_results(-2.5), "-2.5");
        assert_eq!(format_results(3.0), "3");
    }

    #[test]
    fn six_chiffres_significatifs() {
        assert_eq!(format_grid(12.3456789), "12.3457");
        assert_eq!(format_grid(0.1 + 0.2), "0.3");
        assert_eq!(format_grid(42.5), "42.5");
        assert_eq!(format_grid(1234567.0), "1.23457e6");
    }

    #[test]
    fn la_notation_e_commence_a_1e6_et_finit_a_1e_moins_4() {
        // Bornes : 1e-4 est positionnel, juste en dessous non ; 1e6 est en `e`.
        assert_eq!(format_results(1e-4), "0.0001");
        assert_eq!(format_results(9.99e-5), "9.99e-5");
        assert_eq!(format_results(999_000.0), "999000");
        assert_eq!(format_results(1e6), "1e6");
        assert_eq!(format_results(-1e6), "-1e6");
        assert_eq!(format_results(0.000_012_345), "1.23e-5");
        assert_eq!(format_results(2.5e8), "2.5e8");
        assert_eq!(format_results(1.602e-19), "1.6e-19");
    }

    #[test]
    fn la_bascule_se_mesure_sur_la_valeur_arrondie() {
        assert_eq!(format_results(999_999.9), "1e6");
        assert_eq!(format_results(0.000_099_96), "0.0001");
        assert_eq!(format_results(9.996), "10");
    }

    #[test]
    fn zero_n_a_ni_signe_ni_exposant() {
        assert_eq!(format_results(0.0), "0");
        assert_eq!(format_results(-0.0), "0");
        assert_eq!(format_grid(-0.0), "0");
        assert_eq!(format_results(-1e-300), "-1e-300");
    }

    #[test]
    fn le_non_fini_s_ecrit_comme_rust() {
        assert_eq!(format_results(f64::NAN), "NaN");
        assert_eq!(format_results(f64::INFINITY), "inf");
        assert_eq!(format_grid(f64::NEG_INFINITY), "-inf");
        assert_eq!(format_with_uncertainty(f64::NAN, Some(0.1)), "NaN");
    }

    #[test]
    fn les_graduations_partagent_leurs_decimales_sauf_hors_plage() {
        assert_eq!(format_tick(0.0, 1), "0.0");
        assert_eq!(format_tick(-0.0, 1), "0.0");
        assert_eq!(format_tick(-1e-12, 1), "0.0");
        assert_eq!(format_tick(-3e-12, 12), "-3e-12");
        assert_eq!(format_tick(0.2, 2), "0.20");
        assert_eq!(format_tick(200.0, 0), "200");
        assert_eq!(format_tick(-0.04, 0), "0");
        assert_eq!(format_tick(2e-5, 5), "2e-5");
        assert_eq!(format_tick(2_500_000.0, 0), "2.5e6");
        assert_eq!(format_tick(1e-4, 4), "0.0001");
        assert_eq!(format_tick(f64::NAN, 1), "");
    }

    #[test]
    fn la_valeur_exacte_reparse_a_l_identique() {
        for v in [
            0.1 + 0.2,
            1.0 / 3.0,
            1.602_176_634e-19,
            12.3456789,
            1e22,
            -5.0,
        ] {
            let text = format_exact(v);
            assert_eq!(
                text.parse::<f64>().unwrap().to_bits(),
                v.to_bits(),
                "{text}"
            );
            assert!(!text.contains(','));
        }
        assert_eq!(format_exact(42.5), "42.5");
    }

    #[test]
    fn l_incertitude_a_deux_chiffres_et_la_valeur_la_meme_decimale() {
        assert_eq!(
            format_with_uncertainty(12.3456, Some(0.0523)),
            "12.346 ± 0.052"
        );
        assert_eq!(format_with_uncertainty(12.5, Some(0.25)), "12.50 ± 0.25");
        assert_eq!(format_with_uncertainty(0.5, Some(1.0)), "0.5 ± 1.0");
        assert_eq!(
            format_with_uncertainty(-3.14166, Some(0.0012)),
            "-3.1417 ± 0.0012"
        );
    }

    #[test]
    fn une_grande_incertitude_arrondit_aux_dizaines() {
        assert_eq!(
            format_with_uncertainty(12_345.0, Some(123.0)),
            "12350 ± 120"
        );
        assert_eq!(
            format_with_uncertainty(12_345.0, Some(1234.0)),
            "12300 ± 1200"
        );
    }

    #[test]
    fn hors_de_la_plage_les_deux_partagent_l_exposant() {
        assert_eq!(
            format_with_uncertainty(1.234_56e-5, Some(5.2e-8)),
            "(1.2346 ± 0.0052)e-5"
        );
        assert_eq!(
            format_with_uncertainty(2.5e8, Some(3e6)),
            "(2.500 ± 0.030)e8"
        );
    }

    #[test]
    fn sans_incertitude_exploitable_trois_chiffres() {
        for none in [
            None,
            Some(0.0),
            Some(-0.0),
            Some(f64::NAN),
            Some(f64::INFINITY),
        ] {
            assert_eq!(format_with_uncertainty(12.3456, none), "12.3", "{none:?}");
        }
    }

    #[test]
    fn une_valeur_nulle_garde_la_decimale_de_l_incertitude() {
        assert_eq!(format_with_uncertainty(0.0, Some(0.034)), "0.000 ± 0.034");
        assert_eq!(format_with_uncertainty(-1e-9, Some(0.5)), "0.00 ± 0.50");
    }

    #[test]
    fn la_virgule_seule_vaut_le_point_decimal() {
        assert_eq!(parse_input("1,5"), Some(1.5));
        assert_eq!(parse_input(" -0,25 "), Some(-0.25));
        assert_eq!(parse_input("1e-3"), Some(0.001));
        assert_eq!(parse_input("2,5e3"), Some(2500.0));
    }

    #[test]
    fn le_point_reste_lu_comme_avant_et_mille_vaut_un() {
        assert_eq!(parse_input("1.5"), Some(1.5));
        // Effet assumé (S-R6) : « 1,000 » est lu 1.
        assert_eq!(parse_input("1,000"), Some(1.0));
    }

    #[test]
    fn la_forme_milliers_est_reconnue_exactement() {
        for oui in ["1,234", "12,345", "999,000", " 1,234 "] {
            assert!(looks_like_thousands_separator(oui), "{oui:?}");
        }
        for non in [
            "1,5", "1234,567", "12,3456", "1.234", "1,234.5", "", ",", "a,123", "1,23a", "-1,234",
        ] {
            assert!(!looks_like_thousands_separator(non), "{non:?}");
        }
    }

    #[test]
    fn l_ambigu_et_le_non_fini_sont_refuses() {
        for refused in [
            "1,000.5", "1.000,5", "1,2,3", "", ",", "a,5", "1e400", "nan", "inf",
        ] {
            assert_eq!(parse_input(refused), None, "{refused:?}");
        }
    }

    #[test]
    fn une_cellule_mixte_lit_la_virgule_comme_un_nombre() {
        use crate::column::MixedCell;
        assert_eq!(
            MixedCell::from_text(Some("1,5".into())),
            MixedCell::Number(1.5)
        );
        assert!(matches!(
            MixedCell::from_text(Some("1,5 V".into())),
            MixedCell::Text(_)
        ));
    }
}
