//! Plotters' `SVGBackend`, **with the project's text measurement**.
//!
//! # Why a wrapper rather than a hand-written backend
//!
//! `plotters-svg` already does everything needed: ~800 lines that emit
//! correct SVG, maintained upstream, and §5 of `architecture-choisie.md`
//! explicitly keeps them. It lacks **only one** method, and it is
//! precisely the one that matters.
//!
//! # The documented pitfall, and why avoiding it is not optional
//!
//! `SVGBackend` does not override `estimate_text_size`. Left alone, it
//! falls back on the trait's default implementation → `layout_box` → and,
//! our Plotters fonts being cut off (`default-features = false`), on
//! `plotters/src/style/font/naive.rs`:
//!
//! ```text
//! let em = size / 1.24 / 1.24;
//! // width ≈ em * 0.7 * text.len()
//! ```
//!
//! A **monospace width estimated from the number of UTF-8 bytes**: "λ"
//! counts as 2, "µm" as 3, "111" measures like "WWW". The margins
//! reserved for labels would be wrong, hence the plot area different
//! from the screen's: *the exported figure would not be the figure seen*, and
//! nothing would signal it. It is the only silent defect of the work package
//! (`architecture-choisie.md` §4.4).
//!
//! A Rust trait cannot be overridden from outside: the only way to
//! replace a method of `DrawingBackend` on a type from another crate is
//! to wrap it. Hence [`MeasuredSvgBackend`], which delegates the ten drawing
//! methods to `SVGBackend` and **reimplements only one**. The test
//! `tests/measurement.rs` checks that the override is really in force, by
//! comparing with what the naive heuristic would have returned: without this
//! witness, a wrapper that *also* delegated `estimate_text_size` would pass
//! as correct.

use plotters::prelude::*;
use plotters_backend::{
    BackendColor, BackendCoord, BackendStyle, BackendTextStyle, DrawingErrorKind,
};
use stokesforge_render::fonts::{FontSet, FontSource};
use stokesforge_render::text::font_size;
use stokesforge_render::FontChoice;

use crate::typst_export::SvgTextMode;
use stokesforge_render::text::{estimate, TextMeasurer};

/// `SVGBackend`, augmented with the work package's shared text measurement.
///
/// The wrapper is thin by construction: everything that draws is delegated
/// as is, including `present`, which `SVGBackend` uses to close the
/// document. What is written here fits in one method.
pub struct MeasuredSvgBackend<'a> {
    inner: SVGBackend<'a>,
    measurer: &'a TextMeasurer,
    /// Typst path of the export (task R8.4) — a reference, not an owned
    /// value: [`crate::typst_export::TypstExport`] must remain
    /// readable by the caller **after** this backend has been dropped
    /// (`crate::render_to_string_with_typst`), so that
    /// [`crate::typst_export::splice`] finds the witnesses recorded
    /// during plotting. See the doc of [`Self::with_typst`].
    typst: Option<&'a crate::typst_export::TypstExport<'a>>,
    /// What makes ticks be written in the graph's font (#162): the
    /// `font-family` value and the text mode. `None`: Plotters' generic
    /// `font-family="sans-serif"`.
    font: Option<(String, SvgTextMode)>,
}

/// A text style that changes only one trait: the family written in
/// `font-family`. Everything else is the original style's.
struct NamedFamily<'s, S> {
    style: &'s S,
    family: &'s str,
}

impl<S: BackendTextStyle> BackendTextStyle for NamedFamily<'_, S> {
    type FontError = S::FontError;

    fn color(&self) -> BackendColor {
        self.style.color()
    }

    fn size(&self) -> f64 {
        self.style.size()
    }

    fn transform(&self) -> plotters_backend::FontTransform {
        self.style.transform()
    }

    fn style(&self) -> plotters_backend::FontStyle {
        self.style.style()
    }

    fn anchor(&self) -> plotters_backend::text_anchor::Pos {
        self.style.anchor()
    }

    fn family(&self) -> plotters_backend::FontFamily<'_> {
        plotters_backend::FontFamily::Name(self.family)
    }

    fn layout_box(&self, text: &str) -> Result<((i32, i32), (i32, i32)), Self::FontError> {
        self.style.layout_box(text)
    }

    fn draw<E, DrawFunc: FnMut(i32, i32, BackendColor) -> Result<(), E>>(
        &self,
        text: &str,
        pos: BackendCoord,
        draw: DrawFunc,
    ) -> Result<Result<(), E>, Self::FontError> {
        self.style.draw(text, pos, draw)
    }
}

