//! The tests of `stokesforge-app` do not reach the machine's real
//! preferences (#250, #263).
//!
//! # Why this test exists
//!
//! The preferences path (`preferences::default_path`), the startup language
//! (`i18n::startup_language`) and `set_object_manager_open_for_test` are
//! isolated only under `cfg(test)` — which holds only for the tests of the
//! `stokesforge-app-state` crate. The tests of `stokesforge-app` build an
//! `AppState` through `AppState::new` (about twenty in `src/app.rs`): they
//! would read and write the user's real `preferences.json` without the
//! `test-support` feature, which the dev-dependency of `Cargo.toml` enables.
//! Removing that feature makes this test fail.
//!
//! A single test, in its own binary: it sets `STOKESFORGE_CONFIG_DIR`
//! (global process state; the only variable that `directories` does not
//! ignore outside Linux, #332) and reads the language of the global loader.

use stokesforge_app_state::i18n::{self, Language, LANGUAGE_VARIABLE};
use stokesforge_app_state::preferences::Preferences;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_core::Project;
use stokesforge_import::config_dir::{config_dir, CONFIG_DIR_VARIABLE};
use stokesforge_session::Session;

#[test]
fn a_state_built_by_new_does_not_reach_the_real_preferences_file() {
    // The "real" configuration directory is this one, for the duration of the test.
    let config = tempfile::tempdir().expect("temporary directory");
    std::env::set_var(CONFIG_DIR_VARIABLE, config.path());
    std::env::set_var("XDG_CONFIG_HOME", config.path());
    std::env::remove_var(LANGUAGE_VARIABLE);
    let dir = config_dir().expect("configuration directory");
    assert_eq!(
        dir,
        config.path(),
        "witness: the configuration directory follows STOKESFORGE_CONFIG_DIR"
    );
    let file = dir.join("preferences.json");

    // A "user" preference: French, written at the real path.
    let mut real_prefs = Preferences::load_from(Some(file.clone()));
    real_prefs.save_language("fr").expect("write");
    assert_eq!(
        Preferences::load_from(Some(file.clone())).language(),
        Some("fr"),
        "witness: the file is read when it is designated"
    );
    let before = std::fs::read_to_string(&file).expect("read");

    // The startup language does not read this file (before any other access
    // to the global loader of this process).
    assert_eq!(
        i18n::language(),
        Language::English,
        "the startup language read the real preferences"
    );

    // `Preferences::load()` designates no file.
    assert_eq!(Preferences::load().language(), None);

    // A state built by `new` does not write there either.
    let session = Session::spawn(Project::new("P").expect("project"), None).expect("session");
    let mut state = AppState::new(session.handle(), String::new()).expect("state");
    state.dispatch(Action::SetObjectManagerOpen(!state.object_manager_open()));
    // Without a path, saving fails (status bar error message)
    // instead of writing elsewhere: this is the intended behavior.
    let _ = Preferences::load().save_language("en");
    assert_eq!(
        std::fs::read_to_string(&file).expect("read"),
        before,
        "the real preferences file was modified"
    );
    drop(state);
    session.shutdown().expect("shutdown");
}
