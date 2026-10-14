//! Préférences de l'utilisateur, gardées sur cette machine : pour l'instant la
//! langue de l'interface (issue #41) et la disposition des barres d'outils
//! (issue #60), l’ouverture et les réglages de vue du gestionnaire d’objets
//! (#82, #139), et la taille et la position de chaque sorte de dialogue (F8,
//! #228).
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

use std::collections::BTreeMap;
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
    pub show_scales: bool,
    pub show_objects: bool,
    pub graph_open: bool,
    pub layers_open: bool,
    pub plots_open: bool,
    pub axes_open: bool,
    pub objects_open: bool,
}

impl Default for ObjectManagerSettings {
    fn default() -> Self {
        Self {
            show_scales: false,
            show_objects: true,
            graph_open: true,
            layers_open: true,
            plots_open: true,
            axes_open: false,
            objects_open: false,
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

/// Un format offert au presse-papiers par Copy Graph (#149).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CopyFormat {
    Svg,
    Png,
    Pdf,
}

impl CopyFormat {
    pub const ALL: [Self; 3] = [Self::Svg, Self::Png, Self::Pdf];
}

/// Une ligne de la liste des formats de copie : le format, offert ou non.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CopyFormatEntry {
    pub format: CopyFormat,
    pub on: bool,
}

/// L'ordre et l'état par défaut : les trois formats, offerts.
pub fn default_copy_formats() -> Vec<CopyFormatEntry> {
    CopyFormat::ALL
        .map(|format| CopyFormatEntry { format, on: true })
        .to_vec()
}

/// Remet une liste lue (ou saisie) en état : un format répété ne compte que la
/// première fois, un format absent est ajouté en fin, offert, et au moins un
/// format reste offert (copier sans rien offrir n'aurait pas de sens).
pub fn normalize_copy_formats(entries: &[CopyFormatEntry]) -> Vec<CopyFormatEntry> {
    let mut list: Vec<CopyFormatEntry> = Vec::new();
    for entry in entries {
        if list.iter().all(|kept| kept.format != entry.format) {
            list.push(*entry);
        }
    }
    for format in CopyFormat::ALL {
        if list.iter().all(|kept| kept.format != format) {
            list.push(CopyFormatEntry { format, on: true });
        }
    }
    if list.iter().all(|entry| !entry.on) {
        list[0].on = true;
    }
    list
}

/// Taille et position d'un dialogue modal (F8, #228), en points : coin haut
/// gauche et taille. Mémorisée à sa fermeture, reprise à sa prochaine
/// ouverture (ramenée dans l'écran par le dialogue).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DialogGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl DialogGeometry {
    /// Une géométrie utilisable : des nombres finis et une taille positive.
    pub fn is_valid(&self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
            && self.width > 0.0
            && self.height > 0.0
    }
}

/// Le nombre de sortes de dialogue dont la géométrie est gardée : au-delà, la
/// plus ancienne clé (dans l'ordre des clés) laisse sa place. Une sorte de
/// dialogue propre à un graphe (propriétés d'un objet) ne remplit donc pas le
/// fichier.
pub const MAX_DIALOG_GEOMETRIES: usize = 32;

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
    /// Sortie par défaut des analyses : `true` « Keep in Results », `false`
    /// « Show » (#200, maquette M5).
    #[serde(default)]
    analysis_keep_results: Option<bool>,
    /// Les dernières analyses lancées (#249). Gardée brute : une entrée
    /// illisible est ignorée sans effacer les autres.
    #[serde(default)]
    analysis_recent: Option<serde_json::Value>,
    /// Gardée brute : une entrée illisible n'efface pas les autres.
    #[serde(default)]
    dialogs: Option<serde_json::Value>,
    /// Les formats de Copy Graph, dans l'ordre offert (#148, #149). Gardée
    /// brute : une liste illisible retombe sur le défaut.
    #[serde(default)]
    copy_formats: Option<serde_json::Value>,
}

/// Les préférences de l'utilisateur, et le fichier d'où elles viennent.
#[derive(Debug, Clone)]
pub struct Preferences {
    language: Option<String>,
    object_manager_open: bool,
    object_manager_settings: ObjectManagerSettings,
    svg_text: SvgTextMode,
    toolbars: Option<serde_json::Value>,
    analysis_keep_results: bool,
    analysis_recent: Vec<crate::analysis_recent::RecentAnalysis>,
    dialogs: BTreeMap<String, DialogGeometry>,
    copy_formats: Vec<CopyFormatEntry>,
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
        // Les anciennes versions écrivent encore un champ historique.
        // Ils restent prioritaires et sont tenus à jour lors de chaque écriture.
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
            analysis_keep_results: file.analysis_keep_results.unwrap_or(false),
            analysis_recent: read_recent_analyses(file.analysis_recent),
            dialogs: read_dialogs(file.dialogs),
            copy_formats: file
                .copy_formats
                .and_then(|value| serde_json::from_value::<Vec<CopyFormatEntry>>(value).ok())
                .map_or_else(default_copy_formats, |list| normalize_copy_formats(&list)),
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

