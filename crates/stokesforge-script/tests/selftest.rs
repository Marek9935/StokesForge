//! Intègre [`stokesforge_script::run_selftest`] à la suite de tests ordinaire :
//! rejoue le critère de fin du socle et affirme que chaque étape est passée.
//!
//! Un seul test dans ce fichier, à dessein. `cargo test` compile chaque
//! fichier de `tests/` en son **propre binaire**, donc son propre processus :
//! c'est ce qui isole cette vérification de `tests/script.rs` sans avoir à
//! partager son verrou d'accès à l'interpréteur — CPython ne démarre qu'une
//! fois par processus, et [`run_selftest`] démarre son propre moteur de
//! script. Un second test dans ce fichier tournerait sur un fil du même
//! processus et se heurterait au « un script à la fois » du moteur ; un seul
//! test l'évite sans discipline à tenir.
//!
//! Exige le préfixe vendorisé : lance `scripts/python-vendor.sh` puis
//! `scripts/python-test.sh` (qui pose `STOKESFORGE_PYTHON_HOME` avant d'appeler
//! `cargo test`).

#![cfg(feature = "python")]

use std::path::PathBuf;

use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::run_selftest;

/// Résout le préfixe embarqué : `STOKESFORGE_PYTHON_HOME` si posée par le script
/// de test, sinon `vendor/python/extracted/python` relatif au manifeste —
/// même résolution que `tests/script.rs`.
fn home() -> PathBuf {
    match std::env::var_os(interpreter::HOME_ENV) {
        Some(value) => PathBuf::from(value),
        None => {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        }
    }
}

#[test]
fn le_critere_de_fin_du_socle_passe_de_bout_en_bout_depuis_larbre_de_sources() {
    let report = run_selftest(EmbeddedPython::with_home(home()));

    for step in &report.steps {
        assert!(
            step.passed,
            "étape en échec : {} — {}\n\ncompte rendu complet :\n{}",
            step.name,
            step.detail,
            report.render()
        );
    }
    assert!(
        report.passed(),
        "compte rendu complet :\n{}",
        report.render()
    );
}
