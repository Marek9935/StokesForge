//! The reading of a formula typed in the F(x)= field (#433): what the field
//! keeps, and the one rewrite it makes.
//!
//! The field **always writes** (Enter, Tab, focus loss): a text that does not
//! read is kept, framed, and the column stays empty until it is corrected
//! ([`stokesforge_core::Tree::formula_keeping`]). Only the field does this:
//! `pyst` reads strictly
//! ([`stokesforge_core::Tree::formula_for_entry`]) and refuses; a paste goes
//! through the same reading as typing (`AppState::write_formula_text`).
//!
//! The rewrite (Q6): `max(B,1,5)` written by someone who means 1.5 is the range
//! 1 to 5. The range is kept, the call is rewritten with a space after each
//! comma (`max(B, 1, 5)`), and the field says it in orange. Only a column
//! function called with three arguments whose last two are plain numbers
//! written without a space around their comma is concerned: `at`, `round`,
//! `atan2(1,5)` and `if(A>0,1,5)` are left as typed.

use stokesforge_core::{ColumnFormula, ColumnFunc, FormulaError, NodeId, Tree};

use crate::tr;

/// What the field did with a typed text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Outcome {
    /// Read as typed, and it computes.
    Clean,
    /// A range written with a comma between two bare numbers: kept as a range,
    /// rewritten with spaces. `sentence` says so (status bar, history, bubble).
    Warned { rewritten: String, sentence: String },
    /// Kept although it does not compute: `error` says why.
    Kept { error: FormulaError },
}

/// The formula to store for the text typed in the field, and what happened.
pub(crate) fn read(tree: &Tree, column: NodeId, text: &str) -> (ColumnFormula, Outcome) {
    let (formula, error) = tree.formula_keeping(column, text);
    if let Some(error) = error {
        return (formula, Outcome::Kept { error });
    }
    let Some(comma) = decimal_comma_range(text) else {
        return (formula, Outcome::Clean);
    };
    // Re-read the rewritten text: it replaces the typed one only if it reads
    // as cleanly as the typed one did.
    let (rewritten_formula, rewritten_error) = tree.formula_keeping(column, &comma.rewritten);
    if rewritten_error.is_some() {
        return (formula, Outcome::Clean);
    }
    let decimal = format!("{}.{}", comma.from, comma.to);
    let sentence = tr!(
        "status-formula-comma-rewritten",
        from = comma.from.as_str(),
        to = comma.to.as_str(),
        rewritten = comma.rewritten.as_str(),
        decimal = decimal.as_str()
    );
    (
        rewritten_formula,
        Outcome::Warned {
            rewritten: comma.rewritten,
            sentence,
        },
    )
}

/// A range call whose bounds look like a decimal number.
#[derive(Debug, PartialEq)]
struct CommaRange {
    /// The text with a space after each comma of the call.
    rewritten: String,
    /// The two bare numbers, as typed.
    from: String,
    to: String,
}

/// The first column function of `text` called with three arguments, whose last
/// two are plain numbers with no space around the comma that separates them.
fn decimal_comma_range(text: &str) -> Option<CommaRange> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i].is_ascii_alphanumeric() {
            i += 1;
        }
        let name: String = chars[start..i].iter().collect();
        // A column function is called by its name, directly followed by `(`.
        let preceded =
            start > 0 && (chars[start - 1].is_ascii_alphanumeric() || chars[start - 1] == '!');
        if preceded || chars.get(i) != Some(&'(') || !is_range_function(&name) {
            continue;
        }
        if let Some(found) = three_arguments(&chars, i) {
            return Some(found);
        }
    }
    None
}

fn is_range_function(name: &str) -> bool {
    ColumnFunc::ALL
        .iter()
        .any(|function| *function != ColumnFunc::At && function.name().eq_ignore_ascii_case(name))
}

/// The call whose `(` is at `open`: its top-level commas, and the test.
fn three_arguments(chars: &[char], open: usize) -> Option<CommaRange> {
    let mut depth = 0usize;
    let mut commas = Vec::new();
    let mut close = None;
    for (offset, c) in chars[open..].iter().enumerate() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + offset);
                    break;
                }
            }
            ',' if depth == 1 => commas.push(open + offset),
            _ => {}
        }
    }
    let close = close?;
    let [first, second] = commas[..] else {
        return None;
    };
    // The bounds touch their comma: `1,5`, not `1, 5` nor `1 ,5`.
    let from = bare_number_before(chars, first + 1, second)?;
    let to = bare_number_after(chars, second + 1, close)?;
    let mut rewritten = String::new();
    for (index, c) in chars.iter().enumerate() {
        rewritten.push(*c);
        let is_call_comma = index == first || index == second;
        if is_call_comma
            && !chars
                .get(index + 1)
                .is_some_and(|next| next.is_whitespace())
        {
            rewritten.push(' ');
        }
    }
    Some(CommaRange {
        rewritten,
        from,
        to,
    })
}

/// The bare number between the opening comma and the comma at `end`: a space
/// before it is the usual `, 1`, none may touch the comma at `end` (`1 ,5`).
fn bare_number_before(chars: &[char], from: usize, end: usize) -> Option<String> {
    let literal: String = chars[from..end].iter().collect();
    bare_number(literal.trim_start())
}

/// The bare number between the comma at `from - 1` and the `)` at `end`: no
/// space may touch that comma (`1, 5` is a range already spaced), a space
/// before the `)` is harmless.
fn bare_number_after(chars: &[char], from: usize, end: usize) -> Option<String> {
    let literal: String = chars[from..end].iter().collect();
    bare_number(literal.trim_end())
}

/// Digits with at most one point: no sign, no exponent, no name.
fn bare_number(text: &str) -> Option<String> {
    let digits = text.chars().filter(char::is_ascii_digit).count();
    let plain = !text.is_empty()
        && digits > 0
        && text.chars().all(|c| c.is_ascii_digit() || c == '.')
        && text.matches('.').count() <= 1;
    plain.then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewrite(text: &str) -> Option<String> {
        decimal_comma_range(text).map(|found| found.rewritten)
    }

    #[test]
    fn a_range_with_a_bare_comma_is_rewritten_with_spaces() {
        assert_eq!(rewrite("max(B,1,5)").as_deref(), Some("max(B, 1, 5)"));
        assert_eq!(rewrite("max(B, 1,5)").as_deref(), Some("max(B, 1, 5)"));
        assert_eq!(
            rewrite("max(B,330,350)").as_deref(),
            Some("max(B, 330, 350)")
        );
        assert_eq!(rewrite("2*mean(B,0.5,1,5)"), None, "four arguments");
    }

    #[test]
    fn only_a_column_function_with_two_bare_numbers_is_concerned() {
        // The space test: a comma that already has its space is a range.
        assert_eq!(rewrite("max(B, 1, 5)"), None);
        // Left as typed: not a range call.
        assert_eq!(rewrite("at(B,1,5)"), None);
        assert_eq!(rewrite("round(B,1,5)"), None);
        assert_eq!(rewrite("atan2(1,5)"), None);
        assert_eq!(rewrite("if(A>0,1,5)"), None);
        // Not two plain numbers.
        assert_eq!(rewrite("max(B,1,C)"), None);
        assert_eq!(rewrite("max(B,-1,5)"), None);
        assert_eq!(rewrite("max(B,1,5e2)"), None);
    }
}
