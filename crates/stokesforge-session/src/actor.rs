//! The actor's loop: the only place in the process that holds a
//! `&mut Project`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use stokesforge_commands::{Command, CommandError, CommandResult, History, MacroKind};
use stokesforge_core::{NodeId, NodeKind, Project};

use crate::error::{SessionError, SessionResult};
use crate::gauge::{RecalcEnd, RecalcGauge};
use crate::recalc::{FormulaRecalc, Recalculator};
use crate::snapshot::{HistoryState, Revision, Snapshot};
use crate::{ClientId, LoadReport, WindowGeometry};

/// Request sent to the actor by a [`crate::SessionHandle`]. Private to the
/// crate: clients never build a `Request` themselves, they go through the
/// methods of `SessionHandle`, each of which carries its own single-use reply
/// channel.
// `Execute` carries a whole `Command` (column and metadata included), much
// bigger than the other requests: the message is consumed at once by the
// actor, so boxing it would only add one allocation per command.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Request {
    /// Executes a command. Replies with the new revision, or the error
    /// returned by `History::execute` — the tree and the history then stay
    /// unchanged. Refused (`ForeignMacro`) if a macro is open by a sender
    /// other than `client` (A4, see the table at the top of the crate).
    Execute {
        /// Command to apply.
        command: Command,
        /// Sender of the request.
        client: ClientId,
        /// Reply: new revision, or error.
        reply: Sender<CommandResult<Revision>>,
        /// True (#239): reply as soon as the command is applied, then
        /// recalculate the formulas, able to be stopped by the
        /// [`RecalcGauge`]. The revision returned is then the one before
        /// recalculation; one more revision follows when it is finished.
        interruptible: bool,
    },
    /// Undoes the top entry. `Ok(None)` if the stack is empty.
    Undo {
        /// Reply: new revision if something was undone.
        reply: Sender<CommandResult<Option<Revision>>>,
    },
    /// Redoes the most recently undone entry. `Ok(None)` if there is nothing to redo.
    Redo {
        /// Reply: new revision if something was redone.
        reply: Sender<CommandResult<Option<Revision>>>,
    },
    /// Opens a macro. `Err(MacroTooDeep)` beyond `History::MAX_MACRO_DEPTH`
    /// nesting levels (B7, `guide-de-correction-robustesse.md` §F4).
    /// `Err(ForeignMacro)` if a macro is already open by a sender other than
    /// `client` (A4) — the same sender can always nest its own.
    BeginMacro {
        /// Kind of the group.
        kind: MacroKind,
        /// Sender of the request.
        client: ClientId,
        /// Reply: new revision (the history state changed), or the error of
        /// `History::begin_macro`.
        reply: Sender<CommandResult<Revision>>,
    },
    /// Closes the current macro. `Err(ForeignMacro)` if the sender is not the
    /// one that opened it (A4).
    EndMacro {
        /// Sender of the request.
        client: ClientId,
        /// Reply: new revision, or error (`NotInMacro`, `ForeignMacro`).
        reply: Sender<CommandResult<Revision>>,
    },
    /// Aborts the current macro and everything it had applied.
    /// `Err(ForeignMacro)` if the sender is not the one that opened it — this
    /// is exactly the refusal that closes A4: an `abort_macro` can no longer
    /// undo another client's validated work.
    AbortMacro {
        /// Sender of the request.
        client: ClientId,
        /// Reply: new revision, or error (`NotInMacro`, `ForeignMacro`).
        reply: Sender<CommandResult<Revision>>,
    },
    /// Requests a snapshot of the current state.
    Snapshot {
        /// Reply: the requested snapshot.
        reply: Sender<Snapshot>,
    },
    /// Sets (or replaces) the window geometry of a node. Touches neither
    /// `History` nor `revision` (see the crate-level docs): it is not a model
    /// mutation, so not an undoable entry. Always accepted, including for a
    /// `NodeId` that does not (or no longer) exist in the current tree — the
    /// entry would be purged anyway at the next mutation that notices its
    /// absence (see `run`); validating here would bring nothing but one more
    /// round trip.
    SetWindowGeometry {
        /// Node whose window moved or changed size.
        node: NodeId,
        /// New geometry.
        geometry: WindowGeometry,
        /// Reply: new value of `geometry_revision` (see the crate-level docs
        /// — a counter distinct from `revision`, so that moving a window does
        /// not make a rendering client think the model changed).
        reply: Sender<u64>,
    },
    /// Sets (or replaces) the width of column `column` (#325). Same status as
    /// `SetWindowGeometry`: neither `History` nor `revision`, not a model
    /// mutation, so nothing to undo. A width that is not a finite, strictly
    /// positive real is **ignored** (invariant 10): the reply then returns the
    /// unchanged value of `geometry_revision`. An absent `NodeId` is accepted,
    /// as for geometry: the table is **not** purged on mutation (a column
    /// deleted then restored by Ctrl+Z finds its width again, invariant 7);
    /// saving filters the dead keys.
    SetColumnWidth {
        /// Column that was resized.
        column: NodeId,
        /// New width, in logical points.
        width: f32,
        /// Reply: value of `geometry_revision`, which also serves the widths
        /// (a single "view" counter).
        reply: Sender<u64>,
    },
    /// Resets columns `columns` to the default width (#330): removes their key
    /// from the table (the default is an application constant, the session
    /// writes no value). Same status as `SetColumnWidth`: neither `History`
    /// nor `revision`. `geometry_revision` advances only if at least one key
    /// was actually removed; a `NodeId` without a width is ignored.
    ResetColumnWidths {
        /// Columns to reset to the default.
        columns: Vec<NodeId>,
        /// Reply: value of `geometry_revision`.
        reply: Sender<u64>,
    },
    /// Saves the current project to `path`. Does not change the revision:
    /// saving does not modify the model.
    Save {
        /// Destination path.
        path: PathBuf,
        /// Reply: success, or persistence error.
        reply: Sender<SessionResult<()>>,
    },
    /// Loads a complete project from `path` and substitutes it for the
    /// current project. Clears the history. Refused if a macro is open.
    Load {
        /// Source path.
        path: PathBuf,
        /// Read caps: size (A2) and cells (B1), #316.
        limits: stokesforge_persistence::LoadLimits,
        /// Reply: new revision, or error (persistence, or
        /// `Command(MacroOpen)` if a macro is open).
        reply: Sender<SessionResult<(Revision, LoadReport)>>,
    },
    /// Replaces the project with an empty project, with the same guards as Load.
    NewProject {
        reply: Sender<SessionResult<Revision>>,
    },
    /// Reruns analysis calculations (#181), outside the history: empty `ids`
    /// for all stale calculations. Refused (`ForeignMacro`) if a macro is open
    /// by another sender, like any write.
    Recalculate {
        ids: Vec<NodeId>,
        client: ClientId,
        reply: Sender<CommandResult<Revision>>,
    },
    /// Requests the actor's stop.
    Shutdown,
    /// Tests only: makes the actor panic, to exercise the visible death.
    #[cfg(test)]
    PanicForTest,
}

