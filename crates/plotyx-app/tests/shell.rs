//! Critères d'acceptation de la coquille UI, pilotés **sans fenêtre**.
//!
//! Tout ce qui compte dans cette phase est vérifié ici : `AppState` ne dépend
//! pas d'egui, donc la boucle complète (modèle → commande → annulation →
//! sauvegarde → rechargement) tourne en CI sans serveur graphique. La couche
//! `eframe` n'est qu'une traduction clavier/souris → [`Action`] et un
//! peintre ; ce qu'elle ajoute n'est pas testable automatiquement et n'est
//! donc pas testé ici (voir le critère 13, qui est un script).
//!
//! Chaque test démarre **sa propre** `Session` sur un projet de démonstration
//! construit par le helper local [`demo_project`] : les tests ne partagent
//! aucun état et ne dépendent pas du projet de démonstration du binaire, qui
//! peut changer sans invalider ces assertions.
//!
//! # Grille scopée par classeur
//!
//! Depuis que la grille est un onglet par classeur (`Tab::Book`, plutôt que
//! le `Tab::Grid` unique et global d'avant cette tâche), `AppState` n'a plus
//! de « la » feuille sélectionnée : [`AppState::visible_columns`] et
//! [`AppState::row_count`] prennent un `NodeId` de feuille explicite, et
//! [`AppState::selected_sheet_in`] rend la feuille choisie *dans un classeur
//! donné*. Les tests ci-dessous résolvent donc le classeur ou la feuille
//! visée par son nom ([`book_named`], [`sheet_named`]) plutôt que de
//! compter sur une sélection implicite.

use std::path::Path;

use plotyx_app::state::{Action, AppState};
use plotyx_core::{Cell, Column, ColumnType, NodeId, Project, Tree};
use plotyx_session::{Session, SessionHandle};

/// Projet de démonstration des tests : un classeur `Book1` avec `Sheet1`
/// (colonnes `Temps`, `Signal`, `Note`, cette dernière **plus courte**) et
/// une seconde feuille `Sheet2` avec une colonne courte. L'hétérogénéité des
/// longueurs est délibérée : c'est le cas que le modèle autorise et que la
/// grille doit savoir peindre.
fn demo_project() -> Project {
    let mut project = Project::new("Demo").expect("projet de démonstration");
    let root = project.root();
    let book = project
        .add_workbook(root, Some("Book1"))
        .expect("classeur Book1");
    let sheet1 = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("Book1 lisible")
        .expect("Sheet1 créée avec le classeur");
    project
        .add_column(
            sheet1,
            None,
            Some("Temps"),
            Column::from_f64(&[0.0, 0.5, 1.0, 1.5, 2.0]),
        )
        .expect("colonne Temps");
    project
        .add_column(
            sheet1,
            None,
            Some("Signal"),
            Column::from_f64(&[10.0, f64::NAN, 30.0, 40.0, 50.0]),
        )
        .expect("colonne Signal");
    project
        .add_column(
            sheet1,
            None,
            Some("Note"),
            Column::from_text(&[Some("a".to_string()), None, Some(String::new())]),
        )
        .expect("colonne Note");
    let sheet2 = project
        .add_worksheet(book, Some("Sheet2"))
        .expect("feuille Sheet2");
    project
        .add_column(sheet2, None, Some("Court"), Column::from_f64(&[7.0, 8.0]))
        .expect("colonne Court");
    project
}

/// Projet minimal ne contenant qu'**une** feuille : sert à produire un
/// fichier dont les identifiants de nœud ne recouvrent pas ceux de
/// [`demo_project`] (critère 8).
fn single_sheet_project() -> Project {
    let mut project = Project::new("Autre").expect("projet Autre");
    let root = project.root();
    let book = project
        .add_workbook(root, Some("Book1"))
        .expect("classeur Book1");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("Book1 lisible")
        .expect("Sheet1 créée avec le classeur");
    project
        .add_column(sheet, None, Some("Unique"), Column::from_f64(&[1.0, 2.0]))
        .expect("colonne Unique");
    project
}

/// Démarre une session sur `project` et rend l'état applicatif associé, ainsi
/// que la session (à garder vivante : la lâcher termine l'acteur).
fn start(project: Project, path: &Path) -> (Session, AppState) {
    let session = Session::spawn(project, None).expect("thread de session");
    let state = AppState::new(session.handle(), path.display().to_string())
        .expect("état applicatif initial");
    (session, state)
}

