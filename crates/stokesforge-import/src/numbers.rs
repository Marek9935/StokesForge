//! Number reading: decimal separator (fixed or detected), thousands
//! separator (with a space as thousands separator: U+00A0 and U+202F too),
//! missing-value tokens. A non-finite result (`inf`, `Infinity`, `1e400`) is
//! not a number here: `NaN`, so the token is kept as text. A [`NumberFormat`]
//! is resolved **once** per file, on the same lines for
//! [`crate::parse::parse`] and [`crate::preview::preview`]: that is what keeps
//! them in agreement.

use crate::recipe::{DecimalStyle, ImportRecipe};

/// Number of lines examined to detect the decimal separator.
const DETECTION_LINES: usize = 50;

/// Number reading rules, already resolved for a file.
#[derive(Debug, Clone)]
pub(crate) struct NumberFormat {
    /// Never `Auto`: resolved to `Dot` or `Comma`.
    pub decimal: DecimalStyle,
    thousands: Option<char>,
    /// Tokens (lowercase, trimmed) that mean "missing value": read as empty,
    /// not as text to keep.
    missing: Vec<String>,
}

impl NumberFormat {
    /// Resolves a recipe's format from `sample` (the first data lines, with
    /// `skip_chars` already removed).
    pub fn resolve<'a>(recipe: &ImportRecipe, sample: impl Iterator<Item = &'a str>) -> Self {
        Self::new(
            recipe.decimal,
            recipe.thousands,
            &recipe.missing_tokens,
            sample,
        )
    }

    pub fn new<'a>(
        decimal: DecimalStyle,
        thousands: Option<char>,
        missing: &[String],
        sample: impl Iterator<Item = &'a str>,
    ) -> Self {
        let decimal = match decimal {
            DecimalStyle::Auto => detect_decimal(sample.take(DETECTION_LINES)),
            fixed => fixed,
        };
        Self {
            decimal,
            thousands,
            missing: missing
                .iter()
                .map(|token| token.trim().to_lowercase())
                .filter(|token| !token.is_empty())
                .collect(),
        }
    }

    /// Is this (non-empty) token a missing-value marker?
    pub fn is_missing(&self, token: &str) -> bool {
        let token = token.trim().to_lowercase();
        !token.is_empty() && self.missing.contains(&token)
    }

    /// The recipe's thousands separator.
    pub fn thousands(&self) -> Option<char> {
        self.thousands
    }

    /// Parses a token into an `f64`; `NaN` for anything unsuitable, never an
    /// error (invariant 10). A non-finite result (`inf`, `Infinity`, `-inf`,
    /// `1e400`) is also `NaN`: the token then stays visible as text (`Mixed`
    /// column), instead of entering a numeric column as `±inf`.
    pub fn parse(&self, token: &str) -> f64 {
        let value = self.parse_raw(token);
        if value.is_finite() {
            value
        } else {
            f64::NAN
        }
    }

    /// Does the token have the shape of a number, infinity included? For
    /// structure decisions (automatic data end, sub-header): an `inf` line is
    /// a data line, not the end of the file.
    pub fn is_numeric(&self, token: &str) -> bool {
        !self.parse_raw(token).is_nan()
    }

    fn parse_raw(&self, token: &str) -> f64 {
        let trimmed = token.trim();
        if trimmed.is_empty() || self.is_missing(trimmed) {
            return f64::NAN;
        }
        let mut normalized = String::with_capacity(trimmed.len());
        for c in trimmed.chars() {
            if Some(c) == self.thousands
                || (self.thousands == Some(' ') && matches!(c, '\u{a0}' | '\u{202f}'))
            {
                continue;
            }
            normalized.push(match (self.decimal, c) {
                (DecimalStyle::Comma, ',') => '.',
                (_, c) => c,
            });
        }
        normalized.parse::<f64>().unwrap_or(f64::NAN)
    }
}

