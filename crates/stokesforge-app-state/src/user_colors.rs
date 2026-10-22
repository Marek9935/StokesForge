//! Couleurs enregistrées « pour l'utilisateur » : gardées sur cette machine,
//! proposées dans tous les projets.
//!
//! Le pendant hors projet des couleurs enregistrées dans le projet
//! ([`stokesforge_plot::SavedColors`], portées par l'arbre et le fichier `.stf`,
//! modifiées par une commande annulable). Celles-ci ne sont pas dans le
//! modèle — elles n'appartiennent à aucun projet —, ne passent donc par
//! aucune commande et ne s'annulent pas : c'est une préférence de
//! l'utilisateur, comme les recettes d'import
//! (`stokesforge_import::RecipeStore`), dont ce module reprend le stockage — un
//! fichier JSON dans le dossier de configuration de StokesForge, jamais de
//! panique s'il est absent, illisible ou corrompu (invariant 10).

use std::path::{Path, PathBuf};

use stokesforge_plot::SavedColors;

/// Forme du fichier sur disque.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct UserColorsFile {
    #[serde(default)]
    curves: Vec<String>,
    #[serde(default)]
    text: Vec<String>,
    #[serde(default)]
    backgrounds: Vec<String>,
    /// Nuanciers (#178) : `(nom, arrêts)`.
    #[serde(default)]
    gradients: Vec<(String, Vec<String>)>,
}

/// Les couleurs de l'utilisateur, et le fichier où elles vivent.
#[derive(Debug, Clone, Default)]
pub struct UserColors {
    colors: SavedColors,
    path: Option<PathBuf>,
}

impl UserColors {
    /// Charge depuis le fichier de configuration de l'utilisateur. Fichier
    /// absent, illisible ou corrompu : liste vide, pas d'erreur.
    pub fn load() -> Self {
        Self::load_from(default_path())
    }

    /// Charge depuis `path` (`None` : rien à charger, rien ne sera écrit).
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

    /// Les couleurs enregistrées.
    pub fn colors(&self) -> &SavedColors {
        &self.colors
    }

    /// Le fichier où elles sont écrites, s'il y en a un.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Remplace les couleurs et les écrit sur disque (crée le dossier de
    /// configuration au besoin). En cas d'échec d'écriture, les couleurs
    /// restent remplacées pour cette session et l'erreur est rendue.
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

/// Fichier `couleurs.json` du dossier de configuration
/// (`stokesforge_import::config_dir::config_dir`, #332) — le même dossier que
/// `import_recipes.json`.
fn default_path() -> Option<PathBuf> {
    stokesforge_import::config_dir::config_dir().map(|dir| dir.join("couleurs.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_plot::ColorUsage;

    #[test]
    fn les_couleurs_de_l_utilisateur_survivent_a_un_redemarrage() {
        let dir = tempfile::tempdir().expect("dossier temporaire");
        let path = dir.path().join("config").join("couleurs.json");
        let mut store = UserColors::load_from(Some(path.clone()));
        assert!(store.colors().is_empty());
        store
            .replace(SavedColors::default().with_added(ColorUsage::Text, "#123456"))
            .expect("écriture");
        let reloaded = UserColors::load_from(Some(path));
        assert_eq!(reloaded.colors().get(ColorUsage::Text), ["#123456"]);
    }

    #[test]
    fn un_fichier_corrompu_donne_une_liste_vide() {
        let dir = tempfile::tempdir().expect("dossier temporaire");
        let path = dir.path().join("couleurs.json");
        std::fs::write(&path, "{ pas du json").expect("écriture");
        assert!(UserColors::load_from(Some(path)).colors().is_empty());
    }
}
