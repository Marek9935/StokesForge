//! Plotters `DrawingBackend` that **collects `egui::Shape`s** — task R4.2.
//!
//! # The design decision: a `Vec<Shape>`, not a `Painter`
//!
//! This backend holds no `Painter`: it writes into a `&mut Vec<Shape>` that
//! is lent to it. This is the decision of §4.2 of `architecture-choisie.md`,
//! and it is not cosmetic. egui is in immediate mode: it repaints at every
//! mouse move. A backend that drew into a `Painter` would replay the whole
//! Plotters pipeline — `ChartBuilder`, axes, ticks, all the points — at
//! **every frame**, even a still one. With a collector, the result is cached
//! ([`crate::cache`]) and a frame without changes costs one
//! `painter.extend(...)`.
//!
//! `epaint` blesses this storage, with an exception it documents itself:
//! `Shape::Text` depends on `pixels_per_point` and must be rebuilt when it
//! changes. Hence the three-part cache key.
//!
//! Welcome side effect: `ensure_prepared` and `present` have nothing to do,
//! where a texture backend would make `present` an upload point.
//!
//! # The eight overridden methods
//!
//! `DrawingBackend` only requires `get_size`, `ensure_prepared`, `present`
//! and `draw_pixel`; everything else falls back on pixel-by-pixel
//! rasterization. For a vector backend, eight are overridden, each landing
//! on a `Shape` almost one for one (`architecture-choisie.md` §4.1):
//!
//! | plotters | egui |
//! |---|---|
//! | `draw_line` | `Shape::LineSegment` |
//! | `draw_path` | `Shape::Path` |
//! | `draw_rect` | `Shape::Rect` |
//! | `draw_circle` | `Shape::Circle` |
//! | `fill_polygon` | closed `Shape::Path` |
//! | `draw_text` | `Shape::Text` |
//! | `estimate_text_size` | [`stokesforge_render::text::estimate`] |
//! | `blit_bitmap` | **left as default** — "no image handling" |
//!
//! `draw_pixel` is implemented anyway, because the trait requires it: a 1×1
//! rectangle. Nothing R4 draws goes through it (the eight overrides cover
//! everything), but letting it panic would turn an unforeseen primitive into
//! an application crash.

use egui::epaint::{CircleShape, PathShape, RectShape, TextShape};
use egui::{Color32, CornerRadius, Pos2, Shape, Stroke, StrokeKind, Vec2};
use plotters_backend::{
    text_anchor::{HPos, VPos},
    BackendColor, BackendCoord, BackendStyle, BackendTextStyle, DrawingBackend, DrawingErrorKind,
    FontTransform,
};
use stokesforge_render::text::{estimate, font_id, TextSource};

/// The error this backend can return — none.
///
/// Collecting into a `Vec` cannot fail. The type exists because
/// `DrawingBackend::ErrorType` requires it, and it is deliberately
/// uninhabited rather than an alias of an I/O error that will never happen:
/// the signature then tells the truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EguiBackendError {}

impl std::fmt::Display for EguiBackendError {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {}
    }
}

impl std::error::Error for EguiBackendError {}

/// Plotters backend that stacks [`egui::Shape`]s.
///
/// `origin` places the top-left corner of the figure in screen coordinates:
/// Plotters always works in `0..width`, `0..height`, and the translation is
/// applied once, when each shape is built, rather than afterwards on the
/// whole vector. The cache therefore holds shapes already in place, and a
/// frame without changes has nothing at all to recompute.
pub struct EguiBackend<'a> {
    shapes: &'a mut Vec<Shape>,
    origin: Pos2,
    size: (u32, u32),
    text: &'a dyn TextSource,
    /// Typst path of the screen (task R8.3) — `None` as long as nobody has
    /// called [`Self::with_typst`]: building this backend does not enable
    /// Typst. [`crate::GraphView`] always enables it (issue #29); a backend
    /// without it serves as a witness for the tests.
    typst: Option<crate::typst_screen::TypstScreen<'a>>,
}

impl<'a> EguiBackend<'a> {
    /// A backend that writes into `shapes`, offset by `origin`.
    pub fn new(
        shapes: &'a mut Vec<Shape>,
        origin: Pos2,
        size: (u32, u32),
        text: &'a dyn TextSource,
    ) -> Self {
        Self {
            shapes,
            origin,
            size,
            text,
            typst: None,
        }
    }

