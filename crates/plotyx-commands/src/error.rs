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
    #[error("no macro in progress")]
    NotInMacro,
    /// `undo`/`redo`/`set_limit`/`clear` pendant qu'une macro est ouverte.
    /// Le champ nomme l'opération refusée.
    #[error("operation refused while a macro is open: {0}")]
    MacroOpen(&'static str),
    /// Changement de limite refusé : la pile n'est pas vide.
    #[error("the history limit can only be changed when the stack is empty")]
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
    #[error("macro nesting too deep (limit: {0})")]
    MacroTooDeep(usize),
    /// A4 (`guide-de-correction-robustesse.md` §F6) — refus d'un
    /// `Execute`/`BeginMacro`/`EndMacro`/`AbortMacro` dont l'émetteur n'est
    /// pas celui qui a ouvert la macro courante. Distinct de `MacroOpen` :
    /// `MacroOpen` refuse une opération pour **quiconque** pendant qu'une
    /// macro — n'importe laquelle — est ouverte (`undo`, `save`...) ; ici,
    /// une macro **est** ouverte, l'opération lui serait normalement
    /// permise, mais son émetteur n'est pas le propriétaire de celle-ci. Le
    /// champ nomme l'opération refusée.
    #[error("operation refused: a macro is open by another client ({0})")]
    ForeignMacro(&'static str),
}

/// Alias de résultat du crate.
pub type CommandResult<T> = Result<T, CommandError>;
