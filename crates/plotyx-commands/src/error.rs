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
    /// `begin_macro` refusé : l'imbrication a déjà atteint
    /// [`crate::history::MAX_MACRO_DEPTH`] niveaux (B7,
    /// `guide-de-correction-robustesse.md` §F4 — option 1 : une constante
    /// nommée et une erreur, plutôt que de rendre `revert`/`Drop` itératifs
    /// sur `Entry::Macro`). Rien n'empilait cette limite avant ce correctif
    /// : au-delà de ~10 891 niveaux, annuler une imbrication aussi profonde
    /// débordait la pile du fil de l'acteur et faisait `abort()` le
    /// processus — voir `plotyx-robustesse/src/bin/sonde_pile.rs`. Le champ
    /// nomme la limite refusée.
    #[error("imbrication de macros trop profonde (limite : {0})")]
    MacroTooDeep(usize),
}

/// Alias de résultat du crate.
pub type CommandResult<T> = Result<T, CommandError>;
