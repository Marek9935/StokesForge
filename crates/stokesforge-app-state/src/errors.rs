//! The errors of the crates below, put into sentences in the interface language
//! (issue #47, work package #39).
//!
//! # How the roles are split
//!
//! The crates below (`core`, `commands`, `persistence`, `import`, `script`...)
//! return **structured** errors: one variant per cause, its fields as data.
//! Their `Display` (`#[error(...)]`) is in raw **English**: it is what the CLI,
//! the `pyst` scripts and the logs read, and it is never translated — none of
//! these crates depends on Fluent. The application, for its part, maps each
//! variant to an `error-*` key of the catalog (`i18n/en/stokesforge_app.ftl`)
//! and displays the catalog's sentence, in the chosen language.
//!
//! One `error_message` function per family of errors, each in its own submodule:
//! [`model`], [`commands`], [`session`], [`file`], [`import`], [`formula`],
//! [`render`], [`export`], [`io`] and, with the `python` feature, `script`. Every
//! error **shown** to the user goes through one of them, never through
//! `to_string()`.
//!
//! # Adding an error variant
//!
//! The `match` of each function is **exhaustive, without `_`**: a new variant
//! does not compile until it is handled here. Two treatments:
//!
//! - give it an `error-<family>-<cause>` key (text following the glossary and
//!   `docs/ui-texts.md`: what happened, then what to do, without
//!   internal jargon — no *node*, *session*, *macro*, *command*) and use it
//!   through [`crate::tr!`] with its fields as named arguments;
//! - or, failing that, let it fall back to its English `Display` through
//!   [`fallback`]: nothing disappears, but the text is not translated.
//!
//! The test of each family builds **one value of each variant** and fails if one
//! of them is not listed (a `match` without `_` in the test) or if its message
//! is internal jargon.
//!
//! # What stays raw
//!
//! The `detail` fields (a cause described in English by the originating crate:
//! the reason a name was refused, the line of a corrupted file) and the system
//! messages (`std::io::Error`) are taken as is into the sentence: they are in
//! English whatever the interface language. Structuring them further would
//! amount to duplicating the logic of the crates below.

use std::fmt::Display;

/// The raw English text of an error, for a variant with no catalog key.
///
/// Nothing disappears: a cause nobody has put into a sentence yet is displayed
/// through its `Display`, which is readable.
pub fn fallback(error: &dyn Display) -> String {
    error.to_string()
}

/// Errors of the data model (`stokesforge-core`).
pub mod model {
    use stokesforge_core::CoreError;

    use crate::tr;

    /// The sentence of a [`CoreError`].
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
            CoreError::TransposeNoXPrime { letter, name } => tr!(
                "error-model-transpose-no-x-prime",
                letter = letter.as_str(),
                name = name.as_str()
            ),
            CoreError::TransposeNotNumeric { letter, name } => tr!(
                "error-model-transpose-not-numeric",
                letter = letter.as_str(),
                name = name.as_str()
            ),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use stokesforge_core::NodeId;

        /// One value of each variant; the `match` without `_` forces adding one for
        /// each new variant.
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
                CoreError::TransposeNoXPrime {
                    letter: "D".into(),
                    name: "Temps".into(),
                },
                CoreError::TransposeNotNumeric {
                    letter: "B".into(),
                    name: "Notes".into(),
                },
            ];
            let mut seen = [false; 13];
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
                    CoreError::TransposeNoXPrime { .. } => 11,
                    CoreError::TransposeNotNumeric { .. } => 12,
                };
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s), "a variant has no example");
            samples
        }

        #[test]
        fn every_variant_has_its_sentence_and_no_jargon() {
            for error in every_variant() {
                crate::errors::tests_support::check(&error, &error_message(&error));
            }
        }

        #[test]
        fn the_fields_enter_the_sentence() {
            // These sentences are compared in English: the language does not depend on the
            // environment (`STOKESFORGE_LANG`, preferences) of whoever runs the test.
            crate::i18n::set_language(crate::i18n::Language::English);
            assert_eq!(
                error_message(&CoreError::DuplicateName("Sheet1 in Book1".into())),
                "Name already used: Sheet1 in Book1"
            );
            assert_eq!(
                error_message(&CoreError::UnknownNode(NodeId::from_raw(7))),
                "This object no longer exists in the project"
            );
            assert_eq!(
                error_message(&CoreError::TransposeNoXPrime {
                    letter: "D".into(),
                    name: "Temps".into()
                }),
                "Column D (Temps) has no x' value. Set one to analyze a row"
            );
        }
    }
}

