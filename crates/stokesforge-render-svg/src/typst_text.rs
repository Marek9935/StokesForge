//! "Text" SVG emitter for a Typst page (#162).
//!
//! `typst-svg` **always** writes its glyphs as outlines (`<use>` of
//! `<symbol>` in `<defs>`): no option produces `<text>`. So that an exported
//! figure stays editable (selectable, searchable, editable in Inkscape or
//! Illustrator), this module walks the compiled `Frame` and writes the SVG
//! fragment itself:
//!
//! - a **group** keeps its translation and transformation;
//! - a **text** becomes a `<text>` whose every character carries the
//!   position (`x`, `y`) that Typst computed for its glyph, with the
//!   family, size, bold/italic and color of the text's font;
//! - a **shape** (fraction bar, root, rectangle) becomes a `<path>`
//!   or a `<rect>`, like `typst-svg`;
//! - an image, a link and an introspection tag have no rendering here.
//!
//! # Outline fallback, per glyph
//!
//! A glyph is written as text only if its character finds **exactly** that
//! glyph through the font's `cmap` table. A glyph with no Unicode character
//! (stretched delimiter, math variant), a ligature or a character that the
//! font does not link to that glyph comes out as a `<path>` — the outline
//! of that glyph alone — so that the rendering stays Typst's. Spaces have
//! no ink: they are omitted, which also prevents a reader from collapsing
//! whitespace and shifting the following positions.
//!
//! The font is **never** embedded in the SVG: `font-family` names the
//! family, followed by a generic fallback family (decision of #162).

use std::fmt::Write;

use ttf_parser::{GlyphId, OutlineBuilder};
use typst::layout::{Frame, FrameItem, Point};
use typst::text::{FontStyle, TextItem};
use typst::visualize::{Color, CurveItem, FillRule, FixedStroke, Geometry, Paint, Shape};

/// The SVG fragment of `frame`: everything that would go in an `<svg>`,
/// without its root tag.
pub(crate) fn frame_to_svg(frame: &Frame) -> String {
    let mut out = String::new();
    write_frame(&mut out, frame);
    out
}

fn write_frame(out: &mut String, frame: &Frame) {
    for (pos, item) in frame.items() {
        match item {
            FrameItem::Group(group) => {
                let ts = group.transform;
                let _ = write!(
                    out,
                    "<g transform=\"translate({} {}) matrix({} {} {} {} {} {})\">",
                    num(pos.x.to_pt()),
                    num(pos.y.to_pt()),
                    num(ts.sx.get()),
                    num(ts.ky.get()),
                    num(ts.kx.get()),
                    num(ts.sy.get()),
                    num(ts.tx.to_pt()),
                    num(ts.ty.to_pt()),
                );
                write_frame(out, &group.frame);
                out.push_str("</g>");
            }
            FrameItem::Text(text) => write_text(out, *pos, text),
            FrameItem::Shape(shape, _) => write_shape(out, *pos, shape),
            FrameItem::Image(..) | FrameItem::Link(..) | FrameItem::Tag(..) => {}
        }
    }
}

/// A short SVG number: no trailing zeros, never `NaN` or `inf`.
fn num(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_owned();
    }
    let text = format!("{value:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() || text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

/// `#RRGGBB` and opacity of a paint. A gradient or a pattern (never produced
/// by the graph's texts) falls back to black.
fn paint_attrs(paint: &Paint, role: &str) -> String {
    let Paint::Solid(color) = paint else {
        return format!("{role}=\"#000000\"");
    };
    let [r, g, b, a] = color_u8(color);
    let mut attrs = format!("{role}=\"#{r:02X}{g:02X}{b:02X}\"");
    if a != 255 {
        let _ = write!(attrs, " {role}-opacity=\"{}\"", num(f64::from(a) / 255.0));
    }
    attrs
}

fn color_u8(color: &Color) -> [u8; 4] {
    color.to_vec4_u8()
}

fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
}

/// What a glyph becomes: a `<text>` character, or an outline.
enum Placed {
    Char(char, f64, f64),
    Outline(u16, f64, f64),
}

