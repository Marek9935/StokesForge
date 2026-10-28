//! Two-kind history stack and command execution.
//!
//! # The two kinds
//!
//! - **structure** (tree, names, order, adding/removing nodes) →
//!   *persistent snapshot*. The entry keeps `before` and `after`, two
//!   `Arc<Tree>`. Undoing is `replace_tree(before)`. The two trees
//!   physically share everything that did not change: keeping both costs
//!   one pointer. The `before` tree keeps the removed nodes — there is nothing
//!   to free explicitly, and a restored node comes back under its
//!   **original identifier** (`Project::replace_tree` does not move the
//!   identifier allocator back).
//! - **data** (column contents) → *delta*. The entry keeps the
//!   old **and** new values of only the cells written, and replays them
//!   into the **current** tree. A snapshot would keep the whole column buffer
//!   here (8 MB for a million points) at every keystroke.
//!
//! # LIFO invariant
//!
//! A structural entry can be undone only if all later entries have already
//! been: restoring a whole tree overwrites everything written after it. The
//! stack guarantees this by construction, and **in a macro the entries are
//! undone strictly in reverse order of insertion**, whatever their kinds —
//! this is what makes the hybrid hold.
//!
//! # Two limits, the stricter of the two wins
//!
//! A limit counted in entries (`limit`) does not see the pattern measured by
//! `tests/memory_pinning.rs`: by alternating a structure operation and a
//! write on the same big column, each structural entry pins a full version of
//! the buffer that the next write replaces — a hundred renames weigh nothing,
//! a single workbook deletion weighs hundreds of megabytes. `byte_limit`
//! directly bounds the payload bytes that the history alone keeps alive (see
//! [`History::retained_bytes`] and [`History::evict`] for the method and the
//! eviction rules).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use stokesforge_core::{ColumnMeta, ColumnType, ColumnValues, MixedCell, WindowMeta};
use stokesforge_core::{CoreError, CoreResult, NodeId, Project, Tree, ValueBlock};
use stokesforge_plot::{PlotSpec, SavedColors};

use crate::command::Command;
use crate::error::{CommandError, CommandResult};
use crate::label::{HistoryLabel, MacroKind};

/// Delta of a data entry: everything needed to undo **and** redo, without
/// keeping a tree.
#[derive(Debug, Clone, PartialEq)]
pub enum DataDelta {
    /// Writing cells: `before` (+ `before_length`) to undo,
    /// `after` to redo.
    Cells {
        /// Column written.
        column: NodeId,
        /// First row written.
        start: usize,
        /// Values overwritten (the part covering rows that existed).
        before: ValueBlock,
        /// Length of the column before the write.
        before_length: usize,
        /// Values written.
        after: ValueBlock,
    },
    /// Inserting rows: the columns **actually** extended, recorded before
    /// applying.
    InsertRows {
        /// Columns that had a row `at` at the time of the insertion.
        columns: Vec<NodeId>,
        /// Insertion row.
        at: usize,
        /// Number of rows inserted.
        count: usize,
        /// Graphs whose "row" series (`row = Some(r)`) followed the
        /// insertion: `(graph, spec before, spec after)`, captured at apply
        /// time (invariant 8). Undo puts the before back, redo the
        /// after — no inverse recomputed.
        plots: Vec<(NodeId, PlotSpec, PlotSpec)>,
    },
    /// Deleting rows: the values removed, per affected column.
    DeleteRows {
        /// Target sheet (kept for inspection; undo works column by
        /// column).
        sheet: NodeId,
        /// First row deleted.
        at: usize,
        /// Number of rows requested (undo uses the size actually removed,
        /// which can be smaller at the end of a column).
        count: usize,
        /// Values removed, per affected column, in sheet order.
        removed: Vec<(NodeId, ValueBlock)>,
        /// Graphs whose "row" series followed the deletion:
        /// `(graph, spec before, spec after)`. Same rule as for
        /// `InsertRows`.
        plots: Vec<(NodeId, PlotSpec, PlotSpec)>,
    },
    /// Deleting cells by explicit columns, independently of each
    /// other (Delete key on a selection that does not necessarily cover the
    /// whole sheet) — the counterpart of `DeleteRows` for
    /// `Command::DeleteCells`. Same undo rules as `DeleteRows`:
    /// column by column, never at sheet level.
    DeleteCells {
        /// First row deleted.
        at: usize,
        /// Number of rows requested (undo uses the size actually removed per
        /// column, as for `DeleteRows`).
        count: usize,
        /// Values removed, per column actually affected, in the order in
        /// which `Command::DeleteCells::columns` presented them.
        removed: Vec<(NodeId, ValueBlock)>,
    },
    /// The only accepted case of a data snapshot: type conversion is
    /// destructive in both directions ("3,5" → 3.5 → "3.5"), so neither the
    /// old nor the new content can be recomputed from the other.
    ColumnType {
        /// Column converted.
        column: NodeId,
        /// Type and values before.
        before: (ColumnType, ColumnValues),
        /// Type and values after.
        after: (ColumnType, ColumnValues),
    },
    /// Replacing the metadata of a window: neither a structure snapshot
    /// (no node moves) nor a big buffer — `before`/`after` directly, like
    /// `ColumnType`.
    WindowMeta {
        /// Target window.
        id: NodeId,
        /// Metadata before.
        before: WindowMeta,
        /// Metadata after.
        after: WindowMeta,
    },
    /// Replacing the text of a note.
    NoteText {
        /// Target note.
        id: NodeId,
        /// Text before.
        before: String,
        /// Text after.
        after: String,
    },
    /// Replacing the metadata of a column (role, unit, long name,
    /// comment, display format) — distinct from `ColumnType`: this
    /// delta generally never touches the values of the column, only
    /// this small display record.
    ///
    /// Exception: `values_before`. When this command turns
    /// `formula` from `None` to `Some(_)`, the column stops being a source of
    /// truth and becomes, the instant after (`stokesforge-session::recalc::
    /// recalc_formulas`, outside the history — see its head doc), a
    /// recalculated column whose previous content is **overwritten without any
    /// command replaying it**. Without capturing this content here, undoing
    /// this entry does remove the formula (`before`) but leaves the computed
    /// values in place: `recalc_formulas` recalculates only the columns
    /// that *still carry* a formula, it silently ignores a column that has
    /// just lost one — that was the bug. `None` in all other cases (formula
    /// already active before this command, formula removed, or no formula
    /// change): these transitions need nothing more, the recalculation
    /// reloading itself on the basis of the formula — still active on both
    /// sides of the undo/redo — without ever losing user data.
    ColumnMeta {
        /// Target column.
        column: NodeId,
        /// Metadata before (boxed: `ColumnMeta` carries the file metadata and
        /// the import source, much bigger than the other deltas).
        before: Box<ColumnMeta>,
        /// Metadata after.
        after: Box<ColumnMeta>,
        /// Type and values of the column just before this command,
        /// only when `before.formula` was `None` and
        /// `after.formula` becomes `Some(_)` — see the doc above.
        values_before: Option<(ColumnType, ColumnValues)>,
    },
    /// Replacing the `PlotSpec` of a graph. A spec carries only
    /// identities and style, never data (see the head of
    /// `stokesforge-plot/src/lib.rs`): `before`/`after` directly, as for
    /// the three deltas above.
    PlotSpec {
        /// Target graph.
        graph: NodeId,
        /// Spec before.
        before: PlotSpec,
        /// Spec after.
        after: PlotSpec,
    },
    /// Replacing the colors saved in the project — a few
    /// strings, `before`/`after` directly.
    SavedColors {
        /// Colors before.
        before: SavedColors,
        /// Colors after.
        after: SavedColors,
    },
}

/// A history entry: a structure snapshot, a data delta, or
/// a group (macro) that is undone as one block.
#[derive(Debug, Clone)]
pub enum Entry {
    /// Structure snapshot.
    Structural {
        /// Tree before the command.
        before: Arc<Tree>,
        /// Tree after the command.
        after: Arc<Tree>,
        /// History label.
        label: HistoryLabel,
    },
    /// Data delta.
    Data {
        /// What must be replayed into the current tree. `Box`ed: since
        /// the addition of `ColumnMeta::formula` (computed columns), the
        /// variant `DataDelta::ColumnMeta { before, after }` carries two
        /// full `ColumnMeta` and makes `DataDelta` by far the biggest
        /// variant of `Entry` — `clippy::large_enum_variant`
        /// (triggered by that addition) recommends this indirection rather
        /// than making *all* structure/macro entries carry the memory cost
        /// of the biggest possible delta.
        delta: Box<DataDelta>,
        /// History label.
        label: HistoryLabel,
    },
    /// Group of entries, undone in reverse order and redone in forward order.
    Macro {
        /// Entries in their order of application.
        entries: Vec<Entry>,
        /// Label of the group: always [`HistoryLabel::Macro`].
        label: HistoryLabel,
    },
}

impl Entry {
    /// Displayable label of the entry ("Undo: ...").
    pub fn label(&self) -> &HistoryLabel {
        match self {
            Entry::Structural { label, .. }
            | Entry::Data { label, .. }
            | Entry::Macro { label, .. } => label,
        }
    }
}

/// Memory footprint of an entry: for each column buffer it
/// keeps — directly (a data delta carries its own values) or
/// through the tree it captures (a structure snapshot carries the
/// columns of the whole tree) — the address of its allocation and its size
/// in bytes.
///
/// The address only identifies a buffer, never dereferences it:
/// this is what allows deduplicating the buffers shared between several
/// entries, or between an entry and the current tree, without ever reading
/// the memory it designates.
///
/// Computed once and for all when the entry is pushed:
/// the tree being immutable, this set never changes afterwards (see
/// [`footprint_of`]).
type Footprint = HashMap<usize, usize>;

/// What `just_orphaned_by_write` needs to know from before the application
/// of a data command: for each column it could touch,
/// the address and size of the buffer it carries at that moment.
///
/// Captured by [`prior_columns`], by a simple borrow of `project.tree()`
/// **before** applying the command — never the whole tree, never kept
/// alive beyond the call that builds it (see the doc of
/// [`History::execute`], D3 (2nd half) of the guide): this is precisely what
/// allows `Arc::make_mut` to mutate the `Tree` in place rather than clone it.
type PriorColumns = HashMap<NodeId, (usize, usize)>;

