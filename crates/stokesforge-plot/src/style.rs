//! Style of a curve and its markers, plus the default palette.
//!
//! Nothing here borrows its vocabulary from an engine: no `'o'`, no
//! `linestyle='--'`, no `PenStyle`. Named enums, which each
//! renderer translates on its own — the translation table lives in the renderer,
//! never in this crate.

/// Line style of a curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineStyle {
    /// No line at all.
    None,
    /// Solid line (default).
    #[default]
    Solid,
    /// Dashes.
    Dash,
    /// Dots.
    Dot,
    /// Dash-dot.
    DashDot,
}

/// Shape of a point marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SymbolShape {
    /// No marker.
    None,
    /// Disc (default).
    #[default]
    Circle,
    /// Square.
    Square,
    /// Triangle.
    Triangle,
    /// Diamond.
    Diamond,
    /// Straight cross `+`.
    Plus,
    /// Oblique cross `×`.
    Cross,
}

/// Default palette, in hexadecimal `#RRGGBB` — the most neutral form
/// there is: every renderer accepts it as is, and it passes through
/// persistence without conversion.
pub const DEFAULT_PALETTE: [&str; 10] = [
    "#4c78a8", "#f58518", "#54a24b", "#e45756", "#72b7b2", "#eeca3b", "#b279a2", "#ff9da6",
    "#9d755d", "#bab0ac",
];

/// Color of the default palette, cyclic.
pub fn palette_color(index: usize) -> &'static str {
    DEFAULT_PALETTE[index % DEFAULT_PALETTE.len()]
}

/// Line of a curve. [`LineStyle::None`] = no line at all.
///
/// The color is a `#RRGGBB` string and not a structured type: it is the
/// lowest common denominator between engines, and the crate does not need to
/// know how to compose colors to describe a graph.
#[derive(Debug, Clone, PartialEq)]
pub struct LineSpec {
    /// Line color, `#RRGGBB`.
    pub color: String,
    /// Line style.
    pub style: LineStyle,
    /// Thickness, in points.
    pub width: f64,
}

impl LineSpec {
    /// Default line, colored with the `index` color of the palette.
    pub fn from_palette(index: usize) -> Self {
        Self {
            color: palette_color(index).to_string(),
            ..Self::default()
        }
    }
}

impl Default for LineSpec {
    fn default() -> Self {
        Self {
            color: DEFAULT_PALETTE[0].to_string(),
            style: LineStyle::Solid,
            width: 1.5,
        }
    }
}

/// Marker of a point. [`SymbolShape::None`] = no marker.
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolSpec {
    /// Shape of the marker.
    pub shape: SymbolShape,
    /// Size, in points.
    pub size: f64,
    /// Fill color, `#RRGGBB`.
    pub color: String,
    /// Outline of the marker. The empty string means "the same color as the
    /// fill" — a renderer never has to guess an absent value.
    pub edge_color: String,
}

impl SymbolSpec {
    /// Default marker, colored with the `index` color of the palette.
    pub fn from_palette(index: usize) -> Self {
        Self {
            color: palette_color(index).to_string(),
            ..Self::default()
        }
    }

    /// Effective outline color: that of the fill if none is
    /// designated. Spares each renderer from rewriting the rule.
    pub fn effective_edge_color(&self) -> &str {
        if self.edge_color.is_empty() {
            &self.color
        } else {
            &self.edge_color
        }
    }
}

impl Default for SymbolSpec {
    fn default() -> Self {
        Self {
            shape: SymbolShape::Circle,
            size: 7.0,
            color: DEFAULT_PALETTE[0].to_string(),
            edge_color: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_cycles() {
        assert_eq!(palette_color(0), DEFAULT_PALETTE[0]);
        assert_eq!(palette_color(DEFAULT_PALETTE.len()), DEFAULT_PALETTE[0]);
        assert_eq!(palette_color(DEFAULT_PALETTE.len() + 1), DEFAULT_PALETTE[1]);
    }

    #[test]
    fn default_styles_take_the_first_palette_color() {
        assert_eq!(LineSpec::default().color, DEFAULT_PALETTE[0]);
        assert_eq!(SymbolSpec::default().color, DEFAULT_PALETTE[0]);
    }

    #[test]
    fn from_palette_colors_without_touching_the_rest() {
        let line = LineSpec::from_palette(1);
        assert_eq!(line.color, DEFAULT_PALETTE[1]);
        assert_eq!(line.style, LineSpec::default().style);
        assert_eq!(SymbolSpec::from_palette(1).color, DEFAULT_PALETTE[1]);
    }

    #[test]
    fn an_empty_edge_color_means_the_fill_color() {
        let symbol = SymbolSpec::default();
        assert_eq!(symbol.effective_edge_color(), symbol.color);
        let outlined = SymbolSpec {
            edge_color: "#000000".to_string(),
            ..SymbolSpec::default()
        };
        assert_eq!(outlined.effective_edge_color(), "#000000");
    }
}

/// The `[r, g, b, a]` components of a `#RRGGBB` (opaque) or
/// `#RRGGBBAA` color, or `None`.
///
/// **Single source** of the color syntax of the spec: rendering
/// (`stokesforge_render::color::parse_hex`) and the interface color picker
/// (`stokesforge-app::color_picker::parse_hex`) only adapt its
/// result to their type, so that a color accepted on one side cannot
/// be read differently on the other (issue #25). Tolerant of what is
/// tolerable — case-insensitive, edge spaces ignored, `#` optional —
/// and strict on the rest: a malformed string (length, non-hexadecimal
/// character, non-ASCII) returns `None` rather than an invented color.
pub fn parse_hex(text: &str) -> Option<[u8; 4]> {
    let text = text.trim();
    let text = text.strip_prefix('#').unwrap_or(text);
    // Hexadecimal digits only: `from_str_radix` would also accept
    // a leading `+` ("+c78a8" would read as a color).
    if !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |index: usize| u8::from_str_radix(text.get(index..index + 2)?, 16).ok();
    match text.len() {
        6 => Some([byte(0)?, byte(2)?, byte(4)?, 255]),
        8 => Some([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => None,
    }
}

#[cfg(test)]
mod parse_hex_tests {
    use super::parse_hex;

    #[test]
    fn syntax_edge_cases() {
        assert_eq!(parse_hex("#4c78a8"), Some([0x4c, 0x78, 0xa8, 255]));
        assert_eq!(parse_hex("4C78A8"), Some([0x4c, 0x78, 0xa8, 255]));
        assert_eq!(parse_hex("  #4c78a880 "), Some([0x4c, 0x78, 0xa8, 0x80]));
        for bad in [
            "",
            "#",
            "#12345",
            "#1234567",
            "#gg0000",
            "#é00000",
            "4c78a8ff0",
            "+c78a8",
        ] {
            assert_eq!(parse_hex(bad), None, "{bad:?}");
        }
    }
}
