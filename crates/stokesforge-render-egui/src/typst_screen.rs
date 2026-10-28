//! Typst screen path — task R8.3.
//!
//! What `stokesforge-render` (feature `typst`) could not carry itself
//! without pulling `egui`/`typst-render` into a crate that does not need
//! them (`architecture-choisie.md` §8): the rasterization of a compiled page
//! to RGBA, its loading as an `egui` texture, and the construction of a
//! `Shape::Mesh` positioned exactly as an ordinary `Shape::Text` would have
//! been.
//!
//! # Degradation, not error
//!
//! A formula that does not compile (invalid Typst syntax) does not make the
//! rendering fail: [`TypstScreen::measure`] and [`TypstScreen::render`]
//! return `None`, and the caller ([`crate::backend::EguiBackend`]) falls
//! back on the ordinary path — the plain text, `$...$` included, is shown
//! as is rather than making the whole graph disappear. The same policy as
//! broken series references or unknown plot types
//! (`architecture-choisie.md` §11.1): a visible, localized defect, never a
//! panic that bubbles up.

use std::sync::Arc;

use egui::{Color32, ColorImage, TextureHandle, TextureOptions};
use plotters_backend::{text_anchor::Pos, FontTransform};
use stokesforge_render::formula::{self, Compiled, FormulaCache, Ink};
use stokesforge_render::FontChoice;
use typst::text::Font;
use typst::utils::Scalar;
use typst_render::RenderOptions;

/// A formula already rasterized for the screen.
///
/// `size_pt` is the **logical** size (the same frame as the rest of the
/// text, measured at the fixed scale of [`stokesforge_render::text`]) — not
/// the size in pixels of the texture, which grows with `pixels_per_point` to
/// stay sharp on a HiDPI screen, exactly as a `Galley` stays anchored to an
/// unchanged logical size while its atlas is laid out more finely
/// (`stokesforge_render::text`, module header).
///
/// It also keeps the compiled document: [`Self::rerender`] draws an image at
/// another resolution from it without recompiling, when the graph is
/// enlarged (`crate::scaled`, issue #31).
pub struct FormulaTexture {
    handle: TextureHandle,
    size_pt: (f64, f64),
    /// The `pixels_per_point` **requested** for this image (the effective
    /// one can be lower, see [`rasterize`]).
    pixels_per_point: f32,
    /// `None` for an empty formula, which has nothing to rasterize.
    compiled: Option<Compiled>,
    text: String,
}

impl FormulaTexture {
    /// The egui texture the mesh points to.
    pub fn id(&self) -> egui::TextureId {
        self.handle.id()
    }

    /// The size of the image, in pixels.
    pub fn size_pixels(&self) -> [usize; 2] {
        self.handle.size()
    }

    /// The `pixels_per_point` requested from the rasterization.
    pub fn pixels_per_point(&self) -> f32 {
        self.pixels_per_point
    }

    /// The same formula, rasterized again at `pixels_per_point` — without
    /// recompiling: same document, so same size in points and same box, only
    /// the sharpness of the image changes (issue #31). `None` for an empty
    /// formula.
    pub fn rerender(&self, context: &egui::Context, pixels_per_point: f32) -> Option<Self> {
        let compiled = self.compiled.as_ref()?;
        Some(rasterize(context, compiled, pixels_per_point, &self.text))
    }
}

/// What is needed to draw a Typst label on screen — built once per
/// [`crate::GraphView`], reused at every frame.
pub struct TypstScreen<'a> {
    context: &'a egui::Context,
    cache: &'a FormulaCache<FormulaTexture>,
    choice: &'a FontChoice,
    sans: Font,
    math: Font,
}

impl<'a> TypstScreen<'a> {
    /// Resolves the fonts once (`choice`) and assembles the drawing context.
    /// To be rebuilt only if `choice` changes — like
    /// [`stokesforge_render::text::TextMeasurer`], not at every frame.
    pub fn new(
        context: &'a egui::Context,
        cache: &'a FormulaCache<FormulaTexture>,
        choice: &'a FontChoice,
    ) -> Result<Self, String> {
        let fonts = formula::resolve_fonts(choice)?;
        Ok(Self::with_fonts(context, cache, choice, fonts))
    }

