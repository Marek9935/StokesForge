//! Changer de langue sans redémarrer (#49).
//!
//! Un seul test, dans son propre binaire : le chargeur de langue est global, et
//! deux tests qui le changent en parallèle se marcheraient dessus.

use plotyx_app::i18n::{self, Language};
use plotyx_app::tr;

#[test]
fn the_language_changes_at_runtime() {
    i18n::set_language(Language::English);
    assert_eq!(i18n::language(), Language::English);
    assert_eq!(tr!("menu-file"), "File");

    i18n::set_language(Language::French);
    assert_eq!(i18n::language(), Language::French);
    assert_eq!(tr!("menu-file"), "Fichier");
    // Pluriel français : le singulier vaut pour 0 et 1.
    assert_eq!(tr!("history-insert-rows", count = 0), "Insérer 0 ligne");
    assert_eq!(tr!("history-insert-rows", count = 2), "Insérer 2 lignes");

    i18n::set_language(Language::PseudoEnglish);
    assert!(tr!("menu-file").starts_with("[["));

    i18n::set_language(Language::English);
    assert_eq!(tr!("menu-file"), "File");
}