/// Tells the gauge that the actor left its loop. Its `Drop` also runs during a
/// panic unwind: this is what makes the death visible
/// (`SessionHandle::actor_alive`) and frees a recalculation interrupted
/// mid-flight.
struct ActorExit(Arc<RecalcGauge>);

impl Drop for ActorExit {
    fn drop(&mut self) {
        self.0.abandon();
    }
}

/// The actor's loop: serves the requests in the FIFO order in which they
/// arrive on `rx`, until all the senders are dropped or a `Shutdown` arrives.
/// Returns the final `Project`, so that the caller (`Session::shutdown`) can
/// recover it.
pub(crate) fn run(
    mut project: Project,
    history_limit: Option<usize>,
    recalc: FormulaRecalc,
    gauge: Arc<RecalcGauge>,
    rx: Receiver<Request>,
) -> Project {
    let _exit = ActorExit(Arc::clone(&gauge));
    let mut history = match history_limit {
        Some(limit) => History::with_limit(limit),
        None => History::new(),
    };
    let mut revision: Revision = 0;
    // A3 (`guide-de-correction-robustesse.md` §F2/F6) — epoch of the current
    // project: local to this thread, **never persisted** (see the docs of
    // `Snapshot::epoch`). Advances on a successful `Load` or `NewProject`:
    // neither `Execute`/`Undo`/`Redo`, which already advance `revision`, nor
    // macros change the project's identity.
    let mut epoch: u64 = 0;
    // A4 (`guide-de-correction-robustesse.md` §F6) — sender that opened the
    // current macro, `None` outside a macro. Reset to `None` as soon as
    // `history.in_macro()` becomes false again, whether the closing succeeded
    // or not (see the `EndMacro`/`AbortMacro` branches: `History::end_macro`/
    // `abort_macro` may have popped a level before an error comes back).
    let mut macro_owner: Option<ClientId> = None;
    // `NodeId -> WindowGeometry` table, beside the `Project` (see the
    // crate-level docs): mutated only by `SetWindowGeometry`, never by a
    // command. `geometry_revision` is a counter **distinct** from `revision`
    // — see the crate-level docs for the decision and the reason.
    let mut geometries: HashMap<NodeId, WindowGeometry> = HashMap::new();
    let mut geometry_revision: u64 = 0;
    // Column widths (#325), by `NodeId`: same status as the geometries —
    // outside the `Project`, never in the history, shares `geometry_revision`
    // —, but **never purged** on mutation: the width of a deleted column must
    // come back when Ctrl+Z restores it (invariant 7). Dead keys drop at save.
    let mut column_widths: HashMap<NodeId, f32> = HashMap::new();
    // #193 — what the formula recalculation saw on its last pass (see
    // `recalc`): beside the `Project`, like the geometries, never in the
    // history nor in the `.stf`. The project received at creation was never
    // recalculated by this actor: the empty memo will do it entirely at the
    // first mutating request, as before #193.
    let mut formulas = Recalculator::new(recalc);

    for request in rx {
        match request {
            Request::Execute {
                command,
                client,
                reply,
                interruptible,
            } => {
                // A4 — a macro opened by another sender no longer silently
                // absorbs that sender's work: that was the bug.
                if history.in_macro() && macro_owner != Some(client) {
                    let _ = reply.send(Err(CommandError::ForeignMacro("execute")));
                    continue;
                }
                let recorded = match history.execute_recorded(&mut project, command) {
                    Ok(recorded) => recorded,
                    Err(err) => {
                        let _ = reply.send(Err(err));
                        continue;
                    }
                };
                revision += 1;
                if interruptible {
                    // #239 — the command is applied: reply without waiting
                    // for the recalculation, which Stop can interrupt.
                    let id = gauge.begin();
                    let _ = reply.send(Ok(revision));
                    let stop_gauge = Arc::clone(&gauge);
                    let completed = formulas
                        .run_interruptible(&mut project, move || stop_gauge.stop_requested(id));
                    let mut end = RecalcEnd::Done;
                    if !completed {
                        // Stop: undo the command like Ctrl+Z, then
                        // recalculate what changed (the interrupted pass may
                        // have rewritten columns). Inside a macro the undo is
                        // refused: the command stays, the recalculation
                        // completes. A no-op command stored no entry: an
                        // `undo` would then undo the previous action,
                        // unrelated — only what was just stored is undone.
                        if recorded && matches!(history.undo(&mut project), Ok(true)) {
                            end = RecalcEnd::Stopped;
                        }
                        formulas.run(&mut project);
                    }
                    if purge_orphaned_geometries(&project, &mut geometries) {
                        geometry_revision += 1;
                    }
                    // The formulas changed under the same revision: a client
                    // that compares revisions (rendering cache) must see that
                    // there is something new.
                    revision += 1;
                    gauge.finish(id, end);
                } else {
                    formulas.run(&mut project);
                    if purge_orphaned_geometries(&project, &mut geometries) {
                        geometry_revision += 1;
                    }
                    let _ = reply.send(Ok(revision));
                }
            }
            Request::Undo { reply } => {
                let outcome = history.undo(&mut project).map(|reverted| {
                    reverted.then(|| {
                        revision += 1;
                        formulas.run(&mut project);
                        if purge_orphaned_geometries(&project, &mut geometries) {
                            geometry_revision += 1;
                        }
                        revision
                    })
                });
                let _ = reply.send(outcome);
            }
            Request::Redo { reply } => {
                let outcome = history.redo(&mut project).map(|restored| {
                    restored.then(|| {
                        revision += 1;
                        formulas.run(&mut project);
                        if purge_orphaned_geometries(&project, &mut geometries) {
                            geometry_revision += 1;
                        }
                        revision
                    })
                });
                let _ = reply.send(outcome);
            }
            Request::BeginMacro {
                kind,
                client,
                reply,
            } => {
                // A4 — a macro already open by another sender refuses to be
                // nested into; the same sender always can (up to
                // `MAX_MACRO_DEPTH`, B7 below).
                let outcome = if history.in_macro() && macro_owner != Some(client) {
                    Err(CommandError::ForeignMacro("begin_macro"))
                } else {
                    // B7 (`guide-de-correction-robustesse.md` §F4) —
                    // `begin_macro` can refuse (`MacroTooDeep`): the revision
                    // advances only if the nesting actually changed state. A
                    // success, on the other hand, advances the revision even
                    // without a model mutation (the history state, `in_macro`,
                    // changed).
                    let result = history.begin_macro(kind);
                    if result.is_ok() {
                        macro_owner = Some(client);
                    }
                    result.map(|()| {
                        revision += 1;
                        revision
                    })
                };
                let _ = reply.send(outcome);
            }
            Request::EndMacro { client, reply } => {
                // A4 — `NotInMacro` stays reserved for the case where no
                // macro is open; `ForeignMacro` covers the case where a macro
                // IS open, but not by `client`.
                let outcome = if history.in_macro() && macro_owner != Some(client) {
                    Err(CommandError::ForeignMacro("end_macro"))
                } else {
                    let result = history.end_macro(&mut project);
                    if !history.in_macro() {
                        macro_owner = None;
                    }
                    result.map(|()| {
                        revision += 1;
                        revision
                    })
                };
                let _ = reply.send(outcome);
            }
            Request::AbortMacro { client, reply } => {
                // A4 — this is where the bug occurred: `abort_macro` undid
                // another client's work without refusing.
                let outcome = if history.in_macro() && macro_owner != Some(client) {
                    Err(CommandError::ForeignMacro("abort_macro"))
                } else {
                    let result = history.abort_macro(&mut project);
                    if !history.in_macro() {
                        macro_owner = None;
                    }
                    result.map(|()| {
                        revision += 1;
                        // An `abort_macro` amounts to replaying, in one
                        // block, the undo of each command absorbed into the
                        // macro — exactly what a series of `Undo`s would do
                        // (which recalculate afterwards): the formula columns
                        // touched by the undone work must be recalculated
                        // here too, otherwise they would stay on values that
                        // no formula currently in force justifies any more.
                        formulas.run(&mut project);
                        revision
                    })
                };
                let _ = reply.send(outcome);
            }
            Request::Recalculate { ids, client, reply } => {
                let outcome = if history.in_macro() && macro_owner != Some(client) {
                    Err(CommandError::ForeignMacro("recalculate"))
                } else {
                    formulas.recalculate(&mut project, &ids);
                    revision += 1;
                    Ok(revision)
                };
                let _ = reply.send(outcome);
            }
            Request::Snapshot { reply } => {
                let snapshot = build_snapshot(
                    &project,
                    &history,
                    revision,
                    epoch,
                    &geometries,
                    &column_widths,
                    geometry_revision,
                );
                let _ = reply.send(snapshot);
            }
            Request::SetWindowGeometry {
                node,
                geometry,
                reply,
            } => {
                // Touches neither `history` nor `revision`: see the
                // crate-level docs — it is not a model mutation. Always
                // accepted (see the docs of `Request::SetWindowGeometry`).
                geometries.insert(node, geometry);
                geometry_revision += 1;
                let _ = reply.send(geometry_revision);
            }
            Request::SetColumnWidth {
                column,
                width,
                reply,
            } => {
                if stokesforge_persistence::is_valid_column_width(width)
                    && column_widths.get(&column) != Some(&width)
                {
                    column_widths.insert(column, width);
                    geometry_revision += 1;
                }
                let _ = reply.send(geometry_revision);
            }
            Request::ResetColumnWidths { columns, reply } => {
                let mut removed = false;
                for column in columns {
                    removed |= column_widths.remove(&column).is_some();
                }
                if removed {
                    geometry_revision += 1;
                }
                let _ = reply.send(geometry_revision);
            }
            Request::Save { path, reply } => {
                // A11 — declared stopgap (guide-de-correction-robustesse.md
                // §F6): a copy of the guard of `load_project`, not an answer
                // to "which of the eight requests must an open macro
                // suspend?". Without it, saving during a macro writes to disk
                // a state that a later `abort_macro` will undo in memory — the
                // file then keeps what the undo erased. Touches neither the
                // project nor the history: the revision does not advance,
                // saving does not change the model. Refused for **anyone**
                // during a macro, not only its owner (see the table at the top
                // of the crate): the intermediate state of a macro that
                // anyone could still abort must not reach the disk.
                //
                // `save_with_geometries` (not `save`): the window geometry
                // must survive a reload (see the crate-level docs) — written
                // in the separate block that `stokesforge-persistence`
                // reserves for it, never mixed into the undoable tree.
                let outcome = if history.in_macro() {
                    Err(SessionError::Command(CommandError::MacroOpen("save")))
                } else {
                    let view = stokesforge_persistence::ViewTables {
                        window_geometries: geometries.clone(),
                        column_widths: column_widths.clone(),
                    };
                    stokesforge_persistence::save_with_geometries(&project, &view, &path)
                        .map_err(SessionError::from)
                };
                let _ = reply.send(outcome);
            }
            Request::Load {
                path,
                limits,
                reply,
            } => {
                let outcome = load_project(
                    &mut project,
                    &mut history,
                    &mut geometries,
                    &mut column_widths,
                    &path,
                    limits,
                )
                .map(|report| {
                    revision += 1;
                    // The values of a formula column are written to disk
                    // (`stokesforge-persistence`, see its docs) but treated as
                    // a cache that is not trusted blindly: a file may have
                    // been modified by another process, an earlier version of
                    // StokesForge, or carry columns whose order changed
                    // without the values following.
                    // Recalculating the formulas here is the chosen option
                    // (cheap — see `recalc_formulas`). The memo is cleared:
                    // it describes a project that no longer exists.
                    //
                    // A1 (#316): the **analysis calculations** (fit,
                    // deconvolution…) are NOT rerun — a third party's file can
                    // demand thousands of iterations, not interruptible. The
                    // file's values are kept; a calculation whose inputs
                    // changed outside StokesForge is already marked stale by
                    // persistence, and is rerun on demand.
                    formulas.forget();
                    formulas.run_after_load(&mut project);
                    // A3 — the project's identity changes: a handle
                    // (`stokesforge_script::surface::Handle`) that memorized
                    // the old epoch must no longer be able to name a `NodeId`
                    // of the new project without being detected. A `Load` also
                    // clears the history (`load_project`), so there is no open
                    // macro at this point: no need to reset `macro_owner` to
                    // `None` here, `load_project` already refused if a macro
                    // was open.
                    epoch += 1;
                    // The geometry table is entirely replaced by the loaded
                    // file's (`load_project`, below): a rendering client must
                    // know it even if its content, by coincidence, looked like
                    // the old one.
                    geometry_revision += 1;
                    (revision, report)
                });
                let _ = reply.send(outcome);
            }
            Request::NewProject { reply } => {
                // Lifecycle, like Load: no undoable entry can survive the
                // replacement of the project's identity.
                let outcome = if history.in_macro() {
                    Err(SessionError::Command(CommandError::MacroOpen(
                        "new project",
                    )))
                } else {
                    Project::new(crate::NEW_PROJECT_NAME)
                        .map_err(CommandError::from)
                        .map_err(SessionError::from)
                        .map(|empty| {
                            project = empty;
                            let _ = history.clear();
                            geometries.clear();
                            column_widths.clear();
                            formulas.forget();
                            revision += 1;
                            epoch += 1;
                            geometry_revision += 1;
                            revision
                        })
                };
                let _ = reply.send(outcome);
            }
            #[cfg(test)]
            Request::PanicForTest => panic!("actor test panic"),
            Request::Shutdown => {
                // Non-reproducible finding (guide-de-correction-robustesse.md
                // §7.3, table at the top of `stokesforge-session`): a macro
                // still open at shutdown must not be committed silently. It is
                // aborted, level by level, as if its owner had abandoned it
                // before leaving — there is no client left to answer, so no
                // owner check here (`abort_macro` takes `&mut project`
                // directly, outside the `Request` protocol). An error during
                // the unwinding (case already documented by
                // `History::abort_macro`: "the tree may then be half undone")
                // must not turn this loop into an infinite loop: it stops the
                // unwinding rather than ignoring it.
                while history.in_macro() {
                    if history.abort_macro(&mut project).is_err() {
                        break;
                    }
                }
                break;
            }
        }
    }

    project
}

