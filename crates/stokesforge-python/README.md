# stokesforge-python — embedded CPython interpreter

This crate is the **only** one in the repository allowed to start CPython and to give
access to the GIL. It does not know StokesForge's data model: it only handles
scalars and slices of `f64`.

This version delivers task **5.1a**: a **packaged** *hello-world* that
proves that a complete Python stack (frozen interpreter + numpy) starts from a
standalone distribution directory, **fully isolated from the system Python**.
It is the lock of phase 5: packaging is the heaviest risk of the
architecture, and it is lifted before writing the rest of the bridge.

## Version lock

A single source of truth: [`vendor/python/lock.env`](../../vendor/python/lock.env),
read by the scripts (bash) **and** by the tests (`include_str!`).

| Item | Version |
|---|---|
| CPython (python-build-standalone `20260901`, `x86_64-unknown-linux-gnu`, `install_only_stripped`) | `3.12.14` |
| numpy wheel (`cp312`, manylinux x86_64) | `2.5.3` |
| Crate `pyo3` | `0.29.2` |
| Crate `numpy` (rust-numpy) | `0.29.0` |

These four values go together (pitfall no. 6): the numpy ABI is tied to the CPython
ABI, rust-numpy is tied to a precise version of PyO3, itself compiled
against this interpreter. One is never changed without rerunning
`scripts/python-vendor.sh` then `scripts/python-test.sh`.

## Build and verify

Before the distribution scripts, install the pinned license tools
according to [`docs/license-and-extensions.md`](../../docs/license-and-extensions.md).
They also deliver the notices of the code, the resources, CPython and
numpy. An update of the Python lock refreshes
[`vendor/python/licenses-linux/`](../../vendor/python/licenses-linux/README.md) and
[`licenses-windows/`](../../vendor/python/licenses-windows/README.md).

```bash
scripts/python-vendor.sh     # interpreter + pinned numpy in vendor/python/extracted/python
scripts/python-test.sh       # bridge, scripts, robustness, application and CLI against the vendored prefix
scripts/python-memcheck.sh   # zerocopy.rs under valgrind memcheck — explicit no-op if valgrind is absent
scripts/pyhello-dist.sh      # assembles dist/pyhello/ and VALIDATES it in a cleaned environment
```

Without the `python` feature, none of this is needed:

```bash
cargo build   -p stokesforge-python                       # no PyO3 dependency compiled
cargo test    -p stokesforge-python
cargo clippy  -p stokesforge-python --all-targets -- -D warnings
```

The `python` feature is **not** in `default`: the ordinary Cargo checks
work without a vendored interpreter; the Python CI job
prepares its own runtime.

### What CI covers, and what it does not

The three ordinary jobs of `.gitlab-ci.yml` (`fmt`, `clippy`, `test`)
**never** enable the `python` feature: this crate compiles **empty** there, and
nothing of this module is either checked by clippy or tested by these three jobs.
A green pipeline on these three jobs therefore says nothing about the Python bridge.

