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

use plotyx_app::app::{grid_grip_side, GripSide};
use plotyx_app::selection::Selection;
use plotyx_app::state::{
    active_graph, drop_payload_action, file_drop_plan, Action, ActiveTabKind, AppState,
    ColumnDragPayload, GridZone, HeaderField, ImportDestination, MoveDirection, PlotAxis, PlotEdit,
    PlotSettingsTab, PlotTarget,
};
use plotyx_core::{Cell, Column, ColumnRole, ColumnType, NodeId, Project, Tree};
use plotyx_import::{DecimalStyle, HeaderSkip, ImportRecipe, NameSource, RecipeStore, Separator};
use plotyx_plot::{AxisScale, PlotType, RawVariant};
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
            ("Note", ColumnType::Mixed),
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
    // « la colonne A », pas « Temps » : le libellé désigne la colonne comme
    // la grille l'affiche (sa lettre de position), jamais par son nom de
    // nœud, qui n'apparaît nulle part à l'écran — voir
    // `plotyx_commands::command::column_label`. `Temps` reste le nom de nœud
    // de cette colonne (c'est par lui que `column_id` la retrouve ici), il
    // n'a simplement plus sa place dans un libellé destiné à l'utilisateur.
    assert_eq!(
        state.undo_label(),
        Some("Annuler Saisie dans la colonne A".to_string())
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
        Some("Rétablir Saisie dans la colonne A".to_string())
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
fn adding_then_undoing_a_worksheet_restores_the_tree_bit_for_bit() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book1 = book_named(state.tree(), "Book1");
    let before = std::sync::Arc::clone(state.tree());
    let sheets_before = state.worksheets_of(book1);

    state.dispatch(Action::AddWorksheet { book: book1 });

    let sheets_after = state.worksheets_of(book1);
    assert_eq!(sheets_after.len(), sheets_before.len() + 1);
    let (new_sheet, new_name) = sheets_after.last().cloned().expect("la feuille ajoutée");
    assert!(!new_name.is_empty(), "un nom a été généré");
    assert!(
        sheets_before.iter().all(|(id, _)| *id != new_sheet),
        "nouvelle feuille"
    );

    state.dispatch(Action::Undo);
    assert_eq!(**state.tree(), *before);
    assert_eq!(state.worksheets_of(book1).len(), sheets_before.len());
}

#[test]
fn creating_a_workbook_is_undoable_and_redoable() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let before = std::sync::Arc::clone(state.tree());
    let books_before = state.books();

    state.dispatch(Action::AddWorkbook);

    let after = std::sync::Arc::clone(state.tree());
    let books_after = state.books();
    assert_eq!(books_after.len(), books_before.len() + 1);
    let (book, name) = books_after.last().expect("classeur créé");
    assert!(!name.is_empty(), "un nom est généré");
    assert!(
        state.selected_sheet_in(*book).is_some(),
        "un nouveau classeur contient sa feuille initiale"
    );
    assert_eq!(
        state.undo_label().as_deref(),
        Some("Annuler Ajouter un classeur")
    );

    state.dispatch(Action::Undo);
    assert_eq!(**state.tree(), *before);

    state.dispatch(Action::Redo);
    assert_eq!(**state.tree(), *after);
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
// `extend_selection`, puis Ctrl+C/Ctrl+Maj+C/X/V/Ctrl+Maj+V en
// `AppState::copy_selection`/`AppState::copy_full_columns`/
// `Action::ClearSelectedCells`/`Action::Paste`/`Action::PasteAbsolute` —
// tout ce qui suit ce point de traduction est testable ici, sans fenêtre.
// Bug « le copier/couper/coller ne marche pas avec une sélection d'une
// seule cellule » : une sélection posée par un simple clic
// ([`AppState::select_cell`]) est un rectangle 1×1 comme un autre pour
// `Selection` (voir `crate::selection::Selection::single_cell` et ses tests
// unitaires) — les trois tests qui suivent le vérifient au niveau où
// `crate::app` s'arrête.

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
        state.copy_selection(sheet1),
        Some("1".to_string()),
        "Ctrl+C sur une sélection d'une seule cellule copie exactement son texte, rien d'autre"
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
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "9\t99\n".to_string(),
    });

    assert_eq!(cell(&state, temps, 1), Cell::Float(9.0));
    assert_eq!(cell(&state, signal, 1), Cell::Float(99.0));
    // Le reste de la colonne n'a pas bougé.
    assert_eq!(cell(&state, temps, 0), Cell::Float(0.0));
}

/// Régression : coller un texte non numérique dans une colonne `Numeric`
/// écrivait silencieusement `NaN` (la case paraissait vidée) sans jamais
/// élargir la colonne en `Mixed` — contrairement à la même saisie tapée au
/// clavier (`non_numeric_input_promotes_the_column_to_mixed`). Le collage
/// doit se comporter exactement pareil : c'est la même tolérance,
/// `Command::SetCellsPromoting` plutôt qu'un `NaN` qui avale le texte collé.
#[test]
fn pasting_non_numeric_text_onto_a_numeric_cell_promotes_the_column_to_mixed() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let voisin = cell(&state, temps, 0);

    state.select_cell(sheet1, temps, 1);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "n/a".to_string(),
    });

    assert_eq!(
        column_type(&state, temps),
        ColumnType::Mixed,
        "le collage doit promouvoir la colonne comme le ferait une saisie clavier"
    );
    assert_eq!(state.cell_text(temps, 1), "n/a");
    assert_eq!(
        cell(&state, temps, 0),
        voisin,
        "les autres lignes gardent leur nombre"
    );

    // Une seule entrée d'historique pour la promotion **et** l'écriture.
    state.dispatch(Action::Undo);
    assert_eq!(column_type(&state, temps), ColumnType::Numeric);
    assert_eq!(cell(&state, temps, 1), Cell::Float(0.5));
}

/// Même régression, pour un rectangle collé sur plusieurs lignes : une
/// seule valeur non numérique parmi les lignes collées suffit à promouvoir
/// toute l'écriture en `Mixed`, les valeurs numériques du même collage
/// restant des nombres traçables.
#[test]
fn pasting_a_mixed_numeric_and_text_block_promotes_the_column_once() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_cell(sheet1, temps, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "7\nn/a\n9\n".to_string(),
    });

    assert_eq!(column_type(&state, temps), ColumnType::Mixed);
    assert_eq!(cell(&state, temps, 0), Cell::Float(7.0));
    assert_eq!(state.cell_text(temps, 1), "n/a");
    assert_eq!(cell(&state, temps, 2), Cell::Float(9.0));
}

/// Valide un champ d'en-tête via le même chemin que la saisie utilisateur
/// (`begin_header_edit`/`commit_header_edit`), pour préparer une sélection
/// de champ dans les tests de copier/coller mixte ci-dessous.
fn set_header_field(state: &mut AppState, column: NodeId, field: HeaderField, text: &str) {
    state.begin_header_edit(column, field);
    if let Some(edit) = state.header_editing_mut() {
        edit.buffer = text.to_string();
    }
    state.commit_header_edit();
}

/// Bug historique (trois correctifs successifs) : une sélection copiée à
/// cheval sur l'en-tête et les données (« rectangle mixte »,
/// `paste_tsv` d'alors) dumpait les lignes de champ d'en-tête dans des
/// lignes de données, décalant tout ce qui suit. Le nouveau modèle rend ce
/// scénario **structurellement impossible** plutôt que de le corriger au
/// coup par coup : un glisser parti d'un champ d'en-tête qui atteint une
/// cellule de donnée s'arrête au champ Formule (voir `crate::selection`),
/// donc ne peut plus jamais produire une copie mixte. Ce test le vérifie au
/// niveau où `crate::app` s'arrête : la sélection clampée ne copie que des
/// champs, et coller ce résultat sur une cellule de donnée (sortes
/// différentes) ne colle rien plutôt que de mal se répartir.
#[test]
fn extending_a_header_selection_into_data_clamps_instead_of_mixing_a_copy() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, temps, HeaderField::Unit, "nm");
    set_header_field(&mut state, temps, HeaderField::Comment, "a");
    // Formule volontairement laissée vide, pour vérifier qu'un champ vide se
    // copie bien (et ne casse pas le format).

    // Glisser parti du champ Unité vers une cellule de donnée : la
    // sélection s'arrête à Formule, elle ne touche jamais la ligne 2.
    state.select_header_field(sheet1, temps, HeaderField::Unit);
    state.extend_selection(sheet1, temps, 2);
    assert!(state.is_header_field_selected(sheet1, temps, HeaderField::Unit));
    assert!(state.is_header_field_selected(sheet1, temps, HeaderField::Comment));
    assert!(state.is_header_field_selected(sheet1, temps, HeaderField::Formula));
    assert!(
        !state.is_selected(sheet1, temps, 2),
        "le glisser ne traverse plus la frontière en-tête/données"
    );

    let text = state.copy_selection(sheet1).expect("sélection non vide");
    assert_eq!(
        text, "nm\na\n\n",
        "3 lignes de champ en clair, rien de plus : la copie ne porte plus \
         de ligne de donnée"
    );

    let signal_data_before = [
        cell(&state, signal, 0),
        cell(&state, signal, 1),
        cell(&state, signal, 2),
    ];

    // Collée sur une cellule de donnée de `Signal` : sortes différentes
    // (champs copiés, ancre sur une donnée), rien ne se colle (règle 2 de
    // la matrice de comportement, voir `plotyx_app::state`).
    state.select_cell(sheet1, signal, 1);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    let meta = state.column_meta(signal);
    assert_eq!(meta.unit, "");
    assert_eq!(meta.comment, "");
    assert_eq!(cell(&state, signal, 0), signal_data_before[0]);
    assert_eq!(cell(&state, signal, 1), signal_data_before[1]);
    assert_eq!(cell(&state, signal, 2), signal_data_before[2]);
}

/// Un copier/coller purement données (aucun champ d'en-tête dans la
/// sélection) colle exactement comme un tableur de référence : décalé sur
/// l'ancre, rien d'autre touché.
#[test]
fn pure_data_copy_paste_keeps_working_without_header_markers() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, temps, 0);
    state.extend_selection(sheet1, signal, 1);
    let text = state.copy_selection(sheet1).expect("sélection non vide");
    assert_eq!(text, "0\t10\n0.5\t\n");

    state.select_cell(sheet1, temps, 3);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    assert_eq!(cell(&state, temps, 3), Cell::Float(0.0));
    assert_eq!(cell(&state, signal, 3), Cell::Float(10.0));
    assert_eq!(cell(&state, temps, 4), Cell::Float(0.5));
    assert!(matches!(cell(&state, signal, 4), Cell::Float(v) if v.is_nan()));
}

/// Un copier/coller purement champs d'en-tête (sélection entièrement dans
/// l'en-tête, sans toucher la moindre ligne de donnée) va bien sur les
/// champs de la colonne cible — jamais sur ses données.
#[test]
fn pure_header_field_copy_paste_writes_header_fields_not_data() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, temps, HeaderField::LongName, "T");
    set_header_field(&mut state, temps, HeaderField::Unit, "s");

    state.select_header_field(sheet1, temps, HeaderField::LongName);
    state.extend_header_field_selection(sheet1, temps, HeaderField::Unit);

    let text = state.copy_selection(sheet1).expect("sélection non vide");
    assert_eq!(text, "T\ns\n");

    let signal_data_before = [
        cell(&state, signal, 0),
        cell(&state, signal, 1),
        cell(&state, signal, 2),
    ];

    // Ancré sur un champ d'en-tête de `Signal` (même sorte que la copie) :
    // les champs s'appliquent, les données ne bougent pas.
    state.select_header_field(sheet1, signal, HeaderField::LongName);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    let meta = state.column_meta(signal);
    assert_eq!(meta.long_name, "T");
    assert_eq!(meta.unit, "s");
    assert_eq!(cell(&state, signal, 0), signal_data_before[0]);
    assert_eq!(cell(&state, signal, 1), signal_data_before[1]);
    assert_eq!(cell(&state, signal, 2), signal_data_before[2]);
}

/// Un copier de champs d'en-tête collé sur une cellule de donnée ne colle
/// rien : les deux sortes ne se mélangent jamais (règle 2 de la matrice de
/// comportement, voir `plotyx_app::state`) — le pendant de
/// [`pure_header_field_copy_paste_writes_header_fields_not_data`] côté
/// ancre de l'autre sorte.
#[test]
fn header_field_copy_pasted_onto_a_data_cell_anchor_pastes_nothing() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, temps, HeaderField::LongName, "T");
    set_header_field(&mut state, temps, HeaderField::Unit, "s");
    state.select_header_field(sheet1, temps, HeaderField::LongName);
    state.extend_header_field_selection(sheet1, temps, HeaderField::Unit);
    let text = state.copy_selection(sheet1).expect("sélection non vide");

    let signal_data_before = [cell(&state, signal, 0), cell(&state, signal, 1)];

    state.select_cell(sheet1, signal, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    let meta = state.column_meta(signal);
    assert_eq!(meta.long_name, "", "rien ne se colle : sortes différentes");
    assert_eq!(meta.unit, "");
    assert_eq!(cell(&state, signal, 0), signal_data_before[0]);
    assert_eq!(cell(&state, signal, 1), signal_data_before[1]);
}

/// Bug signalé : coller un rectangle de plusieurs champs d'en-tête sur un
/// seul champ sélectionné doit étirer le rectangle depuis cette ancre —
/// comme un tableur de référence — pas retomber sur les champs d'origine
/// (ce qui laissait tous les champs sauf le premier vides).
#[test]
fn pasting_a_header_only_copy_onto_a_single_header_field_anchors_and_offsets_fields() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, temps, HeaderField::LongName, "a");
    set_header_field(&mut state, temps, HeaderField::Unit, "b");
    set_header_field(&mut state, temps, HeaderField::Comment, "c");

    state.select_header_field(sheet1, temps, HeaderField::LongName);
    state.extend_header_field_selection(sheet1, temps, HeaderField::Comment);
    let text = state.copy_selection(sheet1).expect("sélection non vide");
    assert_eq!(text, "a\nb\nc\n");

    // Ancré sur le champ Nom long de `Signal` (le même rang que le premier
    // champ copié) : les trois champs s'appliquent, décalage nul.
    state.select_header_field(sheet1, signal, HeaderField::LongName);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: text.clone(),
    });
    let meta = state.column_meta(signal);
    assert_eq!(meta.long_name, "a");
    assert_eq!(meta.unit, "b");
    assert_eq!(meta.comment, "c");

    // Ancré sur Unité cette fois (un rang plus bas) : le rectangle se
    // décale d'autant — Unité reçoit la première ligne copiée, Commentaire
    // la deuxième, et la troisième (qui débordait sur Formule) s'y écrit
    // aussi puisqu'elle reste dans les 4 champs.
    let note = column_id(&state, sheet1, "Note");
    state.select_header_field(sheet1, note, HeaderField::Unit);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });
    let meta = state.column_meta(note);
    assert_eq!(meta.unit, "a");
    assert_eq!(meta.comment, "b");
    assert_eq!(meta.formula, Some("c".to_string()));
}

// -- Matrice de comportement copier/coller (§3, voir la doc de tête de
// `plotyx_app::state`) --
//
// Les tests plus haut couvrent déjà plusieurs lignes de la matrice (case
// unique → `Text`, rectangle relatif de données ou de champs, débordement
// en colonnes, croisement en-tête/données qui ne colle rien). Ceux qui
// suivent couvrent le reste : `Text` externe sur n'importe quelle ancre,
// `Ctrl+Maj+C` (`AppState::copy_full_columns`), l'autre sens du croisement
// en-tête/données, et `Ctrl+Maj+V` (`Action::PasteAbsolute`) pour un
// rectangle relatif comme pour des colonnes entières.

/// Une seule case copiée — ici un champ d'en-tête — devient `Text` : son
/// texte seul. C'est ce qui permet de copier la formule d'une colonne dans
/// le champ Formule d'une autre, l'exemple qui motive la règle (voir la doc
/// de tête de `plotyx_app::state`).
#[test]
fn copying_a_single_formula_field_can_paste_it_into_another_columns_formula_field() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, temps, HeaderField::Formula, "A*2");
    state.select_header_field(sheet1, temps, HeaderField::Formula);
    let text = state.copy_selection(sheet1).expect("sélection non vide");
    assert_eq!(
        text, "A*2",
        "une seule case copiée : son texte, rien d'autre"
    );

    state.select_header_field(sheet1, signal, HeaderField::Formula);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    assert_eq!(state.column_meta(signal).formula, Some("A*2".to_string()));
}

