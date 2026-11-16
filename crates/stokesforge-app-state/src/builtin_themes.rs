//! The built-in graph themes (#544, S7 of #514) as the application sees them.
//!
//! The catalogue itself (keys, English names, the nine constructors) lives in
//! [`stokesforge_plot::builtin_themes`], shared with scripts (#546), and is
//! re-exported here at the historical path. What stays here is the name shown
//! to the user, which needs the catalogs: [`display_name`].

use crate::tr;

pub use stokesforge_plot::builtin_themes::{index_of, original, COUNT, KEYS, NAMES};

/// The name shown for the built-in `index`, in the current language (catalog
/// keys `plot-theme-<key>`). Empty past the catalogue.
pub fn display_name(index: usize) -> String {
    display_name_in(crate::i18n::loader(), index)
}

/// [`display_name`] in the language of `loader` (a script names a built-in in
/// any language of the catalogues, #546).
pub fn display_name_in(loader: &i18n_embed::fluent::FluentLanguageLoader, index: usize) -> String {
    match index {
        0 => tr!(in loader; "plot-theme-light"),
        1 => tr!(in loader; "plot-theme-dark"),
        2 => tr!(in loader; "plot-theme-publication"),
        3 => tr!(in loader; "plot-theme-publication-wide"),
        4 => tr!(in loader; "plot-theme-presentation"),
        5 => tr!(in loader; "plot-theme-fit-residuals"),
        6 => tr!(in loader; "plot-theme-kinetics"),
        7 => tr!(in loader; "plot-theme-side-by-side"),
        8 => tr!(in loader; "plot-theme-color-map"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_has_a_name_in_both_catalogs() {
        use crate::i18n::{test_language, Language};
        for language in [Language::English, Language::French] {
            let _guard = test_language(language);
            for (index, key) in KEYS.iter().enumerate() {
                let shown = display_name(index);
                assert!(
                    !shown.is_empty() && !shown.contains("plot-theme"),
                    "{language:?} {key}: {shown:?}"
                );
            }
        }
        let _guard = test_language(Language::English);
        for (index, name) in NAMES.iter().enumerate() {
            assert_eq!(display_name(index), *name, "English shows the stored name");
        }
    }
}
