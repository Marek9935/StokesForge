# Application: state, script console, bench, CLI, tests

Continuation of the table of contents [`README.md`](README.md). Contents: state/shell dividing line, rules of conduct, script console, what Qt gave for free, binary, tests, `stokesforge-cli`.

### Two modules, one dividing line

| Crate | Contents | Testable in CI |
|---|---|---|
| `stokesforge-app-state` (`state`…) | **all the logic**, without egui, driven by an `Action` enum | yes (`stokesforge-app-state/tests/shell.rs`) |
| `stokesforge-app` (`app`…) | the `eframe` glue: turns keyboard and mouse into actions, paints the state | no |

The line is kept by the compiler (#263), which replaced the text guard
of #255 (`aucun_module_d_etat_ne_nomme_egui`, removed): `stokesforge-app-state`
depends on none of `egui`, `eframe`, `winit`, `wgpu`,
`stokesforge-render-egui` or `stokesforge-clipboard`
(`stokesforge-app-state/tests/no_ui_dependency.rs`, allowlist read by
`stokesforge-arch-guard`). An egui type enters through `stokesforge-app`, never through
the state. It also refuses, in effect, a `crate::app` in the state (#128): the state
no longer has an `app` module to name. `RenderError` lives in `stokesforge-render`
(re-exported by `stokesforge-render-egui`) so that `errors.rs` can stay there. The
modules are imported one by one (`use stokesforge_app_state::state::Action;`), without
a `pub use` at the root; only `tr!` is exported at the crate root.

Two consequences to know:

- **Copying the figure.** The state keeps only neutral bytes
  (`state::FigureBytes`: an `ExportFormat` and its bytes, taken by
  `AppState::take_figure_clipboard`); `app/figure_copy.rs` turns them into MIME
  types (`mime_of`) and turns the error into a sentence (`clipboard_error_message`, keys
  `error-clipboard-*`) before passing it to `AppState::figure_clipboard_failed`.
- **`cfg(test)` does not cross the crate boundary.** What the state isolates under
  `cfg(test)` — the preferences path (`preferences::default_path`), the starting language
  (`i18n::startup_language`), `set_object_manager_open_for_test` — is isolated
  also under the `test-support` feature of `stokesforge-app-state`, which the
  dev-dependency of `stokesforge-app` enables: without it, the app tests
  (which build an `AppState` with `new`) would read and write the real
  `preferences.json` (`stokesforge-app/tests/isolation_preferences.rs`). This
  feature is never put in a `[dependencies]`.

A window cannot be tested in continuous integration. That is the whole reason for the
split: **nothing that `app` contains is a product decision**.

`Action`: `Undo`, `Redo`, `Save`, `Load`, `SelectSheet`, `CommitCell`,
`AddWorkbook`, `AddColumn`, `InsertRows`, `DeleteRows`, `SetNoteText`, `RunScript`,
`InterruptScript`. `AddColumn`/`InsertRows`/`DeleteRows` now carry an explicit
sheet `NodeId` (`sheet`): since the grid is scoped by
workbook (`Tab::Book`), there is no longer "the" displayed sheet on which
an implicit action could act without ambiguity when several workbooks
are open at the same time.

### Two rules of conduct

1. **`AppState::dispatch` never returns an error.** Every error becomes a
   `StatusMessage` shown to the user. A test bench that dies on an invalid
   input proves nothing. A nuance carried by the type: "impossible
   for now" (undo on empty) is **not** an error; an unreadable path
   is one.
2. **The held snapshot is frozen** (the tree is persistent). It must be
   requested again from the actor after every mutation — which is what `refresh` does, called
   by `dispatch` after any action that may have changed the model.

### The script console does not freeze the window

`ScriptEngine::run` **blocks** until the script ends. Calling it from the
render loop would freeze the whole window, "Stop" button included — the
most visible flaw a script console can have.

The remedy lives in `state`, not in `app`. Three decisions, made there and
nowhere else:

1. **The interpreter never starts when the window opens.**
   `AppState::new` does not touch `ScriptEngine`; only the first
   `RunScript` builds it. The interpreter startup time is one
   of the reasons for the port — losing it at opening would cancel the benefit.
   Locked by `stokesforge-app-state/tests/script_bridge_lazy.rs`.
2. **`RunScript` never blocks the caller**: the work goes onto a **detached
   thread** (lazy creation of the engine included) and the call returns
   at once.
3. **`poll_script` is to be called at every frame**: it polls the result with
   `try_recv` and applies it as soon as it is ready, snapshot refresh
   included. This is what makes `dispatch` safe to call from the render
   loop for **all** actions.

Without the `python` feature, the console stays visible but disabled
(`AppState::script_available`). `ScriptOutput` is a **deliberately independent**
copy of `stokesforge_script::ScriptOutcome`, so that the public surface
of `state` is the same with and without the feature.

A script exception is not an application error: its full Python trace
is shown, without ever marking the status as an error.

### What Qt gave for free, and must be written here

A `QAction` linked to a `QUndoStack` carried at once the keyboard shortcut,
the menu entry, its disabled state and its label (« Annuler *Renommer* »). Here the
four are separate: the shortcut is declared and consumed by hand
(`SHORTCUT_UNDO` and company), the disabled state goes through `add_enabled`, the label
comes from `AppState::undo_label` — and **nothing guarantees their consistency except
review**. This is the point to check first when an action is added.


### Binary

`stokesforge-app [chemin]`. If `chemin` exists, the project is loaded from that
file (and only this case marks it "saved": `AppState::mark_loaded`);
otherwise an **empty** project is built (like File › New, without a workbook)
and `chemin` becomes the target of a future save.

**Without an argument (#333)**: the demonstration is offered **only once**, at the
first opening. It is written to `demo.stf`, next to
`preferences.json` (configuration folder), then opened from that file,
which the user finds again through « Ouvrir »; the `first_run_done` marker of the
preferences is then set and the following launches start from an empty project.
If `demo.stf` cannot be written, the demonstration stays in memory, without a
path, and startup does not fail. The decision is
`stokesforge_app_state::startup::plan` (pure, tested without a window);
`main.rs` carries it out. **`STOKESFORGE_DEMO=1`** (development, screenshots, `run-stokesforge`
driver) keeps the demonstration in memory at every launch, **without** a
path (#315: the title reads "Untitled project", the first Ctrl+S opens the
picker), without setting the marker and **without asking to save on close**; a `demo.stf` in the current folder is
never taken for the project nor overwritten without a check. The example script of the
console is prefilled only with the demonstration (it reads `[Book1]Sheet1!A`). **File › Save** and the
toolbar button are disabled while a script runs (`disabled-script-running`).

The columns of the demonstration project use the automatic names
`A`, `B`, `C`…; the prefilled script reads `[Book1]Sheet1!A`, the first
column visible in the grid. Descriptive names are not used
as hidden keys of the demonstration: the readable titles are long
column names.

The demonstration lives in the code, `demo/` module of the binary (no file):
`Book1` + `Graph1` unchanged, a `Classeurs` folder (`Types`: one example of
each column type and columns with formulas; `Mesures`: the data of the
graphs; a matrix; the `Lisez-moi` note), an `Analyse` folder (one workbook
per calculation family, including `Integrate`, `FitPolynome` and `FitGaussienne` with
their calculation already done, visible in the Results panel) and six graphs (`Panneaux`,
`Axes`, `TypesTrace`, `Annotations`, `Groupes`, `Lignes`). The window names there
are unique across the whole project. `demo_session` requests a full
recalculation right after `Session::spawn`: the columns with formulas carry only
their formula (invariant 11) and would stay empty without it. The noise of the measurements
uses a fixed seed: the demonstration is the same at every launch.

The demonstration graph follows the same default as creations through the menu,
column plotting and `pyst.plot` without a `title` option: no title (#103).
An empty title is not drawn and reserves no band above the plot.
The title stays enterable in **Graph › Properties › Graph Properties… › General**; an
existing title stays editable by double-click on the figure.

Two environment variables serve tests and screenshots:
`STOKESFORGE_APP_AUTOQUIT_MS` and `STOKESFORGE_APP_SCREENSHOT`. For an
interaction (clicks, keyboard), see the `run-stokesforge` skill.

### Tests

In `stokesforge-app-state/tests/` (no window, and no egui or wgpu to
compile): `shell.rs` (the complete loop), `script_panel.rs`,
`script_bridge_lazy.rs` (the interpreter does not start by itself),
`perf_write_under_snapshot.rs`, `analysis.rs`, `analysis_state.rs`,
`project_objects.rs`, `output_names.rs`, `history_text.rs`,
`language_switch.rs` and the boundary guard `no_ui_dependency.rs`. In
`stokesforge-app/tests/`: `libelles.rs` (the translation guard and the dead
keys, see "Multilingual interface" in `theme-i18n-systeme.md`), `gardes.rs` (text guards on the
sources of both crates), `isolation_preferences.rs`, `wayland_{uri,bridge}.rs`.

## `stokesforge-cli`

Windowless binary: `import` builds a new project from instrument
files, `recipes` lists the templates and `run` (`python` feature) runs
a Python file through a headless session. The same `pyst` calls, commands
and undoable groups are available in the CLI and the console; see
[`python-et-script.md`](../python-et-script.md). The standalone Linux package is
built and verified by `scripts/cli-dist.sh`.

The application and CLI distributions also ship the licenses
of StokesForge and the third-party notices (resources, patches, CPython and numpy)
through `scripts/licenses-dist.sh`. Install its tools before assembly;
see [`licence-et-extensions.md`](../licence-et-extensions.md).
