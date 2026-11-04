//! `stokesforge-logo-gen --icon ICON.svg --project PROJECT.svg --out DIR`
//!
//! Writes the PNG trees and the `.ico` files of a release (see the library).
//! Any refusal exits with status 1 and one line on stderr.

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: stokesforge-logo-gen --icon ICON.svg --project PROJECT.svg --out DIR";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("stokesforge-logo-gen: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let (mut icon, mut project, mut out) = (None, None, None);
    let mut it = args.into_iter();
    while let Some(flag) = it.next() {
        let slot = match flag.as_str() {
            "--icon" => &mut icon,
            "--project" => &mut project,
            "--out" => &mut out,
            _ => return Err(format!("unknown argument `{flag}`\n{USAGE}")),
        };
        *slot = Some(PathBuf::from(
            it.next()
                .ok_or_else(|| format!("`{flag}` needs a value\n{USAGE}"))?,
        ));
    }
    let (Some(icon), Some(project), Some(out)) = (icon, project, out) else {
        return Err(USAGE.to_string());
    };
    stokesforge_logo_gen::generate(&icon, &project, &out).map_err(|e| e.to_string())
}
