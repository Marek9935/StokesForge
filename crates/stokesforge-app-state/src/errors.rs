//! Les erreurs des crates du dessous, mises en phrase dans la langue de
//! l'interface (issue #47, chantier #39).
//!
//! # Le partage des rôles
//!
//! Les crates du dessous (`core`, `commands`, `persistence`, `import`,
//! `script`…) rendent des erreurs **structurées** : une variante par cause, ses
//! champs en données. Leur `Display` (`#[error(...)]`) est en **anglais**, brut :
//! c'est ce que lisent le CLI, les scripts `pyst` et les journaux, et il n'est
//! jamais traduit — aucun de ces crates ne dépend de Fluent. L'application, elle,
//! associe chaque variante à une clé `error-*` du catalogue (`i18n/en/
//! stokesforge_app.ftl`) et affiche la phrase du catalogue, dans la langue choisie.
//!
//! Une fonction `error_message` par famille d'erreurs, chacune dans son
//! sous-module : [`model`], [`commands`], [`session`], [`file`], [`import`],
//! [`formula`], [`render`], [`export`], [`io`] et, avec la feature `python`, `script`.
//! Toute erreur **affichée** à l'utilisateur passe par l'une d'elles, jamais par
//! `to_string()`.
//!
//! # Ajouter une variante d'erreur
//!
//! Le `match` de chaque fonction est **exhaustif, sans `_`** : une variante
//! nouvelle ne compile pas tant qu'elle n'est pas traitée ici. Deux traitements :
//!
//! - lui donner une clé `error-<famille>-<cause>` (texte selon le glossaire et
//!   `docs/textes-interface.md` : ce qui s'est passé, puis quoi faire, sans jargon
//!   interne — pas de *nœud*, *session*, *macro*, *commande*) et l'employer par
//!   [`crate::tr!`] avec ses champs en arguments nommés ;
//! - ou, à défaut, la laisser retomber sur son `Display` anglais par
//!   [`fallback`] : rien ne disparaît, mais le texte n'est pas traduit.
//!
//! Le test de chaque famille construit **une valeur de chaque variante** et
//! échoue si l'une d'elles n'est pas listée (un `match` sans `_` dans le test) ou
//! si son message est du jargon interne.
//!
//! # Ce qui reste brut
//!
//! Les champs `detail` (une cause décrite en anglais par le crate d'origine : la
//! raison d'un nom refusé, la ligne d'un fichier corrompu) et les messages du
//! système (`std::io::Error`) sont repris tels quels dans la phrase : ils sont en
//! anglais quelle que soit la langue de l'interface. Les structurer davantage
//! reviendrait à dupliquer la logique des crates du dessous.

use std::fmt::Display;

/// Le texte brut anglais d'une erreur, pour une variante sans clé de catalogue.
///
/// Rien ne disparaît : une cause que personne n'a encore mise en phrase
/// s'affiche par son `Display`, qui est lisible.
pub fn fallback(error: &dyn Display) -> String {
    error.to_string()
}

/// Erreurs du modèle de données (`stokesforge-core`).
pub mod model {
    use stokesforge_core::CoreError;

    use crate::tr;

    /// La phrase d'une [`CoreError`].
    pub fn error_message(error: &CoreError) -> String {
        match error {
            CoreError::InvalidName(detail) => {
                tr!("error-model-invalid-name", detail = detail.as_str())
            }
            CoreError::DuplicateName(detail) => {
                tr!("error-model-duplicate-name", detail = detail.as_str())
            }
            CoreError::Path(detail) => tr!("error-model-invalid-path", detail = detail.as_str()),
            CoreError::Structure(detail) => {
                tr!("error-model-invalid-structure", detail = detail.as_str())
            }
            CoreError::UnknownNode(_) => tr!("error-model-unknown-object"),
            CoreError::IndexOutOfBounds(detail) => {
                tr!("error-model-index-out-of-bounds", detail = detail.as_str())
            }
            CoreError::TypeMismatch(detail) => {
                tr!("error-model-incompatible-values", detail = detail.as_str())
            }
            CoreError::IdentifiersExhausted => tr!("error-model-ids-exhausted"),
            CoreError::ConcurrentModification { .. } => tr!("error-model-concurrent-edit"),
            CoreError::FormulaColumnReadOnly { .. } => {
                tr!("error-model-formula-column-read-only")
            }
            CoreError::CalculatedColumnReadOnly { .. } => {
                tr!("error-model-calculated-column-read-only")
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use stokesforge_core::NodeId;

        /// Une valeur de chaque variante ; le `match` sans `_` oblige à en
        /// ajouter une pour chaque variante nouvelle.
        fn every_variant() -> Vec<CoreError> {
            let samples = vec![
                CoreError::InvalidName("the name is empty".into()),
                CoreError::DuplicateName("Sheet1 in Book1".into()),
                CoreError::Path("empty path".into()),
                CoreError::Structure("a folder cannot hold a column".into()),
                CoreError::UnknownNode(NodeId::from_raw(7)),
                CoreError::IndexOutOfBounds("row 5".into()),
                CoreError::TypeMismatch("text expected".into()),
                CoreError::IdentifiersExhausted,
                CoreError::ConcurrentModification {
                    column: NodeId::from_raw(7),
                    expected: 1,
                    found: 2,
                },
                CoreError::FormulaColumnReadOnly {
                    column: NodeId::from_raw(7),
                },
                CoreError::CalculatedColumnReadOnly {
                    column: NodeId::from_raw(7),
                    calculation: NodeId::from_raw(8),
                },
            ];
            let mut seen = [false; 11];
            for sample in &samples {
                let index = match sample {
                    CoreError::InvalidName(_) => 0,
                    CoreError::DuplicateName(_) => 1,
                    CoreError::Path(_) => 2,
                    CoreError::Structure(_) => 3,
                    CoreError::UnknownNode(_) => 4,
                    CoreError::IndexOutOfBounds(_) => 5,
                    CoreError::TypeMismatch(_) => 6,
                    CoreError::IdentifiersExhausted => 7,
                    CoreError::ConcurrentModification { .. } => 8,
                    CoreError::FormulaColumnReadOnly { .. } => 9,
                    CoreError::CalculatedColumnReadOnly { .. } => 10,
                };
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s), "une variante n'a pas d'exemple");
            samples
        }

        #[test]
        fn chaque_variante_a_sa_phrase_et_pas_de_jargon() {
            for error in every_variant() {
                crate::errors::tests_support::check(&error, &error_message(&error));
            }
        }

        #[test]
        fn les_champs_entrent_dans_la_phrase() {
            // Ces phrases sont comparées en anglais : la langue ne dépend pas
            // de l\'environnement (`STOKESFORGE_LANG`, préférences) de celui qui teste.
            crate::i18n::set_language(crate::i18n::Language::English);
            assert_eq!(
                error_message(&CoreError::DuplicateName("Sheet1 in Book1".into())),
                "Name already used: Sheet1 in Book1"
            );
            assert_eq!(
                error_message(&CoreError::UnknownNode(NodeId::from_raw(7))),
                "This object no longer exists in the project"
            );
        }
    }
}

/// Erreurs du bus de commandes (`stokesforge-commands`).
pub mod commands {
    use stokesforge_commands::CommandError;

