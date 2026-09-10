//! Tests d'intégration de `plotyx-persistence`, via la seule API publique
//! (`save`, `load`).

use std::path::Path;

use plotyx_core::{Column, ColumnMeta, ColumnRole, ColumnType, Project, ValueBlock, WindowMeta};
use plotyx_persistence::{load, save, PersistenceError};

/// Construit le projet de fixture décrit par le critère 1 : dossiers
/// imbriqués, classeurs, feuilles, colonnes de tous les types avec cas
/// limites, fenêtres non triviales.
fn fixture_project() -> Project {
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();

    let manips = project.add_folder(root, "Manips").expect("dossier créé");
    project
        .add_folder(manips, "Sous-dossier")
        .expect("sous-dossier créé");

    let book_in_manips = project
        .add_empty_workbook(manips, Some("BookManips"))
        .expect("classeur créé");
    let book_at_root = project
        .add_empty_workbook(root, Some("BookRacine"))
        .expect("classeur créé");

    project
        .set_window_meta(
            book_in_manips,
            WindowMeta {
                label: "Étiquette du classeur".to_string(),
                comment: "Commentaire du classeur".to_string(),
            },
        )
        .expect("métadonnées de fenêtre valides");

    for book in [book_in_manips, book_at_root] {
        let sheet1 = project
            .add_worksheet(book, Some("Sheet1"))
            .expect("feuille créée");
        project
            .add_worksheet(book, Some("Sheet2"))
            .expect("feuille créée");

        let numeric = Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(vec![1.0, f64::NAN, 3.0, 4.0, f64::NAN]),
            ColumnMeta {
                role: ColumnRole::X,
                unit: "s".to_string(),
                long_name: "Temps".to_string(),
                comment: "colonne de temps".to_string(),
                display_format: "0.000".to_string(),
            },
        )
        .expect("colonne numérique valide");
        project
            .add_column(sheet1, None, Some("A"), numeric)
            .expect("colonne ajoutée");

        let datetime = Column::from_block(
            ColumnType::DateTime,
            ValueBlock::Float(vec![0.0, 86_400.0]),
            ColumnMeta::default(),
        )
        .expect("colonne date-heure valide");
        project
            .add_column(sheet1, None, Some("B"), datetime)
            .expect("colonne ajoutée");

        let text = Column::from_block(
            ColumnType::Text,
            ValueBlock::Text(vec![
                Some(String::new()),
                None,
                Some("é ü 日本".to_string()),
            ]),
            ColumnMeta::default(),
        )
        .expect("colonne texte valide");
        project
            .add_column(sheet1, None, Some("C"), text)
            .expect("colonne ajoutée");

        let empty = Column::new(ColumnType::Numeric);
        project
            .add_column(sheet1, None, Some("D"), empty)
            .expect("colonne vide ajoutée");

        // `Sheet2` reste une feuille vide (aucune colonne).
        let _ = sheet1;
    }

    let graph = project
        .add_graph(root, Some("Graph1"))
        .expect("graphe créé");
    project
        .set_window_meta(
            graph,
            WindowMeta {
                label: "Mon graphe".to_string(),
                comment: String::new(),
            },
        )
        .expect("métadonnées de fenêtre valides");

    project
        .add_matrix(root, Some("MBook1"))
        .expect("matrice créée");

    project
        .add_note(root, Some("Note1"), "Texte de la note, avec accents éàü.")
        .expect("note créée");

    project
}

#[test]
fn full_round_trip_preserves_the_tree_bit_for_bit() {
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("demo.plyx");

    save(&project, &path).expect("sauvegarde réussie");
    let loaded = load(&path).expect("chargement réussi");

    assert!(
        *loaded.tree() == *project.tree(),
        "l'arbre rechargé doit être bit à bit identique à l'original"
    );
    // Un aller-retour disque préserve le compteur exactement : `next_id_hint`
    // persisté est déjà le prochain identifiant à attribuer, et rien dans
    // l'arbre relu ne le contredit.
    assert_eq!(loaded.next_id_hint(), project.next_id_hint());
}

#[test]
fn ids_are_preserved_and_the_counter_resumes_above_them() {
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("demo.plyx");
    save(&project, &path).expect("sauvegarde réussie");

    let highest_before = project
        .tree()
        .iter()
        .map(|(id, _)| id.raw())
        .max()
        .unwrap_or(0);

    let mut loaded = load(&path).expect("chargement réussi");
    let sheet = loaded
        .resolve_path("/Manips/[BookManips]Sheet1")
        .expect("feuille résolue");
    let new_id = loaded
        .add_column(sheet, None, None, Column::new(ColumnType::Numeric))
        .expect("colonne ajoutée après chargement");

    // Le nouvel id reste strictement au-dessus de tous les ids du fichier :
    // aucun id n'est jamais réutilisé, rechargement compris.
    assert!(new_id.raw() > highest_before);
    // Après chargement, ajouter une colonne rend l'identifiant
    // `project.next_id_hint()` lui-même — pas un de plus.
    assert_eq!(new_id.raw(), project.next_id_hint());
}

