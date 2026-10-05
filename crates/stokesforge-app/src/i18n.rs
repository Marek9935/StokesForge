//! Mécanique de traduction de l'interface : Fluent, catalogues embarqués,
//! langue courante, macro [`tr!`](crate::tr!) (issue #41, chantier #39).
//!
//! # Pourquoi Fluent, et pourquoi ici seulement
//!
//! L'anglais est la **langue source** ; le français est une traduction. Fluent
//! (via `i18n-embed` et `i18n-embed-fl`) a été retenu parce que sa macro `fl!`
//! vérifie **à la compilation** qu'une clé et ses arguments existent dans le
//! catalogue `en` — `rust-i18n` laisse passer une clé manquante et l'affiche
//! telle quelle à l'écran. Les pluriels et les accords sont gérés par le format
//! (règles CLDR), et un traducteur édite des `.ftl` sans toucher au Rust.
//!
//! Seule l'application traduit : `fl!` lit l'`i18n.toml` du crate qui l'appelle,
//! et aucun crate du dessous ne dépend de Fluent (les listes blanches de
//! `no_ui_dependency` le refuseraient). Les crates du dessous rendent des
//! données structurées ; l'application les met en phrase (#46, #47).
//!
//! # Utiliser
//!
//! ```ignore
//! use crate::tr;
//! ui.menu_button(tr!("menu-file"), |ui| { /* … */ });
//! ui.label(tr!("history-insert-rows", count = n));
//! ```
//!
//! [`tr!`](crate::tr!) est **l'unique point d'entrée** : il enveloppe `fl!` sur le
//! chargeur global ([`loader`]), pour qu'un changement de mécanique ne touche
//! pas les appels. Une clé absente de `i18n/en/stokesforge_app.ftl`, ou un argument
//! que le message ne déclare pas, **ne compile pas**. Jamais de littéral dans
//! un libellé : `tests/libelles.rs` (`aucun_libelle_en_dur`) le garde.
//!
//! # La langue
//!
//! Par ordre de priorité ([`choose_language`]) : la variable d'environnement
//! `STOKESFORGE_LANG` (`en`, `fr`, `en-XA`), puis la préférence `language` de
//! `preferences.json` ([`crate::preferences`]), sinon l'anglais. On ne suit
//! **pas** la langue du système : l'interface sort en anglais tant que le
//! catalogue français n'est pas complet (#49). Une valeur inconnue ou un
//! fichier corrompu passe au niveau suivant, jamais de panique (invariant 10).
//!
//! La langue est choisie **une fois**, à la première lecture du chargeur global.
//! Les tests ne la changent pas : ils bâtissent leur propre chargeur
//! ([`build_loader`]) pour ne pas se marcher dessus.
//!
//! # La pseudo-langue `en-XA`
//!
//! Produite à la volée depuis `en` ([`pseudo`]) : accents, `[[ ]]`, +30 %. Lancer
//! l'application avec `STOKESFORGE_LANG=en-XA` montre d'un coup d'œil ce qui n'est
//! pas passé par le catalogue (pas de `[[ ]]`) ou qui est tronqué (`]]` absent).
//!
//! # Ce que `fl!` ne peut pas garantir
//!
//! Le français peut manquer d'une clé : le chargeur retombe alors sur l'anglais
//! (`fallback_language` de `i18n.toml`) — c'est voulu tant que #49 n'est pas
//! fait, et le test de complétude *rapporte* ce qui manque sans échouer. En
//! revanche une clé **présente en `fr` et absente de `en`** est une erreur, et
//! des arguments différents d'une langue à l'autre aussi : ce sont les tests de
//! ce module qui les attrapent, pas le compilateur.

pub mod pseudo;

use std::borrow::Cow;
use std::sync::{LazyLock, RwLock};

use i18n_embed::fluent::{fluent_language_loader, FluentLanguageLoader};
use i18n_embed::unic_langid::LanguageIdentifier;
use i18n_embed::{I18nAssets, LanguageLoader};
use rust_embed::RustEmbed;

use crate::preferences::Preferences;