/// Guesses the decimal separator from the data lines. A comma between two
/// digits is a decimal only if the line has another column separator (`;` or
/// tab): in a CSV "1,2,3" it is a separator. The decimal is the comma if
/// these clues outnumber the dots between two digits, otherwise the dot.
fn detect_decimal<'a>(lines: impl Iterator<Item = &'a str>) -> DecimalStyle {
    let (mut commas, mut dots) = (0usize, 0usize);
    for line in lines {
        let other_separator = line.contains(';') || line.contains('\t');
        let chars: Vec<char> = line.chars().collect();
        for window in chars.windows(3) {
            if window[0].is_ascii_digit() && window[2].is_ascii_digit() {
                match window[1] {
                    ',' if other_separator => commas += 1,
                    '.' => dots += 1,
                    _ => {}
                }
            }
        }
    }
    if commas > dots {
        DecimalStyle::Comma
    } else {
        DecimalStyle::Dot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(
        decimal: DecimalStyle,
        thousands: Option<char>,
        missing: &[&str],
        lines: &[&str],
    ) -> NumberFormat {
        let missing: Vec<String> = missing.iter().map(|s| s.to_string()).collect();
        NumberFormat::new(decimal, thousands, &missing, lines.iter().copied())
    }

    #[test]
    fn auto_detects_comma_with_another_separator() {
        let f = fmt(DecimalStyle::Auto, None, &[], &["1,5;2,5", "3,5;4,5"]);
        assert_eq!(f.decimal, DecimalStyle::Comma);
        assert_eq!(f.parse("1,5"), 1.5);
        let tab = fmt(DecimalStyle::Auto, None, &[], &["1,5\t2,5"]);
        assert_eq!(tab.decimal, DecimalStyle::Comma);
    }

    #[test]
    fn auto_keeps_dot_for_plain_csv_and_dot_data() {
        assert_eq!(
            fmt(DecimalStyle::Auto, None, &[], &["1,2,3", "4,5,6"]).decimal,
            DecimalStyle::Dot
        );
        assert_eq!(
            fmt(DecimalStyle::Auto, None, &[], &["1.5,2.5"]).decimal,
            DecimalStyle::Dot
        );
        assert_eq!(
            fmt(DecimalStyle::Auto, None, &[], &["1.5;2.5"]).decimal,
            DecimalStyle::Dot
        );
        assert_eq!(
            fmt(DecimalStyle::Auto, None, &[], &[]).decimal,
            DecimalStyle::Dot
        );
    }

    #[test]
    fn thousands_separator_is_dropped() {
        let f = fmt(DecimalStyle::Comma, Some(' '), &[], &[]);
        assert_eq!(f.parse("1 234,5"), 1234.5);
        assert_eq!(f.parse("1\u{a0}234,5"), 1234.5);
        assert_eq!(f.parse("1\u{202f}234,5"), 1234.5);
        let g = fmt(DecimalStyle::Dot, Some(','), &[], &[]);
        assert_eq!(g.parse("1,234.5"), 1234.5);
    }

    #[test]
    fn non_finite_results_are_nan() {
        let f = fmt(DecimalStyle::Dot, None, &[], &[]);
        for token in ["inf", "Infinity", "-inf", "1e400", "-1e400", "+INF"] {
            assert!(f.parse(token).is_nan(), "{token:?}");
            assert!(f.is_numeric(token), "{token:?}");
        }
        assert!(!f.is_numeric("abc"));
        assert_eq!(f.parse("1e300"), 1e300);
    }

    #[test]
    fn missing_tokens_are_missing_not_text() {
        let f = fmt(DecimalStyle::Dot, None, &["--", "N/A", "*"], &[]);
        for token in ["--", " n/a ", "*"] {
            assert!(f.is_missing(token), "{token:?}");
            assert!(f.parse(token).is_nan());
        }
        assert!(!f.is_missing("abc"));
        assert!(!f.is_missing(""));
    }
}
