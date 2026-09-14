//! Tests d'acceptation du modèle de projet, écrits d'après le cahier de la
//! tranche « nœuds, arbre persistant, Project ». Ils ne passent que par l'API
//! publique du crate.

use std::sync::Arc;

use plotyx_core::{
    Cell, Column, ColumnMeta, ColumnRole, ColumnType, ColumnValues, CoreError, Node, NodeId,
    NodeKind, Project, Tree, ValueBlock, WindowMeta,
};

// -- utilitaires --

fn folder_kind() -> NodeKind {
    NodeKind::Folder {
        children: Vec::new(),
    }
}

fn workbook_kind() -> NodeKind {
    NodeKind::Workbook {
        sheets: Vec::new(),
        meta: WindowMeta::default(),
    }
}

fn column_kind() -> NodeKind {
    NodeKind::Column(Column::new(ColumnType::Numeric))
}

fn name_of(project: &Project, id: NodeId) -> String {
    project
        .tree()
        .get(id)
        .expect("nœud présent")
        .name()
        .to_string()
}

fn child_names(project: &Project, parent: NodeId) -> Vec<String> {
    project
        .tree()
        .children(parent)
        .expect("parent présent")
        .iter()
        .map(|id| name_of(project, *id))
        .collect()
}

fn floats(project: &Project, column: NodeId) -> Vec<f64> {
    project
        .tree()
        .get(column)
        .expect("colonne présente")
        .as_column()
        .expect("nœud colonne")
        .float_values()
        .expect("colonne numérique")
        .as_ref()
        .clone()
}

/// Fixture du cahier : `Demo` → `Manips` → `[Book1]Sheet1` avec `Temps` et `Signal`.
struct Demo {
    project: Project,
    root: NodeId,
    folder: NodeId,
    book: NodeId,
    sheet: NodeId,
    temps: NodeId,
    signal: NodeId,
}

impl Demo {
    fn new() -> Self {
        let mut project = Project::new("Demo").expect("nom valide");
        let root = project.root();
        let folder = project.add_folder(root, "Manips").expect("dossier");
        let book = project.add_workbook(folder, None).expect("classeur");
        let sheet = project.tree().children(book).expect("classeur")[0];
        let temps = project
            .add_column(
                sheet,
                None,
                Some("Temps"),
                Column::from_f64(&[0.0, 1.0, 2.0]),
            )
            .expect("colonne Temps");
        let signal = project
            .add_column(sheet, None, Some("Signal"), Column::from_f64(&[3.0, 4.0]))
            .expect("colonne Signal");
        Self {
            project,
            root,
            folder,
            book,
            sheet,
            temps,
            signal,
        }
    }
}

/// Feuille vide dans un classeur neuf, pour les tests de données.
fn sheet_with(columns: &[(&str, &[f64])]) -> (Project, NodeId, Vec<NodeId>) {
    let mut project = Project::new("Data").expect("nom valide");
    let root = project.root();
    let book = project.add_workbook(root, None).expect("classeur");
    let sheet = project.tree().children(book).expect("classeur")[0];
    let ids = columns
        .iter()
        .map(|(name, values)| {
            project
                .add_column(sheet, None, Some(name), Column::from_f64(values))
                .expect("colonne")
        })
        .collect();
    (project, sheet, ids)
}

// -- 1. construction --

#[test]
fn critere_1_projet_neuf() {
    let project = Project::new("Vide").expect("nom valide");
    assert_eq!(project.name(), "Vide");
    assert_eq!(project.tree().len(), 1);
    let root = project.root();
    let root_node = project.tree().get(root).expect("racine");
    assert!(root_node.kind().is_folder());
    assert!(root_node.children().is_empty());
    assert_eq!(project.path_of(root).expect("chemin"), "/");
    assert_eq!(project.next_id_hint(), 2);
}

#[test]
fn critere_2_noms_par_defaut_des_fenetres() {
    let mut project = Project::new("Demo").expect("nom valide");
    let root = project.root();

    let book1 = project.add_workbook(root, None).expect("classeur");
    assert_eq!(name_of(&project, book1), "Book1");
    assert_eq!(child_names(&project, book1), vec!["Sheet1".to_string()]);

    let book2 = project.add_workbook(root, None).expect("classeur");
    assert_eq!(name_of(&project, book2), "Book2");

    let graph = project
        .add_graph(root, None, Default::default())
        .expect("graphe");
    assert_eq!(name_of(&project, graph), "Graph1");

    let matrix = project.add_matrix(root, None).expect("matrice");
    assert_eq!(name_of(&project, matrix), "MBook1");

    let note = project.add_note(root, None, "bonjour").expect("note");
    assert_eq!(name_of(&project, note), "Note1");
    match project.tree().get(note).expect("note").kind() {
        NodeKind::Note { text, .. } => assert_eq!(text, "bonjour"),
        other => panic!("attendu une note, obtenu {other:?}"),
    }
}

