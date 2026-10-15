//! Critères d'acceptation de la console de script, pilotés **sans fenêtre**
//! mais **avec** l'interpréteur.
//!
//! Symétrique de `tests/shell.rs` : celui-là prouve que le banc fonctionne
//! sans le pont Python, celui-ci prouve que le pont, une fois branché, tient
//! les promesses de la tâche 5.6 — sortie récupérée, grille rafraîchie sans
//! action supplémentaire, annulation par `Action::Undo` de ce qu'un script a
//! écrit, exception de script qui ne casse rien — vues depuis `AppState`,
//! exactement comme les verrait la fenêtre.
//!
//! Exige le préfixe vendorisé et l'environnement Python décrit dans
//! `.codex/README.md`, puis `cargo test -p stokesforge-app --features python
//! --test script_panel`. `scripts/python-test.sh` lance aussi les tests de
//! l'application, dont ce fichier et le parcours numpy → colonnes → graphe.
//!
//! # `Action::RunScript` ne bloque pas
//!
//! [`AppState::dispatch`] ne bloque jamais, `RunScript` compris (voir la doc
//! de tête de `stokesforge_app::state`) : le résultat n'arrive que par
//! [`AppState::poll_script`]. Chaque test attend donc une exécution en
//! sondant cette méthode dans une boucle courte ([`run_and_wait`]) — le même
//! geste que `crate::app` fait à chaque image, jamais un `sleep` bloquant qui
//! espère juste.
//!
//! # Le démarrage paresseux de l'interpréteur n'est PAS prouvé ici
//!
//! `EmbeddedPython::is_initialized()` est un état **global au processus**, et
//! Cargo exécute les tests d'un même binaire en parallèle par défaut : ce
//! fichier contient plusieurs tests qui démarrent tous un script, donc
//! l'interpréteur est de toute façon initialisé dès le premier d'entre eux à
//! s'exécuter, dans un ordre non garanti. La preuve « faux avant, vrai après »
//! (critère 8) exige d'être **seule** à toucher Python dans son processus :
//! elle vit dans son propre fichier, `tests/script_bridge_lazy.rs`, qui est
//! son propre binaire de test.

#![cfg(feature = "python")]

use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use stokesforge_app::state::{Action, AppState};
use stokesforge_core::{Column, NodeId, Project};
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

/// Délai maximal qu'un test attend la fin d'un script avant d'abandonner :
/// aucun test ne doit attendre indéfiniment.
const TIMEOUT: Duration = Duration::from_secs(30);

/// Projet de démonstration de ce fichier : un classeur `Book1` avec `Sheet1`
/// et une colonne numérique `Temps`. Indépendant du projet de démonstration
/// du binaire (`main.rs`) : ce fichier ne doit pas casser si celui-là change,
/// et réciproquement — même principe que `tests/shell.rs`.
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
}

