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
use std::sync::Mutex;

use plotyx_app::state::{Action, AppState, HeaderField, MoveDirection};
use plotyx_core::{Cell, Column, ColumnRole, ColumnType, NodeId, Project, Tree};
use plotyx_import::{DecimalStyle, HeaderSkip, ImportRecipe, NameSource, RecipeStore, Separator};
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

/// Type d'une colonne dans l'arbre courant.
fn column_type(state: &AppState, column: NodeId) -> ColumnType {
    state
        .tree()
        .get(column)
        .expect("colonne présente")
        .as_column()
        .expect("nœud colonne")
        .column_type()
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

/// Protège les tests qui isolent le dossier de config des recettes d'import
/// (voir [`isolate_recipe_store`]) contre d'autres tests de ce même binaire
/// qui feraient la même chose en parallèle : `std::env::set_var` est un état
/// global du *processus*, jamais scopé à un thread — `cargo test` fait
/// tourner les tests de ce fichier sur plusieurs threads par défaut.
static IMPORT_RECIPES_ENV_LOCK: Mutex<()> = Mutex::new(());

/// Redirige `XDG_CONFIG_HOME` vers un répertoire temporaire propre à l'appel,
/// pour la durée de vie de la valeur rendue : `plotyx_import::RecipeStore::
/// load`/`save` (donc `AppState::open_import`/`save_import_recipe`/
/// `delete_import_recipe`) passent par `directories::ProjectDirs`, qui
/// respecte cette variable sur Linux. Sans ceci, ces méthodes liraient et
/// écriraient le vrai dossier de config de la machine qui exécute les
/// tests — non hermétique, et potentiellement pollué par un modèle laissé
/// par une session interactive antérieure.
///
/// Garde le verrou et le répertoire temporaire vivants jusqu'à ce que la
/// valeur rendue soit abandonnée (fin du test, y compris en cas de panique :
/// `Drop` restaure `XDG_CONFIG_HOME` à sa valeur d'avant).
fn isolate_recipe_store() -> impl Drop {
    struct Guard {
        previous: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
        _dir: tempfile::TempDir,
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
        }
    }
    let lock = IMPORT_RECIPES_ENV_LOCK
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    let dir = tempfile::tempdir().expect("répertoire temporaire pour les recettes d'import");
    let previous = std::env::var_os("XDG_CONFIG_HOME");
    std::env::set_var("XDG_CONFIG_HOME", dir.path());
    Guard {
        previous,
        _lock: lock,
        _dir: dir,
    }
}

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

// -- 3. Saisie non numérique dans une colonne numérique --

/// Taper un texte dans une colonne numérique **n'est plus une erreur** : la
/// colonne passe à [`ColumnType::Mixed`] et la cellule porte le texte, les
/// autres gardant leurs nombres (`Command::SetCellsPromoting`). Avant les
/// colonnes à types mixtes, la saisie était refusée et il fallait convertir
/// toute la colonne en texte — donc rendre tout le reste intraçable — pour
/// noter un « n/a » sur une seule ligne.
#[test]
fn non_numeric_input_promotes_the_column_to_mixed() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let before = state.snapshot().revision;
    let voisin = cell(&state, temps, 0);

    state.dispatch(Action::CommitCell {
        column: temps,
        row: 1,
        text: "n/a".to_string(),
    });

    assert!(
        state.status().is_none_or(|status| !status.is_error),
        "une saisie non numérique n'est plus une erreur"
    );
    assert_eq!(column_type(&state, temps), ColumnType::Mixed);
    assert_eq!(state.cell_text(temps, 1), "n/a");
    assert_eq!(
        cell(&state, temps, 0),
        voisin,
        "les autres lignes gardent leur nombre"
    );
    assert!(state.snapshot().revision > before);

    // Une seule entrée d'historique pour la conversion **et** l'écriture :
    // annuler ramène à la fois le type et la valeur d'avant.
    state.dispatch(Action::Undo);
    assert_eq!(column_type(&state, temps), ColumnType::Numeric);
    assert_eq!(cell(&state, temps, 1), Cell::Float(0.5), "valeur intacte");

    // Et la deuxième saisie tolérante, sur une colonne déjà mixte, ne change
    // plus le type : elle écrit simplement une cellule texte.
    state.dispatch(Action::Redo);
    state.dispatch(Action::CommitCell {
        column: temps,
        row: 2,
        text: "hs".to_string(),
    });
    assert_eq!(column_type(&state, temps), ColumnType::Mixed);
    assert_eq!(state.cell_text(temps, 2), "hs");
    // Et un nombre tapé dans une colonne devenue mixte reste un nombre.
    state.dispatch(Action::CommitCell {
        column: temps,
        row: 3,
        text: "7.25".to_string(),
    });
    assert_eq!(cell(&state, temps, 3), Cell::Float(7.25));
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

// -- Divers : la sélection de la grille (souris + Ctrl+C/X/V) --
//
// `crate::app` (egui) traduit clic/glisser en `AppState::select_cell`/
// `extend_selection`, puis Ctrl+C/X/V en `AppState::selected_cells_tsv`/
// `Action::ClearSelectedCells`/`Action::PasteTsv` — tout ce qui suit ce
// point de traduction est testable ici, sans fenêtre. Bug « le copier/
// couper/coller ne marche pas avec une sélection d'une seule cellule » :
// une sélection posée par un simple clic ([`AppState::select_cell`]) est un
// rectangle 1×1 comme un autre pour `Selection` (voir
// `crate::selection::Selection::single_cell` et ses tests unitaires) — les
// trois tests qui suivent le vérifient au niveau où `crate::app` s'arrête.

