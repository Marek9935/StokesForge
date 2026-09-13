//! Chemin écran de Typst — tâche R8.3.
//!
//! Ce que `plotyx-render` (feature `typst`) ne pouvait pas porter lui-même
//! sans faire entrer `egui`/`typst-render` dans un crate qui n'en a pas
//! besoin (`architecture-choisie.md` §8) : la rastérisation d'une page
//! compilée en RGBA, son chargement en texture `egui`, et la construction
//! d'un `Shape::Mesh` positionné exactement comme l'aurait été un
//! `Shape::Text` ordinaire.
//!
//! # Dégradation, pas erreur
//!
//! Une formule qui ne compile pas (syntaxe Typst invalide) ne fait pas
//! échouer le rendu : [`TypstScreen::measure`] et [`TypstScreen::render`]
//! rendent `None`, et l'appelant ([`crate::backend::EguiBackend`]) retombe
//! sur le chemin ordinaire — le texte brut, `$...$` compris, s'affiche tel
//! quel plutôt que de faire disparaître tout le graphe. La même politique
//! que les références de série brisées ou les types de tracé inconnus
//! (`architecture-choisie.md` §11.1) : un défaut visible et localisé, jamais
//! un panic qui remonte.

use std::sync::Arc;

use egui::{Color32, ColorImage, TextureHandle, TextureOptions};
use plotters_backend::{text_anchor::Pos, FontTransform};
use plotyx_render::formula::{self, Compiled, FormulaCache};
use plotyx_render::FontChoice;
use typst::text::Font;
use typst::utils::Scalar;
use typst_render::RenderOptions;

/// Une formule déjà rasterisée pour l'écran.
///
/// `size_pt` est la taille **logique** (le même repère que le reste du
/// texte, mesuré à l'échelle fixe de [`plotyx_render::text`]) — pas la
/// taille en pixels de la texture, qui elle grandit avec `pixels_per_point`
/// pour rester nette sur un écran HiDPI, exactement comme un `Galley` reste
/// ancré à une taille logique inchangée pendant que son atlas est mis en
/// page plus finement (`plotyx_render::text`, tête de module).
pub struct FormulaTexture {
    handle: TextureHandle,
    size_pt: (f64, f64),
}

/// Ce qu'il faut pour dessiner un label Typst à l'écran — construit une
/// fois par [`crate::GraphView`], réutilisé à chaque image.
pub struct TypstScreen<'a> {
    context: &'a egui::Context,
    cache: &'a FormulaCache<FormulaTexture>,
    choice: &'a FontChoice,
    sans: Font,
    math: Font,
}

impl<'a> TypstScreen<'a> {
    /// Résout les polices une fois (`choice`) et assemble le contexte de
    /// dessin. À reconstruire seulement si `choice` change — comme
    /// [`plotyx_render::text::TextMeasurer`], pas à chaque image.
    pub fn new(
        context: &'a egui::Context,
        cache: &'a FormulaCache<FormulaTexture>,
        choice: &'a FontChoice,
    ) -> Result<Self, String> {
        let (sans, math) = formula::resolve_fonts(choice)?;
        Ok(Self {
            context,
            cache,
            choice,
            sans,
            math,
        })
    }

    /// Le `pixels_per_point` réel du `Context` — celui auquel [`Self::render`]
    /// doit rastériser pour rester net sur un écran HiDPI, à ne pas
    /// confondre avec l'échelle fixe utilisée par [`Self::measure`].
    pub fn pixels_per_point(&self) -> f32 {
        self.context.pixels_per_point()
    }

    /// La taille qu'occuperait `text` compilé par Typst, à `size_pt` de
    /// corps — l'invariant de la tâche 8.5. `None` si la formule ne
    /// compile pas.
    pub fn measure(&self, text: &str, size_pt: f32) -> Option<(u32, u32)> {
        let (sans, math) = formula::resolve_fonts(self.choice).ok()?;
        let compiled = formula::compile(text, size_pt, sans, math).ok()?;
        Some(formula::measure(&compiled))
    }

    /// La texture (compilée et rastérisée, ou reprise du cache) pour
    /// `text` à `size_pt` de corps, au `pixels_per_point` réel de l'écran.
    /// `None` si la formule ne compile pas.
    pub fn render(
        &self,
        text: &str,
        size_pt: f32,
        pixels_per_point: f32,
    ) -> Option<Arc<FormulaTexture>> {
        let context = self.context;
        let owned_text = text.to_owned();
        self.cache
            .get_or_try_insert(
                text,
                size_pt,
                self.choice,
                pixels_per_point,
                self.sans.clone(),
                self.math.clone(),
                move |compiled| Ok(rasterize(context, compiled, pixels_per_point, &owned_text)),
            )
            .ok()
    }
}

