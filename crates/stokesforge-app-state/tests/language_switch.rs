//! Changing language without restarting (#49).
//!
//! A single test, in its own binary: the language loader is global, and two
//! tests that change it in parallel would step on each other.

use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::tr;

#[test]
fn the_language_changes_at_runtime() {
    i18n::set_language(Language::English);
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
