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
    fn rejects_invalid_names() {
        for name in ["", "  ", " A", "A ", "A!B", "[A]", "a/b", "A]"] {
            assert!(
                matches!(check_name(name), Err(CoreError::InvalidName(_))),
                "expected InvalidName for {name:?}"
            );
        }
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