    use crate::tr;

    /// La phrase d'une [`CommandError`].
    pub fn error_message(error: &CommandError) -> String {
        match error {
            CommandError::Core(error) => super::model::error_message(error),
            CommandError::NotInMacro => tr!("error-command-no-group"),
            CommandError::MacroOpen(_) => tr!("error-command-busy"),
            CommandError::StackNotEmpty => tr!("error-command-limit-not-empty"),
            CommandError::MacroTooDeep(limit) => {
                tr!("error-command-group-too-deep", limit = (*limit as i64))
            }
            CommandError::ForeignMacro(_) => tr!("error-command-foreign-group"),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use stokesforge_core::CoreError;

        fn every_variant() -> Vec<CommandError> {
            let samples = vec![
                CommandError::Core(CoreError::IdentifiersExhausted),
                CommandError::NotInMacro,
                CommandError::MacroOpen("undo"),
                CommandError::StackNotEmpty,
                CommandError::MacroTooDeep(64),
                CommandError::ForeignMacro("undo"),
            ];
            let mut seen = [false; 6];
            for sample in &samples {
                let index = match sample {
                    CommandError::Core(_) => 0,
                    CommandError::NotInMacro => 1,
                    CommandError::MacroOpen(_) => 2,
                    CommandError::StackNotEmpty => 3,
                    CommandError::MacroTooDeep(_) => 4,
                    CommandError::ForeignMacro(_) => 5,
                };
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s), "une variante n'a pas d'exemple");
            samples
        }

        #[test]
        fn chaque_variante_a_sa_phrase_et_pas_de_jargon() {
            for error in every_variant() {
                crate::errors::tests_support::check(&error, &error_message(&error));
            }
        }

        #[test]
        fn une_erreur_du_modele_garde_sa_phrase() {
            // Ces phrases sont comparées en anglais : la langue ne dépend pas
            // de l\'environnement (`STOKESFORGE_LANG`, préférences) de celui qui teste.
            crate::i18n::set_language(crate::i18n::Language::English);
            let core = CoreError::IdentifiersExhausted;
            assert_eq!(
                error_message(&CommandError::Core(core.clone())),
                super::super::model::error_message(&core)
            );
            assert_eq!(
                error_message(&CommandError::MacroTooDeep(64)),
                "Grouped actions are nested too deeply (limit: 64)"
            );
        }
    }
}

/// Erreurs de la session (`stokesforge-session`) : celles du bus et du fichier,
/// transmises sans réécriture.
pub mod session {
    use stokesforge_session::SessionError;

    use crate::tr;

    /// La phrase d'une [`SessionError`].
    pub fn error_message(error: &SessionError) -> String {
        match error {
            SessionError::Command(error) => super::commands::error_message(error),
            SessionError::Persistence(error) => super::file::error_message(error),
            SessionError::Disconnected => tr!("error-session-closed"),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use stokesforge_commands::CommandError;

        fn every_variant() -> Vec<SessionError> {
            let samples = vec![
                SessionError::Command(CommandError::StackNotEmpty),
                SessionError::Persistence(
                    stokesforge_persistence::PersistenceError::NotAStokesForgeFile {
                        path: "/data/a.txt".into(),
                    },
                ),
                SessionError::Disconnected,
            ];
            let mut seen = [false; 3];
            for sample in &samples {
                let index = match sample {
                    SessionError::Command(_) => 0,
                    SessionError::Persistence(_) => 1,
                    SessionError::Disconnected => 2,
                };
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s), "une variante n'a pas d'exemple");
            samples
        }

        #[test]
        fn chaque_variante_a_sa_phrase_et_pas_de_jargon() {
            for error in every_variant() {
                crate::errors::tests_support::check(&error, &error_message(&error));
            }
        }
    }
}

/// Erreurs de lecture et d'écriture d'un fichier `.stf` (`stokesforge-persistence`).
pub mod file {
    use stokesforge_persistence::PersistenceError;

