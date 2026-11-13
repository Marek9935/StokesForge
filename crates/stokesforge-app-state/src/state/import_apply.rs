//! Applying a finished import from the server thread (#475).
//!
//! The analysis of the files runs on its own thread since the beginning. What
//! was left to the paint pass was **creating** the sheets, which needs the
//! window to be painted: with every window hidden the import stayed finished
//! and unapplied. Now the server thread applies it, in three steps (the same
//! shape as the autosave):
//!
//! 1. [`AppState::import_apply_begin`], under the lock: takes the finished job out of
//!    the state and keeps only what the commands need (a copy of the snapshot,
//!    the selected sheets, the session handle). `import_applying` stays set;
//! 2. [`ImportRun::execute`], with the lock RELEASED: the ordinary
//!    `apply_import` (one macro "Import", plain undoable `Command`s through the
//!    session, invariant 1) runs on a **scratch** `AppState` over a handle with a
//!    sender identity of its own, as a script's. The macro belongs to that sender, so a write of the
//!    interface that arrives meanwhile is refused with `MacroOpen` (#21), never
//!    absorbed into the Import entry;
//! 3. [`AppState::import_apply_finish`], under the lock: takes back what the scratch
//!    state produced (messages, recalculation wait, recent files, last import),
//!    refreshes the snapshot and flags the new workbooks, which the next paint
//!    pass opens as windows (opening a window needs egui).
//!
//! Without the thread (a bench, a test, a thread that ended) the paint pass
//! keeps calling [`AppState::poll_import`], which applies the import in place.

use std::path::PathBuf;

use stokesforge_core::node::NodeKind;
use stokesforge_core::NodeId;
use stokesforge_importer::ImportItem;
use stokesforge_session::{SessionHandle, Snapshot};

use super::{origin_import, AppState, ImportDestination, StatusLevel};
use crate::errors;
use crate::preferences::Preferences;
use crate::tr;

/// A finished import whose commands are to run with the lock released.
pub struct ImportRun {
    handle: SessionHandle,
    snapshot: Snapshot,
    selected_sheets: std::collections::HashMap<NodeId, NodeId>,
    size_locks: std::collections::HashMap<NodeId, crate::frame_sizes::SizeLock>,
    seen_bound: u64,
    applied_revision: u64,
    destination: ImportDestination,
    dropped: Option<Option<NodeId>>,
    skipped: usize,
    items: Vec<ImportItem>,
    models: Vec<String>,
    for_window: Vec<PathBuf>,
    origin: Vec<origin_import::OriginRead>,
    /// Children of the root before the commands.
    books_before: usize,
    /// The scratch state, once the commands ran.
    done: Option<Box<AppState>>,
    /// The scratch state could not be built.
    start_error: Option<String>,
}

impl std::fmt::Debug for ImportRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImportRun")
            .field("items", &self.items.len())
            .finish_non_exhaustive()
    }
}

impl ImportRun {
    /// Runs the commands of the import. To be called with the lock RELEASED.
    pub fn execute(&mut self) {
        let scratch = AppState::new_with_preferences(
            self.handle.clone(),
            String::new(),
            Preferences::load_from(None),
        );
        let mut scratch = match scratch {
            Ok(scratch) => scratch,
            Err(error) => {
                self.start_error = Some(errors::session::error_message(&error));
                return;
            }
        };
        scratch.snapshot = self.snapshot.clone();
        scratch.selected_sheets = std::mem::take(&mut self.selected_sheets);
        scratch.size_locks = std::mem::take(&mut self.size_locks);
        scratch.seen_bound = self.seen_bound;
        scratch.applied_revision = self.applied_revision;
        let items = std::mem::take(&mut self.items);
        let (destination, dropped, skipped) =
            (self.destination, self.dropped.is_some(), self.skipped);
        let models = std::mem::take(&mut self.models);
        scratch.gesture(|state| state.import_batch(destination, dropped, skipped, items, &models));
        self.done = Some(Box::new(scratch));
    }
}

