//! Small drawings of whole figures, kept per key (#543): the theme tiles and
//! their hover zoom draw the same cached shapes.
//!
//! A figure is drawn **once at its natural size** through the ordinary
//! pipeline (so a theme with two layers side by side or an extra axis shows as
//! such, and the density of ticks and the placement of legends are the real
//! ones), then the whole drawing, texts and strokes included, is scaled into
//! the rectangle it is shown in (`crate::scaled`, the very path of
//! [`crate::GraphView::paint_scaled`]: text laid out again at the displayed
//! size, formula textures rasterized again, never a stretched bitmap).
//!
//! # Texts and fonts are the application's
//!
//! A drawing measures and lays out text exactly like a graph window does:
//! [`TextMeasurer`] and the Typst fonts are built from the same definitions
//! ([`crate::font_definitions`], [`FontChoice::default`]), and every title,
//! axis title and legend entry goes through Typst (`TypstScreen`). There is
//! **no private static measurer**: the one measurer, the one
//! [`FormulaCache`] and the resolved fonts belong to the [`ScaledDrawings`]
//! the application holds, shared by all its keys. Figures drawn without Typst
//! would measure titles differently from the graph and show them as raw
//! markup.
//!
//! # Ownership of formula textures (#500, #508)
//!
//! A formula texture lives exactly as long as some `Arc<FormulaTexture>` is
//! held. Each entry holds them twice over, like a graph window: its
//! [`ShapeCache`] (the textures its shapes point to, through
//! `TypstScreen::holding`) and its [`ScaledShapes`] (every texture its scaled
//! meshes point to). [`FormulaCache`] only memoizes: it evicts what nobody
//! holds ([`FormulaCache::evict_unshared`]), never in bulk, and only after the
//! shapes were rebuilt, so a title never vanishes from a tile.
//!
//! # What invalidates an entry
//!
//! The figure itself (the [`PlotSpec`] given, compared by value) and the
//! pixel density. The **tree must not change** under a key: the caller owns
//! that guarantee (the theme specimens are bound to a reference tree that is
//! built once and never edited). A changed theme style, or a pixel density
//! change, draws again; the same ones hit the cache.

use std::collections::HashMap;
use std::hash::Hash;

use egui::emath::TSTransform;
use egui::{Painter, Pos2, Rect, Vec2};
use plotters::drawing::IntoDrawingArea;
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::Tree;
use stokesforge_plot::PlotSpec;
use stokesforge_render::formula::{self, FormulaCache};
use stokesforge_render::{bind_spec, pipeline, FigureTexts, FontChoice};

use crate::cache::{CacheKey, ShapeCache};
use crate::scaled::{ScaledKey, ScaledShapes};
use crate::typst_screen::{FormulaTexture, TypstScreen};
use crate::{
    collect_shapes_with_texts, collect_shapes_with_typst_and_texts, EguiBackend, ScreenText,
    TextMeasurer, FORMULA_CACHE_LIMIT,
};

/// Where a figure of natural size `natural` lands in `rect`: the scale (never
/// above `max_scale`) and the origin of the scaled figure, centred.
pub fn fit(natural: Vec2, rect: Rect, max_scale: f32) -> (f32, Pos2) {
    let scale = (rect.width() / natural.x)
        .min(rect.height() / natural.y)
        .min(max_scale);
    (scale, rect.center() - natural * scale / 2.0)
}

/// One figure of the set.
struct Entry {
    /// The figure as drawn (variables bound), to tell a changed one.
    spec: PlotSpec,
    ppp_bits: u32,
    /// Natural size of the figure, in points.
    natural: Vec2,
    /// The shapes at natural size, and the textures they point to.
    cache: ShapeCache,
    /// The scaled version of `cache`, per displayed rectangle.
    scaled: ScaledShapes,
    /// Rank of the drawing that made `cache` (`ScaledKey::build`).
    build: u64,
}

/// The cached, scaled drawings of figures, by key `K` (module documentation).
pub struct ScaledDrawings<K> {
    measurer: TextMeasurer,
    formulas: FormulaCache<FormulaTexture>,
    font_choice: FontChoice,
    typst_fonts: Option<(typst::text::Font, typst::text::Font)>,
    entries: HashMap<K, Entry>,
    /// Figures drawn at natural size since the set was made.
    draws: u64,
}

