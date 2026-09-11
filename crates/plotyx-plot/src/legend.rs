//! Légende d'un panneau.

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegendSpec {
    /// Légende affichée.
    pub visible: bool,
    /// Coin d'ancrage.
    pub position: LegendPosition,
}

impl Default for LegendSpec {
    fn default() -> Self {
        Self {
            visible: true,
            position: LegendPosition::TopRight,
        }
    }
}
