//! Le niveau d'alerte d'un R² (#288), **sans egui** : l'état dit « orange »,
//! « rouge » ou « boîte de dialogue » ; `stokesforge-app` choisit la teinte
//! (`theme::r_squared_color`) et peint la boîte.
//!
//! Les paliers se cumulent : un R² sous [`DIALOG_BELOW`] est aussi rouge et
//! aussi orange. Les bornes sont strictes (`0,95` pile n'avertit pas) ; un R²
//! négatif — ajustement pire qu'une droite horizontale — relève du palier le
//! plus bas. `NaN` (pas de valeur) n'avertit de rien : on n'alerte pas sur ce
//! qu'on ne sait pas mesurer.

use stokesforge_analysis::ops::OperationKind;
use stokesforge_core::{Calculation, Tree};

use crate::tr;

/// La clé des résultats qui porte R² (ajustements et pics).
pub const R_SQUARED_KEY: &str = "stat.r_squared";

/// Sous cette valeur (stricte), R² s'écrit en orange.
pub const ORANGE_BELOW: f64 = 0.95;
/// Sous cette valeur (stricte), R² s'écrit en rouge.
pub const RED_BELOW: f64 = 0.8;
/// Sous cette valeur (stricte), R² ouvre aussi la boîte de dialogue, si ni la
/// préférence de l'application ni le réglage du calcul ne l'ont coupée.
pub const DIALOG_BELOW: f64 = 0.6;

/// Le palier atteint par un R², du plus doux au plus fort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RSquaredLevel {
    /// Aucun avertissement (bon R², ou `NaN`).
    Good,
    /// `R² < 0,95`.
    Orange,
    /// `R² < 0,8`.
    Red,
    /// `R² < 0,6` : rouge, et boîte de dialogue.
    Dialog,
}

impl RSquaredLevel {
    /// Le palier de `r_squared`.
    pub fn of(r_squared: f64) -> Self {
        if r_squared.is_nan() {
            Self::Good
        } else if r_squared < DIALOG_BELOW {
            Self::Dialog
        } else if r_squared < RED_BELOW {
            Self::Red
        } else if r_squared < ORANGE_BELOW {
            Self::Orange
        } else {
            Self::Good
        }
    }

    /// La valeur se peint-elle en couleur (orange ou plus) ?
    pub fn is_warning(self) -> bool {
        self >= Self::Orange
    }

    /// La valeur se peint-elle en rouge (rouge ou plus) ?
    pub fn is_red(self) -> bool {
        self >= Self::Red
    }
}

/// Le palier d'une valeur calculée : celui de son R² si `key` est
/// [`R_SQUARED_KEY`], sinon [`RSquaredLevel::Good`].
pub fn level_of_value(key: &str, value: f64) -> RSquaredLevel {
    if key == R_SQUARED_KEY {
        RSquaredLevel::of(value)
    } else {
        RSquaredLevel::Good
    }
}

/// Les R² du calcul qui franchissent le palier de la boîte, par série, en bits
/// (une empreinte exacte : deux recalculs au même résultat n'avertissent pas
/// deux fois). Vide : aucun défaut.
pub fn dialog_fingerprint(calc: &Calculation) -> Vec<(usize, u64)> {
    let mut out = Vec::new();
    for (rank, result) in calc.results.iter().enumerate() {
        for value in &result.values {
            if level_of_value(&value.key, value.value) == RSquaredLevel::Dialog {
                out.push((rank, value.value.to_bits()));
            }
        }
    }
    out
}

/// Le nombre de **séries** du calcul dont le R² franchit le palier de la boîte
/// (#290) : l'unité de compte est la série, une courbe = un R².
pub fn low_series_count(calc: &Calculation) -> usize {
    calc.results
        .iter()
        .filter(|result| {
            result
                .values
                .iter()
                .any(|v| level_of_value(&v.key, v.value) == RSquaredLevel::Dialog)
        })
        .count()
}

/// Les séries en défaut d'un type de calcul.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LowRSquaredGroup {
    pub kind: OperationKind,
    /// Toujours au moins 1.
    pub count: usize,
}

