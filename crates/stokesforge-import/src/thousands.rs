//! Thousands separator guess. Numbers written with a
//! grouping (`1,234.5`, `1.234,5`, `1'234.5`, `1 234,5`) are not numbers for
//! the plain detector, so a file of them looks empty. This module judges
//! whether a (separator, grouping char, decimal char) reading fits the
//! tokens of the first lines, on strict evidence:
//!
//! - groups of exactly 3 digits after the first group (1 to 3 digits, no
//!   leading zero), a grouping char different from the decimal char;
//! - seen at least twice, and on at least as many tokens as there are
//!   ungrouped tokens of 4 or more integer digits;
//! - never contradicted: a token made of digits and punctuation that the
//!   reading does not explain (`12,34` with a comma grouping, `3.5` with a
//!   dot grouping) kills it;
//! - a comma or dot grouping needs either a decimal part after the grouping
//!   or two groups in one token, since a lone `1,234` could be `1.234`.
//!
//! Pure and bounded by the scan window; a contradiction ends the scan.

/// Lines (from the first non-blank one) the evidence is looked for in.
pub(crate) const SCAN_LINES: usize = 3000;

/// What a reading found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThousandsGuess {
    /// The grouping char (`' '` stands for the space, the no-break space and
    /// the thin space, as in the recipe).
    pub ch: char,
    /// `true` when the decimal char of the file is the comma.
    pub decimal_comma: bool,
    /// Tokens that showed the grouping.
    pub tokens: usize,
    /// 1-based file line of the first of them.
    pub line: usize,
}

/// The (grouping, decimal) readings tried, in this order.
pub(crate) const READINGS: [(char, char); 6] = [
    (',', '.'),
    ('.', ','),
    ('\'', '.'),
    ('\'', ','),
    (' ', '.'),
    (' ', ','),
];

#[derive(Debug, PartialEq, Eq)]
enum Token {
    /// Text, empty, or a number the reading has nothing to say about.
    Neutral,
    /// Plain digits, four or more.
    Plain4,
    Grouped {
        groups: usize,
        decimal: bool,
    },
    /// Digits and punctuation the reading does not explain.
    Bad,
}

fn is_group(c: char, ch: char) -> bool {
    c == ch || (ch == ' ' && matches!(c, '\u{a0}' | '\u{202f}'))
}

/// Drops a trailing exponent (`E-3`); `None` when an `e` is not an exponent.
fn without_exponent(token: &str) -> Option<&str> {
    let Some(pos) = token.find(['e', 'E']) else {
        return Some(token);
    };
    let exp = token[pos + 1..].trim_start_matches(['+', '-']);
    (pos > 0 && !exp.is_empty() && exp.bytes().all(|b| b.is_ascii_digit())).then(|| &token[..pos])
}

fn classify(token: &str, ch: char, dec: char) -> Token {
    let token = token.trim();
    let Some(body) = without_exponent(token) else {
        return Token::Neutral;
    };
    let body = body.strip_prefix(['+', '-']).unwrap_or(body);
    let punct = |c: char| matches!(c, '.' | ',' | '\'') || c == ' ' || is_group(c, ' ');
    if !body.bytes().any(|b| b.is_ascii_digit())
        || !body.chars().all(|c| c.is_ascii_digit() || punct(c))
    {
        return Token::Neutral;
    }
    // Digits and punctuation only: the reading must explain all of it.
    let mut parts = body.splitn(2, dec);
    let int_part = parts.next().unwrap_or("");
    let frac = parts.next();
    if let Some(f) = frac {
        if f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()) {
            return Token::Bad;
        }
    }
    if !int_part.chars().any(|c| is_group(c, ch)) {
        if !int_part.bytes().all(|b| b.is_ascii_digit()) {
            return Token::Bad;
        }
        return if int_part.len() >= 4 {
            Token::Plain4
        } else {
            Token::Neutral
        };
    }
    let groups: Vec<&str> = int_part.split(|c| is_group(c, ch)).collect();
    let head = groups[0];
    let head_ok = (1..=3).contains(&head.len())
        && head.bytes().all(|b| b.is_ascii_digit())
        && !head.starts_with('0');
    let tail_ok = groups[1..]
        .iter()
        .all(|g| g.len() == 3 && g.bytes().all(|b| b.is_ascii_digit()));
    if head_ok && tail_ok {
        Token::Grouped {
            groups: groups.len() - 1,
            decimal: frac.is_some(),
        }
    } else {
        Token::Bad
    }
}

