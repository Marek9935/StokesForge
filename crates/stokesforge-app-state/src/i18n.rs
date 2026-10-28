//! Interface translation machinery: Fluent, embedded catalogs, current
//! language, the [`tr!`](crate::tr!) macro (issue #41, work package #39).
//!
//! # Why Fluent, and why only here
//!
//! English is the **source language**; French is a translation. Fluent (via
//! `i18n-embed` and `i18n-embed-fl`) was chosen because its `fl!` macro checks
//! **at compile time** that a key and its arguments exist in the `en` catalog
//! — `rust-i18n` lets a missing key through and displays it as is on screen.
//! Plurals and agreement are handled by the format (CLDR rules), and a
//! translator edits `.ftl` files without touching Rust.
//!
//! Only the application translates: `fl!` reads the `i18n.toml` of the crate
//! that calls it, and no crate below depends on Fluent (the allowlists of
//! `no_ui_dependency` would refuse it). The crates below return structured
//! data; the application puts it into sentences (#46, #47).
//!
//! # Using it
//!
//! ```ignore
//! use crate::tr;
//! ui.menu_button(tr!("menu-file"), |ui| { /* … */ });
//! ui.label(tr!("history-insert-rows", count = n));
//! ```
//!
//! [`tr!`](crate::tr!) is **the single entry point**: it wraps `fl!` on the
//! global loader ([`loader`]), so a change of machinery does not touch the
//! calls. A key missing from `i18n/en/stokesforge_app.ftl`, or an argument the
//! message does not declare, **does not compile**. Never a literal in a label:
//! `tests/libelles.rs` (`aucun_libelle_en_dur`) guards this.
//!
//! # The language
//!
//! In order of priority ([`choose_language`]): the `STOKESFORGE_LANG`
//! environment variable (`en`, `fr`, `en-XA`), then the `language` preference
//! of `preferences.json` ([`crate::preferences`]), otherwise English. The
//! system language is **not** followed: the interface comes out in English
//! until the French catalog is complete (#49). An unknown value or a corrupted
//! file falls to the next level, never a panic (invariant 10).
//!
//! The language is chosen **once**, on the first read of the global loader.
//! Tests do not change it: they build their own loader ([`build_loader`]) so
//! as not to step on each other.
//!
//! # The `en-XA` pseudo-locale
//!
//! Produced on the fly from `en` ([`pseudo`]): accents, `[[ ]]`, +30%.
//! Launching the application with `STOKESFORGE_LANG=en-XA` shows at a glance
//! what did not go through the catalog (no `[[ ]]`) or is truncated (`]]`
//! missing).
//!
//! # What `fl!` cannot guarantee
//!
//! French may lack a key: the loader then falls back to English
//! (`fallback_language` of `i18n.toml`) — this is intended until #49 is done,
//! and the completeness test *reports* what is missing without failing. In
//! contrast a key **present in `fr` and absent from `en`** is an error, and so
//! are arguments that differ from one language to the other: the tests of this
//! module catch them, not the compiler.

pub mod pseudo;

use std::borrow::Cow;
use std::sync::{LazyLock, RwLock};

use i18n_embed::fluent::{fluent_language_loader, FluentLanguageLoader};
use i18n_embed::unic_langid::LanguageIdentifier;
use i18n_embed::{I18nAssets, LanguageLoader};
use rust_embed::RustEmbed;

/// Environment variable that forces the language (`en`, `fr`, `en-XA`).
pub const LANGUAGE_VARIABLE: &str = "STOKESFORGE_LANG";

/// The catalogs, embedded in the binary (re-read from disk in the debug profile).
#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Embedded;

/// The catalogs as seen by `i18n-embed`: those of the `i18n/` folder, plus the
/// `en-XA` pseudo-locale, which has **no file** and is built from `en`.
struct Catalogs;

/// The pseudo-locale folder in the catalog view.
const PSEUDO_DIRECTORY: &str = "en-XA";

