//! La règle **unique** du dossier de configuration de StokesForge (#332).
//!
//! Recettes d'import, fichiers récents, couleurs enregistrées et préférences
//! vivent dans le même dossier ; la copie de secours (`recovery/`) en dérive.
//! Cette règle tient en deux lignes :
//!
//! 1. la variable d'environnement [`CONFIG_DIR_VARIABLE`]
//!    (`STOKESFORGE_CONFIG_DIR`), si elle est posée et non vide : ce dossier,
//!    tel quel, sans suffixe ni canonicalisation ;
//! 2. sinon `directories::ProjectDirs::from("", "", "StokesForge")`.
//!
//! Elle sert aussi en production (installation portable : tout le profil dans
//! un dossier au choix) et aux tests : `directories` ignore `XDG_CONFIG_HOME`
//! hors Linux, si bien que les tests redirigés par cette seule variable
//! écrivaient dans le vrai profil sous Windows et macOS. Une redirection de
//! test pose donc `STOKESFORGE_CONFIG_DIR` (en plus de `XDG_CONFIG_HOME`).
//!
//! `ProjectDirs::from` ne s'appelle **nulle part ailleurs** (verrouillé par
//! `stokesforge-arch-guard/tests/config_dir.rs`) : tout chemin du profil passe
//! par [`config_dir`] ou [`recovery_dir`].
//!
//! Les formes `*_from` reçoivent la valeur de la variable, pour se tester sans
//! `set_var` (état global du processus).

use std::ffi::OsString;
use std::path::PathBuf;

/// Nom de la variable d'environnement qui désigne le dossier de configuration.
pub const CONFIG_DIR_VARIABLE: &str = "STOKESFORGE_CONFIG_DIR";

/// Le dossier de configuration de StokesForge, d'après l'environnement du
/// processus. `None` si la variable est absente ou vide **et** que le système
/// ne fournit aucun dossier (très rare).
pub fn config_dir() -> Option<PathBuf> {
    config_dir_from(std::env::var_os(CONFIG_DIR_VARIABLE))
}

/// Le dossier des copies de secours : `<STOKESFORGE_CONFIG_DIR>/recovery`
/// quand la variable est posée, sinon `recovery/` dans le dossier de données
/// du système (inchangé).
pub fn recovery_dir() -> Option<PathBuf> {
    recovery_dir_from(std::env::var_os(CONFIG_DIR_VARIABLE))
}

/// [`config_dir`] pour une valeur de variable donnée.
pub fn config_dir_from(variable: Option<OsString>) -> Option<PathBuf> {
    explicit(variable).or_else(|| system_dirs().map(|dirs| dirs.config_dir().to_path_buf()))
}

/// [`recovery_dir`] pour une valeur de variable donnée.
pub fn recovery_dir_from(variable: Option<OsString>) -> Option<PathBuf> {
    match explicit(variable) {
        Some(dir) => Some(dir.join("recovery")),
        None => system_dirs().map(|dirs| dirs.data_dir().join("recovery")),
    }
}

/// Le dossier désigné par la variable : absente ou vide, rien.
fn explicit(variable: Option<OsString>) -> Option<PathBuf> {
    variable.filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Le seul appel à `ProjectDirs::from` du dépôt.
fn system_dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("", "", "StokesForge")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn systeme() -> PathBuf {
        system_dirs()
            .expect("dossiers du système")
            .config_dir()
            .to_path_buf()
    }

    #[test]
    fn une_variable_posee_est_le_dossier_tel_quel() {
        let dossier = std::env::temp_dir().join("profil portable").join("sub");
        assert_eq!(
            config_dir_from(Some(dossier.clone().into_os_string())),
            Some(dossier),
            "sans suffixe ni canonicalisation"
        );
    }

    #[test]
    fn une_variable_vide_retombe_sur_le_dossier_du_systeme() {
        assert_eq!(config_dir_from(Some(OsString::new())), Some(systeme()));
    }

    #[test]
    fn sans_variable_c_est_le_dossier_du_systeme() {
        assert_eq!(config_dir_from(None), Some(systeme()));
    }

    #[test]
    fn la_recuperation_suit_la_variable_sinon_le_dossier_de_donnees() {
        let dossier = std::env::temp_dir().join("profil");
        assert_eq!(
            recovery_dir_from(Some(dossier.clone().into_os_string())),
            Some(dossier.join("recovery"))
        );
        let donnees = system_dirs().unwrap().data_dir().join("recovery");
        assert_eq!(recovery_dir_from(None), Some(donnees.clone()));
        assert_eq!(recovery_dir_from(Some(OsString::new())), Some(donnees));
    }
}