/// Loads a complete project from `path` and substitutes it for `*project`;
/// clears the history and entirely replaces `*geometries` with the geometry
/// block read from the file (`stokesforge_persistence::load_with_geometries`
/// already filters the orphan entries — see its docs). Refuses
/// (`Err(Command(MacroOpen))`) if a macro is open, **before** touching the
/// disk: in that case neither the project, nor the history, nor the geometries
/// are affected. If the load itself fails (missing, corrupt file, ...), the
/// error comes back as is and, here too, nothing was touched.
fn load_project(
    project: &mut Project,
    history: &mut History,
    geometries: &mut HashMap<NodeId, WindowGeometry>,
    column_widths: &mut HashMap<NodeId, f32>,
    path: &Path,
    limits: stokesforge_persistence::LoadLimits,
) -> SessionResult<LoadReport> {
    if history.in_macro() {
        return Err(SessionError::Command(CommandError::MacroOpen("load")));
    }
    let (mut loaded, view, report) = stokesforge_persistence::load_with_limits(path, limits)?;
    orient_graphs(&mut loaded);
    *project = loaded;
    *geometries = view.window_geometries;
    *column_widths = view.column_widths;
    // The actor is single-threaded and nothing could reopen a macro between
    // the check above and this call: `clear()` cannot fail here. The result is
    // ignored all the same rather than calling `unwrap`/`expect`, banned
    // outside tests.
    let _ = history.clear();
    Ok(report)
}

