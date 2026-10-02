//! Le **fil de script** de StokesForge : cycle de vie de l'interpréteur embarqué,
//! interruption, et la surface `pyst` de navigation, lecture et écriture offerte aux scripts Python.
//!
//! C'est la dernière pièce du socle : elle rend vrai — ou non — le critère de
//! fin du chantier.
//!
//! > Un script Python lit une colonne **sans copie**, en écrit une autre **via
//! > le bus**, et `Ctrl+Z` annule ce que le script vient d'écrire.
//!
//! # Les trois règles du GIL, et comment le compilateur les tient
//!
//! 1. **Le fil de l'interface ne touche jamais à Python.** Rien ici ne
//!    l'oblige à le faire : [`ScriptEngine::run`] s'appelle depuis n'importe
//!    quel fil et délègue au fil de script.
//! 2. **Le fil de script relâche le GIL pendant qu'il attend l'acteur.**
//!    Chaque appel de la surface `pyst` traverse `Python::detach`. Le marqueur
//!    `Ungil` de PyO3 **interdit à la compilation** qu'une valeur liée au GIL
//!    entre dans le bloc d'attente ou en ressorte : ce n'est pas une
//!    discipline à tenir, c'est une erreur de compilation si on la manque.
//! 3. **L'acteur ne prend jamais le GIL.** `stokesforge-session` ne dépend pas de
//!    `stokesforge-python` : le type d'un objet Python n'y serait même pas
//!    nommable. C'est ce qui rend le blocage mutuel structurellement
//!    impossible — sans cette règle, le script tiendrait le GIL en attendant
//!    l'acteur pendant que l'acteur attendrait le GIL.
//!
//! **Le chemin caché** : une requête ou une réponse qui transporterait un
//! objet Python serait détruite **sur le fil de l'acteur** ; or détruire un
//! objet Python décrémente son compteur de références, ce qui est une
//! opération Python. D'où la règle qui ferme l'invariant : **aucun objet
//! Python ne traverse le canal**, ni à l'aller, ni au retour, ni comme champ
//! enfoui. Les deux garde-fous ci-dessus la tiennent sans vigilance humaine.
//!
//! **Limite assumée** : un script exécutant une longue boucle Python **pure**
//! garde le GIL entre deux points de contrôle de l'interpréteur. C'est
//! inhérent à CPython et ce crate ne prétend pas le résoudre.
//!
//! # Le fil de script est un service permanent
//!
//! Le fil qui initialise l'interpréteur devient le **fil principal** de
//! CPython. Comme le fil de l'interface ne touche jamais à Python, c'est
//! nécessairement le fil de script — donc un fil créé *par exécution* ferait
//! mourir le fil principal de CPython avec le premier script. Le fil est donc
//! créé **une fois** par [`ScriptEngine::spawn`] et vit jusqu'à
//! [`ScriptEngine::shutdown`]. **Un script à la fois** : un second
//! [`ScriptEngine::run`] pendant qu'un script tourne rend
//! [`ScriptError::Busy`].
//!
//! # Écritures groupées (#67)
//!
//! Les créations, renommages, suppressions, métadonnées et valeurs passent
//! par les commandes existantes. Une exécution réussie se défait en un seul
//! Ctrl+Z ; erreur non rattrapée et interruption annulent le groupe courant.
//! La réservation reste paresseuse : une exécution de lecture ne l'ouvre pas.
//!
//! # Feature `python`
//!
//! Tout est derrière la feature Cargo `python`, absente de `default`. Sans
//! elle, ce crate compile **vide** et ne tire aucune dépendance PyO3 :
//! `cargo build`, `cargo test` et `cargo tree` restent propres sur une machine
//! sans interpréteur vendorisé.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

#[cfg(feature = "python")]
mod bridge;
#[cfg(feature = "python")]
mod capture;
#[cfg(feature = "python")]
mod engine;
#[cfg(feature = "python")]
mod error;
#[cfg(feature = "python")]
mod graph;
#[cfg(feature = "python")]
mod outcome;
#[cfg(feature = "python")]
mod selftest;
#[cfg(feature = "python")]
mod surface;

#[cfg(feature = "python")]
pub use engine::ScriptEngine;
#[cfg(feature = "python")]
pub use error::ScriptError;
#[cfg(feature = "python")]
pub use outcome::ScriptOutcome;
#[cfg(feature = "python")]
pub use selftest::{run_selftest, SelfTestReport, SelfTestStep};
#[cfg(feature = "python")]
pub use surface::{BrokenReferenceError, Handle, InvalidValueError, UnknownPathError};

/// Indique si ce binaire embarque le fil de script (feature `python`).
///
/// Permet à l'interface de masquer sa console de script sans avoir à tester
/// la feature elle-même.
pub const SCRIPT_ENABLED: bool = cfg!(feature = "python");

#[cfg(test)]
mod tests {
    use super::SCRIPT_ENABLED;

    /// Compile et passe dans les deux configurations de features.
    #[test]
    fn script_enabled_reflete_la_feature() {
        assert_eq!(SCRIPT_ENABLED, cfg!(feature = "python"));
    }
}