/// Règle 1 : un texte externe sans tabulation ni retour à la ligne (jamais
/// passé par `copy_selection`, donc jamais reconnu comme notre presse-
/// papier) s'écrit dans la case ancrée, quelle qu'elle soit — champ
/// d'en-tête comme cellule de donnée.
#[test]
fn external_plain_text_writes_into_whichever_anchor_is_selected() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_header_field(sheet1, temps, HeaderField::LongName);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "Capteur 1".to_string(),
    });
    assert_eq!(state.column_meta(temps).long_name, "Capteur 1");

    state.select_cell(sheet1, temps, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "42".to_string(),
    });
    assert_eq!(cell(&state, temps, 0), Cell::Float(42.0));
}

/// Ctrl+Maj+C promeut en colonne entière (en-tête + corps) la colonne
/// touchée par une sélection d'une seule cellule.
#[test]
fn copy_full_columns_promotes_the_touched_columns_from_a_single_cell_selection() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    set_header_field(&mut state, temps, HeaderField::Unit, "s");
    state.select_cell(sheet1, temps, 2);

    let text = state.copy_full_columns(sheet1).expect("sélection non vide");
    // 4 lignes d'en-tête (Nom long, Unité, Commentaire, Formule) puis tout
    // le corps de `Temps`, indépendamment de la cellule sélectionnée.
    assert_eq!(text, "\ns\n\n\n0\n0.5\n1\n1.5\n2\n");
}

/// Symétrique de [`header_field_copy_pasted_onto_a_data_cell_anchor_pastes_nothing`] :
/// des lignes de données copiées, collées sur un champ d'en-tête (sortes
/// différentes), ne collent rien non plus.
#[test]
fn relative_data_rows_pasted_onto_a_header_field_anchor_pastes_nothing() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, temps, 0);
    state.extend_selection(sheet1, signal, 1);
    let text = state.copy_selection(sheet1).expect("sélection non vide");

    let unit_before = state.column_meta(temps).unit;
    state.select_header_field(sheet1, temps, HeaderField::Unit);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    assert_eq!(state.column_meta(temps).unit, unit_before);
}

/// Ctrl+Maj+V sur un rectangle relatif de champs d'en-tête ignore l'ancre :
/// chaque champ retombe sur son rang d'origine plutôt que de se décaler.
#[test]
fn paste_absolute_replays_relative_header_fields_on_their_original_rank() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, temps, HeaderField::Unit, "s");
    set_header_field(&mut state, temps, HeaderField::Comment, "x");
    state.select_header_field(sheet1, temps, HeaderField::Unit);
    state.extend_header_field_selection(sheet1, temps, HeaderField::Comment);
    state.copy_selection(sheet1).expect("sélection non vide");

    // Ancré sur Nom long (au-dessus du rang d'origine) : en relatif ça
    // décalerait tout vers le haut ; Ctrl+Maj+V ignore l'ancre.
    state.select_header_field(sheet1, signal, HeaderField::LongName);
    state.dispatch(Action::PasteAbsolute { sheet: sheet1 });

    let meta = state.column_meta(signal);
    assert_eq!(meta.long_name, "", "Nom long n'était pas dans la copie");
    assert_eq!(meta.unit, "s");
    assert_eq!(meta.comment, "x");
}

/// Ctrl+Maj+V sur un rectangle relatif de données ignore l'ancre : chaque
/// ligne retombe sur sa ligne d'origine plutôt que de se décaler — même
/// avec une ancre choisie ailleurs.
#[test]
fn paste_absolute_replays_relative_data_rows_on_their_original_row() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    // Copie les lignes 2 et 3, puis les modifie : Ctrl+Maj+V doit les
    // restaurer à leur rang d'origine, pas à partir de l'ancre choisie plus
    // bas (la ligne 0).
    state.select_cell(sheet1, temps, 2);
    state.extend_selection(sheet1, temps, 3);
    state.copy_selection(sheet1).expect("sélection non vide");
    state.dispatch(Action::CommitCell {
        column: temps,
        row: 2,
        text: "99".to_string(),
    });
    state.dispatch(Action::CommitCell {
        column: temps,
        row: 3,
        text: "98".to_string(),
    });

    state.select_cell(sheet1, temps, 0);
    state.dispatch(Action::PasteAbsolute { sheet: sheet1 });

    assert_eq!(
        cell(&state, temps, 0),
        Cell::Float(0.0),
        "l'ancre (ligne 0) n'a pas reçu la première ligne copiée"
    );
    assert_eq!(
        cell(&state, temps, 2),
        Cell::Float(1.0),
        "restaurée à son rang d'origine"
    );
    assert_eq!(cell(&state, temps, 3), Cell::Float(1.5));
}

/// Ctrl+V d'une copie de colonnes entières, ancrée sur un champ d'en-tête :
/// les champs à partir de l'ancre (incluse) s'appliquent identité, ceux
/// au-dessus sont jetés ; le corps se colle sans décalage.
#[test]
fn full_columns_pasted_onto_a_header_field_anchor_applies_from_that_field_down_with_an_unshifted_body(
) {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, temps, HeaderField::LongName, "T");
    set_header_field(&mut state, temps, HeaderField::Unit, "s");
    set_header_field(&mut state, temps, HeaderField::Comment, "x");
    state.select_cell(sheet1, temps, 0);
    let text = state.copy_full_columns(sheet1).expect("sélection non vide");

    state.select_header_field(sheet1, signal, HeaderField::Unit);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    let meta = state.column_meta(signal);
    assert_eq!(meta.long_name, "", "au-dessus de l'ancre (Unité) : jeté");
    assert_eq!(meta.unit, "s");
    assert_eq!(meta.comment, "x");
    assert_eq!(
        cell(&state, signal, 0),
        Cell::Float(0.0),
        "corps collé sans décalage"
    );
    assert_eq!(cell(&state, signal, 1), Cell::Float(0.5));
}

/// Ctrl+V d'une copie de colonnes entières, ancrée sur une cellule de
/// donnée : l'en-tête est entièrement jeté, le corps se décale sur l'ancre.
#[test]
fn full_columns_pasted_onto_a_data_cell_anchor_discards_header_and_shifts_the_body() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, temps, HeaderField::LongName, "T");
    state.select_cell(sheet1, temps, 0);
    let text = state.copy_full_columns(sheet1).expect("sélection non vide");

    let signal_long_name_before = state.column_meta(signal).long_name;
    state.select_cell(sheet1, signal, 2);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    assert_eq!(
        state.column_meta(signal).long_name,
        signal_long_name_before,
        "l'en-tête est entièrement jeté quand l'ancre est une cellule de donnée"
    );
    assert_eq!(
        cell(&state, signal, 2),
        Cell::Float(0.0),
        "1re ligne copiée -> ligne d'ancrage"
    );
    assert_eq!(cell(&state, signal, 3), Cell::Float(0.5));
    assert_eq!(
        cell(&state, signal, 0),
        Cell::Float(10.0),
        "avant l'ancre : inchangé"
    );
}

/// Ctrl+Maj+V d'une copie de colonnes entières ignore l'ancre dans les deux
/// dimensions à la fois : l'en-tête s'applique quand même (contrairement à
/// Ctrl+V sur une ancre de donnée) et le corps ne se décale pas
/// (contrairement à Ctrl+V sur une ancre de donnée non plus).
#[test]
fn paste_absolute_of_full_columns_applies_everything_at_its_origin_regardless_of_anchor() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, temps, HeaderField::LongName, "T");
    state.select_cell(sheet1, temps, 0);
    state.copy_full_columns(sheet1).expect("sélection non vide");

    state.select_cell(sheet1, signal, 2);
    state.dispatch(Action::PasteAbsolute { sheet: sheet1 });

    assert_eq!(
        state.column_meta(signal).long_name,
        "T",
        "l'en-tête s'applique même avec une ancre de donnée"
    );
    assert_eq!(
        cell(&state, signal, 0),
        Cell::Float(0.0),
        "corps à son rang d'origine"
    );
    assert_eq!(
        cell(&state, signal, 2),
        Cell::Float(1.0),
        "pas décalé sur l'ancre (ligne 2) : reçoit Temps[2], pas Temps[0]"
    );
}

/// Un rectangle de champs décalé sur l'ancre peut déborder après Formule,
/// le dernier des 4 champs : ce qui dépasse est **jeté** silencieusement,
/// jamais replié sur les données ni écrasé sur Formule. Chemin de perte de
/// données discrète, donc verrouillé ici.
#[test]
fn pasting_relative_header_fields_drops_what_overflows_past_the_formula_field() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, temps, HeaderField::LongName, "a");
    set_header_field(&mut state, temps, HeaderField::Unit, "b");
    set_header_field(&mut state, temps, HeaderField::Comment, "c");
    set_header_field(&mut state, signal, HeaderField::LongName, "intact");
    set_header_field(&mut state, signal, HeaderField::Unit, "intact aussi");

    state.select_header_field(sheet1, temps, HeaderField::LongName);
    state.extend_header_field_selection(sheet1, temps, HeaderField::Comment);
    let text = state.copy_selection(sheet1).expect("sélection non vide");

    // Ancré sur Commentaire : « a » y tombe, « b » sur Formule, « c »
    // déborderait après Formule.
    state.select_header_field(sheet1, signal, HeaderField::Comment);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    let meta = state.column_meta(signal);
    assert_eq!(meta.comment, "a");
    assert_eq!(meta.formula, Some("b".to_string()));
    assert_eq!(
        meta.long_name, "intact",
        "les champs au-dessus de l'ancre ne sont pas touchés"
    );
    assert_eq!(meta.unit, "intact aussi");
    // « c » n'a atterri nulle part : ni dans un champ, ni dans la colonne
    // voisine (le rectangle copié ne fait qu'une colonne de large).
    assert!(
        [&meta.long_name, &meta.unit, &meta.comment]
            .iter()
            .all(|value| *value != "c")
            && meta.formula.as_deref() != Some("c"),
        "« c » débordait après Formule : il est jeté, pas replié ailleurs"
    );
    let note = column_id(&state, sheet1, "Note");
    assert_eq!(
        state.column_meta(note).long_name,
        "",
        "la colonne voisine n'est pas touchée non plus"
    );
}

/// Un collage qui touche plusieurs colonnes et plusieurs champs ne compte
/// que pour **une** entrée d'historique : une seule annulation doit tout
/// remettre en place, pas une par champ écrit.
#[test]
fn pasting_full_columns_counts_as_a_single_history_entry() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    set_header_field(&mut state, temps, HeaderField::LongName, "T");
    set_header_field(&mut state, temps, HeaderField::Unit, "s");

    // Deux colonnes copiées en entier (en-tête + corps).
    state.select_cell(sheet1, temps, 0);
    state.extend_selection(sheet1, signal, 0);
    let text = state.copy_full_columns(sheet1).expect("sélection non vide");

    let signal_before = state.column_meta(signal).clone();
    let note_before = state.column_meta(note).clone();
    let signal_data_before = cell(&state, signal, 0);
    let note_data_before = cell(&state, note, 0);

    // Collé à partir de `Signal` : déborde sur `Note`, et écrit les 4
    // champs de chacune — largement plus d'une opération.
    state.select_header_field(sheet1, signal, HeaderField::LongName);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });
    // `Signal` reçoit les champs de `Temps`, et le débordement donne à
    // `Note` ceux de `Signal` : plusieurs colonnes, plusieurs champs, plus
    // les deux corps — bien plus d'une opération.
    assert_eq!(state.column_meta(signal).long_name, "T");
    assert_eq!(state.column_meta(signal).unit, "s");
    assert_eq!(cell(&state, signal, 0), Cell::Float(0.0));

    state.dispatch(Action::Undo);

    assert_eq!(
        state.column_meta(signal).long_name,
        signal_before.long_name,
        "une seule annulation doit défaire tout le collage"
    );
    assert_eq!(state.column_meta(signal).unit, signal_before.unit);
    assert_eq!(state.column_meta(note).long_name, note_before.long_name);
    assert_eq!(cell(&state, signal, 0), signal_data_before);
    assert_eq!(cell(&state, note, 0), note_data_before);
}

/// Ctrl+Maj+V rejoue le presse-papier **interne** : sans copie préalable
/// dans cette session, il n'a aucun rang d'origine à rejouer. C'est un
/// avertissement, pas une erreur ni une panique.
#[test]
fn paste_absolute_without_a_prior_copy_is_a_notice_not_an_error() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let before = state.snapshot().revision;

    state.select_cell(sheet1, temps, 0);
    state.dispatch(Action::PasteAbsolute { sheet: sheet1 });

    let status = state.status().expect("un message");
    assert!(
        !status.is_error,
        "« rien à coller » est un état normal, pas une erreur"
    );
    assert_eq!(state.snapshot().revision, before, "le modèle n'a pas bougé");
}

/// Régression : Suppr vide un champ d'en-tête, comme Retour arrière. Un
/// champ n'est pas répété par ligne, donc « vider et faire remonter » n'y a
/// pas d'autre sens que « vider » — et la touche ne doit pas rester sans
/// effet dans l'en-tête.
#[test]
fn delete_on_a_header_field_selection_clears_it_like_backspace() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    set_header_field(&mut state, temps, HeaderField::LongName, "T");
    set_header_field(&mut state, temps, HeaderField::Unit, "s");

    // Un seul champ sélectionné.
    state.select_header_field(sheet1, temps, HeaderField::LongName);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });
    assert_eq!(state.column_meta(temps).long_name, "");
    assert_eq!(
        state.column_meta(temps).unit,
        "s",
        "seul le champ sélectionné est vidé"
    );
    assert_eq!(
        cell(&state, temps, 0),
        Cell::Float(0.0),
        "aucune donnée n'a remonté"
    );

    // Plage de champs : même règle.
    set_header_field(&mut state, temps, HeaderField::LongName, "T");
    state.select_header_field(sheet1, temps, HeaderField::LongName);
    state.extend_header_field_selection(sheet1, temps, HeaderField::Unit);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });
    assert_eq!(state.column_meta(temps).long_name, "");
    assert_eq!(state.column_meta(temps).unit, "");
    assert_eq!(cell(&state, temps, 0), Cell::Float(0.0));
}

/// Suppr en deux temps (`AppState::consume_delete_key`) : la première
/// pression sur une sélection vide la case sans rien décaler, exactement
/// comme Retour arrière ; la deuxième pression sur *cette même sélection*
/// supprime pour de bon et décale ce qui suit.
#[test]
fn pressing_delete_twice_on_the_same_selection_clears_then_shifts() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    // "Temps" = [0.0, 0.5, 1.0, 1.5, 2.0].
    state.select_cell(sheet1, temps, 2);

    // Première pression : vide, ne décale rien.
    let action = state
        .consume_delete_key(sheet1)
        .expect("une sélection est active");
    assert_eq!(action, Action::ClearSelectedCells { sheet: sheet1 });
    state.dispatch(action);
    assert!(
        matches!(cell(&state, temps, 2), Cell::Float(v) if v.is_nan()),
        "la case est vide après la première pression"
    );
    let temps_len = state
        .tree()
        .get(temps)
        .expect("colonne Temps présente")
        .as_column()
        .expect("nœud colonne")
        .len();
    assert_eq!(temps_len, 5, "rien n'a été décalé à la première pression");

    // Deuxième pression, même sélection : supprime et décale.
    let action = state
        .consume_delete_key(sheet1)
        .expect("la sélection n'a pas changé");
    assert_eq!(action, Action::DeleteSelectedCells { sheet: sheet1 });
    state.dispatch(action);
    let temps_len = state
        .tree()
        .get(temps)
        .expect("colonne Temps présente")
        .as_column()
        .expect("nœud colonne")
        .len();
    assert_eq!(temps_len, 4, "la deuxième pression décale bien ce qui suit");
    assert_eq!(cell(&state, temps, 2), Cell::Float(1.5));
}

