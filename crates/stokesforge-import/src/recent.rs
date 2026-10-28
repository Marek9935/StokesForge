//! Recently imported files, and the remembered folder: a JSON file next to
//! the recipes (`import_recent.json`), same policy as [`crate::store`]:
//! never panic if the file is missing, unreadable or corrupt (invariant 10).
//! The remembered folder is that of the most recent file.

use std::path::{Path, PathBuf};

/// Maximum number of recent files kept.
pub const MAX_RECENT_FILES: usize = 10;

/// The recently imported files, from most recent to oldest.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RecentFiles {
    files: Vec<PathBuf>,
}

impl RecentFiles {
    /// Loads from the user config folder; empty list at the slightest
    /// problem.
    pub fn load() -> Self {
        recent_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    /// Writes to disk (creates the config folder if it does not exist).
    pub fn save(&self) -> std::io::Result<()> {
        let path = recent_path().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no usable config folder for this OS",
            )
        })?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// The files, from most recent to oldest.
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// Puts `paths` at the front (in the given order: the first becomes the
    /// most recent), without duplicates, bounded to [`MAX_RECENT_FILES`].
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

    /// The folder of the most recent file: where to reopen the picker.
    pub fn last_dir(&self) -> Option<&Path> {
        self.files.first().and_then(|file| file.parent())
    }
}

fn recent_path() -> Option<PathBuf> {
    crate::config_dir::config_dir().map(|dir| dir.join("import_recent.json"))
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
        let parsed: Option<RecentFiles> = serde_json::from_str("{ not json").ok();
        assert!(parsed.is_none());
        assert!(RecentFiles::default().last_dir().is_none());
    }
}
