//! Actor that owns the StokesForge `Project` and serves it to its clients (the
//! UI, the Python script thread of `stokesforge-script`) through a request
//! channel.
//!
//! # Role
//!
//! This crate makes concrete an architecture decision of the foundation
//! (`docs/archive/rustrewrite/roadmap.md` §6): **the `Project` lives on its own
//! thread**. The UI and the script thread are two symmetric clients: they
//! send requests and receive immutable snapshots ([`Snapshot`]). The actor
//! holds the **only** `&mut Project` of the process, which turns the rule "no
//! mutation outside a command" into a property of ownership — checkable by
//! the compiler — rather than a convention to watch.
//!
//! # Protocol
//!
//! A **FIFO** request channel (`std::sync::mpsc`, deterministic order); each
//! request gets **one** reply, on a single-use channel created by the client
//! for that request. The UI keeps the last [`Snapshot`] received and paints
//! from it without a lock; a client knows whether its snapshot is stale by
//! comparing the `revision` (monotonic counter), never by comparing trees with
//! each other.
//!
//! Two requests go through `stokesforge-persistence` (frozen boundary,
//! provisional format — see the docs of that crate): [`SessionHandle::save`]
//! writes the current project without touching the revision or the history,
//! and [`SessionHandle::load`] **replaces** the current project with a new
//! project loaded from disk and **resets the history** (`undo`/`redo` become
//! empty again) — an old snapshot then has no relation left to the current
//! tree, including its node identifiers, which restart from the value
//! persisted in the file. `load` is refused
//! (`Err(SessionError::Command(CommandError::MacroOpen(_)))`) if a macro is
//! open; in that case, as for any `save`/`load` failure, neither the project
//! nor the history is modified.
//!
//! # Two invariants held by construction
//!
//! 1. **The actor never touches Python.** This crate depends on no Python
//!    crate, even though `stokesforge-script` exists and calls it: the type
//!    of a Python object cannot be named here, which makes a GIL ↔ actor
//!    deadlock structurally impossible. A whitelist of dependencies
//!    (`tests/no_ui_dependency.rs`) forces a conscious decision for every new
//!    dependency, rather than relying on a review.
//! 2. **No non-`Send` object crosses the channel.** Requests and replies are
//!    pure Rust, `Send + 'static` — checked by a static compile-time test
//!    (`assert_send`, in this module and in `actor`).
//!
//! # What an open macro does to the eight requests, and what the sender's identity changes
//!
//! F6 (`guide-de-correction-robustesse.md` §2, §6): everything a client knows
//! about the project is a potentially stale copy, so every assumption must
//! travel in the request and be checked by the actor at the moment of
//! applying it — never before. This table closes the question that the A11
//! stopgap (wave 1 of the campaign) had only brought to light without
//! answering it: *which of the eight requests must an open macro suspend, and
//! for whom?*
//!
//! Each [`SessionHandle`] now carries a stable identity (`ClientId`, private
//! to the crate — see its docs): two `.clone()`s of the same handle remain the
//! same sender (a script that clones its handle for a thread stays "itself");
//! two handles obtained from two **independent** calls to [`Session::handle`]
//! are two distinct senders, even in the same process.
//!
//! | Request | During an open macro, today | What the sender's identity changes |
//! |---|---|---|
//! | `Execute` | absorbed into the current macro, whatever the sender (A4: that was the bug — one client's work leaked into another's macro) | **refused** (`CommandError::ForeignMacro("execute")`) if the sender does not own the macro; absorbed normally otherwise |
//! | `Undo` | refused for **anyone** (`CommandError::MacroOpen("undo")`): undoing during a macro, one's own or another's, would leave an inconsistent state | unchanged — this refusal never depends on who opened the macro |
//! | `Redo` | refused for anyone (`CommandError::MacroOpen("redo")`), same reason as `Undo` | unchanged |
//! | `BeginMacro` | opens one more nesting level, whatever the sender, up to [`stokesforge_commands::MAX_MACRO_DEPTH`] | **refused** (`ForeignMacro("begin_macro")`) if a macro is already open by *another* sender; the same sender can always nest its own |
//! | `EndMacro` | closes the current level into one history entry, whatever the sender | **refused** (`ForeignMacro("end_macro")`) if the sender is not the owner — `CommandError::NotInMacro` stays reserved for the case where **no** macro is open |
//! | `AbortMacro` | undoes the current level and all its content, whatever the sender — **including another client's already validated work, silently**: that was exactly A4 | **refused** (`ForeignMacro("abort_macro")`) if the sender is not the owner: an `abort_macro` can no longer undo somebody else's work |
//! | `Save` | refused for anyone during an open macro (A11, **declared stopgap** in wave 1: the guard of `load_project` copied without more nuance) | unchanged — the refusal does not depend on who opened the macro; writing to disk the intermediate state of a macro that *somebody* could still abort stays forbidden to all |
//! | `Load` | refused for anyone during an open macro, before even touching the disk | unchanged, same reason as `Save` |
//! | `NewProject` | same guards as `Load`; replaces the identity, the history and the geometries | refused for anyone during a macro |
//!
//! `Snapshot` and `Shutdown` mutate nothing: `Snapshot` is a pure read,
//! always served; `Shutdown` does not go through this table but through the
//! finding that follows.
//!
//! # Window geometry
//!
//! Position and size of the UI windows (a workbook, a graph, a matrix, a note
//! — any node that carries a `WindowMeta`) are **not** a field of the model:
//! neither an addition to `WindowMeta`
//! (`crates/stokesforge-core/src/node.rs`), nor anything persistent in
//! `stokesforge-core`. Reason: moving or resizing a window must **never**
//! push an entry onto `History` — Ctrl+Z stays reserved for real data
//! mutations (invariant 1).
//!
//! The actor therefore keeps a `NodeId -> `[`WindowGeometry`]` table **beside**
//! the `Project` (see `actor::run`), mutated by
//! [`SessionHandle::set_window_geometry`]: a light request that goes neither
//! through `History` nor through the `revision` that goes with undo/redo. It
//! does, however, advance a **dedicated** counter,
//! `Snapshot::geometry_revision` — the question asked was: a rendering client
//! that knows the position of the windows only through the `Snapshot` still
//! needs to know that it changed in order to repaint, without it looking like
//! a model mutation that would invalidate a display cache (the
//! `(revision, rect, pixels_per_point, view_generation)` of
//! `stokesforge-render-egui`, see `docs/rendu.md`) that has no reason to be.
//! Separating the two counters closes that question: `revision` never moves
//! for a geometry, `geometry_revision` never moves for a model mutation.
//!
//! Deleting a node: its geometry entry becomes orphaned as soon as a command
//! (current or future) removes it from the tree. The actor purges it
//! automatically — `actor::purge_orphaned_geometries`, called after every
//! `Execute`/`Undo`/`Redo` that actually changed the tree — rather than
//! leaving it around: it is a risk-free cleanup of a plain unreferenced
//! `HashMap` entry, and keeping it would serve no purpose (reopening that same
//! `NodeId` one day makes no sense, identifiers are never reused —
//! invariant 7).
//!
//! **Column widths (#325).** A second `NodeId -> f32` table, beside the first,
//! with the same status (outside the `Project`, never in the history, mutated
//! by [`SessionHandle::set_column_width`], shares `geometry_revision`).
//! **Exception to the purge**: it is never purged on mutation. A column's
//! identifier is never reused (invariant 7) and Ctrl+Z resurrects it under the
//! same `NodeId`: it must find its width again there. Saving is what drops the
//! keys whose node no longer exists.
//!
//! Persisted all the same: [`SessionHandle::save`]/[`SessionHandle::load`] go
//! through `stokesforge_persistence::save_with_geometries`/
//! `load_with_geometries`, which write/read this block **separate from the
//! undoable tree** in the `.stf` file (`FileBody::window_geometries` and
//! `FileBody::column_widths`,
//! `#[serde(default)]` — see `docs/persistance.md`) — never mixed into the
//! `NodeRecord`s, never through `convert.rs`.
//!
//! **`Session::shutdown` during an open macro — finding not reproducible,
//! fixed.** A macro open when the actor stopped had no guard:
//! `Request::Shutdown` directly returned the current `Project`, macro
//! included — its mutations, already applied to the in-memory model (a macro
//! never prevents `Project` from being mutated, it only defers the history
//! entry), ended up **silently committed**, without ever having become an
//! undoable entry and without `end_macro` having been called. Same missing
//! guard as A11, but at exit rather than at save — and with no possible
//! reproduction (the bench cannot observe a `Project` that no longer exists
//! after the actor stops, see `guide-de-correction-robustesse.md` §7.3). The
//! chosen fix, trivial once A4 is in place: `actor::run` undoes (rather than
//! commits) every macro still open on receiving `Shutdown`, level by level
//! (`History::abort_macro`, without distinguishing owners — there is no client
//! left to answer) — a macro open at shutdown is now **aborted, not
//! committed**.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod actor;
mod error;
mod gauge;
mod recalc;
mod snapshot;

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use stokesforge_commands::{Command, MacroKind};
use stokesforge_core::{NodeId, Project};