/// Changer la sélection entre les deux pressions repart au premier temps
/// (vider) : la deuxième pression ne supprime que si elle vise *exactement*
/// la même sélection que la précédente.
#[test]
fn changing_selection_between_two_delete_presses_restarts_at_clearing() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_cell(sheet1, temps, 1);
    let action = state.consume_delete_key(sheet1).expect("sélection active");
    assert_eq!(action, Action::ClearSelectedCells { sheet: sheet1 });
    state.dispatch(action);

    // La sélection bouge avant la deuxième pression.
    state.select_cell(sheet1, temps, 3);
    let action = state.consume_delete_key(sheet1).expect("sélection active");
    assert_eq!(
        action,
        Action::ClearSelectedCells { sheet: sheet1 },
        "sélection différente : on repart au premier temps, pas une suppression"
    );
}

/// Une autre mutation de la grille (ici Retour arrière) entre les deux
/// pressions oublie la première, même si la sélection n'a pas bougé : une
/// pression sur Suppr qui suit ne doit pas supprimer par surprise.
#[test]
fn another_grid_mutation_between_two_delete_presses_forgets_the_first() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_cell(sheet1, temps, 1);
    let action = state.consume_delete_key(sheet1).expect("sélection active");
    state.dispatch(action);

    // Une autre mutation de la grille se glisse entre les deux pressions,
    // sur la même sélection — comme le ferait l'appelant (`app.rs`) avant
    // de dispatcher `Action::ClearSelectedCells` pour Retour arrière.
    state.forget_delete_repeat(sheet1);
    state.dispatch(Action::ClearSelectedCells { sheet: sheet1 });

    let action = state.consume_delete_key(sheet1).expect("sélection active");
    assert_eq!(
        action,
        Action::ClearSelectedCells { sheet: sheet1 },
        "la mutation intercalée a fait oublier la première pression"
    );
}

/// [`AppState::grid_menu`] — ce que le menu contextuel propose est décidé
/// ici, sans egui : la zone touchée, l'accord en nombre des libellés, et le
/// fait que « Supprimer et décaler » n'existe pas dans l'en-tête, où Suppr
/// n'a rien à faire remonter (voir [`AppState::grid_menu`] et la doc de
/// `GridMenu`).
#[test]
fn the_grid_menu_names_what_the_selection_actually_touches() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, temps, 1);
    let menu = state.grid_menu(sheet1).expect("une cellule sélectionnée");
    assert_eq!(menu.zone, GridZone::Data);
    assert!(!menu.plural, "une seule cellule");
    assert!(!menu.multi_column);
    assert_eq!(menu.target_label(), "la cellule");
    assert_eq!(menu.copy_full_columns_label(), "Copier la colonne entière");
    assert!(
        menu.offers_delete_shift,
        "Suppr fait remonter ce qui suit dans les données"
    );

    // Plusieurs lignes d'une seule colonne : pluriel en cases, mais toujours
    // une seule colonne entière à promouvoir.
    state.extend_selection(sheet1, temps, 3);
    let menu = state.grid_menu(sheet1).expect("un rectangle sélectionné");
    assert!(menu.plural);
    assert!(
        !menu.multi_column,
        "trois cases empilées ne touchent qu'une colonne"
    );
    assert_eq!(menu.target_label(), "les cellules");
    assert_eq!(menu.copy_full_columns_label(), "Copier la colonne entière");

    state.extend_selection(sheet1, signal, 3);
    let menu = state.grid_menu(sheet1).expect("un rectangle sélectionné");
    assert!(menu.multi_column);
    assert_eq!(
        menu.copy_full_columns_label(),
        "Copier les colonnes entières"
    );

    // En-tête : un champ n'est pas répété par ligne, rien à faire remonter.
    state.select_header_field(sheet1, temps, HeaderField::LongName);
    let menu = state.grid_menu(sheet1).expect("un champ sélectionné");
    assert_eq!(menu.zone, GridZone::Header);
    assert_eq!(menu.target_label(), "le champ");
    assert!(
        !menu.offers_delete_shift,
        "dans l'en-tête, Suppr ne fait que vider : deux entrées pour un même effet n'auraient pas de sens"
    );
    state.extend_header_field_selection(sheet1, temps, HeaderField::Unit);
    assert_eq!(
        state
            .grid_menu(sheet1)
            .expect("une plage de champs")
            .target_label(),
        "les champs"
    );

    // Colonnes entières : une seule colonne reste au singulier.
    state.select_column(sheet1, temps);
    let menu = state.grid_menu(sheet1).expect("une colonne sélectionnée");
    assert_eq!(menu.zone, GridZone::Columns);
    assert_eq!(menu.target_label(), "la colonne");
    assert!(menu.offers_delete_shift);
    state.extend_column_selection(sheet1, signal);
    assert_eq!(
        state
            .grid_menu(sheet1)
            .expect("une plage de colonnes")
            .target_label(),
        "les colonnes"
    );
}

/// « Coller à l'identique » (Ctrl+Maj+V) rejoue le presse-papier **interne** :
/// le menu doit le griser tant que rien n'a été copié depuis la grille dans
/// cette session, plutôt que d'offrir une entrée qui ne ferait que poser une
/// notice (verrou du pendant menu de
/// [`paste_absolute_without_a_prior_copy_is_a_notice_not_an_error`]).
#[test]
fn the_grid_menu_greys_out_paste_absolute_until_something_was_copied() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_cell(sheet1, temps, 0);
    assert!(
        !state
            .grid_menu(sheet1)
            .expect("une cellule sélectionnée")
            .paste_absolute_enabled,
        "rien n'a encore été copié depuis la grille"
    );

    state.copy_selection(sheet1).expect("copie d'une cellule");
    assert!(
        state
            .grid_menu(sheet1)
            .expect("une cellule sélectionnée")
            .paste_absolute_enabled,
        "une copie depuis la grille donne de quoi rejouer"
    );
}

/// Sans sélection, pas de menu à ouvrir — l'appelant doit s'abstenir plutôt
/// que de poser un menu dont chaque entrée serait sans effet.
#[test]
fn the_grid_menu_is_absent_without_a_selection() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");

    assert!(state.grid_menu(sheet1).is_none());
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

/// La note détachée édite sur une vue locale : sa saisie n'atteint pas l'état
/// de la racine, et sa validation rejoint la voie de commande annulable.
#[test]
fn detached_note_view_keeps_its_buffer_local_and_commits_through_the_root() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let mut project = demo_project();
    let root = project.root();
    let note = project
        .add_note(root, Some("Note1"), "avant")
        .expect("note");
    let (_session, mut state) = start(project, &dir.path().join("projet.plotyx"));

    let mut detached = state.fork_for_detached_view().unwrap();
    detached.begin_note_edit(note);
    if let Some(edit) = detached.note_editing_mut() {
        edit.buffer = "après".to_string();
    }
    assert!(state.note_editing().is_none());
    assert_eq!(state.note_text(note), Some("avant".to_string()));

    state.dispatch(Action::SetNoteText {
        note,
        text: "après".to_string(),
    });
    detached.sync_detached_snapshot(state.snapshot().clone());
    assert_eq!(detached.note_text(note), Some("après".to_string()));
    assert!(state.can_undo());
    state.dispatch(Action::Undo);
    assert_eq!(state.note_text(note), Some("avant".to_string()));
}

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
        destination: ImportDestination::ActiveBook(book),
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
    assert_eq!(state.column_meta(columns_a[0].0).role, ColumnRole::X);
    assert_eq!(state.column_meta(columns_a[1].0).role, ColumnRole::Y);
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

/// Les deux destinations qui créent des classeurs conservent la règle
/// « une feuille par fichier » et le lot entier reste annulable d'un coup.
#[test]
fn importing_into_new_books_groups_files_as_requested() {
    for destination in [
        ImportDestination::NewBookPerFile,
        ImportDestination::NewBookForAll,
    ] {
        let dir = tempfile::tempdir().expect("répertoire temporaire");
        let (_session, mut state) = start_demo(dir.path());
        let root = state.tree().root();
        let before = state.tree().node(root).expect("racine").children().len();
        let a = dir.path().join("mesure_a.csv");
        let b = dir.path().join("mesure_b.csv");
        std::fs::write(&a, "1,10\n2,20\n").unwrap();
        std::fs::write(&b, "3,30\n4,40\n").unwrap();
        state.dispatch(Action::ImportFiles {
            destination,
            recipe: ImportRecipe::new("Sans titre"),
            paths: vec![a, b],
        });
        let added = state.tree().node(root).expect("racine").children().len() - before;
        let expected = if destination == ImportDestination::NewBookPerFile {
            2
        } else {
            1
        };
        assert_eq!(added, expected);
        let sheet_a = sheet_named(state.tree(), "mesure_a");
        let sheet_b = sheet_named(state.tree(), "mesure_b");
        let book_a = state.tree().node(sheet_a).unwrap().parent().unwrap();
        let book_b = state.tree().node(sheet_b).unwrap().parent().unwrap();
        assert_eq!(
            book_a == book_b,
            destination == ImportDestination::NewBookForAll
        );
        assert_eq!(
            state.worksheets_of(book_a).len(),
            if expected == 1 { 2 } else { 1 }
        );
        assert_eq!(
            state.worksheets_of(book_b).len(),
            if expected == 1 { 2 } else { 1 }
        );
        assert_eq!(
            cell_f64(&state, state.visible_columns(sheet_b)[0].0, 0),
            3.0
        );
        state.dispatch(Action::Undo);
        assert_eq!(
            state.tree().node(root).expect("racine").children().len(),
            before
        );
    }
}

/// La vue détachée garde ses sélections et ses copies, puis fournit ce
/// contexte à la voie de commande principale avant un collage différé.
#[test]
fn detached_book_view_reuses_the_grid_selection_and_clipboard() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheet = sheet_named(state.tree(), "Sheet1");
    let columns = state.visible_columns(sheet);
    let source = columns[1].0;
    let target = columns[0].0;
    let mut detached = state.fork_for_detached_view().unwrap();
    detached.select_cell(sheet, source, 0);
    let copied = detached.copy_selection(sheet).unwrap();
    state.adopt_detached_book_view(&mut detached, book);
    assert_eq!(state.selection(sheet), detached.selection(sheet));
    state.select_cell(sheet, target, 1);
    state.dispatch(Action::Paste {
        sheet,
        text: copied,
    });
    assert_eq!(cell_f64(&state, target, 1), cell_f64(&state, source, 0));
}

/// Un fichier illisible ne produit pas de classeur vide ; un fichier valide
/// ultérieur du même lot est quand même importé.
#[test]
fn new_book_import_creates_nothing_for_an_unreadable_file() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let root = state.tree().root();
    let before = state.tree().node(root).unwrap().children().len();
    let valid = dir.path().join("valide.csv");
    std::fs::write(&valid, "1,2\n").unwrap();
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::NewBookPerFile,
        recipe: ImportRecipe::new("Sans titre"),
        paths: vec![dir.path().join("absent.csv"), valid],
    });
    assert_eq!(
        state.tree().node(root).unwrap().children().len(),
        before + 1
    );
    assert!(state.status().unwrap().is_error);
}

#[test]
fn import_without_an_active_book_defaults_to_one_new_book_for_the_batch() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    let import = state.import_state().unwrap();
    assert_eq!(import.active_book, None);
    assert_eq!(import.destination, ImportDestination::NewBookForAll);
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
        destination: ImportDestination::ActiveBook(book),
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
    // Les modèles fournis d'office sont listés, mais rien de l'utilisateur
    // n'a été ajouté ni écrit.
    assert!(
        plotyx_import::RecipeStore::load().recipes().is_empty(),
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
    // Les modèles de l'utilisateur (hors modèles fournis d'office).
    let user_recipes = plotyx_import::RecipeStore::load();
    let names: Vec<&str> = user_recipes
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
        destination: ImportDestination::ActiveBook(book),
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
        destination: ImportDestination::ActiveBook(book),
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
        destination: ImportDestination::ActiveBook(book),
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
        destination: ImportDestination::ActiveBook(book),
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
        destination: ImportDestination::ActiveBook(NodeId::from_raw(999_999)),
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
    // Le texte est dans la première colonne (le X) : la coupure automatique
    // (issue #4) s'y arrêterait, d'où l'option « Aucune coupure ».
    let mut recipe = ImportRecipe::new("Sans titre");
    recipe.data_end = plotyx_import::DataEnd::None;

    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
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
        destination: ImportDestination::ActiveBook(book),
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
    assert_eq!(column_type(&state, note), ColumnType::Mixed);

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
    assert_eq!(column_type(&state, note), ColumnType::Mixed);

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
        ColumnType::Mixed,
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
/// la sélection en cours est déjà une plage de champs) s'arrête à la
/// frontière plutôt que de la traverser : le rectangle couvre les champs
/// entre l'ancre et Formule (le dernier champ d'en-tête), sur la colonne
/// visée comme sur celle de l'ancre, mais ne touche jamais une donnée.
/// Trois correctifs successifs avaient fait vivre un rectangle mixte qui
/// traversait cette frontière ; voir la doc de tête de `crate::selection`
/// pour pourquoi il a été retiré.
#[test]
fn dragging_a_cell_extension_across_the_header_boundary_stops_at_the_formula_field() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_header_field(sheet1, temps, HeaderField::Unit);
    state.extend_selection(sheet1, signal, 2);

    assert!(
        !state.is_selected(sheet1, signal, 2),
        "la cellule visée n'est plus jamais incluse : le glisser s'arrête à la frontière"
    );
    assert!(
        state.is_header_field_selected(sheet1, temps, HeaderField::Unit),
        "le champ d'ancrage reste sélectionné"
    );
    assert!(
        state.is_header_field_selected(sheet1, temps, HeaderField::Comment),
        "champ intermédiaire entre l'ancre et Formule inclus"
    );
    assert!(
        state.is_header_field_selected(sheet1, signal, HeaderField::Formula),
        "le rectangle s'étend jusqu'à Formule sur la colonne visée"
    );
    assert!(
        !state.is_selected(sheet1, temps, 0),
        "aucune donnée de la colonne d'ancrage n'est incluse"
    );
    assert_eq!(
        state.selected_header_field(sheet1),
        None,
        "un rectangle de plusieurs cases n'est jamais réduit à un seul champ"
    );
}

/// Symétrique : glisser depuis une cellule de donnée vers un champ d'en-tête
/// s'arrête à la ligne 0, ne remonte jamais dans l'en-tête.
#[test]
fn dragging_a_header_field_extension_across_the_data_boundary_stops_at_row_zero() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_cell(sheet1, temps, 3);
    state.extend_header_field_selection(sheet1, temps, HeaderField::Comment);

    assert!(
        !state.is_header_field_selected(sheet1, temps, HeaderField::Comment),
        "le champ visé n'est jamais inclus : le glisser s'arrête à la frontière"
    );
    for row in 0..=3 {
        assert!(state.is_selected(sheet1, temps, row), "ligne {row} incluse");
    }
}

/// Une sélection démarrée sur le champ Commentaire et étendue jusqu'à une
/// ligne de données de la même colonne s'arrête à Formule (le dernier
/// champ d'en-tête) : elle ne touche jamais la moindre ligne de données.
/// Régression du bug historique (trois correctifs successifs) du
/// « rectangle mixte », voir la doc de tête de `crate::selection`.
#[test]
fn extending_from_a_header_field_into_data_stops_before_any_data_row() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_header_field(sheet1, temps, HeaderField::Comment);
    state.extend_selection(sheet1, temps, 4);

    assert!(state.is_header_field_selected(sheet1, temps, HeaderField::Comment));
    assert!(
        state.is_header_field_selected(sheet1, temps, HeaderField::Formula),
        "Formule, dernier champ d'en-tête, atteinte"
    );
    assert!(
        !state.is_header_field_selected(sheet1, temps, HeaderField::Unit),
        "Unité est au-dessus de l'ancre : hors du rectangle"
    );
    for row in 0..=4 {
        assert!(
            !state.is_selected(sheet1, temps, row),
            "ligne {row} jamais incluse : le glisser s'est arrêté à Formule"
        );
    }

    let text = state.copy_selection(sheet1).expect("sélection non vide");
    assert_eq!(
        text.lines().count(),
        2,
        "2 champs d'en-tête (Commentaire, Formule), aucune ligne de donnée"
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

/// Tab depuis le champ Formule de la **dernière** colonne visible entre
/// dans les données, à `Data(0)` de la **première** colonne — le pendant de
/// Tab pour la frontière en-tête/données, symétrique de Maj+Tab depuis
/// `Data(0)` de la première colonne (voir le test suivant).
#[test]
fn tab_from_the_last_column_formula_field_enters_the_data() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let note = column_id(&state, sheet1, "Note");

    state.select_header_field(sheet1, note, HeaderField::Formula);
    state.move_selection_tab(sheet1, false);

    assert!(
        state.is_selected(sheet1, temps, 0),
        "Tab depuis Formule de la dernière colonne rejoint Data(0) de la première"
    );
}

