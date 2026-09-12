//! Conversion des couleurs du spec (`#RRGGBB`) vers les deux mondes.
//!
//! `plotyx-plot` décrit ses couleurs par une chaîne hexadécimale et non par
//! un type structuré, délibérément : c'est le plus petit dénominateur commun
//! entre les moteurs, et la couche neutre n'a pas à savoir composer des
//! couleurs (`plotyx-plot/src/style.rs`). Le prix à payer est ici, une fois,
//! pour les deux backends.

/// Les composantes d'une couleur `#RRGGBB` ou `#RRGGBBAA`, ou `None`.
///
/// Tolérant sur ce qui est tolérable — casse indifférente, espaces de bordure
/// ignorés, `#` facultatif — et net sur le reste : une chaîne mal formée rend
/// `None` plutôt qu'une couleur inventée, à charge de l'appelant de choisir
/// son repli. Une couleur venue d'un fichier ou d'un script n'est pas
/// validée à l'entrée (`plotyx-plot` ne porte que la chaîne), donc ce chemin
/// est réellement atteignable.
pub fn parse_hex(text: &str) -> Option<(u8, u8, u8, u8)> {
    let text = text.trim().strip_prefix('#').unwrap_or(text.trim());
    if !text.is_ascii() {
        return None;
    }
    let byte = |index: usize| u8::from_str_radix(text.get(index..index + 2)?, 16).ok();
    match text.len() {
        6 => Some((byte(0)?, byte(2)?, byte(4)?, 255)),
        8 => Some((byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => None,
    }
}

/// La couleur d'un trait, avec repli sur `fallback` si la chaîne est
/// illisible ou vide.
pub fn rgb_or(text: &str, fallback: (u8, u8, u8)) -> (u8, u8, u8) {
    parse_hex(text).map_or(fallback, |(r, g, b, _)| (r, g, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lit_les_trois_formes_acceptees() {
        assert_eq!(parse_hex("#4c78a8"), Some((0x4c, 0x78, 0xa8, 255)));
        assert_eq!(parse_hex("4C78A8"), Some((0x4c, 0x78, 0xa8, 255)));
        assert_eq!(parse_hex("  #4c78a880 "), Some((0x4c, 0x78, 0xa8, 0x80)));
    }

    #[test]
    fn refuse_plutot_que_dinventer() {
        for mauvais in ["", "#", "#abc", "#12345g", "bleu", "#4c78a8ff00", "#4c78aµ"] {
            assert_eq!(
                parse_hex(mauvais),
                None,
                "« {mauvais} » aurait dû être refusé"
            );
        }
    }

    #[test]
    fn le_repli_est_a_lappelant() {
        assert_eq!(rgb_or("pas une couleur", (1, 2, 3)), (1, 2, 3));
        assert_eq!(rgb_or("#010203", (9, 9, 9)), (1, 2, 3));
    }
}
