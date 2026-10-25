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

use stokesforge_app_state::history_text::history_text;
use stokesforge_app_state::selection::Selection;
use stokesforge_app_state::state::{
    active_graph, drop_payload_action, drop_rows_action, file_drop_plan, Action, ActiveTabKind,
    AlertKind, AppState, ColumnDragPayload, GridZone, HeaderField, ImportDestination,
    MoveDirection, PlotAxis, PlotEdit, PlotSettingsTab, PlotTarget, RowAxis, StatusLevel,
    UnsavedChoice, UnsavedGesture, INFO_LIFETIME, STATUS_HISTORY_LEN,
};
use stokesforge_commands::{ColumnRef, HistoryLabel, MacroKind};
use stokesforge_core::{
    Cell, Column, ColumnFormula, ColumnMeta, ColumnRole, ColumnType, FormulaPart, NodeId, Project,
    Tree,
};
use stokesforge_import::{
    DecimalStyle, HeaderSkip, ImportRecipe, NameSource, RecipeStore, Separator,
};
use stokesforge_plot::{AxisScale, PlotType, RawVariant};
use stokesforge_session::{Session, SessionHandle};

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
    let state = AppState::new_with_preferences(
        session.handle(),
        path.display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .expect("état applicatif initial");
    (session, state)
}

/// Session sur le projet de démonstration, fichier de travail dans `dir`.
fn start_demo(dir: &Path) -> (Session, AppState) {
    start(demo_project(), &dir.join("projet.stf"))
}

#[test]
fn graphique_vide_annulable_retabli_et_enregistre() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state) = start_demo(dir.path());
    let before = state.tree().len();
    state.dispatch(Action::AddGraph);
    let graph = *state
        .tree()
        .node(state.tree().root())
        .unwrap()
        .children()
        .last()
        .unwrap();
    let spec = state.plot_spec(graph).unwrap();
    assert!(
        spec.title.is_empty(),
        "#103 : un graphique vide n'a pas de titre"
    );
    assert_eq!(spec.panels.len(), 1);
    assert!(spec.panels.iter().all(|panel| panel.series.is_empty()));
    state.dispatch(Action::Undo);
    assert_eq!(state.tree().len(), before);
    state.dispatch(Action::Redo);
    assert!(state.tree().node(graph).is_some());
    state.dispatch(Action::Save);
    state.dispatch(Action::Load);
    assert!(state.plot_spec(graph).unwrap().title.is_empty());
    assert!(state.plot_spec(graph).unwrap().panels[0].series.is_empty());
    session.shutdown().unwrap();
}

#[test]
fn nouveau_projet_attend_confirmation_et_annulation_preserve_le_travail() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state) = start_demo(dir.path());
    let before = state.tree().len();
    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    assert_eq!(state.unsaved_pending(), Some(&UnsavedGesture::NewProject));
    assert!(state.blocking_dialog());
    assert_eq!(state.tree().len(), before);
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
    assert!(state.unsaved_pending().is_none());
    assert_eq!(state.tree().len(), before);
    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Discard));
    assert_eq!(state.tree().len(), 1);
    assert!(state.path().is_empty());
    assert!(!state.can_undo());
    // Pas de confirmation pour un second projet encore vide.
    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_none());
    state.dispatch(Action::AddWorkbook);
    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_some());
    session.shutdown().unwrap();
}

#[test]
fn nouveau_projet_apres_enregistrement_et_apres_echec() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state) = start_demo(dir.path());
    state.dispatch(Action::Save);
    state.dispatch(Action::NewProject {
        pending_edits: true,
    });
    assert!(state.unsaved_pending().is_some());
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_none());
    assert_eq!(state.tree().len(), 1);
    state.dispatch(Action::AddGraph);
    state.set_path(dir.path().display().to_string()); // une destination qui est un dossier
    state.dispatch(Action::Save);
    // S-M8 (#229) : un enregistrement raté est un échec grave, dit par un
    // dialogue bloquant qui propose Save As….
    let alert = state.alert().expect("échec grave");
    assert_eq!(alert.kind, AlertKind::SaveFailed);
    assert!(!alert.message.is_empty());
    assert!(state.blocking_dialog());
    state.dispatch(Action::DismissAlert);
    assert!(state.alert().is_none());
    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_some());
    session.shutdown().unwrap();
}

/// S-F4 (#229) : la réponse Save enregistre, **puis** poursuit le geste ; un
/// enregistrement raté l'abandonne (le travail reste) et le dit (S-M8).
#[test]
fn la_reponse_save_enregistre_puis_poursuit_le_geste() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state) = start_demo(dir.path());
    let file = dir.path().join("travail.stf");
    state.set_path(file.display().to_string());
    state.dispatch(Action::AddGraph);
    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Save));
    assert!(file.is_file(), "enregistré");
    assert_eq!(state.tree().len(), 1, "puis le nouveau projet");
    assert!(state.unsaved_pending().is_none());

    // Échec : le geste en attente est abandonné, le travail reste.
    state.dispatch(Action::AddWorkbook);
    let len = state.tree().len();
    state.set_path(dir.path().display().to_string());
    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Save));
    assert_eq!(state.tree().len(), len, "le travail reste");
    assert_eq!(
        state.alert().map(|alert| alert.kind),
        Some(AlertKind::SaveFailed)
    );
    assert!(state.unsaved_pending().is_none());
    // Un enregistrement réussi plus tard ne rejoue pas le geste abandonné.
    state.dispatch(Action::DismissAlert);
    state.set_path(file.display().to_string());
    state.dispatch(Action::Save);
    assert_eq!(state.tree().len(), len);

    // Sans fichier : Save note le geste, que l'enregistrement poursuivra
    // (la fenêtre demande où écrire puis pousse Action::Save) ; Cancel
    // (sélecteur fermé sans choix) l'oublie.
    state.dispatch(Action::AddGraph);
    state.set_path(String::new());
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Save));
    assert!(!state.take_quit_ready(), "rien d'enregistré encore");
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
    state.set_path(file.display().to_string());
    state.dispatch(Action::Save);
    assert!(!state.take_quit_ready(), "geste oublié");
    state.dispatch(Action::AddGraph);
    state.set_path(String::new());
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Save));
    state.set_path(file.display().to_string());
    state.dispatch(Action::Save);
    assert!(state.take_quit_ready(), "enregistré, puis Quit");
    session.shutdown().unwrap();
}

/// S-F3 (#229) : Quit et Open… demandent seulement s'il y a du travail à
/// perdre ; Discard poursuit. Un projet illisible garde le projet **et** le
/// chemin courants (S-M8).
#[test]
fn quitter_et_ouvrir_ne_demandent_que_si_du_travail_serait_perdu() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state) = start_demo(dir.path());
    let file = dir.path().join("a.stf");
    state.set_path(file.display().to_string());
    state.dispatch(Action::Save);
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_none());
    assert!(state.take_quit_ready(), "rien à perdre : Quit direct");
    assert!(!state.take_quit_ready(), "une seule fois");

    state.dispatch(Action::Quit {
        pending_edits: true,
    });
    assert_eq!(state.unsaved_pending(), Some(&UnsavedGesture::Quit));
    assert!(!state.take_quit_ready());
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Discard));
    assert!(state.take_quit_ready());

    // Open… d'un fichier illisible, sans travail à perdre.
    let len = state.tree().len();
    let bad = dir.path().join("illisible.stf");
    std::fs::write(&bad, "pas un projet").unwrap();
    state.dispatch(Action::OpenProject {
        path: bad.display().to_string(),
        pending_edits: false,
    });
    assert_eq!(
        state.alert().map(|alert| alert.kind),
        Some(AlertKind::OpenFailed)
    );
    assert_eq!(state.path(), file.display().to_string(), "chemin gardé");
    assert_eq!(state.tree().len(), len);
    state.dispatch(Action::DismissAlert);

    // Open… avec du travail : question, puis Discard ouvre.
    let other = dir.path().join("b.stf");
    state.set_path(other.display().to_string());
    state.dispatch(Action::Save);
    state.set_path(file.display().to_string());
    state.dispatch(Action::AddGraph);
    state.dispatch(Action::OpenProject {
        path: other.display().to_string(),
        pending_edits: false,
    });
    assert!(matches!(
        state.unsaved_pending(),
        Some(UnsavedGesture::Open(path)) if *path == other.display().to_string()
    ));
    assert_eq!(
        state.path(),
        file.display().to_string(),
        "pas encore ouvert"
    );
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Discard));
    assert_eq!(state.path(), other.display().to_string());
    assert_eq!(state.tree().len(), len);
    session.shutdown().unwrap();
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
                matches!(
                    node.kind(),
                    stokesforge_core::node::NodeKind::Workbook { .. }
                ) && node.name() == name
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

/// Redirige le dossier de config vers un répertoire temporaire propre à
/// l'appel, pour la durée de vie de la valeur rendue : `stokesforge_import::
/// RecipeStore::load`/`save` (donc `AppState::open_import`/`save_import_recipe`/
/// `delete_import_recipe`) passent par `stokesforge_import::config_dir::
/// config_dir`, qui lit `STOKESFORGE_CONFIG_DIR` (#332). `XDG_CONFIG_HOME`
/// est posée aussi : c'est la seule que `directories` respecte, et sur
/// Linux seulement. Sans `STOKESFORGE_CONFIG_DIR`, ces méthodes lisaient et
/// écrivaient le vrai profil de la machine sous Windows — non hermétique,
/// et en course entre tests (« Access is denied »).
///
/// Garde le verrou et le répertoire temporaire vivants jusqu'à ce que la
/// valeur rendue soit abandonnée (fin du test, y compris en cas de panique :
/// `Drop` restaure les deux variables à leur valeur d'avant).
fn isolate_recipe_store() -> RecipeStoreIsolation {
    let lock = IMPORT_RECIPES_ENV_LOCK
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    let dir = tempfile::tempdir().expect("répertoire temporaire pour les recettes d'import");
    let names = [
        stokesforge_import::config_dir::CONFIG_DIR_VARIABLE,
        "XDG_CONFIG_HOME",
    ];
    let previous = names.map(|name| (name, std::env::var_os(name)));
    for name in names {
        std::env::set_var(name, dir.path());
    }
    RecipeStoreIsolation {
        previous,
        _lock: lock,
        dir,
    }
}

/// Ce que [`isolate_recipe_store`] rend : le verrou, le dossier temporaire et
/// les valeurs d'avant des variables, restaurées à la chute.
struct RecipeStoreIsolation {
    previous: [(&'static str, Option<std::ffi::OsString>); 2],
    _lock: std::sync::MutexGuard<'static, ()>,
    dir: tempfile::TempDir,
}

impl RecipeStoreIsolation {
    /// Le dossier de config temporaire.
    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }
}

impl Drop for RecipeStoreIsolation {
    fn drop(&mut self) {
        for (name, value) in self.previous.iter_mut() {
            match value.take() {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

/// #332 : sous l'isolation, le dossier de config est le dossier temporaire
/// (`Path` égal, pas une chaîne), et un enregistrement de modèle d'import y
/// écrit son fichier. Sous Windows, `XDG_CONFIG_HOME` seule ne suffit pas :
/// `directories` l'ignore, et le modèle partait dans le vrai profil.
#[test]
fn l_isolation_redirige_le_dossier_de_config_et_l_enregistrement_y_ecrit() {
    let isolation = isolate_recipe_store();
    assert_eq!(
        stokesforge_import::config_dir::config_dir().as_deref(),
        Some(isolation.path()),
        "le dossier de config est le dossier temporaire"
    );
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    state.import_state_mut().unwrap().save_as_name = "Témoin".to_string();
    state.save_import_recipe();
    assert!(!state.status().unwrap().is_error());
    let file = isolation.path().join("import_recipes.json");
    assert!(file.is_file(), "le modèle est écrit dans {file:?}");
    assert!(std::fs::read_to_string(&file).unwrap().contains("Témoin"));
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

/// #222 : la grille peint 6 chiffres significatifs, mais l'édition, le copier
/// et la comparaison de saisie gardent la valeur entière.
#[test]
fn the_grid_paints_six_digits_but_edits_and_copies_the_exact_value() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    state.dispatch(Action::CommitCell {
        column: temps,
        row: 1,
        text: "12.345678901".to_string(),
    });

    assert_eq!(state.cell_display(temps, 1), "12.3457");
    assert_eq!(state.cell_text(temps, 1), "12.345678901");
    state.begin_edit(temps, 1);
    assert_eq!(
        state.editing().map(|edit| edit.buffer.clone()),
        Some("12.345678901".to_string()),
        "le tampon d'édition porte la valeur entière"
    );
}

/// #218 : une colonne Date/Time s'affiche, s'édite et se copie en ISO 8601 ;
/// `10/10/2026 14:30` (jour d'abord) est lu en timestamp ; un nombre nu reste
/// un timestamp ; un texte qui n'est pas une date promeut la colonne en Mixed.
#[test]
fn a_datetime_cell_shows_iso_and_reads_day_first_dates() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    state.dispatch(Action::SetColumnType {
        column: temps,
        column_type: ColumnType::DateTime,
    });
    state.dispatch(Action::CommitCell {
        column: temps,
        row: 0,
        text: "10/10/2026 14:30".to_string(),
    });
    assert_eq!(state.cell_display(temps, 0), "2026-10-10 14:30");
    assert_eq!(state.cell_text(temps, 0), "2026-10-10 14:30");
    let iso = match cell(&state, temps, 0) {
        Cell::Float(value) => value,
        other => panic!("timestamp attendu, reçu {other:?}"),
    };

    // Jour d'abord : 10/11/2026 est le 10 novembre.
    state.dispatch(Action::CommitCell {
        column: temps,
        row: 1,
        text: "10/11/2026".to_string(),
    });
    assert_eq!(state.cell_text(temps, 1), "2026-11-10 00:00");
    // La forme ISO se relit à l'identique.
    state.dispatch(Action::CommitCell {
        column: temps,
        row: 2,
        text: "2026-10-10 14:30".to_string(),
    });
    assert_eq!(cell(&state, temps, 2), Cell::Float(iso));
    // Un nombre nu reste un timestamp POSIX.
    state.dispatch(Action::CommitCell {
        column: temps,
        row: 3,
        text: "86400".to_string(),
    });
    assert_eq!(state.cell_text(temps, 3), "1970-01-02 00:00");
    // Mois d'abord (13e mois) : pas une date, la colonne devient Mixed.
    state.dispatch(Action::CommitCell {
        column: temps,
        row: 3,
        text: "10/13/2026".to_string(),
    });
    assert_eq!(column_type(&state, temps), ColumnType::Mixed);
}

/// #223 : la virgule est lue comme un point décimal dans une cellule Numeric ;
/// l'interface réécrit toujours avec le point. « 1,000 » est lu 1 (S-R6).
#[test]
fn a_decimal_comma_is_read_in_a_numeric_cell_and_shown_with_a_point() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.dispatch(Action::CommitCell {
        column: temps,
        row: 1,
        text: "42,5".to_string(),
    });
    assert_eq!(cell(&state, temps, 1), Cell::Float(42.5));
    assert_eq!(state.cell_text(temps, 1), "42.5");

    state.dispatch(Action::CommitCell {
        column: temps,
        row: 2,
        text: "1,000".to_string(),
    });
    assert_eq!(cell(&state, temps, 2), Cell::Float(1.0));

    // Témoin : une virgule ambiguë n'est pas un nombre, la colonne devient mixte
    // plutôt que de perdre le texte.
    state.dispatch(Action::CommitCell {
        column: temps,
        row: 3,
        text: "1,2,3".to_string(),
    });
    assert_eq!(state.cell_text(temps, 3), "1,2,3");
}

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
    // La colonne A, pas « Temps » : le libellé désigne la colonne comme la
    // grille l'affiche (sa lettre de position, `ColumnRef`), jamais par son
    // nom de nœud, qui n'apparaît nulle part à l'écran. `Temps` reste le nom
    // de nœud de cette colonne (c'est par lui que `column_id` la retrouve
    // ici), il n'a simplement plus sa place dans un libellé destiné à
    // l'utilisateur.
    assert_eq!(
        state.undo_label(),
        Some(undo_entry(&HistoryLabel::SetCells {
            column: Some(ColumnRef(0))
        }))
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
        state.status().is_none_or(|status| !status.is_error()),
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
        state.status().is_none_or(|status| !status.is_error()),
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
    // littérale pour ne pas dupliquer la constante hors de `stokesforge-app`.
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
        Some(redo_entry(&HistoryLabel::SetCells {
            column: Some(ColumnRef(0))
        }))
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
    assert_eq!(
        status.level,
        StatusLevel::Warning,
        "« rien à annuler » est le pendant du bouton grisé : un avertissement, pas une erreur"
    );
    assert_eq!(state.snapshot().revision, before);

    state.dispatch(Action::Redo);
    let status = state.status().expect("un message");
    assert!(!status.is_error());
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
        state.status().is_some_and(|status| !status.is_error()),
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
        state.status().is_some_and(|status| !status.is_error()),
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
    let mine = dir.path().join("mien.stf");
    let other = dir.path().join("autre.stf");

    // Le fichier étranger est écrit par une session distincte.
    {
        let (other_session, mut other_state) = start(single_sheet_project(), &other);
        other_state.dispatch(Action::Save);
        assert!(other_state.status().is_some_and(|s| !s.is_error()));
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

    state.set_path(dir.path().join("inexistant.stf").display().to_string());
    state.dispatch(Action::Load);

    // S-M8 (#229) : un projet illisible est un échec grave, dit par un dialogue.
    assert_eq!(
        state.alert().map(|alert| alert.kind),
        Some(AlertKind::OpenFailed)
    );
    assert_eq!(state.snapshot().revision, revision);
    assert_eq!(**state.tree(), *tree);
    assert!(state.can_undo(), "l'historique est préservé");
}

/// Un état dont le plafond d'ouverture est réglé par le fichier de préférences
/// de `dir` (`max_file_mb` en Mo, A2 #316).
fn start_with_max_file_mb(
    project: Project,
    path: &Path,
    dir: &Path,
    mb: u32,
) -> (Session, AppState) {
    let preferences_file = dir.join("preferences.json");
    std::fs::write(&preferences_file, format!(r#"{{"max_file_mb":{mb}}}"#)).unwrap();
    let session = Session::spawn(project, None).expect("thread de session");
    let state = AppState::new_with_preferences(
        session.handle(),
        path.display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(Some(preferences_file)),
    )
    .expect("état applicatif initial");
    (session, state)
}

/// A2 (#316) : un fichier au-dessus du plafond d'ouverture est refusé par un
/// dialogue, le projet et le chemin courants sont gardés.
#[test]
fn opening_a_file_over_the_limit_is_refused_and_keeps_the_project_and_path() {
    stokesforge_app_state::i18n::set_language(stokesforge_app_state::i18n::Language::English);
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let current = dir.path().join("projet.stf");
    let (_session, mut state) = start_with_max_file_mb(demo_project(), &current, dir.path(), 16);
    state.dispatch(Action::Save);
    let len = state.tree().len();
    // 16 Mo + 1 octet : le contenu n'a pas à être un projet, la taille suffit.
    let big = dir.path().join("gros.stf");
    std::fs::write(&big, vec![0u8; (16 << 20) + 1]).unwrap();
    state.dispatch(Action::OpenProject {
        path: big.display().to_string(),
        pending_edits: false,
    });
    let alert = state.alert().expect("un dialogue d'échec");
    assert_eq!(alert.kind, AlertKind::OpenFailed);
    assert!(
        alert.message.contains("too large") && alert.message.contains("Preferences"),
        "message inattendu : {}",
        alert.message
    );
    assert_eq!(state.path(), current.display().to_string(), "chemin gardé");
    assert_eq!(state.tree().len(), len, "projet gardé");
}

/// A2 (#316) : un enregistrement au-dessus du plafond réussit, mais l'avertit
/// (à la place de « Enregistré »), en disant comment relever la limite.
#[test]
fn saving_over_the_opening_limit_succeeds_with_a_warning() {
    stokesforge_app_state::i18n::set_language(stokesforge_app_state::i18n::Language::English);
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let file = dir.path().join("gros.stf");
    let mut project = demo_project();
    let root = project.root();
    let book = project.add_workbook(root, Some("Gros")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    // 2,3 millions de réels : plus de 16 Mo une fois écrits.
    project
        .add_column(
            sheet,
            None,
            Some("N"),
            Column::from_f64(&vec![1.5; 2_300_000]),
        )
        .unwrap();
    let (_session, mut state) = start_with_max_file_mb(project, &file, dir.path(), 16);
    state.dispatch(Action::Save);
    assert!(file.exists(), "l'enregistrement a réussi");
    assert!(state.alert().is_none());
    let status = state.status().expect("un message");
    assert_eq!(status.level, StatusLevel::Warning);
    assert!(
        status.text.contains("Preferences") && status.text.contains("16 MB"),
        "avertissement inattendu : {}",
        status.text
    );

    // Sous le plafond : le message habituel, sans avertissement.
    let (_session, mut small) = start_with_max_file_mb(
        demo_project(),
        &dir.path().join("petit.stf"),
        dir.path(),
        16,
    );
    small.dispatch(Action::Save);
    let status = small.status().expect("un message");
    assert_ne!(status.level, StatusLevel::Warning);
    assert!(status.text.starts_with("Saved to"), "{}", status.text);
}

/// Un calcul illisible dans le fichier est écarté (le projet se charge) mais
/// dit par un avertissement de la barre d'état, plus en silence (#278).
#[test]
fn loading_a_file_with_an_unreadable_calculation_warns() {
    use stokesforge_analysis::correct::CorrectOp;
    use stokesforge_analysis::ops::Operation;
    use stokesforge_core::{
        CalculationDraft, Column, ColumnMeta, OutputDraft, SeriesRef, SeriesResult,
    };
    stokesforge_app_state::i18n::set_language(stokesforge_app_state::i18n::Language::English);
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("calcul.stf");

    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let y = project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&[1.0, 2.0]))
        .unwrap();
    project
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
            auto_recalc: true,
            low_r2_dialog: true,
        })
        .unwrap();
    stokesforge_persistence::save(&project, &path).expect("écriture");
    // Même longueur : l'opération devient illisible sans décaler un octet.
    let mut bytes = std::fs::read(&path).unwrap();
    let at = bytes
        .windows(8)
        .position(|w| w == b"subtract")
        .expect("l'opération est dans le fichier");
    bytes[at..at + 8].copy_from_slice(b"subtraXX");
    std::fs::write(&path, &bytes).unwrap();

    let (_session, mut state) = start_demo(dir.path());
    state.set_path(path.display().to_string());
    state.dispatch(Action::Load);

    assert!(state.alert().is_none(), "le projet se charge");
    let status = state.status().expect("un message");
    assert_eq!(status.level, StatusLevel::Warning);
    assert!(
        status.text.contains("was discarded") && status.text.contains('#'),
        "{}",
        status.text
    );
}

/// Une erreur de lecture `.stf` s'affiche par la phrase du catalogue de
/// l'application (#47), pas par le texte brut du crate du dessous : « is damaged »
/// ici, « corrupt file » dans le `Display` que lit le CLI.
#[test]
fn a_stf_read_error_is_shown_through_the_catalogue() {
    // Compared in English: the test must not depend on `STOKESFORGE_LANG` or on the
    // saved preference of whoever runs it.
    stokesforge_app_state::i18n::set_language(stokesforge_app_state::i18n::Language::English);
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());

    let truncated = dir.path().join("tronque.stf");
    std::fs::write(&truncated, b"STKF\x01").expect("écriture du fichier de test");
    state.set_path(truncated.display().to_string());
    state.dispatch(Action::Load);
    let alert = state.alert().expect("un échec grave (S-M8)");
    assert_eq!(
        alert.message,
        format!(
            "{} is damaged: header truncated before the version number",
            truncated.display()
        )
    );

    let foreign = dir.path().join("etranger.stf");
    std::fs::write(&foreign, b"hello").expect("écriture du fichier de test");
    state.set_path(foreign.display().to_string());
    state.dispatch(Action::DismissAlert);
    state.dispatch(Action::Load);
    let alert = state.alert().expect("un échec grave (S-M8)");
    assert_eq!(
        alert.message,
        format!("{} is not a StokesForge file", foreign.display())
    );
}

#[test]
fn a_failing_save_reports_an_error_and_writes_nothing() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let target = dir.path().join("absent").join("projet.stf");

    state.set_path(target.display().to_string());
    state.dispatch(Action::Save);

    assert_eq!(
        state.alert().map(|alert| alert.kind),
        Some(AlertKind::SaveFailed),
        "S-M8 : dialogue bloquant"
    );
    assert!(!target.exists(), "rien n'a été écrit");
    assert!(!target.parent().is_some_and(Path::exists));
}

/// #315 : un fichier qui existe au chemin de départ ne fait pas du projet un
/// projet « enregistré » — seul un vrai chargement ([`AppState::mark_loaded`])
/// pose le repère. Sinon Ctrl+S écraserait ce fichier sans contrôle.
#[test]
fn un_fichier_existant_ne_rend_pas_le_projet_enregistre_sans_chargement() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("existant.stf");
    let (writer, _) = start_demo(dir.path());
    writer.handle().save(&existing).unwrap();
    assert!(existing.is_file());

    let (_session, mut state) = start(demo_project(), &existing);
    let title = state.window_title();
    assert!(title.contains("existant.stf •"), "{title}");
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    assert_eq!(state.unsaved_pending(), Some(&UnsavedGesture::Quit));

    // Témoin : le chargement explicite pose le repère.
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
    state.mark_loaded(&stokesforge_session::LoadReport::default());
    let title = state.window_title();
    assert!(!title.contains('•'), "{title}");
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_none());
}

/// #315 : « Enregistrer sous » raté rétablit l'ancien chemin ; le Ctrl+S
/// suivant n'écrit pas sur le fichier qui vient d'échouer.
#[test]
fn un_enregistrer_sous_rate_retablit_l_ancien_chemin() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let old = dir.path().join("projet.stf");
    let target = dir.path().join("absent").join("x.stf");

    state.dispatch(Action::SaveAs {
        path: target.display().to_string(),
    });
    assert_eq!(
        state.alert().map(|alert| alert.kind),
        Some(AlertKind::SaveFailed)
    );
    assert_eq!(state.path(), old.display().to_string());

    state.dispatch(Action::DismissAlert);
    state.dispatch(Action::Save);
    assert!(old.is_file(), "Save écrit l'ancien fichier");
    assert!(!target.exists());
    assert!(!target.parent().is_some_and(Path::exists));
}

#[test]
fn un_enregistrer_sous_reussi_change_de_chemin_et_marque_enregistre() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let target = dir.path().join("nouveau.stf");
    state.dispatch(Action::AddGraph);
    state.dispatch(Action::SaveAs {
        path: target.display().to_string(),
    });
    assert!(target.is_file());
    assert_eq!(state.path(), target.display().to_string());
    let title = state.window_title();
    assert!(title.contains("nouveau.stf"), "{title}");
    assert!(!title.contains('•'), "{title}");
}

/// #315 : sans chemin, le titre ne reste pas vide.
#[test]
fn sans_chemin_le_titre_porte_un_nom_de_repli() {
    let (_session, state) = start(demo_project(), Path::new(""));
    let title = state.window_title();
    assert!(
        title.starts_with(&stokesforge_app_state::tr!("window-title-untitled")),
        "{title}"
    );
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
        state.undo_label(),
        Some(undo_entry(&HistoryLabel::Macro(MacroKind::AddWorkbook)))
    );

    state.dispatch(Action::Undo);
    assert_eq!(**state.tree(), *before);

    state.dispatch(Action::Redo);
    assert_eq!(**state.tree(), *after);
}

/// Les enfants directs de `parent`, avec leur nom.
fn child_names(
    state: &AppState,
    parent: stokesforge_core::NodeId,
) -> Vec<(stokesforge_core::NodeId, String)> {
    state
        .tree()
        .node(parent)
        .expect("parent")
        .children()
        .iter()
        .map(|id| {
            (
                *id,
                state.tree().node(*id).expect("enfant").name().to_string(),
            )
        })
        .collect()
}

/// #326 : Add Folder crée un dossier nommé, annulable et rétablissable.
#[test]
fn creating_a_folder_is_undoable_and_redoable() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let root = state.tree().root();
    let before = std::sync::Arc::clone(state.tree());

    state.dispatch(Action::AddFolder { parent: root });

    let after = std::sync::Arc::clone(state.tree());
    assert_eq!(after.len(), before.len() + 1);
    let (folder, name) = child_names(&state, root).pop().expect("dossier créé");
    assert_eq!(name, "Folder1");
    assert!(matches!(
        state.tree().node(folder).unwrap().kind(),
        stokesforge_core::NodeKind::Folder { .. }
    ));
    assert_eq!(
        state.undo_label(),
        Some(undo_entry(&HistoryLabel::AddFolder { name }))
    );

    state.dispatch(Action::Undo);
    assert_eq!(**state.tree(), *before);

    state.dispatch(Action::Redo);
    assert_eq!(**state.tree(), *after);
}

/// #326 : deux créations de suite ne se heurtent pas (unicité fraternelle).
#[test]
fn two_folders_get_distinct_names() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let root = state.tree().root();

    state.dispatch(Action::AddFolder { parent: root });
    state.dispatch(Action::AddFolder { parent: root });

    assert!(state.status().is_none_or(|status| !status.is_error()));
    let names: Vec<_> = child_names(&state, root)
        .into_iter()
        .map(|(_, name)| name)
        .filter(|name| name.starts_with("Folder"))
        .collect();
    assert_eq!(names, ["Folder1", "Folder2"]);
}

/// #326 : le nom se cherche parmi les enfants du dossier visé, pas de la racine.
#[test]
fn a_folder_is_created_inside_the_chosen_parent() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let root = state.tree().root();
    state.dispatch(Action::AddFolder { parent: root });
    let (outer, _) = child_names(&state, root).pop().expect("Folder1");
    let root_children = child_names(&state, root).len();

    state.dispatch(Action::AddFolder { parent: outer });

    assert!(state.status().is_none_or(|status| !status.is_error()));
    let inner = child_names(&state, outer);
    assert_eq!(
        inner.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>(),
        ["Folder1"],
        "libre dans le sous-dossier, même si la racine porte déjà Folder1"
    );
    assert_eq!(child_names(&state, root).len(), root_children);
}

/// #326 : un parent disparu échoue en barre d'état, sans entrée d'historique.
#[test]
fn a_folder_in_a_removed_parent_fails_without_history() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let root = state.tree().root();
    state.dispatch(Action::AddFolder { parent: root });
    let (gone, _) = child_names(&state, root).pop().expect("dossier");
    state.dispatch(Action::DeleteObjects(vec![gone]));
    assert!(state.tree().node(gone).is_none(), "le dossier est supprimé");
    let label = state.undo_label();
    let revision = state.snapshot().revision;

    state.dispatch(Action::AddFolder { parent: gone });

    assert!(state.status().is_some_and(|status| status.is_error()));
    assert_eq!(state.undo_label(), label);
    assert_eq!(state.snapshot().revision, revision);
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
    let (_session, mut state) = start(project, &dir.path().join("vide.stf"));

    assert!(state.visible_columns(graph).is_empty());
    assert_eq!(state.row_count(graph), 0);

    let revision = state.snapshot().revision;
    state.dispatch(Action::InsertRows {
        sheet: graph,
        at: 0,
        count: 1,
    });
    assert!(state.status().is_some_and(|status| status.is_error()));
    assert_eq!(state.snapshot().revision, revision);

    state.dispatch(Action::AddColumn { sheet: graph });
    assert!(state.status().is_some_and(|status| status.is_error()));
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
    assert!(state.status().is_some_and(|status| status.is_error()));
    assert_eq!(
        state.selected_sheet_in(book1),
        selected,
        "sélection inchangée"
    );

    state.dispatch(Action::SelectSheet(NodeId::from_raw(9_999)));
    assert!(state.status().is_some_and(|status| status.is_error()));
    assert_eq!(state.selected_sheet_in(book1), selected);
}

/// Le texte du menu Annuler pour `label`, dans la langue courante : le test
/// compare la **structure** (quelle entrée, quels paramètres), pas une langue ;
/// les phrases anglaises sont tenues par `tests/history_text.rs`.
fn undo_entry(label: &HistoryLabel) -> String {
    stokesforge_app_state::tr!("history-undo-entry", label = history_text(label))
}

