//! Manual guard (issue #363, decision M5 of #345): every menu entry of the
//! application is named in the user manual (`manual/src/`).
//!
//! The rule it serves: a user-visible feature updates the manual in the same
//! commit (see `CLAUDE.md`, "Conventions"). This test is the safety net for the
//! menus, the one place where a new feature always shows up.
//!
//! # What is checked
//!
//! The menu-building functions of [`MENU_FUNCTIONS`] are read from the sources
//! (test code removed, like `tests/labels.rs`). Every `tr!("key")` they contain
//! that is not a tooltip is a menu or submenu entry. Its key is resolved to its
//! English label through the `en` catalog. The label must appear **in bold** in
//! a page of the manual, alone (`**Reset Zoom**`) or as one item of a menu path
//! (`**Graph ▸ Export ▸ Export Graph…**`); the label is copied exactly, ellipsis
//! included.
//!
//! # What is covered
//!
//! The menu bar (File, Edit, Workbook with Sheet and Columns, Graph, Analysis,
//! View, Window, Help) and the context menus that share their content or build
//! their own entries: the workbook cells and headers, the sheet tabs, the Object
//! Manager background. The context menus of the Project Manager and of the graph
//! components build their entries from tables of actions, which a reader of the
//! sources cannot list reliably: they are not covered.
//!
//! # Pages left out
//!
//! `reference/interface-texts.md` and `reference/shortcuts.md` are generated
//! from the catalog and list every label: they would make the guard pass
//! trivially. They never count.
//!
//! # Exceptions
//!
//! [`NOT_IN_MANUAL`] lists the keys that make no sense for a reader. Each one
//! must still be a live entry, so that the list cannot rot.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[path = "support/sources.rs"]
mod sources;

use sources::{functions_named, sources};

/// The functions that build menu entries, wherever they live. Each name must
/// exist (a renamed function fails the test instead of silently leaving the
/// guard).
const MENU_FUNCTIONS: &[&str] = &[
    // The menu bar.
    "file_menu",
    "edit_menu",
    "workbook_menu",
    "sheet_menu",
    "columns_menu_contents",
    "column_type_role_menus",
    "reset_sheet_widths_entry",
    "move_columns_menu",
    "plot_menu",
    "toggle_header_row_menu",
    "send_to_graph_menu",
    "analysis_menu_contents",
    "graph_menu",
    "graph_properties_menu",
    "view_menu",
    "object_manager_settings_contents",
    "window_menu",
    "help_menu",
    // The context menus with their own entries.
    "add_column_entry",
    "tab_strip_tab_menu",
];

/// The generated pages, left out of the search (see the head doc).
const GENERATED_PAGES: &[&str] = &["reference/interface-texts.md", "reference/shortcuts.md"];

/// Keys of menu entries that the manual does not need to name: key, reason.
/// An exception whose key is no longer an entry fails the test.
const NOT_IN_MANUAL: &[(&str, &str)] = &[];

/// Calls whose argument is a tooltip or a passive text, not an entry.
const NOT_ENTRY_CALLS: &[&str] = &[
    "on_hover_text(",
    "on_disabled_hover_text(",
    "weak(",
    "label(",
    "warn(",
    "notice(",
    "fail(",
];

fn manual_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manual/src")
}

/// The `en` catalog: key to value, single-line entries only (a menu label is
/// always on one line).
fn catalog() -> std::collections::BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../stokesforge-app-state/i18n/en/stokesforge_app.ftl");
    let text = std::fs::read_to_string(path).expect("readable en catalog");
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(" = ")?;
            let is_key = !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            is_key.then(|| (key.to_string(), value.trim().to_string()))
        })
        .collect()
}

/// The keys of the single-argument `tr!("key")` calls of `body` that are
/// entries (not tooltips, not disabled-reason or status texts).
fn entry_keys(body: &str, into: &mut BTreeSet<String>) {
    let mut rest = body;
    let mut offset = 0;
    while let Some(at) = rest.find("tr!(") {
        let start = offset + at;
        let after = &body[start + 4..];
        rest = &body[start + 4..];
        offset = start + 4;
        let trimmed = after.trim_start();
        let Some(quoted) = trimmed.strip_prefix('"') else {
            continue;
        };
        let Some(end) = quoted.find('"') else {
            continue;
        };
        let key = &quoted[..end];
        // Only `tr!("key")`: a second argument is a hint, a reason or a value.
        if !quoted[end + 1..].trim_start().starts_with(')') {
            continue;
        }
        let before = body[..start].trim_end();
        if NOT_ENTRY_CALLS.iter().any(|call| before.ends_with(call)) {
            continue;
        }
        if key.starts_with("disabled-") || key.starts_with("status-") {
            continue;
        }
        into.insert(key.to_string());
    }
}