impl I18nAssets for Catalogs {
    fn get_files(&self, file_path: &str) -> Vec<Cow<'_, [u8]>> {
        let Some(name) = file_path
            .strip_prefix(PSEUDO_DIRECTORY)
            .and_then(|rest| rest.strip_prefix('/'))
        else {
            return Embedded.get_files(file_path);
        };
        Embedded
            .get_files(&format!("en/{name}"))
            .into_iter()
            .filter_map(|source| std::str::from_utf8(&source).ok().map(pseudo::localize))
            .map(|text| Cow::Owned(text.into_bytes()))
            .collect()
    }

    fn filenames_iter(&self) -> Box<dyn Iterator<Item = String> + '_> {
        let files: Vec<String> = Embedded.filenames_iter().collect();
        let pseudo: Vec<String> = files
            .iter()
            .filter_map(|file| file.strip_prefix("en/"))
            .map(|name| format!("{PSEUDO_DIRECTORY}/{name}"))
            .collect();
        Box::new(files.into_iter().chain(pseudo))
    }
}

/// An interface language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    /// English: the source language, and the default value.
    #[default]
    English,
    /// French (complete catalog, #49).
    French,
    /// The test pseudo-locale: English accented, lengthened, framed in `[[ ]]`.
    PseudoEnglish,
}

impl Language {
    /// All the languages offered.
    pub const ALL: [Language; 3] = [Language::English, Language::French, Language::PseudoEnglish];

    /// The language tag: `en`, `fr`, `en-XA`.
    pub fn tag(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::French => "fr",
            Language::PseudoEnglish => PSEUDO_DIRECTORY,
        }
    }

    /// Reads a tag written by the user (`STOKESFORGE_LANG`, preference),
    /// ignoring case and `_` versus `-`: `fr`, `FR`, `fr_FR`, `en-xa`. `None`
    /// for anything that is not an offered language.
    pub fn parse(text: &str) -> Option<Self> {
        let normalized = text.trim().replace('_', "-").to_ascii_lowercase();
        match normalized.as_str() {
            "en-xa" => Some(Language::PseudoEnglish),
            "en" => Some(Language::English),
            "fr" => Some(Language::French),
            other if other.starts_with("en-") => Some(Language::English),
            other if other.starts_with("fr-") => Some(Language::French),
            _ => None,
        }
    }

    /// The name of the language **in that language**, as offered in the menu:
    /// never translated, so that it can be found whatever the current language.
    pub fn native_name(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::French => "Français",
            Language::PseudoEnglish => "English (pseudo)",
        }
    }

    fn identifier(self) -> LanguageIdentifier {
        self.tag().parse().unwrap_or_default()
    }
}

/// The language that applies: `environment` (value of `STOKESFORGE_LANG`)
/// first, then `preference` (saved preference), otherwise English. A value
/// that is not an offered language is **skipped**, not an error: a typo in the
/// variable lets the preference apply.
pub fn choose_language(environment: Option<&str>, preference: Option<&str>) -> Language {
    environment
        .and_then(Language::parse)
        .or_else(|| preference.and_then(Language::parse))
        .unwrap_or_default()
}

/// The startup language: the environment variable, then `preferences.json`.
fn startup_language() -> Language {
    let environment = std::env::var(LANGUAGE_VARIABLE).ok();
    // The tests of the crate share this global loader and some (`errors`)
    // switch it to English: the `preferences.json` of whoever runs the tests
    // must not decide the startup language, otherwise they step on each other.
    // Only the environment variable remains (test subprocesses use it).
    //
    // `cfg(test)` holds only for the tests of this crate: those of
    // `stokesforge-app` turn on the `test-support` feature (#263).
    #[cfg(any(test, feature = "test-support"))]
    return choose_language(environment.as_deref(), None);
    #[cfg(not(any(test, feature = "test-support")))]
    choose_language(
        environment.as_deref(),
        crate::preferences::Preferences::load().language(),
    )
}

/// Builds a loader for `language`. English is always loaded behind it
/// (`fallback_language`), so a key missing from a language falls back to
/// English. Never panics: if loading fails, the loader stays on the fallback.
pub fn build_loader(language: Language) -> FluentLanguageLoader {
    let loader = fluent_language_loader!();
    if loader
        .load_languages(&Catalogs, &[language.identifier()])
        .is_err()
    {
        // Cannot happen with embedded catalogs; the error is swallowed rather
        // than crash the interface (invariant 10).
        let _ = loader.load_fallback_language(&Catalogs);
    }
    // No bidirectional isolation marks around arguments: the interface fonts
    // would draw them as squares.
    loader.set_use_isolating(false);
    loader
}

