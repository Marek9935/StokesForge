//! The text of the "low R²" dialog and of the badge (#290), in English and
//! French, singular and plural. A single test: the language is global to the
//! process, and this file is its own.

use stokesforge_analysis::ops::OperationKind;
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::r_squared::{
    badge_text, group_by_kind, summary_text, total, LowRSquaredGroup,
};

fn groups() -> Vec<LowRSquaredGroup> {
    // Deliberately unordered, and one type in two pieces: 7 + 5 reconvolutions.
    group_by_kind([
        (OperationKind::Fit, 1),
        (OperationKind::Reconvolution, 7),
        (OperationKind::Peaks, 0),
        (OperationKind::Reconvolution, 5),
    ])
}

#[test]
fn the_counts_group_by_type_in_a_fixed_order() {
    let found = groups();
    assert_eq!(
        found,
        vec![
            LowRSquaredGroup {
                kind: OperationKind::Fit,
                count: 1
            },
            LowRSquaredGroup {
                kind: OperationKind::Reconvolution,
                count: 12
            },
        ],
        "types in the order of OperationKind::ALL, zero dropped, totals merged"
    );
    assert_eq!(total(&found), 13);
    assert!(group_by_kind([]).is_empty());
}

#[test]
fn the_summary_and_the_badge_are_plural_aware_in_english_and_french() {
    let _language = i18n::test_language(Language::English);
    let text = summary_text(&groups());
    assert_eq!(
        text,
        "R² < 0.6 for:\n• 1 fit\n• 12 reconvolutions\nSee the Results pane for the values."
    );
    assert_eq!(badge_text(13), "13 low R²");

    i18n::set_language(Language::French);
    let text = summary_text(&groups());
    assert_eq!(
        text,
        // lang: fr
        "R² < 0,6 pour :\n• 1 ajustement\n• 12 reconvolutions\nLes valeurs sont dans le volet Résultats."
    );
    assert_eq!(badge_text(1), "1 R² faible");
    assert_eq!(badge_text(13), "13 R² faibles");
    // Each type has its name in the glossary, in the singular as in the plural.
    for kind in OperationKind::ALL {
        for count in [1, 2] {
            let line =
                stokesforge_app_state::r_squared::group_line(&LowRSquaredGroup { kind, count });
            assert!(
                line.starts_with(&count.to_string()) && line.len() > 3,
                "{line}"
            );
        }
    }
    i18n::set_language(Language::English);
}

// ---- Hardening (#290): names by type, plurals, grouping of the dialog ----

use stokesforge_app_state::r_squared::group_line;

fn with_language<T>(language: Language, f: impl FnOnce() -> T) -> T {
    let _language = i18n::test_language(language);
    f()
}

/// Exhaustive match without `_`: a type added to `OperationKind` breaks the
/// compilation of this test, which then forces giving it a name in the glossary.
fn rank(kind: OperationKind) -> usize {
    match kind {
        OperationKind::Baseline => 0,
        OperationKind::Correct => 1,
        OperationKind::Integrate => 2,
        OperationKind::Fit => 3,
        OperationKind::Peaks => 4,
        OperationKind::Reconvolution => 5,
        OperationKind::DirectDeconvolution => 6,
        OperationKind::Smooth => 7,
    }
}

fn count_line(kind: OperationKind, count: usize) -> String {
    group_line(&LowRSquaredGroup { kind, count })
}

/// The name of a type without its counter: "12 reconvolutions" -> "reconvolutions".
fn type_name(kind: OperationKind, count: usize) -> String {
    let line = count_line(kind, count);
    let prefix = format!("{count} ");
    line.strip_prefix(&prefix)
        .unwrap_or_else(|| panic!("\"{line}\" does not start with \"{prefix}\""))
        .to_string()
}

#[test]
fn every_type_has_a_distinct_singular_and_plural_name_in_fr_and_en() {
    for language in [Language::English, Language::French] {
        with_language(language, || {
            assert_eq!(OperationKind::ALL.len(), 8);
            let mut singulars = Vec::new();
            let mut plurals = Vec::new();
            for (i, kind) in OperationKind::ALL.into_iter().enumerate() {
                assert_eq!(rank(kind), i, "ALL is in rank order");
                let (one, two) = (type_name(kind, 1), type_name(kind, 2));
                assert!(!one.trim().is_empty(), "{kind:?} {language:?}");
                assert!(!two.trim().is_empty(), "{kind:?} {language:?}");
                assert_ne!(one, two, "{kind:?} {language:?}: singular = plural");
                assert!(
                    !one.contains("low-r2") && !two.contains("low-r2"),
                    "unresolved key: {one} / {two}"
                );
                singulars.push(one);
                plurals.push(two);
            }
            for names in [&singulars, &plurals] {
                let mut sorted_names = names.clone();
                sorted_names.sort();
                sorted_names.dedup();
                assert_eq!(
                    sorted_names.len(),
                    names.len(),
                    "type names merged: {names:?}"
                );
            }
        });
    }
}

