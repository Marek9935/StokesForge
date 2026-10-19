//! Intégration par la méthode des trapèzes (#187), dans les **unités de X
//! telles qu'elles sont** : une courbe en longueur d'onde s'intègre en longueur
//! d'onde, une courbe en énergie en énergie. Aucune conversion (décision
//! 2026-10-08).
//!
//! L'aire est celle de la série **triée par X croissant** : elle est positive
//! pour un Y positif quel que soit le sens d'enregistrement du spectre.

use crate::error::AnalysisError;
use crate::series::{interpolate, Series};

/// Ce qu'on retranche sous la courbe d'une plage (M12 b).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Under {
    /// Aire jusqu'à Y = 0.
    Zero,
    /// Aire au-dessus de la droite qui joint les deux extrémités de la plage.
    Line,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IntegrateSpec {
    /// Plages `(de, à)`. Vide : la série entière.
    pub ranges: Vec<(f64, f64)>,
    pub under: Under,
    pub cumulative: bool,
    pub normalize: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RangeArea {
    /// Bornes effectives, rognées au domaine des données.
    pub from: f64,
    pub to: f64,
    pub area: f64,
    /// Points de la série dans la plage.
    pub points: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Integrated {
    pub areas: Vec<RangeArea>,
    pub total: f64,
    /// Intégrale cumulée depuis le plus petit X, un point par point de la série
    /// d'entrée (dans son ordre).
    pub cumulative: Option<Vec<f64>>,
    /// `y / total`, un point par point de la série d'entrée.
    pub normalized: Option<Vec<f64>>,
}

/// Aire par trapèzes de la série triée sur `[lo, hi]` (bornes interpolées).
fn trapezoid(sorted: &Series, lo: f64, hi: f64, under: Under) -> Option<(f64, usize)> {
    let (x0, x1) = (*sorted.x.first()?, *sorted.x.last()?);
    let (lo, hi) = (lo.max(x0), hi.min(x1));
    if !(hi > lo) {
        return None;
    }
    let ylo = interpolate(&sorted.x, &sorted.y, lo)?;
    let yhi = interpolate(&sorted.x, &sorted.y, hi)?;
    let inside: Vec<usize> = (0..sorted.len())
        .filter(|&i| sorted.x[i] > lo && sorted.x[i] < hi)
        .collect();
    let mut xs = vec![lo];
    let mut ys = vec![ylo];
    for &i in &inside {
        xs.push(sorted.x[i]);
        ys.push(sorted.y[i]);
    }
    xs.push(hi);
    ys.push(yhi);
    let mut area = 0.0;
    for k in 1..xs.len() {
        area += 0.5 * (ys[k] + ys[k - 1]) * (xs[k] - xs[k - 1]);
    }
    if under == Under::Line {
        area -= 0.5 * (ylo + yhi) * (hi - lo);
    }
    Some((area, inside.len() + 2))
}

pub fn integrate(series: &Series, spec: &IntegrateSpec) -> Result<Integrated, AnalysisError> {
    if series.len() < 2 {
        return Err(AnalysisError::NotEnoughPoints {
            found: series.len(),
            needed: 2,
        });
    }
    // Une borne non finie (plage pas encore posée, NaN) ferait `min/max`
    // renvoyer l'autre argument : (NaN, NaN) intégrerait toute la courbe (#276).
    if spec
        .ranges
        .iter()
        .any(|&(a, b)| !(a.is_finite() && b.is_finite()))
    {
        return Err(AnalysisError::InvalidParameter {
            name: "ranges".into(),
        });
    }
    let sorted = series.sorted();
    let (xmin, xmax) = (sorted.x[0], sorted.x[sorted.len() - 1]);
    let ranges: Vec<(f64, f64)> = if spec.ranges.is_empty() {
        vec![(xmin, xmax)]
    } else {
        spec.ranges
            .iter()
            .map(|&(a, b)| (a.min(b), a.max(b)))
            .collect()
    };
    let mut areas = Vec::new();
    for (lo, hi) in ranges {
        let (area, points) =
            trapezoid(&sorted, lo, hi, spec.under).ok_or(AnalysisError::EmptyRange)?;
        areas.push(RangeArea {
            from: lo.max(xmin),
            to: hi.min(xmax),
            area,
            points,
        });
    }
    // Un débordement (aire infinie) n'est pas une aire : NaN, comme les autres
    // valeurs non finies du module (#275), pour que la normalisation ne donne
    // pas un 0 trompeur (`y / inf`).
    let total: f64 = areas.iter().map(|a| a.area).sum();
    let total = if total.is_finite() { total } else { f64::NAN };

    let cumulative = spec.cumulative.then(|| {
        // Cumul sur la série triée, puis remis dans l'ordre d'entrée.
        let mut order: Vec<usize> = (0..series.len()).collect();
        order.sort_by(|&a, &b| series.x[a].total_cmp(&series.x[b]));
        let mut out = vec![0.0; series.len()];
        let mut acc = 0.0;
        for k in 0..order.len() {
            if k > 0 {
                let (a, b) = (order[k - 1], order[k]);
                acc += 0.5 * (series.y[a] + series.y[b]) * (series.x[b] - series.x[a]);
            }
            out[order[k]] = if acc.is_finite() { acc } else { f64::NAN };
        }
        out
    });
    let normalized = spec.normalize.then(|| {
        series
            .y
            .iter()
            .map(|y| {
                let v = y / total;
                if v.is_finite() {
                    v
                } else {
                    f64::NAN
                }
            })
            .collect()
    });
    Ok(Integrated {
        areas,
        total,
        cumulative,
        normalized,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> IntegrateSpec {
        IntegrateSpec {
            ranges: vec![],
            under: Under::Zero,
            cumulative: false,
            normalize: false,
        }
    }

    fn parabola() -> Series {
        let x: Vec<f64> = (0..=1000).map(|i| i as f64 / 1000.0).collect();
        let y = x.iter().map(|v| v * v).collect();
        Series::new(x, y)
    }

    #[test]
    fn aire_totale_d_une_parabole() {
        let r = integrate(&parabola(), &spec()).unwrap();
        assert!((r.total - 1.0 / 3.0).abs() < 1e-6, "{}", r.total);
        assert_eq!(r.areas.len(), 1);
        assert_eq!(r.areas[0].points, 1001);
    }

    #[test]
    fn la_plage_est_interpolee_aux_bornes() {
        // Bornes entre deux échantillons : Y est affine par morceaux, l'aire de
        // [0.25, 0.75] de y = x sur grille grossière est exacte.
        let s = Series::new(vec![0.0, 0.5, 1.0], vec![0.0, 0.5, 1.0]);
        let mut sp = spec();
        sp.ranges = vec![(0.25, 0.75)];
        let r = integrate(&s, &sp).unwrap();
        assert!((r.total - 0.25).abs() < 1e-12, "{}", r.total);
    }

    #[test]
    fn plusieurs_plages_donnent_une_aire_chacune() {
        let mut sp = spec();
        sp.ranges = vec![(0.0, 0.5), (0.5, 1.0)];
        let r = integrate(&parabola(), &sp).unwrap();
        assert!((r.areas[0].area - 1.0 / 24.0).abs() < 1e-6);
        assert!((r.areas[1].area - 7.0 / 24.0).abs() < 1e-6);
        assert!((r.total - 1.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn sous_la_droite_entre_les_bornes() {
        // Pic triangulaire sur un fond plat à 2 : aire au-dessus de la droite = 1.
        let s = Series::new(vec![0.0, 1.0, 2.0, 3.0, 4.0], vec![2.0, 2.0, 3.0, 2.0, 2.0]);
        let mut sp = spec();
        sp.under = Under::Line;
        let r = integrate(&s, &sp).unwrap();
        assert!((r.total - 1.0).abs() < 1e-12);
        sp.under = Under::Zero;
        assert!((integrate(&s, &sp).unwrap().total - 9.0).abs() < 1e-12);
    }

    #[test]
    fn un_spectre_decroissant_en_x_donne_une_aire_positive() {
        let s = Series::new(vec![4.0, 3.0, 2.0, 1.0], vec![1.0, 1.0, 1.0, 1.0]);
        assert!((integrate(&s, &spec()).unwrap().total - 3.0).abs() < 1e-12);
    }

    #[test]
    fn cumul_et_normalisation() {
        let s = Series::new(vec![0.0, 1.0, 2.0], vec![2.0, 2.0, 2.0]);
        let mut sp = spec();
        sp.cumulative = true;
        sp.normalize = true;
        let r = integrate(&s, &sp).unwrap();
        assert_eq!(r.cumulative.unwrap(), vec![0.0, 2.0, 4.0]);
        assert_eq!(r.normalized.unwrap(), vec![0.5, 0.5, 0.5]);
    }

    #[test]
    fn cumul_dans_l_ordre_d_entree_pour_un_x_decroissant() {
        let s = Series::new(vec![2.0, 1.0, 0.0], vec![2.0, 2.0, 2.0]);
        let mut sp = spec();
        sp.cumulative = true;
        let c = integrate(&s, &sp).unwrap().cumulative.unwrap();
        // Depuis le plus petit X : x=0 → 0, x=1 → 2, x=2 → 4.
        assert_eq!(c, vec![4.0, 2.0, 0.0]);
    }

    #[test]
    fn normalisation_par_une_aire_nulle_donne_des_cellules_vides() {
        let s = Series::new(vec![0.0, 1.0], vec![0.0, 0.0]);
        let mut sp = spec();
        sp.normalize = true;
        let r = integrate(&s, &sp).unwrap();
        assert!(r.normalized.unwrap().iter().all(|v| v.is_nan()));
    }

    #[test]
    fn debordement_de_l_aire_donne_nan_et_non_zero() {
        let s = Series::new(vec![0.0, 10.0], vec![1e308, 1e308]);
        let mut sp = spec();
        sp.cumulative = true;
        sp.normalize = true;
        let r = integrate(&s, &sp).unwrap();
        assert!(r.total.is_nan());
        assert!(r.normalized.unwrap().iter().all(|v| v.is_nan()));
        let c = r.cumulative.unwrap();
        assert_eq!(c[0], 0.0);
        assert!(c[1].is_nan());
    }

    #[test]
    fn plage_hors_des_donnees_et_serie_trop_courte() {
        let mut sp = spec();
        sp.ranges = vec![(5.0, 6.0)];
        assert_eq!(
            integrate(&parabola(), &sp).unwrap_err(),
            AnalysisError::EmptyRange
        );
        assert!(matches!(
            integrate(&Series::new(vec![1.0], vec![1.0]), &spec()),
            Err(AnalysisError::NotEnoughPoints { .. })
        ));
    }

    #[test]
    fn plage_partiellement_hors_donnees_est_rognee() {
        let mut sp = spec();
        sp.ranges = vec![(-3.0, 0.5)];
        let r = integrate(&parabola(), &sp).unwrap();
        assert_eq!(r.areas[0].from, 0.0);
        assert!((r.total - 1.0 / 24.0).abs() < 1e-6);
    }

    fn rampe() -> Series {
        let x: Vec<f64> = (0..=10).map(f64::from).collect();
        Series::new(x.clone(), x)
    }

    fn avec_plages(ranges: Vec<(f64, f64)>) -> IntegrateSpec {
        let mut sp = spec();
        sp.ranges = ranges;
        sp
    }

    fn plages_invalides() -> Result<Integrated, AnalysisError> {
        Err(AnalysisError::InvalidParameter {
            name: "ranges".into(),
        })
    }

    #[test]
    fn toute_borne_non_finie_est_refusee_quelle_que_soit_la_position() {
        let (nan, inf) = (f64::NAN, f64::INFINITY);
        let fautives = [
            (nan, nan),
            (nan, 5.0),
            (5.0, nan),
            (inf, 5.0),
            (-inf, 5.0),
            (2.0, inf),
            (2.0, -inf),
            (-inf, inf),
        ];
        for f in fautives {
            // Seule, ou après / avant une plage valide : l'ordre ne compte pas.
            for ranges in [vec![f], vec![(1.0, 2.0), f], vec![f, (1.0, 2.0)]] {
                assert_eq!(
                    integrate(&rampe(), &avec_plages(ranges.clone())),
                    plages_invalides(),
                    "{ranges:?}"
                );
            }
        }
    }

    #[test]
    fn la_garde_de_plage_passe_avant_le_calcul_du_cumul_et_de_la_normalisation() {
        let mut sp = avec_plages(vec![(f64::NAN, f64::NAN)]);
        sp.cumulative = true;
        sp.normalize = true;
        assert_eq!(integrate(&rampe(), &sp), plages_invalides());
    }

    #[test]
    fn une_serie_trop_courte_est_signalee_avant_une_plage_non_finie() {
        let s = Series::new(vec![1.0], vec![1.0]);
        assert_eq!(
            integrate(&s, &avec_plages(vec![(f64::NAN, f64::NAN)])),
            Err(AnalysisError::NotEnoughPoints {
                found: 1,
                needed: 2
            })
        );
    }

    #[test]
    fn plage_inversee_acceptee_et_normalisee() {
        let r = integrate(&rampe(), &avec_plages(vec![(8.0, 2.0)])).unwrap();
        // Aire de y = x sur [2, 8] : (64 - 4) / 2 = 30.
        assert_eq!(r.areas[0].from, 2.0);
        assert_eq!(r.areas[0].to, 8.0);
        assert_eq!(r.areas[0].area, 30.0);
        assert_eq!(r.total, 30.0);
    }

    #[test]
    fn plage_degeneree_est_une_plage_vide() {
        assert_eq!(
            integrate(&rampe(), &avec_plages(vec![(5.0, 5.0)])),
            Err(AnalysisError::EmptyRange)
        );
        // Une plage vide après une valide fait échouer le tout.
        assert_eq!(
            integrate(&rampe(), &avec_plages(vec![(1.0, 2.0), (5.0, 5.0)])),
            Err(AnalysisError::EmptyRange)
        );
    }

    #[test]
    fn plage_hors_donnees_de_l_autre_cote_est_vide() {
        assert_eq!(
            integrate(&rampe(), &avec_plages(vec![(-6.0, -5.0)])),
            Err(AnalysisError::EmptyRange)
        );
        // Plage qui ne touche les données qu'en un point : dégénérée.
        assert_eq!(
            integrate(&rampe(), &avec_plages(vec![(10.0, 12.0)])),
            Err(AnalysisError::EmptyRange)
        );
    }

    #[test]
    fn liste_de_plages_vide_integre_toute_la_courbe() {
        let r = integrate(&rampe(), &spec()).unwrap();
        assert_eq!(r.total, 50.0);
        assert_eq!(r.areas.len(), 1);
        assert_eq!((r.areas[0].from, r.areas[0].to), (0.0, 10.0));
        assert_eq!(r.areas[0].points, 11);
    }

    #[test]
    fn plages_chevauchantes_comptees_chacune_et_additionnees() {
        // Comportement actuel : le recouvrement [4, 6] est compté deux fois.
        let r = integrate(&rampe(), &avec_plages(vec![(2.0, 6.0), (4.0, 8.0)])).unwrap();
        assert_eq!(r.areas[0].area, 16.0);
        assert_eq!(r.areas[1].area, 24.0);
        assert_eq!(r.total, 40.0);
    }

    #[test]
    fn cumul_et_normalisation_ordinaires_sur_une_rampe() {
        let mut sp = spec();
        sp.cumulative = true;
        sp.normalize = true;
        let s = Series::new(vec![0.0, 1.0, 2.0, 3.0], vec![0.0, 1.0, 2.0, 3.0]);
        let r = integrate(&s, &sp).unwrap();
        assert_eq!(r.total, 4.5);
        assert_eq!(r.cumulative.unwrap(), vec![0.0, 0.5, 2.0, 4.5]);
        assert_eq!(
            r.normalized.unwrap(),
            vec![0.0, 1.0 / 4.5, 2.0 / 4.5, 3.0 / 4.5]
        );
    }

    #[test]
    fn debordement_total_infini_negatif_donne_aussi_nan() {
        let s = Series::new(vec![0.0, 10.0], vec![-1e308, -1e308]);
        let mut sp = spec();
        sp.normalize = true;
        let r = integrate(&s, &sp).unwrap();
        assert!(r.total.is_nan());
        assert!(r.normalized.unwrap().iter().all(|v| v.is_nan()));
    }

    #[test]
    fn juste_sous_le_debordement_le_resultat_reste_fini_et_exact() {
        let s = Series::new(vec![0.0, 10.0], vec![1e307, 1e307]);
        let mut sp = spec();
        sp.cumulative = true;
        sp.normalize = true;
        let r = integrate(&s, &sp).unwrap();
        assert_eq!(r.total, 1e308);
        assert_eq!(r.areas[0].area, 1e308);
        assert_eq!(r.cumulative.unwrap(), vec![0.0, 1e308]);
        assert!(r
            .normalized
            .unwrap()
            .iter()
            .all(|v| (v - 0.1).abs() < 1e-15));
    }

    #[test]
    fn debordement_d_une_plage_parmi_deux_rend_le_total_nan_mais_garde_l_autre_aire() {
        let s = Series::new(vec![0.0, 1.0, 2.0, 12.0], vec![1.0, 1.0, 1e308, 1e308]);
        let r = integrate(&s, &avec_plages(vec![(0.0, 1.0), (2.0, 12.0)])).unwrap();
        assert_eq!(r.areas[0].area, 1.0);
        assert!(r.areas[1].area.is_infinite());
        assert!(r.total.is_nan());
    }

    #[test]
    fn normalisation_par_une_aire_nulle_avec_y_non_nul_donne_nan_et_non_infini() {
        // Aire nulle (+1 puis -1) mais Y non nul : `y / 0` = ±inf → NaN.
        let s = Series::new(vec![0.0, 1.0, 2.0], vec![1.0, 0.0, -1.0]);
        let mut sp = spec();
        sp.normalize = true;
        let r = integrate(&s, &sp).unwrap();
        assert_eq!(r.total, 0.0);
        assert!(r.normalized.unwrap().iter().all(|v| v.is_nan()));
    }
}