/// Maj+Tab depuis `Data(0)` de la première colonne visible rejoint le champ
/// Formule de la **dernière** colonne — le point d'entrée symétrique dans
/// l'en-tête.
#[test]
fn shift_tab_from_the_first_cell_reaches_the_last_column_formula_field() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let note = column_id(&state, sheet1, "Note");

    state.select_cell(sheet1, temps, 0);
    state.move_selection_tab(sheet1, true);

    assert_eq!(
        state.selected_header_field(sheet1),
        Some((note, HeaderField::Formula)),
    );
}

/// Tab circule normalement au sein de l'en-tête, colonne par colonne, puis
/// champ par champ une fois toutes les colonnes parcourues pour un champ
/// donné — sans jamais entrer dans les données tant que ce n'est pas la
/// dernière colonne au dernier champ.
#[test]
fn tab_cycles_through_header_fields_before_reaching_data() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_header_field(sheet1, temps, HeaderField::LongName);
    state.move_selection_tab(sheet1, false);
    assert_eq!(
        state.selected_header_field(sheet1),
        Some((signal, HeaderField::LongName)),
        "Tab avance d'abord de colonne en colonne pour un même champ"
    );
}

// -- 17. Sélection de lignes entières (gouttière des numéros de ligne) --

/// Un clic sur la gouttière (`select_row`) sélectionne toute la ligne :
/// toutes les colonnes visibles de la feuille, à cette ligne — miroir de
/// [`extending_a_column_selection_covers_the_whole_range`] mais pour une
/// seule ligne.
#[test]
fn clicking_the_row_gutter_selects_the_whole_row() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_row(sheet1, 1);

    assert!(state.is_row_selected(sheet1, 1));
    assert!(!state.is_row_selected(sheet1, 0), "ligne 0 non touchée");
    assert!(state.is_selected(sheet1, temps, 1));
    assert!(state.is_selected(sheet1, signal, 1));
    assert!(
        state.is_selected(sheet1, note, 1),
        "Note est plus courte que les deux autres colonnes, mais reste \
         touchée par la sélection de ligne : une ligne entière couvre \
         toutes les colonnes visibles, pas seulement celles assez longues"
    );
    assert!(
        !state.is_selected(sheet1, temps, 0),
        "seule la ligne 1 est sélectionnée"
    );
}

/// Maj-clic ou glisser sur la gouttière (`extend_row_selection`) étend la
/// plage à plusieurs lignes — miroir de
/// [`extending_a_column_selection_covers_the_whole_range`].
#[test]
fn extending_a_row_selection_covers_the_whole_range() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_row(sheet1, 1);
    state.extend_row_selection(sheet1, 3);

    for row in 1..=3 {
        assert!(state.is_row_selected(sheet1, row), "ligne {row} incluse");
        assert!(state.is_selected(sheet1, temps, row));
    }
    assert!(!state.is_row_selected(sheet1, 0));
    assert!(!state.is_row_selected(sheet1, 4));

    // Le sens du glisser ne compte pas : reglisser vers le haut couvre la
    // même plage.
    state.select_row(sheet1, 3);
    state.extend_row_selection(sheet1, 1);
    for row in 1..=3 {
        assert!(state.is_row_selected(sheet1, row));
    }
}

/// Retour arrière (`Action::ClearSelectedCells`) sur une plage de lignes
/// vide toutes les colonnes touchées à cette ligne, sans rien décaler —
/// même règle que pour un rectangle de cellules.
#[test]
fn backspace_on_a_full_row_selection_clears_it_without_shifting() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    // "Temps" = [0.0, 0.5, 1.0, 1.5, 2.0].
    state.select_row(sheet1, 1);
    state.dispatch(Action::ClearSelectedCells { sheet: sheet1 });

    assert!(
        matches!(cell(&state, temps, 1), Cell::Float(v) if v.is_nan()),
        "la ligne 1 est vide"
    );
    assert_eq!(
        cell(&state, temps, 2),
        Cell::Float(1.0),
        "rien n'a été décalé"
    );
    assert_eq!(state.row_count(sheet1), 5, "aucune ligne retirée");
}

/// Suppr (`Action::DeleteSelectedCells`) sur une plage de lignes supprime la
/// ligne et fait remonter ce qui suit, dans **toutes** les colonnes
/// touchées, indépendamment de leur longueur propre.
#[test]
fn delete_on_a_full_row_selection_shifts_every_column_up() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    // "Temps" = [0.0, 0.5, 1.0, 1.5, 2.0], "Signal" = [10, NaN, 30, 40, 50],
    // "Note" = [Some("a"), None, Some("")] (plus courte que les deux
    // autres : ligne 1 est dans ses bornes).
    state.select_row(sheet1, 1);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });

    assert_eq!(state.row_count(sheet1), 4);
    assert_eq!(
        cell(&state, temps, 1),
        Cell::Float(1.0),
        "la ligne 2 d'origine (1.0) a pris la place de la ligne 1 supprimée"
    );
    assert_eq!(
        cell(&state, temps, 3),
        Cell::Float(2.0),
        "dernière ligne inchangée, juste décalée"
    );
    assert!(matches!(cell(&state, signal, 1), Cell::Float(v) if (v - 30.0).abs() < f64::EPSILON));
    assert_eq!(
        state.cell_text(note, 1),
        "",
        "Note[2] (chaîne vide présente) a remonté en position 1"
    );
}

/// Le cas limite explicitement visé par la tâche : supprimer une ligne
/// *au-delà* de la fin d'une colonne plus courte que les autres (`Note`, 3
/// valeurs sur une feuille de 5 lignes) ne doit ni paniquer ni toucher cette
/// colonne — `Command::DeleteCells` applique `at`/`count` indépendamment par
/// colonne, une colonne plus courte que `at` n'a rien à retirer.
#[test]
fn deleting_the_last_row_leaves_a_shorter_column_untouched() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    let note_before = [
        state.cell_text(note, 0),
        state.cell_text(note, 1),
        state.cell_text(note, 2),
    ];

    // Ligne 4 (la dernière, 0-indexée) : hors des bornes de `Note` (longueur
    // 3), mais bien dans celles de `Temps`/`Signal` (longueur 5).
    state.select_row(sheet1, 4);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });

    assert_eq!(
        state.row_count(sheet1),
        4,
        "Temps/Signal ont perdu une ligne"
    );
    assert_eq!(
        cell(&state, temps, 3),
        Cell::Float(1.5),
        "la dernière ligne restante est l'avant-dernière d'origine"
    );
    assert!(matches!(cell(&state, signal, 3), Cell::Float(v) if (v - 40.0).abs() < f64::EPSILON));
    assert_eq!(
        [
            state.cell_text(note, 0),
            state.cell_text(note, 1),
            state.cell_text(note, 2),
        ],
        note_before,
        "Note, plus courte que la ligne supprimée, n'a pas bougé"
    );
}

/// [`AppState::grid_menu`] sur une plage de lignes rend la zone `Rows` et
/// les bons libellés, singulier et pluriel — le pluriel n'y suit que le
/// nombre de *lignes*, jamais le nombre de colonnes (voir la doc de
/// [`GridMenu::plural`]) : une seule ligne sur une feuille à 3 colonnes
/// reste « la ligne ».
#[test]
fn the_grid_menu_on_a_full_row_selection_names_rows() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");

    state.select_row(sheet1, 1);
    let menu = state.grid_menu(sheet1).expect("une ligne sélectionnée");
    assert_eq!(menu.zone, GridZone::Rows);
    assert!(
        !menu.plural,
        "une seule ligne reste singulière même si elle touche 3 colonnes"
    );
    assert_eq!(menu.target_label(), "la ligne");
    assert!(
        menu.offers_delete_shift,
        "Suppr fait remonter ce qui suit dans toutes les colonnes"
    );

    state.extend_row_selection(sheet1, 3);
    let menu = state.grid_menu(sheet1).expect("une plage de lignes");
    assert!(menu.plural, "plusieurs lignes couvertes");
    assert_eq!(menu.target_label(), "les lignes");
}

/// Copier une ligne entière (Ctrl+C sur la gouttière) puis la coller
/// ailleurs colle chaque colonne à sa place, comme un rectangle de cellules
/// — c'est la matrice `full_rank_range`/`DataRows` de [`AppState::
/// copy_selection`]/[`AppState::paste`] qui s'applique, pas la branche
/// « plage de colonnes entières » (qui n'a pas de sens pour une ligne).
#[test]
fn copying_a_full_row_and_pasting_elsewhere_pastes_the_whole_row() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_row(sheet1, 0);
    let text = state.copy_selection(sheet1).expect("ligne non vide");
    assert_eq!(text, "0\t10\ta\n");

    state.select_row(sheet1, 1);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    assert_eq!(cell(&state, temps, 1), Cell::Float(0.0));
    assert!(matches!(cell(&state, signal, 1), Cell::Float(v) if (v - 10.0).abs() < f64::EPSILON));
    assert_eq!(state.cell_text(note, 1), "a");
    assert_eq!(
        cell(&state, temps, 0),
        Cell::Float(0.0),
        "la ligne copiée elle-même n'a pas bougé"
    );
    assert_eq!(
        cell(&state, temps, 2),
        Cell::Float(1.0),
        "les lignes qui n'ont pas reçu le collage n'ont pas bougé"
    );
}

/// Glisser depuis une cellule de données vers la gouttière repose une
/// sélection d'une seule ligne à la cible plutôt que d'être ignorée — pendant
/// de [`dragging_a_column_extension_across_the_data_boundary_reselects_a_single_column`].
#[test]
fn dragging_a_row_extension_across_the_data_boundary_reselects_a_single_row() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_cell(sheet1, temps, 0);
    state.extend_row_selection(sheet1, 3);

    assert!(
        state.is_row_selected(sheet1, 3),
        "la ligne visée devient la nouvelle sélection"
    );
    assert!(
        !state.is_selected(sheet1, temps, 0),
        "l'ancienne sélection de cellule n'a plus cours"
    );
}

/// Glisser depuis la gouttière vers une cellule de données repose une
/// sélection d'une seule cellule à la cible plutôt que d'étendre
/// silencieusement la plage de lignes sans effet — pendant de
/// [`dragging_a_column_extension_across_the_data_boundary_reselects_a_single_column`]
/// côté colonnes.
#[test]
fn dragging_a_cell_extension_across_the_row_gutter_boundary_reselects_a_single_cell() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_row(sheet1, 1);
    state.extend_selection(sheet1, temps, 3);

    assert_eq!(
        state.selection(sheet1).and_then(Selection::single_cell),
        Some((temps, 3)),
        "la cellule visée devient la nouvelle sélection"
    );
    assert!(
        !state.is_row_selected(sheet1, 1),
        "l'ancienne plage de lignes n'a plus cours"
    );
}

/// Glisser depuis la rangée des lettres vers la gouttière repose une
/// sélection d'une seule ligne, et réciproquement — les deux plages
/// entières ne se traversent jamais l'une l'autre non plus.
#[test]
fn dragging_between_the_letter_row_and_the_row_gutter_never_mixes_the_two_ranges() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_column(sheet1, temps);
    state.extend_row_selection(sheet1, 2);
    assert!(state.is_row_selected(sheet1, 2));
    assert!(!state.is_column_selected(sheet1, temps));

    state.select_row(sheet1, 2);
    state.extend_column_selection(sheet1, temps);
    assert!(state.is_column_selected(sheet1, temps));
    assert!(!state.is_row_selected(sheet1, 2));
}

// -- 18. Sélection de champ d'en-tête entier (libellés de la gouttière) --

/// Un clic sur un libellé de champ dans la gouttière (`select_header_row`)
/// sélectionne ce champ pour **toutes** les colonnes visibles de la feuille
/// — miroir de [`clicking_the_row_gutter_selects_the_whole_row`], côté
/// en-tête.
#[test]
fn clicking_a_header_row_label_selects_the_field_in_every_column() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    set_header_field(&mut state, temps, HeaderField::LongName, "T");
    set_header_field(&mut state, signal, HeaderField::LongName, "S");

    state.select_header_row(sheet1, HeaderField::LongName);

    assert!(state.is_header_row_selected(sheet1, HeaderField::LongName));
    assert!(!state.is_header_row_selected(sheet1, HeaderField::Unit));
    assert!(state.is_header_field_selected(sheet1, temps, HeaderField::LongName));
    assert!(state.is_header_field_selected(sheet1, signal, HeaderField::LongName));
    assert!(
        state.is_header_field_selected(sheet1, note, HeaderField::LongName),
        "Note est une colonne visible comme les autres, même sans nom long \
         défini : un champ entier couvre toutes les colonnes visibles"
    );
    assert!(
        !state.is_header_field_selected(sheet1, temps, HeaderField::Unit),
        "seul Long Name est sélectionné"
    );
}

/// Maj-clic ou glisser sur les libellés (`extend_header_row_selection`)
/// étend la plage à plusieurs champs — miroir de
/// [`extending_a_row_selection_covers_the_whole_range`].
#[test]
fn extending_a_header_row_selection_covers_the_whole_field_range() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_header_row(sheet1, HeaderField::Unit);
    state.extend_header_row_selection(sheet1, HeaderField::Formula);

    for field in [
        HeaderField::Unit,
        HeaderField::Comment,
        HeaderField::Formula,
    ] {
        assert!(state.is_header_row_selected(sheet1, field));
        assert!(state.is_header_field_selected(sheet1, temps, field));
    }
    assert!(!state.is_header_row_selected(sheet1, HeaderField::LongName));

    // Le sens du glisser ne compte pas.
    state.select_header_row(sheet1, HeaderField::Formula);
    state.extend_header_row_selection(sheet1, HeaderField::Unit);
    for field in [
        HeaderField::Unit,
        HeaderField::Comment,
        HeaderField::Formula,
    ] {
        assert!(state.is_header_row_selected(sheet1, field));
    }
}

/// Retour arrière sur une plage de champ d'en-tête entier vide ce champ dans
/// **toutes** les colonnes — même mécanisme que
/// [`delete_on_a_header_field_selection_clears_it_like_backspace`], mais sur
/// toutes les colonnes d'un même geste plutôt qu'une seule.
#[test]
fn clearing_a_header_row_selection_empties_the_field_in_every_column() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, temps, HeaderField::Unit, "s");
    set_header_field(&mut state, signal, HeaderField::Unit, "V");
    set_header_field(&mut state, temps, HeaderField::LongName, "T");

    state.select_header_row(sheet1, HeaderField::Unit);
    state.dispatch(Action::ClearSelectedCells { sheet: sheet1 });

    assert_eq!(state.column_meta(temps).unit, "");
    assert_eq!(state.column_meta(signal).unit, "");
    assert_eq!(
        state.column_meta(temps).long_name,
        "T",
        "seul le champ Unit était sélectionné"
    );
}

/// Suppr sur une plage de champ d'en-tête entier se contente de vider,
/// exactement comme sur un champ d'une seule colonne : un champ n'est pas
/// répété par ligne, il n'y a rien à faire remonter (voir
/// [`Selection::delete_at_count`]).
#[test]
fn delete_on_a_header_row_selection_only_clears_it() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, temps, HeaderField::Comment, "c1");
    set_header_field(&mut state, signal, HeaderField::Comment, "c2");

    state.select_header_row(sheet1, HeaderField::Comment);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });

    assert_eq!(state.column_meta(temps).comment, "");
    assert_eq!(state.column_meta(signal).comment, "");
    assert_eq!(
        cell(&state, temps, 0),
        Cell::Float(0.0),
        "aucune donnée n'a remonté"
    );
}

