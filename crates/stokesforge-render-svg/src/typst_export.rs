//! Typst export path — task R8.4.
//!
//! # The mechanism, and why it does not touch `SVGBackend`
//!
//! `plotters-svg::SVGBackend` writes directly into its target `String` on
//! each `draw_*` call, without going back through a document that could be
//! modified afterwards — and its `target` field is private, so
//! [`crate::backend::MeasuredSvgBackend`] cannot inject raw XML into it at
//! the moment it draws a Typst label. `architecture-choisie.md`
//! §8.1 suggests the way out: note **witness rectangles**, to be substituted
//! once the whole document is written.
//!
//! Concretely: [`TypstExport::record`] compiles the formula (or reuses the
//! cache, task 8.2), keeps its position, rotation and already rendered SVG
//! fragment, assigns itself a witness color **that has not yet been used by
//! any other witness of this export** (see [`TypstExport::next_witness_color`]),
//! and returns that color with the expected size of the rectangle to
//! [`crate::backend::MeasuredSvgBackend::draw_text`], which places a simple
//! `<rect>` of that color and size in place of the ordinary `<text>`. Once
//! the whole graph is plotted and the complete `String` obtained,
//! [`splice`] finds each witness **by its own color**, rechecking that the
//! `<rect>` found really has the expected position and size before
//! replacing it with the recorded fragment.
//!
//! **Accepted limit** (fixed since the single-color version, whose defect
//! this documented): the witness color varies per witness — the red
//! component stays fixed at a value that a user or a project default has
//! reasonably no reason to choose for a real graph element, green and blue
//! encode a counter (65536 distinct colors per export). [`splice`]
//! additionally requires the `<rect>` found to have the exact position and
//! size of the placed witness. To cause a collision, a real graph element
//! would therefore have to share *both* a precise color and a precise
//! geometry with a witness — a risk judged negligible rather than
//! eliminated by construction: a real identifier would cost making
//! `plotters-svg` carry an `id` attribute, which it precisely does not (see
//! `architecture-choisie.md` §5.2). There also remains the case, more
//! theoretical than practical, of an export with more than 65536 witnesses:
//! the color counter wraps around and two witnesses of the same export could
//! again share a color, the geometry check then being the only safety net
//! between them.
//!
//! # The unit of the nested fragment
//!
//! `typst_svg::svg` declares its root `<svg>` with `width`/`height` in
//! CSS points (`"12.5pt"`) — an **absolute** unit, different from the
//! convention already in force in this work package where a bare number in
//! Plotters' SVG represents a pixel (`stokesforge_render::text`,
//! `PLOTTERS_SIZE_PER_POINT`). Nesting Typst's `<svg>` as is would
//! therefore make its content resize by a factor of ~1.33 (1 CSS pt =
//! 1.3333 px) relative to the rest of the document. [`inner_svg`] removes
//! this root tag; [`splice`] replaces it with its own, in bare numbers
//! equal to `size_pt` — consistent with the rest of the document, since
//! Typst's `viewBox` is already in bare numbers on the same scale.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use plotters::style::RGBColor;
use plotters_backend::{text_anchor::Pos, FontTransform};
use stokesforge_render::formula::{self, Compiled, FormulaCache, Ink};
use stokesforge_render::FontChoice;
use typst::text::Font;
use typst_svg::SvgOptions;

use crate::typst_text;

/// Fixed `pixels_per_point` used for all the export's compilations
/// — see the doc of [`stokesforge_render::formula::FormulaCache`]: this term
/// of the cache key never varies on the export side, an SVG fragment being
/// vector and therefore independent of any screen resolution.
const EXPORT_PIXELS_PER_POINT: f32 = 1.0;

/// How the text composed by Typst is written in the SVG (#162).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SvgTextMode {
    /// **SVG text** (`<text>`), selectable and editable in a drawing
    /// program; the font is named, never embedded. A glyph with no Unicode
    /// character goes back to an outline. The default.
    #[default]
    Text,
    /// **Outlines** (`<path>`, via `typst-svg`): the document depends on no
    /// installed font. Ticks also become outlines.
    Outlines,
}