/// Brings the axes of every graph of a project that was just read back to
/// their canonical form (#344): the direction is read from the order of the
/// bounds, a file that carried `inverted` next to increasing bounds gets them
/// swapped (`AxisSpec::orient`). The rendering does not change. The project is
/// not installed yet and nobody holds a copy of it: no command and no history
/// (invariant 1), and persistence normalizes nothing.
fn orient_graphs(project: &mut Project) {
    for id in project.tree().walk_all() {
        let Some(NodeKind::Graph { spec, .. }) = project.tree().node(id).map(|n| n.kind()) else {
            continue;
        };
        let mut oriented = spec.clone();
        oriented.orient_axes();
        if oriented != *spec {
            // `id` is a graph: `set_plot_spec` cannot refuse.
            let _ = project.set_plot_spec(id, oriented);
        }
    }
}

/// Removes from `geometries` every entry whose `NodeId` no longer exists in
/// the tree of `project` (node deleted by an `Execute` command, or by an
/// `Undo`/`Redo` that restores a tree where it did not exist) — a risk-free
/// cleanup of an unreferenced `HashMap` entry (see the crate-level docs: the
/// chosen option is to purge rather than leave it lying around). Returns
/// `true` if something was actually removed, so that the caller decides
/// whether to advance `geometry_revision`.
fn purge_orphaned_geometries(
    project: &Project,
    geometries: &mut HashMap<NodeId, WindowGeometry>,
) -> bool {
    let tree = project.tree();
    let before = geometries.len();
    geometries.retain(|id, _| tree.node(*id).is_some());
    geometries.len() != before
}