/// Le texte du menu Rétablir pour `label`, comme [`undo_entry`].
fn redo_entry(label: &HistoryLabel) -> String {
    stokesforge_app_state::tr!("history-redo-entry", label = history_text(label))
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
    assert_eq!(undo, undo_entry(&HistoryLabel::AddColumn));
    assert_eq!(state.redo_label(), None, "rien à rétablir");

    state.dispatch(Action::Undo);
    let redo = state.redo_label().expect("un libellé de rétablissement");
    assert_eq!(redo, redo_entry(&HistoryLabel::AddColumn));
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

    let (_session, mut state) = start(project, &dir.path().join("projet.stf"));
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
// `stokesforge_app::app` (egui) traduit clic/glisser en `AppState::select_cell`/
// `extend_selection`, puis Ctrl+C/Ctrl+Maj+C/X/V/Ctrl+Maj+V en
// `AppState::copy_selection`/`AppState::copy_full_columns`/
// `Action::ClearSelectedCells`/`Action::Paste`/`Action::PasteAbsolute` —
// tout ce qui suit ce point de traduction est testable ici, sans fenêtre.
// Bug « le copier/couper/coller ne marche pas avec une sélection d'une
// seule cellule » : une sélection posée par un simple clic
// ([`AppState::select_cell`]) est un rectangle 1×1 comme un autre pour
// `Selection` (voir `crate::selection::Selection::single_cell` et ses tests
// unitaires) — les trois tests qui suivent le vérifient au niveau où
// `stokesforge_app::app` s'arrête.

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

// -- Saisie et collage : une seule règle de lecture (#258) --

/// #258, reproduction : une date collée garde sa valeur de date, comme une date
/// tapée. Deux lignes `2026-10-15` puis `n/a` collées dans une colonne
/// DateTime l'élargissent en Mixed ; la première case reste un timestamp, la
/// seconde est du texte, et un seul Annuler rend la colonne DateTime.
#[test]
fn a_pasted_date_keeps_its_value_when_the_column_widens_to_mixed() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    state.dispatch(Action::SetColumnType {
        column: temps,
        column_type: ColumnType::DateTime,
    });
    let attendu = stokesforge_core::datetime::parse_datetime("2026-10-15").expect("date");

    state.select_cell(sheet1, temps, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "2026-10-15\nn/a".to_string(),
    });

    assert_eq!(column_type(&state, temps), ColumnType::Mixed);
    assert_eq!(cell(&state, temps, 0), Cell::Float(attendu));
    assert_eq!(cell(&state, temps, 1), Cell::Text(Some("n/a".to_string())));

    // Témoin : la même suite tapée case par case donne les mêmes cellules.
    let (_autre, mut tape) = start_demo(dir.path());
    let temps_tape = column_id(&tape, sheet_named(tape.tree(), "Sheet1"), "Temps");
    tape.dispatch(Action::SetColumnType {
        column: temps_tape,
        column_type: ColumnType::DateTime,
    });
    for (row, text) in ["2026-10-15", "n/a"].into_iter().enumerate() {
        tape.dispatch(Action::CommitCell {
            column: temps_tape,
            row,
            text: text.to_string(),
        });
    }
    assert_eq!(cell(&tape, temps_tape, 0), cell(&state, temps, 0));
    assert_eq!(cell(&tape, temps_tape, 1), cell(&state, temps, 1));

    state.dispatch(Action::Undo);
    assert_eq!(column_type(&state, temps), ColumnType::DateTime);
    assert_eq!(cell(&state, temps, 0), Cell::Float(0.0));
}

/// #258 : `inf` et `nan` tapés restent du texte, même dans une colonne déjà
/// Mixed (avant, `MixedCell::from_text` les lisait en nombre) ; la colonne reste
/// Mixed et un infini n'entre jamais dans les données par la grille.
#[test]
fn typing_or_pasting_inf_and_nan_in_a_mixed_column_keeps_text() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");
    assert_eq!(column_type(&state, note), ColumnType::Mixed);

    state.dispatch(Action::CommitCell {
        column: note,
        row: 0,
        text: "inf".to_string(),
    });
    assert_eq!(state.cell_text(note, 0), "inf");
    assert_eq!(cell(&state, note, 0), Cell::Text(Some("inf".to_string())));
    assert_eq!(column_type(&state, note), ColumnType::Mixed);

    state.select_cell(sheet1, note, 1);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "-Infinity\nnan".to_string(),
    });
    assert_eq!(
        cell(&state, note, 1),
        Cell::Text(Some("-Infinity".to_string()))
    );
    assert_eq!(cell(&state, note, 2), Cell::Text(Some("nan".to_string())));
    assert_eq!(column_type(&state, note), ColumnType::Mixed);
}

/// #258 : après un collage, une colonne Mixed dont toutes les cases présentes
/// sont des nombres repasse en Numeric, comme après une saisie. La `Note` de la
/// démo vaut `[Some("a"), None, Some("")]` : le collage écrase les trois
/// cases (le `Some("")` bloquerait la repromotion). Un seul Annuler défait le
/// collage **et** la repromotion.
#[test]
fn pasting_numbers_over_a_mixed_column_makes_it_numeric_again_in_one_undo() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");

    state.select_cell(sheet1, note, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "1\n\n2".to_string(),
    });

    assert_eq!(column_type(&state, note), ColumnType::Numeric);
    assert_eq!(cell(&state, note, 0), Cell::Float(1.0));
    assert!(cell(&state, note, 1).is_missing());
    assert_eq!(cell(&state, note, 2), Cell::Float(2.0));
    assert_eq!(
        state.undo_label(),
        Some(undo_entry(&HistoryLabel::Macro(MacroKind::PasteCells))),
        "collage et repromotion forment une entrée"
    );

    state.dispatch(Action::Undo);
    assert_eq!(column_type(&state, note), ColumnType::Mixed);
    assert_eq!(cell(&state, note, 0), Cell::Text(Some("a".to_string())));
    assert_eq!(cell(&state, note, 2), Cell::Text(Some(String::new())));
}

/// #258 : un collage de plusieurs colonnes qui repromeut `Note` reste **une**
/// entrée d'historique (la repromotion se fait avant la fermeture du groupe).
#[test]
fn a_multi_column_paste_with_a_repromotion_is_one_undo() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");
    while state.can_undo() {
        state.dispatch(Action::Undo);
    }

    state.select_cell(sheet1, signal, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "5\t1\n6\t\n7\t2".to_string(),
    });
    assert_eq!(column_type(&state, note), ColumnType::Numeric);
    assert_eq!(cell(&state, signal, 0), Cell::Float(5.0));
    assert_eq!(cell(&state, note, 2), Cell::Float(2.0));

    state.dispatch(Action::Undo);
    assert_eq!(column_type(&state, note), ColumnType::Mixed);
    assert_eq!(cell(&state, note, 0), Cell::Text(Some("a".to_string())));
    assert_eq!(cell(&state, signal, 0), Cell::Float(10.0));
    assert!(!state.can_undo(), "un seul Annuler a tout défait");
}

/// #258, les deux autres points de collage : une case copiée collée
/// (`GridClipboard::Text`) et une colonne entière collée
/// (`GridClipboard::FullColumns`) repromeuvent aussi, en une entrée.
#[test]
fn single_value_and_full_column_pastes_also_repromote_a_mixed_column() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    // Une case : `Signal` passe en Mixed, reçoit un « n/a » puis un nombre.
    state.dispatch(Action::SetColumnType {
        column: signal,
        column_type: ColumnType::Mixed,
    });
    state.select_cell(sheet1, signal, 1);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "n/a".to_string(),
    });
    assert_eq!(column_type(&state, signal), ColumnType::Mixed);
    state.select_cell(sheet1, temps, 2);
    let copie = state.copy_selection(sheet1).expect("sélection non vide");
    state.select_cell(sheet1, signal, 1);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: copie,
    });
    assert_eq!(column_type(&state, signal), ColumnType::Numeric);
    assert_eq!(cell(&state, signal, 1), Cell::Float(1.0));
    state.dispatch(Action::Undo);
    assert_eq!(column_type(&state, signal), ColumnType::Mixed);
    assert_eq!(cell(&state, signal, 1), Cell::Text(Some("n/a".to_string())));

    // Une colonne entière : `Temps` collée sur la `Note` mixte.
    let note = column_id(&state, sheet1, "Note");
    state.select_cell(sheet1, temps, 0);
    let colonnes = state.copy_full_columns(sheet1).expect("sélection non vide");
    state.select_cell(sheet1, note, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: colonnes,
    });
    assert_eq!(column_type(&state, note), ColumnType::Numeric);
    assert_eq!(cell(&state, note, 4), Cell::Float(2.0));
    state.dispatch(Action::Undo);
    assert_eq!(column_type(&state, note), ColumnType::Mixed);
    assert_eq!(cell(&state, note, 0), Cell::Text(Some("a".to_string())));
}

/// #258 : un collage sur une colonne Mixed qui reste Mixed n'ouvre pas de
/// groupe pour rien — c'est la même entrée simple qu'avant.
#[test]
fn a_paste_that_leaves_a_mixed_column_mixed_is_a_plain_entry() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");

    state.select_cell(sheet1, note, 1);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "x\ny".to_string(),
    });
    assert_eq!(column_type(&state, note), ColumnType::Mixed);
    assert_eq!(
        state.undo_label(),
        Some(undo_entry(&HistoryLabel::SetCells {
            column: Some(ColumnRef(2))
        }))
    );
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
/// niveau où `stokesforge_app::app` s'arrête : la sélection clampée ne copie que des
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
    // la matrice de comportement, voir `stokesforge_app_state::state`).
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

/// #311 : une case ABSENTE (ligne plus courte que la colonne visée) garde la
/// valeur existante ; seule la case présente est écrite.
#[test]
fn a_pasted_row_shorter_than_the_others_keeps_the_existing_cell() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, temps, 2);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "1\t2\n3\n4\t5".to_string(),
    });
    assert_eq!(cell(&state, temps, 2), Cell::Float(1.0));
    assert_eq!(cell(&state, signal, 2), Cell::Float(2.0));
    assert_eq!(cell(&state, temps, 3), Cell::Float(3.0));
    assert_eq!(
        cell(&state, signal, 3),
        Cell::Float(40.0),
        "case absente entre deux cases présentes : la valeur existante reste"
    );
    assert_eq!(cell(&state, signal, 4), Cell::Float(5.0));
}

/// #311 : une case PRÉSENTE et vide (`"3\t"`) reste une valeur manquante.
#[test]
fn a_present_but_empty_pasted_cell_is_still_a_missing_value() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, temps, 2);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "1\t2\n3\t".to_string(),
    });
    assert_eq!(cell(&state, temps, 3), Cell::Float(3.0));
    assert!(matches!(cell(&state, signal, 3), Cell::Float(v) if v.is_nan()));
}

/// #311 : la dernière ligne courte n'allonge pas la colonne la plus courte.
#[test]
fn a_short_last_row_does_not_lengthen_the_shorter_column() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let len = |state: &AppState, id| {
        state
            .tree()
            .node(id)
            .and_then(|n| n.as_column())
            .map(|c| c.len())
    };

    state.select_cell(sheet1, temps, 4);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "1\t2\n3".to_string(),
    });
    assert_eq!(len(&state, temps), Some(6), "Temps reçoit ses deux cases");
    assert_eq!(
        len(&state, signal),
        Some(5),
        "Signal ne reçoit que la sienne"
    );
    assert_eq!(cell(&state, signal, 4), Cell::Float(2.0));
}

/// #311 : même collage sur une colonne `Mixed` — la case absente garde sa
/// cellule telle quelle, et le tout reste une seule entrée d'historique.
#[test]
fn a_short_row_pasted_over_a_mixed_column_keeps_its_cell_in_one_undo() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");
    while state.can_undo() {
        state.dispatch(Action::Undo);
    }

    state.select_cell(sheet1, temps, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "1\t2\tx\n3\t4\n5\t6\ty".to_string(),
    });
    assert_eq!(cell(&state, note, 0), Cell::Text(Some("x".to_string())));
    assert_eq!(
        cell(&state, note, 1),
        Cell::Text(None),
        "case absente : la cellule Mixed existante reste"
    );
    assert_eq!(cell(&state, note, 2), Cell::Text(Some("y".to_string())));
    assert_eq!(cell(&state, signal, 1), Cell::Float(4.0));

    state.dispatch(Action::Undo);
    assert_eq!(cell(&state, note, 0), Cell::Text(Some("a".to_string())));
    assert_eq!(cell(&state, note, 2), Cell::Text(Some(String::new())));
    assert_eq!(cell(&state, temps, 0), Cell::Float(0.0));
    assert_eq!(cell(&state, signal, 0), Cell::Float(10.0));
    assert!(!state.can_undo(), "un seul Annuler a tout défait");
}

/// #311 : plus de colonnes copiées que la feuille n'en a à partir de l'ancre :
/// celles qui tiennent s'écrivent, le statut compte les autres.
#[test]
fn a_paste_wider_than_the_sheet_reports_the_dropped_columns() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_cell(sheet1, signal, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "1\t2\t3\n4\t5\t6".to_string(),
    });
    assert_eq!(cell(&state, signal, 0), Cell::Float(1.0));
    assert_eq!(cell(&state, note, 0), Cell::Float(2.0));
    assert_eq!(cell(&state, temps, 0), Cell::Float(0.0));
    let status = state.status().expect("un message");
    assert!(!status.is_error());
    assert!(
        status.text.contains("One column"),
        "statut inattendu : {}",
        status.text
    );
}

/// #311 : un collage qui tient en largeur ne pose aucun statut de débordement.
#[test]
fn a_paste_that_fits_the_sheet_width_reports_nothing() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_cell(sheet1, temps, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "1\t2\t3\n4\t5\t6".to_string(),
    });
    assert!(
        state
            .status()
            .is_none_or(|status| !status.text.contains("did not fit")),
        "aucun statut de débordement"
    );
}

// -- #311 (E2, E4) : avis de virgule décimale, champs d'en-tête débordants --

fn status_text(state: &AppState) -> String {
    state.status().map(|s| s.text.clone()).unwrap_or_default()
}

/// #311 (E2) : « 1,234 » saisi dans une cellule Numeric reste lu 1.234 (R6),
/// et la barre d'état le dit ; « 1,5 » n'a rien d'ambigu : aucun avis.
#[test]
fn typing_a_thousands_looking_number_in_a_numeric_cell_notices_the_decimal_comma() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.dispatch(Action::CommitCell {
        column: temps,
        row: 1,
        text: "1,234".to_string(),
    });
    assert_eq!(cell(&state, temps, 1), Cell::Float(1.234));
    let status = state.status().expect("un avis");
    assert!(!status.is_error());
    assert!(
        status.text.contains("1,234") && status.text.contains("1.234"),
        "avis inattendu : {}",
        status.text
    );

    state.dispatch(Action::CommitCell {
        column: temps,
        row: 2,
        text: "1,5".to_string(),
    });
    assert_eq!(cell(&state, temps, 2), Cell::Float(1.5));
    assert!(
        !status_text(&state).contains("decimal comma"),
        "pas d'avis pour 1,5 : {}",
        status_text(&state)
    );
}

/// #311 (E2) : l'avis vaut aussi pour une cellule d'une colonne Mixed.
#[test]
fn the_decimal_comma_notice_also_applies_to_a_mixed_cell() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    state.dispatch(Action::CommitCell {
        column: temps,
        row: 0,
        text: "n/a".to_string(),
    });
    state.dispatch(Action::CommitCell {
        column: temps,
        row: 1,
        text: "12,345".to_string(),
    });
    assert!(
        status_text(&state).contains("decimal comma"),
        "avis attendu : {}",
        status_text(&state)
    );
}

/// #311 (E2) : le collage ne pose pas l'avis (saisie de cellule seulement).
#[test]
fn pasting_a_thousands_looking_number_does_not_notice() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    state.select_cell(sheet1, temps, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "1,234".to_string(),
    });
    assert!(!status_text(&state).contains("decimal comma"));
}

/// Lettres affichées de la formule d'une colonne (#321 : la formule est
/// stockée par identifiants, les lettres ne sont qu'un affichage).
fn formula_letters(state: &AppState, column: NodeId) -> Option<String> {
    state.tree().formula_letters(column)
}

/// Copie de trois rangées d'en-tête (Nom long, Unité, Commentaire) d'une
/// colonne, prête à coller ; renvoie le texte copié.
fn copy_three_header_rows(state: &mut AppState, sheet: NodeId, column: NodeId) -> String {
    set_header_field(state, column, HeaderField::LongName, "A");
    set_header_field(state, column, HeaderField::Unit, "u");
    set_header_field(state, column, HeaderField::Comment, "c");
    state.select_header_field(sheet, column, HeaderField::LongName);
    state.extend_header_field_selection(sheet, column, HeaderField::Comment);
    state.copy_selection(sheet).expect("sélection non vide")
}

/// #311 (E4) : trois rangées collées avec la première sur Formule : Formule
/// est écrite, les deux autres rangées sont comptées dans le statut, le tout
/// en une seule entrée d'historique.
#[test]
fn a_header_field_paste_anchored_low_counts_the_rows_beyond_formula() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let note = column_id(&state, sheet1, "Note");
    let text = copy_three_header_rows(&mut state, sheet1, temps);

    state.select_header_field(sheet1, note, HeaderField::Formula);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });
    assert_eq!(formula_letters(&state, note).as_deref(), Some("A"));
    assert_eq!(state.column_meta(note).unit, "", "rangée jetée");
    let status = state.status().expect("un statut");
    assert!(!status.is_error(), "{}", status.text);
    assert!(
        status.text.contains("2 header fields did not fit"),
        "statut inattendu : {}",
        status.text
    );
    state.dispatch(Action::Undo);
    assert_eq!(formula_letters(&state, note), None, "un Annuler suffit");
}

/// #311 (E4) : ancré sur Nom long, les trois rangées tiennent : aucun statut.
#[test]
fn a_header_field_paste_anchored_on_long_name_reports_nothing() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let note = column_id(&state, sheet1, "Note");
    let text = copy_three_header_rows(&mut state, sheet1, temps);

    state.select_header_field(sheet1, note, HeaderField::LongName);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });
    assert_eq!(state.column_meta(note).comment, "c");
    assert!(
        !status_text(&state).contains("did not fit"),
        "statut inattendu : {}",
        status_text(&state)
    );
}

/// #311 (E4) : colonnes et champs qui débordent ensemble : un seul message.
#[test]
fn columns_and_header_fields_dropped_together_give_one_combined_notice() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");
    for column in [temps, signal] {
        set_header_field(&mut state, column, HeaderField::LongName, "A");
        set_header_field(&mut state, column, HeaderField::Unit, "u");
    }
    state.select_header_field(sheet1, temps, HeaderField::LongName);
    state.extend_header_field_selection(sheet1, signal, HeaderField::Unit);
    let text = state.copy_selection(sheet1).expect("sélection non vide");

    state.select_header_field(sheet1, note, HeaderField::Formula);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });
    let status = state.status().expect("un statut");
    assert!(!status.is_error(), "{}", status.text);
    assert!(
        status
            .text
            .contains("1 column(s) and 1 header field(s) did not fit"),
        "statut inattendu : {}",
        status.text
    );
}

/// #259 : une case texte qui porte une tabulation ou un retour à la ligne se
/// colle d'un seul tenant dans une autre session (donc par le seul texte du
/// presse-papiers, comme depuis un tableur) — rien ne glisse sur les cases
/// suivantes. Le texte copié est au format Excel/LibreOffice.
#[test]
fn a_multiline_text_cell_pastes_into_another_session_without_shifting_anything() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut source) = start_demo(dir.path());
    let sheet1 = sheet_named(source.tree(), "Sheet1");
    let signal = column_id(&source, sheet1, "Signal");
    let note = column_id(&source, sheet1, "Note");
    let multiline = "ligne 1\nligne 2\r\nTab\tici \"cité\"".to_string();
    source.dispatch(Action::CommitCell {
        column: note,
        row: 0,
        text: multiline.clone(),
    });
    assert_eq!(cell(&source, note, 0), Cell::Text(Some(multiline.clone())));

    source.select_cell(sheet1, signal, 0);
    source.extend_selection(sheet1, note, 1);
    let text = source.copy_selection(sheet1).expect("sélection non vide");
    assert_eq!(
        text, "10\t\"ligne 1\nligne 2\r\nTab\tici \"\"cité\"\"\"\n\t\n",
        "la case texte est entre guillemets, la case manquante reste vide"
    );

    let dir2 = tempfile::tempdir().expect("répertoire temporaire");
    let (_session2, mut target) = start_demo(dir2.path());
    let sheet1 = sheet_named(target.tree(), "Sheet1");
    let signal = column_id(&target, sheet1, "Signal");
    let note = column_id(&target, sheet1, "Note");
    target.select_cell(sheet1, signal, 2);
    target.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });
    assert_eq!(cell(&target, signal, 2), Cell::Float(10.0));
    assert_eq!(cell(&target, note, 2), Cell::Text(Some(multiline)));
    assert!(matches!(cell(&target, signal, 3), Cell::Float(v) if v.is_nan()));
    assert_eq!(
        cell(&target, note, 3),
        Cell::Text(None),
        "la ligne suivante n'a reçu que ses propres cases"
    );
    assert_eq!(
        target
            .tree()
            .node(note)
            .and_then(|n| n.as_column())
            .map(|c| c.len()),
        Some(4),
        "deux lignes collées (2 et 3), pas une de plus : le retour à la ligne \
         de la case n'a rien coupé"
    );
}

/// #259 : une seule case texte multiligne copiée puis collée ailleurs reste
/// une seule case (et non une rangée de cases).
#[test]
fn a_single_multiline_text_cell_pastes_as_one_cell() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut source) = start_demo(dir.path());
    let sheet1 = sheet_named(source.tree(), "Sheet1");
    let note = column_id(&source, sheet1, "Note");
    source.dispatch(Action::CommitCell {
        column: note,
        row: 0,
        text: "a\nb\tc".to_string(),
    });
    source.select_cell(sheet1, note, 0);
    let text = source.copy_selection(sheet1).expect("une case");

    let dir2 = tempfile::tempdir().expect("répertoire temporaire");
    let (_session2, mut target) = start_demo(dir2.path());
    let sheet1 = sheet_named(target.tree(), "Sheet1");
    let note = column_id(&target, sheet1, "Note");
    target.select_cell(sheet1, note, 1);
    target.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });
    assert_eq!(
        cell(&target, note, 1),
        Cell::Text(Some("a\nb\tc".to_string()))
    );
    assert_eq!(cell(&target, note, 0), Cell::Text(Some("a".to_string())));
}

/// #259 : un espace insécable tapé dans un texte n'est pas pris pour une case
/// manquante au copier — il se copie et se recolle tel quel, alors que la case
/// vraiment manquante reste une case vide.
#[test]
fn a_typed_non_breaking_space_survives_copy_and_paste() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut source) = start_demo(dir.path());
    let sheet1 = sheet_named(source.tree(), "Sheet1");
    let note = column_id(&source, sheet1, "Note");
    let signal = column_id(&source, sheet1, "Signal");
    source.dispatch(Action::CommitCell {
        column: note,
        row: 0,
        text: "5\u{00A0}mm".to_string(),
    });
    source.dispatch(Action::CommitCell {
        column: note,
        row: 1,
        text: "m\u{00A0}s".to_string(),
    });
    assert_eq!(
        cell(&source, note, 1),
        Cell::Text(Some("m\u{00A0}s".to_string())),
        "la saisie d'un espace insécable est gardée par le modèle"
    );

    source.select_cell(sheet1, signal, 0);
    source.extend_selection(sheet1, note, 2);
    let text = source.copy_selection(sheet1).expect("sélection non vide");
    assert_eq!(
        text, "10\t5\u{00A0}mm\n\tm\u{00A0}s\n30\t\n",
        "les espaces insécables tapés sont copiés ; la case manquante reste vide"
    );

    let dir2 = tempfile::tempdir().expect("répertoire temporaire");
    let (_session2, mut target) = start_demo(dir2.path());
    let sheet1 = sheet_named(target.tree(), "Sheet1");
    let note = column_id(&target, sheet1, "Note");
    let signal = column_id(&target, sheet1, "Signal");
    target.select_cell(sheet1, signal, 0);
    target.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });
    assert_eq!(
        cell(&target, note, 0),
        Cell::Text(Some("5\u{00A0}mm".to_string()))
    );
    assert_eq!(
        cell(&target, note, 1),
        Cell::Text(Some("m\u{00A0}s".to_string()))
    );
    assert_eq!(
        cell(&target, note, 2),
        Cell::Text(None),
        "la case vraiment manquante n'est pas devenue un espace insécable"
    );
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
/// comportement, voir `stokesforge_app_state::state`) — le pendant de
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
    // « A » : une lettre de colonne valable, car ce texte atterrit sur le
    // champ Formule de `Note` (les formules se saisissent en lettres, #321).
    set_header_field(&mut state, temps, HeaderField::Comment, "A");

    state.select_header_field(sheet1, temps, HeaderField::LongName);
    state.extend_header_field_selection(sheet1, temps, HeaderField::Comment);
    let text = state.copy_selection(sheet1).expect("sélection non vide");
    assert_eq!(text, "a\nb\nA\n");

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
    assert_eq!(meta.comment, "A");

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
    assert_eq!(formula_letters(&state, note).as_deref(), Some("A"));
}

// -- Formules par identifiant (#321) --

/// Une lettre qui ne désigne aucune colonne de la feuille est refusée : rien
/// n'est écrit, la barre d'état dit laquelle (en saisie comme au collage).
#[test]
fn a_formula_naming_a_missing_column_is_refused_with_a_message() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    set_header_field(&mut state, signal, HeaderField::Formula, "Z*2");
    assert_eq!(formula_letters(&state, signal), None, "rien n'est écrit");
    let status = state.status().expect("un statut");
    assert!(status.is_error(), "{}", status.text);
    assert!(
        status.text.contains("Column Z does not exist"),
        "statut inattendu : {}",
        status.text
    );

    // Au collage, le même refus (et son message à lui) : rien n'est écrit.
    state.select_header_field(sheet1, note, HeaderField::Formula);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "Y*2".to_string(),
    });
    assert_eq!(formula_letters(&state, note), None, "collage refusé");
    let status = state.status().expect("un statut");
    assert!(status.is_error(), "{}", status.text);
    assert!(
        status.text.contains("Column Y does not exist"),
        "statut inattendu : {}",
        status.text
    );
}

/// Une colonne lue qui disparaît laisse `#REF` dans le texte affiché ; quitter
/// la case sans rien changer ne réécrit pas la formule (le texte affiché n'est
/// jamais relu : `#REF` serait refusé), et Annuler rend la colonne et sa
/// lettre.
#[test]
fn a_formula_reading_a_deleted_column_shows_ref_and_survives_an_unchanged_edit() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");
    let stored = state.column_meta(signal).formula;
    assert!(stored.is_some());

    state.select_column(sheet1, temps);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });
    assert_eq!(formula_letters(&state, signal).as_deref(), Some("#REF*2"));

    // Ouvrir la case puis la quitter telle quelle : la formule reste, sans
    // message d'erreur.
    state.begin_header_edit(signal, HeaderField::Formula);
    assert_eq!(
        state.header_editing_mut().map(|edit| edit.buffer.clone()),
        Some("#REF*2".to_string())
    );
    state.commit_header_edit();
    assert_eq!(state.column_meta(signal).formula, stored, "formule intacte");
    assert!(
        !state.status().is_some_and(|status| status.is_error()),
        "aucune erreur : {}",
        status_text(&state)
    );
}

/// Le refus de la case Formule se décide dans `app-state`
/// (`header_edit_refusal`), que la grille peint : la colonne qui se lit
/// elle-même est refusée dès la frappe, et valider un texte refusé n'écrit rien.
#[test]
fn a_formula_reading_its_own_column_is_refused_while_typing() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note"); // la colonne C

    state.begin_header_edit(note, HeaderField::Formula);
    assert_eq!(state.header_edit_refusal(), None, "rien de tapé");
    state.header_editing_mut().expect("saisie").buffer = "C*2".to_string();
    let refusal = state.header_edit_refusal().expect("auto-lecture refusée");
    assert!(
        refusal.contains("own column") && refusal.contains('C'),
        "phrase inattendue : {refusal}"
    );
    state.commit_header_edit();
    assert_eq!(formula_letters(&state, note), None, "rien n'est écrit");

    // La minuscule est normalisée : `a*2` s'affiche `A*2`.
    set_header_field(&mut state, note, HeaderField::Formula, "a*2");
    assert_eq!(formula_letters(&state, note).as_deref(), Some("A*2"));
}

/// `#REF` : valider le texte affiché sans le toucher n'écrit rien et n'est pas
/// refusé ; le modifier (`#REF*3`) est refusé ; vider efface ; une erreur de
/// **syntaxe** n'est jamais un refus (elle s'enregistre). Et la case Formule
/// porte son problème (`formula_issue`) tant qu'elle lit une colonne supprimée.
#[test]
fn a_ref_formula_is_refused_only_when_its_text_is_changed() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");
    assert_eq!(state.formula_issue(signal), None, "aucune colonne perdue");
    state.select_column(sheet1, temps);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });
    state.dispatch(Action::CloseColumnUse);
    let issue = state.formula_issue(signal).expect("une colonne est perdue");
    assert!(issue.contains("Column deleted"), "{issue}");
    let stored = state.column_meta(signal).formula;

    state.begin_header_edit(signal, HeaderField::Formula);
    assert_eq!(state.header_edit_refusal(), None, "texte affiché : valide");
    state.header_editing_mut().expect("saisie").buffer = "#REF*3".to_string();
    let refusal = state.header_edit_refusal().expect("#REF modifié refusé");
    assert!(refusal.contains("Column deleted"), "{refusal}");
    state.commit_header_edit();
    assert_eq!(
        state.column_meta(signal).formula,
        stored,
        "rien n'est écrit"
    );

    state.begin_header_edit(signal, HeaderField::Formula);
    state.header_editing_mut().expect("saisie").buffer = "B*".to_string();
    assert_eq!(
        state.header_edit_refusal(),
        None,
        "une erreur de syntaxe s'enregistre"
    );
    state.header_editing_mut().expect("saisie").buffer = String::new();
    assert_eq!(state.header_edit_refusal(), None, "vider efface");
    state.cancel_header_edit();
}

/// Collage d'un rectangle dont la formule est refusée dans la feuille
/// d'arrivée : les autres champs sont collés, et la barre d'état dit le refus
/// **en dernier** (la même phrase qu'à la saisie).
#[test]
fn a_refused_formula_in_a_paste_leaves_the_other_fields_pasted() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let sheet2 = sheet_named(state.tree(), "Sheet2");
    let signal = column_id(&state, sheet1, "Signal");
    let court = column_id(&state, sheet2, "Court"); // la colonne A de Sheet2

    set_header_field(&mut state, signal, HeaderField::LongName, "ln");
    set_header_field(&mut state, signal, HeaderField::Unit, "u");
    set_header_field(&mut state, signal, HeaderField::Comment, "c");
    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");
    // Deux colonnes copiées (Signal et Note) pour une seule d'arrivée : l'avis
    // de débordement part aussi, et le refus de la formule doit passer après.
    let note = column_id(&state, sheet1, "Note");
    state.select_header_field(sheet1, signal, HeaderField::LongName);
    state.extend_header_field_selection(sheet1, note, HeaderField::Formula);
    let text = state.copy_selection(sheet1).expect("sélection non vide");

    // Dans Sheet2, « A » est la colonne qui reçoit : elle se lirait elle-même.
    state.select_header_field(sheet2, court, HeaderField::LongName);
    state.dispatch(Action::Paste {
        sheet: sheet2,
        text,
    });
    let meta = state.column_meta(court);
    assert_eq!(
        (
            meta.long_name.as_str(),
            meta.unit.as_str(),
            meta.comment.as_str()
        ),
        ("ln", "u", "c"),
        "les autres champs sont collés"
    );
    assert_eq!(formula_letters(&state, court), None, "formule refusée");
    let status = state.status().expect("un statut");
    assert!(status.is_error(), "{}", status.text);
    assert!(
        status.text.contains("own column"),
        "statut inattendu (le refus passe après l'avis de débordement) : {}",
        status.text
    );
}

