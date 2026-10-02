//! Quel composant d'un graphe est sous un pixel — titre, zone de tracé, axe
//! ou titre d'axe.
//!
//! Sert au clic droit de l'interface (« propriétés de ce que je vise »). Vit
//! ici plutôt que dans `stokesforge-render-egui` pour la même raison que
//! [`crate::interact`] : c'est de la géométrie pure sur un [`DrawnPlot`],
//! indépendante du backend.
//!
//! # D'où vient la géométrie
//!
//! Rien n'est reconstruit à côté du tracé : la zone de tracé
//! ([`DrawnPanel::pixel_origin`]/[`DrawnPanel::pixel_size`]), l'épaisseur des
//! aires de graduation ([`DrawnPanel::axis_areas`]) et la bande du titre
//! ([`DrawnPlot::title_height`]) sont celles que Plotters a réellement
//! allouées, recopiées par [`crate::pipeline`].
//!
//! Seule approximation : la séparation, dans une aire de graduation, entre
//! les libellés de graduation et le titre de l'axe. Plotters pose ce titre
//! contre le bord **extérieur** de l'aire (`draw_axis_and_labels`) ; on lui
//! attribue une bande de [`AXIS_TITLE_BAND`] pixels de ce côté, au plus la
//! moitié de l'aire. Un clic sur la frontière exacte peut donc tomber sur
//! l'un ou l'autre — sans conséquence, les deux ouvrent les réglages du même
//! axe.

use crate::pipeline::{AxisArea, DrawnPanel, DrawnPlot, AXIS_DESC_SIZE};
use stokesforge_plot::TextTarget;

/// Épaisseur attribuée au titre d'un axe dans son aire de graduation : la
/// hauteur d'une ligne de [`AXIS_DESC_SIZE`], interligne compris.
pub const AXIS_TITLE_BAND: f64 = AXIS_DESC_SIZE * 1.5;

/// Un axe d'un panneau.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AxisSide {
    /// Abscisses, sous la zone de tracé.
    Bottom,
    /// Ordonnée principale, à gauche.
    Left,
    /// Ordonnée secondaire, à droite.
    Right,
}

/// Un composant d'un graphe, désigné par un pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlotComponent {
    /// Le titre du graphe, en haut de la figure.
    Title,
    /// La zone de tracé du panneau d'indice donné (dans
    /// [`DrawnPlot::panels`]).
    PlotArea { panel: usize },
    /// Un axe : sa ligne et ses libellés de graduation.
    Axis { panel: usize, side: AxisSide },
    /// Le titre d'un axe.
    AxisTitle { panel: usize, side: AxisSide },
    /// Un texte libre (issue #35) : `index` dans `PanelSpec::annotations`.
    /// Prioritaire sur tout le reste — il est tracé par-dessus.
    Annotation { panel: usize, index: usize },
    /// Un objet graphique (issue #76) : `index` dans `PanelSpec::shapes`.
    /// Désigné sur son trait, ou dedans s'il est rempli ; prioritaire sur la
    /// zone de tracé, sous les textes libres.
    Shape { panel: usize, index: usize },
    /// Un titre ou une légende (issue #53) : double-clic pour l'éditer,
    /// glisser pour le déplacer. Tracé par-dessus la zone de tracé comme les
    /// textes libres, donc prioritaire sur elle et sur les axes.
    Text(TextTarget),
    /// Une entrée de la légende : la série du panneau `panel` (rang dans
    /// `PanelSpec::series`) qu'elle montre. Prioritaire sur [`Self::Text`]
    /// de la même légende.
    LegendEntry { panel: usize, series: usize },
}

