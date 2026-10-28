//! Recovery copies: actor death (#310) and periodic autosave (#317), in
//! `recovery/` of the application's data folder.
//!
//! All of it is **pure** logic (folder as a parameter), testable without a
//! window or a clock; `state/autosave.rs` adds only the decision.
//!
//! # Actor death (#310)
//!
//! A panic of the actor's thread destroys the `Project`: only the last
//! snapshot that the interface has read back remains (immutable `Arc<Tree>`,
//! already shared). It is rewritten to a **new** file — never over the user's
//! `.stf`, which the copy may precede by one command (a panic in the middle of
//! a recalculation leaves the earlier state).
//!
//! `Project::from_tree(.., 0)` resumes identifiers from the highest live one:
//! those of deleted nodes may come back, with no effect since the history is
//! lost with the actor.
//!
//! # Autosave (#317)
//!
//! One **slot per process**: `<name|Untitled>-<pid>.stf`, overwritten at each
//! deadline (the write is atomic, temporary file then rename, on the
//! persistence side), with a `.json` **sidecar** (origin path, time) and a
//! `.lock` lock held (`File::try_lock`) as long as the process lives. At the
//! next startup, the lock tells apart the copy of another StokesForge still
//! open (not offered) from that of a dead process: the system releases it when
//! the process ends, crashes included. A copy is offered only if it is
//! **newer** than the original `.stf`; with no sidecar or no `.stf`, it
//! always is (an unreadable sidecar degrades, invariant 10, it never blocks
//! recovery).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use stokesforge_core::Project;
use stokesforge_persistence::PersistenceError;
use stokesforge_session::Snapshot;

/// Why the recovery copy could not be written.
#[derive(Debug)]
pub enum RecoveryError {
    /// The recovery folder could not be created.
    CreateDir(std::io::Error),
    /// Writing the file failed.
    Save(PersistenceError),
}

/// Folder of the recovery copies (`stokesforge_import::config_dir::recovery_dir`,
/// #332): `recovery/` in the application's data folder, or, when
/// `STOKESFORGE_CONFIG_DIR` is set, `<that folder>/recovery`. `None` under
/// `cfg(test)` or the `test-support` feature: a test never writes into the
/// machine's real data (like the preferences, #250).
pub(crate) fn default_dir() -> Option<PathBuf> {
    if cfg!(any(test, feature = "test-support")) {
        return None;
    }
    stokesforge_import::config_dir::recovery_dir()
}

/// The copy name of a project: its file name without extension, `Untitled`
/// when it has no path (#317).
pub fn stem_of(project_path: &str) -> String {
    Path::new(project_path).file_stem().map_or_else(
        || "Untitled".to_string(),
        |s| s.to_string_lossy().into_owned(),
    )
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Writes `snapshot` to a new file in `dir` and returns its path. The name
/// reuses the project's (`project_path`) followed by the time: two crashes do
/// not overwrite each other, and `save` never meets an existing file. The
/// sidecar is written too: this copy is offered at the next startup like those
/// of the autosave (#317).
pub fn write_copy(
    dir: &Path,
    project_path: &str,
    snapshot: &Snapshot,
) -> Result<PathBuf, RecoveryError> {
    std::fs::create_dir_all(dir).map_err(RecoveryError::CreateDir)?;
    let stem = stem_of(project_path);
    let seconds = now_seconds();
    let mut path = dir.join(format!("{stem}-{seconds}.stf"));
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("{stem}-{seconds}-{n}.stf"));
        n += 1;
    }
    let project = Project::from_tree(snapshot.tree.clone(), 0);
    let view = stokesforge_persistence::ViewTables {
        window_geometries: (*snapshot.window_geometries).clone(),
        column_widths: (*snapshot.column_widths).clone(),
    };
    stokesforge_persistence::save_with_geometries(&project, &view, &path)
        .map_err(RecoveryError::Save)?;
    write_sidecar(&path, project_path);
    Ok(path)
}

/// A copy's sidecar: where it comes from and when it was written.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Sidecar {
    /// The project's path at the time of writing; empty: never saved.
    origin: String,
    /// Seconds since the Unix epoch.
    at: u64,
}

fn sidecar_path(stf: &Path) -> PathBuf {
    stf.with_extension("json")
}

fn lock_path(stf: &Path) -> PathBuf {
    stf.with_extension("lock")
}

/// Writes the sidecar of `stf`. A failure is ignored: a copy with no sidecar
/// is still offered (with no origin path or date comparison).
fn write_sidecar(stf: &Path, origin: &str) {
    let sidecar = Sidecar {
        origin: origin.to_string(),
        at: now_seconds(),
    };
    if let Ok(text) = serde_json::to_string(&sidecar) {
        let _ = std::fs::write(sidecar_path(stf), text);
    }
}

/// The autosave slot **of this process**: the held lock, and the copy's path.
#[derive(Debug)]
pub struct Slot {
    stem: String,
    stf: PathBuf,
    // Held for its existence alone: the lock drops when the file closes.
    lock: File,
}