use actor::Request;
use gauge::RecalcGauge;

/// Name of a new project: that of File > New, and that of the empty project
/// at startup (#333).
pub const NEW_PROJECT_NAME: &str = "Project1";

pub use error::{SessionError, SessionResult};
pub use gauge::RecalcEnd;
pub use recalc::FormulaRecalc;
pub use snapshot::{HistoryState, Revision, Snapshot};

/// Position and size of a UI window — see "Window geometry" above for what
/// it is and why it lives here rather than in the model. Directly reuses the
/// public type of `stokesforge-persistence`: the top-level doc of that type
/// explains why this is not a duplication (persistence is **below** this crate
/// in the dependency graph and so cannot name a type defined here).
pub use stokesforge_persistence::{WindowGeometry, WindowMode, WindowView};

/// What a `Load` dropped without failing (unreadable calculations, #278):
/// neutral data from `stokesforge-persistence`, put into sentences by the
/// application.
pub use stokesforge_persistence::{DiscardedCalculation, LoadReport};

/// Stable, comparable identity of a request sender (A4,
/// `guide-de-correction-robustesse.md` §F6).
///
/// A global atomic counter, incremented on every **independent** call to
/// [`Session::handle`] — never on `Clone`: two `.clone()`s of the same handle
/// share the same identity (a script that clones its handle to pass it to a
/// thread stays "the same sender" in the actor's eyes), whereas two handles
/// obtained from two distinct calls to `handle()` are two different senders,
/// even if they live in the same process. This identity is what lets the
/// actor refuse that a macro opened by one absorbs or is closed by the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ClientId(u64);