#[test]
fn a_single_cell_selection_copies_exactly_that_cell() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_cell(sheet1, temps, 2);
    assert!(state.is_selected(sheet1, temps, 2));

    let signal = column_id(&state, sheet1, "Signal");
    assert!(
        !state.is_selected(sheet1, signal, 2),
        "une seule cellule est sélectionnée, pas toute la ligne"
    );

    assert_eq!(
        state.selected_cells_tsv(sheet1),
        Some("1\n".to_string()),
        "Ctrl+C sur une sélection d'une seule cellule copie exactement son texte"
    );
}

#[test]
fn clearing_a_single_cell_selection_empties_only_that_cell() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_cell(sheet1, temps, 2);
    state.dispatch(Action::ClearSelectedCells { sheet: sheet1 });

    assert_eq!(cell(&state, temps, 2), Cell::Float(f64::NAN));
    assert_eq!(
        cell(&state, temps, 0),
        Cell::Float(0.0),
        "Ctrl+X/Retour arrière sur une seule cellule ne touche pas les autres"
    );
    assert_eq!(cell(&state, temps, 1), Cell::Float(0.5));
}

#[test]
fn pasting_onto_a_single_cell_selection_writes_from_its_anchor() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    // Sélection d'une seule cellule sur `Temps`, ligne 1 — le collage doit
    // s'étirer depuis cette ancre, sur autant de colonnes que le TSV collé
    // en porte (ici deux : `Temps` et `Signal`), pas seulement la cellule
    // sélectionnée.
    state.select_cell(sheet1, temps, 1);
    state.dispatch(Action::PasteTsv {
        sheet: sheet1,
        text: "9\t99\n".to_string(),
    });

    assert_eq!(cell(&state, temps, 1), Cell::Float(9.0));
    assert_eq!(cell(&state, signal, 1), Cell::Float(99.0));
    // Le reste de la colonne n'a pas bougé.
    assert_eq!(cell(&state, temps, 0), Cell::Float(0.0));
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

// -- 15. Suppr sur une sélection de cellules --

#[test]
fn deleting_a_single_selected_row_shifts_the_rest_up_instead_of_truncating() {
    // Régression : sélectionner la 3e ligne (index 2, 0-indexée) d'une
    // colonne et appuyer sur Suppr doit retirer *cette seule ligne* et faire
    // remonter les suivantes d'une position — pas tronquer tout ce qui suit
    // la ligne sélectionnée.
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    // "Temps" = [0.0, 0.5, 1.0, 1.5, 2.0] avant suppression.
    assert_eq!(state.row_count(sheet1), 5);

    state.select_cell(sheet1, temps, 2);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });

    // Seule la ligne d'index 2 (valeur 1.0) a disparu de la colonne
    // sélectionnée ; les lignes 3 et 4 (1.5, 2.0) sont remontées aux index 2
    // et 3 — la colonne perd exactement une valeur, elle n'est pas tronquée
    // à sa fin. `state.row_count` ne convient pas ici : c'est le maximum des
    // longueurs de colonnes de la feuille, et "Signal" (non sélectionnée)
    // garde ses 5 lignes.
    let temps_len = state
        .tree()
        .get(temps)
        .expect("colonne Temps présente")
        .as_column()
        .expect("nœud colonne")
        .len();
    assert_eq!(temps_len, 4);
    let values: Vec<Cell> = (0..4).map(|row| cell(&state, temps, row)).collect();
    assert_eq!(
        values,
        vec![
            Cell::Float(0.0),
            Cell::Float(0.5),
            Cell::Float(1.5),
            Cell::Float(2.0),
        ]
    );

    assert!(state.can_undo());
    state.dispatch(Action::Undo);
    let temps_len_after_undo = state
        .tree()
        .get(temps)
        .expect("colonne Temps présente")
        .as_column()
        .expect("nœud colonne")
        .len();
    assert_eq!(temps_len_after_undo, 5);
    assert_eq!(cell(&state, temps, 2), Cell::Float(1.0));
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

