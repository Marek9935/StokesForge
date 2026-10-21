//! Les tests d'intégration d'`stokesforge-app-state`, lancés seuls, n'atteignent
//! pas les vraies préférences de la machine (#329).
//!
//! `cfg(test)` ne vaut que pour les tests unitaires du crate : un test
//! d'intégration compile la bibliothèque normalement. Sans la feature
//! `test-support` (activée par la dev-dependency de `Cargo.toml`), il lirait la
//! langue du vrai `preferences.json` — d'où un `project_objects` rouge en
//! français — et pourrait l'écrire. En `--workspace`, `stokesforge-app` active la
//! feature pour tout le monde et masque le défaut : seul `cargo test -p
//! stokesforge-app-state` seul le montre. Retirer la dev-dependency fait échouer
//! ce test.

#[test]
fn la_feature_test_support_est_active_dans_les_tests_d_integration() {
    // `Preferences::load()` ne désigne aucun fichier quand le chemin est isolé :
    // sans la feature, il lirait le vrai `preferences.json` de la machine.
    assert_eq!(
        stokesforge_app_state::preferences::Preferences::load().language(),
        None
    );
}
