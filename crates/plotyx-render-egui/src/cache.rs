//! Le cache de formes — tâche R4.3.
//!
//! # La clé, et pourquoi elle a exactement trois termes
//!
//! `(revision, rect, pixels_per_point)` (`architecture-choisie.md` §4.2) :
//!
//! - **`revision`** — le compteur monotone de `plotyx-session`, qui existait
//!   déjà et dont c'est l'usage prévu (*« un client sait si son instantané
//!   est périmé en comparant la `revision` »*). Le renderer devient un
//!   client de plus du même protocole, au même titre que l'UI et le fil de
//!   script ; il n'y avait rien à inventer ;
//! - **`rect`** — redimensionner la fenêtre change la géométrie de la
//!   figure, pas le modèle ;
//! - **`pixels_per_point`** — c'est l'exception qu'`epaint` documente
//!   lui-même dans sa source : *« storing [shapes] should also be fine with
//!   one exception: `Shape::Text` depends on the current `pixels_per_point`
//!   and so must be recreated every time `pixels_per_point` changes »*.
//!
//! # Ce qui n'est délibérément *pas* dans la clé
//!
//! Le *feathering*. Le couper pendant un glisser — décision du 2026-09-10,
//! tâche R5.5 bis — change la **tessellation**, pas les `Shape`. Le cache
//! n'en est donc pas invalidé ; c'est la retessellation qui change de coût,
//! et elle a lieu de toute façon à chaque image. Écrit ici parce que c'est
//! exactement le genre de terme qu'on ajouterait « par prudence » en R5,
//! au prix d'une reconstruction complète à chaque pression de bouton.
//!
//! Le réticule non plus (tâche R7.3) : il se peint **par-dessus** le cache,
//! sans l'invalider. C'est la propriété qui remplace le *blitting*
//! `copy_from_bbox` de la version Python, et elle est gratuite tant que le
//! réticule n'entre pas dans le pipeline Plotters.

use egui::{Rect, Shape};
use plotyx_session::Revision;

/// Ce qui identifie une figure déjà calculée.
///
/// `PartialEq` sur des `f32` : c'est voulu, et sans piège ici. Les deux
/// termes flottants viennent d'une image à l'autre de la *même* source
/// (la mise en page d'egui, le `pixels_per_point` du système) et sont donc
/// bit à bit identiques tant que rien ne bouge. Une comparaison « à epsilon
/// près » ne ferait que retarder l'invalidation d'un redimensionnement d'un
/// demi-pixel — au prix d'une figure fausse.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CacheKey {
    /// Révision de l'instantané de session dont la figure est issue.
    pub revision: Revision,
    /// Rectangle occupé à l'écran.
    pub rect: Rect,
    /// Échelle d'affichage au moment du calcul.
    pub pixels_per_point: f32,
}

/// Les formes d'une figure, et de quoi savoir si elles sont encore bonnes.
#[derive(Debug, Default)]
pub struct ShapeCache {
    key: Option<CacheKey>,
    shapes: Vec<Shape>,
    hits: u64,
    misses: u64,
}

impl ShapeCache {
    /// Un cache vide.
    pub fn new() -> Self {
        Self::default()
    }

    /// Les formes en cache. Vide tant que rien n'a été construit.
    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    /// La clé des formes en cache, s'il y en a.
    pub fn key(&self) -> Option<CacheKey> {
        self.key
    }

    /// Nombre d'images servies sans rejouer le pipeline Plotters.
    ///
    /// Exposé pour que la tâche R4.3 soit **vérifiable** : « une image sans
    /// changement ne rejoue pas le pipeline » est une affirmation sur un
    /// comportement invisible, donc une affirmation à tester.
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Nombre de reconstructions complètes.
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Vide le cache sans toucher aux compteurs.
    pub fn invalidate(&mut self) {
        self.key = None;
        self.shapes.clear();
    }

    /// Rend les formes de `key`, en les construisant si nécessaire.
    ///
    /// En cas d'échec de `build`, le cache est laissé **vide** plutôt que
    /// partiellement rempli : une figure à moitié construite qu'on servirait
    /// ensuite comme valide serait pire que pas de figure du tout.
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

/// Ce qu'une consultation du cache apprend à l'appelant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome<T> {
    /// Ce que `build` a rendu, si le pipeline a effectivement été rejoué.
    pub built: Option<T>,
    /// Vrai si l'image a été servie depuis le cache.
    pub cached: bool,
    /// Nombre de formes servies.
    pub shape_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(revision: Revision, width: f32, pixels_per_point: f32) -> CacheKey {
        CacheKey {
            revision,
            rect: Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 100.0)),
            pixels_per_point,
        }
    }

    fn une_forme(shapes: &mut Vec<Shape>) -> Result<usize, ()> {
        shapes.push(Shape::Noop);
        Ok(shapes.len())
    }

    #[test]
    fn la_seconde_consultation_ne_reconstruit_pas() {
        let mut cache = ShapeCache::new();
        let k = key(1, 200.0, 1.0);
        assert!(!cache.get_or_build(k, une_forme).unwrap().cached);
        assert!(cache.get_or_build(k, une_forme).unwrap().cached);
        assert_eq!((cache.misses(), cache.hits()), (1, 1));
        assert_eq!(cache.shapes().len(), 1);
    }

    #[test]
    fn chacun_des_trois_termes_invalide() {
        for suivante in [key(2, 200.0, 1.0), key(1, 201.0, 1.0), key(1, 200.0, 2.0)] {
            let mut cache = ShapeCache::new();
            cache.get_or_build(key(1, 200.0, 1.0), une_forme).unwrap();
            assert!(
                !cache.get_or_build(suivante, une_forme).unwrap().cached,
                "{suivante:?} aurait dû invalider"
            );
        }
    }

    #[test]
    fn un_echec_ne_laisse_pas_de_figure_a_moitie_faite() {
        let mut cache = ShapeCache::new();
        let echec = cache.get_or_build::<(), &str>(key(1, 200.0, 1.0), |shapes| {
            shapes.push(Shape::Noop);
            shapes.push(Shape::Noop);
            Err("le pipeline a refusé")
        });
        assert_eq!(echec, Err("le pipeline a refusé"));
        assert!(cache.shapes().is_empty());
        assert_eq!(cache.key(), None);
    }
}