#[test]
fn critere_3_noms_de_colonnes_facon_tableur() {
    let (mut project, sheet, _) = sheet_with(&[]);
    for expected in ["A", "B", "C"] {
        let id = project
            .add_column(sheet, None, None, Column::new(ColumnType::Numeric))
            .expect("colonne");
        assert_eq!(name_of(&project, id), expected);
    }

    // 26 colonnes présentes → la suivante s'appelle AA.
    let (mut project, sheet, _) = sheet_with(&[]);
    for _ in 0..26 {
        project
            .add_column(sheet, None, None, Column::new(ColumnType::Numeric))
            .expect("colonne");
    }
    assert_eq!(
        child_names(&project, sheet).last().map(String::as_str),
        Some("Z")
    );
    let id = project
        .add_column(sheet, None, None, Column::new(ColumnType::Numeric))
        .expect("colonne");
    assert_eq!(name_of(&project, id), "AA");

    // Un nom déjà pris est sauté — jamais réattribué, même s'il occupe un
    // rang antérieur au nombre de colonnes déjà présentes. D1
    // (`guide-de-correction-robustesse.md` §F7) fait partir la recherche du
    // NOMBRE de colonnes déjà présentes plutôt que de « A » : retrouver le
    // plus petit trou libre coûterait de reconstruire la liste des noms pris
    // à chaque colonne ajoutée, précisément le coût en O(n³) que D1
    // supprime. Une seule colonne existe (« B ») quand la première
    // automatique est demandée, donc le premier candidat essayé est « B »
    // (déjà pris), puis « C » (libre) — pas « A », qui reste un nom valide
    // mais n'est plus cherché.
    let (mut project, sheet, _) = sheet_with(&[]);
    project
        .add_column(sheet, None, Some("B"), Column::new(ColumnType::Numeric))
        .expect("colonne B");
    let first = project
        .add_column(sheet, None, None, Column::new(ColumnType::Numeric))
        .expect("colonne");
    assert_eq!(name_of(&project, first), "C");
    let second = project
        .add_column(sheet, None, None, Column::new(ColumnType::Numeric))
        .expect("colonne");
    assert_eq!(name_of(&project, second), "D");
}

#[test]
fn critere_4_nom_de_feuille_par_defaut() {
    let mut project = Project::new("Demo").expect("nom valide");
    let root = project.root();
    let book = project.add_workbook(root, None).expect("classeur");
    let sheet2 = project.add_worksheet(book, None).expect("feuille");
    assert_eq!(name_of(&project, sheet2), "Sheet2");
    assert_eq!(
        child_names(&project, book),
        vec!["Sheet1".to_string(), "Sheet2".to_string()]
    );
}

// -- 5-10. unicité, structure --

#[test]
fn critere_5_regles_d_unicite_des_noms() {
    let mut project = Project::new("Demo").expect("nom valide");
    let root = project.root();
    let a = project.add_folder(root, "A").expect("dossier A");
    let b = project.add_folder(root, "B").expect("dossier B");

    // Deux fenêtres de même nom, même rangées ailleurs : refusé.
    project
        .add_empty_workbook(a, Some("Book1"))
        .expect("classeur");
    assert!(matches!(
        project.add_empty_workbook(b, Some("Book1")),
        Err(CoreError::DuplicateName(_))
    ));

    // Deux dossiers frères de même nom : refusé.
    project.add_folder(a, "Jour1").expect("dossier");
    assert!(matches!(
        project.add_folder(a, "Jour1"),
        Err(CoreError::DuplicateName(_))
    ));

    // Deux dossiers de même nom dans des parents différents : accepté.
    project
        .add_folder(b, "Jour1")
        .expect("dossier homonyme ailleurs");

    // Un dossier et un classeur frères de même nom : refusé (A8,
    // `guide-de-correction-robustesse.md` §F9 — voir
    // `critere_5_bis_lunicite_entre_freres_ne_depend_pas_de_lordre`).
    project
        .add_folder(root, "Mesures")
        .expect("dossier Mesures");
    assert!(
        matches!(
            project.add_empty_workbook(root, Some("Mesures")),
            Err(CoreError::DuplicateName(_))
        ),
        "un classeur ne doit plus pouvoir prendre le nom d'un dossier frère"
    );
}

#[test]
fn critere_5_bis_lunicite_entre_freres_ne_depend_pas_de_lordre() {
    // A8 (`guide-de-correction-robustesse.md` §F9) — corrigée : la version
    // Python (et le socle Rust jusqu'à ce correctif) laissait un dossier
    // unique parmi ses frères toutes natures confondues, mais une fenêtre
    // seulement parmi les fenêtres — asymétrie qui rendait un dossier
    // homonyme d'une fenêtre introuvable par son chemin canonique selon
    // l'ordre de création des deux
    // (`b11_path_of_se_re_resout_toujours_sur_le_meme_noeud`). Cette
    // asymétrie n'est plus « assumée » : refuser plus, jamais moins (§6 du
    // guide) — un dossier et une fenêtre frères ne partagent jamais un nom,
    // quel que soit l'ordre dans lequel chacun l'a pris.
    let mut project = Project::new("Demo").expect("nom valide");
    let root = project.root();
    project
        .add_empty_workbook(root, Some("X"))
        .expect("classeur X");
    assert!(matches!(
        project.add_folder(root, "X"),
        Err(CoreError::DuplicateName(_))
    ));

    // Et dans l'autre sens désormais aussi.
    let mut project = Project::new("Demo").expect("nom valide");
    let root = project.root();
    project.add_folder(root, "Y").expect("dossier Y");
    assert!(matches!(
        project.add_empty_workbook(root, Some("Y")),
        Err(CoreError::DuplicateName(_))
    ));
}

