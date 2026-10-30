//! The help of the `F(x)=` cell of the workbook grid (#404): the callable
//! functions grouped by family, each with its signature, one example line and
//! a one-sentence description.
//!
//! The list is **read from the callees** of `stokesforge-core`
//! ([`Callee::all`]), never copied: [`entry`] is an exhaustive `match` on
//! [`Callee`] then [`Func`], without `_`, so a function added to the shared
//! table (or a callee of a later lot: `at`, `integral`...) does not compile
//! until its help is written, and a test checks that every callee has exactly
//! one row.
//!
//! Signatures and examples are **formula code**, not translated; the family
//! names and the descriptions are Fluent keys (`formula-help-*`). The
//! examples are compiled by a test against a sheet `A`..`D`, so an example
//! that the parser refuses cannot ship.

use stokesforge_analysis::functions::Func;
use stokesforge_core::{Callee, ColumnFunc};

use crate::tr;

/// A family of the help, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Operators,
    Powers,
    Trigonometry,
    Error,
    Rounding,
    Choice,
    ColumnValues,
    Constants,
}

impl Family {
    /// The display order.
    const ALL: [Family; 8] = [
        Family::Operators,
        Family::Powers,
        Family::Trigonometry,
        Family::Error,
        Family::Rounding,
        Family::Choice,
        Family::ColumnValues,
        Family::Constants,
    ];

    fn title(self) -> String {
        match self {
            Family::Operators => tr!("formula-help-family-operators"),
            Family::Powers => tr!("formula-help-family-powers"),
            Family::Trigonometry => tr!("formula-help-family-trigonometry"),
            Family::Error => tr!("formula-help-family-error"),
            Family::Rounding => tr!("formula-help-family-rounding"),
            Family::Choice => tr!("formula-help-family-choice"),
            Family::ColumnValues => tr!("formula-help-family-column"),
            Family::Constants => tr!("formula-help-family-constants"),
        }
    }
}

/// One function of the help.
#[derive(Debug, Clone, PartialEq)]
pub struct HelpRow {
    /// The callee the row documents; `None` for an operator.
    pub callee: Option<Callee>,
    /// The name with its arguments, as written in a formula: `logx(x, b)`.
    pub signature: &'static str,
    /// A formula that uses it, on columns `A`..`D`: `logx(B, 2)`.
    pub example: &'static str,
    /// What it computes, in the interface language.
    pub description: String,
    /// The full explanation, shown by a "?" at the end of the row: only the
    /// "Column values" rows have one (their description is a few words).
    pub detail: Option<String>,
}

/// A family and its rows.
#[derive(Debug, Clone, PartialEq)]
pub struct HelpGroup {
    /// The family name, in the interface language.
    pub title: String,
    pub rows: Vec<HelpRow>,
}

struct Entry {
    family: Family,
    signature: &'static str,
    example: &'static str,
    description: String,
    detail: Option<String>,
}

fn row(
    family: Family,
    signature: &'static str,
    example: &'static str,
    description: String,
) -> Entry {
    Entry {
        family,
        signature,
        example,
        description,
        detail: None,
    }
}

/// A row of the "Column values" family: a short description and, behind a "?",
/// the full explanation.
fn column_row(
    signature: &'static str,
    example: &'static str,
    description: String,
    detail: String,
) -> Entry {
    Entry {
        detail: Some(detail),
        ..row(Family::ColumnValues, signature, example, description)
    }
}

