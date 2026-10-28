//! A single "StokesForge configuration folder" rule (#332).
//!
//! `stokesforge_import::config_dir` (variable `STOKESFORGE_CONFIG_DIR`, else
//! `directories::ProjectDirs`) is the only door to the user's
//! profile. A `ProjectDirs::from` elsewhere bypasses the variable: the
//! tests, which redirect the folder through it, would write into the real profile
//! (on Windows `directories` ignores `XDG_CONFIG_HOME`). This guard reads
//! all the sources of `crates/` and refuses the call outside the module.
//!
//! Comment lines are ignored (a doc may name the call). It does not
//! see another way to compute a profile path (`dirs`, `APPDATA`
//! read by hand): review.

use std::fs;
use std::path::{Path, PathBuf};

/// The call forbidden outside the module (written in two pieces: this file does
/// not count itself, besides being excluded by its path).
const FORBIDDEN_CALL: &str = concat!("ProjectDirs", "::from");

/// The module that holds the rule, relative to the repository root.
const MODULE: &str = "crates/stokesforge-import/src/config_dir.rs";

/// This guard, relative to the root: it cites the call in its witnesses.
const GUARD: &str = "crates/stokesforge-arch-guard/tests/config_dir.rs";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

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

/// The numbers of the code lines (comments excluded) that make the call.
fn offending_lines(source: &str) -> Vec<usize> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .filter(|(_, line)| line.contains(FORBIDDEN_CALL))
        .map(|(n, _)| n + 1)
        .collect()
}

/// Among sources `(path relative to the root, text)`, the places where
/// the call is made outside the module and this guard.
fn offenders(sources: &[(String, String)]) -> Vec<String> {
    sources
        .iter()
        .filter(|(path, _)| path != MODULE && path != GUARD)
        .flat_map(|(path, text)| {
            offending_lines(text)
                .into_iter()
                .map(move |n| format!("{path}:{n}"))
        })
        .collect()
}

#[test]
fn project_dirs_is_called_only_by_the_config_dir_module() {
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

    let module = read
        .iter()
        .find(|(path, _)| path == MODULE)
        .expect("the config_dir module exists");
    assert_eq!(
        offending_lines(&module.1).len(),
        1,
        "the module makes the call, only once"
    );
    let offenders = offenders(&read);
    assert!(
        offenders.is_empty(),
        "ProjectDirs::from hors de {MODULE} : passer par \
         `stokesforge_import::config_dir::config_dir()` (#332)\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_guard_refuses_a_call_outside_the_module_and_ignores_comments() {
    let call = format!("let d = directories::{FORBIDDEN_CALL}(\"\", \"\", \"X\");");
    let fake = vec![
        (
            "crates/stokesforge-app-state/src/fake.rs".to_string(),
            format!("fn f() {{\n    {call}\n}}\n"),
        ),
        (MODULE.to_string(), format!("fn permis() {{ {call} }}\n")),
        (
            "crates/stokesforge-app/src/doc.rs".to_string(),
            format!("/// {call}\n// {call}\nfn rien() {{}}\n"),
        ),
    ];
    assert_eq!(
        offenders(&fake),
        ["crates/stokesforge-app-state/src/fake.rs:2"],
        "only the fake source outside the module, in code, is refused"
    );
}