impl<K: Hash + Eq> Default for ScaledDrawings<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K> std::fmt::Debug for ScaledDrawings<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScaledDrawings")
            .field("entries", &self.entries.len())
            .field("formulas", &self.formulas.len())
            .finish()
    }
}

impl<K: Hash + Eq> ScaledDrawings<K> {
    /// An empty set. Builds the measurement source and resolves the Typst
    /// fonts once (the cost of a [`crate::GraphView`] too): hold one set for
    /// the whole application, not one per frame.
    pub fn new() -> Self {
        let font_choice = FontChoice::default();
        Self {
            measurer: TextMeasurer::default(),
            formulas: FormulaCache::new(),
            typst_fonts: formula::resolve_fonts(&font_choice).ok(),
            font_choice,
            entries: HashMap::new(),
            draws: 0,
        }
    }

    /// Number of figures kept.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if no figure is kept.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Lets go of the figures whose key `keep` refuses (a deleted theme).
    pub fn retain(&mut self, mut keep: impl FnMut(&K) -> bool) {
        self.entries.retain(|key, _| keep(key));
    }

    /// How many times a figure was drawn at natural size, all keys together.
    /// A cache hit leaves it unchanged.
    pub fn draws(&self) -> u64 {
        self.draws
    }

    /// The figure `spec` of `key` drawn at its natural size, now or from the
    /// cache; returns that size in points, `None` when the rendering refuses
    /// the figure (the entry is then dropped).
    ///
    /// `spec` must read columns of `tree` only, and `tree` must not change
    /// under `key` (module documentation).
    pub fn draw(
        &mut self,
        context: &egui::Context,
        key: K,
        tree: &Tree,
        spec: &PlotSpec,
        texts: &FigureTexts,
    ) -> Option<Vec2> {
        let ppp_bits = context.pixels_per_point().to_bits();
        if let Some(entry) = self.entries.get(&key) {
            if entry.ppp_bits == ppp_bits && entry.spec == *spec {
                return Some(entry.natural);
            }
        }
        let built = self.build(context, tree, spec, texts, ppp_bits);
        match built {
            Some(mut fresh) => {
                self.draws += 1;
                fresh.build = self.draws;
                let natural = fresh.natural;
                // The previous entry (if any) goes only now, after the new
                // shapes hold their textures; then what nobody holds leaves
                // the memo.
                self.entries.insert(key, fresh);
                if self.formulas.len() > FORMULA_CACHE_LIMIT {
                    self.formulas.evict_unshared();
                }
                Some(natural)
            }
            None => {
                self.entries.remove(&key);
                None
            }
        }
    }

    fn build(
        &self,
        context: &egui::Context,
        tree: &Tree,
        spec: &PlotSpec,
        texts: &FigureTexts,
        ppp_bits: u32,
    ) -> Option<Entry> {
        let bound = bind_spec(tree, spec, texts);
        let text = ScreenText::new(context, &self.measurer);
        // Natural size: the layout of the figure, measured with this text.
        let natural = {
            let mut scratch = Vec::new();
            let probe = Rect::from_min_size(Pos2::ZERO, egui::vec2(1.0, 1.0));
            let backend = EguiBackend::in_rect(&mut scratch, probe, &text);
            let layout = match &self.typst_fonts {
                Some(fonts) => {
                    let typst = TypstScreen::with_fonts(
                        context,
                        &self.formulas,
                        &self.font_choice,
                        fonts.clone(),
                    );
                    pipeline::figure_layout(&bound, &backend.with_typst(typst).into_drawing_area())
                }
                None => pipeline::figure_layout(&bound, &backend.into_drawing_area()),
            }
            .ok()?;
            egui::vec2(layout.size.0 as f32, layout.size.1 as f32)
        };
        if !(natural.x > 0.0 && natural.y > 0.0) {
            return None;
        }
        let rect = Rect::from_min_size(Pos2::ZERO, natural);
        let plot = resolve_spec(tree, &bound);
        let key = CacheKey {
            revision: 0,
            rect,
            pixels_per_point: f32::from_bits(ppp_bits),
            view_generation: 0,
        };
        let mut cache = ShapeCache::new();
        let built = cache.get_or_build(key, |shapes, held| match &self.typst_fonts {
            Some(fonts) => {
                let typst = TypstScreen::with_fonts(
                    context,
                    &self.formulas,
                    &self.font_choice,
                    fonts.clone(),
                )
                .holding(held);
                collect_shapes_with_typst_and_texts(shapes, rect, &text, typst, &plot, texts)
            }
            None => collect_shapes_with_texts(shapes, rect, &text, &plot, texts),
        });
        built.ok()?;
        Some(Entry {
            spec: spec.clone(),
            ppp_bits,
            natural,
            cache,
            scaled: ScaledShapes::default(),
            build: 0,
        })
    }

