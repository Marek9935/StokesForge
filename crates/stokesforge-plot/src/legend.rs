//! Legend of a panel.

use crate::text_anchor::TextAnchor;

/// Corner of the panel where the legend is anchored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LegendPosition {
    /// Top left.
    TopLeft,
    /// Top right (default).
    #[default]
    TopRight,
    /// Bottom left.
    BottomLeft,
    /// Bottom right.
    BottomRight,
}

/// Legend of a panel: visible or not, and where.
#[derive(Debug, Clone, PartialEq)]
pub struct LegendSpec {
    /// Legend is displayed.
    pub visible: bool,
    /// Anchor corner.
    pub position: LegendPosition,
    /// Position of the frame (its center) if it was moved (issue #53);
    /// takes precedence over [`Self::position`]. `None` = the anchor corner.
    pub anchor: Option<TextAnchor>,
    /// Font and size of the series names in the legend (issue #77).
    pub font: crate::TextFont,
    /// Color of the series names in the legend (issue #90); empty =
    /// automatic, like [`crate::PlotSpec::title_color`].
    pub color: String,
}

impl Default for LegendSpec {
    fn default() -> Self {
        Self {
            visible: true,
            position: LegendPosition::TopRight,
            anchor: None,
            font: crate::TextFont::default(),
            color: String::new(),
        }
    }
}