#[test]
fn critere_6_renommage() {
    let mut demo = Demo::new();
    let other = demo
        .project
        .add_empty_workbook(demo.root, Some("Book2"))
        .expect("classeur");

    let before = demo.project.tree().clone();
    assert!(matches!(
        demo.project.rename(other, "Book1"),
        Err(CoreError::DuplicateName(_))
    ));
    assert_eq!(**demo.project.tree(), *before);
    assert!(Arc::ptr_eq(demo.project.tree(), &before));

    assert_eq!(demo.project.rename(other, "Book2").expect("no-op"), "Book2");

    assert_eq!(
        demo.project.rename(other, "Essais").expect("renommage"),
        "Book2"
    );
    assert_eq!(name_of(&demo.project, other), "Essais");

    assert_eq!(demo.project.rename(demo.root, "X").expect("racine"), "Demo");
    assert_eq!(demo.project.name(), "X");
}

#[test]
fn critere_7_natures_et_index_refuses() {
    let mut demo = Demo::new();

    assert!(matches!(
        demo.project.insert_node(demo.sheet, 0, "X", folder_kind()),
        Err(CoreError::Structure(_))
    ));
    assert!(matches!(
        demo.project
            .add_column(demo.book, None, None, Column::new(ColumnType::Numeric)),
        Err(CoreError::Structure(_))
    ));
    assert!(matches!(
        demo.project
            .add_column(demo.sheet, Some(5), None, Column::new(ColumnType::Numeric)),
        Err(CoreError::IndexOutOfBounds(_))
    ));
    let last = demo
        .project
        .add_column(demo.sheet, Some(2), None, Column::new(ColumnType::Numeric))
        .expect("insertion en fin");
    assert_eq!(demo.project.tree().index_in_parent(last).expect("index"), 2);
}

#[test]
fn critere_8_suppression() {
    let mut demo = Demo::new();
    assert_eq!(demo.project.remove(demo.temps).expect("suppression"), 0);
    assert!(!demo.project.tree().contains(demo.temps));
    assert_eq!(
        child_names(&demo.project, demo.sheet),
        vec!["Signal".to_string()]
    );

    let before = demo.project.tree().len();
    // Book1 + Sheet1 + Signal = 3 nœuds.
    assert_eq!(demo.project.remove(demo.book).expect("suppression"), 0);
    assert_eq!(demo.project.tree().len(), before - 3);
    assert!(!demo.project.tree().contains(demo.sheet));
    assert!(!demo.project.tree().contains(demo.signal));

    assert!(matches!(
        demo.project.remove(demo.root),
        Err(CoreError::Structure(_))
    ));
}

#[test]
fn critere_9_deplacements_acceptes() {
    let mut demo = Demo::new();
    let c = demo
        .project
        .add_column(
            demo.sheet,
            None,
            Some("Autre"),
            Column::new(ColumnType::Numeric),
        )
        .expect("colonne");

    // Réordonner : index 0 → 2 sur trois colonnes (Temps, Signal, Autre).
    assert_eq!(
        demo.project
            .move_node(demo.temps, demo.sheet, 2)
            .expect("réordonnancement"),
        (demo.sheet, 0)
    );
    assert_eq!(
        child_names(&demo.project, demo.sheet),
        vec![
            "Signal".to_string(),
            "Autre".to_string(),
            "Temps".to_string()
        ]
    );
    assert!(demo.project.tree().contains(c));

    // Déplacer une feuille vers un autre classeur.
    let book2 = demo
        .project
        .add_empty_workbook(demo.root, None)
        .expect("classeur vide");
    assert_eq!(
        demo.project
            .move_node(demo.sheet, book2, 0)
            .expect("déplacement de feuille"),
        (demo.book, 0)
    );
    assert_eq!(
        demo.project.path_of(demo.temps).expect("chemin"),
        format!("/[{}]Sheet1!Temps", name_of(&demo.project, book2))
    );

    // Déplacer un classeur vers un autre dossier.
    let ailleurs = demo
        .project
        .add_folder(demo.root, "Ailleurs")
        .expect("dossier");
    let (old_parent, old_index) = demo
        .project
        .move_node(book2, ailleurs, 0)
        .expect("déplacement de classeur");
    assert_eq!(old_parent, demo.root);
    assert_eq!(old_index, 1);
    assert_eq!(
        demo.project.path_of(demo.temps).expect("chemin"),
        format!("/Ailleurs/[{}]Sheet1!Temps", name_of(&demo.project, book2))
    );
    // Les identifiants n'ont pas bougé.
    assert!(demo.project.tree().contains(demo.temps));
    assert!(demo.project.tree().contains(demo.sheet));
}

