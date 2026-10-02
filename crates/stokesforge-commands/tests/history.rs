//! Critères d'acceptation du bus de commandes, exercés par l'API publique
//! seule : on n'inspecte jamais l'intérieur de `History`, seulement ce qu'un
//! appelant peut observer (l'arbre du projet et les accesseurs de la pile).

use std::sync::Arc;

use stokesforge_commands::{
    ColumnRef, Command, CommandError, History, HistoryLabel, MacroKind, DEFAULT_BYTE_LIMIT,
    DEFAULT_LIMIT,
};
use stokesforge_core::{
    Cell, Column, ColumnMeta, ColumnRole, ColumnType, CoreError, MixedCell, Node, NodeId, NodeKind,
    Project, Tree, ValueBlock, WindowMeta,
};
use stokesforge_plot::{ColorUsage, PlotSpec, SavedColors};

// -- fixture --------------------------------------------------------------

/// Projet « Demo » : `Book1` avec `Sheet1` (colonnes `A [1,2,3]`, `B [10,20]`,
/// texte `T ["a", None, "c"]`) et `Sheet2` (colonne `X [0.5, 1.5]`).
///
/// Le montage passe par les primitives de `Project` et non par des commandes :
/// c'est le seul endroit où la règle d'or (« aucune mutation hors d'une
/// commande ») souffre une exception, et c'est celle qu'elle prévoit — les
/// tests.
struct Fixture {
    project: Project,
    book: NodeId,
    sheet1: NodeId,
    sheet2: NodeId,
    a: NodeId,
    b: NodeId,
    t: NodeId,
    x: NodeId,
}

fn fixture() -> Fixture {
    let mut project = Project::new("Demo").expect("projet Demo");
    let root = project.root();
    let book = project
        .add_workbook(root, Some("Book1"))
        .expect("classeur Book1");
    let sheet1 = project
        .resolve_path("[Book1]Sheet1")
        .expect("feuille Sheet1");
    let a = project
        .add_column(sheet1, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("colonne A");
    let b = project
        .add_column(sheet1, None, Some("B"), Column::from_f64(&[10.0, 20.0]))
        .expect("colonne B");
    let t = project
        .add_column(
            sheet1,
            None,
            Some("T"),
            Column::from_text(&[Some("a".to_string()), None, Some("c".to_string())]),
        )
        .expect("colonne T");
    let sheet2 = project
        .add_worksheet(book, Some("Sheet2"))
        .expect("feuille Sheet2");
    let x = project
        .add_column(sheet2, None, Some("X"), Column::from_f64(&[0.5, 1.5]))
        .expect("colonne X");
    Fixture {
        project,
        book,
        sheet1,
        sheet2,
        a,
        b,
        t,
        x,
    }
}

// -- lectures d'appoint ---------------------------------------------------

fn snapshot(project: &Project) -> Arc<Tree> {
    Arc::clone(project.tree())
}

fn floats(project: &Project, column: NodeId) -> Vec<f64> {
    project
        .tree()
        .node(column)
        .and_then(Node::as_column)
        .and_then(|c| c.float_values().cloned())
        .map(|values| values.as_slice().to_vec())
        .expect("colonne numérique")
}

fn texts(project: &Project, column: NodeId) -> Vec<Option<String>> {
    let tree = project.tree();
    let target = tree
        .node(column)
        .and_then(Node::as_column)
        .expect("colonne existante");
    (0..target.len())
        .map(|row| match target.get(row) {
            Cell::Text(text) => text,
            Cell::Float(_) => panic!("colonne numérique, texte attendu"),
        })
        .collect()
}

fn column_len(project: &Project, column: NodeId) -> usize {
    project
        .tree()
        .node(column)
        .and_then(Node::as_column)
        .map(Column::len)
        .expect("colonne existante")
}

fn column_type(project: &Project, column: NodeId) -> ColumnType {
    project
        .tree()
        .node(column)
        .and_then(Node::as_column)
        .map(Column::column_type)
        .expect("colonne existante")
}

fn names(project: &Project, parent: NodeId) -> Vec<String> {
    project
        .tree()
        .children(parent)
        .expect("parent existant")
        .iter()
        .filter_map(|id| project.tree().node(*id))
        .map(|node| node.name().to_string())
        .collect()
}

fn set_cells(column: NodeId, start: usize, values: &[f64]) -> Command {
    Command::SetCells {
        column,
        start,
        values: ValueBlock::Float(values.to_vec()),
        expected_version: None,
    }
}

// -- 1. bus : aller-retour élémentaire ------------------------------------

#[test]
fn execute_undo_redo_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    assert_eq!(history.limit(), DEFAULT_LIMIT);
    assert!(history.is_empty());

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[0.0]))
        .expect("saisie acceptée");

    assert_eq!(history.len(), 1);
    assert!(history.can_undo());
    assert!(!history.can_redo());
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::SetCells {
            column: Some(ColumnRef(0))
        })
    );
    assert_eq!(floats(&f.project, f.a), vec![0.0, 2.0, 3.0]);

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(floats(&f.project, f.a), vec![1.0, 2.0, 3.0]);
    assert!(history.can_redo());
    assert_eq!(
        history.redo_label(),
        Some(&HistoryLabel::SetCells {
            column: Some(ColumnRef(0))
        })
    );
    assert_eq!(history.undo_label(), None);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(floats(&f.project, f.a), vec![0.0, 2.0, 3.0]);

    // Pile vidée puis interrogée : rien à annuler n'est pas une erreur.
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(history.undo(&mut f.project), Ok(false));
}

// -- 2. application avant empilement --------------------------------------

#[test]
fn a_failed_command_changes_neither_tree_nor_stack() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    // Nom déjà pris par la colonne B de la même feuille.
    let err = history
        .execute(
            &mut f.project,
            Command::Rename {
                id: f.a,
                name: "B".to_string(),
            },
        )
        .expect_err("renommage refusé");
    assert!(matches!(
        err,
        CommandError::Core(CoreError::DuplicateName(_))
    ));
    assert_eq!(history.len(), 0);
    assert_eq!(**f.project.tree(), *before);

    let err = history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: Some(99),
                name: None,
                column: Column::from_f64(&[1.0]),
            },
        )
        .expect_err("index refusé");
    assert!(matches!(
        err,
        CommandError::Core(CoreError::IndexOutOfBounds(_))
    ));
    assert_eq!(history.len(), 0);
    assert_eq!(**f.project.tree(), *before);

    let unknown = NodeId::from_raw(9_999);
    let err = history
        .execute(&mut f.project, set_cells(unknown, 0, &[1.0]))
        .expect_err("nœud inconnu refusé");
    assert_eq!(err, CommandError::Core(CoreError::UnknownNode(unknown)));
    assert_eq!(history.len(), 0);
    assert!(!history.can_undo());
    assert_eq!(**f.project.tree(), *before);
}

// -- 3. une nouvelle commande coupe la branche de rétablissement ----------

