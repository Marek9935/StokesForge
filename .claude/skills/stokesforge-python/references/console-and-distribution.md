# Script console and distribution

Read from `stokesforge-python` when touching the console or packaging the runtime.

## The console never freezes the window

Source: `docs/application/etat-et-console.md` § The script console does not freeze the window. `ScriptEngine::run` **blocks** until the script ends, so three decisions live in `stokesforge-app-state/src/state.rs`, not in `app.rs`:

1. the interpreter never starts at window opening — only the first `RunScript` builds it (locked by `stokesforge-app-state/tests/script_bridge_lazy.rs`);
2. `RunScript` never blocks: the work goes to a detached thread and the call returns;
3. `poll_script` is called every frame and applies the result when ready.

A tiny interruption thread keeps `ScriptEngine::interrupt` non-blocking (raising an async exception needs the GIL). A Python exception is not an application error: its full trace shows, status not in error. Without the feature the console stays visible but disabled (`AppState::script_available`). `ScriptOutput` is a deliberate copy of `stokesforge_script::ScriptOutcome`: a field added to one is mirrored in the other.

The painted console lives in `stokesforge-app/src/app/script_console.rs` (`script_header`, `script_contents`, `script_panel`), tested without a window, with and without interpreter. Its texts are `script-*` keys via `tr!`. Detached console (#100): the same `script_fields_contents` paints attached and detached; `ScriptConsoleModel` holds only input, output and availability; engine and job stay in the root `AppState`; drain the input before publishing and before `RunScript`; polling continues when View hides the console.

## Windows packaging (#332)

`scripts/app-dist-windows.sh` (Git Bash) assembles the staging, validates it (window, `stokesforge-selftest` in an emptied environment, two witnesses: without `python312.dll` or without `lib/python` it must fail), compiles the installer `scripts/stokesforge.iss` (Inno Setup 6), installs into a folder with a space and an accent, reruns the selftest and uninstalls. Differences from Linux: no rpath, Visual C++ DLLs beside the exe, `PATH` reduced to `System32`, `MSYS2_ARG_CONV_EXCL='*'` for `/DIR=…`, `python -X utf8` to read Python output from Git Bash. `stokesforge-app` is a windowed subsystem in release and reattaches to the parent terminal (`console_windows.rs`).

Linux: `scripts/app-dist.sh`, `scripts/cli-dist.sh`, `scripts/pyhello-dist.sh`. Notices and policy: `docs/licence-et-extensions.md` § Third-party licenses and distribution.
