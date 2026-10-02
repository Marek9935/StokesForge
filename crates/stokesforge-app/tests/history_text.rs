//! Mise en phrase des libellés d'historique (issue #46) : l'anglais du
//! catalogue dit la même chose que le `Display` de `stokesforge-commands`, variante
//! par variante, les pluriels sont ceux de Fluent, et la pseudo-langue couvre
//! chaque phrase.
//!
//! Les tests bâtissent leur propre chargeur (`build_loader`) : la langue du
//! chargeur global dépend de l'environnement (`STOKESFORGE_LANG`, préférences).

use stokesforge_app::history_text::history_text_in;
use stokesforge_app::i18n::{build_loader, pseudo, Language};
use stokesforge_commands::{ColumnRef, HistoryLabel, MacroKind};

fn name(text: &str) -> Option<String> {
    Some(text.to_string())
}

/// Le rang de chaque variante, `match` exhaustif sans branche par défaut : une
/// variante ajoutée à `HistoryLabel` ne compile pas ici tant qu'elle n'a pas de
/// rang, et le test de couverture exige alors des exemples.
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
    }
}

const VARIANTS: usize = 23;

/// Chaque variante, sous **chacune** de ses formes (cible connue ou non, 0, 1 et
/// plusieurs lignes, chaque nature de groupe).
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
        HistoryLabel::DeleteColumn { column: None },
        HistoryLabel::DeleteColumn {
            column: Some(ColumnRef(1)),
        },
        HistoryLabel::Rename {
            from: name("Book1"),
            to: "Mesures".to_string(),
        },
        HistoryLabel::Rename {
            from: None,
            to: "Mesures".to_string(),
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
            name: "Manips".to_string(),
        },
        HistoryLabel::AddWorkbook,
        HistoryLabel::AddEmptyWorkbook,
        HistoryLabel::AddWorksheet,
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
    labels
}

#[test]
fn the_samples_cover_every_variant() {
    let covered: std::collections::BTreeSet<usize> = all_labels().iter().map(rank).collect();
    assert_eq!(covered, (0..VARIANTS).collect());
}

/// Le catalogue `en` et le `Display` de `stokesforge-commands` (journaux, CLI) disent
/// **la même chose**, sur chaque variante et chaque forme : sans ce test, deux
/// rédactions de la même phrase divergeraient sans bruit.
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

/// Les deux cas de l'issue : le pluriel est celui de Fluent (règles CLDR).
#[test]
fn insert_rows_pluralizes_through_fluent() {
    let english = build_loader(Language::English);
    let insert = |count| history_text_in(&english, &HistoryLabel::InsertRows { count });
    assert_eq!(insert(1), "Insert 1 Row");
    assert_eq!(insert(3), "Insert 3 Rows");
    assert_eq!(insert(0), "Insert 0 Rows");
    // Pas de séparateur de milliers : « 1000 », comme le `Display`.
    assert_eq!(insert(1000), "Insert 1000 Rows");
    let delete = |count| history_text_in(&english, &HistoryLabel::DeleteRows { count });
    assert_eq!(delete(1), "Delete 1 Row");
    assert_eq!(delete(5), "Delete 5 Rows");
}

/// Une colonne se dit par sa lettre de grille ; un nom choisi par l'utilisateur
/// est entre guillemets, sans marque d'isolation bidirectionnelle (les polices
/// de l'interface les dessineraient comme des carrés).
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
        "le groupe d'import"
    );
    for label in all_labels() {
        let phrase = text(label);
        assert!(
            !phrase.contains(['\u{2068}', '\u{2069}']),
            "marque d'isolation dans {phrase:?}"
        );
    }
}

/// Toute phrase passe par le catalogue : en pseudo-langue, elle est encadrée de
/// `⟦ ⟧`, et chaque argument (nombre, lettre, nom) y figure tel quel.
#[test]
fn the_pseudo_language_covers_every_history_sentence() {
    let pseudo_loader = build_loader(Language::PseudoEnglish);
    for label in all_labels() {
        let phrase = history_text_in(&pseudo_loader, &label);
        assert!(
            phrase.starts_with(pseudo::OPEN) && phrase.ends_with(pseudo::CLOSE),
            "{label:?} : {phrase:?} n'est pas passé par le catalogue"
        );
    }
    let phrase = history_text_in(
        &pseudo_loader,
        &HistoryLabel::Rename {
            from: name("Book1"),
            to: "Mesures".to_string(),
        },
    );
    assert!(
        phrase.contains("Book1") && phrase.contains("Mesures"),
        "{phrase:?}"
    );
}

/// Le français a son catalogue (#49) : le même `HistoryLabel`, en français.
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

/// Chaque clé `history-*` du catalogue est employée : une clé orpheline (une
/// forme retirée du Rust, laissée dans le `.ftl`) fait échouer le test.
#[test]
fn every_history_key_of_the_catalogue_is_used() {
    let catalogue = include_str!("../i18n/en/stokesforge_app.ftl");
    let mut sources = String::new();
    for file in ["history_text.rs", "state.rs", "app.rs"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join(file);
        sources.push_str(&std::fs::read_to_string(path).expect("source lisible"));
    }
    let keys: Vec<&str> = catalogue
        .lines()
        .filter_map(|line| line.split(" =").next())
        .filter(|key| key.starts_with("history-") && !key.contains(' '))
        .collect();
    assert!(
        keys.len() > 30,
        "le catalogue d'historique est lu : {keys:?}"
    );
    for key in keys {
        assert!(
            sources.contains(&format!("\"{key}\"")),
            "la clé {key} n'est employée nulle part"
        );
    }
}
