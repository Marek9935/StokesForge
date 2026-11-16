//! The built-in graph themes (#544, S7 of #514): the catalogue the library of
//! the application offers before the user's own. It lives in this crate, not in
//! `stokesforge-app-state`, so that scripts (`stokesforge-script`, #546) read
//! the same catalogue; the application keeps only the displayed names.
//!
//! The catalogue, in its fixed order (the index is `ThemeRef::Builtin`'s):
//!
//! | index | key | look |
//! |---|---|---|
//! | 0 | `light` | colours only |
//! | 1 | `dark` | colours only |
//! | 2 | `publication` | one layer, plot area 8.5 cm wide, closed frame, ticks inside, black and greys, 8-9 pt texts |
//! | 3 | `publication-wide` | the same, 17 cm wide |
//! | 4 | `presentation` | large texts, thick lines |
//! | 5 | `fit-residuals` | two stacked layers, heights 3:1, X numbers and title under the bottom layer only |
//! | 6 | `kinetics` | a group of curves on the Viridis gradient, colour bar on the right |
//! | 7 | `side-by-side` | two equal layers in one row |
//! | 8 | `color-map` | gradient curves on Plasma with a colour bar, no legend |
//!
//! **No built-in has a grid** (decision of form 544-1). Light and Dark stay
//! colour-only ([`GraphTheme::from_colors`]); the seven others are **full
//! style** themes: each is a [`PlotSpec`] built with the ordinary constructors
//! on a placeholder column, then passed through [`GraphTheme::capture`], so a
//! built-in holds exactly what a theme captured from a graph holds, and never a
//! data reference.
//!
//! The key is the stable identity (file keys `builtin:<key>`, override keys);
//! the name stored in the theme is the English one, never shown: the interface
//! shows `display_name` of the application (`plot-theme-<key>` of the catalog).
//!
//! What a theme cannot express: **a zero line** under the residuals. A line at
//! `y = 0` is a data object (a shape anchored to data, or a constant series),
//! and a theme holds no data; the bottom layer of Fit + Residuals carries the
//! points only.

use crate::{
    AxisSide, ColorBarOrientation, ColorBarPosition, ColorBarSpec, GraphTheme, LengthUnit,
    LineStyle, PanelSpec, Panels, PlotSpec, SeriesSpec, SymbolShape, TickDirection,
    GRADIENT_PRESETS, THEMES,
};

/// Number of built-in themes.
pub const COUNT: usize = 9;

/// Stable keys, by catalogue index. Light and Dark keep the keys of the
/// predefined colours ([`THEMES`]), which the files of #539 already carry.
pub const KEYS: [&str; COUNT] = [
    "light",
    "dark",
    "publication",
    "publication-wide",
    "presentation",
    "fit-residuals",
    "kinetics",
    "side-by-side",
    "color-map",
];

/// English names, by index: what a built-in theme stores, and what a user
/// theme may not be called.
pub const NAMES: [&str; COUNT] = [
    "Light",
    "Dark",
    "Publication",
    "Publication Wide",
    "Presentation",
    "Fit + Residuals",
    "Kinetics",
    "Side by Side",
    "Color Map",
];

/// The index of the built-in with the stable `key`.
pub fn index_of(key: &str) -> Option<usize> {
    KEYS.iter().position(|k| *k == key)
}

/// The original of the built-in `index` (no override). `None` past the
/// catalogue.
pub fn original(index: usize) -> Option<GraphTheme> {
    let name = *NAMES.get(index)?;
    Some(match index {
        0 | 1 => GraphTheme::from_colors(name, &THEMES[index]),
        2 => publication(name, 8.5),
        3 => publication(name, 17.0),
        4 => presentation(name),
        5 => fit_residuals(name),
        6 => gradient_group(name, "viridis", false),
        7 => side_by_side(name),
        _ => gradient_group(name, "plasma", true),
    })
}

/// A placeholder column: capture replaces every column of a series.
fn column() -> stokesforge_id::NodeId {
    stokesforge_id::NodeId::from_raw(1)
}

fn cm(value: f64) -> f64 {
    LengthUnit::Cm.to_points(value)
}

