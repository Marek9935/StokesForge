//! Themes: set the global colors of a graph in one gesture (issue #11, #550).
//!
//! The global text color and axes color are **graph-level fields** of the
//! [`PlotSpec`] (`text_color`, `axes_color`). Setting one writes only that
//! field, never a local one: a part with its own color keeps it, and the parts
//! left on automatic follow the global color. Rendering resolves each part:
//! its own color if set, else the graph-level color if set and readable, else
//! the automatic ink (`stokesforge_render::color::auto_ink`, white on a dark
//! background). An empty color is "automatic" everywhere.
//!
//! Light and Dark are the exception by design (#550): they put every part back
//! to automatic (local colors cleared) so a theme always gives a readable
//! graph; Ctrl+Z restores.

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
    /// Global text color as the General tab shows it: the graph-level field.
    /// It is not the graph title color: the General tab has its own « Title
    /// color » row.
    pub fn theme_text_color(&self) -> &str {
        &self.text_color
    }

    /// Global axes color as the General tab shows it: the graph-level field.
    pub fn theme_axes_color(&self) -> &str {
        &self.axes_color
    }

    /// Sets the global text color (graph-level field only, #550): the graph
    /// title, axis titles and free texts left on automatic follow it; a part
    /// with its own color keeps it.
    pub fn set_text_color(&mut self, color: &str) {
        self.text_color = color.to_string();
    }

    /// Sets the global axes color (graph-level field only, #550): the scale
    /// (line, ticks, labels, grid) of every axis left on automatic follows it.
    pub fn set_axes_color(&mut self, color: &str) {
        self.axes_color = color.to_string();
    }

    /// Empties the own text color of every part that follows the graph text
    /// color: title of each layer, title of each axis, legend text, free texts.
    /// The graph title keeps its own « Title color ».
    fn clear_part_text_colors(&mut self) {
        for panel in self.panels.iter_mut() {
            panel.title_color.clear();
            panel.legend.color.clear();
            for side in SIDES {
                panel.axis_mut(side).title_color.clear();
            }
            for annotation in &mut panel.annotations {
                annotation.color.clear();
            }
        }
    }

    /// Empties the own scale color of every axis of every layer.
    fn clear_part_axes_colors(&mut self) {
        for panel in self.panels.iter_mut() {
            for side in SIDES {
                panel.axis_mut(side).tick_color.clear();
            }
        }
    }

    /// Puts the family and the size of a text back to Auto (the box width and
    /// the alignment stay).
    fn clear_font(font: &mut crate::TextFont) {
        font.family.clear();
        font.size = None;
    }

    /// Empties the own font family and size of every text that follows the
    /// graph font and the text scale (#545): the graph title, each layer title,
    /// axis title, tick label size, legend text and free text. A free text
    /// follows the graph size again.
    fn clear_part_fonts_and_sizes(&mut self) {
        Self::clear_font(&mut self.title_font);
        for panel in self.panels.iter_mut() {
            Self::clear_font(&mut panel.title_font);
            Self::clear_font(&mut panel.legend.font);
            for side in SIDES {
                let axis = panel.axis_mut(side);
                Self::clear_font(&mut axis.title_font);
                axis.tick_label_size = None;
            }
            for annotation in &mut panel.annotations {
                annotation.font.clear();
                annotation.follows_graph_size = true;
            }
        }
    }

    /// « Reset Parts to Graph Settings » (#550, #545): empties the own text and
    /// axes colors of every part and puts every hand-set font family and size
    /// back to Auto, so they follow the graph again. Never touches the graph
    /// settings themselves (Font, Text scale, Text color, Axes color), the
    /// series, the graph title's own color, the backgrounds, the legend frame
    /// or the tick mark colors.
    pub fn reset_parts_to_graph(&mut self) {
        self.clear_part_text_colors();
        self.clear_part_axes_colors();
        self.clear_part_fonts_and_sizes();
    }

    /// How many parts have their own font family or size (the graph title
    /// included, a part counted once). Same field set as
    /// [`PlotSpec::reset_parts_to_graph`].
    pub fn parts_with_own_font_or_size(&self) -> usize {
        let own =
            |font: &crate::TextFont| usize::from(!font.family.is_empty() || font.size.is_some());
        own(&self.title_font)
            + self
                .panels
                .iter()
                .map(|panel| {
                    own(&panel.title_font)
                        + own(&panel.legend.font)
                        + SIDES
                            .iter()
                            .map(|&side| {
                                let axis = panel.axis(side);
                                own(&axis.title_font) + usize::from(axis.tick_label_size.is_some())
                            })
                            .sum::<usize>()
                        + panel
                            .annotations
                            .iter()
                            .map(|a| usize::from(!a.font.is_empty() || !a.follows_graph_size))
                            .sum::<usize>()
                })
                .sum::<usize>()
    }

    /// How many parts have their own text color (the graph title, which has
    /// its own row, is not counted). Same field set as
    /// [`PlotSpec::reset_parts_to_graph`].
    pub fn parts_with_own_text_color(&self) -> usize {
        let own = |color: &str| usize::from(!color.is_empty());
        self.panels
            .iter()
            .map(|panel| {
                own(&panel.title_color)
                    + own(&panel.legend.color)
                    + SIDES
                        .iter()
                        .map(|&side| own(&panel.axis(side).title_color))
                        .sum::<usize>()
                    + panel
                        .annotations
                        .iter()
                        .map(|annotation| own(&annotation.color))
                        .sum::<usize>()
            })
            .sum()
    }

    /// How many axes have their own scale color.
    pub fn parts_with_own_axes_color(&self) -> usize {
        self.panels
            .iter()
            .map(|panel| {
                SIDES
                    .iter()
                    .filter(|&&side| !panel.axis(side).tick_color.is_empty())
                    .count()
            })
            .sum()
    }

    /// The Light and Dark rule (#550, decision G2): a colour-only theme puts
    /// every part back to automatic, so a theme always gives a readable graph
    /// (the graph title's own color included).
    pub(crate) fn clear_local_text_colors(&mut self) {
        self.clear_part_text_colors();
        self.title_color.clear();
    }

    /// See [`PlotSpec::clear_local_text_colors`], for the scales.
    pub(crate) fn clear_local_axes_colors(&mut self) {
        self.clear_part_axes_colors();
    }

    /// Applies a predefined theme in full.
    pub fn apply_theme(&mut self, theme: &Theme) {
        self.background = theme.background.to_string();
        self.set_text_color(theme.text);
        self.set_axes_color(theme.axes);
        self.clear_local_text_colors();
        self.clear_local_axes_colors();
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
    fn the_title_color_alone_does_not_move_the_global_text_color() {
        let mut spec = two_panel_spec();
        spec.set_text_color("#123456");
        spec.title_color = "#ff0000".to_string();
        assert_eq!(spec.theme_text_color(), "#123456");
    }

    #[test]
    fn a_global_color_leaves_the_local_colors_alone() {
        let mut spec = two_panel_spec();
        spec.panels.get_mut(0).unwrap().left.title_color = "#ff0000".to_string();
        spec.panels.get_mut(1).unwrap().bottom.tick_color = "#00ff00".to_string();
        spec.set_text_color("#123456");
        spec.set_axes_color("#654321");
        assert_eq!(spec.text_color, "#123456");
        assert_eq!(spec.axes_color, "#654321");
        assert_eq!(spec.theme_text_color(), "#123456");
        assert_eq!(spec.theme_axes_color(), "#654321");
        assert_eq!(spec.panels.as_slice()[0].left.title_color, "#ff0000");
        assert_eq!(spec.panels.as_slice()[1].bottom.tick_color, "#00ff00");
        assert_eq!(spec.title_color, "");
        for (i, panel) in spec.panels.as_slice().iter().enumerate() {
            for side in SIDES {
                let title = if (i, side) == (0, AxisSide::Left) {
                    "#ff0000"
                } else {
                    ""
                };
                let tick = if (i, side) == (1, AxisSide::Bottom) {
                    "#00ff00"
                } else {
                    ""
                };
                assert_eq!(panel.axis(side).title_color, title);
                assert_eq!(panel.axis(side).tick_color, tick);
            }
        }
    }

    #[test]
    fn a_theme_sets_background_text_and_axes() {
        let mut spec = two_panel_spec();
        spec.set_text_color("#ff0000");
        spec.panels.get_mut(1).unwrap().right.title_color = "#00ff00".to_string();
        spec.apply_theme(&THEMES[1]);
        assert_eq!(spec.background, "#1e1e1e");
        assert_eq!(spec.theme_text_color(), "", "automatic text");
        assert_eq!(spec.panels.as_slice()[1].right.title_color, "");
        spec.apply_theme(&THEMES[0]);
        assert_eq!(spec.background, "#ffffff");
    }

    fn spec_with_own_colors() -> PlotSpec {
        let mut spec = two_panel_spec();
        spec.title_color = "#010101".into();
        spec.background = "#fefefe".into();
        spec.text_color = "#123456".into();
        let first = spec.panels.get_mut(0).unwrap();
        first.title_color = "#ff0000".into();
        first.legend.color = "#ff0001".into();
        first.legend.frame_color = "#ff0002".into();
        first.left.title_color = "#ff0003".into();
        first.left.tick_color = "#00ff00".into();
        first.bottom.tick_color = "#00ff01".into();
        first.bottom.tick_mark_color = "#00ff02".into();
        first
            .series
            .push(crate::SeriesSpec::new(stokesforge_id::NodeId::from_raw(1)));
        first.series[0].line.color = "#0000ff".into();
        first
            .annotations
            .push(crate::AnnotationSpec::new("Note", 0.0, 0.0));
        first.annotations[0].color = "#ff0004".into();
        spec
    }

    #[test]
    fn dark_clears_an_own_axis_title_color() {
        let mut spec = spec_with_own_colors();
        spec.apply_theme(&THEMES[1]);
        assert_eq!(spec.panels.as_slice()[0].left.title_color, "");
        assert_eq!(spec.panels.as_slice()[0].title_color, "");
        assert_eq!(spec.panels.as_slice()[0].legend.color, "");
        assert_eq!(spec.title_color, "");
    }

    #[test]
    fn resetting_the_parts_touches_only_text_and_axes_colors_of_parts() {
        let mut spec = spec_with_own_colors();
        assert_eq!(spec.parts_with_own_text_color(), 4);
        assert_eq!(spec.parts_with_own_axes_color(), 2);
        spec.reset_parts_to_graph();
        assert_eq!(spec.parts_with_own_text_color(), 0);
        assert_eq!(spec.parts_with_own_axes_color(), 0);
        let mut expected = spec_with_own_colors();
        let first = expected.panels.get_mut(0).unwrap();
        first.title_color.clear();
        first.legend.color.clear();
        first.left.title_color.clear();
        first.left.tick_color.clear();
        first.bottom.tick_color.clear();
        first.annotations[0].color.clear();
        assert_eq!(
            spec, expected,
            "series, title color, backgrounds, frame, marks and graph colors survive"
        );
    }

    /// A graph with a hand-set font or size on every kind of part, and graph
    /// settings and a series colour that the reset must keep (#545).
    fn spec_with_own_fonts_and_sizes() -> PlotSpec {
        let mut spec = spec_with_own_colors();
        spec.text_font = "Fira Sans".into();
        spec.text_scale = Some(1.5);
        spec.title_font.size = Some(30.0);
        let first = spec.panels.get_mut(0).unwrap();
        first.title_font.family = "Libertinus Sans".into();
        first.left.title_font.size = Some(22.0);
        first.bottom.tick_label_size = Some(9.0);
        first.legend.font.family = "Libertinus Serif".into();
        first.annotations[0].font = "Libertinus Mono".into();
        first.annotations[0].size = 40.0;
        first.annotations[0].follows_graph_size = false;
        spec
    }

    #[test]
    fn resetting_the_parts_puts_hand_set_fonts_and_sizes_back_to_auto() {
        let mut spec = spec_with_own_fonts_and_sizes();
        assert_eq!(spec.parts_with_own_font_or_size(), 6);
        spec.reset_parts_to_graph();
        assert_eq!(spec.parts_with_own_font_or_size(), 0);
        let first = &spec.panels.as_slice()[0];
        assert_eq!(first.left.title_font.size, None);
        assert_eq!(first.bottom.tick_label_size, None);
        assert_eq!(first.legend.font.family, "");
        assert!(first.annotations[0].follows_graph_size);
        assert_eq!(first.annotations[0].font, "");
        assert_eq!(spec.title_font.size, None);
        assert_eq!(first.title_font.family, "");
        // The graph settings and the curves stay.
        assert_eq!(spec.text_font, "Fira Sans");
        assert_eq!(spec.text_scale, Some(1.5));
        assert_eq!(spec.text_color, "#123456");
        assert_eq!(first.series[0].line.color, "#0000ff");
    }

    #[test]
    fn a_hand_set_size_alone_counts_as_a_part_of_its_own() {
        let mut spec = two_panel_spec();
        assert_eq!(spec.parts_with_own_font_or_size(), 0);
        spec.panels.get_mut(1).unwrap().bottom.tick_label_size = Some(12.0);
        assert_eq!(spec.parts_with_own_font_or_size(), 1);
        assert_eq!(
            (
                spec.parts_with_own_text_color(),
                spec.parts_with_own_axes_color()
            ),
            (0, 0),
            "no own colour"
        );
    }
}