/// Importer plusieurs fichiers crée une feuille par fichier, chacune nommée
/// d'après son fichier (`file_stem`) et porteuse des colonnes du fichier —
/// et tout le lot tient dans **une seule** entrée d'historique : un unique
/// Ctrl+Z (`AppState::dispatch(Action::Undo)`) retire les deux feuilles
/// d'un coup, pas une à la fois.
#[test]
fn importing_several_files_creates_one_sheet_each_in_a_single_undo_entry() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();

    let file_a = dir.path().join("mesure_a.csv");
    std::fs::write(&file_a, "X,Y\n1,10\n2,20\n").expect("écrit mesure_a.csv");
    let file_b = dir.path().join("mesure_b.csv");
    std::fs::write(&file_b, "X,Y\n3,30\n4,40\n").expect("écrit mesure_b.csv");

    let mut recipe = plotyx_import::ImportRecipe::new("Sans titre");
    // Les deux fichiers ont une ligne d'en-tête "X,Y" : sans ceci, elle est
    // lue comme une ligne de données (deux valeurs manquantes) et les noms
    // de colonnes retombent sur le nom de fichier plutôt que "X"/"Y".
    recipe.has_column_header_row = true;
    state.dispatch(Action::ImportFiles {
        book,
        recipe,
        paths: vec![file_a.clone(), file_b.clone()],
    });

    let status = state.status().expect("un message de statut");
    assert!(
        !status.is_error,
        "l'import ne doit pas être signalé en échec"
    );
    assert!(
        status.text.contains("2"),
        "le message récapitule le nombre de fichiers importés : {}",
        status.text
    );

    let tree = state.tree();
    assert_eq!(
        state.worksheets_of(book).len(),
        sheets_before + 2,
        "une feuille par fichier importé"
    );
    let sheet_a = sheet_named(tree, "mesure_a");
    let sheet_b = sheet_named(tree, "mesure_b");
    let columns_a = state.visible_columns(sheet_a);
    assert_eq!(columns_a.len(), 2, "deux colonnes lues dans mesure_a.csv");
    assert_eq!(state.column_meta(columns_a[0].0).long_name, "X");
    assert_eq!(state.column_meta(columns_a[1].0).long_name, "Y");
    assert_eq!(cell_f64(&state, columns_a[0].0, 0), 1.0);
    assert_eq!(cell_f64(&state, columns_a[1].0, 1), 20.0);
    let columns_b = state.visible_columns(sheet_b);
    assert_eq!(cell_f64(&state, columns_b[0].0, 0), 3.0);

    // Une seule entrée d'historique pour tout le lot : un unique `Undo`
    // retire les deux feuilles d'un coup.
    assert!(state.can_undo());
    state.dispatch(Action::Undo);
    assert_eq!(
        state.worksheets_of(book).len(),
        sheets_before,
        "un seul Ctrl+Z retire tout l'import, feuilles des deux fichiers comprises"
    );
    assert!(
        !state.can_undo(),
        "l'import entier n'a compté que pour une seule entrée d'historique"
    );
}

/// Un fichier illisible n'interrompt pas le lot : les fichiers suivants sont
/// importés quand même, et le message final récapitule combien ont
/// réellement réussi.
#[test]
fn a_file_read_failure_does_not_abort_the_rest_of_the_batch() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();

    let missing = dir.path().join("absent.csv");
    let file_ok = dir.path().join("mesure.csv");
    std::fs::write(&file_ok, "X,Y\n1,10\n").expect("écrit mesure.csv");

    let recipe = plotyx_import::ImportRecipe::new("Sans titre");
    state.dispatch(Action::ImportFiles {
        book,
        recipe,
        paths: vec![missing, file_ok],
    });

    assert_eq!(
        state.worksheets_of(book).len(),
        sheets_before + 1,
        "seul le fichier lisible a produit une feuille"
    );
    let status = state.status().expect("un message de statut récapitulatif");
    assert!(
        status.text.contains('1'),
        "le récapitulatif final compte 1 fichier réellement importé : {}",
        status.text
    );
    assert!(state.can_undo());
    state.dispatch(Action::Undo);
    assert_eq!(state.worksheets_of(book).len(), sheets_before);
}

/// `AppState::open_import` démarre sur le premier modèle enregistré s'il y
/// en a, sinon sur une recette permissive — les deux branches de sa doc.
#[test]
fn opening_import_starts_from_the_first_saved_recipe_or_a_permissive_default() {
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");

    // Rien d'enregistré : recette permissive par défaut.
    state.open_import(book);
    {
        let import = state.import_state().expect("fenêtre d'import ouverte");
        assert_eq!(import.recipe.separator, Separator::Auto);
        assert_eq!(import.recipe.decimal, DecimalStyle::Dot);
        assert_eq!(import.recipe.header, HeaderSkip::None);
        assert_eq!(import.recipe.footer_skip_lines, 0);
        assert!(!import.recipe.has_column_header_row);
        assert_eq!(import.recipe.name_source, NameSource::Filename);
    }
    state.cancel_import();

    // Un modèle déjà enregistré sur disque (écrit directement, sans passer
    // par la fenêtre) : `open_import` doit le proposer d'entrée.
    let mut store = RecipeStore::default();
    let mut jasco = ImportRecipe::new("Jasco UV-Vis");
    jasco.separator = Separator::Semicolon;
    jasco.decimal = DecimalStyle::Comma;
    store.upsert(jasco);
    store.save().expect("écrit le modèle isolé sur disque");

    state.open_import(book);
    let import = state.import_state().expect("fenêtre d'import ouverte");
    assert_eq!(import.recipe.name, "Jasco UV-Vis");
    assert_eq!(import.recipe.separator, Separator::Semicolon);
    assert_eq!(import.recipe.decimal, DecimalStyle::Comma);
}

/// « Nouveau modèle » repart d'une recette permissive, même après avoir
/// chargé/modifié un modèle existant.
#[test]
fn new_import_recipe_resets_to_a_permissive_default() {
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");

    state.open_import(book);
    {
        let import = state.import_state_mut().expect("fenêtre d'import ouverte");
        import.recipe.separator = Separator::Tab;
        import.recipe.decimal = DecimalStyle::Comma;
        import.recipe.footer_skip_lines = 5;
        import.recipe.has_column_header_row = true;
        import.recipe.name = "Autre chose".to_string();
    }

    state.new_import_recipe();
    let import = state.import_state().expect("fenêtre d'import ouverte");
    assert_eq!(import.recipe.name, "Sans titre");
    assert_eq!(import.recipe.separator, Separator::Auto);
    assert_eq!(import.recipe.decimal, DecimalStyle::Dot);
    assert_eq!(import.recipe.header, HeaderSkip::None);
    assert_eq!(import.recipe.footer_skip_lines, 0);
    assert!(!import.recipe.has_column_header_row);
    assert_eq!(import.recipe.name_source, NameSource::Filename);
}