fn sized(size: f64) -> crate::TextFont {
    crate::TextFont {
        size: Some(size),
        ..crate::TextFont::default()
    }
}

/// The ink of every text and scale of `panel`, the size of its titles (`None`
/// = Auto, which follows the graph text scale), and the grid off on the four
/// axes.
fn ink(panel: &mut PanelSpec, color: &str, title_size: Option<f64>) {
    panel.title_color = color.to_string();
    if let Some(size) = title_size {
        panel.title_font = sized(size);
    }
    for side in [
        AxisSide::Bottom,
        AxisSide::Left,
        AxisSide::Right,
        AxisSide::Top,
    ] {
        let axis = panel.axis_mut(side);
        axis.grid = false;
        axis.title_color = color.to_string();
        axis.tick_color = color.to_string();
        axis.tick_mark_color = color.to_string();
        if let Some(size) = title_size {
            axis.title_font = sized(size);
        }
    }
}

fn spec_of(panels: Vec<PanelSpec>) -> PlotSpec {
    let mut spec = PlotSpec::empty("");
    spec.panels = Panels::from(panels);
    spec.background = "#ffffff".to_string();
    spec.title_visible = false;
    spec
}

fn theme(name: &str, spec: &PlotSpec) -> GraphTheme {
    GraphTheme::capture(name, spec)
}

/// A line with no marker.
fn plain_line(index: usize, color: &str, style: LineStyle, width: f64) -> SeriesSpec {
    let mut series = SeriesSpec::from_palette(column(), index);
    series.line.color = color.to_string();
    series.line.style = style;
    series.line.width = width;
    series.symbol.shape = SymbolShape::None;
    series
}

/// Points only.
fn points(color: &str, size: f64) -> SeriesSpec {
    let mut series = SeriesSpec::new(column());
    series.plot_type = crate::PlotType::Scatter;
    series.line.style = LineStyle::None;
    series.symbol.shape = SymbolShape::Circle;
    series.symbol.size = size;
    series.symbol.color = color.to_string();
    series.line.color = color.to_string();
    series
}

/// Publication and Publication Wide: black and greys, a closed frame with the
/// ticks inside, small texts (text scale 60 %), no grid, a plot area `width_cm` wide.
fn publication(name: &str, width_cm: f64) -> GraphTheme {
    let mut panel = PanelSpec::at(0, 0);
    ink(&mut panel, "#000000", None);
    panel.legend.color = "#000000".to_string();
    panel.legend.frame = false;
    for side in [AxisSide::Bottom, AxisSide::Left] {
        let axis = panel.axis_mut(side);
        axis.ticks = true;
        axis.tick_direction = TickDirection::In;
        axis.minor_ticks = true;
        axis.tick_width = Some(0.75);
    }
    // Closing copies the ticks of the bottom and left axes onto the mirrors.
    panel.set_frame_closed(true);
    panel.series = vec![
        plain_line(0, "#000000", LineStyle::Solid, 1.0),
        plain_line(1, "#555555", LineStyle::Dash, 1.0),
        plain_line(2, "#999999", LineStyle::Dot, 1.0),
    ];
    let mut spec = spec_of(vec![panel]);
    spec.text_scale = Some(0.6);
    spec.frame_widths = vec![cm(width_cm)];
    spec.frame_heights = vec![cm(width_cm) * 0.75];
    theme(name, &spec)
}

/// Large texts (text scale 150 %) and thick lines, for a slide.
fn presentation(name: &str) -> GraphTheme {
    let mut panel = PanelSpec::at(0, 0);
    ink(&mut panel, "#000000", None);
    for side in [AxisSide::Bottom, AxisSide::Left] {
        let axis = panel.axis_mut(side);
        axis.tick_length = Some(8.0);
        axis.tick_width = Some(2.0);
    }
    panel.series = (0..3)
        .map(|i| {
            let mut series = SeriesSpec::from_palette(column(), i);
            series.line.width = 3.0;
            series.symbol.shape = SymbolShape::None;
            series
        })
        .collect();
    let mut spec = spec_of(vec![panel]);
    spec.text_scale = Some(1.5);
    spec.frame_widths = vec![560.0];
    spec.frame_heights = vec![380.0];
    theme(name, &spec)
}