/// A history entry together with its memory footprint.
///
/// This coupling lives next to [`Entry`] rather than in its variants: it is
/// an accounting detail specific to `History` (it serves neither to undo
/// nor to redo), and keeping it out of `Entry` leaves the public API of this
/// type unchanged.
#[derive(Debug)]
struct Pinned {
    /// The entry itself — what `revert`/`reapply` consume.
    entry: Entry,
    /// Its footprint, computed once and for all when pushed.
    footprint: Footprint,
}

/// What is permanently known about a buffer that **at least one** `undo`
/// entry (the invariant "`redo` is always empty between two commands" —
/// see the doc of [`History::push`] — holds here too, so `redo` never
/// needs to be tracked separately) keeps in its footprint: how many
/// entries keep it, and whether it has already left the current tree.
///
/// It is the *live* counterpart, kept up to date buffer by buffer, of what
/// [`History::recompute_retained_bytes`] used to recompute from
/// zero at each command — see [`History::pin_footprint`] and
/// [`History::unpin_footprint`] for how it is maintained, and the
/// section "Incremental tracking (anomaly D3)" on [`History::evict`]
/// for why.
#[derive(Debug, Clone, Copy)]
struct PinnedBuffer {
    /// Bytes of the buffer — copied from the `Footprint` of the entry that
    /// introduced it, never recomputed.
    bytes: usize,
    /// Number of `undo` entries that keep it in their footprint.
    refcount: usize,
    /// True if it has already left the current tree (counted in
    /// `retained_bytes`); false if it is still shared with it (free — not
    /// a cost of the history).
    orphaned: bool,
}

/// Default history limit, in number of entries: the expected undo
/// depth, the order of magnitude of a spreadsheet. Silent eviction of the
/// oldest (`questions-ouvertes.md`, question 7 —
/// recommendation applied, to be confirmed).
pub const DEFAULT_LIMIT: usize = 100;

/// Default history limit, in bytes of retained payload
/// (512 MiB). Protects the pattern that `DEFAULT_LIMIT` alone does not see: see
/// the section "Two limits" at the top of the module.
pub const DEFAULT_BYTE_LIMIT: usize = 512 * 1024 * 1024;

/// Maximum nesting depth of macros (B7,
/// `guide-de-correction-robustesse.md` §F4 — option 1, chosen by the
/// guide: a named constant and an error, rather than making
/// `revert`/`Drop` iterative on `Entry::Macro`, « du code délicat sur un
/// enum récursif »). Nothing bounded the nesting before this fix:
/// `begin_macro`/`end_macro` are themselves iterative (building them never
/// overflows, the probe checks it up to 200,000 levels), but
/// `History::revert` and the derived `Drop` of `Entry` recurse on
/// `Entry::Macro` — beyond ~10,891 levels, one or the other overflows the
/// stack of the actor thread (2 MiB by default) and `abort()`s the whole
/// process, including any unsaved work (see
/// `stokesforge-robustesse/src/bin/sonde_pile.rs`). A thousand levels cover
/// three orders of magnitude above any plausible use (§7 of the guide) while
/// leaving a wide margin under the measured overflow threshold.
pub const MAX_MACRO_DEPTH: usize = 1000;

/// Two-kind history stack.
///
/// It does **not own** the `Project`: each operation receives it as
/// `&mut`. The `stokesforge-session` actor holds both, which avoids locking
/// the model inside the history.
#[derive(Debug)]
pub struct History {
    /// Undoable entries, the most recent last, each with the memory
    /// footprint computed when it was pushed.
    undo: Vec<Pinned>,
    /// Redoable entries, the next one last.
    redo: Vec<Pinned>,
    /// Maximum number of top-level entries; `0` = unlimited.
    limit: usize,
    /// Maximum bytes of retained payload; `0` = unlimited.
    byte_limit: usize,
    /// Bytes of retained payload — exact, kept up to date continuously
    /// (see [`History::retained_bytes`], and the incremental tracking
    /// documented on [`History::evict`]).
    retained_bytes: usize,
    /// Live accounting, buffer by buffer, of what `undo` keeps —
    /// see [`PinnedBuffer`]. `retained_bytes` is, at all times, the sum
    /// of the `bytes` of its `orphaned` entries.
    pinned: HashMap<usize, PinnedBuffer>,
    /// Open macros, from the outermost to the innermost: label and
    /// entries already footprinted, accumulated before becoming a single
    /// `Macro` entry on closing.
    macros: Vec<(MacroKind, Vec<Pinned>)>,
    /// Redo stack as it was before the outermost macro was opened
    /// (`None` outside a macro). Saved at `begin_macro`, it is restored if
    /// this macro is aborted — or closed empty — and dropped if it succeeds
    /// with at least one entry: an aborted macro must leave no trace,
    /// including on redo.
    saved_redo: Option<Vec<Pinned>>,
}

impl History {
    /// New stack, with the two default limits ([`DEFAULT_LIMIT`] entries,
    /// [`DEFAULT_BYTE_LIMIT`] retained bytes).
    pub fn new() -> Self {
        Self::with_limits(DEFAULT_LIMIT, DEFAULT_BYTE_LIMIT)
    }

    /// New stack limited to `limit` entries (`0` = unlimited); keeps
    /// [`DEFAULT_BYTE_LIMIT`] for the byte limit.
    pub fn with_limit(limit: usize) -> Self {
        Self::with_limits(limit, DEFAULT_BYTE_LIMIT)
    }

