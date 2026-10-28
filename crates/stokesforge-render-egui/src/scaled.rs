//! Scaling of cached shapes without blurring the text (issue #18).
//!
//! epaint's `Shape::transform` scales the mesh of a `Shape::Text`: the glyphs
//! stay the ones rasterized at the **logical** size, so an enlarged graph
//! shows a stretched, blurry bitmap. Here the text is laid out again at the
//! **displayed** size (font × scale) so that the atlas rasterizes at the
//! right resolution; the positions stay those of the logical layout (text
//! measurement, invariant 4, is not touched: the pipeline already decided
//! everything before this step).
//!
//! # Typst formulas (issue #31)
//!
//! A formula is a textured `Shape::Mesh` (`crate::typst_screen`) whose image
//! is rasterized at the logical size: stretched, it blurred like text did
//! before #18. The mesh keeps its geometry here — transformed like any
//! shape, so at the same place and the same size — but points to **another
//! texture**: the same formula rasterized at `pixels_per_point × scale`, by
//! [`FormulaTexture::rerender`], from the already compiled document (no
//! recompilation, no new measurement). Same box, same texture coverage
//! (0..1): only the sharpness changes.
//!
//! One variant rasterized directly at the displayed scale during
//! collection. Rejected: the displayed rectangle would have had to go into
//! the key of the main cache, so the whole Plotters pipeline would replay at
//! each step of a resize.
//!
//! The enlarged textures live in [`ScaledShapes`], outside the view's
//! `FormulaCache`: they do not count toward its limit and disappear with the
//! scaled version that points to them. A texture already made at the right
//! resolution is carried over from one version to the next (moving the graph
//! without resizing it rasterizes nothing).

use std::collections::HashMap;
use std::sync::Arc;

use egui::emath::TSTransform;
use egui::epaint::TextShape;
use egui::{Shape, TextureId};
use stokesforge_render::formula::FormulaCache;

use crate::typst_screen::FormulaTexture;

/// Transforms `shape` by `transform` (uniform scaling + offset), with the
/// text rasterized again at the displayed size.
///
/// `retexture` gives, for the texture of a mesh, the one that must replace
/// it at the displayed scale — `None` to keep it (a mesh that is not a
/// formula, or scale 1).
pub fn transform_shape(
    context: &egui::Context,
    shape: &mut Shape,
    transform: TSTransform,
    retexture: &mut dyn FnMut(TextureId) -> Option<TextureId>,
) {
    match shape {
        Shape::Text(text) => transform_text(context, text, transform),
        Shape::Vec(shapes) => {
            for inner in shapes {
                transform_shape(context, inner, transform, retexture);
            }
        }
        Shape::Mesh(mesh) => {
            let formula = retexture(mesh.texture_id);
            if let Some(id) = formula {
                Arc::make_mut(mesh).texture_id = id;
            }
            shape.transform(transform);
            if let (Some(id), Shape::Mesh(mesh)) = (formula, &mut *shape) {
                let size = context.tex_manager().read().meta(id).map(|meta| meta.size);
                if let Some(size) = size {
                    snap_to_pixels(Arc::make_mut(mesh), size, context.pixels_per_point());
                }
            }
        }
        other => other.transform(transform),
    }
}

/// Snaps a formula's mesh to the physical pixel grid, at the exact size of
/// its texture (issue #86).
///
/// A formula is an image: placed at a fraction of a pixel, or stretched by a
/// fraction of a pixel, linear filtering blends it with its neighbors and
/// thin strokes turn blurry — mostly noticeable on small text (a figure
/// reduced in a small window), barely visible on a large one. So the
/// top-left corner of the box lands on a whole pixel and the box measures
/// exactly `size` pixels (the texture is rasterized rounded up, hence the
/// gap). The shift is under one pixel.
///
/// No effect on a mesh that is not an axis-aligned quadrilateral (a formula
/// rotated by an arbitrary angle is not snapped).
fn snap_to_pixels(mesh: &mut egui::epaint::Mesh, size: [usize; 2], pixels_per_point: f32) {
    let bounds = mesh.calc_bounds();
    if mesh.vertices.len() != 4 || !bounds.is_positive() || pixels_per_point <= 0.0 {
        return;
    }
    let axis_aligned = mesh.vertices.iter().all(|vertex| {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
        (near(vertex.pos.x, bounds.min.x) || near(vertex.pos.x, bounds.max.x))
            && (near(vertex.pos.y, bounds.min.y) || near(vertex.pos.y, bounds.max.y))
    });
    if !axis_aligned {
        return;
    }
    // A quarter turn swaps the width and height of the texture.
    let (w, h) = (size[0] as f32, size[1] as f32);
    let (physical_w, physical_h) = (
        bounds.width() * pixels_per_point,
        bounds.height() * pixels_per_point,
    );
    let (target_w, target_h) = if (physical_w - w).abs() + (physical_h - h).abs()
        <= (physical_w - h).abs() + (physical_h - w).abs()
    {
        (w, h)
    } else {
        (h, w)
    };
    let min = egui::pos2(
        (bounds.min.x * pixels_per_point).round() / pixels_per_point,
        (bounds.min.y * pixels_per_point).round() / pixels_per_point,
    );
    let max = min + egui::vec2(target_w, target_h) / pixels_per_point;
    let middle = bounds.center();
    for vertex in &mut mesh.vertices {
        vertex.pos = egui::pos2(
            if vertex.pos.x < middle.x {
                min.x
            } else {
                max.x
            },
            if vertex.pos.y < middle.y {
                min.y
            } else {
                max.y
            },
        );
    }
}

