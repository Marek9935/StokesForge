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

/// The graph-wide text settings of a [`crate::PlotSpec`] (#545): a font family
/// and a scale, the **graph default** that every text left on Auto follows.
///
/// This is the one place that decides what a text's font and size are: a text
/// whose own family or size was set by hand wins, else the graph setting, else
/// the built-in role constant. Rendering, the format toolbar and `pyst` all
/// resolve through it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphText<'a> {
    /// The graph font family; `None` = the embedded default.
    pub family: Option<&'a str>,
    /// Multiplier of the Auto sizes, within [`GraphText::SCALE_RANGE`]; 1 = today.
    pub scale: f64,
}

impl GraphText<'static> {
    /// No graph setting: every Auto text is drawn as it always was.
    pub const NEUTRAL: GraphText<'static> = GraphText {
        family: None,
        scale: 1.0,
    };
}

impl<'a> GraphText<'a> {
    /// Accepted scale, as a multiplier (25 % to 400 %).
    pub const SCALE_RANGE: (f64, f64) = (0.25, 4.0);

    /// The size of a text: `local` when it was set by hand, else the
    /// built-in size of its role times the graph scale; clamped to
    /// [`TextFont::SIZE_RANGE`]. A size that is not finite counts as Auto.
    ///
    /// Decision (#545, after the trial): a hand-set size is **absolute**, the
    /// scale does not multiply it; the field shows what was typed and the text
    /// is drawn at exactly that size.
    pub fn size(&self, local: Option<f64>, built_in: f64) -> f64 {
        let (low, high) = TextFont::SIZE_RANGE;
        match local.filter(|size| size.is_finite()) {
            Some(size) => size.clamp(low, high),
            None => (built_in * self.scale).clamp(low, high),
        }
    }

    /// The family of a text: its own when set, else the graph's, else `None`
    /// (the embedded default).
    pub fn family<'x>(&self, local: &'x str) -> Option<&'x str>
    where
        'a: 'x,
    {
        let local = local.trim();
        if local.is_empty() {
            self.family
        } else {
            Some(local)
        }
    }

    /// The scale as a percentage, for display.
    pub fn percent(&self) -> f64 {
        self.scale * 100.0
    }
}

impl TextFont {
    /// Minimum and maximum size accepted at rendering, like
    /// [`crate::AnnotationSpec::SIZE_RANGE`].
    pub const SIZE_RANGE: (f64, f64) = (4.0, 200.0);

    /// The plotted size of this text under the graph settings (#545): see
    /// [`GraphText::size`].
    pub fn resolved_size(&self, graph: GraphText<'_>, built_in: f64) -> f64 {
        graph.size(self.size, built_in)
    }

    /// The plotted family under the graph settings (#545): see
    /// [`GraphText::family`].
    pub fn resolved_family<'x>(&'x self, graph: GraphText<'x>) -> Option<&'x str> {
        graph.family(&self.family)
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    const GRAPH_150: GraphText<'static> = GraphText {
        family: Some("Libertinus Sans"),
        scale: 1.5,
    };

    /// #545. Witness: the scale applied to the local size too (12 would
    /// become 18).
    #[test]
    fn an_auto_size_follows_the_scale_and_a_hand_set_one_does_not() {
        let auto = TextFont::default();
        assert_eq!(auto.resolved_size(GRAPH_150, 24.0), 36.0);
        let by_hand = TextFont {
            size: Some(12.0),
            ..TextFont::default()
        };
        assert_eq!(by_hand.resolved_size(GRAPH_150, 24.0), 12.0);
        assert_eq!(auto.resolved_size(GraphText::NEUTRAL, 24.0), 24.0);
    }

    /// Witness: the graph family used over a family set by hand.
    #[test]
    fn a_family_set_by_hand_wins_over_the_graph_family() {
        let auto = TextFont::default();
        assert_eq!(auto.resolved_family(GRAPH_150), Some("Libertinus Sans"));
        let by_hand = TextFont {
            family: "Fira Sans".into(),
            ..TextFont::default()
        };
        assert_eq!(by_hand.resolved_family(GRAPH_150), Some("Fira Sans"));
        assert_eq!(auto.resolved_family(GraphText::NEUTRAL), None);
    }

    #[test]
    fn a_damaged_graph_setting_is_clamped_or_ignored() {
        let mut spec = crate::PlotSpec::empty("g");
        for (scale, expected) in [
            (Some(f64::NAN), 1.0),
            (Some(f64::INFINITY), 1.0),
            (Some(-3.0), 0.25),
            (Some(1e300), 4.0),
            (None, 1.0),
        ] {
            spec.text_scale = scale;
            assert_eq!(spec.graph_text().scale, expected, "{scale:?}");
        }
        spec.text_font = "   ".into();
        assert_eq!(spec.graph_text().family, None);
        // The clamp to the size range also holds for a huge built-in size.
        assert_eq!(GraphText::NEUTRAL.size(None, 1e9), TextFont::SIZE_RANGE.1);
    }
}