/// La liste déroulante des modèles charge le modèle choisi dans la recette
/// en cours d'édition ; un nom inconnu est sans effet (pas de panique, pas de
/// recette à moitié remplacée).
#[test]
fn select_import_recipe_loads_a_saved_model_and_ignores_unknown_names() {
    // `select_import_recipe` lui-même ne fait que chercher dans le `Vec` en
    // mémoire, mais `open_import` juste en dessous appelle bien
    // `RecipeStore::load()` pour de vrai : sans isolation, il lirait
    // `XDG_CONFIG_HOME` tel qu'un *autre* test de ce fichier, tournant en
    // parallèle et isolé dans son propre répertoire temporaire, l'aurait
    // laissé à cet instant précis — et chargerait son modèle au lieu de
    // rien, avant que la ligne suivante n'écrase `store` de toute façon.
    // Théorie confirmée à la dure : un run sur ~7 a bien fait échouer un
    // autre test de ce fichier de cette façon avant que cette isolation ne
    // soit ajoutée ici.
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");

    state.open_import(book);
    let mut store = RecipeStore::default();
    store.upsert(ImportRecipe::new("Jasco"));
    let mut fluo = ImportRecipe::new("Fluo");
    fluo.separator = Separator::Tab;
    store.upsert(fluo);
    state.import_state_mut().expect("fenêtre ouverte").store = store;

    state.select_import_recipe("Fluo");
    assert_eq!(
        state.import_state().unwrap().recipe.name,
        "Fluo",
        "le modèle choisi remplace la recette en cours"
    );
    assert_eq!(
        state.import_state().unwrap().recipe.separator,
        Separator::Tab
    );

    state.select_import_recipe("N'existe pas");
    assert_eq!(
        state.import_state().unwrap().recipe.name,
        "Fluo",
        "un nom inconnu ne remplace rien"
    );
}

/// Enregistrer un modèle sans nom (ou un nom blanc) est refusé, sans
/// modifier le store — condition explicitement documentée par
/// `AppState::save_import_recipe`.
#[test]
fn save_import_recipe_requires_a_non_empty_name() {
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");

    state.open_import(book);
    state.import_state_mut().unwrap().save_as_name = "   ".to_string();
    state.save_import_recipe();

    let status = state.status().expect("un message de statut");
    assert!(status.is_error, "un nom blanc doit être refusé");
    assert!(
        state.import_state().unwrap().store.recipes().is_empty(),
        "rien n'a été ajouté au store"
    );
}

/// Un modèle enregistré survit à la fermeture de la fenêtre : le rouvrir
/// (nouvelle session de fenêtre, mais même disque) le retrouve tel quel —
/// l'aller-retour complet à travers `RecipeStore::save`/`load`, jamais
/// exercé avant que ce test n'existe (les tests de `plotyx-import` ne
/// couvraient que la désérialisation, jamais l'écriture réelle).
#[test]
fn save_import_recipe_persists_to_disk_and_is_visible_after_reopening() {
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");

    state.open_import(book);
    {
        let import = state.import_state_mut().unwrap();
        import.recipe.separator = Separator::Semicolon;
        import.recipe.decimal = DecimalStyle::Comma;
        import.recipe.footer_skip_lines = 2;
        import.recipe.has_column_header_row = true;
        import.recipe.name_source = NameSource::HeaderKey("TITRE".to_string());
        import.save_as_name = "Jasco UV".to_string();
    }
    state.save_import_recipe();
    let status = state.status().expect("un message de statut");
    assert!(
        !status.is_error,
        "l'enregistrement a réussi : {}",
        status.text
    );

    state.cancel_import();
    state.open_import(book);
    let import = state.import_state().expect("fenêtre d'import ouverte");
    assert_eq!(import.recipe.name, "Jasco UV");
    assert_eq!(import.recipe.separator, Separator::Semicolon);
    assert_eq!(import.recipe.decimal, DecimalStyle::Comma);
    assert_eq!(import.recipe.footer_skip_lines, 2);
    assert!(import.recipe.has_column_header_row);
    assert_eq!(
        import.recipe.name_source,
        NameSource::HeaderKey("TITRE".to_string())
    );
}

/// Supprimer un modèle le retire du disque, pas seulement de la liste en
/// mémoire de la fenêtre courante.
#[test]
fn delete_import_recipe_removes_it_from_the_store_on_disk() {
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");

    let mut store = RecipeStore::default();
    store.upsert(ImportRecipe::new("Jasco"));
    store.upsert(ImportRecipe::new("Fluo"));
    store
        .save()
        .expect("écrit les deux modèles isolés sur disque");

    state.open_import(book);
    state.delete_import_recipe("Jasco");
    let status = state.status().expect("un message de statut");
    assert!(
        !status.is_error,
        "la suppression a réussi : {}",
        status.text
    );

    state.cancel_import();
    state.open_import(book);
    let names: Vec<&str> = state
        .import_state()
        .unwrap()
        .store
        .recipes()
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["Fluo"],
        "« Jasco » a bien disparu du fichier sur disque, « Fluo » reste"
    );
}

