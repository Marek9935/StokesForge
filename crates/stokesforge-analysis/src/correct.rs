//! Correction par une colonne de référence d'une autre feuille (#186) : la
//! référence (réponse spectrale d'un détecteur, blanc) est ramenée sur la grille
//! X des données par interpolation linéaire, puis combinée.
//!
//! Hors du domaine de la référence, la cellule corrigée est **vide** (`NaN`) et
//! comptée (M11 A) : on ne prolonge pas une mesure qu'on n'a pas.
//!
//! Une cellule vide ou non finie **au milieu** de la référence (X connu, Y non
//! fini) est enjambée par l'interpolation entre ses voisines, et les points de
//! données qui tombent dans ce trou sont comptés (`bridged`, C3 #313). Une ligne
//! de référence dont X manque n'a pas de position : pas un trou.

use crate::error::AnalysisError;
use crate::series::{interpolate, Series};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrectOp {
    Divide,
    Multiply,
    Subtract,
}

/// Résultat de la correction, point par point de la série d'entrée.
#[derive(Debug, Clone, PartialEq)]
pub struct Corrected {
    pub values: Vec<f64>,
    /// La référence ramenée sur les X des données (`NaN` hors domaine).
    pub reference_on_grid: Vec<f64>,
    /// Points hors du domaine de la référence.
    pub out_of_reference: usize,
    /// Points de données interpolés à travers un trou de la référence (C3).
    pub bridged: usize,
    /// Divisions par une référence exactement nulle, rendues `NaN` (C5). Un
    /// débordement à diviseur non nul n'est pas compté.
    pub division_by_zero: usize,
}

/// Corrige `data` par `reference`. Une division par zéro donne `NaN` et est
/// comptée ; le domaine l'est aussi. `holes` : les X des lignes de la référence
/// dont Y n'est pas fini (X fini), pour compter les points enjambés.
pub fn correct(
    data: &Series,
    reference: &Series,
    holes: &[f64],
    op: CorrectOp,
) -> Result<Corrected, AnalysisError> {
    if reference.is_empty() {
        return Err(AnalysisError::EmptyReference);
    }
    let r = reference.sorted();
    let mut values = Vec::with_capacity(data.len());
    let mut on_grid = Vec::with_capacity(data.len());
    let mut out = 0;
    let mut bridged = 0;
    let mut by_zero = 0;
    for (&x, &y) in data.x.iter().zip(&data.y) {
        match interpolate(&r.x, &r.y, x) {
            Some(c) => {
                if bridges_hole(&r.x, holes, x) {
                    bridged += 1;
                }
                if op == CorrectOp::Divide && c == 0.0 && y.is_finite() {
                    by_zero += 1;
                }
                let v = match op {
                    CorrectOp::Divide => y / c,
                    CorrectOp::Multiply => y * c,
                    CorrectOp::Subtract => y - c,
                };
                values.push(if v.is_finite() { v } else { f64::NAN });
                on_grid.push(c);
            }
            None => {
                out += 1;
                values.push(f64::NAN);
                on_grid.push(f64::NAN);
            }
        }
    }
    Ok(Corrected {
        values,
        reference_on_grid: on_grid,
        out_of_reference: out,
        bridged,
        division_by_zero: by_zero,
    })
}