/// The `font-family` value naming the graph's font, followed by a generic
/// fallback family (#162): "'Noto Sans', sans-serif". A system font that
/// cannot be found falls back to Noto, as on the screen.
fn css_family(choice: &FontChoice) -> String {
    let name = match stokesforge_render::fonts::resolve(choice).source {
        FontSource::Embedded(set) => set.sans_family_name().to_owned(),
        FontSource::System(name) => name,
        FontSource::FallenBackToNoto { .. } => FontSet::Noto.sans_family_name().to_owned(),
    };
    format!(
        "'{}', sans-serif",
        name.replace(['\'', '"', '&', '<', '>'], "")
    )
}

impl<'a> MeasuredSvgBackend<'a> {
    /// Wraps an already built `SVGBackend`.
    pub fn new(inner: SVGBackend<'a>, measurer: &'a TextMeasurer) -> Self {
        Self {
            inner,
            measurer,
            typst: None,
            font: None,
        }
    }

    /// A backend that writes into the given `String`.
    pub fn with_string(
        buffer: &'a mut String,
        size: (u32, u32),
        measurer: &'a TextMeasurer,
    ) -> Self {
        Self::new(SVGBackend::with_string(buffer, size), measurer)
    }

    /// Enables the Typst path (task R8.4) on this backend: texts in
    /// rich style ([`stokesforge_render::formula::is_rich`], issue #32) emit a
    /// witness replaced afterwards by [`crate::typst_export::splice`],
    /// all the others stay the ordinary `<text>` of `plotters-svg`.
    /// Additive like `EguiBackend::with_typst`: `new`/`with_string`
    /// stay unchanged, task 8.6.
    pub fn with_typst(mut self, typst: &'a crate::typst_export::TypstExport<'a>) -> Self {
        self.typst = Some(typst);
        self
    }

    /// Ticks in the graph's font (#162): in [`SvgTextMode::Text`] mode,
    /// they carry the same family as the Typst texts; in
    /// [`SvgTextMode::Outlines`] mode, they go through Typst and come out as
    /// outlines, so the document stays homogeneous.
    pub fn with_font(mut self, choice: &FontChoice, mode: SvgTextMode) -> Self {
        self.font = Some((css_family(choice), mode));
        self
    }
}