/// L'aperçu suit les réglages courants image par image (pas de mise en
/// cache) : sans « la première ligne donne les noms de colonnes », la ligne
/// d'en-tête est lue comme une donnée illisible (valeurs manquantes) ; en
/// l'activant, elle nomme les colonnes et la première vraie donnée apparaît.
/// Couvre aussi les deux cas où l'aperçu ne porte pas de table : aucun
/// fichier choisi, et un chemin qui n'existe pas.
#[test]
fn import_preview_reflects_current_recipe_settings_and_reports_parse_errors() {
    // `open_import` appelle `RecipeStore::load()` pour de vrai — isolé pour
    // la même raison que `select_import_recipe_loads_a_saved_model_and_
    // ignores_unknown_names`, ci-dessus.
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("mesure.csv");
    std::fs::write(&file, "Time,Signal\n1,10\n2,20\n").expect("écrit mesure.csv");

    state.open_import(book);
    assert!(
        state.import_preview().is_none(),
        "aucun fichier choisi : pas d'aperçu"
    );

    state.set_import_paths(vec![file.clone()]);
    let table = state
        .import_preview()
        .expect("un aperçu, un fichier est choisi")
        .expect("le fichier se parse");
    assert!(
        table.columns[0].values[0].is_nan(),
        "sans has_column_header_row, « Time » est lu comme une donnée illisible"
    );

    state
        .import_state_mut()
        .unwrap()
        .recipe
        .has_column_header_row = true;
    let table = state.import_preview().unwrap().unwrap();
    assert_eq!(table.columns[0].name.as_deref(), Some("Time"));
    assert_eq!(table.columns[1].name.as_deref(), Some("Signal"));
    assert_eq!(table.columns[0].values[0], 1.0);

    state.import_state_mut().unwrap().paths = vec![dir.path().join("n_existe_pas.csv")];
    assert!(
        state.import_preview().unwrap().is_err(),
        "un chemin introuvable est signalé, pas paniqué"
    );
}

/// `NameSource::HeaderKey` retrouve le nom de la série dans les lignes
/// d'en-tête ignorées (motif « CLÉ<séparateur>VALEUR »), pour une colonne
/// qui n'a par ailleurs aucun nom propre (pas de ligne d'en-tête de
/// colonnes) — le cas d'usage central de la fonctionnalité, un fichier
/// Jasco-like à une seule série.
#[test]
fn importing_resolves_series_name_from_a_header_key_when_the_column_has_no_name_of_its_own() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("spectre.csv");
    std::fs::write(&file, "TITLE;MonEchantillon\nXYDATA\n1;10\n2;20\n").expect("écrit spectre.csv");

    let mut recipe = ImportRecipe::new("Sans titre");
    recipe.separator = Separator::Semicolon;
    recipe.header = HeaderSkip::UntilMarker("XYDATA".to_string());
    recipe.name_source = NameSource::HeaderKey("TITLE".to_string());
    state.dispatch(Action::ImportFiles {
        book,
        recipe,
        paths: vec![file],
    });

    let tree = state.tree();
    let sheet = sheet_named(tree, "spectre");
    let columns = state.visible_columns(sheet);
    assert_eq!(state.column_meta(columns[0].0).long_name, "X");
    assert_eq!(
        state.column_meta(columns[1].0).long_name,
        "MonEchantillon",
        "la deuxième colonne prend le nom lu sur la ligne TITLE, pas le nom de fichier"
    );
}

/// `has_column_header_row` nomme **chaque** colonne d'après la ligne
/// d'en-tête restante, pas seulement la première — le cas d'un fichier «
/// matrice » avec plusieurs séries Y, chacune sous son propre nom.
#[test]
fn importing_a_file_with_a_column_header_row_names_every_series_from_it() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("matrice.txt");
    std::fs::write(&file, "wavelength\tA\tB\n1\t10\t100\n2\t20\t200\n").expect("écrit matrice.txt");

    let mut recipe = ImportRecipe::new("Sans titre");
    recipe.separator = Separator::Tab;
    recipe.has_column_header_row = true;
    state.dispatch(Action::ImportFiles {
        book,
        recipe,
        paths: vec![file],
    });

    let tree = state.tree();
    let sheet = sheet_named(tree, "matrice");
    let columns = state.visible_columns(sheet);
    assert_eq!(columns.len(), 3);
    assert_eq!(state.column_meta(columns[0].0).long_name, "wavelength");
    assert_eq!(state.column_meta(columns[1].0).long_name, "A");
    assert_eq!(state.column_meta(columns[2].0).long_name, "B");
    assert_eq!(cell_f64(&state, columns[1].0, 1), 20.0);
    assert_eq!(cell_f64(&state, columns[2].0, 0), 100.0);
}

/// Importer un seul fichier ne compte lui aussi que pour une seule entrée
/// d'annulation — le lot à un seul élément n'est pas un cas particulier.
#[test]
fn undoing_a_single_file_import_removes_exactly_that_sheet() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();
    let file = dir.path().join("seul.csv");
    std::fs::write(&file, "1,10\n2,20\n").expect("écrit seul.csv");

    state.dispatch(Action::ImportFiles {
        book,
        recipe: ImportRecipe::new("Sans titre"),
        paths: vec![file],
    });
    assert_eq!(state.worksheets_of(book).len(), sheets_before + 1);

    state.dispatch(Action::Undo);
    assert_eq!(state.worksheets_of(book).len(), sheets_before);
    assert!(!state.can_undo());
}

