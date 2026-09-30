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
//!
//! [`preview`] classe chaque ligne d'un fichier en rôle (en-tête principal,
//! sous-en-tête et son rôle éventuel, donnée, pied de page), façon
//! l'assistant d'import ASCII d'OriginLab, pour que la fenêtre d'import
//! montre en direct comment un fichier va être découpé pendant que
//! l'utilisateur règle la recette — voir sa doc de tête pour le budget de
//! coût sur un gros fichier.
//!
//! [`decode`] transforme les octets en texte avant tout découpage : UTF-8,
//! UTF-16 avec marque d'ordre, et en repli Windows-1252, la page de code
//! des logiciels d'appareil sous Windows.

mod data_end;
pub mod decode;
mod layout;
pub mod metadata;
mod numbers;
pub mod parse;
pub mod preview;
pub mod recent;
pub mod recipe;
pub mod store;

pub use decode::*;
pub use metadata::MetadataLine;
pub use parse::*;
pub use preview::*;
pub use recent::{RecentFiles, MAX_RECENT_FILES};
pub use recipe::*;
pub use store::*;
