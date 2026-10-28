# Work package "mixed-type columns" — handover note

**This document is not a reference page.** Unlike the rest of
`docs/`, which describes the code as it is, this one describes a *work package*: what
it aimed at, what it found along the way, and what it leaves open. It has a limited
lifetime — when the "remaining" column is empty, it moves to
`docs/archive/`.

The *what* and the *why* of the result live in the normal pages:
[`modele-de-donnees.md`](modele-de-donnees.md) (the type and its rules),
[`persistance.md`](persistance.md) (the format and compatibility),
[`commandes-et-historique.md`](commandes-et-historique.md) (the two cell
writes), [`application/grille.md`](application/grille.md) (input).

## The goal

A column had a single type for its whole length: `Numeric`,
`DateTime` or `Text`. The goal of the work package: **that a single column holds text
and numbers depending on the row**.

The need comes from a real import. A column of measurements holding
`1,2 / 1,3 / n/a / 1,5` had only two outcomes, both bad:

- **all numbers** — the `n/a` becomes a `NaN`, indistinguishable from a measurement
  that is simply missing; the information "the instrument answered *n/a* here" is
  lost when the file is read, for good;
- **all text** — nothing can be plotted anymore, and the other 999 rows pay
  for the first one.

The constraint that guided the design: **minimize breakage**. The `.stf`
format already written, the existing commands, the rendering that assumes a homogeneous column,
and the zero-copy numpy view of the Python bridge all had to survive.

## What is in place

| Crate | What changed |
|---|---|
| `stokesforge-core` | `ColumnType::Mixed`, `MixedCell { Empty, Number, Text }`, `ColumnValues::Mixed`, `ValueBlock::Mixed`, the six mutators, the conversions in both directions, `Column::cell`, `Column::numeric_view`, `ColumnValues::number_at`; `accepts_child` fixed (see the bugs) |
| `stokesforge-commands` | `Command::SetCellsPromoting` and its three paths; `check_writable_cells`/`write_cells` factored out; memory footprint of a mixed buffer |
| `stokesforge-persistence` | `TypeRecord::Mixed`, `ValuesRecord::Mixed` as three parallel arrays, and the two consistency errors that go with them |
| `stokesforge-session` | `write_formula_column` conforms its block to the column's type |
| `stokesforge-app` | tolerant input, display of a mixed cell, TSV paste, type suffix |
| `stokesforge-script` | error messages that name the real type |
| `stokesforge-render` | **nothing** — that is the result, not an oversight |

The fact that `stokesforge-render` is empty in this table is the property that
matters: the translation "mixed cell → plottable `f64`" happens once, in
`Column::numeric_view`, and the pipeline — hence both backends — learned nothing
new (invariant 5).

## The bugs met along the way

Three, one of them serious. None was caused by this work package; all three were
there before, and the work package brought them to light.

### 1. An unrecognised nature under a sheet lost the whole file

**Serious, fixed.** C5 (level 2) promises that an unknown variant read from a
`.stf` limits its loss to *that node* (invariant 10). The promise held only
for a node stored in a **folder**: `NodeKind::Unknown` counts as a
window, so `Folder::accepts_child` accepted it — but `Workbook` and
`Worksheet` did not. `unknown.rs` did its job correctly, stored the
node as `Unknown`, and the tree validation of `load` refused it right away:
`InvalidModel`, whole file lost.

Discovered while writing the test that was supposed to *prove* the migration strategy —
which failed on the first run, with the message « nœud 5 est mal rangé : une
feuille ne contient que des colonnes ». Without that test, the compatibility note of
[`persistance.md`](persistance.md) would have been false and nobody would have
known until a user opened a colleague's file.

The three containers now accept an unrecognised nature.

### 2. A formula recalculation swallowed silently

**Latent, fixed.** `actor::write_formula_column` always wrote a
`ValueBlock::Float`, and `let _ = project.set_values(...)` threw away the refusal. As long
as all columns were numeric, the refusal never happened; a formula placed on a mixed
column would have made it real — and invisible. The block is now conformed to the
column's type before the write.

### 3. "The column is of text type" for a column that is not

**Cosmetic, fixed.** `pyst.column` and `pyst.set_column` refused any
column without an `f64` buffer with the message "is of text type". Accurate as long as
`Text` was the only case; false afterwards. The two messages now name the real
type, through a `ColumnType::type_name` added to the core.

### What was not a bug

Two tests froze the old behavior (`shell.rs`,
`invalid_numeric_input_reports_an_error_and_sends_no_command`; `b15`,
`..._est_refuse_sans_ecrire`). They were **rewritten, not deleted**: they
keep the property that matters — no data disappears — and now assert
the opposite at the surface, because the behavior was deliberately
changed.

Besides, the six caps of
`docs/archive/rustrewrite/questions-ouvertes.md` §11 were checked before
anything else: `MAX_COLUMN_ROWS` is the only one this work package touches, it
applies to mixed columns through the same function (`write_block`,
generic), and nothing was "fixed" that was merely a documented cap.

## What remains

### Test side

The test gaps listed at the close of the work package are filled, except one —
the `python` feature, which stayed **unverified** (see the dedicated note after the
table, not a code problem but a network access problem in the sandbox that
handled this task). The detail, kept for the record:

| Where | What was missing | State |
|---|---|---|
| `b16` (resolved series) | no mixed case: the common prefix, the too-short error columns and the truncation are tested only on homogeneous columns | **filled** — three cases added (complete mixed column, mixed column as X shorter than Y with text/empty in the middle, too-short mixed error column, empty mixed error column); the two anomalies already documented (signal truncation, vanished series) reproduce identically once passed through `numeric_view` |
| `b17` (cost of operations) | `numeric_view` is **the only read of the model that copies**, and nothing measures it nor guards against a regression — for example someone calling it per plot point instead of once per resolution | **filled** — a guard compares the cost of a full resolution to that of a single call to `numeric_view`, and to that of a counter-example simulating per-point calls |
| `b21` (cost and identity) | `SetCellsPromoting` absent. The property "the costly path happens only once per column" is today asserted only by a test in `shell.rs`, never **measured** | **filled** — directly measures that path 3 (promotion) costs proportionally to the column size, and that paths 1/2 (column already mixed) do not depend on it |
| `memory_pinning` | no scenario with a mixed column; `mixed_buffer_bytes` is an estimate modeled on `text_buffer_bytes`, not tested | **filled** — scenario 8: a deleted mixed column stays retained as long as its entry is on the stack then is released on eviction (same pattern as scenario 6), and `History::retained_bytes()` matches exactly the estimate of `mixed_buffer_bytes`, recomputed independently in the test |
| `b10` (version counter) | no mixed case | **filled** — the tolerant input that promotes a column, then a second input on an already mixed column, both advance the counter |
| `b2` (persistence) | the mixed case lives in `roundtrip.rs`, not in the battery | **filled** — added to `b2_round_trip_preserves_everything`, which already compares type/meta/version/cells node by node |
| `python` feature | **not compiled by default, hence unverified.** `float_values()` returns `None` for a mixed column, which *should* give the expected refusal on the `pyst` side, but no test exercises it. To run with `scripts/python-test.sh` | **still open** — see the dedicated note below |
| `typst` feature (removed since: Typst is a firm dependency, issue #29) | not compiled either. No code under `typst` touches columns (checked), so the risk is low — but not zero | **verified** — `cargo build`/`cargo test` with `--features typst` on `stokesforge-render`, `stokesforge-render-svg` and `stokesforge-render-egui` all pass, with no code change required: confirms that nothing under `typst` touches columns |

**Note on the `python` feature.** `scripts/python-test.sh` requires an embedded
CPython interpreter that `scripts/python-vendor.sh` downloads
(`vendor/python/lock.env`: CPython 3.12.14, ~90 MB, plus a pinned numpy wheel).
The sandbox that handled this task has network access to
`github.com` (checked with a simple `curl`), but the throughput to
`objects.githubusercontent.com` (where GitHub redirects the download of the
release archive) was around 100 to 150 KB/s — the archive alone
did not finish downloading after several minutes, and the script fails if
interrupted halfway (no resume: it rewrites `$ARCHIVE.part` from
scratch). The missing test (`float_values()` returns `None` for a mixed
column → `pyst.column`/`pyst.set_column` must refuse it while naming the right
type, like the two other refusals already covered for `Text`) **could therefore not
be written or run under this feature in this task.** The code concerned
(`stokesforge-python`/`stokesforge-script`) has not changed since the close of the
work package; nothing points to a correctness problem, only a coverage gap that
remains to be filled as soon as an environment with enough network throughput
(or the interpreter already vendored) is available.

### Feature side

- **Import never produces a mixed column.** `stokesforge_import::ParsedColumn`
  holds a `Vec<f64>`: an unreadable token becomes `NaN` there, exactly the loss
  that this work package exists to avoid. This is **the** motivating use case, and
  it does not benefit yet — it belongs to work package 2 (import preview),
  deliberately out of scope here. It is the first thing to do.
- **No UI to change a column's type.** `Command::SetColumnType`
  is dispatched nowhere in `stokesforge-app`; a column can only be made
  mixed through tolerant input, a script or a file. Predates this
  work package, but more visible now that there is a fourth type.
- **`format_f64_for_display` exists twice** — in `stokesforge-core::column` and
  in `stokesforge-app::state`, with different thresholds. The robustness guide
  (§F8, « une seule table, deux directions ») asked for their unification once
  the two halves of fix C4 were merged; they are, the unification
  is not. Unrelated to mixed columns, but this is the time to
  note it.
- **The Python bridge refuses mixed columns**, deliberately (see below).
  If a need appears, the right answer is a distinct surface that says
  it copies, not a loosening of `pyst.column`.

## Decisions not to reopen without a new reason

- **`Mixed` is a fourth type, not an overhaul of the other three.** Making
  every column a `Vec<MixedCell>` would cost 32 bytes per row instead of 8
  and would remove the contiguous buffer on which the zero-copy numpy view and the
  zero-copy resolution depend.
- **`FORMAT_VERSION` does not move.** The addition is additive and the degradation of
  an earlier reader is tested. Incrementing would have made *all* following files
  unreadable by the old readers, mixed column or not.
- **Three parallel arrays on disk, not an array of enums.**
  `to_vec_named` writes the *name* of a variant: seven bytes of `"Number"`
  in front of each eight-byte value.
- **`Cell` is unchanged.** A mixed cell is read in the common vocabulary
  (`Column::get`) or as it is stored (`Column::cell`); adding a
  third variant to `Cell` would have touched all its readers for a
  distinction that only one of them needs.
- **`pyst.column` refuses a mixed column.** It returns a *zero-copy* view;
  giving it the materialized copy of `numeric_view` would be a view that no longer
  follows the column — a silent trap for a script that thinks it holds the
  buffer.
