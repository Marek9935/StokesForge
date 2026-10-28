//! Object format: what the "object format" part of the "Tools" bar sets on a
//! selected graphic object (issue #64, after #76), or the line of a series
//! selected from Object Manager (#83).
//!
//! Unlike the text format ([`crate::text_format`]), which writes tags into a
//! text, here each setting is a **model field**
//! ([`stokesforge_plot::ShapeSpec`]): line color, fill color (whose alpha is
//! the opacity, `#RRGGBBAA`), thickness. So the bar only sends the chosen
//! value; the application applies it to the selected object through a single
//! undoable `Action::SetShape`.
//!
//! Everything is pure, tested without a window.

use stokesforge_plot::{SeriesSpec, ShapeSpec};

/// A setting that the bar knows how to apply.
#[derive(Debug, Clone, PartialEq)]
pub enum ObjectFormat {
    /// Line color; empty = automatic (contrasts with the background).
    LineColor(String),
    /// Fill color; empty = none. No effect on a line or an arrow, which have
    /// no surface.
    FillColor(String),
    /// Line thickness, in points.
    LineWidth(f64),
}

impl ObjectFormat {
    /// Applies the line settings to the selected series (#83).
    pub fn apply_series(&self, series: &mut SeriesSpec) -> bool {
        let before = series.clone();
        match self {
            Self::LineColor(color) => series.line.color = color.clone(),
            Self::LineWidth(width) if width.is_finite() && *width > 0.0 => {
                series.line.width = *width
            }
            _ => {}
        }
        *series != before
    }

    /// Applies the setting to `shape`. Returns `true` if the object changed.
    pub fn apply(&self, shape: &mut ShapeSpec) -> bool {
        let before = shape.clone();
        match self {
            ObjectFormat::LineColor(color) => shape.line_color = color.clone(),
            ObjectFormat::FillColor(color) => {
                if shape.kind.is_closed() {
                    shape.fill_color = color.clone();
                }
            }
            ObjectFormat::LineWidth(width) => {
                if width.is_finite() && *width > 0.0 {
                    shape.line_width = *width;
                }
            }
        }
        *shape != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_plot::ShapeKind;

    fn shape(kind: ShapeKind) -> ShapeSpec {
        ShapeSpec::new(kind, 0.0, 0.0, 1.0, 1.0)
    }

    #[test]
    fn each_setting_sets_its_field_and_says_whether_it_changed() {
        let mut rect = shape(ShapeKind::Rectangle);
        assert!(ObjectFormat::LineColor("#aa3311".into()).apply(&mut rect));
        assert!(ObjectFormat::FillColor("#33669980".into()).apply(&mut rect));
        assert!(ObjectFormat::LineWidth(3.0).apply(&mut rect));
        assert_eq!(rect.line_color, "#aa3311");
        assert_eq!(rect.fill_color, "#33669980");
        assert_eq!(rect.line_width, 3.0);
        // The same setting changes nothing.
        assert!(!ObjectFormat::LineWidth(3.0).apply(&mut rect));
        assert!(!ObjectFormat::LineColor("#aa3311".into()).apply(&mut rect));
    }

    #[test]
    fn a_line_or_an_arrow_has_no_fill() {
        for kind in [ShapeKind::Line, ShapeKind::Arrow] {
            let mut line = shape(kind);
            assert!(!ObjectFormat::FillColor("#336699".into()).apply(&mut line));
            assert!(line.fill_color.is_empty());
        }
    }

    #[test]
    fn an_invalid_thickness_applies_nothing() {
        let mut rect = shape(ShapeKind::Rectangle);
        for width in [0.0, -2.0, f64::NAN, f64::INFINITY] {
            assert!(!ObjectFormat::LineWidth(width).apply(&mut rect));
        }
        assert_eq!(rect.line_width, ShapeSpec::DEFAULT_WIDTH);
    }
}

#[cfg(test)]
mod series_tests {
    use super::*;
    #[test]
    fn the_series_line_is_set_without_changing_the_symbols() {
        let mut spec = SeriesSpec::new(stokesforge_core::NodeId::from_raw(1));
        let symbol = spec.symbol.clone();
        assert!(ObjectFormat::LineColor("#cc3300".into()).apply_series(&mut spec));
        assert!(ObjectFormat::LineWidth(4.0).apply_series(&mut spec));
        assert_eq!(spec.line.color, "#cc3300");
        assert_eq!(spec.line.width, 4.0);
        assert_eq!(spec.symbol, symbol);
        assert!(!ObjectFormat::FillColor("#cc3300".into()).apply_series(&mut spec));
        assert!(!ObjectFormat::LineWidth(f64::NAN).apply_series(&mut spec));
    }
}
