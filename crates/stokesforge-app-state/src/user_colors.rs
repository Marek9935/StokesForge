//! Colors saved "for the user": kept on this machine, offered in all
//! projects.
//!
//! The out-of-project counterpart of the colors saved in the project
//! ([`stokesforge_plot::SavedColors`], carried by the tree and the `.stf`
//! file, changed by an undoable command). These are not in the model — they
//! belong to no project —, so they go through no command and cannot be undone:
//! they are a user preference, like the import recipes
//! (`stokesforge_import::RecipeStore`), whose storage this module reuses — a
//! JSON file in StokesForge's configuration folder, never a panic if it is
//! missing, unreadable or corrupt (invariant 10).

use std::path::{Path, PathBuf};

use stokesforge_plot::SavedColors;

/// Shape of the file on disk.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct UserColorsFile {
    #[serde(default)]
    curves: Vec<String>,
    #[serde(default)]
    text: Vec<String>,
    #[serde(default)]
    backgrounds: Vec<String>,
    /// Color gradients (#178): `(name, stops)`.
    #[serde(default)]
    gradients: Vec<(String, Vec<String>)>,
}

/// The user's colors, and the file where they live.
#[derive(Debug, Clone, Default)]
pub struct UserColors {
    colors: SavedColors,
    path: Option<PathBuf>,
}

impl UserColors {
    /// Loads from the user's configuration file. Missing, unreadable or
    /// corrupt file: empty list, no error.
    pub fn load() -> Self {
        Self::load_from(default_path())
    }

    /// Loads from `path` (`None`: nothing to load, nothing will be written).
    pub fn load_from(path: Option<PathBuf>) -> Self {
        let colors = path
            .as_deref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|content| serde_json::from_str::<UserColorsFile>(&content).ok())
            .map(|file| {
                SavedColors {
                    curves: file.curves,
                    text: file.text,
                    backgrounds: file.backgrounds,
                    gradients: file
                        .gradients
                        .into_iter()
                        .map(|(name, stops)| stokesforge_plot::SavedGradient { name, stops })
                        .collect(),
                }
                .bounded()
            })
            .unwrap_or_default();
        Self { colors, path }
    }

    /// The saved colors.
    pub fn colors(&self) -> &SavedColors {
        &self.colors
    }

    /// The file where they are written, if there is one.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Replaces the colors and writes them to disk (creates the configuration
    /// folder if needed). If the write fails, the colors stay replaced for
    /// this session and the error is returned.
    pub fn replace(&mut self, colors: SavedColors) -> std::io::Result<()> {
        self.colors = colors.bounded();
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = UserColorsFile {
            curves: self.colors.curves.clone(),
            text: self.colors.text.clone(),
            backgrounds: self.colors.backgrounds.clone(),
            gradients: self
                .colors
                .gradients
                .iter()
                .map(|gradient| (gradient.name.clone(), gradient.stops.clone()))
                .collect(),
        };
        let json = serde_json::to_string_pretty(&file)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        std::fs::write(path, json)
    }
}

/// The `couleurs.json` file of the configuration folder
/// (`stokesforge_import::config_dir::config_dir`, #332) — the same folder as
/// `import_recipes.json`.
fn default_path() -> Option<PathBuf> {
    stokesforge_import::config_dir::config_dir().map(|dir| dir.join("couleurs.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_plot::ColorUsage;

    #[test]
    fn the_users_colors_survive_a_restart() {
        let dir = tempfile::tempdir().expect("temporary folder");
        let path = dir.path().join("config").join("couleurs.json");
        let mut store = UserColors::load_from(Some(path.clone()));
        assert!(store.colors().is_empty());
        store
            .replace(SavedColors::default().with_added(ColorUsage::Text, "#123456"))
            .expect("write");
        let reloaded = UserColors::load_from(Some(path));
        assert_eq!(reloaded.colors().get(ColorUsage::Text), ["#123456"]);
    }

    #[test]
    fn a_corrupt_file_gives_an_empty_list() {
        let dir = tempfile::tempdir().expect("temporary folder");
        let path = dir.path().join("couleurs.json");
        std::fs::write(&path, "{ not json").expect("write");
        assert!(UserColors::load_from(Some(path)).colors().is_empty());
    }
}
