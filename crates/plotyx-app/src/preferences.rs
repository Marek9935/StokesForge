//! Préférences de l'utilisateur, gardées sur cette machine : pour l'instant la
//! langue de l'interface (issue #41) et la disposition des barres d'outils
//! (issue #60).
//!
//! Même stockage que les couleurs de l'utilisateur ([`crate::user_colors`]) et
//! que les recettes d'import : un fichier JSON — `preferences.json` — dans le
//! dossier de configuration de PlotYx. Jamais de panique s'il est absent,
//! illisible ou corrompu (invariant 10) : on retombe sur les valeurs par défaut,
//! donc sur l'anglais.
//!
//! La langue se choisit dans le menu Fichier › Langue, qui l'écrit ici
//! ([`Preferences::save_language`]) ; le fichier se crée aussi à la main
//! (`{ "language": "fr" }`). La variable d'environnement `PLOTYX_LANG` l'emporte
//! au démarrage (voir [`crate::i18n::choose_language`]).

use std::path::{Path, PathBuf};

/// Forme du fichier sur disque. Un champ inconnu est ignoré, un champ absent
/// prend sa valeur par défaut : un fichier écrit par une version plus récente
/// reste lisible.
#[derive(Debug, Default, serde::Deserialize)]
struct PreferencesFile {
    #[serde(default)]
    language: Option<String>,
    /// Gardée brute : c'est `toolbar::ToolbarLayout::from_json` qui la lit, en
    /// tolérant ce qu'une autre version y a écrit.
    #[serde(default)]
    toolbars: Option<serde_json::Value>,
}

/// Les préférences de l'utilisateur, et le fichier d'où elles viennent.
#[derive(Debug, Clone, Default)]
pub struct Preferences {
    language: Option<String>,
    toolbars: Option<serde_json::Value>,
    path: Option<PathBuf>,
}

impl Preferences {
    /// Charge depuis le fichier de configuration de l'utilisateur. Fichier
    /// absent, illisible ou corrompu : préférences vides, pas d'erreur.
    pub fn load() -> Self {
        Self::load_from(default_path())
    }

    /// Charge depuis `path` (`None` : rien à charger).
    pub fn load_from(path: Option<PathBuf>) -> Self {
        let file = path
            .as_deref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|content| serde_json::from_str::<PreferencesFile>(&content).ok())
            .unwrap_or_default();
        Self {
            language: file.language,
            toolbars: file.toolbars,
            path,
        }
    }

    /// La langue enregistrée, telle qu'écrite (`"fr"`, `"en"`…), si le fichier
    /// en porte une. Sa validité se juge dans [`crate::i18n::Language::parse`].
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// Enregistre la langue choisie dans le fichier, en gardant les autres
    /// champs qu'il contient (écrits par une version plus récente). Crée le
    /// dossier au besoin. L'erreur d'écriture est rendue, jamais une panique :
    /// l'appelant la montre, et la langue reste appliquée pour la session.
    pub fn save_language(&mut self, tag: &str) -> std::io::Result<()> {
        self.language = Some(tag.to_string());
        self.save_field("language", serde_json::Value::from(tag))
    }

    /// La disposition des barres d'outils enregistrée, telle qu'écrite.
    pub fn toolbars(&self) -> Option<&serde_json::Value> {
        self.toolbars.as_ref()
    }

    /// Enregistre la disposition des barres d'outils (mêmes garanties que
    /// [`Preferences::save_language`]).
    pub fn save_toolbars(&mut self, layout: serde_json::Value) -> std::io::Result<()> {
        self.toolbars = Some(layout.clone());
        self.save_field("toolbars", layout)
    }

    fn save_field(&self, key: &str, value: serde_json::Value) -> std::io::Result<()> {
        let Some(path) = self.path.as_deref() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no configuration folder",
            ));
        };
        let mut document = std::fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
            .and_then(|value| match value {
                serde_json::Value::Object(map) => Some(map),
                _ => None,
            })
            .unwrap_or_default();
        document.insert(key.to_string(), value);
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let text = serde_json::to_string_pretty(&serde_json::Value::Object(document))
            .map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    /// Le fichier d'où viennent les préférences, s'il y en a un.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

