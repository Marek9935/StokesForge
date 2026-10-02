//! Palettes nommées de couleurs de courbes.
//!
//! Proposées par l'outil de couleur des courbes de l'application : chaque
//! palette est une suite ordonnée de couleurs `#RRGGBB`, pensée pour que des
//! courbes voisines restent distinguables. Elles vivent ici, dans la
//! description neutre du graphe, et non dans l'interface : l'ordre d'une
//! palette est exactement ce qu'il faudra pour colorer **plusieurs** courbes
//! d'un coup (une palette qualitative), ou en dégradé (une palette
//! séquentielle comme Viridis) — une évolution prévue, qui n'aura pas à
//! déplacer ces données.

/// Nature d'une palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaletteNature {
    /// Couleurs distinctes, sans ordre de valeur (séries différentes).
    Qualitative,
    /// Dégradé ordonné (séries qui suivent un paramètre : température,
    /// concentration, temps…).
    Sequential,
}

/// Une palette nommée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamedPalette {
    /// Nom affiché.
    pub name: &'static str,
    /// Nature.
    pub nature: PaletteNature,
    /// Couleurs, dans l'ordre.
    pub colors: &'static [&'static str],
}

/// Les palettes de courbes proposées, la palette par défaut de StokesForge en
/// premier.
pub const CURVE_PALETTES: [NamedPalette; 9] = [
    NamedPalette {
        name: "StokesForge (par défaut)",
        nature: PaletteNature::Qualitative,
        colors: &crate::style::DEFAULT_PALETTE,
    },
    NamedPalette {
        name: "Tableau 10",
        nature: PaletteNature::Qualitative,
        colors: &[
            "#4e79a7", "#f28e2b", "#e15759", "#76b7b2", "#59a14f", "#edc948", "#b07aa1", "#ff9da7",
            "#9c755f", "#bab0ac",
        ],
    },
    NamedPalette {
        name: "Okabe-Ito (lisible par les daltoniens)",
        nature: PaletteNature::Qualitative,
        colors: &[
            "#000000", "#e69f00", "#56b4e9", "#009e73", "#f0e442", "#0072b2", "#d55e00", "#cc79a7",
        ],
    },
    NamedPalette {
        name: "ColorBrewer Set1",
        nature: PaletteNature::Qualitative,
        colors: &[
            "#e41a1c", "#377eb8", "#4daf4a", "#984ea3", "#ff7f00", "#ffff33", "#a65628", "#f781bf",
            "#999999",
        ],
    },
    NamedPalette {
        name: "ColorBrewer Dark2",
        nature: PaletteNature::Qualitative,
        colors: &[
            "#1b9e77", "#d95f02", "#7570b3", "#e7298a", "#66a61e", "#e6ab02", "#a6761d", "#666666",
        ],
    },
    NamedPalette {
        name: "ColorBrewer Paired",
        nature: PaletteNature::Qualitative,
        colors: &[
            "#a6cee3", "#1f78b4", "#b2df8a", "#33a02c", "#fb9a99", "#e31a1c", "#fdbf6f", "#ff7f00",
            "#cab2d6", "#6a3d9a", "#ffff99", "#b15928",
        ],
    },
    NamedPalette {
        name: "Viridis (dégradé)",
        nature: PaletteNature::Sequential,
        colors: &[
            "#440154", "#482878", "#3e4989", "#31688e", "#26828e", "#1f9e89", "#35b779", "#6ece58",
            "#b5de2b", "#fde725",
        ],
    },
    NamedPalette {
        name: "Plasma (dégradé)",
        nature: PaletteNature::Sequential,
        colors: &[
            "#0d0887", "#46039f", "#7201a8", "#9c179e", "#bd3786", "#d8576b", "#ed7953", "#fb9f3a",
            "#fdca26", "#f0f921",
        ],
    },
    NamedPalette {
        name: "Niveaux de gris (dégradé)",
        nature: PaletteNature::Sequential,
        colors: &[
            "#000000", "#262626", "#404040", "#595959", "#737373", "#8c8c8c", "#a6a6a6", "#bfbfbf",
            "#d9d9d9", "#f2f2f2",
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaque_palette_est_non_vide_et_bien_formee() {
        assert_eq!(CURVE_PALETTES[0].colors, &crate::style::DEFAULT_PALETTE);
        for palette in CURVE_PALETTES {
            assert!(palette.colors.len() >= 8, "{}", palette.name);
            for color in palette.colors {
                assert!(
                    color.len() == 7
                        && color.starts_with('#')
                        && color[1..].chars().all(|c| c.is_ascii_hexdigit()),
                    "{} : {color}",
                    palette.name
                );
            }
        }
    }
}