    /// La sortie par défaut des analyses : garder les valeurs dans Results
    /// (`true`) ou seulement les montrer (`false`, défaut).
    pub fn analysis_keep_results(&self) -> bool {
        self.analysis_keep_results
    }

    /// Mémorise le choix sans remplacer les autres préférences.
    pub fn save_analysis_keep_results(&mut self, keep: bool) -> std::io::Result<()> {
        self.analysis_keep_results = keep;
        self.save_field("analysis_keep_results", keep.into())
    }

    /// Les dernières analyses lancées, la plus récente en tête (#249).
    pub fn analysis_recent(&self) -> &[crate::analysis_recent::RecentAnalysis] {
        &self.analysis_recent
    }

    /// Mémorise la liste sans remplacer les autres préférences.
    pub fn save_analysis_recent(
        &mut self,
        list: &[crate::analysis_recent::RecentAnalysis],
    ) -> std::io::Result<()> {
        self.analysis_recent = list.to_vec();
        let value = serde_json::Value::Array(list.iter().map(|entry| entry.to_json()).collect());
        self.save_field("analysis_recent", value)
    }

    /// Les formats de Copy Graph, dans l'ordre où ils sont offerts.
    pub fn copy_formats(&self) -> &[CopyFormatEntry] {
        &self.copy_formats
    }

    /// Mémorise l'ordre et l'état des formats de copie (remis en état par
    /// [`normalize_copy_formats`]) sans remplacer les autres préférences.
    pub fn save_copy_formats(&mut self, entries: &[CopyFormatEntry]) -> std::io::Result<()> {
        self.copy_formats = normalize_copy_formats(entries);
        let value = serde_json::to_value(&self.copy_formats).map_err(std::io::Error::other)?;
        self.save_field("copy_formats", value)
    }

    /// La taille et la position mémorisées de chaque sorte de dialogue (F8),
    /// par clé (`app/dialogs.rs` la tire de l'identifiant du dialogue).
    pub fn dialog_geometries(&self) -> &BTreeMap<String, DialogGeometry> {
        &self.dialogs
    }

