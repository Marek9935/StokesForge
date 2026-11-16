//! The user's graph themes: kept on this machine, offered in every project
//! (#539, package #514).
//!
//! The out-of-project counterpart of the themes stored in a project file
//! (#540). Like the user's colours ([`crate::user_colors`]) this is a user
//! preference: it belongs to no project, goes through no command and is **not
//! in the history**. A missing, unreadable, corrupt or oversized file is an
//! empty library, never a panic (invariant 10).
//!
//! # File
//!
//! `user_themes.json` in the configuration folder, next to `couleurs.json`:
//!
//! ```json
//! { "version": 1, "next_id": 4,
//!   "themes":    [ { "id": 3, "data": "<hex>" } ],
//!   "overrides": { "light": "<hex>", "dark": "<hex>" },
//!   "uses":      { "builtin:dark": 2, "user:3": 5, "project:fig a": 1 } }
//! ```
//!
//! `data` is `stokesforge_persistence::encode_theme` (the `.stf` record of a
//! theme, MessagePack) written in hexadecimal: the codec stays the single
//! reader of theme bytes (invariant 3: no serde in the plot crate). The file
//! has its own `version`, independent of the `.stf` one.
//!
//! **A newer `version` is neither read nor overwritten**: the library is empty
//! and read-only for the session ([`UserThemes::read_only`] gives the reason
//! the interface shows), every write is refused with [`UserThemeError::ReadOnly`].
//! A corrupt file, on the other hand, reads as empty and is replaced by the
//! first write. Writes are atomic (temporary file, then rename).
//!
//! # Identity
//!
//! A theme is designated by a [`ThemeRef`]: a built-in (`builtin:light`,
//! `builtin:dark`), a user theme by its **stable id** (`user:<n>`, never
//! reused, survives renaming) or a project theme by **its name** (`project:<name>`,
//! trimmed and lowercased as [`same_theme_name`] compares). Names are unique in
//! a project, and a path would lose the counts when the file is moved or saved
//! under another name; the cost is that two projects' themes of the same name
//! share a count, which is what "Mine" means to the user anyway.
//!
//! # Built-ins
//!
//! The catalogue is [`crate::builtin_themes`]: Light and Dark (colour-only,
//! form 514-4 V4) then seven full-style themes (#544). Editing one stores an
//! **override** ([`UserThemes::set_builtin_from_graph`]), keyed in the file by the
//! built-in's stable key (`light`, `dark`, `publication`, ...);
//! [`UserThemes::reset_builtin`] removes it, and the theme then equals the
//! original again. A file written before #544 holds only the `light` and
//! `dark` keys and reads unchanged. [`ThemeEntry::modified`] tells the
//! interface to mark an edited built-in.
//!
//! # Use counts
//!
//! Counted on this machine, per [`ThemeRef`], including project themes; the
//! tile order is [`UserThemes::ranked`] (Light and Dark first, then the others
//! most used first, ties keep the list order). A count is dropped with its user theme. Project counts are capped
//! ([`MAX_PROJECT_COUNTS`]): the least used are forgotten first.

use std::collections::BTreeMap;
use std::path::PathBuf;

use stokesforge_plot::{same_theme_name, GraphTheme, PlotSpec, MAX_THEMES, MAX_THEME_NAME_CHARS};

use crate::builtin_themes;

/// Version of the file written by this build.
pub const FILE_VERSION: u32 = 1;

/// Largest file read (bytes); a bigger one reads as an empty library.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Most project themes whose use count is kept.
pub const MAX_PROJECT_COUNTS: usize = 500;

/// Built-ins that always come first in the ranking, whatever their use:
/// Light and Dark (the indexes `0` and `1` of the catalogue).
pub const PINNED_BUILTINS: usize = 2;

/// A theme designated independently of where it is stored (module
/// documentation, « Identity »).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ThemeRef {
    /// A built-in, by its index in the catalogue
    /// ([`crate::builtin_themes::KEYS`]: Light `0`, Dark `1`, then the others).
    Builtin(usize),
    /// A theme of the user's library, by stable id.
    User(u64),
    /// A theme of the open project, by name (kept trimmed and lowercased).
    Project(String),
}

impl ThemeRef {
    /// A project theme designated by `name`.
    pub fn project(name: &str) -> Self {
        ThemeRef::Project(name.trim().to_lowercase())
    }