/// Two stacked layers in one column: the data and its fit (3/4 of the height),
/// the residuals (1/4). The X numbers and title are under the bottom layer
/// only.
fn fit_residuals(name: &str) -> GraphTheme {
    let mut top = PanelSpec::at(0, 0);
    ink(&mut top, "#000000", Some(12.0));
    top.bottom.tick_labels = false;
    top.bottom.title_visible = false;
    top.series = vec![
        points("#d62728", 4.0),
        plain_line(0, "#000000", LineStyle::Solid, 1.5),
    ];
    let mut bottom = PanelSpec::at(1, 0);
    ink(&mut bottom, "#000000", Some(12.0));
    bottom.title_visible = false;
    bottom.legend.visible = false;
    bottom.series = vec![points("#d62728", 3.0)];
    let mut spec = spec_of(vec![top, bottom]);
    spec.frame_widths = vec![360.0];
    spec.frame_heights = vec![225.0, 75.0];
    theme(name, &spec)
}

/// Curves of one group coloured along a gradient preset, with the colour bar
/// of the group. `bare`: the Color Map look (lines only, thicker bar, no
/// legend); otherwise the Kinetics look (lines with markers).
fn gradient_group(name: &str, preset: &str, bare: bool) -> GraphTheme {
    let mut panel = PanelSpec::at(0, 0);
    ink(&mut panel, "#000000", Some(12.0));
    let curves = if bare { 8 } else { 5 };
    panel.series = (0..curves)
        .map(|i| {
            let mut series = SeriesSpec::from_palette(column(), i);
            series.group = Some(1);
            series.line.width = if bare { 2.0 } else { 1.5 };
            if bare {
                series.symbol.shape = SymbolShape::None;
            } else {
                series.plot_type = crate::PlotType::LineScatter;
                series.symbol.size = 4.0;
            }
            series
        })
        .collect();
    let stops: Vec<&str> = GRADIENT_PRESETS
        .iter()
        .find(|p| p.id == preset)
        .map(|p| p.stops.to_vec())
        .unwrap_or_default();
    panel.set_group_gradient(1, &stops);
    panel.color_bar = Some(ColorBarSpec {
        orientation: ColorBarOrientation::Vertical,
        position: ColorBarPosition::Right,
        thickness: bare.then_some(16.0),
        ..ColorBarSpec::default()
    });
    panel.legend.visible = !bare;
    let mut spec = spec_of(vec![panel]);
    spec.frame_widths = vec![360.0];
    spec.frame_heights = vec![270.0];
    theme(name, &spec)
}

