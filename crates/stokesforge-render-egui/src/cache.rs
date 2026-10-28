//! The shape cache — task R4.3.
//!
//! # The key, and why it has exactly four terms
//!
//! `(revision, rect, pixels_per_point, view_generation)`
//! (`architecture-choisie.md` §4.2):
//!
//! - **`revision`** — the monotonic counter of `stokesforge-session`, which
//!   already existed and is used as intended (*"a client knows whether its
//!   snapshot is stale by comparing the `revision`"*). The renderer becomes
//!   one more client of the same protocol, like the UI and the script
//!   thread; nothing had to be invented;
//! - **`rect`** — the logical rectangle given to the pipeline changes its
//!   geometry, not the model. `GraphView::paint_scaled` keeps this rectangle
//!   at `DEFAULT_FIGURE_SIZE` and transforms the cached shapes on output:
//!   a plain window resize no longer changes it;
//! - **`pixels_per_point`** — this is the exception that `epaint` documents
//!   itself in its source: *"storing [shapes] should also be fine with
//!   one exception: `Shape::Text` depends on the current `pixels_per_point`
//!   and so must be recreated every time `pixels_per_point` changes"*;
//! - **`view_generation`** (task R7.1) — the counter of
//!   `stokesforge_render::view::ViewState`, which goes up on every committed
//!   zoom, pan or elastic zoom rectangle. Unlike *feathering* (see below),
//!   zoom/pan **changes the drawn geometry** — the axis bounds are no longer
//!   those of the data — so it must enter the key, or a stale figure would
//!   be served from the cache after a navigation gesture. A counter rather
//!   than the bounds themselves: they live in per-panel `f64`s, not really
//!   `Copy`-friendly in a variable number, and an integer that changes on
//!   every commit is enough to invalidate — the exact value of the framing
//!   does not have to pass through here.
//!
//! # What is deliberately *not* in the key
//!
//! *Feathering*. Turning it off during a drag — decision of 2026-09-10,
//! task R5.5 bis — changes the **tessellation**, not the `Shape`s. The cache
//! is therefore not invalidated; it is the retessellation that changes cost,
//! and it happens on every frame anyway. Written here because this is
//! exactly the kind of term one would add "to be safe" in R5, at the price
//! of a full rebuild on every button press.
//!
//! The crosshair neither (task R7.3): it is painted **on top of** the cache,
//! without invalidating it. This is the property that replaces the
//! `copy_from_bbox` *blitting* of the Python version, and it is free as long
//! as the crosshair does not enter the Plotters pipeline.

use egui::{Rect, Shape};
use stokesforge_session::Revision;

/// What identifies an already computed figure.
///
/// `PartialEq` on `f32`s: this is intended, and safe here. The two float
/// terms come from one frame to the next from the *same* source
/// (egui's layout, the system `pixels_per_point`) and are therefore
/// bit for bit identical as long as nothing moves. A comparison "within
/// epsilon" would only delay the invalidation of a half-pixel resize —
/// at the price of a wrong figure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CacheKey {
    /// Revision of the session snapshot the figure comes from.
    pub revision: Revision,
    /// Logical rectangle used by the pipeline (which can differ from the
    /// displayed rectangle after scaling).
    pub rect: Rect,
    /// Display scale at the time of the computation.
    pub pixels_per_point: f32,
    /// Generation of `stokesforge_render::view::ViewState` at the time of the
    /// computation — task R7.1. `0` as long as no zoom or pan was committed.
    pub view_generation: u64,
}

/// The shapes of a figure, and what is needed to know whether they are still valid.
#[derive(Debug, Default)]
pub struct ShapeCache {
    key: Option<CacheKey>,
    shapes: Vec<Shape>,
    hits: u64,
    misses: u64,
}

impl ShapeCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// The cached shapes. Empty as long as nothing has been built.
    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    /// The key of the cached shapes, if any.
    pub fn key(&self) -> Option<CacheKey> {
        self.key
    }

    /// Number of frames served without replaying the Plotters pipeline.
    ///
    /// Exposed so that task R4.3 is **verifiable**: "a frame without
    /// change does not replay the pipeline" is a claim about an invisible
    /// behavior, hence a claim to test.
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Number of full rebuilds.
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Empties the cache without touching the counters.
    pub fn invalidate(&mut self) {
        self.key = None;
        self.shapes.clear();
    }

    /// Returns the shapes of `key`, building them if necessary.
    ///
    /// If `build` fails, the cache is left **empty** rather than partially
    /// filled: a half-built figure that would then be served as valid would
    /// be worse than no figure at all.
    pub fn get_or_build<T, E>(
        &mut self,
        key: CacheKey,
        build: impl FnOnce(&mut Vec<Shape>) -> Result<T, E>,
    ) -> Result<Outcome<T>, E> {
        if self.key == Some(key) {
            self.hits += 1;
            return Ok(Outcome {
                built: None,
                cached: true,
                shape_count: self.shapes.len(),
            });
        }
        self.misses += 1;
        self.key = None;
        self.shapes.clear();
        match build(&mut self.shapes) {
            Ok(value) => {
                self.key = Some(key);
                Ok(Outcome {
                    built: Some(value),
                    cached: false,
                    shape_count: self.shapes.len(),
                })
            }
            Err(error) => {
                self.shapes.clear();
                Err(error)
            }
        }
    }
}

/// What a cache lookup tells the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome<T> {
    /// What `build` returned, if the pipeline was actually replayed.
    pub built: Option<T>,
    /// True if the frame was served from the cache.
    pub cached: bool,
    /// Number of shapes served.
    pub shape_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(revision: Revision, width: f32, pixels_per_point: f32) -> CacheKey {
        key_with_view(revision, width, pixels_per_point, 0)
    }

    fn key_with_view(
        revision: Revision,
        width: f32,
        pixels_per_point: f32,
        view_generation: u64,
    ) -> CacheKey {
        CacheKey {
            revision,
            rect: Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 100.0)),
            pixels_per_point,
            view_generation,
        }
    }

    fn one_shape(shapes: &mut Vec<Shape>) -> Result<usize, ()> {
        shapes.push(Shape::Noop);
        Ok(shapes.len())
    }

    #[test]
    fn the_second_lookup_does_not_rebuild() {
        let mut cache = ShapeCache::new();
        let k = key(1, 200.0, 1.0);
        assert!(!cache.get_or_build(k, one_shape).unwrap().cached);
        assert!(cache.get_or_build(k, one_shape).unwrap().cached);
        assert_eq!((cache.misses(), cache.hits()), (1, 1));
        assert_eq!(cache.shapes().len(), 1);
    }

    #[test]
    fn each_of_the_four_terms_invalidates() {
        for next in [
            key(2, 200.0, 1.0),
            key(1, 201.0, 1.0),
            key(1, 200.0, 2.0),
            key_with_view(1, 200.0, 1.0, 1),
        ] {
            let mut cache = ShapeCache::new();
            cache.get_or_build(key(1, 200.0, 1.0), one_shape).unwrap();
            assert!(
                !cache.get_or_build(next, one_shape).unwrap().cached,
                "{next:?} should have invalidated"
            );
        }
    }

    #[test]
    fn a_failure_leaves_no_half_built_figure() {
        let mut cache = ShapeCache::new();
        let failure = cache.get_or_build::<(), &str>(key(1, 200.0, 1.0), |shapes| {
            shapes.push(Shape::Noop);
            shapes.push(Shape::Noop);
            Err("the pipeline refused")
        });
        assert_eq!(failure, Err("the pipeline refused"));
        assert!(cache.shapes().is_empty());
        assert_eq!(cache.key(), None);
    }
}
