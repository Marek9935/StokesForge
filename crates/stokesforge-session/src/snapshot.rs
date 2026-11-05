//! Immutable snapshot returned to the session's clients.

use std::collections::HashMap;
use std::sync::Arc;

use stokesforge_commands::HistoryLabel;
use stokesforge_core::{NodeId, Tree};

use crate::WindowGeometry;

/// Monotonic state counter: incremented on every effective change of the
/// model or of the history (see the crate docs for the exact list of events
/// that increment it). Clients use it to detect that a snapshot is stale,
/// without ever comparing trees with each other.
pub type Revision = u64;

/// History state as the UI needs it for its actions (disabled menus,
/// undo/redo label), without exposing the stack itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryState {
    /// True if there is something to undo.
    pub can_undo: bool,
    /// True if there is something to redo.
    pub can_redo: bool,
    /// Label of the next undo, if there is one.
    pub undo_label: Option<HistoryLabel>,
    /// Label of the next redo, if there is one.
    pub redo_label: Option<HistoryLabel>,
    /// True if a macro is currently open. A macro is global to the
    /// session: this flag says nothing about who opened it (see the limit
    /// documented at the top of the crate).
    pub in_macro: bool,
}

/// The kept formulas the actor bound to a column or a sheet that appeared
/// (#433), as the snapshot reports them. The binding is written inside the
/// history entry of the gesture that created the object, so the report is only
/// information: the client says which column the formula now reads, once per
/// `count` it has not seen. Undo and redo add nothing (an undo unbinds, a redo
/// replays the recorded entry), a Stop takes back what its command had added.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoundReport {
    /// Number of formulas bound since the session started, monotonic.
    pub count: u64,
    /// The columns of the last bindings, oldest first, capped
    /// ([`BoundReport::RECENT`]): `recent.len()` may be below the difference
    /// of two counts when more were bound between two snapshots.
    pub recent: Vec<NodeId>,
}

impl BoundReport {
    /// How many columns [`BoundReport::recent`] keeps.
    pub const RECENT: usize = 32;

    pub(crate) fn push(&mut self, column: NodeId) {
        self.count += 1;
        self.recent.push(column);
        if self.recent.len() > Self::RECENT {
            self.recent.remove(0);
        }
    }

    /// Takes back the `n` last bindings (a Stop undid their gesture).
    pub(crate) fn retract(&mut self, n: u64) {
        self.count = self.count.saturating_sub(n);
        let keep = self.recent.len().saturating_sub(n as usize);
        self.recent.truncate(keep);
    }
}

/// Immutable snapshot returned to clients. Cloning a `Snapshot` is cheap
/// (two `Arc`s/counters and a handful of fields): the tree is never copied,
/// only shared.
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Project tree at the time of the snapshot.
    pub tree: Arc<Tree>,
    /// Revision this snapshot corresponds to.
    pub revision: Revision,
    /// History state at this revision.
    pub history: HistoryState,
    /// A3 (`guide-de-correction-robustesse.md` §F2/F6) — epoch of the current
    /// project: a counter local to the actor, **never persisted**,
    /// incremented when a `Load` or `NewProject` succeeds (never on
    /// `Execute`/`Undo`/`Redo`/macro, which already advance `revision`). Two
    /// snapshots of the same project identity share the same epoch; a
    /// replacement, even by a file of identical shape, changes it — unlike
    /// `revision`, which would restart at a small value and could look
    /// "earlier" without ever proving that it is still the same project.
    /// `stokesforge-script` uses it to detect that a handle (`Handle`) names
    /// a `NodeId` of a project that is no longer the one that produced it,
    /// before even looking that `NodeId` up in the current tree — where it
    /// could, by coincidence, name another object.
    pub epoch: u64,
    /// Window geometry (position/size), by `NodeId` — a table kept by the
    /// actor **beside** the `Project`, never in the tree (see the crate-level
    /// docs). Cloning a `Snapshot` stays cheap: this table is copied into a
    /// fresh `Arc` at each snapshot on the actor side (`build_snapshot`), but
    /// its cost is bounded by the number of open windows, not by the size of
    /// the model — out of all proportion with `tree`, which is never copied.
    pub window_geometries: Arc<HashMap<NodeId, WindowGeometry>>,
    /// Sheet column widths (#325), in logical points, by column `NodeId` —
    /// same status as `window_geometries`: an actor table outside the
    /// `Project`, never in the history. **Not purged** on mutation: it can
    /// hold the key of a deleted column, whose width comes back with Ctrl+Z
    /// (invariant 7); only saving filters. Shares `geometry_revision`.
    pub column_widths: Arc<HashMap<NodeId, f32>>,
    /// Monotonic counter **distinct** from `revision`: incremented on every
    /// change of `window_geometries` or `column_widths` (setting a geometry
    /// or a width, purging an orphan geometry entry, or a `Load` that
    /// replaces the whole table), never by `Execute`/`Undo`/`Redo`/a macro.
    /// See the crate-level docs for the decision: keeping this counter apart
    /// from `revision` is what lets a rendering client know that a window
    /// moved without it looking like a model mutation (and so without
    /// invalidating a cache that has no reason to be).
    pub geometry_revision: u64,
    /// The formulas bound to a column or sheet that appeared (#433).
    pub bound: Arc<BoundReport>,
    /// Formula columns the recalculation passes have computed since the
    /// session started, monotonic (#439). One per column a pass decided to
    /// compute (a `NaN` block written for a cycle or an unreadable formula
    /// counts), none for a column the targeted mode found clean. Column
    /// versions no longer show whether a column was recomputed (a pass that
    /// finds the same content keeps the version), so this is the witness of
    /// the economy of the targeted mode: an unrelated command leaves it
    /// unchanged. Read-only information, like `bound`: never in the history
    /// nor the `.stf`.
    pub formula_evaluations: u64,
}
