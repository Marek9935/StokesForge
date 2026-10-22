//! Type d'erreur unique du crate.

use crate::id::NodeId;

/// Erreur du modèle de données. Une variante par cause, jamais de fourre-tout.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoreError {
    /// Nom d'objet syntaxiquement invalide (vide, mal détouré, caractère réservé).
    #[error("invalid name: {0}")]
    InvalidName(String),
    /// Nom déjà pris dans le conteneur cible.
    #[error("name already taken: {0}")]
    DuplicateName(String),
    /// Chemin `[Book]Sheet!Col` mal formé ou non résolu.
    #[error("invalid path: {0}")]
    Path(String),
    /// Opération de structure impossible (mauvais type de parent, cycle, ...).
    #[error("invalid structure: {0}")]
    Structure(String),
    /// Identifiant absent de l'arbre courant.
    #[error("unknown node: {0}")]
    UnknownNode(NodeId),
    /// Index de ligne ou de colonne hors bornes.
    #[error("index out of bounds: {0}")]
    IndexOutOfBounds(String),
    /// Valeurs d'un type incompatible avec la colonne.
    #[error("incompatible type: {0}")]
    TypeMismatch(String),
    /// `IdAllocator` a épuisé `u64` : plus aucun identifiant à attribuer.
    /// B1 (`guide-de-correction-robustesse.md` §F2/F3/F4) — remplace le
    /// débordement silencieux (`self.next += 1` sur `u64::MAX`, qui faisait
    /// reculer le compteur et réattribuer un identifiant déjà vivant).
    #[error("identifiers exhausted: no more nodes can be created")]
    IdentifiersExhausted,
    /// A2 (`guide-de-correction-robustesse.md` §F6) — `Command::SetCells`
    /// portait un `expected_version` qui ne coïncide plus avec
    /// `Column::version` au moment d'appliquer la commande : un autre
    /// client a modifié la colonne entre l'instantané lu par l'appelant et
    /// l'application de sa commande. Ni le modèle ni l'historique n'ont été
    /// touchés — variante dédiée pour que l'appelant distingue ce refus
    /// précis (à retenter après relecture) de toute autre erreur (mauvais
    /// type, colonne absente...).
    #[error(
        "concurrent modification: column {column} expected version {expected}, found version \
         {found}"
    )]
    ConcurrentModification {
        /// Colonne visée par l'écriture refusée.
        column: NodeId,
        /// Version que l'appelant croyait encore valable.
        expected: u64,
        /// Version réelle de la colonne au moment du refus.
        found: u64,
    },
    /// Colonnes calculées (v1) — `Command::SetCells` refusé parce que
    /// `column` porte une formule (`ColumnMeta::formula.is_some()`) : ses
    /// valeurs sont recalculées automatiquement par `stokesforge-session::actor`,
    /// une écriture directe de cellule n'y a pas de sens. Le refus arrive
    /// **avant** toute mutation (vérifié sur l'arbre courant, comme A2 pour
    /// `ConcurrentModification`) : ni le modèle ni l'historique ne sont
    /// touchés. Éditer le texte de la formule reste possible
    /// (`Command::SetColumnMeta`), lui aussi annulable.
    #[error("read-only column: {column} is computed by a formula")]
    FormulaColumnReadOnly {
        /// Colonne-formule visée par l'écriture refusée.
        column: NodeId,
    },
    /// Colonne produite par un calcul d'analyse (#181) : `Command::SetCells`
    /// refusé, comme pour une colonne-formule — ses valeurs sont réécrites par
    /// le recalcul, une saisie à la main serait écrasée (M9 d A). Se détache
    /// du calcul par `Command::RemoveCalculation { with_columns: false }`
    /// (« Detach from Calculation »).
    #[error("read-only column: {column} is produced by calculation {calculation}")]
    CalculatedColumnReadOnly {
        /// Colonne de sortie visée par l'écriture refusée.
        column: NodeId,
        /// Calcul qui la produit.
        calculation: NodeId,
    },
    /// Transposition refusée (#253) : la colonne n'est pas numérique (elle
    /// contient du texte). `letter` est sa lettre de position, ce que la grille
    /// affiche ; `name` son nom de nœud.
    #[error("cannot transpose: column {letter} ({name}) is not numeric")]
    TransposeNotNumeric {
        /// Lettre de position de la colonne (A, B, …).
        letter: String,
        /// Nom de la colonne.
        name: String,
    },
    /// Transposition refusée (#253) : la colonne n'a pas de x'.
    #[error("cannot transpose: column {letter} ({name}) has no x' value")]
    TransposeNoXPrime {
        /// Lettre de position de la colonne (A, B, …).
        letter: String,
        /// Nom de la colonne.
        name: String,
    },
}

/// Alias de résultat du crate.
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