/// [`AppState::grid_menu`] sur une plage de champ d'en-tête entier reste
/// dans la zone `Header` (même terminologie qu'un rectangle de champs d'une
/// seule colonne) et n'offre pas « Supprimer et décaler ».
#[test]
fn the_grid_menu_on_a_header_row_selection_stays_in_the_header_zone() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");

    state.select_header_row(sheet1, HeaderField::LongName);
    let menu = state
        .grid_menu(sheet1)
        .expect("un champ entier sélectionné");
    assert_eq!(menu.zone, GridZone::Header);
    assert!(
        menu.plural,
        "un champ entier touche plusieurs colonnes sur Sheet1"
    );
    assert_eq!(menu.target_label(), "les champs");
    assert!(
        !menu.offers_delete_shift,
        "un champ n'est pas répété par ligne, Suppr n'a rien à décaler"
    );
}

/// Copier une plage de champ d'en-tête entier (Ctrl+C sur un libellé) puis
/// la coller ailleurs écrit chaque colonne à sa place, comme un rectangle de
/// champs — c'est la matrice `full_rank_range`/`HeaderFields` de
/// [`AppState::copy_selection`]/[`AppState::paste`] qui s'applique.
#[test]
fn copying_a_header_row_and_pasting_elsewhere_pastes_every_column() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, temps, HeaderField::LongName, "T");
    set_header_field(&mut state, signal, HeaderField::LongName, "S");

    state.select_header_row(sheet1, HeaderField::LongName);
    let text = state.copy_selection(sheet1).expect("champ non vide");
    assert_eq!(
        text, "T\tS\t\n",
        "Sheet1 a une troisième colonne (Note), sans nom long défini"
    );

    set_header_field(&mut state, temps, HeaderField::Unit, "existant");
    state.select_header_row(sheet1, HeaderField::Unit);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    assert_eq!(state.column_meta(temps).unit, "T");
    assert_eq!(state.column_meta(signal).unit, "S");
    assert_eq!(
        state.column_meta(temps).long_name,
        "T",
        "le champ copié lui-même n'a pas bougé"
    );
}

/// Étendre une sélection de cellule de données réinitialise une plage de
/// champ d'en-tête entier en cours plutôt que d'étendre silencieusement sans
/// effet — verrou du garde-fou ajouté à [`AppState::extend_selection`].
#[test]
fn extending_a_data_selection_resets_a_header_rows_selection() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_header_row(sheet1, HeaderField::LongName);
    state.extend_selection(sheet1, temps, 2);

    assert_eq!(
        state.selection(sheet1).and_then(Selection::single_cell),
        Some((temps, 2)),
        "la cellule visée devient la nouvelle sélection"
    );
    assert!(!state.is_header_row_selected(sheet1, HeaderField::LongName));
}

/// Étendre un champ d'une seule colonne réinitialise une plage de champ
/// entier en cours plutôt que de rester bloquée sur l'ancienne plage sans la
/// changer — verrou du garde-fou ajouté à [`AppState::
/// extend_header_field_selection`]. Sans ce garde-fou, l'appel n'aurait rien
/// fait du tout : `extend_row_to` ne mute que `SelectionKind::Rows`, jamais
/// `HeaderRows`, la sélection serait restée figée sur l'ancien champ entier.
#[test]
fn extending_a_single_column_header_field_resets_a_header_rows_selection() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_header_row(sheet1, HeaderField::Unit);
    state.extend_header_field_selection(sheet1, temps, HeaderField::Comment);

    assert_eq!(
        state.selected_header_field(sheet1),
        Some((temps, HeaderField::Comment)),
        "la sélection repart d'un seul champ, celui visé"
    );
    assert!(!state.is_header_row_selected(sheet1, HeaderField::Unit));
}

/// Glisser entre la rangée des lettres et un libellé de champ entier repose
/// une sélection simple dans les deux sens — les deux plages « entières » ne
/// se traversent jamais l'une l'autre, comme la rangée des lettres et la
/// gouttière des numéros de ligne.
#[test]
fn dragging_between_the_letter_row_and_a_header_row_label_never_mixes_the_two_ranges() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_column(sheet1, temps);
    state.extend_header_row_selection(sheet1, HeaderField::Unit);
    assert!(state.is_header_row_selected(sheet1, HeaderField::Unit));
    assert!(!state.is_column_selected(sheet1, temps));

    state.select_header_row(sheet1, HeaderField::Unit);
    state.extend_column_selection(sheet1, temps);
    assert!(state.is_column_selected(sheet1, temps));
    assert!(!state.is_header_row_selected(sheet1, HeaderField::Unit));
}

// -- 19. Sélectionner tout le tableau (« # »), et gouttière « touchée » --

/// Un clic sur le « # » (`select_all`) sélectionne tout le tableau : toutes
/// les lignes, donc toutes les colonnes visibles — `is_all_selected` le
/// confirme, et le menu contextuel de la grille reste bien dans la zone
/// données/lignes (pas d'en-tête) puisque le tableau entier en fait partie.
#[test]
fn clicking_the_hash_gutter_selects_the_whole_table() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_all(sheet1);

    assert!(state.is_all_selected(sheet1));
    for row in 0..state.row_count(sheet1) {
        assert!(state.is_selected(sheet1, temps, row));
        assert!(state.is_selected(sheet1, signal, row));
        assert!(
            state.is_selected(sheet1, note, row),
            "Note est plus courte, mais une sélection de tout le tableau \
             couvre toutes les colonnes visibles, pas seulement celles \
             assez longues"
        );
    }
}

/// Une feuille sans ligne n'a rien à sélectionner : `select_all` n'y pose
/// aucune sélection plutôt que d'en poser une vide et incohérente.
#[test]
fn select_all_on_a_sheet_without_rows_does_nothing() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    state.dispatch(Action::DeleteRows {
        sheet: sheet1,
        at: 0,
        count: state.row_count(sheet1),
    });
    assert_eq!(state.row_count(sheet1), 0);

    state.select_cell(sheet1, temps, 0);
    state.select_all(sheet1);

    assert!(
        !state.is_all_selected(sheet1),
        "rien à sélectionner sur une feuille vide"
    );
}

/// Sélectionner une seule cellule de données « touche » sa colonne et sa
/// ligne — le repère qui teinte la rangée des lettres et la gouttière des
/// numéros de ligne d'une intensité intermédiaire, sans les sélectionner
/// pour autant (`is_column_selected`/`is_row_selected` restent faux : ce
/// n'est pas la rangée des lettres ni la gouttière qui ont été cliquées).
#[test]
fn selecting_a_data_cell_touches_its_column_and_row_but_does_not_select_them() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, temps, 2);

    assert!(state.column_touched_by_selection(sheet1, temps));
    assert!(state.row_touched_by_selection(sheet1, 2));
    assert!(!state.column_touched_by_selection(sheet1, signal));
    assert!(!state.row_touched_by_selection(sheet1, 0));
    assert!(!state.is_column_selected(sheet1, temps));
    assert!(!state.is_row_selected(sheet1, 2));
}

/// Une sélection de champ d'en-tête (colonne seule ou champ entier) ne
/// touche jamais une ligne de données : `row_touched_by_selection` doit
/// rester faux, la gouttière des numéros de ligne n'a pas à réagir à une
/// sélection qui ne descend jamais dans les données.
#[test]
fn a_header_selection_never_touches_a_data_row() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_header_field(sheet1, temps, HeaderField::Unit);
    assert!(!state.row_touched_by_selection(sheet1, 0));

    state.select_header_row(sheet1, HeaderField::Unit);
    assert!(!state.row_touched_by_selection(sheet1, 0));
    assert!(
        state.column_touched_by_selection(sheet1, temps),
        "un champ d'en-tête entier touche bien toutes les colonnes"
    );
}

// -- 20. Tab en dernière colonne d'une ligne de données : ajout de colonne --

/// `Action::AddColumnAndSelect` ajoute une colonne en fin de feuille puis y
/// sélectionne la ligne demandée — le pendant de
/// [`committing_the_last_row_of_the_sheet_adds_a_row`] mais pour l'axe des
/// colonnes (Tab plutôt qu'Entrée).
#[test]
fn add_column_and_select_adds_a_column_and_selects_it_on_the_given_row() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let columns_before = state.visible_columns(sheet1).len();

    state.dispatch(Action::AddColumnAndSelect {
        sheet: sheet1,
        row: 2,
    });

    let columns_after = state.visible_columns(sheet1);
    assert_eq!(columns_after.len(), columns_before + 1);
    let new_column = columns_after.last().expect("colonne ajoutée").0;
    assert!(
        state.is_selected(sheet1, new_column, 2),
        "la ligne demandée est sélectionnée dans la colonne neuve"
    );
}

/// `Action::AddColumnAndSelectHeaderField` — le pendant de
/// [`add_column_and_select_adds_a_column_and_selects_it_on_the_given_row`]
/// côté en-tête : Tab valide sur le champ Formule de la dernière colonne
/// ajoute une colonne plutôt que d'entrer dans les données.
#[test]
fn add_column_and_select_header_field_adds_a_column_and_selects_the_field_in_it() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let columns_before = state.visible_columns(sheet1).len();

    state.dispatch(Action::AddColumnAndSelectHeaderField {
        sheet: sheet1,
        field: HeaderField::Formula,
    });

    let columns_after = state.visible_columns(sheet1);
    assert_eq!(columns_after.len(), columns_before + 1);
    let new_column = columns_after.last().expect("colonne ajoutée").0;
    assert_eq!(
        state.selected_header_field(sheet1),
        Some((new_column, HeaderField::Formula)),
        "le champ demandé est sélectionné dans la colonne neuve"
    );
}

// -- 21. Alignement du corps de la grille : nombres à droite, texte à gauche --

/// Une cellule numérique renseignée est numérique — `cell_is_numeric` pilote
/// `app::cell_widget` pour l'aligner à droite, comme dans tout tableur.
#[test]
fn a_populated_numeric_cell_is_numeric() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    assert!(state.cell_is_numeric(temps, 1));
}

/// Une cellule numérique manquante (`NaN`) n'est pas numérique au sens de
/// l'alignement : un vide n'a pas de raison d'être poussé à droite plutôt
/// qu'à gauche.
#[test]
fn a_missing_numeric_cell_is_not_numeric() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    state.dispatch(Action::CommitCell {
        column: temps,
        row: 2,
        text: "   ".to_string(),
    });

    assert!(!state.cell_is_numeric(temps, 2));
}

/// Une cellule texte n'est jamais numérique.
#[test]
fn a_text_cell_is_not_numeric() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");

    assert!(!state.cell_is_numeric(note, 0), "Note, ligne 0 = \"a\"");
}

/// Sur une colonne `Mixed`, le type se lit par cellule, pas par colonne :
/// deux lignes voisines de la même colonne peuvent s'aligner différemment.
#[test]
fn cell_is_numeric_reads_per_cell_on_a_mixed_column() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");
    assert_eq!(column_type(&state, note), ColumnType::Mixed);

    state.dispatch(Action::CommitCell {
        column: note,
        row: 1,
        text: "3".to_string(),
    });

    assert!(
        state.cell_is_numeric(note, 1),
        "\"3\" saisi dans une colonne Mixed reste une valeur numérique"
    );
    assert!(
        !state.cell_is_numeric(note, 0),
        "la ligne voisine, restée texte (\"a\"), ne l'est pas"
    );
}

// -- 22. Tracer depuis un classeur (menu contextuel de l'en-tête) --

/// `Action::PlotColumns` apparie chaque Y sélectionné à la colonne X la plus
/// proche à sa **gauche** dans la feuille, façon OriginLab — `Temps` est
/// mise en rôle `X`, `Signal` reste `Y` (rôle par défaut), la série créée
/// doit donc tracer `Signal` contre `Temps`.
#[test]
fn plot_columns_pairs_each_y_with_the_nearest_x_to_its_left() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::SetColumnRole {
        column: temps,
        role: ColumnRole::X,
    });

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![temps, signal],
        plot_type: PlotType::Line,
    });

    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("le graphe a été créé dans le dossier du classeur");
    let spec = state.plot_spec(graph).expect("spec du graphe créé");
    assert_eq!(spec.panels.len(), 1);
    let series = &spec.panels[0].series;
    assert_eq!(
        series.len(),
        1,
        "Temps est X, pas une série ; Signal en est une"
    );
    assert_eq!(series[0].y, signal);
    assert_eq!(series[0].x, Some(temps));
    assert_eq!(series[0].plot_type, PlotType::Line);
}

/// Sans colonne X à sa gauche (aucune colonne de rôle `X` dans la feuille),
/// l'abscisse reste implicite — comportement d'Origin sans X désigné.
#[test]
fn plot_columns_without_an_x_to_the_left_uses_an_implicit_abscissa() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Scatter,
    });

    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("le graphe a été créé");
    let spec = state.plot_spec(graph).expect("spec du graphe créé");
    let series = &spec.panels[0].series;
    assert_eq!(series.len(), 1);
    assert_eq!(series[0].y, signal);
    assert_eq!(series[0].x, None, "aucune colonne X dans la feuille");
}

/// Une sélection sans aucune colonne de rôle Y ne crée aucun graphe — pas de
/// commande envoyée, donc rien à annuler.
#[test]
fn plot_columns_without_any_y_column_is_a_no_op() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let root = state.tree().root();
    state.dispatch(Action::SetColumnRole {
        column: temps,
        role: ColumnRole::X,
    });
    let children_before = state.tree().node(root).expect("racine").children().len();
    let undo_label_before = state.undo_label();

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![temps],
        plot_type: PlotType::Line,
    });

    let children_after = state.tree().node(root).expect("racine").children().len();
    assert_eq!(
        children_before, children_after,
        "aucune colonne Y sélectionnée : pas de graphe créé"
    );
    assert_eq!(
        state.undo_label(),
        undo_label_before,
        "aucune commande n'a été envoyée : l'entrée d'historique à annuler ne change pas"
    );
    assert!(state.status().is_some_and(|status| status.is_error));
}

/// Annuler après `Action::PlotColumns` restaure l'arbre bit pour bit — le
/// graphe créé disparaît, comme toute autre création.
#[test]
fn undo_after_plot_columns_restores_the_tree_bit_for_bit() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let before = std::sync::Arc::clone(state.tree());

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    assert!(state.can_undo());

    state.dispatch(Action::Undo);

    assert_eq!(**state.tree(), *before);
}

/// `Action::AddSeriesToGraph` ajoute des séries à un `PlotSpec` existant sans
/// écraser celles déjà présentes — c'est ce que le glisser-déposer du
/// chantier 2 exigera, testé ici en pilotant `AppState` directement.
#[test]
fn add_series_to_graph_appends_without_overwriting_existing_series() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");
    let root = state.tree().root();

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("le graphe a été créé");
    let before = state.plot_spec(graph).expect("spec initiale");
    assert_eq!(before.panels[0].series.len(), 1);

    state.dispatch(Action::AddSeriesToGraph {
        graph,
        columns: vec![note],
        plot_type: PlotType::Scatter,
    });

    let after = state.plot_spec(graph).expect("spec après ajout");
    assert_eq!(after.panels[0].series.len(), 2);
    assert_eq!(
        after.panels[0].series[0].y, signal,
        "la série déjà présente n'est pas écrasée"
    );
    assert_eq!(after.panels[0].series[1].y, note);
    assert_eq!(after.panels[0].series[1].plot_type, PlotType::Scatter);
}

// -- 23. Glisser une sélection de colonnes vers un graphe --

/// La zone de prise du glisser-déposer de colonnes est une mince bande sur
/// le bord *extérieur* gauche ou droit de la grille — [`grid_grip_side`] est
/// la fonction pure qui en décide, hors de tout code de peinture. À
/// l'intérieur des bornes verticales de la grille, un pointeur à `margin`
/// points ou moins du bord gauche rend `Left`, du bord droit `Right`.
#[test]
fn grid_grip_side_detects_the_outer_edges_of_the_grid() {
    let grid = (100.0, 50.0, 300.0, 250.0); // (gauche, haut, droite, bas)
    let margin = 10.0;

    assert_eq!(
        grid_grip_side(grid, (100.0, 150.0), margin),
        Some(GripSide::Left),
        "tout contre le bord gauche"
    );
    assert_eq!(
        grid_grip_side(grid, (108.0, 150.0), margin),
        Some(GripSide::Left),
        "à l'intérieur de la marge, encore le bord gauche"
    );
    assert_eq!(
        grid_grip_side(grid, (300.0, 150.0), margin),
        Some(GripSide::Right),
        "tout contre le bord droit"
    );
    assert_eq!(
        grid_grip_side(grid, (293.0, 150.0), margin),
        Some(GripSide::Right),
        "à l'intérieur de la marge, encore le bord droit"
    );
}

