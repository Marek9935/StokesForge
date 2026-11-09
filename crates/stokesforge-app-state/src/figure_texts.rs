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
        // The plain text Typst produces; empty if the template does not
        // compile (the lists then show the grey fallback of an empty text).
        SeriesLabel::Template { markup } => {
            stokesforge_render::formula::plain_text(markup).unwrap_or_default()
        }
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

/// The label of a « Text from » choice of a legend (#499), in the current
/// language. The names of the header rows are those of the worksheet.
pub fn legend_text_from_label(from: &stokesforge_plot::LegendTextFrom) -> String {
    use stokesforge_plot::LegendTextFrom as From;
    match from {
        From::LongName => tr!("legend-text-from-long-name"),
        From::Comments => tr!("legend-text-from-comments"),
        From::XPrime => tr!("legend-text-from-x-prime"),
        From::Units => tr!("legend-text-from-units"),
        From::ShortName => tr!("legend-text-from-short-name"),
        From::LongNameUnits => tr!("legend-text-from-long-name-units"),
        From::LongNameComments => tr!("legend-text-from-long-name-comments"),
        From::Custom => tr!("legend-text-from-custom"),
        From::Unknown(_) => tr!("legend-text-from-unknown"),
    }
}