impl Slot {
    /// Takes the slot `<stem>-<pid>` of `dir` (created if needed) and sets its
    /// lock. A lock already held (another process, a reused pid) is an error.
    pub fn acquire(dir: &Path, stem: &str, pid: u32) -> Result<Self, std::io::Error> {
        std::fs::create_dir_all(dir)?;
        let stf = dir.join(format!("{stem}-{pid}.stf"));
        let lock = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(lock_path(&stf))?;
        lock.try_lock().map_err(|err| match err {
            std::fs::TryLockError::Error(io) => io,
            std::fs::TryLockError::WouldBlock => std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "the recovery slot is held by another process",
            ),
        })?;
        // A copy left by a dead process with the same pid (reused pid) must
        // not be overwritten: it is set aside under another name, and will be
        // offered like the others.
        if stf.exists() {
            let aside = (1..)
                .map(|n| dir.join(format!("{stem}-{pid}-{n}.stf")))
                .find(|path| !path.exists() && !sidecar_path(path).exists());
            if let Some(aside) = aside {
                let _ = std::fs::rename(&stf, &aside);
                let _ = std::fs::rename(sidecar_path(&stf), sidecar_path(&aside));
            }
        }
        Ok(Self {
            stem: stem.to_string(),
            stf,
            lock,
        })
    }

    /// The project name under which the slot is taken.
    pub fn stem(&self) -> &str {
        &self.stem
    }

    /// The copy's file.
    pub fn path(&self) -> &Path {
        &self.stf
    }

    /// Writes the sidecar, **before** the copy: a copy always exists with its
    /// sidecar.
    pub fn write_sidecar(&self, origin: &str) {
        write_sidecar(&self.stf, origin);
    }

    /// Deletes the copy, its sidecar and the lock (clean close, save). The
    /// lock is released before erasing its file (Windows).
    pub fn remove(self) {
        let Self { stf, lock, .. } = self;
        drop(lock);
        remove_files(&stf);
    }
}

fn remove_files(stf: &Path) {
    let _ = std::fs::remove_file(stf);
    let _ = std::fs::remove_file(sidecar_path(stf));
    let _ = std::fs::remove_file(lock_path(stf));
}

/// A recovery copy offered at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The copy's file.
    pub stf: PathBuf,
    /// The project's path at the time of the copy; `None`: never saved, or
    /// sidecar lost.
    pub origin: Option<PathBuf>,
    /// The name to show: the original file's; `None`: `Untitled`.
    pub name: Option<String>,
    /// Unix seconds of the copy's write.
    pub saved_at: u64,
}

impl Found {
    /// Deletes the copy, its sidecar and its orphan lock.
    pub fn delete(&self) {
        remove_files(&self.stf);
    }
}

/// The name of a copy with no sidecar: the file's without its numeric suffix
/// (`-pid` or `-seconds`), `None` if nothing is left.
fn name_from_copy(stf: &Path) -> Option<String> {
    let stem = stf.file_stem()?.to_string_lossy().into_owned();
    let trimmed = match stem.rsplit_once('-') {
        Some((head, tail))
            if !head.is_empty() && !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) =>
        {
            head.to_string()
        }
        _ => stem,
    };
    (trimmed != "Untitled").then(|| format!("{trimmed}.stf"))
}

/// Is a copy held by a live process?
fn is_held(stf: &Path) -> bool {
    let Ok(lock) = File::options().write(true).open(lock_path(stf)) else {
        return false;
    };
    matches!(lock.try_lock(), Err(std::fs::TryLockError::WouldBlock))
}

/// The copies to offer in `dir`, newest first: those whose process is gone
/// and which are newer than their `.stf`. Missing or unreadable folder: none.
pub fn scan(dir: &Path) -> Vec<Found> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<Found> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "stf"))
        .filter(|stf| !is_held(stf))
        .filter_map(|stf| {
            let modified = std::fs::metadata(&stf).and_then(|m| m.modified()).ok()?;
            let sidecar: Option<Sidecar> = std::fs::read_to_string(sidecar_path(&stf))
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok());
            let origin = sidecar
                .as_ref()
                .filter(|s| !s.origin.is_empty())
                .map(|s| PathBuf::from(&s.origin));
            // Not newer than the `.stf`: it has been saved since.
            let origin_modified = origin
                .as_ref()
                .and_then(|o| std::fs::metadata(o).and_then(|m| m.modified()).ok());
            if origin_modified.is_some_and(|o| modified <= o) {
                return None;
            }
            let name = match &sidecar {
                Some(_) => origin
                    .as_ref()
                    .and_then(|o| o.file_name())
                    .map(|n| n.to_string_lossy().into_owned()),
                None => name_from_copy(&stf),
            };
            let saved_at = sidecar.as_ref().map_or_else(
                || {
                    modified
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs())
                },
                |s| s.at,
            );
            Some(Found {
                stf,
                origin,
                name,
                saved_at,
            })
        })
        .collect();
    found.sort_by(|a, b| b.saved_at.cmp(&a.saved_at).then_with(|| a.stf.cmp(&b.stf)));
    found
}