    /// New stack with both limits explicit. `0` = unlimited, for
    /// either. The stricter of the two wins:
    /// after each push, eviction continues as long as one or the
    /// other is exceeded (see [`History::evict`]).
    pub fn with_limits(limit: usize, byte_limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit,
            byte_limit,
            retained_bytes: 0,
            pinned: HashMap::new(),
            macros: Vec::new(),
            saved_redo: None,
        }
    }

    /// Current limit in number of entries; `0` = unlimited.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Current limit in bytes of retained payload; `0` = unlimited.
    pub fn byte_limit(&self) -> usize {
        self.byte_limit
    }

    /// Bytes of payload that the history alone keeps alive — **exact**, at
    /// all times (see the incremental tracking documented on
    /// [`History::evict`]: `undo`/`redo`/`begin_macro`/
    /// `abort_macro` do not need to trigger it again, since none
    /// of them adds, removes, or changes the "pinned/orphaned" status of
    /// any buffer — they only move entries already counted between the two
    /// stacks, or replay them into the project).
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    /// Changes the limit in number of entries. `Err(MacroOpen)` in a macro;
    /// `Err(StackNotEmpty)` if `undo` or `redo` is not empty — changing the
    /// limit in the middle of a history would require deciding what to evict,
    /// and that decision does not belong to the stack.
    pub fn set_limit(&mut self, limit: usize) -> CommandResult<()> {
        if self.in_macro() {
            return Err(CommandError::MacroOpen("set_limit"));
        }
        if !self.undo.is_empty() || !self.redo.is_empty() {
            return Err(CommandError::StackNotEmpty);
        }
        self.limit = limit;
        Ok(())
    }

    /// Changes the limit in bytes of retained payload. Same rules as
    /// [`History::set_limit`]: `Err(MacroOpen)` in a macro, `Err(StackNotEmpty)`
    /// if a stack is not empty.
    pub fn set_byte_limit(&mut self, bytes: usize) -> CommandResult<()> {
        if self.in_macro() {
            return Err(CommandError::MacroOpen("set_byte_limit"));
        }
        if !self.undo.is_empty() || !self.redo.is_empty() {
            return Err(CommandError::StackNotEmpty);
        }
        self.byte_limit = bytes;
        Ok(())
    }

    /// Applies `command` then pushes it (or adds it to the open macro).
    ///
    /// **Two paths, one and the same contract.** The contract — on `Err`,
    /// neither the tree nor the stack has changed — is identical for both
    /// kinds; only the **mechanism** that holds it differs (D3 (2nd
    /// half), `guide-de-correction-robustesse.md` §6 « ne pas supprimer la
    /// comptabilité, seulement sa fréquence »):
    ///
    /// - **structure**: `before` (an `Arc<Tree>` clone, one pointer) is
    ///   kept alive during the whole application, then put back as a
    ///   safety on `Err` (`project.replace_tree(before)`), without relying
    ///   on the atomicity promise of the core primitives. `Entry::Structural`
    ///   needs it anyway for `before`/`after`: the cost is already
    ///   paid, the insurance is free.
    /// - **data**: keeping `before` alive would force `Arc::make_mut` (seen
    ///   by `Project::set_values` and co.) to clone the whole `Tree`
    ///   instead of mutating in place — the whole point of D3. So we do not
    ///   do it; the contract then rests on each data primitive
    ///   *validating before mutating* (`Column::set_values`,
    ///   `Column::insert_rows`... — tested in `stokesforge-core`): a rejection
    ///   never started writing, so nothing is half-done and needs
    ///   restoring. Only what `just_orphaned_by_write` needs (the
    ///   pointer of the buffer already present in the targeted column(s)) is
    ///   captured, by a simple borrow, **before** applying the command —
    ///   never the tree.
    ///
    /// On success the redo stack is emptied, **including in a macro**: as
    /// soon as a new mutation is applied, what had been undone has no branch
    /// left to return to.
    pub fn execute(&mut self, project: &mut Project, command: Command) -> CommandResult<()> {
        self.execute_recorded(project, command).map(|_| ())
    }

    /// Like [`execute`](Self::execute), but tells whether the command **stored
    /// an entry** (on the stack, or in the open macro): `Ok(false)` for an
    /// accepted command with no effect (B5), which takes no slot.
    ///
    /// The actor uses it so that Stop undoes only the command it has just
    /// executed: without this boolean, an `undo` undid the *previous* action.
    /// It is not deduced from `len()` before/after — eviction (entry or byte
    /// limit) leaves the length unchanged, and an entry stored in a macro
    /// does not change it either.
    pub fn execute_recorded(
        &mut self,
        project: &mut Project,
        command: Command,
    ) -> CommandResult<bool> {
        if command.is_structural() {
            let before = Arc::clone(project.tree());
            let label = command.label(&before);
            match Self::apply_structural(project, command, &before, label) {
                Ok(entry) => {
                    // B5 + the Rename case (`guide-de-correction-robustesse.md`
                    // §F5) — an accepted command that changed NOTHING takes
                    // no undo slot: "Rename M to M" (recorded without
                    // identifier or reproduction in §7.3 of the
                    // guide) — a structural entry whose `before == after`
                    // covers this case and any other that resembles it, without
                    // naming them one by one. Nothing to undo: we simply
                    // return `Ok` without pushing.
                    if entry_has_no_effect(&entry) {
                        return Ok(false);
                    }
                    self.push(entry, &PriorColumns::new(), project);
                    Ok(true)
                }
                Err(err) => {
                    // The `Project` primitives promise to change nothing on
                    // failure, but restoring `before` is free (one pointer)
                    // and makes the guarantee independent of the core.
                    project.replace_tree(before);
                    Err(err)
                }
            }
        } else {
            // No tree clone here: see the doc above. `label`
            // and `prior` only borrow `project.tree()`, never
            // keeping it beyond this line.
            let label = command.label(project.tree());
            let prior = prior_columns(project.tree(), &command);
            match Self::apply_data(project, command, label) {
                Ok(entry) => {
                    // B5 (`guide-de-correction-robustesse.md` §F5) —
                    // `InsertRows`/`DeleteRows` without a target column (see
                    // `Project::row_targets`) take no undo
                    // slot.
                    if entry_has_no_effect(&entry) {
                        return Ok(false);
                    }
                    self.push(entry, &prior, project);
                    Ok(true)
                }
                // No `replace_tree` to do here: see the doc above —
                // a data primitive that fails has mutated nothing.
                Err(err) => Err(err),
            }
        }
    }

    /// Undoes the top entry. `Ok(false)` if the stack is empty — that is
    /// not an error. `Err(MacroOpen)` if a macro is open.
    ///
    /// An error returned while undoing a `Data` entry should not
    /// happen (the LIFO invariant guarantees the state is the expected one).
    /// If it does, the only guarantee is that **the entry was not
    /// moved between stacks**: the tree itself may be half-undone.
    pub fn undo(&mut self, project: &mut Project) -> CommandResult<bool> {
        if self.in_macro() {
            return Err(CommandError::MacroOpen("undo"));
        }
        let Some(pinned) = self.undo.last() else {
            return Ok(false);
        };
        Self::revert(project, &pinned.entry)?;
        if let Some(pinned) = self.undo.pop() {
            // `pinned`/`retained_bytes` count the union of `undo` and
            // `redo` (see their doc): moving an entry from one to the
            // other changes neither what it keeps, nor its
            // pinned/orphaned status — nothing to update here.
            self.redo.push(pinned);
        }
        Ok(true)
    }

    /// Redoes the most recently undone entry. Same conventions as
    /// [`History::undo`].
    pub fn redo(&mut self, project: &mut Project) -> CommandResult<bool> {
        if self.in_macro() {
            return Err(CommandError::MacroOpen("redo"));
        }
        let Some(pinned) = self.redo.last() else {
            return Ok(false);
        };
        Self::reapply(project, &pinned.entry)?;
        if let Some(pinned) = self.redo.pop() {
            self.undo.push(pinned);
        }
        Ok(true)
    }

    /// True if there is something to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// True if there is something to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Label of the next undo.
    pub fn undo_label(&self) -> Option<&HistoryLabel> {
        self.undo.last().map(|pinned| pinned.entry.label())
    }

    /// Label of the next redo.
    pub fn redo_label(&self) -> Option<&HistoryLabel> {
        self.redo.last().map(|pinned| pinned.entry.label())
    }

    /// Number of undoable entries (a macro counts as one).
    pub fn len(&self) -> usize {
        self.undo.len()
    }

    /// True if nothing is undoable.
    pub fn is_empty(&self) -> bool {
        self.undo.is_empty()
    }

    /// Number of redoable entries.
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Opens a macro: the following commands accumulate in it instead of being
    /// pushed. Macros nest, up to [`MAX_MACRO_DEPTH`]
    /// levels: beyond, `Err(MacroTooDeep)` (B7,
    /// `guide-de-correction-robustesse.md` §F4) rather than pushing further
    /// — see the doc of [`MAX_MACRO_DEPTH`] for what this limit avoids.
    /// A refusal changes nothing: neither `self.macros` nor `self.saved_redo`.
    ///
    /// On opening the **outermost** macro (`macro_depth() == 0 →
    /// 1`), the current redo stack is set aside: it
    /// can be restored if this macro is aborted, see
    /// [`History::abort_macro`].
    pub fn begin_macro(&mut self, kind: MacroKind) -> CommandResult<()> {
        if self.macros.len() >= MAX_MACRO_DEPTH {
            return Err(CommandError::MacroTooDeep(MAX_MACRO_DEPTH));
        }
        if self.macros.is_empty() {
            self.saved_redo = Some(std::mem::take(&mut self.redo));
        }
        self.macros.push((kind, Vec::new()));
        Ok(())
    }

    /// Closes the current macro: it becomes **a single** `Macro` entry,
    /// pushed normally (or added to the parent macro if there is one).
    /// An empty macro leaves **no** entry. `Err(NotInMacro)` otherwise.
    ///
    /// The footprint of the `Macro` entry is the union of those, already
    /// known, of its entries — a merge of hash tables, never a new
    /// tree walk.
    ///
    /// On closing the **outermost** macro, the redo stack saved on opening
    /// is dropped: the redo is well and truly lost when the macro succeeds with
    /// at least one entry, as for any command. **Except** if the macro is empty
    /// (no entry applied): it changed nothing, so the saved stack is restored.
    pub fn end_macro(&mut self, project: &mut Project) -> CommandResult<()> {
        let Some((kind, entries)) = self.macros.pop() else {
            return Err(CommandError::NotInMacro);
        };
        let is_outermost = self.macros.is_empty();
        if entries.is_empty() {
            if is_outermost {
                self.restore_saved_redo();
            }
            return Ok(());
        }
        if is_outermost {
            // The redo saved on opening is well and truly lost: its
            // buffers must leave the accounting (`self.pinned`)
            // exactly as if it had been emptied by an ordinary `push`
            // (see [`History::push`]) — without that, `retained_bytes`
            // would keep counting buffers that no live entry
            // keeps any more.
            if let Some(discarded) = self.saved_redo.take() {
                for pinned in discarded {
                    self.unpin_footprint(&pinned.footprint);
                }
            }
        }
        let footprint = union_footprint(entries.iter().map(|pinned| &pinned.footprint));
        let entry = Entry::Macro {
            entries: entries.into_iter().map(|pinned| pinned.entry).collect(),
            label: HistoryLabel::Macro(kind),
        };
        // `prior` is read only on the `Data` branch of `push_pinned`; a
        // `Macro` entry falls back on `resync_pinned`, which does not look at it.
        self.push_pinned(Pinned { entry, footprint }, &PriorColumns::new(), project);
        Ok(())
    }

    /// Undoes, in reverse order, everything the current macro applied, then
    /// drops it. An aborted nested macro does **not** touch what the parent
    /// macro has already applied. `Err(NotInMacro)` otherwise.
    ///
    /// If an undo fails along the way, the error comes up and the macro
    /// is already removed: the tree may then be half-undone (same
    /// reservation as [`History::undo`]).
    ///
    /// An aborted macro leaves **no** trace, including on redo:
    /// when the **outermost** macro is aborted, the redo stack saved at
    /// `begin_macro` is restored once its entries are undone — what the macro
    /// may have made redoable in the meantime (each `execute` empties the redo,
    /// even in a macro) disappears, and what was redoable **before** the
    /// macro comes back.
    pub fn abort_macro(&mut self, project: &mut Project) -> CommandResult<()> {
        let Some((_, entries)) = self.macros.pop() else {
            return Err(CommandError::NotInMacro);
        };
        for pinned in entries.iter().rev() {
            Self::revert(project, &pinned.entry)?;
        }
        if self.macros.is_empty() {
            self.restore_saved_redo();
        }
        Ok(())
    }

    /// True if at least one macro is open.
    pub fn in_macro(&self) -> bool {
        !self.macros.is_empty()
    }

    /// Number of open macros (nesting).
    pub fn macro_depth(&self) -> usize {
        self.macros.len()
    }

    /// Empties both stacks. `Err(MacroOpen)` in a macro: throwing away the history
    /// under an open macro would leave a group with nowhere to land.
    pub fn clear(&mut self) -> CommandResult<()> {
        if self.in_macro() {
            return Err(CommandError::MacroOpen("clear"));
        }
        self.undo.clear();
        self.redo.clear();
        // Exact, not a mere recalculation saving: the union of two empty
        // stacks is empty.
        self.retained_bytes = 0;
        self.pinned.clear();
        Ok(())
    }

    // -- internal --

    /// Restores the redo stack saved when the outermost macro was opened,
    /// if `saved_redo` holds one. Called on `abort_macro` and on
    /// `end_macro` of an empty macro, both at the outermost level.
    fn restore_saved_redo(&mut self) {
        if let Some(saved) = self.saved_redo.take() {
            self.redo = saved;
        }
    }

    /// Pushes a freshly applied entry (outside a macro): the redo stack is
    /// emptied, the footprint of the entry is computed once and for all, then
    /// the entry joins the open macro or the main stack.
    ///
    /// `prior` is what [`prior_columns`] captured, by borrow, on the tree
    /// **just before** this command — empty for a `Structural` entry
    /// (it does not use it, see [`History::push_pinned`]).
    /// A `Data` entry does not keep track of `prior` itself (it keeps
    /// only its delta), but [`History::push_pinned`] needs it
    /// to know whether the write has just orphaned the old buffer of a
    /// column that an *earlier* entry already kept (see its doc).
    fn push(&mut self, entry: Entry, prior: &PriorColumns, project: &Project) {
        // The redo is lost: its buffers must leave the accounting
        // exactly as those of a successfully abandoned macro (see
        // `History::end_macro`) — without that, `retained_bytes` would keep
        // counting buffers that no live entry keeps any more.
        let discarded = std::mem::take(&mut self.redo);
        for pinned in discarded {
            self.unpin_footprint(&pinned.footprint);
        }
        let footprint = footprint_of(&entry);
        self.push_pinned(Pinned { entry, footprint }, prior, project);
    }

    /// Stores an already footprinted entry in the open macro, or on top
    /// of the main stack (with accounting update then
    /// eviction). **Nothing changes in the middle of an open macro**: no
    /// eviction, no accounting of retained bytes — only the closing of
    /// the outermost macro that succeeds with at least one entry (see
    /// [`History::end_macro`]) joins the main stack.
    ///
    /// # Incremental tracking (anomaly D3)
    ///
    /// Before this fix, landing here triggered, at **each** command —
    /// even a simple cell write — a full recomputation of
    /// `retained_bytes`: walking the whole current tree (all the
    /// columns of the project) then redoing the union of the footprints of all
    /// the entries of the two stacks. The cost of a keystroke thus followed the
    /// size of the project *and* the depth of the history, even when no
    /// eviction took place (see
    /// `stokesforge-robustesse/tests/b21_cout_et_identite.rs`, reproductions
    /// `b21_le_cout_dune_ecriture_ne_depend_pas_de_lhistorique` and
    /// `..._du_nombre_de_colonnes`).
    ///
    /// The fix **never** relaxes the exactness of `retained_bytes`
    /// (see `tests/memory_pinning.rs`): instead of recomputing it from
    /// zero, it keeps it **up to date buffer by buffer**, in
    /// [`History::pinned`], at each push — without ever walking
    /// the whole tree or rereading the complete history:
    ///
    /// - a `Structural` entry already captures `before` and `after` in its
    ///   two fields: a buffer of its own footprint is orphaned if it
    ///   is not in `after` (so no longer in the current tree), alive
    ///   otherwise. A single walk of `after` (unavoidable: it is already
    ///   the work that [`footprint_of`] does to build the footprint of
    ///   *this* entry, not a new walk added by this fix);
    /// - a `Data` entry never shares its own delta with the tree
    ///   (see [`footprint_of`]): its whole footprint is orphaned as soon
    ///   as it is pushed. What *can* change elsewhere is that it has just
    ///   replaced, by copy-on-write, the buffer its column
    ///   had *before* it — if that buffer was already pinned by an
    ///   older entry (a lookup by identifier in `before`, O(1),
    ///   never a walk), it has just left the tree: see
    ///   [`just_orphaned_by_write`];
    /// - evicting an entry (below) simply removes its share from the
    ///   count, buffer by buffer (see [`History::unpin_footprint`]).
    ///
    /// The only case not modeled here is the closing of a macro
    /// (`Entry::Macro`): it aggregates sub-commands applied at different
    /// times, and finding *when* each one orphaned
    /// what would require replaying their before/after one by one. As
    /// macros are not triggered per keystroke — the measured anomaly —
    /// this entry falls back on the old exhaustive computation
    /// ([`History::resync_pinned`]), which rebuilds `self.pinned` at the same
    /// time as it recomputes `retained_bytes`, so that the incremental
    /// tracking stays correct afterwards.
    fn push_pinned(&mut self, pinned: Pinned, prior: &PriorColumns, project: &Project) {
        if let Some((_, entries)) = self.macros.last_mut() {
            entries.push(pinned);
            return;
        }
        match &pinned.entry {
            Entry::Macro { .. } => {
                self.undo.push(pinned);
                self.resync_pinned(project);
            }
            Entry::Data { delta, .. } => {
                let just_orphaned = just_orphaned_by_write(prior, delta);
                self.mark_orphaned(&just_orphaned);
                // A `Data` entry never shares its delta with the
                // tree: each of its own buffers is orphaned as soon as
                // it is introduced (see the doc above).
                self.pin_footprint(&pinned.footprint, |_ptr| true);
                self.undo.push(pinned);
            }
            Entry::Structural { .. } => {
                // `after` (the field of the entry) and `project.tree()` are the
                // same tree at this moment: nothing could run between the
                // end of `apply` and this call.
                let current = current_tree_pointers(project.tree());
                self.pin_footprint(&pinned.footprint, |ptr| !current.contains(&ptr));
                self.undo.push(pinned);
            }
        }
        self.evict();
    }

    /// For each buffer of `footprint`: increments its reference count
    /// if it is already known to [`History::pinned`] (its
    /// pinned/orphaned status, decided at its very first entry, does not
    /// change here — see [`History::mark_orphaned`] for the only way an
    /// already live buffer can later become orphaned); otherwise adds it
    /// there, orphaned if `is_orphan` says true for this pointer (and then
    /// counted at once in `retained_bytes`), alive otherwise.
    fn pin_footprint(&mut self, footprint: &Footprint, is_orphan: impl Fn(usize) -> bool) {
        for (&ptr, &bytes) in footprint {
            match self.pinned.entry(ptr) {
                std::collections::hash_map::Entry::Occupied(mut occupied) => {
                    occupied.get_mut().refcount += 1;
                }
                std::collections::hash_map::Entry::Vacant(vacant) => {
                    let orphaned = is_orphan(ptr);
                    if orphaned {
                        self.retained_bytes += bytes;
                    }
                    vacant.insert(PinnedBuffer {
                        bytes,
                        refcount: 1,
                        orphaned,
                    });
                }
            }
        }
    }

    /// For each buffer of `just_left`, if it is already pinned (by an
    /// older entry) and still marked alive, flips it to
    /// orphaned and adds its bytes to `retained_bytes`. A buffer absent from
    /// `self.pinned` is not a cost of the history — it was never
    /// pinned — and does nothing here.
    fn mark_orphaned(&mut self, just_left: &Footprint) {
        for &ptr in just_left.keys() {
            if let Some(buffer) = self.pinned.get_mut(&ptr) {
                if !buffer.orphaned {
                    buffer.orphaned = true;
                    self.retained_bytes += buffer.bytes;
                }
            }
        }
    }

    /// Removes the share of an evicted entry (or a dropped stack) in
    /// [`History::pinned`]: decrements the reference count of each
    /// of its buffers, and when it falls to zero (no entry keeps it
    /// any more), removes it from the accounting — subtracting its bytes from
    /// `retained_bytes` if it was counted there (orphaned).
    fn unpin_footprint(&mut self, footprint: &Footprint) {
        for &ptr in footprint.keys() {
            if let std::collections::hash_map::Entry::Occupied(mut occupied) =
                self.pinned.entry(ptr)
            {
                let buffer = occupied.get_mut();
                buffer.refcount -= 1;
                if buffer.refcount == 0 {
                    if buffer.orphaned {
                        self.retained_bytes -= buffer.bytes;
                    }
                    occupied.remove();
                }
            }
        }
    }

    /// Silently removes the oldest entries of `undo` as long as
    /// one of the two limits (number of entries, retained bytes) is
    /// exceeded. `retained_bytes` is already exact on entry (see
    /// [`History::push_pinned`]): evicting an entry only has to
    /// subtract its share (see [`History::unpin_footprint`]), never to
    /// walk anything again.
    ///
    /// **The last remaining entry is never evicted**, even if it alone
    /// exceeds the byte limit: a heavy operation must stay undoable
    /// right after it was done, otherwise the byte limit produces exactly the
    /// nuisance it seeks to avoid (nothing left to undo after the command that
    /// has just run). A one-entry stack can therefore stay durably above the
    /// byte cap if that entry alone already exceeds it.
    ///
    /// Eviction **never** touches the redo stack (`redo`):
    /// it is emptied by the next push (see [`History::push`]).
    /// The retained total is the union of the two stacks, so **a long redo
    /// branch can keep the total above the byte cap until the next
    /// command** — this is not an oversight: it is the price of never making a
    /// redo branch disappear other than at the next mutation.
    fn evict(&mut self) {
        while self.undo.len() > 1 {
            let over_count = self.limit != 0 && self.undo.len() > self.limit;
            let over_bytes = self.byte_limit != 0 && self.retained_bytes > self.byte_limit;
            if !over_count && !over_bytes {
                break;
            }
            // The stack stays short (about a hundred entries by default): the
            // shifting of a `Vec` is of no consequence next to the cost of a
            // `VecDeque` everywhere else.
            let evicted = self.undo.remove(0);
            self.unpin_footprint(&evicted.footprint);
        }
    }

    /// Safety net: recomputes [`History::retained_bytes`] **and**
    /// rebuilds [`History::pinned`] entirely from zero — the old
    /// exhaustive computation (full walk of the current tree, union of
    /// all the footprints of `undo` and `redo`), plus the rebuilding of the
    /// incremental tracking on which [`History::push_pinned`] then relies.
    /// Reserved for the only case this tracking does not model
    /// (the aggregation of a macro on closing, see
    /// [`History::push_pinned`]): never called on the path of a
    /// simple command, never the cost of an ordinary keystroke.
    fn resync_pinned(&mut self, project: &Project) {
        self.pinned.clear();
        let current = current_tree_pointers(project.tree());
        for pinned in self.undo.iter().chain(self.redo.iter()) {
            for (&ptr, &bytes) in &pinned.footprint {
                self.pinned
                    .entry(ptr)
                    .and_modify(|buffer| buffer.refcount += 1)
                    .or_insert(PinnedBuffer {
                        bytes,
                        refcount: 1,
                        orphaned: !current.contains(&ptr),
                    });
            }
        }
        self.retained_bytes = self
            .pinned
            .values()
            .filter(|buffer| buffer.orphaned)
            .map(|buffer| buffer.bytes)
            .sum();
    }

    /// Applies a **structural** command and builds its history entry.
    /// `before` is the `Arc<Tree>` that [`History::execute`] cloned before
    /// calling this function — needed here: `Entry::
    /// Structural` keeps it, and `DeleteColumn` must read the targeted node as
    /// it was before any mutation.
    ///
    /// Never called with a data command — `execute` dispatches on
    /// [`Command::is_structural`] before calling either one.
    fn apply_structural(
        project: &mut Project,
        command: Command,
        before: &Arc<Tree>,
        label: HistoryLabel,
    ) -> CommandResult<Entry> {
        match command {
            Command::AddColumn {
                sheet,
                index,
                name,
                column,
            } => {
                project.add_column(sheet, index, name.as_deref(), column)?;
                Ok(structural(before, project, label))
            }
            Command::DeleteColumn { column } => {
                // Checked **before** calling `remove`, which would accept
                // any node: a command named "delete a column" must not be
                // able to take away a whole sheet.
                let node = before.get(column)?;
                if !node.kind().is_column() {
                    return Err(CoreError::Structure(format!(
                        "{column} is not a column but {}: column deletion refused",
                        node.kind().with_article()
                    ))
                    .into());
                }
                project.remove(column)?;
                Ok(structural(before, project, label))
            }
            Command::Rename { id, name } => {
                project.rename(id, &name)?;
                Ok(structural(before, project, label))
            }
            Command::Move {
                id,
                new_parent,
                index,
            } => {
                project.move_node(id, new_parent, index)?;
                Ok(structural(before, project, label))
            }
            Command::AddFolder { parent, name } => {
                project.add_folder(parent, &name)?;
                Ok(structural(before, project, label))
            }
            Command::AddWorkbook { parent, name } => {
                project.add_workbook(parent, name.as_deref())?;
                Ok(structural(before, project, label))
            }
            Command::AddEmptyWorkbook { parent, name } => {
                project.add_empty_workbook(parent, name.as_deref())?;
                Ok(structural(before, project, label))
            }
            Command::AddWorksheet { book, name } => {
                project.add_worksheet(book, name.as_deref())?;
                Ok(structural(before, project, label))
            }
            Command::DuplicateWorksheet { sheet, name } => {
                project.duplicate_worksheet(sheet, &name)?;
                Ok(structural(before, project, label))
            }
            Command::InsertCopy { parent, copy, name } => {
                project.insert_copy(parent, &copy, &name)?;
                Ok(structural(before, project, label))
            }
            Command::AddGraph { parent, name, spec } => {
                project.add_graph(parent, name.as_deref(), spec)?;
                Ok(structural(before, project, label))
            }
            Command::AddMatrix { parent, name } => {
                project.add_matrix(parent, name.as_deref())?;
                Ok(structural(before, project, label))
            }
            Command::AddNote { parent, name, text } => {
                project.add_note(parent, name.as_deref(), &text)?;
                Ok(structural(before, project, label))
            }
            Command::RemoveNode { id } => {
                project.remove(id)?;
                Ok(structural(before, project, label))
            }
            Command::ReplaceAnalysisData {
                draft,
                expected_versions,
            } => {
                project.replace_analysis_data(draft, &expected_versions)?;
                Ok(structural(before, project, label))
            }
            Command::AddCalculation { draft } => {
                project.add_calculation(draft)?;
                Ok(structural(before, project, label))
            }
            Command::RestoreOriginalData { trace } => {
                project.restore_original_data(trace)?;
                Ok(structural(before, project, label))
            }
            Command::UpdateCalculation { id, draft } => {
                project.update_calculation(id, draft)?;
                Ok(structural(before, project, label))
            }
            Command::RemoveCalculation { id, with_columns } => {
                project.remove_calculation(id, with_columns)?;
                Ok(structural(before, project, label))
            }
            Command::TransposeRows { source, rows } => {
                project.transpose_rows(source, &rows)?;
                Ok(structural(before, project, label))
            }
            Command::RecalculateTranspose { sheet } => {
                project.recalculate_transpose(sheet)?;
                Ok(structural(before, project, label))
            }
            Command::SetCells { .. }
            | Command::SetCellsPromoting { .. }
            | Command::SetColumnType { .. }
            | Command::InsertRows { .. }
            | Command::DeleteRows { .. }
            | Command::DeleteCells { .. }
            | Command::SetWindowMeta { .. }
            | Command::SetNoteText { .. }
            | Command::SetColumnMeta { .. }
            | Command::SetPlotSpec { .. }
            | Command::EditPlotSpec { .. }
            | Command::SetSavedColors { .. } => {
                unreachable!("apply_structural called with a data command")
            }
        }
    }

    /// Applies a **data** command and builds its history entry. No
    /// `Arc<Tree>` parameter — see the doc of
    /// [`History::execute`], D3 (2nd half) of the guide: keeping it alive here
    /// would force `Arc::make_mut` to clone the whole `Tree` rather than
    /// mutate in place. Everything these commands read before mutating (the
    /// version of a column for `SetCells`) goes through a simple borrow of
    /// `project.tree()`, never kept beyond the line that reads it.
    ///
    /// Never called with a structural command — see
    /// [`History::apply_structural`].
    fn apply_data(
        project: &mut Project,
        command: Command,
        label: HistoryLabel,
    ) -> CommandResult<Entry> {
        match command {
            Command::SetCells {
                column,
                start,
                values,
                expected_version,
            } => {
                check_writable_cells(project, column, expected_version)?;
                Ok(Entry::Data {
                    delta: Box::new(write_cells(project, column, start, values)?),
                    label,
                })
            }
            Command::SetCellsPromoting {
                column,
                start,
                values,
                expected_version,
            } => {
                check_writable_cells(project, column, expected_version)?;
                let column_type = project
                    .tree()
                    .get(column)
                    .ok()
                    .and_then(|node| node.as_column())
                    .map(stokesforge_core::Column::column_type);
                let delta = match column_type {
                    // Path 1 — the block already fits, or the column is not
                    // one (let `set_values` return the natural error
                    // rather than invent one here, same discipline as for A2
                    // above).
                    None => write_cells(project, column, start, values)?,
                    Some(current) if values.fits(current) => {
                        write_cells(project, column, start, values)?
                    }
                    // Path 2 — the column is already mixed: only the block
                    // is converted, and the write stays a cell delta,
                    // proportional to what is written.
                    Some(ColumnType::Mixed) => write_cells(
                        project,
                        column,
                        start,
                        ValueBlock::Mixed(values.into_mixed()),
                    )?,
                    // Path 3 — promotion. The delta is that of
                    // `SetColumnType`: two column snapshots, because
                    // the conversion is not reversible by recomputation
                    // (see `DataDelta::ColumnType`). The write that follows is
                    // **inside** the same `after` snapshot, so in the same
                    // history entry: a single undo brings back both the type
                    // and the values from before.
                    Some(_) => {
                        let before = project.set_column_type(column, ColumnType::Mixed)?;
                        project.set_values(
                            column,
                            start,
                            ValueBlock::Mixed(values.into_mixed()),
                        )?;
                        let after = column_state(project, column)?;
                        DataDelta::ColumnType {
                            column,
                            before,
                            after,
                        }
                    }
                };
                Ok(Entry::Data {
                    delta: Box::new(delta),
                    label,
                })
            }
            Command::SetColumnType {
                column,
                column_type,
            } => {
                check_not_calculated(project, column)?;
                let before_state = project.set_column_type(column, column_type)?;
                let after_state = column_state(project, column)?;
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::ColumnType {
                        column,
                        before: before_state,
                        after: after_state,
                    }),
                    label,
                })
            }
            Command::InsertRows { sheet, at, count } => {
                // The columns actually extended are returned by
                // `Project::insert_rows` itself — the "`at < len`" rule
                // lives in this single place, in the core — and
                // undo takes them one by one (`delete_column_rows`)
                // rather than calling `delete_rows` again at sheet level.
                // This makes undo symmetric with that of
                // `DeleteRows`, where the sheet level is impossible anyway
                // (a column emptied to its end has no row `at` any more).
                // The "row" series of the project follow: specs recorded before
                // the mutation, rewritten after (by `set_plot_spec`).
                let moved = shifted_row_specs(project.tree(), sheet, |r| {
                    (r >= at).then(|| r.saturating_add(count))
                });
                let columns = project.insert_rows(sheet, at, count)?;
                let plots = write_row_specs(project, moved)?;
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::InsertRows {
                        columns,
                        at,
                        count,
                        plots,
                    }),
                    label,
                })
            }
            Command::DeleteRows { sheet, at, count } => {
                // A "row" series after the deleted range moves back by
                // `count`. A series whose row is inside the deleted range
                // stays as it is (open decision, #307 D1): it then plots
                // the row that took its place, or nothing beyond the
                // sheet (NaN, invariant 10).
                let moved = shifted_row_specs(project.tree(), sheet, |r| {
                    (r >= at.saturating_add(count)).then(|| r - count)
                });
                let removed = project.delete_rows(sheet, at, count)?;
                let plots = write_row_specs(project, moved)?;
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::DeleteRows {
                        sheet,
                        at,
                        count,
                        removed,
                        plots,
                    }),
                    label,
                })
            }
            Command::DeleteCells { columns, at, count } => {
                // Before any mutation: a refusal leaves no partial state.
                for column in &columns {
                    check_not_calculated(project, *column)?;
                }
                // Column by column, independently — unlike
                // `DeleteRows`, `columns` is not necessarily "all
                // the columns of the sheet": it is the explicit selection
                // of the caller (a column with no row `at`, or listed
                // twice, is silently ignored, as `row_targets`
                // already does for `DeleteRows`).
                let mut removed = Vec::new();
                let mut seen = HashSet::new();
                for column in columns {
                    if !seen.insert(column) {
                        continue;
                    }
                    let has_row = project
                        .tree()
                        .get(column)
                        .ok()
                        .and_then(|node| node.as_column())
                        .is_some_and(|c| at < c.len());
                    if count == 0 || !has_row {
                        continue;
                    }
                    let block = project.delete_column_rows(column, at, count)?;
                    removed.push((column, block));
                }
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::DeleteCells { at, count, removed }),
                    label,
                })
            }
            Command::SetWindowMeta { id, meta } => {
                let before = project.set_window_meta(id, meta.clone())?;
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::WindowMeta {
                        id,
                        before,
                        after: meta,
                    }),
                    label,
                })
            }
            Command::SetNoteText { id, text } => {
                let before = project.set_note_text(id, &text)?;
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::NoteText {
                        id,
                        before,
                        after: text,
                    }),
                    label,
                })
            }
            Command::SetColumnMeta { column, meta } => {
                // A formula on an output column of a calculation: two
                // sources of truth for the same cells.
                if meta.formula.is_some() {
                    if let Some((calculation, _)) = project.tree().calculation_of_column(column) {
                        return Err(CoreError::CalculatedColumnReadOnly {
                            column,
                            calculation,
                        }
                        .into());
                    }
                }
                let before = project.set_column_meta(column, meta.clone())?;
                // See the doc of `DataDelta::ColumnMeta::values_before`:
                // captured only at the transition "no formula before,
                // formula after" — it is the column that loses, at once,
                // its quality of source of truth in favor of the
                // recalculation (`stokesforge-session::recalc::recalc_formulas`,
                // outside the history). Read **after** `set_column_meta`: that
                // call never touches the values, only `meta`, so the order
                // changes nothing about what is captured here.
                let values_before = if before.formula.is_none() && meta.formula.is_some() {
                    Some(column_state(project, column)?)
                } else {
                    None
                };
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::ColumnMeta {
                        column,
                        before: Box::new(before),
                        after: Box::new(meta),
                        values_before,
                    }),
                    label,
                })
            }
            Command::SetPlotSpec { graph, spec } | Command::EditPlotSpec { graph, spec, .. } => {
                let before = project.set_plot_spec(graph, spec.clone())?;
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::PlotSpec {
                        graph,
                        before,
                        after: spec,
                    }),
                    label,
                })
            }
            Command::SetSavedColors { colors } => {
                let before = project.set_saved_colors(colors.clone());
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::SavedColors {
                        before,
                        after: colors.bounded(),
                    }),
                    label,
                })
            }
            Command::AddColumn { .. }
            | Command::DeleteColumn { .. }
            | Command::Rename { .. }
            | Command::Move { .. }
            | Command::AddFolder { .. }
            | Command::AddWorkbook { .. }
            | Command::AddEmptyWorkbook { .. }
            | Command::AddWorksheet { .. }
            | Command::DuplicateWorksheet { .. }
            | Command::InsertCopy { .. }
            | Command::AddGraph { .. }
            | Command::AddMatrix { .. }
            | Command::AddNote { .. }
            | Command::RemoveNode { .. }
            | Command::ReplaceAnalysisData { .. }
            | Command::AddCalculation { .. }
            | Command::RestoreOriginalData { .. }
            | Command::UpdateCalculation { .. }
            | Command::RemoveCalculation { .. }
            | Command::TransposeRows { .. }
            | Command::RecalculateTranspose { .. } => {
                unreachable!("apply_data called with a structural command")
            }
        }
    }

    /// Undoes an entry. A macro is undone in **reverse** order.
    fn revert(project: &mut Project, entry: &Entry) -> CommandResult<()> {
        match entry {
            Entry::Structural { before, .. } => {
                // A7 (`guide-de-correction-robustesse.md` §F2) —
                // `replace_tree` restores the `before` tree as it is,
                // version counters included: a column can then go
                // back to a number that a different content (its own,
                // at another moment of its history) already carries. We
                // remember the tree we leave before replacing it, to
                // be able to compare the two afterwards — see
                // `restamp_regressed_versions`.
                let discarded = Arc::clone(project.tree());
                project.replace_tree(Arc::clone(before));
                restamp_regressed_versions(project, &discarded, before)?;
                Ok(())
            }
            Entry::Data { delta, .. } => Self::revert_delta(project, delta),
            Entry::Macro { entries, .. } => {
                for entry in entries.iter().rev() {
                    Self::revert(project, entry)?;
                }
                Ok(())
            }
        }
    }

    /// Redoes an entry. A macro is replayed in **forward** order.
    fn reapply(project: &mut Project, entry: &Entry) -> CommandResult<()> {
        match entry {
            Entry::Structural { after, .. } => {
                project.replace_tree(Arc::clone(after));
                Ok(())
            }
            Entry::Data { delta, .. } => Self::reapply_delta(project, delta),
            Entry::Macro { entries, .. } => {
                for entry in entries {
                    Self::reapply(project, entry)?;
                }
                Ok(())
            }
        }
    }

    /// Replays a data delta backwards, in the current tree.
    fn revert_delta(project: &mut Project, delta: &DataDelta) -> CommandResult<()> {
        match delta {
            DataDelta::Cells {
                column,
                start,
                before,
                before_length,
                ..
            } => {
                // Two steps: rewrite what was overwritten, then give the
                // column back its length (a write beyond the end had
                // extended it, and `set_values` never shortens).
                project.set_values(*column, *start, before.clone())?;
                project.set_length(*column, *before_length)?;
                Ok(())
            }
            DataDelta::InsertRows {
                columns,
                at,
                count,
                plots,
            } => {
                for column in columns {
                    project.delete_column_rows(*column, *at, *count)?;
                }
                for (graph, before, _) in plots {
                    project.set_plot_spec(*graph, before.clone())?;
                }
                Ok(())
            }
            DataDelta::DeleteRows {
                at, removed, plots, ..
            } => {
                for (column, block) in removed {
                    project.insert_column_rows(*column, *at, block.len())?;
                    project.set_values(*column, *at, block.clone())?;
                }
                for (graph, before, _) in plots {
                    project.set_plot_spec(*graph, before.clone())?;
                }
                Ok(())
            }
            DataDelta::DeleteCells { at, removed, .. } => {
                for (column, block) in removed {
                    project.insert_column_rows(*column, *at, block.len())?;
                    project.set_values(*column, *at, block.clone())?;
                }
                Ok(())
            }
            DataDelta::ColumnType { column, before, .. } => {
                project.restore_column(*column, before.0, before.1.clone())?;
                Ok(())
            }
            DataDelta::WindowMeta { id, before, .. } => {
                project.set_window_meta(*id, before.clone())?;
                Ok(())
            }
            DataDelta::NoteText { id, before, .. } => {
                project.set_note_text(*id, before)?;
                Ok(())
            }
            DataDelta::ColumnMeta {
                column,
                before,
                values_before,
                ..
            } => {
                project.set_column_meta(*column, ColumnMeta::clone(before))?;
                // See the doc of `DataDelta::ColumnMeta::values_before`: the
                // column becomes an ordinary source of truth again, its
                // values from before the formula must come back with it —
                // otherwise the recalculation that follows (`stokesforge-session::recalc::
                // recalc_formulas`) ignores a column that no longer has a
                // formula and leaves the computed values in place.
                if let Some((column_type, values)) = values_before {
                    project.restore_column(*column, *column_type, values.clone())?;
                }
                Ok(())
            }
            DataDelta::PlotSpec { graph, before, .. } => {
                project.set_plot_spec(*graph, before.clone())?;
                Ok(())
            }
            DataDelta::SavedColors { before, .. } => {
                project.set_saved_colors(before.clone());
                Ok(())
            }
        }
    }

    /// Replays a data delta forwards, in the current tree.
    fn reapply_delta(project: &mut Project, delta: &DataDelta) -> CommandResult<()> {
        match delta {
            DataDelta::Cells {
                column,
                start,
                after,
                ..
            } => {
                // The length follows the write: no `set_length` here.
                project.set_values(*column, *start, after.clone())?;
                Ok(())
            }
            DataDelta::InsertRows {
                columns,
                at,
                count,
                plots,
            } => {
                for column in columns {
                    project.insert_column_rows(*column, *at, *count)?;
                }
                for (graph, _, after) in plots {
                    project.set_plot_spec(*graph, after.clone())?;
                }
                Ok(())
            }
            DataDelta::DeleteRows {
                at, removed, plots, ..
            } => {
                // Column by column, and not `delete_rows` at sheet level:
                // only the columns actually affected the first time
                // must be affected again.
                for (column, block) in removed {
                    project.delete_column_rows(*column, *at, block.len())?;
                }
                for (graph, _, after) in plots {
                    project.set_plot_spec(*graph, after.clone())?;
                }
                Ok(())
            }
            DataDelta::DeleteCells { at, removed, .. } => {
                // Column by column, as for `DeleteRows`: only the
                // columns actually affected the first time are affected
                // again.
                for (column, block) in removed {
                    project.delete_column_rows(*column, *at, block.len())?;
                }
                Ok(())
            }
            DataDelta::ColumnType { column, after, .. } => {
                project.restore_column(*column, after.0, after.1.clone())?;
                Ok(())
            }
            DataDelta::WindowMeta { id, after, .. } => {
                project.set_window_meta(*id, after.clone())?;
                Ok(())
            }
            DataDelta::NoteText { id, after, .. } => {
                project.set_note_text(*id, after)?;
                Ok(())
            }
            DataDelta::ColumnMeta { column, after, .. } => {
                project.set_column_meta(*column, ColumnMeta::clone(after))?;
                Ok(())
            }
            DataDelta::PlotSpec { graph, after, .. } => {
                project.set_plot_spec(*graph, after.clone())?;
                Ok(())
            }
            DataDelta::SavedColors { after, .. } => {
                project.set_saved_colors(after.clone());
                Ok(())
            }
        }
    }
}