/// `directories::ProjectDirs::from("", "", "PlotYx")`, dossier de
/// configuration, fichier `preferences.json` — le même dossier que
/// `couleurs.json` et `import_recipes.json`.
fn default_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "PlotYx")
        .map(|dirs| dirs.config_dir().join("preferences.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(content: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("dossier temporaire");
        let path = dir.path().join("preferences.json");
        std::fs::write(&path, content).expect("écriture");
        (dir, path)
    }

    #[test]
    fn la_langue_enregistree_est_relue() {
        let (_dir, path) = written(r#"{ "language": "fr" }"#);
        assert_eq!(Preferences::load_from(Some(path)).language(), Some("fr"));
    }

    #[test]
    fn la_disposition_des_barres_est_ecrite_sans_perdre_la_langue() {
        let (_dir, path) = written(r#"{ "language": "fr" }"#);
        let mut preferences = Preferences::load_from(Some(path.clone()));
        preferences
            .save_toolbars(serde_json::json!({ "bars": [] }))
            .expect("écriture");
        let relues = Preferences::load_from(Some(path));
        assert_eq!(relues.language(), Some("fr"));
        assert_eq!(relues.toolbars(), Some(&serde_json::json!({ "bars": [] })));
    }

    #[test]
    fn un_fichier_absent_ne_donne_aucune_preference() {
        let dir = tempfile::tempdir().expect("dossier temporaire");
        let path = dir.path().join("absent.json");
        assert_eq!(Preferences::load_from(Some(path)).language(), None);
        assert_eq!(Preferences::load_from(None).language(), None);
    }

    #[test]
    fn un_fichier_corrompu_ne_donne_aucune_preference() {
        for content in ["{ pas du json", "", "[1, 2]", r#"{ "language": 3 }"#] {
            let (_dir, path) = written(content);
            assert_eq!(
                Preferences::load_from(Some(path)).language(),
                None,
                "{content:?}"
            );
        }
    }

    #[test]
    fn la_langue_choisie_s_enregistre_et_se_relit() {
        let dir = tempfile::tempdir().expect("dossier temporaire");
        let path = dir.path().join("config").join("preferences.json");
        let mut preferences = Preferences::load_from(Some(path.clone()));
        preferences.save_language("fr").expect("écriture");
        assert_eq!(Preferences::load_from(Some(path)).language(), Some("fr"));
    }

    #[test]
    fn enregistrer_la_langue_garde_les_autres_champs() {
        let (_dir, path) = written(r#"{ "language": "en", "avenir": true }"#);
        let mut preferences = Preferences::load_from(Some(path.clone()));
        preferences.save_language("fr").expect("écriture");
        let content = std::fs::read_to_string(&path).expect("lecture");
        assert!(content.contains("\"avenir\""), "{content}");
        assert_eq!(Preferences::load_from(Some(path)).language(), Some("fr"));
    }

    #[test]
    fn enregistrer_sur_un_fichier_corrompu_le_remplace_sans_panique() {
        let (_dir, path) = written("{ pas du json");
        let mut preferences = Preferences::load_from(Some(path.clone()));
        preferences.save_language("en").expect("écriture");
        assert_eq!(Preferences::load_from(Some(path)).language(), Some("en"));
    }

    #[test]
    fn sans_dossier_de_configuration_l_enregistrement_echoue_sans_panique() {
        let mut preferences = Preferences::load_from(None);
        assert!(preferences.save_language("fr").is_err());
    }

    #[test]
    fn un_champ_inconnu_est_ignore() {
        let (_dir, path) = written(r#"{ "language": "en", "avenir": true }"#);
        assert_eq!(Preferences::load_from(Some(path)).language(), Some("en"));
    }
}