/// The static fragments of a catalog value: `Restore { $title }` gives
/// `["Restore"]`.
fn fragments(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    for c in value.chars() {
        match c {
            '{' => {
                depth += 1;
                if depth == 1 {
                    out.push(std::mem::take(&mut current));
                }
            }
            '}' => depth -= 1,
            _ if depth == 0 => current.push(c),
            _ => {}
        }
    }
    out.push(current);
    out.into_iter()
        .map(|f| f.trim().to_string())
        .filter(|f| !f.is_empty())
        .collect()
}

/// Every page of the manual but the generated ones.
fn manual_pages() -> Vec<(String, String)> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).expect("manual directory").flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                if !GENERATED_PAGES.contains(&relative.as_str()) {
                    let text = std::fs::read_to_string(&path).expect("readable page");
                    out.push((relative, text));
                }
            }
        }
    }
    let root = manual_dir();
    let mut pages = Vec::new();
    walk(&root, &root, &mut pages);
    pages.sort();
    pages
}

/// The items written in bold: `**A ▸ B**` gives `A` and `B`.
fn bold_items(text: &str) -> BTreeSet<String> {
    text.split("**")
        .skip(1)
        .step_by(2)
        .flat_map(|bold| bold.split(" ▸ "))
        .map(|item| item.trim().to_string())
        .collect()
}

/// A label without placeholder must be one bold item, exactly. One with a
/// placeholder (`Restore { $title }`) is matched by its static fragments inside
/// one item.
fn is_named(items: &BTreeSet<String>, value: &str) -> bool {
    if !value.contains('{') {
        return items.contains(value);
    }
    let fragments = fragments(value);
    !fragments.is_empty()
        && items.iter().any(|item| {
            fragments
                .iter()
                .all(|fragment| item.contains(fragment.as_str()))
        })
}

#[test]
fn every_menu_entry_is_named_in_the_manual() {
    let files = sources();
    let mut keys = BTreeSet::new();
    for name in MENU_FUNCTIONS {
        let found = functions_named(&files, name);
        assert!(
            !found.is_empty(),
            "menu function `{name}` of MENU_FUNCTIONS no longer exists: update the list"
        );
        for function in found {
            entry_keys(&function.body, &mut keys);
        }
    }
    assert!(
        keys.len() > 60,
        "the extraction found only {} entries",
        keys.len()
    );

    let catalog = catalog();
    for (key, reason) in NOT_IN_MANUAL {
        assert!(!reason.is_empty(), "exception `{key}` needs a reason");
        assert!(
            keys.contains(*key),
            "exception `{key}` of NOT_IN_MANUAL is not a menu entry anymore: remove it"
        );
    }

    let items: BTreeSet<String> = manual_pages()
        .iter()
        .flat_map(|(_, text)| bold_items(text))
        .collect();
    let missing: Vec<String> = keys
        .iter()
        .filter(|key| !NOT_IN_MANUAL.iter().any(|(k, _)| k == key))
        .filter_map(|key| {
            let value = catalog
                .get(key.as_str())
                .unwrap_or_else(|| panic!("key `{key}` is not in the en catalog"));
            (!is_named(&items, value)).then(|| format!("  {key}: \"{value}\""))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "menu entries missing from the manual (write them in bold in the right page of \
         manual/src/, or add the key to NOT_IN_MANUAL with a reason):\n{}",
        missing.join("\n")
    );
}

#[test]
fn the_bold_items_split_menu_paths() {
    let items = bold_items("Use **File ▸ Import…** then **Apply**. Not bold: Save.");
    assert!(items.contains("File") && items.contains("Import…") && items.contains("Apply"));
    assert!(!items.contains("Save"));
}

#[test]
fn a_placeholder_label_is_matched_by_its_static_part() {
    assert_eq!(fragments("Restore { $title }"), vec!["Restore".to_string()]);
}
