//! Automatic save and recovery at startup (#317).
//!
//! Every `autosave_minutes` (5 by default, 1 to 60), if the project has changed
//! since the last mark (startup, save, load, previous copy), `AppState` asks
//! the actor to write the project into the **slot** of this process
//! (`recovery::Slot`, a single overwritten file). This is `handle.save`: it
//! creates no history entry, changes neither `saved_revision`, nor the path,
//! nor the "modified" title, and never writes the user's `.stf` (invariant 1:
//! nothing of the model moves).
//!
//! The clock is **injected** (`poll_autosave(now)`): the application calls it
//! once per frame from `logic()` and schedules its wake-up on the returned
//! delay; a test advances `now` without waiting. A deadline that falls during
//! a macro, a script, an import or a recalculation is pushed back by a few
//! seconds, without error: `handle.save` refuses during a macro, and a
//! recalculation makes the actor wait.
//!
//! A write failure (impossible folder) gives **a single** warning as long as it
//! lasts; the next successful copy re-arms it.
//!
//! At startup, `scan_recovery` (called by the root binary only) lists the
//! copies of vanished processes that are newer than their `.stf`; the window
//! offers Open, Ignore and Delete (with confirmation, deletion cannot be
//! undone). Open loads the copy into the current window, behind "Save?" if
//! there is work, under the **original path** (empty for Untitled) and as
//! unsaved work; the opened copy is then recopied into the slot of this
//! process, then erased. Ignore leaves it on disk: it comes back at the next
//! startup.
//!
//! The slot disappears at save, at clean close, and for New, Open of another
//! project and Discard (which are clean closes of the old project). Lost
//! session: the copy is kept.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::AppState;
use crate::errors;
use crate::recovery::{self, Found, Slot};
use crate::tr;

/// Retry delay when a deadline falls during work in progress.
pub const AUTOSAVE_RETRY: Duration = Duration::from_secs(5);

/// The state of the automatic save and of the recovery list.
#[derive(Debug, Default)]
pub(super) struct AutosaveState {
    /// Next deadline; `None`: to arm at the next `poll_autosave`.
    deadline: Option<Instant>,
    /// The revisions already covered (saved, loaded or copied);
    /// `None`: the project is to be copied anyway.
    mark: Option<(u64, u64)>,
    /// The slot of this process, once taken.
    slot: Option<Slot>,
    /// The last write failed (the warning has already been given).
    failed: bool,
    /// The copies offered at startup, as long as the window is open.
    found: Vec<Found>,
    /// The copy whose deletion is waiting for confirmation.
    pending_delete: Option<PathBuf>,
}

impl AutosaveState {
    pub(super) fn at(mark: (u64, u64)) -> Self {
        Self {
            mark: Some(mark),
            ..Self::default()
        }
    }
}

/// A line of the recovery window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryRow {
    /// The copy; it is the key of the Open, Ignore and Delete actions.
    pub path: PathBuf,
    /// "name · date", ready to paint.
    pub text: String,
}

enum AutosaveError {
    Dir(std::io::Error),
    Session(stokesforge_session::SessionError),
}

impl AppState {
    /// Is automatic save active (preference)?
    pub fn autosave_enabled(&self) -> bool {
        self.object_manager_preferences.autosave()
    }

    /// Its period, in minutes.
    pub fn autosave_minutes(&self) -> u32 {
        self.object_manager_preferences.autosave_minutes()
    }

    /// Voir [`super::Action::SetAutosave`].
    pub(super) fn set_autosave(&mut self, on: bool) {
        self.autosave.deadline = None;
        if let Err(error) = self.object_manager_preferences.save_autosave(on) {
            self.fail(tr!(
                "error-preferences-not-saved",
                cause = errors::io::error_message(&error)
            ));
        }
    }

