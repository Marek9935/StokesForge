# Commands and history

`stokesforge-commands` is the **only** layer that mutates a `Project` outside
tests. This is the golden rule of the foundation (invariant 1), and this crate
is its mandatory point of passage.

## A command is a value

Not a stateful object: it holds no `Project`, no `Tree`, not even an `Arc`.
This is what lets it cross an inter-thread channel as is, from the UI to the
actor that holds the one `&mut Project`.

Everything needed to undo is computed **at application time**, by `History`,
never carried by the command.

Authoritative list: `stokesforge-commands/src/command.rs` (`Command`,
`Command::is_structural`). Summary:

| Command | Nature |
|---|---|
| `SetCells { column, start, values, expected_version }` | data |
| `SetCellsPromoting { column, start, values, expected_version }` | data |
| `AddColumn { sheet, index, name, column }` | structure |
| `DeleteColumn { column }` | structure |
| `Rename { id, name }` | structure |
| `Move { id, new_parent, index }` | structure |
| `SetColumnType { column, column_type }` | data |
| `InsertRows { sheet, … }` | data |
| `DeleteRows { sheet, … }` | data |
| `AddFolder { parent, name }` | structure |
| `AddWorkbook { parent, name }` | structure |
| `AddEmptyWorkbook { parent, name }` | structure |
| `AddWorksheet { book, name }` | structure |
| `DuplicateWorksheet { sheet, name }` | structure (the name "copy N" is composed by the application: an interface text) |
| `AddGraph { parent, name, spec }` | structure |
| `AddMatrix { parent, name }` | structure |
| `AddNote { parent, name, text }` | structure |
| `RemoveNode { id }` | structure |
| `SetWindowMeta { id, meta }` | data |
| `SetNoteText { id, text }` | data |
| `SetColumnMeta { column, meta }` | data |
| `SetPlotSpec { graph, spec }` | data |
| `EditPlotSpec { graph, spec, kind }` | data (same spec delta, with a `PlotEditKind` reason) |
| `SetSavedColors { colors }` | data |