/// Ctrl+Maj+C (formules) copie des **lettres** : collées dans une autre
/// feuille, elles sont relues dans la feuille d'arrivée (là, `A` est la
/// colonne de cette feuille, pas celle d'où vient la copie).
#[test]
fn copied_formulas_travel_as_letters_and_are_read_in_the_destination_sheet() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let sheet2 = sheet_named(state.tree(), "Sheet2");
    let signal = column_id(&state, sheet1, "Signal");
    let court = column_id(&state, sheet2, "Court");

    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");
    state.select_column(sheet1, signal);
    assert_eq!(
        state.copy_full_columns(sheet1),
        None,
        "la question est posée"
    );
    state.dispatch(Action::ConfirmFullColumnsCopy(
        stokesforge_app_state::state::FullColumnsCopy::Formulas,
    ));
    let text = state.take_object_clipboard_text().expect("texte copié");
    assert!(text.contains("A*2"), "des lettres : {text}");

    state.dispatch(Action::AddColumn { sheet: sheet2 });
    let columns = state.tree().children(sheet2).expect("feuille").to_vec();
    let arrivee = *columns.last().expect("colonne ajoutée");
    assert_ne!(arrivee, court);
    state.select_header_field(sheet2, arrivee, HeaderField::XPrime);
    state.dispatch(Action::Paste {
        sheet: sheet2,
        text,
    });
    assert_eq!(formula_letters(&state, arrivee).as_deref(), Some("A*2"));
    let lue = state
        .column_meta(arrivee)
        .formula
        .expect("formule collée")
        .columns();
    assert_eq!(
        lue,
        std::collections::BTreeSet::from([court]),
        "lit la colonne A de Sheet2"
    );
}

// -- Retirer une formule (#236) --

fn column_len(state: &AppState, column: NodeId) -> usize {
    state
        .tree()
        .node(column)
        .and_then(|node| node.as_column())
        .map_or(0, |c| c.len())
}

/// Vider la case Formule supprime aussi les valeurs de la colonne, et
/// Ctrl+Z rend formule et données d'un seul coup (#236).
#[test]
fn removing_a_formula_deletes_the_values_and_one_undo_restores_both() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let len = column_len(&state, signal);
    assert!(len > 0);

    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");
    let with_formula = column_len(&state, signal);
    let value = cell(&state, signal, 0);

    set_header_field(&mut state, signal, HeaderField::Formula, "");
    assert_eq!(formula_letters(&state, signal), None);
    assert_eq!(
        column_len(&state, signal),
        0,
        "les valeurs partent avec la formule"
    );
    assert!(column_len(&state, temps) > 0, "les autres colonnes restent");

    state.dispatch(Action::Undo);
    assert_eq!(formula_letters(&state, signal).as_deref(), Some("A*2"));
    assert_eq!(column_len(&state, signal), with_formula);
    assert_eq!(cell(&state, signal, 0), value);
}

/// Vider le champ au clavier (Retour arrière) suit le même chemin.
#[test]
fn clearing_the_formula_field_with_backspace_also_deletes_the_values() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");

    state.select_header_field(sheet1, signal, HeaderField::Formula);
    state.dispatch(Action::ClearSelectedCells { sheet: sheet1 });

    assert_eq!(formula_letters(&state, signal), None);
    assert_eq!(column_len(&state, signal), 0);
}

/// « Remove Formula, Keep Data » garde les valeurs ; une seule annulation.
#[test]
fn remove_formula_keep_data_keeps_the_values_in_one_undo_step() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");
    let len = column_len(&state, signal);
    let value = cell(&state, signal, 0);

    state.select_header_field(sheet1, signal, HeaderField::Formula);
    assert!(state.grid_menu(sheet1).unwrap().offers_keep_formula_data);
    state.dispatch(Action::RemoveFormulaKeepData { sheet: sheet1 });

    assert_eq!(formula_letters(&state, signal), None);
    assert_eq!(column_len(&state, signal), len);
    assert_eq!(cell(&state, signal, 0), value);
    assert!(!state.grid_menu(sheet1).unwrap().offers_keep_formula_data);

    state.dispatch(Action::Undo);
    assert_eq!(formula_letters(&state, signal).as_deref(), Some("A*2"));
}

/// Workbook › Columns : la même entrée, sur des colonnes entières.
#[test]
fn remove_columns_formula_keep_data_works_from_the_columns_menu() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");
    let len = column_len(&state, signal);

    assert!(!state.any_formula(&[temps]));
    assert!(state.any_formula(&[temps, signal]));
    state.dispatch(Action::RemoveColumnsFormulaKeepData {
        columns: vec![temps, signal],
    });
    assert_eq!(formula_letters(&state, signal), None);
    assert_eq!(column_len(&state, signal), len);
    state.dispatch(Action::Undo);
    assert_eq!(formula_letters(&state, signal).as_deref(), Some("A*2"));
}

/// L'entrée n'est offerte que sur une formule existante.
#[test]
fn keep_formula_data_is_only_offered_on_a_formula_field() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_header_field(sheet1, signal, HeaderField::Formula);
    assert!(!state.grid_menu(sheet1).unwrap().offers_keep_formula_data);
    state.select_header_field(sheet1, signal, HeaderField::Unit);
    assert!(!state.grid_menu(sheet1).unwrap().offers_keep_formula_data);
}

// -- Matrice de comportement copier/coller (§3, voir la doc de tête de
// `stokesforge_app_state::state`) --
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
/// de tête de `stokesforge_app_state::state`).
#[test]
fn copying_a_single_formula_field_can_paste_it_into_another_columns_formula_field() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");

    // `Temps` est la colonne A : sa formule ne peut pas se lire elle-même,
    // la source de la copie est donc `Signal`.
    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");
    state.select_header_field(sheet1, signal, HeaderField::Formula);
    let text = state.copy_selection(sheet1).expect("sélection non vide");
    assert_eq!(
        text, "A*2",
        "une seule case copiée : son texte, rien d'autre"
    );

    let note = column_id(&state, sheet1, "Note");
    state.select_header_field(sheet1, note, HeaderField::Formula);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    assert_eq!(formula_letters(&state, note).as_deref(), Some("A*2"));
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

/// #164 : la ligne x' s'édite comme les autres champs d'en-tête, accepte un
/// nombre (virgule décimale comprise), s'annule en un pas, et un texte vide
/// l'efface.
#[test]
fn x_prime_is_edited_cleared_and_undone_like_a_header_field() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    assert_eq!(state.column_meta(temps).x_prime, None);

    set_header_field(&mut state, temps, HeaderField::XPrime, " 12.5 ");
    assert_eq!(state.column_meta(temps).x_prime, Some(12.5));

    state.dispatch(Action::Undo);
    assert_eq!(state.column_meta(temps).x_prime, None);
    state.dispatch(Action::Redo);
    assert_eq!(state.column_meta(temps).x_prime, Some(12.5));

    set_header_field(&mut state, temps, HeaderField::XPrime, "");
    assert_eq!(state.column_meta(temps).x_prime, None);
}

/// #164 : un texte qui n'est pas un nombre fini est refusé avec un message,
/// sans rien écrire ni empiler d'historique.
#[test]
fn x_prime_refuses_text_nan_and_infinity() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    set_header_field(&mut state, temps, HeaderField::XPrime, "3");
    for invalide in ["abc", "NaN", "inf", "-Infinity", "1e400"] {
        set_header_field(&mut state, temps, HeaderField::XPrime, invalide);
        assert_eq!(
            state.column_meta(temps).x_prime,
            Some(3.0),
            "{invalide:?} ne remplace pas la valeur"
        );
        let status = state.status().expect("message d'état");
        assert!(status.is_error(), "{invalide:?} signalé comme erreur");
    }
    // Les refus n'ont rien empilé : un seul Undo retire le « 3 ».
    state.dispatch(Action::Undo);
    assert_eq!(state.column_meta(temps).x_prime, None);
}

/// #164 : x' se vide avec la sélection (Suppr) et se colle d'un champ à
/// l'autre ; coller du texte sur x' est refusé.
#[test]
fn x_prime_is_cleared_and_pasted_as_a_header_field() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    set_header_field(&mut state, temps, HeaderField::XPrime, "7");
    state.select_header_field(sheet1, temps, HeaderField::XPrime);
    state.dispatch(Action::ClearSelectedCells { sheet: sheet1 });
    assert_eq!(state.column_meta(temps).x_prime, None);

    state.select_header_field(sheet1, temps, HeaderField::XPrime);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "42".to_string(),
    });
    assert_eq!(state.column_meta(temps).x_prime, Some(42.0));
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "pas un nombre".to_string(),
    });
    assert_eq!(state.column_meta(temps).x_prime, Some(42.0));
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
    // 5 lignes d'en-tête (x', Nom long, Unité, Commentaire, Formule) puis tout
    // le corps de `Temps`, indépendamment de la cellule sélectionnée.
    assert_eq!(text, "\n\ns\n\n\n0\n0.5\n1\n1.5\n2\n");
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
    // « A » : atterrit sur le champ Formule de `Signal`, où seule une lettre
    // de colonne de la feuille est valable (#321).
    set_header_field(&mut state, temps, HeaderField::Unit, "A");
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
    assert_eq!(formula_letters(&state, signal).as_deref(), Some("A"));
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
            && formula_letters(&state, signal).as_deref() != Some("c"),
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
        !status.is_error(),
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
    // sur la même sélection — comme le ferait l'appelant (`app/grid.rs`) avant
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
// `stokesforge-script` et `stokesforge-python` comme dépendances optionnelles activées
// par la seule feature `python` de ce crate ; sans elle, ni l'un ni l'autre
// n'est compilé dans le binaire — il n'existe donc littéralement aucun code
// capable de démarrer CPython, quelle que soit la façon dont `AppState` est
// construite ou utilisée. `cargo tree -p stokesforge-app-state` (sans `--features
// python`) le confirme : ni `stokesforge-python` ni `stokesforge-script` n'y
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
        status.is_error(),
        "impossible d'exécuter un script n'est pas un succès silencieux"
    );
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("error-script-unavailable"),
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
    assert!(status.is_error());
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("error-script-unavailable")
    );
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
/// se teste sans fenêtre — le reste, c'est `stokesforge-render-egui` qui le
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
    let mut panel = stokesforge_plot::PanelSpec::at(0, 0);
    panel
        .series
        .push(stokesforge_plot::SeriesSpec::new(colonne));
    let spec = stokesforge_plot::PlotSpec::empty("Essai").with_panels(panel);
    project
        .add_graph(root, Some("Graph1"), spec)
        .expect("graphe");

    let (session, state) = start(project, &dir.path().join("projet.stf"));
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
/// utilisé par la disposition initiale de `stokesforge_app::app::StokesForgeApp::new`.
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
    use stokesforge_app_state::state::{ProjectTreeNode, WindowKind};

    let dir = tempfile::tempdir().expect("dossier temporaire");
    let mut project = demo_project();
    let root = project.root();
    let folder = project.add_folder(root, "Manips").expect("dossier Manips");
    project
        .add_note(folder, Some("Note1"), "un texte")
        .expect("note");

    let (_session, state) = start(project, &dir.path().join("projet.stf"));
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
    let (_session, mut state) = start(project, &dir.path().join("projet.stf"));

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

    let (_session, mut state) = start(project, &dir.path().join("projet.stf"));
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

    let (_session, mut state) = start(project, &dir.path().join("projet.stf"));
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

    let mut recipe = stokesforge_import::ImportRecipe::new("Sans titre");
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
        !status.is_error(),
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
    assert_eq!(state.column_meta(columns_b[0].0).role, ColumnRole::X);
    assert_eq!(state.column_meta(columns_b[1].0).role, ColumnRole::Y);
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

    // Le rétablissement restaure aussi les rôles, pour chaque fichier du lot
    // (issue #3), avec les mêmes identifiants de colonnes et leurs données.
    state.dispatch(Action::Redo);
    assert_eq!(state.worksheets_of(book).len(), sheets_before + 2);
    for (sheet, columns, first_x) in [(sheet_a, columns_a, 1.0), (sheet_b, columns_b, 3.0)] {
        assert_eq!(state.visible_columns(sheet), columns);
        assert_eq!(state.column_meta(columns[0].0).role, ColumnRole::X);
        assert_eq!(state.column_meta(columns[1].0).role, ColumnRole::Y);
        assert_eq!(cell_f64(&state, columns[0].0, 0), first_x);
    }
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
    assert!(state.status().unwrap().is_error());
}

#[test]
fn import_without_an_active_book_defaults_to_one_new_book_for_the_batch() {
    let _recipes = isolate_recipe_store();
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

    let recipe = stokesforge_import::ImportRecipe::new("Sans titre");
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

/// S-T1 (#224) : pendant une frappe, les aperçus de l'import gardent la recette
/// d'avant jusqu'à 0,3 s sans changement ; hors frappe, un changement est
/// immédiat.
#[test]
fn the_import_preview_waits_for_a_typing_pause_but_not_for_other_controls() {
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    state.open_import(book);
    let import = state.import_state_mut().expect("fenêtre d'import ouverte");
    let before = import.recipe.comment_prefix.clone();
    assert_eq!(import.debounce_preview(true, 10.0), None, "rien n'a changé");

    // Une frappe : l'aperçu reste sur l'ancienne recette.
    import.recipe.comment_prefix = "#".to_string();
    let wait = import
        .debounce_preview(true, 10.0)
        .expect("une pause court");
    assert!((wait - 0.3).abs() < 1e-9);
    assert_eq!(import.preview_recipe().comment_prefix, before);

    // Une autre frappe repousse l'échéance.
    import.recipe.comment_prefix = "##".to_string();
    assert!(import.debounce_preview(true, 10.2).is_some());
    assert_eq!(import.preview_recipe().comment_prefix, before);
    assert!(
        import.debounce_preview(true, 10.4).is_some(),
        "0,2 s seulement"
    );

    // La pause écoulée, l'aperçu suit la recette.
    assert_eq!(import.debounce_preview(true, 10.55), None);
    assert_eq!(import.preview_recipe().comment_prefix, "##");

    // Hors frappe (liste, case) : immédiat.
    import.recipe.comment_prefix = ";".to_string();
    assert_eq!(import.debounce_preview(false, 11.0), None);
    assert_eq!(import.preview_recipe().comment_prefix, ";");
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
    assert_eq!(
        import.recipe.name,
        stokesforge_app_state::tr!("import-template-untitled")
    );
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

    // S-M7 (#228) : l'erreur d'un geste de l'assistant va dans son pied.
    assert!(
        state.dialog_error().is_some(),
        "un nom blanc doit être refusé"
    );
    // Les modèles fournis d'office sont listés, mais rien de l'utilisateur
    // n'a été ajouté ni écrit.
    assert!(
        stokesforge_import::RecipeStore::load().recipes().is_empty(),
        "rien n'a été ajouté au store"
    );
}

/// Un modèle enregistré survit à la fermeture de la fenêtre : le rouvrir
/// (nouvelle session de fenêtre, mais même disque) le retrouve tel quel —
/// l'aller-retour complet à travers `RecipeStore::save`/`load`, jamais
/// exercé avant que ce test n'existe (les tests de `stokesforge-import` ne
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
        !status.is_error(),
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
        !status.is_error(),
        "la suppression a réussi : {}",
        status.text
    );

    state.cancel_import();
    state.open_import(book);
    // Les modèles de l'utilisateur (hors modèles fournis d'office).
    let user_recipes = stokesforge_import::RecipeStore::load();
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
        status.is_error(),
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
    assert!(
        status.is_error(),
        "un classeur inexistant doit être signalé"
    );
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
    recipe.data_end = stokesforge_import::DataEnd::None;

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
/// (testé côté `stokesforge-import`, voir `preview::tests`).
#[test]
fn subheader_roles_populate_column_meta_long_name_unit_and_comment() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("instrument.csv");
    std::fs::write(
        &file,
        "Temperature,Pressure\ndegC,bar\nmesure lente,mesure rapide\n0.5,12\n1,2\n3,4\n",
    )
    .expect("écrit instrument.csv");

    let mut recipe = ImportRecipe::new("Sans titre");
    recipe.header_layout = stokesforge_import::HeaderLayout {
        main_header_lines: 0,
        subheader_lines: 4,
        auto_detect: false,
    };
    recipe.subheader_roles = stokesforge_import::SubheaderRoles {
        long_name: Some(stokesforge_import::SubheaderLineRange::single(1)),
        unit: Some(stokesforge_import::SubheaderLineRange::single(2)),
        comment: Some(stokesforge_import::SubheaderLineRange::single(3)),
        x_prime: Some(stokesforge_import::SubheaderLineRange::single(4)),
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
    assert_eq!(meta0.x_prime, Some(0.5), "ligne x' (#164)");
    let meta1 = state.column_meta(columns[1].0);
    assert_eq!(meta1.long_name, "Pressure");
    assert_eq!(meta1.unit, "bar");
    assert_eq!(meta1.comment, "mesure rapide");
    assert_eq!(meta1.x_prime, Some(12.0));
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
/// que `Column::set_column_type` côté `stokesforge-core`.
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
/// colonne, voir `Column::new` côté `stokesforge-core`.
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

/// Supprimer une sélection de colonnes entières retire les colonnes de la
/// feuille (et pas seulement leurs cellules) ; Annuler les rend d'un coup,
/// avec leurs valeurs. Régression : aucune voie de l'interface n'appelait
/// `Command::DeleteColumn`, « Delete and Shift Up » ne vidait que les cellules.
#[test]
fn deleting_selected_columns_removes_them_and_undo_restores_them() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let before = state.visible_columns(sheet1).len();

    state.select_column(sheet1, temps);
    state.extend_column_selection(sheet1, signal);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });

    let after: Vec<NodeId> = state
        .visible_columns(sheet1)
        .into_iter()
        .map(|(id, ..)| id)
        .collect();
    assert_eq!(after.len(), before - 2, "les deux colonnes ont disparu");
    assert!(!after.contains(&temps) && !after.contains(&signal));

    state.dispatch(Action::Undo);
    assert_eq!(
        state.visible_columns(sheet1).len(),
        before,
        "un seul Annuler rend les deux colonnes"
    );
    assert_eq!(cell(&state, temps, 1), Cell::Float(0.5));
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

/// Une feuille sans ligne se sélectionne quand même en entier (#159) : la
/// case du coin et Ctrl+A prennent toutes les colonnes visibles, en-tête
/// compris, ce qui laisse par exemple changer leur type ou leur rôle.
#[test]
fn select_all_on_a_sheet_without_rows_selects_every_column() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    state.dispatch(Action::DeleteRows {
        sheet: sheet1,
        at: 0,
        count: state.row_count(sheet1),
    });
    assert_eq!(state.row_count(sheet1), 0);

    state.select_cell(sheet1, temps, 0);
    state.dispatch(Action::SelectAll { sheet: sheet1 });

    assert!(state.is_all_selected(sheet1));
    assert!(state.is_column_selected(sheet1, temps));
    assert!(state.is_column_selected(sheet1, signal));
}

/// Une sélection partielle n'est pas « tout sélectionné ».
#[test]
fn a_partial_column_selection_is_not_all_selected() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_column(sheet1, temps);

    assert!(!state.is_all_selected(sheet1));
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
/// côté en-tête : Tab valide sur un champ d'en-tête de la dernière colonne
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
/// `app::grid::cell_widget` pour l'aligner à droite, comme dans tout tableur.
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
    assert!(
        spec.title.is_empty(),
        "#103 : tracer des colonnes n'ajoute pas de titre"
    );
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

/// #167 : un Y2 s'apparie au X2 le plus proche à sa gauche et part sur un
/// **second calque** superposé (axe X en haut, axe Y à droite) ; le Y reste
/// sur le premier. Ajouter ensuite des colonnes au graphe réutilise ce calque.
#[test]
fn plot_columns_sends_y2_to_a_second_layer_paired_with_x2() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let mut project = Project::new("X2Y2").expect("projet");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("lecture")
        .expect("feuille");
    let mut add = |name: &str| {
        project
            .add_column(sheet, None, Some(name), Column::from_f64(&[1.0, 2.0, 3.0]))
            .expect("colonne")
    };
    let (x, y, x2, y2) = (add("X"), add("Y"), add("X2"), add("Y2"));
    let (_session, mut state) = start(project, &dir.path().join("projet.stf"));
    for (column, role) in [
        (x, ColumnRole::X),
        (y, ColumnRole::Y),
        (x2, ColumnRole::X2),
        (y2, ColumnRole::Y2),
    ] {
        state.dispatch(Action::SetColumnRole { column, role });
    }

    state.dispatch(Action::PlotColumns {
        sheet,
        columns: vec![x, y, x2, y2],
        plot_type: PlotType::Line,
    });

    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("graphe créé");
    let spec = state.plot_spec(graph).expect("spec");
    assert_eq!(spec.panels.len(), 2, "un calque de plus pour Y2");
    assert_eq!(spec.panels[0].overlay, None);
    assert_eq!(
        (spec.panels[0].series[0].x, spec.panels[0].series[0].y),
        (Some(x), y)
    );
    let second = &spec.panels[1];
    assert!(second.overlay.is_some_and(|o| o.x_top && o.y_right));
    assert_eq!((second.row, second.column), (0, 0), "même case");
    assert_eq!((second.series[0].x, second.series[0].y), (Some(x2), y2));

    // Un second dépôt de Y2 reprend le même calque, sans en créer un autre.
    state.dispatch(Action::AddSeriesToGraph {
        graph,
        panel: 0,
        columns: vec![y2],
        plot_type: None,
    });
    let after = state.plot_spec(graph).expect("spec");
    assert_eq!(after.panels.len(), 2);
    assert_eq!(after.panels[1].series.len(), 2);
    assert_eq!(after.panels[0].series.len(), 1);

    state.dispatch(Action::Undo);
    assert_eq!(
        state.plot_spec(graph).expect("spec").panels[1].series.len(),
        1
    );
}

/// #168 : Y3 et Y4 s'apparient à X3 et X4, chacun sur son calque numéroté ;
/// un Y3 déposé plus tard retrouve le sien ; un Y3 sans Y2 ne crée pas de
/// calque 2 vide.
#[test]
fn plot_columns_sends_y3_and_y4_to_their_own_numbered_layers() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let mut project = Project::new("X3Y4").expect("projet");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("lecture")
        .expect("feuille");
    let mut add = |name: &str| {
        project
            .add_column(sheet, None, Some(name), Column::from_f64(&[1.0, 2.0, 3.0]))
            .expect("colonne")
    };
    let c: Vec<NodeId> = ["X", "Y", "X3", "Y3", "X4", "Y4"].map(&mut add).to_vec();
    let (_session, mut state) = start(project, &dir.path().join("projet.stf"));
    for (column, role) in c.iter().copied().zip([
        ColumnRole::X,
        ColumnRole::Y,
        ColumnRole::X3,
        ColumnRole::Y3,
        ColumnRole::X4,
        ColumnRole::Y4,
    ]) {
        state.dispatch(Action::SetColumnRole { column, role });
    }
    state.dispatch(Action::PlotColumns {
        sheet,
        columns: c.clone(),
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("graphe créé");
    let spec = state.plot_spec(graph).expect("spec");
    let calques: Vec<(u8, NodeId, Option<NodeId>)> = spec
        .panels
        .iter()
        .skip(1)
        .map(|p| {
            (
                p.overlay.expect("superposé").layer,
                p.series[0].y,
                p.series[0].x,
            )
        })
        .collect();
    assert_eq!(
        calques,
        [(3, c[3], Some(c[2])), (4, c[5], Some(c[4]))],
        "pas de calque 2 vide, X3/X4 et non X"
    );

    // Un Y3 redéposé rejoint le calque 3, sans en créer un autre.
    state.dispatch(Action::AddSeriesToGraph {
        graph,
        panel: 0,
        columns: vec![c[3]],
        plot_type: None,
    });
    let after = state.plot_spec(graph).expect("spec");
    assert_eq!(after.panels.len(), 3);
    assert_eq!(after.panels[1].series.len(), 2);
    assert_eq!(after.panels[2].series.len(), 1);
}

/// #168 : « New Layer » refuse un quatrième calque superposé ; un calque à
/// deux axes prend le premier numéro de rôle libre.
#[test]
fn add_layer_numbers_two_axes_layers_and_stops_at_three_overlays() {
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
        .expect("graphe créé");
    for _ in 0..4 {
        state.dispatch(Action::AddLayer {
            graph,
            panel: 0,
            x_top: true,
            y_right: true,
        });
    }
    let spec = state.plot_spec(graph).expect("spec");
    let numeros: Vec<u8> = spec
        .panels
        .iter()
        .skip(1)
        .map(|p| p.overlay.unwrap().layer)
        .collect();
    assert_eq!(numeros, [2, 3, 4], "le quatrième est refusé");
}

/// #167 : `AddLayer` pose un calque superposé sur le calque de base du calque
/// visé, en un pas d'annulation ; un calque inexistant ne change rien.
#[test]
fn add_layer_overlays_the_base_of_the_target_layer() {
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
        .expect("graphe créé");

    state.dispatch(Action::AddLayer {
        graph,
        panel: 0,
        x_top: true,
        y_right: false,
    });
    // Depuis le calque superposé, le nouveau recouvre le même calque de base.
    state.dispatch(Action::AddLayer {
        graph,
        panel: 1,
        x_top: false,
        y_right: true,
    });
    let spec = state.plot_spec(graph).expect("spec");
    assert_eq!(spec.panels.len(), 3);
    assert_eq!(
        spec.panels[1].overlay.map(|o| (o.x_top, o.y_right)),
        Some((true, false))
    );
    assert_eq!(
        spec.panels[2].overlay.map(|o| (o.x_top, o.y_right)),
        Some((false, true))
    );
    assert!(spec.panels.iter().all(|p| (p.row, p.column) == (0, 0)));

    state.dispatch(Action::AddLayer {
        graph,
        panel: 9,
        x_top: true,
        y_right: true,
    });
    assert_eq!(state.plot_spec(graph).expect("spec").panels.len(), 3);

    state.dispatch(Action::Undo);
    assert_eq!(state.plot_spec(graph).expect("spec").panels.len(), 2);
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
    assert!(state.status().is_some_and(|status| status.is_error()));
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
        panel: 0,
        columns: vec![note],
        plot_type: Some(PlotType::Scatter),
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

/// #177 : des séries ajoutées **en même temps** forment un groupe ; une série
/// ajoutée seule n'en a pas. Un nouvel ajout multiple ouvre un autre groupe
/// sans toucher au premier.
#[test]
fn des_series_ajoutees_ensemble_forment_un_groupe_une_seule_non() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");
    let root = state.tree().root();
    let last_graph = |state: &AppState| {
        state
            .tree()
            .node(root)
            .and_then(|node| node.children().last().copied())
            .expect("graphe")
    };

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let seul = last_graph(&state);
    assert_eq!(
        state.plot_spec(seul).unwrap().panels[0].series[0].group,
        None
    );

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![temps, signal, note],
        plot_type: PlotType::Line,
    });
    let graph = last_graph(&state);
    let panel = state.plot_spec(graph).unwrap().panels[0].clone();
    assert!(panel.series.len() >= 2);
    assert!(panel.series.iter().all(|s| s.group == Some(1)), "{panel:?}");

    // Une seule colonne ajoutée : pas de groupe ; deux ensemble : un second.
    state.dispatch(Action::AddSeriesToGraph {
        graph: seul,
        panel: 0,
        columns: vec![note],
        plot_type: None,
    });
    let seul_apres = state.plot_spec(seul).unwrap();
    assert!(seul_apres.panels[0]
        .series
        .iter()
        .all(|s| s.group.is_none()));
    state.dispatch(Action::AddSeriesToGraph {
        graph,
        panel: 0,
        columns: vec![signal, note],
        plot_type: None,
    });
    let apres = state.plot_spec(graph).unwrap().panels[0].clone();
    let n = panel.series.len();
    assert!(apres.series[..n].iter().all(|s| s.group == Some(1)));
    assert!(apres.series[n..].iter().all(|s| s.group == Some(2)));
    assert_eq!(apres.series.len(), n + 2);
}

// -- 22 ter. Glisser des lignes vers un graphe (#166) --

/// Feuille « cinétique » : trois colonnes mesurées à t = 0, 10 et 20 s (x'),
/// plus une colonne sans x'. Rend (état, feuille, colonnes avec x').
fn kinetics(dir: &Path) -> (Session, AppState, NodeId, Vec<NodeId>) {
    let mut project = Project::new("Cinétique").expect("projet");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("lecture")
        .expect("feuille");
    let mut add = |name: &str, values: &[f64]| {
        project
            .add_column(sheet, None, Some(name), Column::from_f64(values))
            .expect("colonne")
    };
    let a = add("A", &[1.0, 2.0, 3.0]);
    let b = add("B", &[4.0, 5.0, 6.0]);
    let c = add("C", &[7.0, 8.0, 9.0]);
    let _sans_x = add("D", &[0.0, 0.0, 0.0]);
    let (session, mut state) = start(project, &dir.join("projet.stf"));
    for (column, time) in [(a, "0"), (b, "10"), (c, "20")] {
        set_header_field(&mut state, column, HeaderField::XPrime, time);
    }
    (session, state, sheet, vec![a, b, c])
}

/// Écrit la valeur x' de la colonne nommée `name` de `sheet`.
fn set_x_prime(state: &mut AppState, sheet: NodeId, name: &str, text: &str) {
    let column = column_id(state, sheet, name);
    set_header_field(state, column, HeaderField::XPrime, text);
}

/// Un graphe créé depuis une colonne Y apparié à une colonne X (donc « avec X »).
fn graph_with_x(state: &mut AppState, sheet: NodeId) -> NodeId {
    let root = state.tree().root();
    let x = column_id(state, sheet, "A");
    let y = column_id(state, sheet, "B");
    state.dispatch(Action::SetColumnRole {
        column: x,
        role: ColumnRole::X,
    });
    state.dispatch(Action::PlotColumns {
        sheet,
        columns: vec![y],
        plot_type: PlotType::Line,
    });
    state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("graphe créé")
}

/// Le geste n'est offert que pour des **lignes entières** ET une feuille qui a
/// une ligne x' : ni sans sélection, ni pour des colonnes, ni sans x'.
#[test]
fn row_drag_payload_needs_full_rows_and_an_x_prime_row() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state, sheet, _) = kinetics(dir.path());
    assert_eq!(state.row_drag_payload(sheet), None, "aucune sélection");

    state.select_column(sheet, column_id(&state, sheet, "A"));
    assert_eq!(
        state.row_drag_payload(sheet),
        None,
        "des colonnes, pas des lignes"
    );

    state.select_row(sheet, 1);
    state.extend_row_selection(sheet, 2);
    let payload = state.row_drag_payload(sheet).expect("lignes + x'");
    assert_eq!((payload.sheet, payload.rows.clone()), (sheet, vec![1, 2]));

    // Sans aucune valeur x', plus de geste.
    for column in ["A", "B", "C"] {
        set_x_prime(&mut state, sheet, column, "");
    }
    state.select_row(sheet, 1);
    assert_eq!(state.row_drag_payload(sheet), None, "sans x'");

    // Une sélection de cellules (pas de lignes entières) ne se glisse pas non plus.
    set_x_prime(&mut state, sheet, "A", "1");
    state.select_cell(sheet, column_id(&state, sheet, "A"), 1);
    assert_eq!(state.row_drag_payload(sheet), None);
}

/// Graphe sans X déterminé : pas de dialogue, l'axe X principal ; une ligne =
/// une série, dont les points sont les valeurs de la ligne placées à x'. Un
/// seul pas d'annulation.
#[test]
fn dropping_rows_on_a_graph_without_x_uses_the_main_axis_with_no_dialog() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state, sheet, columns) = kinetics(dir.path());
    state.dispatch(Action::AddGraph);
    let root = state.tree().root();
    let graph = *state.tree().node(root).unwrap().children().last().unwrap();
    state.select_row(sheet, 0);
    state.extend_row_selection(sheet, 1);
    let payload = state.row_drag_payload(sheet).expect("charge");

    state.dispatch(drop_rows_action(&payload, graph, 0));

    assert_eq!(state.pending_row_drop(), None, "pas de question sans X");
    let spec = state.plot_spec(graph).expect("spec");
    assert_eq!(
        spec.panels.len(),
        1,
        "axe principal : pas de nouveau calque"
    );
    let series = &spec.panels[0].series;
    assert_eq!(series.len(), 2, "une série par ligne sélectionnée");
    assert_eq!(
        series.iter().map(|s| s.row).collect::<Vec<_>>(),
        [Some(0), Some(1)]
    );
    assert_eq!(series[0].y, columns[0], "ancrée sur la première colonne x'");

    let resolved = stokesforge_core::resolve_spec(state.tree(), &spec);
    let first = &resolved.panels[0].series[0];
    // Ligne 0 : A=1, B=4, C=7 aux instants 0, 10, 20 — la colonne D, sans x', n'en est pas.
    assert_eq!(first.x.as_ref().unwrap().as_slice(), [0.0, 10.0, 20.0]);
    assert_eq!(first.y.as_ref().unwrap().as_slice(), [1.0, 4.0, 7.0]);
    let second = &resolved.panels[0].series[1];
    assert_eq!(second.y.as_ref().unwrap().as_slice(), [2.0, 5.0, 8.0]);

    state.dispatch(Action::Undo);
    assert!(state.plot_spec(graph).unwrap().panels[0].series.is_empty());
}

