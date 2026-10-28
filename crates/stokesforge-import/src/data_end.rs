//! End of the data zone: the cutting rule, written once so that
//! [`crate::parse::parse`] and [`crate::preview::preview`] cut at the same
//! place (a preview that showed a different cut from the import would be
//! worse than no preview).

use crate::numbers::NumberFormat;
use crate::parse::EffectiveSeparator;
use crate::recipe::DataEnd;

/// Number of data lines to **keep** among `lines` (the lines that follow the
/// header and the column header line, with `skip_chars` already removed).
/// `lines` is walked lazily: the automatic cut stops at the first faulty
/// line.
pub(crate) fn data_len<'a>(
    lines: impl Iterator<Item = &'a str>,
    total: usize,
    end: &DataEnd,
    separator: &EffectiveSeparator,
    numbers: &NumberFormat,
    first_column_kind: crate::recipe::ColumnKind,
    date_order: crate::recipe::DateOrder,
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
            // Only the first column counts: the first non-empty token with a
            // "whitespace" separator, the first field otherwise. Empty line:
            // no number, so a cut (once armed).
            // "Number" according to the kind of the first column: a number, a
            // readable date, or, for a column forced to text, any non-empty
            // token.
            let first_is_number = |line: &str| {
                separator
                    .tokenize(line)
                    .first()
                    .is_some_and(|token| match first_column_kind {
                        crate::recipe::ColumnKind::Auto => numbers.is_numeric(token),
                        crate::recipe::ColumnKind::DateTime => {
                            crate::datetime::parse_datetime_with(token, date_order).is_some()
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

    /// The text before the first numeric line is imported as is; the cut only
    /// arms at the first numeric line.
    #[test]
    fn auto_armed_only_at_first_numeric_line() {
        let text = "text;a\n;\n1;10\n2;20\n\n9;9\n";
        // Lines 1 and 2 (text, empty cell) kept, then 1 and 2, cut at the empty line.
        assert_eq!(rows(text, DataEnd::Auto, Separator::Semicolon), 4);
    }

    /// Only the first column counts: `2;n/a` continues; a key at the start of
    /// a line cuts even if a number follows.
    #[test]
    fn auto_looks_at_first_column_only() {
        let text = "1;10\n2;n/a\n3;30\nData points;551\n4;40\n";
        assert_eq!(rows(text, DataEnd::Auto, Separator::Semicolon), 3);
        assert_eq!(rows(text, DataEnd::None, Separator::Semicolon), 5);
    }

    /// Whitespace separator: the "first column" is the first non-empty token,
    /// despite leading spaces on the line.
    #[test]
    fn auto_with_whitespace_ignores_leading_spaces() {
        let text = "   1   10\n  2  20\n   end   x\n";
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

    /// The preview cuts exactly where the import cuts, and the footer is marked.
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