    /// See [`super::Action::SetAutosaveMinutes`]: the new period counts from
    /// now on.
    pub(super) fn set_autosave_minutes(&mut self, minutes: u32) {
        self.autosave.deadline = None;
        if let Err(error) = self
            .object_manager_preferences
            .save_autosave_minutes(minutes)
        {
            self.fail(tr!(
                "error-preferences-not-saved",
                cause = errors::io::error_message(&error)
            ));
        }
    }

    /// The revisions of the project as it is now.
    fn autosave_revisions(&self) -> (u64, u64) {
        (
            self.snapshot.revision.max(self.applied_revision),
            self.snapshot.geometry_revision,
        )
    }

    /// The project has just been saved, loaded or replaced: there is nothing
    /// more to copy before the next change.
    pub(super) fn autosave_mark_clean(&mut self) {
        self.autosave.mark = Some(self.autosave_revisions());
    }

    /// Deletes the copy of this process: the project is saved, or closed
    /// cleanly.
    pub(super) fn autosave_remove_slot(&mut self) {
        if let Some(slot) = self.autosave.slot.take() {
            slot.remove();
        }
    }

    /// To call at every frame (`logic()` of the root window) with the current
    /// time. Writes the copy if the deadline has fallen and the project has
    /// changed; returns the delay until the next deadline (to schedule the
    /// wake-up), `None` if the save is off.
    pub fn poll_autosave(&mut self, now: Instant) -> Option<Duration> {
        if !self.autosave_enabled() || self.session_lost {
            self.autosave.deadline = None;
            return None;
        }
        let period = Duration::from_secs(u64::from(self.autosave_minutes()) * 60);
        let deadline = *self.autosave.deadline.get_or_insert(now + period);
        if now < deadline {
            return Some(deadline - now);
        }
        let busy = self.snapshot.history.in_macro
            || self.script_running()
            || self.import_job.is_some()
            || self.recalc_since.is_some();
        if busy {
            self.autosave.deadline = Some(now + AUTOSAVE_RETRY);
            return Some(AUTOSAVE_RETRY);
        }
        self.autosave.deadline = Some(now + period);
        if self.autosave.mark != Some(self.autosave_revisions()) {
            match self.write_autosave() {
                Ok(()) => self.autosave.failed = false,
                Err(error) => {
                    if !self.autosave.failed {
                        let cause = match &error {
                            AutosaveError::Dir(io) => errors::io::error_message(io),
                            AutosaveError::Session(err) => errors::session::error_message(err),
                        };
                        self.warn(tr!("status-autosave-failed", cause = cause));
                    }
                    self.autosave.failed = true;
                }
            }
        }
        Some(period)
    }

    /// Writes the project into the slot of this process (renamed if the
    /// project changed path). Without a backup folder (tests): nothing.
    fn write_autosave(&mut self) -> Result<(), AutosaveError> {
        let Some(dir) = self.recovery_dir.clone() else {
            return Ok(());
        };
        let stem = recovery::stem_of(&self.path);
        if self
            .autosave
            .slot
            .as_ref()
            .is_some_and(|slot| slot.stem() != stem)
        {
            self.autosave_remove_slot();
        }
        let slot = match self.autosave.slot.take() {
            Some(slot) => slot,
            None => Slot::acquire(&dir, &stem, std::process::id()).map_err(AutosaveError::Dir)?,
        };
        slot.write_sidecar(&self.path);
        let result = self.handle.save(slot.path());
        self.autosave.slot = Some(slot);
        result.map_err(AutosaveError::Session)?;
        self.autosave_mark_clean();
        Ok(())
    }

    /// Lists the copies to offer (backup folder known).
    /// Called at startup by the root binary.
    pub fn scan_recovery(&mut self) {
        if let Some(dir) = &self.recovery_dir {
            self.autosave.found = recovery::scan(dir);
        }
    }

    /// The lines of the recovery window; `None`: nothing to offer, the window
    /// is closed.
    pub fn recovery_rows(&self) -> Option<Vec<RecoveryRow>> {
        if self.autosave.found.is_empty() {
            return None;
        }
        Some(
            self.autosave
                .found
                .iter()
                .map(|found| RecoveryRow {
                    path: found.stf.clone(),
                    text: Self::recovery_text(found),
                })
                .collect(),
        )
    }