/// Two equal layers in one row.
fn side_by_side(name: &str) -> GraphTheme {
    let layer = |column_index: usize| {
        let mut panel = PanelSpec::at(0, column_index);
        ink(&mut panel, "#000000", Some(12.0));
        panel.series = vec![SeriesSpec::from_palette(column(), 0)];
        panel
    };
    let mut spec = spec_of(vec![layer(0), layer(1)]);
    spec.frame_widths = vec![240.0, 240.0];
    spec.frame_heights = vec![240.0];
    theme(name, &spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LayerPlacement;

    #[test]
    fn the_catalogue_is_complete_and_keys_are_unique() {
        assert_eq!(KEYS.len(), NAMES.len());
        for (index, key) in KEYS.iter().enumerate() {
            assert_eq!(index_of(key), Some(index));
            assert_eq!(original(index).expect("theme").name, NAMES[index]);
        }
        assert_eq!(KEYS[0], THEMES[0].id);
        assert_eq!(KEYS[1], THEMES[1].id);
        assert!(original(COUNT).is_none());
    }

    #[test]
    fn only_light_and_dark_are_colour_only() {
        for index in 0..COUNT {
            assert_eq!(original(index).expect("theme").colors_only, index < 2);
        }
    }

    #[test]
    fn no_built_in_has_a_grid_on_any_axis() {
        for (index, name) in NAMES.iter().enumerate() {
            let theme = original(index).expect("theme");
            for panel in theme.style.panels.iter() {
                for side in [
                    AxisSide::Bottom,
                    AxisSide::Left,
                    AxisSide::Right,
                    AxisSide::Top,
                ] {
                    assert!(!panel.axis(side).grid, "{name} has a grid on {side:?}");
                }
            }
        }
    }

    #[test]
    fn a_built_in_holds_no_data_and_no_free_placement() {
        for (index, name) in NAMES.iter().enumerate().skip(2) {
            let theme = original(index).expect("theme");
            assert_eq!(theme.style.layer_placement, LayerPlacement::Grid);
            assert_eq!(
                theme,
                GraphTheme::capture(*name, &theme.style),
                "{name} is its own capture"
            );
        }
    }

    #[test]
    fn publication_is_8_5_cm_wide_and_wide_is_17() {
        for (index, cm_width) in [(2, 8.5), (3, 17.0)] {
            let theme = original(index).expect("theme");
            let widths = theme.style.sanitized_frame_widths().expect("stored");
            assert_eq!(widths.len(), 1);
            let expected = cm_width / 2.54 * 72.0;
            assert!(
                (widths[0] - expected).abs() < 1e-9,
                "{} wide: {} pt",
                cm_width,
                widths[0]
            );
        }
        let panel = &original(2).expect("theme").style.panels[0];
        assert!(panel.frame_closed());
        assert_eq!(panel.bottom.tick_direction, TickDirection::In);
        assert_eq!(panel.top.tick_direction, TickDirection::In);
    }

    /// #545 (form 545-2): Publication and Presentation are a text scale, their
    /// texts stay on Auto. Witness: a hand-set size back in `ink` fails both.
    #[test]
    fn publication_and_presentation_are_a_scale_with_texts_on_auto() {
        for (index, scale) in [(2, 0.6), (3, 0.6), (4, 1.5)] {
            let theme = original(index).expect("theme");
            assert_eq!(theme.style.text_scale, Some(scale), "theme {index}");
            let panel = &theme.style.panels[0];
            assert_eq!(panel.title_font.size, None);
            assert_eq!(panel.legend.font.size, None);
            assert_eq!(panel.axis(AxisSide::Bottom).title_font.size, None);
            assert_eq!(theme.style.title_font.size, None);
        }
    }

    #[test]
    fn fit_residuals_has_two_layers_three_to_one_with_x_numbers_below_only() {
        let theme = original(5).expect("theme");
        let panels = theme.style.panels.as_slice();
        assert_eq!(panels.len(), 2);
        assert_eq!((panels[0].row, panels[1].row), (0, 1));
        assert_eq!((panels[0].column, panels[1].column), (0, 0));
        let heights = theme.style.sanitized_frame_heights().expect("stored");
        assert!((heights[0] / heights[1] - 3.0).abs() < 1e-9);
        assert!(!panels[0].bottom.tick_labels && !panels[0].bottom.title_visible);
        assert!(panels[1].bottom.tick_labels && panels[1].bottom.title_visible);
    }

    #[test]
    fn the_gradient_built_ins_carry_a_bar_on_the_right() {
        for (index, preset) in [(6, "viridis"), (8, "plasma")] {
            let theme = original(index).expect("theme");
            let panel = &theme.style.panels[0];
            let bar = panel.color_bar.as_ref().expect("a colour bar");
            assert_eq!(bar.position, ColorBarPosition::Right);
            let stops = GRADIENT_PRESETS
                .iter()
                .find(|p| p.id == preset)
                .expect("preset")
                .stops;
            let expected: Vec<String> = stops.iter().map(|s| s.to_string()).collect();
            assert_eq!(panel.group_gradient(1), Some(&expected[..]));
        }
        assert!(!original(8).expect("theme").style.panels[0].legend.visible);
    }

    #[test]
    fn side_by_side_is_two_equal_layers_in_one_row() {
        let theme = original(7).expect("theme");
        assert_eq!(theme.style.panels.len(), 2);
        assert_eq!(theme.style.rows(), 1);
        assert_eq!(theme.style.columns(), 2);
        let widths = theme.style.sanitized_frame_widths().expect("stored");
        assert_eq!(widths[0], widths[1]);
    }
}
