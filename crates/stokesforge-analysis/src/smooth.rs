//! Lissage d'une courbe (#324) : moyenne adjacente, Savitzky-Golay, médiane
//! glissante.
//!
//! Le lissage lit les voisins **par rang** après tri par X, pas par distance :
//! une fenêtre de `n` points est `n` lignes consécutives de la courbe triée. La
//! fenêtre se donne en points (`Points`) ou en unités de X (`XUnits`) ; en unités
//! de X, elle est convertie en points par le pas **médian** de X (les pas nuls,
//! X répétés, ne comptent pas), de sorte que les deux écritures n'ont qu'un
//! calcul. Dans les deux cas elle est **impaire** : un lissage centré a autant de
//! voisins de chaque côté, donc une fenêtre paire est refusée (`Points`) ou
//! relevée au nombre impair suivant (`XUnits`, issu d'un arrondi).
//!
//! Aux extrémités, la courbe est prolongée par **réflexion** (le voisin au-delà du
//! premier point est le deuxième, etc.) : la sortie a autant de points que
//! l'entrée, sans biais d'une fenêtre qui rétrécit. Une fenêtre plus large que la
//! courbe est ramenée au plus grand impair qui tient (au moins 1) : lisser une
//! courbe courte donne un résultat, pas une erreur.
//!
//! Savitzky-Golay suppose un pas **uniforme** (les coefficients sont ceux d'un
//! ajustement polynomial sur des abscisses équidistantes) : un pas qui varie de
//! plus de 1 % est signalé (`step_uniform` faux), le calcul est fait quand même.

use crate::error::AnalysisError;
use crate::linalg::{solve, Matrix};
use crate::series::Series;

/// La méthode de lissage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmoothMethod {
    /// Moyenne des points de la fenêtre.
    AdjacentAverage,
    /// Polynôme d'ordre `order` ajusté sur la fenêtre, évalué en son centre.
    SavitzkyGolay { order: usize },
    /// Médiane des points de la fenêtre (robuste aux pics isolés).
    Median,
}

/// La largeur de la fenêtre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SmoothWindow {
    /// Nombre de points, impair.
    Points(usize),
    /// Largeur en unités de X, convertie en points par le pas médian.
    XUnits(f64),
}

/// Les réglages d'un lissage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothSpec {
    pub method: SmoothMethod,
    pub window: SmoothWindow,
}

/// Plafond de la fenêtre, en points : une fenêtre lue d'un fichier ne doit rien
/// allouer d'absurde (`validate`, comme `MAX_DEGREE`).
pub const MAX_WINDOW: usize = 1001;
/// Ordre minimal et maximal d'un Savitzky-Golay.
pub const MIN_ORDER: usize = 2;
pub const MAX_ORDER: usize = 5;
/// Fenêtre minimale d'un lissage, en points.
pub const MIN_WINDOW: usize = 3;

impl SmoothSpec {
    /// Refuse les réglages hors plafond avant tout calcul.
    pub fn validate(&self) -> Result<(), AnalysisError> {
        let bad = |name: &str| {
            Err(AnalysisError::InvalidParameter {
                name: name.to_string(),
            })
        };
        match self.window {
            SmoothWindow::Points(n) if !(MIN_WINDOW..=MAX_WINDOW).contains(&n) || n % 2 == 0 => {
                return bad("window");
            }
            SmoothWindow::XUnits(w) if !(w.is_finite() && w > 0.0) => return bad("window"),
            _ => {}
        }
        if let SmoothMethod::SavitzkyGolay { order } = self.method {
            if !(MIN_ORDER..=MAX_ORDER).contains(&order) {
                return bad("order");
            }
            if let SmoothWindow::Points(n) = self.window {
                if order >= n {
                    return bad("order");
                }
            }
        }
        Ok(())
    }
}

/// Résultat d'un lissage, ligne à ligne de la série d'entrée.
#[derive(Debug, Clone, PartialEq)]
pub struct Smoothed {
    pub values: Vec<f64>,
    /// La fenêtre réellement employée, en points (impaire).
    pub window: usize,
    /// Les pas de X sont égaux à 1 % près ; sinon Savitzky-Golay n'est
    /// qu'approché (l'appelant le signale pour cette méthode seule).
    pub step_uniform: bool,
}

