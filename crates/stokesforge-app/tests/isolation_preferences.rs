//! Les tests de `stokesforge-app` n'atteignent pas les vraies préférences de la
//! machine (#250, #263).
//!
//! # Pourquoi ce test existe
//!
//! Le chemin des préférences (`preferences::default_path`), la langue de départ
//! (`i18n::startup_language`) et `set_object_manager_open_for_test` ne sont
//! isolés que sous `cfg(test)` — qui ne vaut que pour les tests du crate
//! `stokesforge-app-state`. Les tests de `stokesforge-app` bâtissent un
//! `AppState` par `AppState::new` (une vingtaine dans `src/app.rs`) : ils
//! liraient et écriraient le vrai `preferences.json` de l'utilisateur sans la
//! feature `test-support`, que la dev-dependency de `Cargo.toml` active. Retirer
//! cette feature fait échouer ce test.
//!
//! Un seul test, dans son propre binaire : il pose `XDG_CONFIG_HOME` (état
//! global du processus) et lit la langue du chargeur global.

use stokesforge_app_state::i18n::{self, Language, LANGUAGE_VARIABLE};
use stokesforge_app_state::preferences::Preferences;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_core::Project;
use stokesforge_session::Session;

#[test]
fn un_etat_bati_par_new_n_atteint_pas_le_vrai_fichier_des_preferences() {
    // Le « vrai » dossier de configuration est celui-ci, le temps du test.
    let config = tempfile::tempdir().expect("dossier temporaire");
    std::env::set_var("XDG_CONFIG_HOME", config.path());
    std::env::remove_var(LANGUAGE_VARIABLE);
    let dirs = directories::ProjectDirs::from("", "", "StokesForge").expect("dossiers");
    assert!(
        dirs.config_dir().starts_with(config.path()),
        "témoin : le dossier de configuration suit XDG_CONFIG_HOME ({:?})",
        dirs.config_dir()
    );
    let fichier = dirs.config_dir().join("preferences.json");

    // Une préférence « de l'utilisateur » : français, écrite au vrai chemin.
    let mut reelles = Preferences::load_from(Some(fichier.clone()));
    reelles.save_language("fr").expect("écriture");
    assert_eq!(
        Preferences::load_from(Some(fichier.clone())).language(),
        Some("fr"),
        "témoin : le fichier est lu quand on le désigne"
    );
    let avant = std::fs::read_to_string(&fichier).expect("lecture");

    // La langue de départ ne lit pas ce fichier (avant tout autre accès au
    // chargeur global de ce processus).
    assert_eq!(
        i18n::language(),
        Language::English,
        "la langue de départ a lu les vraies préférences"
    );

    // `Preferences::load()` ne désigne aucun fichier.
    assert_eq!(Preferences::load().language(), None);

    // Un état bâti par `new` n'y écrit pas non plus.
    let session = Session::spawn(Project::new("P").expect("projet"), None).expect("session");
    let mut state = AppState::new(session.handle(), String::new()).expect("état");
    state.dispatch(Action::SetObjectManagerOpen(!state.object_manager_open()));
    // Sans chemin, l'enregistrement échoue (message d'erreur de la barre
    // d'état) au lieu d'écrire ailleurs : c'est le comportement voulu.
    let _ = Preferences::load().save_language("en");
    assert_eq!(
        std::fs::read_to_string(&fichier).expect("lecture"),
        avant,
        "le vrai fichier des préférences a été modifié"
    );
    drop(state);
    session.shutdown().expect("arrêt");
}