/// An SVG fragment already compiled for the export — the cached value
/// (task 8.2), the export analogue of `FormulaTexture` on the screen side.
///
/// Public despite its private fields: it is the type parameter of the
/// [`FormulaCache`] that a caller ([`crate::render_to_string_with_typst`])
/// must be able to name to build its own cache.
pub struct FormulaFragment {
    /// Inner content of a `typst_svg::svg` document, without its root
    /// `<svg>` tag — see the head doc for the reason.
    inner_svg: String,
    /// Size in points, the same frame as the rest of the text.
    size_pt: (f64, f64),
}

/// A slot to substitute — a witness already placed by `draw_rect`,
/// replaced by [`splice`] once the whole document is written.
struct PendingFormula {
    /// **Unrotated** top-left corner of the fragment, in the document's
    /// frame — it is also the pivot of the rotation, like
    /// `epaint::TextShape::pos` (see `crate::typst_screen::mesh` on the
    /// screen side, the same computation).
    origin: (f64, f64),
    /// Clockwise rotation in degrees — the unit that SVG's
    /// `rotate(deg, cx, cy)` expects directly.
    angle_deg: f64,
    size_pt: (f64, f64),
    inner_svg: String,
    /// Witness color specific to this witness — see
    /// [`TypstExport::next_witness_color`]. Distinct from that of any
    /// other witness of the same export.
    color: RGBColor,
    /// **Raw** position (before anchor offset) of the witness `<rect>`
    /// actually placed by `draw_rect`, in the backend's integer frame —
    /// distinct from [`Self::origin`], which serves to place the final
    /// fragment and already includes that offset. This is what [`splice`]
    /// rechecks before substituting.
    witness_pos: (i32, i32),
    /// Size in backend pixels of the witness `<rect>` actually placed —
    /// `size_pt` rounded as
    /// [`crate::backend::MeasuredSvgBackend::draw_text`] rounds for its
    /// `draw_rect` call. Also rechecked by [`splice`].
    witness_size: (i32, i32),
}

/// What [`TypstExport::record`] returns to the caller: the witness color
/// assigned to this call and the size in backend pixels that the witness
/// rectangle must have — exactly what
/// [`crate::backend::MeasuredSvgBackend::draw_text`] must pass to
/// `draw_rect`, and what [`splice`] then rechecks before substituting.
pub(crate) struct WitnessRect {
    pub(crate) color: RGBColor,
    pub(crate) size: (i32, i32),
}

/// What is needed to place Typst witnesses in the export — built once per
/// call to [`crate::render_to_string_with_typst`].
pub struct TypstExport<'a> {
    cache: &'a FormulaCache<FormulaFragment>,
    choice: &'a FontChoice,
    mode: SvgTextMode,
    sans: Font,
    math: Font,
    pending: RefCell<Vec<PendingFormula>>,
    /// Counter for the next call to [`Self::next_witness_color`] — see its
    /// doc for how it becomes a color.
    next_witness: Cell<u16>,
}

impl<'a> TypstExport<'a> {
    /// Resolves the fonts once (`choice`) — like
    /// `stokesforge_render_egui::TypstScreen::new`.
    pub fn new(
        cache: &'a FormulaCache<FormulaFragment>,
        choice: &'a FontChoice,
        mode: SvgTextMode,
    ) -> Result<Self, String> {
        let (sans, math) = formula::resolve_fonts(choice)?;
        Ok(Self {
            cache,
            choice,
            mode,
            sans,
            math,
            pending: RefCell::new(Vec::new()),
            next_witness: Cell::new(0),
        })
    }

    /// The fixed red component of every witness color of this export —
    /// a value that a user or a project default has reasonably no reason
    /// to choose for a real graph element (it was already, alone, the whole
    /// witness color before this fix: see the head doc, section
    /// "Accepted limit").
    const WITNESS_RED: u8 = 1;

    /// The witness color of the next witness of this export: red stays
    /// fixed at [`Self::WITNESS_RED`], green and blue encode a counter —
    /// 65536 distinct colors per export, far beyond what a realistic figure
    /// can contain in Typst labels. This is what makes two witnesses of the
    /// same export impossible to confuse by color, where the old mechanism
    /// (a single shared color) made them indistinguishable from each other.
    ///
    /// Beyond 65536 calls, the counter wraps around and two witnesses
    /// could again share a color — the geometry check of [`splice`] would
    /// then be the only safety net between them, a volume that a realistic
    /// figure should not reach.
    fn next_witness_color(&self) -> RGBColor {
        let n = self.next_witness.get();
        self.next_witness.set(n.wrapping_add(1));
        RGBColor(Self::WITNESS_RED, (n >> 8) as u8, n as u8)
    }

