//! Validation des noms d'objets (dossiers, classeurs, feuilles, colonnes...).

use crate::error::{CoreError, CoreResult};

/// Caractères réservés par la grammaire de chemins `[Book]Sheet!Col`.
pub const RESERVED_NAME_CHARS: &[char] = &['[', ']', '!', '/'];

/// Longueur maximale d'un nom, en caractères Unicode (`chars().count()`, pas
/// en octets) — C7 (`guide-de-correction-robustesse.md` §F4·F9). Un nom de
/// cent mille caractères traversait le disque sans borne ; 255 est généreux
/// pour un nom d'objet et reste une constante nommée, pas une devinette.
pub const MAX_NAME_CHARS: usize = 255;

/// Vrai pour un caractère que `check_name` refuse à l'intérieur d'un nom :
/// tout caractère de contrôle C0/C1 (`char::is_control`, qui couvre
/// `\0..=\x1F`, DEL `\x7F` et `\u{80}..=\u{9F}`), et les marques de
/// formatage/direction Unicode invisibles ou trompeuses les plus courantes —
/// espaces et joints de largeur nulle (`U+200B..=U+200F`) et les marques
/// d'isolation/inversion de sens (`U+202A..=U+202E`, `U+2066..=U+2069`), dont
/// RIGHT-TO-LEFT OVERRIDE (`U+202E`) : un nom qui les porte peut retourner
/// visuellement le reste d'un chemin ou casser tout affichage tabulaire et
/// tout message d'erreur qui le cite (C7).
fn is_control_or_deceptive_format(c: char) -> bool {
    c.is_control()
        || matches!(c,
            '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
        )
}

/// Valide un nom d'objet : non vide, sans espace de bordure, sans caractère
/// réservé, sans caractère de contrôle ni marque de formatage/direction
/// Unicode trompeuse, et d'au plus [`MAX_NAME_CHARS`] caractères. Rend le nom
/// inchangé (emprunté) si valide.
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
    if name.chars().any(is_control_or_deceptive_format) {
        return Err(CoreError::InvalidName(
            "caractère de contrôle ou de formatage Unicode refusé dans un nom".to_string(),
        ));
    }
    let char_count = name.chars().count();
    if char_count > MAX_NAME_CHARS {
        return Err(CoreError::InvalidName(format!(
            "nom de {char_count} caractères, {MAX_NAME_CHARS} au maximum"
        )));
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

    /// C7 (`guide-de-correction-robustesse.md` §F4·F9) — reproduit ici le
    /// scénario de `b21_les_noms_de_controle_sont_refuses`
    /// (`plotyx-robustesse`) au plus près de la fonction qu'il corrige.
    #[test]
    fn rejects_control_and_deceptive_format_characters() {
        for name in [
            "A\nB",           // saut de ligne
            "A\tB",           // tabulation
            "A\u{0}B",        // octet nul
            "A\u{202e}B",     // RIGHT-TO-LEFT OVERRIDE
            "A\u{200b}B",     // ZERO WIDTH SPACE
            "A\u{7f}B",       // DEL
            "A\u{200f}B",     // RIGHT-TO-LEFT MARK
            "A\u{2066}B",     // LEFT-TO-RIGHT ISOLATE
        ] {
            assert!(
                matches!(check_name(name), Err(CoreError::InvalidName(_))),
                "attendu InvalidName pour {name:?}"
            );
        }
    }

    #[test]
    fn rejects_names_longer_than_the_limit() {
        let trop_long = "x".repeat(MAX_NAME_CHARS + 1);
        assert!(matches!(
            check_name(&trop_long),
            Err(CoreError::InvalidName(_))
        ));

        let a_la_limite = "x".repeat(MAX_NAME_CHARS);
        assert_eq!(check_name(&a_la_limite), Ok(a_la_limite.as_str()));
    }
}