/// `x` tombe-t-il entre deux points gardés de la référence qui encadrent un
/// trou ? Un `x` exactement sur un point gardé est exact, pas enjambé ; hors du
/// domaine, rien à enjamber.
fn bridges_hole(kept: &[f64], holes: &[f64], x: f64) -> bool {
    if holes.is_empty() {
        return false;
    }
    let hi = kept.partition_point(|&v| v < x);
    if hi == 0 || hi >= kept.len() || kept[hi] == x {
        return false;
    }
    let (lo, hi) = (kept[hi - 1], kept[hi]);
    holes.iter().any(|&h| h > lo && h < hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn division_par_une_reference_sur_une_autre_grille() {
        let data = Series::new(vec![1.0, 2.0, 3.0, 4.0], vec![10.0, 20.0, 30.0, 40.0]);
        // Référence linéaire 1 + 0.5·(x−1) échantillonnée aux extrémités seulement.
        let reference = Series::new(vec![1.0, 5.0], vec![1.0, 3.0]);
        let c = correct(&data, &reference, &[], CorrectOp::Divide).unwrap();
        assert_eq!(c.out_of_reference, 0);
        let want = [10.0 / 1.0, 20.0 / 1.5, 30.0 / 2.0, 40.0 / 2.5];
        for (a, b) in c.values.iter().zip(want) {
            assert!((a - b).abs() < 1e-12, "{a} vs {b}");
        }
    }

    #[test]
    fn hors_du_domaine_la_cellule_est_vide_et_comptee() {
        let data = Series::new(vec![0.0, 1.0, 2.0, 9.0], vec![1.0; 4]);
        let reference = Series::new(vec![1.0, 2.0], vec![2.0, 2.0]);
        let c = correct(&data, &reference, &[], CorrectOp::Multiply).unwrap();
        assert_eq!(c.out_of_reference, 2);
        assert!(c.values[0].is_nan() && c.values[3].is_nan());
        assert_eq!(c.values[1], 2.0);
        assert!(c.reference_on_grid[0].is_nan());
    }

    #[test]
    fn soustraction_et_reference_desordonnee() {
        let data = Series::new(vec![1.0, 2.0], vec![5.0, 7.0]);
        let reference = Series::new(vec![2.0, 1.0], vec![3.0, 1.0]);
        let c = correct(&data, &reference, &[], CorrectOp::Subtract).unwrap();
        assert_eq!(c.values, vec![4.0, 4.0]);
    }

    #[test]
    fn division_par_zero_donne_une_cellule_vide() {
        let data = Series::new(vec![1.0, 2.0], vec![5.0, 7.0]);
        let reference = Series::new(vec![1.0, 2.0], vec![0.0, 1.0]);
        let c = correct(&data, &reference, &[], CorrectOp::Divide).unwrap();
        assert!(c.values[0].is_nan());
        assert_eq!(c.values[1], 7.0);
        assert_eq!(c.out_of_reference, 0);
    }

    #[test]
    fn un_trou_de_la_reference_est_enjambe_et_compte() {
        // Référence x=[1,2,3,4], y=[1,NaN,3,4] : le trou est en x = 2.
        let reference = Series::new(vec![1.0, 3.0, 4.0], vec![1.0, 3.0, 4.0]);
        let data = Series::new(vec![1.5, 2.0, 2.5, 3.5], vec![1.0; 4]);
        let c = correct(&data, &reference, &[2.0], CorrectOp::Multiply).unwrap();
        assert_eq!(c.bridged, 3);
        assert_eq!(c.values, vec![1.5, 2.0, 2.5, 3.5]);
        // Exactement sur un point gardé : pas enjambé.
        let on_kept = Series::new(vec![1.0, 3.0, 4.0], vec![1.0; 3]);
        let c = correct(&on_kept, &reference, &[2.0], CorrectOp::Multiply).unwrap();
        assert_eq!(c.bridged, 0);
    }

    #[test]
    fn un_trou_en_fin_de_reference_n_enjambe_rien() {
        // y = [1, 2, 3, NaN] : le trou (x = 4) est hors du domaine gardé.
        let reference = Series::new(vec![1.0, 2.0, 3.0], vec![1.0, 2.0, 3.0]);
        let data = Series::new(vec![1.5, 2.5, 3.5], vec![1.0; 3]);
        let c = correct(&data, &reference, &[4.0], CorrectOp::Multiply).unwrap();
        assert_eq!((c.bridged, c.out_of_reference), (0, 1));
    }

    #[test]
    fn la_division_par_une_reference_nulle_est_comptee_pas_le_debordement() {
        let data = Series::new(vec![1.0, 2.0, 3.0], vec![5.0, 1e300, 1.0]);
        let reference = Series::new(vec![1.0, 2.0, 3.0], vec![0.0, 1e-300, 1e-300]);
        let c = correct(&data, &reference, &[], CorrectOp::Divide).unwrap();
        assert_eq!(c.division_by_zero, 1);
        assert!(c.values[0].is_nan() && c.values[1].is_nan());
        assert!(c.values[2].is_finite() && c.values[2] > 1e299);
    }

    #[test]
    fn reference_vide() {
        let data = Series::new(vec![1.0], vec![1.0]);
        assert_eq!(
            correct(&data, &Series::default(), &[], CorrectOp::Divide).unwrap_err(),
            AnalysisError::EmptyReference
        );
    }
}
