//! Émetteur SVG « texte » d'une page Typst (#162).
//!
//! `typst-svg` écrit **toujours** ses glyphes en contours (`<use>` de
//! `<symbol>` dans `<defs>`) : aucune option ne produit du `<text>`. Pour
//! qu'une figure exportée reste retouchable (sélectionnable, cherchable,
//! éditable dans Inkscape ou Illustrator), ce module parcourt le `Frame`
//! compilé et écrit lui-même le fragment SVG :
//!
//! - un **groupe** garde sa translation et sa transformation ;
//! - un **texte** devient un `<text>` dont chaque caractère porte la position
//!   (`x`, `y`) que Typst a calculée pour son glyphe, avec la famille, le
//!   corps, le gras/l'italique et la couleur de la police du texte ;
//! - une **forme** (trait de fraction, racine, rectangle) devient un `<path>`
//!   ou un `<rect>`, comme `typst-svg` ;
//! - une image, un lien et une balise d'introspection n'ont pas de rendu ici.
//!
//! # Repli en contour, par glyphe
//!
//! Un glyphe n'est écrit en texte que si son caractère retrouve **exactement**
//! ce glyphe par la table `cmap` de la police. Un glyphe sans caractère
//! Unicode (délimiteur étiré, variante mathématique), une ligature ou un
//! caractère que la police ne lie pas à ce glyphe sort en `<path>` — le contour
//! de ce seul glyphe — pour que le rendu reste celui de Typst. Les espaces
//! n'ont pas d'encre : ils sont omis, ce qui évite aussi qu'un lecteur ne
//! replie les blancs et ne décale les positions suivantes.
//!
//! La police n'est **jamais** embarquée dans le SVG : `font-family` nomme la
//! famille, suivie d'une famille générique de repli (décision du #162).

use std::fmt::Write;

use ttf_parser::{GlyphId, OutlineBuilder};
use typst::layout::{Frame, FrameItem, Point};
use typst::text::{FontStyle, TextItem};
use typst::visualize::{Color, CurveItem, FillRule, FixedStroke, Geometry, Paint, Shape};

/// Le fragment SVG de `frame` : tout ce qui irait dans un `<svg>`, sans sa
/// balise racine.
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

/// Un nombre SVG court : pas de traîne de zéros, jamais `NaN` ni `inf`.
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

/// `#RRGGBB` et opacité d'une peinture. Un dégradé ou un motif (jamais produits
/// par les textes du graphe) retombent sur le noir.
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

/// Ce que devient un glyphe : un caractère de `<text>`, ou un contour.
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
        // Typst compte `y_offset` vers le haut, le SVG vers le bas.
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
            // Pas d'encre : omis (voir la doc de tête).
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

/// La famille générique qui suit la famille nommée dans `font-family`.
fn generic_family(family: &str) -> &'static str {
    if family.contains("Libertinus") {
        "serif"
    } else {
        "sans-serif"
    }
}

/// Un contour de glyphe, en unités de la police.
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

/// Les attributs de remplissage et de trait d'une forme (précédés d'un blanc).
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