    use crate::tr;

    /// La phrase d'une [`PersistenceError`].
    pub fn error_message(error: &PersistenceError) -> String {
        match error {
            PersistenceError::Io { path, source } => tr!(
                "error-file-io",
                path = path.display().to_string(),
                detail = source.to_string()
            ),
            PersistenceError::NotAStokesForgeFile { path } => {
                tr!(
                    "error-file-not-stokesforge",
                    path = path.display().to_string()
                )
            }
            PersistenceError::UnsupportedVersion {
                path,
                found,
                supported,
            } => tr!(
                "error-file-too-new",
                path = path.display().to_string(),
                found = i64::from(*found),
                supported = i64::from(*supported)
            ),
            PersistenceError::Corrupt { path, detail } => tr!(
                "error-file-corrupt",
                path = path.display().to_string(),
                detail = detail.as_str()
            ),
            PersistenceError::InvalidModel { path, source } => tr!(
                "error-file-invalid-project",
                path = path.display().to_string(),
                detail = super::model::error_message(source)
            ),
            PersistenceError::UnknownVariant { path, detail } => tr!(
                "error-file-unknown-feature",
                path = path.display().to_string(),
                detail = detail.as_str()
            ),
            PersistenceError::ConcurrentModification { path, .. } => {
                tr!("error-file-changed", path = path.display().to_string())
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use stokesforge_core::CoreError;

        fn every_variant() -> Vec<PersistenceError> {
            let path = || std::path::PathBuf::from("/data/run.stf");
            let samples = vec![
                PersistenceError::Io {
                    path: path(),
                    source: std::io::Error::other("disk full"),
                },
                PersistenceError::NotAStokesForgeFile { path: path() },
                PersistenceError::UnsupportedVersion {
                    path: path(),
                    found: 9,
                    supported: 2,
                },
                PersistenceError::Corrupt {
                    path: path(),
                    detail: "header truncated".into(),
                },
                PersistenceError::InvalidModel {
                    path: path(),
                    source: CoreError::IdentifiersExhausted,
                },
                PersistenceError::UnknownVariant {
                    path: path(),
                    detail: "unknown variant `Polar`".into(),
                },
                PersistenceError::ConcurrentModification {
                    path: path(),
                    expected: 1,
                    found: 2,
                },
            ];
            let mut seen = [false; 7];
            for sample in &samples {
                let index = match sample {
                    PersistenceError::Io { .. } => 0,
                    PersistenceError::NotAStokesForgeFile { .. } => 1,
                    PersistenceError::UnsupportedVersion { .. } => 2,
                    PersistenceError::Corrupt { .. } => 3,
                    PersistenceError::InvalidModel { .. } => 4,
                    PersistenceError::UnknownVariant { .. } => 5,
                    PersistenceError::ConcurrentModification { .. } => 6,
                };
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s), "une variante n'a pas d'exemple");
            samples
        }

        #[test]
        fn chaque_variante_a_sa_phrase_et_pas_de_jargon() {
            for error in every_variant() {
                crate::errors::tests_support::check(&error, &error_message(&error));
            }
        }

        #[test]
        fn les_champs_entrent_dans_la_phrase() {
            // Ces phrases sont comparées en anglais : la langue ne dépend pas
            // de l\'environnement (`STOKESFORGE_LANG`, préférences) de celui qui teste.
            crate::i18n::set_language(crate::i18n::Language::English);
            assert_eq!(
                error_message(&PersistenceError::NotAStokesForgeFile {
                    path: "/data/run.stf".into()
                }),
                "/data/run.stf is not a StokesForge file"
            );
            assert_eq!(
                error_message(&PersistenceError::UnsupportedVersion {
                    path: "/data/run.stf".into(),
                    found: 9,
                    supported: 2
                }),
                "/data/run.stf needs a newer version of StokesForge (file format 9, supported up to 2)"
            );
        }
    }
}

/// Erreurs d'analyse d'un fichier d'appareil (`stokesforge-import`,
/// `stokesforge-importer`).
pub mod import {
    use stokesforge_import::ImportError;
    use stokesforge_importer::{ImportFailure, ImportFailureCause};

    use crate::state::ImportReadError;

    use crate::tr;

    /// La phrase d'une [`ImportError`].
    pub fn error_message(error: &ImportError) -> String {
        match error {
            ImportError::NotText => tr!("error-import-binary"),
            ImportError::Empty => tr!("error-import-empty"),
            ImportError::NoDataRows => tr!("error-import-no-data"),
        }
    }

    /// La phrase d'une [`ImportReadError`] de l'assistant d'import.
    pub fn read_message(error: &ImportReadError) -> String {
        match error {
            ImportReadError::NoImport => tr!("error-import-not-open"),
            ImportReadError::Read { path, error } => tr!(
                "error-import-file",
                path = path.display().to_string(),
                detail = super::io::error_message(error)
            ),
            ImportReadError::Parse(error) => error_message(error),
        }
    }

