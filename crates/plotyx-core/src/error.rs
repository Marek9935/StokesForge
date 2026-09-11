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
    }
}
