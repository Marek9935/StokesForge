//! Les textes déplaçables d'un graphe — titre, titres de panneau, titres
//! d'axe, légende (issue #53).
//!
//! Un titre sans [`TextAnchor`] est **là où le rendu le met** : Plotters
//! réserve sa place (bande du titre, aire de graduation, coin de la zone de
//! tracé pour la légende). Avec une ancre, il est **ancré aux données** comme
//! un texte libre ([`crate::AnnotationSpec`]) : son centre suit le zoom et le
//! déplacement de la vue, et la position vaut à l'export (le rendu, donc les
//! deux backends, la lit). Réinitialiser, c'est retirer l'ancre.
//!
//! L'ancre est exprimée dans le repère de données du **panneau** du texte —
//! le premier panneau pour le titre du graphe, qui n'appartient à aucun
//! panneau. Un titre vit en général **hors** de la zone de tracé : l'ancre
//! n'est donc pas bornée au cadrage visible (contrairement à un texte libre,
//! qui n'est pas tracé hors cadre).

use crate::axis::AxisSide;
use crate::series::SeriesAxis;
use crate::spec::PlotSpec;

/// Où un texte déplacé est posé : son **centre**, en données.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextAnchor {
    /// Abscisse du centre, en données.
    pub x: f64,
    /// Ordonnée du centre, en données de l'axe [`Self::axis`].
    pub y: f64,
    /// Ordonnée de rattachement : gauche ou droite, comme un texte libre.
    pub axis: SeriesAxis,
}

impl TextAnchor {
    /// Une ancre au point `(x, y)` de l'ordonnée de gauche.
    pub const fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            axis: SeriesAxis::Left,
        }
    }

    /// Vrai si l'ancre est exploitable : coordonnées finies. Une ancre venue
    /// d'un fichier abîmé est ignorée au rendu (le texte retrouve sa place
    /// par défaut) plutôt que de projeter un `NaN`.
    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// Un texte déplaçable, désigné dans un [`PlotSpec`]. Les panneaux sont
/// adressés par leur position dans la grille, comme [`crate::AnnotationRef`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextTarget {
    /// Le titre du graphe.
    Title,
    /// Le titre d'un panneau.
    PanelTitle {
        /// Ligne du panneau.
        row: usize,
        /// Colonne du panneau.
        column: usize,
    },
    /// Le titre d'un axe d'un panneau.
    AxisTitle {
        /// Ligne du panneau.
        row: usize,
        /// Colonne du panneau.
        column: usize,
        /// Le côté de l'axe.
        side: AxisSide,
    },
    /// La légende d'un panneau (le cadre entier).
    Legend {
        /// Ligne du panneau.
        row: usize,
        /// Colonne du panneau.
        column: usize,
    },
}

impl TextTarget {
    /// La position `(row, column)` du panneau du texte ; `None` pour le titre
    /// du graphe.
    pub fn panel_position(&self) -> Option<(usize, usize)> {
        match *self {
            Self::Title => None,
            Self::PanelTitle { row, column }
            | Self::AxisTitle { row, column, .. }
            | Self::Legend { row, column } => Some((row, column)),
        }
    }

    fn panel_index(&self, spec: &PlotSpec) -> Option<usize> {
        let (row, column) = self.panel_position()?;
        spec.panels
            .iter()
            .position(|panel| panel.row == row && panel.column == column)
    }

    /// L'ancre du texte, s'il a été déplacé.
    pub fn anchor(&self, spec: &PlotSpec) -> Option<TextAnchor> {
        match *self {
            Self::Title => spec.title_anchor,
            Self::PanelTitle { .. } => spec.panels.get(self.panel_index(spec)?)?.title_anchor,
            Self::AxisTitle { side, .. } => {
                spec.panels
                    .get(self.panel_index(spec)?)?
                    .axis(side)
                    .title_anchor
            }
            Self::Legend { .. } => spec.panels.get(self.panel_index(spec)?)?.legend.anchor,
        }
    }