#[test]
fn a_new_command_drops_the_redo_stack() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[7.0]))
        .expect("saisie");
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(history.redo_len(), 1);

    history
        .execute(&mut f.project, set_cells(f.b, 0, &[8.0]))
        .expect("saisie");
    assert_eq!(history.redo_len(), 0);
    assert!(!history.can_redo());
}

// -- 4. clear -------------------------------------------------------------

#[test]
fn clear_empties_both_stacks_and_is_refused_in_a_macro() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[7.0]))
        .expect("saisie");
    history
        .execute(&mut f.project, set_cells(f.a, 1, &[8.0]))
        .expect("saisie");
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(history.len(), 1);
    assert_eq!(history.redo_len(), 1);

    history.clear().expect("purge acceptée");
    assert_eq!(history.len(), 0);
    assert_eq!(history.redo_len(), 0);

    history.begin_macro(MacroKind::Import).unwrap();
    assert_eq!(history.clear(), Err(CommandError::MacroOpen("clear")));
    history
        .abort_macro(&mut f.project)
        .expect("macro vide abandonnée");
}

// -- 5. SetCells ----------------------------------------------------------

#[test]
fn set_cells_beyond_the_end_restores_the_previous_length() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(&mut f.project, set_cells(f.a, 5, &[42.0]))
        .expect("saisie hors longueur");
    let after = snapshot(&f.project);
    assert_eq!(column_len(&f.project, f.a), 6);
    assert!(floats(&f.project, f.a)[3].is_nan());

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(column_len(&f.project, f.a), 3);
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn set_cells_on_a_mixed_column() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::SetCells {
                column: f.t,
                start: 1,
                values: ValueBlock::Mixed(vec![MixedCell::Text("b".to_string())]),
                expected_version: None,
            },
        )
        .expect("saisie texte");
    let after = snapshot(&f.project);
    assert_eq!(
        texts(&f.project, f.t),
        vec![
            Some("a".to_string()),
            Some("b".to_string()),
            Some("c".to_string())
        ]
    );

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn set_cells_with_the_wrong_block_variant_is_refused() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    let err = history
        .execute(
            &mut f.project,
            Command::SetCells {
                column: f.a,
                start: 0,
                values: ValueBlock::Mixed(vec![MixedCell::Text("x".to_string())]),
                expected_version: None,
            },
        )
        .expect_err("bloc mixte sur colonne numérique");
    assert!(matches!(
        err,
        CommandError::Core(CoreError::TypeMismatch(_))
    ));
    assert!(history.is_empty());
    assert_eq!(**f.project.tree(), *before);
}

// -- 6. AddColumn ---------------------------------------------------------

#[test]
fn add_column_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: None,
                column: Column::from_f64(&[9.0]),
            },
        )
        .expect("ajout de colonne");
    let after = snapshot(&f.project);
    // D1 (`guide-de-correction-robustesse.md` §F7) — le nom automatique part
    // du NOMBRE de colonnes déjà présentes (trois : A, B, T), pas du premier
    // nom libre façon tableur : reconstruire la liste des noms pris pour
    // trouver le plus petit trou (« C » resterait libre ici) est précisément
    // le coût en O(n³) que D1 supprime. Le premier candidat essayé est donc
    // « D ».
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T", "D"]);
    assert_eq!(history.undo_label(), Some(&HistoryLabel::AddColumn));

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn add_column_with_an_explicit_name_and_position() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: Some(0),
                name: Some("D".to_string()),
                column: Column::from_f64(&[4.0]),
            },
        )
        .expect("ajout de colonne D");
    let after = snapshot(&f.project);
    assert_eq!(names(&f.project, f.sheet1), vec!["D", "A", "B", "T"]);

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

// -- 7. DeleteColumn ------------------------------------------------------

#[test]
fn delete_column_round_trip_restores_the_same_node_id() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(&mut f.project, Command::DeleteColumn { column: f.a })
        .expect("suppression de A");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::DeleteColumn {
            column: Some(ColumnRef(0))
        })
    );
    assert!(!f.project.tree().contains(f.a));

    assert_eq!(history.undo(&mut f.project), Ok(true));
    // L'arbre d'avant retenait le nœud supprimé : il revient sous son
    // identifiant d'origine, avec ses valeurs.
    assert!(f.project.tree().contains(f.a));
    assert_eq!(floats(&f.project, f.a), vec![1.0, 2.0, 3.0]);
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert!(!f.project.tree().contains(f.a));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn delete_column_refuses_a_node_that_is_not_a_column() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    let err = history
        .execute(&mut f.project, Command::DeleteColumn { column: f.sheet1 })
        .expect_err("une feuille n'est pas une colonne");
    match err {
        CommandError::Core(CoreError::Structure(message)) => {
            assert!(
                message.contains("is not a column"),
                "message inattendu : {message}"
            );
        }
        other => panic!("erreur inattendue : {other:?}"),
    }
    assert!(history.is_empty());
    assert_eq!(**f.project.tree(), *before);
}

// -- 8. Rename ------------------------------------------------------------

#[test]
fn rename_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::Rename {
                id: f.a,
                name: "Z".to_string(),
            },
        )
        .expect("renommage");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::Rename {
            from: Some("A".to_string()),
            to: "Z".to_string()
        })
    );
    assert_eq!(names(&f.project, f.sheet1), vec!["Z", "B", "T"]);

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn renaming_to_the_current_name_is_not_an_entry() {
    // B5, le cas « Rename » (`guide-de-correction-robustesse.md` §F5, §7.3 —
    // consigné sans identifiant ni reproduction jusqu'ici). `Renommer M en
    // M` réussissait déjà et n'écrivait déjà rien dans l'arbre, mais
    // empilait quand même une entrée « Renommer M en M » : un cran
    // d'annulation qui ne défait rien. `entry_has_no_effect` compare
    // l'arbre d'avant à celui d'après pour toute entrée structurelle, donc
    // n'a pas besoin de connaître `Rename` par son nom.
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::Rename {
                id: f.a,
                name: "A".to_string(),
            },
        )
        .expect("renommage vers le nom courant");
    assert!(history.is_empty());
    assert_eq!(history.undo_label(), None);
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
    assert_eq!(**f.project.tree(), *before);
}

#[test]
fn renaming_the_root_is_accepted() {
    let mut f = fixture();
    let mut history = History::new();
    let root = f.project.root();

    history
        .execute(
            &mut f.project,
            Command::Rename {
                id: root,
                name: "P".to_string(),
            },
        )
        .expect("renommage de la racine");
    assert_eq!(f.project.name(), "P");
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(f.project.name(), "Demo");
}

// -- 9. Move --------------------------------------------------------------

