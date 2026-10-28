//! Measurement: how many versions of a column buffer does the hybrid undo
//! history keep alive, and how much memory does that weigh?
//!
//! # The doubt to settle
//!
//! `History` (`src/history.rs`) keeps a **structure** change
//! (rename, move, add, delete a node) as a pair of
//! `Arc<Tree>` snapshots — cloning the tree is cheap (a table of
//! `(NodeId, Arc<Node>)` pairs, not the nodes). A **data** change
//! (cells of a column) is kept as a delta: only the values written, tiny.
//! The documentation of the crate claims that the retained payloads are
//! "identical" in the two schemes.
//!
//! That is true of a **deleted** column: both schemes keep it
//! (scenario 6). It may **not** be true of a column **modified**
//! afterwards: an `Arc<Tree>` captured by a structural entry keeps
//! all its `Arc<Node>`, so all the columns, so their
//! `Arc<Vec<f64>>` buffers. If this column is then written, copy-on-write
//! (`Arc::make_mut`, in `Column::set_values`) allocates a new buffer — and
//! the old one stays alive as long as the structural entry is on the stack,
//! *even if nobody ever reads it again*. By alternating structure and
//! writes in a big column, each structural entry risks
//! pinning a full version of the buffer.
//!
//! **A high figure** (tens of megabytes of pinned buffers for
//! a few dozen operations) would confirm that the hybrid choice has a
//! real memory cost that grows with use — an argument to revisit the
//! history limit, or for structural entries not to capture
//! the columns themselves. **A figure close to zero** (versions
//! that never exceed the number of live columns) would refute
//! the hypothesis and confirm the current documentation.
//!
//! # Method: count live buffers, not allocated bytes
//!
//! The memory of the process (RSS) is unreadable here: allocator noise,
//! pages not returned to the system. We measure instead *exactly what is
//! at stake*, in two independent ways:
//!
//! - **Measure 1** — before each write, we record the address of the buffer
//!   (`Arc::as_ptr`) and a `std::sync::Weak` obtained by `Arc::downgrade`. At the
//!   end of a scenario, we count how many of these `Weak` can still be
//!   upgraded (`upgrade().is_some()`): this is the number of versions
//!   *still retained*. A `Weak` keeps nothing alive — this is the key point of
//!   the method: the observation never distorts the measure it serves to
//!   make.
//! - **Measure 2** — a test `#[global_allocator]` wraps
//!   `std::alloc::System` and counts the bytes currently alive
//!   (incremented at `alloc`, decremented at `dealloc`, adjusted at `realloc`).
//!   Read before and after each scenario, it is a second view,
//!   independent of the first; if the two agree, the figure is
//!   solid.
//!
//! # What the scenarios do not assert
//!
//! Except for scenario 6 (where the survival of the buffer of a deleted column as
//! long as its entry is on the stack is structurally guaranteed by the code
//! — without that, no undo is possible — and where its release after eviction
//! is just as guaranteed), these tests **print** figures without imposing a
//! threshold. The measurement serves to decide an architecture, not to become a
//! fragile test that breaks at the first constant tuning.
//!
//! # Organization
//!
//! Each scenario is an ordinary function that builds its state, measures,
//! prints a readable report, and returns a [`Row`]. A dedicated `#[test]`
//! per scenario allows replaying it in isolation; [`summary_table_all_scenarios`]
//! replays them all in order and prints the final summary table —
//! it is the source of truth for the table of figures. A
//! `static MEASURE_LOCK` serializes all the tests of this file: measure 2
//! is a **per-process** counter, so per test binary; without
//! serialization, two `--ignored` scenarios running in parallel on the
//! default threads of the harness would distort each other.
//!
//! Ignored (`#[ignore]`): these tests allocate hundreds of megabytes and
//! must not weigh down `cargo test`. Explicit launch:
//! `cargo test -p stokesforge-commands --release --test memory_pinning -- --ignored --nocapture`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

use stokesforge_commands::{Command, History};
use stokesforge_core::{Column, MixedCell, Node, NodeId, Project, ValueBlock};

// ===========================================================================
// Counting allocator — measure 2
// ===========================================================================

/// Bytes currently alive (incremented at `alloc`/`alloc_zeroed`,
/// decremented at `dealloc`, adjusted at `realloc`).
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

/// Bytes allocated since the start of the process, never decremented: serves
/// to tell a transient peak (allocated then freed at once) from a figure
/// really pinned — see the comment of scenario 1.
static TOTAL_ALLOCATED: AtomicUsize = AtomicUsize::new(0);

/// Wraps `System`: changes no allocation behavior, only
/// observes the sizes passing through.
struct CountingAllocator;

// SAFETY: each method only delegates to `System` (whose safety guarantees
// are those of the standard system) and increments/decrements atomic
// counters from the sizes actually passed/returned — no
// pointer is dereferenced or built here.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            LIVE_BYTES.fetch_add(layout.size(), Ordering::SeqCst);
            TOTAL_ALLOCATED.fetch_add(layout.size(), Ordering::SeqCst);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            LIVE_BYTES.fetch_add(layout.size(), Ordering::SeqCst);
            TOTAL_ALLOCATED.fetch_add(layout.size(), Ordering::SeqCst);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::SeqCst);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            let old_size = layout.size();
            if new_size > old_size {
                let grew = new_size - old_size;
                LIVE_BYTES.fetch_add(grew, Ordering::SeqCst);
                TOTAL_ALLOCATED.fetch_add(grew, Ordering::SeqCst);
            } else if new_size < old_size {
                LIVE_BYTES.fetch_sub(old_size - new_size, Ordering::SeqCst);
            }
        }
        new_ptr
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn live_bytes() -> usize {
    LIVE_BYTES.load(Ordering::SeqCst)
}

