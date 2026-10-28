//! Allocation probe: run in a subprocess by battery 5, because a Rust
//! allocation failure is an `abort()`, not a catchable panic.
//!
//! Usage: `alloc_probe <case> [argument]`

use stokesforge_core::{Column, Project, ValueBlock};

fn sample_project() -> (Project, stokesforge_core::NodeId, stokesforge_core::NodeId) {
    let mut p = Project::new("S").unwrap();
    let root = p.root();
    p.add_workbook(root, Some("Book1")).unwrap();
    let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
    let col = p
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    (p, sheet, col)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let case = args.get(1).map(String::as_str).unwrap_or("");
    let n: usize = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_usize << 45);

    match case {
        // `SetCells { start: n }` on a column of 3 rows.
        "set_values_start" => {
            let (mut p, _sheet, col) = sample_project();
            println!("before: length = {}", length(&p, col));
            let r = p.set_values(col, n, ValueBlock::Float(vec![7.0]));
            match r {
                Ok(delta) => println!(
                    "ACCEPTED: length = {}, delta.previous_length = {}",
                    length(&p, col),
                    delta.previous_length
                ),
                Err(e) => println!("refused: {e}"),
            }
        }
        // `set_length(n)`
        "set_length" => {
            let (mut p, _sheet, col) = sample_project();
            let r = p.set_length(col, n);
            match r {
                Ok(_) => println!("ACCEPTED: length = {}", length(&p, col)),
                Err(e) => println!("refused: {e}"),
            }
        }
        // `insert_rows(sheet, 0, n)`
        "insert_rows" => {
            let (mut p, sheet, col) = sample_project();
            let r = p.insert_rows(sheet, 0, n);
            match r {
                Ok(v) => println!(
                    "ACCEPTED: {} column(s) touched, length = {}",
                    v.len(),
                    length(&p, col)
                ),
                Err(e) => println!("refused: {e}"),
            }
        }
        // Through the command bus, with history: also measures the delta.
        "command_setcells" => {
            use stokesforge_commands::{Command, History};
            let (mut p, _sheet, col) = sample_project();
            let mut h = History::new();
            let r = h.execute(
                &mut p,
                Command::SetCells {
                    column: col,
                    start: n,
                    values: ValueBlock::Float(vec![7.0]),
                    expected_version: None,
                },
            );
            match r {
                Ok(()) => {
                    println!("ACCEPTED: length = {}", length(&p, col));
                    println!("bytes retained by the history = {}", h.retained_bytes());
                    let ok = h.undo(&mut p).unwrap();
                    println!("undo = {ok}, length after = {}", length(&p, col));
                }
                Err(e) => println!("refused: {e}"),
            }
        }
        // `histogram(bins)`: `bins` comes from the file with no cap.
        "histogram_bins" => {
            use std::num::NonZeroU32;
            let bins = NonZeroU32::new(n.min(u32::MAX as usize) as u32).unwrap_or(NonZeroU32::MIN);
            println!("bins = {bins}");
            let (edges, counts) = stokesforge_core::resolve::histogram(&[1.0, 2.0, 3.0], bins);
            println!("ACCEPTED: {} edges, {} bins", edges.len(), counts.len());
        }
        other => {
            eprintln!("unknown case: {other}");
            std::process::exit(2);
        }
    }
    println!("NORMAL END");
}

fn length(p: &Project, col: stokesforge_core::NodeId) -> usize {
    p.tree().node(col).unwrap().as_column().unwrap().len()
}