#[test]
fn move_round_trip_restores_the_original_index() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::Move {
                id: f.a,
                new_parent: f.sheet2,
                index: 0,
            },
        )
        .expect("déplacement de A");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::Move {
            name: Some("A".to_string())
        })
    );
    assert_eq!(names(&f.project, f.sheet2), vec!["A", "X"]);
    assert_eq!(names(&f.project, f.sheet1), vec!["B", "T"]);

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn a_circular_move_is_refused() {
    let mut f = fixture();
    let root = f.project.root();
    // Montage local : deux dossiers imbriqués, pour pouvoir tenter le
    // déplacement d'un ancêtre dans son propre descendant.
    let outer = f
        .project
        .add_folder(root, "Outer")
        .expect("dossier extérieur");
    let inner = f
        .project
        .add_folder(outer, "Inner")
        .expect("dossier intérieur");
    let mut history = History::new();
    let before = snapshot(&f.project);

    let err = history
        .execute(
            &mut f.project,
            Command::Move {
                id: outer,
                new_parent: inner,
                index: 0,
            },
        )
        .expect_err("déplacement circulaire refusé");
    assert!(matches!(err, CommandError::Core(CoreError::Structure(_))));
    assert!(history.is_empty());
    assert_eq!(**f.project.tree(), *before);
}

// -- 10. SetColumnType ----------------------------------------------------

#[test]
fn set_column_type_mixed_to_numeric_round_trip() {
    // Ancien `set_column_type_text_to_numeric_round_trip` : `f.t` est
    // construite par `Column::from_text`, qui produit désormais directement
    // une colonne `Mixed` (il n'y a plus de type `Text` homogène) — même
    // couverture, sur `Mixed → Numeric`.
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::SetColumnType {
                column: f.t,
                column_type: ColumnType::Numeric,
            },
        )
        .expect("conversion de T");
    let after = snapshot(&f.project);
    // « la colonne C », pas « T » : le libellé désigne la colonne par sa
    // **position** dans la feuille, comme la grille l'affiche, jamais par son
    // nom de nœud. Ce cas-ci est précisément celui qui motive
    // `column_ref` — le nœud s'appelle `T` mais c'est la troisième colonne,
    // donc « C » à l'écran, et « T » n'apparaît nulle part pour l'utilisateur.
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::SetColumnType {
            column: Some(ColumnRef(2))
        })
    );
    let converted = floats(&f.project, f.t);
    assert_eq!(converted.len(), 3);
    assert!(
        converted.iter().all(|value| value.is_nan()),
        "« a », rien et « c » ne s'analysent pas : {converted:?}"
    );

    // La conversion est destructrice : seule la copie retenue permet de
    // retrouver le texte d'origine **exactement**.
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(column_type(&f.project, f.t), ColumnType::Mixed);
    assert_eq!(
        texts(&f.project, f.t),
        vec![Some("a".to_string()), None, Some("c".to_string())]
    );
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(column_type(&f.project, f.t), ColumnType::Numeric);
    assert_eq!(**f.project.tree(), *after);
}

// `set_column_type_numeric_to_text_round_trip` a été retiré : il ciblait
// `ColumnType::Text`, qui n'existe plus. `Numeric → Mixed` n'est pas son
// équivalent — la conversion est sans perte (`MixedCell::Number`, pas une
// reformulation en texte) — donc l'invariant qu'il vérifiait (le nombre
// s'écrit sous sa forme d'affichage) n'a plus de prise ; il n'y a rien à
// retargeter.

// -- 11. InsertRows -------------------------------------------------------

#[test]
fn insert_rows_touches_only_the_columns_that_have_that_row() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::InsertRows {
                sheet: f.sheet1,
                at: 2,
                count: 2,
            },
        )
        .expect("insertion de lignes");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::InsertRows { count: 2 })
    );
    assert_eq!(column_len(&f.project, f.a), 5);
    // B5 (`guide-de-correction-robustesse.md` §F5) — B fait 2 lignes :
    // `at == len` (2 == 2) est désormais une cible, pas seulement `at <
    // len` : c'est la seule façon d'ajouter une ligne en fin de colonne, la
    // plage que `Column::insert_rows` accepte déjà (`at ∈ 0..=len`).
    assert_eq!(column_len(&f.project, f.b), 4);
    assert_eq!(column_len(&f.project, f.t), 5);
    let values = floats(&f.project, f.a);
    assert!(values[2].is_nan() && values[3].is_nan());
    assert_eq!(values[4], 3.0);

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn an_insert_that_changes_nothing_is_not_an_entry() {
    // B5 (`guide-de-correction-robustesse.md` §F5) — une commande acceptée
    // qui ne change rien n'occupe pas de cran d'annulation : l'ancienne
    // version de ce test l'affirmait pour la raison inverse (« la position
    // du curseur d'historique dépendrait de l'état des données »), mais
    // c'est précisément ce qui trompait l'utilisateur — un succès sans
    // effet, et un « Annuler » qui ne défait rien. `History::execute`
    // reconnaît maintenant ce cas (`entry_has_no_effect`) et ne pousse rien.
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    // Aucune colonne n'a de ligne 10 : la commande est valide et ne change
    // rien.
    history
        .execute(
            &mut f.project,
            Command::InsertRows {
                sheet: f.sheet1,
                at: 10,
                count: 1,
            },
        )
        .expect("insertion sans effet");
    assert!(history.is_empty());
    assert_eq!(history.undo_label(), None);
    assert_eq!(**f.project.tree(), *before);

    // Rien à annuler : `undo` rend `Ok(false)`, pas une régression.
    assert_eq!(history.undo(&mut f.project), Ok(false));
    assert_eq!(**f.project.tree(), *before);
}

// -- 12. DeleteRows -------------------------------------------------------

#[test]
fn delete_rows_round_trip_even_when_a_column_is_emptied_to_its_end() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::DeleteRows {
                sheet: f.sheet1,
                at: 1,
                count: 5,
            },
        )
        .expect("suppression de lignes");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::DeleteRows { count: 5 })
    );
    assert_eq!(floats(&f.project, f.a), vec![1.0]);
    assert_eq!(floats(&f.project, f.b), vec![10.0]);
    assert_eq!(texts(&f.project, f.t), vec![Some("a".to_string())]);

    // Après coup, aucune colonne n'a plus de ligne 1 : `insert_rows` au
    // niveau feuille ne les atteindrait pas. L'annulation doit donc opérer
    // colonne par colonne, avec la liste retenue au moment de la suppression.
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

// -- 12b. DeleteCells -------------------------------------------------------

#[test]
fn delete_cells_removes_independently_per_column_ignoring_duplicates() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    // `f.a` en double : ignoré (une seule suppression par colonne). `f.b`
    // (longueur 2) ne peut perdre qu'une seule ligne à partir de `at: 1`,
    // même si `count: 5` en demande davantage — chaque colonne s'arrête à
    // sa propre fin, indépendamment des autres. `f.t` n'est pas listée :
    // intacte.
    history
        .execute(
            &mut f.project,
            Command::DeleteCells {
                columns: vec![f.a, f.b, f.a],
                at: 1,
                count: 5,
            },
        )
        .expect("suppression de cellules");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::DeleteCells { count: 5 })
    );
    assert_eq!(floats(&f.project, f.a), vec![1.0]);
    assert_eq!(floats(&f.project, f.b), vec![10.0]);
    assert_eq!(
        texts(&f.project, f.t),
        vec![Some("a".to_string()), None, Some("c".to_string())]
    );

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn delete_cells_beyond_every_targeted_column_is_not_an_entry() {
    // Même politique que `an_insert_that_changes_nothing_is_not_an_entry`
    // (B5) : `at` au-delà de la fin de toutes les colonnes visées ne change
    // rien, donc n'occupe pas de cran d'annulation.
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::DeleteCells {
                columns: vec![f.a, f.b],
                at: 10,
                count: 1,
            },
        )
        .expect("suppression sans effet");
    assert!(history.is_empty());
    assert_eq!(history.undo_label(), None);
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.undo(&mut f.project), Ok(false));
    assert_eq!(**f.project.tree(), *before);
}