/// Graphe qui a déjà un X : le dépôt ne change rien avant la réponse ; « Créer
/// un axe » (défaut) ajoute un calque à axe X propre, **dans le même pas
/// d'annulation** ; « Axe présent » reste dans le calque visé ; « Annuler »
/// abandonne.
#[test]
fn dropping_rows_on_a_graph_with_x_asks_which_axis() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state, sheet, _) = kinetics(dir.path());
    let graph = graph_with_x(&mut state, sheet);
    let before = state.plot_spec(graph).expect("spec");
    state.select_row(sheet, 2);
    let payload = state.row_drag_payload(sheet).expect("charge");

    state.dispatch(drop_rows_action(&payload, graph, 0));
    assert_eq!(state.pending_row_drop(), Some((graph, 1)), "question posée");
    assert_eq!(state.plot_spec(graph).unwrap(), before, "rien n'a changé");

    // Annuler : plus de question, rien de changé.
    state.dispatch(Action::ConfirmRowDrop(None));
    assert_eq!(state.pending_row_drop(), None);
    assert_eq!(state.plot_spec(graph).unwrap(), before);

    // Nouvel axe : un calque superposé, axe X en haut, un seul Undo.
    state.dispatch(drop_rows_action(&payload, graph, 0));
    state.dispatch(Action::ConfirmRowDrop(Some(RowAxis::New)));
    assert_eq!(state.pending_row_drop(), None);
    let spec = state.plot_spec(graph).unwrap();
    assert_eq!(spec.panels.len(), 2, "un calque de plus");
    assert_eq!(
        spec.panels[0], before.panels[0],
        "le calque d'origine est intact"
    );
    assert!(spec.panels[1]
        .overlay
        .is_some_and(|o| o.x_top && !o.y_right));
    assert_eq!(spec.panels[1].series.len(), 1);
    assert_eq!(spec.panels[1].series[0].row, Some(2));
    state.dispatch(Action::Undo);
    assert_eq!(
        state.plot_spec(graph).unwrap(),
        before,
        "un seul pas, calque compris"
    );

    // Axe présent : les séries rejoignent le calque visé, sans nouveau calque.
    state.dispatch(drop_rows_action(&payload, graph, 0));
    state.dispatch(Action::ConfirmRowDrop(Some(RowAxis::Present)));
    let spec = state.plot_spec(graph).unwrap();
    assert_eq!(spec.panels.len(), 1);
    assert_eq!(spec.panels[0].series.len(), 2);
    assert_eq!(spec.panels[0].series[1].row, Some(2));
}

/// Une ligne hors de la feuille est ignorée ; une feuille sans x' refuse avec
/// un message, sans rien écrire.
#[test]
fn dropping_rows_ignores_missing_rows_and_refuses_a_sheet_without_x_prime() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state, sheet, _) = kinetics(dir.path());
    state.dispatch(Action::AddGraph);
    let root = state.tree().root();
    let graph = *state.tree().node(root).unwrap().children().last().unwrap();

    state.dispatch(Action::AddRowsToGraph {
        graph,
        panel: 0,
        sheet,
        rows: vec![1, 99],
        axis: RowAxis::Ask,
    });
    let series = &state.plot_spec(graph).unwrap().panels[0].series;
    assert_eq!(series.len(), 1);
    assert_eq!(series[0].row, Some(1));

    for column in ["A", "B", "C"] {
        set_x_prime(&mut state, sheet, column, "");
    }
    let before = state.plot_spec(graph).unwrap();
    state.dispatch(Action::AddRowsToGraph {
        graph,
        panel: 0,
        sheet,
        rows: vec![0],
        axis: RowAxis::Ask,
    });
    assert_eq!(state.plot_spec(graph).unwrap(), before);
    assert!(state.status().expect("message").is_error());
}

/// Plot d'une sélection de lignes (#166) : un graphe neuf dont chaque ligne est
/// une série, pas toutes les colonnes traversées ; un seul pas d'annulation.
#[test]
fn plot_rows_creates_a_graph_with_one_row_series_per_row() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state, sheet, columns) = kinetics(dir.path());
    let root = state.tree().root();
    let before = state.tree().len();

    state.dispatch(Action::PlotRows {
        sheet,
        rows: vec![2, 0, 99],
        plot_type: PlotType::Line,
    });

    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("graphe créé");
    let spec = state.plot_spec(graph).expect("spec");
    let series = &spec.panels[0].series;
    assert_eq!(
        series.iter().map(|s| s.row).collect::<Vec<_>>(),
        [Some(0), Some(2)]
    );
    assert_eq!(series[0].y, columns[0]);
    assert!(series.iter().all(|s| s.x.is_none()));
    state.dispatch(Action::Undo);
    assert_eq!(state.tree().len(), before);

    // Sans x', rien n'est créé.
    for name in ["A", "B", "C"] {
        set_x_prime(&mut state, sheet, name, "");
    }
    state.dispatch(Action::PlotRows {
        sheet,
        rows: vec![0],
        plot_type: PlotType::Line,
    });
    assert_eq!(state.tree().len(), before);
    assert!(state.status().expect("message").is_error());
}

// -- 23. Glisser une sélection de colonnes vers un graphe --

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

/// Seule une sélection de **colonnes entières** se glisse (#160) : des
/// cellules ou des lignes, même couvrant des colonnes, n'offrent pas le
/// geste.
#[test]
fn column_drag_payload_needs_whole_columns() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, temps, 0);
    state.extend_selection(sheet1, signal, 1);
    assert_eq!(state.column_drag_payload(sheet1), None, "des cellules");

    state.select_row(sheet1, 1);
    assert_eq!(state.column_drag_payload(sheet1), None, "une ligne");

    state.select_header_row(sheet1, HeaderField::Unit);
    assert_eq!(
        state.column_drag_payload(sheet1),
        None,
        "un champ d'en-tête"
    );
}

/// Avec une sélection de colonnes entières, la charge utile porte la feuille
/// et **toutes** les colonnes sélectionnées dans l'ordre de la feuille —
/// aussi non contiguës —, que `stokesforge_app::app::grid::grid_contents` glisse depuis
/// le bord du bloc et que [`drop_payload_action`] relit au dépôt.
#[test]
fn column_drag_payload_carries_the_sheet_and_its_selected_columns() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_column(sheet1, signal);
    state.extend_column_selection(sheet1, temps);
    let payload = state
        .column_drag_payload(sheet1)
        .expect("sélection couvrant Temps et Signal");
    assert_eq!(payload.sheet, sheet1);
    assert_eq!(payload.columns, vec![temps, signal]);

    state.select_column(sheet1, note);
    state.toggle_column_selection(sheet1, temps);
    assert_eq!(
        state.column_drag_payload(sheet1).unwrap().columns,
        vec![temps, note],
        "non contiguës, dans l'ordre de la feuille"
    );

    state.select_all(sheet1);
    assert_eq!(
        state.column_drag_payload(sheet1).unwrap().columns,
        vec![temps, signal, note],
        "la case du coin les emporte toutes"
    );
}

/// [`drop_payload_action`] vise le calque donné et **n'impose pas** de type
/// de tracé (`None`) : les séries prennent celui du calque (#160).
#[test]
fn drop_payload_action_targets_the_panel_without_forcing_a_plot_type() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let graph = NodeId::from_raw(999);
    let payload = ColumnDragPayload {
        sheet: sheet1,
        columns: vec![signal],
    };

    assert_eq!(
        drop_payload_action(&payload, graph, 2),
        Action::AddSeriesToGraph {
            graph,
            panel: 2,
            columns: vec![signal],
            plot_type: None,
        }
    );
}

/// Trace `Signal` en nuage de points, y dépose `Note` : la nouvelle série
/// prend le type du calque (nuage), pas `Line`. Un seul pas d'annulation.
#[test]
fn a_dropped_series_takes_the_plot_type_of_the_target_panel() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");
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
    let before = state.plot_spec(graph).unwrap();

    state.select_column(sheet1, note);
    let payload = state
        .column_drag_payload(sheet1)
        .expect("Note sélectionnée");
    state.dispatch(drop_payload_action(&payload, graph, 0));

    let spec = state.plot_spec(graph).expect("spec après dépôt");
    assert_eq!(spec.panels[0].series.len(), 2);
    assert_eq!(spec.panels[0].series[1].y, note);
    assert_eq!(spec.panels[0].series[1].plot_type, PlotType::Scatter);

    state.dispatch(Action::Undo);
    assert_eq!(state.plot_spec(graph).unwrap(), before, "un seul pas");
}

/// Un calque qui mélange plusieurs types donne celui de sa dernière série ;
/// un calque vide, `Line` ; un calque inexistant n'ajoute rien.
#[test]
fn the_drop_plot_type_is_the_last_series_type_or_line_for_an_empty_panel() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");
    let root = state.tree().root();

    state.dispatch(Action::AddGraph);
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("graphe vide");
    state.dispatch(Action::AddSeriesToGraph {
        graph,
        panel: 0,
        columns: vec![signal],
        plot_type: None,
    });
    assert_eq!(
        state.plot_spec(graph).unwrap().panels[0].series[0].plot_type,
        PlotType::Line,
        "calque vide : Line"
    );

    state.dispatch(Action::AddSeriesToGraph {
        graph,
        panel: 0,
        columns: vec![note],
        plot_type: Some(PlotType::Bar),
    });
    state.dispatch(Action::AddSeriesToGraph {
        graph,
        panel: 0,
        columns: vec![signal],
        plot_type: None,
    });
    let series = &state.plot_spec(graph).unwrap().panels[0].series;
    assert_eq!(series.len(), 3);
    assert_eq!(series[2].plot_type, PlotType::Bar, "dernière série : Bar");

    let before = state.plot_spec(graph).unwrap();
    state.dispatch(Action::AddSeriesToGraph {
        graph,
        panel: 7,
        columns: vec![signal],
        plot_type: None,
    });
    assert_eq!(state.plot_spec(graph).unwrap(), before, "calque inconnu");
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
/// « General », zone de tracé → « Plot Area », axe ou titre d'axe →
/// « Axes » sur cet axe.
#[test]
fn a_right_clicked_component_opens_the_settings_page_that_sets_it() {
    // Compared in English: the test must not depend on `STOKESFORGE_LANG` or on the
    // saved preference of whoever runs it.
    stokesforge_app_state::i18n::set_language(stokesforge_app_state::i18n::Language::English);
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
        "X (bottom) Axis Title"
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
/// d'une version plus récente de StokesForge que ce lecteur ne connaît pas) doit
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
    use stokesforge_app_state::state::SaveTarget;
    use stokesforge_plot::ColorUsage;
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
    use stokesforge_app_state::state::SaveTarget;
    use stokesforge_plot::ColorUsage;
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

/// #178 : un nuancier se range dans le projet (annulable) ou chez l'utilisateur
/// (fichier, hors historique), se remplace par son nom et se supprime.
#[test]
fn les_nuanciers_s_enregistrent_dans_le_projet_ou_chez_l_utilisateur_et_se_suppriment() {
    use stokesforge_app_state::state::SaveTarget;
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let path = dir.path().join("config").join("couleurs.json");
    state.use_user_colors_file(path.clone());
    let stops = vec!["#112233".to_string(), "#aabbcc".to_string()];

    state.dispatch(Action::SaveGradient {
        name: "Mon nuancier".to_string(),
        stops: stops.clone(),
        target: SaveTarget::Project,
    });
    let projet = state.saved_colors(SaveTarget::Project).gradients;
    assert_eq!(projet.len(), 1);
    assert_eq!(projet[0].name, "Mon nuancier");
    assert_eq!(projet[0].stops, stops);
    assert!(state.saved_colors(SaveTarget::User).gradients.is_empty());
    state.dispatch(Action::Undo);
    assert!(state.saved_colors(SaveTarget::Project).gradients.is_empty());

    state.dispatch(Action::SaveGradient {
        name: "Perso".to_string(),
        stops,
        target: SaveTarget::User,
    });
    assert_eq!(state.saved_colors(SaveTarget::User).gradients.len(), 1);
    assert!(!state.can_undo(), "hors historique");
    assert!(std::fs::read_to_string(&path)
        .expect("fichier écrit")
        .contains("Perso"));
    // Un nuancier du même nom (casse indifférente) est remplacé, pas doublé.
    state.dispatch(Action::SaveGradient {
        name: "perso".to_string(),
        stops: vec!["#000000".to_string(), "#ffffff".to_string()],
        target: SaveTarget::User,
    });
    assert_eq!(state.saved_colors(SaveTarget::User).gradients.len(), 1);

    state.dispatch(Action::RemoveSavedGradient {
        name: "PERSO".to_string(),
        target: SaveTarget::User,
    });
    assert!(state.saved_colors(SaveTarget::User).gradients.is_empty());
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
        stokesforge_plot::TRANSPARENT
    );
}

// -- Glisser-déposer de fichiers (issue #8) --

/// Un dépôt OS peut arriver depuis un viewport détaché malgré le dialogue
/// modal de la racine : il ne doit pas entrer dans la macro du graphe.
#[test]
fn dropping_files_during_plot_settings_is_refused_without_touching_the_macro() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state) = start_demo(dir.path());
    let graph = plotted_graph(&mut state);
    state.dispatch(Action::OpenPlotSettings {
        graph,
        tab: PlotSettingsTab::Graph,
        axis: None,
    });
    let before = session.handle().snapshot().unwrap();
    assert!(before.history.in_macro, "le dialogue ouvre sa macro");
    let file = dir.path().join("mesure.csv");
    std::fs::write(&file, "1,10\n2,20\n").unwrap();
    state.dispatch(Action::DropFiles {
        paths: vec![file.clone()],
        target_book: None,
    });
    let after = session.handle().snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.revision, before.revision);
    assert!(after.history.in_macro, "le dialogue conserve sa macro");
    // S-M7 (#228) : le refus s'écrit dans le pied du dialogue ouvert.
    assert!(state.dialog_error().is_some());
    assert!(!state.can_reopen_last_import());
    state.cancel_plot_settings();
    state.dispatch(Action::DropFiles {
        paths: vec![file],
        target_book: None,
    });
    wait_import(&mut state);
    assert!(
        !state.status().unwrap().is_error(),
        "le dépôt passe après fermeture du dialogue"
    );
    assert!(state.can_reopen_last_import());
}

/// Le dernier modèle réellement utilisé prime sur l'ordre du catalogue.
/// Un dépôt crée toujours de nouvelles feuilles, même après un remplacement.
#[test]
fn dropping_files_uses_the_last_recipe_and_always_creates_new_sheets() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let mut recipe = stokesforge_import::ImportRecipe::new("Dernier modèle utilisé");
    recipe.separator = stokesforge_import::Separator::Semicolon;
    recipe.has_column_header_row = true;
    recipe.import_mode = stokesforge_import::ImportMode::ReplaceSheet;
    recipe.files_layout = stokesforge_import::FilesLayout::SideBySide;
    let initial = dir.path().join("initial.csv");
    std::fs::write(&initial, "Temps;Signal\n1;10\n").unwrap();
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![initial],
    });
    assert!(!state.status().unwrap().is_error());
    let before = state.snapshot().tree.clone();
    let sheets_before = state.worksheets_of(book).len();
    let files: Vec<_> = ["depot1.csv", "depot2.csv"]
        .into_iter()
        .map(|name| {
            let path = dir.path().join(name);
            std::fs::write(&path, "Temps;Signal\n2;20\n3;30\n").unwrap();
            path
        })
        .collect();
    state.dispatch(Action::DropFiles {
        paths: files,
        target_book: Some(book),
    });
    wait_import(&mut state);
    assert_eq!(state.worksheets_of(book).len(), sheets_before + 2);
    let sheet = sheet_named(state.tree(), "depot1");
    let columns = state.visible_columns(sheet);
    assert_eq!(state.column_meta(columns[0].0).long_name, "Temps");
    assert_eq!(cell_f64(&state, columns[0].0, 0), 2.);
    assert!(state
        .status()
        .unwrap()
        .text
        .contains("Dernier modèle utilisé"));
    state.dispatch(Action::Undo);
    assert_eq!(*state.snapshot().tree, *before);
}

/// Sans dernier import ni modèle reconnu, on utilise la recette permissive,
/// jamais le premier modèle du catalogue qui pourrait avaler les données.
#[test]
fn dropping_an_unknown_file_without_history_uses_the_default_recipe() {
    let _recipes = isolate_recipe_store();
    let mut store = stokesforge_import::RecipeStore::default();
    let mut unsuitable = stokesforge_import::ImportRecipe::new("Mauvais modèle");
    unsuitable.header = stokesforge_import::HeaderSkip::Lines(1);
    store.upsert(unsuitable);
    store.save().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let path = dir.path().join("inconnu.csv");
    std::fs::write(&path, "1,10\n2,20\n").unwrap();
    state.dispatch(Action::DropFiles {
        paths: vec![path],
        target_book: Some(book),
    });
    wait_import(&mut state);
    let sheet = sheet_named(state.tree(), "inconnu");
    let x = state.visible_columns(sheet)[0].0;
    assert_eq!(cell_f64(&state, x, 0), 1., "première ligne conservée");
    assert_eq!(cell_f64(&state, x, 1), 2.);
}

/// Le lieu du dépôt fixe la destination ; l'extension `.stf` fait d'un fichier
/// un projet à ouvrir, pas une donnée à importer.
#[test]
fn file_drop_plan_picks_destination_from_the_drop_target_and_splits_projects() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let paths = vec![
        std::path::PathBuf::from("a.csv"),
        std::path::PathBuf::from("projet.STF"),
        std::path::PathBuf::from("b.txt"),
    ];

    let on_book = file_drop_plan(&paths, Some(book));
    assert_eq!(on_book.destination, ImportDestination::ActiveBook(book));
    assert_eq!(
        on_book.projects,
        vec![std::path::PathBuf::from("projet.STF")]
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
    wait_import(&mut state);

    assert_eq!(state.worksheets_of(book).len(), sheets_before + 2);
    assert!(state.import_state().is_none(), "aucune fenêtre d'import");
    let status = state.status().expect("message de statut");
    assert!(!status.is_error());
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("status-import-dropped", count = 2, models = "Untitled"),
        "{}",
        status.text
    );
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
    wait_import(&mut state);

    assert_eq!(
        state.tree().node(root).unwrap().children().len(),
        books_before + 1
    );
}

/// Un `.stf` n'est jamais importé : il est mis en file pour ouverture dans
/// une nouvelle fenêtre, sans toucher au projet courant.
#[test]
fn dropping_a_project_file_queues_it_instead_of_importing() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let revision = state.revision();
    let project = dir.path().join("autre.stf");
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
    wait_import(&mut state);

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
    assert_eq!(
        state.file_metadata_summary(columns[0].0),
        stokesforge_app_state::tr!("workbook-metadata-summary", count = 3)
    );

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

/// Enregistrer le modèle fixe la référence du témoin « modifié », y compris
/// après un changement de nom. Une nouvelle retouche le rallume.
#[test]
fn enregistrer_un_modele_d_import_actualise_le_temoin_de_modification() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    {
        let import = state.import_state_mut().unwrap();
        import.recipe.skip_chars = 2;
        import.save_as_name = "Mon appareil".into();
    }
    assert!(state.import_recipe_modified());

    state.save_import_recipe();

    assert!(!state.status().unwrap().is_error());
    assert!(
        !state.import_recipe_modified(),
        "le modèle vient d'être enregistré"
    );
    let saved = state.import_state().unwrap().recipe.clone();
    assert!(RecipeStore::load().recipes().contains(&saved));

    state.import_state_mut().unwrap().recipe.skip_chars = 3;
    assert!(state.import_recipe_modified());
    state.import_state_mut().unwrap().recipe.skip_chars = 2;
    assert!(
        !state.import_recipe_modified(),
        "retour au modèle enregistré"
    );
}

/// Un échec d'écriture ne doit pas faire croire que les réglages sont sauvés.
#[test]
fn un_echec_d_enregistrement_garde_le_modele_d_import_modifie() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    state.import_state_mut().unwrap().recipe.skip_chars = 2;
    state.save_import_recipe();
    assert!(!state.status().unwrap().is_error());
    state.import_state_mut().unwrap().recipe.skip_chars = 3;

    let file = stokesforge_import::config_dir::config_dir()
        .unwrap()
        .join("import_recipes.json");
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir(&file).unwrap();
    state.save_import_recipe();

    assert!(
        state.dialog_error().is_some(),
        "S-M7 : dans le pied de l'assistant"
    );
    assert!(state.import_recipe_modified());
    state.import_state_mut().unwrap().recipe.skip_chars = 2;
    assert!(
        !state.import_recipe_modified(),
        "la référence reste le dernier succès"
    );
}

/// #267 : un modèle importé dont l'enregistrement échoue reste « modifié ».
#[test]
fn un_echec_d_enregistrement_d_un_modele_importe_le_garde_modifie() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    let mut recipe = state.import_state_mut().unwrap().recipe.clone();
    recipe.name = "importé".to_string();
    recipe.skip_chars = 7;
    let json = dir.path().join("modele.json");
    std::fs::write(&json, recipe.to_json()).unwrap();

    let file = stokesforge_import::config_dir::config_dir()
        .unwrap()
        .join("import_recipes.json");
    let _ = std::fs::remove_file(&file);
    std::fs::create_dir_all(&file).unwrap();
    state.import_import_recipe(&json);

    assert!(state.dialog_error().is_some());
    assert!(
        state.import_recipe_modified(),
        "non écrit sur disque : la référence reste l'ancienne"
    );
}

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
    use stokesforge_app_state::state::ImportStep;
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
    use stokesforge_app_state::state::ImportLineAction;
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
        // La coupure automatique n'est plus le défaut : ce test l'éprouve.
        recipe.data_end = stokesforge_import::DataEnd::Auto;
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
        stokesforge_import::DataEnd::None
    );

    // La ligne 2 donne les unités : elle ouvre le sous-en-tête.
    state.import_line_action(ImportLineAction::UnitsLine, 2);
    let recipe = &state.import_state().unwrap().recipe;
    assert_eq!(recipe.header_layout.main_header_lines, 1);
    assert_eq!(recipe.header_layout.subheader_lines, 2);
    assert_eq!(
        recipe.subheader_roles.unit,
        Some(stokesforge_import::SubheaderLineRange::single(1))
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
        stokesforge_import::ColumnRoleChoice::Auto,
        stokesforge_import::ColumnRoleChoice::Y,
        stokesforge_import::ColumnRoleChoice::YError,
        stokesforge_import::ColumnRoleChoice::Ignore,
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
fn assert_undo_state_matches_actor(session: &Session, state: &AppState, label: HistoryLabel) {
    let actual = session.handle().snapshot().unwrap();
    assert_eq!(state.can_undo(), actual.history.can_undo);
    assert!(state.can_undo(), "{label:?} doit être annulable");
    assert_eq!(actual.history.undo_label.as_ref(), Some(&label));
    assert_eq!(
        state.undo_label(),
        Some(undo_entry(&label)),
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
    assert_undo_state_matches_actor(&session, &state, HistoryLabel::Macro(MacroKind::PasteCells));
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
    assert_undo_state_matches_actor(
        &session,
        &state,
        HistoryLabel::Macro(MacroKind::ClearSelection),
    );
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
        detached.status().is_some_and(|status| status.is_error()),
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

// -- Recalcul lourd : indicateur et Stop (issue #239) --

/// Un projet dont le premier recalcul complet dure assez pour dépasser
/// l'attente de la commande : une grosse colonne source et beaucoup de
/// colonnes-formule qui la lisent.
fn heavy_formula_project() -> Project {
    let mut project = Project::new("Lourd").expect("projet");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("lecture")
        .expect("Sheet1");
    let values: Vec<f64> = (0..200_000).map(f64::from).collect();
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&values))
        .expect("A");
    let colonne_a = project.tree().children(sheet).expect("feuille")[0];
    for index in 0..150 {
        let column = project
            .add_column(
                sheet,
                None,
                Some(&format!("F{index}")),
                Column::new(ColumnType::Numeric),
            )
            .expect("colonne");
        // Formule par identifiant (#321) : elle lit la colonne A.
        let formula = ColumnFormula::from_parts(vec![
            FormulaPart::Column(colonne_a),
            FormulaPart::Text("*2+".into()),
            FormulaPart::Column(colonne_a),
            FormulaPart::Text("*3".into()),
        ]);
        project
            .set_column_meta(
                column,
                ColumnMeta {
                    formula: Some(formula),
                    ..Default::default()
                },
            )
            .expect("formule");
    }
    project
}

/// Une commande qui lance un recalcul long rend la main : l'indicateur se lit,
/// et Stop défait la commande comme Ctrl+Z (rétablissable), avec son message.
/// Sans Stop (témoin), la même commande reste appliquée.
#[test]
fn stop_during_a_heavy_recalculation_undoes_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(heavy_formula_project(), &dir.path().join("lourd.stf"));
    let folders = state.tree().len();
    state.dispatch(Action::AddGraph);
    let elapsed = state
        .recalc_work()
        .expect("le recalcul complet dépasse l'attente de la commande");
    assert!(elapsed >= std::time::Duration::from_millis(200));
    assert_eq!(state.tree().len(), folders, "instantané non rafraîchi");

    state.dispatch(Action::StopRecalc);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while state.recalc_work().is_some() {
        assert!(std::time::Instant::now() < deadline, "recalcul trop long");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    state.poll_recalc();
    assert_eq!(state.tree().len(), folders, "le graphique a disparu");
    assert!(state.snapshot().history.can_redo, "comme après Ctrl+Z");
    assert!(!state.snapshot().history.can_undo);
    let status = state.status().expect("message");
    assert!(!status.is_error());
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("status-recalc-stopped")
    );
}

#[test]
fn without_stop_the_heavy_recalculation_keeps_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(heavy_formula_project(), &dir.path().join("lourd.stf"));
    let folders = state.tree().len();
    state.dispatch(Action::AddGraph);
    assert!(state.recalc_work().is_some());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while state.recalc_work().is_some() {
        assert!(std::time::Instant::now() < deadline, "recalcul trop long");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    state.poll_recalc();
    assert_eq!(state.tree().len(), folders + 1, "le graphique reste");
    assert!(state.snapshot().history.can_undo);
}

/// Les messages de la barre d'état, du plus ancien au plus récent. Le projet
/// lourd pèse environ 240 Mo une fois écrit : si le dossier temporaire est plein
/// (`Disk quota exceeded`), l'écriture échoue et seul ce message dit pourquoi
/// (#342). À joindre aux assertions qui dépendent d'une écriture.
fn status_messages(state: &AppState) -> String {
    state
        .status_history()
        .map(|m| m.text.clone())
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Attend la fin du recalcul en cours, puis laisse l'état le relever.
fn finish_recalculation(state: &mut AppState) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while state.recalc_work().is_some() {
        assert!(std::time::Instant::now() < deadline, "recalcul trop long");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    state.poll_recalc();
}

/// Projet enregistré, puis une commande dont le recalcul dépasse l'attente :
/// l'instantané garde l'ancienne révision, mais la commande est dans
/// l'acteur. Fermer, créer un projet ou en ouvrir un autre doit poser la
/// question « enregistrer ? » — sinon la modification est perdue sans un mot.
#[test]
fn closing_during_a_heavy_recalculation_still_asks_to_save() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("lourd.stf");
    let (_session, mut state) = start(heavy_formula_project(), &file);
    state.dispatch(Action::Save);
    let clean_title = state.window_title();
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    assert!(state.take_quit_ready(), "enregistré : Quit direct");

    state.dispatch(Action::AddGraph);
    assert!(state.recalc_work().is_some(), "recalcul encore en cours");
    assert_ne!(state.window_title(), clean_title, "titre : modifié");
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_some(), "la question est posée");
    assert!(!state.take_quit_ready());
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));

    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_some(), "Nouveau demande aussi");
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
    finish_recalculation(&mut state);
}

/// Témoin inverse : enregistrer pendant le recalcul remet le projet « propre »
/// — la révision appliquée ne doit pas rester au-dessus du repère enregistré.
#[test]
fn saving_during_a_heavy_recalculation_clears_the_unsaved_mark() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("lourd.stf");
    let (_session, mut state) = start(heavy_formula_project(), &file);
    state.dispatch(Action::Save);
    state.dispatch(Action::AddGraph);
    assert!(state.recalc_work().is_some());
    state.dispatch(Action::Save);
    finish_recalculation(&mut state);
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    assert!(
        state.unsaved_pending().is_none(),
        "plus rien à perdre ; messages : {}",
        status_messages(&state)
    );
    assert!(state.take_quit_ready());
}

// -- Mort de l'acteur de session (#310) --

/// L'acteur meurt : une seule alerte bloquante, une copie de secours relisible
/// du dernier état affiché, jamais écrite sur le fichier de l'utilisateur.
#[test]
fn a_dead_session_raises_one_blocking_alert_with_a_recovery_copy() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("travail.stf");
    let (session, mut state) = start(heavy_formula_project(), &file);
    state.dispatch(Action::AddGraph);
    finish_recalculation(&mut state);
    let tree = state.tree().clone();
    let recovery = dir.path().join("secours");
    state.set_recovery_dir(Some(recovery.clone()));
    assert!(!state.session_lost());
    assert!(state.alert().is_none());

    session.shutdown().unwrap();
    state.poll_recalc();

    assert!(state.session_lost());
    let alert = state.alert().expect("alerte bloquante");
    assert_eq!(
        alert.kind,
        stokesforge_app_state::state::AlertKind::SessionLost
    );
    // Seuls les `.stf` comptent : la copie a aussi sa fiche (#317).
    let stf_files = |dir: &std::path::Path| -> Vec<std::path::PathBuf> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "stf"))
            .collect()
    };
    let copies = stf_files(&recovery);
    assert_eq!(
        copies.len(),
        1,
        "une copie de secours ; alerte : {} ; messages : {}",
        alert.message,
        status_messages(&state)
    );
    assert!(alert.message.contains(&copies[0].display().to_string()));
    let back = stokesforge_persistence::load(&copies[0]).expect("la copie se relit");
    assert_eq!(back.tree(), &tree, "le dernier état affiché");
    assert!(
        !file.exists(),
        "le fichier de l'utilisateur n'est pas touché"
    );

    // Une seule fois : ni deuxième copie, ni deuxième alerte.
    state.dispatch(Action::DismissAlert);
    assert!(state.take_quit_ready(), "Close ferme l'application");
    state.poll_recalc();
    state.refresh();
    assert!(state.alert().is_none());
    assert_eq!(stf_files(&recovery).len(), 1);
}

/// Session morte : fermer la fenêtre n'ouvre pas « enregistrer ? » (il n'y a
/// plus rien à enregistrer) et sans dossier de secours le message reste vide.
#[test]
fn quitting_a_dead_session_asks_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state) = start(heavy_formula_project(), &dir.path().join("a.stf"));
    state.set_recovery_dir(None);
    state.dispatch(Action::AddGraph);
    finish_recalculation(&mut state);
    session.shutdown().unwrap();
    state.poll_recalc();
    assert!(state.session_lost());
    assert_eq!(state.alert().unwrap().message, "");

    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_none());
    assert!(state.take_quit_ready());
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
        state.status().is_some_and(|status| status.is_error()),
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

/// Attend la fin de l'import en fond (lecture, analyse, puis application par
/// `poll_import`), sans jamais attendre indéfiniment.
fn wait_import(state: &mut AppState) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while state.import_progress().is_some() {
        state.poll_import();
        assert!(std::time::Instant::now() < deadline, "import trop long");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Un dépôt de gros fichier rend la main aussitôt (#266) : la lecture et
/// l'analyse se font sur le fil de fond, rien n'est encore créé.
#[test]
fn dropping_a_big_csv_returns_immediately_and_applies_later() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();
    let file = dir.path().join("gros.csv");
    let mut text = String::new();
    for i in 0..100_000 {
        text.push_str(&format!("{i},{},{},{}\n", i * 2, i * 3, i * 4));
    }
    std::fs::write(&file, text).unwrap();

    state.dispatch(Action::DropFiles {
        paths: vec![file],
        target_book: Some(book),
    });

    assert!(
        state.import_work().is_some(),
        "travail en cours (S-P1/S-P2)"
    );
    assert_eq!(
        state.worksheets_of(book).len(),
        sheets_before,
        "rien encore"
    );
    wait_import(&mut state);
    assert_eq!(state.worksheets_of(book).len(), sheets_before + 1);
}

/// Un fichier illisible n'empêche pas le bon : l'assistant ne s'ouvre qu'au
/// retour du fil, avec le seul fichier illisible.
#[test]
fn a_background_drop_opens_the_wizard_only_when_the_thread_answers() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let bad = dir.path().join("binaire.dat");
    std::fs::write(&bad, [0u8, 159, 146, 150, 0, 0]).unwrap();
    let good = dir.path().join("bon.csv");
    std::fs::write(&good, "1,10\n2,20\n").unwrap();
    state.dispatch(Action::DropFiles {
        paths: vec![bad.clone(), good],
        target_book: Some(book),
    });
    assert!(state.import_state().is_none(), "assistant fermé pendant");
    wait_import(&mut state);
    assert_eq!(state.import_state().expect("assistant").paths, vec![bad]);
}

