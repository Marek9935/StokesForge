//! The single error type of the crate.
//!
//! Model errors come up unchanged (`Core`, transparent): a command that fails
//! fails for a reason of the core, not for a reason invented here. The three
//! other variants describe the only refusals specific to the history stack.

use stokesforge_core::CoreError;

/// Error of a command bus operation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    /// Failure returned by the model (`stokesforge-core`), passed on unchanged.
    #[error(transparent)]
    Core(#[from] CoreError),
    /// `end_macro`/`abort_macro` without `begin_macro`.
    #[error("no macro in progress")]
    NotInMacro,
    /// `undo`/`redo`/`set_limit`/`clear` while a macro is open.
    /// The field names the refused operation.
    #[error("operation refused while a macro is open: {0}")]
    MacroOpen(&'static str),
    /// Limit change refused: the stack is not empty.
    #[error("the history limit can only be changed when the stack is empty")]
    StackNotEmpty,
    /// `begin_macro` refused: nesting has already reached
    /// [`crate::history::MAX_MACRO_DEPTH`] levels (B7,
    /// `guide-de-correction-robustesse.md` §F4 — option 1: a named constant
    /// and an error, rather than making `revert`/`Drop` iterative on
    /// `Entry::Macro`). Nothing capped this nesting before this fix: beyond
    /// ~10,891 levels, undoing such a deep nesting overflowed the actor
    /// thread's stack and `abort()`ed the process — see
    /// `stokesforge-robustesse/src/bin/sonde_pile.rs`. The field names the
    /// refused limit.
    #[error("macro nesting too deep (limit: {0})")]
    MacroTooDeep(usize),
    /// A4 (`guide-de-correction-robustesse.md` §F6) — refusal of an
    /// `Execute`/`BeginMacro`/`EndMacro`/`AbortMacro` whose sender is not the
    /// one that opened the current macro. Distinct from `MacroOpen`:
    /// `MacroOpen` refuses an operation for **anyone** while a macro — any
    /// macro — is open (`undo`, `save`...); here, a macro **is** open, the
    /// operation would normally be allowed to it, but its sender is not the
    /// owner of that macro. The field names the refused operation.
    #[error("operation refused: a macro is open by another client ({0})")]
    ForeignMacro(&'static str),
}

/// Result alias of the crate.
pub type CommandResult<T> = Result<T, CommandError>;