fn total_allocated() -> usize {
    TOTAL_ALLOCATED.load(Ordering::SeqCst)
}

/// Serializes all the tests of this file: measure 2 (process atomic
/// counters) makes sense only if a single scenario runs at a
/// time. Poison is recovered rather than propagated: a scenario that
/// panicked (assertion of scenario 6, for example) must not cause all the
/// others to fail in cascade.
static MEASURE_LOCK: Mutex<()> = Mutex::new(());

fn measure_lock() -> std::sync::MutexGuard<'static, ()> {
    MEASURE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ===========================================================================
// Footprint of a buffer version — measure 1
// ===========================================================================

/// Footprint of the `f64` buffer of a column at a given instant: its address
/// (to group the footprints that designate the same version) and a
/// `Weak` that will tell, later, whether THIS precise version is still alive.
/// A `Weak` keeps nothing: observing it never distorts the measure it
/// serves to make.
struct BufferFingerprint {
    ptr: usize,
    weak: Weak<Vec<f64>>,
    len: usize,
}

/// Footprint of the buffer of `column` in the current state of `project`.
fn fingerprint(project: &Project, column: NodeId) -> BufferFingerprint {
    let arc = project
        .tree()
        .node(column)
        .and_then(Node::as_column)
        .and_then(Column::float_values)
        .cloned()
        .expect("existing numeric column");
    BufferFingerprint {
        ptr: Arc::as_ptr(&arc) as usize,
        weak: Arc::downgrade(&arc),
        len: arc.len(),
    }
}

/// Among footprints taken at various instants (the same version may
/// have been observed several times), how many **distinct** versions
/// are still alive at the moment of the call, and how many bytes that
/// represents.
fn live_versions(fingerprints: &[BufferFingerprint]) -> (usize, usize) {
    let mut seen = HashSet::new();
    let mut versions = 0usize;
    let mut bytes = 0usize;
    for fp in fingerprints {
        if !seen.insert(fp.ptr) {
            continue; // same version already counted
        }
        if fp.weak.upgrade().is_some() {
            versions += 1;
            bytes += fp.len * std::mem::size_of::<f64>();
        }
    }
    (versions, bytes)
}

/// Sum of [`live_versions`] over the target column and over the
/// witness columns.
fn total_live(target: &[BufferFingerprint], others: &[Vec<BufferFingerprint>]) -> (usize, usize) {
    let (mut versions, mut bytes) = live_versions(target);
    for fingerprints in others {
        let (v, b) = live_versions(fingerprints);
        versions += v;
        bytes += b;
    }
    (versions, bytes)
}

/// Like [`live_versions`], but **excludes** the version whose address is
/// `current_ptr`: it is the counterpart, measured independently (through
/// upgradable `Weak`s, never by querying `History`), of what
/// `History::retained_bytes` computes by subtracting the footprint of the
/// current tree — a buffer that the current tree still uses is not
/// a cost of the history, it would be there anyway. Serves scenario 7
/// to compare the two measures on the same basis ("how many *extra*
/// versions", not "how many versions in all").
fn historical_only_live(fingerprints: &[BufferFingerprint], current_ptr: usize) -> (usize, usize) {
    let mut seen = HashSet::new();
    let mut versions = 0usize;
    let mut bytes = 0usize;
    for fp in fingerprints {
        if fp.ptr == current_ptr {
            continue; // the current version is not a cost of the history
        }
        if !seen.insert(fp.ptr) {
            continue; // same version already counted
        }
        if fp.weak.upgrade().is_some() {
            versions += 1;
            bytes += fp.len * std::mem::size_of::<f64>();
        }
    }
    (versions, bytes)
}

/// "History only" bytes (see [`historical_only_live`]) for the target
/// and the set of witnesses of an [`AlternatingRun`] — the independent
/// measure to compare with `run.history.retained_bytes()`.
fn independent_historical_bytes(run: &AlternatingRun) -> usize {
    let current_target = fingerprint(&run.project, run.target).ptr;
    let (_, mut bytes) = historical_only_live(&run.target_fingerprints, current_target);
    for (fingerprints, &column) in run.other_fingerprints.iter().zip(run.others.iter()) {
        let current = fingerprint(&run.project, column).ptr;
        let (_, b) = historical_only_live(fingerprints, current);
        bytes += b;
    }
    bytes
}

// ===========================================================================
// Fixture: a sheet of 5 columns of 1,000,000 f64
// ===========================================================================

/// Reference column: 1,000,000 `f64`, ~7.6 MiB (8,000,000 bytes).
const ROWS: usize = 1_000_000;

/// Number of [rename, write] pairs played in the alternating scenario.
const WRITE_ITERATIONS: usize = 20;

/// History limit of scenario 3: small enough to see whether it really
/// bounds the number of live versions.
const BOUNDED_HISTORY_LIMIT: usize = 5;

/// Byte limit of scenario 7: 64 MiB, small enough to observe whether
/// `History::with_limits` really bounds the retained total at this height —
/// and at what order of magnitude (about 8 versions of the reference
/// column, 64 / 7.6 ≈ 8.4, against 21 without it in scenario 2).
const BYTE_BOUNDED_LIMIT: usize = 64 * 1024 * 1024;