#[test]
fn critere_10_deplacements_refuses_laissent_l_arbre_intact() {
    let mut demo = Demo::new();
    let f1 = demo.project.add_folder(demo.root, "F1").expect("dossier");
    let f2 = demo.project.add_folder(demo.root, "F2").expect("dossier");
    let x1 = demo.project.add_folder(f1, "X").expect("dossier X dans F1");
    demo.project.add_folder(f2, "X").expect("dossier X dans F2");
    let interne = demo.project.add_folder(f1, "Interne").expect("dossier");

    let before = demo.project.tree().clone();

    // Une colonne ne va pas dans un classeur.
    assert!(matches!(
        demo.project.move_node(demo.temps, demo.book, 0),
        Err(CoreError::Structure(_))
    ));
    // Un dossier ne va pas dans son propre descendant.
    match demo.project.move_node(f1, interne, 0) {
        Err(CoreError::Structure(message)) => {
            assert!(message.contains("circulaire"), "{message}")
        }
        other => panic!("attendu Structure circulaire, obtenu {other:?}"),
    }
    // Nom déjà pris dans la cible.
    assert!(matches!(
        demo.project.move_node(x1, f2, 0),
        Err(CoreError::DuplicateName(_))
    ));
    // Index trop grand (nom libre dans la cible : c'est bien l'index qui bloque).
    assert!(matches!(
        demo.project.move_node(interne, f2, 9),
        Err(CoreError::IndexOutOfBounds(_))
    ));
    // La racine ne se déplace pas.
    assert!(matches!(
        demo.project.move_node(demo.root, f1, 0),
        Err(CoreError::Structure(_))
    ));

    assert!(Arc::ptr_eq(demo.project.tree(), &before));
    assert_eq!(**demo.project.tree(), *before);
}

// -- 11-13. chemins --

#[test]
fn critere_11_resolution_de_chemins() {
    let demo = Demo::new();
    let p = &demo.project;
    assert_eq!(p.resolve_path("/").expect("racine"), demo.root);
    assert_eq!(p.resolve_path("/Manips").expect("dossier"), demo.folder);
    assert_eq!(p.resolve_path("[Book1]").expect("classeur"), demo.book);
    assert_eq!(
        p.resolve_path("[Book1]Sheet1").expect("feuille"),
        demo.sheet
    );
    assert_eq!(
        p.resolve_path("[Book1]Sheet1!Temps").expect("colonne"),
        demo.temps
    );
    assert_eq!(
        p.resolve_path("/Manips/[Book1]Sheet1!Temps")
            .expect("colonne préfixée"),
        demo.temps
    );
    match p.resolve_path("/Ailleurs/[Book1]Sheet1!Temps") {
        Err(CoreError::Path(message)) => {
            assert!(message.contains("n'est pas dans le dossier"), "{message}")
        }
        other => panic!("attendu Path, obtenu {other:?}"),
    }
}

#[test]
fn critere_12_chemins_irresolubles() {
    let mut demo = Demo::new();
    demo.project
        .add_graph(demo.root, Some("Fig1"), Default::default())
        .expect("graphe");
    let p = &demo.project;
    for text in [
        "[Absent]",
        "[Book1]SheetX",
        "[Book1]Sheet1!Absente",
        "/Inconnu",
        "/Manips/Inconnu",
        "/Manips/Book1/Inconnu",
    ] {
        assert!(
            matches!(p.resolve_path(text), Err(CoreError::Path(_))),
            "attendu Path pour {text:?}, obtenu {:?}",
            p.resolve_path(text)
        );
    }
    match p.resolve_path("[Fig1]Sheet1") {
        Err(CoreError::Path(message)) => {
            assert!(message.contains("pas un classeur"), "{message}")
        }
        other => panic!("attendu Path, obtenu {other:?}"),
    }
}

#[test]
fn critere_13_path_of_et_resolve_sont_reciproques() {
    let mut demo = Demo::new();
    // Une feuille vide et une colonne vide, plus une fenêtre hors dossier.
    let vide = demo
        .project
        .add_empty_workbook(demo.root, Some("Vide"))
        .expect("classeur vide");
    let feuille_vide = demo
        .project
        .add_worksheet(vide, None)
        .expect("feuille vide");
    demo.project
        .add_column(feuille_vide, None, None, Column::new(ColumnType::Numeric))
        .expect("colonne vide");
    demo.project
        .add_graph(demo.root, None, Default::default())
        .expect("graphe");
    demo.project.add_note(demo.folder, None, "").expect("note");

    for id in demo.project.tree().walk_all() {
        let path = demo.project.path_of(id).expect("chemin");
        assert_eq!(
            demo.project.resolve_path(&path).expect(&path),
            id,
            "aller-retour pour {path}"
        );
    }

    assert!(matches!(
        demo.project.path_of(NodeId::from_raw(9999)),
        Err(CoreError::UnknownNode(_))
    ));
}

// -- 14-17. données et lignes --

#[test]
fn critere_14_ecriture_de_valeurs() {
    let (mut project, sheet, ids) = sheet_with(&[("A", &[1.0, 2.0, 3.0])]);
    let column = ids[0];
    let delta = project
        .set_values(column, 1, ValueBlock::Float(vec![20.0, 30.0]))
        .expect("écriture");
    assert_eq!(floats(&project, column), vec![1.0, 20.0, 30.0]);
    assert_eq!(delta.start, 1);
    assert_eq!(delta.previous, ValueBlock::Float(vec![2.0, 3.0]));
    assert_eq!(delta.previous_length, 3);

    assert!(matches!(
        project.set_values(sheet, 0, ValueBlock::Float(vec![1.0])),
        Err(CoreError::Structure(_))
    ));
}