    /// Like [`Self::new`], on already resolved fonts
    /// (`formula::resolve_fonts(choice)`) — for a caller that builds this
    /// context at each rebuild of the figure ([`crate::GraphView`]) and does
    /// not want to decode two fonts again each time. `fonts` must come from
    /// `choice`: the cache key carries `choice`, not the fonts themselves.
    pub fn with_fonts(
        context: &'a egui::Context,
        cache: &'a FormulaCache<FormulaTexture>,
        choice: &'a FontChoice,
        (sans, math): (Font, Font),
    ) -> Self {
        Self {
            context,
            cache,
            choice,
            sans,
            math,
        }
    }

    /// The real `pixels_per_point` of the `Context` — the one at which
    /// [`Self::render`] must rasterize to stay sharp on a HiDPI screen, not
    /// to be confused with the fixed scale used by [`Self::measure`].
    pub fn pixels_per_point(&self) -> f32 {
        self.context.pixels_per_point()
    }

    /// The size `text` would occupy compiled by Typst, at `size_pt` of body
    /// — the invariant of task 8.5. `None` if the formula does not compile.
    ///
    /// `ink` does not change the size; it is requested so that the
    /// measurement lands on the entry the drawing reuses. It goes through
    /// the **same** cache and the **same** entry as [`Self::render`] (same
    /// text, same body, same color, same `pixels_per_point`): Plotters
    /// measures a text just before drawing it, so the rasterization done
    /// here is the one the drawing reuses. Before issue #29, each
    /// measurement recompiled and resolved the fonts again. The returned
    /// size is that of the compiled page, in points — independent of the
    /// `pixels_per_point` of the texture, rounded by the same function as
    /// [`formula::measure`].
    pub fn measure(&self, text: &str, size_pt: f32, ink: Ink) -> Option<(u32, u32)> {
        let texture = self.render(text, size_pt, ink, self.pixels_per_point())?;
        Some(formula::round_size_pt(texture.size_pt))
    }

    /// The texture (compiled and rasterized, or taken from the cache) for
    /// `text` at `size_pt` of body, in `ink` (issue #30), at the requested
    /// `pixels_per_point`. `None` if the formula does not compile.
    pub fn render(
        &self,
        text: &str,
        size_pt: f32,
        ink: Ink,
        pixels_per_point: f32,
    ) -> Option<Arc<FormulaTexture>> {
        let context = self.context;
        let owned_text = text.to_owned();
        self.cache
            .get_or_try_insert(
                text,
                size_pt,
                ink,
                self.choice,
                pixels_per_point,
                self.sans.clone(),
                self.math.clone(),
                move |compiled| Ok(rasterize(context, compiled, pixels_per_point, &owned_text)),
            )
            .ok()
    }
}

/// Rasterizes a compiled page into an `egui` texture — task 8.3.
///
/// The image never exceeds the `Context`'s `max_texture_side`: a title
/// enlarged on a full-screen HiDPI display (issue #31) can ask for more than
/// the GPU accepts. Beyond that, the resolution is capped and the image a
/// bit stretched, rather than a texture being refused.
fn rasterize(
    context: &egui::Context,
    compiled: &Compiled,
    pixels_per_point: f32,
    text: &str,
) -> FormulaTexture {
    let size_pt = compiled.size_pt();
    let Some(page) = compiled.page() else {
        // Empty formula: a transparent 1×1 texture rather than one more
        // special case to handle downstream — a `Shape::Mesh` on a
        // zero-size rectangle simply draws nothing.
        let image = ColorImage::new([1, 1], vec![Color32::TRANSPARENT]);
        let handle = context.load_texture("stokesforge-typst-empty", image, TextureOptions::LINEAR);
        return FormulaTexture {
            handle,
            size_pt: (0.0, 0.0),
            pixels_per_point,
            compiled: None,
            text: text.to_owned(),
        };
    };
    let max_side = context.input(|input| input.max_texture_side) as f64;
    let longest_pt = size_pt.0.max(size_pt.1).max(1.0);
    let effective = (pixels_per_point as f64).min((max_side - 2.0).max(1.0) / longest_pt);
    let opts = RenderOptions {
        pixel_per_pt: Scalar::new(effective),
        render_bleed: false,
    };
    let pixmap = typst_render::render(page, &opts);
    let image = ColorImage::from_rgba_premultiplied(
        [pixmap.width() as usize, pixmap.height() as usize],
        pixmap.data(),
    );
    // A name that is stable per content rather than a counter: two identical
    // labels reuse the same entry of egui's texture atlas instead of
    // allocating one per occurrence.
    let handle = context.load_texture(
        format!("stokesforge-typst:{text}"),
        image,
        TextureOptions::LINEAR,
    );
    FormulaTexture {
        handle,
        size_pt,
        pixels_per_point,
        compiled: Some(compiled.clone()),
        text: text.to_owned(),
    }
}

