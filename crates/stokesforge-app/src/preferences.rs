//! Préférences de l'utilisateur, gardées sur cette machine : pour l'instant la
//! langue de l'interface (issue #41) et la disposition des barres d'outils
//! (issue #60), ainsi que l’ouverture et les réglages de vue du gestionnaire
//! d’objets (#82, #139).
//!
//! Même stockage que les couleurs de l'utilisateur ([`crate::user_colors`]) et
//! que les recettes d'import : un fichier JSON — `preferences.json` — dans le
//! dossier de configuration de StokesForge. Jamais de panique s'il est absent,
//! illisible ou corrompu (invariant 10) : on retombe sur les valeurs par défaut,
//! donc sur l'anglais et le gestionnaire d'objets ouvert.
//!
//! La langue se choisit dans le menu Fichier › Langue, qui l'écrit ici
//! ([`Preferences::save_language`]) ; le fichier se crée aussi à la main
//! (`{ "language": "fr" }`). La variable d'environnement `STOKESFORGE_LANG` l'emporte
//! au démarrage (voir [`crate::i18n::choose_language`]).

use std::path::{Path, PathBuf};
use stokesforge_render_svg::SvgTextMode;

/// Familles de groupes dont le repli initial est réglable (#139).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectManagerGroup {
    Graph,
    Layers,
    Plots,
    Axes,
    Objects,
}

impl ObjectManagerGroup {
    pub const ALL: [Self; 5] = [
        Self::Graph,
        Self::Layers,
        Self::Plots,
        Self::Axes,
        Self::Objects,
    ];
}

/// Table unique des réglages de vue. Les valeurs initiales ne remplacent
/// jamais l'état de repli déjà mémorisé par egui pour un groupe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ObjectManagerSettings {
    pub show_texts: bool,
    pub show_scales: bool,
    pub graph_open: bool,
    pub layers_open: bool,
    pub plots_open: bool,
    pub axes_open: bool,
    pub objects_open: bool,
}

impl Default for ObjectManagerSettings {
    fn default() -> Self {
        Self {
            show_texts: false,
            show_scales: false,
            graph_open: true,
            layers_open: true,
            plots_open: true,
            axes_open: false,
            objects_open: true,
        }
    }
}

impl ObjectManagerSettings {
    pub fn group_open(self, group: ObjectManagerGroup) -> bool {
        match group {
            ObjectManagerGroup::Graph => self.graph_open,
            ObjectManagerGroup::Layers => self.layers_open,
            ObjectManagerGroup::Plots => self.plots_open,
            ObjectManagerGroup::Axes => self.axes_open,
            ObjectManagerGroup::Objects => self.objects_open,
        }
    }

    pub fn set_group_open(&mut self, group: ObjectManagerGroup, open: bool) {
        *match group {
            ObjectManagerGroup::Graph => &mut self.graph_open,
            ObjectManagerGroup::Layers => &mut self.layers_open,
            ObjectManagerGroup::Plots => &mut self.plots_open,
            ObjectManagerGroup::Axes => &mut self.axes_open,
            ObjectManagerGroup::Objects => &mut self.objects_open,
        } = open;
    }
}

/// Forme du fichier sur disque. Un champ inconnu est ignoré, un champ absent
/// prend sa valeur par défaut : un fichier écrit par une version plus récente
/// reste lisible.
#[derive(Debug, Default, serde::Deserialize)]
struct PreferencesFile {
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    object_manager_open: Option<bool>,
    #[serde(default)]
    object_manager_show_texts: Option<bool>,
    #[serde(default)]
    object_manager_show_scales: Option<bool>,
    #[serde(default)]
    object_manager_settings: Option<ObjectManagerSettings>,
    /// `"text"` ou `"outlines"` (#162) ; toute autre valeur : le défaut.
    #[serde(default)]
    svg_text: Option<String>,
    /// Gardée brute : c'est `toolbar::ToolbarLayout::from_json` qui la lit, en
    /// tolérant ce qu'une autre version y a écrit.
    #[serde(default)]
    toolbars: Option<serde_json::Value>,
}

