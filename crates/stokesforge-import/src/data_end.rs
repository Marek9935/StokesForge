//! Fin de la zone de données : la règle de coupure, écrite une seule fois pour
//! que [`crate::parse::parse`] et [`crate::preview::preview`] coupent au même
//! endroit (un aperçu qui montrerait une coupure différente de l'import
//! serait pire que pas d'aperçu).

use crate::numbers::NumberFormat;
use crate::parse::EffectiveSeparator;
use crate::recipe::DataEnd;

/// Nombre de lignes de données à **garder** parmi `lines` (les lignes qui
/// suivent l'en-tête et la ligne d'en-tête de colonnes, `skip_chars` déjà
/// retiré). `lines` est parcouru paresseusement : la coupure automatique
/// s'arrête à la première ligne fautive.
pub(crate) fn data_len<'a>(
    lines: impl Iterator<Item = &'a str>,
    total: usize,
    end: &DataEnd,
    separator: &EffectiveSeparator,
    numbers: &NumberFormat,
    first_column_kind: crate::recipe::ColumnKind,
) -> usize {
    match end {
        DataEnd::None => total,
        DataEnd::LinesAfterHeader(n) => (*n).min(total),
        DataEnd::LinesBeforeEnd(n) => total.saturating_sub(*n),
        DataEnd::FirstBlankLine => {
            let mut lines = lines;
            lines
                .position(|line| line.trim().is_empty())
                .unwrap_or(total)
        }
        DataEnd::Marker(marker) => {
            let marker = marker.trim().to_lowercase();
            let mut lines = lines;
            lines
                .position(|line| line.trim().to_lowercase() == marker)
                .unwrap_or(total)
        }
        DataEnd::Auto => {
            // Seule la première colonne compte : le premier jeton non vide en
            // séparateur « espaces », le premier champ sinon. Ligne vide :
            // aucun nombre, donc coupure (une fois armée).
            // « Nombre » selon la nature de la première colonne : un nombre, une
            // date lisible, ou — pour une colonne forcée en texte — n'importe
            // quel jeton non vide.
            let first_is_number = |line: &str| {
                separator
                    .tokenize(line)
                    .first()
                    .is_some_and(|token| match first_column_kind {
                        crate::recipe::ColumnKind::Auto => numbers.is_numeric(token),
                        crate::recipe::ColumnKind::DateTime => {
                            crate::datetime::parse_datetime(token).is_some()
                        }
                        crate::recipe::ColumnKind::Text => !token.trim().is_empty(),
                    })
            };
            let mut armed = false;
            for (index, line) in lines.enumerate() {
                if first_is_number(line) {
                    armed = true;
                } else if armed {
                    return index;
                }
            }
            total
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::recipe::{DataEnd, ImportRecipe, Separator};
    use crate::{parse, preview, LineRole};

    fn rows(text: &str, end: DataEnd, separator: Separator) -> usize {
        let mut recipe = ImportRecipe::new("t");
        recipe.separator = separator;
        recipe.data_end = end;
        parse(text.as_bytes(), &recipe).map_or(0, |table| table.row_count())
    }

    const FILE: &str = "note;x\n\n1;10\n2;n/a\n3;30\n\nData points;551\n4;40\n";

    /// Le texte qui précède la première ligne numérique est importé tel quel ;
    /// la coupure ne s'arme qu'à la première ligne numérique.
    #[test]
    fn auto_armed_only_at_first_numeric_line() {
        let text = "texte;a\n;\n1;10\n2;20\n\n9;9\n";
        // Lignes 1 et 2 (texte, cellule vide) gardées, puis 1 et 2, coupure à la ligne vide.
        assert_eq!(rows(text, DataEnd::Auto, Separator::Semicolon), 4);
    }

    /// Seule la première colonne compte : `2;n/a` continue ; une clé en tête
    /// de ligne coupe même si un nombre suit.
    #[test]
    fn auto_looks_at_first_column_only() {
        let text = "1;10\n2;n/a\n3;30\nData points;551\n4;40\n";
        assert_eq!(rows(text, DataEnd::Auto, Separator::Semicolon), 3);
        assert_eq!(rows(text, DataEnd::None, Separator::Semicolon), 5);
    }

    /// Séparateur espaces : la « première colonne » est le premier jeton non
    /// vide, malgré des espaces en tête de ligne.
    #[test]
    fn auto_with_whitespace_ignores_leading_spaces() {
        let text = "   1   10\n  2  20\n   fin   x\n";
        assert_eq!(rows(text, DataEnd::Auto, Separator::Whitespace), 2);
    }

    #[test]
    fn explicit_ends() {
        let text = "1;1\n2;2\n3;3\n\n4;4\nSTOP\n5;5\n";
        assert_eq!(rows(text, DataEnd::FirstBlankLine, Separator::Semicolon), 3);
        assert_eq!(
            rows(text, DataEnd::Marker(" stop ".into()), Separator::Semicolon),
            5
        );
        assert_eq!(
            rows(text, DataEnd::LinesAfterHeader(2), Separator::Semicolon),
            2
        );
        assert_eq!(
            rows(text, DataEnd::LinesBeforeEnd(3), Separator::Semicolon),
            4
        );
        assert_eq!(
            rows(text, DataEnd::LinesAfterHeader(0), Separator::Semicolon),
            0
        );
    }

    /// L'aperçu coupe exactement là où l'import coupe, et le pied est marqué.
    #[test]
    fn preview_agrees_with_parse() {
        let mut recipe = ImportRecipe::new("t");
        recipe.separator = Separator::Semicolon;
        let shown = preview(FILE.as_bytes(), &recipe).unwrap();
        let parsed = parse(FILE.as_bytes(), &recipe).unwrap();
        assert_eq!(shown.total_data_lines, parsed.row_count());
        let footer: Vec<&str> = shown
            .lines
            .iter()
            .filter(|l| l.role == LineRole::Footer)
            .map(|l| l.text.as_str())
            .collect();
        assert_eq!(footer.len(), parsed.footer_lines().len());
        assert_eq!(footer, parsed.footer_lines());
    }
}