/// Un fichier qui se lit mais ne s'analyse pas (marqueur d'en-tête jamais
/// trouvé) n'interrompt pas non plus le lot, et le message final signale un
/// échec — pas un simple décompte muet, voir la doc d'`AppState::
/// import_files` (§ correctif) : `self.fail`/`self.notice` remplacent
/// `self.status`, un `self.fail` par fichier en échec se serait sinon fait
/// écraser par le message récapitulatif final.
#[test]
fn a_parse_failure_amid_a_batch_is_surfaced_as_an_error_not_silently_swallowed() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();

    let bad = dir.path().join("incomplet.csv");
    std::fs::write(&bad, "TITLE;Incomplet\n").expect("écrit incomplet.csv (jamais XYDATA)");
    let good = dir.path().join("complet.csv");
    std::fs::write(&good, "TITLE;Complet\nXYDATA\n1;10\n").expect("écrit complet.csv");

    let mut recipe = ImportRecipe::new("Sans titre");
    recipe.separator = Separator::Semicolon;
    recipe.header = HeaderSkip::UntilMarker("XYDATA".to_string());
    state.dispatch(Action::ImportFiles {
        book,
        recipe,
        paths: vec![bad.clone(), good],
    });

    assert_eq!(
        state.worksheets_of(book).len(),
        sheets_before + 1,
        "seul le fichier qui s'analyse a produit une feuille"
    );
    let status = state.status().expect("un message de statut");
    assert!(
        status.is_error,
        "un fichier en échec doit se voir, pas disparaître derrière un décompte : {}",
        status.text
    );
    assert!(
        status.text.contains("incomplet.csv"),
        "le message nomme le fichier fautif : {}",
        status.text
    );
}

/// Importer dans un classeur qui n'existe pas (identifiant invalide) échoue
/// proprement — signalé, jamais de panique, aucune feuille fantôme.
#[test]
fn importing_into_a_nonexistent_book_fails_gracefully() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let file = dir.path().join("mesure.csv");
    std::fs::write(&file, "1,10\n").expect("écrit mesure.csv");

    state.dispatch(Action::ImportFiles {
        book: NodeId::from_raw(999_999),
        recipe: ImportRecipe::new("Sans titre"),
        paths: vec![file],
    });

    let status = state.status().expect("un message de statut");
    assert!(status.is_error, "un classeur inexistant doit être signalé");
    assert!(!state.can_undo(), "rien n'a été créé, rien à annuler");
}

/// Lit une cellule numérique brute, pour les assertions sur le contenu
/// importé (au-delà de ce que `cell_text` laisse voir — voir [`cell`]).
fn cell_f64(state: &AppState, column: NodeId, row: usize) -> f64 {
    match cell(state, column, row) {
        Cell::Float(value) => value,
        Cell::Text(_) => panic!("{column} n'est pas une colonne numérique"),
    }
}

/// Une cellule qui ne s'analyse pas en nombre, au milieu d'une colonne par
/// ailleurs numérique, ne devient plus un `NaN` muet : la colonne importée
/// devient `Mixed` et garde le jeton d'origine — c'est le cas d'usage
/// motivant du chantier « prévisualisation d'import », qui consomme enfin
/// côté import le chantier « colonnes à types mixtes »
/// (`ParsedColumn::texts`, voir `docs/chantier-import-previsualisation.md`).
/// Une colonne entièrement numérique, elle, continue de produire une
/// colonne `Numeric` à l'identique d'avant ce chantier — vérifié par
/// ailleurs (toutes les autres assertions de ce fichier sur des imports
/// numériques n'ont pas changé de résultat).
#[test]
fn importing_a_non_numeric_cell_produces_a_mixed_column_with_the_original_text() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("mesure.csv");
    std::fs::write(&file, "1,2\nn/a,4\n5,6\n").expect("écrit mesure.csv");

    state.dispatch(Action::ImportFiles {
        book,
        recipe: ImportRecipe::new("Sans titre"),
        paths: vec![file],
    });

    let tree = state.tree();
    let sheet = sheet_named(tree, "mesure");
    let columns = state.visible_columns(sheet);
    let first = columns[0].0;
    assert_eq!(column_type(&state, first), ColumnType::Mixed);
    assert_eq!(cell_f64(&state, first, 0), 1.0);
    match cell(&state, first, 1) {
        Cell::Text(Some(text)) => assert_eq!(text, "n/a"),
        other => panic!("cellule 1 devrait être un texte conservé, trouvé {other:?}"),
    }
    assert_eq!(cell_f64(&state, first, 2), 5.0);

    let second = columns[1].0;
    assert_eq!(
        column_type(&state, second),
        ColumnType::Numeric,
        "une colonne entièrement numérique n'est pas promue en Mixed"
    );
}

/// Les rôles de sous-en-tête (`long_name`/`unit`/`comment`) assignés dans la
/// recette peuplent `ColumnMeta` — la fonctionnalité centrale du chantier
/// « prévisualisation d'import » côté modèle, au-delà de l'aperçu lui-même
/// (testé côté `plotyx-import`, voir `preview::tests`).
#[test]
fn subheader_roles_populate_column_meta_long_name_unit_and_comment() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("instrument.csv");
    std::fs::write(
        &file,
        "Temperature,Pressure\ndegC,bar\nmesure lente,mesure rapide\n1,2\n3,4\n",
    )
    .expect("écrit instrument.csv");

    let mut recipe = ImportRecipe::new("Sans titre");
    recipe.header_layout = plotyx_import::HeaderLayout {
        main_header_lines: 0,
        subheader_lines: 3,
        auto_detect: false,
    };
    recipe.subheader_roles = plotyx_import::SubheaderRoles {
        long_name: Some(plotyx_import::SubheaderLineRange::single(1)),
        unit: Some(plotyx_import::SubheaderLineRange::single(2)),
        comment: Some(plotyx_import::SubheaderLineRange::single(3)),
        same_line_name_unit: false,
    };
    state.dispatch(Action::ImportFiles {
        book,
        recipe,
        paths: vec![file],
    });

    let tree = state.tree();
    let sheet = sheet_named(tree, "instrument");
    let columns = state.visible_columns(sheet);
    let meta0 = state.column_meta(columns[0].0);
    assert_eq!(meta0.long_name, "Temperature");
    assert_eq!(meta0.unit, "degC");
    assert_eq!(meta0.comment, "mesure lente");
    let meta1 = state.column_meta(columns[1].0);
    assert_eq!(meta1.long_name, "Pressure");
    assert_eq!(meta1.unit, "bar");
    assert_eq!(meta1.comment, "mesure rapide");
}

