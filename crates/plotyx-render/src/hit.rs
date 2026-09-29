//! Quel composant d'un graphe est sous un pixel — titre, zone de tracé, axe
//! ou titre d'axe.
//!
//! Sert au clic droit de l'interface (« propriétés de ce que je vise »). Vit
//! ici plutôt que dans `plotyx-render-egui` pour la même raison que
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
}

/// Le composant sous le pixel `(x, y)` (repère du backend, celui de
/// [`DrawnPanel::pixel_origin`]), ou `None` s'il ne vise que le fond (marges,
/// espace entre panneaux).
pub fn component_at(plot: &DrawnPlot, x: f64, y: f64) -> Option<PlotComponent> {
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
    use plotyx_plot::AxisScale;

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
            }],
            omitted_panels: 0,
            drawn_series: 0,
            broken_series: 0,
            unknown_series: 0,
            decimated_points: 0,
            title_height: 40,
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

    #[test]
    fn sans_titre_le_haut_de_la_figure_n_est_pas_le_titre() {
        let mut plot = figure();
        plot.title_height = 0;
        assert_eq!(component_at(&plot, 240.0, 20.0), None);
    }
}