impl Default for History {
    /// New stack with the two default limits ([`DEFAULT_LIMIT`],
    /// [`DEFAULT_BYTE_LIMIT`]).
    fn default() -> Self {
        Self::new()
    }
}

/// Structural entry: the tree before and the tree after, two pointers.
fn structural(before: &Arc<Tree>, project: &Project, label: HistoryLabel) -> Entry {
    Entry::Structural {
        before: Arc::clone(before),
        after: Arc::clone(project.tree()),
        label,
    }
}

/// True if `entry`, although produced by an accepted command, concretely
/// changed nothing (F5, `guide-de-correction-robustesse.md`): such an
/// entry must not take an undo slot.
///
/// - `Structural`: `before == after` covers any structural command
///   that leaves the tree identical — `Rename` to the name already held
///   ("Rename M to M", §7.3 of the guide) is one case, `Move` to its current
///   position would be another, without either needing
///   to be recognized by name: the structural comparison of the two trees
///   (`Tree` derives `PartialEq`) catches both in the same way.
/// - `Data`: only `InsertRows`/`DeleteRows` can target zero columns
///   (see `Project::row_targets`) — `Cells` and `ColumnType` always
///   write something once `apply` has accepted them.
/// - `Macro`: a macro closed with at least one entry has, by
///   construction, applied at least one real delta (`end_macro` already filters
///   empty macros) — never a no-op at this level.
fn entry_has_no_effect(entry: &Entry) -> bool {
    match entry {
        Entry::Structural { before, after, .. } => before == after,
        Entry::Data { delta, .. } => match delta.as_ref() {
            DataDelta::InsertRows { columns, .. } => columns.is_empty(),
            DataDelta::DeleteRows { removed, .. } => removed.is_empty(),
            DataDelta::DeleteCells { removed, .. } => removed.is_empty(),
            // Like `Cells`/`ColumnType`: once `apply` has accepted them,
            // these four commands have always replaced something (a
            // replacement by an equal value is still a replacement, not
            // a special case to detect here).
            DataDelta::Cells { .. }
            | DataDelta::ColumnType { .. }
            | DataDelta::WindowMeta { .. }
            | DataDelta::NoteText { .. }
            | DataDelta::ColumnMeta { .. }
            | DataDelta::PlotSpec { .. }
            | DataDelta::SavedColors { .. } => false,
        },
        Entry::Macro { .. } => false,
    }
}