fn transform_text(context: &egui::Context, text: &mut TextShape, transform: TSTransform) {
    let scale = transform.scaling;
    if (scale - 1.0).abs() < f32::EPSILON {
        text.transform(transform);
        return;
    }
    let mut job = (*text.galley.job).clone();
    for section in &mut job.sections {
        section.format.font_id.size *= scale;
        section.format.line_height = section.format.line_height.map(|height| height * scale);
        section.format.extra_letter_spacing *= scale;
    }
    text.galley = context.fonts_mut(|fonts| fonts.layout_job(job));
    text.pos = transform * text.pos;
    text.underline.width *= scale;
}

/// What identifies a scaled version of the cached shapes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaledKey {
    /// The logical figure it comes from.
    pub source: crate::cache::CacheKey,
    /// Displayed rectangle (scale **and** position).
    pub display: egui::Rect,
}

/// Scaled shapes, kept from one frame to the next as long as the figure and
/// the displayed rectangle do not change.
#[derive(Default)]
pub struct ScaledShapes {
    key: Option<ScaledKey>,
    shapes: Vec<Shape>,
    /// The formulas rasterized at the displayed scale, by original texture
    /// (the one of the cached mesh) — see the module header.
    formulas: HashMap<TextureId, Arc<FormulaTexture>>,
}

impl std::fmt::Debug for ScaledShapes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScaledShapes")
            .field("key", &self.key)
            .field("shapes", &self.shapes.len())
            .field("formulas", &self.formulas.len())
            .finish()
    }
}

impl ScaledShapes {
    /// The shapes of `source` at the scale of `key.display`, recomputed if the
    /// key changed. `formulas` is the cache that produced the textures of the
    /// formulas of `source`: it is where the document of a formula to
    /// rasterize again is found.
    pub fn get(
        &mut self,
        context: &egui::Context,
        key: ScaledKey,
        source: &[Shape],
        transform: TSTransform,
        formulas: &FormulaCache<FormulaTexture>,
    ) -> &[Shape] {
        if self.key != Some(key) {
            let pixels_per_point = key.source.pixels_per_point * transform.scaling;
            let unscaled = (transform.scaling - 1.0).abs() < f32::EPSILON;
            let previous = std::mem::take(&mut self.formulas);
            let mut kept = HashMap::new();
            // Built at the first mesh met only: a figure without formulas
            // does not walk the cache.
            let mut by_id: Option<HashMap<TextureId, Arc<FormulaTexture>>> = None;
            let mut retexture = |id: TextureId| -> Option<TextureId> {
                let by_id = by_id.get_or_insert_with(|| {
                    formulas
                        .values()
                        .into_iter()
                        .map(|texture| (texture.id(), texture))
                        .collect()
                });
                if unscaled {
                    // No new texture, but the formula is recognized: its
                    // mesh is snapped to the pixels (#86).
                    return by_id.contains_key(&id).then_some(id);
                }
                let sharp = match previous.get(&id) {
                    Some(texture) if texture.pixels_per_point() == pixels_per_point => {
                        Arc::clone(texture)
                    }
                    _ => Arc::new(by_id.get(&id)?.rerender(context, pixels_per_point)?),
                };
                let sharp_id = sharp.id();
                kept.insert(id, sharp);
                Some(sharp_id)
            };
            self.shapes.clear();
            self.shapes.extend(source.iter().cloned().map(|mut shape| {
                transform_shape(context, &mut shape, transform, &mut retexture);
                shape
            }));
            self.formulas = kept;
            self.key = Some(key);
        }
        &self.shapes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Color32, FontId, Pos2};