    /// The stable text form (file keys, preferences).
    pub fn key(&self) -> String {
        match self {
            ThemeRef::Builtin(index) => {
                format!(
                    "builtin:{}",
                    builtin_themes::KEYS.get(*index).unwrap_or(&"?")
                )
            }
            ThemeRef::User(id) => format!("user:{id}"),
            ThemeRef::Project(name) => format!("project:{name}"),
        }
    }

    /// The reference of a [`ThemeRef::key`]; `None` if unreadable.
    pub fn from_key(key: &str) -> Option<Self> {
        if let Some(id) = key.strip_prefix("builtin:") {
            return builtin_themes::index_of(id).map(ThemeRef::Builtin);
        }
        if let Some(id) = key.strip_prefix("user:") {
            return id.parse().ok().map(ThemeRef::User);
        }
        key.strip_prefix("project:")
            .filter(|name| !name.is_empty())
            .map(|name| ThemeRef::Project(name.to_string()))
    }
}

/// Why the library cannot be written for this session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOnlyReason {
    /// The file was written by a newer StokesForge (its `version`).
    NewerVersion(u32),
}

/// Why a change of the library was refused.
#[derive(Debug)]
pub enum UserThemeError {
    /// The library is read-only for this session ([`UserThemes::read_only`]).
    ReadOnly,
    /// The name is empty once trimmed.
    EmptyName,
    /// Another theme (or a built-in) already has this name.
    DuplicateName,
    /// The library holds [`MAX_THEMES`] themes already.
    Full,
    /// No such theme.
    Unknown,
    /// The change holds for this session but could not be written.
    Io(std::io::Error),
}

impl std::fmt::Display for UserThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadOnly => write!(f, "the user themes file is read-only"),
            Self::EmptyName => write!(f, "empty theme name"),
            Self::DuplicateName => write!(f, "a theme with this name exists"),
            Self::Full => write!(f, "too many themes"),
            Self::Unknown => write!(f, "no such theme"),
            Self::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for UserThemeError {}

/// A theme of the library as the interface lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeEntry {
    /// How to designate it.
    pub reference: ThemeRef,
    /// The theme (the override of an edited built-in).
    pub theme: GraphTheme,
    /// True for an edited built-in: the interface marks it (V4).
    pub modified: bool,
    /// Times it was applied on this machine.
    pub uses: u64,
}

/// The user's themes and the file where they live.
#[derive(Debug, Clone, Default)]
pub struct UserThemes {
    themes: Vec<(u64, GraphTheme)>,
    overrides: [Option<GraphTheme>; builtin_themes::COUNT],
    uses: BTreeMap<String, u64>,
    next_id: u64,
    read_only: Option<ReadOnlyReason>,
    path: Option<PathBuf>,
}

impl UserThemes {
    /// Loads from the configuration folder. See [`UserThemes::load_from`].
    pub fn load() -> Self {
        Self::load_from(stokesforge_import::config_dir::config_dir().map(|dir| dir.join(FILE_NAME)))
    }

    /// Loads from `path` (`None`: nothing to load, nothing will be written).
    /// Missing, oversized, unreadable or corrupt: empty. Newer version: empty
    /// and read-only.
    pub fn load_from(path: Option<PathBuf>) -> Self {
        let mut library = Self {
            path: path.clone(),
            ..Self::default()
        };
        let Some(path) = path else {
            return library;
        };
        let Ok(meta) = std::fs::metadata(&path) else {
            return library;
        };
        if meta.len() > MAX_FILE_BYTES {
            return library;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            return library;
        };
        let Ok(serde_json::Value::Object(root)) = serde_json::from_str(&text) else {
            return library;
        };
        let Some(version) = root.get("version").and_then(serde_json::Value::as_u64) else {
            return library;
        };
        if version > u64::from(FILE_VERSION) {
            library.read_only = Some(ReadOnlyReason::NewerVersion(
                u32::try_from(version).unwrap_or(u32::MAX),
            ));
            return library;
        }
        library.read_content(&root);
        library
    }