    /// The compiled fragment (or reused from the cache) for `text` at
    /// `size_pt` body size, in `ink` (issue #30: `typst-svg` writes the color
    /// into the fragment). `None` if the formula does not compile.
    fn fragment(&self, text: &str, size_pt: f32, ink: Ink) -> Option<Arc<FormulaFragment>> {
        self.cache
            .get_or_try_insert(
                text,
                size_pt,
                ink,
                self.choice,
                EXPORT_PIXELS_PER_POINT,
                self.sans.clone(),
                self.math.clone(),
                |compiled| Ok(build_fragment(compiled, self.mode)),
            )
            .ok()
    }

    /// The size (rounded to backend pixels) that `text` compiled by Typst
    /// would occupy — the invariant of task 8.5, export side. `ink`
    /// does not change the size: it makes the measurement land on the entry
    /// that [`Self::record`] reuses.
    pub fn measure(&self, text: &str, size_pt: f32, ink: Ink) -> Option<(u32, u32)> {
        let fragment = self.fragment(text, size_pt, ink)?;
        Some(formula::round_size_pt(fragment.size_pt))
    }

    /// Records a witness to be substituted by [`splice`]: compiles the
    /// formula, computes its anchor and rotation exactly as
    /// `EguiBackend::draw_text` does on the screen (same shared functions
    /// of `stokesforge_render::formula`), assigns it a color that has not
    /// yet been used by any other witness of this export (see
    /// [`Self::next_witness_color`]), and keeps the fragment. Returns the
    /// color and the size in backend pixels of the witness rectangle to
    /// draw — exactly what a caller (`MeasuredSvgBackend`) must pass to
    /// `draw_rect`, and what [`splice`] will recheck. `None` if the
    /// formula does not compile.
    pub(crate) fn record(
        &self,
        text: &str,
        size_pt: f32,
        ink: Ink,
        anchor: Pos,
        transform: FontTransform,
        pos: (f64, f64),
    ) -> Option<WitnessRect> {
        let fragment = self.fragment(text, size_pt, ink)?;
        let offset = formula::anchor_offset(fragment.size_pt, anchor.h_pos, anchor.v_pos);
        let offset = formula::rotate_offset(offset, transform.clone());
        let origin = (pos.0 + offset.0, pos.1 + offset.1);
        let color = self.next_witness_color();
        let witness_pos = (pos.0.round() as i32, pos.1.round() as i32);
        // The same rounding that `MeasuredSvgBackend::draw_text` used to
        // apply itself before calling `draw_rect` — brought back here so
        // that the rectangle actually drawn and the one `splice` expects
        // are computed by the same code, never twice.
        let witness_size = (
            fragment.size_pt.0.ceil().max(1.0) as i32,
            fragment.size_pt.1.ceil().max(1.0) as i32,
        );
        self.pending.borrow_mut().push(PendingFormula {
            origin,
            angle_deg: formula::rotation_degrees(transform),
            size_pt: fragment.size_pt,
            inner_svg: fragment.inner_svg.clone(),
            color,
            witness_pos,
            witness_size,
        });
        Some(WitnessRect {
            color,
            size: witness_size,
        })
    }
}

/// Compiles a page into an SVG fragment — task 8.4.
fn build_fragment(compiled: &Compiled, mode: SvgTextMode) -> FormulaFragment {
    let size_pt = compiled.size_pt();
    let Some(page) = compiled.page() else {
        return FormulaFragment {
            inner_svg: String::new(),
            size_pt: (0.0, 0.0),
        };
    };
    let inner_svg = match mode {
        SvgTextMode::Text => typst_text::frame_to_svg(&page.frame),
        SvgTextMode::Outlines => {
            let document = typst_svg::svg(page, &SvgOptions::default());
            inner_svg(&document).to_owned()
        }
    };
    FormulaFragment { inner_svg, size_pt }
}

