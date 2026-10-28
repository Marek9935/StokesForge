//! The **single** rule for StokesForge's configuration folder (#332).
//!
//! Import recipes, recent files, saved colors and preferences live in the
//! same folder; the recovery copy (`recovery/`) derives from it. The rule
//! takes two lines:
//!
//! 1. the environment variable [`CONFIG_DIR_VARIABLE`]
//!    (`STOKESFORGE_CONFIG_DIR`), if it is set and not empty: that folder,
//!    as is, with no suffix and no canonicalization;
//! 2. otherwise `directories::ProjectDirs::from("", "", "StokesForge")`.
//!
//! It is also used in production (portable install: the whole profile in a
//! folder of the user's choice) and by tests: `directories` ignores
//! `XDG_CONFIG_HOME` outside Linux, so tests redirected by that variable
//! alone wrote into the real profile on Windows and macOS. A test redirection
//! therefore sets `STOKESFORGE_CONFIG_DIR` (in addition to `XDG_CONFIG_HOME`).
//!
//! `ProjectDirs::from` is called **nowhere else** (locked by
//! `stokesforge-arch-guard/tests/config_dir.rs`): every profile path goes
//! through [`config_dir`] or [`recovery_dir`].
//!
//! The `*_from` forms take the value of the variable, so they can be tested
//! without `set_var` (global process state).

use std::ffi::OsString;
use std::path::PathBuf;

/// Name of the environment variable that designates the configuration folder.
pub const CONFIG_DIR_VARIABLE: &str = "STOKESFORGE_CONFIG_DIR";

/// StokesForge's configuration folder, from the process environment. `None`
/// if the variable is missing or empty **and** the system provides no folder
/// (very rare).
pub fn config_dir() -> Option<PathBuf> {
    config_dir_from(std::env::var_os(CONFIG_DIR_VARIABLE))
}

/// The recovery copies folder: `<STOKESFORGE_CONFIG_DIR>/recovery` when the
/// variable is set, otherwise `recovery/` in the system data folder
/// (unchanged).
pub fn recovery_dir() -> Option<PathBuf> {
    recovery_dir_from(std::env::var_os(CONFIG_DIR_VARIABLE))
}

/// [`config_dir`] for a given variable value.
pub fn config_dir_from(variable: Option<OsString>) -> Option<PathBuf> {
    explicit(variable).or_else(|| system_dirs().map(|dirs| dirs.config_dir().to_path_buf()))
}

/// [`recovery_dir`] for a given variable value.
pub fn recovery_dir_from(variable: Option<OsString>) -> Option<PathBuf> {
    match explicit(variable) {
        Some(dir) => Some(dir.join("recovery")),
        None => system_dirs().map(|dirs| dirs.data_dir().join("recovery")),
    }
}

/// The folder designated by the variable: nothing if missing or empty.
fn explicit(variable: Option<OsString>) -> Option<PathBuf> {
    variable.filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// The only call to `ProjectDirs::from` in the repository.
fn system_dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("", "", "StokesForge")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system_config_dir() -> PathBuf {
        system_dirs()
            .expect("system directories")
            .config_dir()
            .to_path_buf()
    }

    #[test]
    fn a_set_variable_is_the_folder_as_is() {
        let folder = std::env::temp_dir().join("portable profile").join("sub");
        assert_eq!(
            config_dir_from(Some(folder.clone().into_os_string())),
            Some(folder),
            "no suffix and no canonicalization"
        );
    }

    #[test]
    fn an_empty_variable_falls_back_to_the_system_folder() {
        assert_eq!(
            config_dir_from(Some(OsString::new())),
            Some(system_config_dir())
        );
    }

    #[test]
    fn without_a_variable_it_is_the_system_folder() {
        assert_eq!(config_dir_from(None), Some(system_config_dir()));
    }

    #[test]
    fn recovery_follows_the_variable_otherwise_the_data_folder() {
        let folder = std::env::temp_dir().join("profile");
        assert_eq!(
            recovery_dir_from(Some(folder.clone().into_os_string())),
            Some(folder.join("recovery"))
        );
        let data = system_dirs().unwrap().data_dir().join("recovery");
        assert_eq!(recovery_dir_from(None), Some(data.clone()));
        assert_eq!(recovery_dir_from(Some(OsString::new())), Some(data));
    }
}