struct Current {
    language: Language,
    loader: FluentLanguageLoader,
}

/// The current language. `egui` redraws all the texts on every frame: a
/// language change therefore applies from the next frame, with no restart.
/// Each change leaks a small loader (a few KB) so that [`loader`] keeps its
/// `&'static`; a user does not change language thousands of times.
static CURRENT: LazyLock<RwLock<&'static Current>> = LazyLock::new(|| {
    let language = startup_language();
    RwLock::new(Box::leak(Box::new(Current {
        language,
        loader: build_loader(language),
    })))
});

fn current() -> &'static Current {
    // A poisoned lock can only come from a panic elsewhere; the value read
    // stays valid.
    CURRENT
        .read()
        .map_or_else(|poisoned| *poisoned.into_inner(), |guard| *guard)
}

/// The global loader, the one used by [`tr!`](crate::tr!).
pub fn loader() -> &'static FluentLanguageLoader {
    &current().loader
}

/// The language of the global loader.
pub fn language() -> Language {
    current().language
}

/// Changes the language of the global loader: the following
/// [`tr!`](crate::tr!) calls speak `language`. Does not touch the preferences
/// ([`Preferences::save_language`]).
pub fn set_language(language: Language) {
    if self::language() == language {
        return;
    }
    let next: &'static Current = Box::leak(Box::new(Current {
        language,
        loader: build_loader(language),
    }));
    match CURRENT.write() {
        Ok(mut guard) => *guard = next,
        Err(poisoned) => *poisoned.into_inner() = next,
    }
}