/// Session sur le projet de démonstration, fichier de travail dans `dir`.
fn start_demo(dir: &Path) -> (Session, AppState) {
    start(demo_project(), &dir.join("projet.plotyx"))
}

/// Identifiant de la colonne `name` dans `sheet`.
fn column_id(state: &AppState, sheet: NodeId, name: &str) -> NodeId {
    state
        .visible_columns(sheet)
        .into_iter()
        .find(|(_, column_name, _)| column_name == name)
        .map(|(id, _, _)| id)
        .unwrap_or_else(|| panic!("colonne {name} visible dans {sheet}"))
}

/// Cellule brute du modèle (au-delà de ce que `cell_text` laisse voir).
fn cell(state: &AppState, column: NodeId, row: usize) -> Cell {
    state
        .tree()
        .get(column)
        .expect("colonne présente")
        .as_column()
        .expect("nœud colonne")
        .get(row)
}

/// Feuille nommée `name` dans l'arbre courant.
fn sheet_named(tree: &Tree, name: &str) -> NodeId {
    tree.walk_all()
        .into_iter()
        .find(|id| {
            tree.node(*id)
                .is_some_and(|node| node.kind().is_worksheet() && node.name() == name)
        })
        .unwrap_or_else(|| panic!("feuille {name} présente"))
}

/// Classeur nommé `name` dans l'arbre courant. Pendant de [`sheet_named`],
/// nécessaire depuis que la sélection de feuille est scopée par classeur
/// ([`AppState::selected_sheet_in`]).
fn book_named(tree: &Tree, name: &str) -> NodeId {
    tree.walk_all()
        .into_iter()
        .find(|id| {
            tree.node(*id).is_some_and(|node| {
                matches!(node.kind(), plotyx_core::node::NodeKind::Workbook { .. })
                    && node.name() == name
            })
        })
        .unwrap_or_else(|| panic!("classeur {name} présent"))
}

/// Poignée de session inutilisée hors construction : silence le lint sur les
/// tests qui n'en ont pas besoin.
#[allow(dead_code)]
fn unused(_: &SessionHandle) {}

// -- 1. État initial --

#[test]
fn a_books_grid_defaults_to_its_first_sheet_and_describes_it() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, state) = start_demo(dir.path());

    let book1 = book_named(state.tree(), "Book1");
    let sheet = state
        .selected_sheet_in(book1)
        .expect("une feuille par défaut");
    assert_eq!(sheet, sheet_named(state.tree(), "Sheet1"));

    let columns = state.visible_columns(sheet);
    let described: Vec<(&str, ColumnType)> = columns
        .iter()
        .map(|(_, name, kind)| (name.as_str(), *kind))
        .collect();
    assert_eq!(
        described,
        vec![
            ("Temps", ColumnType::Numeric),
            ("Signal", ColumnType::Numeric),
            ("Note", ColumnType::Text),
        ]
    );

    // Maximum des longueurs (5, 5, 3), pas la longueur de la première colonne.
    assert_eq!(state.row_count(sheet), 5);

    assert!(!state.can_undo());
    assert!(!state.can_redo());
    assert_eq!(state.undo_label(), None);
    assert_eq!(state.redo_label(), None);
}

// -- 2. Saisie d'une cellule --

#[test]
fn committing_a_numeric_cell_writes_it_and_names_the_undo() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let before = state.snapshot().revision;

    state.dispatch(Action::CommitCell {
        column: temps,
        row: 1,
        text: "42.5".to_string(),
    });

    assert_eq!(cell(&state, temps, 1), Cell::Float(42.5));
    assert_eq!(state.cell_text(temps, 1), "42.5");
    assert!(state.can_undo());
    assert_eq!(
        state.undo_label(),
        Some("Annuler Saisie dans Temps".to_string())
    );
    assert!(state.snapshot().revision > before);
}

// -- 3. Saisie invalide --

#[test]
fn invalid_numeric_input_reports_an_error_and_sends_no_command() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let before = state.snapshot().revision;

    state.dispatch(Action::CommitCell {
        column: temps,
        row: 1,
        text: "abc".to_string(),
    });

    let status = state.status().expect("un message");
    assert!(status.is_error, "une saisie invalide est une erreur");
    assert!(
        status.text.contains("abc"),
        "le message cite la saisie refusée : {}",
        status.text
    );
    assert_eq!(cell(&state, temps, 1), Cell::Float(0.5), "valeur intacte");
    assert!(!state.can_undo(), "aucune commande n'a été envoyée");
    assert_eq!(state.snapshot().revision, before);
}