/// Errors of the command bus (`stokesforge-commands`).
pub mod commands {
    use stokesforge_commands::CommandError;

    use crate::tr;

    /// The sentence of a [`CommandError`].
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
            assert!(seen.iter().all(|&s| s), "a variant has no example");
            samples
        }

        #[test]
        fn every_variant_has_its_sentence_and_no_jargon() {
            for error in every_variant() {
                crate::errors::tests_support::check(&error, &error_message(&error));
            }
        }

        #[test]
        fn a_model_error_keeps_its_sentence() {
            // These sentences are compared in English: the language does not depend on the
            // environment (`STOKESFORGE_LANG`, preferences) of whoever runs the test.
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

/// Errors of the session (`stokesforge-session`): those of the bus and of the
/// file, passed on without rewriting.
pub mod session {
    use stokesforge_session::SessionError;

    use crate::tr;

    /// The sentence of a [`SessionError`].
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
            assert!(seen.iter().all(|&s| s), "a variant has no example");
            samples
        }

        #[test]
        fn every_variant_has_its_sentence_and_no_jargon() {
            for error in every_variant() {
                crate::errors::tests_support::check(&error, &error_message(&error));
            }
        }
    }
}

/// Read and write errors of a `.stf` file (`stokesforge-persistence`).
pub mod file {
    use stokesforge_persistence::PersistenceError;

    use crate::tr;

    /// The sentence of a [`PersistenceError`].
    pub fn error_message(error: &PersistenceError) -> String {
        match error {
            PersistenceError::Io { path, source } => tr!(
                "error-file-io",
                path = path.display().to_string(),
                detail = source.to_string()
            ),
            PersistenceError::TooLarge { path, size, limit } => {
                // In MB, rounded up for the size: "513 MB" for a limit of 512, never "512 MB,
                // limit 512 MB".
                let size_mb = i64::try_from(size.div_ceil(1 << 20)).unwrap_or(i64::MAX);
                let limit_mb = i64::try_from(*limit >> 20).unwrap_or(i64::MAX);
                tr!(
                    "error-file-too-large",
                    path = path.display().to_string(),
                    size = size_mb,
                    limit = limit_mb
                )
            }
            PersistenceError::TooManyCells { path, cells, limit } => tr!(
                "error-file-too-many-cells",
                path = path.display().to_string(),
                cells = i64::try_from(cells.div_ceil(1_000_000)).unwrap_or(i64::MAX),
                limit = i64::try_from(limit / 1_000_000).unwrap_or(i64::MAX)
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
            PersistenceError::UnreadableTarget { path, detail } => tr!(
                "error-file-unreadable-target",
                path = path.display().to_string(),
                detail = detail.as_str()
            ),
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
                PersistenceError::UnreadableTarget {
                    path: path(),
                    detail: "header truncated".into(),
                },
                PersistenceError::TooLarge {
                    path: path(),
                    size: 600 << 20,
                    limit: 512 << 20,
                },
                PersistenceError::TooManyCells {
                    path: path(),
                    cells: 2_000_000_000,
                    limit: 1_000_000_000,
                },
            ];
            let mut seen = [false; 10];
            for sample in &samples {
                let index = match sample {
                    PersistenceError::Io { .. } => 0,
                    PersistenceError::NotAStokesForgeFile { .. } => 1,
                    PersistenceError::UnsupportedVersion { .. } => 2,
                    PersistenceError::Corrupt { .. } => 3,
                    PersistenceError::InvalidModel { .. } => 4,
                    PersistenceError::UnknownVariant { .. } => 5,
                    PersistenceError::ConcurrentModification { .. } => 6,
                    PersistenceError::UnreadableTarget { .. } => 7,
                    PersistenceError::TooLarge { .. } => 8,
                    PersistenceError::TooManyCells { .. } => 9,
                };
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s), "a variant has no example");
            samples
        }

        #[test]
        fn every_variant_has_its_sentence_and_no_jargon() {
            for error in every_variant() {
                crate::errors::tests_support::check(&error, &error_message(&error));
            }
        }

