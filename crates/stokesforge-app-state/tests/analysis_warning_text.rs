//! The sentences of the calculation warnings (#281, #203) in both languages of
//! the catalog: none empty, no bare key, the interpolated number, the French
//! non-breaking space before ":". Clean loaders (`build_loader`): the global
//! language is not touched.

use stokesforge_app_state::i18n::{build_loader, Language};
use stokesforge_app_state::tr;

/// The `analysis-warning-*` keys of the `en` catalog, and their text in `loader`.
fn phrases(language: Language) -> Vec<(&'static str, String)> {
    let loader = build_loader(language);
    vec![
        (
            "analysis-warning-step-not-uniform",
            tr!(in &loader; "analysis-warning-step-not-uniform"),
        ),
        (
            "analysis-warning-irf-step",
            tr!(in &loader; "analysis-warning-irf-step"),
        ),
        (
            "analysis-warning-negative-clipped",
            tr!(in &loader; "analysis-warning-negative-clipped", count = 4_i64),
        ),
        (
            "analysis-warning-not-converged",
            tr!(in &loader; "analysis-warning-not-converged"),
        ),
        (
            "analysis-warning-undetermined",
            tr!(in &loader; "analysis-warning-undetermined", count = 3_i64),
        ),
        (
            "analysis-warning-irf-truncated",
            tr!(in &loader; "analysis-warning-irf-truncated"),
        ),
        (
            "analysis-warning-bridged-reference-gap",
            tr!(in &loader; "analysis-warning-bridged-reference-gap", count = 3_i64),
        ),
        (
            "analysis-warning-division-by-zero",
            tr!(in &loader; "analysis-warning-division-by-zero", count = 2_i64),
        ),
    ]
}

#[test]
fn each_warning_has_a_non_empty_distinct_sentence_in_english_and_french() {
    for language in [Language::English, Language::French] {
        let found = phrases(language);
        for (key, text) in &found {
            assert!(!text.trim().is_empty(), "{language:?} {key}");
            assert!(!text.contains(key), "{language:?}: bare key {key}");
            assert!(!text.contains("{ $"), "{language:?} {key}: {text}");
        }
        let mut texts: Vec<&String> = found.iter().map(|(_, t)| t).collect();
        texts.sort();
        texts.dedup();
        assert_eq!(texts.len(), found.len(), "{language:?}: distinct sentences");
    }
}

#[test]
fn the_non_convergence_sentence_states_the_stop_and_the_lead_in_both_languages() {
    let en = phrases(Language::English)[3].1.clone();
    let fr = phrases(Language::French)[3].1.clone();
    assert!(
        en.contains("did not converge") && en.contains("starting values"),
        "{en}"
    );
    assert!(
        // lang: fr
        fr.contains("pas convergé") && fr.contains("valeurs de départ"),
        "{fr}"
    );
    assert_ne!(en, fr);
}

#[test]
fn numbers_are_interpolated_without_a_thousands_separator() {
    let loader_en = build_loader(Language::English);
    let loader_fr = build_loader(Language::French);
    let n = 12_345_i64;
    for text in [
        tr!(in &loader_en; "analysis-warning-undetermined", count = n),
        tr!(in &loader_fr; "analysis-warning-undetermined", count = n),
        tr!(in &loader_en; "analysis-warning-negative-clipped", count = n),
        tr!(in &loader_fr; "analysis-warning-negative-clipped", count = n),
        tr!(in &loader_en; "analysis-warning-bridged-reference-gap", count = n),
        tr!(in &loader_fr; "analysis-warning-bridged-reference-gap", count = n),
        tr!(in &loader_en; "analysis-warning-division-by-zero", count = n),
        tr!(in &loader_fr; "analysis-warning-division-by-zero", count = n),
    ] {
        assert!(text.contains("12345"), "{text}");
    }
}

#[test]
fn in_french_a_colon_is_preceded_by_a_non_breaking_space() {
    for (key, text) in phrases(Language::French) {
        for (i, c) in text.char_indices() {
            if c == ':' {
                let before = text[..i].chars().next_back();
                assert!(
                    matches!(before, Some('\u{a0}' | '\u{202f}')),
                    "{key}: ':' without a non-breaking space before it in {text:?}"
                );
            }
        }
    }
}

#[test]
fn in_english_the_colon_is_attached() {
    for (key, text) in phrases(Language::English) {
        assert!(!text.contains(" :"), "{key}: {text:?}");
    }
}
