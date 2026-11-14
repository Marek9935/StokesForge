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
        // A typed name shows as typed (#501), like the editors.
        SeriesLabel::Typed { name, .. } => name.clone(),
        // The plain text Typst produces from the bound template; empty if it
        // does not compile (the lists then show the grey fallback of an empty
        // text).
        SeriesLabel::Template { template, fields } => {
            let values = stokesforge_render::variables::VariableValues {
                unknown: tr!("graph-series-broken-reference"),
                ..fields.into()
            };
            let bound = stokesforge_render::variables::bind(template, &values);
            stokesforge_render::formula::plain_text(&bound).unwrap_or_default()
        }
        SeriesLabel::Row(row) => tr!("graph-row-series", number = row.saturating_add(1)),
        SeriesLabel::Missing => tr!("graph-series-broken-reference"),
    }
}

/// The rows of the variable reminder (#501), one per entry of
/// [`stokesforge_render::variables::VARIABLES`]: what is typed (the two names,
/// identical in every language) and what it gives (the field, as the header
/// row is named on screen). Built from the table, so a new variable reaches
/// every help; the last row is `#col.A.ln`, the explicit column (#501).
pub fn variable_help_rows() -> Vec<(String, String)> {
    use stokesforge_plot::LegendField;
    let named = ("#col.A.ln".to_owned(), tr!("plot-variables-col"));
    stokesforge_render::variables::VARIABLES
        .iter()
        .map(|(long, short, field)| {
            let gives = match field {
                LegendField::LongName => tr!("legend-text-from-long-name"),
                LegendField::ShortName => tr!("legend-text-from-short-name"),
                LegendField::Units => tr!("legend-text-from-units"),
                LegendField::Comments => tr!("legend-text-from-comments"),
                LegendField::XPrime => tr!("legend-text-from-x-prime"),
            };
            (format!("#{long}  #{short}"), gives)
        })
        .chain(std::iter::once(named))
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// #501: every variable of the table appears in the help, with both names
    /// and what it gives (English catalog). Witness: drop a row of
    /// `variable_help_rows` (or build it from a hand-written list that lags
    /// the table) and the entry is missing.
    #[test]
    fn every_variable_appears_in_the_help() {
        let rows = variable_help_rows();
        assert_eq!(
            rows.len(),
            stokesforge_render::variables::VARIABLES.len() + 1
        );
        for (long, short, _) in stokesforge_render::variables::VARIABLES {
            let line = rows
                .iter()
                .find(|(names, _)| *names == format!("#{long}  #{short}"))
                .unwrap_or_else(|| panic!("#{long} is missing from the help: {rows:?}"));
            assert!(!line.1.is_empty());
        }
        let gives: Vec<&str> = rows.iter().map(|(_, gives)| gives.as_str()).collect();
        assert_eq!(
            gives,
            [
                "Long Name",
                "Short Name",
                "Units",
                "Comments",
                "x'",
                "The same, read from column A of the sheet"
            ]
        );
    }

    /// #501 lot 4: the help names the explicit column. Witness: drop the row.
    #[test]
    fn the_help_lists_the_explicit_column() {
        let rows = variable_help_rows();
        assert!(
            rows.iter()
                .any(|(names, gives)| names == "#col.A.ln"
                    && gives.contains("column A of the sheet")),
            "{rows:?}"
        );
    }
}