    /// A backend that exactly fills the given rectangle.
    ///
    /// The dimensions are rounded **down**: Plotters reasons in integers and
    /// a figure one pixel too big would overflow the rectangle allocated by
    /// the UI.
    pub fn in_rect(shapes: &'a mut Vec<Shape>, rect: egui::Rect, text: &'a dyn TextSource) -> Self {
        let size = (rect.width().max(1.0) as u32, rect.height().max(1.0) as u32);
        Self::new(shapes, rect.min, size, text)
    }

    /// Enables the Typst path (task R8.3) on this backend: texts in rich
    /// style ([`stokesforge_render::formula::is_rich`] — titles, axis
    /// titles, legend, issue #32) go through `typst`, ticks stay as
    /// `Shape::Text`. Additive: existing callers (`in_rect`, `new`) are
    /// unchanged, and a backend built without this call draws everything as
    /// plain text, as before R8.
    pub fn with_typst(mut self, typst: crate::typst_screen::TypstScreen<'a>) -> Self {
        self.typst = Some(typst);
        self
    }

    /// Converts a Plotters point to screen coordinates.
    fn at(&self, (x, y): BackendCoord) -> Pos2 {
        Pos2::new(self.origin.x + x as f32, self.origin.y + y as f32)
    }

    /// The stroke of a shape, or [`Stroke::NONE`] if the style is transparent.
    fn stroke<S: BackendStyle>(style: &S) -> Stroke {
        Stroke {
            width: style.stroke_width().max(1) as f32,
            color: color(style.color()),
        }
    }
}