    /// La phrase d'un [`ImportFailure`] : le fichier, puis ce qui s'est passé.
    pub fn failure_message(failure: &ImportFailure) -> String {
        let detail = match &failure.cause {
            ImportFailureCause::Read(message) => super::fallback(message),
            ImportFailureCause::Parse(error) => error_message(error),
        };
        tr!(
            "error-import-file",
            path = failure.path.display().to_string(),
            detail = detail
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn every_variant() -> Vec<ImportError> {
            let samples = vec![
                ImportError::NotText,
                ImportError::Empty,
                ImportError::NoDataRows,
            ];
            let mut seen = [false; 3];
            for sample in &samples {
                let index = match sample {
                    ImportError::NotText => 0,
                    ImportError::Empty => 1,
                    ImportError::NoDataRows => 2,
                };
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s), "une variante n'a pas d'exemple");
            samples
        }

        #[test]
        fn chaque_variante_a_sa_phrase_et_pas_de_jargon() {
            for error in every_variant() {
                crate::errors::tests_support::check(&error, &error_message(&error));
            }
        }

        #[test]
        fn un_fichier_en_echec_nomme_le_fichier_et_la_cause() {
            // Ces phrases sont comparées en anglais : la langue ne dépend pas
            // de l\'environnement (`STOKESFORGE_LANG`, préférences) de celui qui teste.
            crate::i18n::set_language(crate::i18n::Language::English);
            let failure = ImportFailure {
                path: "/data/a.csv".into(),
                cause: ImportFailureCause::Parse(ImportError::Empty),
            };
            assert_eq!(failure_message(&failure), "/data/a.csv: The file is empty");
            let failure = ImportFailure {
                path: "/data/a.csv".into(),
                cause: ImportFailureCause::Read("No such file or directory".into()),
            };
            assert_eq!(
                failure_message(&failure),
                "/data/a.csv: No such file or directory"
            );
        }
    }
}

/// Erreurs d'une formule de colonne calculée (`stokesforge-core::formula`).
pub mod formula {
    use stokesforge_core::FormulaError;

    use crate::tr;

    /// La phrase d'une [`FormulaError`].
    pub fn error_message(error: &FormulaError) -> String {
        match error {
            FormulaError::Parse(detail) => tr!("error-formula-invalid", detail = detail.as_str()),
            FormulaError::UnknownColumn(letter) => {
                tr!("error-formula-unknown-column", letter = letter.as_str())
            }
            FormulaError::SelfReference(letter) => {
                tr!("error-formula-self-reference", letter = letter.as_str())
            }
            FormulaError::Cycle => tr!("error-formula-cycle"),
            FormulaError::TooDeep => tr!("error-formula-too-deep"),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn every_variant() -> Vec<FormulaError> {
            let samples = vec![
                FormulaError::Parse("incomplete expression".into()),
                FormulaError::UnknownColumn("C".into()),
                FormulaError::SelfReference("A".into()),
                FormulaError::Cycle,
                FormulaError::TooDeep,
            ];
            let mut seen = [false; 5];
            for sample in &samples {
                let index = match sample {
                    FormulaError::Parse(_) => 0,
                    FormulaError::UnknownColumn(_) => 1,
                    FormulaError::SelfReference(_) => 2,
                    FormulaError::Cycle => 3,
                    FormulaError::TooDeep => 4,
                };
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s), "une variante n'a pas d'exemple");
            samples
        }

        #[test]
        fn chaque_variante_a_sa_phrase_et_pas_de_jargon() {
            for error in every_variant() {
                crate::errors::tests_support::check(&error, &error_message(&error));
            }
        }
    }
}

/// Erreurs d'un calcul d'analyse (`stokesforge-analysis`, #181).
pub mod analysis {
    use stokesforge_analysis::expr::{ExprError, ExprErrorKind};
    use stokesforge_analysis::AnalysisError;

    use crate::tr;

    /// Un compte pour Fluent, qui ne convertit pas `usize`.
    fn number(count: usize) -> i64 {
        i64::try_from(count).unwrap_or(i64::MAX)
    }

    /// La phrase d'une [`AnalysisError`].
    pub fn error_message(error: &AnalysisError) -> String {
        match error {
            AnalysisError::NotEnoughPoints { found, needed } => tr!(
                "error-analysis-not-enough-points",
                found = number(*found),
                needed = number(*needed)
            ),
            AnalysisError::DuplicateX => tr!("error-analysis-duplicate-x"),
            AnalysisError::BadExpression(detail) => {
                tr!(
                    "error-analysis-bad-expression",
                    detail = expr_message(detail)
                )
            }
            AnalysisError::NoFreeParameter => tr!("error-analysis-no-free-parameter"),
            AnalysisError::EmptyBounds { name, min, max } => tr!(
                "error-analysis-empty-bounds",
                name = name.as_str(),
                min = min.to_string(),
                max = max.to_string()
            ),
            AnalysisError::NonFiniteModel => tr!("error-analysis-non-finite-model"),
            AnalysisError::SingularSystem => tr!("error-analysis-singular-system"),
            AnalysisError::NotConverged { iterations } => tr!(
                "error-analysis-not-converged",
                iterations = number(*iterations)
            ),
            AnalysisError::NotEnoughAnchors { found, needed } => tr!(
                "error-analysis-not-enough-anchors",
                found = number(*found),
                needed = number(*needed)
            ),
            AnalysisError::ZonesTooSmall {
                found,
                needed,
                degree,
            } => tr!(
                "error-analysis-zones-too-small",
                found = number(*found),
                needed = number(*needed),
                degree = i64::from(*degree)
            ),
            AnalysisError::DuplicateAnchor => tr!("error-analysis-duplicate-anchor"),
            AnalysisError::EmptyReference => tr!("error-analysis-empty-reference"),
            AnalysisError::EmptyResponse => tr!("error-analysis-empty-response"),
            AnalysisError::EmptyRange => tr!("error-analysis-empty-range"),
            // Le nom est une clé interne du crate de calcul : chacune de celles
            // que l'utilisateur peut provoquer a sa phrase.
            AnalysisError::InvalidParameter { name } => match name.as_str() {
                "peaks" => tr!("error-analysis-no-peak"),
                "sigma" => tr!("error-analysis-bad-sigma"),
                "signal" => tr!("error-analysis-bad-signal"),
                "components" => tr!("error-analysis-bad-components"),
                "degree" => tr!("error-analysis-bad-degree"),
                "iterations" => tr!("error-analysis-bad-iterations"),
                "noise" => tr!("error-analysis-bad-noise"),
                "max_peaks" => tr!("error-analysis-bad-max-peaks"),
                other => tr!("error-analysis-invalid-parameter", name = other),
            },
            AnalysisError::LengthMismatch => tr!("error-analysis-length-mismatch"),
            AnalysisError::OutputUnwritable => tr!("error-analysis-output-unwritable"),
        }
    }

