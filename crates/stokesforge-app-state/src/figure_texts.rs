//! Texts that a figure draws by itself, phrased by the catalog (#369).
//!
//! The renderer (`stokesforge-render`) takes them as data
//! ([`FigureTexts`]); the model crates make no sentence. The script and CLI
//! exports keep the fixed English default of the renderer.

use stokesforge_core::SeriesLabel;
use stokesforge_render::FigureTexts;

use crate::tr;

/// The texts of a figure in the current language of the interface.
///
/// The language tag is the one of [`crate::i18n::language`]: the screen cache
/// rebuilds the figure only when it changes.
pub fn figure_texts() -> FigureTexts {
    FigureTexts::new(
        crate::i18n::language().tag(),
        tr!("graph-series-broken-reference"),
        |name| tr!("graph-series-broken-reference-named", name = name),
        |count| tr!("graph-legend-more", count = count),
        |number| tr!("graph-row-series", number = number),
    )
}

/// The displayed name of a series for lists (Object Manager, properties
/// window): same rule as the legend, phrased in the current language.
pub fn series_label_text(label: &SeriesLabel) -> String {
    match label {
        SeriesLabel::Text(text) => text.clone(),
        SeriesLabel::Row(row) => tr!("graph-row-series", number = row.saturating_add(1)),
        SeriesLabel::Missing => tr!("graph-series-broken-reference"),
    }
}

/// The displayed name of the predefined theme `index` of
/// [`stokesforge_plot::THEMES`] (clamped), in the current language.
pub fn theme_name(index: usize) -> String {
    match index.min(stokesforge_plot::THEMES.len() - 1) {
        0 => tr!("plot-theme-light"),
        _ => tr!("plot-theme-dark"),
    }
}