#[test]
fn critere_15_lignes_seules_les_colonnes_assez_longues() {
    let (mut project, sheet, ids) = sheet_with(&[
        ("longue", &[1.0, 2.0, 3.0, 4.0, 5.0]),
        ("courte", &[1.0, 2.0]),
    ]);
    let (longue, courte) = (ids[0], ids[1]);

    let lengthened = project.insert_rows(sheet, 3, 2).expect("insertion");
    assert_eq!(lengthened, vec![longue]);
    assert_eq!(floats(&project, longue).len(), 7);
    assert_eq!(floats(&project, courte), vec![1.0, 2.0]);

    let removed = project.delete_rows(sheet, 3, 2).expect("suppression");
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].0, longue);
    match &removed[0].1 {
        ValueBlock::Float(values) => {
            assert_eq!(values.len(), 2);
            assert!(values.iter().all(|v| v.is_nan()), "{values:?}");
        }
        other => panic!("attendu un bloc Float, obtenu {other:?}"),
    }
    assert_eq!(floats(&project, longue), vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    assert_eq!(floats(&project, courte), vec![1.0, 2.0]);

    // Au-delà de toutes les colonnes : no-op.
    let before = project.tree().clone();
    let lengthened = project.insert_rows(sheet, 10, 3).expect("no-op");
    assert_eq!(lengthened, Vec::<NodeId>::new());
    assert_eq!(**project.tree(), *before);

    let (mut project, sheet, ids) = sheet_with(&[("A", &[1.0, 2.0, 3.0]), ("B", &[1.0])]);
    let removed = project.delete_rows(sheet, 2, 1).expect("suppression");
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].0, ids[0]);
    assert_eq!(floats(&project, ids[0]), vec![1.0, 2.0]);
    assert_eq!(floats(&project, ids[1]), vec![1.0]);
}

proptest::proptest! {
    /// Critère 16 : sur une feuille de colonnes de longueurs quelconques,
    /// `insert_rows` puis `delete_rows` aux mêmes coordonnées redonnent
    /// exactement l'arbre initial.
    #[test]
    fn critere_16_insertion_puis_suppression_de_lignes_est_l_identite(
        lengths in proptest::collection::vec(0usize..20, 1..5),
        at in 0usize..25,
        count in 0usize..10,
    ) {
        let specs: Vec<(String, Vec<f64>)> = lengths
            .iter()
            .enumerate()
            .map(|(i, len)| {
                (format!("C{i}"), (0..*len).map(|row| row as f64).collect())
            })
            .collect();
        let borrowed: Vec<(&str, &[f64])> = specs
            .iter()
            .map(|(name, values)| (name.as_str(), values.as_slice()))
            .collect();
        let (mut project, sheet, _) = sheet_with(&borrowed);

        let before = project.tree().clone();
        project.insert_rows(sheet, at, count).expect("insertion");
        project.delete_rows(sheet, at, count).expect("suppression");
        proptest::prop_assert_eq!(&**project.tree(), &*before);
    }
}

#[test]
fn critere_17_row_count() {
    let (project, sheet, _) = sheet_with(&[]);
    assert_eq!(project.tree().row_count(sheet).expect("feuille vide"), 0);

    let (project, sheet, _) = sheet_with(&[("A", &[1.0, 2.0]), ("B", &[1.0, 2.0, 3.0, 4.0])]);
    assert_eq!(project.tree().row_count(sheet).expect("feuille"), 4);
}

#[test]
fn les_metadonnees_de_colonne_et_de_fenetre_rendent_les_precedentes() {
    let mut demo = Demo::new();
    let meta = ColumnMeta {
        role: ColumnRole::X,
        unit: "s".to_string(),
        ..Default::default()
    };
    let previous = demo
        .project
        .set_column_meta(demo.temps, meta.clone())
        .expect("métadonnées");
    assert_eq!(previous, ColumnMeta::default());
    assert_eq!(
        demo.project
            .tree()
            .get(demo.temps)
            .expect("colonne")
            .as_column()
            .expect("colonne")
            .meta(),
        &meta
    );

    let window_meta = WindowMeta {
        label: "Manip 3".to_string(),
        comment: "essai".to_string(),
    };
    assert_eq!(
        demo.project
            .set_window_meta(demo.book, window_meta)
            .expect("métadonnées de fenêtre"),
        WindowMeta::default()
    );
    assert!(matches!(
        demo.project
            .set_window_meta(demo.sheet, WindowMeta::default()),
        Err(CoreError::Structure(_))
    ));

    let note = demo.project.add_note(demo.root, None, "un").expect("note");
    assert_eq!(
        demo.project.set_note_text(note, "deux").expect("texte"),
        "un"
    );
    assert!(matches!(
        demo.project.set_note_text(demo.book, "x"),
        Err(CoreError::Structure(_))
    ));

    let graph = demo
        .project
        .add_graph(demo.root, None, plotyx_plot::PlotSpec::default())
        .expect("graphe");
    let spec = plotyx_plot::PlotSpec {
        title: "Figure".to_string(),
        ..Default::default()
    };
    assert_eq!(
        demo.project
            .set_plot_spec(graph, spec.clone())
            .expect("spec"),
        plotyx_plot::PlotSpec::default()
    );
    assert_eq!(
        demo.project
            .tree()
            .get(graph)
            .expect("graphe")
            .kind()
            .clone(),
        plotyx_core::NodeKind::Graph {
            meta: WindowMeta::default(),
            spec: spec.clone(),
        }
    );
    assert!(matches!(
        demo.project.set_plot_spec(demo.book, spec),
        Err(CoreError::Structure(_))
    ));
}