// -- 14. Entrée sur la dernière ligne : ajout de ligne borné à la feuille --

/// Reproduit le bug rapporté : dans `demo_project`, `Note` (colonne texte,
/// 3 valeurs stockées) est plus courte que `Temps`/`Signal` (5 valeurs) sur
/// la même feuille — un cas que le modèle autorise explicitement (voir la
/// doc de [`demo_project`]). Valider par Entrée la ligne 2 de `Note`
/// (`Some("")`, la dernière valeur *stockée* de la colonne, mais pas la
/// dernière ligne *affichée* de la feuille) ne doit pas ajouter de ligne :
/// avant le correctif, `commit_cell_and_advance` comparait `row + 1` à la
/// longueur du buffer de la seule colonne éditée plutôt qu'à la hauteur
/// réelle de la feuille, et ajoutait une ligne fantôme en bas de toute la
/// feuille.
#[test]
fn committing_the_last_stored_value_of_a_short_column_mid_sheet_does_not_add_a_row() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");
    assert_eq!(state.row_count(sheet1), 5, "Temps/Signal font 5 lignes");

    state.dispatch(Action::CommitCellAndAdvance {
        column: note,
        row: 2,
        text: "milieu".to_string(),
        sheet: sheet1,
    });

    assert_eq!(
        state.row_count(sheet1),
        5,
        "éditer une ligne au milieu de la feuille n'ajoute pas de ligne, \
         même si c'était la dernière valeur stockée de cette colonne"
    );
    assert_eq!(state.cell_text(note, 2), "milieu");
}

/// À l'inverse, valider par Entrée la vraie dernière ligne de la feuille
/// ajoute bien une ligne — comportement voulu, inchangé par le correctif.
#[test]
fn committing_the_last_row_of_the_sheet_adds_a_row() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    assert_eq!(state.row_count(sheet1), 5);

    state.dispatch(Action::CommitCellAndAdvance {
        column: temps,
        row: 4,
        text: "2.5".to_string(),
        sheet: sheet1,
    });

    assert_eq!(
        state.row_count(sheet1),
        6,
        "dernière ligne : une ligne ajoutée"
    );
    assert_eq!(cell(&state, temps, 4), Cell::Float(2.5));
}

// -- 15. Conversion de type de colonne --

/// Choix manuel du type d'une colonne (`Action::SetColumnType`) : une
/// colonne texte contenant du texte non numérique perd ce texte en passant
/// en numérique (`NaN`), conversion destructrice assumée — même politique
/// que `Column::set_column_type` côté `plotyx-core`.
#[test]
fn set_column_type_action_converts_and_can_lose_non_numeric_text() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");
    assert_eq!(column_type(&state, note), ColumnType::Text);

    state.dispatch(Action::SetColumnType {
        column: note,
        column_type: ColumnType::Numeric,
    });

    assert_eq!(column_type(&state, note), ColumnType::Numeric);
    match cell(&state, note, 0) {
        Cell::Float(value) => assert!(value.is_nan(), "« a » n'est pas un nombre : disparaît"),
        other => panic!("cellule numérique attendue, reçu {other:?}"),
    }
}

/// Une colonne texte dont toutes les valeurs présentes s'analysent comme
/// des nombres se convertit seule en numérique dès qu'on valide une cellule
/// — pendant du sens inverse déjà couvert par
/// `non_numeric_input_promotes_the_column_to_mixed` (colonne numérique qui
/// reçoit du texte).
#[test]
fn a_text_column_left_with_only_numeric_values_auto_promotes_to_numeric() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");
    assert_eq!(column_type(&state, note), ColumnType::Text);

    // `Note` = [Some("a"), None, Some("")] : remplacer « a » par un nombre
    // laisse la ligne 2 (`Some("")`, présente mais non numérique) en
    // travers — la colonne ne bascule pas tant qu'une valeur *présente*
    // reste non numérique (`None`, lui, est ignoré : c'est une valeur
    // absente, pas un texte qui refuse de s'analyser).
    state.dispatch(Action::CommitCell {
        column: note,
        row: 0,
        text: "1".to_string(),
    });
    assert_eq!(
        column_type(&state, note),
        ColumnType::Text,
        "la ligne 2 (chaîne vide présente) n'est pas un nombre : pas de conversion"
    );

    state.dispatch(Action::CommitCell {
        column: note,
        row: 2,
        text: "2.5".to_string(),
    });

    assert_eq!(
        column_type(&state, note),
        ColumnType::Numeric,
        "toutes les valeurs présentes sont des nombres : conversion automatique"
    );
    assert_eq!(cell(&state, note, 0), Cell::Float(1.0));
    assert_eq!(cell(&state, note, 2), Cell::Float(2.5));
}

/// `Action::SetColumnRole` (sous-menu « Rôle » du menu contextuel d'en-tête,
/// à côté du type) écrit `ColumnMeta::role` — rôle par défaut `Y` pour toute
/// colonne, voir `Column::new` côté `plotyx-core`.
#[test]
fn set_column_role_action_writes_the_column_meta_role() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    assert_eq!(state.column_meta(temps).role, ColumnRole::Y);

    state.dispatch(Action::SetColumnRole {
        column: temps,
        role: ColumnRole::X,
    });

    assert_eq!(state.column_meta(temps).role, ColumnRole::X);
}