impl DrawingBackend for MeasuredSvgBackend<'_> {
    type ErrorType = <SVGBackend<'static> as DrawingBackend>::ErrorType;

    fn get_size(&self) -> (u32, u32) {
        self.inner.get_size()
    }

    fn ensure_prepared(&mut self) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.inner.ensure_prepared()
    }

    fn present(&mut self) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.inner.present()
    }

    fn draw_pixel(
        &mut self,
        point: BackendCoord,
        color: BackendColor,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.inner.draw_pixel(point, color)
    }

    fn draw_line<S: BackendStyle>(
        &mut self,
        from: BackendCoord,
        to: BackendCoord,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.inner.draw_line(from, to, style)
    }

    fn draw_rect<S: BackendStyle>(
        &mut self,
        upper_left: BackendCoord,
        bottom_right: BackendCoord,
        style: &S,
        fill: bool,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.inner.draw_rect(upper_left, bottom_right, style, fill)
    }

    fn draw_path<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        path: I,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.inner.draw_path(path, style)
    }

    fn draw_circle<S: BackendStyle>(
        &mut self,
        center: BackendCoord,
        radius: u32,
        style: &S,
        fill: bool,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.inner.draw_circle(center, radius, style, fill)
    }

    fn fill_polygon<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        vert: I,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.inner.fill_polygon(vert, style)
    }

    fn draw_text<S: BackendTextStyle>(
        &mut self,
        text: &str,
        style: &S,
        pos: BackendCoord,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        // R8.4: a rich text (issue #32) places a witness rectangle in place of
        // the ordinary `<text>`; `crate::typst_export::splice` replaces it
        // afterwards with the `typst-svg` fragment — see
        // `crate::typst_export`, module head, for the full mechanism
        // and why it does not need to touch `SVGBackend`. Degrades
        // to the ordinary `<text>` if Typst is not enabled or the
        // formula does not compile.
        if let Some(typst) = &self.typst {
            if stokesforge_render::formula::is_rich(style) {
                // A transparent text draws nothing, like the `<text>` of
                // `plotters-svg` and the screen path: no witness, hence
                // no Typst fragment (which, for its part, would ignore the
                // style's zero opacity and be visible).
                if style.color().alpha == 0.0 {
                    return Ok(());
                }
                let size_pt = font_size(style.size());
                if let Some(witness) = typst.record(
                    &stokesforge_render::formula::source(text, style),
                    size_pt,
                    stokesforge_render::formula::ink(style.color()),
                    style.anchor(),
                    style.transform(),
                    (pos.0 as f64, pos.1 as f64),
                ) {
                    // The witness need be neither well placed nor well
                    // sized in absolute terms: `splice`
                    // (`crate::typst_export`) spots it by its color
                    // *specific* to this call and rechecks its geometry against
                    // the one recorded by `record` above — so the
                    // position and size passed here must be
                    // exactly those `record` computed, no
                    // approximation possible on either side.
                    let (w, h) = witness.size;
                    return self.inner.draw_rect(
                        (pos.0, pos.1),
                        (pos.0 + w, pos.1 + h),
                        &witness.color,
                        true,
                    );
                }
            }
        }
        match &self.font {
            // Ticks as outlines: composed by Typst, like a literal
            // (escaped) text, in the graph's font.
            Some((_, SvgTextMode::Outlines)) if self.typst.is_some() => {
                if style.color().alpha == 0.0 {
                    return Ok(());
                }
                if let Some(witness) = self.typst.and_then(|typst| {
                    typst.record(
                        &stokesforge_render::formula::escape(text),
                        font_size(style.size()),
                        stokesforge_render::formula::ink(style.color()),
                        style.anchor(),
                        style.transform(),
                        (pos.0 as f64, pos.1 as f64),
                    )
                }) {
                    let (w, h) = witness.size;
                    return self.inner.draw_rect(
                        (pos.0, pos.1),
                        (pos.0 + w, pos.1 + h),
                        &witness.color,
                        true,
                    );
                }
                self.inner.draw_text(text, style, pos)
            }
            Some((family, _)) => {
                let named = NamedFamily { style, family };
                self.inner.draw_text(text, &named, pos)
            }
            None => self.inner.draw_text(text, style, pos),
        }
    }

    /// **The only method that is not delegated** — task R4.4.
    ///
    /// Literally the same function as the one the egui backend calls:
    /// [`stokesforge_render::text::estimate`]. Not "the same formula", not "the
    /// same source": the same call. This is what makes the agreement of the
    /// two backends (R4.5) checkable instead of merely hoped for.
    ///
    /// Ticks are written in the graph's font
    /// ([`Self::with_font`], #162): the named family is the one the measurement
    /// uses, instead of Plotters' generic `sans-serif`. Without
    /// `with_font`, the old asymmetry remains (measurement on the embedded
    /// font, display on the reader's).
    fn estimate_text_size<S: BackendTextStyle>(
        &self,
        text: &str,
        style: &S,
    ) -> Result<(u32, u32), DrawingErrorKind<Self::ErrorType>> {
        // R8.5 — the same invariant as on the screen side (`EguiBackend`): a
        // rich text is measured by querying Typst, never the `epaint`/naive
        // heuristic. Degrades to the `epaint` measurement if Typst is
        // not enabled or the formula does not compile.
        if let Some(typst) = &self.typst {
            if stokesforge_render::formula::is_rich(style) {
                let size_pt = font_size(style.size());
                let ink = stokesforge_render::formula::ink(style.color());
                let source = stokesforge_render::formula::source(text, style);
                if let Some(size) = typst.measure(&source, size_pt, ink) {
                    return Ok(size);
                }
            }
        }
        Ok(estimate(self.measurer, text, style))
    }
}
