//! **The** formatter of displayed numbers (#222, `docs/standard-interface.md`,
//! § Number display): grid, fields, Results pane and ticks
//! all go through here, never through a local `format!`.
//!
//! Three levels, one single shape rule:
//!
//! - [`format_results`]: 3 significant digits (results, ticks);
//! - [`format_grid`]: 6 significant digits (grid, fields);
//! - [`format_exact`]: the whole value, decimal point — for editing,
//!   copying and the results tooltip. It is the model's form
//!   ([`crate::column::format_f64_for_display`]): it parses back identically.
//!
//! The `e` notation (`1.23e-5`) takes over outside [1e−4, 1e6), measured on
//! the **rounded** value: `999999.9` at 3 digits is written `1e6`, not
//! `1000000`. Trailing zeros are never written, nor is `-0`. The typographic
//! exponent (1×10⁻⁵) is the figure's business (Typst), not this module's.
//!
//! **Reading** a typed number is [`parse_input`] (#223): the comma there
//! counts as a decimal point; the interface always writes back with the point.
//!
//! Pure: no interface dependency, no locale — the separator is
//! always the point.

use crate::column::format_f64_for_display;

/// Significant digits of results and ticks.
pub const RESULT_DIGITS: usize = 3;
/// Significant digits of the grid and fields.
pub const GRID_DIGITS: usize = 6;
/// Significant digits of an uncertainty.
pub const UNCERTAINTY_DIGITS: usize = 2;

/// Minimum decimal exponent of the positional form (inclusive: 1e−4).
const POSITIONAL_MIN_EXP: i32 = -4;
/// Maximum decimal exponent of the positional form (inclusive: 9.99e5).
const POSITIONAL_MAX_EXP: i32 = 5;

/// Reads **a single finite number** typed by hand (#223, S-N4/S-N5 → S-R6): "," and
/// "." are two spellings of the decimal separator. The text is trimmed; the
/// comma is read as a point when it is **alone** and there is no point
/// (`1,5` → 1.5, but `1,000` → 1: accepted effect; `1,000.5` or `1,2,3` are
/// refused). `None` if the text is not a finite number.
///
/// Reserved for number fields and cells: formulas and value lists
/// keep the point, the comma separates there.
pub fn parse_input(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    let value = if trimmed.matches(',').count() == 1 && !trimmed.contains('.') {
        trimmed.replacen(',', ".", 1).parse::<f64>().ok()?
    } else {
        trimmed.parse::<f64>().ok()?
    };
    value.is_finite().then_some(value)
}

/// True if `text` (trimmed) has exactly the form `\d{1,3},\d{3}`: an input
/// that [`parse_input`] reads as a decimal (`1,234` → 1.234) while an
/// English reader would see a thousand (#311, S-R6). Used for the discreet notice
/// of the status bar after a cell is entered; does not change the reading.
pub fn looks_like_thousands_separator(text: &str) -> bool {
    let Some((head, tail)) = text.trim().split_once(',') else {
        return false;
    };
    (1..=3).contains(&head.len())
        && tail.len() == 3
        && head.bytes().all(|b| b.is_ascii_digit())
        && tail.bytes().all(|b| b.is_ascii_digit())
}

/// Results and ticks: 3 significant digits.
pub fn format_results(value: f64) -> String {
    format_significant(value, RESULT_DIGITS)
}

/// Grid and fields: 6 significant digits.
pub fn format_grid(value: f64) -> String {
    format_significant(value, GRID_DIGITS)
}

/// The exact value, with a point: what is edited and what is copied.
pub fn format_exact(value: f64) -> String {
    format_f64_for_display(value)
}

