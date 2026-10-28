//! `stokesforge-selftest` replays, **from a packaged distribution**, the
//! completion criterion of the whole foundation, and exits with 0 or 1.
//!
//! A deliberately thin shell: all the logic lives in
//! [`stokesforge_script::run_selftest`], so that an ordinary `cargo` test
//! (`crates/stokesforge-script/tests/selftest.rs`) exercises it too. This
//! binary only locates the embedded Python prefix next to itself
//! ([`EmbeddedPython::locate`]), calls `run_selftest`, prints the report, and
//! translates the verdict into an exit code.
//!
//! `--quiet` prints only the failed steps and the final verdict, useful in
//! continuous integration: when all goes well, only the last line matters.
//!
//! This binary exists only to be run from `dist/stokesforge-app/` (next to
//! `stokesforge-app`, in the same `lib/python` layout); see
//! `scripts/app-dist.sh`, which builds it, assembles it, then runs it again at
//! the end of the script in a cleaned and trapped environment.

use std::process::ExitCode;

use stokesforge_python::interpreter::EmbeddedPython;
use stokesforge_script::run_selftest;

fn main() -> ExitCode {
    let quiet = std::env::args().skip(1).any(|arg| arg == "--quiet");

    let python = match EmbeddedPython::locate() {
        Ok(python) => python,
        Err(error) => {
            eprintln!("stokesforge-selftest: embedded Python prefix not found: {error}");
            return ExitCode::FAILURE;
        }
    };

    let report = run_selftest(python);

    if quiet {
        for step in &report.steps {
            if !step.passed {
                println!("[FAILED] {} — {}", step.name, step.detail);
            }
        }
    } else {
        println!("=== stokesforge-selftest: environment ===");
        for (label, value) in &report.environment {
            println!("{label:<20}: {value}");
        }
        println!();
        println!("=== stokesforge-selftest: completion criterion steps ===");
        println!("{}", report.render());
        println!();
    }

    if report.passed() {
        println!(
            "stokesforge-selftest: OK - foundation completion criterion verified from the package"
        );
        ExitCode::SUCCESS
    } else {
        println!(
            "stokesforge-selftest: FAILED - the foundation completion criterion is not verified"
        );
        ExitCode::FAILURE
    }
}