/// The help entry of a callee. Exhaustive: no `_`.
fn entry(callee: Callee) -> Entry {
    use Family::{Choice, Constants, Error, Powers, Rounding, Trigonometry};
    match callee {
        Callee::If => row(
            Choice,
            "if(c, a, b)",
            "if(B > 0, B, 0)",
            tr!("formula-help-if"),
        ),
        Callee::Column(func) => match func {
            ColumnFunc::At => column_row(
                "at(B, x)",
                "B / at(B, 340)",
                tr!("formula-help-at"),
                tr!("formula-help-at-detail"),
            ),
            ColumnFunc::Max => column_row(
                "max(B)",
                "B / max(B, 330, 350)",
                tr!("formula-help-max-column"),
                tr!("formula-help-max-column-detail"),
            ),
            ColumnFunc::Min => column_row(
                "min(B)",
                "B - min(B)",
                tr!("formula-help-min-column"),
                tr!("formula-help-min-column-detail"),
            ),
            ColumnFunc::Mean => column_row(
                "mean(B)",
                "B - mean(B)",
                tr!("formula-help-mean"),
                tr!("formula-help-mean-detail"),
            ),
            ColumnFunc::Sum => column_row(
                "sum(B)",
                "B / sum(B)",
                tr!("formula-help-sum"),
                tr!("formula-help-sum-detail"),
            ),
            ColumnFunc::Std => column_row(
                "std(B)",
                "(B - mean(B)) / std(B)",
                tr!("formula-help-std"),
                tr!("formula-help-std-detail"),
            ),
            ColumnFunc::Integral => column_row(
                "integral(B)",
                "B / integral(B)",
                tr!("formula-help-integral"),
                tr!("formula-help-integral-detail"),
            ),
        },
        Callee::Table(func) => match func {
            Func::Exp => row(Powers, "exp(x)", "exp(-B)", tr!("formula-help-exp")),
            Func::Ln => row(Powers, "ln(x)", "ln(B)", tr!("formula-help-ln")),
            Func::Log10 => row(Powers, "log(x)", "-log(B)", tr!("formula-help-log")),
            Func::Log2 => row(Powers, "log2(x)", "log2(B)", tr!("formula-help-log2")),
            Func::Logx => row(Powers, "logx(x, b)", "logx(B, 3)", tr!("formula-help-logx")),
            Func::Sqrt => row(Powers, "sqrt(x)", "sqrt(B)", tr!("formula-help-sqrt")),
            Func::Root => row(Powers, "root(x, n)", "root(B, 3)", tr!("formula-help-root")),
            Func::Pow => row(Powers, "pow(x, y)", "pow(B, 2)", tr!("formula-help-pow")),
            Func::Abs => row(Rounding, "abs(x)", "abs(B - C)", tr!("formula-help-abs")),
            Func::Sign => row(Rounding, "sign(x)", "sign(B)", tr!("formula-help-sign")),
            Func::Floor => row(Rounding, "floor(x)", "floor(B)", tr!("formula-help-floor")),
            Func::Ceil => row(Rounding, "ceil(x)", "ceil(B)", tr!("formula-help-ceil")),
            Func::Round => row(
                Rounding,
                "round(x, n)",
                "round(B, 2)",
                tr!("formula-help-round"),
            ),
            Func::Mod => row(Rounding, "mod(x, y)", "mod(B, 2)", tr!("formula-help-mod")),
            Func::Sin => row(Trigonometry, "sin(x)", "sin(B)", tr!("formula-help-sin")),
            Func::Cos => row(Trigonometry, "cos(x)", "cos(B)", tr!("formula-help-cos")),
            Func::Tan => row(Trigonometry, "tan(x)", "tan(B)", tr!("formula-help-tan")),
            Func::Asin => row(Trigonometry, "asin(x)", "asin(B)", tr!("formula-help-asin")),
            Func::Acos => row(Trigonometry, "acos(x)", "acos(B)", tr!("formula-help-acos")),
            Func::Atan => row(Trigonometry, "atan(x)", "atan(B)", tr!("formula-help-atan")),
            Func::Atan2 => row(
                Trigonometry,
                "atan2(y, x)",
                "atan2(C, B)",
                tr!("formula-help-atan2"),
            ),
            Func::Sinh => row(Trigonometry, "sinh(x)", "sinh(B)", tr!("formula-help-sinh")),
            Func::Cosh => row(Trigonometry, "cosh(x)", "cosh(B)", tr!("formula-help-cosh")),
            Func::Tanh => row(Trigonometry, "tanh(x)", "tanh(B)", tr!("formula-help-tanh")),
            Func::Erf => row(Error, "erf(x)", "erf(B)", tr!("formula-help-erf")),
            Func::Erfc => row(Error, "erfc(x)", "erfc(B)", tr!("formula-help-erfc")),
            Func::Min => row(Choice, "min(x, y)", "min(B, C)", tr!("formula-help-min")),
            Func::Max => row(Choice, "max(x, y)", "max(B, 0)", tr!("formula-help-max")),
            Func::Pi => row(Constants, "pi()", "B * pi()", tr!("formula-help-pi")),
        },
    }
}

/// What a formula can use besides functions: a fixed list, since operators are
/// not callees (the exhaustive `match` above covers the callees only).
fn operator_rows() -> Vec<HelpRow> {
    let operator = |signature, example, description| HelpRow {
        callee: None,
        signature,
        example,
        description,
        detail: None,
    };
    vec![
        operator("+ - * /", "(B + C) / 2", tr!("formula-help-op-arithmetic")),
        operator("^", "B^2", tr!("formula-help-op-power")),
        operator(
            "< <= > >= == !=",
            "B > 0",
            tr!("formula-help-op-comparison"),
        ),
    ]
}