/// Variable d'environnement qui impose la langue (`en`, `fr`, `en-XA`).
pub const LANGUAGE_VARIABLE: &str = "STOKESFORGE_LANG";

/// Les catalogues, embarqués dans le binaire (relus sur disque en profil debug).
#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Embedded;

/// Les catalogues vus par `i18n-embed` : ceux du dossier `i18n/`, plus la
/// pseudo-langue `en-XA` qui n'a **pas de fichier** et se fabrique depuis `en`.
struct Catalogues;

/// Le dossier de la pseudo-langue dans la vue des catalogues.
const PSEUDO_DIRECTORY: &str = "en-XA";

impl I18nAssets for Catalogues {
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

/// Une langue de l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    /// L'anglais : la langue source, et la valeur par défaut.
    #[default]
    English,
    /// Le français (catalogue complet, #49).
    French,
    /// La pseudo-langue de test : l'anglais accentué, allongé, encadré de `[[ ]]`.
    PseudoEnglish,
}

impl Language {
    /// Toutes les langues proposées.
    pub const ALL: [Language; 3] = [Language::English, Language::French, Language::PseudoEnglish];

    /// L'étiquette de la langue : `en`, `fr`, `en-XA`.
    pub fn tag(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::French => "fr",
            Language::PseudoEnglish => PSEUDO_DIRECTORY,
        }
    }

    /// Lit une étiquette écrite par l'utilisateur (`STOKESFORGE_LANG`, préférence),
    /// sans tenir compte de la casse ni de `_` contre `-` : `fr`, `FR`, `fr_FR`,
    /// `en-xa`. `None` pour tout ce qui n'est pas une langue proposée.
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

    /// Le nom de la langue **dans cette langue**, tel qu'on le propose dans le
    /// menu : jamais traduit, pour qu'on la retrouve quelle que soit la langue
    /// courante.
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

/// La langue qui s'applique : `environment` (valeur de `STOKESFORGE_LANG`) d'abord,
/// puis `preference` (préférence enregistrée), sinon l'anglais. Une valeur qui
/// n'est pas une langue proposée est **sautée**, pas une erreur : une faute de
/// frappe dans la variable laisse la préférence jouer.
pub fn choose_language(environment: Option<&str>, preference: Option<&str>) -> Language {
    environment
        .and_then(Language::parse)
        .or_else(|| preference.and_then(Language::parse))
        .unwrap_or_default()
}

/// La langue du démarrage : la variable d'environnement, puis `preferences.json`.
fn startup_language() -> Language {
    let environment = std::env::var(LANGUAGE_VARIABLE).ok();
    // Les tests du crate partagent ce chargeur global et certains (`errors`) le
    // passent en anglais : les `preferences.json` de celui qui teste ne doivent
    // pas décider de la langue de départ, sinon ils se marchent dessus. Seule la
    // variable d'environnement reste (les sous-processus de test s'en servent).
    #[cfg(test)]
    return choose_language(environment.as_deref(), None);
    #[cfg(not(test))]
    choose_language(environment.as_deref(), Preferences::load().language())
}

/// Bâtit un chargeur pour `language`. L'anglais est toujours chargé derrière
/// (`fallback_language`), donc une clé absente d'une langue reprend l'anglais.
/// Ne panique jamais : si le chargement échoue, le chargeur reste sur le repli.
pub fn build_loader(language: Language) -> FluentLanguageLoader {
    let loader = fluent_language_loader!();
    if loader
        .load_languages(&Catalogues, &[language.identifier()])
        .is_err()
    {
        // Ne peut pas arriver avec des catalogues embarqués ; l'erreur est
        // avalée plutôt que de planter l'interface (invariant 10).
        let _ = loader.load_fallback_language(&Catalogues);
    }
    // Pas de marques d'isolation bidirectionnelle autour des arguments : les
    // polices de l'interface les dessineraient comme des carrés.
    loader.set_use_isolating(false);
    loader
}

struct Current {
    language: Language,
    loader: FluentLanguageLoader,
}