// -- 16. Multi-sélection de colonnes et de champs d'en-tête --

/// Glisser/Maj-clic sur la rangée des lettres (`extend_column_selection`)
/// sélectionne une plage de colonnes entières, pas une seule.
#[test]
fn extending_a_column_selection_covers_the_whole_range() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_column(sheet1, temps);
    state.extend_column_selection(sheet1, note);

    assert!(state.is_column_selected(sheet1, temps));
    assert!(
        state.is_column_selected(sheet1, signal),
        "colonne intermédiaire incluse"
    );
    assert!(state.is_column_selected(sheet1, note));
}

/// Glisser/Maj-clic sur les rangées de champs d'en-tête
/// (`extend_header_field_selection`) sélectionne un rectangle de champs sur
/// plusieurs colonnes et plusieurs champs, pas un seul.
#[test]
fn extending_a_header_field_selection_covers_the_whole_rectangle() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_header_field(sheet1, temps, HeaderField::Unit);
    state.extend_header_field_selection(sheet1, signal, HeaderField::Formula);

    for field in [
        HeaderField::Unit,
        HeaderField::Comment,
        HeaderField::Formula,
    ] {
        assert!(
            state.is_header_field_selected(sheet1, temps, field),
            "{field:?} de Temps dans le rectangle"
        );
        assert!(
            state.is_header_field_selected(sheet1, signal, field),
            "{field:?} de Signal dans le rectangle"
        );
    }
    assert!(
        !state.is_header_field_selected(sheet1, temps, HeaderField::LongName),
        "Nom long est au-dessus de l'ancre (Unité) : hors du rectangle"
    );
    assert_eq!(
        state.selected_header_field(sheet1),
        None,
        "un rectangle de plusieurs champs n'est jamais un champ unique"
    );
}

/// Glisser depuis un champ d'en-tête vers une cellule de données
/// (`extend_selection`, la méthode *cellule de données*, appelée alors que
/// la sélection en cours est une plage de champs) ne produit pas de
/// rectangle mixte silencieusement ignoré : elle repose une sélection d'une
/// seule cellule à la cible, comme un simple clic — même politique qu'au
/// clavier (voir [`AppState::move_selection`]).
#[test]
fn dragging_a_cell_extension_across_the_header_boundary_reselects_a_single_cell() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_header_field(sheet1, temps, HeaderField::Unit);
    state.extend_selection(sheet1, signal, 2);

    assert!(
        state.is_selected(sheet1, signal, 2),
        "la cellule visée devient la nouvelle sélection"
    );
    assert_eq!(
        state.selected_header_field(sheet1),
        None,
        "plus aucun champ d'en-tête sélectionné après le franchissement"
    );
}

/// Glisser depuis une cellule de données vers la rangée des lettres
/// (`extend_column_selection` appelée alors que la sélection en cours est un
/// rectangle de cellules) repose une sélection d'une seule colonne à la
/// cible plutôt que d'être ignorée.
#[test]
fn dragging_a_column_extension_across_the_data_boundary_reselects_a_single_column() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, temps, 0);
    state.extend_column_selection(sheet1, signal);

    assert!(
        state.is_column_selected(sheet1, signal),
        "la colonne visée devient la nouvelle sélection"
    );
    assert!(
        !state.is_selected(sheet1, temps, 0),
        "l'ancienne sélection de cellule n'a plus cours"
    );
}

/// Glisser depuis la rangée des lettres vers une rangée de champs d'en-tête
/// (`extend_header_field_selection` appelée alors que la sélection en cours
/// est une plage de colonnes) repose une sélection d'un seul champ à la
/// cible plutôt que d'être ignorée.
#[test]
fn dragging_a_header_field_extension_across_the_column_boundary_reselects_a_single_field() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_column(sheet1, temps);
    state.extend_header_field_selection(sheet1, signal, HeaderField::Comment);

    assert_eq!(
        state.selected_header_field(sheet1),
        Some((signal, HeaderField::Comment)),
        "le champ visé devient la nouvelle sélection"
    );
    assert!(
        !state.is_column_selected(sheet1, temps),
        "l'ancienne plage de colonnes n'a plus cours"
    );
}

/// Flèche Haut depuis la première ligne de données rejoint le champ Formule
/// (le plus bas des 4 champs d'en-tête) ; Bas depuis Formule revient à la
/// première ligne de données — la frontière entre l'en-tête et les données
/// se traverse maintenant au clavier.
#[test]
fn arrow_up_from_the_first_data_row_enters_the_header_fields() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_cell(sheet1, temps, 0);
    state.move_selection(sheet1, MoveDirection::Up, false);
    assert_eq!(
        state.selected_header_field(sheet1),
        Some((temps, HeaderField::Formula))
    );

    state.move_selection(sheet1, MoveDirection::Up, false);
    assert_eq!(
        state.selected_header_field(sheet1),
        Some((temps, HeaderField::Comment))
    );

    state.move_selection(sheet1, MoveDirection::Down, false);
    assert_eq!(
        state.selected_header_field(sheet1),
        Some((temps, HeaderField::Formula))
    );

    state.move_selection(sheet1, MoveDirection::Down, false);
    assert!(
        state.is_selected(sheet1, temps, 0),
        "Bas depuis Formule redescend en première ligne de données"
    );
}