    /// Pose (ou, avec `None`, retire) l'ancre du texte. Rend `false` sans
    /// rien changer si le panneau n'existe pas.
    pub fn set_anchor(&self, spec: &mut PlotSpec, anchor: Option<TextAnchor>) -> bool {
        match *self {
            Self::Title => {
                spec.title_anchor = anchor;
                true
            }
            Self::PanelTitle { .. } => {
                let Some(panel) = self
                    .panel_index(spec)
                    .and_then(|index| spec.panels.get_mut(index))
                else {
                    return false;
                };
                panel.title_anchor = anchor;
                true
            }
            Self::AxisTitle { side, .. } => {
                let Some(panel) = self
                    .panel_index(spec)
                    .and_then(|index| spec.panels.get_mut(index))
                else {
                    return false;
                };
                panel.axis_mut(side).title_anchor = anchor;
                true
            }
            Self::Legend { .. } => {
                let Some(panel) = self
                    .panel_index(spec)
                    .and_then(|index| spec.panels.get_mut(index))
                else {
                    return false;
                };
                panel.legend.anchor = anchor;
                true
            }
        }
    }

    /// Le texte du titre tel qu'il s'affiche — `None` pour la légende (ses
    /// textes sont les noms des séries) ou si le panneau n'existe pas. Pour un
    /// titre d'axe, l'unité est incluse (`Signal (u.a.)`,
    /// [`crate::AxisSpec::display_label`]).
    pub fn text(&self, spec: &PlotSpec) -> Option<String> {
        match *self {
            Self::Title => Some(spec.title.clone()),
            Self::PanelTitle { .. } => {
                Some(spec.panels.get(self.panel_index(spec)?)?.title.clone())
            }
            Self::AxisTitle { side, .. } => Some(
                spec.panels
                    .get(self.panel_index(spec)?)?
                    .axis(side)
                    .display_label(),
            ),
            Self::Legend { .. } => None,
        }
    }

    /// Remplace le texte du titre. Rend `false` sans rien changer pour la
    /// légende ou un panneau absent. Pour un titre d'axe, `text` est le texte
    /// **affiché** ([`Self::text`]) : s'il se termine encore par l'unité entre
    /// parenthèses, elle reste l'unité de l'axe et le reste devient le titre ;
    /// sinon l'unité est vidée (le texte saisi est tout ce qui s'affiche).
    pub fn set_text(&self, spec: &mut PlotSpec, text: String) -> bool {
        match *self {
            Self::Title => {
                spec.title = text;
                true
            }
            Self::PanelTitle { .. } => {
                let Some(panel) = self
                    .panel_index(spec)
                    .and_then(|index| spec.panels.get_mut(index))
                else {
                    return false;
                };
                panel.title = text;
                true
            }
            Self::AxisTitle { side, .. } => {
                let Some(panel) = self
                    .panel_index(spec)
                    .and_then(|index| spec.panels.get_mut(index))
                else {
                    return false;
                };
                let axis = panel.axis_mut(side);
                let suffix = format!(" ({})", axis.unit);
                if axis.unit.is_empty() {
                    axis.label = text;
                } else if let Some(label) = text.strip_suffix(suffix.as_str()) {
                    axis.label = label.to_owned();
                } else if text == format!("({})", axis.unit) {
                    axis.label.clear();
                } else {
                    axis.label = text;
                    axis.unit.clear();
                }
                true
            }
            Self::Legend { .. } => false,
        }
    }
}

impl PlotSpec {
    /// Retire toutes les ancres : chaque titre et chaque légende retrouve sa
    /// place par défaut. Rend le nombre d'ancres retirées.
    pub fn reset_text_anchors(&mut self) -> usize {
        let mut removed = usize::from(self.title_anchor.take().is_some());
        for panel in self.panels.iter_mut() {
            removed += usize::from(panel.title_anchor.take().is_some());
            removed += usize::from(panel.legend.anchor.take().is_some());
            for side in [
                AxisSide::Bottom,
                AxisSide::Left,
                AxisSide::Right,
                AxisSide::Top,
            ] {
                removed += usize::from(panel.axis_mut(side).title_anchor.take().is_some());
            }
        }
        removed
    }

