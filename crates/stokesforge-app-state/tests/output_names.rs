//! The names of the analysis output columns (#256): the rule is in
//! `stokesforge_core::OutputName`, the application translates it. The English
//! of the catalog says **the same thing** as `OutputName::english()` (used by
//! `pyst`), variant by variant; without this test, two wordings of the same
//! sentence would diverge silently.
//!
//! The tests build their own loader (`build_loader`): the language of the
//! global loader depends on the environment.

use stokesforge_analysis::ops::OperationKind;
use stokesforge_app_state::analysis::output_name_text_in;
use stokesforge_app_state::history_text::kind_title;
use stokesforge_app_state::i18n::{build_loader, pseudo, Language};
use stokesforge_core::{family_english, OutputName};

/// The rank of each variant, exhaustive `match` with no default branch: a
/// variant added to `OutputName` does not compile here until it has a rank,
/// and the coverage test then requires examples.
fn rank(name: &OutputName) -> usize {
    match name {
        OutputName::BaselineCorrected { .. } => 0,
        OutputName::Corrected { .. } => 1,
        OutputName::Baseline { .. } => 2,
        OutputName::Cumulative { .. } => 3,
        OutputName::Normalized { .. } => 4,
        OutputName::Fit { .. } => 5,
        OutputName::Residuals { .. } => 6,
        OutputName::Peak { .. } => 7,
        OutputName::Background { .. } => 8,
        OutputName::PeaksSum { .. } => 9,
        OutputName::Deconvolved { .. } => 10,
        OutputName::Source { .. } => 11,
        OutputName::Smoothed { .. } => 12,
    }
}

const VARIANTS: usize = 13;

fn all_names() -> Vec<OutputName> {
    let text = |s: &str| s.to_string();
    let mut names = Vec::new();
    for (name, column) in [("Intensity", "B"), ("B", "B"), ("Signal (a.u.)", "AB")] {
        names.push(OutputName::BaselineCorrected {
            name: text(name),
            column: text(column),
        });
        names.push(OutputName::Corrected { name: text(name) });
        names.push(OutputName::Baseline {
            column: text(column),
        });
        names.push(OutputName::Cumulative {
            column: text(column),
        });
        names.push(OutputName::Normalized {
            column: text(column),
        });
        names.push(OutputName::Fit { name: text(name) });
        names.push(OutputName::Residuals { name: text(name) });
        for index in [1, 2, 12] {
            names.push(OutputName::Peak {
                index,
                column: text(column),
            });
        }
        names.push(OutputName::Background {
            column: text(column),
        });
        names.push(OutputName::PeaksSum {
            column: text(column),
        });
        names.push(OutputName::Deconvolved { name: text(name) });
        names.push(OutputName::Smoothed { name: text(name) });
        names.push(OutputName::Source { name: text(name) });
    }
    names
}

#[test]
fn the_samples_cover_every_variant() {
    let covered: std::collections::BTreeSet<usize> = all_names().iter().map(rank).collect();
    assert_eq!(covered, (0..VARIANTS).collect());
}

/// The `en` catalog and `english()` (`pyst` scripts, logs) say the same thing,
/// on each variant.
#[test]
fn the_english_catalogue_matches_the_raw_english_of_the_name() {
    let english = build_loader(Language::English);
    for name in all_names() {
        assert_eq!(
            output_name_text_in(&english, &name),
            name.english(),
            "{name:?}"
        );
    }
}

/// The family title (column comment): the `analysis-kind-*` keys in English
/// say `family_english`.
#[test]
fn the_english_catalogue_matches_the_family_titles() {
    let english = build_loader(Language::English);
    for kind in OperationKind::ALL {
        assert_eq!(kind_title(&english, kind), family_english(kind), "{kind:?}");
    }
}

/// Every translated variant goes through the catalog (except the fallback,
/// which is the user's name): in the pseudo-language, the sentence is framed by `⟦ ⟧`.
#[test]
fn the_pseudo_language_covers_every_output_name() {
    let pseudo_loader = build_loader(Language::PseudoEnglish);
    for name in all_names() {
        let phrase = output_name_text_in(&pseudo_loader, &name);
        if matches!(name, OutputName::Source { .. }) {
            assert_eq!(phrase, name.english());
            continue;
        }
        assert!(
            phrase.starts_with(pseudo::OPEN) && phrase.ends_with(pseudo::CLOSE),
            "{name:?}: {phrase:?} did not go through the catalog"
        );
    }
}

/// French has its catalog: same data, other language.
#[test]
fn french_translates_the_same_name() {
    let french = build_loader(Language::French);
    let english = build_loader(Language::English);
    let name = OutputName::Baseline {
        column: "B".to_string(),
    };
    assert_ne!(
        output_name_text_in(&french, &name),
        output_name_text_in(&english, &name)
    );
}
