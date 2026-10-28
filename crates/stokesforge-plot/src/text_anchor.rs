//! The movable texts of a graph — title, panel titles, axis titles,
//! legend (issue #53).
//!
//! A title without a [`TextAnchor`] is **where rendering puts it**: Plotters
//! reserves its place (title band, tick area, corner of the plot
//! area for the legend). With an anchor, it is **anchored to the data** like
//! a free text ([`crate::AnnotationSpec`]): its center follows the zoom and
//! pan of the view, and the position holds on export (rendering, hence both
//! backends, reads it). Resetting means removing the anchor.
//!
//! The anchor is expressed in the data frame of the text's **panel** —
//! the first panel for the graph title, which belongs to no
//! panel. A title generally lives **outside** the plot area: the anchor
//! is therefore not bounded to the visible framing — no more than a free text
//! (#163): both stay plotted in the margins.

use crate::axis::AxisSide;
use crate::series::SeriesAxis;
use crate::spec::PlotSpec;

/// Where a moved text is placed: its **center**, in data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextAnchor {
    /// Abscissa of the center, in data.
    pub x: f64,
    /// Ordinate of the center, in data of the axis [`Self::axis`].
    pub y: f64,
    /// Ordinate it is attached to: left or right, like a free text.
    pub axis: SeriesAxis,
}

impl TextAnchor {
    /// An anchor at the point `(x, y)` of the left ordinate.
    pub const fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            axis: SeriesAxis::Left,
        }
    }

    /// True if the anchor is usable: finite coordinates. An anchor coming
    /// from a damaged file is ignored at rendering (the text gets its default
    /// place back) rather than projecting a `NaN`.
    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// A movable text, designated in a [`PlotSpec`]. Layers are
/// addressed by their index, like [`crate::AnnotationRef`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextTarget {
    /// The graph title.
    Title,
    /// The title of a panel.
    PanelTitle {
        /// Index of the layer in [`PlotSpec::panels`].
        panel: usize,
    },
    /// The title of an axis of a panel.
    AxisTitle {
        /// Index of the layer in [`PlotSpec::panels`].
        panel: usize,
        /// The side of the axis.
        side: AxisSide,
    },
    /// The legend of a panel (the whole frame).
    Legend {
        /// Index of the layer in [`PlotSpec::panels`].
        panel: usize,
    },
}

impl TextTarget {
    /// The index of the text's layer; `None` for the graph title.
    pub fn panel_index_of(&self) -> Option<usize> {
        match *self {
            Self::Title => None,
            Self::PanelTitle { panel } | Self::AxisTitle { panel, .. } | Self::Legend { panel } => {
                Some(panel)
            }
        }
    }

    fn panel_index(&self, spec: &PlotSpec) -> Option<usize> {
        self.panel_index_of()
            .filter(|panel| *panel < spec.panels.len())
    }

    /// Remembered visibility of the text, without erasing its content or its anchor.
    pub fn visible(&self, spec: &PlotSpec) -> Option<bool> {
        Some(match *self {
            Self::Title => spec.title_visible,
            Self::PanelTitle { .. } => spec.panels.get(self.panel_index(spec)?)?.title_visible,
            Self::AxisTitle { side, .. } => {
                spec.panels
                    .get(self.panel_index(spec)?)?
                    .axis(side)
                    .title_visible
            }
            Self::Legend { .. } => spec.panels.get(self.panel_index(spec)?)?.legend.visible,
        })
    }

    /// Only changes the visibility; an absent target has no effect.
    pub fn set_visible(&self, spec: &mut PlotSpec, visible: bool) -> bool {
        if self.visible(spec).is_none_or(|value| value == visible) {
            return false;
        }
        if *self == Self::Title {
            spec.title_visible = visible;
            return true;
        }
        let Some(index) = self.panel_index(spec) else {
            return false;
        };
        let Some(panel) = spec.panels.get_mut(index) else {
            return false;
        };
        match *self {
            Self::Title => unreachable!(),
            Self::PanelTitle { .. } => panel.title_visible = visible,
            Self::AxisTitle { side, .. } => panel.axis_mut(side).title_visible = visible,
            Self::Legend { .. } => panel.legend.visible = visible,
        }
        true
    }

    /// The anchor of the text, if it was moved.
    pub fn anchor(&self, spec: &PlotSpec) -> Option<TextAnchor> {
        match *self {
            Self::Title => spec.title_anchor,
            Self::PanelTitle { .. } => spec.panels.get(self.panel_index(spec)?)?.title_anchor,
            Self::AxisTitle { side, .. } => {
                spec.panels
                    .get(self.panel_index(spec)?)?
                    .axis(side)
                    .title_anchor
            }
            Self::Legend { .. } => spec.panels.get(self.panel_index(spec)?)?.legend.anchor,
        }
    }