// -- 4. Valeur manquante --

#[test]
fn blank_text_writes_a_missing_numeric_value() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.dispatch(Action::CommitCell {
        column: temps,
        row: 2,
        text: "   ".to_string(),
    });

    match cell(&state, temps, 2) {
        Cell::Float(value) => assert!(value.is_nan(), "valeur manquante = NaN"),
        other => panic!("cellule numérique attendue, reçu {other:?}"),
    }
    assert_eq!(state.cell_text(temps, 2), "");
    assert!(
        state.status().is_none_or(|status| !status.is_error),
        "écrire une valeur manquante n'est pas une erreur"
    );
}

#[test]
fn empty_text_writes_a_missing_text_value_and_undo_restores_it() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");

    state.dispatch(Action::CommitCell {
        column: note,
        row: 0,
        text: "a".to_string(),
    });
    assert_eq!(cell(&state, note, 0), Cell::Text(Some("a".to_string())));

    state.dispatch(Action::CommitCell {
        column: note,
        row: 0,
        text: String::new(),
    });
    assert_eq!(
        cell(&state, note, 0),
        Cell::Text(None),
        "manquante, pas \"\""
    );
    // A9 (option 2, `guide-de-correction-robustesse.md` §F1/§F8) : une valeur
    // absente ne s'affiche plus comme une chaîne vide (qui représenterait une
    // chaîne vide *présente*) mais selon la convention de `cell_text` — un
    // espace insécable, ici comparé par la propriété plutôt que par sa valeur
    // littérale pour ne pas dupliquer la constante hors de `plotyx-app`.
    assert_ne!(
        state.cell_text(note, 0),
        "",
        "une valeur absente ne s'affiche plus comme une chaîne vide présente"
    );

    state.dispatch(Action::Undo);
    assert_eq!(cell(&state, note, 0), Cell::Text(Some("a".to_string())));
    assert_eq!(state.cell_text(note, 0), "a");
}

// -- 5. Annulation / rétablissement --

#[test]
fn undo_and_redo_walk_the_history_both_ways() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.dispatch(Action::CommitCell {
        column: temps,
        row: 1,
        text: "42.5".to_string(),
    });

    state.dispatch(Action::Undo);
    assert_eq!(cell(&state, temps, 1), Cell::Float(0.5), "valeur d'origine");
    assert!(state.can_redo());
    assert_eq!(
        state.redo_label(),
        Some("Rétablir Saisie dans Temps".to_string())
    );

    state.dispatch(Action::Redo);
    assert_eq!(cell(&state, temps, 1), Cell::Float(42.5));
}

// -- 6. Annuler à vide --

#[test]
fn undo_on_an_empty_history_is_a_notice_not_an_error() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let before = state.snapshot().revision;

    state.dispatch(Action::Undo);

    let status = state.status().expect("un message");
    assert!(
        !status.is_error,
        "« rien à annuler » est le pendant du bouton grisé, pas une erreur"
    );
    assert_eq!(state.snapshot().revision, before);

    state.dispatch(Action::Redo);
    let status = state.status().expect("un message");
    assert!(!status.is_error);
    assert_eq!(state.snapshot().revision, before);
}

// -- 7. La boucle complète — critère de sortie de phase --

#[test]
fn the_full_loop_returns_the_saved_tree_bit_for_bit() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.dispatch(Action::CommitCell {
        column: temps,
        row: 1,
        text: "42.5".to_string(),
    });
    let saved_tree = std::sync::Arc::clone(state.tree());

    state.dispatch(Action::Save);
    assert!(
        state.status().is_some_and(|status| !status.is_error),
        "sauvegarde réussie : {:?}",
        state.status()
    );

    state.dispatch(Action::CommitCell {
        column: temps,
        row: 1,
        text: "99.0".to_string(),
    });
    assert_eq!(cell(&state, temps, 1), Cell::Float(99.0));

    state.dispatch(Action::Load);
    assert!(
        state.status().is_some_and(|status| !status.is_error),
        "chargement réussi : {:?}",
        state.status()
    );

    assert_eq!(
        **state.tree(),
        *saved_tree,
        "l'arbre rechargé est identique à celui d'après la première saisie"
    );
    assert!(!state.can_undo(), "le chargement vide l'historique");
    assert!(!state.can_redo());

    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    assert_eq!(state.cell_text(temps, 1), "42.5");
}