    fn context() -> egui::Context {
        let context = egui::Context::default();
        let mut output = context.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
        context
    }

    fn text_shape(context: &egui::Context, size: f32) -> Shape {
        let galley = context.fonts_mut(|fonts| {
            fonts.layout_no_wrap(
                "Tension (V)".to_owned(),
                FontId::proportional(size),
                Color32::BLACK,
            )
        });
        Shape::galley(Pos2::new(10.0, 20.0), galley, Color32::BLACK)
    }

    fn font_size(shape: &Shape) -> f32 {
        let Shape::Text(text) = shape else {
            panic!("a text shape expected");
        };
        text.galley.job.sections[0].format.font_id.size
    }

    /// #18: enlarging a graph rasterizes the text again at the displayed size
    /// instead of stretching the glyph mesh (blur).
    #[test]
    fn enlarged_text_is_laid_out_again_at_the_displayed_size() {
        let context = context();
        let transform = TSTransform::new(egui::vec2(5.0, 7.0), 2.5);
        let mut shape = text_shape(&context, 12.0);
        let logical = match &shape {
            Shape::Text(text) => text.galley.rect.width(),
            _ => unreachable!(),
        };
        transform_shape(&context, &mut shape, transform, &mut |_| None);
        assert!((font_size(&shape) - 30.0).abs() < 1e-4);
        let Shape::Text(text) = &shape else {
            unreachable!()
        };
        assert_eq!(text.pos, Pos2::new(30.0, 57.0));
        // The width stays proportional: the layout does not drift.
        let ratio = text.galley.rect.width() / logical;
        assert!((ratio - 2.5).abs() < 0.15, "width ratio {ratio}");
    }

    #[test]
    fn a_nested_text_is_also_laid_out_again() {
        let context = context();
        let mut shape = Shape::Vec(vec![text_shape(&context, 10.0)]);
        transform_shape(
            &context,
            &mut shape,
            TSTransform::new(egui::Vec2::ZERO, 2.0),
            &mut |_| None,
        );
        let Shape::Vec(inner) = &shape else {
            unreachable!()
        };
        assert!((font_size(&inner[0]) - 20.0).abs() < 1e-4);
    }