#[test]
fn repeated_save_load_cycles_do_not_drift() {
    // Pas de dérive cumulée : trois cycles `save`/`load` enchaînés laissent
    // `next_id_hint()` identique à l'original, et l'arbre égal bit à bit à
    // chaque tour.
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("demo.plyx");

    let mut current = project;
    for tour in 0..3 {
        save(&current, &path).expect("sauvegarde réussie");
        let loaded = load(&path).expect("chargement réussi");
        assert!(
            *loaded.tree() == *current.tree(),
            "arbre non préservé bit à bit au tour {tour}"
        );
        assert_eq!(
            loaded.next_id_hint(),
            current.next_id_hint(),
            "next_id_hint a dérivé au tour {tour}"
        );
        current = loaded;
    }
}

#[test]
fn the_modification_counter_survives_the_round_trip() {
    // Il doit traverser le disque : un compteur remis à zéro au chargement
    // finirait par repasser par une valeur qu'un lecteur d'avant la
    // sauvegarde croirait reconnaître, sur des données devenues tout autres.
    // `Column`'s `PartialEq` ignore le compteur, donc le test d'égalité bit à
    // bit de l'arbre ne couvre pas ce cas — d'où ce test dédié.
    let mut project = Project::new("Compteur").expect("projet valide");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("classeur créé");
    let sheet = project
        .add_worksheet(book, Some("Sheet1"))
        .expect("feuille créée");
    let column_id = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .expect("colonne ajoutée");

    for value in [10.0, 20.0, 30.0] {
        project
            .set_values(column_id, 0, ValueBlock::Float(vec![value]))
            .expect("écriture valide");
    }
    let before = project
        .tree()
        .node(column_id)
        .and_then(|node| node.as_column())
        .expect("colonne présente")
        .version();
    assert_eq!(before, 3, "trois écritures, trois incréments");

    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("compteur.plyx");
    save(&project, &path).expect("sauvegarde réussie");
    let loaded = load(&path).expect("chargement réussi");

    let after = loaded
        .tree()
        .node(column_id)
        .and_then(|node| node.as_column())
        .expect("colonne présente")
        .version();
    assert_eq!(after, before);
}

#[test]
fn empty_string_and_missing_text_survive_the_round_trip() {
    let mut project = Project::new("Texte").expect("projet valide");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("classeur créé");
    let sheet = project
        .add_worksheet(book, Some("Sheet1"))
        .expect("feuille créée");
    let column = Column::from_block(
        ColumnType::Text,
        ValueBlock::Text(vec![Some(String::new()), None, Some("x".to_string())]),
        ColumnMeta::default(),
    )
    .expect("colonne texte valide");
    let column_id = project
        .add_column(sheet, None, Some("A"), column)
        .expect("colonne ajoutée");

    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("texte.plyx");
    save(&project, &path).expect("sauvegarde réussie");
    let loaded = load(&path).expect("chargement réussi");

    let node = loaded.tree().node(column_id).expect("colonne présente");
    let loaded_column = node.as_column().expect("nœud colonne");
    assert_eq!(
        loaded_column.get(0),
        plotyx_core::Cell::Text(Some(String::new()))
    );
    assert_eq!(loaded_column.get(1), plotyx_core::Cell::Text(None));
    assert_eq!(
        loaded_column.get(2),
        plotyx_core::Cell::Text(Some("x".to_string()))
    );
}

#[test]
fn two_saves_of_the_same_project_are_byte_identical() {
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path1 = dir.path().join("a.plyx");
    let path2 = dir.path().join("b.plyx");

    save(&project, &path1).expect("première sauvegarde");
    save(&project, &path2).expect("seconde sauvegarde");

    let bytes1 = std::fs::read(&path1).expect("lecture du premier fichier");
    let bytes2 = std::fs::read(&path2).expect("lecture du second fichier");
    assert_eq!(bytes1, bytes2);
}

