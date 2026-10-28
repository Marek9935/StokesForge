//! The `en-XA` pseudo-locale, built **on the fly** from the `en` catalog
//! (issue #41): no file to keep up to date, it follows English key by key.
//!
//! It is used to *see* what the translation does not cover, without being able
//! to read another language:
//!
//! - letters are accented ("Save Project" -> "Šàṽé Þŕöĵéçţ"), so a text stays
//!   readable but recognizable;
//! - every message is wrapped in `[[ ]]`: **a text without `[[ ]]` on screen
//!   did not go through the catalog** (a literal left in the code), and a
//!   missing `]]` flags a text **truncated** by the layout;
//! - every message is lengthened by 30% (French is longer than English): a
//!   layout that does not fit in `en-XA` will not fit in French.
//!
//! The transformation goes through the Fluent syntax tree (`fluent-syntax`),
//! not a regular expression: only the **texts** are touched, never the
//! `{ $variable }`, the `{ -term }` references, the variant keys (`[one]`,
//! `*[other]`) or the identifiers. This guarantees that the pseudo-locale
//! accepts the same arguments as English.
//!
//! In a `select` (plural), each variant is lengthened separately; the `[[ ]]`
//! frame wraps only the whole message. Terms (`-name = ...`) are only
//! accented: they enter other messages, which are already wrapped.

use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement};
use fluent_syntax::{parser, serializer};

/// Opens a pseudo-locale message. ASCII, so that any font displays it (`⟦ ⟧`
/// came out as "missing glyph" squares with Noto Sans).
pub const OPEN: &str = "[[";
/// Closes a pseudo-locale message. Absent on screen: truncated text.
pub const CLOSE: &str = "]]";
/// Target lengthening, as a percentage of the original text.
pub const EXTRA_PERCENT: usize = 30;
/// Letter used for lengthening: accented, of ordinary width.
const FILLER: char = 'ŏ';

/// A Fluent string literal, `{ "…" }`.
fn literal(value: &str) -> PatternElement<String> {
    PatternElement::Placeable {
        expression: Expression::Inline(InlineExpression::StringLiteral {
            value: value.to_string(),
        }),
    }
}

/// Produces the pseudo-locale of a Fluent catalog. A source that does not
/// parse entirely keeps its valid entries; the unreadable pieces stay as they
/// are (invariant 10: degrade, do not panic — the test
/// `the_english_catalog_parses_without_error` holds English to this).
pub fn localize(source: &str) -> String {
    let mut resource = match parser::parse(source.to_string()) {
        Ok(resource) => resource,
        Err((resource, _errors)) => resource,
    };
    for entry in &mut resource.body {
        match entry {
            Entry::Message(message) => {
                if let Some(pattern) = &mut message.value {
                    localize_pattern(pattern, Role::Message);
                }
                for attribute in &mut message.attributes {
                    localize_pattern(&mut attribute.value, Role::Message);
                }
            }
            Entry::Term(term) => {
                localize_pattern(&mut term.value, Role::Term);
                for attribute in &mut term.attributes {
                    localize_pattern(&mut attribute.value, Role::Term);
                }
            }
            _ => {}
        }
    }
    serializer::serialize(&resource)
}

/// What is done with a pattern depending on where it sits.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Whole message or attribute: accented, lengthened, framed.
    Message,
    /// Variant of a `select`: accented and lengthened, without a frame.
    Variant,
    /// Term: accented only.
    Term,
}

fn localize_pattern(pattern: &mut Pattern<String>, role: Role) {
    // `letters` decides whether there is text to lengthen; `length` (spaces
    // included) sets the lengthening: 30% of the *displayed* length, not of
    // the letters alone, otherwise a message rich in spaces (an error
    // sentence) would stay under +30%.
    let mut letters = 0usize;
    let mut length = 0usize;
    for element in &mut pattern.elements {
        match element {
            PatternElement::TextElement { value } => {
                *value = value.chars().map(accent).collect();
                letters += value.chars().filter(|c| !c.is_whitespace()).count();
                length += value.chars().count();
            }
            PatternElement::Placeable { expression } => localize_expression(expression, role),
        }
    }
    let padding = || {
        let extra = (length * EXTRA_PERCENT).div_ceil(100);
        format!(" {}", FILLER.to_string().repeat(extra.max(1)))
    };
    match role {
        Role::Term => {}
        Role::Variant => {
            if letters > 0 {
                pattern
                    .elements
                    .push(PatternElement::TextElement { value: padding() });
            }
        }
        Role::Message => {
            // The frame is a `{ "[[" }` literal, not text: a `[` at the start
            // of a line of a multi-line pattern would read as a variant key.
            // Fluent does not isolate literals (FSI/PDI).
            if letters > 0 {
                pattern
                    .elements
                    .push(PatternElement::TextElement { value: padding() });
            }
            pattern.elements.insert(0, literal(OPEN));
            pattern.elements.push(literal(CLOSE));
        }
    }
}

/// Descend into the expressions that contain patterns (a `select`, a nested
/// placeable); a variable, a reference or a literal is never touched.
fn localize_expression(expression: &mut Expression<String>, role: Role) {
    match expression {
        Expression::Select { variants, .. } => {
            let inner = if role == Role::Term {
                Role::Term
            } else {
                Role::Variant
            };
            for variant in variants {
                localize_pattern(&mut variant.value, inner);
            }
        }
        Expression::Inline(InlineExpression::Placeable { expression }) => {
            localize_expression(expression, role);
        }
        Expression::Inline(_) => {}
    }
}

