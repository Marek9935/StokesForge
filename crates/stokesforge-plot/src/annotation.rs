//! The free texts of a panel — Origin's Text tool (issue #35).

use crate::series::SeriesAxis;
use crate::spec::PlotSpec;

/// A text placed on a panel, **anchored to the data**: it follows the zoom and
/// pan of the view like a series point.
///
/// The text is Typst markup, like a title (`$lambda$`, `*gras*`); its
/// **center** is placed at the point `(x, y)`, in data coordinates of the
/// panel (decision of 2026-10-01). No frame or background in this
/// first version.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotationSpec {
    /// The text, in Typst markup.
    pub text: String,
    /// Visible at rendering; hiding keeps the content and its address (#81).
    pub visible: bool,
    /// Abscissa of the center of the text, in data.
    pub x: f64,
    /// Ordinate of the center of the text, in data of the axis [`Self::axis`].
    pub y: f64,
    /// Ordinate it is attached to: left or right, like a series.
    pub axis: SeriesAxis,
    /// `#RRGGBB` or `#RRGGBBAA` color; empty = automatic (contrast with
    /// the background).
    pub color: String,
    /// Text size in the Plotters sense (like the pipeline sizes:
    /// 16 = axis title).
    pub size: f64,
    /// Rotation in degrees, counter-clockwise (trigonometric), around the
    /// center of the text.
    pub rotation: f64,
    /// Font family name (issue #78); empty = the graph's font.
    pub font: String,
    /// Width of the box, as a **fraction of the figure** (#244); `None` =
    /// no width: the text stays on one line, as before. When set, it
    /// wraps the text; see [`Self::effective_width`].
    pub width: Option<f64>,
    /// Alignment of the text in the box; no effect as long as
    /// [`Self::width`] is `None`.
    pub align: TextAlign,
}

/// The alignment of a text in its box (#244).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextAlign {
    /// At the left edge.
    Left,
    /// Centered.
    #[default]
    Center,
    /// At the right edge.
    Right,
}

impl AnnotationSpec {
    /// Default size: that of the axis titles.
    pub const DEFAULT_SIZE: f64 = 16.0;
    /// Minimum and maximum size accepted at rendering; beyond that, the size is
    /// clamped to these bounds rather than rasterizing a huge texture.
    pub const SIZE_RANGE: (f64, f64) = (4.0, 200.0);

    /// A text at the point `(x, y)` of the left ordinate, in the default
    /// style.
    pub fn new(text: impl Into<String>, x: f64, y: f64) -> Self {
        Self {
            text: text.into(),
            visible: true,
            x,
            y,
            axis: SeriesAxis::Left,
            color: String::new(),
            size: Self::DEFAULT_SIZE,
            rotation: 0.0,
            font: String::new(),
            width: None,
            align: TextAlign::Center,
        }
    }

    /// Smallest box width, as a fraction of the figure.
    pub const MIN_WIDTH: f64 = 0.02;

    /// The plotted width: [`Self::width`] clamped to `[MIN_WIDTH, 1]` (the
    /// cap is the figure), `None` for an absent or non-finite value.
    pub fn effective_width(&self) -> Option<f64> {
        self.width
            .filter(|width| width.is_finite())
            .map(|width| width.clamp(Self::MIN_WIDTH, 1.0))
    }

    /// The size actually plotted: [`Self::size`] clamped to
    /// [`Self::SIZE_RANGE`], and the default for a non-finite value.
    pub fn effective_size(&self) -> f64 {
        if self.size.is_finite() {
            self.size.clamp(Self::SIZE_RANGE.0, Self::SIZE_RANGE.1)
        } else {
            Self::DEFAULT_SIZE
        }
    }

    /// The rotation actually plotted, brought into `]-180, 180]`; 0 for
    /// a non-finite value.
    pub fn effective_rotation(&self) -> f64 {
        if !self.rotation.is_finite() {
            return 0.0;
        }
        let turned = self.rotation.rem_euclid(360.0);
        if turned > 180.0 {
            turned - 360.0
        } else {
            turned
        }
    }
}