/// Le composant sous le pixel `(x, y)` (repère du backend, celui de
/// [`DrawnPanel::pixel_origin`]), ou `None` s'il ne vise que le fond (marges,
/// espace entre panneaux).
pub fn component_at(plot: &DrawnPlot, x: f64, y: f64) -> Option<PlotComponent> {
    // Le dernier tracé est au-dessus : on cherche du dernier au premier.
    for (panel, drawn) in plot.panels.iter().enumerate().rev() {
        if let Some(annotation) = drawn.annotations.iter().rev().find(|a| a.contains(x, y)) {
            return Some(PlotComponent::Annotation {
                panel,
                index: annotation.index,
            });
        }
    }
    // Les objets graphiques (issue #76) : le plus haut d'abord, et seulement
    // dans la zone de tracé où ils sont rognés.
    for (panel, drawn) in plot.panels.iter().enumerate().rev() {
        let (left, top) = (drawn.pixel_origin.0 as f64, drawn.pixel_origin.1 as f64);
        let inside = x >= left
            && x <= left + drawn.pixel_size.0 as f64
            && y >= top
            && y <= top + drawn.pixel_size.1 as f64;
        if !inside {
            continue;
        }
        if let Some(shape) = drawn.shapes.iter().rev().find(|s| s.contains(x, y)) {
            return Some(PlotComponent::Shape {
                panel,
                index: shape.index,
            });
        }
    }
    // Les titres et légendes (issue #53), du dernier panneau au premier ; la
    // légende d'abord par ses lignes, puis par son cadre.
    for (panel, drawn) in plot.panels.iter().enumerate().rev() {
        let on_legend = drawn
            .texts
            .iter()
            .any(|text| matches!(text.target, TextTarget::Legend { .. }) && text.contains(x, y));
        if on_legend {
            let row = drawn
                .legend_rows
                .iter()
                .find(|row| (y - row.center_y).abs() <= row.height / 2.0);
            return Some(match row {
                Some(row) => PlotComponent::LegendEntry {
                    panel,
                    series: row.series,
                },
                None => PlotComponent::Text(TextTarget::Legend {
                    row: drawn.row,
                    column: drawn.column,
                }),
            });
        }
        if let Some(text) = drawn.texts.iter().rev().find(|text| text.contains(x, y)) {
            return Some(PlotComponent::Text(text.target));
        }
    }
    if let Some(title) = plot.title.filter(|title| title.contains(x, y)) {
        return Some(PlotComponent::Text(title.target));
    }
    if y >= 0.0 && y < plot.title_height as f64 {
        return Some(PlotComponent::Title);
    }
    plot.panels
        .iter()
        .enumerate()
        .find_map(|(index, panel)| panel_component_at(index, panel, x, y))
}

fn panel_component_at(index: usize, panel: &DrawnPanel, x: f64, y: f64) -> Option<PlotComponent> {
    let left = panel.pixel_origin.0 as f64;
    let top = panel.pixel_origin.1 as f64;
    let right = left + panel.pixel_size.0 as f64;
    let bottom = top + panel.pixel_size.1 as f64;
    let within_x = (left..=right).contains(&x);
    let within_y = (top..=bottom).contains(&y);
    if within_x && within_y {
        return Some(PlotComponent::PlotArea { panel: index });
    }
    let areas = &panel.axis_areas;
    // Distance au bord de la zone de tracé, vers l'extérieur, dans chaque
    // aire candidate.
    let candidates = [
        (AxisSide::Bottom, within_x, y - bottom, areas.bottom),
        (AxisSide::Left, within_y, left - x, areas.left),
        (AxisSide::Right, within_y, x - right, areas.right),
    ];
    candidates
        .into_iter()
        .find_map(|(side, aligned, depth, area)| {
            (aligned && depth > 0.0 && depth <= area.size as f64)
                .then(|| axis_part(index, side, depth, area))
        })
}

