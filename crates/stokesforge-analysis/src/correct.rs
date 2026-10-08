//! Correction par une colonne de référence d'une autre feuille (#186) : la
//! référence (réponse spectrale d'un détecteur, blanc) est ramenée sur la grille
//! X des données par interpolation linéaire, puis combinée.
//!
//! Hors du domaine de la référence, la cellule corrigée est **vide** (`NaN`) et
//! comptée (M11 A) : on ne prolonge pas une mesure qu'on n'a pas.

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
}

/// Corrige `data` par `reference`. Une division par zéro donne `NaN`, sans être
/// comptée : seul le domaine l'est.
pub fn correct(
    data: &Series,
    reference: &Series,
    op: CorrectOp,
) -> Result<Corrected, AnalysisError> {
    if reference.is_empty() {
        return Err(AnalysisError::EmptyReference);
    }
    let r = reference.sorted();
    let mut values = Vec::with_capacity(data.len());
    let mut on_grid = Vec::with_capacity(data.len());
    let mut out = 0;
    for (&x, &y) in data.x.iter().zip(&data.y) {
        match interpolate(&r.x, &r.y, x) {
            Some(c) => {
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn division_par_une_reference_sur_une_autre_grille() {
        let data = Series::new(vec![1.0, 2.0, 3.0, 4.0], vec![10.0, 20.0, 30.0, 40.0]);
        // Référence linéaire 1 + 0.5·(x−1) échantillonnée aux extrémités seulement.
        let reference = Series::new(vec![1.0, 5.0], vec![1.0, 3.0]);
        let c = correct(&data, &reference, CorrectOp::Divide).unwrap();
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
        let c = correct(&data, &reference, CorrectOp::Multiply).unwrap();
        assert_eq!(c.out_of_reference, 2);
        assert!(c.values[0].is_nan() && c.values[3].is_nan());
        assert_eq!(c.values[1], 2.0);
        assert!(c.reference_on_grid[0].is_nan());
    }

    #[test]
    fn soustraction_et_reference_desordonnee() {
        let data = Series::new(vec![1.0, 2.0], vec![5.0, 7.0]);
        let reference = Series::new(vec![2.0, 1.0], vec![3.0, 1.0]);
        let c = correct(&data, &reference, CorrectOp::Subtract).unwrap();
        assert_eq!(c.values, vec![4.0, 4.0]);
    }

    #[test]
    fn division_par_zero_donne_une_cellule_vide() {
        let data = Series::new(vec![1.0, 2.0], vec![5.0, 7.0]);
        let reference = Series::new(vec![1.0, 2.0], vec![0.0, 1.0]);
        let c = correct(&data, &reference, CorrectOp::Divide).unwrap();
        assert!(c.values[0].is_nan());
        assert_eq!(c.values[1], 7.0);
        assert_eq!(c.out_of_reference, 0);
    }

    #[test]
    fn reference_vide() {
        let data = Series::new(vec![1.0], vec![1.0]);
        assert_eq!(
            correct(&data, &Series::default(), CorrectOp::Divide).unwrap_err(),
            AnalysisError::EmptyReference
        );
    }
}