        #[test]
        fn the_fields_enter_the_sentence() {
            // These sentences are compared in English: the language does not depend on the
            // environment (`STOKESFORGE_LANG`, preferences) of whoever runs the test.
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
                "/data/run.stf was saved by a newer version of StokesForge (file format 9, this version reads up to 2): update StokesForge to open it"
            );
        }
    }
}

/// Parse errors of an instrument file (`stokesforge-import`,
/// `stokesforge-importer`).
pub mod import {
    use stokesforge_import::ImportError;
    use stokesforge_importer::{ImportFailure, ImportFailureCause};

    use crate::state::ImportReadError;

    use crate::tr;

    /// The sentence of an [`ImportError`].
    pub fn error_message(error: &ImportError) -> String {
        match error {
            ImportError::NotText => tr!("error-import-binary"),
            ImportError::Empty => tr!("error-import-empty"),
            ImportError::NoDataRows => tr!("error-import-no-data"),
            ImportError::WhitespaceWithSpaceThousands => tr!("error-import-whitespace-thousands"),
        }
    }

    /// The sentence of an [`ImportReadError`] of the import wizard.
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

    /// The sentence of an [`ImportFailure`]: the file, then what happened.
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
                ImportError::WhitespaceWithSpaceThousands,
            ];
            let mut seen = [false; 4];
            for sample in &samples {
                let index = match sample {
                    ImportError::NotText => 0,
                    ImportError::Empty => 1,
                    ImportError::NoDataRows => 2,
                    ImportError::WhitespaceWithSpaceThousands => 3,
                };
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s), "a variant has no example");
            samples
        }

        #[test]
        fn every_variant_has_its_sentence_and_no_jargon() {
            for error in every_variant() {
                crate::errors::tests_support::check(&error, &error_message(&error));
            }
        }

        #[test]
        fn a_failed_file_names_the_file_and_the_cause() {
            // These sentences are compared in English: the language does not depend on the
            // environment (`STOKESFORGE_LANG`, preferences) of whoever runs the test.
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

/// Errors of a calculated-column formula (`stokesforge-core::formula`).
pub mod formula {
    use stokesforge_core::FormulaError;

    use crate::tr;

    /// The sentence of a [`FormulaError`].
    pub fn error_message(error: &FormulaError) -> String {
        match error {
            FormulaError::Parse(detail) => tr!("error-formula-invalid", detail = detail.as_str()),
            FormulaError::UnknownColumn(letter) => {
                tr!("error-formula-unknown-column", letter = letter.as_str())
            }
            FormulaError::SelfReference(letter) => {
                tr!("error-formula-self-reference", letter = letter.as_str())
            }
            FormulaError::DeletedColumn => tr!("error-formula-deleted-column"),
            FormulaError::Cycle => tr!("error-formula-cycle"),
            FormulaError::TooDeep => tr!("error-formula-too-deep"),
            FormulaError::UnknownFunction(name) => {
                tr!("error-formula-unknown-function", name = name.as_str())
            }
            FormulaError::WrongArgumentCount {
                name,
                expected,
                found,
            } => {
                // Digits and slashes, so that the sentence needs no "or" of
                // its own language: `1/2`.
                let expected = expected
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join("/");
                let found = *found as i64;
                tr!(
                    "error-formula-wrong-argument-count",
                    name = name.as_str(),
                    expected = expected.as_str(),
                    found = found
                )
            }
            FormulaError::NotAFunction(letter) => {
                tr!("error-formula-not-a-function", letter = letter.as_str())
            }
            FormulaError::PiNeedsParentheses => tr!("error-formula-pi-needs-parentheses"),
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
                FormulaError::DeletedColumn,
                FormulaError::Cycle,
                FormulaError::TooDeep,
                FormulaError::UnknownFunction("foo".into()),
                FormulaError::WrongArgumentCount {
                    name: "sqrt".into(),
                    expected: vec![1],
                    found: 2,
                },
                FormulaError::NotAFunction("A".into()),
                FormulaError::PiNeedsParentheses,
            ];
            let mut seen = [false; 10];
            for sample in &samples {
                let index = match sample {
                    FormulaError::Parse(_) => 0,
                    FormulaError::UnknownColumn(_) => 1,
                    FormulaError::SelfReference(_) => 2,
                    FormulaError::DeletedColumn => 3,
                    FormulaError::Cycle => 4,
                    FormulaError::TooDeep => 5,
                    FormulaError::UnknownFunction(_) => 6,
                    FormulaError::WrongArgumentCount { .. } => 7,
                    FormulaError::NotAFunction(_) => 8,
                    FormulaError::PiNeedsParentheses => 9,
                };
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s), "a variant has no example");
            samples
        }

        #[test]
        fn every_variant_has_its_sentence_and_no_jargon() {
            for error in every_variant() {
                crate::errors::tests_support::check(&error, &error_message(&error));
            }
        }

        #[test]
        fn the_deleted_column_says_what_to_replace() {
            // Compared in English: the language does not depend on the environment.
            crate::i18n::set_language(crate::i18n::Language::English);
            assert_eq!(
                error_message(&FormulaError::DeletedColumn),
                "Column deleted. Replace #REF with a column letter"
            );
            // The `Display` of the crate below stays raw English.
            assert_eq!(
                FormulaError::DeletedColumn.to_string(),
                "the formula reads a column that no longer exists in the worksheet"
            );
            // The French catalog carries the same key.
            let fr = include_str!("../i18n/fr/stokesforge_app.ftl");
            assert!(fr.contains(
                // lang: fr
                "error-formula-deleted-column = Colonne supprimée. Remplacer #REF par une lettre de colonne"
            ));
        }

        #[test]
        fn the_function_errors_say_what_to_write() {
            // Compared in English: the language does not depend on the environment.
            crate::i18n::set_language(crate::i18n::Language::English);
            assert_eq!(
                error_message(&FormulaError::UnknownFunction("foo".into())),
                "Unknown function foo. Check its spelling"
            );
            assert_eq!(
                error_message(&FormulaError::WrongArgumentCount {
                    name: "if".into(),
                    expected: vec![3],
                    found: 2
                }),
                "Wrong number of arguments for if: expected 3, found 2"
            );
            assert_eq!(
                error_message(&FormulaError::NotAFunction("A".into())),
                "A is a column, not a function. To multiply, write A*(...)"
            );
            assert_eq!(
                error_message(&FormulaError::PiNeedsParentheses),
                "For the number π, write pi() (a lone pi would be column PI)"
            );
            // The French catalog carries the same keys.
            let fr = include_str!("../i18n/fr/stokesforge_app.ftl");
            for key in [
                "error-formula-unknown-function",
                "error-formula-wrong-argument-count",
                "error-formula-not-a-function",
                "error-formula-pi-needs-parentheses",
            ] {
                assert!(fr.contains(&format!("{key} = ")), "{key}");
            }
        }

        #[test]
        fn the_too_deep_formula_error_is_put_into_a_sentence() {
            use stokesforge_core::{ColumnFormula, FormulaPart, NodeId};
            let formula = ColumnFormula::from_parts(vec![
                FormulaPart::Text("(".repeat(101)),
                FormulaPart::Column(NodeId::from_raw(1)),
                FormulaPart::Text(")".repeat(101)),
            ]);
            let error = formula.compile().unwrap_err();
            assert_eq!(error, FormulaError::TooDeep);
            // Compared in English: the language does not depend on the environment.
            crate::i18n::set_language(crate::i18n::Language::English);
            assert_eq!(
                error_message(&error),
                "The formula is nested too deeply. Simplify it (fewer nested parentheses or chained terms)"
            );
            // The `Display` of the crate below stays raw English.
            assert_eq!(error.to_string(), "formula nested too deeply");
            // The French catalog carries the same key.
            let fr = include_str!("../i18n/fr/stokesforge_app.ftl");
            assert!(fr.contains(
                // lang: fr
                "error-formula-too-deep = La formule est trop imbriquée. La simplifier (moins de parenthèses imbriquées ou de termes enchaînés)"
            ));
        }
    }
}