/// Axe ou titre d'axe, selon la profondeur du clic dans l'aire.
fn axis_part(panel: usize, side: AxisSide, depth: f64, area: AxisArea) -> PlotComponent {
    let size = area.size as f64;
    let band = AXIS_TITLE_BAND.min(size / 2.0);
    if area.titled && depth > size - band {
        PlotComponent::AxisTitle { panel, side }
    } else {
        PlotComponent::Axis { panel, side }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::AxisAreas;
    use stokesforge_plot::AxisScale;

    /// Figure de 480×360 : titre de 40 px, zone de tracé de (80, 60) à
    /// (440, 280), aire du bas de 52 px, de gauche de 64 px, pas d'axe
    /// secondaire.
    fn figure() -> DrawnPlot {
        let area = |size| AxisArea { size, titled: true };
        DrawnPlot {
            panels: vec![DrawnPanel {
                row: 0,
                column: 0,
                x_range: (0.0, 1.0),
                y_range: (0.0, 1.0),
                y2_range: None,
                pixel_origin: (80, 60),
                pixel_size: (360, 220),
                x_scale: AxisScale::Linear,
                y_scale: AxisScale::Linear,
                y2_scale: AxisScale::Linear,
                drawn_series: 0,
                broken_series: 0,
                unknown_series: 0,
                legend_entries: 0,
                decimated_points: 0,
                axis_areas: AxisAreas {
                    bottom: area(52),
                    left: area(64),
                    right: AxisArea::default(),
                },
                annotations: Vec::new(),
                shapes: Vec::new(),
                texts: Vec::new(),
                legend_rows: Vec::new(),
            }],
            omitted_panels: 0,
            drawn_series: 0,
            broken_series: 0,
            unknown_series: 0,
            decimated_points: 0,
            title_height: 40,
            title: None,
        }
    }

    #[test]
    fn chaque_zone_designe_son_composant() {
        let plot = figure();
        let panel = 0;
        assert_eq!(component_at(&plot, 240.0, 20.0), Some(PlotComponent::Title));
        assert_eq!(
            component_at(&plot, 240.0, 150.0),
            Some(PlotComponent::PlotArea { panel })
        );
        // Juste sous la zone de tracé : les graduations de l'axe du bas.
        assert_eq!(
            component_at(&plot, 240.0, 290.0),
            Some(PlotComponent::Axis {
                panel,
                side: AxisSide::Bottom
            })
        );
        // Contre le bord extérieur de l'aire du bas : son titre.
        assert_eq!(
            component_at(&plot, 240.0, 328.0),
            Some(PlotComponent::AxisTitle {
                panel,
                side: AxisSide::Bottom
            })
        );
        assert_eq!(
            component_at(&plot, 70.0, 150.0),
            Some(PlotComponent::Axis {
                panel,
                side: AxisSide::Left
            })
        );
        assert_eq!(
            component_at(&plot, 20.0, 150.0),
            Some(PlotComponent::AxisTitle {
                panel,
                side: AxisSide::Left
            })
        );
        // Marge à droite, sans axe secondaire : rien.
        assert_eq!(component_at(&plot, 460.0, 150.0), None);
        // Coin bas-gauche, hors des deux aires : rien.
        assert_eq!(component_at(&plot, 40.0, 320.0), None);
    }

    #[test]
    fn un_axe_sans_titre_n_a_pas_de_bande_de_titre() {
        let mut plot = figure();
        plot.panels[0].axis_areas.bottom.titled = false;
        assert_eq!(
            component_at(&plot, 240.0, 328.0),
            Some(PlotComponent::Axis {
                panel: 0,
                side: AxisSide::Bottom
            })
        );
    }

    /// Issue #35 : un texte tourné se désigne sur son rectangle **tourné**,
    /// pas sur sa boîte englobante, et passe devant la zone de tracé et le
    /// titre d'axe qu'il recouvre.
    #[test]
    fn un_texte_tourne_se_designe_sur_son_rectangle_tourne() {
        use crate::pipeline::DrawnAnnotation;
        let mut plot = figure();
        // 100 × 20 px, tourné d'un quart de tour : vertical, centré en
        // (240, 150).
        plot.panels[0].annotations.push(DrawnAnnotation {
            index: 3,
            center: (240.0, 150.0),
            size: (100, 20),
            rotation: 90.0,
        });
        let annotation = Some(PlotComponent::Annotation { panel: 0, index: 3 });
        assert_eq!(
            component_at(&plot, 240.0, 105.0),
            annotation,
            "haut du texte"
        );
        assert_eq!(component_at(&plot, 245.0, 190.0), annotation);
        // Dans la boîte du texte **droit**, mais hors du texte tourné.
        assert_eq!(
            component_at(&plot, 280.0, 150.0),
            Some(PlotComponent::PlotArea { panel: 0 })
        );
        // À 45°, un coin de la boîte englobante n'est pas le texte.
        plot.panels[0].annotations[0].rotation = 45.0;
        assert_eq!(component_at(&plot, 240.0, 150.0), annotation, "centre");
        assert_ne!(component_at(&plot, 280.0, 185.0), annotation, "coin vide");
        assert_eq!(
            component_at(&plot, 270.0, 120.0),
            annotation,
            "le long du texte"
        );
    }

    /// Issue #53 : un titre, un titre d'axe tourné ou une légende se désignent
    /// sur leur rectangle (tourné), avant la zone de tracé qu'ils recouvrent,
    /// et une ligne de légende avant le cadre entier.
    #[test]
    fn un_titre_ou_une_legende_se_designent_avant_la_zone_de_trace() {
        use crate::pipeline::{DrawnLegendRow, DrawnText};
        use stokesforge_plot::AxisSide as Side;
        let mut plot = figure();
        let left = TextTarget::AxisTitle {
            row: 0,
            column: 0,
            side: Side::Left,
        };
        plot.panels[0].texts = vec![
            // Titre d'axe vertical, déplacé en plein dans la zone de tracé.
            DrawnText {
                target: left,
                center: (200.0, 150.0),
                size: (120, 20),
                rotation: 90.0,
                anchored: true,
            },
            DrawnText {
                target: TextTarget::Legend { row: 0, column: 0 },
                center: (380.0, 100.0),
                size: (100, 60),
                rotation: 0.0,
                anchored: false,
            },
        ];
        plot.panels[0].legend_rows = vec![
            DrawnLegendRow {
                series: 0,
                center_y: 85.0,
                height: 20.0,
            },
            DrawnLegendRow {
                series: 3,
                center_y: 110.0,
                height: 20.0,
            },
        ];
        assert_eq!(
            component_at(&plot, 200.0, 100.0),
            Some(PlotComponent::Text(left)),
            "haut du titre vertical"
        );
        assert_eq!(
            component_at(&plot, 230.0, 150.0),
            Some(PlotComponent::PlotArea { panel: 0 }),
            "à côté du titre tourné : la zone de tracé"
        );
        assert_eq!(
            component_at(&plot, 380.0, 112.0),
            Some(PlotComponent::LegendEntry {
                panel: 0,
                series: 3
            })
        );
        assert_eq!(
            component_at(&plot, 380.0, 72.0),
            Some(PlotComponent::Text(TextTarget::Legend {
                row: 0,
                column: 0
            })),
            "le cadre, hors des lignes"
        );
        plot.title = Some(DrawnText {
            target: TextTarget::Title,
            center: (240.0, 20.0),
            size: (100, 24),
            rotation: 0.0,
            anchored: false,
        });
        assert_eq!(
            component_at(&plot, 240.0, 20.0),
            Some(PlotComponent::Text(TextTarget::Title))
        );
        assert_eq!(
            component_at(&plot, 40.0, 20.0),
            Some(PlotComponent::Title),
            "la bande du titre, hors du texte, garde ses propriétés"
        );
    }

    #[test]
    fn sans_titre_le_haut_de_la_figure_n_est_pas_le_titre() {
        let mut plot = figure();
        plot.title_height = 0;
        assert_eq!(component_at(&plot, 240.0, 20.0), None);
    }
}
