//! The font and size of a text of the graph (issues #77 and #78).
//!
//! A choice of font or size applies to the **whole** text area: it is
//! an attribute of the area, not a tag slipped into its content. The
//! content (Typst markup, [`crate::AnnotationSpec::text`]) no longer carries a
//! `#text(font: …)` set by the format bar; rendering composes the font
//! and its associated Math font at plot time (`stokesforge_render::formula`).

/// Font and size of a title, an axis title or the legend. The default
/// changes nothing: the graph's font and the pipeline's size.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextFont {
    /// Family name, embedded or system. Empty = the graph's font.
    pub family: String,
    /// Size in the Plotters sense (16 = axis title). `None` = the default
    /// size of this text.
    pub size: Option<f64>,
    /// Width of the box, as a fraction of the figure width (#244);
    /// `None` = one line. See [`crate::AnnotationSpec::width`].
    pub width: Option<f64>,
    /// Alignment in the box; no effect without a width.
    pub align: crate::TextAlign,
}

impl TextFont {
    /// Minimum and maximum size accepted at rendering, like
    /// [`crate::AnnotationSpec::SIZE_RANGE`].
    pub const SIZE_RANGE: (f64, f64) = (4.0, 200.0);

    /// True if nothing is set.
    pub fn is_default(&self) -> bool {
        self.family.trim().is_empty()
            && self.size.is_none()
            && self.width.is_none()
            && self.align == crate::TextAlign::Center
    }

    /// The plotted size: the setting clamped to [`Self::SIZE_RANGE`],
    /// `default` if it is missing or not finite.
    pub fn effective_size(&self, default: f64) -> f64 {
        match self.size {
            Some(size) if size.is_finite() => size.clamp(Self::SIZE_RANGE.0, Self::SIZE_RANGE.1),
            _ => default,
        }
    }

    /// The plotted width: [`Self::width`] clamped like that of a free text
    /// ([`crate::AnnotationSpec::effective_width`]).
    pub fn effective_width(&self) -> Option<f64> {
        self.width
            .filter(|width| width.is_finite())
            .map(|width| width.clamp(crate::AnnotationSpec::MIN_WIDTH, 1.0))
    }

    /// The requested family, `None` if it is empty.
    pub fn family(&self) -> Option<&str> {
        let family = self.family.trim();
        (!family.is_empty()).then_some(family)
    }
}