/// The address of a free text in a [`PlotSpec`]: its layer, by its
/// index in [`PlotSpec::panels`] (several layers can overlap
/// in the same cell of the grid), and its rank in
/// [`crate::PanelSpec::annotations`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnnotationRef {
    /// Index of the layer in [`PlotSpec::panels`].
    pub panel: usize,
    /// Rank of the text in its panel.
    pub index: usize,
}

impl AnnotationRef {
    fn panel<'a>(&self, spec: &'a PlotSpec) -> Option<&'a crate::PanelSpec> {
        spec.panels.get(self.panel)
    }

    fn panel_index(&self, spec: &PlotSpec) -> Option<usize> {
        (self.panel < spec.panels.len()).then_some(self.panel)
    }

    /// The designated text, if it exists.
    pub fn get<'a>(&self, spec: &'a PlotSpec) -> Option<&'a AnnotationSpec> {
        self.panel(spec)?.annotations.get(self.index)
    }

    /// Places `annotation` at this address: replaces the existing text, or
    /// adds it if `index` is the rank after the last. Returns `false` without
    /// changing anything if the panel does not exist or if `index` is further.
    pub fn set(&self, spec: &mut PlotSpec, annotation: AnnotationSpec) -> bool {
        let Some(panel) = self
            .panel_index(spec)
            .and_then(|index| spec.panels.get_mut(index))
        else {
            return false;
        };
        match self.index.cmp(&panel.annotations.len()) {
            std::cmp::Ordering::Less => panel.annotations[self.index] = annotation,
            std::cmp::Ordering::Equal => panel.annotations.push(annotation),
            std::cmp::Ordering::Greater => return false,
        }
        true
    }

    /// Removes the designated text; returns `false` if it does not exist.
    pub fn remove(&self, spec: &mut PlotSpec) -> bool {
        let Some(panel) = self
            .panel_index(spec)
            .and_then(|index| spec.panels.get_mut(index))
        else {
            return false;
        };
        if self.index >= panel.annotations.len() {
            return false;
        }
        panel.annotations.remove(self.index);
        true
    }

    /// The address of the next text added to the layer `panel`, or `None` if that
    /// layer does not exist.
    pub fn next_in(spec: &PlotSpec, panel: usize) -> Option<Self> {
        let source = spec.panels.get(panel)?;
        Some(Self {
            panel,
            index: source.annotations.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_adds_replaces_and_removes() {
        let mut spec = PlotSpec::empty("A");
        let new = AnnotationRef::next_in(&spec, 0).expect("panel 0,0");
        assert_eq!(new.index, 0);
        assert!(new.set(&mut spec, AnnotationSpec::new("a", 1.0, 2.0)));
        assert_eq!(new.get(&spec).map(|a| a.text.as_str()), Some("a"));
        assert!(new.set(&mut spec, AnnotationSpec::new("b", 1.0, 2.0)));
        assert_eq!(spec.panels[0].annotations.len(), 1, "replaced, not added");
        let far = AnnotationRef { index: 5, ..new };
        assert!(!far.set(&mut spec, AnnotationSpec::new("c", 0.0, 0.0)));
        assert!(AnnotationRef::next_in(&spec, 3).is_none());
        assert!(new.remove(&mut spec));
        assert!(!new.remove(&mut spec));
        assert!(spec.panels[0].annotations.is_empty());
    }

    #[test]
    fn size_and_rotation_are_clamped_to_their_bounds() {
        let mut a = AnnotationSpec::new("x", 0.0, 0.0);
        a.size = 1e9;
        assert_eq!(a.effective_size(), AnnotationSpec::SIZE_RANGE.1);
        a.size = f64::NAN;
        assert_eq!(a.effective_size(), AnnotationSpec::DEFAULT_SIZE);
        a.rotation = 270.0;
        assert_eq!(a.effective_rotation(), -90.0);
        a.rotation = -540.0;
        assert_eq!(a.effective_rotation(), 180.0);
        a.rotation = f64::INFINITY;
        assert_eq!(a.effective_rotation(), 0.0);
    }
}
