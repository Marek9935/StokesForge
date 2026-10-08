//! Un panneau : une zone de tracé, ses quatre axes, sa légende, ses séries,
//! ses textes libres.

use stokesforge_id::NodeId;

use crate::annotation::AnnotationSpec;
use crate::axis::{AxisSide, AxisSpec};
use crate::legend::LegendSpec;
use crate::series::{SeriesAxis, SeriesSpec};
use crate::text_anchor::TextAnchor;

/// Calque superposé à un autre (#167), à la manière du « New Layer (Axes) »
/// d'Origin : il partage la case de la grille **et la zone de tracé** du calque
/// qu'il recouvre (dimension liée), sur un fond transparent.
///
/// Son axe `bottom` est son axe X, son axe `left` son axe Y. `x_top` le place
/// en haut (échelle **propre** au calque) ; sinon l'axe n'est pas affiché et
/// l'échelle X est celle du calque recouvert (échelle liée). Même règle pour
/// `y_right` et l'axe Y, à droite. Un calque sans axe affiché (`false`,
/// `false`) suit donc entièrement le calque recouvert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OverlaySpec {
    /// L'axe X du calque est affiché en haut, avec son échelle propre.
    pub x_top: bool,
    /// L'axe Y du calque est affiché à droite, avec son échelle propre.
    pub y_right: bool,
}

/// Un panneau et ses quatre axes.
///
/// `row`/`column` placent le panneau dans la grille du [`crate::PlotSpec`] —
/// c'est la description du multi-panneaux, sans rien emprunter au système de
/// disposition d'un moteur donné. Ce sont des `usize` : une position négative
/// n'est pas ramenée à zéro, elle ne s'écrit pas.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelSpec {
    /// Titre du panneau.
    pub title: String,
    /// Visibilité du titre, indépendante de son contenu (#85).
    pub title_visible: bool,
    /// Position du titre du panneau s'il a été déplacé (issue #53).
    pub title_anchor: Option<TextAnchor>,
    /// Police et corps du titre du panneau (issue #78).
    pub title_font: crate::TextFont,
    /// Couleur du titre du panneau (issue #90) ; vide = automatique, comme
    /// [`crate::PlotSpec::title_color`].
    pub title_color: String,
    /// Séries tracées, dans l'ordre de superposition.
    pub series: Vec<SeriesSpec>,
    /// Axe du bas.
    pub bottom: AxisSpec,
    /// Axe de gauche.
    pub left: AxisSpec,
    /// Axe de droite (secondaire).
    pub right: AxisSpec,
    /// Axe du haut, masqué par défaut.
    pub top: AxisSpec,
    /// Légende.
    pub legend: LegendSpec,
    /// Calque superposé à un autre de la même case (#167) ; `None` pour un
    /// calque ordinaire. Un calque superposé recouvre le dernier calque
    /// ordinaire qui le précède dans la même case.
    pub overlay: Option<OverlaySpec>,
    /// Ligne du panneau dans la grille.
    pub row: usize,
    /// Colonne du panneau dans la grille.
    pub column: usize,
    /// Textes libres posés sur le panneau, ancrés aux données (issue #35),
    /// dans l'ordre de superposition.
    pub annotations: Vec<AnnotationSpec>,
    /// Objets graphiques (rectangle, ellipse, ligne, flèche — issue #76),
    /// ancrés aux données, du dessous au dessus. Tracés sous les textes
    /// libres.
    pub shapes: Vec<crate::ShapeSpec>,
}

impl PanelSpec {
    /// Panneau par défaut placé en `(row, column)`.
    pub fn at(row: usize, column: usize) -> Self {
        Self {
            row,
            column,
            ..Self::default()
        }
    }

    /// Un calque superposé (#167) au calque `base` : même case, sans titre ni
    /// légende, seuls les axes que `overlay` affiche sont visibles, sans
    /// quadrillage.
    pub fn overlay_on(base: &PanelSpec, overlay: OverlaySpec) -> Self {
        let axis = |visible| AxisSpec {
            visible,
            grid: false,
            ..AxisSpec::default()
        };
        let legend = LegendSpec {
            visible: false,
            ..LegendSpec::default()
        };
        Self {
            title_visible: false,
            bottom: axis(overlay.x_top),
            left: axis(overlay.y_right),
            right: AxisSpec::hidden(),
            legend,
            overlay: Some(overlay),
            ..Self::at(base.row, base.column)
        }
    }