/// The inner content of a standalone SVG document produced by
/// `typst_svg::svg` — between the opening `<svg ...>` tag and
/// `</svg>`. See the module head doc for the reason.
fn inner_svg(document: &str) -> &str {
    let start = document.find('>').map_or(0, |i| i + 1);
    let end = document.rfind("</svg>").unwrap_or(document.len());
    document.get(start..end.max(start)).unwrap_or("").trim()
}

/// Substitutes each witness placed by [`TypstExport::record`] in `buffer`
/// with its Typst fragment — task R8.4, fixed by the per-witness own color
/// and the geometry check (see the head doc, section
/// "Accepted limit").
///
/// `buffer` is the complete SVG document, obtained **after** the backend
/// that built `typst` has been dropped (taking it out of scope releases
/// the borrow on the target `String` that `SVGBackend` held) — see
/// `crate::render_to_string_with_typst`.
pub fn splice(buffer: &str, typst: &TypstExport<'_>) -> String {
    let pending = typst.pending.borrow();
    let mut result = buffer.to_string();
    for formula in pending.iter() {
        // A missing witness, or one found but with a geometry different
        // from the expected one (the signature of a color collision with a
        // real graph element), is left as is: degradation rather than
        // panic or corrupted export, on an otherwise valid document.
        if let Some(range) = matching_witness_range(&result, formula) {
            result.replace_range(range, &group_for(formula));
        }
    }
    result
}

/// Looks in `haystack` for the witness `<rect>` of `formula`: its own color
/// **and** its geometry (position, size) must match what
/// [`TypstExport::record`] recorded. A `<rect>` of the right color but a
/// different geometry is not kept — it is exactly the case of a real graph
/// element that would carry the same color by bad luck, which the check is
/// meant to rule out.
fn matching_witness_range(
    haystack: &str,
    formula: &PendingFormula,
) -> Option<std::ops::Range<usize>> {
    let witness = format!("fill=\"{}\"", make_hex(formula.color));
    let marker = haystack.find(&witness)?;
    let tag_start = haystack[..marker].rfind("<rect")?;
    let tag_end = haystack[marker..].find("/>").map(|i| marker + i + 2)?;
    let tag = &haystack[tag_start..tag_end];
    geometry_matches(tag, formula.witness_pos, formula.witness_size).then_some(tag_start..tag_end)
}

/// The value of a `name="..."` attribute in an XML tag fragment —
/// enough to read `x`, `y`, `width`, `height` on the `<rect>` emitted by
/// `plotters-svg::SVGBackend::draw_rect` (see its source for the exact
/// format of the attributes, reproduced here for reading as [`make_hex`]
/// reproduces its writing of colors).
fn attr_value<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

/// True if the `<rect>` `tag` has the expected position and size, within
/// 0.5 px — the coordinates emitted by `plotters-svg` are integers, the
/// tolerance absorbs only the `f64` parsing rounding, not a real geometric
/// margin.
fn geometry_matches(tag: &str, pos: (i32, i32), size: (i32, i32)) -> bool {
    const EPS: f64 = 0.5;
    let parse = |name| attr_value(tag, name).and_then(|v| v.parse::<f64>().ok());
    let (Some(x), Some(y), Some(w), Some(h)) =
        (parse("x"), parse("y"), parse("width"), parse("height"))
    else {
        return false;
    };
    (x - pos.0 as f64).abs() < EPS
        && (y - pos.1 as f64).abs() < EPS
        && (w - size.0 as f64).abs() < EPS
        && (h - size.1 as f64).abs() < EPS
}

/// The replacement `<g>` of a witness: rotation then translation to
/// `origin`, a nested `<svg>` sized in bare numbers (see the head doc for
/// the unit), and Typst's inner fragment.
///
/// The nested `<svg>` redeclares `xmlns:xlink`: Typst's glyphs are
/// `<use xlink:href="#g…">`, and the root tag that declared this
/// prefix was removed by [`inner_svg`]. Without it, a tolerant browser
/// displayed the SVG, but `usvg` rejected the whole document — so
/// the PDF and PNG export of a graph with Typst text failed (seen when
/// wiring the export into the application, issue #34).
fn group_for(formula: &PendingFormula) -> String {
    let (ox, oy) = formula.origin;
    let (w, h) = formula.size_pt;
    format!(
        "<g transform=\"rotate({deg}, {ox}, {oy}) translate({ox}, {oy})\">\
         <svg width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" \
         xmlns=\"http://www.w3.org/2000/svg\" \
         xmlns:xlink=\"http://www.w3.org/1999/xlink\">{inner}</svg></g>",
        deg = formula.angle_deg,
        inner = formula.inner_svg,
    )
}