/// Tous illisibles : l'assistant s'ouvre, aucune macro vide dans l'historique.
#[test]
fn a_drop_of_only_unreadable_files_opens_the_wizard_without_history() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let bad = dir.path().join("binaire.dat");
    std::fs::write(&bad, [0u8, 159, 146, 150, 0, 0]).unwrap();
    state.dispatch(Action::DropFiles {
        paths: vec![bad.clone()],
        target_book: None,
    });
    wait_import(&mut state);
    assert_eq!(state.import_state().expect("assistant").paths, vec![bad]);
    assert!(!state.can_undo(), "aucune entrée d'historique");
    assert_ne!(
        state.status().map(|status| status.text.clone()),
        Some(stokesforge_app_state::tr!(
            "status-import-dropped",
            count = 0,
            models = ""
        )),
        "aucun import appliqué, aucun message de dépôt"
    );
}

/// Stop pendant un dépôt : rien n'est appliqué, l'assistant ne s'ouvre pas.
#[test]
fn stopping_a_background_drop_applies_nothing() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();
    let bad = dir.path().join("binaire.dat");
    std::fs::write(&bad, [0u8, 159, 146, 150, 0, 0]).unwrap();
    let good = dir.path().join("bon.csv");
    std::fs::write(&good, "1,10\n2,20\n").unwrap();
    state.dispatch(Action::DropFiles {
        paths: vec![bad, good],
        target_book: Some(book),
    });
    state.dispatch(Action::StopImport);
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(!state.poll_import());
    assert_eq!(state.worksheets_of(book).len(), sheets_before);
    assert!(state.import_state().is_none());
    assert!(!state.can_undo());
}

/// Le message « modèles utilisés » est propre au dépôt : un import de menu
/// ne l'affiche pas.
#[test]
fn a_menu_import_does_not_report_a_drop() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("menu.csv");
    std::fs::write(&file, "1,10\n2,20\n").unwrap();
    state.dispatch(Action::StartImport {
        destination: ImportDestination::ActiveBook(book),
        recipe: ImportRecipe::new("Menu"),
        paths: vec![file],
    });
    wait_import(&mut state);
    let status = state.status().unwrap();
    assert!(!status.is_error());
    assert_ne!(
        status.text,
        stokesforge_app_state::tr!("status-import-dropped", count = 1, models = ""),
        "pas de message de dépôt"
    );
}

/// Un dépôt pendant un import en cours est refusé.
#[test]
fn dropping_files_during_a_running_import_is_refused() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let a = dir.path().join("a.csv");
    std::fs::write(&a, "1,10\n2,20\n").unwrap();
    state.dispatch(Action::DropFiles {
        paths: vec![a.clone()],
        target_book: Some(book),
    });
    assert!(state.import_work().is_some());
    state.dispatch(Action::DropFiles {
        paths: vec![a],
        target_book: Some(book),
    });
    let status = state.status().unwrap();
    assert!(status.is_error());
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("error-import-running")
    );
    wait_import(&mut state);
}

/// Propriétés du graphe ouvertes quand le fil répond : l'application est
/// différée, la macro du graphe reste intacte, puis l'import s'applique en
/// entrée séparée.
#[test]
fn a_background_drop_waits_for_the_plot_settings_to_close() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let graph = plotted_graph(&mut state);
    let sheets_before = state.worksheets_of(book).len();
    let file = dir.path().join("depot.csv");
    std::fs::write(&file, "1,10\n2,20\n").unwrap();
    state.dispatch(Action::DropFiles {
        paths: vec![file],
        target_book: Some(book),
    });
    state.dispatch(Action::OpenPlotSettings {
        graph,
        tab: PlotSettingsTab::Graph,
        axis: None,
    });
    let before = session.handle().snapshot().unwrap();
    assert!(before.history.in_macro, "le dialogue ouvre sa macro");
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(!state.poll_import(), "différé");
    let during = session.handle().snapshot().unwrap();
    assert_eq!(*during.tree, *before.tree);
    assert!(during.history.in_macro);
    assert!(state.import_progress().is_some(), "travail gardé");
    state.cancel_plot_settings();
    wait_import(&mut state);
    assert_eq!(state.worksheets_of(book).len(), sheets_before + 1);
}

/// Stop (#232, S-P2) : l'import en fond s'arrête, rien n'est créé, ni
/// historique, et un nouvel import peut repartir aussitôt.
#[test]
fn stopping_a_background_import_creates_nothing() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();
    let files: Vec<std::path::PathBuf> = (0..3)
        .map(|i| {
            let file = dir.path().join(format!("stop{i}.csv"));
            std::fs::write(&file, "1,10\n2,20\n").unwrap();
            file
        })
        .collect();
    let start = |state: &mut AppState| {
        state.dispatch(Action::StartImport {
            destination: ImportDestination::ActiveBook(book),
            recipe: ImportRecipe::new("Stop"),
            paths: files.clone(),
        });
    };

    start(&mut state);
    assert!(state.import_work().is_some(), "un import est en cours");
    state.dispatch(Action::StopImport);
    assert!(state.import_work().is_none() && state.import_progress().is_none());
    assert!(!state.poll_import(), "rien à appliquer");
    assert_eq!(state.worksheets_of(book).len(), sheets_before);
    assert!(state.take_imported_paths().is_empty());
    assert!(!state.can_undo(), "aucune entrée d'historique");
    assert!(
        state.status().is_some_and(|status| !status.is_error()),
        "un arrêt voulu n'est pas une erreur"
    );

    // Témoin : un import lancé après Stop va jusqu'au bout.
    start(&mut state);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !state.poll_import() {
        assert!(std::time::Instant::now() < deadline, "import trop long");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(state.worksheets_of(book).len(), sheets_before + 3);
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
    // Le modèle reconnu prime même sur le dernier modèle utilisé, dont
    // les réglages ne conviennent pas du tout à la structure Jasco.
    let initial = dir.path().join("initial.csv");
    std::fs::write(&initial, "Temps,Signal\n1,10\n").unwrap();
    let mut previous_recipe = stokesforge_import::ImportRecipe::new("Dernier CSV");
    previous_recipe.has_column_header_row = true;
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe: previous_recipe,
        paths: vec![initial],
    });
    let before = state.worksheets_of(book).len();

    state.dispatch(Action::DropFiles {
        paths: vec![fixture_path("jasco_uvvis_semicolon.csv")],
        target_book: Some(book),
    });
    wait_import(&mut state);

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
    assert_eq!(
        state.import_state().unwrap().recipe.name,
        stokesforge_app_state::tr!("import-template-untitled")
    );

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
    assert!(stokesforge_import::RecipeStore::load()
        .recipes()
        .iter()
        .any(|r| r.name == "RPE (tabulation)" && r.skip_chars == 4));

    // Un fichier qui n'est pas un modèle : message d'erreur, rien ne change.
    let bad = dir.path().join("pas_un_modele.json");
    std::fs::write(&bad, "{ pas du json").unwrap();
    state.import_import_recipe(&bad);
    assert!(
        state.dialog_error().is_some(),
        "S-M7 : dans le pied de l'assistant"
    );
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
    wait_import(&mut state);

    assert_eq!(state.worksheets_of(book).len(), before + 2);
    let status = state.status().unwrap();
    assert!(status.text.contains("Jasco (point-virgule)"));
    assert!(status.text.contains("RPE (tabulation)"));
    state.dispatch(Action::Undo);
    assert_eq!(state.worksheets_of(book).len(), before);
}

/// Colonne date-heure et colonne forcée en texte à l'import (issue #7).
#[test]
fn import_builds_datetime_and_forced_text_columns() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("natures.csv");
    std::fs::write(
        &file,
        "007;2000-01-01 00:00:00;1\n008;2000-01-02 00:00:00;2\n",
    )
    .unwrap();
    let mut recipe = ImportRecipe::new("Natures");
    recipe.separator = Separator::Semicolon;
    recipe.column_kinds = vec![
        stokesforge_import::ColumnKind::Text,
        stokesforge_import::ColumnKind::DateTime,
    ];

    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![file],
    });

    let sheet = sheet_named(state.tree(), "natures");
    let columns = state.visible_columns(sheet);
    assert_eq!(column_type(&state, columns[0].0), ColumnType::Mixed);
    match cell(&state, columns[0].0, 0) {
        Cell::Text(Some(text)) => assert_eq!(text, "007"),
        other => panic!("attendu le texte 007, obtenu {other:?}"),
    }
    assert_eq!(column_type(&state, columns[1].0), ColumnType::DateTime);
    assert_eq!(cell_f64(&state, columns[1].0, 0), 946_684_800.0);
    assert_eq!(column_type(&state, columns[2].0), ColumnType::Numeric);
}

// -- Modes d'import et répartition de plusieurs fichiers (issue #7) --

fn write_csv(dir: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
    let file = dir.join(name);
    std::fs::write(&file, content).unwrap();
    file
}

fn column_values(state: &AppState, sheet: NodeId, position: usize) -> Vec<f64> {
    let (column, ..) = state.visible_columns(sheet)[position].clone();
    (0..state.row_count(sheet))
        .map(|row| cell_f64(state, column, row))
        .collect()
}

fn import_with(
    state: &mut AppState,
    book: NodeId,
    mode: stokesforge_import::ImportMode,
    layout: stokesforge_import::FilesLayout,
    files: Vec<std::path::PathBuf>,
) {
    let mut recipe = ImportRecipe::new("Modes");
    recipe.import_mode = mode;
    recipe.files_layout = layout;
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: files,
    });
}

/// Remplacer la feuille active vide ses colonnes d'avant et garde la feuille ;
/// un seul Ctrl+Z rend l'ancien contenu.
#[test]
fn import_mode_replace_sheet_swaps_the_content_in_one_entry() {
    use stokesforge_import::{FilesLayout, ImportMode};
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheet = state.selected_sheet_in(book).unwrap();
    let sheets = state.worksheets_of(book).len();
    let before = state.visible_columns(sheet).len();
    assert!(before > 0);
    let file = write_csv(dir.path(), "neuf.csv", "1,10\n2,20\n3,30\n");

    import_with(
        &mut state,
        book,
        ImportMode::ReplaceSheet,
        FilesLayout::SheetPerFile,
        vec![file],
    );

    assert_eq!(
        state.worksheets_of(book).len(),
        sheets,
        "pas de nouvelle feuille"
    );
    assert_eq!(state.visible_columns(sheet).len(), 2);
    assert_eq!(column_values(&state, sheet, 0), vec![1.0, 2.0, 3.0]);
    state.dispatch(Action::Undo);
    assert_eq!(
        state.visible_columns(sheet).len(),
        before,
        "l'ancien contenu revient"
    );
}

/// Ajouter des colonnes : à la suite de celles de la feuille active.
#[test]
fn import_mode_add_columns_appends_to_the_active_sheet() {
    use stokesforge_import::{FilesLayout, ImportMode};
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheet = state.selected_sheet_in(book).unwrap();
    let before = state.visible_columns(sheet).len();
    let file = write_csv(dir.path(), "plus.csv", "7,70\n8,80\n");

    import_with(
        &mut state,
        book,
        ImportMode::AddColumns,
        FilesLayout::SheetPerFile,
        vec![file],
    );

    assert_eq!(state.visible_columns(sheet).len(), before + 2);
    assert_eq!(column_values(&state, sheet, before)[..2], [7.0, 8.0]);
}

/// Ajouter des lignes : la colonne j du fichier prolonge la colonne j de la
/// feuille.
#[test]
fn import_mode_add_rows_extends_the_existing_columns() {
    use stokesforge_import::{FilesLayout, ImportMode};
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheet = state.selected_sheet_in(book).unwrap();
    let rows_before = state.row_count(sheet);
    let columns_before = state.visible_columns(sheet).len();
    let first_before = column_values(&state, sheet, 0);
    let file = write_csv(dir.path(), "suite.csv", "100,200\n101,201\n");

    import_with(
        &mut state,
        book,
        ImportMode::AddRows,
        FilesLayout::SheetPerFile,
        vec![file],
    );

    assert_eq!(state.row_count(sheet), rows_before + 2);
    assert_eq!(
        state.visible_columns(sheet).len(),
        columns_before,
        "mêmes colonnes"
    );
    let first = column_values(&state, sheet, 0);
    assert_eq!(first[..rows_before], first_before[..]);
    assert_eq!(first[rows_before..], [100.0, 101.0]);
    state.dispatch(Action::Undo);
    assert_eq!(state.row_count(sheet), rows_before);
}

/// Plusieurs fichiers, une seule feuille : côte à côte (X Y X Y), ou avec le X
/// du premier fichier partagé.
#[test]
fn several_files_can_share_one_sheet_side_by_side_or_with_a_common_x() {
    use stokesforge_import::{FilesLayout, ImportMode};
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let a = write_csv(dir.path(), "spec_a.csv", "1,10\n2,20\n");
    let b = write_csv(dir.path(), "spec_b.csv", "1,11\n2,21\n");
    let sheets = state.worksheets_of(book).len();

    import_with(
        &mut state,
        book,
        ImportMode::NewSheet,
        FilesLayout::SideBySide,
        vec![a.clone(), b.clone()],
    );
    assert_eq!(
        state.worksheets_of(book).len(),
        sheets + 1,
        "une seule feuille"
    );
    let sheet = sheet_named(state.tree(), "spec_a");
    assert_eq!(state.visible_columns(sheet).len(), 4, "X Y X Y");
    assert_eq!(column_values(&state, sheet, 3), vec![11.0, 21.0]);
    state.dispatch(Action::Undo);
    assert_eq!(state.worksheets_of(book).len(), sheets);

    import_with(
        &mut state,
        book,
        ImportMode::NewSheet,
        FilesLayout::SharedX,
        vec![a, b],
    );
    let sheet = sheet_named(state.tree(), "spec_a");
    let columns = state.visible_columns(sheet);
    assert_eq!(columns.len(), 3, "un X, puis un Y par fichier");
    assert_eq!(state.column_meta(columns[0].0).role, ColumnRole::X);
    assert_eq!(state.column_meta(columns[2].0).role, ColumnRole::Y);
    assert_eq!(column_values(&state, sheet, 1), vec![10.0, 20.0]);
    assert_eq!(column_values(&state, sheet, 2), vec![11.0, 21.0]);
    // Les Y portent le nom de leur fichier pour la légende.
    assert_eq!(state.column_meta(columns[1].0).long_name, "spec_a");
    assert_eq!(state.column_meta(columns[2].0).long_name, "spec_b");
}

/// Réimport (issue #7) : les colonnes importées se relisent depuis leur fichier
/// avec la recette mémorisée, **en place** — mêmes colonnes (donc les graphes
/// qui les tracent restent valides), valeurs neuves, nombre de lignes suivi —,
/// en une seule entrée annulable.
#[test]
fn refreshing_from_the_source_file_updates_the_columns_in_place() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = write_csv(dir.path(), "mesure.csv", "1;10\n2;20\n");
    let mut recipe = ImportRecipe::new("Point-virgule");
    recipe.separator = Separator::Semicolon;
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![file.clone()],
    });
    let sheet = sheet_named(state.tree(), "mesure");
    assert!(state.can_refresh_from_source(sheet));
    let ids_before: Vec<NodeId> = state.visible_columns(sheet).iter().map(|c| c.0).collect();

    // Le fichier change : plus de lignes, autres valeurs.
    write_csv(dir.path(), "mesure.csv", "5;50\n6;60\n7;70\n");
    state.dispatch(Action::RefreshFromSource { sheet });

    let ids_after: Vec<NodeId> = state.visible_columns(sheet).iter().map(|c| c.0).collect();
    assert_eq!(ids_before, ids_after, "les colonnes gardent leur identité");
    assert_eq!(column_values(&state, sheet, 0), vec![5.0, 6.0, 7.0]);
    assert_eq!(column_values(&state, sheet, 1), vec![50.0, 60.0, 70.0]);
    assert!(state.status().is_some_and(|s| !s.is_error()));

    // Un seul Ctrl+Z revient aux valeurs d'avant l'actualisation.
    state.dispatch(Action::Undo);
    assert_eq!(column_values(&state, sheet, 0), vec![1.0, 2.0]);

    // Un fichier disparu est signalé sans rien casser.
    std::fs::remove_file(&file).unwrap();
    state.dispatch(Action::RefreshFromSource { sheet });
    assert!(state.status().is_some_and(|s| s.is_error()));
    assert_eq!(column_values(&state, sheet, 0), vec![1.0, 2.0]);
    // La feuille de démonstration, saisie à la main, n'a rien à actualiser.
    let demo_sheet = state.selected_sheet_in(book).unwrap();
    assert!(!state.can_refresh_from_source(demo_sheet));
}

/// Un fichier à plusieurs blocs donne une feuille par bloc (nommées d'après le
/// fichier et le rang), ou une feuille commune selon la répartition choisie ;
/// l'actualisation retrouve le bon bloc de chaque colonne (issue #7).
#[test]
fn a_multi_block_file_gives_one_sheet_per_block_and_refreshes_per_block() {
    use stokesforge_import::{BlockSplit, HeaderSkip};
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let before = state.worksheets_of(book).len();
    let file = write_csv(
        dir.path(),
        "spectres.csv",
        "TITLE;a\nXYDATA\n1;10\n2;20\nTITLE;b\nXYDATA\n3;30\n4;40\n5;50\n",
    );
    let mut recipe = ImportRecipe::new("Blocs");
    recipe.separator = Separator::Semicolon;
    recipe.header = HeaderSkip::UntilMarker("XYDATA".into());
    recipe.block_split = BlockSplit::BeforeLine("TITLE".into());

    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![file.clone()],
    });

    assert_eq!(state.worksheets_of(book).len(), before + 2);
    let first = sheet_named(state.tree(), "spectres_1");
    let second = sheet_named(state.tree(), "spectres_2");
    assert_eq!(column_values(&state, first, 1), vec![10.0, 20.0]);
    assert_eq!(column_values(&state, second, 1), vec![30.0, 40.0, 50.0]);
    assert_eq!(
        state.status().unwrap().text,
        stokesforge_app_state::tr!("status-import-done", imported = 1, total = 1)
    );

    // Le fichier change : chaque bloc se met à jour dans sa propre feuille.
    write_csv(
        dir.path(),
        "spectres.csv",
        "TITLE;a\nXYDATA\n1;11\n2;22\nTITLE;b\nXYDATA\n3;33\n4;44\n5;55\n",
    );
    state.dispatch(Action::RefreshFromSource { sheet: first });
    state.dispatch(Action::RefreshFromSource { sheet: second });
    assert_eq!(column_values(&state, first, 1), vec![11.0, 22.0]);
    assert_eq!(column_values(&state, second, 1), vec![33.0, 44.0, 55.0]);

    // Un seul Ctrl+Z pour l'import entier.
    while state.can_undo() {
        state.dispatch(Action::Undo);
    }
    assert_eq!(state.worksheets_of(book).len(), before);
}

// -- Issue #34 : exporter un graphe --

/// Un projet de démonstration avec un graphe titré en Typst.
fn demo_with_graph(dir: &Path) -> (Session, AppState, NodeId) {
    let mut project = demo_project();
    let root = project.root();
    let sheet = sheet_named(project.tree(), "Sheet1");
    let colonne = |name: &str| {
        project
            .tree()
            .child_by_name(sheet, name)
            .expect("Sheet1 lisible")
            .expect("colonne")
    };
    let (x, y) = (colonne("Temps"), colonne("Signal"));
    let mut panel = stokesforge_plot::PanelSpec::at(0, 0);
    panel
        .series
        .push(stokesforge_plot::SeriesSpec::new_xy(x, y));
    let spec = stokesforge_plot::PlotSpec::empty("Spectre $Delta H$").with_panels(panel);
    let graph = project
        .add_graph(root, Some("Graph1"), spec)
        .expect("graphe");
    let (session, state) = start(project, &dir.join("projet.stf"));
    (session, state, graph)
}

fn graph_spec(state: &AppState, graph: NodeId) -> stokesforge_plot::PlotSpec {
    match state.tree().get(graph).expect("graphe").kind() {
        stokesforge_core::node::NodeKind::Graph { spec, .. } => spec.clone(),
        _ => panic!("pas un graphe"),
    }
}

/// L'export écrit ce qu'on lui donne — le spec cadré par la vue, donc le
/// zoom courant — sans toucher au modèle : ni le `PlotSpec` du graphe ni la
/// révision ne bougent. Le témoin est l'export du spec non cadré, qui
/// diffère.
#[test]
fn exporter_un_graphe_zoome_ne_touche_pas_le_modele() {
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let avant = (graph_spec(&state, graph), state.snapshot().revision);

    // Ce que `GraphView::framed_spec` rend après un zoom : un clone cadré.
    let mut zoome = avant.0.clone();
    if let Some(panel) = zoome.panels.get_mut(0) {
        panel.bottom.auto_range = false;
        panel.bottom.range_min = Some(0.5);
        panel.bottom.range_max = Some(1.0);
    }
    let exporte = |state: &mut AppState, nom: &str, spec: &stokesforge_plot::PlotSpec| {
        let path = dir.path().join(nom);
        state.dispatch(Action::ExportGraph {
            path: path.clone(),
            spec: Box::new(spec.clone()),
        });
        let status = state.status().expect("un message");
        assert!(!status.is_error(), "{}", status.text);
        std::fs::read(path).expect("fichier exporté")
    };
    let svg_zoome = exporte(&mut state, "zoome.svg", &zoome);
    let svg_entier = exporte(&mut state, "entier.svg", &avant.0);
    assert_ne!(
        svg_zoome, svg_entier,
        "le cadrage zoomé n'a pas été exporté"
    );
    assert!(String::from_utf8_lossy(&svg_zoome).contains("<svg"));

    assert_eq!(graph_spec(&state, graph), avant.0, "le modèle a changé");
    assert_eq!(
        state.snapshot().revision,
        avant.1,
        "une commande a été émise"
    );
}

/// S-F3 (#229) : le sélecteur natif demande avant d'écraser le nom saisi ;
/// l'extension `.svg` ajoutée ensuite vise un autre fichier, qu'il n'a pas
/// vu. S'il existe, l'export attend la confirmation : Cancel le laisse
/// intact, Replace l'écrase. Témoin : un nom avec extension n'attend rien.
#[test]
fn un_export_qui_ecraserait_un_fichier_non_vu_demande_confirmation() {
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let spec = Box::new(graph_spec(&state, graph));
    let existant = dir.path().join("figure.svg");
    std::fs::write(&existant, "ancien").unwrap();
    let exporte = |state: &mut AppState, nom: &str| {
        state.dispatch(Action::ExportGraph {
            path: dir.path().join(nom),
            spec: spec.clone(),
        });
    };

    exporte(&mut state, "figure");
    assert_eq!(state.pending_overwrite(), Some(existant.as_path()));
    assert!(state.blocking_dialog());
    state.dispatch(Action::ConfirmOverwrite(false));
    assert!(state.pending_overwrite().is_none());
    assert_eq!(std::fs::read_to_string(&existant).unwrap(), "ancien");

    exporte(&mut state, "figure");
    state.dispatch(Action::ConfirmOverwrite(true));
    assert!(std::fs::read_to_string(&existant).unwrap().contains("<svg"));

    std::fs::write(&existant, "ancien").unwrap();
    exporte(&mut state, "figure.svg");
    assert!(state.pending_overwrite().is_none(), "vu par le sélecteur");
    assert!(std::fs::read_to_string(&existant).unwrap().contains("<svg"));
}

/// Le format suit l'extension : PDF et PNG (à l'agrandissement réglé), `.svg`
/// ajouté à un nom sans extension, et une extension inconnue est refusée
/// avec un message, sans fichier écrit.
#[test]
fn le_format_dexport_suit_lextension() {
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let spec = Box::new(graph_spec(&state, graph));
    let exporte = |state: &mut AppState, nom: &str| {
        state.dispatch(Action::ExportGraph {
            path: dir.path().join(nom),
            spec: spec.clone(),
        });
        state.status().expect("un message").clone()
    };

    assert!(!exporte(&mut state, "figure.pdf").is_error());
    let pdf = std::fs::read(dir.path().join("figure.pdf")).expect("PDF");
    assert!(pdf.starts_with(b"%PDF"));

    state.dispatch(Action::SetPngExportScale(2.0));
    assert_eq!(state.png_export_scale(), 2.0);
    assert!(!exporte(&mut state, "figure.png").is_error());
    let png = std::fs::read(dir.path().join("figure.png")).expect("PNG");
    assert_eq!(
        u32::from_be_bytes([png[16], png[17], png[18], png[19]]),
        960
    );
    // Un agrandissement hors de la liste proposée est ignoré.
    state.dispatch(Action::SetPngExportScale(3.0));
    assert_eq!(state.png_export_scale(), 2.0);

    assert!(!exporte(&mut state, "sans_extension").is_error());
    assert!(dir.path().join("sans_extension.svg").exists());

    assert!(exporte(&mut state, "figure.jpg").is_error());
    assert!(!dir.path().join("figure.jpg").exists());
}

/// #162 : l'export SVG garde le texte en `<text>` par défaut, passe en
/// contours (aucun `<text>`) quand l'utilisateur le choisit ; le PDF et le PNG
/// ne dépendent pas de ce réglage, qui n'entre ni au modèle ni à l'historique.
#[test]
fn le_mode_du_texte_svg_suit_la_preference_et_ne_touche_ni_modele_ni_historique() {
    use stokesforge_render_svg::SvgTextMode;
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let mut spec = graph_spec(&state, graph);
    spec.title = "Spectre".to_owned();
    let spec = Box::new(spec);
    let revision = state.revision();
    let exporte = |state: &mut AppState, nom: &str| {
        state.dispatch(Action::ExportGraph {
            path: dir.path().join(nom),
            spec: spec.clone(),
        });
        let status = state.status().expect("un message");
        assert!(!status.is_error(), "{}", status.text);
        std::fs::read_to_string(dir.path().join(nom)).expect("SVG")
    };

    assert_eq!(state.svg_text_mode(), SvgTextMode::Text);
    let texte = exporte(&mut state, "texte.svg");
    assert!(texte.contains("<text"), "texte SVG attendu par défaut");
    assert!(texte.contains("Spectre"));

    state.dispatch(Action::SetSvgTextMode(SvgTextMode::Outlines));
    assert_eq!(state.svg_text_mode(), SvgTextMode::Outlines);
    let contours = exporte(&mut state, "contours.svg");
    assert!(!contours.contains("<text"), "aucun <text> en contours");

    assert_eq!(state.revision(), revision, "une commande a été émise");
}

#[test]
fn le_nom_propose_est_celui_du_graphe() {
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let (_session, state, graph) = demo_with_graph(dir.path());
    assert_eq!(
        stokesforge_app_state::state::export_file_name(state.tree(), graph),
        "Graph1.svg"
    );
}

// -- Issue #35 : textes libres --

/// Ajouter, déplacer, retirer un texte libre : chaque geste est **une**
/// modification du modèle, donc un seul Ctrl+Z — quel que soit le nombre
/// d'images qu'a duré l'édition ou le glisser, dont l'aperçu vit dans la
/// vue. Et un geste qui ne change rien n'empile rien.
#[test]
fn un_texte_libre_sajoute_se_deplace_et_se_retire_en_un_pas_chacun() {
    use stokesforge_plot::{AnnotationRef, AnnotationSpec};
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let target = AnnotationRef { panel: 0, index: 0 };
    let texte = |state: &AppState| target.get(&graph_spec(state, graph)).cloned();
    let revision = |state: &AppState| state.snapshot().revision;

    let depart = revision(&state);
    state.dispatch(Action::SetAnnotation {
        graph,
        target,
        annotation: Box::new(AnnotationSpec::new("Pic $lambda$", 1.0, 30.0)),
    });
    assert_eq!(
        texte(&state).map(|a| a.text),
        Some("Pic $lambda$".to_owned())
    );
    let apres_ajout = revision(&state);
    assert_ne!(apres_ajout, depart);

    // Le même texte : rien ne change, rien n'est empilé.
    state.dispatch(Action::SetAnnotation {
        graph,
        target,
        annotation: Box::new(AnnotationSpec::new("Pic $lambda$", 1.0, 30.0)),
    });
    assert_eq!(
        revision(&state),
        apres_ajout,
        "un geste sans effet a été empilé"
    );

    // Déplacé : une seule entrée, qu'un Ctrl+Z défait entièrement.
    state.dispatch(Action::SetAnnotation {
        graph,
        target,
        annotation: Box::new(AnnotationSpec::new("Pic $lambda$", 1.5, 40.0)),
    });
    assert_eq!(texte(&state).map(|a| (a.x, a.y)), Some((1.5, 40.0)));
    state.dispatch(Action::Undo);
    assert_eq!(texte(&state).map(|a| (a.x, a.y)), Some((1.0, 30.0)));

    state.dispatch(Action::RemoveAnnotation { graph, target });
    assert!(texte(&state).is_none());
    state.dispatch(Action::Undo);
    assert!(texte(&state).is_some(), "Ctrl+Z rend le texte retiré");
    state.dispatch(Action::Undo);
    assert!(texte(&state).is_none(), "Ctrl+Z défait l'ajout");

    // Une adresse au-delà du dernier rang est ignorée.
    state.dispatch(Action::SetAnnotation {
        graph,
        target: AnnotationRef { index: 4, ..target },
        annotation: Box::new(AnnotationSpec::new("x", 0.0, 0.0)),
    });
    assert!(graph_spec(&state, graph).panels[0].annotations.is_empty());
}

// -- Issue #76 : objets graphiques --

/// Ajouter, déplacer, empiler et retirer un objet : chaque geste est **une**
/// modification du modèle (un seul Ctrl+Z), un geste sans effet n'empile rien,
/// et passer au premier plan un objet déjà dessus non plus.
#[test]
fn un_objet_graphique_sajoute_sempile_et_se_retire_en_un_pas_chacun() {
    use stokesforge_app_state::state::ShapeOrder;
    use stokesforge_plot::{ShapeKind, ShapeRef, ShapeSpec};
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let at = |index| ShapeRef { panel: 0, index };
    let formes = |state: &AppState| graph_spec(state, graph).panels[0].shapes.clone();
    let revision = |state: &AppState| state.snapshot().revision;
    let rect = |x: f64| ShapeSpec::new(ShapeKind::Rectangle, x, 0.0, x + 1.0, 1.0);

    for (index, x) in [(0, 1.0), (1, 5.0)] {
        state.dispatch(Action::SetShape {
            graph,
            target: at(index),
            shape: Box::new(rect(x)),
        });
    }
    assert_eq!(formes(&state).len(), 2);

    // Le même objet : rien n'est empilé.
    let apres_ajout = revision(&state);
    state.dispatch(Action::SetShape {
        graph,
        target: at(1),
        shape: Box::new(rect(5.0)),
    });
    assert_eq!(revision(&state), apres_ajout);

    // Déjà au premier plan : rien n'est empilé.
    state.dispatch(Action::ReorderShape {
        graph,
        target: at(1),
        order: ShapeOrder::ToFront,
    });
    assert_eq!(revision(&state), apres_ajout);

    // Au premier plan : l'ordre change, un Ctrl+Z le défait.
    state.dispatch(Action::ReorderShape {
        graph,
        target: at(0),
        order: ShapeOrder::ToFront,
    });
    assert_eq!(
        formes(&state).iter().map(|s| s.x1).collect::<Vec<_>>(),
        [5.0, 1.0]
    );
    state.dispatch(Action::Undo);
    assert_eq!(
        formes(&state).iter().map(|s| s.x1).collect::<Vec<_>>(),
        [1.0, 5.0]
    );

    state.dispatch(Action::RemoveShape {
        graph,
        target: at(0),
    });
    assert_eq!(formes(&state).len(), 1);
    state.dispatch(Action::Undo);
    assert_eq!(formes(&state).len(), 2, "Ctrl+Z rend l'objet retiré");

    // Une adresse au-delà du dernier rang est ignorée.
    state.dispatch(Action::SetShape {
        graph,
        target: at(9),
        shape: Box::new(rect(0.0)),
    });
    assert_eq!(formes(&state).len(), 2);
}

