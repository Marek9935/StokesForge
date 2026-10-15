//! Tests d'intégration de `stokesforge-persistence`, via la seule API publique
//! (`save`, `load`).

use std::path::Path;

use stokesforge_core::{
    Column, ColumnMeta, ColumnRole, ColumnType, MixedCell, NodeKind, Project, ValueBlock,
    WindowMeta,
};
use stokesforge_persistence::{load, save, PersistenceError};
use stokesforge_plot::{
    AxisScale, AxisSpec, LegendPosition, LegendSpec, LineSpec, LineStyle, PanelSpec, PlotSpec,
    PlotType, SeriesAxis, SeriesSpec, SymbolShape, SymbolSpec,
};

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
                ..Default::default()
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
                x_prime: Some(12.5),
                display_format: "0.000".to_string(),
                formula: None,
                file_metadata: Vec::new(),
                import_source: None,
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
            ColumnType::Mixed,
            ValueBlock::Mixed(vec![
                MixedCell::Text(String::new()),
                MixedCell::Empty,
                MixedCell::Text("é ü 日本".to_string()),
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

        // Colonne à types mixtes : les trois variantes de cellule, plus les
        // trois « manquantes » qui doivent rester distinctes au disque
        // (`Empty`, `Number(NaN)`, `Text("")`), plus un `-0.0` — dont le
        // motif binaire ne survit qu'à un encodage brut.
        let mixed = Column::from_block(
            ColumnType::Mixed,
            ValueBlock::Mixed(vec![
                MixedCell::Number(1.5),
                MixedCell::Text("n/a".to_string()),
                MixedCell::Empty,
                MixedCell::Number(f64::NAN),
                MixedCell::Text(String::new()),
                MixedCell::Number(-0.0),
                MixedCell::Text("é ü 日本".to_string()),
            ]),
            ColumnMeta {
                role: ColumnRole::Y,
                unit: "V".to_string(),
                long_name: "Tension mesurée".to_string(),
                comment: "avec des n/a".to_string(),
                x_prime: None,
                display_format: String::new(),
                formula: None,
                file_metadata: Vec::new(),
                import_source: None,
            },
        )
        .expect("colonne mixte valide");
        project
            .add_column(sheet1, None, Some("E"), mixed)
            .expect("colonne mixte ajoutée");

        // `Sheet2` reste une feuille vide (aucune colonne).
        let _ = sheet1;
    }

    let graph = project
        .add_graph(root, Some("Graph1"), Default::default())
        .expect("graphe créé");
    project
        .set_window_meta(
            graph,
            WindowMeta {
                label: "Mon graphe".to_string(),
                comment: String::new(),
                ..Default::default()
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
        ColumnType::Mixed,
        ValueBlock::Mixed(vec![
            MixedCell::Text(String::new()),
            MixedCell::Empty,
            MixedCell::Text("x".to_string()),
        ]),
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
        stokesforge_core::Cell::Text(Some(String::new()))
    );
    assert_eq!(loaded_column.get(1), stokesforge_core::Cell::Text(None));
    assert_eq!(
        loaded_column.get(2),
        stokesforge_core::Cell::Text(Some("x".to_string()))
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
fn a_file_without_the_stokesforge_signature_is_rejected() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");

    let zip_like = dir.path().join("zip_like.plyx");
    std::fs::write(&zip_like, b"PK\x03\x04reste").expect("écriture du fichier de test");
    assert!(matches!(
        load(&zip_like),
        Err(PersistenceError::NotAStokesForgeFile { .. })
    ));

    let empty = dir.path().join("empty.plyx");
    std::fs::write(&empty, b"").expect("écriture du fichier de test");
    assert!(matches!(
        load(&empty),
        Err(PersistenceError::NotAStokesForgeFile { .. })
    ));
}

/// Le texte brut des erreurs de lecture est de l'anglais lisible : c'est ce que
/// voient le CLI, les scripts et les journaux (l'application, elle, le met en
/// phrase par son catalogue, #47).
#[test]
fn reading_errors_display_in_plain_english() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");

    let not_stokesforge = dir.path().join("not_stokesforge.plyx");
    std::fs::write(&not_stokesforge, b"PK\x03\x04reste").expect("écriture du fichier de test");
    let message = load(&not_stokesforge)
        .expect_err("pas un fichier StokesForge")
        .to_string();
    assert_eq!(
        message,
        format!("{} is not a StokesForge file", not_stokesforge.display())
    );

    let truncated = dir.path().join("truncated.plyx");
    std::fs::write(&truncated, b"STKF\x01").expect("écriture du fichier de test");
    let message = load(&truncated).expect_err("en-tête tronqué").to_string();
    assert_eq!(
        message,
        format!(
            "corrupt file {}: header truncated before the version number",
            truncated.display()
        )
    );

    let missing = dir.path().join("absent.plyx");
    let message = load(&missing).expect_err("fichier absent").to_string();
    assert!(
        message.starts_with(&format!("I/O error on {}: ", missing.display())),
        "{message}"
    );
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
    // B8 (`guide-de-correction-robustesse.md` §F2) — deux sauvegardes
    // successives du même `project`, vers le même chemin, ne produisent
    // plus des fichiers identiques octet à octet : chacune fait avancer
    // `FileBody::save_counter`, qui doit changer pour que `save` puisse
    // détecter un écrivain concurrent. Ce test vérifiait auparavant la
    // stabilité bit à bit ; il vérifie maintenant ce qui reste vrai —
    // l'écriture atomique, sans résidu — et ce que B8 ajoute : le compteur
    // avance, et deux sauvegardes successives DEPUIS CE PROCESSUS restent
    // acceptées (`tracked_counter` suit la dernière valeur vue).
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("demo.plyx");

    save(&project, &path).expect("première sauvegarde");
    let first_bytes = std::fs::read(&path).expect("lecture du premier fichier");

    save(&project, &path).expect("seconde sauvegarde par-dessus le premier fichier, acceptée");
    let second_bytes = std::fs::read(&path).expect("lecture du second fichier");

    assert_ne!(
        first_bytes, second_bytes,
        "le compteur de sauvegarde doit avoir avancé entre les deux écritures"
    );
    // Seule la fin du fichier (le compteur MessagePack) doit différer ; tout
    // le reste — nœuds, next_id_hint — est inchangé.
    assert_eq!(first_bytes.len(), second_bytes.len());
    let first_prefix_len = first_bytes.len() - 1;
    assert_eq!(
        &first_bytes[..first_prefix_len],
        &second_bytes[..first_prefix_len],
        "seul le compteur de sauvegarde devait différer"
    );

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

/// Une colonne mixte majoritairement numérique coûte, sur disque, à peu près
/// ce que coûterait la même colonne numérique — un octet d'étiquette par
/// ligne en plus, et rien de plus.
///
/// C'est ce que les trois tableaux parallèles de `ValuesRecord::Mixed`
/// achètent : `to_vec_named` sérialise les énumérations par le **nom** de
/// leur variante, donc un `Vec<MixedCellRecord>` aurait écrit la chaîne
/// `"Number"` devant chacune des 100 000 valeurs — près d'un méga-octet de
/// noms de variante. Le test est le même que pour les colonnes numériques
/// brutes, avec la marge d'un octet par ligne.
#[test]
fn a_large_mixed_column_costs_one_tag_byte_per_row_not_a_variant_name() {
    let mut project = Project::new("Volume").expect("projet valide");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("classeur créé");
    let sheet = project
        .add_worksheet(book, Some("Sheet1"))
        .expect("feuille créée");
    const ROWS: usize = 100_000;
    // Une colonne de mesures dont une ligne sur mille est un « n/a » — le
    // cas réel qui justifie le type.
    let cells: Vec<MixedCell> = (0..ROWS)
        .map(|i| {
            if i % 1000 == 999 {
                MixedCell::Text("n/a".to_string())
            } else {
                MixedCell::Number(i as f64 * 0.5)
            }
        })
        .collect();
    project
        .add_column(sheet, None, Some("A"), Column::from_mixed(&cells))
        .expect("colonne ajoutée");

    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("volume.plyx");
    save(&project, &path).expect("sauvegarde réussie");

    let size = std::fs::metadata(&path)
        .expect("métadonnées du fichier")
        .len();
    // 8 octets par nombre + 1 octet d'étiquette par ligne + les textes.
    let plafond = (ROWS * 8 + ROWS + 4_096) as u64;
    assert!(
        size < plafond,
        "fichier de {size} octets, au-delà du plafond de {plafond} : l'encodage \
         mixte a cessé d'être compact"
    );
}

/// La lecture refuse une colonne mixte dont les trois tableaux ne
/// s'accordent pas, plutôt que de décaler silencieusement toutes les lignes
/// suivantes. Même politique que `BadFloatLength` pour un buffer numérique
/// tronqué : un fichier écrit par ce crate ne produit jamais ce cas, et il
/// n'y a pas de réparation sensée à tenter.
#[test]
fn a_mixed_column_with_inconsistent_tags_is_reported_as_corrupt() {
    use serde::Serialize;

    // Schéma miroir, comme `b3`/`b19` : le format est auto-descriptif, deux
    // déclarations d'accord sur les noms produisent les mêmes octets.
    #[derive(Serialize)]
    struct FileBody {
        root: u64,
        next_id_hint: u64,
        nodes: Vec<NodeRecord>,
    }
    #[derive(Serialize)]
    struct NodeRecord {
        id: u64,
        name: String,
        parent: Option<u64>,
        kind: KindRecord,
    }
    // `Box` sur la grosse variante : seuls les octets produits comptent
    // ici (MessagePack ne voit pas l'indirection), et clippy refuse un enum
    // dont les variantes ont des tailles très différentes.
    #[derive(Serialize)]
    enum KindRecord {
        Folder { children: Vec<u64> },
        Column(Box<ColumnRecord>),
    }
    #[derive(Serialize)]
    struct ColumnRecord {
        column_type: &'static str,
        role: &'static str,
        unit: String,
        long_name: String,
        comment: String,
        display_format: String,
        values: ValuesRecord,
        version: u64,
        formula: Option<String>,
    }
    #[derive(Serialize)]
    enum ValuesRecord {
        Mixed {
            #[serde(with = "serde_bytes")]
            tags: Vec<u8>,
            #[serde(with = "serde_bytes")]
            numbers: Vec<u8>,
            texts: Vec<String>,
        },
    }

    let colonne = |values: ValuesRecord| {
        KindRecord::Column(Box::new(ColumnRecord {
            column_type: "Mixed",
            role: "Y",
            unit: String::new(),
            long_name: String::new(),
            comment: String::new(),
            display_format: String::new(),
            values,
            version: 0,
            formula: None,
        }))
    };

    // Deux étiquettes « nombre » mais un seul `f64` dans le tableau.
    let desaccord = colonne(ValuesRecord::Mixed {
        tags: vec![1, 1],
        numbers: 1.0f64.to_le_bytes().to_vec(),
        texts: Vec::new(),
    });
    // Une étiquette qu'aucune version n'attribue.
    let etiquette_inconnue = colonne(ValuesRecord::Mixed {
        tags: vec![7],
        numbers: Vec::new(),
        texts: Vec::new(),
    });

    for (cas, kind) in [
        ("tableaux en désaccord", desaccord),
        ("étiquette inconnue", etiquette_inconnue),
    ] {
        let body = FileBody {
            root: 0,
            next_id_hint: 2,
            nodes: vec![
                NodeRecord {
                    id: 0,
                    name: "Projet".to_string(),
                    parent: None,
                    kind: KindRecord::Folder { children: vec![1] },
                },
                NodeRecord {
                    id: 1,
                    name: "A".to_string(),
                    parent: Some(0),
                    kind,
                },
            ],
        };
        let mut bytes = b"STKF".to_vec();
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&rmp_serde::to_vec_named(&body).expect("encodage"));

        let dir = tempfile::tempdir().expect("répertoire temporaire");
        let path = dir.path().join("hostile.plyx");
        std::fs::write(&path, &bytes).expect("écriture");

        match load(&path) {
            Err(PersistenceError::Corrupt { .. }) => {}
            other => panic!("{cas} : attendu Corrupt, obtenu {other:?}"),
        }
    }
}

/// Un `PlotSpec` non trivial : deux panneaux, plusieurs séries, des styles
/// qui ne sont **pas** ceux par défaut — pour que l'égalité de la tâche 3.5
/// ne puisse pas passer par accident sur des champs restés à leur valeur par
/// défaut des deux côtés du disque.
fn non_trivial_spec(
    x: stokesforge_core::NodeId,
    y1: stokesforge_core::NodeId,
    y2: stokesforge_core::NodeId,
) -> PlotSpec {
    let scatter = SeriesSpec {
        name: "Brute".to_string(),
        plot_type: PlotType::Scatter,
        line: LineSpec {
            color: "#112233".to_string(),
            style: LineStyle::Dash,
            width: 3.5,
        },
        symbol: SymbolSpec {
            shape: SymbolShape::Diamond,
            size: 9.0,
            color: "#445566".to_string(),
            edge_color: "#000000".to_string(),
        },
        axis: SeriesAxis::Right,
        visible: false,
        show_in_legend: false,
        bar_width: Some(0.42),
        // Série « ligne » (#166) : la ligne 4 de la feuille de `y1`.
        row: Some(4),
        // Groupe de courbes (#177).
        group: Some(3),
        ..SeriesSpec::new_xy(x, y1)
    };
    let histogram = SeriesSpec {
        name: String::new(),
        plot_type: PlotType::Histogram,
        histogram_bins: stokesforge_plot::BinCount::new(17).expect("17 est dans les bornes"),
        ..SeriesSpec::new(y2)
    };

    let panel_a = PanelSpec {
        title: "Signal".to_string(),
        series: vec![scatter],
        bottom: AxisSpec {
            label: "Temps".to_string(),
            unit: "s".to_string(),
            scale: AxisScale::Log,
            auto_range: false,
            range_min: Some(-1.0),
            range_max: Some(10.0),
            grid: true,
            visible: true,
            inverted: true,
            title_color: "#aa3311".to_string(),
            // Issue #53 : textes déplacés, une ancre par sorte de texte.
            title_visible: true,
            title_anchor: Some(stokesforge_plot::TextAnchor::new(3.5, -0.75)),
            // Issues #77 et #78 : police et corps, un jeu par sorte de texte.
            title_font: stokesforge_plot::TextFont {
                family: "Fira Sans".to_string(),
                size: Some(18.0),
                width: Some(0.4),
                align: stokesforge_plot::TextAlign::Right,
            },
            tick_color: "#fefefe".to_string(),
            // #176 : axe miroir, sans étiquettes de graduation.
            mirror: true,
            tick_labels: false,
        },
        title_visible: true,
        title_anchor: Some(stokesforge_plot::TextAnchor {
            axis: SeriesAxis::Right,
            ..stokesforge_plot::TextAnchor::new(0.5, 4.25)
        }),
        legend: LegendSpec {
            visible: true,
            position: LegendPosition::BottomLeft,
            anchor: Some(stokesforge_plot::TextAnchor::new(-2.0, 8.0)),
            font: stokesforge_plot::TextFont {
                family: "Libertinus Sans".to_string(),
                size: Some(11.0),
                ..Default::default()
            },
            color: "#118899".to_string(),
        },
        title_color: "#aa8811".to_string(),
        // Issue #35 : textes libres, tous champs non défaut.
        annotations: vec![
            stokesforge_plot::AnnotationSpec::new("Pic $lambda_\"max\"$", 1.5, 2.5),
            stokesforge_plot::AnnotationSpec {
                axis: SeriesAxis::Right,
                color: "#11223344".to_string(),
                size: 22.0,
                rotation: -30.0,
                font: "Fira Sans".to_string(),
                visible: false,
                width: Some(0.3),
                align: stokesforge_plot::TextAlign::Right,
                ..stokesforge_plot::AnnotationSpec::new("*seuil*", -0.25, 1e-3)
            },
        ],
        // Issue #76 : objets graphiques, un de chaque sorte, dans l'ordre.
        shapes: vec![
            stokesforge_plot::ShapeSpec {
                visible: false,
                axis: SeriesAxis::Right,
                line_color: "#aa221188".to_string(),
                line_width: 3.0,
                fill_color: "#33669955".to_string(),
                ..stokesforge_plot::ShapeSpec::new(
                    stokesforge_plot::ShapeKind::Rectangle,
                    0.5,
                    1.0,
                    2.5,
                    3.0,
                )
            },
            stokesforge_plot::ShapeSpec::new(
                stokesforge_plot::ShapeKind::Ellipse,
                -1.0,
                -2.0,
                4.0,
                6.0,
            ),
            stokesforge_plot::ShapeSpec::new(stokesforge_plot::ShapeKind::Line, 0.0, 0.0, 1.0, 1.0),
            stokesforge_plot::ShapeSpec::new(
                stokesforge_plot::ShapeKind::Arrow,
                2.0,
                2.0,
                1.0,
                0.5,
            ),
        ],
        // Nom du groupe 3 (#197).
        group_names: [(3, "Séries d'été".to_string())].into(),
        // Nuancier retenu par le groupe 3 (#213).
        group_gradients: [(3, vec!["#0000ff".to_string(), "#ff0000".to_string()])].into(),
        ..PanelSpec::at(0, 0)
    };
    let panel_b = PanelSpec {
        title: "Répartition".to_string(),
        series: vec![histogram],
        legend: LegendSpec {
            visible: false,
            position: LegendPosition::TopLeft,
            anchor: None,
            font: stokesforge_plot::TextFont::default(),
            // Issue #90 : couleur de la légende et du titre du panneau.
            color: "#228833".to_string(),
        },
        title_font: stokesforge_plot::TextFont {
            family: "Noto Sans".to_string(),
            size: None,
            ..Default::default()
        },
        title_color: "#8833aa".to_string(),
        ..PanelSpec::at(1, 0)
    };

    PlotSpec {
        background: "#0a0a0a".to_string(),
        title_color: "#ffcc00".to_string(),
        title_visible: true,
        title_anchor: Some(stokesforge_plot::TextAnchor::new(1.0, 9.5)),
        title_font: stokesforge_plot::TextFont {
            family: "Libertinus Sans".to_string(),
            size: Some(30.0),
            ..Default::default()
        },
        ..PlotSpec::empty("Mesure du 11/09").with_panels(vec![panel_a, panel_b])
    }
}

/// Tâche 3.5 : un graphe au `PlotSpec` non trivial, sauvé puis rechargé,
/// rend un spec **égal** — l'aller-retour disque ne perd et n'invente rien,
/// y compris sur des références de colonnes dont l'ordre traverse deux
/// panneaux.
#[test]
fn a_non_trivial_plot_spec_round_trips_intact() {
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("classeur");
    let sheet = project
        .add_worksheet(book, Some("Sheet1"))
        .expect("feuille");
    let x = project
        .add_column(sheet, None, Some("X"), Column::from_f64(&[0.0, 1.0, 2.0]))
        .expect("colonne X");
    let y1 = project
        .add_column(sheet, None, Some("Y1"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("colonne Y1");
    let y2 = project
        .add_column(sheet, None, Some("Y2"), Column::from_f64(&[4.0, 5.0, 6.0]))
        .expect("colonne Y2");

    let spec = non_trivial_spec(x, y1, y2);
    let graph = project
        .add_graph(root, Some("Fig1"), spec.clone())
        .expect("graphe créé");

    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("spec.plyx");
    save(&project, &path).expect("sauvegarde réussie");
    let loaded = load(&path).expect("chargement réussi");

    let node = loaded.tree().node(graph).expect("graphe présent");
    match node.kind() {
        NodeKind::Graph { spec: reloaded, .. } => assert_eq!(*reloaded, spec),
        other => panic!("attendu Graph, obtenu {other:?}"),
    }
}

/// `ColumnMeta::formula` (colonnes calculées, v1) survit à `save`/`load`,
/// `None` comme `Some(_)`.
#[test]
fn column_formula_survives_save_and_load() {
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("classeur");
    let sheet = project
        .add_worksheet(book, Some("Sheet1"))
        .expect("feuille");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .expect("colonne A");
    let b = project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[10.0, 20.0]))
        .expect("colonne B");
    project
        .set_column_meta(
            b,
            ColumnMeta {
                formula: Some("A*2".to_string()),
                ..Default::default()
            },
        )
        .expect("formule posée");

    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("formule.plyx");
    save(&project, &path).expect("sauvegarde réussie");
    let loaded = load(&path).expect("chargement réussi");

    let node_a = loaded.tree().node(a).expect("colonne A présente");
    assert_eq!(node_a.as_column().expect("colonne").meta().formula, None);

    let node_b = loaded.tree().node(b).expect("colonne B présente");
    assert_eq!(
        node_b.as_column().expect("colonne").meta().formula,
        Some("A*2".to_string())
    );
}

/// Les couleurs enregistrées dans le projet, rangées par usage, survivent à
/// `save`/`load` — et l'arbre relu reste identique à l'original.
#[test]
fn saved_colors_survive_save_and_load() {
    use stokesforge_plot::{ColorUsage, SavedColors};
    let mut project = fixture_project();
    project.set_saved_colors(
        SavedColors::default()
            .with_added(ColorUsage::Curves, "#ff0000")
            .with_added(ColorUsage::Text, "#20202080")
            .with_added(ColorUsage::Background, "#ffffff00")
            // Nuancier enregistré (#178).
            .with_gradient_added("Chaud", &["#000000".to_string(), "#ff0000".to_string()]),
    );
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("couleurs.plyx");
    save(&project, &path).expect("sauvegarde réussie");
    let loaded = load(&path).expect("chargement réussi");
    assert_eq!(loaded.tree().saved_colors(), project.tree().saved_colors());
    assert!(*loaded.tree() == *project.tree());
}

/// `ColumnMeta::file_metadata` (métadonnées de fichier importé, issue #5)
/// survit à `save`/`load`, dans l'ordre, sections et lignes sans clé comprises.
#[test]
fn column_file_metadata_survives_save_and_load() {
    use stokesforge_core::MetaLine;
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("classeur");
    let sheet = project
        .add_worksheet(book, Some("Sheet1"))
        .expect("feuille");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0]))
        .expect("colonne A");
    let lines = vec![
        MetaLine {
            key: "[Measurement Information]".into(),
            value: String::new(),
        },
        MetaLine {
            key: "XUNITS".into(),
            value: "NANOMETERS".into(),
        },
        MetaLine {
            key: String::new(),
            value: "##### Extended Information".into(),
        },
    ];
    project
        .set_column_meta(
            a,
            ColumnMeta {
                file_metadata: lines.clone(),
                ..Default::default()
            },
        )
        .expect("métadonnées posées");

    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("meta.plyx");
    save(&project, &path).expect("sauvegarde réussie");
    let loaded = load(&path).expect("chargement réussi");
    let column = loaded.tree().node(a).unwrap().as_column().unwrap();
    assert_eq!(column.meta().file_metadata, lines);
}

/// `ColumnMeta::import_source` (chemin, recette JSON, index de colonne) survit à
/// `save`/`load` : c'est ce qui permet d'actualiser une colonne après
/// réouverture du projet (issue #7).
#[test]
fn column_import_source_survives_save_and_load() {
    use stokesforge_core::ImportSource;
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("classeur");
    let sheet = project
        .add_worksheet(book, Some("Sheet1"))
        .expect("feuille");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0]))
        .expect("colonne A");
    let source = ImportSource {
        path: "/data/mesure.csv".into(),
        recipe_json: "{\"name\":\"x\"}".into(),
        column: 3,
        block: 2,
    };
    project
        .set_column_meta(
            a,
            ColumnMeta {
                import_source: Some(source.clone()),
                ..Default::default()
            },
        )
        .expect("source posée");
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("source.plyx");
    save(&project, &path).expect("sauvegarde réussie");
    let loaded = load(&path).expect("chargement réussi");
    let column = loaded.tree().node(a).unwrap().as_column().unwrap();
    assert_eq!(column.meta().import_source, Some(source));
}
