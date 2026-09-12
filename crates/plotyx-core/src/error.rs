//! Type d'erreur unique du crate.

use crate::id::NodeId;

/// Erreur du modèle de données. Une variante par cause, jamais de fourre-tout.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoreError {
    /// Nom d'objet syntaxiquement invalide (vide, mal détouré, caractère réservé).
    #[error("nom invalide : {0}")]
    InvalidName(String),
    /// Nom déjà pris dans le conteneur cible.
    #[error("nom déjà pris : {0}")]
    DuplicateName(String),
    /// Chemin `[Book]Sheet!Col` mal formé ou non résolu.
    #[error("chemin invalide : {0}")]
    Path(String),
    /// Opération de structure impossible (mauvais type de parent, cycle, ...).
    #[error("structure : {0}")]
    Structure(String),
    /// Identifiant absent de l'arbre courant.
    #[error("nœud inconnu : {0}")]
    UnknownNode(NodeId),
    /// Index de ligne ou de colonne hors bornes.
    #[error("index hors bornes : {0}")]
    IndexOutOfBounds(String),
    /// Valeurs d'un type incompatible avec la colonne.
    #[error("type incompatible : {0}")]
    TypeMismatch(String),
    /// `IdAllocator` a épuisé `u64` : plus aucun identifiant à attribuer.
    /// B1 (`guide-de-correction-robustesse.md` §F2/F3/F4) — remplace le
    /// débordement silencieux (`self.next += 1` sur `u64::MAX`, qui faisait
    /// reculer le compteur et réattribuer un identifiant déjà vivant).
    #[error("identifiants épuisés : plus aucun nœud ne peut être créé")]
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
        "version concurrente : la colonne {column} attendait la version {expected}, elle est \
         à la version {found}"
    )]
    ConcurrentModification {
        /// Colonne visée par l'écriture refusée.
        column: NodeId,
        /// Version que l'appelant croyait encore valable.
        expected: u64,
        /// Version réelle de la colonne au moment du refus.
        found: u64,
    },
}

/// Alias de résultat du crate.
pub type CoreResult<T> = Result<T, CoreError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_messages_are_in_french() {
        assert_eq!(
            CoreError::InvalidName("A!B".to_string()).to_string(),
            "nom invalide : A!B"
        );
        assert_eq!(
            CoreError::DuplicateName("Sheet1".to_string()).to_string(),
            "nom déjà pris : Sheet1"
        );
        assert_eq!(
            CoreError::Path("chemin vide".to_string()).to_string(),
            "chemin invalide : chemin vide"
        );
        assert_eq!(
            CoreError::Structure("cycle détecté".to_string()).to_string(),
            "structure : cycle détecté"
        );
        assert_eq!(
            CoreError::UnknownNode(NodeId::from_raw(412)).to_string(),
            "nœud inconnu : nœud 412"
        );
        assert_eq!(
            CoreError::IndexOutOfBounds("ligne 5".to_string()).to_string(),
            "index hors bornes : ligne 5"
        );
        assert_eq!(
            CoreError::TypeMismatch("texte attendu".to_string()).to_string(),
            "type incompatible : texte attendu"
        );
        assert_eq!(
            CoreError::IdentifiersExhausted.to_string(),
            "identifiants épuisés : plus aucun nœud ne peut être créé"
        );
        assert_eq!(
            CoreError::ConcurrentModification {
                column: NodeId::from_raw(7),
                expected: 3,
                found: 4,
            }
            .to_string(),
            "version concurrente : la colonne nœud 7 attendait la version 3, elle est à la \
             version 4"
        );
    }
}