// -- 13-14. macros --------------------------------------------------------

#[test]
fn a_macro_is_a_single_entry() {
    let mut f = fixture();
    let mut history = History::new();

    history.begin_macro(MacroKind::Import).unwrap();
    assert!(history.in_macro());
    assert_eq!(history.macro_depth(), 1);
    for name in ["D", "E"] {
        history
            .execute(
                &mut f.project,
                Command::AddColumn {
                    sheet: f.sheet1,
                    index: None,
                    name: Some(name.to_string()),
                    column: Column::from_f64(&[0.0]),
                },
            )
            .expect("ajout dans la macro");
    }
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[42.0]))
        .expect("saisie dans la macro");
    assert_eq!(
        history.len(),
        0,
        "rien n'est empilé tant que la macro est ouverte"
    );

    history.end_macro(&mut f.project).expect("macro fermée");
    assert!(!history.in_macro());
    assert_eq!(history.len(), 1);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::Macro(MacroKind::Import))
    );

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
    assert_eq!(floats(&f.project, f.a)[0], 1.0);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T", "D", "E"]);
    assert_eq!(floats(&f.project, f.a)[0], 42.0);
}

#[test]
fn nested_macros_collapse_into_one_entry() {
    let mut f = fixture();
    let mut history = History::new();

    history.begin_macro(MacroKind::GraphProperties).unwrap();
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("D".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("ajout D");
    history.begin_macro(MacroKind::Import).unwrap();
    assert_eq!(history.macro_depth(), 2);
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("E".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("ajout E");
    history.end_macro(&mut f.project).expect("macro interne");
    assert_eq!(history.len(), 0);
    history.end_macro(&mut f.project).expect("macro externe");

    assert_eq!(history.len(), 1);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::Macro(MacroKind::GraphProperties))
    );
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
}

// -- 15. macros mixtes : le test qui fait tenir l'hybride -----------------

#[test]
fn a_macro_mixing_structure_then_data_undoes_in_reverse_order() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(
            &mut f.project,
            Command::Rename {
                id: f.a,
                name: "Z".to_string(),
            },
        )
        .expect("renommage");
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[7.0]))
        .expect("saisie");
    history.end_macro(&mut f.project).expect("macro fermée");

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["Z", "B", "T"]);
    assert_eq!(floats(&f.project, f.a)[0], 7.0);
}

#[test]
fn a_macro_mixing_data_then_structure_undoes_in_reverse_order() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[7.0]))
        .expect("saisie");
    history
        .execute(
            &mut f.project,
            Command::Rename {
                id: f.a,
                name: "Z".to_string(),
            },
        )
        .expect("renommage");
    history.end_macro(&mut f.project).expect("macro fermée");

    // Pourquoi l'ordre compte : l'entrée structurelle du renommage retient un
    // instantané de l'arbre pris **après** la saisie, donc un arbre où A vaut
    // déjà [7,2,3]. La restaurer d'abord (ordre inverse) ramène cet état
    // intermédiaire, et le delta de données, rejoué ensuite, remet [1,2,3].
    // Dans l'autre sens — delta d'abord, snapshot ensuite — le snapshot
    // écraserait la restauration des données et A resterait à [7,2,3] :
    // c'est exactement la faille que l'ordre inverse strict ferme.
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(floats(&f.project, f.a), vec![1.0, 2.0, 3.0]);
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["Z", "B", "T"]);
    assert_eq!(floats(&f.project, f.a)[0], 7.0);
}

#[test]
fn a_macro_deleting_then_recreating_a_homonym_restores_the_original_node() {
    let mut f = fixture();
    let mut history = History::new();

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(&mut f.project, Command::DeleteColumn { column: f.a })
        .expect("suppression de A");
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: Some(0),
                name: Some("A".to_string()),
                column: Column::from_f64(&[100.0]),
            },
        )
        .expect("nouvelle colonne homonyme");
    history.end_macro(&mut f.project).expect("macro fermée");

    let new_a = f
        .project
        .tree()
        .child_by_name(f.sheet1, "A")
        .expect("feuille valide")
        .expect("colonne A présente");
    assert_ne!(new_a, f.a, "le nouvel homonyme est un autre nœud");

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert!(f.project.tree().contains(f.a), "l'ancien nœud A est revenu");
    assert!(
        !f.project.tree().contains(new_a),
        "le nouvel homonyme a disparu"
    );
    assert_eq!(floats(&f.project, f.a), vec![1.0, 2.0, 3.0]);
}

// -- 16. abort_macro ------------------------------------------------------

#[test]
fn abort_macro_undoes_everything_it_applied() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("Z".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("ajout hors macro");

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("D".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("ajout dans la macro");
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[5.0]))
        .expect("saisie dans la macro");
    history
        .abort_macro(&mut f.project)
        .expect("macro abandonnée");

    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T", "Z"]);
    assert_eq!(floats(&f.project, f.a)[0], 1.0);
    assert_eq!(history.len(), 1);
    assert!(!history.can_redo());
    assert!(!history.in_macro());
}

#[test]
fn aborting_a_nested_macro_leaves_the_parent_alone() {
    let mut f = fixture();
    let mut history = History::new();

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("D".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("ajout D");
    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("E".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("ajout E");
    history
        .abort_macro(&mut f.project)
        .expect("macro interne abandonnée");

    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T", "D"]);
    assert!(history.in_macro());
    assert_eq!(history.macro_depth(), 1);

    history.end_macro(&mut f.project).expect("macro parente");
    assert_eq!(history.len(), 1);
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
}

// -- 17. refus liés aux macros -------------------------------------------

#[test]
fn macro_api_refusals() {
    let mut f = fixture();
    let mut history = History::new();

    assert_eq!(
        history.end_macro(&mut f.project),
        Err(CommandError::NotInMacro)
    );
    assert_eq!(
        history.abort_macro(&mut f.project),
        Err(CommandError::NotInMacro)
    );

    history.begin_macro(MacroKind::Import).unwrap();
    assert_eq!(
        history.undo(&mut f.project),
        Err(CommandError::MacroOpen("undo"))
    );
    assert_eq!(
        history.redo(&mut f.project),
        Err(CommandError::MacroOpen("redo"))
    );
    assert_eq!(
        history.set_limit(10),
        Err(CommandError::MacroOpen("set_limit"))
    );
    history
        .end_macro(&mut f.project)
        .expect("macro vide fermée");
}

#[test]
fn an_empty_macro_leaves_no_entry() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[1.5]))
        .expect("saisie");
    let entries = history.len();

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .end_macro(&mut f.project)
        .expect("macro vide fermée");
    assert_eq!(history.len(), entries);
}