    /// L'axe d'un côté donné.
    pub fn axis(&self, side: AxisSide) -> &AxisSpec {
        match side {
            AxisSide::Bottom => &self.bottom,
            AxisSide::Left => &self.left,
            AxisSide::Right => &self.right,
            AxisSide::Top => &self.top,
        }
    }

    /// L'axe d'un côté donné, modifiable.
    pub fn axis_mut(&mut self, side: AxisSide) -> &mut AxisSpec {
        match side {
            AxisSide::Bottom => &mut self.bottom,
            AxisSide::Left => &mut self.left,
            AxisSide::Right => &mut self.right,
            AxisSide::Top => &mut self.top,
        }
    }

    /// Vrai si au moins une série est rattachée à l'axe de droite — c'est ce
    /// qui décide de l'afficher.
    pub fn uses_right_axis(&self) -> bool {
        self.series.iter().any(|s| s.axis == SeriesAxis::Right)
    }

    /// L'axe du haut est un miroir de celui du bas (#176).
    pub fn mirrors_top(&self) -> bool {
        self.overlay.is_none() && self.top.mirror
    }

    /// L'axe de droite est un miroir de celui de gauche (#176) : seulement
    /// quand aucune série n'y est rattachée, sinon c'est un vrai axe.
    pub fn mirrors_right(&self) -> bool {
        self.overlay.is_none() && self.right.mirror && !self.uses_right_axis()
    }

    /// Colonnes référencées par les séries du panneau, sans doublon, dans
    /// l'ordre de première apparition.
    pub fn referenced_columns(&self) -> Vec<NodeId> {
        let mut seen: Vec<NodeId> = Vec::new();
        for series in &self.series {
            for column in series.referenced_columns() {
                if !seen.contains(&column) {
                    seen.push(column);
                }
            }
        }
        seen
    }

    /// Le premier numéro de groupe (#177) qu'aucune série du panneau ne porte.
    pub fn next_group_id(&self) -> u32 {
        self.series
            .iter()
            .filter_map(|s| s.group)
            .max()
            .map_or(1, |max| max.saturating_add(1))
    }

    /// Ajoute des séries **ensemble** (#177) : à deux ou plus, elles forment un
    /// nouveau groupe ; une série seule n'en a pas. Les numéros de groupe que
    /// les séries portaient déjà sont écartés.
    pub fn push_series_batch(&mut self, series: Vec<SeriesSpec>) {
        let group = (series.len() > 1).then(|| self.next_group_id());
        self.series.extend(series.into_iter().map(|mut s| {
            s.group = group;
            s
        }));
    }

    /// Rangs, dans [`PanelSpec::series`], des séries du groupe `group`.
    pub fn group_members(&self, group: u32) -> Vec<usize> {
        self.series
            .iter()
            .enumerate()
            .filter_map(|(index, s)| (s.group == Some(group)).then_some(index))
            .collect()
    }

    /// Les numéros de groupe présents, dans l'ordre de première apparition.
    pub fn group_ids(&self) -> Vec<u32> {
        let mut ids = Vec::new();
        for group in self.series.iter().filter_map(|s| s.group) {
            if !ids.contains(&group) {
                ids.push(group);
            }
        }
        ids
    }

    /// Vrai si une série du panneau référence cette colonne.
    pub fn references(&self, column: NodeId) -> bool {
        self.series.iter().any(|s| s.references(column))
    }
}

impl Default for PanelSpec {
    fn default() -> Self {
        Self {
            title: String::new(),
            title_visible: true,
            title_anchor: None,
            title_font: crate::TextFont::default(),
            title_color: String::new(),
            series: Vec::new(),
            bottom: AxisSpec::default(),
            left: AxisSpec::default(),
            right: AxisSpec::default(),
            top: AxisSpec::hidden(),
            legend: LegendSpec::default(),
            overlay: None,
            row: 0,
            column: 0,
            annotations: Vec::new(),
            shapes: Vec::new(),
        }
    }
}

#[cfg(test)]
mod group_tests {
    use super::*;

    fn serie(n: u64) -> SeriesSpec {
        SeriesSpec::new(NodeId::from_raw(n))
    }

    #[test]
    fn des_series_ajoutees_ensemble_forment_un_groupe_une_seule_non() {
        let mut panel = PanelSpec::default();
        panel.push_series_batch(vec![serie(1)]);
        assert_eq!(panel.series[0].group, None);
        panel.push_series_batch(vec![serie(2), serie(3)]);
        assert_eq!(panel.group_members(1), [1, 2]);
        panel.push_series_batch(vec![serie(4), serie(5)]);
        assert_eq!(panel.group_ids(), [1, 2]);
        assert_eq!(panel.next_group_id(), 3);
    }
}
