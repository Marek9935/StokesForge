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
use stokesforge_render::fonts::{FontSet, FontSource};
use stokesforge_render::text::font_size;
use stokesforge_render::FontChoice;

use crate::typst_export::SvgTextMode;
use stokesforge_render::text::{estimate, TextMeasurer};

/// `SVGBackend`, augmenté de la mesure de texte partagée du chantier.
///
/// L'enveloppe est fine par construction : tout ce qui dessine est délégué
/// tel quel, y compris `present`, dont le `SVGBackend` se sert pour refermer
/// le document. Ce qui est écrit ici tient en une méthode.
pub struct MeasuredSvgBackend<'a> {
    inner: SVGBackend<'a>,
    measurer: &'a TextMeasurer,
    /// Chemin Typst de l'export (tâche R8.4) — une référence, pas une
    /// valeur possédée : [`crate::typst_export::TypstExport`] doit rester
    /// lisible par l'appelant **après** que ce backend a été abandonné
    /// (`crate::render_to_string_with_typst`), pour que
    /// [`crate::typst_export::splice`] retrouve les témoins enregistrés
    /// pendant le tracé. Voir la doc de [`Self::with_typst`].
    typst: Option<&'a crate::typst_export::TypstExport<'a>>,
    /// Ce qui fait écrire les graduations dans la police du graphe (#162) :
    /// la valeur de `font-family` et le mode de texte. `None` : le
    /// `font-family="sans-serif"` générique de Plotters.
    font: Option<(String, SvgTextMode)>,
}

/// Un style de texte qui n'en change qu'un trait : la famille écrite dans
/// `font-family`. Tout le reste est celui du style d'origine.
struct NamedFamily<'s, S> {
    style: &'s S,
    family: &'s str,
}

impl<S: BackendTextStyle> BackendTextStyle for NamedFamily<'_, S> {
    type FontError = S::FontError;

    fn color(&self) -> BackendColor {
        self.style.color()
    }

    fn size(&self) -> f64 {
        self.style.size()
    }

    fn transform(&self) -> plotters_backend::FontTransform {
        self.style.transform()
    }

    fn style(&self) -> plotters_backend::FontStyle {
        self.style.style()
    }

    fn anchor(&self) -> plotters_backend::text_anchor::Pos {
        self.style.anchor()
    }

    fn family(&self) -> plotters_backend::FontFamily<'_> {
        plotters_backend::FontFamily::Name(self.family)
    }

    fn layout_box(&self, text: &str) -> Result<((i32, i32), (i32, i32)), Self::FontError> {
        self.style.layout_box(text)
    }

    fn draw<E, DrawFunc: FnMut(i32, i32, BackendColor) -> Result<(), E>>(
        &self,
        text: &str,
        pos: BackendCoord,
        draw: DrawFunc,
    ) -> Result<Result<(), E>, Self::FontError> {
        self.style.draw(text, pos, draw)
    }
}

/// La valeur de `font-family` qui nomme la police du graphe, suivie d'une
/// famille générique de repli (#162) : « 'Noto Sans', sans-serif ». Une police
/// système introuvable est repliée sur Noto, comme à l'écran.
fn css_family(choice: &FontChoice) -> String {
    let name = match stokesforge_render::fonts::resolve(choice).source {
        FontSource::Embedded(set) => set.sans_family_name().to_owned(),
        FontSource::System(name) => name,
        FontSource::FallenBackToNoto { .. } => FontSet::Noto.sans_family_name().to_owned(),
    };
    format!(
        "'{}', sans-serif",
        name.replace(['\'', '"', '&', '<', '>'], "")
    )
}

impl<'a> MeasuredSvgBackend<'a> {
    /// Enveloppe un `SVGBackend` déjà construit.
    pub fn new(inner: SVGBackend<'a>, measurer: &'a TextMeasurer) -> Self {
        Self {
            inner,
            measurer,
            typst: None,
            font: None,
        }
    }

    /// Un backend qui écrit dans la `String` donnée.
    pub fn with_string(
        buffer: &'a mut String,
        size: (u32, u32),
        measurer: &'a TextMeasurer,
    ) -> Self {
        Self::new(SVGBackend::with_string(buffer, size), measurer)
    }

    /// Active le chemin Typst (tâche R8.4) sur ce backend : les textes en
    /// style riche ([`stokesforge_render::formula::is_rich`], issue #32) rendent un
    /// témoin substitué après coup par [`crate::typst_export::splice`],
    /// tous les autres restent le `<text>` ordinaire de `plotters-svg`.
    /// Additif comme `EguiBackend::with_typst` : `new`/`with_string`
    /// restent inchangés, tâche 8.6.
    pub fn with_typst(mut self, typst: &'a crate::typst_export::TypstExport<'a>) -> Self {
        self.typst = Some(typst);
        self
    }

