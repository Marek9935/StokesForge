//! The logo stays out of the code (#388, #444).
//!
//! `assets/logo/` is not under the GPL (`assets/logo/LICENSE`): official
//! builds ship the logo, the code builds and runs without it. So no Rust
//! source (`src/`, `tests/`, `build.rs` of any crate) may name the folder: an
//! `include_bytes!` of a logo file would make a build without the folder fail,
//! and would put the logo into every binary. The packaging scripts
//! (`scripts/logo-dist.sh`) are the only readers; the application reads the
//! installed copy at run time (`stokesforge_app::app_logo`).
//!
//! Comment lines are ignored (a doc may name the folder). The CI jobs that
//! build and test delete `assets/logo/` first, which proves the same thing on
//! the whole build; this guard says it before the push.

use std::fs;
use std::path::{Path, PathBuf};

/// The text forbidden in code (written in two pieces: this file does not count
/// itself, besides being excluded by its path).
const FORBIDDEN: &str = concat!("assets/", "logo");

/// This guard, relative to the repository root: it cites the folder.
const GUARD: &str = "crates/stokesforge-arch-guard/tests/logo_boundary.rs";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The Rust sources of a crate that are built or run: `src/`, `tests/` and
/// `build.rs`.
fn sources(folder: &Path, output: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(folder).expect("folder read") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "target") {
                sources(&path, output);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            output.push(path);
        }
    }
}

/// The numbers of the code lines (comments excluded) that name the folder.
fn offending_lines(source: &str) -> Vec<usize> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .filter(|(_, line)| line.contains(FORBIDDEN))
        .map(|(n, _)| n + 1)
        .collect()
}

/// Among sources `(path relative to the root, text)`, the places that name the
/// folder in code.
fn offenders(sources: &[(String, String)]) -> Vec<String> {
    sources
        .iter()
        .filter(|(path, _)| path != GUARD)
        .flat_map(|(path, text)| {
            offending_lines(text)
                .into_iter()
                .map(move |n| format!("{path}:{n}"))
        })
        .collect()
}

#[test]
fn no_source_names_the_logo_folder() {
    let root = root();
    let mut files = Vec::new();
    sources(&root.join("crates"), &mut files);
    let read: Vec<(String, String)> = files
        .iter()
        .map(|path| {
            let relative = path
                .strip_prefix(&root)
                .expect("under the root")
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");
            (relative, fs::read_to_string(path).expect("readable source"))
        })
        .collect();
    assert!(read.len() > 100, "too few sources read: {}", read.len());
    assert!(
        read.iter().any(|(path, _)| path.ends_with("/build.rs")),
        "the build scripts are read"
    );
    let offenders = offenders(&read);
    assert!(
        offenders.is_empty(),
        "the code must not name the logo folder (#388): the logo is read at run time \
         from the installed copy\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_guard_refuses_a_literal_and_ignores_comments() {
    let literal = format!("const L: &[u8] = include_bytes!(\"../../../{FORBIDDEN}/x.png\");");
    let fake = vec![
        (
            "crates/stokesforge-app/src/fake.rs".to_string(),
            format!("fn f() {{}}\n{literal}\n"),
        ),
        (
            "crates/stokesforge-app/build.rs".to_string(),
            format!("fn main() {{ let _ = \"{FORBIDDEN}\"; }}\n"),
        ),
        (
            "crates/stokesforge-app/src/doc.rs".to_string(),
            format!("//! {FORBIDDEN}\n/// {FORBIDDEN}\n// {FORBIDDEN}\nfn nothing() {{}}\n"),
        ),
    ];
    assert_eq!(
        offenders(&fake),
        [
            "crates/stokesforge-app/src/fake.rs:2",
            "crates/stokesforge-app/build.rs:1"
        ],
        "only the code literals are refused"
    );
}