    /// La phrase d'une erreur de lecture d'expression de modèle, avec sa position.
    pub fn expr_message(error: &ExprError) -> String {
        let position = number(error.position);
        match &error.kind {
            ExprErrorKind::Empty => tr!("error-analysis-expr-empty"),
            ExprErrorKind::UnexpectedChar(symbol) => tr!(
                "error-analysis-expr-unexpected-char",
                symbol = symbol.to_string(),
                position = position
            ),
            ExprErrorKind::UnexpectedEnd => {
                tr!("error-analysis-expr-unexpected-end", position = position)
            }
            ExprErrorKind::UnexpectedToken => {
                tr!("error-analysis-expr-unexpected-token", position = position)
            }
            ExprErrorKind::UnbalancedParenthesis => {
                tr!("error-analysis-expr-unbalanced", position = position)
            }
            ExprErrorKind::BadNumber => tr!("error-analysis-expr-bad-number", position = position),
            ExprErrorKind::TooDeep => tr!("error-analysis-expr-too-deep", position = position),
            ExprErrorKind::UnknownFunction(name) => tr!(
                "error-analysis-expr-unknown-function",
                name = name.as_str(),
                position = position
            ),
            ExprErrorKind::WrongArgumentCount {
                name,
                expected,
                found,
            } => tr!(
                "error-analysis-expr-wrong-arguments",
                name = name.as_str(),
                expected = number(*expected),
                found = number(*found),
                position = position
            ),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn chaque_variante_a_sa_phrase_et_pas_de_jargon() {
            let samples = [
                AnalysisError::NotEnoughPoints {
                    found: 2,
                    needed: 3,
                },
                AnalysisError::DuplicateX,
                AnalysisError::BadExpression(
                    stokesforge_analysis::expr::Expr::parse("a*").unwrap_err(),
                ),
                AnalysisError::NoFreeParameter,
                AnalysisError::EmptyBounds {
                    name: "tau".into(),
                    min: 2.0,
                    max: 1.0,
                },
                AnalysisError::NonFiniteModel,
                AnalysisError::SingularSystem,
                AnalysisError::NotConverged { iterations: 100 },
                AnalysisError::NotEnoughAnchors {
                    found: 1,
                    needed: 2,
                },
                AnalysisError::ZonesTooSmall {
                    found: 1,
                    needed: 4,
                    degree: 3,
                },
                AnalysisError::DuplicateAnchor,
                AnalysisError::EmptyReference,
                AnalysisError::EmptyResponse,
                AnalysisError::EmptyRange,
                AnalysisError::InvalidParameter {
                    name: "peaks".into(),
                },
                AnalysisError::LengthMismatch,
                AnalysisError::OutputUnwritable,
            ];
            // Un `match` sans `_` : une variante sans exemple ne compile pas.
            for sample in &samples {
                match sample {
                    AnalysisError::NotEnoughPoints { .. }
                    | AnalysisError::DuplicateX
                    | AnalysisError::BadExpression(_)
                    | AnalysisError::NoFreeParameter
                    | AnalysisError::EmptyBounds { .. }
                    | AnalysisError::NonFiniteModel
                    | AnalysisError::SingularSystem
                    | AnalysisError::NotConverged { .. }
                    | AnalysisError::NotEnoughAnchors { .. }
                    | AnalysisError::ZonesTooSmall { .. }
                    | AnalysisError::DuplicateAnchor
                    | AnalysisError::EmptyReference
                    | AnalysisError::EmptyResponse
                    | AnalysisError::EmptyRange
                    | AnalysisError::InvalidParameter { .. }
                    | AnalysisError::LengthMismatch
                    | AnalysisError::OutputUnwritable => {}
                }
                crate::errors::tests_support::check(sample, &error_message(sample));
            }
            assert_eq!(samples.len(), 17);
        }

        #[test]
        fn l_erreur_de_profondeur_d_expression_est_mise_en_phrase_avec_sa_position() {
            use stokesforge_analysis::expr::{Expr, ExprErrorKind};
            let formule = "(".repeat(101) + "1" + &")".repeat(101);
            let erreur = Expr::parse(&formule).unwrap_err();
            assert_eq!(erreur.kind, ExprErrorKind::TooDeep);
            assert_eq!(erreur.position, 101);
            // Comparée en anglais : la langue ne dépend pas de l'environnement.
            crate::i18n::set_language(crate::i18n::Language::English);
            assert_eq!(
                expr_message(&erreur),
                "Expression nested too deeply (position 101)"
            );
            // Enveloppée dans l'erreur d'analyse, la phrase garde le détail.
            assert_eq!(
                error_message(&AnalysisError::BadExpression(erreur)),
                "The model expression is invalid: Expression nested too deeply (position 101)"
            );
            // Le catalogue français porte la même clé et le même champ.
            let fr = include_str!("../i18n/fr/stokesforge_app.ftl");
            assert!(fr.contains(
                "error-analysis-expr-too-deep = Expression trop imbriquée (position { $position })"
            ));
        }
    }
}

/// Erreurs d'affichage d'un graphe (`stokesforge-render`, #255).
pub mod render {
    use stokesforge_render::RenderError;

