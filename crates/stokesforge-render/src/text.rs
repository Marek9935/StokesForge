//! **The single source of text measurement** — task R4.1.
//!
//! Everything that, in the rendering work package, answers the question "how
//! much room does this string take?" goes through here, whichever backend asks
//! it. It is the non-negotiable invariant of §4.3 of `architecture-choisie.md`,
//! and the only place of the work package where a bug is **silent**: a
//! measurement disagreement shifts the axes between the screen and the export
//! without raising any error.
//!
//! # What is shared, and what is not
//!
//! The R0 0.1 measurement (2026-09-10) settled it: the
//! [`epaint::FontDefinitions`] are what must be shared, **not** the
//! [`epaint::Fonts`] instance — which is neither `Clone` nor usable through
//! `&self` anyway. The real danger is not "two instances", it is "two font
//! sets". A ten-line test is enough to guard it, and it is written
//! (`tests/measurement.rs`).
//!
//! # Correction made by R4: the height does depend on `pixels_per_point`
//!
//! Precision 1 of the R0 measurement announced metrics independent of
//! `pixels_per_point`. **That holds for advances, not for the line height**,
//! and R4 noticed it while writing the test meant to confirm it. `epaint`
//! snaps the height of a line to the grid of *physical* pixels
//! (`text_layout.rs`: `max_row_height =
//! point_scale.round_to_pixel(max_row_height)`), hence to a quantity that
//! changes with the screen. Measured: at 9.7 pt body size, a galley is 11
//! points high at `pixels_per_point = 1.0` and 12 at 1.25. R0 could not see
//! it: it compared glyph advances, not galley rectangles.
//!
//! The consequence would be real and exactly of the type the phase exists to
//! prevent: `DrawingArea::titled` lowers the plot area by the title height, so
//! **the exported figure would depend on the screen of whoever exports it**.
//!
//! Hence the rule, held by construction rather than by vigilance: **always
//! measure at [`MEASUREMENT_PIXELS_PER_POINT`]**, whatever the screen; the
//! real `pixels_per_point` is used only to *draw*. This is the reason for the
//! two methods of [`TextSource`], and the price paid is at most one physical
//! pixel between the reserved box and the glyphs displayed on a HiDPI screen —
//! invisible, and with no effect on the geometry, since the text is anchored
//! and not stretched.
//!
//! # What is not here
//!
//! Typst. Ordinary text — tick digits, axis titles — goes through the egui
//! fonts and **never** through Typst: that would be one Typst compilation per
//! tick (`architecture-choisie.md` §4.3). Task R8.5 will add a switch in
//! [`TextSource::measure`], nowhere else.
//!
//! # Addition of 2026-09-13 — a font choice, not only a default
//!
//! [`font_definitions`] used to return epaint's `FontDefinitions::default()`
//! without discussion. A one-off task (outside the R0-R8 sequencing) gave it
//! a real choice — three embedded font pairs (Libertinus, Fira, Noto) or a
//! system font by its name, with a silent fallback to Noto — carried by
//! [`crate::fonts`]. **The invariant of this module does not change**:
//! [`font_definitions`] remains the only place both backends call, with the
//! same result on both sides as long as they are given the same
//! [`fonts::FontChoice`]. Details in
//! `docs/archive/newarchitecture/architecture-choisie.md` §7.

use std::sync::{Arc, Mutex};

use epaint::text::{FontDefinitions, FontFamily, FontId, Fonts, TextOptions};
use epaint::{Color32, Galley, Vec2};
use plotters_backend::{BackendTextStyle, FontFamily as PlottersFamily};

use crate::fonts::{self, FontChoice};

/// The scale at which **every** layout measurement is made.
///
/// Fixed at 1.0 and not "the screen's": see the correction at the top of the
/// module. A figure must not change geometry because the user plugged in a
/// second screen.
pub const MEASUREMENT_PIXELS_PER_POINT: f32 = 1.0;

/// Factor between a font's "size" in Plotters' sense and its size in points
/// in epaint's and SVG's sense.
///
/// This is not a convenience constant: it is **exactly** the divisor that
/// `plotters-svg` writes in the `font-size` attribute (`svg.rs`:
/// `("font-size", format!("{}", style.size() / 1.24))`). Measuring at the
/// same size as the one written in the SVG is what makes the reserved box
/// match the glyphs actually displayed — otherwise the exported figure would
/// reserve correctly, then draw wrongly.
pub const PLOTTERS_SIZE_PER_POINT: f64 = 1.24;

/// epaint font size, in points, for a size in Plotters' sense.
pub fn font_size(plotters_size: f64) -> f32 {
    (plotters_size / PLOTTERS_SIZE_PER_POINT) as f32
}