    /// Sets (or, with `None`, removes) the anchor of the text. Returns `false` without
    /// changing anything if the panel does not exist.
    pub fn set_anchor(&self, spec: &mut PlotSpec, anchor: Option<TextAnchor>) -> bool {
        match *self {
            Self::Title => {
                spec.title_anchor = anchor;
                true
            }
            Self::PanelTitle { .. } => {
                let Some(panel) = self
                    .panel_index(spec)
                    .and_then(|index| spec.panels.get_mut(index))
                else {
                    return false;
                };
                panel.title_anchor = anchor;
                true
            }
            Self::AxisTitle { side, .. } => {
                let Some(panel) = self
                    .panel_index(spec)
                    .and_then(|index| spec.panels.get_mut(index))
                else {
                    return false;
                };
                panel.axis_mut(side).title_anchor = anchor;
                true
            }
            Self::Legend { .. } => {
                let Some(panel) = self
                    .panel_index(spec)
                    .and_then(|index| spec.panels.get_mut(index))
                else {
                    return false;
                };
                panel.legend.anchor = anchor;
                true
            }
        }
    }

    /// The font and size of the text (issue #78), `None` if the panel
    /// does not exist.
    pub fn font(&self, spec: &PlotSpec) -> Option<crate::TextFont> {
        Some(match *self {
            Self::Title => spec.title_font.clone(),
            Self::PanelTitle { .. } => spec.panels.get(self.panel_index(spec)?)?.title_font.clone(),
            Self::AxisTitle { side, .. } => spec
                .panels
                .get(self.panel_index(spec)?)?
                .axis(side)
                .title_font
                .clone(),
            Self::Legend { .. } => spec
                .panels
                .get(self.panel_index(spec)?)?
                .legend
                .font
                .clone(),
        })
    }

    /// Sets the font and size of the text. Returns `false` without changing anything if
    /// the panel does not exist.
    pub fn set_font(&self, spec: &mut PlotSpec, font: crate::TextFont) -> bool {
        if let Self::Title = self {
            spec.title_font = font;
            return true;
        }
        let Some(index) = self.panel_index(spec) else {
            return false;
        };
        let Some(panel) = spec.panels.get_mut(index) else {
            return false;
        };
        match *self {
            Self::Title => unreachable!(),
            Self::PanelTitle { .. } => panel.title_font = font,
            Self::AxisTitle { side, .. } => panel.axis_mut(side).title_font = font,
            Self::Legend { .. } => panel.legend.font = font,
        }
        true
    }

    /// The color of the whole area (issue #90), empty = automatic; `None`
    /// if the panel does not exist.
    pub fn color(&self, spec: &PlotSpec) -> Option<String> {
        Some(match *self {
            Self::Title => spec.title_color.clone(),
            Self::PanelTitle { .. } => spec
                .panels
                .get(self.panel_index(spec)?)?
                .title_color
                .clone(),
            Self::AxisTitle { side, .. } => spec
                .panels
                .get(self.panel_index(spec)?)?
                .axis(side)
                .title_color
                .clone(),
            Self::Legend { .. } => spec
                .panels
                .get(self.panel_index(spec)?)?
                .legend
                .color
                .clone(),
        })
    }

    /// Sets the color of the whole area. Returns `false` without changing anything if the
    /// panel does not exist.
    pub fn set_color(&self, spec: &mut PlotSpec, color: String) -> bool {
        if let Self::Title = self {
            spec.title_color = color;
            return true;
        }
        let Some(index) = self.panel_index(spec) else {
            return false;
        };
        let Some(panel) = spec.panels.get_mut(index) else {
            return false;
        };
        match *self {
            Self::Title => unreachable!(),
            Self::PanelTitle { .. } => panel.title_color = color,
            Self::AxisTitle { side, .. } => panel.axis_mut(side).title_color = color,
            Self::Legend { .. } => panel.legend.color = color,
        }
        true
    }

    /// The title text as displayed — `None` for the legend (its
    /// texts are the series names) or if the panel does not exist. For an
    /// axis title, the unit is included (`Signal (a.u.)`,
    /// [`crate::AxisSpec::display_label`]).
    pub fn text(&self, spec: &PlotSpec) -> Option<String> {
        match *self {
            Self::Title => Some(spec.title.clone()),
            Self::PanelTitle { .. } => {
                Some(spec.panels.get(self.panel_index(spec)?)?.title.clone())
            }
            Self::AxisTitle { side, .. } => Some(
                spec.panels
                    .get(self.panel_index(spec)?)?
                    .axis(side)
                    .display_label(),
            ),
            Self::Legend { .. } => None,
        }
    }

