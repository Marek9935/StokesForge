//! Pont Python de StokesForge : interpréteur CPython **embarqué**, isolé du système.
//!
//! Ce crate est le **seul** du dépôt autorisé à démarrer l'interpréteur et à
//! donner accès au GIL. Il ne connaît pas le modèle de données de StokesForge : il
//! ne manipule que des types scalaires et des tranches de `f64`.
//!
//! # Feature `python`
//!
//! Tout ce qui touche à PyO3 est derrière la feature Cargo `python`, absente de
//! `default`. Sans elle, ce crate compile vide et ne tire aucune dépendance
//! PyO3 : `cargo build`, `cargo test` et la CI restent verts sur une machine
//! sans interpréteur vendorisé.
//!
//! Avec la feature, la compilation exige la variable d'environnement
//! `PYO3_CONFIG_FILE` produite par `scripts/python-config.sh` (cf. README,
//! piège n°1).
//!
//! # Cycle de vie
//!
//! L'initialisation est **explicite et paresseuse** (`auto-initialize` de PyO3
//! n'est pas activée) : voir [`interpreter::EmbeddedPython::initialize`].
//! L'interpréteur n'est **jamais finalisé** : `Py_Finalize` est une source
//! connue de plantages en sortie et n'apporte rien à un processus qui se
//! termine. Conséquence assumée : les `atexit` Python ne s'exécutent pas.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

#[cfg(feature = "python")]
pub mod array;
#[cfg(feature = "python")]
pub mod interpreter;

/// Indique si ce binaire embarque le pont Python (feature `python`).
///
/// Permet au reste de l'application de dégrader proprement son interface
/// (console de script masquée, renderer Matplotlib indisponible) sans avoir à
/// tester la feature elle-même.
pub const PYTHON_ENABLED: bool = cfg!(feature = "python");

#[cfg(test)]
mod tests {
    use super::PYTHON_ENABLED;

    /// Compile et passe dans les deux configurations de features.
    #[test]
    fn python_enabled_reflete_la_feature() {
        assert_eq!(PYTHON_ENABLED, cfg!(feature = "python"));
    }
}