// -- 8. Rechargement et sélection périmée --

#[test]
fn loading_a_foreign_file_leaves_no_stale_per_book_selection() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let mine = dir.path().join("mien.plotyx");
    let other = dir.path().join("autre.plotyx");

    // Le fichier étranger est écrit par une session distincte.
    {
        let (other_session, mut other_state) = start(single_sheet_project(), &other);
        other_state.dispatch(Action::Save);
        assert!(other_state.status().is_some_and(|s| !s.is_error));
        other_session.shutdown().expect("arrêt de la session");
    }

    let (_session, mut state) = start_demo(&mine);
    let book1 = book_named(state.tree(), "Book1");
    let sheet2 = sheet_named(state.tree(), "Sheet2");
    state.dispatch(Action::SelectSheet(sheet2));
    assert_eq!(state.selected_sheet_in(book1), Some(sheet2));
    state.dispatch(Action::Save);

    state.set_path(other.display().to_string());
    state.dispatch(Action::Load);

    assert!(
        !state.tree().contains(sheet2),
        "le fichier étranger ne porte pas l'identifiant de Sheet2 \
         (sinon le critère ne teste rien) — `book1`, créé plus tôt dans les \
         deux projets, peut en revanche coïncider : ce n'est pas ce que ce \
         critère vérifie"
    );

    // Auto-cicatrisant : un classeur du nouvel arbre, jamais explicitement
    // sélectionné, retombe de lui-même sur sa première feuille — voir la doc
    // de `AppState::selected_sheet_in`.
    let book = book_named(state.tree(), "Book1");
    let sheet = state
        .selected_sheet_in(book)
        .expect("une feuille par défaut dans le classeur du nouveau fichier");
    assert!(state
        .tree()
        .node(sheet)
        .is_some_and(|node| node.kind().is_worksheet()));
    let names: Vec<String> = state
        .visible_columns(sheet)
        .into_iter()
        .map(|(_, name, _)| name)
        .collect();
    assert_eq!(names, vec!["Unique".to_string()]);
}

// -- 9. Chemin invalide --

#[test]
fn a_failing_load_leaves_the_model_and_history_untouched() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    state.dispatch(Action::CommitCell {
        column: temps,
        row: 0,
        text: "3.0".to_string(),
    });
    let revision = state.snapshot().revision;
    let tree = std::sync::Arc::clone(state.tree());

    state.set_path(dir.path().join("inexistant.plotyx").display().to_string());
    state.dispatch(Action::Load);

    assert!(state.status().is_some_and(|status| status.is_error));
    assert_eq!(state.snapshot().revision, revision);
    assert_eq!(**state.tree(), *tree);
    assert!(state.can_undo(), "l'historique est préservé");
}

#[test]
fn a_failing_save_reports_an_error_and_writes_nothing() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let target = dir.path().join("absent").join("projet.plotyx");

    state.set_path(target.display().to_string());
    state.dispatch(Action::Save);

    assert!(state.status().is_some_and(|status| status.is_error));
    assert!(!target.exists(), "rien n'a été écrit");
    assert!(!target.parent().is_some_and(Path::exists));
}

// -- 10. Colonnes de longueurs différentes --

#[test]
fn reading_past_the_end_of_a_short_column_yields_a_missing_value() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");

    assert_eq!(state.row_count(sheet1), 5);
    // `Note` = [Some("a"), None, Some("")] : la ligne 2 est une chaîne vide
    // *présente*, les lignes 3 et au-delà sont hors bornes — `Column::get`
    // y rend `Cell::Text(None)`, la même valeur *absente* qu'une ligne 1
    // écrite explicitement à `None`. A9 (option 2) : ces deux natures ne
    // s'affichent plus de la même façon (avant ce correctif, les quatre
    // assertions ci-dessous rendaient toutes `""`).
    assert_eq!(state.cell_text(note, 2), "", "Some(\"\") s'affiche vide");
    let absente = state.cell_text(note, 3);
    assert_ne!(
        absente,
        state.cell_text(note, 2),
        "hors bornes (valeur absente) doit s'afficher différemment de \
         Some(\"\") (chaîne vide présente)"
    );
    assert_eq!(
        state.cell_text(note, 4),
        absente,
        "toute position hors bornes affiche la même chose"
    );
    assert_eq!(
        state.cell_text(note, 10_000),
        absente,
        "toute position hors bornes affiche la même chose"
    );
}

