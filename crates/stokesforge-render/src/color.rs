//! Conversion of the spec's colors (`#RRGGBB`) to the two worlds.
//!
//! `stokesforge-plot` describes its colors by a hexadecimal string, not by a
//! structured type, on purpose: it is the lowest common denominator between
//! the renderers, and the neutral layer has no need to know how to compose
//! colors (`stokesforge-plot/src/style.rs`). The price is paid here, once,
//! for both backends.

/// The components of a `#RRGGBB` or `#RRGGBBAA` color, or `None`: the syntax
/// is that of [`stokesforge_plot::parse_hex`] (single source, shared with the
/// interface's color picker), only the return type differs. A color coming
/// from a file or a script is not validated on input (`stokesforge-plot`
/// carries only the string), so this path is really reachable: the caller
/// chooses its fallback.
pub fn parse_hex(text: &str) -> Option<(u8, u8, u8, u8)> {
    stokesforge_plot::parse_hex(text).map(|[r, g, b, a]| (r, g, b, a))
}

/// The color of a stroke, falling back to `fallback` if the string is
/// unreadable or empty. Opacity is ignored — see [`rgba_or`].
pub fn rgb_or(text: &str, fallback: (u8, u8, u8)) -> (u8, u8, u8) {
    parse_hex(text).map_or(fallback, |(r, g, b, _)| (r, g, b))
}

/// Like [`rgb_or`], opacity included (`#RRGGBBAA`; `#RRGGBB` is opaque).
/// A zero alpha means "no color": a transparent background, for example, to
/// embed the figure in another presentation.
pub fn rgba_or(text: &str, fallback: (u8, u8, u8, u8)) -> (u8, u8, u8, u8) {
    parse_hex(text).unwrap_or(fallback)
}

/// The color seen over white: `rgba` composited on a white background. This
/// is the color assumed behind a transparent background to choose the
/// automatic ink — a figure without a background most often ends up on a
/// light page or slide; on a dark slide, the text is then set to white by
/// hand.
pub fn over_white((r, g, b, a): (u8, u8, u8, u8)) -> (u8, u8, u8) {
    let alpha = a as f64 / 255.0;
    let blend = |c: u8| (c as f64 * alpha + 255.0 * (1.0 - alpha)).round() as u8;
    (blend(r), blend(g), blend(b))
}

/// Dark ink, the one for text and axes on a light background.
pub const DARK_INK: (u8, u8, u8) = (0x20, 0x20, 0x20);
/// Light ink, the one for text and axes on a dark background.
pub const LIGHT_INK: (u8, u8, u8) = (0xff, 0xff, 0xff);

/// Relative luminance of a color (WCAG 2.x), between 0 (black) and 1 (white).
pub fn relative_luminance((r, g, b): (u8, u8, u8)) -> f64 {
    let channel = |value: u8| {
        let c = value as f64 / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/// WCAG contrast ratio between two colors, from 1 to 21.
pub fn contrast_ratio(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (light, dark) = if la >= lb { (la, lb) } else { (lb, la) };
    (light + 0.05) / (dark + 0.05)
}

/// True if `background` is a "dark" background: light ink contrasts better
/// there than dark ink.
pub fn is_dark(background: (u8, u8, u8)) -> bool {
    contrast_ratio(LIGHT_INK, background) > contrast_ratio(DARK_INK, background)
}

/// The automatic ink on `background`: white on a dark background, dark on a
/// light one. It is the color of all the text and all the scales whose color
/// is left empty in the spec — a dark background therefore turns title, axes,
/// ticks and legend white all at once.
pub fn auto_ink(background: (u8, u8, u8)) -> (u8, u8, u8) {
    if is_dark(background) {
        LIGHT_INK
    } else {
        DARK_INK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_is_read_and_composited_over_white() {
        assert_eq!(
            rgba_or("#10182080", (0, 0, 0, 255)),
            (0x10, 0x18, 0x20, 0x80)
        );
        assert_eq!(rgba_or("", (1, 2, 3, 255)), (1, 2, 3, 255));
        assert_eq!(
            over_white((0, 0, 0, 0)),
            (255, 255, 255),
            "transparent: white"
        );
        assert_eq!(over_white((0, 0, 0, 255)), (0, 0, 0));
        // A half-transparent black background still counts as light over white.
        assert_eq!(auto_ink(over_white((0, 0, 0, 0x40))), DARK_INK);
    }

    #[test]
    fn automatic_ink_contrasts_with_the_background() {
        assert_eq!(auto_ink((0xff, 0xff, 0xff)), DARK_INK);
        assert_eq!(
            auto_ink((0xf5, 0xf0, 0xe1)),
            DARK_INK,
            "cream: light background"
        );
        assert_eq!(auto_ink((0x00, 0x00, 0x00)), LIGHT_INK);
        assert_eq!(
            auto_ink((0x1e, 0x2a, 0x3a)),
            LIGHT_INK,
            "midnight blue: dark background"
        );
        assert_eq!(
            auto_ink((0x40, 0x40, 0x40)),
            LIGHT_INK,
            "dark gray: dark background"
        );
    }

    #[test]
    fn reads_the_three_accepted_forms() {
        assert_eq!(parse_hex("#4c78a8"), Some((0x4c, 0x78, 0xa8, 255)));
        assert_eq!(parse_hex("4C78A8"), Some((0x4c, 0x78, 0xa8, 255)));
        assert_eq!(parse_hex("  #4c78a880 "), Some((0x4c, 0x78, 0xa8, 0x80)));
    }

    #[test]
    fn rejects_rather_than_guesses() {
        for bad in ["", "#", "#abc", "#12345g", "blue", "#4c78a8ff00", "#4c78aµ"] {
            assert_eq!(parse_hex(bad), None, "\"{bad}\" should have been rejected");
        }
    }

    #[test]
    fn the_fallback_is_the_callers_choice() {
        assert_eq!(rgb_or("not a color", (1, 2, 3)), (1, 2, 3));
        assert_eq!(rgb_or("#010203", (9, 9, 9)), (1, 2, 3));
    }
}
