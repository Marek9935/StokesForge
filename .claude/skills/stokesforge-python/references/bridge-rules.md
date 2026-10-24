# Bridge rules: GIL, arrays, interpreter isolation

Read from `stokesforge-python` before touching the bridge or the interpreter.

## Invariant 9 — no Python object crosses the actor channel

Source: `docs/invariants.md` § 9. Aucun objet Python ne traverse le canal de l'acteur ; `docs/python-et-script.md` § Le GIL — la partie que le compilateur tient.

1. The UI thread never touches Python.
2. The script thread **releases the GIL** while waiting for the actor: every `pyst` call goes through `Python::detach`, and PyO3's `Ungil` marker forbids at compile time a GIL-bound value entering or leaving the waiting block.
3. The actor never takes the GIL: `stokesforge-session` does not depend on `stokesforge-python`.

The hidden path these rules close: a request or response carrying a Python object would be dropped on the actor thread; dropping it decrements its refcount, a Python operation that needs the GIL. Script holds the GIL waiting for the actor, actor waits for the GIL: a structural deadlock. Hence: **not out, not back, not as a field**.

## The read/write asymmetry is a decision

Source: `docs/python-et-script.md` § Tableaux : l'asymétrie est une décision.

| Direction | Mechanism | Copy? |
|---|---|---|
| Rust → Python (`column`) | numpy array **borrowing** the `Arc<Vec<f64>>`, `writeable = False` | no |
| Rust → Python (`values`) | tuple of cells `float` / `str` / `None` | yes |
| Python → Rust (write) | numpy array copied into a new `Vec<f64>` | yes |

Writes copy on purpose: the model must own its buffers (adopting Python memory would tie a column's lifetime to a Python object), and a copied `Vec<f64>` is pure Rust, droppable on any thread. Do not "optimise" this copy without rereading both reasons. `values` never re-parses texts and never confuses `Empty` (`None`), `Text("")` (`""`) and `Number(NaN)`; `column` refuses mixed columns rather than hide a copy.

## Interpreter isolation

`PYTHONHOME` alone isolates nothing (it lets `PYTHONPATH`, user `site-packages` and system `.pth` in). Initialisation goes through `Py_InitializeFromConfig` with a `PyConfig` from `PyConfig_InitIsolatedConfig`, search paths set explicitly — via `pyo3::ffi` and `unsafe` (`interpreter.rs`; the crate's other `unsafe` is the zero-copy numpy borrow in `array.rs`, although the header of `interpreter.rs` still calls its own the only one). Consequences: initialisation is explicit and lazy (`auto-initialize` off), and `Py_Finalize` is **never** called, so Python `atexit` handlers do not run. `stokesforge-pyhello` proves the isolation holds **once packaged** (`scripts/pyhello-dist.sh`): an initialisation change is checked there, not only under `cargo test`.

The embedded stack carries numpy **and scipy** (#191), pinned in `vendor/python/lock.env`; scipy is for scripts only. Changing a version: `scripts/python-vendor.sh`, `scripts/python-test.sh`, notices (`scripts/licenses-dist.sh`).