/// Builds the `Shape::Mesh` of an already rasterized formula, anchored and
/// rotated exactly as a `Shape::Text` of the same box would have been — same
/// computation as `EguiBackend::draw_text`, through the shared functions of
/// `stokesforge_render::formula`.
pub fn mesh(
    texture: &FormulaTexture,
    screen_origin: egui::Pos2,
    anchor: Pos,
    transform: FontTransform,
) -> egui::Shape {
    let offset = formula::anchor_offset(texture.size_pt, anchor.h_pos, anchor.v_pos);
    let offset = formula::rotate_offset(offset, transform.clone());
    let origin = screen_origin + egui::vec2(offset.0 as f32, offset.1 as f32);
    let (w, h) = (texture.size_pt.0 as f32, texture.size_pt.1 as f32);

    // The four corners of the **unrotated** rectangle, anchored at `origin` —
    // the same frame as `TextShape::pos` (the top-left corner of the
    // unrotated text, `epaint::shapes::text_shape`).
    //
    // **In the exact order in which `Mesh::add_rect_with_uv` places its
    // vertices**: top-left, top-right, bottom-left, bottom-right (epaint
    // 0.36, `mesh.rs`). Each vertex keeps the texture coordinate epaint gave
    // it; a "going around" order (…, bottom-right, bottom-left) swapped the
    // position of the last two without swapping their texture and sheared
    // the image into unreadable noise, in a box that was nevertheless right
    // — an R8 defect that stayed invisible as long as the application did
    // not display Typst, seen at the first capture of issue #29 and held by
    // `every_mesh_vertex_carries_the_texture_of_its_corner`.
    let corners = [
        egui::vec2(0.0, 0.0),
        egui::vec2(w, 0.0),
        egui::vec2(0.0, h),
        egui::vec2(w, h),
    ];
    let angle = formula::rotation_degrees(transform).to_radians() as f32;
    let (sin, cos) = angle.sin_cos();
    let rotate = |v: egui::Vec2| egui::vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos);
    let points: Vec<egui::Pos2> = corners.iter().map(|&c| origin + rotate(c)).collect();

    let mut m = egui::epaint::Mesh::with_texture(texture.handle.id());
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    m.add_rect_with_uv(
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1.0, 1.0)),
        uv,
        Color32::WHITE,
    );
    // `add_rect_with_uv` builds an **axis-aligned** rectangle; its four
    // vertices are replaced by the rotated corners computed above rather
    // than reproducing the triangulation by hand.
    for (vertex, point) in m.vertices.iter_mut().zip(points.iter()) {
        vertex.pos = *point;
    }
    egui::Shape::Mesh(Arc::new(m))
}

/// The environment of the interface formula labels (#204): a texture cache
/// and the fonts, resolved once and kept in egui's memory — neither
/// `GraphView` nor a figure takes part in it.
struct LabelEnv {
    cache: FormulaCache<FormulaTexture>,
    choice: FontChoice,
    fonts: Option<(Font, Font)>,
}

/// Cap on the formulas kept: a window shows only a few of them, the cache
/// only serves to avoid recompiling at every frame.
const LABEL_CACHE_LIMIT: usize = 64;

