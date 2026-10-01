//! Type d'erreur unique du crate.

use plotyx_commands::CommandError;
use plotyx_persistence::PersistenceError;

/// Erreur d'une requête envoyée à la session.
///
/// Ne dérive plus `Clone`/`PartialEq`/`Eq` depuis l'ajout de la variante
/// `Persistence` : `PersistenceError` embarque un `std::io::Error`, qui
/// n'implémente ni l'un ni l'autre. Les tests comparent avec `matches!`.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// Échec rendu par le bus de commandes, transmis sans réécriture.
    #[error(transparent)]
    Command(#[from] CommandError),
    /// Échec de sauvegarde ou de chargement, transmis sans réécriture.
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
    /// L'acteur n'est plus là (thread terminé ou en panique) : aucune
    /// requête ne peut plus être servie.
    #[error("the session is closed")]
    Disconnected,
}

/// Alias de résultat du crate.
pub type SessionResult<T> = Result<T, SessionError>;