/// A Plotters color as an [`egui::Color32`].
///
/// `from_rgba_unmultiplied` and not `from_rgba_premultiplied`: Plotters'
/// alpha is an opacity independent of the components.
fn color(c: BackendColor) -> Color32 {
    let (r, g, b) = c.rgb;
    let alpha = (c.alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

/// The angle, in clockwise radians, of a Plotters rotation.
///
/// `FontTransform` is only 0/90/180/270 while `TextShape::angle` is a free
/// `f32`: the conversion is trivial, and it is one of the reasons why
/// writing this backend costs less than importing one
/// (`architecture-choisie.md` §4.1).
fn angle_of(transform: &FontTransform) -> f32 {
    match transform {
        FontTransform::None => 0.0,
        FontTransform::Rotate90 => std::f32::consts::FRAC_PI_2,
        FontTransform::Rotate180 => std::f32::consts::PI,
        FontTransform::Rotate270 => 3.0 * std::f32::consts::FRAC_PI_2,
    }
}

/// Applies to an offset the same rotation that [`FontTransform::transform`]
/// applies to the glyphs.
///
/// Needed so that the anchor (`HPos`/`VPos`) stays right on a rotated text:
/// Plotters rotates the anchor offset together with the text, and not doing
/// so would put the left axis title beside its place. Transcribed from the
/// default implementation of `draw_text`, not deduced.
fn rotate(offset: Vec2, transform: &FontTransform) -> Vec2 {
    match transform {
        FontTransform::None => offset,
        FontTransform::Rotate90 => Vec2::new(-offset.y, offset.x),
        FontTransform::Rotate180 => Vec2::new(-offset.x, -offset.y),
        FontTransform::Rotate270 => Vec2::new(offset.y, -offset.x),
    }
}

/// Color of the frame of a rich text that Typst rejects (#240, T3).
const COMPILE_ERROR_FRAME: Color32 = Color32::from_rgb(214, 48, 49);

impl DrawingBackend for EguiBackend<'_> {
    type ErrorType = EguiBackendError;

    fn get_size(&self) -> (u32, u32) {
        self.size
    }

    /// Nothing to prepare: there is no buffer to allocate, no graphics
    /// context to open. See the top-level doc.
    fn ensure_prepared(&mut self) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        Ok(())
    }

    /// Nothing to present: the shapes are already in the caller's vector,
    /// which will decide when to paint them.
    fn present(&mut self) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        Ok(())
    }

    fn draw_pixel(
        &mut self,
        point: BackendCoord,
        c: BackendColor,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        let min = self.at(point);
        self.shapes.push(Shape::rect_filled(
            egui::Rect::from_min_size(min, Vec2::splat(1.0)),
            CornerRadius::ZERO,
            color(c),
        ));
        Ok(())
    }

    fn draw_line<S: BackendStyle>(
        &mut self,
        from: BackendCoord,
        to: BackendCoord,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        if style.color().alpha == 0.0 {
            return Ok(());
        }
        let points = [self.at(from), self.at(to)];
        self.shapes.push(Shape::LineSegment {
            points,
            stroke: Self::stroke(style),
        });
        Ok(())
    }

    fn draw_rect<S: BackendStyle>(
        &mut self,
        upper_left: BackendCoord,
        bottom_right: BackendCoord,
        style: &S,
        fill: bool,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        if style.color().alpha == 0.0 {
            return Ok(());
        }
        // `from_two_pos` rather than `from_min_max`: Plotters does not
        // guarantee that the first corner is the smaller one (an inverted
        // axis can make a rectangle "upside down"), and a degenerate egui
        // `Rect` is not painted.
        let rect = egui::Rect::from_two_pos(self.at(upper_left), self.at(bottom_right));
        let shape = if fill {
            RectShape::filled(rect, CornerRadius::ZERO, color(style.color()))
        } else {
            RectShape::stroke(
                rect,
                CornerRadius::ZERO,
                Self::stroke(style),
                StrokeKind::Inside,
            )
        };
        self.shapes.push(Shape::Rect(shape));
        Ok(())
    }

    /// The hot method: the series go through it.
    ///
    /// A single `Shape::Path` for the whole polyline, instead of the N
    /// `LineSegment`s the default implementation would produce: this is what
    /// makes the cache worthwhile, and what the R0 0.3 measurement
    /// quantified (300,000 points = 2.432 MB of vector cache, independent of
    /// resolution).
    fn draw_path<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        path: I,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        if style.color().alpha == 0.0 {
            return Ok(());
        }
        // A subpixel path (`stokesforge_render::subpixel`, #91) arrives in
        // fixed points with a flag in the width.
        let (width, fixed) = stokesforge_render::subpixel::split_width(style.stroke_width());
        let points: Vec<Pos2> = path
            .into_iter()
            .map(|point| {
                if fixed {
                    let (x, y) = stokesforge_render::subpixel::decode(point);
                    Pos2::new(self.origin.x + x as f32, self.origin.y + y as f32)
                } else {
                    self.at(point)
                }
            })
            .collect();
        if points.len() < 2 {
            return Ok(());
        }
        let stroke = Stroke {
            width: width.max(1) as f32,
            color: color(style.color()),
        };
        self.shapes
            .push(Shape::Path(PathShape::line(points, stroke)));
        Ok(())
    }

    fn draw_circle<S: BackendStyle>(
        &mut self,
        center: BackendCoord,
        radius: u32,
        style: &S,
        fill: bool,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        if style.color().alpha == 0.0 {
            return Ok(());
        }
        let center = self.at(center);
        let radius = radius as f32;
        let shape = if fill {
            CircleShape::filled(center, radius, color(style.color()))
        } else {
            CircleShape::stroke(center, radius, Self::stroke(style))
        };
        self.shapes.push(Shape::Circle(shape));
        Ok(())
    }

    /// Filled polygon.
    ///
    /// `PathShape::convex_polygon`: egui can only fill convex polygons, and a
    /// concave polygon would render wrong **without an error**. This is not a
    /// risk in R4 — with `draw_path` overridden, Plotters no longer calls
    /// this method to thicken a stroke, and nothing else in the minimal graph
    /// has a polygon — but it will become one in R5 with bars and areas.
    /// **To be revisited then**: triangulate by hand, or go through a `Mesh`.
    fn fill_polygon<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        vert: I,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        if style.color().alpha == 0.0 {
            return Ok(());
        }
        let points: Vec<Pos2> = vert.into_iter().map(|point| self.at(point)).collect();
        if points.len() < 3 {
            return Ok(());
        }
        self.shapes.push(Shape::Path(PathShape::convex_polygon(
            points,
            color(style.color()),
            Stroke::NONE,
        )));
        Ok(())
    }

    /// Text as `Shape::Text`, or through Typst for a rich text.
    ///
    /// **Ticks** never go through Typst: it would be one compilation per tick
    /// (`architecture-choisie.md` §4.3). Titles, axis titles and legend do
    /// (issue #32) — the pipeline marks them by their style. For an ordinary
    /// text, the `Galley` obtained here is the very one that
    /// [`DrawingBackend::estimate_text_size`] measures, below: measurement
    /// and drawing are not "consistent", they are the same object.
    fn draw_text<S: BackendTextStyle>(
        &mut self,
        text: &str,
        style: &S,
        pos: BackendCoord,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        let fill = style.color();
        if fill.alpha == 0.0 {
            return Ok(());
        }

        // A rich text (`stokesforge_render::formula::is_rich`, issue #32)
        // goes through the Typst texture rather than the egui `Galley`.
        // It silently degrades to the ordinary path if it does not compile
        // or if Typst is not enabled on this backend (`typst_screen`, module
        // header) — never an error reported for a malformed label.
        // A rich text that Typst rejects is drawn as plain text **in a red
        // frame** (#240, T3): a screen marker, absent from exports (SVG
        // backend).
        let mut compile_failed = false;
        if let Some(typst) = &self.typst {
            if stokesforge_render::formula::is_rich(style) {
                let size_pt = stokesforge_render::text::font_size(style.size());
                let ink = stokesforge_render::formula::ink(fill);
                let pixels_per_point = typst.pixels_per_point();
                let source = stokesforge_render::formula::source(text, style);
                if let Some(texture) = typst.render(&source, size_pt, ink, pixels_per_point) {
                    let origin = self.at(pos);
                    let shape = crate::typst_screen::mesh(
                        &texture,
                        origin,
                        style.anchor(),
                        style.transform(),
                    );
                    self.shapes.push(shape);
                    return Ok(());
                }
                compile_failed = !text.trim().is_empty();
            }
        }

        let galley = self.text.layout(text, font_id(style), color(fill));
        let size = galley.rect.size();
        let anchor = style.anchor();
        let offset = Vec2::new(
            match anchor.h_pos {
                HPos::Left => 0.0,
                HPos::Center => -size.x / 2.0,
                HPos::Right => -size.x,
            },
            match anchor.v_pos {
                VPos::Top => 0.0,
                VPos::Center => -size.y / 2.0,
                VPos::Bottom => -size.y,
            },
        );
        let transform = style.transform();
        let origin = self.at(pos) + rotate(offset, &transform);
        if compile_failed {
            // The frame follows the rotation of the text (vertical axis title).
            let pad = 2.0;
            let corners = [
                (-pad, -pad),
                (size.x + pad, -pad),
                (size.x + pad, size.y + pad),
                (-pad, size.y + pad),
            ]
            .map(|(x, y)| origin + rotate(Vec2::new(x, y), &transform));
            self.shapes.push(Shape::closed_line(
                corners.to_vec(),
                Stroke::new(1.5, COMPILE_ERROR_FRAME),
            ));
        }
        self.shapes.push(Shape::Text(
            TextShape::new(origin, galley, color(fill)).with_angle(angle_of(&transform)),
        ));
        Ok(())
    }

    /// Measurement — task R4.1, and the same call as in
    /// `stokesforge-render-svg`.
    fn estimate_text_size<S: BackendTextStyle>(
        &self,
        text: &str,
        style: &S,
    ) -> Result<(u32, u32), DrawingErrorKind<Self::ErrorType>> {
        // R8.5 — **the invariant of the work package**: a rich text is
        // measured by querying Typst, never by the `epaint` heuristic (which
        // would measure the source text itself, not what Typst composes from
        // it). The same condition as `draw_text`, at the same place.
        // It degrades to the `epaint` measurement if Typst is not enabled or
        // the formula does not compile — the same policy as `draw_text`, so
        // that measurement and drawing stay in agreement on what they do
        // (`architecture-choisie.md` §4.3).
        if let Some(typst) = &self.typst {
            if stokesforge_render::formula::is_rich(style) {
                let size_pt = stokesforge_render::text::font_size(style.size());
                let ink = stokesforge_render::formula::ink(style.color());
                let source = stokesforge_render::formula::source(text, style);
                if let Some(size) = typst.measure(&source, size_pt, ink) {
                    return Ok(size);
                }
            }
        }
        Ok(estimate(self.text, text, style))
    }

    // `blit_bitmap` is left as default: the specification says "no image
    // handling" (`architecture-choisie.md` §2). The trait default rasterizes
    // it pixel by pixel, which is slow and correct — the right way to treat
    // what must not happen.
}