// -- 18. échec dans une macro --------------------------------------------

#[test]
fn a_failing_command_inside_a_macro_leaves_nothing_behind() {
    let mut f = fixture();
    let mut history = History::new();

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("D".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("ajout D");
    let d = f
        .project
        .tree()
        .child_by_name(f.sheet1, "D")
        .expect("feuille valide")
        .expect("colonne D");

    let err = history
        .execute(
            &mut f.project,
            Command::Rename {
                id: d,
                name: "A".to_string(),
            },
        )
        .expect_err("nom déjà pris");
    assert!(matches!(
        err,
        CommandError::Core(CoreError::DuplicateName(_))
    ));

    // La macro reste ouverte, avec le seul ajout réussi.
    assert!(history.in_macro());
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T", "D"]);

    history.end_macro(&mut f.project).expect("macro fermée");
    assert_eq!(history.len(), 1);
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(names(&f.project, f.sheet1), vec!["A", "B", "T"]);
}

// -- 19-20. limite et libération -----------------------------------------

#[test]
fn the_oldest_entries_are_evicted_silently() {
    let mut f = fixture();
    let mut history = History::with_limit(3);

    for step in 1..=5 {
        history
            .execute(
                &mut f.project,
                set_cells(f.a, 0, &[100.0 + f64::from(step)]),
            )
            .expect("saisie");
    }
    assert_eq!(history.len(), 3);

    for _ in 0..3 {
        assert_eq!(history.undo(&mut f.project), Ok(true));
    }
    assert_eq!(history.undo(&mut f.project), Ok(false));
    // Les deux premières commandes ne sont plus annulables : on s'arrête à
    // l'état qu'elles ont laissé.
    assert_eq!(floats(&f.project, f.a)[0], 102.0);
}

#[test]
fn set_limit_requires_an_empty_stack() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[1.0]))
        .expect("saisie");
    assert_eq!(history.set_limit(5), Err(CommandError::StackNotEmpty));

    assert_eq!(history.undo(&mut f.project), Ok(true));
    // La pile de rétablissement compte aussi.
    assert_eq!(history.set_limit(5), Err(CommandError::StackNotEmpty));

    history.clear().expect("purge");
    history.set_limit(5).expect("limite acceptée");
    assert_eq!(history.limit(), 5);
}

#[test]
fn a_zero_limit_means_unlimited() {
    let mut f = fixture();
    let mut history = History::with_limit(0);

    for step in 0..200 {
        history
            .execute(&mut f.project, set_cells(f.a, 0, &[f64::from(step)]))
            .expect("saisie");
    }
    assert_eq!(history.len(), 200);
}

#[test]
fn eviction_releases_the_nodes_only_the_history_retained() {
    let mut f = fixture();
    let mut history = History::with_limit(1);

    // Référence faible prise sans conserver le moindre `Arc<Tree>` de test :
    // seul l'historique retiendra l'ancien nœud après la suppression.
    let weak = Arc::downgrade(
        f.project
            .tree()
            .node_arc(f.a)
            .expect("nœud A présent avant suppression"),
    );

    history
        .execute(&mut f.project, Command::DeleteColumn { column: f.a })
        .expect("suppression de A");
    assert!(
        weak.upgrade().is_some(),
        "tant que l'entrée est dans la pile, l'ancien arbre retient A"
    );

    history
        .execute(&mut f.project, set_cells(f.b, 0, &[0.0]))
        .expect("saisie");
    assert_eq!(history.len(), 1, "l'entrée la plus ancienne a été évincée");
    assert!(
        weak.upgrade().is_none(),
        "l'éviction relâche l'arbre d'avant, donc le nœud supprimé"
    );
}

// -- 21. rétablissement partiel ------------------------------------------

#[test]
fn partial_redo_then_a_new_command_cuts_the_branch() {
    let mut f = fixture();
    let mut history = History::with_limit(0);

    for step in 1..=5 {
        history
            .execute(
                &mut f.project,
                set_cells(f.a, 0, &[100.0 + f64::from(step)]),
            )
            .expect("saisie");
    }
    for _ in 0..3 {
        assert_eq!(history.undo(&mut f.project), Ok(true));
    }
    assert_eq!(floats(&f.project, f.a)[0], 102.0);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(floats(&f.project, f.a)[0], 103.0);
    assert_eq!(history.len(), 3);
    assert_eq!(history.redo_len(), 2);

    history
        .execute(&mut f.project, set_cells(f.x, 0, &[9.0]))
        .expect("saisie sur X");
    assert_eq!(history.redo_len(), 0);
    assert!(!history.can_redo());
}

// -- 22. abort_macro restaure le redo d'avant la macro ---------------------
//
// Décision : une macro avortée ne laisse aucune trace, y compris sur le
// redo. À l'ouverture de la macro la plus externe, la pile de
// rétablissement courante est mise de côté ; elle revient telle quelle si
// cette macro est avortée (ou close vide), et n'est jetée que si elle
// aboutit avec au moins une entrée — comme pour toute commande.

#[test]
fn abort_macro_restores_the_redo_stack_that_predates_it() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[1.0]))
        .expect("premiere saisie");
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[2.0]))
        .expect("deuxieme saisie");
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert!(history.can_redo());
    let before_macro = snapshot(&f.project);

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[9.0]))
        .expect("saisie dans la macro");
    history
        .abort_macro(&mut f.project)
        .expect("macro abandonnee");

    // La macro a rendu quelque chose de rétablissable en cours de route
    // (chaque `execute` vide le redo, même en macro) : sitôt avortée, c'est
    // le redo d'avant la macro qui doit être revenu, pas un redo vide.
    assert!(history.can_redo());
    assert_eq!(history.redo_len(), 1);
    assert_eq!(**f.project.tree(), *before_macro);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(floats(&f.project, f.a)[0], 2.0);
}

#[test]
fn a_successful_macro_drops_the_redo_stack_like_any_command() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[1.0]))
        .expect("premiere saisie");
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[2.0]))
        .expect("deuxieme saisie");
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert!(history.can_redo());

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[9.0]))
        .expect("saisie dans la macro");
    history.end_macro(&mut f.project).expect("macro fermee");

    // La macro a abouti avec une entrée : le redo est perdu, comme pour
    // toute commande qui réussit.
    assert!(!history.can_redo());
}

#[test]
fn an_empty_macro_restores_the_redo_stack_on_close() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[1.0]))
        .expect("premiere saisie");
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[2.0]))
        .expect("deuxieme saisie");
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert!(history.can_redo());
    let len_before = history.len();

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .end_macro(&mut f.project)
        .expect("macro vide fermee");

    // Une macro vide n'a rien changé : le redo d'avant revient.
    assert!(history.can_redo());
    assert_eq!(history.len(), len_before);
}