// -- Issue #53 : titres, titres d'axe et légende déplaçables --

/// Déplacer un titre, un titre d'axe ou la légende, puis les remettre : chaque
/// geste est **une** modification du modèle (un seul Ctrl+Z), et un geste sans
/// effet n'empile rien.
/// Issues #77 et #78 : la police et le corps d'un titre, d'un titre d'axe ou
/// de la légende sont un attribut de la zone, posé en une entrée annulable.
#[test]
fn la_police_d_une_zone_de_texte_se_pose_en_un_pas_annulable() {
    use stokesforge_plot::{AxisSide, TextFont, TextTarget};
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let cibles = [
        TextTarget::Title,
        TextTarget::AxisTitle {
            panel: 0,
            side: AxisSide::Left,
        },
        TextTarget::Legend { panel: 0 },
    ];
    let police = |state: &AppState, cible: TextTarget| cible.font(&graph_spec(state, graph));
    let voulue = TextFont {
        family: "Fira Sans".to_string(),
        size: Some(20.0),
        ..Default::default()
    };
    for cible in cibles {
        assert_eq!(police(&state, cible), Some(TextFont::default()));
        state.dispatch(Action::SetTextFont {
            graph,
            target: cible,
            font: voulue.clone(),
        });
        assert_eq!(police(&state, cible), Some(voulue.clone()));
        let apres = state.snapshot().revision;
        state.dispatch(Action::SetTextFont {
            graph,
            target: cible,
            font: voulue.clone(),
        });
        assert_eq!(
            state.snapshot().revision,
            apres,
            "un réglage identique a été empilé"
        );
        state.dispatch(Action::Undo);
        assert_eq!(police(&state, cible), Some(TextFont::default()));
    }
}

#[test]
fn la_couleur_d_une_zone_de_texte_se_pose_en_un_pas_annulable() {
    use stokesforge_plot::{AxisSide, TextTarget};
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let cibles = [
        TextTarget::Title,
        TextTarget::PanelTitle { panel: 0 },
        TextTarget::AxisTitle {
            panel: 0,
            side: AxisSide::Left,
        },
        TextTarget::Legend { panel: 0 },
    ];
    let couleur = |state: &AppState, cible: TextTarget| cible.color(&graph_spec(state, graph));
    for cible in cibles {
        let avant = couleur(&state, cible);
        state.dispatch(Action::SetTextColor {
            graph,
            target: cible,
            color: "#ff0000".to_string(),
        });
        assert_eq!(couleur(&state, cible), Some("#ff0000".to_string()));
        state.dispatch(Action::Undo);
        assert_eq!(couleur(&state, cible), avant, "{cible:?}");
    }
}

#[test]
fn un_titre_se_deplace_et_se_reinitialise_en_un_pas_chacun() {
    use stokesforge_plot::{AxisSide, TextAnchor, TextTarget};
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let cibles = [
        TextTarget::Title,
        TextTarget::AxisTitle {
            panel: 0,
            side: AxisSide::Left,
        },
        TextTarget::Legend { panel: 0 },
    ];
    let ancre = |state: &AppState, cible: TextTarget| cible.anchor(&graph_spec(state, graph));
    let revision = |state: &AppState| state.snapshot().revision;

    for cible in cibles {
        let avant = revision(&state);
        state.dispatch(Action::SetTextAnchor {
            graph,
            target: cible,
            anchor: Some(TextAnchor::new(1.0, 2.0)),
        });
        assert_eq!(ancre(&state, cible), Some(TextAnchor::new(1.0, 2.0)));
        let apres = revision(&state);
        assert_ne!(apres, avant);

        // La même ancre : rien n'est empilé.
        state.dispatch(Action::SetTextAnchor {
            graph,
            target: cible,
            anchor: Some(TextAnchor::new(1.0, 2.0)),
        });
        assert_eq!(revision(&state), apres, "un geste sans effet a été empilé");

        // Un seul Ctrl+Z défait le déplacement.
        state.dispatch(Action::Undo);
        assert_eq!(ancre(&state, cible), None);
        state.dispatch(Action::Redo);
        assert_eq!(ancre(&state, cible), Some(TextAnchor::new(1.0, 2.0)));

        // « Reset Position » d'un seul texte : une entrée.
        state.dispatch(Action::SetTextAnchor {
            graph,
            target: cible,
            anchor: None,
        });
        assert_eq!(ancre(&state, cible), None);
        state.dispatch(Action::Undo);
        assert_eq!(ancre(&state, cible), Some(TextAnchor::new(1.0, 2.0)));
    }

    // Tout remettre d'un coup : une entrée pour l'ensemble.
    assert!(graph_spec(&state, graph).has_text_anchors());
    state.dispatch(Action::ResetTextAnchors { graph });
    assert!(!graph_spec(&state, graph).has_text_anchors());
    let apres_reset = revision(&state);
    state.dispatch(Action::ResetTextAnchors { graph });
    assert_eq!(
        revision(&state),
        apres_reset,
        "tout remettre quand rien n'est déplacé n'empile rien"
    );
    state.dispatch(Action::Undo);
    for cible in cibles {
        assert!(ancre(&state, cible).is_some(), "{cible:?} rendu par Ctrl+Z");
    }
}

/// Éditer un titre au double-clic, renommer une entrée de légende : une
/// modification chacun, et un texte inchangé ne fige rien.
#[test]
fn un_titre_et_un_nom_de_legende_se_modifient_en_un_pas() {
    use stokesforge_plot::{SeriesRef, TextTarget};
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let titre = TextTarget::AxisTitle {
        panel: 0,
        side: stokesforge_plot::AxisSide::Bottom,
    };
    state.dispatch(Action::SetTextContent {
        graph,
        target: titre,
        text: "Temps $t$".to_owned(),
    });
    assert_eq!(
        graph_spec(&state, graph).panels[0].bottom.label,
        "Temps $t$"
    );
    state.dispatch(Action::SetTextContent {
        graph,
        target: TextTarget::Title,
        text: "Courbe".to_owned(),
    });
    assert_eq!(graph_spec(&state, graph).title, "Courbe");
    // La légende n'a pas de texte à elle : sans effet.
    let avant = state.snapshot().revision;
    state.dispatch(Action::SetTextContent {
        graph,
        target: TextTarget::Legend { panel: 0 },
        text: "x".to_owned(),
    });
    assert_eq!(state.snapshot().revision, avant);

    let serie = SeriesRef::new(0, 0);
    state.dispatch(Action::SetLegendEntryName {
        graph,
        series: serie,
        name: "Échantillon".to_owned(),
    });
    assert_eq!(
        graph_spec(&state, graph)
            .series_at(serie)
            .map(|s| s.name.clone()),
        Some("Échantillon".to_owned())
    );
    state.dispatch(Action::Undo);
    assert_eq!(
        graph_spec(&state, graph)
            .series_at(serie)
            .map(|s| s.name.clone()),
        Some(String::new())
    );
    // Une série qui n'existe pas : ignorée.
    let avant = state.snapshot().revision;
    state.dispatch(Action::SetLegendEntryName {
        graph,
        series: SeriesRef::new(0, 99),
        name: "x".to_owned(),
    });
    assert_eq!(state.snapshot().revision, avant);
}

#[test]
fn object_manager_visibilite_groupe_annulable_et_persistante() {
    use stokesforge_app_state::object_manager::{object_tree, VisibilityTarget};
    use stokesforge_plot::{
        AnnotationSpec, PanelSpec, PlotSpec, SeriesRef, SeriesSpec, ShapeKind, ShapeSpec,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut project = demo_project();
    let root = project.root();
    let book = project
        .tree()
        .child_by_name(root, "Book1")
        .unwrap()
        .unwrap();
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .unwrap()
        .unwrap();
    let column = project
        .tree()
        .child_by_name(sheet, "Signal")
        .unwrap()
        .unwrap();
    let mut first = PanelSpec::at(0, 0);
    first.series = vec![SeriesSpec::new(column), SeriesSpec::new(column)];
    first.series[1].name = "Référence".into();
    first
        .annotations
        .push(AnnotationSpec::new("Pic", 0.5, 10.0));
    first
        .shapes
        .push(ShapeSpec::new(ShapeKind::Rectangle, 0.0, 0.0, 1.0, 20.0));
    let mut second = PanelSpec::at(1, 0);
    second.series.push(SeriesSpec::new(column));
    let spec = PlotSpec::empty("Mesures").with_panels(vec![first, second]);
    let graph = project.add_graph(root, None, spec.clone()).unwrap();
    let (_session, mut state) = start(project, &dir.path().join("objets.stf"));
    let target = SeriesRef::new(0, 1);
    let tree = object_tree(state.tree(), graph, Some(target)).unwrap();
    assert_eq!(tree.layers.len(), 2);
    assert_eq!(tree.layers[0].series[1].label, "Référence");
    assert!(tree.layers[0].series[1].selected);
    assert!(!tree.layers[1].series[0].selected);

    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: VisibilityTarget::Series(target),
        visible: false,
    });
    let hidden = graph_spec(&state, graph);
    assert!(!hidden.series_at(target).unwrap().visible);
    assert_eq!(hidden.series_at(target).unwrap().name, "Référence");
    assert!(VisibilityTarget::Group(0).visibility(&hidden).mixed);
    let revision = state.revision();
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: VisibilityTarget::Series(target),
        visible: false,
    });
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: VisibilityTarget::Series(SeriesRef::new(9, 9)),
        visible: false,
    });
    assert_eq!(
        state.revision(),
        revision,
        "ni commande identique, ni cible périmée"
    );
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), spec);
    state.dispatch(Action::Redo);
    assert_eq!(graph_spec(&state, graph), hidden);

    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: VisibilityTarget::Panel(0),
        visible: false,
    });
    let panel_hidden = graph_spec(&state, graph);
    assert!(!panel_hidden.panels[0].annotations[0].visible);
    assert!(!panel_hidden.panels[0].shapes[0].visible);
    assert!(
        panel_hidden.panels[1].series[0].visible,
        "l'autre calque reste visible"
    );
    state.dispatch(Action::Undo);
    assert_eq!(
        graph_spec(&state, graph),
        hidden,
        "le calque entier s'annule en une entrée"
    );
    state.dispatch(Action::Redo);
    state.dispatch(Action::Save);
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: VisibilityTarget::Graph,
        visible: true,
    });
    state.dispatch(Action::Load);
    assert_eq!(graph_spec(&state, graph), panel_hidden);
}

// -- Object Manager : gestes et contenus non-série (#84–85) --

fn object_manager_project() -> (Project, NodeId, stokesforge_plot::PlotSpec) {
    use stokesforge_plot::{AnnotationSpec, PanelSpec, PlotSpec, SeriesSpec, ShapeKind, ShapeSpec};
    let mut project = Project::new("Objets").unwrap();
    let mut first = PanelSpec::at(0, 0);
    first.title = "Calque A".into();
    first.bottom.label = "Temps".into();
    first.bottom.unit = "s".into();
    first.left.label = "Signal".into();
    for name in ["A", "B", "C"] {
        let mut series = SeriesSpec::new(NodeId::from_raw(900));
        series.name = name.into();
        first.series.push(series);
    }
    first.annotations.push(AnnotationSpec::new("Pic", 1.0, 2.0));
    for kind in [
        ShapeKind::Rectangle,
        ShapeKind::Ellipse,
        ShapeKind::Line,
        ShapeKind::Arrow,
    ] {
        first.shapes.push(ShapeSpec::new(kind, 0.0, 0.0, 1.0, 1.0));
    }
    let mut second = PanelSpec::at(1, 2);
    second.series = first.series.clone();
    second.series[1].visible = false;
    let spec = PlotSpec::empty("Titre").with_panels(vec![first, second]);
    let root = project.root();
    let graph = project.add_graph(root, None, spec.clone()).unwrap();
    (project, graph, spec)
}

/// #151 : supprimer une courbe ne touche ni les données partagées ni les
/// autres courbes ; Annuler/Rétablir restaure le graphique et sa projection.
#[test]
fn object_manager_supprime_une_courbe_sans_toucher_aux_donnees() {
    use stokesforge_app_state::object_manager::{object_tree, VisibilityTarget as T};
    use stokesforge_plot::{PanelSpec, PlotSpec, SeriesRef, SeriesSpec};
    let dir = tempfile::tempdir().unwrap();
    let mut project = demo_project();
    let sheet = project
        .tree()
        .iter()
        .find_map(|(id, node)| (node.name() == "Sheet1").then_some(id))
        .unwrap();
    let columns: Vec<_> = project.tree().children(sheet).unwrap().to_vec();
    let mut panel = PanelSpec::default();
    for name in ["Première", "Deuxième"] {
        let mut series = SeriesSpec::new(columns[1]);
        series.x = Some(columns[0]);
        series.name = name.into();
        panel.series.push(series);
    }
    let before = PlotSpec::default().with_panels(panel);
    let graph = project
        .add_graph(project.root(), None, before.clone())
        .unwrap();
    let (_session, mut state) = start(project, &dir.path().join("courbes.stf"));
    let original_nodes: Vec<_> = state
        .tree()
        .iter()
        .filter(|(id, _)| *id != graph)
        .map(|(id, node)| (id, node.clone()))
        .collect();
    state.dispatch(Action::RemovePlotObject {
        graph,
        target: T::Series(SeriesRef::new(0, 0)),
    });
    let mut after = before.clone();
    after.panels.get_mut(0).unwrap().series.remove(0);
    assert_eq!(graph_spec(&state, graph), after);
    let model = object_tree(state.tree(), graph, None).unwrap();
    assert_eq!(model.layers[0].series.len(), 1);
    assert_eq!(model.layers[0].series[0].label, "Deuxième");
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before);
    assert_eq!(
        object_tree(state.tree(), graph, None).unwrap().layers[0]
            .series
            .len(),
        2
    );
    state.dispatch(Action::Redo);
    assert_eq!(graph_spec(&state, graph), after);
    let remaining_nodes: Vec<_> = state
        .tree()
        .iter()
        .filter(|(id, _)| *id != graph)
        .map(|(id, node)| (id, node.clone()))
        .collect();
    assert_eq!(remaining_nodes, original_nodes);
}

/// #197 : les gestes d'un groupe de séries sont des commandes annulables,
/// chacune avec son motif d'historique ; une cible périmée n'en crée aucune.
#[test]
fn les_gestes_d_un_groupe_de_series_sont_annulables_et_persistent() {
    use stokesforge_app_state::object_manager::{object_tree, LayerSlot, VisibilityTarget as T};
    use stokesforge_commands::PlotEditKind as K;
    use stokesforge_plot::{SeriesGroupRef, SeriesRef};
    let dir = tempfile::tempdir().unwrap();
    let (mut project, graph, mut spec) = object_manager_project();
    // Premier calque : A, [B C] — le groupe 1 de « B » et « C ».
    {
        let panel = spec.panels.get_mut(0).unwrap();
        panel.series[1].group = Some(1);
        panel.series[2].group = Some(1);
    }
    project.set_plot_spec(graph, spec.clone()).unwrap();
    let (_session, mut state) = start(project, &dir.path().join("groupes.stf"));
    let group = SeriesGroupRef::new(0, 1);
    let labels = |state: &AppState| state.snapshot().history.undo_label.clone();

    state.dispatch(Action::RenameSeriesGroup {
        graph,
        group,
        name: "  Lots  ".into(),
    });
    let named = graph_spec(&state, graph);
    assert_eq!(named.panels[0].group_label(1), Some("Lots"));
    assert_eq!(
        labels(&state),
        Some(HistoryLabel::EditPlotSpec(K::RenameGroup))
    );
    let model = object_tree(state.tree(), graph, None).unwrap();
    assert!(matches!(
        &model.layers[0].slots[1],
        LayerSlot::Group(row) if row.name == "Lots" && row.members == [1, 2]
    ));

    let revision = state.revision();
    state.dispatch(Action::RenameSeriesGroup {
        graph,
        group,
        name: "Lots".into(),
    });
    state.dispatch(Action::RenameSeriesGroup {
        graph,
        group: SeriesGroupRef::new(0, 9),
        name: "Fantôme".into(),
    });
    state.dispatch(Action::UngroupSeries {
        graph,
        group: SeriesGroupRef::new(0, 9),
    });
    state.dispatch(Action::MoveSeriesGroup {
        graph,
        group,
        up: false,
    });
    assert_eq!(
        state.revision(),
        revision,
        "nom identique, groupe absent, butée : rien"
    );

    // Monter le groupe l'amène avant « A » ; une série seule saute le groupe.
    state.dispatch(Action::MoveSeriesGroup {
        graph,
        group,
        up: true,
    });
    let moved = graph_spec(&state, graph);
    let names = |spec: &stokesforge_plot::PlotSpec| {
        spec.panels[0]
            .series
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&moved), ["B", "C", "A"]);
    assert_eq!(
        labels(&state),
        Some(HistoryLabel::EditPlotSpec(K::ReorderGroup))
    );
    state.dispatch(Action::MoveSeries {
        graph,
        series: SeriesRef::new(0, 2),
        up: true,
    });
    assert_eq!(names(&graph_spec(&state, graph)), ["A", "B", "C"]);
    assert_eq!(
        labels(&state),
        Some(HistoryLabel::EditPlotSpec(K::ReorderSeries))
    );
    state.dispatch(Action::MoveSeries {
        graph,
        series: SeriesRef::new(0, 1),
        up: true,
    });
    assert_eq!(
        names(&graph_spec(&state, graph)),
        ["A", "B", "C"],
        "premier membre d'un groupe : il y reste"
    );

    // Dissoudre garde les séries et leur style ; Annuler rend le groupe et son nom.
    let before = graph_spec(&state, graph);
    state.dispatch(Action::UngroupSeries { graph, group });
    let ungrouped = graph_spec(&state, graph);
    assert_eq!(names(&ungrouped), names(&before));
    assert!(ungrouped.panels[0].series.iter().all(|s| s.group.is_none()));
    assert!(ungrouped.panels[0].group_names.is_empty());
    assert_eq!(
        labels(&state),
        Some(HistoryLabel::EditPlotSpec(K::DissolveGroup))
    );
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before);

    // Supprimer le groupe retire ses séries ; les données ne sont pas touchées.
    state.dispatch(Action::RemovePlotObject {
        graph,
        target: T::SeriesGroup(group),
    });
    assert_eq!(names(&graph_spec(&state, graph)), ["A"]);
    assert_eq!(
        labels(&state),
        Some(HistoryLabel::EditPlotSpec(K::RemoveGroup))
    );
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before);
    state.dispatch(Action::Redo);
    assert_eq!(names(&graph_spec(&state, graph)), ["A"]);
    state.dispatch(Action::Undo);

    // La case du groupe ne commande que ses membres.
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: T::SeriesGroup(group),
        visible: false,
    });
    let hidden = graph_spec(&state, graph);
    assert_eq!(
        hidden.panels[0]
            .series
            .iter()
            .map(|s| s.visible)
            .collect::<Vec<_>>(),
        [true, false, false]
    );
}

#[test]
fn object_manager_series_renommage_ordre_suppression_undo_redo() {
    use stokesforge_app_state::object_manager::VisibilityTarget as T;
    use stokesforge_commands::PlotEditKind as K;
    use stokesforge_plot::SeriesRef;
    let dir = tempfile::tempdir().unwrap();
    let (project, graph, before) = object_manager_project();
    let (_session, mut state) = start(project, &dir.path().join("objets.stf"));
    let source = SeriesRef::new(1, 1); // Série masquée, second calque.
    state.dispatch(Action::SetLegendEntryName {
        graph,
        series: source,
        name: "Référence".into(),
    });
    let renamed = graph_spec(&state, graph);
    assert_eq!(renamed.series_at(source).unwrap().name, "Référence");
    assert!(!renamed.series_at(source).unwrap().visible);
    assert_eq!(
        state.snapshot().history.undo_label,
        Some(HistoryLabel::EditPlotSpec(K::RenameSeries))
    );
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before);
    state.dispatch(Action::Redo);
    assert_eq!(graph_spec(&state, graph), renamed);

    state.dispatch(Action::PlaceSeries {
        graph,
        source,
        group: None,
        position: 2,
    });
    let moved = graph_spec(&state, graph);
    assert_eq!(
        moved.panels[1]
            .series
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["A", "C", "Référence"]
    );
    assert_eq!(moved.panels[0], before.panels[0]);
    assert_eq!(
        state.snapshot().history.undo_label,
        Some(HistoryLabel::EditPlotSpec(K::ReorderSeries))
    );
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), renamed);
    state.dispatch(Action::Redo);
    assert_eq!(graph_spec(&state, graph), moved);

    let revision = state.revision();
    // Déjà à sa place, ou cible périmée : aucune commande.
    state.dispatch(Action::PlaceSeries {
        graph,
        source: SeriesRef::new(1, 2),
        group: None,
        position: 2,
    });
    state.dispatch(Action::PlaceSeries {
        graph,
        source: SeriesRef::new(9, 9),
        group: None,
        position: 0,
    });
    state.dispatch(Action::SetLegendEntryName {
        graph,
        series: SeriesRef::new(9, 9),
        name: "Absent".into(),
    });
    state.dispatch(Action::RemovePlotObject {
        graph,
        target: T::Series(SeriesRef::new(9, 9)),
    });
    assert_eq!(
        state.revision(),
        revision,
        "aucune commande pour une cible périmée ou un geste sans effet"
    );

    state.dispatch(Action::RemovePlotObject {
        graph,
        target: T::Series(SeriesRef::new(1, 2)),
    });
    assert_eq!(graph_spec(&state, graph).panels[1].series.len(), 2);
    assert_eq!(
        state.snapshot().history.undo_label,
        Some(HistoryLabel::EditPlotSpec(K::RemoveSeries))
    );
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), moved);
    state.dispatch(Action::Redo);
    for index in [1, 0] {
        state.dispatch(Action::RemovePlotObject {
            graph,
            target: T::Series(SeriesRef::new(1, index)),
        });
    }
    assert!(graph_spec(&state, graph).panels[1].series.is_empty());
    assert_eq!(
        graph_spec(&state, graph).panels.len(),
        2,
        "la dernière série ne supprime pas le calque"
    );
}

/// Le glisser-déposer de l'Object Manager et de la fenêtre des séries : une
/// commande par geste, annulable, qu'il réordonne, fasse entrer dans un groupe
/// ou en fasse sortir ; un groupe se pose en bloc ; un objet libre se pose dans
/// sa catégorie.
#[test]
fn object_manager_placement_des_series_groupes_et_objets_annulable() {
    use stokesforge_app_state::object_manager::VisibilityTarget as T;
    use stokesforge_commands::PlotEditKind as K;
    use stokesforge_plot::{
        AnnotationRef, AnnotationSpec, PanelSpec, PlotSpec, SeriesGroupRef, SeriesRef, SeriesSpec,
        ShapeKind, ShapeRef, ShapeSpec,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new("Placement").unwrap();
    let mut panel = PanelSpec::at(0, 0);
    for name in ["A", "B", "C", "D"] {
        let mut series = SeriesSpec::new(NodeId::from_raw(900));
        series.name = name.into();
        panel.series.push(series);
    }
    panel.series[1].group = Some(1);
    panel.series[2].group = Some(1);
    for text in ["t1", "t2"] {
        panel.annotations.push(AnnotationSpec::new(text, 0.0, 0.0));
    }
    for kind in [ShapeKind::Line, ShapeKind::Arrow] {
        panel.shapes.push(ShapeSpec::new(kind, 0.0, 0.0, 1.0, 1.0));
    }
    let spec = PlotSpec::empty("Titre").with_panels(vec![panel]);
    let root = project.root();
    let graph = project.add_graph(root, None, spec.clone()).unwrap();
    let (_session, mut state) = start(project, &dir.path().join("placement.stf"));
    let names = |spec: &PlotSpec| {
        spec.panels[0]
            .series
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>()
    };
    let label = |state: &AppState| state.snapshot().history.undo_label.clone();

    // Entrer dans un groupe au rang voulu : A rejoint [B C] entre les deux.
    state.dispatch(Action::PlaceSeries {
        graph,
        source: SeriesRef::new(0, 0),
        group: Some(1),
        position: 1,
    });
    let joined = graph_spec(&state, graph);
    assert_eq!(names(&joined), ["B", "A", "C", "D"]);
    assert_eq!(joined.panels[0].series[1].group, Some(1));
    assert_eq!(
        label(&state),
        Some(HistoryLabel::EditPlotSpec(K::ReorderSeries))
    );
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), spec, "un seul Annuler rend tout");

    // Sortir d'un groupe pour la tête de liste.
    state.dispatch(Action::PlaceSeries {
        graph,
        source: SeriesRef::new(0, 2),
        group: None,
        position: 0,
    });
    let left = graph_spec(&state, graph);
    assert_eq!(names(&left), ["C", "A", "B", "D"]);
    assert_eq!(left.panels[0].series[0].group, None);
    state.dispatch(Action::Undo);

    // Un groupe se pose en bloc après la dernière série seule.
    state.dispatch(Action::PlaceSeriesGroup {
        graph,
        group: SeriesGroupRef::new(0, 1),
        position: 2,
    });
    assert_eq!(names(&graph_spec(&state, graph)), ["A", "D", "B", "C"]);
    assert_eq!(
        label(&state),
        Some(HistoryLabel::EditPlotSpec(K::ReorderGroup))
    );
    state.dispatch(Action::Undo);

    // Un texte se pose parmi les textes, une forme parmi les formes.
    state.dispatch(Action::ReorderObject {
        graph,
        target: T::Annotation(AnnotationRef { panel: 0, index: 1 }),
        index: 0,
    });
    let texts = graph_spec(&state, graph);
    assert_eq!(texts.panels[0].annotations[0].text, "t2");
    assert_eq!(texts.panels[0].shapes, spec.panels[0].shapes);
    assert_eq!(
        label(&state),
        Some(HistoryLabel::EditPlotSpec(K::ReorderObject))
    );
    state.dispatch(Action::ReorderObject {
        graph,
        target: T::Shape(ShapeRef { panel: 0, index: 0 }),
        index: 1,
    });
    assert_eq!(
        graph_spec(&state, graph).panels[0].shapes[0].kind,
        ShapeKind::Arrow
    );

    // Rien à faire : aucune commande.
    let revision = state.revision();
    state.dispatch(Action::PlaceSeries {
        graph,
        source: SeriesRef::new(0, 3),
        group: None,
        position: 3,
    });
    state.dispatch(Action::PlaceSeriesGroup {
        graph,
        group: SeriesGroupRef::new(0, 9),
        position: 0,
    });
    state.dispatch(Action::ReorderObject {
        graph,
        target: T::Series(SeriesRef::new(0, 0)),
        index: 1,
    });
    assert_eq!(state.revision(), revision);
}

#[test]
fn object_manager_proprietes_ouvrent_la_serie_du_bon_calque_et_annulent() {
    use stokesforge_app_state::object_manager::VisibilityTarget as T;
    use stokesforge_plot::SeriesRef;
    let dir = tempfile::tempdir().unwrap();
    let (project, graph, before) = object_manager_project();
    let (_session, mut state) = start(project, &dir.path().join("objets.stf"));
    state.dispatch(Action::OpenObjectProperties {
        graph,
        target: T::Series(SeriesRef::new(1, 1)),
    });
    assert_eq!(state.plot_settings().unwrap().tab, PlotSettingsTab::Series);
    let edit = state.plot_edit_mut().unwrap();
    assert_eq!((edit.selected_panel, edit.selected_series), (1, 1));
    edit.spec.panels.get_mut(1).unwrap().series[1].line.width = 4.0;
    state.commit_plot_edit();
    assert_eq!(
        graph_spec(&state, graph).panels[1].series[1].line.width,
        4.0
    );
    state.cancel_plot_settings();
    assert_eq!(graph_spec(&state, graph), before);
    let revision = state.revision();
    state.dispatch(Action::OpenObjectProperties {
        graph,
        target: T::Series(SeriesRef::new(99, 0)),
    });
    assert!(state.plot_settings().is_none());
    assert_eq!(state.revision(), revision);
}

#[test]
fn object_manager_reglages_persistants_et_reset_restent_hors_historique() {
    use stokesforge_app_state::preferences::{
        ObjectManagerGroup, ObjectManagerSettings, Preferences,
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    let (project, graph, before) = object_manager_project();
    let session = Session::spawn(project, None).unwrap();
    let mut state = AppState::new_with_preferences(
        session.handle(),
        dir.path().join("projet.stf").display().to_string(),
        Preferences::load_from(Some(path.clone())),
    )
    .unwrap();
    let defaults = ObjectManagerSettings::default();
    let revision = state.revision();
    state.dispatch(Action::SetObjectManagerOpen(false));
    for group in ObjectManagerGroup::ALL {
        state.dispatch(Action::SetObjectManagerGroupOpen {
            group,
            open: !defaults.group_open(group),
        });
        assert_eq!(
            state.object_manager_settings().group_open(group),
            !defaults.group_open(group)
        );
    }
    state.dispatch(Action::SetObjectManagerShowScales(true));
    let changed = state.object_manager_settings();
    assert_ne!(
        changed, defaults,
        "témoin : les actions changent les réglages"
    );
    assert_eq!(
        Preferences::load_from(Some(path.clone())).object_manager_settings(),
        changed
    );
    assert!(state.object_manager_show_scales());
    state.dispatch(Action::ResetObjectManagerSettings);
    assert_eq!(state.object_manager_settings(), defaults);
    let loaded = Preferences::load_from(Some(path));
    assert_eq!(loaded.object_manager_settings(), defaults);
    assert!(
        !state.object_manager_open() && !loaded.object_manager_open(),
        "le reset ne rouvre pas le panneau"
    );
    assert_eq!(state.revision(), revision);
    assert!(!state.can_undo() && !state.can_redo());
    assert_eq!(graph_spec(&state, graph), before);
    assert!(state.status().is_none_or(|status| !status.is_error()));
    session.shutdown().unwrap();
}

#[test]
fn object_manager_reglage_reste_actif_si_le_stockage_echoue() {
    use stokesforge_app_state::preferences::{ObjectManagerGroup, Preferences};
    let dir = tempfile::tempdir().unwrap();
    // Un dossier à la place du fichier provoque une erreur portable,
    // sans dépendre des permissions ou de l'identité de l'utilisateur CI.
    let session = Session::spawn(Project::new("Vide").unwrap(), None).unwrap();
    let mut state = AppState::new_with_preferences(
        session.handle(),
        dir.path().join("projet.stf").display().to_string(),
        Preferences::load_from(Some(dir.path().to_path_buf())),
    )
    .unwrap();
    let revision = state.revision();
    state.dispatch(Action::SetObjectManagerGroupOpen {
        group: ObjectManagerGroup::Graph,
        open: false,
    });
    assert!(!state.object_manager_settings().graph_open);
    assert!(state.status().is_some_and(|status| status.is_error()));
    assert_eq!(state.revision(), revision);
    assert!(!state.can_undo());
    session.shutdown().unwrap();
}

#[test]
fn object_manager_textes_lies_jamais_listes_sans_revelation() {
    use stokesforge_app_state::object_manager::{
        object_tree_with_selection, VisibilityTarget as T,
    };
    use stokesforge_plot::{AnnotationRef, ShapeRef, TextTarget};
    let dir = tempfile::tempdir().unwrap();
    let (project, graph, before) = object_manager_project();
    let (_session, mut state) = start(project, &dir.path().join("cache.stf"));
    let text = AnnotationRef { panel: 0, index: 0 };
    let arrow = ShapeRef { panel: 0, index: 3 };
    // #214 : ni titre du graphique, ni titre de calque, ni axes, ni légende ;
    // le texte libre et les quatre formes restent.
    let tree = object_tree_with_selection(state.tree(), graph, None, false, true).unwrap();
    assert_eq!(tree.layers[0].objects.len(), 5);
    assert!(tree.layers[0]
        .objects
        .iter()
        .all(|o| matches!(o.target, T::Annotation(_) | T::Shape(_))));
    assert!(tree.layers[0]
        .objects
        .iter()
        .any(|o| o.target == T::Annotation(text)));
    assert!(tree.layers[0]
        .objects
        .iter()
        .any(|o| o.target == T::Shape(arrow)));
    assert!(!tree.layers[0].series.is_empty(), "séries toujours listées");
    // Pas de révélation à la sélection : un texte lié sélectionné n'apparaît pas.
    let selected = object_tree_with_selection(
        state.tree(),
        graph,
        Some(T::Text(TextTarget::Title)),
        false,
        true,
    )
    .unwrap();
    assert!(selected.layers[0].objects.iter().all(|o| !o.selected));
    // #173 : la case du graphique ne commande que ce que l'arbre affiche ; les
    // textes liés gardent leur état, et ne rendent pas la case indéterminée.
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: T::Graph,
        visible: false,
    });
    let spec = graph_spec(&state, graph);
    assert!(spec.title_visible);
    assert!(spec.panels[0].bottom.title_visible);
    assert!(!T::Graph.visibility(&spec).checked);
    assert!(!T::Graph.visibility(&spec).mixed);
    assert!(!spec.panels[0].series[0].visible);
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before);
}