/// Les préférences de l'utilisateur, et le fichier d'où elles viennent.
#[derive(Debug, Clone)]
pub struct Preferences {
    language: Option<String>,
    object_manager_open: bool,
    object_manager_settings: ObjectManagerSettings,
    svg_text: SvgTextMode,
    toolbars: Option<serde_json::Value>,
    path: Option<PathBuf>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self::load_from(None)
    }
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
        let mut settings = file.object_manager_settings.unwrap_or_default();
        // Les anciennes versions écrivent encore les deux champs historiques.
        // Ils restent prioritaires et sont tenus à jour lors de chaque écriture.
        if let Some(show) = file.object_manager_show_texts {
            settings.show_texts = show;
        }
        if let Some(show) = file.object_manager_show_scales {
            settings.show_scales = show;
        }
        Self {
            language: file.language,
            object_manager_open: file.object_manager_open.unwrap_or(true),
            object_manager_settings: settings,
            svg_text: match file.svg_text.as_deref() {
                Some("outlines") => SvgTextMode::Outlines,
                _ => SvgTextMode::Text,
            },
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

    /// Ouverture mémorisée du panneau Object Manager ; ouvert par défaut.
    pub fn object_manager_open(&self) -> bool {
        self.object_manager_open
    }

    /// Mémorise l'ouverture sans remplacer les autres préférences.
    pub fn save_object_manager_open(&mut self, open: bool) -> std::io::Result<()> {
        self.object_manager_open = open;
        self.save_field("object_manager_open", open.into())
    }

    /// Textes automatiques (titres, légende) listés dans l'Object Manager ;
    /// cachés par défaut (#102).
    pub fn object_manager_show_texts(&self) -> bool {
        self.object_manager_settings.show_texts
    }

    /// Mémorise la bascule sans remplacer les autres préférences.
    pub fn save_object_manager_show_texts(&mut self, show: bool) -> std::io::Result<()> {
        self.object_manager_settings.show_texts = show;
        self.save_field("object_manager_show_texts", show.into())
    }

    /// Échelles (axes) listées dans l'Object Manager ; cachées par défaut (#111).
    pub fn object_manager_show_scales(&self) -> bool {
        self.object_manager_settings.show_scales
    }

    /// Mémorise la bascule sans remplacer les autres préférences.
    pub fn save_object_manager_show_scales(&mut self, show: bool) -> std::io::Result<()> {
        self.object_manager_settings.show_scales = show;
        self.save_field("object_manager_show_scales", show.into())
    }

    pub fn object_manager_settings(&self) -> ObjectManagerSettings {
        self.object_manager_settings
    }

    /// Comment un export SVG écrit le texte composé par Typst (#162) ; le
    /// texte SVG par défaut. Retenu d'un export à l'autre, jamais dans le
    /// `.stf`.
    pub fn svg_text_mode(&self) -> SvgTextMode {
        self.svg_text
    }

    /// Mémorise le mode sans remplacer les autres préférences.
    pub fn save_svg_text_mode(&mut self, mode: SvgTextMode) -> std::io::Result<()> {
        self.svg_text = mode;
        let name = match mode {
            SvgTextMode::Text => "text",
            SvgTextMode::Outlines => "outlines",
        };
        self.save_field("svg_text", name.into())
    }

    /// Écrit les réglages ensemble en conservant les préférences étrangères.
    pub fn save_object_manager_settings(
        &mut self,
        settings: ObjectManagerSettings,
    ) -> std::io::Result<()> {
        self.object_manager_settings = settings;
        self.save_fields([
            (
                "object_manager_settings",
                serde_json::to_value(settings).map_err(std::io::Error::other)?,
            ),
            ("object_manager_show_texts", settings.show_texts.into()),
            ("object_manager_show_scales", settings.show_scales.into()),
        ])
    }

    fn save_field(&self, key: &str, value: serde_json::Value) -> std::io::Result<()> {
        self.save_fields([(key, value)])
    }

    fn save_fields<'a>(
        &self,
        fields: impl IntoIterator<Item = (&'a str, serde_json::Value)>,
    ) -> std::io::Result<()> {
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
        for (key, value) in fields {
            document.insert(key.to_string(), value);
        }
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

/// `directories::ProjectDirs::from("", "", "StokesForge")`, dossier de
/// configuration, fichier `preferences.json` — le même dossier que
/// `couleurs.json` et `import_recipes.json`.
fn default_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "StokesForge")
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
        let preferences = Preferences::load_from(Some(path));
        assert_eq!(preferences.language(), None);
        assert!(preferences.object_manager_open());
        assert_eq!(Preferences::load_from(None).language(), None);
        assert!(Preferences::load_from(None).object_manager_open());
        assert!(Preferences::default().object_manager_open());
    }

    #[test]
    fn un_fichier_corrompu_ne_donne_aucune_preference() {
        for content in ["{ pas du json", "", "[1, 2]", r#"{ "language": 3 }"#] {
            let (_dir, path) = written(content);
            assert_eq!(
                Preferences::load_from(Some(path.clone())).language(),
                None,
                "{content:?}"
            );
            assert!(Preferences::load_from(Some(path)).object_manager_open());
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

#[cfg(test)]
mod object_manager_tests {
    use super::*;

    #[test]
    fn reglages_initiaux_tolerent_les_fichiers_absents_incomplets_et_anciens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        let defaults = ObjectManagerSettings::default();
        assert_eq!(
            Preferences::load_from(Some(path.clone())).object_manager_settings(),
            defaults
        );
        for content in ["{corrompu", "{}", r#"{"object_manager_settings":{}}"#] {
            std::fs::write(&path, content).unwrap();
            assert_eq!(
                Preferences::load_from(Some(path.clone())).object_manager_settings(),
                defaults
            );
        }
        std::fs::write(&path, r#"{"object_manager_settings":{"axes_open":true},"object_manager_show_texts":true,"object_manager_show_scales":true}"#).unwrap();
        let settings = Preferences::load_from(Some(path)).object_manager_settings();
        assert_eq!(
            settings,
            ObjectManagerSettings {
                show_texts: true,
                show_scales: true,
                axes_open: true,
                ..defaults
            }
        );
    }

    #[test]
    fn reglages_et_reinitialisation_preservent_les_preferences_etrangeres() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(
            &path,
            r#"{"language":"fr","object_manager_open":false,"toolbars":{"bars":[]},"avenir":42}"#,
        )
        .unwrap();
        let mut preferences = Preferences::load_from(Some(path.clone()));
        let defaults = ObjectManagerSettings::default();
        let settings = ObjectManagerSettings {
            show_texts: true,
            show_scales: true,
            graph_open: false,
            layers_open: false,
            plots_open: false,
            axes_open: true,
            objects_open: false,
        };
        for expected in [settings, defaults] {
            preferences.save_object_manager_settings(expected).unwrap();
            let loaded = Preferences::load_from(Some(path.clone()));
            assert_eq!(loaded.object_manager_settings(), expected);
            assert_eq!(loaded.language(), Some("fr"));
            assert!(!loaded.object_manager_open());
            assert_eq!(loaded.toolbars(), Some(&serde_json::json!({"bars":[]})));
            let document: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(document["avenir"], 42);
            assert_eq!(document["object_manager_show_texts"], expected.show_texts);
            assert_eq!(document["object_manager_show_scales"], expected.show_scales);
        }
    }
    #[test]
    fn ouverture_memorisee_sans_perdre_langue_barres_et_champs_inconnus() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(
            &path,
            r#"{"language":"fr","toolbars":{"bars":[]},"avenir":42}"#,
        )
        .unwrap();
        let mut preferences = Preferences::load_from(Some(path.clone()));
        assert!(preferences.object_manager_open());
        preferences.save_object_manager_open(true).unwrap();
        preferences.save_language("en").unwrap();
        let relues = Preferences::load_from(Some(path.clone()));
        assert!(relues.object_manager_open());
        assert_eq!(relues.language(), Some("en"));
        assert_eq!(relues.toolbars(), Some(&serde_json::json!({"bars":[]})));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&path).unwrap())
                .unwrap()["avenir"],
            42
        );
        preferences.save_object_manager_open(false).unwrap();
        assert!(!Preferences::load_from(Some(path)).object_manager_open());
    }

    #[test]
    fn textes_automatiques_caches_par_defaut_et_bascule_memorisee() {
        // Absent, corrompu ou sans le champ : cachés.
        assert!(!Preferences::load_from(None).object_manager_show_texts());
        for content in ["{ pas du json", r#"{"language":"fr"}"#] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("preferences.json");
            std::fs::write(&path, content).unwrap();
            assert!(!Preferences::load_from(Some(path)).object_manager_show_texts());
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(&path, r#"{"language":"fr","avenir":42}"#).unwrap();
        let mut preferences = Preferences::load_from(Some(path.clone()));
        preferences.save_object_manager_show_texts(true).unwrap();
        let relues = Preferences::load_from(Some(path.clone()));
        assert!(relues.object_manager_show_texts());
        assert_eq!(relues.language(), Some("fr"));
        assert!(std::fs::read_to_string(&path).unwrap().contains("avenir"));
        // L'ouverture du panneau reste indépendante.
        assert!(relues.object_manager_open());
    }

    #[test]
    fn echelles_cachees_par_defaut_et_bascule_memorisee() {
        assert!(!Preferences::load_from(None).object_manager_show_scales());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(&path, r#"{"object_manager_show_texts":true}"#).unwrap();
        let mut preferences = Preferences::load_from(Some(path.clone()));
        assert!(!preferences.object_manager_show_scales());
        preferences.save_object_manager_show_scales(true).unwrap();
        let relues = Preferences::load_from(Some(path));
        assert!(relues.object_manager_show_scales());
        // Indépendante de la bascule des textes.
        assert!(relues.object_manager_show_texts());
    }

    /// #162 : texte SVG par défaut ; le choix des contours est retenu d'une
    /// session à l'autre, sans écraser les autres préférences.
    #[test]
    fn le_mode_du_texte_svg_se_retient_et_vaut_texte_par_defaut() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(&path, r#"{"language":"fr","svg_text":"n'importe quoi"}"#).unwrap();
        let mut prefs = Preferences::load_from(Some(path.clone()));
        assert_eq!(prefs.svg_text_mode(), SvgTextMode::Text);
        prefs.save_svg_text_mode(SvgTextMode::Outlines).unwrap();
        let relues = Preferences::load_from(Some(path.clone()));
        assert_eq!(relues.svg_text_mode(), SvgTextMode::Outlines);
        assert_eq!(relues.language(), Some("fr"));
        prefs.save_svg_text_mode(SvgTextMode::Text).unwrap();
        assert_eq!(
            Preferences::load_from(Some(path)).svg_text_mode(),
            SvgTextMode::Text
        );
    }
}