/// **The** project's font definition, in one single place.
///
/// Resolves [`FontChoice::default`] — Noto Sans, embedded
/// (`fonts.rs`, font embedding of 2026-09-13) — rather than the raw epaint
/// `FontDefinitions::default()` this function used to return: see
/// [`crate::fonts`] for the choice mechanism (three embedded pairs or a system
/// font, with a fallback) and
/// `docs/archive/newarchitecture/architecture-choisie.md` §7 for the dated
/// decision that replaced the old sentence on fonts.
///
/// Off-screen measurement (SVG export, CLI, script) therefore starts from the
/// same font set as the window, which the R0 0.1 measurement verified at
/// 0.000000 px difference over 540 comparisons — a property unchanged by this
/// choice, since both sides keep calling this function and no other.
///
/// So that this is a property and not a coincidence, the application
/// explicitly sets these definitions on its `Context` at startup
/// (`stokesforge-app`): if egui ever changed its default, the figure would
/// stay the same on both sides.
///
/// **Do not call in a loop**: it decodes the embedded fonts. One
/// [`TextMeasurer`] per backend, built once, is enough.
///
/// To choose a font other than the default (another embedded pair, or a
/// system font), see [`font_definitions_for`].
pub fn font_definitions() -> FontDefinitions {
    font_definitions_for(&FontChoice::default())
}

/// Like [`font_definitions`], but with an explicit [`FontChoice`].
///
/// A caller who wants Fira, Libertinus, or a system font (the "Calibri" case
/// of the user request) goes through here — on both sides at once (screen and
/// export), with the **same** [`FontChoice`], or the invariant this module
/// carries breaks: see [`crate::fonts::resolve`] for the details of the
/// resolution and the fallback.
pub fn font_definitions_for(choice: &FontChoice) -> FontDefinitions {
    fonts::resolve(choice).definitions
}

/// The project's text rendering options.
///
/// Deliberately epaint's default values. Fixing them here rather than leaving
/// them to the caller avoids a backend one day taking its own and believing
/// it measures like the other.
pub fn text_options() -> TextOptions {
    TextOptions::default()
}

/// Translates a Plotters family and size into an [`epaint::FontId`].
///
/// epaint's default fonts offer only two real families: proportional and
/// monospace. `Serif`, `SansSerif` and any named family therefore fall back to
/// the proportional one — a deliberate fallback rather than a failure, and
/// identical on both sides anyway, which is the only thing the agreement of
/// the axes depends on.
pub fn font_id<S: BackendTextStyle>(style: &S) -> FontId {
    let family = match style.family() {
        PlottersFamily::Monospace => FontFamily::Monospace,
        PlottersFamily::Serif | PlottersFamily::SansSerif | PlottersFamily::Name(_) => {
            FontFamily::Proportional
        }
    };
    FontId::new(font_size(style.size()), family)
}

/// Rounds a galley size to the `(u32, u32)` Plotters expects.
///
/// Upward: a reserved box that is too small truncates the label, a box that
/// is too large costs only one pixel of margin. Shared function so that the
/// rounding itself cannot diverge between the two backends.
pub fn round_size(size: Vec2) -> (u32, u32) {
    (size.x.max(0.0).ceil() as u32, size.y.max(0.0).ceil() as u32)
}

/// Whatever can lay out a text — to measure it, and to draw it.
///
/// The two methods are distinct **on purpose**, and this is the only place of
/// the work package where the distinction matters:
///
/// - [`measure`](TextSource::measure) decides the **geometry** of the figure.
///   It must return the same number everywhere: same machine or not, same
///   screen or not, screen or export. Every implementation computes it at
///   [`MEASUREMENT_PIXELS_PER_POINT`];
/// - [`layout`](TextSource::layout) produces the `Galley` that will be
///   **drawn**. That one must be laid out at the target's `pixels_per_point`,
///   otherwise the text would be blurry; and, when there is an
///   `egui::Context`, it must come from it, because a `Galley` carries the UV
///   coordinates of its glyphs **in the atlas that laid it out** — a
///   `Shape::Text` built on a foreign `Galley` would be tessellated against
///   the `Context`'s atlas and display garbage.
///
/// What the two methods must have in common, and that is all, are the
/// [`font_definitions`].
pub trait TextSource {
    /// The room `text` takes, in points, at the measurement scale.
    fn measure(&self, text: &str, font_id: FontId) -> Vec2;

    /// The `Galley` to draw, at the target's display scale.
    fn layout(&self, text: &str, font_id: FontId, color: Color32) -> Arc<Galley>;
}

