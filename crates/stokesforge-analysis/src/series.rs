//! Une série (X, Y) nettoyée, et les opérations de base communes aux calculs.
//!
//! Le modèle de données dit « vide », « texte » ou `NaN` par un `NaN` (le même
//! vocabulaire que `stokesforge-import`) : un calcul ignore ces lignes et les
//! **compte**, le compte figure dans le résultat (#181, §2).

/// Une série de points `(x, y)` finis, dans l'ordre donné.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Series {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
}

/// Une série et le nombre de lignes écartées pour la fabriquer.
#[derive(Debug, Clone, PartialEq)]
pub struct Cleaned {
    pub series: Series,
    /// Lignes dont X ou Y n'était pas un nombre fini.
    pub skipped: usize,
}

impl Series {
    pub fn new(x: Vec<f64>, y: Vec<f64>) -> Self {
        debug_assert_eq!(x.len(), y.len());
        Self { x, y }
    }

    pub fn len(&self) -> usize {
        self.x.len()
    }

    pub fn is_empty(&self) -> bool {
        self.x.is_empty()
    }

    /// Les points dont X est dans `[lo, hi]` (bornes incluses, dans n'importe
    /// quel ordre).
    pub fn restricted(&self, lo: f64, hi: f64) -> Series {
        let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        let mut out = Series::default();
        for (&x, &y) in self.x.iter().zip(&self.y) {
            if x >= lo && x <= hi {
                out.x.push(x);
                out.y.push(y);
            }
        }
        out
    }

    /// Copie triée par X croissant (tri stable).
    pub fn sorted(&self) -> Series {
        let mut idx: Vec<usize> = (0..self.len()).collect();
        idx.sort_by(|&a, &b| self.x[a].total_cmp(&self.x[b]));
        Series {
            x: idx.iter().map(|&i| self.x[i]).collect(),
            y: idx.iter().map(|&i| self.y[i]).collect(),
        }
    }

    /// `true` si X est croissant au sens large.
    pub fn is_ascending(&self) -> bool {
        self.x.windows(2).all(|w| w[0] <= w[1])
    }
}

/// Apparie `x` et `y` ligne à ligne et écarte les lignes où l'un des deux
/// n'est pas fini. Seule la longueur commune est lue : une colonne plus longue
/// que l'autre ne fabrique pas de lignes écartées.
pub fn clean(x: &[f64], y: &[f64]) -> Cleaned {
    let n = x.len().min(y.len());
    let mut series = Series::default();
    let mut skipped = 0;
    for i in 0..n {
        if x[i].is_finite() && y[i].is_finite() {
            series.x.push(x[i]);
            series.y.push(y[i]);
        } else {
            skipped += 1;
        }
    }
    Cleaned { series, skipped }
}

/// Interpolation linéaire de `(xs, ys)` (X croissants) en `at`. `None` hors du
/// domaine `[xs.first, xs.last]`, ou si la série est vide.
pub fn interpolate(xs: &[f64], ys: &[f64], at: f64) -> Option<f64> {
    let n = xs.len().min(ys.len());
    if n == 0 || !at.is_finite() {
        return None;
    }
    if n == 1 {
        return (at == xs[0]).then_some(ys[0]);
    }
    if at < xs[0] || at > xs[n - 1] {
        return None;
    }
    // Premier indice dont X >= at.
    let hi = xs[..n].partition_point(|&v| v < at);
    if hi == 0 {
        return Some(ys[0]);
    }
    let (x0, x1) = (xs[hi - 1], xs[hi]);
    let (y0, y1) = (ys[hi - 1], ys[hi]);
    if x1 == x0 {
        return Some(y1);
    }
    Some(y0 + (y1 - y0) * (at - x0) / (x1 - x0))
}

/// Moyenne des `count` valeurs de Y les plus proches (en X) de `x0` : l'ancrage
/// « moyenne locale » des points posés à la souris. `count` vaut au moins 1.
pub fn local_mean(series: &Series, x0: f64, count: usize) -> Option<f64> {
    if series.is_empty() {
        return None;
    }
    let count = count.clamp(1, series.len());
    let mut idx: Vec<usize> = (0..series.len()).collect();
    idx.sort_by(|&a, &b| {
        (series.x[a] - x0)
            .abs()
            .total_cmp(&(series.x[b] - x0).abs())
    });
    let sum: f64 = idx[..count].iter().map(|&i| series.y[i]).sum();
    Some(sum / count as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_ecarte_et_compte_les_lignes_non_finies() {
        let x = [1.0, f64::NAN, 3.0, 4.0, 5.0];
        let y = [10.0, 20.0, f64::INFINITY, 40.0, 50.0];
        let c = clean(&x, &y);
        assert_eq!(c.series.x, vec![1.0, 4.0, 5.0]);
        assert_eq!(c.series.y, vec![10.0, 40.0, 50.0]);
        assert_eq!(c.skipped, 2);
    }

    #[test]
    fn clean_ne_compte_pas_la_queue_d_une_colonne_plus_longue() {
        let c = clean(&[1.0, 2.0, 3.0], &[1.0, 2.0]);
        assert_eq!(c.series.len(), 2);
        assert_eq!(c.skipped, 0);
    }

    #[test]
    fn restricted_accepte_des_bornes_inversees() {
        let s = Series::new(vec![1.0, 2.0, 3.0, 4.0], vec![0.0; 4]);
        assert_eq!(s.restricted(3.0, 2.0).x, vec![2.0, 3.0]);
    }

    #[test]
    fn sorted_range_par_x_croissant() {
        let s = Series::new(vec![3.0, 1.0, 2.0], vec![30.0, 10.0, 20.0]).sorted();
        assert_eq!(s.x, vec![1.0, 2.0, 3.0]);
        assert_eq!(s.y, vec![10.0, 20.0, 30.0]);
        assert!(s.is_ascending());
    }

    #[test]
    fn interpolate_est_lineaire_et_refuse_l_extrapolation() {
        let xs = [0.0, 1.0, 3.0];
        let ys = [0.0, 10.0, 30.0];
        assert_eq!(interpolate(&xs, &ys, 0.5), Some(5.0));
        assert_eq!(interpolate(&xs, &ys, 2.0), Some(20.0));
        assert_eq!(interpolate(&xs, &ys, 3.0), Some(30.0));
        assert_eq!(interpolate(&xs, &ys, 3.1), None);
        assert_eq!(interpolate(&xs, &ys, -0.1), None);
        assert_eq!(interpolate(&[], &[], 0.0), None);
    }

    #[test]
    fn local_mean_moyenne_les_voisins_en_x() {
        let s = Series::new(
            vec![0.0, 1.0, 2.0, 3.0, 10.0],
            vec![1.0, 2.0, 3.0, 4.0, 100.0],
        );
        assert_eq!(local_mean(&s, 1.0, 1), Some(2.0));
        assert_eq!(local_mean(&s, 1.0, 3), Some(2.0));
        // Plus de voisins que de points : tous.
        assert_eq!(local_mean(&s, 1.0, 99), Some(22.0));
    }
}
