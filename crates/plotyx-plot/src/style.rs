//! Style d'une courbe et de ses marqueurs, plus la palette par défaut.
//!
//! Rien ici n'emprunte son vocabulaire à un moteur : pas de `'o'`, pas de
//! `linestyle='--'`, pas de `PenStyle`. Des énumérations nommées, que chaque
//! renderer traduit chez lui — la table de traduction vit dans le renderer,
//! jamais dans ce crate.

/// Style de trait d'une courbe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineStyle {
    /// Pas de trait du tout.
    None,
    /// Trait plein (défaut).
    #[default]
    Solid,
    /// Tirets.
    Dash,
    /// Pointillés.
    Dot,
    /// Tiret-point.
    DashDot,
}

/// Forme d'un marqueur de point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SymbolShape {
    /// Aucun marqueur.
    None,
    /// Disque (défaut).
    #[default]
    Circle,
    /// Carré.
    Square,
    /// Triangle.
    Triangle,
    /// Losange.
    Diamond,
    /// Croix droite `+`.
    Plus,
    /// Croix oblique `×`.
    Cross,
}

/// Palette par défaut, en hexadécimal `#RRGGBB` — la forme la plus neutre qui
/// soit : tout moteur de rendu l'accepte telle quelle, et elle traverse une
/// persistance sans conversion.
pub const DEFAULT_PALETTE: [&str; 10] = [
    "#4c78a8", "#f58518", "#54a24b", "#e45756", "#72b7b2", "#eeca3b", "#b279a2", "#ff9da6",
    "#9d755d", "#bab0ac",
];

/// Couleur de la palette par défaut, cyclique.
pub fn palette_color(index: usize) -> &'static str {
    DEFAULT_PALETTE[index % DEFAULT_PALETTE.len()]
}

/// Trait d'une courbe. [`LineStyle::None`] = pas de trait du tout.
///
/// La couleur est une chaîne `#RRGGBB` et non un type structuré : c'est le
/// plus petit dénominateur commun entre les moteurs, et le crate n'a pas à
/// savoir composer des couleurs pour décrire un graphe.
#[derive(Debug, Clone, PartialEq)]
pub struct LineSpec {
    /// Couleur du trait, `#RRGGBB`.
    pub color: String,
    /// Style de trait.
    pub style: LineStyle,
    /// Épaisseur, en points.
    pub width: f64,
}

impl LineSpec {
    /// Trait par défaut, coloré avec la couleur `index` de la palette.
    pub fn from_palette(index: usize) -> Self {
        Self {
            color: palette_color(index).to_string(),
            ..Self::default()
        }
    }
}

impl Default for LineSpec {
    fn default() -> Self {
        Self {
            color: DEFAULT_PALETTE[0].to_string(),
            style: LineStyle::Solid,
            width: 1.5,
        }
    }
}

/// Marqueur d'un point. [`SymbolShape::None`] = aucun marqueur.
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolSpec {
    /// Forme du marqueur.
    pub shape: SymbolShape,
    /// Taille, en points.
    pub size: f64,
    /// Couleur de remplissage, `#RRGGBB`.
    pub color: String,
    /// Contour du marqueur. La chaîne vide signifie « la même couleur que le
    /// remplissage » — un renderer n'a jamais à deviner une valeur absente.
    pub edge_color: String,
}

impl SymbolSpec {
    /// Marqueur par défaut, coloré avec la couleur `index` de la palette.
    pub fn from_palette(index: usize) -> Self {
        Self {
            color: palette_color(index).to_string(),
            ..Self::default()
        }
    }

    /// Couleur effective du contour : celle du remplissage si aucune n'est
    /// désignée. Évite que chaque renderer réécrive la règle.
    pub fn effective_edge_color(&self) -> &str {
        if self.edge_color.is_empty() {
            &self.color
        } else {
            &self.edge_color
        }
    }
}

impl Default for SymbolSpec {
    fn default() -> Self {
        Self {
            shape: SymbolShape::Circle,
            size: 7.0,
            color: DEFAULT_PALETTE[0].to_string(),
            edge_color: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_cycles() {
        assert_eq!(palette_color(0), DEFAULT_PALETTE[0]);
        assert_eq!(palette_color(DEFAULT_PALETTE.len()), DEFAULT_PALETTE[0]);
        assert_eq!(palette_color(DEFAULT_PALETTE.len() + 1), DEFAULT_PALETTE[1]);
    }

    #[test]
    fn default_styles_take_the_first_palette_colour() {
        assert_eq!(LineSpec::default().color, DEFAULT_PALETTE[0]);
        assert_eq!(SymbolSpec::default().color, DEFAULT_PALETTE[0]);
    }

    #[test]
    fn from_palette_colours_without_touching_the_rest() {
        let line = LineSpec::from_palette(1);
        assert_eq!(line.color, DEFAULT_PALETTE[1]);
        assert_eq!(line.style, LineSpec::default().style);
        assert_eq!(SymbolSpec::from_palette(1).color, DEFAULT_PALETTE[1]);
    }

    #[test]
    fn an_empty_edge_colour_means_the_fill_colour() {
        let symbol = SymbolSpec::default();
        assert_eq!(symbol.effective_edge_color(), symbol.color);
        let outlined = SymbolSpec {
            edge_color: "#000000".to_string(),
            ..SymbolSpec::default()
        };
        assert_eq!(outlined.effective_edge_color(), "#000000");
    }
}

/// Les composantes `[r, g, b, a]` d'une couleur `#RRGGBB` (opaque) ou
/// `#RRGGBBAA`, ou `None`.
///
/// **Source unique** de la syntaxe des couleurs du spec : le rendu
/// (`plotyx_render::color::parse_hex`) et le sélecteur de couleur de
/// l'interface (`plotyx-app::color_picker::parse_hex`) ne font qu'adapter son
/// résultat à leur type, pour qu'une couleur acceptée d'un côté ne puisse pas
/// être lue autrement de l'autre (issue #25). Tolérant sur ce qui est
/// tolérable — casse indifférente, espaces de bordure ignorés, `#` facultatif —
/// et net sur le reste : une chaîne mal formée (longueur, caractère non
/// hexadécimal, non ASCII) rend `None` plutôt qu'une couleur inventée.
pub fn parse_hex(text: &str) -> Option<[u8; 4]> {
    let text = text.trim();
    let text = text.strip_prefix('#').unwrap_or(text);
    // Uniquement des chiffres hexadécimaux : `from_str_radix` accepterait
    // aussi un `+` de tête (« +c78a8 » se lirait comme une couleur).
    if !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |index: usize| u8::from_str_radix(text.get(index..index + 2)?, 16).ok();
    match text.len() {
        6 => Some([byte(0)?, byte(2)?, byte(4)?, 255]),
        8 => Some([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => None,
    }
}

#[cfg(test)]
mod parse_hex_tests {
    use super::parse_hex;

    #[test]
    fn syntax_edge_cases() {
        assert_eq!(parse_hex("#4c78a8"), Some([0x4c, 0x78, 0xa8, 255]));
        assert_eq!(parse_hex("4C78A8"), Some([0x4c, 0x78, 0xa8, 255]));
        assert_eq!(parse_hex("  #4c78a880 "), Some([0x4c, 0x78, 0xa8, 0x80]));
        for bad in [
            "",
            "#",
            "#12345",
            "#1234567",
            "#gg0000",
            "#é00000",
            "4c78a8ff0",
            "+c78a8",
        ] {
            assert_eq!(parse_hex(bad), None, "{bad:?}");
        }
    }
}