// -- 11. Lignes et colonnes --

#[test]
fn inserting_then_undoing_rows_restores_the_tree_bit_for_bit() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let before = std::sync::Arc::clone(state.tree());

    state.dispatch(Action::InsertRows {
        sheet: sheet1,
        at: 1,
        count: 2,
    });
    assert_eq!(state.row_count(sheet1), 7);

    state.dispatch(Action::Undo);
    assert_eq!(**state.tree(), *before);
    assert_eq!(state.row_count(sheet1), 5);
}

#[test]
fn deleting_then_undoing_rows_restores_the_tree_bit_for_bit() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let before = std::sync::Arc::clone(state.tree());

    state.dispatch(Action::DeleteRows {
        sheet: sheet1,
        at: 1,
        count: 2,
    });
    assert_eq!(state.row_count(sheet1), 3);

    state.dispatch(Action::Undo);
    assert_eq!(**state.tree(), *before);
    assert_eq!(state.row_count(sheet1), 5);
}

#[test]
fn adding_then_undoing_a_column_restores_the_tree_bit_for_bit() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let before = std::sync::Arc::clone(state.tree());
    let names_before: Vec<String> = state
        .visible_columns(sheet1)
        .into_iter()
        .map(|(_, name, _)| name)
        .collect();

    state.dispatch(Action::AddColumn { sheet: sheet1 });

    let columns = state.visible_columns(sheet1);
    assert_eq!(columns.len(), names_before.len() + 1);
    let (_, new_name, new_type) = columns.last().cloned().expect("la colonne ajoutée");
    assert!(!new_name.is_empty(), "un nom a été généré");
    assert!(!names_before.contains(&new_name), "nom libre");
    assert_eq!(new_type, ColumnType::Numeric);

    state.dispatch(Action::Undo);
    assert_eq!(**state.tree(), *before);
    assert_eq!(state.visible_columns(sheet1).len(), names_before.len());
}

#[test]
fn row_actions_on_something_that_is_not_a_sheet_are_refused() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    // Un nœud qui n'est pas une feuille : un graphe, par exemple.
    let mut project = Project::new("Vide").expect("projet vide");
    let root = project.root();
    let graph = project
        .add_graph(root, Some("Graph1"), Default::default())
        .expect("un graphe");
    let (_session, mut state) = start(project, &dir.path().join("vide.plotyx"));

    assert!(state.visible_columns(graph).is_empty());
    assert_eq!(state.row_count(graph), 0);

    let revision = state.snapshot().revision;
    state.dispatch(Action::InsertRows {
        sheet: graph,
        at: 0,
        count: 1,
    });
    assert!(state.status().is_some_and(|status| status.is_error));
    assert_eq!(state.snapshot().revision, revision);

    state.dispatch(Action::AddColumn { sheet: graph });
    assert!(state.status().is_some_and(|status| status.is_error));
    assert_eq!(state.snapshot().revision, revision);
}

#[test]
fn selecting_something_that_is_not_a_sheet_is_refused() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book1 = book_named(state.tree(), "Book1");
    let selected = state.selected_sheet_in(book1);
    let root = state.tree().root();

    state.dispatch(Action::SelectSheet(root));
    assert!(state.status().is_some_and(|status| status.is_error));
    assert_eq!(
        state.selected_sheet_in(book1),
        selected,
        "sélection inchangée"
    );

    state.dispatch(Action::SelectSheet(NodeId::from_raw(9_999)));
    assert!(state.status().is_some_and(|status| status.is_error));
    assert_eq!(state.selected_sheet_in(book1), selected);
}

// -- 12. Libellés --

