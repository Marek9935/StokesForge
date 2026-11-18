//! Guard (#512): no code under `crates/` sends a viewport command that locks
//! the mouse or moves the cursor. The page-corner drag once did, and it failed
//! on Windows; the system cursor stays free, as in the grid (#535).

use std::fs;
use std::path::{Path, PathBuf};

const COMMANDS: [&str; 2] = ["Cursor\u{47}rab", "CursorPos\u{69}tion"];

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

/// The `path:line` of every use of a lock command in `text` of `path`.
fn uses(path: &str, text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| COMMANDS.iter().any(|command| line.contains(command)))
        .map(|(n, _)| format!("{path}:{}", n + 1))
        .collect()
}

#[test]
fn no_code_locks_the_mouse_or_moves_the_cursor() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    rust_files(&crates, &mut files);
    assert!(files.len() > 100, "the sources were not found");
    let mut offenders = Vec::new();
    for file in files {
        let name = file.to_string_lossy().replace('\\', "/");
        if name.ends_with("/tests/cursor_lock_guard.rs")
            // The test frames read the commands the frames asked for.
            || name.ends_with("/stokesforge-render-egui/tests/layout_mode.rs")
        {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&file) {
            offenders.extend(uses(&name, &text));
        }
    }
    assert!(
        offenders.is_empty(),
        "the cursor stays free, no lock or move command: {offenders:?}"
    );
}

/// The witness: a send written anywhere is seen.
#[test]
fn the_guard_sees_a_plain_send() {
    let send = "ctx.send_viewport_cmd(egui::ViewportCommand::Cursor\u{47}rab(g));";
    assert_eq!(uses("a.rs", send), ["a.rs:1"]);
    assert!(uses("a.rs", "let lock = 1;").is_empty());
}
