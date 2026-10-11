//! `DrawingBackend` de Plotters qui **collecte des `egui::Shape`** — tâche
//! R4.2.
//!
//! # La décision de conception : un `Vec<Shape>`, pas un `Painter`
//!
//! Ce backend ne tient pas de `Painter` : il écrit dans un `&mut Vec<Shape>`
//! qu'on lui prête. C'est la décision du §4.2 d'`architecture-choisie.md`,
//! et elle n'est pas cosmétique. egui est en mode immédiat : il repeint à
//! chaque mouvement de souris. Un backend qui dessinerait dans un `Painter`
//! rejouerait tout le pipeline Plotters — `ChartBuilder`, axes, graduations,
//! tous les points — à **chaque image**, même immobile. Avec un collecteur,
//! le résultat se met en cache ([`crate::cache`]) et une image sans
//! changement coûte un `painter.extend(...)`.
//!
//! `epaint` bénit ce stockage, avec une exception qu'il documente lui-même :
//! `Shape::Text` dépend du `pixels_per_point` et doit être reconstruit quand
//! il change. D'où la clé de cache à trois termes.
//!
//! Effet de bord bienvenu : `ensure_prepared` et `present` n'ont rien à
//! faire, là où un backend à texture ferait de `present` un point d'upload.
//!
//! # Les huit méthodes surchargées
//!
//! `DrawingBackend` n'exige que `get_size`, `ensure_prepared`, `present` et
//! `draw_pixel` ; tout le reste retombe sur une rastérisation pixel par
//! pixel. Pour un backend vectoriel on en surcharge huit, chacune tombant sur
//! une `Shape` presque en un pour un (`architecture-choisie.md` §4.1) :
//!
//! | plotters | egui |
//! |---|---|
//! | `draw_line` | `Shape::LineSegment` |
//! | `draw_path` | `Shape::Path` |
//! | `draw_rect` | `Shape::Rect` |
//! | `draw_circle` | `Shape::Circle` |
//! | `fill_polygon` | `Shape::Path` fermé |
//! | `draw_text` | `Shape::Text` |
//! | `estimate_text_size` | [`stokesforge_render::text::estimate`] |
//! | `blit_bitmap` | **laissée en défaut** — « pas de gestion d'images » |
//!
//! `draw_pixel` est implémentée quand même, parce que le trait l'exige : un
//! rectangle de 1×1. Rien de ce que R4 trace n'y passe (les huit surcharges
//! couvrent tout), mais la laisser paniquer transformerait une primitive non
//! prévue en plantage de l'application.

use egui::epaint::{CircleShape, PathShape, RectShape, TextShape};
use egui::{Color32, CornerRadius, Pos2, Shape, Stroke, StrokeKind, Vec2};
use plotters_backend::{
    text_anchor::{HPos, VPos},
    BackendColor, BackendCoord, BackendStyle, BackendTextStyle, DrawingBackend, DrawingErrorKind,
    FontTransform,
};
use stokesforge_render::text::{estimate, font_id, TextSource};

/// L'erreur que ce backend peut rendre — aucune.
///
/// Collecter dans un `Vec` ne peut pas échouer. Le type existe parce que
/// `DrawingBackend::ErrorType` l'exige, et il est volontairement inhabitable
/// plutôt que d'être un alias d'une erreur d'entrée-sortie qui ne se
/// produira jamais : la signature dit alors la vérité.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EguiBackendError {}

impl std::fmt::Display for EguiBackendError {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {}
    }
}

impl std::error::Error for EguiBackendError {}

/// Backend Plotters qui empile des [`egui::Shape`].
///
/// `origin` place le coin supérieur gauche de la figure dans le repère de
/// l'écran : Plotters travaille toujours en `0..width`, `0..height`, et la
/// translation est appliquée une fois, à la construction de chaque forme,
/// plutôt qu'après coup sur tout le vecteur. Le cache contient donc des
/// formes déjà à leur place, et une image sans changement n'a rien à
/// recalculer du tout.
pub struct EguiBackend<'a> {
    shapes: &'a mut Vec<Shape>,
    origin: Pos2,
    size: (u32, u32),
    text: &'a dyn TextSource,
    /// Chemin Typst de l'écran (tâche R8.3) — `None` tant que personne n'a
    /// appelé [`Self::with_typst`] : construire ce backend ne suffit pas à
    /// activer Typst. [`crate::GraphView`] l'active toujours (issue #29) ;
    /// un backend sans lui sert de témoin aux tests.
    typst: Option<crate::typst_screen::TypstScreen<'a>>,
}

