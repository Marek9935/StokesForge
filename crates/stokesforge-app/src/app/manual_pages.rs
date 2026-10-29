//! Two pages of the user manual generated from the code (#364), and the
//! copies of the interface icons the manual shows inline (#367).
//!
//! `manual/src/reference/shortcuts.md` comes from the in-app help table
//! [`HELP`](super::shortcuts::HELP) (private to this crate, hence a test here)
//! and `manual/src/reference/interface-texts.md` from the `en` and `fr`
//! catalogs of `stokesforge-app-state`. The ordinary test
//! `the_generated_manual_pages_are_up_to_date` rebuilds both and compares them
//! with the committed files; it fails when they differ.
//!
//! To regenerate after changing a shortcut or an interface text:
//!
//! ```sh
//! UPDATE_MANUAL_PAGES=1 cargo test -p stokesforge-app --lib manual_pages
//! ```
//!
//! then commit the pages with the change. Never edit them by hand.
//!
//! The manual shows an icon inline with
//! `<img class="icon" src="../images/icons/FILE.svg" alt="">`. The files of
//! `manual/src/images/icons/` are byte-identical copies of
//! `crates/stokesforge-app/assets/icons/FILE` (plus the `LICENSE` of the icons,
//! which the MIT license requires next to the copies). The test
//! `the_manual_icons_are_copies_of_the_application_icons` scans
//! `manual/src/**/*.md` for `images/icons/<file>` and fails when a referenced
//! icon is missing from the assets, missing or different in the manual, when a
//! copy is no longer referenced, or when `LICENSE` differs. The same command
//! (`UPDATE_MANUAL_PAGES=1`) copies and removes files to fix it.
//!
//! The catalogs are read with `fluent-syntax` rather than through `tr!`: the
//! French column needs the whole `fr` catalog by key, not the keys the code
//! knows at compile time, and reading the files keeps the global language
//! untouched for the French side. The shortcut labels (`HELP` holds
//! functions that call `tr!`) need the global language set to English, under
//! [`LANGUAGE_LOCK`].

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use eframe::egui;
use fluent_syntax::ast::{Entry, PatternElement};
use stokesforge_app_state::i18n::{set_language, Language};

use super::shortcuts::{HelpArea, HELP};

const EN: &str = include_str!("../../../stokesforge-app-state/i18n/en/stokesforge_app.ftl");
const FR: &str = include_str!("../../../stokesforge-app-state/i18n/fr/stokesforge_app.ftl");

const COMMAND: &str = "UPDATE_MANUAL_PAGES=1 cargo test -p stokesforge-app --lib manual_pages";

/// The language is global: the generator sets English under this lock so that
/// two tests of this module never overlap. (The other tests that touch the
/// language also set English only.)
static LANGUAGE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Escapes a table cell.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

fn header(title: &str) -> String {
    format!(
        "# {title}\n\n*This page is generated from the application; do not edit it by hand.*\n\n<!-- Regenerate with: {COMMAND} -->\n\n"
    )
}

