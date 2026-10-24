# Rust explained to someone who knows Python

Use when a Rust term cannot be avoided in an answer. Each entry gives the closest Python analogy **and** where it is wrong — an uncorrected analogy always ends in a wrong expectation. Pick the word you need, in one sentence; never unroll the table. Say the answer in French.

## Code organisation

| Rust | In Python | Where the analogy breaks |
|---|---|---|
| *crate* | an installable package (`numpy`) | a crate is also the compilation unit: boundaries between crates are checked by the compiler, not by convention |
| *workspace* | a repo with several packages | they share one version file (`Cargo.lock`) and compile together |
| `Cargo.toml` | `pyproject.toml` | — |
| `cargo` | `pip` + `pytest` + `make` together | — |
| module, `mod` | a `.py` file | — |
| `pub` | no leading `_` | a real barrier: without `pub` it is inaccessible, not just discouraged |
| *feature* | an extra of `pip install pkg[extra]` | a disabled feature is **not compiled at all**: the code behind it is neither checked nor tested |

## Types

| Rust | In Python | Where the analogy breaks |
|---|---|---|
| `struct` | a `dataclass` | — |
| `enum` | an `Enum`… or rather a `Union` of dataclasses | each variant can carry different data; that is its main use here |
| `trait` | a `Protocol` / named duck typing | the compiler checks the implementation; nothing is resolved at run time |
| `Option<T>` | `Optional[T]` | the empty case cannot be forgotten: the compiler refuses |
| `Result<T, E>` | a value or an exception | the error is a **returned value**, not a jump; it must be handled explicitly |
| `match` | Python's `match`, stricter | every case must be covered, or it does not compile |
| generic (`fn f<T>`) | a function accepting any type | the code is copied and specialised per type at compile time — hence the care, in rendering, not to multiply those copies |

## Memory and sharing

| Rust | In Python | Where the analogy breaks |
|---|---|---|
| *ownership* | nothing equivalent | a value has one owner; when it goes, the value is freed. No garbage collector |
| `&T` (shared borrow) | passing an object to a function that reads it | as many as you want, but nothing can modify meanwhile |
| `&mut T` (exclusive borrow) | passing an object to a function that modifies it | **only one at a time**, checked at compile time. The key to StokesForge's architecture |
| `Arc<T>` | an ordinary (counted) Python reference | **read-only** sharing between threads; modifying needs a lock |
| `clone()` | `copy.copy()` | often cheap here: cloning an `Arc<Tree>` copies the counter, not the tree |
| *lifetime* (`'a`) | nothing equivalent | "this reference does not outlive what it comes from"; almost never worth saying aloud |

## Execution

| Rust | In Python | Where the analogy breaks |
|---|---|---|
| `panic!` | an uncaught exception | stricter use: a panic signals a program bug, not invalid data (handled as a value) |
| *thread* | `threading.Thread` | no GIL: two Rust threads really compute in parallel |
| GIL | CPython's global lock | it still exists **in StokesForge's embedded Python**; that is the whole point of invariant 9 |
| `unsafe` | `ctypes` | rare and confined to system edges (CPython start-up and zero-copy arrays, script interruption, Wayland clipboard, Windows console); none in the data model |

## Repo tools

| Command | Mental equivalent |
|---|---|
| `cargo build` | compile |
| `cargo test` | `pytest` |
| `cargo fmt` | `black` |
| `cargo clippy` | `ruff` / `flake8`, but blocking in CI |

Rather than the name (« borrow checker », « monomorphisation »…), say what it **produces**: « le compilateur interdit que deux endroits modifient la même chose en même temps ».