impl AppState {
    /// First step, under the lock: takes a finished import out of the state to
    /// apply it elsewhere. `None`: nothing to apply now, or nothing to create
    /// (then what the job leaves for the windows is done here, as in
    /// `poll_import`).
    pub(crate) fn import_apply_begin(&mut self) -> Option<ImportRun> {
        // The graph properties hold the root's history macro: the work is kept
        // and applied when they close.
        if self.import_applying || self.plot_settings.is_some() || self.session_lost {
            return None;
        }
        if !self
            .import_job
            .as_ref()
            .is_some_and(|job| job.ready.is_some())
        {
            return None;
        }
        let mut job = self.import_job.take()?;
        let mut reply = job.ready.take()?;
        let origin = std::mem::take(&mut reply.origin);
        if !Self::import_is_applied(&job, &reply, &origin) {
            let (dropped, for_window) = (job.dropped, reply.for_window);
            self.gesture(|state| state.import_leftovers(dropped, for_window, origin));
            return None;
        }
        let root = self.snapshot.tree.root();
        let books_before = self
            .snapshot
            .tree
            .node(root)
            .map_or(0, |node| node.children().len());
        self.import_applying = true;
        Some(ImportRun {
            // The sender of its own, as a script's (#21): a gesture of the
            // interface that arrives while the macro is open is refused
            // (MacroOpen), never absorbed into the Import entry.
            handle: self.handle.with_new_identity(),
            snapshot: self.snapshot.clone(),
            selected_sheets: self.selected_sheets.clone(),
            size_locks: self.size_locks.clone(),
            seen_bound: self.seen_bound,
            applied_revision: self.applied_revision,
            destination: job.destination,
            dropped: job.dropped,
            skipped: job.skipped,
            items: reply.items,
            models: reply.models,
            for_window: reply.for_window,
            origin,
            books_before,
            done: None,
            start_error: None,
        })
    }

    /// Last step, under the lock: takes back the result of the commands.
    /// Returns true: the windows must be woken.
    pub fn import_apply_finish(&mut self, run: ImportRun) -> bool {
        self.import_applying = false;
        let ImportRun {
            done,
            start_error,
            dropped,
            for_window,
            origin,
            books_before,
            ..
        } = run;
        let Some(scratch) = done else {
            let cause = start_error.unwrap_or_else(|| tr!("error-import-group"));
            self.gesture(|state| {
                state.fail(cause);
                state.import_leftovers(dropped, for_window, origin);
            });
            return true;
        };
        let mut scratch = *scratch;
        self.applied_revision = self.applied_revision.max(scratch.applied_revision);
        // The scratch state said the "formula now reads" warnings itself.
        self.seen_bound = scratch.seen_bound;
        if scratch.recalc_since.is_some() {
            self.recalc_since = scratch.recalc_since.take();
        }
        self.recent_to_record.append(&mut scratch.recent_to_record);
        if let Some(last) = scratch.last_import.take() {
            self.last_origin_import = None;
            self.last_import = Some(last);
        }
        self.refresh();
        let root = self.snapshot.tree.root();
        let created: Vec<NodeId> = self
            .snapshot
            .tree
            .node(root)
            .map(|node| node.children().iter().skip(books_before).copied().collect())
            .unwrap_or_default();
        for book in created {
            if self
                .snapshot
                .tree
                .node(book)
                .is_some_and(|node| matches!(node.kind(), NodeKind::Workbook { .. }))
            {
                self.books_to_open.push(book);
            }
        }
        // The messages of the commands, in one gesture as before.
        self.gesture(|state| {
            for message in &scratch.status_log {
                let text = message.text.clone();
                match message.level {
                    StatusLevel::Info => state.notice(text),
                    StatusLevel::Warning => state.warn(text),
                    StatusLevel::Error => state.fail(text),
                }
            }
            state.import_leftovers(dropped, for_window, origin);
        });
        true
    }

    /// The workbooks that a finished import created since the last call: the
    /// paint pass opens them as windows (once each).
    pub fn take_books_to_open(&mut self) -> Vec<NodeId> {
        std::mem::take(&mut self.books_to_open)
    }

    /// An import is being applied by the server thread, lock released.
    #[must_use]
    pub fn import_is_applying(&self) -> bool {
        self.import_applying
    }
}