#[test]
fn labels_prefix_the_history_entry_and_vanish_when_the_action_is_impossible() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");

    assert_eq!(state.undo_label(), None);
    assert_eq!(state.redo_label(), None);

    state.dispatch(Action::AddColumn { sheet: sheet1 });
    let undo = state.undo_label().expect("un libellé d'annulation");
    assert!(
        undo.starts_with("Annuler "),
        "préfixe attendu, reçu {undo:?}"
    );
    assert_eq!(undo, "Annuler Ajouter une colonne");
    assert_eq!(state.redo_label(), None, "rien à rétablir");

    state.dispatch(Action::Undo);
    let redo = state.redo_label().expect("un libellé de rétablissement");
    assert!(
        redo.starts_with("Rétablir "),
        "préfixe attendu, reçu {redo:?}"
    );
    assert_eq!(redo, "Rétablir Ajouter une colonne");
    assert_eq!(state.undo_label(), None);
}

// -- Divers : l'explorateur latéral --

#[test]
fn sheets_lists_every_worksheet_with_a_readable_path() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, state) = start_demo(dir.path());

    let sheets = state.sheets();
    let paths: Vec<&str> = sheets.iter().map(|(_, path)| path.as_str()).collect();
    assert_eq!(paths, vec!["Book1 / Sheet1", "Book1 / Sheet2"]);
    assert_eq!(sheets[0].0, sheet_named(state.tree(), "Sheet1"));
    assert_eq!(sheets[1].0, sheet_named(state.tree(), "Sheet2"));
}

#[test]
fn selecting_a_sheet_updates_its_books_selection() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book1 = book_named(state.tree(), "Book1");
    let sheet2 = sheet_named(state.tree(), "Sheet2");

    state.dispatch(Action::SelectSheet(sheet2));

    assert_eq!(state.selected_sheet_in(book1), Some(sheet2));
    let names: Vec<String> = state
        .visible_columns(sheet2)
        .into_iter()
        .map(|(_, name, _)| name)
        .collect();
    assert_eq!(names, vec!["Court".to_string()]);
    assert_eq!(state.row_count(sheet2), 2);
}

/// Deux classeurs ouverts simultanément (deux `Tab::Book`) ne partagent pas
/// leur feuille sélectionnée : sélectionner une feuille dans l'un ne doit
/// jamais affecter l'autre — la raison d'être de [`AppState::
/// selected_sheet_in`] scopé par classeur plutôt qu'une sélection globale.
#[test]
fn selecting_a_sheet_in_one_book_does_not_affect_another_book() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let mut project = demo_project();
    let root = project.root();
    let book2 = project
        .add_workbook(root, Some("Book2"))
        .expect("classeur Book2");
    let book2_sheet2 = project
        .add_worksheet(book2, Some("Autre"))
        .expect("feuille Autre");

    let (_session, mut state) = start(project, &dir.path().join("projet.plotyx"));
    let book1 = book_named(state.tree(), "Book1");
    let book1_sheet2 = sheet_named(state.tree(), "Sheet2");

    state.dispatch(Action::SelectSheet(book1_sheet2));
    state.dispatch(Action::SelectSheet(book2_sheet2));

    assert_eq!(state.selected_sheet_in(book1), Some(book1_sheet2));
    assert_eq!(state.selected_sheet_in(book2), Some(book2_sheet2));
}

// -- Divers : le tampon de saisie d'une cellule --

#[test]
fn the_cell_buffer_only_reaches_the_model_on_commit() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let revision = state.snapshot().revision;

    state.begin_edit(temps, 0);
    assert_eq!(
        state.editing().map(|edit| edit.buffer.as_str()),
        Some("0"),
        "le tampon part du texte affiché"
    );
    if let Some(edit) = state.editing_mut() {
        edit.buffer = "7.25".to_string();
    }
    assert_eq!(
        state.snapshot().revision,
        revision,
        "taper ne touche pas le modèle : sinon une entrée d'historique par caractère"
    );

    state.commit_edit();
    assert_eq!(cell(&state, temps, 0), Cell::Float(7.25));
    assert!(state.editing().is_none(), "la saisie est refermée");
    assert!(state.snapshot().revision > revision);
}

#[test]
fn cancelling_an_edit_discards_the_buffer() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let revision = state.snapshot().revision;

    state.begin_edit(temps, 0);
    if let Some(edit) = state.editing_mut() {
        edit.buffer = "7.25".to_string();
    }
    state.cancel_edit();

    assert!(state.editing().is_none());
    assert_eq!(cell(&state, temps, 0), Cell::Float(0.0));
    assert_eq!(state.snapshot().revision, revision);
}