    use crate::tr;

    /// La phrase d'une [`RenderError`].
    pub fn error_message(error: &RenderError) -> String {
        match error {
            RenderError::UnknownNode(_) => tr!("error-render-unknown-graph"),
            RenderError::NotAGraph(_) => tr!("error-render-not-a-graph"),
            RenderError::Drawing(detail) => tr!("error-export-drawing", detail = detail.as_str()),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use stokesforge_core::NodeId;

        fn every_variant() -> Vec<RenderError> {
            let samples = vec![
                RenderError::UnknownNode(NodeId::from_raw(3)),
                RenderError::NotAGraph(NodeId::from_raw(3)),
                RenderError::Drawing("backend refused".into()),
            ];
            let mut seen = [false; 3];
            for sample in &samples {
                let index = match sample {
                    RenderError::UnknownNode(_) => 0,
                    RenderError::NotAGraph(_) => 1,
                    RenderError::Drawing(_) => 2,
                };
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s), "une variante n'a pas d'exemple");
            samples
        }

        #[test]
        fn chaque_variante_a_sa_phrase_et_pas_de_jargon() {
            for error in every_variant() {
                crate::errors::tests_support::check(&error, &error_message(&error));
            }
        }
    }
}

/// Erreurs d'export d'un graphe en SVG, PDF ou PNG (`stokesforge-render-svg`).
pub mod export {
    use stokesforge_render_svg::export::ExportError;
    use stokesforge_render_svg::pdf::PdfError;
    use stokesforge_render_svg::png::PngError;
    use stokesforge_render_svg::SvgError;

    use crate::tr;

    fn write_failed(path: &str, source: &std::io::Error) -> String {
        tr!(
            "error-export-write",
            path = path,
            detail = source.to_string()
        )
    }

    /// La phrase d'une [`SvgError`].
    pub fn svg_error_message(error: &SvgError) -> String {
        match error {
            SvgError::Drawing(detail) => tr!("error-export-drawing", detail = detail.as_str()),
            SvgError::Write { path, source } => write_failed(path, source),
            SvgError::Typst(detail) => tr!("error-export-fonts", detail = detail.as_str()),
        }
    }

    /// La phrase d'une [`PdfError`].
    pub fn pdf_error_message(error: &PdfError) -> String {
        match error {
            PdfError::Svg(error) => svg_error_message(error),
            PdfError::Parse(detail) => tr!("error-export-reread", detail = detail.as_str()),
            PdfError::Conversion(detail) => tr!("error-export-pdf", detail = detail.as_str()),
            PdfError::Write { path, source } => write_failed(path, source),
        }
    }

    /// La phrase d'une [`PngError`].
    pub fn png_error_message(error: &PngError) -> String {
        match error {
            PngError::Svg(error) => svg_error_message(error),
            PngError::Parse(detail) => tr!("error-export-reread", detail = detail.as_str()),
            PngError::InvalidScale(scale) => {
                tr!("error-export-png-scale", scale = f64::from(*scale))
            }
            PngError::InvalidSize { width, height } => tr!(
                "error-export-png-size",
                width = i64::from(*width),
                height = i64::from(*height)
            ),
            PngError::Encode(detail) => tr!("error-export-png-encode", detail = detail.as_str()),
            PngError::Write { path, source } => write_failed(path, source),
        }
    }

    /// La phrase d'une [`ExportError`].
    pub fn error_message(error: &ExportError) -> String {
        match error {
            ExportError::Svg(error) => svg_error_message(error),
            ExportError::Pdf(error) => pdf_error_message(error),
            ExportError::Png(error) => png_error_message(error),
            ExportError::Write { path, source } => write_failed(path, source),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn io() -> std::io::Error {
            std::io::Error::other("disk full")
        }

        fn svg_variants() -> Vec<SvgError> {
            let samples = vec![
                SvgError::Drawing("backend refused".into()),
                SvgError::Write {
                    path: "/out/fig.svg".into(),
                    source: io(),
                },
                SvgError::Typst("font not found".into()),
            ];
            let mut seen = [false; 3];
            for sample in &samples {
                let index = match sample {
                    SvgError::Drawing(_) => 0,
                    SvgError::Write { .. } => 1,
                    SvgError::Typst(_) => 2,
                };
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s), "une variante n'a pas d'exemple");
            samples
        }

        fn pdf_variants() -> Vec<PdfError> {
            let samples = vec![
                PdfError::Svg(SvgError::Drawing("backend refused".into())),
                PdfError::Parse("bad svg".into()),
                PdfError::Conversion("bad document".into()),
                PdfError::Write {
                    path: "/out/fig.pdf".into(),
                    source: io(),
                },
            ];
            let mut seen = [false; 4];
            for sample in &samples {
                let index = match sample {
                    PdfError::Svg(_) => 0,
                    PdfError::Parse(_) => 1,
                    PdfError::Conversion(_) => 2,
                    PdfError::Write { .. } => 3,
                };
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s), "une variante n'a pas d'exemple");
            samples
        }