    fn recovery_text(found: &Found) -> String {
        tr!(
            "recovery-row",
            name = found
                .name
                .clone()
                .unwrap_or_else(|| tr!("recovery-untitled")),
            date = recovery::format_utc(found.saved_at)
        )
    }

    /// The name of the copy whose deletion is waiting for confirmation.
    pub fn pending_recovery_delete(&self) -> Option<String> {
        let path = self.autosave.pending_delete.as_ref()?;
        let found = self.autosave.found.iter().find(|f| &f.stf == path)?;
        Some(
            found
                .name
                .clone()
                .unwrap_or_else(|| tr!("recovery-untitled")),
        )
    }

    fn recovery_found(&self, path: &PathBuf) -> Option<&Found> {
        self.autosave.found.iter().find(|f| &f.stf == path)
    }

    /// See [`super::Action::RecoveryOpen`]: the same path as File › Open…,
    /// hence the same "Save?" question if there is work.
    pub(super) fn recovery_open(&mut self, path: PathBuf) {
        if self.recovery_found(&path).is_some() {
            self.request_unsaved(super::UnsavedGesture::OpenRecovery(path), false);
        }
    }

    /// Opens the copy `path` in the current window (answer to the "Save?"
    /// question given, or nothing to lose).
    pub(super) fn open_recovery(&mut self, path: PathBuf) {
        let Some(found) = self.recovery_found(&path).cloned() else {
            return;
        };
        let origin = found
            .origin
            .as_ref()
            .map(|o| o.display().to_string())
            .unwrap_or_default();
        let previous = std::mem::replace(&mut self.path, found.stf.display().to_string());
        if !self.load() {
            self.path = previous;
            return;
        }
        // The project is that of the copy, under the path it comes from: unsaved
        // work (the original `.stf` does not have it).
        self.path = origin;
        self.saved_revision = None;
        self.discarded_source = None;
        self.autosave.mark = None;
        // Recopied into ours before erasing the opened copy: a crash right
        // after loses nothing.
        if self.write_autosave().is_ok() && self.recovery_dir.is_some() {
            // The two are never the same file (acquire rules out the old one).
            found.delete();
            self.autosave.found.retain(|f| f.stf != found.stf);
        }
        let text = tr!(
            "status-recovery-opened",
            name = found.name.unwrap_or_else(|| tr!("recovery-untitled"))
        );
        self.notice(text);
    }

    /// See [`super::Action::RecoveryIgnore`]: the copy stays on disk.
    pub(super) fn recovery_ignore(&mut self, path: &PathBuf) {
        self.autosave.found.retain(|f| &f.stf != path);
    }

    /// Voir [`super::Action::RecoveryDelete`] : demande confirmation.
    pub(super) fn recovery_request_delete(&mut self, path: PathBuf) {
        if self.recovery_found(&path).is_some() {
            self.autosave.pending_delete = Some(path);
        }
    }

    /// Voir [`super::Action::ConfirmRecoveryDelete`].
    pub(super) fn recovery_confirm_delete(&mut self, accept: bool) {
        let Some(path) = self.autosave.pending_delete.take() else {
            return;
        };
        if !accept {
            return;
        }
        if let Some(found) = self.recovery_found(&path) {
            found.delete();
        }
        self.autosave.found.retain(|f| f.stf != path);
    }

    /// See [`super::Action::CloseRecovery`]: each copy is ignored.
    pub(super) fn recovery_close(&mut self) {
        self.autosave.found.clear();
        self.autosave.pending_delete = None;
    }

    /// Is a deletion confirmation waiting? (blocking dialog)
    pub(super) fn recovery_delete_pending(&self) -> bool {
        self.autosave.pending_delete.is_some()
    }
}
