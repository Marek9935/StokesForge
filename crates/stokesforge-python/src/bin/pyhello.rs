//! `stokesforge-pyhello` — proof that the embedded Python stack packages correctly.
//!
//! Starts the vendored interpreter, imports numpy, does a one-way then a
//! round-trip of an array, and measures the three timings that decide the
//! architecture: startup, sending 1 M `f64`, bringing back 1 M `f64`.
//!
//! This binary exists only to be run from `dist/pyhello/` in a cleaned
//! environment (see `scripts/pyhello-dist.sh`): its output is the
//! proof that isolation holds.

use std::process::ExitCode;
use std::time::Instant;

use stokesforge_python::interpreter::{self, EmbeddedPython, PythonError};

/// Size of the measurement arrays: large enough for the copy to be measurable.
const BENCH_LEN: usize = 1_000_000;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("stokesforge-pyhello: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), PythonError> {
    let embedded = EmbeddedPython::locate()?;
    println!("embedded prefix       : {}", embedded.home().display());

    // (a) interpreter startup + numpy import.
    let start = Instant::now();
    embedded.initialize()?;
    let init_ms = start.elapsed().as_secs_f64() * 1e3;

    println!(
        "Python version         : {}",
        interpreter::python_version()?
    );
    println!("numpy version          : {}", interpreter::numpy_version()?);
    println!(
        "sys.flags.isolated    : {}",
        interpreter::sys_flags_isolated()?
    );
    println!("sys.prefix            : {}", interpreter::sys_prefix()?);

    let path = interpreter::sys_path()?;
    println!("sys.path              : {} entries", path.len());
    for (index, entry) in path.iter().take(3).enumerate() {
        println!("sys.path[{index}]           : {entry}");
    }

    // Self-check: the distribution declares itself broken if a single `sys.path`
    // entry lies outside the embedded prefix. This test, run from a
    // trapped environment, is the proof of isolation.
    if let Some(stray) = path.iter().find(|entry| !embedded.contains_path(entry)) {
        return Err(PythonError::InitFailed(format!(
            "sys.path contains an entry outside the embedded prefix: {stray}"
        )));
    }

    // One way: Rust → numpy copy then sum on the Python side.
    let values: Vec<f64> = (0..BENCH_LEN).map(|index| index as f64).collect();
    let sum = interpreter::roundtrip_sum(&values)?;
    println!("roundtrip_sum(1M)     : {sum}");

    // Full round trip: `arr * 2.0` on the Python side then bring back.
    let doubled = interpreter::roundtrip_vec(&values)?;
    println!(
        "roundtrip_vec(1M)     : {} values, last = {:?}",
        doubled.len(),
        doubled.last()
    );
    println!(
        "roundtrip_vec(10)     : {:?}",
        interpreter::roundtrip_vec(&values[..10])?
    );

    // (b) and (c): the two halves of the copy, measured separately.
    let timings = interpreter::measure_copy(&values)?;

    println!("--- measurements (ms) ---");
    println!("(a) init : interpreter + numpy import : {init_ms:.1}");
    println!(
        "(b) send   1M f64 (Rust → numpy, copy)   : {:.1}",
        timings.send_ms
    );
    println!(
        "(c) return 1M f64 (numpy → Vec<f64>)     : {:.1}",
        timings.recv_ms
    );

    Ok(())
}
