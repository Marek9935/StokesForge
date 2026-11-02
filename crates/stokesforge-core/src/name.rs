//! Validation of object names (folders, workbooks, sheets, columns...).

use crate::error::{CoreError, CoreResult};

/// Characters reserved by the `[Book]Sheet!Col` path grammar.
pub const RESERVED_NAME_CHARS: &[char] = &['[', ']', '!', '/'];

/// Maximum length of a name, in Unicode characters (`chars().count()`, not
/// in bytes) — C7 (`guide-de-correction-robustesse.md` §F4·F9). A name of
/// a hundred thousand characters went through to disk unbounded; 255 is generous
/// for an object name and stays a named constant, not a guess.
pub const MAX_NAME_CHARS: usize = 255;

/// True for a character that `check_name` refuses inside a name:
/// any C0/C1 control character (`char::is_control`, which covers
/// `\0..=\x1F`, DEL `\x7F` and `\u{80}..=\u{9F}`), and the most common invisible
/// or deceptive Unicode formatting/direction marks —
/// zero-width spaces and joiners (`U+200B..=U+200F`) and the
/// isolate/override direction marks (`U+202A..=U+202E`, `U+2066..=U+2069`), among them
/// RIGHT-TO-LEFT OVERRIDE (`U+202E`): a name carrying them can visually
/// reverse the rest of a path or break any tabular display and
/// any error message that quotes it (C7).
fn is_control_or_deceptive_format(c: char) -> bool {
    c.is_control()
        || matches!(c,
            '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
        )
}

/// True if two names are the same name for a formula and for the sibling rule
/// (#403): equal ignoring case, the way a column letter or a function name is.
/// The one comparison shared by the lookup of a sheet or workbook in a formula
/// and by the check of sibling names, so that they cannot disagree.
pub fn same_name_ignoring_case(a: &str, b: &str) -> bool {
    a == b
        || a.chars()
            .flat_map(char::to_lowercase)
            .eq(b.chars().flat_map(char::to_lowercase))
}

/// Natural name order (#425): case-insensitive, runs of ASCII digits compared
/// by value (`Fit2` before `Fit10`, `a01` after `a1` only as a tie-break on
/// the run length). The digit runs are never parsed to a number, so a run of
/// any length cannot overflow: leading zeros are stripped, then the shorter
/// run is the smaller, then the digits compare as text.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let run = |it: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut s = String::new();
                    while let Some(c) = it.next_if(char::is_ascii_digit) {
                        s.push(c);
                    }
                    s
                };
                let (ra, rb) = (run(&mut ai), run(&mut bi));
                let (ta, tb) = (ra.trim_start_matches('0'), rb.trim_start_matches('0'));
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
                // Same value: the run with fewer leading zeros first.
                let ord = ra.len().cmp(&rb.len());
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                let (lx, ly) = (x.to_lowercase().next(), y.to_lowercase().next());
                let ord = lx.cmp(&ly);
                if ord != Ordering::Equal {
                    return ord;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

/// Validates an object name: not empty, no edge space, no
/// reserved character, no control character nor deceptive Unicode
/// formatting/direction mark, and at most [`MAX_NAME_CHARS`] characters. Returns the name
/// unchanged (borrowed) if valid.
pub fn check_name(name: &str) -> CoreResult<&str> {
    if name.is_empty() {
        return Err(CoreError::InvalidName("the name is empty".to_string()));
    }
    if name != name.trim() {
        return Err(CoreError::InvalidName(format!(
            "\"{name}\" starts or ends with a space"
        )));
    }
    if name.contains(RESERVED_NAME_CHARS) {
        return Err(CoreError::InvalidName(format!(
            "\"{name}\" contains a reserved character ([ ] ! /)"
        )));
    }
    if name.chars().any(is_control_or_deceptive_format) {
        return Err(CoreError::InvalidName(
            "control and Unicode formatting characters are not allowed in a name".to_string(),
        ));
    }
    let char_count = name.chars().count();
    if char_count > MAX_NAME_CHARS {
        return Err(CoreError::InvalidName(format!(
            "the name has {char_count} characters, {MAX_NAME_CHARS} at most"
        )));
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order_compares_digit_runs_by_value() {
        use std::cmp::Ordering::*;
        assert_eq!(natural_cmp("Fit2", "Fit10"), Less);
        assert_eq!(natural_cmp("fit10", "FIT2"), Greater);
        assert_eq!(natural_cmp("a1", "a01"), Less);
        assert_eq!(natural_cmp("a01", "a2"), Less);
        assert_eq!(natural_cmp("Book", "Book1"), Less);
        let big_a = format!("x{}1", "9".repeat(30));
        let big_b = format!("x{}2", "9".repeat(30));
        assert_eq!(natural_cmp(&big_a, &big_b), Less);
        let longer = format!("x{}", "9".repeat(31));
        assert_eq!(natural_cmp(&big_a, &longer), Less);
        assert_eq!(natural_cmp("B", "a"), Greater);
    }

    #[test]
    fn rejects_invalid_names() {
        for name in ["", "  ", " A", "A ", "A!B", "[A]", "a/b", "A]"] {
            assert!(
                matches!(check_name(name), Err(CoreError::InvalidName(_))),
                "expected InvalidName for {name:?}"
            );
        }
    }

    #[test]
    fn names_equal_ignoring_case_only() {
        assert!(same_name_ignoring_case("Sheet1", "SHEET1"));
        assert!(same_name_ignoring_case("Été", "été"));
        assert!(!same_name_ignoring_case("Sheet1", "Sheet 1"));
        assert!(!same_name_ignoring_case("Sheet1", "Sheet2"));
    }

    #[test]
    fn accepts_valid_names() {
        for name in ["A", "Book1", "Δ Temperature (K)", "x-y"] {
            assert_eq!(check_name(name), Ok(name));
        }
    }

    /// C7 (`guide-de-correction-robustesse.md` §F4·F9) — reproduces here the
    /// scenario of `b21_control_names_are_rejected`
    /// (`stokesforge-robustness`) as close as possible to the function it fixes.
    #[test]
    fn rejects_control_and_deceptive_format_characters() {
        for name in [
            "A\nB",       // line feed
            "A\tB",       // tab
            "A\u{0}B",    // null byte
            "A\u{202e}B", // RIGHT-TO-LEFT OVERRIDE
            "A\u{200b}B", // ZERO WIDTH SPACE
            "A\u{7f}B",   // DEL
            "A\u{200f}B", // RIGHT-TO-LEFT MARK
            "A\u{2066}B", // LEFT-TO-RIGHT ISOLATE
        ] {
            assert!(
                matches!(check_name(name), Err(CoreError::InvalidName(_))),
                "expected InvalidName for {name:?}"
            );
        }
    }

    #[test]
    fn rejects_names_longer_than_the_limit() {
        let too_long = "x".repeat(MAX_NAME_CHARS + 1);
        assert!(matches!(
            check_name(&too_long),
            Err(CoreError::InvalidName(_))
        ));

        let at_the_limit = "x".repeat(MAX_NAME_CHARS);
        assert_eq!(check_name(&at_the_limit), Ok(at_the_limit.as_str()));
    }
}