        fn png_variants() -> Vec<PngError> {
            let samples = vec![
                PngError::Svg(SvgError::Drawing("backend refused".into())),
                PngError::Parse("bad svg".into()),
                PngError::InvalidScale(0.0),
                PngError::InvalidSize {
                    width: 0,
                    height: 40,
                },
                PngError::Encode("bad raster".into()),
                PngError::Write {
                    path: "/out/fig.png".into(),
                    source: io(),
                },
            ];
            let mut seen = [false; 6];
            for sample in &samples {
                let index = match sample {
                    PngError::Svg(_) => 0,
                    PngError::Parse(_) => 1,
                    PngError::InvalidScale(_) => 2,
                    PngError::InvalidSize { .. } => 3,
                    PngError::Encode(_) => 4,
                    PngError::Write { .. } => 5,
                };
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s), "une variante n'a pas d'exemple");
            samples
        }

        fn export_variants() -> Vec<ExportError> {
            let samples = vec![
                ExportError::Svg(SvgError::Drawing("backend refused".into())),
                ExportError::Pdf(PdfError::Parse("bad svg".into())),
                ExportError::Png(PngError::Encode("bad raster".into())),
                ExportError::Write {
                    path: "/out/fig.svg".into(),
                    source: io(),
                },
            ];
            let mut seen = [false; 4];
            for sample in &samples {
                let index = match sample {
                    ExportError::Svg(_) => 0,
                    ExportError::Pdf(_) => 1,
                    ExportError::Png(_) => 2,
                    ExportError::Write { .. } => 3,
                };
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s), "une variante n'a pas d'exemple");
            samples
        }

        #[test]
        fn chaque_variante_a_sa_phrase_et_pas_de_jargon() {
            for error in svg_variants() {
                crate::errors::tests_support::check(&error, &svg_error_message(&error));
            }
            for error in pdf_variants() {
                crate::errors::tests_support::check(&error, &pdf_error_message(&error));
            }
            for error in png_variants() {
                crate::errors::tests_support::check(&error, &png_error_message(&error));
            }
            for error in export_variants() {
                crate::errors::tests_support::check(&error, &error_message(&error));
            }
        }

        #[test]
        fn les_champs_entrent_dans_la_phrase() {
            // Ces phrases sont comparées en anglais : la langue ne dépend pas
            // de l\'environnement (`STOKESFORGE_LANG`, préférences) de celui qui teste.
            crate::i18n::set_language(crate::i18n::Language::English);
            assert_eq!(
                error_message(&ExportError::Write {
                    path: "/out/fig.svg".into(),
                    source: io(),
                }),
                "Could not write /out/fig.svg: disk full"
            );
            assert_eq!(
                png_error_message(&PngError::InvalidSize {
                    width: 0,
                    height: 40
                }),
                "The PNG size is invalid (0×40 px). Increase the resolution"
            );
        }
    }
}

/// Erreurs du moteur de script (`stokesforge-script`, `stokesforge-python`), seulement
/// avec la feature `python` qui les tire dans l'application.
#[cfg(feature = "python")]
pub mod script {
    use stokesforge_python::interpreter::PythonError;
    use stokesforge_script::ScriptError;

    use crate::tr;

    /// La phrase d'une [`PythonError`].
    pub fn python_error_message(error: &PythonError) -> String {
        match error {
            PythonError::HomeNotFound(path) => {
                tr!("error-python-home", path = path.display().to_string())
            }
            PythonError::InitFailed(detail) => tr!("error-python-init", detail = detail.as_str()),
            PythonError::Python(trace) => tr!("error-python-trace", trace = trace.as_str()),
        }
    }

    /// La phrase d'une [`ScriptError`].
    pub fn error_message(error: &ScriptError) -> String {
        match error {
            ScriptError::Thread(source) => {
                tr!("error-script-thread", detail = source.to_string())
            }
            ScriptError::Interpreter(error) => python_error_message(error),
            ScriptError::Install(detail) => tr!("error-script-install", detail = detail.as_str()),
            ScriptError::Busy => tr!("error-script-busy"),
            ScriptError::Stopped => tr!("error-script-stopped"),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn python_variants() -> Vec<PythonError> {
            let samples = vec![
                PythonError::HomeNotFound("/opt/python".into()),
                PythonError::InitFailed("bad config".into()),
                PythonError::Python("Traceback (most recent call last)".into()),
            ];
            let mut seen = [false; 3];
            for sample in &samples {
                let index = match sample {
                    PythonError::HomeNotFound(_) => 0,
                    PythonError::InitFailed(_) => 1,
                    PythonError::Python(_) => 2,
                };
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s), "une variante n'a pas d'exemple");
            samples
        }

        fn script_variants() -> Vec<ScriptError> {
            let samples = vec![
                ScriptError::Thread(std::io::Error::other("out of threads")),
                ScriptError::Interpreter(PythonError::InitFailed("bad config".into())),
                ScriptError::Install("pyst missing".into()),
                ScriptError::Busy,
                ScriptError::Stopped,
            ];
            let mut seen = [false; 5];
            for sample in &samples {
                let index = match sample {
                    ScriptError::Thread(_) => 0,
                    ScriptError::Interpreter(_) => 1,
                    ScriptError::Install(_) => 2,
                    ScriptError::Busy => 3,
                    ScriptError::Stopped => 4,
                };
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s), "une variante n'a pas d'exemple");
            samples
        }