fn write_text(out: &mut String, pos: Point, text: &TextItem) {
    let size = text.size.to_pt();
    let ttf = text.font.ttf();
    let mut x = 0.0;
    let mut y = 0.0;
    let mut placed = Vec::with_capacity(text.glyphs.len());
    for glyph in &text.glyphs {
        let gx = pos.x.to_pt() + x + glyph.x_offset.at(text.size).to_pt();
        // Typst counts `y_offset` upward, SVG downward.
        let gy = pos.y.to_pt() - y - glyph.y_offset.at(text.size).to_pt();
        x += glyph.x_advance.at(text.size).to_pt();
        y += glyph.y_advance.at(text.size).to_pt();
        let source = text.text.get(glyph.range()).unwrap_or("");
        let mut chars = source.chars();
        let single = match (chars.next(), chars.next()) {
            (Some(c), None) => Some(c),
            _ => None,
        };
        match single {
            // No ink: omitted (see the head doc).
            Some(c) if c.is_whitespace() => {}
            Some(c) if ttf.glyph_index(c) == Some(GlyphId(glyph.id)) => {
                placed.push(Placed::Char(c, gx, gy));
            }
            _ => placed.push(Placed::Outline(glyph.id, gx, gy)),
        }
    }

    let fill = paint_attrs(&text.fill, "fill");
    let chars: Vec<_> = placed
        .iter()
        .filter_map(|p| match p {
            Placed::Char(c, x, y) => Some((*c, *x, *y)),
            Placed::Outline(..) => None,
        })
        .collect();
    if !chars.is_empty() {
        let info = text.font.info();
        let (family, generic) = (&info.family, generic_family(&info.family));
        let weight = info.variant.weight.to_number();
        let _ = write!(
            out,
            "<text font-family=\"'{}', {generic}\" font-size=\"{}\" {fill}",
            family.replace('&', "&amp;").replace('"', "&quot;"),
            num(size),
        );
        if weight != 400 {
            let _ = write!(out, " font-weight=\"{weight}\"");
        }
        match info.variant.style {
            FontStyle::Normal => {}
            FontStyle::Italic => out.push_str(" font-style=\"italic\""),
            FontStyle::Oblique => out.push_str(" font-style=\"oblique\""),
        }
        let list = |pick: fn(&(char, f64, f64)) -> f64| {
            chars
                .iter()
                .map(|c| num(pick(c)))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let _ = write!(out, " x=\"{}\" y=\"{}\">", list(|c| c.1), list(|c| c.2));
        let content: String = chars.iter().map(|c| c.0).collect();
        escape(&content, out);
        out.push_str("</text>");
    }

    let scale = size / text.font.units_per_em();
    for p in &placed {
        let Placed::Outline(id, gx, gy) = p else {
            continue;
        };
        let mut path = PathBuilder::default();
        if ttf.outline_glyph(GlyphId(*id), &mut path).is_some() && !path.d.is_empty() {
            let _ = write!(
                out,
                "<path transform=\"translate({} {}) scale({} {})\" d=\"{}\" {fill}/>",
                num(*gx),
                num(*gy),
                num(scale),
                num(-scale),
                path.d.trim_end(),
            );
        }
    }
}

/// The generic family that follows the named family in `font-family`.
fn generic_family(family: &str) -> &'static str {
    if family.contains("Libertinus") {
        "serif"
    } else {
        "sans-serif"
    }
}

/// A glyph outline, in font units.
#[derive(Default)]
struct PathBuilder {
    d: String,
}

impl OutlineBuilder for PathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.d, "M{} {} ", num(x.into()), num(y.into()));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.d, "L{} {} ", num(x.into()), num(y.into()));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let _ = write!(
            self.d,
            "Q{} {} {} {} ",
            num(x1.into()),
            num(y1.into()),
            num(x.into()),
            num(y.into())
        );
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let _ = write!(
            self.d,
            "C{} {} {} {} {} {} ",
            num(x1.into()),
            num(y1.into()),
            num(x2.into()),
            num(y2.into()),
            num(x.into()),
            num(y.into())
        );
    }
    fn close(&mut self) {
        self.d.push_str("Z ");
    }
}

fn write_shape(out: &mut String, pos: Point, shape: &Shape) {
    let (x, y) = (num(pos.x.to_pt()), num(pos.y.to_pt()));
    let style = shape_style(shape);
    match &shape.geometry {
        Geometry::Rect(size) => {
            let _ = write!(
                out,
                "<rect x=\"{x}\" y=\"{y}\" width=\"{}\" height=\"{}\"{style}/>",
                num(size.x.to_pt()),
                num(size.y.to_pt()),
            );
        }
        Geometry::Line(end) => {
            let _ = write!(
                out,
                "<path d=\"M{x} {y} L{} {}\"{style}/>",
                num((pos.x + end.x).to_pt()),
                num((pos.y + end.y).to_pt()),
            );
        }
        Geometry::Curve(curve) => {
            let mut d = String::new();
            let at = |p: &Point| (num((pos.x + p.x).to_pt()), num((pos.y + p.y).to_pt()));
            for item in &curve.0 {
                match item {
                    CurveItem::Move(p) => {
                        let (a, b) = at(p);
                        let _ = write!(d, "M{a} {b} ");
                    }
                    CurveItem::Line(p) => {
                        let (a, b) = at(p);
                        let _ = write!(d, "L{a} {b} ");
                    }
                    CurveItem::Cubic(p1, p2, p3) => {
                        let ((a, b), (c, e), (f, g)) = (at(p1), at(p2), at(p3));
                        let _ = write!(d, "C{a} {b} {c} {e} {f} {g} ");
                    }
                    CurveItem::Close => d.push_str("Z "),
                }
            }
            let _ = write!(out, "<path d=\"{}\"{style}/>", d.trim_end());
        }
    }
}

/// The fill and stroke attributes of a shape (preceded by a space).
fn shape_style(shape: &Shape) -> String {
    let mut attrs = String::new();
    match &shape.fill {
        Some(paint) => {
            let _ = write!(attrs, " {}", paint_attrs(paint, "fill"));
            if shape.fill_rule == FillRule::EvenOdd {
                attrs.push_str(" fill-rule=\"evenodd\"");
            }
        }
        None => attrs.push_str(" fill=\"none\""),
    }
    if let Some(stroke) = &shape.stroke {
        stroke_attrs(&mut attrs, stroke);
    }
    attrs
}

fn stroke_attrs(attrs: &mut String, stroke: &FixedStroke) {
    let _ = write!(
        attrs,
        " {} stroke-width=\"{}\"",
        paint_attrs(&stroke.paint, "stroke"),
        num(stroke.thickness.to_pt())
    );
    let cap = match stroke.cap {
        typst::visualize::LineCap::Butt => None,
        typst::visualize::LineCap::Round => Some("round"),
        typst::visualize::LineCap::Square => Some("square"),
    };
    if let Some(cap) = cap {
        let _ = write!(attrs, " stroke-linecap=\"{cap}\"");
    }
    let join = match stroke.join {
        typst::visualize::LineJoin::Miter => None,
        typst::visualize::LineJoin::Round => Some("round"),
        typst::visualize::LineJoin::Bevel => Some("bevel"),
    };
    if let Some(join) = join {
        let _ = write!(attrs, " stroke-linejoin=\"{join}\"");
    }
}