impl<'a> EguiBackend<'a> {
    /// Un backend qui écrira dans `shapes`, décalé de `origin`.
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

    /// Un backend qui remplit exactement le rectangle donné.
    ///
    /// Les dimensions sont arrondies **vers le bas** : Plotters raisonne en
    /// entiers et une figure d'un pixel de trop déborderait du rectangle
    /// alloué par l'UI.
    pub fn in_rect(shapes: &'a mut Vec<Shape>, rect: egui::Rect, text: &'a dyn TextSource) -> Self {
        let size = (rect.width().max(1.0) as u32, rect.height().max(1.0) as u32);
        Self::new(shapes, rect.min, size, text)
    }

    /// Active le chemin Typst (tâche R8.3) sur ce backend : les textes en
    /// style riche ([`stokesforge_render::formula::is_rich`] — titres, titres
    /// d'axe, légende, issue #32) passent par `typst`, les graduations
    /// restent en `Shape::Text`. Additif : les
    /// appelants existants (`in_rect`, `new`) sont inchangés, et un backend
    /// construit sans cet appel dessine tout en texte brut, comme avant R8.
    pub fn with_typst(mut self, typst: crate::typst_screen::TypstScreen<'a>) -> Self {
        self.typst = Some(typst);
        self
    }

    /// Passe d'un point de Plotters au repère de l'écran.
    fn at(&self, (x, y): BackendCoord) -> Pos2 {
        Pos2::new(self.origin.x + x as f32, self.origin.y + y as f32)
    }

    /// Le trait d'une forme, ou [`Stroke::NONE`] si le style est transparent.
    fn stroke<S: BackendStyle>(style: &S) -> Stroke {
        Stroke {
            width: style.stroke_width().max(1) as f32,
            color: color(style.color()),
        }
    }
}