impl ClientId {
    /// Allocates a new identity, never reused over the life of the process
    /// (`AtomicU64`, `Ordering::Relaxed`: only uniqueness matters, not an
    /// order between threads).
    fn fresh() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        ClientId(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// Client handle to a [`Session`]: cloneable, sendable to other threads. Each
/// method sends a request on the actor's FIFO channel and **blocks** until the
/// reply. Returns `Err(SessionError::Disconnected)` if the actor has ended
/// (stopped via [`Session::shutdown`], or panicked).
#[derive(Debug, Clone)]
pub struct SessionHandle {
    tx: mpsc::Sender<Request>,
    /// Identity of this sender (A4): shared by all `.clone()`s of this
    /// handle, distinct from that of any handle obtained from another call to
    /// [`Session::handle`].
    client: ClientId,
    /// Gauge of the interruptible recalculation (#239), common to all handles.
    gauge: Arc<RecalcGauge>,
}

impl SessionHandle {
    /// Handle to the same session, but with a **new** sender identity (A4):
    /// unlike `.clone()`, it is not the same client. This is what any
    /// component that writes alongside a macro opened by another needs —
    /// detached window, script: the actor then refuses its writes
    /// (`ForeignMacro`) instead of absorbing them into the other's macro,
    /// where undoing that macro would take them away (issue #21).
    pub fn with_new_identity(&self) -> SessionHandle {
        SessionHandle {
            tx: self.tx.clone(),
            client: ClientId::fresh(),
            gauge: Arc::clone(&self.gauge),
        }
    }

    /// Sends a request built by `build` (which receives the sender of this
    /// call's single-use reply channel) and blocks until the reply.
    fn request<T>(&self, build: impl FnOnce(mpsc::Sender<T>) -> Request) -> SessionResult<T> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(build(reply_tx))
            .map_err(|_| SessionError::Disconnected)?;
        reply_rx.recv().map_err(|_| SessionError::Disconnected)
    }

    /// Executes a command. Returns the new revision, or the command bus's
    /// error — the tree and the history then stay unchanged. Refused
    /// (`Command(ForeignMacro("execute"))`) if a macro is open by another
    /// sender (A4, see the table at the top of the crate).
    pub fn execute(&self, command: Command) -> SessionResult<Revision> {
        let client = self.client;
        self.request(|reply| Request::Execute {
            command,
            client,
            reply,
            interruptible: false,
        })?
        .map_err(SessionError::from)
    }

    /// Like [`SessionHandle::execute`], but returns as soon as the command is
    /// applied (#239): the formula recalculation continues in the actor and
    /// can be stopped by [`SessionHandle::stop_recalc`]. The following
    /// requests wait, as always, for the end of the recalculation; to avoid
    /// blocking, see [`SessionHandle::recalc_running`]. The revision returned
    /// is the one before recalculation; the end of the recalculation adds one.
    pub fn execute_interruptible(&self, command: Command) -> SessionResult<Revision> {
        let client = self.client;
        self.request(|reply| Request::Execute {
            command,
            client,
            reply,
            interruptible: true,
        })?
        .map_err(SessionError::from)
    }

    /// Is the actor's thread still running? False after a stop, and above all
    /// after a panic: the model is then lost and any request would return
    /// `Disconnected`. Does not go through the actor: never blocks.
    pub fn actor_alive(&self) -> bool {
        self.gauge.actor_alive()
    }

    /// Number of the interruptible recalculation in progress, if there is
    /// one. Does not go through the actor: never blocks.
    pub fn recalc_running(&self) -> Option<u64> {
        self.gauge.running()
    }

    /// Waits at most `timeout` for the end of the recalculation in progress;
    /// true if there is (no longer) any.
    pub fn wait_recalc(&self, timeout: std::time::Duration) -> bool {
        self.gauge.wait_idle(timeout)
    }

    /// Requests the stop of recalculation `id` (#239): it stops between two
    /// columns, the command that triggered it is undone as by Ctrl+Z, then
    /// what had changed is recalculated. False if this recalculation is
    /// already finished. Never blocks.
    pub fn stop_recalc(&self, id: u64) -> bool {
        self.gauge.request_stop(id)
    }

    /// Collects the end of the last interruptible recalculation, only once.
    pub fn take_recalc_end(&self) -> Option<RecalcEnd> {
        self.gauge.take_finished()
    }

    /// Undoes the top entry. `Ok(None)` if the stack is empty.
    pub fn undo(&self) -> SessionResult<Option<Revision>> {
        self.request(|reply| Request::Undo { reply })?
            .map_err(SessionError::from)
    }

    /// Redoes the most recently undone entry. `Ok(None)` if there is nothing
    /// to redo.
    pub fn redo(&self) -> SessionResult<Option<Revision>> {
        self.request(|reply| Request::Redo { reply })?
            .map_err(SessionError::from)
    }

    /// Opens a macro of kind `kind` — see the table at the top of the crate.
    /// Refused (`Command(ForeignMacro("begin_macro"))`) if a macro is already
    /// open by *another* sender; the same sender can always nest its own, up
    /// to [`stokesforge_commands::MAX_MACRO_DEPTH`] levels
    /// (`Err(MacroTooDeep)` beyond).
    pub fn begin_macro(&self, kind: MacroKind) -> SessionResult<Revision> {
        let client = self.client;
        self.request(|reply| Request::BeginMacro {
            kind,
            client,
            reply,
        })?
        .map_err(SessionError::from)
    }

    /// Closes the current macro into a single history entry.
    /// `Err(Command(NotInMacro))` if no macro is open;
    /// `Err(Command(ForeignMacro("end_macro")))` if a macro is open but by
    /// another sender (A4, see the table at the top of the crate).
    pub fn end_macro(&self) -> SessionResult<Revision> {
        let client = self.client;
        self.request(|reply| Request::EndMacro { client, reply })?
            .map_err(SessionError::from)
    }

    /// Aborts the current macro and everything it had applied.
    /// `Err(Command(NotInMacro))` if no macro is open;
    /// `Err(Command(ForeignMacro("abort_macro")))` if a macro is open but by
    /// another sender — an `abort_macro` can no longer undo somebody else's
    /// work (A4, see the table at the top of the crate).
    pub fn abort_macro(&self) -> SessionResult<Revision> {
        let client = self.client;
        self.request(|reply| Request::AbortMacro { client, reply })?
            .map_err(SessionError::from)
    }

    /// Reruns analysis calculations (#181): the given `ids`, or — empty list
    /// — all those that are stale. Outside the history, like automatic
    /// recalculation; the revision advances. A calculation whose input has
    /// disappeared or whose operation fails keeps its last values and marks
    /// itself stale (`Staleness`).
    pub fn recalculate(&self, ids: &[NodeId]) -> SessionResult<Revision> {
        let client = self.client;
        let ids = ids.to_vec();
        self.request(|reply| Request::Recalculate { ids, client, reply })?
            .map_err(SessionError::from)
    }

    /// Requests a snapshot of the current state. Cheap: does not clone the
    /// tree.
    pub fn snapshot(&self) -> SessionResult<Snapshot> {
        self.request(|reply| Request::Snapshot { reply })
    }

    /// Sets (or replaces) the window geometry of `node` — see "Window
    /// geometry" at the top of the crate. Touches **neither** `History` **nor**
    /// `revision`: it is not a model mutation, there is nothing to undo.
    /// Returns the new value of [`Snapshot::geometry_revision`], the dedicated
    /// counter that lets a rendering client know that the geometry changed
    /// without confusing it with a model change. Always accepted, including
    /// during a macro opened by somebody else (it is neither `Execute`, nor
    /// `Undo`/`Redo`, nor `Save`/`Load`: nothing in the table at the top of
    /// the crate concerns it).
    pub fn set_window_geometry(
        &self,
        node: NodeId,
        geometry: WindowGeometry,
    ) -> SessionResult<u64> {
        self.request(|reply| Request::SetWindowGeometry {
            node,
            geometry,
            reply,
        })
    }

    /// Sets (or replaces) the width of column `column`, in logical points
    /// (#325). Same status as [`Self::set_window_geometry`]: neither `History`
    /// nor `revision`, nothing to undo; advances `geometry_revision`
    /// (rendering), unless the width is unchanged or invalid (not a finite
    /// real > 0), in which case it is ignored. Returns the current value of
    /// `geometry_revision`.
    pub fn set_column_width(&self, column: NodeId, width: f32) -> SessionResult<u64> {
        self.request(|reply| Request::SetColumnWidth {
            column,
            width,
            reply,
        })
    }

    /// Resets columns `columns` to the default width (#330): removes their key
    /// from the table. Same status as [`Self::set_column_width`]; advances
    /// `geometry_revision` only if a key was removed. Returns the current value
    /// of `geometry_revision`.
    pub fn reset_column_widths(&self, columns: &[NodeId]) -> SessionResult<u64> {
        self.request(|reply| Request::ResetColumnWidths {
            columns: columns.to_vec(),
            reply,
        })
    }

    /// Saves the current project to `path`. Does not change the revision:
    /// saving does not modify the model.
    pub fn save(&self, path: &Path) -> SessionResult<()> {
        self.request(|reply| Request::Save {
            path: path.to_path_buf(),
            reply,
        })?
    }

    /// Loads a complete project from `path` and substitutes it for the current
    /// project: the history is cleared, the revision advances. `Err(Command(
    /// MacroOpen))` if a macro is open — in that case, as for any failure,
    /// neither the project nor the history is touched.
    pub fn load(&self, path: &Path) -> SessionResult<Revision> {
        self.load_with_report(path)
            .map(|(revision, _report)| revision)
    }

    /// Like [`Self::load`], but also returns the [`LoadReport`] of what the
    /// read dropped without failing (unreadable calculations, #278), so that
    /// the caller reports it.
    pub fn load_with_report(&self, path: &Path) -> SessionResult<(Revision, LoadReport)> {
        self.load_with_limit(path, stokesforge_persistence::DEFAULT_MAX_FILE_BYTES)
    }

    /// Like [`Self::load_with_report`], with a file size cap (bytes, A2 #316):
    /// beyond it, `Err(Persistence(TooLarge))` and nothing is touched.
    pub fn load_with_limit(
        &self,
        path: &Path,
        max_bytes: u64,
    ) -> SessionResult<(Revision, LoadReport)> {
        self.load_with_limits(
            path,
            stokesforge_persistence::LoadLimits {
                max_bytes,
                ..Default::default()
            },
        )
    }

    /// Like [`Self::load_with_limit`], also with the global cell budget (B1,
    /// #316): beyond it, `Err(Persistence(TooManyCells))`, nothing is touched.
    pub fn load_with_limits(
        &self,
        path: &Path,
        limits: stokesforge_persistence::LoadLimits,
    ) -> SessionResult<(Revision, LoadReport)> {
        self.request(|reply| Request::Load {
            path: path.to_path_buf(),
            limits,
            reply,
        })?
    }

    /// Creates an empty project and replaces the current identity. Clears the
    /// history and the geometries; refuses an open macro, like [`Self::load`].
    pub fn new_project(&self) -> SessionResult<Revision> {
        self.request(|reply| Request::NewProject { reply })?
    }
}

/// The session: the actor's thread and a handle to it. Building a `Session`
/// starts the thread immediately.
#[derive(Debug)]
pub struct Session {
    handle: SessionHandle,
    thread: JoinHandle<Project>,
}

impl Session {
    /// Starts the actor with `project` and the given history limit
    /// (`None` → [`stokesforge_commands::DEFAULT_LIMIT`]). `Err` if the
    /// operating system refuses to create the thread.
    pub fn spawn(project: Project, history_limit: Option<usize>) -> std::io::Result<Self> {
        Self::spawn_with_recalc(project, history_limit, FormulaRecalc::Targeted)
    }