/// Paints `source` (Typst, often `$ … $`) as an interface label at `size_pt`
/// of body and in `color`, and returns its response. `None` if the formula
/// does not compile or no font is available: the caller then shows nothing
/// (a help formula is never essential).
pub fn formula_label(
    ui: &mut egui::Ui,
    source: &str,
    size_pt: f32,
    color: Color32,
) -> Option<egui::Response> {
    let id = egui::Id::new("stokesforge-formula-label-env");
    let env: Arc<LabelEnv> = match ui.data(|d| d.get_temp::<Arc<LabelEnv>>(id)) {
        Some(env) => env,
        None => {
            let choice = FontChoice::default();
            let env = Arc::new(LabelEnv {
                fonts: formula::resolve_fonts(&choice).ok(),
                cache: FormulaCache::new(),
                choice,
            });
            ui.data_mut(|d| d.insert_temp(id, Arc::clone(&env)));
            env
        }
    };
    if env.cache.len() > LABEL_CACHE_LIMIT {
        env.cache.clear();
    }
    let fonts = env.fonts.clone()?;
    let screen = TypstScreen::with_fonts(ui.ctx(), &env.cache, &env.choice, fonts);
    let ink: Ink = color.to_array();
    let texture = screen.render(source, size_pt, ink, screen.pixels_per_point())?;
    let (width, height) = texture.size_pt;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width as f32, height as f32),
        egui::Sense::hover(),
    );
    ui.painter().add(mesh(
        &texture,
        rect.min,
        Pos::new(
            plotters_backend::text_anchor::HPos::Left,
            plotters_backend::text_anchor::VPos::Top,
        ),
        FontTransform::None,
    ));
    Some(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_render::FontChoice;

    fn context() -> egui::Context {
        egui::Context::default()
    }

    const BLACK: Ink = [0, 0, 0, 255];

    /// A trivial formula measures a non-zero size — the minimal sanity test
    /// before any drawing.
    #[test]
    fn measuring_a_trivial_formula_returns_a_non_zero_size() {
        let ctx = context();
        let cache = FormulaCache::new();
        let choice = FontChoice::default();
        let screen = TypstScreen::new(&ctx, &cache, &choice).expect("font resolution failed");
        let (w, h) = screen
            .measure("$lambda$", 14.0, BLACK)
            .expect("the formula should have compiled");
        assert!(w > 0 && h > 0, "zero size: ({w}, {h})");
    }

    /// An invalid formula degrades to `None`, without panicking — the policy
    /// documented in the module header.
    #[test]
    fn an_invalid_formula_measures_none() {
        let ctx = context();
        let cache = FormulaCache::new();
        let choice = FontChoice::default();
        let screen = TypstScreen::new(&ctx, &cache, &choice).expect("font resolution failed");
        assert!(screen.measure("$ (unclosed", 14.0, BLACK).is_none());
    }

    /// Rendering the same formula twice at the same `pixels_per_point` serves
    /// the same texture from the cache — task 8.2, screen side.
    #[test]
    fn rendering_the_same_formula_twice_reuses_the_cache() {
        let ctx = context();
        let cache = FormulaCache::new();
        let choice = FontChoice::default();
        let screen = TypstScreen::new(&ctx, &cache, &choice).expect("font resolution failed");
        let a = screen
            .render("$lambda$", 14.0, BLACK, 1.0)
            .expect("rendering failed");
        let b = screen
            .render("$lambda$", 14.0, BLACK, 1.0)
            .expect("rendering failed");
        assert_eq!(a.handle.id(), b.handle.id());
        assert_eq!(cache.len(), 1);
    }

    /// Two colors of the same text are two textures (issue #30): the color is
    /// in the rasterized image, not in a tint applied afterwards, so it must
    /// be in the cache key.
    #[test]
    fn two_colors_occupy_two_entries() {
        let ctx = context();
        let cache = FormulaCache::new();
        let choice = FontChoice::default();
        let screen = TypstScreen::new(&ctx, &cache, &choice).expect("font resolution failed");
        let black = screen
            .render("$lambda$", 14.0, BLACK, 1.0)
            .expect("rendering");
        let white = screen
            .render("$lambda$", 14.0, [255, 255, 255, 255], 1.0)
            .expect("rendering");
        assert_eq!(cache.len(), 2);
        assert_ne!(black.handle.id(), white.handle.id());
        assert_eq!(black.size_pt, white.size_pt, "the color changes the size");
    }

    /// Measurement goes through the cache, and through the same entry as the
    /// drawing that follows it: measuring then drawing a formula compiles
    /// only once (issue #29 — before, each measurement recompiled).
    #[test]
    fn measuring_then_drawing_shares_a_single_entry() {
        let ctx = context();
        let cache = FormulaCache::new();
        let choice = FontChoice::default();
        let screen = TypstScreen::new(&ctx, &cache, &choice).expect("font resolution failed");
        let measured = screen
            .measure("$lambda$", 14.0, BLACK)
            .expect("measurement");
        let texture = screen
            .render("$lambda$", 14.0, BLACK, screen.pixels_per_point())
            .expect("rendering");
        assert_eq!(cache.len(), 1);
        assert_eq!(measured, formula::round_size_pt(texture.size_pt));
    }

    /// Each mesh vertex carries the texture coordinate of **its** corner:
    /// u = 0 at the left edge, u = 1 at the right edge, v = 0 at the top,
    /// v = 1 at the bottom. The bounding box alone does not prove it — a mesh
    /// with swapped vertices has the same box and shows unreadable noise
    /// (the defect seen on screen during issue #29).
    #[test]
    fn every_mesh_vertex_carries_the_texture_of_its_corner() {
        let ctx = context();
        let cache = FormulaCache::new();
        let choice = FontChoice::default();
        let screen = TypstScreen::new(&ctx, &cache, &choice).expect("font resolution failed");
        let texture = screen
            .render("$lambda$", 14.0, BLACK, 1.0)
            .expect("rendering failed");
        let anchor = Pos::new(
            plotters_backend::text_anchor::HPos::Left,
            plotters_backend::text_anchor::VPos::Top,
        );
        let egui::Shape::Mesh(m) = mesh(
            &texture,
            egui::pos2(10.0, 20.0),
            anchor,
            FontTransform::None,
        ) else {
            panic!("a mesh was expected")
        };
        let bounds = m.calc_bounds();
        for vertex in &m.vertices {
            let u = (vertex.pos.x - bounds.min.x) / bounds.width();
            let v = (vertex.pos.y - bounds.min.y) / bounds.height();
            assert!(
                (u - vertex.uv.x).abs() < 1e-4 && (v - vertex.uv.y).abs() < 1e-4,
                "vertex at ({u}, {v}) of the rectangle but texture at {:?}",
                vertex.uv
            );
        }
    }

    /// The produced mesh occupies a box of the right size, anchored at the
    /// expected origin for a `Left`/`Top` anchor without rotation — the
    /// simplest case, which would have revealed an inverted offset sign.
    #[test]
    fn the_mesh_occupies_the_texture_size_without_rotation() {
        let ctx = context();
        let cache = FormulaCache::new();
        let choice = FontChoice::default();
        let screen = TypstScreen::new(&ctx, &cache, &choice).expect("font resolution failed");
        let texture = screen
            .render("$lambda$", 14.0, BLACK, 1.0)
            .expect("rendering failed");
        let anchor = Pos::new(
            plotters_backend::text_anchor::HPos::Left,
            plotters_backend::text_anchor::VPos::Top,
        );
        let shape = mesh(
            &texture,
            egui::pos2(10.0, 20.0),
            anchor,
            FontTransform::None,
        );
        let egui::Shape::Mesh(m) = shape else {
            panic!("a mesh was expected")
        };
        let bounds = m.calc_bounds();
        assert!((bounds.min.x - 10.0).abs() < 0.01);
        assert!((bounds.min.y - 20.0).abs() < 0.01);
        assert!((bounds.width() - texture.size_pt.0 as f32).abs() < 0.01);
        assert!((bounds.height() - texture.size_pt.1 as f32).abs() < 0.01);
    }

    /// An interface formula label allocates the box of its formula and paints
    /// a textured mesh; an invalid source paints nothing (#204).
    #[test]
    fn a_formula_label_paints_a_mesh_and_reports_its_failure() {
        let ctx = context();
        let mut drawn = None;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            drawn = formula_label(ui, "$ y = a + b x $", 14.0, Color32::BLACK).map(|r| r.rect);
        });
        output.textures_delta.clear();
        let rect = drawn.expect("a valid formula paints");
        assert!(rect.width() > 10.0 && rect.height() > 5.0, "{rect:?}");
        let meshes = output
            .shapes
            .iter()
            .filter(|s| matches!(s.shape, egui::Shape::Mesh(_)))
            .count();
        assert_eq!(meshes, 1);

        let mut invalid_painted = true;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            invalid_painted =
                formula_label(ui, "#nonexistent_function()", 14.0, Color32::BLACK).is_some();
        });
        output.textures_delta.clear();
        assert!(
            !invalid_painted,
            "witness: an invalid source paints nothing"
        );
    }
}