/// Hors de la marge (au milieu de la grille) ou hors de ses bornes
/// verticales, la zone de prise ne se déclenche pas — c'est ce qui garde le
/// glisser d'extension de sélection de `cell_widget` intact ailleurs dans la
/// grille.
#[test]
fn grid_grip_side_ignores_the_interior_and_outside_the_vertical_bounds() {
    let grid = (100.0, 50.0, 300.0, 250.0);
    let margin = 10.0;

    assert_eq!(
        grid_grip_side(grid, (200.0, 150.0), margin),
        None,
        "milieu de la grille : pas la zone de prise"
    );
    assert_eq!(
        grid_grip_side(grid, (105.0, 30.0), margin),
        None,
        "au-dessus de la grille, même proche du bord gauche en x"
    );
    assert_eq!(
        grid_grip_side(grid, (105.0, 260.0), margin),
        None,
        "en dessous de la grille"
    );
}

/// [`AppState::column_drag_payload`] rend `None` tant que `sheet` n'a pas de
/// sélection couvrant une colonne — pas de glisser possible sans colonne,
/// même depuis la zone de prise.
#[test]
fn column_drag_payload_is_none_without_a_column_selection() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");

    assert_eq!(state.column_drag_payload(sheet1), None);
}

/// Avec une sélection de cellules couvrant une colonne, la charge utile
/// porte la feuille et exactement les colonnes couvertes par la sélection —
/// c'est elle que `crate::app::grid_contents` glisse depuis la zone de
/// prise, et que [`drop_payload_action`] relit au dépôt.
#[test]
fn column_drag_payload_carries_the_sheet_and_its_selected_columns() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    state.select_cell(sheet1, temps, 0);
    state.extend_selection(sheet1, signal, 1);

    let payload = state
        .column_drag_payload(sheet1)
        .expect("sélection couvrant Temps et Signal");

    assert_eq!(payload.sheet, sheet1);
    assert_eq!(payload.columns, vec![temps, signal]);
}

/// [`drop_payload_action`] convertit toujours la charge utile en
/// `Action::AddSeriesToGraph` avec `PlotType::Line` par défaut — le seul
/// type de tracé demandé par ce chantier pour le glisser-déposer, quel que
/// soit l'outil qui avait servi à construire la sélection au départ.
#[test]
fn drop_payload_action_dispatches_add_series_to_graph_with_line_by_default() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let graph = NodeId::from_raw(999);
    let payload = ColumnDragPayload {
        sheet: sheet1,
        columns: vec![signal],
    };

    let action = drop_payload_action(&payload, graph);

    match action {
        Action::AddSeriesToGraph {
            graph: dropped_graph,
            columns,
            plot_type,
        } => {
            assert_eq!(dropped_graph, graph);
            assert_eq!(columns, vec![signal]);
            assert_eq!(plot_type, PlotType::Line);
        }
        other => panic!("attendu Action::AddSeriesToGraph, obtenu {other:?}"),
    }
}

/// Bout en bout : dispatcher l'action produite par [`drop_payload_action`]
/// ajoute effectivement la série au graphe visé — le même chemin que
/// `crate::app::graph_drop_target` emprunte au relâchement du glisser.
#[test]
fn dispatching_drop_payload_action_adds_the_series_to_the_graph() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");
    let root = state.tree().root();

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("le graphe a été créé");

    state.select_cell(sheet1, note, 0);
    let payload = state
        .column_drag_payload(sheet1)
        .expect("Note sélectionnée");
    state.dispatch(drop_payload_action(&payload, graph));

    let spec = state.plot_spec(graph).expect("spec après dépôt");
    assert_eq!(spec.panels[0].series.len(), 2);
    assert_eq!(spec.panels[0].series[1].y, note);
    assert_eq!(spec.panels[0].series[1].plot_type, PlotType::Line);
}

// -- 24. Propriétés des graphiques --

/// [`active_graph`] décide la présence du menu « Graphique » : seul
/// `ActiveTabKind::Graph` rend `Some`, toutes les autres natures d'onglet
/// (ou aucun onglet actif) rendent `None`.
#[test]
fn active_graph_is_some_only_for_a_graph_tab() {
    let graph = NodeId::from_raw(7);
    assert_eq!(active_graph(ActiveTabKind::Graph(graph)), Some(graph));
    assert_eq!(active_graph(ActiveTabKind::Book(graph)), None);
    assert_eq!(active_graph(ActiveTabKind::Matrix(graph)), None);
    assert_eq!(active_graph(ActiveTabKind::Note(graph)), None);
    assert_eq!(active_graph(ActiveTabKind::None), None);
}

/// Trace `Signal` de `Sheet1` dans un nouveau graphe et le rend.
fn plotted_graph(state: &mut AppState) -> NodeId {
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("le graphe a été créé")
}

/// Le clic droit sur un composant ouvre l'onglet qui le règle : titre →
/// « Graphe », zone de tracé → « Zone de tracé », axe ou titre d'axe →
/// « Axes » sur cet axe.
#[test]
fn a_right_clicked_component_opens_the_settings_page_that_sets_it() {
    assert_eq!(
        PlotTarget::Title.settings_page(),
        (PlotSettingsTab::Graph, None)
    );
    assert_eq!(
        PlotTarget::Background.settings_page(),
        (PlotSettingsTab::Graph, None)
    );
    assert_eq!(
        PlotTarget::PlotArea.settings_page(),
        (PlotSettingsTab::PlotArea, None)
    );
    assert_eq!(
        PlotTarget::Axis(PlotAxis::Left).settings_page(),
        (PlotSettingsTab::Axes, Some(PlotAxis::Left))
    );
    assert_eq!(
        PlotTarget::AxisTitle(PlotAxis::Bottom).settings_page(),
        (PlotSettingsTab::Axes, Some(PlotAxis::Bottom))
    );
    assert_eq!(
        PlotTarget::AxisTitle(PlotAxis::Bottom).label(),
        "Titre de l'axe X (bas)"
    );
}

/// `Action::OpenPlotSettings` ouvre la fenêtre sur l'onglet et l'axe
/// demandés et un brouillon du graphe ; sans axe précisé (menu
/// « Graphique »), l'axe déjà choisi est gardé.
#[test]
fn open_plot_settings_opens_the_requested_tab_and_keeps_the_chosen_axis() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let graph = plotted_graph(&mut state);
    assert!(state.plot_settings().is_none());

    state.dispatch(PlotTarget::Axis(PlotAxis::Left).open_settings(graph));
    let settings = state.plot_settings().expect("fenêtre ouverte");
    assert_eq!(settings.graph, graph);
    assert_eq!(settings.tab, PlotSettingsTab::Axes);
    assert_eq!(settings.axis, PlotAxis::Left);
    assert_eq!(state.plot_edit().expect("brouillon").graph, graph);

    state.dispatch(Action::OpenPlotSettings {
        graph,
        tab: PlotSettingsTab::Legend,
        axis: None,
    });
    let settings = state.plot_settings().expect("fenêtre ouverte");
    assert_eq!(settings.tab, PlotSettingsTab::Legend);
    assert_eq!(settings.axis, PlotAxis::Left, "l'axe choisi est gardé");
}

/// Un nœud qui n'est pas un graphe n'ouvre rien.
#[test]
fn open_plot_settings_ignores_a_node_that_is_not_a_graph() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    state.dispatch(Action::OpenPlotSettings {
        graph: sheet1,
        tab: PlotSettingsTab::Graph,
        axis: None,
    });
    assert!(state.plot_settings().is_none());
    assert!(state.plot_edit().is_none());
}

/// Fermer la fenêtre valide une saisie encore en cours (champ qui n'a pas
/// perdu le focus), puis abandonne le brouillon.
#[test]
fn closing_plot_settings_commits_a_pending_edit() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let graph = plotted_graph(&mut state);
    state.dispatch(PlotTarget::Title.open_settings(graph));
    state.plot_edit_mut().expect("brouillon").spec.title = "Spectre".to_string();

    state.close_plot_settings();

    assert!(state.plot_settings().is_none());
    assert!(state.plot_edit().is_none());
    assert_eq!(state.plot_spec(graph).expect("spec").title, "Spectre");
}

/// #15 : « Annuler » (aussi la croix et Échap) remet le graphe dans l'état où
/// il était à l'ouverture de la fenêtre, après un aperçu en direct, et ne
/// laisse aucune entrée d'historique.
#[test]
fn cancelling_plot_settings_restores_the_graph_and_leaves_no_history() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let graph = plotted_graph(&mut state);
    let original = state.plot_spec(graph).expect("spec");
    let label_before = state.undo_label();
    state.dispatch(PlotTarget::Title.open_settings(graph));

    state.plot_edit_mut().expect("brouillon").spec.title = "Aperçu".to_string();
    state.commit_plot_edit();
    assert_eq!(
        state.plot_spec(graph).expect("spec").title,
        "Aperçu",
        "aperçu en direct dans le modèle"
    );

    state.cancel_plot_settings();

    assert!(state.plot_settings().is_none());
    assert!(state.plot_edit().is_none());
    assert_eq!(state.plot_spec(graph).expect("spec"), original);
    assert_eq!(state.undo_label(), label_before, "aucune entrée ajoutée");
}

/// #15 : « Valider » garde tout, en **une seule** entrée d'historique — un
/// Ctrl+Z défait tous les gestes de la fenêtre d'un coup.
#[test]
fn accepting_plot_settings_is_a_single_history_entry() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let graph = plotted_graph(&mut state);
    let original = state.plot_spec(graph).expect("spec");
    state.dispatch(PlotTarget::Title.open_settings(graph));

    state.plot_edit_mut().expect("brouillon").spec.title = "Premier".to_string();
    state.commit_plot_edit();
    state.plot_edit_mut().expect("brouillon").spec.background = "#102030".to_string();
    state.commit_plot_edit();
    state.close_plot_settings();

    let kept = state.plot_spec(graph).expect("spec");
    assert_eq!(kept.title, "Premier");
    assert_eq!(kept.background, "#102030");

    state.dispatch(Action::Undo);

    assert_eq!(
        state.plot_spec(graph).expect("spec"),
        original,
        "un seul Ctrl+Z défait les deux gestes"
    );
}

/// Ouvrir les propriétés d'un autre graphe valide d'abord le brouillon du
/// premier : rien n'est perdu en passant de l'un à l'autre.
#[test]
fn switching_plot_settings_to_another_graph_commits_the_first_draft() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let first = plotted_graph(&mut state);
    let second = plotted_graph(&mut state);
    assert_ne!(first, second);
    state.dispatch(PlotTarget::Title.open_settings(first));
    state.plot_edit_mut().expect("brouillon").spec.title = "Premier".to_string();

    state.dispatch(PlotTarget::Title.open_settings(second));

    assert_eq!(state.plot_spec(first).expect("spec").title, "Premier");
    assert_eq!(state.plot_edit().expect("brouillon").graph, second);
}

/// Un brouillon sans saisie en cours suit le modèle : après Annuler, la
/// fenêtre montre le spec rétabli, pas l'ancien — sinon la validation
/// suivante réécrirait par-dessus l'annulation.
#[test]
fn an_idle_plot_draft_follows_undo() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let graph = plotted_graph(&mut state);
    let original = state.plot_spec(graph).expect("spec").title;
    // Brouillon ouvert sans la fenêtre (pas de macro d'historique : Annuler
    // reste possible, contrairement à la fenêtre modale qui le refuse).
    state.begin_plot_edit(graph);
    state.plot_edit_mut().expect("brouillon").spec.title = "Modifié".to_string();
    state.commit_plot_edit();

    state.dispatch(Action::Undo);
    state.begin_plot_edit(graph);

    assert_eq!(state.plot_spec(graph).expect("spec").title, original);
    assert_eq!(
        state.plot_edit().expect("brouillon").spec.title,
        original,
        "le brouillon a suivi l'annulation"
    );
}

/// `AppState::begin_plot_edit` ouvre un brouillon initialisé sur le spec
/// courant du graphe, et `AppState::commit_plot_edit` l'envoie au modèle
/// (`Action::SetPlotSpec`) — un aller-retour complet, comme le ferait le
/// panneau en éditant le titre du graphe.
#[test]
fn begin_and_commit_plot_edit_round_trip_through_set_plot_spec() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("le graphe a été créé");

    state.begin_plot_edit(graph);
    let edit = state.plot_edit().expect("brouillon ouvert").clone();
    assert_eq!(edit.graph, graph);
    assert_eq!(edit.spec, state.plot_spec(graph).expect("spec initiale"));

    state.plot_edit_mut().expect("brouillon").spec.title = "Ma figure".to_string();
    state.commit_plot_edit();

    let after = state.plot_spec(graph).expect("spec après commit");
    assert_eq!(after.title, "Ma figure");
}

/// Valider un brouillon qui n'a **pas** changé n'envoie aucune commande —
/// sans quoi perdre le focus d'un champ texte non modifié, ou relâcher un
/// curseur qui n'a pas bougé, empilerait une entrée d'historique inutile.
#[test]
fn committing_an_unchanged_plot_edit_sends_no_command() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("le graphe a été créé");

    state.begin_plot_edit(graph);
    let undo_label_before = state.undo_label();

    state.commit_plot_edit();

    assert_eq!(
        state.undo_label(),
        undo_label_before,
        "brouillon identique au spec courant : rien à valider"
    );
}

/// Ctrl+Z après une validation du panneau restaure le spec d'avant, comme
/// n'importe quelle autre commande annulable — le panneau n'a pas de chemin
/// de mutation à lui, il passe par `Command::SetPlotSpec` comme les autres.
#[test]
fn undo_after_committing_a_plot_edit_restores_the_previous_spec() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("le graphe a été créé");
    let before = state.plot_spec(graph).expect("spec initial");

    state.begin_plot_edit(graph);
    state.plot_edit_mut().expect("brouillon").spec.title = "Titre modifié".to_string();
    state.commit_plot_edit();
    assert_ne!(state.plot_spec(graph).expect("spec après commit"), before);

    state.dispatch(Action::Undo);

    assert_eq!(
        state.plot_spec(graph).expect("spec après annulation"),
        before
    );
}

/// Repeindre la même image sur le même graphe ne doit jamais effacer une
/// saisie en cours : `begin_plot_edit` est un no-op tant que le graphe visé
/// n'a pas changé, même appelé plusieurs fois de suite.
#[test]
fn begin_plot_edit_does_not_clobber_an_in_progress_draft() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("le graphe a été créé");

    state.begin_plot_edit(graph);
    state.plot_edit_mut().expect("brouillon").spec.title = "En cours de saisie".to_string();

    // Répète l'appel, comme le ferait `plot_settings_window` à chaque image.
    state.begin_plot_edit(graph);
    state.begin_plot_edit(graph);

    assert_eq!(
        state
            .plot_edit()
            .expect("brouillon toujours ouvert")
            .spec
            .title,
        "En cours de saisie",
        "le brouillon en cours de saisie n'a pas été effacé"
    );
}

/// Changer de graphe réinitialise le brouillon sur le spec de l'autre
/// graphe — pas de résidu de l'édition précédente affiché sur le mauvais
/// graphe.
#[test]
fn begin_plot_edit_resets_the_draft_when_switching_graphs() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");
    let root = state.tree().root();

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let first_graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("premier graphe créé");
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![note],
        plot_type: PlotType::Scatter,
    });
    let second_graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("second graphe créé");
    assert_ne!(first_graph, second_graph);

    state.begin_plot_edit(first_graph);
    state.plot_edit_mut().expect("brouillon").spec.title = "Résidu".to_string();

    state.begin_plot_edit(second_graph);

    let edit = state.plot_edit().expect("brouillon du second graphe");
    assert_eq!(edit.graph, second_graph);
    assert_ne!(
        edit.spec.title, "Résidu",
        "le brouillon du premier graphe ne doit pas fuiter sur le second"
    );
}

