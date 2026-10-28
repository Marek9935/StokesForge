//! The crate's single error type.

use stokesforge_commands::CommandError;
use stokesforge_persistence::PersistenceError;

/// Error of a request sent to the session.
///
/// No longer derives `Clone`/`PartialEq`/`Eq` since the `Persistence` variant
/// was added: `PersistenceError` embeds a `std::io::Error`, which implements
/// neither. Tests compare with `matches!`.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// Failure returned by the command bus, passed on unchanged.
    #[error(transparent)]
    Command(#[from] CommandError),
    /// Save or load failure, passed on unchanged.
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
    /// The actor is gone (thread ended or panicked): no request can be
    /// served any more.
    #[error("the session is closed")]
    Disconnected,
}

/// Result alias of the crate.
pub type SessionResult<T> = Result<T, SessionError>;