/// Builds the current snapshot: the tree and the geometry table are shared
/// (`Arc::clone`/`Arc::new`), nothing is copied for the tree; the geometry
/// table, on the other hand, is copied into a fresh `Arc` at each snapshot — a
/// negligible cost, bounded by the number of open windows, not by the size of
/// the model.
fn build_snapshot(
    project: &Project,
    history: &History,
    revision: Revision,
    epoch: u64,
    geometries: &HashMap<NodeId, WindowGeometry>,
    column_widths: &HashMap<NodeId, f32>,
    geometry_revision: u64,
) -> Snapshot {
    Snapshot {
        tree: Arc::clone(project.tree()),
        revision,
        history: HistoryState {
            // C6 — during a macro, `undo` is always refused
            // (`Err(MacroOpen)`) whatever the stack says: the snapshot must
            // not announce an undo that the actor will refuse.
            can_undo: history.can_undo() && !history.in_macro(),
            can_redo: history.can_redo(),
            undo_label: history.undo_label().cloned(),
            redo_label: history.redo_label().cloned(),
            in_macro: history.in_macro(),
        },
        epoch,
        window_geometries: Arc::new(geometries.clone()),
        column_widths: Arc::new(column_widths.clone()),
        geometry_revision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exists only so that the compiler checks the `Send` invariant — it is
    /// not a behavioral test.
    fn assert_send<T: Send>() {}

    #[test]
    fn request_is_send() {
        assert_send::<Request>();
    }
}
