//! Column variables in any graph text (#501): `#ln`, `#u`… resolve like in the
//! legend Custom template of #499.
//!
//! [`bind`] adds, in front of a text, one `#let` per variable the text uses as
//! **code** (`#ln`, `#if u != "" […]`, `#units`). A math identifier (`$ln(x)$`)
//! is never a variable: the logarithm stays the logarithm. A text that uses no
//! variable comes back byte-identical, so it keeps its cache entry and its
//! pixels.
//!
//! The preamble is on a single line, framed by two comments that draw nothing,
//! so it adds no line and no space to a title. [`strip_preamble`] drops it
//! again: the `epaint` fallback of invariant 10 shows and measures what the
//! user typed, never the `#let`s.

use stokesforge_plot::{typst_string, LegendField};
use typst::syntax::{parse, SyntaxKind, SyntaxNode};

/// The variables: long name, short name, field. The same table as the legend
/// Custom template (`stokesforge_plot::VARIABLES`), the one place that defines
/// them.
pub const VARIABLES: [(&str, &str, LegendField); 5] = stokesforge_plot::VARIABLES;

/// What each variable stands for: the text of the column's fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VariableValues {
    pub long_name: String,
    pub short_name: String,
    pub units: String,
    pub comments: String,
    pub x_prime: String,
}

impl VariableValues {
    /// The text of one field.
    pub fn get(&self, field: LegendField) -> &str {
        match field {
            LegendField::LongName => &self.long_name,
            LegendField::ShortName => &self.short_name,
            LegendField::Units => &self.units,
            LegendField::Comments => &self.comments,
            LegendField::XPrime => &self.x_prime,
        }
    }
}

/// Opens the preamble. A comment: it draws nothing.
const START: &str = "/*sf-vars*/";
/// Closes the preamble.
const END: &str = "/*sf-end*/";

fn collect_identifiers(node: &SyntaxNode, used: &mut Vec<String>) {
    if node.kind() == SyntaxKind::Ident {
        if let Some(ident) = node.cast::<typst::syntax::ast::Ident>() {
            used.push(ident.as_str().to_owned());
        }
    }
    for child in node.children() {
        collect_identifiers(child, used);
    }
}

/// `text` with a preamble that defines the variables it uses as code. A text
/// that uses none is returned unchanged. A `#let` of the user in the text
/// comes after the preamble and wins.
pub fn bind(text: &str, values: &VariableValues) -> String {
    // A name can only be used if the text mentions it: skip the parse otherwise.
    if !VARIABLES
        .iter()
        .any(|(long, short, _)| text.contains(long) || text.contains(short))
    {
        return text.to_owned();
    }
    let mut used = Vec::new();
    collect_identifiers(&parse(text), &mut used);
    let mut preamble = String::new();
    for (long, short, field) in VARIABLES {
        let quoted = typst_string(values.get(field));
        for name in [long, short] {
            if used.iter().any(|ident| ident == name) {
                preamble.push_str(&format!("#let {name} = {quoted};"));
            }
        }
    }
    if preamble.is_empty() {
        return text.to_owned();
    }
    format!("{START}{preamble}{END}{text}")
}

/// `text` without the preamble [`bind`] put in front of it: what the user
/// typed. Identity on any other text.
pub fn strip_preamble(text: &str) -> &str {
    let Some(mut rest) = text.strip_prefix(START) else {
        return text;
    };
    loop {
        if let Some(after) = rest.strip_prefix(END) {
            return after;
        }
        // `#let name = "…";` — the string may hold anything, `;` and the end
        // marker included: walk it with its escapes.
        let Some(body) = rest.strip_prefix("#let ") else {
            return text;
        };
        let Some(open) = body.find('"') else {
            return text;
        };
        let mut chars = body[open + 1..].char_indices();
        let mut close = None;
        while let Some((index, character)) = chars.next() {
            match character {
                '\\' => {
                    chars.next();
                }
                '"' => {
                    close = Some(open + 1 + index + 1);
                    break;
                }
                _ => {}
            }
        }
        let Some(close) = close else {
            return text;
        };
        match body[close..].strip_prefix(';') {
            Some(next) => rest = next,
            None => return text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::FontChoice;
    use crate::formula::{compile, fallback_text, plain_text, resolve_fonts};

    fn values(long: &str, units: &str) -> VariableValues {
        VariableValues {
            long_name: long.to_owned(),
            short_name: "s".to_owned(),
            units: units.to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn a_value_is_drawn_literally() {
        let long = "A *b* \" \\ #";
        let bound = bind("#ln", &values(long, ""));
        assert_eq!(plain_text(&bound).as_deref(), Some(long));
    }

    #[test]
    fn a_math_identifier_is_not_a_variable() {
        let text = "$ln(x)$";
        assert_eq!(bind(text, &values("Abs", "mM")), text);
    }

    #[test]
    fn a_text_without_variable_is_unchanged() {
        for text in ["Wavenumber", "", "#strong[Abs] (cm)", "a # b"] {
            assert_eq!(bind(text, &values("Abs", "mM")), text);
        }
    }

    #[test]
    fn an_axis_title_composes_without_extra_space() {
        let bound = bind("#ln#if u != \"\" [ (#u)]", &values("Abs", "mM"));
        assert!(!bound.contains('\n'), "{bound:?}");
        assert_eq!(plain_text(&bound).as_deref(), Some("Abs (mM)"));
    }

    #[test]
    fn the_users_own_let_wins() {
        let bound = bind("#let ln = \"mine\";#ln", &values("Abs", ""));
        assert_eq!(plain_text(&bound).as_deref(), Some("mine"));
    }

    #[test]
    fn a_failing_text_falls_back_to_what_was_typed() {
        let bound = bind("#ln #nope", &values("Abs", ""));
        assert!(bound.starts_with(START));
        assert_eq!(plain_text(&bound), None);
        assert_eq!(fallback_text(&bound), "#ln #nope");
        assert_eq!(fallback_text("plain"), "plain");
    }

    #[test]
    fn a_hostile_value_binds_and_compiles() {
        let hostile = format!("{}\"\\\n*/]\u{202e}{END};#let x = \"", "é".repeat(5_000));
        assert!(hostile.len() > 10_000);
        let bound = bind("#ln #u", &values(&hostile, &hostile));
        let (sans, math) = resolve_fonts(&FontChoice::default()).unwrap();
        compile(&bound, 12.0, [0, 0, 0, 255], sans, math).expect("compiles");
        assert_eq!(strip_preamble(&bound), "#ln #u");
    }
}
