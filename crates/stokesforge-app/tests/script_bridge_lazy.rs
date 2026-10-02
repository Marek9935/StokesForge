//! Critère 3/8 : l'interpréteur ne démarre **qu'au premier script**, jamais
//! avant — pas à la construction de `AppState`, pas à la pose du descripteur
//! Python, pas à l'ouverture d'une fenêtre.
//!
//! # Pourquoi ce test vit seul dans son propre fichier
//!
//! `EmbeddedPython::is_initialized()` lit un état **global au processus**
//! (`static INIT: OnceLock<...>` dans `stokesforge-python::interpreter`), et Cargo
//! exécute par défaut les tests d'un même binaire en parallèle, sur des
//! threads du même processus. N'importe quel autre test du même binaire qui
//! exécute un script aurait déjà démarré Python avant celui-ci, dans un ordre
//! non garanti — l'assertion « faux avant » ne pourrait alors jamais être
//! honnête. Un fichier sous `tests/` est son **propre binaire** (donc son
//! propre processus) : celui-ci ne contient qu'un seul test, qui est donc
//! seul à pouvoir toucher Python dans son processus. C'est ce qui rend « faux
//! avant, vrai après » vérifiable sans dépendre de l'ordre d'exécution.
//!
//! Les autres critères du script (exécution, sortie, rafraîchissement,
//! annulation, exception) sont dans `tests/script_panel.rs`, où l'ordre
//! d'initialisation de l'interpréteur n'a pas d'importance.

#![cfg(feature = "python")]

use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use stokesforge_app::state::{Action, AppState};
use stokesforge_core::{Column, Project};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_session::Session;

/// Résout le préfixe embarqué : `STOKESFORGE_PYTHON_HOME` si posée par le script
/// de test, sinon `vendor/python/extracted/python` relatif au manifeste.
fn home() -> PathBuf {
    match std::env::var_os(interpreter::HOME_ENV) {
        Some(value) => PathBuf::from(value),
        None => {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        }
    }
}

#[test]
fn the_bridge_starts_only_at_the_first_script() {
    assert!(
        !EmbeddedPython::is_initialized(),
        "ce test doit être seul dans son binaire pour que ceci soit honnête \
         (voir la doc de tête de ce fichier)"
    );

    let mut project = Project::new("Demo").expect("projet de démonstration");
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
        .add_column(sheet, None, Some("Temps"), Column::from_f64(&[1.0]))
        .expect("colonne Temps");

    let session = Session::spawn(project, None).expect("thread de session");
    let mut state = AppState::new(session.handle(), "script-test.stokesforge".to_string())
        .expect("état applicatif initial");

    // Construire l'état ne doit pas toucher Python : c'est le cœur du
    // critère « fenêtre et feuille s'ouvrent sans interpréteur ».
    assert!(
        !EmbeddedPython::is_initialized(),
        "AppState::new ne démarre pas l'interpréteur"
    );

    state.set_python(EmbeddedPython::with_home(home()));
    assert!(
        !EmbeddedPython::is_initialized(),
        "poser le descripteur ne démarre rien non plus : locate/with_home \
         ne touchent pas à CPython"
    );

    state.set_script_source("print('bonjour')".to_string());
    state.dispatch(Action::RunScript);

    let deadline = Instant::now() + Duration::from_secs(30);
    while state.script_running() {
        assert!(
            Instant::now() <= deadline,
            "le script n'a pas terminé sous 30 s"
        );
        thread::sleep(Duration::from_millis(1));
        state.poll_script();
    }

    assert!(
        EmbeddedPython::is_initialized(),
        "le premier script doit avoir démarré l'interpréteur"
    );
    let output = state.script_output().expect("une sortie enregistrée");
    assert!(
        output.error.is_none(),
        "script en échec : {:?}",
        output.error
    );
    assert!(
        output.stdout.contains("bonjour"),
        "sortie : {:?}",
        output.stdout
    );
}