// -- 13. Le script Python — ce qui doit tenir SANS la feature `python` --
//
// Ce qui exige un interpréteur (exécution, sortie, annulation d'une écriture
// de script, démarrage paresseux prouvé à l'exécution) vit dans
// `tests/script_panel.rs`, qui ne compile qu'avec la feature. Ici, la preuve
// que l'interpréteur ne démarre **jamais** dans cette configuration n'est pas
// un test à part : elle est **structurelle**. `Cargo.toml` déclare
// `plotyx-script` et `plotyx-python` comme dépendances optionnelles activées
// par la seule feature `python` de ce crate ; sans elle, ni l'un ni l'autre
// n'est compilé dans le binaire — il n'existe donc littéralement aucun code
// capable de démarrer CPython, quelle que soit la façon dont `AppState` est
// construite ou utilisée. `cargo tree -p plotyx-app` (sans `--features
// python`) le confirme : ni `plotyx-python` ni `plotyx-script` n'y
// apparaissent.
//
// Les deux tests qui suivent supposent la feature ABSENTE (« sans le pont
// Python, RunScript est refusé ») ; ils sont donc `#[cfg(not(feature =
// "python"))]` — sans quoi, compilés avec la feature (`cargo test --features
// python`), `script_available()` serait vrai et ils échoueraient à juste
// titre. Le pendant « avec la feature » de ces critères vit dans
// `tests/script_panel.rs`.

#[cfg(not(feature = "python"))]
#[test]
fn script_available_is_false_and_run_script_explains_why_without_the_feature() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let before = state.snapshot().revision;

    assert!(
        !state.script_available(),
        "le pont Python n'est pas compilé dans cette configuration"
    );
    assert!(state.script_output().is_none(), "rien n'a encore tourné");

    state.dispatch(Action::RunScript);

    let status = state.status().expect("un message explicite");
    assert!(
        status.is_error,
        "impossible d'exécuter un script n'est pas un succès silencieux"
    );
    assert!(
        status.text.to_lowercase().contains("python"),
        "le message explique pourquoi : {}",
        status.text
    );
    assert_eq!(
        state.snapshot().revision,
        before,
        "aucune commande n'a été envoyée à l'acteur"
    );
    assert!(
        !state.can_undo(),
        "rien à annuler : rien n'a été exécuté par le script"
    );
    assert!(
        state.script_output().is_none(),
        "aucune sortie : le script refusé n'a jamais tourné"
    );
}

#[cfg(not(feature = "python"))]
#[test]
fn interrupt_script_is_also_explained_without_the_feature() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());

    state.dispatch(Action::InterruptScript);

    let status = state.status().expect("un message explicite");
    assert!(status.is_error);
    assert!(status.text.to_lowercase().contains("python"));
}

#[test]
fn script_source_lives_in_state_and_is_not_lost_by_other_actions() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");

    assert_eq!(state.script_source(), "", "vide par défaut");

    state.set_script_source("print('bonjour')".to_string());
    assert_eq!(state.script_source(), "print('bonjour')");

    // Des actions qui n'ont rien à voir avec le script ne doivent pas y
    // toucher : le tampon de script est indépendant du reste de l'état.
    state.dispatch(Action::AddColumn { sheet: sheet1 });
    assert_eq!(state.script_source(), "print('bonjour')");
    state.dispatch(Action::Undo);
    assert_eq!(state.script_source(), "print('bonjour')");

    state.set_script_source("v = 2".to_string());
    assert_eq!(
        state.script_source(),
        "v = 2",
        "relecture après remplacement"
    );
}

/// Phase R4 : le banc doit savoir où sont les graphes, puisqu'il en affiche
/// un. Le pendant de `sheets()`, et la seule partie du panneau de rendu qui
/// se teste sans fenêtre — le reste, c'est `plotyx-render-egui` qui le
/// couvre (`tests/cache_session.rs`).
#[test]
fn les_graphes_du_projet_sont_enumeres_avec_leur_chemin() {
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let mut project = demo_project();
    let root = project.root();
    let sheet = sheet_named(project.tree(), "Sheet1");
    let colonne = project
        .tree()
        .child_by_name(sheet, "Temps")
        .expect("Sheet1 lisible")
        .expect("colonne Temps");
    let mut panel = plotyx_plot::PanelSpec::at(0, 0);
    panel.series.push(plotyx_plot::SeriesSpec::new(colonne));
    let spec = plotyx_plot::PlotSpec::empty("Essai").with_panels(panel);
    project
        .add_graph(root, Some("Graph1"), spec)
        .expect("graphe");

    let (session, state) = start(project, &dir.path().join("projet.plotyx"));
    let graphes = state.graphs();
    assert_eq!(graphes.len(), 1, "un seul graphe : {graphes:?}");
    assert_eq!(graphes[0].1, "Graph1");
    // Une feuille n'est pas un graphe, et réciproquement.
    assert!(state.sheets().iter().all(|(id, _)| *id != graphes[0].0));
    unused(&session.handle());
}

