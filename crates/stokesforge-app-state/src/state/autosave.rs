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
//! The clock is **injected**. Since #475 the autosave is followed by the server
//! thread of the application, in three steps: `autosave_decide(now)` under the
//! lock (called by `server_tick`), the blocking write (`AutosaveJob::run`) with
//! the lock RELEASED, then `finish_autosave` under the lock. The slot never
//! leaves `AppState`; the job carries the path and the generation of the slot
//! (`epoch`), and a result that arrives after the slot was removed (save, Save
//! As, New, Open, quit) is discarded; a slot removed during a write is deleted
//! when the write ends. `poll_autosave(now)` does the three steps on the calling
//! thread: it is the fallback of the paint pass when no thread runs, and what
//! the tests use. A deadline that falls during
//! a macro, a script, an import or a recalculation is pushed back by a few
//! seconds, without error: `handle.save` refuses during a macro (a refusal
//! `MacroOpen` counts as busy even when the root snapshot does not show the
//! macro yet, e.g. a dialog just opened), and a recalculation makes the
//! actor wait. The period restarts only after a write that succeeded or
//! really failed; the sidecar is written only after a successful save.
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

use stokesforge_commands::CommandError;
use stokesforge_session::SessionError;

use super::import_apply::ImportRun;
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
    /// The slot of this process, once taken. It never leaves `AppState`: a job
    /// in flight carries only the path (#475).
    slot: Option<Slot>,
    /// Generation of the slot: bumped when the slot is removed (save, Save As,
    /// New, Open, quit). A job result from an older generation is discarded.
    epoch: u64,
    /// A job is being written by the server thread, with the lock released.
    pub(super) in_flight: bool,
    /// The slot removed while a job was in flight: its file is deleted when
    /// the write ends, never before (the write would bring it back).
    orphan: Option<Slot>,
    /// The last write failed (the warning has already been given).
    failed: bool,
    /// Quit is accepted and waits for the write in flight; the status message
    /// has been said.
    pub(super) quit_waiting: bool,
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
    /// The actor refused the save because a macro is open.
    Busy,
    Dir(std::io::Error),
    Session(stokesforge_session::SessionError),
}

/// What the first third of an autosave decided.
pub(super) enum AutosaveStep {
    /// Nothing to write now; the delay until the next deadline (`None`: off).
    Wait(Option<Duration>),
    /// A copy is due: write it with the lock released.
    Job(AutosaveJob),
}

/// One copy to write (#475). It carries the path of the slot, never the slot:
/// the slot stays in [`AppState`]. `epoch` is the generation of the slot when
/// the job was taken (see [`AppState::finish_autosave`]).
#[derive(Debug)]
pub struct AutosaveJob {
    handle: stokesforge_session::SessionHandle,
    path: PathBuf,
    epoch: u64,
    revisions: (u64, u64),
    at: Instant,
    period: Duration,
}

impl AutosaveJob {
    /// The blocking write (`handle.save`): creates no history entry and never
    /// commits (invariant 1). To be called with the lock RELEASED.
    pub fn run(&self) -> Result<(), SessionError> {
        self.handle.save(&self.path)
    }
}