A fourth job, `test-python`, covers this bridge: it vendors
the interpreter, generates the PyO3 configuration and runs `scripts/python-test.sh`
— exactly the chain described above. It is **automatic and blocking**
(#70). The archive and the prefix are cached according to the version lock;
the Python builds have a distinct Cargo cache key. The script also tests
the script thread, robustness, the console and the CLI. The ignored
reproductions are not run in this job.

## Verify the foundation yourself

The end criterion of **the whole** foundation (a cell edit undone by
`Ctrl+Z` and redone by `Ctrl+Y`; `Ctrl+S` then `Ctrl+O` identical; a
Python script that reads a column without copy, writes another through the bus,
undone by `Ctrl+Z`) is replayed with a single command, **from a packaged
binary**:

```bash
scripts/app-dist.sh                    # builds dist/stokesforge-app/, runs stokesforge-selftest there in a trapped environment, fails if its exit code is not 0
```

To rerun only the self-check from the already built package
(without rebuilding or reopening the window):

```bash
dist/stokesforge-app/stokesforge-selftest        # replays the end criterion from dist/stokesforge-app/; --quiet to show only the failing steps and the verdict
```

And from the source tree, without packaging, as an ordinary `cargo`
test (the same logic as the binary above, exercised by
`crates/stokesforge-script/tests/selftest.rs`):

```bash
scripts/python-vendor.sh               # once: vendored interpreter + numpy (if not already done)
scripts/python-test.sh                 # runs tests/selftest.rs (and tests/script.rs) against the vendored prefix
```

All the verification logic lives in `stokesforge_script::run_selftest`
(`crates/stokesforge-script/src/selftest.rs`): the `stokesforge-selftest` binary and the
`tests/selftest.rs` test are two thin shells around the same code,
which guarantees that what the package proves and what `cargo test` covers never
diverge.

## The seven pitfalls and how they are handled here

### 1. PyO3 does not find `libpython` at build time

The `sysconfig` of python-build-standalone returns `LIBDIR = /install/lib` — the
path on Astral's build machine, nonexistent here. PyO3's automatic detection
is therefore bypassed by a hand-written `PYO3_CONFIG_FILE`.

* Versioned template: [`pyo3-config.template.txt`](pyo3-config.template.txt).
* Generation: [`scripts/python-config.sh`](../../scripts/python-config.sh) substitutes
  the absolute path of the prefix (machine-dependent, hence not versioned) and
  writes `target/pyo3-config.txt`, whose path it prints on `stdout`:
  `export PYO3_CONFIG_FILE="$(scripts/python-config.sh)"`.
* The script **removes the comment lines** of the template: `pyo3-build-config`
  reads only `key=value` pairs.

### 2. The binary does not find `libpython` at run time

**Relative** `rpath` set at link time, never `LD_LIBRARY_PATH`:

* distribution — `scripts/pyhello-dist.sh`: `RUSTFLAGS='-C link-arg=-Wl,-rpath,$ORIGIN/lib'`
  (**single** quotes: `$ORIGIN` must reach the linker literally);
* tests and development — `scripts/python-test.sh`: same mechanism with an **absolute**
  rpath to `vendor/python/extracted/python/lib`, since the test binary
  is not distributed.

The flags go through `RUSTFLAGS` and not through `.cargo/config.toml`: the section
`[target.x86_64-unknown-linux-gnu]` would be **workspace-global** there and
would impose this rpath on all the other binaries of the repository.

Proof, in the output of `scripts/pyhello-dist.sh`:

```
libpython3.12.so.1.0 => …/dist/pyhello/lib/libpython3.12.so.1.0
```

### 3. Static linking of `libpython`

Rejected: the compiled extension modules (numpy's, those of `lib-dynload`)
expect a **shared** `libpython`. We take the
`install_only_stripped` variant of python-build-standalone, which provides a
**shared** `libpython3.12.so.1.0` (like `install_only`, from which it differs
only by the absence of native debug symbols), and we embed it. No single
binary — it is a choice, not an accident (`shared=true` in the
PyO3 config template).

**`install_only` → `install_only_stripped`**: the unstripped `libpython3.12.so.1.0`
weighs 209 MB, most of the 307 MB of the original `dist/pyhello/` (§
Measurements). The stripped variant brings it down to 31 MB (measured), without changing
the fact that it stays shared. What is lost: **native** debugging of
CPython — checked here with `readelf -S`: the `.debug_*` sections (DWARF,
lines/types) are gone, so `gdb` can no longer map
address → source line in `libpython`. A measured nuance, finer than
"no function name any more": the ELF symbol table (`.symtab`/`.dynsym`)
is **kept** — `nm` still lists the C function names in it — so
a native call stack keeps its function names, only no longer their
lines or types. What is **not** lost, checked by running a
Python script that raises an exception under the stripped interpreter:

```
Traceback (most recent call last):
  File "…/preuve_traceback.py", line 7, in <module>
    niveau_un()
  File "…/preuve_traceback.py", line 5, in niveau_un
    niveau_deux()
  File "…/preuve_traceback.py", line 2, in niveau_deux
    raise ValueError("preuve : traceback complet apres stripping de libpython")
ValueError: preuve : traceback complet apres stripping de libpython
```

File, line number, function name and exception type are all
present: Python tracebacks come from the interpreter (CPython code objects
carry their own `co_filename`/`co_firstlineno`), not from the native ELF
symbols — binary stripping does not touch them.

### 4. `PYTHONHOME` alone isolates nothing

It lets `PYTHONPATH`, the user `site-packages`
(`~/.local/lib/python3.12/…`) and the system's `.pth` files in. Initialization is therefore done
through `PyConfig` in isolated mode — `src/interpreter.rs`, function
`start_interpreter`:

| Field | Value | Effect |
|---|---|---|
| `isolated` | `1` | `sys.flags.isolated == 1` |
| `use_environment` | `0` | no `PYTHON*` variable is read |
| `user_site_directory` | `0` | no `~/.local/lib/python3.12` |
| `install_signal_handlers` | `0` | CPython does not touch the process's signals |
| `parse_argv` | `0` | the Rust binary's `argv` is never interpreted by Python |
| `home` | embedded prefix | sets `sys.prefix` / `sys.exec_prefix` |
| `module_search_paths_set` | `1` | CPython no longer computes `sys.path` |
| `module_search_paths` | `lib/python3.12`, `…/lib-dynload`, `…/site-packages` | **exhaustive** `sys.path`, in this order |

Measured result in `dist/pyhello/`, trapped environment: `sys.path` contains
**exactly 3 entries**, all under the embedded prefix. `site.py` adds
nothing. The `pyhello` binary checks itself: it exits with an error if a single
`sys.path` entry lies outside the prefix.

### 5. `Py_SetPythonHome`/`Py_SetProgramName` deprecated (3.11), removed (3.13)

None of these setters is used. The chosen path is
`PyConfig_InitIsolatedConfig` → `PyConfig_SetBytesString` (for `home`) →
`Py_DecodeLocale` + `PyWideStringList_Append` (for each module path) →
`Py_InitializeFromConfig` → `PyConfig_Clear`, through `pyo3::ffi`
(`src/interpreter.rs`, functions `start_interpreter` and `append_search_path`).

**PyO3 0.29 offers no safe API for a custom initialization
configuration**: `Python::initialize()` calls `Py_InitializeEx(0)`, which reads
the environment. Going through `pyo3::ffi` is therefore mandatory — it is the only
`unsafe` of the crate, confined to two functions.

A detail not listed in the pitfalls but essential: after
`Py_InitializeFromConfig`, the calling thread **holds the GIL**. `start_interpreter`
releases it with `PyEval_SaveThread()` — without which any `Python::attach` from
another thread would block. It is the same gesture as PyO3's `Python::initialize`.

### 6. numpy is tied to a precise interpreter version

Handled by the version lock above. Two automatic guards:

* `scripts/python-vendor.sh` refuses to continue if the extracted interpreter is
  not the lock's version, and rechecks `numpy.__version__` after installation;
* `tests/embedded.rs` reads `lock.env` through `include_str!` and compares
  `python_version()` and `numpy_version()` with the locked values — a
  mismatch breaks the tests, it does not pass silently.

Installation is done with `--no-deps --only-binary=:all:`: never a
local build, never an unplanned dependency.

### 7. No `Py_Finalize`

`Py_Finalize` is **never** called, anywhere. It is a known source
of crashes on exit (extension modules that outlive finalization) and
brings nothing to a process that is ending: the system reclaims the memory.

**Accepted consequence: Python `atexit` handlers do not run.** Anything that
must be flushed to disk at shutdown must be done on the Rust side, explicitly.
No user script can count on `atexit` — to be documented in the help
of the script console (task 5.4).

## Layout of `dist/pyhello/`

```
dist/pyhello/
├── LICENSE                                   StokesForge license
├── THIRD-PARTY-LICENSES.txt                  texts of the Rust dependencies
├── licenses/                                resources, patches, CPython, numpy
├── pyhello                                  release binary (0.7 MB)
└── lib/
    ├── libpython3.12.so.1.0                 31 MB (stripped), target of the $ORIGIN/lib rpath
    ├── libpython3.12.so -> libpython3.12.so.1.0
    └── python/                              = embedded prefix (97 MB), logical $PYTHONHOME
        └── lib/
            ├── libtcl9.0.so, …              auxiliary libraries of the prefix
            └── python3.12/
                ├── …                        standard library (+ shipped __pycache__)
                ├── lib-dynload/             core extension modules
                └── site-packages/
                    ├── numpy/               43 MB
                    └── numpy.libs/          27 MB (OpenBLAS)
```

`libpython3.12.so.1.0` is the only piece affected by the move to
`install_only_stripped` (209 MB → 31 MB): the standard library, numpy and
`numpy.libs` are outside the *flavor*'s scope and keep their size.

`EmbeddedPython::locate()` looks for `<executable dir>/lib/python`, which
gives exactly this layout. In development, the `STOKESFORGE_PYTHON_HOME` variable
bypasses this resolution — **only** for tests and development, never as a user
mechanism.

Three assembly choices, all in `scripts/pyhello-dist.sh`:

* `bin/` is **excluded** (98 MB): initialization through `PyConfig` needs only
  `home` and the module paths, never a `python3` executable;
* `pip` is **removed** from `site-packages` (12 MB): an architecture decision, pip
  is not exposed to the user; it is used only at build time (`python-vendor.sh`);
* `libpython3.12.so.1.0` is copied **only once**, in `lib/`, not in
  `lib/python/lib/`: duplicating it would add 209 MB for nothing, the `lib-dynload`
  modules reference it by SONAME and find it already loaded;
* the `__pycache__` shipped by python-build-standalone are **kept**: removing
  them takes initialization from ~50 ms to ~250 ms (recompilation of the
  standard library), and a distribution installed read-only
  could never rebuild this cache.

## Measurements

Fedora, x86_64, `dist/pyhello/pyhello` run under
`env -i PATH=/usr/bin:/bin HOME=/tmp PYTHONPATH=/tmp/piege PYTHONHOME=/tmp/piege2`,
`--release` binary, average of three runs with a warm cache:

| Measurement | Time |
|---|---|
| (a) initialization: CPython startup + `import numpy` | **≈ 55 ms** (54–55) |
| (b) send 1 M `f64`: Rust → numpy `ndarray` copy | **≈ 0.7 ms** |
| (c) return 1 M `f64`: numpy `ndarray` → `Vec<f64>` | **≈ 0.4 ms** |

Size of `dist/pyhello/`: **128 MB** (`install_only_stripped`), against
**307 MB** with the original `install_only` distribution — the decision
"switch to *stripped*", taken after 5.1a, has been carried out: see § 3
above for what it costs and what it does not cost. These timing
measurements do not move with the *flavor*: stripping removes ELF
debug sections, not executed code.

Reading: copying 8 MB of `f64` costs **less than a millisecond per direction**,
about 10 GB/s. Zero-copy (task 5.2) will therefore **not** be justified by
CPU time but by **memory**: each round trip duplicates the column.

## Zero-copy: reading without copy, writing by copy (task 5.2)

The second lock of the phase: the array round trip between Rust and numpy,
with **zero-copy reading**. Implemented in [`src/array.rs`](src/array.rs),
verified by the 19 tests of [`tests/zerocopy.rs`](tests/zerocopy.rs).

### The asymmetry is a decision

| Direction | Mechanism | Copy? |
|---|---|---|
| Rust → Python (read) | numpy array **borrowing** the `Arc<Vec<f64>>`, `writeable = False` | **no** |
| Python → Rust (write) | copy of the numpy array into a fresh `Vec<f64>` | **yes** |

Writing copies **by choice**: the model must own its buffers — adopting
memory allocated by the Python allocator would make the lifetime
of a column depend on that of a Python object. A second, independent reason: a copied
`Vec<f64>` is pure Rust, it is destroyed on any thread — whereas
destroying a Python object requires the GIL, which would forbid passing the value
to the thread that owns the model (task 5.4).

### The borrow mechanism and its safety contract

[`borrowed_readonly_array`] returns a numpy view that points **exactly** to
`data.as_ptr()`, through the module's only `unsafe`,
`PyArray::borrow_from_array`. Its safety clause requires that the borrowed buffer
be neither reallocated nor freed as long as the `container` set as the array's `base`
is alive.

The `container` is [`FloatBufferOwner`]: a `#[pyclass(frozen)]` that holds
a **clone** of the `Arc`. It is this clone, and only this one, that makes concurrent
writing safe: as long as a view exists, the reference count is
**≥ 2**, so any write on the model side goes through `Arc::make_mut`, which — with
a count ≥ 2 — **allocates a fresh buffer** and leaves the old one intact. Neither
in-place mutation, nor reallocation (growth of the `Vec`), nor freeing can
reach the buffer under the view. The safety clause is honored **by
construction, not by vigilance**: no calling discipline is required of
whoever uses `borrowed_readonly_array`.

`writeable = False` is a guard, not the safety boundary: it catches
the slip of attention (`col[0] = 3` in a script) but does not resist a
determined script — that is not its role, the trust model already gives
full powers to the embedded interpreter.

### The stale view policy

A view taken **before** a write stays on the **old** buffer: the script
keeps reading stale values from it, silently. This is the **chosen**
behavior, not a defect:

```python
v = pyst.column(col)       # v looks straight at the model's memory
print(v[0])               # 1.0
pyst.set_column(col, other_values)
print(v[0])               # 1.0  ← still the old value
print(pyst.column(col)[0]) # 2.0  ← the new one
```

This is the only safe choice: mutating the buffer under numpy's feet would be a
crash, not a surprising value. The two rejected options, for the record:
*invalidating the view* (numpy cannot cleanly invalidate an `ndarray`; we would have to
return a wrapper object, thus losing the fact that it is a real
`ndarray`, which the whole scientific library expects); *live view that
updates itself* (impossible without a copy or a lock — precisely what
zero-copy forbids).

**Simple rule that avoids the whole problem: do not keep a view longer
than a computation; re-read the column after any write.**

This choice is deliberately documented rather than fixed — see the test
`stale_view_stays_on_the_old_buffer` in `tests/zerocopy.rs`, which engraves the
policy: old pointer unchanged, old value re-read without a crash,
new borrow on a different pointer returning the new value.

### Measurements

`measure_borrow_vs_copy(1_000_000)`, `--release` test binary, Fedora
x86_64, several consecutive runs (the machine is shared, hence the
variance):

| Measurement | Time |
|---|---|
| borrow of 1 M `f64` (two Python allocations, no byte copied) | **≈ 5–9 µs** |
| copy of 1 M `f64` (`PyArray1::from_slice`) | **≈ 1.8–2.2 ms** |
| observed ratio | **×240 to ×370** |

The test `borrow_at_least_twenty_times_faster_than_copy` demands a ratio
**≥ ×20**; the observed margin is an order of magnitude above. The borrow takes
**constant** time (it allocates only the `FloatBufferOwner` and the `ndarray`
object) whereas the copy is **linear** in the size of the column — the gap
can therefore only grow with columns larger than 1 M values.

These figures, measured inside the test binary through `cargo test`, are
higher on the copy side than the ≈ 0.8 ms measured in 5.1a by `dist/pyhello`
run outside any harness (§ Measurements above): the methodology differs
(`libtest` harness, a single measurement per run rather than an average of
three process launches). The ratio, not the absolute value, is what this
lock verifies.

### Instrumented check: run, excellent result

[`scripts/python-memcheck.sh`](../../scripts/python-memcheck.sh) runs the
test binary of `tests/zerocopy.rs` under `valgrind --error-exitcode=1`
(without `--leak-check`: the "still reachable" allocations of the CPython
allocator, never finalized, are not what we are looking for — we look for **invalid**
reads or writes), with `PYTHONMALLOC=malloc` set to avoid the
flood of false positives that CPython's small-object allocator produces
under memcheck.

`valgrind` is now installed on the development machine. Result:

```
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

==ERROR SUMMARY: 0 errors from 0 contexts (suppressed: 0 from 0)
```

**Zero errors, over the 19 tests.** The lifetime contract of the zero-copy
numpy view (§ above) is no longer verified only by Rust
assertions: `valgrind` confirms, through binary instrumentation, that no read or
write reaches an invalid memory area at the Rust/PyO3/numpy boundary.

#### A false negative fixed: the ×20 ratio is meaningless under instrumentation

The test `borrow_at_least_twenty_times_faster_than_copy` first
**failed** under valgrind:

```
borrow of 1000000 f64: 1170881 ns
copy   of 1000000 f64: 10496668 ns
ratio: ×9.0
panicked: the borrow must be at least 20× faster
```

Outside instrumentation, the borrow costs 5 to 9 µs against ≈ 2 ms for the copy
(ratio ×240 to ×370, § Measurements above). `valgrind` slows everything down, but
**not uniformly**: the borrow, dominated by a few calls (allocation of the
`FloatBufferOwner`, construction of the `ndarray`), is proportionally much
more penalized than the copy, dominated by a linear memory transfer — the
ratio measured under instrumentation therefore has no relation to the real
ratio.

The test is now **aware of its environment**:
`scripts/python-memcheck.sh` sets `STOKESFORGE_UNDER_VALGRIND=1` before running the
binary under valgrind; on seeing it, the test prints its two measurements and
**does not assert the ratio** (with a message explaining why), instead of
failing on a false negative. The `borrow_ns > 0` assertion (clock fine enough)
is checked in all cases, and the `× 20` ratio assertion stays
**active in ordinary execution** (`scripts/python-test.sh`, without the variable) —
it is what protects zero-copy against a silent regression into a
copy. Measurements printed under instrumentation, with `--nocapture` (without which
libtest swallows the output of a passing test):

```
borrow of 1000000 f64: 1085997 ns
copy   of 1000000 f64: 9853264 ns
ratio: ×9.1
STOKESFORGE_UNDER_VALGRIND set: ratio not asserted (valgrind slows down
the borrow and the copy non-uniformly, the ratio measured here
is meaningless)
```

`miri` is **not** an alternative to `valgrind` here: it does not execute
calls to foreign libraries (FFI), hence nothing that goes through PyO3 —
that is, this whole module. It remains the relevant tool for the rest of the
StokesForge model, which is pure Rust without FFI.

## Deliberately out of this task

* **Lifecycle and ownership of Python objects** (`Py<T>` kept between two
  GIL acquisitions, release) — task **5.3**.
* **Dedicated script thread** (user console, interruption, `detach` during
  long computations) — task **5.4**. Here everything runs on the calling thread.
* **Windows** — task **5.1b**: `wchar_t` is 16 bits there, there is no
  `$ORIGIN` (the loader looks next to the executable, which changes the
  layout), and `Py_DecodeLocale` does not behave the same way. The code of
  `start_interpreter` is written to be portable, but **nothing has been tested
  anywhere other than Linux x86_64**.
