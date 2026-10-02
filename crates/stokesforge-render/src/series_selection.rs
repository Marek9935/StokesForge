//! Désignation des séries par les pixels réellement tracés (#83).
//!
//! Le pipeline fournit ces chemins après décimation et rognage, avec le rang
//! d'origine. Aucun second calcul des axes, aucune résolution au mouvement de
//! souris : axe droit, histogramme et courbe suivent le même chemin que le rendu.

use crate::{DrawnPlot, PanelTransform};
type Pixel = (i32, i32);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DrawnSeries {
    pub index: usize,
    pub paths: Vec<Vec<Pixel>>,
    pub points: Vec<Pixel>,
    pub bars: Vec<(Pixel, Pixel)>,
}

fn segment_distance(p: (f64, f64), a: Pixel, b: Pixel) -> f64 {
    let (dx, dy) = (b.0 as f64 - a.0 as f64, b.1 as f64 - a.1 as f64);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 {
        0.0
    } else {
        ((p.0 - a.0 as f64) * dx + (p.1 - a.1 as f64) * dy) / length
    }
    .clamp(0.0, 1.0);
    (p.0 - a.0 as f64 - t * dx).hypot(p.1 - a.1 as f64 - t * dy)
}

impl DrawnSeries {
    pub fn distance(&self, pixel: (f64, f64)) -> f64 {
        let paths = self
            .paths
            .iter()
            .flat_map(|path| path.windows(2))
            .map(|s| segment_distance(pixel, s[0], s[1]));
        let points = self.points.iter().map(|&p| segment_distance(pixel, p, p));
        let bars = self.bars.iter().map(|&(a, b)| {
            let x = pixel.0.clamp(a.0 as f64, b.0 as f64);
            let y = pixel.1.clamp(a.1 as f64, b.1 as f64);
            (pixel.0 - x).hypot(pixel.1 - y)
        });
        paths
            .chain(points)
            .chain(bars)
            .fold(f64::INFINITY, f64::min)
    }
}

/// Rang d'origine dans le panneau déclaré. À égalité, le dernier tracé gagne.
pub fn series_at(plot: &DrawnPlot, pixel: (f64, f64)) -> Option<(usize, usize, usize)> {
    for panel in plot.panels.iter().rev() {
        if !PanelTransform::of(panel)?.contains(pixel.0, pixel.1) {
            continue;
        }
        let mut best = None;
        let mut distance = 5.0;
        for series in panel.series_geometry.iter().rev() {
            let candidate = series.distance(pixel);
            if candidate < distance {
                distance = candidate;
                best = Some((panel.row, panel.column, series.index));
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn les_segments_et_les_barres_se_designent_entre_leurs_points() {
        let line = DrawnSeries {
            paths: vec![vec![(0, 0), (100, 100)]],
            ..Default::default()
        };
        assert!(line.distance((50.0, 50.0)) < 0.1);
        assert!(line.distance((20.0, 80.0)) > 30.0);
        let bars = DrawnSeries {
            bars: vec![((10, 20), (30, 70))],
            ..Default::default()
        };
        assert_eq!(bars.distance((20.0, 40.0)), 0.0);
        assert_eq!(bars.distance((40.0, 40.0)), 10.0);
        assert!(DrawnSeries::default().distance((0.0, 0.0)).is_infinite());
    }
}
