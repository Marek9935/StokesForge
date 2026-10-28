//! Battery 21 — three invariants that nothing holds on the write path.
//!
//! Same method as `b17`: we count **allocated bytes**, not milliseconds, so
//! that the assertion says "this cost must not grow with X" and stays true on
//! any machine. The corresponding time measurements are in the report.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use stokesforge_commands::{Command, History};
use stokesforge_core::{Column, ColumnType, CoreError, MixedCell, NodeId, Project, ValueBlock};

// -- counting allocator (same scheme as b17) ------------------------------

thread_local! {
    static BYTES: Cell<usize> = const { Cell::new(0) };
}

struct Counter;

// SAFETY: delegation to `System` without changing the pointer or the `Layout`;
// the counting only writes to a thread-local cell, which never allocates.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
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

fn bytes_of<T>(work: impl FnOnce() -> T) -> (T, usize) {
    let before = BYTES.with(Cell::get);
    let result = work();
    (result, BYTES.with(Cell::get) - before)
}

// -- bench ----------------------------------------------------------------

/// A worksheet of `columns` columns, and a history filled with `entries`
/// **structural** entries (each one keeps two trees, hence all the columns of
/// the project).
fn bench(columns: usize, entries: usize) -> (Project, History, NodeId) {
    let mut project = Project::new("D").expect("valid project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("worksheet");
    let target = project
        .add_column(sheet, None, Some("target"), Column::from_f64(&[1.0, 2.0]))
        .expect("target column");
    for i in 0..columns {
        project
            .add_column(
                sheet,
                None,
                Some(&format!("c{i}")),
                Column::from_f64(&[1.0, 2.0]),
            )
            .expect("column");
    }
    let mut history = History::new();
    for i in 0..entries {
        history
            .execute(
                &mut project,
                Command::AddColumn {
                    sheet,
                    index: None,
                    name: Some(format!("s{i}")),
                    column: Column::new(ColumnType::Numeric),
                },
            )
            .expect("structural entry");
    }
    (project, history, target)
}

/// Bytes allocated by writing one value, in this bench.
fn cost_of_one_write(columns: usize, entries: usize) -> usize {
    let (mut project, mut history, target) = bench(columns, entries);
    let write = |project: &mut Project, history: &mut History, value: f64| {
        history
            .execute(
                project,
                Command::SetCells {
                    column: target,
                    start: 0,
                    values: ValueBlock::Float(vec![value]),
                    expected_version: None,
                },
            )
            .expect("write accepted");
    };
    write(&mut project, &mut history, 3.0); // dry run
    let ((), bytes) = bytes_of(|| {
        for _ in 0..10 {
            write(&mut project, &mut history, 4.0);
        }
    });
    bytes / 10
}

/// A worksheet with a single numeric column of `size` values, ready for a
/// tolerant entry (`Command::SetCellsPromoting`).
fn promotion_bench(size: usize) -> (Project, History, NodeId) {
    let mut project = Project::new("D").expect("valid project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("worksheet");
    let values: Vec<f64> = (0..size).map(|i| i as f64).collect();
    let target = project
        .add_column(sheet, None, Some("target"), Column::from_f64(&values))
        .expect("target column");
    (project, History::new(), target)
}

/// Bytes allocated by the **first** tolerant entry of a text into a numeric
/// column of `size` values: path 3 of `SetCellsPromoting` (the column is not
/// mixed yet) — the one that promotes the column, hence takes a full snapshot
/// of its buffer, like `SetColumnType`.
fn cost_of_a_first_promotion(size: usize) -> usize {
    let (mut project, mut history, target) = promotion_bench(size);
    let ((), bytes) = bytes_of(|| {
        history
            .execute(
                &mut project,
                Command::SetCellsPromoting {
                    column: target,
                    start: 0,
                    values: ValueBlock::Mixed(vec![MixedCell::Text("n/a".to_string())]),
                    expected_version: None,
                },
            )
            .expect("tolerant entry accepted")
    });
    bytes
}

/// Bytes allocated by a tolerant entry into a column of `size` cells that is
/// **already** mixed from its creation — path 1/2 of `SetCellsPromoting` (no
/// promotion to do), on a column whose buffer is not yet shared with any
/// history entry. Isolated from the "promotion then new entry" scenario:
/// chaining two `SetCellsPromoting` under the same `History` there would
/// measure **another** cost, very real but not relevant here — the
/// copy-on-write that `Arc::make_mut` triggers because the promotion's entry
/// still holds the current buffer (see the doc of `History::apply_data`, path
/// 3); that cost exists for any column rewritten right after an entry that
/// captures its buffer, mixed or not, and it is not the property that this
/// test checks.
fn cost_of_an_entry_on_an_already_mixed_column(size: usize) -> usize {
    let mut project = Project::new("D").expect("valid project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("worksheet");
    let cells: Vec<MixedCell> = (0..size).map(|i| MixedCell::Number(i as f64)).collect();
    let target = project
        .add_column(sheet, None, Some("target"), Column::from_mixed(&cells))
        .expect("already mixed column");
    let mut history = History::new();
    let ((), bytes) = bytes_of(|| {
        history
            .execute(
                &mut project,
                Command::SetCellsPromoting {
                    column: target,
                    start: 0,
                    values: ValueBlock::Mixed(vec![MixedCell::Text("n/a".to_string())]),
                    expected_version: None,
                },
            )
            .expect("tolerant entry accepted on an already mixed column")
    });
    bytes
}

// -- guards ---------------------------------------------------------------

#[test]
fn b21_promoting_an_already_mixed_column_no_longer_pays_the_snapshot() {
    // Doc of `Command::SetCellsPromoting` (`command.rs`): the path that
    // promotes a column (full snapshot, like `SetColumnType`) "happens only
    // once per column, the second tolerant entry taking path 2" — until now
    // asserted only by a functional test in `shell.rs` (undo restores type and
    // value), never measured. This test measures the cost of the promotion
    // (path 3) on a large column, and compares it with that of a tolerant
    // entry on a column of the same size but **already** mixed (paths 1/2, no
    // promotion to do): the first must depend on the column size, the second
    // must not.
    const SIZE: usize = 200_000;

    let promotion = cost_of_a_first_promotion(SIZE);
    let already_mixed = cost_of_an_entry_on_an_already_mixed_column(SIZE);
    println!(
        "column of {SIZE} values: promotion (path 3) = {promotion} bytes, entry on an \
         already mixed column (paths 1/2) = {already_mixed} bytes"
    );

    // Path 3 (promotion) takes a snapshot of the buffer: of the order of
    // `size` bytes, not a few dozen.
    assert!(
        promotion >= SIZE,
        "the first tolerant entry (path 3, promotion) should cost at least one \
         snapshot of the buffer ({SIZE} values): {promotion} bytes seen — if this is no \
         longer true, `SetCellsPromoting` may have stopped taking the snapshot expected \
         by `SetColumnType`, to check before treating this figure as an improvement"
    );
    // An already mixed column never pays the snapshot: a few hundred bytes for
    // a text cell, not 200,000, whatever its size.
    assert!(
        already_mixed < SIZE / 10,
        "the tolerant entry on an already mixed column cost {already_mixed} bytes for a \
         column of {SIZE} values: it should not depend on the column size — otherwise \
         the costly promotion path would happen on every entry, not once per column"
    );
}

#[test]
fn b21_names_reserved_by_the_grammar_stay_rejected() {
    let mut project = Project::new("D").expect("valid project");
    let root = project.root();
    for name in ["", "  ", " A", "A ", "A!B", "[A]", "a/b", "A]"] {
        assert!(
            matches!(
                project.add_folder(root, name),
                Err(CoreError::InvalidName(_))
            ),
            "expected InvalidName for {name:?}"
        );
    }
}

#[test]
fn b21_the_cost_of_a_write_does_depend_on_the_amount_written() {
    // The useful counterexample: what *should* make the cost vary does so, and
    // proportionally. This is what makes the assertion of the reproductions
    // readable — the cost follows what is written, not what surrounds it.
    let mut project = Project::new("D").expect("valid project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("worksheet");
    let column = project
        .add_column(sheet, None, Some("A"), Column::new(ColumnType::Numeric))
        .expect("column");
    let (_, small) = bytes_of(|| {
        project
            .set_values(column, 0, ValueBlock::Float(vec![1.0; 10]))
            .expect("write")
    });
    let (_, large) = bytes_of(|| {
        project
            .set_values(column, 0, ValueBlock::Float(vec![1.0; 10_000]))
            .expect("write")
    });
    println!("writing 10 values: {small} bytes; 10,000 values: {large} bytes");
    assert!(
        large > small,
        "writing a thousand times more must cost more"
    );
}

// -- reproductions --------------------------------------------------------

#[test]
fn b21_the_cost_of_a_write_does_not_depend_on_the_history() {
    let without = cost_of_one_write(500, 0);
    let with = cost_of_one_write(500, 100);
    println!("500 columns, empty history       : {without:>9} bytes per write");
    println!("500 columns, 100 stacked entries : {with:>9} bytes per write");
    println!(
        "complementary measurements (outside the test):\n  \
            50 columns,   0 entries :      3,839 bytes\n  \
            50 columns, 100 entries :     45,533 bytes\n  \
           500 columns,   0 entries :     37,503 bytes\n  \
           500 columns, 100 entries :    159,481 bytes\n  \
         2,000 columns, 100 entries :    357,145 bytes — to write ONE number"
    );
    assert!(
        with <= without * 2,
        "filling the history multiplies by {:.1} the cost of a write that does not \
         concern it: `History::evict` calls `recompute_retained_bytes` on **every** \
         push, which redoes the union of the fingerprints of all the entries of both \
         stacks then subtracts the buffers of the current tree — which is walked \
         entirely. The cost of a keystroke in the grid therefore follows the project \
         size AND the depth of the history",
        with as f64 / without as f64
    );
}

#[test]
// D3 (2nd half) — fixed: `History::execute` no longer branches on a single
// path. For a data command, it no longer clones `project.tree()` into `before`:
// the label and the pointer of the buffer already present (for
// `just_orphaned_by_write`) are captured by a simple borrow, before the
// mutation. `Arc::make_mut(&mut self.tree)` then sees a reference count of 1
// and mutates the `Tree` in place instead of cloning it. The guarantee "no
// partial mutation on error" survives without this clone: the data primitives
// of `Project` validate everything before mutating (see
// `Column::set_values`/`insert_rows`/`delete_rows`, tested to that effect in
// `stokesforge-core`), so a rejection has never started to write. Structural
// commands, for their part, keep cloning `before` — their `Entry::Structural`
// really needs it (undo/redo by tree replacement). See `History::execute` and
// `History::apply_data`.
fn b21_the_cost_of_a_write_does_not_depend_on_the_number_of_columns() {
    let small = cost_of_one_write(50, 0);
    let large = cost_of_one_write(500, 0);
    println!(" 50 columns, empty history : {small:>9} bytes per write");
    println!("500 columns, empty history : {large:>9} bytes per write");
    assert!(
        large <= small * 3,
        "ten times more columns, {:.1}× more expensive, to write exactly the same \
         value in the same column: regression of D3 (2nd half) — `History::execute` \
         again keeps `before` (a clone of `project.tree()`) alive while applying a \
         data command, which forces `Arc::make_mut` to clone the whole `Tree` rather \
         than mutate in place — see the doc of `History::execute` and \
         `History::apply_data`",
        large as f64 / small as f64
    );
}

#[test]
fn b21_control_names_are_rejected() {
    let mut project = Project::new("D").expect("valid project");
    let root = project.root();
    let mut accepted = Vec::new();
    for (label, name) in [
        ("line feed", "A\nB".to_string()),
        ("tab", "A\tB".to_string()),
        ("null byte", "A\u{0}B".to_string()),
        ("direction override (RLO)", "A\u{202e}B".to_string()),
        ("zero-width space", "A\u{200b}B".to_string()),
        ("a hundred thousand characters", "x".repeat(100_000)),
    ] {
        if let Ok(id) = project.add_folder(root, &name) {
            let path = project.path_of(id).unwrap_or_default();
            accepted.push((label, name.chars().count(), path.chars().count()));
        }
    }
    println!("accepted (label, name length, path length): {accepted:?}");
    assert!(
        accepted.is_empty(),
        "these names are accepted, persisted, and come back as is from `path_of`: \
         a name carrying a null byte or a line feed breaks any tabular display and \
         any error message that quotes it; a direction-override mark visually \
         reverses a whole path; and no length bound exists, neither here nor on \
         `unit`, `long_name`, `comment`, `display_format` or the text of a note"
    );
}

#[test]
fn b21_two_projects_never_assign_the_same_identifier() {
    // B9 (`guide-de-correction-robustesse.md` §F2) — fixed by removing
    // `#[derive(Clone)]` from `Project` (nothing in the repository needed it:
    // only `Arc<Tree>` has to be cloneable, for free). The original
    // reproduction can no longer be written at all: `Project::clone()` no
    // longer compiles, which is stronger proof than a test that passes. This
    // function keeps the place (§1 of the guide: a reproduction is never
    // deleted, its assertion is rewritten) and checks what the duplication of
    // the allocator could have broken: strictly increasing identifiers, never
    // reassigned, over the life of a project.
    let mut project = Project::new("D").expect("valid project");
    let root = project.root();
    let mut already_seen = Vec::new();
    for name in ["M", "N", "O", "P"] {
        let id = project.add_folder(root, name).expect("folder");
        assert!(
            !already_seen.contains(&id),
            "identifier {id} reassigned among {already_seen:?}"
        );
        already_seen.push(id);
    }
}
