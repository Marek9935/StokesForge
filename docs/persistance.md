# Persistence — the `.stf` format

## Frozen boundary

Two functions, one direction each, a whole `Project` in and out and nothing
else:

```rust
save(project: &Project, path: &Path) -> PersistenceResult<()>;
load(path: &Path)                    -> PersistenceResult<Project>;
```

No caller knows the on-disk format. If it ever had to be replaced (Zarr was
considered), nothing above would have to change; since V1 the format itself is
frozen (invariant 12, version rule below). Loading is **complete**: the whole
project fits in memory, and there is no partial or deferred loading.

The session uses the variants `save_with_geometries` and
`load_with_geometries`, which also carry a `NodeId -> WindowGeometry` table
(position and size in logical points). This `window_geometries` block belongs
to the same `.stf` file, but stays separate from the undoable tree. An old file
without this block loads with an empty table. The application passes geometry
changes to the actor and reloads this table both from the File menu and at
startup with a project path.

Column width (#325): the same function carries a second table
`NodeId -> f32` (logical points), `FileBody::column_widths`, in the struct
`ViewTables { window_geometries, column_widths }` that `load_with_report` returns (and
that `save_with_geometries` takes). Same status as the geometries: outside the
undoable tree, `#[serde(default)]` (old file: empty table, the columns
take the default width), keys bounded by `MAX_NODE_ID`, placed before
`save_counter`. On reading, a key with no node and a value that is not
a finite, strictly positive real number (NaN, ∞, ≤ 0) are dropped **one by one**
(invariant 10). The actor does not purge its table on mutation (Ctrl+Z must
get the width back): it is **writing** that does not emit the key of a column
that is no longer in the tree.

Each `WindowGeometry` also optionally carries a `WindowView` (issue #16):
mode `Open`/`Maximized`/`Closed`, collapsed state, stacking rank among the open
windows. `x`/`y`/`width`/`height` always stay the **floating** geometry there;
detaching is never saved (a detached window is written `Open`), and neither is
the position of a detached OS window. Absent `view` = default initial layout; an
unknown mode reads as `Closed`.

Columns also carry `import_source` (issue #7, S2 of #323): an
`ImportSourceRecord` with named fields — file path, recipe, column index,
block rank —, enough to refresh the column after reopening. The recipe has its
own mirror types (`recipe_record.rs`, one per type of
`stokesforge_import::recipe`, exhaustive `match` without `_`): the format does
not depend on the internal shape of the recipe, and the dependency
`persistence → import` is declared in the allow list. The model keeps the
recipe as JSON (`ImportSource::recipe_json`): the conversion reads it back on
writing and rewrites it on reading. An unreadable recipe (column built by
hand) is written **without a source**: saving does not stop on it. Absent for
a column typed in by hand.

Columns also carry `file_metadata` (issue #5): the key / value lines of the
header of the imported file (and of the footer if the recipe asks for it), in
order, sections included. Absent = empty list.

## The format is not written into the model

`stokesforge-core` derives no `Serialize`/`Deserialize`. This crate defines its
**own transport structs** (`schema.rs`, private to the crate) and writes by hand
the conversions to/from the model (`convert.rs`).

The on-disk schema is therefore an explicit object, versioned independently of
the core types: renaming a model field without breaking the files already
written will cost only one conversion function to adjust.

Direct consequence for whoever adds a field to the model: **it will not appear
in the file** until `schema.rs` and `convert.rs` have been touched. This is not
an oversight of the format, it is its point.

## File layout

```text
octets 0..4   signature "STKF"
octets 4..8   FORMAT_VERSION, u32 little-endian
octets 8..    corps MessagePack (FileBody), rmp_serde::to_vec_named
```

The body lists the nodes in **pre-order from the root**
(`Tree::walk_all`): two successive saves of the same project produce
**byte-for-byte** identical files.

The numeric column buffers escape the generic MessagePack encoding: they are
written as raw little-endian bytes through `serde_bytes` (8 bytes per `f64`),
not as an array of elements. A million points weigh 8 MB on disk, not an
inflated multiple due to per-element encoding.

## Five properties held from version 1

Adding them afterwards would cost a migration.

1. **Atomic write.** `save` writes to a temporary file in the same directory as
   the destination, then renames it onto the target. A reader never sees a
   half-written file; a write that fails leaves no trace on the destination. A
   target that carries the `STKF` signature but cannot be read (interrupted
   copy, undecodable body, or a body that `load` would refuse: inconsistent
   model) is **not** overwritten: `UnreadableTarget`, and the user moves the
   file or chooses another name. The full rebuild is done only for a target
   that the process has never loaded or written. A foreign or empty target
   stays replaceable.
2. **Version number first**, read and checked **before** touching the body
   (`version.rs`: `read_header`, then `decode_body(version, octets)`, one
   `match` per version, shared by `load` and by the counter reading of `save`).
   Newer file → `UnsupportedVersion` (S1: no degraded reading, the message says
   to update StokesForge, key `error-file-too-new`); file that is not a
   StokesForge file → `NotAStokesForgeFile`. Never an attempt to decode a body
   that is not understood.
3. **Missing value ≠ empty string.** `None` against `Some("")` for text;
   numeric `NaN`s are written and read back **bit for bit**
   (`to_le_bytes`/`from_le_bytes` keep the exact bit pattern).
4. **Persisted `NodeId` counter.** `next_id_hint` is the **next** identifier to
   allocate, not the largest one allocated. On loading, the counter restarts at
   `max(valeur persistée, plus grand id lu + 1)`: a file written by a buggy
   version cannot make it go backwards.
5. **Unknown fields ignored on reading.** `rmp_serde::to_vec_named`
   serializes each struct as a **named map**, which makes the file
   self-describing. Every field added after version 1 carries
   `#[serde(default)]`, so that an older file that does not carry it
   stays readable.

The choice of MessagePack follows directly from property 5: `bincode`
encodes structs as **positional** tuples — adding a field shifts
everything after it, and removing one is just as breaking.

## The history is not in the file

The `.stf` carries **neither an undo stack nor history labels**:
`stokesforge-persistence` does not even depend on `stokesforge-commands`. The
`label` fields of the schema are those of the *windows* (`WindowMeta::label`),
unrelated. A reopened project starts from an empty history, so no history
sentence survives the language of the session that wrote it (issue #46: the
finding "the `.stf` saves French sentences" was inaccurate).

If the history is ever written, the path is laid out: `HistoryLabel` is
already structured data. It will need a transport type in `schema.rs`
(without `serde` in `stokesforge-commands`, as for `stokesforge-plot`) and the
same degradation as `KindRecord` — an unknown variant name captured in a
`RawVariant`, rendered by a generic label, never a read failure
(invariant 10).

## Unknown variants: the loss is bounded to the node

Property 5 covered only **fields**, not enum **variants**. `serde` derives an
externally-tagged enum decoding that cannot recover: an unknown variant name
made the enum fail, so the node, so the **whole file**.

`KindRecord` and `PlotTypeRecord` now write their
`Serialize`/`Deserialize` by hand (`unknown.rs`). The principle: first decode
the value that follows the variant name into a generic tree
(`rmpv::Value`), re-encode it, then *try* to decode it again as a known
variant — through `rmp_serde`, never `rmpv::ext::from_value` (the latter
represents enums by **index**, a different convention from the one this format
writes). If the attempt fails, the raw name and content are kept in a
`stokesforge_plot::RawVariant`.

This is what bounds the loss to **that single node** (or that single series)
instead of the whole file, and what makes an unknown variant **survive two
successive saves unchanged** (battery `b19`, which relies on the deterministic
writing checked by `b20`). `RawVariant` knows neither `serde` nor MessagePack:
it carries only opaque bytes, already encoded as read, and only this crate
knows how to reinterpret them.

One detail that cannot be seen from here but without which none of this
holds: the degraded node must then **pass tree validation**. An unrecognized
kind counts as a window, so a folder accepted it; a workbook and a sheet did
not — and a degraded column under its sheet therefore made the whole file
fail, exactly where the degradation was meant to help. The three containers
accept it since then (`stokesforge_core::NodeKind::accepts_child`, battery
`b19`).

The trade-off is accepted and documented: the detour through `rmpv::Value`
copies the bytes of a column once or twice more than a direct decoding.
Nothing measures the cost of `load`/`save` today; it is a cost to watch if the
format ends up carrying much larger columns.

## Version rule: every V1 file can be read back up to V3

Software of version N reads back any file of version ≤ N and **migrates it in
memory** to the current model; it writes it in its own version, never in an
older one. A newer file is refused (S1). What cannot be fixed after V1 is
**what V1 writes**, and what it does with a newer file.

| Schema change | Version number |
|---|---|
| adding a `#[serde(default)]` field, a variant read with a fallback | stays (additive); regenerate the current witness |
| removing or renaming a field or a variant, changing the meaning of a value, of the unit, of the order of a positional list | **goes up** |

**Risk of an additive field.** It can ship in a `Z` version (1.0.1) or a `Y`
version, without raising `FORMAT_VERSION`. But an older version that reads and
then **re-saves** the file ignores the field and loses it, without a message.
Each additive field is therefore announced in the CHANGELOG. A breaking change
of the format raises `FORMAT_VERSION` and requires an `X` version.

**Format N frozen.** As soon as format N+1 exists on a branch (`vN`), format N
is frozen on all the other branches: touching it would produce files that the
N+1 branch migrates badly, or data lost without a message. Only the branch of
format N+1 changes the schema.

When the number goes up (V → V+1): copy the schema of V to `schema/vV.rs`, add
the function `vV → courant` there, add its arm in `version::decode_body`,
freeze the witness (`tests/witnesses/vV/`, never modified) and create the witness
`vV+1/`. `FORMAT_VERSION` is never changed "just to try": the CI (`scripts/versions-alignees.sh`, `fmt` job) requires the file `tests/witnesses/vN/project.stf` as soon as `FORMAT_VERSION` = N.

**Witness files** (`tests/witness_files.rs`, `tests/witnesses/v1/project.stf`):
the witness project exercises all the node, column and series types, with
non-default values everywhere. Two guards: the file reads back as exactly the
project (tree, view tables, clean report); saving this project gives back
exactly the bytes of the file (drift of the written format requires a
decision). Explicit regeneration: `STOKESFORGE_REGENERATE_WITNESSES=1 cargo
test -p stokesforge-persistence --test witness_files regenerate -- --ignored`.

**Stable order**: the view tables are `BTreeMap`s in the body
(`window_geometries`, `column_widths`); two saves of the same project
give the same bytes (`tests/deterministic_views.rs`).

**Input fingerprint** (`inputs_digest`, S3): part of the format. Its input byte
and the FNV-1a are frozen by `the_digest_is_frozen_on_known_inputs`
(literal values); changing it would make every calculation of the files already
written stale on reading.

## Mixed-type columns: additive change, no migration

`ColumnType::Mixed` ([`modele-de-donnees.md`](modele-de-donnees.md)) added
one variant to `TypeRecord` and one to `ValuesRecord`. **`FORMAT_VERSION` did
not move, and there is no migration function.** Both directions rely on
mechanisms already in place:

| Direction | What happens |
|---|---|
| old file, today's reader | nothing changed for it — the three historical variants and their encodings are intact, homogeneous columns are written byte for byte as before |
| today's file, yesterday's reader | the variant name `Mixed` is unknown to it, its strict decoding of the column fails, `unknown.rs` puts it into `KindRecord::Unknown`: it loses **this column**, not the file, and a re-save gives it back unchanged |

The second row is what makes the choice not to increment `FORMAT_VERSION`
defensible, and this is why it is **tested**
(`b19_un_type_de_colonne_inconnu_ne_perd_que_cette_colonne`) rather than
assumed. Incrementing would have had a cost with no counterpart: *all* the
files written afterwards, mixed column or not, would have become unreadable by
earlier readers.

Same mechanism, without a variant this time, for the **free texts** of a
panel (issue #35): `PanelSpecRecord::annotations`, a
`#[serde(default)]` field — an older file reads back without text, an older
reader ignores the field (it loses it on re-save). Round trip held
by `a_non_trivial_plot_spec_round_trips_intact`.

Width and alignment of a free text (#244): `AnnotationSpecRecord::width`
(`Option<f64>`) and `align` (`TextAlignRecord`), `#[serde(default)]` — an older
file reads back without width, centered. Same round trip; `TextFontRecord` (titles,
axes, legend) receives the same two fields.

Likewise for the **graphic objects** (issue #76): `PanelSpecRecord::shapes`
(`ShapeSpecRecord`, `ShapeKindRecord`), `#[serde(default)]`; same round trip
held by `a_non_trivial_plot_spec_round_trips_intact`.

Likewise for the **font and size of text boxes** (issues #77, #78):
`TextFontRecord` (`family`, `size`) as a `#[serde(default)]` field of
`PlotSpecRecord`, `PanelSpecRecord`, `AxisSpecRecord` (`title_font`) and
`LegendSpecRecord` (`font`), and the colors `PanelSpecRecord::title_color` and
`LegendSpecRecord::color` (#90, `#[serde(default)]`: absent = automatic), and `font` on `AnnotationSpecRecord` — absent from
an older file = the font of the graph. Round trip held by
`a_non_trivial_plot_spec_round_trips_intact`.

Likewise for the **moved texts** (issue #53): `TextAnchorRecord` as a field
`#[serde(default)]` on `PlotSpecRecord` (title), `PanelSpecRecord` (panel
title), `AxisSpecRecord` (axis title) and `LegendSpecRecord` (legend) —
absent from an older file = the text stays at its default place. A non-finite
anchor read from the file is kept as is; it is the rendering that
ignores it. Round trip held by `a_non_trivial_plot_spec_round_trips_intact`.

### Why three parallel arrays and not an array of enums

`ValuesRecord::Mixed` carries `{ tags, numbers, texts }`: one tag byte
per row (`0` empty, `1` number, `2` text), then only the values that
exist — the numbers in raw little-endian like `Float`, the texts one after
the other.

The reason is the same as the one that makes numeric columns written as raw
bytes: `to_vec_named` serializes an enum by the **name** of its
variant. A `Vec<MixedCellRecord>` would have written the string `"Number"` —
seven bytes — in front of each eight-byte value. With the parallel arrays,
a column of a million mostly numeric points weighs 9 MB instead of 16, and
`NaN`s as well as `-0.0` go through bit for bit.

The price is a consistency to check on reading: the tags must announce exactly
what the two arrays carry. Any inconsistency — unknown tag included — is a
`Corrupt`, never a silent shift of all the following rows. Same policy as
`BadFloatLength`.

## Read bounds

`MAX_NODE_ID` = 10¹². Any node identifier — or `next_id_hint` — above it is
refused **as soon as it is read**. Very far from `u64::MAX`, where
`IdAllocator::next` would refuse anyway, but early enough that a forged file
cannot bring an unusable project into memory (battery `b4`).

`MAX_COLUMN_ROWS` (`stokesforge_core::column`). A column for which the file
announces more rows (`tags.len()` for `Mixed`, `octets / 8` for `Float`,
`values.len()` for the old `Text`) is refused as `Corrupt`
(`ConvertError::TooManyRows`, helper `check_rows`) **before** any allocation
(#316). 

**Global cell budget (B1, #316).** `load_with_limits(path,
LoadLimits { max_bytes, max_cells })` sums the declared lengths of all
the columns (`check_cell_budget`: `tags.len()`, `octets / 8`, `values.len()`)
once the body is decoded and **before** `record_to_kind`, therefore before any
allocation of a model column; above the limit, `PersistenceError::TooManyCells
{ path, cells, limit }` (key `error-file-too-many-cells`). Default
`DEFAULT_MAX_CELLS` = 1 billion; adjustable in File › Preferences › General
(millions of cells, from 1 to 10,000, key `max_cells_m`, `Action::SetMaxCellsM`).
CLI, scripts and `load_with_limit` keep the default value
(`LoadLimits::default()`). Witnesses: `tests/cell_budget.rs`, battery `b3`.

**Decoding copies (#316).** `unknown::deserialize_with_fallback` frees the
captured `rmpv::Value` as soon as it is re-encoded, before building the known
record; the failure path (unknown variant) reads the value again from the
re-encoded bytes. Measurement (`VmHWM`, one column of 20 M `f64`, 160 MB file,
test build): peak 631 MB before, 475 MB after (-25 %).

**File size cap on opening (A2, #316).** `load_with_limit`
(and `load_with_report`, which takes `DEFAULT_MAX_FILE_BYTES` = 512 MB) reads
the size through `metadata().len()` and refuses above the limit with
`PersistenceError::TooLarge { path, size, limit }`, **before** any reading;
exactly the limit passes. The reading itself is bounded (`take(limite + 1)`)
in case the file grows between the measurement and the reading. The `.stf` is
**not compressed**: its size on disk is its uncompressed size, so there is no
second check to make against a "small file that inflates" (memory inflation,
for its part, is bounded by `MAX_COLUMN_ROWS`). The limit is adjustable in the
preferences (File › Preferences › General, 512 MB by default, from 16 MB to
16 GB, no "unlimited"); the application reads it **before** starting the
session (`main.rs`, `session_for_startup`). A refusal on opening leaves the
current project and path unchanged (`open_project`). On saving, a file that
exceeds the limit is still written, but a warning (`status-saved-over-limit`)
replaces "Saved to" and says how to raise it.

## What to know before modifying

- **Reread `docs/archive/rustrewrite/format-fichier-plx.md`** before touching
  the schema: several options are already ruled out there with their reasons.
- Every new field carries `#[serde(default)]`. Without it, yesterday's file
  becomes unreadable.
  Example: `AxisSpecRecord::mirror` / `tick_labels` (#176, the latter true by
  default); `ticks` (#344, **true** by default, otherwise an old file would lose
  its tick marks), `minor_ticks` (#344, false if the field is missing: an old file has
  no minor ticks; true for an axis absent from the file, like a new axis),
  `minor_tick_count` (`Option`, `None` = automatic) and `label_every` (1 by
  default), `tick_length` / `minor_tick_length` / `tick_width` (`Option`,
  `None` = automatic) and `tick_mark_color` (empty = scale color),
  the "Advanced" settings of the tick marks (#344; the raw value is kept, the
  rendering bounds it), and `tick_direction` (`TickDirectionRecord`, unknown value read
  as `Out`, test `an_unknown_tick_direction_reads_as_outward`); the text colors (`PlotSpecRecord::title_color`,
  `AxisSpecRecord::title_color`/`tick_color`) — an older file
  reads back with empty strings, that is, in automatic color (test
  `a_spec_from_before_the_text_colors_reads_back_as_automatic`). Or
  `FileBody::saved_colors` (saved colors of the project), placed before
  `window_geometries` and `save_counter`, which must stay the last field.
- **The direction of an axis (#344) is not normalized by persistence.**
  `AxisSpecRecord::inverted` stays written as is, and `range_min` /
  `range_max` too: the file gives back what it was given, byte for byte
  (common witness unchanged: `inverted` with bounds (-1, 10)). The direction is
  read from the order of the bounds (From > To = inverted axis); it is
  `AxisSpec::orient` that brings an axis back to its canonical form, called **on
  opening** by the session (`load_project`, project not yet installed: no
  command, no history) and by `pyst.set_graph` / `pyst.plot`. The rendering of
  an axis and that of its oriented equivalent are identical.
- `tests/roundtrip.rs` checks the round trip; on the robustness side, `b2`
  (persistence), `b3` (malformed then semantically hostile files), `b13`
  (bounds of a `PlotSpec` coming from disk), `b19` (file written by a newer
  version), `b20` (two writers on the same file).

**To read**: `stokesforge-persistence/src/lib.rs`, `src/schema.rs`,
`src/unknown.rs` (heads).

## Overlaid layers and roles X2…Y4 (#167, #168)

`PanelSpecRecord::overlay` (`OverlaySpecRecord { x_top, y_right, layer }`,
absent = ordinary layer; `layer` `#[serde(default)]` = 0, layer with no role)
and `RoleRecord::X2`…`Y4` are added to the schema; files
without them read back as before. The runtime addresses (`AnnotationRef`,
`ShapeRef`, `TextTarget`) carry the layer index, nothing of it is written:
only the anchored texts stay stored in their layer.

`KindRecord::Column::x_prime` (#164, `Option<f64>`) and
`SeriesSpecRecord::row` (#166, `Option<u64>`) are added to the schema, both
`#[serde(default)]`: files without them read back as before (`None`). A
non-finite x' read from a file is brought back to `None`.

`SavedColorsRecord::gradients` (#178, list of `{ name, stops }`, `#[serde(default)]`)
carries the saved palettes of the project.

`SeriesSpecRecord::group` (#177, `Option<u32>`): `#[serde(default)]`, files
without it read back without a group. `PanelSpecRecord::group_names` (#197,
list of `{ group, name }`, `#[serde(default)]`): the group names; absent,
all groups carry their automatic name, and an empty name read from a file
is dropped. `PanelSpecRecord::group_gradients` (#213, list of
`{ group, stops }`, `#[serde(default)]`): the stops of the palette chosen by a
group; absent, no group has a palette, and an entry without stops is
dropped.

## Object visibility (#81, #85)

The visibility of series was already carried by `SeriesSpecRecord`.
`AnnotationSpecRecord::visible` and `ShapeSpecRecord::visible` complete it;
absent fields read back as `true`. Hiding deletes neither data,
style, position nor rank. The Object Manager panel does not go into the `.stf`:
its opening is a preference of the person.

`PlotSpecRecord::title_visible`, `PanelSpecRecord::title_visible` and
`AxisSpecRecord::title_visible` also carry the visibility of titles.
These absent fields read back as `true`: earlier files keep all
their titles visible. The format number does not change for the addition of
these fields with a default value. The label of an `EditPlotSpec` command
belongs to the in-memory history; it is not saved in the file.

## Object clipboard (#105)

`clipboard::{encode_objects, decode_objects}` reuses `NodeRecord` and the
conversions of the `.stf`, without touching the file format. The transport is
a text starting with `StokesForge Objects` then a line break, followed by
a MessagePack body encoded in hexadecimal. It carries no version number:
the instances must run the same version of the software.

The body contains an opaque origin and a list of objects, each in pre-order
with its descendants. Each object read is validated as a standalone tree,
with the same identifier bounds and the same containment rules as the
file. Values and metadata are kept, notably NaNs bit for bit,
mixed cells and import provenance. The source identifiers describe
only the structure of the transport: they are replaced on insertion.
The origin tells the current project from another one: external graphs
carry their data and replace all their references (#107).
It changes after a load or a new project; it is not persisted.

The dependencies of each graph are carried as a standalone workbook
subtree in `graph_data`, validated by the same reader as the
objects. `WindowMeta.hidden` (default `false`, in the window records) saves
the hiding of the data
embedded in the `.stf` and the clipboard. Broken references are
replaced by reserved identifiers with no node in the destination,
never reused, including after save/reload. The text
transport is limited to `MAX_OBJECT_CLIPBOARD_BYTES` (64 MiB).

## Analysis calculations (#181)

`FileBody::calculations` (`#[serde(default)]`, placed before `window_geometries` and
`save_counter`) carries the calculations of `Tree::calculations()`: one
`CalculationRecord` per calculation, in `crates/stokesforge-persistence/src/calculations.rs`.

- The **operation** goes through the file as a generic tree
  (`stokesforge_analysis::tree::Tree`, written as `rmpv::Value`): a new setting or
  operation does not change the schema; absent optional field (booleans, lists,
  ranges) = default, unknown field ignored, essential field (`op`, `under`,
  `shift_initial`, `center`…) mandatory: its absence drops the calculation. Non-finite
  numbers are not written (`null`): the interface therefore accepts none.
- Series, annex and outputs reference columns by raw `NodeId`; the result
  values (`NaN` included) and the warnings are written flat.
- Staleness: `manual` and `input_missing` are kept; a `Failed(_)` is written
  `manual` (the error is not persisted: on opening an automatic calculation is
  recomputed and finds its error again, a manual calculation stays stale without a message).
  **Since A1 (#316), opening no longer recomputes any calculation** (see
  "Input fingerprint" below): it stays stale (`manual`) with the
  values of the file until "Recalculate".
- Formulas (#321): `formula_parts` — list of fragments `Text(texte)` /
  `Column(identifiant)`, the identifier bounded on reading like any identifier
  (`MAX_NODE_ID`, invariant 7). An identifier that designates no column of the
  sheet (forged file, vanished column) loads and displays as `#REF`
  (invariant 10). The old key `formula` (letters) is no longer read (change made
  before V1, when the format did not have to stay compatible): the column
  loads with its values and without a formula.
- Input fingerprint (A1, #316): each calculation writes `inputs_digest`, a
  fingerprint **of the content** of its input columns (`Calculation::inputs_digest`,
  `stokesforge-core/src/calc.rs`: type + values, little-endian `f64::to_bits`,
  texts preceded by their length, FNV-1a written by hand — the algorithm is
  frozen, it is in files —, never a column version or a `NodeId`).
  On reading, **on the values of the file, before any formula recalculation**,
  `mark_changed_inputs` compares it with the content read back: if different, the calculation is
  marked `Manual` (stale, "Recalculate" button), its values stay those
  of the file. Field absent (old file), or input missing at writing:
  nothing is compared, the calculation is believed as is (invariant 10: degrade to
  "stale", never to failure). A file edited by hand in a column
  that no calculation reads is, for its part, not rechecked.
- Tolerant reading (invariant 10): a calculation whose operation is unreadable
  (shape, or `Operation::validate` failing: a hand-written `NaN`, #291),
  or one of whose identifiers exceeds `MAX_NODE_ID`, is **dropped** without failure — its
  columns stay ordinary columns. The shape is rechecked on reading
  by the single rule of core (`validate_calculation_shape`, also called by
  `CalculationDraft::validate`): 1 to `MAX_CALCULATION_SERIES` series, outputs
  on an existing series, empty results or one per series (#309). An unknown
  warning is ignored.
  The gap is no longer silent (#278): `load_with_report` returns a `LoadReport`
  (`discarded_calculations`: id from the file, operation family if it can be read),
  which the session passes on (`SessionHandle::load_with_report`) and which `AppState`
  shows as a warning (`status-load-discarded`, see below: a single
  message counts calculations and traces). The unreadable
  record is **not kept** (it cannot be rebuilt): it would disappear
  on the next save, hence the confirmation described below (#309). Two
  calculations with the same identifier in a file: the **first** is kept, the
  second is counted among the dropped (`records_to_calculations`, and the name
  table of `records_to_corrections`). Upstream, `Project::add_calculation`/`update_calculation`
  refuse through `Operation::validate` any operation that would not read back (non-finite
  number), so that such a record is not written.
- The output columns are normal columns of the workbook: the file keeps
  their values, no recalculation is done on opening, for a manual calculation as
  for an automatic one (A1, #316); only formula columns are recomputed.
- A copy of objects (#105) does not carry the calculations: the copied columns
  become ordinary.

## What reading ruled out: say, warn, confirm (#309)

- **Single message**: `discarded_calculations_text` (`stokesforge-app-state`)
  counts dropped calculations **and** traces in a single status bar sentence
  ("2 calculations and 1 correction record were discarded from the project
  (#3, #4, #7)", keys `status-load-discarded*`); at most five identifiers then
  "…", the count stays exact. Same text at startup (`mark_loaded`) and in the
  File menu (`load`).
- **CLI**: `stokesforge-cli run --project` reads through `load_with_report`; a
  file with gaps writes `warning: … discarded` (English) to stderr, then the
  script runs (exit code 0). Neither an error nor a question.
- **Confirmation on saving**: `AppState` keeps the source path of a
  load that dropped something (`discarded_source`, set by
  `mark_loaded` and `load`). Saving **to that path** goes through the window
  "Replace File" (`Action::ConfirmDiscardedSave`,
  `pending_discarded_save`, catalog `docs/fenetres.md`): waiting is not
  a failure (no alert, `after_save` kept). Accepting writes (and clears the
  marker: the question does not come back); Cancel writes nothing, forgets the pending
  gesture and restores the path of a Save As. No question for
  Save As to another path, for a clean file, for the CLI and
  scripts, nor after a successful save; `new_project` and another
  load clear the marker. A Save As that returns to the source
  asks again as long as the gaps are not saved.

## Final data corrections (#212)

`FileBody::correction_traces` (`#[serde(default)]`) carries separately the
`CorrectionRecord`s of the tree: operation, source series, optional reference
and results. The transport reuses the encoding of the operations/results of
calculations, but recreates no active calculation or read-only column on reading.
An old file reads without traces; an unknown operation or an out-of-bounds
identifier, an invalid shape or an operation that is not a correction
makes only its trace dropped, **counted** in `LoadReport::discarded_corrections`
(#309). `main` passes the startup report to `AppState::mark_loaded`. The corrected values stay
in the ordinary columns. The counter resumes beyond the identifiers of the
traces, even if `next_id_hint` is too low. Column copies do not carry
these traces, just as they do not carry the calculations. The replaced
columns are written in `originals`, named pairs
`OriginalRecord { column, copy }` of the `CorrectionTraceRecord` (dedicated type of the
field, #323); the copy lives in
the hidden workbook of analysis copies (#248), an ordinary `hidden` workbook
marked by its window comment.

## Working path and "saved" marker (#315)

On the `AppState` side (not in the format): the "saved" marker (`saved_revision`,
which makes the "•" disappear from the title and dispenses with the "save?" question)
is set only by a **real load** (`load`, `open_project`, or
`AppState::mark_loaded` called by the binary after `stokesforge-app <chemin>`).
It is never deduced from the existence of a file at the path: otherwise a `demo.stf`
in the current folder would pass for the project and Ctrl+S would overwrite it without the check
of saving. Without an argument, the in-memory demo (`STOKESFORGE_DEMO=1`) has no path; the first-opening one is opened from `demo.stf` in the configuration folder (#333).

- `Action::SaveAs { path }` replaces the path then saves; on failure the
  previous path is **restored** (after `save`), so that the next Ctrl+S
  does not write on the file that just failed.
- No saving while a script is running: `save` refuses
  (`error-save-script-running`) and abandons the pending gesture; the button and the
  menu are disabled.
## Backup copies (#317)

The format does not change: a backup copy is an ordinary `.stf`, written by
`SessionHandle::save` (so through temporary file and rename) in `recovery/` of the
data folder (or `<STOKESFORGE_CONFIG_DIR>/recovery` when the variable is set,
#332), next to a `.json` card (original path, time) and a `.lock` lock
that do not belong to the format. It never goes through the "saved" marker:
neither `saved_revision` nor working path. Offered at the next startup
only if it is newer than its `.stf`. Details:
`docs/application/projet-et-import.md`, `stokesforge-app-state/src/recovery.rs`.

## Linked transposed sheets (#253)

`FileBody::transposes` (`#[serde(default)]`, placed after `correction_traces` and
before `window_geometries`) carries the links of `Tree::transposes()`: one
`TransposeRecord` per transposed sheet (`crates/stokesforge-persistence/src/transposes.rs`) —
the sheet, its source, the x' column, its fingerprint, the requested rows
(row, column, fingerprint) and the columns of the source as the link
read them (column, version, x' in bits). "Stale" and "modified" are **not**
written: they are derived on reading, hence the importance of the column versions
(written by the file) and of the content fingerprints (FNV-1a written in hard code, stable
from one Rust version to another).

Tolerant reading (invariant 10): a link whose sheet or source is not
a sheet of the tree, or one of whose identifiers exceeds `MAX_NODE_ID`, is dropped alone;
the sheet stays, ordinary. A column identifier that no longer exists is accepted
(the column may have been deleted): the link recreates it on refresh. The object
clipboard does not carry the links (a copy is ordinary). Additive field: it
does not change the version number of the format.