#[test]
fn object_manager_show_objects_liste_ou_cache_les_objets_libres() {
    use stokesforge_app_state::object_manager::{
        object_tree_with_selection, VisibilityTarget as T,
    };
    let dir = tempfile::tempdir().unwrap();
    let (project, graph, before) = object_manager_project();
    let (_session, mut state) = start(project, &dir.path().join("objets.stf"));
    let listed = object_tree_with_selection(state.tree(), graph, None, false, true).unwrap();
    assert_eq!(listed.layers[0].objects.len(), 5);
    let hidden = object_tree_with_selection(state.tree(), graph, None, false, false).unwrap();
    assert!(hidden.layers[0].objects.is_empty());
    assert!(!hidden.layers[0].series.is_empty());
    // Les objets non listés ne comptent pas dans la case du graphique (#173).
    state.dispatch(Action::SetObjectManagerShowObjects(false));
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: T::Graph,
        visible: false,
    });
    let spec = graph_spec(&state, graph);
    assert!(spec.panels[0].annotations.iter().all(|a| a.visible));
    assert!(!spec.panels[0].series[0].visible);
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before);
}

#[test]
fn object_manager_echelles_cachees_par_defaut_une_case_par_axe() {
    use stokesforge_app_state::object_manager::{
        object_tree_with_selection, AxisRef, ObjectKind, VisibilityTarget as T,
    };
    use stokesforge_plot::AxisSide;
    let dir = tempfile::tempdir().unwrap();
    let (project, graph, before) = object_manager_project();
    let (_session, mut state) = start(project, &dir.path().join("cache.stf"));
    let axes = |tree: &stokesforge_app_state::object_manager::ObjectTree| -> Vec<T> {
        tree.layers[0]
            .axes
            .iter()
            .filter(|o| matches!(o.kind, ObjectKind::Axis))
            .map(|o| o.target)
            .collect()
    };
    // Cachées par défaut, même textes affichés.
    let hidden = object_tree_with_selection(state.tree(), graph, None, false, true).unwrap();
    assert!(axes(&hidden).is_empty());
    // Bascule active : le groupe « Axes » et ses quatre lignes, dont le haut et
    // la droite, axes miroirs (#176) ; les autres objets du calque n'en
    // contiennent aucun.
    let shown = object_tree_with_selection(state.tree(), graph, None, true, true).unwrap();
    let axis = |side| T::Axis(AxisRef { panel: 0, side });
    assert_eq!(
        axes(&shown),
        [
            AxisSide::Bottom,
            AxisSide::Left,
            AxisSide::Right,
            AxisSide::Top
        ]
        .map(axis)
    );
    assert!(shown.layers[0]
        .objects
        .iter()
        .all(|o| !matches!(o.kind, ObjectKind::Axis)));
    // Une case par axe : `AxisSpec::visible`, une seule entrée d'historique.
    let left = axis(AxisSide::Left);
    assert!(left.visibility(&before).checked);
    // Les miroirs du haut et de la droite sont décochés d'office (#176) : le
    // groupe est mixte dès l'ouverture.
    assert!(T::Axes(0).visibility(&before).mixed);
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: left,
        visible: false,
    });
    let spec = graph_spec(&state, graph);
    assert!(!spec.panels[0].left.visible);
    assert!(!left.visibility(&spec).checked);
    // La case du groupe est mixte dès qu'un de ses axes est masqué, et les
    // commande tous en une écriture.
    assert!(T::Axes(0).visibility(&spec).mixed);
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: T::Axes(0),
        visible: true,
    });
    assert!(T::Axes(0).visibility(&graph_spec(&state, graph)).checked);
    state.dispatch(Action::Undo);
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before);
    // Les cases du graphique et du calque ne touchent pas aux axes.
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: T::Graph,
        visible: true,
    });
    let spec = graph_spec(&state, graph);
    for side in [AxisSide::Bottom, AxisSide::Right, AxisSide::Top] {
        assert_eq!(
            spec.panels[0].axis(side).visible,
            before.panels[0].axis(side).visible
        );
    }
}

#[test]
fn object_manager_arbre_objets_libres_visibilite_et_fichier() {
    use stokesforge_app_state::object_manager::{
        object_tree_with_selection, VisibilityTarget as T,
    };
    use stokesforge_plot::{AnnotationRef, AxisSide, ShapeRef, TextTarget};
    let dir = tempfile::tempdir().unwrap();
    let (project, graph, before) = object_manager_project();
    let (_session, mut state) = start(project, &dir.path().join("objets.stf"));
    let text = AnnotationRef { panel: 0, index: 0 };
    let arrow = ShapeRef { panel: 0, index: 3 };
    let model = object_tree_with_selection(state.tree(), graph, Some(T::Shape(arrow)), false, true)
        .unwrap();
    assert_eq!(
        model.layers[0].objects.len(),
        5,
        "texte libre et quatre formes, aucun texte lié (#214)"
    );
    assert_eq!(
        model.layers[0]
            .objects
            .iter()
            .filter(|o| o.selected)
            .count(),
        1
    );
    assert!(model.layers[0]
        .objects
        .iter()
        .any(|o| o.target == T::Shape(arrow) && o.selected));
    let target = TextTarget::AxisTitle {
        panel: 0,
        side: AxisSide::Bottom,
    };
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: T::Text(target),
        visible: false,
    });
    let hidden_title = graph_spec(&state, graph);
    assert_eq!(target.text(&hidden_title).as_deref(), Some("Temps (s)"));
    assert_eq!(target.visible(&hidden_title), Some(false));
    assert!(
        !T::Panel(0).visibility(&hidden_title).mixed,
        "un titre lié n'est pas listé : il ne compte pas dans la case du calque"
    );
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before);

    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: T::Graph,
        visible: false,
    });
    let hidden = graph_spec(&state, graph);
    assert!(
        hidden.title_visible && hidden.panels[0].bottom.title_visible,
        "les textes liés non listés gardent leur état (#214)"
    );
    assert!(hidden.panels[0].bottom.visible);
    assert!(!text.get(&hidden).unwrap().visible);
    assert!(!arrow.get(&hidden).unwrap().visible);
    assert!(!T::Graph.visibility(&hidden).checked);
    assert!(!T::Graph.visibility(&hidden).mixed);
    let model =
        object_tree_with_selection(state.tree(), graph, Some(T::Annotation(text)), false, true)
            .unwrap();
    assert!(model.layers[0]
        .objects
        .iter()
        .any(|o| o.target == T::Annotation(text) && o.selected));
    state.dispatch(Action::Save);
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: T::Graph,
        visible: true,
    });
    state.dispatch(Action::Load);
    assert_eq!(graph_spec(&state, graph), hidden);

    for target in [
        T::Annotation(text),
        T::Shape(arrow),
        T::Text(TextTarget::Title),
    ] {
        let before = graph_spec(&state, graph);
        state.dispatch(Action::RemovePlotObject { graph, target });
        assert_ne!(graph_spec(&state, graph), before);
        state.dispatch(Action::Undo);
        assert_eq!(graph_spec(&state, graph), before);
        state.dispatch(Action::Redo);
        assert_ne!(graph_spec(&state, graph), before);
        state.dispatch(Action::Undo);
    }
}

// -- Feuilles : renommer, dupliquer, supprimer (#106) --

/// Renommer une feuille : une seule entrée d'historique, annulable ; nom vide
/// ou déjà pris dans le classeur → l'ancien nom reste, message d'erreur, rien
/// dans l'historique.
#[test]
fn renaming_a_sheet_is_undoable_and_rejects_empty_or_taken_names() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    state.dispatch(Action::AddWorksheet { book });
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let other = state.worksheets_of(book).last().unwrap().clone();

    state.dispatch(Action::BeginRenameSheet(sheet1));
    assert_eq!(state.renaming_sheet(), Some(sheet1));
    state.dispatch(Action::CommitRenameSheet {
        sheet: sheet1,
        name: "Mesures".to_string(),
    });
    assert_eq!(state.renaming_sheet(), None);
    assert_eq!(state.tree().node(sheet1).unwrap().name(), "Mesures");
    state.dispatch(Action::Undo);
    assert_eq!(state.tree().node(sheet1).unwrap().name(), "Sheet1");

    let revision = state.revision();
    for name in ["   ", other.1.as_str()] {
        state.dispatch(Action::BeginRenameSheet(sheet1));
        state.dispatch(Action::CommitRenameSheet {
            sheet: sheet1,
            name: name.to_string(),
        });
        assert_eq!(state.tree().node(sheet1).unwrap().name(), "Sheet1");
        assert!(state.status().is_some_and(|status| status.is_error()));
        assert_eq!(state.revision(), revision, "aucune entrée d'historique");
    }

    state.dispatch(Action::BeginRenameSheet(sheet1));
    state.dispatch(Action::CancelRenameSheet);
    assert_eq!(state.renaming_sheet(), None);
}

/// Dupliquer : « … copie 1 », « … copie 2 » (premier numéro libre), colonnes
/// aux identifiants neufs et mêmes valeurs, copie affichée ; les graphes de la
/// mère la tracent toujours ; un Ctrl+Z retire la copie.
#[test]
fn duplicating_a_sheet_copies_its_columns_and_keeps_graphs_on_the_original() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![temps, signal],
        plot_type: PlotType::Line,
    });
    let before = std::sync::Arc::clone(state.tree());
    let count = state.worksheets_of(book).len();

    state.dispatch(Action::DuplicateSheet(sheet1));
    state.dispatch(Action::DuplicateSheet(sheet1));

    let sheets = state.worksheets_of(book);
    assert_eq!(sheets.len(), count + 2);
    let names: Vec<&str> = sheets.iter().map(|(_, n)| n.as_str()).collect();
    assert!(names.contains(&"Sheet1 copy 1"), "{names:?}");
    assert!(names.contains(&"Sheet1 copy 2"), "{names:?}");
    let copy2 = sheet_named(state.tree(), "Sheet1 copy 2");
    assert_eq!(state.selected_sheet_in(book), Some(copy2));
    let copy1 = sheet_named(state.tree(), "Sheet1 copy 1");
    let original: Vec<_> = state.tree().node(sheet1).unwrap().children().to_vec();
    let copied: Vec<_> = state.tree().node(copy1).unwrap().children().to_vec();
    assert_eq!(original.len(), copied.len());
    for (a, b) in original.iter().zip(&copied) {
        assert_ne!(a, b, "identifiants neufs");
        assert_eq!(
            state.tree().node(*a).unwrap().name(),
            state.tree().node(*b).unwrap().name()
        );
        assert_eq!(
            state.tree().node(*a).unwrap().as_column(),
            state.tree().node(*b).unwrap().as_column()
        );
    }
    assert_eq!(state.sheet_users(copy1), Vec::<String>::new());
    assert_eq!(state.sheet_users(sheet1).len(), 1);

    state.dispatch(Action::Undo);
    state.dispatch(Action::Undo);
    assert_eq!(**state.tree(), *before);
}

/// Supprimer une feuille ne demande rien (S-F3, #229) : le message nomme les
/// graphes qui la traçaient, Undo la rend ; la dernière feuille d'un classeur
/// est refusée.
#[test]
fn deleting_a_sheet_needs_no_confirmation_and_keeps_the_last_sheet() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![temps, signal],
        plot_type: PlotType::Line,
    });
    let only = state.worksheets_of(book);
    if only.len() == 1 {
        assert!(!state.can_delete_sheet(sheet1));
        state.dispatch(Action::DeleteSheet(sheet1));
        assert!(state.tree().node(sheet1).is_some());
        assert!(state.status().is_some_and(|status| status.is_error()));
    }
    state.dispatch(Action::AddWorksheet { book });
    let before = std::sync::Arc::clone(state.tree());

    assert!(state.can_delete_sheet(sheet1));
    let users = state.sheet_users(sheet1);
    assert_eq!(users.len(), 1);
    state.dispatch(Action::DeleteSheet(sheet1));
    assert!(state.tree().node(sheet1).is_none());
    assert!(!state.blocking_dialog());
    let status = state.status().expect("message");
    assert!(!status.is_error());
    assert!(status.text.contains(&users[0]), "{}", status.text);
    state.dispatch(Action::Undo);
    assert_eq!(**state.tree(), *before);
}

// -- Issue #149 : copier la figure vers une application externe --

/// La copie prépare SVG, PNG puis PDF de la figure — dans cet ordre, celui de
/// la préférence — sans toucher au modèle : ni le `PlotSpec` ni la révision ne
/// bougent, donc rien à annuler. Le PNG suit l'agrandissement réglé pour les
/// exports, et la représentation n'est relevée qu'une fois.
#[test]
fn copier_la_figure_prepare_svg_png_pdf_sans_toucher_au_modele() {
    use stokesforge_render_svg::ExportFormat;
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let avant = (graph_spec(&state, graph), state.snapshot().revision);
    state.dispatch(Action::SetPngExportScale(2.0));

    state.dispatch(Action::CopyGraphFigure {
        spec: Box::new(avant.0.clone()),
    });
    let status = state.status().expect("un message");
    assert!(!status.is_error(), "{}", status.text);
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("status-graph-copied", formats = "SVG, PNG, PDF")
    );
    let reps = state.take_figure_clipboard().expect("la figure est prête");
    assert!(matches!(
        reps.iter().map(|r| r.format).collect::<Vec<_>>()[..],
        [
            ExportFormat::Svg { .. },
            ExportFormat::Png { .. },
            ExportFormat::Pdf
        ]
    ));
    assert!(String::from_utf8_lossy(&reps[0].bytes).contains("<svg"));
    assert!(reps[1].bytes.starts_with(b"\x89PNG"));
    // Témoin du réglage : le PNG est à l'échelle 2 (480 × 2).
    assert_eq!(
        u32::from_be_bytes([
            reps[1].bytes[16],
            reps[1].bytes[17],
            reps[1].bytes[18],
            reps[1].bytes[19]
        ]),
        960
    );
    assert!(reps[2].bytes.starts_with(b"%PDF"));
    assert!(state.take_figure_clipboard().is_none(), "relevée une fois");

    assert_eq!(graph_spec(&state, graph), avant.0, "le modèle a changé");
    assert_eq!(
        state.snapshot().revision,
        avant.1,
        "une commande a été émise"
    );
    assert!(!state.can_undo(), "aucune entrée d'historique");
}

/// Preferences › Export règle la copie : l'ordre des représentations est celui
/// de la liste, un format décoché n'est pas offert, et le message ne nomme que
/// ceux qui le sont.
#[test]
fn copier_la_figure_suit_l_ordre_et_le_choix_des_preferences() {
    use stokesforge_app_state::preferences::{CopyFormat, CopyFormatEntry};
    use stokesforge_render_svg::ExportFormat;
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    state.dispatch(Action::SetCopyFormats(vec![
        CopyFormatEntry {
            format: CopyFormat::Pdf,
            on: true,
        },
        CopyFormatEntry {
            format: CopyFormat::Svg,
            on: false,
        },
        CopyFormatEntry {
            format: CopyFormat::Png,
            on: true,
        },
    ]));
    state.dispatch(Action::CopyGraphFigure {
        spec: Box::new(graph_spec(&state, graph)),
    });
    let status = state.status().expect("un message");
    assert!(!status.is_error(), "{}", status.text);
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("status-graph-copied", formats = "PDF, PNG")
    );
    let reps = state.take_figure_clipboard().expect("la figure est prête");
    assert!(matches!(
        reps.iter().map(|r| r.format).collect::<Vec<_>>()[..],
        [ExportFormat::Pdf, ExportFormat::Png { .. }]
    ));
}

/// La figure copiée est celle du spec cadré qu'on donne : un zoom change les
/// octets (même témoin que l'export vers un fichier).
#[test]
fn copier_la_figure_zoomee_copie_ce_qu_on_voit() {
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let entier = graph_spec(&state, graph);
    let mut zoome = entier.clone();
    if let Some(panel) = zoome.panels.get_mut(0) {
        panel.bottom.auto_range = false;
        panel.bottom.range_min = Some(0.5);
        panel.bottom.range_max = Some(1.0);
    }
    let svg = |state: &mut AppState, spec: &stokesforge_plot::PlotSpec| {
        state.dispatch(Action::CopyGraphFigure {
            spec: Box::new(spec.clone()),
        });
        state.take_figure_clipboard().expect("prête")[0]
            .bytes
            .clone()
    };
    assert_ne!(svg(&mut state, &zoome), svg(&mut state, &entier));
}

/// Un refus du système remplace le message « copié » par sa cause. La phrase
/// est mise par la fenêtre (`clipboard_error_message`, testée dans
/// `app/figure_copy.rs`) ; l'état la montre telle quelle, en erreur.
#[test]
fn un_refus_du_presse_papiers_remplace_le_message_copie() {
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    state.dispatch(Action::CopyGraphFigure {
        spec: Box::new(graph_spec(&state, graph)),
    });
    assert!(!state.status().expect("message").is_error());
    // La phrase vient de la fenêtre (`clipboard_error_message`, testée dans
    // `app/figure_copy.rs`) : l'état ne fait que l'afficher en erreur.
    state.figure_clipboard_failed("Could not copy the graph: selection refused".into());
    let status = state.status().expect("message");
    assert!(status.is_error());
    assert_eq!(status.text, "Could not copy the graph: selection refused");
}

/// #153 : un redimensionnement passe par la même action que l'insertion ;
/// un seul annuler/rétablir restaure les dimensions et tout le style.
#[test]
fn le_redimensionnement_d_une_forme_est_annulable_en_un_pas() {
    use stokesforge_plot::{ShapeKind, ShapeRef, ShapeSpec};
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let target = ShapeRef { panel: 0, index: 0 };
    for kind in [
        ShapeKind::Rectangle,
        ShapeKind::Ellipse,
        ShapeKind::Line,
        ShapeKind::Arrow,
    ] {
        let original = ShapeSpec::new(kind, 1.0, 2.0, 5.0, 4.0);
        state.dispatch(Action::SetShape {
            graph,
            target,
            shape: Box::new(original.clone()),
        });
        let resized = ShapeSpec {
            x2: 9.0,
            y2: 6.0,
            ..original.clone()
        };
        state.dispatch(Action::SetShape {
            graph,
            target,
            shape: Box::new(resized.clone()),
        });
        assert_eq!(target.get(&graph_spec(&state, graph)), Some(&resized));
        state.dispatch(Action::Undo);
        assert_eq!(target.get(&graph_spec(&state, graph)), Some(&original));
        state.dispatch(Action::Redo);
        assert_eq!(target.get(&graph_spec(&state, graph)), Some(&resized));
    }
}

// -- 20. Sélection multiple de colonnes : Ctrl+clic, Maj+clic (#159) --

/// Le Ctrl+clic ajoute puis retire une colonne, une par une, sans exiger
/// qu'elle touche les autres ; l'ordre des actions est celui de la feuille.
#[test]
fn ctrl_click_toggles_non_contiguous_columns() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_column(sheet1, note);
    state.toggle_column_selection(sheet1, temps);

    assert!(state.is_column_selected(sheet1, temps));
    assert!(!state.is_column_selected(sheet1, signal));
    assert!(state.is_column_selected(sheet1, note));
    assert_eq!(
        state
            .selection(sheet1)
            .unwrap()
            .columns(state.tree(), sheet1),
        vec![temps, note],
        "l'ordre de la feuille, pas celui des clics"
    );

    state.toggle_column_selection(sheet1, note);
    assert!(!state.is_column_selected(sheet1, note));
    assert!(state.is_column_selected(sheet1, temps));
}

/// Maj+clic remplace tout par la plage ancre → cible ; Ctrl+Maj+clic
/// (`extend_column_selection`) l'ajoute aux colonnes déjà posées.
#[test]
fn shift_click_replaces_and_ctrl_shift_click_adds_a_range() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let columns: Vec<NodeId> = state
        .visible_columns(sheet1)
        .into_iter()
        .map(|(id, _, _)| id)
        .collect();
    assert!(columns.len() >= 3);
    let (first, last) = (columns[0], columns[columns.len() - 1]);

    state.select_column(sheet1, first);
    state.toggle_column_selection(sheet1, last);
    // Maj+clic : la plage ancre (`last`) → `first` remplace les deux.
    state.extend_column_selection_replacing(sheet1, first);
    for column in &columns {
        assert!(state.is_column_selected(sheet1, *column));
    }

    state.select_column(sheet1, first);
    state.toggle_column_selection(sheet1, last);
    // Ctrl+Maj+clic : ancre `last` → colonne suivante, les anciennes restent.
    state.extend_column_selection(sheet1, columns[columns.len() - 2]);
    assert!(state.is_column_selected(sheet1, first));
    assert!(state.is_column_selected(sheet1, columns[columns.len() - 2]));
    assert!(state.is_column_selected(sheet1, last));
}

/// Une sélection non contiguë grise Paste et Paste in Place (cible
/// ambiguë) et le collage au clavier est refusé avec un message ; une
/// sélection contiguë garde les deux.
#[test]
fn a_non_contiguous_selection_cannot_be_pasted_into() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_column(sheet1, temps);
    state.toggle_column_selection(sheet1, signal);
    assert!(state.grid_menu(sheet1).unwrap().paste_enabled);

    state.select_column(sheet1, temps);
    state.toggle_column_selection(sheet1, note);
    assert!(!state.grid_menu(sheet1).unwrap().paste_enabled);

    let before = cell(&state, temps, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "999".to_string(),
    });
    assert_eq!(cell(&state, temps, 0), before, "rien n'est collé");
}

/// Copier une sélection non contiguë met les colonnes côte à côte, dans
/// l'ordre de la feuille.
#[test]
fn copying_non_contiguous_columns_joins_them_in_sheet_order() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_column(sheet1, note);
    state.toggle_column_selection(sheet1, temps);
    let text = state.copy_selection(sheet1).expect("sélection non vide");

    state.select_column(sheet1, temps);
    state.toggle_column_selection(sheet1, signal);
    state.toggle_column_selection(sheet1, note);
    let all = state.copy_selection(sheet1).expect("sélection non vide");

    let first_line = text.lines().next().unwrap();
    assert_eq!(
        first_line.split('\t').count(),
        2,
        "deux colonnes : {text:?}"
    );
    assert_eq!(all.lines().next().unwrap().split('\t').count(), 3);
    assert_eq!(
        first_line.split('\t').next(),
        all.lines().next().unwrap().split('\t').next(),
        "Temps d'abord dans les deux copies"
    );
}

/// Une colonne retirée du modèle sort de la sélection multiple.
#[test]
fn toggling_a_column_off_keeps_the_others() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_column(sheet1, temps);
    state.toggle_column_selection(sheet1, signal);
    state.toggle_column_selection(sheet1, temps);

    assert!(!state.is_column_selected(sheet1, temps));
    assert!(state.is_column_selected(sheet1, signal));
    // Ctrl+clic sur une cellule d'une autre sorte : une sélection simple.
    state.select_cell(sheet1, temps, 0);
    state.toggle_column_selection(sheet1, signal);
    assert!(state.is_column_selected(sheet1, signal));
    assert!(!state.is_column_selected(sheet1, temps));
}

// -- 21. Sélection multiple de lignes et de cellules (#159) --

/// Retirer une cellule du milieu d'un rectangle le découpe autour d'elle ;
/// la remettre la rajoute. Les cases voisines restent sélectionnées.
#[test]
fn toggling_a_cell_out_of_a_rectangle_splits_it() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, temps, 0);
    state.extend_selection(sheet1, signal, 2);
    state.toggle_cell_selection(sheet1, temps, 1);

    assert!(!state.is_selected(sheet1, temps, 1));
    for (column, row) in [
        (temps, 0),
        (temps, 2),
        (signal, 0),
        (signal, 1),
        (signal, 2),
    ] {
        assert!(state.is_selected(sheet1, column, row), "{row}");
    }
    state.toggle_cell_selection(sheet1, temps, 1);
    assert!(state.is_selected(sheet1, temps, 1));
}

/// Une seule sorte à la fois : un Ctrl+clic sur un champ d'en-tête quand des
/// cellules de données sont sélectionnées repose une sélection simple.
#[test]
fn ctrl_click_never_mixes_header_and_data() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_cell(sheet1, temps, 0);
    state.toggle_header_field_selection(sheet1, temps, HeaderField::Unit);

    assert!(!state.is_selected(sheet1, temps, 0));
    assert!(state.is_header_field_selected(sheet1, temps, HeaderField::Unit));
}

/// Deux cellules qui partagent leur ligne se copient côte à côte ; deux
/// cellules qui ne partagent ni ligne ni colonne ne se copient pas (Copy et
/// Cut grisés).
#[test]
fn copying_multiple_cells_needs_shared_rows_or_columns() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, temps, 1);
    state.toggle_cell_selection(sheet1, signal, 1);
    assert!(state.grid_menu(sheet1).unwrap().copy_enabled);
    let side_by_side = state.copy_selection(sheet1).expect("même ligne");
    assert_eq!(side_by_side.lines().count(), 1);
    assert_eq!(side_by_side.lines().next().unwrap().split('\t').count(), 2);

    // Même colonne, lignes 0 et 2 : empilées, sans la ligne 1.
    state.select_cell(sheet1, temps, 0);
    state.toggle_cell_selection(sheet1, temps, 2);
    let stacked = state.copy_selection(sheet1).expect("même colonne");
    assert_eq!(stacked.lines().count(), 2);

    state.select_cell(sheet1, temps, 0);
    state.toggle_cell_selection(sheet1, signal, 2);
    assert!(!state.grid_menu(sheet1).unwrap().copy_enabled);
    assert_eq!(state.copy_selection(sheet1), None);
}

/// Effacer des lignes non contiguës vide chacune ; Suppr ne décale pas (des
/// trous rendraient la remontée ambiguë) et se contente de vider.
#[test]
fn clearing_non_contiguous_rows_empties_each_without_shifting() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let rows = state.row_count(sheet1);
    assert!(rows >= 4);
    let kept = cell(&state, temps, 1);

    state.select_row(sheet1, 0);
    state.toggle_row_selection(sheet1, 2);
    assert!(state.is_row_selected(sheet1, 0));
    assert!(!state.is_row_selected(sheet1, 1));
    assert!(state.is_row_selected(sheet1, 2));
    assert!(!state.grid_menu(sheet1).unwrap().offers_delete_shift);

    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });

    assert_eq!(state.row_count(sheet1), rows, "rien n'a remonté");
    assert_eq!(cell(&state, temps, 1), kept, "la ligne 1 est intacte");
    assert_ne!(cell(&state, temps, 0), kept);
    state.dispatch(Action::Undo);
}

/// Ctrl+clic qui retire la dernière ligne ou colonne ne laisse pas de
/// sélection vide.
#[test]
fn toggling_off_the_last_item_clears_the_selection() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");

    state.select_column(sheet1, temps);
    state.toggle_column_selection(sheet1, temps);
    assert!(state.selection(sheet1).is_none());

    state.select_row(sheet1, 1);
    state.toggle_row_selection(sheet1, 1);
    assert!(state.selection(sheet1).is_none());
}

/// Type et rôle de plusieurs colonnes forment **un seul** pas d'annulation
/// (#159) ; sur une seule colonne, pas de groupe.
#[test]
fn changing_type_and_role_of_several_columns_is_one_undo_step() {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let temps = column_id(&state, sheet1, "Temps");
    let signal = column_id(&state, sheet1, "Signal");
    let role_before = (
        state.column_meta(temps).role,
        state.column_meta(signal).role,
    );
    let type_before = (
        state
            .tree()
            .node(temps)
            .unwrap()
            .as_column()
            .unwrap()
            .column_type(),
        state
            .tree()
            .node(signal)
            .unwrap()
            .as_column()
            .unwrap()
            .column_type(),
    );

    state.dispatch(Action::SetColumnsRole {
        columns: vec![temps, signal],
        role: ColumnRole::Z,
    });
    assert_eq!(state.column_meta(temps).role, ColumnRole::Z);
    assert_eq!(state.column_meta(signal).role, ColumnRole::Z);

    state.dispatch(Action::Undo);
    assert_eq!(
        (
            state.column_meta(temps).role,
            state.column_meta(signal).role
        ),
        role_before,
        "un seul Annuler rend les deux rôles"
    );

    state.dispatch(Action::SetColumnsType {
        columns: vec![temps, signal],
        column_type: ColumnType::Mixed,
    });
    state.dispatch(Action::Undo);
    let type_after = (
        state
            .tree()
            .node(temps)
            .unwrap()
            .as_column()
            .unwrap()
            .column_type(),
        state
            .tree()
            .node(signal)
            .unwrap()
            .as_column()
            .unwrap()
            .column_type(),
    );
    assert_eq!(
        type_after, type_before,
        "un seul Annuler rend les deux types"
    );
}

/// S-F8 (#228) : la géométrie d'un dialogue fermé s'écrit dans les préférences,
/// hors historique et hors projet, et se relit à la session suivante.
#[test]
fn la_geometrie_d_un_dialogue_ferme_va_dans_les_preferences() {
    use stokesforge_app_state::preferences::{DialogGeometry, Preferences};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    let session = Session::spawn(Project::new("Vide").unwrap(), None).unwrap();
    let mut state = AppState::new_with_preferences(
        session.handle(),
        dir.path().join("projet.stf").display().to_string(),
        Preferences::load_from(Some(path.clone())),
    )
    .unwrap();
    let revision = state.revision();
    let geometry = DialogGeometry {
        x: 12.0,
        y: 34.0,
        width: 560.0,
        height: 420.0,
    };
    state.dispatch(Action::RememberDialogGeometry {
        kind: "import".to_string(),
        geometry,
    });
    assert_eq!(state.dialog_geometries().get("import"), Some(&geometry));
    assert_eq!(
        Preferences::load_from(Some(path))
            .dialog_geometries()
            .get("import"),
        Some(&geometry)
    );
    assert_eq!(state.revision(), revision, "ni commande ni historique");
    assert!(!state.can_undo());
    session.shutdown().unwrap();
}

/// S-M7 (#228) : hors de tout dialogue, une erreur reste dans la barre d'état ;
/// un dialogue ouvert la prend dans son pied, et elle ne lui survit pas.
#[test]
fn une_erreur_va_dans_le_pied_du_dialogue_ouvert_seulement() {
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    state.import_state_mut().unwrap().save_as_name = String::new();
    state.save_import_recipe();
    assert!(state.dialog_error().is_some());
    assert!(!state.status().is_some_and(|status| status.is_error()));
    state.cancel_import();
    assert!(state.dialog_error().is_none(), "l'erreur ne survit pas");

    // Témoin : sans dialogue, une erreur (préférences impossibles à écrire :
    // un dossier à la place du fichier) va dans la barre d'état.
    use stokesforge_app_state::preferences::Preferences;
    let session = Session::spawn(Project::new("Vide").unwrap(), None).unwrap();
    let mut state = AppState::new_with_preferences(
        session.handle(),
        dir.path().join("projet.stf").display().to_string(),
        Preferences::load_from(Some(dir.path().to_path_buf())),
    )
    .unwrap();
    state.dispatch(Action::SetSvgTextMode(
        stokesforge_render_svg::SvgTextMode::Outlines,
    ));
    assert!(state.dialog_error().is_none());
    assert!(
        state.status().is_some_and(|status| status.is_error()),
        "hors dialogue, la barre d'état"
    );
    session.shutdown().unwrap();
}

// -- Barre d'état : durée, historique, titre (#230) --

