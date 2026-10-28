//! Single error type of the crate.

use std::path::PathBuf;

use stokesforge_core::CoreError;

/// Error while saving or loading a [`stokesforge_core::Project`].
///
/// Does not derive `PartialEq`: `std::io::Error` does not implement it. The
/// tests compare the variants with `matches!`.
#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    /// I/O failure on the file.
    #[error("I/O error on {path}: {source}")]
    Io {
        /// Path targeted by the operation.
        path: PathBuf,
        /// Original system error.
        #[source]
        source: std::io::Error,
    },
    /// The file exceeds the maximum size accepted at open (A2,
    /// #316). The `.stf` is not compressed: its size on disk is its
    /// decompressed size, so this cap also bounds the memory of the
    /// decoding. Nothing was read beyond `limit` bytes.
    #[error("{path} is too large to open ({size} bytes, limit {limit} bytes)")]
    TooLarge {
        /// Path of the file read.
        path: PathBuf,
        /// Size of the file, in bytes.
        size: u64,
        /// Cap in force, in bytes.
        limit: u64,
    },
    /// The columns of the file declare more cells than the global budget
    /// accepted at open (B1, #316). Refused before any column
    /// allocation; `cells` is the running total at the moment of the overrun.
    #[error("{path} declares too many cells ({cells} so far, limit {limit})")]
    TooManyCells {
        /// Path of the file read.
        path: PathBuf,
        /// Cells accumulated when the cap was crossed.
        cells: u64,
        /// Budget in force.
        limit: u64,
    },
    /// The file does not start with the `STKF` signature: it is not a
    /// StokesForge file (or it is empty / truncated before the signature).
    #[error("{path} is not a StokesForge file")]
    NotAStokesForgeFile {
        /// Path of the file read.
        path: PathBuf,
    },
    /// The file carries a version number higher than the one this
    /// reader can understand. The body was **not** read.
    #[error("{path} was written by a newer version (format {found}, reader supports {supported})")]
    UnsupportedVersion {
        /// Path of the file read.
        path: PathBuf,
        /// Version number found in the file.
        found: u32,
        /// Highest version number that this reader can decode.
        supported: u32,
    },
    /// The body of the file could not be decoded (invalid MessagePack,
    /// truncated, or `Float` column buffer of a length not a multiple of 8).
    #[error("corrupt file {path}: {detail}")]
    Corrupt {
        /// Path of the file read.
        path: PathBuf,
        /// Description of the anomaly.
        detail: String,
    },
    /// The content read does not form a valid model (`Tree::from_nodes` or
    /// `Node::with_parent` refused it).
    #[error("invalid model in {path}: {source}")]
    InvalidModel {
        /// Path of the file read.
        path: PathBuf,
        /// Error of the core that refused the reconstruction.
        #[source]
        source: CoreError,
    },
    /// The body could not be decoded because of an **enum variant**
    /// that this reader does not know (a trace type, a column type, …
    /// added by a newer version of StokesForge) — distinct from
    /// [`Corrupt`](Self::Corrupt): the file is not damaged, it is
    /// simply newer than this reader. C5 (level 1,
    /// `guide-de-correction-robustesse.md` §F3): recognize this case rather
    /// than confuse it with real corruption; graceful degradation
    /// (keeping what is understood, level 2) remains a separate work
    /// package.
    #[error("{path} was written by a newer version of StokesForge (unknown variant): {detail}")]
    UnknownVariant {
        /// Path of the file read.
        path: PathBuf,
        /// Original message of `rmp_serde`/`serde`, which names the variant.
        detail: String,
    },
    /// `save` detected that the targeted file was modified — by another
    /// window, a script, or any other writer — since the last time
    /// THIS process loaded or saved it. B8
    /// (`guide-de-correction-robustesse.md` §F2): without this check,
    /// the write would silently overwrite the work of the other writer.
    #[error(
        "{path} changed since this process last loaded or saved it \
         (expected save counter {expected}, found {found}): reload before overwriting"
    )]
    ConcurrentModification {
        /// Path of the file targeted by `save`.
        path: PathBuf,
        /// Save counter that this process expected.
        expected: u64,
        /// Save counter actually present on disk.
        found: u64,
    },
    /// `save` found at the targeted location a file that carries the StokesForge
    /// signature but cannot be read (truncated header, version 0,
    /// undecodable body — interrupted copy, file of a nearby version).
    /// Writing over it would destroy it with no way back: we refuse, and it is
    /// up to the user to move it or choose another name.
    #[error(
        "{path} exists but cannot be read as a StokesForge file ({detail}): \
         refusing to overwrite it"
    )]
    UnreadableTarget {
        /// Path of the file targeted by `save`.
        path: PathBuf,
        /// Why it cannot be read (same text as the refusal at load).
        detail: String,
    },
}

/// Result alias of the crate.
pub type PersistenceResult<T> = Result<T, PersistenceError>;