/// Wraps each key of a key text in `<kbd>`: "Ctrl+Shift+Z or Ctrl+Y" gives
/// one `<kbd>` per key, joined by `+`; the words "or" and "/" stay plain.
fn kbd(keys: &str) -> String {
    keys.split_whitespace()
        .map(|token| {
            if token == "or" || token == "/" {
                token.to_string()
            } else {
                token
                    .split('+')
                    .map(|key| format!("<kbd>{}</kbd>", cell(key)))
                    .collect::<Vec<_>>()
                    .join("+")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The "Shortcuts" page.
fn shortcuts_page() -> String {
    let _lock = LANGUAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_language(Language::English);
    let mut page = header("Shortcuts");
    page.push_str(
        "On macOS, Cmd replaces Ctrl.\n\nThe same list is shown by **Help ▸ Keyboard Shortcuts…**.\n",
    );
    for area in HelpArea::ALL {
        page.push_str(&format!(
            "\n## {}\n\n| Action | Keys |\n|---|---|\n",
            area.title()
        ));
        for entry in HELP.iter().filter(|entry| entry.area == area) {
            // Ctrl spelling whatever the host: the same text on every machine.
            let keys = entry
                .keys_text_with(|shortcut| shortcut.format(&egui::ModifierNames::NAMES, false));
            page.push_str(&format!(
                "| {} | {} |\n",
                cell(&(entry.label)()),
                kbd(&keys)
            ));
        }
    }
    page
}

/// Messages of a catalog whose value is plain text: key and text. A message
/// with a placeable (variable, selector, reference) has no readable fixed
/// text and is left out.
fn plain_messages(source: &str) -> Vec<(String, String)> {
    // A Windows checkout (core.autocrlf) gives CRLF catalogs: without this,
    // the `\r` would end up in multi-line texts and differ from Linux.
    let source = source.replace("\r\n", "\n");
    let resource = match fluent_syntax::parser::parse(source.as_str()) {
        Ok(resource) => resource,
        Err((resource, _)) => resource,
    };
    let mut messages = Vec::new();
    for entry in &resource.body {
        let Entry::Message(message) = entry else {
            continue;
        };
        let Some(pattern) = &message.value else {
            continue;
        };
        let mut text = String::new();
        let mut plain = true;
        for element in &pattern.elements {
            match element {
                PatternElement::TextElement { value } => text.push_str(value),
                PatternElement::Placeable { .. } => plain = false,
            }
        }
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if plain && !text.is_empty() {
            messages.push((message.id.name.to_string(), text));
        }
    }
    messages
}

/// Titles that are not window or dialog titles: a field label, a section
/// header.
const NOT_A_WINDOW_TITLE: &[&str] = &[
    "plot-target-title",
    "plot-general-title",
    "plot-area-layer-title",
    "import-preview-title",
];

/// Whether a key names something the user looks for: a menu or an entry of a
/// menu or a context menu, a toolbar, a tab, or the title of a window or a
/// dialog.
fn is_looked_up(key: &str) -> bool {
    key.starts_with("menu-")
        || key.starts_with("action-")
        || key.starts_with("toolbar-")
        || key.starts_with("plot-tab-")
        || key.starts_with("analysis-category-")
        || key.starts_with("window-title-")
        || (key.ends_with("-title") && !NOT_A_WINDOW_TITLE.contains(&key))
}

/// The "Interface texts" page and its number of rows.
fn interface_texts_page() -> (String, usize) {
    let french: std::collections::HashMap<String, String> =
        plain_messages(FR).into_iter().collect();
    let mut rows: Vec<(String, String)> = plain_messages(EN)
        .into_iter()
        .filter(|(key, _)| is_looked_up(key))
        .map(|(key, english)| {
            let fr = french.get(&key).cloned().unwrap_or_else(|| english.clone());
            (english, fr)
        })
        .collect();
    rows.sort_by(|a, b| {
        a.0.to_lowercase()
            .cmp(&b.0.to_lowercase())
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.1.cmp(&b.1))
    });
    rows.dedup();
    let mut page = header("Interface texts");
    page.push_str(
        "You can switch the interface to French with **File ▸ Language**. This table helps you find an entry of the French interface from its English name, and the other way round. It lists:\n\n\
- the menus and their entries, including the entries of the context menus (right-click);\n\
- the titles of windows and dialogs;\n\
- the names of the tabs of the graph settings and of the analysis categories;\n\
- the names of the toolbars.\n\n\
Texts that contain a variable (a name, a number) are not listed. A trailing \"…\" means that the entry opens a window. Rows are sorted by English text; a text used in several places appears once.\n\n\
<!-- lang: fr columns: French -->\n| English | French |\n|---|---|\n",
    );
    for (english, fr) in &rows {
        page.push_str(&format!("| {} | {} |\n", cell(english), cell(fr)));
    }
    (page, rows.len())
}

fn manual_file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../manual/src/reference")
        .join(name)
}

/// Compares a generated page with its committed file; rewrites it instead
/// when `UPDATE_MANUAL_PAGES=1`. Returns an error message when it is stale.
fn check(name: &str, generated: &str) -> Result<(), String> {
    let path = manual_file(name);
    // Same line endings on every platform (CRLF on a Windows checkout).
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    if committed == generated {
        return Ok(());
    }
    if std::env::var("UPDATE_MANUAL_PAGES").as_deref() == Ok("1") {
        std::fs::write(&path, generated).map_err(|err| format!("cannot write {name}: {err}"))?;
        return Ok(());
    }
    Err(format!(
        "manual/src/reference/{name} is out of date with the application. Regenerate it with:\n    {COMMAND}\nthen commit the page."
    ))
}

#[test]
fn the_generated_manual_pages_are_up_to_date() {
    let (interface_texts, _) = interface_texts_page();
    let results = [
        check("shortcuts.md", &shortcuts_page()),
        check("interface-texts.md", &interface_texts),
    ];
    let errors: Vec<String> = results.into_iter().filter_map(Result::err).collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn the_generated_pages_cover_the_help_and_the_menus() {
    let shortcuts = shortcuts_page();
    // One row per entry of the help, one section per area.
    assert_eq!(
        shortcuts.lines().filter(|l| l.starts_with("| ")).count(),
        HELP.len() + HelpArea::ALL.len(),
        "rows (plus one header per section)"
    );
    assert!(shortcuts.contains("<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd>"));
    let (_, rows) = interface_texts_page();
    assert!(rows > 100, "{rows} rows only");
    let (page, _) = interface_texts_page();
    for expected in ["| Save As… | Enregistrer sous… |", "| Preferences | "] {
        assert!(page.contains(expected), "missing {expected}");
    }
}

// --- Icon copies (#367) ----------------------------------------------------

fn manual_src() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../manual/src")
}

fn app_icons() -> PathBuf {
    crate::icons::source_folder()
}

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            markdown_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
}

/// The icon file names that the pages of the manual point to
/// (`images/icons/<file>`).
fn referenced_icons() -> BTreeSet<String> {
    let mut pages = Vec::new();
    markdown_files(&manual_src(), &mut pages);
    let marker = "images/icons/";
    let mut names = BTreeSet::new();
    for page in pages {
        let text = std::fs::read_to_string(&page).unwrap_or_default();
        for part in text.split(marker).skip(1) {
            let name: String = part
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
                .collect();
            if !name.is_empty() {
                names.insert(name);
            }
        }
    }
    names
}

/// Makes `copy` identical to `original` (or reports that it is not), as
/// [`check`] does for a page.
fn sync_copy(original: &Path, copy: &Path, label: &str, errors: &mut Vec<String>) {
    let wanted = std::fs::read(original).ok();
    let Some(wanted) = wanted else {
        errors.push(format!(
            "a page of the manual points to images/icons/{label}, which does not exist in {}. Reference an existing icon.",
            app_icons().display()
        ));
        return;
    };
    if std::fs::read(copy).ok().as_deref() == Some(wanted.as_slice()) {
        return;
    }
    if std::env::var("UPDATE_MANUAL_PAGES").as_deref() == Ok("1") {
        let written = std::fs::create_dir_all(copy.parent().unwrap_or(Path::new(".")))
            .and_then(|()| std::fs::write(copy, &wanted));
        if let Err(err) = written {
            errors.push(format!("cannot write {label}: {err}"));
        }
        return;
    }
    errors.push(format!(
        "manual/src/images/icons/{label} is missing or differs from {}. Regenerate it with:\n    {COMMAND}\nthen commit the copy.",
        original.display()
    ));
}

fn check_icon_copies() -> Result<(), String> {
    let copies = manual_src().join("images/icons");
    let referenced = referenced_icons();
    let mut errors = Vec::new();
    for name in &referenced {
        sync_copy(
            &app_icons().join(name),
            &copies.join(name),
            name,
            &mut errors,
        );
    }
    sync_copy(
        &app_icons().join("LICENSE"),
        &copies.join("LICENSE"),
        "LICENSE",
        &mut errors,
    );
    if let Ok(entries) = std::fs::read_dir(&copies) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "LICENSE" || referenced.contains(&name) {
                continue;
            }
            if std::env::var("UPDATE_MANUAL_PAGES").as_deref() == Ok("1") {
                if let Err(err) = std::fs::remove_file(entry.path()) {
                    errors.push(format!("cannot remove {name}: {err}"));
                }
            } else {
                errors.push(format!(
                    "manual/src/images/icons/{name} is no longer used by any page of the manual. Remove it with:\n    {COMMAND}\nthen commit the removal."
                ));
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

#[test]
fn the_manual_icons_are_copies_of_the_application_icons() {
    if let Err(message) = check_icon_copies() {
        panic!("{message}");
    }
}