    /// Paints the figure of `key` (already [`draw`](Self::draw)n) centred in
    /// `rect`, scaled whole, by at most `max_scale`; returns the rectangle it
    /// covers, `None` if nothing is kept for `key` or `rect` is empty.
    pub fn paint(
        &mut self,
        context: &egui::Context,
        painter: &Painter,
        rect: Rect,
        max_scale: f32,
        key: &K,
    ) -> Option<Rect> {
        let entry = self.entries.get_mut(key)?;
        if !(rect.width() > 0.0 && rect.height() > 0.0) {
            return None;
        }
        let (scale, origin) = fit(entry.natural, rect, max_scale);
        let display = Rect::from_min_size(origin, entry.natural * scale);
        let source = entry.cache.key()?;
        let transform = TSTransform::new(origin.to_vec2(), scale);
        let scaled_key = ScaledKey {
            source,
            display,
            build: entry.build,
        };
        let painter = painter.with_clip_rect(painter.clip_rect().intersect(display.expand(1.0)));
        let shapes = entry.scaled.get(
            context,
            scaled_key,
            entry.cache.shapes(),
            transform,
            entry.cache.textures(),
        );
        painter.extend(shapes.iter().cloned());
        Some(display)
    }

    /// [`draw`](Self::draw) then [`paint`](Self::paint): the one call of a
    /// tile.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_scaled(
        &mut self,
        context: &egui::Context,
        painter: &Painter,
        rect: Rect,
        max_scale: f32,
        key: K,
        tree: &Tree,
        spec: &PlotSpec,
        texts: &FigureTexts,
    ) -> Option<Rect>
    where
        K: Clone,
    {
        self.draw(context, key.clone(), tree, spec, texts)?;
        self.paint(context, painter, rect, max_scale, &key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Shape;
    use stokesforge_core::Project;
    use stokesforge_plot::{PanelSpec, Panels};

    fn context() -> egui::Context {
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render::font_definitions());
        context
    }

    fn spec(title: &str) -> PlotSpec {
        let mut panel = PanelSpec::at(0, 0);
        panel.bottom.label = "t (s)".to_owned();
        let mut spec = PlotSpec::empty(title).with_panels(Panels::new(vec![panel]));
        spec.frame_widths = vec![200.0];
        spec.frame_heights = vec![120.0];
        spec
    }

    /// Runs one frame, painting `key` into `rect`; returns what was covered
    /// and the shapes painted.
    fn frame(
        context: &egui::Context,
        drawings: &mut ScaledDrawings<u32>,
        rect: Rect,
        key: u32,
    ) -> (Option<Rect>, Vec<Shape>) {
        let mut covered = None;
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            covered = drawings.paint(context, ui.painter(), rect, 1.0, &key);
        });
        output.textures_delta.clear();
        let shapes = output.shapes.into_iter().map(|c| c.shape).collect();
        (covered, shapes)
    }

    fn draw(
        context: &egui::Context,
        drawings: &mut ScaledDrawings<u32>,
        key: u32,
        spec: &PlotSpec,
    ) -> Option<Vec2> {
        let project = Project::new("drawings").expect("project");
        let mut size = None;
        let mut output = context.run_ui(egui::RawInput::default(), |_| {
            size = drawings.draw(context, key, project.tree(), spec, &FigureTexts::default());
        });
        output.textures_delta.clear();
        size
    }

    fn meshes(shapes: &[Shape]) -> Vec<egui::TextureId> {
        fn walk(shape: &Shape, out: &mut Vec<egui::TextureId>) {
            match shape {
                Shape::Mesh(mesh) => out.push(mesh.texture_id),
                Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|s| walk(s, &mut out));
        out
    }

    #[test]
    fn fit_centres_and_never_exceeds_the_cap() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), egui::vec2(100.0, 100.0));
        let (scale, origin) = fit(egui::vec2(400.0, 200.0), rect, 1.0);
        assert!((scale - 0.25).abs() < 1e-6);
        assert_eq!(origin, Pos2::new(10.0, 45.0));
        let (scale, _) = fit(egui::vec2(40.0, 20.0), rect, 1.0);
        assert!((scale - 1.0).abs() < 1e-6, "the cap holds");
    }

    #[test]
    fn a_figure_is_drawn_once_and_again_only_when_it_changes() {
        let context = context();
        let mut drawings = ScaledDrawings::new();
        let first = draw(&context, &mut drawings, 1, &spec("Title"));
        assert!(first.is_some());
        assert_eq!(drawings.draws(), 1);
        draw(&context, &mut drawings, 1, &spec("Title"));
        assert_eq!(drawings.draws(), 1, "the same figure hits the cache");
        let mut styled = spec("Title");
        for panel in styled.panels.iter_mut() {
            panel.bottom.tick_color = "#ff0000".to_owned();
        }
        draw(&context, &mut drawings, 1, &styled);
        assert_eq!(drawings.draws(), 2, "a changed style draws again");
        draw(&context, &mut drawings, 2, &styled);
        assert_eq!(drawings.draws(), 3, "each key has its own figure");
        assert_eq!(drawings.len(), 2);
    }

    #[test]
    fn a_new_pixel_density_draws_again() {
        let context = context();
        let mut drawings = ScaledDrawings::new();
        draw(&context, &mut drawings, 1, &spec("Title"));
        context.set_pixels_per_point(2.0);
        draw(&context, &mut drawings, 1, &spec("Title"));
        assert_eq!(drawings.draws(), 2);
    }

    #[test]
    fn the_natural_size_is_the_layout_of_the_figure() {
        let context = context();
        let mut drawings = ScaledDrawings::new();
        let size = draw(&context, &mut drawings, 1, &spec("Title")).expect("drawn");
        // The 200 x 120 plot area plus margins, axis bands and the title band.
        assert!(size.x > 200.0 && size.y > 120.0, "{size:?}");
    }

    #[test]
    fn the_drawing_is_scaled_whole_into_the_rect() {
        let context = context();
        let mut drawings = ScaledDrawings::new();
        let natural = draw(&context, &mut drawings, 1, &spec("Title")).expect("drawn");
        let rect = Rect::from_min_size(Pos2::new(5.0, 5.0), egui::vec2(100.0, 100.0));
        let (covered, shapes) = frame(&context, &mut drawings, rect, 1);
        let covered = covered.expect("painted");
        let scale = (100.0 / natural.x).min(100.0 / natural.y);
        assert!((covered.width() - natural.x * scale).abs() < 1e-3);
        assert_eq!(covered.center(), rect.center());
        assert!(rect.expand(0.5).contains_rect(covered));
        assert!(!shapes.is_empty());
        // Nothing for a key that was never drawn, or an empty rect.
        assert!(frame(&context, &mut drawings, rect, 9).0.is_none());
        assert!(frame(&context, &mut drawings, Rect::NOTHING, 1).0.is_none());
    }

    #[test]
    fn a_title_survives_the_eviction_of_unshared_formulas() {
        let context = context();
        let mut drawings = ScaledDrawings::new();
        draw(&context, &mut drawings, 1, &spec("Spectrum"));
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(120.0, 120.0));
        let (_, shapes) = frame(&context, &mut drawings, rect, 1);
        let textures = meshes(&shapes);
        assert!(!textures.is_empty(), "the title goes through Typst");
        // What nobody holds is freed; the figure must hold its own.
        drawings.formulas.evict_unshared();
        let (_, shapes) = frame(&context, &mut drawings, rect, 1);
        let alive = |id| context.tex_manager().read().meta(id).is_some();
        assert!(meshes(&shapes).into_iter().all(alive));
    }
}
