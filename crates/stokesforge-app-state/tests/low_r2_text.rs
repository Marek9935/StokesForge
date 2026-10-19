//! Le texte de la boîte « R² faible » et de la pastille (#290), en anglais et
//! en français, singulier et pluriel. Un seul test : la langue est globale au
//! processus, et ce fichier est le sien.

use stokesforge_analysis::ops::OperationKind;
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::r_squared::{
    badge_text, group_by_kind, summary_text, total, LowRSquaredGroup,
};

fn groups() -> Vec<LowRSquaredGroup> {
    // Désordre volontaire, et un type en deux morceaux : 7 + 5 reconvolutions.
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
        "types dans l'ordre de OperationKind::ALL, zéro écarté, totaux fusionnés"
    );
    assert_eq!(total(&found), 13);
    assert!(group_by_kind([]).is_empty());
}

#[test]
fn the_summary_and_the_badge_are_plural_aware_in_english_and_french() {
    i18n::set_language(Language::English);
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
        "R² < 0,6 pour :\n• 1 ajustement\n• 12 reconvolutions\nLes valeurs sont dans le volet Résultats."
    );
    assert_eq!(badge_text(1), "1 R² faible");
    assert_eq!(badge_text(13), "13 R² faibles");
    // Chaque type a son nom au glossaire, au singulier comme au pluriel.
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