#[test]
fn changement_de_type_et_restauration_d_une_colonne() {
    let (mut project, _, ids) = sheet_with(&[("A", &[1.0, 2.0])]);
    let column = ids[0];
    let (old_type, old_values) = project
        .set_column_type(column, ColumnType::Text)
        .expect("changement de type");
    assert_eq!(old_type, ColumnType::Numeric);
    assert_eq!(
        project
            .tree()
            .get(column)
            .expect("colonne")
            .as_column()
            .expect("colonne")
            .get(0),
        Cell::Text(Some("1".to_string()))
    );
    project
        .restore_column(column, old_type, old_values)
        .expect("restauration");
    assert_eq!(floats(&project, column), vec![1.0, 2.0]);

    let delta = project.set_length(column, 4).expect("longueur");
    assert_eq!(delta.previous_length, 2);
    assert_eq!(floats(&project, column).len(), 4);
}

// -- 18-21. partage structurel, identité --

#[test]
fn critere_18_partage_structurel_apres_renommage() {
    let demo = Demo::new();
    let mut project = demo.project;
    let before = project.tree().clone();

    project.rename(demo.temps, "Z").expect("renommage");

    assert_eq!(before.get(demo.temps).expect("colonne").name(), "Temps");
    assert_eq!(name_of(&project, demo.temps), "Z");

    for id in before.walk_all() {
        let old = before.node_arc(id).expect("nœud");
        let new = project.tree().node_arc(id).expect("nœud");
        if id == demo.temps {
            assert!(
                !Arc::ptr_eq(old, new),
                "la colonne renommée doit être clonée"
            );
        } else {
            assert!(Arc::ptr_eq(old, new), "{id} doit rester partagé");
        }
    }
}

#[test]
fn critere_18_bis_partage_structurel_apres_ecriture() {
    let demo = Demo::new();
    let mut project = demo.project;
    let before = project.tree().clone();

    project
        .set_values(demo.temps, 0, ValueBlock::Float(vec![9.0]))
        .expect("écriture");

    for id in before.walk_all() {
        let old = before.node_arc(id).expect("nœud");
        let new = project.tree().node_arc(id).expect("nœud");
        if id == demo.temps {
            assert!(!Arc::ptr_eq(old, new));
        } else {
            assert!(Arc::ptr_eq(old, new), "{id} doit rester partagé");
        }
    }

    // Les buffers des autres colonnes sont physiquement partagés.
    let old_signal = before
        .get(demo.signal)
        .expect("colonne")
        .as_column()
        .expect("colonne")
        .float_values()
        .expect("numérique");
    let new_signal = project
        .tree()
        .get(demo.signal)
        .expect("colonne")
        .as_column()
        .expect("colonne")
        .float_values()
        .expect("numérique");
    assert!(Arc::ptr_eq(old_signal, new_signal));
}

#[test]
fn critere_19_copie_a_l_ecriture_sous_instantane() {
    let demo = Demo::new();
    let mut project = demo.project;

    let before = project.tree().clone();
    project
        .set_values(demo.temps, 0, ValueBlock::Float(vec![9.0]))
        .expect("écriture");
    let old = before
        .get(demo.temps)
        .expect("colonne")
        .as_column()
        .expect("colonne");
    assert_eq!(old.get(0), Cell::Float(0.0));
    assert_eq!(floats(&project, demo.temps), vec![9.0, 1.0, 2.0]);
    drop(before);

    // Sans instantané vivant, l'écriture se fait en place : même buffer.
    let ptr_before = buffer_ptr(&project, demo.temps);
    project
        .set_values(demo.temps, 0, ValueBlock::Float(vec![7.0]))
        .expect("écriture");
    assert_eq!(ptr_before, buffer_ptr(&project, demo.temps));
}

fn buffer_ptr(project: &Project, column: NodeId) -> *const Vec<f64> {
    Arc::as_ptr(
        project
            .tree()
            .get(column)
            .expect("colonne")
            .as_column()
            .expect("colonne")
            .float_values()
            .expect("numérique"),
    )
}

