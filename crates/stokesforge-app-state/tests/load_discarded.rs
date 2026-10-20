//! L'avertissement de chargement des calculs écartés (#278) : singulier et
//! pluriel, au plus cinq identifiants puis « … », parité anglais/français.
//!
//! Un seul test, dans son propre binaire : le chargeur de langue est global.

use std::path::Path;

use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::ops::Operation;
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::state::{Action, AppState, StatusLevel};
use stokesforge_core::{
    CalculationDraft, Column, ColumnMeta, OutputDraft, Project, SeriesRef, SeriesResult,
};
use stokesforge_session::Session;

/// Un fichier à `total` calculs de correction « subtract » sur autant de
/// colonnes ; les `damaged` premiers sont rendus illisibles dans les octets.
/// Rend les identifiants de calcul, croissants.
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
    // Chaque occurrence de « subtract » est une opération ; on abîme les
    // `damaged` premières (l'ordre du fichier est celui des identifiants).
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
    assert_eq!(seen, total, "une occurrence d'opération par calcul");
    std::fs::write(path, bytes).unwrap();
    ids
}

/// Charge `path` dans une session neuve : le message de la barre d'état.
fn load_message(path: &Path) -> (StatusLevel, String) {
    let session = Session::spawn(Project::new("Vide").unwrap(), None).unwrap();
    let mut state = AppState::new_with_preferences(
        session.handle(),
        path.display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    state.dispatch(Action::Load);
    assert!(state.alert().is_none(), "le projet se charge");
    let status = state.status().expect("un message");
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

    i18n::set_language(Language::English);
    // Aucun écart : « Loaded », en information, pas d'avertissement.
    write_file(&path, 2, 0);
    let (level, text) = load_message(&path);
    assert_eq!(level, StatusLevel::Info);
    assert!(text.starts_with("Loaded from"), "{text}");

    // Un seul : singulier, son identifiant.
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

    // Deux : pluriel et compte.
    let ids = write_file(&path, 3, 2);
    let (_, text) = load_message(&path);
    assert_eq!(
        text,
        format!(
            "2 calculations were discarded from the project ({})",
            list(&ids[..2])
        )
    );

    // Cinq pile : tous nommés, pas de points de suspension.
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

    // Six : cinq identifiants puis « … », le compte reste exact.
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
        "le sixième n'est pas nommé"
    );

    // Parité française : même structure, même liste.
    i18n::set_language(Language::French);
    let ids = write_file(&path, 2, 1);
    let (level, text) = load_message(&path);
    assert_eq!(level, StatusLevel::Warning);
    assert_eq!(
        text,
        format!("1 calcul a été écarté du projet ({})", list(&ids[..1]))
    );
    let ids = write_file(&path, 8, 6);
    let (_, text) = load_message(&path);
    assert_eq!(
        text,
        format!(
            "6 calculs ont été écartés du projet ({}, …)",
            list(&ids[..5])
        )
    );
    i18n::set_language(Language::English);
}
