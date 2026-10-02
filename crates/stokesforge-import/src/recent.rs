//! Fichiers importés récemment, et dossier mémorisé : un fichier JSON à côté
//! des recettes (`import_recent.json`), même politique que [`crate::store`] —
//! jamais de panique si le fichier est absent, illisible ou corrompu
//! (invariant 10). Le dossier mémorisé est celui du fichier le plus récent.

use std::path::{Path, PathBuf};

/// Nombre maximal de fichiers récents gardés.
pub const MAX_RECENT_FILES: usize = 10;

/// Les fichiers importés récemment, du plus récent au plus ancien.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RecentFiles {
    files: Vec<PathBuf>,
}

impl RecentFiles {
    /// Charge depuis le dossier de config utilisateur ; liste vide au moindre
    /// problème.
    pub fn load() -> Self {
        recent_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    /// Écrit sur disque (crée le dossier de config s'il n'existe pas).
    pub fn save(&self) -> std::io::Result<()> {
        let path = recent_path().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "aucun dossier de config exploitable pour cet OS",
            )
        })?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Les fichiers, du plus récent au plus ancien.
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// Place `paths` en tête (dans l'ordre donné : le premier devient le plus
    /// récent), sans doublon, borné à [`MAX_RECENT_FILES`].
    pub fn record(&mut self, paths: &[PathBuf]) {
        let mut files: Vec<PathBuf> = Vec::new();
        for path in paths.iter().chain(self.files.iter()) {
            if !files.contains(path) {
                files.push(path.clone());
            }
        }
        files.truncate(MAX_RECENT_FILES);
        self.files = files;
    }

    /// Le dossier du fichier le plus récent : là où rouvrir le sélecteur.
    pub fn last_dir(&self) -> Option<&Path> {
        self.files.first().and_then(|file| file.parent())
    }
}

fn recent_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "StokesForge")
        .map(|dirs| dirs.config_dir().join("import_recent.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str) -> PathBuf {
        PathBuf::from(format!("/data/{name}"))
    }

    #[test]
    fn record_puts_latest_first_without_duplicates() {
        let mut recent = RecentFiles::default();
        recent.record(&[p("a.csv"), p("b.csv")]);
        recent.record(&[p("b.csv"), p("c.csv")]);
        assert_eq!(recent.files(), &[p("b.csv"), p("c.csv"), p("a.csv")]);
        assert_eq!(recent.last_dir(), Some(Path::new("/data")));
    }

    #[test]
    fn the_list_is_bounded() {
        let mut recent = RecentFiles::default();
        for i in 0..(MAX_RECENT_FILES + 5) {
            recent.record(&[p(&format!("{i}.csv"))]);
        }
        assert_eq!(recent.files().len(), MAX_RECENT_FILES);
        assert_eq!(
            recent.files()[0],
            p(&format!("{}.csv", MAX_RECENT_FILES + 4))
        );
    }

    #[test]
    fn a_corrupt_file_reads_as_empty() {
        let parsed: Option<RecentFiles> = serde_json::from_str("{ pas du json").ok();
        assert!(parsed.is_none());
        assert!(RecentFiles::default().last_dir().is_none());
    }
}