/// Errors of an analysis computation (`stokesforge-analysis`, #181).
pub mod analysis {
    use stokesforge_analysis::expr::{ExprError, ExprErrorKind};
    use stokesforge_analysis::AnalysisError;

    use crate::tr;

    /// A count for Fluent, which does not convert `usize`.
    fn number(count: usize) -> i64 {
        i64::try_from(count).unwrap_or(i64::MAX)
    }

    /// The sentence of an [`AnalysisError`].
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
            AnalysisError::ModelNotEvaluable => tr!("error-analysis-model-not-evaluable"),
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
            // The name is an internal key of the computation crate: each one the user can
            // trigger has its sentence.
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

    /// The sentence of a model-expression read error, with its position.
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
        fn every_variant_has_its_sentence_and_no_jargon() {
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
                AnalysisError::ModelNotEvaluable,
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
            // A `match` without `_`: a variant with no example does not compile.
            for sample in &samples {
                match sample {
                    AnalysisError::NotEnoughPoints { .. }
                    | AnalysisError::DuplicateX
                    | AnalysisError::BadExpression(_)
                    | AnalysisError::NoFreeParameter
                    | AnalysisError::EmptyBounds { .. }
                    | AnalysisError::NonFiniteModel
                    | AnalysisError::ModelNotEvaluable
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
            assert_eq!(samples.len(), 18);
        }

        #[test]
        fn the_expression_depth_error_is_put_into_a_sentence_with_its_position() {
            use stokesforge_analysis::expr::{Expr, ExprErrorKind};
            let formula = "(".repeat(101) + "1" + &")".repeat(101);
            let error = Expr::parse(&formula).unwrap_err();
            assert_eq!(error.kind, ExprErrorKind::TooDeep);
            assert_eq!(error.position, 101);
            // Compared in English: the language does not depend on the environment.
            crate::i18n::set_language(crate::i18n::Language::English);
            assert_eq!(
                expr_message(&error),
                "Expression nested too deeply (position 101)"
            );
            // Wrapped in the analysis error, the sentence keeps the detail.
            assert_eq!(
                error_message(&AnalysisError::BadExpression(error)),
                "The model expression is invalid: Expression nested too deeply (position 101)"
            );
            // The French catalog carries the same key and the same field.
            let fr = include_str!("../i18n/fr/stokesforge_app.ftl");
            assert!(fr.contains(
                "error-analysis-expr-too-deep = Expression trop imbriquée (position { $position })"
            ));
        }
    }
}

/// Display errors of a graph (`stokesforge-render`, #255).
pub mod render {
    use stokesforge_render::RenderError;