#[test]
fn aborting_the_outer_macro_of_a_nested_pair_restores_redo() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[1.0]))
        .expect("premiere saisie");
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[2.0]))
        .expect("deuxieme saisie");
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert!(history.can_redo());
    let before_macro = snapshot(&f.project);

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("D".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("ajout D");

    history.begin_macro(MacroKind::Import).unwrap();
    history
        .execute(
            &mut f.project,
            Command::AddColumn {
                sheet: f.sheet1,
                index: None,
                name: Some("E".to_string()),
                column: Column::from_f64(&[0.0]),
            },
        )
        .expect("ajout E");
    history
        .abort_macro(&mut f.project)
        .expect("macro interne abandonnee");

    // La macro interne avortée ne touche pas à ce que l'externe a déjà
    // appliqué, ni à la sauvegarde du redo (qui appartient à l'externe).
    let current_names = names(&f.project, f.sheet1);
    assert!(current_names.contains(&"D".to_string()));
    assert!(!current_names.contains(&"E".to_string()));
    assert!(history.in_macro());

    history
        .abort_macro(&mut f.project)
        .expect("macro externe abandonnee");

    let current_names = names(&f.project, f.sheet1);
    assert!(!current_names.contains(&"D".to_string()));
    assert!(!history.in_macro());
    assert!(history.can_redo());
    assert_eq!(**f.project.tree(), *before_macro);
}

// -- 23-29. seconde limite : octets de charge utile retenue ---------------
//
// `History` borne désormais l'historique par deux limites, la plus
// contraignante des deux gagnant : le nombre d'entrées (déjà couvert
// ci-dessus) et les octets de charge utile que l'historique est seul à
// maintenir vivants. Ces tests-ci n'exercent que l'API publique, sur un
// projet léger (fixture) : la preuve à l'échelle (un million de flottants)
// vit dans `tests/memory_pinning.rs`.

#[test]
fn new_history_has_both_default_limits_and_starts_at_zero_retained_bytes() {
    let history = History::new();
    assert_eq!(history.limit(), DEFAULT_LIMIT);
    assert_eq!(history.byte_limit(), DEFAULT_BYTE_LIMIT);
    assert_eq!(history.retained_bytes(), 0);
}

#[test]
fn with_limit_keeps_the_default_byte_limit() {
    let history = History::with_limit(3);
    assert_eq!(history.limit(), 3);
    assert_eq!(history.byte_limit(), DEFAULT_BYTE_LIMIT);
}

#[test]
fn with_limits_zero_zero_is_fully_unbounded() {
    let mut f = fixture();
    let mut history = History::with_limits(0, 0);
    assert_eq!(history.limit(), 0);
    assert_eq!(history.byte_limit(), 0);

    for step in 0..200 {
        history
            .execute(&mut f.project, set_cells(f.a, 0, &[f64::from(step)]))
            .expect("saisie");
    }
    assert_eq!(history.len(), 200);
}

#[test]
fn retained_bytes_grows_with_structure_then_write_and_falls_on_eviction() {
    let mut f = fixture();
    // Limite en entrées à 2 : la troisième commande évince la première (le
    // renommage), ce qui doit faire retomber les octets retenus.
    let mut history = History::with_limit(2);
    assert_eq!(history.retained_bytes(), 0);

    // Renommer seul ne change rien : l'arbre `before` et l'arbre `after`
    // référencent encore exactement les buffers de l'arbre courant.
    history
        .execute(
            &mut f.project,
            Command::Rename {
                id: f.a,
                name: "A2".to_string(),
            },
        )
        .expect("renommage");
    let after_rename = history.retained_bytes();

    // L'écriture qui suit force une copie-à-l'écriture du buffer de A : la
    // version que le renommage avait capturée dans son arbre `after`
    // (partagé avec l'arbre `before`, puisque rien n'avait encore changé)
    // n'est plus celle de l'arbre courant. Elle devient un coût pur de
    // l'historique.
    history
        .execute(&mut f.project, set_cells(f.a, 0, &[42.0]))
        .expect("saisie");
    let after_write = history.retained_bytes();
    assert!(
        after_write > after_rename,
        "l'écriture qui suit le renommage doit faire croître les octets retenus : \
         {after_rename} -> {after_write}"
    );

    // Une troisième commande évince le renommage (limite = 2) : le buffer
    // qu'il épinglait n'est plus référencé par aucune entrée de la pile.
    history
        .execute(&mut f.project, set_cells(f.b, 0, &[0.0]))
        .expect("saisie sur B");
    let after_eviction = history.retained_bytes();
    assert!(
        after_eviction < after_write,
        "évincer l'entrée structurelle qui épinglait le buffer doit faire retomber \
         les octets retenus : {after_write} -> {after_eviction}"
    );
}

#[test]
fn a_lone_data_command_retains_only_its_delta() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[42.0]))
        .expect("saisie");
    let retained = history.retained_bytes();

    // Aucune opération de structure autour : pas de buffer entier épinglé,
    // seulement le delta (quelques `f64`), quelques dizaines d'octets tout
    // au plus — très loin de la taille d'un buffer de colonne complet.
    assert!(
        retained < 1024,
        "une commande de données seule ne devrait retenir qu'un delta minuscule, \
         pas {retained} octets"
    );
}

#[test]
fn set_byte_limit_requires_an_empty_stack_and_no_open_macro() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[1.0]))
        .expect("saisie");
    assert_eq!(
        history.set_byte_limit(1024),
        Err(CommandError::StackNotEmpty)
    );

    assert_eq!(history.undo(&mut f.project), Ok(true));
    // La pile de rétablissement compte aussi.
    assert_eq!(
        history.set_byte_limit(1024),
        Err(CommandError::StackNotEmpty)
    );

    history.clear().expect("purge");

    history.begin_macro(MacroKind::Import).unwrap();
    assert_eq!(
        history.set_byte_limit(1024),
        Err(CommandError::MacroOpen("set_byte_limit"))
    );
    history
        .end_macro(&mut f.project)
        .expect("macro vide fermée");

    history.set_byte_limit(2048).expect("limite acceptée");
    assert_eq!(history.byte_limit(), 2048);
}

#[test]
fn the_last_entry_is_never_evicted_even_over_the_byte_limit() {
    let mut f = fixture();
    // `0` : illimité en nombre d'entrées. `1` octet : n'importe quelle
    // entrée le dépasse à elle seule.
    let mut history = History::with_limits(0, 1);

    history
        .execute(&mut f.project, set_cells(f.a, 0, &[42.0]))
        .expect("saisie");

    assert_eq!(history.len(), 1);
    assert!(
        history.retained_bytes() > history.byte_limit(),
        "le scénario suppose que l'unique entrée dépasse à elle seule la limite"
    );
    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(floats(&f.project, f.a), vec![1.0, 2.0, 3.0]);
}

#[test]
fn an_absurdly_low_byte_limit_still_settles_on_exactly_one_entry() {
    let mut f = fixture();
    let mut history = History::with_limits(0, 1);

    for step in 0..5 {
        history
            .execute(&mut f.project, set_cells(f.a, 0, &[f64::from(step)]))
            .expect("saisie");
        assert!(
            !history.is_empty(),
            "la dernière entrée ne doit jamais être évincée"
        );
    }
    assert_eq!(
        history.len(),
        1,
        "la limite en octets, ici absurde, ramène la pile à une seule entrée"
    );
}