#[test]
fn plurals_follow_the_catalog_rule_in_english_and_french() {
    let values = [0usize, 1, 2, 5, 12, 21, 100, 101];
    // (English: 0 is plural; French: 0 and 1 are singular)
    with_language(Language::English, || {
        for n in values {
            let expected = if n == 1 { "fit" } else { "fits" };
            assert_eq!(count_line(OperationKind::Fit, n), format!("{n} {expected}"));
            let expected = if n == 1 {
                "peak decomposition"
            } else {
                "peak decompositions"
            };
            assert_eq!(
                count_line(OperationKind::Peaks, n),
                format!("{n} {expected}")
            );
        }
        assert_eq!(badge_text(0), "0 low R²");
        assert_eq!(badge_text(21), "21 low R²");
    });
    with_language(Language::French, || {
        for n in values {
            let expected = if n <= 1 { "ajustement" } else { "ajustements" };
            assert_eq!(
                count_line(OperationKind::Fit, n),
                format!("{n} {expected}"),
                "n = {n}"
            );
            let expected = if n <= 1 {
                "déconvolution directe"
            } else {
                "déconvolutions directes"
            };
            assert_eq!(
                count_line(OperationKind::DirectDeconvolution, n),
                format!("{n} {expected}"),
                "n = {n}"
            );
            let expected = if n <= 1 { "R² faible" } else { "R² faibles" };
            assert_eq!(badge_text(n), format!("{n} {expected}"), "n = {n}");
        }
    });
}

#[test]
fn the_dialog_message_for_one_type_two_types_and_all_types() {
    with_language(Language::English, || {
        let one = group_by_kind([(OperationKind::Fit, 1)]);
        assert_eq!(
            summary_text(&one),
            "R² < 0.6 for:\n• 1 fit\nSee the Results pane for the values."
        );
        let twelve = group_by_kind([(OperationKind::Reconvolution, 12)]);
        assert_eq!(
            summary_text(&twelve),
            "R² < 0.6 for:\n• 12 reconvolutions\nSee the Results pane for the values."
        );
        let two = group_by_kind([(OperationKind::Reconvolution, 12), (OperationKind::Fit, 1)]);
        assert_eq!(
            summary_text(&two),
            "R² < 0.6 for:\n• 1 fit\n• 12 reconvolutions\nSee the Results pane for the values."
        );
        // All types, passed in reverse order: the output order is that of
        // ALL, one line per type, each with its counter.
        let all_kinds = group_by_kind(
            OperationKind::ALL
                .into_iter()
                .rev()
                .map(|k| (k, rank(k) + 1)),
        );
        assert_eq!(all_kinds.len(), 8);
        let summary = summary_text(&all_kinds);
        let lines: Vec<&str> = summary.lines().collect();
        assert_eq!(lines.len(), 10, "head + 8 types + pointer: {summary}");
        assert_eq!(lines[0], "R² < 0.6 for:");
        for kind in OperationKind::ALL {
            assert_eq!(
                lines[1 + rank(kind)],
                format!("• {}", count_line(kind, rank(kind) + 1)),
                "{kind:?}"
            );
        }
        assert_eq!(total(&all_kinds), 36);
    });
}

#[test]
fn grouping_merges_and_drops_zeros_whatever_the_order() {
    let a = group_by_kind([
        (OperationKind::Peaks, 2),
        (OperationKind::Fit, 3),
        (OperationKind::Peaks, 4),
        (OperationKind::Fit, 0),
    ]);
    let b = group_by_kind([
        (OperationKind::Fit, 3),
        (OperationKind::Peaks, 6),
        (OperationKind::Baseline, 0),
    ]);
    assert_eq!(a, b, "merging and order independent of the input order");
    assert_eq!(
        a.iter().map(|g| (g.kind, g.count)).collect::<Vec<_>>(),
        vec![(OperationKind::Fit, 3), (OperationKind::Peaks, 6)]
    );
    assert!(group_by_kind([(OperationKind::Fit, 0)]).is_empty());
}
