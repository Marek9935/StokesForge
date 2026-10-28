# Script writes, plotting and messages

Read from `stokesforge-python` before adding or changing what a script writes or sees. Contracts live in the tests named below and in `docs/python-et-script.md`.

## Writes go through the same undoable commands

- `Ctrl+Z` undoes what a script just wrote: that is the end criterion of the core, replayed by `stokesforge-script::run_selftest`. Source: `docs/python-et-script.md` § Writes and undo of a script (#67).
- The first write reserves a `Script` macro (reads open none); revalidate the handle after acquiring it. Success closes it; an uncaught error or a requested interruption aborts it, GIL released. Imports nest their macro. `undo()` ends then undoes the current group.
- `set_column` keeps a fixed length; `set_values` replaces length and cells without changing type or provenance; `True`/`False` are refused (`PyBool` tested before `PyInt`). Contract: `stokesforge-script/tests/mutations.rs`.
- **Read version (#312)**: a new write exposed to scripts carries `expected_version` (kept at read by `ScriptContext`, optional keyword `expected_version=`, immediate `ConcurrentWriteError`, no retry). Source: `docs/python-et-script.md` § Read version (#312).
- **Formulas (#321)**: `meta["formula"]` reads and writes **column letters**; the only passage is `Tree::formula_from_letters`; writing another field keeps the **stored** formula, never a round trip through letters. A script never sees an id.
- `pyst.import_file` reuses `stokesforge-importer`, then the commands in an « Importer » macro.

## Plotting without a window

`pyst.plot` creates a `PlotSpec` by `AddGraph` or adds a series to the first layer (index 0 of `PlotSpec::panels`). Composition rules are the interface's, in `stokesforge_core::compose` (#257): without `x=`, the nearest X-role column **to the left**; automatic colour by `SeriesSpec::from_palette`. A list of columns forms a group (`push_series_batch`); group gestures call `PanelSpec` methods (`crates/stokesforge-script/src/groups.rs`): a new group gesture is written in `PanelSpec` first. `add_shape` / `remove_shape` also go through the `Script` macro. Explicit titles are Typst markup; automatic labels stay literal. Contract and SVG witnesses: `stokesforge-script/tests/graphs.rs`. Source: `docs/python-et-script.md` § Native graphs (#68).

`stokesforge-cli run` uses the same engine and session, never a parallel Python API; it saves `--out` only on success (`docs/python-et-script.md` § CLI without a window (#71)).

## Texts a script sees are raw English

Exception messages of `pyst` (`UnknownPathError`, `BrokenReferenceError`, `InvalidValueError`, `ConcurrentWriteError`), numpy, interpreter and script-thread errors are **raw English**, never translated (#47). The console phrases `ScriptError` through `errors::script::error_message`; the traceback stays Python's. Output column names written by `pyst.analysis` come from `stokesforge_core::OutputName::english()` (#256): `pyst` never builds an output name itself. Docstrings stay French; parameter names English.

## Tests

`stokesforge-script/tests/selftest.rs` (end criterion), `graphs.rs`, `mutations.rs`, `navigation.rs`, `analysis.rs`, `surface_figee.rs`; `stokesforge-cli/tests/run.rs`; `stokesforge-app-state/tests/script_panel.rs` and `script_bridge_lazy.rs`; robustness `b7` (Rust ↔ Python boundary) and `b9` (script under stress).
