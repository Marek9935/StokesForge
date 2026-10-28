//! Stack probe: run in a subprocess, because a stack overflow is **not** a
//! panic. The Rust runtime detects it, writes `has overflowed its stack` and
//! calls `abort()`: neither `catch_unwind` nor `JoinHandle::join` gives
//! control back, and the whole process dies, including the test binary if it
//! lived there.
//!
//! Usage: `stokesforge-stack-probe <depth> <action> [stack_bytes]`
//!
//! `action` is one of `build`, `undo`, `clear`, `drop`. The stack defaults to
//! 2 MiB, the size a `std::thread` gets when nobody sets it, which is the size
//! of the `stokesforge-session` actor thread: it calls
//! `thread::Builder::new().name(…).spawn(…)` without `stack_size`.

use stokesforge_commands::{Command, History, MacroKind};
use stokesforge_core::{Column, Project, ValueBlock};

/// Stack of a `std::thread` when `stack_size` is not set.
const DEFAULT_STACK: usize = 2 * 1024 * 1024;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let depth: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(64);
    let action = args.get(2).cloned().unwrap_or_else(|| "undo".to_string());
    let stack: usize = args
        .get(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_STACK);

    println!("depth = {depth}, action = {action}, stack = {stack} bytes");

    let worker = std::thread::Builder::new()
        .name("stack-probe".to_string())
        .stack_size(stack)
        .spawn(move || work(depth, &action))
        .expect("thread spawned");
    match worker.join() {
        Ok(()) => println!("NORMAL END"),
        Err(_) => {
            println!("the thread panicked");
            std::process::exit(3);
        }
    }
}

fn work(depth: usize, action: &str) {
    let mut project = Project::new("D").expect("valid project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("sheet");
    let column = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0]))
        .expect("column");

    let mut history = History::with_limit(0);
    for i in 0..depth {
        // B7 (`guide-de-correction-robustesse.md` §F4): `begin_macro` now
        // refuses (`MacroTooDeep`) beyond a bounded depth, well before the
        // boundary where `revert`/`Drop` (recursive on `Entry::Macro`) would
        // overflow the stack. A probe run with a `depth` beyond this bound
        // must therefore see this refusal, not a crash: that is exactly the
        // proof that the fix works.
        if let Err(err) = history.begin_macro(MacroKind::Import) {
            println!("  begin_macro refused at level {i}: {err} - CLEAN REFUSAL, no crash");
            return;
        }
    }
    history
        .execute(
            &mut project,
            Command::SetCells {
                column,
                start: 0,
                values: ValueBlock::Float(vec![9.0]),
                expected_version: None,
            },
        )
        .expect("write accepted");
    for _ in 0..depth {
        history.end_macro(&mut project).expect("close");
    }
    println!("  close: OK, {} top-level entr(ies)", history.len());

    match action {
        // Builds the nesting, then forgets it without destroying it: shows
        // that BUILDING it never overflows (everything is iterative there).
        // Walking it, or destroying it, is what overflows.
        "build" => {
            std::mem::forget(history);
            println!("  build only: OK");
        }
        // `History::revert` recurses on `Entry::Macro`.
        "undo" => {
            let done = history.undo(&mut project).expect("undo");
            println!("  undo: {done}");
        }
        // `clear()` empties the stacks: the derived `Drop` of `Entry` recurses.
        // This is what `stokesforge-session` calls on every `load`.
        "clear" => {
            history.clear().expect("clear");
            println!("  clear: OK");
        }
        // Dropping the history: same recursive `Drop`.
        "drop" => {
            drop(history);
            println!("  drop: OK");
        }
        other => {
            eprintln!("unknown action: {other}");
            std::process::exit(2);
        }
    }
}
