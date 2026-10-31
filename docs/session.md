# Session — the actor and concurrency

`stokesforge-session` makes concrete the decision that holds the whole
foundation together: **the `Project` lives on its own thread**.

The actor holds the `&mut Project` **of the session**. A client has only a frozen
`Arc<Tree>` and can mutate only through `SessionHandle::execute(Command)`.
"No mutation outside a command" is therefore an ownership property **for the
clients of a session**, not a compiler guarantee for all the code. The public
primitives of `stokesforge-core` also serve to *build* a project before
`Session::spawn`, in tests, and in the CLI (which handles a `Project` with a
`History`, without a session). Outside the history **by choice**: the
recalculation of formula columns (`Project::set_length`/`set_values` in the
actor, see below), window geometry and state, view framing.

## The protocol

A **FIFO** request channel (`std::sync::mpsc`, deterministic order). Each
request gets **one** response, on a single-use channel that the client creates
for that request.

The UI and the script thread are two **symmetric** clients: they send requests
and receive immutable snapshots. The UI keeps the last `Snapshot` it received
and paints from it **without a lock**. It knows its snapshot is stale by
comparing the `revision` — a monotonic counter — never by comparing trees with
each other.

```rust
Session::spawn(project, history_limit) -> Session   // starts the thread
session.handle()                       -> SessionHandle  // cloneable, to give to the clients
session.shutdown()                     -> Project        // hands the project back
```

`SessionHandle`: `execute`, `undo`, `redo`, `begin_macro`, `end_macro`,
`abort_macro`, `snapshot`, `save`, `load`, `new_project`.

## What a snapshot carries

```rust
Snapshot { tree: Arc<Tree>, revision, history: HistoryState, epoch }
```

Cloning it is cheap: two `Arc`s and a handful of fields. **The tree is never
copied, only shared.**

