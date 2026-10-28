//! The integration tests of `stokesforge-app-state`, run alone, do not reach
//! the machine's real preferences (#329).
//!
//! `cfg(test)` only holds for the crate's unit tests: an integration test
//! compiles the library normally. Without the `test-support` feature (enabled
//! by the dev-dependency in `Cargo.toml`), it would read the language from the
//! real `preferences.json` (hence a red `project_objects` in French) and might
//! write it. With `--workspace`, `stokesforge-app` enables the feature for
//! everyone and hides the flaw: only `cargo test -p stokesforge-app-state`
//! alone shows it. Removing the dev-dependency makes this test fail.

#[test]
fn the_test_support_feature_is_enabled_in_integration_tests() {
    // `Preferences::load()` points to no file when the path is isolated:
    // without the feature, it would read the machine's real `preferences.json`.
    assert_eq!(
        stokesforge_app_state::preferences::Preferences::load().language(),
        None
    );
}