/// `YYYY-MM-DD HH:MM` (UTC) of an instant in Unix seconds: the application has
/// no date library, and does not know the local time.
pub fn format_utc(seconds: u64) -> String {
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let rest = seconds % 86_400;
    // Days since 1970-01-01 -> civil date (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    /// Any `.stf` (content irrelevant to `scan`).
    fn touch(path: &Path, age_seconds: u64) {
        std::fs::write(path, b"x").unwrap();
        let file = File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(age_seconds))
            .unwrap();
    }

    fn copy_with_sidecar(dir: &Path, name: &str, origin: &str, age: u64) -> PathBuf {
        let stf = dir.join(name);
        touch(&stf, age);
        write_sidecar(&stf, origin);
        stf
    }

    #[test]
    fn the_civil_date_is_right() {
        assert_eq!(format_utc(0), "1970-01-01 00:00");
        assert_eq!(
            format_utc(951_782_400 + 3600 * 13 + 60 * 5),
            "2000-02-29 13:05"
        );
        assert_eq!(format_utc(1_792_598_400), "2026-10-21 16:00");
    }

    #[test]
    fn the_name_of_a_copy_without_a_path_is_untitled() {
        assert_eq!(stem_of(""), "Untitled");
        assert_eq!(stem_of("/a/b/measurements.stf"), "measurements");
    }

    #[test]
    fn older_than_the_stf_is_excluded_newer_is_listed() {
        let dir = tempfile::tempdir().unwrap();
        let rec = dir.path().join("recovery");
        std::fs::create_dir_all(&rec).unwrap();
        let user = dir.path().join("work.stf");
        touch(&user, 100);
        // Copy written 200 s ago: older than the .stf (100 s).
        copy_with_sidecar(&rec, "old-1.stf", user.to_str().unwrap(), 200);
        // Copy written 10 s ago: newer.
        let recent = copy_with_sidecar(&rec, "recent-2.stf", user.to_str().unwrap(), 10);
        let found = scan(&rec);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].stf, recent);
        assert_eq!(found[0].origin.as_deref(), Some(user.as_path()));
        assert_eq!(found[0].name.as_deref(), Some("work.stf"));
    }

    #[test]
    fn without_a_stf_or_without_a_sidecar_the_copy_is_listed() {
        let dir = tempfile::tempdir().unwrap();
        let rec = dir.path();
        // Origin gone from the disk.
        copy_with_sidecar(rec, "lost-1.stf", "/nowhere/lost.stf", 5);
        // Never saved: empty origin.
        copy_with_sidecar(rec, "Untitled-2.stf", "", 5);
        // No sidecar (a #310 copy from before the sidecar): name taken from the file.
        touch(&rec.join("old-1700000000.stf"), 5);
        // Unreadable sidecar: degraded like an absence (invariant 10).
        touch(&rec.join("damaged-3.stf"), 5);
        std::fs::write(rec.join("damaged-3.json"), "{not json").unwrap();
        let mut names: Vec<_> = scan(rec).into_iter().map(|f| f.name).collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                None,
                Some("damaged.stf".to_string()),
                Some("lost.stf".to_string()),
                Some("old.stf".to_string()),
            ]
        );
    }

    #[test]
    fn a_310_recovery_copy_is_listed_with_its_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let session =
            stokesforge_session::Session::spawn(Project::new("Demo").unwrap(), None).unwrap();
        let snapshot = session.handle().snapshot().unwrap();
        let path = write_copy(dir.path(), "/x/work.stf", &snapshot).unwrap();
        assert!(sidecar_path(&path).exists());
        let found = scan(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].stf, path);
        assert_eq!(found[0].name.as_deref(), Some("work.stf"));
        session.shutdown().unwrap();
    }

    #[test]
    fn a_held_lock_excludes_the_copy_and_dropping_it_releases_the_copy() {
        let dir = tempfile::tempdir().unwrap();
        let slot = Slot::acquire(dir.path(), "project", 4242).unwrap();
        slot.write_sidecar("");
        touch(slot.path(), 5);
        assert!(scan(dir.path()).is_empty(), "held by a live process");
        assert!(
            Slot::acquire(dir.path(), "project", 4242).is_err(),
            "a slot cannot be taken twice"
        );
        let path = slot.path().to_path_buf();
        drop(slot);
        let found = scan(dir.path());
        assert_eq!(
            found.len(),
            1,
            "with the lock released, the copy is offered"
        );
        assert_eq!(found[0].stf, path);
    }

    #[test]
    fn remove_and_delete_erase_copy_sidecar_and_lock() {
        let dir = tempfile::tempdir().unwrap();
        let slot = Slot::acquire(dir.path(), "a", 1).unwrap();
        slot.write_sidecar("");
        touch(slot.path(), 5);
        slot.remove();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);

        let slot = Slot::acquire(dir.path(), "b", 2).unwrap();
        slot.write_sidecar("");
        touch(slot.path(), 5);
        drop(slot);
        let found = scan(dir.path());
        assert_eq!(found.len(), 1);
        found[0].delete();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