The snapshot also carries, outside the tree, `window_geometries` and
`column_widths` (#325, width of sheet columns by `NodeId`), with
`geometry_revision`, the counter shared by these two tables. Setting a width
advances neither `revision` nor the history. Only the geometry table is purged
on mutation. The width table **is not** (Ctrl+Z gives back the deleted column
with its width, invariant 7); saving filters out the dead keys.
`Load` and `NewProject` replace or empty both.
`Request::ResetColumnWidths { columns }` (#330, `SessionHandle::reset_column_widths`)
**removes the keys** of the columns (the default is an application constant,
the session writes nothing). `geometry_revision` advances only if a key was
really removed; neither `revision` nor the history moves.

`HistoryState` gives the UI what it needs for its menus — `can_undo`,
`can_redo`, the two labels, `in_macro` — without exposing the stack.

`epoch` is distinct from `revision` and answers a question that `revision`
cannot settle: **is it still the same project?** It is a counter local to the
actor, never persisted, incremented when a `Load` or `NewProject` succeeds.
Both **also** increment `revision`: it stays monotonic over the whole life of
the session. What it does not say is that the project was replaced; `epoch`,
for its part, has changed. `stokesforge-script` uses it to detect that a handle
designates a `NodeId` of a project that is no longer the one that produced
it — even before looking up that `NodeId` in the tree, where by coincidence it
could designate another object.

## `save`, `load` and `new_project`

`save` and `load` go through `stokesforge-persistence`
([`persistence.md`](persistence.md)).

- `save` writes the current project **without touching** the revision or the
  history;
- `load` **replaces** the current project and **resets the history**
  (`undo`/`redo` become empty again). An old snapshot then has no relation to
  the current tree, including its node identifiers, which restart from the
  value persisted in the file.

`new_project` replaces the project with an empty `Project1`, with no undoable
entry. Like `load`, it empties the history and the geometries, and advances the
revision, the epoch and the geometry revision. The confirmation belongs to the
application, not to the actor.

All three are refused while a macro is open. On failure, neither the project
nor the history is modified.

## An open macro, and the identity of the sender

Each `SessionHandle` carries a stable identity (`ClientId`, private to the
crate). Two `.clone()`s of the same handle remain **the same** sender — a script
that clones its handle for a thread stays "itself". Two **independent** calls to
`Session::handle()` are two **distinct** senders, even in the same process.
`SessionHandle::with_new_identity()` gives a handle to the same session with a
**new** identity. This is what the detached views
(`AppState::fork_for_detached_view`, in `stokesforge-app-state`) and the script
engine use, so that their writes are refused (`ForeignMacro`) while a macro is
open by the interface — instead of being absorbed into it and then undone with
it (issue #21).

This identity exists for a precise reason: without it, one client's work leaked
into another's macro, and an `abort_macro` could silently undo someone else's
already approved work.

| Request | During an open macro |
|---|---|
| `Execute` | **refused** (`ForeignMacro`) if the sender is not the owner; otherwise absorbed into the macro |
| `Undo` / `Redo` | refused for **anyone** (`MacroOpen`) — undoing during a macro, one's own or another's, would leave an inconsistent state |
| `BeginMacro` | **refused** if a macro is already open by an *other* sender; the same sender can nest its own |
| `EndMacro` | **refused** if the sender is not the owner (`NotInMacro` stays reserved for the case where no macro is open) |
| `AbortMacro` | **refused** if the sender is not the owner |
| `Save` / `Load` / `NewProject` | refused for anyone — writing the intermediate state of a macro that someone could still abort stays forbidden to all |
| `Snapshot` | always served: pure read |
| `Shutdown` | undoes (rather than commits) any macro still open, level by level |

The last point deserves detail: a macro never prevents `Project` from being
mutated — it only **defers the history entry**. Without a guard, a `Shutdown`
returned the project with these mutations **silently committed**, without them
ever having become an undoable entry. A macro left open at shutdown is therefore
now undone.

The full table, with what it fixes and why, is at the top of
`stokesforge-session/src/lib.rs`.

## Two invariants held by construction

1. **The actor never touches Python.** This crate depends on no Python crate,
   and `tests/no_ui_dependency.rs` forces a conscious decision for any new
   dependency. This is the third pillar of invariant 9: without it, the mutual
   blocking between the GIL and the actor would be structural.
2. **No non-`Send` object crosses the channel.** Requests and responses are
   pure Rust, `Send + 'static` — checked by a compile-time assertion
   (`assert_send`).

## The principle that governs refusals

Everything a client knows about the project is a **potentially stale** copy.
So: every assumption must travel in the request and be checked by the actor
**at the moment of applying it**, never before.

This is the same principle as `Command::SetCells { expected_version }`
([`commands-and-history.md`](commands-and-history.md)): check nothing on
the client side, check everything on the actor side.

## Recalculation of formula columns (#193)

After each successful `Execute`, `Undo`, `Redo`, `AbortMacro` and after each
`Load`, the actor goes over the formula columns (`src/recalc.rs`). It
**rewrites** only those for which some data of the computation has changed since
their last computation. For each one it keeps a *fingerprint* — the stored
formula (text and column identifiers, #321), the version of each column read
from the sheet it must be in (or its absence: deleted, moved to another sheet,
or its sheet gone), the length of the result, the version of the column itself,
the outcome (computed, unreadable, caught in a cycle) — and compares it with the
current state. An unrelated
command (a curve color, a folder) therefore rewrites no column; modifying A
rewrites only what reads A, then, through the version of what has just been
rewritten, what reads that in turn.

**Across sheets and workbooks (#403).** A formula may read a column of another
sheet or workbook (`max(Sheet3!B)`, `at([Book2]Sheet5!B, 340)`): the pass
orders the formula columns of the **whole project** in one dependency order (a
sheet is no longer processed alone), and the fingerprint holds the remote
column, its paired X and their membership in the sheet they must be in. The
formula stores the sheet identifier of a remote column; the workbook is read
from the tree, so moving the sheet changes the displayed prefix, not the
values. A loop across sheets is a `FormulaError::Cycle` (cells empty, never a
hang) and is refused at input (`Tree::formula_for_entry`), by the interface and
by `pyst`; a file that already holds one frames the Formula cell. A remote
column does not lengthen the result: it keeps the length of its own sheet.

**Result length (#194).** A formula column has the length of the longest of the
columns it reads in its own sheet; beyond the length of a shorter column, the row is `NaN`. When
it reads no column — constant, unreadable formula, caught in a cycle — it takes
the length of the longest **ordinary** column (without a formula) of the sheet,
never that of another formula column. A formula therefore does not grow with a
column it does not read, nor with another formula, and becomes short again when
the growth of its source is undone.

| | |
|---|---|
| where the memo lives | in the actor, next to the `Project` (`Recalculator`): neither history nor `.stf` |
| what empties it | `Load` (the values of a file are not taken on trust) and `NewProject` |
| what it rests on | the versions doctrine: a number never designates two contents of the same column (type + values, not the metadata) |
| the oracle | `FormulaRecalc::Full`, the full recalculation from before #193, through `Session::spawn_with_recalc` — never in production |
| what holds it | `tests/recalc_targeted.rs` (one named case per component, with witness) and `stokesforge-robustness` `b22` (two twin sessions, same trees at each step) |

### Interruptible recalculation (#239)

`SessionHandle::execute_interruptible` answers as soon as the command is
applied, then recalculates the formulas in a way that can be stopped; `execute`
stays blocking from end to end (scripts, CLI, tests). Only `AppState::execute`
uses it.

| Piece | Role |
|---|---|
| `gauge.rs` (`RecalcGauge`) | the only shared state **outside the FIFO channel**: number of the running recalculation, Stop flag, "done" condition. During a recalculation the actor no longer reads its requests: a Stop or a snapshot sent through the channel would wait for the end |
| `SessionHandle::actor_alive` (#310) | is the actor thread still running? A `Drop` guard in `actor::run` calls `RecalcGauge::abandon` when the loop exits, **panic included**: `running` goes back to `None` (no more everlasting recalculation indicator), the waits are woken, `actor_alive` becomes false. The model, for its part, is lost with the thread: the application writes a backup copy of the last snapshot (`stokesforge-app-state::recovery`) and announces the death with a blocking dialog (`AlertKind::SessionLost`). Autosave (#317) also goes through the actor: `SessionHandle::save` to the process's recovery location, with no history entry and no revision change, refused during a macro (the application then skips the deadline) |
| `SessionHandle::recalc_running`, `wait_recalc`, `stop_recalc(id)`, `take_recalc_end` | read and write the gauge, without the actor, so never blocking |
| `Recalculator::run_interruptible` | checks for the stop between two sheets, two columns, two computations, two rounds — never in the middle of a column. An interrupted pass leaves a consistent state: the columns written are in the memo, the others will be seen as stale |
| Stop | `history.undo` (like Ctrl+Z: the command stays redoable) then a normal full recalculation; `RecalcEnd::Stopped`. Only if the command stored an entry (`History::execute_recorded`): a command with no effect stores none, so Stop then undoes **nothing** (never the previous action) and the recalculation completes (`Done`). In a macro the undo is refused: the command stays, the recalculation completes (`Done`) |

The number prevents a late Stop from stopping the next recalculation. The
revision returned by `execute_interruptible` is the one from before
recalculation; **the end of the recalculation adds one** (the formulas change
under the same revision, and a rendering cache indexed by revision would stay
stale). The other requests (`undo`, `save`, `snapshot`…) always wait for the end
of the recalculation: on the interface side, `AppState::refresh` skips the
snapshot while a recalculation is running. Only `Execute` is interruptible;
`Undo`, `Redo`, `Load` recalculate in one block.

The rule not to break: **any data that enters the computation of a formula
enters its fingerprint**. A computation that read one more piece of data
without adding it there would display a wrong value without an error. A hidden
input counts: `at(B, 340)`, `integral(B)` and the range forms read the X column
paired with `B` by the rule of the plot (#402), which the text does not name.
The formula's **inputs** are the columns it names plus those X
(`compose::paired_x_columns`): one set for the dependency order, the fingerprint
(`Outcome::Evaluated::paired_x`: identifier and version of the X), the loops
through a calculation and the evaluation. The tests
to write when the mechanism is extended (new formula function, cross-sheet
reference #186, recalculated analyses #184):
[`tests.md`](tests.md), "Targeted recalculation".

## What to know before modifying

- Adding a request means deciding its row in the table above. A mutating
  request that is not listed there is a hole.
- `tests/session.rs` carries the named cases; on the robustness side, `b6`
  (concurrency between clients), `b18` (what stays possible while a macro is
  open) and `b22` (targeted recalculation against full recalculation).
- A request that replaces the project must empty the recalculation memo
  (`Recalculator::forget`), like `Load` and `NewProject`.

**To read**: `stokesforge-session/src/lib.rs` (the top, and its table), `src/actor.rs`,
`src/recalc.rs`, `src/gauge.rs`, `src/snapshot.rs`.