/// The same hexadecimal representation as `plotters-svg::make_svg_color`
/// (private module of that third-party crate) — reproduced here to recognize
/// the witness color in the produced document, without depending on a
/// function that `plotters-svg` does not expose.
fn make_hex(color: plotters::style::RGBColor) -> String {
    format!("#{:02X}{:02X}{:02X}", color.0, color.1, color.2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotters_backend::text_anchor::{HPos, VPos};

    const BLACK: Ink = [0, 0, 0, 255];

    fn export() -> (FormulaCache<FormulaFragment>, FontChoice) {
        (FormulaCache::new(), FontChoice::default())
    }

    /// A trivial formula gets recorded and returns a non-zero size.
    #[test]
    fn recording_a_trivial_formula_returns_a_non_zero_size() {
        let (cache, choice) = export();
        let typst = TypstExport::new(&cache, &choice, SvgTextMode::Outlines)
            .expect("font resolution failed");
        let anchor = Pos::new(HPos::Left, VPos::Top);
        let witness = typst
            .record(
                "$lambda$",
                14.0,
                BLACK,
                anchor,
                FontTransform::None,
                (10.0, 20.0),
            )
            .expect("the formula should have compiled");
        assert!(witness.size.0 > 0 && witness.size.1 > 0);
    }

    /// An invalid formula does not get recorded — degradation, not panic.
    #[test]
    fn an_invalid_formula_is_not_recorded() {
        let (cache, choice) = export();
        let typst = TypstExport::new(&cache, &choice, SvgTextMode::Outlines)
            .expect("font resolution failed");
        let anchor = Pos::new(HPos::Left, VPos::Top);
        assert!(typst
            .record(
                "$ (unclosed",
                14.0,
                BLACK,
                anchor,
                FontTransform::None,
                (0.0, 0.0)
            )
            .is_none());
    }

    /// `splice` finds the witness and injects the Typst fragment — leaving
    /// the rest of the document intact.
    #[test]
    fn splice_replaces_the_witness_with_the_fragment() {
        let (cache, choice) = export();
        let typst = TypstExport::new(&cache, &choice, SvgTextMode::Outlines)
            .expect("font resolution failed");
        let anchor = Pos::new(HPos::Left, VPos::Top);
        let witness = typst
            .record(
                "$lambda$",
                14.0,
                BLACK,
                anchor,
                FontTransform::None,
                (10.0, 20.0),
            )
            .expect("compilation failed");

        let hex = make_hex(witness.color);
        let (w, h) = witness.size;
        let document = format!(
            "<svg><rect x=\"10\" y=\"20\" width=\"{w}\" height=\"{h}\" fill=\"{hex}\"/><circle/></svg>"
        );
        let result = splice(&document, &typst);
        assert!(!result.contains(&hex), "the witness is still present");
        assert!(
            result.contains("<svg width="),
            "the nested fragment is missing"
        );
        assert!(
            result.contains("<circle/>"),
            "the rest of the document was lost"
        );
    }

    /// A document without a witness does not change — the normal case when no
    /// label of the figure is a formula.
    #[test]
    fn without_a_witness_the_document_does_not_change() {
        let (cache, choice) = export();
        let typst = TypstExport::new(&cache, &choice, SvgTextMode::Outlines)
            .expect("font resolution failed");
        let document = "<svg><circle/></svg>".to_string();
        assert_eq!(splice(&document, &typst), document);
    }

    /// Two different formulas in the same export do not get mixed up:
    /// each receives its own fragment, never the other's. With the old
    /// single-color mechanism, this test could have passed by accident
    /// (the two witnesses were indistinguishable by color); a color specific
    /// to each witness makes it verifiable for real.
    #[test]
    fn two_different_formulas_do_not_get_mixed_up() {
        let (cache, choice) = export();
        let typst = TypstExport::new(&cache, &choice, SvgTextMode::Outlines)
            .expect("font resolution failed");
        let anchor = Pos::new(HPos::Left, VPos::Top);

        let lambda = typst
            .record(
                "$lambda$",
                14.0,
                BLACK,
                anchor,
                FontTransform::None,
                (10.0, 20.0),
            )
            .expect("compilation failed (lambda)");
        let alpha = typst
            .record(
                "$alpha$",
                14.0,
                BLACK,
                anchor,
                FontTransform::None,
                (100.0, 200.0),
            )
            .expect("compilation failed (alpha)");

        assert_ne!(
            make_hex(lambda.color),
            make_hex(alpha.color),
            "the two witnesses share a color"
        );

        let (inner_lambda, inner_alpha) = {
            let pending = typst.pending.borrow();
            (pending[0].inner_svg.clone(), pending[1].inner_svg.clone())
        };
        assert!(!inner_lambda.is_empty() && !inner_alpha.is_empty());
        assert_ne!(
            inner_lambda, inner_alpha,
            "the two formulas compile to the same fragment, the test cannot prove anything"
        );

        let document = format!(
            "<svg><rect x=\"10\" y=\"20\" width=\"{}\" height=\"{}\" fill=\"{}\"/>\
             <rect x=\"100\" y=\"200\" width=\"{}\" height=\"{}\" fill=\"{}\"/></svg>",
            lambda.size.0,
            lambda.size.1,
            make_hex(lambda.color),
            alpha.size.0,
            alpha.size.1,
            make_hex(alpha.color),
        );

        let result = splice(&document, &typst);

        // Each compiled Typst fragment (lambda, alpha) itself contains
        // inner `<g>`s (Typst structures its glyphs that way): bounding a
        // block at the first `</g>` met would therefore cut inside the
        // fragment rather than at the end of the substitution group. Instead
        // each block is bounded by the start of the other — as the two
        // groups are written in the order `record` was called, the lambda
        // one necessarily precedes the alpha one in the document.
        let lambda_group = result
            .find("translate(10, 20)")
            .expect("lambda group missing");
        let alpha_group = result
            .find("translate(100, 200)")
            .expect("alpha group missing");
        assert!(
            lambda_group < alpha_group,
            "the lambda group should precede the alpha one in the document"
        );

        let lambda_block = &result[lambda_group..alpha_group];
        let alpha_block = &result[alpha_group..];
        assert!(
            lambda_block.contains(&inner_lambda),
            "the lambda fragment is not in its group"
        );
        assert!(
            alpha_block.contains(&inner_alpha),
            "the alpha fragment is not in its group"
        );
        assert!(
            !lambda_block.contains(&inner_alpha),
            "the alpha fragment leaked into the lambda group"
        );
        assert!(
            !alpha_block.contains(&inner_lambda),
            "the lambda fragment leaked into the alpha group"
        );
    }

    /// A `<rect>` of the right witness color but a size different from the
    /// expected one — the signature of a collision with a real graph
    /// element that would carry the same color by bad luck — is not
    /// substituted: the geometry check must rule it out, not only in
    /// theory.
    #[test]
    fn a_witness_of_the_wrong_size_is_not_substituted() {
        let (cache, choice) = export();
        let typst = TypstExport::new(&cache, &choice, SvgTextMode::Outlines)
            .expect("font resolution failed");
        let anchor = Pos::new(HPos::Left, VPos::Top);
        let witness = typst
            .record(
                "$lambda$",
                14.0,
                BLACK,
                anchor,
                FontTransform::None,
                (10.0, 20.0),
            )
            .expect("compilation failed");

        let hex = make_hex(witness.color);
        let (w, h) = witness.size;
        let document = format!(
            "<svg><rect x=\"10\" y=\"20\" width=\"{}\" height=\"{}\" fill=\"{hex}\"/></svg>",
            w + 50,
            h + 50,
        );

        let result = splice(&document, &typst);
        assert_eq!(
            result, document,
            "a witness of the wrong size should not have been substituted"
        );
    }
}