    /// Like [`Session::spawn`], choosing how the actor recalculates formula
    /// columns. [`FormulaRecalc::Full`] exists only as the oracle of the
    /// differential test (`stokesforge-robustesse`, battery `b22`): a
    /// production session takes [`Session::spawn`].
    pub fn spawn_with_recalc(
        project: Project,
        history_limit: Option<usize>,
        recalc: FormulaRecalc,
    ) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let gauge = Arc::new(RecalcGauge::default());
        let actor_gauge = Arc::clone(&gauge);
        let thread = thread::Builder::new()
            .name("stokesforge-session".to_string())
            .spawn(move || actor::run(project, history_limit, recalc, actor_gauge, rx))?;
        Ok(Self {
            handle: SessionHandle {
                tx,
                client: ClientId::fresh(),
                gauge,
            },
            thread,
        })
    }

    /// **New** client handle to this session: a sender distinct from any other
    /// call to `handle()` (A4, see the table at the top of the crate).
    /// `.clone()` on the returned handle stays the same sender; only a new call
    /// to `handle()` creates another.
    pub fn handle(&self) -> SessionHandle {
        SessionHandle {
            tx: self.handle.tx.clone(),
            client: ClientId::fresh(),
            gauge: Arc::clone(&self.handle.gauge),
        }
    }