// -- Divers : le panneau Project Manager --

/// Le Project Manager s'arrête à `Workbook`/`Graph`/`Matrix`/`Note` : jamais
/// de descente dans les feuilles d'un classeur ni dans le contenu d'un
/// graphe (§1 de la tâche). `books()` reste le pendant à plat de `graphs()`,
/// utilisé par la disposition initiale de `crate::app::PlotyxApp::new`.
#[test]
fn books_are_enumerated_like_graphs_with_their_readable_path() {
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let (_session, state) = start_demo(dir.path());

    let books = state.books();
    assert_eq!(books.len(), 1, "un seul classeur : {books:?}");
    assert_eq!(books[0].1, "Book1");
}

/// [`AppState::project_tree`] montre les dossiers et les quatre natures de
/// fenêtre, jamais les feuilles d'un classeur.
#[test]
fn project_tree_stops_at_windows_and_never_descends_into_a_book() {
    use plotyx_app::state::{ProjectTreeNode, WindowKind};

    let dir = tempfile::tempdir().expect("dossier temporaire");
    let mut project = demo_project();
    let root = project.root();
    let folder = project.add_folder(root, "Manips").expect("dossier Manips");
    project
        .add_note(folder, Some("Note1"), "un texte")
        .expect("note");

    let (_session, state) = start(project, &dir.path().join("projet.plotyx"));
    let tree = state.project_tree();

    let book = tree
        .iter()
        .find(|node| {
            matches!(
                node,
                ProjectTreeNode::Window { name, kind, .. }
                    if name == "Book1" && *kind == WindowKind::Book
            )
        })
        .expect("Book1 est une fenêtre du Project Manager");
    // Une fenêtre du Project Manager n'a jamais d'enfants dans cette vue,
    // même si `Workbook` a bien des `Worksheet` dans le modèle.
    assert!(matches!(book, ProjectTreeNode::Window { .. }));

    let folder_node = tree
        .iter()
        .find_map(|node| match node {
            ProjectTreeNode::Folder { name, children, .. } if name == "Manips" => Some(children),
            _ => None,
        })
        .expect("Manips est un dossier déplié dans le Project Manager");
    assert!(
        folder_node.iter().any(|child| matches!(
            child,
            ProjectTreeNode::Window {
                kind: WindowKind::Note,
                ..
            }
        )),
        "la note apparaît dans son dossier"
    );
}

// -- Divers : l'édition de note --

#[test]
fn editing_a_note_commits_on_dispatch_and_is_undoable() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let mut project = demo_project();
    let root = project.root();
    let note = project
        .add_note(root, Some("Note1"), "avant")
        .expect("note");

    let (_session, mut state) = start(project, &dir.path().join("projet.plotyx"));
    assert_eq!(state.note_text(note), Some("avant".to_string()));

    state.begin_note_edit(note);
    if let Some(edit) = state.note_editing_mut() {
        edit.buffer = "après".to_string();
    }
    state.commit_note_edit();

    assert_eq!(state.note_text(note), Some("après".to_string()));
    assert!(state.editing().is_none());
    assert!(state.can_undo());

    state.dispatch(Action::Undo);
    assert_eq!(state.note_text(note), Some("avant".to_string()));
}

#[test]
fn committing_a_note_without_typing_writes_nothing() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let mut project = demo_project();
    let root = project.root();
    let note = project
        .add_note(root, Some("Note1"), "inchangé")
        .expect("note");

    let (_session, mut state) = start(project, &dir.path().join("projet.plotyx"));
    let revision = state.snapshot().revision;

    state.begin_note_edit(note);
    state.commit_note_edit();

    assert_eq!(state.snapshot().revision, revision, "rien n'a été écrit");
    assert!(!state.can_undo());
}