/// The help, family by family, in the interface language. A family with no
/// callee is left out.
pub fn groups() -> Vec<HelpGroup> {
    let entries: Vec<(Callee, Entry)> = Callee::all()
        .map(|callee| (callee, entry(callee)))
        .collect();
    Family::ALL
        .into_iter()
        .filter_map(|family| {
            let mut rows: Vec<HelpRow> = Vec::new();
            if family == Family::Operators {
                rows.extend(operator_rows());
            }
            rows.extend(
                entries
                    .iter()
                    .filter(|(_, entry)| entry.family == family)
                    .map(|(callee, entry)| HelpRow {
                        callee: Some(*callee),
                        signature: entry.signature,
                        example: entry.example,
                        description: entry.description.clone(),
                        detail: entry.detail.clone(),
                    }),
            );
            (!rows.is_empty()).then(|| HelpGroup {
                title: family.title(),
                rows,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_core::{ColumnFormula, NodeId};

    fn all_rows() -> Vec<HelpRow> {
        groups().into_iter().flat_map(|group| group.rows).collect()
    }

    /// The list is the callees', not a copy: a callee with no row (or two)
    /// would show here.
    #[test]
    fn every_callee_has_exactly_one_non_empty_row() {
        let rows = all_rows();
        for callee in Callee::all() {
            let mine: Vec<&HelpRow> = rows
                .iter()
                .filter(|row| row.callee == Some(callee))
                .collect();
            assert_eq!(mine.len(), 1, "{}", callee.name());
            let row = mine[0];
            assert!(!row.signature.is_empty(), "{}", callee.name());
            assert!(!row.example.is_empty(), "{}", callee.name());
            assert!(!row.description.is_empty(), "{}", callee.name());
            assert!(
                row.signature.starts_with(callee.name()),
                "{} signature: {}",
                callee.name(),
                row.signature
            );
        }
        assert_eq!(
            rows.iter().filter(|row| row.callee.is_some()).count(),
            Callee::all().count()
        );
        assert!(rows.iter().any(|row| row.callee.is_none()), "operators");
    }

    /// An example is a formula that the parser accepts, on a sheet `A`..`D`.
    #[test]
    fn every_example_compiles_on_a_four_column_sheet() {
        let sheet: Vec<NodeId> = (1..=4).map(NodeId::from_raw).collect();
        let own = NodeId::from_raw(99);
        for row in all_rows() {
            let formula = ColumnFormula::from_letters(row.example, &sheet, own)
                .unwrap_or_else(|error| panic!("{}: {error}", row.example));
            formula
                .compile()
                .unwrap_or_else(|error| panic!("{}: {error}", row.example));
        }
    }

    /// An example calls the function it illustrates, with an allowed count.
    #[test]
    fn every_example_calls_its_own_function() {
        for row in all_rows() {
            let Some(callee) = row.callee else { continue };
            let name = callee.name();
            assert!(
                row.example
                    .to_ascii_lowercase()
                    .contains(&format!("{name}(")),
                "{name}: {}",
                row.example
            );
        }
    }

    /// A "Column values" row says little and explains behind its "?": every one
    /// has a detail, in both languages, and no other family has one.
    #[test]
    fn every_column_values_row_has_a_detail() {
        for language in [
            crate::i18n::Language::English,
            crate::i18n::Language::French,
        ] {
            crate::i18n::set_language(language);
            let rows = all_rows();
            let column_rows: Vec<&HelpRow> = rows
                .iter()
                .filter(|row| matches!(row.callee, Some(Callee::Column(_))))
                .collect();
            assert_eq!(column_rows.len(), ColumnFunc::ALL.len());
            for row in column_rows {
                let detail = row.detail.as_deref().unwrap_or_default();
                assert!(detail.len() > row.description.len(), "{}", row.signature);
            }
            assert!(rows
                .iter()
                .filter(|row| !matches!(row.callee, Some(Callee::Column(_))))
                .all(|row| row.detail.is_none()));
        }
        crate::i18n::set_language(crate::i18n::Language::English);
    }

    #[test]
    fn the_families_follow_the_display_order_and_the_texts_exist_in_both_languages() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let titles: Vec<String> = groups().into_iter().map(|group| group.title).collect();
        assert_eq!(titles.len(), Family::ALL.len());
        assert!(titles.iter().all(|title| !title.is_empty()));
        crate::i18n::set_language(crate::i18n::Language::French);
        let french: Vec<String> = groups().into_iter().map(|group| group.title).collect();
        assert_ne!(titles, french);
        crate::i18n::set_language(crate::i18n::Language::English);
    }
}