    /// Requests the actor's stop and waits for its thread to end; returns the
    /// final `Project`. The other handles then receive `Err(Disconnected)` on
    /// their next call. `Err(Disconnected)` here if the actor's thread
    /// panicked.
    ///
    /// Dropping a `Session` (and all the handles derived from it) without
    /// calling `shutdown` also ends the thread: the actor's loop stops as soon
    /// as the last sender of the request channel is dropped
    /// (`for request in rx`, `tests/session.rs`).
    pub fn shutdown(self) -> SessionResult<Project> {
        // Send failure ignored: the actor may already have ended if all the
        // other handles were also dropped in the meantime.
        let _ = self.handle.tx.send(Request::Shutdown);
        self.thread.join().map_err(|_| SessionError::Disconnected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exists only so that the compiler checks the `Send` invariant — it is
    /// not a behavioral test.
    fn assert_send<T: Send>() {}

    #[test]
    fn public_types_are_send() {
        assert_send::<SessionHandle>();
        assert_send::<Snapshot>();
        assert_send::<Command>();
    }

    /// `A [1,2,3]` and an empty column `C`, to put a formula on.
    fn sheet_with_empty_c() -> (Session, NodeId, NodeId) {
        let mut project = Project::new("Demo").expect("project");
        let root = project.root();
        project.add_workbook(root, Some("Book1")).expect("workbook");
        let sheet = project.resolve_path("[Book1]Sheet1").expect("sheet");
        project
            .add_column(
                sheet,
                None,
                Some("A"),
                stokesforge_core::Column::from_f64(&[1.0, 2.0, 3.0]),
            )
            .expect("A");
        let c = project
            .add_column(
                sheet,
                None,
                Some("C"),
                stokesforge_core::Column::new(stokesforge_core::ColumnType::Numeric),
            )
            .expect("C");
        (Session::spawn(project, None).expect("actor"), sheet, c)
    }

    fn formula_c(handle: &SessionHandle, c: NodeId) -> Command {
        let formula = handle
            .snapshot()
            .expect("snapshot")
            .tree
            .formula_from_letters(c, "A*2")
            .expect("formula entered");
        Command::SetColumnMeta {
            column: c,
            meta: stokesforge_core::ColumnMeta {
                formula: Some(formula),
                ..Default::default()
            },
        }
    }

    /// #239: without Stop, the command is applied, the recalculation completes
    /// and the end is collected once; each step adds a revision.
    #[test]
    fn an_interruptible_recalculation_not_stopped_completes_normally() {
        let (session, _, c) = sheet_with_empty_c();
        let handle = session.handle();
        let before = handle.snapshot().expect("snapshot").revision;
        handle
            .execute_interruptible(formula_c(&handle, c))
            .expect("command");
        assert!(handle.wait_recalc(std::time::Duration::from_secs(5)));
        assert_eq!(handle.take_recalc_end(), Some(RecalcEnd::Done));
        assert_eq!(handle.take_recalc_end(), None, "collected only once");
        let snapshot = handle.snapshot().expect("snapshot");
        assert_eq!(
            snapshot.revision,
            before + 2,
            "command, then end of recalculation"
        );
        assert!(snapshot.history.can_undo);
        let column = snapshot.tree.node(c).and_then(|n| n.as_column()).unwrap();
        assert!(column.meta().formula.is_some());
    }

    /// #239: Stop undoes the command like Ctrl+Z (history intact: nothing more
    /// to redo than what an Undo would have left), and the end is `Stopped`.
    /// The stop is set in advance on the next recalculation, which makes it
    /// deterministic: the first stop point sees it.
    #[test]
    fn stop_undoes_the_command_that_triggered_the_recalculation() {
        let (session, _, c) = sheet_with_empty_c();
        let handle = session.handle();
        let first = handle.snapshot().expect("snapshot");
        handle.gauge.stop_next_for_test();
        handle
            .execute_interruptible(formula_c(&handle, c))
            .expect("command");
        assert!(handle.wait_recalc(std::time::Duration::from_secs(5)));
        assert_eq!(handle.take_recalc_end(), Some(RecalcEnd::Stopped));
        let snapshot = handle.snapshot().expect("snapshot");
        let column = snapshot.tree.node(c).and_then(|n| n.as_column()).unwrap();
        assert!(column.meta().formula.is_none(), "the formula is undone");
        assert!(!snapshot.history.can_undo, "the command left the stack");
        assert!(snapshot.history.can_redo, "as after a Ctrl+Z");
        assert!(snapshot.revision > first.revision);
    }

    /// Stop after an accepted but **no-op** command (B5: no entry stored)
    /// undoes nothing: without this, the actor's `undo` removed the formula
    /// set just before, unrelated to the interrupted gesture.
    #[test]
    fn stop_after_a_no_op_command_does_not_undo_the_previous_action() {
        for structural in [true, false] {
            let (session, sheet, c) = sheet_with_empty_c();
            let handle = session.handle();
            handle.execute(formula_c(&handle, c)).expect("formula");
            let command = if structural {
                // "Rename M to M".
                Command::Rename {
                    id: sheet,
                    name: "Sheet1".into(),
                }
            } else {
                // Insertion of zero rows: no target column.
                Command::InsertRows {
                    sheet,
                    at: 0,
                    count: 0,
                }
            };
            handle.gauge.stop_next_for_test();
            handle.execute_interruptible(command).expect("command");
            assert!(handle.wait_recalc(std::time::Duration::from_secs(5)));
            assert_eq!(handle.take_recalc_end(), Some(RecalcEnd::Done));
            let snapshot = handle.snapshot().expect("snapshot");
            let column = snapshot.tree.node(c).and_then(|n| n.as_column()).unwrap();
            assert!(column.meta().formula.is_some(), "the formula stays set");
            assert!(snapshot.history.can_undo, "the previous entry stays");
            assert!(!snapshot.history.can_redo, "nothing was undone");
        }
    }

    /// An actor panic is visible: `actor_alive` becomes false, and any request
    /// returns `Disconnected` (the model is lost).
    #[test]
    fn an_actor_panic_is_visible() {
        let (session, _, _) = sheet_with_empty_c();
        let handle = session.handle();
        assert!(handle.actor_alive());
        handle.tx.send(actor::Request::PanicForTest).expect("send");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while handle.actor_alive() {
            assert!(std::time::Instant::now() < deadline, "actor still alive");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(matches!(handle.snapshot(), Err(SessionError::Disconnected)));
    }

    /// A panic **during a recalculation** does not leave the "in progress"
    /// indicator forever: the gauge is released, `wait_recalc` returns true
    /// right away and `actor_alive` says why.
    #[test]
    fn a_panic_in_the_middle_of_a_recalculation_releases_the_gauge() {
        let (session, _, c) = sheet_with_empty_c();
        let handle = session.handle();
        handle.gauge.panic_in_next_recalc_for_test();
        handle
            .execute_interruptible(formula_c(&handle, c))
            .expect("the command is applied before the recalculation");
        assert!(handle.wait_recalc(std::time::Duration::from_secs(5)));
        assert_eq!(handle.recalc_running(), None);
        assert!(!handle.actor_alive());
    }

    /// A normal stop also counts as "gone": nothing left to wait for.
    #[test]
    fn after_shutdown_the_actor_is_no_longer_alive() {
        let (session, _, _) = sheet_with_empty_c();
        let handle = session.handle();
        session.shutdown().expect("shutdown");
        assert!(!handle.actor_alive());
    }

    /// A late Stop (the targeted recalculation is finished) does nothing — and
    /// above all does not stop the next one.
    #[test]
    fn a_late_stop_does_not_stop_the_next_recalculation() {
        let (session, _, c) = sheet_with_empty_c();
        let handle = session.handle();
        handle
            .execute_interruptible(formula_c(&handle, c))
            .expect("command");
        assert!(handle.wait_recalc(std::time::Duration::from_secs(5)));
        let finished = 1; // number of the first recalculation
        assert!(!handle.stop_recalc(finished), "already finished");
        handle
            .execute_interruptible(Command::AddFolder {
                parent: handle.snapshot().expect("snapshot").tree.root(),
                name: "Folder".into(),
            })
            .expect("command");
        assert!(handle.wait_recalc(std::time::Duration::from_secs(5)));
        assert_eq!(handle.take_recalc_end(), Some(RecalcEnd::Done));
    }
}