#[test]
fn a_file_from_a_newer_version_is_rejected_without_reading_its_body() {
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("future.plyx");
    save(&project, &path).expect("sauvegarde réussie");

    let mut bytes = std::fs::read(&path).expect("lecture du fichier sauvegardé");
    let mut version = [0u8; 4];
    version.copy_from_slice(&bytes[4..8]);
    let bumped = u32::from_le_bytes(version) + 1;
    bytes[4..8].copy_from_slice(&bumped.to_le_bytes());

    // Corps rendu totalement illisible : la preuve qu'il n'est pas lu passe
    // par le fait que l'erreur reste `UnsupportedVersion`, pas `Corrupt`.
    bytes.truncate(8);
    bytes.extend_from_slice(b"ceci n'est pas du msgpack valide");

    std::fs::write(&path, &bytes).expect("écriture du fichier modifié");

    let err = load(&path).expect_err("version future doit être refusée");
    match err {
        PersistenceError::UnsupportedVersion {
            found, supported, ..
        } => {
            assert_eq!(found, bumped);
            assert_eq!(supported, bumped - 1);
        }
        other => panic!("attendu UnsupportedVersion, obtenu {other:?}"),
    }
}

#[test]
fn a_file_without_the_plotyx_signature_is_rejected() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");

    let zip_like = dir.path().join("zip_like.plyx");
    std::fs::write(&zip_like, b"PK\x03\x04reste").expect("écriture du fichier de test");
    assert!(matches!(
        load(&zip_like),
        Err(PersistenceError::NotAPlotyxFile { .. })
    ));

    let empty = dir.path().join("empty.plyx");
    std::fs::write(&empty, b"").expect("écriture du fichier de test");
    assert!(matches!(
        load(&empty),
        Err(PersistenceError::NotAPlotyxFile { .. })
    ));
}

#[test]
fn a_truncated_or_garbage_body_is_reported_as_corrupt() {
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("corrupt.plyx");
    save(&project, &path).expect("sauvegarde réussie");

    let mut bytes = std::fs::read(&path).expect("lecture du fichier sauvegardé");
    // Version inchangée (toujours supportée), mais le corps est tronqué en
    // plein milieu d'un enregistrement MessagePack.
    bytes.truncate(bytes.len() - 5);
    std::fs::write(&path, &bytes).expect("écriture du fichier tronqué");

    assert!(matches!(load(&path), Err(PersistenceError::Corrupt { .. })));
}

#[test]
fn saving_into_a_missing_directory_fails_cleanly_and_creates_nothing() {
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("dossier_absent").join("demo.plyx");

    let err = save(&project, &path).expect_err("répertoire parent absent doit échouer");
    assert!(matches!(err, PersistenceError::Io { .. }));
    assert!(!path.exists());
    assert!(!Path::new(&dir.path().join("dossier_absent")).exists());
}

#[test]
fn saving_leaves_no_temporary_file_behind() {
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("demo.plyx");

    save(&project, &path).expect("sauvegarde réussie");

    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .expect("lecture du répertoire")
        .collect::<Result<_, _>>()
        .expect("entrées valides");
    assert_eq!(
        entries.len(),
        1,
        "un seul fichier attendu dans le répertoire après sauvegarde : {entries:?}"
    );
}

#[test]
fn saving_over_an_existing_valid_file_replaces_it_atomically() {
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("demo.plyx");

    save(&project, &path).expect("première sauvegarde");
    let first_bytes = std::fs::read(&path).expect("lecture du premier fichier");

    save(&project, &path).expect("seconde sauvegarde par-dessus le premier fichier");
    let second_bytes = std::fs::read(&path).expect("lecture du second fichier");

    assert_eq!(first_bytes, second_bytes);
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .expect("lecture du répertoire")
        .collect::<Result<_, _>>()
        .expect("entrées valides");
    assert_eq!(entries.len(), 1, "aucun résidu temporaire : {entries:?}");
}

#[test]
fn loading_a_missing_path_fails_with_io() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("absent.plyx");
    assert!(matches!(load(&path), Err(PersistenceError::Io { .. })));
}

#[test]
fn a_large_float_column_is_stored_as_raw_bytes_not_as_a_msgpack_array() {
    let mut project = Project::new("Volume").expect("projet valide");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("classeur créé");
    let sheet = project
        .add_worksheet(book, Some("Sheet1"))
        .expect("feuille créée");
    let values: Vec<f64> = (0..100_000).map(|i| i as f64 * 0.5).collect();
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&values))
        .expect("colonne ajoutée");

    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("volume.plyx");
    save(&project, &path).expect("sauvegarde réussie");

    let size = std::fs::metadata(&path)
        .expect("métadonnées du fichier")
        .len();
    assert!(
        size < 100_000 * 8 + 4_096,
        "fichier de {size} octets : l'encodage ne doit pas dépasser les données brutes de plus de 4 Ko"
    );
}