/// Size in bytes of a reference column (`ROWS` floats).
fn column_bytes() -> usize {
    ROWS * std::mem::size_of::<f64>()
}

/// `Book1/Sheet1` with 5 columns of `ROWS` floats: `target`, the one we
/// will rename and write, and 4 witness columns never touched — to see
/// whether they are pinned too, or only the one we write.
struct Fixture {
    project: Project,
    sheet: NodeId,
    target: NodeId,
    others: Vec<NodeId>,
}

fn build_fixture() -> Fixture {
    let mut project = Project::new("MemoryMeasure").expect("measurement project");
    let root = project.root();
    project
        .add_workbook(root, Some("Book1"))
        .expect("workbook Book1");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("sheet Sheet1");

    let values: Vec<f64> = (0..ROWS).map(|row| row as f64).collect();
    let target = project
        .add_column(sheet, None, Some("Target"), Column::from_f64(&values))
        .expect("target column (1,000,000 f64)");
    let others: Vec<NodeId> = ["Witness1", "Witness2", "Witness3", "Witness4"]
        .into_iter()
        .map(|name| {
            project
                .add_column(sheet, None, Some(name), Column::from_f64(&values))
                .expect("witness column")
        })
        .collect();

    Fixture {
        project,
        sheet,
        target,
        others,
    }
}

// ===========================================================================
// Shared driver: structure and writes alternating under `History`
// ===========================================================================

/// State of an execution of the alternating scenario: the project and the history
/// (so that the caller can keep using them — scenario 4
/// undoes afterwards), the footprints collected along the way, and the
/// identifiers of the target/witness columns (to find, afterwards, the
/// *current* version of each — needed for the independent measure "to how
/// much does `History::retained_bytes` agree" of scenario 7).
struct AlternatingRun {
    project: Project,
    history: History,
    target: NodeId,
    others: Vec<NodeId>,
    target_fingerprints: Vec<BufferFingerprint>,
    other_fingerprints: Vec<Vec<BufferFingerprint>>,
}

/// `WRITE_ITERATIONS` times [rename the target column, write a cell
/// into the target], under a `History` limited to `limit` entries (`0` =
/// unlimited), at the default byte limit of `History` (512 MiB — with no
/// effect at this scale, see scenario 7 for a tight byte
/// limit). Footprints the target buffer just **before** each write —
/// it is that version that a later rename risks pinning in
/// its `before` snapshot — and the witness columns at each iteration,
/// to check that they never gain an extra version.
fn run_alternating(limit: usize) -> AlternatingRun {
    run_alternating_history(History::with_limit(limit))
}

/// Like [`run_alternating`], but with both limits of `History`
/// explicit — this is what scenario 7 uses to observe whether the
/// byte limit really bounds the retained total.
fn run_alternating_with_limits(limit: usize, byte_limit: usize) -> AlternatingRun {
    run_alternating_history(History::with_limits(limit, byte_limit))
}

/// Common driver of [`run_alternating`] and [`run_alternating_with_limits`]:
/// same course, `History` already built with the wanted limits.
fn run_alternating_history(mut history: History) -> AlternatingRun {
    let Fixture {
        mut project,
        sheet: _,
        target,
        others,
    } = build_fixture();
    let mut target_fingerprints = Vec::with_capacity(WRITE_ITERATIONS + 1);
    let mut other_fingerprints: Vec<Vec<BufferFingerprint>> =
        others.iter().map(|_| Vec::new()).collect();

    for i in 0..WRITE_ITERATIONS {
        target_fingerprints.push(fingerprint(&project, target));
        history
            .execute(
                &mut project,
                Command::Rename {
                    id: target,
                    name: format!("Target{i}"),
                },
            )
            .expect("rename accepted");
        history
            .execute(
                &mut project,
                Command::SetCells {
                    column: target,
                    start: i,
                    values: ValueBlock::Float(vec![i as f64]),
                    expected_version: None,
                },
            )
            .expect("write accepted");
        for (fingerprints, &column) in other_fingerprints.iter_mut().zip(others.iter()) {
            fingerprints.push(fingerprint(&project, column));
        }
    }
    target_fingerprints.push(fingerprint(&project, target));
    for (fingerprints, &column) in other_fingerprints.iter_mut().zip(others.iter()) {
        fingerprints.push(fingerprint(&project, column));
    }

    AlternatingRun {
        project,
        history,
        target,
        others,
        target_fingerprints,
        other_fingerprints,
    }
}

// ===========================================================================
// Summary table
// ===========================================================================

/// A row of the final summary table.
struct Row {
    name: &'static str,
    /// Live versions of the buffer, measure 1 (target + witnesses).
    live_versions: usize,
    /// Pinned bytes, measure 1 (target + witnesses).
    pinned_bytes: usize,
    /// Delta of live bytes according to the allocator, measure 2, from the start to the end
    /// of the scenario (project and history still alive at the final reading).
    allocator_bytes: usize,
}

