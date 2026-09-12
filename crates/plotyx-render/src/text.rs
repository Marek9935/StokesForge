//! **La source unique de mesure de texte** — tâche R4.1.
//!
//! Tout ce qui, dans le chantier de rendu, répond à la question « quelle
//! place prend cette chaîne ? » passe par ici, quel que soit le backend qui
//! pose la question. C'est l'invariant non négociable du §4.3 de
//! `architecture-choisie.md`, et le seul endroit du chantier où un bug est
//! **silencieux** : un désaccord de mesure décale les axes entre l'écran et
//! l'export sans lever la moindre erreur.
//!
//! # Ce qui est partagé, et ce qui ne l'est pas
//!
//! La mesure R0 0.1 (2026-09-10) a tranché : ce sont les
//! [`epaint::FontDefinitions`] qu'il faut partager, **pas** l'instance
//! [`epaint::Fonts`] — qui n'est de toute façon ni `Clone` ni utilisable par
//! `&self`. Le danger réel n'est pas « deux instances », c'est « deux jeux de
//! polices ». Un test de dix lignes suffit à le garder, et il est écrit
//! (`tests/mesure.rs`).
//!
//! # Correction apportée par R4 : la hauteur, elle, dépend du
//! `pixels_per_point`
//!
//! La précision 1 de la mesure R0 annonçait les métriques indépendantes du
//! `pixels_per_point`. **C'est vrai des avances, faux de la hauteur de
//! ligne**, et R4 s'en est aperçu en écrivant le test qui devait
//! l'entériner. `epaint` cale la hauteur d'une ligne sur la grille de pixels
//! *physiques* (`text_layout.rs` : `max_row_height =
//! point_scale.round_to_pixel(max_row_height)`), donc sur une quantité qui
//! change avec l'écran. Mesuré : à 9,7 pt de corps, un galley fait 11 points
//! de haut à `pixels_per_point = 1,0` et 12 à 1,25. R0 ne pouvait pas le
//! voir : elle comparait des avances de glyphes, pas des rectangles de
//! galley.
//!
//! La conséquence serait réelle et exactement du type que la phase existe
//! pour empêcher : `DrawingArea::titled` fait descendre la zone de tracé de
//! la hauteur du titre, donc **la figure exportée dépendrait de l'écran de
//! celui qui l'exporte**.
//!
//! D'où la règle, tenue par construction plutôt que par vigilance : **on
//! mesure toujours à [`MEASUREMENT_PIXELS_PER_POINT`]**, quel que soit
//! l'écran ; le `pixels_per_point` réel ne sert qu'à *dessiner*. C'est la
//! raison d'être des deux méthodes de [`TextSource`], et le prix payé est
//! d'au plus un pixel physique entre la boîte réservée et les glyphes
//! affichés sur un écran HiDPI — invisible, et sans effet sur la géométrie,
//! puisque le texte est ancré et non étiré.
//!
//! # Ce qui n'est pas ici
//!
//! Typst. Le texte ordinaire — chiffres des graduations, titres d'axe —
//! passe par les fonts egui et **jamais** par Typst : ce serait une
//! compilation Typst par graduation (`architecture-choisie.md` §4.3). La
//! tâche R8.5 ajoutera un aiguillage dans [`TextSource::measure`], pas
//! ailleurs.

use std::sync::{Arc, Mutex};

use epaint::text::{FontDefinitions, FontFamily, FontId, Fonts, TextOptions};
use epaint::{Color32, Galley, Vec2};
use plotters_backend::{BackendTextStyle, FontFamily as PlottersFamily};

/// L'échelle à laquelle **toute** mesure de mise en page est faite.
///
/// Fixée à 1,0 et non « celle de l'écran » : voir la correction en tête de
/// module. Une figure ne doit pas changer de géométrie parce que
/// l'utilisateur a branché un second écran.
pub const MEASUREMENT_PIXELS_PER_POINT: f32 = 1.0;

/// Facteur entre la « taille » d'une police au sens de Plotters et sa taille
/// en points au sens d'epaint et du SVG.
///
/// Ce n'est pas une constante de confort : c'est **exactement** le diviseur
/// que `plotters-svg` écrit dans l'attribut `font-size` (`svg.rs` :
/// `("font-size", format!("{}", style.size() / 1.24))`). Mesurer à la même
/// taille que celle inscrite dans le SVG est ce qui fait que la boîte
/// réservée correspond aux glyphes réellement affichés — sinon la figure
/// exportée réserverait juste, puis dessinerait faux.
pub const PLOTTERS_SIZE_PER_POINT: f64 = 1.24;