    fn read_content(&mut self, root: &serde_json::Map<String, serde_json::Value>) {
        if let Some(items) = root.get("themes").and_then(serde_json::Value::as_array) {
            for item in items {
                if self.themes.len() >= MAX_THEMES {
                    break;
                }
                let id = item.get("id").and_then(serde_json::Value::as_u64);
                let theme = item
                    .get("data")
                    .and_then(serde_json::Value::as_str)
                    .and_then(from_hex)
                    .and_then(|bytes| stokesforge_persistence::decode_theme(&bytes).ok());
                let (Some(id), Some(theme)) = (id, theme) else {
                    continue;
                };
                let taken =
                    self.themes.iter().any(|(other, kept)| {
                        *other == id || same_theme_name(&kept.name, &theme.name)
                    }) || self.name_of_builtin(&theme.name);
                if !taken {
                    self.themes.push((id, theme));
                }
            }
        }
        if let Some(map) = root.get("overrides").and_then(serde_json::Value::as_object) {
            for (index, key) in builtin_themes::KEYS.iter().enumerate() {
                self.overrides[index] = map
                    .get(*key)
                    .and_then(serde_json::Value::as_str)
                    .and_then(from_hex)
                    .and_then(|bytes| stokesforge_persistence::decode_theme(&bytes).ok())
                    .map(|mut theme| {
                        theme.name = builtin_themes::NAMES[index].to_string();
                        theme
                    });
            }
        }
        if let Some(map) = root.get("uses").and_then(serde_json::Value::as_object) {
            for (key, count) in map.iter().take(MAX_THEMES + MAX_PROJECT_COUNTS + 2) {
                if let (Some(reference), Some(count)) = (ThemeRef::from_key(key), count.as_u64()) {
                    if count > 0 {
                        self.uses.insert(reference.key(), count);
                    }
                }
            }
        }
        let highest = self
            .themes
            .iter()
            .map(|(id, _)| *id)
            .max()
            .map_or(0, |id| id + 1);
        let stored = root
            .get("next_id")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        self.next_id = stored.max(highest);
    }

