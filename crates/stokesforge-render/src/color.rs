//! Conversion des couleurs du spec (`#RRGGBB`) vers les deux mondes.
//!
//! `stokesforge-plot` décrit ses couleurs par une chaîne hexadécimale et non par
//! un type structuré, délibérément : c'est le plus petit dénominateur commun
//! entre les moteurs, et la couche neutre n'a pas à savoir composer des
//! couleurs (`stokesforge-plot/src/style.rs`). Le prix à payer est ici, une fois,
//! pour les deux backends.

/// Les composantes d'une couleur `#RRGGBB` ou `#RRGGBBAA`, ou `None` : la
/// syntaxe est celle de [`stokesforge_plot::parse_hex`] (source unique, partagée
/// avec le sélecteur de couleur de l'interface), seul le type de retour
/// diffère. Une couleur venue d'un fichier ou d'un script n'est pas validée à
/// l'entrée (`stokesforge-plot` ne porte que la chaîne), donc ce chemin est
/// réellement atteignable : à charge de l'appelant de choisir son repli.
pub fn parse_hex(text: &str) -> Option<(u8, u8, u8, u8)> {
    stokesforge_plot::parse_hex(text).map(|[r, g, b, a]| (r, g, b, a))
}

/// La couleur d'un trait, avec repli sur `fallback` si la chaîne est
/// illisible ou vide. L'opacité est ignorée — voir [`rgba_or`].
pub fn rgb_or(text: &str, fallback: (u8, u8, u8)) -> (u8, u8, u8) {
    parse_hex(text).map_or(fallback, |(r, g, b, _)| (r, g, b))
}

/// Comme [`rgb_or`], opacité comprise (`#RRGGBBAA` ; `#RRGGBB` est opaque).
/// Un alpha nul est « aucune couleur » : un fond transparent, par exemple,
/// pour intégrer la figure à une autre présentation.
pub fn rgba_or(text: &str, fallback: (u8, u8, u8, u8)) -> (u8, u8, u8, u8) {
    parse_hex(text).unwrap_or(fallback)
}

/// La couleur vue sur du blanc : `rgba` composée sur un fond blanc. C'est la
/// couleur qu'on suppose derrière un fond transparent pour choisir l'encre
/// automatique — une figure sans fond finit le plus souvent sur une page ou
/// une diapositive claire ; sur une diapositive sombre, on choisit alors le
/// texte en blanc à la main.
pub fn over_white((r, g, b, a): (u8, u8, u8, u8)) -> (u8, u8, u8) {
    let alpha = a as f64 / 255.0;
    let blend = |c: u8| (c as f64 * alpha + 255.0 * (1.0 - alpha)).round() as u8;
    (blend(r), blend(g), blend(b))
}

/// Encre sombre, celle du texte et des axes sur un fond clair.
pub const DARK_INK: (u8, u8, u8) = (0x20, 0x20, 0x20);
/// Encre claire, celle du texte et des axes sur un fond sombre.
pub const LIGHT_INK: (u8, u8, u8) = (0xff, 0xff, 0xff);

/// Luminance relative d'une couleur (WCAG 2.x), entre 0 (noir) et 1 (blanc).
pub fn relative_luminance((r, g, b): (u8, u8, u8)) -> f64 {
    let channel = |value: u8| {
        let c = value as f64 / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/// Rapport de contraste WCAG entre deux couleurs, de 1 à 21.
pub fn contrast_ratio(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (light, dark) = if la >= lb { (la, lb) } else { (lb, la) };
    (light + 0.05) / (dark + 0.05)
}

/// Vrai si `background` est un fond « sombre » : l'encre claire y contraste
/// mieux que l'encre sombre.
pub fn is_dark(background: (u8, u8, u8)) -> bool {
    contrast_ratio(LIGHT_INK, background) > contrast_ratio(DARK_INK, background)
}

/// L'encre automatique sur `background` : blanche sur un fond sombre,
/// sombre sur un fond clair. C'est la couleur de tout le texte et de toutes
/// les échelles dont la couleur est laissée vide dans le spec — un fond
/// sombre fait donc passer d'un coup titre, axes, graduations et légende en
/// blanc.
pub fn auto_ink(background: (u8, u8, u8)) -> (u8, u8, u8) {
    if is_dark(background) {
        LIGHT_INK
    } else {
        DARK_INK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_opacite_est_lue_et_composee_sur_du_blanc() {
        assert_eq!(
            rgba_or("#10182080", (0, 0, 0, 255)),
            (0x10, 0x18, 0x20, 0x80)
        );
        assert_eq!(rgba_or("", (1, 2, 3, 255)), (1, 2, 3, 255));
        assert_eq!(
            over_white((0, 0, 0, 0)),
            (255, 255, 255),
            "transparent : blanc"
        );
        assert_eq!(over_white((0, 0, 0, 255)), (0, 0, 0));
        // Un fond noir à moitié transparent reste un fond clair sur du blanc.
        assert_eq!(auto_ink(over_white((0, 0, 0, 0x40))), DARK_INK);
    }

    #[test]
    fn l_encre_automatique_contraste_avec_le_fond() {
        assert_eq!(auto_ink((0xff, 0xff, 0xff)), DARK_INK);
        assert_eq!(auto_ink((0xf5, 0xf0, 0xe1)), DARK_INK, "crème : fond clair");
        assert_eq!(auto_ink((0x00, 0x00, 0x00)), LIGHT_INK);
        assert_eq!(
            auto_ink((0x1e, 0x2a, 0x3a)),
            LIGHT_INK,
            "bleu nuit : fond sombre"
        );
        assert_eq!(
            auto_ink((0x40, 0x40, 0x40)),
            LIGHT_INK,
            "gris foncé : fond sombre"
        );
    }

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