/// Regroupe des `(type, nombre de séries)` par type, dans l'ordre fixe de
/// [`OperationKind::ALL`] (stable d'un instantané à l'autre) ; les types sans
/// série en défaut disparaissent.
pub fn group_by_kind(
    items: impl IntoIterator<Item = (OperationKind, usize)>,
) -> Vec<LowRSquaredGroup> {
    let mut totals = [0usize; OperationKind::ALL.len()];
    for (kind, count) in items {
        if let Some(rank) = OperationKind::ALL.iter().position(|k| *k == kind) {
            totals[rank] += count;
        }
    }
    OperationKind::ALL
        .iter()
        .zip(totals)
        .filter(|(_, count)| *count > 0)
        .map(|(kind, count)| LowRSquaredGroup { kind: *kind, count })
        .collect()
}

/// Les R² faibles de tout le projet (#290), regroupés par type : tous les
/// calculs qui gardent leurs valeurs, quel que soit leur réglage
/// `low_r2_dialog` (le réglage coupe la boîte, pas l'information). Un calcul
/// « Show » ne garde rien : il n'y compte pas.
pub fn project_groups(tree: &Tree) -> Vec<LowRSquaredGroup> {
    group_by_kind(
        tree.calculations()
            .values()
            .map(|calc| (calc.operation.kind(), low_series_count(calc))),
    )
}

/// Le total de séries des groupes.
pub fn total(groups: &[LowRSquaredGroup]) -> usize {
    groups.iter().map(|g| g.count).sum()
}

fn count_i64(count: usize) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

/// « 12 reconvolutions » : le compteur d'un type, avec son pluriel.
pub fn group_line(group: &LowRSquaredGroup) -> String {
    let count = count_i64(group.count);
    match group.kind {
        OperationKind::Baseline => tr!("low-r2-count-baseline", count = count),
        OperationKind::Correct => tr!("low-r2-count-correct", count = count),
        OperationKind::Integrate => tr!("low-r2-count-integrate", count = count),
        OperationKind::Fit => tr!("low-r2-count-fit", count = count),
        OperationKind::Peaks => tr!("low-r2-count-peaks", count = count),
        OperationKind::Reconvolution => tr!("low-r2-count-reconvolution", count = count),
        OperationKind::DirectDeconvolution => {
            tr!("low-r2-count-direct-deconvolution", count = count)
        }
        OperationKind::Smooth => tr!("low-r2-count-smooth", count = count),
    }
}

/// Le texte de la boîte et de la liste de la pastille : « R² < 0,6 pour : »,
/// une ligne « • N type » par groupe, puis le renvoi vers les Résultats.
pub fn summary_text(groups: &[LowRSquaredGroup]) -> String {
    let mut text = tr!("low-r2-summary-head");
    for group in groups {
        text.push_str("\n• ");
        text.push_str(&group_line(group));
    }
    text.push('\n');
    text.push_str(&tr!("low-r2-summary-see-results"));
    text
}