/// Rastérise une page compilée en texture `egui` — tâche 8.3.
fn rasterize(
    context: &egui::Context,
    compiled: &Compiled,
    pixels_per_point: f32,
    text: &str,
) -> FormulaTexture {
    let size_pt = compiled.size_pt();
    let Some(page) = compiled.page() else {
        // Formule vide : une texture 1×1 transparente plutôt qu'un cas
        // particulier de plus à traiter en aval — `Shape::Mesh` sur un
        // rectangle de taille nulle ne dessine simplement rien.
        let image = ColorImage::new([1, 1], vec![Color32::TRANSPARENT]);
        let handle = context.load_texture("plotyx-typst-empty", image, TextureOptions::LINEAR);
        return FormulaTexture {
            handle,
            size_pt: (0.0, 0.0),
        };
    };
    let opts = RenderOptions {
        pixel_per_pt: Scalar::new(pixels_per_point as f64),
        render_bleed: false,
    };
    let pixmap = typst_render::render(page, &opts);
    let image = ColorImage::from_rgba_premultiplied(
        [pixmap.width() as usize, pixmap.height() as usize],
        pixmap.data(),
    );
    // Un nom stable par contenu plutôt qu'un compteur : deux labels
    // identiques réutilisent la même entrée de l'atlas de textures d'egui
    // au lieu d'en allouer une par occurrence.
    let handle = context.load_texture(
        format!("plotyx-typst:{text}"),
        image,
        TextureOptions::LINEAR,
    );
    FormulaTexture { handle, size_pt }
}

/// Construit le `Shape::Mesh` d'une formule déjà rastérisée, ancrée et
/// tournée exactement comme l'aurait été un `Shape::Text` de même boîte —
/// même calcul que `EguiBackend::draw_text`, via les fonctions partagées de
/// `plotyx_render::formula`.
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

    // Les quatre coins du rectangle **non tourné**, ancré en `origin` — le
    // même repère que `TextShape::pos` (le coin supérieur gauche du texte
    // non tourné, `epaint::shapes::text_shape`).
    let corners = [
        egui::vec2(0.0, 0.0),
        egui::vec2(w, 0.0),
        egui::vec2(w, h),
        egui::vec2(0.0, h),
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
    // `add_rect_with_uv` construit un rectangle **axé sur les axes** ; on
    // remplace ses quatre sommets par les coins tournés calculés ci-dessus
    // plutôt que de reproduire la triangulation à la main.
    for (vertex, point) in m.vertices.iter_mut().zip(points.iter()) {
        vertex.pos = *point;
    }
    egui::Shape::Mesh(Arc::new(m))
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotyx_render::FontChoice;

    fn context() -> egui::Context {
        egui::Context::default()
    }

    /// Une formule triviale mesure une taille non nulle — le test de
    /// sanité minimal avant tout dessin.
    #[test]
    fn mesurer_une_formule_triviale_rend_une_taille_non_nulle() {
        let ctx = context();
        let cache = FormulaCache::new();
        let choice = FontChoice::default();
        let screen =
            TypstScreen::new(&ctx, &cache, &choice).expect("résolution des polices échouée");
        let (w, h) = screen
            .measure("$lambda$", 14.0)
            .expect("la formule aurait dû compiler");
        assert!(w > 0 && h > 0, "taille nulle : ({w}, {h})");
    }

    /// Une formule invalide dégrade en `None`, sans paniquer — la
    /// politique documentée en tête de module.
    #[test]
    fn une_formule_invalide_mesure_none() {
        let ctx = context();
        let cache = FormulaCache::new();
        let choice = FontChoice::default();
        let screen =
            TypstScreen::new(&ctx, &cache, &choice).expect("résolution des polices échouée");
        assert!(screen.measure("$ (unclosed", 14.0).is_none());
    }

    /// Rendre deux fois la même formule au même `pixels_per_point` sert la
    /// même texture depuis le cache — tâche 8.2, côté écran.
    #[test]
    fn rendre_deux_fois_la_meme_formule_reutilise_le_cache() {
        let ctx = context();
        let cache = FormulaCache::new();
        let choice = FontChoice::default();
        let screen =
            TypstScreen::new(&ctx, &cache, &choice).expect("résolution des polices échouée");
        let a = screen
            .render("$lambda$", 14.0, 1.0)
            .expect("échec du rendu");
        let b = screen
            .render("$lambda$", 14.0, 1.0)
            .expect("échec du rendu");
        assert_eq!(a.handle.id(), b.handle.id());
        assert_eq!(cache.len(), 1);
    }

    /// Le maillage produit occupe une boîte de la bonne taille, ancrée à
    /// l'origine attendue pour un ancrage `Left`/`Top` sans rotation — le
    /// cas le plus simple, qui aurait révélé un signe d'offset inversé.
    #[test]
    fn le_maillage_occupe_la_taille_de_la_texture_sans_rotation() {
        let ctx = context();
        let cache = FormulaCache::new();
        let choice = FontChoice::default();
        let screen =
            TypstScreen::new(&ctx, &cache, &choice).expect("résolution des polices échouée");
        let texture = screen
            .render("$lambda$", 14.0, 1.0)
            .expect("échec du rendu");
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
            panic!("un maillage était attendu")
        };
        let bounds = m.calc_bounds();
        assert!((bounds.min.x - 10.0).abs() < 0.01);
        assert!((bounds.min.y - 20.0).abs() < 0.01);
        assert!((bounds.width() - texture.size_pt.0 as f32).abs() < 0.01);
        assert!((bounds.height() - texture.size_pt.1 as f32).abs() < 0.01);
    }
}