    use crate::tr;

    /// The sentence of a [`RenderError`].
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
            assert!(seen.iter().all(|&s| s), "a variant has no example");
            samples
        }

        #[test]
        fn every_variant_has_its_sentence_and_no_jargon() {
            for error in every_variant() {
                crate::errors::tests_support::check(&error, &error_message(&error));
            }
        }
    }
}

/// Errors exporting a graph to SVG, PDF or PNG (`stokesforge-render-svg`).
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

    /// The sentence of a [`SvgError`].
    pub fn svg_error_message(error: &SvgError) -> String {
        match error {
            SvgError::Drawing(detail) => tr!("error-export-drawing", detail = detail.as_str()),
            SvgError::Write { path, source } => write_failed(path, source),
            SvgError::Typst(detail) => tr!("error-export-fonts", detail = detail.as_str()),
        }
    }

    /// The sentence of a [`PdfError`].
    pub fn pdf_error_message(error: &PdfError) -> String {
        match error {
            PdfError::Svg(error) => svg_error_message(error),
            PdfError::Parse(detail) => tr!("error-export-reread", detail = detail.as_str()),
            PdfError::Conversion(detail) => tr!("error-export-pdf", detail = detail.as_str()),
            PdfError::Write { path, source } => write_failed(path, source),
        }
    }

    /// The sentence of a [`PngError`].
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

    /// The sentence of an [`ExportError`].
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
            assert!(seen.iter().all(|&s| s), "a variant has no example");
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
            assert!(seen.iter().all(|&s| s), "a variant has no example");
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
            assert!(seen.iter().all(|&s| s), "a variant has no example");
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
            assert!(seen.iter().all(|&s| s), "a variant has no example");
            samples
        }

        #[test]
        fn every_variant_has_its_sentence_and_no_jargon() {
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
        fn the_fields_enter_the_sentence() {
            // These sentences are compared in English: the language does not depend on the
            // environment (`STOKESFORGE_LANG`, preferences) of whoever runs the test.
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

/// Errors of the script engine (`stokesforge-script`, `stokesforge-python`),
/// only with the `python` feature that pulls them into the application.
#[cfg(feature = "python")]
pub mod script {
    use stokesforge_python::interpreter::PythonError;
    use stokesforge_script::ScriptError;

    use crate::tr;

    /// The sentence of a [`PythonError`].
    pub fn python_error_message(error: &PythonError) -> String {
        match error {
            PythonError::HomeNotFound(path) => {
                tr!("error-python-home", path = path.display().to_string())
            }
            PythonError::InitFailed(detail) => tr!("error-python-init", detail = detail.as_str()),
            PythonError::Python(trace) => tr!("error-python-trace", trace = trace.as_str()),
        }
    }

    /// The sentence of a [`ScriptError`].
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
            assert!(seen.iter().all(|&s| s), "a variant has no example");
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
            assert!(seen.iter().all(|&s| s), "a variant has no example");
            samples
        }

        #[test]
        fn every_variant_has_its_sentence_and_no_jargon() {
            for error in python_variants() {
                crate::errors::tests_support::check(&error, &python_error_message(&error));
            }
            for error in script_variants() {
                crate::errors::tests_support::check(&error, &error_message(&error));
            }
        }
    }
}

