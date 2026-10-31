//! The load warning for discarded calculations (#278): singular and plural,
//! at most five identifiers then "…", English/French parity.
//!
//! A single test, in its own binary: the language loader is global.

use std::path::Path;

use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::ops::Operation;
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::state::{Action, AppState, StatusLevel};
use stokesforge_core::{
    CalculationDraft, Column, ColumnMeta, OutputDraft, Project, SeriesRef, SeriesResult,
};
use stokesforge_session::Session;

/// A file with `total` "subtract" correction calculations on as many columns;
/// the first `damaged` are made unreadable in the bytes.
/// Returns the calculation identifiers, ascending.
fn write_file(path: &Path, total: usize, damaged: usize) -> Vec<u64> {
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let mut ids = Vec::new();
    for n in 0..total {
        let y = project
            .add_column(
                sheet,
                None,
                Some(&format!("Y{n}")),
                Column::from_f64(&[1.0, 2.0]),
            )
            .unwrap();
        let id = project
            .add_calculation(CalculationDraft {
                operation: Operation::Correct {
                    op: CorrectOp::Subtract,
                },
                series: vec![SeriesRef {
                    x: None,
                    y,
                    sigma: None,
                }],
                aux: None,
                outputs: vec![OutputDraft {
                    series: 0,
                    key: "corrected".into(),
                    index: None,
                    meta: ColumnMeta::default(),
                    data: vec![0.0, 1.0],
                }],
                results: vec![SeriesResult::default()],
                keep_results: true,
                auto_recalc: false,
                low_r2_dialog: false,
            })
            .unwrap();
        ids.push(id.raw());
    }
    stokesforge_persistence::save(&project, path).unwrap();
    // Each occurrence of "subtract" is an operation; the first `damaged` are
    // corrupted (the file order is the identifier order).
    let mut bytes = std::fs::read(path).unwrap();
    let mut seen = 0;
    let mut at = 0;
    while let Some(found) = bytes[at..]
        .windows(8)
        .position(|w| w == b"subtract")
        .map(|p| p + at)
    {
        if seen < damaged {
            bytes[found..found + 8].copy_from_slice(b"subtraXX");
        }
        seen += 1;
        at = found + 8;
    }
    assert_eq!(seen, total, "one operation occurrence per calculation");
    std::fs::write(path, bytes).unwrap();
    ids
}

/// Loads `path` into a fresh session: the status bar message.
fn load_message(path: &Path) -> (StatusLevel, String) {
    let session = Session::spawn(Project::new("Empty").unwrap(), None).unwrap();
    let mut state = AppState::new_with_preferences(
        session.handle(),
        path.display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    state.dispatch(Action::Load);
    assert!(state.alert().is_none(), "the project loads");
    let status = state.status().expect("a message");
    (status.level, status.text.clone())
}

fn list(ids: &[u64]) -> String {
    ids.iter()
        .map(|i| format!("#{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[test]
fn the_discarded_calculations_warning_counts_and_abbreviates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");

    let _language = i18n::test_language(Language::English);
    // No discard: "Loaded", as information, no warning.
    write_file(&path, 2, 0);
    let (level, text) = load_message(&path);
    assert_eq!(level, StatusLevel::Info);
    assert!(text.starts_with("Loaded from"), "{text}");

    // Just one: singular, its identifier.
    let ids = write_file(&path, 2, 1);
    let (level, text) = load_message(&path);
    assert_eq!(level, StatusLevel::Warning);
    assert_eq!(
        text,
        format!(
            "1 calculation was discarded from the project ({})",
            list(&ids[..1])
        )
    );

    // Two: plural and count.
    let ids = write_file(&path, 3, 2);
    let (_, text) = load_message(&path);
    assert_eq!(
        text,
        format!(
            "2 calculations were discarded from the project ({})",
            list(&ids[..2])
        )
    );

    // Exactly five: all named, no ellipsis.
    let ids = write_file(&path, 6, 5);
    let (_, text) = load_message(&path);
    assert_eq!(
        text,
        format!(
            "5 calculations were discarded from the project ({})",
            list(&ids[..5])
        )
    );
    assert!(!text.contains('…'));

    // Six: five identifiers then "…", the count stays exact.
    let ids = write_file(&path, 8, 6);
    let (_, text) = load_message(&path);
    assert_eq!(
        text,
        format!(
            "6 calculations were discarded from the project ({}, …)",
            list(&ids[..5])
        )
    );
    assert!(
        !text.contains(&format!("#{}", ids[5])),
        "the sixth is not named"
    );

    // French parity: same structure, same list.
    i18n::set_language(Language::French);
    let ids = write_file(&path, 2, 1);
    let (level, text) = load_message(&path);
    assert_eq!(level, StatusLevel::Warning);
    assert_eq!(
        text,
        // lang: fr
        format!("1 calcul a été écarté du projet ({})", list(&ids[..1]))
    );
    let ids = write_file(&path, 8, 6);
    let (_, text) = load_message(&path);
    assert_eq!(
        text,
        format!(
            // lang: fr
            "6 calculs ont été écartés du projet ({}, …)",
            list(&ids[..5])
        )
    );
    i18n::set_language(Language::English);
}