/// What one pass of the server thread found (#475).
#[derive(Debug)]
pub struct ServerTick {
    /// Something the windows show changed during this tick: they must be woken.
    pub changed: bool,
    /// How long the thread may sleep; `None`: until something wakes it.
    pub next: Option<Duration>,
    /// A copy to write, with the lock released, then `finish_autosave`.
    pub autosave: Option<AutosaveJob>,
    /// A finished import to apply, with the lock released, then `import_apply_finish`.
    pub import: Option<ImportRun>,
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
        self.autosave.epoch += 1;
        if let Some(slot) = self.autosave.slot.take() {
            if self.autosave.in_flight {
                // The server thread is writing this file right now: it is
                // removed by `finish_autosave`, once the write ended.
                self.autosave.orphan = Some(slot);
            } else {
                slot.remove();
            }
        }
    }

    /// The fallback of the server thread (#475): polls the autosave from the
    /// paint pass, with the current time, when no server thread runs (tests,
    /// a thread that ended). Writes the copy if the deadline has fallen and the
    /// project has changed, on the calling thread; returns the delay until the
    /// next deadline, `None` if the save is off.
    pub fn poll_autosave(&mut self, now: Instant) -> Option<Duration> {
        self.background(|state| match state.autosave_decide(now) {
            AutosaveStep::Wait(next) => next,
            AutosaveStep::Job(job) => {
                let result = job.run();
                state.finish_autosave_pass(&job, result);
                state
                    .autosave
                    .deadline
                    .map(|deadline| deadline.saturating_duration_since(job.at))
            }
        })
    }

    /// First third of an autosave (#475), under the lock: decides whether a
    /// copy is due. Pure with respect to the clock (`now` is injected).
    pub(super) fn autosave_decide(&mut self, now: Instant) -> AutosaveStep {
        if !self.autosave_enabled() || self.session_lost {
            self.autosave.deadline = None;
            return AutosaveStep::Wait(None);
        }
        let period = Duration::from_secs(u64::from(self.autosave_minutes()) * 60);
        let deadline = *self.autosave.deadline.get_or_insert(now + period);
        if now < deadline {
            return AutosaveStep::Wait(Some(deadline - now));
        }
        let busy = self.snapshot.history.in_macro
            || self.script_running()
            || self.import_job.is_some()
            || self.import_applying
            || self.recalc_since.is_some()
            || self.autosave.in_flight;
        if busy {
            self.autosave.deadline = Some(now + AUTOSAVE_RETRY);
            return AutosaveStep::Wait(Some(AUTOSAVE_RETRY));
        }
        if self.autosave.mark != Some(self.autosave_revisions()) {
            match self.prepare_autosave(now, period) {
                Ok(Some(job)) => {
                    self.autosave.in_flight = true;
                    return AutosaveStep::Job(job);
                }
                // No backup folder (tests): nothing to write.
                Ok(None) => self.autosave.failed = false,
                Err(error) => self.autosave_failure(&error),
            }
        }
        self.autosave.deadline = Some(now + period);
        AutosaveStep::Wait(Some(period))
    }

    /// Reports a write failure: a single warning as long as it lasts.
    fn autosave_failure(&mut self, error: &AutosaveError) {
        if !self.autosave.failed {
            let cause = match error {
                AutosaveError::Dir(io) => errors::io::error_message(io),
                AutosaveError::Session(err) => errors::session::error_message(err),
                AutosaveError::Busy => unreachable!("a retry, not a failure"),
            };
            self.warn(tr!("status-autosave-failed", cause = cause));
        }
        self.autosave.failed = true;
    }

    /// Takes (or renames) the slot and describes the write. Without a backup
    /// folder (tests): nothing.
    fn prepare_autosave(
        &mut self,
        now: Instant,
        period: Duration,
    ) -> Result<Option<AutosaveJob>, AutosaveError> {
        let Some(dir) = self.recovery_dir.clone() else {
            return Ok(None);
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
        let path = match &self.autosave.slot {
            Some(slot) => slot.path().to_path_buf(),
            None => {
                let slot =
                    Slot::acquire(&dir, &stem, std::process::id()).map_err(AutosaveError::Dir)?;
                let path = slot.path().to_path_buf();
                self.autosave.slot = Some(slot);
                path
            }
        };
        Ok(Some(AutosaveJob {
            handle: self.handle.clone(),
            path,
            epoch: self.autosave.epoch,
            revisions: self.autosave_revisions(),
            at: now,
            period,
        }))
    }

    /// Last third of an autosave (#475), under the lock: records the outcome
    /// of the write that the server thread did with the lock released. A job
    /// whose slot was removed meanwhile (save, Save As, New, Open, quit) is
    /// discarded: nothing marked, no warning, the new slot untouched.
    /// Returns true if something visible changed (a message).
    pub fn finish_autosave(&mut self, job: &AutosaveJob, result: Result<(), SessionError>) -> bool {
        let before = self.recorded_count();
        self.background(|state| state.finish_autosave_pass(job, result));
        // A quit that waited for this write: wake the window so that it closes.
        self.recorded_count() != before || self.autosave.quit_waiting
    }

    fn finish_autosave_pass(&mut self, job: &AutosaveJob, result: Result<(), SessionError>) {
        self.autosave.in_flight = false;
        if let Some(orphan) = self.autosave.orphan.take() {
            orphan.remove();
        }
        if job.epoch != self.autosave.epoch {
            return;
        }
        let outcome = match result {
            Err(SessionError::Command(CommandError::MacroOpen(_))) => Err(AutosaveError::Busy),
            Err(error) => Err(AutosaveError::Session(error)),
            Ok(()) => Ok(()),
        };
        match outcome {
            Ok(()) => {
                if let Some(slot) = &self.autosave.slot {
                    slot.write_sidecar(&self.path);
                }
                self.autosave.mark = Some(job.revisions);
                self.autosave.failed = false;
            }
            Err(AutosaveError::Busy) => {
                // An action is still open (the root snapshot may not show it
                // yet): wait silently, `failed` untouched.
                self.autosave.deadline = Some(job.at + AUTOSAVE_RETRY);
                return;
            }
            Err(error) => self.autosave_failure(&error),
        }
        self.autosave.deadline = Some(job.at + job.period);
    }

    /// A synchronous write (recovery open): decide, write, finish.
    fn write_autosave(&mut self) -> Result<(), AutosaveError> {
        if self.autosave.in_flight {
            return Err(AutosaveError::Busy);
        }
        let Some(job) = self.prepare_autosave(Instant::now(), Duration::ZERO)? else {
            return Ok(());
        };
        let result = job.run();
        if let Some(slot) = &self.autosave.slot {
            if result.is_ok() {
                slot.write_sidecar(&self.path);
            }
        }
        match result {
            Err(SessionError::Command(CommandError::MacroOpen(_))) => Err(AutosaveError::Busy),
            Err(error) => Err(AutosaveError::Session(error)),
            Ok(()) => {
                self.autosave_mark_clean();
                Ok(())
            }
        }
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

    /// Is the recovery list showing (some recovery copy was found)?
    pub(super) fn recovery_list_open(&self) -> bool {
        !self.autosave.found.is_empty()
    }

    /// Is a deletion confirmation waiting? (blocking dialog)
    pub(super) fn recovery_delete_pending(&self) -> bool {
        self.autosave.pending_delete.is_some()
    }
}
