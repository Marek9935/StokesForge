//! Traitement mathématique des données de StokesForge (chantier #181).
//!
//! Crate de **calcul pur** : lignes de base, correction par une référence,
//! intégrales, ajustements, décomposition en pics, déconvolution, lissage. Il ne dépend
//! d'aucun crate du workspace (comme `stokesforge-import`), ni d'une interface,
//! ni de Python : tout ce que les menus proposent fonctionne sans interpréteur
//! (décision du 2026-10-08). Le vocabulaire est celui des données brutes : des
//! `f64` où `NaN` dit « vide », « texte » ou « invalide ».
//!
//! Aucun nom d'interface ici : les résultats portent des **clés stables**
//! (`area`, `tau`, `r_squared`…) que `stokesforge-app` met en libellés.

// Deux lints du calcul numérique, écartés ici avec leur raison :
// - `!(x > 0.0)` est voulu : il est vrai aussi pour `NaN`, ce que `x <= 0.0` ne
//   serait pas ; réécrire en `partial_cmp` alourdirait chaque garde ;
// - les boucles d'algèbre indexent plusieurs tableaux par les mêmes indices
//   (`a[i][j]`, `x[j]`) : un itérateur y est moins lisible que l'indice.
#![allow(clippy::neg_cmp_op_on_partial_ord, clippy::needless_range_loop)]

pub mod baseline;
pub mod correct;
pub mod deconv;
pub mod error;
pub mod expr;
pub mod fit;
pub mod integrate;
pub mod linalg;
pub mod model;
pub mod ops;
pub mod peaks;
pub mod series;
pub mod smooth;
pub mod solver;
pub mod tree;
pub mod voigt;

pub use error::AnalysisError;
