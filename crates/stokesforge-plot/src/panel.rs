//! Un panneau : une zone de tracé, ses quatre axes, sa légende, ses séries,
//! ses textes libres.

use stokesforge_id::NodeId;

use crate::annotation::AnnotationSpec;
use crate::axis::{AxisSide, AxisSpec};
use crate::legend::LegendSpec;
use crate::series::{SeriesAxis, SeriesSpec};
use crate::text_anchor::TextAnchor;

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
            series: Vec::new(),
            bottom: AxisSpec::default(),
            left: AxisSpec::default(),
            right: AxisSpec::default(),
            top: AxisSpec::hidden(),
            legend: LegendSpec::default(),
            row: 0,
            column: 0,
            annotations: Vec::new(),
            shapes: Vec::new(),
        }
    }
}