    fn key_for(display: f32) -> ScaledKey {
        ScaledKey {
            source: crate::cache::CacheKey {
                revision: Default::default(),
                rect: egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 100.0)),
                pixels_per_point: 1.0,
                view_generation: 0,
            },
            display: egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(display, display)),
        }
    }

    #[test]
    fn scaled_shapes_are_kept_as_long_as_the_key_does_not_change() {
        let context = context();
        let source = [text_shape(&context, 12.0)];
        let key = key_for(200.0);
        let transform = TSTransform::new(egui::Vec2::ZERO, 2.0);
        let formulas = FormulaCache::new();
        let mut cache = ScaledShapes::default();
        let first = cache
            .get(&context, key, &source, transform, &formulas)
            .as_ptr();
        let second = cache
            .get(&context, key, &source, transform, &formulas)
            .as_ptr();
        assert_eq!(first, second, "no recomputation with an identical key");
        assert!(
            (font_size(&cache.get(&context, key, &source, transform, &formulas)[0]) - 24.0).abs()
                < 1e-4
        );
    }

    /// A `$lambda$` formula rasterized at `pixels_per_point` 1, and its mesh
    /// anchored at (10, 20).
    fn formula(
        context: &egui::Context,
        cache: &FormulaCache<FormulaTexture>,
    ) -> (Arc<FormulaTexture>, Shape) {
        let choice = stokesforge_render::FontChoice::default();
        let screen = crate::typst_screen::TypstScreen::new(context, cache, &choice)
            .expect("font resolution failed");
        let texture = screen
            .render("$lambda$", 14.0, [0, 0, 0, 255], 1.0)
            .expect("rendering");
        let anchor = plotters_backend::text_anchor::Pos::new(
            plotters_backend::text_anchor::HPos::Left,
            plotters_backend::text_anchor::VPos::Top,
        );
        let shape = crate::typst_screen::mesh(
            &texture,
            Pos2::new(10.0, 20.0),
            anchor,
            plotters_backend::FontTransform::None,
        );
        (texture, shape)
    }

    fn mesh_of(shape: &Shape) -> &egui::epaint::Mesh {
        let Shape::Mesh(mesh) = shape else {
            panic!("a mesh expected");
        };
        mesh
    }

    /// Issue #31: enlarged twice, a formula points to a texture with twice as
    /// many pixels per side, in **the same box** as the simply stretched
    /// mesh — the sharpness changes, not the layout.
    #[test]
    fn an_enlarged_formula_is_rasterized_at_the_displayed_size() {
        let context = context();
        let formulas = FormulaCache::new();
        let (base, source) = formula(&context, &formulas);
        let transform = TSTransform::new(egui::vec2(5.0, 7.0), 2.0);

        let mut stretched = source.clone();
        stretched.transform(transform);
        let mut cache = ScaledShapes::default();
        let enlarged = &cache.get(&context, key_for(200.0), &[source], transform, &formulas)[0];

        let (before, after) = (mesh_of(&stretched), mesh_of(enlarged));
        // The mesh is snapped to the pixels (#86): the box moves by less than
        // one physical pixel (ppp 1 here), no layout change.
        let (before, after_box) = (before.calc_bounds(), after.calc_bounds());
        assert!(
            (before.min - after_box.min).abs().max_elem() <= 1.0
                && (before.size() - after_box.size()).abs().max_elem() <= 1.0,
            "the box moved: {before:?} -> {after_box:?}"
        );
        assert_ne!(after.texture_id, base.id(), "original texture stretched");
        let fine = context
            .tex_manager()
            .read()
            .meta(after.texture_id)
            .expect("enlarged texture missing")
            .size;
        let [w, h] = base.size_pixels();
        assert!(
            fine[0].abs_diff(2 * w) <= 1 && fine[1].abs_diff(2 * h) <= 1,
            "{fine:?} is not twice {:?}",
            [w, h]
        );
        assert_eq!(
            formulas.len(),
            1,
            "the enlarged texture entered the view's cache"
        );
    }

    /// The witness: at scale 1 (a simple move), nothing is rasterized again
    /// — the mesh keeps its texture. And an enlarged texture already made is
    /// carried over when only the position changes.
    #[test]
    fn moving_without_enlarging_rasterizes_nothing() {
        let context = context();
        let formulas = FormulaCache::new();
        let (base, source) = formula(&context, &formulas);
        let mut cache = ScaledShapes::default();

        let moved = TSTransform::new(egui::vec2(30.0, 0.0), 1.0);
        let shapes = cache.get(
            &context,
            key_for(100.0),
            std::slice::from_ref(&source),
            moved,
            &formulas,
        );
        assert_eq!(mesh_of(&shapes[0]).texture_id, base.id());

        let double = TSTransform::new(egui::Vec2::ZERO, 2.0);
        let first = mesh_of(
            &cache.get(
                &context,
                key_for(200.0),
                std::slice::from_ref(&source),
                double,
                &formulas,
            )[0],
        )
        .texture_id;
        let shifted = TSTransform::new(egui::vec2(40.0, 0.0), 2.0);
        let mut elsewhere = key_for(200.0);
        elsewhere.display = elsewhere.display.translate(egui::vec2(40.0, 0.0));
        let second =
            mesh_of(&cache.get(&context, elsewhere, &[source], shifted, &formulas)[0]).texture_id;
        assert_eq!(
            first, second,
            "same scale: the enlarged texture is carried over"
        );
    }

    /// Issue #86: on screen, a formula's mesh lands on a whole physical pixel
    /// and measures exactly its texture, including when the figure is reduced
    /// to a scale that does not fit exactly.
    #[test]
    fn a_reduced_formula_is_snapped_to_the_pixels() {
        let context = context();
        let formulas = FormulaCache::new();
        let (_, source) = formula(&context, &formulas);
        let transform = TSTransform::new(egui::vec2(5.3, 7.7), 0.63);
        let mut cache = ScaledShapes::default();
        let shapes = cache.get(&context, key_for(63.0), &[source], transform, &formulas);
        let bounds = mesh_of(&shapes[0]).calc_bounds();
        let ppp = context.pixels_per_point();
        let whole = |v: f32| ((v * ppp) - (v * ppp).round()).abs() < 1e-3;
        assert!(whole(bounds.min.x) && whole(bounds.min.y), "{bounds:?}");
        let size = context
            .tex_manager()
            .read()
            .meta(mesh_of(&shapes[0]).texture_id)
            .expect("texture")
            .size;
        assert!((bounds.width() * ppp - size[0] as f32).abs() < 1e-3);
        assert!((bounds.height() * ppp - size[1] as f32).abs() < 1e-3);
    }
}