/// Lisse `data` (points finis, ordre quelconque).
pub fn smooth(data: &Series, spec: &SmoothSpec) -> Result<Smoothed, AnalysisError> {
    spec.validate()?;
    let n = data.len();
    if n == 0 {
        return Err(AnalysisError::NotEnoughPoints {
            found: 0,
            needed: 1,
        });
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| data.x[a].total_cmp(&data.x[b]));
    let x: Vec<f64> = order.iter().map(|&i| data.x[i]).collect();
    let y: Vec<f64> = order.iter().map(|&i| data.y[i]).collect();

    let requested = match spec.window {
        SmoothWindow::Points(p) => p,
        SmoothWindow::XUnits(w) => {
            let step = median_step(&x).ok_or(AnalysisError::InvalidParameter {
                name: "window".to_string(),
            })?;
            let points = (w / step).round().clamp(1.0, MAX_WINDOW as f64) as usize;
            (points | 1).max(MIN_WINDOW)
        }
    };
    // Une fenêtre plus large que la courbe : le plus grand impair qui tient.
    let window = requested.min(if n % 2 == 1 { n } else { n - 1 }).max(1);
    let half = window / 2;

    let smoothed_sorted = match spec.method {
        SmoothMethod::AdjacentAverage => convolve(&y, half, &vec![1.0 / window as f64; window]),
        SmoothMethod::SavitzkyGolay { order } => {
            // Un polynôme d'ordre ≥ fenêtre passe par tous les points : la
            // fenêtre est relevée à `order + 1` (impair) quand la courbe le permet.
            let kernel = savitzky_golay_kernel(half, order.min(window - 1))?;
            convolve(&y, half, &kernel)
        }
        SmoothMethod::Median => (0..n).map(|i| window_median(&y, i, half)).collect(),
    };
    let mut values = vec![f64::NAN; n];
    for (rank, &row) in order.iter().enumerate() {
        values[row] = smoothed_sorted[rank];
    }
    Ok(Smoothed {
        values,
        window,
        step_uniform: uniform_step(&x),
    })
}

/// Le pas médian de X trié, sans les pas nuls ; `None` sans aucun pas positif.
fn median_step(x: &[f64]) -> Option<f64> {
    let mut steps: Vec<f64> = x
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|d| *d > 0.0 && d.is_finite())
        .collect();
    if steps.is_empty() {
        return None;
    }
    steps.sort_by(f64::total_cmp);
    Some(steps[steps.len() / 2])
}

/// Les pas de X trié sont-ils égaux à 1 % près du pas médian ?
fn uniform_step(x: &[f64]) -> bool {
    match median_step(x) {
        None => true,
        Some(step) => x
            .windows(2)
            .all(|w| ((w[1] - w[0]) - step).abs() <= 0.01 * step),
    }
}

/// L'indice `i` ramené dans `0..n` par réflexion sans répétition du bord.
fn reflect(i: isize, n: usize) -> usize {
    if n == 1 {
        return 0;
    }
    let period = 2 * (n as isize - 1);
    let m = i.rem_euclid(period);
    if m < n as isize {
        m as usize
    } else {
        (period - m) as usize
    }
}

/// Convolution par `kernel` (`2·half + 1` coefficients, du plus ancien au plus
/// récent), bords réfléchis.
fn convolve(y: &[f64], half: usize, kernel: &[f64]) -> Vec<f64> {
    let n = y.len();
    (0..n)
        .map(|i| {
            kernel
                .iter()
                .enumerate()
                .map(|(k, c)| c * y[reflect(i as isize + k as isize - half as isize, n)])
                .sum()
        })
        .collect()
}

fn window_median(y: &[f64], i: usize, half: usize) -> f64 {
    let n = y.len();
    let mut w: Vec<f64> = (0..=2 * half)
        .map(|k| y[reflect(i as isize + k as isize - half as isize, n)])
        .collect();
    w.sort_by(f64::total_cmp);
    w[half]
}