/// La langue courante. `egui` redessine tous les textes à chaque image : un
/// changement de langue s'applique donc dès l'image suivante, sans redémarrer.
/// Chaque changement fait fuir un petit chargeur (quelques Ko) pour que
/// [`loader`] garde son `&'static` ; un utilisateur ne change pas de langue des
/// milliers de fois.
static CURRENT: LazyLock<RwLock<&'static Current>> = LazyLock::new(|| {
    let language = startup_language();
    RwLock::new(Box::leak(Box::new(Current {
        language,
        loader: build_loader(language),
    })))
});

fn current() -> &'static Current {
    // Un verrou empoisonné ne peut venir que d'un panic ailleurs ; la valeur
    // lue reste valide.
    CURRENT
        .read()
        .map_or_else(|poisoned| *poisoned.into_inner(), |guard| *guard)
}

/// Le chargeur global, celui de [`tr!`](crate::tr!).
pub fn loader() -> &'static FluentLanguageLoader {
    &current().loader
}

/// La langue du chargeur global.
pub fn language() -> Language {
    current().language
}

/// Change la langue du chargeur global : les [`tr!`](crate::tr!) suivants parlent
/// `language`. Ne touche pas aux préférences ([`Preferences::save_language`]).
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

/// Traduit une clé du catalogue `en` dans la langue courante.
///
/// - sans argument : `tr!("menu-file")` ;
/// - avec arguments nommés : `tr!("history-insert-rows", count = n)` ;
/// - dans un autre chargeur que le global : `tr!(in loader; "menu-file")`, pour
///   un test qui compare une langue précise ([`build_loader`]) sans dépendre de
///   celle de l'environnement. Les mêmes vérifications à la compilation.
///
/// Enveloppe `i18n_embed_fl::fl!` : une clé inconnue ou un argument inconnu
/// **ne compile pas**. C'est le seul point d'entrée à employer dans
/// l'interface (voir la doc du module).
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
    use fluent_syntax::ast::{
        CallArguments, Entry, Expression, InlineExpression, Pattern, PatternElement,
    };
    use fluent_syntax::parser;
    use std::collections::{BTreeMap, BTreeSet};

    const EN: &str = include_str!("../i18n/en/stokesforge_app.ftl");
    const FR: &str = include_str!("../i18n/fr/stokesforge_app.ftl");

    // --- Lecture d'un catalogue : clés et arguments --------------------------

    /// Les variables `$nom` qu'un motif emploie, sélecteurs et variantes compris.
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

    /// Les entrées d'un catalogue : clé (`message`, ou `message.attribut`) vers
    /// l'ensemble de ses arguments, et les morceaux que Fluent n'a pas pu lire.
    struct Catalogue {
        messages: BTreeMap<String, BTreeSet<String>>,
        junk: Vec<String>,
    }

    fn read(source: &str) -> Catalogue {
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
        Catalogue { messages, junk }
    }

    /// Ce qu'un catalogue de traduction doit à la référence.
    struct Comparison {
        /// Clés écrites dans la traduction mais absentes de `en` : erreur.
        unknown_in_reference: Vec<String>,
        /// Clés dont les arguments diffèrent de ceux de `en` : erreur.
        arguments_differ: Vec<String>,
        /// Clés de `en` pas encore traduites : un rapport, pas une erreur.
        untranslated: Vec<String>,
    }

    fn compare(reference: &Catalogue, translation: &Catalogue) -> Comparison {
        let mut unknown_in_reference = Vec::new();
        let mut arguments_differ = Vec::new();
        for (key, variables) in &translation.messages {
            match reference.messages.get(key) {
                None => unknown_in_reference.push(key.clone()),
                Some(expected) if expected != variables => arguments_differ
                    .push(format!("{key} : en {expected:?}, traduction {variables:?}")),
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

    // --- Les catalogues ------------------------------------------------------

    #[test]
    fn le_catalogue_anglais_s_analyse_sans_erreur() {
        let en = read(EN);
        assert!(en.junk.is_empty(), "morceaux illisibles : {:?}", en.junk);
        assert!(!en.messages.is_empty());
        let fr = read(FR);
        assert!(fr.junk.is_empty(), "morceaux illisibles : {:?}", fr.junk);
    }

    /// Complétude **bloquante** (#49) : toute clé de `fr` existe dans `en` avec
    /// les mêmes arguments, et toute clé de `en` existe dans `fr`. Une clé
    /// ajoutée à `en` s'ajoute à `fr` dans le même commit.
    #[test]
    fn toute_cle_francaise_existe_en_anglais_avec_les_memes_arguments() {
        let comparison = compare(&read(EN), &read(FR));
        assert!(
            comparison.unknown_in_reference.is_empty(),
            "clés du catalogue fr absentes de en (l'anglais est la source) : {:?}",
            comparison.unknown_in_reference
        );
        assert!(
            comparison.arguments_differ.is_empty(),
            "arguments différents de l'anglais : {:?}",
            comparison.arguments_differ
        );
        assert!(
            comparison.untranslated.is_empty(),
            "{} clé(s) de `en` absente(s) de `fr` : {:?}",
            comparison.untranslated.len(),
            comparison.untranslated
        );
    }

    /// Le témoin du test de complétude : sans lui, un comparateur qui ne
    /// détecte rien passerait pour un catalogue conforme.
    #[test]
    fn la_completude_detecte_une_cle_inconnue_et_un_argument_different() {
        let en = read("a = A\nb = B { $count }\nc = C\n");
        let fr = read("a = A\nb = B { $nombre }\nz = Z\n");
        let comparison = compare(&en, &fr);
        assert_eq!(comparison.unknown_in_reference, ["z"]);
        assert_eq!(comparison.arguments_differ.len(), 1);
        assert!(comparison.arguments_differ[0].starts_with("b :"));
        assert_eq!(comparison.untranslated, ["c"]);
    }

    #[test]
    fn la_completude_lit_les_arguments_des_pluriels_et_des_attributs() {
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

    // --- Le chargeur ---------------------------------------------------------

    #[test]
    fn l_anglais_est_la_langue_source() {
        let loader = build_loader(Language::English);
        assert_eq!(loader.get("menu-file"), "File");
        assert_eq!(loader.get("window-title"), "StokesForge");
        assert_eq!(loader.current_language().to_string(), "en");
    }

    #[test]
    fn le_francais_traduit_le_catalogue() {
        let loader = build_loader(Language::French);
        assert_eq!(loader.current_language().to_string(), "fr");
        assert_eq!(loader.get("menu-file"), "Fichier");
        assert_eq!(loader.get("window-title"), "StokesForge");
    }

    /// Règle CLDR française : le singulier vaut pour 0 **et** 1.
    /// Une préférence `fr` ouvre l'interface en français au démarrage (sans
    /// `STOKESFORGE_LANG`), et la variable l'emporte sur elle.
    #[test]
    fn une_preference_francaise_ouvre_l_interface_en_francais() {
        let language = choose_language(None, Some("fr"));
        assert_eq!(language, Language::French);
        assert_eq!(build_loader(language).get("menu-file"), "Fichier");
        assert_eq!(choose_language(Some("en"), Some("fr")), Language::English);
    }

    #[test]
    fn les_pluriels_francais_suivent_la_regle_cldr() {
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

    /// Typographie française : une espace insécable avant « : » et « ? », et
    /// dans les guillemets — jamais une espace ordinaire.
    #[test]
    fn la_typographie_francaise_utilise_l_espace_insecable() {
        for (number, line) in FR.lines().enumerate() {
            if line.starts_with('#') {
                continue;
            }
            for bad in [" :", " ?", " !", "« ", " »"] {
                assert!(
                    !line.contains(bad),
                    "fr:{} : espace ordinaire avant ou dans « {bad} » : {line}",
                    number + 1
                );
            }
        }
    }

    #[test]
    fn un_argument_et_un_pluriel_passent_par_le_chargeur() {
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
    fn la_pseudo_langue_entoure_et_allonge_chaque_message() {
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
                "{key} : {text:?} n'est pas encadré"
            );
            let inner: String = text
                .trim_start_matches(pseudo::OPEN)
                .trim_end_matches(pseudo::CLOSE)
                .to_string();
            assert!(
                inner.chars().count() * 10 >= plain.chars().count() * 13,
                "{key} : {text:?} n'est pas allongé de 30 % par rapport à {plain:?}"
            );
            assert_ne!(inner, plain, "{key} : pas accentué");
        }
    }

    #[test]
    fn la_pseudo_langue_garde_les_arguments_de_l_anglais() {
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
    fn la_pseudo_langue_couvre_toutes_les_cles_de_l_anglais() {
        let pseudo = build_loader(Language::PseudoEnglish);
        for key in read(EN).messages.keys().filter(|key| !key.contains('.')) {
            assert!(pseudo.has(key), "{key} manque en en-XA");
        }
        // Et le français vide reprend l'anglais, clé par clé.
        let french = build_loader(Language::French);
        for key in read(EN).messages.keys().filter(|key| !key.contains('.')) {
            assert!(french.has(key), "{key} ne se résout pas en fr");
        }
    }

    // --- Le choix de la langue ----------------------------------------------

    #[test]
    fn l_etiquette_d_une_langue_se_relit() {
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
    fn la_variable_l_emporte_sur_la_preference_puis_l_anglais() {
        use Language::{English, French, PseudoEnglish};
        assert_eq!(choose_language(Some("en-XA"), Some("fr")), PseudoEnglish);
        assert_eq!(choose_language(Some("en"), Some("fr")), English);
        assert_eq!(choose_language(None, Some("fr")), French);
        assert_eq!(choose_language(None, None), English);
    }

    #[test]
    fn une_valeur_inconnue_passe_au_niveau_suivant() {
        use Language::{English, French};
        assert_eq!(choose_language(Some("klingon"), Some("fr")), French);
        assert_eq!(choose_language(Some("klingon"), None), English);
        assert_eq!(choose_language(None, Some("???")), English);
        assert_eq!(choose_language(Some(""), Some("")), English);
    }

    /// La préférence est lue dans `preferences.json` : corrompue, l'application
    /// démarre en anglais.
    #[test]
    fn une_preference_corrompue_demarre_en_anglais() {
        let dir = tempfile::tempdir().expect("dossier temporaire");
        let path = dir.path().join("preferences.json");
        std::fs::write(&path, "{ \"language\": ").expect("écriture");
        let preferences = Preferences::load_from(Some(path));
        assert_eq!(
            choose_language(None, preferences.language()),
            Language::English
        );

        let path = dir.path().join("valide.json");
        std::fs::write(&path, r#"{ "language": "fr" }"#).expect("écriture");
        let preferences = Preferences::load_from(Some(path));
        assert_eq!(
            choose_language(None, preferences.language()),
            Language::French
        );
    }

    // --- La macro ------------------------------------------------------------

    /// `tr!` passe par le chargeur global, dont la langue dépend de
    /// l'environnement du test : on ne compare donc qu'à ce que la même langue
    /// donne par un chargeur construit à part.
    #[test]
    fn la_macro_traduit_avec_et_sans_argument() {
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

    /// Une clé du catalogue `en` que le code n'emploie nulle part est une clé
    /// morte (un texte qu'on croit visible, ou le reste d'une conversion) : elle
    /// fait échouer ce test (#48). Une clé est employée quand son nom, entre
    /// guillemets, figure dans une source de `src/` — la liste récursive
    /// partagée par toutes les gardes qui lisent le code (#116).
    #[test]
    fn aucune_cle_du_catalogue_n_est_morte() {
        let sources: String = crate::sources::sources()
            .into_iter()
            .map(|fichier| fichier.texte)
            .collect();
        let mortes: Vec<String> = read(EN)
            .messages
            .keys()
            .filter(|key| !key.contains('.'))
            .filter(|key| !sources.contains(&format!("\"{key}\"")))
            .cloned()
            .collect();
        assert!(
            mortes.is_empty(),
            "{} clé(s) de `en` sans emploi dans `src/` : {mortes:?}",
            mortes.len()
        );
    }
}
