//! Default folders of the file dialogs (#148): where Open and Save As, the
//! import and the exports start when the user has not asked for anything else.
//!
//! Three kinds of folder, set in Preferences › Folders: **Projects** (Open and
//! Save As of `.stf` files), **Import** (instrument files and import templates)
//! and **Export** (graph exports). Empty means "the last folder used", which is
//! what the file dialogs did before.
//!
//! The rule (decisions of 2026-10-29): the default folder serves at the **first
//! use of the session**, then the session's last folder of that kind takes over
//! ([`FolderMemory`], never saved: another launch starts again from the default).
//! Changing a default during the session forgets the session's memory of that
//! kind. A default or a remembered folder that no longer exists is skipped
//! without noise ([`resolve`]); only the Preferences page says "Folder not
//! found". Only a file dialog feeds the memory: dropping a file or opening one of
//! the recent files does not.
//!
//! Pure decisions, no egui (#263). The existence test is a parameter of
//! [`resolve`]: it touches the disk, so it runs on the picker's thread, never on
//! the interface thread (`file_picker.rs`).

use std::path::{Path, PathBuf};

/// A kind of folder the file dialogs start in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FolderKind {
    /// Open and Save As of `.stf` projects.
    Projects,
    /// Instrument files to import, import templates (`.json`).
    Import,
    /// Graph exports.
    Export,
}

impl FolderKind {
    pub const ALL: [Self; 3] = [Self::Projects, Self::Import, Self::Export];

    /// Key of the default in `preferences.json`.
    pub fn preferences_key(self) -> &'static str {
        match self {
            Self::Projects => "folder_projects",
            Self::Import => "folder_import",
            Self::Export => "folder_export",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Projects => 0,
            Self::Import => 1,
            Self::Export => 2,
        }
    }
}

/// The last folder a file dialog answered in, per kind, for this session only.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FolderMemory {
    last: [Option<PathBuf>; 3],
}

impl FolderMemory {
    pub fn last(&self, kind: FolderKind) -> Option<&Path> {
        self.last[kind.index()].as_deref()
    }

    /// Remembers the folder a dialog was answered in.
    pub fn record(&mut self, kind: FolderKind, folder: &Path) {
        self.last[kind.index()] = Some(folder.to_path_buf());
    }

    /// Forgets the kind: the default takes over again (a default was changed).
    pub fn forget(&mut self, kind: FolderKind) {
        self.last[kind.index()] = None;
    }
}

/// The folders a dialog may start in, in the order of [`resolve`]; plain paths,
/// nothing has been looked up on the disk yet.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FolderCandidates {
    /// The last folder of the session for this kind.
    pub session_last: Option<PathBuf>,
    /// The default of Preferences › Folders (`None` if empty).
    pub default: Option<PathBuf>,
    /// What serves when neither exists (Import: the folder of the most recent
    /// imported file).
    pub fallback: Option<PathBuf>,
}

impl FolderCandidates {
    /// The starting folder of a dialog: see [`resolve`].
    pub fn resolve(&self, is_dir: impl Fn(&Path) -> bool) -> Option<PathBuf> {
        resolve(
            self.session_last.as_deref(),
            self.default.as_deref(),
            self.fallback.as_deref(),
            is_dir,
        )
    }
}

/// The folder a dialog starts in: the session's last folder, else the default,
/// else the fallback, else `None` (the system decides). A candidate that is not
/// a folder (`is_dir`) or is empty is skipped. Never `Some("")`: an empty path
/// would be handed to the system's dialog as a directory.
pub fn resolve(
    session_last: Option<&Path>,
    default: Option<&Path>,
    fallback: Option<&Path>,
    is_dir: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    [session_last, default, fallback]
        .into_iter()
        .flatten()
        .find(|path| !path.as_os_str().is_empty() && is_dir(path))
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(existing: &'static [&'static str]) -> impl Fn(&Path) -> bool {
        move |path| existing.iter().any(|dir| Path::new(dir) == path)
    }

    fn p(path: &str) -> Option<&Path> {
        Some(Path::new(path))
    }

    #[test]
    fn the_default_serves_when_the_session_has_nothing() {
        let found = resolve(None, p("/d"), None, dirs(&["/d"]));
        assert_eq!(found, Some(PathBuf::from("/d")));
    }

    #[test]
    fn the_session_folder_wins_over_the_default() {
        let found = resolve(p("/s"), p("/d"), p("/r"), dirs(&["/s", "/d", "/r"]));
        assert_eq!(found, Some(PathBuf::from("/s")));
    }

    /// A missing folder is skipped without noise; the order goes on.
    #[test]
    fn a_missing_folder_falls_through_to_the_next() {
        assert_eq!(
            resolve(p("/s"), p("/d"), p("/r"), dirs(&["/d", "/r"])),
            Some(PathBuf::from("/d"))
        );
        assert_eq!(
            resolve(None, p("/d"), p("/r"), dirs(&["/r"])),
            Some(PathBuf::from("/r"))
        );
        assert_eq!(resolve(None, p("/d"), p("/r"), dirs(&[])), None);
        assert_eq!(resolve(None, None, None, dirs(&["/d"])), None);
    }

    /// An empty path is never a folder to start in, even if the test says so.
    #[test]
    fn an_empty_path_is_never_chosen() {
        assert_eq!(resolve(p(""), p(""), None, |_| true), None);
        assert_eq!(
            resolve(p(""), p("/d"), None, |_| true),
            Some(PathBuf::from("/d"))
        );
    }

    #[test]
    fn the_memory_records_per_kind_and_forgets_one_kind() {
        let mut memory = FolderMemory::default();
        memory.record(FolderKind::Projects, Path::new("/x"));
        memory.record(FolderKind::Export, Path::new("/e"));
        assert_eq!(memory.last(FolderKind::Projects), p("/x"));
        assert_eq!(memory.last(FolderKind::Import), None);
        memory.forget(FolderKind::Projects);
        assert_eq!(memory.last(FolderKind::Projects), None);
        assert_eq!(
            memory.last(FolderKind::Export),
            p("/e"),
            "the other kind stays"
        );
    }
}
