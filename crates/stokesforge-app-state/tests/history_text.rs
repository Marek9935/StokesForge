//! Wording of the history labels (issue #46): the English of the catalog says
//! the same thing as the `Display` of `stokesforge-commands`, variant by
//! variant, plurals are Fluent's, and the pseudo-language covers every
//! sentence.
//!
//! The tests build their own loader (`build_loader`): the language of the
//! global loader depends on the environment (`STOKESFORGE_LANG`, preferences).

use stokesforge_analysis::ops::OperationKind;
use stokesforge_app_state::history_text::history_text_in;
use stokesforge_app_state::i18n::{build_loader, pseudo, Language};
use stokesforge_commands::{ColumnRef, HistoryLabel, MacroKind, PlotEditKind};

fn name(text: &str) -> Option<String> {
    Some(text.to_string())
}

/// The rank of each variant, exhaustive `match` with no default branch: a
/// variant added to `HistoryLabel` does not compile here until it has a rank,
/// and the coverage test then requires examples.
fn rank(label: &HistoryLabel) -> usize {
    match label {
        HistoryLabel::SetCells { .. } => 0,
        HistoryLabel::AddColumn => 1,
        HistoryLabel::DeleteColumn { .. } => 2,
        HistoryLabel::Rename { .. } => 3,
        HistoryLabel::Move { .. } => 4,
        HistoryLabel::SetColumnType { .. } => 5,
        HistoryLabel::InsertRows { .. } => 6,
        HistoryLabel::DeleteRows { .. } => 7,
        HistoryLabel::DeleteCells { .. } => 8,
        HistoryLabel::AddFolder { .. } => 9,
        HistoryLabel::AddWorkbook => 10,
        HistoryLabel::AddEmptyWorkbook => 11,
        HistoryLabel::AddWorksheet => 12,
        HistoryLabel::AddGraph => 13,
        HistoryLabel::AddMatrix => 14,
        HistoryLabel::AddNote => 15,
        HistoryLabel::RemoveNode { .. } => 16,
        HistoryLabel::SetWindowMeta { .. } => 17,
        HistoryLabel::SetNoteText { .. } => 18,
        HistoryLabel::SetColumnMeta { .. } => 19,
        HistoryLabel::SetPlotSpec { .. } => 20,
        HistoryLabel::SetSavedColors => 21,
        HistoryLabel::Macro(_) => 22,
        HistoryLabel::EditPlotSpec(_) => 23,
        HistoryLabel::DuplicateWorksheet { .. } => 24,
        HistoryLabel::AddCalculation(_) => 25,
        HistoryLabel::UpdateCalculation(_) => 26,
        HistoryLabel::RemoveCalculation { .. } => 27,
        HistoryLabel::ReplaceAnalysisData(_) => 28,
        HistoryLabel::RestoreOriginalData(_) => 29,
        HistoryLabel::TransposeRows => 30,
        HistoryLabel::RecalculateTranspose => 31,
    }
}

const VARIANTS: usize = 32;

/// Each variant, in **each** of its forms (known target or not, 0, 1 and
/// several rows, each kind of group).
fn all_labels() -> Vec<HistoryLabel> {
    let mut labels = vec![
        HistoryLabel::SetCells { column: None },
        HistoryLabel::SetCells {
            column: Some(ColumnRef(0)),
        },
        HistoryLabel::SetCells {
            column: Some(ColumnRef(27)),
        },
        HistoryLabel::AddColumn,
        HistoryLabel::TransposeRows,
        HistoryLabel::RecalculateTranspose,
        HistoryLabel::DeleteColumn { column: None },
        HistoryLabel::DeleteColumn {
            column: Some(ColumnRef(1)),
        },
        HistoryLabel::Rename {
            from: name("Book1"),
            to: "Measurements".to_string(),
        },
        HistoryLabel::Rename {
            from: None,
            to: "Measurements".to_string(),
        },
        HistoryLabel::Move { name: None },
        HistoryLabel::Move {
            name: name("Sheet2"),
        },
        HistoryLabel::SetColumnType { column: None },
        HistoryLabel::SetColumnType {
            column: Some(ColumnRef(2)),
        },
        HistoryLabel::AddFolder {
            name: "Experiments".to_string(),
        },
        HistoryLabel::AddWorkbook,
        HistoryLabel::AddEmptyWorkbook,
        HistoryLabel::AddWorksheet,
        HistoryLabel::DuplicateWorksheet { name: None },
        HistoryLabel::DuplicateWorksheet {
            name: name("Sheet2"),
        },
        HistoryLabel::AddGraph,
        HistoryLabel::AddMatrix,
        HistoryLabel::AddNote,
        HistoryLabel::RemoveNode { name: None },
        HistoryLabel::RemoveNode {
            name: name("Sheet2"),
        },
        HistoryLabel::SetWindowMeta { name: None },
        HistoryLabel::SetWindowMeta {
            name: name("Book1"),
        },
        HistoryLabel::SetNoteText { name: None },
        HistoryLabel::SetNoteText { name: name("N1") },
        HistoryLabel::SetColumnMeta { column: None },
        HistoryLabel::SetColumnMeta {
            column: Some(ColumnRef(0)),
        },
        HistoryLabel::SetPlotSpec { name: None },
        HistoryLabel::SetPlotSpec {
            name: name("Graph1"),
        },
        HistoryLabel::SetSavedColors,
    ];
    for count in [0, 1, 2, 3, 1000] {
        labels.push(HistoryLabel::InsertRows { count });
        labels.push(HistoryLabel::DeleteRows { count });
        labels.push(HistoryLabel::DeleteCells { count });
    }
    labels.extend(MacroKind::ALL.map(HistoryLabel::Macro));
    labels.extend(PlotEditKind::ALL.map(HistoryLabel::EditPlotSpec));
    for kind in OperationKind::ALL {
        labels.push(HistoryLabel::AddCalculation(kind));
        labels.push(HistoryLabel::ReplaceAnalysisData(kind));
        labels.push(HistoryLabel::RestoreOriginalData(Some(kind)));
        labels.push(HistoryLabel::UpdateCalculation(kind));
        for with_columns in [false, true] {
            labels.push(HistoryLabel::RemoveCalculation {
                kind: Some(kind),
                with_columns,
            });
        }
    }
    for with_columns in [false, true] {
        labels.push(HistoryLabel::RemoveCalculation {
            kind: None,
            with_columns,
        });
    }
    labels.push(HistoryLabel::RestoreOriginalData(None));
    labels
}

