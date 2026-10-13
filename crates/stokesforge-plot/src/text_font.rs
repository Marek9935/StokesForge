//! La police et le corps d'un texte du graphe (issues #77 et #78).
//!
//! Un choix de police ou de corps vaut pour **toute** la zone de texte : c'est
//! un attribut de la zone, pas une balise glissée dans son contenu. Le
//! contenu (markup Typst, [`crate::AnnotationSpec::text`]) ne porte plus de
//! `#text(font: …)` posé par la barre de format ; le rendu compose la police
//! et sa police Math associée au tracé (`stokesforge_render::formula`).

/// Police et corps d'un titre, d'un titre d'axe ou de la légende. Le défaut
/// ne change rien : la police du graphe et le corps du pipeline.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextFont {
    /// Nom de famille, embarquée ou du système. Vide = la police du graphe.
    pub family: String,
    /// Corps au sens de Plotters (16 = titre d'axe). `None` = le corps par
    /// défaut de ce texte.
    pub size: Option<f64>,
    /// Largeur de la boîte, en fraction de la largeur de la figure (#244) ;
    /// `None` = une ligne. Voir [`crate::AnnotationSpec::width`].
    pub width: Option<f64>,
    /// Alignement dans la boîte ; sans effet sans largeur.
    pub align: crate::TextAlign,
}

impl TextFont {
    /// Corps minimal et maximal accepté au rendu, comme
    /// [`crate::AnnotationSpec::SIZE_RANGE`].
    pub const SIZE_RANGE: (f64, f64) = (4.0, 200.0);

    /// Vrai si rien n'est réglé.
    pub fn is_default(&self) -> bool {
        self.family.trim().is_empty()
            && self.size.is_none()
            && self.width.is_none()
            && self.align == crate::TextAlign::Center
    }

    /// Le corps tracé : celui du réglage borné dans [`Self::SIZE_RANGE`],
    /// `default` s'il manque ou n'est pas fini.
    pub fn effective_size(&self, default: f64) -> f64 {
        match self.size {
            Some(size) if size.is_finite() => size.clamp(Self::SIZE_RANGE.0, Self::SIZE_RANGE.1),
            _ => default,
        }
    }

    /// La largeur tracée : [`Self::width`] bornée comme celle d'un texte libre
    /// ([`crate::AnnotationSpec::effective_width`]).
    pub fn effective_width(&self) -> Option<f64> {
        self.width
            .filter(|width| width.is_finite())
            .map(|width| width.clamp(crate::AnnotationSpec::MIN_WIDTH, 1.0))
    }

    /// La famille demandée, `None` si elle est vide.
    pub fn family(&self) -> Option<&str> {
        let family = self.family.trim();
        (!family.is_empty()).then_some(family)
    }
}