/// A linear axis tick: all those of an axis share the same
/// number of decimals (`0.0, 0.2, 0.4`, never `0, 0.2, 0.4`), except outside
/// [1e−4, 1e6) where each one switches to `e` notation at 3 digits ([`format_results`]).
/// A zero has no sign. Not finite: empty text, nothing to tick.
pub fn format_tick(value: f64, decimals: usize) -> String {
    if !value.is_finite() {
        return String::new();
    }
    let text = format!("{value:.decimals$}");
    // What rounds to zero at the axis precision IS zero (ticks
    // accumulate noise: 5.5e-17 in place of 0).
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

/// `value` rounded to `digits` significant digits (at least 1), with no useless
/// zero; `e` notation outside [1e−4, 1e6). The digits **before** the
/// point are never rounded in positional form (`1234.5` → `1234`,
/// not `1230`): only the decimal part is truncated. `NaN` and ±∞ are written like
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

/// A value and its uncertainty: the uncertainty at 2 significant digits,
/// the value rounded to the same decimal (`12.346 ± 0.052`). Outside
/// [1e−4, 1e6), both share the exponent of the larger of the two:
/// `(1.2346 ± 0.0052)e-5`.
///
/// With no usable uncertainty (absent, zero, not finite) or for a value
/// not finite: [`format_results`].
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
    // Decimal of the last significant digit of the uncertainty.
    let last_decimal = UNCERTAINTY_DIGITS as i32 - 1 - error_exp;
    // The common exponent is that of the larger of the two: a tiny value
    // next to a large uncertainty is written positionally (`0.00 ± 0.50`).
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

/// `(mantissa, exponent)` of `value` rounded to `fraction_digits` decimals of
/// mantissa (`1.23e-5` → `("1.23", -5)`) — Rust's rounding, which renormalizes
/// (`9.996` at 3 digits → `1.00e1`).
fn split_scientific(value: f64, fraction_digits: usize) -> (String, i32) {
    let text = format!("{value:.fraction_digits$e}");
    let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
    (mantissa.to_owned(), exponent.parse().unwrap_or(0))
}

/// `value` to `decimals` decimals; a negative decimal rounds to
/// tens, hundreds… (`12345` at −2 → `12300`). Never "-0".
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

/// Removes the trailing zeros of a decimal part, and the point that would remain.
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
    fn three_significant_digits() {
        assert_eq!(format_results(12.3456), "12.3");
        assert_eq!(format_results(1234.5), "1234");
        assert_eq!(format_results(0.123456), "0.123");
        assert_eq!(format_results(100.0), "100");
        assert_eq!(format_results(-2.5), "-2.5");
        assert_eq!(format_results(3.0), "3");
    }

    #[test]
    fn six_significant_digits() {
        assert_eq!(format_grid(12.3456789), "12.3457");
        assert_eq!(format_grid(0.1 + 0.2), "0.3");
        assert_eq!(format_grid(42.5), "42.5");
        assert_eq!(format_grid(1234567.0), "1.23457e6");
    }

    #[test]
    fn e_notation_starts_at_1e6_and_ends_at_1e_minus_4() {
        // Bounds: 1e-4 is positional, just below it is not; 1e6 is in `e`.
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
    fn the_switch_is_measured_on_the_rounded_value() {
        assert_eq!(format_results(999_999.9), "1e6");
        assert_eq!(format_results(0.000_099_96), "0.0001");
        assert_eq!(format_results(9.996), "10");
    }

    #[test]
    fn zero_has_neither_sign_nor_exponent() {
        assert_eq!(format_results(0.0), "0");
        assert_eq!(format_results(-0.0), "0");
        assert_eq!(format_grid(-0.0), "0");
        assert_eq!(format_results(-1e-300), "-1e-300");
    }

    #[test]
    fn non_finite_is_written_like_rust() {
        assert_eq!(format_results(f64::NAN), "NaN");
        assert_eq!(format_results(f64::INFINITY), "inf");
        assert_eq!(format_grid(f64::NEG_INFINITY), "-inf");
        assert_eq!(format_with_uncertainty(f64::NAN, Some(0.1)), "NaN");
    }

    #[test]
    fn ticks_share_their_decimals_except_out_of_range() {
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
    fn the_exact_value_parses_back_identically() {
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
    fn the_uncertainty_has_two_digits_and_the_value_the_same_decimal() {
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
    fn a_large_uncertainty_rounds_to_tens() {
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
    fn out_of_range_both_share_the_exponent() {
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
    fn without_a_usable_uncertainty_three_digits() {
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
    fn a_zero_value_keeps_the_decimal_of_the_uncertainty() {
        assert_eq!(format_with_uncertainty(0.0, Some(0.034)), "0.000 ± 0.034");
        assert_eq!(format_with_uncertainty(-1e-9, Some(0.5)), "0.00 ± 0.50");
    }

    #[test]
    fn a_lone_comma_counts_as_the_decimal_point() {
        assert_eq!(parse_input("1,5"), Some(1.5));
        assert_eq!(parse_input(" -0,25 "), Some(-0.25));
        assert_eq!(parse_input("1e-3"), Some(0.001));
        assert_eq!(parse_input("2,5e3"), Some(2500.0));
    }

    #[test]
    fn the_point_is_read_as_before_and_1_000_is_read_as_one() {
        assert_eq!(parse_input("1.5"), Some(1.5));
        // Accepted effect (S-R6): "1,000" is read as 1.
        assert_eq!(parse_input("1,000"), Some(1.0));
    }

    #[test]
    fn the_thousands_form_is_recognized_exactly() {
        for yes in ["1,234", "12,345", "999,000", " 1,234 "] {
            assert!(looks_like_thousands_separator(yes), "{yes:?}");
        }
        for no in [
            "1,5", "1234,567", "12,3456", "1.234", "1,234.5", "", ",", "a,123", "1,23a", "-1,234",
        ] {
            assert!(!looks_like_thousands_separator(no), "{no:?}");
        }
    }

    #[test]
    fn the_ambiguous_and_the_non_finite_are_refused() {
        for refused in [
            "1,000.5", "1.000,5", "1,2,3", "", ",", "a,5", "1e400", "nan", "inf",
        ] {
            assert_eq!(parse_input(refused), None, "{refused:?}");
        }
    }

    #[test]
    fn a_mixed_cell_reads_the_comma_as_a_number() {
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