/// A column produced by an analysis calculation (#181) also refuses
/// writes that would change its shape (type, deleted cells): the
/// recalculation would undo them, or fail without saying so (`TypeMismatch` on a
/// column switched to text).
fn check_not_calculated(project: &Project, column: NodeId) -> CommandResult<()> {
    if let Some((calculation, _)) = project.tree().calculation_of_column(column) {
        return Err(CoreError::CalculatedColumnReadOnly {
            column,
            calculation,
        }
        .into());
    }
    Ok(())
}

/// The two refusals that every cell write — `SetCells` as well as
/// `SetCellsPromoting` — must raise **before** touching the model, so
/// that a failure leaves no partial state.
///
/// 1. **A2** (`guide-de-correction-robustesse.md` §F6) — the optimistic
///    condition `expected_version`, checked HERE, on the current tree at the
///    moment of applying this command (not a snapshot that the caller
///    read earlier), never before sending: that is the whole point of the fix.
/// 2. **Calculated columns** (v1, `stokesforge_core::formula`) — a
///    formula column accepts direct cell writes from no
///    client: its values are recalculated only by
///    `stokesforge-session::actor` afterwards, outside the history.
///    Same refusal for a column produced by an analysis calculation (#181).
///
/// A missing column or one of another type is handled by neither
/// one: it is `Project::set_values`, afterwards, that will return the
/// natural error (`UnknownNode`/`Structure`) — which must not be
/// disguised as a version disagreement.
fn check_writable_cells(
    project: &Project,
    column: NodeId,
    expected_version: Option<u64>,
) -> CommandResult<()> {
    let target = project
        .tree()
        .get(column)
        .ok()
        .and_then(|node| node.as_column());
    if let (Some(expected), Some(found)) = (
        expected_version,
        target.map(stokesforge_core::Column::version),
    ) {
        if found != expected {
            return Err(CoreError::ConcurrentModification {
                column,
                expected,
                found,
            }
            .into());
        }
    }
    if target.is_some_and(|column| column.meta().formula.is_some()) {
        return Err(CoreError::FormulaColumnReadOnly { column }.into());
    }
    if let Some((calculation, _)) = project.tree().calculation_of_column(column) {
        return Err(CoreError::CalculatedColumnReadOnly {
            column,
            calculation,
        }
        .into());
    }
    Ok(())
}