    /// Vrai si au moins un texte a été déplacé.
    pub fn has_text_anchors(&self) -> bool {
        self.title_anchor.is_some()
            || self.panels.iter().any(|panel| {
                panel.title_anchor.is_some()
                    || panel.legend.anchor.is_some()
                    || [
                        AxisSide::Bottom,
                        AxisSide::Left,
                        AxisSide::Right,
                        AxisSide::Top,
                    ]
                    .into_iter()
                    .any(|side| panel.axis(side).title_anchor.is_some())
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_ancre_se_pose_se_lit_et_se_retire_pour_chaque_texte() {
        let mut spec = PlotSpec::empty("Titre");
        let cibles = [
            TextTarget::Title,
            TextTarget::PanelTitle { row: 0, column: 0 },
            TextTarget::AxisTitle {
                row: 0,
                column: 0,
                side: AxisSide::Left,
            },
            TextTarget::Legend { row: 0, column: 0 },
        ];
        for cible in cibles {
            assert_eq!(cible.anchor(&spec), None);
            assert!(cible.set_anchor(&mut spec, Some(TextAnchor::new(1.0, 2.0))));
            assert_eq!(cible.anchor(&spec), Some(TextAnchor::new(1.0, 2.0)));
        }
        assert!(spec.has_text_anchors());
        assert_eq!(spec.reset_text_anchors(), 4);
        assert!(!spec.has_text_anchors());
        assert_eq!(spec.reset_text_anchors(), 0);
    }

    #[test]
    fn un_panneau_absent_ne_change_rien() {
        let mut spec = PlotSpec::empty("T");
        let loin = TextTarget::Legend { row: 3, column: 3 };
        assert!(!loin.set_anchor(&mut spec, Some(TextAnchor::new(0.0, 0.0))));
        assert_eq!(loin.anchor(&spec), None);
        assert!(!spec.has_text_anchors());
    }

    #[test]
    fn le_texte_d_un_titre_se_lit_et_se_remplace() {
        let mut spec = PlotSpec::empty("A");
        let axe = TextTarget::AxisTitle {
            row: 0,
            column: 0,
            side: AxisSide::Bottom,
        };
        assert_eq!(TextTarget::Title.text(&spec).as_deref(), Some("A"));
        assert!(TextTarget::Title.set_text(&mut spec, "B".into()));
        assert!(axe.set_text(&mut spec, "t".into()));
        assert_eq!(spec.title, "B");
        assert_eq!(spec.panels[0].bottom.label, "t");
        // L'unité s'affiche avec le titre : la garder ou l'effacer à l'édition.
        spec.panels.get_mut(0).expect("panneau").bottom.unit = "s".into();
        assert_eq!(axe.text(&spec).as_deref(), Some("t (s)"));
        assert!(axe.set_text(&mut spec, "temps (s)".into()));
        assert_eq!(spec.panels[0].bottom.label, "temps");
        assert_eq!(spec.panels[0].bottom.unit, "s");
        assert!(axe.set_text(&mut spec, "durée".into()));
        assert_eq!(spec.panels[0].bottom.label, "durée");
        assert_eq!(spec.panels[0].bottom.unit, "");
        let legende = TextTarget::Legend { row: 0, column: 0 };
        assert_eq!(legende.text(&spec), None);
        assert!(!legende.set_text(&mut spec, "x".into()));
    }

    #[test]
    fn une_ancre_non_finie_n_est_pas_exploitable() {
        assert!(TextAnchor::new(1.0, 2.0).is_finite());
        assert!(!TextAnchor::new(f64::NAN, 2.0).is_finite());
    }
}