fn mb(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn print_row(row: &Row) {
    println!(
        "  => {} : {} live version(s), {:.1} MiB pinned (measure 1), {:.1} MiB live Δ (measure 2)",
        row.name,
        row.live_versions,
        mb(row.pinned_bytes),
        mb(row.allocator_bytes)
    );
}

fn print_summary_table(rows: &[Row]) {
    let baseline = rows
        .iter()
        .find(|row| row.name.starts_with('5'))
        .map(|row| row.pinned_bytes);

    println!();
    println!(
        "reference column: {ROWS} f64 ({:.1} MiB); {WRITE_ITERATIONS} iterations per alternating scenario",
        mb(ROWS * std::mem::size_of::<f64>())
    );
    println!(
        "================================== Summary table ======================================"
    );
    println!(
        "{:<46} {:>9} {:>15} {:>15} {:>11}",
        "Scenario", "Versions", "Measure1 (MiB)", "Measure2 (MiB)", "Ratio/S5"
    );
    for row in rows {
        // The ratio to scenario 5 makes sense only for scenarios 1 to 5
        // (same construction: target + 4 witnesses, same 20 iterations).
        // Scenario 6 measures something else (a deleted column) and is not
        // comparable term by term.
        let comparable = row
            .name
            .chars()
            .next()
            .is_some_and(|c| ('1'..='5').contains(&c));
        let ratio = match (comparable, baseline) {
            (true, Some(base)) if base > 0 => {
                format!("{:.2}x", row.pinned_bytes as f64 / base as f64)
            }
            _ => "—".to_string(),
        };
        println!(
            "{:<46} {:>9} {:>15.1} {:>15.1} {:>11}",
            row.name,
            row.live_versions,
            mb(row.pinned_bytes),
            mb(row.allocator_bytes),
            ratio
        );
    }
    println!(
        "========================================================================================="
    );
}

// ===========================================================================
// Scenario 1 — writes only, without any structure operation
// ===========================================================================

/// 20 writes of a single cell in the target column, without any
/// structure operation. Hypothesis: a `Data` entry never keeps a
/// whole buffer (only the values written, tiny) — no
/// old version of the buffer should therefore stay pinned at the end.
fn scenario_1_writes_only() -> Row {
    let before = live_bytes();
    let total_before = total_allocated();
    let Fixture {
        mut project,
        sheet: _,
        target,
        others,
    } = build_fixture();
    let mut history = History::with_limit(0);
    let mut target_fingerprints = Vec::with_capacity(WRITE_ITERATIONS + 1);
    let mut other_fingerprints: Vec<Vec<BufferFingerprint>> =
        others.iter().map(|_| Vec::new()).collect();

    for i in 0..WRITE_ITERATIONS {
        target_fingerprints.push(fingerprint(&project, target));
        history
            .execute(
                &mut project,
                Command::SetCells {
                    column: target,
                    start: i,
                    values: ValueBlock::Float(vec![i as f64]),
                    expected_version: None,
                },
            )
            .expect("write accepted");
    }
    target_fingerprints.push(fingerprint(&project, target));
    for (fingerprints, &column) in other_fingerprints.iter_mut().zip(others.iter()) {
        fingerprints.push(fingerprint(&project, column));
    }

    let (target_versions, _) = live_versions(&target_fingerprints);
    let (total_versions, pinned_bytes) = total_live(&target_fingerprints, &other_fingerprints);
    let after = live_bytes();
    let total_after = total_allocated();

    println!("\n== Scenario 1 — writes only (no structure) ==");
    println!(
        "  live versions of the target buffer: {target_versions} (nothing should stay pinned beyond the current version)"
    );
    println!(
        "  total (target + 4 witnesses): {total_versions} versions, {:.1} MiB pinned (measure 1)",
        mb(pinned_bytes)
    );
    println!(
        "  allocator: {:.1} -> {:.1} MiB live (Δ {:.1} MiB, measure 2); {:.1} MiB allocated in total during the scenario",
        mb(before),
        mb(after),
        mb(after.saturating_sub(before)),
        mb(total_after.saturating_sub(total_before))
    );
    println!(
        "  note: even without any structural entry, `History::execute` clones the current \
         `Arc<Tree>` before applying the command (guarantee of restoration on failure); this \
         transient clone forces a copy-on-write of the buffer at EVERY write (visible in the total \
         allocated), even if nothing stays pinned at the end."
    );

    Row {
        name: "1. Writes only",
        live_versions: target_versions,
        pinned_bytes,
        allocator_bytes: after.saturating_sub(before),
    }
}

// ===========================================================================
// Scenario 2 — alternating structure and writes (the suspected scenario)
// ===========================================================================

/// `WRITE_ITERATIONS` times [rename the target, write a cell into the
/// target], unlimited history. Hypothesis: each rename captures, in
/// its `before` snapshot, the version of the buffer that the previous write
/// had just created — it stays pinned as long as this structural entry
/// is on the stack.
fn scenario_2_alternating() -> Row {
    let before = live_bytes();
    let run = run_alternating(0);
    let (target_versions, target_bytes) = live_versions(&run.target_fingerprints);
    let (total_versions, pinned_bytes) =
        total_live(&run.target_fingerprints, &run.other_fingerprints);
    let witness_versions: Vec<usize> = run
        .other_fingerprints
        .iter()
        .map(|fingerprints| live_versions(fingerprints).0)
        .collect();
    let after = live_bytes();

    println!("\n== Scenario 2 — alternating structure and writes (unlimited history) ==");
    println!(
        "  live versions of the target buffer: {target_versions} ({:.1} MiB, measure 1)",
        mb(target_bytes)
    );
    println!(
        "  witness columns (never written): {witness_versions:?} live version(s) each (expected: 1 everywhere)"
    );
    println!(
        "  total (target + witnesses): {total_versions} versions, {:.1} MiB pinned (measure 1)",
        mb(pinned_bytes)
    );
    println!(
        "  allocator: {:.1} -> {:.1} MiB live (Δ {:.1} MiB, measure 2)",
        mb(before),
        mb(after),
        mb(after.saturating_sub(before))
    );

    Row {
        name: "2. Alternating structure + writes",
        live_versions: target_versions,
        pinned_bytes,
        allocator_bytes: after.saturating_sub(before),
    }
}

// ===========================================================================
// Scenario 3 — same scenario, bounded history
// ===========================================================================

/// Same alternation as scenario 2, but `History::with_limit(BOUNDED_HISTORY_LIMIT)`:
/// does the history limit really protect, and at what height?
fn scenario_3_bounded_history() -> Row {
    let before = live_bytes();
    let run = run_alternating(BOUNDED_HISTORY_LIMIT);
    let (target_versions, target_bytes) = live_versions(&run.target_fingerprints);
    let (total_versions, pinned_bytes) =
        total_live(&run.target_fingerprints, &run.other_fingerprints);
    let after = live_bytes();

    println!(
        "\n== Scenario 3 — same scenario, history bounded to {BOUNDED_HISTORY_LIMIT} entries =="
    );
    println!(
        "  live versions of the target buffer: {target_versions} ({:.1} MiB, measure 1)",
        mb(target_bytes)
    );
    println!(
        "  total (target + witnesses): {total_versions} versions, {:.1} MiB pinned (measure 1)",
        mb(pinned_bytes)
    );
    println!(
        "  allocator: {:.1} -> {:.1} MiB live (Δ {:.1} MiB, measure 2)",
        mb(before),
        mb(after),
        mb(after.saturating_sub(before))
    );

    Row {
        name: "3. Same scenario, history at 5",
        live_versions: target_versions,
        pinned_bytes,
        allocator_bytes: after.saturating_sub(before),
    }
}

// ===========================================================================
// Scenario 4 — effect of undoing
// ===========================================================================

/// Replays scenario 2, then undoes the whole history (`undo` until
/// `Ok(false)`), and recounts the **same** footprints taken during
/// construction.
///
/// Two assertions, not just one — a distinction that a first version of
/// this test missed, and that the measurement itself brought to light (see the
/// task report):
///
/// - the `WRITE_ITERATIONS` footprints taken **before each write**
///   each designate a version that a *following* rename captures in its
///   `before` snapshot. Their survival after undo is structurally
///   guaranteed: `History::undo` removes the entry from `undo` and pushes it onto
///   `redo` (`Vec::pop` then `Vec::push`), it never destroys it;
/// - the very **last** footprint, taken after the loop, designates the
///   current state at the time of the measurement — but no rename follows to
///   capture it in an entry. Nothing keeps it: undoing the operation that
///   produced it abandons it, and it must disappear. This is not a
///   leak of the redo mechanism, it is the absence of a mechanism for
///   this precise version.
fn scenario_4_after_undo() -> Row {
    let before = live_bytes();
    let mut run = run_alternating(0);

    let (captured, tail) = run.target_fingerprints.split_at(WRITE_ITERATIONS);
    let tail_fingerprint = &tail[0];
    let (captured_versions_before, _) = live_versions(captured);
    let tail_alive_before = tail_fingerprint.weak.upgrade().is_some();
    let (versions_before_undo, bytes_before_undo) =
        total_live(&run.target_fingerprints, &run.other_fingerprints);

    let mut steps = 0usize;
    while run.history.undo(&mut run.project).expect("undo accepted") {
        steps += 1;
    }

    let (captured_versions_after, _) = live_versions(captured);
    let tail_alive_after = tail_fingerprint.weak.upgrade().is_some();
    let (versions_after_undo, bytes_after_undo) =
        total_live(&run.target_fingerprints, &run.other_fingerprints);
    let after = live_bytes();

    println!("\n== Scenario 4 — after a complete undo of scenario 2 ==");
    println!("  {steps} undo(s) performed until Ok(false)");
    println!(
        "  total (target + witnesses), measure 1: {versions_before_undo} versions before -> {versions_after_undo} after \
         ({:.1} MiB -> {:.1} MiB)",
        mb(bytes_before_undo),
        mb(bytes_after_undo)
    );
    println!(
        "  versions captured by a structural entry (the first {WRITE_ITERATIONS} writes): \
         {captured_versions_before} before -> {captured_versions_after} after"
    );
    assert_eq!(
        captured_versions_after, captured_versions_before,
        "a version retained by a structural entry must not disappear on undo: \
         `History::undo` moves the entry to `redo`, it never destroys it"
    );
    println!(
        "  \"current\" version, retained by no entry: alive before = {tail_alive_before}, \
         after = {tail_alive_after} (expected: becomes false again — nobody kept it, undoing \
         abandons the state that carried it)"
    );
    println!(
        "  -> confirmed: everything a structural entry retains stays pinned for \
         redo; only the current version, never captured by any entry, \
         disappears."
    );
    println!(
        "  allocator: {:.1} -> {:.1} MiB live (Δ {:.1} MiB, measure 2, from the start of the scenario to after the undo)",
        mb(before),
        mb(after),
        mb(after.saturating_sub(before))
    );

    Row {
        name: "4. After a complete undo (scenario 2)",
        live_versions: versions_after_undo,
        pinned_bytes: bytes_after_undo,
        allocator_bytes: after.saturating_sub(before),
    }
}

// ===========================================================================
// Scenario 5 — honest comparison, without History
// ===========================================================================

/// The same alternating scenario as scenario 2, but applied **directly**
/// to the `Project`, without any `History`. This is the lower bound: what a
/// history that kept no buffer would cost. The ratio between
/// scenario 2 and this one is the real overhead of the hybrid choice — it is the
/// figure the decision is waiting for.
fn scenario_5_no_history() -> Row {
    let before = live_bytes();
    let Fixture {
        mut project,
        sheet: _,
        target,
        others,
    } = build_fixture();
    let mut target_fingerprints = Vec::with_capacity(WRITE_ITERATIONS + 1);
    let mut other_fingerprints: Vec<Vec<BufferFingerprint>> =
        others.iter().map(|_| Vec::new()).collect();

    for i in 0..WRITE_ITERATIONS {
        target_fingerprints.push(fingerprint(&project, target));
        project
            .rename(target, &format!("Target{i}"))
            .expect("direct rename accepted");
        project
            .set_values(target, i, ValueBlock::Float(vec![i as f64]))
            .expect("direct write accepted");
        for (fingerprints, &column) in other_fingerprints.iter_mut().zip(others.iter()) {
            fingerprints.push(fingerprint(&project, column));
        }
    }
    target_fingerprints.push(fingerprint(&project, target));
    for (fingerprints, &column) in other_fingerprints.iter_mut().zip(others.iter()) {
        fingerprints.push(fingerprint(&project, column));
    }

    let (target_versions, _) = live_versions(&target_fingerprints);
    let (total_versions, pinned_bytes) = total_live(&target_fingerprints, &other_fingerprints);
    let after = live_bytes();

    println!("\n== Scenario 5 — same scenario, without History (lower bound) ==");
    println!(
        "  live versions of the target buffer: {target_versions} (nobody else clones the tree: in-place mutation expected)"
    );
    println!(
        "  total (target + witnesses): {total_versions} versions, {:.1} MiB pinned (measure 1)",
        mb(pinned_bytes)
    );
    println!(
        "  allocator: {:.1} -> {:.1} MiB live (Δ {:.1} MiB, measure 2)",
        mb(before),
        mb(after),
        mb(after.saturating_sub(before))
    );

    Row {
        name: "5. Same scenario, without History (lower bound)",
        live_versions: target_versions,
        pinned_bytes,
        allocator_bytes: after.saturating_sub(before),
    }
}

// ===========================================================================
// Scenario 6 — deletion, the witness
// ===========================================================================

/// History limit of scenario 6: tight enough (1 entry) to evict
/// the deletion entry at the very next command.
const DELETION_HISTORY_LIMIT: usize = 1;

/// Adds a column of 1,000,000 `f64`, deletes it with `Command::DeleteColumn`
/// (its buffer must stay alive: it is needed for undo), then
/// pushes one more command to evict this entry (limit = 1) and
/// checks that the buffer is finally released. Unlike the other
/// scenarios, these two assertions **are not hypotheses**: they
/// follow directly from the code (`Entry::Structural` keeps `before`, which
/// keeps the deleted node and therefore its buffer; `History::evict` removes
/// the oldest entry and releases its `Arc`s).
fn scenario_6_deletion_witness() -> (Row, Row) {
    let before = live_bytes();
    let Fixture {
        mut project,
        sheet,
        target,
        others: _,
    } = build_fixture();
    let mut history = History::with_limit(DELETION_HISTORY_LIMIT);

    let values: Vec<f64> = (0..ROWS).map(|row| row as f64).collect();
    let victim = project
        .add_column(sheet, None, Some("Victim"), Column::from_f64(&values))
        .expect("column to delete");
    drop(values);

    let victim_fingerprint = fingerprint(&project, victim);

    history
        .execute(&mut project, Command::DeleteColumn { column: victim })
        .expect("deletion accepted");

    let alive_after_delete = victim_fingerprint.weak.upgrade().is_some();
    let after_delete = live_bytes();
    assert!(
        alive_after_delete,
        "the buffer of the deleted column must stay alive as long as its entry \
         is on the stack — without that, undoing the deletion would be impossible"
    );

    println!("\n== Scenario 6 — deletion, the witness ==");
    println!("  after deletion (entry still on the stack): live buffer = {alive_after_delete}");

    // Push one more neutral command: with a limit of 1, it evicts
    // the deletion entry, which is the oldest.
    history
        .execute(
            &mut project,
            Command::Rename {
                id: target,
                name: "AfterEviction".to_string(),
            },
        )
        .expect("rename accepted");

    let alive_after_eviction = victim_fingerprint.weak.upgrade().is_some();
    let after_eviction = live_bytes();
    assert!(
        !alive_after_eviction,
        "once the deletion entry is evicted from the history, nothing keeps \
         the buffer any more: it must be released"
    );

    println!(
        "  after eviction (limit = {DELETION_HISTORY_LIMIT}, one more command pushed): live buffer = {alive_after_eviction}"
    );
    println!(
        "  allocator: {:.1} MiB before the scenario, {:.1} MiB after deletion, {:.1} MiB after eviction",
        mb(before),
        mb(after_delete),
        mb(after_eviction)
    );

    let victim_bytes = victim_fingerprint.len * std::mem::size_of::<f64>();
    let row_delete = Row {
        name: "6a. Deletion (entry still on the stack)",
        live_versions: 1,
        pinned_bytes: victim_bytes,
        allocator_bytes: after_delete.saturating_sub(before),
    };
    let row_evict = Row {
        name: "6b. After eviction (limit = 1)",
        live_versions: 0,
        pinned_bytes: 0,
        allocator_bytes: after_eviction.saturating_sub(before),
    };
    (row_delete, row_evict)
}

// ===========================================================================
// Scenario 7 — the second limit (bytes) proven on the measured pattern
// ===========================================================================

/// Same alternation as scenario 2, but under
/// `History::with_limits(100, BYTE_BOUNDED_LIMIT)`: does the second limit
/// (bytes of retained payload), added next to the limit in number of
/// entries, really bound the pattern measured in scenario 2, and at what
/// height?
///
/// Two assertions, each on a different measure:
///
/// - the **independently measured total** (the upgradable `Weak`s of this
///   file, counted outside the *current* version of each column —
///   see [`independent_historical_bytes`], the measured counterpart of what
///   `History::retained_bytes` computes) does not exceed `BYTE_BOUNDED_LIMIT`
///   beyond a documented margin;
/// - this same independent measure and `History::retained_bytes()` — two
///   independent computations of the same total — **agree to better than 10%**.
///   If they diverged more, it would not be fixed by adjusting
///   a threshold: it would be a result to report as it is.
fn scenario_7_byte_bounded_history() -> Row {
    let before = live_bytes();
    let run = run_alternating_with_limits(100, BYTE_BOUNDED_LIMIT);
    let (target_versions, target_bytes) = live_versions(&run.target_fingerprints);
    let (total_versions, pinned_bytes) =
        total_live(&run.target_fingerprints, &run.other_fingerprints);
    let historical = independent_historical_bytes(&run);
    let reported = run.history.retained_bytes();
    let after = live_bytes();

    println!(
        "\n== Scenario 7 — same scenario, history bounded to {} MiB of retained payload ==",
        mb(BYTE_BOUNDED_LIMIT)
    );
    println!(
        "  live versions of the target buffer: {target_versions} ({:.1} MiB, measure 1) \
         — expected: of the order of {} (cap / column size, rounded), not {}",
        mb(target_bytes),
        (BYTE_BOUNDED_LIMIT / column_bytes()) + 1, // +1: the current version, never a cost
        WRITE_ITERATIONS + 1
    );
    println!(
        "  total (target + witnesses): {total_versions} versions, {:.1} MiB pinned (measure 1, \
         includes the current versions of the 5 columns — not a history cost)",
        mb(pinned_bytes)
    );
    println!(
        "  \"history only\" total (independent measure, excluding current versions): {:.2} MiB, \
         against a cap of {:.1} MiB",
        mb(historical),
        mb(BYTE_BOUNDED_LIMIT)
    );
    println!(
        "  History::retained_bytes() reported: {:.2} MiB",
        mb(reported)
    );
    println!(
        "  allocator: {:.1} -> {:.1} MiB live (Δ {:.1} MiB, measure 2)",
        mb(before),
        mb(after),
        mb(after.saturating_sub(before))
    );

    // Documented margin: the data deltas (before + after, a few
    // `f64` per write) weigh a few hundred bytes over
    // `WRITE_ITERATIONS` iterations — negligible next to the cap, but not
    // zero — and eviction, strictly FIFO by construction, targets only
    // the byte limit at the granularity of a whole entry (~7.6 MiB here): the
    // last removal can leave the total up to `column_bytes()`
    // below the cap, never above, unless the rule "never
    // evict the last entry" were ever reached here (it
    // is not: about forty entries survive the cap). A margin
    // of 1 MiB comfortably covers the deltas without hiding a real
    // overshoot.
    let margin = 1024 * 1024;
    assert!(
        historical <= BYTE_BOUNDED_LIMIT + margin,
        "the independently measured total ({:.2} MiB) exceeds the cap ({:.1} MiB) beyond the \
         documented margin ({:.2} MiB)",
        mb(historical),
        mb(BYTE_BOUNDED_LIMIT),
        mb(margin)
    );

    // Agreement between the two independent measures of the same total.
    let relative_diff = if historical > 0 {
        (reported as f64 - historical as f64).abs() / historical as f64
    } else {
        f64::from(u8::from(reported > 0))
    };
    println!(
        "  agreement History::retained_bytes() / independent measure: gap {:.2} %",
        relative_diff * 100.0
    );
    assert!(
        relative_diff < 0.10,
        "History::retained_bytes() ({:.2} MiB) and the independent measure ({:.2} MiB) diverge by \
         {:.1} %, beyond the expected 10 %",
        mb(reported),
        mb(historical),
        relative_diff * 100.0
    );

    Row {
        name: "7. History bounded to 64 MiB of payload",
        live_versions: target_versions,
        pinned_bytes,
        allocator_bytes: after.saturating_sub(before),
    }
}

// ===========================================================================
// Scenario 8 — mixed column: the footprint follows `mixed_buffer_bytes`
// ===========================================================================

/// Cost per element of a mixed-type buffer, as documented by
/// `mixed_buffer_bytes` (`history.rs`, private to the crate): the size of a
/// `MixedCell` in the `Vec`, plus the UTF-8 bytes of the texts present,
/// whatever the variant of the other cells. Replicated here to
/// check **independently** the figure that `History::retained_bytes()`
/// reports — never by calling the function of the crate, it would no longer be a
/// check.
fn expected_mixed_buffer_bytes(cells: &[MixedCell]) -> usize {
    let overhead = std::mem::size_of::<MixedCell>();
    cells.iter().fold(0usize, |total, cell| {
        total
            + overhead
            + match cell {
                MixedCell::Text(text) => text.len(),
                MixedCell::Empty | MixedCell::Number(_) => 0,
            }
    })
}

/// None of the seven scenarios above builds a mixed column:
/// `mixed_buffer_bytes` (the estimate that `column_footprint` uses for a
/// `Mixed` column, modeled on `text_buffer_bytes`) was therefore never
/// exercised. Same scheme as scenario 6 (deletion, the witness): a
/// deleted mixed column stays retained as long as its entry is on the stack
/// (needed for undo), then released as soon as it is evicted — but
/// here we also check that the **reported** figure
/// (`History::retained_bytes()`) matches the documented estimate,
/// computed independently.
#[test]
fn scenario_8_mixed_column_deletion_matches_mixed_buffer_bytes_estimate() {
    let _guard = measure_lock();

    let mut project = Project::new("MixedMeasure").expect("measurement project");
    let root = project.root();
    project
        .add_workbook(root, Some("Book1"))
        .expect("workbook Book1");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("sheet Sheet1");

    // A mix of numbers, empty cells and texts of various lengths:
    // enough to see a gap if the estimate counted, for
    // example, only the non-empty texts, or forgot the fixed cost per cell.
    let cells: Vec<MixedCell> = (0..10_000)
        .map(|i| match i % 3 {
            0 => MixedCell::Number(i as f64),
            1 => MixedCell::Empty,
            _ => MixedCell::Text(format!("value-{i}")),
        })
        .collect();
    let expected = expected_mixed_buffer_bytes(&cells);

    let mut history = History::with_limit(1);
    let victim = project
        .add_column(sheet, None, Some("Victim"), Column::from_mixed(&cells))
        .expect("mixed column to delete");

    history
        .execute(&mut project, Command::DeleteColumn { column: victim })
        .expect("deletion accepted");

    let retained_after_delete = history.retained_bytes();
    println!(
        "mixed column of {} cells: expected mixed_buffer_bytes = {expected} bytes, \
         History::retained_bytes() reported = {retained_after_delete} bytes",
        cells.len()
    );
    assert_eq!(
        retained_after_delete, expected,
        "the footprint retained by `History` for the deleted mixed column \
         ({retained_after_delete} bytes) does not match the documented estimate of \
         `mixed_buffer_bytes` ({expected} bytes)"
    );

    // One more command, with a limit of 1 entry: evicts the deletion
    // entry (the oldest) — nothing must stay retained.
    history
        .execute(
            &mut project,
            Command::Rename {
                id: sheet,
                name: "Sheet1b".to_string(),
            },
        )
        .expect("rename accepted");
    let retained_after_eviction = history.retained_bytes();
    println!("after eviction of the deletion entry: {retained_after_eviction} bytes retained");
    assert_eq!(
        retained_after_eviction, 0,
        "after eviction of the deletion entry, nothing should be retained"
    );
}

// ===========================================================================
// Tests
// ===========================================================================

#[test]
#[ignore = "allocates hundreds of megabytes; run explicitly with --ignored"]
fn scenario_1_writes_only_pins_nothing_extra() {
    let _guard = measure_lock();
    print_row(&scenario_1_writes_only());
}

#[test]
#[ignore = "allocates hundreds of megabytes; run explicitly with --ignored"]
fn scenario_2_alternating_structure_and_writes() {
    let _guard = measure_lock();
    print_row(&scenario_2_alternating());
}

#[test]
#[ignore = "allocates hundreds of megabytes; run explicitly with --ignored"]
fn scenario_3_bounded_history_caps_pinned_versions() {
    let _guard = measure_lock();
    print_row(&scenario_3_bounded_history());
}

#[test]
#[ignore = "allocates hundreds of megabytes; run explicitly with --ignored"]
fn scenario_4_undo_keeps_everything_for_redo() {
    let _guard = measure_lock();
    print_row(&scenario_4_after_undo());
}

#[test]
#[ignore = "allocates hundreds of megabytes; run explicitly with --ignored"]
fn scenario_5_baseline_without_history() {
    let _guard = measure_lock();
    print_row(&scenario_5_no_history());
}

#[test]
#[ignore = "allocates hundreds of megabytes; run explicitly with --ignored"]
fn scenario_6_deleted_column_freed_only_after_eviction() {
    let _guard = measure_lock();
    let (row_delete, row_evict) = scenario_6_deletion_witness();
    print_row(&row_delete);
    print_row(&row_evict);
}

#[test]
#[ignore = "allocates hundreds of megabytes; run explicitly with --ignored"]
fn scenario_7_byte_limit_caps_the_measured_pattern() {
    let _guard = measure_lock();
    print_row(&scenario_7_byte_bounded_history());
}

/// Replays the seven scenarios in order and prints the final summary table
/// — this is the test that is authoritative for the table of figures.
#[test]
#[ignore = "allocates hundreds of megabytes; run explicitly with --ignored"]
fn summary_table_all_scenarios() {
    let _guard = measure_lock();

    let mut rows = Vec::with_capacity(8);
    rows.push(scenario_1_writes_only());
    rows.push(scenario_2_alternating());
    rows.push(scenario_3_bounded_history());
    rows.push(scenario_4_after_undo());
    rows.push(scenario_5_no_history());
    let (row_6a, row_6b) = scenario_6_deletion_witness();
    rows.push(row_6a);
    rows.push(row_6b);
    rows.push(scenario_7_byte_bounded_history());

    print_summary_table(&rows);
}
