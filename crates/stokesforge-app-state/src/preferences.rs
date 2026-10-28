//! User preferences, kept on this machine: for now the interface language
//! (issue #41) and the toolbar layout (issue #60), the open state and view
//! settings of the Object Manager (#82, #139), and the size and position of
//! each kind of dialog (S-F8, #228).
//!
//! Same storage as the user's colors ([`crate::user_colors`]) and the import
//! templates: a JSON file — `preferences.json` — in the StokesForge
//! configuration folder. Never a panic if it is missing, unreadable or
//! corrupted (invariant 10): it falls back to the defaults, hence English and
//! the Object Manager open.
//!
//! The language is chosen in the File › Language menu, which writes it here
//! ([`Preferences::save_language`]); the file can also be created by hand
//! (`{ "language": "fr" }`). The `STOKESFORGE_LANG` environment variable wins at
//! startup (see [`crate::i18n::choose_language`]).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use stokesforge_render_svg::SvgTextMode;

/// Groups families whose initial collapsed state can be set (#139).
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

/// Single table of view settings. The initial values never replace the collapsed
/// state egui has already remembered for a group.
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

/// A format offered to the clipboard by Copy Graph (#149).
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

/// A row of the copy formats list: the format, offered or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CopyFormatEntry {
    pub format: CopyFormat,
    pub on: bool,
}

/// The default order and state: the three formats, offered.
pub fn default_copy_formats() -> Vec<CopyFormatEntry> {
    CopyFormat::ALL
        .map(|format| CopyFormatEntry { format, on: true })
        .to_vec()
}

/// Puts a list that was read (or typed) back in order: a repeated format counts
/// only the first time, a missing format is added at the end, offered, and at
/// least one format stays offered (copying while offering nothing would make no
/// sense).
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

/// Size and position of a modal dialog (S-F8, #228), in points: top-left corner
/// and size. Remembered when it closes, restored the next time it opens (brought
/// back onto the screen by the dialog).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DialogGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl DialogGeometry {
    /// A usable geometry: finite numbers and a positive size.
    pub fn is_valid(&self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
            && self.width > 0.0
            && self.height > 0.0
    }
}

/// The number of dialog kinds whose geometry is kept: beyond it, the oldest key
/// (in key order) gives way. A graph-specific kind of dialog (object properties)
/// therefore does not fill the file.
pub const MAX_DIALOG_GEOMETRIES: usize = 32;

/// Size cap of a `.stf` on open (A2, #316), in MB: default value (512, that of
/// `stokesforge_persistence::DEFAULT_MAX_FILE_BYTES`) and bounds of the setting.
/// No "no limit" option: the cap protects against opening a file received from a
/// third party.
pub const DEFAULT_MAX_FILE_MB: u32 = (stokesforge_persistence::DEFAULT_MAX_FILE_BYTES >> 20) as u32;
pub const DEFAULT_MAX_FILE_BYTES: u64 = stokesforge_persistence::DEFAULT_MAX_FILE_BYTES;
pub const MIN_MAX_FILE_MB: u32 = 16;
pub const MAX_MAX_FILE_MB: u32 = 16384;

/// Global cell budget on open (B1, #316), in millions: default (1000 = one
/// billion, `stokesforge_persistence::DEFAULT_MAX_CELLS`) and bounds.
pub const DEFAULT_MAX_CELLS_M: u32 =
    (stokesforge_persistence::DEFAULT_MAX_CELLS / 1_000_000) as u32;
pub const MIN_MAX_CELLS_M: u32 = 1;
pub const MAX_MAX_CELLS_M: u32 = 10_000;

pub use stokesforge_persistence::LoadLimits;

/// The default open caps (CLI, scripts, tests).
pub fn default_load_limits() -> LoadLimits {
    stokesforge_persistence::LoadLimits::default()
}

/// Autosave (#317): default period and bounds, in minutes.
pub const DEFAULT_AUTOSAVE_MINUTES: u32 = 5;
pub const MIN_AUTOSAVE_MINUTES: u32 = 1;
pub const MAX_AUTOSAVE_MINUTES: u32 = 60;

