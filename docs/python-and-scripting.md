# Python and scripts

`pyst` is the public scripting extension interface. StokesForge is under pure
GPL-3.0-or-later: no additional permission covers scripts (the 1.0.0 exception
was withdrawn, #348). License and notices:
[`license-and-extensions.md`](license-and-extensions.md).

Two crates, behind the `python` feature (absent from `default`):

- **`stokesforge-python`** — the **embedded** CPython interpreter. It does not
  know the StokesForge model: it handles only scalars and slices of `f64`.
- **`stokesforge-script`** — the script thread, the interruption, and the
  `pyst` surface offered to scripts. It knows the model, and talks to the actor.

Without the feature, both compile empty: `cargo build` and `cargo test` work
without a vendored interpreter. The `test-python` CI job prepares its runtime
and exercises this path automatically. With the feature, the build requires
`PYO3_CONFIG_FILE`, produced by `scripts/python-config.sh`.

## What these crates make true

> A Python script reads a column **without copying**, writes another one **through
> the bus**, and `Ctrl+Z` undoes what the script just wrote.

This is the completion criterion of the foundation, and
`stokesforge-script::run_selftest` replays all of it — from a packaged
distribution for the `stokesforge-selftest` binary, under `cargo test` for
`tests/selftest.rs`. Every failure, including the interpreter start-up, becomes
a failed step: never a panic.

## The embedded interpreter

`PYTHONHOME` alone **isolates nothing**: it lets `PYTHONPATH`, the user
`site-packages` and the system `.pth` files in. So initialization goes through
`Py_InitializeFromConfig` with a `PyConfig` from `PyConfig_InitIsolatedConfig`,
setting `isolated`, `use_environment`, `user_site_directory`,
`install_signal_handlers` and above all `module_search_paths` **explicitly**. No
environment variable, no deprecated setter.

PyO3 offers **no** safe API for a custom initialization configuration —
`Python::initialize()` calls `Py_InitializeEx(0)`, which reads the environment.
Going through `pyo3::ffi` is therefore mandatory, and it is the only `unsafe` of
the crate.

Initialization is **explicit and lazy** (`auto-initialize` is not enabled).
`Py_Finalize` is **never** called: it is a known source of crashes at exit, with
no benefit for a process that is ending. Accepted consequence: Python `atexit`
handlers do not run.

`stokesforge-pyhello` exists to prove that this isolation holds once packaged
(`scripts/pyhello-dist.sh`, clean environment).

### `pyst.analysis`: batches and global fit (#201)

`y` can be a list of handles: one multi-series calculation, as in the analysis
window (one Ctrl+Z, one Results row). `x` and `sigma` apply to all series (one
handle) or one per series (list of the same length; otherwise
`InvalidValueError`). The result of a batch is
`{"series": [{columns, values, errors, warnings}, …]}`, in the order of `y`;
a single handle keeps the flat dictionary. `fit(shared=['tau'])` fits globally.
Witnesses: `a_batch_is_a_single_multi_series_computation_undone_in_one_go`,
`a_malformed_batch_is_refused_with_a_message`.

### `pyst.analysis.fit`: models (#204)

`model=` accepts `linear`, `polynomial` (`degree`), `exponential`
(`A·exp(−x/τ)`), `exponential_offset` (with `y0`), `exponential2` (two
components and `y0`), `logarithm`, `power`, `gaussian`, `lorentzian`, or a
formula. `pyst.analysis.peaks(center_window=…)` bounds each center to ± this
distance from its starting position. Witness:
`the_three_exponentials_and_the_center_window_have_their_name`.

### scipy in the embedded prefix (#191)

The embedded stack carries numpy **and scipy** (`SCIPY_VERSION` of
`vendor/python/lock.env`, cp312 wheel: manylinux on Linux, `win_amd64` on
Windows; installed by `scripts/python-vendor.sh` with
`--no-deps --only-binary=:all:` like numpy).
Both are tied to CPython and to each other (the scipy wheel requires
`numpy>=2.0,<2.8`): changing one is replayed by `scripts/python-vendor.sh` then
`scripts/python-test.sh`. scipy serves `pyst` **scripts** (for example
`scipy.optimize.curve_fit` for a model that `pyst.analysis.fit` does not
offer); the interface and the menus never depend on it, all menu processing is
pure Rust (`stokesforge-analysis`). Weight: ≈ 140 MB installed
(`scipy` 109 MB + `scipy.libs` 30 MB, OpenBLAS, libgfortran and libquadmath
included), to be counted in the package. The notices go in `licenses/scipy/`
(`scripts/licenses-dist.sh`). Witness: `stokesforge-script/tests/analysis.rs`,
`scipy_fits_an_exponential_from_a_script` (`test-python` CI job).
Windows: the same script (under Git Bash, #332) installs the `win_amd64` wheel,
and the `test-python-windows` CI job runs the script tests with it.

## Arrays: the asymmetry is a decision

| Direction | Mechanism | Copy? |
|---|---|---|
| Rust → Python (`column`) | numpy array **borrowing** the `Arc<Vec<f64>>`, `writeable = False` | **no** |
| Rust → Python (`values`) | tuple of `float` / `str` / `None` cells | **yes** |
| Python → Rust (write) | copy of the numpy array into a new `Vec<f64>` | **yes** |

Writing copies **by choice**, for two independent reasons:

- the model must own its buffers — adopting memory allocated by the Python
  allocator would make the lifetime of a column depend on that of a Python
  object;
- a copied `Vec<f64>` is pure Rust: it is dropped on **any thread**, whereas
  dropping a Python object requires the GIL — which would forbid passing the
  value to the thread that owns the model.

## Two threads, not one

- the **script thread** starts CPython, becomes its main thread, and runs the
  scripts one after the other;
- a tiny **interruption thread** exists so that `ScriptEngine::interrupt` **never**
  blocks. Raising an asynchronous exception in the script thread requires the
  GIL; taking it from the thread that asks for the stop (the interface) could
  make it wait there. The interface sets a flag and sends a message; this small
  thread waits for the GIL in its place.

**Two flags, not one**, too: `busy` is set by `run` **before** the work is
sent (it serves the `ScriptError::Busy` refusal); `running` is set by the
script thread once it has **really entered** the script. The distinction
guarantees that an interruption requested while `is_running()` is true can no
longer be erased by the start-of-run reset.

## Interruption, in two halves

The interpreter starts **without signal handling**: no `KeyboardInterrupt` will
be raised by itself.

1. an **atomic flag** checked by every call of the `pyst` surface — it catches a
   script that loops while calling the API, at no extra cost;
2. **`PyThreadState_SetAsyncExc`** from the interruption thread, which raises a
   `KeyboardInterrupt` at the interpreter's next check point — it catches a
   **pure** Python loop.

*Rejected*: a `sys.settrace` hook, which would cost on every executed line,
permanently, for a granularity that is not needed.

**Limit stated honestly**: neither of them interrupts a long **native** call. A
numpy operation on ten million points runs to completion; the exception is
raised only on returning to the interpreter. A script that swallows exceptions
(`except: pass` in a pure loop) resists the second half — the first catches it
as soon as it touches `pyst`.

## The `pyst` surface

The Python work package (#65) restates the API from the code; the v1
specification in the archive is historical. Batches #66 to #68 add navigation,
cell reading, project manipulation and native graphs through undoable commands.
No call depends on an active sheet or a window: the surface works with any
session, including a session without an interface. The authoritative list
(names, signatures, keys, values) is "Frozen API V1" below; the table below
describes its effects.

| Call | Result / effect |
|---|---|
| `root()` | handle of the root folder of the current project |
| `children(handle=None)` | list of handles of the direct children, in model order; without a handle, children of the root |
| `info(handle)` | detached dictionary: name, kind, parent, row count and column metadata |
| `values(handle)` | copied tuple of the stored cells, as `float`, `str` or `None` |
| `resolve(path)` | opaque handle resolved by the model's path grammar |
| `column(handle)` | numeric/datetime numpy array, without copy and read-only |
| `set_column(handle, values)` | numeric write of the same length through the commands, undoable |
| `create_workbook(name=None, parent=None)` | empty workbook in the requested folder, root by default; handle |
| `create_worksheet(workbook, name=None)` | empty sheet at the end of the workbook; handle |
| `create_column(worksheet, name=None, column_type="numeric")` | empty column at the end of the sheet; handle; types `numeric`, `datetime`, `mixed` |
| `rename(handle, name)` | renames the node, keeping its handle |
| `remove(handle)` | removes the node and its descendants, except the root |
| `set_values(handle, values)` | replaces the values and the length of this column only |
| `set_column_meta(handle, **metadata)` | changes only the named fields, preserving provenance |
| `plot(y, x=None, *, graph=None, name=None, parent=None, **options)` | creates a native graph or adds a series to a graph; handle; without `x`, the X column to the left by role, otherwise the row index |
| `add_shape(graph, kind, x1, y1, x2, y2, *, panel=(0, 0), axis="left", line_color="", line_width=1.5, fill_color="")` | adds a graphic object in data coordinates; returns the rank in the panel |
| `remove_shape(graph, index, *, panel=(0, 0))` | removes the graphic object of this rank |
| `undo()` | ends then undoes the current group, or undoes the last already ended group; `False` if the history is empty |
| `redo()` | symmetric to `undo()`: redoes the last undone entry; `False` if there is nothing to redo (and after writes by the same script, which clear the redo stack) |
| `save()` | saves at the project's current path; `InvalidValueError` if it has none (use `save_as`) |
| `save_as(path)` | saves at this path (text or `pathlib.Path`), which becomes the current path |
| `export_graph(graph, path, *, png_scale=4.0)` | exports a graph to `.svg`, `.pdf` or `.png` (unknown extension: `InvalidValueError`), same rendering as the application's export |
| `import_file(path, recipe=None)` | import into a new workbook, one undoable macro; handles of the created sheets |
| `set_graph(graph, *, panel=(0, 0), axis="left", **options)` | sets an existing graph through the Graph Properties keys ("`set_graph`" table below); **a single history entry**; unknown key or refused value: `InvalidValueError`, nothing is written |
| `insert_rows(worksheet, at, count=1)` | inserts empty rows at row `at` (0 = at the top) in all columns of the sheet; `at` beyond the last row: `InvalidValueError` |
| `delete_rows(worksheet, at, count=1)` | deletes `count` rows from `at` in the columns of the sheet; `undo()` gives the cells back |
| `set_column_type(column, column_type)` | `numeric`, `datetime` or `mixed`; **destructive conversion** of the values the new type cannot represent (undoable); a formula column refuses |
| `move(handle, parent, index=None)` | moves under `parent` at `index` among the other children (default: last; same parent = reorder); the handle stays valid; unsuitable parent, cycle, name taken or index out of range: `InvalidValueError` |
| `duplicate(handle, name=None)` | copies the object and its descendants into the same parent; handle of the copy; a sheet right after the original (`DuplicateWorksheet`), any other object at the end of the parent (`InsertCopy`); without `name`, "`<nom> copy N`" (first free N, English: the script is not translated); the root is refused |
| `set_note(note, text)` | replaces the whole text of a note |

**Saving (#331).** `save` and `save_as` go through `SessionHandle::save`, like
the application: saving is not a command, touches neither the revision nor the
history, and handles stay valid. The script's group of writes is first
**committed** (commit point, like `undo`): the session refuses to write during
an open macro, and what was saved is no longer undone by a later error of the
script. The current path is not in the session: the engine carries it
(`ScriptEngine::set_project_path`, set by the application before each run with
the path of the open file; `pyst.save_as` changes it, and the application adopts
it at the end of the run). Without a host that sets it (CLI), the project has no
path before the first `save_as`.
`export_graph` reads the graph spec in a snapshot and writes with
`stokesforge-render-svg` (size, fonts and Typst of the application, SVG text
left as text); `stokesforge-script` therefore depends on `render-svg`, which
pulls in neither egui nor winit.

### `pyst.analysis` (#181)

`readiness(kind, **options)` (#331) says, without reading any data or writing
anything, what is missing before a calculation (`Operation::readiness`): a dict
`ready`, `reason` (`needs_points`, `needs_zone`, `bad_expression`,
`no_parameter`, `invalid`) and an English `message`; `kind` is `baseline`,
`integrate` or `fit` with the options of the function of the same name (a `nan`
= not set yet), the other calculations always being ready.

Submodule of the Analysis menu calculations: `baseline`, `correct`, `smooth`,
`integrate`, `fit`, `peaks`, `reconvolve`, `deconvolve`. `smooth(y, x=None, *,
method="adjacent"|"savitzky-golay"|"median", window=5, window_unit="points"|"x",
order=2, replace_original=False)` (#324): `window` is an odd integer number
of points or, with `window_unit="x"`, a width in X units; the written column is
`columns["smoothed"]`. Each call describes an operation
on **one** Y column (handle, optional X) and records it as a project calculation
through `Command::AddCalculation`, in the script's group of writes: a
single `pyst.undo()` undoes it with its columns. Same `compute` as the
interface, same automatic recalculation (`auto_recalc`) and same `keep_results`
option.
Return: a dictionary `{columns, values, errors, warnings}` with the stable keys
of the calculation crate (`"area.total"`, `"param.tau"`, `"peak.height.1"`…);
the warnings are in English. Long name, unit and comment of the output columns
are **those of the analysis window** (#256): a single rule,
`stokesforge_core::OutputName` (`Intensity (B-corr)`, `Baseline of B`,
`Peak 2 of B`…), the unit of the source, the family title as the comment;
`pyst` writes them through `english()`, so always in English. The name stays
frozen at creation (recalculation rewrites only the data). `peaks` has, like the
window, `peak_columns=True` (one column per peak `peak.N` and the background
`background`) and `sum_column=True` (the sum `peaks_sum`); the other helper
columns follow `keep_baseline`, `cumulative`, `normalize`, `residuals`.
**Limits**: one series per call (no batch, no global fit).
Contract: `stokesforge-script/tests/analysis.rs`. The dependency of `script` on
`stokesforge-analysis` is declared in its allowlist (`no_ui_dependency.rs`).

The constant baseline (#211) is called with
`pyst.analysis.baseline(y, x, method="constant", points=[(x0, y0)])`, or with
`zones=[(x_min, x_max)]` for the mean of the finite Y values of this inclusive
zone. Exactly one point **or** one zone is required, not combined. Like the
other baselines, `op="subtract"` subtracts the background and `op="divide"`
normalizes by this background; dividing by zero produces missing values.
Both methods are recorded with their settings in the `.stf`.

### Writes and undo of a script (#67)

Writes go through the commands the application already uses.
The first mutation opens an exclusive `MacroKind::Script` macro; later
mutations, imports included, join this group. On success, a single Ctrl+Z
undoes the whole run (label "Run Script"). An uncaught exception or a
requested interruption undoes the mutations of the current group and preserves
the previous history, redo included. A caught error lets the script continue
and commit its other writes.

A read-only script opens no macro. From the first mutation, other clients can
no longer write, save or load during the group; reads remain possible. The
engine always has a client identity distinct from its caller, even if the
caller passes it a clone of its handle. Mutations revalidate the epoch after
acquiring the macro; cell writes still carry `expected_version` (see "Read
version" below).

### Read version (#312)

Two mechanisms, which share the same version comparison:

- **Version read, kept by default.** `pyst.column` and `pyst.values` keep the
  `version` of the column read (same snapshot as the values), in a
  `(nœud, époque) → version` table of the script context, which survives from one
  console input to the next. `set_column` / `set_values` require this
  version: if the column has changed since (interface edit, other client), the
  write is refused at once with `ConcurrentWriteError`, without retry and
  without overwriting the edit. After a successful write, the kept version
  becomes the new version: two successive writes succeed. `pyst.undo()` forgets
  all the kept versions.
- **Optional `expected_version=`** (keyword) on `set_column` and `set_values`:
  it replaces the kept version. `pyst.info(h)['column']['version']` gives the
  current version; `pyst.info` keeps nothing. Priority: argument, then kept
  version, then current version.

Known limits: a column **written without a prior read** is not protected
(current version); a column **B read to compute A** protects only B, the write
of A stays unprotected if A was not read. The version covers only type and
values (invariants 8 and 11), not name or metadata. Accepted asymmetry: no value
is filtered on write, an `inf` written by a script is stored as is, whereas
import converts it to NaN or to text.

`pyst.undo()` remains usable during a script: it ends the current group then
undoes it entirely. It is an explicit commit point; a later write opens a new
group, and a later failure does not undo the history operations already
requested. Two successive writes before `undo()` are therefore undone together.

`set_column` keeps its strict numpy contract: float64, one dimension,
unchanged length, non-contiguous arrays accepted and non-empty mask refused.
`set_values` also replaces the length, by composing `SetCells` then
`DeleteCells` to remove the tail; identity, type, metadata and the other
columns stay unchanged. No tail deletion precedes the validation of the write.
The write and the tail deletion are wrapped in a nested macro (`abort_macro` if
the second fails): everything succeeds or nothing is left. The read version
follows the same rule as `set_column`.

`set_values` accepts a 1D numpy float64 array (same checks) or an iterable of
`float`, `int`, `str`, `None`. Python integers outside [-2⁵³, 2⁵³] are refused:
convert explicitly if the loss of precision is intended. `True` and `False` are
refused (`InvalidValueError`, column unchanged): convert explicitly to `int` or
`float`. A numeric/datetime column refuses text and translates `None` to NaN; a
mixed column keeps each cell as is (`None`, `""`, `"12"`, NaN and -0.0 are
distinct). Inputs are copied. Dates are POSIX timestamps in seconds.

Automatic names come from the model (`BookN`, `SheetN`, column letters). Name
length (255 characters), reserved characters, uniqueness and parent kinds keep
the model's rules. The cap of 100,000,000 rows is shared with the model and
checked before copying a numpy array; an iterable is also bounded while it is
copied. Creations return empty objects; for text data, create a `mixed` column,
then call `set_values`.

`set_column_meta` accepts `role`, `long_name`, `unit`, `comment`,
`display_format` and `formula`. Values are strings, except `formula=None`, which
removes the formula; the roles are those of `info`.
The formula is written **in column letters** of the column's sheet
(`formula="A * 2"`, case is normalized) and is read the same way; a letter that
designates no column of the sheet, the column itself or `#REF` raise
`InvalidValueError` without writing anything (#321). The script never handles
identifiers: the stored text of the formula is not rewritten by another field
(changing `unit` of a column whose formula displays `#REF` succeeds).
`type` and `version` stay read-only. A formula column refuses direct writes.
Setting a formula triggers the existing recalculation, which can change the
column type to carry calculation failures; removing the formula does not
convert this type back automatically.

```python
import numpy as np

x = pyst.column(pyst.resolve("[Book1]Sheet1!A"))
book = pyst.create_workbook("Results")
sheet = pyst.create_worksheet(book, "Computation")
result = pyst.create_column(sheet, "Signal")
pyst.set_values(result, np.asarray(x * 2, dtype=np.float64))
pyst.set_column_meta(result, role="y", long_name="Doubled signal", unit="mV")
# A single Ctrl+Z removes this workbook and all its data.
```

Contract: `crates/stokesforge-script/tests/mutations.rs` (creation, lengths,
exact cells, undo/redo, refusals, epochs, interruption and imports).

### Native graphs (#68)

`pyst.plot(y, x)` references the model's columns and creates a graph at the
root. Without `x`, each `y` takes the nearest X-role column to its left in its
sheet (Xn for a `y` of role Yn: X2 for Y2, X3 for Y3…), like the window
(`stokesforge_core::compose::x_for`, #257); the abscissa is the row index
only if the sheet has no X column to the left. **Change from #257**: before,
`plot(y)` without `x=` always gave the row index, even when a column
has the X role. An explicit `x=` keeps priority, role or not. `parent` accepts a
folder, `name` chooses the name (automatic `GraphN` otherwise). `graph=g` adds
a series to the first panel of an existing graph and keeps its other
panels, series and settings; `name` and `parent` are then refused. One call
adds one series, or several if `y` is a list of columns: these series form a group (§ Series groups, #196).
Without a `title` option, a new graph has no title; the graph name
serves to identify its window, not to title the figure (#103).

| Option | Values |
|---|---|
| `plot_type` | `line`, `scatter`, `line_scatter`, `bar`, `histogram`, `error_bar` |
| `color` | `#RRGGBB` or `#RRGGBBAA`, line and symbol (list: one color per series, string: all); shared palette otherwise |
| `line_style`, `line_width` | `none`, `solid`, `dash`, `dot`, `dash_dot`; finite positive width |
| `symbol`, `symbol_size` | `none`, `circle`, `square`, `triangle`, `diamond`, `plus`, `cross`; finite positive size |
| `label`, `title`, `x_title`, `y_title` | strings with explicit Typst markup |
| `x_scale`, `y_scale` | `linear` or `log` |
| `x_from`/`x_to`, `y_from`/`y_to` | both bounds together, finite and different, positive on a log axis; `from > to` draws the axis reversed; automatic range otherwise on a new graph |
| `axis` | `left` or `right` for the series and its Y options; makes the right axis visible |
| `x_error`, `y_error` | handles of the error columns |
| `histogram_bins` | integer from 1 to 65,536 |

The automatic label comes from the column and stays literal text: metadata
containing markup is not interpreted. Titles and labels given explicitly use
Typst, like the application's properties.
An unknown option, an invalid range or a wrong target is refused.
Handles and parameters are validated before the mutation; broken references
are detected with their epoch. The `AddGraph` and `SetPlotSpec` commands join
the same `Script` group as the column writes.

```python
x = pyst.resolve('[Book1]Sheet1!A')
y = pyst.resolve('[Book1]Sheet1!B')
g = pyst.plot(y, x, name='Graph1', title='$x^2$',
             plot_type='line_scatter', color='#123456', x_title='Time (s)')
pyst.plot(y, x, graph=g, line_style='dash', label='Second series')
```

Contract: `tests/graphs.rs` compares the exact `PlotSpec` and the shared SVG,
with a different witness and Typst; it also covers adding, rollback,
persistence and invalidated references. Matplotlib is evaluated separately by
the [spike #69](python-matplotlib-spike.md), without changing the native
rendering.

`pyst.add_shape(g, "arrow", 0, 0, 5, 5, line_color="#aa3311")` places an object
(`rectangle`, `ellipse`, `line` or `arrow`, tip at `(x2, y2)`) in data
coordinates of the base layer of the `panel=(ligne, colonne)` cell (the ordinary layer, never an overlaid layer, #167), above the others, and returns
its rank. `axis` is `left` or `right`. An empty line color is
automatic, an empty fill color means no fill.
`remove_shape` removes this rank; the following objects move down one rank.

These #76 calls join the same `Script` macro, even if they are the
first mutation of the run. An error or interruption therefore also undoes
the additions and removals of objects. Kind, epoch and panel are revalidated
after reservation; no concurrent client can overwrite the spec during this
write. Non-finite coordinates, an invalid type, axis, style or rank
are refused without mutation. Contract: `tests/script.rs` and `tests/graphs.rs`.

### `set_graph` and workbook editing (#331, batch 2)

`set_graph` writes the `PlotSpec` through a `SetPlotSpec` (like `plot`, series
groups and `add_shape`): **a single history entry** whatever the number of
keys, and a call that changes nothing writes nothing. All keys
are read and validated before writing. The list lives in one place,
`SET_GRAPH_OPTIONS` (`stokesforge-script/src/set_graph.rs`), which the surface
test will read. The Y keys target the `axis` axis (`left` by default, `right`).

| Key | Value |
|---|---|
| `title`, `panel_title`, `x_title`, `y_title`, `x_unit`, `y_unit` | strings (Typst markup for the titles) |
| `title_visible`, `stale_warning`, `decimal_comma` | booleans (graph) |
| `title_color`, `background` | `#RRGGBB` or `#RRGGBBAA`; `""` = automatic (title) or white (background) |
| `x_scale`, `y_scale` | `linear` or `log` |
| `x_from`/`x_to`, `y_from`/`y_to` | both together, finite and different, positive on a log axis; `from > to` draws the axis reversed (and marks the axis as inverted, #344); switch the framing to manual |
| `x_auto_range`, `y_auto_range` | `True` makes the framing automatic (contradictory with bounds given in the same call) |
| `x_inverted`, `y_inverted`, `x_grid`, `y_grid`, `x_visible`, `y_visible` | booleans; `x_inverted=True` on increasing bounds swaps them (the direction is read from their order, `AxisSpec::orient`, #344); the surface itself does not change |
| `legend_visible`, `legend_position` | boolean; `top_left`, `top_right`, `bottom_left`, `bottom_right` (legend of the `panel` panel) |

Outside `set_graph` for now (nothing technically prevents them, they
were not judged safe enough for this batch): fonts and anchors of the titles,
colors of the axes and of the legend, mirror axis and tick labels,
top axis, themes, and every series setting (`plot`, `set_group_style`).

The workbook editing batch sends the commands the interface uses
(`InsertRows`, `DeleteRows`, `SetColumnType`, `Move`, `DuplicateWorksheet` or
`InsertCopy`, `SetNoteText`), in the script's undoable group. A column write
by the script (`insert_rows`, `delete_rows`, `set_column_type`)
updates the version it had read of these columns (see "Read
version"): its following writes do not refuse themselves. The calculation
(`update_calculation`, `remove_calculation`) and the transposition are not
exposed.

### Series groups (#196)

`pyst.plot([y1, y2, …], x)` creates **one group**, like series added
together from the interface (`PanelSpec::push_series_batch`); a list of a
single column creates none. `x` is `None` (the X column to the left of each `y`
by its role, otherwise the row index), a shared column or a list of the
same length as `y`. `label` is a string for
one series, a list (one name per series) for a batch; `group_name` names the
group (refused for a single series); `x_error`/`y_error` serve only a
single series. Automatic colors follow the palette, series after series (`SeriesSpec::from_palette`, at the series' rank in the panel).
`color` follows the same rule as `label` (#207): a list gives one color per series (`InvalidValueError` if its length is not that of the batch); a single string applies to **all** series of the batch; for a gradient, see `apply_gradient`.

| Call | Effect |
|---|---|
| `series(g, *, panel=(0, 0))` | list of dictionaries `index`, `name`, `group` (`None` if alone), `color`, `visible`, `y` (handle) |
| `series_groups(g, *, panel)` | list of `id`, `name` (`None` = automatic name), `members` (ranks) |
| `group_series(g, indices, *, name=None, panel)` | new group in place of the first series; returns its number. The series leave their former group, which disappears with its name if it is empty |
| `set_series_group(g, index, group, *, panel)` | joins after the members, or leaves the group with `None`; returns the new rank |
| `rename_group(g, group, name, *, panel)` | `""` restores the automatic name |
| `ungroup(g, group, *, panel)` | dissolves: the series stay, style kept |
| `remove_group(g, group, *, panel)` | removes the group **and its series** |
| `set_group_style(g, group, *, panel, line_style, line_width, symbol, symbol_size)` | sets each given option on all members |
| `apply_gradient(g, group, gradient, *, panel)` | `gradient`: identifier of a provided palette (`blue-red`…), name of a palette saved **in the project**, or list of `#RRGGBB` stops |

A group number is the spec's: stable as long as the group exists. The
gestures replay those of the interface (`PanelSpec::group_series`,
`set_series_group`, `dissolve_group`, `remove_group`, `apply_group_gradient`);
groups stay contiguous. Palettes saved at user level (outside the project) are
not visible to a script, which stays reproducible. All these writes are
`SetPlotSpec` commands of the `Script` group (pattern of `add_shape`: validation
before and after reservation). Contract:
`tests/graphs.rs` (groups in one call, reading, gestures and undo with a single
`undo`, refusal without mutation, stale handles).

### CLI without a window (#71)

```sh
stokesforge-cli run --out resultat.stf calcul.pyst
stokesforge-cli run --project resultat.stf --out suite.stf suite.pyst
```

The `python` feature is required; without it, `run` explains how to enable it.
Without `--project`, the script receives an empty project; without `--out`,
there is no final save. The Python file is read as UTF-8. stdout and stderr
are preserved; an exception prints its full trace and returns a non-zero
code. `--out` is written only after success. Scripts go through the
same `ScriptEngine`, actor and commands as the console.

`scripts/cli-dist.sh` builds the Linux package `dist/stokesforge-cli/`, with the
base runtime (without Matplotlib), a relative rpath and no pip. It checks
numpy, writes, graph, save/reload and undo from this package,
without a display or `STOKESFORGE_PYTHON_HOME`, with `PYTHONHOME`/`PYTHONPATH`
booby-trapped.

### Navigation and metadata

`children` follows folders, workbooks, then sheets; a column, a graph,
a note or a matrix returns an empty list. A returned list does not update
by itself. Each call reads a single snapshot of the project, and every
returned handle keeps the epoch of that snapshot.

`info` always returns the following five keys:

| Key | Value |
|---|---|
| `name` | short name of the node, or project name for its root |
| `kind` | `folder`, `workbook`, `worksheet`, `column`, `graph`, `matrix`, `note` or `unknown` |
| `parent` | handle of the parent; `None` at the root |
| `row_count` | length of a column; maximum of the lengths for a sheet (zero if empty); `None` elsewhere |
| `column` | dictionary below for a column; `None` elsewhere |

`values` and `set_values` of a `datetime` column stay numbers (POSIX timestamps, seconds): only the grid displays and reads dates (#218).

The `column` metadata carry `type` (`numeric`, `datetime`, `mixed`),
`role` (`x`, `y`, `x2`, `y2`, `x3`, `y3`, `x4`, `y4`, `z`, `x_error`, `y_error`, `label`, `none`), `long_name`,
`unit`, `comment`, `x_prime` (number or `None`, #164, read-only), `display_format`,
`formula` (column letters, `#REF` for a read column that has disappeared, or `None`) and `version`
(modification counter). Modifying this dictionary never touches the
model. Read `info` again after a modification to get the current state.

```python
for objet in pyst.children():
    print(pyst.info(objet)["name"], pyst.info(objet)["kind"])

feuille = pyst.resolve("[Book1]Sheet1")
for colonne in pyst.children(feuille):
    description = pyst.info(colonne)
    print(description["name"], description["column"]["unit"],
          description["row_count"])
```

### Text and mixed cells

The current model no longer has a homogeneous `Text` type: a text column
is a `mixed` column containing text cells. `values` preserves the
stored type of each cell: a text `"12.5"` stays a string, an empty cell
becomes `None`, an empty string stays `""`, and a NaN number stays a
`float` NaN. Dates stay POSIX timestamps in seconds.

```python
cellules = pyst.values(pyst.resolve("[Book1]Sheet1!Etiquette"))
# Exemple : ("échantillon A", None, "", "12.5")

import numpy as np
numerique = pyst.column(pyst.resolve("[Book1]Sheet1!A"))  # sans copie
mixte = np.array(cellules, dtype=object)                # copie explicite
```

`values` copies in time and memory proportional to the number of cells,
including for a numeric column; its tuple is an immutable snapshot.
`column` keeps its numeric contract without copy and refuses a mixed
column with `InvalidValueError`. Reads change neither the revision nor
the history. A target other than a column makes `values` fail with
`InvalidValueError`; any deleted handle, or one from a reloaded project, is
refused with `BrokenReferenceError`, before looking at its kind.

`pyst.import_file("spectre.csv", recipe="My Jasco")` (a template you saved) accepts a
text path or `pathlib.Path` and returns the list of handles of the sheets
created. Without a template name, it applies the automatic choice, otherwise the
permissive recipe. It respects the template settings (ignored columns, types,
metadata, blocks and split into sheets) and always creates a new
workbook; the modes that modify an active sheet concern the wizard.
A single `pyst.undo()` or Ctrl+Z removes the whole import. An unknown template or an
unreadable file raises `InvalidValueError` before any mutation; an error
during creation aborts the macro. Parsing and commands run with
the GIL released, like the other calls to the model.

A handle (`Handle`) is **opaque**: no Python constructor, no exposed integer;
resolution, navigation, creations and import produce them. It stays
valid after a rename, a move, and even after a deletion
**followed by an undo** — this is
the identity contract, ensured by the model (monotonic identifiers, never
reused). It is invalidated by a `Load` or a `NewProject`, detected through the `epoch` of
the snapshot even before looking up the `NodeId` in the current tree
([`session.md`](session.md)).

Four exceptions, and only four: `UnknownPathError`,
`BrokenReferenceError`, `InvalidValueError`, `ConcurrentWriteError`. The first
three come as is from the v1 specification, to avoid renaming
them later; the fourth distinguishes the immediate refusal of a write whose column has
changed since it was read (#312, without retry) from the other refusals.
Every host error — `SessionError`, `CommandError`, `CoreError` — is
**translated** into one of them, never carried as is through the
channel.

## Frozen API V1 (#331)

This is the **reference list** of the `pyst` surface: names,
signatures, exceptions, option keys, keys of the returned dictionaries
and their enumerated values. A script written against it must keep
working; the tables of the neighboring sections **comment on** it (effects,
examples) but do not replace it. Docstrings and error messages stay
outside the list (S8: both in English; docstrings since #376).

**Change rule.** Any change to this list — call, parameter,
default, exception, option key, key or value of a return — is an **explicit
decision** (issue or user request), made **in the same commit**
as the code: the relevant block is updated here, and only then the code.
An addition is an evolution of the API, not a detail; a removal or a rename
breaks scripts. The test `crates/stokesforge-script/tests/frozen_api.rs`
reads the blocks below (it keeps no copy of them) and fails on any
difference with the module actually built, saying which one.

Format (read by the test): one ```` ```pyst-api <genre> <nom> ```` block per
list, one entry per line.

- `module <nom>`: `function nom(signature)` (the signature is the one
  `inspect.signature` returns, default values included; two defaults that PyO3
  cannot write, `panel=(0, 0)` and `png_scale=4.0`, appear in plain form and the
  test matches them to the `Ellipsis` that Python returns), `class nom`,
  `exception nom` or `module nom` (submodule). Names that start with `_`
  are not public and are not listed.
- `options <appel>`: the keys accepted by `**options` / `**metadata`. For
  `set_graph`, the list is the constant `SET_GRAPH_OPTIONS`
  (`stokesforge-script/src/set_graph.rs`); for `plot`, `PLOT_OPTIONS`
  (`graph.rs`); for `set_column_meta`, `COLUMN_META_FIELDS` (`surface.rs`).
  The "unknown key" message of each one lists them, and the test compares this
  message to this block.
- `returns <nom>`: the keys of the returned dictionary (the order is not fixed).
- `enum <nom>`: the enumerated values, as input (accepted) and/or as output
  (returned). Any other value is refused with `InvalidValueError`, except the
  `model` of `fit`, where an unknown text is read as a formula.

### Module `pyst`

```pyst-api module pyst
exception BrokenReferenceError
exception ConcurrentWriteError
class Handle
exception InvalidValueError
exception UnknownPathError
function add_shape(graph, kind, x1, y1, x2, y2, *, panel=(0, 0), axis='left', line_color='', line_width=1.5, fill_color='')
module analysis
function apply_gradient(graph, group, gradient, *, panel=(0, 0))
function children(handle=None)
function column(handle)
function create_column(worksheet, name=None, column_type='numeric')
function create_workbook(name=None, parent=None)
function create_worksheet(workbook, name=None)
function delete_rows(worksheet, at, count=1)
function duplicate(handle, name=None)
function export_graph(graph, path, *, png_scale=4.0)
function group_series(graph, indices, *, name=None, panel=(0, 0))
function import_file(path, recipe=None)
function info(handle)
function insert_rows(worksheet, at, count=1)
function move(handle, parent, index=None)
function plot(y, x=None, *, graph=None, name=None, parent=None, **options)
function redo()
function remove(handle)
function remove_group(graph, group, *, panel=(0, 0))
function remove_shape(graph, index, *, panel=(0, 0))
function rename(handle, name)
function rename_group(graph, group, name, *, panel=(0, 0))
function resolve(path)
function root()
function save()
function save_as(path)
function series(graph, *, panel=(0, 0))
function series_groups(graph, *, panel=(0, 0))
function set_column(handle, values, *, expected_version=None)
function set_column_meta(handle, **metadata)
function set_column_type(column, column_type)
function set_graph(graph, *, panel=(0, 0), axis='left', **options)
function set_group_style(graph, group, *, panel=(0, 0), line_style=None, line_width=None, symbol=None, symbol_size=None)
function set_note(note, text)
function set_series_group(graph, index, group, *, panel=(0, 0))
function set_values(handle, values, *, expected_version=None)
function undo()
function ungroup(graph, group, *, panel=(0, 0))
function values(handle)
```

A `Handle` has no public attribute or method (no constructor, no exposed
integer): `dir(pyst.Handle)` without the `_` names is empty. The four
exceptions derive from `Exception`.

### Module `pyst.analysis`

```pyst-api module pyst.analysis
function baseline(y, x=None, *, method='anchors', points=None, zones=None, degree=1, interpolation='segments', op='subtract', keep_baseline=False, keep_results=False, auto_recalc=True, replace_original=False)
function correct(y, reference, x=None, reference_x=None, *, op='divide', keep_results=False, auto_recalc=True, replace_original=False)
function deconvolve(y, response, x=None, response_x=None, *, method='richardson-lucy', iterations=50, noise=None, shift=0.0, keep_results=False, auto_recalc=False)
function fit(y, x=None, *, model='linear', degree=3, initial=None, bounds=None, fixed=None, shared=None, fit_range=None, residuals=False, sigma=None, keep_results=False, auto_recalc=True)
function integrate(y, x=None, *, ranges=None, under='zero', cumulative=False, normalize=False, keep_results=False, auto_recalc=True)
function peaks(y, x=None, *, shape='gaussian', background='constant', centers=None, link_widths=False, center_window=None, prominence=0.05, max_peaks=6, fit_range=None, residuals=False, peak_columns=True, sum_column=True, keep_results=False, auto_recalc=True)
function readiness(kind, **options)
function reconvolve(y, response, x=None, response_x=None, *, components=1, shift=0.0, fit_shift=True, window=None, residuals=False, keep_results=False, auto_recalc=False)
function smooth(y, x=None, *, method='adjacent', window=5.0, window_unit='points', order=2, keep_results=False, auto_recalc=True, replace_original=False)
```

### Option keys

```pyst-api options plot
plot_type
color
line_style
line_width
symbol
symbol_size
label
group_name
title
x_title
y_title
x_scale
y_scale
x_from
x_to
y_from
y_to
axis
x_error
y_error
histogram_bins
```

```pyst-api options set_graph
title
title_visible
title_color
background
stale_warning
decimal_comma
panel_title
x_title
y_title
x_unit
y_unit
x_scale
y_scale
x_from
x_to
y_from
y_to
x_auto_range
y_auto_range
x_inverted
y_inverted
x_grid
y_grid
x_visible
y_visible
legend_visible
legend_position
```

```pyst-api options set_column_meta
role
long_name
unit
comment
display_format
formula
```

### Return shapes

```pyst-api returns info
name
kind
parent
row_count
column
```

```pyst-api returns info.column
type
role
long_name
unit
comment
x_prime
display_format
formula
version
```

```pyst-api returns series.item
index
name
group
color
visible
y
```

```pyst-api returns series_groups.item
id
name
members
```

A `pyst.analysis` calculation returns `{columns, values, errors, warnings}`; for
a list of `y`, `{series: [ce dictionnaire, …]}`. The keys of `values`
(`"area.total"`, `"param.tau"`…) are those of the calculation crate, outside this
list. The keys of `columns` below are frozen for the three operations
that always write the same columns.

```pyst-api returns analysis
columns
values
errors
warnings
```

```pyst-api returns analysis.batch
series
```

```pyst-api returns analysis.baseline.columns
corrected
baseline
```

```pyst-api returns analysis.correct.columns
corrected
```

```pyst-api returns analysis.smooth.columns
smoothed
```

```pyst-api returns readiness
ready
reason
message
```

`readiness` adds `needed` and `have` when `reason` is `needs_points`.

```pyst-api returns readiness.needs_points
ready
reason
message
needed
have
```

### Enumerated values

```pyst-api enum kind
folder
workbook
worksheet
column
graph
matrix
note
unknown
```

```pyst-api enum type
numeric
datetime
mixed
```

```pyst-api enum column_type
numeric
datetime
mixed
```

```pyst-api enum role
x
y
x2
y2
x3
y3
x4
y4
z
x_error
y_error
label
none
```

```pyst-api enum plot_type
line
scatter
line_scatter
bar
histogram
error_bar
```

```pyst-api enum line_style
none
solid
dash
dot
dash_dot
```

```pyst-api enum symbol
none
circle
square
triangle
diamond
plus
cross
```

```pyst-api enum scale
linear
log
```

```pyst-api enum axis
left
right
```

```pyst-api enum legend_position
top_left
top_right
bottom_left
bottom_right
```

```pyst-api enum shape_kind
rectangle
ellipse
line
arrow
```

```pyst-api enum export_extension
svg
pdf
png
```

```pyst-api enum analysis_kind
baseline
correct
smooth
integrate
fit
peaks
reconvolve
deconvolve
```

```pyst-api enum readiness_reason
needs_points
needs_zone
bad_expression
no_parameter
invalid
```

```pyst-api enum baseline_method
constant
anchors
line
zones
```

```pyst-api enum baseline_op
subtract
divide
```

```pyst-api enum integrate_under
zero
line
```

```pyst-api enum smooth_method
adjacent
savitzky-golay
median
```

```pyst-api enum fit_model
linear
polynomial
exponential
exponential_offset
exponential2
logarithm
power
gaussian
lorentzian
```

## The GIL — the part the compiler holds

The three rules and their hidden path are in [`invariants.md`](invariants.md)
(invariant 9). Their trace in the code: all of `bridge.rs` runs
inside a `Python::detach`, GIL released, and **nothing there names a PyO3
type** — this is what lets the `Ungil` marker check at compile time
that no GIL-bound value crosses the wait on the actor.

Path resolution is no longer rewritten in the bridge: `stokesforge-core` exposes
`Tree::resolve`, the repository's only implementation of the `[Book1]Sheet1!ColA`
grammar, and the bridge calls it on the tree of its snapshot.

## Output capture

`sys.stdout` is **global to the interpreter**. The usual recipe — save,
replace, restore — is correct only if a single actor uses it at a
time; two script engines in the same process would steal each other's
restore.

So a single relay object is set **once** in `sys.stdout`/`sys.stderr`,
and it does the routing: to the buffer of the **current thread** if there is one,
otherwise to the real output of the process. The object is shared and stateless; the
only state lives in a `thread_local!`. Intended consequence: a `print` run
outside a script keeps going to the process output.

## What to know before modifying

- An error *of the script* is not an engine error: it lands in
  `ScriptOutcome::error` with its Python trace. `ScriptError` is reserved for
  **host** failures (thread start-up, engine busy).
- `stokesforge-script/tests/navigation.rs` checks the contract of batch #66: order,
  metadata, lengths, exact values, absence of mutation and identity of
  handles after rename, move, delete/undo and load.
- Run the Python tests: `scripts/python-test.sh`, which sets the three
  required environment variables and tests bridge, scripts, robustness,
  application and CLI. The `script_panel` scenario (`stokesforge-app-state/tests/`) reads numpy, creates the results
  and a graph, then undoes/redoes the whole from `AppState`. On the robustness side, `b7` (Rust ↔ Python
  boundary in both directions) and `b9` (script under constraint) are
  behind the same feature.

**To read**: `stokesforge-script/src/lib.rs`, `src/engine.rs`, `src/surface.rs`,
`src/bridge.rs`, `stokesforge-python/src/interpreter.rs`, `src/array.rs` (heads).

### Replacing a correction in its source (#212)

`pyst.analysis.baseline(..., replace_original=True)` and
`pyst.analysis.correct(..., replace_original=True)` (and `smooth`, #324)
write into the source Y columns instead of adding columns.
`columns['corrected']` (`columns['smoothed']` for `smooth`) then designates the
original source. Example:

```python
r = pyst.analysis.baseline(y, x, method="constant", points=[(0., 1.)],
                           replace_original=True)
```

The correction is no longer re-editable nor recalculated; its trace and settings
are kept in Results and in the `.stf`, even if `keep_results=False`.
`auto_recalc` and the additional columns do not apply to
replacement. Formula or calculated columns are refused. A batch that
produces contradictory corrections on the same Y is refused before any
write. The versions of the sources are revalidated in the script's
reservation. The replacement joins its Script macro: Undo reverts all the
corrections of the script at once. The script explicitly asks for the
replacement; the confirmation for shared sources belongs to the interface.