The empty graph of "File › Add Graph" (#73) reuses `AddGraph` with
`PlotSpec::empty("")`: one plot area, no series, the same undo and persistence
as a graph built from columns. "New Project" replaces the identity of the
project through the session, like `load`; it clears the history after
confirmation and is not an undoable command.

One creation command per node kind rather than a generic `AddNode` (see the
head doc of `command.rs`); `RemoveNode` stays generic, like `Project::remove`.
The four content-editing commands
(`SetWindowMeta`/`SetNoteText`/`SetColumnMeta`/`SetPlotSpec`) replace their
target **as a whole**, like `SetCells` — no per-subfield editing.

### Two cell writes, strict and tolerant

`SetCells` refuses a block that does not match the column type
(`CoreError::TypeMismatch`), which is what a caller who knows what it writes
wants — the Python bridge, a script. `SetCellsPromoting` **widens the column**
to `ColumnType::Mixed` instead of refusing: it is the command of grid input,
where typing "n/a" in a column of measurements must produce a text cell, not
an error
([`data-model.md`](data-model.md)).

The choice of path is made at **application time**, never at the call:

| Situation | Delta | Cost |
|---|---|---|
| the block already fits the type | `Cells` | that of `SetCells`, identical |
| the column is already mixed, the block does not fit | `Cells` | same: only the block is converted |
| the column is not mixed and the block does not fit it | `ColumnType` | one column snapshot, like `SetColumnType`, which it contains |

The third path happens only **once per column**: the next tolerant input finds
a column that is already mixed and falls back on the second path. And the
conversion and the write fit in the **same** history entry, so a single
`Ctrl+Z` brings back the type *and* the previous value.

`expected_version` is the optimistic condition: `None` checks nothing (the
behavior of the UI, which has no such discipline); `Some(v)` requires that
`Column::version` still equals exactly `v` **at the time of applying**, never
before — otherwise the command is refused
(`CoreError::ConcurrentModification`) without touching the model or the
history. This is what `stokesforge-script` fills in to close the TOCTOU between
the snapshot a script has read and the command it sends.

This counter also serves the targeted recalculation of formula columns (#193,
[`session.md`](session.md)): the actor recalculates a formula only if the
version of one of its sources has changed. So every data write to a column —
command, undo, redo — must receive a fresh number from the single counter of
the project (`Project::next_column_version`), a column inserted with a number
(`AddColumn`, paste) moves this counter beyond it (`Project::insert_node`), and
a tree restore must bring back only (version, content) pairs that really
coexisted (`restamp_regressed_versions`). A command that wrote cells any other
way would make a stale formula pass for up to date (invariant 11).

## The history is hybrid, and that is a cost decision

Structure and data do not have the same price.

| Nature | What is kept | Why |
|---|---|---|
| **structure** (tree, names, order, add/remove) | two `Arc<Tree>`, `before` and `after` | the tree is persistent: both versions share everything that did not move, keeping both costs one pointer |
| **data** (column content) | a **delta**: old *and* new values of the written cells only | a snapshot would keep the whole column buffer — 8 MB for a million points — at every keystroke |

Two pleasant consequences on the structure side: the `before` tree keeps the
removed nodes, so there is nothing to free explicitly; and a restored node
comes back under its **original identifier**, since `replace_tree` does not
move the allocator back.

### What the hybrid imposes: strict LIFO

Restoring a whole tree overwrites everything written after it. A structural
entry can therefore be undone only once all later entries have been undone —
guaranteed by construction, and **inside a macro the entries are undone
strictly in reverse order of insertion**, whatever their natures. This is what
makes the hybrid hold (invariant 8).

## Two limits, the most constraining wins

A limit counted in **entries** (`DEFAULT_LIMIT` = 100) does not see the pattern
that `tests/memory_pinning.rs` measures: by alternating a structure operation
and a write on the same large column, each structural entry pins a full
version of the buffer that the next write replaces. A hundred renames weigh
nothing; a single workbook deletion weighs hundreds of megabytes.

Hence `byte_limit` (`DEFAULT_BYTE_LIMIT` = 512 MiB), which directly bounds the
payload bytes that the history **alone** keeps alive
(`History::retained_bytes`, always exact through incremental tracking).

Three eviction rules to know, all deliberate:

- **the last remaining entry is never evicted**, even if it alone exceeds the
  limit — otherwise the limit produces exactly the nuisance it tries to avoid:
  nothing left to undo right after the heavy operation;
- eviction **never** touches the redo stack, which is cleared only by the next
  push. A long `redo` branch can therefore keep the total above the cap until
  the next mutation;
- `set_limit`/`set_byte_limit` refuse (`StackNotEmpty`) if a stack is not
  empty: deciding what to evict in the middle of a history does not belong to
  the stack.

## History labels: data, not a sentence

Each entry carries a `HistoryLabel` (`stokesforge-commands/src/label.rs`): an
enumeration — the action and its parameters (`InsertRows { count }`,
`Rename { from, to }`, `SetCells { column: Option<ColumnRef> }`…), plus
`Macro(MacroKind)` for a group. `Command::label(&Tree)` computes it on the
tree **from before** (the name of a removed node no longer exists afterwards)
and cannot fail: a node not found gives `None`, which the sentence renders in
a generic form. A column is designated by its position (`ColumnRef`, grid
letter), never by its node name.

The crate puts **nothing** into a sentence and does not depend on Fluent:

| Where | Role |
|---|---|
| `stokesforge-commands` | renders the data; `Display` in **English** for logs and the CLI |
| `stokesforge-app-state/src/history_text.rs` | `history_text`: the variant as a sentence, through `tr!` and the `history-*` keys of the `en` catalog, Fluent plurals; exhaustive `match` |
| `stokesforge-app-state/tests/history_text.rs` | the `en` catalog and `Display` say the same thing, variant by variant |

`begin_macro` takes a `MacroKind`, a **closed** enumeration (paste, clear the
selection, graph properties, refresh, import, create a workbook or a sheet, run
a script, delete or move columns): a group is a value that can be compared
(`undo_label ==
Some(HistoryLabel::Macro(MacroKind::Import))`), not a text to reread. A new
group adds a `MacroKind` variant, its `history-macro-*` key and its `Display`
line. The history is not written to the `.stf`
([`persistence.md`](persistence.md)).

### A data command that moves rows rewrites the row references (#307)

`DataDelta::InsertRows` and `DeleteRows` carry `plots: Vec<(NodeId, PlotSpec,
PlotSpec)>` (graph, spec before, spec after): the "row" series
(`row = Some(r)`) whose `y` anchor is in the sheet follow the shift. The
specs are collected before the data mutation, rewritten afterwards by
`Project::set_plot_spec` (invariant 1), and captured at application time
(invariant 8): `revert` puts back the before, `reapply` the after, with no
recomputed inverse and no `Entry::Macro` (the label "Delete N rows" stays). A
broken anchor is ignored; a row inside the deleted range stays as it is (D1
open).

## Macros

`begin_macro` / `end_macro` / `abort_macro` group commands into a single undoable
entry. Nesting is allowed up to `MAX_MACRO_DEPTH` (1000) — a deeper macro is
refused rather than overflowing the call stack.

Note: a macro never prevents the `Project` from being mutated; it **only defers
the history entry**. This is why `save` is refused during an open macro, and
why a macro still open when the actor stops is **aborted, not committed**
([`session.md`](session.md)).

What the identity of the client changes during an open macro — who can run,
close, abort — belongs to the actor and is read in the table of
`stokesforge-session/src/lib.rs`.

## Errors

Model errors come up **as they are** (`CommandError::Core`, transparent): a
command that fails fails for a reason of the core, not for a reason invented
here. The other variants describe only the refusals specific to the stack: open
macro, no macro, non-empty stack, foreign macro, macro too deep.

## What to know before modifying

- Adding a command also means deciding its **nature** (`is_structural`):
  getting it wrong breaks the LIFO invariant without raising anything.
- `tests/no_ui_dependency.rs` lists the allowed dependencies of the crate.
- `tests/undo_random.rs` draws random sequences; `tests/history.rs` holds
  the named cases; `tests/memory_pinning.rs` measures memory pinning.
  On the robustness side: `b5` (macros), `b12` (commands accepted with no
  effect), `b18` (what stays possible while a macro is open), `b21`
  (cost and identity on the write path).

`History::execute_recorded` is `execute` that tells whether an entry has been
stored (`false` for a command accepted but with no effect, rule B5): the actor
uses it so that Stop (#239) undoes only the command it has just executed. It
is not deduced from `len()` before/after (eviction, macro).

**To read**: `stokesforge-commands/src/lib.rs` and `src/history.rs` (heads).

The Object Manager (#81–85) reuses the delta of `SetPlotSpec` through
`Command::EditPlotSpec { graph, spec, kind }`: a replacement as a whole,
with the `PlotEditKind` reason in `HistoryLabel::EditPlotSpec`. Visibility,
renaming, ordering and removal have their `history-plot-*` keys, with no
sentence carried by the actor. The menu and the tree emit the same `Action`.
A stale target, an unchanged rank or an identical value sends no command.
Frame sizes (#394) are part of that spec: Add Layer Beside / Below and Delete
carry their column or row frame in the same `EditPlotSpec` (`AddCell`,
`RemoveCell`: one step, the frames restored by Undo), and `Action::SetFrameSize`
sends one `EditPlotSpec` with `PlotEditKind::ResizeCell` ("Resize Plot Area"). A
legacy graph (no stored frame) is given the frames it is drawn with **in that
same command**, never in a separate step and never at load: any
`SetPlotSpec` / `EditPlotSpec` / `AddGraph` that goes through `AppState::execute`
(or `bridge::set_graph_spec` for `pyst`) carries them, so Undo returns to the
legacy file as it was. The Graph Properties size fields edit the draft of the
window, hence the window's single `GraphProperties` macro.
Opening the panel, choosing a content or opening its properties belongs to
the view state; validated changes are undoable.

## Copying objects (#105–106)

`NodeCopy::capture` freezes a subtree without keeping the project or its
allocator. `Command::InsertCopy` recreates it through `Project::insert_copy`,
with new identifiers allocated in the destination. Every kind is copyable by
default: the copy keeps its content and rebuilds its children, with no
allowlist. A failure restores the tree; the identifier counter never moves
back. `DuplicateWorksheet` reuses this atomic primitive and puts the sheet
right after the original, including when unknown kinds are kept in it.

Multiple gestures are grouped by `MacroKind::{PasteObjects,
DuplicateObjects, DeleteObjects, RenameObjects, MoveObjects}` (Project
Manager); moving several columns of a sheet has its own,
`MacroKind::MoveColumns` (#319: a series of `Command::Move` computed by
`stokesforge-app-state::column_move::plan`, index counted **after removal**, a
single `execute` with no macro for one column); an intermediate failure aborts
the whole group. These labels stay data and are not recorded in the `.stf`.

For cross-project graphs (#107), the copy also carries the frozen
dependencies; `NodeCopy::into_external` enables their insertion in
`InsertCopy`. Hidden workbooks and graphs are created atomically in the same
structural entry, with the references replaced. Within the same project, the
dependencies are not inserted. Revealing the workbook reuses
`SetWindowMeta`, hence its delta and its undo.

## Analysis calculations (#181)

`AddCalculation`, `UpdateCalculation` and `RemoveCalculation` are
**structural** commands (tree snapshot): a calculation creates, rewrites or
deletes output columns at the same time as its record, and the undo must give
everything back in one block — one history entry per gesture, like a macro,
without opening one. Labels: `HistoryLabel::AddCalculation(kind)`,
`UpdateCalculation(kind)`, `RemoveCalculation { kind, with_columns }` (the
operation family is data, `OperationKind`).

- `RemoveCalculation { with_columns: false }` "detaches": the columns stay,
  ordinary and editable again.
- A column produced by a calculation is **read-only** for the commands on cells,
  on type and on cell deletion (`CoreError::CalculatedColumnReadOnly`,
  `check_writable_cells` and `check_not_calculated`). Only the name, the unit
  and the other metadata (apart from the formula) remain allowed; a
  recalculation whose output would still refuse the result marks the
  calculation `Failed(OutputUnwritable)`.
- The automatic recalculation (`Project::refresh_calculation`, session side)
  does not go through the history: it is a derived update, like the
  recalculation of formulas.

## Linked transposed sheets (#253)

`TransposeRows { source, rows }` creates or completes the linked transposed
sheet (sheet, x' column, "Row N" columns, link) and
`RecalculateTranspose { sheet }` rewrites its linked columns from the source.
Both are **structural**
(tree snapshot: nodes appear, the link is carried by the tree) and atomic — a
refusal (column without x', text column) restores the tree and pushes nothing.
Labels: `HistoryLabel::TransposeRows`, `RecalculateTranspose`; group
`MacroKind::AnalyzeRows`.

- **A single undoable group**: the analysis gesture on a row opens the
  `AnalyzeRows` group (`state/transpose.rs`), executes `TransposeRows`, then
  opens the analysis window, which is modal: Apply closes the group with the
  calculation, Cancel aborts it and the sheet never existed. A single Ctrl+Z
  undoes sheet, columns, link and calculation.
- A stale and untouched sheet is first brought up to date when it is completed;
  a modified sheet is never overwritten except by `RecalculateTranspose` (the
  interface asks for confirmation first).
- Every write of values goes through `Project::restore_column` / `add_column`:
  a fresh version per column (« Pièges connus » of `CLAUDE.md`).

## Removing a formula (#236)

Clearing the Formula field of a column **also deletes its values**: a
`MacroKind::RemoveFormula` macro (`SetColumnMeta` + `DeleteCells` of the whole
column), a single history entry, which Ctrl+Z gives back at once. All paths
(cleared input, Backspace, paste, several columns) go through
`AppState::set_meta_dropping_formula_data`. The right-click offers "Remove
Formula, Keep Data" (`Action::RemoveFormulaKeepData`), the same macro without
the deletion of values.

## Replacing a correction (#212)

`ReplaceAnalysisData { draft, expected_versions }` replaces in place the Y of a
baseline or of a correction by reference, and adds an immutable
`CorrectionRecord` trace. A structural snapshot keeps values and trace
together: a single Undo removes the gesture, Redo restores it. No calculation
link is kept; the columns stay editable and their graphs follow their new
values. The versions of all the inputs read are required before writing. A
column with a formula or a calculated column is refused.

`RestoreOriginalData { trace }` (#248, structural) gives back to the replaced
columns their original, kept in the hidden workbook of copies: it overwrites
the input made since, and an Undo reverts it. **Always the raw data** (#308): a
column corrected several times keeps the copy of the very first replacement
(`first_original_of`), shared by all its traces; restoring any one of them
gives back this raw data and takes the column out of **all** the traces (a
trace with no remaining column disappears). The copies (frozen reference of a
correction, originals) live in **a single hidden workbook per project**
(`stokesforge-core/src/analysis_store.rs`); a copy is removed with the last
analysis or trace that cites it. All these writes are inside the command that
causes them: undoing brings back the tree, copies included.

The length of Y stays identical. The rows whose X or Y is not finite keep their
original cell; a NaN produced on a valid row (division by zero, reference out
of range) stays visible. Several series sharing Y must produce identical
values: otherwise nothing is written. Each replacement keeps its own trace
(hence its Results row), including after an ordinary input; all share the raw
data of the column. Deleting all its sources removes the trace; undoing their
deletion restores it.