/// Les coefficients de Savitzky-Golay (lissage, dérivée 0) d'une fenêtre de
/// `2·half + 1` points et d'un polynôme d'ordre `order` : la première ligne de
/// `(AᵀA)⁻¹Aᵀ` avec `A[j][k] = j^k`, j de `-half` à `half`. Les abscisses sont
/// réduites par `half` pour garder le système bien conditionné.
fn savitzky_golay_kernel(half: usize, order: usize) -> Result<Vec<f64>, AnalysisError> {
    let size = 2 * half + 1;
    if half == 0 {
        return Ok(vec![1.0]);
    }
    let cols = order + 1;
    let mut a = Matrix::zeros(size, cols);
    for r in 0..size {
        let t = (r as f64 - half as f64) / half as f64;
        let mut p = 1.0;
        for c in 0..cols {
            a.set(r, c, p);
            p *= t;
        }
    }
    // c = e₀ᵀ (AᵀA)⁻¹ Aᵀ : on résout (AᵀA) u = e₀, puis c = A u.
    let gram = a.gram();
    let mut e0 = vec![0.0; cols];
    e0[0] = 1.0;
    let u = solve(&gram, &e0).ok_or(AnalysisError::SingularSystem)?;
    Ok(a.mul_vec(&u))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(n: usize) -> Series {
        let x: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let y = x.iter().map(|x| 2.0 * x + 1.0).collect();
        Series::new(x, y)
    }

    fn spec(method: SmoothMethod, points: usize) -> SmoothSpec {
        SmoothSpec {
            method,
            window: SmoothWindow::Points(points),
        }
    }

    #[test]
    fn la_reflexion_ne_repete_pas_le_bord() {
        let indices: Vec<usize> = (-3..8).map(|i| reflect(i, 5)).collect();
        assert_eq!(indices, [3, 2, 1, 0, 1, 2, 3, 4, 3, 2, 1]);
        assert_eq!(reflect(-4, 1), 0);
    }

    #[test]
    fn la_moyenne_adjacente_garde_une_droite_en_dedans_et_ajuste_les_bords() {
        let s = smooth(&line(9), &spec(SmoothMethod::AdjacentAverage, 3)).unwrap();
        // Au milieu, la moyenne de trois points alignés est le point central.
        assert!((s.values[4] - 9.0).abs() < 1e-12);
        // Au bord, la réflexion remplace le voisin manquant (point 1) : (3+1+3)/3.
        assert!((s.values[0] - 7.0 / 3.0).abs() < 1e-12, "{:?}", s.values);
        assert_eq!(s.window, 3);
    }

    /// Témoin : Savitzky-Golay d'ordre 2 rend **exactement** un polynôme de
    /// degré 2 (hors bords), là où la moyenne le déforme.
    #[test]
    fn savitzky_golay_conserve_une_parabole_que_la_moyenne_deforme() {
        let x: Vec<f64> = (0..21).map(f64::from).collect();
        let y: Vec<f64> = x.iter().map(|x| 0.5 * x * x - 3.0 * x + 2.0).collect();
        let data = Series::new(x, y.clone());
        let sg = smooth(&data, &spec(SmoothMethod::SavitzkyGolay { order: 2 }, 7)).unwrap();
        for i in 3..18 {
            assert!((sg.values[i] - y[i]).abs() < 1e-9, "i={i}");
        }
        let avg = smooth(&data, &spec(SmoothMethod::AdjacentAverage, 7)).unwrap();
        assert!((avg.values[10] - y[10]).abs() > 1.0, "la moyenne déforme");
    }

    #[test]
    fn la_mediane_ecarte_un_pic_isole() {
        let mut data = line(11);
        data.y[5] = 1000.0;
        let s = smooth(&data, &spec(SmoothMethod::Median, 3)).unwrap();
        assert!((s.values[5] - 13.0).abs() < 1e-12, "{}", s.values[5]);
    }

    #[test]
    fn la_sortie_suit_les_lignes_de_l_entree_meme_non_triee() {
        let data = Series::new(vec![3.0, 1.0, 2.0, 0.0], vec![30.0, 10.0, 20.0, 0.0]);
        let s = smooth(&data, &spec(SmoothMethod::Median, 3)).unwrap();
        // Triée : 0,10,20,30 -> médianes réfléchies 10,10,20,20 ; remises à leur ligne.
        assert_eq!(s.values, [20.0, 10.0, 20.0, 10.0]);
    }

    #[test]
    fn une_fenetre_en_unites_de_x_se_convertit_par_le_pas_median() {
        let data = line(30);
        let en_x = SmoothSpec {
            method: SmoothMethod::AdjacentAverage,
            window: SmoothWindow::XUnits(4.0),
        };
        // 4 unités à pas 1 : 4 points, relevés à 5 (impair).
        assert_eq!(smooth(&data, &en_x).unwrap().window, 5);
        let cinq = smooth(&data, &spec(SmoothMethod::AdjacentAverage, 5)).unwrap();
        assert_eq!(smooth(&data, &en_x).unwrap().values, cinq.values);
    }

    #[test]
    fn une_fenetre_trop_large_est_ramenee_a_la_courbe() {
        let s = smooth(&line(6), &spec(SmoothMethod::AdjacentAverage, 101)).unwrap();
        assert_eq!(s.window, 5);
        assert_eq!(s.values.len(), 6);
        let one = smooth(
            &Series::new(vec![0.0], vec![4.0]),
            &spec(SmoothMethod::Median, 5),
        )
        .unwrap();
        assert_eq!(one.values, [4.0]);
    }

    #[test]
    fn les_reglages_hors_plafond_sont_refuses() {
        for bad in [
            spec(SmoothMethod::AdjacentAverage, 4),
            spec(SmoothMethod::AdjacentAverage, 1),
            spec(SmoothMethod::AdjacentAverage, MAX_WINDOW + 2),
            spec(SmoothMethod::SavitzkyGolay { order: 6 }, 9),
            spec(SmoothMethod::SavitzkyGolay { order: 1 }, 9),
            spec(SmoothMethod::SavitzkyGolay { order: 5 }, 5),
            SmoothSpec {
                method: SmoothMethod::Median,
                window: SmoothWindow::XUnits(f64::NAN),
            },
            SmoothSpec {
                method: SmoothMethod::Median,
                window: SmoothWindow::XUnits(-1.0),
            },
        ] {
            assert!(
                matches!(bad.validate(), Err(AnalysisError::InvalidParameter { .. })),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn un_pas_non_uniforme_est_signale() {
        let data = Series::new(vec![0.0, 1.0, 2.0, 4.0, 8.0, 9.0, 10.0], vec![0.0; 7]);
        let s = smooth(&data, &spec(SmoothMethod::SavitzkyGolay { order: 2 }, 5)).unwrap();
        assert!(!s.step_uniform);
        assert!(
            smooth(&line(9), &spec(SmoothMethod::Median, 3))
                .unwrap()
                .step_uniform
        );
    }
}