/// Shape of the file on disk. An unknown field is ignored, a missing field takes
/// its default value: a file written by a newer version stays readable.
#[derive(Debug, Default, serde::Deserialize)]
struct PreferencesFile {
    #[serde(default, deserialize_with = "lenient")]
    language: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    object_manager_open: Option<bool>,
    #[serde(default, deserialize_with = "lenient")]
    object_manager_show_scales: Option<bool>,
    #[serde(default, deserialize_with = "lenient")]
    object_manager_settings: Option<ObjectManagerSettings>,
    /// `"text"` or `"outlines"` (#162); any other value: the default.
    #[serde(default, deserialize_with = "lenient")]
    svg_text: Option<String>,
    /// Size cap of a `.stf` on open, in MB (A2, #316). Out of bounds: brought back
    /// within the bounds; missing or unreadable: the default.
    #[serde(default, deserialize_with = "lenient")]
    max_file_mb: Option<u32>,
    /// Global cell budget on open, in millions (B1, #316); same rules as
    /// `max_file_mb`.
    #[serde(default, deserialize_with = "lenient")]
    max_cells_m: Option<u32>,
    /// Autosave (#317); missing or unreadable: on.
    #[serde(default, deserialize_with = "lenient")]
    autosave: Option<bool>,
    /// Autosave period, in minutes; out of bounds: brought back within the bounds;
    /// missing or unreadable: 5.
    #[serde(default, deserialize_with = "lenient")]
    autosave_minutes: Option<u32>,
    /// Kept raw: `toolbar::ToolbarLayout::from_json` reads it, tolerating what another
    /// version wrote there.
    #[serde(default)]
    toolbars: Option<serde_json::Value>,
    /// Default output of the analyses: `true` "Keep in Results", `false` "Show"
    /// (#200, mockup M5).
    #[serde(default, deserialize_with = "lenient")]
    analysis_keep_results: Option<bool>,
    /// Dialog box when the R² of a computation is very low (#288); missing: on.
    #[serde(default, deserialize_with = "lenient")]
    analysis_low_r2_dialog: Option<bool>,
    /// Header rows of **new** workbooks: Metadata (shown by default) and x' (hidden
    /// by default).
    #[serde(default, deserialize_with = "lenient")]
    workbook_show_metadata: Option<bool>,
    #[serde(default, deserialize_with = "lenient")]
    workbook_show_x_prime: Option<bool>,
    /// The most recently run analyses (#249). Kept raw: an unreadable entry is
    /// ignored without erasing the others.
    #[serde(default)]
    analysis_recent: Option<serde_json::Value>,
    /// Kept raw: an unreadable entry does not erase the others.
    #[serde(default)]
    dialogs: Option<serde_json::Value>,
    /// The Copy Graph formats, in the order offered (#148, #149). Kept raw: an
    /// unreadable list falls back to the default.
    #[serde(default)]
    copy_formats: Option<serde_json::Value>,
    /// The demo has already been offered at the first launch; missing or unreadable:
    /// not yet (#333).
    #[serde(default, deserialize_with = "lenient")]
    first_run_done: Option<bool>,
}

/// Name of the demo file in the configuration folder (#333).
pub const DEMO_FILE_NAME: &str = "demo.stf";

fn clamp_autosave_minutes(minutes: u32) -> u32 {
    minutes.clamp(MIN_AUTOSAVE_MINUTES, MAX_AUTOSAVE_MINUTES)
}

fn clamp_max_cells_m(millions: u32) -> u32 {
    millions.clamp(MIN_MAX_CELLS_M, MAX_MAX_CELLS_M)
}

fn clamp_max_file_mb(megabytes: u32) -> u32 {
    megabytes.clamp(MIN_MAX_FILE_MB, MAX_MAX_FILE_MB)
}

/// Reads a field tolerating a value of the wrong type: it falls back to its
/// default (`None`) without invalidating the other fields of the file (#297).
fn lenient<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let value = <serde_json::Value as serde::Deserialize>::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

/// The user's preferences, and the file they come from.
#[derive(Debug, Clone)]
pub struct Preferences {
    language: Option<String>,
    object_manager_open: bool,
    object_manager_settings: ObjectManagerSettings,
    svg_text: SvgTextMode,
    max_file_mb: u32,
    max_cells_m: u32,
    autosave: bool,
    autosave_minutes: u32,
    toolbars: Option<serde_json::Value>,
    analysis_keep_results: bool,
    analysis_low_r2_dialog: bool,
    workbook_show_metadata: bool,
    workbook_show_x_prime: bool,
    analysis_recent: Vec<crate::analysis_recent::RecentAnalysis>,
    dialogs: BTreeMap<String, DialogGeometry>,
    copy_formats: Vec<CopyFormatEntry>,
    first_run_done: bool,
    path: Option<PathBuf>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self::load_from(None)
    }
}

impl Preferences {
    /// Loads from the user's configuration file. File missing, unreadable or
    /// corrupted: empty preferences, no error.
    pub fn load() -> Self {
        Self::load_from(default_path())
    }