/// The measurement, as `DrawingBackend::estimate_text_size` must return it.
///
/// Both backends call **this function**, not a transcription each on their
/// own side: this is what makes task R4.5 verifiable rather than hoped for.
pub fn estimate<T: TextSource + ?Sized, S: BackendTextStyle>(
    source: &T,
    text: &str,
    style: &S,
) -> (u32, u32) {
    round_size(source.measure(text, font_id(style)))
}

/// An [`epaint::Fonts`] instance outside a `Context`, ready to measure and
/// lay out.
///
/// Built on [`font_definitions`] and [`text_options`], hence on the same
/// source as the screen. The `Mutex` is not a concurrency precaution but a
/// signature necessity: `Fonts::layout` takes `&mut self` while
/// `DrawingBackend::estimate_text_size` takes `&self`. Its cost is unrelated
/// to that of the layout itself, and epaint's galley cache absorbs repeated
/// ticks.
pub struct TextMeasurer {
    fonts: Mutex<Fonts>,
    pixels_per_point: f32,
}

impl TextMeasurer {
    /// A text source whose `Galley`s will be laid out at `pixels_per_point`,
    /// measuring with [`FontChoice::default`] (embedded Noto Sans).
    ///
    /// This parameter concerns **only** drawing: measurement is always done at
    /// [`MEASUREMENT_PIXELS_PER_POINT`].
    ///
    /// To choose another font, see [`Self::with_font_choice`].
    pub fn new(pixels_per_point: f32) -> Self {
        Self::with_font_choice(pixels_per_point, &FontChoice::default())
    }

    /// Like [`Self::new`], but with an explicit [`FontChoice`].
    ///
    /// To be called with the **same** [`FontChoice`] as the one set on the
    /// screen's `egui::Context` (`stokesforge-app`): this is what holds the
    /// invariant of this module when the font is not the default — see
    /// [`crate::fonts`].
    pub fn with_font_choice(pixels_per_point: f32, choice: &FontChoice) -> Self {
        Self {
            fonts: Mutex::new(Fonts::new(text_options(), font_definitions_for(choice))),
            pixels_per_point,
        }
    }

    /// The layout scale of the `Galley`s drawn by this source.
    pub fn pixels_per_point(&self) -> f32 {
        self.pixels_per_point
    }

    /// Lays out at a given scale.
    fn layout_at(
        &self,
        pixels_per_point: f32,
        text: &str,
        font_id: FontId,
        color: Color32,
    ) -> Arc<Galley> {
        // Poisoned lock: a `layout` can panic only on an epaint defect, and
        // refusing to measure would make an export fail for a reason the
        // caller cannot fix. We take the data back as it is.
        let mut fonts = self.fonts.lock().unwrap_or_else(|err| err.into_inner());
        fonts
            .with_pixels_per_point(pixels_per_point)
            .layout_no_wrap(text.to_owned(), font_id, color)
    }
}

impl Default for TextMeasurer {
    /// At `pixels_per_point = 1.0`: the right value for the export, where a
    /// Plotters "pixel" is an SVG unit and nothing else.
    fn default() -> Self {
        Self::new(MEASUREMENT_PIXELS_PER_POINT)
    }
}

impl std::fmt::Debug for TextMeasurer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextMeasurer")
            .field("pixels_per_point", &self.pixels_per_point)
            .finish_non_exhaustive()
    }
}

impl TextSource for TextMeasurer {
    fn measure(&self, text: &str, font_id: FontId) -> Vec2 {
        self.layout_at(
            MEASUREMENT_PIXELS_PER_POINT,
            text,
            font_id,
            Color32::PLACEHOLDER,
        )
        .rect
        .size()
    }

    fn layout(&self, text: &str, font_id: FontId, color: Color32) -> Arc<Galley> {
        self.layout_at(self.pixels_per_point, text, font_id, color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fact that justifies [`MEASUREMENT_PIXELS_PER_POINT`], locked **in
    /// the affirmative direction**: yes, a galley's height does depend on the
    /// display scale. If epaint ever fixed this behavior, this test would fail
    /// and the constant could go away — which is exactly what we want to know.
    #[test]
    fn a_galley_height_does_depend_on_pixels_per_point() {
        let source = TextMeasurer::new(1.0);
        let font = FontId::new(font_size(12.0), FontFamily::Proportional);
        let a = source
            .layout_at(1.0, "0,5", font.clone(), Color32::BLACK)
            .rect
            .height();
        let b = source
            .layout_at(1.25, "0,5", font, Color32::BLACK)
            .rect
            .height();
        assert_ne!(
            a, b,
            "epaint no longer snaps the line height to the physical pixel grid: \
             MEASUREMENT_PIXELS_PER_POINT no longer has a reason to exist"
        );
    }
}
