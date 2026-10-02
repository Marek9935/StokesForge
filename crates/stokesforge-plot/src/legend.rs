//! Légende d'un panneau.

use crate::text_anchor::TextAnchor;

/// Coin du panneau où s'ancre la légende.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LegendPosition {
    /// En haut à gauche.
    TopLeft,
    /// En haut à droite (défaut).
    #[default]
    TopRight,
    /// En bas à gauche.
    BottomLeft,
    /// En bas à droite.
    BottomRight,
}

/// Légende d'un panneau : visible ou non, et où.
#[derive(Debug, Clone, PartialEq)]
pub struct LegendSpec {
    /// Légende affichée.
    pub visible: bool,
    /// Coin d'ancrage.
    pub position: LegendPosition,
    /// Position du cadre (son centre) s'il a été déplacé (issue #53) ;
    /// prime sur [`Self::position`]. `None` = le coin d'ancrage.
    pub anchor: Option<TextAnchor>,
    /// Police et corps des noms de série de la légende (issue #77).
    pub font: crate::TextFont,
    /// Couleur des noms de série de la légende (issue #90) ; vide =
    /// automatique, comme [`crate::PlotSpec::title_color`].
    pub color: String,
}

impl Default for LegendSpec {
    fn default() -> Self {
        Self {
            visible: true,
            position: LegendPosition::TopRight,
            anchor: None,
            font: crate::TextFont::default(),
            color: String::new(),
        }
    }
}