/// Une série dont le type de tracé est [`PlotType::Unknown`] (C5, variante
/// d'une version plus récente de PlotYx que ce lecteur ne connaît pas) doit
/// pouvoir être ouverte dans le brouillon d'édition et affichée dans la
/// liste des séries sans jamais paniquer — invariant 10 (« un défaut de
/// donnée dégrade, il ne plante pas »), qui vaut aussi pour ce panneau.
#[test]
fn a_series_with_an_unknown_plot_type_can_be_opened_without_panicking() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("le graphe a été créé");

    state.begin_plot_edit(graph);
    {
        let edit = state.plot_edit_mut().expect("brouillon");
        edit.spec.panels.get_mut(0).expect("panneau").series[0].plot_type =
            PlotType::Unknown(RawVariant {
                name: "FutureType".to_string(),
                payload: vec![1, 2, 3],
            });
    }
    state.commit_plot_edit();

    let spec = state.plot_spec(graph).expect("spec après commit");
    assert!(matches!(
        spec.panels[0].series[0].plot_type,
        PlotType::Unknown(_)
    ));
}

/// L'échelle d'un axe se modifie bien à travers le brouillon : ce test tient
/// lieu de garde-fou pour tout champ d'`AxisSpec` édité par le panneau — si
/// un futur renommage de champ casse le binding direct des widgets, un test
/// qui ne passe plus par `PlotEdit` du tout ne le verrait pas.
#[test]
fn editing_an_axis_scale_through_the_draft_is_visible_after_commit() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("le graphe a été créé");

    state.begin_plot_edit(graph);
    {
        let edit: &mut PlotEdit = state.plot_edit_mut().expect("brouillon");
        let panel = edit.spec.panels.get_mut(0).expect("panneau");
        panel.left.scale = AxisScale::Log;
        panel.left.auto_range = false;
        panel.left.range_min = Some(0.1);
        panel.left.range_max = Some(100.0);
    }
    state.commit_plot_edit();

    let spec = state.plot_spec(graph).expect("spec après commit");
    assert_eq!(spec.panels[0].left.scale, AxisScale::Log);
    assert!(spec.panels[0].left.has_explicit_range());
    assert_eq!(spec.panels[0].left.range_min, Some(0.1));
    assert_eq!(spec.panels[0].left.range_max, Some(100.0));
}

// -- 25. Couleurs enregistrées --

/// Enregistrer une couleur « dans le projet » passe par une commande
/// annulable : elle apparaît dans l'arbre, Annuler la retire.
#[test]
fn saving_a_color_in_the_project_is_undoable() {
    use plotyx_app::state::SaveTarget;
    use plotyx_plot::ColorUsage;
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    state.use_user_colors_file(dir.path().join("couleurs.json"));

    state.dispatch(Action::SaveColor {
        usage: ColorUsage::Curves,
        color: "#FF0000".to_string(),
        target: SaveTarget::Project,
    });
    assert_eq!(
        state
            .saved_colors(SaveTarget::Project)
            .get(ColorUsage::Curves),
        ["#ff0000"]
    );
    assert!(
        state
            .saved_colors(SaveTarget::Project)
            .get(ColorUsage::Text)
            .is_empty(),
        "rangée par usage"
    );
    assert!(
        state.saved_colors(SaveTarget::User).is_empty(),
        "pas chez l'utilisateur"
    );

    state.dispatch(Action::Undo);
    assert!(state.saved_colors(SaveTarget::Project).is_empty());
}

/// Enregistrer une couleur « utilisateur » l'écrit dans le fichier de
/// l'utilisateur, hors projet et hors historique ; la retirer aussi.
#[test]
fn saving_a_color_for_the_user_writes_the_user_file() {
    use plotyx_app::state::SaveTarget;
    use plotyx_plot::ColorUsage;
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let path = dir.path().join("config").join("couleurs.json");
    state.use_user_colors_file(path.clone());

    state.dispatch(Action::SaveColor {
        usage: ColorUsage::Background,
        color: "#ffffff00".to_string(),
        target: SaveTarget::User,
    });
    assert_eq!(
        state
            .saved_colors(SaveTarget::User)
            .get(ColorUsage::Background),
        ["#ffffff00"]
    );
    assert!(state.saved_colors(SaveTarget::Project).is_empty());
    assert!(!state.can_undo(), "hors historique");
    let written = std::fs::read_to_string(&path).expect("fichier écrit");
    assert!(written.contains("#ffffff00"));

    state.dispatch(Action::RemoveSavedColor {
        usage: ColorUsage::Background,
        color: "#FFFFFF00".to_string(),
        target: SaveTarget::User,
    });
    assert!(state.saved_colors(SaveTarget::User).is_empty());
}

/// Un graphe créé depuis le classeur n'a pas de fond : « aucune couleur »
/// (transparent) est le fond par défaut.
#[test]
fn a_new_graph_has_no_background_color() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let graph = plotted_graph(&mut state);
    assert_eq!(
        state.plot_spec(graph).expect("spec").background,
        plotyx_plot::TRANSPARENT
    );
}

// -- Glisser-déposer de fichiers (issue #8) --

/// Le lieu du dépôt fixe la destination ; l'extension `.plx` fait d'un fichier
/// un projet à ouvrir, pas une donnée à importer.
#[test]
fn file_drop_plan_picks_destination_from_the_drop_target_and_splits_projects() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let paths = vec![
        std::path::PathBuf::from("a.csv"),
        std::path::PathBuf::from("projet.PLX"),
        std::path::PathBuf::from("b.txt"),
    ];

    let on_book = file_drop_plan(&paths, Some(book));
    assert_eq!(on_book.destination, ImportDestination::ActiveBook(book));
    assert_eq!(
        on_book.projects,
        vec![std::path::PathBuf::from("projet.PLX")]
    );
    assert_eq!(on_book.data.len(), 2);

    let elsewhere = file_drop_plan(&paths, None);
    assert_eq!(elsewhere.destination, ImportDestination::NewBookForAll);
}

/// Dépôt sur un classeur : une feuille par fichier, une seule entrée
/// d'historique, et le message nomme le modèle utilisé.
#[test]
fn dropping_files_on_a_book_imports_them_as_one_undo_entry() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();
    let file_a = dir.path().join("depot_a.csv");
    let file_b = dir.path().join("depot_b.csv");
    std::fs::write(&file_a, "1,10\n2,20\n").unwrap();
    std::fs::write(&file_b, "3,30\n4,40\n").unwrap();

    state.dispatch(Action::DropFiles {
        paths: vec![file_a, file_b],
        target_book: Some(book),
    });

    assert_eq!(state.worksheets_of(book).len(), sheets_before + 2);
    assert!(state.import_state().is_none(), "aucune fenêtre d'import");
    let status = state.status().expect("message de statut");
    assert!(!status.is_error);
    assert!(status.text.contains("modèle"), "{}", status.text);
    state.dispatch(Action::Undo);
    assert_eq!(state.worksheets_of(book).len(), sheets_before);
    assert!(!state.can_undo(), "un seul Ctrl+Z pour tout le dépôt");
}

/// Dépôt hors classeur : un nouveau classeur partagé par le lot.
#[test]
fn dropping_files_elsewhere_creates_a_new_book() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let root = state.tree().root();
    let books_before = state.tree().node(root).unwrap().children().len();
    let file = dir.path().join("depot.csv");
    std::fs::write(&file, "1,10\n2,20\n").unwrap();

    state.dispatch(Action::DropFiles {
        paths: vec![file],
        target_book: None,
    });

    assert_eq!(
        state.tree().node(root).unwrap().children().len(),
        books_before + 1
    );
}

/// Un `.plx` n'est jamais importé : il est mis en file pour ouverture dans
/// une nouvelle fenêtre, sans toucher au projet courant.
#[test]
fn dropping_a_project_file_queues_it_instead_of_importing() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let revision = state.revision();
    let project = dir.path().join("autre.plx");
    std::fs::write(&project, b"pas lu ici").unwrap();

    state.dispatch(Action::DropFiles {
        paths: vec![project.clone()],
        target_book: None,
    });

    assert_eq!(state.revision(), revision, "le projet courant est intact");
    assert_eq!(state.take_projects_to_open(), vec![project]);
    assert!(state.take_projects_to_open().is_empty(), "file vidée");
}

/// Un fichier que la recette lit mal n'est pas importé : la fenêtre d'import
/// s'ouvre, pré-remplie ; un fichier correct du même lot est importé.
#[test]
fn dropping_an_unreadable_file_opens_the_import_window_prefilled() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();
    let bad = dir.path().join("binaire.dat");
    std::fs::write(&bad, [0u8, 159, 146, 150, 0, 0]).unwrap();
    let good = dir.path().join("bon.csv");
    std::fs::write(&good, "1,10\n2,20\n").unwrap();

    state.dispatch(Action::DropFiles {
        paths: vec![bad.clone(), good],
        target_book: Some(book),
    });

    assert_eq!(state.worksheets_of(book).len(), sheets_before + 1);
    let import = state.import_state().expect("fenêtre d'import ouverte");
    assert_eq!(import.paths, vec![bad]);
    assert_eq!(import.destination, ImportDestination::ActiveBook(book));
}

// -- Création d'un classeur ou d'une feuille : colonnes X et Y d'office --

/// Un nouveau classeur naît avec une colonne X et une colonne Y, et un seul
/// Ctrl+Z retire le tout.
#[test]
fn a_new_book_starts_with_an_x_and_a_y_column() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let root = state.tree().root();
    let before = state.tree().node(root).unwrap().children().len();

    state.dispatch(Action::AddWorkbook);

    let book = *state.tree().node(root).unwrap().children().last().unwrap();
    assert_eq!(
        state.tree().node(root).unwrap().children().len(),
        before + 1
    );
    let sheet = state.worksheets_of(book)[0].0;
    let columns = state.visible_columns(sheet);
    assert_eq!(columns.len(), 2);
    assert_eq!(state.column_meta(columns[0].0).role, ColumnRole::X);
    assert_eq!(state.column_meta(columns[1].0).role, ColumnRole::Y);

    state.dispatch(Action::Undo);
    assert_eq!(state.tree().node(root).unwrap().children().len(), before);
}

/// Une feuille ajoutée à un classeur reçoit elle aussi X et Y.
#[test]
fn a_new_sheet_starts_with_an_x_and_a_y_column() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let before = state.worksheets_of(book).len();

    state.dispatch(Action::AddWorksheet { book });

    let sheets = state.worksheets_of(book);
    assert_eq!(sheets.len(), before + 1);
    let columns = state.visible_columns(sheets.last().unwrap().0);
    assert_eq!(columns.len(), 2);
    assert_eq!(state.column_meta(columns[0].0).role, ColumnRole::X);
    assert_eq!(state.column_meta(columns[1].0).role, ColumnRole::Y);

    state.dispatch(Action::Undo);
    assert_eq!(state.worksheets_of(book).len(), before);
}

// -- Métadonnées de fichier par colonne (issue #5) --

/// L'import garde l'en-tête du fichier, clé / valeur, sur chaque colonne ; la
/// fenêtre de consultation s'ouvre et se ferme sans toucher à l'historique ;
/// un seul Ctrl+Z retire l'import et ses métadonnées.
#[test]
fn import_keeps_file_metadata_on_every_column() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("jasco.csv");
    std::fs::write(
        &file,
        "TITLE;S13\n[Measurement Information]\nXUNITS;NANOMETERS\nXYDATA\n800;1\n799;2\n",
    )
    .unwrap();
    let mut recipe = ImportRecipe::new("Jasco");
    recipe.separator = Separator::Semicolon;
    recipe.header = HeaderSkip::UntilMarker("XYDATA".into());

    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![file],
    });

    let sheet = sheet_named(state.tree(), "jasco");
    let columns = state.visible_columns(sheet);
    assert_eq!(columns.len(), 2);
    for (column, ..) in &columns {
        let lines = state.column_meta(*column).file_metadata;
        let keys: Vec<&str> = lines.iter().map(|l| l.key.as_str()).collect();
        assert_eq!(
            keys,
            vec!["TITLE", "[Measurement Information]", "XUNITS", ""]
        );
        assert_eq!(
            lines[3].value, "XYDATA",
            "ligne sans clé gardée telle quelle"
        );
        assert_eq!(lines[2].value, "NANOMETERS");
        assert!(lines[1].is_section());
    }
    assert_eq!(state.file_metadata_summary(columns[0].0), "3 clés");

    let revision = state.revision();
    state.dispatch(Action::ViewColumnMetadata {
        column: columns[0].0,
    });
    assert_eq!(state.column_metadata_view(), Some(columns[0].0));
    state.dispatch(Action::CloseColumnMetadata);
    assert_eq!(state.column_metadata_view(), None);
    assert_eq!(
        state.revision(),
        revision,
        "consulter ne change pas le modèle"
    );

    state.dispatch(Action::Undo);
    assert!(state
        .worksheets_of(book)
        .iter()
        .all(|(id, _)| state.tree().node(*id).unwrap().name() != "jasco"));
}

/// Une colonne saisie à la main n'a pas de métadonnées : case vide.
#[test]
fn a_hand_made_column_has_no_file_metadata() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    state.dispatch(Action::AddWorkbook);
    let root = state.tree().root();
    let book = *state.tree().node(root).unwrap().children().last().unwrap();
    let sheet = state.worksheets_of(book)[0].0;
    let column = state.visible_columns(sheet)[0].0;
    assert!(state.column_meta(column).file_metadata.is_empty());
    assert_eq!(state.file_metadata_summary(column), "");
}

// -- Assistant d'import en étapes (issue #6) --

fn jasco_like(dir: &std::path::Path) -> std::path::PathBuf {
    let file = dir.join("jasco.csv");
    std::fs::write(
        &file,
        "TITLE;S13\nXUNITS;NANOMETERS\nXYDATA\n800;1\n799;2\n798;3\n\nData points;3\n",
    )
    .unwrap();
    file
}

/// Les étapes s'enchaînent dans l'ordre, bornées aux deux bouts.
#[test]
fn import_wizard_steps_go_forward_and_back_within_bounds() {
    use plotyx_app::state::ImportStep;
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    assert_eq!(state.import_state().unwrap().step, ImportStep::Files);
    state.import_step_back();
    assert_eq!(state.import_state().unwrap().step, ImportStep::Files);
    for _ in 0..10 {
        state.import_step_forward();
    }
    assert_eq!(state.import_state().unwrap().step, ImportStep::Destination);
    state.import_step_back();
    assert_eq!(state.import_state().unwrap().step, ImportStep::Metadata);
}

/// Le fichier n'est lu qu'une fois : après sa suppression du disque, l'aperçu
/// reste servi par le cache (plus de relecture à chaque image).
#[test]
fn import_preview_reads_the_file_only_once() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let file = jasco_like(dir.path());
    state.open_import(None::<NodeId>);
    state.set_import_paths(vec![file.clone()]);
    assert!(matches!(state.import_line_preview(), Some(Ok(_))));
    std::fs::remove_file(&file).unwrap();
    assert!(
        matches!(state.import_line_preview(), Some(Ok(_))),
        "l'aperçu vient du cache, pas du disque"
    );
    assert_eq!(state.import_encoding(), Some("UTF-8"));
}

/// Plusieurs fichiers : on en retire un, on choisit celui qu'on prévisualise,
/// et chacun porte son propre état (lignes lues ou erreur) avant l'import.
#[test]
fn import_wizard_lists_files_with_their_status_and_previews_the_chosen_one() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let good = jasco_like(dir.path());
    let missing = dir.path().join("absent.csv");
    state.open_import(None::<NodeId>);
    state.set_import_paths(vec![good.clone(), missing.clone()]);

    let statuses = state.import_file_statuses();
    assert!(statuses[0].1.is_ok());
    assert!(
        statuses[1].1.is_err(),
        "l'erreur est connue avant d'importer"
    );

    state.set_import_preview_index(1);
    assert!(matches!(state.import_line_preview(), Some(Err(_))));
    state.remove_import_path(1);
    assert_eq!(state.import_state().unwrap().paths, vec![good]);
    assert!(matches!(state.import_line_preview(), Some(Ok(_))));
}