    /// The file the library is written to, if there is one.
    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }

    /// `Some(reason)` when no change can be written this session.
    pub fn read_only(&self) -> Option<&ReadOnlyReason> {
        self.read_only.as_ref()
    }

    // -- Reading -------------------------------------------------------------

    /// The built-in theme `index` of the catalogue: its override if it was
    /// edited, otherwise the original. `None` past the catalogue.
    pub fn builtin(&self, index: usize) -> Option<GraphTheme> {
        let original = builtin_themes::original(index)?;
        Some(self.overrides[index].clone().unwrap_or(original))
    }

    /// True when the built-in `index` carries an edit (to be reset).
    pub fn is_modified(&self, index: usize) -> bool {
        self.overrides.get(index).is_some_and(Option::is_some)
    }

    /// The theme designated by `reference`, for a built-in or a user theme.
    /// A project theme is not stored here: `None`.
    pub fn get(&self, reference: &ThemeRef) -> Option<GraphTheme> {
        match reference {
            ThemeRef::Builtin(index) => self.builtin(*index),
            ThemeRef::User(id) => self
                .themes
                .iter()
                .find(|(other, _)| other == id)
                .map(|(_, theme)| theme.clone()),
            ThemeRef::Project(_) => None,
        }
    }

    /// Built-ins first (the catalogue order), then the user themes in creation
    /// order.
    pub fn entries(&self) -> Vec<ThemeEntry> {
        let builtins = (0..builtin_themes::COUNT).filter_map(|index| {
            Some(ThemeEntry {
                reference: ThemeRef::Builtin(index),
                theme: self.builtin(index)?,
                modified: self.is_modified(index),
                uses: self.uses_of(&ThemeRef::Builtin(index)),
            })
        });
        let users = self.themes.iter().map(|(id, theme)| ThemeEntry {
            reference: ThemeRef::User(*id),
            theme: theme.clone(),
            modified: false,
            uses: self.uses_of(&ThemeRef::User(*id)),
        });
        builtins.chain(users).collect()
    }

    /// The tile order: Light and Dark **always first** (fixed, whatever their
    /// use), then every other theme, [`UserThemes::entries`] plus the themes of
    /// the open project (`project`, as [`ThemeRef::Project`]), most used first;
    /// equal counts keep the order built-ins (catalogue order), user themes
    /// (creation order), project themes.
    pub fn ranked(&self, project: &[GraphTheme]) -> Vec<ThemeEntry> {
        let mut all = self.entries();
        all.extend(project.iter().map(|theme| ThemeEntry {
            reference: ThemeRef::project(&theme.name),
            theme: theme.clone(),
            modified: false,
            uses: self.uses_of(&ThemeRef::project(&theme.name)),
        }));
        let rest = all.split_off(PINNED_BUILTINS);
        let mut rest = rest;
        rest.sort_by_key(|entry| std::cmp::Reverse(entry.uses));
        all.extend(rest);
        all
    }

    /// Times `reference` was applied on this machine.
    pub fn uses_of(&self, reference: &ThemeRef) -> u64 {
        self.uses.get(&reference.key()).copied().unwrap_or(0)
    }

    // -- Writing -------------------------------------------------------------

    fn writable(&self) -> Result<(), UserThemeError> {
        match self.read_only {
            Some(_) => Err(UserThemeError::ReadOnly),
            None => Ok(()),
        }
    }

    fn name_of_builtin(&self, name: &str) -> bool {
        builtin_themes::NAMES
            .iter()
            .any(|builtin| same_theme_name(builtin, name))
    }

    /// The name cut and checked against the library (`except`: the theme being
    /// renamed or overwritten).
    fn checked_name(&self, name: &str, except: Option<u64>) -> Result<String, UserThemeError> {
        let name: String = name.trim().chars().take(MAX_THEME_NAME_CHARS).collect();
        if name.is_empty() {
            return Err(UserThemeError::EmptyName);
        }
        let taken = self.name_of_builtin(&name)
            || self
                .themes
                .iter()
                .any(|(id, kept)| Some(*id) != except && same_theme_name(&kept.name, &name));
        if taken {
            return Err(UserThemeError::DuplicateName);
        }
        Ok(name)
    }

    /// Adds the style of `spec` as a new theme (data stripped by
    /// [`GraphTheme::capture`]) and returns its id.
    pub fn add_from_graph(&mut self, name: &str, spec: &PlotSpec) -> Result<u64, UserThemeError> {
        self.writable()?;
        let name = self.checked_name(name, None)?;
        if self.themes.len() >= MAX_THEMES {
            return Err(UserThemeError::Full);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.themes.push((id, GraphTheme::capture(name, spec)));
        self.write().map(|()| id)
    }

    /// Renames a user theme (its id and its use count stay).
    pub fn rename(&mut self, id: u64, name: &str) -> Result<(), UserThemeError> {
        self.writable()?;
        let name = self.checked_name(name, Some(id))?;
        let entry = self
            .themes
            .iter_mut()
            .find(|(other, _)| *other == id)
            .ok_or(UserThemeError::Unknown)?;
        entry.1.name = name;
        self.write()
    }

    /// Deletes a user theme and its use count.
    pub fn delete(&mut self, id: u64) -> Result<(), UserThemeError> {
        self.writable()?;
        let before = self.themes.len();
        self.themes.retain(|(other, _)| *other != id);
        if self.themes.len() == before {
            return Err(UserThemeError::Unknown);
        }
        self.uses.remove(&ThemeRef::User(id).key());
        self.write()
    }

    /// Replaces the style of a user theme with that of `spec`, keeping its
    /// name, id and count.
    pub fn overwrite_from_graph(&mut self, id: u64, spec: &PlotSpec) -> Result<(), UserThemeError> {
        self.writable()?;
        let entry = self
            .themes
            .iter_mut()
            .find(|(other, _)| *other == id)
            .ok_or(UserThemeError::Unknown)?;
        entry.1 = GraphTheme::capture(entry.1.name.clone(), spec);
        self.write()
    }

    /// Stores the edited built-in `index` as the style of `spec` (an
    /// override); the name stays the built-in's.
    pub fn set_builtin_from_graph(
        &mut self,
        index: usize,
        spec: &PlotSpec,
    ) -> Result<(), UserThemeError> {
        self.writable()?;
        let name = builtin_themes::NAMES
            .get(index)
            .ok_or(UserThemeError::Unknown)?;
        self.overrides[index] = Some(GraphTheme::capture(*name, spec));
        self.write()
    }

    /// Removes the edit of the built-in `index`: it equals its original again.
    pub fn reset_builtin(&mut self, index: usize) -> Result<(), UserThemeError> {
        self.writable()?;
        let slot = self
            .overrides
            .get_mut(index)
            .ok_or(UserThemeError::Unknown)?;
        if slot.take().is_none() {
            return Ok(());
        }
        self.write()
    }

    /// Counts one application of `reference` (a project theme included).
    pub fn record_use(&mut self, reference: &ThemeRef) -> Result<(), UserThemeError> {
        self.writable()?;
        *self.uses.entry(reference.key()).or_insert(0) += 1;
        self.prune_project_counts();
        self.write()
    }

    /// Moves the use count of `from` to `to` (a renamed project theme keeps
    /// its rank). Nothing to move: nothing written.
    pub fn move_use(&mut self, from: &ThemeRef, to: &ThemeRef) -> Result<(), UserThemeError> {
        self.writable()?;
        let Some(count) = self.uses.remove(&from.key()) else {
            return Ok(());
        };
        *self.uses.entry(to.key()).or_insert(0) += count;
        self.write()
    }

    fn prune_project_counts(&mut self) {
        let mut projects: Vec<(String, u64)> = self
            .uses
            .iter()
            .filter(|(key, _)| key.starts_with("project:"))
            .map(|(key, count)| (key.clone(), *count))
            .collect();
        if projects.len() <= MAX_PROJECT_COUNTS {
            return;
        }
        projects.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        for (key, _) in projects.into_iter().skip(MAX_PROJECT_COUNTS) {
            self.uses.remove(&key);
        }
    }

    fn write(&self) -> Result<(), UserThemeError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        self.write_to(path).map_err(UserThemeError::Io)
    }

    fn write_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        let encode = |theme: &GraphTheme| {
            stokesforge_persistence::encode_theme(theme)
                .map(|bytes| to_hex(&bytes))
                .map_err(std::io::Error::other)
        };
        let mut themes = Vec::new();
        for (id, theme) in &self.themes {
            themes.push(serde_json::json!({ "id": id, "data": encode(theme)? }));
        }
        let mut overrides = serde_json::Map::new();
        for (index, slot) in self.overrides.iter().enumerate() {
            if let Some(theme) = slot {
                overrides.insert(
                    builtin_themes::KEYS[index].to_string(),
                    encode(theme)?.into(),
                );
            }
        }
        let document = serde_json::json!({
            "version": FILE_VERSION,
            "next_id": self.next_id,
            "themes": themes,
            "overrides": overrides,
            "uses": self.uses,
        });
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let text = serde_json::to_string(&document).map_err(std::io::Error::other)?;
        // Atomic: a reader never sees a truncated file.
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, text)?;
        std::fs::rename(&temporary, path).inspect_err(|_| {
            let _ = std::fs::remove_file(&temporary);
        })
    }
}

