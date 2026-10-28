---
name: stokesforge-python
description: Work on the StokesForge Python bridge (stokesforge-python, stokesforge-script) — pyst surface, script console, numpy, GIL, PYO3_CONFIG_FILE. Use for a pyst script, an API exposed to scripts, the console, or a defect seen only with the python feature (« mon script plante »).
---

Two crates behind the `python` feature, **absent from `default`**: `stokesforge-python` (embedded CPython, knows only scalars and `f64` slices) and `stokesforge-script` (script thread, interruption, the `pyst` surface; talks to the actor). Without the feature both compile **empty**: `cargo build`, `cargo test --workspace` and `clippy` stay green even if the Python code is broken.

## Read first

`docs/python-et-script.md` § What to know before modifying and `crates/stokesforge-python/README.md`; rules: `.claude/conventions.md` § `pyst` scripts.

## Steps

1. **Keep invariant 9**: no Python object crosses the actor channel — not out, not back, not as a field; the script thread releases the GIL while waiting (`Python::detach`). Why, and the read/write asymmetry that must not be "optimised": [references/bridge-rules.md](references/bridge-rules.md).
2. **The API is frozen (#331).** Adding or changing a call, parameter, default, exception, option key, returned key or enum value is an explicit decision (issue or the user's agreement), made in the same commit: update the `pyst-api` blocks of `docs/python-et-script.md` § Frozen API V1 (#331) first; `stokesforge-script/tests/surface_figee.rs` reads them.
3. **Writes reuse the undoable commands**, in the `Script` macro, with `expected_version` for any new exposed write; plotting reuses `stokesforge_core::compose`; texts seen by a script are raw English: [references/script-writes.md](references/script-writes.md).
4. **The console never freezes the window**: the interpreter starts on the first `RunScript`, never at window opening; `RunScript` returns at once; `poll_script` runs every frame. `ScriptOutput` mirrors `ScriptOutcome` so `state`'s public surface is identical with and without the feature. Detail and Windows packaging: [references/console-and-distribution.md](references/console-and-distribution.md).
5. **Licence** is plain GPL-3.0-or-later, no extension exception (#348); a runtime change updates its notices before any distribution.
6. **Run the feature explicitly**: `scripts/python-test.sh` (sets `PYO3_CONFIG_FILE`; tests the bridge, scripts, robustness, app-state, app and CLI), plus `-- --ignored` for the boundary reproductions; apply the same environment to Clippy. The interpreter is vendored in `vendor/python/` and **not versioned**: if the script fails for lack of it, say so instead of concluding the change is good.

Done when `scripts/python-test.sh` really ran green (a plain `cargo test` proves nothing here), no Python object reaches an actor request or response, any API change is in the frozen list, and `stokesforge-verification` is green.