/// Writes `values` and builds the matching [`DataDelta::Cells`]. The block
/// is cloned before applying: it is the `after` of the delta, what will have
/// to be rewritten to redo.
fn write_cells(
    project: &mut Project,
    column: NodeId,
    start: usize,
    values: ValueBlock,
) -> CommandResult<DataDelta> {
    let after = values.clone();
    let delta = project.set_values(column, start, values)?;
    Ok(DataDelta::Cells {
        column,
        start,
        before: delta.previous,
        before_length: delta.previous_length,
        after,
    })
}

/// Current type and values of a column (snapshot of a type change).
/// Specs of the graphs where at least one "row" series (`row = Some(r)`,
/// `y` anchor in `sheet`) moves according to `shift` (`Some(new row)` or
/// `None` to leave it). Pure: reads the tree, mutates nothing. A broken
/// anchor (missing column) is ignored. Returns `(graph, before, after)` for
/// the modified graphs only.
fn shifted_row_specs(
    tree: &Tree,
    sheet: NodeId,
    shift: impl Fn(usize) -> Option<usize>,
) -> Vec<(NodeId, PlotSpec, PlotSpec)> {
    let mut out = Vec::new();
    for id in tree.walk_all() {
        let Ok(node) = tree.get(id) else { continue };
        let stokesforge_core::NodeKind::Graph { spec, .. } = node.kind() else {
            continue;
        };
        let mut after = spec.clone();
        let mut changed = false;
        for panel in after.panels.iter_mut() {
            for series in &mut panel.series {
                let Some(row) = series.row else { continue };
                if tree.parent(series.y).ok().flatten() != Some(sheet) {
                    continue;
                }
                if let Some(new_row) = shift(row) {
                    series.row = Some(new_row);
                    changed = true;
                }
            }
        }
        if changed {
            out.push((id, spec.clone(), after));
        }
    }
    out
}