// -- création/suppression de nœud ------------------------------------------

#[test]
fn add_folder_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let root = f.project.root();

    history
        .execute(
            &mut f.project,
            Command::AddFolder {
                parent: root,
                name: "Manips".to_string(),
            },
        )
        .expect("ajout de dossier");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::AddFolder {
            name: "Manips".to_string()
        })
    );
    assert!(names(&f.project, root).contains(&"Manips".to_string()));

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn add_workbook_round_trip_creates_a_default_sheet() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let root = f.project.root();

    history
        .execute(
            &mut f.project,
            Command::AddWorkbook {
                parent: root,
                name: Some("Book2".to_string()),
            },
        )
        .expect("ajout de classeur");
    let after = snapshot(&f.project);
    let sheet = f
        .project
        .resolve_path("[Book2]Sheet1")
        .expect("feuille par défaut créée avec le classeur");
    assert!(f.project.tree().contains(sheet));

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn add_empty_workbook_round_trip_has_no_sheet() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let root = f.project.root();

    history
        .execute(
            &mut f.project,
            Command::AddEmptyWorkbook {
                parent: root,
                name: Some("Empty".to_string()),
            },
        )
        .expect("ajout de classeur vide");
    let after = snapshot(&f.project);
    let book = f.project.resolve_path("[Empty]").expect("classeur vide");
    assert!(f
        .project
        .tree()
        .children(book)
        .expect("classeur")
        .is_empty());

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn add_worksheet_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::AddWorksheet {
                book: f.book,
                name: None,
            },
        )
        .expect("ajout de feuille");
    let after = snapshot(&f.project);
    // D1 — deux feuilles déjà présentes (Sheet1, Sheet2) : le premier
    // candidat essayé est « Sheet3 ».
    let sheet3 = f
        .project
        .resolve_path("[Book1]Sheet3")
        .expect("nom automatique Sheet3");
    assert!(f.project.tree().contains(sheet3));

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn add_graph_round_trip_carries_its_spec() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let root = f.project.root();
    let spec = PlotSpec::empty("Courbe");

    history
        .execute(
            &mut f.project,
            Command::AddGraph {
                parent: root,
                name: Some("Fig1".to_string()),
                spec: spec.clone(),
            },
        )
        .expect("ajout de graphe");
    let after = snapshot(&f.project);
    let graph = f.project.resolve_path("[Fig1]").expect("graphe");
    match f.project.tree().get(graph).expect("nœud").kind() {
        NodeKind::Graph { spec: found, .. } => assert_eq!(found, &spec),
        other => panic!("attendu un graphe, trouvé {other:?}"),
    }

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn add_matrix_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let root = f.project.root();

    history
        .execute(
            &mut f.project,
            Command::AddMatrix {
                parent: root,
                name: Some("Mat1".to_string()),
            },
        )
        .expect("ajout de matrice");
    let after = snapshot(&f.project);
    assert!(f.project.resolve_path("[Mat1]").is_ok());

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn add_note_round_trip_carries_its_text() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let root = f.project.root();

    history
        .execute(
            &mut f.project,
            Command::AddNote {
                parent: root,
                name: Some("N1".to_string()),
                text: "Bonjour".to_string(),
            },
        )
        .expect("ajout de note");
    let after = snapshot(&f.project);
    let note = f.project.resolve_path("[N1]").expect("note");
    match f.project.tree().get(note).expect("nœud").kind() {
        NodeKind::Note { text, .. } => assert_eq!(text, "Bonjour"),
        other => panic!("attendu une note, trouvé {other:?}"),
    }

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn remove_node_round_trip_restores_the_same_node_id_and_its_descendants() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(&mut f.project, Command::RemoveNode { id: f.sheet2 })
        .expect("suppression de Sheet2");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::RemoveNode {
            name: Some("Sheet2".to_string())
        })
    );
    assert!(!f.project.tree().contains(f.sheet2));
    assert!(!f.project.tree().contains(f.x));

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert!(f.project.tree().contains(f.sheet2));
    assert!(f.project.tree().contains(f.x));
    assert_eq!(floats(&f.project, f.x), vec![0.5, 1.5]);
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert!(!f.project.tree().contains(f.sheet2));
    assert_eq!(**f.project.tree(), *after);
}

// -- édition de contenu : fenêtre, note, colonne, graphe --------------------

#[test]
fn set_window_meta_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let meta = WindowMeta {
        label: "Manip 3".to_string(),
        comment: "essai".to_string(),
    };

    history
        .execute(
            &mut f.project,
            Command::SetWindowMeta {
                id: f.book,
                meta: meta.clone(),
            },
        )
        .expect("métadonnées de fenêtre");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::SetWindowMeta {
            name: Some("Book1".to_string())
        })
    );
    match f.project.tree().get(f.book).expect("nœud").kind() {
        NodeKind::Workbook { meta: found, .. } => assert_eq!(found, &meta),
        other => panic!("attendu un classeur, trouvé {other:?}"),
    }

    assert_eq!(history.undo(&mut f.project), Ok(true));
    match f.project.tree().get(f.book).expect("nœud").kind() {
        NodeKind::Workbook { meta: found, .. } => assert_eq!(found, &WindowMeta::default()),
        other => panic!("attendu un classeur, trouvé {other:?}"),
    }
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn set_window_meta_refuses_a_node_that_is_not_a_window() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    let err = history
        .execute(
            &mut f.project,
            Command::SetWindowMeta {
                id: f.sheet1,
                meta: WindowMeta::default(),
            },
        )
        .expect_err("une feuille n'est pas une fenêtre");
    assert!(matches!(err, CommandError::Core(CoreError::Structure(_))));
    assert!(history.is_empty());
    assert_eq!(**f.project.tree(), *before);
}

#[test]
fn set_note_text_round_trip() {
    let mut f = fixture();
    let root = f.project.root();
    let note = f.project.add_note(root, Some("N1"), "avant").expect("note");
    let mut history = History::new();
    let before = snapshot(&f.project);

    history
        .execute(
            &mut f.project,
            Command::SetNoteText {
                id: note,
                text: "après".to_string(),
            },
        )
        .expect("texte de note");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::SetNoteText {
            name: Some("N1".to_string())
        })
    );
    match f.project.tree().get(note).expect("nœud").kind() {
        NodeKind::Note { text, .. } => assert_eq!(text, "après"),
        other => panic!("attendu une note, trouvé {other:?}"),
    }

    assert_eq!(history.undo(&mut f.project), Ok(true));
    match f.project.tree().get(note).expect("nœud").kind() {
        NodeKind::Note { text, .. } => assert_eq!(text, "avant"),
        other => panic!("attendu une note, trouvé {other:?}"),
    }
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn set_column_meta_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let meta = ColumnMeta {
        role: ColumnRole::X,
        unit: "s".to_string(),
        ..Default::default()
    };

    history
        .execute(
            &mut f.project,
            Command::SetColumnMeta {
                column: f.a,
                meta: meta.clone(),
            },
        )
        .expect("métadonnées de colonne");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::SetColumnMeta {
            column: Some(ColumnRef(0))
        })
    );
    assert_eq!(
        f.project
            .tree()
            .get(f.a)
            .expect("colonne")
            .as_column()
            .expect("colonne")
            .meta(),
        &meta
    );

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert_eq!(
        f.project
            .tree()
            .get(f.a)
            .expect("colonne")
            .as_column()
            .expect("colonne")
            .meta(),
        &ColumnMeta::default()
    );
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

