//! Themes: set the global colors of a graph in one gesture (issue #11).
//!
//! A theme is **not** a field of the [`PlotSpec`]: applying a theme or a
//! global color writes the same color into all the local fields
//! (graph title, title and scale of each axis of each panel, background).
//! The last setting therefore overwrites what was set one by one, and nothing
//! is remembered beyond the fields themselves — no change to the `.stf`
//! format. An empty color stays "automatic": rendering resolves it with
//! `stokesforge_render::color::auto_ink` (white on a dark background).

use crate::{AxisSide, PlotSpec};

/// A predefined theme: a background, a text color, an axes color.
/// An empty string means "automatic" (`""`) for the text and the
/// axes, and "white" for the background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Stable identifier (`light`, `dark`). Not shown: the interface names
    /// themes through its catalog (`plot-theme-*` keys).
    pub id: &'static str,
    /// Background of the graph (`#RRGGBB[AA]`).
    pub background: &'static str,
    /// Text color: titles of the graph and of the axes.
    pub text: &'static str,
    /// Axes color: line, ticks, labels and grid.
    pub axes: &'static str,
}

/// The predefined themes. Text and axes stay automatic: they follow
/// the background (dark ink on a light background, white on a dark one).
pub const THEMES: [Theme; 2] = [
    Theme {
        id: "light",
        background: "#ffffff",
        text: "",
        axes: "",
    },
    Theme {
        id: "dark",
        background: "#1e1e1e",
        text: "",
        axes: "",
    },
];

const SIDES: [AxisSide; 4] = [
    AxisSide::Bottom,
    AxisSide::Left,
    AxisSide::Right,
    AxisSide::Top,
];

impl PlotSpec {
    /// Global text color as the « Thème » tab shows it: that
    /// of the graph title.
    pub fn theme_text_color(&self) -> &str {
        &self.title_color
    }

    /// Global axes color as the « Thème » tab shows it: that
    /// of the scale of the bottom axis of the first panel.
    pub fn theme_axes_color(&self) -> &str {
        self.panels
            .first()
            .map_or("", |panel| panel.bottom.tick_color.as_str())
    }

    /// Writes `color` as the text color: graph title, title of
    /// each axis and free texts (issue #35) of each panel.
    pub fn set_text_color(&mut self, color: &str) {
        self.title_color = color.to_string();
        for panel in self.panels.iter_mut() {
            for side in SIDES {
                panel.axis_mut(side).title_color = color.to_string();
            }
            for annotation in &mut panel.annotations {
                annotation.color = color.to_string();
            }
        }
    }

    /// Writes `color` as the axes color: scale (line, ticks,
    /// labels, grid) of each axis of each panel.
    pub fn set_axes_color(&mut self, color: &str) {
        for panel in self.panels.iter_mut() {
            for side in SIDES {
                panel.axis_mut(side).tick_color = color.to_string();
            }
        }
    }

    /// Applies a predefined theme in full.
    pub fn apply_theme(&mut self, theme: &Theme) {
        self.background = theme.background.to_string();
        self.set_text_color(theme.text);
        self.set_axes_color(theme.axes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PanelSpec;

    fn two_panel_spec() -> PlotSpec {
        let mut spec = PlotSpec::empty("Figure").with_panels(PanelSpec::at(0, 0));
        spec.panels.push(PanelSpec::at(0, 1));
        spec
    }

    #[test]
    fn a_global_color_overwrites_local_settings_everywhere() {
        let mut spec = two_panel_spec();
        spec.panels.get_mut(1).unwrap().left.title_color = "#ff0000".to_string();
        spec.panels.get_mut(0).unwrap().bottom.tick_color = "#00ff00".to_string();
        spec.set_text_color("#123456");
        spec.set_axes_color("#654321");
        assert_eq!(spec.title_color, "#123456");
        assert_eq!(spec.theme_text_color(), "#123456");
        assert_eq!(spec.theme_axes_color(), "#654321");
        for panel in spec.panels.as_slice() {
            for side in SIDES {
                assert_eq!(panel.axis(side).title_color, "#123456");
                assert_eq!(panel.axis(side).tick_color, "#654321");
            }
        }
    }

    #[test]
    fn a_theme_sets_background_text_and_axes() {
        let mut spec = two_panel_spec();
        spec.set_text_color("#ff0000");
        spec.apply_theme(&THEMES[1]);
        assert_eq!(spec.background, "#1e1e1e");
        assert_eq!(spec.theme_text_color(), "", "automatic text");
        assert_eq!(spec.panels.as_slice()[1].right.title_color, "");
        spec.apply_theme(&THEMES[0]);
        assert_eq!(spec.background, "#ffffff");
    }
}