/// Writes the specs recorded by [`shifted_row_specs`] (through
/// `Project::set_plot_spec`, invariant 1) and returns the `plots` delta.
fn write_row_specs(
    project: &mut Project,
    moved: Vec<(NodeId, PlotSpec, PlotSpec)>,
) -> CoreResult<Vec<(NodeId, PlotSpec, PlotSpec)>> {
    for (graph, _, after) in &moved {
        project.set_plot_spec(*graph, after.clone())?;
    }
    Ok(moved)
}

fn column_state(project: &Project, column: NodeId) -> CoreResult<(ColumnType, ColumnValues)> {
    let node = project.tree().get(column)?;
    let target = node
        .as_column()
        .ok_or_else(|| CoreError::Structure(format!("{column} is not a column")))?;
    Ok((target.column_type(), target.values().clone()))
}

// ===========================================================================
// A7 — the version counter of a column never goes back (§F2)
// ===========================================================================
//
// `Entry::Structural::revert` replaces the whole tree (`Project::replace_tree`),
// which also restores the version counter of each column it carries
// — see the doctrine of the stamps in the guide (§F2, table): a content
// version must be monotonic and never designate two contents. Restoring an
// old tree can make the counter go back for a column whose content has,
// in the meantime, diverged (by a normal write, or by the undo of a
// normal write — « annuler est une écriture de plus », §6 of the guide):
// a future write starting again from this lower number can then land on
// a number already carried by another content of that same column.
//
// Two earlier versions of this fix computed a "fresh" number
// locally — first from the two trees directly involved in the
// restoration, then from everything the history still kept — and
// both proved insufficient: a random sweep
// (`b14_une_version_ne_designe_jamais_deux_contenus`) found that an
// ORDINARY write (never through a `revert`) can, for its part,
// advance a column to a number that neither local
// computation could foresee. The only computation that closes the question for
// good is a single counter held by `Project` itself
// (`Project::next_column_version`, doctrine of the stamps of the guide:
// « attribuée par le projet ») and consulted by **every** column
// write, ordinary or restoration — never a computation derived from a
// subset of what the project has seen.

/// Reassigns a **fresh** version number (drawn from [`Project::
/// next_column_version`]) to each column whose buffer is **not** the
/// same object in `discarded` and `restored` — called just after
/// `revert` has replaced the current tree (`discarded`, the one we leave) by
/// `restored`.
///
/// Compared by **pointer identity** (see [`column_footprint`]), never
/// by equality of values: two distinct buffers that happened to contain
/// the same bytes remain two different allocations, so two distinct
/// moments of the history of this column (copy-on-write
/// creates a new one as soon as a shared buffer is modified — see
/// `Column::set_values`); its counter may therefore have advanced without
/// `restored` (frozen when the entry was pushed) knowing it, and
/// could go back if we do not touch it. A column whose buffer
/// stayed the **same** object in both trees has suffered no write
/// between the two: nothing could advance, nothing can go back, and
/// leaving it intact avoids an `Arc::make_mut` for nothing (D3 of the guide
/// already watches over this kind of cost). A column absent from `discarded` (it
/// has just reappeared) or from `restored` (it disappears) is never
/// concerned: see the body.
fn restamp_regressed_versions(
    project: &mut Project,
    discarded: &Tree,
    restored: &Tree,
) -> CommandResult<()> {
    let mut targets = Vec::new();
    for (id, node) in restored.iter() {
        let Some(restored_column) = node.as_column() else {
            continue;
        };
        // Absent from `discarded`: it has just reappeared (undo
        // of a deletion, for example) — nothing could be written to a
        // node that did not exist yet, its counter cannot have
        // regressed.
        let Some(discarded_column) = discarded.node(id).and_then(|node| node.as_column()) else {
            continue;
        };
        let same_buffer = column_footprint(discarded_column.values()).0
            == column_footprint(restored_column.values()).0;
        if !same_buffer {
            targets.push(id);
        }
    }
    for id in targets {
        let fresh = project.next_column_version();
        project.restamp_column_version(id, fresh)?;
    }
    Ok(())
}

// ===========================================================================
// Memory footprint of entries
// ===========================================================================
//
// The principle: each entry knows, computed once and for all when it is
// pushed, the set of buffers it keeps — a collection of
// (address, bytes) pairs, the address serving only as identity. The total
// retained by `History` (see `History::recompute_retained_bytes`) is
// then the union of these already known sets, minus the buffers of the
// current tree: an operation on hash tables, never a new
// walk of the trees kept by the entries.

/// Footprint of an entry freshly built by [`History::apply`] —
/// always `Structural` or `Data`, never `Macro` (a macro is produced
/// only by [`History::end_macro`], from the already known footprints of
/// its entries, without going through this function again).
fn footprint_of(entry: &Entry) -> Footprint {
    match entry {
        Entry::Structural { before, after, .. } => {
            // Structure: the buffer of each column of the tree before
            // **and** of the tree after — an immutable tree keeps all its
            // nodes, so all its columns.
            let mut out = Footprint::new();
            insert_tree_footprint(&mut out, before);
            insert_tree_footprint(&mut out, after);
            out
        }
        // Data: no tree kept, so no column buffer — the own
        // cost is that of the values of the delta (old and
        // new), directly known.
        Entry::Data { delta, .. } => data_delta_footprint(delta),
        Entry::Macro { entries, .. } => {
            // Does not happen in practice (see the doc of the function): a
            // correct safety net — the recursive union of the children — rather
            // than a panic.
            let mut out = Footprint::new();
            for child in entries {
                out.extend(footprint_of(child));
            }
            out
        }
    }
}

/// Union of several already known footprints (the entries of a macro that
/// is closing, for example): a merge of hash tables, never a
/// new tree walk.
fn union_footprint<'a>(footprints: impl Iterator<Item = &'a Footprint>) -> Footprint {
    let mut out = Footprint::new();
    for footprint in footprints {
        out.extend(footprint.iter().map(|(&ptr, &bytes)| (ptr, bytes)));
    }
    out
}