    /// Les graduations dans la police du graphe (#162) : en mode
    /// [`SvgTextMode::Text`], elles portent la même famille que les textes
    /// Typst ; en mode [`SvgTextMode::Outlines`], elles passent par Typst et
    /// sortent en contours, pour que le document reste homogène.
    pub fn with_font(mut self, choice: &FontChoice, mode: SvgTextMode) -> Self {
        self.font = Some((css_family(choice), mode));
        self
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
        // R8.4 : un texte riche (issue #32) pose un rectangle-témoin à la place du
        // `<text>` ordinaire ; `crate::typst_export::splice` le remplace
        // après coup par le fragment `typst-svg` — voir
        // `crate::typst_export`, tête de module, pour le mécanisme complet
        // et pourquoi il n'a pas besoin de toucher `SVGBackend`. Dégrade
        // vers le `<text>` ordinaire si Typst n'est pas activé ou que la
        // formule ne compile pas.
        if let Some(typst) = &self.typst {
            if stokesforge_render::formula::is_rich(style) {
                // Un texte transparent ne dessine rien, comme le `<text>` de
                // `plotters-svg` et le chemin écran : pas de témoin, donc
                // pas de fragment Typst (qui, lui, ignorerait l'opacité
                // nulle du style et serait visible).
                if style.color().alpha == 0.0 {
                    return Ok(());
                }
                let size_pt = font_size(style.size());
                if let Some(witness) = typst.record(
                    &stokesforge_render::formula::source(text, style),
                    size_pt,
                    stokesforge_render::formula::ink(style.color()),
                    style.anchor(),
                    style.transform(),
                    (pos.0 as f64, pos.1 as f64),
                ) {
                    // Le témoin n'a besoin d'être ni bien placé ni bien
                    // dimensionné dans l'absolu : `splice`
                    // (`crate::typst_export`) le repère par sa couleur
                    // *propre* à cet appel et revérifie sa géométrie contre
                    // celle enregistrée par `record` ci-dessus — donc la
                    // position et la taille passées ici doivent être
                    // exactement celles que `record` a calculées, pas
                    // d'approximation possible des deux côtés.
                    let (w, h) = witness.size;
                    return self.inner.draw_rect(
                        (pos.0, pos.1),
                        (pos.0 + w, pos.1 + h),
                        &witness.color,
                        true,
                    );
                }
            }
        }
        match &self.font {
            // Graduations en contours : composées par Typst, comme un texte
            // littéral (échappé), dans la police du graphe.
            Some((_, SvgTextMode::Outlines)) if self.typst.is_some() => {
                if style.color().alpha == 0.0 {
                    return Ok(());
                }
                if let Some(witness) = self.typst.and_then(|typst| {
                    typst.record(
                        &stokesforge_render::formula::escape(text),
                        font_size(style.size()),
                        stokesforge_render::formula::ink(style.color()),
                        style.anchor(),
                        style.transform(),
                        (pos.0 as f64, pos.1 as f64),
                    )
                }) {
                    let (w, h) = witness.size;
                    return self.inner.draw_rect(
                        (pos.0, pos.1),
                        (pos.0 + w, pos.1 + h),
                        &witness.color,
                        true,
                    );
                }
                self.inner.draw_text(text, style, pos)
            }
            Some((family, _)) => {
                let named = NamedFamily { style, family };
                self.inner.draw_text(text, &named, pos)
            }
            None => self.inner.draw_text(text, style, pos),
        }
    }

    /// **La seule méthode qui n'est pas déléguée** — tâche R4.4.
    ///
    /// Même fonction, littéralement, que celle qu'appelle le backend egui :
    /// [`stokesforge_render::text::estimate`]. Pas « la même formule », pas « la
    /// même source » : le même appel. C'est ce qui fait que l'accord des
    /// deux backends (R4.5) se vérifie au lieu de s'espérer.
    ///
    /// Les graduations s'écrivent dans la police du graphe
    /// ([`Self::with_font`], #162) : la famille nommée est celle que la mesure
    /// utilise, au lieu du `sans-serif` générique de Plotters. Sans
    /// `with_font`, l'asymétrie ancienne demeure (mesure sur la police
    /// embarquée, affichage sur celle du lecteur).
    fn estimate_text_size<S: BackendTextStyle>(
        &self,
        text: &str,
        style: &S,
    ) -> Result<(u32, u32), DrawingErrorKind<Self::ErrorType>> {
        // R8.5 — le même invariant que côté écran (`EguiBackend`) : un
        // texte riche se mesure en interrogeant Typst, jamais l'heuristique
        // `epaint`/naïve. Dégrade vers la mesure `epaint` si Typst n'est
        // pas activé ou que la formule ne compile pas.
        if let Some(typst) = &self.typst {
            if stokesforge_render::formula::is_rich(style) {
                let size_pt = font_size(style.size());
                let ink = stokesforge_render::formula::ink(style.color());
                let source = stokesforge_render::formula::source(text, style);
                if let Some(size) = typst.measure(&source, size_pt, ink) {
                    return Ok(size);
                }
            }
        }
        Ok(estimate(self.measurer, text, style))
    }
}
