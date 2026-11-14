//! Guard (#512): the two viewport commands that lock the mouse and move the
//! cursor are sent by `src/corner_lock.rs` only. A lock sent anywhere else
//! would have no lease, and a lock left on freezes the user's mouse.

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
fn the_cursor_lock_commands_stay_in_corner_lock() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    rust_files(&crates, &mut files);
    assert!(files.len() > 100, "the sources were not found");
    let mut offenders = Vec::new();
    for file in files {
        let name = file.to_string_lossy().replace('\\', "/");
        if name.ends_with("/stokesforge-render-egui/src/corner_lock.rs")
            || name.ends_with("/tests/cursor_lock_guard.rs")
            // The benches read the commands the frames asked for.
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
        "lock the mouse through `corner_lock` only: {offenders:?}"
    );
}

/// The witness: a send written anywhere is seen.
#[test]
fn the_guard_sees_a_plain_send() {
    let send = "ctx.send_viewport_cmd(egui::ViewportCommand::Cursor\u{47}rab(g));";
    assert_eq!(uses("a.rs", send), ["a.rs:1"]);
    assert!(uses("a.rs", "let lock = 1;").is_empty());
}