/// Adds to `out` the buffer of each column of `tree`.
fn insert_tree_footprint(out: &mut Footprint, tree: &Tree) {
    for (_, node) in tree.iter() {
        if let Some(column) = node.as_column() {
            insert_column_footprint(out, column.values());
        }
    }
}

/// Adds to `out` the buffer of a column: address and size in bytes.
fn insert_column_footprint(out: &mut Footprint, values: &ColumnValues) {
    let (ptr, bytes) = column_footprint(values);
    out.insert(ptr, bytes);
}

/// Address (identity — never dereferenced) and size in bytes of the buffer
/// of a column. `8 × length` for a float buffer; for a
/// mixed column, an estimate (see [`mixed_buffer_bytes`]), the exact
/// allocation cost per `String` not being modelable independently of
/// the allocator.
fn column_footprint(values: &ColumnValues) -> (usize, usize) {
    match values {
        ColumnValues::Float(arc) => (
            Arc::as_ptr(arc) as usize,
            arc.len() * std::mem::size_of::<f64>(),
        ),
        ColumnValues::Mixed(arc) => (Arc::as_ptr(arc) as usize, mixed_buffer_bytes(arc)),
    }
}

/// Addresses of the column buffers present in `tree` — the right side of
/// the subtraction: a buffer that the current tree still uses is not
/// a cost of the history, it would be there anyway.
fn current_tree_pointers(tree: &Tree) -> HashSet<usize> {
    tree.iter()
        .filter_map(|(_, node)| node.as_column())
        .map(|column| column_footprint(column.values()).0)
        .collect()
}

/// Footprint of a data delta: no tree kept, so no shared column
/// buffer — only the values specific to the delta count
/// (old **and** new, when they exist), directly known
/// without needing to deduplicate them with anything else
/// (they are never shared with the tree).
fn data_delta_footprint(delta: &DataDelta) -> Footprint {
    let mut out = Footprint::new();
    match delta {
        DataDelta::Cells { before, after, .. } => {
            insert_block_footprint(&mut out, before);
            insert_block_footprint(&mut out, after);
        }
        DataDelta::InsertRows { .. } => {
            // Nothing to keep: `InsertRows` records only the affected
            // columns, the row and the count — no stored value (the
            // inserted rows are missing by construction).
        }
        DataDelta::DeleteRows { removed, .. } | DataDelta::DeleteCells { removed, .. } => {
            for (_, block) in removed {
                insert_block_footprint(&mut out, block);
            }
        }
        DataDelta::ColumnType { before, after, .. } => {
            // Only data delta that carries a `ColumnValues` (so an
            // `Arc` potentially shared with the current tree, for
            // example just after the conversion) rather than a `ValueBlock`
            // owned by itself: same function as for a tree
            // column, so that deduplication by address plays its role.
            insert_column_footprint(&mut out, &before.1);
            insert_column_footprint(&mut out, &after.1);
        }
        // Window/note metadata and graph spec: no buffer
        // shared with the tree to account for, unlike
        // `ColumnType` — just a few strings or identities, never the
        // volume that justifies this mechanism (data columns). Same
        // choice as `InsertRows` above.
        DataDelta::WindowMeta { .. }
        | DataDelta::NoteText { .. }
        | DataDelta::PlotSpec { .. }
        | DataDelta::SavedColors { .. } => {}
        // `ColumnMeta` joins `ColumnType` as soon as it carries `values_before`
        // (see its doc): it is then, too, a `ColumnValues`
        // potentially shared with the current tree.
        DataDelta::ColumnMeta { values_before, .. } => {
            if let Some((_, values)) = values_before {
                insert_column_footprint(&mut out, values);
            }
        }
    }
    out
}

/// Columns that `command` could touch, with the address and size of the
/// buffer each one carries **before** its application — a simple borrow of
/// `tree`, never kept alive beyond this call (see [`PriorColumns`] and
/// the doc of [`History::execute`], D3 (2nd half) of the guide).
///
/// Targets exactly the column(s) that the command *names* (`column`
/// for `SetCells`/`SetColumnType`) or, for `InsertRows`/`DeleteRows` which
/// know their exact targets only once `Project::row_targets` has been
/// consulted (private to `stokesforge-core`), **all** the columns of the
/// targeted sheet — a superset bounded by the size of that sheet, never by
/// that of the project. [`just_orphaned_by_write`] will then look in it only for
/// the columns that the delta, once the command is applied, says were really
/// touched: the superset never marks as orphaned a buffer that
/// is not one.
///
/// Empty for a structural command (it is never called with one
/// from [`History::execute`], but staying total here avoids a
/// panic at no cost).
fn prior_columns(tree: &Tree, command: &Command) -> PriorColumns {
    let mut out = PriorColumns::new();
    let mut record = |id: NodeId| {
        if let Some(column) = tree.node(id).and_then(|node| node.as_column()) {
            out.insert(id, column_footprint(column.values()));
        }
    };
    match command {
        Command::SetCells { column, .. }
        | Command::SetCellsPromoting { column, .. }
        | Command::SetColumnType { column, .. } => {
            record(*column);
        }
        Command::InsertRows { sheet, .. } | Command::DeleteRows { sheet, .. } => {
            if let Ok(children) = tree.children(*sheet) {
                for &id in children {
                    record(id);
                }
            }
        }
        Command::DeleteCells { columns, .. } => {
            for &id in columns {
                record(id);
            }
        }
        // None of these commands targets a column whose buffer would have
        // to be recorded before applying: the eight structural ones
        // never reach this function (see the doc above), and the
        // four metadata/spec commands never touch the values of a column
        // — `SetColumnMeta` replaces `ColumnMeta`, not
        // `ColumnValues`.
        Command::AddColumn { .. }
        | Command::DeleteColumn { .. }
        | Command::Rename { .. }
        | Command::Move { .. }
        | Command::AddFolder { .. }
        | Command::AddWorkbook { .. }
        | Command::AddEmptyWorkbook { .. }
        | Command::AddWorksheet { .. }
        | Command::DuplicateWorksheet { .. }
        | Command::InsertCopy { .. }
        | Command::AddGraph { .. }
        | Command::AddMatrix { .. }
        | Command::AddNote { .. }
        | Command::RemoveNode { .. }
        | Command::ReplaceAnalysisData { .. }
        | Command::AddCalculation { .. }
        | Command::RestoreOriginalData { .. }
        | Command::UpdateCalculation { .. }
        | Command::RemoveCalculation { .. }
        | Command::TransposeRows { .. }
        | Command::RecalculateTranspose { .. }
        | Command::SetWindowMeta { .. }
        | Command::SetNoteText { .. }
        | Command::SetColumnMeta { .. }
        | Command::SetPlotSpec { .. }
        | Command::EditPlotSpec { .. }
        | Command::SetSavedColors { .. } => {}
    }
    out
}

/// Buffer(s) that the columns touched by `delta` had in `prior`
/// (captured by [`prior_columns`] before this command) — what
/// copy-on-write has just replaced. A buffer that is not in it
/// was not already pinned (see [`History::pin_footprint`], which does
/// nothing for an unknown pointer): looking for it is therefore safe even when
/// nothing needs to flip to orphaned.
///
/// A single lookup by identifier in `prior` per touched column —
/// never a tree walk — hence the name: `just_orphaned_by_write`
/// gives the buffers that *have just* left the current tree at the instant
/// of this write, not all those that an older entry keeps.
fn just_orphaned_by_write(prior: &PriorColumns, delta: &DataDelta) -> Footprint {
    let mut out = Footprint::new();
    let previous_buffer_of = |out: &mut Footprint, column: NodeId| {
        if let Some(&(ptr, bytes)) = prior.get(&column) {
            out.insert(ptr, bytes);
        }
    };
    match delta {
        DataDelta::Cells { column, .. } | DataDelta::ColumnType { column, .. } => {
            previous_buffer_of(&mut out, *column);
        }
        DataDelta::InsertRows { columns, .. } => {
            for &column in columns {
                previous_buffer_of(&mut out, column);
            }
        }
        DataDelta::DeleteRows { removed, .. } | DataDelta::DeleteCells { removed, .. } => {
            for (column, _) in removed {
                previous_buffer_of(&mut out, *column);
            }
        }
        // See `prior_columns` and `data_delta_footprint`: these four deltas
        // never carry a column buffer, nothing to orphan here.
        DataDelta::WindowMeta { .. }
        | DataDelta::NoteText { .. }
        | DataDelta::ColumnMeta { .. }
        | DataDelta::PlotSpec { .. }
        | DataDelta::SavedColors { .. } => {}
    }
    out
}

/// Adds to `out` the block `block`: the address of its `Vec` (never shared
/// with the tree — a delta `ValueBlock` is always owned by itself,
/// so never net by subtraction against the current tree) and its size
/// in bytes.
fn insert_block_footprint(out: &mut Footprint, block: &ValueBlock) {
    let (ptr, bytes) = block_footprint(block);
    out.insert(ptr, bytes);
}

/// Address and size in bytes of a `ValueBlock`.
fn block_footprint(block: &ValueBlock) -> (usize, usize) {
    match block {
        ValueBlock::Float(values) => (
            values.as_ptr() as usize,
            values.len() * std::mem::size_of::<f64>(),
        ),
        ValueBlock::Mixed(values) => (values.as_ptr() as usize, mixed_buffer_bytes(values)),
    }
}

/// Fixed cost per element of a mixed-type buffer: the size of a
/// [`MixedCell`] in the `Vec`, that of its biggest variant
/// (`String`, `{ptr, len, cap}`) plus the discriminant — an `f64` opens
/// no usable niche, unlike the pointer of a `String`, so
/// this is not the size of an `Option<String>`.
const MIXED_ELEMENT_OVERHEAD: usize = std::mem::size_of::<MixedCell>();

/// Estimate of the size of a mixed-type buffer: sum of the UTF-8 bytes
/// of the texts present, plus [`MIXED_ELEMENT_OVERHEAD`] per cell,
/// whatever its variant.
fn mixed_buffer_bytes(values: &[MixedCell]) -> usize {
    values.iter().fold(0usize, |total, cell| {
        total
            + MIXED_ELEMENT_OVERHEAD
            + match cell {
                MixedCell::Text(text) => text.len(),
                MixedCell::Empty | MixedCell::Number(_) => 0,
            }
    })
}