#[test]
fn the_samples_cover_every_variant() {
    let covered: std::collections::BTreeSet<usize> = all_labels().iter().map(rank).collect();
    assert_eq!(covered, (0..VARIANTS).collect());
}

/// The `en` catalog and the `Display` of `stokesforge-commands` (logs, CLI) say
/// **the same thing**, on each variant and each form: without this test, two
/// wordings of the same sentence would diverge silently.
#[test]
fn the_english_catalogue_matches_the_display_of_the_label() {
    let english = build_loader(Language::English);
    for label in all_labels() {
        assert_eq!(
            history_text_in(&english, &label),
            label.to_string(),
            "{label:?}"
        );
    }
}

/// The two cases of the issue: the plural is Fluent's (CLDR rules).
#[test]
fn insert_rows_pluralizes_through_fluent() {
    let english = build_loader(Language::English);
    let insert = |count| history_text_in(&english, &HistoryLabel::InsertRows { count });
    assert_eq!(insert(1), "Insert 1 Row");
    assert_eq!(insert(3), "Insert 3 Rows");
    assert_eq!(insert(0), "Insert 0 Rows");
    // No thousands separator: "1000", like the `Display`.
    assert_eq!(insert(1000), "Insert 1000 Rows");
    let delete = |count| history_text_in(&english, &HistoryLabel::DeleteRows { count });
    assert_eq!(delete(1), "Delete 1 Row");
    assert_eq!(delete(5), "Delete 5 Rows");
}

/// A column is named by its grid letter; a name chosen by the user is in
/// quotes, with no bidirectional isolation mark (the interface fonts would
/// draw them as squares).
#[test]
fn columns_are_letters_and_names_are_quoted() {
    let english = build_loader(Language::English);
    let text = |label| history_text_in(&english, &label);
    assert_eq!(
        text(HistoryLabel::SetCells {
            column: Some(ColumnRef(0))
        }),
        "Edit Cells in Column A"
    );
    assert_eq!(
        text(HistoryLabel::Rename {
            from: name("A"),
            to: "Z".to_string()
        }),
        "Rename “A” to “Z”"
    );
    assert_eq!(
        text(HistoryLabel::Macro(MacroKind::Import)),
        "Import",
        "the import group"
    );
    for label in all_labels() {
        let phrase = text(label);
        assert!(
            !phrase.contains(['\u{2068}', '\u{2069}']),
            "isolation mark in {phrase:?}"
        );
    }
}

/// Every sentence goes through the catalog: in the pseudo-language, it is framed
/// by `⟦ ⟧`, and each argument (number, letter, name) appears in it as is.
#[test]
fn the_pseudo_language_covers_every_history_sentence() {
    let pseudo_loader = build_loader(Language::PseudoEnglish);
    for label in all_labels() {
        let phrase = history_text_in(&pseudo_loader, &label);
        assert!(
            phrase.starts_with(pseudo::OPEN) && phrase.ends_with(pseudo::CLOSE),
            "{label:?}: {phrase:?} did not go through the catalog"
        );
    }
    let phrase = history_text_in(
        &pseudo_loader,
        &HistoryLabel::Rename {
            from: name("Book1"),
            to: "Measurements".to_string(),
        },
    );
    assert!(
        phrase.contains("Book1") && phrase.contains("Measurements"),
        "{phrase:?}"
    );
}

/// French has its catalog (#49): the same `HistoryLabel`, in French.
#[test]
fn french_translates_the_same_label() {
    let french = build_loader(Language::French);
    assert_eq!(
        history_text_in(&french, &HistoryLabel::InsertRows { count: 3 }),
        "Insérer 3 lignes"
    );
    assert_eq!(
        history_text_in(&french, &HistoryLabel::InsertRows { count: 1 }),
        "Insérer 1 ligne"
    );
}
