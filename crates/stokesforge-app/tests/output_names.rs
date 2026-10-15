//! Les noms des colonnes de sortie d'analyse (#256) : la règle est dans
//! `stokesforge_core::OutputName`, l'application la traduit. L'anglais du
//! catalogue dit **la même chose** que `OutputName::english()` (utilisé par
//! `pyst`), variante par variante ; sans ce test, deux rédactions de la même
//! phrase divergeraient sans bruit.
//!
//! Les tests bâtissent leur propre chargeur (`build_loader`) : la langue du
//! chargeur global dépend de l'environnement.

use stokesforge_analysis::ops::OperationKind;
use stokesforge_app::analysis::output_name_text_in;
use stokesforge_app::history_text::kind_title;
use stokesforge_app::i18n::{build_loader, pseudo, Language};
use stokesforge_core::{family_english, OutputName};

/// Le rang de chaque variante, `match` exhaustif sans branche par défaut : une
/// variante ajoutée à `OutputName` ne compile pas ici tant qu'elle n'a pas de
/// rang, et le test de couverture exige alors des exemples.
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
    }
}

const VARIANTS: usize = 12;

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
        names.push(OutputName::Source { name: text(name) });
    }
    names
}

#[test]
fn the_samples_cover_every_variant() {
    let covered: std::collections::BTreeSet<usize> = all_names().iter().map(rank).collect();
    assert_eq!(covered, (0..VARIANTS).collect());
}

/// Le catalogue `en` et `english()` (scripts `pyst`, journaux) disent la même
/// chose, sur chaque variante.
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

/// Le titre de famille (commentaire des colonnes) : les clés `analysis-kind-*`
/// en anglais disent `family_english`.
#[test]
fn the_english_catalogue_matches_the_family_titles() {
    let english = build_loader(Language::English);
    for kind in OperationKind::ALL {
        assert_eq!(kind_title(&english, kind), family_english(kind), "{kind:?}");
    }
}

/// Toute variante traduite passe par le catalogue (hors le repli, qui est le
/// nom de l'utilisateur) : en pseudo-langue, la phrase est encadrée de `⟦ ⟧`.
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
            "{name:?} : {phrase:?} n'est pas passé par le catalogue"
        );
    }
}

/// Le français a son catalogue : même donnée, autre langue.
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