/// Le libellé de la pastille de la barre d'état : « ⚠ 13 R² faibles ».
pub fn badge_text(total: usize) -> String {
    tr!("status-low-r2-badge", count = count_i64(total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_paliers_ont_des_bornes_strictes() {
        use RSquaredLevel::*;
        let cas = [
            (1.0, Good),
            (0.95, Good),
            (0.9499, Orange),
            (0.8, Orange),
            (0.7999, Red),
            (0.6, Red),
            (0.5999, Dialog),
            (0.0, Dialog),
            (-3.0, Dialog),
            (f64::NEG_INFINITY, Dialog),
            (f64::NAN, Good),
        ];
        for (r2, attendu) in cas {
            assert_eq!(RSquaredLevel::of(r2), attendu, "R² = {r2}");
        }
    }

    #[test]
    fn les_paliers_se_cumulent() {
        let dialog = RSquaredLevel::of(0.1);
        assert!(dialog.is_warning() && dialog.is_red());
        let orange = RSquaredLevel::of(0.9);
        assert!(orange.is_warning() && !orange.is_red());
        assert!(!RSquaredLevel::of(0.99).is_warning());
    }

    #[test]
    fn seule_la_cle_r_carre_est_jugee() {
        assert_eq!(level_of_value("param.tau", 0.1), RSquaredLevel::Good);
        assert_eq!(level_of_value(R_SQUARED_KEY, 0.1), RSquaredLevel::Dialog);
    }

    fn suivant(x: f64) -> f64 {
        f64::from_bits(x.to_bits() + 1)
    }

    fn precedent(x: f64) -> f64 {
        f64::from_bits(x.to_bits() - 1)
    }

    /// Les trois seuils, pile et à un ulp de chaque côté : la borne est stricte.
    #[test]
    fn chaque_palier_est_strict_a_l_ulp_pres() {
        use RSquaredLevel::*;
        let cas = [
            (ORANGE_BELOW, Good),
            (precedent(ORANGE_BELOW), Orange),
            (suivant(ORANGE_BELOW), Good),
            (ORANGE_BELOW - f64::EPSILON, Orange),
            (ORANGE_BELOW + f64::EPSILON, Good),
            (RED_BELOW, Orange),
            (precedent(RED_BELOW), Red),
            (suivant(RED_BELOW), Orange),
            (RED_BELOW - f64::EPSILON, Red),
            (RED_BELOW + f64::EPSILON, Orange),
            (DIALOG_BELOW, Red),
            (precedent(DIALOG_BELOW), Dialog),
            (suivant(DIALOG_BELOW), Red),
            (DIALOG_BELOW - f64::EPSILON, Dialog),
            (DIALOG_BELOW + f64::EPSILON, Red),
        ];
        for (r2, attendu) in cas {
            assert_eq!(RSquaredLevel::of(r2), attendu, "R² = {r2:e}");
        }
        // Les seuils eux-mêmes sont ceux de l'issue.
        assert_eq!((ORANGE_BELOW, RED_BELOW, DIALOG_BELOW), (0.95, 0.8, 0.6));
    }

    #[test]
    fn les_valeurs_extremes_et_speciales() {
        use RSquaredLevel::*;
        let cas = [
            (1.0, Good),
            (1.0 + f64::EPSILON, Good),
            (0.0, Dialog),
            (-0.0, Dialog),
            (-1.0, Dialog),
            (f64::MIN, Dialog),
            (f64::NEG_INFINITY, Dialog),
            (f64::INFINITY, Good),
            (f64::MAX, Good),
            (f64::NAN, Good),
            (-f64::NAN, Good),
            (f64::MIN_POSITIVE, Dialog),
        ];
        for (r2, attendu) in cas {
            assert_eq!(RSquaredLevel::of(r2), attendu, "R² = {r2:e}");
        }
    }

    /// Les paliers se cumulent, palier par palier : `is_warning` et `is_red`
    /// pour chacun des quatre niveaux.
    #[test]
    fn le_cumul_des_paliers_est_exact() {
        use RSquaredLevel::*;
        let table = [
            (Good, false, false),
            (Orange, true, false),
            (Red, true, true),
            (Dialog, true, true),
        ];
        for (niveau, avertit, rouge) in table {
            assert_eq!(niveau.is_warning(), avertit, "{niveau:?}");
            assert_eq!(niveau.is_red(), rouge, "{niveau:?}");
        }
        assert!(Good < Orange && Orange < Red && Red < Dialog);
    }

    #[test]
    fn les_autres_cles_sont_ignorees_meme_avec_une_valeur_tres_basse() {
        for cle in ["param.tau", "area", "stat.r_squared_adj", "R_SQUARED", ""] {
            for valeur in [-5.0, 0.0, 0.3, f64::NEG_INFINITY] {
                assert_eq!(
                    level_of_value(cle, valeur),
                    RSquaredLevel::Good,
                    "{cle:?} = {valeur}"
                );
            }
        }
        assert_eq!(level_of_value(R_SQUARED_KEY, 0.9), RSquaredLevel::Orange);
        assert_eq!(level_of_value(R_SQUARED_KEY, 0.7), RSquaredLevel::Red);
        assert_eq!(level_of_value(R_SQUARED_KEY, 0.99), RSquaredLevel::Good);
        assert_eq!(R_SQUARED_KEY, "stat.r_squared");
    }

    fn valeur(key: &str, value: f64) -> stokesforge_core::ResultValue {
        stokesforge_core::ResultValue {
            key: key.to_string(),
            index: None,
            value,
            error: None,
        }
    }

    fn calcul(series: Vec<Vec<(&str, f64)>>) -> Calculation {
        use stokesforge_analysis::correct::CorrectOp;
        use stokesforge_analysis::ops::Operation;
        Calculation {
            operation: Operation::Correct {
                op: CorrectOp::Subtract,
            },
            series: Vec::new(),
            aux: None,
            outputs: Vec::new(),
            results: series
                .into_iter()
                .map(|values| stokesforge_core::SeriesResult {
                    values: values.into_iter().map(|(k, v)| valeur(k, v)).collect(),
                    ..Default::default()
                })
                .collect(),
            keep_results: true,
            auto_recalc: false,
            low_r2_dialog: true,
            stale: None,
        }
    }

    #[test]
    fn l_empreinte_est_vide_sans_r2_sous_le_palier_de_la_boite() {
        let bon = calcul(vec![
            vec![(R_SQUARED_KEY, 0.99), ("param.tau", 0.1)],
            vec![(R_SQUARED_KEY, DIALOG_BELOW)],
            vec![(R_SQUARED_KEY, f64::NAN)],
            vec![("param.tau", -9.0)],
            vec![(R_SQUARED_KEY, 0.7)],
        ]);
        assert!(dialog_fingerprint(&bon).is_empty());
        assert_eq!(low_series_count(&bon), 0);
        assert!(dialog_fingerprint(&calcul(Vec::new())).is_empty());
        assert!(dialog_fingerprint(&calcul(vec![Vec::new()])).is_empty());
    }

    #[test]
    fn l_empreinte_porte_le_rang_de_la_serie_et_les_bits_du_r2() {
        let calc = calcul(vec![
            vec![(R_SQUARED_KEY, 0.99)],
            vec![(R_SQUARED_KEY, 0.2), ("param.tau", 0.1)],
            vec![(R_SQUARED_KEY, -1.0)],
        ]);
        assert_eq!(
            dialog_fingerprint(&calc),
            vec![(1, 0.2f64.to_bits()), (2, (-1.0f64).to_bits())]
        );
        assert_eq!(low_series_count(&calc), 2);
    }

    #[test]
    fn l_empreinte_est_stable_et_change_au_moindre_bit() {
        let a = calcul(vec![vec![(R_SQUARED_KEY, 0.3)]]);
        let b = calcul(vec![vec![(R_SQUARED_KEY, 0.3)]]);
        assert_eq!(dialog_fingerprint(&a), dialog_fingerprint(&b));
        // Épingle : l'empreinte est exacte, un écart de 1e-12 compte comme un
        // résultat neuf (la boîte se rouvre).
        let c = calcul(vec![vec![(R_SQUARED_KEY, 0.3 + 1e-12)]]);
        assert_ne!(dialog_fingerprint(&a), dialog_fingerprint(&c));
        // Même R² sur une autre série : autre empreinte.
        let d = calcul(vec![Vec::new(), vec![(R_SQUARED_KEY, 0.3)]]);
        assert_ne!(dialog_fingerprint(&a), dialog_fingerprint(&d));
    }

    #[test]
    fn low_series_count_compte_les_series_pas_les_valeurs() {
        let calc = calcul(vec![
            vec![(R_SQUARED_KEY, 0.1), (R_SQUARED_KEY, 0.2)],
            vec![(R_SQUARED_KEY, 0.9)],
            vec![(R_SQUARED_KEY, 0.3)],
        ]);
        assert_eq!(low_series_count(&calc), 2);
        assert_eq!(dialog_fingerprint(&calc).len(), 3, "une entrée par valeur");
    }
}