    /// Loads from `path` (`None`: nothing to load).
    pub fn load_from(path: Option<PathBuf>) -> Self {
        let file = path
            .as_deref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|content| serde_json::from_str::<PreferencesFile>(&content).ok())
            .unwrap_or_default();
        let mut settings = file.object_manager_settings.unwrap_or_default();
        // Older versions still write a legacy field.
        // They stay prioritized and are kept up to date on every write.
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
            max_file_mb: file
                .max_file_mb
                .map_or(DEFAULT_MAX_FILE_MB, clamp_max_file_mb),
            max_cells_m: file
                .max_cells_m
                .map_or(DEFAULT_MAX_CELLS_M, clamp_max_cells_m),
            autosave: file.autosave.unwrap_or(true),
            autosave_minutes: file
                .autosave_minutes
                .map_or(DEFAULT_AUTOSAVE_MINUTES, clamp_autosave_minutes),
            toolbars: file.toolbars,
            analysis_keep_results: file.analysis_keep_results.unwrap_or(false),
            analysis_low_r2_dialog: file.analysis_low_r2_dialog.unwrap_or(true),
            workbook_show_metadata: file.workbook_show_metadata.unwrap_or(true),
            workbook_show_x_prime: file.workbook_show_x_prime.unwrap_or(false),
            analysis_recent: read_recent_analyses(file.analysis_recent),
            dialogs: read_dialogs(file.dialogs),
            copy_formats: file
                .copy_formats
                .and_then(|value| serde_json::from_value::<Vec<CopyFormatEntry>>(value).ok())
                .map_or_else(default_copy_formats, |list| normalize_copy_formats(&list)),
            first_run_done: file.first_run_done.unwrap_or(false),
            path,
        }
    }

    /// The saved language, as written (`"fr"`, `"en"`...), if the file carries one.
    /// Its validity is judged in [`crate::i18n::Language::parse`].
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// Saves the chosen language in the file, keeping the other fields it contains
    /// (written by a newer version). Creates the folder if needed. The write error
    /// is returned, never a panic: the caller shows it, and the language stays
    /// applied for the session.
    pub fn save_language(&mut self, tag: &str) -> std::io::Result<()> {
        self.language = Some(tag.to_string());
        self.save_field("language", serde_json::Value::from(tag))
    }

    /// The saved toolbar layout, as written.
    pub fn toolbars(&self) -> Option<&serde_json::Value> {
        self.toolbars.as_ref()
    }

    /// Saves the toolbar layout (same guarantees as
    /// [`Preferences::save_language`]).
    pub fn save_toolbars(&mut self, layout: serde_json::Value) -> std::io::Result<()> {
        self.toolbars = Some(layout.clone());
        self.save_field("toolbars", layout)
    }

    /// Remembered open state of the Object Manager panel; open by default.
    pub fn object_manager_open(&self) -> bool {
        self.object_manager_open
    }

    /// Remembers the open state without replacing the other preferences.
    pub fn save_object_manager_open(&mut self, open: bool) -> std::io::Result<()> {
        self.object_manager_open = open;
        self.save_field("object_manager_open", open.into())
    }

    /// Scales (axes) listed in the Object Manager; hidden by default (#111).
    pub fn object_manager_show_scales(&self) -> bool {
        self.object_manager_settings.show_scales
    }

    /// Remembers the toggle without replacing the other preferences.
    pub fn save_object_manager_show_scales(&mut self, show: bool) -> std::io::Result<()> {
        self.object_manager_settings.show_scales = show;
        self.save_field("object_manager_show_scales", show.into())
    }

    pub fn object_manager_settings(&self) -> ObjectManagerSettings {
        self.object_manager_settings
    }

    /// How an SVG export writes the text composed by Typst (#162); SVG text by
    /// default. Kept from one export to the next, never in the `.stf`.
    pub fn svg_text_mode(&self) -> SvgTextMode {
        self.svg_text
    }

    /// Remembers the mode without replacing the other preferences.
    pub fn save_svg_text_mode(&mut self, mode: SvgTextMode) -> std::io::Result<()> {
        self.svg_text = mode;
        let name = match mode {
            SvgTextMode::Text => "text",
            SvgTextMode::Outlines => "outlines",
        };
        self.save_field("svg_text", name.into())
    }

    /// Size cap of a `.stf` on open, in MB (A2, #316); 512 by default, always within
    /// `[MIN_MAX_FILE_MB, MAX_MAX_FILE_MB]`.
    pub fn max_file_mb(&self) -> u32 {
        self.max_file_mb
    }

    /// The same cap in bytes, as passed by the session and the persistence.
    pub fn max_file_bytes(&self) -> u64 {
        u64::from(self.max_file_mb) << 20
    }

    /// Remembers the cap (brought back within the bounds) without replacing the
    /// other preferences.
    pub fn save_max_file_mb(&mut self, megabytes: u32) -> std::io::Result<()> {
        self.max_file_mb = clamp_max_file_mb(megabytes);
        self.save_field("max_file_mb", self.max_file_mb.into())
    }

    /// Global cell budget on open, in millions (B1, #316).
    pub fn max_cells_m(&self) -> u32 {
        self.max_cells_m
    }

    /// The two open caps, as the session passes them.
    pub fn load_limits(&self) -> stokesforge_persistence::LoadLimits {
        stokesforge_persistence::LoadLimits {
            max_bytes: self.max_file_bytes(),
            max_cells: u64::from(self.max_cells_m) * 1_000_000,
        }
    }

    /// Remembers the budget (brought back within the bounds).
    pub fn save_max_cells_m(&mut self, millions: u32) -> std::io::Result<()> {
        self.max_cells_m = clamp_max_cells_m(millions);
        self.save_field("max_cells_m", self.max_cells_m.into())
    }

    /// Is autosave on? Yes by default (#317).
    pub fn autosave(&self) -> bool {
        self.autosave
    }

    /// Remembers the on/off state without replacing the other preferences.
    pub fn save_autosave(&mut self, on: bool) -> std::io::Result<()> {
        self.autosave = on;
        self.save_field("autosave", on.into())
    }

    /// Autosave period, in minutes, always within
    /// `[MIN_AUTOSAVE_MINUTES, MAX_AUTOSAVE_MINUTES]`; 5 by default.
    pub fn autosave_minutes(&self) -> u32 {
        self.autosave_minutes
    }

    /// Remembers the period (brought back within the bounds).
    pub fn save_autosave_minutes(&mut self, minutes: u32) -> std::io::Result<()> {
        self.autosave_minutes = clamp_autosave_minutes(minutes);
        self.save_field("autosave_minutes", self.autosave_minutes.into())
    }

    /// The default output of the analyses: keep the values in Results (`true`) or
    /// only show them (`false`, default).
    pub fn analysis_keep_results(&self) -> bool {
        self.analysis_keep_results
    }

    /// Remembers the choice without replacing the other preferences.
    pub fn save_analysis_keep_results(&mut self, keep: bool) -> std::io::Result<()> {
        self.analysis_keep_results = keep;
        self.save_field("analysis_keep_results", keep.into())
    }

    /// Is the "very low R²" dialog box allowed (#288)? On by default; the setting
    /// of each computation turns it off too (OR).
    pub fn analysis_low_r2_dialog(&self) -> bool {
        self.analysis_low_r2_dialog
    }

    /// Remembers the choice without replacing the other preferences.
    pub fn save_analysis_low_r2_dialog(&mut self, on: bool) -> std::io::Result<()> {
        self.analysis_low_r2_dialog = on;
        self.save_field("analysis_low_r2_dialog", on.into())
    }

    /// The header rows a workbook shows when it opens: `(Metadata, x')`,
    /// `(true, false)` by default.
    pub fn workbook_header_rows(&self) -> (bool, bool) {
        (self.workbook_show_metadata, self.workbook_show_x_prime)
    }

    /// Remembers the rows of new workbooks without replacing the other
    /// preferences.
    pub fn save_workbook_header_rows(
        &mut self,
        metadata: bool,
        x_prime: bool,
    ) -> std::io::Result<()> {
        self.workbook_show_metadata = metadata;
        self.workbook_show_x_prime = x_prime;
        self.save_field("workbook_show_metadata", metadata.into())?;
        self.save_field("workbook_show_x_prime", x_prime.into())
    }

    /// The most recently run analyses, the latest first (#249).
    pub fn analysis_recent(&self) -> &[crate::analysis_recent::RecentAnalysis] {
        &self.analysis_recent
    }

    /// Remembers the list without replacing the other preferences.
    pub fn save_analysis_recent(
        &mut self,
        list: &[crate::analysis_recent::RecentAnalysis],
    ) -> std::io::Result<()> {
        self.analysis_recent = list.to_vec();
        let value = serde_json::Value::Array(list.iter().map(|entry| entry.to_json()).collect());
        self.save_field("analysis_recent", value)
    }

    /// The Copy Graph formats, in the order they are offered.
    pub fn copy_formats(&self) -> &[CopyFormatEntry] {
        &self.copy_formats
    }

    /// Remembers the order and state of the copy formats (put back in order by
    /// [`normalize_copy_formats`]) without replacing the other preferences.
    pub fn save_copy_formats(&mut self, entries: &[CopyFormatEntry]) -> std::io::Result<()> {
        self.copy_formats = normalize_copy_formats(entries);
        let value = serde_json::to_value(&self.copy_formats).map_err(std::io::Error::other)?;
        self.save_field("copy_formats", value)
    }

    /// The remembered size and position of each kind of dialog (S-F8), by key
    /// (`app/dialogs.rs` derives it from the dialog's identifier).
    pub fn dialog_geometries(&self) -> &BTreeMap<String, DialogGeometry> {
        &self.dialogs
    }

    /// Remembers the geometry of a kind of dialog without replacing the other
    /// preferences; an invalid geometry is ignored. Beyond
    /// [`MAX_DIALOG_GEOMETRIES`] kinds, the first key is forgotten.
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

    /// Writes the settings together while keeping foreign preferences.
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
        // Atomic write: a concurrent reader never sees a truncated file (#250).
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, text)?;
        std::fs::rename(&temporary, path).inspect_err(|_| {
            let _ = std::fs::remove_file(&temporary);
        })
    }

    /// The file the preferences come from, if there is one.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// `true` if the demo has already been offered: it opens only at the very first
    /// launch (#333).
    pub fn first_run_done(&self) -> bool {
        self.first_run_done
    }

    /// Remembers that the demo has been offered, without replacing the other
    /// preferences. Without a configuration folder, the error is returned: the demo
    /// will then come back at the next launch.
    pub fn save_first_run_done(&mut self) -> std::io::Result<()> {
        self.first_run_done = true;
        self.save_field("first_run_done", true.into())
    }

    /// Where the demo file lives: `demo.stf`, next to the preferences file (the
    /// user's configuration folder). `None` without a preferences file.
    pub fn demo_file_path(&self) -> Option<PathBuf> {
        self.path
            .as_deref()
            .and_then(Path::parent)
            .map(|folder| folder.join(DEMO_FILE_NAME))
    }
}

