//! Lecture des nombres : séparateur décimal (fixé ou détecté), séparateur de
//! milliers (avec milliers espace : U+00A0 et U+202F aussi), jetons de valeur
//! manquante. Un résultat non fini (`inf`, `Infinity`, `1e400`) n'est pas un
//! nombre ici : `NaN`, donc jeton conservé en texte. Un [`NumberFormat`] est résolu **une
//! fois** par fichier, sur les mêmes lignes pour [`crate::parse::parse`] et
//! [`crate::preview::preview`] : c'est ce qui les garde d'accord.

use crate::recipe::{DecimalStyle, ImportRecipe};

/// Nombre de lignes examinées pour détecter la décimale.
const DETECTION_LINES: usize = 50;

/// Règles de lecture d'un nombre, déjà résolues pour un fichier.
#[derive(Debug, Clone)]
pub(crate) struct NumberFormat {
    /// Jamais `Auto` : résolu en `Dot` ou `Comma`.
    pub decimal: DecimalStyle,
    thousands: Option<char>,
    /// Jetons (en minuscules, bordures retirées) qui valent « valeur
    /// manquante » : lus comme vides, pas comme du texte à conserver.
    missing: Vec<String>,
}

impl NumberFormat {
    /// Résout le format d'une recette d'après `sample` (les premières lignes
    /// de données, `skip_chars` déjà retiré).
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

    /// Ce jeton (non vide) est-il un marqueur de valeur manquante ?
    pub fn is_missing(&self, token: &str) -> bool {
        let token = token.trim().to_lowercase();
        !token.is_empty() && self.missing.contains(&token)
    }

    /// Le séparateur de milliers de la recette.
    pub fn thousands(&self) -> Option<char> {
        self.thousands
    }

    /// Analyse un jeton en `f64` ; `NaN` pour tout ce qui ne s'y prête pas —
    /// jamais d'erreur (invariant 10). Un résultat non fini (`inf`,
    /// `Infinity`, `-inf`, `1e400`) vaut aussi `NaN` : le jeton reste alors
    /// visible en texte (colonne `Mixed`), au lieu d'entrer comme `±inf`
    /// dans une colonne numérique.
    pub fn parse(&self, token: &str) -> f64 {
        let value = self.parse_raw(token);
        if value.is_finite() {
            value
        } else {
            f64::NAN
        }
    }

    /// Le jeton a-t-il la forme d'un nombre, infini compris ? Pour les
    /// décisions de structure (fin automatique des données, sous-en-tête) :
    /// une ligne `inf` est une ligne de données, pas la fin du fichier.
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

/// Devine la décimale d'après les lignes de données. Une virgule entre deux
/// chiffres n'est une décimale que si la ligne a un autre séparateur de
/// colonnes (`;` ou tabulation) — dans un CSV « 1,2,3 » c'est un séparateur.
/// La décimale est la virgule si ces indices l'emportent sur les points entre
/// deux chiffres, sinon le point.
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
