//! Battery 17 — what common operations cost as the project grows.
//!
//! # Why count allocated bytes rather than milliseconds
//!
//! A time budget depends on the machine, the load and the build profile: put in
//! CI, it becomes either too loose to say anything, or unstable.
//! **Allocations**, on the other hand, are deterministic: a counting global
//! allocator (installed for this test binary only, and per thread so that the
//! tests stay parallelizable) measures exactly the same number everywhere.
//!
//! The shape of the assertion follows: never "fewer than X bytes", always "this
//! cost must not grow with the size of the project". That is the property that
//! matters, and it is the one that is false.
//!
//! The corresponding time measurements are in the report; they are not here, on
//! purpose.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use stokesforge_core::resolve::resolve_series;
use stokesforge_core::{Column, ColumnType, MixedCell, Node, NodeId, Project};
use stokesforge_plot::{PlotType, SeriesSpec};

// -- counting allocator ---------------------------------------------------

thread_local! {
    /// Bytes requested by the current thread since it started. `Cell<usize>`
    /// without a destructor and initialized as a `const`: the variable cannot
    /// itself allocate, which rules out any recursion in the allocator.
    static BYTES: Cell<usize> = const { Cell::new(0) };
}

struct Counter;

// SAFETY: each method delegates to `System` without changing either the
// pointer or the `Layout`; the counting is only a side effect on a
// thread-local cell, which never allocates.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // `try_with`: during the destruction of thread variables, the variable
        // is no longer accessible — nothing is counted then.
        let _ = BYTES.try_with(|bytes| bytes.set(bytes.get() + layout.size()));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let _ = BYTES.try_with(|bytes| bytes.set(bytes.get() + size));
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counter = Counter;

/// Bytes allocated by `work`, on the current thread.
fn bytes_of<T>(work: impl FnOnce() -> T) -> (T, usize) {
    let before = BYTES.with(Cell::get);
    let result = work();
    (result, BYTES.with(Cell::get) - before)
}

// -- benches --------------------------------------------------------------