/// An ASCII letter becomes an accented letter; the rest is unchanged.
fn accent(c: char) -> char {
    match c {
        'a' => 'à',
        'b' => 'ƀ',
        'c' => 'ç',
        'd' => 'ð',
        'e' => 'é',
        'f' => 'ƒ',
        'g' => 'ĝ',
        'h' => 'ĥ',
        'i' => 'î',
        'j' => 'ĵ',
        'k' => 'ķ',
        'l' => 'ĺ',
        'm' => 'ɱ',
        'n' => 'ñ',
        'o' => 'ö',
        'p' => 'þ',
        'q' => 'ɋ',
        'r' => 'ŕ',
        's' => 'š',
        't' => 'ţ',
        'u' => 'ü',
        'v' => 'ṽ',
        'w' => 'ŵ',
        'x' => 'ẋ',
        'y' => 'ý',
        'z' => 'ž',
        'A' => 'À',
        'B' => 'Ɓ',
        'C' => 'Ç',
        'D' => 'Ð',
        'E' => 'É',
        'F' => 'Ƒ',
        'G' => 'Ĝ',
        'H' => 'Ĥ',
        'I' => 'Î',
        'J' => 'Ĵ',
        'K' => 'Ķ',
        'L' => 'Ĺ',
        'M' => 'Ṁ',
        'N' => 'Ñ',
        'O' => 'Ö',
        'P' => 'Þ',
        'Q' => 'Ɋ',
        'R' => 'Ŕ',
        'S' => 'Š',
        'T' => 'Ţ',
        'U' => 'Ü',
        'V' => 'Ṽ',
        'W' => 'Ŵ',
        'X' => 'Ẋ',
        'Y' => 'Ý',
        'Z' => 'Ž',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A catalog that gathers everything the transformation must not damage:
    /// variable, plural, term, attribute, comment, literal.
    const SAMPLE: &str = "\
# A comment
-brand = StokesForge
save = Save Project
open-file = Open { $name } now
count =
    { $count ->
        [one] { $count } item
       *[other] { $count } items
    }
about = About { -brand }
    .hint = Shows the version
literal = Say { \"hi\" }
";

    #[test]
    fn the_pseudo_locale_keeps_the_same_catalog_shape() {
        let pseudo = localize(SAMPLE);
        let (resource, errors) = match parser::parse(pseudo.as_str()) {
            Ok(resource) => (resource, vec![]),
            Err((resource, errors)) => (resource, errors),
        };
        assert!(
            errors.is_empty(),
            "the pseudo-locale parses: {errors:?}\n{pseudo}"
        );
        let (original, _) = match parser::parse(SAMPLE) {
            Ok(resource) => (resource, ()),
            Err((resource, _)) => (resource, ()),
        };
        let ids = |resource: &fluent_syntax::ast::Resource<&str>| -> Vec<String> {
            resource
                .body
                .iter()
                .filter_map(|entry| match entry {
                    Entry::Message(message) => Some(message.id.name.to_string()),
                    Entry::Term(term) => Some(format!("-{}", term.id.name)),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(ids(&resource), ids(&original), "same keys, same order");
    }

    #[test]
    fn a_message_is_accented_framed_and_lengthened() {
        let pseudo = localize("save = Save Project\n");
        let text = pseudo
            .strip_prefix("save = ")
            .and_then(|rest| rest.strip_suffix('\n'))
            .expect("one line");
        // The serialized source carries the frame as `{ "[[" }` literals.
        let (open, close) = (format!("{{ \"{OPEN}\" }}"), format!("{{ \"{CLOSE}\" }}"));
        assert!(text.starts_with(&open), "{text}");
        assert!(text.ends_with(&close), "{text}");
        assert!(text.contains("Šàṽé Þŕöĵéçţ"), "accented: {text}");
        let inner = text.trim_start_matches(&open).trim_end_matches(&close);
        assert!(
            inner.chars().count() >= "Save Project".chars().count() * 13 / 10,
            "lengthened by at least 30%: {inner}"
        );
    }

    #[test]
    fn variables_terms_and_keys_are_not_touched() {
        let pseudo = localize(SAMPLE);
        for fragment in [
            "{ $name }",
            "{ $count }",
            "[one]",
            "*[other]",
            "{ -brand }",
            "-brand = ŠţöķéšƑöŕĝé",
            ".hint = ",
            "{ \"hi\" }",
            "# A comment",
        ] {
            assert!(
                pseudo.contains(fragment),
                "{fragment:?} missing from:\n{pseudo}"
            );
        }
        assert!(
            !pseudo.contains("-brand = { \"[[\" }"),
            "a term is not framed"
        );
    }

    #[test]
    fn each_variant_of_a_plural_is_lengthened() {
        let pseudo = localize(SAMPLE);
        assert!(pseudo.contains("[one] { $count } îţéɱ ŏ"), "{pseudo}");
        assert!(pseudo.contains("*[other] { $count } îţéɱš ŏ"), "{pseudo}");
    }

    #[test]
    fn a_message_without_text_is_still_framed() {
        let pseudo = localize("only = { $n }\n");
        assert!(pseudo.contains("{ \"[[\" }{ $n }{ \"]]\" }"), "{pseudo}");
    }
}
