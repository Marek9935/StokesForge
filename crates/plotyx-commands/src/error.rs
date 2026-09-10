//! Type d'erreur unique du crate.
//!
//! Les erreurs du modèle remontent telles quelles (`Core`, transparente) :
//! une commande qui échoue échoue pour une raison du cœur, pas pour une
//! raison inventée ici. Les trois autres variantes décrivent les seuls
//! refus propres à la pile d'historique.

use plotyx_core::CoreError;

/// Erreur d'une opération du bus de commandes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    /// Échec rendu par le modèle (`plotyx-core`), transmis sans réécriture.
    #[error(transparent)]
    Core(#[from] CoreError),
    /// `end_macro`/`abort_macro` sans `begin_macro`.
    #[error("aucune macro en cours")]
    NotInMacro,
    /// `undo`/`redo`/`set_limit`/`clear` pendant qu'une macro est ouverte.
    /// Le champ nomme l'opération refusée.
    #[error("opération refusée pendant une macro : {0}")]
    MacroOpen(&'static str),
    /// Changement de limite refusé : la pile n'est pas vide.
    #[error("la limite d'historique ne se change que sur une pile vide")]
    StackNotEmpty,
}

/// Alias de résultat du crate.
pub type CommandResult<T> = Result<T, CommandError>;