/// Taille de police d'epaint, en points, pour une taille au sens de Plotters.
pub fn font_size(plotters_size: f64) -> f32 {
    (plotters_size / PLOTTERS_SIZE_PER_POINT) as f32
}

/// **La** définition des polices du projet, en un seul endroit.
///
/// C'est `FontDefinitions::default()` d'epaint, c'est-à-dire exactement ce
/// qu'un `egui::Context` neuf utilise : la mesure hors écran (export SVG,
/// CLI, script) part donc du même jeu de polices que la fenêtre, ce que la
/// mesure R0 0.1 a vérifié à 0,000000 px d'écart sur 540 comparaisons.
///
/// Pour que ce soit une propriété et non une coïncidence, l'application
/// pose explicitement ces définitions sur son `Context` au démarrage
/// (`plotyx-app`) : si egui changeait un jour son défaut, la figure
/// resterait la même des deux côtés.
///
/// **Ne pas appeler en boucle** : elle décode les polices embarquées. Un
/// [`TextMeasurer`] par backend, construit une fois, suffit.
pub fn font_definitions() -> FontDefinitions {
    FontDefinitions::default()
}

/// Les options de rendu de texte du projet.
///
/// Volontairement les valeurs par défaut d'epaint. Les fixer ici plutôt que
/// de les laisser à l'appelant évite qu'un backend ne prenne un jour les
/// siennes et croie mesurer comme l'autre.
pub fn text_options() -> TextOptions {
    TextOptions::default()
}

/// Traduit une famille et une taille de Plotters en [`epaint::FontId`].
///
/// Les fonts par défaut d'epaint n'offrent que deux familles réelles :
/// proportionnelle et monospace. `Serif`, `SansSerif` et toute famille
/// nommée tombent donc sur la proportionnelle — un repli assumé plutôt qu'un
/// échec, et de toute façon identique des deux côtés, ce qui est la seule
/// chose dont dépend l'accord des axes.
pub fn font_id<S: BackendTextStyle>(style: &S) -> FontId {
    let family = match style.family() {
        PlottersFamily::Monospace => FontFamily::Monospace,
        PlottersFamily::Serif | PlottersFamily::SansSerif | PlottersFamily::Name(_) => {
            FontFamily::Proportional
        }
    };
    FontId::new(font_size(style.size()), family)
}

/// Arrondi d'une taille de galley vers le `(u32, u32)` que Plotters attend.
///
/// Vers le haut : une boîte réservée trop petite tronque le libellé, une
/// boîte trop grande ne coûte qu'un pixel de marge. Fonction partagée pour
/// que l'arrondi lui-même ne puisse pas diverger entre les deux backends.
pub fn round_size(size: Vec2) -> (u32, u32) {
    (size.x.max(0.0).ceil() as u32, size.y.max(0.0).ceil() as u32)
}

/// Ce qui sait mettre un texte en page — pour le mesurer, et pour le
/// dessiner.
///
/// Les deux méthodes sont distinctes **à dessein**, et c'est le seul endroit
/// du chantier où la distinction compte :
///
/// - [`measure`](TextSource::measure) décide de la **géométrie** de la
///   figure. Elle doit rendre le même nombre partout : même machine ou non,
///   même écran ou non, écran ou export. Toute implémentation la calcule à
///   [`MEASUREMENT_PIXELS_PER_POINT`] ;
/// - [`layout`](TextSource::layout) produit le `Galley` qui sera **dessiné**.
///   Celui-là doit être mis en page au `pixels_per_point` de la cible, sans
///   quoi le texte serait flou ; et, quand il y a un `egui::Context`, il doit
///   venir de lui, parce qu'un `Galley` porte les coordonnées UV de ses
///   glyphes **dans l'atlas qui l'a mis en page** — un `Shape::Text`
///   construit sur un `Galley` étranger serait tesselé contre l'atlas du
///   `Context` et afficherait n'importe quoi.
///
/// Ce que les deux méthodes doivent avoir en commun, et c'est tout, ce sont
/// les [`font_definitions`].
pub trait TextSource {
    /// La place que prend `text`, en points, à l'échelle de mesure.
    fn measure(&self, text: &str, font_id: FontId) -> Vec2;

    /// Le `Galley` à dessiner, à l'échelle d'affichage de la cible.
    fn layout(&self, text: &str, font_id: FontId, color: Color32) -> Arc<Galley>;
}

