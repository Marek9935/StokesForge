//! Named palettes of curve colors.
//!
//! Offered by the application's curve color tool: each
//! palette is an ordered list of `#RRGGBB` colors, designed so that neighboring
//! curves stay distinguishable. They live here, in the neutral
//! graph description, and not in the interface: the order of a
//! palette is exactly what will be needed to color **several** curves
//! at once (a qualitative palette), or as a gradient (a
//! sequential palette such as Viridis) — a planned change, which will not have to
//! move this data.

/// Nature of a palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaletteNature {
    /// Distinct colors, with no value order (different series).
    Qualitative,
    /// Ordered gradient (series that follow a parameter: temperature,
    /// concentration, time…).
    Sequential,
}

/// A named palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamedPalette {
    /// Displayed name.
    pub name: &'static str,
    /// Nature.
    pub nature: PaletteNature,
    /// Colors, in order.
    pub colors: &'static [&'static str],
}

/// The curve palettes offered, the StokesForge default palette
/// first.
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
    fn every_palette_is_non_empty_and_well_formed() {
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