    /// Mémorise la géométrie d'une sorte de dialogue sans remplacer les autres
    /// préférences ; une géométrie invalide est ignorée. Au-delà de
    /// [`MAX_DIALOG_GEOMETRIES`] sortes, la première clé est oubliée.
    pub fn save_dialog_geometry(
        &mut self,
        kind: &str,
        geometry: DialogGeometry,
    ) -> std::io::Result<()> {
        if !geometry.is_valid() || self.dialogs.get(kind) == Some(&geometry) {
            return Ok(());
        }
        self.dialogs.insert(kind.to_string(), geometry);
        while self.dialogs.len() > MAX_DIALOG_GEOMETRIES {
            let Some(first) = self.dialogs.keys().find(|key| *key != kind).cloned() else {
                break;
            };
            self.dialogs.remove(&first);
        }
        let value = serde_json::to_value(&self.dialogs).map_err(std::io::Error::other)?;
        self.save_field("dialogs", value)
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

/// Les géométries de dialogue lues dans le fichier : les entrées illisibles ou
/// invalides sont écartées une à une.
fn read_dialogs(value: Option<serde_json::Value>) -> BTreeMap<String, DialogGeometry> {
    let Some(serde_json::Value::Object(map)) = value else {
        return BTreeMap::new();
    };
    map.into_iter()
        .filter_map(|(kind, value)| {
            serde_json::from_value::<DialogGeometry>(value)
                .ok()
                .filter(DialogGeometry::is_valid)
                .map(|geometry| (kind, geometry))
        })
        .take(MAX_DIALOG_GEOMETRIES)
        .collect()
}

/// Les analyses récentes lues dans le fichier : une entrée illisible est
/// écartée, la liste est plafonnée (#249).
fn read_recent_analyses(
    value: Option<serde_json::Value>,
) -> Vec<crate::analysis_recent::RecentAnalysis> {
    let Some(serde_json::Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(crate::analysis_recent::RecentAnalysis::from_json)
        .take(crate::analysis_recent::MAX_RECENT)
        .collect()
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

    #[test]
    fn la_sortie_par_defaut_des_analyses_est_ecrite_et_relue_sans_perdre_la_langue() {
        let (_dir, path) = written(r#"{ "language": "fr" }"#);
        let mut preferences = Preferences::load_from(Some(path.clone()));
        assert!(!preferences.analysis_keep_results(), "Show par défaut");
        preferences.save_analysis_keep_results(true).unwrap();
        let reloaded = Preferences::load_from(Some(path));
        assert!(reloaded.analysis_keep_results());
        assert_eq!(reloaded.language(), Some("fr"));
    }

    /// F8 (#228) : la géométrie d'un dialogue s'écrit sans perdre les autres
    /// préférences et se relit ; une entrée illisible du fichier n'emporte pas
    /// les autres, une géométrie invalide n'est pas écrite.
    #[test]
    fn la_geometrie_d_un_dialogue_est_ecrite_et_relue_sans_perdre_le_reste() {
        let (_dir, path) = written(
            r#"{ "language": "fr", "dialogs": { "abimee": { "x": "?" }, "nulle": { "x": 0, "y": 0, "width": 0, "height": 10 } } }"#,
        );
        let mut preferences = Preferences::load_from(Some(path.clone()));
        assert!(preferences.dialog_geometries().is_empty());
        let geometry = DialogGeometry {
            x: 10.0,
            y: 20.0,
            width: 400.0,
            height: 300.0,
        };
        preferences
            .save_dialog_geometry("proprietes", geometry)
            .unwrap();
        preferences
            .save_dialog_geometry(
                "invalide",
                DialogGeometry {
                    width: f32::NAN,
                    ..geometry
                },
            )
            .unwrap();
        let relues = Preferences::load_from(Some(path));
        assert_eq!(relues.language(), Some("fr"));
        assert_eq!(
            relues.dialog_geometries().get("proprietes"),
            Some(&geometry)
        );
        assert_eq!(relues.dialog_geometries().len(), 1);
    }

    /// Au-delà de [`MAX_DIALOG_GEOMETRIES`] sortes, la plus ancienne clé part ;
    /// la sorte qui vient d'être écrite reste.
    #[test]
    fn les_geometries_de_dialogue_sont_plafonnees() {
        let (_dir, path) = written("{}");
        let mut preferences = Preferences::load_from(Some(path));
        let geometry = DialogGeometry {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        for index in 0..MAX_DIALOG_GEOMETRIES + 3 {
            preferences
                .save_dialog_geometry(&format!("{index:03}"), geometry)
                .unwrap();
        }
        let kinds = preferences.dialog_geometries();
        assert_eq!(kinds.len(), MAX_DIALOG_GEOMETRIES);
        assert!(kinds.contains_key(&format!("{:03}", MAX_DIALOG_GEOMETRIES + 2)));
        assert!(!kinds.contains_key("000"));
    }

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
            show_scales: true,
            show_objects: false,
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
    fn echelles_cachees_par_defaut_et_bascule_memorisee() {
        assert!(!Preferences::load_from(None).object_manager_show_scales());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(&path, r#"{"language":"fr"}"#).unwrap();
        let mut preferences = Preferences::load_from(Some(path.clone()));
        assert!(!preferences.object_manager_show_scales());
        preferences.save_object_manager_show_scales(true).unwrap();
        let relues = Preferences::load_from(Some(path));
        assert!(relues.object_manager_show_scales());
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

    #[test]
    fn les_formats_de_copie_se_relisent_dans_leur_ordre() {
        let dossier = tempfile::tempdir().unwrap();
        let path = dossier.path().join("preferences.json");
        let mut prefs = Preferences::load_from(Some(path.clone()));
        assert_eq!(prefs.copy_formats(), default_copy_formats());
        let choisis = [
            CopyFormatEntry {
                format: CopyFormat::Pdf,
                on: true,
            },
            CopyFormatEntry {
                format: CopyFormat::Svg,
                on: false,
            },
            CopyFormatEntry {
                format: CopyFormat::Png,
                on: true,
            },
        ];
        prefs.save_copy_formats(&choisis).unwrap();
        assert_eq!(
            Preferences::load_from(Some(path)).copy_formats(),
            choisis.as_slice()
        );
    }

    #[test]
    fn une_liste_de_formats_abimee_est_remise_en_etat() {
        let png_seul = [CopyFormatEntry {
            format: CopyFormat::Png,
            on: false,
        }];
        let remise = normalize_copy_formats(&png_seul);
        assert_eq!(remise.len(), 3, "les formats absents reviennent");
        assert_eq!(remise[0].format, CopyFormat::Png);
        assert!(remise.iter().any(|e| e.on), "au moins un format offert");
        let doublon = [png_seul[0], png_seul[0]];
        assert_eq!(normalize_copy_formats(&doublon).len(), 3);

        let dossier = tempfile::tempdir().unwrap();
        let path = dossier.path().join("preferences.json");
        std::fs::write(&path, r#"{"copy_formats":"n'importe quoi"}"#).unwrap();
        assert_eq!(
            Preferences::load_from(Some(path)).copy_formats(),
            default_copy_formats()
        );
    }
}
