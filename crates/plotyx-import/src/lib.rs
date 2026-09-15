//! Import de fichiers de données d'instruments scientifiques (CSV/TXT/ASC)
//! dans une feuille de calcul, à la façon de l'assistant d'import ASCII
//! d'OriginLab : l'utilisateur choisit une **recette** ([`ImportRecipe`])
//! décrivant le format d'un appareil donné (séparateur, séparateur
//! décimal, lignes d'en-tête à ignorer, nom de série…), l'applique à un
//! fichier via [`parse`], et peut l'enregistrer pour la réutiliser
//! ([`RecipeStore`]).
//!
//! Crate volontairement pur : zéro dépendance UI, zéro dépendance à
//! `plotyx-commands`/`plotyx-session`, et zéro dépendance à `plotyx-core` —
//! le vocabulaire de sortie du parseur (`f64`, `NaN` pour une valeur
//! manquante) est le plus simple qui existe, pour rester indépendant.
//! `plotyx-app` fait la conversion vers `plotyx_core::Column` de son côté.

pub mod parse;
pub mod recipe;
pub mod store;

pub use parse::*;
pub use recipe::*;
pub use store::*;
