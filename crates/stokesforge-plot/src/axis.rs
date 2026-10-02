//! Les quatre axes d'un panneau : échelle, limites, grille.

use crate::text_anchor::TextAnchor;

/// Échelle d'un axe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AxisScale {
    /// Échelle linéaire (défaut).
    #[default]
    Linear,
    /// Échelle logarithmique.
    Log,
}

/// Côté du panneau où vit un axe. Les quatre existent ; une série, elle, ne
/// se rattache qu'à deux d'entre eux — voir [`crate::SeriesAxis`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AxisSide {
    /// Abscisse, en bas.
    Bottom,
    /// Ordonnée principale, à gauche.
    Left,
    /// Ordonnée secondaire, à droite.
    Right,
    /// Abscisse secondaire, en haut.
    Top,
}

/// Un axe : titre, unité, échelle, limites.
///
/// [`AxisSpec::auto_range`] prime sur `range_min`/`range_max` : les bornes
/// restent mémorisées quand on repasse en automatique, pour qu'un aller-retour
/// dans le panneau de propriétés ne les perde pas. C'est pour ça que les
/// bornes ne sont pas fondues dans un unique `Option<(f64, f64)>` — la
/// représentation serait plus serrée mais perdrait ce que l'utilisateur avait
/// saisi.
#[derive(Debug, Clone, PartialEq)]
pub struct AxisSpec {
    /// Titre de l'axe, sans l'unité.
    pub label: String,
    /// Visibilité du titre, indépendante de son contenu (#85).
    pub title_visible: bool,
    /// Unité affichée entre parenthèses après le titre.
    pub unit: String,
    /// Échelle.
    pub scale: AxisScale,
    /// Cadrage automatique sur les données.
    pub auto_range: bool,
    /// Borne basse mémorisée.
    pub range_min: Option<f64>,
    /// Borne haute mémorisée.
    pub range_max: Option<f64>,
    /// Grille sur cet axe.
    pub grid: bool,
    /// Axe affiché.
    pub visible: bool,
    /// Axe inversé (valeurs décroissantes).
    pub inverted: bool,
    /// Couleur du titre de l'axe (`#RRGGBB`). Vide = automatique, voir
    /// [`crate::PlotSpec::title_color`].
    pub title_color: String,
    /// Position du titre de l'axe s'il a été déplacé (issue #53).
    pub title_anchor: Option<TextAnchor>,
    /// Police et corps du titre de l'axe (issue #78).
    pub title_font: crate::TextFont,
    /// Couleur de l'échelle — ligne d'axe, graduations et leurs libellés, et
    /// la grille, atténuée (`#RRGGBB`). Vide = automatique.
    pub tick_color: String,
}

impl AxisSpec {
    /// Axe masqué — la valeur par défaut de l'axe du haut d'un panneau.
    pub fn hidden() -> Self {
        Self {
            visible: false,
            ..Self::default()
        }
    }

    /// Titre affiché : `label (unité)`, sans parenthèses vides.
    pub fn display_label(&self) -> String {
        match (self.label.is_empty(), self.unit.is_empty()) {
            (false, false) => format!("{} ({})", self.label, self.unit),
            (false, true) => self.label.clone(),
            (true, false) => format!("({})", self.unit),
            (true, true) => String::new(),
        }
    }

    /// Vrai quand le cadrage manuel est à la fois demandé et complet : les
    /// deux bornes sont là et l'automatique est coupé.
    pub fn has_explicit_range(&self) -> bool {
        !self.auto_range && self.range_min.is_some() && self.range_max.is_some()
    }
}

impl Default for AxisSpec {
    fn default() -> Self {
        Self {
            label: String::new(),
            unit: String::new(),
            scale: AxisScale::Linear,
            auto_range: true,
            range_min: None,
            range_max: None,
            grid: false,
            visible: true,
            inverted: false,
            title_color: String::new(),
            title_visible: true,
            title_anchor: None,
            title_font: crate::TextFont::default(),
            tick_color: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_only_flips_visibility() {
        let hidden = AxisSpec::hidden();
        assert!(!hidden.visible);
        assert_eq!(
            AxisSpec {
                visible: true,
                ..hidden
            },
            AxisSpec::default()
        );
    }
}