/// Does some line of the scan window hold a number grouped by `ch`
/// (`digit`, `ch`, three digits, then no digit)? A cheap filter that spares
/// the full reading on files that cannot use it.
pub(crate) fn present(raw: &[&str], first: usize, ch: char) -> bool {
    let end = raw.len().min(first.saturating_add(SCAN_LINES));
    raw[first.min(end)..end].iter().any(|line| {
        // A grouped number holds a grouping char.
        let has_group = if ch == ' ' {
            line.contains([' ', '\u{a0}', '\u{202f}'])
        } else {
            line.contains(ch)
        };
        if !has_group {
            return false;
        }
        let (mut before, mut in_group, mut after) = (false, false, 0usize);
        for c in line.chars() {
            if c.is_ascii_digit() {
                if in_group {
                    after += 1;
                } else {
                    before = true;
                }
            } else if is_group(c, ch) && before && !in_group {
                in_group = true;
                after = 0;
            } else {
                if in_group && after == 3 {
                    return true;
                }
                before = false;
                in_group = false;
                after = 0;
            }
        }
        in_group && after == 3
    })
}

/// Judges the reading (`ch` grouping, `dec` decimal) on the tokens that
/// `fields` gives for the lines of `raw` from `first`. `None` unless the
/// evidence is strict (see the module documentation).
pub(crate) fn evidence<'a>(
    raw: &[&'a str],
    first: usize,
    fields: &dyn Fn(&'a str) -> Vec<&'a str>,
    ch: char,
    dec: char,
) -> Option<ThousandsGuess> {
    let end = raw.len().min(first.saturating_add(SCAN_LINES));
    let mut grouped = 0usize;
    let mut plain4 = 0usize;
    let mut sure = false;
    let mut first_line = None;
    for (i, line) in raw.iter().enumerate().take(end).skip(first) {
        for token in fields(line) {
            match classify(token, ch, dec) {
                Token::Bad => return None,
                Token::Plain4 => plain4 += 1,
                Token::Grouped { groups, decimal } => {
                    grouped += 1;
                    first_line.get_or_insert(i + 1);
                    sure |= decimal || groups >= 2 || !matches!(ch, ',' | '.');
                }
                Token::Neutral => {}
            }
        }
    }
    (grouped >= 2 && grouped >= plain4 && sure).then(|| ThousandsGuess {
        ch,
        decimal_comma: dec == ',',
        tokens: grouped,
        line: first_line.unwrap_or(first + 1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sniff::sniff;

    fn guess_of(text: &str) -> Option<ThousandsGuess> {
        sniff(text.as_bytes(), "t.txt")
            .ok()
            .and_then(|s| s.thousands)
    }

    #[test]
    fn each_grouping_char_is_found() {
        let comma = guess_of("t\tv\n1\t1,234.5\n2\t12,345.25\n3\t987.5\n").unwrap();
        assert_eq!((comma.ch, comma.decimal_comma), (',', false));
        let dot = guess_of("t\tv\n1\t1.234,5\n2\t12.345,25\n3\t987,5\n").unwrap();
        assert_eq!((dot.ch, dot.decimal_comma), ('.', true));
        let apostrophe = guess_of("t;v\n1;1'234.5\n2;12'345.25\n3;987.5\n").unwrap();
        assert_eq!((apostrophe.ch, apostrophe.decimal_comma), ('\'', false));
        let space = guess_of("t;v\n1;1 234,5\n2;12 345,25\n3;987,5\n").unwrap();
        assert_eq!((space.ch, space.decimal_comma), (' ', true));
        let nbsp = guess_of("t;v\n1;1\u{a0}234,5\n2;12\u{202f}345,25\n3;987,5\n").unwrap();
        assert_eq!(nbsp.ch, ' ');
    }

    #[test]
    fn the_recipe_reads_the_grouped_numbers_as_numbers() {
        let text = "t\tv\n1\t1,234.5\n2\t12,345.25\n3\t987.5\n";
        let s = sniff(text.as_bytes(), "t.txt").unwrap();
        assert_eq!(s.recipe.thousands, Some(','));
        assert_eq!(s.rows, 3);
        let table = crate::parse::parse(text.as_bytes(), &s.recipe).unwrap();
        assert_eq!(table.columns[1].values, vec![1234.5, 12345.25, 987.5]);
    }

    #[test]
    fn two_groups_make_a_lone_comma_certain() {
        let g = guess_of("t\tv\n1\t12,345,678\n2\t1,234,567\n3\t5\n").unwrap();
        assert_eq!(g.ch, ',');
    }

    #[test]
    fn a_single_ambiguous_comma_is_not_a_grouping() {
        // `1,234` could be 1.234: no decimal part, no second group.
        assert_eq!(guess_of("t\tv\n1\t1,234\n2\t5,678\n3\t9,012\n"), None);
        assert_eq!(guess_of("t\tv\n1\t1.234\n2\t5.678\n3\t9.012\n"), None);
    }

    #[test]
    fn an_ambiguous_comma_stays_ambiguous_when_the_plain_reading_fails_elsewhere() {
        // `1,234` twice and dot decimals elsewhere: no decimal after a
        // grouping, no second group.
        assert_eq!(
            guess_of("t\tv\n1\t1,234\n2\t0.5\n3\t5,678\n4\t0.25\n"),
            None
        );
    }

    #[test]
    fn space_and_apostrophe_groupings_need_no_decimal_part() {
        assert_eq!(guess_of("t;v\n1;1 234\n2;12 345\n3;5\n").unwrap().ch, ' ');
        assert_eq!(guess_of("t;v\n1;1'234\n2;12'345\n3;5\n").unwrap().ch, '\'');
    }

    #[test]
    fn whitespace_separated_files_can_be_grouped_too() {
        assert_eq!(
            guess_of("t v\n1 1,234.5\n2 12,345.5\n3 5.5\n").unwrap().ch,
            ','
        );
    }

    #[test]
    fn one_grouped_number_is_not_enough() {
        assert_eq!(guess_of("t;v\n1;1,234.5\n2;5\n3;6\n"), None);
    }

    #[test]
    fn a_plain_decimal_comma_kills_the_comma_grouping() {
        assert_eq!(guess_of("t\tv\n1\t1,234.5\n2\t12,345.25\n3\t12,34\n"), None);
    }

    #[test]
    fn a_csv_with_commas_between_columns_is_no_grouping() {
        assert_eq!(guess_of("a,b,c\n1,234,5\n2,345,6\n3,456,7\n"), None);
        // Three-digit columns of a comma CSV stay columns.
        let s = sniff(b"a,b,c\n1,234,567\n2,345,678\n3,456,789\n", "t.csv").unwrap();
        assert_eq!(s.recipe.thousands, None);
        assert_eq!(s.columns, 3);
    }

    #[test]
    fn plain_numbers_and_tuned_style_files_have_no_grouping() {
        assert_eq!(guess_of("1;2\n3;4\n5;6\n"), None);
        assert_eq!(guess_of("t;v\n1;1234.5\n2;2345.5\n3;3456.5\n"), None);
        // Mostly ungrouped four-digit numbers outweigh two grouped ones.
        assert_eq!(
            guess_of("t;v\n1;1234.5\n2;2345.5\n3;3456.5\n4;4,567.5\n5;5,678.5\n"),
            None
        );
    }

    #[test]
    fn a_date_with_dots_is_no_grouping() {
        assert_eq!(
            guess_of("d;v\n12.03.2024;1\n13.03.2024;2\n14.03.2024;3\n"),
            None
        );
    }

    #[test]
    fn a_leading_zero_group_is_a_decimal() {
        assert_eq!(guess_of("t\tv\n1\t0,123.5\n2\t0,456.5\n3\t0,789.5\n"), None);
    }

    #[test]
    fn grouped_numbers_in_a_semicolon_file_keep_the_semicolon() {
        let s = sniff(b"t;v;w\n1;1,234.5;2\n2;12,345.5;3\n3;99.5;4\n", "t.csv").unwrap();
        assert_eq!(s.recipe.thousands, Some(','));
        assert_eq!(s.recipe.separator, crate::recipe::Separator::Semicolon);
        assert_eq!(s.columns, 3);
    }

    #[test]
    fn classify_covers_the_grammar() {
        assert_eq!(
            classify("1,234.5", ',', '.'),
            Token::Grouped {
                groups: 1,
                decimal: true
            }
        );
        assert_eq!(classify("1234", ',', '.'), Token::Plain4);
        assert_eq!(classify("12", ',', '.'), Token::Neutral);
        assert_eq!(classify("abc", ',', '.'), Token::Neutral);
        assert_eq!(classify("1,23", ',', '.'), Token::Bad);
        assert_eq!(classify("1,2345", ',', '.'), Token::Bad);
        assert_eq!(classify("1e3", ',', '.'), Token::Neutral);
        assert_eq!(
            classify("-1,234E3", ',', '.'),
            Token::Grouped {
                groups: 1,
                decimal: false
            }
        );
    }
}
