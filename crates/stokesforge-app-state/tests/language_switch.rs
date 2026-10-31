//! Changing language without restarting (#49).
//!
//! A single test: the language loader is global; the test takes the guard so
//! that no other test of the binary changes it meanwhile.

use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::tr;

#[test]
fn the_language_changes_at_runtime() {
    let _language = i18n::test_language(Language::English);
    assert_eq!(i18n::language(), Language::English);
    assert_eq!(tr!("menu-file"), "File");

    i18n::set_language(Language::French);
    assert_eq!(i18n::language(), Language::French);
    assert_eq!(tr!("menu-file"), "Fichier");
    // French plural: the singular covers 0 and 1.
    assert_eq!(tr!("history-insert-rows", count = 0), "Insérer 0 ligne");
    assert_eq!(tr!("history-insert-rows", count = 2), "Insérer 2 lignes");

    i18n::set_language(Language::PseudoEnglish);
    assert!(tr!("menu-file").starts_with("[["));

    i18n::set_language(Language::English);
    assert_eq!(tr!("menu-file"), "File");
}