/// La mesure, telle que `DrawingBackend::estimate_text_size` doit la rendre.
///
/// Les deux backends appellent **cette fonction-ci**, pas une transcription
/// chacun de leur côté : c'est ce qui rend la tâche R4.5 vérifiable plutôt
/// qu'espérée.
pub fn estimate<T: TextSource + ?Sized, S: BackendTextStyle>(
    source: &T,
    text: &str,
    style: &S,
) -> (u32, u32) {
    round_size(source.measure(text, font_id(style)))
}

/// Une instance d'[`epaint::Fonts`] hors `Context`, prête à mesurer et à
/// mettre en page.
///
/// Construite sur [`font_definitions`] et [`text_options`], donc sur la même
/// source que l'écran. Le `Mutex` n'est pas une précaution de concurrence
/// mais une nécessité de signature : `Fonts::layout` prend `&mut self` alors
/// que `DrawingBackend::estimate_text_size` prend `&self`. Le coût est sans
/// rapport avec celui de la mise en page elle-même, et le cache de galleys
/// d'epaint amortit les graduations répétées.
pub struct TextMeasurer {
    fonts: Mutex<Fonts>,
    pixels_per_point: f32,
}

impl TextMeasurer {
    /// Une source de texte dont les `Galley` seront mis en page à
    /// `pixels_per_point`.
    ///
    /// Ce paramètre ne concerne **que** le dessin : la mesure se fait
    /// toujours à [`MEASUREMENT_PIXELS_PER_POINT`].
    pub fn new(pixels_per_point: f32) -> Self {
        Self {
            fonts: Mutex::new(Fonts::new(text_options(), font_definitions())),
            pixels_per_point,
        }
    }

    /// L'échelle de mise en page des `Galley` dessinés par cette source.
    pub fn pixels_per_point(&self) -> f32 {
        self.pixels_per_point
    }

    /// Met en page à une échelle donnée.
    fn layout_at(
        &self,
        pixels_per_point: f32,
        text: &str,
        font_id: FontId,
        color: Color32,
    ) -> Arc<Galley> {
        // Verrou empoisonné : un `layout` ne peut paniquer que sur un défaut
        // d'epaint, et refuser de mesurer ferait échouer un export pour une
        // raison que l'appelant ne peut pas corriger. On reprend la donnée
        // telle quelle.
        let mut fonts = self.fonts.lock().unwrap_or_else(|err| err.into_inner());
        fonts
            .with_pixels_per_point(pixels_per_point)
            .layout_no_wrap(text.to_owned(), font_id, color)
    }
}

impl Default for TextMeasurer {
    /// À `pixels_per_point = 1.0` : la valeur juste pour l'export, où un
    /// « pixel » de Plotters est une unité SVG et rien d'autre.
    fn default() -> Self {
        Self::new(MEASUREMENT_PIXELS_PER_POINT)
    }
}

impl std::fmt::Debug for TextMeasurer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextMeasurer")
            .field("pixels_per_point", &self.pixels_per_point)
            .finish_non_exhaustive()
    }
}

impl TextSource for TextMeasurer {
    fn measure(&self, text: &str, font_id: FontId) -> Vec2 {
        self.layout_at(
            MEASUREMENT_PIXELS_PER_POINT,
            text,
            font_id,
            Color32::PLACEHOLDER,
        )
        .rect
        .size()
    }

    fn layout(&self, text: &str, font_id: FontId, color: Color32) -> Arc<Galley> {
        self.layout_at(self.pixels_per_point, text, font_id, color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le fait qui justifie [`MEASUREMENT_PIXELS_PER_POINT`], verrouillé
    /// **dans le sens de l'affirmation** : oui, la hauteur d'un galley
    /// dépend bien de l'échelle d'affichage. Si epaint corrigeait ce
    /// comportement un jour, ce test tomberait et la constante pourrait
    /// disparaître — c'est exactement ce qu'on veut savoir.
    #[test]
    fn la_hauteur_dun_galley_depend_bien_du_pixels_per_point() {
        let source = TextMeasurer::new(1.0);
        let font = FontId::new(font_size(12.0), FontFamily::Proportional);
        let a = source
            .layout_at(1.0, "0,5", font.clone(), Color32::BLACK)
            .rect
            .height();
        let b = source
            .layout_at(1.25, "0,5", font, Color32::BLACK)
            .rect
            .height();
        assert_ne!(
            a, b,
            "epaint ne cale plus la hauteur de ligne sur la grille de pixels \
             physiques : MEASUREMENT_PIXELS_PER_POINT n'a plus de raison d'être"
        );
    }
}