/// File-system errors (`std::io::Error`).
///
/// No family below translates them: the application puts them into sentences
/// here, by their `ErrorKind`. Intended exception to the rule "`match` without
/// `_`": `ErrorKind` is `#[non_exhaustive]`, the `error-io-other` fallback covers
/// any kind not listed (and takes the system's raw text from it).
pub mod io {
    use std::io::ErrorKind;

    use crate::tr;

    /// The cause of a [`std::io::Error`], as a short noun or verb phrase, meant to
    /// complete a context sentence (`{ $cause }`).
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
            // `ErrorKind` is `#[non_exhaustive]`: an exhaustive `match` is impossible, the
            // fallback takes the system's raw text.
            _ => tr!("error-io-other", detail = error.to_string()),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::errors::tests_support::check;

        #[test]
        fn every_listed_kind_has_its_sentence_and_the_fallback_takes_the_system_text() {
            let kinds = [
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
            let mut sentences = std::collections::HashSet::new();
            for kind in kinds {
                let io_error = std::io::Error::from(kind);
                let sentence = error_message(&io_error);
                check(&io_error, &sentence);
                assert!(!sentence.contains("error-io"), "unresolved key: {sentence}");
                sentences.insert(sentence);
            }
            // Only two kinds share a sentence (disk full / quota).
            assert_eq!(sentences.len(), kinds.len() - 1);
            let other_error = std::io::Error::other("Device on fire");
            let sentence = error_message(&other_error);
            check(&other_error, &sentence);
            assert!(sentence.contains("Device on fire"), "{sentence}");
        }
    }
}

/// The check common to the tests of each family.
#[cfg(any(test, feature = "test-support"))]
pub mod tests_support {
    use std::fmt::Display;

    /// Internal jargon words that an error sentence does not use
    /// (`docs/ui-texts.md`, R9). The `detail`s of the examples contain none:
    /// a forbidden word can only come from the catalog.
    const JARGON: &[&str] = &[
        "node", "session", "tree", "macro", "command", "handle", "snapshot", "please", "sorry",
    ];

    /// `message` is the catalog sentence of `error`: not empty, without jargon,
    /// without a final period or exclamation mark, in Sentence case. It may
    /// coincide with the English `Display` ("X is not a StokesForge file"): what
    /// guarantees that the variant is mapped is the exhaustive `match`.
    ///
    pub fn check(error: &dyn Display, message: &str) {
        let raw = error.to_string();
        assert!(!message.trim().is_empty(), "empty message for {raw:?}");
        let lower = message.to_lowercase();
        for word in JARGON {
            assert!(
                !lower.contains(word),
                "internal jargon \"{word}\" in {message:?}"
            );
        }
        assert!(
            !message.ends_with('.') && !message.contains('!'),
            "final punctuation in {message:?}"
        );
        let first = message.chars().next();
        assert!(
            first.is_some_and(|c| !c.is_lowercase()),
            "no initial capital in {message:?}"
        );
    }
}