/// The file name in the configuration folder.
pub const FILE_NAME: &str = "user_themes.json";

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_plot::PlotSpec;

    fn temp() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("temporary folder");
        let path = dir.path().join("config").join(FILE_NAME);
        (dir, path)
    }

    fn styled(background: &str) -> PlotSpec {
        let mut spec = PlotSpec::empty("g");
        spec.background = background.into();
        spec
    }

    #[test]
    fn the_library_survives_a_restart() {
        let (_dir, path) = temp();
        let mut library = UserThemes::load_from(Some(path.clone()));
        let id = library
            .add_from_graph("Poster", &styled("#102030"))
            .expect("add");
        library.record_use(&ThemeRef::User(id)).expect("use");
        library
            .record_use(&ThemeRef::project("Fig A"))
            .expect("use");
        library
            .set_builtin_from_graph(0, &styled("#eeeeee"))
            .expect("edit");
        let reread = UserThemes::load_from(Some(path));
        let theme = reread.get(&ThemeRef::User(id)).expect("kept");
        assert_eq!(theme.name, "Poster");
        assert_eq!(theme.style.background, "#102030");
        assert_eq!(reread.uses_of(&ThemeRef::User(id)), 1);
        assert_eq!(reread.uses_of(&ThemeRef::project(" fig a ")), 1);
        assert!(reread.is_modified(0) && !reread.is_modified(1));
    }

    #[test]
    fn a_corrupt_or_hostile_file_gives_an_empty_library() {
        for content in [
            "{ not json".to_string(),
            "[1,2]".to_string(),
            r#"{"themes":[]}"#.to_string(),
            r#"{"version":1,"themes":[{"id":1,"data":"zz"},{"id":"x"},7],"overrides":{"light":"00"},"uses":{"user:q":3,"":1}}"#.to_string(),
        ] {
            let (_dir, path) = temp();
            std::fs::create_dir_all(path.parent().expect("folder")).expect("folder");
            std::fs::write(&path, content).expect("write");
            let library = UserThemes::load_from(Some(path));
            assert!(library.read_only().is_none());
            assert_eq!(
                library.entries().len(),
                builtin_themes::COUNT,
                "built-ins only"
            );
            assert!(!library.is_modified(0));
        }
    }

    #[test]
    fn a_newer_version_is_neither_read_nor_overwritten() {
        let (_dir, path) = temp();
        std::fs::create_dir_all(path.parent().expect("folder")).expect("folder");
        let newer = r#"{"version":99,"themes":[],"future":true}"#;
        std::fs::write(&path, newer).expect("write");
        let mut library = UserThemes::load_from(Some(path.clone()));
        assert_eq!(library.read_only(), Some(&ReadOnlyReason::NewerVersion(99)));
        assert!(matches!(
            library.add_from_graph("A", &styled("#111111")),
            Err(UserThemeError::ReadOnly)
        ));
        assert!(matches!(
            library.record_use(&ThemeRef::Builtin(0)),
            Err(UserThemeError::ReadOnly)
        ));
        assert_eq!(std::fs::read_to_string(&path).expect("read"), newer);
    }

    #[test]
    fn an_edited_built_in_equals_the_original_after_a_reset() {
        let (_dir, path) = temp();
        let mut library = UserThemes::load_from(Some(path.clone()));
        let original = library.builtin(0).expect("light");
        library
            .set_builtin_from_graph(0, &styled("#abcdef"))
            .expect("edit");
        assert_ne!(library.builtin(0).expect("light"), original);
        assert!(library.entries()[0].modified);
        library.reset_builtin(0).expect("reset");
        assert_eq!(library.builtin(0).expect("light"), original);
        assert!(!library.entries()[0].modified);
        assert_eq!(
            UserThemes::load_from(Some(path)).builtin(0).expect("light"),
            original,
            "the reset is written"
        );
    }

    #[test]
    fn the_ranking_follows_the_use_counts() {
        let mut library = UserThemes::load_from(None);
        let a = library.add_from_graph("A", &styled("#111111")).expect("a");
        let b = library.add_from_graph("B", &styled("#222222")).expect("b");
        let project = [GraphTheme::capture("Mine", &styled("#333333"))];
        let order = |library: &UserThemes| -> Vec<String> {
            library
                .ranked(&project)
                .iter()
                .map(|entry| entry.theme.name.clone())
                .collect()
        };
        let tail = |library: &UserThemes| order(library)[builtin_themes::COUNT..].to_vec();
        assert_eq!(order(&library)[..2], ["Light", "Dark"]);
        assert_eq!(
            order(&library)[2..builtin_themes::COUNT],
            builtin_themes::NAMES[2..],
            "ties keep the catalogue order"
        );
        assert_eq!(tail(&library), ["A", "B", "Mine"]);
        library.record_use(&ThemeRef::User(b)).expect("use");
        library.record_use(&ThemeRef::project("mine")).expect("use");
        library.record_use(&ThemeRef::project("mine")).expect("use");
        library
            .record_use(&ThemeRef::Builtin(5))
            .expect("use a new built-in");
        let ranked = order(&library);
        assert_eq!(
            ranked[..5],
            ["Light", "Dark", "Mine", "Fit + Residuals", "B"]
        );
        assert_eq!(
            ranked[5], "Publication",
            "a tie keeps built-ins before user themes"
        );
        library.delete(b).expect("delete");
        assert_eq!(library.uses_of(&ThemeRef::User(b)), 0);
        let _ = a;
    }

    #[test]
    fn names_are_unique_ignoring_case_and_ids_are_not_reused() {
        let mut library = UserThemes::load_from(None);
        let a = library
            .add_from_graph("Poster", &styled("#111111"))
            .expect("a");
        assert!(matches!(
            library.add_from_graph(" poster ", &styled("#222222")),
            Err(UserThemeError::DuplicateName)
        ));
        assert!(matches!(
            library.add_from_graph("light", &styled("#222222")),
            Err(UserThemeError::DuplicateName)
        ));
        assert!(matches!(
            library.add_from_graph("  ", &styled("#222222")),
            Err(UserThemeError::EmptyName)
        ));
        library
            .rename(a, "Poster")
            .expect("same name is the theme's own");
        library.delete(a).expect("delete");
        let b = library
            .add_from_graph("Next", &styled("#333333"))
            .expect("b");
        assert_ne!(a, b);
    }

    #[test]
    fn a_library_without_a_file_keeps_changes_in_memory() {
        let mut library = UserThemes::load_from(None);
        let id = library
            .add_from_graph("A", &styled("#111111"))
            .expect("add");
        assert!(library.get(&ThemeRef::User(id)).is_some());
    }

    #[test]
    fn theme_keys_round_trip() {
        for reference in [
            ThemeRef::Builtin(1),
            ThemeRef::Builtin(5),
            ThemeRef::User(12),
            ThemeRef::project("Fig A"),
        ] {
            assert_eq!(ThemeRef::from_key(&reference.key()), Some(reference));
        }
        assert_eq!(ThemeRef::from_key("builtin:nope"), None);
        assert_eq!(ThemeRef::from_key("user:x"), None);
    }

    #[test]
    fn light_and_dark_stay_first_with_no_use_while_another_theme_has_many() {
        let mut library = UserThemes::load_from(None);
        let mine = library
            .add_from_graph("Mine", &styled("#111111"))
            .expect("add");
        for _ in 0..50 {
            library.record_use(&ThemeRef::User(mine)).expect("use");
            library.record_use(&ThemeRef::Builtin(4)).expect("use");
        }
        library.record_use(&ThemeRef::User(mine)).expect("use");
        let ranked = library.ranked(&[]);
        assert_eq!(ranked[0].reference, ThemeRef::Builtin(0));
        assert_eq!(ranked[1].reference, ThemeRef::Builtin(1));
        assert_eq!(ranked[0].uses + ranked[1].uses, 0);
        assert_eq!(
            ranked[2].reference,
            ThemeRef::User(mine),
            "most used of the rest"
        );
        assert_eq!(ranked[3].reference, ThemeRef::Builtin(4));
    }

    #[test]
    fn every_new_built_in_can_be_edited_and_reset_to_its_original() {
        let (_dir, path) = temp();
        let mut library = UserThemes::load_from(Some(path.clone()));
        for index in 2..builtin_themes::COUNT {
            let original = library.builtin(index).expect("built-in");
            assert!(!original.colors_only);
            library
                .set_builtin_from_graph(index, &styled("#abcdef"))
                .expect("edit");
            assert!(library.is_modified(index));
            assert_ne!(library.builtin(index).expect("built-in"), original);
            assert_eq!(
                UserThemes::load_from(Some(path.clone()))
                    .builtin(index)
                    .expect("built-in")
                    .style
                    .background,
                "#abcdef",
                "the edit is written under its key"
            );
            library.reset_builtin(index).expect("reset");
            assert_eq!(library.builtin(index).expect("built-in"), original);
            assert_eq!(builtin_themes::original(index), Some(original));
        }
    }

    #[test]
    fn a_file_written_before_the_catalogue_still_reads() {
        let (_dir, path) = temp();
        let mut library = UserThemes::load_from(Some(path.clone()));
        library
            .set_builtin_from_graph(0, &styled("#eeeeee"))
            .expect("edit light");
        library
            .set_builtin_from_graph(1, &styled("#222222"))
            .expect("edit dark");
        library.record_use(&ThemeRef::Builtin(1)).expect("use");
        let text = std::fs::read_to_string(&path).expect("file");
        let json: serde_json::Value = serde_json::from_str(&text).expect("json");
        let keys: Vec<&String> = json["overrides"].as_object().expect("map").keys().collect();
        assert_eq!(keys, ["dark", "light"], "the keys of #539 are kept");
        assert!(json["uses"].get("builtin:dark").is_some());
        let reread = UserThemes::load_from(Some(path));
        assert!(reread.is_modified(0) && reread.is_modified(1));
        assert_eq!(reread.builtin(1).expect("dark").style.background, "#222222");
        assert_eq!(reread.uses_of(&ThemeRef::Builtin(1)), 1);
        assert!((2..builtin_themes::COUNT).all(|index| !reread.is_modified(index)));
    }

    #[test]
    fn a_name_of_a_built_in_is_refused_to_a_user_theme() {
        let mut library = UserThemes::load_from(None);
        for name in builtin_themes::NAMES {
            assert!(matches!(
                library.add_from_graph(name, &styled("#111111")),
                Err(UserThemeError::DuplicateName)
            ));
        }
    }
}