/// Démarre une session sur le projet de démonstration et l'état applicatif
/// associé, avec un descripteur d'interpréteur posé (mais **pas démarré** —
/// voir la doc de `stokesforge_app::state`). Rend la session, à garder vivante,
/// et la feuille explicite que les tests pilotent, comme `tests/shell.rs`.
fn start() -> (Session, AppState, NodeId) {
    let project = demo_project();
    let sheet = project
        .tree()
        .resolve("[Book1]Sheet1")
        .expect("feuille de démonstration");
    let session = Session::spawn(project, None).expect("thread de session");
    let mut state = AppState::new_with_preferences(
        session.handle(),
        "script-test.stf".to_string(),
        stokesforge_app::preferences::Preferences::load_from(None),
    )
    .expect("état applicatif initial");
    state.set_python(EmbeddedPython::with_home(home()));
    (session, state, sheet)
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

/// Pose `source` comme script courant, le lance et attend — en sondant
/// [`AppState::poll_script`], comme `crate::app` le fait à chaque image —
/// que le résultat soit prêt. Panique si `TIMEOUT` est dépassé.
fn run_and_wait(state: &mut AppState, source: &str) {
    state.set_script_source(source.to_string());
    state.dispatch(Action::RunScript);

    let deadline = Instant::now() + TIMEOUT;
    while state.script_running() {
        assert!(
            Instant::now() <= deadline,
            "le script n'a pas terminé sous {TIMEOUT:?}"
        );
        thread::sleep(Duration::from_millis(1));
        state.poll_script();
    }
}

// --- 4. Un script s'exécute et sa sortie revient ----------------------------

#[test]
fn a_script_runs_and_its_output_comes_back() {
    let (_session, mut state, _sheet) = start();

    run_and_wait(&mut state, "print(\"bonjour\")\n");

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

// --- 5. La grille reflète le modèle après le script --------------------------

#[test]
fn the_grid_reflects_the_model_after_a_script_without_any_extra_action() {
    let (_session, mut state, sheet) = start();
    let temps = column_id(&state, sheet, "Temps");
    assert_eq!(state.cell_text(temps, 0), "0");
    assert_eq!(state.cell_text(temps, 4), "2");

    run_and_wait(
        &mut state,
        "a = pyst.resolve(\"[Book1]Sheet1!Temps\")\n\
         pyst.set_column(a, pyst.column(a) * 0.0 + 42.0)\n",
    );

    // Aucune action supplémentaire n'a été dispatchée : c'est `poll_script`,
    // sondé par `run_and_wait` (comme `crate::app` le fait à chaque image),
    // qui a rafraîchi l'instantané. C'est le piège de cette tâche.
    assert_eq!(state.cell_text(temps, 0), "42");
    assert_eq!(state.cell_text(temps, 4), "42");
}

// --- 6. Annulation de ce qu'un script a écrit --------------------------------

#[test]
fn action_undo_reverts_what_a_script_wrote() {
    let (_session, mut state, sheet) = start();
    let temps = column_id(&state, sheet, "Temps");
    assert_eq!(state.cell_text(temps, 1), "0.5");

    run_and_wait(
        &mut state,
        "a = pyst.resolve(\"[Book1]Sheet1!Temps\")\n\
         pyst.set_column(a, pyst.column(a) * 0.0 + 99.0)\n",
    );
    assert_eq!(state.cell_text(temps, 1), "99");
    assert!(
        state.can_undo(),
        "l'écriture du script doit être dans l'historique"
    );

    // Ctrl+Z côté fenêtre : c'est le critère de fin, vu depuis l'interface.
    state.dispatch(Action::Undo);

    assert_eq!(
        state.cell_text(temps, 1),
        "0.5",
        "Action::Undo doit défaire l'écriture faite par le script"
    );
}

// --- 7. Une exception de script est signalée, sa trace reste dans la console --

#[test]
fn a_script_exception_is_a_short_error_with_its_trace_in_the_console() {
    let (_session, mut state, sheet) = start();
    let before = state.snapshot().revision;

    run_and_wait(&mut state, "1 / 0\n");

    let output = state.script_output().expect("une sortie enregistrée");
    assert!(
        output
            .error
            .as_deref()
            .is_some_and(|trace| trace.contains("ZeroDivisionError")),
        "trace attendue avec ZeroDivisionError : {:?}",
        output.error
    );
    assert!(
        output
            .error
            .as_deref()
            .is_some_and(|trace| trace.contains("Traceback")),
        "trace d'appels complète attendue : {:?}",
        output.error
    );

    // M10 (#230) : l'exception est signalée dans la barre d'état, la trace
    // restant dans la console — un seul message court, jamais la trace.
    let status = state.status();
    assert!(
        status.is_some_and(
            |status| status.is_error() && status.text == "Script failed. See the console"
        ),
        "une exception de script doit être signalée en erreur courte : {status:?}"
    );
    // Rien n'a été écrit dans le modèle : la révision n'a pas bougé.
    assert_eq!(state.snapshot().revision, before);

    // L'état reste utilisable pour de vrai : une action normale marche
    // encore après une exception de script.
    let columns_before = state.visible_columns(sheet).len();
    state.dispatch(Action::AddColumn { sheet });
    assert!(state.status().is_some_and(|status| !status.is_error()));
    assert_eq!(state.visible_columns(sheet).len(), columns_before + 1);
}

#[test]
fn lecture_calcul_resultats_et_graphe_se_rafraichissent_et_sannulent_ensemble() {
    let (_session, mut state, _sheet) = start();
    let before = state.snapshot().tree.clone();
    run_and_wait(
        &mut state,
        r#"
import numpy as np
source = pyst.column(pyst.resolve('[Book1]Sheet1!Temps'))
book = pyst.create_workbook('Résultats')
sheet = pyst.create_worksheet(book, 'Calcul')
x = pyst.create_column(sheet, 'X')
y = pyst.create_column(sheet, 'Y')
pyst.set_values(x, np.asarray(source, dtype=np.float64))
pyst.set_values(y, source**2)
pyst.set_column_meta(y, unit='mV', role='y')
pyst.plot(y, x, name='Graph1', title='$t^2$')
"#,
    );
    assert!(
        state.script_output().unwrap().error.is_none(),
        "{:?}",
        state.script_output()
    );
    assert!(state.snapshot().tree.resolve("[Graph1]").is_ok());
    let y = state
        .snapshot()
        .tree
        .resolve("[Résultats]Calcul!Y")
        .unwrap();
    assert_eq!(
        state
            .snapshot()
            .tree
            .node(y)
            .unwrap()
            .as_column()
            .unwrap()
            .float_values()
            .unwrap()
            .as_slice(),
        &[0., 0.25, 1., 2.25, 4.]
    );
    let after = state.snapshot().tree.clone();
    state.dispatch(Action::Undo);
    assert_eq!(*state.snapshot().tree, *before);
    state.dispatch(Action::Redo);
    assert_eq!(*state.snapshot().tree, *after);
}