/// Une couleur de Plotters en [`egui::Color32`].
///
/// `from_rgba_unmultiplied` et non `from_rgba_premultiplied` : l'alpha de
/// Plotters est une opacité indépendante des composantes.
fn color(c: BackendColor) -> Color32 {
    let (r, g, b) = c.rgb;
    let alpha = (c.alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

/// L'angle, en radians horaires, d'une rotation de Plotters.
///
/// `FontTransform` ne vaut que 0/90/180/270 alors que `TextShape::angle` est
/// un `f32` libre : la conversion est triviale, et c'est l'une des raisons
/// pour lesquelles écrire ce backend coûte moins cher que d'en importer un
/// (`architecture-choisie.md` §4.1).
fn angle_of(transform: &FontTransform) -> f32 {
    match transform {
        FontTransform::None => 0.0,
        FontTransform::Rotate90 => std::f32::consts::FRAC_PI_2,
        FontTransform::Rotate180 => std::f32::consts::PI,
        FontTransform::Rotate270 => 3.0 * std::f32::consts::FRAC_PI_2,
    }
}

/// Applique à un décalage la même rotation que
/// [`FontTransform::transform`] applique aux glyphes.
///
/// Nécessaire pour que l'ancrage (`HPos`/`VPos`) reste juste sur un texte
/// tourné : Plotters tourne le décalage d'ancrage en même temps que le
/// texte, et ne pas le faire mettrait le titre de l'axe de gauche à côté de
/// sa place. Transcrit sur l'implémentation par défaut de `draw_text`, pas
/// déduit.
fn rotate(offset: Vec2, transform: &FontTransform) -> Vec2 {
    match transform {
        FontTransform::None => offset,
        FontTransform::Rotate90 => Vec2::new(-offset.y, offset.x),
        FontTransform::Rotate180 => Vec2::new(-offset.x, -offset.y),
        FontTransform::Rotate270 => Vec2::new(offset.y, -offset.x),
    }
}

/// Couleur du cadre d'un texte riche que Typst refuse (#240, T3).
const COMPILE_ERROR_FRAME: Color32 = Color32::from_rgb(214, 48, 49);

impl DrawingBackend for EguiBackend<'_> {
    type ErrorType = EguiBackendError;

    fn get_size(&self) -> (u32, u32) {
        self.size
    }

    /// Rien à préparer : il n'y a pas de tampon à allouer, pas de contexte
    /// graphique à ouvrir. Voir la doc de tête.
    fn ensure_prepared(&mut self) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        Ok(())
    }

    /// Rien à présenter : les formes sont déjà dans le vecteur de
    /// l'appelant, qui décidera quand les peindre.
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
        // `from_two_pos` plutôt que `from_min_max` : Plotters ne garantit pas
        // que le premier coin soit le plus petit (un axe inversé peut rendre
        // un rectangle « à l'envers »), et un `Rect` dégénéré d'egui ne se
        // peint pas.
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

    /// La méthode chaude : c'est par elle que passent les séries.
    ///
    /// Un seul `Shape::Path` pour toute la polyligne, au lieu des N
    /// `LineSegment` que produirait l'implémentation par défaut : c'est ce
    /// qui rend le cache intéressant, et ce que la mesure R0 0.3 a chiffré
    /// (300 000 points = 2,432 Mo de cache vectoriel, indépendant de la
    /// résolution).
    fn draw_path<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        path: I,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        if style.color().alpha == 0.0 {
            return Ok(());
        }
        // Un chemin sous-pixel (`stokesforge_render::subpixel`, #91) arrive en
        // points fixes avec un drapeau dans l'épaisseur.
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

    /// Polygone plein.
    ///
    /// `PathShape::convex_polygon` : egui ne sait remplir que les polygones
    /// convexes, et un polygone concave y rendrait faux **sans erreur**. Ce
    /// n'est pas un risque en R4 — `draw_path` étant surchargée, Plotters
    /// n'appelle plus cette méthode pour épaissir un trait, et rien d'autre
    /// dans le graphe minimal n'a de polygone — mais ça le deviendra en R5
    /// avec les barres et les aires. **À reprendre à ce moment-là** :
    /// trianguler soi-même, ou passer par un `Mesh`.
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

    /// Texte en `Shape::Text`, ou par Typst pour un texte riche.
    ///
    /// Les **graduations** ne passent jamais par Typst : ce serait une
    /// compilation par graduation (`architecture-choisie.md` §4.3). Titres,
    /// titres d'axe et légende, si (issue #32) — le pipeline les marque par
    /// leur style. Pour un texte ordinaire, le
    /// `Galley` obtenu ici est celui-là même que
    /// [`DrawingBackend::estimate_text_size`] mesure, ci-dessous : la mesure
    /// et le dessin ne sont pas « cohérents », ils sont le même objet.
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

        // Un texte riche (`stokesforge_render::formula::is_rich`, issue #32)
        // passe par la texture Typst plutôt que par le `Galley` egui.
        // Dégrade silencieusement vers le chemin ordinaire s'il ne compile
        // pas ou que Typst n'est pas activé sur ce backend (`typst_screen`,
        // tête de module) — jamais d'erreur remontée pour un label malformé.
        // Un texte riche que Typst refuse est dessiné en brut **dans un cadre
        // rouge** (#240, T3) : repère d'écran, absent des exports (backend SVG).
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
            // Le cadre suit la rotation du texte (titre de l'axe vertical).
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

    /// La mesure — tâche R4.1, et le même appel que dans
    /// `stokesforge-render-svg`.
    fn estimate_text_size<S: BackendTextStyle>(
        &self,
        text: &str,
        style: &S,
    ) -> Result<(u32, u32), DrawingErrorKind<Self::ErrorType>> {
        // R8.5 — **l'invariant du chantier** : un texte riche se mesure en
        // interrogeant Typst, jamais l'heuristique `epaint` (qui mesurerait
        // le texte source lui-même, pas ce que Typst en compose). La même
        // condition que `draw_text`, au même endroit.
        // Dégrade vers la mesure `epaint` si Typst n'est pas activé ou que
        // la formule ne compile pas — la même politique que `draw_text`,
        // pour que la mesure et le dessin restent d'accord sur ce qu'ils
        // font (`architecture-choisie.md` §4.3).
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

    // `blit_bitmap` est laissée en défaut : le cahier des charges dit « pas
    // de gestion d'images » (`architecture-choisie.md` §2). Le défaut du
    // trait la rastérise pixel par pixel, ce qui est lent et correct — la
    // bonne façon de traiter ce qui ne doit pas arriver.
}