    /// Replaces the title text. Returns `false` without changing anything for the
    /// legend or an absent panel. For an axis title, `text` is the **displayed**
    /// text ([`Self::text`]): if it still ends with the unit in
    /// parentheses, that stays the unit of the axis and the rest becomes the title;
    /// otherwise the unit is cleared (the typed text is all that is displayed).
    pub fn set_text(&self, spec: &mut PlotSpec, text: String) -> bool {
        match *self {
            Self::Title => {
                spec.title = text;
                true
            }
            Self::PanelTitle { .. } => {
                let Some(panel) = self
                    .panel_index(spec)
                    .and_then(|index| spec.panels.get_mut(index))
                else {
                    return false;
                };
                panel.title = text;
                true
            }
            Self::AxisTitle { side, .. } => {
                let Some(panel) = self
                    .panel_index(spec)
                    .and_then(|index| spec.panels.get_mut(index))
                else {
                    return false;
                };
                let axis = panel.axis_mut(side);
                let suffix = format!(" ({})", axis.unit);
                if axis.unit.is_empty() {
                    axis.label = text;
                } else if let Some(label) = text.strip_suffix(suffix.as_str()) {
                    axis.label = label.to_owned();
                } else if text == format!("({})", axis.unit) {
                    axis.label.clear();
                } else {
                    axis.label = text;
                    axis.unit.clear();
                }
                true
            }
            Self::Legend { .. } => false,
        }
    }
}

impl PlotSpec {
    /// Removes all the anchors: each title and each legend gets its default
    /// place back. Returns the number of anchors removed.
    pub fn reset_text_anchors(&mut self) -> usize {
        let mut removed = usize::from(self.title_anchor.take().is_some());
        for panel in self.panels.iter_mut() {
            removed += usize::from(panel.title_anchor.take().is_some());
            removed += usize::from(panel.legend.anchor.take().is_some());
            for side in [
                AxisSide::Bottom,
                AxisSide::Left,
                AxisSide::Right,
                AxisSide::Top,
            ] {
                removed += usize::from(panel.axis_mut(side).title_anchor.take().is_some());
            }
        }
        removed
    }

    /// True if at least one text was moved.
    pub fn has_text_anchors(&self) -> bool {
        self.title_anchor.is_some()
            || self.panels.iter().any(|panel| {
                panel.title_anchor.is_some()
                    || panel.legend.anchor.is_some()
                    || [
                        AxisSide::Bottom,
                        AxisSide::Left,
                        AxisSide::Right,
                        AxisSide::Top,
                    ]
                    .into_iter()
                    .any(|side| panel.axis(side).title_anchor.is_some())
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_anchor_is_set_read_and_removed_for_each_text() {
        let mut spec = PlotSpec::empty("Title");
        let targets = [
            TextTarget::Title,
            TextTarget::PanelTitle { panel: 0 },
            TextTarget::AxisTitle {
                panel: 0,
                side: AxisSide::Left,
            },
            TextTarget::Legend { panel: 0 },
        ];
        for target in targets {
            assert_eq!(target.anchor(&spec), None);
            assert!(target.set_anchor(&mut spec, Some(TextAnchor::new(1.0, 2.0))));
            assert_eq!(target.anchor(&spec), Some(TextAnchor::new(1.0, 2.0)));
        }
        assert!(spec.has_text_anchors());
        assert_eq!(spec.reset_text_anchors(), 4);
        assert!(!spec.has_text_anchors());
        assert_eq!(spec.reset_text_anchors(), 0);
    }

    #[test]
    fn an_absent_panel_changes_nothing() {
        let mut spec = PlotSpec::empty("T");
        let far = TextTarget::Legend { panel: 3 };
        assert!(!far.set_anchor(&mut spec, Some(TextAnchor::new(0.0, 0.0))));
        assert_eq!(far.anchor(&spec), None);
        assert!(!spec.has_text_anchors());
    }

    #[test]
    fn the_text_of_a_title_is_read_and_replaced() {
        let mut spec = PlotSpec::empty("A");
        let axis = TextTarget::AxisTitle {
            panel: 0,
            side: AxisSide::Bottom,
        };
        assert_eq!(TextTarget::Title.text(&spec).as_deref(), Some("A"));
        assert!(TextTarget::Title.set_text(&mut spec, "B".into()));
        assert!(axis.set_text(&mut spec, "t".into()));
        assert_eq!(spec.title, "B");
        assert_eq!(spec.panels[0].bottom.label, "t");
        // The unit is displayed with the title: keep it or clear it on edit.
        spec.panels.get_mut(0).expect("panel").bottom.unit = "s".into();
        assert_eq!(axis.text(&spec).as_deref(), Some("t (s)"));
        assert!(axis.set_text(&mut spec, "time (s)".into()));
        assert_eq!(spec.panels[0].bottom.label, "time");
        assert_eq!(spec.panels[0].bottom.unit, "s");
        assert!(axis.set_text(&mut spec, "duration".into()));
        assert_eq!(spec.panels[0].bottom.label, "duration");
        assert_eq!(spec.panels[0].bottom.unit, "");
        let legend = TextTarget::Legend { panel: 0 };
        assert_eq!(legend.text(&spec), None);
        assert!(!legend.set_text(&mut spec, "x".into()));
    }

    #[test]
    fn a_non_finite_anchor_is_not_usable() {
        assert!(TextAnchor::new(1.0, 2.0).is_finite());
        assert!(!TextAnchor::new(f64::NAN, 2.0).is_finite());
    }
}
