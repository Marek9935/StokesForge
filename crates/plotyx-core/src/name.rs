//! Validation des noms d'objets (dossiers, classeurs, feuilles, colonnes...).

use crate::error::{CoreError, CoreResult};

/// Caractères réservés par la grammaire de chemins `[Book]Sheet!Col`.
pub const RESERVED_NAME_CHARS: &[char] = &['[', ']', '!', '/'];

/// Valide un nom d'objet : non vide, sans espace de bordure, sans caractère réservé.
/// Rend le nom inchangé (emprunté) si valide.
pub fn check_name(name: &str) -> CoreResult<&str> {
    if name.is_empty() {
        return Err(CoreError::InvalidName(name.to_string()));
    }
    if name != name.trim() {
        return Err(CoreError::InvalidName(name.to_string()));
    }
    if name.contains(RESERVED_NAME_CHARS) {
        return Err(CoreError::InvalidName(name.to_string()));
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_names() {
        for name in ["", "  ", " A", "A ", "A!B", "[A]", "a/b", "A]"] {
            assert!(
                matches!(check_name(name), Err(CoreError::InvalidName(_))),
                "attendu InvalidName pour {name:?}"
            );
        }
    }

    #[test]
    fn accepts_valid_names() {
        for name in ["A", "Book1", "Température (K)", "x-y"] {
            assert_eq!(check_name(name), Ok(name));
        }
    }
}