/// Colonnes calculées (v1) — poser une formule sur une colonne qui portait
/// déjà des valeurs, puis annuler, doit restaurer **ces valeurs**, pas
/// seulement retirer la formule. `stokesforge-commands` ne recalcule jamais
/// lui-même (c'est `stokesforge-session::actor::recalc_formulas`, hors
/// historique — voir `crates/stokesforge-session/tests/formula.rs` pour le test
/// de bout en bout via l'acteur réel) : ce test simule ici, par un
/// `project.set_values` direct après l'exécution de `SetColumnMeta`, l'effet
/// de ce recalcul, pour vérifier que `DataDelta::ColumnMeta::values_before`
/// (capturé par `History::execute` avant que ce recalcul n'ait eu lieu) est
/// bien ce qui permet à `undo` de restaurer les valeurs d'origine — c'était
/// le bug : sans cette capture, `undo` ne retirait que la formule et
/// laissait les valeurs recalculées en place.
#[test]
fn undoing_a_formula_set_on_a_column_with_prior_values_restores_them() {
    let mut f = fixture();
    let mut history = History::new();

    history
        .execute(
            &mut f.project,
            Command::SetColumnMeta {
                column: f.a,
                meta: ColumnMeta {
                    formula: Some("B*2".to_string()),
                    ..Default::default()
                },
            },
        )
        .expect("poser une formule sur A, qui portait déjà [1,2,3]");
    // Simule le recalcul immédiat que `stokesforge-session::actor` ferait ici,
    // hors historique : écrase les valeurs de A par des valeurs qui n'ont
    // rien à voir avec [1,2,3].
    f.project
        .set_values(f.a, 0, ValueBlock::Float(vec![20.0, 40.0]))
        .expect("simule le recalcul (hors historique, comme le ferait l'acteur)");

    assert_eq!(history.undo(&mut f.project), Ok(true));

    let column = f
        .project
        .tree()
        .get(f.a)
        .expect("colonne A")
        .as_column()
        .expect("le nœud est une colonne");
    assert_eq!(column.meta().formula, None, "la formule doit être retirée");
    assert_eq!(
        (0..column.len())
            .map(|row| match column.get(row) {
                Cell::Float(value) => value,
                Cell::Text(_) => unreachable!("A est numérique"),
            })
            .collect::<Vec<_>>(),
        vec![1.0, 2.0, 3.0],
        "les valeurs d'avant la formule doivent être restaurées"
    );
}

/// Colonnes calculées (v1) — `SetCells` direct sur une colonne dont
/// `ColumnMeta.formula` est renseignée est refusé avant toute mutation, et
/// ne pousse rien sur l'historique (ni de nouvelle entrée à annuler, ni de
/// changement observable de l'arbre).
#[test]
fn set_cells_on_a_formula_column_is_refused() {
    let mut f = fixture();
    let mut history = History::new();
    history
        .execute(
            &mut f.project,
            Command::SetColumnMeta {
                column: f.a,
                meta: ColumnMeta {
                    formula: Some("B*2".to_string()),
                    ..Default::default()
                },
            },
        )
        .expect("poser une formule sur A");
    let before = snapshot(&f.project);
    let undo_label_before = history.undo_label().cloned();

    let err = history
        .execute(
            &mut f.project,
            Command::SetCells {
                column: f.a,
                start: 0,
                values: ValueBlock::Float(vec![99.0]),
                expected_version: None,
            },
        )
        .expect_err("une colonne-formule refuse une écriture directe de cellule");
    assert_eq!(
        err,
        CommandError::Core(CoreError::FormulaColumnReadOnly { column: f.a })
    );

    // Ni le modèle ni l'historique n'ont bougé.
    assert_eq!(**f.project.tree(), *before);
    assert_eq!(history.undo_label(), undo_label_before.as_ref());
}

/// Enregistrer une couleur dans le projet est une commande de données
/// annulable : Annuler rend la liste d'avant, Rétablir celle d'après.
#[test]
fn set_saved_colors_round_trip() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);
    let colors = SavedColors::default().with_added(ColorUsage::Curves, "#ff0000");

    history
        .execute(
            &mut f.project,
            Command::SetSavedColors {
                colors: colors.clone(),
            },
        )
        .expect("couleurs enregistrées");
    assert_eq!(history.undo_label(), Some(&HistoryLabel::SetSavedColors));
    assert_eq!(f.project.tree().saved_colors(), &colors);
    let after = snapshot(&f.project);

    assert_eq!(history.undo(&mut f.project), Ok(true));
    assert!(f.project.tree().saved_colors().is_empty());
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn set_plot_spec_round_trip() {
    let mut f = fixture();
    let root = f.project.root();
    let graph = f
        .project
        .add_graph(root, Some("Fig1"), PlotSpec::default())
        .expect("graphe");
    let mut history = History::new();
    let before = snapshot(&f.project);
    let spec = PlotSpec::empty("Courbe");

    history
        .execute(
            &mut f.project,
            Command::SetPlotSpec {
                graph,
                spec: spec.clone(),
            },
        )
        .expect("spec de graphe");
    let after = snapshot(&f.project);
    assert_eq!(
        history.undo_label(),
        Some(&HistoryLabel::SetPlotSpec {
            name: Some("Fig1".to_string())
        })
    );
    match f.project.tree().get(graph).expect("nœud").kind() {
        NodeKind::Graph { spec: found, .. } => assert_eq!(found, &spec),
        other => panic!("attendu un graphe, trouvé {other:?}"),
    }

    assert_eq!(history.undo(&mut f.project), Ok(true));
    match f.project.tree().get(graph).expect("nœud").kind() {
        NodeKind::Graph { spec: found, .. } => assert_eq!(found, &PlotSpec::default()),
        other => panic!("attendu un graphe, trouvé {other:?}"),
    }
    assert_eq!(**f.project.tree(), *before);

    assert_eq!(history.redo(&mut f.project), Ok(true));
    assert_eq!(**f.project.tree(), *after);
}

#[test]
fn set_plot_spec_refuses_a_node_that_is_not_a_graph() {
    let mut f = fixture();
    let mut history = History::new();
    let before = snapshot(&f.project);

    let err = history
        .execute(
            &mut f.project,
            Command::SetPlotSpec {
                graph: f.book,
                spec: PlotSpec::default(),
            },
        )
        .expect_err("un classeur n'est pas un graphe");
    assert!(matches!(err, CommandError::Core(CoreError::Structure(_))));
    assert!(history.is_empty());
    assert_eq!(**f.project.tree(), *before);
}
