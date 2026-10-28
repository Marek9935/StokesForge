//! The crate's single error type.

use crate::id::NodeId;

/// Data model error. One variant per cause, never a catch-all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoreError {
    /// Object name syntactically invalid (empty, badly trimmed, reserved character).
    #[error("invalid name: {0}")]
    InvalidName(String),
    /// Name already taken in the target container.
    #[error("name already taken: {0}")]
    DuplicateName(String),
    /// Malformed or unresolved `[Book]Sheet!Col` path.
    #[error("invalid path: {0}")]
    Path(String),
    /// Structure operation impossible (wrong parent type, cycle, ...).
    #[error("invalid structure: {0}")]
    Structure(String),
    /// Identifier absent from the current tree.
    #[error("unknown node: {0}")]
    UnknownNode(NodeId),
    /// Row or column index out of bounds.
    #[error("index out of bounds: {0}")]
    IndexOutOfBounds(String),
    /// Values of a type incompatible with the column.
    #[error("incompatible type: {0}")]
    TypeMismatch(String),
    /// `IdAllocator` has exhausted `u64`: no identifier left to assign.
    /// B1 (`guide-de-correction-robustesse.md` §F2/F3/F4) — replaces the
    /// silent overflow (`self.next += 1` on `u64::MAX`, which made the
    /// counter go backwards and reassign an identifier already alive).
    #[error("identifiers exhausted: no more nodes can be created")]
    IdentifiersExhausted,
    /// A2 (`guide-de-correction-robustesse.md` §F6) — `Command::SetCells`
    /// carried an `expected_version` that no longer matches
    /// `Column::version` when the command is applied: another
    /// client changed the column between the snapshot the caller read and
    /// the application of its command. Neither the model nor the history was
    /// touched — a dedicated variant so the caller can tell this
    /// precise refusal (to retry after a re-read) from any other error (wrong
    /// type, missing column...).
    #[error(
        "concurrent modification: column {column} expected version {expected}, found version \
         {found}"
    )]
    ConcurrentModification {
        /// Column targeted by the refused write.
        column: NodeId,
        /// Version the caller still believed valid.
        expected: u64,
        /// Actual version of the column at the time of the refusal.
        found: u64,
    },
    /// Computed columns (v1) — `Command::SetCells` refused because
    /// `column` carries a formula (`ColumnMeta::formula.is_some()`): its
    /// values are recalculated automatically by `stokesforge-session::actor`,
    /// a direct cell write makes no sense there. The refusal comes
    /// **before** any mutation (checked on the current tree, like A2 for
    /// `ConcurrentModification`): neither the model nor the history is
    /// touched. Editing the formula text stays possible
    /// (`Command::SetColumnMeta`), also undoable.
    #[error("read-only column: {column} is computed by a formula")]
    FormulaColumnReadOnly {
        /// Formula column targeted by the refused write.
        column: NodeId,
    },
    /// Column produced by an analysis calculation (#181): `Command::SetCells`
    /// refused, as for a formula column — its values are rewritten by the
    /// recalculation, a hand entry would be overwritten (M9 d A). Detach it
    /// from the calculation with `Command::RemoveCalculation { with_columns: false }`
    /// (« Detach from Calculation »).
    #[error("read-only column: {column} is produced by calculation {calculation}")]
    CalculatedColumnReadOnly {
        /// Output column targeted by the refused write.
        column: NodeId,
        /// Calculation that produces it.
        calculation: NodeId,
    },
    /// Transpose refused (#253): the column is not numeric (it
    /// contains text). `letter` is its position letter, what the grid
    /// shows; `name` is its node name.
    #[error("cannot transpose: column {letter} ({name}) is not numeric")]
    TransposeNotNumeric {
        /// Position letter of the column (A, B, …).
        letter: String,
        /// Name of the column.
        name: String,
    },
    /// Transpose refused (#253): the column has no x'.
    #[error("cannot transpose: column {letter} ({name}) has no x' value")]
    TransposeNoXPrime {
        /// Position letter of the column (A, B, …).
        letter: String,
        /// Name of the column.
        name: String,
    },
}

/// Result alias of the crate.
pub type CoreResult<T> = Result<T, CoreError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_messages_are_in_english() {
        assert_eq!(
            CoreError::InvalidName("A!B".to_string()).to_string(),
            "invalid name: A!B"
        );
        assert_eq!(
            CoreError::DuplicateName("Sheet1".to_string()).to_string(),
            "name already taken: Sheet1"
        );
        assert_eq!(
            CoreError::Path("empty path".to_string()).to_string(),
            "invalid path: empty path"
        );
        assert_eq!(
            CoreError::Structure("cycle detected".to_string()).to_string(),
            "invalid structure: cycle detected"
        );
        assert_eq!(
            CoreError::UnknownNode(NodeId::from_raw(412)).to_string(),
            "unknown node: node 412"
        );
        assert_eq!(
            CoreError::IndexOutOfBounds("row 5".to_string()).to_string(),
            "index out of bounds: row 5"
        );
        assert_eq!(
            CoreError::TypeMismatch("text expected".to_string()).to_string(),
            "incompatible type: text expected"
        );
        assert_eq!(
            CoreError::IdentifiersExhausted.to_string(),
            "identifiers exhausted: no more nodes can be created"
        );
        assert_eq!(
            CoreError::ConcurrentModification {
                column: NodeId::from_raw(7),
                expected: 3,
                found: 4,
            }
            .to_string(),
            "concurrent modification: column node 7 expected version 3, found version 4"
        );
        assert_eq!(
            CoreError::FormulaColumnReadOnly {
                column: NodeId::from_raw(9),
            }
            .to_string(),
            "read-only column: node 9 is computed by a formula"
        );
        assert_eq!(
            CoreError::CalculatedColumnReadOnly {
                column: NodeId::from_raw(9),
                calculation: NodeId::from_raw(12),
            }
            .to_string(),
            "read-only column: node 9 is produced by calculation node 12"
        );
    }
}