/// The dialog geometries read from the file: unreadable or invalid entries are
/// dropped one by one.
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

/// The recent analyses read from the file: an unreadable entry is dropped, the
/// list is capped (#249).
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

/// `preferences.json` file of the configuration folder
/// (`stokesforge_import::config_dir::config_dir`, #332) — the same folder as
/// `couleurs.json` and `import_recipes.json`.
///
/// Under `cfg(test)` **or** the `test-support` feature: `None`. A unit test
/// never writes into the machine's real preferences (#250); the integration
/// tests, for their part, go through `AppState::new_with_preferences` and a
/// `Preferences::load_from`. `cfg(test)` holds only for the tests of THIS
/// crate: those of `stokesforge-app`, which build an `AppState` through `new`
/// (#263), turn on `test-support` through their dev-dependency — never a normal
/// build (`tests/isolation_preferences.rs` of `stokesforge-app`).
fn default_path() -> Option<PathBuf> {
    if cfg!(any(test, feature = "test-support")) {
        return None;
    }
    stokesforge_import::config_dir::config_dir().map(|dir| dir.join("preferences.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_run_is_pending_while_the_marker_is_missing_or_unreadable() {
        for content in ["{}", r#"{ "first_run_done": "yes" }"#] {
            let (_dir, path) = written(content);
            assert!(!Preferences::load_from(Some(path)).first_run_done());
        }
    }

    #[test]
    fn the_first_run_marker_is_written_and_keeps_the_rest() {
        let (_dir, path) = written(r#"{ "language": "fr", "future": 7 }"#);
        let mut preferences = Preferences::load_from(Some(path.clone()));
        preferences.save_first_run_done().unwrap();
        let reloaded = Preferences::load_from(Some(path.clone()));
        assert!(reloaded.first_run_done());
        assert_eq!(reloaded.language(), Some("fr"));
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.contains("\"future\""), "an unknown field is kept");
    }

    #[test]
    fn the_demo_file_sits_next_to_the_preferences_and_is_absent_without_them() {
        let (dir, path) = written("{}");
        let preferences = Preferences::load_from(Some(path));
        assert_eq!(
            preferences.demo_file_path(),
            Some(dir.path().join(DEMO_FILE_NAME))
        );
        assert_eq!(Preferences::load_from(None).demo_file_path(), None);
    }

    #[test]
    fn the_low_r2_dialog_is_on_by_default_written_and_read_back() {
        let (_dir, path) = written(r#"{ "language": "fr" }"#);
        let mut preferences = Preferences::load_from(Some(path.clone()));
        assert!(preferences.analysis_low_r2_dialog(), "on by default");
        preferences.save_analysis_low_r2_dialog(false).unwrap();
        let reloaded = Preferences::load_from(Some(path));
        assert!(!reloaded.analysis_low_r2_dialog());
        assert_eq!(reloaded.language(), Some("fr"), "the language is kept");
    }

    #[test]
    fn the_low_r2_dialog_turns_on_or_off_in_the_file_round_trip() {
        let (_dir, path) = written("{}");
        let mut preferences = Preferences::load_from(Some(path.clone()));
        for on in [false, true, false] {
            preferences.save_analysis_low_r2_dialog(on).unwrap();
            assert_eq!(preferences.analysis_low_r2_dialog(), on);
            assert_eq!(
                Preferences::load_from(Some(path.clone())).analysis_low_r2_dialog(),
                on
            );
        }
    }

    #[test]
    fn the_low_r2_dialog_is_on_without_the_field_or_without_a_file() {
        let (_dir, path) = written(r#"{ "language": "en", "analysis_keep_results": true }"#);
        assert!(Preferences::load_from(Some(path)).analysis_low_r2_dialog());
        assert!(Preferences::load_from(None).analysis_low_r2_dialog());
        let (_dir, empty) = written("");
        assert!(Preferences::load_from(Some(empty)).analysis_low_r2_dialog());
        let (_dir, null) = written(r#"{ "analysis_low_r2_dialog": null }"#);
        assert!(Preferences::load_from(Some(null)).analysis_low_r2_dialog());
    }

    #[test]
    fn the_low_r2_dialog_reads_the_field_and_ignores_unknown_fields() {
        let (_dir, path) =
            written(r#"{ "analysis_low_r2_dialog": false, "future_field": { "a": 1 } }"#);
        let mut preferences = Preferences::load_from(Some(path.clone()));
        assert!(!preferences.analysis_low_r2_dialog());
        // Writing another field keeps the unknown field and the setting.
        preferences.save_analysis_keep_results(true).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("future_field"), "{text}");
        assert!(!Preferences::load_from(Some(path)).analysis_low_r2_dialog());
    }

    /// A non-boolean value makes only its own field fall back to the default; the
    /// other preferences, language included, are kept (#297).
    #[test]
    fn a_non_boolean_value_falls_back_to_the_default() {
        for raw in ["\"yes\"", "0", "1", "[]", "{}"] {
            let (_dir, path) = written(&format!(
                r#"{{ "language": "fr", "analysis_low_r2_dialog": {raw} }}"#
            ));
            let preferences = Preferences::load_from(Some(path));
            assert!(preferences.analysis_low_r2_dialog(), "{raw}");
            assert_eq!(preferences.language(), Some("fr"), "{raw}");
        }
    }

    #[test]
    fn the_default_analysis_output_is_written_and_read_back_without_losing_the_language() {
        let (_dir, path) = written(r#"{ "language": "fr" }"#);
        let mut preferences = Preferences::load_from(Some(path.clone()));
        assert_eq!(preferences.workbook_header_rows(), (true, false));
        preferences.save_workbook_header_rows(false, true).unwrap();
        assert_eq!(
            Preferences::load_from(preferences.path.clone()).workbook_header_rows(),
            (false, true)
        );
        assert!(!preferences.analysis_keep_results(), "Show by default");
        preferences.save_analysis_keep_results(true).unwrap();
        let reloaded = Preferences::load_from(Some(path));
        assert!(reloaded.analysis_keep_results());
        assert_eq!(reloaded.language(), Some("fr"));
    }

    /// S-F8 (#228): a dialog's geometry is written without losing the other
    /// preferences and read back; an unreadable entry in the file does not take the
    /// others with it, an invalid geometry is not written.
    #[test]
    fn a_dialog_geometry_is_written_and_read_back_without_losing_the_rest() {
        let (_dir, path) = written(
            r#"{ "language": "fr", "dialogs": { "damaged": { "x": "?" }, "zero_size": { "x": 0, "y": 0, "width": 0, "height": 10 } } }"#,
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
            .save_dialog_geometry("properties", geometry)
            .unwrap();
        preferences
            .save_dialog_geometry(
                "invalid",
                DialogGeometry {
                    width: f32::NAN,
                    ..geometry
                },
            )
            .unwrap();
        let reread = Preferences::load_from(Some(path));
        assert_eq!(reread.language(), Some("fr"));
        assert_eq!(
            reread.dialog_geometries().get("properties"),
            Some(&geometry)
        );
        assert_eq!(reread.dialog_geometries().len(), 1);
    }

    /// Beyond [`MAX_DIALOG_GEOMETRIES`] kinds, the oldest key goes; the kind just
    /// written stays.
    #[test]
    fn dialog_geometries_are_capped() {
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
        let dir = tempfile::tempdir().expect("temporary folder");
        let path = dir.path().join("preferences.json");
        std::fs::write(&path, content).expect("write");
        (dir, path)
    }

    #[test]
    fn the_saved_language_is_read_back() {
        let (_dir, path) = written(r#"{ "language": "fr" }"#);
        assert_eq!(Preferences::load_from(Some(path)).language(), Some("fr"));
    }

    #[test]
    fn the_toolbar_layout_is_written_without_losing_the_language() {
        let (_dir, path) = written(r#"{ "language": "fr" }"#);
        let mut preferences = Preferences::load_from(Some(path.clone()));
        preferences
            .save_toolbars(serde_json::json!({ "bars": [] }))
            .expect("write");
        let reread = Preferences::load_from(Some(path));
        assert_eq!(reread.language(), Some("fr"));
        assert_eq!(reread.toolbars(), Some(&serde_json::json!({ "bars": [] })));
    }

    #[test]
    fn a_missing_file_gives_no_preference() {
        let dir = tempfile::tempdir().expect("temporary folder");
        let path = dir.path().join("absent.json");
        let preferences = Preferences::load_from(Some(path));
        assert_eq!(preferences.language(), None);
        assert!(preferences.object_manager_open());
        assert_eq!(Preferences::load_from(None).language(), None);
        assert!(Preferences::load_from(None).object_manager_open());
        assert!(Preferences::default().object_manager_open());
    }

    #[test]
    fn a_corrupted_file_gives_no_preference() {
        for content in ["{ not json", "", "[1, 2]", r#"{ "language": 3 }"#] {
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
    fn the_chosen_language_is_saved_and_read_back() {
        let dir = tempfile::tempdir().expect("temporary folder");
        let path = dir.path().join("config").join("preferences.json");
        let mut preferences = Preferences::load_from(Some(path.clone()));
        preferences.save_language("fr").expect("write");
        assert_eq!(Preferences::load_from(Some(path)).language(), Some("fr"));
    }

    #[test]
    fn saving_the_language_keeps_the_other_fields() {
        let (_dir, path) = written(r#"{ "language": "en", "future": true }"#);
        let mut preferences = Preferences::load_from(Some(path.clone()));
        preferences.save_language("fr").expect("write");
        let content = std::fs::read_to_string(&path).expect("read");
        assert!(content.contains("\"future\""), "{content}");
        assert_eq!(Preferences::load_from(Some(path)).language(), Some("fr"));
    }

    #[test]
    fn saving_over_a_corrupted_file_replaces_it_without_panic() {
        let (_dir, path) = written("{ not json");
        let mut preferences = Preferences::load_from(Some(path.clone()));
        preferences.save_language("en").expect("write");
        assert_eq!(Preferences::load_from(Some(path)).language(), Some("en"));
    }

    #[test]
    fn without_a_config_folder_saving_fails_without_panic() {
        let mut preferences = Preferences::load_from(None);
        assert!(preferences.save_language("fr").is_err());
    }

    #[test]
    fn an_unknown_field_is_ignored() {
        let (_dir, path) = written(r#"{ "language": "en", "future": true }"#);
        assert_eq!(Preferences::load_from(Some(path)).language(), Some("en"));
    }
}

#[cfg(test)]
mod object_manager_tests {
    use super::*;

    #[test]
    fn initial_settings_tolerate_missing_incomplete_and_old_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        let defaults = ObjectManagerSettings::default();
        assert_eq!(
            Preferences::load_from(Some(path.clone())).object_manager_settings(),
            defaults
        );
        for content in ["{corrupted", "{}", r#"{"object_manager_settings":{}}"#] {
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
    fn settings_and_reset_preserve_foreign_preferences() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(
            &path,
            r#"{"language":"fr","object_manager_open":false,"toolbars":{"bars":[]},"future":42}"#,
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
            assert_eq!(document["future"], 42);
            assert_eq!(document["object_manager_show_scales"], expected.show_scales);
        }
    }
    #[test]
    fn remembered_open_state_without_losing_language_toolbars_and_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(
            &path,
            r#"{"language":"fr","toolbars":{"bars":[]},"future":42}"#,
        )
        .unwrap();
        let mut preferences = Preferences::load_from(Some(path.clone()));
        assert!(preferences.object_manager_open());
        preferences.save_object_manager_open(true).unwrap();
        preferences.save_language("en").unwrap();
        let reread = Preferences::load_from(Some(path.clone()));
        assert!(reread.object_manager_open());
        assert_eq!(reread.language(), Some("en"));
        assert_eq!(reread.toolbars(), Some(&serde_json::json!({"bars":[]})));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&path).unwrap())
                .unwrap()["future"],
            42
        );
        preferences.save_object_manager_open(false).unwrap();
        assert!(!Preferences::load_from(Some(path)).object_manager_open());
    }

    #[test]
    fn scales_hidden_by_default_and_toggle_remembered() {
        assert!(!Preferences::load_from(None).object_manager_show_scales());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(&path, r#"{"language":"fr"}"#).unwrap();
        let mut preferences = Preferences::load_from(Some(path.clone()));
        assert!(!preferences.object_manager_show_scales());
        preferences.save_object_manager_show_scales(true).unwrap();
        let reread = Preferences::load_from(Some(path));
        assert!(reread.object_manager_show_scales());
    }

    /// #162: SVG text by default; the choice of outlines is remembered from one
    /// session to the next, without overwriting the other preferences.
    #[test]
    fn the_svg_text_mode_is_remembered_and_defaults_to_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(&path, r#"{"language":"fr","svg_text":"anything"}"#).unwrap();
        let mut prefs = Preferences::load_from(Some(path.clone()));
        assert_eq!(prefs.svg_text_mode(), SvgTextMode::Text);
        prefs.save_svg_text_mode(SvgTextMode::Outlines).unwrap();
        let reread = Preferences::load_from(Some(path.clone()));
        assert_eq!(reread.svg_text_mode(), SvgTextMode::Outlines);
        assert_eq!(reread.language(), Some("fr"));
        prefs.save_svg_text_mode(SvgTextMode::Text).unwrap();
        assert_eq!(
            Preferences::load_from(Some(path)).svg_text_mode(),
            SvgTextMode::Text
        );
    }

    /// B1 (#316): one billion cells with no preference, round trip, bounds, and
    /// passing to the session caps.
    #[test]
    fn the_cell_budget_defaults_to_one_billion_and_is_remembered() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(&path, r#"{"language":"fr"}"#).unwrap();
        let mut prefs = Preferences::load_from(Some(path.clone()));
        assert_eq!(prefs.max_cells_m(), 1000);
        assert_eq!(prefs.load_limits().max_cells, 1_000_000_000);
        prefs.save_max_cells_m(50).unwrap();
        let reread = Preferences::load_from(Some(path.clone()));
        assert_eq!(reread.max_cells_m(), 50);
        assert_eq!(reread.load_limits().max_cells, 50_000_000);
        assert_eq!(reread.language(), Some("fr"));
        prefs.save_max_cells_m(0).unwrap();
        assert_eq!(prefs.max_cells_m(), MIN_MAX_CELLS_M);
        prefs.save_max_cells_m(u32::MAX).unwrap();
        assert_eq!(prefs.max_cells_m(), MAX_MAX_CELLS_M);
        std::fs::write(&path, r#"{"max_cells_m":"a lot"}"#).unwrap();
        assert_eq!(Preferences::load_from(Some(path)).max_cells_m(), 1000);
    }

    /// A2 (#316): 512 MB with no preference, round trip, bounds.
    #[test]
    fn the_size_cap_defaults_to_512_mb_and_is_remembered() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(&path, r#"{"language":"fr"}"#).unwrap();
        let mut prefs = Preferences::load_from(Some(path.clone()));
        assert_eq!(prefs.max_file_mb(), 512);
        assert_eq!(prefs.max_file_bytes(), 512 * 1024 * 1024);
        prefs.save_max_file_mb(64).unwrap();
        let reread = Preferences::load_from(Some(path.clone()));
        assert_eq!(reread.max_file_mb(), 64);
        assert_eq!(reread.language(), Some("fr"));
        // Out of bounds: brought within them, on write as on read.
        prefs.save_max_file_mb(1).unwrap();
        assert_eq!(prefs.max_file_mb(), MIN_MAX_FILE_MB);
        prefs.save_max_file_mb(u32::MAX).unwrap();
        assert_eq!(prefs.max_file_mb(), MAX_MAX_FILE_MB);
        std::fs::write(&path, r#"{"max_file_mb":0}"#).unwrap();
        assert_eq!(
            Preferences::load_from(Some(path.clone())).max_file_mb(),
            MIN_MAX_FILE_MB
        );
        std::fs::write(&path, r#"{"max_file_mb":"a lot"}"#).unwrap();
        assert_eq!(Preferences::load_from(Some(path)).max_file_mb(), 512);
    }

    #[test]
    fn copy_formats_read_back_in_their_order() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("preferences.json");
        let mut prefs = Preferences::load_from(Some(path.clone()));
        assert_eq!(prefs.copy_formats(), default_copy_formats());
        let chosen = [
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
        prefs.save_copy_formats(&chosen).unwrap();
        assert_eq!(
            Preferences::load_from(Some(path)).copy_formats(),
            chosen.as_slice()
        );
    }

    #[test]
    fn a_damaged_format_list_is_repaired() {
        let png_only = [CopyFormatEntry {
            format: CopyFormat::Png,
            on: false,
        }];
        let repaired = normalize_copy_formats(&png_only);
        assert_eq!(repaired.len(), 3, "the missing formats come back");
        assert_eq!(repaired[0].format, CopyFormat::Png);
        assert!(repaired.iter().any(|e| e.on), "at least one format offered");
        let duplicate = [png_only[0], png_only[0]];
        assert_eq!(normalize_copy_formats(&duplicate).len(), 3);

        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("preferences.json");
        std::fs::write(&path, r#"{"copy_formats":"anything"}"#).unwrap();
        assert_eq!(
            Preferences::load_from(Some(path)).copy_formats(),
            default_copy_formats()
        );
    }

    /// Autosave (#317): on and 5 minutes by default, period brought back to 1–60,
    /// unreadable value: the default; round trip.
    ///
    #[test]
    fn autosave_has_its_defaults_and_bounds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        let parse_json = |json: &str| {
            std::fs::write(&path, json).unwrap();
            let prefs = Preferences::load_from(Some(path.clone()));
            (prefs.autosave(), prefs.autosave_minutes())
        };
        assert_eq!(parse_json("{}"), (true, 5), "missing: on, 5 minutes");
        assert_eq!(parse_json(r#"{"autosave_minutes":0}"#), (true, 1));
        assert_eq!(parse_json(r#"{"autosave_minutes":999}"#), (true, 60));
        assert_eq!(
            parse_json(r#"{"autosave_minutes":"x","autosave":"x"}"#),
            (true, 5)
        );
        assert_eq!(parse_json(r#"{"autosave":false}"#), (false, 5));

        let _ = std::fs::remove_file(&path);
        let mut prefs = Preferences::load_from(Some(path.clone()));
        prefs.save_autosave(false).unwrap();
        prefs.save_autosave_minutes(15).unwrap();
        let reread = Preferences::load_from(Some(path));
        assert_eq!((reread.autosave(), reread.autosave_minutes()), (false, 15));
        prefs.save_autosave_minutes(0).unwrap();
        assert_eq!(prefs.autosave_minutes(), 1);
        prefs.save_autosave_minutes(u32::MAX).unwrap();
        assert_eq!(prefs.autosave_minutes(), 60);
    }
}