/// A project with one populated workbook, plus `workbooks` empty workbooks —
/// enough to vary the **total size** of the project without touching the
/// resolved path.
fn project_with(workbooks: usize) -> Project {
    let mut project = Project::new("D").expect("valid project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("worksheet");
    for i in 0..5 {
        project
            .add_column(
                sheet,
                None,
                Some(&format!("c{i}")),
                Column::new(ColumnType::Numeric),
            )
            .expect("column");
    }
    for i in 0..workbooks {
        project
            .add_empty_workbook(root, Some(&format!("B{i}")))
            .expect("empty workbook");
    }
    project
}

/// Adds `n` columns to a fresh worksheet, with or without an imposed name.
fn add_columns(n: usize, imposed_name: bool) -> usize {
    let ((), bytes) = bytes_of(|| {
        let mut project = Project::new("D").expect("valid project");
        let root = project.root();
        project.add_workbook(root, Some("Book1")).expect("workbook");
        let sheet: NodeId = project.resolve_path("[Book1]Sheet1").expect("worksheet");
        for i in 0..n {
            let name = imposed_name.then(|| format!("c{i}"));
            project
                .add_column(
                    sheet,
                    None,
                    name.as_deref(),
                    Column::new(ColumnType::Numeric),
                )
                .expect("column");
        }
    });
    bytes
}

// -- guards ---------------------------------------------------------------

#[test]
fn b17_the_counting_allocator_does_measure_something() {
    // Without this test, a regression that broke the counting would make all
    // the other assertions of this battery true for the wrong reason.
    let (_, bytes) = bytes_of(|| vec![0u8; 4096]);
    assert!(
        bytes >= 4096,
        "the counting allocator must see a 4 KiB allocation (seen: {bytes})"
    );
    let (_, nothing) = bytes_of(|| 1 + 1);
    assert_eq!(nothing, 0, "an addition does not allocate");
}

#[test]
fn b17_writing_to_a_column_does_not_depend_on_the_project_size() {
    use stokesforge_core::ValueBlock;
    let mut small = project_with(10);
    let mut large = project_with(1000);
    let small_target = small.resolve_path("[Book1]Sheet1!c0").expect("column");
    let large_target = large.resolve_path("[Book1]Sheet1!c0").expect("column");

    let (_, a) = bytes_of(|| small.set_values(small_target, 0, ValueBlock::Float(vec![1.0; 64])));
    let (_, b) = bytes_of(|| large.set_values(large_target, 0, ValueBlock::Float(vec![1.0; 64])));
    println!(
        "write: {a} bytes in a project of {} nodes, {b} in one of {} nodes",
        small.tree().len(),
        large.tree().len()
    );
    assert!(
        b <= a * 4,
        "writing to a column costs {b} bytes on a large project against {a} on a \
         small one: the write must depend on the written block, not on the project size"
    );
}

#[test]
fn b17_numeric_view_of_a_mixed_column_costs_only_once_per_resolution() {
    // `Column::numeric_view` is documented as "the only model read that
    // copies" (`column.rs`): a mixed column has no contiguous `f64` buffer, so
    // `resolve_series` materializes one, once per referenced mixed column. The
    // guard: a full resolution of a series whose Y is mixed costs no more than
    // a single call to `numeric_view` on that same column — if one day
    // `resolve_series` (or a renderer downstream) started calling it again per
    // plotted point rather than once per resolution, this test would grow with
    // the number of calls, not with the size of the column.
    const POINTS: usize = 20_000;

    let mut project = Project::new("D").expect("valid project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("worksheet");
    let cells: Vec<MixedCell> = (0..POINTS)
        .map(|i| {
            if i % 7 == 0 {
                MixedCell::Text("n/a".to_string())
            } else {
                MixedCell::Number(i as f64)
            }
        })
        .collect();
    let y = project
        .add_column(sheet, None, Some("Y"), Column::from_mixed(&cells))
        .expect("mixed column");
    let series = SeriesSpec {
        plot_type: PlotType::Histogram, // needs_x() == false: a single mixed column involved
        ..SeriesSpec::new(y)
    };

    // Cost of a full resolution (one implicit call to `numeric_view`).
    let (_, cost_of_one_resolution) = bytes_of(|| resolve_series(project.tree(), &series));

    // Reference cost: a single direct call to `numeric_view`, outside any
    // resolution — what `resolve_series` must come close to.
    let column = project
        .tree()
        .node(y)
        .and_then(Node::as_column)
        .expect("mixed column");
    let (_, cost_of_one_call) = bytes_of(|| column.numeric_view());

    // The useful counterexample: what a real regression — calling
    // `numeric_view` again for every plotted point rather than once — would
    // cost. 50 calls is a very conservative budget of displayed points (a graph
    // reduced to a few dozen pixels wide).
    const SIMULATED_CALLS: usize = 50;
    let (_, cost_of_per_point_calls) = bytes_of(|| {
        for _ in 0..SIMULATED_CALLS {
            let _ = column.numeric_view();
        }
    });

    println!(
        "mixed column of {POINTS} cells: full resolution = {cost_of_one_resolution} bytes, \
         one direct call to numeric_view = {cost_of_one_call} bytes, {SIMULATED_CALLS} calls \
         (simulated regression) = {cost_of_per_point_calls} bytes"
    );

    assert!(
        cost_of_one_resolution <= cost_of_one_call * 2,
        "resolving the series cost {cost_of_one_resolution} bytes against {cost_of_one_call} for a \
         single call to `numeric_view`: `resolve_series` calls `numeric_view` more than once \
         for this column"
    );
    assert!(
        cost_of_per_point_calls >= cost_of_one_call * (SIMULATED_CALLS / 2),
        "the counterexample ({SIMULATED_CALLS} direct calls) should cost about \
         {SIMULATED_CALLS}× one call — {cost_of_per_point_calls} bytes seen against \
         {cost_of_one_call} for a single one: if this is no longer true, the counterexample \
         proves nothing any more and this test must be revisited"
    );
}

// -- reproductions --------------------------------------------------------

#[test]
fn b17_resolving_a_path_does_not_depend_on_the_project_size() {
    let small = project_with(10);
    let large = project_with(1000);
    let path = "[Book1]Sheet1!c0"; // strictly the same path, same depth

    // A dry run: the first resolution in a fresh project may pay allocator
    // caches that have nothing to do with the measurement.
    let _ = small.resolve_path(path);
    let _ = large.resolve_path(path);

    let (_, a) = bytes_of(|| {
        for _ in 0..100 {
            small.resolve_path(path).expect("path resolved");
        }
    });
    let (_, b) = bytes_of(|| {
        for _ in 0..100 {
            large.resolve_path(path).expect("path resolved");
        }
    });
    println!(
        "{} nodes: {} bytes per resolution — {} nodes: {} bytes per resolution",
        small.tree().len(),
        a / 100,
        large.tree().len(),
        b / 100
    );
    assert!(
        b <= a * 4,
        "the project is {:.0}× larger and resolving the same path allocates {:.0}× more \
         ({} bytes against {}): `Tree::find_window` calls `windows()`, which calls \
         `walk_all()`, which builds a `Vec` of ALL the nodes of the project — on every \
         call. This is the operation the Python bridge does before every access",
        large.tree().len() as f64 / small.tree().len() as f64,
        b as f64 / a as f64,
        b / 100,
        a / 100
    );
}

#[test]
fn b17_naming_a_column_automatically_costs_no_more_than_naming_it() {
    let mut gaps = Vec::new();
    for n in [100usize, 200, 400] {
        let auto = add_columns(n, false);
        let explicit = add_columns(n, true);
        let factor = auto as f64 / explicit as f64;
        println!(
            "{n:>4} columns: automatic name {auto:>9} bytes, imposed name {explicit:>7} \
             bytes ({factor:.0}×)"
        );
        gaps.push((n, factor));
    }
    // The factor doubles when n doubles: this is the signature of the cube.
    let worst = gaps.last().copied().unwrap_or((0, 0.0));
    assert!(
        worst.1 <= 4.0,
        "at {} columns, automatic naming costs {:.0}× imposed naming, and this \
         factor doubles with every doubling of n ({gaps:?}): `next_column_name` \
         tries the names A, B, … in order and compares each candidate to ALL the \
         names already taken — O(n²) per column, O(n³) for the worksheet. Measured \
         outside the test, in `--release`: 4,000 columns with an automatic name take \
         17.8 s against 0.19 s with an imposed name",
        worst.0,
        worst.1
    );
}

/// #316: loading a folder with a very large number of children must not cost a
/// scan of the list per child (quadratic). Very loose time bound: 200,000
/// children took several minutes before, a few dozen milliseconds after.
#[test]
fn b17_loading_a_folder_with_many_children_is_not_quadratic() {
    use stokesforge_core::{NodeKind, Tree};
    const N: u64 = 200_000;
    let root = NodeId::from_raw(1);
    let children: Vec<NodeId> = (2..N + 2).map(NodeId::from_raw).collect();
    let mut nodes = vec![Node::new(
        root,
        "Project",
        NodeKind::Folder {
            children: children.clone(),
        },
    )
    .unwrap()];
    for (i, id) in children.iter().enumerate() {
        nodes.push(
            Node::with_parent(
                *id,
                &format!("D{i}"),
                Some(root),
                NodeKind::Folder { children: vec![] },
            )
            .unwrap(),
        );
    }
    let start = std::time::Instant::now();
    let tree = Tree::from_nodes(root, nodes).expect("valid tree");
    let elapsed = start.elapsed();
    assert_eq!(tree.len(), N as usize + 1);
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "loading {N} children: {elapsed:?} (quadratic?)"
    );
}