#[test]
fn critere_20_les_identifiants_ne_sont_jamais_reutilises() {
    let (mut project, sheet, _) = sheet_with(&[]);
    let x = project
        .add_column(sheet, None, None, Column::new(ColumnType::Numeric))
        .expect("colonne A");

    let snapshot = project.tree().clone();
    project.remove(x).expect("suppression");
    assert!(!project.tree().contains(x));

    project.replace_tree(snapshot);
    assert!(project.tree().contains(x));
    assert_eq!(name_of(&project, x), "A");

    let next = project
        .add_column(sheet, None, None, Column::new(ColumnType::Numeric))
        .expect("colonne");
    assert!(next.raw() > x.raw(), "{next} doit dépasser {x}");
    for (id, _) in project.tree().iter() {
        assert!(id == next || id.raw() < next.raw());
    }

    // Sans restauration, l'identifiant libéré n'est pas recyclé non plus.
    let mut project = Project::new("Demo").expect("nom valide");
    let root = project.root();
    let y = project.add_folder(root, "Y").expect("dossier");
    project.remove(y).expect("suppression");
    let z = project.add_folder(root, "Z").expect("dossier");
    assert_ne!(z, y);
    assert!(z.raw() > y.raw());
}

#[test]
fn critere_21_from_tree_reprend_le_compteur() {
    let mut project = Project::new("Demo").expect("nom valide");
    let root = project.root();
    // Identifiants 1..=7 : racine + 6 dossiers.
    for n in 0..6 {
        project.add_folder(root, &format!("D{n}")).expect("dossier");
    }
    assert_eq!(project.next_id_hint(), 8);

    let reloaded = Project::from_tree(project.tree().clone(), 3);
    assert_eq!(reloaded.next_id_hint(), 8);

    // `next_id_hint` EST déjà le prochain identifiant à attribuer, pas le
    // plus grand identifiant déjà attribué : avec un hint de 20, rien dans
    // l'arbre (plus grand id : 7) ne le contredit, le compteur repart donc
    // directement à 20. L'ancienne valeur attendue ici (21) venait de
    // traiter `hint` comme « le plus grand identifiant déjà attribué » et de
    // lui rajouter 1 après le `max` — confusion corrigée dans
    // `IdAllocator::resume`.
    let reloaded = Project::from_tree(project.tree().clone(), 20);
    assert_eq!(reloaded.next_id_hint(), 20);
}