#[test]
fn une_information_s_efface_apres_huit_secondes_pas_un_avertissement() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.dispatch(Action::Save);
    let at = state.status().expect("enregistré").at;
    assert_eq!(state.status().unwrap().level, StatusLevel::Info);

    let before = at + INFO_LIFETIME - std::time::Duration::from_secs(3);
    assert_eq!(
        state.expire_status(before),
        Some(std::time::Duration::from_secs(3)),
        "la prochaine échéance, pour ne programmer que ce redessin"
    );
    assert!(state.status().is_some());
    assert_eq!(state.expire_status(at + INFO_LIFETIME), None);
    assert!(state.status().is_none(), "l'information s'est effacée");

    // Un avertissement reste, quel que soit l'âge, et ne programme rien.
    state.dispatch(Action::Undo);
    state.dispatch(Action::Undo);
    let at = state.status().expect("avertissement").at;
    assert_eq!(state.status().unwrap().level, StatusLevel::Warning);
    assert_eq!(state.expire_status(at + INFO_LIFETIME * 10), None);
    assert!(state.status().is_some());
}

#[test]
fn l_historique_garde_les_cinquante_derniers_messages_dans_l_ordre() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    assert_eq!(state.status_history().count(), 0);
    for _ in 0..STATUS_HISTORY_LEN + 10 {
        state.dispatch(Action::Undo);
    }
    assert_eq!(state.status_history().count(), STATUS_HISTORY_LEN);
    let times: Vec<_> = state.status_history().map(|m| m.wall).collect();
    assert!(times.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(!state.status_history_open());
    state.dispatch(Action::OpenStatusHistory);
    assert!(state.status_history_open());
    state.dispatch(Action::CloseStatusHistory);
    assert!(!state.status_history_open());
}

#[test]
fn le_titre_porte_le_nom_du_projet_et_un_point_tant_qu_il_n_est_pas_enregistre() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.dispatch(Action::Save);
    let saved = state.window_title();
    assert!(saved.contains("projet.stf"), "{saved}");
    assert!(!saved.contains('•'), "{saved}");
    state.dispatch(Action::AddGraph);
    let dirty = state.window_title();
    assert!(dirty.contains("projet.stf •"), "{dirty}");
    state.dispatch(Action::Save);
    assert_eq!(state.window_title(), saved);
}

/// #309 : le rapport de la lecture du démarrage est dit comme celui du menu
/// Fichier ; un rapport vide ne dit rien de plus.
#[test]
fn mark_loaded_avertit_des_calculs_ecartes_du_rapport() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(demo_project(), &dir.path().join("p.stf"));
    state.mark_loaded(&stokesforge_session::LoadReport::default());
    assert!(
        state
            .status()
            .is_none_or(|s| s.level != stokesforge_app_state::state::StatusLevel::Warning),
        "rapport vide : pas d'avertissement"
    );
    let report = stokesforge_session::LoadReport {
        discarded_calculations: vec![stokesforge_session::DiscardedCalculation {
            id: 42,
            kind: Some("fit".into()),
        }],
        ..Default::default()
    };
    state.mark_loaded(&report);
    let status = state.status().expect("un avertissement");
    assert_eq!(
        status.level,
        stokesforge_app_state::state::StatusLevel::Warning
    );
    assert!(status.text.contains("#42"), "{}", status.text);
}

fn discarded(ids: &[u64]) -> Vec<stokesforge_session::DiscardedCalculation> {
    ids.iter()
        .map(|&id| stokesforge_session::DiscardedCalculation { id, kind: None })
        .collect()
}

/// #309 (F1) : des traces de correction seules sont dites (message compté) ;
/// calculs et traces forment **un seul** message qui compte les deux.
#[test]
fn le_rapport_compte_les_traces_et_les_calculs_dans_un_seul_message() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(demo_project(), &dir.path().join("p.stf"));
    state.mark_loaded(&stokesforge_session::LoadReport {
        discarded_corrections: discarded(&[7]),
        ..Default::default()
    });
    let status = state.status().expect("un avertissement");
    assert_eq!(
        status.level,
        stokesforge_app_state::state::StatusLevel::Warning
    );
    assert_eq!(
        status.text,
        "1 correction record was discarded from the project (#7)"
    );
    state.mark_loaded(&stokesforge_session::LoadReport {
        discarded_calculations: discarded(&[3, 4]),
        discarded_corrections: discarded(&[7]),
    });
    assert_eq!(
        state.status().unwrap().text,
        "2 calculations and 1 correction record were discarded from the project (#3, #4, #7)"
    );
}

/// Un `.stf` à un calcul de correction que la lecture écartera (octets
/// abîmés), à `path`.
fn write_file_with_a_discarded_calculation(path: &Path) {
    use stokesforge_analysis::correct::CorrectOp;
    use stokesforge_analysis::ops::Operation;
    use stokesforge_core::{CalculationDraft, OutputDraft, SeriesRef, SeriesResult};
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), None).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let y = project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&[1.0, 2.0]))
        .unwrap();
    project
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
    stokesforge_persistence::save(&project, path).unwrap();
    let mut bytes = std::fs::read(path).unwrap();
    let at = bytes
        .windows(8)
        .position(|w| w == b"subtract")
        .expect("l'opération est dans le fichier");
    bytes[at..at + 8].copy_from_slice(b"subtraXX");
    std::fs::write(path, bytes).unwrap();
}

/// Une session sur un fichier abîmé, chargé par `Action::Load`.
fn start_on_a_discarding_file(dir: &Path) -> (Session, AppState, std::path::PathBuf) {
    let file = dir.join("abime.stf");
    write_file_with_a_discarded_calculation(&file);
    let (session, mut state) = start(Project::new("Vide").unwrap(), &file);
    state.dispatch(Action::Load);
    assert!(state.alert().is_none(), "le projet se charge");
    (session, state, file)
}

/// #309 (F3) : enregistrer sur le fichier source d'une lecture qui a écarté
/// des données demande d'abord ; accepter écrit, et l'enregistrement suivant
/// ne demande plus rien.
#[test]
fn enregistrer_sur_la_source_avec_ecarts_demande_puis_ecrit() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state, file) = start_on_a_discarding_file(dir.path());
    let before = std::fs::read(&file).unwrap();

    state.dispatch(Action::Save);
    let (name, what) = state.pending_discarded_save().expect("une question");
    assert_eq!(name, "abime.stf");
    assert_eq!(what, "1 calculation");
    assert!(state.blocking_dialog());
    assert!(state.alert().is_none(), "l'attente n'est pas un échec");
    assert_eq!(std::fs::read(&file).unwrap(), before, "rien d'écrit");

    state.dispatch(Action::ConfirmDiscardedSave(true));
    assert!(state.pending_discarded_save().is_none());
    assert_ne!(std::fs::read(&file).unwrap(), before, "écrit");
    let (_, _, report) = stokesforge_persistence::load_with_report(&file).unwrap();
    assert!(report.is_clean(), "le fichier réécrit est propre");

    let written = std::fs::read(&file).unwrap();
    state.dispatch(Action::AddGraph);
    state.dispatch(Action::Save);
    assert!(state.pending_discarded_save().is_none(), "plus de question");
    assert_ne!(std::fs::read(&file).unwrap(), written);
    session.shutdown().unwrap();
}

/// Cancel n'écrit rien et oublie le geste qui attendait l'enregistrement.
#[test]
fn refuser_l_ecrasement_n_ecrit_rien_et_oublie_le_geste() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state, file) = start_on_a_discarding_file(dir.path());
    let before = std::fs::read(&file).unwrap();
    state.dispatch(Action::AddGraph);
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Save));
    assert!(state.pending_discarded_save().is_some());
    assert!(!state.take_quit_ready());

    state.dispatch(Action::ConfirmDiscardedSave(false));
    assert!(state.pending_discarded_save().is_none());
    assert_eq!(std::fs::read(&file).unwrap(), before, "rien d'écrit");
    assert!(!state.take_quit_ready(), "le geste est oublié");

    // La question revient, et accepter n'exécute pas le Quit oublié.
    state.dispatch(Action::Save);
    assert!(
        state.pending_discarded_save().is_some(),
        "écarts toujours là"
    );
    state.dispatch(Action::ConfirmDiscardedSave(true));
    assert!(!state.take_quit_ready(), "Quit oublié");
    session.shutdown().unwrap();
}

/// Quit, Save, puis accepter : le projet est enregistré, puis l'application quitte.
#[test]
fn quitter_enregistrer_accepter_enregistre_puis_quitte() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state, file) = start_on_a_discarding_file(dir.path());
    let before = std::fs::read(&file).unwrap();
    state.dispatch(Action::AddGraph);
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Save));
    assert!(!state.take_quit_ready(), "attend la réponse");
    state.dispatch(Action::ConfirmDiscardedSave(true));
    assert_ne!(std::fs::read(&file).unwrap(), before);
    assert!(state.take_quit_ready(), "enregistré, puis Quit");
    session.shutdown().unwrap();
}

/// Enregistrer sous un AUTRE chemin ne demande rien et laisse la source
/// intacte ; y revenir demande tant que les écarts ne sont pas enregistrés, et
/// Cancel rétablit le chemin précédent.
#[test]
fn enregistrer_sous_ailleurs_ne_demande_rien_mais_revenir_a_la_source_oui() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state, file) = start_on_a_discarding_file(dir.path());
    let before = std::fs::read(&file).unwrap();
    let other = dir.path().join("autre.stf");

    state.dispatch(Action::SaveAs {
        path: other.display().to_string(),
    });
    assert!(
        state.pending_discarded_save().is_none(),
        "ailleurs : sans question"
    );
    assert!(other.exists());
    assert_eq!(std::fs::read(&file).unwrap(), before, "source intacte");
    assert_eq!(state.path(), other.display().to_string());

    state.dispatch(Action::SaveAs {
        path: file.display().to_string(),
    });
    assert!(
        state.pending_discarded_save().is_some(),
        "retour à la source"
    );
    state.dispatch(Action::ConfirmDiscardedSave(false));
    assert_eq!(state.path(), other.display().to_string(), "chemin rétabli");
    assert_eq!(std::fs::read(&file).unwrap(), before);

    state.dispatch(Action::SaveAs {
        path: file.display().to_string(),
    });
    state.dispatch(Action::ConfirmDiscardedSave(true));
    assert_eq!(state.path(), file.display().to_string());
    assert_ne!(std::fs::read(&file).unwrap(), before);
    session.shutdown().unwrap();
}

/// Un fichier propre, ou un nouveau projet, n'ouvre aucune question.
#[test]
fn un_fichier_propre_ou_un_nouveau_projet_n_ouvre_aucune_question() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("propre.stf");
    stokesforge_persistence::save(&demo_project(), &file).unwrap();
    let (session, mut state) = start(Project::new("Vide").unwrap(), &file);
    state.dispatch(Action::Load);
    state.dispatch(Action::Save);
    assert!(state.pending_discarded_save().is_none());

    // Un nouveau projet efface le repère de la source abîmée.
    let (session2, mut state2, bad) = start_on_a_discarding_file(dir.path());
    state2.dispatch(Action::NewProject {
        pending_edits: false,
    });
    state2.set_path(bad.display().to_string());
    state2.dispatch(Action::Save);
    assert!(state2.pending_discarded_save().is_none());
    session.shutdown().unwrap();
    session2.shutdown().unwrap();
}

/// Le chargement du démarrage (`mark_loaded`) pose le même repère que le menu
/// Fichier : le premier Enregistrer sur la source demande.
#[test]
fn le_chargement_du_demarrage_pose_le_repere_de_la_source() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("p.stf");
    let (session, mut state) = start(demo_project(), &file);
    state.mark_loaded(&stokesforge_session::LoadReport::default());
    state.dispatch(Action::Save);
    assert!(state.pending_discarded_save().is_none(), "rapport propre");

    state.mark_loaded(&stokesforge_session::LoadReport {
        discarded_corrections: discarded(&[7]),
        ..Default::default()
    });
    let before = std::fs::read(&file).unwrap();
    state.dispatch(Action::Save);
    let (_, what) = state.pending_discarded_save().expect("une question");
    assert_eq!(what, "1 correction record");
    assert_eq!(std::fs::read(&file).unwrap(), before);
    session.shutdown().unwrap();
}

// -- Sauvegarde automatique et récupération (#317) --

mod autosave {
    use super::*;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    const PERIOD: Duration = Duration::from_secs(300);

    /// Session sur la démonstration, dossier de secours dans `dir/recovery`.
    fn start_recovery(dir: &Path, project_file: &str) -> (Session, AppState, PathBuf) {
        let path = if project_file.is_empty() {
            Path::new("").to_path_buf()
        } else {
            dir.join(project_file)
        };
        let (session, mut state) = start(demo_project(), &path);
        let recovery = dir.join("recovery");
        state.set_recovery_dir(Some(recovery.clone()));
        (session, state, recovery)
    }

    /// Les fichiers `.stf` du dossier de secours (la fiche et le verrou ne
    /// comptent pas).
    fn copies(recovery: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(recovery) else {
            return Vec::new();
        };
        let mut copies: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "stf"))
            .collect();
        copies.sort();
        copies
    }

    fn change(state: &mut AppState) {
        state.dispatch(Action::AddGraph);
    }

    // -- Préférences, via les actions --

    #[test]
    fn la_sauvegarde_est_active_par_defaut_avec_cinq_minutes() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, _) = start_recovery(dir.path(), "a.stf");
        assert!(state.autosave_enabled());
        assert_eq!(state.autosave_minutes(), 5);
        state.dispatch(Action::SetAutosaveMinutes(999));
        assert_eq!(state.autosave_minutes(), 60);
        state.dispatch(Action::SetAutosaveMinutes(0));
        assert_eq!(state.autosave_minutes(), 1);
        state.dispatch(Action::SetAutosave(false));
        assert!(!state.autosave_enabled());
    }

    // -- L'écriture --

    /// Une modification puis l'échéance : une copie relue égale au modèle, sans
    /// aucune trace dans l'historique, le titre, le chemin, ni la question de
    /// Quit ; le `.stf` de l'utilisateur n'est pas écrit.
    #[test]
    fn la_copie_relit_le_modele_sans_toucher_historique_titre_ni_quit() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, recovery) = start_recovery(dir.path(), "a.stf");
        let t0 = Instant::now();
        assert_eq!(state.poll_autosave(t0), Some(PERIOD));
        change(&mut state);
        let undo_label = state.snapshot().history.undo_label.clone();
        let revision = state.snapshot().revision;
        let title = state.window_title();
        assert!(undo_label.is_some());

        state.poll_autosave(t0 + PERIOD);

        let copies = copies(&recovery);
        assert_eq!(copies.len(), 1);
        let name = copies[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(name.starts_with("a-"), "nom du projet : {name}");
        let back = stokesforge_persistence::load(&copies[0]).expect("la copie se relit");
        assert_eq!(back.tree(), state.tree());
        assert!(
            !dir.path().join("a.stf").exists(),
            "le .stf n'est pas écrit"
        );
        assert_eq!(state.snapshot().history.undo_label, undo_label);
        assert_eq!(state.snapshot().revision, revision);
        assert_eq!(state.window_title(), title, "titre « modifié » inchangé");
        assert_eq!(state.path(), dir.path().join("a.stf").display().to_string());

        state.dispatch(Action::Quit {
            pending_edits: false,
        });
        assert_eq!(
            state.unsaved_pending(),
            Some(&UnsavedGesture::Quit),
            "la copie n'a pas enregistré le projet"
        );
        state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
        // Témoin : un vrai enregistrement, lui, lève la question.
        state.dispatch(Action::Save);
        state.dispatch(Action::Quit {
            pending_edits: false,
        });
        assert!(state.take_quit_ready(), "enregistré : Quit direct");
    }

    #[test]
    fn avant_l_echeance_rien_n_est_ecrit() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, recovery) = start_recovery(dir.path(), "a.stf");
        let t0 = Instant::now();
        state.poll_autosave(t0);
        change(&mut state);
        let almost = t0 + PERIOD - Duration::from_secs(1);
        assert_eq!(state.poll_autosave(almost), Some(Duration::from_secs(1)));
        assert!(copies(&recovery).is_empty());
        state.poll_autosave(t0 + PERIOD);
        assert_eq!(copies(&recovery).len(), 1);
    }

    /// Rien n'a changé depuis le démarrage, puis depuis la dernière copie :
    /// aucune écriture (la copie effacée à la main ne revient pas).
    #[test]
    fn sans_changement_aucune_ecriture() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, recovery) = start_recovery(dir.path(), "a.stf");
        let t0 = Instant::now();
        state.poll_autosave(t0);
        state.poll_autosave(t0 + PERIOD);
        assert!(copies(&recovery).is_empty(), "rien depuis le démarrage");

        change(&mut state);
        state.poll_autosave(t0 + 2 * PERIOD);
        let copy = copies(&recovery).pop().expect("copie écrite");
        std::fs::remove_file(&copy).unwrap();
        state.poll_autosave(t0 + 3 * PERIOD);
        assert!(
            copies(&recovery).is_empty(),
            "rien depuis la dernière copie"
        );
    }

    /// Un enregistrement remet le repère : aucune copie tant que rien ne change.
    #[test]
    fn apres_un_enregistrement_rien_a_copier() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, recovery) = start_recovery(dir.path(), "a.stf");
        let t0 = Instant::now();
        state.poll_autosave(t0);
        change(&mut state);
        state.dispatch(Action::Save);
        state.poll_autosave(t0 + PERIOD);
        assert!(copies(&recovery).is_empty());
    }

    /// Deux échéances avec des changements : un seul fichier, écrasé.
    #[test]
    fn deux_echeances_ecrasent_la_meme_copie() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, recovery) = start_recovery(dir.path(), "a.stf");
        let t0 = Instant::now();
        state.poll_autosave(t0);
        change(&mut state);
        state.poll_autosave(t0 + PERIOD);
        change(&mut state);
        state.poll_autosave(t0 + 2 * PERIOD);
        let copies = copies(&recovery);
        assert_eq!(copies.len(), 1, "{copies:?}");
        let back = stokesforge_persistence::load(&copies[0]).unwrap();
        assert_eq!(back.tree(), state.tree(), "la seconde écriture l'emporte");
    }

    #[test]
    fn preference_coupee_rien_n_est_ecrit() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, recovery) = start_recovery(dir.path(), "a.stf");
        state.dispatch(Action::SetAutosave(false));
        let t0 = Instant::now();
        assert_eq!(state.poll_autosave(t0), None);
        change(&mut state);
        assert_eq!(state.poll_autosave(t0 + 10 * PERIOD), None);
        assert!(copies(&recovery).is_empty());
    }

    /// La période réglée s'applique, à partir du réglage.
    #[test]
    fn la_periode_reglee_est_celle_du_minuteur() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, recovery) = start_recovery(dir.path(), "a.stf");
        let t0 = Instant::now();
        state.poll_autosave(t0);
        // Réglée en cours de route : la nouvelle période compte dès l'image
        // suivante, pas à l'ancienne échéance (5 minutes).
        state.dispatch(Action::SetAutosaveMinutes(1));
        change(&mut state);
        let t1 = t0 + Duration::from_secs(10);
        state.poll_autosave(t1);
        state.poll_autosave(t1 + Duration::from_secs(59));
        assert!(copies(&recovery).is_empty());
        state.poll_autosave(t1 + Duration::from_secs(60));
        assert_eq!(copies(&recovery).len(), 1);
    }

    #[test]
    fn un_projet_sans_chemin_est_copie_sous_untitled() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, recovery) = start_recovery(dir.path(), "");
        let t0 = Instant::now();
        state.poll_autosave(t0);
        change(&mut state);
        state.poll_autosave(t0 + PERIOD);
        let copies = copies(&recovery);
        assert_eq!(copies.len(), 1);
        let name = copies[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(name.starts_with("Untitled-"), "{name}");
    }

    /// Macro ouverte : l'échéance est repoussée sans erreur ; fermée, la copie
    /// part au nouvel essai.
    #[test]
    fn pendant_une_macro_l_echeance_saute_sans_erreur() {
        let dir = tempfile::tempdir().unwrap();
        let (session, mut state, recovery) = start_recovery(dir.path(), "a.stf");
        let t0 = Instant::now();
        state.poll_autosave(t0);
        change(&mut state);
        let handle = session.handle();
        handle.begin_macro(MacroKind::Import).unwrap();
        state.refresh();
        assert!(state.snapshot().history.in_macro);
        let due = t0 + PERIOD;
        let messages = state.status_history().count();
        let retry = state.poll_autosave(due).expect("réarmé");
        assert!(retry < PERIOD, "nouvel essai rapproché : {retry:?}");
        assert!(copies(&recovery).is_empty());
        assert_eq!(
            state.status_history().count(),
            messages,
            "aucun message : sans avertissement ni erreur"
        );
        handle.end_macro().unwrap();
        state.refresh();
        state.poll_autosave(due + retry);
        assert_eq!(copies(&recovery).len(), 1);
    }

    /// Un recalcul en cours : même saut.
    #[test]
    fn pendant_un_recalcul_l_echeance_saute() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state) = start(heavy_formula_project(), &dir.path().join("l.stf"));
        let recovery = dir.path().join("recovery");
        state.set_recovery_dir(Some(recovery.clone()));
        let t0 = Instant::now();
        state.poll_autosave(t0);
        state.dispatch(Action::AddGraph);
        assert!(state.recalc_work().is_some(), "recalcul en cours");
        let retry = state.poll_autosave(t0 + PERIOD).expect("réarmé");
        assert!(retry < PERIOD);
        assert!(copies(&recovery).is_empty());
        finish_recalculation(&mut state);
        state.poll_autosave(t0 + PERIOD + retry);
        assert_eq!(
            copies(&recovery).len(),
            1,
            "messages : {}",
            status_messages(&state)
        );
    }

    /// Dossier impossible : un seul avertissement tant que ça dure.
    #[test]
    fn un_dossier_impossible_ne_dit_qu_un_avertissement() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, _) = start_recovery(dir.path(), "a.stf");
        let obstacle = dir.path().join("fichier");
        std::fs::write(&obstacle, b"x").unwrap();
        state.set_recovery_dir(Some(obstacle.join("recovery")));
        let t0 = Instant::now();
        state.poll_autosave(t0);
        change(&mut state);
        let warnings = |state: &AppState| {
            state
                .status_history()
                .filter(|m| m.level == StatusLevel::Warning)
                .count()
        };
        let before = warnings(&state);
        state.poll_autosave(t0 + PERIOD);
        state.poll_autosave(t0 + 2 * PERIOD);
        state.poll_autosave(t0 + 3 * PERIOD);
        assert_eq!(warnings(&state), before + 1, "un seul avertissement");
        assert!(state.status().is_some_and(|s| s.text
            == stokesforge_app_state::tr!(
                "status-autosave-failed",
                cause = stokesforge_app_state::errors::io::error_message(
                    &std::fs::create_dir_all(obstacle.join("recovery")).unwrap_err()
                )
            )));
    }

    // -- La suppression de la copie --

    fn with_copy(dir: &Path, file: &str) -> (Session, AppState, PathBuf, Instant) {
        let (session, mut state, recovery) = start_recovery(dir, file);
        let t0 = Instant::now();
        state.poll_autosave(t0);
        change(&mut state);
        state.poll_autosave(t0 + PERIOD);
        assert_eq!(copies(&recovery).len(), 1, "copie écrite");
        (session, state, recovery, t0)
    }

    #[test]
    fn enregistrer_supprime_la_copie() {
        let dir = tempfile::tempdir().unwrap();
        let (_s, mut state, recovery, _) = with_copy(dir.path(), "a.stf");
        state.dispatch(Action::Save);
        assert!(copies(&recovery).is_empty());
        assert_eq!(
            std::fs::read_dir(&recovery).unwrap().count(),
            0,
            "fiche et verrou aussi"
        );
    }

    #[test]
    fn enregistrer_sous_supprime_la_copie() {
        let dir = tempfile::tempdir().unwrap();
        let (_s, mut state, recovery, _) = with_copy(dir.path(), "");
        state.dispatch(Action::SaveAs {
            path: dir.path().join("neuf.stf").display().to_string(),
        });
        assert!(copies(&recovery).is_empty());
    }

    #[test]
    fn fermer_en_abandonnant_supprime_la_copie() {
        let dir = tempfile::tempdir().unwrap();
        let (_s, mut state, recovery, _) = with_copy(dir.path(), "a.stf");
        state.dispatch(Action::Quit {
            pending_edits: false,
        });
        assert_eq!(
            copies(&recovery).len(),
            1,
            "la question est posée, rien n'est perdu"
        );
        state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Discard));
        assert!(state.take_quit_ready());
        assert!(copies(&recovery).is_empty());
    }

    #[test]
    fn nouveau_projet_supprime_la_copie() {
        let dir = tempfile::tempdir().unwrap();
        let (_s, mut state, recovery, _) = with_copy(dir.path(), "a.stf");
        state.dispatch(Action::NewProject {
            pending_edits: false,
        });
        state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Discard));
        assert!(copies(&recovery).is_empty());
    }

    #[test]
    fn ouvrir_un_autre_projet_supprime_la_copie() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("autre.stf");
        {
            let (_s, mut state) = start_demo(dir.path());
            state.dispatch(Action::SaveAs {
                path: other.display().to_string(),
            });
        }
        let (_s, mut state, recovery, _) = with_copy(dir.path(), "a.stf");
        state.dispatch(Action::OpenProject {
            path: other.display().to_string(),
            pending_edits: false,
        });
        state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Discard));
        assert!(copies(&recovery).is_empty());
    }

    /// Annuler la question garde la copie.
    #[test]
    fn annuler_la_question_garde_la_copie() {
        let dir = tempfile::tempdir().unwrap();
        let (_s, mut state, recovery, _) = with_copy(dir.path(), "a.stf");
        state.dispatch(Action::Quit {
            pending_edits: false,
        });
        state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
        assert_eq!(copies(&recovery).len(), 1);
    }

    /// Session perdue puis Close : la copie automatique est gardée.
    #[test]
    fn session_perdue_puis_close_garde_la_copie() {
        let dir = tempfile::tempdir().unwrap();
        let (session, mut state, recovery, _) = with_copy(dir.path(), "a.stf");
        let automatic = copies(&recovery);
        session.shutdown().unwrap();
        state.poll_recalc();
        assert!(state.session_lost());
        state.dispatch(Action::DismissAlert);
        assert!(state.take_quit_ready(), "Close ferme l'application");
        for copy in automatic {
            assert!(copy.exists(), "la copie automatique reste : {copy:?}");
        }
    }

    // -- La récupération au démarrage --

    /// Une copie d'un processus disparu : écrite sans verrou, comme #310.
    fn dead_copy(recovery: &Path, project_path: &str, graphs: usize) -> (PathBuf, Tree) {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut source) = start_demo(dir.path());
        for _ in 0..graphs {
            source.dispatch(Action::AddGraph);
        }
        let path =
            stokesforge_app_state::recovery::write_copy(recovery, project_path, source.snapshot())
                .unwrap();
        (path, source.tree().as_ref().clone())
    }

    #[test]
    fn open_reprend_le_chemin_d_origine_et_un_historique_vide() {
        let dir = tempfile::tempdir().unwrap();
        let recovery = dir.path().join("recovery");
        let origin = dir.path().join("travail.stf");
        let (copy, tree) = dead_copy(&recovery, &origin.display().to_string(), 2);
        // Un projet vide : rien à perdre, Open charge sans poser de question.
        let (_s, mut state) = start(Project::new("Vide").unwrap(), Path::new(""));
        state.set_recovery_dir(Some(recovery.clone()));
        state.scan_recovery();
        let rows = state.recovery_rows().expect("une copie proposée");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, copy);

        state.dispatch(Action::RecoveryOpen(copy.clone()));

        assert_eq!(state.tree().as_ref(), &tree);
        assert_eq!(state.path(), origin.display().to_string());
        assert!(!state.snapshot().history.can_undo, "historique vide");
        assert!(
            state.recovery_rows().is_none(),
            "liste vide : fenêtre fermée"
        );
        assert!(!copy.exists(), "la copie ouverte est effacée");
        assert_eq!(copies(&recovery).len(), 1, "recopiée dans l'emplacement");
        // Travail non enregistré : fermer pose la question.
        state.dispatch(Action::Quit {
            pending_edits: false,
        });
        assert_eq!(state.unsaved_pending(), Some(&UnsavedGesture::Quit));
    }

    #[test]
    fn open_d_une_copie_sans_chemin_laisse_le_chemin_vide() {
        let dir = tempfile::tempdir().unwrap();
        let recovery = dir.path().join("recovery");
        let (copy, _) = dead_copy(&recovery, "", 1);
        let (_s, mut state) = start(Project::new("Vide").unwrap(), &dir.path().join("x.stf"));
        state.set_recovery_dir(Some(recovery));
        state.scan_recovery();
        state.dispatch(Action::RecoveryOpen(copy));
        assert_eq!(state.path(), "");
    }

    /// Du travail en cours : la question « Enregistrer ? » d'abord ; la liste
    /// reste ouverte pour les autres copies.
    #[test]
    fn open_demande_enregistrer_et_la_liste_reste_ouverte() {
        let dir = tempfile::tempdir().unwrap();
        let recovery = dir.path().join("recovery");
        let (first, _) = dead_copy(&recovery, "", 1);
        let (second, _) = dead_copy(&recovery, "/x/b.stf", 2);
        let (_s, mut state) = start_demo(dir.path());
        change(&mut state);
        state.set_recovery_dir(Some(recovery.clone()));
        state.scan_recovery();
        assert_eq!(state.recovery_rows().unwrap().len(), 2);

        state.dispatch(Action::RecoveryOpen(first.clone()));
        assert_eq!(
            state.unsaved_pending(),
            Some(&UnsavedGesture::OpenRecovery(first.clone()))
        );
        assert!(first.exists(), "rien n'est chargé avant la réponse");
        state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Discard));

        assert!(!first.exists());
        let rows = state.recovery_rows().expect("la liste reste ouverte");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, second);
    }

    #[test]
    fn ignore_garde_le_fichier_et_il_revient_au_demarrage_suivant() {
        let dir = tempfile::tempdir().unwrap();
        let recovery = dir.path().join("recovery");
        let (copy, _) = dead_copy(&recovery, "", 1);
        let (_s, mut state) = start_demo(dir.path());
        state.set_recovery_dir(Some(recovery.clone()));
        state.scan_recovery();
        state.dispatch(Action::RecoveryIgnore(copy.clone()));
        assert!(state.recovery_rows().is_none());
        assert!(copy.exists(), "Ignore garde le fichier");
        let (_s2, mut next) = start_demo(dir.path());
        next.set_recovery_dir(Some(recovery));
        next.scan_recovery();
        assert_eq!(next.recovery_rows().unwrap().len(), 1, "reproposée");
    }

    #[test]
    fn delete_demande_confirmation_puis_efface_copie_fiche_et_verrou() {
        let dir = tempfile::tempdir().unwrap();
        let recovery = dir.path().join("recovery");
        let (copy, _) = dead_copy(&recovery, "/x/b.stf", 1);
        std::fs::write(copy.with_extension("lock"), b"").unwrap();
        let (_s, mut state) = start_demo(dir.path());
        state.set_recovery_dir(Some(recovery.clone()));
        state.scan_recovery();

        state.dispatch(Action::RecoveryDelete(copy.clone()));
        assert!(copy.exists(), "rien n'est effacé avant la confirmation");
        assert_eq!(state.pending_recovery_delete().as_deref(), Some("b.stf"));
        assert!(state.blocking_dialog());
        state.dispatch(Action::ConfirmRecoveryDelete(false));
        assert!(copy.exists(), "Cancel garde la copie");
        assert!(state.recovery_rows().is_some());

        state.dispatch(Action::RecoveryDelete(copy.clone()));
        state.dispatch(Action::ConfirmRecoveryDelete(true));
        assert_eq!(std::fs::read_dir(&recovery).unwrap().count(), 0);
        assert!(state.recovery_rows().is_none());
    }

    /// Close ignore tout : les fichiers restent.
    #[test]
    fn close_ignore_les_copies_restantes() {
        let dir = tempfile::tempdir().unwrap();
        let recovery = dir.path().join("recovery");
        let (copy, _) = dead_copy(&recovery, "", 1);
        let (_s, mut state) = start_demo(dir.path());
        state.set_recovery_dir(Some(recovery));
        state.scan_recovery();
        state.dispatch(Action::CloseRecovery);
        assert!(state.recovery_rows().is_none());
        assert!(copy.exists());
    }

    /// Sans dossier de secours (tests, système sans dossier de données) : rien
    /// n'est proposé, rien n'est écrit.
    #[test]
    fn sans_dossier_de_secours_rien_ne_se_passe() {
        let dir = tempfile::tempdir().unwrap();
        let (_s, mut state) = start_demo(dir.path());
        state.set_recovery_dir(None);
        state.scan_recovery();
        assert!(state.recovery_rows().is_none());
        let t0 = Instant::now();
        state.poll_autosave(t0);
        change(&mut state);
        state.poll_autosave(t0 + PERIOD);
    }
}