/// Les deux réglages d'en-tête sont ramenés à un seul à l'ouverture d'un
/// modèle ; la suppression d'un modèle demande une confirmation.
#[test]
fn import_wizard_unifies_header_and_confirms_recipe_deletion() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    {
        let import = state.import_state_mut().unwrap();
        import.recipe.header = HeaderSkip::Lines(3);
        import.save_as_name = "Trois lignes".into();
    }
    state.save_import_recipe();
    state.open_import(None::<NodeId>);
    state.select_import_recipe("Trois lignes");
    let recipe = &state.import_state().unwrap().recipe;
    assert_eq!(recipe.header, HeaderSkip::None);
    assert_eq!(recipe.header_layout.main_header_lines, 3);
    assert!(!state.import_recipe_modified());

    state.import_state_mut().unwrap().recipe.skip_chars = 2;
    assert!(
        state.import_recipe_modified(),
        "modifié sans être enregistré"
    );

    state.request_delete_import_recipe("Trois lignes");
    state.cancel_delete_import_recipe();
    assert!(state
        .import_state()
        .unwrap()
        .store
        .recipes()
        .iter()
        .any(|r| r.name == "Trois lignes"));
    state.request_delete_import_recipe("Trois lignes");
    state.confirm_delete_import_recipe();
    assert!(state
        .import_state()
        .unwrap()
        .store
        .recipes()
        .iter()
        .all(|r| r.name != "Trois lignes"));
}

/// Le clic droit sur l'aperçu règle l'en-tête, la ligne des unités et la fin
/// des données.
#[test]
fn import_preview_line_actions_adjust_the_recipe() {
    use plotyx_app::state::ImportLineAction;
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let file = jasco_like(dir.path());
    state.open_import(None::<NodeId>);
    state.set_import_paths(vec![file]);
    {
        let recipe = &mut state.import_state_mut().unwrap().recipe;
        recipe.separator = Separator::Semicolon;
        recipe.header = HeaderSkip::None;
        recipe.header_layout = Default::default();
    }

    // Les données commencent à la ligne 4 : trois lignes d'en-tête.
    state.import_line_action(ImportLineAction::DataStartsHere, 4);
    assert_eq!(
        state
            .import_state()
            .unwrap()
            .recipe
            .header_layout
            .main_header_lines,
        3
    );
    assert_eq!(state.import_preview().unwrap().unwrap().row_count(), 3);

    // Fin des données à la ligne 6 (la dernière donnée est la ligne 6).
    state.import_line_action(ImportLineAction::DataEndsHere, 6);
    assert_eq!(state.import_preview().unwrap().unwrap().row_count(), 2);

    state.import_line_action(ImportLineAction::NoAutoCut, 6);
    assert_eq!(
        state.import_state().unwrap().recipe.data_end,
        plotyx_import::DataEnd::None
    );

    // La ligne 2 donne les unités : elle ouvre le sous-en-tête.
    state.import_line_action(ImportLineAction::UnitsLine, 2);
    let recipe = &state.import_state().unwrap().recipe;
    assert_eq!(recipe.header_layout.main_header_lines, 1);
    assert_eq!(recipe.header_layout.subheader_lines, 2);
    assert_eq!(
        recipe.subheader_roles.unit,
        Some(plotyx_import::SubheaderLineRange::single(1))
    );
}

/// L'étape « Colonnes » : un rôle Ignorer retire la colonne, erreur Y est
/// posée sur la colonne choisie.
#[test]
fn import_column_roles_from_the_wizard_are_applied() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("trois.csv");
    std::fs::write(&file, "1,10,0.1,7\n2,20,0.2,8\n").unwrap();
    let mut recipe = ImportRecipe::new("Colonnes");
    recipe.column_roles = vec![
        plotyx_import::ColumnRoleChoice::Auto,
        plotyx_import::ColumnRoleChoice::Y,
        plotyx_import::ColumnRoleChoice::YError,
        plotyx_import::ColumnRoleChoice::Ignore,
    ];

    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![file],
    });

    let sheet = sheet_named(state.tree(), "trois");
    let columns = state.visible_columns(sheet);
    assert_eq!(columns.len(), 3, "la quatrième colonne est ignorée");
    assert_eq!(state.column_meta(columns[0].0).role, ColumnRole::X);
    assert_eq!(state.column_meta(columns[1].0).role, ColumnRole::Y);
    assert_eq!(state.column_meta(columns[2].0).role, ColumnRole::YError);
}

/// Un dépôt pendant que l'assistant est ouvert ajoute les fichiers à sa liste.
#[test]
fn dropping_files_while_the_wizard_is_open_adds_them_to_its_list() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let file = jasco_like(dir.path());
    state.open_import(None::<NodeId>);
    state.dispatch(Action::DropFiles {
        paths: vec![file.clone()],
        target_book: None,
    });
    assert_eq!(state.import_state().unwrap().paths, vec![file.clone()]);
    state.dispatch(Action::DropFiles {
        paths: vec![file],
        target_book: None,
    });
    assert_eq!(state.import_state().unwrap().paths.len(), 1, "sans doublon");
}

// -- État Annuler après clôture d'une opération groupée (issue #22) --

/// L'instantané de l'UI reflète la macro close dès la fin de l'action : même
/// `can_undo` et même libellé que l'acteur.
fn assert_undo_state_matches_actor(session: &Session, state: &AppState, label: &str) {
    let actual = session.handle().snapshot().unwrap();
    assert_eq!(state.can_undo(), actual.history.can_undo);
    assert!(state.can_undo(), "{label} doit être annulable");
    assert_eq!(actual.history.undo_label.as_deref(), Some(label));
    assert_eq!(
        state.undo_label().as_deref(),
        Some(format!("Annuler {label}").as_str()),
        "le libellé du menu vient de l'entrée close"
    );
}

#[test]
fn undo_state_is_fresh_right_after_a_grouped_paste() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheet = state.selected_sheet_in(book).unwrap();
    let columns = state.visible_columns(sheet);
    state.select_cell(sheet, columns[0].0, 0);
    // Séance neuve : rien d'annulable avant le collage.
    while state.can_undo() {
        state.dispatch(Action::Undo);
    }
    state.dispatch(Action::Paste {
        sheet,
        text: "10\t20".into(),
    });
    assert_undo_state_matches_actor(&session, &state, "Coller");
}

#[test]
fn undo_state_is_fresh_right_after_a_grouped_clear() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheet = state.selected_sheet_in(book).unwrap();
    let columns = state.visible_columns(sheet);
    state.select_cell(sheet, columns[0].0, 0);
    state.extend_selection(sheet, columns[1].0, 0);
    while state.can_undo() {
        state.dispatch(Action::Undo);
    }
    state.dispatch(Action::ClearSelectedCells { sheet });
    assert_undo_state_matches_actor(&session, &state, "Effacer la sélection");
}

// -- Annuler les propriétés d'un graphe n'annule pas une saisie détachée (#21) --

/// Pendant que la fenêtre des propriétés d'un graphe tient une macro, une vue
/// détachée (autre identité d'émetteur) ne peut pas y glisser une écriture :
/// elle est refusée, et annuler les propriétés ne touche pas au classeur.
#[test]
fn cancelling_plot_settings_does_not_undo_a_detached_entry() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut main) = start_demo(dir.path());
    let graph = plotted_graph(&mut main);
    let book = book_named(main.tree(), "Book1");
    let sheet = main.selected_sheet_in(book).unwrap();
    let column = main.visible_columns(sheet)[0].0;
    let before = cell_f64(&main, column, 0);

    let mut detached = main.fork_for_detached_view().expect("vue détachée");
    main.dispatch(Action::OpenPlotSettings {
        graph,
        tab: PlotSettingsTab::Graph,
        axis: None,
    });
    detached.dispatch(Action::CommitCell {
        column,
        row: 0,
        text: "99".into(),
    });
    assert!(
        detached.status().is_some_and(|status| status.is_error),
        "la saisie étrangère est refusée pendant le dialogue"
    );
    main.cancel_plot_settings();
    detached.refresh();
    assert_eq!(cell_f64(&detached, column, 0), before);

    // Le dialogue fermé, la même saisie passe et reste.
    detached.dispatch(Action::CommitCell {
        column,
        row: 0,
        text: "99".into(),
    });
    main.refresh();
    assert_eq!(cell_f64(&main, column, 0), 99.0);
}

// -- Import en fond, reprise du dernier import, fichiers récents (issue #6) --

/// L'analyse tourne sur un fil de fond : `start_import` rend la main, la
/// progression se lit, et `poll_import` crée les feuilles une fois prêt —
/// toujours en une seule entrée d'historique. Un second lancement est refusé
/// pendant qu'un import est en cours.
#[test]
fn a_background_import_reports_progress_then_applies_in_one_entry() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();
    let files: Vec<std::path::PathBuf> = (0..3)
        .map(|i| {
            let file = dir.path().join(format!("fond{i}.csv"));
            std::fs::write(&file, "1,10\n2,20\n").unwrap();
            file
        })
        .collect();

    state.dispatch(Action::StartImport {
        destination: ImportDestination::ActiveBook(book),
        recipe: ImportRecipe::new("Fond"),
        paths: files.clone(),
    });
    let (_, total) = state.import_progress().expect("un import est en cours");
    assert_eq!(total, 3);
    state.dispatch(Action::StartImport {
        destination: ImportDestination::ActiveBook(book),
        recipe: ImportRecipe::new("Fond"),
        paths: files.clone(),
    });
    assert!(
        state.status().is_some_and(|status| status.is_error),
        "un second import est refusé"
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !state.poll_import() {
        assert!(std::time::Instant::now() < deadline, "import trop long");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(state.import_progress().is_none());
    assert_eq!(state.worksheets_of(book).len(), sheets_before + 3);
    assert_eq!(state.take_imported_paths().len(), 3);
    assert!(state.take_imported_paths().is_empty(), "file vidée");
    state.dispatch(Action::Undo);
    assert_eq!(state.worksheets_of(book).len(), sheets_before);
    assert!(!state.can_undo(), "une seule entrée pour tout le lot");
}

/// « Régler le dernier import » annule l'import s'il est encore au sommet de
/// l'historique, puis rouvre l'assistant sur ses fichiers et sa recette.
#[test]
fn reopening_the_last_import_undoes_it_and_prefills_the_wizard() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    assert!(!state.can_reopen_last_import());
    let file = dir.path().join("reprise.csv");
    std::fs::write(&file, "1;10\n2;20\n").unwrap();
    let mut recipe = ImportRecipe::new("Point-virgule");
    recipe.separator = Separator::Semicolon;
    let sheets_before = state.worksheets_of(book).len();
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe: recipe.clone(),
        paths: vec![file.clone()],
    });
    assert_eq!(state.worksheets_of(book).len(), sheets_before + 1);
    assert!(state.can_reopen_last_import());

    state.dispatch(Action::ReopenLastImport);

    assert_eq!(state.worksheets_of(book).len(), sheets_before, "annulé");
    let import = state.import_state().expect("assistant rouvert");
    assert_eq!(import.paths, vec![file]);
    assert_eq!(import.recipe.separator, Separator::Semicolon);
    assert_eq!(import.destination, ImportDestination::ActiveBook(book));
}

/// Si autre chose a été fait depuis, l'ancien import n'est pas annulé : on ne
/// défait jamais un travail étranger.
#[test]
fn reopening_the_last_import_leaves_later_work_alone() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("reprise.csv");
    std::fs::write(&file, "1,10\n2,20\n").unwrap();
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe: ImportRecipe::new("Sans titre"),
        paths: vec![file],
    });
    state.dispatch(Action::AddWorkbook);
    let root = state.tree().root();
    let books = state.tree().node(root).unwrap().children().len();

    state.dispatch(Action::ReopenLastImport);

    assert_eq!(state.tree().node(root).unwrap().children().len(), books);
    assert!(
        state.worksheets_of(book).len() >= 2,
        "l'import reste en place"
    );
    assert!(state.import_state().is_some());
}

// -- Modèles fournis, choix automatique, partage (issue #7) --

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/import")
        .join(name)
}

/// Un fichier Jasco déposé est lu avec le modèle fourni qui le reconnaît
/// (signature `ORIGIN;JASCO`), sans qu'aucun modèle n'ait été enregistré — les
/// nombres à virgule sont lus (décimale détectée).
#[test]
fn dropping_a_jasco_file_picks_the_builtin_model_automatically() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let before = state.worksheets_of(book).len();

    state.dispatch(Action::DropFiles {
        paths: vec![fixture_path("jasco_uvvis_semicolon.csv")],
        target_book: Some(book),
    });

    assert_eq!(state.worksheets_of(book).len(), before + 1);
    assert!(state.import_state().is_none(), "lu sans assistant");
    let status = state.status().expect("message");
    assert!(
        status.text.contains("Jasco (point-virgule)"),
        "{}",
        status.text
    );
    let sheet = sheet_named(state.tree(), "jasco_uvvis_semicolon");
    let columns = state.visible_columns(sheet);
    assert_eq!(cell_f64(&state, columns[0].0, 0), 800.0);
    assert_eq!(column_type(&state, columns[0].0), ColumnType::Numeric);
}

/// Dans l'assistant, le premier fichier ajouté choisit le modèle d'après son
/// contenu tant que le modèle courant n'a pas été modifié.
#[test]
fn the_wizard_chooses_the_model_from_the_first_file() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    assert_eq!(state.import_state().unwrap().recipe.name, "Sans titre");

    state.add_import_paths(vec![fixture_path("jasco_fluo_tab.txt")]);

    assert_eq!(
        state.import_state().unwrap().recipe.name,
        "Jasco (tabulation)"
    );
    assert!(!state.import_recipe_modified());
    // Un modèle modifié à la main n'est pas écrasé par un second ajout.
    state.import_state_mut().unwrap().recipe.skip_chars = 1;
    state.add_import_paths(vec![fixture_path("rpe_nohead_tab.asc")]);
    assert_eq!(
        state.import_state().unwrap().recipe.name,
        "Jasco (tabulation)"
    );
}

/// Un modèle s'exporte dans un fichier et se réimporte (même nom : il
/// remplace), et il est alors enregistré pour l'utilisateur.
#[test]
fn a_model_can_be_exported_and_imported_from_a_file() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    state.select_import_recipe("RPE (tabulation)");
    state.import_state_mut().unwrap().recipe.skip_chars = 4;
    let file = dir.path().join("rpe.json");
    state.export_import_recipe(&file);
    assert!(file.exists());

    state.open_import(None::<NodeId>);
    state.import_import_recipe(&file);

    let recipe = &state.import_state().unwrap().recipe;
    assert_eq!(recipe.name, "RPE (tabulation)");
    assert_eq!(recipe.skip_chars, 4);
    assert!(plotyx_import::RecipeStore::load()
        .recipes()
        .iter()
        .any(|r| r.name == "RPE (tabulation)" && r.skip_chars == 4));

    // Un fichier qui n'est pas un modèle : message d'erreur, rien ne change.
    let bad = dir.path().join("pas_un_modele.json");
    std::fs::write(&bad, "{ pas du json").unwrap();
    state.import_import_recipe(&bad);
    assert!(state.status().is_some_and(|s| s.is_error));
    assert_eq!(state.import_state().unwrap().recipe.skip_chars, 4);
}

/// Plusieurs modèles dans un même dépôt : chaque fichier a le sien, le tout en
/// une seule entrée d'historique.
#[test]
fn dropping_files_of_different_formats_uses_one_model_each_in_one_entry() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let before = state.worksheets_of(book).len();

    state.dispatch(Action::DropFiles {
        paths: vec![
            fixture_path("jasco_uvvis_semicolon.csv"),
            fixture_path("rpe_nohead_tab.asc"),
        ],
        target_book: Some(book),
    });

    assert_eq!(state.worksheets_of(book).len(), before + 2);
    let status = state.status().unwrap();
    assert!(status.text.contains("Jasco (point-virgule)"));
    assert!(status.text.contains("RPE (tabulation)"));
    state.dispatch(Action::Undo);
    assert_eq!(state.worksheets_of(book).len(), before);
}