/// Translates a key of the `en` catalog into the current language.
///
/// - without an argument: `tr!("menu-file")`;
/// - with named arguments: `tr!("history-insert-rows", count = n)`;
/// - in a loader other than the global one: `tr!(in loader; "menu-file")`, for
///   a test that compares a specific language ([`build_loader`]) without
///   depending on the environment's. The same compile-time checks.
///
/// Wraps `i18n_embed_fl::fl!`: an unknown key or an unknown argument **does
/// not compile**. It is the only entry point to use in the interface (see the
/// module doc).
#[macro_export]
macro_rules! tr {
    (in $loader:expr; $message_id:literal) => {
        ::i18n_embed_fl::fl!($loader, $message_id)
    };
    (in $loader:expr; $message_id:literal, $($args:tt)*) => {
        ::i18n_embed_fl::fl!($loader, $message_id, $($args)*)
    };
    ($message_id:literal) => {
        ::i18n_embed_fl::fl!($crate::i18n::loader(), $message_id)
    };
    ($message_id:literal, $($args:tt)*) => {
        ::i18n_embed_fl::fl!($crate::i18n::loader(), $message_id, $($args)*)
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::Preferences;
    use fluent_syntax::ast::{
        CallArguments, Entry, Expression, InlineExpression, Pattern, PatternElement,
    };
    use fluent_syntax::parser;
    use std::collections::{BTreeMap, BTreeSet};

    const EN: &str = include_str!("../i18n/en/stokesforge_app.ftl");
    const FR: &str = include_str!("../i18n/fr/stokesforge_app.ftl");

    // --- Reading a catalog: keys and arguments --------------------------------

    /// The `$name` variables a pattern uses, selectors and variants included.
    fn pattern_variables(pattern: &Pattern<&str>, into: &mut BTreeSet<String>) {
        for element in &pattern.elements {
            if let PatternElement::Placeable { expression } = element {
                expression_variables(expression, into);
            }
        }
    }

    fn arguments_variables(arguments: &CallArguments<&str>, into: &mut BTreeSet<String>) {
        for positional in &arguments.positional {
            inline_variables(positional, into);
        }
        for named in &arguments.named {
            inline_variables(&named.value, into);
        }
    }

    fn inline_variables(expression: &InlineExpression<&str>, into: &mut BTreeSet<String>) {
        match expression {
            InlineExpression::VariableReference { id } => {
                into.insert(id.name.to_string());
            }
            InlineExpression::FunctionReference { arguments, .. } => {
                arguments_variables(arguments, into);
            }
            InlineExpression::TermReference {
                arguments: Some(arguments),
                ..
            } => arguments_variables(arguments, into),
            InlineExpression::Placeable { expression } => expression_variables(expression, into),
            _ => {}
        }
    }

    fn expression_variables(expression: &Expression<&str>, into: &mut BTreeSet<String>) {
        match expression {
            Expression::Select { selector, variants } => {
                inline_variables(selector, into);
                for variant in variants {
                    pattern_variables(&variant.value, into);
                }
            }
            Expression::Inline(inline) => inline_variables(inline, into),
        }
    }

    /// The entries of a catalog: key (`message`, or `message.attribute`) to
    /// its set of arguments, and the pieces Fluent could not read.
    struct Catalog {
        messages: BTreeMap<String, BTreeSet<String>>,
        junk: Vec<String>,
    }

    fn read(source: &str) -> Catalog {
        let resource = match parser::parse(source) {
            Ok(resource) => resource,
            Err((resource, _)) => resource,
        };
        let mut messages = BTreeMap::new();
        let mut junk = Vec::new();
        for entry in &resource.body {
            match entry {
                Entry::Message(message) => {
                    let id = message.id.name.to_string();
                    if let Some(pattern) = &message.value {
                        let mut variables = BTreeSet::new();
                        pattern_variables(pattern, &mut variables);
                        messages.insert(id.clone(), variables);
                    }
                    for attribute in &message.attributes {
                        let mut variables = BTreeSet::new();
                        pattern_variables(&attribute.value, &mut variables);
                        messages.insert(format!("{id}.{}", attribute.id.name), variables);
                    }
                }
                Entry::Junk { content } => junk.push(content.to_string()),
                _ => {}
            }
        }
        Catalog { messages, junk }
    }

    /// What a translation catalog owes to the reference.
    struct Comparison {
        /// Keys written in the translation but absent from `en`: error.
        unknown_in_reference: Vec<String>,
        /// Keys whose arguments differ from those of `en`: error.
        arguments_differ: Vec<String>,
        /// Keys of `en` not yet translated: a report, not an error.
        untranslated: Vec<String>,
    }

    fn compare(reference: &Catalog, translation: &Catalog) -> Comparison {
        let mut unknown_in_reference = Vec::new();
        let mut arguments_differ = Vec::new();
        for (key, variables) in &translation.messages {
            match reference.messages.get(key) {
                None => unknown_in_reference.push(key.clone()),
                Some(expected) if expected != variables => arguments_differ
                    .push(format!("{key}: en {expected:?}, translation {variables:?}")),
                Some(_) => {}
            }
        }
        let untranslated = reference
            .messages
            .keys()
            .filter(|key| !translation.messages.contains_key(*key))
            .cloned()
            .collect();
        Comparison {
            unknown_in_reference,
            arguments_differ,
            untranslated,
        }
    }

    // --- The catalogs --------------------------------------------------------

    #[test]
    fn the_english_catalog_parses_without_error() {
        let en = read(EN);
        assert!(en.junk.is_empty(), "unreadable pieces: {:?}", en.junk);
        assert!(!en.messages.is_empty());
        let fr = read(FR);
        assert!(fr.junk.is_empty(), "unreadable pieces: {:?}", fr.junk);
    }

    /// **Blocking** completeness (#49): every key of `fr` exists in `en` with
    /// the same arguments, and every key of `en` exists in `fr`. A key added to
    /// `en` is added to `fr` in the same commit.
    #[test]
    fn every_french_key_exists_in_english_with_the_same_arguments() {
        let comparison = compare(&read(EN), &read(FR));
        assert!(
            comparison.unknown_in_reference.is_empty(),
            "keys of the fr catalog missing from en (English is the source): {:?}",
            comparison.unknown_in_reference
        );
        assert!(
            comparison.arguments_differ.is_empty(),
            "arguments different from English: {:?}",
            comparison.arguments_differ
        );
        assert!(
            comparison.untranslated.is_empty(),
            "{} key(s) of `en` missing from `fr`: {:?}",
            comparison.untranslated.len(),
            comparison.untranslated
        );
    }

    /// The witness of the completeness test: without it, a comparator that
    /// detects nothing would pass for a conforming catalog.
    #[test]
    fn completeness_detects_an_unknown_key_and_a_different_argument() {
        let en = read("a = A\nb = B { $count }\nc = C\n");
        let fr = read("a = A\nb = B { $nombre }\nz = Z\n");
        let comparison = compare(&en, &fr);
        assert_eq!(comparison.unknown_in_reference, ["z"]);
        assert_eq!(comparison.arguments_differ.len(), 1);
        assert!(comparison.arguments_differ[0].starts_with("b:"));
        assert_eq!(comparison.untranslated, ["c"]);
    }

    #[test]
    fn completeness_reads_the_arguments_of_plurals_and_attributes() {
        let catalogue = read(
            "n =\n    { $count ->\n        [one] { $count } item\n       *[other] { $count } items\n    }\n\
             t = Title\n    .hint = Open { $name }\n",
        );
        assert_eq!(
            catalogue.messages["n"],
            BTreeSet::from(["count".to_string()])
        );
        assert_eq!(
            catalogue.messages["t.hint"],
            BTreeSet::from(["name".to_string()])
        );
        assert!(catalogue.messages["t"].is_empty());
    }

    // --- The loader ----------------------------------------------------------

    #[test]
    fn english_is_the_source_language() {
        let loader = build_loader(Language::English);
        assert_eq!(loader.get("menu-file"), "File");
        assert_eq!(loader.current_language().to_string(), "en");
    }

    #[test]
    fn french_translates_the_catalog() {
        let loader = build_loader(Language::French);
        assert_eq!(loader.current_language().to_string(), "fr");
        assert_eq!(loader.get("menu-file"), "Fichier");
    }

    /// French CLDR rule: the singular holds for 0 **and** 1.
    /// An `fr` preference opens the interface in French at startup (without
    /// `STOKESFORGE_LANG`), and the variable overrides it.
    #[test]
    fn a_french_preference_opens_the_interface_in_french() {
        let language = choose_language(None, Some("fr"));
        assert_eq!(language, Language::French);
        assert_eq!(build_loader(language).get("menu-file"), "Fichier");
        assert_eq!(choose_language(Some("en"), Some("fr")), Language::English);
    }

    #[test]
    fn french_plurals_follow_the_cldr_rule() {
        let loader = build_loader(Language::French);
        let message = |count: i32| {
            loader.get_args(
                "history-insert-rows",
                std::collections::HashMap::from([("count", count)]),
            )
        };
        assert_eq!(message(0), "Insérer 0 ligne");
        assert_eq!(message(1), "Insérer 1 ligne");
        assert_eq!(message(2), "Insérer 2 lignes");
    }

    /// French typography: a non-breaking space before ":" and "?", and inside
    /// the guillemets — never an ordinary space.
    #[test]
    fn french_typography_uses_the_non_breaking_space() {
        for (number, line) in FR.lines().enumerate() {
            if line.starts_with('#') {
                continue;
            }
            for bad in [" :", " ?", " !", "« ", " »"] {
                assert!(
                    !line.contains(bad),
                    "fr:{}: ordinary space before or in « {bad} »: {line}",
                    number + 1
                );
            }
        }
    }

    #[test]
    fn an_argument_and_a_plural_go_through_the_loader() {
        let loader = build_loader(Language::English);
        let message = |count: i32| {
            loader.get_args(
                "history-insert-rows",
                std::collections::HashMap::from([("count", count)]),
            )
        };
        assert_eq!(message(1), "Insert 1 Row");
        assert_eq!(message(3), "Insert 3 Rows");
    }

    #[test]
    fn the_pseudo_locale_frames_and_lengthens_every_message() {
        let english = build_loader(Language::English);
        let pseudo = build_loader(Language::PseudoEnglish);
        assert_eq!(pseudo.current_language().to_string(), "en-XA");
        let keys: Vec<String> = read(EN)
            .messages
            .into_iter()
            .filter(|(key, variables)| variables.is_empty() && !key.contains('.'))
            .map(|(key, _)| key)
            .collect();
        assert!(!keys.is_empty());
        for key in keys {
            let plain = english.get(&key);
            let text = pseudo.get(&key);
            assert!(
                text.starts_with(pseudo::OPEN) && text.ends_with(pseudo::CLOSE),
                "{key}: {text:?} is not framed"
            );
            let inner: String = text
                .trim_start_matches(pseudo::OPEN)
                .trim_end_matches(pseudo::CLOSE)
                .to_string();
            assert!(
                inner.chars().count() * 10 >= plain.chars().count() * 13,
                "{key}: {text:?} is not lengthened by 30% compared with {plain:?}"
            );
            assert_ne!(inner, plain, "{key}: not accented");
        }
    }

    #[test]
    fn the_pseudo_locale_keeps_the_arguments_of_english() {
        let pseudo = build_loader(Language::PseudoEnglish);
        let message = pseudo.get_args(
            "history-insert-rows",
            std::collections::HashMap::from([("count", 3)]),
        );
        assert!(message.starts_with(pseudo::OPEN), "{message}");
        assert!(message.contains(" 3 "), "{message}");
        assert!(message.ends_with(pseudo::CLOSE), "{message}");
    }

    #[test]
    fn the_pseudo_locale_covers_every_key_of_english() {
        let pseudo = build_loader(Language::PseudoEnglish);
        for key in read(EN).messages.keys().filter(|key| !key.contains('.')) {
            assert!(pseudo.has(key), "{key} missing from en-XA");
        }
        // And empty French falls back to English, key by key.
        let french = build_loader(Language::French);
        for key in read(EN).messages.keys().filter(|key| !key.contains('.')) {
            assert!(french.has(key), "{key} does not resolve in fr");
        }
    }

    // --- Choosing the language ----------------------------------------------

    #[test]
    fn a_language_tag_reads_back() {
        for language in Language::ALL {
            assert_eq!(Language::parse(language.tag()), Some(language));
        }
        assert_eq!(Language::parse("FR"), Some(Language::French));
        assert_eq!(Language::parse("fr_FR"), Some(Language::French));
        assert_eq!(Language::parse(" en-us "), Some(Language::English));
        assert_eq!(Language::parse("en_xa"), Some(Language::PseudoEnglish));
        assert_eq!(Language::parse("de"), None);
        assert_eq!(Language::parse(""), None);
        assert_eq!(Language::parse("english"), None);
    }

    #[test]
    fn the_variable_overrides_the_preference_then_english() {
        use Language::{English, French, PseudoEnglish};
        assert_eq!(choose_language(Some("en-XA"), Some("fr")), PseudoEnglish);
        assert_eq!(choose_language(Some("en"), Some("fr")), English);
        assert_eq!(choose_language(None, Some("fr")), French);
        assert_eq!(choose_language(None, None), English);
    }

    #[test]
    fn an_unknown_value_falls_to_the_next_level() {
        use Language::{English, French};
        assert_eq!(choose_language(Some("klingon"), Some("fr")), French);
        assert_eq!(choose_language(Some("klingon"), None), English);
        assert_eq!(choose_language(None, Some("???")), English);
        assert_eq!(choose_language(Some(""), Some("")), English);
    }

    /// The preference is read from `preferences.json`: corrupted, the
    /// application starts in English.
    #[test]
    fn a_corrupted_preference_starts_in_english() {
        let dir = tempfile::tempdir().expect("temporary folder");
        let path = dir.path().join("preferences.json");
        std::fs::write(&path, "{ \"language\": ").expect("write");
        let preferences = Preferences::load_from(Some(path));
        assert_eq!(
            choose_language(None, preferences.language()),
            Language::English
        );

        let path = dir.path().join("valid.json");
        std::fs::write(&path, r#"{ "language": "fr" }"#).expect("write");
        let preferences = Preferences::load_from(Some(path));
        assert_eq!(
            choose_language(None, preferences.language()),
            Language::French
        );
    }

    // --- The macro -----------------------------------------------------------

    /// `tr!` goes through the global loader, whose language depends on the
    /// test environment: so we compare only with what the same language gives
    /// through a separately built loader.
    #[test]
    fn the_macro_translates_with_and_without_an_argument() {
        let reference = build_loader(language());
        assert_eq!(tr!("menu-file"), reference.get("menu-file"));
        assert_eq!(
            tr!("history-insert-rows", count = 2),
            reference.get_args(
                "history-insert-rows",
                std::collections::HashMap::from([("count", 2)])
            )
        );
    }
}