        #[test]
        fn chaque_variante_a_sa_phrase_et_pas_de_jargon() {
            for error in python_variants() {
                crate::errors::tests_support::check(&error, &python_error_message(&error));
            }
            for error in script_variants() {
                crate::errors::tests_support::check(&error, &error_message(&error));
            }
        }
    }
}

/// Erreurs du système de fichiers (`std::io::Error`).
///
/// Aucune famille du dessous ne les traduit : l'application les met en phrase
/// ici, par leur `ErrorKind`. Exception voulue à la règle « `match` sans `_` » :
/// `ErrorKind` est `#[non_exhaustive]`, le repli `error-io-other` couvre tout
/// genre non listé (et en reprend le texte brut du système).
pub mod io {
    use std::io::ErrorKind;

    use crate::tr;

    /// La cause d'une [`std::io::Error`], en groupe nominal ou verbal court,
    /// destinée à compléter une phrase de contexte (`{ $cause }`).
    pub fn error_message(error: &std::io::Error) -> String {
        match error.kind() {
            ErrorKind::NotFound => tr!("error-io-not-found"),
            ErrorKind::PermissionDenied => tr!("error-io-permission-denied"),
            ErrorKind::AlreadyExists => tr!("error-io-already-exists"),
            ErrorKind::StorageFull | ErrorKind::QuotaExceeded => tr!("error-io-storage-full"),
            ErrorKind::ReadOnlyFilesystem => tr!("error-io-read-only"),
            ErrorKind::InvalidData => tr!("error-io-invalid-data"),
            ErrorKind::InvalidInput => tr!("error-io-invalid-input"),
            ErrorKind::Interrupted => tr!("error-io-interrupted"),
            ErrorKind::UnexpectedEof => tr!("error-io-unexpected-eof"),
            ErrorKind::IsADirectory => tr!("error-io-is-directory"),
            ErrorKind::NotADirectory => tr!("error-io-not-directory"),
            ErrorKind::TimedOut => tr!("error-io-timed-out"),
            // `ErrorKind` est `#[non_exhaustive]` : un `match` exhaustif est
            // impossible, le repli reprend le texte brut du système.
            _ => tr!("error-io-other", detail = error.to_string()),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::errors::tests_support::check;

        #[test]
        fn chaque_genre_liste_a_sa_phrase_et_le_repli_reprend_le_texte_du_systeme() {
            let genres = [
                ErrorKind::NotFound,
                ErrorKind::PermissionDenied,
                ErrorKind::AlreadyExists,
                ErrorKind::StorageFull,
                ErrorKind::QuotaExceeded,
                ErrorKind::ReadOnlyFilesystem,
                ErrorKind::InvalidData,
                ErrorKind::InvalidInput,
                ErrorKind::Interrupted,
                ErrorKind::UnexpectedEof,
                ErrorKind::IsADirectory,
                ErrorKind::NotADirectory,
                ErrorKind::TimedOut,
            ];
            let mut phrases = std::collections::HashSet::new();
            for genre in genres {
                let erreur = std::io::Error::from(genre);
                let phrase = error_message(&erreur);
                check(&erreur, &phrase);
                assert!(!phrase.contains("error-io"), "clé non résolue : {phrase}");
                phrases.insert(phrase);
            }
            // Deux genres seulement partagent une phrase (disque plein / quota).
            assert_eq!(phrases.len(), genres.len() - 1);
            let autre = std::io::Error::other("Device on fire");
            let phrase = error_message(&autre);
            check(&autre, &phrase);
            assert!(phrase.contains("Device on fire"), "{phrase}");
        }
    }
}

/// Le contrôle commun des tests de chaque famille.
#[cfg(any(test, feature = "test-support"))]
pub mod tests_support {
    use std::fmt::Display;

    /// Mots du jargon interne qu'une phrase d'erreur n'emploie pas
    /// (`docs/textes-interface.md`, R9). Les `detail` des exemples n'en
    /// contiennent aucun : un mot interdit ne peut venir que du catalogue.
    const JARGON: &[&str] = &[
        "node", "session", "tree", "macro", "command", "handle", "snapshot", "please", "sorry",
    ];

    /// `message` est la phrase du catalogue de `error` : pas vide, sans jargon,
    /// sans point final ni point d'exclamation, en Sentence case. Elle peut
    /// coïncider avec le `Display` anglais (« X is not a StokesForge file ») : ce qui
    /// garantit que la variante est associée, c'est le `match` exhaustif.
    pub fn check(error: &dyn Display, message: &str) {
        let raw = error.to_string();
        assert!(!message.trim().is_empty(), "message vide pour {raw:?}");
        let lower = message.to_lowercase();
        for word in JARGON {
            assert!(
                !lower.contains(word),
                "jargon interne « {word} » dans {message:?}"
            );
        }
        assert!(
            !message.ends_with('.') && !message.contains('!'),
            "ponctuation finale dans {message:?}"
        );
        let first = message.chars().next();
        assert!(
            first.is_some_and(|c| !c.is_lowercase()),
            "pas de majuscule initiale dans {message:?}"
        );
    }
}