#[test]
fn critere_21_bis_aller_retour_sans_disque_preserve_next_id_hint() {
    // Critère naturel qu'un aller-retour disque doit respecter, vérifié ici
    // sans même passer par plotyx-persistence : reconstruire un `Project`
    // depuis son propre arbre et son propre `next_id_hint` ne doit ni le
    // faire dériver, ni gaspiller un identifiant.
    let mut project = Project::new("X").expect("nom valide");
    let root = project.root();

    let book1 = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("classeur");
    let sheet1 = project
        .add_worksheet(book1, Some("Sheet1"))
        .expect("feuille");
    project
        .add_column(sheet1, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .expect("colonne");

    let book2 = project
        .add_empty_workbook(root, Some("Book2"))
        .expect("classeur");
    let sheet2 = project
        .add_worksheet(book2, Some("Sheet2"))
        .expect("feuille");
    project
        .add_column(sheet2, None, Some("B"), Column::from_f64(&[3.0]))
        .expect("colonne");

    let expected = project.next_id_hint();
    let mut reloaded = Project::from_tree(project.tree().clone(), expected);
    assert_eq!(reloaded.next_id_hint(), expected);

    // Un nœud ajouté après reprise reçoit exactement l'identifiant annoncé
    // par `next_id_hint()`, pas un de plus.
    let new_id = reloaded
        .add_column(sheet2, None, Some("C"), Column::new(ColumnType::Numeric))
        .expect("colonne ajoutée après reprise");
    assert_eq!(new_id.raw(), expected);
}

// -- 22-23. from_nodes --

#[test]
fn critere_22_from_nodes_reconstruit_la_fixture() {
    let demo = Demo::new();
    let nodes: Vec<Node> = demo
        .project
        .tree()
        .iter()
        .map(|(_, node)| node.clone())
        .collect();
    let rebuilt = Tree::from_nodes(demo.root, nodes).expect("arbre valide");
    assert_eq!(rebuilt, **demo.project.tree());
}

#[test]
fn critere_23_from_nodes_refuse_les_arbres_incoherents() {
    let root_id = NodeId::from_raw(1);
    let a = NodeId::from_raw(2);
    let b = NodeId::from_raw(3);

    let root_with = |children: Vec<NodeId>| {
        Node::new(root_id, "Projet", NodeKind::Folder { children }).expect("racine")
    };
    let child = |id: NodeId, name: &str, parent: NodeId, kind: NodeKind| {
        Node::with_parent(id, name, Some(parent), kind).expect("nœud")
    };

    let cases: Vec<(&str, NodeId, Vec<Node>)> = vec![
        (
            "identifiant dupliqué",
            root_id,
            vec![root_with(vec![]), root_with(vec![])],
        ),
        ("racine absente", root_id, vec![]),
        (
            "racine non-dossier",
            root_id,
            vec![Node::new(root_id, "Book1", workbook_kind()).expect("nœud")],
        ),
        (
            "racine avec parent",
            root_id,
            vec![Node::with_parent(root_id, "Projet", Some(a), folder_kind()).expect("nœud")],
        ),
        (
            "parent absent",
            root_id,
            vec![
                root_with(vec![]),
                child(a, "A", NodeId::from_raw(42), folder_kind()),
            ],
        ),
        (
            "absent de la liste d'enfants de son parent",
            root_id,
            vec![root_with(vec![]), child(a, "A", root_id, folder_kind())],
        ),
        (
            "enfant dont le parent est un autre nœud",
            root_id,
            vec![
                root_with(vec![a, b]),
                Node::with_parent(
                    a,
                    "A",
                    Some(root_id),
                    NodeKind::Folder { children: vec![b] },
                )
                .expect("nœud"),
                child(b, "B", root_id, folder_kind()),
            ],
        ),
        (
            "nœud inaccessible (deux racines)",
            root_id,
            vec![
                root_with(vec![]),
                Node::new(a, "Autre", folder_kind()).expect("nœud"),
            ],
        ),
        (
            "colonne sous un dossier",
            root_id,
            vec![root_with(vec![a]), child(a, "A", root_id, column_kind())],
        ),
        (
            "deux fenêtres de même nom",
            root_id,
            vec![
                root_with(vec![a, b]),
                child(a, "Book1", root_id, workbook_kind()),
                child(b, "Book1", root_id, workbook_kind()),
            ],
        ),
    ];

    for (label, root, nodes) in cases {
        let outcome = Tree::from_nodes(root, nodes);
        assert!(
            matches!(outcome, Err(CoreError::Structure(_))),
            "attendu Structure pour « {label} », obtenu {outcome:?}"
        );
    }
}

#[test]
fn from_nodes_refuse_un_dossier_et_une_fenetre_homonymes() {
    // A8 (`guide-de-correction-robustesse.md` §F9) — ce test affirmait
    // auparavant que « ce que `insert` peut produire, `from_nodes` doit
    // pouvoir relire », en construisant précisément le cas qu'A8 corrige :
    // un dossier et un classeur frères homonymes. `insert` (via
    // `check_name_available`) refuse désormais cette paire lui-même — la
    // prémisse du test n'a donc plus lieu d'être, mais l'esprit reste : ce
    // que l'API de mutation refuse de produire, `from_nodes` doit aussi le
    // refuser en le relisant (c'est `critere_23_from_nodes_refuse_les_
    // arbres_incoherents` qui couvre ce sens-là) ; ce test-ci vérifie le
    // premier maillon, que la paire n'est plus produisible du tout.
    let mut project = Project::new("Demo").expect("nom valide");
    let root = project.root();
    project.add_folder(root, "X").expect("dossier");
    assert!(matches!(
        project.add_empty_workbook(root, Some("X")),
        Err(CoreError::DuplicateName(_))
    ));
}

#[test]
fn les_valeurs_typees_survivent_a_un_aller_retour_from_nodes() {
    let demo = Demo::new();
    let nodes: Vec<Node> = demo.project.tree().iter().map(|(_, n)| n.clone()).collect();
    let rebuilt = Tree::from_nodes(demo.root, nodes).expect("arbre valide");
    let column = rebuilt
        .get(demo.signal)
        .expect("colonne")
        .as_column()
        .expect("colonne");
    assert!(matches!(column.values(), ColumnValues::Float(_)));
    assert_eq!(column.len(), 2);
}

// -- 24-26. primitives par colonne, atomicité --

#[test]
fn critere_24_les_primitives_par_colonne_annulent_delete_rows() {
    let (mut project, sheet, ids) = sheet_with(&[("A", &[1.0, 2.0])]);
    let column = ids[0];

    let removed = project.delete_rows(sheet, 1, 5).expect("suppression");
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].0, column);
    assert_eq!(removed[0].1, ValueBlock::Float(vec![2.0]));
    assert_eq!(floats(&project, column), vec![1.0]);

    // Recette d'annulation : réinsérer puis réécrire, colonne par colonne.
    for (id, block) in removed {
        let len = block.len();
        project
            .insert_column_rows(id, 1, len)
            .expect("insertion par colonne");
        project.set_values(id, 1, block).expect("réécriture");
    }
    assert_eq!(floats(&project, column), vec![1.0, 2.0]);
}

#[test]
fn critere_25_les_primitives_par_colonne_valident_leurs_bornes() {
    let (mut project, sheet, ids) = sheet_with(&[("A", &[1.0, 2.0])]);
    let column = ids[0];

    // La cible doit être une colonne, pas la feuille.
    assert!(matches!(
        project.insert_column_rows(sheet, 0, 1),
        Err(CoreError::Structure(_))
    ));

    // Position au-delà de `len + 1` refusée.
    assert!(matches!(
        project.insert_column_rows(column, 3, 1),
        Err(CoreError::IndexOutOfBounds(_))
    ));

    // Suppression de zéro ligne : no-op, bloc vide rendu.
    let block = project
        .delete_column_rows(column, 0, 0)
        .expect("suppression de zéro ligne");
    assert!(block.is_empty());
    assert_eq!(floats(&project, column), vec![1.0, 2.0]);
}

#[test]
fn critere_26_add_workbook_ajoute_exactement_deux_noeuds() {
    let mut project = Project::new("Demo").expect("nom valide");
    let root = project.root();
    let before = project.tree().len();

    project.add_workbook(root, None).expect("classeur");

    assert_eq!(project.tree().len(), before + 2);
}
