//! Le `SVGBackend` de Plotters, **avec la mesure de texte du projet**.
//!
//! # Pourquoi une enveloppe plutôt qu'un backend écrit à la main
//!
//! `plotters-svg` fait déjà tout ce qu'il faut : ~800 lignes qui émettent du
//! SVG correct, maintenues en amont, et le §5 de `architecture-choisie.md`
//! les retient explicitement. Il ne lui manque **qu'une** méthode, et c'est
//! précisément celle qui compte.
//!
//! # Le piège documenté, et pourquoi il n'est pas optionnel de l'éviter
//!
//! `SVGBackend` ne surcharge pas `estimate_text_size`. Sans rien faire, il
//! retombe sur l'implémentation par défaut du trait → `layout_box` → et,
//! nos fonts Plotters étant coupées (`default-features = false`), sur
//! `plotters/src/style/font/naive.rs` :
//!
//! ```text
//! let em = size / 1.24 / 1.24;
//! // largeur ≈ em * 0.7 * text.len()
//! ```
//!
//! Une largeur **monospace estimée sur le nombre d'octets UTF-8** : « λ »
//! compte pour 2, « µm » pour 3, « 111 » mesure comme « WWW ». Les marges
//! réservées aux libellés seraient fausses, donc la zone de tracé différente
//! de celle de l'écran : *la figure exportée ne serait pas la figure vue*, et
//! rien ne le signalerait. C'est le seul défaut silencieux du chantier
//! (`architecture-choisie.md` §4.4).
//!
//! Un trait Rust ne se surcharge pas depuis l'extérieur : la seule façon de
//! remplacer une méthode de `DrawingBackend` sur un type d'un autre crate est
//! de l'envelopper. D'où [`MeasuredSvgBackend`], qui délègue les dix méthodes
//! de dessin à `SVGBackend` et **n'en réimplémente qu'une**. Le test
//! `tests/mesure.rs` vérifie que la surcharge est réellement en vigueur, en
//! comparant à ce qu'aurait rendu l'heuristique naïve : sans ce témoin, une
//! enveloppe qui déléguerait *aussi* `estimate_text_size` passerait pour
//! correcte.

use plotters::prelude::*;
use plotters_backend::{
    BackendColor, BackendCoord, BackendStyle, BackendTextStyle, DrawingErrorKind,
};
use plotyx_render::text::{estimate, TextMeasurer};

/// `SVGBackend`, augmenté de la mesure de texte partagée du chantier.
///
/// L'enveloppe est fine par construction : tout ce qui dessine est délégué
/// tel quel, y compris `present`, dont le `SVGBackend` se sert pour refermer
/// le document. Ce qui est écrit ici tient en une méthode.
pub struct MeasuredSvgBackend<'a> {
    inner: SVGBackend<'a>,
    measurer: &'a TextMeasurer,
}

impl<'a> MeasuredSvgBackend<'a> {
    /// Enveloppe un `SVGBackend` déjà construit.
    pub fn new(inner: SVGBackend<'a>, measurer: &'a TextMeasurer) -> Self {
        Self { inner, measurer }
    }

    /// Un backend qui écrit dans la `String` donnée.
    pub fn with_string(
        buffer: &'a mut String,
        size: (u32, u32),
        measurer: &'a TextMeasurer,
    ) -> Self {
        Self::new(SVGBackend::with_string(buffer, size), measurer)
    }
}

impl DrawingBackend for MeasuredSvgBackend<'_> {
    type ErrorType = <SVGBackend<'static> as DrawingBackend>::ErrorType;

    fn get_size(&self) -> (u32, u32) {
        self.inner.get_size()
    }

    fn ensure_prepared(&mut self) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.inner.ensure_prepared()
    }

    fn present(&mut self) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.inner.present()
    }

    fn draw_pixel(
        &mut self,
        point: BackendCoord,
        color: BackendColor,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.inner.draw_pixel(point, color)
    }

    fn draw_line<S: BackendStyle>(
        &mut self,
        from: BackendCoord,
        to: BackendCoord,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.inner.draw_line(from, to, style)
    }

    fn draw_rect<S: BackendStyle>(
        &mut self,
        upper_left: BackendCoord,
        bottom_right: BackendCoord,
        style: &S,
        fill: bool,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.inner.draw_rect(upper_left, bottom_right, style, fill)
    }

    fn draw_path<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        path: I,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.inner.draw_path(path, style)
    }

    fn draw_circle<S: BackendStyle>(
        &mut self,
        center: BackendCoord,
        radius: u32,
        style: &S,
        fill: bool,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.inner.draw_circle(center, radius, style, fill)
    }

    fn fill_polygon<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        vert: I,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.inner.fill_polygon(vert, style)
    }

    fn draw_text<S: BackendTextStyle>(
        &mut self,
        text: &str,
        style: &S,
        pos: BackendCoord,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.inner.draw_text(text, style, pos)
    }

    /// **La seule méthode qui n'est pas déléguée** — tâche R4.4.
    ///
    /// Même fonction, littéralement, que celle qu'appelle le backend egui :
    /// [`plotyx_render::text::estimate`]. Pas « la même formule », pas « la
    /// même source » : le même appel. C'est ce qui fait que l'accord des
    /// deux backends (R4.5) se vérifie au lieu de s'espérer.
    ///
    /// Reste l'asymétrie connue, qui n'est pas de notre ressort ici : le SVG
    /// émet `<text font-family="sans-serif">`, donc les glyphes réellement
    /// affichés dépendent de la font installée chez le lecteur, alors que la
    /// mesure, elle, est faite sur nos polices embarquées. C'est le point
    /// ouvert n°6 d'`architecture-choisie.md`, à trancher avant le premier
    /// export destiné à une publication — et il ne se règle pas ici mais en
    /// décidant d'outliner, ou non, le texte de l'export.
    fn estimate_text_size<S: BackendTextStyle>(
        &self,
        text: &str,
        style: &S,
    ) -> Result<(u32, u32), DrawingErrorKind<Self::ErrorType>> {
        Ok(estimate(self.measurer, text, style))
    }
}
